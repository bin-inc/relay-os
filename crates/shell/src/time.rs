//! Calendar dates for `date`, `ls -l` and `stat`, in UTC (the CMOS clock
//! holds UTC, spec §1.3).

use alloc::format;
use alloc::string::String;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Half of 365.2425 days, GNU `ls`'s limit for showing the time of day.
const SIX_MONTHS: u64 = 31_556_952 / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    /// 1–12
    pub month: u32,
    /// 1–31
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl DateTime {
    /// Seconds since 1970 to a date (Howard Hinnant's `civil_from_days`).
    pub fn from_unix(secs: u64) -> DateTime {
        let days = (secs / 86_400) as i64;
        let rem = secs % 86_400;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        DateTime {
            year,
            month,
            day,
            hour: (rem / 3600) as u32,
            minute: (rem / 60 % 60) as u32,
            second: (rem % 60) as u32,
        }
    }

    fn month_name(&self) -> &'static str {
        MONTHS[(self.month - 1) as usize]
    }
}

/// `stat`: `2026-09-26 12:00:00.000000000 +0000`.
pub fn full(secs: u64) -> String {
    let t = DateTime::from_unix(secs);
    format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02}.000000000 +0000",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// `ls -l`: `Sep 26 12:00` for the last six months, `Sep 26  2025`
/// otherwise (also for times in the future).
pub fn short(secs: u64, now: u64) -> String {
    let t = DateTime::from_unix(secs);
    if secs <= now && now - secs < SIX_MONTHS {
        format!(
            "{} {:>2} {:02}:{:02}",
            t.month_name(),
            t.day,
            t.hour,
            t.minute
        )
    } else {
        format!("{} {:>2}  {}", t.month_name(), t.day, t.year)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        let t = DateTime::from_unix(0);
        assert_eq!((t.year, t.month, t.day), (1970, 1, 1));
        let t = DateTime::from_unix(1_790_424_000);
        assert_eq!((t.year, t.month, t.day, t.hour), (2026, 9, 26, 12));
        // A leap day and the last second of a year.
        let t = DateTime::from_unix(951_782_400);
        assert_eq!((t.year, t.month, t.day), (2000, 2, 29));
        let t = DateTime::from_unix(1_767_225_599);
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute, t.second),
            (2025, 12, 31, 23, 59, 59)
        );
        // 32-bit ext2 times end in 2106.
        let t = DateTime::from_unix(u32::MAX as u64);
        assert_eq!((t.year, t.month, t.day), (2106, 2, 7));
    }

    #[test]
    fn formats() {
        assert_eq!(full(1_736_065_800), "2025-01-05 08:30:00.000000000 +0000");
    }

    #[test]
    fn ls_shows_the_year_for_old_and_future_times() {
        let now = 1_790_424_000;
        assert_eq!(short(now - 3600, now), "Sep 26 11:00");
        assert_eq!(short(1_736_065_800, now), "Jan  5  2025");
        assert_eq!(short(now + 86_400, now), "Sep 27  2026");
    }
}
