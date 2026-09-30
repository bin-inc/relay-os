//! The dispatcher's fake of a program (tests only): its memory, its
//! files and what it asked of the kernel.

use super::*;
use crate::fd::{FdTable, File};
use crate::mm::paging::{PAGE, PageTables, Perm};
use crate::mm::space::AddressSpace;
use crate::mm::testing::FakeMem;
use alloc::boxed::Box;
use relay_abi::decode;
use vfs::{Env, FileSystem, MemFs, MountTable};

pub const U: u64 = 0x40_0000;
/// The writable page.
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (the shell's output redirected to a
/// full disk).
pub const FULL: u64 = 7;
/// How much file data `/full` holds.
pub const FULL_BYTES: u64 = 8192;

/// What the fake clock says.
pub const NOW: Time = Time {
    unix_seconds: 1_790_000_000,
    uptime_ns: 12_345_678_901,
};

pub const MEM: MemInfo = MemInfo {
    ram_total: 16 << 30,
    ram_free: 15 << 30,
    heap_total: 32 << 20,
    heap_used: 1 << 20,
};

/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; the console as fd 0,
/// the in-kernel shell's outputs as fds 1 and 2 and one to a full disk as
/// `FULL`; the files `/root/f` ("hello", `/root` its current directory)
/// and `/full` (8 KiB of room); what it wrote to the console (as fd 0) or
/// the outputs (as 1 and 2), started, killed, and how long it slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
    pub fds: FdTable,
    pub vfs: MountTable,
    pub written: Vec<(u64, Vec<u8>)>,
    pub slept: Vec<u64>,
    pub spawned: Vec<Spawn>,
    /// Children that have ended, and whether any still runs.
    pub ended: Vec<(u32, WaitStatus)>,
    pub running: bool,
    pub killed: Vec<i64>,
}

impl Caller for Fake {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        slice.read(&self.space, &mut self.mem, offset, buf)
    }
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
        slice.write(&self.space, &mut self.mem, offset, bytes)
    }
    fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno> {
        // Writing what is there already writes nothing new.
        let mut old = alloc::vec![0; slice.len() as usize];
        slice.read(&self.space, &mut self.mem, 0, &mut old)?;
        slice.write(&self.space, &mut self.mem, 0, &old)
    }
    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno> {
        s.read(&self.space, &mut self.mem)
    }
    fn with_fds<R>(&mut self, f: impl FnOnce(&mut FdTable) -> R) -> R {
        f(&mut self.fds)
    }
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R {
        f(&mut self.vfs)
    }
    fn console_write(&mut self, bytes: &[u8]) {
        self.written.push((0, bytes.to_vec()));
    }
    fn shell_output(&mut self, _: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
        if u64::from(n) == FULL {
            return Err(Errno::ENOSPC);
        }
        self.written.push((u64::from(n), bytes.to_vec()));
        Ok(())
    }
    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno> {
        if s.path == b"missing" {
            return Err(Errno::ENOENT);
        }
        self.spawned.push(s.clone());
        Ok(100 + self.spawned.len() as u32)
    }
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
        let at = self
            .ended
            .iter()
            .position(|&(pid, _)| child == Child::Any || child == Child::Pid(pid));
        match at {
            Some(i) => Ok(Some(self.ended.remove(i))),
            None if self.running && nohang => Ok(None),
            None => Err(Errno::ECHILD),
        }
    }
    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        self.killed.push(target);
        if target == 1 {
            Err(Errno::EPERM)
        } else {
            Ok(())
        }
    }
    fn pid(&self) -> u32 {
        42
    }
    fn memory(&self) -> MemInfo {
        MEM
    }
    fn time(&self) -> Time {
        NOW
    }
    fn sleep(&mut self, ms: u64) {
        self.slept.push(ms);
    }
}

pub fn fake() -> Fake {
    let mut mem = FakeMem::new();
    let mut k = PageTables::new(&mut mem).unwrap();
    k.fill_upper_half(&mut mem).unwrap();
    let mut space = AddressSpace::new(&mut mem, &k).unwrap();
    space.map_zeroed(&mut mem, U, 3, Perm::Read).unwrap();
    let pattern: Vec<u8> = (0..3 * PAGE).map(|i| (i % 251) as u8).collect();
    space.fill(&mut mem, U, &pattern).unwrap();
    space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
    let mut fds = FdTable::shell();
    fds.set(FULL as usize, Arc::new(File::ShellOutput(FULL as u32)));
    Fake {
        mem,
        space,
        fds,
        vfs: files(),
        written: Vec::new(),
        slept: Vec::new(),
        spawned: Vec::new(),
        ended: Vec::new(),
        running: false,
        killed: Vec::new(),
    }
}

pub fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
    match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
        Outcome::Return(r) => decode(r),
        Outcome::Exit(code) => panic!("exited with {code}"),
    }
}

/// Everything written, joined, per fd.
pub fn text(f: &Fake, fd: u64) -> Vec<u8> {
    f.written
        .iter()
        .filter(|(d, _)| *d == fd)
        .flat_map(|(_, b)| b.clone())
        .collect()
}

/// Puts `bytes` into the writable page at `at`, as the program would.
pub fn put(f: &mut Fake, at: u64, bytes: &[u8]) {
    UserSlice::new(at, bytes.len() as u64)
        .unwrap()
        .write(&f.space, &mut f.mem, 0, bytes)
        .unwrap();
}

pub fn get(f: &mut Fake, at: u64, len: usize) -> Vec<u8> {
    let mut buf = alloc::vec![0; len];
    UserSlice::new(at, len as u64)
        .unwrap()
        .read(&f.space, &mut f.mem, 0, &mut buf)
        .unwrap();
    buf
}

struct Clock;

impl Env for Clock {
    fn now(&self) -> u64 {
        1_000
    }
    fn log(&self, _: &str) {}
}

/// `/root/f` holding "hello", with `/root` the current directory, and a
/// filesystem at `/full` with room for `FULL_BYTES`.
fn files() -> MountTable {
    let mut fs = MemFs::new(Box::new(Clock));
    let root = fs.root();
    let home = fs.mkdir(root, b"root").unwrap();
    let f = fs.create(home, b"f").unwrap();
    fs.write_at(f, 0, b"hello").unwrap();
    let mut t = MountTable::new(Box::new(fs));
    let full = MemFs::new(Box::new(Clock)).with_capacity(FULL_BYTES);
    t.mount(b"/full", Box::new(full)).unwrap();
    t.chdir(b"/root").unwrap();
    t
}
