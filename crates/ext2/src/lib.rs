//! The ext2 filesystem driver (spec §8.2, §8.3): revision 1 filesystems with
//! 1, 2 or 4 KiB blocks, as `mke2fs -t ext2` makes them, behind the
//! [`vfs::FileSystem`] contract.
//!
//! Everything is `no_std + alloc` and reaches the disk only through a
//! [`vfs::BlockDevice`], so it is tested on the host against images made by
//! e2fsprogs and used unchanged by the kernel. Nothing on the disk is
//! trusted: corrupt metadata is logged and reported as `EIO`, never a panic.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod cache;

/// The block cache's default size (spec §8.3).
pub const CACHE_BYTES: usize = 8 << 20;
