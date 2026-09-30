//! Files of the VFS as processes have them open (user-space gate §7.3,
//! §16 item 4): a node, what the file was opened for, and an offset that
//! every fd sharing the open file shares (the fds `spawn` hands a child);
//! each `open` makes a new one with an offset of its own, as on Linux.
//!
//! `open`'s flags and their errors are Linux's where they apply. `seek`
//! may go past the end (a write there leaves a hole). `read_dir` gives a
//! directory's entries sorted by name and continues after the last name it
//! gave, so an entry that is there throughout comes exactly once however
//! the directory changes between calls.
//!
//! A removal elsewhere can free the file's inode, which the filesystem may
//! then give to a new file; the kernel marks every open file of it gone,
//! and from then on everything but closing it is `ENOENT`, what the
//! filesystem says for an inode it has freed.
//!
//! The functions take the `Vfs` to use, so they are tested over a `MemFs`.

use alloc::vec::Vec;
use core::fmt;
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, KIND_UNKNOWN, OPEN_APPEND, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE,
    OPEN_FLAGS, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE, SEEK_CURRENT, SEEK_END, SEEK_START,
    put_dir_entry,
};
use spin::Mutex;
use vfs::{Errno, FileType, Node, Vfs};

/// A file of the VFS, open.
pub struct OpenFile {
    node: Node,
    read: bool,
    write: bool,
    append: bool,
    dir: bool,
    state: Mutex<State>,
}

struct State {
    offset: u64,
    /// The last name `read_dir` gave.
    after: Option<Vec<u8>>,
    /// A removal freed the inode.
    gone: bool,
}

impl fmt::Debug for OpenFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpenFile({:?})", self.node)
    }
}

/// Open files are the same only if they are one (fds that share it).
impl PartialEq for OpenFile {
    fn eq(&self, other: &OpenFile) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for OpenFile {}

/// Opens `path` with `open`'s `flags` (spec §7.3).
pub fn open(vfs: &mut dyn Vfs, path: &[u8], flags: u32) -> Result<OpenFile, Errno> {
    let has = |f: u32| flags & f != 0;
    let valid = flags & !OPEN_FLAGS == 0
        && (has(OPEN_READ) || has(OPEN_WRITE))
        && (has(OPEN_WRITE) || !has(OPEN_TRUNCATE | OPEN_APPEND))
        && (has(OPEN_CREATE) || !has(OPEN_EXCLUSIVE))
        && !(has(OPEN_CREATE) && has(OPEN_DIRECTORY));
    if !valid {
        return Err(Errno::EINVAL);
    }
    let (node, created) = match vfs.lookup(path) {
        Ok(_) if has(OPEN_CREATE) && has(OPEN_EXCLUSIVE) => return Err(Errno::EEXIST),
        Ok(node) => (node, false),
        Err(Errno::ENOENT) if has(OPEN_CREATE) => (vfs.create(path)?, true),
        Err(e) => return Err(e),
    };
    let dir = !created && vfs.stat(node)?.kind == FileType::Directory;
    if dir && has(OPEN_WRITE) {
        return Err(Errno::EISDIR);
    }
    if !dir && has(OPEN_DIRECTORY) {
        return Err(Errno::ENOTDIR);
    }
    if has(OPEN_TRUNCATE) && !created {
        vfs.truncate(node, 0)?;
    }
    Ok(OpenFile {
        node,
        read: has(OPEN_READ),
        write: has(OPEN_WRITE),
        append: has(OPEN_APPEND),
        dir,
        state: Mutex::new(State {
            offset: 0,
            after: None,
            gone: false,
        }),
    })
}

/// A `KIND_*` number.
pub fn kind(t: FileType) -> u8 {
    match t {
        FileType::Regular => KIND_REGULAR,
        FileType::Directory => KIND_DIRECTORY,
        FileType::Symlink => KIND_SYMLINK,
        FileType::CharDev => KIND_CHAR_DEVICE,
        FileType::BlockDev => KIND_BLOCK_DEVICE,
        FileType::Fifo => KIND_FIFO,
        FileType::Socket => KIND_SOCKET,
    }
}

/// `stat`'s answer for a program.
pub fn stat_of(s: &vfs::Stat) -> relay_abi::Stat {
    relay_abi::Stat {
        ino: s.ino,
        size: s.size,
        blocks: s.blocks,
        atime: s.atime,
        mtime: s.mtime,
        ctime: s.ctime,
        kind: u32::from(kind(s.kind)),
        perm: u32::from(s.perm),
        nlink: s.nlink,
        uid: s.uid,
        gid: s.gid,
        block_size: s.block_size,
    }
}

/// How much bigger than a directory's size its entries may be in memory
/// (an ext2 entry of 12 bytes becomes about 48): `read_dir` refuses a
/// directory the heap has no room for, since the kernel's heap panics when
/// it runs out.
const DIR_MEMORY_FACTOR: u64 = 4;

impl OpenFile {
    pub fn node(&self) -> Node {
        self.node
    }

