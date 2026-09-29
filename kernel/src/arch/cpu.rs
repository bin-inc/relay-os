//! What the CPU enforces (user-space gate §5.1, §5.5): SMEP and SMAP, where
//! CPUID reports them, and CR0.TS.
//!
//! - The kernel copies a program's memory through the linear map
//!   (`UserSlice`), never at the program's own addresses, and runs none of
//!   a program's code, so with SMEP and SMAP on any access to a program's
//!   page from ring 0 is a kernel bug that faults (the panic screen)
//!   instead of passing silently.
//! - The kernel is built without floating point and saves no FPU state,
//!   so CR0.TS stays set: any x87, MMX or SSE instruction (and `fwait`,
//!   with MP) raises #NM, and a program that uses one is killed instead of
//!   reading or corrupting another's registers.

use core::fmt;
use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

/// CPUID leaf 7's EBX bits.
const SMEP: u32 = 1 << 7;
const SMAP: u32 = 1 << 20;

/// What `init` turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protection {
    pub smep: bool,
    pub smap: bool,
}

impl fmt::Display for Protection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let on = |b| if b { "on" } else { "not available" };
        write!(f, "SMEP {}, SMAP {}", on(self.smep), on(self.smap))
    }
}

/// SMEP and SMAP as CPUID reports them: `max_leaf` is leaf 0's EAX, and
/// `leaf7_ebx` leaf 7's (subleaf 0) EBX.
pub fn features(max_leaf: u32, leaf7_ebx: u32) -> Protection {
    let ebx = if max_leaf >= 7 { leaf7_ebx } else { 0 };
    Protection {
        smep: ebx & SMEP != 0,
        smap: ebx & SMAP != 0,
    }
}

/// CR4 with SMEP (bit 20) and SMAP (bit 21) set as `p` says.
pub fn cr4_with(cr4: u64, p: Protection) -> u64 {
    let mut cr4 = cr4 & !(1 << 20 | 1 << 21);
    if p.smep {
        cr4 |= 1 << 20;
    }
    if p.smap {
        cr4 |= 1 << 21;
    }
    cr4
}

/// CR0 with TS (bit 3) and MP (bit 1) set and EM (bit 2) clear: FPU and
/// SSE instructions raise #NM, not #UD.
pub fn cr0_with(cr0: u64) -> u64 {
    (cr0 | 1 << 3 | 1 << 1) & !(1 << 2)
}

/// Turns SMEP and SMAP on where the CPU has them, and sets CR0.TS.
pub fn init() -> Protection {
    // SAFETY: the kernel runs no FPU or SSE instruction.
    unsafe { Cr0::write(Cr0Flags::from_bits_retain(cr0_with(Cr0::read_raw()))) };
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    let max = __cpuid(0).eax;
    let ebx = if max >= 7 { __cpuid_count(7, 0).ebx } else { 0 };
    let p = features(max, ebx);
    let cr4 = cr4_with(Cr4::read_raw(), p);
    // SAFETY: only bits CPUID reports are set; the kernel touches no
    // program's page.
    unsafe { Cr4::write(Cr4Flags::from_bits_retain(cr4)) };
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smep_and_smap_are_leaf_7_s_ebx_bits_7_and_20() {
        let both = features(0x20, SMEP | SMAP | 1);
        assert_eq!(
            both,
            Protection {
                smep: true,
                smap: true
            }
        );
        assert_eq!(
            features(0x20, SMEP),
            Protection {
                smep: true,
                smap: false
            }
        );
        assert_eq!(
            features(0x20, SMAP),
            Protection {
                smep: false,
                smap: true
            }
        );
        assert_eq!(
            features(6, SMEP | SMAP),
            Protection {
                smep: false,
                smap: false
            },
            "no leaf 7"
        );
        assert_eq!(both.to_string(), "SMEP on, SMAP on");
        assert_eq!(
            features(0, 0).to_string(),
            "SMEP not available, SMAP not available"
        );
    }

    #[test]
    fn cr0_makes_fpu_and_sse_instructions_fault_with_nm() {
        // PE, ET, NE, WP, PG as a UEFI loader leaves them, with EM set.
        let firmware = 0x8005_0031 | 1 << 2;
        assert_eq!(cr0_with(firmware), 0x8005_0031 | 1 << 3 | 1 << 1);
        assert_eq!(cr0_with(0), 0b1010);
    }

    #[test]
    fn cr4_gets_bits_20_and_21_and_keeps_the_rest() {
        let p = Protection {
            smep: true,
            smap: true,
        };
        assert_eq!(cr4_with(0x6F0, p), 0x6F0 | 3 << 20);
        let none = Protection {
            smep: false,
            smap: false,
        };
        assert_eq!(cr4_with(0x6F0 | 3 << 20, none), 0x6F0);
        let smep = Protection {
            smep: true,
            smap: false,
        };
        assert_eq!(cr4_with(0, smep), 1 << 20);
    }
}
