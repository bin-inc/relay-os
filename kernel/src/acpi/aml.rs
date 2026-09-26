//! Finding `\_S5` in the DSDT without an AML interpreter (spec §5.4).
//!
//! Firmware declares the soft-off sleep type as a named package:
//! `Name (_S5, Package (N) { SLP_TYPa, SLP_TYPb, ... })`, encoded as
//! `NameOp (0x08) ['\'] "_S5_" PackageOp (0x12) PkgLength NumElements
//! elements...`. We scan the raw bytes for that shape and decode the first
//! two integer elements.

use super::tables::HEADER_LEN;

/// The SLP_TYP values for PM1a_CNT and PM1b_CNT that select S5 (soft off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepType {
    pub a: u8,
    pub b: u8,
}

const NAME_OP: u8 = 0x08;
const ROOT_CHAR: u8 = b'\\';
const PACKAGE_OP: u8 = 0x12;

/// Decodes an AML integer at `*i` (ZeroOp, OneOp, OnesOp or a Byte, Word,
/// DWord or QWord constant) and moves `*i` past it.
fn integer(aml: &[u8], i: &mut usize) -> Option<u64> {
    let op = *aml.get(*i)?;
    let (value, len) = match op {
        0x00 => (0, 1),
        0x01 => (1, 1),
        0xFF => (u64::MAX, 1),
        0x0A => (*aml.get(*i + 1)? as u64, 2),
        0x0B => (
            u16::from_le_bytes(aml.get(*i + 1..*i + 3)?.try_into().ok()?) as u64,
            3,
        ),
        0x0C => (
            u32::from_le_bytes(aml.get(*i + 1..*i + 5)?.try_into().ok()?) as u64,
            5,
        ),
        0x0E => (
            u64::from_le_bytes(aml.get(*i + 1..*i + 9)?.try_into().ok()?),
            9,
        ),
        _ => return None,
    };
    *i += len;
    Some(value)
}

/// Decodes the package that follows a `_S5_` name at `name`.
fn package_at(aml: &[u8], name: usize) -> Option<SleepType> {
    let mut i = name + 4;
    if *aml.get(i)? != PACKAGE_OP {
        return None;
    }
    // PkgLength: bits 7-6 of the lead byte count the bytes that follow.
    let lead = *aml.get(i + 1)?;
    i += 2 + (lead >> 6) as usize;
    let count = *aml.get(i)?;
    i += 1;
    if count == 0 {
        return None;
    }
    let a = integer(aml, &mut i)?;
    let b = if count >= 2 { integer(aml, &mut i)? } else { 0 };
    // SLP_TYP is a 3-bit field.
    Some(SleepType {
        a: (a & 7) as u8,
        b: (b & 7) as u8,
    })
}

/// SLP_TYPa/b for S5 from a DSDT (header included), or `None` if the table
/// declares no `_S5` package we can decode.
pub fn find_s5(dsdt: &[u8]) -> Option<SleepType> {
    let aml = dsdt.get(HEADER_LEN..)?;
    let names = aml.windows(4).enumerate().filter(|(_, w)| w == b"_S5_");
    for (i, _) in names {
        let declared = match i {
            0 => false,
            1 => aml[0] == NAME_OP,
            _ => aml[i - 1] == NAME_OP || (aml[i - 1] == ROOT_CHAR && aml[i - 2] == NAME_OP),
        };
        if declared && let Some(s) = package_at(aml, i) {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dsdt(aml: &[u8]) -> Vec<u8> {
        let mut t = vec![0u8; HEADER_LEN];
        t[..4].copy_from_slice(b"DSDT");
        t.extend_from_slice(aml);
        t
    }

    #[test]
    fn nuc_s5_is_7_0() {
        // Name (_S5_, Package (0x04) { 0x07, Zero, Zero, Zero })
        let t = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/acpi/nuc/DSDT.bin"
        ));
        assert_eq!(find_s5(t), Some(SleepType { a: 7, b: 0 }));
    }

    #[test]
    fn qemu_s5_is_0_0() {
        // Name (_S5_, Package (0x04) { Zero, Zero, Zero, Zero })
        let t = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/acpi/qemu/DSDT.bin"
        ));
        assert_eq!(find_s5(t), Some(SleepType { a: 0, b: 0 }));
    }

    #[test]
    fn root_prefix_and_wider_constants() {
        // Name (\_S5_, Package (0x02) { 0x0005 (word), One })
        let t = dsdt(&[
            0x10, 0x08, b'\\', b'_', b'S', b'5', b'_', 0x12, 0x06, 0x02, 0x0B, 0x05, 0x00, 0x01,
        ]);
        assert_eq!(find_s5(&t), Some(SleepType { a: 5, b: 1 }));
    }

    #[test]
    fn multi_byte_package_length() {
        // PkgLength with one extra byte (lead 0x40 | low nibble).
        let t = dsdt(&[
            0x08, b'_', b'S', b'5', b'_', 0x12, 0x41, 0x00, 0x02, 0x0A, 0x07, 0x0A, 0x03,
        ]);
        assert_eq!(find_s5(&t), Some(SleepType { a: 7, b: 3 }));
    }

    #[test]
    fn a_single_element_package_gives_b_zero() {
        let t = dsdt(&[0x08, b'_', b'S', b'5', b'_', 0x12, 0x04, 0x01, 0x0A, 0x05]);
        assert_eq!(find_s5(&t), Some(SleepType { a: 5, b: 0 }));
    }

    #[test]
    fn a_reference_that_is_not_a_declaration_is_skipped() {
        // A method body mentioning _S5_ (not after NameOp), then the real
        // declaration.
        let t = dsdt(&[
            0x70, b'_', b'S', b'5', b'_', 0x60, // Store (_S5_, Local0)
            0x08, b'_', b'S', b'5', b'_', 0x12, 0x06, 0x04, 0x0A, 0x07, 0x00, 0x00, 0x00,
        ]);
        assert_eq!(find_s5(&t), Some(SleepType { a: 7, b: 0 }));
    }

    #[test]
    fn missing_or_truncated_is_none() {
        assert_eq!(
            find_s5(&dsdt(&[
                0x08, b'_', b'S', b'4', b'_', 0x12, 0x06, 0x04, 0x0A, 0x05
            ])),
            None
        );
        assert_eq!(
            find_s5(&dsdt(&[
                0x08, b'_', b'S', b'5', b'_', 0x12, 0x06, 0x04, 0x0A
            ])),
            None
        );
        assert_eq!(
            find_s5(&dsdt(&[0x08, b'_', b'S', b'5', b'_', 0x11])),
            None,
            "not a package"
        );
        assert_eq!(find_s5(&[0u8; 10]), None, "shorter than a header");
    }
}
