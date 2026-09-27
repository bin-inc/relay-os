//! Directories: the Relay image layout read through the mount table, an
//! htree-indexed directory, directories without file types, lookup's rules
//! and `statfs`.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use vfs::{Errno, FileSystem, FileType, MountTable, Vfs};

/// The top-level directories of `/` and their modes, as `xtask image`
/// makes them (`xtask/src/config.rs`).
const ROOT_DIRS: &[(&str, u32)] = &[
    ("bin", 0o755),
    ("dev", 0o755),
    ("etc", 0o755),
    ("home", 0o755),
    ("root", 0o755),
    ("tmp", 0o1777),
    ("usr", 0o755),
    ("var", 0o755),
];

/// The flags `xtask image` passes to `mke2fs` (`xtask/src/image.rs`).
const RELAY_MKE2FS: &[&str] = &[
    "-I",
    "256",
    "-O",
    "^dir_index,^resize_inode,^ext_attr",
    "-L",
    "relayroot",
    "-E",
    "root_owner=0:0",
];

fn copy_tree(src: &Path, dst: &Path) {
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&to).unwrap();
            fs::set_permissions(&to, fs::Permissions::from_mode(0o755)).unwrap();
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).unwrap();
            fs::set_permissions(&to, fs::Permissions::from_mode(0o644)).unwrap();
        }
    }
}

/// The staging tree `xtask image` builds: `rootfs/` plus [`ROOT_DIRS`].
fn relay_staging() -> PathBuf {
    let dir = staging("tree-relay");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rootfs"),
        &dir,
    );
    for (name, mode) in ROOT_DIRS {
        let p = dir.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(*mode)).unwrap();
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

fn sorted_names(vfs: &mut MountTable, path: &[u8]) -> Vec<Vec<u8>> {
    let node = vfs.lookup(path).unwrap();
    let mut names: Vec<_> = vfs
        .read_dir(node)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n != b"." && n != b"..")
        .collect();
    names.sort();
    names
}

/// Compares `path` in `vfs` with the host tree at `host`: names, types,
/// modes, sizes and contents.
fn compare(vfs: &mut MountTable, path: &[u8], host: &Path) {
    let shown = String::from_utf8_lossy(path).into_owned();
    let node = vfs.lookup(path).unwrap();
    let st = vfs.stat(node).unwrap();
    let meta = fs::symlink_metadata(host).unwrap();
    assert_eq!(st.perm as u32, meta.mode() & 0o7777, "{shown}");
    if meta.is_dir() {
        assert_eq!(st.kind, FileType::Directory, "{shown}");
        let mut names = sorted_names(vfs, path);
        if path == b"/" {
            names.retain(|n| n != b"lost+found");
        }
        let mut want: Vec<Vec<u8>> = fs::read_dir(host)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_encoded_bytes())
            .collect();
        want.sort();
        assert_eq!(names, want, "{shown}");
        for name in want {
            let child = vfs::path::join(path, &name);
            compare(vfs, &child, &host.join(String::from_utf8(name).unwrap()));
        }
    } else {
        assert_eq!(st.kind, FileType::Regular, "{shown}");
        let want = fs::read(host).unwrap();
        assert_eq!(st.size, want.len() as u64, "{shown}");
        let mut got = vec![0; want.len() + 10];
        assert_eq!(vfs.read_at(node, 0, &mut got).unwrap(), want.len());
        assert_eq!(&got[..want.len()], &want[..], "{shown}");
    }
}

#[test]
fn the_relay_image_reads_through_the_mount_table() {
    let dir = relay_staging();
    let img = mkfs("tree-relay", 4096, 32 << 10, RELAY_MKE2FS, Some(&dir));
    let (fs, env) = mount(&img, ro());
    let mut vfs = MountTable::new(Box::new(fs));
    compare(&mut vfs, b"/", &dir);
    let tmp = vfs.lookup(b"/tmp").unwrap();
    assert_eq!(vfs.stat(tmp).unwrap().perm, 0o1777);
    let motd = vfs.lookup(b"/etc/motd").unwrap();
    assert!(vfs.stat(motd).unwrap().size > 0);
    vfs.chdir(b"/root").unwrap();
    assert_eq!(
        vfs.lookup(b"../etc/hostname").unwrap(),
        vfs.lookup(b"/etc/hostname").unwrap()
    );
    assert!(env.lines().is_empty(), "{:?}", env.lines());
}

