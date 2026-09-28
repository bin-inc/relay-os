//! Restarting and switching off (spec §7.4), through the registers the
//! FADT describes and the `\_S5` values of the DSDT. The shell has already
//! shut the filesystems down when these run. Which registers to write, and
//! what, is worked out here and tested on the host; the writes themselves
//! are thin glue.

use crate::acpi::aml::SleepType;
use crate::acpi::tables::{AddressSpace, Fadt, GenericAddress};
use crate::mm::{self, paging::Cache};
use crate::{acpi, arch, console, kprintln, timer};
use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;
use x86_64::instructions::port::Port;

/// PM1 control: the sleep type field and the sleep enable bit (ACPI 6.5
/// table 4.13).
pub const SLP_TYP_SHIFT: u16 = 10;
pub const SLP_TYP_MASK: u16 = 7 << SLP_TYP_SHIFT;
pub const SLP_EN: u16 = 1 << 13;
/// The reset control register of Intel chipsets (and QEMU's): 0x06 asks
/// for a full reset.
pub const RESET_CONTROL_PORT: u16 = 0xCF9;
pub const RESET_CONTROL_VALUE: u8 = 0x06;
/// QEMU's `isa-debug-exit` device (the e2e machine has it at 0xF4).
pub const DEBUG_EXIT_PORT: u16 = 0xF4;
/// What `poweroff` writes there in test mode: QEMU then exits with status
/// (0x10 << 1) | 1 = 33, which the e2e runner takes as a clean power-off.
pub const TEST_EXIT_CODE: u8 = 0x10;
/// How long each way of restarting or switching off gets before the next.
const ATTEMPT_TIME: Duration = Duration::from_millis(500);

/// A register the firmware describes, and the width of one access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reg {
    Io { port: u16, bytes: u8 },
    Memory { phys: u64, bytes: u8 },
}

impl Reg {
    /// The register a Generic Address Structure names, if this kernel can
    /// reach it: I/O or memory space (a memory register aligned to its
    /// width), a whole number of bytes starting at bit 0, 1, 2, 4 or 8
    /// bytes wide (the access size field wins over the bit width when it is
    /// set, ACPI 6.5 §5.2.3.2).
    pub fn from_gas(g: &GenericAddress) -> Option<Reg> {
        let bytes = match g.access_size {
            0 if g.bit_width.is_multiple_of(8) => g.bit_width / 8,
            0 => return None,
            n @ 1..=4 => 1 << (n - 1),
            _ => return None,
        };
        if g.bit_offset != 0 || g.address == 0 || !matches!(bytes, 1 | 2 | 4 | 8) {
            return None;
        }
        match g.space {
            AddressSpace::Io if bytes <= 4 => Some(Reg::Io {
                port: u16::try_from(g.address).ok()?,
                bytes,
            }),
            // A misaligned volatile access could fault.
            AddressSpace::Memory if g.address.is_multiple_of(bytes as u64) => Some(Reg::Memory {
                phys: g.address,
                bytes,
            }),
            _ => None,
        }
    }
}

/// One way to restart the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMethod {
    /// The FADT's reset register and the value to write to it.
    Register(Reg, u8),
    /// Port 0xCF9 (spec §7.4's fallback).
    ResetControl,
    /// An empty interrupt table and an exception: always resets.
    TripleFault,
}

/// How the screen names each way: the last line before a reset is the one
/// that worked.
impl fmt::Display for ResetMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            ResetMethod::Register(..) => "the FADT reset register",
            ResetMethod::ResetControl => "port 0xcf9",
            ResetMethod::TripleFault => "a triple fault",
        })
    }
}

/// The ways to restart, in the order spec §7.4 tries them.
pub fn reset_methods(fadt: Option<&Fadt>) -> Vec<ResetMethod> {
    let mut methods = Vec::new();
    if let Some((gas, value)) = fadt.and_then(|f| f.reset)
        && let Some(reg) = Reg::from_gas(&gas)
    {
        methods.push(ResetMethod::Register(reg, value));
    }
    methods.push(ResetMethod::ResetControl);
    methods.push(ResetMethod::TripleFault);
    methods
}

/// PM1 control after asking for sleep type `slp_typ` (the `\_S5` value):
/// the other bits are kept, as Linux does.
pub fn pm1_sleep_value(current: u16, slp_typ: u8) -> u16 {
    current & !(SLP_TYP_MASK | SLP_EN) | ((slp_typ as u16) << SLP_TYP_SHIFT) & SLP_TYP_MASK | SLP_EN
}

