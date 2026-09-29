//! The Relay system-call ABI (spec §7 of the user-space gate): the version,
//! the ELF note that marks a program built for it, the call numbers, the
//! error numbers, how a call's result carries a value or an error, and the
//! structs the calls pass.
//!
//! The kernel and the programs' runtime both build on this crate, so they
//! cannot disagree. It holds no architecture detail: which registers carry
//! the number, the arguments and the result is written down in spec §7.1
//! and implemented only in the `arch` modules.
#![cfg_attr(not(test), no_std)]

mod call;
pub mod errno;
mod result;
pub mod wait;

pub use call::Call;
pub use result::{MAX_ERRNO, decode, encode};
pub use wait::WaitStatus;

/// Changes whenever a call's meaning or a struct's layout changes; adding
/// a call does not change it (spec §7.4). Written into `system.img`'s
/// header and into every program's ELF note, and checked by the kernel.
pub const VERSION: u32 = 1;

/// The ELF note that names the ABI a program was built for (spec §5.2):
/// owner `Relay`, type [`NOTE_TYPE`], a 4-byte descriptor holding
/// [`VERSION`].
pub const NOTE_NAME: &[u8] = b"Relay";
pub const NOTE_TYPE: u32 = 1;
