//! Inodes (spec §8.2): the raw on-disk inode and its fields, file types,
//! where an inode lives on the disk, and the block map's index math.

use crate::Ext2;
use crate::le::{set_u16, set_u32, u16_at, u32_at};
use alloc::vec::Vec;
use vfs::{BlockDevice, Errno, FileType, Ino, Stat};

/// Direct block pointers in `i_block`; then single, double and triple
/// indirect.
pub const N_DIRECT: usize = 12;
/// Bytes of `i_block`, which holds a fast symlink's target instead.
pub const I_BLOCK_BYTES: usize = 60;

const MODE: usize = 0;
const UID: usize = 2;
const SIZE: usize = 4;
const ATIME: usize = 8;
const CTIME: usize = 12;
const MTIME: usize = 16;
const GID: usize = 24;
const LINKS: usize = 26;
const BLOCKS: usize = 28;
const FLAGS: usize = 32;
const I_BLOCK: usize = 40;
const FILE_ACL: usize = 104;
const SIZE_HIGH: usize = 108;
const UID_HIGH: usize = 120;
const GID_HIGH: usize = 122;
// Large inodes only, as far as `i_extra_isize` reaches.
const EXTRA_ISIZE: usize = 128;
const CTIME_EXTRA: usize = 132;
const MTIME_EXTRA: usize = 136;
const ATIME_EXTRA: usize = 140;
const CRTIME: usize = 144;
/// What e2fsprogs gives new large inodes: every extra field up to
/// `i_crtime_extra`.
const NEW_EXTRA_ISIZE: u16 = 32;

/// `i_flags`: the directory has an htree index.
pub const INDEX_FL: u32 = 0x1000;

const S_IFMT: u16 = 0o170000;

/// The file type in an inode's mode; `None` for an unknown one.
pub fn kind_of(mode: u16) -> Option<FileType> {
    Some(match mode & S_IFMT {
        0o010000 => FileType::Fifo,
        0o020000 => FileType::CharDev,
        0o040000 => FileType::Directory,
        0o060000 => FileType::BlockDev,
        0o100000 => FileType::Regular,
        0o120000 => FileType::Symlink,
        0o140000 => FileType::Socket,
        _ => return None,
    })
}

/// An inode as stored on the disk (all `inode_size` bytes, so fields this
/// driver does not know survive a rewrite).
#[derive(Clone)]
pub struct Inode {
    pub ino: u32,
    raw: Vec<u8>,
}

impl Inode {
    /// A new inode of `size` bytes on the disk (spec §8.2): zeroed, then
    /// `mode`, one link (two for a directory), owned by root, all times
    /// `now`. Large inodes also get `i_extra_isize` and a creation time,
    /// as e2fsprogs gives them.
    pub fn fresh(ino: u32, size: usize, mode: u16, now: u32) -> Inode {
        let mut inode = Inode {
            ino,
            raw: alloc::vec![0; size],
        };
        set_u16(&mut inode.raw, MODE, mode);
        let links = if inode.kind() == Some(FileType::Directory) {
            2
        } else {
            1
        };
        inode.set_links(links);
        if size > EXTRA_ISIZE {
            set_u16(&mut inode.raw, EXTRA_ISIZE, NEW_EXTRA_ISIZE);
            set_u32(&mut inode.raw, CRTIME, now);
        }
        inode.set_time(ATIME, ATIME_EXTRA, now);
        inode.touch(now);
        inode
    }

    /// `raw` is at least the 128 bytes of a revision 0 inode.
    pub fn new(ino: u32, raw: &[u8]) -> Inode {
        assert!(raw.len() >= 128);
        Inode {
            ino,
            raw: raw.to_vec(),
        }
    }

    fn u16(&self, at: usize) -> u16 {
        u16_at(&self.raw, at)
    }

    fn u32(&self, at: usize) -> u32 {
        u32_at(&self.raw, at)
    }

