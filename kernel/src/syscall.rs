//! The system-call dispatcher (user-space gate §7), architecture-neutral:
//! the `arch` entry stub hands it the call number and the six arguments,
//! and it answers with the result register's value or with the program's
//! exit. It checks and copies what the program passes (`UserSlice`,
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. Plan 3a serves `exit`, `spawn`, `wait`, `kill`, `getpid`,
//! `write`, `time`, `sleep` and `sys_info`'s memory figures; every other
//! call is `ENOSYS` until the plan that brings it.

use crate::exec::ARGS_MAX;
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::vec::Vec;
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::Errno;

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
    pub new_group: bool,
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
    /// `EBADF` unless `fd` is open for writing.
    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno>;
    /// Writes `bytes` to `fd` (checked with `writable_fd`); the file's
    /// error, if any.
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// Starts a child; its pid.
    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno>;
    /// A child that has ended, with how; `None` if `nohang` and none has.
    /// `ECHILD` if there is no such child.
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
    fn kill(&mut self, target: i64) -> Result<(), Errno>;
    fn pid(&self) -> u32;
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
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Time) => time(caller, args[0]),
        Some(Call::Sleep) => {
            caller.sleep(args[0]);
            Ok(0)
        }
        Some(Call::SysInfo) => sys_info(caller, args[0], args[1], args[2]),
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
    if a.flags & !NEW_GROUP != 0 || a.fd_count as usize > SPAWN_FDS {
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
        new_group: a.flags & NEW_GROUP != 0,
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

/// `write(fd, buffer, length)`. Copies the buffer out a page at a time; a
/// page that is not the program's, or the file's error, ends the call with
/// the bytes written before it, or with the error if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    caller.writable_fd(fd)?;
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        // To the end of the page, so a bad page costs only its own bytes.
        // `UserSlice::new` checked that `addr + len` does not overflow.
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        let written = caller
            .read(&slice, done, &mut buf[..n])
            .and_then(|()| caller.output(fd, &buf[..n]));
        if let Err(e) = written {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        done += n as u64;
    }
    Ok(done)
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

/// `sys_info(kind, buffer, length)` (spec §7.3): the bytes written. Plan
/// 3a has the memory figures (`MemInfo`); the `uname` fields and the kernel
/// log come with plan 3b.
fn sys_info(caller: &mut impl Caller, kind: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    if kind != u64::from(INFO_MEMORY) {
        return Err(Errno::EINVAL);
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
        assert_eq!(call(&mut f, Call::Write, [0, U, 1]), Ok(1), "the console");
        assert_eq!(text(&f, 1), [10, 11, 12, 13, 14]);
        assert_eq!(text(&f, 2), [0, 1, 2]);
        // Across pages, in pieces of at most 4 KiB.
        f.written.clear();
        assert_eq!(call(&mut f, Call::Write, [1, U + 100, 10_000]), Ok(10_000));
        let want: Vec<u8> = (100..10_100).map(|i| (i % 251) as u8).collect();
        assert_eq!(text(&f, 1), want);
        assert!(f.written.iter().all(|(_, b)| b.len() <= 4096));
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
    fn a_file_s_error_reaches_the_program() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [FULL, U, 10]), Err(errno::ENOSPC));
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
        assert_eq!(text(&f, 1), want);
        f.written.clear();
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 904, 2000]),
            Ok(904),
            "less than a page before the hole"
        );
        assert_eq!(text(&f, 1).len(), 904);
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
                new_group: true,
            }]
        );
        let a = spawn_args(&mut f, b"x\0", |a| {
            a.flags = 0;
            a.cwd_len = 0;
            a.fd_count = 0;
        });
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(102));
        let s = &f.spawned[1];
        assert!(!s.new_group && s.cwd.is_empty() && s.fds.is_empty());
        assert_eq!(s.argc, 1);
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
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 2), Err(errno::EINVAL));
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
        assert_eq!(call(&mut f, Call::SysInfo, [2, W, 32]), Err(errno::EINVAL));
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
    fn every_other_call_is_enosys() {
        let mut f = fake();
        let served = [
            Call::Exit,
            Call::Spawn,
            Call::Wait,
            Call::Kill,
            Call::Getpid,
            Call::Write,
            Call::Time,
            Call::Sleep,
            Call::SysInfo,
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
