//! Four-level x86_64 page tables (spec §5.3). The logic reaches physical
//! memory only through `PhysMem`, so it is tested on the host against a fake;
//! the kernel's implementation goes through the linear map.
//!
//! The kernel's own mappings (`map`) are writable and not executable (the
//! kernel image keeps the loader's mappings). Each physical page appears at
//! most once in the linear map, at `PHYS_OFFSET + phys`, so there are never
//! two mappings of one page with different cache types.
//!
//! A program's pages (`map_user`, user-space gate §5.1) are 4 KiB pages in
//! the lower half with the user bit and the permissions of their segment;
//! `free_lower_half` gives back every one of them with its tables.

use boot_info::{MemoryKind, MemoryRegion};

pub const PAGE: u64 = 4096;
pub const HUGE: u64 = 2 << 20;

const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const ACCESSED: u64 = 1 << 5;
const DIRTY: u64 = 1 << 6;
const HUGE_PAGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDR: u64 = 0x000F_FFFF_FFFF_F000;
const HUGE_ADDR: u64 = 0x000F_FFFF_FFE0_0000;
/// The first address of the upper half; PML4 entries 256-511.
pub const UPPER_HALF: u64 = 0xFFFF_8000_0000_0000;
/// The end of the lower half (PML4 entries 0-255), which programs own.
pub const LOWER_HALF_END: u64 = 0x0000_8000_0000_0000;

/// What a program may do with one of its pages (user-space gate §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perm {
    /// Code: read and execute.
    ReadExec,
    /// Read-only data: not executable.
    Read,
    /// Data, the stack: writable, not executable.
    ReadWrite,
}

impl Perm {
    fn bits(self) -> u64 {
        match self {
            Perm::ReadExec => 0,
            Perm::Read => NO_EXECUTE,
            Perm::ReadWrite => WRITABLE | NO_EXECUTE,
        }
    }

    fn of_entry(e: u64) -> Perm {
        if e & WRITABLE != 0 {
            Perm::ReadWrite
        } else if e & NO_EXECUTE != 0 {
            Perm::Read
        } else {
            Perm::ReadExec
        }
    }
}

/// Memory types used by the kernel. The PWT/PCD bits of an entry pick one
/// of the first four PAT entries (the PAT bit itself is never set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cache {
    /// RAM. PAT entry 0.
    WriteBack,
    /// The framebuffer. PAT entry 1.
    WriteCombining,
    /// Device registers. PAT entry 3.
    Uncached,
}

impl Cache {
    fn bits(self) -> u64 {
        match self {
            Cache::WriteBack => 0,
            Cache::WriteCombining => PWT,
            Cache::Uncached => PWT | PCD,
        }
    }

    #[cfg(test)]
    fn of_entry(e: u64) -> Cache {
        match e & (PWT | PCD) {
            0 => Cache::WriteBack,
            PWT => Cache::WriteCombining,
            _ => Cache::Uncached,
        }
    }
}

/// Value for the IA32_PAT MSR (0x277). One byte per entry, entry 0 lowest:
/// WB, WC, UC-, UC, WB, WT, UC-, UC. Entry 1 is write-combining instead of
/// the power-on write-through; the others keep their power-on types.
pub const PAT_VALUE: u64 = 0x0007_0406_0007_0106;

