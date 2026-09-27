//! Root ports (xHCI 4.19): PORTSC writes that change only what they mean
//! to, port reset (USB 2 ports are reset, USB 3 ports train by
//! themselves), and the list of ports whose connection changed.

use super::caps::PortProtocol;
use super::context::speed_from_id;
use super::init::wait_for;
use super::regs::{
    ALL_ONES, CCS, CHANGE_BITS, CSC, PED, PLS_SHIFT, PR, PRC, SPEED_SHIFT, WPR, WRC, portsc_neutral,
};
use super::{PortChange, Xhci};
use crate::{Hal, Speed, UsbError};
use alloc::vec::Vec;
use core::time::Duration;

/// How long a port reset or USB 3 link training may take (spec §6.2).
pub const PORT_RESET_TIMEOUT: Duration = Duration::from_millis(500);
/// Reset recovery before the device must answer (USB 2.0 7.1.7.5).
const RESET_RECOVERY: Duration = Duration::from_millis(10);
// PORTSC.PLS: link states (xHCI table 5-27).
const U0: u32 = 0;
const SS_INACTIVE: u32 = 6;
const COMPLIANCE: u32 = 10;
const POLLING: u32 = 7;

fn link_state(portsc: u32) -> u32 {
    portsc >> PLS_SHIFT & 0xF
}

/// Enabled with the link in U0: a trained USB 3 port.
fn trained(portsc: u32) -> bool {
    portsc & PED != 0 && link_state(portsc) == U0
}

impl<H: Hal> Xhci<H> {
    fn portsc(&self, port: u8) -> u32 {
        self.regs.portsc(&self.hal, port)
    }

    /// Clears the change bits in `bits` (RW1C) and nothing else.
    fn clear_changes(&self, port: u8, portsc: u32, bits: u32) {
        if portsc & bits != 0 {
            let value = portsc_neutral(portsc) | portsc & bits;
            self.regs.set_portsc(&self.hal, port, value);
        }
    }

    /// A USB 3 port whose link is still training (Polling, CCS 0), if any.
    pub(super) fn usb3_link_training(&self) -> Option<u8> {
        (1..=self.info.ports).find(|&port| {
            self.ports[port as usize - 1] == Some(PortProtocol::Usb3)
                && link_state(self.portsc(port)) == POLLING
        })
    }

    /// A Port Status Change Event: the port is looked at by the next
    /// `port_changes`, which also clears its change bits.
    pub(super) fn port_event(&mut self, port: u8) {
        if let Some(flag) = port
            .checked_sub(1)
            .and_then(|i| self.port_flags.get_mut(i as usize))
        {
            *flag = true;
        }
    }

    /// Ports whose status changed since the last call, in port order, with
    /// their change bits cleared. Right after `new` every port that is
    /// connected counts as changed, so the caller attaches boot-time devices
    /// and later hot-plugged ones the same way. Calls `poll` first.
    pub fn port_changes(&mut self) -> Vec<PortChange> {
        self.poll();
        let mut changes = Vec::new();
        if self.dead {
            return changes;
        }
        let first = core::mem::replace(&mut self.first_scan, false);
        for port in 1..=self.info.ports {
            let i = port as usize - 1;
            if !(first || self.port_flags[i]) {
                continue;
            }
            self.port_flags[i] = false;
            let sc = self.portsc(port);
            if sc == ALL_ONES {
                continue;
            }
            self.clear_changes(port, sc, CHANGE_BITS);
            let connected = sc & CCS != 0;
            let reconnected = sc & CSC != 0;
            if self.ports[i].is_none() || !(reconnected || first && connected) {
                continue;
            }
            let what = if connected {
                "connected"
            } else {
                "disconnected"
            };
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: {what}, PORTSC {sc:#010x}"
            );
            changes.push(PortChange {
                port,
                connected,
                reconnected,
            });
        }
        changes
    }

