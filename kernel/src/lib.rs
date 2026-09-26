//! Relay OS kernel. The binary (`main.rs`) only provides the entry point and
//! panic handler; everything else lives here so the pure parts can be unit
//! tested on the host with `cargo test -p relay-kernel --lib`.
#![cfg_attr(not(test), no_std)]

pub mod arch;
pub mod cmdline;
pub mod console;
pub mod klog;
pub mod panic_screen;
pub mod serial;

use boot_info::{BootInfo, MemoryKind, PHYS_OFFSET};
use cmdline::{Cmdline, PanicTest};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn kernel_main(info: &'static BootInfo) -> ! {
    if !info.is_valid() {
        serial::init();
        serial::write(b"relay: BootInfo magic/version mismatch; halting\n");
        arch::halt_forever();
    }
    // Boot-progress squares 9-10 (1-8 are the loader's); console::init
    // clears them once text output works.
    let fb = info.framebuffer;
    let mark = |stage| unsafe { fb.mark_stage(PHYS_OFFSET + fb.phys_addr, stage) };
    mark(9);
    serial::init();
    mark(10);
    console::init(&info.framebuffer);
    kprintln!("Relay OS {VERSION}");
    let (cols, rows) = console::size().unwrap_or((0, 0));
    console::ok(format_args!(
        "console {}x{} ({cols}x{rows} cells)",
        fb.width, fb.height
    ));

    arch::gdt::init();
    arch::idt::init();
    console::ok(format_args!("cpu tables"));

    // SAFETY: built by relay-boot, lives forever.
    let map = unsafe { info.memory_map() };
    let usable: u64 = map
        .iter()
        .filter(|r| r.kind == MemoryKind::Usable)
        .map(|r| r.len)
        .sum();
    let cmdline = Cmdline::parse(info.cmdline());
    console::ok(format_args!(
        "boot info: {} MiB usable in {} regions, cmdline '{}'",
        usable >> 20,
        map.len(),
        info.cmdline()
    ));

    if let Some(t) = cmdline.panic_test {
        trigger(t);
    }
    kprintln!("relay: early boot complete");
    arch::halt_forever()
}

/// Deliberate crashes for the panic-screen tests.
fn trigger(t: PanicTest) {
    kprintln!("relay: triggering {t:?} as requested");
    match t {
        PanicTest::PageFault => unsafe {
            core::ptr::read_volatile(0x0000_7FFF_DEAD_0000 as *const u64);
        },
        PanicTest::InvalidOpcode => unsafe { core::arch::asm!("ud2") },
        PanicTest::Panic => panic!("test panic requested on the command line"),
        PanicTest::StackOverflow => {
            recurse(0);
        }
    }
}

#[inline(never)]
#[allow(unconditional_recursion)]
fn recurse(depth: u64) -> u64 {
    let pad = core::hint::black_box([depth; 64]);
    recurse(depth + 1) + pad[0]
}
