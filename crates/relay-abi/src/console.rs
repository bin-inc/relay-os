//! What the console calls take (spec §6.4, §6.5, §7.3).

/// `console_mode`'s modes: bytes as they are typed, or a line at a time
/// with the line discipline of spec §6.5.
pub const MODE_RAW: u32 = 0;
pub const MODE_LINE: u32 = 1;

/// `console_size`'s result: the columns in the low 32 bits, the rows in the
/// high 32.
pub const fn size_result(columns: u32, rows: u32) -> u64 {
    columns as u64 | (rows as u64) << 32
}

/// The columns and rows in `console_size`'s result.
pub const fn size_of_result(r: u64) -> (u32, u32) {
    (r as u32, (r >> 32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_sizes() {
        assert_eq!((MODE_RAW, MODE_LINE), (0, 1));
        assert_eq!(size_result(120, 33), 120 | 33 << 32);
        assert_eq!(size_of_result(size_result(120, 33)), (120, 33));
        assert_eq!(size_of_result(size_result(u32::MAX, 1)), (u32::MAX, 1));
    }
}
