//! Bringing the controller up (spec §6.2): taking it over from the BIOS,
//! halting and resetting it. Each hardware quirk is its own function.

use super::caps::{CapList, LEGACY_SUPPORT};
use super::regs::{ALL_ONES, CNR, HCH, HCRST, RUN, Regs, USBCMD, USBSTS};
use crate::{Hal, UsbError};
use core::fmt;
use core::time::Duration;

/// Every register wait (spec §6.2).
pub const REGISTER_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a register wait looks again.
const POLL_INTERVAL: Duration = Duration::from_micros(100);
/// Linux (xhci_reset) waits this long after HCRST before touching any
/// register: some Intel hosts hang otherwise.
const HCRST_SETTLE: Duration = Duration::from_millis(1);

// USBLEGSUP (xHCI 7.1.1).
const BIOS_OWNED: u32 = 1 << 16;
const OS_OWNED: u32 = 1 << 24;
// USBLEGCTLSTS (xHCI 7.1.2): the SMI enables, and the SMI status bits,
// which are cleared by writing 1.
const LEGCTLSTS: usize = 4;
const SMI_ENABLES: u32 = 1 | 1 << 4 | 1 << 13 | 1 << 14 | 1 << 15;
const SMI_STATUS: u32 = 0x7 << 29;

/// Polls `done` until it holds, sleeping in between, for up to `timeout`.
/// Returns how long it took, or `None` on a timeout.
pub fn wait_for<H: Hal>(
    hal: &H,
    timeout: Duration,
    mut done: impl FnMut() -> bool,
) -> Option<Duration> {
    let start = hal.now();
    loop {
        if done() {
            return Some(hal.now() - start);
        }
        if hal.now() - start >= timeout {
            return None;
        }
        hal.sleep(POLL_INTERVAL);
    }
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

/// How the BIOS handoff went.
enum Handoff {
    NotBiosOwned,
    HandedOver(Duration),
    TakenOver,
}

impl fmt::Display for Handoff {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Handoff::NotBiosOwned => write!(f, "not BIOS owned"),
            Handoff::HandedOver(t) => write!(f, "BIOS owned, handed over after {} ms", ms(*t)),
            Handoff::TakenOver => write!(
                f,
                "BIOS owned, did not let go in {} ms, taken over",
                ms(REGISTER_TIMEOUT)
            ),
        }
    }
}

/// Takes the controller from the BIOS (xHCI 4.22.1): asks for it through
/// USB Legacy Support, waits up to 1 s, takes it anyway if the BIOS does
/// not let go (as Linux does), then turns the BIOS's SMIs off. Without
/// this the NUC firmware's SMM keyboard emulation keeps interfering.
pub fn bios_handoff<H: Hal>(hal: &H, regs: &Regs, caps: &CapList, name: &str) {
    let Some(at) = caps.find(LEGACY_SUPPORT) else {
        xlog!(hal, name, "no legacy support capability");
        return;
    };
    // The list comes from the hardware: its capability may sit where the
    // control register after it is outside the BAR.
    if regs.try_read(hal, at + LEGCTLSTS).is_none() {
        xlog!(
            hal,
            name,
            "legacy support at {at:#x}: control register outside the BAR; no handoff"
        );
        return;
    }
    let sup = regs.read(hal, at);
    regs.write(hal, at, sup | OS_OWNED);
    let outcome = if sup & BIOS_OWNED == 0 {
        Handoff::NotBiosOwned
    } else if let Some(t) = wait_for(hal, REGISTER_TIMEOUT, || {
        regs.read(hal, at) & BIOS_OWNED == 0
    }) {
        Handoff::HandedOver(t)
    } else {
        let sup = regs.read(hal, at);
        regs.write(hal, at, sup & !BIOS_OWNED | OS_OWNED);
        Handoff::TakenOver
    };
    let ctl = regs.read(hal, at + LEGCTLSTS);
    // Reserved bits are written back as read; status bits are RW1C.
    regs.write(hal, at + LEGCTLSTS, ctl & !SMI_ENABLES | SMI_STATUS);
    xlog!(
        hal,
        name,
        "legacy support at {at:#x}: {outcome}; SMI enables {:#010x} cleared",
        ctl & SMI_ENABLES
    );
}

