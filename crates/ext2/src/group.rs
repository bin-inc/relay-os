//! Block group descriptors (spec §8.2). The whole table is kept in memory
//! as raw bytes, so fields this driver does not know survive a rewrite.

use crate::le::{set_u16, u16_at, u32_at};
use crate::superblock::{DESC_SIZE, Geometry};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const BLOCK_BITMAP: usize = 0;
const INODE_BITMAP: usize = 4;
const INODE_TABLE: usize = 8;
const FREE_BLOCKS: usize = 12;
const FREE_INODES: usize = 14;
const USED_DIRS: usize = 16;

/// The group descriptor table, with a note of whether it changed since it
/// was last written.
pub struct Groups {
    raw: Vec<u8>,
    count: u32,
    dirty: bool,
}

impl Groups {
    /// The table in `raw` (whole blocks) for `count` groups.
    pub fn new(raw: Vec<u8>, count: u32) -> Groups {
        assert!(raw.len() >= count as usize * DESC_SIZE as usize);
        Groups {
            raw,
            count,
            dirty: false,
        }
    }

    /// The table as stored: whole blocks.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_clean(&mut self) {
        self.dirty = false;
    }

    fn set16(&mut self, g: u32, field: usize, v: u16) {
        let at = self.at(g, field);
        set_u16(&mut self.raw, at, v);
        self.dirty = true;
    }

    fn at(&self, g: u32, field: usize) -> usize {
        debug_assert!(g < self.count);
        g as usize * DESC_SIZE as usize + field
    }

    pub fn block_bitmap(&self, g: u32) -> u32 {
        u32_at(&self.raw, self.at(g, BLOCK_BITMAP))
    }

    pub fn inode_bitmap(&self, g: u32) -> u32 {
        u32_at(&self.raw, self.at(g, INODE_BITMAP))
    }

    pub fn inode_table(&self, g: u32) -> u32 {
        u32_at(&self.raw, self.at(g, INODE_TABLE))
    }

    /// Group `g`'s metadata as `(first block, blocks, what)`: its
    /// superblock and descriptor copy (none: 0 blocks), bitmaps and inode
    /// table.
    fn metadata(&self, geo: &Geometry, g: u32) -> [(u64, u64, &'static str); 4] {
        [
            (
                geo.group_start(g),
                geo.super_blocks(g) as u64,
                "superblock copy",
            ),
            (self.block_bitmap(g) as u64, 1, "block bitmap"),
            (self.inode_bitmap(g) as u64, 1, "inode bitmap"),
            (
                self.inode_table(g) as u64,
                geo.inode_table_blocks as u64,
                "inode table",
            ),
        ]
    }

    /// Whether `block` holds metadata: a superblock or descriptor copy
    /// (reserved descriptor blocks included), a bitmap or an inode table,
    /// or lies before the first group. No data or indirect block does, so a
    /// block pointer to one is corrupt.
    pub fn is_metadata(&self, geo: &Geometry, block: u32) -> bool {
        let Some(rel) = block.checked_sub(geo.first_data_block) else {
            return true;
        };
        let g = rel / geo.blocks_per_group;
        g < self.count
            && self
                .metadata(geo, g)
                .iter()
                .any(|&(first, len, _)| (first..first + len).contains(&(block as u64)))
    }

    pub fn free_blocks(&self, g: u32) -> u16 {
        u16_at(&self.raw, self.at(g, FREE_BLOCKS))
    }

    pub fn set_free_blocks(&mut self, g: u32, n: u16) {
        self.set16(g, FREE_BLOCKS, n);
    }

    pub fn free_inodes(&self, g: u32) -> u16 {
        u16_at(&self.raw, self.at(g, FREE_INODES))
    }

    pub fn set_free_inodes(&mut self, g: u32, n: u16) {
        self.set16(g, FREE_INODES, n);
    }

    pub fn used_dirs(&self, g: u32) -> u16 {
        u16_at(&self.raw, self.at(g, USED_DIRS))
    }

    pub fn set_used_dirs(&mut self, g: u32, n: u16) {
        self.set16(g, USED_DIRS, n);
    }
}

