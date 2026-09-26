//! Creating files and directories: entry insertion, new directory blocks,
//! inode placement, htree directories and running out of inodes or
//! blocks; checked with e2fsck and debugfs.

mod common;

use common::*;
use ext2::Ext2;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use vfs::{Errno, FileSystem, FileType, Ino};

/// Checks that debugfs lists exactly `names` (plus `.` and `..`) in
/// `dir`, with the inode numbers ext2 gave them.
fn assert_listing(img: &Path, dir: &str, names: &BTreeMap<Vec<u8>, Ino>) {
    let mut listed: BTreeMap<Vec<u8>, Ino> = debugfs_ls(img, dir)
        .into_iter()
        .map(|e| (e.name.into_bytes(), e.ino))
        .collect();
    listed.remove(&b"."[..]);
    listed.remove(&b".."[..]);
    listed.remove(&b"lost+found"[..]);
    assert_eq!(&listed, names, "{dir}");
}

fn group_of(img: &Path, ino: Ino) -> u64 {
    (ino - 1) / sb_field(img, "Inodes per group")
}

#[test]
fn many_entries_fill_new_directory_blocks() {
    for bs in [1024, 4096] {
        let img = mkfs("create-many", bs, 16 << 10, &[], None);
        let (mut fs, env) = mount(&img, rw());
        let root = fs.root();
        let dir = fs.mkdir(root, b"dir").unwrap();
        let mut names = BTreeMap::new();
        for i in 0..400 {
            let name = format!("file-with-a-name-of-some-length-{i:04}").into_bytes();
            let ino = fs.create(dir, &name).unwrap();
            assert_eq!(fs.lookup(dir, &name), Ok(ino));
            names.insert(name, ino);
        }
        let st = fs.stat(dir).unwrap();
        assert!(st.size > 4 * bs as u64, "grew to {} bytes", st.size);
        fs.sync().unwrap();
        fsck(&img);
        let d = debugfs_stat(&img, "dir");
        assert_eq!((d.size, d.blockcount), (st.size, st.blocks));
        assert_listing(&img, "dir", &names);
        let d = debugfs_stat(&img, "dir/file-with-a-name-of-some-length-0123");
        assert_eq!((d.mode, d.uid, d.gid, d.size, d.links), (0o644, 0, 0, 0, 1));
        assert!(d.text.contains("Size of extra inode fields: 32"));
        fs.shutdown().unwrap();
        fsck(&img);
        assert!(env.lines().is_empty(), "{:?}", env.lines());
    }
}

