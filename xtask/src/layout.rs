use std::ops::Range;

pub const SECTOR_SIZE: u64 = 512;
pub const ESP_RANGE: Range<u64> = 2_048..133_120;
pub const ROOT_RANGE: Range<u64> = 133_120..395_264;
pub const DISK_SECTORS: u64 = 397_312;

pub const DISK_GUID: &str = "52454c41-5900-4000-8000-000000000001";
pub const ESP_GUID: &str = "52454c41-5900-4000-8000-000000000002";
pub const ROOT_GUID: &str = "52454c41-5900-4000-8000-000000000003";
pub const EXT2_UUID: &str = "52454c41-5900-4000-8000-000000000004";

pub const fn root_config() -> &'static [u8] {
    b"root-guid=52454c41-5900-4000-8000-000000000003\n"
}

pub const fn sectors_to_bytes(sectors: u64) -> Option<u64> {
    sectors.checked_mul(SECTOR_SIZE)
}
