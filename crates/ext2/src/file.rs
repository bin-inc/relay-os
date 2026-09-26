//! File data (spec §8.2): mapping logical blocks through the direct and
//! indirect pointers, reading files and symlinks, writing (allocating data
//! and indirect blocks) and truncating (freeing them).

use crate::Ext2;
use crate::inode::{I_BLOCK_BYTES, Inode, N_DIRECT, map_path, max_file_size};
use crate::le::{set_u32, u32_at};
use crate::superblock::RO_COMPAT_LARGE_FILE;
use alloc::vec::Vec;
use vfs::{BlockDevice, Errno, FileType, Ino};

/// Files past this size need the `large_file` feature.
const LARGE_FILE: u64 = 0x7FFF_FFFF;

impl<D: BlockDevice> Ext2<D> {
    /// Pointers per indirect block.
    pub(crate) fn per_block(&self) -> u64 {
        self.geo.block_size as u64 / 4
    }

    /// Whether a block pointer is impossible: outside the filesystem, or
    /// into its metadata (no data or indirect block is ever a superblock or
    /// descriptor copy, a bitmap or an inode table).
    pub(crate) fn bad_block(&self, block: u32) -> bool {
        block >= self.geo.blocks_count || self.groups.is_metadata(&self.geo, block)
    }

    /// Checks a block pointer read from inode `ino` or its indirect blocks.
    pub(crate) fn check_block(&self, ino: u32, block: u32) -> Result<(), Errno> {
        if block >= self.geo.blocks_count {
            return Err(self.corrupt(format_args!(
                "inode {ino}: block pointer {block} outside the filesystem"
            )));
        }
        if self.groups.is_metadata(&self.geo, block) {
            return Err(self.corrupt(format_args!(
                "inode {ino}: block pointer {block} into metadata"
            )));
        }
        Ok(())
    }

    /// The physical block holding logical block `lb` of `inode`; `None` for
    /// a hole.
    pub(crate) fn bmap(&mut self, inode: &Inode, lb: u64) -> Result<Option<u32>, Errno> {
        let Some(path) = map_path(lb, self.per_block()) else {
            return Ok(None);
        };
        let mut ptr = inode.block(path.index[0]);
        for &index in &path.index[1..=path.depth] {
            if ptr == 0 {
                return Ok(None);
            }
            self.check_block(inode.ino, ptr)?;
            ptr = u32_at(self.cache.read(ptr as u64)?, index * 4);
        }
        if ptr == 0 {
            return Ok(None);
        }
        self.check_block(inode.ino, ptr)?;
        Ok(Some(ptr))
    }

    /// The physical block for logical block `lb` of `inode`, allocating it
    /// and the indirect blocks leading to it if missing. New blocks are
    /// zeroed in the cache, never read. The free count is checked first,
    /// so a full filesystem gives `ENOSPC` without a half-built path.
    pub(crate) fn bmap_alloc(&mut self, inode: &mut Inode, lb: u64) -> Result<u32, Errno> {
        let path = map_path(lb, self.per_block()).ok_or(Errno::EFBIG)?;
        // ptrs[level]: the pointer at that level, 0 where missing.
        let mut ptrs = [0u32; 4];
        ptrs[0] = inode.block(path.index[0]);
        let mut level = 0;
        while ptrs[level] != 0 && level < path.depth {
            self.check_block(inode.ino, ptrs[level])?;
            let block = self.cache.read(ptrs[level] as u64)?;
            ptrs[level + 1] = u32_at(block, path.index[level + 1] * 4);
            level += 1;
        }
        if ptrs[level] != 0 {
            self.check_block(inode.ino, ptrs[level])?;
            return Ok(ptrs[level]);
        }
        let missing = (path.depth - level + 1) as u32;
        if self.sb.free_blocks_count() < missing {
            return Err(Errno::ENOSPC);
        }
        let mut goal = self.block_goal(inode, lb)?;
        for l in level..=path.depth {
            let block = self.alloc_block(goal)?;
            goal = block as u64 + 1;
            self.cache.zeroed(block as u64)?;
            if l == 0 {
                inode.set_block(path.index[0], block);
            } else {
                let parent = self.cache.write(ptrs[l - 1] as u64)?;
                set_u32(parent, path.index[l] * 4, block);
            }
            inode.add_blocks(self.geo.block_size, 1);
            ptrs[l] = block;
        }
        Ok(ptrs[path.depth])
    }

    /// Where a new block for logical block `lb` should go: after the block
    /// before it, or at the start of the inode's group (spec §8.2).
    fn block_goal(&mut self, inode: &Inode, lb: u64) -> Result<u64, Errno> {
        if lb > 0
            && let Some(prev) = self.bmap(inode, lb - 1)?
        {
            return Ok(prev as u64 + 1);
        }
        let group = (inode.ino - 1) / self.geo.inodes_per_group;
        Ok(self.geo.group_start(group))
    }

