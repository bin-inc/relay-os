//! The fake's register file: capability registers, the extended
//! capability list, and the operational and runtime registers.

use super::{ExtCap, FakeXhci};
use crate::testing::hal::Dma;

// Operational register offsets and bits, as the fake knows them.
const USBCMD: usize = 0x00;
const USBSTS: usize = 0x04;
const PAGESIZE: usize = 0x08;
const DNCTRL: usize = 0x14;
const CRCR: usize = 0x18;
const DCBAAP: usize = 0x30;
const CONFIG: usize = 0x38;
const PORTS: usize = 0x400;

pub const HCH: u32 = 1 << 0;
/// USBSTS bits software clears by writing 1: HSE, EINT, PCD, SRE.
const USBSTS_RW1C: u32 = 1 << 2 | 1 << 3 | 1 << 4 | 1 << 10;
pub const CRR: u64 = 1 << 3;

pub const PP: u32 = 1 << 9;

// Interrupter 0, from the runtime base.
const IMAN: usize = 0x20;
const IMOD: usize = 0x24;
const ERSTSZ: usize = 0x28;
const ERSTBA: usize = 0x30;
const ERDP: usize = 0x38;

/// Sets the low or high half of a 64-bit register.
fn set_half(reg: &mut u64, high: bool, value: u32) {
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

    pub fn read(&mut self, offset: usize, _dma: &Dma) -> u32 {
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

    pub fn write(&mut self, offset: usize, value: u32, _dma: &Dma) {
        match self.block(offset) {
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
            Block::Operational(o) => self.op_write(o, value),
            Block::Runtime(o) => self.runtime_write(o, value),
            Block::Doorbell(_) => {}
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
        let legacy = self
            .config
            .caps
            .iter()
            .find(|cap| cap.cap == ExtCap::Legacy)
            .map(|cap| cap.offset);
        match legacy {
            Some(at) if offset == at => self.legacy[0] = value & !0xFFFF,
            Some(at) if offset == at + 4 => self.legacy[1] = value,
            _ => panic!("fake xhci: write to read-only capability space at {offset:#x}"),
        }
    }

    fn op_read(&self, offset: usize) -> u32 {
        match offset {
            USBCMD => self.usbcmd,
            USBSTS => self.usbsts,
            PAGESIZE => self.config.page_size,
            DNCTRL => self.dnctrl,
            // xHCI 5.4.5: only CRR reads back; the pointer reads as 0.
            CRCR => (self.crcr & CRR) as u32,
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

    fn op_write(&mut self, offset: usize, value: u32) {
        match offset {
            USBCMD => self.usbcmd = value,
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
            DNCTRL => self.dnctrl = value,
            0x18 | 0x1C => set_half(&mut self.crcr, offset == 0x1C, value),
            0x30 | 0x34 => set_half(&mut self.dcbaap, offset == 0x34, value),
            CONFIG => self.config_reg = value,
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 {
                    self.portsc[port] = value;
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
            0x38 | 0x3C => half(self.erdp, offset == 0x3C),
            _ => 0,
        }
    }

    fn runtime_write(&mut self, offset: usize, value: u32) {
        match offset {
            IMAN => self.iman = value,
            IMOD => self.imod = value,
            ERSTSZ => self.erstsz = value & 0xFFFF,
            0x30 | 0x34 => set_half(&mut self.erstba, offset == 0x34, value),
            0x38 | 0x3C => set_half(&mut self.erdp, offset == 0x3C, value),
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

    #[test]
    #[should_panic(expected = "fake xhci: write to capability register")]
    fn capability_registers_are_read_only() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x04, 0, &Dma::default());
    }
}
