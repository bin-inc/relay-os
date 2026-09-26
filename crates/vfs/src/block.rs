//! The block-device trait (spec §6.5). The USB storage driver, GPT
//! partitions and file-backed test devices implement it; filesystems use it.

use alloc::boxed::Box;

/// A device error. Drivers turn their own failures into one of these; the
/// VFS turns every one into `EIO` (spec §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoError {
    /// The request reaches past the end of the device, or its buffer is not
    /// a whole number of blocks.
    OutOfRange,
    /// The device failed the request.
    Device,
}

/// A device addressed in fixed-size blocks.
pub trait BlockDevice {
    /// Bytes per block (512 for the USB stick).
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;
    /// Reads `buf.len() / block_size()` blocks starting at block `lba`.
    /// `buf.len()` must be a multiple of the block size.
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError>;
    /// Writes `buf.len() / block_size()` blocks starting at block `lba`.
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError>;
    /// Makes every completed write durable (`SYNCHRONIZE CACHE` on USB).
    fn flush(&mut self) -> Result<(), IoError>;
}

/// Checks that a request of `len` bytes at block `lba` is whole blocks and
/// lies inside a device of `block_count` blocks of `block_size` bytes.
pub fn check_request(
    block_size: usize,
    block_count: u64,
    lba: u64,
    len: usize,
) -> Result<(), IoError> {
    if block_size == 0 || !len.is_multiple_of(block_size) {
        return Err(IoError::OutOfRange);
    }
    let blocks = (len / block_size) as u64;
    match lba.checked_add(blocks) {
        Some(end) if end <= block_count => Ok(()),
        _ => Err(IoError::OutOfRange),
    }
}

impl<T: BlockDevice + ?Sized> BlockDevice for &mut T {
    fn block_size(&self) -> usize {
        (**self).block_size()
    }
    fn block_count(&self) -> u64 {
        (**self).block_count()
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        (**self).read(lba, buf)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        (**self).write(lba, buf)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        (**self).flush()
    }
}

impl<T: BlockDevice + ?Sized> BlockDevice for Box<T> {
    fn block_size(&self) -> usize {
        (**self).block_size()
    }
    fn block_count(&self) -> u64 {
        (**self).block_count()
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        (**self).read(lba, buf)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        (**self).write(lba, buf)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        (**self).flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_must_be_whole_blocks_inside_the_device() {
        assert_eq!(check_request(512, 8, 0, 4096), Ok(()));
        assert_eq!(check_request(512, 8, 7, 512), Ok(()));
        assert_eq!(check_request(512, 8, 7, 1024), Err(IoError::OutOfRange));
        assert_eq!(check_request(512, 8, 8, 0), Ok(()));
        assert_eq!(check_request(512, 8, 9, 0), Err(IoError::OutOfRange));
        assert_eq!(check_request(512, 8, 0, 100), Err(IoError::OutOfRange));
        assert_eq!(
            check_request(512, 8, u64::MAX, 512),
            Err(IoError::OutOfRange)
        );
    }

    struct Counting {
        flushes: u32,
    }

    impl BlockDevice for Counting {
        fn block_size(&self) -> usize {
            512
        }
        fn block_count(&self) -> u64 {
            4
        }
        fn read(&mut self, _: u64, _: &mut [u8]) -> Result<(), IoError> {
            Ok(())
        }
        fn write(&mut self, _: u64, _: &[u8]) -> Result<(), IoError> {
            Err(IoError::Device)
        }
        fn flush(&mut self) -> Result<(), IoError> {
            self.flushes += 1;
            Ok(())
        }
    }

    fn exercise(mut dev: impl BlockDevice) {
        assert_eq!((dev.block_size(), dev.block_count()), (512, 4));
        assert_eq!(dev.write(0, &[0; 512]), Err(IoError::Device));
        dev.flush().unwrap();
    }

    #[test]
    fn borrowed_and_boxed_devices_forward_every_call() {
        let mut dev = Counting { flushes: 0 };
        exercise(&mut dev);
        exercise(Box::new(&mut dev));
        assert_eq!(dev.flushes, 2);
    }
}
