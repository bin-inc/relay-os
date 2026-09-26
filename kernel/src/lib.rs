//! Relay OS kernel. The binary (`main.rs`) only provides the entry point and
//! panic handler; everything else lives here so the pure parts can be unit
//! tested on the host with `cargo test -p relay-kernel --lib`.
#![cfg_attr(not(test), no_std)]

pub mod cmdline;
pub mod klog;
pub mod serial;

use boot_info::BootInfo;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// First version: prove the hand-off works by writing to COM1, then stop.
/// Replaced by the full startup sequence once the console exists.
pub fn kernel_main(info: &'static BootInfo) -> ! {
    serial::init();
    if info.is_valid() {
        serial::write(b"relay-kernel: entered\n");
    } else {
        serial::write(b"relay: BootInfo magic/version mismatch; halting\n");
    }
    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
