//! Reading and writing a disk (spec §6.4, SBC-3 §5.8, §5.32, §5.22):
//! READ(10) and WRITE(10) of at most [`MAX_BULK`] bytes each, and
//! SYNCHRONIZE CACHE(10) for `flush`. A request must be whole blocks inside
//! the disk, or nothing is sent.
//!
//! A disk whose command times out on every try (its firmware hangs: it
//! stays connected and NAKs everything) is given up: each later request
//! would cost three 5 s timeouts, and the block cache writes every dirty
//! block again on each sync, so one shell command would take minutes. From
//! then on every request fails at once with `Timeout`, until the stick is
//! plugged in again (a new `MassStorage`).

use super::disk::MassStorage;
use super::scsi::{self, ILLEGAL_REQUEST};
use super::transport::{Data, Need};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK};

/// How a request moves: READ(10) into the buffer or WRITE(10) from it.
enum Request<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl MassStorage {
    /// Reads `buf.len() / block_size` blocks from `lba`: READ(10) commands
    /// of at most MAX_BULK bytes each. `buf` must be whole blocks inside
    /// the disk (`Unsupported` otherwise, nothing sent). A disk given up as
    /// not answering fails at once with `Timeout`.
    pub fn read(&mut self, bus: &mut dyn Bus, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
        self.request(bus, lba, Request::Read(buf))
    }

    /// Writes `buf` to the blocks from `lba`, as `read`. A failure in one
    /// command fails the request; the commands before it were written.
    pub fn write(&mut self, bus: &mut dyn Bus, lba: u64, buf: &[u8]) -> Result<(), UsbError> {
        self.request(bus, lba, Request::Write(buf))
    }

    /// SYNCHRONIZE CACHE(10) over the whole disk. A device that does not
    /// support it (ILLEGAL REQUEST) has no cache to flush: that is
    /// success, logged once. A disk given up as not answering fails at
    /// once with `Timeout`.
    pub fn flush(&mut self, bus: &mut dyn Bus) -> Result<(), UsbError> {
        if self.not_answering {
            return Err(UsbError::Timeout);
        }
        if self.no_cache {
            return Ok(());
        }
        let cdb = scsi::synchronize_cache_10();
        match self.command(bus, &cdb, Data::None, Need::UpTo) {
            Err(UsbError::Sense(s)) if s.key == ILLEGAL_REQUEST => {
                slog!(
                    bus,
                    self.slot(),
                    "SYNCHRONIZE CACHE not supported: no cache to flush"
                );
                self.no_cache = true;
                Ok(())
            }
            r => r.map(|_| ()),
        }
    }

    /// One command with its tries; a device that did not answer any of
    /// them is given up (logged once).
    fn command(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let r = self.transport.command(bus, cdb, data, need);
        if r == Err(UsbError::Timeout) {
            slog!(
                bus,
                self.slot(),
                "not answering; given up until it is plugged in again"
            );
            self.not_answering = true;
        }
        r
    }

