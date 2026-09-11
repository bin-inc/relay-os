use core::arch::asm;

use super::exception_stubs;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const EMPTY: Self = Self {
        offset_low: 0,
        selector: 0,
        options: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn handler(address: usize) -> Self {
        Self {
            offset_low: address as u16,
            selector: 0x08,
            options: 0x8e00,
            offset_middle: (address >> 16) as u16,
            offset_high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

static mut GDT: [u64; 3] = [0, 0x00af_9a00_0000_ffff, 0x00af_9200_0000_ffff];
static mut IDT: [IdtEntry; 256] = [IdtEntry::EMPTY; 256];

/// # Safety
/// Descriptor tables are global processor state and may be installed only once during single-core
/// startup while interrupts are disabled.
pub unsafe fn install() {
    // SAFETY: this is the sole initialization of global descriptor tables before they are loaded.
    unsafe {
        let idt = (&raw mut IDT).cast::<IdtEntry>();
        for index in 0..256 {
            idt.add(index)
                .write(IdtEntry::handler(exception_stubs::handler_for(index as u8)));
        }
    }
    let gdt = DescriptorTablePointer {
        limit: (core::mem::size_of::<[u64; 3]>() - 1) as u16,
        base: (&raw const GDT) as *const _ as u64,
    };
    let idt = DescriptorTablePointer {
        limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: (&raw const IDT) as *const _ as u64,
    };
    // SAFETY: the static descriptor tables remain valid forever and use the loader-compatible code selector.
    unsafe {
        asm!("cli", "lgdt [{}]", "lidt [{}]", in(reg) &gdt, in(reg) &idt, options(readonly, nostack));
    }
}

#[unsafe(no_mangle)]
extern "C" fn relay_exception_diagnostic(vector: u64, error: u64, rip: u64) -> ! {
    crate::console::write_fmt(format_args!(
        "[relay] exception vector={vector} error={error:#x} rip={rip:#x}\n"
    ));
    crate::arch::x86_64::halt();
}
