//! The superblock (spec §8.2): the fields ext2 uses, the feature bits and
//! the mount checks. The raw 1024 bytes are kept and patched in place, so
//! fields this driver does not know survive a rewrite.

use crate::le::{set_u16, set_u32, u16_at, u32_at};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

/// Where the superblock starts on the device, whatever the block size.
pub const SUPERBLOCK_OFFSET: u64 = 1024;
pub const SUPERBLOCK_SIZE: usize = 1024;
const MAGIC: u16 = 0xEF53;
/// The dynamic revision, the only one supported.
const DYNAMIC_REV: u32 = 1;
/// The root directory's inode.
pub const ROOT_INO: u32 = 2;
/// Bytes per group descriptor (filesystems without `64bit`).
pub const DESC_SIZE: u32 = 32;
/// The most groups mounted: it bounds the descriptor table kept in memory
/// (2 MiB) and every loop over the groups, and still allows 512 GiB with
/// 1 KiB blocks and 8 TiB with 4 KiB blocks.
const MAX_GROUPS: u32 = 1 << 16;

/// Backups only in the groups `s_backup_bgs` names.
const COMPAT_SPARSE_SUPER2: u32 = 0x200;
pub const INCOMPAT_FILETYPE: u32 = 0x2;
const INCOMPAT_SUPPORTED: u32 = INCOMPAT_FILETYPE;
pub const RO_COMPAT_SPARSE_SUPER: u32 = 0x1;
pub const RO_COMPAT_LARGE_FILE: u32 = 0x2;
const RO_COMPAT_SUPPORTED: u32 = RO_COMPAT_SPARSE_SUPER | RO_COMPAT_LARGE_FILE;

/// `s_state`: cleanly unmounted.
pub const STATE_VALID: u16 = 1;
/// `s_state`: errors were detected.
pub const STATE_ERROR: u16 = 2;

const INCOMPAT_NAMES: &[(u32, &str)] = &[
    (0x1, "compression"),
    (0x2, "filetype"),
    (0x4, "needs_recovery"),
    (0x8, "journal_dev"),
    (0x10, "meta_bg"),
    (0x40, "extent"),
    (0x80, "64bit"),
    (0x100, "mmp"),
    (0x200, "flex_bg"),
    (0x400, "ea_inode"),
    (0x1000, "dirdata"),
    (0x2000, "metadata_csum_seed"),
    (0x4000, "large_dir"),
    (0x8000, "inline_data"),
    (0x10000, "encrypt"),
    (0x20000, "casefold"),
];

const RO_COMPAT_NAMES: &[(u32, &str)] = &[
    (0x1, "sparse_super"),
    (0x2, "large_file"),
    (0x4, "btree_dir"),
    (0x8, "huge_file"),
    (0x10, "gdt_csum"),
    (0x20, "dir_nlink"),
    (0x40, "extra_isize"),
    (0x80, "has_snapshot"),
    (0x100, "quota"),
    (0x200, "bigalloc"),
    (0x400, "metadata_csum"),
    (0x800, "replica"),
    (0x1000, "read-only"),
    (0x2000, "project"),
    (0x8000, "verity"),
    (0x10000, "orphan_present"),
];

// Field offsets.
const INODES_COUNT: usize = 0;
const BLOCKS_COUNT: usize = 4;
const R_BLOCKS_COUNT: usize = 8;
const FREE_BLOCKS_COUNT: usize = 12;
const FREE_INODES_COUNT: usize = 16;
const FIRST_DATA_BLOCK: usize = 20;
const LOG_BLOCK_SIZE: usize = 24;
const BLOCKS_PER_GROUP: usize = 32;
const INODES_PER_GROUP: usize = 40;
const MTIME: usize = 44;
const WTIME: usize = 48;
const MNT_COUNT: usize = 52;
const MAGIC_AT: usize = 56;
const STATE: usize = 58;
const REV_LEVEL: usize = 76;
const FIRST_INO: usize = 84;
const INODE_SIZE: usize = 88;
const BLOCK_GROUP_NR: usize = 90;
const FEATURE_COMPAT: usize = 92;
const FEATURE_INCOMPAT: usize = 96;
const FEATURE_RO_COMPAT: usize = 100;
const RESERVED_GDT_BLOCKS: usize = 206;
const BACKUP_BGS: usize = 0x24C;

/// The superblock, with a note of whether it changed since it was last
/// written.
pub struct Superblock {
    raw: Box<[u8]>,
    dirty: bool,
}

