//! Device slots and endpoints as the controller keeps them: the commands
//! that change them (xHCI 4.6.5-4.6.10), checking input contexts like the
//! hardware does, and the output (device) contexts the driver reads.

use super::commands::{SLOT_NOT_ENABLED, SUCCESS};
use super::{Consumer, FakeXhci};
use crate::Setup;
use crate::Speed;
use crate::testing::device::Stall;
use crate::testing::hal::Dma;
use core::time::Duration;
use std::collections::BTreeMap;

pub const USB_TRANSACTION_ERROR: u32 = 4;
pub const ENDPOINT_NOT_ENABLED: u32 = 12;
pub const CONTEXT_STATE_ERROR: u32 = 19;
pub const STOPPED: u32 = 26;

// Slot states.
const ENABLED: u32 = 0;
const DEFAULT: u32 = 1;
const ADDRESSED: u32 = 2;
pub const CONFIGURED: u32 = 3;

// Endpoint states.
pub const RUNNING: u32 = 1;
pub const HALTED: u32 = 2;
pub const STOPPED_STATE: u32 = 3;
const EP_ERROR: u32 = 4;

/// Endpoint type 4: control.
const CONTROL: u32 = 4;

/// A slot: its state, its port, where its device context is.
#[derive(Clone, Debug, Default)]
pub struct FakeSlot {
    pub state: u32,
    pub port: u8,
    pub output: u64,
    pub context_entries: u32,
    pub endpoints: BTreeMap<usize, FakeEndpoint>,
    /// When the device got its SET_ADDRESS.
    pub addressed_at: Duration,
}

/// An endpoint as its context describes it, and its transfer ring.
#[derive(Clone, Debug)]
pub struct FakeEndpoint {
    pub state: u32,
    pub ring: Consumer,
    pub ep_type: u32,
    pub max_packet: u32,
    pub cerr: u32,
    pub interval: u32,
    pub mult: u32,
    pub max_burst: u32,
    pub average_trb_length: u32,
    pub max_esit_payload: u32,
    /// A TD is in progress, waiting for the device.
    pub busy: bool,
}

/// The default EP0 packet size of a speed (USB 2.0 5.5.3, USB 3.2 9.6.1).
fn default_max_packet0(speed: Speed) -> u32 {
    speed.default_max_packet0() as u32
}

fn speed_id(speed: Speed) -> u32 {
    match speed {
        Speed::Full => 1,
        Speed::Low => 2,
        Speed::High => 3,
        Speed::Super => 4,
        Speed::SuperPlus => 5,
    }
}

impl FakeXhci {
    pub(super) fn stride(&self) -> u64 {
        if self.config.context_64 { 64 } else { 32 }
    }

    pub fn slot(&self, slot: usize) -> Option<FakeSlot> {
        self.slots.get(slot).cloned().flatten()
    }

    pub fn endpoint(&self, slot: usize, dci: usize) -> Option<FakeEndpoint> {
        self.slot(slot)?.endpoints.get(&dci).cloned()
    }

    /// The input context at `trb`'s pointer, checked like the controller
    /// would before it reads it.
    fn input_context(&self, trb: &[u32; 4], dma: &Dma) -> u64 {
        let input = trb[0] as u64 | (trb[1] as u64) << 32;
        if input & 0xF != 0 || !dma.contains(input, 33 * self.stride() as usize) {
            panic!("fake xhci: input context at {input:#x} is not 33 contexts of allocated memory");
        }
        input
    }

    /// Parses the endpoint context at `at` and checks its ring.
    fn read_endpoint(&self, at: u64, dma: &Dma, what: &str) -> FakeEndpoint {
        let d = [dma.read32(at), dma.read32(at + 4), dma.read32(at + 16)];
        let dequeue = dma.read64(at + 8);
        let ring = Consumer {
            dequeue: dequeue & !0xF,
            cycle: dequeue & 1 != 0,
        };
        self.check_dequeue(dequeue, dma, what);
        FakeEndpoint {
            state: RUNNING,
            ring,
            ep_type: d[1] >> 3 & 7,
            max_packet: d[1] >> 16,
            cerr: d[1] >> 1 & 3,
            interval: d[0] >> 16 & 0xFF,
            mult: d[0] >> 8 & 3,
            max_burst: d[1] >> 8 & 0xFF,
            average_trb_length: d[2] & 0xFFFF,
            max_esit_payload: (d[0] >> 24) << 16 | d[2] >> 16,
            busy: false,
        }
    }

