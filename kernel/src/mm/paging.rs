//! Four-level x86_64 page tables (spec §5.3). The logic reaches physical
//! memory only through `PhysMem`, so it is tested on the host against a fake;
//! the kernel's implementation goes through the linear map.
//!
//! Every mapping made here is writable and not executable (the kernel image
//! keeps the loader's mappings). Each physical page appears at most once in
//! the linear map, at `PHYS_OFFSET + phys`, so there are never two mappings
//! of one page with different cache types.

use boot_info::{MemoryKind, MemoryRegion};

pub const PAGE: u64 = 4096;
pub const HUGE: u64 = 2 << 20;

const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const ACCESSED: u64 = 1 << 5;
const DIRTY: u64 = 1 << 6;
const HUGE_PAGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDR: u64 = 0x000F_FFFF_FFFF_F000;
const HUGE_ADDR: u64 = 0x000F_FFFF_FFE0_0000;

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
    /// A zeroed frame for a new table, or `None` when memory is exhausted.
    fn alloc_table(&mut self) -> Option<u64>;
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
}

impl core::fmt::Display for MapError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            MapError::OutOfMemory => write!(f, "out of memory for page tables"),
            MapError::Conflict { virt } => write!(f, "{virt:#x} is already mapped differently"),
            MapError::Unaligned => write!(f, "range is not page-aligned"),
            MapError::OutOfRange => write!(f, "physical address beyond the linear map"),
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
    fn child(
        &mut self,
        mem: &mut impl PhysMem,
        table: u64,
        i: usize,
        virt: u64,
    ) -> Result<u64, MapError> {
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            mem.table(table)[i] = t | PRESENT | WRITABLE;
            Ok(t)
        } else if e & HUGE_PAGE != 0 {
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

    /// The physical address, cache type and page size `virt` maps to.
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
    use std::collections::HashMap;

    /// Page tables in host memory, with made-up physical addresses.
    struct FakeMem {
        tables: HashMap<u64, Box<[u64; 512]>>,
        next: u64,
        limit: usize,
    }

    impl FakeMem {
        fn new() -> FakeMem {
            FakeMem {
                tables: HashMap::new(),
                next: 0x1000_0000,
                limit: usize::MAX,
            }
        }
    }

    impl PhysMem for FakeMem {
        fn table(&mut self, phys: u64) -> &mut [u64; 512] {
            self.tables.get_mut(&phys).expect("not a table frame")
        }
        fn alloc_table(&mut self) -> Option<u64> {
            if self.tables.len() >= self.limit {
                return None;
            }
            let p = self.next;
            self.next += PAGE;
            self.tables.insert(p, Box::new([0; 512]));
            Some(p)
        }
    }

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
