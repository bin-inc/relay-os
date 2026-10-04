//! `cargo xtask host-shell <img>`: the Relay shell on the host, over the
//! ext2 root partition of an image file (spec §9.1), through a file-backed
//! `BlockDevice`, with the kernel's `/dev` mounted over the image's. For
//! trying the filesystem and the shell's commands without a machine: the
//! in-process runner runs the command functions (no programs, no `&`), its
//! environment init's `HOME=/root`. It changes the image in place.

use crate::image;
use crate::util::run_stdout;
use anyhow::{Context, Result, anyhow, bail};
use ext2::{Ext2, MountOptions};
use shell::{Console, MemInfo, Shell, System};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use vfs::{BlockDevice, DevFs, Env, IoError, MountTable, Vfs, check_request};

const SECTOR: usize = 512;

/// A partition of an image file, as a device of 512-byte sectors.
pub struct FileDisk {
    file: File,
    offset: u64,
    sectors: u64,
}

impl FileDisk {
    pub fn new(file: File, offset: u64, sectors: u64) -> FileDisk {
        FileDisk {
            file,
            offset,
            sectors,
        }
    }

    fn position(&self, lba: u64, len: usize) -> Result<u64, IoError> {
        check_request(SECTOR, self.sectors, lba, len)?;
        Ok(self.offset + lba * SECTOR as u64)
    }
}

impl BlockDevice for FileDisk {
    fn block_size(&self) -> usize {
        SECTOR
    }
    fn block_count(&self) -> u64 {
        self.sectors
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let at = self.position(lba, buf.len())?;
        self.file
            .read_exact_at(buf, at)
            .map_err(|_| IoError::Device)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        let at = self.position(lba, buf.len())?;
        self.file.write_all_at(buf, at).map_err(|_| IoError::Device)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.file.sync_data().map_err(|_| IoError::Device)
    }
}

/// The session's kernel log: what ext2 reports, shown by `dmesg`.
#[derive(Default)]
struct Log(RefCell<Vec<u8>>);

impl Log {
    fn contents(&self) -> Vec<u8> {
        self.0.borrow().clone()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

struct HostEnv(Rc<Log>);

impl Env for HostEnv {
    fn now(&self) -> u64 {
        now()
    }
    fn log(&self, line: &str) {
        let mut log = self.0.0.borrow_mut();
        log.extend_from_slice(line.as_bytes());
        log.push(b'\n');
    }
}

/// The host as the shell's `System`: the host clock, no memory figures,
/// and `reboot`/`poweroff` simply end the session.
struct HostSystem(Rc<Log>);

impl System for HostSystem {
    fn now(&self) -> u64 {
        now()
    }
    fn memory(&self) -> Option<MemInfo> {
        None
    }
    fn kernel_log(&self) -> Vec<u8> {
        self.0.contents()
    }
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        None
    }
    fn sleep(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    fn reboot(&mut self, _force: bool) -> Result<(), vfs::Errno> {
        Ok(())
    }
    fn poweroff(&mut self, _force: bool) -> Result<(), vfs::Errno> {
        Ok(())
    }
}

/// Standard input and output.
struct Terminal {
    columns: usize,
}

impl Console for Terminal {
    fn read_byte(&mut self) -> Option<u8> {
        let mut byte = [0u8];
        match std::io::stdin().lock().read(&mut byte) {
            Ok(1) => Some(byte[0]),
            _ => None,
        }
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

fn stty(args: &[&str]) -> Result<String> {
    run_stdout(Command::new("stty").args(args).stdin(Stdio::inherit()))
}

/// Raw keyboard input while alive, like the kernel's console: keys arrive
/// one by one, unechoed, Ctrl-C as a byte. Output processing stays on, so
/// `\n` still starts a new line. Does nothing when stdin is not a terminal.
struct RawMode(Option<String>);

impl RawMode {
    fn enter() -> Result<RawMode> {
        if !std::io::stdin().is_terminal() {
            return Ok(RawMode(None));
        }
        let saved = stty(&["-g"])?.trim().to_string();
        // xtask builds with `panic = "abort"`, so `Drop` never runs after a
        // panic; the hook restores the terminal before the abort.
        let restore = saved.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = stty(&[&restore]);
            previous(info);
        }));
        stty(&[
            "-icanon", "-echo", "-isig", "-ixon", "-icrnl", "min", "1", "time", "0",
        ])?;
        Ok(RawMode(Some(saved)))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let Some(saved) = &self.0 {
            let _ = stty(&[saved]);
        }
    }
}

/// The terminal's width, 80 if it cannot be found.
fn terminal_columns() -> usize {
    stty(&["size"])
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse().ok())
        .unwrap_or(80)
}

/// Whether partition `p` lies inside a file of `len` bytes.
fn fits(p: image::Partition, len: u64) -> bool {
    p.offset()
        .checked_add(p.bytes())
        .is_some_and(|end| end <= len)
}

