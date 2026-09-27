//! Namespace changes (spec §8.2): creating and removing files and
//! directories.

use crate::Ext2;
use crate::dirent;
use crate::inode::Inode;
use vfs::{BlockDevice, Errno, FileType, Ino, path};

/// Linux's ext2 limit on a directory's links, so subdirectories.
pub(crate) const LINK_MAX: u16 = 32000;

impl<D: BlockDevice> Ext2<D> {
    /// Rules 1–4 of the contract for a change to `dir/name`: `ENOENT`,
    /// `EROFS`, the name's error, `ENOTDIR`.
    pub(crate) fn dir_for_change(&mut self, dir: Ino, name: &[u8]) -> Result<Inode, Errno> {
        let inode = self.inode(dir)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        path::check_name(name)?;
        if inode.kind() != Some(FileType::Directory) {
            return Err(Errno::ENOTDIR);
        }
        Ok(inode)
    }

    /// Creates `dir/name`, a regular file or a directory. The new inode is
    /// written only once its entry is in place; until then a failure frees
    /// what was allocated and leaves the filesystem as it was.
    pub(crate) fn make(&mut self, dir: Ino, name: &[u8], kind: FileType) -> Result<Ino, Errno> {
        let mut parent = self.dir_for_change(dir, name)?;
        if self.find_entry(&parent, name)?.is_some() {
            return Err(Errno::EEXIST);
        }
        let is_dir = kind == FileType::Directory;
        if is_dir && parent.links() >= LINK_MAX {
            // No EMLINK in the contract; the directory is full.
            return Err(Errno::ENOSPC);
        }
        let mode = if is_dir { 0o040755 } else { 0o100644 };
        let ino = self.alloc_inode(parent.ino, is_dir)?;
        let mut inode = Inode::fresh(ino, self.geo.inode_size as usize, mode, self.now());
        let result = self.link_new(&mut parent, name, &mut inode, kind);
        if let Err(e) = result {
            self.free_from(&mut inode, 0)?;
            self.free_inode(ino, is_dir)?;
            return Err(e);
        }
        self.write_inode(&inode)?;
        if is_dir {
            parent.set_links(parent.links() + 1);
        }
        self.write_inode(&parent)?;
        Ok(ino as Ino)
    }

    /// Gives a new directory its first block, then enters `inode` in
    /// `parent`.
    fn link_new(
        &mut self,
        parent: &mut Inode,
        name: &[u8],
        inode: &mut Inode,
        kind: FileType,
    ) -> Result<(), Errno> {
        if kind == FileType::Directory {
            let block = self.bmap_alloc(inode, 0)?;
            let data = self.cache.write(block as u64)?;
            dirent::init_dir(data, inode.ino, parent.ino, self.geo.filetype);
            inode.set_size(self.geo.block_size as u64);
        }
        self.add_entry(parent, name, inode.ino, kind)
    }

    /// Checks that directory `dir`'s `..` names `parent`, the directory it
    /// is found in. A directory entry naming a directory that lives
    /// elsewhere (one corrupt field) must not be removed, moved or replaced
    /// through: that would free or re-parent the real directory behind its
    /// real parent's back.
    pub(crate) fn check_parent(&mut self, dir: &Inode, parent: Ino) -> Result<(), Errno> {
        match self.find_entry(dir, b"..")? {
            Some(dotdot) if dotdot.entry.inode as Ino == parent => Ok(()),
            _ => Err(self.corrupt(format_args!(
                "directory {} is entered in {parent}, but its .. names another directory",
                dir.ino
            ))),
        }
    }

    /// Drops one link of a non-directory, freeing it with the last.
    pub(crate) fn drop_link(&mut self, inode: &mut Inode) -> Result<(), Errno> {
        inode.set_links(inode.links().saturating_sub(1));
        inode.set_ctime(self.now());
        if inode.links() == 0 {
            self.release_inode(inode)
        } else {
            self.write_inode(inode)
        }
    }

    pub(crate) fn remove_file(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        let mut parent = self.dir_for_change(dir, name)?;
        let found = self.find_entry(&parent, name)?.ok_or(Errno::ENOENT)?;
        let mut inode = self.entry_inode(&parent, &found)?;
        if inode.kind() == Some(FileType::Directory) {
            return Err(Errno::EISDIR);
        }
        self.remove_entry(&mut parent, &found)?;
        self.write_inode(&parent)?;
        self.drop_link(&mut inode)
    }

    pub(crate) fn remove_dir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        let mut parent = self.dir_for_change(dir, name)?;
        let found = self.find_entry(&parent, name)?.ok_or(Errno::ENOENT)?;
        let mut inode = self.entry_inode(&parent, &found)?;
        if inode.kind() != Some(FileType::Directory) {
            return Err(Errno::ENOTDIR);
        }
        self.check_parent(&inode, dir)?;
        if !self.dir_is_empty(&inode)? {
            return Err(Errno::ENOTEMPTY);
        }
        self.remove_entry(&mut parent, &found)?;
        // Its `..` no longer counts.
        parent.set_links(parent.links().saturating_sub(1));
        self.write_inode(&parent)?;
        inode.set_ctime(self.now());
        self.release_inode(&mut inode)
    }
}
