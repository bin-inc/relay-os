//! Pipes (user-space gate §9.1, §16 item 8): a ring of 16 KiB between the
//! processes that have its read end and those that have its write end.
//!
//! A read gets what the ring holds, up to its length, and must wait only
//! while the ring is empty and a write end is open; once every write end
//! has closed it gets 0, the end of the data. A write takes what fits and
//! must wait only while nothing fits; once every read end has closed it is
//! `EPIPE`. The waiting is the caller's: these calls never block, they say
//! when the caller would have to. An end closes when the last fd that has
//! it closes, and the processes waiting on the pipe are woken, to find
//! the end of the data or `EPIPE`.
//!
//! The ring lives in memory of its own (`Memory`): four frames in the
//! kernel, a buffer in the tests. Waking is the kernel's too: a pipe is
//! made with the function that wakes its waiters (`proc::wake_pipe`).

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use spin::Mutex;
use vfs::Errno;

/// The ring's size.
pub const SIZE: usize = 16 * 1024;

/// Where a pipe's bytes are: [`SIZE`] bytes that are the pipe's alone, for
/// as long as it lives.
pub trait Memory: Send {
    fn bytes(&mut self) -> &mut [u8; SIZE];
}

pub struct Pipe {
    ring: Mutex<Ring>,
    /// Wakes the processes waiting on the pipe whose id it is given.
    wake: fn(u64),
}

struct Ring {
    memory: Box<dyn Memory>,
    /// Where the oldest byte is, and how many there are.
    start: usize,
    len: usize,
    readers: u32,
    writers: u32,
}

impl Pipe {
    /// What the processes waiting on this pipe wait for.
    pub fn id(&self) -> u64 {
        self as *const Pipe as u64
    }

    /// Takes up to `buf.len()` bytes into `buf`: `Some(n)` with `n` bytes,
    /// `Some(0)` at the end of the data (every write end closed), `None`
    /// when the ring is empty and a write end is open (the caller waits).
    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut r = self.ring.lock();
        if r.len == 0 {
            return (r.writers == 0).then_some(0);
        }
        let n = buf.len().min(r.len);
        let start = r.start;
        let ring = r.memory.bytes();
        let first = n.min(SIZE - start);
        buf[..first].copy_from_slice(&ring[start..start + first]);
        buf[first..n].copy_from_slice(&ring[..n - first]);
        r.start = (start + n) % SIZE;
        r.len -= n;
        Some(n)
    }

    /// Puts as much of `bytes` as fits: `Ok(Some(n))` with `n` bytes taken
    /// (0 only for an empty `bytes`), `Ok(None)` when the ring is full (the
    /// caller waits), `EPIPE` once every read end has closed.
    pub fn write(&self, bytes: &[u8]) -> Result<Option<usize>, Errno> {
        let mut r = self.ring.lock();
        if r.readers == 0 {
            return Err(Errno::EPIPE);
        }
        let n = bytes.len().min(SIZE - r.len);
        if n == 0 && !bytes.is_empty() {
            return Ok(None);
        }
        let at = (r.start + r.len) % SIZE;
        let ring = r.memory.bytes();
        let first = n.min(SIZE - at);
        ring[at..at + first].copy_from_slice(&bytes[..first]);
        ring[..n - first].copy_from_slice(&bytes[first..n]);
        r.len += n;
        Ok(Some(n))
    }
}

/// Which end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Read,
    Write,
}

/// One end of a pipe, as the fd table holds it (shared by every fd that
/// has it). Dropping it closes it.
pub struct End {
    pipe: Arc<Pipe>,
    side: Side,
}

impl End {
    pub fn pipe(&self) -> &Pipe {
        &self.pipe
    }

    pub fn side(&self) -> Side {
        self.side
    }
}

impl fmt::Debug for End {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "End({:#x}, {:?})", self.pipe.id(), self.side)
    }
}

