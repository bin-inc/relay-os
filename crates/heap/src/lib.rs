//! A heap allocator over one region of memory (spec §5.2 of milestone 1).
//! The kernel's `#[global_allocator]` is one of these behind a lock
//! (`kernel/src/mm/heap.rs`); user programs will get one too.
//!
//! - **Small blocks** (size and alignment at most 2 KiB) come from size
//!   classes 16 B to 2 KiB. Each class has a free list of blocks carved out
//!   of 4 KiB slabs; slabs are taken from the large-block list and never
//!   returned.
//! - **Larger blocks** come from an address-ordered first-fit free list.
//!   Freed blocks are merged with free neighbours.
//!
//! No block header is needed: `dealloc` receives the same `Layout` as
//! `alloc`, which gives the class or the rounded size again.
#![cfg_attr(not(test), no_std)]

use core::alloc::Layout;
use core::ptr::NonNull;

pub const SIZE_CLASSES: [usize; 8] = [16, 32, 64, 128, 256, 512, 1024, 2048];
const SLAB: usize = 4096;
const MIN_BLOCK: usize = 16;

/// A free large block, stored in the free memory itself.
struct FreeBlock {
    size: usize,
    next: Link<FreeBlock>,
}

/// A free small block: only the link to the next one of its class.
struct SmallBlock {
    next: Link<SmallBlock>,
}

/// A free-list link; `None` ends the list. Every block on a list is free
/// heap memory, so a `Some` may be read and written.
type Link<T> = Option<NonNull<T>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapStats {
    pub total: usize,
    /// Bytes handed out, counted at their class or rounded size.
    pub used: usize,
    /// Bytes on the large-block free list.
    pub free_large: usize,
    pub largest_free: usize,
}

pub struct Heap {
    start: usize,
    end: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
    large: Link<FreeBlock>,
    used: usize,
}

// SAFETY: the links point into the heap region, which the Heap owns.
unsafe impl Send for Heap {}

fn class_of(layout: Layout) -> Option<usize> {
    let size = layout.size().max(layout.align());
    SIZE_CLASSES.iter().position(|&c| c >= size)
}

