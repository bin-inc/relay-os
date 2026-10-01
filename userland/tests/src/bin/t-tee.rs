//! `t-tee KIND`: console tees (spec §6.5). Each answer is printed as
//! `<what>: <value or error name>`; what a tee got is shown afterwards,
//! line by line, as `| <line>`.
//!
//! - `t-tee basic`: a tee gets what this program and its child write to
//!   the console, in 4 KiB pieces and at every `sync`, until it is
//!   popped; the pushes and pops that are refused.
//! - `t-tee end`: pushes a tee on `t-tee.end` and ends without popping
//!   it: it gets what came before the end, written at the next sync.
//! - `t-tee gone`: a tee whose file is removed is gone: the sync's write
//!   to it fails, its pop is `ENOENT`, and a new file that may have got its
//!   inode gets nothing.
//! - `t-tee typed`: reads lines from the console until `end`, writing
//!   nothing: the tee gets their echo as they are read, 4 KiB at a time.
//! - `t-tee full`, on a full disk: a tee that cannot be written is removed,
//!   and its pop says `ENOSPC`.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::errno;
use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"end") => end(),
        Some(b"gone") => gone(),
        Some(b"full") => full(),
        Some(b"typed") => typed(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|typed|full\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-tee: {}", name(e));
            1
        }
    }
}

fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

fn show_ok(what: &str, r: Result<(), u16>) {
    let _ = writeln!(Fd(1), "{what}: {}", r.map_or_else(name, |()| "ok"));
}

/// A new file for a tee, open for writing.
fn create(path: &[u8]) -> Result<u32, u16> {
    sys::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)
}

fn size(path: &[u8]) -> u64 {
    sys::stat(path, 0).map_or(0, |s| s.size)
}

/// Prints the lines of `path` as `| <line>`, and how many bytes it holds.
fn show_file(path: &[u8]) -> Result<(), u16> {
    let fd = sys::open(path, OPEN_READ)?;
    let mut buf = [0u8; 256];
    let mut line = [0u8; 256];
    let (mut len, mut total) = (0, 0);
    // The logs are a few KiB: a file that never ends is cut at 64 KiB.
    for _ in 0..256 {
        let n = sys::read(fd, &mut buf)?;
        if n == 0 {
            break;
        }
        total += n;
        for &b in &buf[..n] {
            if b == b'\n' {
                let _ = sys::write_all(1, b"| ");
                let _ = sys::write_all(1, &line[..len]);
                let _ = sys::write_all(1, b"\n");
                len = 0;
            } else if len < line.len() {
                line[len] = b;
                len += 1;
            }
        }
    }
    sys::close(fd)?;
    let _ = writeln!(Fd(1), "{total} bytes");
    Ok(())
}

fn basic() -> Result<(), u16> {
    let log = create(b"t-tee.log")?;
    show_ok("push", sys::console_tee_push(log));
    let _ = writeln!(Fd(1), "to the screen and the tee");
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0from a child\0", b"", &fds, 0, 0)?;
    sys::wait(i64::from(pid), false)?;
    let _ = writeln!(Fd(1), "nothing written yet: {}", size(b"t-tee.log") == 0);
    show_ok("sync", sys::sync());
    let _ = writeln!(Fd(1), "written at the sync: {}", size(b"t-tee.log") > 0);
    // 4 KiB of dots is written as it comes.
    let before = size(b"t-tee.log");
    let dots = [b'.'; 99];
    for _ in 0..42 {
        let _ = sys::write_all(1, &dots);
        let _ = sys::write_all(1, b"\n");
    }
    let _ = writeln!(
        Fd(1),
        "written every 4 KiB: {}",
        size(b"t-tee.log") >= before + 4096
    );
    show_ok("pop", sys::console_tee_pop());
    let _ = writeln!(Fd(1), "not in the tee");
    show_ok("pop again", sys::console_tee_pop());
    show_ok("push the screen", sys::console_tee_push(1));
    let r = sys::open(b"t-tee.log", OPEN_READ)?;
    show_ok("push a file open for reading", sys::console_tee_push(r));
    show_ok("push an fd not open", sys::console_tee_push(30));
    // 4 tees at most; a kernel without the limit is stopped at 16.
    let mut pushed = 0;
    let mut full = None;
    while pushed < 16 && full.is_none() {
        match sys::console_tee_push(log) {
            Ok(()) => pushed += 1,
            Err(e) => full = Some(e),
        }
    }
    let _ = match full {
        Some(e) => writeln!(Fd(1), "pushed {pushed}, then {}", name(e)),
        None => writeln!(Fd(1), "pushed {pushed}, and no end"),
    };
    for _ in 0..pushed {
        sys::console_tee_pop()?;
    }
    sys::close(log)?;
    sys::close(r)?;
    show_file(b"t-tee.log")
}

fn end() -> Result<(), u16> {
    let log = create(b"t-tee.end")?;
    sys::console_tee_push(log)?;
    let _ = writeln!(Fd(1), "before the end");
    Ok(())
}

fn gone() -> Result<(), u16> {
    let log = create(b"t-tee.gone")?;
    sys::console_tee_push(log)?;
    // Only the tee has the file now.
    sys::close(log)?;
    sys::unlink(b"t-tee.gone")?;
    // A new file may get its inode: the tee must not write into it.
    let new = create(b"t-tee.new")?;
    let _ = writeln!(Fd(1), "after the removal");
    // The sync's write fails (and is logged); the pop says so.
    sys::sync()?;
    show_ok("pop", sys::console_tee_pop());
    let _ = writeln!(Fd(1), "the new file holds {} bytes", size(b"t-tee.new"));
    sys::close(new)?;
    sys::unlink(b"t-tee.new")
}

fn full() -> Result<(), u16> {
    let log = create(b"t-tee.full")?;
    sys::console_tee_push(log)?;
    for _ in 0..100 {
        let _ = writeln!(Fd(1), "a line the disk has no room for");
    }
    show_ok("pop", sys::console_tee_pop());
    sys::close(log)?;
    sys::unlink(b"t-tee.full")
}

fn typed() -> Result<(), u16> {
    let log = create(b"t-tee.typed")?;
    sys::console_tee_push(log)?;
    let mut buf = [0u8; 4096];
    for _ in 0..20 {
        let n = sys::read(0, &mut buf)?;
        if n == 0 || &buf[..n] == b"end\n" {
            break;
        }
    }
    // Nothing written, no sync: only the reads can have written the echo.
    let written = size(b"t-tee.typed") >= 4096;
    sys::console_tee_pop()?;
    let _ = writeln!(Fd(1), "written as it was typed: {written}");
    sys::close(log)?;
    sys::unlink(b"t-tee.typed")
}
