//! The runtime every Relay OS program links (spec §8.1 of the user-space
//! gate): the entry point, the arguments and the environment, system-call
//! wrappers, the heap (`alloc` works in every program), the panic handler,
//! the ELF note that names the ABI, and the shell's `Vfs`, `Console`,
//! `System`, `Stdin`, `Stdout` and `Programs` over system calls, so a
//! command function runs unchanged in a program.
//!
//! A program is a `#![no_std]`, `#![no_main]` binary that names its main
//! function with [`main!`]:
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! relay_rt::main!(main);
//! fn main(args: relay_rt::Args) -> u8 { 0 }
//! ```
//!
//! The architecture-specific parts (the system-call instruction and
//! `_start`) are in `arch`, built only for Relay OS; the rest is host-tested.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod allocator;
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
mod arch;
mod args;
pub mod env;
// Off Relay OS only the tests use these.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod note;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod start;
pub mod sys;
pub mod sysio;
pub mod sysvfs;
mod testing;

pub use args::Args;
pub use start::name;
pub use sysio::{SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem};
pub use sysvfs::SysVfs;

/// Names the program's `fn main(args: Args) -> u8`; its result is the exit
/// status.
#[macro_export]
macro_rules! main {
    ($main:path) => {
        #[unsafe(no_mangle)]
        extern "Rust" fn __relay_main(args: $crate::Args) -> u8 {
            $main(args)
        }
    };
}
