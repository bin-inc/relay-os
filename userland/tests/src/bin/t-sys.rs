//! `t-sys KIND`: what `sys_info` and `power` answer (spec §7.3).
//!
//! - `t-sys uname`: the system's names, as `uname -a` prints them.
//! - `t-sys log`: the kernel log, whole (its first line) and its newest 16
//!   bytes.
//! - `t-sys poweroff [-f]`, `t-sys reboot`: shut the filesystems down and
//!   switch off or restart; if that fails, the error, and the machine
//!   stays up (`-f` goes ahead anyway).
#![no_std]
#![no_main]

extern crate alloc;

use core::fmt::Write;
use relay_abi::Uname;
use relay_abi::errno;
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"uname") => uname(),
        Some(b"log") => log(),
        Some(b"poweroff") => power(POWER_POWEROFF, args.get(2)),
        Some(b"reboot") => power(POWER_REBOOT, args.get(2)),
        _ => {
            let _ = sys::write_all(2, b"usage: t-sys uname|log|poweroff [-f]|reboot [-f]\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-sys: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn uname() -> Result<(), u16> {
    let u = sys::uname()?;
    for (i, f) in [u.sysname, u.nodename, u.release, u.machine]
        .iter()
        .enumerate()
    {
        if i > 0 {
            sys::write_all(1, b" ")?;
        }
        sys::write_all(1, Uname::name(f))?;
    }
    sys::write_all(1, b"\n")
}

fn log() -> Result<(), u16> {
    // Room for the whole log.
    let mut buf = alloc::vec![0u8; LOG_MAX];
    let n = sys::kernel_log(&mut buf)?;
    let first = buf[..n].split(|&b| b == b'\n').next().unwrap_or(&[]);
    let _ = write!(Fd(1), "{n} bytes, the first line: ");
    sys::write_all(1, first)?;
    let mut tail = [0u8; 16];
    let k = sys::kernel_log(&mut tail)?;
    let _ = writeln!(
        Fd(1),
        "\nthe newest 16 bytes: {} of them, the log's end: {}",
        k,
        tail[..k] == buf[n - k..n]
    );
    Ok(())
}

fn power(kind: u32, flag: Option<&[u8]>) -> Result<(), u16> {
    let flags = if flag == Some(b"-f") { POWER_FORCE } else { 0 };
    let e = sys::power(kind, flags);
    let _ = writeln!(Fd(1), "power: {}", errno::name(e).unwrap_or("?"));
    Ok(())
}
