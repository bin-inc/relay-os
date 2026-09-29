//! The mount table and path resolution (spec §8.1): the [`Vfs`] the kernel
//! gives the shell.
//!
//! A path is resolved one name at a time from `/` or from the current
//! directory. `..` goes back along the names walked so far (at `/` it stays
//! at `/`), and every step checks the mount table, so another filesystem
//! can be mounted on a directory later without changing any caller.
//! Symbolic links are never followed (milestone 1): one used as a directory
//! is `ENOTDIR`, and reading or writing one is `EINVAL`.

use crate::Errno;
use crate::fs::{DirEntry, FileSystem, FileType, Ino, Stat, StatFs};
use crate::path::{self, Component};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// An inode in a mounted filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// Index in the mount table.
    pub mount: usize,
    pub ino: Ino,
}

/// File operations by path, relative paths starting at the current
/// directory. This is all the shell knows about files (spec §7.3).
pub trait Vfs {
    /// The current directory as an absolute path, e.g. `/root`.
    fn cwd(&self) -> Vec<u8>;
    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno>;
    /// Resolves a path to its inode. A symbolic link at the end is not
    /// followed.
    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno>;
    fn stat(&mut self, node: Node) -> Result<Stat, Errno>;
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno>;
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno>;
    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno>;
    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno>;
    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno>;
    fn touch(&mut self, node: Node) -> Result<(), Errno>;
    /// Creates an empty regular file; `EEXIST` if the path exists.
    fn create(&mut self, path: &[u8]) -> Result<Node, Errno>;
    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno>;
    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno>;
    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno>;
    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno>;
    /// The filesystem holding `path`.
    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno>;
    /// Syncs every filesystem.
    fn sync(&mut self) -> Result<(), Errno>;
    /// Shuts every filesystem down cleanly (before a reboot or power-off).
    fn shutdown(&mut self) -> Result<(), Errno>;
}

struct Mount {
    fs: Box<dyn FileSystem>,
    /// Where it is mounted; `None` for `/`.
    on: Option<MountPoint>,
}

/// A directory of the filesystem below, or a name that directory does not
/// have (spec §4.4 of the user-space gate: `/bin` on a root without one).
/// A mount on a name is shown in its directory like any other, and the
/// name behaves as a mount point does: it cannot be created, removed or
/// renamed through the mount table, the only way the shell reaches files.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MountPoint {
    Dir(Node),
    Name(Node, Vec<u8>),
}

/// The names walked to reach a directory: `(node, name)` pairs from `/`
/// (whose name is empty) to the directory itself.
type Trail = Vec<(Node, Vec<u8>)>;

/// The mount table plus the current directory.
pub struct MountTable {
    mounts: Vec<Mount>,
    cwd: Trail,
    /// The current directory was removed: relative paths resolve to
    /// nothing (as in Linux) until the next `chdir`. Its inode number may
    /// be reused, so the trail must not be followed any more.
    cwd_gone: bool,
}

/// A path split into the directory holding its last name and that name.
struct Parent {
    dir: Node,
    /// `None` when the path ends in `.` or `..` or is `/`.
    name: Option<Vec<u8>>,
    trailing_slash: bool,
}

impl MountTable {
    /// A table with `root` mounted at `/`, which is also the current
    /// directory.
    pub fn new(root: Box<dyn FileSystem>) -> MountTable {
        let node = Node {
            mount: 0,
            ino: root.root(),
        };
        MountTable {
            mounts: alloc::vec![Mount { fs: root, on: None }],
            cwd: alloc::vec![(node, Vec::new())],
            cwd_gone: false,
        }
    }

    /// Mounts `fs` on the directory at `path`, or on its last name if the
    /// directory holding it has no such name. `EBUSY` if something is
    /// mounted there already.
    pub fn mount(&mut self, path: &[u8], fs: Box<dyn FileSystem>) -> Result<(), Errno> {
        let on = match self.walk(path) {
            Ok(trail) => {
                let on = trail.last().expect("never empty").0;
                if self.stat(on)?.kind != FileType::Directory {
                    return Err(Errno::ENOTDIR);
                }
                if on.ino == self.fs(on.mount)?.root() || self.mounted_on(on).is_some() {
                    return Err(Errno::EBUSY);
                }
                MountPoint::Dir(on)
            }
            Err(Errno::ENOENT) => {
                // Only the last name may be missing.
                let parent = self.parent(path)?;
                let name = parent.name.ok_or(Errno::ENOENT)?;
                MountPoint::Name(parent.dir, name)
            }
            Err(e) => return Err(e),
        };
        self.mounts.push(Mount { fs, on: Some(on) });
        Ok(())
    }

