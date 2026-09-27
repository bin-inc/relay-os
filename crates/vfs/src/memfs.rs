//! An in-memory filesystem. It keeps the [`FileSystem`] contract with as
//! little machinery as possible, so it serves as the shell's test
//! filesystem, as the reference model the ext2 driver is checked against,
//! and as the empty read-only `/` the kernel falls back to when no disk
//! mounts (spec §10).

use crate::fs::{DirEntry, Env, FileSystem, FileType, Ino, Stat, StatFs};
use crate::{Errno, path};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use alloc::vec;
use alloc::vec::Vec;

/// File data lives in chunks of this size; missing chunks are holes.
const CHUNK: u64 = 4096;
/// The largest file (1 TiB).
pub const MAX_FILE_SIZE: u64 = 1 << 40;
const ROOT: Ino = 1;
/// What `statfs` reports as the size when there is no capacity limit.
const UNLIMITED: u64 = 1 << 32;

enum Body {
    File {
        size: u64,
        chunks: BTreeMap<u64, Box<[u8]>>,
    },
    Dir {
        parent: Ino,
        entries: BTreeMap<Vec<u8>, Ino>,
    },
    Symlink(Vec<u8>),
}

struct Node {
    perm: u16,
    nlink: u32,
    atime: u64,
    mtime: u64,
    ctime: u64,
    body: Body,
}

pub struct MemFs {
    nodes: BTreeMap<Ino, Node>,
    env: Box<dyn Env>,
    read_only: bool,
    /// Bytes of file data allowed (whole chunks), if limited.
    capacity: Option<u64>,
    used: u64,
}

impl MemFs {
    /// An empty filesystem: just the root directory, mode `0755`.
    pub fn new(env: Box<dyn Env>) -> MemFs {
        let now = env.now();
        let mut nodes = BTreeMap::new();
        nodes.insert(ROOT, Node::new(0o755, 2, now, Body::dir(ROOT)));
        MemFs {
            nodes,
            env,
            read_only: false,
            capacity: None,
            used: 0,
        }
    }

    /// Limits file data to `bytes` (rounded down to whole 4 KiB chunks), so
    /// tests can fill the filesystem.
    pub fn with_capacity(mut self, bytes: u64) -> MemFs {
        self.capacity = Some(bytes / CHUNK * CHUNK);
        self
    }

    /// Makes every change fail with `EROFS`.
    pub fn read_only(mut self) -> MemFs {
        self.read_only = true;
        self
    }

    /// Creates a symbolic link (tests only: the shell cannot make one).
    pub fn symlink(&mut self, dir: Ino, name: &[u8], target: &[u8]) -> Result<Ino, Errno> {
        self.new_node(dir, name, 0o777, Body::Symlink(target.to_vec()))
    }

    /// Enters inode `target` in `dir` once more (tests only). Unlike a
    /// real hard link it accepts a directory, to model the alias a corrupt
    /// disk can hold.
    pub fn link(&mut self, dir: Ino, name: &[u8], target: Ino) -> Result<(), Errno> {
        self.check_change(dir, name)?;
        self.node(target)?;
        if self.entries(dir)?.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        self.entries_mut(dir)?.insert(name.to_vec(), target);
        self.node_mut(target)?.nlink += 1;
        Ok(())
    }

    fn node(&self, ino: Ino) -> Result<&Node, Errno> {
        self.nodes.get(&ino).ok_or(Errno::ENOENT)
    }

    fn node_mut(&mut self, ino: Ino) -> Result<&mut Node, Errno> {
        self.nodes.get_mut(&ino).ok_or(Errno::ENOENT)
    }

    fn entries(&self, dir: Ino) -> Result<&BTreeMap<Vec<u8>, Ino>, Errno> {
        match &self.node(dir)?.body {
            Body::Dir { entries, .. } => Ok(entries),
            _ => Err(Errno::ENOTDIR),
        }
    }

    fn entries_mut(&mut self, dir: Ino) -> Result<&mut BTreeMap<Vec<u8>, Ino>, Errno> {
        match &mut self.node_mut(dir)?.body {
            Body::Dir { entries, .. } => Ok(entries),
            _ => Err(Errno::ENOTDIR),
        }
    }

    fn is_dir(&self, ino: Ino) -> Result<bool, Errno> {
        Ok(matches!(self.node(ino)?.body, Body::Dir { .. }))
    }