/// Physical memory as the page-table code sees it.
pub trait PhysMem {
    /// The page table stored in the frame at `phys`.
    fn table(&mut self, phys: u64) -> &mut [u64; 512];
    /// A zeroed frame for a new table (or a program's page), or `None`
    /// when memory is exhausted.
    fn alloc_table(&mut self) -> Option<u64>;
    /// Gives back a frame from `alloc_table`.
    fn free_frame(&mut self, phys: u64);
    /// The bytes of the frame at `phys`.
    fn bytes(&mut self, phys: u64) -> &mut [u8; PAGE as usize] {
        let table: *mut [u64; 512] = self.table(phys);
        // SAFETY: a frame of 512 `u64`s is 4096 bytes, and bytes have no
        // alignment of their own.
        unsafe { &mut *table.cast::<[u8; PAGE as usize]>() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    OutOfMemory,
    /// `virt` is already mapped to another address or with another cache
    /// type.
    Conflict {
        virt: u64,
    },
    /// Addresses and lengths must be multiples of 4 KiB.
    Unaligned,
    /// The physical range lies beyond what the linear map can cover.
    OutOfRange,
    /// A program's page must lie in the lower half.
    NotUser {
        virt: u64,
    },
}

impl core::fmt::Display for MapError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            MapError::OutOfMemory => write!(f, "out of memory for page tables"),
            MapError::Conflict { virt } => write!(f, "{virt:#x} is already mapped differently"),
            MapError::Unaligned => write!(f, "range is not page-aligned"),
            MapError::OutOfRange => write!(f, "physical address beyond the linear map"),
            MapError::NotUser { virt } => write!(f, "{virt:#x} is not in the lower half"),
        }
    }
}

fn index(virt: u64, level: u32) -> usize {
    ((virt >> (12 + 9 * level)) & 0x1FF) as usize
}

/// Equal apart from the accessed and dirty bits, which the CPU sets.
fn same(entry: u64, want: u64) -> bool {
    entry & !(ACCESSED | DIRTY) == want
}

pub struct PageTables {
    pub pml4: u64,
}

impl PageTables {
    pub fn new(mem: &mut impl PhysMem) -> Result<PageTables, MapError> {
        let pml4 = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
        Ok(PageTables { pml4 })
    }

    /// Gives every empty upper-half PML4 entry (256-511) an empty table,
    /// so the kernel's half never gains a PML4 entry again: every address
    /// space copies these entries once, when it is made, and still sees
    /// every kernel mapping made later (user-space gate §5.1).
    pub fn fill_upper_half(&mut self, mem: &mut impl PhysMem) -> Result<(), MapError> {
        for i in 256..512 {
            if mem.table(self.pml4)[i] & PRESENT == 0 {
                let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
                mem.table(self.pml4)[i] = t | PRESENT | WRITABLE;
            }
        }
        Ok(())
    }

    /// Sets PML4 entry `i` directly, for sharing a subtree of other tables.
    pub fn set_pml4_entry(&mut self, mem: &mut impl PhysMem, i: usize, entry: u64) {
        mem.table(self.pml4)[i] = entry;
    }

    /// Maps [virt, virt + len) to [phys, phys + len), writable and not
    /// executable, using 2 MiB pages wherever both addresses are aligned.
    /// Pages already mapped the same way are left alone.
    pub fn map(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        phys: u64,
        len: u64,
        cache: Cache,
    ) -> Result<(), MapError> {
        if !(virt | phys | len).is_multiple_of(PAGE) {
            return Err(MapError::Unaligned);
        }
        let mut off = 0;
        while off < len {
            let (v, p) = (virt + off, phys + off);
            if (v | p).is_multiple_of(HUGE)
                && len - off >= HUGE
                && self.map_huge(mem, v, p, cache)?
            {
                off += HUGE;
            } else {
                self.map_4k(mem, v, p, cache)?;
                off += PAGE;
            }
        }
        Ok(())
    }

