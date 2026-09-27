//! Fake USB devices: what the fake controller talks to on its ports.

use crate::bus::{GET_DESCRIPTOR, SET_CONFIGURATION};
use crate::descriptor::{CONFIGURATION, DEVICE};
use crate::{Setup, Speed};
use std::cell::RefCell;
use std::rc::Rc;

const SET_ADDRESS: u8 = 5;

/// A STALL handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stall;

/// A device as the fake controller sees it on a port.
pub trait FakeDevice {
    fn speed(&self) -> Speed;
    /// Endpoint 0's packet size: how the device splits a data stage.
    fn max_packet0(&self) -> u16;
    /// A control request with its OUT data; the IN data, or `None` if the
    /// device never answers it (NAKs until the host gives up).
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>>;
}

/// A control request as the device got it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub setup: Setup,
    pub data: Vec<u8>,
}

/// A configurable device with its descriptors, modelled on real ones. It
/// answers the standard requests itself and records every request.
pub struct FakeUsbDevice {
    speed: Speed,
    /// The device descriptor (18 bytes).
    device: Vec<u8>,
    /// The whole configuration descriptor.
    configuration: Vec<u8>,
    requests: Vec<Request>,
    /// Knob: requests (bRequest, wValue) that are stalled.
    stalls: Vec<(u8, u16)>,
    /// Knob: this many requests from now on are never answered.
    ignore: usize,
    /// Knob: at most this many bytes of the configuration are sent.
    configuration_limit: Option<usize>,
    address: u8,
    configuration_value: u8,
}

/// The Logitech K120's configuration (as in `descriptor.rs`'s tests): a
/// boot keyboard and a second HID interface for its extra keys.
pub const K120_CONFIGURATION: [u8; 59] = [
    9, 2, 59, 0, 2, 1, 0, 0xA0, 50, // configuration 1, 2 interfaces
    9, 4, 0, 0, 1, 3, 1, 1, 0, // interface 0: HID, boot, keyboard
    9, 0x21, 0x10, 1, 0, 1, 0x22, 65, 0, // HID descriptor
    7, 5, 0x81, 3, 8, 0, 10, // endpoint 1 IN, interrupt, 8 bytes, 10 ms
    9, 4, 1, 0, 1, 3, 0, 0, 0, // interface 1: HID, no boot protocol
    9, 0x21, 0x10, 1, 0, 1, 0x22, 51, 0, // HID descriptor
    7, 5, 0x82, 3, 4, 0, 0xFF, // endpoint 2 IN, interrupt, 4 bytes
];

/// A device descriptor: `bcdUSB`, EP0's size, vendor and product.
fn device_descriptor(usb: u16, max_packet0: u8, vendor: u16, product: u16) -> Vec<u8> {
    let [u0, u1] = usb.to_le_bytes();
    let [v0, v1] = vendor.to_le_bytes();
    let [p0, p1] = product.to_le_bytes();
    vec![
        18,
        1,
        u0,
        u1,
        0,
        0,
        0,
        max_packet0,
        v0,
        v1,
        p0,
        p1,
        0,
        1,
        1,
        2,
        3,
        1,
    ]
}

/// A configuration descriptor around `body` (interfaces and endpoints).
fn configuration(interfaces: u8, body: &[u8]) -> Vec<u8> {
    let total = (9 + body.len()) as u16;
    let [t0, t1] = total.to_le_bytes();
    let mut c = vec![9, 2, t0, t1, interfaces, 1, 0, 0x80, 50];
    c.extend_from_slice(body);
    c
}

impl FakeUsbDevice {
    pub fn new(
        speed: Speed,
        device: Vec<u8>,
        configuration: Vec<u8>,
    ) -> Rc<RefCell<FakeUsbDevice>> {
        Rc::new(RefCell::new(FakeUsbDevice {
            speed,
            device,
            configuration,
            requests: Vec::new(),
            stalls: Vec::new(),
            ignore: 0,
            configuration_limit: None,
            address: 0,
            configuration_value: 0,
        }))
    }

    /// The NUC's keyboard: Logitech K120, low-speed, `046d:c31c`.
    pub fn k120() -> Rc<RefCell<FakeUsbDevice>> {
        FakeUsbDevice::new(
            Speed::Low,
            device_descriptor(0x0110, 8, 0x046D, 0xC31C),
            K120_CONFIGURATION.to_vec(),
        )
    }

