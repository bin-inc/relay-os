//! Error numbers with Linux names, messages and values (spec §8.1, §10 of
//! milestone 1; §7.2 of the user-space gate).

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
    /// The arguments of a new program are too long.
    E2BIG,
    /// A file that is not a program this kernel can run.
    ENOEXEC,
    /// A file descriptor that is not open.
    EBADF,
    /// No such child to wait for.
    ECHILD,
    /// A limit on processes was reached; trying later may work.
    EAGAIN,
    /// Not enough memory for a program.
    ENOMEM,
    /// A pointer a program passed does not point at its memory.
    EFAULT,
    /// A system call this kernel does not have.
    ENOSYS,
    /// No process or process group by that number.
    ESRCH,
    /// Process 1 cannot be killed.
    EPERM,
    /// A call that blocked was cut short (the process was killed).
    EINTR,
    /// Every fd of a process is in use.
    EMFILE,
    /// A buffer too short for the answer (`getcwd`).
    ERANGE,
}

impl Errno {
    /// Every error number.
    pub const ALL: [Errno; 26] = [
        Errno::ENOENT,
        Errno::EEXIST,
        Errno::ENOTDIR,
        Errno::EISDIR,
        Errno::ENOTEMPTY,
        Errno::ENOSPC,
        Errno::EIO,
        Errno::EROFS,
        Errno::EINVAL,
        Errno::ENAMETOOLONG,
        Errno::EXDEV,
        Errno::EBUSY,
        Errno::EFBIG,
        Errno::E2BIG,
        Errno::ENOEXEC,
        Errno::EBADF,
        Errno::ECHILD,
        Errno::EAGAIN,
        Errno::ENOMEM,
        Errno::EFAULT,
        Errno::ENOSYS,
        Errno::ESRCH,
        Errno::EPERM,
        Errno::EINTR,
        Errno::EMFILE,
        Errno::ERANGE,
    ];

    /// The error with Linux's number `n` (a system call's error, in a
    /// program); `EIO` for a number this list does not have.
    pub fn from_number(n: u16) -> Errno {
        Errno::ALL
            .into_iter()
            .find(|e| e.number() == n)
            .unwrap_or(Errno::EIO)
    }

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
            Errno::E2BIG => "Argument list too long",
            Errno::ENOEXEC => "Exec format error",
            Errno::EBADF => "Bad file descriptor",
            Errno::ECHILD => "No child processes",
            Errno::EAGAIN => "Resource temporarily unavailable",
            Errno::ENOMEM => "Cannot allocate memory",
            Errno::EFAULT => "Bad address",
            Errno::ENOSYS => "Function not implemented",
            Errno::ESRCH => "No such process",
            Errno::EPERM => "Operation not permitted",
            Errno::EINTR => "Interrupted system call",
            Errno::EMFILE => "Too many open files",
            Errno::ERANGE => "Numerical result out of range",
        }
    }

    /// The number a system call returns for it (`relay_abi::errno`, Linux's
    /// values).
    pub const fn number(self) -> u16 {
        use relay_abi::errno as n;
        match self {
            Errno::ENOENT => n::ENOENT,
            Errno::EEXIST => n::EEXIST,
            Errno::ENOTDIR => n::ENOTDIR,
            Errno::EISDIR => n::EISDIR,
            Errno::ENOTEMPTY => n::ENOTEMPTY,
            Errno::ENOSPC => n::ENOSPC,
            Errno::EIO => n::EIO,
            Errno::EROFS => n::EROFS,
            Errno::EINVAL => n::EINVAL,
            Errno::ENAMETOOLONG => n::ENAMETOOLONG,
            Errno::EXDEV => n::EXDEV,
            Errno::EBUSY => n::EBUSY,
            Errno::EFBIG => n::EFBIG,
            Errno::E2BIG => n::E2BIG,
            Errno::ENOEXEC => n::ENOEXEC,
            Errno::EBADF => n::EBADF,
            Errno::ECHILD => n::ECHILD,
            Errno::EAGAIN => n::EAGAIN,
            Errno::ENOMEM => n::ENOMEM,
            Errno::EFAULT => n::EFAULT,
            Errno::ENOSYS => n::ENOSYS,
            Errno::ESRCH => n::ESRCH,
            Errno::EPERM => n::EPERM,
            Errno::EINTR => n::EINTR,
            Errno::EMFILE => n::EMFILE,
            Errno::ERANGE => n::ERANGE,
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

    /// The name, number and message the host's C library gives: the real
    /// thing, independent of `relay_abi`'s table.
    #[test]
    fn each_number_and_message_is_the_host_s() {
        for e in Errno::ALL {
            let n = e.number();
            let host = std::io::Error::from_raw_os_error(i32::from(n)).to_string();
            assert_eq!(
                host,
                format!("{} (os error {n})", e.message()),
                "{e:?} is {n}"
            );
        }
        let mut numbers: Vec<u16> = Errno::ALL.iter().map(|e| e.number()).collect();
        numbers.sort();
        numbers.dedup();
        assert_eq!(numbers.len(), Errno::ALL.len(), "numbers are unique");
    }

    #[test]
    fn a_number_names_its_error() {
        for e in Errno::ALL {
            assert_eq!(Errno::from_number(e.number()), e);
        }
        assert_eq!(Errno::from_number(0), Errno::EIO);
        assert_eq!(Errno::from_number(4095), Errno::EIO);
    }

    #[test]
    fn device_errors_become_eio() {
        assert_eq!(Errno::from(IoError::Device), Errno::EIO);
        assert_eq!(Errno::from(IoError::OutOfRange), Errno::EIO);
    }
}
