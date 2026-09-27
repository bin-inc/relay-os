//! Corrupt images: nothing on the disk may make the driver panic. Every
//! failure is an error, and corrupt metadata is logged.

mod common;

use common::*;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use vfs::{Errno, FileSystem, FileType, Ino};

/// A 1 KiB-block image with files reaching every level of indirection,
/// a directory of several blocks, nested directories and symlinks.
fn fixture(name: &str) -> PathBuf {
    let dir = staging(name);
    fs::write(dir.join("small"), b"small file\n").unwrap();
    fs::write(dir.join("big"), vec![0x5A; 300 << 10]).unwrap();
    let sparse = fs::File::create(dir.join("sparse")).unwrap();
    sparse.write_all_at(b"start", 0).unwrap();
    sparse.write_all_at(b"double", 1 << 20).unwrap();
    sparse.write_all_at(b"triple", 70 << 20).unwrap();
    fs::create_dir_all(dir.join("a/b/c")).unwrap();
    fs::write(dir.join("a/b/c/deep"), b"deep\n").unwrap();
    fs::create_dir(dir.join("many")).unwrap();
    for i in 0..60 {
        fs::write(dir.join(format!("many/file-number-{i}")), b"x").unwrap();
    }
    std::os::unix::fs::symlink("small", dir.join("fast")).unwrap();
    std::os::unix::fs::symlink("a/".repeat(40), dir.join("slow")).unwrap();
    mkfs(name, 1024, 2 << 10, &[], Some(&dir))
}

/// The numbers after `(IND):`, `(DIND):` and `(TIND):` in `debugfs stat`.
fn indirect_blocks(img: &Path, path: &str) -> Vec<u64> {
    let text = debugfs_stat(img, path).text;
    let mut out = Vec::new();
    for tag in ["(IND):", "(DIND):", "(TIND):"] {
        for part in text.split(tag).skip(1) {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            out.push(digits.parse().unwrap());
        }
    }
    out
}

/// The blocks worth corrupting: superblock, group descriptors, bitmaps,
/// the used part of the inode table, directory and indirect blocks.
fn metadata_blocks(img: &Path) -> Vec<u64> {
    let mut blocks = vec![1, 2];
    let text = dumpe2fs(img);
    let number_after = |key: &str| -> u64 {
        let rest = text.split(key).nth(1).unwrap();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().unwrap()
    };
    blocks.push(number_after("Block bitmap at "));
    blocks.push(number_after("Inode bitmap at "));
    let table = number_after("Inode table at ");
    blocks.extend(table..table + 8);
    for dir in ["/", "/a", "/a/b", "/a/b/c", "/many"] {
        for b in debugfs(img, &format!("blocks {dir}")).split_whitespace() {
            blocks.push(b.parse().unwrap());
        }
    }
    blocks.extend(indirect_blocks(img, "big"));
    blocks.extend(indirect_blocks(img, "sparse"));
    blocks
}

/// Reads everything reachable from `dir`, ignoring errors. Directory
/// cycles and huge sizes (both possible in a corrupt image) are cut short.
fn walk(fs: &mut impl FileSystem, dir: Ino, seen: &mut BTreeSet<Ino>) {
    if !seen.insert(dir) || seen.len() > 200 {
        return;
    }
    let Ok(entries) = fs.read_dir(dir) else {
        return;
    };
    let _ = fs.lookup(dir, b"..");
    for e in entries {
        if e.name == b"." || e.name == b".." {
            continue;
        }
        let _ = fs.lookup(dir, &e.name);
        let Ok(st) = fs.stat(e.ino) else {
            continue;
        };
        match st.kind {
            FileType::Directory => walk(fs, e.ino, seen),
            FileType::Symlink => {
                let _ = fs.read_link(e.ino);
            }
            _ => {
                let mut buf = vec![0; 64 << 10];
                for offset in [0, 1 << 20, 70 << 20, st.size.saturating_sub(100)] {
                    let _ = fs.read_at(e.ino, offset, &mut buf);
                }
                for chunk in 0..4 {
                    let _ = fs.read_at(e.ino, chunk * buf.len() as u64, &mut buf);
                }
            }
        }
    }
}

