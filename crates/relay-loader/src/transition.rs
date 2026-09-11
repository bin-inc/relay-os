#[cfg(any(target_os = "uefi", test))]
use core::arch::global_asm;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionPage {
    pub physical_start: u64,
}

#[cfg(any(target_os = "uefi", test))]
global_asm!(
    ".global relay_transition_page_start",
    "relay_transition_page_start:",
    "cli",
    "mov r13, rdx",
    "mov r12, rcx",
    "mov r11, rdi",
    "mov eax, 0x80000000",
    "cpuid",
    "cmp eax, 0x80000001",
    "jb 2f",
    "mov eax, 0x80000001",
    "cpuid",
    "test edx, 0x100000",
    "jz 2f",
    "mov ecx, 0xc0000080",
    "rdmsr",
    "or eax, 0x800",
    "wrmsr",
    "2:",
    "mov cr3, rsi",
    "cld",
    "mov rsp, r13",
    "and rsp, -16",
    "sub rsp, 8",
    "mov rdi, r11",
    "jmp r12",
    ".global relay_transition_page_end",
    "relay_transition_page_end:",
);

#[cfg(any(target_os = "uefi", test))]
unsafe extern "C" {
    static relay_transition_page_start: u8;
    static relay_transition_page_end: u8;
}

#[cfg(any(target_os = "uefi", test))]
pub(crate) fn trampoline_bytes() -> &'static [u8] {
    let start = core::ptr::addr_of!(relay_transition_page_start);
    let end = core::ptr::addr_of!(relay_transition_page_end);
    let len = (end as usize) - (start as usize);
    // SAFETY: the global assembly labels delimit the fixed trampoline instruction sequence.
    unsafe { core::slice::from_raw_parts(start, len) }
}
