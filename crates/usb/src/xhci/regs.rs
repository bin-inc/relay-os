//! Register offsets and bits (xHCI chapter 5) and [`Regs`], which knows
//! where the register blocks are and keeps every access inside the BAR.

use super::ControllerInfo;
use crate::{Hal, UsbError};

/// What a register of a powered-down or absent device reads as.
pub const ALL_ONES: u32 = 0xFFFF_FFFF;

// Capability registers (xHCI 5.3), from the BAR.
/// CAPLENGTH in bits 7:0, HCIVERSION in bits 31:16.
pub const CAPLENGTH: usize = 0x00;
pub const HCSPARAMS1: usize = 0x04;
pub const HCSPARAMS2: usize = 0x08;
pub const HCCPARAMS1: usize = 0x10;
pub const DBOFF: usize = 0x14;
pub const RTSOFF: usize = 0x18;

// HCCPARAMS1 bits.
pub const AC64: u32 = 1 << 0;
pub const CSZ: u32 = 1 << 2;
pub const PPC: u32 = 1 << 3;

// Operational registers (xHCI 5.4), from CAPLENGTH.
pub const USBCMD: usize = 0x00;
pub const USBSTS: usize = 0x04;
pub const PAGESIZE: usize = 0x08;
pub const CRCR: usize = 0x18;
pub const DCBAAP: usize = 0x30;
pub const CONFIG: usize = 0x38;
const PORT_BASE: usize = 0x400;
const PORT_STRIDE: usize = 0x10;

// PORTSC bits (xHCI 5.4.8). PED and the change bits are RW1C: writing 1
// to PED disables the port.
pub const CCS: u32 = 1 << 0;
pub const PED: u32 = 1 << 1;
pub const PR: u32 = 1 << 4;
pub const PLS_SHIFT: u32 = 5;
pub const PP: u32 = 1 << 9;
pub const SPEED_SHIFT: u32 = 10;
const PIC: u32 = 3 << 14;
pub const CSC: u32 = 1 << 17;
pub const PEC: u32 = 1 << 18;
pub const WRC: u32 = 1 << 19;
pub const OCC: u32 = 1 << 20;
pub const PRC: u32 = 1 << 21;
pub const PLC: u32 = 1 << 22;
pub const CEC: u32 = 1 << 23;
const WAKE: u32 = 7 << 25;
pub const WPR: u32 = 1 << 31;
pub const CHANGE_BITS: u32 = CSC | PEC | WRC | OCC | PRC | PLC | CEC;

/// The start of every PORTSC write: of what was read, only PP, PIC and
/// the wake bits, which keep their value when written back. Writing back
/// anything else would clear change bits by accident or disable the port.
pub fn portsc_neutral(portsc: u32) -> u32 {
    portsc & (PP | PIC | WAKE)
}

// USBCMD bits.
pub const RUN: u32 = 1 << 0;
pub const HCRST: u32 = 1 << 1;
pub const INTE: u32 = 1 << 2;

// USBSTS bits.
pub const HCH: u32 = 1 << 0;
pub const HSE: u32 = 1 << 2;
pub const CNR: u32 = 1 << 11;
pub const HCE: u32 = 1 << 12;

// CRCR bits.
pub const RCS: u32 = 1 << 0;
pub const CA: u32 = 1 << 2;
pub const CRR: u32 = 1 << 3;

// Interrupter registers (xHCI 5.5.2), from the runtime base; interrupter 0
// only.
const INTERRUPTER_0: usize = 0x20;
pub const IMAN: usize = 0x00;
pub const ERSTSZ: usize = 0x08;
pub const ERSTBA: usize = 0x10;
pub const ERDP: usize = 0x18;
/// IMAN: Interrupt Enable.
pub const IE: u32 = 1 << 1;
/// ERDP: Event Handler Busy (RW1C).
pub const EHB: u64 = 1 << 3;

/// Where the register blocks of one controller are.
#[derive(Debug)]
pub struct Regs {
    base: usize,
    len: usize,
    op: usize,
    runtime: usize,
    doorbells: usize,
}

impl Regs {
    /// Finds the register blocks of the controller mapped at `base` (`len`
    /// bytes) and checks that all of them lie inside the mapping.
    pub fn new<H: Hal>(hal: &H, base: usize, len: usize) -> Result<Regs, UsbError> {
        if len < 0x20 {
            return Err(UsbError::Unsupported("register space too small"));
        }
        let mut regs = Regs {
            base,
            len,
            op: 0,
            runtime: 0,
            doorbells: 0,
        };
        let caplength = regs.read(hal, CAPLENGTH);
        if caplength == ALL_ONES {
            return Err(UsbError::NotResponding);
        }
        let params = regs.read(hal, HCSPARAMS1);
        let slots = params as u8 as usize;
        let ports = (params >> 24) as usize;
        regs.op = (caplength & 0xFF) as usize;
        regs.runtime = (regs.read(hal, RTSOFF) & !0x1F) as usize;
        regs.doorbells = (regs.read(hal, DBOFF) & !0x3) as usize;
        let inside = |start: usize, size: usize| start.checked_add(size).is_some_and(|e| e <= len);
        if regs.op < 0x20
            || !inside(regs.op, PORT_BASE + ports * PORT_STRIDE)
            || !inside(regs.runtime, INTERRUPTER_0 + 0x20)
            || !inside(regs.doorbells, 4 * (slots + 1))
        {
            return Err(UsbError::Unsupported("registers outside the BAR"));
        }
        Ok(regs)
    }

