use std::{cell::Cell, rc::Rc};

use relay_core::block::{BlockDevice, BlockError, BlockGeometry};

#[derive(Clone, Default)]
pub struct OperationLog {
    reads: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
    flushes: Rc<Cell<usize>>,
}

impl OperationLog {
    pub fn read_count(&self) -> usize {
        self.reads.get()
    }

    pub fn write_count(&self) -> usize {
        self.writes.get()
    }

    pub fn flush_count(&self) -> usize {
        self.flushes.get()
    }
}

pub struct MemoryDevice {
    sector_size: u32,
    sector_count: u64,
    bytes: Vec<u8>,
    read_lbas: Vec<u64>,
    write_count: usize,
    read_only: bool,
    flush_error: bool,
    operations: OperationLog,
}

impl MemoryDevice {
    pub fn new(sector_size: u32, sector_count: u64) -> Result<Self, BlockError> {
        let sector_size_usize =
            usize::try_from(sector_size).map_err(|_| BlockError::InvalidRequest)?;
        let sector_count_usize =
            usize::try_from(sector_count).map_err(|_| BlockError::InvalidRequest)?;
        let byte_count = sector_size_usize
            .checked_mul(sector_count_usize)
            .ok_or(BlockError::InvalidRequest)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_count)
            .map_err(|_| BlockError::InvalidRequest)?;
        bytes.resize(byte_count, 0);

        Self::from_bytes(sector_size, bytes)
    }

    pub fn from_bytes(sector_size: u32, bytes: Vec<u8>) -> Result<Self, BlockError> {
        let sector_size_usize =
            usize::try_from(sector_size).map_err(|_| BlockError::InvalidRequest)?;
        if sector_size_usize == 0 || !bytes.len().is_multiple_of(sector_size_usize) {
            return Err(BlockError::InvalidRequest);
        }
        let sector_count = u64::try_from(bytes.len() / sector_size_usize)
            .map_err(|_| BlockError::InvalidRequest)?;

        Ok(Self {
            sector_size,
            sector_count,
            bytes,
            read_lbas: Vec::new(),
            write_count: 0,
            read_only: false,
            flush_error: false,
            operations: OperationLog::default(),
        })
    }

    pub fn fill_sector(&mut self, lba: u64, byte: u8) -> Result<(), BlockError> {
        let range = self.byte_range(lba, self.sector_size_usize()?)?;
        self.bytes
            .get_mut(range)
            .ok_or(BlockError::OutOfRange)?
            .fill(byte);
        Ok(())
    }

    pub fn read_count(&self) -> usize {
        self.read_lbas.len()
    }

    pub fn write_count(&self) -> usize {
        self.write_count
    }

    pub fn read_lbas(&self) -> &[u64] {
        &self.read_lbas
    }

    pub fn operations(&self) -> OperationLog {
        self.operations.clone()
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn with_read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_flush_error(mut self) -> Self {
        self.flush_error = true;
        self
    }
}

impl BlockDevice for MemoryDevice {
    fn geometry(&self) -> BlockGeometry {
        BlockGeometry {
            logical_sector_size: self.sector_size,
            sector_count: self.sector_count,
        }
    }

    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError> {
        validate_request(self.geometry(), first_lba, dst.len())?;
        let range = self.byte_range(first_lba, dst.len())?;
        dst.copy_from_slice(self.bytes.get(range).ok_or(BlockError::OutOfRange)?);
        self.read_lbas.push(first_lba);
        self.operations.reads.set(self.operations.reads.get() + 1);
        Ok(())
    }

    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError> {
        validate_request(self.geometry(), first_lba, src.len())?;
        if self.read_only {
            return Err(BlockError::ReadOnly);
        }

        let range = self.byte_range(first_lba, src.len())?;
        self.bytes
            .get_mut(range)
            .ok_or(BlockError::OutOfRange)?
            .copy_from_slice(src);
        self.write_count += 1;
        self.operations.writes.set(self.operations.writes.get() + 1);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.operations
            .flushes
            .set(self.operations.flushes.get() + 1);
        if self.flush_error {
            return Err(BlockError::Flush);
        }
        Ok(())
    }
}

impl MemoryDevice {
    fn sector_size_usize(&self) -> Result<usize, BlockError> {
        usize::try_from(self.sector_size).map_err(|_| BlockError::InvalidRequest)
    }

    fn byte_range(
        &self,
        first_lba: u64,
        bytes: usize,
    ) -> Result<core::ops::Range<usize>, BlockError> {
        let first_lba = usize::try_from(first_lba).map_err(|_| BlockError::OutOfRange)?;
        let start = first_lba
            .checked_mul(self.sector_size_usize()?)
            .ok_or(BlockError::OutOfRange)?;
        let end = start.checked_add(bytes).ok_or(BlockError::OutOfRange)?;
        if end > self.bytes.len() {
            return Err(BlockError::OutOfRange);
        }

        Ok(start..end)
    }
}

fn validate_request(
    geometry: BlockGeometry,
    first_lba: u64,
    bytes: usize,
) -> Result<(), BlockError> {
    let sector_size = geometry.logical_sector_size as usize;
    if sector_size == 0 || bytes == 0 || !bytes.is_multiple_of(sector_size) {
        return Err(BlockError::InvalidRequest);
    }

    let count = (bytes / sector_size) as u64;
    let end_lba = first_lba.checked_add(count).ok_or(BlockError::OutOfRange)?;
    if end_lba > geometry.sector_count {
        return Err(BlockError::OutOfRange);
    }

    Ok(())
}
