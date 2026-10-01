//! What `proc_list` fills in (spec §7.3, §9.3): one [`ProcInfo`] per
//! process, for `ps`.

/// The most processes that exist at once (spec §5.4), so a buffer of this
/// many entries always holds every one.
pub const PROC_MAX: usize = 64;
/// The length of [`ProcInfo::name`].
pub const PROC_NAME: usize = 64;

/// [`ProcInfo::state`]: it runs now (the caller of `proc_list` does).
pub const STATE_RUN: u32 = 1;
/// [`ProcInfo::state`]: it waits for its turn on the CPU.
pub const STATE_READY: u32 = 2;
/// [`ProcInfo::state`]: it waits in `wait` for a child to end.
pub const STATE_WAIT: u32 = 3;
/// [`ProcInfo::state`]: it waits for what is typed on the console.
pub const STATE_READ: u32 = 4;
/// [`ProcInfo::state`]: it waits in `sleep`.
pub const STATE_SLEEP: u32 = 5;
/// [`ProcInfo::state`]: it waits for data in a pipe, or room in one.
pub const STATE_PIPE: u32 = 6;
/// [`ProcInfo::state`]: it has ended, and its parent has not collected it.
pub const STATE_ZOMBIE: u32 = 7;

/// One process, as `proc_list` reports it, `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    /// Its parent (process 1 for an orphan, 0 for process 1).
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
    /// One of the `STATE_*` numbers.
    pub state: u32,
    /// The frames of memory its address space holds: its pages and their
    /// page tables (0 for a zombie and for process 1, which has none).
    pub frames: u64,
    /// The timer ticks (milliseconds) it has run for.
    pub ticks: u64,
    /// The path it was started from, padded with NULs, cut at
    /// [`PROC_NAME`] bytes.
    pub name: [u8; PROC_NAME],
}

impl ProcInfo {
    pub const SIZE: usize = core::mem::size_of::<ProcInfo>();

    /// A `ProcInfo` whose name is `name`, cut at [`PROC_NAME`] bytes.
    pub fn new(
        pid: u32,
        ppid: u32,
        pgid: u32,
        state: u32,
        frames: u64,
        ticks: u64,
        name: &[u8],
    ) -> ProcInfo {
        let mut field = [0; PROC_NAME];
        let n = name.len().min(PROC_NAME);
        field[..n].copy_from_slice(&name[..n]);
        ProcInfo {
            pid,
            ppid,
            pgid,
            state,
            frames,
            ticks,
            name: field,
        }
    }

    /// Its name, without the padding.
    pub fn name(&self) -> &[u8] {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(PROC_NAME);
        &self.name[..end]
    }

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; ProcInfo::SIZE] {
        let mut b = [0; ProcInfo::SIZE];
        for (i, v) in [self.pid, self.ppid, self.pgid, self.state]
            .iter()
            .enumerate()
        {
            b[4 * i..4 * i + 4].copy_from_slice(&v.to_ne_bytes());
        }
        b[16..24].copy_from_slice(&self.frames.to_ne_bytes());
        b[24..32].copy_from_slice(&self.ticks.to_ne_bytes());
        b[32..].copy_from_slice(&self.name);
        b
    }

    /// The struct whose bytes are `b`.
    pub fn from_bytes(b: &[u8; ProcInfo::SIZE]) -> ProcInfo {
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        ProcInfo {
            pid: u32_at(0),
            ppid: u32_at(4),
            pgid: u32_at(8),
            state: u32_at(12),
            frames: u64_at(16),
            ticks: u64_at(24),
            name: b[32..].try_into().unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<ProcInfo>(), 96);
        assert_eq!(ProcInfo::SIZE, 96);
        assert_eq!(offset_of!(ProcInfo, pid), 0);
        assert_eq!(offset_of!(ProcInfo, ppid), 4);
        assert_eq!(offset_of!(ProcInfo, pgid), 8);
        assert_eq!(offset_of!(ProcInfo, state), 12);
        assert_eq!(offset_of!(ProcInfo, frames), 16);
        assert_eq!(offset_of!(ProcInfo, ticks), 24);
        assert_eq!(offset_of!(ProcInfo, name), 32);
        assert_eq!((PROC_MAX, PROC_NAME), (64, 64));
        assert_eq!(
            [
                STATE_RUN,
                STATE_READY,
                STATE_WAIT,
                STATE_READ,
                STATE_SLEEP,
                STATE_PIPE,
                STATE_ZOMBIE
            ],
            [1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn its_bytes_are_the_struct_s_and_its_name_is_cut_at_64() {
        let p = ProcInfo::new(7, 2, 5, STATE_SLEEP, 41, 1234, b"/bin/sleep");
        assert_eq!(p.name(), b"/bin/sleep");
        // SAFETY: `repr(C)` of integers and bytes, no padding.
        let mem: [u8; ProcInfo::SIZE] = unsafe { core::mem::transmute(p) };
        assert_eq!(p.to_bytes(), mem);
        assert_eq!(ProcInfo::from_bytes(&mem), p);
        let long = [b'x'; 70];
        assert_eq!(
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 0, &long).name(),
            &long[..64]
        );
    }
}