impl Superblock {
    /// Wraps the first [`SUPERBLOCK_SIZE`] bytes of `raw`.
    pub fn new(raw: &[u8]) -> Superblock {
        Superblock {
            raw: raw[..SUPERBLOCK_SIZE].into(),
            dirty: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Copies the superblock into `buf` as the copy of group `group`
    /// (`s_block_group_nr`).
    pub fn copy_to(&self, buf: &mut [u8], group: u32) {
        buf[..SUPERBLOCK_SIZE].copy_from_slice(&self.raw);
        set_u16(buf, BLOCK_GROUP_NR, group as u16);
    }

    /// Notes that every copy has been written.
    pub fn set_clean(&mut self) {
        self.dirty = false;
    }

    fn set_u32(&mut self, at: usize, v: u32) {
        if self.u32(at) != v {
            set_u32(&mut self.raw, at, v);
            self.dirty = true;
        }
    }

    fn set_u16(&mut self, at: usize, v: u16) {
        if self.u16(at) != v {
            set_u16(&mut self.raw, at, v);
            self.dirty = true;
        }
    }

    fn u32(&self, at: usize) -> u32 {
        u32_at(&self.raw, at)
    }

    fn u16(&self, at: usize) -> u16 {
        u16_at(&self.raw, at)
    }

    /// Blocks reserved for root.
    pub fn r_blocks_count(&self) -> u32 {
        self.u32(R_BLOCKS_COUNT)
    }

    pub fn free_blocks_count(&self) -> u32 {
        self.u32(FREE_BLOCKS_COUNT)
    }

    pub fn set_free_blocks_count(&mut self, n: u32) {
        self.set_u32(FREE_BLOCKS_COUNT, n);
    }

    pub fn free_inodes_count(&self) -> u32 {
        self.u32(FREE_INODES_COUNT)
    }

    pub fn set_free_inodes_count(&mut self, n: u32) {
        self.set_u32(FREE_INODES_COUNT, n);
    }

    pub fn state(&self) -> u16 {
        self.u16(STATE)
    }

    pub fn set_state(&mut self, state: u16) {
        self.set_u16(STATE, state);
    }

    pub fn mnt_count(&self) -> u16 {
        self.u16(MNT_COUNT)
    }

    pub fn set_mnt_count(&mut self, n: u16) {
        self.set_u16(MNT_COUNT, n);
    }

    /// The last mount time.
    pub fn set_mtime(&mut self, t: u32) {
        self.set_u32(MTIME, t);
    }

    /// The last write time. Set as the superblock is written, so it does
    /// not count as a change of its own.
    pub fn stamp_wtime(&mut self, t: u32) {
        set_u32(&mut self.raw, WTIME, t);
    }

    pub fn feature_ro_compat(&self) -> u32 {
        self.u32(FEATURE_RO_COMPAT)
    }

    pub fn set_feature_ro_compat(&mut self, bits: u32) {
        self.set_u32(FEATURE_RO_COMPAT, bits);
    }

    /// The ro_compat features this driver cannot keep consistent, which
    /// make it mount read-only.
    pub fn unsupported_ro_compat(&self) -> u32 {
        self.feature_ro_compat() & !RO_COMPAT_SUPPORTED
    }
}

/// Which groups hold a backup of the superblock and group descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backups {
    /// Every group.
    All,
    /// `sparse_super`: groups 1 and the powers of 3, 5 and 7.
    Sparse,
    /// `sparse_super2`: the (up to) two groups listed; 0 means none.
    Listed([u32; 2]),
}

/// What the mount checks derive from a valid superblock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub block_size: usize,
    pub blocks_count: u32,
    pub inodes_count: u32,
    pub first_data_block: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: u32,
    /// The first inode not reserved for the filesystem itself.
    pub first_ino: u32,
    pub groups: u32,
    /// Blocks of the group descriptor table.
    pub gdt_blocks: u32,
    /// Blocks reserved after each descriptor table copy for growing it
    /// (`resize_inode`).
    pub reserved_gdt_blocks: u32,
    /// Blocks of each group's inode table.
    pub inode_table_blocks: u32,
    pub backups: Backups,
    /// Directory entries carry a file type byte.
    pub filetype: bool,
}

impl Geometry {
    /// The first block of group `g`.
    pub fn group_start(&self, g: u32) -> u64 {
        self.first_data_block as u64 + g as u64 * self.blocks_per_group as u64
    }

    /// The blocks in group `g`: the last group may be short.
    pub fn group_blocks(&self, g: u32) -> u32 {
        let end = self.blocks_count as u64 - self.group_start(g);
        end.min(self.blocks_per_group as u64) as u32
    }

