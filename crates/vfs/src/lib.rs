//! The virtual filesystem layer: error numbers, the block-device trait, path
//! syntax, the filesystem trait, an in-memory filesystem and the mount table
//! the shell works through (spec §6.5, §8.1).
//!
//! Everything here is `no_std + alloc` and has no hardware access, so it is
//! tested on the host and used unchanged by the kernel.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod block;
mod errno;
mod fs;
mod memfs;
mod mount;
pub mod path;

pub use block::{BlockDevice, IoError, check_request};
pub use errno::Errno;
pub use fs::{DirEntry, Env, FileSystem, FileType, Ino, Stat, StatFs};
pub use memfs::{MAX_FILE_SIZE, MemFs};
pub use mount::{Cwd, MountTable, Node, Vfs};
