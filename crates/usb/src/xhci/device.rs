//! Devices (spec §6.2 steps 1-5): enumeration on a root port, and
//! detaching. Each step logs why it failed; a failed attach disables its
//! slot and frees everything it allocated, so a misbehaving device costs
//! nothing else.

use super::context::{CONTROL, EndpointContext, Input, Output, SlotContext, speed_id};
use super::regs::{CCS, CSC};
use super::transfer::DATA_BUFFER_SIZE;
use super::trb::Trb;
use super::{Device, Slot, Xhci};
use crate::descriptor::{
    self, CONFIGURATION, DEVICE, DEVICE_LEN, DeviceDescriptor, EndpointKind, Interface,
};
use crate::{Hal, Setup, Speed, UsbError};
use alloc::string::String;
use alloc::vec;
use core::fmt::Write;
use core::time::Duration;

/// A connection must last this long before the port is reset (USB 2.0
/// 7.1.7.3: 100 ms of debounce).
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How often the debounce looks at the port (Linux: 25 ms).
const DEBOUNCE_STEP: Duration = Duration::from_millis(25);
/// How long a connection may bounce before the debounce gives up (Linux:
/// 2 s).
const DEBOUNCE_LIMIT: Duration = Duration::from_secs(2);
/// How long a device gets after SET_ADDRESS before its next request.
const SET_ADDRESS_RECOVERY: Duration = Duration::from_millis(10);
/// Input control context flags: A0 is the slot context, A1 EP0.
pub(super) const A0: u32 = 1 << 0;
const A1: u32 = 1 << 1;

/// EP0's context: control, 3 retries, `max_packet`, its ring, and an
/// average TRB length of 8 (xHCI 4.14.1.1: setup packets).
fn ep0_context(max_packet: u16, dequeue: u64) -> EndpointContext {
    EndpointContext {
        ep_type: CONTROL,
        cerr: 3,
        max_packet,
        dequeue,
        average_trb_length: 8,
        ..EndpointContext::default()
    }
}

pub(super) fn kind_name(kind: EndpointKind) -> &'static str {
    match kind {
        EndpointKind::Control => "control",
        EndpointKind::Isochronous => "isochronous",
        EndpointKind::Bulk => "bulk",
        EndpointKind::Interrupt => "interrupt",
    }
}

/// "interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 10"
fn describe(interface: &Interface) -> String {
    let mut line = String::new();
    let i = interface;
    let _ = write!(
        line,
        "interface {} class {}/{}/{}",
        i.number, i.class, i.subclass, i.protocol
    );
    for (n, e) in i.endpoints.iter().enumerate() {
        let sep = if n == 0 { ", endpoints " } else { ", " };
        let _ = write!(
            line,
            "{sep}{:#04x} {} {} bytes interval {}",
            e.address,
            kind_name(e.kind),
            e.packet_size(),
            e.interval
        );
    }
    line
}

