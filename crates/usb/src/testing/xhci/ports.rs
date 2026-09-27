//! Root ports (xHCI 4.19, 5.4.8): PORTSC with its RW1C, RWS and RO bits,
//! USB 2 port reset, USB 3 link training, hot and warm reset, and devices
//! being plugged and unplugged. Every change bit rising from an all-clear
//! state posts a Port Status Change Event. A device present when its port
//! is reset or powered takes a while to show: a USB 2 device signals its
//! attach after `usb2_attach_delay`, a USB 3 link trains in Polling with
//! CCS 0 for `usb3_training`.

use super::FakeXhci;
use super::commands::{PORT_STATUS_CHANGE, SUCCESS};
use crate::testing::device::FakeDevice;
use crate::testing::hal::Dma;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

pub const CCS: u32 = 1 << 0;
pub const PED: u32 = 1 << 1;
pub const PR: u32 = 1 << 4;
const PLS: u32 = 0xF << 5;
pub const PP: u32 = 1 << 9;
const SPEED: u32 = 0xF << 10;
const PIC: u32 = 3 << 14;
const LWS: u32 = 1 << 16;
pub const CSC: u32 = 1 << 17;
pub const WRC: u32 = 1 << 19;
pub const PLC: u32 = 1 << 22;
pub const PRC: u32 = 1 << 21;
/// CSC, PEC, WRC, OCC, PRC, PLC, CEC: RW1C.
pub const CHANGES: u32 = 0x7F << 17;
const WAKE: u32 = 7 << 25;
pub const WPR: u32 = 1 << 31;

// Port link states.
const U0: u32 = 0;
const RX_DETECT: u32 = 5;
const SS_INACTIVE: u32 = 6;
const POLLING: u32 = 7;

/// How long a port reset or warm reset takes.
pub const RESET_TIME: Duration = Duration::from_millis(10);

pub type Device = Rc<RefCell<dyn FakeDevice>>;

fn with_pls(sc: u32, pls: u32) -> u32 {
    sc & !PLS | pls << 5
}

/// The Protocol Speed ID of the default table (xHCI 7.2.2.1.1).
fn speed_id(speed: crate::Speed) -> u32 {
    use crate::Speed::*;
    match speed {
        Full => 1,
        Low => 2,
        High => 3,
        Super => 4,
        SuperPlus => 5,
    }
}

impl FakeXhci {
    /// Connects `device` to `port` (1-based): CCS and CSC, and an event.
    pub fn plug(&mut self, port: u8, device: Device) {
        let i = port as usize - 1;
        let speed = device.borrow().speed();
        if speed.is_superspeed() != self.usb3[i] {
            panic!("fake xhci: a {speed} device cannot be on port {port}");
        }
        assert!(self.devices[i].is_none(), "fake: port {port} is taken");
        self.devices[i] = Some(device);
        if self.portsc[i] & PP != 0 {
            self.connect(i, false);
        }
    }

    /// Disconnects `port`: CCS and PED clear, CSC set, an event; the
    /// device's transfers in progress fail.
    pub fn unplug(&mut self, port: u8) {
        let i = port as usize - 1;
        if self.devices[i].take().is_none() {
            return;
        }
        self.port_generation[i] += 1;
        let was = self.portsc[i];
        let sc = was & !(CCS | PED | PR | SPEED);
        self.portsc[i] = with_pls(sc, RX_DETECT);
        if was & CCS != 0 {
            self.set_changes(i, CSC);
        }
        if self.config.fail_transfers_on_unplug {
            self.after(Duration::ZERO, move |x, dma| x.device_gone(port, dma));
        }
    }

    pub fn device(&self, port: u8) -> Option<Device> {
        self.devices[port as usize - 1].clone()
    }

    /// When the last reset of `port` completed.
    pub fn reset_done_at(&self, port: u8) -> Option<Duration> {
        self.reset_done[port as usize - 1]
    }

    /// Every PORTSC write so far: port and value.
    pub fn portsc_writes(&self) -> &[(u8, u32)] {
        &self.portsc_writes
    }

    /// A device on a powered port. `boot`: the port was just reset or
    /// powered, so a USB 2 device takes `usb2_attach_delay` to signal its
    /// attach (USB 2.0 7.1.7.3). A USB 3 link trains first, in Polling with
    /// CCS 0 (xHCI 4.19.1.2), then is enabled in U0, or ends in SS.Inactive
    /// if it fails to train.
    fn connect(&mut self, i: usize, boot: bool) {
        self.port_generation[i] += 1;
        let generation = self.port_generation[i];
        if self.usb3[i] {
            self.portsc[i] = with_pls(self.portsc[i] & !CCS, POLLING);
            self.after(self.config.usb3_training, move |x, _| {
                x.trained(i, generation)
            });
        } else if boot {
            self.after(self.config.usb2_attach_delay, move |x, _| {
                if x.port_generation[i] == generation && x.devices[i].is_some() {
                    x.attached(i);
                }
            });
        } else {
            self.attached(i);
        }
    }

    /// A USB 2 device signalled its attach: connected, not yet enabled.
    fn attached(&mut self, i: usize) {
        self.portsc[i] = with_pls(self.portsc[i] | CCS, POLLING);
        self.set_changes(i, CSC);
    }

    /// The end of USB 3 link training.
    fn trained(&mut self, i: usize, generation: u64) {
        if self.port_generation[i] != generation || self.devices[i].is_none() {
            return;
        }
        if self.config.usb3_link_fails {
            self.portsc[i] = with_pls(self.portsc[i] | CCS, SS_INACTIVE);
        } else {
            self.enable(i, generation);
        }
        self.set_changes(i, CSC);
    }

