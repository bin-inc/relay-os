//! The SCSI commands a USB stick needs (SPC-4, SBC-3): command blocks for
//! INQUIRY, TEST UNIT READY, REQUEST SENSE, READ CAPACITY(10) and (16),
//! READ(10), WRITE(10) and SYNCHRONIZE CACHE(10), and parsers for what the
//! device answers. The answers come from the device, so short or garbled
//! data is an error or a `?`, never a panic.

use crate::UsbError;
use alloc::string::String;
use core::fmt;

/// Operation codes (SPC-4 §6, SBC-3 §5).
pub const TEST_UNIT_READY: u8 = 0x00;
pub const REQUEST_SENSE: u8 = 0x03;
pub const INQUIRY: u8 = 0x12;
pub const READ_CAPACITY_10: u8 = 0x25;
pub const READ_10: u8 = 0x28;
pub const WRITE_10: u8 = 0x2A;
pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
pub const SERVICE_ACTION_IN_16: u8 = 0x9E;
/// SERVICE ACTION IN(16)'s service action for READ CAPACITY(16).
pub const READ_CAPACITY_16: u8 = 0x10;

/// The standard INQUIRY data up to the revision (SPC-4 §6.4.2).
pub const INQUIRY_LEN: usize = 36;
/// Fixed-format sense data without additional bytes (SPC-4 §4.5.3).
pub const SENSE_LEN: usize = 18;
/// READ CAPACITY(10) data (SBC-3 §5.15.2).
pub const CAPACITY_10_LEN: usize = 8;
/// READ CAPACITY(16) data (SBC-3 §5.16.2).
pub const CAPACITY_16_LEN: usize = 32;

/// Sense keys (SPC-4 table 49) the driver acts on.
pub const NOT_READY: u8 = 0x02;
pub const MEDIUM_ERROR: u8 = 0x03;
pub const ILLEGAL_REQUEST: u8 = 0x05;
pub const UNIT_ATTENTION: u8 = 0x06;
pub const DATA_PROTECT: u8 = 0x07;

/// INQUIRY with an allocation length of `len` bytes (SPC-4 §6.4.1).
pub fn inquiry(len: u16) -> [u8; 6] {
    let [l0, l1] = len.to_be_bytes();
    [INQUIRY, 0, 0, l0, l1, 0]
}

pub fn test_unit_ready() -> [u8; 6] {
    [TEST_UNIT_READY, 0, 0, 0, 0, 0]
}

/// REQUEST SENSE, fixed format, `len` bytes (SPC-4 §6.29).
pub fn request_sense(len: u8) -> [u8; 6] {
    [REQUEST_SENSE, 0, 0, 0, len, 0]
}

