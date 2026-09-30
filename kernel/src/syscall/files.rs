//! The file calls (user-space gate §7.3): what a program passes is checked
//! and copied here, the open files do the rest (`crate::file`).

use super::{Caller, PATH_MAX, file};
use crate::fd::{FDS, File};
use crate::file as open_file;
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::sync::Arc;
use alloc::vec::Vec;
use relay_abi::file::{KIND_CHAR_DEVICE, Stat};
use vfs::Errno;

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
        // The console's reads come with its line discipline.
        File::Console => return Err(Errno::ENOSYS),
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

    #[test]
    fn reading_the_console_waits_for_its_line_discipline() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Read, [0, W, 1]), Err(errno::ENOSYS));
    }
}
