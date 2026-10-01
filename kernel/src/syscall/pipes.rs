//! The `pipe` call, and reading and writing a pipe's ends (user-space gate
//! §7.3, §9.1, §16 item 8). The ring never blocks (`pipe::Pipe`); the
//! waiting is here, through the `Caller`: a read waits only while the pipe
//! is empty and a writer is open, a write only while nothing of it has
//! gone in. A process killed while it waits gets `EINTR` from the wait,
//! and ends before its program sees it.

use super::Caller;
use crate::fd::{FDS, File};
use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use crate::pipe::{self, End, Side};
use alloc::sync::Arc;
use vfs::Errno;

/// `pipe(&mut [u32; 2])`: a new pipe, its read end and its write end as
/// the two lowest free fds. The memory is checked writable, and two fds
/// free (`EMFILE`), before anything is made; `ENOMEM` when the ring's
/// frames would eat into the reserve.
pub(super) fn pipe(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let slice = UserSlice::new(addr, 8)?;
    caller.writable(&slice)?;
    if caller.with_fds(|t| FDS - t.open()) < 2 {
        return Err(Errno::EMFILE);
    }
    let (read, write) = caller.new_pipe()?;
    let (read, write) = (Arc::new(File::Pipe(read)), Arc::new(File::Pipe(write)));
    let fds = caller.with_fds(|t| Ok::<_, Errno>([t.insert(read)?, t.insert(write)?]))?;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&fds[0].to_ne_bytes());
    bytes[4..].copy_from_slice(&fds[1].to_ne_bytes());
    caller.write(&slice, 0, &bytes)?;
    Ok(0)
}

/// Reads a read end into the program's buffer: what the pipe holds, up to
/// the buffer's length (at most the ring's size), waiting only while it
/// holds nothing and a writer is open; 0 at the end of the data. The
/// buffer is checked writable first, so nothing taken is lost.
pub(super) fn read(caller: &mut impl Caller, end: &End, addr: u64, len: u64) -> Result<u64, Errno> {
    if end.side() != Side::Read {
        return Err(Errno::EBADF);
    }
    let len = len.min(pipe::SIZE as u64);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let want = (len - done).min(PAGE) as usize;
        match end.pipe().read(&mut buf[..want]) {
            Some(0) => break,
            Some(n) => {
                caller.pipe_wake(end.pipe().id());
                caller.write(&slice, done, &buf[..n])?;
                done += n as u64;
            }
            None if done > 0 => break,
            None => caller.pipe_wait(end.pipe().id())?,
        }
    }
    Ok(done)
}