    fn fs(&mut self, mount: usize) -> Result<&mut dyn FileSystem, Errno> {
        match self.mounts.get_mut(mount) {
            Some(m) => Ok(m.fs.as_mut()),
            None => Err(Errno::ENOENT),
        }
    }

    fn mounted_on(&self, node: Node) -> Option<usize> {
        self.mounts
            .iter()
            .position(|m| m.on == Some(MountPoint::Dir(node)))
    }

    /// The mount on the name `name` in `dir`, which `dir` does not have.
    fn mounted_at_name(&self, dir: Node, name: &[u8]) -> Option<usize> {
        self.mounts.iter().position(|m| match &m.on {
            Some(MountPoint::Name(d, n)) => *d == dir && n == name,
            _ => false,
        })
    }

    /// The root of mount `m`.
    fn mount_root(&self, m: usize) -> Node {
        Node {
            mount: m,
            ino: self.mounts[m].fs.root(),
        }
    }

    /// Something is mounted on `node`, or it is a mounted filesystem's root
    /// (what `child` gives for a mount on a name).
    fn is_mount_point(&self, node: Node) -> bool {
        self.mounted_on(node).is_some() || (node.mount != 0 && node == self.mount_root(node.mount))
    }

    fn is_dir(&mut self, node: Node) -> Result<bool, Errno> {
        Ok(self.stat(node)?.kind == FileType::Directory)
    }

    /// Resolves `path` to the trail of names leading to it.
    fn walk(&mut self, path: &[u8]) -> Result<Trail, Errno> {
        let p = path::parse(path)?;
        let mut trail = self.start(p.absolute)?;
        let last = p.components.len();
        for (i, c) in p.components.iter().enumerate() {
            self.step(&mut trail, *c)?;
            if i + 1 == last && matches!(c, Component::Name(_)) && !p.trailing_slash {
                return Ok(trail);
            }
        }
        // Ended in `/`, `.` or `..`: the target must be a directory.
        let target = trail.last().expect("never empty").0;
        if !self.is_dir(target)? {
            return Err(Errno::ENOTDIR);
        }
        Ok(trail)
    }

    fn start(&self, absolute: bool) -> Result<Trail, Errno> {
        if absolute {
            Ok(self.cwd[..1].to_vec())
        } else if self.cwd_gone {
            Err(Errno::ENOENT)
        } else {
            Ok(self.cwd.clone())
        }
    }

    fn in_cwd(&self, node: Node) -> bool {
        self.cwd.iter().any(|(n, _)| *n == node)
    }

    /// Takes one step from the directory at the end of `trail`.
    fn step(&mut self, trail: &mut Trail, c: Component<'_>) -> Result<(), Errno> {
        let here = trail.last().expect("never empty").0;
        if !self.is_dir(here)? {
            return Err(Errno::ENOTDIR);
        }
        match c {
            Component::Current => {}
            Component::Parent => {
                if trail.len() > 1 {
                    trail.pop();
                }
            }
            Component::Name(name) => {
                if let Some(m) = self.mounted_at_name(here, name) {
                    trail.push((self.mount_root(m), name.to_vec()));
                    return Ok(());
                }
                let ino = self.fs(here.mount)?.lookup(here.ino, name)?;
                let mut node = Node {
                    mount: here.mount,
                    ino,
                };
                if let Some(m) = self.mounted_on(node) {
                    node = Node {
                        mount: m,
                        ino: self.mounts[m].fs.root(),
                    };
                }
                trail.push((node, name.to_vec()));
            }
        }
        Ok(())
    }

