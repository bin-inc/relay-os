use crate::GptGuid;

pub const BOOT_INFO_MAGIC: u64 = 0x5245_4c41_5942_4f4f;
pub const BOOT_ABI_VERSION: u32 = 1;

#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub abi_version: u32,
    pub struct_size: u32,
    pub memory_map: MemoryMap,
    pub framebuffer: FramebufferInfo,
    pub root_partition_guid: GptGuid,
    pub physical_memory_offset: u64,
    pub acpi_rsdp_phys: u64,
}

#[repr(C)]
pub struct MemoryMap {
    pub entries_address: u64,
    pub entry_count: u64,
}

#[repr(C)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct FramebufferInfo {
    pub physical_base: u64,
    pub byte_len: u64,
    pub width: u32,
    pub height: u32,
    pub stride_pixels: u32,
    pub bytes_per_pixel: u32,
    pub pixel_format: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

const _: () = assert!(core::mem::size_of::<MemoryRegion>() == 24);
const _: () = assert!(core::mem::size_of::<FramebufferInfo>() == 56);
const _: () = assert!(core::mem::size_of::<BootInfo>() == 120);