    /// The inode as stored.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Whether the 4-byte field at `at` exists: past the first 128 bytes
    /// only as far as `i_extra_isize` says.
    fn has_extra(&self, at: usize) -> bool {
        self.raw.len() > EXTRA_ISIZE
            && at + 4 <= EXTRA_ISIZE + self.u16(EXTRA_ISIZE) as usize
            && at + 4 <= self.raw.len()
    }

    /// Sets a time in seconds. The matching `_extra` field (nanoseconds
    /// and epoch bits) is zeroed, so it cannot shift the new time.
    fn set_time(&mut self, at: usize, extra: usize, t: u32) {
        set_u32(&mut self.raw, at, t);
        if self.has_extra(extra) {
            set_u32(&mut self.raw, extra, 0);
        }
    }

    pub fn mode(&self) -> u16 {
        self.u16(MODE)
    }

    pub fn kind(&self) -> Option<FileType> {
        kind_of(self.mode())
    }

    pub fn uid(&self) -> u32 {
        self.u16(UID) as u32 | (self.u16(UID_HIGH) as u32) << 16
    }

    pub fn gid(&self) -> u32 {
        self.u16(GID) as u32 | (self.u16(GID_HIGH) as u32) << 16
    }

    /// Sets the size; only regular files keep the high 32 bits.
    pub fn set_size(&mut self, size: u64) {
        set_u32(&mut self.raw, SIZE, size as u32);
        if self.kind() == Some(FileType::Regular) {
            set_u32(&mut self.raw, SIZE_HIGH, (size >> 32) as u32);
        }
    }

    /// The size in bytes; only regular files use the high 32 bits.
    pub fn size(&self) -> u64 {
        let high = match self.kind() {
            Some(FileType::Regular) => self.u32(SIZE_HIGH) as u64,
            _ => 0,
        };
        self.u32(SIZE) as u64 | high << 32
    }

    pub fn links(&self) -> u16 {
        self.u16(LINKS)
    }

    pub fn set_links(&mut self, n: u16) {
        set_u16(&mut self.raw, LINKS, n);
    }

    pub fn flags(&self) -> u32 {
        self.u32(FLAGS)
    }

    pub fn set_flags(&mut self, flags: u32) {
        set_u32(&mut self.raw, FLAGS, flags);
    }

    /// Space used in 512-byte units, indirect and EA blocks included.
    pub fn blocks(&self) -> u32 {
        self.u32(BLOCKS)
    }

    pub fn set_blocks(&mut self, sectors: u32) {
        set_u32(&mut self.raw, BLOCKS, sectors);
    }

    /// Adds (or with a negative `delta`, removes) `i_blocks` for whole
    /// filesystem blocks of `block_size` bytes.
    pub fn add_blocks(&mut self, block_size: usize, delta: i32) {
        let sectors = (block_size / 512) as u32;
        let blocks = if delta >= 0 {
            self.blocks().saturating_add(sectors * delta as u32)
        } else {
            self.blocks().saturating_sub(sectors * delta.unsigned_abs())
        };
        self.set_blocks(blocks);
    }

    pub fn atime(&self) -> u32 {
        self.u32(ATIME)
    }

    pub fn ctime(&self) -> u32 {
        self.u32(CTIME)
    }

    pub fn mtime(&self) -> u32 {
        self.u32(MTIME)
    }

    pub fn set_ctime(&mut self, t: u32) {
        self.set_time(CTIME, CTIME_EXTRA, t);
    }

    pub fn set_mtime(&mut self, t: u32) {
        self.set_time(MTIME, MTIME_EXTRA, t);
    }

    /// A change to the data: mtime and ctime become `t`.
    pub fn touch(&mut self, t: u32) {
        self.set_mtime(t);
        self.set_ctime(t);
    }

    /// Entry `i` (0..15) of `i_block`.
    pub fn block(&self, i: usize) -> u32 {
        self.u32(I_BLOCK + 4 * i)
    }

    pub fn set_block(&mut self, i: usize, block: u32) {
        set_u32(&mut self.raw, I_BLOCK + 4 * i, block);
    }

    /// The raw `i_block` bytes: a fast symlink's target.
    pub fn block_bytes(&self) -> &[u8] {
        &self.raw[I_BLOCK..I_BLOCK + I_BLOCK_BYTES]
    }

