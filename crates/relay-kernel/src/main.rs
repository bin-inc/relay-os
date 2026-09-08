#![no_std]
#![no_main]

mod entry;
mod serial;

struct NoAllocator;

unsafe impl core::alloc::GlobalAlloc for NoAllocator {
    unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

// Task 4 kernel entry is allocation-free; this satisfies transitive `alloc` linkage
// without making post-ExitBootServices allocation appear to work.
#[global_allocator]
static ALLOCATOR: NoAllocator = NoAllocator;

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
/// # Safety
/// The loader must have exited UEFI boot services and supplied an identity-mapped,
/// initialized `BootInfo` conforming to the versioned ABI.
pub unsafe extern "C" fn _start(boot_info: *const relay_abi::BootInfo) -> ! {
    // SAFETY: the loader transfers control only through the documented BootInfo ABI.
    unsafe { entry::enter(boot_info) }
}
