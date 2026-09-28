//! Transfers (xHCI 4.11): control transfers on EP0, IN transfers that
//! complete later, bulk transfers that are waited for, matching transfer
//! events to what is in flight, and putting an endpoint back in order
//! after a STALL or a timeout.

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output, dci};
use super::trb::{
    SHORT_PACKET, STALL, STOPPED, STOPPED_LENGTH_INVALID, SUCCESS, Trb, completion_name,
};
use super::{Control, Transfer, Xhci};
use crate::{Bus, Hal, MAX_BULK, Setup, UsbError};
use core::fmt;
use core::sync::atomic::{Ordering, fence};
use core::time::Duration;

/// How long a control transfer may take (spec §6.2).
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a bulk transfer may take (spec §6.2).
pub const BULK_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest control or interrupt transfer: one page of DMA buffer.
pub const DATA_BUFFER_SIZE: usize = 4096;
/// A bulk endpoint's buffer: the largest bulk transfer.
pub const BULK_BUFFER_SIZE: usize = MAX_BULK;
/// How often a transfer wait polls.
const TRANSFER_POLL: Duration = Duration::from_micros(10);
/// EP0's Device Context Index.
const EP0: usize = 1;

impl Control {
    /// Takes in a transfer event for one of this request's TRBs. Each stage
    /// is its own TD (xHCI 4.11.2.2): a short IN data stage reports the
    /// residual and the status stage still follows.
    fn on_event(&mut self, event: &Trb) {
        let (trb, code) = (event.pointer(), event.completion_code());
        if self.result.is_some() {
            return;
        }
        // Some controllers report a short data stage as Success with the
        // residual (Linux: XHCI_TRUST_TX_LENGTH); both mean the same.
        if Some(trb) == self.data && matches!(code, SHORT_PACKET | SUCCESS) {
            self.residual = event.transfer_length();
        } else if trb == self.status && code == SUCCESS {
            self.result = Some(Ok(()));
        } else if (trb == self.setup || Some(trb) == self.data || trb == self.status)
            && code != SUCCESS
        {
            self.result = Some(Err(code));
        }
    }
}

