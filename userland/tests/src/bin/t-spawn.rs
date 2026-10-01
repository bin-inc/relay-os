//! `t-spawn N`: starts N children that exit at once, waiting for each, and
//! prints the free frames before and after (spec §8.5), and how many were
//! lost, so a scenario sees that starting and ending a program leaks
//! nothing. More kinds:
//!
//! - `t-spawn kill` starts `t-spin`, kills it after a moment and prints how
//!   it ended; then shows that process 1 cannot be killed and a pid nobody
//!   has does not exist;
//! - `t-spawn kill-new` starts `t-args` and kills it before it has run:
//!   none of its code may run;
//! - `t-spawn orphan` starts `t-spin 1` and ends without waiting for it,
//!   so it passes to process 1;
//! - `t-spawn fill` starts children that nap for 300 ms until the process
//!   table is full, and ends without waiting for them: their zombies pass
//!   to process 1;
//! - `t-spawn join` starts children in the group of another child, which
//!   a pipeline's stages do (spec §9.1): one that dozes, then one that
//!   exited and was not collected yet; `kill` of the group ends both
//!   dozers; joining process 1's group, or a new group and another at
//!   once, is refused;
//! - `t-spawn sleepers` starts three children that sleep for a minute in
//!   its group and one in a group of its own, and waits for that one: a
//!   group blocked in the kernel, for Ctrl-C, whose `wait` nothing but the
//!   kill can end;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
//!   nap` after 300 ms, `t-spawn doze` after a minute.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Standard output and error, as this program has them.
const STD: [FdMap; 2] = [
    FdMap {
        child: 1,
        parent: 1,
    },
    FdMap {
        child: 2,
        parent: 2,
    },
];

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"child") => return 0,
        Some(b"nap") => {
            sys::sleep(300);
            return 0;
        }
        Some(b"fill") => fill(),
        Some(b"doze") => {
            sys::sleep(60_000);
            return 0;
        }
        Some(b"sleepers") => sleepers(),
        Some(b"join") => join(),
        Some(b"kill") => kill(),
        Some(b"kill-new") => kill_new(),
        Some(b"orphan") => {
            sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0, 0).map(|_| ())
        }
        Some(n) => match parse(n) {
            Some(n) => many(n),
            None => return usage(),
        },
        None => return usage(),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-spawn: error {e}");
            1
        }
    }
}

fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-spawn N|kill|kill-new|orphan|fill|sleepers|join\n",
    );
    2
}

/// Free frames now.
fn free() -> Result<u64, u16> {
    Ok(sys::memory()?.ram_free / 4096)
}

/// Starts `t-spawn child` and waits for it; it must exit with 0.
fn child() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, 0)?;
    match sys::wait(i64::from(pid), false)? {
        Some((p, w)) if p == pid && w == relay_abi::WaitStatus::exited(0) => Ok(()),
        _ => Err(relay_abi::errno::ECHILD),
    }
}

fn many(n: u64) -> Result<(), u16> {
    // The first child may make the page tables of a kernel-stack slot,
    // which stay: count after it.
    child()?;
    let before = free()?;
    for _ in 0..n {
        child()?;
    }
    let after = free()?;
    let _ = writeln!(Fd(1), "free frames before: {before}");
    let _ = writeln!(Fd(1), "free frames after: {after}");
    // For the NUC's check script, which compares no two lines.
    let lost = i128::from(before) - i128::from(after);
    let _ = writeln!(Fd(1), "frames lost: {lost}");
    Ok(())
}

fn kill() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, 0, 0)?;
    // It spins, and this one sleeps: the tick wakes it all the same.
    sys::sleep(200);
    sys::kill(i64::from(pid))?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "t-spin: {w}");
    }
    let name = |e: u16| match e {
        relay_abi::errno::EPERM => "EPERM",
        relay_abi::errno::ESRCH => "ESRCH",
        _ => "?",
    };
    for target in [1, 999_999] {
        let said = sys::kill(target).map_or_else(name, |()| "killed");
        let _ = writeln!(Fd(1), "kill {target}: {said}");
    }
    let me = sys::getpid();
    let _ = writeln!(Fd(1), "pid above 1: {}", me > 1);
    Ok(())
}

/// Children asleep, and this one waiting for one outside its group.
fn sleepers() -> Result<(), u16> {
    for _ in 0..3 {
        sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], 0, 0)?;
    }
    let apart = relay_abi::spawn::NEW_GROUP;
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], apart, 0)?;
    sys::wait(i64::from(pid), false)?;
    let _ = sys::write_all(1, b"t-spawn: the sleeper woke\n");
    Ok(())
}

/// Children in another child's group, and the groups no child may join.
fn join() -> Result<(), u16> {
    use relay_abi::spawn::NEW_GROUP;
    let name = |e: u16| relay_abi::errno::name(e).unwrap_or("?");
    let doze = |flags, pgid| sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], flags, pgid);
    let first = doze(NEW_GROUP, 0)?;
    let second = doze(0, first)?;
    let _ = writeln!(Fd(1), "joined a dozing child's group");
    for (what, flags, pgid) in [
        ("group 1", 0, 1),
        ("a new group and another", NEW_GROUP, first),
    ] {
        let said = doze(flags, pgid).map_or_else(name, |_| "started");
        let _ = writeln!(Fd(1), "{what}: {said}");
    }
    sys::kill(-i64::from(first))?;
    for pid in [first, second] {
        if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
            let _ = writeln!(
                Fd(1),
                "{}: {w}",
                if pid == first { "first" } else { "second" }
            );
        }
    }
    // A child that has ended, and was not collected, still has its group.
    let ended = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], NEW_GROUP, 0)?;
    sys::sleep(200);
    let late = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, ended)?;
    let _ = writeln!(Fd(1), "joined an ended child's group");
    for pid in [ended, late] {
        sys::wait(i64::from(pid), false)?;
    }
    let said = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, ended)
        .map_or_else(name, |_| "started");
    let _ = writeln!(Fd(1), "once collected: {said}");
    Ok(())
}

/// Starts napping children until the table is full, and leaves them.
fn fill() -> Result<(), u16> {
    let mut n = 0;
    loop {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0, 0) {
            Ok(_) => n += 1,
            Err(relay_abi::errno::EAGAIN) => break,
            Err(e) => return Err(e),
        }
    }
    let _ = writeln!(Fd(1), "filled the table with {n} children");
    Ok(())
}

/// Kills a child before it has run, and says how it ended.
fn kill_new() -> Result<(), u16> {
    let pid = sys::spawn(
        b"/bin/t-args",
        b"t-args\0SHOULD-NOT-PRINT\0",
        b"",
        &STD,
        0,
        0,
    )?;
    sys::kill(i64::from(pid))?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "t-args: {w}");
    }
    Ok(())
}

/// A decimal number.
fn parse(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0u64, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(u64::from(d))
    })
}