/// Ends are the same only if they are one.
impl PartialEq for End {
    fn eq(&self, other: &End) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for End {}

impl Drop for End {
    /// The last fd of this end closed: the other side's waiters wake, to
    /// find the end of the data or `EPIPE`.
    fn drop(&mut self) {
        {
            let mut r = self.pipe.ring.lock();
            match self.side {
                Side::Read => r.readers -= 1,
                Side::Write => r.writers -= 1,
            }
        }
        (self.pipe.wake)(self.pipe.id());
    }
}

/// A new pipe in `memory`, whose waiters `wake` wakes: its read end and
/// its write end.
pub fn new(memory: Box<dyn Memory>, wake: fn(u64)) -> (End, End) {
    let pipe = Arc::new(Pipe {
        ring: Mutex::new(Ring {
            memory,
            start: 0,
            len: 0,
            readers: 1,
            writers: 1,
        }),
        wake,
    });
    let read = End {
        pipe: Arc::clone(&pipe),
        side: Side::Read,
    };
    (
        read,
        End {
            pipe,
            side: Side::Write,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    struct Buffer(Box<[u8; SIZE]>);

    impl Memory for Buffer {
        fn bytes(&mut self) -> &mut [u8; SIZE] {
            &mut self.0
        }
    }

    std::thread_local! {
        /// The ids `wake` was called with, in this test's thread.
        static WOKEN: core::cell::RefCell<Vec<u64>> = const { core::cell::RefCell::new(Vec::new()) };
    }

    fn wake(id: u64) {
        WOKEN.with(|w| w.borrow_mut().push(id));
    }

    fn woken() -> Vec<u64> {
        WOKEN.with(|w| core::mem::take(&mut *w.borrow_mut()))
    }

    fn pipe() -> (End, End) {
        new(Box::new(Buffer(Box::new([0; SIZE]))), wake)
    }

    #[test]
    fn what_is_written_is_read_in_order_in_any_pieces() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(b"hello, "), Ok(Some(7)));
        assert_eq!(w.pipe().write(b"world"), Ok(Some(5)));
        let mut buf = [0; 4];
        assert_eq!(r.pipe().read(&mut buf), Some(4));
        assert_eq!(&buf, b"hell");
        let mut buf = [0; 100];
        assert_eq!(r.pipe().read(&mut buf), Some(8), "what is there, no more");
        assert_eq!(&buf[..8], b"o, world");
        assert_eq!(r.pipe().read(&mut buf), None, "empty: the reader waits");
        assert_eq!(r.pipe().read(&mut []), None);
        assert_eq!(w.pipe().write(b""), Ok(Some(0)), "nothing to write");
    }

    #[test]
    fn a_full_ring_takes_what_fits_then_makes_the_writer_wait() {
        let (r, w) = pipe();
        let data: Vec<u8> = (0..SIZE + 1000).map(|i| (i % 251) as u8).collect();
        assert_eq!(w.pipe().write(&data[..SIZE - 10]), Ok(Some(SIZE - 10)));
        assert_eq!(
            w.pipe().write(&data[SIZE - 10..]),
            Ok(Some(10)),
            "what fits"
        );
        assert_eq!(w.pipe().write(&data[SIZE..]), Ok(None), "full: it waits");
        // Room again, across the ring's end.
        let mut buf = vec![0; 3000];
        assert_eq!(r.pipe().read(&mut buf), Some(3000));
        assert_eq!(buf, data[..3000]);
        assert_eq!(w.pipe().write(&data[SIZE..]), Ok(Some(1000)));
        let mut got = Vec::new();
        let mut buf = vec![0; 5000];
        // Bounded: a pipe that never empties ends the loop anyway.
        for _ in 0..10 {
            match r.pipe().read(&mut buf) {
                Some(n) if n > 0 => got.extend_from_slice(&buf[..n]),
                _ => break,
            }
        }
        assert_eq!(got, data[3000..], "every byte once, in order");
    }

    #[test]
    fn the_ring_wraps_around_many_times() {
        let (r, w) = pipe();
        let mut next = 0u8;
        let mut want = 0u8;
        let mut buf = vec![0; 7000];
        for round in 0..50 {
            let piece: Vec<u8> = (0..6001)
                .map(|_| {
                    next = next.wrapping_add(1);
                    next
                })
                .collect();
            assert_eq!(w.pipe().write(&piece), Ok(Some(6001)), "{round}");
            assert_eq!(r.pipe().read(&mut buf), Some(6001), "{round}");
            for &b in &buf[..6001] {
                want = want.wrapping_add(1);
                assert_eq!(b, want, "{round}");
            }
        }
    }

    #[test]
    fn closing_the_write_end_is_the_end_of_the_data_after_what_is_left() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(b"last"), Ok(Some(4)));
        drop(w);
        let mut buf = [0; 10];
        assert_eq!(r.pipe().read(&mut buf), Some(4));
        assert_eq!(&buf[..4], b"last");
        assert_eq!(r.pipe().read(&mut buf), Some(0), "the end, not a wait");
        assert_eq!(r.pipe().read(&mut buf), Some(0));
    }

    #[test]
    fn closing_the_read_end_makes_every_write_epipe() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(&[7; SIZE]), Ok(Some(SIZE)));
        assert_eq!(w.pipe().write(b"x"), Ok(None));
        drop(r);
        assert_eq!(
            w.pipe().write(b"x"),
            Err(Errno::EPIPE),
            "full, but no reader"
        );
        assert_eq!(w.pipe().write(b""), Err(Errno::EPIPE));
    }

    #[test]
    fn closing_an_end_wakes_the_pipe_s_waiters_once() {
        let (r, w) = pipe();
        let id = r.pipe().id();
        assert_eq!(w.pipe().write(b"x"), Ok(Some(1)));
        assert_eq!(r.pipe().read(&mut [0; 2]), Some(1));
        assert_eq!(woken(), [], "reads and writes do not: their callers do");
        drop(w);
        assert_eq!(woken(), [id]);
        drop(r);
        assert_eq!(woken(), [id]);
    }

    #[test]
    fn ends_know_their_side_and_pipe() {
        let (r, w) = pipe();
        assert_eq!((r.side(), w.side()), (Side::Read, Side::Write));
        assert_eq!(r.pipe().id(), w.pipe().id());
        let (r2, _w2) = pipe();
        assert_ne!(r.pipe().id(), r2.pipe().id());
        assert_ne!(r, r2);
        assert_eq!(r, r);
    }
}
