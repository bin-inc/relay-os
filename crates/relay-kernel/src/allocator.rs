use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
};

const HEAP_SIZE: usize = 64 * 1024;

struct HeapState {
    start: usize,
    end: usize,
    next: usize,
}

pub struct BumpAllocator {
    state: UnsafeCell<HeapState>,
}

// Kernel initialization is single-core and interrupts remain disabled while this allocator is used.
unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(HeapState {
                start: 0,
                end: 0,
                next: 0,
            }),
        }
    }

    /// # Safety
    /// This must run once before allocation, while no allocation is active. The static heap is a
    /// permanently reserved kernel image range and therefore remains writable for kernel lifetime.
    pub unsafe fn initialize(&self) {
        // SAFETY: single-core entry owns both static cells before any allocation is exposed.
        let bytes = unsafe { &mut *HEAP.0.get() };
        let state = unsafe { &mut *self.state.get() };
        state.start = bytes.0.as_mut_ptr() as usize;
        state.end = state.start + HEAP_SIZE;
        state.next = state.start;
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let state = unsafe { &mut *self.state.get() };
        let aligned = match state
            .next
            .checked_add(layout.align() - 1)
            .map(|address| address & !(layout.align() - 1))
        {
            Some(address) => address,
            None => return ptr::null_mut(),
        };
        let end = match aligned.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(),
        };
        if state.start == 0 || end > state.end {
            return ptr::null_mut();
        }
        state.next = end;
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
        // The bounded bootstrap heap is monotonic for the kernel lifetime.
    }
}

#[repr(align(16))]
struct HeapBytes([u8; HEAP_SIZE]);

struct HeapStorage(UnsafeCell<HeapBytes>);

// The allocator is the only code that accesses this storage after initialization.
unsafe impl Sync for HeapStorage {}

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new(HeapBytes([0; HEAP_SIZE])));
