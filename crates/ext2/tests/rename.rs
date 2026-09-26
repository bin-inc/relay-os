//! Rename: every rule of the contract, `..` and link counts of moved
//! directories, replaced inodes freed, and a rename that cannot fit
//! changing nothing; checked with e2fsck and debugfs.

mod common;

use common::*;
use ext2::Ext2;
use std::fs;
use std::path::Path;
use vfs::{Errno, FileSystem, Ino, MountTable, Vfs};

fn free_counts(img: &Path) -> (u64, u64) {
    (sb_field(img, "Free blocks"), sb_field(img, "Free inodes"))
}

/// The inode debugfs lists for `name` in `dir`.
fn listed(img: &Path, dir: &str, name: &str) -> Option<u64> {
    debugfs_ls(img, dir)
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.ino)
}

#[test]
fn files_and_directories_move_within_and_across_directories() {
    for bs in [1024, 4096] {
        let img = mkfs("rename-move", bs, 8 << 10, &[], None);
        let env = TestEnv::new();
        let mut fs = Ext2::mount(FileDisk::open(&img), Box::new(env.clone()), rw()).unwrap();
        let root = fs.root();
        let a = fs.mkdir(root, b"a").unwrap();
        let b = fs.mkdir(root, b"b").unwrap();
        let f = fs.create(a, b"f").unwrap();
        fs.write_at(f, 0, b"contents").unwrap();
        let d = fs.mkdir(a, b"d").unwrap();
        fs.create(d, b"inside").unwrap();
        // Within a directory.
        fs.rename(a, b"f", a, b"g").unwrap();
        assert_eq!(fs.lookup(a, b"g"), Ok(f));
        assert_eq!(fs.lookup(a, b"f"), Err(Errno::ENOENT));
        fs.rename(a, b"d", a, b"e").unwrap();
        assert_eq!(fs.stat(a).unwrap().nlink, 3, "same parent: links unchanged");
        // Across directories, with the times of both and of the inode.
        env.set_now(T0 + 30);
        fs.rename(a, b"g", b, b"g").unwrap();
        for ino in [a, b] {
            let st = fs.stat(ino).unwrap();
            assert_eq!((st.mtime, st.ctime), (T0 + 30, T0 + 30));
        }
        let st = fs.stat(f).unwrap();
        assert_eq!((st.ctime, st.mtime), (T0 + 30, T0));
        // A directory to another parent: `..` and both link counts follow.
        fs.rename(a, b"e", b, b"moved").unwrap();
        assert_eq!(fs.lookup(d, b".."), Ok(b));
        assert_eq!(fs.stat(a).unwrap().nlink, 2);
        assert_eq!(fs.stat(b).unwrap().nlink, 3);
        fs.sync().unwrap();
        fsck(&img);
        assert_eq!(listed(&img, "b", "g"), Some(f));
        assert_eq!(listed(&img, "b/moved", ".."), Some(b));
        assert_eq!(listed(&img, "a", "g"), None);
        assert_eq!(debugfs_stat(&img, "b").links, 3);
        let mut buf = [0u8; 8];
        fs.read_at(f, 0, &mut buf).unwrap();
        assert_eq!(&buf, b"contents");
        // Up to the root, through the mount table.
        fs.shutdown().unwrap();
        let (fs, _) = mount(&img, rw());
        let mut vfs = MountTable::new(Box::new(fs));
        vfs.rename(b"/b/moved", b"/top").unwrap();
        vfs.chdir(b"/top").unwrap();
        assert_eq!(vfs.lookup(b"..").unwrap(), vfs.lookup(b"/").unwrap());
        assert!(vfs.lookup(b"inside").is_ok());
        vfs.shutdown().unwrap();
        fsck(&img);
        assert_eq!(listed(&img, "top", ".."), Some(2));
    }
}

