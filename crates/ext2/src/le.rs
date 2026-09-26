//! Little-endian fields in on-disk structures. Callers check that the field
//! lies inside the buffer; these only convert.

pub fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

pub fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
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
}
