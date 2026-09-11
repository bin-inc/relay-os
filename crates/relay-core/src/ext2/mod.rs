mod on_disk;
mod validate;

use crate::block::{BlockDevice, BlockError};
use validate::Geometry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ext2Error {
    Block(BlockError),
    UnsupportedSector { sector_size: u32 },
    UnsupportedProfile { field: &'static str },
    UnsupportedFeature { field: &'static str, bits: u32 },
    MountRequiresCleanFilesystem,
    CorruptMetadata { field: &'static str },
    InvalidNode,
    WrongNodeKind,
    NotFound,
    UnsupportedFile,
    SparseFile,
    AllocationFailure,
}

#[allow(dead_code)] // Tasks 2 and 3 consume the mounted device and validated geometry.
pub struct Ext2<D> {
    device: D,
    mode: MountMode,
    geometry: Geometry,
}

impl<D: BlockDevice> Ext2<D> {
    pub fn mount(mut device: D, mode: MountMode) -> Result<Self, Ext2Error> {
        let geometry = validate::mount(&mut device, mode)?;
        Ok(Self {
            device,
            mode,
            geometry,
        })
    }
}
