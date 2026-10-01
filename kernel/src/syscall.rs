//! The system-call dispatcher (user-space gate §7), architecture-neutral:
//! the `arch` entry stub hands it the call number and the six arguments,
//! and it answers with the result register's value or with the program's
//! exit. It checks and copies what the program passes (`UserSlice`,
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`, the pipe's in `pipes`.

use crate::exec::ARGS_MAX;
use crate::fd::{FdTable, File};
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use crate::proc::table::Group;
use alloc::sync::Arc;
use alloc::vec::Vec;
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::{Errno, Vfs};

mod files;
mod pipes;
#[cfg(test)]
mod testing;

/// The longest path a program may pass (Linux's `PATH_MAX`).
pub const PATH_MAX: usize = 4096;

/// A child to start, as the dispatcher checked it (spec §7.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub path: Vec<u8>,
    /// The arguments, each followed by a NUL, and how many there are (at
    /// least argument 0).
    pub args: Vec<u8>,
    pub argc: u64,
    /// Relative to the caller's current directory; empty for that one.
    pub cwd: Vec<u8>,
    pub fds: Vec<FdMap>,
    /// Its parent's group, a new one, or another child's (`SpawnArgs::pgid`).
    pub group: Group,
    /// The new group gets the console (`FOREGROUND`).
    pub foreground: bool,
}

/// Which child `wait` waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Any,
    Pid(u32),
}

/// What the dispatcher needs of the program that called: its memory, its
/// files, and the kernel's services.
pub trait Caller {
    /// Copies `buf.len()` bytes from `offset` into `slice`; `EFAULT` if
    /// they are not all the program's.
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno>;
    /// Copies `bytes` into `slice` from `offset`; `EFAULT`, and nothing
    /// written, if they are not all the program's and writable.
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// Whether `slice` is all the program's and writable (`EFAULT`
    /// otherwise), without writing it.
    fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno>;
    /// A byte string of the program's, copied in.
    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno>;
    /// Runs `f` with the program's fds. `f` must not block.
    fn with_fds<R>(&mut self, f: impl FnOnce(&mut FdTable) -> R) -> R;
    /// Runs `f` with the files, from the program's current directory.
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R;
    /// The most bytes the kernel's heap may give one allocation now.
    fn heap_room(&self) -> usize;
    /// Maps `pages` fresh pages in the `mem_map` area; their address.
    fn mem_map(&mut self, pages: u64) -> Result<u64, Errno>;
    /// Gives back `pages` pages from `addr`, which `mem_map` gave.
    fn mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno>;
    /// Writes `bytes` to the screen.
    fn console_write(&mut self, bytes: &[u8]);
    /// Reads the console into `buf` (spec §6.4, §6.5), waiting for input:
    /// 0 at once for a process outside the foreground group, and at end of
    /// input; `EINTR` if the program was killed while it waited.
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
    /// A new pipe: its read end and its write end (spec §9.1); `ENOMEM`
    /// when its ring's frames would eat into the reserve.
    fn new_pipe(&mut self) -> Result<(crate::pipe::End, crate::pipe::End), Errno>;
    /// Waits until the pipe `id` changes (data, room, an end closed);
    /// `EINTR` if the program was killed meanwhile (or before).
    fn pipe_wait(&mut self, id: u64) -> Result<(), Errno>;
    /// Wakes whoever waits on the pipe `id`.
    fn pipe_wake(&mut self, id: u64);
    /// Line mode (`true`) or raw mode; the previous one. `EPERM` unless
    /// the program's group holds the console (spec §16 item 9).
    fn console_mode(&mut self, line: bool) -> Result<bool, Errno>;
    /// The console's columns and rows.
    fn console_size(&self) -> (u32, u32);
    /// Makes `pgid` the foreground group: `ESRCH` if no process is in
    /// `pgid`, `EPERM` unless the program's group holds the console.
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
    /// Pushes `file` as a console tee of the program (spec §6.5).
    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno>;
    /// Pops the newest tee the program pushed.
    fn tee_pop(&mut self) -> Result<(), Errno>;
    /// Writes what waits for the tees, then syncs every filesystem.
    fn sync(&mut self) -> Result<(), Errno>;
    /// The kernel log.
    fn kernel_log(&self) -> Vec<u8>;
    /// Syncs and shuts the filesystems down, then restarts (`reboot`) or
    /// switches the machine off; returns only the shutdown's error, unless
    /// `force` goes ahead anyway (spec §7.3).
    fn power(&mut self, reboot: bool, force: bool) -> Errno;
    /// Starts a child; its pid.
    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno>;
    /// A child that has ended, with how; `None` if `nohang` and none has.
    /// `ECHILD` if there is no such child.
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
    fn kill(&mut self, target: i64) -> Result<(), Errno>;
    fn pid(&self) -> u32;
    /// Every process, by pid, as `ps` shows it (spec §9.3).
    fn processes(&mut self) -> Vec<ProcInfo>;
    /// The memory figures of `free`.
    fn memory(&self) -> MemInfo;
    /// The wall clock and the uptime.
    fn time(&self) -> Time;
    /// Blocks the program for `ms` milliseconds.
    fn sleep(&mut self, ms: u64);
}

/// How a call ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Back to the program with this result register.
    Return(u64),
    /// The program called `exit` with this code.
    Exit(u8),
}

/// Serves call `number` with `args` for `caller`.
pub fn dispatch(caller: &mut impl Caller, number: u64, args: [u64; 6]) -> Outcome {
    let result = match Call::from_number(number) {
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Spawn) => spawn(caller, args[0]),
        Some(Call::Wait) => wait(caller, args[0] as i64, args[1], args[2]),
        Some(Call::Kill) => caller.kill(args[0] as i64).map(|()| 0),
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::ProcList) => proc_list(caller, args[0], args[1]),
        Some(Call::MemMap) => mem_map(caller, args[0]),
        Some(Call::MemUnmap) => mem_unmap(caller, args[0], args[1]),
        Some(Call::Open) => files::open(caller, args[0], args[1], args[2]),
        Some(Call::Close) => files::close(caller, args[0]),
        Some(Call::Read) => files::read(caller, args[0], args[1], args[2]),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Seek) => files::seek(caller, args[0], args[1] as i64, args[2]),
        Some(Call::Fstat) => files::fstat(caller, args[0], args[1]),
        Some(Call::Stat) => files::stat(caller, args[0], args[1], args[2], args[3]),
        Some(Call::ReadDir) => files::read_dir(caller, args[0], args[1], args[2]),
        Some(Call::Mkdir) => files::on_path(caller, args[0], args[1], |v, p| v.mkdir(p)),
        Some(Call::Rmdir) => files::on_path(caller, args[0], args[1], |v, p| v.rmdir(p)),
        Some(Call::Unlink) => files::on_path(caller, args[0], args[1], |v, p| v.unlink(p)),
        Some(Call::Truncate) => files::truncate(caller, args[0], args[1], args[2]),
        Some(Call::Touch) => files::on_path(caller, args[0], args[1], files::touch),
        Some(Call::Readlink) => files::readlink(caller, args[0], args[1], args[2], args[3]),
        Some(Call::Rename) => files::rename(caller, [args[0], args[1], args[2], args[3]]),
        Some(Call::Statfs) => files::statfs(caller, args[0], args[1], args[2]),
        Some(Call::Sync) => caller.sync().map(|()| 0),
        Some(Call::Chdir) => files::on_path(caller, args[0], args[1], |v, p| v.chdir(p)),
        Some(Call::Getcwd) => files::getcwd(caller, args[0], args[1]),
        Some(Call::ConsoleMode) => console_mode(caller, args[0]),
        Some(Call::ConsoleSize) => {
            let (columns, rows) = caller.console_size();
            Ok(relay_abi::console::size_result(columns, rows))
        }
        Some(Call::ConsoleTeePush) => file(caller, args[0])
            .and_then(|f| caller.tee_push(f))
            .map(|()| 0),
        Some(Call::ConsoleTeePop) => caller.tee_pop().map(|()| 0),
        Some(Call::ConsoleForeground) => {
            let pgid = u32::try_from(args[0]).ok().filter(|&g| g != 0);
            pgid.ok_or(Errno::ESRCH)
                .and_then(|g| caller.console_foreground(g))
                .map(|()| 0)
        }
        Some(Call::Time) => time(caller, args[0]),
        Some(Call::Sleep) => {
            caller.sleep(args[0]);
            Ok(0)
        }
        Some(Call::SysInfo) => sys_info(caller, args[0], args[1], args[2]),
        Some(Call::Power) => power(caller, args[0], args[1]),
        Some(Call::Pipe) => pipes::pipe(caller, args[0]),
        _ => Err(Errno::ENOSYS),
    };
    Outcome::Return(encode(result.map_err(Errno::number)))
}

/// `spawn(&SpawnArgs)` (spec §7.3): the struct, then the path, the
/// arguments and the working directory it points at, each checked and
/// copied in before anything starts.
fn spawn(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let mut raw = [0u8; SpawnArgs::SIZE];
    caller.read(&UserSlice::new(addr, raw.len() as u64)?, 0, &mut raw)?;
    let a = SpawnArgs::from_bytes(&raw);
    let foreground = a.flags & FOREGROUND != 0;
    let new_group = a.flags & NEW_GROUP != 0;
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && !new_group)
        || (new_group && a.pgid != 0)
        || a.fd_count as usize > SPAWN_FDS
        || a.reserved != 0
    {
        return Err(Errno::EINVAL);
    }
    let path = caller.read_str(&UserStr::new(
        a.path,
        a.path_len,
        PATH_MAX,
        Errno::ENAMETOOLONG,
    )?)?;
    let args = caller.read_str(&UserStr::new(a.args, a.args_len, ARGS_MAX, Errno::E2BIG)?)?;
    // Argument 0 at least, and every argument ends with its NUL.
    if args.last() != Some(&0) {
        return Err(Errno::EINVAL);
    }
    let argc = args.iter().filter(|&&b| b == 0).count() as u64;
    let cwd = caller.read_str(&UserStr::new(
        a.cwd,
        a.cwd_len,
        PATH_MAX,
        Errno::ENAMETOOLONG,
    )?)?;
    let s = Spawn {
        path,
        args,
        argc,
        cwd,
        fds: a.fds[..a.fd_count as usize].to_vec(),
        group: match a.pgid {
            _ if new_group => Group::New,
            0 => Group::Parent,
            g => Group::Join(g),
        },
        foreground,
    };
    caller.spawn(&s).map(u64::from)
}

/// `wait(pid or -1, flags, &mut WaitStatus or 0)` (spec §7.3). The status's
/// memory is checked before a child is collected, so a bad pointer never
/// loses a child's status.
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
    if flags & !u64::from(WAIT_NOHANG) != 0 {
        return Err(Errno::EINVAL);
    }
    let child = match pid {
        WAIT_ANY => Child::Any,
        p if p > 0 => Child::Pid(u32::try_from(p).map_err(|_| Errno::ECHILD)?),
        _ => return Err(Errno::EINVAL),
    };
    let size = core::mem::size_of::<WaitStatus>() as u64;
    let status = match addr {
        0 => None,
        a => Some(UserSlice::new(a, size)?),
    };
    if let Some(slice) = &status {
        caller.writable(slice)?;
    }
    let Some((pid, w)) = caller.wait(child, flags & u64::from(WAIT_NOHANG) != 0)? else {
        return Ok(0);
    };
    if let Some(slice) = &status {
        let mut bytes = [0u8; 32];
        for (i, v) in [w.how, w.code, w.fault, w.detail].iter().enumerate() {
            bytes[4 * i..4 * i + 4].copy_from_slice(&v.to_ne_bytes());
        }
        bytes[16..24].copy_from_slice(&w.address.to_ne_bytes());
        bytes[24..].copy_from_slice(&w.ip.to_ne_bytes());
        caller.write(slice, 0, &bytes)?;
    }
    Ok(u64::from(pid))
}

/// `proc_list(buffer, length)` (spec §7.3, §9.3): a `ProcInfo` for each
/// process, by pid, as many as the buffer holds; how many processes there
/// are, so a caller whose buffer was too short knows. Nothing is written
/// unless all that fits can be (`EFAULT`).
fn proc_list(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let procs = caller.processes();
    let size = ProcInfo::SIZE as u64;
    let fit = (len / size).min(procs.len() as u64);
    if fit > 0 {
        let slice = UserSlice::new(addr, fit * size)?;
        let bytes: Vec<u8> = procs[..fit as usize]
            .iter()
            .flat_map(|p| p.to_bytes())
            .collect();
        caller.write(&slice, 0, &bytes)?;
    }
    Ok(procs.len() as u64)
}

/// The file open as `fd`.
fn file(caller: &mut impl Caller, fd: u64) -> Result<Arc<File>, Errno> {
    caller.with_fds(|t| t.get(fd).cloned())
}

/// `write(fd, buffer, length)`. Copies the buffer in a page at a time; a
/// page that is not the program's, or the file's error, ends the call with
/// the bytes written before it, or with the error if there were none (a
/// full disk takes what fits, then says `ENOSPC`).
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    match &*file {
        File::Vfs(open) if !open.is_writable() => return Err(Errno::EBADF),
        File::Pipe(end) => return pipes::write(caller, end, addr, len),
        _ => {}
    }
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        // To the end of the page, so a bad page costs only its own bytes.
        // `UserSlice::new` checked that `addr + len` does not overflow.
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        let written = caller
            .read(&slice, done, &mut buf[..n])
            .and_then(|()| write_to(caller, &file, &buf[..n]));
        match written {
            Ok(k) => done += k as u64,
            Err(e) if done == 0 => return Err(e),
            Err(_) => break,
        }
    }
    Ok(done)
}

/// Writes `bytes` to `file`: how many it took.
fn write_to(caller: &mut impl Caller, file: &Arc<File>, bytes: &[u8]) -> Result<usize, Errno> {
    match &**file {
        File::Console => {
            caller.console_write(bytes);
            Ok(bytes.len())
        }
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
        File::Pipe(_) => unreachable!("write takes pipes apart"),
    }
}

/// `console_mode(mode)` (spec §7.3): the previous mode.
fn console_mode(caller: &mut impl Caller, mode: u64) -> Result<u64, Errno> {
    use relay_abi::console::{MODE_LINE, MODE_RAW};
    let line = match u32::try_from(mode) {
        Ok(MODE_RAW) => false,
        Ok(MODE_LINE) => true,
        _ => return Err(Errno::EINVAL),
    };
    let was = caller.console_mode(line)?;
    Ok(u64::from(if was { MODE_LINE } else { MODE_RAW }))
}

/// `mem_map(length)` (spec §7.3): fresh zeroed read-write pages, the
/// length rounded up to whole pages; their address. `EINVAL` for nothing,
/// `ENOMEM` when they do not fit or the frames would run too low.
fn mem_map(caller: &mut impl Caller, len: u64) -> Result<u64, Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    caller.mem_map(len.div_ceil(PAGE))
}

/// `mem_unmap(address, length)` (spec §7.3): whole pages of earlier
/// `mem_map`s, the length rounded up to pages; `EINVAL` otherwise.
fn mem_unmap(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    caller.mem_unmap(addr, len.div_ceil(PAGE)).map(|()| 0)
}

/// `time(&mut Time)` (spec §7.3).
fn time(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let t = caller.time();
    let mut bytes = [0u8; size_of::<Time>()];
    bytes[..8].copy_from_slice(&t.unix_seconds.to_ne_bytes());
    bytes[8..].copy_from_slice(&t.uptime_ns.to_ne_bytes());
    caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
    Ok(0)
}

/// `power(kind, flags)` (spec §7.3): returns only with the error that kept
/// the machine up.
fn power(caller: &mut impl Caller, kind: u64, flags: u64) -> Result<u64, Errno> {
    use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
    let reboot = match u32::try_from(kind) {
        Ok(POWER_REBOOT) => true,
        Ok(POWER_POWEROFF) => false,
        _ => return Err(Errno::EINVAL),
    };
    if flags & !u64::from(POWER_FORCE) != 0 {
        return Err(Errno::EINVAL);
    }
    Err(caller.power(reboot, flags & u64::from(POWER_FORCE) != 0))
}

/// `sys_info(kind, buffer, length)` (spec §7.3): the bytes written. The
/// memory figures and the names need room for their whole struct; the
/// kernel log gives its newest bytes that fit.
fn sys_info(caller: &mut impl Caller, kind: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    use relay_abi::info::{INFO_LOG, INFO_UNAME};
    match u32::try_from(kind) {
        Ok(INFO_MEMORY) => {}
        Ok(INFO_UNAME) => {
            let u = relay_abi::Uname::new(
                b"Relay",
                b"relay",
                env!("CARGO_PKG_VERSION").as_bytes(),
                crate::arch::MACHINE.as_bytes(),
            );
            let bytes = u.to_bytes();
            if len < bytes.len() as u64 {
                return Err(Errno::EINVAL);
            }
            caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
            return Ok(bytes.len() as u64);
        }
        Ok(INFO_LOG) => {
            let slice = UserSlice::new(addr, len)?;
            let log = caller.kernel_log();
            let n = log.len().min(len as usize);
            caller.write(&slice, 0, &log[log.len() - n..])?;
            return Ok(n as u64);
        }
        _ => return Err(Errno::EINVAL),
    }
    let m = caller.memory();
    let mut bytes = [0u8; size_of::<MemInfo>()];
    for (i, v) in [m.ram_total, m.ram_free, m.heap_total, m.heap_used]
        .iter()
        .enumerate()
    {
        bytes[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
    }
    if len < bytes.len() as u64 {
        return Err(Errno::EINVAL);
    }
    caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::mm::paging::PAGE;
    use relay_abi::errno;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE};

    /// A `SpawnArgs` at `W` naming a path, arguments and a working
    /// directory stored after it; `edit` changes it first.
    fn spawn_args(f: &mut Fake, args: &[u8], edit: impl FnOnce(&mut SpawnArgs)) -> u64 {
        let (path, cwd) = (b"/bin/t-args", b"sub");
        put(f, W + 200, path);
        put(f, W + 300, cwd);
        put(f, W + 400, args);
        let mut fds = [FdMap::default(); SPAWN_FDS];
        fds[0] = FdMap {
            child: 1,
            parent: 2,
        };
        fds[1] = FdMap {
            child: 2,
            parent: 2,
        };
        let mut a = SpawnArgs {
            path: W + 200,
            path_len: path.len() as u64,
            args: W + 400,
            args_len: args.len() as u64,
            cwd: W + 300,
            cwd_len: cwd.len() as u64,
            fds,
            fd_count: 2,
            flags: NEW_GROUP,
            pgid: 0,
            reserved: 0,
        };
        edit(&mut a);
        // SAFETY: `SpawnArgs` is `repr(C)` of integers with no padding.
        let bytes: [u8; SpawnArgs::SIZE] = unsafe { core::mem::transmute(a) };
        put(f, W, &bytes);
        W
    }

    #[test]
    fn exit_ends_the_program_with_its_code() {
        let mut f = fake();
        assert_eq!(dispatch(&mut f, 1, [7, 0, 0, 0, 0, 0]), Outcome::Exit(7));
        // The code is a byte.
        assert_eq!(
            dispatch(&mut f, 1, [0x1_02, 0, 0, 0, 0, 0]),
            Outcome::Exit(2)
        );
    }

    #[test]
    fn write_copies_the_buffer_to_the_fd() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [1, U + 10, 5]), Ok(5));
        assert_eq!(call(&mut f, Call::Write, [2, U, 3]), Ok(3));
        assert_eq!(call(&mut f, Call::Write, [0, U, 1]), Ok(1), "fd 0 too");
        assert_eq!(text(&f), [10, 11, 12, 13, 14, 0, 1, 2, 0], "the console");
        // Across pages, in pieces of at most 4 KiB.
        f.written.clear();
        assert_eq!(call(&mut f, Call::Write, [1, U + 100, 10_000]), Ok(10_000));
        let want: Vec<u8> = (100..10_100).map(|i| (i % 251) as u8).collect();
        assert_eq!(text(&f), want);
        assert!(f.written.iter().all(|b| b.len() <= 4096));
        assert_eq!(
            call(&mut f, Call::Write, [1, 0, 0]),
            Ok(0),
            "nothing, anywhere"
        );
    }

    #[test]
    fn an_fd_that_is_not_open_is_ebadf() {
        let mut f = fake();
        for fd in [3, 31, 1 << 32 | 1, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 1]),
                Err(errno::EBADF),
                "{fd}"
            );
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 0]),
                Err(errno::EBADF),
                "{fd}, nothing to write"
            );
        }
        assert!(f.written.is_empty());
    }

    #[test]
    fn a_bad_buffer_is_efault_or_a_short_write() {
        let mut f = fake();
        let end = U + 4 * PAGE;
        assert_eq!(
            call(&mut f, Call::Write, [1, 0, 1]),
            Err(errno::EFAULT),
            "null"
        );
        assert_eq!(call(&mut f, Call::Write, [1, end, 1]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Write, [1, 0xFFFF_FFFF_8000_0000, 8]),
            Err(errno::EFAULT),
            "the kernel"
        );
        assert_eq!(
            call(&mut f, Call::Write, [1, U, u64::MAX]),
            Err(errno::EFAULT)
        );
        assert!(f.written.is_empty());
        // Every byte before the hole is written, as on Linux.
        assert_eq!(call(&mut f, Call::Write, [1, end - 5000, 9000]), Ok(5000));
        let want: Vec<u8> = (4 * PAGE - 5000..3 * PAGE)
            .map(|i| (i % 251) as u8)
            .chain([0; PAGE as usize])
            .collect();
        assert_eq!(text(&f), want);
        f.written.clear();
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 904, 2000]),
            Ok(904),
            "less than a page before the hole"
        );
        assert_eq!(text(&f).len(), 904);
    }

    #[test]
    fn spawn_copies_in_everything_its_struct_names() {
        let mut f = fake();
        let a = spawn_args(&mut f, b"t-args\0a\0\0", |_| {});
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(101));
        assert_eq!(
            f.spawned,
            [Spawn {
                path: b"/bin/t-args".to_vec(),
                args: b"t-args\0a\0\0".to_vec(),
                argc: 3,
                cwd: b"sub".to_vec(),
                fds: vec![
                    FdMap {
                        child: 1,
                        parent: 2
                    },
                    FdMap {
                        child: 2,
                        parent: 2
                    }
                ],
                group: Group::New,
                foreground: false,
            }]
        );
        let a = spawn_args(&mut f, b"x\0", |a| {
            a.flags = 0;
            a.cwd_len = 0;
            a.fd_count = 0;
        });
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(102));
        let s = &f.spawned[1];
        assert!(s.group == Group::Parent && s.cwd.is_empty() && s.fds.is_empty());
        assert_eq!(s.argc, 1);
        // A group of its own that gets the console.
        let a = spawn_args(&mut f, b"x\0", |a| a.flags = NEW_GROUP | FOREGROUND);
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(103));
        assert!(f.spawned[2].group == Group::New && f.spawned[2].foreground);
        // Another child's group (whether it may is the table's to say).
        let a = spawn_args(&mut f, b"x\0", |a| {
            a.flags = 0;
            a.pgid = 102;
        });
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(104));
        assert_eq!(f.spawned[3].group, Group::Join(102));
        // The caller's refusal.
        let a = spawn_args(&mut f, b"x\0", |a| a.path_len = 7);
        put(&mut f, W + 200, b"missing");
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Err(errno::ENOENT));
    }

    #[test]
    fn spawn_refuses_what_is_not_a_valid_request() {
        let mut f = fake();
        let refused = |f: &mut Fake, args: &[u8], edit: fn(&mut SpawnArgs)| {
            let a = spawn_args(f, args, edit);
            call(f, Call::Spawn, [a, 0, 0])
        };
        assert_eq!(refused(&mut f, b"x", |_| {}), Err(errno::EINVAL), "no NUL");
        assert_eq!(
            refused(&mut f, b"", |_| {}),
            Err(errno::EINVAL),
            "no argument 0"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.flags = FOREGROUND),
            Err(errno::EINVAL),
            "the console goes to a group of the child's own"
        );
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.pgid = 2),
            Err(errno::EINVAL),
            "a new group and another child's"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| {
                a.flags = FOREGROUND;
                a.pgid = 2;
            }),
            Err(errno::EINVAL),
            "the console goes to a new group only"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.reserved = 1),
            Err(errno::EINVAL),
            "reserved"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.fd_count = 9),
            Err(errno::EINVAL)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.path_len = PATH_MAX as u64 + 1),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.cwd_len = PATH_MAX as u64 + 1),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.args_len = ARGS_MAX as u64 + 1),
            Err(errno::E2BIG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.args = 0),
            Err(errno::EFAULT),
            "arguments at null"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.path = u64::MAX),
            Err(errno::EFAULT)
        );
        assert_eq!(call(&mut f, Call::Spawn, [0, 0, 0]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Spawn, [W + PAGE - 8, 0, 0]),
            Err(errno::EFAULT),
            "the struct runs off the page"
        );
        assert!(f.spawned.is_empty());
    }

    #[test]
    fn wait_copies_out_how_the_child_ended() {
        let mut f = fake();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0x10, 0x40_1a2c);
        f.ended = vec![(7, WaitStatus::exited(3)), (8, fault)];
        assert_eq!(call(&mut f, Call::Wait, [8, 0, W + 64]), Ok(8));
        let b = get(&mut f, W + 64, 32);
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        assert_eq!(
            (
                u32_at(0),
                u32_at(4),
                u32_at(8),
                u32_at(12),
                u64_at(16),
                u64_at(24)
            ),
            (
                fault.how,
                fault.code,
                fault.fault,
                fault.detail,
                0x10,
                0x40_1a2c
            )
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-1i64 as u64, 0, 0]),
            Ok(7),
            "any, no status"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-1i64 as u64, 0, 0]),
            Err(errno::ECHILD)
        );
        f.running = true;
        assert_eq!(
            call(
                &mut f,
                Call::Wait,
                [-1i64 as u64, u64::from(WAIT_NOHANG), W]
            ),
            Ok(0),
            "nothing has ended"
        );
    }

    #[test]
    fn proc_list_gives_what_fits_and_how_many_there_are() {
        use relay_abi::proc::{STATE_RUN, STATE_SLEEP, STATE_WAIT};
        let mut f = fake();
        f.procs = alloc::vec![
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 5, b"init"),
            ProcInfo::new(2, 1, 2, STATE_WAIT, 300, 70, b"/bin/sh"),
            ProcInfo::new(9, 2, 9, STATE_RUN, 41, 2, b"/bin/ps"),
            ProcInfo::new(12, 2, 12, STATE_SLEEP, 40, 0, b"/bin/sleep"),
        ];
        let size = ProcInfo::SIZE as u64;
        let all: Vec<u8> = f.procs.iter().flat_map(|p| p.to_bytes()).collect();
        assert_eq!(call(&mut f, Call::ProcList, [W, 4 * size, 0]), Ok(4));
        assert_eq!(get(&mut f, W, all.len()), all);
        // A buffer too short gets what fits, whole entries only, and the
        // count of all.
        put(&mut f, W, &[0xAA; 4 * 96]);
        assert_eq!(call(&mut f, Call::ProcList, [W, 3 * size - 1, 0]), Ok(4));
        assert_eq!(get(&mut f, W, 2 * 96), all[..2 * 96]);
        assert_eq!(
            get(&mut f, W + 2 * size, 96),
            [0xAA; 96],
            "nothing of the third"
        );
        assert_eq!(
            call(&mut f, Call::ProcList, [0, 0, 0]),
            Ok(4),
            "just the count"
        );
        assert_eq!(call(&mut f, Call::ProcList, [0, size - 1, 0]), Ok(4));
        // More room than processes takes only what they need.
        assert_eq!(call(&mut f, Call::ProcList, [W, 40 * size, 0]), Ok(4));
    }

    #[test]
    fn proc_list_writes_nothing_into_memory_that_is_not_all_the_program_s() {
        let mut f = fake();
        f.procs = (1..=3)
            .map(|pid| ProcInfo::new(pid, 0, pid, 1, 0, 0, b"p"))
            .collect();
        let size = ProcInfo::SIZE as u64;
        assert_eq!(
            call(&mut f, Call::ProcList, [U, size, 0]),
            Err(errno::EFAULT),
            "read-only"
        );
        let end = W + PAGE - size - 10;
        put(&mut f, end, &[0xAA; 96]);
        assert_eq!(
            call(&mut f, Call::ProcList, [end, 2 * size, 0]),
            Err(errno::EFAULT),
            "the second runs past the page"
        );
        assert_eq!(get(&mut f, end, 96), [0xAA; 96], "not even the first");
        assert_eq!(
            call(&mut f, Call::ProcList, [0xFFFF_8000_0000_0000, size, 0]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn a_bad_status_pointer_loses_no_child() {
        let mut f = fake();
        f.ended = vec![(7, WaitStatus::exited(3))];
        for bad in [U, W + PAGE - 16, 0xFFFF_8000_0000_0000, u64::MAX - 8] {
            assert_eq!(
                call(&mut f, Call::Wait, [7, 0, bad]),
                Err(errno::EFAULT),
                "{bad:#x}"
            );
        }
        assert_eq!(f.ended.len(), 1, "still there");
        assert_eq!(call(&mut f, Call::Wait, [7, 0, W]), Ok(7));
    }

    #[test]
    fn wait_refuses_what_it_cannot_mean() {
        let mut f = fake();
        f.ended = vec![(7, WaitStatus::exited(3))];
        assert_eq!(
            call(&mut f, Call::Wait, [0, 0, 0]),
            Err(errno::EINVAL),
            "a group"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-2i64 as u64, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::Wait, [7, 2, 0]),
            Err(errno::EINVAL),
            "flags"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [(1 << 32) + 7, 0, 0]),
            Err(errno::ECHILD),
            "no such pid, and not 7"
        );
        assert_eq!(f.ended.len(), 1);
    }

    #[test]
    fn kill_getpid_and_the_memory_figures() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Kill, [5, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Kill, [-5i64 as u64, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Kill, [1, 0, 0]), Err(errno::EPERM));
        assert_eq!(f.killed, [5, -5, 1]);
        assert_eq!(call(&mut f, Call::Getpid, [0, 0, 0]), Ok(42));
        let info = u64::from(INFO_MEMORY);
        assert_eq!(call(&mut f, Call::SysInfo, [info, W, 32]), Ok(32));
        let b = get(&mut f, W, 32);
        let at = |i: usize| u64::from_ne_bytes(b[8 * i..8 * i + 8].try_into().unwrap());
        assert_eq!(
            [at(0), at(1), at(2), at(3)],
            [MEM.ram_total, MEM.ram_free, MEM.heap_total, MEM.heap_used]
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [info, W, 31]),
            Err(errno::EINVAL)
        );
        assert_eq!(call(&mut f, Call::SysInfo, [4, W, 32]), Err(errno::EINVAL));
        assert_eq!(
            call(&mut f, Call::SysInfo, [info, U, 32]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn time_fills_in_the_clock_s_answer() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Time, [W + 8, 0, 0]), Ok(0));
        let buf = get(&mut f, W + 8, 16);
        assert_eq!(buf[..8], NOW.unix_seconds.to_ne_bytes());
        assert_eq!(buf[8..], NOW.uptime_ns.to_ne_bytes());
        for bad in [0, U, W + PAGE - 8, u64::MAX - 4] {
            assert_eq!(
                call(&mut f, Call::Time, [bad, 0, 0]),
                Err(errno::EFAULT),
                "{bad:#x}"
            );
        }
    }

    #[test]
    fn sleep_blocks_for_what_it_is_asked() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Sleep, [250, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Sleep, [0, 0, 0]), Ok(0));
        assert_eq!(f.slept, [250, 0]);
    }

    #[test]
    fn mem_map_gives_whole_pages_and_mem_unmap_takes_them_back() {
        use crate::mm::space::MAP_START;
        let mut f = fake();
        assert_eq!(
            call(&mut f, Call::MemMap, [1, 0, 0]),
            Ok(MAP_START),
            "a page for a byte"
        );
        assert_eq!(
            call(&mut f, Call::MemMap, [PAGE + 1, 0, 0]),
            Ok(MAP_START + PAGE)
        );
        assert_eq!(f.space.maps(), [(MAP_START, 3)]);
        assert_eq!(call(&mut f, Call::MemMap, [0, 0, 0]), Err(errno::EINVAL));
        assert_eq!(
            call(&mut f, Call::MemMap, [u64::MAX, 0, 0]),
            Err(errno::ENOMEM)
        );
        f.room = 2;
        assert_eq!(
            call(&mut f, Call::MemMap, [3 * PAGE, 0, 0]),
            Err(errno::ENOMEM),
            "the reserve"
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START + PAGE, 1, 0]),
            Ok(0),
            "a byte is its page"
        );
        assert_eq!(f.space.maps(), [(MAP_START, 1), (MAP_START + 2 * PAGE, 1)]);
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START + 1, 1, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [U, PAGE, 0]),
            Err(errno::EINVAL),
            "not mem_map's"
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START, u64::MAX, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(f.unmapped, [(MAP_START + PAGE, 1)], "flushed once");
    }

    #[test]
    fn the_console_s_mode_size_and_foreground() {
        use relay_abi::console::{MODE_LINE, MODE_RAW, size_of_result};
        let mut f = fake();
        let line = u64::from(MODE_LINE);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [line, 0, 0]),
            Ok(u64::from(MODE_RAW))
        );
        assert!(f.line_mode);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [0, 0, 0]),
            Ok(line),
            "the previous one"
        );
        assert!(!f.line_mode);
        for bad in [2, 1 << 32, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::ConsoleMode, [bad, 0, 0]),
                Err(errno::EINVAL)
            );
        }
        let size = call(&mut f, Call::ConsoleSize, [0, 0, 0]).unwrap();
        assert_eq!(size_of_result(size), (120, 33));
        assert_eq!(call(&mut f, Call::ConsoleForeground, [42, 0, 0]), Ok(0));
        assert_eq!(f.foreground, 42);
        for bad in [0, 7, 1 << 32 | 42, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::ConsoleForeground, [bad, 0, 0]),
                Err(errno::ESRCH),
                "{bad}"
            );
        }
        assert_eq!(f.foreground, 42);
    }

    #[test]
    fn the_console_is_changed_only_by_a_group_that_holds_it() {
        use relay_abi::console::MODE_LINE;
        let mut f = fake();
        f.holds_console = false;
        let line = u64::from(MODE_LINE);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [line, 0, 0]),
            Err(errno::EPERM)
        );
        assert!(!f.line_mode, "the mode stays");
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [7, 0, 0]),
            Err(errno::EINVAL),
            "a mode that is none, first"
        );
        assert_eq!(
            call(&mut f, Call::ConsoleForeground, [42, 0, 0]),
            Err(errno::EPERM)
        );
        assert_eq!(f.foreground, 1, "the group stays");
        // The tees are not the console's state: anyone may push one.
        assert_eq!(call(&mut f, Call::ConsoleTeePush, [1, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::ConsoleTeePop, [0, 0, 0]), Ok(0));
    }

    #[test]
    fn tees_are_pushed_by_fd_and_popped() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::ConsoleTeePush, [1, 0, 0]), Ok(0));
        assert_eq!(f.tees.len(), 1);
        assert!(
            Arc::ptr_eq(&f.tees[0], f.fds.get(1).unwrap()),
            "the fd's file"
        );
        assert_eq!(
            call(&mut f, Call::ConsoleTeePush, [9, 0, 0]),
            Err(errno::EBADF)
        );
        assert_eq!(call(&mut f, Call::ConsoleTeePop, [0, 0, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::ConsoleTeePop, [0, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(call(&mut f, Call::Sync, [0, 0, 0]), Ok(0));
        assert_eq!(f.syncs, 1);
    }

    #[test]
    fn sys_info_names_the_system_and_gives_the_newest_of_the_log() {
        use relay_abi::info::{INFO_LOG, INFO_UNAME};
        let mut f = fake();
        let uname = u64::from(INFO_UNAME);
        assert_eq!(call(&mut f, Call::SysInfo, [uname, W, 256]), Ok(256));
        let b = get(&mut f, W, 256);
        assert_eq!(&b[..6], b"Relay\0");
        assert_eq!(&b[64..70], b"relay\0");
        assert_eq!(
            &b[128..128 + 6],
            concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()
        );
        assert_eq!(&b[192..199], b"x86_64\0");
        assert_eq!(
            call(&mut f, Call::SysInfo, [uname, W, 255]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [uname, W + PAGE - 100, 256]),
            Err(errno::EFAULT)
        );
        let log = u64::from(INFO_LOG);
        assert_eq!(
            call(&mut f, Call::SysInfo, [log, W, 1000]),
            Ok(FAKE_LOG.len() as u64)
        );
        assert_eq!(get(&mut f, W, FAKE_LOG.len()), FAKE_LOG);
        assert_eq!(call(&mut f, Call::SysInfo, [log, W, 5]), Ok(5));
        assert_eq!(
            get(&mut f, W, 5),
            FAKE_LOG[FAKE_LOG.len() - 5..],
            "the newest"
        );
        assert_eq!(call(&mut f, Call::SysInfo, [log, W, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::SysInfo, [log, U, 10]),
            Err(errno::EFAULT)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [4, W, 1000]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [1 << 32 | 1, W, 1000]),
            Err(errno::EINVAL)
        );
    }

    #[test]
    fn power_returns_only_the_error_that_kept_the_machine_up() {
        use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
        let mut f = fake();
        let (reboot, off, force) = (
            u64::from(POWER_REBOOT),
            u64::from(POWER_POWEROFF),
            u64::from(POWER_FORCE),
        );
        assert_eq!(call(&mut f, Call::Power, [reboot, 0, 0]), Err(errno::EIO));
        assert_eq!(call(&mut f, Call::Power, [off, force, 0]), Err(errno::EIO));
        assert_eq!(f.powered, [(true, false), (false, true)]);
        for (kind, flags) in [(0, 0), (3, 0), (reboot, 2), (1 << 32 | 1, 0)] {
            assert_eq!(
                call(&mut f, Call::Power, [kind, flags, 0]),
                Err(errno::EINVAL)
            );
        }
        assert_eq!(f.powered.len(), 2, "refused before anything was shut down");
    }

    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        let served = [
            Call::Exit,
            Call::Spawn,
            Call::Wait,
            Call::Kill,
            Call::Getpid,
            Call::ProcList,
            Call::MemMap,
            Call::MemUnmap,
            Call::Open,
            Call::Close,
            Call::Read,
            Call::Write,
            Call::Seek,
            Call::Fstat,
            Call::Stat,
            Call::ReadDir,
            Call::Mkdir,
            Call::Rmdir,
            Call::Unlink,
            Call::Truncate,
            Call::Touch,
            Call::Readlink,
            Call::Rename,
            Call::Statfs,
            Call::Sync,
            Call::Chdir,
            Call::Getcwd,
            Call::ConsoleMode,
            Call::ConsoleSize,
            Call::ConsoleForeground,
            Call::ConsoleTeePush,
            Call::ConsoleTeePop,
            Call::Time,
            Call::Sleep,
            Call::SysInfo,
            Call::Power,
            Call::Pipe,
        ];
        for c in Call::ALL {
            if !served.contains(&c) {
                assert_eq!(call(&mut f, c, [1, U, 1]), Err(errno::ENOSYS), "{c:?}");
            }
        }
        for n in [0, 38, 1000, u64::MAX] {
            assert_eq!(
                dispatch(&mut f, n, [0; 6]),
                Outcome::Return(encode(Err(errno::ENOSYS))),
                "{n}"
            );
        }
        assert!(f.written.is_empty() && f.spawned.is_empty() && f.killed.is_empty());
    }
}
