//! System calls (spec §7.3). Each wrapper passes its arguments as the ABI
//! says and decodes the result into a value or an error number.

use core::fmt;
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Stat, StatFs, WaitStatus, decode};

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
use crate::arch::syscall;

/// Off Relay OS (the host tests) there is no kernel to call.
#[cfg(not(all(target_arch = "x86_64", target_os = "none")))]
unsafe fn syscall(_call: Call, _args: [u64; 6]) -> u64 {
    unimplemented!("system calls exist only on Relay OS")
}

/// Makes call `c` with `args` (the rest zero): its value or error.
fn call(c: Call, args: &[u64]) -> Result<u64, u16> {
    let mut a = [0; 6];
    a[..args.len()].copy_from_slice(args);
    decode(unsafe { syscall(c, a) })
}

/// Sets the console's mode (`relay_abi::console`'s `MODE_RAW` or
/// `MODE_LINE`); the previous one.
pub fn console_mode(mode: u32) -> Result<u32, u16> {
    call(Call::ConsoleMode, &[u64::from(mode)]).map(|m| m as u32)
}

/// The console's columns and rows.
pub fn console_size() -> (u32, u32) {
    relay_abi::console::size_of_result(call(Call::ConsoleSize, &[]).unwrap_or(0))
}

/// Pushes `fd`, a file open for writing, as a console tee (spec §6.5):
/// it gets a copy of everything written to the console.
pub fn console_tee_push(fd: u32) -> Result<(), u16> {
    call(Call::ConsoleTeePush, &[u64::from(fd)]).map(|_| ())
}

/// Pops the newest tee this program pushed; the error of a write to it
/// that failed.
pub fn console_tee_pop() -> Result<(), u16> {
    call(Call::ConsoleTeePop, &[]).map(|_| ())
}

/// Gives the console to process group `pgid`.
pub fn console_foreground(pgid: u32) -> Result<(), u16> {
    call(Call::ConsoleForeground, &[u64::from(pgid)]).map(|_| ())
}

/// Maps `len` bytes (rounded up to pages) of fresh zeroed memory; its
/// address.
pub fn mem_map(len: usize) -> Result<usize, u16> {
    call(Call::MemMap, &[len as u64]).map(|a| a as usize)
}

/// Gives back `len` bytes (rounded up to pages) from `addr`, of memory
/// `mem_map` gave.
///
/// # Safety
/// Nothing may use that memory any more.
pub unsafe fn mem_unmap(addr: usize, len: usize) -> Result<(), u16> {
    call(Call::MemUnmap, &[addr as u64, len as u64]).map(|_| ())
}

/// Opens `path` with `relay_abi::file`'s `OPEN_*` flags: the new fd.
pub fn open(path: &[u8], flags: u32) -> Result<u32, u16> {
    call(
        Call::Open,
        &[path.as_ptr() as u64, path.len() as u64, u64::from(flags)],
    )
    .map(|fd| fd as u32)
}

/// Makes a pipe (spec §9.1): its read end and its write end, the two
/// lowest free fds.
pub fn pipe() -> Result<(u32, u32), u16> {
    let mut fds = [0u32; 2];
    call(Call::Pipe, &[&raw mut fds as u64]).map(|_| (fds[0], fds[1]))
}

/// Closes `fd`.
pub fn close(fd: u32) -> Result<(), u16> {
    call(Call::Close, &[u64::from(fd)]).map(|_| ())
}

