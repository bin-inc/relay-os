//! The rules a program's ELF file must follow before the kernel runs it
//! (spec §5.2 of the user-space gate): a static ELF64 executable with its
//! segments in the program area and the ABI's note. The kernel checks every
//! program `spawn` loads with [`check`]; xtask checks every program it
//! builds with the same function, so the build refuses exactly what the
//! kernel would.
//!
//! Everything here is `no_std + alloc` over a byte slice, host-tested with
//! randomized input: the file comes from a disk, so nothing in it may make
//! [`check`] panic or read outside it.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod check;

pub use check::{
    Access, EM_X86_64, ElfError, MAX_SIZE, PROGRAM_BASE, PROGRAM_END, Program, Segment, check,
};
