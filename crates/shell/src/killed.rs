//! What the shell says about a program that did not exit by itself (spec
//! §11.1 of the user-space gate): `killed (page fault at 0x0, read, ip
//! 0x401a2c)`, and the status bash would give for the signal Linux would
//! send (128 + 11 for SIGSEGV, + 4 for SIGILL, + 8 for SIGFPE).

use alloc::format;
use alloc::string::String;
use relay_abi::WaitStatus;
use relay_abi::wait::*;

/// A program killed with no reason the shell knows (bash's SIGKILL).
pub const KILLED: i32 = 128 + 9;

/// The words for a program that did not exit, and its exit status.
pub fn killed(w: &WaitStatus) -> (String, i32) {
    if !w.is_known_fault() {
        return (String::from("killed"), KILLED);
    }
    let signal = match w.fault {
        FAULT_INVALID_OPCODE => 4,
        FAULT_DIVIDE | FAULT_FPU => 8,
        _ => 11,
    };
    (format!("killed ({w})"), 128 + signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fault(kind: u32) -> (String, i32) {
        killed(&WaitStatus::fault(kind, ACCESS_READ, 0, 0x40_1a2c))
    }

    #[test]
    fn each_fault_gets_bash_s_status_for_its_signal() {
        assert_eq!(
            fault(FAULT_PAGE),
            ("killed (page fault at 0x0, read, ip 0x401a2c)".into(), 139)
        );
        assert_eq!(fault(FAULT_STACK_OVERFLOW).1, 139);
        assert_eq!(fault(FAULT_GENERAL_PROTECTION).1, 139);
        assert_eq!(fault(FAULT_OTHER).1, 139);
        assert_eq!(fault(FAULT_INVALID_OPCODE).1, 132);
        assert_eq!(fault(FAULT_DIVIDE).1, 136);
        assert_eq!(
            fault(FAULT_FPU),
            ("killed (FPU/SSE instruction, ip 0x401a2c)".into(), 136)
        );
    }

    #[test]
    fn what_the_shell_does_not_know_is_just_killed() {
        assert_eq!(fault(99), ("killed".into(), 137));
        let w = WaitStatus {
            how: relay_abi::wait::KILLED,
            code: 7,
            ..WaitStatus::default()
        };
        assert_eq!(killed(&w), ("killed".into(), 137));
    }
}
