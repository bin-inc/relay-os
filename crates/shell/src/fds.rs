//! The fd context (programmable shell gate §7.2, §7.4): what a command's
//! fds 0, 1 and 2 are where the walker stands, and the files its
//! redirections opened. A context is made from the one around it and the
//! command's redirections, left to right; a file is counted by the fds of
//! every context that holds it and closed once none does, so a file a later
//! redirection replaces is closed at once. Under `/bin/sh` a file is an fd
//! of the shell's, which a program gets as one of its fds and a built-in
//! writes through, so both share its offset; the shell's own fds 0 to 2
//! never change. In the in-process runner it is the file's node and an
//! offset every fd of it shares.

use crate::io::Programs;
use crate::parser::{Redirect, RedirectOp};
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs};

/// What one of a command's fds is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The shell's own fd of that number.
    Shell(u32),
    /// A file a redirection opened, by its place in [`Files`].
    File(usize),
    /// In a pipeline, the pipe from the command before.
    PipeIn,
    /// In a pipeline, the pipe to the command after.
    PipeOut,
}

/// A command's fds 0, 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fds(pub [Slot; 3]);

impl Fds {
    /// The shell's own, where nothing is redirected.
    pub const SHELL: Fds = Fds([Slot::Shell(0), Slot::Shell(1), Slot::Shell(2)]);
}

/// The in-process runner's offset of a file opened with `>>`: every write
/// goes to its end, as the kernel's append does, wherever another fd of it
/// has written.
pub(crate) const AT_END: u64 = u64::MAX;

/// Writes all of `bytes` to the in-process runner's file `node` at
/// `offset`, or at its end for [`AT_END`]: where the next write goes, and
/// the error that stopped it (`ENOSPC` for a write that took nothing).
pub(crate) fn write_file(
    vfs: &mut dyn Vfs,
    node: Node,
    offset: u64,
    bytes: &[u8],
) -> (u64, Option<Errno>) {
    let mut at = offset;
    let mut done = 0;
    while done < bytes.len() {
        let pos = if offset == AT_END {
            match vfs.stat(node) {
                Ok(st) => st.size,
                Err(e) => return (at, Some(e)),
            }
        } else {
            at
        };
        match vfs.write_at(node, pos, &bytes[done..]) {
            Ok(0) => return (at, Some(Errno::ENOSPC)),
            Ok(n) => {
                done += n;
                if offset != AT_END {
                    at += n as u64;
                }
            }
            Err(e) => return (at, Some(e)),
        }
    }
    (at, None)
}

/// A file a redirection opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    /// `/bin/sh`'s fd.
    Fd(u32),
    /// The in-process runner's: the file, and where the next read or
    /// write goes ([`AT_END`] for one opened with `>>`).
    Node { node: Node, offset: u64 },
}

/// The files open for redirections, each with how many fds hold it.
#[derive(Default)]
pub(crate) struct Files {
    open: Vec<Option<(Handle, usize)>>,
}

/// How files are opened and closed: through `/bin/sh`'s calls when it has
/// `programs`, or else in the `Vfs`.
pub(crate) struct Opener<'x> {
    pub vfs: &'x mut dyn Vfs,
    pub programs: Option<&'x mut dyn Programs>,
}

impl Opener<'_> {
    /// Opens `path` for reading.
    fn open_input(&mut self, path: &str) -> Result<Handle, Errno> {
        if let Some(programs) = self.programs.as_deref_mut() {
            return programs.open_input(path.as_bytes()).map(Handle::Fd);
        }
        let node = self.vfs.lookup(path.as_bytes())?;
        Ok(Handle::Node { node, offset: 0 })
    }

    /// Opens `path` for output, emptied or, with `append`, at its end.
    fn open(&mut self, path: &str, append: bool) -> Result<Handle, Errno> {
        if let Some(programs) = self.programs.as_deref_mut() {
            return programs
                .open_output(path.as_bytes(), append)
                .map(Handle::Fd);
        }
        let (node, offset) = open_output(&mut *self.vfs, path, append)?;
        Ok(Handle::Node { node, offset })
    }

    fn close(&mut self, handle: Handle) {
        if let (Handle::Fd(fd), Some(programs)) = (handle, self.programs.as_deref_mut()) {
            programs.close(fd);
        }
    }
}

