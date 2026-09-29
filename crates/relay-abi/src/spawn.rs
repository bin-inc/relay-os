//! What `spawn` and `wait` take (spec §7.3): the new program's path,
//! arguments, working directory and file descriptors, and `wait`'s flags.

/// How many fds `spawn` can hand a child.
pub const SPAWN_FDS: usize = 8;

/// `SpawnArgs::flags`: the child starts a process group of its own, with
/// its pid as the group's number, instead of joining its parent's (spec
/// §6.4).
pub const NEW_GROUP: u32 = 1;

/// `wait`'s pid for any child.
pub const WAIT_ANY: i64 = -1;
/// `wait`'s flags: return 0 at once when no child has ended.
pub const WAIT_NOHANG: u32 = 1;

/// One of the child's fds: `child` gets what the caller has open as
/// `parent`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FdMap {
    pub child: u32,
    pub parent: u32,
}

/// `spawn`'s argument, `#[repr(C)]` with no padding. Every pointer and
/// length names bytes of the caller's memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnArgs {
    /// The program's path, relative to the caller's working directory.
    pub path: u64,
    pub path_len: u64,
    /// The arguments, argument 0 first, each followed by a NUL.
    pub args: u64,
    pub args_len: u64,
    /// The child's working directory, relative to the caller's; empty for
    /// the caller's own.
    pub cwd: u64,
    pub cwd_len: u64,
    /// The first `fd_count` entries are the child's fds; every other fd of
    /// the child is closed.
    pub fds: [FdMap; SPAWN_FDS],
    pub fd_count: u32,
    /// [`NEW_GROUP`] or 0.
    pub flags: u32,
}

impl SpawnArgs {
    /// Its size in bytes.
    pub const SIZE: usize = core::mem::size_of::<SpawnArgs>();

    /// The struct whose bytes (as the calling program laid them out) are
    /// `b`.
    pub fn from_bytes(b: &[u8; SpawnArgs::SIZE]) -> SpawnArgs {
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let mut fds = [FdMap::default(); SPAWN_FDS];
        for (k, fd) in fds.iter_mut().enumerate() {
            *fd = FdMap {
                child: u32_at(48 + 8 * k),
                parent: u32_at(52 + 8 * k),
            };
        }
        SpawnArgs {
            path: u64_at(0),
            path_len: u64_at(8),
            args: u64_at(16),
            args_len: u64_at(24),
            cwd: u64_at(32),
            cwd_len: u64_at(40),
            fds,
            fd_count: u32_at(112),
            flags: u32_at(116),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<FdMap>(), 8);
        assert_eq!(offset_of!(FdMap, child), 0);
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 120);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
        assert_eq!(offset_of!(SpawnArgs, path_len), 8);
        assert_eq!(offset_of!(SpawnArgs, args), 16);
        assert_eq!(offset_of!(SpawnArgs, args_len), 24);
        assert_eq!(offset_of!(SpawnArgs, cwd), 32);
        assert_eq!(offset_of!(SpawnArgs, cwd_len), 40);
        assert_eq!(offset_of!(SpawnArgs, fds), 48);
        assert_eq!(offset_of!(SpawnArgs, fd_count), 112);
        assert_eq!(offset_of!(SpawnArgs, flags), 116);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
    }

    #[test]
    fn a_program_s_bytes_read_back_as_its_struct() {
        let mut fds = [FdMap::default(); SPAWN_FDS];
        for (k, fd) in fds.iter_mut().enumerate() {
            *fd = FdMap {
                child: k as u32,
                parent: 100 + k as u32,
            };
        }
        let a = SpawnArgs {
            path: 0x40_1000,
            path_len: 11,
            args: 0x40_2000,
            args_len: 22,
            cwd: 0x40_3000,
            cwd_len: 33,
            fds,
            fd_count: 3,
            flags: NEW_GROUP,
        };
        // SAFETY: `SpawnArgs` is `repr(C)` of integers with no padding.
        let bytes: [u8; SpawnArgs::SIZE] = unsafe { core::mem::transmute(a) };
        assert_eq!(SpawnArgs::from_bytes(&bytes), a);
    }
}