    /// Checks the request, then sends one command per MAX_BULK bytes (a
    /// multiple of every block size the disk may have).
    fn request(&mut self, bus: &mut dyn Bus, lba: u64, req: Request) -> Result<(), UsbError> {
        let len = match &req {
            Request::Read(b) => b.len(),
            Request::Write(b) => b.len(),
        };
        if self.not_answering {
            return Err(UsbError::Timeout);
        }
        let size = self.block_size;
        if !len.is_multiple_of(size) {
            return Err(UsbError::Unsupported("not whole blocks"));
        }
        match lba.checked_add((len / size) as u64) {
            Some(end) if end <= self.block_count => {}
            _ => return Err(UsbError::Unsupported("past the end of the disk")),
        }
        let per_command = (MAX_BULK / size) as u64;
        // Command `i` of `bytes`: its LBA is inside the disk, whose at most
        // 2^32 blocks READ(10) reaches (`start` checked), so it fits 32
        // bits; and at most MAX_BULK / 512 blocks fit 16.
        let command = |i: usize, bytes: usize| {
            let at = lba + i as u64 * per_command;
            (at as u32, (bytes / size) as u16)
        };
        match req {
            Request::Read(buf) => {
                for (i, part) in buf.chunks_mut(MAX_BULK).enumerate() {
                    let (at, blocks) = command(i, part.len());
                    let cdb = scsi::read_10(at, blocks);
                    self.command(bus, &cdb, Data::In(part), Need::All)?;
                }
            }
            Request::Write(buf) => {
                for (i, part) in buf.chunks(MAX_BULK).enumerate() {
                    let (at, blocks) = command(i, part.len());
                    let cdb = scsi::write_10(at, blocks);
                    self.command(bus, &cdb, Data::Out(part), Need::All)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Sense;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op};
    use crate::xhci::Xhci;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::time::Duration;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn started(stick: &Stick) -> (FakeHal, Xhci<FakeHal>, MassStorage) {
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, stick);
        let disk = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]).unwrap();
        (hal, xhci, disk)
    }

