//! Rename (spec §8.2), with the contract's checks in the contract's order.
//! A target that exists has its entry pointed at the moved inode in
//! place, which needs no space; otherwise the new entry is added first
//! (the only step that can fail with `ENOSPC`, before anything changed)
//! and the old one removed after.

use crate::Ext2;
use crate::dirent;
use crate::ops::LINK_MAX;
use crate::superblock::ROOT_INO;
use vfs::{BlockDevice, Errno, FileType, Ino, path};

/// The deepest directory rename walks up from: far more than any real
/// tree, and quick to walk even around a `..` loop.
const MAX_DEPTH: u32 = 4096;

impl<D: BlockDevice> Ext2<D> {
    /// Whether directory `ancestor` is `dir` or above it, walking `..`
    /// up to the root. A walk longer than the directories in use, or than
    /// [`MAX_DEPTH`], is a `..` loop on a corrupt disk.
    fn is_ancestor(&mut self, ancestor: u32, dir: Ino) -> Result<bool, Errno> {
        let used = self.geo.inodes_count - self.sb.free_inodes_count().min(self.geo.inodes_count);
        let limit = used.min(MAX_DEPTH);
        let mut here = dir;
        for _ in 0..=limit {
            if here == ancestor as Ino {
                return Ok(true);
            }
            if here == ROOT_INO as Ino {
                return Ok(false);
            }
            let inode = self.dir_inode(here)?;
            let Some(parent) = self.find_entry(&inode, b"..")? else {
                return Err(self.corrupt(format_args!("directory {here}: no ..")));
            };
            here = parent.entry.inode as Ino;
        }
        Err(self.corrupt(format_args!(
            "directory {dir}: no root within {limit} levels of .."
        )))
    }

    pub(crate) fn move_entry(
        &mut self,
        from_dir: Ino,
        from: &[u8],
        to_dir: Ino,
        to: &[u8],
    ) -> Result<(), Errno> {
        let src_parent = self.inode(from_dir)?;
        let mut dst_parent = self.inode(to_dir)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        path::check_name(from)?;
        path::check_name(to)?;
        for dir in [&src_parent, &dst_parent] {
            if dir.kind() != Some(FileType::Directory) {
                return Err(Errno::ENOTDIR);
            }
        }
        let found = self.find_entry(&src_parent, from)?.ok_or(Errno::ENOENT)?;
        let src = self.entry_inode(&src_parent, &found)?;
        let target = self.find_entry(&dst_parent, to)?;
        if target.as_ref().is_some_and(|t| t.entry.inode == src.ino) {
            return Ok(());
        }
        let kind = src.kind().expect("in use");
        let src_is_dir = kind == FileType::Directory;
        if src_is_dir && self.is_ancestor(src.ino, to_dir)? {
            return Err(Errno::EINVAL);
        }
        let replaced = match &target {
            Some(t) => {
                let inode = self.entry_inode(&dst_parent, t)?;
                let is_dir = inode.kind() == Some(FileType::Directory);
                if src_is_dir && !is_dir {
                    return Err(Errno::ENOTDIR);
                }
                if !src_is_dir && is_dir {
                    return Err(Errno::EISDIR);
                }
                if is_dir && !self.dir_is_empty(&inode)? {
                    return Err(Errno::ENOTEMPTY);
                }
                Some(inode)
            }
            None => None,
        };
        let moves_dir = src_is_dir && from_dir != to_dir;
        if moves_dir && replaced.is_none() && dst_parent.links() >= LINK_MAX {
            return Err(Errno::ENOSPC);
        }

        // The new name.
        match (&target, replaced) {
            (Some(t), Some(mut old)) => {
                let file_type = self.geo.filetype.then(|| dirent::type_byte(kind));
                let block = self.cache.write(t.block as u64)?;
                dirent::retarget(block, t.entry, src.ino, file_type);
                self.changed_dir(&mut dst_parent);
                if old.kind() == Some(FileType::Directory) {
                    dst_parent.set_links(dst_parent.links().saturating_sub(1));
                    self.write_inode(&dst_parent)?;
                    old.set_ctime(self.now());
                    self.release_inode(&mut old)?;
                } else {
                    self.write_inode(&dst_parent)?;
                    self.drop_link(&mut old)?;
                }
            }
            _ => {
                self.add_entry(&mut dst_parent, to, src.ino, kind)?;
                self.write_inode(&dst_parent)?;
            }
        }
        // The old name goes. Inodes are read again from here on: the two
        // parents may be one directory.
        let mut src_parent = self.inode(from_dir)?;
        let Some(found) = self.find_entry(&src_parent, from)? else {
            return Err(self.corrupt(format_args!("directory {from_dir}: entry vanished")));
        };
        self.remove_entry(&mut src_parent, &found)?;
        if moves_dir {
            src_parent.set_links(src_parent.links().saturating_sub(1));
        }
        self.write_inode(&src_parent)?;
        let mut src = self.inode(src.ino as Ino)?;
        if moves_dir {
            let mut dst_parent = self.inode(to_dir)?;
            dst_parent.set_links(dst_parent.links() + 1);
            self.write_inode(&dst_parent)?;
            let Some(dotdot) = self.find_entry(&src, b"..")? else {
                return Err(self.corrupt(format_args!("directory {}: no ..", src.ino)));
            };
            let file_type = self
                .geo
                .filetype
                .then(|| dirent::type_byte(FileType::Directory));
            let block = self.cache.write(dotdot.block as u64)?;
            dirent::retarget(block, dotdot.entry, to_dir as u32, file_type);
        }
        src.set_ctime(self.now());
        self.write_inode(&src)
    }
}
