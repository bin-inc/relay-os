//! What `time` and `sys_info` fill in (spec §7.3).

/// `time`'s answer, `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// Wall-clock seconds since 1970, UTC (0 without a clock).
    pub unix_seconds: u64,
    /// Nanoseconds since the machine started.
    pub uptime_ns: u64,
}

/// `sys_info`'s kind for memory figures ([`MemInfo`]).
pub const INFO_MEMORY: u32 = 1;
/// `sys_info`'s kind for the system's names ([`Uname`]).
pub const INFO_UNAME: u32 = 2;
/// `sys_info`'s kind for the kernel log (for `dmesg`): its newest bytes,
/// as many as the buffer holds. The log keeps at most [`LOG_MAX`].
pub const INFO_LOG: u32 = 3;
/// The most bytes the kernel log holds.
pub const LOG_MAX: usize = 64 * 1024;

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

/// The length of each of [`Uname`]'s fields.
pub const UNAME_FIELD: usize = 64;

/// What `uname` prints, `#[repr(C)]` with no padding: each field a name,
/// padded with NULs.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uname {
    /// `Relay`.
    pub sysname: [u8; UNAME_FIELD],
    /// `relay`.
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.4.0`.
    pub release: [u8; UNAME_FIELD],
    /// The machine, `x86_64`.
    pub machine: [u8; UNAME_FIELD],
}

impl Uname {
    pub const SIZE: usize = core::mem::size_of::<Uname>();

    /// A `Uname` from its names, each cut at [`UNAME_FIELD`] bytes.
    pub fn new(sysname: &[u8], nodename: &[u8], release: &[u8], machine: &[u8]) -> Uname {
        fn field(name: &[u8]) -> [u8; UNAME_FIELD] {
            let mut f = [0; UNAME_FIELD];
            let n = name.len().min(UNAME_FIELD);
            f[..n].copy_from_slice(&name[..n]);
            f
        }
        Uname {
            sysname: field(sysname),
            nodename: field(nodename),
            release: field(release),
            machine: field(machine),
        }
    }

    /// A field's name, without its padding.
    pub fn name(field: &[u8; UNAME_FIELD]) -> &[u8] {
        let end = field.iter().position(|&b| b == 0).unwrap_or(UNAME_FIELD);
        &field[..end]
    }

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; Uname::SIZE] {
        let mut b = [0; Uname::SIZE];
        for (i, f) in [self.sysname, self.nodename, self.release, self.machine]
            .iter()
            .enumerate()
        {
            b[UNAME_FIELD * i..UNAME_FIELD * (i + 1)].copy_from_slice(f);
        }
        b
    }
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
        assert_eq!((INFO_MEMORY, INFO_UNAME, INFO_LOG), (1, 2, 3));
        assert_eq!(Uname::SIZE, 256);
        assert_eq!(offset_of!(Uname, sysname), 0);
        assert_eq!(offset_of!(Uname, nodename), 64);
        assert_eq!(offset_of!(Uname, release), 128);
        assert_eq!(offset_of!(Uname, machine), 192);
    }

    #[test]
    fn uname_s_fields_are_padded_names() {
        let u = Uname::new(b"Relay", b"relay", b"0.2.0", &[b'x'; 70]);
        assert_eq!(Uname::name(&u.sysname), b"Relay");
        assert_eq!(Uname::name(&u.release), b"0.2.0");
        assert_eq!(Uname::name(&u.machine), &[b'x'; 64], "cut at 64 bytes");
        // SAFETY: `repr(C)` of byte arrays, no padding.
        let mem: [u8; Uname::SIZE] = unsafe { core::mem::transmute(u) };
        assert_eq!(u.to_bytes(), mem);
        assert_eq!(mem[64..70], *b"relay\0");
    }
}
