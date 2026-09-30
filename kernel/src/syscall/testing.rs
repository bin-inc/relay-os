//! The dispatcher's fake of a program (tests only): its memory, its
//! files and what it asked of the kernel.

use super::*;
use crate::mm::paging::{PAGE, PageTables, Perm};
use crate::mm::space::AddressSpace;
use crate::mm::testing::FakeMem;
use relay_abi::decode;

pub const U: u64 = 0x40_0000;
/// The writable page.
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (a full disk).
pub const FULL: u64 = 7;

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
/// writable page after them and nothing after that; fds 0-2 and a full
/// disk as `FULL`; what it wrote, started, killed, and how long it
/// slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
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
    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno> {
        match fd {
            0..=2 | FULL => Ok(()),
            _ => Err(Errno::EBADF),
        }
    }
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
        if fd == FULL {
            return Err(Errno::ENOSPC);
        }
        self.written.push((fd, bytes.to_vec()));
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
    Fake {
        mem,
        space,
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
