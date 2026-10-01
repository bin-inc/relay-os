//! The console's tees (user-space gate §6.5): files that get a copy of
//! everything written to the console. A process pushes at most 4 at a
//! time, and the stack holds at most 64, one for each process the table
//! can hold, so a background script's transcript never keeps another
//! script from starting (milestone 3); a process pops the newest one it
//! pushed. What they get is buffered and
//! written once 4 KiB wait, and all of it at every `sync` and when the tee
//! is popped, so a big `cat` is never held whole.
//!
//! A write that fails removes the tee from the copying (the kernel logs
//! it); the entry stays until its owner pops it, and that pop returns the
//! write's error, so a script's shell can report it. When a process ends,
//! its tees stop getting copies at once and are written for the last time
//! at the next flush (a process may end where the kernel cannot write
//! files: in a fault or a tick).
//!
//! The stack is generic over the file (`F`) and gets a writer at each
//! flush, so it is tested without files.

use alloc::vec::Vec;
use vfs::Errno;

/// Tees on the stack at most: one for each process the table can hold.
pub const TEES: usize = 64;
/// Tees one process may have pushed at a time.
pub const TEES_EACH: usize = 4;
/// A tee's copies are written once this much waits.
pub const CHUNK: usize = 4096;
/// The echo a tee keeps while it waits: what the line discipline echoes
/// comes from ticks, where no file can be written, so it waits for the
/// next write, read or sync of a process; beyond this it is dropped, as a
/// terminal drops what nobody reads.
pub const ECHO_MAX: usize = 4 * CHUNK;

struct Tee<F> {
    owner: u32,
    /// `None` once a write failed.
    file: Option<F>,
    pending: Vec<u8>,
    error: Option<Errno>,
}

/// How a flush writes a tee's copies to its file: all of them, or the
/// error.
pub type Writer<'a, F> = dyn FnMut(&F, &[u8]) -> Result<(), Errno> + 'a;

pub struct TeeStack<F> {
    tees: Vec<Tee<F>>,
    /// Tees of processes that have ended, written at the next flush.
    closing: Vec<Tee<F>>,
}

impl<F> Default for TeeStack<F> {
    fn default() -> Self {
        TeeStack::new()
    }
}

impl<F> TeeStack<F> {
    pub const fn new() -> TeeStack<F> {
        TeeStack {
            tees: Vec::new(),
            closing: Vec::new(),
        }
    }

    /// Pushes `file` for process `owner`. `EBUSY` if `owner` has 4 tees
    /// pushed, or the stack holds 64.
    pub fn push(&mut self, owner: u32, file: F) -> Result<(), Errno> {
        let mine = self.tees.iter().filter(|t| t.owner == owner).count();
        if mine >= TEES_EACH || self.tees.len() >= TEES {
            return Err(Errno::EBUSY);
        }
        self.tees.push(Tee {
            owner,
            file: Some(file),
            pending: Vec::new(),
            error: None,
        });
        Ok(())
    }

    /// Whether any tee gets copies (so a console write can skip `add`).
    pub fn is_copying(&self) -> bool {
        self.tees.iter().any(|t| t.file.is_some())
    }

    /// A copy of what the console was given, for every tee.
    pub fn add(&mut self, bytes: &[u8]) {
        for t in self.tees.iter_mut().filter(|t| t.file.is_some()) {
            t.pending.extend_from_slice(bytes);
        }
    }

