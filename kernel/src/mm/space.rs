//! A program's address space (user-space gate §5.1): a PML4 of its own,
//! whose lower half holds the program's pages and whose upper half is the
//! kernel's, shared. The kernel's upper-half PML4 entries never change
//! after `mm::init` (`PageTables::fill_upper_half`), so copying them once
//! is enough for the space to see every kernel mapping, later ones too.

use super::paging::{LOWER_HALF_END, MapError, PAGE, PageTables, Perm, PhysMem};

pub struct AddressSpace {
    tables: PageTables,
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
        Ok(AddressSpace { tables })
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
