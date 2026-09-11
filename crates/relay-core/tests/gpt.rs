pub mod support;

use core::str::FromStr;
use std::path::Path;

use crc::{CRC_32_ISO_HDLC, Crc};
use relay_abi::GptGuid;
use relay_core::block::BlockDevice;
use relay_core::gpt::{GptCopy, GptCopyError, GptError, GptHealth, find_partition_by_unique_guid};
use support::memory_device::MemoryDevice;

const SECTOR_SIZE: usize = 512;
const DISK_SECTORS: usize = 397_312;
const ENTRY_COUNT: usize = 128;
const ENTRY_SIZE: usize = 128;
const MAX_ENTRY_ARRAY_BYTES: usize = ENTRY_COUNT * ENTRY_SIZE;
const OVERSIZED_ENTRY_COUNT: usize = ENTRY_COUNT + 1;
const OVERSIZED_ENTRY_BYTES: usize = OVERSIZED_ENTRY_COUNT * ENTRY_SIZE;
const HEADER_SIZE: usize = 92;
const PRIMARY_HEADER: usize = 1;
const PRIMARY_ENTRIES: usize = 2;
const BACKUP_HEADER: usize = DISK_SECTORS - 1;
const BACKUP_ENTRIES: usize = DISK_SECTORS - 33;
const OVERSIZED_BACKUP_ENTRIES: usize = BACKUP_HEADER - 34;
const FIRST_USABLE: u64 = 34;
const LAST_USABLE: u64 = DISK_SECTORS as u64 - 34;

#[test]
fn guid_decodes_gpt_mixed_endian_fields() {
    let guid = GptGuid::from_gpt_bytes([
        0x41, 0x4c, 0x45, 0x52, 0x00, 0x59, 0x00, 0x40, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03,
    ]);
    assert_eq!(guid.to_string(), "52454c41-5900-4000-8000-000000000003");
}

#[test]
fn matching_copies_select_the_unique_guid_with_redundant_health() {
    let target = test_guid(3);
    let disk = mirrored_disk_with_entry(target, 133_120, 395_263);

    let selected = find_partition_by_unique_guid(disk, target).unwrap();

    assert_eq!(selected.health, GptHealth::Redundant);
    assert_eq!(selected.device.geometry().sector_count, 262_144);
}

#[test]
fn one_valid_copy_returns_typed_degradation() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    corrupt_backup_header_crc(&mut disk);

    let selected = find_partition_by_unique_guid(disk, target).unwrap();

    assert_eq!(
        selected.health,
        GptHealth::Degraded {
            invalid_copy: GptCopy::Backup
        }
    );
}

#[test]
fn valid_backup_with_invalid_primary_is_degraded() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    corrupt_primary_header_crc(&mut disk);

    let selected = find_partition_by_unique_guid(disk, target).unwrap();

    assert_eq!(
        selected.health,
        GptHealth::Degraded {
            invalid_copy: GptCopy::Primary
        }
    );
}

#[test]
fn conflicting_valid_copy_is_rejected() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    rewrite_backup_entry_range_and_crcs(&mut disk, 133_121, 395_263);

    assert!(matches!(
        find_partition_by_unique_guid(disk, target),
        Err(GptError::ConflictingCopies),
    ));
}

#[test]
fn duplicate_unique_guid_is_rejected() {
    let target = test_guid(3);
    let disk = mirrored_disk_with_duplicate_unique_guid(target);

    assert!(matches!(
        find_partition_by_unique_guid(disk, target),
        Err(GptError::DuplicateGuid),
    ));
}

#[test]
fn non_512_byte_devices_are_rejected_without_reads() {
    let disk = MemoryDevice::new(4_096, 64).unwrap();
    let operations = disk.operations();

    assert!(matches!(
        find_partition_by_unique_guid(disk, test_guid(3)),
        Err(GptError::UnsupportedSectorSize(4_096)),
    ));
    assert_eq!(operations.read_count(), 0);
}

#[test]
fn memory_device_operations_remain_observable_after_selection_moves_it() {
    let disk = mirrored_disk_with_entry(test_guid(3), 133_120, 395_263);
    let operations = disk.operations();

    let mut selected = find_partition_by_unique_guid(disk, test_guid(3)).unwrap();
    selected
        .device
        .read_sectors(0, &mut [0; SECTOR_SIZE])
        .unwrap();
    selected.device.write_sectors(0, &[0; SECTOR_SIZE]).unwrap();
    selected.device.flush().unwrap();

    assert!(operations.read_count() > 4);
    assert_eq!(operations.write_count(), 1);
    assert_eq!(operations.flush_count(), 1);
}

