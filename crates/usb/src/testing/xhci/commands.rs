//! The command ring (xHCI 4.6): the fake consumes TRBs while their cycle
//! bit matches its cycle state, follows Link TRBs, executes each command
//! and posts its completion; CRCR.CA aborts. Commands are decoded with the
//! fake's own field layouts.

use super::slots::FakeSlot;
use super::{Consumer, FakeXhci};
use crate::testing::hal::Dma;

// TRB types (xHCI table 6-91).
pub const LINK: u32 = 6;
pub const ENABLE_SLOT: u32 = 9;
pub const DISABLE_SLOT: u32 = 10;
pub const ADDRESS_DEVICE: u32 = 11;
pub const EVALUATE_CONTEXT: u32 = 13;
pub const RESET_ENDPOINT: u32 = 14;
pub const STOP_ENDPOINT: u32 = 15;
pub const SET_TR_DEQUEUE: u32 = 16;
pub const NO_OP_COMMAND: u32 = 23;
pub const TRANSFER_EVENT: u32 = 32;
pub const COMMAND_COMPLETION: u32 = 33;
pub const PORT_STATUS_CHANGE: u32 = 34;

// Completion codes.
pub const SUCCESS: u32 = 1;
pub const NO_SLOTS: u32 = 9;
pub const SLOT_NOT_ENABLED: u32 = 11;
pub const COMMAND_RING_STOPPED: u32 = 24;
pub const COMMAND_ABORTED: u32 = 25;

pub fn trb_type(trb: &[u32; 4]) -> u32 {
    trb[3] >> 10 & 0x3F
}

pub fn slot_of(trb: &[u32; 4]) -> usize {
    (trb[3] >> 24) as usize
}

/// A command the fake executed: its TRB's address and type, and the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Executed {
    pub addr: u64,
    pub kind: u32,
    pub slot: usize,
}

impl Consumer {
    /// The TRB at the dequeue pointer if software has handed it over,
    /// following Link TRBs (toggling the cycle state where they say so).
    pub fn peek(&mut self, dma: &Dma) -> Option<(u64, [u32; 4])> {
        for _ in 0..2 {
            let at = self.dequeue;
            let trb = [
                dma.read32(at),
                dma.read32(at + 4),
                dma.read32(at + 8),
                dma.read32(at + 12),
            ];
            if (trb[3] & 1 != 0) != self.cycle {
                return None;
            }
            if trb_type(&trb) != LINK {
                return Some((at, trb));
            }
            self.dequeue = (trb[0] as u64 | (trb[1] as u64) << 32) & !0xF;
            if trb[3] & 2 != 0 {
                self.cycle = !self.cycle;
            }
        }
        panic!("fake xhci: a Link TRB leads to a Link TRB");
    }

    pub fn advance(&mut self) {
        self.dequeue += 16;
    }
}

impl FakeXhci {
    /// Doorbell 0 (xHCI 5.6): target 0 runs the command ring.
    pub(super) fn command_doorbell(&mut self, target: u32) {
        if target != 0 {
            panic!("fake xhci: doorbell 0 rung with target {target}");
        }
        if !self.running() {
            panic!("fake xhci: doorbell rung while halted");
        }
        self.crr = true;
    }

    /// Executes every command handed over, unless one hangs.
    pub(super) fn process_commands(&mut self, dma: &Dma) {
        if !self.crr || !self.running() {
            return;
        }
        loop {
            let Some(ring) = self.command_ring.as_mut() else {
                return;
            };
            let Some((addr, trb)) = ring.peek(dma) else {
                return;
            };
            if self.config.hang_command == Some(trb_type(&trb)) {
                self.hung = Some(addr);
                return;
            }
            ring.advance();
            self.execute(addr, trb, dma);
        }
    }

