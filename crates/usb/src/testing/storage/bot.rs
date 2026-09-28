//! The fake stick's Bulk-Only Transport (BOT 5, 6): waiting for a CBW, the
//! data phase, the CSW; the class requests GET_MAX_LUN and Bulk-Only Mass
//! Storage Reset (BOT 3.1, 3.2); and the halts a reset recovery must clear
//! (BOT 5.3.4).

use super::scsi::Answer;
use super::*;
use crate::Speed;
use crate::bus::{CLEAR_FEATURE, ENDPOINT_HALT, RECIPIENT_ENDPOINT};
use crate::testing::device::Stall;

/// `add_boot_keyboard`'s interface and endpoint.
const KEYBOARD_INTERFACE: u16 = 1;
const KEYBOARD_IN: u8 = 0x83;

pub(super) enum Phase {
    /// Waiting for a CBW on bulk OUT.
    Cbw,
    /// `data` goes to the host; `left` of the announced bytes have not.
    DataIn {
        tag: u32,
        data: Vec<u8>,
        sent: usize,
        left: u32,
        status: u8,
        stall: bool,
    },
    /// `left` announced bytes still to come.
    DataOut {
        tag: u32,
        left: u32,
        received: Vec<u8>,
        write: Option<u64>,
        status: u8,
        stall: bool,
    },
    /// The CSW waits to be read on bulk IN.
    Csw { tag: u32, residue: u32, status: u8 },
}

impl FakeStorage {
    /// A CBW arrived on bulk OUT: checks it and starts the command.
    fn cbw(&mut self, b: &[u8]) {
        if b.len() != CBW_LEN {
            panic!("fake storage: CBW of {} bytes", b.len());
        }
        let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if le32(0) != CBW_SIGNATURE {
            panic!("fake storage: CBW signature {:#010x}", le32(0));
        }
        if !self.uncleared.is_empty() {
            panic!(
                "fake storage: CBW after a reset before the halts of {:x?} were cleared (BOT 5.3.4)",
                self.uncleared
            );
        }
        let (tag, length, flags, lun, cb_len) = (le32(4), le32(8), b[12], b[13], b[14]);
        if self.last_tag == Some(tag) {
            panic!("fake storage: CBW tag {tag:#x} again");
        }
        if flags & 0x7F != 0 {
            panic!("fake storage: reserved CBW flags {flags:#04x}");
        }
        if lun != 0 {
            panic!("fake storage: CBW for LUN {lun}");
        }
        if cb_len == 0 || cb_len > 16 {
            panic!("fake storage: bCBWCBLength {cb_len}");
        }
        self.last_tag = Some(tag);
        let cdb = &b[15..15 + cb_len as usize];
        self.command(tag, length, flags & 0x80 != 0, cdb);
    }

    /// Enters the data phase of a command that ends with `status`: a
    /// command that did not pass stalls its data phase if `stall`, or pads
    /// it (zeros in, data out discarded).
    pub(super) fn begin(
        &mut self,
        tag: u32,
        length: u32,
        dir_in: bool,
        answer: Answer,
        status: u8,
        stall: bool,
    ) {
        self.phase = if length == 0 {
            Phase::Csw {
                tag,
                residue: 0,
                status,
            }
        } else if dir_in {
            let mut data = match answer {
                Answer::In(data) => data,
                _ if stall => Vec::new(),
                _ => vec![0; length as usize],
            };
            data.truncate(length as usize);
            if status == PASSED {
                let short = std::mem::take(&mut self.short_data_in) as usize;
                data.truncate(data.len().saturating_sub(short));
            }
            Phase::DataIn {
                tag,
                data,
                sent: 0,
                left: length,
                status,
                stall,
            }
        } else {
            let write = match answer {
                Answer::Out(lba) if status == PASSED => Some(lba),
                _ => None,
            };
            Phase::DataOut {
                tag,
                left: length,
                received: Vec::new(),
                write,
                status,
                stall,
            }
        };
    }
}

impl FakeDevice for FakeStorage {
    fn speed(&self) -> Speed {
        self.usb.speed()
    }

    fn max_packet0(&self) -> u16 {
        self.usb.max_packet0()
    }

    fn usb_address(&self) -> u8 {
        self.usb.usb_address()
    }

    /// A bus reset also ends whatever command was going on.
    fn bus_reset(&mut self) {
        self.usb.bus_reset();
        self.phase = Phase::Cbw;
        self.uncleared.clear();
    }

