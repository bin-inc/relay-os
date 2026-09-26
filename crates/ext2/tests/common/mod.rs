//! Test support: images made by e2fsprogs, a file-backed block device, the
//! e2fsprogs checkers and an environment with a settable clock and a
//! recorded log.
#![allow(dead_code)] // each test file uses a different part

use ext2::{Ext2, MountOptions};
use std::cell::{Cell, RefCell};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use vfs::{BlockDevice, Env, IoError};

/// The clock tests start at. It lies in the past, so e2fsck never finds
/// times in the future.
pub const T0: u64 = 1_750_000_000;

/// A fresh path `<target tmpdir>/ext2/<name>-<n><ext>`, unique within the
/// test binary.
pub fn scratch(name: &str, ext: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ext2");
    fs::create_dir_all(&dir).unwrap();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{name}-{n}{ext}"));
    if path.is_dir() {
        fs::remove_dir_all(&path).unwrap();
    } else if path.exists() {
        fs::remove_file(&path).unwrap();
    }
    path
}

/// An empty staging directory for `mke2fs -d`.
pub fn staging(name: &str) -> PathBuf {
    let dir = scratch(name, ".d");
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// An e2fsprogs command. Missing tools fail the test: these checks are the
/// point of the tests, so they are never skipped.
pub fn tool(name: &str) -> Command {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain(["/usr/sbin".into(), "/sbin".into()]);
    for dir in dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let mut cmd = Command::new(candidate);
            cmd.env("DEBUGFS_PAGER", "__none__")
                .env("PAGER", "__none__");
            return cmd;
        }
    }
    panic!("install e2fsprogs: {name} not found");
}

