//! The file calls (user-space gate §7.3): what a program passes is checked
//! and copied here, the open files do the rest (`crate::file`).

use super::{Caller, PATH_MAX, file};
use crate::fd::{FDS, File};
use crate::file as open_file;
use crate::line::LINE_MAX;
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::sync::Arc;
use alloc::vec::Vec;
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, STAT_NOFOLLOW, Stat};
use vfs::{Errno, Vfs};

/// A path of the program's, copied in: at most 4096 bytes.
pub(super) fn path(caller: &mut impl Caller, addr: u64, len: u64) -> Result<Vec<u8>, Errno> {
    caller.read_str(&UserStr::new(addr, len, PATH_MAX, Errno::ENAMETOOLONG)?)
}

/// `open(path, length, flags)`: the lowest free fd. `EMFILE` when all 32
/// are in use, before anything is opened or created.
pub(super) fn open(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    flags: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let flags = u32::try_from(flags).map_err(|_| Errno::EINVAL)?;
    if caller.with_fds(|t| t.open()) >= FDS {
        return Err(Errno::EMFILE);
    }
    let open = caller.with_vfs(|v| open_file::open(v, &path, flags))?;
    let file = Arc::new(File::Vfs(open));
    caller.with_fds(|t| t.insert(file)).map(u64::from)
}

/// `close(fd)`. The open file lives on while another fd has it.
pub(super) fn close(caller: &mut impl Caller, fd: u64) -> Result<u64, Errno> {
    caller.with_fds(|t| t.remove(fd)).map(|_| 0)
}

/// `read(fd, buffer, length)`: from the file's offset, a page at a time,
/// each page checked writable before anything is read into it, so the
/// offset never moves past what the program got. A bad page ends the
/// call with the bytes read before it, or with `EFAULT` if there were
/// none; so does the end of the file.
pub(super) fn read(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let open = match &*file {
        File::Vfs(open) if open.is_readable() => open,
        File::Vfs(_) | File::ShellOutput(_) => return Err(Errno::EBADF),
        File::Console => return console_read(caller, addr, len),
    };
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(PAGE - (addr + done) % PAGE);
        let page = UserSlice::new(addr + done, n)?;
        let got = caller
            .writable(&page)
            .and_then(|()| caller.with_vfs(|v| open.read(v, &mut buf[..n as usize])));
        match got {
            Ok(k) => {
                caller.write(&slice, done, &buf[..k])?;
                done += k as u64;
                if k < n as usize {
                    break;
                }
            }
            Err(e) if done == 0 => return Err(e),
            Err(_) => break,
        }
    }
    Ok(done)
}

/// A console read: at most one line (or what was typed, in raw mode), into
/// a buffer checked writable before anything is taken from the console.
fn console_read(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let len = len.min(LINE_MAX as u64);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let mut buf = [0u8; LINE_MAX];
    let n = caller.console_read(&mut buf[..len as usize])?;
    caller.write(&slice, 0, &buf[..n])?;
    Ok(n as u64)
}

/// `seek(fd, offset, whence)`: the new offset. The console and the shell's
/// outputs have none (`EINVAL`).
pub(super) fn seek(
    caller: &mut impl Caller,
    fd: u64,
    offset: i64,
    whence: u64,
) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let File::Vfs(open) = &*file else {
        return Err(Errno::EINVAL);
    };
    let whence = u32::try_from(whence).map_err(|_| Errno::EINVAL)?;
    caller.with_vfs(|v| open.seek(v, offset, whence))
}

/// `fstat(fd, &mut Stat)`. The console and the shell's outputs are
/// character devices, with nothing else to say.
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let slice = UserSlice::new(addr, Stat::SIZE as u64)?;
    let stat = match &*file {
        File::Vfs(open) => caller.with_vfs(|v| open.stat(v))?,
        File::Console | File::ShellOutput(_) => Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
            ..Stat::default()
        },
    };
    caller.write(&slice, 0, &stat.to_bytes())?;
    Ok(0)
}

/// `read_dir`'s records are made in a buffer of at most this, and copied
/// out whole.
const READ_DIR_MAX: u64 = 64 * 1024;