/// Writes the program's buffer to a write end, a page at a time: as much
/// as goes in, waiting only while nothing of it has (so a full pipe gives
/// fewer bytes than asked). `EPIPE` once every read end has closed, unless
/// some of the buffer went in first; a page that is not the program's ends
/// it the same way (`EFAULT`).
pub(super) fn write(
    caller: &mut impl Caller,
    end: &End,
    addr: u64,
    len: u64,
) -> Result<u64, Errno> {
    if end.side() != Side::Write {
        return Err(Errno::EBADF);
    }
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done == 0 { Err(e) } else { Ok(done) };
        }
        let mut put = 0;
        while put < n {
            match end.pipe().write(&buf[put..n]) {
                Ok(Some(k)) => {
                    put += k;
                    caller.pipe_wake(end.pipe().id());
                }
                Ok(None) if done + put as u64 > 0 => return Ok(done + put as u64),
                Ok(None) => caller.pipe_wait(end.pipe().id())?,
                Err(e) if done + put as u64 == 0 => return Err(e),
                Err(_) => return Ok(done + put as u64),
            }
        }
        done += n as u64;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::*;
    use relay_abi::errno;
    use relay_abi::file::{KIND_FIFO, SEEK_START};

    /// A new pipe's fds.
    fn new_pipe(f: &mut Fake) -> (u64, u64) {
        assert_eq!(call(f, Call::Pipe, [W, 0, 0]), Ok(0));
        let fds = get(f, W, 8);
        let n = |b: &[u8]| u64::from(u32::from_ne_bytes(b.try_into().unwrap()));
        (n(&fds[..4]), n(&fds[4..]))
    }

    #[test]
    fn a_pipe_s_ends_are_the_lowest_free_fds_read_end_first() {
        let mut f = fake();
        assert_eq!(new_pipe(&mut f), (3, 4));
        assert_eq!(call(&mut f, Call::Close, [1, 0, 0]), Ok(0));
        assert_eq!(new_pipe(&mut f), (1, 5));
        // The memory first, then two free fds, then the ring's frames.
        assert_eq!(
            call(&mut f, Call::Pipe, [U, 0, 0]),
            Err(errno::EFAULT),
            "read-only"
        );
        assert_eq!(
            call(&mut f, Call::Pipe, [W + PAGE - 4, 0, 0]),
            Err(errno::EFAULT)
        );
        assert_eq!(f.pipes_made, 2);
        for _ in 6..31 {
            open_any(&mut f);
        }
        assert_eq!(f.fds.open(), 31);
        assert_eq!(
            call(&mut f, Call::Pipe, [W, 0, 0]),
            Err(errno::EMFILE),
            "one left"
        );
        assert_eq!(f.fds.open(), 31, "none taken");
        assert_eq!(f.pipes_made, 2, "no ring made");
        assert_eq!(call(&mut f, Call::Close, [30, 0, 0]), Ok(0));
        f.no_pipe_memory = true;
        assert_eq!(call(&mut f, Call::Pipe, [W, 0, 0]), Err(errno::ENOMEM));
        assert_eq!(f.fds.open(), 30);
    }

    /// Opens `/root/f` as the next fd.
    fn open_any(f: &mut Fake) {
        put(f, W + 512, b"/root/f");
        let flags = u64::from(relay_abi::file::OPEN_READ);
        call(f, Call::Open, [W + 512, 7, flags]).unwrap();
    }

    #[test]
    fn what_goes_in_one_end_comes_out_of_the_other() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // `U` holds the pattern 0, 1, 2, ...
        assert_eq!(call(&mut f, Call::Write, [w, U + 10, 5]), Ok(5));
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 100]),
            Ok(5),
            "what is there"
        );
        assert_eq!(get(&mut f, W, 5), [10, 11, 12, 13, 14]);
        // Across pages, in one call.
        assert_eq!(call(&mut f, Call::Write, [w, U + 100, 10_000]), Ok(10_000));
        assert_eq!(call(&mut f, Call::Read, [r, W, 3000]), Ok(3000));
        assert_eq!(get(&mut f, W, 3), [100, 101, 102]);
        let rest = W + PAGE - 7000;
        assert!(
            call(&mut f, Call::Read, [r, rest, 7000]).is_err(),
            "not all writable"
        );
        assert_eq!(f.waits, [], "nothing waited");
        assert!(!f.woken.is_empty(), "the other side is woken");
    }

    #[test]
    fn a_buffer_not_all_the_program_s_loses_no_byte_and_doubles_none() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // `U` holds the pattern 0, 1, 2, ...; the page after `W` is no one's.
        let pattern: Vec<u8> = (0..300).map(|i| (i % 251) as u8).collect();
        assert_eq!(call(&mut f, Call::Write, [w, U, 300]), Ok(300));
        // A read whose buffer runs out of the program's memory takes nothing.
        assert_eq!(
            call(&mut f, Call::Read, [r, W + PAGE - 100, 200]),
            Err(errno::EFAULT)
        );
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 1000]),
            Ok(300),
            "all still there"
        );
        assert_eq!(get(&mut f, W, 300), pattern);
        // A write whose second page is no one's puts the first page's bytes,
        // and says so, so that a program never writes them twice.
        put(&mut f, W + PAGE - 100, &[7; 100]);
        assert_eq!(call(&mut f, Call::Write, [w, W + PAGE - 100, 200]), Ok(100));
        assert_eq!(call(&mut f, Call::Read, [r, W, 1000]), Ok(100));
        assert_eq!(get(&mut f, W, 100), [7; 100]);
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 1000]),
            Err(errno::EINTR),
            "nothing more: the fake's wait"
        );
    }

    #[test]
    fn an_empty_pipe_waits_and_a_killed_wait_is_eintr() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // The fake's wait ends as a kill would end it.
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Err(errno::EINTR));
        assert_eq!(f.waits.len(), 1);
        // Full: the next write waits (and is killed), having taken nothing.
        assert_eq!(call(&mut f, Call::Write, [w, U, 3 * PAGE]), Ok(3 * PAGE));
        assert_eq!(
            call(&mut f, Call::Write, [w, U, 3 * PAGE]),
            Ok(PAGE),
            "what fits"
        );
        assert_eq!(f.waits.len(), 1, "it took some, so it did not wait");
        assert_eq!(call(&mut f, Call::Write, [w, U, 1]), Err(errno::EINTR));
        assert_eq!(f.waits.len(), 2);
        assert_eq!(f.waits[0], f.waits[1], "the same pipe");
    }

    #[test]
    fn the_last_writer_closing_is_the_end_of_the_data() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Write, [w, U, 3]), Ok(3));
        assert_eq!(call(&mut f, Call::Close, [w, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(3));
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(0), "no wait");
        assert_eq!(f.waits, []);
    }

    #[test]
    fn the_last_reader_closing_makes_writes_epipe() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Close, [r, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Write, [w, U, 3]), Err(errno::EPIPE));
        assert_eq!(f.waits, []);
    }

    #[test]
    fn an_end_shared_by_two_fds_closes_with_the_last() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // As a child given the write end would hold it.
        let held = Arc::clone(f.fds.get(w).unwrap());
        assert_eq!(call(&mut f, Call::Close, [w, 0, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 10]),
            Err(errno::EINTR),
            "it waits"
        );
        drop(held);
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(0));
    }

    #[test]
    fn what_a_pipe_s_end_is_not() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(call(&mut f, Call::Write, [r, U, 1]), Err(errno::EBADF));
        for fd in [r, w] {
            assert_eq!(
                call(&mut f, Call::Seek, [fd, 0, u64::from(SEEK_START)]),
                Err(errno::EINVAL)
            );
            assert_eq!(
                call(&mut f, Call::ReadDir, [fd, W, 100]),
                Err(errno::ENOTDIR)
            );
            assert_eq!(call(&mut f, Call::Fstat, [fd, W, 0]), Ok(0));
            let b = get(&mut f, W, relay_abi::Stat::SIZE);
            let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
            let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
            // `kind` at 48, `size` at 8, `dev` at 72.
            assert_eq!(
                (u32_at(48), u64_at(8), u64_at(72)),
                (u32::from(KIND_FIFO), 0, 0)
            );
        }
        assert_eq!(f.waits, []);
    }
}
