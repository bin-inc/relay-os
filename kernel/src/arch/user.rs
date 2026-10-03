//! Ring 3 on x86_64 (user-space gate §5.3, §6.2): the `syscall` entry, and
//! going into a program the first time.
//!
//! - **`syscall`** jumps to `syscall_entry` with interrupts, direction,
//!   trap and alignment-check flags cleared (`SFMASK`). The stub reaches
//!   the per-CPU block with `swapgs`, keeps the program's stack pointer
//!   there, switches to the program's kernel stack, saves every register
//!   as a `SyscallFrame`, turns interrupts back on and calls the
//!   architecture-neutral dispatcher (`crate::proc::system_call`). It
//!   returns with `sysret`, but only to a canonical address: on Intel CPUs
//!   `sysret` to a non-canonical `rcx` faults in ring 0 with the program's
//!   stack pointer, so such a program is killed instead.
//! - **`enter`** goes to ring 3 at a program's entry point with `iretq`,
//!   the first time the program runs (a new process's first frame calls it,
//!   see `context`). After that a program leaves the kernel only the way
//!   it came in: returning from a system call or an interrupt.
//!
//! A program sets the flags it likes (`popfq`), and neither `syscall` nor
//! an exception clears all of them: `SFMASK` clears the ones the kernel
//! must not run with (NT would make the next `iretq` fault in ring 0; AC
//! would switch SMAP off), and every switch between processes checks that
//! none of them reached the kernel (`context`).
//!
//! **`gs` and the flags.** The kernel always runs with its own `gs` base
//! (the per-CPU block) and the program's (always 0: nothing lets a program
//! set one) in `KernelGsBase`: every way into the kernel from ring 3
//! (`syscall`, and the interrupt and exception stubs when they interrupted
//! ring 3) starts with `swapgs`, and every way back ends with one. So every
//! kernel context has the same `gs`, whichever way it came in, and
//! switching from one to another never mixes them. The same ways in leave
//! the program's flags behind: `SFMASK` for `syscall`, and the stubs load
//! the kernel's own (`push 2; popfq`), since an interrupt or trap gate
//! clears only IF, TF, NT and RF, and AC would switch SMAP off. That works
//! on every CPU, where `clac` exists only with SMAP.

use super::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA};
use crate::exec::Entry;
use core::arch::naked_asm;
use x86_64::VirtAddr;
use x86_64::registers::model_specific::{
    Efer, EferFlags, GsBase, KernelGsBase, LStar, Msr, SFMask, Star,
};
use x86_64::registers::rflags::RFlags;
use x86_64::structures::gdt::SegmentSelector;

/// The flags `syscall` clears (spec §6.2): no interrupts until the stub is
/// on the kernel stack, and the direction, trap, alignment-check and
/// nested-task flags in their kernel state.
pub const SFMASK: RFlags = RFlags::INTERRUPT_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::TRAP_FLAG)
    .union(RFlags::ALIGNMENT_CHECK)
    .union(RFlags::NESTED_TASK);

/// Flags a program may set that the kernel must never run with.
const PROGRAM_FLAGS: RFlags = RFlags::TRAP_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::ALIGNMENT_CHECK)
    .union(RFlags::NESTED_TASK);

/// `sysenter`'s code segment, stack and entry MSRs.
const IA32_SYSENTER: [u32; 3] = [0x174, 0x175, 0x176];

/// A program starts with only the interrupt flag (and bit 1, always set).
const USER_RFLAGS: u64 = 0x202;

/// Reached through `gs` in `syscall_entry`: the kernel stack for the next
/// system call, and the program's stack pointer during one.
#[repr(C)]
struct PerCpu {
    kernel_rsp: u64,
    user_rsp: u64,
}

static mut PER_CPU: PerCpu = PerCpu {
    kernel_rsp: 0,
    user_rsp: 0,
};

