//! CRC-32 (IEEE 802.3), as GPT (spec §6.5 of milestone 1) and the system
//! archive (spec §4.2 of the user-space gate) use it.
#![cfg_attr(not(test), no_std)]

/// The reflected IEEE 802.3 polynomial.
const POLY: u32 = 0xEDB8_8320;

/// The CRC of each byte value, built at compile time so it costs no stack.
static TABLE: [u32; 256] = table();

const fn table() -> [u32; 256] {
    let mut t = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
            bit += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

/// CRC-32 (IEEE 802.3, reflected, init and final xor 0xFFFFFFFF), as GPT and zlib use it.
pub fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &b| {
        TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn every_byte_value_counts() {
        // zlib's crc32 of the bytes 0..=255 in order, which reaches every
        // entry of the table.
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(crc32(&all), 0x2905_8C73);
    }
}
