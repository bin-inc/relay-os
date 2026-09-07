#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

mod boot;
mod guid;

pub use boot::{
    BOOT_ABI_VERSION, BOOT_INFO_MAGIC, BootInfo, FramebufferInfo, MemoryMap, MemoryRegion,
};
pub use guid::{GptGuid, GuidParseError};
