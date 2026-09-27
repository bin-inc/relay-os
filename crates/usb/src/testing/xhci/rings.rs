//! The rings as software sets them up: CRCR, the event ring segment table
//! and ERDP, with the checks the controller makes when it starts.

use super::regs::{HCH, set_half};
use super::{Consumer, EventRing, FakeXhci};
use crate::testing::hal::Dma;

impl FakeXhci {
    pub(super) fn forbid_while_running(&self, what: &str) {
        if self.running() {
            panic!("fake xhci: {what} written while running");
        }
    }

    /// CRCR (xHCI 5.4.5): the ring pointer takes effect with the high half.
    pub(super) fn write_crcr(&mut self, high: bool, value: u32, dma: &Dma) {
        if self.crr {
            panic!("fake xhci: CRCR written while the command ring runs");
        }
        set_half(&mut self.crcr, high, value);
        if high {
            let dequeue = self.crcr & !0x3F;
            if !dma.contains(dequeue, 16) {
                panic!("fake xhci: CRCR names unallocated memory at {dequeue:#x}");
            }
            self.command_ring = Some(Consumer {
                dequeue,
                cycle: self.crcr & 1 != 0,
            });
        }
    }

    /// ERSTBA (xHCI 4.9.4): writing it (the high half last) makes the
    /// controller read the segment table, so ERSTSZ must already be set.
    pub(super) fn write_erstba(&mut self, high: bool, value: u32, dma: &Dma) {
        self.forbid_while_running("ERSTBA");
        if self.erstsz == 0 {
            panic!("fake xhci: ERSTBA written before ERSTSZ");
        }
        set_half(&mut self.erstba, high, value);
        if !high {
            return;
        }
        if self.erstba & 0x3F != 0 {
            panic!("fake xhci: ERSTBA {:#x} not 64-byte aligned", self.erstba);
        }
        let base = dma.read64(self.erstba) & !0x3F;
        let size = (dma.read32(self.erstba + 8) & 0xFFFF) as usize;
        if !(16..=4096).contains(&size) || !dma.contains(base, size * 16) {
            panic!("fake xhci: bad event ring segment {base:#x} of {size} TRBs");
        }
        self.event_ring = Some(EventRing {
            base,
            size,
            enqueue: 0,
            cycle: true,
        });
        self.check_erdp();
    }

    /// ERDP: EHB (bit 3) is RW1C; the pointer must stay in the segment.
    pub(super) fn write_erdp(&mut self, high: bool, value: u32) {
        if high {
            set_half(&mut self.erdp, true, value);
            self.check_erdp();
        } else {
            if value & super::regs::EHB != 0 {
                self.ehb = false;
            }
            set_half(&mut self.erdp, false, value & !0xF);
        }
    }

    fn check_erdp(&self) {
        if let Some(r) = self.event_ring
            && !(r.base..r.base + 16 * r.size as u64).contains(&self.erdp)
        {
            panic!(
                "fake xhci: ERDP {:#x} outside the event ring segment",
                self.erdp
            );
        }
    }

    /// R/S = 1: what must be programmed first (xHCI 4.2), then running.
    pub(super) fn start(&mut self, dma: &Dma) {
        let slots = (self.config_reg & 0xFF) as usize;
        if slots == 0 {
            panic!("fake xhci: R/S set with CONFIG.MaxSlotsEn 0");
        }
        if self.dcbaap == 0
            || self.dcbaap & 0x3F != 0
            || !dma.contains(self.dcbaap, 8 * (slots + 1))
        {
            panic!("fake xhci: R/S set without a DCBAA ({:#x})", self.dcbaap);
        }
        if self.command_ring.is_none() {
            panic!("fake xhci: R/S set without CRCR");
        }
        if self.event_ring.is_none() {
            panic!("fake xhci: R/S set without an event ring");
        }
        self.scratchpad_pages = self.read_scratchpads(dma);
        if let Some(delay) = self.config.run_time {
            self.after(delay, |x, _| x.usbsts &= !HCH);
        }
    }

    /// DCBAA[0] names the scratchpad array when there are buffers (xHCI
    /// 4.20); each entry a page of its own.
    fn read_scratchpads(&self, dma: &Dma) -> Vec<u64> {
        let n = self.config.scratchpads as usize;
        let array = dma.read64(self.dcbaap);
        if n == 0 {
            return Vec::new();
        }
        if array == 0 || array & 0x3F != 0 || !dma.contains(array, 8 * n) {
            panic!("fake xhci: no scratchpad array for {n} buffers in DCBAA[0] ({array:#x})");
        }
        let pages: Vec<u64> = (0..n).map(|i| dma.read64(array + 8 * i as u64)).collect();
        for (i, &page) in pages.iter().enumerate() {
            if page & 0xFFF != 0 || !dma.contains(page, 4096) || pages[..i].contains(&page) {
                panic!("fake xhci: scratchpad buffer {i} at {page:#x} is not a page of its own");
            }
        }
        pages
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;

    #[test]
    #[should_panic(expected = "ERSTBA written before ERSTSZ")]
    fn the_segment_table_size_comes_first() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x1000 + 0x30, 0, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "R/S set with CONFIG.MaxSlotsEn 0")]
    fn running_needs_the_programming_done() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x40, 1, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "CONFIG written while running")]
    fn config_cannot_change_while_running() {
        let mut x = FakeXhci::new(FakeConfig::intel());
        x.write(0x80 + 0x38, 1, &Dma::default());
    }
}
