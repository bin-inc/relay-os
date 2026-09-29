//! The programs the kernel runs (user-space gate §5), as plan 2 of
//! milestone 2 has them: one child at a time, started by the in-kernel
//! shell. `spawn` loads the child; `wait` runs it on its own kernel stack
//! until it exits, and gives everything it had back. There is no
//! scheduler: while the child runs, the shell waits inside `wait`, and the
//! timer only counts ticks. Plan 3 replaces this with the process table and
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

pub mod table;

use crate::exec::{self, Entry};
use crate::mm::kstack::KernelStack;
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::UserSlice;
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, klogln, mm};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use relay_abi::WaitStatus;
use spin::Mutex;
use vfs::{Errno, FileType, Vfs};

/// A program `spawn` loaded and nobody has waited for yet.
struct Child {
    pid: u32,
    /// Its path, for the kernel log.
    name: String,
    space: AddressSpace,
    stack: KernelStack,
    entry: Entry,
}

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// Pids count from 1 and are not used again while the kernel runs.
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

/// The child while it runs: what its system calls and faults reach. Lives
/// on `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
    pid: u32,
    name: &'a str,
    space: &'a AddressSpace,
    out: &'a mut dyn FnMut(u32, &[u8]),
    /// The stack pointer `arch::user::enter` saved; `leave` goes back to it.
    waiter: u64,
    status: Option<WaitStatus>,
}

static RUNNING: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

impl Caller for Running<'_> {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        // The lock is held only for the copy: the output may reach a disk,
        // whose driver allocates memory too.
        mm::with_user_memory(|mem, _| slice.read(self.space, mem, offset, buf))
    }

    fn output(&mut self, fd: u32, bytes: &[u8]) {
        (self.out)(fd, bytes);
    }
}

/// Why `spawn` refuses a file.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    /// It cannot be read (missing, a directory, a disk error).
    Unreadable(Errno),
    /// It is not a program this kernel runs: `ENOEXEC`, and the reason
    /// goes to the kernel log.
    NotAProgram(elf::ElfError),
}

impl From<Errno> for Refusal {
    fn from(e: Errno) -> Refusal {
        Refusal::Unreadable(e)
    }
}

/// The file at `path` and the program in it (spec §5.2). A file over 16
/// MiB, or one whose first bytes are not an ELF file's, is refused before
/// the rest is read; one bigger than `room` bytes of heap is `ENOMEM`
/// before anything is allocated for it, since the kernel's heap panics
/// when it runs out.
fn read_program(
    vfs: &mut dyn Vfs,
    path: &[u8],
    room: usize,
) -> Result<(Vec<u8>, elf::Program), Refusal> {
    let node = vfs.lookup(path)?;
    let stat = vfs.stat(node)?;
    if stat.kind == FileType::Directory {
        return Err(Errno::EISDIR.into());
    }
    let size = usize::try_from(stat.size).unwrap_or(usize::MAX);
    if size > elf::MAX_SIZE {
        return Err(Refusal::NotAProgram(elf::ElfError::TooBig(size)));
    }
    let mut magic = [0u8; 4];
    let mut done = 0;
    while done < magic.len() {
        match vfs.read_at(node, done as u64, &mut magic[done..])? {
            0 => break,
            n => done += n,
        }
    }
    if magic != *b"\x7fELF" {
        return Err(Refusal::NotAProgram(elf::ElfError::NotElf));
    }
    if size > room {
        return Err(Errno::ENOMEM.into());
    }
    let mut file = alloc::vec![0; size];
    let mut done = 0;
    while done < file.len() {
        match vfs.read_at(node, done as u64, &mut file[done..])? {
            0 => break,
            n => done += n,
        }
    }
    file.truncate(done);
    match elf::check(&file, arch::ELF_MACHINE, relay_abi::VERSION) {
        Ok(program) => Ok((file, program)),
        Err(e) => Err(Refusal::NotAProgram(e)),
    }
}

fn memory_error(e: MapError) -> Errno {
    match e {
        MapError::OutOfMemory => Errno::ENOMEM,
        _ => Errno::ENOEXEC,
    }
}

