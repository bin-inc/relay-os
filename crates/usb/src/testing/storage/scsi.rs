//! The fake stick's SCSI side (SPC-4, SBC-3): a command block is checked
//! against its CBW, then answered from the in-memory disk and the knobs:
//! INQUIRY, TEST UNIT READY, REQUEST SENSE (fixed format), READ CAPACITY(10)
//! and (16), READ(10), WRITE(10) and SYNCHRONIZE CACHE(10).

use super::*;

/// The operation codes the fake knows (SPC-4, SBC-3), for tests to name.
pub mod op {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const INQUIRY: u8 = 0x12;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const WRITE_10: u8 = 0x2A;
    pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
    pub const SERVICE_ACTION_IN_16: u8 = 0x9E;
}
use op::*;

/// Sense key, ASC and ASCQ.
pub(super) type Sense = (u8, u8, u8);
pub(super) const NO_SENSE: Sense = (0, 0, 0);
const BECOMING_READY: Sense = (0x02, 0x04, 0x01);
const MEDIUM_NOT_PRESENT: Sense = (0x02, 0x3A, 0x00);
const POWER_ON_RESET: Sense = (0x06, 0x29, 0x00);

/// The Kingston DataTraveler 3.0's INQUIRY data, as Linux read it from the
/// NUC's stick (`/sys/block/sda/device/inquiry`).
pub const KINGSTON_INQUIRY: [u8; 62] = [
    0x00, 0x80, 0x06, 0x02, 0x39, 0x00, 0x00, 0x00, //
    b'K', b'i', b'n', b'g', b's', b't', b'o', b'n', //
    b'D', b'a', b't', b'a', b'T', b'r', b'a', b'v', //
    b'e', b'l', b'e', b'r', b' ', b'3', b'.', b'0', //
    b'P', b'M', b'A', b'P', //
    0x50, 0x4d, 0x41, 0x50, 0x31, 0x32, 0x33, 0x34, 0x87, 0x5b, 0x82, 0xc9, 0x22, 0xb4, 0x96, 0x9e,
    0xa5, 0x84, 0xe9, 0x8f, 0xbc, 0xe1, 0x04, 0x60, 0x04, 0xc0,
];

/// The Kingston stick's size: 30,277,632 blocks of 512 bytes.
pub const KINGSTON_BLOCKS: u64 = 30_277_632;

/// QEMU 8.2's scsi-disk INQUIRY data: a disk, not removable, SPC-3, with
/// QEMU's names and version.
pub(super) fn qemu_inquiry() -> Vec<u8> {
    let mut b = vec![0x00, 0x00, 0x05, 0x12, 31, 0x00, 0x00, 0x10];
    b.extend_from_slice(b"QEMU    QEMU HARDDISK   2.5+");
    b
}

/// What a command answers: its data phase.
pub(super) enum Answer {
    None,
    In(Vec<u8>),
    /// WRITE(10) data to store at this LBA.
    Out(u64),
}

