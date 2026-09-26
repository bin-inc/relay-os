//! The CMOS real-time clock (spec §4.4 step 6), read once at boot for
//! wall-clock time. It holds UTC (spec §1.3). Later reads add the uptime.

use crate::timer;
use core::fmt;
use spin::Once;
use x86_64::instructions::port::Port;

const SECONDS: u8 = 0x00;
const MINUTES: u8 = 0x02;
const HOURS: u8 = 0x04;
const DAY: u8 = 0x07;
const MONTH: u8 = 0x08;
const YEAR: u8 = 0x09;
const STATUS_A: u8 = 0x0A;
const STATUS_B: u8 = 0x0B;
const UPDATE_IN_PROGRESS: u8 = 0x80;
const BINARY: u8 = 0x04;
const HOURS_24: u8 = 0x02;
const PM: u8 = 0x80;

/// CMOS register access, so reading is tested on the host.
pub trait Cmos {
    fn read(&mut self, reg: u8) -> u8;
}

/// Ports 0x70 (index) and 0x71 (data).
pub struct RealCmos;

impl Cmos for RealCmos {
    fn read(&mut self, reg: u8) -> u8 {
        // SAFETY: standard CMOS ports; bit 7 of the index (NMI disable)
        // stays clear.
        unsafe {
            Port::<u8>::new(0x70).write(reg & 0x7F);
            Port::<u8>::new(0x71).read()
        }
    }
}

/// Clock registers as read, before BCD and 12-hour decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Raw {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8,
    /// The FADT's century register, if it names one.
    pub century: Option<u8>,
    pub status_b: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