fn large_size(layout: Layout) -> usize {
    layout.size().max(MIN_BLOCK).next_multiple_of(MIN_BLOCK)
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap {
            start: 0,
            end: 0,
            classes: [None; SIZE_CLASSES.len()],
            large: None,
            used: 0,
        }
    }

    /// # Safety
    /// [start, start + size) must be writable memory owned by this heap for
    /// its whole life. `start` must be 16-byte aligned.
    pub unsafe fn new(start: usize, size: usize) -> Heap {
        assert!(start.is_multiple_of(MIN_BLOCK));
        let size = size & !(MIN_BLOCK - 1);
        let mut h = Heap::empty();
        h.start = start;
        h.end = start + size;
        unsafe { h.insert_free(start, size) };
        h
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let addr = match class_of(layout) {
            Some(c) => self.alloc_small(c)?,
            None => {
                let size = large_size(layout);
                let a = self.alloc_large(size, layout.align().max(MIN_BLOCK))?;
                self.used += size;
                a
            }
        };
        NonNull::new(addr as *mut u8)
    }

    /// # Safety
    /// `ptr` must come from `alloc` on this heap with the same `layout`.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let addr = ptr.as_ptr() as usize;
        debug_assert!(self.start <= addr && addr < self.end, "foreign pointer");
        match class_of(layout) {
            Some(c) => {
                let block = ptr.cast::<SmallBlock>();
                unsafe {
                    block.write(SmallBlock {
                        next: self.classes[c],
                    })
                };
                self.classes[c] = Some(block);
                self.used -= SIZE_CLASSES[c];
            }
            None => {
                let size = large_size(layout);
                unsafe { self.insert_free(addr, size) };
                self.used -= size;
            }
        }
    }

    pub fn stats(&self) -> HeapStats {
        let (mut free_large, mut largest_free) = (0, 0);
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size, next } = unsafe { b.read() };
            free_large += size;
            largest_free = largest_free.max(size);
            link = next;
        }
        HeapStats {
            total: self.end - self.start,
            used: self.used,
            free_large,
            largest_free,
        }
    }

    fn alloc_small(&mut self, c: usize) -> Option<usize> {
        let block = match self.classes[c] {
            Some(block) => block,
            None => self.refill(c)?,
        };
        self.classes[c] = unsafe { block.read() }.next;
        self.used += SIZE_CLASSES[c];
        Some(block.as_ptr() as usize)
    }

    /// Carves a new slab into blocks of class `c` and returns the first.
    fn refill(&mut self, c: usize) -> Option<NonNull<SmallBlock>> {
        let slab = self.alloc_large(SLAB, SLAB)?;
        let size = SIZE_CLASSES[c];
        // Push the blocks in reverse so they are handed out in address
        // order.
        for addr in (slab..slab + SLAB).step_by(size).rev() {
            let block = block_at::<SmallBlock>(addr);
            unsafe {
                block.write(SmallBlock {
                    next: self.classes[c],
                })
            };
            self.classes[c] = Some(block);
        }
        self.classes[c]
    }

    /// First fit: the lowest free block that holds `size` bytes at an
    /// `align`-aligned address. What is left before and after goes back on
    /// the free list.
    fn alloc_large(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: Link<FreeBlock> = None;
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size: bsize, next } = unsafe { b.read() };
            let start = b.as_ptr() as usize;
            let aligned = start.next_multiple_of(align);
            if aligned + size <= start + bsize {
                // Unlink, then return the unused head and tail.
                match prev {
                    None => self.large = next,
                    Some(mut p) => unsafe { p.as_mut().next = next },
                }
                unsafe {
                    if aligned > start {
                        self.insert_free(start, aligned - start);
                    }
                    let tail = start + bsize - (aligned + size);
                    if tail > 0 {
                        self.insert_free(aligned + size, tail);
                    }
                }
                return Some(aligned);
            }
            prev = link;
            link = next;
        }
        None
    }

    /// Inserts [addr, addr + size) in address order, merging it with the
    /// blocks directly before and after it.
    ///
    /// # Safety
    /// The range must be free heap memory, 16-byte aligned, at least 16
    /// bytes long.
    unsafe fn insert_free(&mut self, addr: usize, size: usize) {
        let mut prev: Link<FreeBlock> = None;
        let mut next = self.large;
        while let Some(n) = next.filter(|n| (n.as_ptr() as usize) < addr) {
            prev = next;
            next = unsafe { n.read() }.next;
        }
        let mut block = block_at::<FreeBlock>(addr);
        unsafe { block.write(FreeBlock { size, next }) };
        let b = unsafe { block.as_mut() };
        if let Some(n) = next.filter(|n| addr + size == n.as_ptr() as usize) {
            let n = unsafe { n.read() };
            b.size += n.size;
            b.next = n.next;
        }
        match prev {
            None => self.large = Some(block),
            Some(mut p) => {
                let p = unsafe { p.as_mut() };
                if p as *mut FreeBlock as usize + p.size == addr {
                    p.size += b.size;
                    p.next = b.next;
                } else {
                    p.next = Some(block);
                }
            }
        }
    }
}