impl<H: Hal> Xhci<H> {
    /// A control transfer on EP0 of `slot` (the `Bus::control` contract):
    /// at most 4096 bytes, 1 s at most. After a STALL, an error or a
    /// timeout EP0 is reset or stopped and repositioned, so the next
    /// request works.
    pub(super) fn control_transfer(
        &mut self,
        slot: u8,
        setup: Setup,
        data: &mut [u8],
    ) -> Result<usize, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let len = setup.length as usize;
        if data.len() < len {
            return Err(UsbError::Unsupported("buffer smaller than request"));
        }
        if len > DATA_BUFFER_SIZE {
            return Err(UsbError::Unsupported("request over 4096 bytes"));
        }
        let Some(s) = self.slots.get_mut(slot as usize).and_then(Option::as_mut) else {
            return Err(UsbError::Disconnected);
        };
        if s.control.is_some() {
            return Err(UsbError::Unsupported("control request in flight"));
        }
        if !setup.is_in() {
            s.data.write_bytes(0, &data[..len]);
        }
        let setup_trb = s.ep0.push(Trb::setup_stage(&setup));
        let data_trb = (len > 0).then(|| {
            s.ep0
                .push(Trb::data_stage(s.data.phys(), len as u32, setup.is_in()))
        });
        // The status stage goes the other way; IN when there is no data.
        let status = s.ep0.push(Trb::status_stage(len == 0 || !setup.is_in()));
        s.control = Some(Control {
            setup: setup_trb,
            data: data_trb,
            status,
            residual: 0,
            result: None,
        });
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, EP0 as u32);
        let result = self.wait_control(slot as usize);
        let s = self.slots[slot as usize]
            .as_mut()
            .expect("the slot outlives its request");
        let residual = s.control.take().map_or(0, |c| c.residual);
        let failure = match result {
            Some(Ok(())) => {
                let n = len.saturating_sub(residual as usize);
                if setup.is_in() {
                    s.data.read_bytes(0, &mut data[..n]);
                }
                return Ok(n);
            }
            Some(Err(STALL)) => UsbError::Stall,
            Some(Err(code)) => UsbError::Transfer(code),
            None if self.dead => return Err(UsbError::ControllerDead),
            None => UsbError::Timeout,
        };
        let what = match failure {
            UsbError::Transfer(code) => completion_name(code),
            UsbError::Stall => "stall",
            _ => "timed out",
        };
        xlog!(
            &self.hal,
            &self.name,
            "slot {slot}: control request {:#04x}/{} failed: {what}",
            setup.request_type,
            setup.request
        );
        if let Err(e) = self.reposition(slot as usize, EP0) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: EP0 recovery failed: {e}"
            );
        }
        Err(failure)
    }

    /// Polls until the request on `slot`'s EP0 ends; `None` after 1 s or
    /// when the controller dies.
    fn wait_control(&mut self, slot: usize) -> Option<Result<(), u8>> {
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                return None;
            }
            let control = self.slots[slot].as_ref().and_then(|s| s.control.as_ref());
            if let Some(result) = control.and_then(|c| c.result) {
                return Some(result);
            }
            if self.hal.now() - start >= CONTROL_TIMEOUT {
                return None;
            }
            self.hal.sleep(TRANSFER_POLL);
        }
    }

    /// A transfer event: it goes to the request it names. Events for a
    /// slot or TRB nothing waits for (a detached device, a request given
    /// up) are dropped.
    pub(super) fn transfer_event(&mut self, event: Trb) {
        let Some(s) = self
            .slots
            .get_mut(event.slot_id() as usize)
            .and_then(Option::as_mut)
        else {
            return;
        };
        let dci = event.endpoint_id();
        if dci == EP0 {
            if let Some(control) = s.control.as_mut() {
                control.on_event(&event);
            }
            return;
        }
        let Some(ep) = s.endpoint(dci) else {
            return;
        };
        let Transfer::Queued { trb, len } = ep.transfer else {
            return;
        };
        if trb != event.pointer() {
            return;
        }
        let code = event.completion_code();
        let residual = event.transfer_length() as usize;
        ep.transfer = Transfer::Done(match code {
            SUCCESS | SHORT_PACKET => Ok(len.saturating_sub(residual)),
            STALL => Err(UsbError::Stall),
            code => Err(UsbError::Transfer(code)),
        });
        // Stopped is the driver's own doing (an abort), not a failure.
        if !matches!(
            code,
            SUCCESS | SHORT_PACKET | STOPPED | STOPPED_LENGTH_INVALID
        ) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {} endpoint {:#04x}: transfer failed: {}",
                event.slot_id(),
                ep.address,
                completion_name(code)
            );
        }
    }

    /// A bulk transfer of `len` bytes on `endpoint` (the `Bus::bulk_in`
    /// and `bulk_out` contract): `out` is the data to send for an OUT
    /// endpoint. One Normal TRB into the endpoint's 64 KiB buffer; after
    /// 5 s the endpoint is stopped and its ring moved past the TRB.
    fn bulk_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        len: usize,
        out: Option<&[u8]>,
    ) -> Result<usize, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        if ep.lost {
            return Err(UsbError::Unsupported("endpoint lost after a failed abort"));
        }
        let Some(buffer) = ep.buffer.as_ref().filter(|b| b.size() == BULK_BUFFER_SIZE) else {
            return Err(UsbError::Unsupported("not a bulk endpoint"));
        };
        if len > buffer.size() {
            return Err(UsbError::Unsupported("bulk transfer over 64 KiB"));
        }
        if !matches!(ep.transfer, Transfer::Idle) {
            return Err(UsbError::Unsupported("transfer already queued"));
        }
        let dci = ep.dci;
        // A halted endpoint ignores its doorbell (xHCI 4.8.3): the
        // transfer would only time out.
        if self.endpoint_state(slot as usize, dci)? == EP_HALTED {
            return Err(UsbError::Stall);
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        let buffer = ep.buffer.as_ref().ok_or(UsbError::Disconnected)?;
        if let Some(data) = out {
            buffer.write_bytes(0, data);
        }
        let trb = ep.ring.push(Trb::normal(buffer.phys(), len as u32));
        ep.transfer = Transfer::Queued { trb, len };
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, dci as u32);
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                return Err(UsbError::ControllerDead);
            }
            let ep = self
                .endpoint_mut(slot, endpoint)
                .ok_or(UsbError::Disconnected)?;
            match core::mem::replace(&mut ep.transfer, Transfer::Idle) {
                Transfer::Done(result) => return result,
                other => ep.transfer = other,
            }
            if self.hal.now() - start >= BULK_TIMEOUT {
                break;
            }
            self.hal.sleep(TRANSFER_POLL);
        }
        xlog!(
            &self.hal,
            &self.name,
            "slot {slot} endpoint {endpoint:#04x}: bulk transfer of {len} bytes timed out"
        );
        let aborted = self.reposition(slot as usize, dci);
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
            ep.transfer = Transfer::Idle;
            if let Err(e) = aborted {
                // The controller may still own the TRB and its buffer.
                ep.lost = true;
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot} endpoint {endpoint:#04x}: abort failed: {e}; endpoint not used until reset"
                );
            }
        }
        Err(UsbError::Timeout)
    }

    /// Makes an endpoint usable after a halt or a timeout: Reset Endpoint
    /// if it is Halted, Stop Endpoint if it still runs, then Set TR Dequeue
    /// Pointer to where the next TRB will go, dropping whatever was queued.
    pub(super) fn reposition(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        match self.endpoint_state(slot, dci)? {
            EP_HALTED => {
                self.command(Trb::reset_endpoint(slot as u8, dci))?;
            }
            EP_RUNNING => {
                self.command(Trb::stop_endpoint(slot as u8, dci))?;
            }
            EP_STOPPED | EP_ERROR => {}
            EP_DISABLED => return Err(UsbError::Disconnected),
            _ => return Err(UsbError::Unsupported("reserved endpoint state")),
        }
        self.set_dequeue(slot, dci)
    }

    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        let s = self.slots[slot].as_mut().ok_or(UsbError::Disconnected)?;
        let dequeue = if dci == EP0 {
            s.ep0.enqueue_pointer()
        } else {
            s.endpoint(dci)
                .ok_or(UsbError::Disconnected)?
                .ring
                .enqueue_pointer()
        };
        self.command(Trb::set_tr_dequeue(slot as u8, dci, dequeue))?;
        Ok(())
    }

    /// The configured endpoint `address` of `slot`.
    fn endpoint_mut(&mut self, slot: u8, address: u8) -> Option<&mut super::Endpoint> {
        let s = self.slots.get_mut(slot as usize)?.as_mut()?;
        s.endpoints.iter_mut().find(|e| e.address == address)
    }

    /// The endpoint's state as the controller last wrote it.
    fn endpoint_state(&self, slot: usize, dci: usize) -> Result<u8, UsbError> {
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        Ok(Output::new(&s.output, self.info.context_size)
            .endpoint(dci)
            .state)
    }
}

