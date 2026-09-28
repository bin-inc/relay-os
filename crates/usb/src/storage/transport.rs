//! One SCSI command over Bulk-Only Transport (BOT 5 and 6): the CBW on bulk
//! OUT, the data phase, then the CSW on bulk IN. A command that failed
//! (CSW status 1) is followed by REQUEST SENSE, and its sense becomes the
//! error. A phase error, an invalid CSW or a transfer that fails is followed
//! by a reset recovery (BOT 5.3.4), after which the device waits for a CBW
//! again.

use super::bot::{CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};
use super::scsi::{self, SENSE_LEN, Sense};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};

/// Bulk-Only Mass Storage Reset (BOT 3.1).
const MASS_STORAGE_RESET: u8 = 0xFF;

/// A command's data phase.
pub enum Data<'a> {
    None,
    /// Into this buffer; the command expects its length.
    In(&'a mut [u8]),
}

impl Data<'_> {
    fn len(&self) -> usize {
        match self {
            Data::None => 0,
            Data::In(b) => b.len(),
        }
    }
}

/// The device or the controller is gone: nothing more is tried.
pub fn is_fatal(e: UsbError) -> bool {
    matches!(e, UsbError::Disconnected | UsbError::ControllerDead)
}

/// The bulk pipes of one mass-storage interface and the next CBW tag.
pub struct Transport {
    slot: u8,
    interface: u8,
    bulk_in: u8,
    bulk_out: u8,
    next_tag: u32,
}

