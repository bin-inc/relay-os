//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! and the fake xHCI controller behind its registers.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod hal;
mod xhci;

pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use xhci::{ExtCap, FakeConfig};
