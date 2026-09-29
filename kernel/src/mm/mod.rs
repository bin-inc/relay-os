//! Memory management (spec §5.1–5.3): the frame allocator, the kernel's own
//! page tables and the heap.
//!
//! `init` replaces the loader's page tables. The new PML4 shares the
//! loader's subtrees for the kernel image and stack, rebuilds the linear
//! map from the RAM regions only (write-back) plus the framebuffer
//! (write-combining through PAT entry 1), and drops everything in the lower
//! half, including the loader's identity-mapped trampoline. Device memory is
//! mapped later, on demand, with `map_mmio`. Every upper-half PML4 entry
//! exists from `init` on, so programs' address spaces (`space`) share the
//! kernel's half by copying those entries once.

pub mod frame;
pub mod heap;
pub mod kstack;
pub mod paging;
pub mod space;
#[cfg(test)]
pub mod testing;
pub mod user;

use boot_info::{BootInfo, HEAP_BASE, HEAP_SIZE, PHYS_MAP_MAX, PHYS_OFFSET};
use core::fmt;
use frame::{FRAME_SIZE, FrameAllocator};
use heap::{HeapStats, KernelHeap};
use kstack::{KernelStack, KernelStacks};
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
    stacks: KernelStacks,
}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

/// Frames user memory must leave free (user-space gate §11.1): 8 MiB, so
/// DMA buffers, page tables and kernel stacks never run out.
pub const USER_RESERVE_FRAMES: u64 = (8 << 20) / FRAME_SIZE;

/// Whether one more frame may go to a program's memory while `free` frames
/// are free.
pub fn user_may_take(free: u64) -> bool {
    free > USER_RESERVE_FRAMES
}

/// Page tables are reached through the linear map; new ones come from the
/// frame allocator.
struct LinearMem<'a>(&'a mut FrameAllocator<'static>);

/// The same for a program's memory, which leaves `USER_RESERVE_FRAMES`
/// free.
pub struct UserMem<'a>(LinearMem<'a>);

impl PhysMem for UserMem<'_> {
    fn table(&mut self, phys: u64) -> &mut [u64; 512] {
        self.0.table(phys)
    }

    fn alloc_table(&mut self) -> Option<u64> {
        if !user_may_take(self.0.0.free_frames()) {
            return None;
        }
        self.0.alloc_table()
    }

    fn free_frame(&mut self, phys: u64) {
        self.0.free_frame(phys)
    }
}

/// Runs `f` with the frames for a program's memory and the kernel's page
/// tables (which a new `AddressSpace` shares).
pub fn with_user_memory<R>(f: impl FnOnce(&mut UserMem<'_>, &PageTables) -> R) -> R {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    f(&mut UserMem(LinearMem(&mut m.frames)), &m.tables)
}

/// A kernel stack for a program (user-space gate §5.4), from the frames
/// the kernel keeps for itself; `None` when every slot is in use.
pub fn alloc_kernel_stack() -> Option<KernelStack> {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    m.stacks.alloc(&mut m.tables, &mut LinearMem(&mut m.frames))
}

/// Gives back a kernel stack nothing runs on any more.
pub fn free_kernel_stack(stack: KernelStack) {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    let pages: alloc::vec::Vec<u64> = stack.pages().collect();
    m.stacks
        .free(stack, &mut m.tables, &mut LinearMem(&mut m.frames));
    for virt in pages {
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
    }
}

/// Heap the kernel keeps for everything else while it reads a program.
const HEAP_MARGIN: usize = 1 << 20;

/// The most bytes one allocation may take now without the heap running out
/// (which panics): the largest free block, less a margin for the rest.
pub fn heap_room() -> usize {
    HEAP.stats().largest_free.saturating_sub(HEAP_MARGIN)
}

/// The kernel's own page tables, for CR3 when no program runs.
pub fn kernel_pml4() -> u64 {
    MEMORY
        .lock()
        .as_ref()
        .expect("mm::init has not run")
        .tables
        .pml4
}

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

    fn free_frame(&mut self, phys: u64) {
        self.0.free(phys, 1);
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
    tables.fill_upper_half(&mut mem)?;
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

    *MEMORY.lock() = Some(Memory {
        frames,
        tables,
        stacks: KernelStacks::new(),
    });
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

/// Frames and frame alignment for a DMA buffer of `size` bytes aligned to
/// `align` bytes: whole frames, and at least frame-aligned.
pub fn dma_frames(size: usize, align: usize) -> (usize, usize) {
    let frame = FRAME_SIZE as usize;
    (size.max(1).div_ceil(frame), align.div_ceil(frame).max(1))
}

/// Zeroed, physically contiguous memory for device DMA (the USB stack's
/// rings, contexts and buffers): whole frames, aligned to `align` bytes or
/// a frame. Frames are RAM, so the linear map covers them, write-back
/// (x86 DMA is cache-coherent). Returns the virtual and physical address.
pub fn alloc_dma(size: usize, align: usize) -> Option<(core::ptr::NonNull<u8>, u64)> {
    let (count, align) = dma_frames(size, align);
    if !align.is_power_of_two() {
        return None;
    }
    let phys = MEMORY.lock().as_mut()?.frames.alloc(count, align)?;
    let virt = (PHYS_OFFSET + phys) as *mut u8;
    // SAFETY: fresh frames, mapped through the linear map, owned by the
    // caller from now on.
    unsafe { core::ptr::write_bytes(virt, 0, count * FRAME_SIZE as usize) };
    Some((core::ptr::NonNull::new(virt)?, phys))
}

/// Gives back memory from `alloc_dma` of the same `size`.
pub fn free_dma(phys: u64, size: usize) {
    let (count, _) = dma_frames(size, 1);
    if let Some(m) = MEMORY.lock().as_mut() {
        m.frames.free(phys, count);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_memory_leaves_8_mib_free() {
        assert_eq!(USER_RESERVE_FRAMES, 2048);
        assert!(user_may_take(2049), "the frame taken leaves 2048");
        assert!(!user_may_take(2048));
        assert!(!user_may_take(0));
    }

    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
        assert_eq!(dma_frames(1, 64), (1, 1));
        assert_eq!(dma_frames(4096, 4096), (1, 1));
        assert_eq!(dma_frames(4097, 16), (2, 1));
        assert_eq!(dma_frames(0, 0), (1, 1));
        assert_eq!(dma_frames(8192, 65536), (2, 16));
    }
}
