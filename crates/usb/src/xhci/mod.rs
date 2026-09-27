//! The xHCI host controller driver (spec §6.2).
// The driver is built bottom-up: parts land before their users.
#![allow(dead_code)]

mod context;
mod ring;
mod trb;