/// READ CAPACITY(10) (SBC-3 §5.15).
pub fn read_capacity_10() -> [u8; 10] {
    [READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// READ CAPACITY(16) with an allocation length of `len` bytes (SBC-3
/// §5.16).
pub fn read_capacity_16(len: u32) -> [u8; 16] {
    let mut c = [0; 16];
    c[0] = SERVICE_ACTION_IN_16;
    c[1] = READ_CAPACITY_16;
    c[10..14].copy_from_slice(&len.to_be_bytes());
    c
}

/// READ(10) or WRITE(10): the LBA and the transfer length big-endian.
fn rw_10(opcode: u8, lba: u32, blocks: u16) -> [u8; 10] {
    let [a0, a1, a2, a3] = lba.to_be_bytes();
    let [n0, n1] = blocks.to_be_bytes();
    [opcode, 0, a0, a1, a2, a3, 0, n0, n1, 0]
}

/// READ(10) of `blocks` blocks from `lba` (SBC-3 §5.8).
pub fn read_10(lba: u32, blocks: u16) -> [u8; 10] {
    rw_10(READ_10, lba, blocks)
}

/// WRITE(10) of `blocks` blocks to `lba` (SBC-3 §5.32).
pub fn write_10(lba: u32, blocks: u16) -> [u8; 10] {
    rw_10(WRITE_10, lba, blocks)
}

/// SYNCHRONIZE CACHE(10) of the whole medium: LBA 0 and 0 blocks, which
/// means "to the end" (SBC-3 §5.22).
pub fn synchronize_cache_10() -> [u8; 10] {
    rw_10(SYNCHRONIZE_CACHE_10, 0, 0)
}

/// The name of a command for log lines.
pub fn command_name(opcode: u8) -> &'static str {
    match opcode {
        TEST_UNIT_READY => "TEST UNIT READY",
        REQUEST_SENSE => "REQUEST SENSE",
        INQUIRY => "INQUIRY",
        READ_CAPACITY_10 => "READ CAPACITY(10)",
        READ_10 => "READ(10)",
        WRITE_10 => "WRITE(10)",
        SYNCHRONIZE_CACHE_10 => "SYNCHRONIZE CACHE(10)",
        SERVICE_ACTION_IN_16 => "READ CAPACITY(16)",
        _ => "command",
    }
}

/// What INQUIRY says about the device (SPC-4 §6.4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inquiry {
    /// 0 is a disk (direct-access block device).
    pub peripheral_type: u8,
    /// The RMB bit: the medium can be removed (a USB stick says so).
    pub removable: bool,
    /// T10 vendor identification, product identification and product
    /// revision level, without padding; bytes outside printable ASCII are
    /// `?`. Empty where the device sent less.
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

/// An ASCII field of INQUIRY data: bytes `range` of `b` as far as they
/// came, without the blanks or NULs around them.
fn ascii_field(b: &[u8], range: core::ops::Range<usize>) -> String {
    let end = range.end.min(b.len());
    let field = b.get(range.start..end).unwrap_or(&[]);
    let blank = |c: &u8| *c == b' ' || *c == 0;
    let first = field.iter().position(|c| !blank(c)).unwrap_or(field.len());
    let last = field
        .iter()
        .rposition(|c| !blank(c))
        .map_or(first, |i| i + 1);
    field[first..last]
        .iter()
        .map(|&c| {
            if (0x20..0x7F).contains(&c) {
                c as char
            } else {
                '?'
            }
        })
        .collect()
}

impl Inquiry {
    /// Parses INQUIRY data; 5 bytes (the header) are enough, the names are
    /// taken from what came and what the additional length covers.
    pub fn parse(b: &[u8]) -> Result<Inquiry, UsbError> {
        if b.len() < 5 {
            return Err(UsbError::Protocol("INQUIRY data too short"));
        }
        // Byte 4 is the number of bytes after it.
        let b = &b[..b.len().min(5 + b[4] as usize)];
        Ok(Inquiry {
            peripheral_type: b[0] & 0x1F,
            removable: b[1] & 0x80 != 0,
            vendor: ascii_field(b, 8..16),
            product: ascii_field(b, 16..32),
            revision: ascii_field(b, 32..36),
        })
    }
}

/// The medium's size: its last LBA and the bytes in a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub last_lba: u64,
    pub block_size: u32,
}

fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

impl Capacity {
    /// READ CAPACITY(10) data: 0xFFFFFFFF as the last LBA means READ
    /// CAPACITY(16) must be asked.
    pub fn parse_10(b: &[u8]) -> Result<Capacity, UsbError> {
        if b.len() < CAPACITY_10_LEN {
            return Err(UsbError::Protocol("READ CAPACITY data too short"));
        }
        Ok(Capacity {
            last_lba: be32(b, 0) as u64,
            block_size: be32(b, 4),
        })
    }

    /// READ CAPACITY(16) data (the first 12 bytes are used).
    pub fn parse_16(b: &[u8]) -> Result<Capacity, UsbError> {
        if b.len() < 12 {
            return Err(UsbError::Protocol("READ CAPACITY data too short"));
        }
        Ok(Capacity {
            last_lba: (be32(b, 0) as u64) << 32 | be32(b, 4) as u64,
            block_size: be32(b, 8),
        })
    }
}

/// Why a command failed: the sense key and additional sense code and
/// qualifier (SPC-4 §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sense {
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
}

