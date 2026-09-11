use crate::block::BlockDevice;

use super::{
    Ext2Error, MountMode,
    on_disk::{self, BLOCK_BYTES, GROUP_DESCRIPTOR_BYTES, SUPERBLOCK_BYTES},
};

const SECTOR_BYTES: u32 = 512;
const SECTORS_PER_BLOCK: u64 = 8;
const EXT2_SUPER_MAGIC: u16 = 0xef53;
const EXT2_DYNAMIC_REV: u32 = 1;
const EXT2_VALID_FS: u16 = 1;
const EXT2_FEATURE_INCOMPAT_FILETYPE: u32 = 2;
const PROFILE_BLOCKS: u32 = 32_768;
const PROFILE_INODES: u32 = 4_096;
const PROFILE_INODE_BYTES: u16 = 256;
const PROFILE_SECTORS: u64 = 262_144;

#[allow(dead_code)] // Tasks 2 and 3 consume these validated locations.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub(crate) block_bitmap: u32,
    pub(crate) inode_bitmap: u32,
    pub(crate) inode_table: u32,
    pub(crate) inode_table_blocks: u32,
}

pub fn mount<D: BlockDevice>(device: &mut D, mode: MountMode) -> Result<Geometry, Ext2Error> {
    let device_geometry = device.geometry();
    if device_geometry.logical_sector_size != SECTOR_BYTES {
        return Err(Ext2Error::UnsupportedSector {
            sector_size: device_geometry.logical_sector_size,
        });
    }
    if device_geometry.sector_count < PROFILE_SECTORS {
        return Err(Ext2Error::UnsupportedProfile {
            field: "device_length",
        });
    }

    let mut superblock = [0; SUPERBLOCK_BYTES];
    read_sectors(device, 2, &mut superblock)?;
    validate_superblock(&superblock, mode)?;

    let mut descriptor_block = [0; BLOCK_BYTES];
    read_sectors(device, SECTORS_PER_BLOCK, &mut descriptor_block)?;
    validate_group_descriptor(&descriptor_block)
}

fn read_sectors<D: BlockDevice>(
    device: &mut D,
    first_lba: u64,
    bytes: &mut [u8],
) -> Result<(), Ext2Error> {
    device
        .read_sectors(first_lba, bytes)
        .map_err(Ext2Error::Block)
}

fn validate_superblock(bytes: &[u8], mode: MountMode) -> Result<(), Ext2Error> {
    require_u16(bytes, on_disk::SUPERBLOCK_MAGIC, EXT2_SUPER_MAGIC, "magic")?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_REVISION_LEVEL,
        EXT2_DYNAMIC_REV,
        "revision",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_LOG_BLOCK_SIZE,
        2,
        "log_block_size",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_LOG_FRAGMENT_SIZE,
        2,
        "log_fragment_size",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_BLOCKS_COUNT,
        PROFILE_BLOCKS,
        "blocks_count",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_INODES_COUNT,
        PROFILE_INODES,
        "inodes_count",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_BLOCKS_PER_GROUP,
        PROFILE_BLOCKS,
        "blocks_per_group",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_FRAGMENTS_PER_GROUP,
        PROFILE_BLOCKS,
        "fragments_per_group",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_INODES_PER_GROUP,
        PROFILE_INODES,
        "inodes_per_group",
    )?;
    require_u16(
        bytes,
        on_disk::SUPERBLOCK_INODE_SIZE,
        PROFILE_INODE_BYTES,
        "inode_size",
    )?;
    require_u32(
        bytes,
        on_disk::SUPERBLOCK_FIRST_DATA_BLOCK,
        0,
        "first_data_block",
    )?;
    require_feature(bytes, on_disk::SUPERBLOCK_FEATURE_COMPAT, 0, "compatible")?;
    require_feature(
        bytes,
        on_disk::SUPERBLOCK_FEATURE_INCOMPAT,
        EXT2_FEATURE_INCOMPAT_FILETYPE,
        "incompatible",
    )?;
    require_feature(
        bytes,
        on_disk::SUPERBLOCK_FEATURE_RO_COMPAT,
        0,
        "read_only_compatible",
    )?;

    if mode == MountMode::ReadWrite {
        let state = on_disk::u16(bytes, on_disk::SUPERBLOCK_STATE, "state")?;
        if state != EXT2_VALID_FS {
            return Err(Ext2Error::MountRequiresCleanFilesystem);
        }
    }
    Ok(())
}

fn require_u16(
    bytes: &[u8],
    offset: usize,
    expected: u16,
    field: &'static str,
) -> Result<(), Ext2Error> {
    if on_disk::u16(bytes, offset, field)? != expected {
        return Err(Ext2Error::UnsupportedProfile { field });
    }
    Ok(())
}

fn require_u32(
    bytes: &[u8],
    offset: usize,
    expected: u32,
    field: &'static str,
) -> Result<(), Ext2Error> {
    if on_disk::u32(bytes, offset, field)? != expected {
        return Err(Ext2Error::UnsupportedProfile { field });
    }
    Ok(())
}

fn require_feature(
    bytes: &[u8],
    offset: usize,
    expected: u32,
    field: &'static str,
) -> Result<(), Ext2Error> {
    let actual = on_disk::u32(bytes, offset, field)?;
    if actual != expected {
        return Err(Ext2Error::UnsupportedFeature {
            field,
            bits: actual ^ expected,
        });
    }
    Ok(())
}

fn validate_group_descriptor(bytes: &[u8]) -> Result<Geometry, Ext2Error> {
    let block_bitmap = on_disk::u32(
        bytes,
        on_disk::GROUP_DESCRIPTOR_BLOCK_BITMAP,
        "block_bitmap",
    )?;
    let inode_bitmap = on_disk::u32(
        bytes,
        on_disk::GROUP_DESCRIPTOR_INODE_BITMAP,
        "inode_bitmap",
    )?;
    let inode_table = on_disk::u32(bytes, on_disk::GROUP_DESCRIPTOR_INODE_TABLE, "inode_table")?;
    let inode_table_blocks = PROFILE_INODES
        .checked_mul(u32::from(PROFILE_INODE_BYTES))
        .and_then(|bytes| bytes.checked_div(BLOCK_BYTES as u32))
        .ok_or(Ext2Error::CorruptMetadata {
            field: "inode_table",
        })?;

    let inode_table_end =
        inode_table
            .checked_add(inode_table_blocks)
            .ok_or(Ext2Error::CorruptMetadata {
                field: "inode_table",
            })?;
    if block_bitmap >= PROFILE_BLOCKS
        || inode_bitmap >= PROFILE_BLOCKS
        || inode_table >= PROFILE_BLOCKS
        || inode_table_end > PROFILE_BLOCKS
    {
        return Err(Ext2Error::CorruptMetadata {
            field: "group_metadata",
        });
    }

    let metadata_end = 2;
    if block_bitmap < metadata_end
        || inode_bitmap < metadata_end
        || inode_table < metadata_end
        || block_bitmap == inode_bitmap
        || range_contains(inode_table, inode_table_end, block_bitmap)
        || range_contains(inode_table, inode_table_end, inode_bitmap)
    {
        return Err(Ext2Error::CorruptMetadata {
            field: "group_metadata",
        });
    }

    Ok(Geometry {
        block_bitmap,
        inode_bitmap,
        inode_table,
        inode_table_blocks,
    })
}

fn range_contains(start: u32, end: u32, value: u32) -> bool {
    start <= value && value < end
}

const _: usize = GROUP_DESCRIPTOR_BYTES;
