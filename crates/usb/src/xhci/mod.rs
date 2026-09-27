//! The xHCI host controller driver (spec §6.2).
/// Logs one line starting "xhci <name>: ", as every line of this driver
/// does (spec §13: the NUC is debugged from a photo of `dmesg`).
macro_rules! xlog {
    ($hal:expr, $name:expr, $($arg:tt)*) => {
        $crate::Hal::log($hal, format_args!("xhci {}: {}", $name, format_args!($($arg)*)))
    };
}

mod caps;
mod command;
mod configure;
mod context;
mod device;
mod init;
mod port;
mod regs;
mod ring;
mod start;
mod transfer;
mod trb;

use crate::descriptor::{Configuration, DeviceDescriptor};
use crate::{DmaBuf, Hal, Speed, UsbError};
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

/// An addressed device: what enumeration found (spec §6.2 steps 1-5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub slot: u8,
    pub port: u8,
    pub speed: Speed,
    pub descriptor: DeviceDescriptor,
    /// The first configuration, parsed.
    pub configuration: Configuration,
}

/// A root port whose state changed since the last `port_changes()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortChange {
    pub port: u8,
    pub connected: bool,
    /// A connect or disconnect happened (CSC was set): a device that is
    /// attached on this port is gone, even if something is connected now.
    pub reconnected: bool,
}

/// A control request in flight on EP0: its TRBs, and what their events
/// said so far.
#[derive(Debug)]
struct Control {
    setup: u64,
    data: Option<u64>,
    status: u64,
    /// Bytes of the data stage not transferred (a short packet).
    residual: u32,
    /// `Err` holds the completion code that ended it.
    result: Option<Result<(), u8>>,
}

/// Where an endpoint's one transfer is.
#[derive(Debug)]
enum Transfer {
    Idle,
    /// A Normal TRB at `trb` for `len` bytes is on the ring.
    Queued {
        trb: u64,
        len: usize,
    },
    /// Finished; `take_in` returns it once.
    Done(Result<usize, UsbError>),
}

/// A configured endpoint other than EP0.
#[derive(Debug)]
struct Endpoint {
    address: u8,
    dci: usize,
    ring: ProducerRing,
    /// IN endpoints: the 4 KiB buffer their transfers land in.
    buffer: Option<DmaBuf>,
    transfer: Transfer,
}

impl Endpoint {
    fn free<H: Hal>(self, hal: &H) {
        self.ring.free(hal);
        if let Some(buf) = self.buffer {
            hal.free_dma(buf);
        }
    }
}

/// A device slot: its contexts, EP0 and the buffer control transfers use.
#[derive(Debug)]
struct Slot {
    port: u8,
    speed: Speed,
    /// The device (output) context the controller writes; DCBAA[slot].
    output: DmaBuf,
    /// The input context of this slot's commands.
    input: DmaBuf,
    ep0: ProducerRing,
    /// Control data stages go through this 4 KiB buffer.
    data: DmaBuf,
    /// EP0's max packet size as its context has it.
    max_packet0: u16,
    control: Option<Control>,
    /// What `configure` set up.
    endpoints: Vec<Endpoint>,
}

impl Slot {
    /// Allocates everything a slot needs, or nothing.
    fn new<H: Hal>(hal: &H, port: u8, speed: Speed, stride: usize) -> Result<Slot, UsbError> {
        let output = hal.alloc_dma(context::device_size(stride), 64);
        let input = hal.alloc_dma(context::input_size(stride), 64);
        let ep0 = ProducerRing::new(hal).ok();
        let data = hal.alloc_dma(transfer::DATA_BUFFER_SIZE, 64);
        match (output, input, ep0, data) {
            (Some(output), Some(input), Some(ep0), Some(data)) => Ok(Slot {
                port,
                speed,
                output,
                input,
                ep0,
                data,
                max_packet0: speed.default_max_packet0(),
                control: None,
                endpoints: Vec::new(),
            }),
            (output, input, ep0, data) => {
                for buf in [output, input, data].into_iter().flatten() {
                    hal.free_dma(buf);
                }
                if let Some(ring) = ep0 {
                    ring.free(hal);
                }
                Err(UsbError::NoMemory)
            }
        }
    }

    fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.output);
        hal.free_dma(self.input);
        hal.free_dma(self.data);
        self.ep0.free(hal);
        for ep in self.endpoints {
            ep.free(hal);
        }
    }

    fn endpoint(&mut self, dci: usize) -> Option<&mut Endpoint> {
        self.endpoints.iter_mut().find(|e| e.dci == dci)
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
    // The controller uses these pages; the driver only owns them.
    #[cfg_attr(not(test), expect(dead_code))]
    scratchpads: Option<Scratchpads>,
    commands: ProducerRing,
    events: EventRing,
    /// The command in flight (one at a time).
    pending: Option<Pending>,
    /// A Command Ring Stopped event came since the last abort.
    ring_stopped: bool,
    /// Ports a Port Status Change Event named since `port_changes` looked.
    port_flags: Vec<bool>,
    /// `port_changes` has not run yet: every port counts.
    first_scan: bool,
    /// Index = slot ID (0 unused).
    slots: Vec<Option<Slot>>,
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