impl Transport {
    pub fn new(slot: u8, interface: u8, bulk_in: u8, bulk_out: u8) -> Transport {
        Transport {
            slot,
            interface,
            bulk_in,
            bulk_out,
            next_tag: 1,
        }
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// Runs `cdb` once with its data phase and returns the bytes the data
    /// phase moved. A failed command's error is its sense; any other
    /// failure is followed by a reset recovery. A data phase over
    /// [`MAX_BULK`] is `Unsupported`, and nothing is sent.
    pub fn execute(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
    ) -> Result<usize, UsbError> {
        if data.len() > MAX_BULK {
            return Err(UsbError::Unsupported("data phase over 64 KiB"));
        }
        let name = scsi::command_name(cdb.first().copied().unwrap_or(0xFF));
        match self.exchange(bus, cdb, data) {
            Ok((moved, CswStatus::Passed)) => Ok(moved),
            Ok((_, CswStatus::Failed)) => Err(self.request_sense(bus)),
            Ok((_, CswStatus::PhaseError)) => {
                Err(self.recover(bus, name, UsbError::Protocol("phase error")))
            }
            Err(e) => Err(self.recover(bus, name, e)),
        }
    }

    /// CBW, data phase, CSW: the bytes moved and the CSW's status.
    fn exchange(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
    ) -> Result<(usize, CswStatus), UsbError> {
        let tag = self.next_tag;
        self.next_tag = tag.wrapping_add(1);
        // At most MAX_BULK (checked by `execute`).
        let length = data.len() as u32;
        let cbw = Cbw {
            tag,
            data_length: length,
            dir_in: matches!(data, Data::In(_)),
            cdb,
        };
        if bus.bulk_out(self.slot, self.bulk_out, &cbw.to_bytes())? != CBW_LEN {
            return Err(UsbError::Protocol("CBW not taken whole"));
        }
        let moved = match data {
            Data::None => 0,
            Data::In(buf) => bus.bulk_in(self.slot, self.bulk_in, buf)?,
        };
        let mut raw = [0u8; CSW_LEN];
        let n = bus.bulk_in(self.slot, self.bulk_in, &mut raw)?;
        let csw = Csw::parse(&raw[..n.min(CSW_LEN)], tag, length)?;
        Ok((moved, csw.status))
    }

    /// REQUEST SENSE after a failed command (SPC-4 §5.11): the error the
    /// command fails with.
    fn request_sense(&mut self, bus: &mut dyn Bus) -> UsbError {
        let name = scsi::command_name(scsi::REQUEST_SENSE);
        let mut buf = [0u8; SENSE_LEN];
        let cdb = scsi::request_sense(SENSE_LEN as u8);
        match self.exchange(bus, &cdb, Data::In(&mut buf)) {
            Ok((n, CswStatus::Passed)) => match Sense::parse(&buf[..n.min(SENSE_LEN)]) {
                Ok(sense) => UsbError::Sense(sense),
                Err(e) => e,
            },
            Ok((_, CswStatus::Failed)) => UsbError::Protocol("REQUEST SENSE failed"),
            Ok((_, CswStatus::PhaseError)) => {
                self.recover(bus, name, UsbError::Protocol("phase error"))
            }
            Err(e) => self.recover(bus, name, e),
        }
    }

    /// Reset recovery after `cause` in command `name` (BOT 5.3.4):
    /// Bulk-Only Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT) of
    /// bulk IN and bulk OUT. Returns `cause`, or the error that showed the
    /// device or controller gone (then nothing more is sent).
    fn recover(&mut self, bus: &mut dyn Bus, name: &str, cause: UsbError) -> UsbError {
        if is_fatal(cause) {
            return cause;
        }
        slog!(bus, self.slot, "{name}: {cause}; reset recovery");
        let reset = Setup {
            request_type: TYPE_CLASS | RECIPIENT_INTERFACE,
            request: MASS_STORAGE_RESET,
            value: 0,
            index: self.interface as u16,
            length: 0,
        };
        // The reset, then each halt (the endpoint to clear).
        let steps = [
            ("mass storage reset", None),
            ("clearing the bulk IN halt", Some(self.bulk_in)),
            ("clearing the bulk OUT halt", Some(self.bulk_out)),
        ];
        for (what, endpoint) in steps {
            let done = match endpoint {
                None => bus.control(self.slot, reset, &mut []).map(|_| ()),
                Some(ep) => bus.clear_halt(self.slot, ep),
            };
            match done {
                Ok(()) => {}
                Err(e) if is_fatal(e) => return e,
                Err(e) => slog!(bus, self.slot, "{what} failed: {e}"),
            }
        }
        cause
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured};
    use crate::xhci::Xhci;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn kingston() -> (FakeHal, Xhci<FakeHal>, Transport, Stick) {
        let stick = FakeStorage::kingston();
        let (hal, xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        (hal, xhci, Transport::new(d.slot, 0, 0x81, 0x02), stick)
    }

    const TUR: [u8; 6] = [0; 6];

    fn tags(stick: &Stick) -> Vec<u32> {
        stick.borrow().commands().iter().map(|c| c.tag).collect()
    }

    #[test]
    fn a_command_goes_cbw_data_csw_with_a_new_tag_each_time() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut buf = [0u8; 36];
        let n = t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf));
        assert_eq!(n, Ok(36));
        assert_eq!(&buf[8..16], b"Kingston");
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x12, 0x00]);
        assert_eq!(tags(&stick), [1, 2]);
    }

    #[test]
    fn tags_wrap() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        t.next_tag = u32::MAX;
        for _ in 0..2 {
            assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        }
        assert_eq!(tags(&stick), [u32::MAX, 0]);
    }

    #[test]
    fn a_failed_command_is_followed_by_request_sense_and_fails_with_its_sense() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().not_ready_for(1);
        let not_ready = Sense {
            key: 2,
            asc: 4,
            ascq: 1,
        };
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Sense(not_ready))
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x00, 0x03, 0x00]);
        assert_eq!(stick.borrow().resets(), 0);
    }

    #[test]
    fn a_phase_error_is_followed_by_a_reset_recovery_and_the_device_works_again() {
        let (hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().phase_error_next();
        let mut buf = [0u8; 36];
        assert_eq!(
            t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf)),
            Err(UsbError::Protocol("phase error"))
        );
        let events = stick.borrow().events();
        assert_eq!(
            events[1..],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
        assert!(
            hal.log_text()
                .contains("storage: slot 1: INQUIRY: protocol error: phase error; reset recovery")
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
    }

    #[test]
    fn a_data_phase_over_64_kib_is_refused_before_anything_is_sent() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut big = vec![0u8; MAX_BULK + 512];
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_10(0, 129), Data::In(&mut big)),
            Err(UsbError::Unsupported("data phase over 64 KiB"))
        );
        assert!(stick.borrow().events().is_empty());
        // 64 KiB is fine.
        let mut max = vec![0u8; MAX_BULK];
        let read = scsi::read_10(0, 128);
        assert_eq!(
            t.execute(&mut xhci, &read, Data::In(&mut max)),
            Ok(MAX_BULK)
        );
    }

    #[test]
    fn a_stalled_data_phase_is_followed_by_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        // A failed command: the Kingston stalls its data phase.
        stick.borrow_mut().never_ready();
        let mut buf = [0u8; 8];
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_capacity_10(), Data::In(&mut buf)),
            Err(UsbError::Stall)
        );
        assert_eq!(
            stick.borrow().events()[1..],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
        let mut buf = [0u8; 36];
        let inquiry = scsi::inquiry(36);
        assert_eq!(t.execute(&mut xhci, &inquiry, Data::In(&mut buf)), Ok(36));
    }

    #[test]
    fn a_cbw_the_device_did_not_take_whole_is_a_protocol_error() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut bus = TamperBus::new(&mut xhci);
        bus.short_out = true;
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Protocol("CBW not taken whole"))
        );
        assert_eq!(stick.borrow().resets(), 1);
    }

    #[test]
    fn a_device_that_goes_during_recovery_is_left_alone() {
        let (hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().phase_error_next();
        let mut bus = TamperBus::new(&mut xhci);
        bus.fail_control = Some(UsbError::Disconnected);
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Disconnected)
        );
        // Neither halt was cleared after the reset failed that way.
        assert_eq!(stick.borrow().events().len(), 1);
        assert!(!hal.log_text().contains("failed: device disconnected"));
    }

    #[test]
    fn a_gone_device_is_not_recovered() {
        let (hal, mut xhci, mut t, stick) = kingston();
        hal.fake().unplug(13);
        xhci.detach(t.slot());
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Disconnected)
        );
        assert!(!hal.log_text().contains("reset recovery"));
        assert!(stick.borrow().events().is_empty());
    }
}
