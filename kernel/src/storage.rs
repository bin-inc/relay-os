//! Startup step 9 (spec §4.4, §6.5, §10): the root filesystem. The GPT of
//! every USB disk is read, the root partition chosen by the boot
//! partition's GUID, and ext2 mounted on it; if the read-write mount fails
//! with an I/O error (a worn-out stick often turns read-only) it is mounted
//! read-only, and only if that fails too does the shell get the empty
//! read-only `/`.

use crate::block::gpt::{self, Guid};
use crate::block::partition::Partition;
use crate::block::root::{self, RootChoice};
use crate::session::KernelEnv;
use crate::usb::{self, UsbDisk};
use crate::{console, klogln, kprintln};
use ::usb::host::Size;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use ext2::{Ext2, MountOptions};
use vfs::{BlockDevice, Env, Errno, FileSystem, MemFs};

/// Mounts ext2 on the device `open` returns: read-write, and if that fails
/// with `EIO`, read-only on a fresh device. Returns the filesystem and, if
/// it is read-only, the error of the read-write attempt.
pub fn mount_ext2<D: BlockDevice>(
    mut open: impl FnMut() -> Result<D, Errno>,
    env: impl Fn() -> Box<dyn Env>,
) -> Result<(Ext2<D>, Option<Errno>), Errno> {
    match Ext2::mount(open()?, env(), MountOptions::default()) {
        Ok(fs) => Ok((fs, None)),
        Err(Errno::EIO) => {
            let opts = MountOptions {
                read_only: true,
                ..MountOptions::default()
            };
            Ext2::mount(open()?, env(), opts).map(|fs| (fs, Some(Errno::EIO)))
        }
        Err(e) => Err(e),
    }
}

/// What the `mount /` status line says about the root: `ext2 on 00:02.0
/// port 2 partition 2, 190 MiB`, the size as on the disk's boot line.
pub fn root_text(disk: &str, number: u32, bytes: u64) -> String {
    format!("ext2 on {disk} partition {number}, {}", Size(bytes))
}

/// How the root disk was chosen (spec §6.5): a log line when it has the
/// boot partition, a warning for the screen when it is the fallback.
pub fn choice_text(disk: &str, fallback: bool, boot: Option<Guid>) -> String {
    const FALLBACK: &str = "the first disk with one ESP and one Linux partition";
    match (fallback, boot.filter(|g| !g.is_zero())) {
        (false, Some(g)) => {
            format!("storage: root on {disk}, the disk with the boot partition {g}")
        }
        (false, None) => format!("storage: root on {disk}"),
        (true, Some(g)) => format!(
            "mount /: warning: no disk has the boot partition {g}; using {disk}, {FALLBACK}"
        ),
        (true, None) => format!(
            "mount /: warning: the loader did not name the boot partition; using {disk}, {FALLBACK}"
        ),
    }
}

/// Startup step 9: the root filesystem, or the empty read-only `/` of spec
/// §10 if there is none. Prints the `mount /` status line.
pub fn mount_root(boot: Option<[u8; 16]>) -> Box<dyn FileSystem> {
    match try_mount_root(boot.map(Guid)) {
        Ok((fs, what, None)) => {
            console::ok(format_args!("mount /: {what}"));
            fs
        }
        Ok((fs, what, Some(e))) => {
            console::fail("mount /", format_args!("{e}; mounted read-only, {what}"));
            fs
        }
        Err(why) => {
            console::fail("mount /", format_args!("{why}"));
            Box::new(MemFs::new(Box::new(KernelEnv)).read_only())
        }
    }
}

type Mounted = (Box<dyn FileSystem>, String, Option<Errno>);

