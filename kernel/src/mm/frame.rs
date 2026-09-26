//! Physical frame allocator (spec §5.1): one bit per 4 KiB frame, covering
//! physical memory up to the end of the highest `Usable` region.
//!
//! Only `Usable` frames are ever handed out. `Bootloader` memory (which holds
//! the kernel image, its stack, the loader's page tables and `BootInfo`),
//! ACPI memory and everything else stay reserved for the whole milestone.

use boot_info::{MemoryKind, MemoryRegion};

pub const FRAME_SIZE: u64 = 4096;
/// Frames below 1 MiB are never handed out: frame 0 would look like a null
/// pointer, and low memory is kept for real-mode code a later milestone may
/// need (an application-processor start-up trampoline).
pub const LOW_LIMIT: u64 = 0x10_0000;

fn is_usable(r: &&MemoryRegion) -> bool {
    r.kind == MemoryKind::Usable
}

const fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

/// Number of `u64` words the bitmap needs for `map`.
pub fn bitmap_words(map: &[MemoryRegion]) -> usize {
    let end = map
        .iter()
        .filter(is_usable)
        .map(|r| r.end())
        .max()
        .unwrap_or(0);
    (end / FRAME_SIZE).div_ceil(64) as usize
}

/// Where the bitmap goes: the first frame-aligned spot at or above
/// `LOW_LIMIT` in the first `Usable` region that can hold `bytes`.
pub fn place_bitmap(map: &[MemoryRegion], bytes: usize) -> Option<u64> {
    let size = (bytes as u64).div_ceil(FRAME_SIZE) * FRAME_SIZE;
    map.iter().filter(is_usable).find_map(|r| {
        let start = r.start.max(LOW_LIMIT).next_multiple_of(FRAME_SIZE);
        (start + size <= r.end()).then_some(start)
    })
}

pub struct FrameAllocator<'a> {
    /// Bit set: the frame is in use or not allocatable RAM.
    bits: &'a mut [u64],
    free: u64,
    total: u64,
    /// Where the next single-frame search starts.
    next: usize,
}

impl<'a> FrameAllocator<'a> {
    /// Builds the allocator for `map`. `bits` must hold at least
    /// `bitmap_words(map)` words and lives at physical `bitmap_phys` (from
    /// `place_bitmap`); those frames are marked used.
    pub fn new(map: &[MemoryRegion], bits: &'a mut [u64], bitmap_phys: u64) -> Self {
        bits.fill(!0);
        let mut a = FrameAllocator {
            bits,
            free: 0,
            total: 0,
            next: 0,
        };
        for r in map.iter().filter(is_usable) {
            let first = r.start.div_ceil(FRAME_SIZE) as usize;
            let last = (r.end() / FRAME_SIZE) as usize;
            for i in first..last.min(a.bits.len() * 64) {
                a.set(i, false);
                a.total += 1;
            }
        }
        a.reserve(0, LOW_LIMIT);
        let words = a.bits.len() as u64 * 8;
        a.reserve(bitmap_phys, bitmap_phys + words);
        a.free = (0..a.bits.len() * 64).filter(|&i| !a.used(i)).count() as u64;
        a
    }

