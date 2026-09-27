//! Removing files and directories: entry removal, freeing inodes and
//! blocks, link counts, special files and extended attribute blocks;
//! checked with e2fsck, debugfs and the free counts.

mod common;

use common::*;
use ext2::Ext2;
use std::fs;
use std::path::Path;
use vfs::{Errno, FileSystem, Ino};

fn free_counts(img: &Path) -> (u64, u64) {
    (sb_field(img, "Free blocks"), sb_field(img, "Free inodes"))
}

/// What the tree holds: `(parent index or None for the root, name,
/// directory?, data)`, parents before children.
type Tree = Vec<(Option<usize>, String, bool, Vec<(u64, usize)>)>;

/// A tree with small, indirect and sparse files, nested directories and
/// a directory of many entries.
fn tree(bs: u64) -> Tree {
    let mut t: Tree = vec![
        (None, "a".into(), true, vec![]),
        (Some(0), "b".into(), true, vec![]),
        (Some(1), "c".into(), true, vec![]),
        (None, "many".into(), true, vec![]),
        (None, "small".into(), false, vec![(0, 100)]),
        (
            Some(0),
            "indirect".into(),
            false,
            vec![(0, 20 * bs as usize)],
        ),
        (
            Some(2),
            "sparse".into(),
            false,
            vec![(0, 10), (70 << 20, 10)],
        ),
        (Some(1), "empty".into(), false, vec![]),
    ];
    for i in 0..150 {
        t.push((
            Some(3),
            format!("entry-number-{i:03}"),
            false,
            vec![(0, 10)],
        ));
    }
    t
}

fn build(fs: &mut impl FileSystem, tree: &Tree) -> Vec<(Ino, Ino)> {
    let mut made: Vec<(Ino, Ino)> = Vec::new();
    for (parent, name, dir, data) in tree {
        let p = parent.map_or(fs.root(), |i| made[i].1);
        let ino = if *dir {
            fs.mkdir(p, name.as_bytes()).unwrap()
        } else {
            fs.create(p, name.as_bytes()).unwrap()
        };
        for &(offset, len) in data {
            fs.write_at(ino, offset, &vec![0x77; len]).unwrap();
        }
        made.push((p, ino));
    }
    made
}

#[test]
fn removing_everything_restores_the_free_counts() {
    for bs in [1024u32, 4096] {
        for order in ["forward", "reverse", "shuffled"] {
            let img = mkfs("remove-all", bs, 16 << 10, &[], None);
            let start = free_counts(&img);
            let (mut fs, env) = mount(&img, rw());
            let t = tree(bs as u64);
            let made = build(&mut fs, &t);
            fs.sync().unwrap();
            fsck(&img);
            // Files first in the chosen order, then directories deepest
            // first.
            let mut files: Vec<usize> = (0..t.len()).filter(|&i| !t[i].2).collect();
            match order {
                "reverse" => files.reverse(),
                "shuffled" => {
                    let mut rng = Rng::new(bs as u64);
                    for i in (1..files.len()).rev() {
                        files.swap(i, rng.below(i as u64 + 1) as usize);
                    }
                }
                _ => {}
            }
            for (n, &i) in files.iter().enumerate() {
                fs.unlink(made[i].0, t[i].1.as_bytes()).unwrap();
                assert_eq!(fs.stat(made[i].1), Err(Errno::ENOENT));
                if n % 50 == 0 {
                    fs.sync().unwrap();
                    fsck(&img);
                }
            }
            for i in (0..t.len()).filter(|&i| t[i].2).rev() {
                fs.rmdir(made[i].0, t[i].1.as_bytes()).unwrap();
            }
            fs.sync().unwrap();
            fsck(&img);
            assert_eq!(free_counts(&img), start, "{bs} {order}");
            let names: Vec<String> = debugfs_ls(&img, "/").into_iter().map(|e| e.name).collect();
            assert_eq!(names, [".", "..", "lost+found"]);
            fs.shutdown().unwrap();
            fsck(&img);
            assert!(env.lines().is_empty(), "{:?}", env.lines());
        }
    }
}

