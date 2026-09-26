//! x86_64-specific setup: segmentation, interrupt table, CPU control.

pub mod gdt;
pub mod idt;
pub mod irq;
pub mod lapic;
pub mod pic;

/// Stops the CPU for good (interrupts stay disabled).
pub fn halt_forever() -> ! {
    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