    /// A TR Dequeue Pointer with its DCS: 16-byte aligned, in allocated
    /// memory, and not pointing at a TRB its DCS already says is queued
    /// (this driver always hands over an empty ring position).
    pub(super) fn check_dequeue(&self, dequeue: u64, dma: &Dma, what: &str) {
        let ptr = dequeue & !0xF;
        if dequeue & 0xE != 0 || !dma.contains(ptr, 16) {
            panic!("fake xhci: {what}: bad TR dequeue pointer {dequeue:#x}");
        }
        let dcs = dequeue & 1 != 0;
        if (dma.read32(ptr + 12) & 1 != 0) == dcs {
            panic!(
                "fake xhci: {what}: DCS {} at {ptr:#x}, whose TRB it makes look queued",
                dcs as u8
            );
        }
    }

    /// Writes an endpoint's state into the fake and the device context.
    pub(super) fn set_ep_state(&mut self, slot: usize, dci: usize, state: u32, dma: &Dma) {
        let stride = self.stride();
        let Some(s) = self.slots[slot].as_mut() else {
            return;
        };
        if let Some(ep) = s.endpoints.get_mut(&dci) {
            ep.state = state;
            let at = s.output + dci as u64 * stride;
            dma.write32(at, dma.read32(at) & !7 | state);
        }
    }

    fn slot_state(&self, slot: usize) -> Option<u32> {
        self.slots
            .get(slot)
            .and_then(|s| s.as_ref())
            .map(|s| s.state)
    }

    /// Address Device (xHCI 4.6.5): the slot and EP0 contexts are checked
    /// and copied to the device context, and SET_ADDRESS goes to the
    /// device on the slot context's root port.
    pub(super) fn address_device(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        let Some(state) = self.slot_state(slot) else {
            return SLOT_NOT_ENABLED;
        };
        if !matches!(state, ENABLED | DEFAULT) {
            return CONTEXT_STATE_ERROR;
        }
        let stride = self.stride();
        let output = dma.read64(self.dcbaap + 8 * slot as u64);
        if output & 0x3F != 0 || !dma.contains(output, 32 * stride as usize) {
            panic!("fake xhci: DCBAA[{slot}] = {output:#x} is not a device context");
        }
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop != 0 || add != 0b11 {
            panic!("fake xhci: Address Device with drop {drop:#x} add {add:#x}, not A0 and A1");
        }
        let slot_ctx = input + stride;
        let d0 = dma.read32(slot_ctx);
        let port = (dma.read32(slot_ctx + 4) >> 16 & 0xFF) as u8;
        if d0 >> 27 == 0 {
            panic!("fake xhci: Address Device with context entries 0");
        }
        if port == 0 || port > self.config.ports {
            panic!("fake xhci: Address Device for root port {port}");
        }
        let Some(dev) = self.devices[port as usize - 1].clone() else {
            return USB_TRANSACTION_ERROR;
        };
        let speed = dev.borrow().speed();
        if self.portsc(port) & 2 == 0 {
            return USB_TRANSACTION_ERROR;
        }
        if d0 >> 20 & 0xF != speed_id(speed) {
            panic!(
                "fake xhci: slot context speed {} for a {speed} device",
                d0 >> 20 & 0xF
            );
        }
        let mut ep0 = self.read_endpoint(input + 2 * stride, dma, "Address Device EP0");
        if ep0.ep_type != CONTROL || ep0.max_packet != default_max_packet0(speed) {
            panic!(
                "fake xhci: EP0 context type {} max packet {} for a {speed} device",
                ep0.ep_type, ep0.max_packet
            );
        }
        if trb[3] & 1 << 9 == 0 {
            let set_address = Setup {
                request_type: 0,
                request: 5,
                value: slot as u16,
                index: 0,
                length: 0,
            };
            if dev.borrow_mut().control(set_address, &[]) != Some(Ok::<_, Stall>(Vec::new())) {
                return USB_TRANSACTION_ERROR;
            }
        }
        let new_state = if trb[3] & 1 << 9 != 0 {
            DEFAULT
        } else {
            ADDRESSED
        };
        for i in 0..4 {
            dma.write32(output + 4 * i, dma.read32(slot_ctx + 4 * i));
        }
        dma.write32(output + 12, slot as u32 | new_state << 27);
        for i in 0..5 {
            dma.write32(
                output + stride + 4 * i,
                dma.read32(input + 2 * stride + 4 * i),
            );
        }
        ep0.state = RUNNING;
        let s = self.slots[slot].as_mut().expect("slot checked above");
        (s.state, s.port, s.output, s.context_entries) = (new_state, port, output, d0 >> 27);
        s.addressed_at = self.now;
        s.endpoints.insert(1, ep0);
        self.set_ep_state(slot, 1, RUNNING, dma);
        SUCCESS
    }

