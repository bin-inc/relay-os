//! The xHCI host controller driver (spec §6.2).
// The driver is built bottom-up: parts land before their users.
#![allow(dead_code)]

/// Logs one line starting "xhci <name>: ", as every line of this driver
/// does (spec §13: the NUC is debugged from a photo of `dmesg`).
macro_rules! xlog {
    ($hal:expr, $name:expr, $($arg:tt)*) => {
        $crate::Hal::log($hal, format_args!("xhci {}: {}", $name, format_args!($($arg)*)))
    };
}

mod caps;
mod command;
mod context;
mod init;
mod regs;
mod ring;
mod start;
mod trb;

use crate::{DmaBuf, Hal};
use alloc::string::String;
use alloc::vec::Vec;
use caps::PortProtocol;
use command::Pending;
use core::fmt;
use regs::Regs;
use ring::{EventRing, ProducerRing};
use start::Scratchpads;

/// What the controller reported about itself (for the status line and
/// dmesg).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControllerInfo {
    /// HCIVERSION, BCD: 0x0120 is 1.20.
    pub version: u16,
    pub max_slots: u8,
    pub ports: u8,
    /// Ports the Supported Protocol capabilities name USB 2 and USB 3.
    pub usb2_ports: u8,
    pub usb3_ports: u8,
    /// 32 or 64 bytes (HCCPARAMS1.CSZ).
    pub context_size: usize,
    pub scratchpads: u16,
}

impl fmt::Display for ControllerInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "xHCI {:x}.{:02x}, {} ports ({} USB 2, {} USB 3), {}-byte contexts, {} scratchpad{}",
            self.version >> 8,
            self.version & 0xFF,
            self.ports,
            self.usb2_ports,
            self.usb3_ports,
            self.context_size,
            self.scratchpads,
            if self.scratchpads == 1 { "" } else { "s" },
        )
    }
}

/// One xHCI controller, brought up by [`Xhci::new`].
pub struct Xhci<H: Hal> {
    hal: H,
    name: String,
    regs: Regs,
    info: ControllerInfo,
    /// Each root port's protocol; index 0 is port 1.
    ports: Vec<Option<PortProtocol>>,
    dcbaa: DmaBuf,
    scratchpads: Option<Scratchpads>,
    commands: ProducerRing,
    events: EventRing,
    /// The command in flight (one at a time).
    pending: Option<Pending>,
    /// A Command Ring Stopped event came since the last abort.
    ring_stopped: bool,
    /// Set when the controller stopped working: nothing is sent to it any
    /// more (`UsbError::ControllerDead`).
    dead: bool,
}

impl<H: Hal> Xhci<H> {
    pub fn info(&self) -> &ControllerInfo {
        &self.info
    }

    /// The PCI address every log line starts with.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hal(&self) -> &H {
        &self.hal
    }
}

#[cfg(test)]
mod tests {
    use super::caps::{CapList, PortProtocol, port_map, protocols};
    use super::regs::{Params, Regs};
    use super::*;
    use crate::Hal;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};
    use alloc::string::ToString;

    fn info(config: FakeConfig) -> ControllerInfo {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let params = Params::read(&hal, &regs);
        let list = CapList::walk(&hal, &regs, params.xecp);
        let map = port_map(params.ports, &protocols(&hal, &regs, &list).usable);
        let count = |kind| map.iter().filter(|&&p| p == Some(kind)).count() as u8;
        params.info(count(PortProtocol::Usb2), count(PortProtocol::Usb3))
    }

    #[test]
    fn the_qemu_controller_describes_itself() {
        let i = info(FakeConfig::basic());
        assert_eq!(
            i,
            ControllerInfo {
                version: 0x0100,
                max_slots: 64,
                ports: 8,
                usb2_ports: 4,
                usb3_ports: 4,
                context_size: 32,
                scratchpads: 0,
            }
        );
        assert_eq!(
            i.to_string(),
            "xHCI 1.00, 8 ports (4 USB 2, 4 USB 3), 32-byte contexts, 0 scratchpads"
        );
    }

    #[test]
    fn the_intel_controller_describes_itself() {
        let i = info(FakeConfig::intel());
        assert_eq!((i.usb2_ports, i.usb3_ports, i.scratchpads), (12, 4, 2));
        assert_eq!(
            i.to_string(),
            "xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 64-byte contexts, 2 scratchpads"
        );
    }

    #[test]
    fn versions_print_as_two_bcd_digits_and_one_scratchpad_is_singular() {
        let mut i = info(FakeConfig::basic());
        i.version = 0x0110;
        i.scratchpads = 1;
        assert!(i.to_string().starts_with("xHCI 1.10, "));
        assert!(i.to_string().ends_with(", 1 scratchpad"));
    }
}