fn run(mut cmd: Command) -> Output {
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("running {cmd:?}: {e}"));
    if !out.status.success() {
        panic!(
            "{cmd:?} failed ({}):\n{}{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    out
}

/// A fresh image of `size_kib` made with `mke2fs -q -F -t ext2 -b
/// <block_size> <extra…>`, populated from `staging` if given.
pub fn mkfs(
    name: &str,
    block_size: u32,
    size_kib: u64,
    extra: &[&str],
    staging: Option<&Path>,
) -> PathBuf {
    let path = scratch(name, ".img");
    File::create(&path)
        .unwrap()
        .set_len(size_kib * 1024)
        .unwrap();
    let mut cmd = tool("mke2fs");
    cmd.args(["-q", "-F", "-t", "ext2", "-b", &block_size.to_string()]);
    cmd.args(extra);
    if let Some(dir) = staging {
        cmd.arg("-d").arg(dir);
    }
    cmd.arg(&path);
    run(cmd);
    path
}

/// `e2fsck -fn`: panics with the full output unless the filesystem is
/// clean. With `-n` some problems (a wrong free count in the superblock)
/// still exit 0, so any declined fix counts as a failure too.
pub fn fsck(path: &Path) {
    let out = tool("e2fsck").arg("-fn").arg(path).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() || text.contains("? no") {
        panic!("e2fsck -fn {} ({}):\n{text}", path.display(), out.status);
    }
}

/// `debugfs -R <cmd>`: its standard output.
pub fn debugfs(path: &Path, cmd: &str) -> String {
    let out = run({
        let mut c = tool("debugfs");
        c.arg("-R").arg(cmd).arg(path);
        c
    });
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `debugfs -w -R <cmd>`; panics if debugfs reports anything but its
/// banner.
pub fn debugfs_w(path: &Path, cmd: &str) {
    let out = run({
        let mut c = tool("debugfs");
        c.arg("-w").arg("-R").arg(cmd).arg(path);
        c
    });
    let err = String::from_utf8_lossy(&out.stderr);
    let extra: Vec<_> = err.lines().filter(|l| !l.starts_with("debugfs ")).collect();
    assert!(extra.is_empty(), "debugfs -w -R {cmd:?}: {err}");
}

/// `dumpe2fs -h`.
pub fn dumpe2fs_h(path: &Path) -> String {
    let out = run({
        let mut c = tool("dumpe2fs");
        c.arg("-h").arg(path);
        c
    });
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The value of a `Key: value` line (as `dumpe2fs -h` prints them).
pub fn field(text: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    text.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no {key:?} in:\n{text}"))
        .trim()
        .to_string()
}

/// The inode number of `path` inside the image.
pub fn ino_of(img: &Path, path: &str) -> u64 {
    let out = debugfs(img, &format!("stat \"{path}\""));
    let rest = out
        .split("Inode: ")
        .nth(1)
        .unwrap_or_else(|| panic!("no inode for {path}: {out}"));
    rest.split_whitespace().next().unwrap().parse().unwrap()
}

/// What the fake device records and can be told to fail.
#[derive(Default)]
pub struct Faults {
    pub fail_writes: Cell<bool>,
    /// Device writes and flushes so far.
    pub writes: Cell<u64>,
    pub flushes: Cell<u64>,
}

/// A block device over an image file.
pub struct FileDisk {
    file: File,
    block_size: usize,
    blocks: u64,
    pub faults: Rc<Faults>,
}

impl FileDisk {
    /// 512-byte blocks, like the USB stick.
    pub fn open(path: &Path) -> FileDisk {
        FileDisk::with_block_size(path, 512)
    }

    pub fn with_block_size(path: &Path, block_size: usize) -> FileDisk {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let blocks = file.metadata().unwrap().len() / block_size as u64;
        FileDisk {
            file,
            block_size,
            blocks,
            faults: Rc::default(),
        }
    }
}

impl BlockDevice for FileDisk {
    fn block_size(&self) -> usize {
        self.block_size
    }
    fn block_count(&self) -> u64 {
        self.blocks
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        vfs::check_request(self.block_size, self.blocks, lba, buf.len())?;
        self.file
            .read_exact_at(buf, lba * self.block_size as u64)
            .map_err(|_| IoError::Device)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        vfs::check_request(self.block_size, self.blocks, lba, buf.len())?;
        if self.faults.fail_writes.get() {
            return Err(IoError::Device);
        }
        self.faults.writes.set(self.faults.writes.get() + 1);
        self.file
            .write_all_at(buf, lba * self.block_size as u64)
            .map_err(|_| IoError::Device)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.faults.flushes.set(self.faults.flushes.get() + 1);
        self.file.sync_data().map_err(|_| IoError::Device)
    }
}

/// An environment with a settable clock that records the log.
pub struct TestEnv {
    now: Cell<u64>,
    lines: RefCell<Vec<String>>,
}

impl TestEnv {
    pub fn new() -> Rc<TestEnv> {
        Rc::new(TestEnv {
            now: Cell::new(T0),
            lines: RefCell::new(Vec::new()),
        })
    }

    pub fn set_now(&self, t: u64) {
        self.now.set(t);
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.borrow().clone()
    }

    /// Whether a logged line contains `text`.
    pub fn logged(&self, text: &str) -> bool {
        self.lines.borrow().iter().any(|l| l.contains(text))
    }
}

impl Env for TestEnv {
    fn now(&self) -> u64 {
        self.now.get()
    }
    fn log(&self, line: &str) {
        assert!(line.starts_with("ext2: "), "log line {line:?}");
        self.lines.borrow_mut().push(line.to_string());
    }
}

pub fn ro() -> MountOptions {
    MountOptions {
        read_only: true,
        ..MountOptions::default()
    }
}

pub fn rw() -> MountOptions {
    MountOptions::default()
}

/// Mounts the image at `path`.
pub fn mount(path: &Path, opts: MountOptions) -> (Ext2<FileDisk>, Rc<TestEnv>) {
    let env = TestEnv::new();
    match Ext2::mount(FileDisk::open(path), Box::new(env.clone()), opts) {
        Ok(fs) => (fs, env),
        Err(e) => panic!("mount {}: {e:?}, log: {:?}", path.display(), env.lines()),
    }
}

/// Tries to mount; the result and the log.
pub fn try_mount(
    dev: FileDisk,
    opts: MountOptions,
) -> (Result<Ext2<FileDisk>, vfs::Errno>, Rc<TestEnv>) {
    let env = TestEnv::new();
    (Ext2::mount(dev, Box::new(env.clone()), opts), env)
}

/// Overwrites bytes of the image at `offset`.
pub fn poke(path: &Path, offset: u64, bytes: &[u8]) {
    let f = OpenOptions::new().write(true).open(path).unwrap();
    f.write_all_at(bytes, offset).unwrap();
}

/// Reads bytes of the image at `offset`.
pub fn peek(path: &Path, offset: u64, len: usize) -> Vec<u8> {
    let f = File::open(path).unwrap();
    let mut buf = vec![0; len];
    f.read_exact_at(&mut buf, offset).unwrap();
    buf
}

/// The fields of `debugfs -R "stat <path>"`.
#[derive(Debug)]
pub struct DebugfsStat {
    pub ino: u64,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub links: u32,
    pub blockcount: u64,
    pub mtime: u64,
    /// The text, for other fields.
    pub text: String,
}

/// `debugfs -R "stat <path>"`, parsed.
pub fn debugfs_stat(img: &Path, path: &str) -> DebugfsStat {
    let text = debugfs(img, &format!("stat \"{path}\""));
    // The first value after `key:` on any line.
    let value = |key: &str| -> String {
        let at = text
            .find(&format!("{key}:"))
            .unwrap_or_else(|| panic!("no {key} for {path}: {text}"));
        let rest = &text[at + key.len() + 1..];
        rest.split_whitespace().next().unwrap().to_string()
    };
    let number = |key: &str| -> u64 { value(key).parse().unwrap() };
    let mtime = value("mtime");
    let mtime = mtime.trim_start_matches("0x");
    let mtime = mtime.split(':').next().unwrap();
    DebugfsStat {
        ino: number("Inode"),
        mode: u16::from_str_radix(&value("Mode"), 8).unwrap(),
        uid: number("User") as u32,
        gid: number("Group") as u32,
        size: number("Size"),
        links: number("Links") as u32,
        blockcount: number("Blockcount"),
        mtime: u64::from_str_radix(mtime, 16).unwrap(),
        text,
    }
}
