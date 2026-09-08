use alloc::vec::Vec;
use uefi::{
    boot, cstr16,
    proto::media::file::{File, FileAttribute, FileMode},
};

const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileError {
    Open,
    InvalidHandle,
    Read,
    TooLarge,
}

pub fn read_loader_file(path: &'static uefi::CStr16) -> Result<Vec<u8>, FileError> {
    let mut file_system =
        boot::get_image_file_system(boot::image_handle()).map_err(|_| FileError::Open)?;
    let mut root = file_system.open_volume().map_err(|_| FileError::Open)?;
    let handle = root
        .open(path, FileMode::Read, FileAttribute::empty())
        .map_err(|_| FileError::Open)?;
    let mut file = handle.into_regular_file().ok_or(FileError::InvalidHandle)?;
    let mut contents = Vec::new();
    let mut buffer = [0_u8; 4096];

    loop {
        let count = file.read(&mut buffer).map_err(|_| FileError::Read)?;
        if count == 0 {
            return Ok(contents);
        }
        let end = contents.checked_len_add(count).ok_or(FileError::TooLarge)?;
        if end > MAX_FILE_BYTES {
            return Err(FileError::TooLarge);
        }
        contents.extend_from_slice(&buffer[..count]);
    }
}

trait CheckedLenAdd {
    fn checked_len_add(&self, rhs: usize) -> Option<usize>;
}

impl CheckedLenAdd for Vec<u8> {
    fn checked_len_add(&self, rhs: usize) -> Option<usize> {
        self.len().checked_add(rhs)
    }
}

pub fn read_config() -> Result<Vec<u8>, FileError> {
    read_loader_file(cstr16!("\\EFI\\RELAY\\relay.cfg"))
}

pub fn read_kernel() -> Result<Vec<u8>, FileError> {
    read_loader_file(cstr16!("\\EFI\\RELAY\\kernel.elf"))
}
