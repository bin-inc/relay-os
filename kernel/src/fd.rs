//! A process's file descriptors (user-space gate §5.4): 32 slots, each a
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent, offset and all. The files are the
//! console and files of the VFS; milestone 3 adds pipe ends.

use crate::file::OpenFile;
use alloc::sync::Arc;
use relay_abi::FdMap;
use relay_abi::spawn::SPAWN_FDS;
use vfs::{Change, Errno};

/// Slots per process (spec §5.4).
pub const FDS: usize = 32;

/// An open file.
#[derive(Debug, PartialEq, Eq)]
pub enum File {
    /// The screen and keyboard.
    Console,
    /// A file of the VFS.
    Vfs(OpenFile),
}

pub struct FdTable {
    slots: [Option<Arc<File>>; FDS],
}

impl Default for FdTable {
    fn default() -> Self {
        FdTable::new()
    }
}

impl FdTable {
    /// No fd open.
    pub fn new() -> FdTable {
        FdTable {
            slots: [const { None }; FDS],
        }
    }

    /// Process 1's fds: the console as 0, 1 and 2, which the shell it
    /// starts gets.
    pub fn console() -> FdTable {
        let mut t = FdTable::new();
        for slot in &mut t.slots[..3] {
            *slot = Some(Arc::new(File::Console));
        }
        t
    }

    /// Opens `file` as `fd` (below 32), closing what was there.
    pub fn set(&mut self, fd: usize, file: Arc<File>) {
        self.slots[fd] = Some(file);
    }

