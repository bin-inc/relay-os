//! File data (spec §8.2): mapping logical blocks through the direct and
//! indirect pointers, and reading files and symlinks.

use crate::Ext2;
use crate::inode::{I_BLOCK_BYTES, Inode, map_path};
use crate::le::u32_at;
use alloc::vec::Vec;
use vfs::{BlockDevice, Errno, FileType, Ino};

impl<D: BlockDevice> Ext2<D> {
    /// Pointers per indirect block.
    pub(crate) fn per_block(&self) -> u64 {
        self.geo.block_size as u64 / 4
    }

    /// Checks a block pointer read from inode `ino` or its indirect blocks:
    /// it must lie inside the filesystem and not in its metadata (no data
    /// or indirect block is ever a superblock or descriptor copy, a bitmap
    /// or an inode table).
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
