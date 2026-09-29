//! `t-spawn N`: starts N children that exit at once, waiting for each, and
//! prints the free frames before and after (spec §8.5), so a scenario sees
//! that starting and ending a program leaks nothing. More kinds:
//!
//! - `t-spawn kill` starts `t-spin`, kills it after a moment and prints how
//!   it ended; then shows that process 1 cannot be killed and a pid nobody
//!   has does not exist;
//! - `t-spawn orphan` starts `t-spin 1` and ends without waiting for it,
//!   so it passes to process 1;
//! - `t-spawn child` exits at once (the children of `t-spawn N`).
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
        Some(b"kill") => kill(),
        Some(b"orphan") => sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0).map(|_| ()),
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
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|orphan\n");
    2
}

/// Free frames now.
fn free() -> Result<u64, u16> {
    Ok(sys::memory()?.ram_free / 4096)
}

/// Starts `t-spawn child` and waits for it; it must exit with 0.
fn child() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0)?;
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
    Ok(())
}

fn kill() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, 0)?;
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
