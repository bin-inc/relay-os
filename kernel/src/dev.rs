//! `/dev` (programmable shell gate §8.4): after the root (startup step 9)
//! and before `/bin` (step 10), the kernel mounts a `vfs::DevFs` at `/dev`,
//! over whatever the disk has there, so `/dev` is filesystem 2. Its line
//! is `[ ok ] dev: /dev/null`, or `[FAIL] dev: cannot mount /dev: <reason>`
//! (a disk whose `/dev` is a file), and the boot goes on without it.

use crate::{console, rtc};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use vfs::{DevFs, Errno, MountTable};

/// Mounts `/dev`, made at `now`.
pub fn mount_into(vfs: &mut MountTable, now: u64) -> Result<(), Errno> {
    vfs.mount(b"/dev", Box::new(DevFs::new(now)))
}

/// The reason on the `[FAIL]` line.
pub fn failure(e: Errno) -> String {
    format!("cannot mount /dev: {}", e.message())
}

/// Mounts `/dev` and prints its startup line.
pub fn mount(vfs: &mut MountTable) {
    match mount_into(vfs, rtc::now_unix().unwrap_or(0)) {
        Ok(()) => console::ok(format_args!("dev: /dev/null")),
        Err(e) => console::fail("dev", format_args!("{}", failure(e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vfs::{Env, FileSystem, FileType, MemFs, Vfs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    #[test]
    fn dev_null_is_mounted_before_bin() {
        // The root's /dev, as QEMU's disk has one, or none at all.
        for with_dir in [true, false] {
            let mut fs = MemFs::new(Box::new(Clock));
            if with_dir {
                let root = fs.root();
                fs.mkdir(root, b"dev").unwrap();
            }
            let mut t = MountTable::new(Box::new(fs.read_only()));
            assert_eq!(mount_into(&mut t, 1_000), Ok(()));
            let null = t.lookup(b"/dev/null").unwrap();
            assert_eq!(null.mount, 1, "filesystem 2, before /bin");
            let st = t.stat(null).unwrap();
            assert_eq!((st.kind, st.mtime), (FileType::CharDev, 1_000));
        }
    }

    #[test]
    fn a_dev_that_is_a_file_fails_the_mount() {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        fs.create(root, b"dev").unwrap();
        let mut t = MountTable::new(Box::new(fs));
        let e = mount_into(&mut t, 0).unwrap_err();
        assert_eq!(e, Errno::ENOTDIR);
        assert_eq!(failure(e), "cannot mount /dev: Not a directory");
    }
}
