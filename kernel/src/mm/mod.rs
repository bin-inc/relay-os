//! Memory management (spec §5.1–5.3): the frame allocator, the kernel's own
//! page tables and the heap.
//!
//! `init` replaces the loader's page tables. The new PML4 shares the
//! loader's subtrees for the kernel image and stack, rebuilds the linear
//! map from the RAM regions only (write-back) plus the framebuffer
//! (write-combining through PAT entry 1), and drops everything in the lower
//! half, including the loader's identity-mapped trampoline. Device memory is
//! mapped later, on demand, with `map_mmio`.

pub mod frame;
pub mod heap;
pub mod paging;

use boot_info::{BootInfo, HEAP_BASE, HEAP_SIZE, PHYS_MAP_MAX, PHYS_OFFSET};
use core::fmt;
use frame::{FRAME_SIZE, FrameAllocator};
use heap::{HeapStats, KernelHeap};
use paging::{Cache, MapError, PAGE, PAT_VALUE, PageTables, PhysMem};
use spin::Mutex;
use x86_64::PhysAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::registers::model_specific::Msr;
use x86_64::structures::paging::PhysFrame;

#[cfg_attr(not(test), global_allocator)]
pub static HEAP: KernelHeap = KernelHeap::new();

/// First PML4 entry after the linear map and the heap. Loader entries from
/// here up (kernel stack, kernel image) are shared with the new tables.
const FIRST_SHARED_PML4_ENTRY: usize = 385;
const IA32_PAT: u32 = 0x277;

struct Memory {
    frames: FrameAllocator<'static>,
    tables: PageTables,
}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

/// Page tables are reached through the linear map; new ones come from the
/// frame allocator.
struct LinearMem<'a>(&'a mut FrameAllocator<'static>);

