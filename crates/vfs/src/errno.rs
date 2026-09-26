//! Error numbers with Linux names and messages (spec §8.1, §10).

use crate::IoError;
use core::fmt;

/// A filesystem error. The names and messages are Linux's, so the shell can
/// print them the way GNU tools do (`cat: foo: No such file or directory`).
#[allow(clippy::upper_case_acronyms)] // the Linux names
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Errno {
    ENOENT,
    EEXIST,
    ENOTDIR,
    EISDIR,
    ENOTEMPTY,
    ENOSPC,
    EIO,
    EROFS,
    EINVAL,
    ENAMETOOLONG,
    EXDEV,
    EBUSY,
    /// A file would grow past the largest size the filesystem can map.
    EFBIG,
}

impl Errno {
    /// Linux's `strerror` text.
    pub const fn message(self) -> &'static str {
        match self {
            Errno::ENOENT => "No such file or directory",
            Errno::EEXIST => "File exists",
            Errno::ENOTDIR => "Not a directory",
            Errno::EISDIR => "Is a directory",
            Errno::ENOTEMPTY => "Directory not empty",
            Errno::ENOSPC => "No space left on device",
            Errno::EIO => "Input/output error",
            Errno::EROFS => "Read-only file system",
            Errno::EINVAL => "Invalid argument",
            Errno::ENAMETOOLONG => "File name too long",
            Errno::EXDEV => "Invalid cross-device link",
            Errno::EBUSY => "Device or resource busy",
            Errno::EFBIG => "File too large",
        }
    }
}

impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

/// Device errors reach callers as `EIO` (spec §10).
impl From<IoError> for Errno {
    fn from(_: IoError) -> Errno {
        Errno::EIO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_linux_strerror_texts() {
        assert_eq!(Errno::ENOENT.to_string(), "No such file or directory");
        assert_eq!(Errno::ENOTEMPTY.to_string(), "Directory not empty");
        assert_eq!(Errno::ENOSPC.to_string(), "No space left on device");
        assert_eq!(Errno::EIO.to_string(), "Input/output error");
        assert_eq!(Errno::EROFS.to_string(), "Read-only file system");
        assert_eq!(Errno::EXDEV.to_string(), "Invalid cross-device link");
        assert_eq!(Errno::EFBIG.to_string(), "File too large");
    }

    #[test]
    fn device_errors_become_eio() {
        assert_eq!(Errno::from(IoError::Device), Errno::EIO);
        assert_eq!(Errno::from(IoError::OutOfRange), Errno::EIO);
    }
}