    /// Resolves everything but the last name of `path`.
    fn parent(&mut self, path: &[u8]) -> Result<Parent, Errno> {
        let p = path::parse(path)?;
        let mut trail = self.start(p.absolute)?;
        let (name, init) = match p.components.split_last() {
            Some((Component::Name(n), init)) => (Some(n.to_vec()), init),
            _ => (None, &p.components[..]),
        };
        for c in init {
            self.step(&mut trail, *c)?;
        }
        let dir = trail.last().expect("never empty").0;
        if !self.is_dir(dir)? {
            return Err(Errno::ENOTDIR);
        }
        Ok(Parent {
            dir,
            name,
            trailing_slash: p.trailing_slash,
        })
    }

    /// The node `parent`'s name refers to, if any: for a mount on a name,
    /// the mounted root.
    fn child(&mut self, parent: &Parent) -> Result<Option<Node>, Errno> {
        let Some(name) = &parent.name else {
            return Ok(None);
        };
        if let Some(m) = self.mounted_at_name(parent.dir, name) {
            return Ok(Some(self.mount_root(m)));
        }
        match self.fs(parent.dir.mount)?.lookup(parent.dir.ino, name) {
            Ok(ino) => Ok(Some(Node {
                mount: parent.dir.mount,
                ino,
            })),
            Err(Errno::ENOENT) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Vfs for MountTable {
    fn cwd(&self) -> Vec<u8> {
        if self.cwd.len() == 1 {
            return b"/".to_vec();
        }
        let mut out = Vec::new();
        for (_, name) in &self.cwd[1..] {
            out.push(b'/');
            out.extend_from_slice(name);
        }
        out
    }

    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        let trail = self.walk(path)?;
        let target = trail.last().expect("never empty").0;
        if !self.is_dir(target)? {
            return Err(Errno::ENOTDIR);
        }
        self.cwd = trail;
        self.cwd_gone = false;
        Ok(())
    }

    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno> {
        Ok(self.walk(path)?.last().expect("never empty").0)
    }

    fn stat(&mut self, node: Node) -> Result<Stat, Errno> {
        self.fs(node.mount)?.stat(node.ino)
    }

    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        let mut entries = self.fs(node.mount)?.read_dir(node.ino)?;
        for (m, mount) in self.mounts.iter().enumerate() {
            if let Some(MountPoint::Name(dir, name)) = &mount.on
                && *dir == node
            {
                entries.push(DirEntry {
                    name: name.clone(),
                    ino: self.mounts[m].fs.root(),
                });
            }
        }
        Ok(entries)
    }

    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
        self.fs(node.mount)?.read_link(node.ino)
    }

    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.fs(node.mount)?.read_at(node.ino, offset, buf)
    }

    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.fs(node.mount)?.write_at(node.ino, offset, buf)
    }

    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno> {
        self.fs(node.mount)?.truncate(node.ino, size)
    }

    fn touch(&mut self, node: Node) -> Result<(), Errno> {
        self.fs(node.mount)?.touch(node.ino)
    }

    fn create(&mut self, path: &[u8]) -> Result<Node, Errno> {
        let parent = self.parent(path)?;
        let Some(name) = &parent.name else {
            return Err(Errno::EEXIST);
        };
        if parent.trailing_slash {
            return Err(Errno::EISDIR);
        }
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EEXIST);
        }
        let ino = self.fs(parent.dir.mount)?.create(parent.dir.ino, name)?;
        Ok(Node {
            mount: parent.dir.mount,
            ino,
        })
    }

    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        let parent = self.parent(path)?;
        let Some(name) = &parent.name else {
            return Err(Errno::EEXIST);
        };
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EEXIST);
        }
        self.fs(parent.dir.mount)?.mkdir(parent.dir.ino, name)?;
        Ok(())
    }

    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        let parent = self.parent(path)?;
        let Some(name) = &parent.name else {
            return Err(Errno::EISDIR);
        };
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EISDIR);
        }
        if parent.trailing_slash
            && let Some(child) = self.child(&parent)?
            && !self.is_dir(child)?
        {
            return Err(Errno::ENOTDIR);
        }
        self.fs(parent.dir.mount)?.unlink(parent.dir.ino, name)
    }

    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        let parent = self.parent(path)?;
        let Some(name) = &parent.name else {
            // Linux: `rmdir .` is EINVAL, `rmdir ..` ENOTEMPTY, `rmdir /`
            // EBUSY.
            let p = path::parse(path)?;
            return Err(match p.components.last() {
                Some(Component::Current) => Errno::EINVAL,
                Some(Component::Parent) => Errno::ENOTEMPTY,
                _ => Errno::EBUSY,
            });
        };
        let child = self.child(&parent)?;
        if child.is_some_and(|c| self.is_mount_point(c)) {
            return Err(Errno::EBUSY);
        }
        self.fs(parent.dir.mount)?.rmdir(parent.dir.ino, name)?;
        if child.is_some_and(|c| self.in_cwd(c)) {
            self.cwd_gone = true;
        }
        Ok(())
    }

    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        let src = self.parent(from)?;
        let dst = self.parent(to)?;
        let (Some(src_name), Some(dst_name)) = (&src.name, &dst.name) else {
            return Err(Errno::EBUSY);
        };
        if src.dir.mount != dst.dir.mount {
            return Err(Errno::EXDEV);
        }
        let Some(moving) = self.child(&src)? else {
            return Err(Errno::ENOENT);
        };
        if (src.trailing_slash || dst.trailing_slash) && !self.is_dir(moving)? {
            return Err(Errno::ENOTDIR);
        }
        if self.is_mount_point(moving) {
            return Err(Errno::EBUSY);
        }
        let target = self.child(&dst)?;
        if target.is_some_and(|t| self.is_mount_point(t)) {
            return Err(Errno::EBUSY);
        }
        let (from_dir, to_dir) = (src.dir.ino, dst.dir.ino);
        self.fs(src.dir.mount)?
            .rename(from_dir, src_name, to_dir, dst_name)?;
        // A directory replaced by the rename is gone; one moved takes the
        // current directory's path along.
        if target.is_some_and(|t| t != moving && self.in_cwd(t)) {
            self.cwd_gone = true;
        } else if let Some(k) = self.cwd.iter().position(|(n, _)| *n == moving) {
            match self.walk(to) {
                Ok(mut trail) => {
                    trail.extend_from_slice(&self.cwd[k + 1..]);
                    self.cwd = trail;
                }
                Err(_) => self.cwd_gone = true,
            }
        }
        Ok(())
    }

    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno> {
        let node = self.lookup(path)?;
        self.fs(node.mount)?.statfs()
    }

    fn sync(&mut self) -> Result<(), Errno> {
        let mut result = Ok(());
        for m in &mut self.mounts {
            if let Err(e) = m.fs.sync() {
                result = Err(e);
            }
        }
        result
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        let mut result = Ok(());
        for m in self.mounts.iter_mut().rev() {
            if let Err(e) = m.fs.shutdown() {
                result = Err(e);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Env, MemFs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            1_000
        }
        fn log(&self, _: &str) {}
    }

    fn memfs() -> MemFs {
        MemFs::new(Box::new(Clock))
    }

    /// `/etc/motd`, `/root/`, `/tmp/`, a symlink `/root/link -> /etc`.
    fn table() -> MountTable {
        let mut fs = memfs();
        let root = fs.root();
        let etc = fs.mkdir(root, b"etc").unwrap();
        let motd = fs.create(etc, b"motd").unwrap();
        fs.write_at(motd, 0, b"welcome\n").unwrap();
        let home = fs.mkdir(root, b"root").unwrap();
        fs.symlink(home, b"link", b"/etc").unwrap();
        fs.mkdir(root, b"tmp").unwrap();
        MountTable::new(Box::new(fs))
    }

    fn read(t: &mut MountTable, path: &[u8]) -> Result<Vec<u8>, Errno> {
        let node = t.lookup(path)?;
        let mut buf = alloc::vec![0; 64];
        let n = t.read_at(node, 0, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    #[test]
    fn absolute_and_relative_paths_resolve() {
        let mut t = table();
        assert_eq!(read(&mut t, b"/etc/motd").unwrap(), b"welcome\n");
        assert_eq!(t.cwd(), b"/");
        t.chdir(b"/root").unwrap();
        assert_eq!(t.cwd(), b"/root");
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
        assert_eq!(read(&mut t, b".//..//etc/./motd").unwrap(), b"welcome\n");
    }

    #[test]
    fn dot_dot_at_the_root_stays_at_the_root() {
        let mut t = table();
        t.chdir(b"/../../..").unwrap();
        assert_eq!(t.cwd(), b"/");
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
    }

    #[test]
    fn cd_keeps_the_names_it_walked() {
        let mut t = table();
        t.chdir(b"etc").unwrap();
        t.chdir(b"..").unwrap();
        t.chdir(b"root/.").unwrap();
        assert_eq!(t.cwd(), b"/root");
        assert_eq!(t.chdir(b"/etc/motd"), Err(Errno::ENOTDIR));
        assert_eq!(t.chdir(b"/nope"), Err(Errno::ENOENT));
        assert_eq!(t.cwd(), b"/root", "a failed cd changes nothing");
    }

    #[test]
    fn trailing_slashes_and_dots_need_a_directory() {
        let mut t = table();
        assert!(t.lookup(b"/etc/").is_ok());
        assert_eq!(t.lookup(b"/etc/motd/"), Err(Errno::ENOTDIR));
        assert_eq!(t.lookup(b"/etc/motd/."), Err(Errno::ENOTDIR));
        assert_eq!(t.lookup(b"/etc/motd/.."), Err(Errno::ENOTDIR));
        assert_eq!(t.lookup(b"/etc/motd/x"), Err(Errno::ENOTDIR));
    }

    #[test]
    fn symlinks_are_not_followed() {
        let mut t = table();
        let link = t.lookup(b"/root/link").unwrap();
        assert_eq!(t.stat(link).unwrap().kind, FileType::Symlink);
        assert_eq!(t.read_link(link).unwrap(), b"/etc");
        assert_eq!(t.lookup(b"/root/link/motd"), Err(Errno::ENOTDIR));
        assert_eq!(read(&mut t, b"/root/link"), Err(Errno::EINVAL));
    }

    #[test]
    fn long_names_and_paths_are_rejected() {
        let mut t = table();
        assert_eq!(t.lookup(&[b'a'; 256]), Err(Errno::ENAMETOOLONG));
        assert_eq!(t.create(&[b'a'; 256]), Err(Errno::ENAMETOOLONG));
        assert_eq!(t.lookup(b""), Err(Errno::ENOENT));
    }

    #[test]
    fn create_mkdir_and_their_edge_cases() {
        let mut t = table();
        let f = t.create(b"/tmp/new").unwrap();
        assert_eq!(t.lookup(b"/tmp/new").unwrap(), f);
        assert_eq!(t.create(b"/tmp/new"), Err(Errno::EEXIST));
        assert_eq!(t.create(b"/tmp/other/"), Err(Errno::EISDIR));
        assert_eq!(t.create(b"/tmp/."), Err(Errno::EEXIST));
        assert_eq!(t.create(b"/"), Err(Errno::EEXIST));
        assert_eq!(t.create(b"/missing/f"), Err(Errno::ENOENT));
        assert_eq!(t.create(b"/etc/motd/f"), Err(Errno::ENOTDIR));
        t.mkdir(b"/tmp/d/").unwrap();
        assert_eq!(t.mkdir(b"/tmp/d"), Err(Errno::EEXIST));
        assert_eq!(t.mkdir(b"/"), Err(Errno::EEXIST));
    }

    #[test]
    fn unlink_and_rmdir_edge_cases() {
        let mut t = table();
        assert_eq!(t.unlink(b"/etc/motd/"), Err(Errno::ENOTDIR));
        assert_eq!(t.unlink(b"/etc/"), Err(Errno::EISDIR));
        assert_eq!(t.unlink(b"/etc/.."), Err(Errno::EISDIR));
        t.unlink(b"/etc/motd").unwrap();
        assert_eq!(t.lookup(b"/etc/motd"), Err(Errno::ENOENT));
        assert_eq!(t.rmdir(b"/tmp/."), Err(Errno::EINVAL));
        assert_eq!(t.rmdir(b"/tmp/.."), Err(Errno::ENOTEMPTY));
        assert_eq!(t.rmdir(b"/"), Err(Errno::EBUSY));
        t.rmdir(b"/tmp/").unwrap();
        assert_eq!(t.lookup(b"/tmp"), Err(Errno::ENOENT));
    }

    #[test]
    fn rename_edge_cases() {
        let mut t = table();
        t.rename(b"/etc/motd", b"/tmp/motd").unwrap();
        assert_eq!(read(&mut t, b"/tmp/motd").unwrap(), b"welcome\n");
        assert_eq!(t.rename(b"/tmp/motd/", b"/tmp/x"), Err(Errno::ENOTDIR));
        assert_eq!(t.rename(b"/tmp/motd", b"/tmp/x/"), Err(Errno::ENOTDIR));
        assert_eq!(t.rename(b"/tmp/missing", b"/tmp/x"), Err(Errno::ENOENT));
        assert_eq!(t.rename(b"/", b"/tmp/x"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/tmp/..", b"/tmp/x"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/etc", b"/etc/sub"), Err(Errno::EINVAL));
        t.rename(b"/etc/", b"/etc2").unwrap();
        assert!(t.lookup(b"/etc2").is_ok());
    }

    #[test]
    fn a_removed_current_directory_resolves_nothing() {
        let mut t = table();
        t.mkdir(b"/tmp/x").unwrap();
        t.chdir(b"/tmp/x").unwrap();
        let x = t.lookup(b"/tmp/x").unwrap();
        t.rmdir(b"/tmp/x").unwrap();
        // Its inode number is reused at once; nothing relative may reach
        // the new directory through the old current directory.
        t.mkdir(b"/root/y").unwrap();
        assert_eq!(t.lookup(b"/root/y").unwrap(), x);
        assert_eq!(t.lookup(b"."), Err(Errno::ENOENT));
        assert_eq!(t.lookup(b".."), Err(Errno::ENOENT));
        assert_eq!(t.create(b"f"), Err(Errno::ENOENT));
        assert_eq!(t.lookup(b"/root/y/f"), Err(Errno::ENOENT));
        assert_eq!(t.cwd(), b"/tmp/x", "the prompt keeps the old path");
        assert!(t.lookup(b"/etc/motd").is_ok());
        t.chdir(b"/tmp").unwrap();
        assert!(t.create(b"f").is_ok());
    }

    #[test]
    fn renames_carry_the_current_directory_along_or_remove_it() {
        let mut t = table();
        t.mkdir(b"/tmp/a").unwrap();
        t.mkdir(b"/tmp/a/b").unwrap();
        t.chdir(b"/tmp/a/b").unwrap();
        t.rename(b"/tmp/a", b"/root/z").unwrap();
        assert_eq!(t.cwd(), b"/root/z/b");
        t.create(b"f").unwrap();
        assert!(t.lookup(b"/root/z/b/f").is_ok());
        assert_eq!(t.lookup(b"../..").unwrap(), t.lookup(b"/root").unwrap());
        // An empty current directory replaced by another one is gone.
        t.mkdir(b"/tmp/e").unwrap();
        t.mkdir(b"/tmp/other").unwrap();
        t.chdir(b"/tmp/e").unwrap();
        t.rename(b"/tmp/other", b"/tmp/e").unwrap();
        assert_eq!(t.lookup(b"."), Err(Errno::ENOENT));
    }

    #[test]
    fn a_second_filesystem_mounts_on_a_directory() {
        let mut t = table();
        let mut extra = memfs();
        let r = extra.root();
        extra.create(r, b"inside").unwrap();
        t.mount(b"/tmp", Box::new(extra)).unwrap();
        let inside = t.lookup(b"/tmp/inside").unwrap();
        assert_eq!(inside.mount, 1);
        // `..` from the mounted root goes back to the directory above the
        // mount point.
        t.chdir(b"/tmp").unwrap();
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
        assert_eq!(t.lookup(b".").unwrap().mount, 1);
        // Crossing filesystems and removing mount points fail.
        assert_eq!(t.rename(b"/tmp/inside", b"/root/inside"), Err(Errno::EXDEV));
        assert_eq!(t.rename(b"/tmp", b"/tmp2"), Err(Errno::EBUSY));
        assert_eq!(t.rmdir(b"/tmp"), Err(Errno::EBUSY));
        assert_eq!(t.mount(b"/tmp", Box::new(memfs())), Err(Errno::EBUSY));
        assert_eq!(
            t.mount(b"/etc/motd", Box::new(memfs())),
            Err(Errno::ENOTDIR)
        );
        // New files land in the mounted filesystem.
        t.create(b"/tmp/new").unwrap();
        assert_eq!(t.lookup(b"/tmp/new").unwrap().mount, 1);
    }

    fn programs() -> MemFs {
        let mut fs = memfs();
        let r = fs.root();
        let f = fs.create(r, b"prog").unwrap();
        fs.write_at(f, 0, b"program").unwrap();
        fs.read_only()
    }

    fn names_in(t: &mut MountTable, path: &[u8]) -> Vec<Vec<u8>> {
        let node = t.lookup(path).unwrap();
        let mut names: Vec<Vec<u8>> = t
            .read_dir(node)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_filesystem_mounts_on_a_name_its_directory_does_not_have() {
        let mut t = table();
        t.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(read(&mut t, b"/bin/prog").unwrap(), b"program");
        assert_eq!(t.lookup(b"/bin/prog").unwrap().mount, 1);
        let bin = t.lookup(b"/bin/").unwrap();
        assert_eq!(t.stat(bin).unwrap().kind, FileType::Directory);
        assert_eq!(
            names_in(&mut t, b"/"),
            [&b"."[..], b"..", b"bin", b"etc", b"root", b"tmp"],
            "listed once in its directory"
        );
        assert_eq!(
            names_in(&mut t, b"/etc"),
            [&b"."[..], b"..", b"motd"],
            "and nowhere else"
        );
        t.chdir(b"/bin").unwrap();
        assert_eq!(t.cwd(), b"/bin");
        assert_eq!(t.lookup(b".").unwrap().mount, 1);
        assert_eq!(read(&mut t, b"prog").unwrap(), b"program");
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
        assert_eq!(read(&mut t, b"/bin/../bin/./prog").unwrap(), b"program");
        assert!(t.statfs(b"/bin").is_ok());
    }

    #[test]
    fn a_mount_on_a_name_is_a_mount_point_like_any_other() {
        let mut t = table();
        t.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(t.rmdir(b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rmdir(b"/bin/"), Err(Errno::EBUSY));
        assert_eq!(t.unlink(b"/bin"), Err(Errno::EISDIR));
        assert_eq!(t.mkdir(b"/bin"), Err(Errno::EEXIST));
        assert_eq!(t.create(b"/bin"), Err(Errno::EEXIST));
        assert_eq!(t.rename(b"/bin", b"/bin2"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/etc/motd", b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/tmp", b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/etc/motd", b"/bin/motd"), Err(Errno::EXDEV));
        assert_eq!(t.create(b"/bin/new"), Err(Errno::EROFS));
        assert_eq!(t.mount(b"/bin", Box::new(memfs())), Err(Errno::EBUSY));
        // The filesystem below never got the name.
        assert_eq!(names_in(&mut t, b"/root"), [&b"."[..], b"..", b"link"]);
        t.mkdir(b"/tmp/bin").unwrap();
        assert_eq!(
            names_in(&mut t, b"/tmp"),
            [&b"."[..], b"..", b"bin"],
            "other directories keep the name free"
        );
    }

    #[test]
    fn only_the_last_name_of_a_mount_point_may_be_missing() {
        let mut t = table();
        assert_eq!(
            t.mount(b"/missing/bin", Box::new(memfs())),
            Err(Errno::ENOENT)
        );
        assert_eq!(
            t.mount(b"/etc/motd/bin", Box::new(memfs())),
            Err(Errno::ENOTDIR)
        );
        assert_eq!(
            t.mount(b"/root/link/bin", Box::new(memfs())),
            Err(Errno::ENOTDIR)
        );
        assert_eq!(
            t.mount(b"/tmp/..", Box::new(memfs())),
            Err(Errno::EBUSY),
            "that is /"
        );
        // A read-only root (the fallbacks of M1 §10) gets one too.
        let mut ro = MountTable::new(Box::new(memfs().read_only()));
        ro.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(read(&mut ro, b"/bin/prog").unwrap(), b"program");
    }

    #[test]
    fn shutdown_reaches_every_filesystem() {
        let mut t = table();
        t.mount(b"/tmp", Box::new(memfs())).unwrap();
        t.shutdown().unwrap();
        assert_eq!(t.create(b"/f"), Err(Errno::EROFS));
        assert_eq!(t.create(b"/tmp/f"), Err(Errno::EROFS));
        assert!(t.statfs(b"/tmp").is_ok());
    }
}