/// The registers of a program in a system call, as `syscall_entry` pushes
/// them, lowest address first.
#[repr(C)]
#[derive(Debug)]
pub struct SyscallFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
    /// `rcx`: where `syscall` came from.
    pub rip: u64,
    /// `r11`: the flags `syscall` saw.
    pub rflags: u64,
    pub rsp: u64,
}

/// Whether `addr` is canonical: bits 63-47 all equal.
pub fn is_canonical(addr: u64) -> bool {
    let top = addr >> 47;
    top == 0 || top == 0x1_FFFF
}

/// Whether `gs` is the kernel's (the per-CPU block), as it always must be
/// while the kernel runs.
pub fn gs_is_kernel() -> bool {
    GsBase::read().as_u64() == &raw const PER_CPU as u64
}

/// Whether none of the flags a program may set and the kernel must never
/// run with (TF, DF, AC, NT) is set.
pub fn flags_are_kernel() -> bool {
    !x86_64::registers::rflags::read().intersects(PROGRAM_FLAGS)
}

/// Turns on `syscall`, points it at `syscall_entry`, and gives the kernel
/// its `gs` (the program's is 0).
pub fn init() {
    // SAFETY: the GDT has the segments STAR names; the entry stub is
    // ready; the per-CPU block is 'static.
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };
    Star::write(
        SegmentSelector(USER_CODE),
        SegmentSelector(USER_DATA),
        SegmentSelector(KERNEL_CODE),
        SegmentSelector(KERNEL_DATA),
    )
    .expect("the GDT's segments are in the order sysret needs");
    LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
    SFMask::write(SFMASK);
    GsBase::write(VirtAddr::new(&raw const PER_CPU as u64));
    KernelGsBase::write(VirtAddr::new(0));
    // `sysenter` is legal in 64-bit mode on Intel CPUs: with a code segment
    // of 0 it is a general protection fault, whatever the firmware left.
    for msr in IA32_SYSENTER {
        // SAFETY: these MSRs exist on every x86_64 CPU; zero disables
        // `sysenter`.
        unsafe { Msr::new(msr).write(0) };
    }
}

/// Points the CPU at `top` as the kernel stack for the next process's
/// system calls (the per-CPU block) and for its interrupts and exceptions
/// (TSS `rsp0`).
pub fn set_kernel_stack(top: u64) {
    // SAFETY: interrupts are off during a switch and no system call is
    // running, so nothing reads the per-CPU block now.
    unsafe { PER_CPU.kernel_rsp = top };
    super::gdt::set_kernel_stack(top);
}

/// Whether `syscall` and interrupts from ring 3 would land on the same
/// kernel stack.
pub fn kernel_stacks_agree() -> bool {
    // SAFETY: a plain read, as above.
    unsafe { PER_CPU.kernel_rsp == super::gdt::kernel_stack() }
}

/// Goes to ring 3 at `entry` (spec §5.3) with the program's `gs`: its
/// stack, `rdi` = the arguments' address, `rsi` their length, `rdx` their
/// count, `rcx`, `r8` and `r9` the environment's (programmable shell gate
/// §8.2), interrupts on, every other register zero. The kernel stack this
/// is called on is where the program's system calls, interrupts and
/// faults arrive from now on.
///
/// # Safety
/// The program's address space is in CR3 and its kernel stack is set
/// (`set_kernel_stack`).
#[unsafe(naked)]
pub unsafe extern "C" fn enter(entry: *const Entry) -> ! {
    naked_asm!(
        "cli",
        "push {ss}",
        "push qword ptr [rdi + {sp}]",
        "push {rflags}",
        "push {cs}",
        "push qword ptr [rdi + {ip}]",
        "mov rsi, [rdi + {len}]",
        "mov rdx, [rdi + {argc}]",
        "mov rcx, [rdi + {env}]",
        "mov r8, [rdi + {env_len}]",
        "mov r9, [rdi + {envc}]",
        "mov rdi, [rdi + {args}]",
        "xor eax, eax", "xor ebx, ebx", "xor ebp, ebp",
        "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "swapgs",
        "iretq",
        ss = const USER_DATA as u64,
        cs = const USER_CODE as u64,
        rflags = const USER_RFLAGS,
        ip = const core::mem::offset_of!(Entry, ip),
        sp = const core::mem::offset_of!(Entry, sp),
        args = const core::mem::offset_of!(Entry, args),
        len = const core::mem::offset_of!(Entry, args_len),
        argc = const core::mem::offset_of!(Entry, argc),
        env = const core::mem::offset_of!(Entry, env),
        env_len = const core::mem::offset_of!(Entry, env_len),
        envc = const core::mem::offset_of!(Entry, envc),
    )
}