    /// Reads the register at `offset` from the start of the BAR. Offsets
    /// come from this module's constants inside blocks `new` checked.
    pub fn read<H: Hal>(&self, hal: &H, offset: usize) -> u32 {
        assert!(self.inside(offset), "register {offset:#x} outside the BAR");
        // SAFETY: `base` is `len` bytes of registers from `map_mmio`, and
        // the offset is aligned and inside them (asserted).
        unsafe { hal.read32(self.base + offset) }
    }

    pub fn write<H: Hal>(&self, hal: &H, offset: usize, value: u32) {
        assert!(self.inside(offset), "register {offset:#x} outside the BAR");
        // SAFETY: as in `read`.
        unsafe { hal.write32(self.base + offset, value) }
    }

    /// Reads a register at an offset the hardware gave (the capability
    /// list): `None` if it is not inside the BAR.
    pub fn try_read<H: Hal>(&self, hal: &H, offset: usize) -> Option<u32> {
        self.inside(offset).then(|| self.read(hal, offset))
    }

    fn inside(&self, offset: usize) -> bool {
        offset.is_multiple_of(4) && offset.checked_add(4).is_some_and(|e| e <= self.len)
    }

    pub fn op_read<H: Hal>(&self, hal: &H, reg: usize) -> u32 {
        self.read(hal, self.op + reg)
    }

    pub fn op_write<H: Hal>(&self, hal: &H, reg: usize, value: u32) {
        self.write(hal, self.op + reg, value);
    }

    /// A 64-bit register as two dwords, low first (xHCI 5.1).
    pub fn op_write64<H: Hal>(&self, hal: &H, reg: usize, value: u64) {
        self.op_write(hal, reg, value as u32);
        self.op_write(hal, reg + 4, (value >> 32) as u32);
    }

    /// PORTSC of root port `port` (1-based).
    pub fn portsc<H: Hal>(&self, hal: &H, port: u8) -> u32 {
        self.op_read(hal, self.port_reg(port))
    }

    pub fn set_portsc<H: Hal>(&self, hal: &H, port: u8, value: u32) {
        self.op_write(hal, self.port_reg(port), value);
    }

    fn port_reg(&self, port: u8) -> usize {
        assert!(port >= 1, "port 0");
        PORT_BASE + (port as usize - 1) * PORT_STRIDE
    }

    /// A register of the primary interrupter.
    pub fn intr_read<H: Hal>(&self, hal: &H, reg: usize) -> u32 {
        self.read(hal, self.runtime + INTERRUPTER_0 + reg)
    }

    pub fn intr_write<H: Hal>(&self, hal: &H, reg: usize, value: u32) {
        self.write(hal, self.runtime + INTERRUPTER_0 + reg, value);
    }

    pub fn intr_write64<H: Hal>(&self, hal: &H, reg: usize, value: u64) {
        self.intr_write(hal, reg, value as u32);
        self.intr_write(hal, reg + 4, (value >> 32) as u32);
    }

    /// Rings doorbell `slot` (0 is the command ring) for `target` (xHCI
    /// 5.6).
    pub fn ring_doorbell<H: Hal>(&self, hal: &H, slot: u8, target: u32) {
        self.write(hal, self.doorbells + 4 * slot as usize, target);
    }
}

/// What the capability registers say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Params {
    pub version: u16,
    pub max_slots: u8,
    pub interrupters: u16,
    pub ports: u8,
    pub scratchpads: u16,
    pub context_size: usize,
    pub ac64: bool,
    pub ppc: bool,
    /// The extended capability list's offset in dwords (0: none).
    pub xecp: u16,
}

impl Params {
    pub fn read<H: Hal>(hal: &H, regs: &Regs) -> Params {
        let hcs1 = regs.read(hal, HCSPARAMS1);
        let hcc1 = regs.read(hal, HCCPARAMS1);
        Params {
            version: (regs.read(hal, CAPLENGTH) >> 16) as u16,
            max_slots: hcs1 as u8,
            interrupters: (hcs1 >> 8) as u16 & 0x7FF,
            ports: (hcs1 >> 24) as u8,
            scratchpads: scratchpads(regs.read(hal, HCSPARAMS2)),
            context_size: if hcc1 & CSZ != 0 { 64 } else { 32 },
            ac64: hcc1 & AC64 != 0,
            ppc: hcc1 & PPC != 0,
            xecp: (hcc1 >> 16) as u16,
        }
    }

