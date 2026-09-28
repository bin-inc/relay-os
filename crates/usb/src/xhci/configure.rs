//! Configuring a device (spec §6.2 step 7): endpoint contexts for the
//! interfaces a class driver claimed, Configure Endpoint, then
//! SET_CONFIGURATION.

use super::context::{
    EndpointContext, Input, SlotContext, dci, endpoint_type, interval_exponent, speed_id,
};
use super::device::{A0, kind_name};
use super::ring::ProducerRing;
use super::transfer::{BULK_BUFFER_SIZE, DATA_BUFFER_SIZE};
use super::trb::Trb;
use super::{Device, Endpoint, Transfer, Xhci};
use crate::descriptor::{self, EndpointKind};
use crate::{Hal, Setup, Speed, UsbError};
use alloc::vec::Vec;

/// The endpoint context for `e` of a device at `speed` (xHCI 6.2.3,
/// 4.14.1.1): bursts from the SuperSpeed companion or, for high-speed
/// periodic endpoints, the extra transactions; interrupt endpoints move
/// one packet per interval, bulk ones large TRBs.
fn endpoint_context(e: &descriptor::Endpoint, speed: Speed, dequeue: u64) -> EndpointContext {
    let interrupt = e.kind == EndpointKind::Interrupt;
    let max_burst = if speed.is_superspeed() {
        e.max_burst
    } else if speed == Speed::High && interrupt {
        e.extra_transactions()
    } else {
        0
    };
    let packet = e.packet_size();
    EndpointContext {
        ep_type: endpoint_type(e.kind, e.is_in()),
        cerr: 3,
        max_packet: packet,
        max_burst,
        interval: interval_exponent(speed, e.kind, e.interval),
        dequeue,
        average_trb_length: if interrupt { packet } else { 3072 },
        max_esit_payload: if interrupt {
            packet as u32 * (max_burst as u32 + 1)
        } else {
            0
        },
        ..EndpointContext::default()
    }
}

/// A transfer ring, and a buffer, for endpoint `e`: bulk endpoints get
/// 64 KiB aligned to 64 KiB, so one Normal TRB covers any transfer without
/// crossing a 64 KiB boundary (xHCI 6.4.1.1); interrupt IN endpoints get
/// 4 KiB; interrupt OUT endpoints none.
fn new_endpoint<H: Hal>(hal: &H, e: &descriptor::Endpoint) -> Result<Endpoint, UsbError> {
    let ring = ProducerRing::new(hal)?;
    let size = match e.kind {
        EndpointKind::Bulk => Some(BULK_BUFFER_SIZE),
        _ if e.is_in() => Some(DATA_BUFFER_SIZE),
        _ => None,
    };
    let buffer = match size {
        Some(size) => {
            let Some(buf) = hal.alloc_dma(size, size) else {
                ring.free(hal);
                return Err(UsbError::NoMemory);
            };
            Some(buf)
        }
        None => None,
    };
    Ok(Endpoint {
        address: e.address,
        dci: dci(e.address),
        ring,
        buffer,
        transfer: Transfer::Idle,
        lost: false,
        context: EndpointContext::default(),
    })
}

impl<H: Hal> Xhci<H> {
    /// Configures the endpoints of the given interfaces of `device` (spec
    /// §6.2 step 7: Configure Endpoint, then SET_CONFIGURATION). Endpoints of
    /// other interfaces are left unconfigured.
    pub fn configure(&mut self, device: &Device, interfaces: &[u8]) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let slot = device.slot;
        let stride = self.info.context_size;
        let s = match self.slots.get(slot as usize).and_then(Option::as_ref) {
            Some(s) if s.port == device.port => s,
            _ => return Err(UsbError::Disconnected),
        };
        if !s.endpoints.is_empty() {
            return Err(UsbError::Unsupported("already configured"));
        }
        let input = Input::new(&s.input, stride);
        input.clear();
        let mut endpoints: Vec<Endpoint> = Vec::new();
        let mut add = A0;
        let claimed = device
            .configuration
            .interfaces
            .iter()
            .filter(|i| interfaces.contains(&i.number));
        for e in claimed.flat_map(|i| &i.endpoints) {
            let dci = dci(e.address);
            if !matches!(e.kind, EndpointKind::Interrupt | EndpointKind::Bulk)
                || endpoints.iter().any(|ep| ep.dci == dci)
            {
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot}: endpoint {:#04x} ({}) not configured",
                    e.address,
                    kind_name(e.kind)
                );
                continue;
            }
            let r = if e.packet_size() == 0 {
                Err(UsbError::BadDescriptor("endpoint packet size 0"))
            } else {
                new_endpoint(&self.hal, e)
            };
            let ep = match r {
                Ok(ep) => ep,
                Err(err) => {
                    endpoints.into_iter().for_each(|ep| ep.free(&self.hal));
                    xlog!(
                        &self.hal,
                        &self.name,
                        "slot {slot}: endpoint {:#04x}: {err}",
                        e.address
                    );
                    return Err(err);
                }
            };
            let mut ep = ep;
            ep.context = endpoint_context(e, s.speed, ep.ring.enqueue_pointer());
            input.set_endpoint(dci, &ep.context);
            add |= 1 << dci;
            endpoints.push(ep);
        }
        input.set_flags(0, add);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            // The highest DCI in use (xHCI 6.2.2).
            context_entries: 31 - add.leading_zeros() as u8,
            root_port: s.port,
            ..SlotContext::default()
        });
        let trb = Trb::configure_endpoint(s.input.phys(), slot, false);
        if let Err(e) = self.command(trb) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: Configure Endpoint: {e}"
            );
            // A dead controller may still have the new rings.
            if !self.dead {
                endpoints.into_iter().for_each(|ep| ep.free(&self.hal));
            }
            return Err(e);
        }
        for ep in &endpoints {
            let e = device
                .configuration
                .interfaces
                .iter()
                .flat_map(|i| &i.endpoints);
            let interval = e
                .clone()
                .find(|e| e.address == ep.address)
                .map_or(0, |e| interval_exponent(device.speed, e.kind, e.interval));
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: endpoint {:#04x} configured (DCI {}, interval exponent {interval})",
                ep.address,
                ep.dci
            );
        }
        if let Some(s) = self.slots[slot as usize].as_mut() {
            s.endpoints = endpoints;
        }
        let value = device.configuration.value;
        self.control_transfer(slot, Setup::set_configuration(value), &mut [])
            .inspect_err(|e| {
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot}: SET_CONFIGURATION({value}): {e}"
                )
            })?;
        Ok(())
    }
}

