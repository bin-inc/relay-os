//! How a child ended, as `wait` reports it (spec §7.3): it exited with a
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction. Its `Display` says so in the
//! words of spec §11.1 (`page fault at 0x0, read, ip 0x401a2c`), for the
//! kernel log and the shells.

use core::fmt;

/// `WaitStatus::how`: the child called `exit`.
pub const EXITED: u32 = 1;
/// `WaitStatus::how`: the child was killed; `code` says why.
pub const KILLED: u32 = 2;

/// `WaitStatus::code` of a killed child: the CPU refused what it did.
pub const KILLED_FAULT: u32 = 1;

/// `WaitStatus::fault`: what the CPU refused (spec §11.1), in words any
/// architecture has.
pub const FAULT_PAGE: u32 = 1;
pub const FAULT_GENERAL_PROTECTION: u32 = 2;
pub const FAULT_INVALID_OPCODE: u32 = 3;
pub const FAULT_DIVIDE: u32 = 4;
/// A floating-point or SIMD instruction (spec §5.5).
pub const FAULT_FPU: u32 = 5;
/// A page fault in the stack's guard page.
pub const FAULT_STACK_OVERFLOW: u32 = 6;
/// Any other exception; `detail` is the architecture's number for it.
pub const FAULT_OTHER: u32 = 7;

/// `WaitStatus::detail` of a page fault or stack overflow: the access.
pub const ACCESS_READ: u32 = 1;
pub const ACCESS_WRITE: u32 = 2;
pub const ACCESS_EXECUTE: u32 = 3;

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
    /// For a fault: more about it (the access of a page fault, the
    /// exception number of `FAULT_OTHER`).
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

    /// Whether the status is a fault whose kind this ABI knows.
    pub const fn is_known_fault(&self) -> bool {
        self.how == KILLED
            && self.code == KILLED_FAULT
            && self.fault >= FAULT_PAGE
            && self.fault <= FAULT_OTHER
    }

    /// A child killed for `fault` at instruction `ip`.
    pub const fn fault(fault: u32, detail: u32, address: u64, ip: u64) -> WaitStatus {
        WaitStatus {
            how: KILLED,
            code: KILLED_FAULT,
            fault,
            detail,
            address,
            ip,
        }
    }
}

impl fmt::Display for WaitStatus {
    /// `exited with 3`; for a fault what it was, where and at which
    /// instruction; `killed` for any other end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.how == EXITED {
            return write!(f, "exited with {}", self.code);
        }
        if !self.is_known_fault() {
            return f.write_str("killed");
        }
        let access = match self.detail {
            ACCESS_READ => "read",
            ACCESS_WRITE => "write",
            ACCESS_EXECUTE => "execute",
            _ => "access",
        };
        let address = self.address;
        match self.fault {
            FAULT_PAGE => write!(f, "page fault at {address:#x}, {access}")?,
            FAULT_STACK_OVERFLOW => write!(f, "stack overflow at {address:#x}")?,
            FAULT_GENERAL_PROTECTION => f.write_str("general protection fault")?,
            FAULT_INVALID_OPCODE => f.write_str("invalid opcode")?,
            FAULT_DIVIDE => f.write_str("divide error")?,
            FAULT_FPU => f.write_str("FPU/SSE instruction")?,
            _ => write!(f, "CPU exception {}", self.detail)?,
        }
        write!(f, ", ip {:#x}", self.ip)
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

    #[test]
    fn a_fault_carries_its_kind_address_and_instruction() {
        let w = WaitStatus::fault(FAULT_PAGE, ACCESS_WRITE, 0x10, 0x40_1000);
        assert_eq!(
            w,
            WaitStatus {
                how: KILLED,
                code: KILLED_FAULT,
                fault: FAULT_PAGE,
                detail: ACCESS_WRITE,
                address: 0x10,
                ip: 0x40_1000
            }
        );
        let kinds = [
            FAULT_PAGE,
            FAULT_GENERAL_PROTECTION,
            FAULT_INVALID_OPCODE,
            FAULT_DIVIDE,
            FAULT_FPU,
            FAULT_STACK_OVERFLOW,
            FAULT_OTHER,
        ];
        assert_eq!(kinds, [1, 2, 3, 4, 5, 6, 7], "numbers are the ABI's");
        assert_eq!([ACCESS_READ, ACCESS_WRITE, ACCESS_EXECUTE], [1, 2, 3]);
    }

    fn words(kind: u32, detail: u32, address: u64) -> String {
        WaitStatus::fault(kind, detail, address, 0x40_1a2c).to_string()
    }

    #[test]
    fn each_fault_reads_as_the_spec_says() {
        assert_eq!(
            words(FAULT_PAGE, ACCESS_READ, 0),
            "page fault at 0x0, read, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, ACCESS_WRITE, 0x40_1000),
            "page fault at 0x401000, write, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, ACCESS_EXECUTE, 0x40_3000),
            "page fault at 0x403000, execute, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, 9, 8),
            "page fault at 0x8, access, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_STACK_OVERFLOW, ACCESS_WRITE, 0x7fff_ffef_fff8),
            "stack overflow at 0x7fffffeffff8, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_GENERAL_PROTECTION, 0, 0),
            "general protection fault, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_INVALID_OPCODE, 0, 0),
            "invalid opcode, ip 0x401a2c"
        );
        assert_eq!(words(FAULT_DIVIDE, 0, 0), "divide error, ip 0x401a2c");
        assert_eq!(words(FAULT_FPU, 0, 0), "FPU/SSE instruction, ip 0x401a2c");
        assert_eq!(words(FAULT_OTHER, 17, 0), "CPU exception 17, ip 0x401a2c");
    }

    #[test]
    fn other_ends_read_as_exits_or_kills() {
        assert_eq!(WaitStatus::exited(3).to_string(), "exited with 3");
        assert_eq!(words(0, 0, 0), "killed");
        assert_eq!(words(8, 0, 0), "killed");
        let other_reason = WaitStatus {
            how: KILLED,
            code: 7,
            ..WaitStatus::default()
        };
        assert_eq!(other_reason.to_string(), "killed");
        assert!(!other_reason.is_known_fault());
        assert!(WaitStatus::fault(FAULT_OTHER, 3, 0, 0).is_known_fault());
    }
}
