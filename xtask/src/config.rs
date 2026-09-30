//! Fixed facts about the build outputs, the disk layout and the test hardware.

/// The Kingston DataTraveler 3.0 test stick, by its reported serial number.
pub const USB_SERIAL: &str = "08606E6D413FB27127135F8E";
pub const USB_BY_ID: &str =
    "/dev/disk/by-id/usb-Kingston_DataTraveler_3.0_08606E6D413FB27127135F8E-0:0";
/// Refuse to touch anything bigger than this.
pub const USB_MAX_BYTES: u64 = 64 << 30;

pub const OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE_4M.fd";
pub const OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS_4M.fd";

pub const SECTOR: u64 = 512;
pub const IMAGE_BYTES: u64 = 256 << 20;
/// The `diskfull` scenario's image (spec §9.3 #7): the same ESP and a
/// 32 MiB ext2 root.
pub const SMALL_IMAGE_BYTES: u64 = (ROOT_START_LBA * SECTOR) + (32 << 20) + (1 << 20);
pub const ESP_START_LBA: u64 = 2048;
pub const ESP_SECTORS: u64 = 131_072; // 64 MiB
pub const ROOT_START_LBA: u64 = ESP_START_LBA + ESP_SECTORS;
/// The ext2 root on the stick: 2 GiB, not the rest of the stick. ext2 has
/// no lazy inode-table initialisation and USB sticks take no discard, so
/// `mke2fs` writes every inode table over USB: about 32 MiB for 2 GiB, 230
/// MiB for the whole 15.4 GB stick. The rest of the stick stays unused.
pub const STICK_ROOT_SECTORS: u64 = (2 << 30) / SECTOR;

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
/// User programs (spec §8.4): `relay` plus LTO, without debug info.
pub const USER_PROFILE: &str = "user";
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-sh", "relay-tests", "relay-utils"];
