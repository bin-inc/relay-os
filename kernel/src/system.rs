//! The system archive (spec §4.3 of the user-space gate): the programs the
//! loader read from `\EFI\RELAY\system.img`, checked and mounted read-only
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 29 programs, ABI 1` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.

use crate::console;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use boot_info::{BootInfo, MemoryKind, MemoryRegion, PHYS_OFFSET};
use core::fmt;
use sysimg::{SysImgError, SysImgFs};
use vfs::{Errno, MountTable};

#[derive(Debug, PartialEq, Eq)]
pub enum SystemError {
    /// The loader could not read a `system.img`.
    Missing,
    /// The loader's range is not memory it allocated.
    NotLoaderMemory {
        phys: u64,
        len: u64,
    },
    Bad(SysImgError),
    /// Built for another ABI than this kernel's.
    Abi(u32),
    Mount(Errno),
}

impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SystemError::Missing => write!(f, "no system.img"),
            SystemError::NotLoaderMemory { phys, len } => {
                write!(
                    f,
                    "system.img at {phys:#x} ({len} bytes) is not loader memory"
                )
            }
            SystemError::Bad(e) => write!(f, "system.img: {e}"),
            SystemError::Abi(abi) => {
                write!(f, "ABI {abi}, kernel wants {}", relay_abi::VERSION)
            }
            SystemError::Mount(e) => write!(f, "cannot mount /bin: {}", e.message()),
        }
    }
}

/// `phys..phys + len` lies in `Bootloader` regions only (the map is sorted
/// by start; neighbouring regions may split the range).
pub fn in_loader_memory(map: &[MemoryRegion], phys: u64, len: u64) -> bool {
    let Some(end) = phys.checked_add(len) else {
        return false;
    };
    let mut covered = phys;
    for r in map {
        if covered >= end {
            break;
        }
        if r.kind == MemoryKind::Bootloader && r.start <= covered && covered < r.end() {
            covered = r.end();
        }
    }
    covered >= end
}

/// Checks the archive and its ABI version.
pub fn open(bytes: &'static [u8]) -> Result<SysImgFs, SystemError> {
    let fs = SysImgFs::new(bytes).map_err(SystemError::Bad)?;
    match fs.archive().abi() {
        relay_abi::VERSION => Ok(fs),
        other => Err(SystemError::Abi(other)),
    }
}

/// Opens the archive in `bytes` and mounts it at `/bin`; returns the text
/// of the startup line.
pub fn mount_into(vfs: &mut MountTable, bytes: &'static [u8]) -> Result<String, SystemError> {
    let fs = open(bytes)?;
    let n = fs.archive().len();
    let text = format!(
        "{n} program{}, ABI {}",
        if n == 1 { "" } else { "s" },
        relay_abi::VERSION
    );
    vfs.mount(b"/bin", Box::new(fs))
        .map_err(SystemError::Mount)?;
    Ok(text)
}

/// Startup step 10: mounts the loader's archive at `/bin` and prints the
/// `system` line.
pub fn mount(info: &BootInfo, vfs: &mut MountTable) {
    let result = (|| {
        let (phys, len) = info.system_image().ok_or(SystemError::Missing)?;
        // SAFETY: built by relay-boot, lives forever.
        let map = unsafe { info.memory_map() };
        if !in_loader_memory(map, phys, len) {
            return Err(SystemError::NotLoaderMemory { phys, len });
        }
        // SAFETY: loader memory, in the linear map and never handed out
        // (`mm::frame`), so it stays as the loader left it.
        let bytes =
            unsafe { core::slice::from_raw_parts((PHYS_OFFSET + phys) as *const u8, len as usize) };
        mount_into(vfs, bytes)
    })();
    match result {
        Ok(text) => console::ok(format_args!("system: {text}")),
        Err(e) => console::fail("system", format_args!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use vfs::{Env, FileSystem, MemFs, Vfs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    fn archive(abi: u32, names: &[&str]) -> &'static [u8] {
        let entries: Vec<sysimg::Entry<'_>> = names
            .iter()
            .map(|n| sysimg::Entry {
                name: n.as_bytes(),
                mode: 0o755,
                data: b"\x7fELF",
            })
            .collect();
        Vec::leak(sysimg::write(abi, 0, &entries).unwrap())
    }

    fn table(with_bin: bool) -> MountTable {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        if with_bin {
            fs.mkdir(root, b"bin").unwrap();
        }
        MountTable::new(Box::new(fs))
    }

    fn region(start: u64, end: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion {
            start,
            len: end - start,
            kind,
        }
    }

    #[test]
    fn the_archive_is_mounted_at_bin() {
        let mut vfs = table(true);
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 1");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
        assert_eq!(vfs.stat(node).unwrap().perm, 0o755);
        let mut one = table(true);
        assert_eq!(
            mount_into(&mut one, archive(1, &["t-args"])).unwrap(),
            "1 program, ABI 1"
        );
    }

    #[test]
    fn a_root_without_bin_gets_one() {
        let mut vfs = table(false);
        mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args"])).unwrap();
        assert!(vfs.lookup(b"/bin/t-args").is_ok());
    }

    #[test]
    fn a_bad_archive_is_not_mounted() {
        let mut vfs = table(true);
        let e = mount_into(&mut vfs, &[]).unwrap_err();
        assert_eq!(e.to_string(), "system.img: only 0 bytes", "an empty file");
        let junk: &'static [u8] =
            Vec::leak(b"this is not an archive, but long enough to have a header....".repeat(2));
        let e = mount_into(&mut vfs, junk).unwrap_err();
        assert_eq!(e.to_string(), "system.img: not a system archive");
        let e = mount_into(&mut vfs, archive(99, &["t-args"])).unwrap_err();
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 1");
        assert_eq!(
            vfs.lookup(b"/bin/t-args"),
            Err(Errno::ENOENT),
            "nothing mounted"
        );
    }

    #[test]
    fn a_bin_that_is_a_file_is_reported() {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        fs.create(root, b"bin").unwrap();
        let mut vfs = MountTable::new(Box::new(fs));
        let e = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args"])).unwrap_err();
        assert_eq!(e.to_string(), "cannot mount /bin: Not a directory");
    }

    #[test]
    fn the_other_messages() {
        assert_eq!(SystemError::Missing.to_string(), "no system.img");
        assert_eq!(
            SystemError::NotLoaderMemory {
                phys: 0x10_0000,
                len: 5
            }
            .to_string(),
            "system.img at 0x100000 (5 bytes) is not loader memory"
        );
    }

    #[test]
    fn the_archive_must_lie_in_loader_memory() {
        use MemoryKind::*;
        let map = [
            region(0x1000, 0x8000, Usable),
            region(0x8000, 0x10000, Bootloader),
            region(0x10000, 0x20000, Bootloader),
            region(0x20000, 0x30000, Usable),
            region(0x40000, 0x50000, Bootloader),
        ];
        assert!(in_loader_memory(&map, 0x8000, 0x8000));
        assert!(
            in_loader_memory(&map, 0x9000, 0x10000),
            "across two loader regions"
        );
        assert!(in_loader_memory(&map, 0x40000, 0x10000));
        assert!(
            !in_loader_memory(&map, 0x7000, 0x2000),
            "starts in usable memory"
        );
        assert!(
            !in_loader_memory(&map, 0x1f000, 0x2000),
            "ends in usable memory"
        );
        assert!(!in_loader_memory(&map, 0x1f000, 0x30000), "spans a gap");
        assert!(!in_loader_memory(&map, 0x60000, 1), "outside the map");
        assert!(!in_loader_memory(&map, u64::MAX - 1, 0x10), "wraps around");
    }
}
