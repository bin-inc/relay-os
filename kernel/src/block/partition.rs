//! A GPT partition as a block device (spec §6.5), so a filesystem sees only
//! its own blocks.

use vfs::{BlockDevice, IoError, check_request};

/// One GPT partition of a disk as a block device of its own (spec §6.5): block numbers
/// are relative to its start, and requests outside it are refused (vfs::check_request).
pub struct Partition<D: BlockDevice> {
    dev: D,
    start: u64,
    blocks: u64,
}

impl<D: BlockDevice> Partition<D> {
    /// `first_lba..=last_lba` of `dev`; `Err(IoError::OutOfRange)` if that is not inside the disk or is reversed.
    pub fn new(dev: D, first_lba: u64, last_lba: u64) -> Result<Partition<D>, IoError> {
        if first_lba > last_lba || last_lba >= dev.block_count() {
            return Err(IoError::OutOfRange);
        }
        Ok(Partition {
            dev,
            start: first_lba,
            // last_lba < block_count, so this cannot overflow.
            blocks: last_lba - first_lba + 1,
        })
    }

    /// The partition's first block on the disk.
    pub fn start(&self) -> u64 {
        self.start
    }
}

impl<D: BlockDevice> BlockDevice for Partition<D> {
    fn block_size(&self) -> usize {
        self.dev.block_size()
    }
    fn block_count(&self) -> u64 {
        self.blocks
    }
    // After check_request, start + lba lies inside the disk, so it cannot
    // overflow.
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        check_request(self.block_size(), self.blocks, lba, buf.len())?;
        self.dev.read(self.start + lba, buf)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        check_request(self.block_size(), self.blocks, lba, buf.len())?;
        self.dev.write(self.start + lba, buf)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.dev.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::MemDisk;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A 16-block disk whose block `n` is filled with `n`.
    fn disk(bs: usize) -> MemDisk {
        let data: Vec<u8> = (0..16u8).flat_map(|n| vec![n; bs]).collect();
        MemDisk::new(data, bs)
    }

    #[test]
    fn block_numbers_are_relative_to_the_start() {
        for bs in [512, 4096] {
            let mut disk = disk(bs);
            let mut p = Partition::new(&mut disk, 4, 11).unwrap();
            assert_eq!(p.start(), 4);
            assert_eq!(p.block_size(), bs);
            assert_eq!(p.block_count(), 8);
            let mut buf = vec![0; 2 * bs];
            p.read(1, &mut buf).unwrap();
            assert!(buf[..bs].iter().all(|&b| b == 5));
            assert!(buf[bs..].iter().all(|&b| b == 6));
            p.write(2, &vec![0xEE; bs]).unwrap();
            assert!(disk.data[6 * bs..7 * bs].iter().all(|&b| b == 0xEE));
            assert!(disk.data[5 * bs..6 * bs].iter().all(|&b| b == 5));
            assert!(disk.data[7 * bs..8 * bs].iter().all(|&b| b == 7));
        }
    }

    #[test]
    fn the_first_and_last_blocks_are_reachable() {
        let mut disk = disk(512);
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let mut buf = vec![0; 512];
        p.read(0, &mut buf).unwrap();
        assert_eq!(buf[0], 4);
        p.read(7, &mut buf).unwrap();
        assert_eq!(buf[0], 11);
        let mut all = vec![0; 8 * 512];
        p.read(0, &mut all).unwrap();
        assert_eq!((all[0], all[all.len() - 1]), (4, 11));
        p.write(7, &[0xEE; 512]).unwrap();
        assert_eq!((disk.data[11 * 512], disk.data[12 * 512]), (0xEE, 12));
    }

    #[test]
    fn requests_outside_the_partition_touch_nothing() {
        let mut disk = disk(512);
        let before = disk.data.clone();
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let cases: [(u64, usize); 6] = [
            (8, 512),        // at the end
            (9, 512),        // past the end
            (7, 1024),       // runs over the end
            (u64::MAX, 512), // start + lba overflows
            (u64::MAX - 3, 512),
            (0, 100), // not whole blocks
        ];
        for (lba, len) in cases {
            let mut buf = vec![0xAA; len];
            assert_eq!(p.read(lba, &mut buf), Err(IoError::OutOfRange), "{lba}");
            assert!(buf.iter().all(|&b| b == 0xAA));
            assert_eq!(p.write(lba, &buf), Err(IoError::OutOfRange), "{lba}");
        }
        assert_eq!(disk.requests, 0);
        assert!(disk.data == before);
    }

    #[test]
    fn new_refuses_ranges_outside_the_disk_or_reversed() {
        let mut disk = disk(512);
        for (first, last) in [(4, 16), (16, 16), (4, u64::MAX), (5, 4), (u64::MAX, 0)] {
            assert!(
                matches!(
                    Partition::new(&mut disk, first, last),
                    Err(IoError::OutOfRange)
                ),
                "{first}..={last}"
            );
        }
        // The whole disk and single blocks are fine.
        assert_eq!(Partition::new(&mut disk, 0, 15).unwrap().block_count(), 16);
        assert_eq!(Partition::new(&mut disk, 15, 15).unwrap().block_count(), 1);
        assert_eq!(Partition::new(&mut disk, 0, 0).unwrap().block_count(), 1);
    }

    #[test]
    fn flush_forwards() {
        let mut disk = disk(512);
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        p.flush().unwrap();
        p.flush().unwrap();
        assert_eq!(disk.flushes, 2);
    }

    #[test]
    fn device_errors_pass_through() {
        let mut disk = disk(512);
        disk.bad = 5..6;
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let mut buf = vec![0; 512];
        assert_eq!(p.read(1, &mut buf), Err(IoError::Device));
        assert_eq!(p.read(0, &mut buf), Ok(()));
    }
}
