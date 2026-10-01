//! A program's address space (user-space gate §5.1): a PML4 of its own,
//! whose lower half holds the program's pages and whose upper half is the
//! kernel's, shared. The kernel's upper-half PML4 entries never change
//! after `mm::init` (`PageTables::fill_upper_half`), so copying them once
//! is enough for the space to see every kernel mapping, later ones too.
//!
//! The space also keeps the list of what `mem_map` gave the program
//! (§5.1, §5.4): regions of the `mem_map` area, first fit from its start,
//! merged where they meet, so `mem_unmap` can give back whole pages of
//! them and nothing else.

use super::paging::{LOWER_HALF_END, MapError, PAGE, PageTables, Perm, PhysMem};
use alloc::vec::Vec;
use vfs::Errno;

/// The `mem_map` area (spec §5.1).
pub const MAP_START: u64 = 0x0000_1000_0000_0000;
pub const MAP_END: u64 = 0x0000_7000_0000_0000;
/// The most regions of `mem_map` memory a program may have, so its list
/// cannot fill the kernel's heap (regions that meet count as one, as
/// Linux's `max_map_count` counts them).
pub const MAPS_MAX: usize = 1024;

pub struct AddressSpace {
    tables: PageTables,
    /// The `mem_map` regions, as (start, pages), sorted and apart.
    maps: Vec<(u64, u64)>,
}

impl AddressSpace {
    /// An empty lower half under the kernel's upper half (PML4 entries
    /// 256-511 of `kernel`).
    pub fn new(mem: &mut impl PhysMem, kernel: &PageTables) -> Result<AddressSpace, MapError> {
        let mut tables = PageTables::new(mem)?;
        for i in 256..512 {
            let e = mem.table(kernel.pml4)[i];
            tables.set_pml4_entry(mem, i, e);
        }
        Ok(AddressSpace {
            tables,
            maps: Vec::new(),
        })
    }

    /// The physical address of its PML4, for CR3.
    pub fn pml4(&self) -> u64 {
        self.tables.pml4
    }

    /// Maps `pages` fresh zeroed pages from `virt` for the program, with
    /// `perm` (spec §5.1: memory is allocated and zeroed when it is mapped).
    /// On failure the pages mapped so far stay mapped; `destroy` gives them
    /// back with the rest.
    pub fn map_zeroed(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        pages: u64,
        perm: Perm,
    ) -> Result<(), MapError> {
        let end = pages
            .checked_mul(PAGE)
            .and_then(|len| virt.checked_add(len))
            .filter(|&end| end <= LOWER_HALF_END)
            .ok_or(MapError::NotUser { virt })?;
        let mut v = virt;
        while v < end {
            let frame = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            if let Err(e) = self.tables.map_user(mem, v, frame, perm) {
                mem.free_frame(frame);
                return Err(e);
            }
            v += PAGE;
        }
        Ok(())
    }

    /// The frame and permission of the program's page holding `virt`.
    pub fn user_page(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Perm)> {
        self.tables.user_page(mem, virt)
    }

    /// Copies `bytes` to `virt`, whatever the pages' permissions: the
    /// kernel filling a program's pages before it runs. Every page must be
    /// mapped already.
    pub fn fill(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        bytes: &[u8],
    ) -> Result<(), MapError> {
        let mut done = 0;
        while done < bytes.len() {
            let v = virt
                .checked_add(done as u64)
                .ok_or(MapError::NotUser { virt })?;
            let (frame, _) = self
                .user_page(mem, v)
                .ok_or(MapError::NotUser { virt: v })?;
            let at = (v % PAGE) as usize;
            let n = (PAGE as usize - at).min(bytes.len() - done);
            mem.bytes(frame)[at..at + n].copy_from_slice(&bytes[done..done + n]);
            done += n;
        }
        Ok(())
    }