    /// The link is up: connected, enabled, in U0, at the device's speed.
    fn enable(&mut self, i: usize, generation: u64) {
        if self.port_generation[i] != generation {
            return;
        }
        let Some(dev) = &self.devices[i] else {
            return;
        };
        let speed = speed_id(dev.borrow().speed());
        let sc = self.portsc[i] & !SPEED | CCS | PED | speed << 10;
        self.portsc[i] = with_pls(sc, U0);
    }

    /// Sets change bits; an event when none was set before (xHCI 4.19.2).
    fn set_changes(&mut self, i: usize, bits: u32) {
        let before = self.portsc[i] & CHANGES;
        self.portsc[i] |= bits;
        if before == 0 {
            self.port_event(i);
        }
    }

    fn port_event(&mut self, i: usize) {
        let trb = [
            ((i + 1) as u32) << 24,
            0,
            SUCCESS << 24,
            PORT_STATUS_CHANGE << 10,
        ];
        self.pending_events.push(trb);
    }

    /// Reconnects the devices on powered ports (after power-on or HCRST).
    pub(super) fn reconnect_ports(&mut self) {
        for i in 0..self.portsc.len() {
            if self.portsc[i] & PP != 0 && self.devices[i].is_some() {
                self.connect(i, true);
            }
        }
    }

    pub(super) fn write_portsc(&mut self, i: usize, value: u32, _dma: &Dma) {
        self.port_writes += 1;
        self.portsc_writes.push(((i + 1) as u8, value));
        if value & LWS != 0 {
            panic!("fake xhci: port {} link state written", i + 1);
        }
        let old = self.portsc[i];
        // Change bits and PED are RW1C: a 1 clears them, and a 1 in PED
        // disables the port (xHCI 4.19.1.1).
        let mut sc = old & !(value & (CHANGES | PED));
        sc = sc & !(PIC | WAKE) | value & (PIC | WAKE);
        self.portsc[i] = sc;
        if self.config.ppc && value & PP == 0 && old & PP != 0 {
            panic!("fake xhci: port {} powered off by a PORTSC write", i + 1);
        }
        if self.config.ppc && value & PP != 0 && old & PP == 0 {
            self.portsc[i] |= PP;
            self.powered_at = Some(self.now);
            if self.devices[i].is_some() {
                self.connect(i, true);
            }
        }
        if value & WPR != 0 {
            if !self.usb3[i] {
                panic!("fake xhci: warm reset on USB 2 port {}", i + 1);
            }
            self.start_reset(i, true);
        } else if value & PR != 0 {
            self.start_reset(i, false);
        }
    }

    /// PR or WPR: the port is disabled while the reset runs; afterwards
    /// PRC (and WRC) is set and, with a device, the port is enabled (on a
    /// USB 3 port: in U0). A warm reset takes the link down and trains it
    /// again, so it also reports a connect and a link state change (CSC,
    /// PLC), which is why Linux clears both after every SuperSpeed reset.
    fn start_reset(&mut self, i: usize, warm: bool) {
        if self.portsc[i] & PP == 0 {
            return;
        }
        self.portsc[i] = self.portsc[i] & !PED | PR;
        let Some(delay) = self.config.port_reset_time else {
            return;
        };
        self.port_generation[i] += 1;
        let generation = self.port_generation[i];
        self.after(delay, move |x, _| {
            x.portsc[i] &= !PR;
            x.reset_done[i] = Some(x.now);
            if x.devices[i].is_some() && x.port_generation[i] == generation {
                x.enable(i, generation);
            }
            let changes = match (warm, x.devices[i].is_some()) {
                (true, true) => PRC | WRC | CSC | PLC,
                (true, false) => PRC | WRC,
                (false, _) => PRC,
            };
            x.set_changes(i, changes);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;
    use crate::testing::device::FakeUsbDevice;

    fn basic() -> FakeXhci {
        FakeXhci::new(FakeConfig::basic())
    }

    #[test]
    fn a_plugged_device_is_connected_but_not_enabled() {
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        assert_eq!(x.portsc(1) & (CCS | PED | CSC), CCS | CSC);
        assert_eq!(x.pending_events.len(), 1);
        x.plug(2, FakeUsbDevice::k120());
        x.unplug(2);
        assert_eq!(x.pending_events.len(), 2, "CSC was already set: one event");
    }

    #[test]
    fn a_usb2_reset_enables_the_port_after_10_ms() {
        let dma = Dma::default();
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        x.write_portsc(0, PP | PR, &dma);
        assert_eq!(x.portsc(1) & (PR | PED), PR);
        x.advance_to(RESET_TIME, &dma);
        assert_eq!(x.portsc(1) & (PR | PED | PRC), PED | PRC);
        assert_eq!(x.portsc(1) >> 10 & 0xF, 2, "low-speed");
    }

    #[test]
    fn writing_ped_disables_the_port_and_change_bits_clear_on_1() {
        let dma = Dma::default();
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        x.write_portsc(0, PP | PR, &dma);
        x.advance_to(RESET_TIME, &dma);
        x.write_portsc(0, PP | CSC, &dma);
        assert_eq!(x.portsc(1) & (PED | CSC | PRC), PED | PRC);
        x.write_portsc(0, x.portsc(1), &dma);
        assert_eq!(
            x.portsc(1) & (PED | CHANGES),
            0,
            "writing back what was read"
        );
    }

    #[test]
    #[should_panic(expected = "warm reset on USB 2 port 2")]
    fn usb2_ports_have_no_warm_reset() {
        basic().write_portsc(1, PP | WPR, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "cannot be on port 5")]
    fn a_usb2_device_does_not_fit_a_usb3_port() {
        basic().plug(5, FakeUsbDevice::k120());
    }
}
