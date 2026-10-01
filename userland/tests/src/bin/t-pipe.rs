//! `t-pipe KIND`: the `pipe` call from ring 3 (spec §9.1, §16 item 8),
//! each answer printed as `<what>: <value or error name>`. Every loop that
//! waits for the kernel to say stop is bounded, and says `and no end` when
//! it never does.
//!
//! - `t-pipe basic`: a pipe's fds, a write and a read, `fstat`, and what an
//!   end is not (`seek`, the wrong direction, a tee); the end of the data
//!   once the write end is closed.
//! - `t-pipe room`: a write into an empty pipe takes 16 KiB of 20000 bytes,
//!   and the bytes read back are those.
//! - `t-pipe child`: 1 MiB through a pipe to a child that reads it
//!   (`drain`), both blocking in turn; both print a checksum.
//! - `t-pipe eof`: the write end is held by a child that naps, so the end
//!   of the data comes only when it ends.
//! - `t-pipe epipe`: a write once the read end is closed; and `t-args`
//!   writing to a pipe nobody reads, which ends it quietly with 141.
//! - `t-pipe killed`: a child blocked reading an empty pipe, and one
//!   blocked writing a full one, are killed.
//! - `t-pipe many`: pipes until the fds run out (`EMFILE`, no fd taken).
//! - `t-pipe drain` reads fd 0 to its end and prints how much and its
//!   checksum; `hold` naps with fd 3; `flood` writes to fd 3 until it is
//!   stopped (the children of the kinds above).
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::fmt::Write;
use relay_abi::file::{KIND_FIFO, OPEN_DIRECTORY, OPEN_READ, SEEK_START};
use relay_abi::{FdMap, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// How much `child` sends.
const SENT: usize = 1 << 20;

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"room") => room(),
        Some(b"child") => child(),
        Some(b"eof") => eof(),
        Some(b"epipe") => epipe(),
        Some(b"killed") => killed(),
        Some(b"many") => many(),
        Some(b"drain") => drain(),
        Some(b"hold") => {
            sys::sleep(300);
            Ok(())
        }
        Some(b"flood") => flood(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-pipe basic|room|child|eof|epipe|killed|many\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-pipe: {}", name(e));
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

/// Starts this program as `kind` with `fds` (child, parent).
fn start(kind: &[u8], fds: &[(u32, u32)]) -> Result<u32, u16> {
    let mut args = vec![];
    args.extend_from_slice(b"t-pipe\0");
    args.extend_from_slice(kind);
    args.push(0);
    let maps: alloc::vec::Vec<FdMap> = fds
        .iter()
        .map(|&(child, parent)| FdMap { child, parent })
        .collect();
    sys::spawn(b"/bin/t-pipe", &args, b"", &maps, 0, 0)
}

/// Waits for `pid` and prints how it ended as `<what>: <how>`.
fn reap(what: &str, pid: u32) -> Result<(), u16> {
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "{what}: {w}");
    }
    Ok(())
}

fn basic() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let _ = writeln!(Fd(1), "pipe: {r} {w}");
    show("write", sys::write(w, b"hello"));
    let mut buf = [0u8; 100];
    let n = sys::read(r, &mut buf)?;
    let _ = writeln!(
        Fd(1),
        "read: {}",
        core::str::from_utf8(&buf[..n]).unwrap_or("?")
    );
    let st = sys::fstat(r)?;
    let kind = if st.kind == u32::from(KIND_FIFO) {
        "fifo"
    } else {
        "?"
    };
    let _ = writeln!(Fd(1), "fstat: {kind}, dev {}", st.dev);
    show("seek", sys::seek(r, 0, SEEK_START));
    show("read the write end", sys::read(w, &mut buf));
    show("write the read end", sys::write(r, b"x"));
    show("tee", sys::console_tee_push(w).map(|()| "pushed"));
    sys::close(w)?;
    show("after the writer closed", sys::read(r, &mut buf));
    sys::close(r)
}

fn room() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let data = vec![b'x'; 20_000];
    show("write 20000", sys::write(w, &data));
    let mut buf = vec![0u8; 20_000];
    let mut got = 0;
    // Exactly what went in: one read more would wait for ever.
    for _ in 0..8 {
        if got >= 16_384 {
            break;
        }
        got += sys::read(r, &mut buf[got..16_384])?;
    }
    let all_x = buf[..got].iter().all(|&b| b == b'x');
    let _ = writeln!(Fd(1), "read back: {got}, all as written: {all_x}");
    sys::close(w)?;
    sys::close(r)
}