/// Opens a file for output in `vfs`: created if missing, emptied, or with
/// `append` written at its end; the node and the offset to write at.
fn open_output(vfs: &mut dyn Vfs, path: &str, append: bool) -> Result<(Node, u64), Errno> {
    let path = path.as_bytes();
    let node = match vfs.lookup(path) {
        Ok(node) => {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            if !append {
                vfs.truncate(node, 0)?;
            }
            node
        }
        Err(Errno::ENOENT) => vfs.create(path)?,
        Err(e) => return Err(e),
    };
    let offset = if append { AT_END } else { 0 };
    Ok((node, offset))
}

/// A redirection that could not be made: the context as it stood, on
/// whose fd 2 it is told and which the caller releases, and the file and
/// why.
#[derive(Debug)]
pub(crate) struct Failed {
    pub fds: Fds,
    pub path: String,
    pub error: Errno,
}

impl Files {
    /// The file at `i`.
    pub fn handle(&self, i: usize) -> Handle {
        match self.open.get(i) {
            Some(Some((handle, _))) => *handle,
            _ => unreachable!("a slot names an open file"),
        }
    }

    /// The in-process runner's file `i` has been read or written up to
    /// `to`, where every fd of it goes on from (an open file's offset).
    pub fn set_offset(&mut self, i: usize, to: u64) {
        if let Some(Some((Handle::Node { offset, .. }, _))) = self.open.get_mut(i) {
            *offset = to;
        }
    }

    /// The context made from `base` and `redirects`, left to right
    /// (programmable shell gate §7.2), which holds its files until it is
    /// released; a file a later redirection replaces is closed at once.
    pub fn redirect(
        &mut self,
        base: Fds,
        redirects: &[Redirect],
        opener: &mut Opener<'_>,
    ) -> Result<Fds, Failed> {
        let mut fds = base;
        self.hold(&fds);
        for r in redirects {
            let (path, opened) = match &r.op {
                RedirectOp::Read(path) => (path, opener.open_input(path)),
                RedirectOp::Write(path) => (path, opener.open(path, false)),
                RedirectOp::Append(path) => (path, opener.open(path, true)),
                // The other fd as it is now, held once more.
                RedirectOp::Copy(from) => {
                    let slot = fds.0[*from as usize];
                    self.hold_slot(slot);
                    let replaced = core::mem::replace(&mut fds.0[r.fd as usize], slot);
                    self.drop_slot(replaced, opener);
                    continue;
                }
            };
            let slot = match opened {
                Ok(handle) => Slot::File(self.add(handle)),
                Err(error) => {
                    let path = path.clone();
                    return Err(Failed { fds, path, error });
                }
            };
            let fd = r.fd as usize;
            let replaced = core::mem::replace(&mut fds.0[fd], slot);
            self.drop_slot(replaced, opener);
        }
        Ok(fds)
    }

    /// A context made with [`Files::redirect`] ends: the files it held
    /// last are closed.
    pub fn release(&mut self, fds: Fds, opener: &mut Opener<'_>) {
        for slot in fds.0 {
            self.drop_slot(slot, opener);
        }
    }

    /// One more fd holds each of `fds`' files.
    fn hold(&mut self, fds: &Fds) {
        for slot in fds.0 {
            self.hold_slot(slot);
        }
    }

    /// One more fd holds `slot`.
    fn hold_slot(&mut self, slot: Slot) {
        if let Slot::File(i) = slot
            && let Some(Some((_, count))) = self.open.get_mut(i)
        {
            *count += 1;
        }
    }

