//! Little-endian fields in on-disk structures. Callers check that the field
//! lies inside the buffer; these only convert.

pub fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

pub fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

pub fn set_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

pub fn set_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_little_endian() {
        let b = [0, 0x78, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0];
        assert_eq!(u32_at(&b, 1), 0x1234_5678);
        assert_eq!(u16_at(&b, 5), 0xABCD);
    }

    #[test]
    fn setters_write_little_endian() {
        let mut b = [0u8; 8];
        set_u32(&mut b, 1, 0x1234_5678);
        set_u16(&mut b, 5, 0xABCD);
        assert_eq!(b, [0, 0x78, 0x56, 0x34, 0x12, 0xCD, 0xAB, 0]);
    }
}