impl PhysMem for LinearMem<'_> {
    fn table(&mut self, phys: u64) -> &mut [u64; 512] {
        // SAFETY: page-table frames are RAM, which the linear map covers.
        unsafe { &mut *((PHYS_OFFSET + phys) as *mut [u64; 512]) }
    }

    fn alloc_table(&mut self) -> Option<u64> {
        let p = self.0.alloc(1, 1)?;
        // SAFETY: a fresh usable frame, mapped through the linear map.
        unsafe { core::ptr::write_bytes((PHYS_OFFSET + p) as *mut u8, 0, PAGE as usize) };
        Some(p)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MemError {
    NoRoomForBitmap,
    NoRoomForHeap,
    Map(MapError),
}

impl From<MapError> for MemError {
    fn from(e: MapError) -> Self {
        MemError::Map(e)
    }
}

impl fmt::Display for MemError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MemError::NoRoomForBitmap => write!(f, "no usable region holds the frame bitmap"),
            MemError::NoRoomForHeap => write!(f, "no 32 MiB of contiguous memory for the heap"),
            MemError::Map(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MemStats {
    pub free_frames: u64,
    pub total_frames: u64,
    pub heap: HeapStats,
}

pub fn stats() -> MemStats {
    let m = MEMORY.lock();
    let m = m.as_ref().expect("mm::init has not run");
    MemStats {
        free_frames: m.frames.free_frames(),
        total_frames: m.frames.total_frames(),
        heap: HEAP.stats(),
    }
}

/// Sets up frames, page tables and the heap. Runs once, with interrupts
/// disabled.
pub fn init(info: &BootInfo) -> Result<MemStats, MemError> {
    // SAFETY: built by relay-boot, lives forever.
    let map = unsafe { info.memory_map() };
    let words = frame::bitmap_words(map);
    let bitmap_phys = frame::place_bitmap(map, words * 8).ok_or(MemError::NoRoomForBitmap)?;
    // SAFETY: usable memory, covered by the loader's linear map, and handed
    // to nobody else (the allocator marks these frames used).
    let bits =
        unsafe { core::slice::from_raw_parts_mut((PHYS_OFFSET + bitmap_phys) as *mut u64, words) };
    let mut frames = FrameAllocator::new(map, bits, bitmap_phys);

    let mut mem = LinearMem(&mut frames);
    let mut tables = PageTables::new(&mut mem)?;
    let (loader_pml4, _) = Cr3::read();
    let loader_pml4 = loader_pml4.start_address().as_u64();
    for i in FIRST_SHARED_PML4_ENTRY..512 {
        let e = mem.table(loader_pml4)[i];
        if e != 0 {
            tables.set_pml4_entry(&mut mem, i, e);
        }
    }
    for (start, end) in paging::linear_ranges(map) {
        let start = start & !(PAGE - 1);
        let end = end.next_multiple_of(PAGE);
        tables.map(
            &mut mem,
            PHYS_OFFSET + start,
            start,
            end - start,
            Cache::WriteBack,
        )?;
    }
    let fb = &info.framebuffer;
    let fb_start = fb.phys_addr & !(PAGE - 1);
    let fb_end = (fb.phys_addr + fb.size).next_multiple_of(PAGE);
    tables.map(
        &mut mem,
        PHYS_OFFSET + fb_start,
        fb_start,
        fb_end - fb_start,
        Cache::WriteCombining,
    )?;

    // SAFETY: PAT entry 1 changes from write-through to write-combining; no
    // current mapping selects it (the loader sets neither PWT nor PCD). The
    // new tables map the kernel, its stack and all RAM as before.
    unsafe {
        Msr::new(IA32_PAT).write(PAT_VALUE);
        core::arch::asm!("wbinvd", options(nostack));
        let pml4 = PhysFrame::containing_address(PhysAddr::new(tables.pml4));
        Cr3::write(pml4, Cr3Flags::empty());
    }

    let heap_phys = mem
        .0
        .alloc(
            (HEAP_SIZE / FRAME_SIZE) as usize,
            (paging::HUGE / FRAME_SIZE) as usize,
        )
        .ok_or(MemError::NoRoomForHeap)?;
    tables.map(&mut mem, HEAP_BASE, heap_phys, HEAP_SIZE, Cache::WriteBack)?;
    // SAFETY: just mapped, owned by the heap from now on.
    unsafe { HEAP.init(HEAP_BASE as usize, HEAP_SIZE as usize) };

    *MEMORY.lock() = Some(Memory { frames, tables });
    heap_self_test();
    Ok(stats())
}

/// A quick check that the heap hands out working memory: a large block (not
/// `Box::new([..])`, which may build the array on the 64 KiB kernel stack
/// first) and a small one.
fn heap_self_test() {
    let block = alloc::vec![0x5Au8; 64 * 1024];
    let mut v = alloc::vec::Vec::with_capacity(1024);
    v.extend(0..1024u32);
    assert!(block.iter().all(|&b| b == 0x5A));
    assert!(v.iter().enumerate().all(|(i, &x)| x == i as u32));
}

/// Maps device memory (or firmware tables) at `PHYS_OFFSET + phys` with the
/// given cache type and returns a pointer to `phys`. Mapping a range again
/// the same way is fine; mapping it with a different cache type is an error.
pub fn map_mmio(phys: u64, len: u64, cache: Cache) -> Result<*mut u8, MapError> {
    let start = phys & !(PAGE - 1);
    let end = match phys.checked_add(len.max(1)) {
        Some(end) if end <= PHYS_MAP_MAX => end.next_multiple_of(PAGE),
        _ => return Err(MapError::OutOfRange),
    };
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("map_mmio before mm::init");
    let mut mem = LinearMem(&mut m.frames);
    m.tables
        .map(&mut mem, PHYS_OFFSET + start, start, end - start, cache)?;
    x86_64::instructions::tlb::flush_all();
    Ok((PHYS_OFFSET + phys) as *mut u8)
}
