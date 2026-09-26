//! The randomized model test (spec §12): seeded operations run on ext2 and
//! on a `MemFs` through the `FileSystem` trait, and every result must be
//! the same, errors included. Inode numbers, block counts, directory sizes
//! and times legitimately differ and are not compared. After each batch
//! the whole trees are compared and e2fsck must find the image clean.
//!
//! Replay one run: `RELAY_MODEL_SEED=<n>` (with `RELAY_MODEL_BS` and
//! `RELAY_MODEL_CACHE` to pick the run) and `RELAY_MODEL_OPS=<n>` for the
//! number of operations; a failure prints the exact command.

mod common;

use common::*;
use ext2::{Ext2, MountOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use vfs::{Errno, FileSystem, FileType, Ino, MemFs, Stat};

const OPS: usize = 2000;
const BATCH: usize = 200;
/// Files up to this size are compared whole; larger ones (sparse) around
/// the offsets written.
const WHOLE: u64 = 4 << 20;
/// Names are mostly from a small pool, so they collide.
const POOL: &[&str] = &["a", "b", "c", "d", "e", "f", "g", "h", "dir", "file"];

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().map(|v| v.parse().expect(key))
}

/// Both inode numbers of one node: `(MemFs, ext2)`.
type Pair = (Ino, Ino);

#[derive(Debug)]
enum Op {
    Create(Pair, Name),
    Mkdir(Pair, Name),
    Write(Pair, u64, usize),
    Truncate(Pair, u64),
    Read(Pair, u64, usize),
    Rename(Pair, Name, Pair, Name),
    Unlink(Pair, Name),
    Rmdir(Pair, Name),
    Touch(Pair),
    Lookup(Pair, Name),
    ReadDir(Pair),
    Stat(Pair),
    /// Power cut after a command: sync, drop, remount.
    SyncRemount,
    ShutdownRemount,
}

/// A name, shown readably in failure messages.
#[derive(Clone)]
struct Name(Vec<u8>);

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.len() > 20 {
            write!(
                f,
                "{:?}…({} bytes)",
                String::from_utf8_lossy(&self.0[..8]),
                self.0.len()
            )
        } else {
            write!(f, "{:?}", String::from_utf8_lossy(&self.0))
        }
    }
}

struct Model {
    seed: u64,
    bs: u32,
    cache: usize,
    ops: usize,
    img: PathBuf,
    env: Rc<TestEnv>,
    ext: Option<Ext2<FileDisk>>,
    mem: MemFs,
    rng: Rng,
    dirs: Vec<Pair>,
    files: Vec<Pair>,
    /// MemFs inode → offsets written, to compare large sparse files.
    hot: BTreeMap<Ino, BTreeSet<u64>>,
    step: usize,
    op: String,
    /// A power cut happened: the filesystem stays not clean (until
    /// e2fsck), as on Linux.
    power_cut: bool,
}

impl Model {
    fn new(seed: u64, bs: u32, cache: usize, ops: usize) -> Model {
        let img = mkfs(
            &format!("model-{bs}-{cache}"),
            bs,
            48 << 10,
            &["-E", "root_owner=0:0"],
            None,
        );
        let env = TestEnv::new();
        let mut m = Model {
            seed,
            bs,
            cache,
            ops,
            img,
            env: env.clone(),
            ext: None,
            mem: MemFs::new(Box::new(env)),
            rng: Rng::new(seed),
            dirs: Vec::new(),
            files: Vec::new(),
            hot: BTreeMap::new(),
            step: 0,
            op: String::from("setup"),
            power_cut: false,
        };
        m.mount();
        // MemFs has no lost+found.
        let root = m.ext().root();
        m.ext().rmdir(root, b"lost+found").unwrap();
        m.refresh();
        m
    }

    fn mount(&mut self) {
        let opts = MountOptions {
            read_only: false,
            cache_bytes: self.cache,
        };
        let dev = FileDisk::open(&self.img);
        match Ext2::mount(dev, Box::new(self.env.clone()), opts) {
            Ok(fs) => self.ext = Some(fs),
            Err(e) => self.fail(format!("mount: {e:?}")),
        }
    }