#[test]
fn freed_entry_space_is_reused() {
    let img = mkfs("remove-reuse", 1024, 4 << 10, &[], None);
    let (mut fs, _) = mount(&img, rw());
    let d = fs.mkdir(fs.root(), b"d").unwrap();
    for name in ["a", "b", "c"] {
        fs.create(d, name.as_bytes()).unwrap();
    }
    fs.unlink(d, b"b").unwrap();
    fs.sync().unwrap();
    let names: Vec<String> = debugfs_ls(&img, "d").into_iter().map(|e| e.name).collect();
    assert_eq!(names, [".", "..", "a", "c"]);
    // Fill a few blocks, then empty the first entries of later blocks.
    for i in 0..100 {
        fs.create(d, format!("name-{i:03}").as_bytes()).unwrap();
    }
    let size = fs.stat(d).unwrap().size;
    for i in 0..100 {
        fs.unlink(d, format!("name-{i:03}").as_bytes()).unwrap();
    }
    for round in 0..300 {
        let name = format!("again-{}", round % 100);
        if round < 100 {
            fs.create(d, name.as_bytes()).unwrap();
        } else {
            fs.unlink(d, name.as_bytes()).unwrap();
            fs.create(d, name.as_bytes()).unwrap();
        }
    }
    assert_eq!(fs.stat(d).unwrap().size, size, "no new blocks");
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(debugfs_ls(&img, "d").len(), 2 + 2 + 100);
    fs.shutdown().unwrap();
}

#[test]
fn special_files_are_freed_without_reading_i_block() {
    let dir = staging("remove-special");
    fs::write(dir.join("keep"), vec![0x42; 3000]).unwrap();
    let img = mkfs("remove-special", 1024, 4 << 10, &[], Some(&dir));
    // A device number that reads as a block pointer to `keep`'s data.
    let block = bmap(&img, "keep", &[0])[0];
    debugfs_w(
        &img,
        &format!("mknod blockdev b {} {}", block / 256, block % 256),
    );
    debugfs_w(&img, "mknod chardev c 1 3");
    debugfs_w(&img, "mknod fifo p");
    debugfs_w(&img, "symlink fast keep");
    debugfs_w(&img, &format!("symlink slow {}", "x/".repeat(60)));
    fsck(&img);
    let start = free_counts(&img);
    let (mut fs, env) = mount(&img, rw());
    let root = fs.root();
    let slow = fs.lookup(root, b"slow").unwrap();
    assert_eq!(fs.stat(slow).unwrap().blocks, 2);
    for name in ["blockdev", "chardev", "fifo", "fast", "slow"] {
        fs.unlink(root, name.as_bytes()).unwrap();
    }
    fs.sync().unwrap();
    fsck(&img);
    let (blocks, inodes) = free_counts(&img);
    assert_eq!(
        (blocks, inodes),
        (start.0 + 1, start.1 + 5),
        "the slow link's block"
    );
    let keep = fs.lookup(root, b"keep").unwrap();
    let mut buf = vec![0; 3000];
    fs.read_at(keep, 0, &mut buf).unwrap();
    assert_eq!(buf, vec![0x42; 3000]);
    assert!(env.lines().is_empty(), "{:?}", env.lines());
    fs.shutdown().unwrap();
}