impl FakeStorage {
    /// Checks the command block against the CBW and runs it.
    pub(super) fn command(&mut self, tag: u32, length: u32, dir_in: bool, cdb: &[u8]) {
        let opcode = cdb[0];
        let size = match opcode {
            TEST_UNIT_READY | REQUEST_SENSE | INQUIRY => 6,
            READ_CAPACITY_10 | READ_10 | WRITE_10 | SYNCHRONIZE_CACHE_10 => 10,
            SERVICE_ACTION_IN_16 => 16,
            _ => panic!("fake storage: unknown opcode {opcode:#04x}"),
        };
        if cdb.len() != size {
            panic!(
                "fake storage: {opcode:#04x} with a {}-byte command block",
                cdb.len()
            );
        }
        let be16 = |i: usize| u16::from_be_bytes([cdb[i], cdb[i + 1]]) as u32;
        let be32 = |i: usize| u32::from_be_bytes([cdb[i], cdb[i + 1], cdb[i + 2], cdb[i + 3]]);
        let (lba, blocks) = match opcode {
            READ_10 | WRITE_10 | SYNCHRONIZE_CACHE_10 => (be32(2) as u64, be16(7)),
            _ => (0, 0),
        };
        // The data phase the command needs: bytes and direction.
        let (want, want_in) = match opcode {
            REQUEST_SENSE => (cdb[4] as u64, true),
            INQUIRY => {
                if cdb[1] & 1 != 0 || cdb[2] != 0 {
                    panic!("fake storage: INQUIRY of a VPD page is not modelled");
                }
                (be16(3) as u64, true)
            }
            READ_CAPACITY_10 => (8, true),
            SERVICE_ACTION_IN_16 => {
                if cdb[1] & 0x1F != 0x10 {
                    panic!("fake storage: service action {:#04x}", cdb[1] & 0x1F);
                }
                (be32(10) as u64, true)
            }
            READ_10 | WRITE_10 => {
                if blocks == 0 {
                    panic!("fake storage: {opcode:#04x} of 0 blocks");
                }
                (blocks as u64 * self.block_size as u64, opcode == READ_10)
            }
            _ => (0, false),
        };
        if lba + blocks as u64 > self.blocks
            || (opcode == SYNCHRONIZE_CACHE_10 && lba >= self.blocks)
        {
            panic!(
                "fake storage: {opcode:#04x} of {blocks} blocks at LBA {lba} past the end ({} blocks)",
                self.blocks
            );
        }
        if length as u64 != want || (want > 0 && dir_in != want_in) {
            panic!(
                "fake storage: CBW for {opcode:#04x} announces {length} bytes {}, the command moves {want} {}",
                if dir_in { "in" } else { "out" },
                if want_in { "in" } else { "out" }
            );
        }
        self.events.push(Event::Command(Command {
            opcode,
            tag,
            data_length: length,
            lba,
            blocks,
        }));
        if std::mem::take(&mut self.phase_error) {
            return self.begin(tag, length, dir_in, Answer::None, PHASE_ERROR);
        }
        match self.answer(opcode, lba, blocks) {
            Ok(answer) => {
                if opcode != REQUEST_SENSE {
                    self.sense = NO_SENSE;
                }
                self.begin(tag, length, dir_in, answer, PASSED);
            }
            Err(sense) => {
                self.sense = sense;
                self.begin(tag, length, dir_in, Answer::None, FAILED);
            }
        }
    }

