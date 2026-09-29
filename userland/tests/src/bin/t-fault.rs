//! `t-fault KIND`: does something the CPU refuses, on purpose (spec §8.5),
//! so a scenario sees the kernel end the program, not itself:
//! `null-read`, `null-write`, `write-code`, `exec-data`, `ud`, `div0`,
//! `stack` (runs into the guard page below the stack) and `kernel-read`
//! (reads an upper-half address). `flags-exit` and `flags-ud` set the
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program. `flags-ac` sets
//! them and spins through a few hundred timer ticks before it faults: the
//! kernel's interrupt handlers must not run with them either (AC would
//! switch SMAP off). `sse` runs an SSE instruction, which CR0.TS makes a
//! fault (spec §5.5).
#![no_std]
#![no_main]

use core::arch::asm;
use relay_rt::Args;
use relay_rt::sys;

relay_rt::main!(main);

/// Somewhere writable and not executable to jump to: `ret`, in data.
static mut DATA: [u8; 16] = [0xC3; 16];

/// RFLAGS bits: direction, nested task, alignment check.
const FLAGS: u64 = (1 << 10) | (1 << 14) | (1 << 18);

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
            b"sse" => asm!("xorps xmm0, xmm0"),
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
            // `exit(0)` at once: no Rust code runs with these flags.
            b"flags-exit" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "syscall",
                flags = in(reg) FLAGS,
                in("rax") 1u64,
                in("rdi") 0u64,
                options(noreturn),
            ),
            b"flags-ac" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "2:",
                "dec rcx",
                "jnz 2b",
                "ud2",
                flags = in(reg) FLAGS,
                in("rcx") 1u64 << 30,
                options(noreturn),
            ),
            b"flags-ud" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "ud2",
                flags = in(reg) FLAGS,
                options(noreturn),
            ),
            _ => {
                let _ = sys::write_all(
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|sse\n",
                );
                return 2;
            }
        }
    }
    let _ = sys::write_all(2, b"t-fault: no fault\n");
    1
}