#[test]
fn an_ea_block_goes_with_its_last_user() {
    let dir = staging("remove-ea");
    fs::write(dir.join("f"), b"one").unwrap();
    fs::write(dir.join("g"), b"two").unwrap();
    let img = mkfs("remove-ea", 1024, 4 << 10, &[], Some(&dir));
    let value = scratch("remove-ea-value", ".bin");
    fs::write(&value, Rng::new(9).bytes(600)).unwrap();
    // Too big for the inode, so it gets a block.
    debugfs_w(&img, &format!("ea_set -f {} f user.big", value.display()));
    let ea = field(&debugfs_stat(&img, "f").text, "File ACL");
    let ea: u64 = ea.split_whitespace().next().unwrap().parse().unwrap();
    assert!(ea > 0);
    // Share it with g: refcount 2, and g's block count includes it.
    debugfs_w(&img, &format!("set_inode_field g file_acl {ea}"));
    let g_blocks = debugfs_stat(&img, "g").blockcount;
    debugfs_w(&img, &format!("set_inode_field g blocks {}", g_blocks + 2));
    poke(&img, ea * 1024 + 4, &2u32.to_le_bytes());
    fsck(&img);
    let start = free_counts(&img);
    let (mut fs, env) = mount(&img, rw());
    let root = fs.root();
    fs.unlink(root, b"f").unwrap();
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(
        peek(&img, ea * 1024 + 4, 4),
        1u32.to_le_bytes(),
        "one user left"
    );
    assert_eq!(
        free_counts(&img),
        (start.0 + 1, start.1 + 1),
        "f's data block"
    );
    fs.unlink(root, b"g").unwrap();
    fs.sync().unwrap();
    fsck(&img);
    assert_eq!(
        free_counts(&img),
        (start.0 + 3, start.1 + 2),
        "g's and the EA block"
    );
    assert!(env.lines().is_empty(), "{:?}", env.lines());
    fs.shutdown().unwrap();
}

#[test]
fn hard_links_keep_the_inode_until_the_last_goes() {
    let dir = staging("remove-links");
    fs::write(dir.join("f"), b"shared").unwrap();
    let img = mkfs("remove-links", 1024, 4 << 10, &[], Some(&dir));
    debugfs_w(&img, "ln f f2");
    debugfs_w(&img, "set_inode_field f links_count 2");
    fsck(&img);
    let env = TestEnv::new();
    let mut fs = Ext2::mount(FileDisk::open(&img), Box::new(env.clone()), rw()).unwrap();
    let root = fs.root();
    let f = fs.lookup(root, b"f").unwrap();
    env.set_now(T0 + 10);
    fs.unlink(root, b"f").unwrap();
    let st = fs.stat(f).unwrap();
    assert_eq!((st.nlink, st.ctime), (1, T0 + 10));
    fs.sync().unwrap();
    fsck(&img);
    fs.unlink(root, b"f2").unwrap();
    assert_eq!(fs.stat(f), Err(Errno::ENOENT));
    fs.sync().unwrap();
    fsck(&img);
    let d = debugfs(&img, &format!("stat <{f}>"));
    assert!(d.contains("dtime:"), "{d}");
    fs.shutdown().unwrap();
}

