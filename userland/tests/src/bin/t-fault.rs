//! `t-fault KIND`: does something the CPU refuses, on purpose (spec §8.5),
//! so a scenario sees the kernel end the program, not itself:
//! `null-read`, `null-write`, `write-code`, `exec-data`, `ud`, `div0`,
//! `stack` (runs into the guard page below the stack) and `kernel-read`
//! (reads an upper-half address). Plan 3 adds `sse`, once CR0.TS makes
//! SSE fault.
#![no_std]
#![no_main]

use core::arch::asm;
use relay_rt::Args;
use relay_rt::sys;

relay_rt::main!(main);

/// Somewhere writable and not executable to jump to: `ret`, in data.
static mut DATA: [u8; 16] = [0xC3; 16];

fn main(args: Args) -> u8 {
    let kind = args.get(1).unwrap_or(b"");
    // SAFETY: none. Each of these faults, which is the point.
    unsafe {
        match kind {
            b"null-read" => asm!("mov {0}, qword ptr [{0}]", inout(reg) 0u64 => _),
            b"null-write" => asm!("mov qword ptr [{0}], 1", in(reg) 0u64),
            b"write-code" => {
                asm!("mov byte ptr [{0}], 0x90", in(reg) main as *const () as u64)
            }
            b"exec-data" => asm!("call {0}", in(reg) &raw const DATA as u64),
            b"ud" => asm!("ud2"),
            b"div0" => asm!(
                "div {0:e}",
                in(reg) 0u32,
                inout("eax") 1u32 => _,
                inout("edx") 0u32 => _,
            ),
            b"stack" => asm!("2:", "push rax", "jmp 2b", options(noreturn)),
            b"kernel-read" => {
                asm!("mov {0}, qword ptr [{0}]", inout(reg) 0xFFFF_8000_0000_0000u64 => _)
            }
            _ => {
                let _ = sys::write_all(
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read\n",
                );
                return 2;
            }
        }
    }
    let _ = sys::write_all(2, b"t-fault: no fault\n");
    1
}