    /// Whether group `g` starts with a copy of the superblock and the
    /// group descriptors (group 0 holds the primary ones).
    pub fn has_super(&self, g: u32) -> bool {
        let power_of = |base: u32| {
            let mut n = 1;
            while n < g {
                n = n.saturating_mul(base);
            }
            n == g
        };
        match self.backups {
            _ if g == 0 => true,
            Backups::All => true,
            Backups::Sparse => power_of(3) || power_of(5) || power_of(7),
            Backups::Listed(groups) => groups.contains(&g),
        }
    }

    /// Blocks at the start of group `g` taken by its superblock and
    /// descriptor copy, reserved descriptor blocks included; 0 without one.
    pub fn super_blocks(&self, g: u32) -> u32 {
        if self.has_super(g) {
            1 + self.gdt_blocks + self.reserved_gdt_blocks
        } else {
            0
        }
    }

    /// Where group `g`'s copy of the superblock is: its first block, at
    /// byte 1024 of block 0 for the primary copy with blocks over 1 KiB.
    pub fn super_location(&self, g: u32) -> (u64, usize) {
        if g == 0 {
            let offset = SUPERBLOCK_OFFSET as usize;
            ((offset / self.block_size) as u64, offset % self.block_size)
        } else {
            (self.group_start(g), 0)
        }
    }

    /// Blocks holding metadata, as Linux counts them for `statfs`: the
    /// blocks before the first group, superblock and descriptor copies,
    /// bitmaps and inode tables (reserved descriptor blocks are not).
    pub fn overhead(&self) -> u64 {
        let copies = (0..self.groups).filter(|&g| self.has_super(g)).count() as u64;
        self.first_data_block as u64
            + copies * (1 + self.gdt_blocks as u64)
            + self.groups as u64 * (2 + self.inode_table_blocks as u64)
    }
}

/// Checks the superblock of a filesystem on a device of `device_bytes`
/// (spec §8.2). The error is the reason, for the log.
pub fn check(sb: &Superblock, device_bytes: u64) -> Result<Geometry, String> {
    if sb.u16(MAGIC_AT) != MAGIC {
        return Err(format!("bad magic number {:#06x}", sb.u16(MAGIC_AT)));
    }
    let rev = sb.u32(REV_LEVEL);
    if rev != DYNAMIC_REV {
        return Err(format!("revision {rev} is not supported (only 1)"));
    }
    let incompat = sb.u32(FEATURE_INCOMPAT) & !INCOMPAT_SUPPORTED;
    if incompat != 0 {
        let names = feature_names(incompat, INCOMPAT_NAMES);
        return Err(format!("unsupported incompat features: {names}"));
    }
    let log = sb.u32(LOG_BLOCK_SIZE);
    if log > 2 {
        return Err(format!("block size 2^(10+{log}) is not supported"));
    }
    let block_size = 1024u32 << log;
    let inode_size = sb.u16(INODE_SIZE) as u32;
    if !inode_size.is_power_of_two() || inode_size < 128 || inode_size > block_size {
        return Err(format!("bad inode size {inode_size}"));
    }
    let blocks = sb.u32(BLOCKS_COUNT);
    let inodes = sb.u32(INODES_COUNT);
    let first_data_block = sb.u32(FIRST_DATA_BLOCK);
    // The superblock is always in the first data block.
    if first_data_block != (block_size == 1024) as u32 {
        return Err(format!("bad first data block {first_data_block}"));
    }
    if blocks <= first_data_block {
        return Err(format!("bad block count {blocks}"));
    }
    let bpg = sb.u32(BLOCKS_PER_GROUP);
    let ipg = sb.u32(INODES_PER_GROUP);
    // At least e2fsck's minimum; mke2fs makes 8 × the block size.
    if bpg < 8 || bpg > 8 * block_size {
        return Err(format!("bad blocks per group {bpg}"));
    }
    if ipg == 0 || ipg > 8 * block_size {
        return Err(format!("bad inodes per group {ipg}"));
    }
    let groups = (blocks - first_data_block).div_ceil(bpg);
    if groups > MAX_GROUPS {
        return Err(format!("too many groups ({groups}, at most {MAX_GROUPS})"));
    }
    if groups as u64 * ipg as u64 != inodes as u64 {
        return Err(format!(
            "inode count {inodes} is not {groups} groups of {ipg}"
        ));
    }
    let first_ino = sb.u32(FIRST_INO);
    if first_ino <= ROOT_INO || first_ino > inodes {
        return Err(format!("bad first inode {first_ino}"));
    }
    let fs_bytes = blocks as u64 * block_size as u64;
    if fs_bytes > device_bytes {
        return Err(format!(
            "filesystem of {fs_bytes} bytes is larger than the device ({device_bytes})"
        ));
    }
    let gdt_blocks = (groups as u64 * DESC_SIZE as u64).div_ceil(block_size as u64);
    if first_data_block as u64 + 1 + gdt_blocks > blocks as u64 {
        return Err(String::from("group descriptors outside the filesystem"));
    }
    let backups = if sb.u32(FEATURE_COMPAT) & COMPAT_SPARSE_SUPER2 != 0 {
        Backups::Listed([sb.u32(BACKUP_BGS), sb.u32(BACKUP_BGS + 4)])
    } else if sb.feature_ro_compat() & RO_COMPAT_SPARSE_SUPER != 0 {
        Backups::Sparse
    } else {
        Backups::All
    };
    Ok(Geometry {
        block_size: block_size as usize,
        blocks_count: blocks,
        inodes_count: inodes,
        first_data_block,
        blocks_per_group: bpg,
        inodes_per_group: ipg,
        inode_size,
        first_ino,
        groups,
        gdt_blocks: gdt_blocks as u32,
        reserved_gdt_blocks: sb.u16(RESERVED_GDT_BLOCKS) as u32,
        inode_table_blocks: (ipg * inode_size).div_ceil(block_size),
        backups,
        filetype: sb.u32(FEATURE_INCOMPAT) & INCOMPAT_FILETYPE != 0,
    })
}

