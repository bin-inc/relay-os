//! The allocator (spec §8.2): blocks and inodes through the group bitmaps.
//! Every change updates the free counts in the group descriptor and the
//! superblock.

use crate::Ext2;
use crate::bitmap;
use vfs::{BlockDevice, Errno};

impl<D: BlockDevice> Ext2<D> {
    /// The group holding block `block` and its index there.
    fn block_group(&self, block: u32) -> (u32, usize) {
        let rel = block - self.geo.first_data_block;
        (
            rel / self.geo.blocks_per_group,
            (rel % self.geo.blocks_per_group) as usize,
        )
    }

    fn count_free_blocks(&mut self, g: u32, delta: i32) {
        let free = self
            .groups
            .free_blocks(g)
            .saturating_add_signed(delta as i16);
        self.groups.set_free_blocks(g, free);
        let total = self.sb.free_blocks_count().saturating_add_signed(delta);
        self.sb.set_free_blocks_count(total);
    }

    /// Allocates a block near `goal`: from the goal on in its group
    /// (wrapping), then in the following groups. Root is the only user, so
    /// reserved blocks count as free: `ENOSPC` only when none is left.
    pub(crate) fn alloc_block(&mut self, goal: u64) -> Result<u32, Errno> {
        if self.sb.free_blocks_count() == 0 {
            return Err(Errno::ENOSPC);
        }
        let goal = goal.clamp(
            self.geo.first_data_block as u64,
            self.geo.blocks_count as u64 - 1,
        ) as u32;
        let (first, index) = self.block_group(goal);
        for step in 0..self.geo.groups {
            let g = (first + step) % self.geo.groups;
            if self.groups.free_blocks(g) == 0 {
                continue;
            }
            let from = if step == 0 { index } else { 0 };
            let len = self.geo.group_blocks(g) as usize;
            let bitmap = self.groups.block_bitmap(g) as u64;
            let Some(i) = bitmap::find_zero(self.cache.read(bitmap)?, len, from) else {
                continue;
            };
            let block = self.geo.group_start(g) as u32 + i as u32;
            if self.groups.is_metadata(&self.geo, block) {
                return Err(self.corrupt(format_args!(
                    "group {g}: block bitmap offers metadata block {block}"
                )));
            }
            bitmap::set(self.cache.write(bitmap)?, i);
            self.count_free_blocks(g, -1);
            return Ok(block);
        }
        Err(Errno::ENOSPC)
    }

    /// Frees a block. A block outside the filesystem, in its metadata or
    /// already free is logged and left alone: the pointer to it was corrupt.
    pub(crate) fn free_block(&mut self, block: u32) -> Result<(), Errno> {
        if self.bad_block(block) {
            self.log(format_args!(
                "not freeing block {block}: outside the filesystem or metadata"
            ));
            return Ok(());
        }
        let (g, i) = self.block_group(block);
        let bitmap = self.groups.block_bitmap(g) as u64;
        if !bitmap::test(self.cache.read(bitmap)?, i) {
            self.log(format_args!("freeing free block {block}"));
            return Ok(());
        }
        bitmap::clear(self.cache.write(bitmap)?, i);
        self.count_free_blocks(g, 1);
        Ok(())
    }

    fn count_free_inodes(&mut self, g: u32, delta: i32, dir: bool) {
        let free = self
            .groups
            .free_inodes(g)
            .saturating_add_signed(delta as i16);
        self.groups.set_free_inodes(g, free);
        if dir {
            let dirs = self
                .groups
                .used_dirs(g)
                .saturating_add_signed(-delta as i16);
            self.groups.set_used_dirs(g, dirs);
        }
        let total = self.sb.free_inodes_count().saturating_add_signed(delta);
        self.sb.set_free_inodes_count(total);
    }

    /// Allocates an inode: a directory's in the group with the most free
    /// inodes (the lowest of equals), anything else's in `parent`'s group,
    /// else the next group with one free. Never below `s_first_ino`.
    pub(crate) fn alloc_inode(&mut self, parent: u32, dir: bool) -> Result<u32, Errno> {
        if self.sb.free_inodes_count() == 0 {
            return Err(Errno::ENOSPC);
        }
        let ipg = self.geo.inodes_per_group;
        let first = if dir {
            let most = (0..self.geo.groups)
                .map(|g| self.groups.free_inodes(g))
                .max();
            (0..self.geo.groups)
                .find(|&g| Some(self.groups.free_inodes(g)) == most)
                .unwrap_or(0)
        } else {
            (parent - 1) / ipg
        };
        for step in 0..self.geo.groups {
            let g = (first + step) % self.geo.groups;
            if self.groups.free_inodes(g) == 0 {
                continue;
            }
            let from = (self.geo.first_ino - 1).saturating_sub(g * ipg) as usize;
            let bitmap = self.groups.inode_bitmap(g) as u64;
            let Some(i) = bitmap::first_zero(self.cache.read(bitmap)?, from, ipg as usize) else {
                continue;
            };
            bitmap::set(self.cache.write(bitmap)?, i);
            self.count_free_inodes(g, -1, dir);
            return Ok(g * ipg + i as u32 + 1);
        }
        Err(Errno::ENOSPC)
    }

    /// Frees inode number `ino` in its bitmap. One already free is logged
    /// and left alone.
    pub(crate) fn free_inode(&mut self, ino: u32, dir: bool) -> Result<(), Errno> {
        let ipg = self.geo.inodes_per_group;
        let (g, i) = ((ino - 1) / ipg, ((ino - 1) % ipg) as usize);
        let bitmap = self.groups.inode_bitmap(g) as u64;
        if !bitmap::test(self.cache.read(bitmap)?, i) {
            self.log(format_args!("freeing free inode {ino}"));
            return Ok(());
        }
        bitmap::clear(self.cache.write(bitmap)?, i);
        self.count_free_inodes(g, 1, dir);
        Ok(())
    }
}
