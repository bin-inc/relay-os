//! x86_64 (spec §7.1): the call number in `rax`, arguments in `rdi`, `rsi`,
//! `rdx`, `r10`, `r8`, `r9`, the result in `rax`; `syscall` itself
//! clobbers `rcx` and `r11`. And `_start`, which the kernel jumps to with
//! the arguments' address, length and count in `rdi`, `rsi`, `rdx`, and the
//! environment's in `rcx`, `r8`, `r9`.

use core::arch::{asm, naked_asm};
use relay_abi::Call;

/// # Safety
/// The arguments must be what `call` expects (pointers to memory the
/// program owns, of the stated lengths).
pub unsafe fn syscall(call: Call, a: [u64; 6]) -> u64 {
    let result: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") call.number() => result,
            in("rdi") a[0],
            in("rsi") a[1],
            in("rdx") a[2],
            in("r10") a[3],
            in("r8") a[4],
            in("r9") a[5],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

/// The program's first instruction. The stack is 16-byte aligned at entry
/// (spec §5.3); a call leaves it as the SysV ABI expects on function
/// entry. `rdi`, `rsi`, `rdx`, `rcx`, `r8` and `r9` pass through to `start`
/// untouched, its six arguments.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "sysv64" fn _start() -> ! {
    naked_asm!(
        "xor ebp, ebp",
        "and rsp, -16",
        "call {start}",
        "ud2",
        start = sym crate::start::start,
    )
}