impl Sense {
    /// Parses fixed-format (response code 0x70/0x71) or descriptor-format
    /// (0x72/0x73) sense data; only the key, ASC and ASCQ are kept. In
    /// fixed format the ASC and ASCQ count only if the additional sense
    /// length covers them (0 otherwise).
    pub fn parse(b: &[u8]) -> Result<Sense, UsbError> {
        let short = Err(UsbError::Protocol("sense data too short"));
        // Bit 7 of byte 0 is VALID (fixed format), not the response code.
        match b.first().map(|c| c & 0x7F) {
            Some(0x70 | 0x71) => {
                if b.len() < 3 {
                    return short;
                }
                // Byte 7 counts the bytes after it; ASC and ASCQ are 12-13.
                let valid = b.len().min(8 + b.get(7).copied().unwrap_or(0) as usize);
                let byte = |i: usize| if i < valid { b[i] } else { 0 };
                Ok(Sense {
                    key: b[2] & 0x0F,
                    asc: byte(12),
                    ascq: byte(13),
                })
            }
            Some(0x72 | 0x73) => match b {
                [_, key, asc, ascq, ..] => Ok(Sense {
                    key: key & 0x0F,
                    asc: *asc,
                    ascq: *ascq,
                }),
                _ => short,
            },
            Some(_) => Err(UsbError::Protocol("unknown sense data format")),
            None => short,
        }
    }

    /// The sense key's name (SPC-4 table 49).
    fn key_name(&self) -> Option<&'static str> {
        Some(match self.key {
            0x0 => "NO SENSE",
            0x1 => "RECOVERED ERROR",
            0x2 => "NOT READY",
            0x3 => "MEDIUM ERROR",
            0x4 => "HARDWARE ERROR",
            0x5 => "ILLEGAL REQUEST",
            0x6 => "UNIT ATTENTION",
            0x7 => "DATA PROTECT",
            0x8 => "BLANK CHECK",
            0x9 => "VENDOR SPECIFIC",
            0xA => "COPY ABORTED",
            0xB => "ABORTED COMMAND",
            0xD => "VOLUME OVERFLOW",
            0xE => "MISCOMPARE",
            _ => return None,
        })
    }
}

