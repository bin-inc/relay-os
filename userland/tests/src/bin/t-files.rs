//! `t-files KIND`: the file calls from ring 3 (spec §7.3), each answer
//! printed as `<what>: <value or error name>`, so a scenario and the NUC's
//! check script see what the kernel said. It works in the current
//! directory, on `t-files.tmp`.
//!
//! - `t-files basic`: `open` with its flags, `read`, `write`, `seek`,
//!   `fstat` and `close`; an offset shared with a child that got the fd;
//!   32 fds and then `EMFILE`.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
//!   of `basic`).
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::errno;
use relay_abi::file::{
    KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
    SEEK_END, SEEK_START,
};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

const TMP: &[u8] = b"t-files.tmp";

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"child") => child(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-files: {}", name(e));
            1
        }
    }
}

/// An error's name.
fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

/// `<what>: <value>` or `<what>: <error>`.
fn show<T: core::fmt::Display>(what: &str, r: Result<T, u16>) {
    let _ = match r {
        Ok(v) => writeln!(Fd(1), "{what}: {v}"),
        Err(e) => writeln!(Fd(1), "{what}: {}", name(e)),
    };
}

/// `<what>: ok` or `<what>: <error>`.
fn show_ok(what: &str, r: Result<(), u16>) {
    show(what, r.map(|()| "ok"));
}

/// `<what>: "<text>"` for bytes read, a newline as `\n`.
fn show_text(what: &str, bytes: &[u8]) {
    let _ = write!(Fd(1), "{what}: \"");
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        let _ = match line.strip_suffix(b"\n") {
            Some(l) => sys::write_all(1, l).and_then(|()| sys::write_all(1, b"\\n")),
            None => sys::write_all(1, line),
        };
    }
    let _ = sys::write_all(1, b"\"\n");
}

fn basic() -> Result<(), u16> {
    let rw = OPEN_READ | OPEN_WRITE;
    let fd = sys::open(TMP, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    show("create", Ok(fd));
    show(
        "exclusive",
        sys::open(TMP, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE),
    );
    show("no flags", sys::open(TMP, 0));
    show("missing", sys::open(b"t-files.missing", OPEN_READ));
    show("write", sys::write(fd, b"hello, world\n"));
    show("read a write-only fd", sys::read(fd, &mut [0; 4]));
    show("seek", sys::seek(fd, 7, SEEK_START));
    show("overwrite", sys::write(fd, b"files\n"));
    show_ok("close", sys::close(fd));
    show_ok("close again", sys::close(fd));

    let r = sys::open(TMP, OPEN_READ)?;
    let mut buf = [0u8; 64];
    let n = sys::read(r, &mut buf[..5])?;
    show_text("read", &buf[..n]);
    let st = sys::fstat(r)?;
    let _ = writeln!(
        Fd(1),
        "fstat: {} bytes, regular {}",
        st.size,
        st.kind == u32::from(KIND_REGULAR)
    );
    show("seek to the end", sys::seek(r, 0, SEEK_END));
    show("read at the end", sys::read(r, &mut buf));
    let a = sys::open(TMP, OPEN_WRITE | OPEN_APPEND)?;
    show("append", sys::write(a, b"more\n"));
    let n = sys::read(r, &mut buf)?;
    show_text("read what was appended", &buf[..n]);
    show("seek before the start", sys::seek(r, -1, SEEK_START));
    show("seek the screen", sys::seek(1, 0, SEEK_START));
    sys::close(a)?;

    // A child that got the fd shares its offset.
    sys::seek(r, 0, SEEK_START)?;
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
        FdMap {
            child: 3,
            parent: r,
        },
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0child\0", b"", &fds, 0)?;
    sys::wait(i64::from(pid), false)?;
    let n = sys::read(r, &mut buf[..5])?;
    show_text("read after the child", &buf[..n]);
    sys::close(r)?;

    // 32 fds at most.
    let mut last = 0;
    let full = loop {
        match sys::open(TMP, rw) {
            Ok(fd) => last = fd,
            Err(e) => break e,
        }
    };
    let _ = writeln!(Fd(1), "fds up to {last}, then {}", name(full));
    for fd in 3..=last {
        sys::close(fd)?;
    }
    Ok(())
}

fn child() -> Result<(), u16> {
    let mut buf = [0u8; 7];
    let n = sys::read(3, &mut buf)?;
    show_text("the child read", &buf[..n]);
    Ok(())
}