#[test]
fn a_replaced_file_is_freed() {
    let dir = staging("rename-replace");
    fs::write(dir.join("f"), b"new").unwrap();
    fs::write(dir.join("g"), vec![7u8; 5000]).unwrap();
    fs::write(dir.join("h"), b"old").unwrap();
    let img = mkfs("rename-replace", 1024, 4 << 10, &[], Some(&dir));
    debugfs_w(&img, "symlink link f");
    let start = free_counts(&img);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let f = fs.lookup(root, b"f").unwrap();
    let g = fs.lookup(root, b"g").unwrap();
    fs.rename(root, b"f", root, b"g").unwrap();
    assert_eq!(fs.lookup(root, b"g"), Ok(f));
    assert_eq!(fs.stat(g), Err(Errno::ENOENT));
    // A symlink replaces a file: the entry's type byte changes too.
    let link = fs.lookup(root, b"link").unwrap();
    fs.rename(root, b"link", root, b"h").unwrap();
    let h = fs.lookup(root, b"h").unwrap();
    assert_eq!(fs.read_link(h).unwrap(), b"f");
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(listed(&img, "/", "h"), Some(link));
    assert_eq!(
        free_counts(&img),
        (start.0 + 6, start.1 + 2),
        "g's 5 blocks, h's 1"
    );
    fs.shutdown().unwrap();
}

