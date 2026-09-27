//! Transfer rings (xHCI 4.10, 4.11): slot doorbells, control TDs played
//! against the device on the slot's port, NAKs, STALLs, short packets,
//! babble, and what a disconnect does to transfers in progress.

use super::commands::{SUCCESS, TRANSFER_EVENT, trb_type};
use super::slots::{HALTED, RUNNING, STOPPED_STATE, USB_TRANSACTION_ERROR};
use super::{Consumer, FakeXhci};
use crate::Setup;
use crate::testing::device::Stall;
use crate::testing::hal::Dma;
use core::time::Duration;

const SETUP_STAGE: u32 = 2;
const DATA_STAGE: u32 = 3;
const STATUS_STAGE: u32 = 4;
const BABBLE: u32 = 3;
const STALL: u32 = 6;
const SHORT_PACKET: u32 = 13;
const ISP: u32 = 1 << 2;
const IOC: u32 = 1 << 5;
const IDT: u32 = 1 << 6;
const DIR_IN: u32 = 1 << 16;

/// How far a TD got.
enum Td {
    /// Finished (well or not); the next TD may follow.
    Done,
    /// Waiting for the device.
    Waiting,
    /// Software has not handed over all of its TRBs yet.
    Incomplete,
}

fn pointer(trb: &[u32; 4]) -> u64 {
    trb[0] as u64 | (trb[1] as u64) << 32
}

impl FakeXhci {
    /// Doorbell `slot` (xHCI 5.6): its target is the DCI to run.
    pub(super) fn slot_doorbell(&mut self, slot: usize, value: u32, dma: &Dma) {
        if !self.running() {
            panic!("fake xhci: doorbell rung while halted");
        }
        let dci = (value & 0xFF) as usize;
        if value >> 16 != 0 {
            panic!("fake xhci: doorbell with a stream ID");
        }
        let Some(s) = self.slots.get(slot).and_then(|s| s.as_ref()) else {
            panic!("fake xhci: doorbell for slot {slot}, which is not enabled");
        };
        let Some(ep) = s.endpoints.get(&dci) else {
            panic!("fake xhci: doorbell for DCI {dci} of slot {slot}, which is not configured");
        };
        match ep.state {
            // xHCI 4.8.3: a halted endpoint ignores its doorbell.
            HALTED => return,
            STOPPED_STATE => self.set_ep_state(slot, dci, RUNNING, dma),
            _ => {}
        }
        self.active.insert((slot, dci));
    }

    /// Runs every endpoint with work.
    pub(super) fn process_transfers(&mut self, dma: &Dma) {
        if !self.running() {
            return;
        }
        for (slot, dci) in self.active.clone() {
            if !self.process_endpoint(slot, dci, dma) {
                self.active.remove(&(slot, dci));
            }
        }
    }

    fn ring(&mut self, slot: usize, dci: usize) -> Option<&mut super::slots::FakeEndpoint> {
        self.slots.get_mut(slot)?.as_mut()?.endpoints.get_mut(&dci)
    }

    /// Runs TDs until the ring is empty or one waits; whether it waits.
    fn process_endpoint(&mut self, slot: usize, dci: usize, dma: &Dma) -> bool {
        loop {
            let Some(ep) = self.ring(slot, dci) else {
                return false;
            };
            if ep.state != RUNNING {
                return false;
            }
            if ep.busy && dci == 1 {
                // A control request the device never answers: it waits
                // for Stop Endpoint.
                return true;
            }
            let Some((_, trb)) = ep.ring.peek(dma) else {
                return false;
            };
            let td = match trb_type(&trb) {
                SETUP_STAGE if dci == 1 => self.control_td(slot, dma),
                t => panic!("fake xhci: TRB type {t} on the ring of slot {slot} DCI {dci}"),
            };
            match td {
                Td::Done => {}
                Td::Waiting => return true,
                Td::Incomplete => return false,
            }
        }
    }

    pub(super) fn post_transfer(
        &mut self,
        slot: usize,
        dci: usize,
        trb: u64,
        code: u32,
        residual: u32,
        dma: &Dma,
    ) {
        let event = [
            trb as u32,
            (trb >> 32) as u32,
            code << 24 | residual & 0xFF_FFFF,
            TRANSFER_EVENT << 10 | (dci as u32) << 16 | (slot as u32) << 24,
        ];
        self.post(event, dma);
    }

    /// An error on the TRB at `at` (xHCI 4.10.2): an event, and the
    /// endpoint halts with its dequeue pointer on that TRB.
    fn fail(&mut self, slot: usize, dci: usize, at: Consumer, code: u32, dma: &Dma) {
        self.post_transfer(slot, dci, at.dequeue, code, 0, dma);
        if let Some(ep) = self.ring(slot, dci) {
            ep.ring = at;
            ep.busy = false;
        }
        self.set_ep_state(slot, dci, HALTED, dma);
    }

