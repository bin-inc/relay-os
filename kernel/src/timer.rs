//! Time since boot (spec §4.4 step 5). The TSC frequency comes from CPUID
//! leaf 0x15 (0x16 fills in a missing crystal frequency, as in Linux), or
//! is measured against the HPET where those leaves are missing (QEMU). The
//! LAPIC timer is calibrated against the TSC and then interrupts at 1 kHz,
//! driving the uptime clock and timeouts. These are the first interrupts
//! the kernel takes.

use crate::arch::lapic::{self, Lapic, Mode};
use crate::arch::{irq, pic};
use crate::mm::{self, paging::Cache, paging::MapError};
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::time::Duration;

pub const TICK_HZ: u64 = 1000;
/// How long the HPET and LAPIC measurements take.
const CALIBRATION_MS: u64 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TscSource {
    Cpuid15,
    Cpuid16,
    Hpet,
}

impl fmt::Display for TscSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            TscSource::Cpuid15 => "CPUID 0x15",
            TscSource::Cpuid16 => "CPUID 0x16",
            TscSource::Hpet => "HPET",
        })
    }
}

/// The TSC frequency from CPUID. `leaf15` is (EAX, EBX, ECX) of leaf 0x15:
/// the TSC/crystal ratio denominator and numerator and the crystal
/// frequency in Hz. When the crystal frequency is 0, the TSC runs at the
/// base frequency of leaf 0x16 (EAX, in MHz).
pub fn tsc_hz_from_cpuid(
    max_leaf: u32,
    leaf15: [u32; 3],
    leaf16_eax: u32,
) -> Option<(u64, TscSource)> {
    let [den, num, crystal] = leaf15;
    if max_leaf < 0x15 || den == 0 || num == 0 {
        return None;
    }
    if crystal != 0 {
        return Some((crystal as u64 * num as u64 / den as u64, TscSource::Cpuid15));
    }
    (max_leaf >= 0x16 && leaf16_eax != 0)
        .then(|| (leaf16_eax as u64 * 1_000_000, TscSource::Cpuid16))
}

fn cpuid_tsc() -> Option<(u64, TscSource)> {
    use core::arch::x86_64::__cpuid;
    let max = __cpuid(0).eax;
    let l15 = match max >= 0x15 {
        true => {
            let r = __cpuid(0x15);
            [r.eax, r.ebx, r.ecx]
        }
        false => [0; 3],
    };
    let l16 = if max >= 0x16 { __cpuid(0x16).eax } else { 0 };
    tsc_hz_from_cpuid(max, l15, l16)
}

fn rdtsc() -> u64 {
    // SAFETY: RDTSC has no side effects.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// HPET counter periods above 100 ns are invalid (HPET spec 1.0a, 2.3.9.1).
pub fn hpet_period_ok(period_fs: u64) -> bool {
    period_fs != 0 && period_fs <= 100_000_000
}

/// TSC cycles per second, given how far the TSC and the HPET counter moved
/// in the same interval.
pub fn tsc_hz_from_hpet(tsc_delta: u64, hpet_delta: u64, period_fs: u64) -> u64 {
    (tsc_delta as u128 * 1_000_000_000_000_000 / (hpet_delta as u128 * period_fs as u128)) as u64
}

/// Events per second of a counter that moved `count` while the TSC moved
/// `tsc_delta` cycles.
pub fn rate(count: u64, tsc_delta: u64, tsc_hz: u64) -> u64 {
    (count as u128 * tsc_hz as u128 / tsc_delta as u128) as u64
}

#[derive(Clone, Copy, Debug)]
pub enum TimerError {
    NoSource,
    BadHpetPeriod(u64),
    HpetStopped,
    Map(MapError),
}

impl From<MapError> for TimerError {
    fn from(e: MapError) -> Self {
        TimerError::Map(e)
    }
}

impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TimerError::NoSource => write!(f, "no TSC frequency (no CPUID leaf 0x15 and no HPET)"),
            TimerError::BadHpetPeriod(p) => write!(f, "HPET period {p} fs is invalid"),
            TimerError::HpetStopped => write!(f, "HPET counter does not run"),
            TimerError::Map(e) => write!(f, "{e}"),
        }
    }
}

const HPET_CAPABILITIES: usize = 0x00;
const HPET_CONFIG: usize = 0x10;
const HPET_COUNTER: usize = 0xF0;
const HPET_COUNTER_64BIT: u64 = 1 << 13;

