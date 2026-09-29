//! A process's file descriptors (user-space gate §5.4): 32 slots, each a
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent. Plan 3a's files are the console and
//! the in-kernel shell's output; plan 3b adds files of the VFS, milestone 3
//! pipe ends.

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