    fn ext(&mut self) -> &mut Ext2<FileDisk> {
        self.ext.as_mut().expect("mounted")
    }

    fn fail(&self, what: String) -> ! {
        panic!(
            "model test failed: {what}\n  seed {}, block size {}, cache {} bytes, op #{}: {}\n  \
             log: {:?}\n  replay: RELAY_MODEL_SEED={} RELAY_MODEL_BS={} RELAY_MODEL_CACHE={} \
             RELAY_MODEL_OPS={} cargo test -p ext2 --test model -- --nocapture",
            self.seed,
            self.bs,
            self.cache,
            self.step,
            self.op,
            self.env.lines(),
            self.seed,
            self.bs,
            self.cache,
            self.step + 1
        );
    }

    fn same<T: PartialEq + fmt::Debug>(&self, what: &str, ext: &T, mem: &T) {
        if ext != mem {
            self.fail(format!("{what}: ext2 {ext:?}, MemFs {mem:?}"));
        }
    }

    /// The same error, or both `Ok`.
    fn same_outcome<T, U>(&self, ext: &Result<T, Errno>, mem: &Result<U, Errno>) {
        self.same("result", &ext.as_ref().err(), &mem.as_ref().err());
    }

    fn name(&mut self) -> Name {
        let r = self.rng.below(100);
        Name(match r {
            0 => Vec::new(),
            1 => b"a/b".to_vec(),
            2 => vec![b'x'; 256],
            3 => b"..".to_vec(),
            4..=9 => {
                let len = 1 + self.rng.below(255) as usize;
                (0..len).map(|_| b'a' + self.rng.below(26) as u8).collect()
            }
            _ => POOL[self.rng.below(POOL.len() as u64) as usize]
                .as_bytes()
                .to_vec(),
        })
    }

    /// A name that exists in `dir` most of the time, of a directory if
    /// `dirs` says so.
    fn child_name(&mut self, dir: Pair, dirs: Option<bool>) -> Name {
        if self.rng.chance(75)
            && let Ok(entries) = self.mem.read_dir(dir.0)
        {
            let names: Vec<_> = entries
                .into_iter()
                .filter(|e| e.name != b"." && e.name != b"..")
                .filter(|e| {
                    let is_dir = self.mem.stat(e.ino).map(|s| s.kind) == Ok(FileType::Directory);
                    dirs.is_none_or(|d| d == is_dir)
                })
                .map(|e| e.name)
                .collect();
            if !names.is_empty() {
                return Name(names[self.rng.below(names.len() as u64) as usize].clone());
            }
        }
        self.name()
    }

    fn pick(&mut self, from_files: bool) -> Pair {
        let pool = if from_files && !self.files.is_empty() {
            &self.files
        } else {
            &self.dirs
        };
        pool[self.rng.below(pool.len() as u64) as usize]
    }

    fn dir(&mut self) -> Pair {
        self.pick(false)
    }

    /// Mostly a file, sometimes a directory (for the type errors).
    fn node(&mut self) -> Pair {
        let file = self.rng.chance(90);
        self.pick(file)
    }

    /// Mostly a directory, sometimes a file (for the type errors).
    fn mostly_dir(&mut self) -> Pair {
        let file = self.rng.chance(10);
        self.pick(file)
    }

    /// Where the triple indirect range starts.
    fn triple(&self) -> u64 {
        let p = self.bs as u64 / 4;
        (12 + p + p * p) * self.bs as u64
    }

    fn offset(&mut self, size: u64) -> u64 {
        match self.rng.below(100) {
            0..=4 => self.triple() - (1 << 20) + self.rng.below(8 << 20),
            5..=14 => self.rng.below(1 << 20),
            15..=49 => size.saturating_sub(self.rng.below(5000)) + self.rng.below(100),
            _ => self.rng.below(size + 1 + 2 * self.bs as u64),
        }
    }

    fn len(&mut self) -> usize {
        if self.rng.chance(10) {
            self.rng.below(64 << 10) as usize
        } else {
            self.rng.below(3 * self.bs as u64) as usize
        }
    }

    fn size_of(&mut self, p: Pair) -> u64 {
        self.mem.stat(p.0).map(|s| s.size).unwrap_or(0)
    }

