//! The file calls over a `vfs::MountTable` of `MemFs`s, for testing
//! `SysVfs` on the host: fds with offsets, `read_dir`'s records in name
//! order, and each call's errors where a test needs them.
#![cfg(test)]

use crate::sysvfs::Calls;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_TRUNCATE, OPEN_WRITE,
    put_dir_entry,
};
use vfs::{Env, Errno, FileType, MemFs, MountTable, Node, Vfs};

/// The tests' time: Sat Sep 26 12:00:00 UTC 2026.
pub const NOW: u64 = 1_790_424_000;

struct Clock;

impl Env for Clock {
    fn now(&self) -> u64 {
        NOW
    }
    fn log(&self, _: &str) {}
}

pub fn memfs() -> MemFs {
    MemFs::new(Box::new(Clock))
}

struct Open {
    node: Node,
    offset: u64,
    /// The last name `read_dir` gave.
    after: Option<Vec<u8>>,
}

pub struct FakeCalls {
    pub table: RefCell<MountTable>,
    fds: RefCell<Vec<Option<Open>>>,
    /// How many fds are open now, and the most there ever were.
    pub open_now: Cell<usize>,
    pub open_most: Cell<usize>,
    /// Every call made, by name.
    pub calls: RefCell<Vec<&'static str>>,
    /// `read_dir` gives at most this many bytes a call.
    pub dir_chunk: Cell<usize>,
    /// `read_dir` never goes on: it gives the first records every time.
    pub dir_repeats: Cell<bool>,
}

fn kind(t: FileType) -> u8 {
    match t {
        FileType::Regular => KIND_REGULAR,
        FileType::Directory => KIND_DIRECTORY,
        FileType::Symlink => KIND_SYMLINK,
        FileType::CharDev => KIND_CHAR_DEVICE,
        FileType::BlockDev => KIND_BLOCK_DEVICE,
        FileType::Fifo => KIND_FIFO,
        FileType::Socket => KIND_SOCKET,
    }
}

impl FakeCalls {
    pub fn new(table: MountTable) -> FakeCalls {
        FakeCalls {
            table: RefCell::new(table),
            fds: RefCell::new(Vec::new()),
            open_now: Cell::new(0),
            open_most: Cell::new(0),
            calls: RefCell::new(Vec::new()),
            dir_chunk: Cell::new(usize::MAX),
            dir_repeats: Cell::new(false),
        }
    }

    fn called(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }

    fn stat_of(&self, node: Node) -> Result<relay_abi::Stat, Errno> {
        let s = self.table.borrow_mut().stat(node)?;
        Ok(relay_abi::Stat {
            ino: s.ino,
            size: s.size,
            blocks: s.blocks,
            atime: s.atime,
            mtime: s.mtime,
            ctime: s.ctime,
            kind: u32::from(kind(s.kind)),
            perm: u32::from(s.perm),
            nlink: s.nlink,
            uid: s.uid,
            gid: s.gid,
            block_size: s.block_size,
            dev: node.mount as u64 + 1,
        })
    }

    fn with_open<R>(&self, fd: u32, f: impl FnOnce(&mut Open) -> R) -> Result<R, Errno> {
        let mut fds = self.fds.borrow_mut();
        let open = fds
            .get_mut(fd as usize)
            .and_then(Option::as_mut)
            .ok_or(Errno::EBADF)?;
        Ok(f(open))
    }
}

impl Calls for FakeCalls {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno> {
        self.called("open");
        let mut t = self.table.borrow_mut();
        let node = if flags & OPEN_CREATE != 0 {
            match t.lookup(path) {
                Ok(_) if flags & OPEN_EXCLUSIVE != 0 => return Err(Errno::EEXIST),
                Ok(node) => node,
                Err(Errno::ENOENT) => t.create(path)?,
                Err(e) => return Err(e),
            }
        } else {
            t.lookup(path)?
        };
        let st = t.stat(node)?;
        let dir = st.kind == FileType::Directory;
        if flags & OPEN_DIRECTORY != 0 && !dir {
            return Err(Errno::ENOTDIR);
        }
        if flags & OPEN_WRITE != 0 && dir {
            return Err(Errno::EISDIR);
        }
        if flags & OPEN_TRUNCATE != 0 {
            t.truncate(node, 0)?;
        }
        let mut fds = self.fds.borrow_mut();
        let open = Open {
            node,
            offset: 0,
            after: None,
        };
        let fd = match fds.iter().position(Option::is_none) {
            Some(i) => {
                fds[i] = Some(open);
                i
            }
            None => {
                fds.push(Some(open));
                fds.len() - 1
            }
        };
        self.open_now.set(self.open_now.get() + 1);
        self.open_most
            .set(self.open_most.get().max(self.open_now.get()));
        Ok(fd as u32)
    }