    /// What the command answers, or the sense it fails with.
    fn answer(&mut self, opcode: u8, lba: u64, blocks: u32) -> Result<Answer, Sense> {
        match opcode {
            // Neither reports a unit attention (SPC-4 §5.14); REQUEST
            // SENSE hands it out and clears it.
            REQUEST_SENSE => {
                let (key, asc, ascq) = if std::mem::take(&mut self.unit_attention) {
                    POWER_ON_RESET
                } else {
                    std::mem::replace(&mut self.sense, NO_SENSE)
                };
                let mut b = vec![0u8; 18];
                (b[0], b[2], b[7], b[12], b[13]) = (0x70, key, 10, asc, ascq);
                return Ok(Answer::In(b));
            }
            INQUIRY => return Ok(Answer::In(self.inquiry.clone())),
            _ => {}
        }
        if std::mem::take(&mut self.unit_attention) {
            return Err(POWER_ON_RESET);
        }
        if self.no_medium {
            return Err(MEDIUM_NOT_PRESENT);
        }
        if self.never_ready || self.not_ready > 0 {
            if opcode == TEST_UNIT_READY {
                self.not_ready = self.not_ready.saturating_sub(1);
            }
            return Err(BECOMING_READY);
        }
        let last = self.blocks - 1;
        Ok(match opcode {
            READ_CAPACITY_10 => {
                let mut b = (last.min(0xFFFF_FFFF) as u32).to_be_bytes().to_vec();
                b.extend_from_slice(&self.block_size.to_be_bytes());
                Answer::In(b)
            }
            SERVICE_ACTION_IN_16 => {
                let mut b = last.to_be_bytes().to_vec();
                b.extend_from_slice(&self.block_size.to_be_bytes());
                b.resize(32, 0);
                Answer::In(b)
            }
            READ_10 => Answer::In(self.read_blocks(lba, blocks as u64)),
            WRITE_10 => Answer::Out(lba),
            _ => Answer::None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inquiry_goes_cbw_data_csw_and_is_truncated_to_the_allocation() {
        let mut d = kingston_device();
        send(&mut d, &cbw_bytes(1, 36, true, &[INQUIRY, 0, 0, 0, 36, 0]));
        assert_eq!(receive(&mut d, 36), KINGSTON_INQUIRY[..36]);
        assert_eq!(read_csw(&mut d), (1, 0, PASSED));
        // More asked than the device has: a short data phase, the rest is
        // the residue.
        send(
            &mut d,
            &cbw_bytes(2, 100, true, &[INQUIRY, 0, 0, 0, 100, 0]),
        );
        assert_eq!(receive(&mut d, 100).len(), 62);
        assert_eq!(read_csw(&mut d), (2, 38, PASSED));
        assert_eq!(d.opcodes(), [INQUIRY, INQUIRY]);
    }

    #[test]
    fn capacity_is_capped_at_0xffffffff_for_read_capacity_10() {
        let mut d = kingston_device();
        send(
            &mut d,
            &cbw_bytes(1, 8, true, &[READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
        assert_eq!(receive(&mut d, 8), [0x01, 0xCD, 0xFF, 0xFF, 0, 0, 2, 0]);
        read_csw(&mut d);
        d.set_capacity(1 << 33, 4096);
        send(
            &mut d,
            &cbw_bytes(2, 8, true, &[READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
        assert_eq!(receive(&mut d, 8), [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]);
        read_csw(&mut d);
        let mut rc16 = [0u8; 16];
        (rc16[0], rc16[1], rc16[13]) = (SERVICE_ACTION_IN_16, 0x10, 32);
        send(&mut d, &cbw_bytes(3, 32, true, &rc16));
        let b = receive(&mut d, 32);
        assert_eq!(b[..12], [0, 0, 0, 1, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]);
        assert_eq!(read_csw(&mut d), (3, 0, PASSED));
    }

    #[test]
    fn a_unit_attention_fails_the_next_command_and_request_sense_clears_it() {
        // QEMU's TEST UNIT READY passes at once...
        let q = FakeStorage::qemu(2048);
        let mut d = q.borrow_mut();
        send(
            &mut d,
            &cbw_bytes(9, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (9, 0, PASSED));
        // ...unless a unit attention is pending. INQUIRY does not report it.
        d.unit_attention();
        send(&mut d, &cbw_bytes(1, 36, true, &[INQUIRY, 0, 0, 0, 36, 0]));
        assert_eq!(&receive(&mut d, 36)[8..], b"QEMU    QEMU HARDDISK   2.5+");
        read_csw(&mut d);
        send(
            &mut d,
            &cbw_bytes(2, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (2, 0, FAILED));
        send(
            &mut d,
            &cbw_bytes(3, 18, true, &[REQUEST_SENSE, 0, 0, 0, 18, 0]),
        );
        let sense = receive(&mut d, 18);
        assert_eq!(
            (sense[0], sense[2], sense[12], sense[13]),
            (0x70, 6, 0x29, 0)
        );
        read_csw(&mut d);
        send(
            &mut d,
            &cbw_bytes(4, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (4, 0, PASSED));
    }

    #[test]
    fn writes_are_stored_and_read_back() {
        let mut d = kingston_device();
        d.set_capacity(64, 512);
        let data: Vec<u8> = (0..1024).map(|i| i as u8).collect();
        let mut w = read_10_cdb(62, 2);
        w[0] = WRITE_10;
        send(&mut d, &cbw_bytes(1, 1024, false, &w));
        // In two transfers.
        send(&mut d, &data[..512]);
        send(&mut d, &data[512..]);
        assert_eq!(read_csw(&mut d), (1, 0, PASSED));
        send(&mut d, &cbw_bytes(2, 1024, true, &read_10_cdb(62, 2)));
        assert_eq!(receive(&mut d, 1024), data);
        read_csw(&mut d);
        assert_eq!(d.read_blocks(62, 2), data);
        let c = d.commands();
        assert_eq!((c[0].opcode, c[0].lba, c[0].blocks), (WRITE_10, 62, 2));
    }

    #[test]
    fn data_phases_that_do_not_match_the_command_panic() {
        // Lengths and directions: as allocated, or blocks × block size.
        panics_with(|d| send(d, &cbw_bytes(1, 35, true, &[INQUIRY, 0, 0, 0, 36, 0])));
        panics_with(|d| send(d, &cbw_bytes(1, 36, false, &[INQUIRY, 0, 0, 0, 36, 0])));
        panics_with(|d| send(d, &cbw_bytes(1, 512, false, &read_10_cdb(0, 1))));
        panics_with(|d| send(d, &cbw_bytes(1, 1024, true, &read_10_cdb(0, 1))));
        panics_with(|d| {
            send(
                d,
                &cbw_bytes(1, 512, true, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
            )
        });
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, true, &read_10_cdb(0, 0))));
        assert!(msg.contains("of 0 blocks"));
        let msg = panics_with(|d| {
            d.set_capacity(100, 512);
            send(d, &cbw_bytes(1, 1024, true, &read_10_cdb(99, 2)));
        });
        assert!(msg.contains("past the end"));
        panics_with(|d| {
            d.set_capacity(100, 512);
            send(
                d,
                &cbw_bytes(
                    1,
                    0,
                    false,
                    &[SYNCHRONIZE_CACHE_10, 0, 0, 0, 0, 100, 0, 0, 0, 0],
                ),
            );
        });
    }
}