    /// Resets `port` as its protocol needs and returns the speed the device
    /// connected at.
    pub(super) fn reset_port(&mut self, port: u8) -> Result<Speed, UsbError> {
        match self.ports.get(port as usize - 1).copied().flatten() {
            Some(PortProtocol::Usb2) => self.reset_usb2(port)?,
            Some(PortProtocol::Usb3) => self.reset_usb3(port)?,
            None => return Err(UsbError::Unsupported("port without a supported protocol")),
        }
        let sc = self.portsc(port);
        let Some(speed) = speed_from_id((sc >> SPEED_SHIFT & 0xF) as u8) else {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: unknown speed, PORTSC {sc:#010x}"
            );
            return Err(UsbError::Unsupported("unknown port speed"));
        };
        xlog!(&self.hal, &self.name, "port {port}: reset done, {speed}");
        Ok(speed)
    }

    /// Waits up to 500 ms for `done` or a disconnect; the last PORTSC read.
    fn wait_port(&self, port: u8, done: impl Fn(u32) -> bool) -> (bool, u32) {
        let ok = wait_for(&self.hal, PORT_RESET_TIMEOUT, || {
            let sc = self.portsc(port);
            done(sc) || sc & CCS == 0
        });
        let sc = self.portsc(port);
        (ok.is_some() && sc & CCS != 0, sc)
    }

