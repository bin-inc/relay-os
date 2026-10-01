//! `SysVfs`: the shell's `vfs::Vfs` over the file calls (user-space gate
//! §8.1), so a command function runs unchanged in a program.
//!
//! The `Vfs` names files by `Node`, the program's calls by path or fd.
//! `SysVfs` goes by path: a `Node` is the file's filesystem and inode, as
//! `stat` gives them (`Stat::dev`, `Stat::ino`), so the same file is always
//! the same node (`cp a a`, `rm -r /`) and files of two filesystems never
//! are; it remembers the path each node was found by, and every operation
//! on a node opens that path, works and closes it. A command then holds
//! no fd between calls, and always reaches the file the path names now.

use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_START,
    STAT_NOFOLLOW, dir_entries,
};
use vfs::{DirEntry, Errno, FileType, Node, Stat, StatFs, Vfs};

/// The longest path `getcwd` gives (the kernel's limit, spec §16 item 3).
const PATH_MAX: usize = 4096;
/// `read_dir` is asked for this much at a time.
const DIR_BUFFER: usize = 16 * 1024;
/// A directory with more entries than this is refused (`EIO`), so a
/// kernel that never says it is done cannot make a listing grow without
/// end.
const DIR_ENTRIES_MAX: usize = 1 << 18;

/// The calls `SysVfs` makes: the program's (`Sys`), or a fake in tests.
pub trait Calls {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno>;
    fn close(&self, fd: u32);
    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno>;
    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno>;
    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno>;
    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno>;
    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno>;
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno>;
    fn mkdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn rmdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn unlink(&self, path: &[u8]) -> Result<(), Errno>;
    fn touch(&self, path: &[u8]) -> Result<(), Errno>;
    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno>;
    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno>;
    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno>;
    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno>;
    fn sync(&self) -> Result<(), Errno>;
    fn chdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno>;
}

/// The program's own calls.
pub struct Sys;

fn errno(e: u16) -> Errno {
    Errno::from_number(e)
}

impl Calls for Sys {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno> {
        crate::sys::open(path, flags).map_err(errno)
    }
    fn close(&self, fd: u32) {
        let _ = crate::sys::close(fd);
    }
    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::read(fd, buf).map_err(errno)
    }
    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno> {
        crate::sys::write(fd, bytes).map_err(errno)
    }
    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno> {
        let offset = i64::try_from(offset).map_err(|_| Errno::EINVAL)?;
        crate::sys::seek(fd, offset, SEEK_START)
            .map(|_| ())
            .map_err(errno)
    }
    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno> {
        crate::sys::fstat(fd).map_err(errno)
    }
    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno> {
        crate::sys::stat(path, STAT_NOFOLLOW).map_err(errno)
    }
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::read_dir(fd, buf).map_err(errno)
    }
    fn mkdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::mkdir(path).map_err(errno)
    }
    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::rmdir(path).map_err(errno)
    }
    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::unlink(path).map_err(errno)
    }
    fn touch(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::touch(path).map_err(errno)
    }
    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno> {
        crate::sys::truncate(path, size).map_err(errno)
    }
    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::readlink(path, buf).map_err(errno)
    }
    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        crate::sys::rename(from, to).map_err(errno)
    }
    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno> {
        crate::sys::statfs(path).map_err(errno)
    }
    fn sync(&self) -> Result<(), Errno> {
        crate::sys::sync().map_err(errno)
    }
    fn chdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::chdir(path).map_err(errno)
    }
    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::getcwd(buf).map_err(errno)
    }
}

/// The node `stat` describes: its filesystem and inode.
pub fn node_of(st: &relay_abi::Stat) -> Node {
    Node {
        mount: st.dev as usize,
        ino: st.ino,
    }
}

fn file_type(kind: u32) -> FileType {
    match u8::try_from(kind) {
        Ok(KIND_REGULAR) => FileType::Regular,
        Ok(KIND_DIRECTORY) => FileType::Directory,
        Ok(KIND_SYMLINK) => FileType::Symlink,
        Ok(KIND_CHAR_DEVICE) => FileType::CharDev,
        Ok(KIND_BLOCK_DEVICE) => FileType::BlockDev,
        Ok(KIND_FIFO) => FileType::Fifo,
        Ok(KIND_SOCKET) => FileType::Socket,
        // The kernel names only the kinds above.
        _ => FileType::Regular,
    }
}

/// A program's `Stat` as the `Vfs` gives it.
fn stat_from(st: &relay_abi::Stat) -> Stat {
    Stat {
        ino: st.ino,
        kind: file_type(st.kind),
        perm: (st.perm & 0o7777) as u16,
        nlink: st.nlink,
        uid: st.uid,
        gid: st.gid,
        size: st.size,
        blocks: st.blocks,
        block_size: st.block_size,
        atime: st.atime,
        mtime: st.mtime,
        ctime: st.ctime,
    }
}