    /// The standard requests go to the `FakeUsbDevice` (with its knobs);
    /// the class requests are GET_MAX_LUN and the Bulk-Only Mass Storage
    /// Reset (BOT 3.1, 3.2), both for interface 0 only.
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>> {
        let answer = self.usb.control(setup, data_out)?;
        if answer.is_err() {
            return Some(answer);
        }
        if setup.request_type & 0x60 == 0x20 {
            if self.keyboard && setup.index == KEYBOARD_INTERFACE {
                return Some(answer);
            }
            if setup.request_type & 0x1F != 1 || setup.index != INTERFACE || setup.value != 0 {
                panic!("fake storage: class request {setup:?} not for interface 0");
            }
            return Some(match (setup.request_type, setup.request, setup.length) {
                (0xA1, GET_MAX_LUN, 1) => {
                    self.events.push(Event::GetMaxLun);
                    self.max_lun.map(|n| vec![n]).ok_or(Stall)
                }
                (0x21, RESET, 0) => {
                    self.phase = Phase::Cbw;
                    self.uncleared = BTreeSet::from([BULK_IN, BULK_OUT]);
                    self.events.push(Event::Reset);
                    Ok(Vec::new())
                }
                _ => panic!("fake storage: class request {setup:?}"),
            });
        }
        if (setup.request_type, setup.request, setup.value)
            == (RECIPIENT_ENDPOINT, CLEAR_FEATURE, ENDPOINT_HALT)
        {
            let endpoint = setup.index as u8;
            self.uncleared.remove(&endpoint);
            self.events.push(Event::ClearHalt(endpoint));
        }
        Some(answer)
    }

    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
        if self.keyboard && endpoint == KEYBOARD_IN {
            return self.usb.data_in(endpoint, max_len);
        }
        if endpoint != BULK_IN {
            panic!("fake storage: IN transfer on endpoint {endpoint:#04x}");
        }
        if self.nak {
            return None;
        }
        if self.usb.is_halted(BULK_IN) {
            return Some(Err(Stall));
        }
        match &mut self.phase {
            Phase::Cbw => panic!("fake storage: IN transfer while waiting for a CBW"),
            Phase::DataOut { .. } => panic!("fake storage: IN transfer during a data-OUT phase"),
            Phase::DataIn {
                tag,
                data,
                sent,
                left,
                status,
                stall,
            } => {
                // More would take the CSW into the data (BOT 6.7.2).
                if max_len > *left as usize {
                    panic!(
                        "fake storage: IN transfer of {max_len} bytes, {left} left of the data phase"
                    );
                }
                if *stall {
                    let (tag, residue, status) = (*tag, *left, *status);
                    self.usb.stall_endpoint(BULK_IN);
                    self.phase = Phase::Csw {
                        tag,
                        residue,
                        status,
                    };
                    return Some(Err(Stall));
                }
                let n = max_len.min(data.len() - *sent);
                let chunk = data[*sent..*sent + n].to_vec();
                *sent += n;
                *left -= n as u32;
                if *sent == data.len() {
                    self.phase = Phase::Csw {
                        tag: *tag,
                        residue: *left,
                        status: *status,
                    };
                }
                Some(Ok(chunk))
            }
            Phase::Csw {
                tag,
                residue,
                status,
            } => {
                if max_len < CSW_LEN {
                    panic!("fake storage: CSW read of {max_len} bytes");
                }
                if self.stall_csw > 0 {
                    self.stall_csw -= 1;
                    self.usb.stall_endpoint(BULK_IN);
                    return Some(Err(Stall));
                }
                let residue = self.residue.take().unwrap_or(*residue);
                let (mut tag, mut signature) = (*tag, CSW_SIGNATURE);
                let bad = self.bad_csw.take();
                match bad {
                    Some(BadCsw::Signature) => signature = CBW_SIGNATURE,
                    Some(BadCsw::Tag) => tag = tag.wrapping_add(1),
                    _ => {}
                }
                let mut b = signature.to_le_bytes().to_vec();
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&residue.to_le_bytes());
                b.push(*status);
                if bad == Some(BadCsw::Short) {
                    b.truncate(CSW_LEN - 1);
                }
                self.phase = Phase::Cbw;
                Some(Ok(b))
            }
        }
    }

    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>> {
        if endpoint != BULK_OUT {
            panic!("fake storage: OUT transfer on endpoint {endpoint:#04x}");
        }
        if self.nak {
            return None;
        }
        if self.usb.is_halted(BULK_OUT) {
            return Some(Err(Stall));
        }
        match &mut self.phase {
            Phase::Cbw => self.cbw(data),
            Phase::DataIn { .. } => panic!("fake storage: OUT transfer during a data-IN phase"),
            Phase::Csw { .. } => panic!("fake storage: OUT transfer while the CSW is pending"),
            Phase::DataOut {
                tag,
                left,
                received,
                write,
                status,
                stall,
            } => {
                if *stall {
                    let (tag, residue, status) = (*tag, *left, *status);
                    self.usb.stall_endpoint(BULK_OUT);
                    self.phase = Phase::Csw {
                        tag,
                        residue,
                        status,
                    };
                    return Some(Err(Stall));
                }
                if data.len() > *left as usize {
                    panic!(
                        "fake storage: OUT transfer of {} bytes, {left} announced",
                        data.len()
                    );
                }
                received.extend_from_slice(data);
                *left -= data.len() as u32;
                if *left == 0 {
                    let (tag, status, write) = (*tag, *status, *write);
                    let received = std::mem::take(received);
                    if let Some(lba) = write {
                        self.write_blocks(lba, &received);
                    }
                    self.phase = Phase::Csw {
                        tag,
                        residue: 0,
                        status,
                    };
                }
            }
        }
        Some(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_requests_answer_and_a_reset_waits_for_both_halts() {
        let mut d = kingston_device();
        assert!(d.events().is_empty());
        assert_eq!(
            d.control(class_request(0xA1, GET_MAX_LUN, 0, 1), &[]),
            Some(Ok(vec![0]))
        );
        d.set_max_lun(None);
        assert_eq!(
            d.control(class_request(0xA1, GET_MAX_LUN, 0, 1), &[]),
            Some(Err(Stall))
        );
        // Both were recorded, the stalled one too.
        assert_eq!(d.events(), [Event::GetMaxLun, Event::GetMaxLun]);
        // A CBW whose CSW is never read, then a reset.
        send(
            &mut d,
            &cbw_bytes(1, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(
            d.control(class_request(0x21, RESET, 0, 0), &[]),
            Some(Ok(vec![]))
        );
        d.control(Setup::clear_halt(BULK_IN), &[]);
        d.control(Setup::clear_halt(BULK_OUT), &[]);
        send(
            &mut d,
            &cbw_bytes(2, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (2, 0, PASSED));
        assert_eq!(d.resets(), 1);
        assert_eq!(
            d.events()[3..6],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
    }

    #[test]
    fn a_failed_read_stalls_on_the_kingston_and_pads_on_qemu() {
        let mut d = kingston_device();
        d.never_ready();
        send(&mut d, &cbw_bytes(1, 512, true, &read_10_cdb(0, 1)));
        assert_eq!(d.data_in(BULK_IN, 512), Some(Err(Stall)));
        // Halted until the host clears it; then the CSW.
        assert_eq!(d.data_in(BULK_IN, 13), Some(Err(Stall)));
        d.control(Setup::clear_halt(BULK_IN), &[]);
        assert_eq!(read_csw(&mut d), (1, 512, FAILED));
        let q = FakeStorage::qemu(16);
        let mut d = q.borrow_mut();
        d.never_ready();
        send(&mut d, &cbw_bytes(1, 1024, true, &read_10_cdb(2, 2)));
        assert_eq!(receive(&mut d, 1024), [0; 1024]);
        assert_eq!(read_csw(&mut d), (1, 0, FAILED));
    }

    #[test]
    fn what_a_correct_host_never_does_panics() {
        let tur = [TEST_UNIT_READY, 0, 0, 0, 0, 0];
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, false, &tur)[..30]));
        assert!(msg.contains("CBW of 30 bytes"));
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[0] = 0;
            send(d, &b);
        });
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(7, 0, false, &tur));
            read_csw(d);
            send(d, &cbw_bytes(7, 0, false, &tur));
        });
        assert!(msg.contains("tag 0x7 again"));
        let msg = panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[13] = 1;
            send(d, &b);
        });
        assert!(msg.contains("LUN 1"));
        panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[])));
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[14] = 17;
            send(d, &b);
        });
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[12] = 0x40;
            send(d, &b);
        });
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[0xA0; 12])));
        assert!(msg.contains("unknown opcode 0xa0"));
        panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[TEST_UNIT_READY; 10])));
    }

    #[test]
    fn transfers_out_of_turn_panic() {
        let tur = [TEST_UNIT_READY, 0, 0, 0, 0, 0];
        let inq = [INQUIRY, 0, 0, 0, 36, 0];
        let msg = panics_with(|d| {
            d.data_in(BULK_IN, 13);
        });
        assert!(msg.contains("while waiting for a CBW"));
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(1, 0, false, &tur));
            send(d, &cbw_bytes(2, 0, false, &tur));
        });
        assert!(msg.contains("while the CSW is pending"));
        panics_with(|d| {
            send(d, &cbw_bytes(1, 36, true, &inq));
            send(d, &cbw_bytes(2, 0, false, &tur));
        });
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(1, 36, true, &inq));
            d.data_in(BULK_IN, 512);
        });
        assert!(msg.contains("IN transfer of 512 bytes, 36 left"));
        panics_with(|d| {
            let mut w = read_10_cdb(0, 1);
            w[0] = WRITE_10;
            send(d, &cbw_bytes(1, 512, false, &w));
            send(d, &[0; 1024]);
        });
        panics_with(|d| {
            send(d, &cbw_bytes(1, 0, false, &tur));
            d.data_in(BULK_IN, 12);
        });
        let msg = panics_with(|d| {
            d.control(class_request(0x21, RESET, 0, 0), &[]);
            d.control(Setup::clear_halt(BULK_IN), &[]);
            send(d, &cbw_bytes(1, 0, false, &tur));
        });
        assert!(msg.contains("before the halts of {2} were cleared"));
        panics_with(|d| {
            d.control(class_request(0xA1, GET_MAX_LUN, 1, 1), &[]);
        });
        panics_with(|d| {
            d.data_in(0x83, 8);
        });
    }
}
