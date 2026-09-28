//! Block devices below the filesystem (spec §6.5): the GPT of a disk, read
//! with CRC32 checks and the backup header as a fallback.

pub mod crc32;
pub mod gpt;

#[cfg(test)]
mod testing;