fn is_leap(y: u16) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn days_in_month(y: u16, m: u8) -> u8 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl DateTime {
    /// Seconds since 1970-01-01 00:00:00 UTC.
    pub fn unix(&self) -> u64 {
        // Days from civil (H. Hinnant): years start in March.
        let (y, m) = (self.year as i64, self.month as i64);
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days as u64 * 86_400
            + self.hour as u64 * 3600
            + self.minute as u64 * 60
            + self.second as u64
    }

    /// A real date and time, not before 1970 (the Unix epoch; a cleared
    /// CMOS can report a century register of 0 or 19).
    fn is_valid(&self) -> bool {
        self.year >= 1970
            && (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Decodes the registers (BCD or binary, 12- or 24-hour, as status
/// register B says). `None` if a value is out of range.
pub fn decode(raw: Raw) -> Option<DateTime> {
    let binary = raw.status_b & BINARY != 0;
    let num = |v: u8| -> Option<u8> {
        if binary {
            Some(v)
        } else if v >> 4 <= 9 && v & 0xF <= 9 {
            Some((v >> 4) * 10 + (v & 0xF))
        } else {
            None
        }
    };
    let mut hour = num(raw.hour & !PM)?;
    if raw.status_b & HOURS_24 == 0 {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = hour % 12 + if raw.hour & PM != 0 { 12 } else { 0 };
    }
    let century = match raw.century {
        Some(c) => num(c)? as u16,
        None => 20,
    };
    let t = DateTime {
        year: century * 100 + num(raw.year)? as u16,
        month: num(raw.month)?,
        day: num(raw.day)?,
        hour,
        minute: num(raw.minute)?,
        second: num(raw.second)?,
    };
    t.is_valid().then_some(t)
}

/// Waits (briefly, and never forever) until no update is in progress.
fn wait_for_update(cmos: &mut impl Cmos) {
    for _ in 0..1_000_000 {
        if cmos.read(STATUS_A) & UPDATE_IN_PROGRESS == 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

fn read_once(cmos: &mut impl Cmos, century_reg: u8) -> Raw {
    wait_for_update(cmos);
    Raw {
        second: cmos.read(SECONDS),
        minute: cmos.read(MINUTES),
        hour: cmos.read(HOURS),
        day: cmos.read(DAY),
        month: cmos.read(MONTH),
        year: cmos.read(YEAR),
        century: (century_reg != 0).then(|| cmos.read(century_reg)),
        status_b: cmos.read(STATUS_B),
    }
}

/// Reads until two reads in a row agree, so a read that straddles the
/// clock's once-a-second update is never used.
pub fn read(cmos: &mut impl Cmos, century_reg: u8) -> Raw {
    let mut last = read_once(cmos, century_reg);
    for _ in 0..10 {
        let next = read_once(cmos, century_reg);
        if next == last {
            break;
        }
        last = next;
    }
    last
}

/// The RTC's seconds register, decoded (for the timer self-check).
pub fn second() -> u8 {
    let mut cmos = RealCmos;
    wait_for_update(&mut cmos);
    let raw = cmos.read(SECONDS);
    if cmos.read(STATUS_B) & BINARY != 0 {
        raw
    } else {
        (raw >> 4) * 10 + (raw & 0xF)
    }
}

/// Wall-clock time at boot and the tick count when it was read.
static BOOT: Once<(u64, u64)> = Once::new();

/// Reads the clock. `century_reg` is the FADT's century register (0 if
/// none: the years are then 2000-2099).
pub fn init(century_reg: u8) -> Result<DateTime, Raw> {
    let raw = read(&mut RealCmos, century_reg);
    let t = decode(raw).ok_or(raw)?;
    BOOT.call_once(|| (t.unix(), timer::ticks()));
    Ok(t)
}

/// Seconds since 1970 (UTC), or `None` if the clock could not be read.
pub fn now_unix() -> Option<u64> {
    let &(unix, ticks) = BOOT.get()?;
    Some(unix + (timer::ticks() - ticks) / timer::TICK_HZ)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(bytes: [u8; 6], status_b: u8) -> Raw {
        let [second, minute, hour, day, month, year] = bytes;
        Raw {
            second,
            minute,
            hour,
            day,
            month,
            year,
            century: None,
            status_b,
        }
    }

    fn dt(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
        DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn bcd_24_hour() {
        // The PC default: BCD, 24-hour (status B = 0x02).
        let r = raw([0x59, 0x30, 0x18, 0x26, 0x09, 0x26], 0x02);
        assert_eq!(decode(r), Some(dt(2026, 9, 26, 18, 30, 59)));
    }

    #[test]
    fn bcd_12_hour_with_pm_bit() {
        let pm = |h| decode(raw([0, 0, h, 1, 1, 0x26], 0x00)).map(|t| t.hour);
        assert_eq!(pm(0x12), Some(0), "12 AM is midnight");
        assert_eq!(pm(0x01), Some(1));
        assert_eq!(pm(0x80 | 0x12), Some(12), "12 PM is noon");
        assert_eq!(pm(0x80 | 0x06), Some(18));
        assert_eq!(pm(0x00), None, "hour 0 does not exist in 12-hour mode");
    }

    #[test]
    fn binary_mode_and_century_register() {
        let mut r = raw([5, 4, 23, 31, 12, 99], 0x06);
        assert_eq!(decode(r), Some(dt(2099, 12, 31, 23, 4, 5)));
        r.century = Some(19);
        assert_eq!(decode(r).unwrap().year, 1999);
        r.year = 69;
        assert_eq!(decode(r), None, "1969 is before the epoch");
        let mut r = raw([0, 0, 0, 1, 1, 0x26], 0x02);
        r.century = Some(0x20);
        assert_eq!(decode(r).unwrap().year, 2026);
    }

    #[test]
    fn nonsense_is_rejected() {
        assert_eq!(
            decode(raw([0, 0, 0, 1, 0x13, 0x26], 0x02)),
            None,
            "month 13"
        );
        assert_eq!(decode(raw([0x5A, 0, 0, 1, 1, 0x26], 0x02)), None, "not BCD");
        assert_eq!(decode(raw([0, 0, 0x24, 1, 1, 0x26], 0x02)), None, "hour 24");
        assert_eq!(
            decode(raw([0, 0, 0, 0x29, 2, 0x25], 0x02)),
            None,
            "no Feb 29 in 2025"
        );
        assert!(decode(raw([0, 0, 0, 0x29, 2, 0x24], 0x02)).is_some());
        assert_eq!(decode(raw([0xFF; 6], 0xFF)), None, "floating bus");
        let mut cleared = raw([0, 0, 0, 1, 1, 0x26], 0x02);
        cleared.century = Some(0x00);
        assert_eq!(decode(cleared), None, "year 26 after a CMOS clear");
    }

    #[test]
    fn unix_time() {
        assert_eq!(dt(1970, 1, 1, 0, 0, 0).unix(), 0);
        assert_eq!(dt(2000, 3, 1, 0, 0, 0).unix(), 951_868_800);
        assert_eq!(dt(2024, 2, 29, 23, 59, 59).unix(), 1_709_251_199);
        assert_eq!(dt(2026, 9, 26, 18, 0, 0).unix(), 1_790_445_600);
    }

    #[test]
    fn display() {
        assert_eq!(
            dt(2026, 9, 6, 8, 5, 3).to_string(),
            "2026-09-06 08:05:03 UTC"
        );
    }

    /// A clock that is updating for the first reads and ticks over from
    /// 23:59:59 to 00:00:00 in the middle of the first full read, which
    /// therefore sees 00:59:59 on the new day.
    struct Rolling {
        reads: usize,
        uip_reads: usize,
    }

    impl Cmos for Rolling {
        fn read(&mut self, reg: u8) -> u8 {
            self.reads += 1;
            let rolled = self.reads > 6;
            match reg {
                STATUS_A if self.uip_reads > 0 => {
                    self.uip_reads -= 1;
                    UPDATE_IN_PROGRESS
                }
                STATUS_A => 0,
                STATUS_B => 0x02,
                SECONDS => {
                    if rolled {
                        0x00
                    } else {
                        0x59
                    }
                }
                MINUTES => {
                    if rolled {
                        0x00
                    } else {
                        0x59
                    }
                }
                HOURS => {
                    if rolled {
                        0x00
                    } else {
                        0x23
                    }
                }
                DAY => {
                    if rolled {
                        0x27
                    } else {
                        0x26
                    }
                }
                MONTH => 0x09,
                YEAR => 0x26,
                _ => 0,
            }
        }
    }

    #[test]
    fn a_read_across_the_update_is_retried() {
        let mut c = Rolling {
            reads: 0,
            uip_reads: 3,
        };
        let r = read(&mut c, 0);
        assert_eq!(decode(r), Some(dt(2026, 9, 27, 0, 0, 0)));
        assert_eq!(c.uip_reads, 0, "waited for the update to finish");
    }
}