    /// The extended attribute block, 0 if none.
    pub fn file_acl(&self) -> u32 {
        self.u32(FILE_ACL)
    }

    /// A symlink whose target lives in `i_block`: it has no blocks but an
    /// extended attribute block (Linux's rule).
    pub fn is_fast_symlink(&self, block_size: usize) -> bool {
        let ea_blocks = if self.file_acl() != 0 {
            block_size as u32 / 512
        } else {
            0
        };
        self.kind() == Some(FileType::Symlink) && self.blocks() == ea_blocks
    }
}

/// The largest file: what the block map reaches, limited (as Linux's ext2
/// limits it) so that a dense file's `i_blocks`, counting data and
/// indirect blocks in 512-byte units, fits in 32 bits.
pub fn max_file_size(block_size: usize) -> u64 {
    let bits = block_size.trailing_zeros();
    let p = 1u64 << (bits - 2);
    let limit = u32::MAX as u64 >> (bits - 9);
    let mapped = N_DIRECT as u64 + p + p * p + p * p * p;
    let meta = 1 + (1 + p) + (1 + p + p * p);
    let blocks = if mapped + meta <= limit {
        mapped
    } else {
        // The indirect blocks a file of `limit` blocks needs.
        let mut rest = limit - N_DIRECT as u64 - p;
        let mut meta = 1;
        if rest < p * p {
            meta += 1 + rest.div_ceil(p);
        } else {
            meta += 1 + p;
            rest -= p * p;
            meta += 1 + rest.div_ceil(p) + rest.div_ceil(p * p);
        }
        limit - meta
    };
    blocks << bits
}

/// Where a logical block sits in the block map: `index[0]` is the entry of
/// `i_block`, followed by one index per level of indirection (`depth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapPath {
    pub depth: usize,
    pub index: [usize; 4],
}

/// The path to logical block `lb` with `per_block` pointers per indirect
/// block; `None` past the triple indirect range.
pub fn map_path(lb: u64, per_block: u64) -> Option<MapPath> {
    let p = per_block;
    let mut rest = lb;
    if rest < N_DIRECT as u64 {
        return Some(MapPath {
            depth: 0,
            index: [rest as usize, 0, 0, 0],
        });
    }
    rest -= N_DIRECT as u64;
    let mut span = 1;
    for depth in 1..=3 {
        span *= p;
        if rest < span {
            let mut index = [N_DIRECT + depth - 1, 0, 0, 0];
            for (level, slot) in index.iter_mut().enumerate().skip(1).take(depth) {
                let below = p.pow((depth - level) as u32);
                *slot = (rest / below % p) as usize;
            }
            return Some(MapPath { depth, index });
        }
        rest -= span;
    }
    None
}

impl<D: BlockDevice> Ext2<D> {
    /// The block holding inode `ino` and its offset there.
    fn inode_location(&self, ino: u32) -> (u64, usize) {
        let index = ino - 1;
        let group = index / self.geo.inodes_per_group;
        let byte = (index % self.geo.inodes_per_group) as u64 * self.geo.inode_size as u64;
        let bs = self.geo.block_size as u64;
        let block = self.groups.inode_table(group) as u64 + byte / bs;
        (block, (byte % bs) as usize)
    }

    /// Inode `ino` as stored, in use or not. `ENOENT` for a number outside
    /// the filesystem.
    pub(crate) fn read_inode(&mut self, ino: Ino) -> Result<Inode, Errno> {
        let ino = match u32::try_from(ino) {
            Ok(n) if n >= 1 && n <= self.geo.inodes_count => n,
            _ => return Err(Errno::ENOENT),
        };
        let (block, at) = self.inode_location(ino);
        let size = self.geo.inode_size as usize;
        let data = self.cache.read(block)?;
        Ok(Inode::new(ino, &data[at..at + size]))
    }