    /// A copy of the line discipline's echo, for every tee, as far as
    /// `ECHO_MAX` of waiting copies allows.
    pub fn add_echo(&mut self, bytes: &[u8]) {
        for t in self.tees.iter_mut().filter(|t| t.file.is_some()) {
            let room = ECHO_MAX.saturating_sub(t.pending.len());
            t.pending.extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
    }

    /// Writes the copies that wait: of the tees with 4 KiB waiting, or of
    /// all of them with `all`, and the last ones of the tees whose process
    /// ended. The tees whose write failed, as (owner, error), for the log.
    pub fn flush(&mut self, all: bool, write: &mut Writer<'_, F>) -> Vec<(u32, Errno)> {
        let mut failed = Vec::new();
        for t in &mut self.tees {
            if (all || t.pending.len() >= CHUNK)
                && let Err(e) = write_out(t, write)
            {
                failed.push((t.owner, e));
            }
        }
        for mut t in core::mem::take(&mut self.closing) {
            if let Err(e) = write_out(&mut t, write) {
                failed.push((t.owner, e));
            }
        }
        failed
    }

    /// Pops the newest tee `owner` pushed, after writing what waits for it.
    /// Its write's error if one failed, now or before; `EINVAL` if `owner`
    /// has none.
    pub fn pop(&mut self, owner: u32, write: &mut Writer<'_, F>) -> Result<(), Errno> {
        let i = self
            .tees
            .iter()
            .rposition(|t| t.owner == owner)
            .ok_or(Errno::EINVAL)?;
        let mut t = self.tees.remove(i);
        let flushed = write_out(&mut t, write);
        match t.error {
            Some(e) => Err(e),
            None => flushed,
        }
    }

    /// Process `owner` ended: its tees get no more copies, and are written
    /// for the last time at the next flush.
    pub fn end(&mut self, owner: u32) {
        let (theirs, others) = core::mem::take(&mut self.tees)
            .into_iter()
            .partition(|t| t.owner == owner);
        self.tees = others;
        self.closing
            .extend(theirs.into_iter().filter(|t| t.file.is_some()));
    }

    /// Every file on the stack (to mark the ones a removal took, spec
    /// §16 item 4).
    pub fn files(&self) -> impl Iterator<Item = &F> {
        self.tees
            .iter()
            .chain(&self.closing)
            .filter_map(|t| t.file.as_ref())
    }
}

/// Writes what waits for `t`; on an error the tee gets no more copies and
/// keeps the error.
fn write_out<F>(t: &mut Tee<F>, write: &mut Writer<'_, F>) -> Result<(), Errno> {
    let Some(file) = &t.file else {
        return Ok(());
    };
    if t.pending.is_empty() {
        return Ok(());
    }
    let r = write(file, &t.pending);
    t.pending.clear();
    if let Err(e) = r {
        t.file = None;
        t.error = Some(e);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// Files by number: what was written to each, and the ones that fail.
    #[derive(Default)]
    struct Files {
        written: BTreeMap<u32, Vec<u8>>,
        writes: usize,
        full: Vec<u32>,
    }

    impl Files {
        fn writer(&mut self) -> impl FnMut(&u32, &[u8]) -> Result<(), Errno> + '_ {
            |f, bytes| {
                self.writes += 1;
                if self.full.contains(f) {
                    return Err(Errno::ENOSPC);
                }
                self.written.entry(*f).or_default().extend_from_slice(bytes);
                Ok(())
            }
        }

        fn of(&self, f: u32) -> &[u8] {
            self.written.get(&f).map_or(&[], |v| v)
        }
    }

    #[test]
    fn at_most_4_tees_a_process_and_each_gets_a_copy() {
        let mut s = TeeStack::new();
        assert!(!s.is_copying());
        for f in 1..=4 {
            s.push(10, f).unwrap();
        }
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY));
        assert!(s.is_copying());
        s.add(b"hello\n");
        let mut files = Files::default();
        assert!(s.flush(true, &mut files.writer()).is_empty());
        for f in 1..=4 {
            assert_eq!(files.of(f), b"hello\n");
        }
    }

    #[test]
    fn other_processes_push_theirs_until_64_are_pushed() {
        let mut s = TeeStack::new();
        for f in 1..=4 {
            s.push(10, f).unwrap();
        }
        // Background scripts, one tee each (the review found the fifth
        // script refused at a stack of 4 for the whole machine).
        for owner in 11..71 {
            s.push(owner, owner).unwrap();
        }
        assert_eq!(s.push(71, 71), Err(Errno::EBUSY), "64 in all");
        let mut files = Files::default();
        s.pop(70, &mut files.writer()).unwrap();
        s.push(71, 71).unwrap();
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY), "still 4 for 10");
    }

    #[test]
    fn copies_are_written_every_4_kib_and_at_a_full_flush() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        let mut files = Files::default();
        s.add(&[b'a'; CHUNK - 1]);
        s.flush(false, &mut files.writer());
        assert_eq!(files.writes, 0, "less than 4 KiB waits");
        s.add(b"b");
        s.flush(false, &mut files.writer());
        assert_eq!((files.writes, files.of(1).len()), (1, CHUNK));
        s.add(b"tail");
        s.flush(true, &mut files.writer());
        assert_eq!(&files.of(1)[CHUNK..], b"tail");
        s.flush(true, &mut files.writer());
        assert_eq!(files.writes, 2, "nothing waits, nothing is written");
    }

    #[test]
    fn a_pop_writes_what_waits_and_takes_the_owner_s_newest() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(20, 2).unwrap();
        s.push(10, 3).unwrap();
        s.add(b"x");
        let mut files = Files::default();
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!((files.of(3), files.of(1)), (&b"x"[..], &b""[..]));
        s.add(b"y");
        s.flush(true, &mut files.writer());
        assert_eq!(files.of(3), b"x", "popped: no more copies");
        assert_eq!((files.of(1), files.of(2)), (&b"xy"[..], &b"xy"[..]));
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
        s.pop(20, &mut files.writer()).unwrap();
        assert!(!s.is_copying());
        // Room for 4 again.
        for f in 1..=4 {
            s.push(30, f).unwrap();
        }
    }

    #[test]
    fn a_failed_write_removes_the_tee_and_its_pop_says_why() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(10, 2).unwrap();
        let mut files = Files {
            full: vec![1],
            ..Files::default()
        };
        s.add(&[b'a'; CHUNK]);
        assert_eq!(
            s.flush(false, &mut files.writer()),
            [(10, Errno::ENOSPC)],
            "for the log"
        );
        let writes = files.writes;
        s.add(&[b'b'; CHUNK]);
        assert!(s.tees[0].pending.is_empty(), "nothing piles up for it");
        s.flush(true, &mut files.writer());
        assert_eq!(files.writes, writes + 1, "tee 1 gets nothing more");
        assert_eq!(s.push(10, 3), Ok(()));
        assert_eq!(s.push(10, 4), Ok(()));
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY), "it still takes its place");
        s.pop(10, &mut files.writer()).unwrap();
        s.pop(10, &mut files.writer()).unwrap();
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::ENOSPC));
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
        // The pop's own write can fail too.
        s.push(10, 1).unwrap();
        s.add(b"z");
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::ENOSPC));
    }

    #[test]
    fn echo_alone_waits_no_more_than_its_limit() {
        // Found by the prototype's review: the line discipline's echo only
        // waits (it comes from ticks, where no file can be written), so a
        // tee of a program that reads and never writes grew without bound.
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        for _ in 0..3 * ECHO_MAX {
            s.add_echo(b"x\x08 \x08");
        }
        assert_eq!(s.tees[0].pending.len(), ECHO_MAX);
        // Once written, it takes echo again.
        let mut files = Files::default();
        s.flush(false, &mut files.writer());
        assert_eq!(files.of(1).len(), ECHO_MAX);
        s.add_echo(b"y");
        assert_eq!(s.tees[0].pending, b"y");
        // What programs write is never dropped.
        s.add(&[b'z'; 2 * ECHO_MAX]);
        assert_eq!(s.tees[0].pending.len(), 2 * ECHO_MAX + 1);
    }

    #[test]
    fn a_process_s_tees_end_with_it_and_are_written_at_the_next_flush() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(20, 2).unwrap();
        s.push(10, 3).unwrap();
        s.add(b"last");
        s.end(10);
        s.add(b"more");
        assert_eq!(s.files().count(), 3, "the closing ones too");
        for f in [4, 5, 6] {
            s.push(30, f).unwrap();
        }
        let mut files = Files::default();
        s.flush(false, &mut files.writer());
        assert_eq!((files.of(1), files.of(3)), (&b"last"[..], &b"last"[..]));
        assert_eq!(files.of(2), b"", "not due yet");
        assert_eq!(s.files().count(), 4, "written for the last time");
        s.flush(true, &mut files.writer());
        assert_eq!(files.of(1), b"last");
        assert_eq!(files.of(2), b"lastmore");
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
    }
}