#[test]
fn unlink_and_rmdir_follow_the_contract() {
    let img = mkfs("remove-contract", 1024, 4 << 10, &[], None);
    let env = TestEnv::new();
    let mut fs = Ext2::mount(FileDisk::open(&img), Box::new(env.clone()), rw()).unwrap();
    let root = fs.root();
    let d = fs.mkdir(root, b"d").unwrap();
    let sub = fs.mkdir(d, b"sub").unwrap();
    let f = fs.create(d, b"f").unwrap();
    assert_eq!(fs.unlink(root, b"d"), Err(Errno::EISDIR));
    assert_eq!(fs.rmdir(d, b"f"), Err(Errno::ENOTDIR));
    assert_eq!(fs.rmdir(root, b"d"), Err(Errno::ENOTEMPTY));
    assert_eq!(fs.unlink(root, b"missing"), Err(Errno::ENOENT));
    assert_eq!(fs.rmdir(root, b"missing"), Err(Errno::ENOENT));
    assert_eq!(fs.unlink(f, b"x"), Err(Errno::ENOTDIR));
    assert_eq!(fs.unlink(f, b""), Err(Errno::ENOENT), "the name first");
    assert_eq!(fs.unlink(900, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.unlink(root, b"a/b"), Err(Errno::EINVAL));
    assert_eq!(fs.rmdir(root, &[b'x'; 256]), Err(Errno::ENAMETOOLONG));
    let links = fs.stat(d).unwrap().nlink;
    env.set_now(T0 + 20);
    fs.rmdir(d, b"sub").unwrap();
    let st = fs.stat(d).unwrap();
    assert_eq!(
        (st.nlink, st.mtime, st.ctime),
        (links - 1, T0 + 20, T0 + 20)
    );
    // A stale number of a removed directory reaches nothing.
    assert_eq!(fs.stat(sub), Err(Errno::ENOENT));
    assert_eq!(fs.read_dir(sub), Err(Errno::ENOENT));
    assert_eq!(fs.create(sub, b"x"), Err(Errno::ENOENT));
    assert_eq!(fs.lookup(sub, b".."), Err(Errno::ENOENT));
    fs.unlink(d, b"f").unwrap();
    fs.rmdir(root, b"d").unwrap();
    fs.sync().unwrap();
    fsck(&img);
    fs.shutdown().unwrap();
    assert_eq!(
        fs.unlink(root, b"missing"),
        Err(Errno::EROFS),
        "EROFS before ENOENT"
    );
    assert_eq!(fs.rmdir(900, b"x"), Err(Errno::ENOENT));
}

/// Group 0's block bitmap and inode table (1 KiB blocks: the descriptor
/// table is at byte 2048).
fn group0_metadata(img: &Path) -> (u64, u64) {
    let d = peek(img, 2048, 12);
    let at = |i: usize| u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as u64;
    (at(0), at(8))
}

#[test]
fn removing_through_pointers_into_metadata_leaves_it_intact() {
    let dir = staging("remove-meta");
    fs::write(dir.join("victim"), vec![1u8; 20 << 10]).unwrap();
    fs::write(dir.join("ea"), b"").unwrap();
    let img = mkfs("remove-meta", 1024, 4 << 10, &[], Some(&dir));
    let (bitmap, table) = group0_metadata(&img);
    debugfs_w(&img, &format!("set_inode_field victim block[IND] {table}"));
    debugfs_w(&img, &format!("set_inode_field ea file_acl {bitmap}"));
    let victim = ino_of(&img, "victim");
    // The inode table but the root's and the victim's own inodes (the
    // unlink changes them), and the bitmap.
    let snapshot = |img: &Path| {
        let mut t = peek(img, table * 1024, 8 * 1024);
        for ino in [2, victim as usize] {
            t[(ino - 1) * 256..ino * 256].fill(0);
        }
        (t, peek(img, bitmap * 1024, 1024))
    };
    let (table_before, _) = snapshot(&img);
    let (mut fs, env) = mount(&img, rw());
    let root = fs.root();
    fs.unlink(root, b"victim").unwrap();
    assert!(env.logged(&format!(
        "inode {victim}: dropping block pointer {table}: outside the filesystem or metadata"
    )));
    fs.sync().unwrap();
    assert!(
        snapshot(&img).0 == table_before,
        "the inode table is intact"
    );
    let bitmap_before = snapshot(&img).1;
    let ea = fs.lookup(root, b"ea").unwrap();
    fs.unlink(root, b"ea").unwrap();
    assert!(env.logged(&format!("inode {ea}: bad EA block {bitmap}")));
    fs.sync().unwrap();
    assert!(
        snapshot(&img).1 == bitmap_before,
        "the block bitmap is intact"
    );
    assert!(fs.stat(root).is_ok());
    assert_eq!(fs.read_dir(root).unwrap().len(), 3, ". .. lost+found");
}

/// Changes everything reachable from `dir` (ignoring errors), then removes
/// it: files get a write and a truncate, directories a new file. Cycles in
/// a corrupt image are cut short.
fn change_and_remove(fs: &mut impl FileSystem, dir: Ino, seen: &mut Vec<Ino>) {
    if seen.contains(&dir) || seen.len() > 100 {
        return;
    }
    seen.push(dir);
    let _ = fs.create(dir, b"new-file");
    let Ok(entries) = fs.read_dir(dir) else {
        return;
    };
    for e in entries.iter().filter(|e| e.name != b"." && e.name != b"..") {
        match fs.stat(e.ino).map(|s| s.kind) {
            Ok(vfs::FileType::Directory) => {
                change_and_remove(fs, e.ino, seen);
                let _ = fs.rmdir(dir, &e.name);
            }
            Ok(_) => {
                let _ = fs.write_at(e.ino, 5000, b"more data");
                let _ = fs.truncate(e.ino, 100);
                let _ = fs.unlink(dir, &e.name);
            }
            Err(_) => {}
        }
    }
}

#[test]
fn seeded_corruption_under_writes_never_panics() {
    let dir = staging("remove-seeded");
    fs::create_dir_all(dir.join("a/b")).unwrap();
    fs::write(dir.join("a/b/deep"), b"deep\n").unwrap();
    fs::write(dir.join("a/big"), vec![3u8; 300 << 10]).unwrap();
    fs::create_dir(dir.join("many")).unwrap();
    for i in 0..60 {
        fs::write(dir.join(format!("many/file-number-{i}")), b"x").unwrap();
    }
    let img = mkfs("remove-seeded", 1024, 2 << 10, &[], Some(&dir));
    let pristine = fs::read(&img).unwrap();
    let used = sb_field(&img, "Block count") - sb_field(&img, "Free blocks");
    // Intact, everything goes and e2fsck agrees.
    let (mut intact, _) = mount(&img, rw());
    let root = intact.root();
    change_and_remove(&mut intact, root, &mut Vec::new());
    let left: Vec<_> = intact
        .read_dir(root)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(left.len(), 2, "only . and ..: {left:?}");
    intact.shutdown().unwrap();
    fsck(&img);
    // Corrupt: any result but a panic will do.
    let work = scratch("remove-seeded-work", ".img");
    let mut rng = Rng::new(0x5EED_0008);
    for round in 0..200 {
        let mut image = pristine.clone();
        let block = 1 + rng.below(used - 1) as usize;
        let at = block * 1024 + rng.below(256) as usize * 4;
        let value = [0, 1, 7, 0xFFFF_FFFF, rng.next() as u32][rng.below(5) as usize];
        println!("round {round}: block {block}, byte {at}, value {value:#x}");
        image[at..at + 4].copy_from_slice(&value.to_le_bytes());
        fs::write(&work, &image).unwrap();
        let (result, _) = try_mount(FileDisk::open(&work), rw());
        let Ok(mut fs) = result else {
            continue;
        };
        let root = fs.root();
        change_and_remove(&mut fs, root, &mut Vec::new());
        let _ = fs.shutdown();
    }
}

/// A directory entry whose inode is a directory living elsewhere (one
/// corrupt field; `debugfs link` makes it). Removing or moving it through
/// the alias would free or re-parent the real directory behind its real
/// parent's back, so it is refused as corrupt, and nothing changes.
#[test]
fn a_directory_reached_through_an_alias_is_not_removed_or_moved() {
    let img = mkfs("remove-alias", 1024, 4 << 10, &[], None);
    debugfs_w(&img, "mkdir keep");
    debugfs_w(&img, "mkdir junk");
    debugfs_w(&img, "link keep junk/evil");
    let keep = ino_of(&img, "/keep");
    let (mut fs, env) = mount(&img, rw());
    let root = fs.root();
    let junk = fs.lookup(root, b"junk").unwrap();
    assert_eq!(fs.lookup(junk, b"evil").unwrap(), keep);
    fs.mkdir(root, b"x").unwrap();
    let root_links = fs.stat(root).unwrap().nlink;
    assert_eq!(fs.rmdir(junk, b"evil"), Err(Errno::EIO));
    assert_eq!(fs.rename(junk, b"evil", root, b"moved"), Err(Errno::EIO));
    // Nor is it replaced through the alias.
    assert_eq!(fs.rename(root, b"x", junk, b"evil"), Err(Errno::EIO));
    assert!(env.logged("ext2:"), "{:?}", env.lines());
    assert_eq!(fs.lookup(root, b"keep").unwrap(), keep);
    assert_eq!(fs.stat(keep).unwrap().nlink, 2);
    assert_eq!(fs.stat(root).unwrap().nlink, root_links);
    assert_eq!(fs.lookup(keep, b"..").unwrap(), root);
    // The real directory still goes the normal way.
    fs.unlink(junk, b"evil").unwrap_err();
    fs.rmdir(root, b"keep").unwrap();
    fs.shutdown().unwrap();
}
