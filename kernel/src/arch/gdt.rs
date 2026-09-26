//! GDT with kernel code/data segments and a TSS whose IST1 stack is used for
//! double faults (so a kernel stack overflow still reaches the panic screen).

use spin::Once;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

/// IST index (1-based in the gate, 0-based in the TSS table).
pub const DOUBLE_FAULT_IST: u8 = 1;
const IST_STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
#[allow(dead_code)] // only its address is used
struct Stack([u8; IST_STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: Stack = Stack([0; IST_STACK_SIZE]);

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

static TSS: Once<TaskStateSegment> = Once::new();
static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::new();

pub fn init() {
    let tss = TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();
        let top = &raw const DOUBLE_FAULT_STACK as u64 + IST_STACK_SIZE as u64;
        tss.interrupt_stack_table[(DOUBLE_FAULT_IST - 1) as usize] = VirtAddr::new(top);
        tss
    });
    let (gdt, sel) = GDT.call_once(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let code = gdt.append(Descriptor::kernel_code_segment());
        let data = gdt.append(Descriptor::kernel_data_segment());
        let tss = gdt.append(Descriptor::tss_segment(tss));
        (gdt, Selectors { code, data, tss })
    });
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