/// Reads some of `fd` into `buf`; returns how many bytes, 0 at the end.
pub fn read(fd: u32, buf: &mut [u8]) -> Result<usize, u16> {
    call(
        Call::Read,
        &[u64::from(fd), buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// Moves `fd`'s offset (`relay_abi::file`'s `SEEK_*`); the new offset.
pub fn seek(fd: u32, offset: i64, whence: u32) -> Result<u64, u16> {
    call(
        Call::Seek,
        &[u64::from(fd), offset as u64, u64::from(whence)],
    )
}

/// What `fd` is.
pub fn fstat(fd: u32) -> Result<Stat, u16> {
    let mut st = Stat::default();
    call(Call::Fstat, &[u64::from(fd), &raw mut st as u64])?;
    Ok(st)
}

/// What `path` is (`relay_abi::file::STAT_NOFOLLOW` in `flags`: a link's
/// own status; links are never followed anyway).
pub fn stat(path: &[u8], flags: u32) -> Result<Stat, u16> {
    let mut st = Stat::default();
    let args = [
        path.as_ptr() as u64,
        path.len() as u64,
        u64::from(flags),
        &raw mut st as u64,
    ];
    call(Call::Stat, &args)?;
    Ok(st)
}

/// The directory `fd`'s next entries into `buf`, as `relay_abi::file`
/// records (`dir_entries` reads them); 0 after the last.
pub fn read_dir(fd: u32, buf: &mut [u8]) -> Result<usize, u16> {
    call(
        Call::ReadDir,
        &[u64::from(fd), buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// A call on one path.
fn on_path(c: Call, path: &[u8], rest: &[u64]) -> Result<u64, u16> {
    let mut a = [path.as_ptr() as u64, path.len() as u64, 0, 0];
    a[2..2 + rest.len()].copy_from_slice(rest);
    call(c, &a)
}

pub fn mkdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Mkdir, path, &[]).map(|_| ())
}

pub fn rmdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Rmdir, path, &[]).map(|_| ())
}

pub fn unlink(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Unlink, path, &[]).map(|_| ())
}

/// Sets the file's times to now; it must exist.
pub fn touch(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Touch, path, &[]).map(|_| ())
}

pub fn truncate(path: &[u8], size: u64) -> Result<(), u16> {
    on_path(Call::Truncate, path, &[size]).map(|_| ())
}

