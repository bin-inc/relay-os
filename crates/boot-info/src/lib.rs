//! The hand-off contract between `relay-boot` and `relay-kernel`.
//!
//! Everything here is `#[repr(C)]` and contains no pointers into loader
//! memory except `memory_map_ptr`, which is a kernel-virtual address inside
//! the linear physical-memory map.
#![cfg_attr(not(test), no_std)]

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
/// Physical addresses below this can be reached through the linear map.
pub const PHYS_MAP_MAX: u64 = 64 << 40;
/// The kernel heap: a fixed virtual region, backed by frames when the kernel
/// sets it up.
pub const HEAP_BASE: u64 = 0xFFFF_C000_0000_0000;
pub const HEAP_SIZE: u64 = 32 << 20;

pub const CMDLINE_MAX: usize = 256;

// The virtual layout must not overlap: the linear map covers up to 64 TiB of
// physical address space, then come the heap, the stack (with its guard
// page) and the kernel image.
const _: () = {
    assert!(PHYS_OFFSET + PHYS_MAP_MAX <= HEAP_BASE);
    assert!(HEAP_BASE + HEAP_SIZE <= KERNEL_STACK_BOTTOM - 0x1000);
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

/// Colours (0xRRGGBB) of the boot-progress squares, stage 1 first.
pub const STAGE_COLORS: [u32; 12] = [
    0xFF0000, 0xFF8000, 0xFFFF00, 0x00FF00, 0x00FFFF, 0x0040FF, 0xFF00FF, 0xFFFFFF, 0x808080,
    0xFF8080, 0x80FF80, 0x8080FF,
];
const STAGE_SIZE: u32 = 24;
const STAGE_GAP: u32 = 8;
const STAGE_TOP: u32 = 8;

impl FramebufferInfo {
    /// Paints the square for boot `stage` (1-based) along the top edge of the
    /// screen, stage 1 rightmost. This works where no console does: in the
    /// loader after it has taken over the GOP, and in the kernel before its
    /// console exists. The last square on screen shows how far boot got.
    /// Squares are clipped to the visible area.
    ///
    /// # Safety
    /// `base` must be the virtual address of the framebuffer (identity-mapped
    /// in the loader, `PHYS_OFFSET + phys_addr` in the kernel), valid for
    /// `stride * height` pixels.
    pub unsafe fn mark_stage(&self, base: u64, stage: u32) {
        let rgb = STAGE_COLORS[(stage.max(1) as usize - 1) % STAGE_COLORS.len()];
        let px = match self.format {
            PixelFormat::Bgr => rgb,
            PixelFormat::Rgb => ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | (rgb >> 16),
        };
        let right = self
            .width
            .saturating_sub(stage.max(1) * (STAGE_SIZE + STAGE_GAP) - STAGE_SIZE);
        let left = right.saturating_sub(STAGE_SIZE);
        let bottom = (STAGE_TOP + STAGE_SIZE).min(self.height);
        for y in STAGE_TOP..bottom {
            for x in left..right.min(self.width) {
                let offset = (y as usize * self.stride as usize + x as usize) * 4;
                unsafe { core::ptr::write_volatile((base as usize + offset) as *mut u32, px) };
            }
        }
    }
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
    /// Not produced by relay-boot: the kernel image is reported as
    /// `Bootloader`, and `BootInfo::kernel_phys_*` gives its exact range.
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

    fn fb(width: u32, height: u32, stride: u32, format: PixelFormat) -> FramebufferInfo {
        FramebufferInfo {
            phys_addr: 0,
            size: u64::from(stride * height * 4),
            width,
            height,
            stride,
            format,
        }
    }

    #[test]
    fn stage_marks_are_squares_along_the_top_right() {
        let info = fb(200, 100, 256, PixelFormat::Bgr);
        let mut px = vec![0u32; 256 * 100];
        unsafe { info.mark_stage(px.as_mut_ptr() as u64, 1) };
        // Stage 1 is the rightmost square: x 168..192, y 8..32.
        let red = STAGE_COLORS[0];
        assert_eq!(px[8 * 256 + 168], red);
        assert_eq!(px[31 * 256 + 191], red);
        assert_eq!(px[8 * 256 + 192], 0, "gap to the right edge");
        assert_eq!(px[7 * 256 + 168], 0, "top margin");
        assert_eq!(px[32 * 256 + 168], 0, "square is 24 px tall");
        unsafe { info.mark_stage(px.as_mut_ptr() as u64, 2) };
        assert_eq!(
            px[8 * 256 + 136],
            STAGE_COLORS[1],
            "stage 2 sits left of stage 1"
        );
    }

    #[test]
    fn stage_marks_follow_the_pixel_format() {
        let info = fb(64, 40, 64, PixelFormat::Rgb);
        let mut px = vec![0u32; 64 * 40];
        unsafe { info.mark_stage(px.as_mut_ptr() as u64, 2) };
        // Stage 2 is orange 0xFF8000; RGB memory order stores it as 0x0080FF.
        assert_eq!(STAGE_COLORS[1], 0xFF8000);
        assert_eq!(px[8 * 64], 0x0080FF, "leftmost column of the square");
    }

    #[test]
    fn stage_marks_never_write_outside_the_framebuffer() {
        let info = fb(10, 12, 10, PixelFormat::Bgr);
        let mut px = vec![0u32; 10 * 12 + 16];
        px[120..].fill(0xDEAD_BEEF);
        for stage in 1..=20 {
            unsafe { info.mark_stage(px.as_mut_ptr() as u64, stage) };
        }
        assert!(px[120..].iter().all(|&p| p == 0xDEAD_BEEF));
        assert!(px[..80].iter().all(|&p| p == 0), "rows above y=8 untouched");
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
