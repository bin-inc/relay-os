//! A process's file descriptors (user-space gate §5.4): 32 slots, each a
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent, offset and all. The files are the
//! console, the in-kernel shell's output and files of the VFS; milestone 3
//! adds pipe ends.

use crate::file::OpenFile;
use alloc::sync::Arc;
use relay_abi::FdMap;
use relay_abi::spawn::SPAWN_FDS;
use vfs::Errno;

/// Slots per process (spec §5.4).
pub const FDS: usize = 32;

/// An open file.
#[derive(Debug, PartialEq, Eq)]
pub enum File {
    /// The screen and keyboard.
    Console,
    /// Standard output (1) or standard error (2) of the in-kernel shell:
    /// what is written goes to the shell, which sends it where the command
    /// line says (a redirection, the screen, a script's transcript). Plan 4
    /// replaces the in-kernel shell and this with it.
    ShellOutput(u32),
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

    /// The in-kernel shell's fds: the console to read, and its standard
    /// output and error.
    pub fn shell() -> FdTable {
        let mut t = FdTable::new();
        t.slots[0] = Some(Arc::new(File::Console));
        t.slots[1] = Some(Arc::new(File::ShellOutput(1)));
        t.slots[2] = Some(Arc::new(File::ShellOutput(2)));
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
    fn the_shell_has_the_console_and_its_outputs() {
        let t = FdTable::shell();
        assert_eq!(**t.get(0).unwrap(), File::Console);
        assert_eq!(**t.get(1).unwrap(), File::ShellOutput(1));
        assert_eq!(**t.get(2).unwrap(), File::ShellOutput(2));
        assert_eq!(t.open(), 3);
        for fd in [3, 31, 32, 1 << 32, u64::MAX] {
            assert_eq!(t.get(fd).err(), Some(Errno::EBADF), "{fd}");
        }
    }

    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
        let parent = FdTable::shell();
        let child = parent
            .for_child(&[map(0, 0), map(1, 2), map(31, 1)])
            .unwrap();
        assert!(Arc::ptr_eq(child.get(0).unwrap(), parent.get(0).unwrap()));
        assert!(Arc::ptr_eq(child.get(1).unwrap(), parent.get(2).unwrap()));
        assert_eq!(**child.get(31).unwrap(), File::ShellOutput(1));
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
        let mut shell = FdTable::shell();
        let before = shell.for_child(&[map(1, 1)]).unwrap();
        shell.set(1, Arc::new(File::ShellOutput(1)));
        let after = shell.for_child(&[map(1, 1)]).unwrap();
        assert!(!Arc::ptr_eq(before.get(1).unwrap(), after.get(1).unwrap()));
        assert!(Arc::ptr_eq(after.get(1).unwrap(), shell.get(1).unwrap()));
        assert_eq!(**before.get(1).unwrap(), File::ShellOutput(1));
    }

    #[test]
    fn a_file_opened_takes_the_lowest_free_fd_up_to_32() {
        let mut t = FdTable::shell();
        let console = || Arc::new(File::Console);
        assert_eq!(t.insert(console()), Ok(3));
        assert_eq!(t.remove(1).map(|f| *f == File::ShellOutput(1)), Ok(true));
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
    fn a_bad_mapping_is_refused() {
        let parent = FdTable::shell();
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