/// `stat(path, length, flags, &mut Stat)`. Symbolic links are never
/// followed (milestone 1), so a link's status is its own with
/// `STAT_NOFOLLOW` or without.
pub(super) fn stat(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    flags: u64,
    out: u64,
) -> Result<u64, Errno> {
    if flags & !u64::from(STAT_NOFOLLOW) != 0 {
        return Err(Errno::EINVAL);
    }
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, Stat::SIZE as u64)?;
    let (node, st) = caller.with_vfs(|v| v.lookup(&path).and_then(|n| Ok((n, v.stat(n)?))))?;
    caller.write(&slice, 0, &open_file::stat_of(node, &st).to_bytes())?;
    Ok(0)
}

/// `read_dir(fd, buffer, length)`: the directory's next entries as
/// `relay_abi::file` records, the bytes written; 0 after the last. The
/// buffer is checked writable before anything is read, so no entry is
/// lost to a bad one.
pub(super) fn read_dir(
    caller: &mut impl Caller,
    fd: u64,
    addr: u64,
    len: u64,
) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let File::Vfs(open) = &*file else {
        return Err(Errno::ENOTDIR);
    };
    let len = len.min(READ_DIR_MAX);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let room = caller.heap_room();
    let mut buf = alloc::vec![0u8; len as usize];
    let n = caller.with_vfs(|v| open.read_dir(v, &mut buf, room))?;
    caller.write(&slice, 0, &buf[..n])?;
    Ok(n as u64)
}

/// A call on one path: `mkdir`, `rmdir`, `unlink`, `touch`, `chdir`.
pub(super) fn on_path(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    op: impl FnOnce(&mut dyn Vfs, &[u8]) -> Result<(), Errno>,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    caller.with_vfs(|v| op(v, &path)).map(|()| 0)
}

/// `touch(path, length)`: the file's times set to now; `ENOENT` if it does
/// not exist (a program creates files with `open`).
pub(super) fn touch(v: &mut dyn Vfs, path: &[u8]) -> Result<(), Errno> {
    let node = v.lookup(path)?;
    v.touch(node)
}

/// `truncate(path, length, size)`.
pub(super) fn truncate(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    size: u64,
) -> Result<u64, Errno> {
    on_path(caller, addr, len, |v, path| {
        let node = v.lookup(path)?;
        v.truncate(node, size)
    })
}

/// `readlink(path, length, buffer, buffer length)`: the link's target, cut
/// at the buffer's length as on Linux; the bytes written.
pub(super) fn readlink(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    out: u64,
    out_len: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, out_len)?;
    let target = caller.with_vfs(|v| v.lookup(&path).and_then(|n| v.read_link(n)))?;
    let n = target.len().min(out_len as usize);
    caller.write(&slice, 0, &target[..n])?;
    Ok(n as u64)
}

/// `rename(from, length, to, length)`.
pub(super) fn rename(caller: &mut impl Caller, a: [u64; 4]) -> Result<u64, Errno> {
    let from = path(caller, a[0], a[1])?;
    let to = path(caller, a[2], a[3])?;
    caller.with_vfs(|v| v.rename(&from, &to)).map(|()| 0)
}

/// `statfs(path, length, &mut StatFs)`: the filesystem holding the path.
pub(super) fn statfs(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    out: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, StatFs::SIZE as u64)?;
    let f = caller.with_vfs(|v| v.statfs(&path))?;
    let s = StatFs {
        block_size: f.block_size,
        blocks: f.blocks,
        free_blocks: f.free_blocks,
        avail_blocks: f.avail_blocks,
        files: f.files,
        free_files: f.free_files,
    };
    caller.write(&slice, 0, &s.to_bytes())?;
    Ok(0)
}