fn measure_with_hpet(phys: u64) -> Result<u64, TimerError> {
    let base = mm::map_mmio(phys, 0x400, Cache::Uncached)? as usize;
    // SAFETY: the HPET register block, mapped uncached.
    let reg = |off: usize| unsafe { core::ptr::read_volatile((base + off) as *const u64) };
    let caps = reg(HPET_CAPABILITIES);
    let period_fs = caps >> 32;
    if !hpet_period_ok(period_fs) {
        return Err(TimerError::BadHpetPeriod(period_fs));
    }
    let mask = if caps & HPET_COUNTER_64BIT != 0 {
        u64::MAX
    } else {
        u32::MAX as u64
    };
    // Start the main counter (ENABLE_CNF).
    unsafe { core::ptr::write_volatile((base + HPET_CONFIG) as *mut u64, reg(HPET_CONFIG) | 1) };
    let target = CALIBRATION_MS * 1_000_000_000_000 / period_fs;
    let (h0, t0) = (reg(HPET_COUNTER), rdtsc());
    let mut spins = 0u64;
    loop {
        let (h, t) = (reg(HPET_COUNTER), rdtsc());
        let dh = h.wrapping_sub(h0) & mask;
        if dh >= target {
            return Ok(tsc_hz_from_hpet(t - t0, dh, period_fs));
        }
        spins += 1;
        if dh == 0 && spins > 10_000_000 {
            return Err(TimerError::HpetStopped);
        }
        core::hint::spin_loop();
    }
}

/// LAPIC timer ticks per second (after its divide-by-16), measured against
/// the TSC.
fn calibrate_lapic(l: &mut Lapic, tsc_hz: u64) -> u64 {
    lapic::start_calibration(l);
    let start = rdtsc();
    let end = start + tsc_hz * CALIBRATION_MS / 1000;
    while rdtsc() < end {
        core::hint::spin_loop();
    }
    let count = lapic::elapsed(l);
    rate(count as u64, rdtsc() - start, tsc_hz)
}

static TSC_HZ: AtomicU64 = AtomicU64::new(0);
/// Whether the 1 kHz tick runs.
static TICKING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug)]
pub struct TimerInfo {
    pub tsc_hz: u64,
    pub source: TscSource,
    pub lapic_mode: Mode,
}

impl fmt::Display for TimerInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mode = match self.lapic_mode {
            Mode::XApic { .. } => "xAPIC",
            Mode::X2Apic => "x2APIC",
        };
        write!(
            f,
            "TSC {}.{:03} MHz ({}), {TICK_HZ} Hz tick ({mode})",
            self.tsc_hz / 1_000_000,
            self.tsc_hz / 1000 % 1000,
            self.source
        )
    }
}

/// Finds the TSC frequency, masks the legacy PIC, starts the LAPIC timer at
/// `TICK_HZ` and enables interrupts. `hpet` is the HPET base from ACPI;
/// `force_hpet` (cmdline `tsc=hpet`) measures against it even when CPUID
/// has the frequency.
pub fn init(hpet: Option<u64>, force_hpet: bool) -> Result<TimerInfo, TimerError> {
    let (tsc_hz, source) = match cpuid_tsc() {
        Some(found) if !force_hpet => found,
        _ => {
            let base = hpet.ok_or(TimerError::NoSource)?;
            (measure_with_hpet(base)?, TscSource::Hpet)
        }
    };
    TSC_HZ.store(tsc_hz, Ordering::Relaxed);

    pic::remap_and_mask(&mut pic::RealPorts, irq::PIC_BASE);
    let mode = Lapic::detect();
    let mmio = match mode {
        Mode::XApic { phys } => mm::map_mmio(phys, 0x1000, Cache::Uncached)? as usize,
        Mode::X2Apic => 0,
    };
    let mut l = Lapic::new(mode, mmio);
    lapic::enable(&mut l, irq::SPURIOUS_VECTOR);
    let timer_hz = calibrate_lapic(&mut l, tsc_hz);
    lapic::LAPIC.call_once(|| l);
    lapic::start_periodic(
        &mut l,
        irq::TIMER_VECTOR,
        lapic::periodic_count(timer_hz, TICK_HZ),
    );
    x86_64::instructions::interrupts::enable();
    TICKING.store(true, Ordering::Relaxed);
    Ok(TimerInfo {
        tsc_hz,
        source,
        lapic_mode: mode,
    })
}

