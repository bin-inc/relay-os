//! One host controller with its class drivers (spec §6): devices are set up
//! when they appear on a root port (at start and when plugged in later),
//! boot-keyboard interfaces go to the keyboard driver, and a device's
//! drivers are dropped when it goes away. The kernel keeps one `Host` per
//! xHCI controller and polls it from the console.

use crate::hid::{BootKeyboard, KeyEvent, is_boot_keyboard};
use crate::xhci::{Device, Xhci};
use crate::{Hal, Speed, UsbError};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::fmt;

/// Key events kept for the console at most; beyond this new ones are
/// dropped (nobody is reading).
pub const MAX_EVENTS: usize = 256;

/// What happened when a device was set up, for the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    pub port: u8,
    pub outcome: Result<Found, UsbError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub vendor: u16,
    pub product: u16,
    pub speed: Speed,
    /// Boot-keyboard interfaces now in use.
    pub keyboards: usize,
    /// Why a boot-keyboard interface could not be started, if one could
    /// not: the screen must say so, because `dmesg` needs a keyboard.
    pub not_started: Option<UsbError>,
}

impl fmt::Display for Attached {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "port {}: ", self.port)?;
        match &self.outcome {
            Err(e) => write!(f, "setup failed: {e}"),
            Ok(d) => {
                write!(f, "{:04x}:{:04x} {}, ", d.vendor, d.product, d.speed)?;
                match (d.keyboards, d.not_started) {
                    (0, None) => write!(f, "not claimed"),
                    (0, Some(e)) => write!(f, "keyboard not started: {e}"),
                    (1, None) => write!(f, "keyboard"),
                    (n, None) => write!(f, "{n} keyboards"),
                    (n, Some(e)) => write!(f, "{n} keyboards, another not started: {e}"),
                }
            }
        }
    }
}

pub struct Host<H: Hal> {
    xhci: Xhci<H>,
    keyboards: Vec<BootKeyboard>,
    events: VecDeque<KeyEvent>,
}

impl<H: Hal> Host<H> {
    pub fn new(xhci: Xhci<H>) -> Host<H> {
        Host {
            xhci,
            keyboards: Vec::new(),
            events: VecDeque::new(),
        }
    }

    pub fn xhci(&self) -> &Xhci<H> {
        &self.xhci
    }

    /// Boot keyboards in use.
    pub fn keyboards(&self) -> usize {
        self.keyboards.len()
    }

    /// Handles ports that changed: sets up new devices, drops those that
    /// went away, and lets the keyboards do their blocking work (LEDs,
    /// recovery). Waits while a device is set up (a few hundred ms), so it
    /// runs only where the caller may block. Returns one line per device
    /// set up.
    pub fn service(&mut self) -> Vec<Attached> {
        let mut attached = Vec::new();
        for change in self.xhci.port_changes() {
            if let Some(slot) = self.xhci.slot_of_port(change.port)
                && (!change.connected || change.reconnected)
            {
                self.drop_device(slot);
            }
            if change.connected && self.xhci.slot_of_port(change.port).is_none() {
                attached.push(self.attach(change.port));
            }
        }
        for k in &mut self.keyboards {
            k.service(&mut self.xhci);
        }
        attached
    }

    /// Processes events and keyboard reports. Never waits.
    pub fn poll(&mut self) {
        self.xhci.poll();
        for k in &mut self.keyboards {
            k.poll(&mut self.xhci, &mut self.events);
        }
        self.events.truncate(MAX_EVENTS);
    }

    /// The oldest key event not yet taken.
    pub fn next_key(&mut self) -> Option<KeyEvent> {
        self.events.pop_front()
    }

    fn attach(&mut self, port: u8) -> Attached {
        let outcome = self.xhci.attach(port).and_then(|d| self.claim(d));
        if let Err(e) = &outcome {
            self.log(format_args!("port {port}: setup failed: {e}"));
        }
        Attached { port, outcome }
    }

    /// Configures the device for the interfaces a driver wants and starts
    /// the drivers. A device nothing claims stays addressed but unused.
    fn claim(&mut self, d: Device) -> Result<Found, UsbError> {
        let interfaces: Vec<u8> = d
            .configuration
            .interfaces
            .iter()
            .filter(|i| is_boot_keyboard(i))
            .map(|i| i.number)
            .collect();
        let mut found = Found {
            vendor: d.descriptor.vendor,
            product: d.descriptor.product,
            speed: d.speed,
            keyboards: 0,
            not_started: None,
        };
        if interfaces.is_empty() {
            self.log(format_args!("slot {}: no driver for this device", d.slot));
            return Ok(found);
        }
        if let Err(e) = self.xhci.configure(&d, &interfaces) {
            self.xhci.detach(d.slot);
            return Err(e);
        }
        for iface in d
            .configuration
            .interfaces
            .iter()
            .filter(|i| interfaces.contains(&i.number))
        {
            match BootKeyboard::start(&mut self.xhci, d.slot, iface) {
                Ok(k) => {
                    self.keyboards.push(k);
                    found.keyboards += 1;
                }
                Err(e) => {
                    self.log(format_args!(
                        "slot {} interface {}: keyboard not started: {e}",
                        d.slot, iface.number
                    ));
                    found.not_started = Some(e);
                }
            }
        }
        Ok(found)
    }

    fn log(&self, args: fmt::Arguments) {
        self.xhci
            .hal()
            .log(format_args!("xhci {}: {args}", self.xhci.name()));
    }

