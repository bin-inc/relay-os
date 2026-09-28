//! Bulk-Only Transport wrappers (USB Mass Storage Class Bulk-Only
//! Transport 1.0, §5): the 31-byte Command Block Wrapper the host sends on
//! bulk OUT before each command, and the 13-byte Command Status Wrapper the
//! device answers with on bulk IN after the data phase. The CSW comes from
//! the device, so every field is checked before it is believed (BOT 6.3).

use crate::UsbError;

/// A CBW is always 31 bytes (BOT 5.1).
pub const CBW_LEN: usize = 31;
/// A CSW is always 13 bytes (BOT 5.2).
pub const CSW_LEN: usize = 13;
/// `dCBWSignature`: "USBC" in little-endian order.
pub const CBW_SIGNATURE: u32 = 0x4342_5355;
/// `dCSWSignature`: "USBS" in little-endian order.
pub const CSW_SIGNATURE: u32 = 0x5342_5355;
/// The longest command block a CBW carries.
pub const MAX_CDB_LEN: usize = 16;

/// A Command Block Wrapper for LUN 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cbw<'a> {
    /// `dCBWTag`: the device returns it in the CSW.
    pub tag: u32,
    /// `dCBWDataTransferLength`: the bytes the host expects to move.
    pub data_length: u32,
    /// The data phase goes from the device to the host.
    pub dir_in: bool,
    /// The SCSI command block, 1 to 16 bytes (a longer one is cut off).
    pub cdb: &'a [u8],
}

impl Cbw<'_> {
    /// The CBW as it goes on the wire (BOT 5.1, table 5.1).
    pub fn to_bytes(&self) -> [u8; CBW_LEN] {
        let cdb = &self.cdb[..self.cdb.len().min(MAX_CDB_LEN)];
        let mut b = [0; CBW_LEN];
        b[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        b[4..8].copy_from_slice(&self.tag.to_le_bytes());
        b[8..12].copy_from_slice(&self.data_length.to_le_bytes());
        // bmCBWFlags: bit 7 is the direction, the rest is reserved.
        b[12] = if self.dir_in { 0x80 } else { 0 };
        // bCBWLUN stays 0.
        b[14] = cdb.len() as u8;
        b[15..15 + cdb.len()].copy_from_slice(cdb);
        b
    }
}

/// `bCSWStatus` (BOT 5.2, table 5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CswStatus {
    /// Command Passed.
    Passed,
    /// Command Failed: REQUEST SENSE says why.
    Failed,
    /// Phase Error: only a reset recovery helps (BOT 5.3.4).
    PhaseError,
}

/// A Command Status Wrapper that passed every check of BOT 6.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Csw {
    pub tag: u32,
    /// `dCSWDataResidue`: the bytes of the data phase not processed.
    pub residue: u32,
    pub status: CswStatus,
}

