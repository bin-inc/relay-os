//! The kernel heap (spec §5.2): a `heap::Heap` behind a spin lock,
//! registered as `#[global_allocator]` in `mm`. Interrupt handlers must
//! never allocate (the lock is not interrupt-safe).

use ::heap::Heap;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use spin::Mutex;

pub use ::heap::HeapStats;

/// The global allocator: a `Heap` behind a spin lock. Empty until `init`.
pub struct KernelHeap(Mutex<Heap>);

impl KernelHeap {
    pub const fn new() -> KernelHeap {
        KernelHeap(Mutex::new(Heap::empty()))
    }

    /// # Safety
    /// As for `Heap::new`; call once.
    pub unsafe fn init(&self, start: usize, size: usize) {
        *self.0.lock() = unsafe { Heap::new(start, size) };
    }

    pub fn stats(&self) -> HeapStats {
        self.0.lock().stats()
    }
}

impl Default for KernelHeap {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = self.0.lock().alloc(layout);
        match block {
            Some(p) => p.as_ptr(),
            None => {
                let s = self.stats();
                panic!(
                    "kernel heap exhausted: {} bytes (align {}) requested, {} of {} KiB in use, largest free block {} bytes",
                    layout.size(),
                    layout.align(),
                    s.used / 1024,
                    s.total / 1024,
                    s.largest_free
                )
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(p) = NonNull::new(ptr) {
            unsafe { self.0.lock().dealloc(p, layout) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    #[should_panic(expected = "kernel heap exhausted: 100 bytes (align 8) requested")]
    fn global_allocator_panics_with_a_clear_message() {
        let k = KernelHeap::new();
        unsafe { k.alloc(l(100, 8)) };
    }

    #[test]
    fn global_allocator_serves_from_its_heap() {
        let k = KernelHeap::new();
        let layout = Layout::from_size_align(1 << 20, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        unsafe { k.init(mem, 1 << 20) };
        let p = unsafe { k.alloc(l(4000, 16)) };
        assert!((mem..mem + (1 << 20)).contains(&(p as usize)));
        assert_eq!(k.stats().used, 4000);
        unsafe { k.dealloc(p, l(4000, 16)) };
        assert_eq!(k.stats().used, 0);
    }
}