/// The shell's `Vfs` over `C`'s calls.
pub struct SysVfs<C: Calls = Sys> {
    calls: C,
    /// Every node found so far and the path it was found by.
    paths: RefCell<Vec<(Node, Vec<u8>)>>,
    /// The kinds of the entries of the directory listed last, from its
    /// records (`entry_kind`).
    kinds: Vec<(Node, Vec<u8>, FileType)>,
}

impl SysVfs<Sys> {
    pub fn new() -> SysVfs<Sys> {
        SysVfs::with(Sys)
    }
}

impl Default for SysVfs<Sys> {
    fn default() -> SysVfs<Sys> {
        SysVfs::new()
    }
}

impl<C: Calls> SysVfs<C> {
    pub fn with(calls: C) -> SysVfs<C> {
        SysVfs {
            calls,
            paths: RefCell::new(Vec::new()),
            kinds: Vec::new(),
        }
    }

    pub fn calls(&self) -> &C {
        &self.calls
    }

    /// Remembers that `node` is at `path`, the latest path it was found by.
    fn found(&self, node: Node, path: &[u8]) {
        let mut paths = self.paths.borrow_mut();
        match paths.iter_mut().find(|(n, _)| *n == node) {
            Some((_, p)) => {
                p.clear();
                p.extend_from_slice(path);
            }
            None => paths.push((node, path.to_vec())),
        }
    }

    /// The path `node` was found by; `ENOENT` for a node this `SysVfs`
    /// never gave.
    fn path(&self, node: Node) -> Result<Vec<u8>, Errno> {
        let paths = self.paths.borrow();
        paths
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, p)| p.clone())
            .ok_or(Errno::ENOENT)
    }

    /// Runs `f` on the file at `node`'s path, opened with `flags`.
    fn with_fd<R>(
        &self,
        node: Node,
        flags: u32,
        f: impl FnOnce(&C, u32) -> Result<R, Errno>,
    ) -> Result<R, Errno> {
        let fd = self.calls.open(&self.path(node)?, flags)?;
        let result = f(&self.calls, fd);
        self.calls.close(fd);
        result
    }
}