#[test]
fn missing_unique_guid_is_rejected_even_when_type_guid_matches() {
    let disk = mirrored_disk_with_entry(test_guid(4), 133_120, 395_263);

    assert!(matches!(
        find_partition_by_unique_guid(disk, test_guid(1)),
        Err(GptError::GuidNotFound),
    ));
}

#[test]
fn both_invalid_copies_retain_their_validation_errors() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    corrupt_primary_header_crc(&mut disk);
    corrupt_backup_header_crc(&mut disk);

    let Err(GptError::BothCopiesInvalid { primary, backup }) =
        find_partition_by_unique_guid(disk, target)
    else {
        panic!("expected both GPT copies to be invalid");
    };
    assert_eq!(primary, GptCopyError::HeaderCrc,);
    assert_eq!(backup, GptCopyError::HeaderCrc,);
}

macro_rules! malformed_header_test {
    ($name:ident, $mutation:ident) => {
        #[test]
        fn $name() {
            let target = test_guid(3);
            let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
            $mutation(&mut disk, PRIMARY_HEADER);

            assert_degraded_primary(disk, target);
        }
    };
}

malformed_header_test!(invalid_header_signature_is_rejected, corrupt_signature);
malformed_header_test!(invalid_header_revision_is_rejected, corrupt_revision);
malformed_header_test!(invalid_header_size_is_rejected, corrupt_size);
malformed_header_test!(invalid_current_lba_is_rejected, corrupt_current_lba);
malformed_header_test!(invalid_alternate_lba_is_rejected, corrupt_alternate_lba);
malformed_header_test!(invalid_usable_range_is_rejected, corrupt_usable_range);
malformed_header_test!(zero_entry_count_is_rejected, corrupt_entry_count);
malformed_header_test!(small_entry_size_is_rejected, corrupt_entry_size);
malformed_header_test!(
    overflowing_entry_byte_length_is_rejected,
    overflow_entry_byte_length
);
malformed_header_test!(
    out_of_device_entry_array_is_rejected,
    corrupt_entry_array_lba
);

#[test]
fn entry_array_overlapping_a_gpt_header_is_rejected_before_crc_validation() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    set_entry_array_lba(&mut disk, PRIMARY_HEADER, PRIMARY_HEADER, PRIMARY_HEADER);
    set_entry_array_lba(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES, BACKUP_HEADER);

    assert_entry_array_range_for_both_copies(disk, target);
}

#[test]
fn entry_array_overlapping_usable_lbas_is_rejected_before_crc_validation() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    copy_entry_array(disk.bytes_mut(), PRIMARY_ENTRIES, FIRST_USABLE as usize);
    set_entry_array_lba(
        &mut disk,
        PRIMARY_HEADER,
        PRIMARY_ENTRIES,
        FIRST_USABLE as usize,
    );
    set_entry_array_lba(
        &mut disk,
        BACKUP_HEADER,
        BACKUP_ENTRIES,
        FIRST_USABLE as usize,
    );

    assert_entry_array_range_for_both_copies(disk, target);
}

#[test]
fn usable_range_overlapping_primary_header_is_rejected_by_both_copies() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    set_usable_range(&mut disk, PRIMARY_HEADER, 1, LAST_USABLE);
    set_usable_range(&mut disk, BACKUP_HEADER, 1, LAST_USABLE);

    assert_usable_range_for_both_copies(disk, target);
}

#[test]
fn usable_range_overlapping_backup_header_is_rejected_by_both_copies() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    set_usable_range(
        &mut disk,
        PRIMARY_HEADER,
        FIRST_USABLE,
        BACKUP_HEADER as u64,
    );
    set_usable_range(&mut disk, BACKUP_HEADER, FIRST_USABLE, BACKUP_HEADER as u64);

    assert_usable_range_for_both_copies(disk, target);
}

