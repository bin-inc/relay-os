//! The fake's register file: capability registers, the extended
//! capability list, and the operational and runtime registers.

use super::{ExtCap, FakeXhci};
use crate::testing::hal::Dma;
use core::time::Duration;

// Operational register offsets and bits, as the fake knows them.
const USBCMD: usize = 0x00;
const USBSTS: usize = 0x04;
const PAGESIZE: usize = 0x08;
const DNCTRL: usize = 0x14;
const CRCR: usize = 0x18;
const DCBAAP: usize = 0x30;
const CONFIG: usize = 0x38;
const PORTS: usize = 0x400;

pub const RUN: u32 = 1 << 0;
pub const HCRST: u32 = 1 << 1;
pub const HCH: u32 = 1 << 0;
pub const HSE: u32 = 1 << 2;
pub const CNR: u32 = 1 << 11;
/// USBSTS bits software clears by writing 1: HSE, EINT, PCD, SRE.
const USBSTS_RW1C: u32 = 1 << 2 | 1 << 3 | 1 << 4 | 1 << 10;
pub const CRR: u32 = 1 << 3;

pub const PP: u32 = 1 << 9;

// USB Legacy Support (xHCI 7.1).
pub const BIOS_OWNED: u32 = 1 << 16;
const OS_OWNED: u32 = 1 << 24;
/// USBLEGCTLSTS: SMI enables (RW), SMI status (RW1C), and the RsvdP bits
/// software must write back as it read them.
const SMI_ENABLES: u32 = 1 | 1 << 4 | 0x7 << 13;
const SMI_STATUS: u32 = 0x7 << 29;
const LEGCTL_RESERVED: u32 = 0xE | 0xFF << 5;

// Interrupter 0, from the runtime base.
const IMAN: usize = 0x20;
const IMOD: usize = 0x24;
const ERSTSZ: usize = 0x28;
const ERSTBA: usize = 0x30;
const ERDP: usize = 0x38;
pub const EHB: u32 = 1 << 3;

/// Sets the low or high half of a 64-bit register.
pub fn set_half(reg: &mut u64, high: bool, value: u32) {
    *reg = if high {
        *reg & 0xFFFF_FFFF | (value as u64) << 32
    } else {
        *reg & !0xFFFF_FFFF | value as u64
    };
}

fn half(reg: u64, high: bool) -> u32 {
    if high { (reg >> 32) as u32 } else { reg as u32 }
}

/// Which register block an offset falls in.
enum Block {
    Capability(usize),
    Operational(usize),
    Runtime(usize),
    Doorbell(usize),
    Extended(usize),
}

impl FakeXhci {
    fn block(&self, offset: usize) -> Block {
        let c = &self.config;
        let op = c.cap_length as usize;
        let rt = c.rtsoff as usize;
        let db = c.dboff as usize;
        if offset < 0x20 {
            Block::Capability(offset)
        } else if (op..op + PORTS + 0x10 * c.ports as usize).contains(&offset) {
            Block::Operational(offset - op)
        } else if (rt..rt + 0x20 * (c.interrupters as usize + 1)).contains(&offset) {
            Block::Runtime(offset - rt)
        } else if (db..db + 4 * (c.max_slots as usize + 1)).contains(&offset) {
            Block::Doorbell((offset - db) / 4)
        } else {
            Block::Extended(offset)
        }
    }

    /// Linux (xhci_reset): some Intel hosts hang if touched within 1 ms of
    /// HCRST, so the fake refuses it.
    fn check_access(&self, offset: usize) {
        if let Some(t) = self.hcrst_at
            && self.now < t + Duration::from_millis(1)
        {
            panic!(
                "fake xhci: register {offset:#x} accessed {:?} after HCRST",
                self.now - t
            );
        }
    }

