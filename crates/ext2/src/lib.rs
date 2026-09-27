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
mod dir;
mod dirent;
mod file;
mod group;
mod inode;
mod le;
mod superblock;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use cache::BlockCache;
use core::fmt;
use group::Groups;
use superblock::{Geometry, STATE_ERROR, STATE_VALID, Superblock};
use vfs::{BlockDevice, DirEntry, Env, Errno, FileSystem, Ino, Stat, StatFs};

/// The block cache's default size (spec §8.3).
pub const CACHE_BYTES: usize = 8 << 20;

/// How to mount a filesystem.
#[derive(Clone, Copy, Debug)]
pub struct MountOptions {
    /// Never write to the device.
    pub read_only: bool,
    /// The block cache's size in bytes.
    pub cache_bytes: usize,
}

impl Default for MountOptions {
    fn default() -> MountOptions {
        MountOptions {
            read_only: false,
            cache_bytes: CACHE_BYTES,
        }
    }
}

/// A mounted ext2 filesystem on a block device.
pub struct Ext2<D: BlockDevice> {
    cache: BlockCache<D>,
    env: Box<dyn Env>,
    sb: Superblock,
    geo: Geometry,
    groups: Groups,
    read_only: bool,
}

/// Logs `ext2: <what>`.
fn log(env: &dyn Env, what: fmt::Arguments<'_>) {
    env.log(&format!("ext2: {what}"));
}

impl<D: BlockDevice> Ext2<D> {
    /// Checks the superblock and features (spec §8.2) and, read-write,
    /// marks the filesystem not clean. Every refusal is logged: `EINVAL`
    /// for a filesystem this driver cannot mount, `EIO` for a device error.
    pub fn mount(mut dev: D, env: Box<dyn Env>, opts: MountOptions) -> Result<Ext2<D>, Errno> {
        let env_ref = &*env;
        let refuse = |why: &str| {
            log(env_ref, format_args!("{why}"));
            Errno::EINVAL
        };
        let device_bytes = dev.block_count().saturating_mul(dev.block_size() as u64);
        if device_bytes < superblock::SUPERBLOCK_OFFSET + superblock::SUPERBLOCK_SIZE as u64 {
            return Err(refuse("device too small for a superblock"));
        }
        let sb = read_superblock(&mut dev)
            .inspect_err(|_| log(env_ref, format_args!("cannot read the superblock")))?;
        let geo = superblock::check(&sb, device_bytes).map_err(|why| refuse(&why))?;
        let dev_block = dev.block_size();
        let mut cache = BlockCache::new(dev, geo.block_size, opts.cache_bytes).map_err(|_| {
            refuse(&format!(
                "device block size {dev_block} does not divide the block size {}",
                geo.block_size
            ))
        })?;
        let groups = read_groups(&mut cache, &geo)
            .inspect_err(|_| log(env_ref, format_args!("cannot read the group descriptors")))?;
        group::check(&groups, &geo).map_err(|why| refuse(&why))?;
        // Only read-only mounting so far.
        let read_only = true;
        let unsupported = sb.unsupported_ro_compat();
        if unsupported != 0 {
            let names = superblock::ro_compat_names(unsupported);
            log(
                env_ref,
                format_args!("unsupported ro_compat features: {names}; mounting read-only"),
            );
        }
        if sb.state() & STATE_VALID == 0 {
            log(
                env_ref,
                format_args!("warning: not cleanly unmounted, run e2fsck"),
            );
        }
        if sb.state() & STATE_ERROR != 0 {
            log(
                env_ref,
                format_args!("warning: filesystem has errors, run e2fsck"),
            );
        }
        Ok(Ext2 {
            cache,
            env,
            sb,
            geo,
            groups,
            read_only,
        })
    }

    /// Whether changes are refused: mounted read-only, fallen back to
    /// read-only, or shut down.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Gives the device back (tests; after `shutdown` or `sync`, as
    /// unwritten changes are dropped).
    pub fn into_device(self) -> D {
        self.cache.into_inner()
    }

    /// Like Linux's ext2: sizes without the metadata, and space reserved
    /// for root is free but not available.
    fn fs_stat(&self) -> StatFs {
        let free = self.sb.free_blocks_count() as u64;
        StatFs {
            block_size: self.geo.block_size as u64,
            blocks: (self.geo.blocks_count as u64).saturating_sub(self.geo.overhead()),
            free_blocks: free,
            avail_blocks: free.saturating_sub(self.sb.r_blocks_count() as u64),
            files: self.geo.inodes_count as u64,
            free_files: self.sb.free_inodes_count() as u64,
        }
    }

    /// Logs corrupt metadata and gives the error to return for it.
    pub(crate) fn corrupt(&self, what: fmt::Arguments<'_>) -> Errno {
        log(&*self.env, what);
        Errno::EIO
    }
}

/// Reads the superblock straight from the device: the filesystem block
/// size is not known yet.
fn read_superblock<D: BlockDevice>(dev: &mut D) -> Result<Superblock, Errno> {
    let dev_block = dev.block_size() as u64;
    if dev_block == 0 {
        return Err(Errno::EINVAL);
    }
    let first = superblock::SUPERBLOCK_OFFSET / dev_block;
    let end =
        (superblock::SUPERBLOCK_OFFSET + superblock::SUPERBLOCK_SIZE as u64).div_ceil(dev_block);
    let mut buf = vec![0; ((end - first) * dev_block) as usize];
    dev.read(first, &mut buf)?;
    let at = (superblock::SUPERBLOCK_OFFSET - first * dev_block) as usize;
    Ok(Superblock::new(&buf[at..]))
}

fn read_groups<D: BlockDevice>(cache: &mut BlockCache<D>, geo: &Geometry) -> Result<Groups, Errno> {
    let mut raw = Vec::with_capacity(geo.gdt_blocks as usize * geo.block_size);
    for i in 0..geo.gdt_blocks as u64 {
        raw.extend_from_slice(cache.read(geo.first_data_block as u64 + 1 + i)?);
    }
    Ok(Groups::new(raw, geo.groups))
}

/// Only reading so far: every change is `EROFS`.
impl<D: BlockDevice> FileSystem for Ext2<D> {
    fn root(&self) -> Ino {
        superblock::ROOT_INO as Ino
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        self.inode_stat(ino)
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.dir_lookup(dir, name)
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        self.dir_list(dir)
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.link_target(ino)
    }

    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.file_read(ino, offset, buf)
    }

    fn write_at(&mut self, _: Ino, _: u64, _: &[u8]) -> Result<usize, Errno> {
        Err(Errno::EROFS)
    }

    fn truncate(&mut self, _: Ino, _: u64) -> Result<(), Errno> {
        Err(Errno::EROFS)
    }

    fn touch(&mut self, _: Ino) -> Result<(), Errno> {
        Err(Errno::EROFS)
    }

    fn create(&mut self, _: Ino, _: &[u8]) -> Result<Ino, Errno> {
        Err(Errno::EROFS)
    }

    fn mkdir(&mut self, _: Ino, _: &[u8]) -> Result<Ino, Errno> {
        Err(Errno::EROFS)
    }

    fn unlink(&mut self, _: Ino, _: &[u8]) -> Result<(), Errno> {
        Err(Errno::EROFS)
    }

    fn rmdir(&mut self, _: Ino, _: &[u8]) -> Result<(), Errno> {
        Err(Errno::EROFS)
    }

    fn rename(&mut self, _: Ino, _: &[u8], _: Ino, _: &[u8]) -> Result<(), Errno> {
        Err(Errno::EROFS)
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        Ok(self.fs_stat())
    }

    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}
