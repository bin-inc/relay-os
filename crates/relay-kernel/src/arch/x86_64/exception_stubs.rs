use core::arch::global_asm;

macro_rules! exception_stub {
    ($name:ident, $vector:literal) => {
        global_asm!(concat!(
            ".global ",
            stringify!($name),
            "\n",
            stringify!($name),
            ":\n",
            "push ",
            stringify!($vector),
            "\n",
            "mov rdi, ",
            stringify!($vector),
            "\n",
            "xor esi, esi\n",
            "mov rdx, [rsp + 8]\n",
            // Preserve the extracted arguments, then satisfy SysV's 16-byte pre-call stack alignment.
            "and rsp, -16\n",
            "call relay_exception_diagnostic\n",
            "1: hlt\n",
            "jmp 1b\n",
        ));
        unsafe extern "C" {
            pub fn $name();
        }
    };
    ($name:ident, $vector:literal, error) => {
        global_asm!(concat!(
            ".global ",
            stringify!($name),
            "\n",
            stringify!($name),
            ":\n",
            "push ",
            stringify!($vector),
            "\n",
            "mov rdi, ",
            stringify!($vector),
            "\n",
            "mov rsi, [rsp + 8]\n",
            "mov rdx, [rsp + 16]\n",
            // Error-code frames have a different hardware stack shape; dynamic alignment handles both.
            "and rsp, -16\n",
            "call relay_exception_diagnostic\n",
            "1: hlt\n",
            "jmp 1b\n",
        ));
        unsafe extern "C" {
            pub fn $name();
        }
    };
}

exception_stub!(divide_error, 0);
exception_stub!(debug, 1);
exception_stub!(nmi, 2);
exception_stub!(breakpoint, 3);
exception_stub!(overflow, 4);
exception_stub!(bound_range, 5);
exception_stub!(invalid_opcode, 6);
exception_stub!(device_not_available, 7);
exception_stub!(double_fault, 8, error);
exception_stub!(coprocessor_segment_overrun, 9);
exception_stub!(invalid_tss, 10, error);
exception_stub!(segment_not_present, 11, error);
exception_stub!(stack_segment_fault, 12, error);
exception_stub!(general_protection, 13, error);
exception_stub!(page_fault, 14, error);
exception_stub!(reserved_15, 15);
exception_stub!(x87_floating_point, 16);
exception_stub!(alignment_check, 17, error);
exception_stub!(machine_check, 18);
exception_stub!(simd_floating_point, 19);
exception_stub!(virtualization, 20);
exception_stub!(control_protection, 21, error);
exception_stub!(reserved_22, 22);
exception_stub!(reserved_23, 23);
exception_stub!(reserved_24, 24);
exception_stub!(reserved_25, 25);
exception_stub!(reserved_26, 26);
exception_stub!(reserved_27, 27);
exception_stub!(hypervisor_injection, 28);
exception_stub!(vmm_communication, 29, error);
exception_stub!(security, 30, error);
exception_stub!(reserved_31, 31);
exception_stub!(unhandled, 255);

pub fn handler_for(vector: u8) -> usize {
    let handler = match vector {
        0 => divide_error,
        1 => debug,
        2 => nmi,
        3 => breakpoint,
        4 => overflow,
        5 => bound_range,
        6 => invalid_opcode,
        7 => device_not_available,
        8 => double_fault,
        9 => coprocessor_segment_overrun,
        10 => invalid_tss,
        11 => segment_not_present,
        12 => stack_segment_fault,
        13 => general_protection,
        14 => page_fault,
        15 => reserved_15,
        16 => x87_floating_point,
        17 => alignment_check,
        18 => machine_check,
        19 => simd_floating_point,
        20 => virtualization,
        21 => control_protection,
        22 => reserved_22,
        23 => reserved_23,
        24 => reserved_24,
        25 => reserved_25,
        26 => reserved_26,
        27 => reserved_27,
        28 => hypervisor_injection,
        29 => vmm_communication,
        30 => security,
        31 => reserved_31,
        _ => unhandled,
    };
    handler as *const () as usize
}
