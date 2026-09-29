//! How a result register carries a value or an error (spec §7.1).

/// Results from `MAX_ERRNO` below 2^64 up are negated error numbers.
pub const MAX_ERRNO: u64 = 4095;

/// A call's result as the kernel returns it: a value from 0 to 2^63 − 1, or
/// a negated error number from −4095 to −1.
pub fn encode(result: Result<u64, u16>) -> u64 {
    match result {
        Ok(v) => {
            debug_assert!(v < 1 << 63, "a result value must be below 2^63");
            v
        }
        Err(e) => {
            debug_assert!(e != 0 && u64::from(e) <= MAX_ERRNO);
            u64::from(e).wrapping_neg()
        }
    }
}

/// The value or error number in a result register.
pub fn decode(raw: u64) -> Result<u64, u16> {
    if raw > u64::MAX - MAX_ERRNO {
        Err(raw.wrapping_neg() as u16)
    } else {
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errno;

    #[test]
    fn results_carry_values_or_negated_error_numbers() {
        assert_eq!(encode(Ok(0)), 0);
        assert_eq!(encode(Ok(4096)), 4096);
        assert_eq!(encode(Err(errno::EPERM)), u64::MAX, "-1");
        assert_eq!(encode(Err(errno::ENOENT)), u64::MAX - 1, "-2");
        assert_eq!(encode(Err(4095)), u64::MAX - 4094, "-4095");
        for r in [Ok(0), Ok(1), Ok((1 << 63) - 1), Err(1), Err(38), Err(4095)] {
            assert_eq!(decode(encode(r)), r, "{r:?}");
        }
    }

    #[test]
    fn only_the_top_4095_values_are_errors() {
        assert_eq!(decode(u64::MAX - 4095), Ok(u64::MAX - 4095));
        assert_eq!(decode(u64::MAX - 4094), Err(4095));
        assert_eq!(decode(1 << 63), Ok(1 << 63));
    }
}
