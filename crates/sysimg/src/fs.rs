//! `SysImgFs`: the archive as a read-only filesystem (spec §4.2), read in
//! place without copying. The root directory lists the entries; every
//! change is `EROFS`; the file times are the archive's build time.

use crate::format::{Archive, Entry, SysImgError};
use alloc::vec::Vec;
use vfs::{DirEntry, Errno, FileSystem, FileType, Ino, Stat, StatFs};

/// The root directory's inode; entry `i` is inode `FIRST_FILE + i`.
pub const ROOT: Ino = 1;
const FIRST_FILE: Ino = 2;
/// The block size `stat` and `statfs` report: the data alignment.
const BLOCK: u32 = 4096;

pub struct SysImgFs {
    archive: Archive<'static>,
}

enum Node {
    Root,
    File(Entry<'static>),
}

impl SysImgFs {
    /// Checks `bytes` (see `Archive::parse`) and serves them.
    pub fn new(bytes: &'static [u8]) -> Result<SysImgFs, SysImgError> {
        Ok(SysImgFs {
            archive: Archive::parse(bytes)?,
        })
    }

    pub fn archive(&self) -> &Archive<'static> {
        &self.archive
    }

    fn node(&self, ino: Ino) -> Result<Node, Errno> {
        if ino == ROOT {
            return Ok(Node::Root);
        }
        let i = ino
            .checked_sub(FIRST_FILE)
            .and_then(|i| usize::try_from(i).ok())
            .ok_or(Errno::ENOENT)?;
        let e = self.archive.entry(i).ok_or(Errno::ENOENT)?;
        Ok(Node::File(e))
    }

    fn file_ino(i: usize) -> Ino {
        FIRST_FILE + i as Ino
    }

    /// Every change: `ENOENT` for an inode not in use, `EROFS` otherwise
    /// (the order of the `FileSystem` contract).
    fn refuse(&self, inos: &[Ino]) -> Errno {
        match inos.iter().find(|&&i| self.node(i).is_err()) {
            Some(_) => Errno::ENOENT,
            None => Errno::EROFS,
        }
    }
}

