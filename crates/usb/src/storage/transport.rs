//! One SCSI command over Bulk-Only Transport (BOT 5 and 6): the CBW on bulk
//! OUT, the data phase, then the CSW on bulk IN, with the recovery of spec
//! §6.4 and BOT 5.3 and 6.7:
//!
//! - a STALL in the data phase: the halt is cleared and the CSW read;
//! - a STALL on the CSW: the halt is cleared and the CSW read once more;
//! - CSW status 1: REQUEST SENSE, whose sense becomes the error;
//! - a phase error, an invalid CSW, a transfer that fails or times out:
//!   reset recovery (BOT 5.3.4), after which the device waits for a CBW.
//!
//! A command is tried three times at most; a gone device or controller is
//! not tried again.

use super::bot::{CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};
use super::scsi::{self, ILLEGAL_REQUEST, SENSE_LEN, Sense};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use core::fmt;

/// Bulk-Only Mass Storage Reset (BOT 3.1).
const MASS_STORAGE_RESET: u8 = 0xFF;

/// A command's data phase.
pub enum Data<'a> {
    None,
    /// Into this buffer; the command expects its length.
    In(&'a mut [u8]),
    Out(&'a [u8]),
}

impl Data<'_> {
    /// The same buffer again, for another try.
    fn reborrow(&mut self) -> Data<'_> {
        match self {
            Data::None => Data::None,
            Data::In(b) => Data::In(b),
            Data::Out(b) => Data::Out(b),
        }
    }

    fn len(&self) -> usize {
        match self {
            Data::None => 0,
            Data::In(b) => b.len(),
            Data::Out(b) => b.len(),
        }
    }
}

/// How much of its data phase a command needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// What the device has, up to the buffer (INQUIRY, REQUEST SENSE and
    /// READ CAPACITY may answer less than was asked).
    UpTo,
    /// Every byte (READ, WRITE): a data phase that moved less, or a CSW
    /// with a residue, is an error.
    All,
}

/// Tries of one command: the first and two retries (spec §6.4).
pub const MAX_TRIES: u32 = 3;

/// The device or the controller is gone: nothing more is tried.
pub fn is_fatal(e: UsbError) -> bool {
    matches!(e, UsbError::Disconnected | UsbError::ControllerDead)
}

