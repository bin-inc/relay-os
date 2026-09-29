//! The programs the kernel runs (user-space gate §5), as plan 2 of
//! milestone 2 has them: one child at a time, started by the in-kernel
//! shell. `spawn` loads the child; `wait` runs it on its own kernel stack
//! until it exits, and gives everything it had back. There is no
//! scheduler: while the child runs, the shell waits inside `wait`, and the
//! timer only counts ticks. Plan 3 replaces this with the process table and
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

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
    space: AddressSpace,
    stack: KernelStack,
    entry: Entry,
}

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// Pids count from 1 and are not used again while the kernel runs.
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

/// The child while it runs: what its system calls reach. Lives on
/// `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
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

/// The whole of the file at `path`, if it may be a program: not a
/// directory, at most 16 MiB (spec §5.2).
fn read_program(vfs: &mut dyn Vfs, path: &[u8]) -> Result<Vec<u8>, Errno> {
    let node = vfs.lookup(path)?;
    let stat = vfs.stat(node)?;
    if stat.kind == FileType::Directory {
        return Err(Errno::EISDIR);
    }
    if stat.size > elf::MAX_SIZE as u64 {
        return Err(Errno::ENOEXEC);
    }
    let mut file = alloc::vec![0; stat.size as usize];
    let mut done = 0;
    while done < file.len() {
        match vfs.read_at(node, done as u64, &mut file[done..])? {
            0 => break,
            n => done += n,
        }
    }
    file.truncate(done);
    Ok(file)
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
    let file = read_program(vfs, path)?;
    let program = elf::check(&file, arch::ELF_MACHINE, relay_abi::VERSION).map_err(|e| {
        klogln!("spawn {name}: {e}");
        Errno::ENOEXEC
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
        space: &child.space,
        out,
        waiter: 0,
        status: None,
    };
    let e = child.entry;
    let entry = arch::user::UserEntry {
        ip: e.ip,
        sp: e.sp,
        rdi: e.args,
        rsi: e.args_len,
        rdx: e.argc,
    };
    RUNNING.store((&raw mut running).cast(), Ordering::Release);
    let waiter = &raw mut running.waiter;
    // SAFETY: `running` outlives the run; the system calls reach it only
    // through `RUNNING`, while this function waits in `run`.
    arch::user::run(
        &entry,
        child.stack.top(),
        child.space.pml4(),
        mm::kernel_pml4(),
        unsafe { &mut *waiter },
    );
    RUNNING.store(core::ptr::null_mut(), Ordering::Release);
    let status = running.status.unwrap_or_default();
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
    let waiter = &raw const r.waiter;
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

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the child is killed instead.
pub fn non_canonical_return(ip: u64) -> ! {
    klogln!("pid killed: return to non-canonical address {ip:#x}");
    end(WaitStatus {
        how: relay_abi::wait::KILLED,
        ip,
        ..WaitStatus::default()
    })
}
