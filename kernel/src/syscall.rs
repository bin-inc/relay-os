//! The system-call dispatcher (user-space gate §7), architecture-neutral:
//! the `arch` entry stub hands it the call number and the six arguments,
//! and it answers with the result register's value or with the program's
//! exit. Plan 2 of milestone 2 serves `exit` and `write` to fds 1 and 2;
//! every other call is `ENOSYS` until the plan that brings it.

use crate::mm::user::UserSlice;
use relay_abi::{Call, encode};
use vfs::Errno;

/// What the dispatcher needs of the program that called: its memory and
/// where its output goes.
pub trait Caller {
    /// Copies `buf.len()` bytes from `offset` into `slice`; `EFAULT` if
    /// they are not all the program's.
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno>;
    /// What the program wrote to fd 1 or 2.
    fn output(&mut self, fd: u32, bytes: &[u8]);
}

/// How a call ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Back to the program with this result register.
    Return(u64),
    /// The program called `exit` with this code.
    Exit(u8),
}

/// Bytes copied out of a program per step of a `write`.
const CHUNK: usize = 4096;

/// Serves call `number` with `args` for `caller`.
pub fn dispatch(caller: &mut impl Caller, number: u64, args: [u64; 6]) -> Outcome {
    let result = match Call::from_number(number) {
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        _ => Err(Errno::ENOSYS),
    };
    Outcome::Return(encode(result.map_err(Errno::number)))
}

/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out in pieces; a piece that is not the program's ends the call
/// with the bytes written before it, or with `EFAULT` if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let fd = match fd {
        1 | 2 => fd as u32,
        _ => return Err(Errno::EBADF),
    };
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; CHUNK];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(CHUNK as u64) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        caller.output(fd, &buf[..n]);
        done += n as u64;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::{PAGE, PageTables, Perm};
    use crate::mm::space::AddressSpace;
    use crate::mm::testing::FakeMem;
    use relay_abi::{decode, errno};

    const U: u64 = 0x40_0000;

    /// A program with three readable pages at `U` holding a pattern, and
    /// nothing after them; what it wrote.
    struct Fake {
        mem: FakeMem,
        space: AddressSpace,
        written: Vec<(u32, Vec<u8>)>,
    }

    impl Caller for Fake {
        fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
            slice.read(&self.space, &mut self.mem, offset, buf)
        }
        fn output(&mut self, fd: u32, bytes: &[u8]) {
            self.written.push((fd, bytes.to_vec()));
        }
    }

    fn fake() -> Fake {
        let mut mem = FakeMem::new();
        let mut k = PageTables::new(&mut mem).unwrap();
        k.fill_upper_half(&mut mem).unwrap();
        let mut space = AddressSpace::new(&mut mem, &k).unwrap();
        space.map_zeroed(&mut mem, U, 3, Perm::Read).unwrap();
        let pattern: Vec<u8> = (0..3 * PAGE).map(|i| (i % 251) as u8).collect();
        space.fill(&mut mem, U, &pattern).unwrap();
        Fake {
            mem,
            space,
            written: Vec::new(),
        }
    }

    fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
        match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
            Outcome::Return(r) => decode(r),
            Outcome::Exit(code) => panic!("exited with {code}"),
        }
    }

    /// Everything written, joined, per fd.
    fn text(f: &Fake, fd: u32) -> Vec<u8> {
        f.written
            .iter()
            .filter(|(d, _)| *d == fd)
            .flat_map(|(_, b)| b.clone())
            .collect()
    }

    #[test]
    fn exit_ends_the_program_with_its_code() {
        let mut f = fake();
        assert_eq!(dispatch(&mut f, 1, [7, 0, 0, 0, 0, 0]), Outcome::Exit(7));
        // The code is a byte.
        assert_eq!(
            dispatch(&mut f, 1, [0x1_02, 0, 0, 0, 0, 0]),
            Outcome::Exit(2)
        );
    }

    #[test]
    fn write_copies_the_buffer_to_fd_1_or_2() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [1, U + 10, 5]), Ok(5));
        assert_eq!(call(&mut f, Call::Write, [2, U, 3]), Ok(3));
        assert_eq!(text(&f, 1), [10, 11, 12, 13, 14]);
        assert_eq!(text(&f, 2), [0, 1, 2]);
        // Across pages, in pieces of at most 4 KiB.
        f.written.clear();
        assert_eq!(call(&mut f, Call::Write, [1, U + 100, 10_000]), Ok(10_000));
        let want: Vec<u8> = (100..10_100).map(|i| (i % 251) as u8).collect();
        assert_eq!(text(&f, 1), want);
        assert!(f.written.iter().all(|(_, b)| b.len() <= 4096));
        assert_eq!(
            call(&mut f, Call::Write, [1, 0, 0]),
            Ok(0),
            "nothing, anywhere"
        );
    }

    #[test]
    fn other_fds_are_ebadf() {
        let mut f = fake();
        for fd in [0, 3, 31, 1 << 32 | 1, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 1]),
                Err(errno::EBADF),
                "{fd}"
            );
        }
        assert!(f.written.is_empty());
    }

    #[test]
    fn a_bad_buffer_is_efault_or_a_short_write() {
        let mut f = fake();
        let end = U + 3 * PAGE;
        assert_eq!(
            call(&mut f, Call::Write, [1, 0, 1]),
            Err(errno::EFAULT),
            "null"
        );
        assert_eq!(call(&mut f, Call::Write, [1, end, 1]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Write, [1, 0xFFFF_FFFF_8000_0000, 8]),
            Err(errno::EFAULT),
            "the kernel"
        );
        assert_eq!(
            call(&mut f, Call::Write, [1, U, u64::MAX]),
            Err(errno::EFAULT)
        );
        assert!(f.written.is_empty());
        // The pieces before the hole are written.
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 5000, 9000]),
            Ok(4096),
            "the first piece only"
        );
        assert_eq!(text(&f, 1).len(), 4096);
    }

    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        for c in Call::ALL {
            if c != Call::Exit && c != Call::Write {
                assert_eq!(call(&mut f, c, [1, U, 1]), Err(errno::ENOSYS), "{c:?}");
            }
        }
        for n in [0, 38, 1000, u64::MAX] {
            assert_eq!(
                dispatch(&mut f, n, [0; 6]),
                Outcome::Return(encode(Err(errno::ENOSYS))),
                "{n}"
            );
        }
        assert!(f.written.is_empty());
    }
}