    fn next_op(&mut self) -> Op {
        match self.rng.below(100) {
            0..=11 => {
                let d = self.dir();
                Op::Create(d, self.name())
            }
            12..=17 => {
                let d = self.dir();
                Op::Mkdir(d, self.name())
            }
            18..=37 => {
                let f = self.node();
                let size = self.size_of(f);
                Op::Write(f, self.offset(size), self.len())
            }
            38..=44 => {
                let f = self.node();
                let size = self.size_of(f);
                let new = match self.rng.below(4) {
                    0 => 0,
                    1 => self.offset(size),
                    _ => self.rng.below(size + 1),
                };
                Op::Truncate(f, new)
            }
            45..=54 => {
                let f = self.node();
                let size = self.size_of(f);
                Op::Read(f, self.offset(size), self.len())
            }
            55..=66 => {
                let (a, b) = (self.mostly_dir(), self.mostly_dir());
                let from = self.child_name(a, None);
                let to = if self.rng.chance(40) {
                    self.child_name(b, None)
                } else {
                    self.name()
                };
                Op::Rename(a, from, b, to)
            }
            67..=74 => {
                let d = self.mostly_dir();
                let files = self.rng.chance(85);
                Op::Unlink(d, self.child_name(d, Some(!files)))
            }
            75..=80 => {
                let d = self.mostly_dir();
                let dirs = self.rng.chance(85);
                Op::Rmdir(d, self.child_name(d, Some(dirs)))
            }
            81..=83 => Op::Touch(self.node()),
            84..=89 => {
                let d = self.mostly_dir();
                let name = match self.rng.below(5) {
                    0 => Name(b".".to_vec()),
                    1 => Name(b"..".to_vec()),
                    _ => self.child_name(d, None),
                };
                Op::Lookup(d, name)
            }
            90..=93 => Op::ReadDir(self.mostly_dir()),
            94..=97 => Op::Stat(self.node()),
            98 => Op::SyncRemount,
            _ => Op::ShutdownRemount,
        }
    }

