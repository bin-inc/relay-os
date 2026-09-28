//! The Relay USB stack (spec §6): an xHCI host controller driver and the
//! HID boot-keyboard class driver.
//!
//! The crate reaches hardware only through [`Hal`], which the kernel
//! implements over its page tables, frame allocator and timer, and the tests
//! over a fake register file and heap memory. So every sequence, including
//! the ones QEMU never exercises (BIOS handoff, scratchpad buffers, 64-byte
//! contexts, SuperSpeed ports), is tested on the host.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod bus;
pub mod descriptor;
mod error;
mod hal;
pub mod hid;
pub mod host;
#[cfg(test)]
mod testing;
pub mod xhci;

pub use bus::{Bus, Setup, Speed};
pub use error::UsbError;
pub use hal::{DmaBuf, Hal};