#[test]
fn an_empty_directory_is_replaced() {
    let img = mkfs("rename-emptydir", 1024, 4 << 10, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let a = fs.mkdir(root, b"a").unwrap();
    let b = fs.mkdir(root, b"b").unwrap();
    let x = fs.mkdir(a, b"x").unwrap();
    fs.create(x, b"file").unwrap();
    let y = fs.mkdir(b, b"y").unwrap();
    fs.sync().unwrap();
    let start = free_counts(&img);
    fs.rename(a, b"x", b, b"y").unwrap();
    assert_eq!(fs.stat(y), Err(Errno::ENOENT));
    assert_eq!(fs.lookup(b, b"y"), Ok(x));
    assert_eq!(fs.lookup(x, b".."), Ok(b));
    assert_eq!(fs.stat(a).unwrap().nlink, 2);
    assert_eq!(fs.stat(b).unwrap().nlink, 3);
    // Within one directory too.
    let z = fs.mkdir(b, b"z").unwrap();
    fs.rename(b, b"y", b, b"z").unwrap();
    assert_eq!(fs.stat(z), Err(Errno::ENOENT));
    assert_eq!(fs.stat(b).unwrap().nlink, 3);
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(
        free_counts(&img),
        (start.0 + 1, start.1 + 1),
        "y's block and inode"
    );
    assert_eq!(debugfs_stat(&img, "b").links, 3);
    fs.shutdown().unwrap();
}

#[test]
fn refused_renames_change_nothing() {
    let img = mkfs("rename-refused", 1024, 4 << 10, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let a = fs.mkdir(root, b"a").unwrap();
    let b = fs.mkdir(a, b"b").unwrap();
    let full = fs.mkdir(root, b"full").unwrap();
    fs.create(full, b"x").unwrap();
    let f = fs.create(root, b"f").unwrap();
    fs.create(b, b"file").unwrap();
    fs.sync().unwrap();
    let before = fs::read(&img).unwrap();
    // Into itself or below; EINVAL before the target's own checks.
    assert_eq!(fs.rename(root, b"a", a, b"x"), Err(Errno::EINVAL));
    assert_eq!(fs.rename(root, b"a", b, b"x"), Err(Errno::EINVAL));
    assert_eq!(fs.rename(root, b"a", b, b"file"), Err(Errno::EINVAL));
    assert_eq!(fs.rename(root, b"a", root, b"f"), Err(Errno::ENOTDIR));
    assert_eq!(fs.rename(root, b"f", root, b"a"), Err(Errno::EISDIR));
    assert_eq!(fs.rename(root, b"a", root, b"full"), Err(Errno::ENOTEMPTY));
    assert_eq!(fs.rename(root, b"missing", root, b"x"), Err(Errno::ENOENT));
    // The contract's first rules: unused directories, names, not
    // directories.
    assert_eq!(fs.rename(900, b"f", root, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.rename(root, b"f", 900, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.rename(root, b"", root, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.rename(root, b"f", root, b"a/b"), Err(Errno::EINVAL));
    assert_eq!(
        fs.rename(root, b"f", root, &[b'n'; 256]),
        Err(Errno::ENAMETOOLONG)
    );
    assert_eq!(fs.rename(f, b"x", root, b"y"), Err(Errno::ENOTDIR));
    assert_eq!(fs.rename(root, b"f", f, b"y"), Err(Errno::ENOTDIR));
    // The same inode: nothing happens.
    fs.rename(root, b"f", root, b"f").unwrap();
    fs.rename(root, b"a", root, b"a").unwrap();
    fs.sync().unwrap();
    assert!(fs::read(&img).unwrap() == before, "the image is unchanged");
    fs.shutdown().unwrap();
    assert_eq!(fs.rename(root, b"f", root, b"g"), Err(Errno::EROFS));
    assert_eq!(fs.rename(900, b"f", root, b"g"), Err(Errno::ENOENT));
}

#[test]
fn hard_links_to_one_inode_rename_as_a_no_op() {
    let dir = staging("rename-links");
    fs::write(dir.join("f"), b"x").unwrap();
    let img = mkfs("rename-links", 1024, 4 << 10, &[], Some(&dir));
    debugfs_w(&img, "ln f f2");
    debugfs_w(&img, "set_inode_field f links_count 2");
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let f: Ino = fs.lookup(root, b"f").unwrap();
    fs.rename(root, b"f", root, b"f2").unwrap();
    assert_eq!(fs.lookup(root, b"f"), Ok(f), "both names stay");
    assert_eq!(fs.lookup(root, b"f2"), Ok(f));
    assert_eq!(fs.stat(f).unwrap().nlink, 2);
    fs.sync().unwrap();
    fsck(&img);
    fs.shutdown().unwrap();
}

#[test]
fn a_rename_that_needs_a_block_on_a_full_filesystem_changes_nothing() {
    let img = mkfs("rename-full", 1024, 1024, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let dst = fs.mkdir(root, b"dst").unwrap();
    let f = fs.create(root, b"f").unwrap();
    let old = fs.create(dst, b"old").unwrap();
    let fill = fs.create(root, b"fill").unwrap();
    fs.write_at(fill, 0, &vec![1u8; 2 << 20]).unwrap();
    let mut i = 0;
    while fs.create(dst, format!("entry-{i:03}").as_bytes()).is_ok() {
        i += 1;
    }
    assert_eq!(fs.statfs().unwrap().free_blocks, 0);
    fs.sync().unwrap();
    fsck(&img);
    let before = fs::read(&img).unwrap();
    assert_eq!(
        fs.rename(root, b"f", dst, b"a-new-long-name"),
        Err(Errno::ENOSPC)
    );
    fs.sync().unwrap();
    assert!(fs::read(&img).unwrap() == before, "the image is unchanged");
    assert_eq!(fs.lookup(root, b"f"), Ok(f));
    // Replacing needs no space.
    fs.rename(root, b"f", dst, b"old").unwrap();
    assert_eq!(fs.lookup(dst, b"old"), Ok(f));
    assert_eq!(fs.stat(old), Err(Errno::ENOENT));
    fs.sync().unwrap();
    fsck(&img);
    fs.shutdown().unwrap();
}

#[test]
fn a_dotdot_loop_is_eio_quickly() {
    let img = mkfs("rename-loop", 1024, 8 << 10, &["-N", "8192"], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let a = fs.mkdir(root, b"a").unwrap();
    let b = fs.mkdir(a, b"b").unwrap();
    fs.mkdir(root, b"x").unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    // b's `..` (after `.`, 12 bytes into its block) names b itself, and the
    // free inode count claims every inode is in use.
    let block: u64 = debugfs(&img, "blocks /a/b").trim().parse().unwrap();
    poke(&img, block * 1024 + 12, &(b as u32).to_le_bytes());
    debugfs_w(&img, "ssv free_inodes_count 0");
    let (mut fs, env) = mount(&img, rw());
    let start = std::time::Instant::now();
    assert_eq!(fs.rename(root, b"x", b, b"y"), Err(Errno::EIO));
    assert!(
        env.logged(&format!(
            "ext2: directory {b}: no root within 4096 levels of .."
        )),
        "{:?}",
        env.lines()
    );
    assert!(start.elapsed().as_secs() < 5);
    assert_eq!(fs.lookup(root, b"x").map(|_| ()), Ok(()), "nothing moved");
}
