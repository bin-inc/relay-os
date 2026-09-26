//! The hand-off contract between `relay-boot` and `relay-kernel`.
//!
//! Everything here is `#[repr(C)]` and contains no pointers into loader
//! memory except `memory_map_ptr`, which is a kernel-virtual address inside
//! the linear physical-memory map.
#![no_std]

/// `"RELAYBOO"` in ASCII, little-endian.
pub const BOOT_INFO_MAGIC: u64 = u64::from_le_bytes(*b"RELAYBOO");
pub const BOOT_INFO_VERSION: u32 = 1;

/// Virtual address the kernel ELF is linked at.
pub const KERNEL_BASE: u64 = 0xFFFF_FFFF_8000_0000;
/// Base of the linear map of physical memory: virt = PHYS_OFFSET + phys.
pub const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;
/// Lowest mapped byte of the kernel stack. The page below it is left unmapped
/// as a guard page.
pub const KERNEL_STACK_BOTTOM: u64 = 0xFFFF_FE00_0000_1000;
pub const KERNEL_STACK_SIZE: u64 = 64 * 1024;
pub const KERNEL_STACK_TOP: u64 = KERNEL_STACK_BOTTOM + KERNEL_STACK_SIZE;
/// UEFI memory type (OS-defined range) the loader uses for kernel pages.
pub const UEFI_KERNEL_MEMORY_TYPE: u32 = 0x8000_5245;

pub const CMDLINE_MAX: usize = 256;

// The virtual layout must not overlap: the linear map may cover up to 64 TiB
// of physical address space, then comes the stack (with its guard page),
// then the kernel image.
const _: () = {
    assert!(PHYS_OFFSET + (64u64 << 40) <= KERNEL_STACK_BOTTOM - 0x1000);
    assert!(KERNEL_STACK_TOP < KERNEL_BASE);
    assert!(KERNEL_STACK_TOP.is_multiple_of(16));
};

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Byte order in memory: R, G, B, reserved.
    Rgb = 0,
    /// Byte order in memory: B, G, R, reserved.
    Bgr = 1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramebufferInfo {
    pub phys_addr: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    /// Pixels per scan line (>= width).
    pub stride: u32,
    pub format: PixelFormat,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryKind {
    Usable = 1,
    Bootloader = 2,
    AcpiReclaimable = 3,
    AcpiNvs = 4,
    Reserved = 5,
    Mmio = 6,
    Kernel = 7,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    pub start: u64,
    pub len: u64,
    pub kind: MemoryKind,
}

impl MemoryRegion {
    pub const fn end(&self) -> u64 {
        self.start + self.len
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub framebuffer: FramebufferInfo,
    /// Kernel-virtual pointer to `memory_map_len` regions, sorted by `start`.
    pub memory_map_ptr: u64,
    pub memory_map_len: u64,
    /// Physical address of the ACPI 2.0 RSDP, or 0 if the firmware had none.
    pub rsdp_addr: u64,
    pub phys_offset: u64,
    pub kernel_phys_start: u64,
    pub kernel_phys_len: u64,
    pub cmdline: [u8; CMDLINE_MAX],
    pub cmdline_len: u32,
    /// GPT unique partition GUID of the ESP we booted from, in on-disk byte
    /// order. Only meaningful when `has_boot_partition_guid` is 1.
    pub boot_partition_guid: [u8; 16],
    pub has_boot_partition_guid: u32,
}

impl BootInfo {
    pub fn is_valid(&self) -> bool {
        self.magic == BOOT_INFO_MAGIC && self.version == BOOT_INFO_VERSION
    }

    /// # Safety
    /// `memory_map_ptr` must point to `memory_map_len` initialised regions
    /// that live for `'static` (true for a `BootInfo` built by `relay-boot`).
    pub unsafe fn memory_map(&self) -> &'static [MemoryRegion] {
        unsafe {
            core::slice::from_raw_parts(
                self.memory_map_ptr as *const MemoryRegion,
                self.memory_map_len as usize,
            )
        }
    }

    pub fn cmdline(&self) -> &str {
        let len = (self.cmdline_len as usize).min(CMDLINE_MAX);
        core::str::from_utf8(&self.cmdline[..len]).unwrap_or("")
    }

    pub fn boot_partition_guid(&self) -> Option<[u8; 16]> {
        (self.has_boot_partition_guid == 1).then_some(self.boot_partition_guid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> BootInfo {
        BootInfo {
            magic: BOOT_INFO_MAGIC,
            version: BOOT_INFO_VERSION,
            framebuffer: FramebufferInfo {
                phys_addr: 0,
                size: 0,
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::Bgr,
            },
            memory_map_ptr: 0,
            memory_map_len: 0,
            rsdp_addr: 0,
            phys_offset: PHYS_OFFSET,
            kernel_phys_start: 0,
            kernel_phys_len: 0,
            cmdline: [0; CMDLINE_MAX],
            cmdline_len: 0,
            boot_partition_guid: [0; 16],
            has_boot_partition_guid: 0,
        }
    }

    #[test]
    fn magic_spells_relayboo() {
        assert_eq!(&BOOT_INFO_MAGIC.to_le_bytes(), b"RELAYBOO");
    }

    #[test]
    fn validity_checks_magic_and_version() {
        let mut bi = blank();
        assert!(bi.is_valid());
        bi.version = 2;
        assert!(!bi.is_valid());
        bi.version = BOOT_INFO_VERSION;
        bi.magic = 0;
        assert!(!bi.is_valid());
    }

    #[test]
    fn cmdline_is_clamped_and_utf8_checked() {
        let mut bi = blank();
        bi.cmdline[..6].copy_from_slice(b"test=1");
        bi.cmdline_len = 6;
        assert_eq!(bi.cmdline(), "test=1");
        bi.cmdline_len = 9999;
        assert_eq!(bi.cmdline().len(), CMDLINE_MAX);
        bi.cmdline[0] = 0xFF;
        assert_eq!(bi.cmdline(), "");
    }

    #[test]
    fn guid_only_when_flagged() {
        let mut bi = blank();
        bi.boot_partition_guid = [7; 16];
        assert_eq!(bi.boot_partition_guid(), None);
        bi.has_boot_partition_guid = 1;
        assert_eq!(bi.boot_partition_guid(), Some([7; 16]));
    }
}