    /// A control transfer (xHCI 4.11.2.2): Setup, an optional Data and a
    /// Status stage, each its own TD. The fake checks each stage's fields.
    fn control_td(&mut self, slot: usize, dma: &Dma) -> Td {
        let ep = self
            .ring(slot, 1)
            .expect("EP0 checked by the caller")
            .clone();
        let mut c = ep.ring;
        let Some((_, s)) = c.peek(dma) else {
            return Td::Incomplete;
        };
        if s[3] & IDT == 0 || s[2] & 0x1_FFFF != 8 {
            panic!("fake xhci: Setup Stage without IDT or not 8 bytes");
        }
        let setup = Setup {
            request_type: s[0] as u8,
            request: (s[0] >> 8) as u8,
            value: (s[0] >> 16) as u16,
            index: s[1] as u16,
            length: (s[1] >> 16) as u16,
        };
        let trt = s[3] >> 16 & 3;
        let want = match (setup.length, setup.is_in()) {
            (0, _) => 0,
            (_, true) => 3,
            (_, false) => 2,
        };
        if trt != want {
            panic!("fake xhci: Setup Stage TRT {trt} for {setup:?}");
        }
        c.advance();
        let after_setup = c;
        let data = if trt == 0 {
            None
        } else {
            let Some((at, d)) = c.peek(dma) else {
                return Td::Incomplete;
            };
            let len = d[2] & 0x1_FFFF;
            if trb_type(&d) != DATA_STAGE
                || (d[3] & DIR_IN != 0) != setup.is_in()
                || len != setup.length as u32
            {
                panic!("fake xhci: Data Stage {d:x?} does not match {setup:?}");
            }
            c.advance();
            Some((at, d))
        };
        let Some((status_at, st)) = c.peek(dma) else {
            return Td::Incomplete;
        };
        let status_in = data.is_none() || !setup.is_in();
        if trb_type(&st) != STATUS_STAGE || (st[3] & DIR_IN != 0) != status_in {
            panic!("fake xhci: Status Stage {st:x?} does not match {setup:?}");
        }
        c.advance();
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.devices.get(port as usize - 1).cloned().flatten() else {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
            return Td::Done;
        };
        let out = match data {
            Some((_, d)) if !setup.is_in() => dma.read_bytes(pointer(&d), setup.length as usize),
            _ => Vec::new(),
        };
        // USB 2.0 9.2.6.3: a device needs 2 ms after SET_ADDRESS before it
        // takes the next request.
        let recovered = self.slots[slot]
            .as_ref()
            .is_some_and(|s| self.now >= s.addressed_at + Duration::from_millis(2));
        if !recovered {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
            return Td::Done;
        }
        let answer = dev.borrow_mut().control(setup, &out);
        let mut stage = after_setup;
        match answer {
            None => {
                let ep = self.ring(slot, 1).expect("EP0");
                ep.ring = after_setup;
                ep.busy = true;
                return Td::Waiting;
            }
            Some(Err(Stall)) => {
                self.fail(slot, 1, stage, STALL, dma);
                return Td::Done;
            }
            Some(Ok(bytes)) => {
                if let Some((at, d)) = data.filter(|_| setup.is_in()) {
                    let len = setup.length as usize;
                    let dev_mps = dev.borrow().max_packet0().max(1) as usize;
                    let ctx_mps = ep.max_packet as usize;
                    let mut sent = 0;
                    let mut babble = bytes.len() > len;
                    for chunk in bytes.chunks(dev_mps) {
                        // A packet bigger than the context's max packet
                        // size is babble; a smaller one ends the stage.
                        if chunk.len() > ctx_mps {
                            babble = true;
                            break;
                        }
                        dma.write_bytes(pointer(&d) + sent as u64, chunk);
                        sent += chunk.len();
                        if chunk.len() < ctx_mps {
                            break;
                        }
                    }
                    if babble {
                        self.fail(slot, 1, stage, BABBLE, dma);
                        return Td::Done;
                    }
                    if sent < len && d[3] & ISP != 0 {
                        let code = if self.config.short_as_success {
                            SUCCESS
                        } else {
                            SHORT_PACKET
                        };
                        self.post_transfer(slot, 1, at, code, (len - sent) as u32, dma);
                    }
                    stage.advance();
                }
            }
        }
        if st[3] & IOC != 0 {
            self.post_transfer(slot, 1, status_at, SUCCESS, 0, dma);
        }
        if let Some(ep) = self.ring(slot, 1) {
            ep.ring = c;
        }
        Td::Done
    }

    /// The device on `port` went away: every TD in progress of its slot
    /// fails with a USB Transaction Error, as Intel controllers do.
    pub(super) fn device_gone(&mut self, port: u8, dma: &Dma) {
        let slots: Vec<usize> = (1..self.slots.len())
            .filter(|&s| self.slots[s].as_ref().is_some_and(|s| s.port == port))
            .collect();
        for slot in slots {
            let busy: Vec<(usize, Consumer)> = self.slots[slot]
                .as_ref()
                .map(|s| {
                    s.endpoints
                        .iter()
                        .filter(|(_, ep)| ep.busy)
                        .map(|(&dci, ep)| (dci, ep.ring))
                        .collect()
                })
                .unwrap_or_default();
            for (dci, at) in busy {
                self.fail(slot, dci, at, USB_TRANSACTION_ERROR, dma);
            }
        }
    }
}
