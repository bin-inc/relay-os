//! Error numbers (spec §7.2). The values are Linux's, so a program's
//! messages match what GNU tools print for the same error.

pub const EPERM: u16 = 1;
pub const ENOENT: u16 = 2;
pub const ESRCH: u16 = 3;
pub const EINTR: u16 = 4;
pub const EIO: u16 = 5;
pub const E2BIG: u16 = 7;
pub const ENOEXEC: u16 = 8;
pub const EBADF: u16 = 9;
pub const ECHILD: u16 = 10;
pub const EAGAIN: u16 = 11;
pub const ENOMEM: u16 = 12;
pub const EFAULT: u16 = 14;
pub const EBUSY: u16 = 16;
pub const EEXIST: u16 = 17;
pub const EXDEV: u16 = 18;
pub const ENOTDIR: u16 = 20;
pub const EISDIR: u16 = 21;
pub const EINVAL: u16 = 22;
pub const EMFILE: u16 = 24;
pub const EFBIG: u16 = 27;
pub const ENOSPC: u16 = 28;
pub const EROFS: u16 = 30;
pub const EPIPE: u16 = 32;
pub const ENAMETOOLONG: u16 = 36;
pub const ENOSYS: u16 = 38;
pub const ENOTEMPTY: u16 = 39;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// `#define ENAME value` lines of Linux's own headers (linux-libc-dev,
    /// on every Ubuntu machine with a C compiler, the CI runner included).
    fn linux_errnos() -> HashMap<String, u16> {
        let mut map = HashMap::new();
        for file in ["errno-base.h", "errno.h"] {
            let path = format!("/usr/include/asm-generic/{file}");
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            for line in text.lines() {
                let mut words = line.split_whitespace();
                if let (Some("#define"), Some(name), Some(value)) =
                    (words.next(), words.next(), words.next())
                    && let Ok(n) = value.parse()
                {
                    map.insert(name.to_string(), n);
                }
            }
        }
        map
    }

    #[test]
    fn every_number_is_linux_s() {
        let linux = linux_errnos();
        let ours = [
            ("EPERM", EPERM),
            ("ENOENT", ENOENT),
            ("ESRCH", ESRCH),
            ("EINTR", EINTR),
            ("EIO", EIO),
            ("E2BIG", E2BIG),
            ("ENOEXEC", ENOEXEC),
            ("EBADF", EBADF),
            ("ECHILD", ECHILD),
            ("EAGAIN", EAGAIN),
            ("ENOMEM", ENOMEM),
            ("EFAULT", EFAULT),
            ("EBUSY", EBUSY),
            ("EEXIST", EEXIST),
            ("EXDEV", EXDEV),
            ("ENOTDIR", ENOTDIR),
            ("EISDIR", EISDIR),
            ("EINVAL", EINVAL),
            ("EMFILE", EMFILE),
            ("EFBIG", EFBIG),
            ("ENOSPC", ENOSPC),
            ("EROFS", EROFS),
            ("EPIPE", EPIPE),
            ("ENAMETOOLONG", ENAMETOOLONG),
            ("ENOSYS", ENOSYS),
            ("ENOTEMPTY", ENOTEMPTY),
        ];
        for (name, n) in ours {
            assert_eq!(linux.get(name), Some(&n), "{name}");
        }
    }
}