    /// The outcome when a wait on `port` did not end well.
    fn port_failed(&self, port: u8, sc: u32, what: &str) -> UsbError {
        if sc & CCS == 0 {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: disconnected during {what}"
            );
            UsbError::Disconnected
        } else {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: {what} failed, PORTSC {sc:#010x}"
            );
            UsbError::Timeout
        }
    }

    /// USB 2 (xHCI 4.3.1): PR, up to 500 ms for PRC, clear it, PED must be
    /// set, then 10 ms of reset recovery.
    fn reset_usb2(&mut self, port: u8) -> Result<(), UsbError> {
        let sc = self.portsc(port);
        self.regs
            .set_portsc(&self.hal, port, portsc_neutral(sc) | PR);
        let (ok, sc) = self.wait_port(port, |sc| sc & PRC != 0);
        self.clear_changes(port, sc, PRC);
        if !ok || sc & PED == 0 {
            return Err(self.port_failed(port, sc, "reset"));
        }
        self.hal.sleep(RESET_RECOVERY);
        Ok(())
    }

    /// USB 3 (xHCI 4.3.1): the link trains by itself. Once it is enabled in
    /// U0, a hot reset (PR) resets the device, as Linux does, so one that
    /// kept its address across HCRST (the stick the machine booted from)
    /// starts afresh. A link not in U0 within 500 ms, or in SS.Inactive or
    /// Compliance, gets one warm reset instead.
    fn reset_usb3(&mut self, port: u8) -> Result<(), UsbError> {
        let (ok, sc) = self.wait_port(port, |sc| {
            trained(sc) || matches!(link_state(sc), SS_INACTIVE | COMPLIANCE)
        });
        if ok && trained(sc) {
            self.regs
                .set_portsc(&self.hal, port, portsc_neutral(sc) | PR);
            let (ok, sc) = self.wait_port(port, |sc| sc & PRC != 0);
            self.clear_changes(port, sc, PRC);
            if !ok || !trained(sc) {
                return Err(self.port_failed(port, sc, "hot reset"));
            }
            return Ok(());
        }
        if sc & CCS == 0 {
            return Err(self.port_failed(port, sc, "link training"));
        }
        xlog!(
            &self.hal,
            &self.name,
            "port {port}: link not trained (PLS {}), warm reset",
            link_state(sc)
        );
        self.regs
            .set_portsc(&self.hal, port, portsc_neutral(sc) | WPR);
        let (ok, sc) = self.wait_port(port, |sc| sc & (PRC | WRC) != 0);
        self.clear_changes(port, sc, PRC | WRC);
        if !ok || !trained(sc) {
            return Err(self.port_failed(port, sc, "warm reset"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::regs::PP;
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec;

    fn change(port: u8, connected: bool, reconnected: bool) -> PortChange {
        PortChange {
            port,
            connected,
            reconnected,
        }
    }

    fn wpr_writes(hal: &FakeHal) -> usize {
        hal.fake()
            .portsc_writes()
            .iter()
            .filter(|(_, v)| v & WPR != 0)
            .count()
    }

    #[test]
    fn a_usb2_reset_gives_each_device_its_speed_and_leaves_the_port_enabled() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        for (port, speed) in [(1, Speed::Low), (2, Speed::Full), (3, Speed::High)] {
            hal.fake().plug(port, FakeUsbDevice::with_speed(speed));
            assert_eq!(xhci.reset_port(port), Ok(speed));
            let sc = hal.fake().portsc(port);
            assert_eq!(
                sc & (PED | PR | CHANGE_BITS),
                PED | CSC,
                "port {port}: only CSC left"
            );
            let done = hal.fake().reset_done_at(port).unwrap();
            assert!(
                hal.clock() - done >= RESET_RECOVERY,
                "port {port}: reset recovery"
            );
        }
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 1: reset done, low-speed")
        );
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: reset done, high-speed")
        );
        assert_eq!(wpr_writes(&hal), 0, "a USB 2 port never gets WPR");
    }

    #[test]
    fn clearing_change_bits_leaves_the_port_enabled() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        xhci.reset_port(3).unwrap();
        assert_eq!(xhci.port_changes(), vec![change(3, true, true)]);
        let sc = hal.fake().portsc(3);
        assert_eq!(sc & (PED | PP | CHANGE_BITS), PED | PP);
    }

    fn pr_writes(hal: &FakeHal, port: u8) -> usize {
        let writes = hal.fake().portsc_writes().to_vec();
        writes
            .iter()
            .filter(|&&(p, v)| p == port && v & PR != 0)
            .count()
    }

    /// Lets a USB 3 link plugged just now train (the fake takes 50 ms).
    fn train(hal: &FakeHal) {
        hal.sleep(Duration::from_millis(60));
    }

    #[test]
    fn a_usb3_link_trains_by_itself_and_then_gets_a_hot_reset() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(13, FakeUsbDevice::kingston_stick());
        assert_eq!(
            hal.fake().portsc(13) & CCS,
            0,
            "no connection while training"
        );
        train(&hal);
        assert_eq!(xhci.reset_port(13), Ok(Speed::Super));
        assert_eq!(pr_writes(&hal, 13), 1, "one hot reset");
        assert_eq!(wpr_writes(&hal), 0);
        let sc = hal.fake().portsc(13);
        assert_eq!(sc & (PED | CHANGE_BITS), PED | CSC, "enabled, PRC cleared");
        assert!(hal.log_text().contains("port 13: reset done, SuperSpeed"));
    }

    #[test]
    fn a_usb3_link_that_fails_to_train_gets_one_warm_reset() {
        let mut config = FakeConfig::intel();
        config.usb3_link_fails = true;
        let (hal, mut xhci) = start(config);
        hal.fake().plug(14, FakeUsbDevice::kingston_stick());
        train(&hal);
        assert_eq!(
            xhci.port_changes(),
            vec![change(14, true, true)],
            "SS.Inactive, CCS 1"
        );
        let before = hal.clock();
        assert_eq!(xhci.reset_port(14), Ok(Speed::Super));
        assert!(
            hal.clock() - before < Duration::from_millis(50),
            "no wait in SS.Inactive"
        );
        assert_eq!(wpr_writes(&hal), 1);
        assert_eq!(pr_writes(&hal, 14), 0, "the warm reset reset the device");
        assert_eq!(
            hal.fake().portsc(14) & CHANGE_BITS,
            0,
            "PRC and WRC cleared"
        );
        assert!(
            hal.log_text()
                .contains("port 14: link not trained (PLS 6), warm reset")
        );
    }

    #[test]
    fn devices_present_at_boot_are_in_the_first_port_changes() {
        for training in [50, 300] {
            let mut config = FakeConfig::intel();
            config.usb3_training = Duration::from_millis(training);
            let hal = FakeHal::with_controller(config);
            hal.fake().plug(3, FakeUsbDevice::k120());
            hal.fake().plug(13, FakeUsbDevice::kingston_stick());
            let mut xhci = Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
            assert_eq!(
                xhci.port_changes(),
                vec![change(3, true, true), change(13, true, true)],
                "USB 3 training for {training} ms"
            );
            let waited = hal.clock() - hal.fake().powered_at().unwrap();
            let want = Duration::from_millis(training.max(100));
            assert!(waited >= want && waited < want + Duration::from_millis(5));
        }
        // A link that never trains is given up after 1 s.
        let mut config = FakeConfig::intel();
        config.usb3_training = Duration::from_secs(5);
        let hal = FakeHal::with_controller(config);
        hal.fake().plug(13, FakeUsbDevice::kingston_stick());
        Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
        assert!(hal.clock() < Duration::from_millis(1050));
        assert!(
            hal.log_text()
                .contains("port 13: USB 3 link still training after")
        );
    }

    #[test]
    fn a_reset_that_never_completes_times_out_after_500_ms() {
        let mut config = FakeConfig::basic();
        config.port_reset_time = None;
        let (hal, mut xhci) = start(config);
        hal.fake().plug(2, FakeUsbDevice::qemu_keyboard());
        let before = hal.clock();
        assert_eq!(xhci.reset_port(2), Err(UsbError::Timeout));
        let waited = hal.clock() - before;
        assert!(waited >= PORT_RESET_TIMEOUT && waited < PORT_RESET_TIMEOUT * 2);
        assert!(hal.log_text().contains("port 2: reset failed, PORTSC 0x"));
        assert_eq!(wpr_writes(&hal), 0, "a USB 2 port never gets WPR");
    }

    #[test]
    fn a_device_unplugged_during_reset_is_disconnected() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.fake().plug(1, FakeUsbDevice::k120());
        hal.fake()
            .after(Duration::from_millis(3), |x, _| x.unplug(1));
        assert_eq!(xhci.reset_port(1), Err(UsbError::Disconnected));
        assert!(hal.log_text().contains("port 1: disconnected during reset"));
        hal.fake().plug(5, FakeUsbDevice::kingston_stick());
        train(&hal);
        hal.fake()
            .after(Duration::from_millis(3), |x, _| x.unplug(5));
        assert_eq!(xhci.reset_port(5), Err(UsbError::Disconnected));
    }

    #[test]
    fn right_after_start_every_connected_port_is_listed_once() {
        for config in [FakeConfig::basic(), FakeConfig::intel()] {
            let hal = FakeHal::with_controller(config.clone());
            let usb3 = if config.ppc { 13 } else { 5 };
            hal.fake().plug(1, FakeUsbDevice::k120());
            hal.fake().plug(3, FakeUsbDevice::usb2_stick());
            hal.fake().plug(usb3, FakeUsbDevice::kingston_stick());
            let mut xhci = Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
            assert_eq!(
                xhci.port_changes(),
                vec![
                    change(1, true, true),
                    change(3, true, true),
                    change(usb3, true, true)
                ]
            );
            assert_eq!(xhci.port_changes(), vec![]);
            for port in [1, 3, usb3] {
                assert_eq!(hal.fake().portsc(port) & CHANGE_BITS, 0, "port {port}");
            }
        }
    }

    #[test]
    fn plugs_and_unplugs_are_listed_once_each() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.port_changes(), vec![]);
        hal.fake().plug(2, FakeUsbDevice::k120());
        assert_eq!(xhci.port_changes(), vec![change(2, true, true)]);
        assert_eq!(xhci.port_changes(), vec![]);
        hal.fake().unplug(2);
        assert_eq!(xhci.port_changes(), vec![change(2, false, true)]);
        // Out and in again between two looks: the old device is gone.
        hal.fake().plug(2, FakeUsbDevice::k120());
        xhci.port_changes();
        hal.fake().unplug(2);
        hal.fake().plug(2, FakeUsbDevice::k120());
        assert_eq!(xhci.port_changes(), vec![change(2, true, true)]);
        assert_eq!(hal.fake().portsc(2) & CHANGE_BITS, 0);
    }

    #[test]
    fn a_reset_on_its_own_is_not_a_connection_change() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.fake().plug(4, FakeUsbDevice::k120());
        xhci.port_changes();
        xhci.reset_port(4).unwrap();
        assert_eq!(xhci.port_changes(), vec![]);
    }

    #[test]
    fn events_for_ports_that_do_not_exist_are_ignored() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        xhci.port_changes();
        hal.act(|x, dma| {
            x.post([0, 0, 1 << 24, 34 << 10], dma);
            x.post([200 << 24, 0, 1 << 24, 34 << 10], dma);
        });
        assert_eq!(xhci.port_changes(), vec![]);
    }
}