impl<C: Calls> Vfs for SysVfs<C> {
    fn cwd(&self) -> Vec<u8> {
        let mut buf = vec![0; PATH_MAX];
        match self.calls.getcwd(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                buf
            }
            Err(_) => b"/".to_vec(),
        }
    }

    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.chdir(path)
    }

    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno> {
        let node = node_of(&self.calls.stat(path)?);
        self.found(node, path);
        Ok(node)
    }

    fn stat(&mut self, node: Node) -> Result<Stat, Errno> {
        Ok(stat_from(&self.calls.stat(&self.path(node)?)?))
    }

    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        let mut entries = Vec::new();
        let mut kinds = Vec::new();
        self.with_fd(node, OPEN_READ | OPEN_DIRECTORY, |calls, fd| {
            let mut buf = vec![0; DIR_BUFFER];
            loop {
                let n = calls.read_dir(fd, &mut buf)?;
                if n == 0 {
                    return Ok(());
                }
                let before = entries.len();
                for r in dir_entries(&buf[..n]) {
                    entries.push(DirEntry {
                        name: r.name.to_vec(),
                        ino: r.ino,
                    });
                    kinds.push((node, r.name.to_vec(), file_type(u32::from(r.kind))));
                }
                // A call that gave bytes but no record, or a directory that
                // never ends, is the kernel's fault; stop rather than spin.
                if entries.len() == before || entries.len() > DIR_ENTRIES_MAX {
                    return Err(Errno::EIO);
                }
            }
        })?;
        self.kinds = kinds;
        Ok(entries)
    }

    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno> {
        if let Some((_, _, kind)) = self
            .kinds
            .iter()
            .find(|(d, name, _)| *d == dir && *name == entry.name)
        {
            return Ok(*kind);
        }
        let mut path = self.path(dir)?;
        path.push(b'/');
        path.extend_from_slice(&entry.name);
        Ok(file_type(self.calls.stat(&path)?.kind))
    }

    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
        let mut buf = vec![0; PATH_MAX];
        let n = self.calls.readlink(&self.path(node)?, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.with_fd(node, OPEN_READ, |calls, fd| {
            calls.seek(fd, offset)?;
            calls.read(fd, buf)
        })
    }

    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.with_fd(node, OPEN_WRITE, |calls, fd| {
            calls.seek(fd, offset)?;
            calls.write(fd, buf)
        })
    }

    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno> {
        self.calls.truncate(&self.path(node)?, size)
    }

    fn touch(&mut self, node: Node) -> Result<(), Errno> {
        self.calls.touch(&self.path(node)?)
    }

    fn create(&mut self, path: &[u8]) -> Result<Node, Errno> {
        let flags = OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE;
        let fd = self.calls.open(path, flags)?;
        let st = self.calls.fstat(fd);
        self.calls.close(fd);
        let node = node_of(&st?);
        self.found(node, path);
        Ok(node)
    }

    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.mkdir(path)
    }

    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.unlink(path)
    }

    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.rmdir(path)
    }

    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.calls.rename(from, to)
    }

    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno> {
        let s = self.calls.statfs(path)?;
        Ok(StatFs {
            block_size: s.block_size,
            blocks: s.blocks,
            free_blocks: s.free_blocks,
            avail_blocks: s.avail_blocks,
            files: s.files,
            free_files: s.free_files,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        self.calls.sync()
    }

    /// The kernel's `power` shuts the filesystems down (spec §7.3); a
    /// program has nothing to do first.
    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeCalls, memfs};
    use alloc::boxed::Box;
    use alloc::string::String;
    use shell::{Console, MemInfo, Stdout, System};
    use vfs::{FileSystem, MountTable};

    /// `/root/a`, `/root/sub/`, `/tmp/`, and at `/bin` a filesystem of its
    /// own whose `x` has the same inode number as `/root/a`.
    fn tree() -> MountTable {
        let mut t = MountTable::new(Box::new(memfs()));
        for d in ["/root", "/root/sub", "/tmp", "/bin"] {
            t.mkdir(d.as_bytes()).unwrap();
        }
        let a = t.create(b"/root/a").unwrap();
        t.write_at(a, 0, b"one\ntwo\nthree\n").unwrap();
        // A fresh MemFs numbers its files as the root's first ones were.
        let mut bin = memfs();
        let root = bin.root();
        for d in ["r", "s", "t", "b"] {
            bin.mkdir(root, d.as_bytes()).unwrap();
        }
        let x = bin.create(root, b"x").unwrap();
        bin.write_at(x, 0, b"program\n").unwrap();
        assert_eq!(x, a.ino, "the same inode number on both");
        t.mount(b"/bin", Box::new(bin)).unwrap();
        t
    }

    fn sysvfs() -> SysVfs<FakeCalls> {
        SysVfs::with(FakeCalls::new(tree()))
    }

    #[derive(Default)]
    struct Screen(Vec<u8>);

    impl Console for Screen {
        fn read_byte(&mut self) -> Option<u8> {
            None
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        fn columns(&self) -> usize {
            80
        }
    }

    impl Stdout for Screen {
        fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
        fn is_tty(&self) -> bool {
            true
        }
        fn node(&self) -> Option<Node> {
            None
        }
    }

    struct Clock;

    impl System for Clock {
        fn now(&self) -> u64 {
            crate::testing::NOW
        }
        fn memory(&self) -> Option<MemInfo> {
            None
        }
        fn kernel_log(&self) -> Vec<u8> {
            Vec::new()
        }
        fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
            None
        }
        fn sleep(&mut self, _: u64) {}
        fn reboot(&mut self, _: bool) -> Result<(), Errno> {
            Ok(())
        }
        fn poweroff(&mut self, _: bool) -> Result<(), Errno> {
            Ok(())
        }
    }

    /// Runs `line` as its program does over `vfs`: the status, the errors
    /// and the output.
    fn run(vfs: &mut dyn Vfs, line: &str) -> (i32, String, String) {
        let words = shell::parser::parse(line).unwrap().remove(0).words;
        let (mut errors, mut out) = (Screen::default(), Screen::default());
        let run = shell::commands::find(&words[0]).unwrap().run;
        let io = shell::CommandIo {
            vfs,
            console: &mut errors,
            system: &mut Clock,
            stdin: &mut shell::Bytes::new(Vec::new()),
            stdout: &mut out,
        };
        let status = shell::run_command(&words[0], run, &words[1..], io);
        let text = |s: Screen| String::from_utf8(s.0).unwrap();
        (status, text(errors), text(out))
    }

    #[test]
    fn commands_do_through_the_calls_what_they_do_over_the_mount_table() {
        let mut direct = tree();
        let mut sys = sysvfs();
        for line in [
            "ls -la /root",
            "ls -l /",
            "ls /bin",
            "cat /root/a /bin/x /nope",
            "wc /root/a",
            "head -n 1 /root/a",
            "tail -n 2 /root/a",
            "stat /root/a /bin",
            "df",
            "mkdir /root/d /root/d",
            "touch /root/d/x /root/a",
            "cp /root/a /root/b",
            "cp /root/a /root/a",
            "cp /bin/x /root/a",
            "cat /root/a",
            "mv /root/b /root/d/",
            "ls -l /root/d",
            "rm /root/sub",
            "rm -r /root/d /nope",
            "rm -r /",
            "rmdir /tmp /root/sub",
            "rm /bin/x",
            "cat /root/sub",
            "ls /root",
        ] {
            assert_eq!(run(&mut sys, line), run(&mut direct, line), "{line}");
        }
    }

    #[test]
    fn a_file_is_one_node_whatever_its_path() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        assert_eq!(v.lookup(b"/root/../root/sub/../a").unwrap(), a);
        v.chdir(b"/root").unwrap();
        assert_eq!(v.lookup(b"a").unwrap(), a);
        assert_eq!(v.cwd(), b"/root");
    }

    #[test]
    fn files_of_two_filesystems_are_never_one_node() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        let x = v.lookup(b"/bin/x").unwrap();
        assert_eq!(a.ino, x.ino);
        assert_ne!(a, x);
        assert_eq!(run(&mut v, "cp /bin/x /root/a").0, 0, "not the same file");
        assert_eq!(run(&mut v, "cat /root/a").2, "program\n");
    }

    #[test]
    fn a_command_holds_no_fd_between_calls() {
        let mut v = sysvfs();
        for line in [
            "cat /root/a",
            "cp /root/a /tmp/c",
            "ls -l /root",
            "rm -r /tmp",
        ] {
            run(&mut v, line);
            assert_eq!(v.calls().open_now.get(), 0, "{line}");
        }
        assert_eq!(v.calls().open_most.get(), 1);
    }

    #[test]
    fn a_big_directory_is_listed_over_many_calls() {
        let mut v = sysvfs();
        for i in 0..300 {
            v.create(alloc::format!("/tmp/f{i:03}").as_bytes()).unwrap();
        }
        v.mkdir(b"/tmp/zdir").unwrap();
        v.calls().dir_chunk.set(200);
        let tmp = v.lookup(b"/tmp").unwrap();
        let entries = v.read_dir(tmp).unwrap();
        assert_eq!(entries.len(), 303, ". and .. too");
        let reads = v
            .calls()
            .calls
            .borrow()
            .iter()
            .filter(|c| **c == "read_dir")
            .count();
        assert!(reads > 30, "{reads} calls");
        let zdir = entries.iter().find(|e| e.name == b"zdir").unwrap();
        assert_eq!(v.entry_kind(tmp, zdir).unwrap(), FileType::Directory);
        let f = entries.iter().find(|e| e.name == b"f123").unwrap();
        assert_eq!(v.entry_kind(tmp, f).unwrap(), FileType::Regular);
        // A mount point is a directory in its parent.
        let root = v.lookup(b"/").unwrap();
        let bin = v
            .read_dir(root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == b"bin")
            .unwrap();
        assert_eq!(v.entry_kind(root, &bin).unwrap(), FileType::Directory);
    }

    #[test]
    fn a_directory_that_never_ends_is_eio() {
        let mut v = sysvfs();
        v.calls().dir_repeats.set(true);
        let root = v.lookup(b"/root").unwrap();
        assert_eq!(v.read_dir(root), Err(Errno::EIO));
        assert_eq!(v.calls().open_now.get(), 0);
    }

    #[test]
    fn reads_and_writes_go_by_offset() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        let mut buf = [0; 3];
        assert_eq!(v.read_at(a, 4, &mut buf), Ok(3));
        assert_eq!(&buf, b"two");
        assert_eq!(v.write_at(a, 4, b"TWO"), Ok(3));
        assert_eq!(v.read_at(a, 0, &mut [0; 64]), Ok(14));
        assert_eq!(run(&mut v, "cat /root/a").2, "one\nTWO\nthree\n");
    }

    #[test]
    fn errors_are_the_calls_own() {
        let mut v = sysvfs();
        assert_eq!(v.lookup(b"/nope"), Err(Errno::ENOENT));
        assert_eq!(v.lookup(b"/root/a/b"), Err(Errno::ENOTDIR));
        assert_eq!(v.create(b"/root/a"), Err(Errno::EEXIST));
        let a = v.lookup(b"/root/a").unwrap();
        assert_eq!(v.read_dir(a), Err(Errno::ENOTDIR));
        let sub = v.lookup(b"/root/sub").unwrap();
        assert_eq!(v.write_at(sub, 0, b"x"), Err(Errno::EISDIR));
        // A node this SysVfs never gave names no file.
        let stranger = Node { mount: 9, ino: 9 };
        assert_eq!(v.stat(stranger), Err(Errno::ENOENT));
    }

    #[test]
    fn the_kernel_shuts_the_filesystems_down() {
        let mut v = sysvfs();
        assert_eq!(v.shutdown(), Ok(()));
        assert!(v.calls().calls.borrow().is_empty(), "no call");
    }
}