    /// The summary, given how many ports each protocol has.
    pub fn info(&self, usb2_ports: u8, usb3_ports: u8) -> ControllerInfo {
        ControllerInfo {
            version: self.version,
            max_slots: self.max_slots,
            ports: self.ports,
            usb2_ports,
            usb3_ports,
            context_size: self.context_size,
            scratchpads: self.scratchpads,
        }
    }
}

/// Max Scratchpad Buffers from HCSPARAMS2: the high five bits are in bits
/// 25:21, the low five in bits 31:27 (xHCI 5.3.4).
pub fn scratchpads(hcsparams2: u32) -> u16 {
    let high = (hcsparams2 >> 21) & 0x1F;
    let low = (hcsparams2 >> 27) & 0x1F;
    (high << 5 | low) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn regs(config: FakeConfig) -> (FakeHal, Regs) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        (hal, regs)
    }

    #[test]
    fn the_qemu_parameters_are_read() {
        let (hal, regs) = regs(FakeConfig::basic());
        let p = Params::read(&hal, &regs);
        assert_eq!((p.version, p.max_slots, p.ports), (0x0100, 64, 8));
        assert_eq!((p.context_size, p.scratchpads), (32, 0));
        assert!(p.ac64 && !p.ppc);
        assert_eq!(p.xecp, 0x20 / 4);
    }

    #[test]
    fn the_intel_parameters_are_read() {
        let (hal, regs) = regs(FakeConfig::intel());
        let p = Params::read(&hal, &regs);
        assert_eq!((p.version, p.max_slots, p.ports), (0x0120, 64, 16));
        assert_eq!((p.context_size, p.scratchpads, p.interrupters), (64, 2, 8));
        assert!(p.ac64 && p.ppc);
    }

    #[test]
    fn scratchpad_counts_use_both_halves_of_the_field() {
        assert_eq!(scratchpads(2 << 27), 2);
        assert_eq!(scratchpads(1 << 21 | 1 << 27), 33);
        assert_eq!(scratchpads(0x1F << 21 | 0x1F << 27), 1023);
        let mut config = FakeConfig::intel();
        config.scratchpads = 33;
        let (hal, regs) = regs(config);
        assert_eq!(Params::read(&hal, &regs).scratchpads, 33);
    }

    #[test]
    fn the_register_blocks_are_found_from_the_capabilities() {
        let (hal, regs) = regs(FakeConfig::intel());
        assert_eq!(regs.op, 0x80);
        assert_eq!(regs.runtime, 0x2000);
        assert_eq!(regs.doorbells, 0x3000);
        // PAGESIZE is operational register 2: bit 0 means 4 KiB.
        assert_eq!(regs.op_read(&hal, PAGESIZE), 1);
    }

    #[test]
    fn registers_that_read_all_ones_mean_the_controller_is_not_there() {
        let mut config = FakeConfig::intel();
        config.all_ones = true;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        assert_eq!(
            Regs::new(&hal, base, FAKE_BAR_LEN).err(),
            Some(UsbError::NotResponding)
        );
    }

    #[test]
    fn register_blocks_outside_the_bar_are_refused() {
        let mut config = FakeConfig::intel();
        config.dboff = 0xFFF0;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        assert_eq!(
            Regs::new(&hal, base, FAKE_BAR_LEN).err(),
            Some(UsbError::Unsupported("registers outside the BAR"))
        );
        let base = hal.map_mmio(FAKE_BAR, 0x1000).unwrap();
        assert!(Regs::new(&hal, base, 0x1000).is_err(), "a BAR cut short");
    }

    #[test]
    fn a_neutral_portsc_write_keeps_only_power_indicator_and_wake_bits() {
        let all = 0xFFFF_FFFF;
        assert_eq!(portsc_neutral(all), PP | 3 << 14 | 7 << 25);
        assert_eq!(portsc_neutral(PED | CHANGE_BITS | PR | WPR | CCS), 0);
    }

    #[test]
    fn offsets_from_the_hardware_are_checked_against_the_bar() {
        let (hal, regs) = regs(FakeConfig::basic());
        assert_eq!(regs.try_read(&hal, FAKE_BAR_LEN), None);
        assert_eq!(regs.try_read(&hal, FAKE_BAR_LEN - 2), None);
        assert_eq!(regs.try_read(&hal, 0x21), None, "unaligned");
        assert!(regs.try_read(&hal, FAKE_BAR_LEN - 4).is_some());
    }
}