#[test]
fn entry_array_larger_than_the_standard_table_is_rejected_before_allocation() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    configure_oversized_entry_arrays(&mut disk);

    let Err(GptError::BothCopiesInvalid { primary, backup }) =
        find_partition_by_unique_guid(disk, target)
    else {
        panic!("expected both GPT copies to reject the oversized entry array");
    };
    assert_eq!(
        primary,
        GptCopyError::EntryArrayTooLarge {
            byte_len: OVERSIZED_ENTRY_BYTES as u64,
        }
    );
    assert_eq!(
        backup,
        GptCopyError::EntryArrayTooLarge {
            byte_len: OVERSIZED_ENTRY_BYTES as u64,
        }
    );
}

#[test]
fn entry_array_crc_failure_is_rejected() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    byte_mut(disk.bytes_mut(), PRIMARY_ENTRIES, 0)[0] ^= 1;

    assert_degraded_primary(disk, target);
}

#[test]
fn nonempty_entry_range_outside_usable_lbas_is_rejected() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    write_u64(
        byte_mut(disk.bytes_mut(), PRIMARY_ENTRIES, 0),
        32,
        FIRST_USABLE - 1,
    );
    update_copy_crcs(disk.bytes_mut(), PRIMARY_HEADER, PRIMARY_ENTRIES);

    assert_degraded_primary(disk, target);
}

#[test]
fn truncated_entry_array_is_rejected() {
    let target = test_guid(3);
    let mut disk = mirrored_disk_with_entry(target, 133_120, 395_263);
    write_u32(header_mut(disk.bytes_mut(), PRIMARY_HEADER), 80, u32::MAX);
    update_header_crc(disk.bytes_mut(), PRIMARY_HEADER);

    assert_degraded_primary(disk, target);
}

#[test]
fn generated_image_selects_the_root_partition_when_present() {
    let image = Path::new("target/relay-os.img");
    if !image.exists() {
        return;
    }
    let bytes = std::fs::read(image).unwrap();
    let disk = MemoryDevice::from_bytes(512, bytes).unwrap();
    let selected = find_partition_by_unique_guid(disk, test_guid(3)).unwrap();

    assert_eq!(selected.health, GptHealth::Redundant);
    assert_eq!(selected.device.geometry().sector_count, 262_144);
}

fn assert_degraded_primary(disk: MemoryDevice, target: GptGuid) {
    assert!(matches!(
        find_partition_by_unique_guid(disk, target),
        Ok(selected) if selected.health == GptHealth::Degraded { invalid_copy: GptCopy::Primary }
    ));
}

fn assert_entry_array_range_for_both_copies(disk: MemoryDevice, target: GptGuid) {
    let Err(GptError::BothCopiesInvalid { primary, backup }) =
        find_partition_by_unique_guid(disk, target)
    else {
        panic!("expected both GPT copies to reject the entry array placement");
    };
    assert!(matches!(primary, GptCopyError::EntryArrayRange { .. }));
    assert!(matches!(backup, GptCopyError::EntryArrayRange { .. }));
}

fn assert_usable_range_for_both_copies(disk: MemoryDevice, target: GptGuid) {
    let Err(GptError::BothCopiesInvalid { primary, backup }) =
        find_partition_by_unique_guid(disk, target)
    else {
        panic!("expected both GPT copies to reject the usable range");
    };
    assert!(matches!(primary, GptCopyError::UsableRange { .. }));
    assert!(matches!(backup, GptCopyError::UsableRange { .. }));
}

fn test_guid(last_byte: u8) -> GptGuid {
    GptGuid::from_str(&format!(
        "52454c41-5900-4000-8000-0000000000{last_byte:02x}"
    ))
    .unwrap()
}

fn mirrored_disk_with_entry(target: GptGuid, first_lba: u64, last_lba: u64) -> MemoryDevice {
    let mut disk = vec![0; DISK_SECTORS * SECTOR_SIZE];
    write_entry(&mut disk, PRIMARY_ENTRIES, 0, target, first_lba, last_lba);
    copy_entries(&mut disk);
    write_header(
        &mut disk,
        PRIMARY_HEADER,
        PRIMARY_ENTRIES,
        1,
        BACKUP_HEADER as u64,
    );
    write_header(
        &mut disk,
        BACKUP_HEADER,
        BACKUP_ENTRIES,
        BACKUP_HEADER as u64,
        1,
    );
    MemoryDevice::from_bytes(512, disk).unwrap()
}

