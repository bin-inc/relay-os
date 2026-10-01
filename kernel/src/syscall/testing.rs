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
/// What the fake kernel log holds.
pub const FAKE_LOG: &[u8] = b"Relay OS 0.2.0\n[ ok ] everything\n";
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
/// writable page after them and nothing after that; the console as fds 0,
/// 1 and 2; the files `/root/f` ("hello", `/root` its current directory)
/// and `/full` (8 KiB of room); what it wrote to the console, started,
/// killed, and how long it slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
    pub fds: FdTable,
    pub vfs: MountTable,
    /// What `heap_room` says.
    pub heap_room: usize,
    /// Frames `mem_map` may take.
    pub room: u64,
    /// What `mem_unmap` gave back (and the kernel would flush).
    pub unmapped: Vec<(u64, u64)>,
    /// What each console read gets, then end of input; the lengths asked;
    /// whether a read ends in a kill.
    pub typed: alloc::collections::VecDeque<Vec<u8>>,
    pub asked: Vec<usize>,
    pub killed_while_reading: bool,
    /// The console's mode and foreground group (groups 1 and 42 exist).
    pub line_mode: bool,
    pub foreground: u32,
    /// The tees pushed, and the syncs.
    pub tees: Vec<Arc<File>>,
    pub syncs: u32,
    /// The `power` calls: (reboot, force).
    pub powered: Vec<(bool, bool)>,
    /// Each write to the console.
    pub written: Vec<Vec<u8>>,
    pub slept: Vec<u64>,
    pub spawned: Vec<Spawn>,
    /// Children that have ended, and whether any still runs.
    pub ended: Vec<(u32, WaitStatus)>,
    pub running: bool,
    pub killed: Vec<i64>,
    /// The pipes made, whether there is memory for another, the pipes
    /// waited on (each wait ends as a kill would: `EINTR`) and woken.
    pub pipes_made: u32,
    pub no_pipe_memory: bool,
    pub waits: Vec<u64>,
    pub woken: Vec<u64>,
    /// What `proc_list` reports.
    pub procs: Vec<ProcInfo>,
}

/// A pipe's ring on the heap.
struct Ring(Box<[u8; crate::pipe::SIZE]>);

impl crate::pipe::Memory for Ring {
    fn bytes(&mut self) -> &mut [u8; crate::pipe::SIZE] {
        &mut self.0
    }
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
    fn heap_room(&self) -> usize {
        self.heap_room
    }
    fn mem_map(&mut self, pages: u64) -> Result<u64, Errno> {
        self.space.map_area(&mut self.mem, pages, self.room)
    }
    fn mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno> {
        self.space.unmap_area(&mut self.mem, addr, pages)?;
        self.unmapped.push((addr, pages));
        Ok(())
    }
    fn console_write(&mut self, bytes: &[u8]) {
        self.written.push(bytes.to_vec());
    }
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        if self.killed_while_reading {
            return Err(Errno::EINTR);
        }
        self.asked.push(buf.len());
        let Some(line) = self.typed.pop_front() else {
            return Ok(0);
        };
        let n = line.len().min(buf.len());
        buf[..n].copy_from_slice(&line[..n]);
        Ok(n)
    }
    fn console_mode(&mut self, line: bool) -> bool {
        core::mem::replace(&mut self.line_mode, line)
    }
    fn console_size(&self) -> (u32, u32) {
        (120, 33)
    }
    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno> {
        self.tees.push(file);
        Ok(())
    }
    fn tee_pop(&mut self) -> Result<(), Errno> {
        self.tees.pop().map(|_| ()).ok_or(Errno::EINVAL)
    }
    fn kernel_log(&self) -> Vec<u8> {
        FAKE_LOG.to_vec()
    }
    fn new_pipe(&mut self) -> Result<(crate::pipe::End, crate::pipe::End), Errno> {
        if self.no_pipe_memory {
            return Err(Errno::ENOMEM);
        }
        self.pipes_made += 1;
        Ok(crate::pipe::new(
            Box::new(Ring(Box::new([0; crate::pipe::SIZE]))),
            |_| {},
        ))
    }
    fn pipe_wait(&mut self, id: u64) -> Result<(), Errno> {
        self.waits.push(id);
        Err(Errno::EINTR)
    }
    fn pipe_wake(&mut self, id: u64) {
        self.woken.push(id);
    }
    /// A machine whose filesystems cannot be shut down (an unplugged
    /// stick): the call comes back.
    fn power(&mut self, reboot: bool, force: bool) -> Errno {
        self.powered.push((reboot, force));
        Errno::EIO
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.syncs += 1;
        self.vfs.sync()
    }
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno> {
        if ![1, 42].contains(&pgid) {
            return Err(Errno::ESRCH);
        }
        self.foreground = pgid;
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
    fn processes(&mut self) -> Vec<ProcInfo> {
        self.procs.clone()
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
        fds: FdTable::console(),
        vfs: files(),
        heap_room: usize::MAX,
        room: u64::MAX,
        unmapped: Vec::new(),
        typed: alloc::collections::VecDeque::new(),
        asked: Vec::new(),
        killed_while_reading: false,
        line_mode: false,
        foreground: 1,
        tees: Vec::new(),
        syncs: 0,
        powered: Vec::new(),
        written: Vec::new(),
        slept: Vec::new(),
        spawned: Vec::new(),
        ended: Vec::new(),
        running: false,
        killed: Vec::new(),
        pipes_made: 0,
        no_pipe_memory: false,
        waits: Vec::new(),
        woken: Vec::new(),
        procs: Vec::new(),
    }
}

pub fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
    match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
        Outcome::Return(r) => decode(r),
        Outcome::Exit(code) => panic!("exited with {code}"),
    }
}

/// Everything written to the console, joined.
pub fn text(f: &Fake) -> Vec<u8> {
    f.written.concat()
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

/// `/root/f` holding "hello" and `/root/link` to it, with `/root` the
/// current directory, and a filesystem at `/full` with room for
/// `FULL_BYTES`.
fn files() -> MountTable {
    let mut fs = MemFs::new(Box::new(Clock));
    let root = fs.root();
    let home = fs.mkdir(root, b"root").unwrap();
    let f = fs.create(home, b"f").unwrap();
    fs.write_at(f, 0, b"hello").unwrap();
    fs.symlink(home, b"link", b"f").unwrap();
    let mut t = MountTable::new(Box::new(fs));
    let full = MemFs::new(Box::new(Clock)).with_capacity(FULL_BYTES);
    t.mount(b"/full", Box::new(full)).unwrap();
    t.chdir(b"/root").unwrap();
    t
}
