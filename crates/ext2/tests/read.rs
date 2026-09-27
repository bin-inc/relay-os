//! Reading files, symlinks and special files from images populated by
//! `mke2fs -d`, compared with the host files and with `debugfs`.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use vfs::{Errno, FileSystem, FileType};

/// Deterministic bytes that differ between files and blocks.
fn pattern(len: usize, seed: u8) -> Vec<u8> {
    let mut x = 0x9E37_79B9u32 ^ seed as u32;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect()
}

/// Where the sparse file's far data sits: past the start of the triple
/// indirect range (12 + p + p² blocks).
fn far_offset(bs: u32) -> u64 {
    if bs == 1024 { 70 << 20 } else { 5 << 30 }
}

/// An image with every kind of file, and the staging tree it came from.
fn fixture(bs: u32) -> (PathBuf, PathBuf) {
    let dir = staging(&format!("read-{bs}"));
    fs::write(dir.join("empty"), b"").unwrap();
    fs::write(dir.join("small"), b"hello, ext2\n").unwrap();
    fs::write(dir.join("multi"), pattern(10_000, 1)).unwrap();
    fs::write(dir.join("big"), pattern(300 << 10, 2)).unwrap();
    let sparse = fs::File::create(dir.join("sparse")).unwrap();
    sparse.write_all_at(b"start", 0).unwrap();
    sparse.write_all_at(&pattern(3000, 3), 1 << 20).unwrap();
    sparse
        .write_all_at(&pattern(5000, 4), far_offset(bs) - 1000)
        .unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/nested"), b"nested\n").unwrap();
    std::os::unix::fs::symlink("small", dir.join("fast")).unwrap();
    std::os::unix::fs::symlink("sub/".repeat(25), dir.join("slow")).unwrap();
    let img = mkfs("read", bs, 16 << 10, &[], Some(&dir));
    debugfs_w(&img, "mknod null c 1 3");
    debugfs_w(&img, "mknod fifo p");
    (img, dir)
}

fn read_all(fs: &mut impl FileSystem, ino: u64, offset: u64, len: usize, chunk: usize) -> Vec<u8> {
    let mut out = vec![0; len];
    let mut done = 0;
    while done < len {
        let end = (done + chunk).min(len);
        let n = fs
            .read_at(ino, offset + done as u64, &mut out[done..end])
            .unwrap();
        assert!(n > 0, "short read at {}", offset + done as u64);
        done += n;
    }
    out
}

fn host_range(path: &Path, offset: u64, len: usize) -> Vec<u8> {
    let mut buf = vec![0; len];
    fs::File::open(path)
        .unwrap()
        .read_exact_at(&mut buf, offset)
        .unwrap();
    buf
}

#[test]
fn files_read_back_byte_for_byte() {
    for bs in [1024, 4096] {
        let (img, dir) = fixture(bs);
        let (mut fs, env) = mount(&img, ro());
        for name in ["empty", "small", "multi", "big", "sub/nested"] {
            let ino = ino_of(&img, name);
            let want = fs::read(dir.join(name)).unwrap();
            for chunk in [1usize, 1000, 7000, 1 << 20] {
                if chunk == 1 && want.len() > 20_000 {
                    continue;
                }
                assert_eq!(read_all(&mut fs, ino, 0, want.len(), chunk), want, "{name}");
            }
        }
        // The sparse file: its data, and holes around and between.
        let ino = ino_of(&img, "sparse");
        let host = dir.join("sparse");
        let size = fs::metadata(&host).unwrap().len();
        assert_eq!(fs.stat(ino).unwrap().size, size);
        for (offset, len) in [
            (0, 1 << 20),
            ((1 << 20) - 100, 5000),
            (far_offset(bs) - 70_000, 70_000 + 4000),
            (size - 4000, 4000),
            (size / 2, 9000),
            (far_offset(bs) / 3, 9000),
        ] {
            let got = read_all(&mut fs, ino, offset, len, 64 << 10);
            assert!(got == host_range(&host, offset, len), "sparse at {offset}");
        }
        assert!(env.lines().is_empty(), "{:?}", env.lines());
    }
}