impl<H: Hal> Xhci<H> {
    /// Drops endpoint `dci` of `slot` and adds it again with one Configure
    /// Endpoint: its context as `configure` made it, the ring going on from
    /// where the next TRB will go (whatever was queued is dropped). The
    /// controller starts it over, with its data toggle or sequence number
    /// 0 (xHCI 4.6.6, and 4.6.8's note on endpoints that are not Halted).
    /// The endpoint must be stopped.
    pub(super) fn add_again(&mut self, slot: u8, dci: usize) -> Result<(), UsbError> {
        let stride = self.info.context_size;
        let s = self
            .slots
            .get(slot as usize)
            .and_then(Option::as_ref)
            .ok_or(UsbError::Disconnected)?;
        let ep = s
            .endpoints
            .iter()
            .find(|e| e.dci == dci)
            .ok_or(UsbError::Disconnected)?;
        let highest = s.endpoints.iter().map(|e| e.dci).max().unwrap_or(dci);
        let input = Input::new(&s.input, stride);
        input.clear();
        input.set_flags(1 << dci, A0 | 1 << dci);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            context_entries: highest as u8,
            root_port: s.port,
            ..SlotContext::default()
        });
        let context = EndpointContext {
            dequeue: ep.ring.enqueue_pointer(),
            ..ep.context
        };
        input.set_endpoint(dci, &context);
        let trb = Trb::configure_endpoint(s.input.phys(), slot, false);
        self.command(trb)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bus;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn plugged(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(core::time::Duration::from_millis(60));
        xhci.port_changes();
        (hal, xhci)
    }

    fn configured(
        config: FakeConfig,
        port: u8,
        dev: &Dev,
        interfaces: &[u8],
    ) -> (FakeHal, Xhci<FakeHal>, Device) {
        let (hal, mut xhci) = plugged(config, port, dev);
        let device = xhci.attach(port).unwrap();
        xhci.configure(&device, interfaces).unwrap();
        (hal, xhci, device)
    }

    #[test]
    fn configuring_the_k120s_boot_interface_sets_up_its_interrupt_endpoint() {
        let k120 = FakeUsbDevice::k120();
        let (hal, _xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0]);
        let ep = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        assert_eq!(
            (ep.ep_type, ep.max_packet, ep.interval, ep.cerr),
            (7, 8, 6, 3)
        );
        assert_eq!((ep.average_trb_length, ep.max_esit_payload), (8, 8));
        assert_eq!(hal.fake().slot(d.slot as usize).unwrap().context_entries, 3);
        // SET_CONFIGURATION(1) reached the device once the slot was
        // configured.
        let set = hal
            .fake()
            .requests()
            .iter()
            .find(|(_, s, _)| s.request == 9)
            .copied();
        assert_eq!(set.map(|(_, s, state)| (s.value, state)), Some((1, 3)));
        assert_eq!(k120.borrow().configuration_value(), 1);
        assert!(
            hal.log_text()
                .contains("slot 1: endpoint 0x81 configured (DCI 3, interval exponent 6)")
        );
        // Interface 1 was not asked for.
        assert!(hal.fake().endpoint(d.slot as usize, 5).is_none());
    }

    #[test]
    fn the_sticks_bulk_endpoints_get_their_burst_and_packet_size() {
        let (hal, _xhci, d) = configured(
            FakeConfig::intel(),
            13,
            &FakeUsbDevice::kingston_stick(),
            &[0],
        );
        let bulk_in = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        let bulk_out = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(
            (bulk_in.ep_type, bulk_in.max_packet, bulk_in.max_burst),
            (6, 1024, 3)
        );
        assert_eq!(
            (bulk_out.ep_type, bulk_out.max_packet, bulk_out.max_burst),
            (2, 1024, 3)
        );
        assert_eq!((bulk_in.interval, bulk_in.average_trb_length), (0, 3072));
        let (hal, _xhci, d) =
            configured(FakeConfig::basic(), 3, &FakeUsbDevice::usb2_stick(), &[0]);
        let bulk_in = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        assert_eq!((bulk_in.max_packet, bulk_in.max_burst), (512, 0));
    }

    #[test]
    fn isochronous_endpoints_are_skipped_and_empty_ones_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut config = k120.borrow().configuration_descriptor().to_vec();
        config[30] = 1; // endpoint 0x81 becomes isochronous
        let device = k120.borrow().device_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config.clone());
        let (hal, _xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0, 1]);
        assert!(hal.fake().endpoint(d.slot as usize, 3).is_none());
        assert!(hal.fake().endpoint(d.slot as usize, 5).is_some());
        assert!(
            hal.log_text()
                .contains("endpoint 0x81 (isochronous) not configured")
        );
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 2, &FakeUsbDevice::k120());
        let mut device = xhci.attach(2).unwrap();
        device.configuration.interfaces[0].endpoints[0].max_packet = 0;
        let before = hal.outstanding_dma();
        assert_eq!(
            xhci.configure(&device, &[1, 0]),
            Err(UsbError::BadDescriptor("endpoint packet size 0"))
        );
        assert_eq!(hal.outstanding_dma(), before);
    }

    #[test]
    fn a_failed_configure_endpoint_frees_the_new_rings() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(12);
        let (hal, mut xhci) = plugged(config, 1, &FakeUsbDevice::k120());
        let device = xhci.attach(1).unwrap();
        let before = hal.outstanding_dma();
        assert_eq!(xhci.configure(&device, &[0, 1]), Err(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), before);
        assert!(
            hal.log_text()
                .contains("slot 1: Configure Endpoint: timed out")
        );
    }

    #[test]
    fn a_configure_endpoint_that_kills_the_controller_keeps_the_new_rings() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(12);
        config.abort_never_completes = true;
        let (hal, mut xhci) = plugged(config, 1, &FakeUsbDevice::k120());
        let device = xhci.attach(1).unwrap();
        let before = hal.outstanding_dma();
        assert_eq!(xhci.configure(&device, &[0, 1]), Err(UsbError::Timeout));
        assert_eq!(
            hal.outstanding_dma(),
            before + 4,
            "two rings and two buffers kept"
        );
    }

    #[test]
    fn running_out_of_memory_while_configuring_frees_what_it_got() {
        for n in 0..3 {
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
            let device = xhci.attach(1).unwrap();
            let before = hal.outstanding_dma();
            hal.fail_one_alloc(n);
            assert_eq!(
                xhci.configure(&device, &[0, 1]),
                Err(UsbError::NoMemory),
                "allocation {n}"
            );
            assert_eq!(hal.outstanding_dma(), before, "allocation {n}");
        }
    }

    #[test]
    fn adding_an_endpoint_again_keeps_its_context_with_the_ring_going_on() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        let before = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[0; 31]), Ok(31));
        xhci.command(Trb::stop_endpoint(d.slot, 4)).unwrap();
        xhci.add_again(d.slot, 4).unwrap();
        let after = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(
            (
                after.ep_type,
                after.max_packet,
                after.max_burst,
                after.toggle
            ),
            (before.ep_type, before.max_packet, before.max_burst, 0)
        );
        // The ring goes on after the TRB used: its dequeue moved one TRB.
        assert_eq!(after.ring.dequeue, before.ring.dequeue + 16);
        // With the device's toggle reset too, the next transfer works.
        let clear = Setup::clear_halt(0x02);
        xhci.control_transfer(d.slot, clear, &mut []).unwrap();
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[0; 31]), Ok(31));
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint drops DCI 4 while it runs")]
    fn the_fake_refuses_to_drop_a_running_endpoint() {
        let stick = FakeUsbDevice::kingston_stick();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        let _ = xhci.add_again(d.slot, 4);
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint drops DCI 5, which is not configured")]
    fn the_fake_refuses_to_drop_an_endpoint_that_is_not_there() {
        let stick = FakeUsbDevice::kingston_stick();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        // As if the driver had lost track of which endpoint it set up.
        let s = xhci.slots[d.slot as usize].as_mut().unwrap();
        s.endpoints[1].dci = 5;
        let _ = xhci.add_again(d.slot, 5);
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint adds DCI 3 again without dropping it")]
    fn the_fake_refuses_to_add_an_endpoint_twice() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0]);
        // As if the driver had forgotten it configured the device.
        let s = xhci.slots[d.slot as usize].as_mut().unwrap();
        for ep in core::mem::take(&mut s.endpoints) {
            ep.free(&hal);
        }
        let _ = xhci.configure(&d, &[0]);
    }
}