    /// The device in `slot` is gone: its keyboards stop (a held key stops
    /// repeating with them) and the controller forgets it.
    fn drop_device(&mut self, slot: u8) {
        self.keyboards.retain(|k| k.slot() != slot);
        self.xhci.detach(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Key;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::string::{String, ToString};
    use core::time::Duration;

    fn host(config: FakeConfig) -> (FakeHal, Host<FakeHal>) {
        let (hal, xhci) = start(config);
        (hal, Host::new(xhci))
    }

    /// Polls for `ms` milliseconds of fake time and returns the characters
    /// of the key presses that came out.
    fn typed(hal: &FakeHal, host: &mut Host<FakeHal>, ms: u64) -> String {
        let mut s = String::new();
        for _ in 0..ms {
            host.poll();
            while let Some(e) = host.next_key() {
                if let (true, Key::Char(c)) = (e.pressed, e.key) {
                    s.push(c as char);
                }
            }
            hal.sleep(Duration::from_millis(1));
        }
        s
    }

    fn key_report(usage: u8) -> [u8; 8] {
        [0, 0, usage, 0, 0, 0, 0, 0]
    }

    #[test]
    fn devices_present_at_start_are_set_up_and_reported() {
        // Laid out as QEMU's e2e machine: USB 3 ports first.
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.fake().plug(2, FakeUsbDevice::kingston_stick());
        hal.fake().plug(5, FakeUsbDevice::qemu_keyboard());
        // The stick's USB 3 link trains before its port shows a connection.
        hal.sleep(Duration::from_millis(60));
        let lines: Vec<String> = host.service().iter().map(|a| a.to_string()).collect();
        assert_eq!(
            lines,
            [
                "port 2: 0951:1666 SuperSpeed, not claimed",
                "port 5: 0627:0001 high-speed, keyboard",
            ]
        );
        assert_eq!(host.keyboards(), 1);
        // Nothing changed since: nothing more to do.
        assert!(host.service().is_empty());
    }

    #[test]
    fn typing_on_a_keyboard_gives_key_events() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for usage in [0x0B, 0x00, 0x0C, 0x00] {
            k120.borrow_mut().push_in(0x81, &key_report(usage));
        }
        assert_eq!(typed(&hal, &mut host, 200), "hi");
    }

    #[test]
    fn only_the_receivers_keyboard_interface_is_claimed() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(1, FakeUsbDevice::unifying_receiver());
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 1: 046d:c534 full-speed, keyboard"
        );
        let slot = host.xhci().slot_of_port(1).unwrap() as usize;
        // Interface 0's endpoint 0x81 (DCI 3) only; the mouse's 0x82 and
        // the vendor interface's 0x83 stay unconfigured.
        assert!(hal.fake().endpoint(slot, 3).is_some());
        assert!(hal.fake().endpoint(slot, 5).is_none());
        assert!(hal.fake().endpoint(slot, 7).is_none());
    }

    #[test]
    fn a_keyboard_plugged_in_later_is_picked_up_and_dropped_when_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        assert!(host.service().is_empty());
        let before = hal.outstanding_dma();
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1);
        hal.fake().unplug(3);
        assert!(host.service().is_empty());
        assert_eq!(host.keyboards(), 0);
        assert_eq!(host.xhci().slot_of_port(3), None);
        assert_eq!(
            hal.outstanding_dma(),
            before,
            "the device's memory is freed"
        );
        // Plugged in again, it works again.
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        k120.borrow_mut().push_in(0x81, &key_report(0x00));
        assert_eq!(typed(&hal, &mut host, 100), "a");
    }

    #[test]
    fn a_quick_replug_replaces_the_device() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        host.service();
        // Unplugged and another keyboard plugged in before the console
        // looked again: the port is connected both times.
        hal.fake().unplug(3);
        let other = FakeUsbDevice::qemu_keyboard();
        hal.fake().plug(3, other.clone());
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(
            attached[0].to_string(),
            "port 3: 0627:0001 high-speed, keyboard"
        );
        assert_eq!(host.keyboards(), 1);
        other.borrow_mut().push_in(0x81, &key_report(0x05));
        assert_eq!(typed(&hal, &mut host, 100), "b");
    }

    #[test]
    fn a_held_key_stops_repeating_when_its_keyboard_is_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(typed(&hal, &mut host, 2000), "");
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_keyboard_whose_transfers_just_stop_is_dropped_too() {
        // A controller that does not fail the queued report on unplug: the
        // keyboard never sees an error, so only the port change stops it.
        let mut config = FakeConfig::intel();
        config.fail_transfers_on_unplug = false;
        let (hal, mut host) = host(config);
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(host.keyboards(), 0);
        assert_eq!(typed(&hal, &mut host, 2000), "");
    }

    #[test]
    fn a_keyboard_that_cannot_be_started_says_so_on_screen() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x0B, 0); // SET_PROTOCOL(boot)
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 3: 046d:c31c low-speed, keyboard not started: stalled"
        );
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_device_that_fails_setup_is_reported_once() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x06, 0x0100);
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(attached[0].to_string(), "port 3: setup failed: stalled");
        assert!(host.service().is_empty());
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: setup failed: stalled")
        );
    }

    #[test]
    fn unread_key_events_are_bounded() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for _ in 0..MAX_EVENTS {
            k120.borrow_mut().push_in(0x81, &key_report(0x04));
            k120.borrow_mut().push_in(0x81, &key_report(0x00));
        }
        for _ in 0..4 * MAX_EVENTS {
            host.poll();
            hal.sleep(Duration::from_millis(1));
        }
        assert_eq!(host.events.len(), MAX_EVENTS);
    }

    #[test]
    fn a_keyboard_plugged_in_before_boot_is_found_by_the_first_service() {
        use crate::testing::{FAKE_BAR, FAKE_BAR_LEN};
        let hal = FakeHal::with_controller(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        let xhci = Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0").unwrap();
        let mut host = Host::new(xhci);
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1, "[ ok ] keyboard at boot");
    }
}
