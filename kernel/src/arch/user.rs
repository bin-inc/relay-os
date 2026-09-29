//! Ring 3 on x86_64 (user-space gate §5.3, §6.2): the `syscall` entry, going
//! into a program and coming back out of it.
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
//! - **`enter`** saves the kernel's callee-saved registers and stack pointer
//!   (the waiting kernel code), then goes to ring 3 with `iretq`; **`leave`**
//!   goes back to that kernel code from the program's kernel stack, which
//!   is abandoned, with the kernel's flags. `exit` and a program's fault
//!   end that way.
//!
//! A program sets the flags it likes (`popfq`), and neither `syscall` nor
//! an exception clears all of them: `SFMASK` clears the ones the kernel
//! must not run with (NT would make the next `iretq` fault in ring 0; AC
//! would switch SMAP off), and `leave` loads the kernel's own flags, so
//! nothing of the program's reaches the code that waited for it.
//!
//! The kernel uses `gs` only in `syscall_entry`. Because `exit` leaves
//! from inside a call, before the stub's second `swapgs`, `run` sets both
//! `gs` bases afresh before every program.

use super::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA};
use core::arch::naked_asm;
use x86_64::instructions::interrupts;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::registers::model_specific::{
    Efer, EferFlags, GsBase, KernelGsBase, LStar, Msr, SFMask, Star,
};
use x86_64::registers::rflags::RFlags;
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::paging::PhysFrame;
use x86_64::{PhysAddr, VirtAddr};

/// The flags `syscall` clears (spec §6.2): no interrupts until the stub is
/// on the kernel stack, and the direction, trap, alignment-check and
/// nested-task flags in their kernel state.
pub const SFMASK: RFlags = RFlags::INTERRUPT_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::TRAP_FLAG)
    .union(RFlags::ALIGNMENT_CHECK)
    .union(RFlags::NESTED_TASK);

/// The kernel's flags when `leave` goes back: only bit 1, always set.
const KERNEL_RFLAGS: u64 = 0x2;

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

/// Where a program starts (spec §5.3), in the order `enter` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserEntry {
    pub ip: u64,
    pub sp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
}

/// Whether `addr` is canonical: bits 63-47 all equal.
pub fn is_canonical(addr: u64) -> bool {
    let top = addr >> 47;
    top == 0 || top == 0x1_FFFF
}

/// Turns on `syscall` and points it at `syscall_entry`.
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
    // `sysenter` is legal in 64-bit mode on Intel CPUs: with a code segment
    // of 0 it is a general protection fault, whatever the firmware left.
    for msr in IA32_SYSENTER {
        // SAFETY: these MSRs exist on every x86_64 CPU; zero disables
        // `sysenter`.
        unsafe { Msr::new(msr).write(0) };
    }
}

/// Runs a program: `entry` in the address space whose PML4 is at `pml4`,
/// with `kernel_stack` (its top) for its system calls, interrupts and
/// faults. Returns once something calls `leave(waiter)`, with the kernel's
/// own tables (`kernel_pml4`) back in CR3 and interrupts as they were.
/// `waiter` is where `enter` saves the stack pointer to go back to.
///
/// # Safety
/// `waiter` must stay valid until `run` returns, and only this program's
/// `leave` may use it; `pml4` must map the kernel as the current tables do.
pub unsafe fn run(
    entry: &UserEntry,
    kernel_stack: u64,
    pml4: u64,
    kernel_pml4: u64,
    waiter: *mut u64,
) {
    let enabled = interrupts::are_enabled();
    interrupts::disable();
    super::gdt::set_kernel_stack(kernel_stack);
    // SAFETY: interrupts are off and no system call is running, so nothing
    // reads the per-CPU block now. The page tables map the kernel as the
    // current ones do (the upper half is shared).
    unsafe {
        PER_CPU.kernel_rsp = kernel_stack;
        GsBase::write(VirtAddr::new(0));
        KernelGsBase::write(VirtAddr::new(&raw const PER_CPU as u64));
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(pml4)),
            Cr3Flags::empty(),
        );
        enter(waiter, entry);
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(kernel_pml4)),
            Cr3Flags::empty(),
        );
    }
    // A kernel bug if the program's flags came back with it.
    debug_assert!(
        !x86_64::registers::rflags::read().intersects(PROGRAM_FLAGS),
        "a program's flags reached the kernel"
    );
    if enabled {
        interrupts::enable();
    }
}

/// Saves the callee-saved registers and the stack pointer in `*waiter`,
/// then goes to ring 3 at `entry`: its stack, `rdi`, `rsi`, `rdx`,
/// interrupts on, every other register zero. Returns when `leave(waiter)`
/// runs.
#[unsafe(naked)]
unsafe extern "C" fn enter(waiter: *mut u64, entry: *const UserEntry) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "push {ss}",
        "push qword ptr [rsi + 8]",
        "push {rflags}",
        "push {cs}",
        "push qword ptr [rsi]",
        "mov rdi, [rsi + 16]",
        "mov rdx, [rsi + 32]",
        "mov rsi, [rsi + 24]",
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
        ss = const USER_DATA as u64,
        cs = const USER_CODE as u64,
        rflags = const USER_RFLAGS,
    )
}

/// Goes back to the kernel code waiting in `enter(waiter, _)`, on its
/// stack, with interrupts off and the kernel's flags. The stack this runs
/// on is abandoned.
///
/// # Safety
/// `waiter` must be what `enter` saved, and that `enter` must not have
/// returned yet.
#[unsafe(naked)]
pub unsafe extern "C" fn leave(waiter: *mut u64) -> ! {
    naked_asm!(
        "cli",
        "mov rsp, [rdi]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "push {rflags}",
        "popfq",
        "ret",
        rflags = const KERNEL_RFLAGS,
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
    fn enter_reads_the_entry_at_these_offsets() {
        assert_eq!(offset_of!(UserEntry, ip), 0);
        assert_eq!(offset_of!(UserEntry, sp), 8);
        assert_eq!(offset_of!(UserEntry, rdi), 16);
        assert_eq!(offset_of!(UserEntry, rsi), 24);
        assert_eq!(offset_of!(UserEntry, rdx), 32);
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
        assert_eq!(KERNEL_RFLAGS, 2, "leave: nothing of the program's");
    }
}
