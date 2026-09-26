//! Fixed facts about the build outputs, the disk layout and the test hardware.

pub const OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE_4M.fd";
pub const OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS_4M.fd";

pub const SECTOR: u64 = 512;
pub const IMAGE_BYTES: u64 = 256 << 20;
pub const ESP_START_LBA: u64 = 2048;
pub const ESP_SECTORS: u64 = 131_072; // 64 MiB
pub const ROOT_START_LBA: u64 = ESP_START_LBA + ESP_SECTORS;

pub const ESP_TYPE_GUID: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";
pub const LINUX_FS_TYPE_GUID: &str = "0FC63DAF-8483-4772-8E79-3D69D8477DE4";
/// Fixed GUIDs for the QEMU image so runs are reproducible. The stick gets
/// random ones from sfdisk.
pub const IMAGE_DISK_GUID: &str = "52454C41-5900-4000-8000-000000000001";
pub const IMAGE_ESP_GUID: &str = "52454C41-5900-4000-8000-000000000002";
pub const IMAGE_ROOT_GUID: &str = "52454C41-5900-4000-8000-000000000003";

pub const ESP_LABEL: &str = "RELAYESP";
pub const ROOT_LABEL: &str = "relayroot";
pub const EXT2_BLOCK: u64 = 4096;
pub const MKE2FS_FEATURES: &str = "^dir_index,^resize_inode,^ext_attr";

/// Top-level directories of `/` and their modes. Files come from `rootfs/`.
pub const ROOT_DIRS: &[(&str, u32)] = &[
    ("bin", 0o755),
    ("dev", 0o755),
    ("etc", 0o755),
    ("home", 0o755),
    ("root", 0o755),
    ("tmp", 0o1777),
    ("usr", 0o755),
    ("var", 0o755),
];

pub const UEFI_TARGET: &str = "x86_64-unknown-uefi";
pub const KERNEL_TARGET: &str = "x86_64-unknown-none";
pub const PROFILE: &str = "relay";