    /// Inode `ino`, which must be in use: `ENOENT` otherwise (a stale
    /// number must never reach freed blocks), `EIO` for an unknown type.
    pub(crate) fn inode(&mut self, ino: Ino) -> Result<Inode, Errno> {
        let inode = self.read_inode(ino)?;
        if inode.links() == 0 {
            return Err(Errno::ENOENT);
        }
        if inode.kind().is_none() {
            return Err(self.corrupt(format_args!(
                "inode {ino}: unknown mode {:#o}",
                inode.mode()
            )));
        }
        // Callers never see a size no file can have.
        if inode.size() > max_file_size(self.geo.block_size) {
            return Err(self.corrupt(format_args!(
                "inode {ino}: size {} beyond the largest file",
                inode.size()
            )));
        }
        Ok(inode)
    }

    pub(crate) fn write_inode(&mut self, inode: &Inode) -> Result<(), Errno> {
        let (block, at) = self.inode_location(inode.ino);
        let raw = inode.raw();
        self.cache.write(block)?[at..at + raw.len()].copy_from_slice(raw);
        Ok(())
    }

    pub(crate) fn inode_stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let inode = self.inode(ino)?;
        Ok(Stat {
            ino,
            kind: inode.kind().expect("checked by inode()"),
            perm: inode.mode() & 0o7777,
            nlink: inode.links() as u32,
            uid: inode.uid(),
            gid: inode.gid(),
            size: inode.size(),
            blocks: inode.blocks() as u64,
            block_size: self.geo.block_size as u32,
            atime: inode.atime() as u64,
            mtime: inode.mtime() as u64,
            ctime: inode.ctime() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_inode(fields: &[(usize, &[u8])]) -> Inode {
        let mut raw = [0u8; 256];
        for (at, bytes) in fields {
            raw[*at..*at + bytes.len()].copy_from_slice(bytes);
        }
        Inode::new(12, &raw)
    }

    #[test]
    fn modes_give_file_types() {
        assert_eq!(kind_of(0o100644), Some(FileType::Regular));
        assert_eq!(kind_of(0o040755), Some(FileType::Directory));
        assert_eq!(kind_of(0o120777), Some(FileType::Symlink));
        assert_eq!(kind_of(0o020666), Some(FileType::CharDev));
        assert_eq!(kind_of(0o060660), Some(FileType::BlockDev));
        assert_eq!(kind_of(0o010644), Some(FileType::Fifo));
        assert_eq!(kind_of(0o140755), Some(FileType::Socket));
        assert_eq!(kind_of(0o000644), None);
        assert_eq!(kind_of(0o170644), None);
    }

    #[test]
    fn owners_and_sizes_combine_their_halves() {
        let file = raw_inode(&[
            (MODE, &0o100644u16.to_le_bytes()),
            (UID, &[0x34, 0x12]),
            (UID_HIGH, &[0x01, 0x00]),
            (GID, &[7, 0]),
            (SIZE, &[1, 0, 0, 0]),
            (SIZE_HIGH, &[2, 0, 0, 0]),
        ]);
        assert_eq!((file.uid(), file.gid()), (0x1_1234, 7));
        assert_eq!(file.size(), 0x2_0000_0001);
        let dir = raw_inode(&[
            (MODE, &0o040755u16.to_le_bytes()),
            (SIZE, &[0, 4, 0, 0]),
            (SIZE_HIGH, &[2, 0, 0, 0]),
        ]);
        assert_eq!(dir.size(), 1024, "the high half is not a directory's size");
    }

    #[test]
    fn setting_a_time_clears_its_extra_field_where_present() {
        let mut large = raw_inode(&[
            (EXTRA_ISIZE, &32u16.to_le_bytes()),
            (CTIME_EXTRA, &[0xFF; 4]),
            (MTIME_EXTRA, &[0xFF; 4]),
        ]);
        large.touch(1234);
        assert_eq!((large.mtime(), large.ctime()), (1234, 1234));
        assert_eq!(&large.raw()[CTIME_EXTRA..CTIME_EXTRA + 8], &[0; 8]);
        // `i_extra_isize` 4 covers neither.
        let mut small = raw_inode(&[(EXTRA_ISIZE, &4u16.to_le_bytes()), (CTIME_EXTRA, &[7; 4])]);
        small.set_ctime(9);
        assert_eq!(&small.raw()[CTIME_EXTRA..CTIME_EXTRA + 4], &[7; 4]);
    }

    #[test]
    fn sizes_and_block_counts_are_set() {
        let mut file = raw_inode(&[(MODE, &0o100644u16.to_le_bytes())]);
        file.set_size(0x3_0000_0010);
        assert_eq!(file.size(), 0x3_0000_0010);
        file.add_blocks(4096, 3);
        file.add_blocks(4096, -1);
        assert_eq!(file.blocks(), 16);
        file.add_blocks(1024, -100);
        assert_eq!(file.blocks(), 0, "saturates");
        let mut dir = raw_inode(&[(MODE, &0o040755u16.to_le_bytes())]);
        dir.set_size(0x1_0000_0400);
        assert_eq!(&dir.raw()[SIZE_HIGH..SIZE_HIGH + 4], &[0; 4]);
    }

    #[test]
    fn new_inodes_look_like_e2fsprogs_ones() {
        let file = Inode::fresh(12, 256, 0o100644, 777);
        assert_eq!(
            (file.kind(), file.mode(), file.links()),
            (Some(FileType::Regular), 0o100644, 1)
        );
        assert_eq!((file.atime(), file.mtime(), file.ctime()), (777, 777, 777));
        assert_eq!(
            (file.uid(), file.gid(), file.size(), file.blocks()),
            (0, 0, 0, 0)
        );
        assert_eq!(u16_at(file.raw(), EXTRA_ISIZE), 32);
        assert_eq!(u32_at(file.raw(), CRTIME), 777);
        let dir = Inode::fresh(13, 128, 0o040755, 5);
        assert_eq!((dir.links(), dir.raw().len()), (2, 128));
    }

    #[test]
    fn fast_symlinks_have_no_blocks_but_their_ea_block() {
        let link = |blocks: u32, acl: u32| {
            raw_inode(&[
                (MODE, &0o120777u16.to_le_bytes()),
                (BLOCKS, &blocks.to_le_bytes()),
                (FILE_ACL, &acl.to_le_bytes()),
            ])
        };
        assert!(link(0, 0).is_fast_symlink(1024));
        assert!(link(2, 99).is_fast_symlink(1024));
        assert!(!link(2, 0).is_fast_symlink(1024));
        assert!(!link(8, 99).is_fast_symlink(1024));
    }

    #[test]
    fn the_largest_file_is_linuxs() {
        assert_eq!(max_file_size(1024), 16_843_020 * 1024);
        assert_eq!(max_file_size(2048), 134_480_396 * 2048);
        assert_eq!(max_file_size(4096), 536_346_110 * 4096);
    }

    #[test]
    fn map_paths_cover_direct_to_triple_indirect() {
        let p = 256;
        let path = |lb| map_path(lb, p).map(|m| (m.depth, m.index));
        assert_eq!(path(0), Some((0, [0, 0, 0, 0])));
        assert_eq!(path(11), Some((0, [11, 0, 0, 0])));
        assert_eq!(path(12), Some((1, [12, 0, 0, 0])));
        assert_eq!(path(12 + 255), Some((1, [12, 255, 0, 0])));
        assert_eq!(path(12 + 256), Some((2, [13, 0, 0, 0])));
        assert_eq!(path(12 + 256 + 257), Some((2, [13, 1, 1, 0])));
        assert_eq!(path(12 + 256 + 65535), Some((2, [13, 255, 255, 0])));
        let triple = 12 + 256 + 65536;
        assert_eq!(path(triple), Some((3, [14, 0, 0, 0])));
        assert_eq!(path(triple + 65536 + 256 + 1), Some((3, [14, 1, 1, 1])));
        assert_eq!(
            path(triple + 256 * 65536 - 1),
            Some((3, [14, 255, 255, 255]))
        );
        assert_eq!(path(triple + 256 * 65536), None);
        assert_eq!(path(u64::MAX), None);
    }
}