/// Runs one shell session on the ext2 partition of `img` until `reboot`,
/// `poweroff` or the end of input, then shuts the filesystem down cleanly.
pub fn session(img: &Path, console: &mut dyn Console) -> Result<()> {
    // Only image files: a device node (such as the test stick) could be
    // mounted by the host at the same time.
    let meta = std::fs::metadata(img).with_context(|| format!("opening {}", img.display()))?;
    if !meta.is_file() {
        bail!(
            "{} is not a regular file; host-shell works on image files",
            img.display()
        );
    }
    let layout = image::read_layout(img)?;
    if !fits(layout.root, meta.len()) {
        bail!(
            "the root partition of {} ends past the end of the file",
            img.display()
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(img)
        .with_context(|| format!("opening {}", img.display()))?;
    let disk = FileDisk::new(file, layout.root.offset(), layout.root.sectors);
    let log = Rc::new(Log::default());
    let fs = Ext2::mount(
        disk,
        Box::new(HostEnv(log.clone())),
        MountOptions::default(),
    )
    .map_err(|e| {
        let details = String::from_utf8_lossy(&log.contents()).into_owned();
        anyhow!(
            "cannot mount the ext2 partition of {}: {e}\n{details}",
            img.display()
        )
    })?;
    // Mount warnings (such as a read-only fallback) are the boot messages.
    console.write(&log.contents());
    let mut vfs = MountTable::new(Box::new(fs));
    // The kernel's /dev (programmable shell gate §8.4), over the image's.
    if let Err(e) = vfs.mount(b"/dev", Box::new(DevFs::new(now()))) {
        console.write(format!("cannot mount /dev: {e}\n").as_bytes());
    }
    let mut system = HostSystem(log);
    // As init starts /bin/sh (programmable shell gate §8.5).
    let mut shell = Shell::new(&mut vfs, console, &mut system).with_environment(b"HOME=/root\0");
    shell.greet();
    shell.run();
    vfs.shutdown()
        .map_err(|e| anyhow!("cannot unmount {} cleanly: {e}", img.display()))
}

pub fn run(img: &Path) -> Result<()> {
    eprintln!(
        "host-shell: {} (ext2 root partition). Type `exit` to leave.",
        img.display()
    );
    let mut terminal = Terminal {
        columns: terminal_columns(),
    };
    let _raw = RawMode::enter()?;
    session(img, &mut terminal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IMAGE_BYTES;
    use crate::util::out_dir;
    use std::collections::VecDeque;

    /// Types a script and records the screen.
    struct Script {
        input: VecDeque<u8>,
        output: Vec<u8>,
    }

    impl Console for Script {
        fn read_byte(&mut self) -> Option<u8> {
            self.input.pop_front()
        }
        fn write(&mut self, bytes: &[u8]) {
            self.output.extend_from_slice(bytes);
        }
        fn columns(&self) -> usize {
            80
        }
    }

    #[test]
    fn a_session_changes_the_image_and_leaves_it_clean() {
        let dir = out_dir().join("host-shell-test");
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("disk.img");
        let _ = std::fs::remove_file(&img);
        File::create(&img).unwrap().set_len(IMAGE_BYTES).unwrap();
        image::partition(&img, true, None).unwrap();
        let layout = image::read_layout(&img).unwrap();
        let staging = image::stage_rootfs().unwrap();
        image::make_ext2(&img, layout.root, &staging).unwrap();

        let mut console = Script {
            input: b"mkdir /root/notes\recho hello > /root/notes/a\rcat /root/notes/a\r\
                echo gone > /dev/null\r[ -c /dev/null ] && echo device\recho $HOME\rpoweroff\r"
                .iter()
                .copied()
                .collect(),
            output: Vec::new(),
        };
        session(&img, &mut console).unwrap();
        let screen = String::from_utf8_lossy(&console.output);
        // The motd starts with a blank line.
        assert!(screen.starts_with("\nWelcome to Relay OS.\n"), "{screen:?}");
        assert!(
            screen.contains("root@relay:~# cat /root/notes/a\nhello\n"),
            "{screen}"
        );
        // /dev/null is the kernel's DevFs here too, not a file of the image.
        assert!(screen.contains("&& echo device\ndevice\n"), "{screen}");
        // HOME is init's.
        assert!(screen.contains("# echo $HOME\n/root\n"), "{screen}");

        image::fsck(&img, layout.root).unwrap();
        let target = image::e2fs_target(&img, layout.root);
        let cat =
            run_stdout(Command::new("debugfs").args(["-R", "cat /root/notes/a", &target])).unwrap();
        assert_eq!(cat, "hello\n");
        let ls = run_stdout(Command::new("debugfs").args(["-R", "ls /dev", &target])).unwrap();
        assert!(!ls.contains("null"), "{ls}");
        let header = run_stdout(Command::new("dumpe2fs").args(["-h", &target])).unwrap();
        assert!(
            header.contains("Filesystem state:         clean"),
            "{header}"
        );
    }

    #[test]
    fn only_whole_image_files_are_accepted() {
        let mut console = Script {
            input: VecDeque::new(),
            output: Vec::new(),
        };
        let err = session(Path::new("/dev/null"), &mut console).unwrap_err();
        assert!(err.to_string().contains("not a regular file"), "{err:#}");
        // A partition table describing more than the file holds.
        let p = image::Partition {
            start_lba: 2048,
            sectors: 4096,
        };
        assert!(fits(p, (2048 + 4096) * 512));
        assert!(!fits(p, (2048 + 4096) * 512 - 1));
    }

    #[test]
    fn a_file_disk_refuses_requests_outside_its_partition() {
        let dir = out_dir().join("host-shell-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("small.img");
        let file = File::create(&path).unwrap();
        file.set_len(4096).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut disk = FileDisk::new(file, 1024, 4);
        disk.write(3, &[7u8; 512]).unwrap();
        let mut buf = [0u8; 512];
        disk.read(3, &mut buf).unwrap();
        assert_eq!(buf, [7u8; 512]);
        assert_eq!(disk.read(4, &mut buf), Err(IoError::OutOfRange));
        assert_eq!(disk.write(3, &[0u8; 1024]), Err(IoError::OutOfRange));
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes[1024 + 3 * 512..1024 + 4 * 512]
                .iter()
                .all(|&b| b == 7)
        );
    }
}
