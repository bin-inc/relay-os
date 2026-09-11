#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod block;
pub mod console;
pub mod ext2;
pub mod fs;
pub mod gpt;
pub mod memory;
pub mod vfs;