    pub fn read(&mut self, offset: usize, dma: &Dma) -> u32 {
        self.check_access(offset);
        // Whatever happened since the last tick is visible now.
        self.flush_events(dma);
        self.highest_read = self.highest_read.max(offset);
        if self.config.all_ones {
            return 0xFFFF_FFFF;
        }
        match self.block(offset) {
            Block::Capability(o) => self.capability(o),
            Block::Operational(o) => self.op_read(o),
            Block::Runtime(o) => self.runtime_read(o),
            Block::Doorbell(_) => 0,
            Block::Extended(o) => {
                self.cap_reads += 1;
                self.extended_read(o)
            }
        }
    }

    pub fn write(&mut self, offset: usize, value: u32, dma: &Dma) {
        self.check_access(offset);
        let block = self.block(offset);
        // xHCI 5.4.2: no operational or runtime register may be written
        // until the controller is ready.
        if self.usbsts & CNR != 0 && !matches!(block, Block::Extended(_)) {
            panic!("fake xhci: register {offset:#x} written while USBSTS.CNR is 1");
        }
        match block {
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
            Block::Operational(o) => self.op_write(o, value, dma),
            Block::Runtime(o) => self.runtime_write(o, value, dma),
            Block::Doorbell(0) => self.command_doorbell(value),
            Block::Doorbell(slot) => self.slot_doorbell(slot, value, dma),
            Block::Extended(o) => self.extended_write(o, value),
        }
    }

    fn capability(&self, offset: usize) -> u32 {
        let c = &self.config;
        match offset {
            0x00 => c.cap_length as u32 | (c.version as u32) << 16,
            0x04 => c.max_slots as u32 | (c.interrupters as u32) << 8 | (c.ports as u32) << 24,
            // xHCI 5.3.4: Max Scratchpad Buffers high bits 25:21, low 31:27.
            0x08 => {
                let n = c.scratchpads as u32;
                (n >> 5 & 0x1F) << 21 | (n & 0x1F) << 27 | 0xF << 4
            }
            0x10 => {
                c.ac64 as u32
                    | (c.context_64 as u32) << 2
                    | (c.ppc as u32) << 3
                    | ((c.xecp / 4) as u32) << 16
            }
            0x14 => c.dboff,
            0x18 => c.rtsoff,
            _ => 0,
        }
    }

    fn extended_read(&self, offset: usize) -> u32 {
        let c = &self.config;
        if c.endless_caps && c.xecp != 0 && offset >= c.xecp {
            return 0xC0 | 1 << 8;
        }
        let Some(cap) = c
            .caps
            .iter()
            .find(|cap| (cap.offset..cap.offset + 16).contains(&offset))
        else {
            return 0;
        };
        let header = |id: u32| id | (cap.next as u32) << 8;
        match (&cap.cap, (offset - cap.offset) / 4) {
            (ExtCap::Legacy, 0) => header(1) | self.legacy[0],
            (ExtCap::Legacy, 1) => self.legacy[1],
            (ExtCap::Protocol { major, minor, .. }, 0) => {
                header(2) | (*minor as u32) << 16 | (*major as u32) << 24
            }
            (ExtCap::Protocol { .. }, 1) => c.protocol_name,
            (ExtCap::Protocol { first, count, .. }, 2) => *first as u32 | (*count as u32) << 8,
            (ExtCap::Other(id), 0) => header(*id as u32),
            _ => 0,
        }
    }

    fn extended_write(&mut self, offset: usize, value: u32) {
        self.cap_writes += 1;
        let legacy = self
            .config
            .caps
            .iter()
            .find(|cap| cap.cap == ExtCap::Legacy)
            .map(|cap| cap.offset);
        match legacy {
            Some(at) if offset == at => self.write_usblegsup(value),
            Some(at) if offset == at + 4 => self.write_usblegctlsts(value),
            _ => panic!("fake xhci: write to read-only capability space at {offset:#x}"),
        }
    }

    /// The OS asking for the controller starts the BIOS's release.
    fn write_usblegsup(&mut self, value: u32) {
        let asked = value & OS_OWNED != 0 && self.legacy[0] & OS_OWNED == 0;
        self.legacy[0] = value & (BIOS_OWNED | OS_OWNED);
        if asked
            && value & BIOS_OWNED != 0
            && let Some(delay) = self.config.bios_release
        {
            self.after(delay, |x, _| x.legacy[0] &= !BIOS_OWNED);
        }
    }