/// The `syscall` instruction's target. See the module comment.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs",
        "mov gs:[8], rsp",
        "mov rsp, gs:[0]",
        "push qword ptr gs:[8]",
        "push r11", "push rcx",
        "push rax", "push rdi", "push rsi", "push rdx", "push r10", "push r8", "push r9",
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",
        "sti",
        "call {dispatch}",
        "cli",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "pop r9", "pop r8", "pop r10", "pop rdx", "pop rsi", "pop rdi", "pop rax",
        "pop rcx", "pop r11",
        "pop rsp",
        "swapgs",
        "sysretq",
        dispatch = sym syscall_dispatch,
    )
}

extern "C" fn syscall_dispatch(frame: &mut SyscallFrame) {
    let args = [
        frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9,
    ];
    frame.rax = crate::proc::system_call(frame.rax, args);
    crate::proc::before_user();
    if !is_canonical(frame.rip) {
        crate::proc::non_canonical_return(frame.rip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_frame_is_what_the_stub_pushes() {
        // 16 pushes: the program's rsp first (highest), r15 last.
        assert_eq!(size_of::<SyscallFrame>(), 16 * 8);
        assert_eq!(offset_of!(SyscallFrame, r15), 0);
        assert_eq!(offset_of!(SyscallFrame, rbx), 5 * 8);
        assert_eq!(offset_of!(SyscallFrame, r9), 6 * 8);
        assert_eq!(offset_of!(SyscallFrame, r10), 8 * 8);
        assert_eq!(offset_of!(SyscallFrame, rdi), 11 * 8);
        assert_eq!(offset_of!(SyscallFrame, rax), 12 * 8);
        assert_eq!(offset_of!(SyscallFrame, rip), 13 * 8);
        assert_eq!(offset_of!(SyscallFrame, rflags), 14 * 8);
        assert_eq!(offset_of!(SyscallFrame, rsp), 15 * 8);
        // 16 pushes from a 16-byte aligned top keep the call aligned.
        assert_eq!(size_of::<SyscallFrame>() % 16, 0);
    }

    #[test]
    fn the_per_cpu_block_is_where_syscall_entry_looks() {
        assert_eq!(offset_of!(PerCpu, kernel_rsp), 0);
        assert_eq!(offset_of!(PerCpu, user_rsp), 8);
    }

    #[test]
    fn only_canonical_addresses_are_returned_to() {
        for ok in [
            0,
            0x40_1000,
            0x7FFF_FFFF_FFFF,
            0xFFFF_8000_0000_0000,
            u64::MAX,
        ] {
            assert!(is_canonical(ok), "{ok:#x}");
        }
        for bad in [
            0x8000_0000_0000,
            0x7FFF_FFFF_FFFF + 1,
            0xFFFF_7FFF_FFFF_FFFF,
            1 << 63,
        ] {
            assert!(!is_canonical(bad), "{bad:#x}");
        }
    }

    #[test]
    fn syscall_clears_the_flags_the_kernel_needs_clear() {
        // IF, DF, TF, AC and NT.
        assert_eq!(
            SFMASK.bits(),
            (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18) | (1 << 14)
        );
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
    }

    #[test]
    fn a_program_starts_with_its_own_gs() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(enter as *const [u8; 128]) };
        // `swapgs; iretq`.
        let last = [0x0F, 0x01, 0xF8, 0x48, 0xCF];
        assert!(code.windows(5).any(|w| w == last));
    }
}