    /// The next-level table under entry `i` of `table`, created if missing.
    /// Tables of the lower half carry the user bit (the leaf entry decides
    /// what ring 3 may do); the kernel's never do.
    fn child(
        &mut self,
        mem: &mut impl PhysMem,
        table: u64,
        i: usize,
        virt: u64,
    ) -> Result<u64, MapError> {
        let flags = if virt < LOWER_HALF_END {
            PRESENT | WRITABLE | USER
        } else {
            PRESENT | WRITABLE
        };
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            mem.table(table)[i] = t | flags;
            Ok(t)
        } else if e & HUGE_PAGE != 0 || e & flags != flags {
            Err(MapError::Conflict { virt })
        } else {
            Ok(e & ADDR)
        }
    }

    /// The page directory covering `virt` and the entry index in it.
    fn pd_slot(&mut self, mem: &mut impl PhysMem, virt: u64) -> Result<(u64, usize), MapError> {
        let pdpt = self.child(mem, self.pml4, index(virt, 3), virt)?;
        let pd = self.child(mem, pdpt, index(virt, 2), virt)?;
        Ok((pd, index(virt, 1)))
    }

    /// Maps the 4 KiB page at `virt` in the lower half to the frame `phys`
    /// for ring 3, with `perm`. A page already mapped is a conflict: a
    /// program's pages are mapped once, when they are allocated.
    pub fn map_user(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        phys: u64,
        perm: Perm,
    ) -> Result<(), MapError> {
        if !(virt | phys).is_multiple_of(PAGE) {
            return Err(MapError::Unaligned);
        }
        if virt >= LOWER_HALF_END {
            return Err(MapError::NotUser { virt });
        }
        let (pd, i) = self.pd_slot(mem, virt)?;
        let pt = self.child(mem, pd, i, virt)?;
        let slot = &mut mem.table(pt)[index(virt, 0)];
        if *slot & PRESENT != 0 {
            return Err(MapError::Conflict { virt });
        }
        *slot = phys | PRESENT | USER | perm.bits();
        Ok(())
    }

    /// The 4 KiB leaf entry for `virt`, if every table on the way is there.
    fn leaf(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, usize)> {
        let mut table = self.pml4;
        for level in (1..4).rev() {
            let e = mem.table(table)[index(virt, level)];
            if e & PRESENT == 0 || e & HUGE_PAGE != 0 {
                return None;
            }
            table = e & ADDR;
        }
        Some((table, index(virt, 0)))
    }

    /// Removes the 4 KiB page at `virt` and returns its frame; `None` if
    /// no 4 KiB page is mapped there. The tables stay; the caller flushes
    /// the TLB.
    pub fn unmap(&mut self, mem: &mut impl PhysMem, virt: u64) -> Option<u64> {
        let (pt, i) = self.leaf(mem, virt)?;
        let e = mem.table(pt)[i];
        if e & PRESENT == 0 {
            return None;
        }
        mem.table(pt)[i] = 0;
        Some(e & ADDR)
    }

    /// The frame and permission of the page holding `virt`, if ring 3 may
    /// use it: in the lower half, and the user bit at every level.
    pub fn user_page(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Perm)> {
        if virt >= LOWER_HALF_END {
            return None;
        }
        let mut table = self.pml4;
        for level in (0..4).rev() {
            let e = mem.table(table)[index(virt, level)];
            if e & (PRESENT | USER) != PRESENT | USER || e & HUGE_PAGE != 0 {
                return None;
            }
            if level == 0 {
                return Some((e & ADDR, Perm::of_entry(e)));
            }
            table = e & ADDR;
        }
        None
    }

    /// Gives back every page of the lower half and every table below its
    /// PML4 entries, which are left empty (user-space gate §5.1). Only
    /// `map_user` puts pages there, so all of them are 4 KiB pages.
    pub fn free_lower_half(&mut self, mem: &mut impl PhysMem) {
        for i in 0..256 {
            let pdpt = mem.table(self.pml4)[i];
            if pdpt & PRESENT == 0 {
                continue;
            }
            mem.table(self.pml4)[i] = 0;
            free_table(mem, pdpt & ADDR, 2);
        }
    }

    /// Maps one 2 MiB page. `Ok(false)` if that slot already holds a table
    /// of 4 KiB pages; the caller then maps page by page.
    fn map_huge(
        &mut self,
        mem: &mut impl PhysMem,
        v: u64,
        p: u64,
        cache: Cache,
    ) -> Result<bool, MapError> {
        let (pd, i) = self.pd_slot(mem, v)?;
        let e = mem.table(pd)[i];
        let want = p | PRESENT | WRITABLE | HUGE_PAGE | NO_EXECUTE | cache.bits();
        if e & PRESENT == 0 {
            mem.table(pd)[i] = want;
            Ok(true)
        } else if e & HUGE_PAGE == 0 {
            Ok(false)
        } else if same(e, want) {
            Ok(true)
        } else {
            Err(MapError::Conflict { virt: v })
        }
    }

    fn map_4k(
        &mut self,
        mem: &mut impl PhysMem,
        v: u64,
        p: u64,
        cache: Cache,
    ) -> Result<(), MapError> {
        let (pd, i) = self.pd_slot(mem, v)?;
        let e = mem.table(pd)[i];
        if e & PRESENT != 0 && e & HUGE_PAGE != 0 {
            // Inside an existing 2 MiB page: fine if it maps `v` the same way.
            let want =
                (p & !(HUGE - 1)) | PRESENT | WRITABLE | HUGE_PAGE | NO_EXECUTE | cache.bits();
            return if same(e, want) && (e & HUGE_ADDR) + v % HUGE == p {
                Ok(())
            } else {
                Err(MapError::Conflict { virt: v })
            };
        }
        let pt = self.child(mem, pd, i, v)?;
        let slot = &mut mem.table(pt)[index(v, 0)];
        let want = p | PRESENT | WRITABLE | NO_EXECUTE | cache.bits();
        if *slot & PRESENT == 0 {
            *slot = want;
            Ok(())
        } else if same(*slot, want) {
            Ok(())
        } else {
            Err(MapError::Conflict { virt: v })
        }
    }

    /// The physical address, cache type and page size `virt` maps to (for
    /// the tests of what maps and unmaps).
    #[cfg(test)]
    pub fn translate(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Cache, u64)> {
        let mut table = self.pml4;
        for level in (0..4).rev() {
            let e = mem.table(table)[index(virt, level)];
            if e & PRESENT == 0 {
                return None;
            }
            let size = 1u64 << (12 + 9 * level);
            if level == 0 || e & HUGE_PAGE != 0 {
                let base = e & ADDR & !(size - 1);
                return Some((base + virt % size, Cache::of_entry(e), size));
            }
            table = e & ADDR;
        }
        None
    }
}