    fn write_usblegctlsts(&mut self, value: u32) {
        let old = self.legacy[1];
        if (old ^ value) & LEGCTL_RESERVED != 0 {
            panic!("fake xhci: USBLEGCTLSTS reserved bits changed ({old:#010x} -> {value:#010x})");
        }
        let status = old & SMI_STATUS & !value;
        self.legacy[1] = old & !(SMI_ENABLES | SMI_STATUS) | value & SMI_ENABLES | status;
    }

    fn write_usbcmd(&mut self, value: u32, dma: &Dma) {
        let was = self.usbcmd;
        if value & HCRST != 0 {
            if self.usbsts & HCH == 0 {
                panic!("fake xhci: HCRST set while the controller runs");
            }
            self.reset();
            return;
        }
        self.usbcmd = value;
        if was & RUN != 0 && value & RUN == 0 {
            if let Some(delay) = self.config.halt_time {
                self.after(delay, |x, _| x.usbsts |= HCH);
            }
        } else if was & RUN == 0 && value & RUN != 0 {
            self.start(dma);
        }
    }

    /// HCRST (xHCI 4.22.1): registers to their defaults, CNR set, HCRST
    /// and then CNR clearing after their delays.
    fn reset(&mut self) {
        self.usbcmd = HCRST;
        self.usbsts = HCH | CNR;
        self.dnctrl = 0;
        (self.crcr, self.command_ring, self.crr) = (0, None, false);
        self.dcbaap = 0;
        self.config_reg = 0;
        self.portsc.iter_mut().for_each(|p| *p = 0);
        self.power_on_ports();
        self.reconnect_ports();
        (self.iman, self.imod, self.erstsz, self.erstba) = (0, 0, 0, 0);
        (self.event_ring, self.erdp, self.ehb) = (None, 0, false);
        self.pending_events.clear();
        self.slots.iter_mut().for_each(|s| *s = None);
        self.active.clear();
        self.hcrst_at = Some(self.now);
        if let Some(delay) = self.config.reset_time {
            self.after(delay, |x, _| {
                x.usbcmd &= !HCRST;
                if let Some(cnr) = x.config.cnr_time {
                    x.after(cnr, |x, _| x.usbsts &= !CNR);
                }
            });
        }
    }

