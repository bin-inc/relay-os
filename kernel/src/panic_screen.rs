//! The red panic screen, shown for Rust panics and CPU exceptions. It does
//! not allocate, unlocks the console forcibly, and halts.

use crate::arch::idt::{ExceptionFrame, exception_name};
use crate::{arch, console, klog, kprint, kprintln};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};

static PANICKING: AtomicBool = AtomicBool::new(false);
const TAIL_LINES: usize = 20;

/// Common start: stop recursion, free the console, keep the log tail.
/// Returns the tail bytes captured before the screen is cleared.
fn begin(tail: &mut [u8]) -> usize {
    x86_64::instructions::interrupts::disable();
    if PANICKING.swap(true, Ordering::SeqCst) {
        // Panicked while drawing the panic screen: serial only, then stop.
        unsafe { console::force_unlock() };
        crate::serial::write(b"\n*** nested panic ***\n");
        arch::halt_forever();
    }
    unsafe { console::force_unlock() };
    let n = klog::KLOG.lock().tail_lines(TAIL_LINES, tail);
    kprint!("\x1b[0m\x1b[97;41m\x1b[2J\x1b[H");
    kprintln!("*** KERNEL PANIC ***");
    kprintln!();
    n
}

fn finish(tail: &mut [u8]) -> ! {
    let n = klog::strip_ansi_in_place(tail);
    kprintln!();
    kprintln!("--- last kernel log lines ---");
    console::write_bytes(&tail[..n]);
    kprintln!();
    kprintln!("System halted.");
    arch::halt_forever()
}

pub fn panic(info: &PanicInfo) -> ! {
    let mut tail = [0u8; 4096];
    let n = begin(&mut tail);
    kprintln!("{}", info.message());
    if let Some(loc) = info.location() {
        kprintln!("at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    finish(&mut tail[..n])
}

pub fn exception(f: &ExceptionFrame) -> ! {
    let mut tail = [0u8; 4096];
    let n = begin(&mut tail);
    let cr2 = x86_64::registers::control::Cr2::read_raw();
    let cr3 = x86_64::registers::control::Cr3::read_raw()
        .0
        .start_address()
        .as_u64();
    kprintln!(
        "CPU exception {}: {} (error code {:#x})",
        f.vector,
        exception_name(f.vector),
        f.error_code
    );
    kprintln!(
        "RIP={:#018x} CS={:#06x} RFLAGS={:#018x}",
        f.rip,
        f.cs,
        f.rflags
    );
    kprintln!("RSP={:#018x} SS={:#06x}", f.rsp, f.ss);
    kprintln!(
        "RAX={:#018x} RBX={:#018x} RCX={:#018x} RDX={:#018x}",
        f.rax,
        f.rbx,
        f.rcx,
        f.rdx
    );
    kprintln!(
        "RSI={:#018x} RDI={:#018x} RBP={:#018x}",
        f.rsi,
        f.rdi,
        f.rbp
    );
    kprintln!(
        "R8 ={:#018x} R9 ={:#018x} R10={:#018x} R11={:#018x}",
        f.r8,
        f.r9,
        f.r10,
        f.r11
    );
    kprintln!(
        "R12={:#018x} R13={:#018x} R14={:#018x} R15={:#018x}",
        f.r12,
        f.r13,
        f.r14,
        f.r15
    );
    kprintln!("CR2={cr2:#018x} CR3={cr3:#018x}");
    finish(&mut tail[..n])
}
