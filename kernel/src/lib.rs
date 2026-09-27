//! Relay OS kernel. The binary (`main.rs`) only provides the entry point and
//! panic handler; everything else lives here so the pure parts can be unit
//! tested on the host with `cargo test -p relay-kernel --lib`.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod acpi;
pub mod arch;
pub mod cmdline;
pub mod console;
pub mod input;
pub mod klog;
pub mod mm;
pub mod panic_screen;
pub mod pci;
pub mod rtc;
pub mod serial;
pub mod timer;

use boot_info::{BootInfo, MemoryKind, PHYS_OFFSET};
use cmdline::{Cmdline, PanicTest};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn kernel_main(info: &'static BootInfo) -> ! {
    if !info.is_valid() {
        serial::init();
        serial::write(b"relay: BootInfo magic/version mismatch; halting\n");
        arch::halt_forever();
    }
    // Boot-progress squares 10-11 (1-9 are the loader's); console::init
    // clears them once text output works.
    let fb = info.framebuffer;
    let mark = |stage| unsafe { fb.mark_stage(PHYS_OFFSET + fb.phys_addr, stage) };
    mark(10);
    // Our own GDT and IDT come first: until they are loaded the CPU uses the
    // firmware's, and a fault would triple-fault without a word. From here
    // on the panic screen starts the console itself if it has to.
    console::set_framebuffer(&info.framebuffer);
    arch::gdt::init();
    arch::idt::init();
    let cmdline = Cmdline::parse(info.cmdline());
    serial::init();
    if cmdline.panic_test == Some(PanicTest::Early) {
        trigger(PanicTest::Early);
    }
    mark(11);
    console::init();
    kprintln!("Relay OS {VERSION}");
    let (cols, rows) = console::size().unwrap_or((0, 0));
    console::ok(format_args!(
        "console {}x{} ({cols}x{rows} cells)",
        fb.width, fb.height
    ));
    console::ok(format_args!("cpu tables"));

    // SAFETY: built by relay-boot, lives forever.
    let map = unsafe { info.memory_map() };
    let usable: u64 = map
        .iter()
        .filter(|r| r.kind == MemoryKind::Usable)
        .map(|r| r.len)
        .sum();
    console::ok(format_args!(
        "boot info: {} MiB usable in {} regions, cmdline '{}'",
        usable >> 20,
        map.len(),
        info.cmdline()
    ));

    match mm::init(info) {
        Ok(s) => console::ok(format_args!(
            "memory: {} MiB free of {} MiB, heap {} MiB",
            (s.free_frames * mm::frame::FRAME_SIZE) >> 20,
            (s.total_frames * mm::frame::FRAME_SIZE) >> 20,
            s.heap.total >> 20
        )),
        Err(e) => {
            // Nothing later can work without memory.
            console::fail("memory", format_args!("{e}"));
            arch::halt_forever();
        }
    }

    let acpi = match acpi::init(info.rsdp_addr) {
        Ok(a) => {
            console::ok(format_args!("acpi: {a}"));
            Some(a)
        }
        Err(e) => {
            console::fail("acpi", format_args!("{e}"));
            None
        }
    };

    match timer::init(acpi.and_then(|a| a.hpet), cmdline.tsc_hpet) {
        Ok(t) => console::ok(format_args!("timer: {t}")),
        Err(e) => console::fail("timer", format_args!("{e}")),
    }

    let century = acpi.and_then(|a| a.fadt).map_or(0, |f| f.century);
    match rtc::init(century) {
        Ok(t) => console::ok(format_args!("rtc: {t}")),
        Err(raw) => console::fail("rtc", format_args!("invalid clock registers {raw:?}")),
    }
    if cmdline.check_timer {
        check_timer();
    }

    let ecam = acpi.map_or(&[][..], |a| a.ecam.as_slice());
    match pci::init(ecam) {
        Ok(devices) => {
            // The xHCI controllers matter for USB (plan 4) and on a photo
            // of the NUC's screen; the rest goes to the kernel log.
            for d in devices {
                if d.is_xhci() {
                    kprintln!("pci: {d}");
                } else {
                    klogln!("pci: {d}");
                }
            }
            console::ok(format_args!("pci: {}", pci::Summary(devices)));
        }
        Err(e) => console::fail("pci", format_args!("{e}")),
    }

    if let Some(t) = cmdline.panic_test {
        trigger(t);
    }
    kprintln!("relay: early boot complete");
    arch::idle_forever()
}

/// `check=timer`: the 1 kHz tick measured against the RTC's seconds.
fn check_timer() {
    const SECONDS: u64 = 3;
    let expected = SECONDS * timer::TICK_HZ;
    match timer::count_ticks_over(SECONDS, rtc::second) {
        Some(n) if timer::tick_check_ok(n, SECONDS) => {
            kprintln!("timer check: ok, {n} ticks in {SECONDS} RTC seconds")
        }
        Some(n) => kprintln!(
            "timer check: FAILED, {n} ticks in {SECONDS} RTC seconds (expected {expected} +/- 10%)"
        ),
        None => kprintln!("timer check: FAILED, the RTC seconds do not change"),
    }
}

/// Deliberate crashes for the panic-screen tests.
fn trigger(t: PanicTest) {
    kprintln!("relay: triggering {t:?} as requested");
    match t {
        PanicTest::Early | PanicTest::PageFault => unsafe {
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
