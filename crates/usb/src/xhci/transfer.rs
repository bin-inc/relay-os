//! Transfers (xHCI 4.11): control transfers on EP0, matching transfer
//! events to what is in flight, and putting an endpoint back in order
//! after a STALL or a timeout.

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output};
use super::trb::{SHORT_PACKET, STALL, SUCCESS, Trb, completion_name};
use super::{Control, Xhci};
use crate::{Hal, Setup, UsbError};
use core::sync::atomic::{Ordering, fence};
use core::time::Duration;

/// How long a control transfer may take (spec §6.2).
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
/// The largest transfer: one page of DMA buffer.
pub const DATA_BUFFER_SIZE: usize = 4096;
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
        if event.endpoint_id() == EP0
            && let Some(control) = s.control.as_mut()
        {
            control.on_event(&event);
        }
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
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        let dequeue = if dci == EP0 {
            s.ep0.enqueue_pointer()
        } else {
            return Err(UsbError::Disconnected);
        };
        self.command(Trb::set_tr_dequeue(slot as u8, dci, dequeue))?;
        Ok(())
    }

    /// The endpoint's state as the controller last wrote it.
    fn endpoint_state(&self, slot: usize, dci: usize) -> Result<u8, UsbError> {
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        Ok(Output::new(&s.output, self.info.context_size)
            .endpoint(dci)
            .state)
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
}