/// The PM1 control registers and the sleep types that switch the machine
/// off (S5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftOff {
    pub pm1a: (Reg, u8),
    pub pm1b: Option<(Reg, u8)>,
}

/// The writes that switch the machine off, given what PM1a and PM1b
/// control hold: the sleep types into both, then the same with `SLP_EN`, so
/// PM1b is set up before PM1a starts the sleep (as Linux's
/// `acpi_hw_legacy_sleep` does).
pub fn soft_off_writes(off: &SoftOff, current_a: u16, current_b: u16) -> Vec<(Reg, u16)> {
    let regs = [
        Some((off.pm1a, current_a)),
        off.pm1b.map(|b| (b, current_b)),
    ];
    let mut writes = Vec::new();
    for enable in [0, SLP_EN] {
        for ((reg, typ), current) in regs.iter().flatten() {
            writes.push((*reg, pm1_sleep_value(*current, *typ) & !SLP_EN | enable));
        }
    }
    writes
}

/// What `poweroff` writes, or why it cannot (the reason is logged, and the
/// machine halts with the safe-to-power-off message).
pub fn soft_off(fadt: Option<&Fadt>, s5: Option<SleepType>) -> Result<SoftOff, &'static str> {
    let fadt = fadt.ok_or("no FADT")?;
    let s5 = s5.ok_or("no \\_S5 in the DSDT")?;
    let pm1a = fadt
        .pm1a_cnt
        .as_ref()
        .and_then(Reg::from_gas)
        .ok_or("no usable PM1a control register")?;
    let pm1b = fadt.pm1b_cnt.as_ref().and_then(Reg::from_gas);
    Ok(SoftOff {
        pm1a: (pm1a, s5.a),
        pm1b: pm1b.map(|r| (r, s5.b)),
    })
}

fn read(reg: Reg) -> u64 {
    // SAFETY: the firmware describes these registers for this purpose.
    unsafe {
        match reg {
            Reg::Io { port, bytes: 1 } => Port::<u8>::new(port).read() as u64,
            Reg::Io { port, bytes: 2 } => Port::<u16>::new(port).read() as u64,
            Reg::Io { port, .. } => Port::<u32>::new(port).read() as u64,
            Reg::Memory { phys, bytes } => {
                match mm::map_mmio(phys, bytes as u64, Cache::Uncached) {
                    Ok(p) => match bytes {
                        1 => p.read_volatile() as u64,
                        2 => p.cast::<u16>().read_volatile() as u64,
                        4 => p.cast::<u32>().read_volatile() as u64,
                        _ => p.cast::<u64>().read_volatile(),
                    },
                    Err(_) => 0,
                }
            }
        }
    }
}

fn write(reg: Reg, value: u64) {
    // SAFETY: as in `read`.
    unsafe {
        match reg {
            Reg::Io { port, bytes: 1 } => Port::<u8>::new(port).write(value as u8),
            Reg::Io { port, bytes: 2 } => Port::<u16>::new(port).write(value as u16),
            Reg::Io { port, .. } => Port::<u32>::new(port).write(value as u32),
            Reg::Memory { phys, bytes } => {
                if let Ok(p) = mm::map_mmio(phys, bytes as u64, Cache::Uncached) {
                    match bytes {
                        1 => p.write_volatile(value as u8),
                        2 => p.cast::<u16>().write_volatile(value as u16),
                        4 => p.cast::<u32>().write_volatile(value as u32),
                        _ => p.cast::<u64>().write_volatile(value),
                    }
                }
            }
        }
    }
}

/// Restarts the machine (spec §7.4): the FADT reset register, then port
/// 0xCF9, then a triple fault, each given half a second and named on the
/// screen first.
pub fn reboot() -> ! {
    x86_64::instructions::interrupts::disable();
    for method in reset_methods(acpi::get().and_then(|a| a.fadt.as_ref())) {
        kprintln!("relay: restarting through {method}");
        match method {
            ResetMethod::Register(reg, value) => write(reg, value as u64),
            ResetMethod::ResetControl => {
                write(
                    Reg::Io {
                        port: RESET_CONTROL_PORT,
                        bytes: 1,
                    },
                    RESET_CONTROL_VALUE as u64,
                );
            }
            ResetMethod::TripleFault => {
                // SAFETY: nothing runs after this; an exception with an
                // empty interrupt table resets the CPU.
                unsafe {
                    x86_64::instructions::tables::lidt(
                        &x86_64::structures::DescriptorTablePointer {
                            limit: 0,
                            base: x86_64::VirtAddr::new(0),
                        },
                    );
                    core::arch::asm!("int3");
                }
            }
        }
        timer::sleep(ATTEMPT_TIME);
    }
    arch::halt_forever()
}

