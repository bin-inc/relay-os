//! `t-spin [SECONDS]`: spins without system calls (spec §8.5), for ever or
//! for `SECONDS` seconds, reading the clock only every 2^20 iterations,
//! then prints how many iterations it made. Nothing but the timer takes
//! the CPU away from it, and the keyboard must keep working meanwhile.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Iterations between two looks at the clock.
const CHECK_EVERY: u64 = 1 << 20;

fn main(args: Args) -> u8 {
    let limit = match args.get(1) {
        None => None,
        Some(s) => match parse(s) {
            Some(secs) => Some(secs.saturating_mul(1_000_000_000)),
            None => {
                let _ = sys::write_all(2, b"usage: t-spin [SECONDS]\n");
                return 2;
            }
        },
    };
    let Ok(start) = sys::time() else {
        let _ = sys::write_all(2, b"t-spin: no clock\n");
        return 1;
    };
    let mut n: u64 = 0;
    loop {
        n = core::hint::black_box(n + 1);
        if n.is_multiple_of(CHECK_EVERY)
            && let Some(limit) = limit
            && sys::time().is_ok_and(|t| t.uptime_ns - start.uptime_ns >= limit)
        {
            break;
        }
    }
    if writeln!(Fd(1), "{n} iterations").is_err() {
        return 1;
    }
    0
}

/// A decimal number of seconds.
fn parse(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0u64, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(u64::from(d))
    })
}