impl FileSystem for SysImgFs {
    fn root(&self) -> Ino {
        ROOT
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let t = self.archive.build_time();
        let (kind, perm, nlink, size) = match self.node(ino)? {
            Node::Root => (FileType::Directory, 0o755, 2, u64::from(BLOCK)),
            Node::File(e) => (FileType::Regular, e.mode, 1, e.data.len() as u64),
        };
        Ok(Stat {
            ino,
            kind,
            perm,
            nlink,
            uid: 0,
            gid: 0,
            size,
            blocks: size.div_ceil(512),
            block_size: BLOCK,
            atime: t,
            mtime: t,
            ctime: t,
        })
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        if let Node::File(_) = self.node(dir)? {
            return Err(Errno::ENOTDIR);
        }
        if name == b"." || name == b".." {
            return Ok(ROOT);
        }
        self.archive
            .find(name)
            .map(Self::file_ino)
            .ok_or(Errno::ENOENT)
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        if let Node::File(_) = self.node(dir)? {
            return Err(Errno::ENOTDIR);
        }
        let mut out = alloc::vec![
            DirEntry {
                name: b".".to_vec(),
                ino: ROOT,
            },
            DirEntry {
                name: b"..".to_vec(),
                ino: ROOT,
            },
        ];
        out.extend(self.archive.entries().enumerate().map(|(i, e)| DirEntry {
            name: e.name.to_vec(),
            ino: Self::file_ino(i),
        }));
        Ok(out)
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.node(ino)?;
        Err(Errno::EINVAL)
    }

    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        let data = match self.node(ino)? {
            Node::Root => return Err(Errno::EISDIR),
            Node::File(e) => e.data,
        };
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        let rest = data.get(start..).unwrap_or(&[]);
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }

    fn write_at(&mut self, ino: Ino, _offset: u64, _buf: &[u8]) -> Result<usize, Errno> {
        Err(self.refuse(&[ino]))
    }

    fn truncate(&mut self, ino: Ino, _size: u64) -> Result<(), Errno> {
        Err(self.refuse(&[ino]))
    }

    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        Err(self.refuse(&[ino]))
    }

    fn create(&mut self, dir: Ino, _name: &[u8]) -> Result<Ino, Errno> {
        Err(self.refuse(&[dir]))
    }

    fn mkdir(&mut self, dir: Ino, _name: &[u8]) -> Result<Ino, Errno> {
        Err(self.refuse(&[dir]))
    }

    fn unlink(&mut self, dir: Ino, _name: &[u8]) -> Result<(), Errno> {
        Err(self.refuse(&[dir]))
    }

    fn rmdir(&mut self, dir: Ino, _name: &[u8]) -> Result<(), Errno> {
        Err(self.refuse(&[dir]))
    }

    fn rename(
        &mut self,
        from_dir: Ino,
        _from: &[u8],
        to_dir: Ino,
        _to: &[u8],
    ) -> Result<(), Errno> {
        Err(self.refuse(&[from_dir, to_dir]))
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        let files = self.archive.len() as u64 + 1;
        Ok(StatFs {
            block_size: u64::from(BLOCK),
            blocks: self.archive.total_len().div_ceil(u64::from(BLOCK)),
            free_blocks: 0,
            avail_blocks: 0,
            files,
            free_files: 0,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::write;
    use vfs::{Env, MemFs};

    const BUILT: u64 = 1_790_000_000;

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            BUILT
        }
        fn log(&self, _line: &str) {}
    }

    const FILES: [(&str, u16, &[u8]); 3] = [
        ("t-args", 0o755, b"\x7fELF arguments"),
        ("empty", 0o644, b""),
        ("cat", 0o700, b"\x7fELF cat"),
    ];

    fn fs() -> SysImgFs {
        let files: Vec<Entry<'_>> = FILES
            .iter()
            .map(|&(name, mode, data)| Entry {
                name: name.as_bytes(),
                mode,
                data,
            })
            .collect();
        let bytes = write(1, BUILT, &files).unwrap();
        SysImgFs::new(Box::leak(bytes.into_boxed_slice())).unwrap()
    }

    #[test]
    fn a_bad_archive_is_not_served() {
        let bytes: &'static [u8] = b"not an archive, too short";
        assert_eq!(SysImgFs::new(bytes).err(), Some(SysImgError::TooShort(25)));
    }

    #[test]
    fn the_root_lists_every_program() {
        let mut fs = fs();
        let mut names: Vec<Vec<u8>> = fs
            .read_dir(ROOT)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        assert_eq!(names, [&b"."[..], b"..", b"cat", b"empty", b"t-args"]);
        for e in fs.read_dir(ROOT).unwrap() {
            assert_eq!(fs.lookup(ROOT, &e.name), Ok(e.ino), "{:?}", e.name);
        }
        assert_eq!(fs.lookup(ROOT, b"ls"), Err(Errno::ENOENT));
        let cat = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.lookup(cat, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(cat), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(99), Err(Errno::ENOENT));
    }

    #[test]
    fn stat_shows_modes_sizes_and_the_build_time() {
        let mut fs = fs();
        let root = fs.stat(ROOT).unwrap();
        assert_eq!(
            (root.kind, root.perm, root.nlink),
            (FileType::Directory, 0o755, 2)
        );
        let args = fs.lookup(ROOT, b"t-args").unwrap();
        let s = fs.stat(args).unwrap();
        assert_eq!(s.ino, args);
        assert_eq!((s.kind, s.perm, s.nlink), (FileType::Regular, 0o755, 1));
        assert_eq!((s.uid, s.gid), (0, 0));
        assert_eq!(s.size, 14);
        assert_eq!((s.blocks, s.block_size), (1, 4096));
        assert_eq!((s.atime, s.mtime, s.ctime), (BUILT, BUILT, BUILT));
        let cat = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.stat(cat).unwrap().perm, 0o700);
        for bad in [0, FIRST_FILE + 3, u64::MAX] {
            assert_eq!(fs.stat(bad), Err(Errno::ENOENT), "{bad}");
        }
    }

    #[test]
    fn files_read_like_any_other() {
        let mut fs = fs();
        let args = fs.lookup(ROOT, b"t-args").unwrap();
        let mut buf = [0u8; 64];
        assert_eq!(fs.read_at(args, 0, &mut buf), Ok(14));
        assert_eq!(&buf[..14], b"\x7fELF arguments");
        assert_eq!(fs.read_at(args, 5, &mut buf[..4]), Ok(4));
        assert_eq!(&buf[..4], b"argu");
        assert_eq!(fs.read_at(args, 14, &mut buf), Ok(0));
        assert_eq!(fs.read_at(args, 1 << 40, &mut buf), Ok(0));
        assert_eq!(fs.read_at(args, u64::MAX, &mut buf), Ok(0));
        assert_eq!(fs.read_at(ROOT, 0, &mut buf), Err(Errno::EISDIR));
        assert_eq!(fs.read_link(args), Err(Errno::EINVAL));
        assert_eq!(fs.read_link(77), Err(Errno::ENOENT));
    }

    #[test]
    fn every_change_is_refused() {
        let mut fs = fs();
        let f = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.write_at(f, 0, b"x"), Err(Errno::EROFS));
        assert_eq!(fs.truncate(f, 0), Err(Errno::EROFS));
        assert_eq!(fs.touch(f), Err(Errno::EROFS));
        assert_eq!(fs.create(ROOT, b"new"), Err(Errno::EROFS));
        assert_eq!(fs.mkdir(ROOT, b"d"), Err(Errno::EROFS));
        assert_eq!(fs.unlink(ROOT, b"cat"), Err(Errno::EROFS));
        assert_eq!(fs.rmdir(ROOT, b"cat"), Err(Errno::EROFS));
        assert_eq!(fs.rename(ROOT, b"cat", ROOT, b"dog"), Err(Errno::EROFS));
        assert_eq!(fs.touch(42), Err(Errno::ENOENT), "an unknown inode first");
        assert_eq!(fs.rename(ROOT, b"cat", 42, b"dog"), Err(Errno::ENOENT));
        assert_eq!(fs.sync(), Ok(()));
        assert_eq!(fs.shutdown(), Ok(()));
        let mut buf = [0u8; 8];
        assert_eq!(fs.read_at(f, 0, &mut buf), Ok(8), "still readable");
    }

    #[test]
    fn statfs_counts_the_archive_as_full() {
        let mut fs = fs();
        let blocks = fs.archive().total_len().div_ceil(4096);
        assert_eq!(
            fs.statfs(),
            Ok(StatFs {
                block_size: 4096,
                blocks,
                free_blocks: 0,
                avail_blocks: 0,
                files: 4,
                free_files: 0,
            })
        );
    }

    /// The same files in a read-only `MemFs`, operation by operation with
    /// seeded random inodes, names and offsets: every result must agree,
    /// errors included (inodes compared by what they name).
    #[test]
    fn it_answers_as_a_read_only_memfs_does() {
        let mut ours = fs();
        let mut mem = MemFs::new(Box::new(Clock));
        for (name, _, data) in FILES {
            let ino = mem.create(mem.root(), name.as_bytes()).unwrap();
            mem.write_at(ino, 0, data).unwrap();
        }
        let mut mem = mem.read_only();
        let names: [&[u8]; 7] = [b"t-args", b"empty", b"cat", b"ls", b".", b"..", b"x/y"];
        let ino_of = |fs: &mut dyn FileSystem, k: usize| -> Ino {
            match k {
                0..=2 => fs.lookup(fs.root(), names[k]).unwrap(),
                3 => fs.root(),
                _ => 1000 + k as Ino,
            }
        };
        let ours_inos: Vec<Ino> = (0..5).map(|k| ino_of(&mut ours, k)).collect();
        let mem_inos: Vec<Ino> = (0..5).map(|k| ino_of(&mut mem, k)).collect();
        // An inode result as the index of what it names.
        let index =
            |inos: &[Ino], r: Result<Ino, Errno>| r.map(|i| inos.iter().position(|&x| x == i));
        let mut seed = 0x0123_4567_89AB_CDEFu64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..5_000 {
            let k = rnd() as usize % 5;
            let k2 = rnd() as usize % 5;
            let name = names[rnd() as usize % names.len()];
            let offset = [0, 3, 14, 15, 16, 4096, u64::MAX][rnd() as usize % 7];
            let len = rnd() as usize % 20;
            let (a, b) = (ours_inos[k], mem_inos[k]);
            let (a2, b2) = (ours_inos[k2], mem_inos[k2]);
            match rnd() % 12 {
                0 => assert_eq!(
                    index(&ours_inos, ours.lookup(a, name)),
                    index(&mem_inos, mem.lookup(b, name)),
                    "lookup {k} {name:?}"
                ),
                1 => {
                    let mut x = vec![0; len];
                    let mut y = vec![0; len];
                    assert_eq!(
                        ours.read_at(a, offset, &mut x),
                        mem.read_at(b, offset, &mut y),
                        "read {k}"
                    );
                    assert_eq!(x, y);
                }
                2 => {
                    let names_of = |fs: &mut dyn FileSystem, ino| {
                        fs.read_dir(ino).map(|v| {
                            let mut n: Vec<Vec<u8>> = v.into_iter().map(|e| e.name).collect();
                            n.sort();
                            n
                        })
                    };
                    assert_eq!(
                        names_of(&mut ours, a),
                        names_of(&mut mem, b),
                        "read_dir {k}"
                    );
                }
                3 => assert_eq!(
                    ours.stat(a).map(|s| (s.kind, s.size)),
                    mem.stat(b).map(|s| (s.kind, s.size)),
                    "stat {k}"
                ),
                4 => assert_eq!(ours.read_link(a), mem.read_link(b), "read_link {k}"),
                5 => assert_eq!(
                    ours.write_at(a, offset, b"zz"),
                    mem.write_at(b, offset, b"zz"),
                    "write {k}"
                ),
                6 => assert_eq!(
                    ours.truncate(a, offset),
                    mem.truncate(b, offset),
                    "truncate {k}"
                ),
                7 => assert_eq!(ours.touch(a), mem.touch(b), "touch {k}"),
                8 => assert_eq!(
                    index(&ours_inos, ours.create(a, name)),
                    index(&mem_inos, mem.create(b, name)),
                    "create {k} {name:?}"
                ),
                9 => assert_eq!(
                    ours.unlink(a, name),
                    mem.unlink(b, name),
                    "unlink {k} {name:?}"
                ),
                10 => assert_eq!(
                    ours.rmdir(a, name),
                    mem.rmdir(b, name),
                    "rmdir {k} {name:?}"
                ),
                _ => assert_eq!(
                    ours.rename(a, name, a2, b"n"),
                    mem.rename(b, name, b2, b"n"),
                    "rename {k} {name:?} {k2}"
                ),
            }
        }
    }
}
