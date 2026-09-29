//! Hardware interrupt entry. Unlike exceptions, interrupts return: the stub
//! saves every general register, calls `dispatch` and restores them before
//! `iretq`. The kernel is built without SSE and without a red zone, so no
//! other state needs saving and handlers can run on the current stack.
//!
//! Vectors: 32-47 the (masked) legacy PIC, 48 the LAPIC timer, 255 the LAPIC
//! spurious vector. Every other vector from 32 up has a gate too: an
//! interrupt the firmware left pending costs a count and an EOI instead of a
//! fault, and one that keeps firing (a source nobody masked) panics as an
//! interrupt storm, naming its vector.

use super::idt::ExceptionFrame;
use core::arch::naked_asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The first vector after the CPU exceptions.
pub const FIRST_VECTOR: u8 = 32;
pub const PIC_BASE: u8 = 32;
pub const TIMER_VECTOR: u8 = 48;
pub const SPURIOUS_VECTOR: u8 = 255;
/// An unexpected vector that fires this often has a live source.
pub const STORM_LIMIT: u32 = 1000;
/// Bytes per entry stub; each starts on a 16-byte boundary.
const STUB_SIZE: u64 = 16;

/// What an interrupt vector means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Timer,
    /// A legacy PIC line. Every line is masked, so this is a spurious IRQ 7
    /// or 15. A spurious IRQ 15 still needs an EOI to the master PIC
    /// (which really did take the cascade interrupt); IRQ 7 needs none.
    Legacy {
        irq: u8,
        eoi_master: bool,
    },
    /// The LAPIC's spurious vector: no EOI.
    Spurious,
    Unexpected,
}

pub fn classify(vector: u8) -> Action {
    match vector {
        v if (PIC_BASE..PIC_BASE + 16).contains(&v) => {
            let irq = v - PIC_BASE;
            Action::Legacy {
                irq,
                eoi_master: irq == 15,
            }
        }
        TIMER_VECTOR => Action::Timer,
        SPURIOUS_VECTOR => Action::Spurious,
        _ => Action::Unexpected,
    }
}

/// Timer interrupts since the timer started.
pub static TICKS: AtomicU64 = AtomicU64::new(0);
/// Spurious and legacy-PIC interrupts seen.
pub static SPURIOUS: AtomicU64 = AtomicU64::new(0);
/// Interrupts per vector that nothing handles.
static UNEXPECTED: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

/// Counts an interrupt on a vector nothing handles; true once that vector
/// has fired `STORM_LIMIT` times.
pub fn note_unexpected(counts: &[AtomicU32; 256], vector: u8) -> bool {
    counts[vector as usize].fetch_add(1, Ordering::Relaxed) + 1 >= STORM_LIMIT
}

extern "C" fn dispatch(frame: &ExceptionFrame) {
    debug_assert!(
        super::user::gs_is_kernel(),
        "an interrupt with the program's gs"
    );
    debug_assert!(
        super::user::flags_are_kernel(),
        "an interrupt with the program's flags"
    );
    match classify(frame.vector as u8) {
        Action::Timer => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            super::lapic::eoi();
            // After the EOI: the tick may switch to another process.
            if frame.cs & 3 == 3 {
                crate::proc::user_tick();
            } else {
                crate::proc::kernel_tick();
            }
        }
        Action::Legacy { eoi_master, .. } => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
            if eoi_master {
                super::pic::eoi_master(&mut super::pic::RealPorts);
            }
        }
        Action::Spurious => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
        }
        Action::Unexpected => {
            let vector = frame.vector as u8;
            if note_unexpected(&UNEXPECTED, vector) {
                panic!(
                    "interrupt storm: vector {vector} fired {STORM_LIMIT} times and nothing handles it"
                );
            }
            // Only LAPIC-delivered sources can raise these vectors.
            super::lapic::eoi();
        }
    }
}

/// One entry stub per vector: aligned to 16 bytes, it pushes a zero error
/// code and the vector, then jumps to `irq_common`.
macro_rules! stubs {
    ($($vector:literal)*) => {
        concat!($(".balign 16\n", "push 0\n", "push ", $vector, "\n", "jmp {common}\n",)*)
    };
}

/// The entry stubs for vectors 32-255, one every 16 bytes.
#[unsafe(naked)]
unsafe extern "C" fn irq_stubs() {
    naked_asm!(
        stubs!(
        32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47
        48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63
        64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79
        80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95
        96 97 98 99 100 101 102 103 104 105 106 107 108 109 110 111
        112 113 114 115 116 117 118 119 120 121 122 123 124 125 126 127
        128 129 130 131 132 133 134 135 136 137 138 139 140 141 142 143
        144 145 146 147 148 149 150 151 152 153 154 155 156 157 158 159
        160 161 162 163 164 165 166 167 168 169 170 171 172 173 174 175
        176 177 178 179 180 181 182 183 184 185 186 187 188 189 190 191
        192 193 194 195 196 197 198 199 200 201 202 203 204 205 206 207
        208 209 210 211 212 213 214 215 216 217 218 219 220 221 222 223
        224 225 226 227 228 229 230 231 232 233 234 235 236 237 238 239
        240 241 242 243 244 245 246 247 248 249 250 251 252 253 254 255
        ),
        common = sym irq_common,
    )
}

