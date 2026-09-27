//! Standard descriptors (USB 2.0 chapter 9, USB 3.2 §9.6): the device
//! descriptor and the configuration descriptor with its interfaces and
//! endpoints. Devices are not trusted: a malformed descriptor is an error,
//! never a panic or a loop.

use crate::{Speed, UsbError};
use alloc::vec::Vec;

pub const DEVICE: u8 = 1;
pub const CONFIGURATION: u8 = 2;
pub const INTERFACE: u8 = 4;
pub const ENDPOINT: u8 = 5;
pub const SS_ENDPOINT_COMPANION: u8 = 0x30;

/// Length of the device descriptor.
pub const DEVICE_LEN: usize = 18;
/// Length of the configuration descriptor's own header.
pub const CONFIGURATION_LEN: usize = 9;

fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    /// `bcdUSB`, for example 0x0200.
    pub usb_version: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    /// `bMaxPacketSize0` as sent: bytes, or an exponent for SuperSpeed.
    pub max_packet0: u8,
    pub vendor: u16,
    pub product: u16,
    pub device_version: u16,
    pub configurations: u8,
}

impl DeviceDescriptor {
    pub fn parse(b: &[u8]) -> Result<DeviceDescriptor, UsbError> {
        if b.len() < DEVICE_LEN || (b[0] as usize) < DEVICE_LEN {
            return Err(UsbError::BadDescriptor("device descriptor too short"));
        }
        if b[1] != DEVICE {
            return Err(UsbError::BadDescriptor("not a device descriptor"));
        }
        Ok(DeviceDescriptor {
            usb_version: le16(b, 2),
            class: b[4],
            subclass: b[5],
            protocol: b[6],
            max_packet0: b[7],
            vendor: le16(b, 8),
            product: le16(b, 10),
            device_version: le16(b, 12),
            configurations: b[17],
        })
    }
}

/// Endpoint 0's packet size from the first 8 bytes of the device
/// descriptor (all a full-speed device is asked for before its packet size
/// is known). SuperSpeed devices send an exponent (9 means 512).
pub fn max_packet0(prefix: &[u8], speed: Speed) -> Result<u16, UsbError> {
    if prefix.len() < 8 || prefix[1] != DEVICE {
        return Err(UsbError::BadDescriptor("not a device descriptor"));
    }
    let raw = prefix[7];
    let size = if speed.is_superspeed() {
        if raw > 15 {
            return Err(UsbError::BadDescriptor("endpoint 0 packet size"));
        }
        1u16 << raw
    } else {
        raw as u16
    };
    match (speed, size) {
        (Speed::Low, 8) | (Speed::Full, 8 | 16 | 32 | 64) | (Speed::High, 64) => Ok(size),
        (Speed::Super | Speed::SuperPlus, 512) => Ok(size),
        _ => Err(UsbError::BadDescriptor("endpoint 0 packet size")),
    }
}

/// `wTotalLength` from the configuration descriptor's 9-byte header.
pub fn configuration_length(header: &[u8]) -> Result<u16, UsbError> {
    if header.len() < CONFIGURATION_LEN || header[1] != CONFIGURATION {
        return Err(UsbError::BadDescriptor("not a configuration descriptor"));
    }
    let total = le16(header, 2);
    if (total as usize) < CONFIGURATION_LEN {
        return Err(UsbError::BadDescriptor("configuration too short"));
    }
    Ok(total)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointKind {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// `bEndpointAddress`: the number in bits 0-3, bit 7 set for IN.
    pub address: u8,
    pub kind: EndpointKind,
    /// `wMaxPacketSize` as sent: the size in bits 0-10, extra transactions
    /// per microframe in bits 11-12 (high-speed periodic endpoints).
    pub max_packet: u16,
    /// `bInterval` as sent (its unit depends on speed and kind).
    pub interval: u8,
    /// `bMaxBurst` from a SuperSpeed endpoint companion, else 0.
    pub max_burst: u8,
}

impl Endpoint {
    pub fn is_in(&self) -> bool {
        self.address & 0x80 != 0
    }

    pub fn number(&self) -> u8 {
        self.address & 0x0F
    }

    /// Bytes per packet.
    pub fn packet_size(&self) -> u16 {
        self.max_packet & 0x7FF
    }

    /// Additional transactions per microframe (high-speed periodic).
    pub fn extra_transactions(&self) -> u8 {
        (self.max_packet >> 11) as u8 & 3
    }
}

/// An interface in its default setting (alternate setting 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub number: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// `bConfigurationValue`, for `SET_CONFIGURATION`.
    pub value: u8,
    pub interfaces: Vec<Interface>,
}

