//! x86_64-specific setup: segmentation, interrupt table, CPU control, ring
//! 3 and the context switch.

pub mod context;
pub mod cpu;
pub mod fault;
pub mod gdt;
pub mod idt;
pub mod irq;
pub mod lapic;
pub mod pic;
pub mod user;

/// The machine, as `uname` names it.
pub const MACHINE: &str = "x86_64";

/// Stops the CPU for good (interrupts stay disabled).
pub fn halt_forever() -> ! {
    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}

/// Sleeps until the next interrupt (the 1 kHz tick) when interrupts are on;
/// without them (the timer failed to start) it only pauses briefly, so
/// callers that poll keep running.
pub fn wait_for_interrupt() {
    if x86_64::instructions::interrupts::are_enabled() {
        x86_64::instructions::hlt();
    } else {
        core::hint::spin_loop();
    }
}

/// Waits for interrupts forever: the CPU sleeps between timer ticks.
pub fn idle_forever() -> ! {
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

/// `e_machine` of the programs this kernel runs.
pub const ELF_MACHINE: u16 = elf::EM_X86_64;
