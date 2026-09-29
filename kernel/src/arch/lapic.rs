//! The local APIC: interrupt acceptance, end-of-interrupt and the timer.
//!
//! Firmware normally hands over in xAPIC mode (registers in a 4 KiB MMIO
//! page, usually at 0xFEE00000), but it may have switched to x2APIC mode,
//! where the same registers are MSRs and MMIO accesses do nothing. Both are
//! supported; the mode is read from IA32_APIC_BASE.

use spin::Once;
use x86_64::registers::model_specific::Msr;

pub const IA32_APIC_BASE: u32 = 0x1B;
const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_X2APIC: u64 = 1 << 10;

pub const VERSION: u32 = 0x30;
pub const TPR: u32 = 0x80;
pub const EOI: u32 = 0xB0;
pub const SVR: u32 = 0xF0;
/// First of the eight in-service registers (0x100, 0x110, ... 0x170).
pub const ISR0: u32 = 0x100;
pub const LVT_CMCI: u32 = 0x2F0;
pub const LVT_TIMER: u32 = 0x320;
pub const LVT_THERMAL: u32 = 0x330;
pub const LVT_PERF: u32 = 0x340;
pub const LVT_LINT0: u32 = 0x350;
pub const LVT_ERROR: u32 = 0x370;
pub const TIMER_INITIAL: u32 = 0x380;
pub const TIMER_CURRENT: u32 = 0x390;
pub const TIMER_DIVIDE: u32 = 0x3E0;

const MASKED: u32 = 1 << 16;
const PERIODIC: u32 = 1 << 17;
const SVR_ENABLE: u32 = 1 << 8;
/// TIMER_DIVIDE value for "divide by 16".
pub const DIVIDE_BY_16: u32 = 0b0011;

/// How the registers are reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Registers at this physical address.
    XApic {
        phys: u64,
    },
    X2Apic,
}

/// Decodes IA32_APIC_BASE. `None` if the APIC is globally disabled.
pub fn mode_from_base_msr(value: u64) -> Option<Mode> {
    if value & APIC_BASE_ENABLE == 0 {
        None
    } else if value & APIC_BASE_X2APIC != 0 {
        Some(Mode::X2Apic)
    } else {
        Some(Mode::XApic {
            phys: value & 0x000F_FFFF_FFFF_F000,
        })
    }
}

/// The MSR that holds xAPIC register `reg` in x2APIC mode.
pub fn x2apic_msr(reg: u32) -> u32 {
    0x800 + (reg >> 4)
}

/// Register access, so the programming sequences are tested on the host.
pub trait Regs {
    fn read(&mut self, reg: u32) -> u32;
    fn write(&mut self, reg: u32, value: u32);
}

/// Accepts interrupts: task priority 0; LINT0 (the legacy PIC), the error,
/// performance-counter, thermal and machine-check interrupts masked (the
/// last three only where the LAPIC has them); the timer masked until it is
/// started; spurious interrupts on `spurious_vector`. LINT1 (NMI) keeps the
/// firmware's setup. Interrupts the firmware accepted but never finished
/// would keep their priority class blocked, so they are ended with EOIs.
pub fn enable(regs: &mut impl Regs, spurious_vector: u8) {
    // Bits 16-23: the number of LVT entries minus one.
    let max_lvt = (regs.read(VERSION) >> 16) & 0xFF;
    regs.write(TPR, 0);
    regs.write(LVT_LINT0, MASKED);
    regs.write(LVT_ERROR, MASKED);
    regs.write(LVT_TIMER, MASKED);
    if max_lvt >= 4 {
        regs.write(LVT_PERF, MASKED);
    }
    if max_lvt >= 5 {
        regs.write(LVT_THERMAL, MASKED);
    }
    if max_lvt >= 6 {
        regs.write(LVT_CMCI, MASKED);
    }
    regs.write(SVR, SVR_ENABLE | spurious_vector as u32);
    for _ in 0..256 {
        if (0..8).all(|i| regs.read(ISR0 + 0x10 * i) == 0) {
            break;
        }
        regs.write(EOI, 0);
    }
}

/// Whether `vector` is in service: the LAPIC delivered it and waits for its
/// EOI.
pub fn in_service(regs: &mut impl Regs, vector: u8) -> bool {
    let reg = ISR0 + 0x10 * u32::from(vector / 32);
    regs.read(reg) & (1 << (vector % 32)) != 0
}