/// Whether the 1 kHz tick runs (`init` succeeded).
pub fn is_ticking() -> bool {
    TICKING.load(Ordering::Relaxed)
}

/// The tick a sleep of `ms` milliseconds that starts at tick `now` ends on:
/// the ticks `ms` takes, rounded up, and one more, since the next tick may
/// come right after `now`. So a sleep is never shorter than asked.
pub fn sleep_until(now: u64, ms: u64) -> u64 {
    let ticks = ms
        .checked_mul(TICK_HZ)
        .map_or(u64::MAX, |t| t.div_ceil(1000));
    now.saturating_add(ticks).saturating_add(1)
}

/// Timer ticks since the timer started (1 per millisecond).
pub fn ticks() -> u64 {
    irq::TICKS.load(Ordering::Relaxed)
}

/// Time since the timer started, in whole milliseconds.
pub fn uptime() -> Duration {
    Duration::from_millis(ticks() * 1000 / TICK_HZ)
}

/// TSC cycles per second (0 before `init`).
pub fn tsc_hz() -> u64 {
    TSC_HZ.load(Ordering::Relaxed)
}

/// The time `cycles` TSC cycles take at `tsc_hz`.
pub fn cycles_to_duration(cycles: u64, tsc_hz: u64) -> Duration {
    Duration::from_nanos((cycles as u128 * 1_000_000_000 / tsc_hz as u128) as u64)
}

/// Time since the CPU started, from the TSC, with sub-microsecond
/// resolution; `None` until `init` has found the TSC's frequency. Unlike
/// `uptime` it does not depend on the timer interrupt, so timeouts still
/// expire if the LAPIC timer could not be started.
pub fn tsc_time() -> Option<Duration> {
    let hz = tsc_hz();
    (hz != 0).then(|| cycles_to_duration(rdtsc(), hz))
}

/// TSC cycles in `d`, rounded up.
pub fn sleep_cycles(d: Duration, tsc_hz: u64) -> u64 {
    (d.as_nanos() * tsc_hz as u128).div_ceil(1_000_000_000) as u64
}

/// Busy-waits at least `d` (spec §6.1), measured with the TSC: the tick
/// count cannot time a wait shorter than a tick, and the next tick may come
/// right after the wait starts. Before `init` has found the TSC frequency
/// there is no clock, and it returns at once.
pub fn sleep(d: Duration) {
    let end = rdtsc() + sleep_cycles(d, tsc_hz());
    while rdtsc() < end {
        core::hint::spin_loop();
    }
}

/// Whether `ticks` over `seconds` is within 10% of the nominal rate (the
/// timer self-check, cmdline `check=timer`).
pub fn tick_check_ok(ticks: u64, seconds: u64) -> bool {
    let expected = seconds * TICK_HZ;
    ticks.abs_diff(expected) * 10 <= expected
}