    fn close(&self, fd: u32) {
        self.called("close");
        if let Some(slot) = self.fds.borrow_mut().get_mut(fd as usize)
            && slot.take().is_some()
        {
            self.open_now.set(self.open_now.get() - 1);
        }
    }

    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("read");
        let (node, offset) = self.with_open(fd, |o| (o.node, o.offset))?;
        let n = self.table.borrow_mut().read_at(node, offset, buf)?;
        self.with_open(fd, |o| o.offset += n as u64)?;
        Ok(n)
    }

    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno> {
        self.called("write");
        let (node, offset) = self.with_open(fd, |o| (o.node, o.offset))?;
        let n = self.table.borrow_mut().write_at(node, offset, bytes)?;
        self.with_open(fd, |o| o.offset += n as u64)?;
        Ok(n)
    }

    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno> {
        self.called("seek");
        self.with_open(fd, |o| o.offset = offset)
    }

    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno> {
        self.called("fstat");
        let node = self.with_open(fd, |o| o.node)?;
        self.stat_of(node)
    }

    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno> {
        self.called("stat");
        let node = self.table.borrow_mut().lookup(path)?;
        self.stat_of(node)
    }

    /// Records in name order, from the one after the last name given, as
    /// many as fit in `buf` and `dir_chunk`.
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("read_dir");
        let (node, after) = self.with_open(fd, |o| (o.node, o.after.clone()))?;
        let mut t = self.table.borrow_mut();
        let mut entries = t.read_dir(node)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let limit = buf.len().min(self.dir_chunk.get());
        let mut done = 0;
        let mut last = None;
        for e in entries
            .iter()
            .filter(|e| after.as_ref().is_none_or(|a| e.name > *a))
        {
            let k = kind(t.entry_kind(node, e)?);
            match put_dir_entry(&mut buf[done..limit], e.ino, k, &e.name) {
                Some(n) => done += n,
                None => break,
            }
            last = Some(e.name.clone());
        }
        if last.is_some() && !self.dir_repeats.get() {
            self.with_open(fd, |o| o.after = last)?;
        }
        Ok(done)
    }

    fn mkdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("mkdir");
        self.table.borrow_mut().mkdir(path)
    }

    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("rmdir");
        self.table.borrow_mut().rmdir(path)
    }

    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("unlink");
        self.table.borrow_mut().unlink(path)
    }

    fn touch(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("touch");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        t.touch(node)
    }

    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno> {
        self.called("truncate");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        t.truncate(node, size)
    }

    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("readlink");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        let target = t.read_link(node)?;
        let n = target.len().min(buf.len());
        buf[..n].copy_from_slice(&target[..n]);
        Ok(n)
    }

    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.called("rename");
        self.table.borrow_mut().rename(from, to)
    }

    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno> {
        self.called("statfs");
        let s = self.table.borrow_mut().statfs(path)?;
        Ok(relay_abi::StatFs {
            block_size: s.block_size,
            blocks: s.blocks,
            free_blocks: s.free_blocks,
            avail_blocks: s.avail_blocks,
            files: s.files,
            free_files: s.free_files,
            flags: 0,
        })
    }

    fn sync(&self) -> Result<(), Errno> {
        self.called("sync");
        self.table.borrow_mut().sync()
    }

    fn chdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("chdir");
        self.table.borrow_mut().chdir(path)
    }

    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("getcwd");
        let cwd = self.table.borrow().cwd();
        let dst = buf.get_mut(..cwd.len()).ok_or(Errno::ERANGE)?;
        dst.copy_from_slice(&cwd);
        Ok(cwd.len())
    }
}