fn mirrored_disk_with_duplicate_unique_guid(target: GptGuid) -> MemoryDevice {
    let mut disk = mirrored_disk_with_entry(target, 133_120, 200_000).into_bytes();
    write_entry(&mut disk, PRIMARY_ENTRIES, 1, target, 200_001, 395_263);
    copy_entries(&mut disk);
    update_copy_crcs(&mut disk, PRIMARY_HEADER, PRIMARY_ENTRIES);
    update_copy_crcs(&mut disk, BACKUP_HEADER, BACKUP_ENTRIES);
    MemoryDevice::from_bytes(512, disk).unwrap()
}

fn write_entry(
    bytes: &mut [u8],
    entries_lba: usize,
    index: usize,
    guid: GptGuid,
    first: u64,
    last: u64,
) {
    let entry = byte_mut(bytes, entries_lba, index * ENTRY_SIZE);
    entry[..16].copy_from_slice(&test_guid(1).to_gpt_bytes());
    entry[16..32].copy_from_slice(&guid.to_gpt_bytes());
    write_u64(entry, 32, first);
    write_u64(entry, 40, last);
}

fn copy_entries(bytes: &mut [u8]) {
    copy_entry_array(bytes, PRIMARY_ENTRIES, BACKUP_ENTRIES);
}

fn copy_entry_array(bytes: &mut [u8], source_lba: usize, destination_lba: usize) {
    copy_entry_array_with_len(bytes, source_lba, destination_lba, MAX_ENTRY_ARRAY_BYTES);
}

fn copy_entry_array_with_len(
    bytes: &mut [u8],
    source_lba: usize,
    destination_lba: usize,
    byte_len: usize,
) {
    let primary = sector_range(source_lba, byte_len);
    let backup = sector_range(destination_lba, byte_len);
    let entries = bytes[primary].to_vec();
    bytes[backup].copy_from_slice(&entries);
}

fn write_header(bytes: &mut [u8], lba: usize, entries_lba: usize, current: u64, alternate: u64) {
    let header = header_mut(bytes, lba);
    header[..8].copy_from_slice(b"EFI PART");
    write_u32(header, 8, 0x0001_0000);
    write_u32(header, 12, HEADER_SIZE as u32);
    write_u64(header, 24, current);
    write_u64(header, 32, alternate);
    write_u64(header, 40, FIRST_USABLE);
    write_u64(header, 48, LAST_USABLE);
    header[56..72].copy_from_slice(&test_guid(2).to_gpt_bytes());
    write_u64(header, 72, entries_lba as u64);
    write_u32(header, 80, ENTRY_COUNT as u32);
    write_u32(header, 84, ENTRY_SIZE as u32);
    update_copy_crcs(bytes, lba, entries_lba);
}

