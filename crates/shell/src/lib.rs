//! The Relay shell (spec §7.3): line editor, parser, expansion, pipelines,
//! jobs and the function of every command.
//!
//! It reaches the rest of the system only through traits: [`Vfs`] for
//! files, [`Console`] for the screen and keyboard, [`System`] for the
//! clock, memory figures, the kernel log, reboot and power-off, and
//! `Stdin`, `Stdout` and `Programs` for a command's input, its output and
//! the programs it starts. So the same code runs in `/bin/sh` and every
//! program of `/bin` (through `relay-rt`), on the host (`cargo xtask
//! host-shell`) and in tests.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod commands;
mod corpus;
mod ctx;
pub mod editor;
mod expand;
mod io;
pub mod jobs;
pub mod killed;
pub mod parser;
mod pattern;
mod program;
mod reader;
mod runner;
mod scan;
mod shell;
mod testing;
pub mod time;
mod transcript;

pub use ctx::Ctx;
pub use io::{Bytes, Console, Group, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command};
pub use shell::Shell;
pub use vfs::Vfs;
