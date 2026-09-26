//! Allocation bitmaps (spec §8.2): bit `i` is bit `i % 8` of byte `i / 8`,
//! set while block or inode `i` of the group is in use.

pub fn test(bits: &[u8], i: usize) -> bool {
    bits[i / 8] & (1 << (i % 8)) != 0
}

pub fn set(bits: &mut [u8], i: usize) {
    bits[i / 8] |= 1 << (i % 8);
}

pub fn clear(bits: &mut [u8], i: usize) {
    bits[i / 8] &= !(1 << (i % 8));
}

/// The first clear bit in `from..to`.
pub fn first_zero(bits: &[u8], from: usize, to: usize) -> Option<usize> {
    let mut i = from;
    while i < to {
        // Skip whole bytes in use.
        if i.is_multiple_of(8) && i + 8 <= to && bits[i / 8] == 0xFF {
            i += 8;
            continue;
        }
        if !test(bits, i) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The first clear bit at or after `goal` among the first `len`, wrapping
/// around to the start.
pub fn find_zero(bits: &[u8], len: usize, goal: usize) -> Option<usize> {
    let goal = goal.min(len);
    first_zero(bits, goal, len).or_else(|| first_zero(bits, 0, goal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_are_set_tested_and_cleared_lsb_first() {
        let mut b = [0u8; 2];
        set(&mut b, 0);
        set(&mut b, 9);
        assert_eq!(b, [0x01, 0x02]);
        assert!(test(&b, 9) && !test(&b, 8));
        clear(&mut b, 0);
        assert_eq!(b, [0x00, 0x02]);
    }

    #[test]
    fn the_first_clear_bit_is_found_in_a_range() {
        let b = [0xFF, 0xFF, 0b1110_1111, 0xFF];
        assert_eq!(first_zero(&b, 0, 32), Some(20));
        assert_eq!(first_zero(&b, 21, 32), None);
        assert_eq!(first_zero(&b, 0, 20), None);
        assert_eq!(first_zero(&[0xFF, 0x7F], 3, 16), Some(15));
    }

    #[test]
    fn the_search_wraps_around_to_the_start() {
        let b = [0b1111_1011, 0xFF];
        assert_eq!(find_zero(&b, 16, 2), Some(2));
        assert_eq!(find_zero(&b, 16, 3), Some(2));
        assert_eq!(find_zero(&b, 16, 100), Some(2));
        assert_eq!(
            find_zero(&[0xFF, 0x01], 9, 0),
            None,
            "padding bits beyond len"
        );
        assert_eq!(find_zero(&[0xFF, 0x01], 10, 5), Some(9));
    }
}