    /// Maps `pages` fresh zeroed read-write pages at the lowest place in the
    /// `mem_map` area they fit (spec §7.3); their address. `ENOMEM` if
    /// they do not fit in the area, need more than the `room` frames the
    /// program may still take, would make more than `MAPS_MAX` regions, or
    /// the frames run out; then nothing stays mapped.
    pub fn map_area(
        &mut self,
        mem: &mut impl PhysMem,
        pages: u64,
        room: u64,
    ) -> Result<u64, Errno> {
        if pages == 0 {
            return Err(Errno::EINVAL);
        }
        if pages > room {
            return Err(Errno::ENOMEM);
        }
        let len = pages.checked_mul(PAGE).ok_or(Errno::ENOMEM)?;
        // First fit: the gaps between the regions, in address order.
        let mut at = MAP_START;
        let mut i = 0;
        loop {
            let next = self.maps.get(i).map_or(MAP_END, |&(s, _)| s);
            if next - at >= len {
                break;
            }
            match self.maps.get(i) {
                Some(&(s, n)) => at = s + n * PAGE,
                None => return Err(Errno::ENOMEM),
            }
            i += 1;
        }
        let meets_before = i > 0 && self.end_of(i - 1) == at;
        let meets_after = self.maps.get(i).is_some_and(|&(s, _)| s == at + len);
        if !meets_before && !meets_after && self.maps.len() >= MAPS_MAX {
            return Err(Errno::ENOMEM);
        }
        for k in 0..pages {
            if self
                .map_zeroed(mem, at + k * PAGE, 1, Perm::ReadWrite)
                .is_err()
            {
                self.unmap_pages(mem, at, k);
                return Err(Errno::ENOMEM);
            }
        }
        match (meets_before, meets_after) {
            (true, true) => {
                let (_, n) = self.maps.remove(i);
                self.maps[i - 1].1 += pages + n;
            }
            (true, false) => self.maps[i - 1].1 += pages,
            (false, true) => self.maps[i] = (at, pages + self.maps[i].1),
            (false, false) => self.maps.insert(i, (at, pages)),
        }
        Ok(at)
    }

    /// The first address after region `i`.
    fn end_of(&self, i: usize) -> u64 {
        let (s, n) = self.maps[i];
        s + n * PAGE
    }

    /// Unmaps `pages` pages from `addr` (spec §7.3) and gives their frames
    /// back: whole pages of earlier `map_area`s only, `EINVAL` otherwise;
    /// `ENOMEM` if cutting a region in two would make more than `MAPS_MAX`.
    /// The caller flushes the TLB for them.
    pub fn unmap_area(
        &mut self,
        mem: &mut impl PhysMem,
        addr: u64,
        pages: u64,
    ) -> Result<(), Errno> {
        let end = pages
            .checked_mul(PAGE)
            .and_then(|len| addr.checked_add(len))
            .ok_or(Errno::EINVAL)?;
        if pages == 0 || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let i = self
            .maps
            .iter()
            .position(|&(s, n)| s <= addr && addr < s + n * PAGE)
            .ok_or(Errno::EINVAL)?;
        let (s, _) = self.maps[i];
        let region_end = self.end_of(i);
        if end > region_end {
            return Err(Errno::EINVAL);
        }
        let (before, after) = (addr > s, end < region_end);
        if before && after && self.maps.len() >= MAPS_MAX {
            return Err(Errno::ENOMEM);
        }
        self.unmap_pages(mem, addr, pages);
        match (before, after) {
            (true, true) => {
                self.maps[i].1 = (addr - s) / PAGE;
                self.maps.insert(i + 1, (end, (region_end - end) / PAGE));
            }
            (true, false) => self.maps[i].1 = (addr - s) / PAGE,
            (false, true) => self.maps[i] = (end, (region_end - end) / PAGE),
            (false, false) => {
                self.maps.remove(i);
            }
        }
        Ok(())
    }

    /// Unmaps `pages` pages from `addr` and frees their frames.
    fn unmap_pages(&mut self, mem: &mut impl PhysMem, addr: u64, pages: u64) {
        for k in 0..pages {
            if let Some(frame) = self.tables.unmap(mem, addr + k * PAGE) {
                mem.free_frame(frame);
            }
        }
    }

    /// The `mem_map` regions, as (start, pages).
    pub fn maps(&self) -> &[(u64, u64)] {
        &self.maps
    }

    /// The frames it holds: its pages, their tables and its PML4, for
    /// `ps` (spec §9.3).
    pub fn frames(&self, mem: &mut impl PhysMem) -> u64 {
        1 + self.tables.lower_half_frames(mem)
    }

