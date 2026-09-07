#[test]
fn image_geometry_is_fixed() {
    assert_eq!(relay_xtask::layout::SECTOR_SIZE, 512);
    assert_eq!(relay_xtask::layout::ESP_RANGE, 2_048..133_120);
    assert_eq!(relay_xtask::layout::ROOT_RANGE, 133_120..395_264);
    assert_eq!(relay_xtask::layout::DISK_SECTORS, 397_312);
}

#[test]
fn root_config_uses_the_canonical_root_guid() {
    assert_eq!(
        relay_xtask::layout::root_config(),
        b"root-guid=52454c41-5900-4000-8000-000000000003\n"
    );
}

#[test]
fn generated_image_verifies_its_partitioned_boot_config() {
    let _lock = image_build_lock();
    let image = std::path::Path::new("target/image-test.img");
    relay_xtask::image::build(image).unwrap();
    if let Err(error) = relay_xtask::verify::verify_image(image) {
        panic!("generated image failed verification: {error}");
    }
}

#[test]
fn generated_images_are_byte_identical() {
    let _lock = image_build_lock();
    let first = std::path::Path::new("target/image-first.img");
    let second = std::path::Path::new("target/image-second.img");
    relay_xtask::image::build(first).unwrap();
    relay_xtask::image::build(second).unwrap();
    assert_eq!(
        std::fs::read(first).unwrap(),
        std::fs::read(second).unwrap()
    );
}

#[test]
fn verifier_rejects_wrong_partition_types_and_populated_extra_entries() {
    let _lock = image_build_lock();
    let image = std::path::Path::new("target/image-gpt-regression.img");
    relay_xtask::image::build(image).unwrap();
    let original = std::fs::read(image).unwrap();

    let mut wrong_esp_type = original.clone();
    wrong_esp_type[GPT_ENTRIES..GPT_ENTRIES + 16].copy_from_slice(&LINUX_FILESYSTEM_TYPE);
    update_gpt_checksums(&mut wrong_esp_type);
    let wrong_esp_type_path = std::path::Path::new("target/image-wrong-esp-type.img");
    std::fs::write(wrong_esp_type_path, wrong_esp_type).unwrap();
    assert!(relay_xtask::verify::verify_image(wrong_esp_type_path).is_err());

    let mut extra_partition = original;
    let entry = GPT_ENTRIES + 2 * GPT_ENTRY_SIZE;
    extra_partition[entry..entry + 16].copy_from_slice(&EFI_SYSTEM_PARTITION_TYPE);
    extra_partition[entry + 16..entry + 32].copy_from_slice(&EXTRA_PARTITION_GUID);
    extra_partition[entry + 32..entry + 40].copy_from_slice(&395_264_u64.to_le_bytes());
    extra_partition[entry + 40..entry + 48].copy_from_slice(&397_278_u64.to_le_bytes());
    update_gpt_checksums(&mut extra_partition);
    let extra_partition_path = std::path::Path::new("target/image-extra-partition.img");
    std::fs::write(extra_partition_path, extra_partition).unwrap();
    assert!(relay_xtask::verify::verify_image(extra_partition_path).is_err());
}

#[test]
fn verifier_rejects_every_omitted_ext2_profile_field() {
    let _lock = image_build_lock();
    let image = std::path::Path::new("target/image-ext2-regression.img");
    relay_xtask::image::build(image).unwrap();
    let original = std::fs::read(image).unwrap();

    for (name, offset, value) in [
        ("uuid", EXT2_SUPERBLOCK + 104, 0xff),
        ("inode-count", EXT2_SUPERBLOCK, 0xff),
        ("first-data-block", EXT2_SUPERBLOCK + 20, 0x01),
    ] {
        let mut corrupted = original.clone();
        corrupted[offset] ^= value;
        let path = format!("target/image-ext2-{name}.img");
        std::fs::write(&path, corrupted).unwrap();
        let error = match relay_xtask::verify::verify_image(std::path::Path::new(&path)) {
            Ok(_) => panic!("verifier accepted ext2 with wrong {name}"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("ext2 filesystem violates the fixed profile")
        );
    }
}

const SECTOR_SIZE: usize = 512;
const DISK_SECTORS: usize = 397_312;
const GPT_ENTRIES: usize = 2 * SECTOR_SIZE;
const GPT_ENTRY_SIZE: usize = 128;
const GPT_ENTRY_COUNT: usize = 128;
const GPT_HEADER_SIZE: usize = 92;
const ROOT_START: usize = 133_120 * SECTOR_SIZE;
const EXT2_SUPERBLOCK: usize = ROOT_START + 1024;
const EFI_SYSTEM_PARTITION_TYPE: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0xf8, 0x1f, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
const LINUX_FILESYSTEM_TYPE: [u8; 16] = [
    0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];
const EXTRA_PARTITION_GUID: [u8; 16] = [
    0x41, 0x4c, 0x45, 0x52, 0x00, 0x59, 0x00, 0x40, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];

fn update_gpt_checksums(bytes: &mut [u8]) {
    let crc = Crc::<u32>::new(&CRC_32_ISO_HDLC);
    let entries_end = GPT_ENTRIES + GPT_ENTRY_COUNT * GPT_ENTRY_SIZE;
    let entries_crc = crc.checksum(&bytes[GPT_ENTRIES..entries_end]);
    let entries = bytes[GPT_ENTRIES..entries_end].to_vec();
    let backup_entries = (DISK_SECTORS - 33) * SECTOR_SIZE;
    bytes[backup_entries..backup_entries + GPT_ENTRY_COUNT * GPT_ENTRY_SIZE]
        .copy_from_slice(&entries);
    for header in [SECTOR_SIZE, (DISK_SECTORS - 1) * SECTOR_SIZE] {
        bytes[header + 88..header + 92].copy_from_slice(&entries_crc.to_le_bytes());
        bytes[header + 16..header + 20].fill(0);
        let header_crc = crc.checksum(&bytes[header..header + GPT_HEADER_SIZE]);
        bytes[header + 16..header + 20].copy_from_slice(&header_crc.to_le_bytes());
    }
}

static IMAGE_BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn image_build_lock() -> std::sync::MutexGuard<'static, ()> {
    IMAGE_BUILD_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}
use crc::{CRC_32_ISO_HDLC, Crc};
