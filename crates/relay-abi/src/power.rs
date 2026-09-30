//! What `power` takes (spec §7.3).

/// `power`'s kinds.
pub const POWER_REBOOT: u32 = 1;
pub const POWER_POWEROFF: u32 = 2;
/// `power`'s flag: go ahead even if the filesystems cannot be shut down
/// cleanly (milestone 1's `reboot -f`).
pub const POWER_FORCE: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_fixed() {
        assert_eq!((POWER_REBOOT, POWER_POWEROFF, POWER_FORCE), (1, 2, 1));
    }
}