    /// Gives back every page, table and the PML4. The kernel must not be
    /// running on this space's tables (CR3) any more.
    pub fn destroy(mut self, mem: &mut impl PhysMem) {
        self.tables.free_lower_half(mem);
        mem.free_frame(self.tables.pml4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::Cache;
    use crate::mm::testing::FakeMem;

    const V: u64 = 0xFFFF_8000_0000_0000;
    const U: u64 = 0x40_0000;

    fn kernel(m: &mut FakeMem) -> PageTables {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        k.map(m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        k
    }

    /// The bytes of the program's memory at `virt`.
    fn read(m: &mut FakeMem, s: &AddressSpace, virt: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| {
                let (frame, _) = s.user_page(m, virt + i).expect("mapped");
                m.bytes(frame)[((virt + i) % PAGE) as usize]
            })
            .collect()
    }

    #[test]
    fn a_new_space_shares_the_kernel_half_and_sees_later_kernel_mappings() {
        let mut m = FakeMem::new();
        let mut k = kernel(&mut m);
        let s = AddressSpace::new(&mut m, &k).unwrap();
        let (kp, sp) = (*m.table(k.pml4), *m.table(s.pml4()));
        assert_eq!(kp[256..], sp[256..], "the upper half is the kernel's");
        assert!(sp[..256].iter().all(|&e| e == 0), "the lower half is empty");
        // A kernel mapping made afterwards (a kernel stack, `map_mmio`).
        k.map(&mut m, V + 0x7000_0000, 0x7000_0000, PAGE, Cache::Uncached)
            .unwrap();
        let space_tables = PageTables { pml4: s.pml4() };
        assert_eq!(
            space_tables.translate(&mut m, V + 0x7000_0000),
            Some((0x7000_0000, Cache::Uncached, PAGE))
        );
        s.destroy(&mut m);
    }

    #[test]
    fn mapped_pages_are_zeroed_filled_and_all_given_back() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        s.map_zeroed(&mut m, U, 3, Perm::ReadExec).unwrap();
        s.map_zeroed(&mut m, 0x7FFF_FFF0_0000, 255, Perm::ReadWrite)
            .unwrap();
        assert_eq!(
            read(&mut m, &s, U, 3 * PAGE as usize),
            vec![0; 3 * PAGE as usize]
        );
        // Across a page boundary, into a read-only page.
        s.fill(&mut m, U + PAGE - 2, b"code").unwrap();
        assert_eq!(read(&mut m, &s, U + PAGE - 3, 6), b"\0code\0");
        assert_eq!(s.user_page(&mut m, U).unwrap().1, Perm::ReadExec);
        assert_eq!(
            s.fill(&mut m, U + 3 * PAGE - 1, b"xy"),
            Err(MapError::NotUser { virt: U + 3 * PAGE }),
            "past the mapped pages"
        );
        assert!(m.frames() > before + 258);
        s.destroy(&mut m);
        assert_eq!(m.frames(), before, "every frame came back");
    }

    #[test]
    fn a_space_counts_every_frame_it_holds() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames() as u64;
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        assert_eq!(s.frames(&mut m), 1, "only its PML4");
        s.map_zeroed(&mut m, U, 3, Perm::ReadExec).unwrap();
        // Another table of each level for a page far away.
        s.map_zeroed(&mut m, 0x7FFF_FFF0_0000, 1, Perm::ReadWrite)
            .unwrap();
        let a = s.map_area(&mut m, 600, u64::MAX).unwrap();
        assert_eq!(s.frames(&mut m), m.frames() as u64 - before);
        s.unmap_area(&mut m, a, 600).unwrap();
        assert_eq!(s.frames(&mut m), m.frames() as u64 - before);
        s.destroy(&mut m);
    }

