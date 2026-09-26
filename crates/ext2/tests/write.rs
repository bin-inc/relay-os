//! Writing and truncating files made by `mke2fs -d`, with a `MemFs` as the
//! model of what each file holds. After every scenario: e2fsck clean, and
//! debugfs sees the same sizes, block counts and contents.

mod common;

use common::*;
use ext2::{Ext2, MountOptions};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use vfs::{Errno, FileSystem, Ino, MemFs};

fn pattern(len: usize, seed: u64) -> Vec<u8> {
    Rng::new(seed).bytes(len)
}

/// Files larger than this are checked block by block where they were
/// written, rather than dumped whole.
const DUMP_LIMIT: u64 = 32 << 20;

struct File {
    ino: Ino,
    model: Ino,
    /// Every range written, to check the large files block by block.
    written: Vec<(u64, usize)>,
}

/// An image and a `MemFs` holding the same files; every change goes to
/// both.
struct Pair {
    img: PathBuf,
    bs: u64,
    fs: Ext2<FileDisk>,
    env: Rc<TestEnv>,
    faults: Rc<Faults>,
    model: MemFs,
    files: BTreeMap<&'static str, File>,
}

/// What the image's files hold at first.
fn contents() -> [(&'static str, Vec<u8>); 4] {
    [
        ("f", b"hello, world\n".to_vec()),
        ("big", pattern(20_000, 1)),
        ("e", Vec::new()),
        ("fill", Vec::new()),
    ]
}

/// An image holding [`contents`], a symlink and a device node.
fn image(name: &str, bs: u32, size_kib: u64, extra: &[&str]) -> PathBuf {
    let dir = staging(name);
    for (file, data) in contents() {
        fs::write(dir.join(file), data).unwrap();
    }
    let img = mkfs(name, bs, size_kib, extra, Some(&dir));
    debugfs_w(&img, "symlink link f");
    debugfs_w(&img, "mknod null c 1 3");
    img
}

impl Pair {
    fn new(name: &str, bs: u32, size_kib: u64, extra: &[&str], opts: MountOptions) -> Pair {
        Pair::open(image(name, bs, size_kib, extra), bs, opts)
    }

    fn open(img: PathBuf, bs: u32, opts: MountOptions) -> Pair {
        let dev = FileDisk::open(&img);
        let faults = dev.faults.clone();
        let env = TestEnv::new();
        let mut fs = Ext2::mount(dev, Box::new(env.clone()), opts).unwrap();
        let mut model = MemFs::new(Box::new(env.clone()));
        let mut files = BTreeMap::new();
        for (file, data) in contents() {
            let ino = fs.lookup(fs.root(), file.as_bytes()).unwrap();
            let m = model.create(model.root(), file.as_bytes()).unwrap();
            model.write_at(m, 0, &data).unwrap();
            let written = vec![(0, data.len())];
            files.insert(
                file,
                File {
                    ino,
                    model: m,
                    written,
                },
            );
        }
        Pair {
            img,
            bs: bs as u64,
            fs,
            env,
            faults,
            model,
            files,
        }
    }

    fn ino(&self, name: &str) -> Ino {
        self.files[name].ino
    }

    /// Writes through ext2, then what it wrote to the model.
    fn write(&mut self, name: &str, offset: u64, data: &[u8]) -> Result<usize, Errno> {
        let f = self.files.get_mut(name).unwrap();
        let got = self.fs.write_at(f.ino, offset, data);
        if let Ok(n) = got {
            self.model.write_at(f.model, offset, &data[..n]).unwrap();
            f.written.push((offset, n));
        }
        got
    }

    fn truncate(&mut self, name: &str, size: u64) {
        let f = &self.files[name];
        self.fs.truncate(f.ino, size).unwrap();
        self.model.truncate(f.model, size).unwrap();
    }

    fn read_model(&mut self, name: &str, offset: u64, len: usize) -> Vec<u8> {
        let mut buf = vec![0; len];
        let n = self
            .model
            .read_at(self.files[name].model, offset, &mut buf)
            .unwrap();
        buf.truncate(n);
        buf
    }

    fn read_ext2(&mut self, name: &str, offset: u64, len: usize) -> Vec<u8> {
        let mut buf = vec![0; len];
        let n = self
            .fs
            .read_at(self.files[name].ino, offset, &mut buf)
            .unwrap();
        buf.truncate(n);
        buf
    }

    fn free_blocks(&mut self) -> u64 {
        self.fs.statfs().unwrap().free_blocks
    }

    /// Syncs, then checks the image with e2fsck and every file with
    /// debugfs against the model.
    fn verify(&mut self) {
        self.fs.sync().unwrap();
        fsck(&self.img);
        assert_eq!(self.free_blocks(), sb_field(&self.img, "Free blocks"));
        let names: Vec<&str> = self.files.keys().copied().collect();
        for name in names {
            let st = self.fs.stat(self.ino(name)).unwrap();
            let size = self.model.stat(self.files[name].model).unwrap().size;
            assert_eq!(st.size, size, "{name}");
            let d = debugfs_stat(&self.img, name);
            assert_eq!((d.size, d.blockcount), (st.size, st.blocks), "{name}");
            if size <= DUMP_LIMIT {
                let host = scratch("write-dump", ".bin");
                debugfs(&self.img, &format!("dump {name} {}", host.display()));
                let want = self.read_model(name, 0, size as usize);
                assert!(
                    fs::read(&host).unwrap() == want,
                    "{name}: debugfs dump differs"
                );
                assert!(
                    self.read_ext2(name, 0, size as usize) == want,
                    "{name}: read differs"
                );
            } else {
                self.verify_written(name, size);
            }
        }
    }

    /// Checks the blocks a large file was written to, reading them from
    /// the image where debugfs maps them.
    fn verify_written(&mut self, name: &str, size: u64) {
        let bs = self.bs;
        let mut lbs: Vec<u64> = Vec::new();
        for &(offset, len) in &self.files[name].written {
            if len > 0 {
                lbs.extend(offset / bs..=(offset + len as u64 - 1) / bs);
            }
        }
        lbs.retain(|&lb| lb * bs < size);
        lbs.sort();
        lbs.dedup();
        let phys = bmap(&self.img, name, &lbs);
        for (&lb, &p) in lbs.iter().zip(&phys) {
            let mut want = self.read_model(name, lb * bs, bs as usize);
            want.resize(bs as usize, 0);
            let on_disk = if p == 0 {
                vec![0; bs as usize]
            } else {
                peek(&self.img, p * bs, bs as usize)
            };
            let len = (size - lb * bs).min(bs) as usize;
            assert!(on_disk[..len] == want[..len], "{name}: block {lb}");
            assert!(
                self.read_ext2(name, lb * bs, len) == want[..len],
                "{name}: block {lb}"
            );
        }
    }

    fn shutdown(mut self) {
        self.fs.shutdown().unwrap();
        fsck(&self.img);
        assert_eq!(field(&dumpe2fs_h(&self.img), "Filesystem state"), "clean");
    }
}

#[test]
fn overwrite_append_and_holes() {
    for bs in [1024u32, 4096] {
        let mut p = Pair::new("write-basic", bs, 16 << 10, &[], rw());
        let bs = bs as u64;
        let blocks = p.fs.stat(p.ino("f")).unwrap().blocks;
        assert_eq!(p.write("f", 0, b"HELLO"), Ok(5));
        assert_eq!(p.fs.stat(p.ino("f")).unwrap().blocks, blocks, "in place");
        assert_eq!(p.write("f", 13, b"appended\n"), Ok(9));
        assert_eq!(
            p.write("big", 5000, &pattern(3 * bs as usize, 2)),
            Ok(3 * bs as usize)
        );
        assert_eq!(p.write("big", 19_990, &pattern(100, 3)), Ok(100));
        let offset = 3 * bs + 10;
        assert_eq!(p.write("e", offset, b"after a hole"), Ok(12));
        let st = p.fs.stat(p.ino("e")).unwrap();
        assert_eq!((st.size, st.blocks), (offset + 12, bs / 512));
        assert_eq!(p.read_ext2("e", 0, 20), vec![0; 20]);
        p.verify();
        p.shutdown();
    }
}

#[test]
fn files_grow_through_every_level_of_indirection() {
    for bs in [1024u32, 4096] {
        let mut p = Pair::new("write-levels", bs, 16 << 10, &[], rw());
        let bs = bs as u64;
        let per = bs / 4;
        let single = 12 * bs;
        let double = single + per * bs;
        let triple = double + per * per * bs;
        // One write across the start of the single and double indirect
        // ranges.
        let span = (double + 3 * bs) as usize;
        if span <= 1 << 20 {
            assert_eq!(p.write("big", 0, &pattern(span, 4)), Ok(span));
        }
        for (i, start) in [single, double, triple].into_iter().enumerate() {
            let data = pattern(2 * bs as usize, 10 + i as u64);
            assert_eq!(p.write("e", start - bs - 7, &data), Ok(data.len()));
        }
        let far = if bs == 1024 { 70 << 20 } else { 5 << 30 };
        assert_eq!(p.write("fill", far, b"far away"), Ok(8));
        p.verify();
        p.shutdown();
    }
}

#[test]
fn truncate_frees_blocks_and_grows_with_holes() {
    for bs in [1024u32, 4096] {
        let mut p = Pair::new("write-truncate", bs, 16 << 10, &[], rw());
        let bs = bs as u64;
        let start = p.free_blocks();
        let (dense, far) = if bs == 1024 {
            (300 << 10, 70 << 20)
        } else {
            (5 << 20, 5 << 30)
        };
        p.write("e", 0, &pattern(dense, 5)).unwrap();
        p.write("e", far, b"tail").unwrap();
        p.verify();
        // Into the double indirect range: the triple indirect tree goes.
        let double = (12 + bs / 4) * bs;
        p.truncate("e", double + 5 * bs + 100);
        p.verify();
        // Into the single indirect range, mid-block.
        p.truncate("e", 13 * bs + 5);
        p.verify();
        assert_eq!(
            p.fs.stat(p.ino("e")).unwrap().blocks,
            15 * bs / 512,
            "14 + indirect"
        );
        // Inside the first block.
        p.truncate("e", bs / 2);
        p.verify();
        p.truncate("e", 0);
        assert_eq!(p.fs.stat(p.ino("e")).unwrap().blocks, 0);
        assert_eq!(p.free_blocks(), start);
        // Growing makes a hole.
        p.truncate("e", 10 * bs + 3);
        assert_eq!(p.read_ext2("e", 0, 100), vec![0; 100]);
        assert_eq!(p.fs.stat(p.ino("e")).unwrap().blocks, 0);
        p.truncate("big", 20_000);
        p.verify();
        assert_eq!(p.free_blocks(), start);
        p.shutdown();
    }
}

#[test]
fn data_past_the_end_never_reappears() {
    for bs in [1024u32, 4096] {
        let mut p = Pair::new("write-tail", bs, 8 << 10, &[], rw());
        let bs = bs as usize;
        p.write("e", 0, &[0xAA; 100]).unwrap();
        p.truncate("e", 10);
        p.truncate("e", 100);
        assert_eq!(p.read_ext2("e", 10, 90), vec![0; 90]);
        p.write("f", 0, &[0xBB; 100]).unwrap();
        p.truncate("f", 10);
        p.write("f", 50, b"x").unwrap();
        assert_eq!(p.read_ext2("f", 10, 40), vec![0; 40]);
        p.write("fill", 0, &vec![0xCC; bs]).unwrap();
        p.truncate("fill", bs as u64 / 2);
        p.write("fill", 2 * bs as u64, b"y").unwrap();
        assert_eq!(p.read_ext2("fill", bs as u64 / 2, bs / 2), vec![0; bs / 2]);
        p.verify();
        p.shutdown();
    }
}

#[test]
fn a_file_past_2_gib_turns_on_large_file() {
    // mke2fs always turns large_file on; debugfs can turn it off.
    let img = image("write-large", 4096, 16 << 10, &[]);
    debugfs_w(&img, "feature -large_file");
    let mut p = Pair::open(img, 4096, rw());
    let features = |p: &Pair| field(&dumpe2fs_h(&p.img), "Filesystem features");
    assert!(!features(&p).contains("large_file"));
    p.write("e", 3 << 30, b"big").unwrap();
    p.verify();
    assert!(features(&p).contains("large_file"));
    p.shutdown();
}

#[test]
fn a_full_filesystem_writes_what_fits_then_reports_enospc() {
    for bs in [1024u32, 4096] {
        let kib = if bs == 1024 { 1024 } else { 4096 };
        let mut p = Pair::new("write-full", bs, kib, &[], rw());
        let bs = bs as u64;
        let data = pattern(8 << 20, 6);
        let n = p.write("fill", 0, &data).unwrap();
        assert!(n > 0 && n < data.len(), "{n}");
        assert_eq!(n as u64 % bs, 0);
        assert_eq!(p.free_blocks(), 0);
        assert_eq!(p.write("fill", n as u64, b"more"), Err(Errno::ENOSPC));
        assert_eq!(p.write("e", 0, b"x"), Err(Errno::ENOSPC));
        // Overwriting needs no space.
        assert_eq!(p.write("fill", 0, b"still fits"), Ok(10));
        p.verify();
        // With the last block freed, a write whose data block needs new
        // indirect blocks too fails before allocating anything.
        p.truncate("fill", n as u64 - bs);
        let free = p.free_blocks();
        assert!((1..=2).contains(&free));
        let triple = (12 + bs / 4 + (bs / 4) * (bs / 4)) * bs;
        assert_eq!(p.write("fill", triple, b"z"), Err(Errno::ENOSPC));
        assert_eq!(p.free_blocks(), free);
        assert_eq!(
            p.write("fill", n as u64 - bs, &data[..bs as usize]),
            Ok(bs as usize)
        );
        p.verify();
        p.truncate("fill", 0);
        p.verify();
        p.shutdown();
    }
}

#[test]
fn a_small_cache_writes_dirty_blocks_early() {
    let opts = MountOptions {
        read_only: false,
        cache_bytes: 32 << 10,
    };
    let mut p = Pair::new("write-early", 1024, 8 << 10, &[], opts);
    let before = p.faults.writes.get();
    p.write("e", 0, &pattern(1 << 20, 7)).unwrap();
    assert!(
        p.faults.writes.get() > before + 100,
        "written before any sync"
    );
    p.verify();
    p.shutdown();
}

#[test]
fn a_failing_device_fails_sync_until_it_heals() {
    let mut p = Pair::new("write-broken", 1024, 8 << 10, &[], rw());
    p.write("e", 0, &pattern(50_000, 8)).unwrap();
    p.faults.fail_writes.set(true);
    assert_eq!(p.fs.sync(), Err(Errno::EIO));
    assert!(p.env.logged("ext2: write error"), "{:?}", p.env.lines());
    assert_eq!(
        p.read_ext2("e", 0, 10),
        p.read_model("e", 0, 10),
        "still cached"
    );
    assert_eq!(p.fs.sync(), Err(Errno::EIO));
    p.faults.fail_writes.set(false);
    p.verify();
    p.shutdown();
}

#[test]
fn data_changes_follow_the_contract() {
    let mut p = Pair::new("write-contract", 1024, 16 << 10, &[], rw());
    let root = p.fs.root();
    let link = p.fs.lookup(root, b"link").unwrap();
    let null = p.fs.lookup(root, b"null").unwrap();
    let f = p.ino("f");
    for (ino, err) in [
        (root, Errno::EISDIR),
        (link, Errno::EINVAL),
        (null, Errno::EINVAL),
    ] {
        assert_eq!(p.fs.write_at(ino, 0, b"x"), Err(err));
        assert_eq!(p.fs.write_at(ino, 0, b""), Err(err));
        assert_eq!(p.fs.truncate(ino, 0), Err(err));
    }
    assert_eq!(p.fs.write_at(500, 0, b"x"), Err(Errno::ENOENT));
    assert_eq!(p.fs.truncate(500, 0), Err(Errno::ENOENT));
    assert_eq!(p.fs.touch(500), Err(Errno::ENOENT));
    // The largest file, as Linux computes it for 1 KiB blocks.
    let max = 16_843_020 * 1024;
    assert_eq!(p.fs.write_at(f, max, b"x"), Err(Errno::EFBIG));
    assert_eq!(p.fs.write_at(f, u64::MAX, b"x"), Err(Errno::EFBIG));
    assert_eq!(p.fs.write_at(f, u64::MAX, b""), Ok(0));
    assert_eq!(p.fs.truncate(f, max + 1), Err(Errno::EFBIG));
    assert_eq!(p.write("e", max - 1, b"x"), Ok(1));
    // Times: data changes set mtime and ctime, never atime.
    let atime = p.fs.stat(f).unwrap().atime;
    p.env.set_now(T0 + 100);
    p.write("f", 0, b"J").unwrap();
    let st = p.fs.stat(f).unwrap();
    assert_eq!((st.atime, st.mtime, st.ctime), (atime, T0 + 100, T0 + 100));
    p.env.set_now(T0 + 200);
    p.fs.touch(f).unwrap();
    assert_eq!(p.fs.stat(f).unwrap().mtime, T0 + 200);
    p.env.set_now(T0 + 300);
    p.truncate("f", 3);
    assert_eq!(p.fs.stat(f).unwrap().ctime, T0 + 300);
    p.fs.touch(root).unwrap();
    p.verify();
    let img = p.img.clone();
    p.shutdown();
    let (mut fs, _) = mount(&img, ro());
    assert_eq!(fs.write_at(f, 0, b"x"), Err(Errno::EROFS));
    assert_eq!(
        fs.write_at(root, 0, b"x"),
        Err(Errno::EROFS),
        "EROFS before EISDIR"
    );
    assert_eq!(fs.truncate(f, 0), Err(Errno::EROFS));
    assert_eq!(fs.touch(f), Err(Errno::EROFS));
    assert_eq!(fs.touch(500), Err(Errno::ENOENT), "ENOENT before EROFS");
}

/// Group 0's block bitmap and inode table (1 KiB blocks: the descriptor
/// table is at byte 2048), and the bytes of both but inode `skip`'s, to
/// show they survive.
fn metadata_snapshot(img: &Path, skip: u64) -> (u32, u32, Vec<u8>) {
    let d = peek(img, 2048, 12);
    let at = |i: usize| u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
    let (bitmap, table) = (at(0), at(8));
    let mut inodes = peek(img, table as u64 * 1024, 8 * 1024);
    let own = (skip as usize - 1) * 256;
    inodes[own..own + 256].fill(0);
    let mut bytes = peek(img, bitmap as u64 * 1024, 1024);
    bytes.extend(inodes);
    (bitmap, table, bytes)
}

#[test]
fn pointers_into_metadata_are_never_written_or_freed() {
    // (the pointer set on `e`, and the metadata it aims at)
    for (field, target) in [
        ("block[0]", "table"),
        ("block[IND]", "table"),
        ("block[IND]", "bitmap"),
    ] {
        let img = image("write-meta", 1024, 4 << 10, &[]);
        let e = ino_of(&img, "e");
        let (bitmap, table, before) = metadata_snapshot(&img, e);
        let aim = if target == "table" { table } else { bitmap };
        debugfs_w(&img, &format!("set_inode_field e {field} {aim}"));
        debugfs_w(&img, "set_inode_field e size 20480");
        let mut p = Pair::open(img.clone(), 1024, rw());
        let offset = if field == "block[0]" { 0 } else { 13 * 1024 };
        assert_eq!(p.fs.write_at(e, offset, b"x"), Err(Errno::EIO), "{field}");
        assert!(p.env.logged(&format!("block pointer {aim} into metadata")));
        p.fs.truncate(e, 0).unwrap();
        assert!(
            p.env
                .logged(&format!("{aim}: outside the filesystem or metadata"))
        );
        p.fs.sync().unwrap();
        assert!(
            metadata_snapshot(&img, e).2 == before,
            "{field} → {target}: metadata changed"
        );
        let root = p.fs.root();
        assert!(p.fs.stat(root).is_ok());
        assert!(p.fs.lookup(root, b"f").is_ok());
        fsck(&img);
    }
}

#[test]
fn a_failed_shutdown_loses_no_data() {
    let mut p = Pair::new("write-shutdown", 1024, 8 << 10, &[], rw());
    let data = pattern(30_000, 11);
    p.write("e", 0, &data).unwrap();
    p.faults.fail_writes.set(true);
    assert_eq!(p.fs.shutdown(), Err(Errno::EIO));
    p.faults.fail_writes.set(false);
    p.fs.sync().unwrap();
    fsck(&p.img);
    assert_eq!(field(&dumpe2fs_h(&p.img), "Filesystem state"), "clean");
    let host = scratch("write-shutdown-dump", ".bin");
    debugfs(&p.img, &format!("dump e {}", host.display()));
    assert!(
        fs::read(&host).unwrap() == data,
        "the data reached the disk"
    );
}
