//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! the fake xHCI controller behind its registers, and the fake devices on
//! its ports.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod bus;
mod device;
mod hal;
mod storage;
mod xhci;

pub use bus::TamperBus;
pub use device::FakeUsbDevice;
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use storage::{BadCsw, Event, FakeStorage, configured, op};
pub use xhci::{ExtCap, FakeCap, FakeConfig};

use crate::xhci::Xhci;

/// A fake machine with a controller made from `config`, and the driver
/// brought up on it as "00:14.0".
pub fn start(config: FakeConfig) -> (FakeHal, Xhci<FakeHal>) {
    let hal = FakeHal::with_controller(config);
    let xhci = Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0")
        .unwrap_or_else(|e| panic!("the controller did not start: {e}\n{}", hal.log_text()));
    (hal, xhci)
}
