//! `t-proc KIND`: the `proc_list` call from ring 3 (spec §9.3), each answer
//! printed as `<what>: <value or error name>`.
//!
//! - `t-proc list`: process 1, its parent (the shell), itself, and four
//!   children of its own as `proc_list` shows them: one sleeping, one
//!   ended and not yet collected, one waiting on an empty pipe and one
//!   reading the console; then that the list is by pid.
//! - `t-proc short`: a buffer of one entry gets the first process and the
//!   count of all; one of none only the count. (A buffer the program does
//!   not have is `EFAULT`: the dispatcher's tests, over the same checks.)
//! - `t-proc long`: a child started by a path longer than 64 bytes shows
//!   its first 64.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::proc::{
    PROC_MAX, PROC_NAME, STATE_PIPE, STATE_READ, STATE_READY, STATE_RUN, STATE_SLEEP, STATE_WAIT,
    STATE_ZOMBIE,
};
use relay_abi::{FdMap, ProcInfo, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"list") => list(),
        Some(b"short") => short(),
        Some(b"long") => long(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-proc list|short|long\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-proc: {}", name(e));
            1
        }
    }
}

/// An error's name.
fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

/// A state's word, as `ps` says it.
fn state(s: u32) -> &'static str {
    match s {
        STATE_RUN => "run",
        STATE_READY => "ready",
        STATE_WAIT => "wait",
        STATE_READ => "read",
        STATE_SLEEP => "sleep",
        STATE_PIPE => "pipe",
        STATE_ZOMBIE => "zombie",
        _ => "?",
    }
}

/// Every process, and how many `proc_list` said there are.
fn processes(buf: &mut [ProcInfo; PROC_MAX]) -> Result<&[ProcInfo], u16> {
    let n = sys::proc_list(buf)?;
    Ok(&buf[..n.min(PROC_MAX)])
}

fn entry(all: &[ProcInfo], pid: u32) -> Option<&ProcInfo> {
    all.iter().find(|p| p.pid == pid)
}

/// `<what>: <name>, <state>` and whether it holds memory.
fn print(what: &str, p: Option<&ProcInfo>) {
    let _ = match p {
        Some(p) => writeln!(
            Fd(1),
            "{what}: {}, {}, {}",
            core::str::from_utf8(p.name()).unwrap_or("?"),
            state(p.state),
            if p.frames > 0 { "frames" } else { "no frames" }
        ),
        None => writeln!(Fd(1), "{what}: none"),
    };
}

/// Starts `path` with `args` (each followed by a NUL) and `fds`.
fn start(path: &[u8], args: &[u8], fds: &[(u32, u32)]) -> Result<u32, u16> {
    let maps: [FdMap; 3] = core::array::from_fn(|i| {
        let (child, parent) = fds.get(i).copied().unwrap_or((0, 0));
        FdMap { child, parent }
    });
    sys::spawn(path, args, b"", &maps[..fds.len()], 0, 0)
}

/// Whether `pid`'s entry says `want`, asked again every 10 ms for at most
/// a second (a child needs a moment to get there).
fn until(pid: u32, want: u32) -> Result<(), u16> {
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    for _ in 0..100 {
        if entry(processes(&mut buf)?, pid).is_some_and(|p| p.state == want) {
            return Ok(());
        }
        sys::sleep(10);
    }
    let _ = writeln!(Fd(1), "pid {pid} never {}, and no end", state(want));
    Ok(())
}

fn list() -> Result<(), u16> {
    let me = sys::getpid();
    let sleeper = start(b"/bin/sleep", b"sleep\x005\0", &[])?;
    let ended = start(b"/bin/true", b"true\0", &[])?;
    let (r, w) = sys::pipe()?;
    let piped = start(b"/bin/cat", b"cat\0", &[(0, r)])?;
    // In this program's group, which has the console.
    let reader = start(b"/bin/cat", b"cat\0", &[(0, 0)])?;
    for (pid, want) in [
        (sleeper, STATE_SLEEP),
        (ended, STATE_ZOMBIE),
        (piped, STATE_PIPE),
        (reader, STATE_READ),
    ] {
        until(pid, want)?;
    }
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    let all = processes(&mut buf)?;
    let mine = entry(all, me);
    let parent = mine.and_then(|p| entry(all, p.ppid));
    let init = entry(all, 1);
    let _ = writeln!(
        Fd(1),
        "init: ppid {}, group {}",
        init.map_or(99, |p| p.ppid),
        init.map_or(99, |p| p.pgid)
    );
    print("init", init);
    print("parent", parent);
    print("me", mine);
    print("sleeping child", entry(all, sleeper));
    print("ended child", entry(all, ended));
    print("child on a pipe", entry(all, piped));
    print("child reading", entry(all, reader));
    let groups = [sleeper, ended, piped, reader].map(|c| entry(all, c).map_or(0, |p| p.pgid));
    let _ = writeln!(
        Fd(1),
        "children in my group: {}",
        groups.iter().all(|&g| Some(g) == mine.map(|p| p.pgid))
    );
    let by_pid = all.windows(2).all(|w| w[0].pid < w[1].pid);
    let _ = writeln!(Fd(1), "by pid: {by_pid}");
    for pid in [sleeper, piped, reader] {
        sys::kill(i64::from(pid))?;
    }
    sys::close(r)?;
    sys::close(w)?;
    for pid in [sleeper, ended, piped, reader] {
        sys::wait(i64::from(pid), false)?;
    }
    Ok(())
}

fn short() -> Result<(), u16> {
    let mut one = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); 1];
    let n = sys::proc_list(&mut one)?;
    let _ = writeln!(
        Fd(1),
        "one entry: pid {}, of more than one: {}",
        one[0].pid,
        n > 1
    );
    let all = sys::proc_list(&mut [])?;
    let _ = writeln!(Fd(1), "no entry: the same count: {}", all == n);
    Ok(())
}

fn long() -> Result<(), u16> {
    // `/bin/` and 40 times `./`, then `sleep`: 90 bytes, /bin/sleep.
    let mut path = [0u8; 90];
    path[..5].copy_from_slice(b"/bin/");
    for i in 0..40 {
        path[5 + 2 * i..7 + 2 * i].copy_from_slice(b"./");
    }
    path[85..].copy_from_slice(b"sleep");
    let child = start(&path, b"sleep\x001\0", &[])?;
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    let all = processes(&mut buf)?;
    let shown = entry(all, child).map_or(&b""[..], |p| p.name());
    let _ = writeln!(
        Fd(1),
        "a name of {} bytes: {} bytes, its start: {}",
        path.len(),
        shown.len(),
        shown == &path[..PROC_NAME]
    );
    sys::kill(i64::from(child))?;
    sys::wait(i64::from(child), false)?;
    Ok(())
}
