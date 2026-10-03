//! `/dev` (programmable shell gate §8.4): a filesystem of one node,
//! `null`, a character device that reads as the end of input at once and
//! takes every write, keeping nothing. Nothing can be made, removed or
//! moved in it (`EPERM`, as on a Linux filesystem without those
//! operations); truncating and touching `null` succeed and change nothing.
//! It is not read-only: `null` is written.

use crate::Errno;
use crate::fs::{DirEntry, FileSystem, FileType, Ino, Stat, StatFs};
use crate::path::check_name;
use alloc::vec;
use alloc::vec::Vec;

const ROOT: Ino = 1;
const NULL: Ino = 2;

/// The block size it reports.
const BLOCK: u32 = 4096;

pub struct DevFs {
    /// Every time of both nodes: when it was made.
    time: u64,
}

impl DevFs {
    /// Made at `now`, seconds since 1970.
    pub fn new(now: u64) -> DevFs {
        DevFs { time: now }
    }

    /// `ENOENT` for a number that is no node, `ENOTDIR` for `null`.
    fn dir(&self, ino: Ino) -> Result<(), Errno> {
        match ino {
            ROOT => Ok(()),
            NULL => Err(Errno::ENOTDIR),
            _ => Err(Errno::ENOENT),
        }
    }

    /// The node a name of the root names: `ENOENT` for any but `null`.
    /// The errors come in the contract's order (`FileSystem`): a number
    /// that is no node, then the name's, then `ENOTDIR`.
    fn entry(&self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        if dir != ROOT && dir != NULL {
            return Err(Errno::ENOENT);
        }
        check_name(name)?;
        self.dir(dir)?;
        match name {
            b"null" => Ok(NULL),
            _ => Err(Errno::ENOENT),
        }
    }

    /// `null` for its own operations, `EISDIR` for the root.
    fn null(&self, ino: Ino) -> Result<(), Errno> {
        match ino {
            NULL => Ok(()),
            ROOT => Err(Errno::EISDIR),
            _ => Err(Errno::ENOENT),
        }
    }

    /// Making `name` in `dir`: `EEXIST` for `null`, `EPERM` for any other.
    fn make(&self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        match self.entry(dir, name) {
            Ok(_) => Err(Errno::EEXIST),
            Err(Errno::ENOENT) if self.dir(dir).is_ok() => Err(Errno::EPERM),
            Err(e) => Err(e),
        }
    }
}

impl FileSystem for DevFs {
    fn root(&self) -> Ino {
        ROOT
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let (kind, perm, nlink) = match ino {
            ROOT => (FileType::Directory, 0o755, 2),
            NULL => (FileType::CharDev, 0o666, 1),
            _ => return Err(Errno::ENOENT),
        };
        Ok(Stat {
            ino,
            kind,
            perm,
            nlink,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            block_size: BLOCK,
            atime: self.time,
            mtime: self.time,
            ctime: self.time,
        })
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.dir(dir)?;
        match name {
            b"." | b".." => Ok(ROOT),
            _ => self.entry(dir, name),
        }
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        self.dir(dir)?;
        let entry = |name: &[u8], ino| DirEntry {
            name: name.to_vec(),
            ino,
        };
        Ok(vec![
            entry(b".", ROOT),
            entry(b"..", ROOT),
            entry(b"null", NULL),
        ])
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.stat(ino)?;
        Err(Errno::EINVAL)
    }

    fn read_at(&mut self, ino: Ino, _offset: u64, _buf: &mut [u8]) -> Result<usize, Errno> {
        self.null(ino)?;
        Ok(0)
    }

    fn write_at(&mut self, ino: Ino, _offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.null(ino)?;
        Ok(buf.len())
    }

