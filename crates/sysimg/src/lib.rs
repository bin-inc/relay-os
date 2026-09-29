//! The system archive, `system.img` (spec §4 of the user-space gate): the
//! programs of `/bin` in one file on the ESP, which xtask writes and the
//! kernel reads in place after the loader has put it in memory.
//!
//! Everything here is `no_std + alloc` and has no hardware access, so it is
//! tested on the host and used unchanged by the kernel.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod format;

pub use format::{
    Archive, DATA_ALIGN, ENTRY_LEN, Entry, FORMAT_VERSION, HEADER_LEN, MAGIC, MODE_MASK, NAME_MAX,
    SysImgError, valid_name, write,
};
