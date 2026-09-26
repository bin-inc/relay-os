//! Directories (spec §8.2): finding and listing entries. Directories are
//! read linearly, which covers htree-indexed ones too: their index blocks
//! look like blocks holding only unused entries.

use crate::Ext2;
use crate::dirent::{self, Entry};
use crate::inode::{INDEX_FL, Inode};
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use vfs::{BlockDevice, DirEntry, Errno, FileType, Ino};

/// Where a directory entry was found.
pub(crate) struct Found {
    pub entry: Entry,
}

impl<D: BlockDevice> Ext2<D> {
    /// Directory `ino`: `ENOENT` if not in use, `ENOTDIR` if not a
    /// directory.
    pub(crate) fn dir_inode(&mut self, ino: Ino) -> Result<Inode, Errno> {
        let inode = self.inode(ino)?;
        if inode.kind() != Some(FileType::Directory) {
            return Err(Errno::ENOTDIR);
        }
        Ok(inode)
    }

    /// Calls `f` with each block of `dir` (logical and physical number,
    /// data, entries) until it returns something.
    fn scan_dir<T>(
        &mut self,
        dir: &Inode,
        mut f: impl FnMut(u64, u32, &[u8], &[Entry]) -> Option<T>,
    ) -> Result<Option<T>, Errno> {
        let bs = self.geo.block_size as u64;
        if !dir.size().is_multiple_of(bs) {
            return Err(self.corrupt(format_args!(
                "directory {}: size {} is not whole blocks",
                dir.ino,
                dir.size()
            )));
        }
        // Directories have no holes, so every block of the size is counted
        // in `i_blocks`; a huge size is refused before any work.
        let blocks = dir.size() / bs;
        if blocks > dir.blocks() as u64 / (bs / 512) {
            return Err(self.corrupt(format_args!(
                "directory {}: size {} needs more blocks than it has",
                dir.ino,
                dir.size()
            )));
        }
        let (filetype, max_inode) = (self.geo.filetype, self.geo.inodes_count);
        let mut seen = BTreeSet::new();
        for lb in 0..blocks {
            let Some(block) = self.bmap(dir, lb)? else {
                return Err(self.corrupt(format_args!("directory {}: hole at block {lb}", dir.ino)));
            };
            if !seen.insert(block) {
                return Err(self.corrupt(format_args!(
                    "directory {}: block {block} appears twice",
                    dir.ino
                )));
            }
            let result = {
                let data = self.cache.read(block as u64)?;
                dirent::parse(data, filetype, max_inode)
                    .and_then(|entries| {
                        dirent::check_names(data, &entries, lb == 0)?;
                        Ok(entries)
                    })
                    .map(|entries| f(lb, block, data, &entries))
            };
            match result {
                Ok(Some(found)) => return Ok(Some(found)),
                Ok(None) => {}
                Err(c) => {
                    return Err(self.corrupt(format_args!(
                        "directory {}: block {lb}: {} at offset {}",
                        dir.ino, c.what, c.offset
                    )));
                }
            }
        }
        Ok(None)
    }

    /// The entry called `name` in `dir`.
    pub(crate) fn find_entry(&mut self, dir: &Inode, name: &[u8]) -> Result<Option<Found>, Errno> {
        self.scan_dir(dir, |_, _, data, entries| {
            entries
                .iter()
                .find(|e| e.inode != 0 && e.name(data) == name)
                .map(|&entry| Found { entry })
        })
    }

    pub(crate) fn dir_lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        let inode = self.dir_inode(dir)?;
        if name == b"." {
            return Ok(dir);
        }
        match self.find_entry(&inode, name)? {
            Some(found) => Ok(found.entry.inode as Ino),
            None => Err(Errno::ENOENT),
        }
    }

    pub(crate) fn dir_list(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        let inode = self.dir_inode(dir)?;
        let mut out = Vec::new();
        self.scan_dir(&inode, |_, _, data, entries| {
            let used = entries.iter().filter(|e| e.inode != 0);
            out.extend(used.map(|e| DirEntry {
                name: e.name(data).to_vec(),
                ino: e.inode as Ino,
            }));
            None::<()>
        })?;
        Ok(out)
    }

    /// Adds the entry `name` → `ino` (of type `kind`) to `dir`, in the first
    /// block with room, else in a new block at the end. Only the new block
    /// can fail to fit (`ENOSPC`), before anything changed. The caller
    /// writes `dir` back.
    pub(crate) fn add_entry(
        &mut self,
        dir: &mut Inode,
        name: &[u8],
        ino: u32,
        kind: FileType,
    ) -> Result<(), Errno> {
        let spot = self.scan_dir(dir, |_, block, _, entries| {
            dirent::room(entries, name.len()).map(|at| (block, at))
        })?;
        let (block, at) = match spot {
            Some(spot) => spot,
            None => {
                let bs = self.geo.block_size as u64;
                if dir.size() + bs > u32::MAX as u64 {
                    return Err(Errno::ENOSPC);
                }
                let block = self.bmap_alloc(dir, dir.size() / bs)?;
                dirent::init_empty(self.cache.write(block as u64)?);
                dir.set_size(dir.size() + bs);
                let at = Entry {
                    offset: 0,
                    inode: 0,
                    rec_len: bs as usize,
                    name_len: 0,
                };
                (block, at)
            }
        };
        let file_type = self.geo.filetype.then(|| dirent::type_byte(kind));
        dirent::insert(self.cache.write(block as u64)?, at, name, ino, file_type);
        self.changed_dir(dir);
        Ok(())
    }

    /// Notes a change to `dir`'s entries: mtime and ctime, and an htree
    /// index this driver does not maintain is dropped (the directory reads
    /// linearly just as well).
    pub(crate) fn changed_dir(&mut self, dir: &mut Inode) {
        dir.set_flags(dir.flags() & !INDEX_FL);
        dir.touch(self.now());
    }
}