/// The block at `addr`, a heap address (never 0: no heap starts at page 0,
/// which stays unmapped in the kernel and in user programs).
fn block_at<T>(addr: usize) -> NonNull<T> {
    NonNull::new(addr as *mut T).expect("heap block at address 0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heap over a fresh 4 KiB-aligned host allocation.
    fn heap(size: usize) -> Heap {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        unsafe { Heap::new(mem, size) }
    }

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn small_blocks_are_aligned_distinct_and_reused() {
        let mut h = heap(64 * 1024);
        let a = h.alloc(l(24, 8)).unwrap();
        let b = h.alloc(l(24, 8)).unwrap();
        assert_eq!(b.as_ptr() as usize - a.as_ptr() as usize, 32, "class 32");
        let c = h.alloc(l(1, 1)).unwrap();
        let d = h.alloc(l(100, 128)).unwrap();
        assert_eq!(d.as_ptr() as usize % 128, 0);
        assert_eq!(h.stats().used, 32 + 32 + 16 + 128);
        unsafe { h.dealloc(a, l(24, 8)) };
        assert_eq!(h.alloc(l(20, 4)), Some(a), "freed block is reused");
        unsafe {
            h.dealloc(c, l(1, 1));
            h.dealloc(d, l(100, 128));
        }
    }

    #[test]
    fn large_blocks_merge_when_freed() {
        let mut h = heap(1 << 20);
        let total = h.stats().free_large;
        let a = h.alloc(l(10_000, 16)).unwrap();
        let b = h.alloc(l(20_000, 16)).unwrap();
        let c = h.alloc(l(30_000, 16)).unwrap();
        unsafe {
            h.dealloc(b, l(20_000, 16));
            h.dealloc(a, l(10_000, 16));
        }
        // a and b merged: 30_000 bytes fit where they were.
        assert_eq!(h.alloc(l(30_000, 16)), Some(a));
        unsafe {
            h.dealloc(a, l(30_000, 16));
            h.dealloc(c, l(30_000, 16));
        }
        let s = h.stats();
        assert_eq!(s.used, 0);
        assert_eq!(s.free_large, total);
        assert_eq!(s.largest_free, total, "everything merged into one block");
    }

    #[test]
    fn large_alignment_is_honoured() {
        let mut h = heap(1 << 20);
        let _small = h.alloc(l(5000, 16)).unwrap();
        let p = h.alloc(l(64 * 1024, 4096)).unwrap();
        assert_eq!(p.as_ptr() as usize % 4096, 0);
        let q = h.alloc(l(16, 8192)).unwrap(); // tiny but over-aligned
        assert_eq!(q.as_ptr() as usize % 8192, 0);
        unsafe { h.dealloc(q, l(16, 8192)) };
    }

    #[test]
    fn running_out_returns_none() {
        let mut h = heap(64 * 1024);
        assert!(h.alloc(l(65 * 1024, 16)).is_none());
        let a = h.alloc(l(60 * 1024, 16)).unwrap();
        assert!(h.alloc(l(8 * 1024, 16)).is_none());
        unsafe { h.dealloc(a, l(60 * 1024, 16)) };
        assert!(h.alloc(l(8 * 1024, 16)).is_some());
    }

    /// Seeded random alloc/free mix. Every block is filled with a tag byte
    /// and checked before it is freed, so any overlap between live blocks
    /// shows up.
    #[test]
    fn random_mix_never_overlaps() {
        let mut h = heap(4 << 20);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut live: Vec<(NonNull<u8>, Layout, u8)> = Vec::new();
        for i in 0..20_000u32 {
            if live.len() > 300 || (!live.is_empty() && rnd() % 3 == 0) {
                let (p, layout, tag) = live.swap_remove(rnd() as usize % live.len());
                let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), layout.size()) };
                assert!(bytes.iter().all(|&b| b == tag), "block overwritten");
                unsafe { h.dealloc(p, layout) };
            } else {
                let size = match rnd() % 4 {
                    0 => 1 + rnd() as usize % 64,
                    1 => 1 + rnd() as usize % 2048,
                    _ => 1 + rnd() as usize % 20_000,
                };
                let align = 1 << (rnd() % 13);
                let layout = l(size, align);
                let p = h.alloc(layout).expect("4 MiB is plenty for 300 blocks");
                assert_eq!(p.as_ptr() as usize % align, 0);
                let tag = i as u8;
                unsafe { std::ptr::write_bytes(p.as_ptr(), tag, size) };
                live.push((p, layout, tag));
            }
        }
        for (p, layout, _) in live.drain(..) {
            unsafe { h.dealloc(p, layout) };
        }
        assert_eq!(h.stats().used, 0);
    }
}
