use super::{BlockDevice, BlockError, BlockGeometry, validate_request};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PartitionRange {
    pub first_lba: u64,
    pub sector_count: u64,
}

pub struct PartitionDevice<D> {
    inner: D,
    range: PartitionRange,
    geometry: BlockGeometry,
}

impl<D: BlockDevice> PartitionDevice<D> {
    pub fn new(inner: D, range: PartitionRange) -> Result<Self, BlockError> {
        let inner_geometry = inner.geometry();
        if range.sector_count == 0 || inner_geometry.logical_sector_size == 0 {
            return Err(BlockError::InvalidRequest);
        }

        let end_lba = range
            .first_lba
            .checked_add(range.sector_count)
            .ok_or(BlockError::OutOfRange)?;
        if end_lba > inner_geometry.sector_count {
            return Err(BlockError::OutOfRange);
        }

        Ok(Self {
            inner,
            range,
            geometry: BlockGeometry {
                logical_sector_size: inner_geometry.logical_sector_size,
                sector_count: range.sector_count,
            },
        })
    }

    pub fn inner(&self) -> &D {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut D {
        &mut self.inner
    }

    pub fn into_inner(self) -> D {
        self.inner
    }
}

impl<D: BlockDevice> BlockDevice for PartitionDevice<D> {
    fn geometry(&self) -> BlockGeometry {
        self.geometry
    }

    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError> {
        validate_request(self.geometry, first_lba, dst.len())?;
        let inner_lba = self
            .range
            .first_lba
            .checked_add(first_lba)
            .ok_or(BlockError::OutOfRange)?;
        self.inner.read_sectors(inner_lba, dst)
    }

    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError> {
        validate_request(self.geometry, first_lba, src.len())?;
        let inner_lba = self
            .range
            .first_lba
            .checked_add(first_lba)
            .ok_or(BlockError::OutOfRange)?;
        self.inner.write_sectors(inner_lba, src)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.inner.flush()
    }
}