impl<H: Hal> Xhci<H> {
    /// The slot of the device attached on `port`, if any.
    pub fn slot_of_port(&self, port: u8) -> Option<u8> {
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.port == port))
            .map(|slot| slot as u8)
    }

    pub(super) fn connected(&self, port: u8) -> bool {
        self.regs.portsc(&self.hal, port) & CCS != 0
    }

    /// Sets up the device on `port` (spec §6.2 steps 1-5): debounce (the
    /// connection must be stable for 100 ms), port reset, Enable Slot,
    /// Address Device, device descriptor (fixing EP0's packet size),
    /// configuration descriptor. On an error the slot is disabled and
    /// everything allocated for it freed.
    pub fn attach(&mut self, port: u8) -> Result<Device, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        if port == 0 || port > self.info.ports {
            return Err(UsbError::Unsupported("no such port"));
        }
        if self.ports[port as usize - 1].is_none() {
            return Err(UsbError::Unsupported("port without a supported protocol"));
        }
        if let Some(slot) = self.slot_of_port(port) {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: already attached as slot {slot}"
            );
            return Err(UsbError::Unsupported("port already attached"));
        }
        self.debounce(port)?;
        let speed = self.reset_port(port)?;
        let slot = self.enable_slot(port)?;
        match self.enumerate(slot, port, speed) {
            Ok(device) => Ok(device),
            Err(e) => {
                let e = if self.connected(port) {
                    e
                } else {
                    UsbError::Disconnected
                };
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: attach failed ({e}), slot {slot} released"
                );
                self.release_slot(slot);
                Err(e)
            }
        }
    }

    /// Waits until the connection on `port` has been stable for 100 ms
    /// (USB 2.0 7.1.7.3), as Linux's `hub_port_debounce` does: it looks
    /// every 25 ms, and a change of CCS, or a CSC, starts the 100 ms again.
    /// It clears CSC, so the next `port_changes` does not take the bounce
    /// for a replug; a change after the debounce sets it again. A
    /// connection stable for 100 ms is `Ok`, none for 100 ms is
    /// `Disconnected`. After 2 s of bouncing it gives up: `Disconnected` if
    /// the last look showed no connection (the next connect is a change
    /// again), `Timeout` if it did (the host tries again).
    fn debounce(&self, port: u8) -> Result<(), UsbError> {
        let start = self.hal.now();
        let (mut since, mut connected) = (start, None);
        let stable = loop {
            let sc = self.regs.portsc(&self.hal, port);
            let now = self.hal.now();
            let ccs = sc & CCS != 0;
            if sc & CSC != 0 || connected != Some(ccs) {
                self.clear_changes(port, sc, CSC);
                (since, connected) = (now, Some(ccs));
            } else if now - since >= DEBOUNCE {
                break true;
            }
            if now - start >= DEBOUNCE_LIMIT {
                break false;
            }
            self.hal.sleep(DEBOUNCE_STEP);
        };
        let waited = (self.hal.now() - start).as_millis();
        match (connected, stable) {
            (Some(true), true) => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: connection stable after {waited} ms"
                );
                Ok(())
            }
            (Some(true), false) => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: connection not stable after {waited} ms"
                );
                Err(UsbError::Timeout)
            }
            _ => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: not connected after debounce"
                );
                Err(UsbError::Disconnected)
            }
        }
    }

    fn enable_slot(&mut self, port: u8) -> Result<u8, UsbError> {
        let done = self
            .command(Trb::enable_slot())
            .inspect_err(|e| xlog!(&self.hal, &self.name, "port {port}: Enable Slot: {e}"))?;
        let slot = done.slot;
        if slot == 0 || slot > self.info.max_slots || self.slots[slot as usize].is_some() {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: Enable Slot gave slot {slot}"
            );
            return Err(UsbError::Unsupported("bad slot ID from Enable Slot"));
        }
        Ok(slot)
    }

    /// Logs a failed step of the attach on `port`.
    fn step<T>(&self, port: u8, what: &str, r: Result<T, UsbError>) -> Result<T, UsbError> {
        r.inspect_err(|e| xlog!(&self.hal, &self.name, "port {port}: {what}: {e}"))
    }

    /// GET_DESCRIPTOR into `buf`; the number of bytes that came.
    fn get_descriptor(
        &mut self,
        port: u8,
        slot: u8,
        (kind, what): (u8, &str),
        buf: &mut [u8],
    ) -> Result<usize, UsbError> {
        let setup = Setup::get_descriptor(kind, 0, buf.len() as u16);
        let r = self.control_transfer(slot, setup, buf);
        self.step(port, what, r)
    }

    fn enumerate(&mut self, slot: u8, port: u8, speed: Speed) -> Result<Device, UsbError> {
        let stride = self.info.context_size;
        let s = Slot::new(&self.hal, port, speed, stride);
        let s = self.step(port, "slot memory", s)?;
        self.dcbaa.write64(8 * slot as usize, s.output.phys());
        self.slots[slot as usize] = Some(s);
        let r = self.address_device(slot);
        self.step(port, "Address Device", r)?;
        let mut prefix = [0; 8];
        let n = self.get_descriptor(
            port,
            slot,
            (DEVICE, "device descriptor (8 bytes)"),
            &mut prefix,
        )?;
        let max_packet0 = self.step(
            port,
            "EP0 packet size",
            descriptor::max_packet0(&prefix[..n], speed),
        )?;
        if max_packet0 != speed.default_max_packet0() {
            let r = self.set_max_packet0(slot, max_packet0);
            self.step(port, "Evaluate Context", r)?;
        }
        let mut raw = [0; DEVICE_LEN];
        let n = self.get_descriptor(port, slot, (DEVICE, "device descriptor"), &mut raw)?;
        let desc = self.step(
            port,
            "device descriptor",
            DeviceDescriptor::parse(&raw[..n]),
        )?;
        if desc.configurations == 0 {
            return self.step(
                port,
                "device descriptor",
                Err(UsbError::Unsupported("no configuration")),
            );
        }
        let mut header = [0; descriptor::CONFIGURATION_LEN];
        let what = (CONFIGURATION, "configuration descriptor");
        let n = self.get_descriptor(port, slot, what, &mut header)?;
        let total = self.step(port, what.1, descriptor::configuration_length(&header[..n]))?;
        if total as usize > DATA_BUFFER_SIZE {
            let e = Err(UsbError::BadDescriptor("configuration too large"));
            return self.step(port, what.1, e);
        }
        let mut raw = vec![0; total as usize];
        let n = self.get_descriptor(port, slot, what, &mut raw)?;
        let configuration = self.step(port, what.1, descriptor::parse_configuration(&raw[..n]))?;
        let plural = if desc.configurations == 1 { "" } else { "s" };
        // The USB address the controller gave the device (xHCI 6.2.2).
        let address = self.slots[slot as usize]
            .as_ref()
            .map_or(0, |s| Output::new(&s.output, stride).slot().address);
        xlog!(
            &self.hal,
            &self.name,
            "port {port}: slot {slot} at address {address}, {:04x}:{:04x} USB {:x}.{:02x}, \
             ep0 {max_packet0} bytes, {} configuration{plural}",
            desc.vendor,
            desc.product,
            desc.usb_version >> 8,
            desc.usb_version & 0xFF,
            desc.configurations
        );
        for interface in &configuration.interfaces {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: {}",
                describe(interface)
            );
        }
        Ok(Device {
            slot,
            port,
            speed,
            descriptor: desc,
            configuration,
        })
    }

    /// Address Device with BSR = 0 (xHCI 4.3.3): the slot context (speed,
    /// one context entry, root port) and EP0 with the speed's default
    /// packet size; the controller sends SET_ADDRESS.
    fn address_device(&mut self, slot: u8) -> Result<(), UsbError> {
        let s = self.slots[slot as usize]
            .as_ref()
            .ok_or(UsbError::Disconnected)?;
        let input = Input::new(&s.input, self.info.context_size);
        input.clear();
        input.set_flags(0, A0 | A1);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            context_entries: 1,
            root_port: s.port,
            ..SlotContext::default()
        });
        input.set_endpoint(1, &ep0_context(s.max_packet0, s.ep0.enqueue_pointer()));
        let trb = Trb::address_device(s.input.phys(), slot, false);
        self.command(trb)?;
        // SET_ADDRESS recovery: USB 2.0 9.2.6.3 gives the device 2 ms; Linux
        // waits 10.
        self.hal.sleep(SET_ADDRESS_RECOVERY);
        Ok(())
    }

    /// Evaluate Context with EP0's real packet size (xHCI 4.6.7).
    fn set_max_packet0(&mut self, slot: u8, max_packet: u16) -> Result<(), UsbError> {
        let s = self.slots[slot as usize]
            .as_mut()
            .ok_or(UsbError::Disconnected)?;
        let input = Input::new(&s.input, self.info.context_size);
        input.clear();
        input.set_flags(0, A1);
        input.set_endpoint(1, &ep0_context(max_packet, s.ep0.enqueue_pointer()));
        let trb = Trb::evaluate_context(s.input.phys(), slot);
        self.command(trb)?;
        if let Some(s) = self.slots[slot as usize].as_mut() {
            s.max_packet0 = max_packet;
        }
        Ok(())
    }

    /// Forgets the device in `slot`: Disable Slot, free its rings, contexts
    /// and buffers. Later events for it are ignored. If Disable Slot fails,
    /// or the controller is dead, the memory is kept, as the controller may
    /// still use it.
    pub fn detach(&mut self, slot: u8) {
        let Some(port) = self
            .slots
            .get(slot as usize)
            .and_then(Option::as_ref)
            .map(|s| s.port)
        else {
            return;
        };
        self.release_slot(slot);
        xlog!(&self.hal, &self.name, "port {port}: slot {slot} released");
    }

    /// Disable Slot (a failure is logged; the slot is forgotten anyway),
    /// DCBAA[slot] = 0, and every ring, context and buffer freed.
    ///
    /// The memory is freed only once Disable Slot succeeded: until then the
    /// controller may still write to it (a transfer in progress, the device
    /// context). If it failed, timed out or the controller is dead, the
    /// memory is kept (leaked) and the slot forgotten.
    pub(super) fn release_slot(&mut self, slot: u8) {
        let s = self.slots[slot as usize].take();
        let outcome = if self.dead {
            Err(UsbError::ControllerDead)
        } else {
            self.command(Trb::disable_slot(slot)).map(|_| ())
        };
        match (outcome, s) {
            (Ok(()), s) => {
                self.dcbaa.write64(8 * slot as usize, 0);
                if let Some(s) = s {
                    s.free(&self.hal);
                }
            }
            (Err(e), _) => xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: Disable Slot failed ({e}); its memory is kept"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::regs::PR;
    use super::*;
    use crate::bus::GET_DESCRIPTOR;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn plugged(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(Duration::from_millis(60));
        xhci.port_changes();
        (hal, xhci)
    }

    /// The commands the fake executed, by TRB type.
    fn commands(hal: &FakeHal) -> Vec<u32> {
        hal.fake().executed().iter().map(|e| e.kind).collect()
    }

    fn requests(dev: &Dev) -> Vec<(u8, u16, u16)> {
        let r = dev.borrow().requests();
        r.iter()
            .map(|r| (r.setup.request, r.setup.value, r.setup.length))
            .collect()
    }

    #[test]
    fn the_k120_is_addressed_with_ep0_8_and_its_descriptors_parse() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let dev = xhci.attach(1).unwrap();
        assert_eq!((dev.slot, dev.port, dev.speed), (1, 1, Speed::Low));
        assert_eq!(
            (dev.descriptor.vendor, dev.descriptor.product),
            (0x046D, 0xC31C)
        );
        assert_eq!(dev.configuration.value, 1);
        assert_eq!(dev.configuration.interfaces.len(), 2);
        assert_eq!(xhci.slot_of_port(1), Some(1));
        assert_eq!(
            k120.borrow().address(),
            1,
            "SET_ADDRESS from Address Device"
        );
        let ep0 = hal.fake().endpoint(1, 1).unwrap();
        assert_eq!((ep0.ep_type, ep0.max_packet, ep0.cerr), (4, 8, 3));
        assert_eq!(ep0.average_trb_length, 8);
        assert_eq!(hal.fake().slot(1).unwrap().port, 1);
        let log = hal.log_text();
        assert!(log.contains(
            "xhci 00:14.0: port 1: slot 1 at address 1, 046d:c31c USB 1.10, ep0 8 bytes, 1 configuration"
        ));
        assert!(log.contains(
            "xhci 00:14.0: slot 1: interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 10"
        ));
        assert!(
            !commands(&hal).contains(&13),
            "no Evaluate Context for 8 bytes"
        );
    }

    #[test]
    fn the_superspeed_stick_gets_512_at_64_byte_contexts() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 13, &stick);
        let dev = xhci.attach(13).unwrap();
        assert_eq!(dev.speed, Speed::Super);
        assert_eq!(dev.descriptor.product, 0x1666);
        assert_eq!(
            hal.fake()
                .endpoint(dev.slot as usize, 1)
                .unwrap()
                .max_packet,
            512
        );
        assert!(!commands(&hal).contains(&13));
        assert!(hal.log_text().contains("0951:1666 USB 3.20, ep0 512 bytes"));
    }

    #[test]
    fn the_unifying_receiver_is_asked_for_8_bytes_before_ep0_becomes_64() {
        let receiver = FakeUsbDevice::unifying_receiver();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 2, &receiver);
        let dev = xhci.attach(2).unwrap();
        assert_eq!(dev.configuration.interfaces.len(), 3);
        assert_eq!(
            requests(&receiver),
            [
                (5, 1, 0),
                (GET_DESCRIPTOR, 0x0100, 8),
                (GET_DESCRIPTOR, 0x0100, 18),
                (GET_DESCRIPTOR, 0x0200, 9),
                (GET_DESCRIPTOR, 0x0200, 84),
            ]
        );
        assert!(commands(&hal).contains(&13), "Evaluate Context");
        assert_eq!(hal.fake().endpoint(1, 1).unwrap().max_packet, 64);
    }

    #[test]
    fn a_stall_fails_the_attach_and_frees_everything() {
        for (config, port, dev) in [
            (FakeConfig::basic(), 1, FakeUsbDevice::k120()),
            (FakeConfig::intel(), 13, FakeUsbDevice::kingston_stick()),
        ] {
            dev.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0100);
            let (hal, mut xhci) = plugged(config, port, &dev);
            let before = hal.outstanding_dma();
            assert_eq!(xhci.attach(port).err(), Some(UsbError::Stall));
            assert_eq!(hal.outstanding_dma(), before);
            assert!(!hal.fake().slot_enabled(1), "the slot is disabled");
            assert_eq!(xhci.dcbaa.read64(8), 0);
            assert_eq!(xhci.slot_of_port(port), None);
            let log = hal.log_text();
            let reason = alloc::format!("port {port}: device descriptor (8 bytes): stalled");
            assert!(log.contains(&reason), "{log}");
            assert!(log.contains("attach failed (stalled), slot 1 released"));
        }
    }

    #[test]
    fn a_device_that_never_answers_fails_its_attach_after_a_second() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().ignore_requests(1);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 3, &k120);
        let before = (hal.outstanding_dma(), hal.clock());
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Timeout));
        assert!(hal.clock() - before.1 >= Duration::from_secs(1));
        assert_eq!(hal.outstanding_dma(), before.0);
        // Stop Endpoint and Set TR Dequeue Pointer before Disable Slot.
        assert!(commands(&hal).ends_with(&[15, 16, 10]));
    }

    #[test]
    fn running_out_of_memory_for_a_slot_frees_what_it_got() {
        for n in 0..4 {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let before = hal.outstanding_dma();
            hal.fail_one_alloc(n);
            assert_eq!(
                xhci.attach(1).err(),
                Some(UsbError::NoMemory),
                "allocation {n}"
            );
            assert_eq!(hal.outstanding_dma(), before, "allocation {n}");
            assert!(!hal.fake().slot_enabled(1));
            assert!(
                hal.log_text()
                    .contains("port 1: slot memory: out of memory")
            );
        }
    }

    #[test]
    fn a_short_configuration_still_parses_what_arrived() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().truncate_configuration(34);
        let (_hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let dev = xhci.attach(1).unwrap();
        assert_eq!(dev.configuration.interfaces.len(), 1);
    }

    #[test]
    fn a_configuration_over_4096_bytes_is_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut config = k120.borrow().configuration_descriptor().to_vec();
        config[2..4].copy_from_slice(&5000u16.to_le_bytes());
        let device = k120.borrow().device_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let before = hal.outstanding_dma();
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::BadDescriptor("configuration too large"))
        );
        assert_eq!(hal.outstanding_dma(), before);
        assert!(
            hal.log_text()
                .contains("configuration descriptor: bad descriptor: configuration too large")
        );
    }

    #[test]
    fn a_device_without_configurations_is_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut device = k120.borrow().device_descriptor().to_vec();
        device[17] = 0;
        let config = k120.borrow().configuration_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config);
        let (_hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::Unsupported("no configuration"))
        );
    }

    #[test]
    fn a_device_unplugged_mid_enumeration_fails_cleanly() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().ignore_requests(1);
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 3, &k120);
        let before = hal.outstanding_dma();
        hal.fake()
            .after(Duration::from_millis(300), |x, _| x.unplug(3));
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Disconnected));
        assert!(hal.clock() < Duration::from_secs(1), "no timeout needed");
        assert_eq!(hal.outstanding_dma(), before);
        assert!(!hal.fake().slot_enabled(1));
        assert!(hal.log_text().contains("USB transaction error"));
        assert!(
            hal.log_text()
                .contains("port 3: attach failed (device disconnected)")
        );
    }

    #[test]
    fn a_device_unplugged_within_the_debounce_is_not_reset() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let start = hal.clock();
        hal.fake()
            .after(Duration::from_millis(50), |x, _| x.unplug(1));
        assert_eq!(xhci.attach(1).err(), Some(UsbError::Disconnected));
        assert!(hal.clock() - start >= DEBOUNCE);
        let resets = hal
            .fake()
            .portsc_writes()
            .iter()
            .filter(|(p, v)| *p == 1 && v & PR != 0)
            .count();
        assert_eq!(resets, 0);
    }

    /// When the port reset of `port` began (the fake's reset takes 10 ms).
    fn reset_began(hal: &FakeHal, port: u8) -> Duration {
        hal.fake().reset_done_at(port).expect("the port was reset") - Duration::from_millis(10)
    }

    #[test]
    fn a_steady_connection_is_reset_after_100_ms() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let start = hal.clock();
        xhci.attach(1).unwrap();
        let waited = reset_began(&hal, 1) - start;
        assert!(waited >= DEBOUNCE && waited < DEBOUNCE + Duration::from_millis(5));
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 1: connection stable after 100 ms")
        );
    }

    #[test]
    fn a_device_that_bounces_is_reset_100_ms_after_its_last_change() {
        // Out at 50 ms and in at 60 ms; and out and in again between two
        // looks, where only CSC shows it.
        for (out, back) in [(50, 60), (30, 40)] {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let start = hal.clock();
            let again = k120.clone();
            hal.fake()
                .after(Duration::from_millis(out), |x, _| x.unplug(1));
            hal.fake()
                .after(Duration::from_millis(back), move |x, _| x.plug(1, again));
            assert_eq!(xhci.attach(1).map(|d| d.port), Ok(1));
            let settled = Duration::from_millis(back) + DEBOUNCE;
            assert!(
                reset_began(&hal, 1) - start >= settled,
                "bounce at {out}-{back} ms: reset {:?} after the attach began",
                reset_began(&hal, 1) - start
            );
            assert!(hal.clock() - start >= settled);
            // The bounce is not taken for a replug later.
            assert_eq!(xhci.port_changes(), vec![], "bounce at {out}-{back} ms");
            // A real unplug still is.
            hal.fake().unplug(1);
            let changes = xhci.port_changes();
            assert_eq!(changes.len(), 1);
            assert_eq!(
                (changes[0].connected, changes[0].reconnected),
                (false, true)
            );
        }
    }

    #[test]
    fn a_replug_while_the_unplugs_csc_is_cleared_still_counts() {
        // Out at 40 ms; in again just as the debounce, looking at 50 ms,
        // clears the unplug's CSC, which clears the replug's too: only CCS
        // shows the device is back. One of these instants is the one.
        for us in 0..=10 {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let again = k120.clone();
            hal.fake()
                .after(Duration::from_millis(40), |x, _| x.unplug(1));
            let back = Duration::from_millis(50) + Duration::from_micros(us);
            hal.fake().after(back, move |x, _| x.plug(1, again));
            assert_eq!(xhci.attach(1).map(|d| d.port), Ok(1), "back at {back:?}");
        }
    }

    #[test]
    fn a_connection_that_keeps_bouncing_is_given_up_after_2_s_without_a_reset() {
        // Every 40 ms out for 20 ms, for 3 s; at 2 s it is in, or out.
        for (phase, outcome) in [(10, UsbError::Timeout), (30, UsbError::Disconnected)] {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let start = hal.clock();
            for n in 0..75 {
                let again = k120.clone();
                let out = Duration::from_millis(phase + 40 * n);
                hal.fake().after(out, |x, _| x.unplug(1));
                let back = out + Duration::from_millis(20);
                hal.fake().after(back, move |x, _| x.plug(1, again));
            }
            assert_eq!(xhci.attach(1).err(), Some(outcome), "phase {phase} ms");
            let took = hal.clock() - start;
            assert!(took >= Duration::from_secs(2) && took < Duration::from_millis(2050));
            assert_eq!(hal.fake().reset_done_at(1), None, "no reset");
            assert!(!hal.fake().slot_enabled(1));
            let log = hal.log_text();
            match outcome {
                UsbError::Timeout => {
                    assert!(log.contains("port 1: connection not stable after 2000 ms"))
                }
                _ => assert!(log.contains("port 1: not connected after debounce")),
            }
        }
    }

    #[test]
    fn ports_without_a_device_or_a_protocol_are_refused() {
        let (_hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.attach(2).err(), Some(UsbError::Disconnected));
        assert_eq!(
            xhci.attach(9).err(),
            Some(UsbError::Unsupported("no such port"))
        );
        assert_eq!(
            xhci.attach(0).err(),
            Some(UsbError::Unsupported("no such port"))
        );
    }

    #[test]
    fn a_port_is_attached_once_and_a_new_device_after_a_failure_works() {
        let bad = FakeUsbDevice::k120();
        bad.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0200);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &bad);
        assert_eq!(xhci.attach(1).err(), Some(UsbError::Stall));
        hal.fake().unplug(1);
        hal.fake().plug(1, FakeUsbDevice::k120());
        xhci.port_changes();
        assert_eq!(xhci.attach(1).map(|d| d.slot), Ok(1));
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::Unsupported("port already attached"))
        );
    }

    #[test]
    fn a_slot_whose_disable_fails_keeps_its_memory() {
        let mut config = FakeConfig::intel();
        config.hang_command = Some(10); // Disable Slot
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0100);
        let (hal, mut xhci) = plugged(config, 3, &k120);
        let before = hal.outstanding_dma();
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Stall));
        assert!(
            hal.fake().slot_enabled(1),
            "the controller still has the slot"
        );
        assert_eq!(
            hal.outstanding_dma(),
            before + 4,
            "contexts, EP0 ring and buffer kept"
        );
        assert_eq!(xhci.slot_of_port(3), None, "forgotten all the same");
        assert!(
            hal.log_text().contains(
                "xhci 00:14.0: slot 1: Disable Slot failed (timed out); its memory is kept"
            )
        );
    }

    #[test]
    fn detach_frees_everything_and_the_port_can_be_attached_again() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 3, &k120);
        let before = hal.outstanding_dma();
        let d = xhci.attach(3).unwrap();
        xhci.configure(&d, &[0, 1]).unwrap();
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert!(!hal.fake().slot_enabled(d.slot as usize));
        assert_eq!(xhci.dcbaa.read64(8 * d.slot as usize), 0);
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(hal.log_text().contains("port 3: slot 1 released"));
        xhci.detach(d.slot);
        let again = xhci.attach(3).unwrap();
        assert_eq!(again.slot, 1, "the lowest free slot");
        xhci.configure(&again, &[0]).unwrap();
    }
}