/// Loads the program at `path` (read through `vfs`) with `args` (argument
/// 0 first) as the child; its pid. `EAGAIN` while another child exists.
pub fn spawn(vfs: &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Result<u32, Errno> {
    let mut child = CHILD.lock();
    if child.is_some() {
        return Err(Errno::EAGAIN);
    }
    let name = String::from_utf8_lossy(path);
    let (file, program) = read_program(vfs, path, mm::heap_room()).map_err(|r| match r {
        Refusal::Unreadable(e) => e,
        Refusal::NotAProgram(e) => {
            klogln!("spawn {name}: {e}");
            Errno::ENOEXEC
        }
    })?;
    let arg_bytes = exec::arg_bytes(args)?;
    let stack = mm::alloc_kernel_stack().ok_or(Errno::EAGAIN)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(
            &mut space,
            mem,
            &file,
            &program,
            &arg_bytes,
            args.len() as u64,
        ) {
            Ok(entry) => Ok((space, entry)),
            Err(e) => {
                space.destroy(mem);
                Err(e)
            }
        }
    });
    let (space, entry) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            mm::free_kernel_stack(stack);
            klogln!("spawn {name}: {e}");
            return Err(e);
        }
    };
    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
    *child = Some(Child {
        pid,
        name: name.into_owned(),
        space,
        stack,
        entry,
    });
    Ok(pid)
}

/// Runs the child `pid` until it ends, giving what it writes to fds 1 and 2
/// to `out`; then gives back its memory and kernel stack. `ECHILD` if
/// `pid` is not the child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let child = {
        let mut slot = CHILD.lock();
        match slot.take() {
            Some(c) if c.pid == pid => c,
            other => {
                *slot = other;
                return Err(Errno::ECHILD);
            }
        }
    };
    let mut running = Running {
        pid: child.pid,
        name: &child.name,
        space: &child.space,
        out,
        waiter: 0,
        status: None,
    };
    // From here until `run` returns, `running` is reached only through
    // this pointer: here, and in its system calls and faults (`RUNNING`).
    let r = &raw mut running;
    let e = child.entry;
    let entry = arch::user::UserEntry {
        ip: e.ip,
        sp: e.sp,
        rdi: e.args,
        rsi: e.args_len,
        rdx: e.argc,
    };
    RUNNING.store(r.cast(), Ordering::Release);
    // SAFETY: `running` outlives the run, and nothing else touches it
    // while this function waits in `run`; the space shares the kernel's
    // upper half.
    unsafe {
        arch::user::run(
            &entry,
            child.stack.top(),
            child.space.pml4(),
            mm::kernel_pml4(),
            &raw mut (*r).waiter,
        );
    }
    RUNNING.store(core::ptr::null_mut(), Ordering::Release);
    // SAFETY: the run is over; `r` is the only way to `running` again.
    let status = unsafe { (*r).status }.unwrap_or_default();
    mm::with_user_memory(|mem, _| child.space.destroy(mem));
    mm::free_kernel_stack(child.stack);
    Ok(status)
}

/// The running child and the stack pointer to leave to.
///
/// # Safety
/// Only while a child runs (from inside its system calls and faults).
unsafe fn running<'a>() -> &'a mut Running<'a> {
    let p = RUNNING.load(Ordering::Acquire);
    assert!(!p.is_null(), "a system call without a running program");
    // SAFETY: set by `wait` to its `Running`, which lives until `run`
    // returns.
    unsafe { &mut *p.cast::<Running<'a>>() }
}

/// Ends the running child with `status` and goes back to `wait`.
fn end(status: WaitStatus) -> ! {
    // SAFETY: called on the child's kernel stack while it runs.
    let r = unsafe { running() };
    r.status = Some(status);
    let waiter = &raw mut r.waiter;
    // SAFETY: `enter` saved it and has not returned.
    unsafe { arch::user::leave(waiter) }
}

/// A system call of the running child: its result register, or, for
/// `exit`, back to `wait`.
pub fn system_call(number: u64, args: [u64; 6]) -> u64 {
    // SAFETY: called from the entry stub, on the child's kernel stack.
    let r = unsafe { running() };
    match syscall::dispatch(r, number, args) {
        Outcome::Return(result) => result,
        Outcome::Exit(code) => end(WaitStatus::exited(code)),
    }
}