impl fmt::Display for Sense {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.key_name() {
            Some(name) => f.write_str(name)?,
            None => write!(f, "sense key {:#04x}", self.key)?,
        }
        write!(f, " (asc {:#04x}, ascq {:#04x})", self.asc, self.ascq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The Kingston DataTraveler 3.0's INQUIRY data as Linux read it
    /// (`/sys/block/sda/device/inquiry`).
    const KINGSTON: [u8; 62] = [
        0x00, 0x80, 0x06, 0x02, 0x39, 0x00, 0x00, 0x00, //
        b'K', b'i', b'n', b'g', b's', b't', b'o', b'n', //
        b'D', b'a', b't', b'a', b'T', b'r', b'a', b'v', //
        b'e', b'l', b'e', b'r', b' ', b'3', b'.', b'0', //
        b'P', b'M', b'A', b'P', //
        0x50, 0x4d, 0x41, 0x50, 0x31, 0x32, 0x33, 0x34, 0x87, 0x5b, 0x82, 0xc9, 0x22, 0xb4, 0x96,
        0x9e, 0xa5, 0x84, 0xe9, 0x8f, 0xbc, 0xe1, 0x04, 0x60, 0x04, 0xc0,
    ];

    #[test]
    fn command_blocks_are_byte_exact() {
        assert_eq!(inquiry(36), [0x12, 0, 0, 0, 36, 0]);
        assert_eq!(inquiry(0x0102), [0x12, 0, 0, 0x01, 0x02, 0]);
        assert_eq!(test_unit_ready(), [0; 6]);
        assert_eq!(request_sense(18), [0x03, 0, 0, 0, 18, 0]);
        assert_eq!(read_capacity_10(), [0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            read_capacity_16(32),
            [0x9E, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0]
        );
        assert_eq!(
            read_capacity_16(0x0102_0304)[10..14],
            [0x01, 0x02, 0x03, 0x04]
        );
        // SBC-3 table 97: operation code, flags, LBA (big-endian), group,
        // transfer length (big-endian), control.
        assert_eq!(
            read_10(0x1234_5678, 128),
            [0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0]
        );
        assert_eq!(
            write_10(0xFFFF_FFFF, 0xFFFF),
            [0x2A, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0xFF, 0xFF, 0]
        );
        assert_eq!(write_10(1, 1), [0x2A, 0, 0, 0, 0, 1, 0, 0, 1, 0]);
        assert_eq!(synchronize_cache_10(), [0x35, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn commands_have_names_for_the_log() {
        assert_eq!(command_name(READ_10), "READ(10)");
        assert_eq!(command_name(WRITE_10), "WRITE(10)");
        assert_eq!(command_name(TEST_UNIT_READY), "TEST UNIT READY");
        assert_eq!(command_name(SERVICE_ACTION_IN_16), "READ CAPACITY(16)");
        assert_eq!(command_name(SYNCHRONIZE_CACHE_10), "SYNCHRONIZE CACHE(10)");
        assert_eq!(command_name(0xA0), "command");
    }

    #[test]
    fn the_kingston_inquiry_data_parses() {
        let i = Inquiry::parse(&KINGSTON).unwrap();
        assert_eq!(
            i,
            Inquiry {
                peripheral_type: 0,
                removable: true,
                vendor: "Kingston".into(),
                product: "DataTraveler 3.0".into(),
                revision: "PMAP".into(),
            }
        );
        // Only the 36 bytes asked for.
        assert_eq!(Inquiry::parse(&KINGSTON[..36]), Ok(i));
    }

    #[test]
    fn inquiry_names_are_trimmed_and_cleaned() {
        let mut b = [0u8; 36];
        b[0] = 0x05; // a CD-ROM
        b[4] = 31;
        b[8..16].copy_from_slice(b"QEMU    ");
        b[16..32].copy_from_slice(b"QEMU HARDDISK   ");
        b[32..36].copy_from_slice(b"2.5+");
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.peripheral_type, i.removable), (5, false));
        assert_eq!(
            (i.vendor.as_str(), i.product.as_str()),
            ("QEMU", "QEMU HARDDISK")
        );
        assert_eq!(i.revision, "2.5+");
        // Leading blanks, NUL padding and bytes outside printable ASCII.
        b[8..16].copy_from_slice(b"  Ven\x01\0\0");
        b[16..32].copy_from_slice(b"\xFFroduct\0\0\0\0\0\0\0\0\0");
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("Ven?", "?roduct"));
        // The peripheral qualifier is not part of the type, and only bit 7
        // of byte 1 is RMB (bit 6 is LU_CONG).
        b[0] = 0x20;
        b[1] = 0x40;
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.peripheral_type, i.removable), (0, false));
    }