/// Stops a running controller: clears R/S and waits up to 1 s for HCH
/// (xHCI 4.22.1: HCRST is only allowed on a halted controller).
pub fn halt<H: Hal>(hal: &H, regs: &Regs, name: &str) -> Result<(), UsbError> {
    let sts = regs.op_read(hal, USBSTS);
    if sts == ALL_ONES {
        xlog!(
            hal,
            name,
            "USBSTS reads {ALL_ONES:#x}: controller powered down or gone"
        );
        return Err(UsbError::NotResponding);
    }
    if sts & HCH != 0 {
        xlog!(hal, name, "already halted");
        return Ok(());
    }
    let cmd = regs.op_read(hal, USBCMD);
    regs.op_write(hal, USBCMD, cmd & !RUN);
    match wait_for(hal, REGISTER_TIMEOUT, || {
        regs.op_read(hal, USBSTS) & HCH != 0
    }) {
        Some(t) => {
            xlog!(hal, name, "halted after {} ms", ms(t));
            Ok(())
        }
        None => {
            xlog!(
                hal,
                name,
                "USBSTS.HCH still 0 {} ms after R/S = 0",
                ms(REGISTER_TIMEOUT)
            );
            Err(UsbError::Timeout)
        }
    }
}

/// Resets a halted controller: HCRST, 1 ms without touching it, then up to
/// 1 s for HCRST and USBSTS.CNR to read 0 (no register may be written
/// while CNR is 1, xHCI 5.4.2).
pub fn reset<H: Hal>(hal: &H, regs: &Regs, name: &str) -> Result<(), UsbError> {
    let start = hal.now();
    regs.op_write(hal, USBCMD, HCRST);
    hal.sleep(HCRST_SETTLE);
    let ready = || regs.op_read(hal, USBCMD) & HCRST == 0 && regs.op_read(hal, USBSTS) & CNR == 0;
    if wait_for(hal, REGISTER_TIMEOUT, ready).is_some() {
        xlog!(hal, name, "reset after {} ms", ms(hal.now() - start));
        return Ok(());
    }
    let stuck = if regs.op_read(hal, USBCMD) & HCRST != 0 {
        "USBCMD.HCRST"
    } else {
        "USBSTS.CNR"
    };
    xlog!(
        hal,
        name,
        "{stuck} still 1 {} ms after reset",
        ms(REGISTER_TIMEOUT)
    );
    Err(UsbError::Timeout)
}