impl<H: Hal> Bus for Xhci<H> {
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
        self.control_transfer(slot, setup, data)
    }

    /// One Normal TRB (IOC + ISP) into the endpoint's own buffer.
    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        if ep.lost {
            return Err(UsbError::Unsupported("endpoint lost after a failed abort"));
        }
        let Some(buffer) = ep.buffer.as_ref().filter(|_| endpoint & 0x80 != 0) else {
            return Err(UsbError::Unsupported("not an IN endpoint"));
        };
        if len > buffer.size() {
            return Err(UsbError::Unsupported("transfer larger than the buffer"));
        }
        if !matches!(ep.transfer, Transfer::Idle) {
            return Err(UsbError::Unsupported("transfer already queued"));
        }
        let trb = ep.ring.push(Trb::normal(buffer.phys(), len as u32));
        ep.transfer = Transfer::Queued { trb, len };
        let dci = ep.dci as u32;
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, dci);
        Ok(())
    }

    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        self.poll();
        // A dead controller finishes nothing: the class driver must hear
        // that, or a keyboard would wait (and repeat a held key) for ever.
        if self.dead {
            return Some(Err(UsbError::ControllerDead));
        }
        let Some(ep) = self.endpoint_mut(slot, endpoint) else {
            return Some(Err(UsbError::Disconnected));
        };
        if !matches!(ep.transfer, Transfer::Done(_)) {
            return None;
        }
        let Transfer::Done(result) = core::mem::replace(&mut ep.transfer, Transfer::Idle) else {
            return None;
        };
        Some(result.map(|n| {
            let n = n.min(buf.len());
            if let Some(buffer) = &ep.buffer {
                buffer.read_bytes(0, &mut buf[..n]);
            }
            n
        }))
    }

    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError> {
        if endpoint & 0x80 == 0 {
            return Err(UsbError::Unsupported("bulk IN on an OUT endpoint"));
        }
        let n = self.bulk_transfer(slot, endpoint, buf.len(), None)?;
        let n = n.min(buf.len());
        if let Some(buffer) = self
            .endpoint_mut(slot, endpoint)
            .and_then(|e| e.buffer.as_ref())
        {
            buffer.read_bytes(0, &mut buf[..n]);
        }
        Ok(n)
    }

    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError> {
        if endpoint & 0x80 != 0 {
            return Err(UsbError::Unsupported("bulk OUT on an IN endpoint"));
        }
        self.bulk_transfer(slot, endpoint, data.len(), Some(data))
    }

    /// Reset Endpoint if the context says Halted (Stop Endpoint if it still
    /// runs), Set TR Dequeue Pointer to the enqueue position, dropping
    /// anything outstanding, then CLEAR_FEATURE(ENDPOINT_HALT). An endpoint
    /// lost after a failed abort is usable again once this succeeds.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let dci = dci(endpoint);
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        ep.transfer = Transfer::Idle;
        self.reposition(slot as usize, dci)?;
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
            ep.lost = false;
        }
        self.control_transfer(slot, Setup::clear_halt(endpoint), &mut [])?;
        Ok(())
    }

    fn now(&self) -> Duration {
        self.hal.now()
    }

    fn sleep(&self, d: Duration) {
        self.hal.sleep(d);
    }

    fn log(&self, args: fmt::Arguments) {
        self.hal.log(args);
    }
}

