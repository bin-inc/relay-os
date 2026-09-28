//! The Relay USB stack (spec §6): an xHCI host controller driver, the HID
//! boot-keyboard class driver and the mass-storage class driver.
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
pub mod storage;
#[cfg(test)]
mod testing;
pub mod xhci;

pub use bus::{Bus, MAX_BULK, Setup, Speed};
pub use error::UsbError;
pub use hal::{DmaBuf, Hal};
