//! The mount table (user-space gate §7.3): one for the whole kernel,
//! shared by the in-kernel shell and every process's calls, with each
//! caller's current directory put in while the kernel works for it
//! (§5.4). The lock is held only for one operation, never while anything
//! waits, so a process that blocks never holds it.

use alloc::vec::Vec;
use spin::Mutex;
use vfs::{Cwd, DirEntry, Errno, FileType, MountTable, Node, Stat, StatFs, Vfs};

/// The table behind its lock.
struct Mounts(Option<MountTable>);

// SAFETY: one CPU, and the filesystems are reached only through `MOUNTS`'s
// lock; nothing of theirs is used from an interrupt.
unsafe impl Send for Mounts {}

static MOUNTS: Mutex<Mounts> = Mutex::new(Mounts(None));

/// Makes `table` the kernel's mount table; the root, as a current
/// directory.
pub fn init(table: MountTable) -> Cwd {
    let root = table.root_cwd();
    MOUNTS.lock().0 = Some(table);
    root
}

/// Runs `f` on the mount table with `cwd` as its current directory, and
/// keeps what `f` makes of it (a `chdir`) in `cwd`.
pub fn with<R>(cwd: &mut Cwd, f: impl FnOnce(&mut MountTable) -> R) -> R {
    let mut guard = MOUNTS.lock();
    let table = guard.0.as_mut().expect("mounts::init has not run");
    let theirs = table.swap_cwd(cwd.clone());
    let r = f(table);
    *cwd = table.swap_cwd(theirs);
    r
}

/// Whether the mount table is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    MOUNTS.is_locked()
}

/// The mount table as the in-kernel shell's `Vfs`, with the current
/// directory of the process it works for.
pub struct KernelVfs;

impl KernelVfs {
    fn with<R>(&mut self, f: impl FnOnce(&mut MountTable) -> R) -> R {
        crate::proc::with_cwd(|cwd| with(cwd, f))
    }
}

impl Vfs for KernelVfs {
    fn cwd(&self) -> Vec<u8> {
        crate::proc::with_cwd(|cwd| with(cwd, |t| t.cwd()))
    }
    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.chdir(path))
    }
    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno> {
        self.with(|t| t.lookup(path))
    }
    fn stat(&mut self, node: Node) -> Result<Stat, Errno> {
        self.with(|t| t.stat(node))
    }
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        self.with(|t| t.read_dir(node))
    }
    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno> {
        self.with(|t| t.entry_kind(dir, entry))
    }
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
        self.with(|t| t.read_link(node))
    }
    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.with(|t| t.read_at(node, offset, buf))
    }
    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.with(|t| t.write_at(node, offset, buf))
    }
    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno> {
        self.with(|t| t.truncate(node, size))
    }
    fn touch(&mut self, node: Node) -> Result<(), Errno> {
        self.with(|t| t.touch(node))
    }
    fn create(&mut self, path: &[u8]) -> Result<Node, Errno> {
        self.with(|t| t.create(path))
    }
    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.mkdir(path))
    }
    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.unlink(path))
    }
    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.rmdir(path))
    }
    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.rename(from, to))
    }
    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno> {
        self.with(|t| t.statfs(path))
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.with(|t| t.sync())
    }
    fn shutdown(&mut self) -> Result<(), Errno> {
        self.with(|t| t.shutdown())
    }
}