    /// An fd that held `slot` holds it no more.
    fn drop_slot(&mut self, slot: Slot, opener: &mut Opener<'_>) {
        let Slot::File(i) = slot else {
            return;
        };
        let Some(entry) = self.open.get_mut(i) else {
            return;
        };
        if let Some((handle, count)) = entry {
            *count -= 1;
            if *count == 0 {
                let handle = *handle;
                *entry = None;
                opener.close(handle);
            }
        }
    }

    /// A file just opened, held by one fd: its place.
    pub fn add(&mut self, handle: Handle) -> usize {
        let entry = Some((handle, 1));
        match self.open.iter().position(Option::is_none) {
            Some(i) => {
                self.open[i] = entry;
                i
            }
            None => {
                self.open.push(entry);
                self.open.len() - 1
            }
        }
    }

    /// How many files are open.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.open.iter().flatten().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Harness;

    fn write(fd: u32, path: &str) -> Redirect {
        Redirect {
            fd,
            op: RedirectOp::Write(path.into()),
        }
    }

    #[test]
    fn a_context_holds_its_files_until_it_is_released() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: Some(&mut h.programs),
        };
        let mut files = Files::default();
        let outer = files
            .redirect(Fds::SHELL, &[write(1, "/tmp/a")], &mut opener)
            .unwrap();
        assert_eq!(outer.0, [Slot::Shell(0), Slot::File(0), Slot::Shell(2)]);
        // A command inside holds the same file, and its own.
        let inner = files.redirect(outer, &[], &mut opener).unwrap();
        assert_eq!(inner, outer);
        files.release(inner, &mut opener);
        assert_eq!(files.count(), 1, "the outer context still holds it");
        files.release(outer, &mut opener);
        assert_eq!(files.count(), 0);
        assert_eq!(h.programs.opened, [("/tmp/a".into(), false, 4)]);
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_file_a_later_redirection_replaces_is_closed_at_once() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: Some(&mut h.programs),
        };
        let mut files = Files::default();
        let fds = files
            .redirect(
                Fds::SHELL,
                &[write(1, "/tmp/a"), write(1, "/tmp/b")],
                &mut opener,
            )
            .unwrap();
        // `/tmp/a` (fd 4) is closed as soon as `/tmp/b` takes its place,
        // before the command runs.
        assert_eq!(fds.0[1], Slot::File(1));
        assert_eq!(files.handle(1), Handle::Fd(5));
        assert_eq!(files.count(), 1);
        files.release(fds, &mut opener);
        assert_eq!(h.programs.closed, [4, 5]);
    }

    #[test]
    fn a_failed_redirection_gives_the_context_as_it_stood() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: None,
        };
        let mut files = Files::default();
        let failed = files
            .redirect(
                Fds::SHELL,
                &[write(1, "/tmp/a"), write(1, "/nodir/b")],
                &mut opener,
            )
            .unwrap_err();
        assert_eq!(
            (failed.path.as_str(), failed.error),
            ("/nodir/b", Errno::ENOENT)
        );
        assert_eq!(failed.fds.0[1], Slot::File(0));
        files.release(failed.fds, &mut opener);
        assert_eq!(files.count(), 0);
        // In the `Vfs` a file is made, emptied or appended to.
        h.put("/tmp/a", b"12345");
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: None,
        };
        let append = Redirect {
            fd: 1,
            op: RedirectOp::Append("/tmp/a".into()),
        };
        let fds = files.redirect(Fds::SHELL, &[append], &mut opener).unwrap();
        let Handle::Node { offset, .. } = files.handle(0) else {
            unreachable!()
        };
        assert_eq!(offset, AT_END, "written at its end each time");
        files.release(fds, &mut opener);
        let fds = files
            .redirect(Fds::SHELL, &[write(1, "/tmp/a")], &mut opener)
            .unwrap();
        files.release(fds, &mut opener);
        assert_eq!(h.get("/tmp/a"), b"");
    }
}