    fn used(&self, i: usize) -> bool {
        self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    fn set(&mut self, i: usize, used: bool) {
        if used {
            self.bits[i / 64] |= 1 << (i % 64);
        } else {
            self.bits[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Marks every frame touching [start, end) as used.
    fn reserve(&mut self, start: u64, end: u64) {
        let first = (start / FRAME_SIZE) as usize;
        let last = (end.div_ceil(FRAME_SIZE) as usize).min(self.bits.len() * 64);
        for i in first..last {
            self.set(i, true);
        }
    }

    /// Frames that can still be allocated.
    pub fn free_frames(&self) -> u64 {
        self.free
    }

    /// `Usable` frames in the memory map, allocated or not.
    pub fn total_frames(&self) -> u64 {
        self.total
    }

    /// Allocates `count` physically contiguous frames whose first frame
    /// number is a multiple of `align` (a power of two, in frames). Returns
    /// the physical address of the first frame. The frames are not zeroed.
    pub fn alloc(&mut self, count: usize, align: usize) -> Option<u64> {
        assert!(count > 0 && align.is_power_of_two());
        let n = self.bits.len() * 64;
        let single = count == 1 && align == 1;
        let hint = if single { self.next } else { 0 };
        let i = self
            .find(hint, n, count, align)
            .or_else(|| self.find(0, n, count, align))?;
        for j in i..i + count {
            self.set(j, true);
        }
        if single {
            self.next = i + 1;
        }
        self.free -= count as u64;
        Some(i as u64 * FRAME_SIZE)
    }

    fn find(&self, from: usize, to: usize, count: usize, align: usize) -> Option<usize> {
        let mut i = align_up(from, align);
        while i + count <= to {
            if i.is_multiple_of(64) && self.bits[i / 64] == !0 {
                i = align_up(i + 64, align);
                continue;
            }
            match (i..i + count).find(|&j| self.used(j)) {
                None => return Some(i),
                Some(j) => i = align_up(j + 1, align),
            }
        }
        None
    }

    /// Returns `count` frames starting at `phys` (from `alloc`). Freeing a
    /// frame that is not allocated is a kernel bug and panics.
    pub fn free(&mut self, phys: u64, count: usize) {
        let first = (phys / FRAME_SIZE) as usize;
        for i in first..first + count {
            assert!(
                self.used(i),
                "double free of frame {:#x}",
                i as u64 * FRAME_SIZE
            );
            self.set(i, false);
        }
        self.free += count as u64;
        self.next = self.next.min(first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MemoryKind::*;

    fn r(start: u64, end: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion {
            start,
            len: end - start,
            kind,
        }
    }

    /// The NUC 12's firmware memory map as Linux reports it
    /// (`/sys/firmware/memmap`), end addresses exclusive.
    fn nuc_map() -> Vec<MemoryRegion> {
        vec![
            r(0x0, 0x9e000, Usable),
            r(0x9e000, 0x9f000, Reserved),
            r(0x9f000, 0xa0000, Usable),
            r(0xa0000, 0x100000, Reserved),
            r(0x100000, 0x4dd33000, Usable),
            r(0x4dd33000, 0x56c5f000, Reserved),
            r(0x56c5f000, 0x56d48000, AcpiReclaimable),
            r(0x56d48000, 0x56ece000, AcpiNvs),
            r(0x56ece000, 0x5a7ff000, Reserved),
            r(0x5a7ff000, 0x5a800000, Usable),
            r(0x5a800000, 0x61200000, Reserved),
            r(0x61e00000, 0x68800000, Reserved),
            r(0xc0000000, 0xd0000000, Reserved),
            r(0xfed00000, 0xfed01000, Reserved),
            r(0xfee00000, 0xfee01000, Reserved),
            r(0x100000000, 0x497800000, Usable),
        ]
    }

    /// A small map: usable memory split by loader memory, with an unaligned
    /// region edge.
    fn small_map() -> Vec<MemoryRegion> {
        vec![
            r(0x0, 0xa0000, Usable),
            r(0x100000, 0x180000, Usable),
            r(0x180000, 0x190000, Bootloader),
            r(0x190000, 0x200000, Usable),
            r(0x200800, 0x205000, Usable), // frames 0x201..0x205 only
            r(0x300000, 0x301000, AcpiReclaimable),
        ]
    }

    fn build(map: &[MemoryRegion]) -> (FrameAllocator<'static>, u64) {
        let words = bitmap_words(map);
        let phys = place_bitmap(map, words * 8).unwrap();
        let bits = Box::leak(vec![0u64; words].into_boxed_slice());
        (FrameAllocator::new(map, bits, phys), phys)
    }

    #[test]
    fn bitmap_for_the_nuc_covers_all_ram_and_sits_above_1_mib() {
        let map = nuc_map();
        assert_eq!(bitmap_words(&map), 0x497800 / 64);
        assert_eq!(place_bitmap(&map, bitmap_words(&map) * 8), Some(0x100000));
    }

    #[test]
    fn bitmap_goes_to_the_first_region_big_enough() {
        let map = [r(0x100000, 0x101000, Usable), r(0x200000, 0x400000, Usable)];
        assert_eq!(place_bitmap(&map, 8192), Some(0x200000));
        assert_eq!(place_bitmap(&map, 3 << 20), None);
    }

    #[test]
    fn hands_out_every_usable_frame_once_and_nothing_else() {
        let map = small_map();
        let (mut a, bitmap) = build(&map);
        let free = a.free_frames();
        let mut got = Vec::new();
        while let Some(p) = a.alloc(1, 1) {
            got.push(p);
        }
        assert_eq!(got.len() as u64, free);
        assert_eq!(a.free_frames(), 0);
        got.sort();
        got.dedup();
        assert_eq!(got.len() as u64, free, "no frame twice");
        for p in &got {
            assert!(*p >= LOW_LIMIT, "{p:#x} is below 1 MiB");
            assert_ne!(*p, bitmap, "the bitmap's own frame");
            let region = map
                .iter()
                .find(|m| m.start <= *p && *p + FRAME_SIZE <= m.end());
            assert_eq!(region.map(|m| m.kind), Some(Usable), "{p:#x}");
        }
        // 0x100000..0x180000 minus the bitmap frame, 0x190000..0x200000,
        // 0x201000..0x205000.
        assert_eq!(free, 0x80 - 1 + 0x70 + 4);
        assert_eq!(a.total_frames(), 0xa0 + 0x80 + 0x70 + 4);
    }

    #[test]
    fn contiguous_aligned_runs() {
        let (mut a, _) = build(&nuc_map());
        let heap = a.alloc(8192, 512).unwrap();
        assert_eq!(heap % (2 << 20), 0, "2 MiB aligned");
        let next = a.alloc(8192, 512).unwrap();
        assert!(next >= heap + (32 << 20) || next + (32 << 20) <= heap);
        // A run never spans a reserved hole: 1.2 GiB does not fit below the
        // hole at 0x4dd33000, so it comes from the RAM above 4 GiB.
        let big = a.alloc(0x4dd33, 1).unwrap();
        assert!(big >= 0x1_0000_0000, "{big:#x}");
    }

    #[test]
    fn freed_frames_come_back() {
        let (mut a, _) = build(&small_map());
        let before = a.free_frames();
        let p = a.alloc(16, 16).unwrap();
        assert_eq!(p % (16 * FRAME_SIZE), 0);
        assert_eq!(a.free_frames(), before - 16);
        a.free(p, 16);
        assert_eq!(a.free_frames(), before);
        assert_eq!(a.alloc(16, 16), Some(p));
    }

    #[test]
    fn exhaustion_is_none_not_a_panic() {
        let (mut a, _) = build(&small_map());
        assert_eq!(a.alloc(0x1000, 1), None);
        assert!(a.alloc(1, 1).is_some());
    }

    #[test]
    #[should_panic(expected = "double free")]
    fn double_free_panics() {
        let (mut a, _) = build(&small_map());
        let p = a.alloc(1, 1).unwrap();
        a.free(p, 1);
        a.free(p, 1);
    }
}
