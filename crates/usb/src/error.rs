//! USB errors. Device errors are values, not panics (spec §10).

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// A wait ran out of time: a register, a command or a transfer.
    Timeout,
    /// The device answered with a STALL handshake.
    Stall,
    /// A transfer ended with this xHCI completion code.
    Transfer(u8),
    /// A command ended with this xHCI completion code.
    Command(u8),
    /// No DMA memory left, or the registers could not be mapped.
    NoMemory,
    /// The controller's registers read as all ones: it is powered down or
    /// not there.
    NotResponding,
    /// The controller or device needs something this driver does not do.
    Unsupported(&'static str),
    /// A descriptor the device sent is malformed.
    BadDescriptor(&'static str),
    /// The device is no longer connected.
    Disconnected,
    /// The controller stopped working (a command timed out or it reported a
    /// host system error); nothing more is sent to it.
    ControllerDead,
}

impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UsbError::Timeout => write!(f, "timed out"),
            UsbError::Stall => write!(f, "stalled"),
            UsbError::Transfer(code) => write!(f, "transfer failed (completion code {code})"),
            UsbError::Command(code) => write!(f, "command failed (completion code {code})"),
            UsbError::NoMemory => write!(f, "out of memory"),
            UsbError::NotResponding => write!(f, "controller not responding"),
            UsbError::Unsupported(what) => write!(f, "unsupported: {what}"),
            UsbError::BadDescriptor(what) => write!(f, "bad descriptor: {what}"),
            UsbError::Disconnected => write!(f, "device disconnected"),
            UsbError::ControllerDead => write!(f, "controller stopped working"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn messages_read_well_in_a_status_line() {
        assert_eq!(UsbError::Timeout.to_string(), "timed out");
        assert_eq!(
            UsbError::Command(5).to_string(),
            "command failed (completion code 5)"
        );
        assert_eq!(
            UsbError::Unsupported("32-bit addressing").to_string(),
            "unsupported: 32-bit addressing"
        );
        assert_eq!(
            UsbError::BadDescriptor("zero length").to_string(),
            "bad descriptor: zero length"
        );
    }
}