impl Csw {
    /// Checks what the device sent for the CBW with `expected_tag` and
    /// `data_length` (BOT 6.3): a CSW is valid if it is 13 bytes with the
    /// signature and that tag, and meaningful if its status is 0 or 1 with a
    /// residue of at most the data length, or 2 (a phase error, whose residue
    /// means nothing). Anything else is a `Protocol` error naming the check.
    pub fn parse(bytes: &[u8], expected_tag: u32, data_length: u32) -> Result<Csw, UsbError> {
        let Ok(b) = <&[u8; CSW_LEN]>::try_from(bytes) else {
            return Err(UsbError::Protocol("CSW not 13 bytes"));
        };
        let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if le32(0) != CSW_SIGNATURE {
            return Err(UsbError::Protocol("bad CSW signature"));
        }
        let tag = le32(4);
        if tag != expected_tag {
            return Err(UsbError::Protocol("CSW tag does not match its CBW"));
        }
        let residue = le32(8);
        let status = match b[12] {
            0 => CswStatus::Passed,
            1 => CswStatus::Failed,
            2 => CswStatus::PhaseError,
            _ => return Err(UsbError::Protocol("bad CSW status")),
        };
        if status != CswStatus::PhaseError && residue > data_length {
            return Err(UsbError::Protocol("CSW residue over the data length"));
        }
        Ok(Csw {
            tag,
            residue,
            status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csw(signature: u32, tag: u32, residue: u32, status: u8) -> [u8; CSW_LEN] {
        let mut b = [0; CSW_LEN];
        b[0..4].copy_from_slice(&signature.to_le_bytes());
        b[4..8].copy_from_slice(&tag.to_le_bytes());
        b[8..12].copy_from_slice(&residue.to_le_bytes());
        b[12] = status;
        b
    }

    #[test]
    fn a_read_cbw_is_byte_exact() {
        // READ(10) of 128 blocks at LBA 0x12345678: 64 KiB in.
        let cdb = [0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0];
        let cbw = Cbw {
            tag: 0x0102_0304,
            data_length: 0x1_0000,
            dir_in: true,
            cdb: &cdb,
        };
        assert_eq!(
            cbw.to_bytes(),
            [
                0x55, 0x53, 0x42, 0x43, // "USBC"
                0x04, 0x03, 0x02, 0x01, // tag
                0x00, 0x00, 0x01, 0x00, // 65536 bytes
                0x80, // data in
                0x00, // LUN 0
                10,   // CB length
                0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0, // the CDB
                0, 0, 0, 0, 0, 0, // padded to 16
            ]
        );
    }

    #[test]
    fn an_out_cbw_and_one_without_data_have_flags_0() {
        let cdb = [0x2A, 0, 0, 0, 0, 1, 0, 0, 1, 0];
        let b = Cbw {
            tag: 7,
            data_length: 512,
            dir_in: false,
            cdb: &cdb,
        }
        .to_bytes();
        assert_eq!((&b[8..12], b[12], b[14]), (&[0, 2, 0, 0][..], 0, 10));
        let b = Cbw {
            tag: 8,
            data_length: 0,
            dir_in: false,
            cdb: &[0; 6],
        }
        .to_bytes();
        assert_eq!((b[4], b[12], b[13], b[14]), (8, 0, 0, 6));
        assert!(b[15..].iter().all(|&x| x == 0));
    }

    #[test]
    fn a_16_byte_cdb_fills_the_block_and_a_longer_one_is_cut() {
        let cdb: [u8; 16] = core::array::from_fn(|i| i as u8 + 1);
        let b = Cbw {
            tag: 1,
            data_length: 32,
            dir_in: true,
            cdb: &cdb,
        }
        .to_bytes();
        assert_eq!((b[14], &b[15..]), (16, &cdb[..]));
        let long = [0xAA; 20];
        let b = Cbw {
            tag: 1,
            data_length: 0,
            dir_in: false,
            cdb: &long,
        }
        .to_bytes();
        assert_eq!((b[14], &b[15..]), (16, &long[..16]));
    }

    #[test]
    fn a_good_csw_parses() {
        let b = csw(CSW_SIGNATURE, 42, 0, 0);
        assert_eq!(
            Csw::parse(&b, 42, 512),
            Ok(Csw {
                tag: 42,
                residue: 0,
                status: CswStatus::Passed
            })
        );
        let b = csw(CSW_SIGNATURE, 43, 512, 1);
        assert_eq!(
            Csw::parse(&b, 43, 512).map(|c| (c.residue, c.status)),
            Ok((512, CswStatus::Failed))
        );
        // A phase error's residue means nothing (BOT 6.3.2), so it is not
        // checked.
        let b = csw(CSW_SIGNATURE, 44, u32::MAX, 2);
        assert_eq!(
            Csw::parse(&b, 44, 0).map(|c| c.status),
            Ok(CswStatus::PhaseError)
        );
    }

    #[test]
    fn each_csw_check_fails_on_its_own() {
        let good = csw(CSW_SIGNATURE, 5, 0, 0);
        assert!(Csw::parse(&good, 5, 0).is_ok());
        let err = |b: &[u8], tag, len| match Csw::parse(b, tag, len) {
            Err(UsbError::Protocol(what)) => what,
            other => panic!("expected a protocol error, got {other:?}"),
        };
        assert_eq!(err(&good[..12], 5, 0), "CSW not 13 bytes");
        let mut long = good.to_vec();
        long.push(0);
        assert_eq!(err(&long, 5, 0), "CSW not 13 bytes");
        assert_eq!(err(&[], 5, 0), "CSW not 13 bytes");
        assert_eq!(err(&csw(CBW_SIGNATURE, 5, 0, 0), 5, 0), "bad CSW signature");
        assert_eq!(err(&good, 6, 0), "CSW tag does not match its CBW");
        assert_eq!(err(&csw(CSW_SIGNATURE, 5, 0, 3), 5, 0), "bad CSW status");
        assert_eq!(err(&csw(CSW_SIGNATURE, 5, 0, 0xFF), 5, 0), "bad CSW status");
        assert_eq!(
            err(&csw(CSW_SIGNATURE, 5, 513, 0), 5, 512),
            "CSW residue over the data length"
        );
        assert_eq!(
            err(&csw(CSW_SIGNATURE, 5, 1, 1), 5, 0),
            "CSW residue over the data length"
        );
        // The residue may be the whole length.
        assert!(Csw::parse(&csw(CSW_SIGNATURE, 5, 512, 0), 5, 512).is_ok());
    }
}
