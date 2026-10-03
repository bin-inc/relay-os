//! Read-write mounting: the clean flag, the mount count, backup
//! superblocks, and what `sync` and `shutdown` write (spec §8.2).

mod common;

use common::*;
use ext2::Ext2;
use std::fs;
use std::path::Path;
use vfs::{Errno, FileSystem};

fn state(img: &Path) -> String {
    field(&dumpe2fs_h(img), "Filesystem state")
}

/// The block numbers of the backup superblocks, from `dumpe2fs`.
fn backups(img: &Path) -> Vec<u64> {
    dumpe2fs(img)
        .split("Backup superblock at ")
        .skip(1)
        .map(|s| s.split(',').next().unwrap().trim().parse().unwrap())
        .collect()
}

/// `dumpe2fs -h` of the backup superblock at block `block`.
fn backup_header(img: &Path, block: u64, bs: u32) -> String {
    let out = tool("dumpe2fs")
        .arg("-o")
        .arg(format!("superblock={block}"))
        .arg("-o")
        .arg(format!("blocksize={bs}"))
        .arg("-h")
        .arg(img)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_read_write_mount_is_not_clean_until_shutdown() {
    for bs in [1024, 4096] {
        let img = mkfs("clean-state", bs, 8 << 10, &[], None);
        assert_eq!(state(&img), "clean");
        let (mut fs, env) = mount(&img, rw());
        assert!(!fs.is_read_only());
        assert!(!fs.statfs().unwrap().read_only);
        // Mounting itself wrote the superblock and flushed.
        assert_eq!(state(&img), "not clean");
        assert_eq!(sb_field(&img, "Mount count"), 1);
        assert_ne!(field(&dumpe2fs_h(&img), "Last mount time"), "n/a");
        fs.sync().unwrap();
        fsck(&img);
        fs.shutdown().unwrap();
        assert!(fs.is_read_only());
        assert!(fs.statfs().unwrap().read_only, "statfs says so");
        assert_eq!(fs.touch(fs.root()), Err(Errno::EROFS));
        assert_eq!(state(&img), "clean");
        assert_eq!(sb_field(&img, "Mount count"), 1);
        fsck(&img);
        drop(fs);
        let (fs, _) = mount(&img, rw());
        drop(fs);
        assert_eq!(sb_field(&img, "Mount count"), 2);
        assert!(env.lines().is_empty(), "{:?}", env.lines());
    }
}

#[test]
fn every_superblock_write_updates_the_backups() {
    let cases: [(u32, u64, &[&str], usize); 4] = [
        (1024, 64 << 10, &[], 4),
        (4096, 512 << 10, &[], 2),
        (1024, 40 << 10, &["-O", "^sparse_super,^resize_inode"], 4),
        (1024, 64 << 10, &["-O", "sparse_super2"], 2),
    ];
    for (bs, kib, extra, copies) in cases {
        let img = mkfs("clean-backups", bs, kib, extra, None);
        let blocks = backups(&img);
        assert_eq!(blocks.len(), copies, "{extra:?}");
        let before = fs::read(&img).unwrap();
        let (mut fs, _) = mount(&img, rw());
        let primary = dumpe2fs_h(&img);
        for &b in &blocks {
            let backup = backup_header(&img, b, bs);
            for key in [
                "Filesystem state",
                "Mount count",
                "Free blocks",
                "Free inodes",
            ] {
                assert_eq!(field(&backup, key), field(&primary, key), "{key} at {b}");
            }
            // Each copy names its own group.
            let first = if bs == 1024 { 1 } else { 0 };
            let group = (b - first) / sb_field(&img, "Blocks per group");
            let nr = peek(&img, b * bs as u64 + 90, 2);
            assert_eq!(u16::from_le_bytes([nr[0], nr[1]]) as u64, group);
        }
        fs.shutdown().unwrap();
        let after = fs::read(&img).unwrap();
        // Only the superblock copies changed.
        let changed: Vec<u64> = (0..before.len() / bs as usize)
            .filter(|&i| {
                let r = i * bs as usize..(i + 1) * bs as usize;
                before[r.clone()] != after[r]
            })
            .map(|i| i as u64)
            .collect();
        let mut want = vec![if bs == 1024 { 1 } else { 0 }];
        want.extend(&blocks);
        assert_eq!(changed, want, "{extra:?}");
        assert_eq!(
            field(&backup_header(&img, blocks[0], bs), "Filesystem state"),
            "clean"
        );
        fsck(&img);
    }
}

#[test]
fn sync_with_nothing_changed_writes_nothing() {
    let img = mkfs("clean-idle", 1024, 4 << 10, &[], None);
    let dev = FileDisk::open(&img);
    let faults = dev.faults.clone();
    let env = TestEnv::new();
    let mut fs = Ext2::mount(dev, Box::new(env.clone()), rw()).unwrap();
    let writes = faults.writes.get();
    assert!(writes > 0, "mounting marks the filesystem in use");
    let root = fs.root();
    fs.read_dir(root).unwrap();
    fs.sync().unwrap();
    fs.sync().unwrap();
    assert_eq!(faults.writes.get(), writes);
    fs.shutdown().unwrap();
    let writes = faults.writes.get();
    fs.sync().unwrap();
    fs.shutdown().unwrap();
    assert_eq!(faults.writes.get(), writes, "shut down: nothing more");
}

#[test]
fn a_read_only_mount_writes_nothing() {
    let dir = staging("clean-ro");
    fs::write(dir.join("f"), b"data\n").unwrap();
    let img = mkfs("clean-ro", 1024, 4 << 10, &[], Some(&dir));
    debugfs_w(&img, "ssv state 0");
    let before = fs::read(&img).unwrap();
    let dev = FileDisk::open(&img);
    let faults = dev.faults.clone();
    let env = TestEnv::new();
    let mut fs = Ext2::mount(dev, Box::new(env.clone()), ro()).unwrap();
    let f = fs.lookup(fs.root(), b"f").unwrap();
    let mut buf = [0u8; 16];
    fs.read_at(f, 0, &mut buf).unwrap();
    assert!(fs.statfs().unwrap().read_only);
    fs.sync().unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    assert_eq!((faults.writes.get(), faults.flushes.get()), (0, 0));
    assert!(fs::read(&img).unwrap() == before, "the image is unchanged");
    assert!(env.logged("not cleanly unmounted"));
    // Read-write, the same steps write.
    let dev = FileDisk::open(&img);
    let faults = dev.faults.clone();
    let mut fs = Ext2::mount(dev, Box::new(env.clone()), rw()).unwrap();
    fs.shutdown().unwrap();
    assert!(faults.writes.get() > 0 && faults.flushes.get() > 0);
}

#[test]
fn a_read_only_fallback_writes_nothing() {
    let plain = mkfs("clean-plain", 1024, 4 << 10, &[], None);
    assert!(!mount(&plain, rw()).0.is_read_only());
    let img = mkfs("clean-huge", 1024, 4 << 10, &["-O", "huge_file"], None);
    let before = fs::read(&img).unwrap();
    let (mut fs, _) = mount(&img, rw());
    assert!(fs.is_read_only());
    assert!(fs.statfs().unwrap().read_only, "fell back to read-only");
    fs.shutdown().unwrap();
    assert!(fs::read(&img).unwrap() == before);
}

#[test]
fn a_device_that_cannot_write_fails_the_read_write_mount() {
    let img = mkfs("clean-broken", 1024, 4 << 10, &[], None);
    let dev = FileDisk::open(&img);
    dev.faults.fail_writes.set(true);
    let (r, env) = try_mount(dev, rw());
    assert_eq!(r.err(), Some(Errno::EIO));
    assert!(env.logged("ext2: write error"), "{:?}", env.lines());
    assert_eq!(state(&img), "clean");
    let (r, _) = try_mount(FileDisk::open(&img), ro());
    assert!(r.is_ok(), "read-only still works");
}

#[test]
fn a_failed_shutdown_is_finished_by_sync_or_another_shutdown() {
    for retry_with_sync in [true, false] {
        let img = mkfs("clean-retry", 1024, 4 << 10, &[], None);
        let dev = FileDisk::open(&img);
        let faults = dev.faults.clone();
        let env = TestEnv::new();
        let mut fs = Ext2::mount(dev, Box::new(env.clone()), rw()).unwrap();
        faults.fail_writes.set(true);
        assert_eq!(fs.shutdown(), Err(Errno::EIO));
        assert!(env.logged("ext2: write error"));
        assert!(fs.is_read_only());
        assert_eq!(fs.touch(fs.root()), Err(Errno::EROFS));
        assert_eq!(fs.sync(), Err(Errno::EIO), "still failing");
        assert_eq!(state(&img), "not clean");
        faults.fail_writes.set(false);
        if retry_with_sync {
            fs.sync().unwrap();
        } else {
            fs.shutdown().unwrap();
        }
        assert_eq!(state(&img), "clean");
        fsck(&img);
        let writes = faults.writes.get();
        fs.sync().unwrap();
        fs.shutdown().unwrap();
        assert_eq!(faults.writes.get(), writes, "then nothing is left");
    }
}

#[test]
fn only_a_filesystem_clean_at_mount_is_marked_clean() {
    for (before, after) in [
        ("0", "not clean"),
        ("1", "clean"),
        ("3", "clean with errors"),
        ("2", "not clean with errors"),
    ] {
        let img = mkfs("clean-mountstate", 1024, 4 << 10, &[], None);
        debugfs_w(&img, &format!("ssv state {before}"));
        let (mut fs, _) = mount(&img, rw());
        assert_eq!(
            state(&img),
            if before.contains(['2', '3']) {
                "not clean with errors"
            } else {
                "not clean"
            }
        );
        fs.shutdown().unwrap();
        assert_eq!(state(&img), after, "state {before}");
    }
}