    fn op_read(&self, offset: usize) -> u32 {
        match offset {
            USBCMD => self.usbcmd,
            USBSTS => self.usbsts,
            PAGESIZE => self.config.page_size,
            DNCTRL => self.dnctrl,
            // xHCI 5.4.5: only CRR reads back; the pointer reads as 0.
            CRCR => {
                if self.crr {
                    CRR
                } else {
                    0
                }
            }
            0x1C => 0,
            0x30 | 0x34 => half(self.dcbaap, offset == 0x34),
            CONFIG => self.config_reg,
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 { self.portsc[port] } else { 0 }
            }
            _ => 0,
        }
    }

    fn op_write(&mut self, offset: usize, value: u32, dma: &Dma) {
        match offset {
            USBCMD => self.write_usbcmd(value, dma),
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
            DNCTRL => self.dnctrl = value,
            0x18 | 0x1C => self.write_crcr(offset == 0x1C, value, dma),
            0x30 | 0x34 => {
                self.forbid_while_running("DCBAAP");
                set_half(&mut self.dcbaap, offset == 0x34, value);
            }
            CONFIG => {
                self.forbid_while_running("CONFIG");
                if value & 0xFF > self.config.max_slots as u32 {
                    panic!("fake xhci: MaxSlotsEn {} above MaxSlots", value & 0xFF);
                }
                self.config_reg = value;
            }
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 {
                    self.write_portsc(port, value, dma);
                }
            }
            _ => panic!("fake xhci: write to read-only operational register {offset:#x}"),
        }
    }

    fn runtime_read(&self, offset: usize) -> u32 {
        match offset {
            IMAN => self.iman,
            IMOD => self.imod,
            ERSTSZ => self.erstsz,
            0x30 | 0x34 => half(self.erstba, offset == 0x34),
            ERDP => self.erdp as u32 | if self.ehb { EHB } else { 0 },
            0x3C => (self.erdp >> 32) as u32,
            _ => 0,
        }
    }

    fn runtime_write(&mut self, offset: usize, value: u32, dma: &Dma) {
        match offset {
            // IP (bit 0) is RW1C.
            IMAN => self.iman = value & 2 | self.iman & 1 & !value,
            IMOD => self.imod = value,
            ERSTSZ => self.erstsz = value & 0xFFFF,
            0x30 | 0x34 => self.write_erstba(offset == 0x34, value, dma),
            0x38 | 0x3C => self.write_erdp(offset == 0x3C, value),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;

    #[test]
    fn the_scratchpad_count_is_split_over_two_fields() {
        let mut config = FakeConfig::intel();
        config.scratchpads = 33;
        let x = FakeXhci::new(config);
        assert_eq!(x.capability(0x08) >> 21 & 0x1F, 1);
        assert_eq!(x.capability(0x08) >> 27, 1);
    }

    #[test]
    fn the_legacy_capability_reads_its_id_and_next_pointer() {
        let x = FakeXhci::new(FakeConfig::intel());
        assert_eq!(x.extended_read(0x8000) & 0xFFFF, 0x0801);
        assert_eq!(x.extended_read(0x8080) >> 16, 0x0310);
    }

    fn intel() -> FakeXhci {
        FakeXhci::new(FakeConfig::intel())
    }

    #[test]
    fn hcrst_takes_its_time_and_cnr_follows() {
        let dma = Dma::default();
        let mut x = intel();
        x.write(0x80, 0, &dma);
        x.advance_to(Duration::from_millis(1), &dma);
        assert_eq!(x.usbsts & HCH, HCH);
        x.write(0x80, HCRST, &dma);
        x.advance_to(Duration::from_millis(4), &dma);
        assert_eq!((x.usbcmd & HCRST, x.usbsts & CNR), (0, CNR));
        x.advance_to(Duration::from_millis(13), &dma);
        assert_eq!(x.usbsts & CNR, 0);
    }

    #[test]
    #[should_panic(expected = "after HCRST")]
    fn a_register_touched_right_after_hcrst_panics() {
        let dma = Dma::default();
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x40, HCRST, &dma);
        x.advance_to(Duration::from_micros(900), &dma);
        x.read(0x44, &dma);
    }

    #[test]
    #[should_panic(expected = "while USBSTS.CNR is 1")]
    fn writes_before_the_controller_is_ready_panic() {
        let dma = Dma::default();
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.config.reset_time = None;
        x.write(0x40, HCRST, &dma);
        x.advance_to(Duration::from_millis(2), &dma);
        x.write(0x40 + 0x38, 1, &dma);
    }

    #[test]
    #[should_panic(expected = "HCRST set while the controller runs")]
    fn a_reset_without_a_halt_panics() {
        intel().write(0x80, HCRST, &Dma::default());
    }

    #[test]
    fn the_bios_lets_go_after_its_delay() {
        let dma = Dma::default();
        let mut x = intel();
        x.write(0x8000, BIOS_OWNED | OS_OWNED, &dma);
        x.advance_to(Duration::from_millis(4), &dma);
        assert_eq!(x.legacy[0] & BIOS_OWNED, BIOS_OWNED);
        x.advance_to(Duration::from_millis(5), &dma);
        assert_eq!(x.legacy[0], OS_OWNED);
    }

    #[test]
    #[should_panic(expected = "reserved bits changed")]
    fn legacy_control_reserved_bits_must_be_kept() {
        intel().write(0x8004, 0, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "fake xhci: write to capability register")]
    fn capability_registers_are_read_only() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x04, 0, &Dma::default());
    }
}
