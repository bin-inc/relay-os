//! The program's heap (spec §8.1): `crates/heap` over memory from
//! `mem_map`, growing a mebibyte or more at a time. Memory that meets the
//! heap's last region merges with it, as `mem_map` fills its area from the
//! start. When the heap cannot grow, the program says `<name>: out of
//! memory` and exits with 134, as a Rust program aborting on Linux does.
//! Nothing is given back to the kernel before the program ends.
// Off Relay OS only the tests use it.
#![cfg_attr(not(target_os = "none"), allow(dead_code))]

use core::alloc::Layout;
use core::ptr::NonNull;
use heap::Heap;

/// The heap grows by multiples of this.
pub const STEP: usize = 1 << 20;

/// What to map so the heap can serve `layout`: its size with room for its
/// alignment and for a new slab of small blocks, in whole steps (`None`
/// cannot happen for a valid layout, whose size is below 2^63).
pub fn grow_size(layout: Layout) -> Option<usize> {
    layout
        .size()
        .checked_add(layout.align())?
        .checked_add(4096)?
        .checked_next_multiple_of(STEP)
}

/// A block of `layout` from `heap`, which grows by what `map` gives (a
/// length in, an address out) when it has no room. `None` if `map` has
/// nothing.
pub fn alloc_growing(
    heap: &mut Heap,
    layout: Layout,
    map: impl FnOnce(usize) -> Option<usize>,
) -> Option<NonNull<u8>> {
    if let Some(p) = heap.alloc(layout) {
        return Some(p);
    }
    let size = grow_size(layout)?;
    let at = map(size)?;
    // SAFETY: `map` gave `size` bytes of fresh memory that nothing else
    // uses, page-aligned.
    unsafe { heap.add(at, size) };
    heap.alloc(layout)
}

/// The `#[global_allocator]` of every program.
#[cfg(target_os = "none")]
mod on_relay {
    use super::alloc_growing;
    use crate::sys;
    use core::alloc::{GlobalAlloc, Layout};
    use core::ptr::NonNull;
    use heap::Heap;
    use spin::Mutex;

    struct Allocator(Mutex<Heap>);

    // SAFETY: blocks come from the heap, which hands each out once.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let got = alloc_growing(&mut self.0.lock(), layout, |len| sys::mem_map(len).ok());
            match got {
                Some(p) => p.as_ptr(),
                None => out_of_memory(),
            }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if let Some(p) = NonNull::new(ptr) {
                // SAFETY: `ptr` came from `alloc` with this layout.
                unsafe { self.0.lock().dealloc(p, layout) };
            }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator(Mutex::new(Heap::empty()));

    /// `<name>: out of memory`, and the end (spec §8.1).
    fn out_of_memory() -> ! {
        let _ = sys::write_all(2, crate::name());
        let _ = sys::write_all(2, b": out of memory\n");
        sys::exit(134)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `len` bytes of host memory, page-aligned, as `mem_map` would give.
    fn region(len: usize) -> usize {
        let layout = Layout::from_size_align(len, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        mem
    }

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn growth_comes_in_whole_steps_with_room_for_the_block() {
        assert_eq!(grow_size(l(1, 1)), Some(STEP));
        assert_eq!(grow_size(l(STEP, 16)), Some(2 * STEP));
        assert_eq!(grow_size(l(STEP - 4096 - 4096, 4096)), Some(STEP));
        assert_eq!(grow_size(l(STEP - 4096 - 4095, 4096)), Some(2 * STEP));
        // The biggest a layout can be still has a size to map (which
        // `mem_map` refuses).
        let most = l(isize::MAX as usize - 15, 16);
        assert_eq!(grow_size(most), Some((1 << 63) + STEP));
    }

    #[test]
    fn an_empty_heap_grows_by_a_step_and_serves_from_it() {
        let mut heap = Heap::empty();
        let mut asked = Vec::new();
        let a = alloc_growing(&mut heap, l(100, 8), |len| {
            asked.push(len);
            Some(region(len))
        })
        .unwrap();
        assert_eq!(asked, [STEP]);
        // The next ones come from the same step without mapping.
        for _ in 0..100 {
            alloc_growing(&mut heap, l(1000, 8), |_| panic!("no need to grow")).unwrap();
        }
        assert_eq!(heap.stats().total, STEP);
        unsafe { heap.dealloc(a, l(100, 8)) };
    }

    #[test]
    fn a_big_block_gets_a_big_enough_step() {
        let mut heap = Heap::empty();
        let size = 3 * STEP + 5;
        let p = alloc_growing(&mut heap, l(size, 4096), |len| {
            assert_eq!(len, 4 * STEP);
            Some(region(len))
        })
        .unwrap();
        assert_eq!(p.as_ptr() as usize % 4096, 0);
        unsafe { core::ptr::write_bytes(p.as_ptr(), 1, size) };
    }

    #[test]
    fn steps_that_meet_merge_into_one_region() {
        // `mem_map` gives the area in order: each step after the last.
        let area = region(4 * STEP);
        let mut next = area;
        let mut map = |len: usize| {
            let at = next;
            next += len;
            Some(at)
        };
        let mut heap = Heap::empty();
        let a = alloc_growing(&mut heap, l(STEP - 8192, 16), &mut map).unwrap();
        // More than is left of the first step: a second step, which meets
        // it, so the free end of the first is used too.
        let b = alloc_growing(&mut heap, l(STEP / 2 + 8192, 16), &mut map).unwrap();
        assert_eq!(b.as_ptr() as usize, a.as_ptr() as usize + STEP - 8192);
        assert_eq!(heap.stats().total, 2 * STEP);
    }

    #[test]
    fn nothing_to_map_is_nothing_allocated() {
        let mut heap = Heap::empty();
        assert_eq!(alloc_growing(&mut heap, l(16, 8), |_| None), None);
        assert_eq!(heap.stats().total, 0);
    }
}