    /// Evaluate Context (xHCI 4.6.7): for EP0 only its max packet size.
    pub(super) fn evaluate_context(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        match self.slot_state(slot) {
            None => return SLOT_NOT_ENABLED,
            Some(DEFAULT | ADDRESSED | CONFIGURED) => {}
            Some(_) => return CONTEXT_STATE_ERROR,
        }
        let stride = self.stride();
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop != 0 || add & !0b11 != 0 {
            panic!("fake xhci: Evaluate Context with drop {drop:#x} add {add:#x}");
        }
        if add & 0b10 != 0 {
            let mps = dma.read32(input + 2 * stride + 4) >> 16;
            if mps == 0 {
                panic!("fake xhci: Evaluate Context with EP0 max packet size 0");
            }
            let s = self.slots[slot].as_mut().expect("slot checked above");
            if let Some(ep0) = s.endpoints.get_mut(&1) {
                ep0.max_packet = mps;
            }
            let at = s.output + stride + 4;
            dma.write32(at, dma.read32(at) & 0xFFFF | mps << 16);
        }
        SUCCESS
    }

    /// The endpoint a Reset/Stop/Set TR Dequeue command names, or the
    /// completion code for a slot or endpoint that is not there.
    fn command_endpoint(&self, trb: &[u32; 4]) -> Result<(usize, usize, u32), u32> {
        let slot = (trb[3] >> 24) as usize;
        let dci = (trb[3] >> 16 & 0x1F) as usize;
        let s = self
            .slots
            .get(slot)
            .and_then(|s| s.as_ref())
            .ok_or(SLOT_NOT_ENABLED)?;
        let ep = s.endpoints.get(&dci).ok_or(ENDPOINT_NOT_ENABLED)?;
        Ok((slot, dci, ep.state))
    }

