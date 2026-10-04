//! The filesystem trait and the types it uses (spec §8.1).
//!
//! Operations name inodes by number rather than through inode objects: a
//! filesystem owns its block cache and its device, and handing out objects
//! that borrow them would tie every caller to its lifetimes.

use crate::Errno;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;

/// An inode number, unique within one filesystem.
pub type Ino = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    Symlink,
    CharDev,
    BlockDev,
    Fifo,
    Socket,
}

/// What `stat` reports about an inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub ino: Ino,
    pub kind: FileType,
    /// Permission bits, including set-user-ID, set-group-ID and sticky
    /// (`0o7777`). Stored and shown, never enforced (spec §1.3).
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    /// Space used, in 512-byte units, as Linux's `st_blocks`.
    pub blocks: u64,
    /// The filesystem's block size (`IO Block` in `stat`).
    pub block_size: u32,
    /// Seconds since 1970, UTC.
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

/// One directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: Vec<u8>,
    pub ino: Ino,
}

/// What `df` reports about a filesystem. Counts are in `block_size` units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatFs {
    pub block_size: u64,
    pub blocks: u64,
    pub free_blocks: u64,
    /// Free blocks an unprivileged user could use (the rest is reserved).
    pub avail_blocks: u64,
    pub files: u64,
    pub free_files: u64,
    /// Every change is refused (`EROFS`): mounted so, or shut down.
    pub read_only: bool,
}

/// What a filesystem needs from its surroundings.
pub trait Env {
    /// Wall-clock time in seconds since 1970, UTC (the RTC in the kernel).
    fn now(&self) -> u64;
    /// Adds one line to the kernel log (`dmesg`).
    fn log(&self, line: &str);
}

impl<T: Env + ?Sized> Env for &T {
    fn now(&self) -> u64 {
        (**self).now()
    }
    fn log(&self, line: &str) {
        (**self).log(line)
    }
}

impl<T: Env + ?Sized> Env for Box<T> {
    fn now(&self) -> u64 {
        (**self).now()
    }
    fn log(&self, line: &str) {
        (**self).log(line)
    }
}

impl<T: Env + ?Sized> Env for Rc<T> {
    fn now(&self) -> u64 {
        (**self).now()
    }
    fn log(&self, line: &str) {
        (**self).log(line)
    }
}

/// A mounted filesystem.
///
/// The rules below are the contract every implementation keeps exactly;
/// the randomized model test holds ext2 to `MemFs` operation by operation,
/// errors included. Where several errors apply, the first in this order
/// wins:
///
/// 1. An inode number that is not in use: `ENOENT`.
/// 2. A change (`write_at`, `truncate`, `touch`, `create`, `mkdir`,
///    `unlink`, `rmdir`, `rename`) on a read-only filesystem: `EROFS`.
/// 3. A name to create, remove or rename that fails
///    [`check_name`](crate::path::check_name): its error. Names passed in
///    are never `.` or `..`; the mount table deals with those.
/// 4. A `dir` argument that is not a directory: `ENOTDIR`.
/// 5. The operation's own errors, as documented on each method.
///
/// New files get mode `0644`, new directories `0755`, uid and gid 0, and
/// all three times set to now. Changing a file's data sets its mtime and
/// ctime; adding or removing a directory entry sets the directory's.
///
/// A filesystem of devices, [`DevFs`](crate::DevFs), keeps the order of
/// errors but not the rest where its devices differ from files: a device's
/// reads, writes and truncation do what the device does (`null`'s ignore
/// the offset and the size), making a new name and removing or moving one
/// are `EPERM` (a rename of a name onto itself too), and `shutdown` leaves
/// it writable. The model test holds only ext2 and `MemFs` to the contract.
pub trait FileSystem {
    /// The root directory.
    fn root(&self) -> Ino;
    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno>;
    /// `.` gives `dir` itself and `..` its parent (the root is its own
    /// parent). A missing name is `ENOENT`.
    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno>;
    /// Every entry, including `.` and `..`, in no particular order.
    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno>;
    /// A symbolic link's target; `EINVAL` for anything else.
    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno>;
    /// Reads from `offset`; returns fewer bytes at the end of the file and
    /// 0 at or past it. Holes read as zeros. `EISDIR` for a directory,
    /// `EINVAL` for any other non-regular file.
    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno>;
    /// Writes at `offset`, growing the file (and leaving a hole) if it
    /// starts past the end. Returns how many bytes were written: fewer than
    /// asked means the filesystem filled up, and `ENOSPC` means not even
    /// one byte fitted. `EFBIG` if the end would pass the largest file
    /// size. An empty write changes nothing. `EISDIR`/`EINVAL` as `read_at`.
    fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno>;
    /// Sets the size. Growing leaves a hole; shrinking frees the space, and
    /// a later growth reads zeros there. `EFBIG`/`EISDIR`/`EINVAL` as
    /// `write_at`.
    fn truncate(&mut self, ino: Ino, size: u64) -> Result<(), Errno>;
    /// Sets the mtime and ctime to now (`touch`).
    fn touch(&mut self, ino: Ino) -> Result<(), Errno>;
    /// Creates an empty regular file. `EEXIST` if the name exists.
    fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno>;
    /// Creates a directory holding `.` and `..`. `EEXIST` if the name
    /// exists.
    fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno>;
    /// Removes a name of anything but a directory (`EISDIR`). The inode is
    /// freed with its last link.
    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno>;
    /// Removes an empty directory: `ENOTDIR` if the name is not one,
    /// `ENOTEMPTY` if it holds more than `.` and `..`.
    fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno>;
    /// Moves `from_dir/from` to `to_dir/to`, replacing what is there.
    /// `ENOENT` if `from` does not exist. If both names already refer to
    /// the same inode nothing happens. Otherwise, in this order: `EINVAL` if a directory would move into
    /// itself or a descendant; `ENOTDIR` if a directory would replace a
    /// non-directory; `EISDIR` if a non-directory would replace a
    /// directory; `ENOTEMPTY` if the directory it replaces is not empty.
    /// A moved directory's `..` then names its new parent.
    fn rename(&mut self, from_dir: Ino, from: &[u8], to_dir: Ino, to: &[u8]) -> Result<(), Errno>;
    fn statfs(&mut self) -> Result<StatFs, Errno>;
    /// Makes every change so far durable on the device.
    fn sync(&mut self) -> Result<(), Errno>;
    /// Syncs and marks the filesystem cleanly unmounted (before a reboot or
    /// power-off). The filesystem is read-only afterwards.
    fn shutdown(&mut self) -> Result<(), Errno>;
}