/// The running child caused an exception (spec §11.1): it is killed, and
/// the kernel log says how. A page fault in the stack's guard page is a
/// stack overflow.
pub fn fault(kind: u32, detail: u32, address: u64, ip: u64) -> ! {
    use relay_abi::wait::{FAULT_PAGE, FAULT_STACK_OVERFLOW};
    let kind = if kind == FAULT_PAGE && exec::in_guard_page(address) {
        FAULT_STACK_OVERFLOW
    } else {
        kind
    };
    let status = WaitStatus::fault(kind, detail, address, ip);
    // SAFETY: called on the child's kernel stack while it runs.
    let r = unsafe { running() };
    klogln!("pid {} ({}): killed: {status}", r.pid, r.name);
    end(status)
}

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the child is killed as if the return
/// itself had faulted.
pub fn non_canonical_return(ip: u64) -> ! {
    fault(relay_abi::wait::FAULT_GENERAL_PROTECTION, 0, 0, ip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use vfs::{DirEntry, Env, FileSystem, Ino, MemFs, MountTable, Stat, StatFs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    /// A `MemFs` that fails a test when one read asks for more than `limit`
    /// bytes: a refused file is never read into memory.
    struct NoReads {
        fs: MemFs,
        limit: usize,
    }

    impl FileSystem for NoReads {
        fn root(&self) -> Ino {
            self.fs.root()
        }
        fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
            self.fs.stat(ino)
        }
        fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.fs.lookup(dir, name)
        }
        fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
            self.fs.read_dir(dir)
        }
        fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
            self.fs.read_link(ino)
        }
        fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
            assert!(buf.len() <= self.limit, "a read of {} bytes", buf.len());
            self.fs.read_at(ino, offset, buf)
        }
        fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
            self.fs.write_at(ino, offset, buf)
        }
        fn truncate(&mut self, ino: Ino, size: u64) -> Result<(), Errno> {
            self.fs.truncate(ino, size)
        }
        fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
            self.fs.touch(ino)
        }
        fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.fs.create(dir, name)
        }
        fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.fs.mkdir(dir, name)
        }
        fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
            self.fs.unlink(dir, name)
        }
        fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
            self.fs.rmdir(dir, name)
        }
        fn rename(
            &mut self,
            from_dir: Ino,
            from: &[u8],
            to_dir: Ino,
            to: &[u8],
        ) -> Result<(), Errno> {
            self.fs.rename(from_dir, from, to_dir, to)
        }
        fn statfs(&mut self) -> Result<StatFs, Errno> {
            self.fs.statfs()
        }
        fn sync(&mut self) -> Result<(), Errno> {
            self.fs.sync()
        }
        fn shutdown(&mut self) -> Result<(), Errno> {
            self.fs.shutdown()
        }
    }

    /// `/root` with a text file and a sparse file of 16 MiB and one byte,
    /// on a filesystem that fails a test if more than 16 MiB are read.
    fn root() -> MountTable {
        root_reading_at_most(elf::MAX_SIZE)
    }

    /// The same, failing a test on a read of more than `limit` bytes.
    fn root_reading_at_most(limit: usize) -> MountTable {
        let fs = NoReads {
            fs: MemFs::new(Box::new(Clock)),
            limit,
        };
        let mut vfs = MountTable::new(Box::new(fs));
        vfs.mkdir(b"/root").unwrap();
        let text = vfs.create(b"/root/text").unwrap();
        vfs.write_at(text, 0, b"not a program\n").unwrap();
        let big = vfs.create(b"/root/big").unwrap();
        vfs.truncate(big, elf::MAX_SIZE as u64 + 1).unwrap();
        // An ELF file's first bytes, then a mebibyte of nothing.
        let elf = vfs.create(b"/root/elf").unwrap();
        vfs.write_at(elf, 0, b"\x7fELF").unwrap();
        vfs.truncate(elf, 1 << 20).unwrap();
        vfs
    }

    #[test]
    fn a_file_that_is_no_program_says_why() {
        let mut vfs = root();
        assert_eq!(
            read_program(&mut vfs, b"/root/text", usize::MAX).unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::NotElf)
        );
        assert_eq!(
            read_program(&mut vfs, b"/root/big", usize::MAX).unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::TooBig(16 * 1024 * 1024 + 1)),
            "refused by its size, with the reason for the log"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_its_error() {
        let mut vfs = root();
        assert_eq!(
            read_program(&mut vfs, b"/root/missing", usize::MAX).unwrap_err(),
            Refusal::Unreadable(Errno::ENOENT)
        );
        assert_eq!(
            read_program(&mut vfs, b"/root", usize::MAX).unwrap_err(),
            Refusal::Unreadable(Errno::EISDIR)
        );
    }

    #[test]
    fn a_file_that_is_no_elf_file_is_refused_by_its_first_bytes() {
        let mut vfs = root_reading_at_most(4);
        assert_eq!(
            read_program(&mut vfs, b"/root/text", usize::MAX).unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::NotElf)
        );
    }

    #[test]
    fn a_program_the_heap_has_no_room_for_is_enomem_before_it_is_read() {
        let mut vfs = root_reading_at_most(4);
        assert_eq!(
            read_program(&mut vfs, b"/root/elf", (1 << 20) - 1).unwrap_err(),
            Refusal::Unreadable(Errno::ENOMEM)
        );
        // With room, it is read and checked.
        let mut vfs = root();
        assert_eq!(
            read_program(&mut vfs, b"/root/elf", 1 << 20).unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::Not64Bit),
            "read whole and checked: no ELF64 header behind the magic"
        );
    }
}
