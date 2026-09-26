//! relay-kernel entry point. See `lib.rs`.
#![no_std]
#![no_main]

use boot_info::BootInfo;

/// Called by relay-boot on the kernel stack with interrupts disabled.
#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(info: &'static BootInfo) -> ! {
    relay_kernel::kernel_main(info)
}

/// Replaced by the panic screen once the console exists.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
