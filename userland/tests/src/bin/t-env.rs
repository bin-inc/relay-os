//! `t-env`: prints the environment it was started with (programmable shell
//! gate §8.1–§8.3, §11.3), a line with its count and length, as the
//! registers gave them, then `[n] <entry>` for each, so a scenario sees
//! exactly what `spawn` passed. More kinds:
//!
//! - `t-env var NAME` prints `relay_rt::env::var(NAME)`, or `unset`;
//! - `t-env raw` prints whether the address register was 0, the length and
//!   the count;
//! - `t-env sizes` prints its arguments' count and length and its
//!   environment's;
//! - `t-env child` starts `t-env` with an environment of non-ASCII text,
//!   an entry without `=` and an empty one; `t-env none` starts `t-env raw`
//!   with none;
//! - `t-env limits` starts `t-env sizes` with 64 KiB of environment, then
//!   one byte more (`E2BIG`), then 64 KiB of arguments as well, then a
//!   block without its final NUL (`EINVAL`);
//! - `t-env sh` starts `/bin/sh` with an environment, reading `t-env` from
//!   a pipe: the grandchild's environment is what the shell got;
//! - `t-env deep FILE` starts `sh FILE` with 64 KiB of arguments and 64 KiB
//!   of environment at once, so a script at the shell's nesting bound runs
//!   on what is left of the stack.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
use relay_abi::{FdMap, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// The most a block holds (programmable shell gate §8.1).
const MAX: usize = 64 * 1024;

/// Standard input, output and error, as this program has them.
const STD: [FdMap; 3] = [
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

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        None => {
            show();
            Ok(())
        }
        Some(b"var") => match args.get(2) {
            Some(name) => {
                let value = relay_rt::env::var(name).unwrap_or(b"unset");
                sys::write_all(1, value).and_then(|()| sys::write_all(1, b"\n"))
            }
            None => return usage(),
        },
        Some(b"raw") => {
            let (at, len, count) = relay_rt::env::raw();
            let at = if at == 0 { "no address" } else { "an address" };
            let _ = writeln!(Fd(1), "{at}, {len} bytes, {count} entries");
            Ok(())
        }
        Some(b"sizes") => {
            sizes(&args);
            Ok(())
        }
        Some(b"child") => run(b"A=1\0B=two words\0C=\xc3\xa9t\xc3\xa9\0none\0\0", None),
        Some(b"none") => run(b"", Some(b"raw")),
        Some(b"limits") => limits(),
        Some(b"sh") => sh(),
        Some(b"deep") => match args.get(2) {
            Some(file) => deep(file),
            None => return usage(),
        },
        Some(_) => return usage(),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-env: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-env [var NAME|raw|sizes|child|none|limits|sh|deep FILE]\n",
    );
    2
}

/// `1 entry` or `N entries`.
fn entries(n: usize) -> &'static str {
    if n == 1 { "entry" } else { "entries" }
}

/// The count and length the registers gave, then each entry.
fn show() {
    let (_, len, n) = relay_rt::env::raw();
    let _ = writeln!(Fd(1), "{n} {}, {len} bytes", entries(n));
    for (i, e) in relay_rt::env::program().entries().enumerate() {
        let _ = write!(Fd(1), "[{i}] ");
        let _ = sys::write_all(1, e);
        let _ = sys::write_all(1, b"\n");
    }
}

fn sizes(args: &Args) {
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    let (_, env_len, n) = relay_rt::env::raw();
    let _ = writeln!(
        Fd(1),
        "{} arguments, {len} bytes; {n} {}, {env_len} bytes",
        args.len(),
        entries(n)
    );
}

/// Starts `t-env` (or `t-env <kind>`) with `env` and waits for it.
fn run(env: &[u8], kind: Option<&[u8]>) -> Result<(), u16> {
    let mut args = Vec::from(&b"t-env\0"[..]);
    if let Some(k) = kind {
        args.extend_from_slice(k);
        args.push(0);
    }
    start(b"/bin/t-env", &args, env, &STD)
}

/// Starts `path` and waits for it.
fn start(path: &[u8], args: &[u8], env: &[u8], fds: &[FdMap]) -> Result<(), u16> {
    let pid = sys::spawn_env(path, args, env, b"", fds, 0, 0)?;
    sys::wait(i64::from(pid), false)?;
    Ok(())
}

/// One entry, `A=vvv…`, of `len` bytes with its NUL.
fn entry_of(len: usize) -> Vec<u8> {
    let mut e = Vec::from(&b"A="[..]);
    e.resize(len - 1, b'v');
    e.push(0);
    e
}

/// `t-env sizes` and, after it, `len` bytes of arguments in all.
fn sizes_args(len: usize) -> Vec<u8> {
    let mut a = Vec::from(&b"t-env\0sizes\0"[..]);
    a.resize(len - 1, b'x');
    a.push(0);
    a
}

/// Each case as `<what>: `, then the child's `sizes` line or the error's
/// name.
fn limits() -> Result<(), u16> {
    let sizes = &b"t-env\0sizes\0"[..];
    let cases: [(&str, Vec<u8>, Vec<u8>); 4] = [
        ("64 KiB", sizes.into(), entry_of(MAX)),
        ("one byte more", sizes.into(), entry_of(MAX + 1)),
        ("64 KiB of each", sizes_args(MAX), entry_of(MAX)),
        ("no final NUL", sizes.into(), b"A=1".into()),
    ];
    for (what, args, env) in &cases {
        let _ = write!(Fd(1), "{what}: ");
        if let Err(e) = start(b"/bin/t-env", args, env, &STD) {
            let _ = writeln!(Fd(1), "{}", errno::name(e).unwrap_or("?"));
        }
    }
    Ok(())
}

/// `/bin/sh` with `X=1` and `HOME=/root`, reading `t-env` from a pipe.
fn sh() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let fds = [
        FdMap {
            child: 0,
            parent: r,
        },
        STD[1],
        STD[2],
    ];
    let pid = sys::spawn_env(b"/bin/sh", b"sh\0", b"X=1\0HOME=/root\0", b"", &fds, 0, 0)?;
    sys::close(r)?;
    sys::write_all(w, b"t-env\n")?;
    sys::close(w)?;
    sys::wait(i64::from(pid), false)?;
    Ok(())
}

/// `sh FILE` with both blocks full.
fn deep(file: &[u8]) -> Result<(), u16> {
    let mut args = Vec::from(&b"sh\0"[..]);
    args.extend_from_slice(file);
    args.push(0);
    args.resize(MAX - 1, b'x');
    args.push(0);
    start(b"/bin/sh", &args, &entry_of(MAX), &STD)
}
