//! Interrupt descriptor table. The entry stubs save all registers and call
//! `exception_dispatch`. An exception in ring 3 ends the program that
//! caused it (user-space gate §11.1); any other draws the panic screen.
//! Hardware interrupts have their own, returning stubs in `irq`.
//!
//! Entry stubs are naked functions (stable Rust), not the nightly-only
//! `x86-interrupt` ABI.

use core::arch::naked_asm;
use spin::Once;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::instructions::tables::lidt;
use x86_64::structures::DescriptorTablePointer;

/// Registers saved by the stubs, lowest address first.
#[repr(C)]
#[derive(Debug)]
pub struct ExceptionFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub const EXCEPTION_NAMES: [&str; 32] = [
    "divide error",
    "debug",
    "non-maskable interrupt",
    "breakpoint",
    "overflow",
    "bound range exceeded",
    "invalid opcode",
    "device not available",
    "double fault",
    "coprocessor segment overrun",
    "invalid TSS",
    "segment not present",
    "stack-segment fault",
    "general protection fault",
    "page fault",
    "reserved (15)",
    "x87 floating-point error",
    "alignment check",
    "machine check",
    "SIMD floating-point error",
    "virtualization exception",
    "control protection exception",
    "reserved (22)",
    "reserved (23)",
    "reserved (24)",
    "reserved (25)",
    "reserved (26)",
    "reserved (27)",
    "hypervisor injection exception",
    "VMM communication exception",
    "security exception",
    "reserved (31)",
];

pub fn exception_name(vector: u64) -> &'static str {
    EXCEPTION_NAMES
        .get(vector as usize)
        .copied()
        .unwrap_or("unknown")
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Gate {
    offset_lo: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_hi: u32,
    reserved: u32,
}

impl Gate {
    const MISSING: Gate = Gate {
        offset_lo: 0,
        selector: 0,
        ist: 0,
        type_attr: 0,
        offset_mid: 0,
        offset_hi: 0,
        reserved: 0,
    };

    /// Present, DPL 0, 64-bit interrupt gate (interrupts stay disabled).
    fn new(handler: u64, selector: u16, ist: u8) -> Gate {
        Gate {
            offset_lo: handler as u16,
            selector,
            ist,
            type_attr: 0x8E,
            offset_mid: (handler >> 16) as u16,
            offset_hi: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, align(16))]
struct Idt([Gate; 256]);

static IDT: Once<Idt> = Once::new();

macro_rules! stub {
    // The CPU pushed an error code.
    ($name:ident, $vector:literal, err) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(concat!("push ", $vector), "jmp {common}", common = sym exception_common)
        }
    };
    // No error code: push 0 so every frame has the same layout.
    ($name:ident, $vector:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!("push 0", concat!("push ", $vector), "jmp {common}", common = sym exception_common)
        }
    };
}

stub!(ex0, 0);
stub!(ex1, 1);
stub!(ex2, 2);
stub!(ex3, 3);
stub!(ex4, 4);
stub!(ex5, 5);
stub!(ex6, 6);
stub!(ex7, 7);
stub!(ex8, 8, err);
stub!(ex9, 9);
stub!(ex10, 10, err);
stub!(ex11, 11, err);
stub!(ex12, 12, err);
stub!(ex13, 13, err);
stub!(ex14, 14, err);
stub!(ex15, 15);
stub!(ex16, 16);
stub!(ex17, 17, err);
stub!(ex18, 18);
stub!(ex19, 19);
stub!(ex20, 20);
stub!(ex21, 21, err);
stub!(ex22, 22);
stub!(ex23, 23);
stub!(ex24, 24);
stub!(ex25, 25);
stub!(ex26, 26);
stub!(ex27, 27);
stub!(ex28, 28);
stub!(ex29, 29, err);
stub!(ex30, 30, err);
stub!(ex31, 31);

const STUBS: [unsafe extern "C" fn(); 32] = [
    ex0, ex1, ex2, ex3, ex4, ex5, ex6, ex7, ex8, ex9, ex10, ex11, ex12, ex13, ex14, ex15, ex16,
    ex17, ex18, ex19, ex20, ex21, ex22, ex23, ex24, ex25, ex26, ex27, ex28, ex29, ex30, ex31,
];