#[cfg(test)]
mod tests {
    use super::super::regs::{CONFIG, Params};
    use super::*;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn setup(config: FakeConfig) -> (FakeHal, Regs, CapList) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let caps = CapList::walk(&hal, &regs, Params::read(&hal, &regs).xecp);
        (hal, regs, caps)
    }

    #[test]
    fn the_bios_hands_over_after_5_ms_and_its_smis_are_turned_off() {
        let (hal, regs, caps) = setup(FakeConfig::intel());
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        let [sup, ctl] = hal.fake().legacy();
        assert_eq!(sup & (BIOS_OWNED | OS_OWNED), OS_OWNED);
        assert_eq!(ctl & SMI_ENABLES, 0, "SMIs off");
        assert_eq!(ctl & SMI_STATUS, 0, "SMI status cleared");
        assert_eq!(ctl, 0x0000_0100, "the reserved bit is kept");
        let log = hal.log_text();
        assert_eq!(
            log,
            "xhci 00:14.0: legacy support at 0x8000: BIOS owned, handed over after 5 ms; \
             SMI enables 0x0000e011 cleared"
        );
    }

    #[test]
    fn a_bios_that_never_lets_go_is_taken_over_after_a_second() {
        let mut config = FakeConfig::intel();
        config.bios_release = None;
        let (hal, regs, caps) = setup(config);
        let start = hal.clock();
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        assert!(hal.clock() - start >= REGISTER_TIMEOUT);
        assert!(hal.clock() - start < REGISTER_TIMEOUT + Duration::from_millis(5));
        let [sup, ctl] = hal.fake().legacy();
        assert_eq!(sup & (BIOS_OWNED | OS_OWNED), OS_OWNED);
        assert_eq!(ctl & (SMI_ENABLES | SMI_STATUS), 0);
        assert!(
            hal.log_text()
                .contains("did not let go in 1000 ms, taken over; SMI")
        );
    }

    #[test]
    fn a_controller_the_bios_does_not_own_is_claimed_at_once() {
        let mut config = FakeConfig::intel();
        config.bios_owned = false;
        let (hal, regs, caps) = setup(config);
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        assert_eq!(hal.fake().legacy()[0], OS_OWNED);
        assert!(hal.clock() < Duration::from_millis(1));
        assert!(
            hal.log_text()
                .contains("legacy support at 0x8000: not BIOS owned")
        );
    }

    #[test]
    fn without_legacy_support_nothing_is_written() {
        let (hal, regs, caps) = setup(FakeConfig::basic());
        bios_handoff(&hal, &regs, &caps, "00:04.0");
        assert_eq!(hal.fake().cap_writes(), 0);
        assert_eq!(hal.log_text(), "xhci 00:04.0: no legacy support capability");
    }

    #[test]
    fn a_running_controller_is_halted_first() {
        let (hal, regs, _) = setup(FakeConfig::intel());
        assert_eq!(hal.fake().usbsts() & HCH, 0);
        halt(&hal, &regs, "00:14.0").unwrap();
        assert_eq!(hal.fake().usbcmd() & RUN, 0);
        assert_eq!(hal.fake().usbsts() & HCH, HCH);
        assert!(hal.log_text().contains("xhci 00:14.0: halted after 1 ms"));
    }

    #[test]
    fn a_halted_controller_is_left_alone() {
        let (hal, regs, _) = setup(FakeConfig::basic());
        halt(&hal, &regs, "q").unwrap();
        assert!(hal.log_text().contains("already halted"));
    }

    #[test]
    fn reset_waits_for_hcrst_and_then_for_cnr() {
        let mut config = FakeConfig::intel();
        config.running = false;
        config.reset_time = Some(Duration::from_millis(3));
        config.cnr_time = Some(Duration::from_millis(5));
        let (hal, regs, _) = setup(config);
        reset(&hal, &regs, "00:14.0").unwrap();
        assert_eq!(hal.fake().usbsts() & CNR, 0);
        assert_eq!(hal.fake().usbcmd() & HCRST, 0);
        // The fake panics on a write while CNR is 1.
        regs.op_write(&hal, CONFIG, 1);
        assert!(hal.clock() >= Duration::from_millis(8));
        assert!(hal.log_text().contains("xhci 00:14.0: reset after 8 ms"));
    }

    #[test]
    fn nothing_is_touched_in_the_millisecond_after_hcrst() {
        // The fake panics on any access within 1 ms of HCRST; QEMU's reset
        // is instant, so only that rule makes the driver wait.
        let (hal, regs, _) = setup(FakeConfig::basic());
        reset(&hal, &regs, "q").unwrap();
        assert!(hal.clock() >= Duration::from_millis(1));
    }

    #[test]
    fn a_halt_that_never_comes_times_out_naming_hch() {
        let mut config = FakeConfig::intel();
        config.halt_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(halt(&hal, &regs, "00:14.0"), Err(UsbError::Timeout));
        assert!(hal.clock() >= REGISTER_TIMEOUT);
        assert!(
            hal.log_text()
                .contains("USBSTS.HCH still 0 1000 ms after R/S = 0")
        );
    }

    #[test]
    fn a_reset_that_never_ends_times_out_naming_hcrst() {
        let mut config = FakeConfig::basic();
        config.reset_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(reset(&hal, &regs, "q"), Err(UsbError::Timeout));
        assert!(
            hal.log_text()
                .contains("USBCMD.HCRST still 1 1000 ms after reset")
        );
    }

    #[test]
    fn a_controller_never_ready_times_out_naming_cnr() {
        let mut config = FakeConfig::basic();
        config.cnr_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(reset(&hal, &regs, "q"), Err(UsbError::Timeout));
        assert!(hal.clock() >= REGISTER_TIMEOUT);
        assert!(
            hal.log_text()
                .contains("USBSTS.CNR still 1 1000 ms after reset")
        );
    }

    #[test]
    fn a_controller_reading_all_ones_is_not_responding() {
        let (hal, regs, _) = setup(FakeConfig::intel());
        hal.fake().config_mut().all_ones = true;
        assert_eq!(halt(&hal, &regs, "00:0d.0"), Err(UsbError::NotResponding));
        assert!(
            hal.log_text()
                .contains("xhci 00:0d.0: USBSTS reads 0xffffffff")
        );
    }
}