    /// Rules 1–4 of the contract for a change to `dir/name`.
    fn check_change(&self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.node(dir)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        path::check_name(name)?;
        self.entries(dir).map(|_| ())
    }

    /// Rules 1–2 for a change to a regular file's data, then its type.
    fn check_file_change(&self, ino: Ino) -> Result<(), Errno> {
        let node = self.node(ino)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        match node.body {
            Body::File { .. } => Ok(()),
            Body::Dir { .. } => Err(Errno::EISDIR),
            Body::Symlink(_) => Err(Errno::EINVAL),
        }
    }

    fn new_node(&mut self, dir: Ino, name: &[u8], perm: u16, body: Body) -> Result<Ino, Errno> {
        self.check_change(dir, name)?;
        if self.entries(dir)?.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        let now = self.env.now();
        let ino = self.free_ino();
        let nlink = if matches!(body, Body::Dir { .. }) {
            2
        } else {
            1
        };
        self.nodes.insert(ino, Node::new(perm, nlink, now, body));
        self.entries_mut(dir)?.insert(name.to_vec(), ino);
        self.changed_dir(dir, now);
        Ok(ino)
    }

    /// The lowest unused inode number. Freed numbers are reused, as on a
    /// real disk, so code that keeps a stale number around gets caught.
    fn free_ino(&self) -> Ino {
        let mut ino = ROOT + 1;
        for &used in self.nodes.keys().filter(|&&k| k > ROOT) {
            if used != ino {
                break;
            }
            ino += 1;
        }
        ino
    }

    fn changed_dir(&mut self, dir: Ino, now: u64) {
        if let Some(d) = self.nodes.get_mut(&dir) {
            d.mtime = now;
            d.ctime = now;
        }
    }

    /// Drops one link; frees the inode and its data with the last one.
    fn drop_link(&mut self, ino: Ino, now: u64) {
        let Some(node) = self.nodes.get_mut(&ino) else {
            return;
        };
        node.nlink = node.nlink.saturating_sub(1);
        node.ctime = now;
        // A directory has lost its only name (its `.` does not count).
        if node.nlink > 0 && !matches!(node.body, Body::Dir { .. }) {
            return;
        }
        if let Some(Node {
            body: Body::File { chunks, .. },
            ..
        }) = self.nodes.remove(&ino)
        {
            self.used -= chunks.len() as u64 * CHUNK;
        }
    }

    /// Whether `ancestor` is `ino` or one of its parents.
    fn is_ancestor(&self, ancestor: Ino, mut ino: Ino) -> bool {
        loop {
            if ino == ancestor {
                return true;
            }
            match self.nodes.get(&ino).map(|n| &n.body) {
                Some(Body::Dir { parent, .. }) if *parent != ino => ino = *parent,
                _ => return false,
            }
        }
    }
}

impl Node {
    fn new(perm: u16, nlink: u32, now: u64, body: Body) -> Node {
        Node {
            perm,
            nlink,
            atime: now,
            mtime: now,
            ctime: now,
            body,
        }
    }
}

impl Body {
    fn dir(parent: Ino) -> Body {
        Body::Dir {
            parent,
            entries: BTreeMap::new(),
        }
    }
}

