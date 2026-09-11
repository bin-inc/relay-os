use alloc::vec::Vec;

use crc::{CRC_32_ISO_HDLC, Crc};
use relay_abi::GptGuid;

use crate::block::{BlockDevice, BlockError, BlockGeometry, PartitionDevice, PartitionRange};

const SECTOR_SIZE: u32 = 512;
const HEADER_SIZE_MIN: u32 = 92;
const REVISION_1_0: u32 = 0x0001_0000;
const MAX_STANDARD_ENTRY_ARRAY_BYTES: u64 = 128 * 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GptCopy {
    Primary,
    Backup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GptHealth {
    Redundant,
    Degraded { invalid_copy: GptCopy },
}

pub struct SelectedPartition<D> {
    pub device: PartitionDevice<D>,
    pub health: GptHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GptCopyError {
    Device(BlockError),
    Signature,
    Revision(u32),
    HeaderSize(u32),
    HeaderCrc,
    CurrentLba {
        expected: u64,
        actual: u64,
    },
    AlternateLba {
        expected: u64,
        actual: u64,
    },
    UsableRange {
        first_lba: u64,
        last_lba: u64,
    },
    EntryCount,
    EntrySize(u32),
    EntryByteLength {
        count: u32,
        size: u32,
    },
    EntryArrayTooLarge {
        byte_len: u64,
    },
    EntryArrayRange {
        first_lba: u64,
        sector_count: u64,
    },
    EntryAllocation {
        byte_len: u64,
    },
    EntryCrc,
    EntryRange {
        index: u32,
        first_lba: u64,
        last_lba: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GptError {
    UnsupportedSectorSize(u32),
    Device(BlockError),
    BothCopiesInvalid {
        primary: GptCopyError,
        backup: GptCopyError,
    },
    ConflictingCopies,
    GuidNotFound,
    DuplicateGuid,
}

struct GptTable {
    disk_guid: GptGuid,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries: Vec<GptEntry>,
}

#[derive(Eq, PartialEq)]
struct GptEntry {
    type_guid: GptGuid,
    unique_guid: GptGuid,
    range: PartitionRange,
}

impl GptTable {
    fn matches(&self, other: &Self) -> bool {
        self.disk_guid == other.disk_guid
            && self.first_usable_lba == other.first_usable_lba
            && self.last_usable_lba == other.last_usable_lba
            && self.entry_count == other.entry_count
            && self.entry_size == other.entry_size
            && self.entries == other.entries
    }
}

pub fn find_partition_by_unique_guid<D: BlockDevice>(
    mut device: D,
    target: GptGuid,
) -> Result<SelectedPartition<D>, GptError> {
    let geometry = device.geometry();
    if geometry.logical_sector_size != SECTOR_SIZE {
        return Err(GptError::UnsupportedSectorSize(
            geometry.logical_sector_size,
        ));
    }
    let backup_lba = geometry
        .sector_count
        .checked_sub(1)
        .ok_or(GptError::BothCopiesInvalid {
            primary: GptCopyError::CurrentLba {
                expected: 1,
                actual: 0,
            },
            backup: GptCopyError::CurrentLba {
                expected: 0,
                actual: 0,
            },
        })?;

    let primary = parse_copy(&mut device, geometry, 1, backup_lba);
    let backup = parse_copy(&mut device, geometry, backup_lba, 1);
    let (table, health) = match (primary, backup) {
        (Ok(primary), Ok(backup)) if primary.matches(&backup) => (primary, GptHealth::Redundant),
        (Ok(_), Ok(_)) => return Err(GptError::ConflictingCopies),
        (Ok(primary), Err(_)) => (
            primary,
            GptHealth::Degraded {
                invalid_copy: GptCopy::Backup,
            },
        ),
        (Err(_), Ok(backup)) => (
            backup,
            GptHealth::Degraded {
                invalid_copy: GptCopy::Primary,
            },
        ),
        (Err(primary), Err(backup)) => {
            return Err(GptError::BothCopiesInvalid { primary, backup });
        }
    };

    let mut selected = None;
    for entry in table.entries {
        if entry.unique_guid == target && selected.replace(entry.range).is_some() {
            return Err(GptError::DuplicateGuid);
        }
    }
    let range = selected.ok_or(GptError::GuidNotFound)?;
    let device = PartitionDevice::new(device, range).map_err(GptError::Device)?;
    Ok(SelectedPartition { device, health })
}

fn parse_copy<D: BlockDevice>(
    device: &mut D,
    geometry: BlockGeometry,
    header_lba: u64,
    expected_alternate_lba: u64,
) -> Result<GptTable, GptCopyError> {
    let mut header = [0; SECTOR_SIZE as usize];
    device
        .read_sectors(header_lba, &mut header)
        .map_err(GptCopyError::Device)?;
    if &header[..8] != b"EFI PART" {
        return Err(GptCopyError::Signature);
    }
    if read_u32(&header, 8)? != REVISION_1_0 {
        return Err(GptCopyError::Revision(read_u32(&header, 8)?));
    }
    let header_size = read_u32(&header, 12)?;
    if !(HEADER_SIZE_MIN..=SECTOR_SIZE).contains(&header_size) {
        return Err(GptCopyError::HeaderSize(header_size));
    }
    let header_size =
        usize::try_from(header_size).map_err(|_| GptCopyError::HeaderSize(u32::MAX))?;
    let stored_header_crc = read_u32(&header, 16)?;
    let mut crc_header = [0; SECTOR_SIZE as usize];
    crc_header[..header_size].copy_from_slice(&header[..header_size]);
    crc_header[16..20].fill(0);
    if crc().checksum(&crc_header[..header_size]) != stored_header_crc {
        return Err(GptCopyError::HeaderCrc);
    }
    let current_lba = read_u64(&header, 24)?;
    if current_lba != header_lba {
        return Err(GptCopyError::CurrentLba {
            expected: header_lba,
            actual: current_lba,
        });
    }
    let alternate_lba = read_u64(&header, 32)?;
    if alternate_lba != expected_alternate_lba {
        return Err(GptCopyError::AlternateLba {
            expected: expected_alternate_lba,
            actual: alternate_lba,
        });
    }
    let first_usable_lba = read_u64(&header, 40)?;
    let last_usable_lba = read_u64(&header, 48)?;
    let usable_end = last_usable_lba
        .checked_add(1)
        .ok_or(GptCopyError::UsableRange {
            first_lba: first_usable_lba,
            last_lba: last_usable_lba,
        })?;
    if first_usable_lba > last_usable_lba
        || last_usable_lba >= geometry.sector_count
        || ranges_overlap(first_usable_lba, usable_end, 1, 2)
        || ranges_overlap(
            first_usable_lba,
            usable_end,
            geometry.sector_count - 1,
            geometry.sector_count,
        )
    {
        return Err(GptCopyError::UsableRange {
            first_lba: first_usable_lba,
            last_lba: last_usable_lba,
        });
    }
    let disk_guid = read_guid(&header, 56)?;
    let entries_lba = read_u64(&header, 72)?;
    let entry_count = read_u32(&header, 80)?;
    if entry_count == 0 {
        return Err(GptCopyError::EntryCount);
    }
    let entry_size = read_u32(&header, 84)?;
    if entry_size < 128 {
        return Err(GptCopyError::EntrySize(entry_size));
    }
    let entry_bytes = u64::from(entry_count)
        .checked_mul(u64::from(entry_size))
        .ok_or(GptCopyError::EntryByteLength {
            count: entry_count,
            size: entry_size,
        })?;
    if entry_bytes > MAX_STANDARD_ENTRY_ARRAY_BYTES {
        return Err(GptCopyError::EntryArrayTooLarge {
            byte_len: entry_bytes,
        });
    }
    let entry_sectors = entry_bytes
        .checked_add(u64::from(SECTOR_SIZE - 1))
        .and_then(|value| value.checked_div(u64::from(SECTOR_SIZE)))
        .ok_or(GptCopyError::EntryByteLength {
            count: entry_count,
            size: entry_size,
        })?;
    let entries_end =
        entries_lba
            .checked_add(entry_sectors)
            .ok_or(GptCopyError::EntryArrayRange {
                first_lba: entries_lba,
                sector_count: entry_sectors,
            })?;
    if entries_lba >= geometry.sector_count
        || entries_end > geometry.sector_count
        || ranges_overlap(entries_lba, entries_end, 1, 2)
        || ranges_overlap(
            entries_lba,
            entries_end,
            geometry.sector_count - 1,
            geometry.sector_count,
        )
        || ranges_overlap(entries_lba, entries_end, first_usable_lba, usable_end)
    {
        return Err(GptCopyError::EntryArrayRange {
            first_lba: entries_lba,
            sector_count: entry_sectors,
        });
    }
    let rounded_bytes =
        entry_sectors
            .checked_mul(u64::from(SECTOR_SIZE))
            .ok_or(GptCopyError::EntryByteLength {
                count: entry_count,
                size: entry_size,
            })?;
    let rounded_bytes =
        usize::try_from(rounded_bytes).map_err(|_| GptCopyError::EntryByteLength {
            count: entry_count,
            size: entry_size,
        })?;
    let entry_byte_len =
        usize::try_from(entry_bytes).map_err(|_| GptCopyError::EntryByteLength {
            count: entry_count,
            size: entry_size,
        })?;
    let mut entry_bytes_buffer = Vec::new();
    entry_bytes_buffer
        .try_reserve_exact(rounded_bytes)
        .map_err(|_| GptCopyError::EntryAllocation {
            byte_len: entry_bytes,
        })?;
    entry_bytes_buffer.resize(rounded_bytes, 0);
    device
        .read_sectors(entries_lba, &mut entry_bytes_buffer)
        .map_err(GptCopyError::Device)?;
    if crc().checksum(&entry_bytes_buffer[..entry_byte_len]) != read_u32(&header, 88)? {
        return Err(GptCopyError::EntryCrc);
    }

    let mut entries = Vec::new();
    entries
        .try_reserve_exact(usize::try_from(entry_count).unwrap_or(usize::MAX))
        .map_err(|_| GptCopyError::EntryAllocation {
            byte_len: entry_bytes,
        })?;
    for index in 0..entry_count {
        let offset = u64::from(index)
            .checked_mul(u64::from(entry_size))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(GptCopyError::EntryByteLength {
                count: entry_count,
                size: entry_size,
            })?;
        let entry =
            entry_bytes_buffer
                .get(offset..offset + 128)
                .ok_or(GptCopyError::EntryByteLength {
                    count: entry_count,
                    size: entry_size,
                })?;
        let type_guid = read_guid(entry, 0)?;
        if type_guid == GptGuid([0; 16]) {
            continue;
        }
        let first_lba = read_u64(entry, 32)?;
        let last_lba = read_u64(entry, 40)?;
        if first_lba > last_lba || first_lba < first_usable_lba || last_lba > last_usable_lba {
            return Err(GptCopyError::EntryRange {
                index,
                first_lba,
                last_lba,
            });
        }
        let sector_count = last_lba
            .checked_sub(first_lba)
            .and_then(|value| value.checked_add(1))
            .ok_or(GptCopyError::EntryRange {
                index,
                first_lba,
                last_lba,
            })?;
        entries.push(GptEntry {
            type_guid,
            unique_guid: read_guid(entry, 16)?,
            range: PartitionRange {
                first_lba,
                sector_count,
            },
        });
    }
    Ok(GptTable {
        disk_guid,
        first_usable_lba,
        last_usable_lba,
        entry_count,
        entry_size,
        entries,
    })
}

fn crc() -> Crc<u32> {
    Crc::<u32>::new(&CRC_32_ISO_HDLC)
}

fn ranges_overlap(first_start: u64, first_end: u64, second_start: u64, second_end: u64) -> bool {
    first_start < second_end && second_start < first_end
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, GptCopyError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(GptCopyError::HeaderSize(0))?;
    Ok(u32::from_le_bytes(
        value.try_into().map_err(|_| GptCopyError::HeaderSize(0))?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, GptCopyError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(GptCopyError::HeaderSize(0))?;
    Ok(u64::from_le_bytes(
        value.try_into().map_err(|_| GptCopyError::HeaderSize(0))?,
    ))
}

fn read_guid(bytes: &[u8], offset: usize) -> Result<GptGuid, GptCopyError> {
    let value: [u8; 16] = bytes
        .get(offset..offset + 16)
        .ok_or(GptCopyError::HeaderSize(0))?
        .try_into()
        .map_err(|_| GptCopyError::HeaderSize(0))?;
    Ok(GptGuid::from_gpt_bytes(value))
}
