mod partition;

pub use partition::{PartitionDevice, PartitionRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockGeometry {
    pub logical_sector_size: u32,
    pub sector_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockError {
    InvalidRequest,
    OutOfRange,
    ShortTransfer,
    Timeout,
    Transport,
    ReadOnly,
    Flush,
}

pub trait BlockDevice {
    fn geometry(&self) -> BlockGeometry;
    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError>;
    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;
}

pub(crate) fn validate_request(
    geometry: BlockGeometry,
    first_lba: u64,
    bytes: usize,
) -> Result<(), BlockError> {
    let sector_size =
        usize::try_from(geometry.logical_sector_size).map_err(|_| BlockError::InvalidRequest)?;
    if sector_size == 0 || bytes == 0 || !bytes.is_multiple_of(sector_size) {
        return Err(BlockError::InvalidRequest);
    }

    let count = u64::try_from(bytes / sector_size).map_err(|_| BlockError::InvalidRequest)?;
    let end_lba = first_lba.checked_add(count).ok_or(BlockError::OutOfRange)?;
    if end_lba > geometry.sector_count {
        return Err(BlockError::OutOfRange);
    }

    Ok(())
}