#[test]
fn reads_stop_at_the_end_of_the_file() {
    let (img, _) = fixture(1024);
    let (mut fs, _) = mount(&img, ro());
    let ino = ino_of(&img, "small");
    let mut buf = [0xAAu8; 32];
    assert_eq!(fs.read_at(ino, 7, &mut buf).unwrap(), 5);
    assert_eq!(&buf[..5], b"ext2\n");
    assert_eq!(fs.read_at(ino, 12, &mut buf).unwrap(), 0);
    assert_eq!(fs.read_at(ino, u64::MAX, &mut buf).unwrap(), 0);
    assert_eq!(fs.read_at(ino, 0, &mut []).unwrap(), 0);
}

#[test]
fn stat_agrees_with_the_host_and_debugfs() {
    for bs in [1024, 4096] {
        let (img, dir) = fixture(bs);
        let (mut fs, _) = mount(&img, ro());
        for name in [
            "empty",
            "small",
            "multi",
            "big",
            "sparse",
            "sub",
            "sub/nested",
            "fast",
            "slow",
            "null",
            "fifo",
            "lost+found",
        ] {
            let d = debugfs_stat(&img, name);
            let st = fs.stat(d.ino).unwrap();
            let kind_bits = match st.kind {
                FileType::Regular => 0o100000,
                FileType::Directory => 0o040000,
                FileType::Symlink => 0o120000,
                FileType::CharDev => 0o020000,
                FileType::Fifo => 0o010000,
                other => panic!("{name}: {other:?}"),
            };
            assert_eq!(st.ino, d.ino);
            assert_eq!(kind_bits | st.perm, d.mode | kind_bits, "{name}");
            assert_eq!((st.uid, st.gid), (d.uid, d.gid), "{name}");
            assert_eq!((st.size, st.nlink), (d.size, d.links), "{name}");
            assert_eq!(st.blocks, d.blockcount, "{name}");
            assert_eq!(st.mtime, d.mtime, "{name}");
            assert_eq!(st.block_size, bs);
            if let Ok(host) = fs::symlink_metadata(dir.join(name)) {
                assert_eq!(st.perm as u32, host.mode() & 0o7777, "{name}");
                assert_eq!(st.mtime, host.mtime() as u64, "{name}");
                if st.kind != FileType::Directory {
                    assert_eq!(st.size, host.len(), "{name}");
                }
            }
        }
        let root = fs.root();
        let st = fs.stat(root).unwrap();
        assert_eq!((st.kind, st.nlink), (FileType::Directory, 4));
        // The sparse file stores its data runs, not tens of megabytes.
        let sparse = fs.stat(ino_of(&img, "sparse")).unwrap();
        assert!(sparse.blocks < 256, "{} sectors", sparse.blocks);
    }
}

#[test]
fn symlinks_give_their_targets() {
    for bs in [1024, 4096] {
        let (img, _) = fixture(bs);
        let (mut fs, _) = mount(&img, ro());
        let fast = ino_of(&img, "fast");
        let slow = ino_of(&img, "slow");
        assert_eq!(fs.read_link(fast).unwrap(), b"small");
        assert_eq!(fs.read_link(slow).unwrap(), "sub/".repeat(25).as_bytes());
        assert_eq!(fs.stat(fast).unwrap().blocks, 0, "fast: no data block");
        assert_eq!(fs.stat(slow).unwrap().blocks, bs as u64 / 512);
        let mut buf = [0u8; 8];
        assert_eq!(fs.read_at(fast, 0, &mut buf), Err(Errno::EINVAL));
        assert_eq!(fs.read_link(ino_of(&img, "small")), Err(Errno::EINVAL));
        assert_eq!(fs.read_link(fs.root()), Err(Errno::EINVAL));
    }
}