fn corrupt_backup_header_crc(disk: &mut MemoryDevice) {
    corrupt_header_crc(disk.bytes_mut(), BACKUP_HEADER);
}
fn corrupt_primary_header_crc(disk: &mut MemoryDevice) {
    corrupt_header_crc(disk.bytes_mut(), PRIMARY_HEADER);
}
fn corrupt_header_crc(bytes: &mut [u8], lba: usize) {
    header_mut(bytes, lba)[16] ^= 1;
}
fn corrupt_signature(bytes: &mut MemoryDevice, lba: usize) {
    header_mut(bytes.bytes_mut(), lba)[0] ^= 1;
}
fn corrupt_revision(bytes: &mut MemoryDevice, lba: usize) {
    write_u32(header_mut(bytes.bytes_mut(), lba), 8, 0);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_size(bytes: &mut MemoryDevice, lba: usize) {
    write_u32(header_mut(bytes.bytes_mut(), lba), 12, 91);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_current_lba(bytes: &mut MemoryDevice, lba: usize) {
    write_u64(header_mut(bytes.bytes_mut(), lba), 24, 2);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_alternate_lba(bytes: &mut MemoryDevice, lba: usize) {
    write_u64(header_mut(bytes.bytes_mut(), lba), 32, 2);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_usable_range(bytes: &mut MemoryDevice, lba: usize) {
    write_u64(header_mut(bytes.bytes_mut(), lba), 40, LAST_USABLE);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_entry_count(bytes: &mut MemoryDevice, lba: usize) {
    write_u32(header_mut(bytes.bytes_mut(), lba), 80, 0);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_entry_size(bytes: &mut MemoryDevice, lba: usize) {
    write_u32(header_mut(bytes.bytes_mut(), lba), 84, 127);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn overflow_entry_byte_length(bytes: &mut MemoryDevice, lba: usize) {
    write_u32(header_mut(bytes.bytes_mut(), lba), 80, u32::MAX);
    write_u32(header_mut(bytes.bytes_mut(), lba), 84, u32::MAX);
    update_header_crc(bytes.bytes_mut(), lba);
}
fn corrupt_entry_array_lba(bytes: &mut MemoryDevice, lba: usize) {
    write_u64(header_mut(bytes.bytes_mut(), lba), 72, BACKUP_HEADER as u64);
    update_header_crc(bytes.bytes_mut(), lba);
}

fn set_entry_array_lba(
    disk: &mut MemoryDevice,
    header_lba: usize,
    previous_entries_lba: usize,
    entries_lba: usize,
) {
    write_u64(
        header_mut(disk.bytes_mut(), header_lba),
        72,
        entries_lba as u64,
    );
    update_copy_crcs(disk.bytes_mut(), header_lba, previous_entries_lba);
}

fn set_usable_range(disk: &mut MemoryDevice, header_lba: usize, first_lba: u64, last_lba: u64) {
    let header = header_mut(disk.bytes_mut(), header_lba);
    write_u64(header, 40, first_lba);
    write_u64(header, 48, last_lba);
    update_header_crc(disk.bytes_mut(), header_lba);
}

fn configure_oversized_entry_arrays(disk: &mut MemoryDevice) {
    copy_entry_array_with_len(
        disk.bytes_mut(),
        PRIMARY_ENTRIES,
        OVERSIZED_BACKUP_ENTRIES,
        OVERSIZED_ENTRY_BYTES,
    );
    for (header_lba, entries_lba) in [
        (PRIMARY_HEADER, PRIMARY_ENTRIES),
        (BACKUP_HEADER, OVERSIZED_BACKUP_ENTRIES),
    ] {
        let header = header_mut(disk.bytes_mut(), header_lba);
        write_u64(header, 40, FIRST_USABLE + 1);
        write_u64(header, 48, (OVERSIZED_BACKUP_ENTRIES - 1) as u64);
        write_u64(header, 72, entries_lba as u64);
        write_u32(header, 80, OVERSIZED_ENTRY_COUNT as u32);
        update_copy_crcs_with_entry_len(
            disk.bytes_mut(),
            header_lba,
            entries_lba,
            OVERSIZED_ENTRY_BYTES,
        );
    }
}

fn rewrite_backup_entry_range_and_crcs(disk: &mut MemoryDevice, first: u64, last: u64) {
    write_u64(byte_mut(disk.bytes_mut(), BACKUP_ENTRIES, 0), 32, first);
    write_u64(byte_mut(disk.bytes_mut(), BACKUP_ENTRIES, 0), 40, last);
    update_copy_crcs(disk.bytes_mut(), BACKUP_HEADER, BACKUP_ENTRIES);
}

fn update_copy_crcs(bytes: &mut [u8], header_lba: usize, entries_lba: usize) {
    update_copy_crcs_with_entry_len(bytes, header_lba, entries_lba, MAX_ENTRY_ARRAY_BYTES);
}

fn update_copy_crcs_with_entry_len(
    bytes: &mut [u8],
    header_lba: usize,
    entries_lba: usize,
    entry_byte_len: usize,
) {
    let entries = sector_range(entries_lba, entry_byte_len);
    let entries_crc = crc().checksum(&bytes[entries]);
    write_u32(header_mut(bytes, header_lba), 88, entries_crc);
    update_header_crc(bytes, header_lba);
}

fn update_header_crc(bytes: &mut [u8], lba: usize) {
    let range = sector_range(lba, HEADER_SIZE);
    bytes[range.start + 16..range.start + 20].fill(0);
    let checksum = crc().checksum(&bytes[range.clone()]);
    bytes[range.start + 16..range.start + 20].copy_from_slice(&checksum.to_le_bytes());
}

fn crc() -> Crc<u32> {
    Crc::<u32>::new(&CRC_32_ISO_HDLC)
}
fn header_mut(bytes: &mut [u8], lba: usize) -> &mut [u8] {
    &mut bytes[sector_range(lba, SECTOR_SIZE)]
}
fn byte_mut(bytes: &mut [u8], lba: usize, offset: usize) -> &mut [u8] {
    &mut bytes[lba * SECTOR_SIZE + offset..]
}
fn sector_range(lba: usize, len: usize) -> std::ops::Range<usize> {
    let start = lba * SECTOR_SIZE;
    start..start + len
}
fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
