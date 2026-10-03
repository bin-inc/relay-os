//! What `spawn` and `wait` take (spec §7.3): the new program's path,
//! arguments, working directory, file descriptors and process group, and
//! `wait`'s flags.

/// How many fds `spawn` can hand a child.
pub const SPAWN_FDS: usize = 8;

/// `SpawnArgs::flags`: the child starts a process group of its own, with
/// its pid as the group's number, instead of joining its parent's (spec
/// §6.4).
pub const NEW_GROUP: u32 = 1;
/// `SpawnArgs::flags`, with [`NEW_GROUP`]: the new group becomes the
/// console's foreground, in line mode, before the child runs, as an
/// interactive shell gives the console to its command (spec §6.4, §16
/// item 5). Without it the child might read the console before its parent
/// could hand it over, and get end of input.
pub const FOREGROUND: u32 = 2;

/// `wait`'s pid for any child.
pub const WAIT_ANY: i64 = -1;
/// `wait`'s flags: return 0 at once when no child has ended.
pub const WAIT_NOHANG: u32 = 1;
/// `wait`'s flags: a Ctrl-C typed while the caller's group has the console
/// in raw mode ends the wait with `EINTR`, and is taken from the input
/// (a shell's `wait` built-in, spec §9.2, §16 item 9). Without it a raw
/// Ctrl-C is only input, which the shell would read after the wait. With
/// [`WAIT_NOHANG`] and pid 0, `wait` only asks whether such a Ctrl-C was
/// typed: `EINTR` if one was, which it takes, 0 if not; no child is
/// collected (a shell between its own commands, programmable shell gate
/// §15 item 2).
pub const WAIT_CTRL_C: u32 = 2;

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
    /// [`NEW_GROUP`], with or without [`FOREGROUND`], or 0.
    pub flags: u32,
    /// Without [`NEW_GROUP`], the process group the child joins instead of
    /// its parent's (a pipeline's later stages join the first one's, spec
    /// §9.1); 0 for the parent's.
    pub pgid: u32,
    /// 0.
    pub reserved: u32,
    /// The child's environment: `NAME=value` entries, each followed by a
    /// NUL (programmable shell gate §8.1); empty for none.
    pub env: u64,
    pub env_len: u64,
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
            pgid: u32_at(120),
            reserved: u32_at(124),
            env: u64_at(128),
            env_len: u64_at(136),
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
        assert_eq!(SpawnArgs::SIZE, 144);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
        assert_eq!(offset_of!(SpawnArgs, path_len), 8);
        assert_eq!(offset_of!(SpawnArgs, args), 16);
        assert_eq!(offset_of!(SpawnArgs, args_len), 24);
        assert_eq!(offset_of!(SpawnArgs, cwd), 32);
        assert_eq!(offset_of!(SpawnArgs, cwd_len), 40);
        assert_eq!(offset_of!(SpawnArgs, fds), 48);
        assert_eq!(offset_of!(SpawnArgs, fd_count), 112);
        assert_eq!(offset_of!(SpawnArgs, flags), 116);
        assert_eq!(offset_of!(SpawnArgs, pgid), 120);
        assert_eq!(offset_of!(SpawnArgs, reserved), 124);
        assert_eq!(offset_of!(SpawnArgs, env), 128);
        assert_eq!(offset_of!(SpawnArgs, env_len), 136);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
        assert_eq!((FOREGROUND, WAIT_CTRL_C), (2, 2));
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
            pgid: 44,
            reserved: 55,
            env: 0x40_4000,
            env_len: 66,
        };
        // SAFETY: `SpawnArgs` is `repr(C)` of integers with no padding.
        let bytes: [u8; SpawnArgs::SIZE] = unsafe { core::mem::transmute(a) };
        assert_eq!(SpawnArgs::from_bytes(&bytes), a);
    }
}