    fn truncate(&mut self, ino: Ino, _size: u64) -> Result<(), Errno> {
        self.null(ino)
    }

    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        self.stat(ino).map(|_| ())
    }

    fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.make(dir, name)
    }

    fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.make(dir, name)
    }

    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.entry(dir, name)?;
        Err(Errno::EPERM)
    }

    fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.entry(dir, name)?;
        Err(Errno::ENOTDIR)
    }

    fn rename(&mut self, from_dir: Ino, from: &[u8], to_dir: Ino, to: &[u8]) -> Result<(), Errno> {
        self.entry(from_dir, from)?;
        check_name(to)?;
        self.dir(to_dir)?;
        Err(Errno::EPERM)
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        Ok(StatFs {
            block_size: u64::from(BLOCK),
            blocks: 0,
            free_blocks: 0,
            avail_blocks: 0,
            files: 2,
            free_files: 0,
            read_only: false,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemFs, MountTable, Vfs};
    use alloc::boxed::Box;

    const NOW: u64 = 1_790_424_000;

    fn names(fs: &mut DevFs, dir: Ino) -> Vec<Vec<u8>> {
        let mut n: Vec<Vec<u8>> = fs
            .read_dir(dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        n.sort();
        n
    }

    #[test]
    fn null_reads_as_the_end_and_takes_every_write() {
        let mut fs = DevFs::new(NOW);
        let null = fs.lookup(fs.root(), b"null").unwrap();
        let mut buf = [7u8; 8];
        assert_eq!(fs.read_at(null, 0, &mut buf), Ok(0));
        assert_eq!(fs.read_at(null, 1 << 40, &mut buf), Ok(0));
        assert_eq!(buf, [7; 8], "nothing written into the buffer");
        assert_eq!(fs.write_at(null, 0, b"gone"), Ok(4));
        assert_eq!(fs.write_at(null, u64::MAX, &[0; 5000]), Ok(5000));
        assert_eq!(fs.truncate(null, 0), Ok(()), "`> /dev/null` truncates it");
        assert_eq!(fs.truncate(null, 9), Ok(()));
        assert_eq!(fs.touch(null), Ok(()));
        let st = fs.stat(null).unwrap();
        assert_eq!((st.size, st.blocks), (0, 0), "keeps nothing");
        assert_eq!((st.atime, st.mtime, st.ctime), (NOW, NOW, NOW));
    }

    #[test]
    fn null_is_a_character_device_anyone_may_read_and_write() {
        let mut fs = DevFs::new(NOW);
        let root = fs.root();
        let null = fs.lookup(root, b"null").unwrap();
        let st = fs.stat(null).unwrap();
        assert_eq!(
            (st.ino, st.kind, st.perm, st.nlink, st.uid, st.gid),
            (null, FileType::CharDev, 0o666, 1, 0, 0)
        );
        let dir = fs.stat(root).unwrap();
        assert_eq!(
            (dir.kind, dir.perm, dir.nlink),
            (FileType::Directory, 0o755, 2)
        );
        assert_eq!(fs.stat(99), Err(Errno::ENOENT));
        assert_eq!(names(&mut fs, root), [&b"."[..], b"..", b"null"]);
        assert_eq!(fs.lookup(root, b"."), Ok(root));
        assert_eq!(fs.lookup(root, b".."), Ok(root));
        assert_eq!(fs.lookup(root, b"zero"), Err(Errno::ENOENT));
        assert_eq!(fs.lookup(null, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(null), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_link(null), Err(Errno::EINVAL));
        let mut buf = [0u8; 4];
        assert_eq!(fs.read_at(root, 0, &mut buf), Err(Errno::EISDIR));
        assert_eq!(fs.write_at(root, 0, b"x"), Err(Errno::EISDIR));
        let s = fs.statfs().unwrap();
        assert!(!s.read_only, "null is written");
        assert_eq!((s.blocks, s.files), (0, 2));
    }

    #[test]
    fn nothing_is_made_removed_or_moved() {
        // As on a Linux filesystem without those operations: EPERM, after
        // the errors a missing or existing name gives.
        let mut fs = DevFs::new(NOW);
        let root = fs.root();
        let null = fs.lookup(root, b"null").unwrap();
        assert_eq!(fs.create(root, b"x"), Err(Errno::EPERM));
        assert_eq!(fs.create(root, b"null"), Err(Errno::EEXIST));
        assert_eq!(fs.mkdir(root, b"d"), Err(Errno::EPERM));
        assert_eq!(fs.mkdir(root, b"null"), Err(Errno::EEXIST));
        assert_eq!(fs.unlink(root, b"null"), Err(Errno::EPERM));
        assert_eq!(fs.unlink(root, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.rmdir(root, b"null"), Err(Errno::ENOTDIR));
        assert_eq!(fs.rmdir(root, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.rename(root, b"null", root, b"n"), Err(Errno::EPERM));
        assert_eq!(fs.rename(root, b"x", root, b"n"), Err(Errno::ENOENT));
        assert_eq!(fs.create(null, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.create(99, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.create(root, b"a/b"), Err(Errno::EINVAL));
        assert_eq!(fs.create(99, b"a/b"), Err(Errno::ENOENT), "the node first");
        assert_eq!(fs.create(null, b"a/b"), Err(Errno::EINVAL), "then the name");
        assert_eq!(names(&mut fs, root).len(), 3, "still just null");
    }

    #[test]
    fn mounted_at_dev_it_stands_over_what_the_disk_has_there() {
        let mut disk = MemFs::new(Box::new(Clock));
        let root = disk.root();
        let dev = disk.mkdir(root, b"dev").unwrap();
        disk.create(dev, b"x").unwrap();
        let mut t = MountTable::new(Box::new(disk));
        t.mount(b"/dev", Box::new(DevFs::new(NOW))).unwrap();
        let null = t.lookup(b"/dev/null").unwrap();
        assert_eq!(null.mount, 1);
        assert_eq!(t.write_at(null, 0, b"x"), Ok(1));
        assert_eq!(t.lookup(b"/dev/x"), Err(Errno::ENOENT));
        assert_eq!(t.create(b"/dev/x"), Err(Errno::EPERM));
        assert_eq!(t.unlink(b"/dev/null"), Err(Errno::EPERM));
        assert!(!t.statfs(b"/dev/null").unwrap().read_only);
        // Where the disk has no /dev, the name is a mount point all the same.
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock)).read_only()));
        t.mount(b"/dev", Box::new(DevFs::new(NOW))).unwrap();
        assert!(t.lookup(b"/dev/null").is_ok());
    }

    struct Clock;

    impl crate::Env for Clock {
        fn now(&self) -> u64 {
            NOW
        }
        fn log(&self, _: &str) {}
    }
}