    pub fn is_readable(&self) -> bool {
        self.read
    }

    pub fn is_writable(&self) -> bool {
        self.write
    }

    pub fn is_dir(&self) -> bool {
        self.dir
    }

    /// A removal freed the file's inode.
    pub fn mark_gone(&self) {
        self.state.lock().gone = true;
    }

    /// The offset now, after checking the file is still there.
    fn offset(&self) -> Result<u64, Errno> {
        let s = self.state.lock();
        if s.gone {
            return Err(Errno::ENOENT);
        }
        Ok(s.offset)
    }

    /// Reads from the offset, which moves past what was read; 0 at or past
    /// the end.
    pub fn read(&self, vfs: &mut dyn Vfs, buf: &mut [u8]) -> Result<usize, Errno> {
        let at = self.offset()?;
        if !self.read {
            return Err(Errno::EBADF);
        }
        if self.dir {
            return Err(Errno::EISDIR);
        }
        let n = vfs.read_at(self.node, at, buf)?;
        self.state.lock().offset = at + n as u64;
        Ok(n)
    }

    /// Writes at the offset (at the end if opened to append), which moves
    /// past what was written. Fewer bytes than asked when the filesystem
    /// fills up; `ENOSPC` when not one fits.
    pub fn write(&self, vfs: &mut dyn Vfs, bytes: &[u8]) -> Result<usize, Errno> {
        let mut at = self.offset()?;
        if !self.write {
            return Err(Errno::EBADF);
        }
        if self.append {
            at = vfs.stat(self.node)?.size;
        }
        let n = vfs.write_at(self.node, at, bytes)?;
        self.state.lock().offset = at + n as u64;
        Ok(n)
    }

    /// Writes all of `bytes`, as a tee does: `ENOSPC` if the filesystem
    /// fills up first.
    pub fn write_all(&self, vfs: &mut dyn Vfs, mut bytes: &[u8]) -> Result<(), Errno> {
        while !bytes.is_empty() {
            match self.write(vfs, bytes)? {
                0 => return Err(Errno::ENOSPC),
                n => bytes = &bytes[n..],
            }
        }
        Ok(())
    }

