//! What class drivers see of a host controller: control requests, IN
//! transfers that complete later, bulk transfers that are waited for, and
//! halt recovery. The xHCI driver implements [`Bus`]; the class drivers'
//! tests use a fake.

use crate::UsbError;
use core::fmt;
use core::time::Duration;

/// The largest bulk transfer (spec §6.4: storage commands move at most
/// 64 KiB).
pub const MAX_BULK: usize = 64 * 1024;

/// A device's connection speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
    SuperPlus,
}

impl Speed {
    /// Endpoint 0's packet size until the device descriptor says otherwise
    /// (USB 2.0 §5.5.3, USB 3.2 §9.6.1).
    pub fn default_max_packet0(self) -> u16 {
        match self {
            Speed::Low | Speed::Full => 8,
            Speed::High => 64,
            Speed::Super | Speed::SuperPlus => 512,
        }
    }

    /// SuperSpeed and faster, which use USB 3 ports and descriptors.
    pub fn is_superspeed(self) -> bool {
        matches!(self, Speed::Super | Speed::SuperPlus)
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Speed::Low => "low-speed",
            Speed::Full => "full-speed",
            Speed::High => "high-speed",
            Speed::Super => "SuperSpeed",
            Speed::SuperPlus => "SuperSpeedPlus",
        })
    }
}

/// Standard request codes (USB 2.0 table 9-4).
pub const CLEAR_FEATURE: u8 = 1;
pub const GET_DESCRIPTOR: u8 = 6;
pub const SET_CONFIGURATION: u8 = 9;

/// `bmRequestType` bits.
pub const DIR_IN: u8 = 0x80;
pub const TYPE_CLASS: u8 = 0x20;
pub const RECIPIENT_INTERFACE: u8 = 0x01;
pub const RECIPIENT_ENDPOINT: u8 = 0x02;

/// The feature selector of `CLEAR_FEATURE(ENDPOINT_HALT)`.
pub const ENDPOINT_HALT: u16 = 0;

/// The 8-byte setup packet of a control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setup {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl Setup {
    /// Whether the data stage goes from the device to the host.
    pub fn is_in(&self) -> bool {
        self.request_type & DIR_IN != 0
    }

    /// The packet as it goes on the wire (little-endian fields).
    pub fn to_bytes(&self) -> [u8; 8] {
        let [v0, v1] = self.value.to_le_bytes();
        let [i0, i1] = self.index.to_le_bytes();
        let [l0, l1] = self.length.to_le_bytes();
        [self.request_type, self.request, v0, v1, i0, i1, l0, l1]
    }

    /// `GET_DESCRIPTOR` of descriptor type `kind`, number `index`.
    pub fn get_descriptor(kind: u8, index: u8, length: u16) -> Setup {
        Setup {
            request_type: DIR_IN,
            request: GET_DESCRIPTOR,
            value: (kind as u16) << 8 | index as u16,
            index: 0,
            length,
        }
    }

    pub fn set_configuration(value: u8) -> Setup {
        Setup {
            request_type: 0,
            request: SET_CONFIGURATION,
            value: value as u16,
            index: 0,
            length: 0,
        }
    }

    /// `CLEAR_FEATURE(ENDPOINT_HALT)` for endpoint address `endpoint`.
    pub fn clear_halt(endpoint: u8) -> Setup {
        Setup {
            request_type: RECIPIENT_ENDPOINT,
            request: CLEAR_FEATURE,
            value: ENDPOINT_HALT,
            index: endpoint as u16,
            length: 0,
        }
    }
}

/// A host controller as class drivers use it. Devices are named by their
/// slot number on that controller; endpoints by their address (bit 7 set
/// for IN).
pub trait Bus {
    /// A control transfer on endpoint 0: `setup`, then `data` (received for
    /// an IN request, sent for an OUT one; `setup.length` bytes at most),
    /// then the status stage. Waits for it (1 s at most) and returns the
    /// number of data bytes moved. A STALL is `UsbError::Stall`, and the
    /// endpoint is usable again afterwards.
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError>;
    /// Starts an IN transfer of up to `len` bytes on `endpoint` and returns
    /// at once. At most one is outstanding per endpoint.
    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError>;
    /// The outcome of the IN transfer on `endpoint` once it has finished:
    /// the bytes received are copied into `buf`. `None` while it is still
    /// running (or none was queued), and `ControllerDead` on every call
    /// once the controller stopped working. Never waits.
    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>>;
    /// A bulk IN transfer on `endpoint` of up to `buf.len()` bytes
    /// ([`MAX_BULK`] at most): waits for it (5 s at most) and returns the
    /// number of bytes received into `buf`. A STALL is `UsbError::Stall`,
    /// and the endpoint stays halted until [`Bus::clear_halt`]. A transfer
    /// that times out is aborted before this returns; if the abort fails,
    /// the endpoint takes no transfer until `clear_halt` succeeds.
    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError>;
    /// A bulk OUT transfer of `data` on `endpoint`, as [`Bus::bulk_in`]:
    /// returns the number of bytes sent.
    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError>;
    /// Makes a halted `endpoint` usable again: resets it on the controller
    /// and sends `CLEAR_FEATURE(ENDPOINT_HALT)` to the device.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError>;
    /// Monotonic time since boot.
    fn now(&self) -> Duration;
    /// Waits at least `d` (a device that asks for time, such as a disk
    /// spinning up).
    fn sleep(&self, d: Duration);
    /// Adds one line to the kernel log.
    fn log(&self, args: fmt::Arguments);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn setup_packets_are_little_endian() {
        let s = Setup::get_descriptor(2, 0, 0x0109);
        assert!(s.is_in());
        assert_eq!(s.to_bytes(), [0x80, 6, 0, 2, 0, 0, 0x09, 0x01]);
        assert_eq!(
            Setup::set_configuration(1).to_bytes(),
            [0, 9, 1, 0, 0, 0, 0, 0]
        );
        let c = Setup::clear_halt(0x81);
        assert!(!c.is_in());
        assert_eq!(c.to_bytes(), [2, 1, 0, 0, 0x81, 0, 0, 0]);
    }

    #[test]
    fn default_packet_sizes_follow_the_speed() {
        assert_eq!(Speed::Low.default_max_packet0(), 8);
        assert_eq!(Speed::Full.default_max_packet0(), 8);
        assert_eq!(Speed::High.default_max_packet0(), 64);
        assert_eq!(Speed::Super.default_max_packet0(), 512);
        assert!(Speed::SuperPlus.is_superspeed() && !Speed::High.is_superspeed());
        assert_eq!(Speed::Low.to_string(), "low-speed");
        assert_eq!(Speed::Super.to_string(), "SuperSpeed");
    }
}
