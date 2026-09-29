//! What an x86_64 exception in ring 3 means for the program (user-space
//! gate §11.1): the fault's kind, its detail and address in the ABI's
//! architecture-neutral words. NMIs, double faults and machine checks are
//! the machine's trouble, not the program's, and still reach the panic
//! screen.

use relay_abi::wait::*;

/// A page fault's error code: the access was a write, an instruction
/// fetch.
const PF_WRITE: u64 = 1 << 1;
const PF_FETCH: u64 = 1 << 4;

/// A fault's kind, detail and address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    pub kind: u32,
    pub detail: u32,
    pub address: u64,
}

/// The fault exception `vector` (with its `error_code`, and `cr2` for a
/// page fault) is, if it is the program's.
pub fn classify(vector: u64, error_code: u64, cr2: u64) -> Option<Fault> {
    let fault = |kind, detail, address| {
        Some(Fault {
            kind,
            detail,
            address,
        })
    };
    match vector {
        0 => fault(FAULT_DIVIDE, 0, 0),
        6 => fault(FAULT_INVALID_OPCODE, 0, 0),
        7 => fault(FAULT_FPU, 0, 0),
        // A stack-segment fault in 64-bit mode is a non-canonical stack
        // address: as a general protection fault would be.
        12 | 13 => fault(FAULT_GENERAL_PROTECTION, 0, 0),
        14 => {
            let access = if error_code & PF_FETCH != 0 {
                ACCESS_EXECUTE
            } else if error_code & PF_WRITE != 0 {
                ACCESS_WRITE
            } else {
                ACCESS_READ
            };
            fault(FAULT_PAGE, access, cr2)
        }
        2 | 8 | 18 => None,
        v => fault(FAULT_OTHER, v as u32, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_faults_say_which_access_at_which_address() {
        // Error codes as the CPU gives them: present, write, user, fetch.
        let pf = |code| classify(14, code, 0x1234).unwrap();
        assert_eq!(
            pf(0b0_0100),
            Fault {
                kind: FAULT_PAGE,
                detail: ACCESS_READ,
                address: 0x1234
            }
        );
        assert_eq!(pf(0b0_0110).detail, ACCESS_WRITE);
        assert_eq!(pf(0b0_0111).detail, ACCESS_WRITE, "to a read-only page");
        assert_eq!(
            pf(0b1_0101).detail,
            ACCESS_EXECUTE,
            "from a no-execute page"
        );
        assert_eq!(pf(0b0_0101).detail, ACCESS_READ, "a kernel page");
    }

    #[test]
    fn each_exception_has_its_kind() {
        let kind = |v| classify(v, 0, 0x99).map(|f| f.kind);
        assert_eq!(kind(0), Some(FAULT_DIVIDE));
        assert_eq!(kind(6), Some(FAULT_INVALID_OPCODE));
        assert_eq!(kind(7), Some(FAULT_FPU));
        assert_eq!(kind(13), Some(FAULT_GENERAL_PROTECTION));
        assert_eq!(kind(12), Some(FAULT_GENERAL_PROTECTION));
        assert_eq!(
            classify(13, 0x18, 0x99).unwrap().address,
            0,
            "cr2 is a page fault's"
        );
        assert_eq!(
            classify(17, 0, 0),
            Some(Fault {
                kind: FAULT_OTHER,
                detail: 17,
                address: 0
            }),
            "alignment check"
        );
        assert_eq!(classify(3, 0, 0).unwrap().detail, 3);
    }

    #[test]
    fn nmis_double_faults_and_machine_checks_are_not_the_program_s() {
        for v in [2, 8, 18] {
            assert_eq!(classify(v, 0, 0), None, "vector {v}");
        }
    }
}