/// Counts timer ticks over `seconds` changes of a once-a-second clock (the
/// RTC's seconds register), starting at a change. `None` if the clock does
/// not change within 5 s, measured with the TSC.
pub fn count_ticks_over(seconds: u64, mut clock: impl FnMut() -> u8) -> Option<u64> {
    let mut next_change = || {
        let now = clock();
        let deadline = rdtsc() + 5 * tsc_hz();
        while clock() == now {
            if rdtsc() > deadline {
                return false;
            }
            core::hint::spin_loop();
        }
        true
    };
    if !next_change() {
        return None;
    }
    let start = ticks();
    for _ in 0..seconds {
        if !next_change() {
            return None;
        }
    }
    Some(ticks() - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sleep_is_never_shorter_than_asked() {
        // At tick 100 the next tick may come at once: 1 ms needs 2 more.
        assert_eq!(sleep_until(100, 1), 102);
        assert_eq!(sleep_until(100, 20), 121);
        assert_eq!(sleep_until(0, 0), 1);
        assert_eq!(sleep_until(u64::MAX - 5, 10), u64::MAX, "no overflow");
        assert_eq!(sleep_until(7, u64::MAX), u64::MAX);
    }

    #[test]
    fn tsc_cycles_become_time() {
        assert_eq!(
            cycles_to_duration(2_496_000_000, 2_496_000_000),
            Duration::from_secs(1)
        );
        assert_eq!(
            cycles_to_duration(2_496, 2_496_000_000),
            Duration::from_micros(1)
        );
        // Ten years of cycles at 5 GHz do not overflow.
        let ten_years = 10 * 365 * 86_400;
        assert_eq!(
            cycles_to_duration(ten_years * 5_000_000_000, 5_000_000_000),
            Duration::from_secs(ten_years)
        );
    }

    #[test]
    fn nuc_cpuid_gives_2496_mhz() {
        // The NUC's i7-1260P: leaf 0x15 = (2, 130, 38.4 MHz), leaf 0x16 base
        // 2500 MHz. Linux also reports "Detected 2496.000 MHz TSC".
        assert_eq!(
            tsc_hz_from_cpuid(0x20, [2, 0x82, 0x249_F000], 0x9C4),
            Some((2_496_000_000, TscSource::Cpuid15))
        );
    }

    #[test]
    fn missing_crystal_falls_back_to_the_base_frequency() {
        assert_eq!(
            tsc_hz_from_cpuid(0x16, [2, 176, 0], 2200),
            Some((2_200_000_000, TscSource::Cpuid16))
        );
        assert_eq!(
            tsc_hz_from_cpuid(0x15, [2, 176, 0], 2200),
            None,
            "leaf 0x16 absent"
        );
    }

    #[test]
    fn missing_or_empty_leaf_15_means_no_cpuid_frequency() {
        assert_eq!(tsc_hz_from_cpuid(0x0D, [2, 130, 38_400_000], 2500), None);
        assert_eq!(tsc_hz_from_cpuid(0x16, [0, 0, 0], 2500), None);
    }

    #[test]
    fn hpet_measurement_math() {
        // QEMU's HPET: 100 MHz (10 ns = 10_000_000 fs). 50 ms is 5_000_000
        // counts; a 3 GHz TSC moves 150_000_000 cycles meanwhile.
        assert_eq!(
            tsc_hz_from_hpet(150_000_000, 5_000_000, 10_000_000),
            3_000_000_000
        );
        // The NUC's 19.2 MHz HPET (52_083_333 fs), 2496 MHz TSC, 960_000 counts.
        let hz = tsc_hz_from_hpet(124_800_000, 960_000, 52_083_333);
        assert!(hz.abs_diff(2_496_000_000) < 1_000, "{hz}");
        assert!(hpet_period_ok(10_000_000));
        assert!(!hpet_period_ok(0));
        assert!(!hpet_period_ok(100_000_001));
    }

    #[test]
    fn lapic_rate_against_the_tsc() {
        // 2.4 MHz timer input (38.4 MHz crystal / 16) over 50 ms of a 2496 MHz TSC.
        assert_eq!(rate(120_000, 124_800_000, 2_496_000_000), 2_400_000);
    }

    #[test]
    fn sleep_waits_whole_tsc_cycles_rounded_up() {
        // Sub-millisecond waits (xHCI port resets need them) are not zero.
        assert_eq!(
            sleep_cycles(Duration::from_micros(10), 2_496_000_000),
            24_960
        );
        assert_eq!(
            sleep_cycles(Duration::from_nanos(1), 2_500_000_000),
            3,
            "2.5 rounds up"
        );
        assert_eq!(
            sleep_cycles(Duration::from_millis(1500), 3_000_000_000),
            4_500_000_000
        );
        assert_eq!(sleep_cycles(Duration::ZERO, 3_000_000_000), 0);
    }

    #[test]
    fn tick_check_allows_ten_percent() {
        assert!(tick_check_ok(3000, 3));
        assert!(tick_check_ok(2700, 3));
        assert!(tick_check_ok(3300, 3));
        assert!(!tick_check_ok(2699, 3));
        assert!(!tick_check_ok(3301, 3));
        assert!(!tick_check_ok(0, 3));
    }

    #[test]
    fn status_text() {
        let info = TimerInfo {
            tsc_hz: 2_496_000_000,
            source: TscSource::Cpuid15,
            lapic_mode: Mode::XApic { phys: 0xFEE0_0000 },
        };
        assert_eq!(
            info.to_string(),
            "TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)"
        );
        let info = TimerInfo {
            tsc_hz: 2_893_415_123,
            source: TscSource::Hpet,
            lapic_mode: Mode::X2Apic,
        };
        assert_eq!(
            info.to_string(),
            "TSC 2893.415 MHz (HPET), 1000 Hz tick (x2APIC)"
        );
    }
}