/// The checksum both sides print.
fn checksum(sum: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(sum, |s, &b| s.wrapping_mul(31).wrapping_add(u64::from(b)))
}

fn child() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let pid = start(b"drain", &[(0, r), (1, 1), (2, 2)])?;
    sys::close(r)?;
    let mut sum = 0;
    let mut piece = vec![0u8; 64 * 1024];
    for k in 0..SENT / piece.len() {
        for (i, b) in piece.iter_mut().enumerate() {
            *b = ((k * 7 + i) % 251) as u8;
        }
        sum = checksum(sum, &piece);
        sys::write_all(w, &piece)?;
    }
    sys::close(w)?;
    sys::wait(i64::from(pid), false)?;
    let _ = writeln!(Fd(1), "sent {SENT} bytes, checksum {sum}");
    Ok(())
}

/// Reads fd 0 to its end.
fn drain() -> Result<(), u16> {
    let mut buf = vec![0u8; 10_000];
    let (mut total, mut sum) = (0usize, 0);
    // 4 MiB at most, in reads of at least a byte.
    for _ in 0..4 << 20 {
        match sys::read(0, &mut buf)? {
            0 => {
                let _ = writeln!(Fd(1), "drained {total} bytes, checksum {sum}");
                return Ok(());
            }
            n => {
                sum = checksum(sum, &buf[..n]);
                total += n;
            }
        }
        if total > 4 << 20 {
            break;
        }
    }
    let _ = writeln!(Fd(1), "drained {total} bytes, and no end");
    Ok(())
}

fn eof() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let pid = start(b"hold", &[(3, w), (2, 2)])?;
    sys::close(w)?;
    let mut buf = [0u8; 10];
    show("end of data once the child ended", sys::read(r, &mut buf));
    sys::wait(i64::from(pid), false)?;
    sys::close(r)
}

fn epipe() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    sys::close(r)?;
    show("write with no reader", sys::write(w, b"x"));
    let args = b"t-args\0to nobody\0";
    let fds = [
        FdMap {
            child: 1,
            parent: w,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-args", args, b"", &fds, 0, 0)?;
    sys::close(w)?;
    reap("t-args with nobody reading", pid)
}

/// Writes to fd 3 until stopped: 64 MiB at most.
fn flood() -> Result<(), u16> {
    let piece = [b'f'; 4096];
    for _ in 0..(64 << 20) / piece.len() {
        sys::write_all(3, &piece)?;
    }
    let _ = writeln!(Fd(1), "flooded 64 MiB, and no end");
    Ok(())
}

fn killed() -> Result<(), u16> {
    // This program keeps both ends of each pipe, so neither child ever
    // sees an end: one waits for data, the other for room.
    let (r, w) = sys::pipe()?;
    let reader = start(b"drain", &[(0, r), (1, 1), (2, 2)])?;
    let (r2, w2) = sys::pipe()?;
    let writer = start(b"flood", &[(3, w2), (1, 1), (2, 2)])?;
    sys::sleep(200);
    sys::kill(i64::from(reader))?;
    reap("a reader blocked on an empty pipe", reader)?;
    sys::kill(i64::from(writer))?;
    reap("a writer blocked on a full pipe", writer)?;
    for fd in [r, w, r2, w2] {
        sys::close(fd)?;
    }
    Ok(())
}

fn many() -> Result<(), u16> {
    let mut ends = vec![];
    let mut said = None;
    // 32 fds: never more than 16 pipes.
    for _ in 0..17 {
        match sys::pipe() {
            Ok((r, w)) => ends.extend([r, w]),
            Err(e) => {
                said = Some(e);
                break;
            }
        }
    }
    let _ = writeln!(
        Fd(1),
        "pipes: {}, then {}",
        ends.len() / 2,
        said.map_or("no end", name)
    );
    show("the last fd", sys::open(b"/", OPEN_READ | OPEN_DIRECTORY));
    for fd in ends {
        sys::close(fd)?;
    }
    Ok(())
}