    /// Opens `file` as the lowest fd that is free; `EMFILE` if all 32 are
    /// in use (spec §11.1).
    pub fn insert(&mut self, file: Arc<File>) -> Result<u32, Errno> {
        let fd = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Errno::EMFILE)?;
        self.slots[fd] = Some(file);
        Ok(fd as u32)
    }

    /// Closes `fd`; the file, which lives on while anything else has it.
    /// `EBADF` if `fd` is not open.
    pub fn remove(&mut self, fd: u64) -> Result<Arc<File>, Errno> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
            .and_then(Option::take)
            .ok_or(Errno::EBADF)
    }

    /// Every open file.
    pub fn files(&self) -> impl Iterator<Item = &Arc<File>> {
        self.slots.iter().flatten()
    }

    /// A removal elsewhere (spec §16 item 4): the open files of an inode it
    /// freed are gone.
    pub fn follow(&self, change: &Change) {
        let Some(node) = change.removed() else {
            return;
        };
        for file in self.files() {
            if let File::Vfs(open) = &**file
                && open.node() == node
            {
                open.mark_gone();
            }
        }
    }

    /// The file open as `fd`; `EBADF` if none is.
    pub fn get(&self, fd: u64) -> Result<&Arc<File>, Errno> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots.get(i))
            .and_then(Option::as_ref)
            .ok_or(Errno::EBADF)
    }

    /// A child's fds from `maps` (spec §7.3): each child fd gets the file
    /// this table has as the parent fd, every other one is closed. `EBADF`
    /// if a parent fd is not open or a child fd is not below 32, `EINVAL`
    /// for more than 8 pairs or a child fd named twice.
    pub fn for_child(&self, maps: &[FdMap]) -> Result<FdTable, Errno> {
        if maps.len() > SPAWN_FDS {
            return Err(Errno::EINVAL);
        }
        let mut child = FdTable::new();
        for m in maps {
            let file = self.get(u64::from(m.parent))?;
            let slot = child.slots.get_mut(m.child as usize).ok_or(Errno::EBADF)?;
            if slot.is_some() {
                return Err(Errno::EINVAL);
            }
            *slot = Some(Arc::clone(file));
        }
        Ok(child)
    }

    /// How many fds are open.
    pub fn open(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(child: u32, parent: u32) -> FdMap {
        FdMap { child, parent }
    }

    #[test]
    fn process_1_has_the_console_as_0_1_and_2() {
        let t = FdTable::console();
        for fd in 0..3 {
            assert_eq!(**t.get(fd).unwrap(), File::Console);
        }
        assert_eq!(t.open(), 3);
        for fd in [3, 31, 32, 1 << 32, u64::MAX] {
            assert_eq!(t.get(fd).err(), Some(Errno::EBADF), "{fd}");
        }
    }

    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
        let parent = FdTable::console();
        let child = parent
            .for_child(&[map(0, 0), map(1, 2), map(31, 1)])
            .unwrap();
        assert!(Arc::ptr_eq(child.get(0).unwrap(), parent.get(0).unwrap()));
        assert!(Arc::ptr_eq(child.get(1).unwrap(), parent.get(2).unwrap()));
        assert!(Arc::ptr_eq(child.get(31).unwrap(), parent.get(1).unwrap()));
        assert_eq!(child.get(2).err(), Some(Errno::EBADF), "not given");
        assert_eq!(child.open(), 3);
        // The files stay while either has them.
        assert_eq!(Arc::strong_count(parent.get(2).unwrap()), 2);
        drop(child);
        assert_eq!(Arc::strong_count(parent.get(2).unwrap()), 1);
        assert_eq!(parent.for_child(&[]).unwrap().open(), 0);
    }

    #[test]
    fn a_file_put_in_replaces_the_old_one_for_later_children_only() {
        let mut parent = FdTable::console();
        let old = Arc::clone(parent.get(1).unwrap());
        let before = parent.for_child(&[map(1, 1)]).unwrap();
        parent.set(1, Arc::new(File::Console));
        let after = parent.for_child(&[map(1, 1)]).unwrap();
        assert!(!Arc::ptr_eq(before.get(1).unwrap(), after.get(1).unwrap()));
        assert!(Arc::ptr_eq(after.get(1).unwrap(), parent.get(1).unwrap()));
        assert!(Arc::ptr_eq(before.get(1).unwrap(), &old));
    }

    #[test]
    fn a_file_opened_takes_the_lowest_free_fd_up_to_32() {
        let mut t = FdTable::console();
        let console = || Arc::new(File::Console);
        assert_eq!(t.insert(console()), Ok(3));
        assert_eq!(t.remove(1).map(|f| *f == File::Console), Ok(true));
        assert_eq!(t.insert(console()), Ok(1), "the lowest free one");
        for fd in 4..32 {
            assert_eq!(t.insert(console()), Ok(fd));
        }
        assert_eq!(t.insert(console()), Err(Errno::EMFILE));
        assert_eq!(t.open(), 32);
        assert_eq!(t.files().count(), 32);
        assert!(t.remove(31).is_ok());
        assert_eq!(t.remove(31).err(), Some(Errno::EBADF), "closed already");
        for fd in [32, 1 << 32, u64::MAX] {
            assert_eq!(t.remove(fd).err(), Some(Errno::EBADF), "{fd}");
        }
        assert_eq!(t.insert(console()), Ok(31));
    }

    #[test]
    fn the_open_files_of_a_freed_inode_are_gone() {
        use crate::file;
        use alloc::boxed::Box;
        use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_WRITE};
        use vfs::{Env, MemFs, MountTable, Vfs};
        struct Clock;
        impl Env for Clock {
            fn now(&self) -> u64 {
                0
            }
            fn log(&self, _: &str) {}
        }
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock))));
        let rw = OPEN_READ | OPEN_WRITE | OPEN_CREATE;
        let a = Arc::new(File::Vfs(file::open(&mut t, b"/a", rw).unwrap()));
        let b = Arc::new(File::Vfs(file::open(&mut t, b"/b", rw).unwrap()));
        let mut fds = FdTable::console();
        fds.insert(Arc::clone(&a)).unwrap();
        fds.insert(Arc::clone(&a)).unwrap();
        fds.insert(Arc::clone(&b)).unwrap();
        let freed = t.lookup(b"/a").unwrap();
        t.unlink(b"/a").unwrap();
        for c in t.take_changes() {
            fds.follow(&c);
        }
        // The next file gets the freed inode: `a` must not reach it.
        let c = t.create(b"/c").unwrap();
        assert_eq!(c, freed);
        t.write_at(c, 0, b"new").unwrap();
        let read = |f: &File, t: &mut MountTable| match f {
            File::Vfs(o) => o.read(t, &mut [0; 4]),
            _ => unreachable!(),
        };
        assert_eq!(read(&a, &mut t), Err(Errno::ENOENT));
        assert_eq!(read(&b, &mut t), Ok(0), "another file");
        // A move frees nothing.
        t.mkdir(b"/d").unwrap();
        t.rename(b"/d", b"/e").unwrap();
        for c in t.take_changes() {
            fds.follow(&c);
        }
        assert_eq!(read(&b, &mut t), Ok(0));
    }

    #[test]
    fn a_bad_mapping_is_refused() {
        let parent = FdTable::console();
        assert_eq!(parent.for_child(&[map(0, 3)]).err(), Some(Errno::EBADF));
        assert_eq!(parent.for_child(&[map(32, 0)]).err(), Some(Errno::EBADF));
        assert_eq!(
            parent.for_child(&[map(u32::MAX, 0)]).err(),
            Some(Errno::EBADF)
        );
        assert_eq!(
            parent.for_child(&[map(1, 1), map(1, 2)]).err(),
            Some(Errno::EINVAL),
            "fd 1 twice"
        );
        let nine: Vec<FdMap> = (0..9).map(|i| map(i, 0)).collect();
        assert_eq!(parent.for_child(&nine).err(), Some(Errno::EINVAL));
        assert_eq!(parent.for_child(&nine[..8]).unwrap().open(), 8);
    }
}
