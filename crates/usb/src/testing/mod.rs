//! Test support: a fake `Hal` with virtual time and checked DMA memory.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod hal;

pub use hal::FakeHal;