/// Address of the entry stub for `vector` (32-255).
pub fn stub(vector: u8) -> u64 {
    assert!(vector >= FIRST_VECTOR);
    let first = (irq_stubs as *const () as u64).next_multiple_of(STUB_SIZE);
    first + (vector - FIRST_VECTOR) as u64 * STUB_SIZE
}

/// Saves the registers in `ExceptionFrame` layout (the stub pushed a zero
/// error code and the vector), calls `dispatch` on a 16-byte aligned stack,
/// restores everything and returns from the interrupt. An interrupt of
/// ring 3 swaps to the kernel's `gs` and loads the kernel's flags first,
/// and swaps back last (see `user`); `iretq` gives the program its flags
/// back.
#[unsafe(naked)]
unsafe extern "C" fn irq_common() {
    naked_asm!(
        // The interrupted CS, above the vector, the error code and RIP.
        "test qword ptr [rsp + 24], 3",
        "jz 2f",
        "swapgs",
        // The kernel's flags (bit 1 only): nothing the program set, AC
        // above all (it would switch SMAP off), reaches the handler.
        "push 2",
        "popfq",
        "2:",
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",
        "mov rbp, rsp",
        "and rsp, -16",
        "cld",
        "call {dispatch}",
        "mov rsp, rbp",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rbp", "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rbx", "pop rax",
        "add rsp, 16",
        "test qword ptr [rsp + 8], 3",
        "jz 3f",
        "swapgs",
        "3:",
        "iretq",
        dispatch = sym dispatch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_are_classified() {
        assert_eq!(classify(48), Action::Timer);
        assert_eq!(classify(255), Action::Spurious);
        assert_eq!(
            classify(39),
            Action::Legacy {
                irq: 7,
                eoi_master: false
            }
        );
        assert_eq!(
            classify(47),
            Action::Legacy {
                irq: 15,
                eoi_master: true
            }
        );
        assert_eq!(
            classify(32),
            Action::Legacy {
                irq: 0,
                eoi_master: false
            }
        );
        assert_eq!(classify(49), Action::Unexpected);
        assert_eq!(classify(14), Action::Unexpected);
    }

    #[test]
    fn each_stub_pushes_zero_and_its_own_vector() {
        for vector in FIRST_VECTOR..=255 {
            // SAFETY: reads the kernel's own code.
            let code = unsafe { *(stub(vector) as *const [u8; 7]) };
            assert_eq!(code[..2], [0x6A, 0x00], "push 0 (vector {vector})");
            if vector < 0x80 {
                assert_eq!(code[2..4], [0x6A, vector], "push imm8");
            } else {
                // push imm32: an imm8 would be sign-extended.
                assert_eq!(code[2], 0x68, "push imm32 (vector {vector})");
                assert_eq!(code[3..7], (vector as u32).to_le_bytes());
            }
        }
    }

    #[test]
    fn an_unexpected_vector_is_a_storm_at_the_limit() {
        let counts = [const { AtomicU32::new(0) }; 256];
        for _ in 1..STORM_LIMIT {
            assert!(!note_unexpected(&counts, 100));
        }
        assert!(note_unexpected(&counts, 100));
        assert!(!note_unexpected(&counts, 101), "counted per vector");
    }

    /// `test qword ptr [rsp + 24], 3; jz +6; swapgs; push 2; popfq`: from
    /// ring 3, to the kernel's `gs` and flags.
    const FROM_RING_3: [u8; 17] = [
        0x48, 0xF7, 0x44, 0x24, 0x18, 0x03, 0x00, 0x00, 0x00, 0x74, 0x06, 0x0F, 0x01, 0xF8, 0x6A,
        0x02, 0x9D,
    ];

    #[test]
    fn an_interrupt_of_ring_3_gets_the_kernel_s_gs_and_flags() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(irq_common as *const [u8; 128]) };
        assert_eq!(code[..17], FROM_RING_3, "first thing");
        // Before `iretq`: `test qword ptr [rsp + 8], 3; jz +3; swapgs`.
        let back = [
            0x48, 0xF7, 0x44, 0x24, 0x08, 0x03, 0x00, 0x00, 0x00, 0x74, 0x03, 0x0F, 0x01, 0xF8,
            0x48, 0xCF,
        ];
        assert!(code.windows(back.len()).any(|w| w == back), "last thing");
    }
}