    #[test]
    fn running_out_of_frames_midway_leaks_nothing() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        for extra in 1..12 {
            m.limit = before + extra;
            let Ok(mut s) = AddressSpace::new(&mut m, &k) else {
                assert_eq!(m.frames(), before);
                continue;
            };
            assert_eq!(
                s.map_zeroed(&mut m, U, 16, Perm::ReadWrite),
                Err(MapError::OutOfMemory)
            );
            s.destroy(&mut m);
            assert_eq!(m.frames(), before, "limit {extra}");
        }
    }

    fn space(m: &mut FakeMem) -> AddressSpace {
        let k = kernel(m);
        AddressSpace::new(m, &k).unwrap()
    }

    #[test]
    fn mem_map_fills_the_area_from_its_start_and_merges_what_meets() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let a = s.map_area(&mut m, 2, u64::MAX).unwrap();
        assert_eq!(a, MAP_START);
        let (frame, perm) = s.user_page(&mut m, a + PAGE).unwrap();
        assert_eq!(perm, Perm::ReadWrite);
        assert!(m.bytes(frame).iter().all(|&b| b == 0), "zeroed");
        let b = s.map_area(&mut m, 3, u64::MAX).unwrap();
        assert_eq!(b, a + 2 * PAGE);
        assert_eq!(s.maps(), [(MAP_START, 5)], "one region");
        // A hole is filled first fit, and closing it merges all three.
        s.unmap_area(&mut m, a + PAGE, 2).unwrap();
        assert_eq!(s.maps(), [(MAP_START, 1), (MAP_START + 3 * PAGE, 2)]);
        assert!(s.user_page(&mut m, a + PAGE).is_none(), "unmapped");
        assert_eq!(
            s.map_area(&mut m, 3, u64::MAX).unwrap(),
            MAP_START + 5 * PAGE,
            "too big for the hole"
        );
        assert_eq!(s.map_area(&mut m, 2, u64::MAX).unwrap(), a + PAGE);
        assert_eq!(s.maps(), [(MAP_START, 8)]);
        s.destroy(&mut m);
    }

    #[test]
    fn mem_unmap_takes_whole_pages_of_earlier_maps_only() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        let a = s.map_area(&mut m, 4, u64::MAX).unwrap();
        for (addr, pages) in [
            (a + 1, 1),
            (a, 0),
            (a, 5),
            (a - PAGE, 1),
            (a + 4 * PAGE, 1),
            (U, 1),
            (u64::MAX - PAGE + 1, 2),
            (a, u64::MAX),
        ] {
            assert_eq!(
                s.unmap_area(&mut m, addr, pages),
                Err(Errno::EINVAL),
                "{addr:#x} {pages}"
            );
        }
        assert_eq!(s.maps(), [(a, 4)], "nothing changed");
        // The first and last pages, then the middle.
        s.unmap_area(&mut m, a, 1).unwrap();
        s.unmap_area(&mut m, a + 3 * PAGE, 1).unwrap();
        assert_eq!(s.maps(), [(a + PAGE, 2)]);
        assert_eq!(
            s.unmap_area(&mut m, a, 2),
            Err(Errno::EINVAL),
            "the first is gone"
        );
        s.unmap_area(&mut m, a + PAGE, 2).unwrap();
        assert!(s.maps().is_empty());
        s.destroy(&mut m);
        assert_eq!(m.frames(), before, "every frame came back");
    }

    #[test]
    fn mem_map_refuses_what_it_cannot_give() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        assert_eq!(s.map_area(&mut m, 0, u64::MAX), Err(Errno::EINVAL));
        assert_eq!(
            s.map_area(&mut m, 11, 10),
            Err(Errno::ENOMEM),
            "more than the room"
        );
        let area = (MAP_END - MAP_START) / PAGE;
        assert_eq!(s.map_area(&mut m, area + 1, u64::MAX), Err(Errno::ENOMEM));
        assert_eq!(s.map_area(&mut m, u64::MAX, u64::MAX), Err(Errno::ENOMEM));
        assert!(s.maps().is_empty());
        // Running out of frames midway leaves nothing mapped.
        let frames = m.frames();
        m.limit = frames + 6;
        assert_eq!(s.map_area(&mut m, 8, u64::MAX), Err(Errno::ENOMEM));
        assert!(s.maps().is_empty());
        assert!(s.user_page(&mut m, MAP_START).is_none());
        m.limit = usize::MAX;
        assert_eq!(
            s.map_area(&mut m, 8, u64::MAX),
            Ok(MAP_START),
            "the same place again"
        );
        s.destroy(&mut m);
    }

    #[test]
    fn at_most_1024_regions() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let n = MAPS_MAX as u64;
        let page = |k: u64| MAP_START + k * PAGE;
        // Pages 0..=2n, then 0 and 1 and every odd page from 3 given back:
        // n regions of one page, 2, 4, ..., 2n, and a hole of two pages at
        // the start of the area.
        s.map_area(&mut m, 2 * n + 1, u64::MAX).unwrap();
        s.unmap_area(&mut m, page(0), 2).unwrap();
        for k in 1..n {
            s.unmap_area(&mut m, page(2 * k + 1), 1).unwrap();
        }
        assert_eq!(s.maps().len(), MAPS_MAX);
        // A region apart from the others is refused; one that meets
        // another is not.
        assert_eq!(s.map_area(&mut m, 1, u64::MAX), Err(Errno::ENOMEM));
        assert_eq!(s.map_area(&mut m, 2, u64::MAX), Ok(page(0)));
        assert_eq!(s.maps().len(), MAPS_MAX);
        assert_eq!(s.maps()[0], (page(0), 3));
        // So is cutting a region in two; taking its end is not.
        assert_eq!(s.unmap_area(&mut m, page(1), 1), Err(Errno::ENOMEM));
        assert!(s.user_page(&mut m, page(1)).is_some(), "still mapped");
        s.unmap_area(&mut m, page(0), 1).unwrap();
        assert_eq!(s.maps().len(), MAPS_MAX);
        s.destroy(&mut m);
    }

    #[test]
    fn a_range_must_stay_in_the_lower_half() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        let last = LOWER_HALF_END - PAGE;
        assert_eq!(
            s.map_zeroed(&mut m, last, 2, Perm::Read),
            Err(MapError::NotUser { virt: last })
        );
        assert_eq!(
            s.map_zeroed(&mut m, U, u64::MAX / 2, Perm::Read),
            Err(MapError::NotUser { virt: U })
        );
        s.map_zeroed(&mut m, last, 1, Perm::Read).unwrap();
        // Mapping a page twice fails without keeping the second frame.
        let frames = m.frames();
        assert_eq!(
            s.map_zeroed(&mut m, last, 1, Perm::Read),
            Err(MapError::Conflict { virt: last })
        );
        assert_eq!(m.frames(), frames);
        s.destroy(&mut m);
        assert_eq!(m.frames(), before);
    }
}