fn try_mount_root(boot: Option<Guid>) -> Result<Mounted, String> {
    let mut disks = usb::disks();
    if disks.is_empty() {
        return Err(String::from("no USB disk"));
    }
    let gpts: Vec<_> = disks
        .iter_mut()
        .map(|d| match gpt::read_gpt(d) {
            Ok(g) => {
                if g.used_backup {
                    kprintln!("mount /: {}: primary GPT damaged, using the backup", d.name);
                }
                klogln!(
                    "storage: {}: GPT with {} partitions",
                    d.name,
                    g.partitions.len()
                );
                Some(g)
            }
            Err(e) => {
                klogln!("storage: {}: {e}", d.name);
                None
            }
        })
        .collect();
    let RootChoice {
        disk,
        partition,
        fallback,
    } = root::choose_root(&gpts, boot).map_err(|e| format!("{e}"))?;
    let dev: UsbDisk = disks.swap_remove(disk);
    let how = choice_text(&dev.name, fallback, boot);
    if fallback {
        kprintln!("{how}");
    } else {
        klogln!("{how}");
    }
    let bytes =
        (partition.last_lba - partition.first_lba + 1).saturating_mul(dev.block_size() as u64);
    let what = root_text(&dev.name, partition.number, bytes);
    let open = || {
        Partition::new(dev.clone(), partition.first_lba, partition.last_lba).map_err(|_| Errno::EIO)
    };
    let env = || Box::new(KernelEnv) as Box<dyn Env>;
    match mount_ext2(open, env) {
        Ok((fs, ro)) => Ok((Box::new(fs), what, ro)),
        Err(e) => Err(format!("{what}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::{MemDisk, mke2fs_image};
    use alloc::rc::Rc;
    use core::cell::RefCell;
    use vfs::IoError;

    /// A disk the test keeps a handle to while the filesystem uses it.
    #[derive(Clone)]
    struct Shared(Rc<RefCell<MemDisk>>);

    impl BlockDevice for Shared {
        fn block_size(&self) -> usize {
            self.0.borrow().block_size()
        }
        fn block_count(&self) -> u64 {
            self.0.borrow().block_count()
        }
        fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
            self.0.borrow_mut().read(lba, buf)
        }
        fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
            self.0.borrow_mut().write(lba, buf)
        }
        fn flush(&mut self) -> Result<(), IoError> {
            self.0.borrow_mut().flush()
        }
    }

    struct Quiet;

    impl Env for Quiet {
        fn now(&self) -> u64 {
            1_750_000_000
        }
        fn log(&self, _: &str) {}
    }

    fn quiet() -> Box<dyn Env> {
        Box::new(Quiet)
    }

    fn disk(data: Vec<u8>) -> (Shared, Rc<core::cell::Cell<u32>>) {
        (
            Shared(Rc::new(RefCell::new(MemDisk::new(data, 512)))),
            Rc::new(core::cell::Cell::new(0)),
        )
    }

    fn opener(
        d: &Shared,
        opens: &Rc<core::cell::Cell<u32>>,
    ) -> impl FnMut() -> Result<Shared, Errno> {
        let (d, opens) = (d.clone(), opens.clone());
        move || {
            opens.set(opens.get() + 1);
            Ok(d.clone())
        }
    }

    #[test]
    fn a_good_filesystem_is_mounted_read_write() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        let (mut fs, ro) = mount_ext2(opener(&d, &opens), quiet).unwrap();
        assert_eq!(ro, None);
        assert_eq!(opens.get(), 1);
        let root = fs.root();
        fs.create(root, b"new").unwrap();
        fs.shutdown().unwrap();
    }

    #[test]
    fn a_disk_that_refuses_writes_is_mounted_read_only() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        d.0.borrow_mut().read_only = true;
        let (mut fs, ro) = mount_ext2(opener(&d, &opens), quiet).unwrap();
        assert_eq!(ro, Some(Errno::EIO));
        assert_eq!(opens.get(), 2, "a fresh device for the second try");
        let root = fs.root();
        assert_eq!(fs.create(root, b"new"), Err(Errno::EROFS));
        assert!(fs.lookup(root, b"lost+found").is_ok(), "files can be read");
    }

    #[test]
    fn what_is_not_ext2_is_not_tried_again() {
        let (d, opens) = disk(vec![0; 8 << 20]);
        assert_eq!(
            mount_ext2(opener(&d, &opens), quiet).err(),
            Some(Errno::EINVAL)
        );
        assert_eq!(opens.get(), 1);
    }

    #[test]
    fn a_disk_that_cannot_be_read_fails_both_tries() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        d.0.borrow_mut().bad = 0..u64::MAX;
        assert_eq!(
            mount_ext2(opener(&d, &opens), quiet).err(),
            Some(Errno::EIO)
        );
        assert_eq!(opens.get(), 2);
    }

    #[test]
    fn how_the_root_was_chosen_is_said() {
        let esp = crate::block::gpt::Guid::from_fields(
            0x5245_4C41,
            0x5900,
            0x4000,
            [0x80, 0, 0, 0, 0, 0, 0, 0x02],
        );
        assert_eq!(
            choice_text("00:02.0 port 2", false, Some(esp)),
            "storage: root on 00:02.0 port 2, the disk with the boot partition 52454C41-5900-4000-8000-000000000002"
        );
        assert_eq!(
            choice_text("00:14.0 port 15", true, Some(esp)),
            "mount /: warning: no disk has the boot partition 52454C41-5900-4000-8000-000000000002; using 00:14.0 port 15, the first disk with one ESP and one Linux partition"
        );
        assert_eq!(
            choice_text("00:14.0 port 15", true, None),
            "mount /: warning: the loader did not name the boot partition; using 00:14.0 port 15, the first disk with one ESP and one Linux partition"
        );
    }

    #[test]
    fn the_status_line_names_the_disk_and_partition() {
        assert_eq!(
            root_text("00:02.0 port 2", 2, 190 << 20),
            "ext2 on 00:02.0 port 2 partition 2, 190 MiB"
        );
        // The size is the boot line's (`usb::host::Size`, tested there).
        assert_eq!(
            root_text("00:14.0 port 15", 2, u64::MAX),
            "ext2 on 00:14.0 port 15 partition 2, 17179869183.9 GiB"
        );
    }
}