    /// Logitech Unifying receiver, full-speed, `046d:c534`, EP0 64 bytes,
    /// three HID interfaces: keyboard, mouse and a vendor one.
    pub fn unifying_receiver() -> Rc<RefCell<FakeUsbDevice>> {
        let hid = |n: u8, sub: u8, proto: u8, ep: u8, size: u8| {
            let mut d = vec![9, 4, n, 0, 1, 3, sub, proto, 0];
            d.extend_from_slice(&[9, 0x21, 0x11, 1, 0, 1, 0x22, 59, 0]);
            d.extend_from_slice(&[7, 5, ep, 3, size, 0, 8]);
            d
        };
        let body = [
            hid(0, 1, 1, 0x81, 8),
            hid(1, 1, 2, 0x82, 8),
            hid(2, 0, 0, 0x83, 32),
        ]
        .concat();
        FakeUsbDevice::new(
            Speed::Full,
            device_descriptor(0x0200, 64, 0x046D, 0xC534),
            configuration(3, &body),
        )
    }

    /// QEMU 8.2's usb-kbd on an xHCI: high-speed, `0627:0001`, USB 2.00,
    /// EP0 64 bytes, one boot keyboard with interrupt IN 0x81 of 8 bytes
    /// and bInterval 7 (2^6 microframes: 8 ms).
    pub fn qemu_keyboard() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 1, 3, 1, 1, 0, //
            9, 0x21, 0x11, 1, 0, 1, 0x22, 63, 0, //
            7, 5, 0x81, 3, 8, 0, 7,
        ];
        FakeUsbDevice::new(
            Speed::High,
            device_descriptor(0x0200, 64, 0x0627, 0x0001),
            configuration(1, &body),
        )
    }

    /// The NUC's stick: Kingston DataTraveler 3.0, SuperSpeed,
    /// `0951:1666`, mass storage (8/6/0x50), bulk IN and OUT of 1024 bytes
    /// with a burst of 4 (companion bMaxBurst 3).
    pub fn kingston_stick() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 4, 0, //
            6, 0x30, 3, 0, 0, 0, //
            7, 5, 0x02, 2, 0, 4, 0, //
            6, 0x30, 3, 0, 0, 0,
        ];
        FakeUsbDevice::new(
            Speed::Super,
            device_descriptor(0x0320, 9, 0x0951, 0x1666),
            configuration(1, &body),
        )
    }

    /// A USB 2 stick: high-speed mass storage with bulk endpoints of 512.
    pub fn usb2_stick() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 2, 0, //
            7, 5, 0x02, 2, 0, 2, 0,
        ];
        FakeUsbDevice::new(
            Speed::High,
            device_descriptor(0x0200, 64, 0x0951, 0x1665),
            configuration(1, &body),
        )
    }

    /// A device of `speed` with the K120's descriptors otherwise.
    pub fn with_speed(speed: Speed) -> Rc<RefCell<FakeUsbDevice>> {
        let dev = FakeUsbDevice::k120();
        dev.borrow_mut().speed = speed;
        let mps = speed.default_max_packet0();
        dev.borrow_mut().device[7] = if speed.is_superspeed() { 9 } else { mps as u8 };
        dev
    }

    pub fn device_descriptor(&self) -> &[u8] {
        &self.device
    }

    pub fn configuration_descriptor(&self) -> &[u8] {
        &self.configuration
    }

    /// Replaces the descriptors (a device that sends a wrong one).
    pub fn set_descriptors(&mut self, device: Vec<u8>, configuration: Vec<u8>) {
        self.device = device;
        self.configuration = configuration;
    }

    /// Every request so far, SET_ADDRESS from the controller included.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.clone()
    }

    /// Stalls every `request` with `value` from now on.
    pub fn stall_request(&mut self, request: u8, value: u16) {
        self.stalls.push((request, value));
    }

    /// Never answers the next `n` requests of the driver (SET_ADDRESS,
    /// which the controller sends, is always answered).
    pub fn ignore_requests(&mut self, n: usize) {
        self.ignore = n;
    }

    /// Sends at most `len` bytes of the configuration descriptor.
    pub fn truncate_configuration(&mut self, len: usize) {
        self.configuration_limit = Some(len);
    }

    /// The address SET_ADDRESS gave it.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The value SET_CONFIGURATION gave it.
    pub fn configuration_value(&self) -> u8 {
        self.configuration_value
    }

    fn descriptor(&self, setup: &Setup) -> Result<Vec<u8>, Stall> {
        let mut d = match ((setup.value >> 8) as u8, setup.value as u8) {
            (DEVICE, 0) => self.device.clone(),
            (CONFIGURATION, 0) => {
                let limit = self.configuration_limit.unwrap_or(usize::MAX);
                self.configuration[..self.configuration.len().min(limit)].to_vec()
            }
            _ => return Err(Stall),
        };
        d.truncate(setup.length as usize);
        Ok(d)
    }
}