#[test]
fn special_files_and_directories_are_not_read() {
    let (img, _) = fixture(1024);
    let (mut fs, _) = mount(&img, ro());
    let null = ino_of(&img, "null");
    let fifo = ino_of(&img, "fifo");
    assert_eq!(fs.stat(null).unwrap().kind, FileType::CharDev);
    assert_eq!(fs.stat(fifo).unwrap().kind, FileType::Fifo);
    let mut buf = [0u8; 8];
    assert_eq!(fs.read_at(null, 0, &mut buf), Err(Errno::EINVAL));
    assert_eq!(fs.read_at(fifo, 0, &mut buf), Err(Errno::EINVAL));
    let root = fs.root();
    assert_eq!(fs.read_at(root, 0, &mut buf), Err(Errno::EISDIR));
}

#[test]
fn inodes_not_in_use_are_enoent() {
    let (img, _) = fixture(1024);
    let (mut fs, _) = mount(&img, ro());
    let inodes: u64 = field(&dumpe2fs_h(&img), "Inode count").parse().unwrap();
    let mut buf = [0u8; 8];
    for ino in [0, 1, 200, inodes, inodes + 1, u32::MAX as u64 + 3, u64::MAX] {
        assert_eq!(fs.stat(ino), Err(Errno::ENOENT), "{ino}");
        assert_eq!(fs.read_at(ino, 0, &mut buf), Err(Errno::ENOENT), "{ino}");
        assert_eq!(fs.read_link(ino), Err(Errno::ENOENT), "{ino}");
    }
}

/// Group 0's block bitmap and first inode table block (1 KiB blocks: the
/// descriptor table is at byte 2048).
fn group0_metadata(img: &Path) -> (u32, u32) {
    let d = common::peek(img, 2048, 12);
    let at = |i: usize| u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
    (at(0), at(8))
}

#[test]
fn block_pointers_into_metadata_are_eio() {
    let (img, _) = fixture(1024);
    let (bitmap, table) = group0_metadata(&img);
    debugfs_w(&img, &format!("set_inode_field small block[0] {table}"));
    debugfs_w(&img, &format!("set_inode_field big block[IND] {bitmap}"));
    debugfs_w(&img, "set_inode_field multi block[1] 1");
    let (mut fs, env) = mount(&img, ro());
    let mut buf = vec![0u8; 2048];
    let small = ino_of(&img, "small");
    assert_eq!(fs.read_at(small, 0, &mut buf), Err(Errno::EIO));
    assert!(env.logged(&format!(
        "ext2: inode {small}: block pointer {table} into metadata"
    )));
    let big = ino_of(&img, "big");
    assert_eq!(
        fs.read_at(big, 0, &mut buf),
        Ok(2048),
        "direct blocks still read"
    );
    assert_eq!(fs.read_at(big, 20 << 10, &mut buf), Err(Errno::EIO));
    let multi = ino_of(&img, "multi");
    assert_eq!(
        fs.read_at(multi, 1024, &mut buf),
        Err(Errno::EIO),
        "the superblock"
    );
    assert!(fs.stat(fs.root()).is_ok());
}

#[test]
fn an_impossible_file_size_is_eio() {
    let (img, _) = fixture(1024);
    debugfs_w(&img, "set_inode_field small size_hi 0x10000");
    let (mut fs, env) = mount(&img, ro());
    let small = ino_of(&img, "small");
    let mut buf = [0u8; 8];
    assert_eq!(fs.stat(small), Err(Errno::EIO));
    assert_eq!(fs.read_at(small, 0, &mut buf), Err(Errno::EIO));
    assert!(env.logged(&format!(
        "ext2: inode {small}: size {} beyond the largest file",
        (0x10000u64 << 32) + 12
    )));
}
