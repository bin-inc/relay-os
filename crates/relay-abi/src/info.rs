//! What `time` and `sys_info` fill in (spec §7.3).

/// `time`'s answer, `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// Wall-clock seconds since 1970, UTC (0 without a clock).
    pub unix_seconds: u64,
    /// Nanoseconds since the kernel's timer started.
    pub uptime_ns: u64,
}

/// `sys_info`'s kind for memory figures ([`MemInfo`]).
pub const INFO_MEMORY: u32 = 1;

/// The memory figures of M1's `free`, in bytes, `#[repr(C)]` with no
/// padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub ram_total: u64,
    pub ram_free: u64,
    pub heap_total: u64,
    pub heap_used: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layouts_are_fixed() {
        assert_eq!(size_of::<Time>(), 16);
        assert_eq!(offset_of!(Time, unix_seconds), 0);
        assert_eq!(offset_of!(Time, uptime_ns), 8);
        assert_eq!(size_of::<MemInfo>(), 32);
        assert_eq!(offset_of!(MemInfo, ram_total), 0);
        assert_eq!(offset_of!(MemInfo, ram_free), 8);
        assert_eq!(offset_of!(MemInfo, heap_total), 16);
        assert_eq!(offset_of!(MemInfo, heap_used), 24);
        assert_eq!(INFO_MEMORY, 1);
    }
}
