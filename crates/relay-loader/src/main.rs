#![cfg_attr(target_os = "uefi", no_std)]
#![cfg_attr(target_os = "uefi", no_main)]

#[cfg(target_os = "uefi")]
use uefi::prelude::*;

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[entry]
#[cfg(target_os = "uefi")]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }
    match relay_loader::handoff::boot() {
        Ok(()) => Status::SUCCESS,
        Err(error) => {
            uefi::println!("[relay] phase=uefi-setup status={error:?}");
            Status::ABORTED
        }
    }
}

#[cfg(not(target_os = "uefi"))]
fn main() {}
