//! `t-files KIND`: the file calls from ring 3 (spec §7.3), each answer
//! printed as `<what>: <value or error name>`, so a scenario and the NUC's
//! check script see what the kernel said. It works in the current
//! directory, on `t-files.tmp`.
//!
//! - `t-files basic`: `open` with its flags, `read`, `write`, `seek`,
//!   `fstat` and `close`; an offset shared with a child that got the fd;
//!   32 fds and then `EMFILE`.
//! - `t-files dir`: `mkdir`, `read_dir` a record or two at a time, `stat`,
//!   `truncate`, `touch`, `rename`, `readlink`, `statfs`, `sync`, `unlink`
//!   and `rmdir`, in `t-files.d`.
//! - `t-files cwd`: `chdir` and `getcwd`, and the working directory a child
//!   starts in.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
//!   of `basic`); `t-files pwd` prints its working directory (the children
//!   of `cwd`).
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::errno;
use relay_abi::file::{
    KIND_DIRECTORY, KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE,
    OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE, SEEK_END, SEEK_START, dir_entries,
};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

const TMP: &[u8] = b"t-files.tmp";

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"child") => child(),
        Some(b"dir") => dir(),
        Some(b"cwd") => cwd(),
        Some(b"pwd") => pwd("my working directory"),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd\n");
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

/// Creates `path` holding `bytes`.
fn put(path: &[u8], bytes: &[u8]) -> Result<(), u16> {
    let fd = sys::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    sys::write_all(fd, bytes)?;
    sys::close(fd)
}

fn dir() -> Result<(), u16> {
    show_ok("mkdir", sys::mkdir(b"t-files.d"));
    show_ok("mkdir again", sys::mkdir(b"t-files.d"));
    put(b"t-files.d/b", b"abc")?;
    put(b"t-files.d/a", b"")?;
    sys::mkdir(b"t-files.d/sub")?;
    // One record at a time: a short name's takes 24 bytes, so 40 hold
    // only one.
    let d = sys::open(b"t-files.d", OPEN_READ | OPEN_DIRECTORY)?;
    let mut buf = [0u8; 40];
    let mut calls = 0;
    let _ = write!(Fd(1), "entries:");
    loop {
        let n = sys::read_dir(d, &mut buf)?;
        if n == 0 {
            break;
        }
        calls += 1;
        for r in dir_entries(&buf[..n]) {
            let kind = match r.kind {
                KIND_DIRECTORY => "/",
                KIND_REGULAR => "",
                _ => "?",
            };
            let _ = write!(Fd(1), " ");
            let _ = sys::write_all(1, r.name);
            let _ = write!(Fd(1), "{kind}");
        }
    }
    let _ = writeln!(Fd(1), " ({calls} calls)");
    show("read_dir at the end", sys::read_dir(d, &mut buf));
    show("read_dir of a file", sys::read_dir(1, &mut buf));
    sys::close(d)?;
    show("stat size", sys::stat(b"t-files.d/b", 0).map(|s| s.size));
    show_ok("truncate", sys::truncate(b"t-files.d/b", 10));
    show("stat size", sys::stat(b"t-files.d/b", 0).map(|s| s.size));
    show_ok("touch", sys::touch(b"t-files.d/b"));
    show_ok("touch a missing file", sys::touch(b"t-files.d/nope"));
    show_ok("rename", sys::rename(b"t-files.d/a", b"t-files.d/c"));
    show(
        "stat the old name",
        sys::stat(b"t-files.d/a", 0).map(|s| s.size),
    );
    show(
        "readlink of a file",
        sys::readlink(b"t-files.d/b", &mut buf),
    );
    show_ok("rmdir a full directory", sys::rmdir(b"t-files.d"));
    show_ok("unlink a directory", sys::unlink(b"t-files.d/sub"));
    let f = sys::statfs(b".")?;
    let _ = writeln!(
        Fd(1),
        "statfs: {} blocks, free ones among them {}",
        if f.blocks > 0 { "some" } else { "no" },
        f.free_blocks <= f.blocks
    );
    show_ok("sync", sys::sync());
    sys::unlink(b"t-files.d/b")?;
    sys::unlink(b"t-files.d/c")?;
    sys::rmdir(b"t-files.d/sub")?;
    show_ok("rmdir", sys::rmdir(b"t-files.d"));
    show("stat it", sys::stat(b"t-files.d", 0).map(|s| s.size));
    Ok(())
}

/// Prints the working directory as `<what>: <path>`.
fn pwd(what: &str) -> Result<(), u16> {
    let mut buf = [0u8; 256];
    let n = sys::getcwd(&mut buf)?;
    let _ = write!(Fd(1), "{what}: ");
    sys::write_all(1, &buf[..n])?;
    sys::write_all(1, b"\n")
}

/// Starts `t-files pwd` in `cwd` and waits for it.
fn child_in(cwd: &[u8]) -> Result<(), u16> {
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
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0pwd\0", cwd, &fds, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}

fn cwd() -> Result<(), u16> {
    pwd("getcwd")?;
    show("getcwd into 3 bytes", sys::getcwd(&mut [0; 3]));
    sys::mkdir(b"t-files.c")?;
    show_ok("chdir", sys::chdir(b"t-files.c"));
    pwd("getcwd")?;
    put(b"x", b"here")?;
    show(
        "stat it from above",
        sys::stat(b"../t-files.c/x", 0).map(|s| s.size),
    );
    show_ok("chdir to a file", sys::chdir(b"x"));
    show_ok("chdir to nothing", sys::chdir(b"nope"));
    child_in(b"")?;
    child_in(b"..")?;
    sys::chdir(b"..")?;
    pwd("getcwd")?;
    sys::unlink(b"t-files.c/x")?;
    sys::rmdir(b"t-files.c")
}