/// Frees the table at `table` of `level` (2 a PDPT, 1 a PD, 0 a PT), the
/// tables below it and, from a PT, the pages it maps.
fn free_table(mem: &mut impl PhysMem, table: u64, level: u32) {
    for i in 0..512 {
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            continue;
        }
        if level == 0 {
            mem.free_frame(e & ADDR);
        } else {
            free_table(mem, e & ADDR, level - 1);
        }
    }
    mem.free_frame(table);
}

/// The physical ranges the kernel's linear map covers: every RAM-type
/// region (usable, loader, kernel and ACPI memory), with touching regions
/// merged so 2 MiB pages can span their boundaries. Reserved and MMIO
/// ranges are left out; `map_mmio` maps those on demand.
pub fn linear_ranges(map: &[MemoryRegion]) -> impl Iterator<Item = (u64, u64)> + '_ {
    let ram = |r: &&MemoryRegion| {
        matches!(
            r.kind,
            MemoryKind::Usable
                | MemoryKind::Bootloader
                | MemoryKind::Kernel
                | MemoryKind::AcpiReclaimable
                | MemoryKind::AcpiNvs
        )
    };
    let mut regions = map.iter().filter(ram).peekable();
    core::iter::from_fn(move || {
        let first = regions.next()?;
        let (start, mut end) = (first.start, first.end());
        while let Some(next) = regions.next_if(|n| n.start == end) {
            end = next.end();
        }
        Some((start, end))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::testing::FakeMem;

    const V: u64 = 0xFFFF_8000_0000_0000;

    fn setup() -> (FakeMem, PageTables) {
        let mut m = FakeMem::new();
        let t = PageTables::new(&mut m).unwrap();
        (m, t)
    }

    #[test]
    fn maps_4k_pages_with_their_cache_type() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0xFEE0_0000, 0xFEE0_0000, PAGE, Cache::Uncached)
            .unwrap();
        assert_eq!(
            t.translate(&mut m, V + 0xFEE0_0123),
            Some((0xFEE0_0123, Cache::Uncached, PAGE))
        );
        assert_eq!(t.translate(&mut m, V + 0xFEE0_1000), None);
        assert_eq!(m.tables.len(), 4, "PML4, PDPT, PD, PT");
    }

    #[test]
    fn uses_2mib_pages_where_aligned() {
        let (mut m, mut t) = setup();
        // 4 KiB up to the 2 MiB boundary, one 2 MiB page, one 4 KiB page.
        t.map(
            &mut m,
            V + 0x1F_F000,
            0x1F_F000,
            0x20_2000,
            Cache::WriteBack,
        )
        .unwrap();
        assert_eq!(t.translate(&mut m, V + 0x1F_F000).unwrap().2, PAGE);
        assert_eq!(
            t.translate(&mut m, V + 0x30_0000),
            Some((0x30_0000, Cache::WriteBack, HUGE))
        );
        assert_eq!(t.translate(&mut m, V + 0x40_0000).unwrap().2, PAGE);
        assert_eq!(t.translate(&mut m, V + 0x40_1000), None);
    }

    #[test]
    fn entries_are_writable_no_execute_and_pick_the_pat_entry() {
        let (mut m, mut t) = setup();
        t.map(
            &mut m,
            V + 0x40_0000_0000,
            0x40_0000_0000,
            HUGE,
            Cache::WriteCombining,
        )
        .unwrap();
        t.map(
            &mut m,
            V + 0x60_3D18_0000,
            0x60_3D18_0000,
            PAGE,
            Cache::Uncached,
        )
        .unwrap();
        let leaf = |m: &mut FakeMem, v: u64| {
            let mut table = t.pml4;
            for level in (0..4).rev() {
                let e = m.table(table)[index(v, level)];
                if level == 0 || e & HUGE_PAGE != 0 {
                    return e;
                }
                table = e & ADDR;
            }
            unreachable!()
        };
        let wc = leaf(&mut m, V + 0x40_0000_0000);
        assert_eq!(wc & (PWT | PCD | HUGE_PAGE), PWT | HUGE_PAGE, "PAT entry 1");
        let uc = leaf(&mut m, V + 0x60_3D18_0000);
        assert_eq!(uc & (PWT | PCD), PWT | PCD, "PAT entry 3");
        for e in [wc, uc] {
            assert_ne!(e & WRITABLE, 0);
            assert_ne!(e & NO_EXECUTE, 0);
        }
    }

    #[test]
    fn pat_value_has_wc_in_entry_1_and_power_on_types_elsewhere() {
        let entries = PAT_VALUE.to_le_bytes();
        // Encodings: UC 0, WC 1, WT 4, WB 6, UC- 7.
        assert_eq!(entries, [6, 1, 7, 0, 6, 4, 7, 0]);
    }

    #[test]
    fn mapping_again_the_same_way_is_fine_but_conflicts_are_reported() {
        let (mut m, mut t) = setup();
        let fb = 0x8000_0000;
        t.map(&mut m, V + fb, fb, 4 * HUGE, Cache::WriteCombining)
            .unwrap();
        t.map(&mut m, V + fb, fb, 4 * HUGE, Cache::WriteCombining)
            .unwrap();
        // A 4 KiB piece of an existing 2 MiB page, mapped the same way.
        t.map(
            &mut m,
            V + fb + 0x5000,
            fb + 0x5000,
            PAGE,
            Cache::WriteCombining,
        )
        .unwrap();
        assert_eq!(
            t.map(&mut m, V + fb + 0x5000, fb + 0x5000, PAGE, Cache::Uncached),
            Err(MapError::Conflict {
                virt: V + fb + 0x5000
            })
        );
        assert_eq!(
            t.map(&mut m, V + fb, fb + HUGE, HUGE, Cache::WriteCombining),
            Err(MapError::Conflict { virt: V + fb })
        );
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(
            t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::Uncached),
            Err(MapError::Conflict { virt: V + 0x1000 })
        );
    }

    #[test]
    fn accessed_and_dirty_bits_do_not_count_as_a_difference() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        let pd = m.table(t.pml4)[index(V, 3)] & ADDR;
        let pd = m.table(pd)[index(V, 2)] & ADDR;
        let pt = m.table(pd)[index(V, 1)] & ADDR;
        m.table(pt)[1] |= ACCESSED | DIRTY; // as the CPU would
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
    }

    #[test]
    fn a_2mib_range_over_existing_4k_pages_is_mapped_page_by_page() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x20_0000, 0x20_0000, PAGE, Cache::WriteBack)
            .unwrap();
        t.map(&mut m, V + 0x20_0000, 0x20_0000, HUGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(
            t.translate(&mut m, V + 0x3F_F000).unwrap(),
            (0x3F_F000, Cache::WriteBack, PAGE)
        );
    }

    #[test]
    fn unaligned_input_and_exhaustion_are_errors() {
        let (mut m, mut t) = setup();
        assert_eq!(
            t.map(&mut m, V + 1, 0, PAGE, Cache::WriteBack),
            Err(MapError::Unaligned)
        );
        assert_eq!(
            t.map(&mut m, V, 0, 100, Cache::WriteBack),
            Err(MapError::Unaligned)
        );
        m.limit = 2;
        assert_eq!(
            t.map(&mut m, V, 0, PAGE, Cache::WriteBack),
            Err(MapError::OutOfMemory)
        );
    }

    #[test]
    fn shared_pml4_entries_are_copied_verbatim() {
        let (mut m, mut t) = setup();
        t.set_pml4_entry(&mut m, 511, 0x1234_5000 | PRESENT | WRITABLE);
        assert_eq!(m.table(t.pml4)[511], 0x1234_5003);
        assert!(
            m.table(t.pml4)[..256].iter().all(|&e| e == 0),
            "lower half empty"
        );
    }

    #[test]
    fn the_upper_half_gets_every_pml4_entry_up_front() {
        let (mut m, mut t) = setup();
        t.set_pml4_entry(&mut m, 511, 0x1234_5000 | PRESENT | WRITABLE);
        t.fill_upper_half(&mut m).unwrap();
        let pml4 = *m.table(t.pml4);
        assert_eq!(pml4[511], 0x1234_5003, "the loader's entry stays");
        for (i, e) in pml4.iter().enumerate() {
            if i < 256 {
                assert_eq!(*e, 0, "entry {i}: the lower half stays empty");
            } else {
                assert_eq!(
                    e & (PRESENT | WRITABLE | USER),
                    PRESENT | WRITABLE,
                    "entry {i}"
                );
            }
        }
        // 255 new tables, each empty; filling again adds none.
        assert_eq!(m.tables.len(), 1 + 255);
        t.fill_upper_half(&mut m).unwrap();
        assert_eq!(m.tables.len(), 1 + 255);
        // Kernel mappings now go under those tables.
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(m.table(t.pml4)[256], pml4[256]);
    }

    #[test]
    fn filling_the_upper_half_can_run_out_of_memory() {
        let (mut m, mut t) = setup();
        m.limit = 10;
        assert_eq!(t.fill_upper_half(&mut m), Err(MapError::OutOfMemory));
    }

    /// A lower-half address of a program's (spec §5.1).
    const U: u64 = 0x40_0000;

    /// The raw leaf entry for `v`.
    fn leaf_entry(m: &mut FakeMem, t: &PageTables, v: u64) -> u64 {
        let (pt, i) = t.leaf(m, v).expect("tables");
        m.table(pt)[i]
    }

    #[test]
    fn user_pages_carry_the_user_bit_and_their_permission() {
        let (mut m, mut t) = setup();
        let perms = [Perm::ReadExec, Perm::Read, Perm::ReadWrite];
        for (n, perm) in perms.into_iter().enumerate() {
            let frame = m.alloc_table().unwrap();
            let v = U + n as u64 * PAGE;
            t.map_user(&mut m, v, frame, perm).unwrap();
            assert_eq!(t.user_page(&mut m, v + 0x123), Some((frame, perm)));
        }
        let code = leaf_entry(&mut m, &t, U);
        assert_eq!(code & (USER | WRITABLE | NO_EXECUTE), USER, "R-X");
        let rodata = leaf_entry(&mut m, &t, U + PAGE);
        assert_eq!(rodata & (USER | WRITABLE | NO_EXECUTE), USER | NO_EXECUTE);
        let data = leaf_entry(&mut m, &t, U + 2 * PAGE);
        assert_eq!(
            data & (USER | WRITABLE | NO_EXECUTE),
            USER | WRITABLE | NO_EXECUTE
        );
        // The tables on the way let the leaf decide.
        let mut table = t.pml4;
        for level in (1..4).rev() {
            let e = m.table(table)[index(U, level)];
            assert_eq!(
                e & (PRESENT | WRITABLE | USER | NO_EXECUTE),
                PRESENT | WRITABLE | USER
            );
            table = e & ADDR;
        }
        assert_eq!(t.user_page(&mut m, U + 3 * PAGE), None, "not mapped");
    }

    #[test]
    fn kernel_pages_are_not_user_pages() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.user_page(&mut m, V + 0x1000), None);
        let pdpt = m.table(t.pml4)[index(V, 3)];
        assert_eq!(pdpt & USER, 0, "kernel tables have no user bit");
        // A kernel-only mapping in the lower half (none exists, but the
        // walk must not trust the leaf alone).
        let frame = m.alloc_table().unwrap();
        t.map_user(&mut m, U, frame, Perm::ReadWrite).unwrap();
        let (pt, i) = t.leaf(&mut m, U).unwrap();
        m.table(pt)[i] &= !USER;
        assert_eq!(t.user_page(&mut m, U), None);
        assert_eq!(t.user_page(&mut m, LOWER_HALF_END), None);
        assert_eq!(t.user_page(&mut m, u64::MAX), None);
        // An upper-half page is never a program's, whatever its bits say.
        let mut table = t.pml4;
        for level in (0..4).rev() {
            let e = &mut m.table(table)[index(V + 0x1000, level)];
            *e |= USER;
            table = *e & ADDR;
        }
        assert_eq!(t.user_page(&mut m, V + 0x1000), None);
    }

    #[test]
    fn user_pages_are_lower_half_4k_pages_mapped_once() {
        let (mut m, mut t) = setup();
        let frame = m.alloc_table().unwrap();
        assert_eq!(
            t.map_user(&mut m, LOWER_HALF_END, frame, Perm::Read),
            Err(MapError::NotUser {
                virt: LOWER_HALF_END
            })
        );
        assert_eq!(
            t.map_user(&mut m, U + 8, frame, Perm::Read),
            Err(MapError::Unaligned)
        );
        t.map_user(&mut m, U, frame, Perm::Read).unwrap();
        assert_eq!(
            t.map_user(&mut m, U, frame, Perm::Read),
            Err(MapError::Conflict { virt: U })
        );
        // A lower-half table without the user bit is not used for one.
        let kernel_table = m.alloc_table().unwrap();
        t.set_pml4_entry(&mut m, 1, kernel_table | PRESENT | WRITABLE);
        assert_eq!(
            t.map_user(&mut m, 1 << 39, frame, Perm::Read),
            Err(MapError::Conflict { virt: 1 << 39 })
        );
        // The last page below the upper half is fine.
        t.map_user(&mut m, LOWER_HALF_END - PAGE, frame, Perm::Read)
            .unwrap();
    }

    #[test]
    fn unmap_returns_the_frame_and_leaves_nothing_behind() {
        let (mut m, mut t) = setup();
        let frame = m.alloc_table().unwrap();
        t.map_user(&mut m, U, frame, Perm::ReadWrite).unwrap();
        assert_eq!(t.unmap(&mut m, U), Some(frame));
        assert_eq!(t.user_page(&mut m, U), None);
        assert_eq!(t.unmap(&mut m, U), None, "already gone");
        assert_eq!(t.unmap(&mut m, U + HUGE), None, "never mapped");
        // Kernel pages too (the kernel-stack area).
        t.map(&mut m, V + 0x5000, 0x5000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.unmap(&mut m, V + 0x5000), Some(0x5000));
        assert_eq!(t.translate(&mut m, V + 0x5000), None);
        // Not a 4 KiB page: left alone.
        t.map(&mut m, V + HUGE, HUGE, HUGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.unmap(&mut m, V + HUGE), None);
        assert!(t.translate(&mut m, V + HUGE).is_some());
    }

    #[test]
    fn freeing_the_lower_half_gives_back_every_frame() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        let kernel = m.tables.len();
        // Pages spread over several tables of each level: two PML4 entries,
        // two PDPT entries, two PDs, several PTs.
        let spots = [
            U,
            U + PAGE,
            U + HUGE,
            U + (1 << 30),
            0x7FFF_FFFF_E000,
            0x7FFF_FFF0_0000,
        ];
        for v in spots {
            let frame = m.alloc_table().unwrap();
            t.map_user(&mut m, v, frame, Perm::ReadWrite).unwrap();
        }
        assert!(m.tables.len() > kernel + spots.len() + 4);
        t.free_lower_half(&mut m);
        assert_eq!(m.tables.len(), kernel, "only the kernel's tables are left");
        assert!(m.table(t.pml4)[..256].iter().all(|&e| e == 0));
        assert_eq!(
            t.translate(&mut m, V + 0x1000).map(|(p, ..)| p),
            Some(0x1000),
            "the kernel half is untouched"
        );
        for v in spots {
            assert_eq!(t.user_page(&mut m, v), None);
        }
        // An empty lower half frees nothing.
        t.free_lower_half(&mut m);
        assert_eq!(m.tables.len(), kernel);
    }

    #[test]
    fn a_frame_s_bytes_are_its_table_s() {
        let (mut m, _) = setup();
        let f = m.alloc_table().unwrap();
        m.bytes(f)[8..16].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(m.table(f)[1], 0x1122_3344_5566_7788);
        assert_eq!(m.bytes(f).len(), 4096);
    }

    #[test]
    fn linear_ranges_merge_touching_ram_and_skip_holes() {
        use MemoryKind::*;
        let r = |start: u64, end: u64, kind| MemoryRegion {
            start,
            len: end - start,
            kind,
        };
        let map = [
            r(0x0, 0x9e000, Usable),
            r(0x9e000, 0x9f000, Reserved),
            r(0x9f000, 0xa0000, Usable),
            r(0x100000, 0x4000000, Usable),
            r(0x4000000, 0x4800000, Bootloader),
            r(0x4800000, 0x4dd33000, Usable),
            r(0x4dd33000, 0x56c5f000, Reserved),
            r(0x56c5f000, 0x56d48000, AcpiReclaimable),
            r(0x56d48000, 0x56ece000, AcpiNvs),
            r(0xc0000000, 0xd0000000, Reserved),
            r(0xfee00000, 0xfee01000, Mmio),
            r(0x100000000, 0x497800000, Usable),
        ];
        let got: Vec<_> = linear_ranges(&map).collect();
        assert_eq!(
            got,
            vec![
                (0x0, 0x9e000),
                (0x9f000, 0xa0000),
                (0x100000, 0x4dd33000),
                (0x56c5f000, 0x56ece000),
                (0x100000000, 0x497800000),
            ]
        );
    }
}
