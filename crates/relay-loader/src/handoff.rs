use core::{arch::naked_asm, mem};

use relay_abi::{BootInfo, FramebufferInfo};
use uefi::{
    boot,
    mem::memory_map::{MemoryMap as UefiMemoryMap, MemoryType},
    proto::console::gop::{GraphicsOutput, PixelFormat},
    proto::loaded_image::LoadedImage,
    system,
    table::cfg::ConfigTableEntry,
};

use crate::{
    files,
    memory::{
        self, BootData, PHYSICAL_MEMORY_OFFSET, allocate_boot_data, allocate_zeroed, load_kernel,
    },
    paging::PageTables,
    parse_config, parse_load_plan,
};

const STACK_PAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandoffError {
    Files,
    Config,
    Elf,
    Memory,
    Paging,
    Graphics,
    Acpi,
}

pub fn boot() -> Result<(), HandoffError> {
    let config = files::read_config().map_err(|_| HandoffError::Files)?;
    let root_guid = parse_config(&config)
        .map_err(|_| HandoffError::Config)?
        .root_guid;
    drop(config);

    let image = files::read_kernel().map_err(|_| HandoffError::Files)?;
    let plan = parse_load_plan(&image).map_err(|_| HandoffError::Elf)?;
    let kernel = load_kernel(&plan, &image).map_err(|_| HandoffError::Memory)?;
    drop(plan);
    drop(image);

    let framebuffer = capture_framebuffer()?;
    let acpi_rsdp_phys = capture_rsdp()?;
    let stack = allocate_zeroed(STACK_PAGES).map_err(|_| HandoffError::Memory)?;
    let boot_data = allocate_boot_data().map_err(|_| HandoffError::Memory)?;
    let mut tables = PageTables::new().map_err(|_| HandoffError::Paging)?;
    tables
        .map_identity_first_4g()
        .map_err(|_| HandoffError::Paging)?;
    let (loader_image_start, loader_image_len) = loaded_image_range()?;
    tables
        .map_identity_execution_range(loader_image_start, loader_image_len)
        .map_err(|_| HandoffError::Paging)?;
    tables
        .map_identity_allocation(stack)
        .map_err(|_| HandoffError::Paging)?;
    tables
        .map_identity_allocation(boot_data)
        .map_err(|_| HandoffError::Paging)?;

    let usable_map = boot::memory_map(MemoryType::LOADER_DATA).map_err(|_| HandoffError::Memory)?;
    for descriptor in usable_map.entries() {
        if descriptor.ty == MemoryType::CONVENTIONAL {
            let end = descriptor
                .phys_start
                .checked_add(
                    descriptor
                        .page_count
                        .checked_mul(memory::PAGE_SIZE)
                        .ok_or(HandoffError::Memory)?,
                )
                .ok_or(HandoffError::Memory)?;
            tables
                .map_usable_ram(descriptor.phys_start, end)
                .map_err(|_| HandoffError::Paging)?;
        }
    }
    drop(usable_map);

    for segment in &kernel.segments {
        tables
            .map_kernel_segment(
                segment.page_virtual_start,
                segment.physical_start,
                segment.pages,
                segment.flags,
            )
            .map_err(|_| HandoffError::Paging)?;
    }
    tables
        .map_framebuffer(framebuffer.physical_base, framebuffer.byte_len)
        .map_err(|_| HandoffError::Paging)?;
    let stack_top = stack.end().map_err(|_| HandoffError::Memory)?;
    let cr3 = tables.root_physical_address();
    let boot_info = boot_data.physical_start as *mut BootData;

    // All UEFI protocol handles and heap-backed values are released before this call.
    // The final map is the only firmware data retained past this point.
    let final_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };
    unsafe {
        if memory::normalize_memory_map(boot_info, &final_map).is_err() {
            halt();
        }
        (*boot_info).info.magic = relay_abi::BOOT_INFO_MAGIC;
        (*boot_info).info.abi_version = relay_abi::BOOT_ABI_VERSION;
        (*boot_info).info.struct_size = core::mem::size_of::<BootInfo>() as u32;
        (*boot_info).info.framebuffer = framebuffer;
        (*boot_info).info.root_partition_guid = root_guid;
        (*boot_info).info.physical_memory_offset = PHYSICAL_MEMORY_OFFSET;
        (*boot_info).info.acpi_rsdp_phys = acpi_rsdp_phys;
    }

    // No allocation or UEFI access is possible after exit. Page tables remain loader-owned
    // memory and are intentionally leaked together with the final handoff data.
    mem::forget(tables);
    mem::forget(final_map);
    unsafe { jump_to_kernel(boot_info.cast::<BootInfo>(), cr3, stack_top, kernel.entry) }
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

fn capture_framebuffer() -> Result<FramebufferInfo, HandoffError> {
    let handle =
        boot::get_handle_for_protocol::<GraphicsOutput>().map_err(|_| HandoffError::Graphics)?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle)
        .map_err(|_| HandoffError::Graphics)?;
    let info = gop.current_mode_info();
    let (width, height) = info.resolution();
    let stride = info.stride();
    let pixel_format = match info.pixel_format() {
        PixelFormat::Rgb => 0,
        PixelFormat::Bgr => 1,
        PixelFormat::Bitmask => 2,
        PixelFormat::BltOnly => return Err(HandoffError::Graphics),
    };
    let mut buffer = gop.frame_buffer();
    let byte_len = u64::try_from(buffer.size()).map_err(|_| HandoffError::Graphics)?;
    Ok(FramebufferInfo {
        physical_base: buffer.as_mut_ptr() as u64,
        byte_len,
        width: u32::try_from(width).map_err(|_| HandoffError::Graphics)?,
        height: u32::try_from(height).map_err(|_| HandoffError::Graphics)?,
        stride_pixels: u32::try_from(stride).map_err(|_| HandoffError::Graphics)?,
        bytes_per_pixel: 4,
        pixel_format,
        red_mask: 0,
        green_mask: 0,
        blue_mask: 0,
        reserved_mask: 0,
    })
}

fn capture_rsdp() -> Result<u64, HandoffError> {
    system::with_config_table(|entries| {
        let address = entries
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.guid == ConfigTableEntry::ACPI_GUID)
            })
            .map(|entry| entry.address as u64)
            .ok_or(HandoffError::Acpi)?;
        (address != 0).then_some(address).ok_or(HandoffError::Acpi)
    })
}

fn loaded_image_range() -> Result<(u64, u64), HandoffError> {
    let image = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle())
        .map_err(|_| HandoffError::Paging)?;
    let (base, len) = image.info();
    let start = base as u64;
    if start == 0 || len == 0 || start.checked_add(len).is_none() {
        return Err(HandoffError::Paging);
    }
    Ok((start, len))
}

/// # Safety
/// `boot_info` is identity-mapped and valid, `cr3` names a complete page-table root,
/// `stack_top` is a writable identity-mapped stack, and `entry` is an executable mapping.
#[unsafe(naked)]
unsafe extern "sysv64" fn jump_to_kernel(
    _boot_info: *const BootInfo,
    _cr3: u64,
    _stack_top: u64,
    _entry: u64,
) -> ! {
    naked_asm!(
        "cli",
        "mov cr3, rsi",
        "mov rsp, rdx",
        "and rsp, -16",
        "sub rsp, 8",
        "cld",
        "jmp rcx",
    )
}
