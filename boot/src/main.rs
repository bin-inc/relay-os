//! relay-boot: the Relay OS UEFI loader.
//!
//! Loads \EFI\RELAY\kernel.elf, sets up the display, builds page tables,
//! exits boot services and jumps to the kernel with a `BootInfo`.
#![no_std]
#![no_main]

extern crate alloc;

mod paging;
mod video;

use alloc::vec::Vec;
use boot_info::*;
use relay_boot::{cmdline as cmdline_text, elf, memmap};
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::device_path::media::PartitionSignature;
use uefi::proto::device_path::{DevicePath, DevicePathNodeEnum};
use uefi::proto::loaded_image::LoadedImage;
use uefi::table::cfg::ConfigTableEntry;
use uefi::{CStr16, cstr16, println};
use x86_64::structures::paging::PageTableFlags as F;

const KERNEL_PATH: &CStr16 = cstr16!("\\EFI\\RELAY\\kernel.elf");
const CMDLINE_PATH: &CStr16 = cstr16!("\\EFI\\RELAY\\cmdline");
/// Spare memory-map slots for descriptors created by our own allocations.
const MMAP_SLACK: usize = 64;

fn alloc_pages(ty: MemoryType, count: usize) -> *mut u8 {
    let p = boot::allocate_pages(AllocateType::AnyPages, ty, count).expect("out of memory");
    unsafe { core::ptr::write_bytes(p.as_ptr(), 0, count * 4096) };
    p.as_ptr()
}

fn read_file(path: &CStr16) -> Option<Vec<u8>> {
    let sfs = boot::get_image_file_system(boot::image_handle()).ok()?;
    uefi::fs::FileSystem::new(sfs).read(path).ok()
}

/// GPT partition GUID of the partition this loader was started from.
fn boot_partition_guid() -> Option<[u8; 16]> {
    let image = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()).ok()?;
    let device = image.device()?;
    let path = boot::open_protocol_exclusive::<DevicePath>(device).ok()?;
    path.node_iter().find_map(|node| match node.as_enum() {
        Ok(DevicePathNodeEnum::MediaHardDrive(hd)) => match hd.partition_signature() {
            PartitionSignature::Guid(g) => Some(g.to_bytes()),
            _ => None,
        },
        _ => None,
    })
}

fn rsdp() -> u64 {
    uefi::system::with_config_table(|t| {
        t.iter()
            .find(|e| e.guid == ConfigTableEntry::ACPI2_GUID)
            .map_or(0, |e| e.address as u64)
    })
}

fn raw_desc(d: &uefi::mem::memory_map::MemoryDescriptor) -> memmap::RawDesc {
    memmap::RawDesc {
        ty: d.ty.0,
        phys_start: d.phys_start,
        pages: d.page_count,
    }
}

/// Loads PT_LOAD segments into one contiguous physical block and maps them.
/// Returns (entry, phys_start, phys_len).
fn load_kernel(file: &[u8], tables: &mut paging::Tables) -> (u64, u64, u64) {
    let k = match elf::parse(file, KERNEL_BASE) {
        Ok(k) => k,
        Err(e) => panic!("kernel.elf is not a valid kernel: {e:?}"),
    };
    let (vstart, vend) = k.span();
    let pages = ((vend - vstart) / 4096) as usize;
    let base = alloc_pages(MemoryType::custom(UEFI_KERNEL_MEMORY_TYPE), pages) as u64;
    for s in k.segments() {
        let dst = (base + (s.vaddr - vstart)) as *mut u8;
        let src = &file[s.file_offset as usize..(s.file_offset + s.file_size) as usize];
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        let mut flags = F::PRESENT;
        if s.writable {
            flags |= F::WRITABLE;
        }
        if !s.executable {
            flags |= F::NO_EXECUTE;
        }
        let first = s.vaddr & !0xFFF;
        let last = (s.vaddr + s.mem_size + 0xFFF) & !0xFFF;
        for v in (first..last).step_by(4096) {
            tables.map_4k(v, base + (v - vstart), flags);
        }
    }
    (k.entry, base, vend - vstart)
}

