use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use relay_core::block::{BlockDevice, BlockError, BlockGeometry};

pub struct FileDevice {
    file: File,
    geometry: BlockGeometry,
    read_only: bool,
}

impl FileDevice {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Self::open_with(path, false)
    }

    pub fn open_read_only(path: &Path) -> std::io::Result<Self> {
        Self::open_with(path, true)
    }

    fn open_with(path: &Path, read_only: bool) -> std::io::Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        if !read_only {
            options.write(true);
        }
        let file = options.open(path)?;
        let bytes = file.metadata()?.len();
        if bytes % 512 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "device length is not sector aligned",
            ));
        }
        Ok(Self {
            file,
            geometry: BlockGeometry {
                logical_sector_size: 512,
                sector_count: bytes / 512,
            },
            read_only,
        })
    }

    fn offset(&self, first_lba: u64, bytes: usize) -> Result<u64, BlockError> {
        if bytes == 0 || !bytes.is_multiple_of(512) {
            return Err(BlockError::InvalidRequest);
        }
        let sectors = u64::try_from(bytes / 512).map_err(|_| BlockError::InvalidRequest)?;
        if first_lba
            .checked_add(sectors)
            .ok_or(BlockError::OutOfRange)?
            > self.geometry.sector_count
        {
            return Err(BlockError::OutOfRange);
        }
        first_lba.checked_mul(512).ok_or(BlockError::OutOfRange)
    }
}

impl BlockDevice for FileDevice {
    fn geometry(&self) -> BlockGeometry {
        self.geometry
    }

    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError> {
        let offset = self.offset(first_lba, dst.len())?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| BlockError::Transport)?;
        self.file.read_exact(dst).map_err(|_| BlockError::Transport)
    }

    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError> {
        if self.read_only {
            return Err(BlockError::ReadOnly);
        }
        let offset = self.offset(first_lba, src.len())?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| BlockError::Transport)?;
        self.file
            .write_all(src)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                    BlockError::ReadOnly
                }
                _ => BlockError::Transport,
            })
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.file.sync_all().map_err(|_| BlockError::Transport)
    }
}