/// A symbolic link's target into `buf`, cut at its length; how many bytes.
pub fn readlink(path: &[u8], buf: &mut [u8]) -> Result<usize, u16> {
    on_path(
        Call::Readlink,
        path,
        &[buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

pub fn rename(from: &[u8], to: &[u8]) -> Result<(), u16> {
    let args = [
        from.as_ptr() as u64,
        from.len() as u64,
        to.as_ptr() as u64,
        to.len() as u64,
    ];
    call(Call::Rename, &args).map(|_| ())
}

/// The figures of the filesystem holding `path`.
pub fn statfs(path: &[u8]) -> Result<StatFs, u16> {
    let mut f = StatFs::default();
    on_path(Call::Statfs, path, &[&raw mut f as u64])?;
    Ok(f)
}

/// Makes every change to every filesystem durable.
pub fn sync() -> Result<(), u16> {
    call(Call::Sync, &[]).map(|_| ())
}

pub fn chdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Chdir, path, &[]).map(|_| ())
}

/// The current directory's path into `buf`; how many bytes (`ERANGE` if
/// it does not fit).
pub fn getcwd(buf: &mut [u8]) -> Result<usize, u16> {
    call(Call::Getcwd, &[buf.as_mut_ptr() as u64, buf.len() as u64]).map(|n| n as usize)
}

/// The status a program ends with once nobody reads its standard output
/// (spec §8.1): bash's for a program SIGPIPE ended, 128 + 13.
pub const BROKEN_PIPE: u8 = 141;

/// Writes some of `bytes` to `fd`; returns how many. A write to fd 1 that
/// fails with `EPIPE` ends the program at once with [`BROKEN_PIPE`] and no
/// message, as SIGPIPE ends one on Linux: nobody reads its output any
/// more (`cat big | head -n 1`).
pub fn write(fd: u32, bytes: &[u8]) -> Result<usize, u16> {
    let args = [
        u64::from(fd),
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
        0,
        0,
    ];
    let r = decode(unsafe { syscall(Call::Write, args) }).map(|n| n as usize);
    if ends_quietly(fd, r) {
        exit(BROKEN_PIPE);
    }
    r
}

/// Whether a write's result ends the program: a broken pipe on standard
/// output. Any other fd's `EPIPE` is the program's to see.
fn ends_quietly(fd: u32, r: Result<usize, u16>) -> bool {
    fd == 1 && r == Err(relay_abi::errno::EPIPE)
}

/// Writes all of `bytes` to `fd`, however many calls that takes.
pub fn write_all(fd: u32, bytes: &[u8]) -> Result<(), u16> {
    write_all_by(|b| write(fd, b), bytes)
}

/// Writes all of `bytes` through `write`; a write that takes nothing is
/// `ENOSPC`, as gnulib's `full_write` says of one (and `shell::Stdout`).
fn write_all_by(
    mut write: impl FnMut(&[u8]) -> Result<usize, u16>,
    mut bytes: &[u8],
) -> Result<(), u16> {
    while !bytes.is_empty() {
        match write(bytes)? {
            0 => return Err(relay_abi::errno::ENOSPC),
            n => bytes = &bytes[n.min(bytes.len())..],
        }
    }
    Ok(())
}

/// Starts the program at `path` with `args` (each followed by a NUL,
/// argument 0 first) and no environment in `cwd` (empty: this program's),
/// giving it the fds `fds` names (child, parent) and closing its others;
/// with `NEW_GROUP` in `flags` it starts a process group of its own, which
/// `FOREGROUND` also gives the console, in line mode; without it, a `pgid`
/// other than 0 is the group of another child of this program's that it
/// joins. Its pid.
pub fn spawn(
    path: &[u8],
    args: &[u8],
    cwd: &[u8],
    fds: &[FdMap],
    flags: u32,
    pgid: u32,
) -> Result<u32, u16> {
    spawn_env(path, args, b"", cwd, fds, flags, pgid)
}

/// [`spawn`] with the environment `env`: entries each followed by a NUL,
/// at most 64 KiB (programmable shell gate §8.1); `crate::env::block()`
/// passes on this program's own.
pub fn spawn_env(
    path: &[u8],
    args: &[u8],
    env: &[u8],
    cwd: &[u8],
    fds: &[FdMap],
    flags: u32,
    pgid: u32,
) -> Result<u32, u16> {
    if fds.len() > SPAWN_FDS {
        return Err(relay_abi::errno::EINVAL);
    }
    let mut a = SpawnArgs {
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        args: args.as_ptr() as u64,
        args_len: args.len() as u64,
        cwd: cwd.as_ptr() as u64,
        cwd_len: cwd.len() as u64,
        fd_count: fds.len() as u32,
        flags,
        pgid,
        env: env.as_ptr() as u64,
        env_len: env.len() as u64,
        ..SpawnArgs::default()
    };
    a.fds[..fds.len()].copy_from_slice(fds);
    let r = unsafe { syscall(Call::Spawn, [&raw const a as u64, 0, 0, 0, 0, 0]) };
    decode(r).map(|pid| pid as u32)
}

/// Waits for the child `pid` (or any, `relay_abi::spawn::WAIT_ANY`) to
/// end: its pid and how it ended. With `nohang`, `None` at once if none
/// has.
pub fn wait(pid: i64, nohang: bool) -> Result<Option<(u32, WaitStatus)>, u16> {
    wait_with(pid, if nohang { WAIT_NOHANG } else { 0 })
}

/// [`wait`] with `relay_abi::spawn`'s `WAIT_*` flags: with `WAIT_CTRL_C`,
/// a Ctrl-C typed while this program's group has the console in raw mode
/// ends the wait with `EINTR`.
pub fn wait_with(pid: i64, flags: u32) -> Result<Option<(u32, WaitStatus)>, u16> {
    let mut w = WaitStatus::default();
    let args = [pid as u64, u64::from(flags), &raw mut w as u64, 0, 0, 0];
    match decode(unsafe { syscall(Call::Wait, args) })? {
        0 => Ok(None),
        pid => Ok(Some((pid as u32, w))),
    }
}

/// Whether a Ctrl-C was typed while this program's group has the console
/// in raw mode; it is taken (`wait(0, WAIT_NOHANG | WAIT_CTRL_C)`).
pub fn take_ctrl_c() -> bool {
    use relay_abi::spawn::{WAIT_CTRL_C, WAIT_NOHANG};
    wait_with(0, WAIT_NOHANG | WAIT_CTRL_C) == Err(relay_abi::errno::EINTR)
}

/// Kills the process `target`, or the process group `-target`.
pub fn kill(target: i64) -> Result<(), u16> {
    decode(unsafe { syscall(Call::Kill, [target as u64, 0, 0, 0, 0, 0]) }).map(|_| ())
}

/// Fills `buf` with a `ProcInfo` per process, by pid, as many as fit
/// (`relay_abi::proc::PROC_MAX` entries always hold them all); how many
/// processes there are.
pub fn proc_list(buf: &mut [ProcInfo]) -> Result<usize, u16> {
    let len = core::mem::size_of_val(buf) as u64;
    call(Call::ProcList, &[buf.as_mut_ptr() as u64, len]).map(|n| n as usize)
}

/// This program's pid.
pub fn getpid() -> u32 {
    unsafe { syscall(Call::Getpid, [0; 6]) as u32 }
}

/// The memory figures of `free`.
pub fn memory() -> Result<MemInfo, u16> {
    let mut m = MemInfo::default();
    let len = core::mem::size_of::<MemInfo>() as u64;
    let kind = u64::from(relay_abi::info::INFO_MEMORY);
    let args = [kind, &raw mut m as u64, len, 0, 0, 0];
    decode(unsafe { syscall(Call::SysInfo, args) })?;
    Ok(m)
}

/// The system's names, as `uname` prints them.
pub fn uname() -> Result<relay_abi::Uname, u16> {
    let mut u = relay_abi::Uname::new(b"", b"", b"", b"");
    let len = relay_abi::Uname::SIZE as u64;
    let kind = u64::from(relay_abi::info::INFO_UNAME);
    call(Call::SysInfo, &[kind, &raw mut u as u64, len])?;
    Ok(u)
}

/// The newest bytes of the kernel log that fit in `buf` (it holds at most
/// `relay_abi::info::LOG_MAX`); how many.
pub fn kernel_log(buf: &mut [u8]) -> Result<usize, u16> {
    let kind = u64::from(relay_abi::info::INFO_LOG);
    call(
        Call::SysInfo,
        &[kind, buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// Restarts the machine or switches it off (`relay_abi::power`'s kinds and
/// `POWER_FORCE`) after shutting the filesystems down; returns only with
/// the error that kept it up.
pub fn power(kind: u32, flags: u32) -> u16 {
    match call(Call::Power, &[u64::from(kind), u64::from(flags)]) {
        Err(e) => e,
        Ok(_) => relay_abi::errno::EIO,
    }
}

/// The wall clock and the time since the machine started.
pub fn time() -> Result<relay_abi::Time, u16> {
    let mut t = relay_abi::Time::default();
    let args = [&raw mut t as u64, 0, 0, 0, 0, 0];
    decode(unsafe { syscall(Call::Time, args) })?;
    Ok(t)
}

/// Blocks the program for `ms` milliseconds.
pub fn sleep(ms: u64) {
    unsafe { syscall(Call::Sleep, [ms, 0, 0, 0, 0, 0]) };
}

/// Ends the program with status `code`.
pub fn exit(code: u8) -> ! {
    unsafe { syscall(Call::Exit, [u64::from(code), 0, 0, 0, 0, 0]) };
    // `exit` does not return.
    loop {
        core::hint::spin_loop();
    }
}

/// A file descriptor to `write!` to: `Fd(1)` is standard output, `Fd(2)`
/// standard error.
pub struct Fd(pub u32);

impl fmt::Write for Fd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_all(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use relay_abi::errno::{EIO, ENOSPC};

    #[test]
    fn only_a_broken_pipe_on_standard_output_ends_the_program() {
        use relay_abi::errno::EPIPE;
        assert!(ends_quietly(1, Err(EPIPE)));
        assert!(!ends_quietly(3, Err(EPIPE)), "a pipe of its own");
        assert!(!ends_quietly(2, Err(EPIPE)));
        assert!(!ends_quietly(1, Err(EIO)));
        assert!(!ends_quietly(1, Ok(0)));
        assert_eq!(BROKEN_PIPE, 128 + 13);
    }

    #[test]
    fn a_write_that_takes_nothing_is_enospc() {
        let mut got = Vec::new();
        let mut room = 5;
        let r = write_all_by(
            |b| {
                let n = b.len().min(room).min(3);
                room -= n;
                got.extend_from_slice(&b[..n]);
                Ok(n)
            },
            b"abcdefgh",
        );
        assert_eq!(r, Err(ENOSPC));
        assert_eq!(got, b"abcde", "everything that fitted, in pieces");
        assert_eq!(
            write_all_by(|_| Err(EIO), b"x"),
            Err(EIO),
            "the call's own error"
        );
        assert_eq!(write_all_by(|b| Ok(b.len()), b"all"), Ok(()));
        assert_eq!(write_all_by(|_| Ok(0), b""), Ok(()), "nothing to write");
    }
}