    fn apply(&mut self, op: &Op) {
        let changed = match *op {
            Op::Create(d, ref n) => {
                let e = self.ext().create(d.1, &n.0);
                let m = self.mem.create(d.0, &n.0);
                self.same_outcome(&e, &m);
                e.is_ok()
            }
            Op::Mkdir(d, ref n) => {
                let e = self.ext().mkdir(d.1, &n.0);
                let m = self.mem.mkdir(d.0, &n.0);
                self.same_outcome(&e, &m);
                e.is_ok()
            }
            Op::Write(f, offset, len) => {
                let data = self.rng.bytes(len);
                let e = self.ext().write_at(f.1, offset, &data);
                let m = self.mem.write_at(f.0, offset, &data);
                self.same("write", &e, &m);
                if e.is_ok() {
                    self.hot.entry(f.0).or_default().insert(offset);
                }
                false
            }
            Op::Truncate(f, size) => {
                let e = self.ext().truncate(f.1, size);
                let m = self.mem.truncate(f.0, size);
                self.same("truncate", &e, &m);
                if e.is_ok() {
                    self.hot.entry(f.0).or_default().insert(size);
                }
                false
            }
            Op::Read(f, offset, len) => {
                let (e, m) = self.read_both(f, offset, len);
                self.same("read", &e, &m);
                false
            }
            Op::Rename(a, ref from, b, ref to) => {
                let e = self.ext().rename(a.1, &from.0, b.1, &to.0);
                let m = self.mem.rename(a.0, &from.0, b.0, &to.0);
                self.same("rename", &e, &m);
                e.is_ok()
            }
            Op::Unlink(d, ref n) | Op::Rmdir(d, ref n) => {
                let unlink = matches!(op, Op::Unlink(..));
                let gone = self.pair_of(d, &n.0);
                let (e, m) = if unlink {
                    (self.ext().unlink(d.1, &n.0), self.mem.unlink(d.0, &n.0))
                } else {
                    (self.ext().rmdir(d.1, &n.0), self.mem.rmdir(d.0, &n.0))
                };
                self.same("remove", &e, &m);
                // A stale number reaches nothing (before ext2 reuses it).
                if let (Ok(()), Some(p)) = (&e, gone) {
                    let e = self.ext().stat(p.1).err();
                    self.same("stale stat", &e, &Some(Errno::ENOENT));
                }
                e.is_ok()
            }
            Op::Touch(p) => {
                let e = self.ext().touch(p.1);
                let m = self.mem.touch(p.0);
                self.same("touch", &e, &m);
                false
            }
            Op::Lookup(d, ref n) => {
                let e = self.ext().lookup(d.1, &n.0);
                let m = self.mem.lookup(d.0, &n.0);
                self.same_outcome(&e, &m);
                if let (Ok(e), Ok(m)) = (e, m)
                    && !self.dirs.contains(&(m, e))
                    && !self.files.contains(&(m, e))
                {
                    self.fail(format!(
                        "lookup gave ext2 {e}, MemFs {m}: not the same node"
                    ));
                }
                false
            }
            Op::ReadDir(d) => {
                let names = |r: Result<Vec<vfs::DirEntry>, Errno>| {
                    r.map(|v| v.into_iter().map(|e| e.name).collect::<BTreeSet<_>>())
                };
                let e = names(self.ext().read_dir(d.1));
                let m = names(self.mem.read_dir(d.0));
                self.same("read_dir names", &e, &m);
                false
            }
            Op::Stat(p) => {
                self.compare_stat(p);
                false
            }
            Op::SyncRemount => {
                self.sync();
                drop(self.ext.take());
                self.power_cut = true;
                self.fsck();
                self.mount();
                false
            }
            Op::ShutdownRemount => {
                if let Err(e) = self.ext().shutdown() {
                    self.fail(format!("shutdown: {e:?}"));
                }
                let root = self.ext().root();
                let e = self.ext().create(root, b"after-shutdown").err();
                self.same("create after shutdown", &e, &Some(Errno::EROFS));
                drop(self.ext.take());
                self.fsck();
                let state = if self.power_cut { "not clean" } else { "clean" };
                self.same(
                    "state",
                    &field(&dumpe2fs_h(&self.img), "Filesystem state"),
                    &state.to_string(),
                );
                self.mount();
                false
            }
        };
        if changed {
            self.refresh();
        }
    }

    /// The pair `dir/name` names, if any.
    fn pair_of(&mut self, dir: Pair, name: &[u8]) -> Option<Pair> {
        let m = self.mem.lookup(dir.0, name).ok()?;
        self.dirs
            .iter()
            .chain(&self.files)
            .copied()
            .find(|p| p.0 == m)
    }

    fn read_both(
        &mut self,
        f: Pair,
        offset: u64,
        len: usize,
    ) -> (Result<Vec<u8>, Errno>, Result<Vec<u8>, Errno>) {
        let mut a = vec![0; len];
        let mut b = vec![0; len];
        let e = self
            .ext()
            .read_at(f.1, offset, &mut a)
            .map(|n| a[..n].to_vec());
        let m = self
            .mem
            .read_at(f.0, offset, &mut b)
            .map(|n| b[..n].to_vec());
        (e, m)
    }

    /// What `stat` must agree on: not the inode number, block counts,
    /// times, nor the sizes of directories.
    fn comparable(st: Stat) -> (FileType, u16, u32, u32, u32, Option<u64>) {
        let size = (st.kind == FileType::Regular).then_some(st.size);
        (st.kind, st.perm, st.nlink, st.uid, st.gid, size)
    }

    fn compare_stat(&mut self, p: Pair) {
        let e = self.ext().stat(p.1).map(Model::comparable);
        let m = self.mem.stat(p.0).map(Model::comparable);
        self.same("stat", &e, &m);
    }