/// The names of the features in `bits`, e.g. `extent 64bit`; unknown ones
/// in hex.
pub fn ro_compat_names(bits: u32) -> String {
    feature_names(bits, RO_COMPAT_NAMES)
}

fn feature_names(bits: u32, table: &[(u32, &str)]) -> String {
    let mut out = String::new();
    for i in 0..32 {
        let bit = 1u32 << i;
        if bits & bit == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        match table.iter().find(|(b, _)| *b == bit) {
            Some((_, name)) => out.push_str(name),
            None => out.push_str(&format!("{bit:#x}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// A 1 KiB-block filesystem of 20000 blocks: 3 groups of 8192 blocks
    /// and 512 inodes.
    fn sample() -> [u8; SUPERBLOCK_SIZE] {
        let mut b = [0u8; SUPERBLOCK_SIZE];
        set_u32(&mut b, INODES_COUNT, 1536);
        set_u32(&mut b, BLOCKS_COUNT, 20000);
        set_u32(&mut b, FIRST_DATA_BLOCK, 1);
        set_u32(&mut b, BLOCKS_PER_GROUP, 8192);
        set_u32(&mut b, INODES_PER_GROUP, 512);
        set_u16(&mut b, MAGIC_AT, MAGIC);
        set_u32(&mut b, REV_LEVEL, 1);
        set_u32(&mut b, FIRST_INO, 11);
        set_u16(&mut b, INODE_SIZE, 256);
        set_u32(&mut b, FEATURE_INCOMPAT, INCOMPAT_FILETYPE);
        b
    }

    const DEVICE: u64 = 20000 * 1024;

    fn check_with(patch: impl FnOnce(&mut [u8])) -> Result<Geometry, String> {
        let mut b = sample();
        patch(&mut b);
        check(&Superblock::new(&b), DEVICE)
    }

    #[test]
    fn a_valid_superblock_gives_the_geometry() {
        let geo = check_with(|_| {}).unwrap();
        assert_eq!(geo.block_size, 1024);
        assert_eq!(geo.groups, 3);
        assert_eq!(geo.gdt_blocks, 1);
        assert_eq!(geo.inode_table_blocks, 128);
        assert_eq!(geo.group_start(2), 16385);
        assert_eq!(geo.group_blocks(0), 8192);
        assert_eq!(geo.group_blocks(2), 20000 - 16385);
        assert_eq!(geo.backups, Backups::All);
        assert_eq!(geo.super_blocks(2), 2);
        assert!(geo.filetype);
        // 1 block before group 0, 3 × (superblock + descriptors), 3 × (two
        // bitmaps + 128 inode table blocks).
        assert_eq!(geo.overhead(), 1 + 3 * 2 + 3 * 130);
    }

    #[test]
    fn backups_follow_sparse_super_and_sparse_super2() {
        let mut geo =
            check_with(|b| set_u32(b, FEATURE_RO_COMPAT, RO_COMPAT_SPARSE_SUPER)).unwrap();
        assert_eq!(geo.backups, Backups::Sparse);
        let with: Vec<u32> = (0..400).filter(|&g| geo.has_super(g)).collect();
        assert_eq!(with, [0, 1, 3, 5, 7, 9, 25, 27, 49, 81, 125, 243, 343]);
        assert_eq!((geo.super_blocks(1), geo.super_blocks(2)), (2, 0));
        geo.backups = Backups::Listed([5, 0]);
        let with: Vec<u32> = (0..400).filter(|&g| geo.has_super(g)).collect();
        assert_eq!(with, [0, 5]);
        let geo = check_with(|b| {
            set_u32(b, FEATURE_COMPAT, COMPAT_SPARSE_SUPER2);
            set_u32(b, BACKUP_BGS, 1);
            set_u32(b, BACKUP_BGS + 4, 2);
            b[RESERVED_GDT_BLOCKS] = 3;
        })
        .unwrap();
        assert_eq!(geo.backups, Backups::Listed([1, 2]));
        assert!(geo.has_super(2));
        assert_eq!(geo.super_blocks(2), 5);
    }

    #[test]
    fn copies_sit_at_the_start_of_their_group() {
        let geo = check_with(|_| {}).unwrap();
        assert_eq!(geo.super_location(0), (1, 0));
        assert_eq!(geo.super_location(2), (16385, 0));
        let geo4k = Geometry {
            block_size: 4096,
            first_data_block: 0,
            blocks_per_group: 32768,
            ..geo
        };
        assert_eq!(geo4k.super_location(0), (0, 1024));
        assert_eq!(geo4k.super_location(1), (32768, 0));
    }

    #[test]
    fn only_real_changes_make_the_superblock_dirty() {
        let mut sb = Superblock::new(&sample());
        sb.set_state(0);
        sb.stamp_wtime(5);
        assert!(!sb.is_dirty());
        sb.set_state(STATE_VALID);
        sb.set_mnt_count(3);
        assert!(sb.is_dirty());
        let mut copy = [0u8; SUPERBLOCK_SIZE];
        sb.copy_to(&mut copy, 7);
        sb.set_clean();
        assert!(!sb.is_dirty());
        assert_eq!(u16_at(&copy, STATE), STATE_VALID);
        assert_eq!(u16_at(&copy, BLOCK_GROUP_NR), 7);
        assert_eq!(u32_at(&copy, WTIME), 5);
        assert_eq!(sb.mnt_count(), 3);
    }

    #[test]
    fn each_bad_field_is_refused_with_a_reason() {
        type Patch = fn(&mut [u8]);
        let cases: [(Patch, &str); 14] = [
            (|b| set_u16(b, MAGIC_AT, 0x1234), "magic"),
            (|b| set_u32(b, REV_LEVEL, 0), "revision"),
            (
                |b| set_u32(b, FEATURE_INCOMPAT, 0x2C2),
                "extent 64bit flex_bg",
            ),
            (|b| set_u32(b, LOG_BLOCK_SIZE, 3), "block size"),
            (|b| set_u16(b, INODE_SIZE, 200), "inode size"),
            (|b| set_u16(b, INODE_SIZE, 2048), "inode size"),
            (|b| set_u32(b, FIRST_DATA_BLOCK, 0), "first data block"),
            (|b| set_u32(b, BLOCKS_PER_GROUP, 0), "blocks per group"),
            (|b| set_u32(b, BLOCKS_PER_GROUP, 4), "blocks per group"),
            (
                |b| {
                    set_u32(b, BLOCKS_PER_GROUP, 8);
                    set_u32(b, BLOCKS_COUNT, 8 * 70_000 + 1);
                },
                "too many groups (70000",
            ),
            (|b| set_u32(b, INODES_PER_GROUP, 9000), "inodes per group"),
            (|b| set_u32(b, INODES_COUNT, 1000), "inode count"),
            (|b| set_u32(b, FIRST_INO, 2), "first inode"),
            (
                |b| set_u32(b, BLOCKS_COUNT, 24577),
                "larger than the device",
            ),
        ];
        for (patch, reason) in cases {
            let err = check_with(patch).unwrap_err();
            assert!(err.contains(reason), "{err:?} should mention {reason:?}");
        }
    }

    #[test]
    fn feature_names_are_listed_with_unknown_bits_in_hex() {
        assert_eq!(ro_compat_names(0x8 | 0x400), "huge_file metadata_csum");
        assert_eq!(ro_compat_names(0x4000_0000), "0x40000000");
        let mut b = sample();
        set_u32(&mut b, FEATURE_RO_COMPAT, 0x3 | 0x8);
        assert_eq!(Superblock::new(&b).unsupported_ro_compat(), 0x8);
    }
}