/// Parses a whole configuration descriptor (`wTotalLength` bytes).
/// Alternate settings other than 0 are skipped with their endpoints, as are
/// class-specific descriptors (HID's among them) and endpoint 0 entries.
pub fn parse_configuration(b: &[u8]) -> Result<Configuration, UsbError> {
    let total = configuration_length(b)? as usize;
    if (b[0] as usize) < CONFIGURATION_LEN {
        return Err(UsbError::BadDescriptor("configuration too short"));
    }
    let b = &b[..total.min(b.len())];
    let mut config = Configuration {
        value: b[5],
        interfaces: Vec::new(),
    };
    // Where endpoints go: the interface being parsed, or none while an
    // alternate setting or a repeated interface number is skipped.
    let mut current: Option<usize> = None;
    let mut pos = b[0] as usize;
    while pos < b.len() {
        let rest = &b[pos..];
        if rest.len() < 2 || rest[0] < 2 {
            return Err(UsbError::BadDescriptor("zero-length descriptor"));
        }
        let len = rest[0] as usize;
        if len > rest.len() {
            return Err(UsbError::BadDescriptor("descriptor runs past the end"));
        }
        let d = &rest[..len];
        match d[1] {
            INTERFACE => {
                if len < 9 {
                    return Err(UsbError::BadDescriptor("interface descriptor too short"));
                }
                let seen = config.interfaces.iter().any(|i| i.number == d[2]);
                current = if d[3] == 0 && !seen {
                    config.interfaces.push(Interface {
                        number: d[2],
                        class: d[5],
                        subclass: d[6],
                        protocol: d[7],
                        endpoints: Vec::new(),
                    });
                    Some(config.interfaces.len() - 1)
                } else {
                    None
                };
            }
            ENDPOINT => {
                if len < 7 {
                    return Err(UsbError::BadDescriptor("endpoint descriptor too short"));
                }
                let kind = match d[3] & 3 {
                    0 => EndpointKind::Control,
                    1 => EndpointKind::Isochronous,
                    2 => EndpointKind::Bulk,
                    _ => EndpointKind::Interrupt,
                };
                if let Some(i) = current
                    && d[2] & 0x0F != 0
                {
                    config.interfaces[i].endpoints.push(Endpoint {
                        address: d[2] & 0x8F,
                        kind,
                        max_packet: le16(d, 4),
                        interval: d[6],
                        max_burst: 0,
                    });
                }
            }
            SS_ENDPOINT_COMPANION if len >= 6 => {
                if let Some(ep) = current.and_then(|i| config.interfaces[i].endpoints.last_mut()) {
                    ep.max_burst = d[2];
                }
            }
            _ => {}
        }
        pos += len;
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// The Logitech K120's configuration: a boot keyboard and a second HID
    /// interface for its extra keys, each with a HID descriptor.
    pub const K120: [u8; 59] = [
        9, 2, 59, 0, 2, 1, 0, 0xA0, 50, // configuration 1, 2 interfaces
        9, 4, 0, 0, 1, 3, 1, 1, 0, // interface 0: HID, boot, keyboard
        9, 0x21, 0x10, 1, 0, 1, 0x22, 65, 0, // HID descriptor
        7, 5, 0x81, 3, 8, 0, 10, // endpoint 1 IN, interrupt, 8 bytes, 10 ms
        9, 4, 1, 0, 1, 3, 0, 0, 0, // interface 1: HID, no boot protocol
        9, 0x21, 0x10, 1, 0, 1, 0x22, 51, 0, // HID descriptor
        7, 5, 0x82, 3, 4, 0, 0xFF, // endpoint 2 IN, interrupt, 4 bytes
    ];

    fn ep(address: u8, kind: EndpointKind, max_packet: u16, interval: u8) -> Endpoint {
        Endpoint {
            address,
            kind,
            max_packet,
            interval,
            max_burst: 0,
        }
    }

    #[test]
    fn a_device_descriptor_parses() {
        let d = [
            18, 1, 0x10, 1, 0, 0, 0, 8, 0x6D, 0x04, 0x1C, 0xC3, 0x10, 0x01, 1, 2, 0, 1,
        ];
        let dev = DeviceDescriptor::parse(&d).unwrap();
        assert_eq!((dev.vendor, dev.product), (0x046D, 0xC31C));
        assert_eq!(dev.usb_version, 0x0110);
        assert_eq!(dev.max_packet0, 8);
        assert_eq!(dev.configurations, 1);
        assert!(DeviceDescriptor::parse(&d[..17]).is_err());
        let mut wrong = d;
        wrong[1] = 2;
        assert!(DeviceDescriptor::parse(&wrong).is_err());
        let mut short = d;
        short[0] = 8;
        assert!(DeviceDescriptor::parse(&short).is_err());
    }

    #[test]
    fn endpoint_zero_packet_sizes_are_checked_against_the_speed() {
        let prefix = |mps| [18, 1, 0, 2, 0, 0, 0, mps];
        assert_eq!(max_packet0(&prefix(8), Speed::Low), Ok(8));
        assert_eq!(max_packet0(&prefix(64), Speed::Full), Ok(64));
        assert_eq!(max_packet0(&prefix(64), Speed::High), Ok(64));
        assert_eq!(max_packet0(&prefix(9), Speed::Super), Ok(512));
        assert!(max_packet0(&prefix(64), Speed::Low).is_err());
        assert!(max_packet0(&prefix(0), Speed::Full).is_err());
        assert!(max_packet0(&prefix(12), Speed::Full).is_err());
        assert!(max_packet0(&prefix(200), Speed::Super).is_err());
        assert!(max_packet0(&prefix(8)[..7], Speed::Full).is_err());
    }

    #[test]
    fn the_k120_configuration_parses() {
        assert_eq!(configuration_length(&K120[..9]), Ok(59));
        let c = parse_configuration(&K120).unwrap();
        assert_eq!(c.value, 1);
        assert_eq!(
            c.interfaces,
            vec![
                Interface {
                    number: 0,
                    class: 3,
                    subclass: 1,
                    protocol: 1,
                    endpoints: vec![ep(0x81, EndpointKind::Interrupt, 8, 10)],
                },
                Interface {
                    number: 1,
                    class: 3,
                    subclass: 0,
                    protocol: 0,
                    endpoints: vec![ep(0x82, EndpointKind::Interrupt, 4, 0xFF)],
                },
            ]
        );
        let e = c.interfaces[0].endpoints[0];
        assert!(e.is_in());
        assert_eq!(
            (e.number(), e.packet_size(), e.extra_transactions()),
            (1, 8, 0)
        );
    }

    #[test]
    fn alternate_settings_and_repeated_interfaces_are_skipped() {
        let d = [
            9, 2, 50, 0, 1, 1, 0, 0x80, 50, //
            9, 4, 0, 0, 1, 8, 6, 0x50, 0, // interface 0 alt 0: mass storage
            7, 5, 0x81, 2, 0, 2, 0, // bulk IN 512
            9, 4, 0, 1, 1, 8, 6, 0x62, 0, // interface 0 alt 1 (UAS)
            7, 5, 0x83, 2, 0, 2, 0, // its endpoint
            9, 4, 0, 0, 0, 3, 1, 1, 0, // interface 0 again: ignored
        ];
        let c = parse_configuration(&d).unwrap();
        assert_eq!(c.interfaces.len(), 1);
        assert_eq!(c.interfaces[0].protocol, 0x50);
        assert_eq!(
            c.interfaces[0].endpoints,
            vec![ep(0x81, EndpointKind::Bulk, 512, 0)]
        );
    }

    #[test]
    fn superspeed_companions_give_the_burst() {
        let d = [
            9, 2, 44, 0, 1, 1, 0, 0x80, 50, //
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 4, 0, // bulk IN 1024
            6, 0x30, 15, 0, 0, 0, // companion: burst of 16
            7, 5, 0x02, 2, 0, 4, 0, // bulk OUT 1024
            6, 0x30, 3, 0, 0, 0, //
        ];
        let c = parse_configuration(&d).unwrap();
        let eps = &c.interfaces[0].endpoints;
        assert_eq!((eps[0].address, eps[0].max_burst), (0x81, 15));
        assert_eq!((eps[1].address, eps[1].max_burst), (0x02, 3));
        assert!(!eps[1].is_in());
    }

    #[test]
    fn malformed_configurations_are_errors() {
        // A zero-length descriptor would loop forever if it were believed.
        let mut zero = K120;
        zero[18] = 0;
        assert!(parse_configuration(&zero).is_err());
        // An endpoint descriptor that runs past wTotalLength.
        let mut past = K120;
        past[52] = 9;
        assert!(parse_configuration(&past).is_err());
        let mut short = K120;
        short[9] = 5;
        assert!(parse_configuration(&short).is_err());
        assert!(configuration_length(&[9, 2, 5, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(configuration_length(&K120[..8]).is_err());
        assert!(parse_configuration(&[9, 1, 9, 0, 0, 1, 0, 0, 0]).is_err());
    }

    #[test]
    fn data_beyond_the_total_length_is_ignored_and_less_is_parsed() {
        let mut long = K120.to_vec();
        long.extend_from_slice(&[0, 0, 0]);
        assert_eq!(parse_configuration(&long).unwrap().interfaces.len(), 2);
        // A device that sends fewer bytes than it announced: what arrived
        // is parsed, as long as no descriptor is cut in half.
        assert_eq!(
            parse_configuration(&K120[..34]).unwrap().interfaces.len(),
            1
        );
        assert!(parse_configuration(&K120[..30]).is_err());
    }

    #[test]
    fn endpoint_zero_and_endpoints_outside_interfaces_are_ignored() {
        let d = [
            9, 2, 32, 0, 1, 1, 0, 0x80, 50, //
            7, 5, 0x81, 3, 8, 0, 10, // before any interface
            9, 4, 0, 0, 1, 3, 1, 1, 0, //
            7, 5, 0x80, 3, 8, 0, 10, // endpoint 0: invalid here
        ];
        let c = parse_configuration(&d).unwrap();
        assert!(c.interfaces[0].endpoints.is_empty());
    }
}