/// Saves the general registers (building an `ExceptionFrame` on the stack)
/// and calls the Rust dispatcher. Never returns.
#[unsafe(naked)]
unsafe extern "C" fn exception_common() {
    naked_asm!(
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",
        "cld",
        "and rsp, -16",
        "call {dispatch}",
        "ud2",
        dispatch = sym exception_dispatch,
    )
}

extern "C" fn exception_dispatch(frame: &ExceptionFrame) -> ! {
    if frame.cs & 3 == 3 {
        let cr2 = x86_64::registers::control::Cr2::read_raw();
        if let Some(f) = super::fault::classify(frame.vector, frame.error_code, cr2) {
            crate::proc::fault(f.kind, f.detail, f.address, frame.rip);
        }
    }
    crate::panic_screen::exception(frame)
}

/// Vectors that run on the IST stack whatever the stack pointer was: a
/// double fault (the kernel stack may have overflowed), an NMI and a
/// machine check (they may arrive between `syscall` and the switch to the
/// kernel stack, or just before `sysret`, with a program's stack pointer).
const IST_VECTORS: [usize; 3] = [2, 8, 18];

/// Gates for the 32 exceptions (three on the IST stack) and for every
/// hardware interrupt vector, 32-255.
fn gates(selector: u16) -> [Gate; 256] {
    let mut gates = [Gate::MISSING; 256];
    for (v, stub) in STUBS.iter().enumerate() {
        let ist = if IST_VECTORS.contains(&v) {
            super::gdt::DOUBLE_FAULT_IST
        } else {
            0
        };
        gates[v] = Gate::new(*stub as *const () as u64, selector, ist);
    }
    for v in super::irq::FIRST_VECTOR..=255 {
        gates[v as usize] = Gate::new(super::irq::stub(v), selector, 0);
    }
    gates
}

pub fn init() {
    let idt = IDT.call_once(|| Idt(gates(CS::get_reg().0)));
    let ptr = DescriptorTablePointer {
        limit: (core::mem::size_of::<Idt>() - 1) as u16,
        base: VirtAddr::new(idt as *const Idt as u64),
    };
    // SAFETY: the IDT is 'static and its gates point at valid stubs.
    unsafe { lidt(&ptr) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_splits_handler_address() {
        let g = Gate::new(0xFFFF_FFFF_8012_3456, 0x08, 1);
        assert_eq!({ g.offset_lo }, 0x3456);
        assert_eq!({ g.offset_mid }, 0x8012);
        assert_eq!({ g.offset_hi }, 0xFFFF_FFFF);
        assert_eq!(g.type_attr, 0x8E);
        assert_eq!(g.ist, 1);
        assert_eq!(core::mem::size_of::<Gate>(), 16);
    }

    #[test]
    fn frame_layout_matches_stub_pushes() {
        // 15 GPRs + vector + error code + 5 CPU-pushed words.
        assert_eq!(core::mem::size_of::<ExceptionFrame>(), 22 * 8);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, vector), 15 * 8);
        assert_eq!(core::mem::offset_of!(ExceptionFrame, rip), 17 * 8);
    }

    #[test]
    fn every_vector_has_a_gate() {
        let g = gates(0x08);
        assert!(g.iter().all(|gate| gate.type_attr == 0x8E));
        assert_eq!(g[8].ist, 1, "double fault on IST1");
        assert_eq!(g[2].ist, 1, "NMI on IST1");
        assert_eq!(g[18].ist, 1, "machine check on IST1");
        let others = (0..256).filter(|v| ![2, 8, 18].contains(v));
        assert!(others.into_iter().all(|v| g[v].ist == 0));
        let addr = |v: usize| {
            g[v].offset_lo as u64 | (g[v].offset_mid as u64) << 16 | (g[v].offset_hi as u64) << 32
        };
        assert_eq!(addr(48), super::super::irq::stub(48));
        assert_eq!(addr(255) - addr(254), 16);
    }

    #[test]
    fn names() {
        assert_eq!(exception_name(14), "page fault");
        assert_eq!(exception_name(8), "double fault");
        assert_eq!(exception_name(99), "unknown");
    }
}