    /// Walks both trees, checking that they hold the same names, and
    /// relearns every node's pair of inode numbers.
    fn refresh(&mut self) {
        self.dirs.clear();
        self.files.clear();
        let root = (self.mem.root(), self.ext().root());
        let mut todo = vec![root];
        while let Some(d) = todo.pop() {
            self.dirs.push(d);
            let e = self.ext().read_dir(d.1);
            let m = self.mem.read_dir(d.0);
            let (Ok(e), Ok(m)) = (e, m) else {
                self.fail(format!("read_dir of {d:?}"));
            };
            let e: BTreeMap<Vec<u8>, Ino> = e.into_iter().map(|x| (x.name, x.ino)).collect();
            let m: BTreeMap<Vec<u8>, Ino> = m.into_iter().map(|x| (x.name, x.ino)).collect();
            let names =
                |t: &BTreeMap<Vec<u8>, Ino>| t.keys().map(|k| Name(k.clone())).collect::<Vec<_>>();
            if e.keys().ne(m.keys()) {
                self.fail(format!(
                    "directory {d:?}: ext2 {:?}, MemFs {:?}",
                    names(&e),
                    names(&m)
                ));
            }
            for (name, &mi) in &m {
                if name == b"." || name == b".." {
                    continue;
                }
                let pair = (mi, e[name]);
                match self.mem.stat(mi).map(|s| s.kind) {
                    Ok(FileType::Directory) => todo.push(pair),
                    _ => self.files.push(pair),
                }
            }
        }
    }

    /// Compares every node's attributes and every file's contents.
    fn compare_trees(&mut self) {
        self.refresh();
        let nodes: Vec<Pair> = self.dirs.iter().chain(&self.files).copied().collect();
        for p in nodes {
            self.compare_stat(p);
        }
        for f in self.files.clone() {
            let size = self.size_of(f);
            let mut windows = vec![(0, size.min(WHOLE))];
            if size > WHOLE {
                windows.push((size - (64 << 10), size));
                for &o in self.hot.get(&f.0).into_iter().flatten() {
                    let start = o.saturating_sub(32 << 10).min(size);
                    windows.push((start, (start + (96 << 10)).min(size)));
                }
            }
            for (start, end) in windows {
                let (e, m) = self.read_both(f, start, (end - start) as usize);
                if e != m {
                    self.fail(format!("contents of {f:?} in {start}..{end} differ"));
                }
            }
        }
    }

    fn sync(&mut self) {
        if let Err(e) = self.ext().sync() {
            self.fail(format!("sync: {e:?}"));
        }
    }

    /// e2fsck: clean, apart from the not-clean state after a power cut.
    fn fsck(&self) {
        let out = tool("e2fsck").arg("-fn").arg(&self.img).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        if !out.status.success() || text.contains("? no") {
            self.fail(format!("e2fsck -fn ({}):\n{text}", out.status));
        }
    }

    fn run(mut self) {
        while self.step < self.ops {
            self.env.set_now(T0 + self.step as u64);
            let op = self.next_op();
            self.op = format!("{op:?}");
            self.apply(&op);
            self.step += 1;
            if self.step.is_multiple_of(BATCH) || self.step == self.ops {
                self.op = format!("batch check after op #{}", self.step - 1);
                self.compare_trees();
                self.sync();
                self.fsck();
            }
        }
        if let Err(e) = self.ext().shutdown() {
            self.fail(format!("shutdown: {e:?}"));
        }
        self.fsck();
    }
}

/// One run, unless the environment asks for another one.
fn run(bs: u32, cache: usize, seed: u64) {
    if env_u64("RELAY_MODEL_BS").is_some_and(|b| b != bs as u64)
        || env_u64("RELAY_MODEL_CACHE").is_some_and(|c| c != cache as u64)
    {
        return;
    }
    let seed = env_u64("RELAY_MODEL_SEED").unwrap_or(seed);
    let ops = env_u64("RELAY_MODEL_OPS").map_or(OPS, |n| n as usize);
    Model::new(seed, bs, cache, ops).run();
}

const TINY: usize = 64 << 10;

#[test]
fn model_1k() {
    run(1024, ext2::CACHE_BYTES, 1);
}

#[test]
fn model_2k() {
    run(2048, ext2::CACHE_BYTES, 2);
}

#[test]
fn model_4k() {
    run(4096, ext2::CACHE_BYTES, 3);
}

#[test]
fn model_1k_tiny_cache() {
    run(1024, TINY, 4);
}

#[test]
fn model_2k_tiny_cache() {
    run(2048, TINY, 5);
}

#[test]
fn model_4k_tiny_cache() {
    run(4096, TINY, 6);
}