/// Starts the timer in periodic mode (divide by 16), interrupting on
/// `vector` every `count` timer ticks.
pub fn start_periodic(regs: &mut impl Regs, vector: u8, count: u32) {
    regs.write(TIMER_DIVIDE, DIVIDE_BY_16);
    regs.write(LVT_TIMER, PERIODIC | vector as u32);
    regs.write(TIMER_INITIAL, count);
}

/// Starts the timer counting down from `u32::MAX` (divide by 16, one-shot,
/// masked) so its rate can be measured with `elapsed`.
pub fn start_calibration(regs: &mut impl Regs) {
    regs.write(TIMER_DIVIDE, DIVIDE_BY_16);
    regs.write(LVT_TIMER, MASKED);
    regs.write(TIMER_INITIAL, u32::MAX);
}

/// Timer ticks since `start_calibration`.
pub fn elapsed(regs: &mut impl Regs) -> u32 {
    u32::MAX - regs.read(TIMER_CURRENT)
}

/// Initial count for `tick_hz` interrupts per second when the timer
/// (after its divider) counts at `timer_hz`.
pub fn periodic_count(timer_hz: u64, tick_hz: u64) -> u32 {
    (timer_hz / tick_hz).clamp(1, u32::MAX as u64) as u32
}

/// The LAPIC, once the timer driver has set it up.
pub static LAPIC: Once<Lapic> = Once::new();

/// Whether the LAPIC has `vector` in service (false before the timer
/// driver has set it up).
pub fn vector_in_service(vector: u8) -> bool {
    LAPIC
        .get()
        .copied()
        .is_some_and(|mut l| in_service(&mut l, vector))
}

/// Signals end of interrupt to the LAPIC (for every vector except the
/// spurious one).
pub fn eoi() {
    if let Some(mut l) = LAPIC.get().copied() {
        l.write(EOI, 0);
    }
}

/// The real registers.
#[derive(Clone, Copy, Debug)]
pub struct Lapic {
    mode: Mode,
    /// Virtual address of the xAPIC register page.
    mmio: usize,
}

impl Lapic {
    /// `mmio` is the mapped register page (unused in x2APIC mode).
    pub fn new(mode: Mode, mmio: usize) -> Lapic {
        Lapic { mode, mmio }
    }

    /// Reads IA32_APIC_BASE and sets the global enable bit if the firmware
    /// left it clear.
    pub fn detect() -> Mode {
        let mut msr = Msr::new(IA32_APIC_BASE);
        // SAFETY: IA32_APIC_BASE exists on every x86_64 CPU.
        let mut value = unsafe { msr.read() };
        if value & APIC_BASE_ENABLE == 0 {
            value |= APIC_BASE_ENABLE;
            unsafe { msr.write(value) };
        }
        mode_from_base_msr(value).expect("enable bit was just set")
    }
}

impl Regs for Lapic {
    fn read(&mut self, reg: u32) -> u32 {
        match self.mode {
            // SAFETY: `mmio` maps the register page uncached.
            Mode::XApic { .. } => unsafe {
                core::ptr::read_volatile((self.mmio + reg as usize) as *const u32)
            },
            Mode::X2Apic => unsafe { Msr::new(x2apic_msr(reg)).read() as u32 },
        }
    }