/// Switches to the new page tables and stack, then calls the kernel.
/// Must be identity-mapped in the new tables.
#[unsafe(naked)]
unsafe extern "sysv64" fn enter_kernel(pml4: u64, stack_top: u64, entry: u64, info: u64) -> ! {
    core::arch::naked_asm!(
        "mov cr3, rdi",
        "mov rsp, rsi",
        "mov rdi, rcx",
        "xor ebp, ebp",
        "push 0", // fake return address keeps the SysV stack alignment
        "jmp rdx",
    )
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    println!("relay-boot {}", env!("CARGO_PKG_VERSION"));

    let kernel_file = read_file(KERNEL_PATH).expect("cannot read \\EFI\\RELAY\\kernel.elf");
    let cmdline_file = read_file(CMDLINE_PATH).unwrap_or_default();
    let cmdline = cmdline_text::normalize(&cmdline_file);
    let fb = video::setup(cmdline).expect("no usable 32-bpp GOP framebuffer");
    // Boot-progress squares (see FramebufferInfo::mark_stage): the firmware
    // console stops drawing once we hold the GOP, and the NUC has no serial
    // port, so these are the only progress signal on real hardware.
    let mark = |stage| unsafe { fb.mark_stage(fb.phys_addr, stage) };
    mark(1);
    let guid = boot_partition_guid();
    let rsdp_addr = rsdp();
    mark(2);

    let mut tables = paging::Tables::new();
    let (entry, kernel_phys, kernel_len) = load_kernel(&kernel_file, &mut tables);
    mark(3);
    drop(kernel_file);

    // Linear map of RAM (from the current memory map) and the framebuffer.
    let mmap = boot::memory_map(MemoryType::LOADER_DATA).expect("memory map");
    let descs: Vec<memmap::RawDesc> = mmap.entries().map(raw_desc).collect();
    drop(mmap);
    tables.map_linear(0, memmap::linear_map_end(descs.iter().copied()));
    tables.map_linear(fb.phys_addr, fb.phys_addr + fb.size);
    mark(4);

    // Kernel stack below a guard page.
    let stack_pages = (KERNEL_STACK_SIZE / 4096) as usize;
    let stack = alloc_pages(MemoryType::LOADER_DATA, stack_pages) as u64;
    for i in 0..stack_pages as u64 {
        tables.map_4k(
            KERNEL_STACK_BOTTOM + i * 4096,
            stack + i * 4096,
            F::WRITABLE | F::NO_EXECUTE,
        );
    }

    // Identity-map the trampoline so the instruction after `mov cr3` is valid.
    let tramp = enter_kernel as *const () as u64 & !0xFFF;
    tables.map_4k(tramp, tramp, F::empty());
    tables.map_4k(tramp + 4096, tramp + 4096, F::empty());
    mark(5);

    // Everything the kernel reads must be allocated before ExitBootServices.
    // Size the region array from the memory map as it is now (after all the
    // page-table allocations), plus slack for the allocations below.
    let slots = boot::memory_map(MemoryType::LOADER_DATA)
        .expect("memory map")
        .len()
        + MMAP_SLACK;
    let info_phys = alloc_pages(MemoryType::LOADER_DATA, 1) as u64;
    let region_bytes = slots * core::mem::size_of::<MemoryRegion>();
    let regions_phys = alloc_pages(MemoryType::LOADER_DATA, region_bytes.div_ceil(4096)) as u64;
    mark(6);

    println!("relay-boot: starting kernel");
    mark(7);
    let final_map = unsafe { boot::exit_boot_services(None) };
    mark(8);
    let regions =
        unsafe { core::slice::from_raw_parts_mut(regions_phys as *mut MemoryRegion, slots) };
    // No allocation from here on: the UEFI allocator is gone.
    let count = memmap::translate(final_map.entries().map(raw_desc), regions)
        .expect("memory map grew beyond reserved slots");
    // `final_map`'s backing memory is LOADER_DATA; it is never freed, and
    // dropping it would call into boot services, which are gone.
    core::mem::forget(final_map);

    let mut cmd = [0u8; CMDLINE_MAX];
    let n = cmdline.len(); // normalize() guarantees n <= CMDLINE_MAX
    cmd[..n].copy_from_slice(cmdline.as_bytes());
    let info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        framebuffer: fb,
        memory_map_ptr: PHYS_OFFSET + regions_phys,
        memory_map_len: count as u64,
        rsdp_addr,
        phys_offset: PHYS_OFFSET,
        kernel_phys_start: kernel_phys,
        kernel_phys_len: kernel_len,
        cmdline: cmd,
        cmdline_len: n as u32,
        boot_partition_guid: guid.unwrap_or([0; 16]),
        has_boot_partition_guid: guid.is_some() as u32,
    };
    unsafe {
        core::ptr::write(info_phys as *mut BootInfo, info);
        fb.mark_stage(fb.phys_addr, 9);
        x86_64::instructions::interrupts::disable();
        x86_64::registers::model_specific::Efer::update(|f| {
            f.insert(x86_64::registers::model_specific::EferFlags::NO_EXECUTE_ENABLE)
        });
        enter_kernel(
            tables.pml4_phys,
            KERNEL_STACK_TOP,
            entry,
            PHYS_OFFSET + info_phys,
        )
    }
}