#[cfg(test)]
mod tests {
    use super::super::Device;
    use super::*;
    use crate::bus::GET_DESCRIPTOR;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn attached(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>, Device) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(Duration::from_millis(60));
        xhci.port_changes();
        let d = xhci.attach(port).unwrap();
        (hal, xhci, d)
    }

    fn both() -> [(FakeConfig, u8, Dev); 2] {
        [
            (FakeConfig::basic(), 1, FakeUsbDevice::k120()),
            (FakeConfig::intel(), 13, FakeUsbDevice::kingston_stick()),
        ]
    }

    fn commands_since(hal: &FakeHal, n: usize) -> Vec<u32> {
        hal.fake().executed()[n..].iter().map(|e| e.kind).collect()
    }

    fn get_device(xhci: &mut Xhci<FakeHal>, slot: u8, len: u16) -> Result<usize, UsbError> {
        let mut buf = [0; 64];
        xhci.control_transfer(slot, Setup::get_descriptor(1, 0, len), &mut buf)
    }

    #[test]
    fn a_short_in_data_stage_returns_the_bytes_that_came() {
        for (config, port, dev) in both() {
            let (_hal, mut xhci, d) = attached(config, port, &dev);
            let mut buf = [0; 64];
            let n = xhci.control_transfer(d.slot, Setup::get_descriptor(1, 0, 64), &mut buf);
            assert_eq!(n, Ok(18));
            assert_eq!(&buf[..18], dev.borrow().device_descriptor());
        }
    }

    #[test]
    fn requests_without_data_and_out_requests_work() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::set_configuration(1), &mut []),
            Ok(0)
        );
        let k120 = FakeUsbDevice::k120();
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 2, &k120);
        let set_report = Setup {
            request_type: 0x21,
            request: 9,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(
            xhci.control_transfer(d.slot, set_report, &mut [0x02]),
            Ok(1)
        );
        let last = k120.borrow().requests().pop().unwrap();
        assert_eq!((last.setup, last.data), (set_report, vec![0x02]));
    }

    #[test]
    fn a_stall_resets_ep0_and_the_next_request_works() {
        for (config, port, dev) in both() {
            let (hal, mut xhci, d) = attached(config, port, &dev);
            dev.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0300);
            let n = hal.fake().executed().len();
            let mut buf = [0; 4];
            let r = xhci.control_transfer(d.slot, Setup::get_descriptor(3, 0, 4), &mut buf);
            assert_eq!(r, Err(UsbError::Stall));
            // Reset Endpoint, then Set TR Dequeue Pointer.
            assert_eq!(commands_since(&hal, n), [14, 16]);
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18));
            assert!(
                hal.log_text()
                    .contains("control request 0x80/6 failed: stall")
            );
        }
    }

    #[test]
    fn a_request_never_answered_times_out_and_ep0_is_repositioned() {
        for (config, port, dev) in both() {
            let (hal, mut xhci, d) = attached(config, port, &dev);
            dev.borrow_mut().ignore_requests(1);
            let (n, before) = (hal.fake().executed().len(), hal.clock());
            assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
            let waited = hal.clock() - before;
            assert!(waited >= CONTROL_TIMEOUT && waited < CONTROL_TIMEOUT * 2);
            // Stop Endpoint, then Set TR Dequeue Pointer.
            assert_eq!(commands_since(&hal, n), [15, 16]);
            assert_eq!(
                get_device(&mut xhci, d.slot, 18),
                Ok(18),
                "the device answers now"
            );
            assert_eq!(
                hal.fake().endpoint(d.slot as usize, 1).unwrap().state,
                1,
                "running"
            );
        }
    }

    #[test]
    fn many_requests_wrap_the_ep0_ring() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        for i in 0..200 {
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18), "request {i}");
        }
    }

    #[test]
    fn requests_the_driver_cannot_do_are_refused() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        let mut small = [0; 8];
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::get_descriptor(1, 0, 18), &mut small),
            Err(UsbError::Unsupported("buffer smaller than request"))
        );
        let mut big = vec![0; 5000];
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::get_descriptor(2, 0, 5000), &mut big),
            Err(UsbError::Unsupported("request over 4096 bytes"))
        );
        assert_eq!(get_device(&mut xhci, 7, 18), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_short_data_stage_reported_as_success_gives_its_length() {
        let mut config = FakeConfig::basic();
        config.short_as_success = true;
        let (_hal, mut xhci, d) = attached(config, 1, &FakeUsbDevice::k120());
        assert_eq!(get_device(&mut xhci, d.slot, 64), Ok(18));
    }

    #[test]
    fn without_unplug_failures_a_request_to_a_gone_device_times_out() {
        let mut config = FakeConfig::basic();
        config.fail_transfers_on_unplug = false;
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(config, 1, &k120);
        k120.borrow_mut().ignore_requests(1);
        hal.fake()
            .after(Duration::from_millis(5), |x, _| x.unplug(1));
        assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
    }

    fn keyboard(config: FakeConfig, port: u8) -> (FakeHal, Xhci<FakeHal>, Device, Dev) {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(config, port, &k120);
        xhci.configure(&d, &[0]).unwrap();
        (hal, xhci, d, k120)
    }

    /// `take_in` after letting the fake run for `ms` milliseconds.
    fn take_after(
        hal: &FakeHal,
        xhci: &mut Xhci<FakeHal>,
        slot: u8,
        ms: u64,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        hal.sleep(Duration::from_millis(ms));
        xhci.take_in(slot, 0x81, buf)
    }

    #[test]
    fn a_dead_controller_fails_the_queued_transfer_instead_of_keeping_it_pending() {
        // A keyboard waits for its report with `take_in`; if the controller
        // dies it must hear so, or a held key would repeat for ever.
        let (hal, mut xhci, d, _k120) = keyboard(FakeConfig::intel(), 3);
        let mut buf = [0; 8];
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        hal.fake().host_system_error();
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Err(UsbError::ControllerDead))
        );
        // And every later look says the same.
        assert_eq!(
            xhci.take_in(d.slot, 0x81, &mut buf),
            Some(Err(UsbError::ControllerDead))
        );
    }

    #[test]
    fn a_nakked_report_stays_pending_then_arrives_exactly_once() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        let mut buf = [0; 8];
        assert_eq!(xhci.take_in(d.slot, 0x81, &mut buf), None, "nothing queued");
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        for _ in 0..100 {
            assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        }
        k120.borrow_mut().push_in(0x81, &[0, 0, 4, 5, 6, 0, 0, 0]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
        assert_eq!(buf, [0, 0, 4, 5, 6, 0, 0, 0]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            None,
            "returned once"
        );
    }

    #[test]
    fn a_short_report_gives_its_length() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::intel(), 2);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[1, 2, 3]);
        let mut buf = [0; 8];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(3))
        );
        assert_eq!(&buf[..3], [1, 2, 3]);
        // A buffer smaller than what came gets what fits.
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[9; 8]);
        let mut small = [0; 2];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut small),
            Some(Ok(2))
        );
    }

    #[test]
    fn a_stalled_endpoint_is_recovered_by_clear_halt() {
        for (config, port) in [(FakeConfig::basic(), 1), (FakeConfig::intel(), 5)] {
            let (hal, mut xhci, d, k120) = keyboard(config, port);
            k120.borrow_mut().stall_endpoint(0x81);
            xhci.queue_in(d.slot, 0x81, 8).unwrap();
            let mut buf = [0; 8];
            assert_eq!(
                take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
                Some(Err(UsbError::Stall))
            );
            assert!(
                hal.log_text()
                    .contains("slot 1 endpoint 0x81: transfer failed: stall")
            );
            assert_eq!(
                hal.fake().endpoint(d.slot as usize, 3).unwrap().state,
                2,
                "halted"
            );
            let n = hal.fake().executed().len();
            xhci.clear_halt(d.slot, 0x81).unwrap();
            // Reset Endpoint, Set TR Dequeue Pointer, then the request.
            assert_eq!(commands_since(&hal, n), [14, 16]);
            let last = k120.borrow().requests().pop().unwrap();
            assert_eq!(last.setup, Setup::clear_halt(0x81));
            xhci.queue_in(d.slot, 0x81, 8).unwrap();
            k120.borrow_mut().push_in(0x81, &[7; 8]);
            assert_eq!(
                take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
                Some(Ok(8))
            );
        }
    }

    #[test]
    fn clear_halt_on_a_running_endpoint_drops_what_was_queued() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.sleep(Duration::from_millis(1));
        let n = hal.fake().executed().len();
        xhci.clear_halt(d.slot, 0x81).unwrap();
        // Not halted: Stop Endpoint instead of Reset Endpoint.
        assert_eq!(commands_since(&hal, n), [15, 16]);
        let mut buf = [0; 8];
        assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[5; 8]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
    }

    #[test]
    fn unplugging_fails_the_queued_transfer_and_detach_frees_everything() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(3, k120.clone());
        xhci.port_changes();
        let before = hal.outstanding_dma();
        let d = xhci.attach(3).unwrap();
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.fake().unplug(3);
        let mut buf = [0; 8];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Err(UsbError::Transfer(4)))
        );
        let changes = xhci.port_changes();
        assert!(changes[0].reconnected && !changes[0].connected);
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert_eq!(
            xhci.take_in(d.slot, 0x81, &mut buf),
            Some(Err(UsbError::Disconnected))
        );
    }

    #[test]
    fn a_bulk_in_transfer_gets_what_the_device_sent() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci, d) = attached(FakeConfig::intel(), 13, &stick);
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 512).unwrap();
        // A mass storage status wrapper: 13 bytes.
        stick.borrow_mut().push_in(0x81, &[0x55; 13]);
        let mut buf = [0; 512];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(13))
        );
        assert_eq!(buf[..13], [0x55; 13]);
    }

    #[test]
    fn an_event_for_another_trb_does_not_finish_a_transfer() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.act(|x, dma| x.post_transfer(d.slot as usize, 3, 0x1000, 1, 0, dma));
        let mut buf = [0; 8];
        assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        k120.borrow_mut().push_in(0x81, &[3; 8]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
    }

    #[test]
    fn events_for_a_detached_slot_are_ignored() {
        let (hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        xhci.detach(d.slot);
        hal.act(|x, dma| {
            x.post_transfer(1, 3, 0x1000, 1, 0, dma);
            x.post_transfer(1, 1, 0x2000, 6, 0, dma);
        });
        xhci.poll();
        assert_eq!(
            xhci.control(d.slot, Setup::set_configuration(1), &mut []),
            Err(UsbError::Disconnected)
        );
    }

    #[test]
    fn in_transfers_the_driver_cannot_do_are_refused() {
        let (_hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        assert_eq!(
            xhci.queue_in(d.slot, 0x82, 4),
            Err(UsbError::Disconnected),
            "interface 1"
        );
        assert_eq!(xhci.queue_in(9, 0x81, 8), Err(UsbError::Disconnected));
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 5000),
            Err(UsbError::Unsupported("transfer larger than the buffer"))
        );
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 8),
            Err(UsbError::Unsupported("transfer already queued"))
        );
        let mut buf = [0; 8];
        assert_eq!(
            xhci.take_in(d.slot, 0x82, &mut buf),
            Some(Err(UsbError::Disconnected))
        );
        assert_eq!(xhci.clear_halt(d.slot, 0x83), Err(UsbError::Disconnected));
    }

    #[test]
    fn the_bus_forwards_time_and_log_lines() {
        let (hal, xhci, _d, _k120) = keyboard(FakeConfig::basic(), 1);
        let before = hal.clock();
        assert!(Bus::now(&xhci) > before);
        Bus::sleep(&xhci, Duration::from_millis(3));
        assert!(hal.clock() >= before + Duration::from_millis(3));
        Bus::log(&xhci, format_args!("hid: hello"));
        assert!(hal.log_text().ends_with("hid: hello"));
    }

    fn stick(config: FakeConfig, port: u8) -> (FakeHal, Xhci<FakeHal>, Device, Dev) {
        let stick = if config.ports > 8 {
            FakeUsbDevice::kingston_stick()
        } else {
            FakeUsbDevice::usb2_stick()
        };
        let (hal, mut xhci, d) = attached(config, port, &stick);
        xhci.configure(&d, &[0]).unwrap();
        (hal, xhci, d, stick)
    }

    fn sticks() -> [(FakeHal, Xhci<FakeHal>, Device, Dev); 2] {
        [
            stick(FakeConfig::intel(), 13),
            stick(FakeConfig::basic(), 3),
        ]
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 256) as u8).collect()
    }

    #[test]
    fn bulk_transfers_move_up_to_64_kib_each_way() {
        for (_hal, mut xhci, d, dev) in sticks() {
            let data = pattern(MAX_BULK);
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &data), Ok(MAX_BULK));
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &data[..31]), Ok(31));
            let got = dev.borrow().data_out_received();
            assert_eq!(got, [(0x02, data.clone()), (0x02, data[..31].to_vec())]);
            dev.borrow_mut().push_in(0x81, &data);
            let mut buf = vec![0; MAX_BULK];
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(MAX_BULK));
            assert_eq!(buf, data);
        }
    }

    #[test]
    fn a_short_bulk_in_returns_what_came() {
        for (_hal, mut xhci, d, dev) in sticks() {
            dev.borrow_mut().push_in(0x81, &[0x55; 13]);
            let mut buf = vec![0; 512];
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
            assert_eq!(buf[..13], [0x55; 13]);
            assert!(buf[13..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn bulk_buffers_are_64_kib_aligned_so_no_transfer_crosses_a_boundary() {
        for (_hal, xhci, d, _dev) in sticks() {
            let s = xhci.slots[d.slot as usize].as_ref().unwrap();
            for ep in &s.endpoints {
                let buf = ep.buffer.as_ref().expect("bulk endpoints have a buffer");
                assert_eq!(buf.size(), MAX_BULK);
                assert_eq!(
                    buf.phys() % MAX_BULK as u64,
                    0,
                    "endpoint {:#x}",
                    ep.address
                );
            }
        }
        // Interrupt endpoints keep their page.
        let (_hal, xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        let s = xhci.slots[d.slot as usize].as_ref().unwrap();
        assert_eq!(s.endpoints[0].buffer.as_ref().unwrap().size(), 4096);
    }

    #[test]
    fn a_stalled_bulk_endpoint_stays_halted_until_its_halt_is_cleared() {
        for (hal, mut xhci, d, dev) in sticks() {
            dev.borrow_mut().stall_endpoint(0x02);
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Err(UsbError::Stall));
            // Halted: refused at once instead of waiting for a doorbell the
            // controller ignores.
            let before = hal.clock();
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Err(UsbError::Stall));
            assert!(hal.clock() - before < Duration::from_millis(1));
            xhci.clear_halt(d.slot, 0x02).unwrap();
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[2; 31]), Ok(31));
            assert_eq!(dev.borrow().data_out_received(), [(0x02, vec![2; 31])]);
        }
    }

    #[test]
    fn a_bulk_transfer_that_times_out_is_aborted_and_the_endpoint_works_again() {
        for (hal, mut xhci, d, dev) in sticks() {
            let (n, before) = (hal.fake().executed().len(), hal.clock());
            let mut buf = vec![0; 512];
            // Nothing to send: the device NAKs.
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Timeout));
            let waited = hal.clock() - before;
            assert!(waited >= BULK_TIMEOUT && waited < BULK_TIMEOUT + Duration::from_secs(1));
            // Stop Endpoint, then Set TR Dequeue Pointer past the TRB.
            assert_eq!(commands_since(&hal, n), [15, 16]);
            assert!(
                hal.log_text()
                    .contains("slot 1 endpoint 0x81: bulk transfer of 512 bytes timed out")
            );
            assert!(
                !hal.log_text().contains("transfer failed"),
                "a stop is no failure"
            );
            dev.borrow_mut().push_in(0x81, &[9; 13]);
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
        }
    }

    #[test]
    fn an_endpoint_whose_abort_failed_is_not_used_until_its_halt_is_cleared() {
        let (hal, mut xhci, d, dev) = stick(FakeConfig::intel(), 13);
        hal.fake().config_mut().hang_command = Some(15); // Stop Endpoint
        let mut buf = vec![0; 512];
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Timeout));
        assert!(hal.log_text().contains("abort failed"));
        let (n, before) = (hal.fake().executed().len(), hal.outstanding_dma());
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Unsupported("endpoint lost after a failed abort"))
        );
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 13),
            Err(UsbError::Unsupported("endpoint lost after a failed abort"))
        );
        assert_eq!(hal.fake().executed().len(), n, "nothing sent");
        // The controller still owns the TRB: the device's data lands in
        // the buffer, which is still allocated (the fake panics otherwise).
        dev.borrow_mut().push_in(0x81, &[7; 13]);
        hal.sleep(Duration::from_millis(5));
        assert_eq!(hal.outstanding_dma(), before);
        hal.fake().config_mut().hang_command = None;
        xhci.clear_halt(d.slot, 0x81).unwrap();
        dev.borrow_mut().push_in(0x81, &[8; 13]);
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
        assert_eq!(buf[..13], [8; 13]);
    }

    #[test]
    fn unplugging_during_a_bulk_transfer_fails_it_at_once() {
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        hal.fake()
            .after(Duration::from_millis(5), |x, _| x.unplug(13));
        let before = hal.clock();
        let mut buf = vec![0; 512];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Transfer(4))
        );
        assert!(hal.clock() - before < Duration::from_millis(10));
    }

    #[test]
    fn bulk_transfers_the_driver_cannot_do_are_refused() {
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        let mut big = vec![0; MAX_BULK + 1];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut big),
            Err(UsbError::Unsupported("bulk transfer over 64 KiB"))
        );
        assert_eq!(
            xhci.bulk_out(d.slot, 0x81, &[0; 4]),
            Err(UsbError::Unsupported("bulk OUT on an IN endpoint"))
        );
        assert_eq!(
            xhci.bulk_in(d.slot, 0x02, &mut [0; 4]),
            Err(UsbError::Unsupported("bulk IN on an OUT endpoint"))
        );
        assert_eq!(
            xhci.bulk_in(9, 0x81, &mut [0; 4]),
            Err(UsbError::Disconnected)
        );
        hal.fake().host_system_error();
        xhci.poll();
        assert_eq!(
            xhci.bulk_out(d.slot, 0x02, &[0; 4]),
            Err(UsbError::ControllerDead)
        );
        let (_hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut [0; 8]),
            Err(UsbError::Unsupported("not a bulk endpoint"))
        );
    }

    #[test]
    fn a_disable_slot_that_times_out_keeps_the_memory_the_controller_uses() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(FakeConfig::intel(), 3, &k120);
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.sleep(Duration::from_millis(20));
        let before = hal.outstanding_dma();
        hal.fake().config_mut().hang_command = Some(10); // Disable Slot
        xhci.detach(d.slot);
        assert!(
            hal.fake().slot_enabled(d.slot as usize),
            "the controller still has the slot"
        );
        assert_eq!(hal.outstanding_dma(), before, "nothing freed");
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(
            hal.log_text()
                .contains("slot 1: Disable Slot failed (timed out); its memory is kept")
        );
        // The keyboard sends a report: the controller writes it into the
        // endpoint buffer, which is still allocated (the fake panics on
        // freed memory).
        k120.borrow_mut().push_in(0x81, &[0, 0, 4, 0, 0, 0, 0, 0]);
        hal.sleep(Duration::from_millis(20));
    }

    #[test]
    fn detaching_on_a_dead_controller_frees_nothing() {
        let (hal, mut xhci, d, _k120) = keyboard(FakeConfig::intel(), 3);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.fake().host_system_error();
        xhci.poll();
        let before = hal.outstanding_dma();
        let commands = hal.fake().executed().len();
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert_eq!(hal.fake().executed().len(), commands, "no command sent");
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(hal.log_text().contains(
            "slot 1: Disable Slot failed (controller stopped working); its memory is kept"
        ));
    }
}