    fn write(&mut self, reg: u32, value: u32) {
        match self.mode {
            Mode::XApic { .. } => unsafe {
                core::ptr::write_volatile((self.mmio + reg as usize) as *mut u32, value)
            },
            Mode::X2Apic => unsafe { Msr::new(x2apic_msr(reg)).write(value as u64) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A LAPIC with `max_lvt`, some interrupts in service (an EOI ends the
    /// highest one) and a timer that has counted 1234 ticks.
    #[derive(Default)]
    struct Fake {
        max_lvt: u32,
        isr: [u32; 8],
        writes: Vec<(u32, u32)>,
    }

    impl Regs for Fake {
        fn read(&mut self, reg: u32) -> u32 {
            match reg {
                VERSION => (self.max_lvt << 16) | 0x15,
                TIMER_CURRENT => u32::MAX - 1234,
                r if (ISR0..ISR0 + 0x80).contains(&r) => self.isr[((r - ISR0) / 0x10) as usize],
                r => panic!("unexpected read of {r:#x}"),
            }
        }
        fn write(&mut self, reg: u32, value: u32) {
            if reg == EOI
                && let Some(i) = (0..8).rev().find(|&i| self.isr[i] != 0)
            {
                let top = 31 - self.isr[i].leading_zeros();
                self.isr[i] &= !(1 << top);
            }
            self.writes.push((reg, value));
        }
    }

    #[test]
    fn a_vector_is_in_service_by_its_isr_bit() {
        let mut r = Fake::default();
        r.isr[1] = 1 << 3; // vector 35
        r.isr[7] = 1 << 31; // vector 255
        assert!(in_service(&mut r, 35));
        assert!(in_service(&mut r, 255));
        for v in [0, 3, 34, 36, 67, 254] {
            assert!(!in_service(&mut r, v), "vector {v}");
        }
    }

    #[test]
    fn base_msr_selects_the_mode() {
        assert_eq!(
            mode_from_base_msr(0xFEE0_0900), // enabled, BSP
            Some(Mode::XApic { phys: 0xFEE0_0000 })
        );
        assert_eq!(mode_from_base_msr(0xFEE0_0D00), Some(Mode::X2Apic));
        assert_eq!(mode_from_base_msr(0xFEE0_0100), None, "globally disabled");
    }

    #[test]
    fn x2apic_registers_are_msrs_from_0x800() {
        assert_eq!(x2apic_msr(EOI), 0x80B);
        assert_eq!(x2apic_msr(SVR), 0x80F);
        assert_eq!(x2apic_msr(LVT_TIMER), 0x832);
        assert_eq!(x2apic_msr(TIMER_DIVIDE), 0x83E);
    }

    #[test]
    fn enable_masks_every_other_source_and_sets_the_spurious_vector() {
        let mut r = Fake {
            max_lvt: 6, // Alder Lake has CMCI
            ..Fake::default()
        };
        enable(&mut r, 0xFF);
        assert_eq!(
            r.writes,
            [
                (TPR, 0),
                (LVT_LINT0, 1 << 16),
                (LVT_ERROR, 1 << 16),
                (LVT_TIMER, 1 << 16),
                (LVT_PERF, 1 << 16),
                (LVT_THERMAL, 1 << 16),
                (LVT_CMCI, 1 << 16),
                (SVR, 0x1FF),
            ]
        );
    }

    #[test]
    fn lvt_entries_a_lapic_lacks_are_not_touched() {
        // x2APIC mode would fault on a missing register's MSR.
        let mut r = Fake {
            max_lvt: 4,
            ..Fake::default()
        };
        enable(&mut r, 0xFF);
        let regs: Vec<u32> = r.writes.iter().map(|&(reg, _)| reg).collect();
        assert!(regs.contains(&LVT_PERF));
        assert!(!regs.contains(&LVT_THERMAL));
        assert!(!regs.contains(&LVT_CMCI));
    }

    #[test]
    fn interrupts_left_in_service_are_ended() {
        let mut r = Fake {
            max_lvt: 6,
            ..Fake::default()
        };
        r.isr[1] = 1 << 7; // vector 0x27
        r.isr[3] = 1 << 8; // vector 0x68, say the firmware's timer
        enable(&mut r, 0xFF);
        assert_eq!(r.isr, [0; 8]);
        let eois = r.writes.iter().filter(|&&(reg, _)| reg == EOI).count();
        assert_eq!(eois, 2);
    }

    #[test]
    fn calibration_counts_down_from_the_top() {
        let mut r = Fake::default();
        start_calibration(&mut r);
        assert_eq!(
            r.writes,
            [
                (TIMER_DIVIDE, 0b0011),
                (LVT_TIMER, 1 << 16),
                (TIMER_INITIAL, u32::MAX)
            ]
        );
        assert_eq!(elapsed(&mut r), 1234);
    }

    #[test]
    fn periodic_timer_programming() {
        let mut r = Fake::default();
        start_periodic(&mut r, 48, 62_500);
        assert_eq!(
            r.writes,
            [
                (TIMER_DIVIDE, 0b0011),
                (LVT_TIMER, (1 << 17) | 48),
                (TIMER_INITIAL, 62_500)
            ]
        );
        // QEMU's timer runs at 1 GHz: 62.5 MHz after the divider.
        assert_eq!(periodic_count(62_500_000, 1000), 62_500);
        assert_eq!(
            periodic_count(500, 1000),
            1,
            "never zero (that stops the timer)"
        );
        assert_eq!(periodic_count(u64::MAX, 1), u32::MAX);
    }
}
