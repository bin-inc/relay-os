//! The context switch on x86_64 (user-space gate §5.4, §6.1). A process
//! that is not running is a kernel stack whose top holds everything needed
//! to go on: the kernel code that gave up the CPU pushed its callee-saved
//! registers and flags there, and the process table keeps only the stack
//! pointer. `switch` saves the current context that way and continues
//! another; a new stack gets a first frame that looks as if it had given
//! up the CPU just before calling its function.
//!
//! Every switch happens in the kernel, with interrupts off and no lock
//! held; `switch_to` first points the CPU at the next process's page
//! tables and kernel stack (TSS `rsp0`, and the per-CPU block's stack for
//! `syscall`).

use core::arch::naked_asm;
use x86_64::PhysAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::PhysFrame;

/// Words in a first frame (and in what `switch` pushes, with the return
/// address).
pub const FRAME_WORDS: usize = 8;

/// The words a new kernel stack starts with, lowest address first, so the
/// first `switch` to it (with the stack pointer at the first word) runs
/// `f(arg)` with interrupts on: the flags (interrupts off until `start`),
/// `r15` to `rbp` as `switch` pops them (`r12` holding `f`, `r13` `arg`),
/// and `start` as the return address. The frame fills the top 64 bytes of
/// a 16-byte aligned stack, so `f` is called with the stack aligned as the
/// ABI wants.
pub fn first_frame(f: extern "C" fn(u64) -> !, arg: u64) -> [u64; FRAME_WORDS] {
    [
        0x2,
        0,
        0,
        arg,
        f as *const () as u64,
        0,
        0,
        start as *const () as u64,
    ]
}

/// Where a new stack's first `switch` returns to.
#[unsafe(naked)]
unsafe extern "C" fn start() -> ! {
    naked_asm!("mov rdi, r13", "sti", "call r12", "ud2")
}

/// Saves the callee-saved registers and the flags on the current stack and
/// the stack pointer in `*save`, then continues the context saved at
/// `next`. Returns when something switches back to `*save`.
///
/// # Safety
/// Interrupts off; `next` is a stack pointer a `switch` saved or a first
/// frame's, on a mapped stack.
#[unsafe(naked)]
unsafe extern "C" fn switch(save: *mut u64, next: u64) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "pushfq",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "popfq",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

/// What the next process runs on: its page tables and the top of its
/// kernel stack.
pub struct Next {
    /// The stack pointer `switch` saved, or its first frame's.
    pub rsp: u64,
    /// Its PML4 (the kernel's own for a process without a program).
    pub pml4: u64,
    pub stack_top: u64,
}

/// Saves the running context in `*save` and continues `next`, with the CPU
/// pointed at its page tables and kernel stack. Returns when something
/// switches back.
///
/// # Safety
/// As `switch`; `next.pml4` maps the kernel as the current tables do.
pub unsafe fn switch_to(save: *mut u64, next: &Next) {
    debug_assert!(
        !x86_64::instructions::interrupts::are_enabled(),
        "a switch with interrupts on"
    );
    debug_assert!(
        super::user::flags_are_kernel(),
        "a program's flags reached a switch"
    );
    super::user::set_kernel_stack(next.stack_top);
    debug_assert!(
        super::user::kernel_stacks_agree(),
        "TSS rsp0 and the syscall stack differ"
    );
    let (current, _) = Cr3::read_raw();
    if current.start_address().as_u64() != next.pml4 {
        // SAFETY: the tables map the kernel as the current ones do.
        unsafe {
            Cr3::write(
                PhysFrame::containing_address(PhysAddr::new(next.pml4)),
                Cr3Flags::empty(),
            )
        };
    }
    // SAFETY: the caller's.
    unsafe { switch(save, next.rsp) };
    debug_assert!(
        super::user::flags_are_kernel(),
        "a program's flags came back with a switch"
    );
}

/// Points CR3 at the kernel's own tables (before a program's are freed).
pub fn use_kernel_tables(pml4: u64) {
    // SAFETY: the kernel's tables map everything the kernel uses.
    unsafe {
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(pml4)),
            Cr3Flags::empty(),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn f(_: u64) -> ! {
        unreachable!()
    }

    #[test]
    fn a_first_frame_is_what_switch_pops() {
        let w = first_frame(f, 42);
        assert_eq!(w.len() * 8, 64, "the stack's top 64 bytes");
        assert_eq!(w[0], 0x2, "popfq: interrupts off, bit 1");
        // pop r15, r14, r13, r12, rbx, rbp.
        assert_eq!(w[3], 42, "r13: the argument");
        assert_eq!(w[4], f as *const () as u64, "r12: the function");
        assert_eq!([w[1], w[2], w[5], w[6]], [0; 4]);
        assert_eq!(w[7], start as *const () as u64, "ret");
    }

    #[test]
    fn a_switch_pushes_what_a_first_frame_holds() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(switch as *const [u8; 11]) };
        // push rbp, rbx, r12, r13, r14, r15; pushfq: the first frame's
        // words from the top down.
        assert_eq!(
            code,
            [
                0x55, 0x53, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x9C
            ]
        );
    }
}