#[test]
fn seeded_metadata_corruption_never_panics() {
    let img = fixture("corrupt-seeded");
    fsck(&img);
    // Intact, the walk reaches every directory.
    let (mut intact, _) = mount(&img, ro());
    let root = intact.root();
    let mut seen = BTreeSet::new();
    walk(&mut intact, root, &mut seen);
    assert_eq!(seen.len(), 6, "/, lost+found, a, a/b, a/b/c, many");
    let pristine = fs::read(&img).unwrap();
    let targets = metadata_blocks(&img);
    let work = scratch("corrupt-seeded-work", ".img");
    let mut rng = Rng::new(0xC0FF_EE00);
    let mut mounted = 0;
    for round in 0..400 {
        let mut image = pristine.clone();
        let block = targets[rng.below(targets.len() as u64) as usize];
        let base = block as usize * 1024;
        // Replay: the round, block and change are printed before the
        // attempt, so a panic shows what caused it.
        let how = rng.below(4);
        println!("round {round}: block {block}, kind {how}");
        match how {
            0 => {
                for _ in 0..1 + rng.below(8) {
                    let at = base + rng.below(1024) as usize;
                    image[at] = rng.next() as u8;
                }
            }
            1 => {
                let at = base + rng.below(256) as usize * 4;
                let v = [0, 1, 0xFFFF_FFFF, 0x8000_0000, rng.next() as u32][rng.below(5) as usize];
                image[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            2 => image[base..base + 1024].copy_from_slice(&rng.bytes(1024)),
            _ => image[base..base + 1024].fill([0, 0xFF][rng.below(2) as usize]),
        }
        fs::write(&work, &image).unwrap();
        let (result, _) = try_mount(FileDisk::open(&work), ro());
        let Ok(mut fs) = result else {
            continue;
        };
        mounted += 1;
        let _ = fs.statfs();
        let root = fs.root();
        walk(&mut fs, root, &mut BTreeSet::new());
        for ino in 1..40 {
            let _ = fs.stat(ino);
        }
    }
    assert!(mounted > 200, "only {mounted} corrupt images mounted");
}

#[test]
fn a_directory_entry_with_rec_len_0_is_eio() {
    let img = fixture("corrupt-reclen");
    let block: u64 = debugfs(&img, "blocks /many")
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    // The `..` entry, after `.` (12 bytes).
    poke(&img, block * 1024 + 12 + 4, &[0, 0]);
    let (mut fs, env) = mount(&img, ro());
    let root = fs.root();
    let many = fs.lookup(root, b"many").unwrap();
    assert_eq!(fs.lookup(many, b"file-number-7"), Err(Errno::EIO));
    assert_eq!(fs.read_dir(many), Err(Errno::EIO));
    assert!(
        env.logged(&format!(
            "ext2: directory {many}: block 0: bad rec_len at offset 12"
        )),
        "{:?}",
        env.lines()
    );
    assert!(
        fs.lookup(root, b"small").is_ok(),
        "other directories still work"
    );
}

#[test]
fn an_indirect_pointer_outside_the_filesystem_is_eio() {
    let img = fixture("corrupt-indirect");
    debugfs_w(&img, "set_inode_field big block[IND] 99999999");
    let (mut fs, env) = mount(&img, ro());
    let big = fs.lookup(fs.root(), b"big").unwrap();
    let mut buf = vec![0; 4096];
    assert_eq!(
        fs.read_at(big, 0, &mut buf),
        Ok(4096),
        "direct blocks still read"
    );
    assert_eq!(fs.read_at(big, 20 << 10, &mut buf), Err(Errno::EIO));
    assert!(env.logged(&format!(
        "ext2: inode {big}: block pointer 99999999 outside the filesystem"
    )));
}

#[test]
fn an_unknown_file_type_is_eio() {
    let img = fixture("corrupt-mode");
    debugfs_w(&img, "set_inode_field small mode 0170644");
    let (mut fs, env) = mount(&img, ro());
    let small = fs.lookup(fs.root(), b"small").unwrap();
    assert_eq!(fs.stat(small), Err(Errno::EIO));
    assert!(env.logged(&format!("ext2: inode {small}: unknown mode 0o170644")));
}

/// The first block of `/many`, whose third entry (after `.` and `..`, 12
/// bytes each) is a file.
fn many_block(img: &Path) -> u64 {
    debugfs(img, "blocks /many")
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn entries_with_impossible_names_are_eio() {
    // Patches of the third entry (at byte 24), and the logged reason.
    type Patch<'a> = &'a [(u64, &'a [u8])];
    let cases: [(Patch, &str); 4] = [
        (&[(24 + 8, b"/")], "name with / or NUL at offset 24"),
        (&[(24 + 9, b"\0")], "name with / or NUL at offset 24"),
        (&[(24 + 6, &[0])], "empty name at offset 24"),
        (
            &[(24 + 6, &[2]), (24 + 8, b"..")],
            "misplaced . or .. at offset 24",
        ),
    ];
    for (patch, reason) in cases {
        let img = fixture("corrupt-names");
        let block = many_block(&img);
        for &(at, bytes) in patch {
            poke(&img, block * 1024 + at, bytes);
        }
        let (mut fs, env) = mount(&img, ro());
        let root = fs.root();
        let many = fs.lookup(root, b"many").unwrap();
        assert_eq!(fs.read_dir(many), Err(Errno::EIO), "{reason}");
        assert_eq!(fs.lookup(many, b"file-number-59"), Err(Errno::EIO));
        assert!(
            env.logged(&format!("ext2: directory {many}: block 0: {reason}")),
            "{:?}",
            env.lines()
        );
    }
}

#[test]
fn directories_with_impossible_block_maps_are_eio() {
    let cases = [
        (
            "size 0x7ffffc00",
            "size 2147482624 needs more blocks than it has",
        ),
        ("block[1] FIRST", "appears twice"),
        ("block[1] 0", "hole at block 1"),
    ];
    for (field_value, reason) in cases {
        let img = fixture("corrupt-dirmap");
        let first = many_block(&img).to_string();
        let set = field_value.replace("FIRST", &first);
        debugfs_w(&img, &format!("set_inode_field many {set}"));
        let (mut fs, env) = mount(&img, ro());
        let root = fs.root();
        let many = fs.lookup(root, b"many").unwrap();
        assert_eq!(fs.read_dir(many), Err(Errno::EIO), "{reason}");
        assert_eq!(fs.lookup(many, b"missing"), Err(Errno::EIO));
        assert!(env.logged(reason), "{reason}: {:?}", env.lines());
    }
}