/// A command for log lines: its name, and for READ(10) and WRITE(10) where
/// and how much.
struct Named<'a>(&'a [u8]);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self.0 {
            [
                op @ (scsi::READ_10 | scsi::WRITE_10),
                _,
                a,
                b,
                c,
                d,
                _,
                n0,
                n1,
                ..,
            ] => {
                let lba = u32::from_be_bytes([a, b, c, d]);
                let blocks = u16::from_be_bytes([n0, n1]);
                let plural = if blocks == 1 { "" } else { "s" };
                let name = scsi::command_name(op);
                write!(f, "{name} at LBA {lba}, {blocks} block{plural}")
            }
            [op, ..] => f.write_str(scsi::command_name(op)),
            [] => f.write_str("command"),
        }
    }
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
    /// phase moved. A failed command's error is its sense; a transfer that
    /// fails or times out, an invalid CSW or a phase error is followed by a
    /// reset recovery. With `Need::All`, a short data phase or a residue
    /// is an error too. A data phase over [`MAX_BULK`] is `Unsupported`,
    /// and nothing is sent.
    pub fn execute(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let length = data.len();
        if length > MAX_BULK {
            return Err(UsbError::Unsupported("data phase over 64 KiB"));
        }
        match self.exchange(bus, cdb, data) {
            Ok((moved, csw)) => match csw.status {
                CswStatus::Passed if need == Need::All && moved != length => {
                    Err(UsbError::Protocol("short data phase"))
                }
                CswStatus::Passed if need == Need::All && csw.residue != 0 => {
                    Err(UsbError::Protocol("data residue"))
                }
                CswStatus::Passed => Ok(moved),
                CswStatus::Failed => Err(self.request_sense(bus)),
                CswStatus::PhaseError => {
                    Err(self.recover(bus, cdb, UsbError::Protocol("phase error")))
                }
            },
            Err(e) => Err(self.recover(bus, cdb, e)),
        }
    }

    /// Runs `cdb` until it succeeds, [`MAX_TRIES`] times at most, and
    /// returns the bytes its data phase moved. A gone device or controller
    /// ends it at once, and so does ILLEGAL REQUEST (the command itself is
    /// wrong). Each failed try is logged with the command.
    pub fn command(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        mut data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let mut tries = 1;
        loop {
            let e = match self.execute(bus, cdb, data.reborrow(), need) {
                Ok(n) => return Ok(n),
                Err(e) if is_fatal(e) => return Err(e),
                Err(e) => e,
            };
            slog!(
                bus,
                self.slot,
                "{}: {e} (try {tries} of {MAX_TRIES})",
                Named(cdb)
            );
            let illegal = matches!(e, UsbError::Sense(s) if s.key == ILLEGAL_REQUEST);
            if illegal || tries >= MAX_TRIES {
                return Err(e);
            }
            tries += 1;
        }
    }

    /// CBW, data phase, CSW (BOT 5): the bytes moved and the CSW. A STALL
    /// ends the data phase early (BOT 6.7.2, 6.7.3): the halt is cleared
    /// and the CSW read as usual.
    fn exchange(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
    ) -> Result<(usize, Csw), UsbError> {
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
        let (moved, endpoint) = match data {
            Data::None => (Ok(0), self.bulk_in),
            Data::In(buf) => (bus.bulk_in(self.slot, self.bulk_in, buf), self.bulk_in),
            Data::Out(buf) => (bus.bulk_out(self.slot, self.bulk_out, buf), self.bulk_out),
        };
        let moved = match moved {
            Err(UsbError::Stall) => {
                slog!(bus, self.slot, "{}: data phase stalled", Named(cdb));
                bus.clear_halt(self.slot, endpoint)?;
                0
            }
            moved => moved?,
        };
        let csw = self.read_csw(bus, cdb, tag, length)?;
        Ok((moved, csw))
    }

    /// Reads and checks the CSW. A STALL is cleared and the CSW read once
    /// more (BOT 5.3.3, figure 2).
    fn read_csw(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        tag: u32,
        length: u32,
    ) -> Result<Csw, UsbError> {
        let mut raw = [0u8; CSW_LEN];
        let n = match bus.bulk_in(self.slot, self.bulk_in, &mut raw) {
            Err(UsbError::Stall) => {
                slog!(bus, self.slot, "{}: CSW stalled", Named(cdb));
                bus.clear_halt(self.slot, self.bulk_in)?;
                bus.bulk_in(self.slot, self.bulk_in, &mut raw)?
            }
            n => n?,
        };
        Csw::parse(&raw[..n.min(CSW_LEN)], tag, length)
    }

    /// REQUEST SENSE after a failed command (SPC-4 §5.11): the error the
    /// command fails with.
    fn request_sense(&mut self, bus: &mut dyn Bus) -> UsbError {
        let mut buf = [0u8; SENSE_LEN];
        let cdb = scsi::request_sense(SENSE_LEN as u8);
        match self.exchange(bus, &cdb, Data::In(&mut buf)) {
            Ok((n, csw)) => match csw.status {
                CswStatus::Passed => match Sense::parse(&buf[..n.min(SENSE_LEN)]) {
                    Ok(sense) => UsbError::Sense(sense),
                    Err(e) => e,
                },
                CswStatus::Failed => UsbError::Protocol("REQUEST SENSE failed"),
                CswStatus::PhaseError => self.recover(bus, &cdb, UsbError::Protocol("phase error")),
            },
            Err(e) => self.recover(bus, &cdb, e),
        }
    }

    /// Reset recovery after `cause` in command `cdb` (BOT 5.3.4):
    /// Bulk-Only Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT) of
    /// bulk IN and bulk OUT. Returns `cause`, or the error that showed the
    /// device or controller gone (then nothing more is sent).
    fn recover(&mut self, bus: &mut dyn Bus, cdb: &[u8], cause: UsbError) -> UsbError {
        if is_fatal(cause) {
            return cause;
        }
        slog!(bus, self.slot, "{}: {cause}; reset recovery", Named(cdb));
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
    use crate::testing::{
        BadCsw, Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op,
    };
    use crate::xhci::Xhci;
    use core::time::Duration;
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
        let n = t.execute(
            &mut xhci,
            &scsi::inquiry(36),
            Data::In(&mut buf),
            Need::UpTo,
        );
        assert_eq!(n, Ok(36));
        assert_eq!(&buf[8..16], b"Kingston");
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x12, 0x00]);
        assert_eq!(tags(&stick), [1, 2]);
    }

    #[test]
    fn tags_wrap() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        t.next_tag = u32::MAX;
        for _ in 0..2 {
            assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
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
            t.execute(&mut xhci, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Sense(not_ready))
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x00, 0x03, 0x00]);
        assert_eq!(stick.borrow().resets(), 0);
    }

    #[test]
    fn a_phase_error_is_followed_by_a_reset_recovery_and_the_device_works_again() {
        let (hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().phase_error_next();
        let mut buf = [0u8; 36];
        assert_eq!(
            t.execute(
                &mut xhci,
                &scsi::inquiry(36),
                Data::In(&mut buf),
                Need::UpTo
            ),
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
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
    }

    #[test]
    fn a_data_phase_over_64_kib_is_refused_before_anything_is_sent() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut big = vec![0u8; MAX_BULK + 512];
        assert_eq!(
            t.execute(
                &mut xhci,
                &scsi::read_10(0, 129),
                Data::In(&mut big),
                Need::UpTo
            ),
            Err(UsbError::Unsupported("data phase over 64 KiB"))
        );
        assert!(stick.borrow().events().is_empty());
        // 64 KiB is fine.
        let mut max = vec![0u8; MAX_BULK];
        let read = scsi::read_10(0, 128);
        assert_eq!(
            t.execute(&mut xhci, &read, Data::In(&mut max), Need::All),
            Ok(MAX_BULK)
        );
    }

    #[test]
    fn a_cbw_the_device_did_not_take_whole_is_a_protocol_error() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut bus = TamperBus::new(&mut xhci);
        bus.short_out = true;
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None, Need::UpTo),
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
            t.execute(&mut bus, &TUR, Data::None, Need::UpTo),
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
            t.execute(&mut xhci, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Disconnected)
        );
        assert!(!hal.log_text().contains("reset recovery"));
        assert!(stick.borrow().events().is_empty());
    }

    fn stick_with(
        knobs: impl FnOnce(&mut FakeStorage),
    ) -> (FakeHal, Xhci<FakeHal>, Transport, Stick) {
        let (hal, xhci, t, stick) = kingston();
        let block: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
        stick.borrow_mut().write_blocks(5, &block);
        knobs(&mut stick.borrow_mut());
        (hal, xhci, t, stick)
    }

    fn read_block(t: &mut Transport, bus: &mut dyn Bus, lba: u32) -> Result<Vec<u8>, UsbError> {
        let mut buf = vec![0u8; 512];
        let n = t.command(bus, &scsi::read_10(lba, 1), Data::In(&mut buf), Need::All)?;
        assert_eq!(n, 512);
        Ok(buf)
    }

    fn write_block(
        t: &mut Transport,
        bus: &mut dyn Bus,
        lba: u32,
        data: &[u8],
    ) -> Result<usize, UsbError> {
        t.command(bus, &scsi::write_10(lba, 1), Data::Out(data), Need::All)
    }

    /// The events as opcodes, resets and cleared halts, for comparing.
    fn trace(stick: &Stick) -> Vec<String> {
        stick
            .borrow()
            .events()
            .iter()
            .map(|e| match e {
                Event::GetMaxLun => "GET_MAX_LUN".into(),
                Event::Command(c) => format!("{:02x}", c.opcode),
                Event::Reset => "reset".into(),
                Event::ClearHalt(ep) => format!("clear {ep:02x}"),
            })
            .collect()
    }

    #[test]
    fn a_stalled_data_in_phase_is_cleared_the_csw_read_and_the_command_tried_again() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_next_data_in());
        let block = stick.borrow().read_blocks(5, 1);
        assert_eq!(read_block(&mut t, &mut xhci, 5), Ok(block));
        assert_eq!(trace(&stick), ["28", "clear 81", "03", "28"]);
        let log = hal.log_text();
        assert!(log.contains("storage: slot 1: READ(10) at LBA 5, 1 block: data phase stalled"));
        // The halt was cleared before the CSW was read, which did not stall.
        assert!(!log.contains("CSW stalled"));
        assert!(log.contains(
            "storage: slot 1: READ(10) at LBA 5, 1 block: ABORTED COMMAND (asc 0x00, ascq 0x00) (try 1 of 3)"
        ));
        let c = stick.borrow().commands();
        assert_eq!((c[0].opcode, c[0].lba, c[0].blocks), (op::READ_10, 5, 1));
    }

    #[test]
    fn a_stalled_data_out_phase_is_cleared_the_csw_read_and_the_command_tried_again() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_next_data_out());
        let data = [0x5A; 512];
        assert_eq!(write_block(&mut t, &mut xhci, 7, &data), Ok(512));
        assert_eq!(trace(&stick), ["2a", "clear 02", "03", "2a"]);
        assert_eq!(stick.borrow().read_blocks(7, 1), data);
    }

    #[test]
    fn a_stalled_csw_is_read_again_after_clearing_the_halt() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_csw_reads(1));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "clear 81"]);
        assert!(
            hal.log_text()
                .contains("storage: slot 1: READ(10) at LBA 5, 1 block: CSW stalled")
        );
    }

    #[test]
    fn a_csw_that_stalls_twice_is_followed_by_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_csw_reads(2));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(
            trace(&stick),
            ["28", "clear 81", "reset", "clear 81", "clear 02", "28"]
        );
    }

    #[test]
    fn a_unit_attention_is_retried_like_any_failure() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.unit_attention());
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "clear 81", "03", "28"]);
        assert!(hal.log_text().contains(
            "READ(10) at LBA 5, 1 block: UNIT ATTENTION (asc 0x29, ascq 0x00) (try 1 of 3)"
        ));
    }

    #[test]
    fn a_phase_error_is_retried_after_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.phase_error_next());
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "reset", "clear 81", "clear 02", "28"]);
    }

    #[test]
    fn invalid_csws_are_retried_after_a_reset_recovery() {
        for (bad, why) in [
            (BadCsw::Signature, "bad CSW signature"),
            (BadCsw::Tag, "CSW tag does not match its CBW"),
            (BadCsw::Short, "CSW not 13 bytes"),
        ] {
            let (hal, mut xhci, mut t, stick) = stick_with(|s| s.bad_csw_next(bad));
            assert!(read_block(&mut t, &mut xhci, 5).is_ok());
            assert_eq!(trace(&stick), ["28", "reset", "clear 81", "clear 02", "28"]);
            assert!(hal.log_text().contains(&format!(
                "READ(10) at LBA 5, 1 block: protocol error: {why}; reset recovery"
            )));
        }
    }

    #[test]
    fn short_data_or_a_residue_fails_a_read_or_write_and_it_is_tried_again() {
        // Short, with the residue that says so, or without it.
        for residue in [None, Some(0)] {
            let (hal, mut xhci, mut t, stick) = stick_with(|s| {
                s.short_next_data_in(100);
                if let Some(r) = residue {
                    s.residue_next(r);
                }
            });
            assert!(read_block(&mut t, &mut xhci, 5).is_ok());
            assert_eq!(trace(&stick), ["28", "28"]);
            assert!(
                hal.log_text()
                    .contains("protocol error: short data phase (try 1 of 3)")
            );
        }
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.residue_next(512));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "28"]);
        assert!(
            hal.log_text()
                .contains("protocol error: data residue (try 1 of 3)")
        );
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.residue_next(1));
        assert_eq!(write_block(&mut t, &mut xhci, 5, &[1; 512]), Ok(512));
        assert_eq!(trace(&stick), ["2a", "2a"]);
        // Less than asked is fine for INQUIRY.
        let (_hal, mut xhci, mut t, _stick) = stick_with(|s| s.short_next_data_in(10));
        let mut buf = [0u8; 36];
        let inquiry = scsi::inquiry(36);
        let n = t.command(&mut xhci, &inquiry, Data::In(&mut buf), Need::UpTo);
        assert_eq!(n, Ok(26));
    }

    #[test]
    fn a_command_is_tried_three_times_then_fails_with_its_last_error() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.medium_error_at(5));
        let medium_error = Sense {
            key: 3,
            asc: 0x11,
            ascq: 0,
        };
        assert_eq!(
            read_block(&mut t, &mut xhci, 5),
            Err(UsbError::Sense(medium_error))
        );
        let one = ["28", "clear 81", "03"];
        assert_eq!(trace(&stick), [one, one, one].concat());
        assert!(hal.log_text().contains(
            "storage: slot 1: READ(10) at LBA 5, 1 block: MEDIUM ERROR (asc 0x11, ascq 0x00) (try 3 of 3)"
        ));
        // The rest of the disk still reads.
        assert!(read_block(&mut t, &mut xhci, 6).is_ok());
    }

    #[test]
    fn a_device_that_does_not_answer_costs_5_s_a_try_and_ends_with_a_timeout() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.nak(true));
        let before = hal.clock();
        assert_eq!(read_block(&mut t, &mut xhci, 5), Err(UsbError::Timeout));
        let took = hal.clock() - before;
        assert!(
            took >= Duration::from_secs(15) && took < Duration::from_secs(17),
            "{took:?}"
        );
        // Each CBW timed out (the device never took one) and was followed
        // by a reset recovery.
        assert!(stick.borrow().commands().is_empty());
        assert_eq!(stick.borrow().resets(), 3);
        stick.borrow_mut().nak(false);
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
    }

    #[test]
    fn a_gone_device_or_controller_is_not_tried_again() {
        for gone in [UsbError::Disconnected, UsbError::ControllerDead] {
            let (_hal, mut xhci, mut t, stick) = stick_with(|_| {});
            let mut bus = TamperBus::new(&mut xhci);
            bus.fail_bulk = Some((0, gone));
            assert_eq!(read_block(&mut t, &mut bus, 5), Err(gone));
            assert_eq!(bus.bulk, 1);
            assert!(stick.borrow().events().is_empty());
        }
    }

    #[test]
    fn illegal_request_is_not_tried_again() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.no_synchronize_cache());
        let sync = scsi::synchronize_cache_10();
        let r = t.command(&mut xhci, &sync, Data::None, Need::UpTo);
        let invalid = Sense {
            key: 5,
            asc: 0x20,
            ascq: 0,
        };
        assert_eq!(r, Err(UsbError::Sense(invalid)));
        assert_eq!(trace(&stick), ["35", "03"]);
    }
}
