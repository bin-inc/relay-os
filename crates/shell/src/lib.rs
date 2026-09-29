//! The Relay shell (spec §7.3): line editor, parser and built-in commands.
//!
//! It reaches the rest of the system only through three traits: [`Vfs`]
//! for files, [`Console`] for the screen and keyboard, and [`System`] for
//! the clock, memory figures, the kernel log, reboot and power-off. So the
//! same code runs in the kernel, on the host (`cargo xtask host-shell`) and
//! in tests.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod commands;
mod ctx;
pub mod editor;
mod io;
pub mod killed;
pub mod parser;
mod shell;
mod testing;
mod time;
mod transcript;

pub use ctx::Ctx;
pub use io::{Console, MemInfo, System};
pub use shell::Shell;
pub use vfs::Vfs;