#[test]
fn names_of_every_length_fit() {
    let img = mkfs("create-lengths", 1024, 8 << 10, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let dir = fs.mkdir(fs.root(), b"lengths").unwrap();
    let mut names = BTreeMap::new();
    for len in 1..=255 {
        let name: Vec<u8> = (0..len).map(|i| b'a' + (i % 26) as u8).collect();
        names.insert(name.clone(), fs.create(dir, &name).unwrap());
    }
    assert_eq!(fs.create(dir, &[b'z'; 256]), Err(Errno::ENAMETOOLONG));
    for (name, &ino) in &names {
        assert_eq!(fs.lookup(dir, name), Ok(ino));
    }
    fs.sync().unwrap();
    fsck(&img);
    assert_listing(&img, "lengths", &names);
    fs.shutdown().unwrap();
}

#[test]
fn nested_directories_keep_link_counts() {
    let img = mkfs("create-nested", 1024, 8 << 10, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let root_links = fs.stat(root).unwrap().nlink;
    let mut dir = root;
    let mut path = String::new();
    for depth in 0..20 {
        let name = format!("level{depth}");
        let child = fs.mkdir(dir, name.as_bytes()).unwrap();
        fs.create(child, b"file").unwrap();
        assert_eq!(fs.lookup(child, b".."), Ok(dir));
        assert_eq!(fs.stat(child).unwrap().nlink, 2);
        path = format!("{path}/{name}");
        dir = child;
    }
    let a = fs.lookup(root, b"level0").unwrap();
    fs.mkdir(a, b"second").unwrap();
    assert_eq!(fs.stat(a).unwrap().nlink, 4);
    assert_eq!(fs.stat(root).unwrap().nlink, root_links + 1);
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(debugfs_stat(&img, "level0").links, 4);
    assert_eq!(debugfs_stat(&img, &path).links, 2);
    let names: Vec<String> = debugfs_ls(&img, &path)
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, [".", "..", "file"]);
    fs.shutdown().unwrap();
}

#[test]
fn new_inodes_are_placed_by_group() {
    // Four groups of 16 inodes; group 0 holds the reserved ones.
    let img = mkfs("create-groups", 1024, 32 << 10, &["-N", "64"], None);
    assert_eq!(sb_field(&img, "Inodes per group"), 16);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    // Directories go to the group with the most free inodes, the lowest
    // of equals.
    let mut dirs = Vec::new();
    for (i, want) in [1, 2, 3, 1].into_iter().enumerate() {
        let d = fs.mkdir(root, format!("d{i}").as_bytes()).unwrap();
        assert_eq!(group_of(&img, d), want, "d{i}");
        dirs.push(d);
    }
    // Files go to their directory's group while it has room, then to the
    // next group with a free inode.
    for i in 0..14 {
        let f = fs.create(dirs[0], format!("f{i}").as_bytes()).unwrap();
        assert_eq!(group_of(&img, f), 1);
    }
    let f = fs.create(dirs[0], b"overflow").unwrap();
    assert_eq!(group_of(&img, f), 2);
    let f = fs.create(root, b"in-root").unwrap();
    assert_eq!(f, 12, "the first free inode after s_first_ino");
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(debugfs_stat(&img, "d3").ino, dirs[3]);
    fs.shutdown().unwrap();
}

#[test]
fn creating_in_an_htree_directory_drops_the_index() {
    let dir = staging("create-htree");
    fs::create_dir(dir.join("big")).unwrap();
    for i in 0..500 {
        fs::write(dir.join(format!("big/some-entry-name-{i:04}")), b"").unwrap();
    }
    let img = mkfs("create-htree", 1024, 8 << 10, &[], Some(&dir));
    let status = tool("e2fsck")
        .arg("-fyD")
        .arg(&img)
        .output()
        .unwrap()
        .status;
    assert!(matches!(status.code(), Some(0 | 1)));
    assert!(debugfs_stat(&img, "big").text.contains("Flags: 0x1000"));
    let (mut fs, _) = mount(&img, rw());
    let big = fs.lookup(fs.root(), b"big").unwrap();
    let old = fs.lookup(big, b"some-entry-name-0250").unwrap();
    let new = fs.create(big, b"a-new-entry").unwrap();
    fs.sync().unwrap();
    fsck(&img);
    assert!(debugfs_stat(&img, "big").text.contains("Flags: 0x0"));
    assert_eq!(ino_of(&img, "big/a-new-entry"), new);
    assert_eq!(ino_of(&img, "big/some-entry-name-0250"), old);
    assert_eq!(fs.read_dir(big).unwrap().len(), 503);
    fs.shutdown().unwrap();
}

#[test]
fn running_out_of_inodes_is_enospc() {
    let img = mkfs("create-inodes", 1024, 4 << 10, &["-N", "32"], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let free = fs.statfs().unwrap().free_files;
    for i in 0..free {
        fs.create(root, format!("f{i}").as_bytes()).unwrap();
    }
    assert_eq!(fs.statfs().unwrap().free_files, 0);
    assert_eq!(fs.create(root, b"one-more"), Err(Errno::ENOSPC));
    assert_eq!(fs.mkdir(root, b"dir"), Err(Errno::ENOSPC));
    assert_eq!(fs.lookup(root, b"one-more"), Err(Errno::ENOENT));
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(sb_field(&img, "Free inodes"), 0);
    fs.shutdown().unwrap();
}

#[test]
fn running_out_of_blocks_for_directories_is_enospc() {
    let img = mkfs("create-blocks", 1024, 1024, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let fill = fs.create(root, b"fill").unwrap();
    let n = fs.write_at(fill, 0, &vec![0xAB; 2 << 20]).unwrap();
    assert_eq!(fs.statfs().unwrap().free_blocks, 0);
    // A directory needs a block.
    let files = fs.statfs().unwrap().free_files;
    assert_eq!(fs.mkdir(root, b"dir"), Err(Errno::ENOSPC));
    assert_eq!(
        fs.statfs().unwrap().free_files,
        files,
        "the inode went back"
    );
    // Files fit while the root's blocks have room.
    let mut created = 0;
    let err = loop {
        match fs.create(root, format!("name-{created:03}").as_bytes()) {
            Ok(_) => created += 1,
            Err(e) => break e,
        }
    };
    assert_eq!(err, Errno::ENOSPC);
    assert!(created > 10, "{created}");
    fs.sync().unwrap();
    fsck(&img);
    // One block free: the new directory's block fits, the root's next
    // block does not, and everything is undone.
    fs.truncate(fill, n as u64 - 1024).unwrap();
    assert_eq!(fs.statfs().unwrap().free_blocks, 1);
    let files = fs.statfs().unwrap().free_files;
    assert_eq!(fs.mkdir(root, b"dir"), Err(Errno::ENOSPC));
    assert_eq!(fs.statfs().unwrap().free_blocks, 1);
    assert_eq!(fs.statfs().unwrap().free_files, files);
    fs.sync().unwrap();
    fsck(&img);
    fs.shutdown().unwrap();
    fsck(&img);
}

#[test]
fn create_and_mkdir_follow_the_contract() {
    let dir = staging("create-contract");
    fs::write(dir.join("file"), b"x").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    let img = mkfs("create-contract", 1024, 4 << 10, &[], Some(&dir));
    let dev = FileDisk::open(&img);
    let env = TestEnv::new();
    let mut fs = Ext2::mount(dev, Box::new(env.clone()), rw()).unwrap();
    let root = fs.root();
    let file = fs.lookup(root, b"file").unwrap();
    assert_eq!(fs.create(root, b"file"), Err(Errno::EEXIST));
    assert_eq!(fs.create(root, b"sub"), Err(Errno::EEXIST));
    assert_eq!(fs.mkdir(root, b"file"), Err(Errno::EEXIST));
    assert_eq!(fs.create(file, b"x"), Err(Errno::ENOTDIR));
    assert_eq!(fs.create(file, b""), Err(Errno::ENOENT), "the name first");
    assert_eq!(fs.create(900, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.create(root, b""), Err(Errno::ENOENT));
    assert_eq!(fs.create(root, b"a/b"), Err(Errno::EINVAL));
    assert_eq!(fs.mkdir(root, b".."), Err(Errno::EINVAL));
    assert_eq!(fs.mkdir(root, &[b'n'; 256]), Err(Errno::ENAMETOOLONG));
    // New inodes: modes, owner, links and times; the parent's times.
    env.set_now(T0 + 50);
    let f = fs.create(root, b"new").unwrap();
    let st = fs.stat(f).unwrap();
    assert_eq!(
        (st.kind, st.perm, st.nlink, st.size, st.blocks),
        (FileType::Regular, 0o644, 1, 0, 0)
    );
    assert_eq!((st.uid, st.gid), (0, 0));
    assert_eq!((st.atime, st.mtime, st.ctime), (T0 + 50, T0 + 50, T0 + 50));
    let root_st = fs.stat(root).unwrap();
    assert_eq!((root_st.mtime, root_st.ctime), (T0 + 50, T0 + 50));
    let links = root_st.nlink;
    env.set_now(T0 + 60);
    let d = fs.mkdir(root, b"newdir").unwrap();
    let st = fs.stat(d).unwrap();
    assert_eq!(
        (st.kind, st.perm, st.nlink, st.size, st.blocks),
        (FileType::Directory, 0o755, 2, 1024, 2)
    );
    assert_eq!(fs.stat(root).unwrap().nlink, links + 1);
    assert_eq!(fs.lookup(d, b".."), Ok(root));
    let mut names: Vec<_> = fs
        .read_dir(d)
        .unwrap()
        .into_iter()
        .map(|e| (e.name, e.ino))
        .collect();
    names.sort();
    assert_eq!(names, [(b".".to_vec(), d), (b"..".to_vec(), root)]);
    fs.sync().unwrap();
    fsck(&img);
    fs.shutdown().unwrap();
    assert_eq!(fs.create(root, b"late"), Err(Errno::EROFS));
    assert_eq!(
        fs.create(root, b""),
        Err(Errno::EROFS),
        "EROFS before the name"
    );
    assert_eq!(
        fs.mkdir(900, b"x"),
        Err(Errno::ENOENT),
        "ENOENT before EROFS"
    );
}

#[test]
fn directories_without_file_types_grow_too() {
    let img = mkfs(
        "create-nofiletype",
        1024,
        4 << 10,
        &["-O", "^filetype"],
        None,
    );
    let (mut fs, _) = mount(&img, rw());
    let root = fs.root();
    let d = fs.mkdir(root, b"dir").unwrap();
    let mut names = BTreeMap::new();
    for i in 0..100 {
        let name = format!("entry-{i}").into_bytes();
        names.insert(name.clone(), fs.create(d, &name).unwrap());
    }
    fs.sync().unwrap();
    fsck(&img);
    assert_listing(&img, "dir", &names);
    fs.shutdown().unwrap();
}