impl FakeDevice for FakeUsbDevice {
    fn speed(&self) -> Speed {
        self.speed
    }

    fn max_packet0(&self) -> u16 {
        let raw = self.device[7];
        if self.speed.is_superspeed() {
            1 << raw
        } else {
            raw as u16
        }
    }

    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>> {
        self.requests.push(Request {
            setup,
            data: data_out.to_vec(),
        });
        if self.ignore > 0 && setup.request != SET_ADDRESS {
            self.ignore -= 1;
            return None;
        }
        if self.stalls.contains(&(setup.request, setup.value)) {
            return Some(Err(Stall));
        }
        Some(match (setup.request_type, setup.request) {
            (0x80, GET_DESCRIPTOR) => self.descriptor(&setup),
            (0x00, SET_ADDRESS) => {
                self.address = setup.value as u8;
                Ok(Vec::new())
            }
            (0x00, SET_CONFIGURATION) => {
                self.configuration_value = setup.value as u8;
                Ok(Vec::new())
            }
            _ if setup.is_in() => Ok(vec![0; setup.length as usize]),
            _ => Ok(Vec::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{DeviceDescriptor, parse_configuration};

    #[test]
    fn the_presets_have_valid_descriptors() {
        for (dev, vendor, product, interfaces) in [
            (FakeUsbDevice::k120(), 0x046D, 0xC31C, 2),
            (FakeUsbDevice::unifying_receiver(), 0x046D, 0xC534, 3),
            (FakeUsbDevice::qemu_keyboard(), 0x0627, 0x0001, 1),
            (FakeUsbDevice::kingston_stick(), 0x0951, 0x1666, 1),
            (FakeUsbDevice::usb2_stick(), 0x0951, 0x1665, 1),
        ] {
            let d = dev.borrow();
            let desc = DeviceDescriptor::parse(d.device_descriptor()).unwrap();
            assert_eq!((desc.vendor, desc.product), (vendor, product));
            let c = parse_configuration(d.configuration_descriptor()).unwrap();
            assert_eq!(c.interfaces.len(), interfaces);
        }
        let stick = FakeUsbDevice::kingston_stick();
        let c = parse_configuration(stick.borrow().configuration_descriptor()).unwrap();
        assert_eq!(c.interfaces[0].endpoints[0].max_burst, 3);
        assert_eq!(stick.borrow().max_packet0(), 512);
        assert_eq!(
            FakeUsbDevice::unifying_receiver().borrow().max_packet0(),
            64
        );
    }

    #[test]
    fn standard_requests_are_answered_and_recorded() {
        let dev = FakeUsbDevice::k120();
        let mut d = dev.borrow_mut();
        let got = d.control(Setup::get_descriptor(DEVICE, 0, 8), &[]);
        assert_eq!(got.map(|r| r.map(|v| v.len())), Some(Ok(8)));
        d.truncate_configuration(20);
        let got = d.control(Setup::get_descriptor(CONFIGURATION, 0, 255), &[]);
        assert_eq!(got.map(|r| r.map(|v| v.len())), Some(Ok(20)));
        d.stall_request(GET_DESCRIPTOR, 0x0300);
        assert_eq!(
            d.control(Setup::get_descriptor(3, 0, 4), &[]),
            Some(Err(Stall))
        );
        d.ignore_requests(1);
        let set_address = Setup {
            request: SET_ADDRESS,
            ..Setup::set_configuration(3)
        };
        assert_eq!(d.control(set_address, &[]), Some(Ok(vec![])));
        assert_eq!(d.control(Setup::set_configuration(1), &[]), None);
        assert_eq!(
            d.control(Setup::set_configuration(1), &[]),
            Some(Ok(vec![]))
        );
        assert_eq!(d.configuration_value(), 1);
        assert_eq!(d.requests().len(), 6);
        assert_eq!(d.address(), 3);
    }
}