/// Checks that every group's superblock copy, bitmaps and inode table lie
/// inside the group, as they do on every ext2 filesystem (no `flex_bg`),
/// and do not overlap: writing one must never overwrite another. The error
/// is the reason, for the log.
pub fn check(groups: &Groups, geo: &Geometry) -> Result<(), String> {
    for g in 0..groups.count {
        let start = geo.group_start(g);
        let end = start + geo.group_blocks(g) as u64;
        let parts = groups.metadata(geo, g);
        for &(first, len, what) in &parts {
            if first < start || first + len > end {
                return Err(format!("group {g}: {what} outside the group"));
            }
        }
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                if a.1 > 0 && b.1 > 0 && a.0 < b.0 + b.1 && b.0 < a.0 + a.1 {
                    return Err(format!("group {g}: {} overlaps the {}", a.2, b.2));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two groups of 100 blocks starting at block 1.
    fn geo() -> Geometry {
        Geometry {
            block_size: 1024,
            blocks_count: 181,
            inodes_count: 64,
            first_data_block: 1,
            blocks_per_group: 100,
            inodes_per_group: 32,
            inode_size: 256,
            first_ino: 11,
            groups: 2,
            gdt_blocks: 1,
            reserved_gdt_blocks: 0,
            inode_table_blocks: 8,
            backups: crate::superblock::Backups::Sparse,
            filetype: true,
        }
    }

    fn table(descs: &[[u32; 3]]) -> Groups {
        let mut raw = alloc::vec![0u8; 1024];
        for (g, d) in descs.iter().enumerate() {
            for (i, v) in d.iter().enumerate() {
                let at = g * 32 + i * 4;
                raw[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        Groups::new(raw, descs.len() as u32)
    }

    #[test]
    fn descriptors_are_read_from_the_raw_table() {
        let t = table(&[[3, 4, 5], [103, 104, 105]]);
        assert_eq!(
            (t.block_bitmap(1), t.inode_bitmap(1), t.inode_table(1)),
            (103, 104, 105)
        );
        assert_eq!(check(&t, &geo()), Ok(()));
    }

    #[test]
    fn changed_counts_mark_the_table_dirty() {
        let mut t = table(&[[3, 4, 5], [103, 104, 105]]);
        assert!(!t.is_dirty());
        t.set_free_blocks(1, 77);
        t.set_free_inodes(1, 5);
        t.set_used_dirs(1, 3);
        assert_eq!((t.free_blocks(0), t.free_blocks(1)), (0, 77));
        assert_eq!((t.free_inodes(1), t.used_dirs(1)), (5, 3));
        assert_eq!(&t.raw()[32 + 12..32 + 14], &[77, 0]);
        assert!(t.is_dirty());
        t.set_clean();
        assert!(!t.is_dirty());
    }

    #[test]
    fn metadata_outside_its_group_is_refused() {
        let bad = [
            ([[3, 4, 5], [3, 104, 105]], "group 1: block bitmap"),
            ([[3, 400, 5], [103, 104, 105]], "group 0: inode bitmap"),
            ([[3, 4, 95], [103, 104, 105]], "group 0: inode table"),
            ([[3, 4, 5], [103, 104, 175]], "group 1: inode table"),
        ];
        for (descs, reason) in bad {
            let err = check(&table(&descs), &geo()).unwrap_err();
            assert!(err.starts_with(reason), "{err}");
        }
    }

    #[test]
    fn overlapping_metadata_is_refused() {
        let bad = [
            (
                [[5, 4, 5], [103, 104, 105]],
                "group 0: block bitmap overlaps the inode table",
            ),
            (
                [[3, 3, 5], [103, 104, 105]],
                "group 0: block bitmap overlaps the inode bitmap",
            ),
            (
                [[2, 4, 5], [103, 104, 105]],
                "group 0: superblock copy overlaps the block bitmap",
            ),
            (
                [[3, 4, 5], [103, 104, 102]],
                "group 1: superblock copy overlaps the inode table",
            ),
        ];
        for (descs, reason) in bad {
            assert_eq!(check(&table(&descs), &geo()), Err(reason.into()));
        }
    }

    #[test]
    fn metadata_blocks_are_known() {
        let t = table(&[[3, 4, 5], [103, 104, 105]]);
        let geo = geo();
        let meta: Vec<u32> = (0..181).filter(|&b| t.is_metadata(&geo, b)).collect();
        let mut want = vec![0, 1, 2, 3, 4];
        want.extend(5..13);
        want.extend([101, 102, 103, 104]);
        want.extend(105..113);
        assert_eq!(meta, want);
    }
}
