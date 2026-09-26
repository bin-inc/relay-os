//! Translating the UEFI memory map into `boot_info::MemoryRegion`s.

use boot_info::{MemoryKind, MemoryRegion};

/// One UEFI memory descriptor, reduced to what we need.
#[derive(Clone, Copy, Debug)]
pub struct RawDesc {
    pub ty: u32,
    pub phys_start: u64,
    pub pages: u64,
}

/// UEFI memory type numbers (UEFI spec 2.10, table 7.10).
pub fn kind_of(ty: u32) -> MemoryKind {
    match ty {
        3 | 4 | 7 => MemoryKind::Usable, // boot services code/data, conventional
        1 | 2 => MemoryKind::Bootloader, // loader code/data
        9 => MemoryKind::AcpiReclaimable,
        10 => MemoryKind::AcpiNvs,
        11 | 12 => MemoryKind::Mmio,
        _ => MemoryKind::Reserved,
    }
}

/// Fills `out` with sorted, merged regions and returns how many were written,
/// or `None` if `out` is too small for the unmerged input.
pub fn translate(descs: impl Iterator<Item = RawDesc>, out: &mut [MemoryRegion]) -> Option<usize> {
    let mut n = 0;
    for d in descs.filter(|d| d.pages > 0) {
        *out.get_mut(n)? = MemoryRegion {
            start: d.phys_start,
            len: d.pages * 4096,
            kind: kind_of(d.ty),
        };
        n += 1;
    }
    out[..n].sort_unstable_by_key(|r| r.start);
    let mut w = 0;
    for r in 0..n {
        let cur = out[r];
        if w > 0 && out[w - 1].kind == cur.kind && out[w - 1].end() == cur.start {
            out[w - 1].len += cur.len;
        } else {
            out[w] = cur;
            w += 1;
        }
    }
    Some(w)
}

/// End of the physical range the loader maps linearly: the highest address
/// of any RAM-type descriptor. Reserved and MMIO ranges above that (OVMF
/// reports one near 1 TiB) are mapped on demand by the kernel.
pub fn linear_map_end(descs: impl Iterator<Item = RawDesc>) -> u64 {
    descs
        .filter(|d| {
            matches!(
                kind_of(d.ty),
                MemoryKind::Usable
                    | MemoryKind::Bootloader
                    | MemoryKind::Kernel
                    | MemoryKind::AcpiReclaimable
                    | MemoryKind::AcpiNvs
            )
        })
        .map(|d| d.phys_start + d.pages * 4096)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(ty: u32, start: u64, pages: u64) -> RawDesc {
        RawDesc {
            ty,
            phys_start: start,
            pages,
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(7), MemoryKind::Usable);
        assert_eq!(kind_of(4), MemoryKind::Usable);
        assert_eq!(kind_of(2), MemoryKind::Bootloader);
        assert_eq!(
            kind_of(0x8000_5245),
            MemoryKind::Reserved,
            "OS-defined types"
        );
        assert_eq!(kind_of(5), MemoryKind::Reserved);
        assert_eq!(kind_of(11), MemoryKind::Mmio);
    }

    #[test]
    fn sorts_and_merges_adjacent_same_kind() {
        let mut out = [MemoryRegion {
            start: 0,
            len: 0,
            kind: MemoryKind::Reserved,
        }; 8];
        let input = [
            d(7, 0x2000, 1),
            d(3, 0x1000, 1),
            d(2, 0x3000, 2),
            d(7, 0x5000, 1),
            d(7, 0x9000, 1),
        ];
        let n = translate(input.into_iter(), &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                MemoryRegion {
                    start: 0x1000,
                    len: 0x2000,
                    kind: MemoryKind::Usable
                },
                MemoryRegion {
                    start: 0x3000,
                    len: 0x2000,
                    kind: MemoryKind::Bootloader
                },
                MemoryRegion {
                    start: 0x5000,
                    len: 0x1000,
                    kind: MemoryKind::Usable
                },
                MemoryRegion {
                    start: 0x9000,
                    len: 0x1000,
                    kind: MemoryKind::Usable
                },
            ]
        );
    }

    #[test]
    fn too_small_output_is_reported() {
        let mut out = [MemoryRegion {
            start: 0,
            len: 0,
            kind: MemoryKind::Reserved,
        }; 1];
        assert_eq!(
            translate([d(7, 0, 1), d(2, 0x1000, 1)].into_iter(), &mut out),
            None
        );
    }

    #[test]
    fn linear_end_covers_ram_kinds_only() {
        let input = [
            d(7, 0, 16),
            d(10, 0x7F00_0000, 1),         // ACPI NVS counts
            d(11, 0xFE00_0000, 1),         // MMIO does not
            d(0, 0xFD_0000_0000, 0x10000), // high reserved window does not
        ];
        assert_eq!(linear_map_end(input.into_iter()), 0x7F00_1000);
    }
}
