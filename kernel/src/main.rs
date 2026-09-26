//! relay-kernel entry point. See `lib.rs`.
#![no_std]
#![no_main]

use boot_info::BootInfo;

/// Called by relay-boot on the kernel stack with interrupts disabled.
#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(info: &'static BootInfo) -> ! {
    relay_kernel::kernel_main(info)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    relay_kernel::panic_screen::panic(info)
}
