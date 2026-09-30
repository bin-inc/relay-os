//! `t-read [KIND]`: reads the console (spec §6.4, §6.5) and prints each
//! read as `[<bytes>] <what it got>`, a newline as `\n` and other control
//! bytes as `^X`, until end of input.
//!
//! - `t-read`: in line mode, as the shell gives it the console: what was
//!   typed, a line at a time, echoed as it is typed.
//! - `t-read raw`: in raw mode, bytes as they are typed and not echoed,
//!   until `q`; then the mode it found again.
//! - `t-read apart`: starts `t-read` in a process group of its own, which
//!   is not the console's, so it gets end of input at once.
//! - `t-read size`: the console's columns and rows, and what
//!   `console_mode` and `console_foreground` refuse.
//! - `t-read leave`: leaves a child behind that puts the console in line
//!   mode half a second later, while the shell waits at its prompt.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::console::{MODE_LINE, MODE_RAW};
use relay_abi::errno;
use relay_abi::spawn::NEW_GROUP;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        None => lines(),
        Some(b"raw") => raw(),
        Some(b"apart") => apart(),
        Some(b"size") => size(),
        Some(b"leave") => sys::spawn(
            b"/bin/t-read",
            b"t-read\0leave-child\0",
            b"",
            &[],
            NEW_GROUP,
        )
        .map(|_| ()),
        Some(b"leave-child") => {
            sys::sleep(500);
            sys::console_mode(MODE_LINE).map(|_| ())
        }
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|size|leave]\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-read: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

/// `[<n>] <bytes>`.
fn show(bytes: &[u8]) {
    let _ = write!(Fd(1), "[{}] ", bytes.len());
    for &b in bytes {
        let _ = match b {
            b'\n' => sys::write_all(1, b"\\n"),
            0..=0x1F | 0x7F => sys::write_all(1, &[b'^', b ^ 0x40]),
            _ => sys::write_all(1, &[b]),
        };
    }
    let _ = sys::write_all(1, b"\n");
}

/// Reads until end of input, 100 reads at most.
fn lines() -> Result<(), u16> {
    let mut buf = [0u8; 64];
    for _ in 0..100 {
        match sys::read(0, &mut buf)? {
            0 => {
                let _ = sys::write_all(1, b"end of input\n");
                return Ok(());
            }
            n => show(&buf[..n]),
        }
    }
    Ok(())
}

fn raw() -> Result<(), u16> {
    let was = sys::console_mode(MODE_RAW)?;
    let _ = writeln!(Fd(1), "was line mode: {}", was == MODE_LINE);
    let mut buf = [0u8; 16];
    for _ in 0..100 {
        let n = sys::read(0, &mut buf)?;
        show(&buf[..n]);
        if buf[..n].contains(&b'q') {
            break;
        }
    }
    sys::console_mode(was)?;
    Ok(())
}

fn apart() -> Result<(), u16> {
    let fds = [
        FdMap {
            child: 0,
            parent: 0,
        },
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-read", b"t-read\0", b"", &fds, NEW_GROUP)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}

fn size() -> Result<(), u16> {
    let (columns, rows) = sys::console_size();
    let _ = writeln!(Fd(1), "console: {columns}x{rows}");
    let name = |r: Result<u32, u16>| r.map_or_else(|e| errno::name(e).unwrap_or("?"), |_| "ok");
    let _ = writeln!(Fd(1), "mode 7: {}", name(sys::console_mode(7)));
    let fg = sys::console_foreground(999_999).map(|()| 0);
    let _ = writeln!(Fd(1), "foreground 999999: {}", name(fg));
    let me = sys::getpid();
    let _ = writeln!(
        Fd(1),
        "foreground of my own group: {}",
        name(sys::console_foreground(me).map(|()| 0))
    );
    Ok(())
}
