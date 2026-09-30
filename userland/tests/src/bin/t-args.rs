//! `t-args [ARG]...`: prints each argument after argument 0 as
//! `[n] <arg>`, one per line (spec §8.5), so a scenario sees exactly what
//! `spawn` passed.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    for (n, arg) in args.iter().enumerate().skip(1) {
        let printed = write!(Fd(1), "[{n}] ").is_ok()
            && sys::write_all(1, arg).is_ok()
            && sys::write_all(1, b"\n").is_ok();
        if !printed {
            return 1;
        }
    }
    0
}