/// Switches the machine off (spec §7.4): in test mode QEMU's
/// `isa-debug-exit` first, then `SLP_TYPa | SLP_EN` into PM1a control (and
/// PM1b's value into PM1b). If that is impossible or does not work, the
/// safe-to-power-off message and a halt.
pub fn poweroff(test_mode: bool) -> ! {
    kprintln!("relay: powering off");
    x86_64::instructions::interrupts::disable();
    if test_mode {
        write(
            Reg::Io {
                port: DEBUG_EXIT_PORT,
                bytes: 1,
            },
            TEST_EXIT_CODE as u64,
        );
    }
    let acpi = acpi::get();
    match soft_off(acpi.and_then(|a| a.fadt.as_ref()), acpi.and_then(|a| a.s5)) {
        Ok(off) => {
            let current_a = read(off.pm1a.0) as u16;
            let current_b = off.pm1b.map_or(0, |(reg, _)| read(reg) as u16);
            for (reg, value) in soft_off_writes(&off, current_a, current_b) {
                write(reg, value as u64);
            }
            timer::sleep(ATTEMPT_TIME);
            kprintln!("relay: the machine did not switch off");
        }
        Err(why) => kprintln!("relay: cannot switch off: {why}"),
    }
    console::write_output(b"System halted. It is now safe to power off.\n");
    arch::halt_forever()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gas(space: AddressSpace, bits: u8, access: u8, address: u64) -> GenericAddress {
        GenericAddress {
            space,
            bit_width: bits,
            bit_offset: 0,
            access_size: access,
            address,
        }
    }

    fn fadt(reset: Option<(GenericAddress, u8)>) -> Fadt {
        Fadt {
            dsdt: 0,
            pm1a_cnt: Some(gas(AddressSpace::Io, 16, 0, 0x604)),
            pm1b_cnt: None,
            reset,
            century: 0,
        }
    }

    #[test]
    fn registers_take_their_width_from_the_access_size_or_the_bit_width() {
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 8, 1, 0xCF9)),
            Some(Reg::Io {
                port: 0xCF9,
                bytes: 1
            })
        );
        // QEMU's PM1a control block: 16 bits, access size unset.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 16, 0, 0x604)),
            Some(Reg::Io {
                port: 0x604,
                bytes: 2
            })
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 8, 3, 0xFED0_0000)),
            Some(Reg::Memory {
                phys: 0xFED0_0000,
                bytes: 4
            })
        );
    }

    #[test]
    fn registers_this_kernel_cannot_reach_are_refused() {
        for g in [
            gas(AddressSpace::Other(2), 8, 1, 0xCF9), // PCI config space
            gas(AddressSpace::Io, 8, 1, 0x1_0000),    // beyond the I/O space
            gas(AddressSpace::Io, 8, 1, 0),           // not described
            gas(AddressSpace::Io, 12, 0, 0xCF9),      // not whole bytes
            gas(AddressSpace::Io, 8, 5, 0xCF9),       // reserved access size
            gas(AddressSpace::Io, 64, 0, 0xCF9),      // no 64-bit port access
            gas(AddressSpace::Memory, 0, 0, 0xFED0_0000), // no width
        ] {
            assert_eq!(Reg::from_gas(&g), None, "{g:?}");
        }
        let mut g = gas(AddressSpace::Io, 8, 1, 0xCF9);
        g.bit_offset = 1;
        assert_eq!(Reg::from_gas(&g), None);
    }

    #[test]
    fn a_memory_register_off_its_alignment_is_refused() {
        // A volatile access to it would fault (and fails Rust's alignment
        // check): firmware values must never do that.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 32, 3, 0xFED0_0001)),
            None
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 16, 2, 0xFED0_0003)),
            None
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 16, 2, 0xFED0_0002)),
            Some(Reg::Memory {
                phys: 0xFED0_0002,
                bytes: 2
            })
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 8, 1, 0xFED0_0003)),
            Some(Reg::Memory {
                phys: 0xFED0_0003,
                bytes: 1
            })
        );
        // I/O ports have no alignment rule.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 16, 2, 0x605)),
            Some(Reg::Io {
                port: 0x605,
                bytes: 2
            })
        );
    }

    #[test]
    fn reboot_tries_the_reset_register_then_port_cf9_then_a_triple_fault() {
        // QEMU's FADT: reset io 0xcf9 <- 0xf.
        let f = fadt(Some((gas(AddressSpace::Io, 8, 1, 0xCF9), 0x0F)));
        assert_eq!(
            reset_methods(Some(&f)),
            [
                ResetMethod::Register(
                    Reg::Io {
                        port: 0xCF9,
                        bytes: 1
                    },
                    0x0F
                ),
                ResetMethod::ResetControl,
                ResetMethod::TripleFault,
            ]
        );
        let unusable = fadt(Some((gas(AddressSpace::Other(2), 8, 1, 0xCF9), 0x06)));
        for f in [None, Some(&fadt(None)), Some(&unusable)] {
            assert_eq!(
                reset_methods(f),
                [ResetMethod::ResetControl, ResetMethod::TripleFault]
            );
        }
    }

    #[test]
    fn each_way_of_restarting_is_named_on_the_screen() {
        let reg = Reg::Io {
            port: 0xCF9,
            bytes: 1,
        };
        assert_eq!(
            ResetMethod::Register(reg, 0x0F).to_string(),
            "the FADT reset register"
        );
        assert_eq!(ResetMethod::ResetControl.to_string(), "port 0xcf9");
        assert_eq!(ResetMethod::TripleFault.to_string(), "a triple fault");
    }

    #[test]
    fn the_sleep_value_sets_the_type_and_enable_and_keeps_the_rest() {
        assert_eq!(pm1_sleep_value(0, 0), SLP_EN);
        // The NUC's S5 type is 7.
        assert_eq!(pm1_sleep_value(0, 7), 0x3C00);
        // SCI_EN (bit 0) stays; an old sleep type is replaced.
        assert_eq!(
            pm1_sleep_value(0x0001 | 5 << 10, 2),
            0x0001 | 2 << 10 | SLP_EN
        );
        // Only three bits of type exist.
        assert_eq!(pm1_sleep_value(0, 0xFF), 0x3C00);
    }

    #[test]
    fn poweroff_writes_the_s5_types_into_pm1a_and_pm1b() {
        let s5 = SleepType { a: 7, b: 3 };
        let mut f = fadt(None);
        assert_eq!(
            soft_off(Some(&f), Some(s5)),
            Ok(SoftOff {
                pm1a: (
                    Reg::Io {
                        port: 0x604,
                        bytes: 2
                    },
                    7
                ),
                pm1b: None,
            })
        );
        f.pm1b_cnt = Some(gas(AddressSpace::Io, 16, 0, 0x608));
        assert_eq!(
            soft_off(Some(&f), Some(s5)).unwrap().pm1b,
            Some((
                Reg::Io {
                    port: 0x608,
                    bytes: 2
                },
                3
            ))
        );
    }

    #[test]
    fn both_sleep_types_are_written_before_either_enable() {
        let a = Reg::Io {
            port: 0x604,
            bytes: 2,
        };
        let b = Reg::Io {
            port: 0x608,
            bytes: 2,
        };
        let off = SoftOff {
            pm1a: (a, 7),
            pm1b: Some((b, 3)),
        };
        // As Linux's acpi_hw_legacy_sleep: SLP_TYP to both, then SLP_EN.
        assert_eq!(
            soft_off_writes(&off, 0x0001, 0x0001),
            [
                (a, 0x0001 | 7 << 10),
                (b, 0x0001 | 3 << 10),
                (a, 0x0001 | 7 << 10 | SLP_EN),
                (b, 0x0001 | 3 << 10 | SLP_EN),
            ]
        );
        let only_a = SoftOff {
            pm1a: (a, 0),
            pm1b: None,
        };
        assert_eq!(soft_off_writes(&only_a, 0, 0), [(a, 0), (a, SLP_EN)]);
    }

    #[test]
    fn poweroff_says_why_it_cannot() {
        let s5 = Some(SleepType { a: 0, b: 0 });
        assert_eq!(soft_off(None, s5), Err("no FADT"));
        assert_eq!(
            soft_off(Some(&fadt(None)), None),
            Err("no \\_S5 in the DSDT")
        );
        let mut f = fadt(None);
        f.pm1a_cnt = None;
        assert_eq!(
            soft_off(Some(&f), s5),
            Err("no usable PM1a control register")
        );
    }

    #[test]
    fn the_test_exit_code_makes_qemu_exit_with_status_33() {
        assert_eq!((TEST_EXIT_CODE as u32) << 1 | 1, 33);
    }
}