    fn kingston() -> (FakeHal, Xhci<FakeHal>, MassStorage, Stick) {
        let stick = FakeStorage::kingston();
        let (hal, xhci, disk) = started(&stick);
        (hal, xhci, disk, stick)
    }

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 512) as u8 ^ seed).collect()
    }

    /// The READ(10) or WRITE(10) commands sent after setup: LBA, blocks
    /// and bytes.
    fn io(stick: &Stick, opcode: u8) -> Vec<(u64, u32, u32)> {
        let c = stick.borrow().commands();
        c.iter()
            .filter(|c| c.opcode == opcode)
            .map(|c| (c.lba, c.blocks, c.data_length))
            .collect()
    }

    #[test]
    fn what_is_written_is_read_back() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let data = pattern(3 * 512, 1);
        disk.write(&mut xhci, 10, &data).unwrap();
        assert_eq!(stick.borrow().read_blocks(10, 3), data);
        let mut buf = vec![0u8; 3 * 512];
        disk.read(&mut xhci, 10, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(io(&stick, op::WRITE_10), [(10, 3, 1536)]);
        assert_eq!(io(&stick, op::READ_10), [(10, 3, 1536)]);
    }

    #[test]
    fn a_200_kib_request_is_split_into_64_kib_commands() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let data = pattern(200 * 1024, 2);
        disk.write(&mut xhci, 1000, &data).unwrap();
        let chunks = [
            (1000, 128, 65536),
            (1128, 128, 65536),
            (1256, 128, 65536),
            (1384, 16, 8192),
        ];
        assert_eq!(io(&stick, op::WRITE_10), chunks);
        let mut buf = vec![0u8; 200 * 1024];
        disk.read(&mut xhci, 1000, &mut buf).unwrap();
        assert_eq!(io(&stick, op::READ_10), chunks);
        assert!(buf == data);
    }

    #[test]
    fn blocks_of_4096_bytes_go_16_to_a_command() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().set_capacity(1000, 4096);
        let (_hal, mut xhci, mut disk) = started(&stick);
        let data = pattern(20 * 4096, 3);
        disk.write(&mut xhci, 980, &data).unwrap();
        assert_eq!(
            io(&stick, op::WRITE_10),
            [(980, 16, 65536), (996, 4, 16384)]
        );
        let mut buf = vec![0u8; 20 * 4096];
        disk.read(&mut xhci, 980, &mut buf).unwrap();
        assert!(buf == data);
    }

    #[test]
    fn the_first_and_the_last_block_can_be_used() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let last = disk.block_count() - 1;
        assert_eq!(last, 30_277_631);
        for lba in [0, last] {
            let data = pattern(512, lba as u8);
            disk.write(&mut xhci, lba, &data).unwrap();
            let mut buf = vec![0u8; 512];
            disk.read(&mut xhci, lba, &mut buf).unwrap();
            assert_eq!(buf, data);
        }
        assert_eq!(io(&stick, op::READ_10), [(0, 1, 512), (last, 1, 512)]);
    }

    #[test]
    fn requests_that_are_not_whole_blocks_inside_the_disk_send_nothing() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let count = disk.block_count();
        let sent = stick.borrow().events().len();
        let not_whole = Err(UsbError::Unsupported("not whole blocks"));
        let past = Err(UsbError::Unsupported("past the end of the disk"));
        let mut buf = vec![0u8; 1024];
        assert_eq!(disk.read(&mut xhci, 0, &mut buf[..511]), not_whole);
        assert_eq!(disk.write(&mut xhci, 0, &buf[..513]), not_whole);
        assert_eq!(disk.read(&mut xhci, count, &mut buf[..512]), past);
        assert_eq!(disk.read(&mut xhci, count - 1, &mut buf), past);
        assert_eq!(disk.write(&mut xhci, count - 1, &buf), past);
        assert_eq!(disk.read(&mut xhci, u64::MAX, &mut buf[..512]), past);
        assert_eq!(disk.write(&mut xhci, u64::MAX - 1, &buf), past);
        // Nothing to do is nothing sent.
        assert_eq!(disk.read(&mut xhci, count, &mut []), Ok(()));
        assert_eq!(disk.write(&mut xhci, 5, &[]), Ok(()));
        assert_eq!(stick.borrow().events().len(), sent);
    }

    #[test]
    fn a_failed_command_fails_the_request_after_the_ones_before_it() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().medium_error_at(250);
        let mut buf = vec![0u8; 200 * 1024];
        let medium_error = Sense {
            key: 3,
            asc: 0x11,
            ascq: 0,
        };
        assert_eq!(
            disk.read(&mut xhci, 100, &mut buf),
            Err(UsbError::Sense(medium_error))
        );
        // The first command read LBA 100-227; the second has the bad
        // block and is tried three times; nothing after it is sent.
        assert_eq!(
            io(&stick, op::READ_10),
            [
                (100, 128, 65536),
                (228, 128, 65536),
                (228, 128, 65536),
                (228, 128, 65536)
            ]
        );
        assert!(hal.log_text().contains(
            "storage: slot 1: READ(10) at LBA 228, 128 blocks: MEDIUM ERROR (asc 0x11, ascq 0x00) (try 3 of 3)"
        ));
    }

    #[test]
    fn a_write_protected_stick_fails_writes_with_its_sense() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().write_protect();
        let protected = Sense {
            key: 7,
            asc: 0x27,
            ascq: 0,
        };
        assert_eq!(
            disk.write(&mut xhci, 8, &[0xAA; 512]),
            Err(UsbError::Sense(protected))
        );
        assert_eq!(io(&stick, op::WRITE_10).len(), 3);
        assert_eq!(stick.borrow().read_blocks(8, 1), [0; 512]);
        // Reads still work.
        let mut buf = [0u8; 512];
        assert_eq!(disk.read(&mut xhci, 8, &mut buf), Ok(()));
    }

    #[test]
    fn flush_synchronizes_the_cache() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        assert_eq!(disk.flush(&mut xhci), Ok(()));
        let sync = stick.borrow().commands();
        let sync = sync.last().unwrap();
        assert_eq!(
            (sync.opcode, sync.lba, sync.blocks),
            (op::SYNCHRONIZE_CACHE_10, 0, 0)
        );
    }

    #[test]
    fn a_stick_without_synchronize_cache_has_nothing_to_flush() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().no_synchronize_cache();
        for _ in 0..3 {
            assert_eq!(disk.flush(&mut xhci), Ok(()));
        }
        // Asked once (ILLEGAL REQUEST is not tried again), then not at all.
        let syncs = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::SYNCHRONIZE_CACHE_10)
            .count();
        assert_eq!(syncs, 1);
        let log = hal.log_text();
        assert_eq!(
            log.matches("SYNCHRONIZE CACHE not supported: no cache to flush")
                .count(),
            1
        );
    }

    #[test]
    fn a_flush_that_fails_otherwise_is_an_error() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().never_ready();
        let not_ready = Sense {
            key: 2,
            asc: 4,
            ascq: 1,
        };
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Sense(not_ready)));
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Sense(not_ready)));
    }

    #[test]
    fn unplugging_during_a_request_fails_it_at_once() {
        let (hal, mut xhci, mut disk, _stick) = kingston();
        // How long one command takes, to unplug the stick during the
        // second command of the next request.
        let mut buf = vec![0u8; 200 * 1024];
        let before = hal.clock();
        disk.read(&mut xhci, 0, &mut buf[..MAX_BULK]).unwrap();
        let one = hal.clock() - before;
        hal.fake().after(one + one / 2, |x, _| x.unplug(13));
        let before = hal.clock();
        assert_eq!(
            disk.read(&mut xhci, 0, &mut buf),
            Err(UsbError::Disconnected)
        );
        let took = hal.clock() - before;
        assert!(
            took > one && took < 3 * one,
            "{took:?}, one command {one:?}"
        );
        // Neither tried again nor recovered.
        let log = hal.log_text();
        assert!(!log.contains("(try") && !log.contains("reset recovery"));
        assert_eq!(
            disk.write(&mut xhci, 0, &buf[..512]),
            Err(UsbError::Disconnected)
        );
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_disk_that_stops_answering_is_given_up() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().nak(true);
        let mut buf = vec![0u8; 512];
        let before = hal.clock();
        assert_eq!(disk.read(&mut xhci, 5, &mut buf), Err(UsbError::Timeout));
        let took = hal.clock() - before;
        assert!(
            took >= Duration::from_secs(15) && took < Duration::from_secs(17),
            "{took:?}"
        );
        let log = hal.log_text();
        assert!(
            log.contains("storage: slot 1: not answering; given up until it is plugged in again")
        );
        // From now on nothing is sent, even once the device would answer.
        stick.borrow_mut().nak(false);
        let requests = hal.fake().requests().len();
        let seen = stick.borrow().events().len();
        let mut bus = TamperBus::new(&mut xhci);
        for _ in 0..2 {
            let before = hal.clock();
            assert_eq!(disk.read(&mut bus, 5, &mut buf), Err(UsbError::Timeout));
            assert_eq!(disk.write(&mut bus, 5, &buf), Err(UsbError::Timeout));
            assert_eq!(disk.flush(&mut bus), Err(UsbError::Timeout));
            assert!(hal.clock() - before < Duration::from_millis(1));
        }
        assert_eq!(bus.bulk, 0, "no bulk transfer");
        assert_eq!(hal.fake().requests().len(), requests, "no control request");
        assert_eq!(
            stick.borrow().events().len(),
            seen,
            "the device saw nothing"
        );
        assert_eq!(hal.log_text().matches("given up").count(), 1, "logged once");
    }

    #[test]
    fn a_disk_that_answers_with_an_error_is_not_given_up() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().medium_error_at(5);
        stick.borrow_mut().write_protect();
        let mut buf = vec![0u8; 512];
        assert!(matches!(
            disk.read(&mut xhci, 5, &mut buf),
            Err(UsbError::Sense(_))
        ));
        assert!(matches!(
            disk.write(&mut xhci, 6, &buf),
            Err(UsbError::Sense(_))
        ));
        assert_eq!(disk.read(&mut xhci, 6, &mut buf), Ok(()));
        assert_eq!(disk.flush(&mut xhci), Ok(()));
    }

    #[test]
    fn a_gone_device_is_not_given_up_as_not_answering() {
        let (hal, mut xhci, mut disk, _stick) = kingston();
        let mut bus = TamperBus::new(&mut xhci);
        bus.fail_bulk = Some((0, UsbError::Disconnected));
        let mut buf = vec![0u8; 512];
        assert_eq!(
            disk.read(&mut bus, 5, &mut buf),
            Err(UsbError::Disconnected)
        );
        assert!(!hal.log_text().contains("given up"));
    }
}
