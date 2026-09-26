//! Mounting: the superblock, feature and group descriptor checks (spec
//! §8.2) against images made by `mke2fs`.

mod common;

use common::*;
use std::fs::OpenOptions;
use vfs::{Errno, FileSystem};

#[test]
fn images_with_1k_2k_and_4k_blocks_mount() {
    for bs in [1024, 2048, 4096] {
        let img = mkfs("mount-plain", bs, 8192, &[], None);
        let (mut fs, env) = mount(&img, ro());
        assert_eq!(fs.root(), 2);
        assert!(fs.is_read_only());
        assert!(env.lines().is_empty(), "{:?}", env.lines());
        fs.shutdown().unwrap();
        assert_eq!(fs.into_device().faults.writes.get(), 0);
    }
}

#[test]
fn a_device_block_size_that_divides_the_block_size_works() {
    let img = mkfs("mount-4kn", 4096, 8192, &[], None);
    let (r, _) = try_mount(FileDisk::with_block_size(&img, 4096), ro());
    assert!(r.is_ok());
    let img = mkfs("mount-4kn-1k", 1024, 8192, &[], None);
    let (r, env) = try_mount(FileDisk::with_block_size(&img, 4096), ro());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(env.logged("ext2: device block size 4096 does not divide the block size 1024"));
}

#[test]
fn ext4_incompat_features_are_refused() {
    for features in ["extent", "extent,64bit", "flex_bg"] {
        let img = mkfs("mount-ext4", 4096, 8192, &["-O", features], None);
        let (r, env) = try_mount(FileDisk::open(&img), ro());
        assert_eq!(r.err(), Some(Errno::EINVAL), "{features}");
        let line = env
            .lines()
            .into_iter()
            .find(|l| l.starts_with("ext2: unsupported incompat features: "))
            .unwrap_or_else(|| panic!("{features}: {:?}", env.lines()));
        for f in features.split(',') {
            assert!(line.contains(f), "{line}");
        }
    }
}

#[test]
fn unknown_ro_compat_features_mount_read_only() {
    let img = mkfs("mount-huge", 1024, 4096, &["-O", "huge_file"], None);
    let (mut fs, env) = mount(&img, rw());
    assert!(fs.is_read_only());
    assert!(env.logged("ext2: unsupported ro_compat features: huge_file; mounting read-only"));
    let root = fs.root();
    assert_eq!(fs.create(root, b"f"), Err(Errno::EROFS));
    assert_eq!(fs.mkdir(root, b"d"), Err(Errno::EROFS));
    assert_eq!(fs.write_at(root, 0, b"x"), Err(Errno::EROFS));
    assert_eq!(fs.touch(root), Err(Errno::EROFS));
    fs.sync().unwrap();
    fs.shutdown().unwrap();
    assert_eq!(fs.into_device().faults.writes.get(), 0);
}

#[test]
fn a_bad_magic_number_is_refused() {
    let img = mkfs("mount-magic", 1024, 4096, &[], None);
    poke(&img, 1024 + 56, &[0x34, 0x12]);
    let (r, env) = try_mount(FileDisk::open(&img), ro());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(env.logged("ext2: bad magic number 0x1234"));
}

#[test]
fn a_truncated_device_is_refused() {
    let img = mkfs("mount-short", 4096, 8192, &[], None);
    let f = OpenOptions::new().write(true).open(&img).unwrap();
    f.set_len(4096 * 1024).unwrap();
    let (r, env) = try_mount(FileDisk::open(&img), ro());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(env.logged("larger than the device"), "{:?}", env.lines());
    f.set_len(1024).unwrap();
    let (r, env) = try_mount(FileDisk::open(&img), ro());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(env.logged("ext2: device too small for a superblock"));
}

#[test]
fn a_corrupt_group_descriptor_is_refused() {
    // The table starts in the block after the superblock's.
    for (bs, table) in [(1024u64, 2048u64), (4096, 4096)] {
        let img = mkfs("mount-gdt", bs as u32, 8192, &[], None);
        poke(&img, table + 8, &0xFFFF_FF00u32.to_le_bytes());
        let (r, env) = try_mount(FileDisk::open(&img), ro());
        assert_eq!(r.err(), Some(Errno::EINVAL));
        assert!(env.logged("ext2: group 0: inode table outside the group"));
    }
}

#[test]
fn an_unclean_filesystem_mounts_with_a_warning() {
    let img = mkfs("mount-state", 1024, 4096, &[], None);
    debugfs_w(&img, "ssv state 2");
    let (_, env) = mount(&img, ro());
    assert!(env.logged("ext2: warning: not cleanly unmounted, run e2fsck"));
    assert!(env.logged("ext2: warning: filesystem has errors, run e2fsck"));
}

#[test]
fn no_superblock_field_value_makes_mount_panic() {
    let img = mkfs("mount-fuzz", 1024, 4096, &[], None);
    let original = peek(&img, 1024, 1024);
    for at in (0..256).step_by(2) {
        for value in [0u8, 0xFF, 0x80] {
            poke(&img, 1024 + at, &[value, value]);
            let (r, env) = try_mount(FileDisk::open(&img), ro());
            if let Err(e) = r {
                assert!(matches!(e, Errno::EINVAL | Errno::EIO), "{e:?}");
                assert!(!env.lines().is_empty(), "refusals are logged");
            }
            poke(&img, 1024, &original);
        }
    }
}

#[test]
fn overlapping_group_metadata_is_refused() {
    let img = mkfs("mount-overlap", 1024, 8192, &[], None);
    // Group 0's descriptor (1 KiB blocks: at byte 2048): its block bitmap
    // moves onto the first inode table block.
    let table = peek(&img, 2048 + 8, 4);
    poke(&img, 2048, &table);
    let (r, env) = try_mount(FileDisk::open(&img), rw());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(
        env.logged("ext2: group 0: block bitmap overlaps the inode table"),
        "{:?}",
        env.lines()
    );
}

#[test]
fn a_cleared_sparse_super_bit_cannot_make_backups_overwrite_bitmaps() {
    // Without sparse_super every group would hold a superblock copy, and
    // group 2's copy would land on its block bitmap.
    let img = mkfs("mount-sparse", 1024, 32 << 10, &[], None);
    let ro_compat = peek(&img, 1024 + 100, 4);
    poke(&img, 1024 + 100, &[ro_compat[0] & !1]);
    let before = std::fs::read(&img).unwrap();
    let (r, env) = try_mount(FileDisk::open(&img), rw());
    assert_eq!(r.err(), Some(Errno::EINVAL));
    assert!(
        env.logged("ext2: group 2: superblock copy overlaps the block bitmap"),
        "{:?}",
        env.lines()
    );
    assert!(std::fs::read(&img).unwrap() == before, "nothing written");
}