    /// Reset Endpoint (xHCI 4.6.8): Halted to Stopped.
    pub(super) fn reset_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        match self.command_endpoint(trb) {
            Ok((slot, dci, HALTED)) => {
                self.set_ep_state(slot, dci, STOPPED_STATE, dma);
                SUCCESS
            }
            Ok(_) => CONTEXT_STATE_ERROR,
            Err(code) => code,
        }
    }

    /// Stop Endpoint (xHCI 4.6.9): a TD in progress ends with a Stopped
    /// event, then the endpoint is Stopped.
    pub(super) fn stop_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let (slot, dci) = match self.command_endpoint(trb) {
            Ok((slot, dci, RUNNING)) => (slot, dci),
            Ok(_) => return CONTEXT_STATE_ERROR,
            Err(code) => return code,
        };
        let ep = &self.slots[slot]
            .as_ref()
            .expect("slot checked above")
            .endpoints[&dci];
        let (busy, dequeue) = (ep.busy, ep.ring.dequeue);
        if busy {
            self.post_transfer(slot, dci, dequeue, STOPPED, 0, dma);
        }
        self.set_ep_state(slot, dci, STOPPED_STATE, dma);
        if let Some(ep) = self.slots[slot]
            .as_mut()
            .and_then(|s| s.endpoints.get_mut(&dci))
        {
            ep.busy = false;
        }
        SUCCESS
    }

    /// Set TR Dequeue Pointer (xHCI 4.6.10): only on a Stopped endpoint.
    pub(super) fn set_tr_dequeue(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let (slot, dci) = match self.command_endpoint(trb) {
            Ok((slot, dci, STOPPED_STATE | EP_ERROR)) => (slot, dci),
            Ok(_) => return CONTEXT_STATE_ERROR,
            Err(code) => return code,
        };
        let dequeue = trb[0] as u64 | (trb[1] as u64) << 32;
        self.check_dequeue(dequeue, dma, "Set TR Dequeue Pointer");
        let stride = self.stride();
        let s = self.slots[slot].as_mut().expect("slot checked above");
        let ep = s.endpoints.get_mut(&dci).expect("endpoint checked above");
        ep.ring = Consumer {
            dequeue: dequeue & !0xF,
            cycle: dequeue & 1 != 0,
        };
        dma.write64(s.output + dci as u64 * stride + 8, dequeue);
        SUCCESS
    }

    /// Configure Endpoint (xHCI 4.6.6): the added endpoints' contexts are
    /// checked and copied, the dropped ones disabled.
    pub(super) fn configure_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        match self.slot_state(slot) {
            None => return SLOT_NOT_ENABLED,
            Some(ADDRESSED | CONFIGURED) => {}
            Some(_) => return CONTEXT_STATE_ERROR,
        }
        if trb[3] & 1 << 9 != 0 {
            panic!("fake xhci: Configure Endpoint with Deconfigure is not modelled");
        }
        let stride = self.stride();
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop & 0b11 != 0 || add & 0b11 != 0b01 {
            panic!(
                "fake xhci: Configure Endpoint with drop {drop:#x} add {add:#x}: A0 only, no EP0"
            );
        }
        let entries = dma.read32(input + stride) >> 27;
        let highest = 31 - add.leading_zeros();
        if entries < highest {
            panic!("fake xhci: context entries {entries} below DCI {highest}");
        }
        let mut added = Vec::new();
        for dci in 2..32usize {
            if add & 1 << dci == 0 {
                continue;
            }
            let at = input + (dci as u64 + 1) * stride;
            let ep = self.read_endpoint(at, dma, "Configure Endpoint");
            let types: &[u32] = if dci % 2 == 1 { &[5, 6, 7] } else { &[1, 2, 3] };
            if !types.contains(&ep.ep_type) || ep.max_packet == 0 || ep.interval > 15 {
                panic!("fake xhci: bad context for DCI {dci}: {ep:?}");
            }
            if ep.average_trb_length == 0 || (ep.ep_type % 4 == 3 && ep.max_esit_payload == 0) {
                panic!("fake xhci: DCI {dci} lacks its average TRB length or ESIT payload");
            }
            added.push((dci, at, ep));
        }
        let s = self.slots[slot].as_mut().expect("slot checked above");
        for dci in 2..32usize {
            if drop & 1 << dci != 0 {
                s.endpoints.remove(&dci);
            }
        }
        for (dci, at, ep) in added {
            let out = s.output + dci as u64 * stride;
            for i in 0..5 {
                dma.write32(out + 4 * i, dma.read32(at + 4 * i));
            }
            dma.write32(out, dma.read32(out) & !7 | RUNNING);
            s.endpoints.insert(dci, ep);
        }
        s.context_entries = entries;
        s.state = if s.endpoints.len() > 1 {
            CONFIGURED
        } else {
            ADDRESSED
        };
        let d0 = dma.read32(s.output);
        dma.write32(s.output, d0 & 0x07FF_FFFF | entries << 27);
        dma.write32(
            s.output + 12,
            dma.read32(s.output + 12) & 0xFF | s.state << 27,
        );
        SUCCESS
    }

    /// Disable Slot (xHCI 4.6.4): the slot and its endpoints are gone.
    pub(super) fn disable_slot(&mut self, slot: usize, dma: &Dma) -> u32 {
        match self.slots.get_mut(slot).and_then(Option::take) {
            Some(s) => {
                if s.output != 0 {
                    dma.write32(s.output + 12, 0);
                }
                SUCCESS
            }
            None => SLOT_NOT_ENABLED,
        }
    }
}