    /// Reads `buf.len()` bytes of `inode`'s data from `offset`, holes as
    /// zeros; the caller keeps it inside the file.
    fn read_data(&mut self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        let bs = self.geo.block_size;
        let mut done = 0;
        while done < buf.len() {
            let pos = offset + done as u64;
            let within = (pos % bs as u64) as usize;
            let len = (buf.len() - done).min(bs - within);
            let dst = &mut buf[done..done + len];
            match self.bmap(inode, pos / bs as u64)? {
                Some(block) => {
                    dst.copy_from_slice(&self.cache.read(block as u64)?[within..][..len])
                }
                None => dst.fill(0),
            }
            done += len;
        }
        Ok(())
    }

    pub(crate) fn file_read(
        &mut self,
        ino: Ino,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, Errno> {
        let inode = self.inode(ino)?;
        match inode.kind() {
            Some(FileType::Regular) => {}
            Some(FileType::Directory) => return Err(Errno::EISDIR),
            _ => return Err(Errno::EINVAL),
        }
        let size = inode.size();
        if offset >= size {
            return Ok(0);
        }
        let n = (buf.len() as u64).min(size - offset) as usize;
        self.read_data(&inode, offset, &mut buf[..n])?;
        Ok(n)
    }

    /// Regular file `ino` for a change to its data: rules 1 and 2 of the
    /// contract (`ENOENT`, `EROFS`), then `EISDIR` or `EINVAL`.
    fn file_for_change(&mut self, ino: Ino) -> Result<Inode, Errno> {
        let inode = self.inode(ino)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        match inode.kind() {
            Some(FileType::Regular) => Ok(inode),
            Some(FileType::Directory) => Err(Errno::EISDIR),
            _ => Err(Errno::EINVAL),
        }
    }

    /// Zeroes the rest of the block holding byte `size`, so that data past
    /// the end of the file never reappears when it grows.
    fn zero_tail(&mut self, inode: &Inode, size: u64) -> Result<(), Errno> {
        let bs = self.geo.block_size as u64;
        if size.is_multiple_of(bs) {
            return Ok(());
        }
        if let Some(block) = self.bmap(inode, size / bs)? {
            self.cache.write(block as u64)?[(size % bs) as usize..].fill(0);
        }
        Ok(())
    }

    /// Sets the size, turning `large_file` on for sizes past 2 GiB as Linux
    /// does (e2fsck insists).
    fn set_file_size(&mut self, inode: &mut Inode, size: u64) {
        inode.set_size(size);
        let features = self.sb.feature_ro_compat();
        if size > LARGE_FILE && features & RO_COMPAT_LARGE_FILE == 0 {
            self.sb
                .set_feature_ro_compat(features | RO_COMPAT_LARGE_FILE);
        }
    }

    /// Writes `buf` at `offset`, allocating blocks as needed. Stops at the
    /// first error; returns how much was written and that error.
    fn write_data(
        &mut self,
        inode: &mut Inode,
        offset: u64,
        buf: &[u8],
    ) -> (usize, Result<(), Errno>) {
        let bs = self.geo.block_size;
        let mut done = 0;
        while done < buf.len() {
            let pos = offset + done as u64;
            let within = (pos % bs as u64) as usize;
            let len = (buf.len() - done).min(bs - within);
            let block = match self.bmap_alloc(inode, pos / bs as u64) {
                Ok(block) => block as u64,
                Err(e) => return (done, Err(e)),
            };
            // A whole block is overwritten without reading it first.
            let data = if len == bs {
                self.cache.zeroed(block)
            } else {
                self.cache.write(block)
            };
            match data {
                Ok(data) => data[within..within + len].copy_from_slice(&buf[done..done + len]),
                Err(e) => return (done, Err(e)),
            }
            done += len;
        }
        (done, Ok(()))
    }

    pub(crate) fn file_write(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        let mut inode = self.file_for_change(ino)?;
        if buf.is_empty() {
            return Ok(0);
        }
        match offset.checked_add(buf.len() as u64) {
            Some(end) if end <= max_file_size(self.geo.block_size) => {}
            _ => return Err(Errno::EFBIG),
        }
        let before = inode.clone();
        let size = inode.size();
        if offset > size {
            self.zero_tail(&inode, size)?;
        }
        let (done, result) = self.write_data(&mut inode, offset, buf);
        if done > 0 {
            self.set_file_size(&mut inode, size.max(offset + done as u64));
            inode.touch(self.now());
        }
        // Blocks may have been allocated even when nothing was written.
        if inode.raw() != before.raw() {
            self.write_inode(&inode)?;
        }
        match result {
            Err(Errno::ENOSPC) if done > 0 => Ok(done),
            Err(e) => Err(e),
            Ok(()) => Ok(done),
        }
    }

    pub(crate) fn file_truncate(&mut self, ino: Ino, size: u64) -> Result<(), Errno> {
        let mut inode = self.file_for_change(ino)?;
        if size > max_file_size(self.geo.block_size) {
            return Err(Errno::EFBIG);
        }
        let old = inode.size();
        let mut result = Ok(());
        if size > old {
            self.zero_tail(&inode, old)?;
        } else if size < old {
            self.zero_tail(&inode, size)?;
            let first = size.div_ceil(self.geo.block_size as u64);
            result = self.free_from(&mut inode, first);
        }
        // After a failure part of the space may be freed: keep the old size,
        // so holes stand where blocks were, and save what did change.
        if result.is_ok() {
            self.set_file_size(&mut inode, size);
        }
        inode.touch(self.now());
        self.write_inode(&inode)?;
        result
    }

    pub(crate) fn inode_touch(&mut self, ino: Ino) -> Result<(), Errno> {
        let mut inode = self.inode(ino)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        inode.touch(self.now());
        self.write_inode(&inode)
    }

    /// Frees every data block of `inode` from logical block `first` on,
    /// and the indirect blocks left empty.
    pub(crate) fn free_from(&mut self, inode: &mut Inode, first: u64) -> Result<(), Errno> {
        for i in (first.min(N_DIRECT as u64) as usize)..N_DIRECT {
            let block = inode.block(i);
            if block != 0 {
                self.free_block(block)?;
                inode.set_block(i, 0);
                inode.add_blocks(self.geo.block_size, -1);
            }
        }
        let p = self.per_block();
        let mut start = N_DIRECT as u64;
        let mut span = 1;
        for depth in 1..=3 {
            span *= p;
            let slot = N_DIRECT + depth - 1;
            let root = inode.block(slot);
            if root != 0 && first < start + span {
                let from = first.saturating_sub(start);
                if self.free_tree(inode, root, depth, from)? {
                    inode.set_block(slot, 0);
                }
            }
            start += span;
        }
        Ok(())
    }

    /// Frees the entries from index `from` on of the indirect block
    /// `block` at `depth` (1: it points at data), and the block itself if
    /// it ends up empty. Returns whether it did. A freed entry is cleared
    /// at once, so an error midway leaves nothing pointing at free blocks;
    /// an impossible pointer (outside the filesystem, or into metadata) is
    /// logged and dropped, never followed.
    fn free_tree(
        &mut self,
        inode: &mut Inode,
        block: u32,
        depth: usize,
        from: u64,
    ) -> Result<bool, Errno> {
        if self.bad_block(block) {
            self.log(format_args!(
                "inode {}: dropping block pointer {block}: outside the filesystem or metadata",
                inode.ino
            ));
            return Ok(true);
        }
        let span = self.per_block().pow(depth as u32 - 1);
        let ptrs: Vec<u32> = self
            .cache
            .read(block as u64)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        let first = (from / span) as usize;
        let mut kept = ptrs[..first.min(ptrs.len())].iter().any(|&p| p != 0);
        for (i, &ptr) in ptrs.iter().enumerate().skip(first) {
            if ptr == 0 {
                continue;
            }
            let freed = if depth == 1 {
                self.free_block(ptr)?;
                inode.add_blocks(self.geo.block_size, -1);
                true
            } else {
                let sub = if i == first { from % span } else { 0 };
                self.free_tree(inode, ptr, depth - 1, sub)?
            };
            if freed {
                set_u32(self.cache.write(block as u64)?, i * 4, 0);
            } else {
                kept = true;
            }
        }
        if kept {
            return Ok(false);
        }
        self.free_block(block)?;
        inode.add_blocks(self.geo.block_size, -1);
        Ok(true)
    }

    pub(crate) fn link_target(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        let inode = self.inode(ino)?;
        if inode.kind() != Some(FileType::Symlink) {
            return Err(Errno::EINVAL);
        }
        // Linux keeps targets under 60 bytes in `i_block` and longer ones,
        // up to a block, in the first data block.
        let size = inode.size() as usize;
        if inode.is_fast_symlink(self.geo.block_size) {
            if size == 0 || size >= I_BLOCK_BYTES {
                return Err(self.corrupt(format_args!("inode {ino}: bad symlink size {size}")));
            }
            return Ok(inode.block_bytes()[..size].to_vec());
        }
        if size == 0 || size > self.geo.block_size {
            return Err(self.corrupt(format_args!("inode {ino}: bad symlink size {size}")));
        }
        let Some(block) = self.bmap(&inode, 0)? else {
            return Err(self.corrupt(format_args!("inode {ino}: symlink without a block")));
        };
        Ok(self.cache.read(block as u64)?[..size].to_vec())
    }
}