    /// Moves the offset (spec §7.3): from the start, the offset or the end.
    /// Past the end is allowed; before the start, past 2^63 − 1 and an
    /// unknown `whence` are `EINVAL`. A directory only goes back to its
    /// start, where `read_dir` begins again.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.dir {
            if (offset, whence) != (0, SEEK_START) {
                return Err(Errno::EINVAL);
            }
            let mut s = self.state.lock();
            s.offset = 0;
            s.after = None;
            return Ok(0);
        }
        let base = match whence {
            SEEK_START => 0,
            SEEK_CURRENT => at,
            SEEK_END => vfs.stat(self.node)?.size,
            _ => return Err(Errno::EINVAL),
        };
        let to = i64::try_from(base)
            .ok()
            .and_then(|b| b.checked_add(offset))
            .filter(|&to| to >= 0)
            .ok_or(Errno::EINVAL)? as u64;
        self.state.lock().offset = to;
        Ok(to)
    }

    pub fn stat(&self, vfs: &mut dyn Vfs) -> Result<relay_abi::Stat, Errno> {
        self.offset()?;
        Ok(stat_of(&vfs.stat(self.node)?))
    }

    /// Fills `buf` with the directory's next entries (spec §7.3), sorted by
    /// name, as `relay_abi::file` records: the bytes written, 0 after the
    /// last. `EINVAL` if not even the next one fits; `ENOMEM` if the
    /// directory is too big for the `room` bytes of heap there are.
    pub fn read_dir(&self, vfs: &mut dyn Vfs, buf: &mut [u8], room: usize) -> Result<usize, Errno> {
        self.offset()?;
        if !self.dir {
            return Err(Errno::ENOTDIR);
        }
        let size = vfs.stat(self.node)?.size;
        if size.saturating_mul(DIR_MEMORY_FACTOR) > room as u64 {
            return Err(Errno::ENOMEM);
        }
        let mut entries = vfs.read_dir(self.node)?;
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let after = self.state.lock().after.clone();
        let start = match &after {
            Some(last) => entries.partition_point(|e| e.name <= *last),
            None => 0,
        };
        let mut done = 0;
        let mut last = None;
        for e in &entries[start..] {
            let k = vfs.entry_kind(self.node, e).map_or(KIND_UNKNOWN, kind);
            match put_dir_entry(&mut buf[done..], e.ino, k, &e.name) {
                Some(n) => {
                    done += n;
                    last = Some(&e.name);
                }
                None if done == 0 => return Err(Errno::EINVAL),
                None => break,
            }
        }
        if let Some(name) = last {
            self.state.lock().after = Some(name.clone());
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use relay_abi::file::{DirEntry, dir_entries};
    use vfs::{Env, FileSystem, MemFs, MountTable};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            1_000
        }
        fn log(&self, _: &str) {}
    }

    const RW: u32 = OPEN_READ | OPEN_WRITE;

    /// `/root` with `/root/f` holding "hello", a symlink `/root/l`, and a
    /// read-only filesystem at `/bin` holding `prog`.
    fn table() -> MountTable {
        table_on(MemFs::new(Box::new(Clock)))
    }

    fn table_on(fs: MemFs) -> MountTable {
        let mut fs = fs;
        let root = fs.root();
        let home = fs.mkdir(root, b"root").unwrap();
        let f = fs.create(home, b"f").unwrap();
        fs.write_at(f, 0, b"hello").unwrap();
        fs.symlink(home, b"l", b"f").unwrap();
        let mut bin = MemFs::new(Box::new(Clock));
        let r = bin.root();
        bin.create(r, b"prog").unwrap();
        let mut t = MountTable::new(Box::new(fs));
        t.mount(b"/bin", Box::new(bin.read_only())).unwrap();
        t
    }

    fn read_all(t: &mut MountTable, f: &OpenFile) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0; 3];
        loop {
            assert!(out.len() < 1 << 20, "a read that never ends");
            match f.read(t, &mut buf).unwrap() {
                0 => return out,
                n => out.extend_from_slice(&buf[..n]),
            }
        }
    }

    #[test]
    fn open_s_flags_are_checked() {
        let mut t = table();
        for bad in [
            0,
            OPEN_CREATE,
            OPEN_READ | OPEN_TRUNCATE,
            OPEN_READ | OPEN_APPEND,
            OPEN_READ | OPEN_EXCLUSIVE,
            RW | OPEN_CREATE | OPEN_DIRECTORY,
            OPEN_READ | 128,
            u32::MAX,
        ] {
            assert_eq!(
                open(&mut t, b"/root/f", bad).err(),
                Some(Errno::EINVAL),
                "{bad:#x}"
            );
        }
        assert_eq!(
            open(&mut t, b"/root/nope", OPEN_READ).err(),
            Some(Errno::ENOENT)
        );
        assert_eq!(
            open(&mut t, b"/root/f", RW | OPEN_CREATE | OPEN_EXCLUSIVE).err(),
            Some(Errno::EEXIST)
        );
        assert_eq!(
            open(&mut t, b"/root", OPEN_WRITE).err(),
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f", OPEN_READ | OPEN_DIRECTORY).err(),
            Some(Errno::ENOTDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f/", OPEN_READ).err(),
            Some(Errno::ENOTDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/new/", OPEN_WRITE | OPEN_CREATE).err(),
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/bin/new", OPEN_WRITE | OPEN_CREATE).err(),
            Some(Errno::EROFS)
        );
        assert_eq!(
            open(&mut t, b"/bin/prog", OPEN_WRITE | OPEN_TRUNCATE).err(),
            Some(Errno::EROFS)
        );
        // What may be opened.
        let d = open(&mut t, b"/root", OPEN_READ | OPEN_DIRECTORY).unwrap();
        assert!(d.is_dir() && d.is_readable() && !d.is_writable());
        assert!(open(&mut t, b"/root", OPEN_READ).unwrap().is_dir());
        assert!(open(&mut t, b"/bin/prog", OPEN_READ).is_ok());
        let f = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert!(!f.is_readable() && f.is_writable() && !f.is_dir());
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(read_all(&mut t, &r), b"hello");
    }

    #[test]
    fn create_makes_a_file_and_truncate_empties_one() {
        let mut t = table();
        let f = open(
            &mut t,
            b"/root/new",
            OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
        )
        .unwrap();
        assert_eq!(f.write(&mut t, b"abc"), Ok(3));
        let node = t.lookup(b"/root/new").unwrap();
        assert_eq!(f.node(), node);
        let g = open(&mut t, b"/root/new", RW | OPEN_TRUNCATE).unwrap();
        assert_eq!(read_all(&mut t, &g), b"");
        assert_eq!(t.stat(node).unwrap().size, 0);
        // Without TRUNCATE it stays.
        f.write(&mut t, b"xyz").unwrap();
        let h = open(&mut t, b"/root/new", RW | OPEN_CREATE).unwrap();
        assert_eq!(read_all(&mut t, &h), b"\0\0\0xyz", "f's offset was 3");
    }

    #[test]
    fn reads_and_writes_move_the_offset_that_sharers_share() {
        let mut t = table();
        let f = alloc::sync::Arc::new(open(&mut t, b"/root/f", RW).unwrap());
        let shared = alloc::sync::Arc::clone(&f);
        let mut buf = [0; 2];
        assert_eq!(f.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"he");
        assert_eq!(shared.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"ll", "the same offset");
        // Another open has its own.
        let other = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(other.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"he");
        assert_eq!(f.write(&mut t, b"O!"), Ok(2));
        assert_eq!(f.read(&mut t, &mut buf), Ok(0), "at the end");
        assert_eq!(read_all(&mut t, &other), b"llO!");
        assert_ne!(*f, other);
        assert_eq!(*f, *shared);
    }

    #[test]
    fn append_writes_at_the_end_whatever_the_offset() {
        let mut t = table();
        let a = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_APPEND).unwrap();
        let w = open(&mut t, b"/root/f", OPEN_WRITE).unwrap();
        a.write(&mut t, b" world").unwrap();
        w.write(&mut t, b"J").unwrap();
        a.seek(&mut t, 0, SEEK_START).unwrap();
        a.write(&mut t, b"!").unwrap();
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(read_all(&mut t, &r), b"Jello world!");
    }

    #[test]
    fn seek_goes_anywhere_from_the_start_on() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", RW).unwrap();
        assert_eq!(f.seek(&mut t, 1, SEEK_START), Ok(1));
        assert_eq!(f.seek(&mut t, 2, SEEK_CURRENT), Ok(3));
        assert_eq!(f.seek(&mut t, -1, SEEK_CURRENT), Ok(2));
        assert_eq!(f.seek(&mut t, -2, SEEK_END), Ok(3));
        let mut buf = [0; 8];
        assert_eq!(f.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf[..2], b"lo");
        // Past the end: reads nothing, and a write leaves a hole.
        assert_eq!(f.seek(&mut t, 3, SEEK_END), Ok(8));
        assert_eq!(f.read(&mut t, &mut buf), Ok(0));
        f.write(&mut t, b"!").unwrap();
        f.seek(&mut t, 0, SEEK_START).unwrap();
        assert_eq!(read_all(&mut t, &f), b"hello\0\0\0!");
        for (off, whence) in [(-1, SEEK_START), (-10, SEEK_END), (0, 3), (1, u32::MAX)] {
            assert_eq!(
                f.seek(&mut t, off, whence),
                Err(Errno::EINVAL),
                "{off} {whence}"
            );
        }
        f.seek(&mut t, i64::MAX, SEEK_START).unwrap();
        assert_eq!(
            f.seek(&mut t, 1, SEEK_CURRENT),
            Err(Errno::EINVAL),
            "past 2^63 - 1"
        );
        assert_eq!(
            f.seek(&mut t, 0, SEEK_CURRENT),
            Ok(i64::MAX as u64),
            "unchanged"
        );
        assert_eq!(f.write(&mut t, b"x"), Err(Errno::EFBIG));
    }

    #[test]
    fn what_a_file_was_not_opened_for_is_ebadf() {
        let mut t = table();
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(r.write(&mut t, b"x"), Err(Errno::EBADF));
        let w = open(&mut t, b"/root/f", OPEN_WRITE).unwrap();
        assert_eq!(w.read(&mut t, &mut [0; 4]), Err(Errno::EBADF));
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(d.read(&mut t, &mut [0; 4]), Err(Errno::EISDIR));
        assert_eq!(
            r.read_dir(&mut t, &mut [0; 64], usize::MAX),
            Err(Errno::ENOTDIR)
        );
        // A symbolic link is never followed: reading one is the
        // filesystem's EINVAL.
        let l = open(&mut t, b"/root/l", OPEN_READ).unwrap();
        assert_eq!(l.read(&mut t, &mut [0; 4]), Err(Errno::EINVAL));
    }

    #[test]
    fn a_full_filesystem_gives_a_short_write_then_enospc() {
        let mut t = table_on(MemFs::new(Box::new(Clock)).with_capacity(8192));
        let f = open(&mut t, b"/root/big", OPEN_WRITE | OPEN_CREATE).unwrap();
        let n = f.write(&mut t, &[b'x'; 10_000]).unwrap();
        assert!(n < 10_000, "{n}");
        assert_eq!(f.write(&mut t, &[b'x'; 10_000]), Err(Errno::ENOSPC));
        assert_eq!(f.write_all(&mut t, b"y"), Err(Errno::ENOSPC));
        assert_eq!(
            f.seek(&mut t, 0, SEEK_CURRENT),
            Ok(n as u64),
            "at what was written"
        );
    }

    #[test]
    fn stat_maps_every_field() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        let s = t.stat(f.node()).unwrap();
        let got = f.stat(&mut t).unwrap();
        assert_eq!(
            got,
            relay_abi::Stat {
                ino: s.ino,
                size: 5,
                blocks: s.blocks,
                atime: 1_000,
                mtime: 1_000,
                ctime: 1_000,
                kind: u32::from(KIND_REGULAR),
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                block_size: s.block_size,
            }
        );
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(d.stat(&mut t).unwrap().kind, u32::from(KIND_DIRECTORY));
        assert_eq!(kind(FileType::Symlink), KIND_SYMLINK);
        assert_eq!(kind(FileType::Fifo), KIND_FIFO);
    }

    /// The names and kinds `read_dir` gives with a buffer of `size` bytes,
    /// call by call. A `read_dir` that never gets to the end fails the test
    /// instead of running for ever.
    fn listing(t: &mut MountTable, d: &OpenFile, size: usize) -> Vec<Vec<(Vec<u8>, u8)>> {
        let mut calls = Vec::new();
        let mut buf = vec![0; size];
        loop {
            assert!(calls.len() < 100, "read_dir never ends: {:?}", &calls[..3]);
            match d.read_dir(t, &mut buf, usize::MAX).unwrap() {
                0 => return calls,
                n => calls.push(
                    dir_entries(&buf[..n])
                        .map(|r| (r.name.to_vec(), r.kind))
                        .collect(),
                ),
            }
        }
    }

    #[test]
    fn read_dir_gives_sorted_records_and_goes_on_where_it_stopped() {
        let mut t = table();
        let root = open(&mut t, b"/", OPEN_READ | OPEN_DIRECTORY).unwrap();
        let calls = listing(&mut t, &root, 4096);
        assert_eq!(
            calls,
            [vec![
                (b".".to_vec(), KIND_DIRECTORY),
                (b"..".to_vec(), KIND_DIRECTORY),
                (b"bin".to_vec(), KIND_DIRECTORY),
                (b"root".to_vec(), KIND_DIRECTORY),
            ]]
        );
        let home = open(&mut t, b"/root", OPEN_READ).unwrap();
        // Room for one record at a time.
        let one = DirEntry::record_len(2);
        let calls = listing(&mut t, &home, one);
        assert_eq!(
            calls,
            [
                vec![(b".".to_vec(), KIND_DIRECTORY)],
                vec![(b"..".to_vec(), KIND_DIRECTORY)],
                vec![(b"f".to_vec(), KIND_REGULAR)],
                vec![(b"l".to_vec(), KIND_SYMLINK)],
            ]
        );
        // At the end until it goes back to the start.
        assert_eq!(home.read_dir(&mut t, &mut [0; 64], usize::MAX), Ok(0));
        assert_eq!(home.seek(&mut t, 0, SEEK_START), Ok(0));
        assert_eq!(listing(&mut t, &home, 4096)[0].len(), 4);
        for (off, whence) in [(1, SEEK_START), (0, SEEK_END), (0, SEEK_CURRENT)] {
            assert_eq!(home.seek(&mut t, off, whence), Err(Errno::EINVAL));
        }
    }

    #[test]
    fn an_entry_there_throughout_comes_once_however_the_directory_changes() {
        let mut t = table();
        for name in ["a", "c", "e", "g"] {
            t.create(&[b"/root/", name.as_bytes()].concat()).unwrap();
        }
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        let mut buf = vec![0; 2 * DirEntry::record_len(1)];
        let mut names = Vec::new();
        let mut take = |t: &mut MountTable| {
            let n = d.read_dir(t, &mut buf, usize::MAX).unwrap();
            let got: Vec<Vec<u8>> = dir_entries(&buf[..n]).map(|r| r.name.to_vec()).collect();
            names.extend(got.clone());
            got
        };
        assert_eq!(take(&mut t), [b".".to_vec(), b"..".to_vec()]);
        assert_eq!(take(&mut t), [b"a".to_vec(), b"c".to_vec()]);
        // Removing what was given and adding before and after the place it
        // stopped.
        t.unlink(b"/root/a").unwrap();
        t.create(b"/root/b").unwrap();
        t.create(b"/root/d").unwrap();
        assert_eq!(take(&mut t), [b"d".to_vec(), b"e".to_vec()]);
        assert_eq!(take(&mut t), [b"f".to_vec(), b"g".to_vec()]);
        assert_eq!(take(&mut t), [b"l".to_vec()]);
        assert!(take(&mut t).is_empty());
        for n in ["c", "e", "f", "g", "l"] {
            assert_eq!(
                names.iter().filter(|x| *x == n.as_bytes()).count(),
                1,
                "{n}"
            );
        }
    }

    #[test]
    fn read_dir_needs_room_for_one_record_and_the_heap_for_the_directory() {
        let mut t = table();
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(
            d.read_dir(&mut t, &mut [0; DirEntry::SIZE], usize::MAX),
            Err(Errno::EINVAL),
            "not even `.` fits"
        );
        let size = t.stat(d.node()).unwrap().size;
        assert!(size > 0);
        let need = (size * DIR_MEMORY_FACTOR) as usize;
        assert_eq!(
            d.read_dir(&mut t, &mut [0; 64], need - 1),
            Err(Errno::ENOMEM)
        );
        assert!(d.read_dir(&mut t, &mut [0; 64], need).unwrap() > 0);
    }

    #[test]
    fn a_file_marked_gone_is_enoent_for_everything() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", RW).unwrap();
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        f.mark_gone();
        d.mark_gone();
        assert_eq!(f.read(&mut t, &mut [0; 4]), Err(Errno::ENOENT));
        assert_eq!(f.write(&mut t, b"x"), Err(Errno::ENOENT));
        assert_eq!(f.seek(&mut t, 0, SEEK_START), Err(Errno::ENOENT));
        assert_eq!(f.stat(&mut t).err(), Some(Errno::ENOENT));
        assert_eq!(
            d.read_dir(&mut t, &mut [0; 64], usize::MAX),
            Err(Errno::ENOENT)
        );
        assert_eq!(d.seek(&mut t, 0, SEEK_START), Err(Errno::ENOENT));
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(
            read_all(&mut t, &r),
            b"hello",
            "the file itself is untouched"
        );
    }
}
