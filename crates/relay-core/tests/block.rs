mod support;

use std::{cell::Cell, rc::Rc};

use relay_core::block::{BlockDevice, BlockError, BlockGeometry, PartitionDevice, PartitionRange};
use support::memory_device::MemoryDevice;

#[test]
fn partition_rejects_end_overflow_without_io() {
    let inner = MemoryDevice::new(512, 16).unwrap();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange {
            first_lba: 8,
            sector_count: 8,
        },
    )
    .unwrap();

    assert_eq!(
        part.read_sectors(7, &mut [0; 1024]),
        Err(BlockError::OutOfRange)
    );
    assert_eq!(part.inner().read_count(), 0);
}

#[test]
fn partition_translates_valid_relative_requests() {
    let mut inner = MemoryDevice::new(512, 16).unwrap();
    inner.fill_sector(9, 0x5a).unwrap();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange {
            first_lba: 8,
            sector_count: 4,
        },
    )
    .unwrap();
    let mut bytes = [0; 512];

    part.read_sectors(1, &mut bytes).unwrap();

    assert_eq!(bytes, [0x5a; 512]);
    assert_eq!(part.inner().read_lbas(), &[9]);
}

#[test]
fn partition_rejects_empty_or_non_sector_buffers_without_io() {
    let inner = MemoryDevice::new(512, 16).unwrap();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange {
            first_lba: 8,
            sector_count: 8,
        },
    )
    .unwrap();

    assert_eq!(part.write_sectors(0, &[]), Err(BlockError::InvalidRequest));
    assert_eq!(
        part.write_sectors(0, &[0; 1]),
        Err(BlockError::InvalidRequest)
    );
    assert_eq!(part.inner().write_count(), 0);

    assert_eq!(
        part.read_sectors(0, &mut []),
        Err(BlockError::InvalidRequest)
    );
    assert_eq!(
        part.read_sectors(0, &mut [0; 1]),
        Err(BlockError::InvalidRequest)
    );
    assert_eq!(part.inner().read_count(), 0);
}

#[test]
fn partition_constructor_rejects_an_inner_range_overflow() {
    assert!(matches!(
        PartitionDevice::new(
            MemoryDevice::new(512, 16).unwrap(),
            PartitionRange {
                first_lba: 15,
                sector_count: 2,
            },
        ),
        Err(BlockError::OutOfRange),
    ));
}

#[test]
fn partition_forwards_flush_and_write_errors_unchanged() {
    let inner = MemoryDevice::new(512, 16)
        .unwrap()
        .with_read_only()
        .with_flush_error();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange {
            first_lba: 8,
            sector_count: 8,
        },
    )
    .unwrap();

    assert_eq!(part.write_sectors(0, &[0; 512]), Err(BlockError::ReadOnly));
    assert_eq!(part.flush(), Err(BlockError::Flush));
}

#[test]
fn partition_rejects_u64_max_relative_lba_without_io() {
    let inner = MemoryDevice::new(512, 16).unwrap();
    let mut part = PartitionDevice::new(
        inner,
        PartitionRange {
            first_lba: 8,
            sector_count: 8,
        },
    )
    .unwrap();

    assert_eq!(
        part.read_sectors(u64::MAX, &mut [0; 512]),
        Err(BlockError::OutOfRange)
    );
    assert_eq!(part.inner().read_count(), 0);
}

#[test]
fn partition_constructor_rejects_u64_max_range_start() {
    let io_count = Rc::new(Cell::new(0));
    assert!(matches!(
        PartitionDevice::new(
            IoProbe {
                io_count: Rc::clone(&io_count),
            },
            PartitionRange {
                first_lba: u64::MAX,
                sector_count: 1,
            },
        ),
        Err(BlockError::OutOfRange),
    ));
    assert_eq!(io_count.get(), 0);
}

#[test]
fn memory_device_rejects_unrepresentable_allocation_and_fill_lba() {
    assert!(matches!(
        MemoryDevice::new(512, u64::MAX),
        Err(BlockError::InvalidRequest),
    ));

    let mut device = MemoryDevice::new(512, 16).unwrap();
    assert_eq!(
        device.fill_sector(u64::MAX, 0x5a),
        Err(BlockError::OutOfRange)
    );
}

struct IoProbe {
    io_count: Rc<Cell<usize>>,
}

impl BlockDevice for IoProbe {
    fn geometry(&self) -> BlockGeometry {
        BlockGeometry {
            logical_sector_size: 512,
            sector_count: 16,
        }
    }

    fn read_sectors(&mut self, _first_lba: u64, _dst: &mut [u8]) -> Result<(), BlockError> {
        self.io_count.set(self.io_count.get() + 1);
        Ok(())
    }

    fn write_sectors(&mut self, _first_lba: u64, _src: &[u8]) -> Result<(), BlockError> {
        self.io_count.set(self.io_count.get() + 1);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.io_count.set(self.io_count.get() + 1);
        Ok(())
    }
}
