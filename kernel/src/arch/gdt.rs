//! GDT with the kernel's and ring 3's code and data segments, in the order
//! `syscall` and `sysret` need (user-space gate §6.2), and a TSS: `rsp0` is
//! the stack the CPU switches to when an interrupt or exception arrives in
//! ring 3, and IST1 the stack for double faults, NMIs and machine checks
//! (so a kernel stack overflow, or an NMI taken with a program's stack
//! pointer, still reaches the panic screen).

use spin::Once;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

/// IST index (1-based in the gate, 0-based in the TSS table).
pub const DOUBLE_FAULT_IST: u8 = 1;
const IST_STACK_SIZE: usize = 16 * 1024;

/// The selectors, as `gdt()` lays the table out: kernel code and data,
/// then user data before user code, as `sysret` loads them (STAR's user
/// base + 8 for SS, + 16 for CS), each with its privilege level.
pub const KERNEL_CODE: u16 = 0x08;
pub const KERNEL_DATA: u16 = 0x10;
pub const USER_DATA: u16 = 0x18 | 3;
pub const USER_CODE: u16 = 0x20 | 3;

#[repr(align(16))]
#[allow(dead_code)] // only its address is used
struct Stack([u8; IST_STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: Stack = Stack([0; IST_STACK_SIZE]);

/// Written by `set_kernel_stack` while the CPU may read it, so it is not
/// behind a reference.
static mut TSS: TaskStateSegment = TaskStateSegment::new();

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    user_data: SegmentSelector,
    user_code: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::new();

/// The table, with the TSS at `tss`.
fn gdt(tss: Descriptor) -> (GlobalDescriptorTable, Selectors) {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let user_data = gdt.append(Descriptor::user_data_segment());
    let user_code = gdt.append(Descriptor::user_code_segment());
    let tss = gdt.append(tss);
    (
        gdt,
        Selectors {
            code,
            data,
            user_data,
            user_code,
            tss,
        },
    )
}

pub fn init() {
    let tss = &raw mut TSS;
    // SAFETY: runs once, before interrupts and before anything reads the
    // TSS; the stack is 'static.
    unsafe {
        let top = &raw const DOUBLE_FAULT_STACK as u64 + IST_STACK_SIZE as u64;
        (*tss).interrupt_stack_table[(DOUBLE_FAULT_IST - 1) as usize] = VirtAddr::new(top);
    }
    // SAFETY: the TSS is 'static and stays where it is.
    let (gdt, sel) = GDT.call_once(|| gdt(unsafe { Descriptor::tss_segment_unchecked(tss) }));
    debug_assert_eq!((sel.user_data.0, sel.user_code.0), (USER_DATA, USER_CODE));
    gdt.load();
    // SAFETY: the selectors index the GDT that was just loaded.
    unsafe {
        CS::set_reg(sel.code);
        SS::set_reg(sel.data);
        DS::set_reg(sel.data);
        ES::set_reg(sel.data);
        load_tss(sel.tss);
    }
}

/// The stack the CPU switches to when an interrupt or exception arrives
/// in ring 3 (TSS `rsp0`): the running program's kernel stack.
pub fn set_kernel_stack(top: u64) {
    // SAFETY: one CPU, and the CPU reads rsp0 only when it enters ring 0
    // from ring 3, which cannot happen while the kernel runs here.
    unsafe { TSS.privilege_stack_table[0] = VirtAddr::new(top) };
}

/// TSS `rsp0`, as `set_kernel_stack` left it.
pub fn kernel_stack() -> u64 {
    // SAFETY: a plain read on one CPU.
    unsafe { TSS.privilege_stack_table[0].as_u64() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x86_64::PrivilegeLevel;

    #[test]
    fn the_segments_are_in_the_order_sysret_needs() {
        static FAKE_TSS: TaskStateSegment = TaskStateSegment::new();
        let (_, sel) = gdt(Descriptor::tss_segment(&FAKE_TSS));
        assert_eq!(sel.code.0, KERNEL_CODE);
        assert_eq!(sel.data.0, KERNEL_DATA);
        assert_eq!(sel.user_data.0, USER_DATA);
        assert_eq!(sel.user_code.0, USER_CODE);
        assert_eq!(sel.user_code.rpl(), PrivilegeLevel::Ring3);
        assert_eq!(sel.user_data.rpl(), PrivilegeLevel::Ring3);
        // syscall: CS from STAR[47:32], SS 8 above it; sysret: SS 8 and CS
        // 16 above STAR[63:48], with RPL 3.
        assert_eq!(KERNEL_DATA, KERNEL_CODE + 8);
        let user_base = KERNEL_DATA;
        assert_eq!(USER_DATA, (user_base + 8) | 3);
        assert_eq!(USER_CODE, (user_base + 16) | 3);
        assert_eq!(sel.tss.0, 0x28);
    }
}