    fn execute(&mut self, addr: u64, trb: [u32; 4], dma: &Dma) {
        let kind = trb_type(&trb);
        let (code, slot) = match kind {
            NO_OP_COMMAND => (SUCCESS, 0),
            ENABLE_SLOT => self.enable_slot(),
            DISABLE_SLOT => (self.disable_slot(slot_of(&trb), dma), slot_of(&trb)),
            ADDRESS_DEVICE => (self.address_device(&trb, dma), slot_of(&trb)),
            EVALUATE_CONTEXT => (self.evaluate_context(&trb, dma), slot_of(&trb)),
            RESET_ENDPOINT => (self.reset_endpoint(&trb, dma), slot_of(&trb)),
            STOP_ENDPOINT => (self.stop_endpoint(&trb, dma), slot_of(&trb)),
            SET_TR_DEQUEUE => (self.set_tr_dequeue(&trb, dma), slot_of(&trb)),
            _ => panic!("fake xhci: TRB type {kind} on the command ring"),
        };
        self.executed.push(Executed { addr, kind, slot });
        self.post_completion(addr, code, slot, dma);
    }

    pub fn post_completion(&mut self, addr: u64, code: u32, slot: usize, dma: &Dma) {
        self.post(
            [
                addr as u32,
                (addr >> 32) as u32,
                code << 24,
                COMMAND_COMPLETION << 10 | (slot as u32) << 24,
            ],
            dma,
        );
    }

    /// The lowest free slot up to CONFIG.MaxSlotsEn (xHCI 4.6.3).
    fn enable_slot(&mut self) -> (u32, usize) {
        let enabled = (self.config_reg & 0xFF) as usize;
        match (1..=enabled).find(|&s| self.slots[s].is_none()) {
            Some(s) => {
                self.slots[s] = Some(FakeSlot::default());
                (SUCCESS, s)
            }
            None => (NO_SLOTS, 0),
        }
    }

    /// CRCR.CA (xHCI 4.6.1.2): the command in progress gets a Command
    /// Aborted completion and is consumed, then the ring stops with a
    /// Command Ring Stopped event naming the next TRB. With
    /// `abort_keeps_dequeue` the ring stops on the aborted command instead,
    /// so whatever software leaves there runs when the ring restarts.
    pub(super) fn abort_commands(&mut self, dma: &Dma) {
        if self.config.abort_never_completes || !self.crr {
            return;
        }
        self.aborts += 1;
        self.crr = false;
        if let Some(addr) = self.hung.take()
            && !self.config.abort_keeps_dequeue
        {
            self.post_completion(addr, COMMAND_ABORTED, 0, dma);
            if let Some(ring) = self.command_ring.as_mut() {
                ring.advance();
                // Past a Link TRB, if the next TRB is one.
                let _ = ring.peek(dma);
            }
        }
        let dequeue = self.command_ring.map_or(0, |r| r.dequeue);
        self.post_completion(dequeue, COMMAND_RING_STOPPED, 0, dma);
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;
    use crate::testing::FakeHal;
    use crate::{DmaBuf, Hal};

    fn put(buf: &DmaBuf, index: usize, trb: [u32; 4]) {
        for (i, d) in trb.iter().enumerate() {
            buf.write32(16 * index + 4 * i, *d);
        }
    }

    #[test]
    fn the_consumer_follows_links_and_toggles_its_cycle() {
        let hal = FakeHal::with_controller(FakeConfig::basic());
        let buf = hal.alloc_dma(4096, 64).unwrap();
        put(&buf, 0, [0, 0, 0, NO_OP_COMMAND << 10 | 1]);
        put(
            &buf,
            1,
            [
                buf.phys() as u32,
                (buf.phys() >> 32) as u32,
                0,
                LINK << 10 | 2 | 1,
            ],
        );
        put(&buf, 2, [0, 0, 0, NO_OP_COMMAND << 10 | 1]);
        let mut c = Consumer {
            dequeue: buf.phys(),
            cycle: true,
        };
        hal.act(|_, dma| {
            assert_eq!(c.peek(dma).map(|(a, _)| a), Some(buf.phys()));
            c.advance();
            // The link toggles to cycle 0, and TRB 0 still has cycle 1.
            assert_eq!(c.peek(dma), None);
            assert_eq!((c.dequeue, c.cycle), (buf.phys(), false));
        });
        hal.free_dma(buf);
    }

    #[test]
    #[should_panic(expected = "doorbell rung while halted")]
    fn a_halted_controller_takes_no_doorbell() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.command_doorbell(0);
    }
}
