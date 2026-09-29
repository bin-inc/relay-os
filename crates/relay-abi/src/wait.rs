//! How a child ended, as `wait` reports it (spec §7.3): it exited with a
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction.

/// `WaitStatus::how`: the child called `exit`.
pub const EXITED: u32 = 1;
/// `WaitStatus::how`: the child was killed; `code` says why.
pub const KILLED: u32 = 2;

/// `WaitStatus` (spec §7.3), `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaitStatus {
    /// [`EXITED`] or [`KILLED`].
    pub how: u32,
    /// The exit code, or why the child was killed.
    pub code: u32,
    /// For a fault: what the CPU refused.
    pub fault: u32,
    /// For a fault: more about it (the access of a page fault).
    pub detail: u32,
    /// For a fault: the address it concerned.
    pub address: u64,
    /// For a fault: the instruction it happened at.
    pub ip: u64,
}

impl WaitStatus {
    /// A child that exited with `code`.
    pub const fn exited(code: u8) -> WaitStatus {
        WaitStatus {
            how: EXITED,
            code: code as u32,
            fault: 0,
            detail: 0,
            address: 0,
            ip: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<WaitStatus>(), 32);
        assert_eq!(offset_of!(WaitStatus, how), 0);
        assert_eq!(offset_of!(WaitStatus, code), 4);
        assert_eq!(offset_of!(WaitStatus, fault), 8);
        assert_eq!(offset_of!(WaitStatus, detail), 12);
        assert_eq!(offset_of!(WaitStatus, address), 16);
        assert_eq!(offset_of!(WaitStatus, ip), 24);
    }

    #[test]
    fn an_exit_carries_only_its_code() {
        let w = WaitStatus::exited(3);
        assert_eq!((w.how, w.code), (EXITED, 3));
        assert_eq!(
            WaitStatus {
                how: EXITED,
                code: 3,
                ..Default::default()
            },
            w
        );
        assert_ne!(EXITED, KILLED);
    }
}