impl FileSystem for MemFs {
    fn root(&self) -> Ino {
        ROOT
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let n = self.node(ino)?;
        let (kind, size, blocks) = match &n.body {
            Body::File { size, chunks } => {
                (FileType::Regular, *size, chunks.len() as u64 * CHUNK / 512)
            }
            Body::Dir { .. } => (FileType::Directory, CHUNK, CHUNK / 512),
            Body::Symlink(t) => (FileType::Symlink, t.len() as u64, 0),
        };
        Ok(Stat {
            ino,
            kind,
            perm: n.perm,
            nlink: n.nlink,
            uid: 0,
            gid: 0,
            size,
            blocks,
            block_size: CHUNK as u32,
            atime: n.atime,
            mtime: n.mtime,
            ctime: n.ctime,
        })
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        let entries = self.entries(dir)?;
        match name {
            b"." => Ok(dir),
            b".." => match self.node(dir)?.body {
                Body::Dir { parent, .. } => Ok(parent),
                _ => Err(Errno::ENOTDIR),
            },
            _ => entries.get(name).copied().ok_or(Errno::ENOENT),
        }
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        let parent = self.lookup(dir, b"..")?;
        let mut out = vec![
            DirEntry {
                name: b".".to_vec(),
                ino: dir,
            },
            DirEntry {
                name: b"..".to_vec(),
                ino: parent,
            },
        ];
        for (name, &ino) in self.entries(dir)? {
            out.push(DirEntry {
                name: name.clone(),
                ino,
            });
        }
        Ok(out)
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        match &self.node(ino)?.body {
            Body::Symlink(target) => Ok(target.clone()),
            _ => Err(Errno::EINVAL),
        }
    }

    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        let (size, chunks) = match &self.node(ino)?.body {
            Body::File { size, chunks } => (*size, chunks),
            Body::Dir { .. } => return Err(Errno::EISDIR),
            Body::Symlink(_) => return Err(Errno::EINVAL),
        };
        if offset >= size {
            return Ok(0);
        }
        let n = buf.len().min((size - offset) as usize);
        let mut done = 0;
        while done < n {
            let pos = offset + done as u64;
            let (index, within) = (pos / CHUNK, (pos % CHUNK) as usize);
            let len = (n - done).min(CHUNK as usize - within);
            let dst = &mut buf[done..done + len];
            match chunks.get(&index) {
                Some(chunk) => dst.copy_from_slice(&chunk[within..within + len]),
                None => dst.fill(0),
            }
            done += len;
        }
        Ok(n)
    }

    fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.check_file_change(ino)?;
        if buf.is_empty() {
            return Ok(0);
        }
        match offset.checked_add(buf.len() as u64) {
            Some(end) if end <= MAX_FILE_SIZE => {}
            _ => return Err(Errno::EFBIG),
        }
        let now = self.env.now();
        let (capacity, mut used) = (self.capacity, self.used);
        let node = self.node_mut(ino)?;
        let Body::File { size, chunks } = &mut node.body else {
            unreachable!("checked above");
        };
        let mut done = 0;
        while done < buf.len() {
            let pos = offset + done as u64;
            let (index, within) = (pos / CHUNK, (pos % CHUNK) as usize);
            let len = (buf.len() - done).min(CHUNK as usize - within);
            let chunk = match chunks.entry(index) {
                Entry::Occupied(e) => e.into_mut(),
                Entry::Vacant(e) => {
                    if capacity.is_some_and(|c| used + CHUNK > c) {
                        break;
                    }
                    used += CHUNK;
                    e.insert(vec![0; CHUNK as usize].into_boxed_slice())
                }
            };
            chunk[within..within + len].copy_from_slice(&buf[done..done + len]);
            done += len;
        }
        if done == 0 {
            return Err(Errno::ENOSPC);
        }
        *size = (*size).max(offset + done as u64);
        node.mtime = now;
        node.ctime = now;
        self.used = used;
        Ok(done)
    }

    fn truncate(&mut self, ino: Ino, new_size: u64) -> Result<(), Errno> {
        self.check_file_change(ino)?;
        if new_size > MAX_FILE_SIZE {
            return Err(Errno::EFBIG);
        }
        let now = self.env.now();
        let node = self.node_mut(ino)?;
        let Body::File { size, chunks } = &mut node.body else {
            unreachable!("checked above");
        };
        let keep = new_size.div_ceil(CHUNK);
        let before = chunks.len();
        chunks.retain(|&index, _| index < keep);
        let freed = (before - chunks.len()) as u64 * CHUNK;
        // A later growth must read zeros past the new end.
        if let Some(last) = chunks.get_mut(&(new_size / CHUNK)) {
            last[(new_size % CHUNK) as usize..].fill(0);
        }
        *size = new_size;
        node.mtime = now;
        node.ctime = now;
        self.used -= freed;
        Ok(())
    }

    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        self.node(ino)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        let now = self.env.now();
        let node = self.node_mut(ino)?;
        node.mtime = now;
        node.ctime = now;
        Ok(())
    }

    fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        let body = Body::File {
            size: 0,
            chunks: BTreeMap::new(),
        };
        self.new_node(dir, name, 0o644, body)
    }

    fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        let ino = self.new_node(dir, name, 0o755, Body::dir(dir))?;
        self.node_mut(dir)?.nlink += 1;
        Ok(ino)
    }

    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.check_change(dir, name)?;
        let ino = self.lookup(dir, name)?;
        if self.is_dir(ino)? {
            return Err(Errno::EISDIR);
        }
        let now = self.env.now();
        self.entries_mut(dir)?.remove(name);
        self.drop_link(ino, now);
        self.changed_dir(dir, now);
        Ok(())
    }

    fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.check_change(dir, name)?;
        let ino = self.lookup(dir, name)?;
        if !self.entries(ino)?.is_empty() {
            return Err(Errno::ENOTEMPTY);
        }
        let now = self.env.now();
        self.entries_mut(dir)?.remove(name);
        self.drop_link(ino, now);
        let parent = self.node_mut(dir)?;
        parent.nlink -= 1;
        self.changed_dir(dir, now);
        Ok(())
    }

    fn rename(&mut self, from_dir: Ino, from: &[u8], to_dir: Ino, to: &[u8]) -> Result<(), Errno> {
        self.node(from_dir)?;
        self.node(to_dir)?;
        if self.read_only {
            return Err(Errno::EROFS);
        }
        path::check_name(from)?;
        path::check_name(to)?;
        self.entries(from_dir)?;
        self.entries(to_dir)?;
        let src = self.lookup(from_dir, from)?;
        let target = self.entries(to_dir)?.get(to).copied();
        if target == Some(src) {
            return Ok(());
        }
        let src_is_dir = self.is_dir(src)?;
        if src_is_dir && self.is_ancestor(src, to_dir) {
            return Err(Errno::EINVAL);
        }
        if let Some(t) = target {
            let t_is_dir = self.is_dir(t)?;
            if src_is_dir && !t_is_dir {
                return Err(Errno::ENOTDIR);
            }
            if !src_is_dir && t_is_dir {
                return Err(Errno::EISDIR);
            }
            if t_is_dir && !self.entries(t)?.is_empty() {
                return Err(Errno::ENOTEMPTY);
            }
        }
        let now = self.env.now();
        if let Some(t) = target {
            if self.is_dir(t)? {
                self.node_mut(to_dir)?.nlink -= 1;
            }
            self.drop_link(t, now);
        }
        self.entries_mut(from_dir)?.remove(from);
        self.entries_mut(to_dir)?.insert(to.to_vec(), src);
        if src_is_dir && from_dir != to_dir {
            if let Body::Dir { parent, .. } = &mut self.node_mut(src)?.body {
                *parent = to_dir;
            }
            self.node_mut(from_dir)?.nlink -= 1;
            self.node_mut(to_dir)?.nlink += 1;
        }
        self.node_mut(src)?.ctime = now;
        self.changed_dir(from_dir, now);
        self.changed_dir(to_dir, now);
        Ok(())
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        let blocks = self.capacity.unwrap_or(UNLIMITED) / CHUNK;
        let free = blocks.saturating_sub(self.used / CHUNK);
        Ok(StatFs {
            block_size: CHUNK,
            blocks,
            free_blocks: free,
            avail_blocks: free,
            files: self.nodes.len() as u64,
            free_files: u32::MAX as u64 - self.nodes.len() as u64,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        self.read_only = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    struct Clock(Cell<u64>);

    impl Env for Clock {
        fn now(&self) -> u64 {
            self.0.get()
        }
        fn log(&self, _: &str) {}
    }

    fn fs() -> MemFs {
        MemFs::new(Box::new(Clock(Cell::new(1_000))))
    }

    fn names(fs: &mut MemFs, dir: Ino) -> Vec<Vec<u8>> {
        let mut v: Vec<_> = fs
            .read_dir(dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_new_filesystem_has_an_empty_root() {
        let mut fs = fs();
        let root = fs.root();
        let st = fs.stat(root).unwrap();
        assert_eq!(
            (st.kind, st.perm, st.nlink),
            (FileType::Directory, 0o755, 2)
        );
        assert_eq!(names(&mut fs, root), [b".".to_vec(), b"..".to_vec()]);
        assert_eq!(fs.lookup(root, b"..").unwrap(), root);
        assert_eq!(fs.lookup(root, b"nope"), Err(Errno::ENOENT));
        assert_eq!(fs.stat(99), Err(Errno::ENOENT));
    }

    #[test]
    fn files_are_written_read_and_created_with_the_contract_modes() {
        let mut fs = fs();
        let f = fs.create(ROOT, b"notes").unwrap();
        let st = fs.stat(f).unwrap();
        assert_eq!(
            (st.kind, st.perm, st.nlink, st.size),
            (FileType::Regular, 0o644, 1, 0)
        );
        assert_eq!((st.uid, st.gid, st.atime, st.mtime), (0, 0, 1_000, 1_000));
        assert_eq!(fs.write_at(f, 0, b"hello world").unwrap(), 11);
        let mut buf = [0u8; 32];
        assert_eq!(fs.read_at(f, 6, &mut buf).unwrap(), 5);
        assert_eq!(&buf[..5], b"world");
        assert_eq!(fs.read_at(f, 11, &mut buf).unwrap(), 0);
        assert_eq!(fs.read_at(f, 100, &mut buf).unwrap(), 0);
        assert_eq!(fs.create(ROOT, b"notes"), Err(Errno::EEXIST));
    }

    #[test]
    fn writing_past_the_end_leaves_a_hole_that_reads_as_zeros() {
        let mut fs = fs();
        let f = fs.create(ROOT, b"sparse").unwrap();
        fs.write_at(f, 3 * CHUNK + 10, b"end").unwrap();
        let st = fs.stat(f).unwrap();
        assert_eq!(st.size, 3 * CHUNK + 13);
        assert_eq!(st.blocks, CHUNK / 512, "only the written chunk is stored");
        let mut buf = vec![0xAAu8; 20];
        assert_eq!(fs.read_at(f, 3 * CHUNK, &mut buf).unwrap(), 13);
        assert_eq!(&buf[..13], b"\0\0\0\0\0\0\0\0\0\0end");
    }

    #[test]
    fn shrinking_frees_data_and_regrowth_reads_zeros() {
        let mut fs = fs().with_capacity(2 * CHUNK);
        let f = fs.create(ROOT, b"f").unwrap();
        fs.write_at(f, 0, &[7u8; 2 * CHUNK as usize]).unwrap();
        assert_eq!(fs.statfs().unwrap().free_blocks, 0);
        fs.truncate(f, 5).unwrap();
        assert_eq!(fs.statfs().unwrap().free_blocks, 1);
        fs.truncate(f, 10).unwrap();
        let mut buf = [0xAAu8; 10];
        fs.read_at(f, 0, &mut buf).unwrap();
        assert_eq!(buf, [7, 7, 7, 7, 7, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_full_filesystem_writes_what_fits_then_reports_enospc() {
        let mut fs = fs().with_capacity(CHUNK);
        let f = fs.create(ROOT, b"f").unwrap();
        assert_eq!(
            fs.write_at(f, 100, &[1u8; 8192]).unwrap(),
            CHUNK as usize - 100
        );
        assert_eq!(fs.write_at(f, CHUNK, b"more"), Err(Errno::ENOSPC));
        assert_eq!(fs.stat(f).unwrap().size, CHUNK);
        assert_eq!(fs.write_at(f, 0, b"overwrite fits").unwrap(), 14);
    }

    #[test]
    fn file_size_limits_are_efbig() {
        let mut fs = fs();
        let f = fs.create(ROOT, b"f").unwrap();
        assert_eq!(fs.write_at(f, MAX_FILE_SIZE, b"x"), Err(Errno::EFBIG));
        assert_eq!(fs.write_at(f, u64::MAX, b"x"), Err(Errno::EFBIG));
        assert_eq!(fs.truncate(f, MAX_FILE_SIZE + 1), Err(Errno::EFBIG));
        assert_eq!(fs.write_at(f, MAX_FILE_SIZE, b""), Ok(0));
    }

    #[test]
    fn data_operations_check_the_file_type() {
        let mut fs = fs();
        let d = fs.mkdir(ROOT, b"d").unwrap();
        let l = fs.symlink(ROOT, b"l", b"d").unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(fs.read_at(d, 0, &mut buf), Err(Errno::EISDIR));
        assert_eq!(fs.write_at(d, 0, b"x"), Err(Errno::EISDIR));
        assert_eq!(fs.truncate(d, 0), Err(Errno::EISDIR));
        assert_eq!(fs.read_at(l, 0, &mut buf), Err(Errno::EINVAL));
        assert_eq!(fs.write_at(l, 0, b"x"), Err(Errno::EINVAL));
        assert_eq!(fs.read_link(l).unwrap(), b"d");
        assert_eq!(fs.read_link(d), Err(Errno::EINVAL));
        assert_eq!(fs.lookup(l, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.create(l, b"x"), Err(Errno::ENOTDIR));
    }

    #[test]
    fn mkdir_and_rmdir_keep_link_counts() {
        let mut fs = fs();
        let a = fs.mkdir(ROOT, b"a").unwrap();
        let b = fs.mkdir(a, b"b").unwrap();
        assert_eq!(fs.stat(ROOT).unwrap().nlink, 3);
        assert_eq!(fs.stat(a).unwrap().nlink, 3);
        assert_eq!(fs.stat(b).unwrap().nlink, 2);
        assert_eq!(fs.lookup(b, b"..").unwrap(), a);
        assert_eq!(fs.rmdir(ROOT, b"a"), Err(Errno::ENOTEMPTY));
        fs.create(ROOT, b"f").unwrap();
        assert_eq!(fs.rmdir(ROOT, b"f"), Err(Errno::ENOTDIR));
        assert_eq!(fs.rmdir(ROOT, b"missing"), Err(Errno::ENOENT));
        fs.rmdir(a, b"b").unwrap();
        assert_eq!(fs.stat(a).unwrap().nlink, 2);
        assert_eq!(fs.stat(b), Err(Errno::ENOENT), "the inode is freed");
        fs.rmdir(ROOT, b"a").unwrap();
        assert_eq!(fs.stat(ROOT).unwrap().nlink, 2);
    }

    #[test]
    fn unlink_frees_files_but_refuses_directories() {
        let mut fs = fs().with_capacity(CHUNK);
        let f = fs.create(ROOT, b"f").unwrap();
        fs.write_at(f, 0, b"data").unwrap();
        fs.mkdir(ROOT, b"d").unwrap();
        assert_eq!(fs.unlink(ROOT, b"d"), Err(Errno::EISDIR));
        fs.unlink(ROOT, b"f").unwrap();
        assert_eq!(fs.stat(f), Err(Errno::ENOENT));
        assert_eq!(fs.statfs().unwrap().free_blocks, 1);
        assert_eq!(fs.unlink(ROOT, b"f"), Err(Errno::ENOENT));
    }

    #[test]
    fn link_enters_an_inode_again() {
        let mut fs = fs();
        let d = fs.mkdir(ROOT, b"d").unwrap();
        fs.link(ROOT, b"alias", d).unwrap();
        assert_eq!(fs.lookup(ROOT, b"alias").unwrap(), d);
        assert_eq!(fs.stat(d).unwrap().nlink, 3);
        assert_eq!(fs.link(ROOT, b"alias", d), Err(Errno::EEXIST));
        assert_eq!(fs.link(ROOT, b"x", 99), Err(Errno::ENOENT));
    }

    #[test]
    fn freed_inode_numbers_are_reused() {
        let mut fs = fs();
        let a = fs.create(ROOT, b"a").unwrap();
        let b = fs.mkdir(ROOT, b"b").unwrap();
        fs.unlink(ROOT, b"a").unwrap();
        assert_eq!(fs.create(ROOT, b"c").unwrap(), a);
        assert_eq!(fs.create(ROOT, b"d").unwrap(), b + 1);
    }

    #[test]
    fn names_are_checked_before_anything_else_but_the_directory() {
        let mut fs = fs();
        assert_eq!(fs.create(ROOT, b""), Err(Errno::ENOENT));
        assert_eq!(fs.mkdir(ROOT, b".."), Err(Errno::EINVAL));
        assert_eq!(fs.unlink(ROOT, b"a/b"), Err(Errno::EINVAL));
        assert_eq!(fs.rmdir(ROOT, &[b'x'; 256]), Err(Errno::ENAMETOOLONG));
        assert_eq!(fs.create(77, b""), Err(Errno::ENOENT));
    }

    #[test]
    fn rename_moves_and_replaces_by_the_contract() {
        let mut fs = fs();
        let a = fs.mkdir(ROOT, b"a").unwrap();
        let b = fs.mkdir(ROOT, b"b").unwrap();
        let f = fs.create(a, b"f").unwrap();
        let g = fs.create(b, b"g").unwrap();
        // A file replaces a file; the replaced inode is freed.
        fs.rename(a, b"f", b, b"g").unwrap();
        assert_eq!(fs.lookup(b, b"g").unwrap(), f);
        assert_eq!(fs.stat(g), Err(Errno::ENOENT));
        assert_eq!(fs.lookup(a, b"f"), Err(Errno::ENOENT));
        // Same inode: nothing happens.
        fs.rename(b, b"g", b, b"g").unwrap();
        // Type clashes and non-empty targets.
        let c = fs.mkdir(ROOT, b"c").unwrap();
        assert_eq!(fs.rename(ROOT, b"c", b, b"g"), Err(Errno::ENOTDIR));
        assert_eq!(fs.rename(b, b"g", ROOT, b"c"), Err(Errno::EISDIR));
        assert_eq!(fs.rename(ROOT, b"c", ROOT, b"b"), Err(Errno::ENOTEMPTY));
        assert_eq!(fs.rename(ROOT, b"missing", ROOT, b"x"), Err(Errno::ENOENT));
        // A directory moves into another; link counts and `..` follow.
        fs.rename(ROOT, b"c", a, b"c2").unwrap();
        assert_eq!(fs.lookup(c, b"..").unwrap(), a);
        assert_eq!(fs.stat(ROOT).unwrap().nlink, 4);
        assert_eq!(fs.stat(a).unwrap().nlink, 3);
        // An empty directory is replaced by a directory.
        let e = fs.mkdir(ROOT, b"e").unwrap();
        fs.rename(a, b"c2", ROOT, b"e").unwrap();
        assert_eq!(fs.stat(e), Err(Errno::ENOENT));
        assert_eq!(fs.lookup(ROOT, b"e").unwrap(), c);
        assert_eq!(fs.stat(ROOT).unwrap().nlink, 5);
        assert_eq!(fs.stat(a).unwrap().nlink, 2);
    }

    #[test]
    fn a_directory_cannot_move_into_itself_or_below() {
        let mut fs = fs();
        let a = fs.mkdir(ROOT, b"a").unwrap();
        let b = fs.mkdir(a, b"b").unwrap();
        assert_eq!(fs.rename(ROOT, b"a", a, b"x"), Err(Errno::EINVAL));
        assert_eq!(fs.rename(ROOT, b"a", b, b"x"), Err(Errno::EINVAL));
        // EINVAL comes before the target's own checks.
        fs.create(b, b"file").unwrap();
        assert_eq!(fs.rename(ROOT, b"a", b, b"file"), Err(Errno::EINVAL));
        // Moving up is fine.
        fs.rename(a, b"b", ROOT, b"b").unwrap();
        assert_eq!(fs.lookup(b, b"..").unwrap(), ROOT);
    }

    #[test]
    fn a_read_only_filesystem_refuses_every_change_first() {
        let mut fs = fs();
        let f = fs.create(ROOT, b"f").unwrap();
        let mut fs = fs.read_only();
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
        assert_eq!(fs.mkdir(ROOT, b""), Err(Errno::EROFS));
        assert_eq!(fs.write_at(f, 0, b""), Err(Errno::EROFS));
        assert_eq!(fs.truncate(f, 0), Err(Errno::EROFS));
        assert_eq!(fs.touch(f), Err(Errno::EROFS));
        assert_eq!(fs.unlink(ROOT, b"f"), Err(Errno::EROFS));
        assert_eq!(fs.rename(ROOT, b"f", ROOT, b"g"), Err(Errno::EROFS));
        assert_eq!(fs.touch(99), Err(Errno::ENOENT));
        assert!(fs.stat(f).is_ok());
    }

    #[test]
    fn times_follow_the_contract() {
        let clock: &'static Clock = Box::leak(Box::new(Clock(Cell::new(10))));
        let mut fs = MemFs::new(Box::new(clock));
        let f = fs.create(ROOT, b"f").unwrap();
        clock.0.set(20);
        fs.write_at(f, 0, b"x").unwrap();
        let st = fs.stat(f).unwrap();
        assert_eq!((st.atime, st.mtime, st.ctime), (10, 20, 20));
        clock.0.set(30);
        fs.touch(f).unwrap();
        assert_eq!(fs.stat(f).unwrap().mtime, 30);
        clock.0.set(40);
        fs.mkdir(ROOT, b"d").unwrap();
        let root = fs.stat(ROOT).unwrap();
        assert_eq!((root.mtime, root.ctime), (40, 40));
    }

    #[test]
    fn shutdown_leaves_it_read_only() {
        let mut fs = fs();
        fs.shutdown().unwrap();
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
    }
}