#[test]
fn an_htree_directory_reads_completely() {
    let dir = staging("tree-htree");
    let big = dir.join("big");
    fs::create_dir(&big).unwrap();
    let names: Vec<String> = (0..3000)
        .map(|i| format!("entry-with-a-longish-name-{i:05}"))
        .collect();
    for name in &names {
        fs::write(big.join(name), b"").unwrap();
    }
    let img = mkfs("tree-htree", 1024, 16 << 10, &[], Some(&dir));
    // mke2fs links entries linearly; e2fsck -D rebuilds big directories as
    // htrees.
    let status = tool("e2fsck")
        .arg("-fyD")
        .arg(&img)
        .output()
        .unwrap()
        .status;
    assert!(
        matches!(status.code(), Some(0 | 1)),
        "e2fsck -fyD: {status}"
    );
    let d = debugfs_stat(&img, "big");
    assert!(d.text.contains("Flags: 0x1000"), "not indexed: {}", d.text);
    let (mut fs, env) = mount(&img, ro());
    let big_ino = fs.lookup(fs.root(), b"big").unwrap();
    assert_eq!(big_ino, d.ino);
    let mut got: Vec<Vec<u8>> = fs
        .read_dir(big_ino)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    got.sort();
    let mut want: Vec<Vec<u8>> = names.iter().map(|n| n.as_bytes().to_vec()).collect();
    want.extend([b".".to_vec(), b"..".to_vec()]);
    want.sort();
    assert_eq!(got, want);
    for name in [&names[0], &names[1234], &names[2999]] {
        let ino = fs.lookup(big_ino, name.as_bytes()).unwrap();
        assert_eq!(ino, ino_of(&img, &format!("big/{name}")));
    }
    assert!(env.lines().is_empty(), "{:?}", env.lines());
}

#[test]
fn directories_without_file_types_read() {
    let dir = staging("tree-nofiletype");
    fs::create_dir_all(dir.join("a/b")).unwrap();
    fs::write(dir.join("a/b/deep"), b"deep\n").unwrap();
    for i in 0..100 {
        fs::write(dir.join(format!("a/f{i}")), format!("{i}\n")).unwrap();
    }
    let img = mkfs(
        "tree-nofiletype",
        1024,
        4 << 10,
        &["-O", "^filetype"],
        Some(&dir),
    );
    assert!(!field(&dumpe2fs_h(&img), "Filesystem features").contains("filetype"));
    let (fs, _) = mount(&img, ro());
    let mut vfs = MountTable::new(Box::new(fs));
    compare(&mut vfs, b"/a", &dir.join("a"));
}

#[test]
fn lookup_follows_the_contract() {
    let dir = staging("tree-lookup");
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("file"), b"x").unwrap();
    let img = mkfs("tree-lookup", 1024, 4 << 10, &[], Some(&dir));
    let (mut fs, _) = mount(&img, ro());
    let root = fs.root();
    let sub = fs.lookup(root, b"sub").unwrap();
    let file = fs.lookup(root, b"file").unwrap();
    assert_eq!(fs.lookup(root, b".").unwrap(), root);
    assert_eq!(
        fs.lookup(root, b"..").unwrap(),
        root,
        "the root is its own parent"
    );
    assert_eq!(fs.lookup(sub, b".").unwrap(), sub);
    assert_eq!(fs.lookup(sub, b"..").unwrap(), root);
    assert_eq!(fs.lookup(root, b"missing"), Err(Errno::ENOENT));
    assert_eq!(fs.lookup(root, b""), Err(Errno::ENOENT));
    assert_eq!(fs.lookup(root, &[b'x'; 300]), Err(Errno::ENOENT));
    assert_eq!(fs.lookup(file, b"x"), Err(Errno::ENOTDIR));
    assert_eq!(fs.read_dir(file), Err(Errno::ENOTDIR));
    assert_eq!(fs.lookup(300, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.read_dir(300), Err(Errno::ENOENT));
    let entries = fs.read_dir(sub).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.name == b".." && e.ino == root));
}

/// Metadata blocks as Linux's ext2 counts them for `statfs`, from the
/// group lines `dumpe2fs` prints.
fn overhead(img: &Path) -> u64 {
    let text = dumpe2fs(img);
    let groups = text.lines().filter(|l| l.starts_with("Group ")).count() as u64;
    let copies = text.matches("superblock at").count() as u64;
    let gdt = text
        .split("Group descriptors at ")
        .nth(1)
        .unwrap()
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()
        .unwrap()
        .split('-')
        .map(|n| n.parse::<u64>().unwrap())
        .collect::<Vec<_>>();
    let gdt_blocks = gdt[1] - gdt[0] + 1;
    sb_field(img, "First block")
        + copies * (1 + gdt_blocks)
        + groups * (2 + sb_field(img, "Inode blocks per group"))
}

#[test]
fn statfs_counts_like_linux() {
    let cases: [(u32, &[&str]); 4] = [
        (1024, &[]),
        (4096, &[]),
        (1024, &["-O", "^sparse_super,^resize_inode"]),
        (2048, &["-m", "10"]),
    ];
    for (bs, extra) in cases {
        let img = mkfs("tree-statfs", bs, 64 << 10, extra, None);
        let (mut fs, _) = mount(&img, ro());
        let st = fs.statfs().unwrap();
        let free = sb_field(&img, "Free blocks");
        assert_eq!(st.block_size, bs as u64);
        assert_eq!(
            st.blocks,
            sb_field(&img, "Block count") - overhead(&img),
            "{bs} {extra:?}"
        );
        assert_eq!(st.free_blocks, free);
        assert_eq!(
            st.avail_blocks,
            free - sb_field(&img, "Reserved block count")
        );
        assert_eq!(st.files, sb_field(&img, "Inode count"));
        assert_eq!(st.free_files, sb_field(&img, "Free inodes"));
    }
}