/// `getcwd(buffer, length)`: the current directory's path, the bytes
/// written; `ERANGE` if the buffer is too short. A directory that was
/// removed keeps the path it had, as the in-kernel shell's `pwd` shows it.
pub(super) fn getcwd(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let slice = UserSlice::new(addr, len)?;
    let cwd = caller.with_vfs(|v| v.cwd());
    if cwd.len() as u64 > len {
        return Err(Errno::ERANGE);
    }
    caller.write(&slice, 0, &cwd)?;
    Ok(cwd.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use crate::mm::paging::PAGE;
    use relay_abi::Call;
    use relay_abi::errno;
    use relay_abi::file::{
        KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_END,
        SEEK_START,
    };

    /// Opens `path` (put at `W + 512`) with `flags`.
    fn open(f: &mut Fake, path: &[u8], flags: u32) -> Result<u64, u16> {
        put(f, W + 512, path);
        call(
            f,
            Call::Open,
            [W + 512, path.len() as u64, u64::from(flags)],
        )
    }

    #[test]
    fn open_gives_the_lowest_free_fd_and_close_frees_it() {
        let mut f = fake();
        assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(3));
        assert_eq!(
            open(&mut f, b"f", OPEN_READ),
            Ok(4),
            "from /root, the current directory"
        );
        assert_eq!(call(&mut f, Call::Close, [3, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Close, [3, 0, 0]), Err(errno::EBADF));
        assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(3));
        for fd in [5, 31, 32, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Close, [fd, 0, 0]),
                Err(errno::EBADF),
                "{fd}"
            );
        }
        // Up to 32 (7 is `FULL`); the 33rd creates nothing.
        for fd in (5..32).filter(|&fd| fd != FULL) {
            assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(fd));
        }
        assert_eq!(
            open(&mut f, b"/root/new", OPEN_WRITE | OPEN_CREATE),
            Err(errno::EMFILE)
        );
        assert_eq!(open(&mut f, b"/root/new", OPEN_READ), Err(errno::EMFILE));
        assert_eq!(call(&mut f, Call::Close, [4, 0, 0]), Ok(0));
        assert_eq!(
            open(&mut f, b"/root/new", OPEN_READ),
            Err(errno::ENOENT),
            "was not created"
        );
    }

    #[test]
    fn open_refuses_a_bad_path_or_flags() {
        let mut f = fake();
        assert_eq!(
            call(&mut f, Call::Open, [0, 4, u64::from(OPEN_READ)]),
            Err(errno::EFAULT)
        );
        assert_eq!(
            call(&mut f, Call::Open, [W, 4097, u64::from(OPEN_READ)]),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(open(&mut f, b"/root/f", 0), Err(errno::EINVAL));
        put(&mut f, W + 512, b"/root/f");
        assert_eq!(
            call(
                &mut f,
                Call::Open,
                [W + 512, 7, 1 << 32 | u64::from(OPEN_READ)]
            ),
            Err(errno::EINVAL),
            "flags above 32 bits"
        );
        assert_eq!(
            open(
                &mut f,
                b"/root/f",
                OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE
            ),
            Err(errno::EEXIST)
        );
        assert_eq!(open(&mut f, b"", OPEN_READ), Err(errno::ENOENT));
    }

    #[test]
    fn read_and_write_move_the_offset_of_the_open_file() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/f", OPEN_READ | OPEN_WRITE).unwrap();
        assert_eq!(call(&mut f, Call::Read, [fd, W, 2]), Ok(2));
        assert_eq!(get(&mut f, W, 2), b"he");
        // `U` holds the pattern 0, 1, 2, ...
        assert_eq!(call(&mut f, Call::Write, [fd, U + 10, 3]), Ok(3));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, u64::from(SEEK_START)]),
            Ok(0)
        );
        assert_eq!(
            call(&mut f, Call::Read, [fd, W, 100]),
            Ok(5),
            "fewer at the end"
        );
        assert_eq!(get(&mut f, W, 5), [b'h', b'e', 10, 11, 12]);
        assert_eq!(call(&mut f, Call::Read, [fd, W, 100]), Ok(0), "at the end");
        assert_eq!(call(&mut f, Call::Read, [fd, W, 0]), Ok(0));
        let ap = open(&mut f, b"/root/f", OPEN_WRITE | OPEN_APPEND).unwrap();
        assert_eq!(call(&mut f, Call::Write, [ap, U, 2]), Ok(2));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, -2i64 as u64, u64::from(SEEK_END)]),
            Ok(5)
        );
        assert_eq!(call(&mut f, Call::Read, [fd, W, 10]), Ok(2));
        assert_eq!(get(&mut f, W, 2), [0, 1]);
    }

    #[test]
    fn a_read_across_pages_copies_them_all_and_stops_at_a_bad_one() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/big", OPEN_READ | OPEN_WRITE | OPEN_CREATE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [fd, U, 3 * PAGE]), Ok(3 * PAGE));
        call(&mut f, Call::Seek, [fd, 0, 0]).unwrap();
        // Into the one writable page, 100 bytes before its end: 100 bytes,
        // and the offset stays after them.
        assert_eq!(
            call(&mut f, Call::Read, [fd, W + PAGE - 100, 1000]),
            Ok(100)
        );
        assert_eq!(
            get(&mut f, W + PAGE - 100, 100),
            (0..100).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        assert_eq!(call(&mut f, Call::Seek, [fd, 0, 1]), Ok(100));
        assert_eq!(
            call(&mut f, Call::Read, [fd, U, 10]),
            Err(errno::EFAULT),
            "read-only memory"
        );
        assert_eq!(call(&mut f, Call::Read, [fd, 0, 10]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, 1]),
            Ok(100),
            "nothing was read"
        );
        assert_eq!(call(&mut f, Call::Read, [fd, W, PAGE]), Ok(PAGE));
        assert_eq!(get(&mut f, W, 4), [100, 101, 102, 103]);
    }

    #[test]
    fn what_an_fd_was_not_opened_for_is_ebadf() {
        let mut f = fake();
        let r = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        let w = open(&mut f, b"/root/f", OPEN_WRITE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [r, U, 1]), Err(errno::EBADF));
        assert_eq!(
            call(&mut f, Call::Write, [r, 0, 1]),
            Err(errno::EBADF),
            "before the buffer"
        );
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(
            call(&mut f, Call::Read, [1, W, 1]),
            Err(errno::EBADF),
            "an output"
        );
        assert_eq!(
            call(&mut f, Call::Read, [9, W, 1]),
            Err(errno::EBADF),
            "not open"
        );
        let d = open(&mut f, b"/root", OPEN_READ).unwrap();
        assert_eq!(call(&mut f, Call::Read, [d, W, 1]), Err(errno::EISDIR));
        assert_eq!(call(&mut f, Call::Write, [d, U, 1]), Err(errno::EBADF));
    }

    #[test]
    fn a_full_disk_is_a_short_write_then_enospc() {
        let mut f = fake();
        let fd = open(&mut f, b"/full/x", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [fd, U, 3 * PAGE]), Ok(FULL_BYTES));
        assert_eq!(call(&mut f, Call::Write, [fd, U, 10]), Err(errno::ENOSPC));
    }

    #[test]
    fn seek_has_no_meaning_on_the_console() {
        let mut f = fake();
        for fd in [0, 1, 2] {
            assert_eq!(
                call(&mut f, Call::Seek, [fd, 0, 0]),
                Err(errno::EINVAL),
                "{fd}"
            );
        }
        assert_eq!(call(&mut f, Call::Seek, [9, 0, 0]), Err(errno::EBADF));
        let fd = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, 1 << 32]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::Seek, [fd, -1i64 as u64, 0]),
            Err(errno::EINVAL)
        );
    }

    #[test]
    fn fstat_fills_in_the_file_s_status() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(call(&mut f, Call::Fstat, [fd, W, 0]), Ok(0));
        let b = get(&mut f, W, 72);
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(
            (u64_at(8), u32_at(48), u32_at(52)),
            (5, u32::from(KIND_REGULAR), 0o644)
        );
        assert_eq!(call(&mut f, Call::Fstat, [0, W, 0]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_CHAR_DEVICE]);
        assert_eq!(call(&mut f, Call::Fstat, [fd, U, 0]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Fstat, [fd, W + PAGE - 71, 0]),
            Err(errno::EFAULT)
        );
        assert_eq!(call(&mut f, Call::Fstat, [9, W, 0]), Err(errno::EBADF));
    }

    /// Calls `c` on `path` (put at `W + 512`) and the other arguments.
    fn on(f: &mut Fake, c: Call, path: &[u8], rest: [u64; 2]) -> Result<u64, u16> {
        put(f, W + 512, path);
        let args = [W + 512, path.len() as u64, rest[0], rest[1], 0, 0];
        match super::super::dispatch(f, c.number(), args) {
            super::super::Outcome::Return(r) => relay_abi::decode(r),
            super::super::Outcome::Exit(_) => unreachable!(),
        }
    }

    fn u64_at(b: &[u8], i: usize) -> u64 {
        u64::from_ne_bytes(b[i..i + 8].try_into().unwrap())
    }

    #[test]
    fn stat_names_a_file_by_its_path_and_never_follows_a_link() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Stat, b"f", [0, W]), Ok(0));
        assert_eq!(u64_at(&get(&mut f, W, 72), 8), 5, "its size");
        assert_eq!(on(&mut f, Call::Stat, b"link", [0, W]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_SYMLINK]);
        let nofollow = u64::from(relay_abi::file::STAT_NOFOLLOW);
        assert_eq!(on(&mut f, Call::Stat, b"link", [nofollow, W]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_SYMLINK]);
        assert_eq!(on(&mut f, Call::Stat, b"f", [2, W]), Err(errno::EINVAL));
        assert_eq!(on(&mut f, Call::Stat, b"nope", [0, W]), Err(errno::ENOENT));
        assert_eq!(on(&mut f, Call::Stat, b"f", [0, U]), Err(errno::EFAULT));
    }

    #[test]
    fn read_dir_copies_out_records_and_goes_on_where_it_stopped() {
        let mut f = fake();
        let d = open(&mut f, b"/root", OPEN_READ).unwrap();
        // Room for two records of short names.
        let two = 2 * relay_abi::file::DirEntry::record_len(4) as u64;
        let mut names = Vec::new();
        for _ in 0..4 {
            let n = call(&mut f, Call::ReadDir, [d, W, two]).unwrap();
            let bytes = get(&mut f, W, n as usize);
            names.extend(relay_abi::file::dir_entries(&bytes).map(|r| r.name.to_vec()));
        }
        assert_eq!(
            names,
            [
                b".".to_vec(),
                b"..".to_vec(),
                b"f".to_vec(),
                b"link".to_vec()
            ]
        );
        call(&mut f, Call::Seek, [d, 0, 0]).unwrap();
        // A bad buffer loses no entry.
        assert_eq!(call(&mut f, Call::ReadDir, [d, U, two]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::ReadDir, [d, W + PAGE - 8, two]),
            Err(errno::EFAULT)
        );
        let n = call(&mut f, Call::ReadDir, [d, W, two]).unwrap();
        assert_eq!(
            relay_abi::file::dir_entries(&get(&mut f, W, n as usize))
                .next()
                .unwrap()
                .name,
            b"."
        );
        assert_eq!(
            call(&mut f, Call::ReadDir, [d, W, 8]),
            Err(errno::EINVAL),
            "not one fits"
        );
        let file = open(&mut f, b"f", OPEN_READ).unwrap();
        assert_eq!(
            call(&mut f, Call::ReadDir, [file, W, two]),
            Err(errno::ENOTDIR)
        );
        assert_eq!(
            call(&mut f, Call::ReadDir, [0, W, two]),
            Err(errno::ENOTDIR),
            "the console"
        );
        f.heap_room = 0;
        assert_eq!(call(&mut f, Call::ReadDir, [d, W, two]), Err(errno::ENOMEM));
        // A huge buffer is fine: records are made in 64 KiB at most.
        f.heap_room = usize::MAX;
        call(&mut f, Call::Seek, [d, 0, 0]).unwrap();
        assert!(call(&mut f, Call::ReadDir, [d, W, PAGE]).unwrap() > 0);
    }

    #[test]
    fn the_calls_on_a_path_do_what_their_vfs_operation_does() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Mkdir, b"d", [0, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Mkdir, b"d", [0, 0]), Err(errno::EEXIST));
        assert_eq!(open(&mut f, b"d/x", OPEN_WRITE | OPEN_CREATE), Ok(3));
        assert_eq!(on(&mut f, Call::Rmdir, b"d", [0, 0]), Err(errno::ENOTEMPTY));
        assert_eq!(on(&mut f, Call::Truncate, b"d/x", [10, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Stat, b"d/x", [0, W]), Ok(0));
        assert_eq!(u64_at(&get(&mut f, W, 72), 8), 10);
        assert_eq!(on(&mut f, Call::Touch, b"d/x", [0, 0]), Ok(0));
        assert_eq!(
            on(&mut f, Call::Touch, b"d/nope", [0, 0]),
            Err(errno::ENOENT)
        );
        put(&mut f, W + 900, b"d/y");
        assert_eq!(on(&mut f, Call::Rename, b"d/x", [W + 900, 3]), Ok(0));
        assert_eq!(on(&mut f, Call::Unlink, b"d/x", [0, 0]), Err(errno::ENOENT));
        assert_eq!(on(&mut f, Call::Unlink, b"d/y", [0, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Unlink, b"d", [0, 0]), Err(errno::EISDIR));
        assert_eq!(on(&mut f, Call::Rmdir, b"d", [0, 0]), Ok(0));
        put(&mut f, W + 900, b"/full/f");
        assert_eq!(
            on(&mut f, Call::Rename, b"f", [W + 900, 7]),
            Err(errno::EXDEV)
        );
        assert_eq!(on(&mut f, Call::Rename, b"f", [0, 7]), Err(errno::EFAULT));
        assert_eq!(call(&mut f, Call::Sync, [0, 0, 0]), Ok(0));
        for c in [
            Call::Mkdir,
            Call::Rmdir,
            Call::Unlink,
            Call::Touch,
            Call::Chdir,
        ] {
            assert_eq!(
                call(&mut f, c, [W, 4097, 0]),
                Err(errno::ENAMETOOLONG),
                "{c:?}"
            );
            assert_eq!(call(&mut f, c, [0, 1, 0]), Err(errno::EFAULT), "{c:?}");
        }
    }

    #[test]
    fn readlink_gives_the_target_cut_at_the_buffer() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Readlink, b"link", [W, 100]), Ok(1));
        assert_eq!(get(&mut f, W, 1), b"f");
        assert_eq!(on(&mut f, Call::Readlink, b"link", [W, 0]), Ok(0));
        assert_eq!(
            on(&mut f, Call::Readlink, b"f", [W, 100]),
            Err(errno::EINVAL),
            "not a link"
        );
        assert_eq!(
            on(&mut f, Call::Readlink, b"link", [U, 100]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn statfs_fills_in_the_filesystem_s_figures() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Statfs, b"/full", [W, 0]), Ok(0));
        let b = get(&mut f, W, 48);
        let want = vfs::Vfs::statfs(&mut f.vfs, b"/full").unwrap();
        assert_eq!(
            [
                u64_at(&b, 0),
                u64_at(&b, 8),
                u64_at(&b, 16),
                u64_at(&b, 24),
                u64_at(&b, 32),
                u64_at(&b, 40)
            ],
            [
                want.block_size,
                want.blocks,
                want.free_blocks,
                want.avail_blocks,
                want.files,
                want.free_files
            ]
        );
        assert_eq!(
            on(&mut f, Call::Statfs, b"/nope", [W, 0]),
            Err(errno::ENOENT)
        );
        assert_eq!(
            on(&mut f, Call::Statfs, b"/", [W + PAGE - 40, 0]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn chdir_and_getcwd() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Getcwd, [W, 100, 0]), Ok(5));
        assert_eq!(get(&mut f, W, 5), b"/root");
        assert_eq!(call(&mut f, Call::Getcwd, [W, 4, 0]), Err(errno::ERANGE));
        assert_eq!(call(&mut f, Call::Getcwd, [U, 100, 0]), Err(errno::EFAULT));
        assert_eq!(on(&mut f, Call::Chdir, b"/full", [0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Getcwd, [W, 100, 0]), Ok(5));
        assert_eq!(get(&mut f, W, 5), b"/full");
        assert_eq!(
            on(&mut f, Call::Chdir, b"/root/f", [0, 0]),
            Err(errno::ENOTDIR)
        );
        assert_eq!(
            on(&mut f, Call::Chdir, b"/nope", [0, 0]),
            Err(errno::ENOENT)
        );
        assert_eq!(
            open(&mut f, b"../root/f", OPEN_READ),
            Ok(3),
            "relative to /full"
        );
    }

    #[test]
    fn a_console_read_copies_out_what_the_console_gives() {
        let mut f = fake();
        f.typed.push_back(b"hello\n".to_vec());
        assert_eq!(call(&mut f, Call::Read, [0, W, 100]), Ok(6));
        assert_eq!(get(&mut f, W, 6), b"hello\n");
        assert_eq!(f.asked, [100], "the length it asked for");
        // A bad buffer takes nothing from the console.
        f.typed.push_back(b"kept\n".to_vec());
        assert_eq!(call(&mut f, Call::Read, [0, U, 10]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Read, [0, W + PAGE - 2, 10]),
            Err(errno::EFAULT)
        );
        assert_eq!(f.typed.len(), 1, "still there");
        // At most a line's worth is asked for, and checked, whatever the
        // buffer's length.
        assert_eq!(call(&mut f, Call::Read, [0, W, 1 << 40]), Ok(5));
        assert_eq!(f.asked, [100, crate::line::LINE_MAX]);
        // Nothing more: end of input.
        assert_eq!(call(&mut f, Call::Read, [0, W, 10]), Ok(0));
        f.killed_while_reading = true;
        assert_eq!(call(&mut f, Call::Read, [0, W, 10]), Err(errno::EINTR));
    }
}