    #[test]
    fn short_inquiry_data_gives_what_came() {
        let i = Inquiry::parse(&KINGSTON[..5]).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("", ""));
        assert!(i.removable);
        let i = Inquiry::parse(&KINGSTON[..20]).unwrap();
        assert_eq!(
            (i.vendor.as_str(), i.product.as_str()),
            ("Kingston", "Data")
        );
        // The additional length says less than came: the rest is not used.
        let mut b = KINGSTON;
        b[4] = 11;
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("Kingston", ""));
        for len in 0..5 {
            assert_eq!(
                Inquiry::parse(&KINGSTON[..len]),
                Err(UsbError::Protocol("INQUIRY data too short"))
            );
        }
    }

    #[test]
    fn capacities_parse_including_the_largest() {
        // The Kingston stick: 30,277,632 blocks of 512 bytes.
        let c = Capacity::parse_10(&[0x01, 0xCD, 0xFF, 0xFF, 0, 0, 2, 0]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (30_277_631, 512));
        let c = Capacity::parse_10(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (0xFFFF_FFFF, 4096));
        assert!(Capacity::parse_10(&[0; 7]).is_err());
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&0x1_2345_6789u64.to_be_bytes());
        b[8..12].copy_from_slice(&512u32.to_be_bytes());
        let c = Capacity::parse_16(&b).unwrap();
        assert_eq!((c.last_lba, c.block_size), (0x1_2345_6789, 512));
        assert_eq!(Capacity::parse_16(&b[..12]), Ok(c));
        let c = Capacity::parse_16(&[0xFF; 32]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (u64::MAX, u32::MAX));
        assert_eq!(
            Capacity::parse_16(&b[..11]),
            Err(UsbError::Protocol("READ CAPACITY data too short"))
        );
    }

    #[test]
    fn fixed_and_descriptor_sense_data_parse() {
        // Fixed format: NOT READY, becoming ready.
        let mut fixed = [0u8; 18];
        fixed[0] = 0x70;
        fixed[2] = 0x02;
        fixed[7] = 10;
        fixed[12] = 0x04;
        fixed[13] = 0x01;
        let s = Sense::parse(&fixed).unwrap();
        assert_eq!(
            s,
            Sense {
                key: 2,
                asc: 4,
                ascq: 1
            }
        );
        // Deferred errors and the VALID bit; the FILEMARK/EOM/ILI bits
        // are not part of the key.
        fixed[0] = 0xF1;
        fixed[2] = 0xE3;
        assert_eq!(Sense::parse(&fixed).unwrap().key, 3);
        // Descriptor format: UNIT ATTENTION, power on or reset.
        let desc = [0x72, 0x06, 0x29, 0x00, 0, 0, 0, 0];
        assert_eq!(
            Sense::parse(&desc),
            Ok(Sense {
                key: 6,
                asc: 0x29,
                ascq: 0
            })
        );
        assert_eq!(Sense::parse(&[0x73, 0x05, 0x20, 0x00]).unwrap().asc, 0x20);
    }

    #[test]
    fn short_and_garbage_sense_data_never_panics() {
        // Fixed format with only the key: no ASC and ASCQ.
        assert_eq!(
            Sense::parse(&[0x70, 0, 0x05]),
            Ok(Sense {
                key: 5,
                asc: 0,
                ascq: 0
            })
        );
        // The additional length does not reach the ASC: not believed.
        let mut b = [0u8; 18];
        b[0] = 0x70;
        b[2] = 0x03;
        b[7] = 4;
        b[12] = 0x11;
        assert_eq!(Sense::parse(&b).unwrap().asc, 0);
        assert!(Sense::parse(&[]).is_err());
        assert!(Sense::parse(&[0x70, 0]).is_err());
        assert!(Sense::parse(&[0x72, 0x06, 0x29]).is_err());
        assert_eq!(
            Sense::parse(&[0x00; 18]),
            Err(UsbError::Protocol("unknown sense data format"))
        );
        for len in 0..=18 {
            for fill in [0x00, 0x70, 0x72, 0xFF] {
                let _ = Sense::parse(&[fill; 18][..len]);
            }
        }
    }

    #[test]
    fn sense_reads_well_in_a_log_line() {
        let s = |key, asc, ascq| Sense { key, asc, ascq }.to_string();
        assert_eq!(s(2, 4, 1), "NOT READY (asc 0x04, ascq 0x01)");
        assert_eq!(s(3, 0x11, 0), "MEDIUM ERROR (asc 0x11, ascq 0x00)");
        assert_eq!(s(6, 0x29, 0), "UNIT ATTENTION (asc 0x29, ascq 0x00)");
        assert_eq!(s(7, 0x27, 0), "DATA PROTECT (asc 0x27, ascq 0x00)");
        assert_eq!(s(5, 0x20, 0), "ILLEGAL REQUEST (asc 0x20, ascq 0x00)");
        assert_eq!(s(0, 0, 0), "NO SENSE (asc 0x00, ascq 0x00)");
        assert_eq!(s(0xB, 0, 0), "ABORTED COMMAND (asc 0x00, ascq 0x00)");
        assert_eq!(s(0xE, 0x1D, 0), "MISCOMPARE (asc 0x1d, ascq 0x00)");
        assert_eq!(s(0xC, 0, 0), "sense key 0x0c (asc 0x00, ascq 0x00)");
    }
}
