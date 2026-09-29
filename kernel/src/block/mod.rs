//! Block devices below the filesystem (spec §6.5): the GPT of a disk, read
//! with CRC32 checks and the backup header as a fallback, its partitions as
//! block devices of their own, and the choice of the root partition.

pub mod gpt;
pub mod partition;
pub mod root;

#[cfg(test)]
pub(crate) mod testing;
