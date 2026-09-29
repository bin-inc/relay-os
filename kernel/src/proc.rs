//! Processes (user-space gate §5.4, §6.1): the process table, and the
//! kernel's side of running them. Every process has its own kernel stack;
//! one that is not running is switched out on it (`arch::context`). The
//! idle task is the context the kernel booted in, process 0: it runs when
//! nothing is ready, polls the console and the USB hosts, and sleeps until
//! the next tick. Process 1 is the in-kernel shell, a process without a
//! program, which blocks in `wait` while its commands run and on the
//! console while it waits for a line (plan 4 replaces it with `/bin/sh`).
//!
//! The kernel is not preemptible, and a switch happens only in the kernel
//! with interrupts off and no lock held, so a blocked process never holds
//! a lock another one needs.

pub mod table;

use crate::arch::context::{self, Next};
use crate::exec::{self, Entry};
use crate::fd::{FdTable, File};
use crate::mm::kstack::{self, KernelStack};
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::{UserSlice, UserStr};
use crate::mounts::KernelVfs;
use crate::syscall::{self, Caller, Child, Outcome, Spawn};
use crate::{arch, console, klogln, mm, mounts, rtc, timer, tty, usb};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::{MemInfo, Time, WaitStatus};
use spin::Mutex;
use table::{Blocked, Table, Want};
use vfs::{Cwd, Errno, FileType, Vfs};
use x86_64::instructions::interrupts;

/// What a process owns besides its entry in the table.
struct Res {
    stack: KernelStack,
    /// Its program's memory; `None` for the in-kernel shell, and once the
    /// process has ended.
    space: Option<AddressSpace>,
    /// Where its program starts, until it first runs.
    entry: Option<Entry>,
    fds: FdTable,
    /// Its current directory; out of the table while the mount table works
    /// with it (`with_cwd`).
    cwd: Option<Cwd>,
}

static PROCS: Mutex<Table<Res>> = Mutex::new(Table::new());

/// The stack pointer each switched-out process left (`arch::context`), by
/// kernel-stack slot: fixed addresses, which the table's entries are not.
static SAVED: [AtomicU64; kstack::SLOTS] = [const { AtomicU64::new(0) }; kstack::SLOTS];
/// The idle task's.
static IDLE: AtomicU64 = AtomicU64::new(0);

/// The in-kernel shell's output while it waits for a command (plan 2's
/// hook, `System::wait`): what its children write to fds 1 and 2, and the
/// answer to each write (a redirection file's error).
type Out<'a> = &'a mut shell::Output<'a>;
static SHELL_OUT: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Where the running context is saved when it gives up the CPU.
fn save_slot(t: &Table<Res>) -> *mut u64 {
    match t.get(t.current()) {
        Some(p) => SAVED[p.res.stack.slot()].as_ptr(),
        None => IDLE.as_ptr(),
    }
}

/// What the running process runs on, for the switch to it.
fn next(t: &Table<Res>) -> Next {
    match t.get(t.current()) {
        Some(p) => Next {
            rsp: SAVED[p.res.stack.slot()].load(Ordering::Relaxed),
            pml4: p
                .res
                .space
                .as_ref()
                .map_or_else(mm::kernel_pml4, AddressSpace::pml4),
            stack_top: p.res.stack.top(),
        },
        None => Next {
            rsp: IDLE.load(Ordering::Relaxed),
            pml4: mm::kernel_pml4(),
            stack_top: 0,
        },
    }
}

/// A kernel bug if one of these is held when the CPU goes to another
/// process: that one could wait for it for ever.
fn no_lock_held() -> bool {
    !PROCS.is_locked() && !mounts::is_locked() && !tty::is_locked() && !mm::is_locked()
}

/// Gives the CPU to the next ready process, or to the idle task, and
/// returns when the running one gets it back (at once if nothing else is
/// ready and it still is). A process that has blocked returns once woken;
/// one that has ended never does.
fn reschedule() {
    let enabled = interrupts::are_enabled();
    interrupts::disable();
    let switch = {
        let mut t = PROCS.lock();
        let me = t.current();
        if me != 0 {
            settle_ticks(&mut t, 0);
        } else {
            KERNEL_TICKS.store(0, Ordering::Relaxed);
        }
        let save = save_slot(&t);
        (t.schedule() != me).then(|| (save, next(&t)))
    };
    if let Some((save, next)) = switch {
        debug_assert!(no_lock_held(), "a lock held across a switch");
        // SAFETY: interrupts are off; `next` is a context `switch` saved
        // or a first frame; every process's tables map the kernel.
        unsafe { context::switch_to(save, &next) };
    }
    if enabled {
        interrupts::enable();
    }
}

/// Ticks that interrupted the kernel since they were last counted: the
/// kernel is not preemptible, so such a tick only counts here (spec §6.1's
/// `need_resched`), and the count is settled when the running process
/// returns to ring 3 or gives up the CPU.
static KERNEL_TICKS: AtomicU64 = AtomicU64::new(0);

/// Charges the ticks that interrupted the kernel (and `more`) to the
/// running process; whether its slice is used up while another process is
/// ready. Due sleepers are woken first, so a sleeper does not wait for the
/// idle task behind a process that never blocks.
fn settle_ticks(t: &mut Table<Res>, more: u64) -> bool {
    t.wake_sleepers(timer::ticks());
    let n = KERNEL_TICKS.swap(0, Ordering::Relaxed) + more;
    let mut used_up = false;
    for _ in 0..n {
        used_up |= t.tick();
    }
    used_up
}

/// A tick interrupted the kernel (spec §6.1): counted for later, nothing
/// else. The idle task's ticks are nobody's.
pub fn kernel_tick() {
    KERNEL_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// What the console's input asks of the processes: a Ctrl-C in line mode
/// kills the foreground group (spec §6.4), and anything typed wakes whoever
/// waits for input.
fn console_input(t: &mut Table<Res>) {
    if let Some(pgid) = tty::ctrl_c() {
        // Refused only for process 1's group, which never has the console
        // in line mode.
        let _ = t.kill(-i64::from(pgid), relay_abi::wait::KILLED_CTRL_C);
    }
    if tty::has_input() {
        t.wake_all(Blocked::Console);
    }
}

/// A tick interrupted a program (spec §6.1, §6.3): the kernel holds nothing
/// now, so the tick polls the console, and the program gives up the CPU if
/// its slice is used up, or ends if it was killed.
pub fn user_tick() {
    tty::poll();
    let used_up = {
        let mut t = PROCS.lock();
        console_input(&mut t);
        settle_ticks(&mut t, 1)
    };
    if used_up {
        reschedule();
    }
    end_if_killed();
}

/// On the way back to ring 3 from a system call: the ticks the call took
/// are counted, the program gives up the CPU if its slice is used up, and
/// it ends if it was killed meanwhile.
pub fn before_user() {
    let used_up = settle_ticks(&mut PROCS.lock(), 0);
    if used_up {
        reschedule();
    }
    end_if_killed();
}

/// Ends the running process if Ctrl-C or `kill` marked it (spec §11.1):
/// before it runs another instruction of its program.
fn end_if_killed() {
    let killed = {
        let t = PROCS.lock();
        t.get(t.current()).and_then(|p| p.killed)
    };
    if let Some(reason) = killed {
        let status = WaitStatus::killed(reason);
        {
            let t = PROCS.lock();
            let me = t.current();
            let name = t.get(me).map_or("?", |p| p.name.as_str());
            klogln!("pid {me} ({name}): killed: {status}");
        }
        end(status);
    }
}

/// Gives the console to the process group of the running process's child
/// `pid`, in line mode, while the in-kernel shell waits for it (spec
/// §6.4). A Ctrl-C typed before the command started waits in the input
/// queue, and the next poll in line mode finds it: it is the command's.
pub fn give_console(pid: u32) {
    if let Some(p) = PROCS.lock().get(pid) {
        tty::set_foreground(p.pgid);
        tty::set_line_mode(true);
    }
}

/// Gives the console back to the in-kernel shell: its own group, raw mode.
pub fn take_console() {
    tty::set_line_mode(false);
    tty::set_foreground(table::INIT);
}

/// The running process blocks on `why` until something wakes it.
fn block(why: Blocked) {
    PROCS.lock().block(why);
    reschedule();
}

/// The idle task (spec §6.1): the context the kernel booted in, from the
/// moment process 1 exists. It polls the console, services the USB hosts,
/// wakes the sleepers whose time has come, and sleeps until the next tick
/// when nothing is ready. Never returns.
fn idle() -> ! {
    loop {
        tty::poll();
        console_input(&mut PROCS.lock());
        usb::service();
        PROCS.lock().wake_sleepers(timer::ticks());
        if PROCS.lock().others_ready() {
            reschedule();
        } else {
            arch::wait_for_interrupt();
        }
    }
}

/// A new kernel stack whose first switch runs `f(arg)`.
fn prepare(stack: &KernelStack, f: extern "C" fn(u64) -> !, arg: u64) {
    let frame = context::first_frame(f, arg);
    let at = stack.top() - (frame.len() * 8) as u64;
    // SAFETY: the stack is mapped, nothing runs on it yet, and the frame
    // fits in its top page.
    unsafe { core::ptr::write(at as *mut [u64; context::FRAME_WORDS], frame) };
    SAVED[stack.slot()].store(at, Ordering::Relaxed);
}

/// The running process (the in-kernel shell) waits until something is
/// typed.
pub fn wait_for_input() {
    block(Blocked::Console);
}

/// Starts the in-kernel shell as process 1 in `cwd`, running
/// `shell(arg)`, and becomes the idle task. Never returns.
pub fn start(shell: extern "C" fn(u64) -> !, arg: u64, cwd: Cwd) -> ! {
    let stack = mm::alloc_kernel_stack().expect("a kernel stack for process 1");
    prepare(&stack, shell, arg);
    let res = Res {
        stack,
        space: None,
        entry: None,
        fds: FdTable::shell(),
        cwd: Some(cwd),
    };
    let pid = PROCS
        .lock()
        .insert(0, true, String::from("relay-sh"), res)
        .unwrap_or_else(|_| unreachable!("the table is empty"));
    assert_eq!(pid, table::INIT);
    idle()
}

/// Runs `f` with the running process's current directory, and keeps what
/// `f` makes of it.
pub fn with_cwd<R>(f: impl FnOnce(&mut Cwd) -> R) -> R {
    let taken = {
        let mut t = PROCS.lock();
        let me = t.current();
        t.get_mut(me).and_then(|p| p.res.cwd.take())
    };
    let mut cwd = taken.expect("a process with its current directory");
    let r = f(&mut cwd);
    let mut t = PROCS.lock();
    let me = t.current();
    if let Some(p) = t.get_mut(me) {
        p.res.cwd = Some(cwd);
    }
    r
}

/// Where a new process's program starts: its first switch comes here, with
/// no lock held.
extern "C" fn first_run(_: u64) -> ! {
    // Killed before it ever ran: none of its program runs either.
    end_if_killed();
    let entry = {
        let mut t = PROCS.lock();
        let me = t.current();
        t.get_mut(me).and_then(|p| p.res.entry.take())
    };
    let entry = entry.expect("a new process has its entry");
    // SAFETY: the switch put its address space in CR3 and its kernel stack
    // in the TSS and the per-CPU block; this stack is its kernel stack.
    unsafe { arch::user::enter(&entry) }
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

/// Starts the child `s` of the running process (spec §5.2-§5.4, §7.3): the
/// program at its path, read through the mount table from the running
/// process's current directory, with its arguments, fds, working directory
/// and group. `EAGAIN` when the table is full or every pid has been
/// used; the fds and the working directory are checked before the program
/// is read.
pub fn spawn(s: &Spawn) -> Result<u32, Errno> {
    let (fds, mut cwd) = {
        let t = PROCS.lock();
        if !t.has_room() {
            return Err(Errno::EAGAIN);
        }
        let parent = t.get(t.current()).expect("a process spawns");
        (parent.res.fds.for_child(&s.fds)?, parent.res.cwd.clone())
    };
    let mut cwd = cwd.take().expect("the parent has its current directory");
    if !s.cwd.is_empty() {
        mounts::with(&mut cwd, |t| t.chdir(&s.cwd))?;
    }
    let name = String::from_utf8_lossy(&s.path);
    let (file, program) =
        read_program(&mut KernelVfs, &s.path, mm::heap_room()).map_err(|r| match r {
            Refusal::Unreadable(e) => e,
            Refusal::NotAProgram(e) => {
                klogln!("spawn {name}: {e}");
                Errno::ENOEXEC
            }
        })?;
    let stack = mm::alloc_kernel_stack().map_err(kstack::StackError::errno)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(&mut space, mem, &file, &program, &s.args, s.argc) {
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
    let mut t = PROCS.lock();
    if !t.has_room() {
        // Checked above, and nothing else ran since; but a refusal here
        // must give back what was taken, not panic.
        drop(t);
        mm::free_kernel_stack(stack);
        mm::with_user_memory(|mem, _| space.destroy(mem));
        return Err(Errno::EAGAIN);
    }
    prepare(&stack, first_run, 0);
    let res = Res {
        stack,
        space: Some(space),
        entry: Some(entry),
        fds,
        cwd: Some(cwd),
    };
    let me = t.current();
    t.insert(me, s.new_group, name.into_owned(), res)
        .map_err(|e| unreachable!("has_room was checked under this lock: {e}"))
}

/// A child of the running process that has ended, taken out of the table
/// with its kernel stack given back; `None` with `nohang` when none has.
/// `ECHILD` if there is no such child; `EINTR` if the running process was
/// killed while it waited (it ends on its way back to ring 3).
fn collect(child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
    let want = match child {
        Child::Any => Want::Any,
        Child::Pid(pid) => Want::Pid(pid),
    };
    loop {
        let reaped = {
            let mut t = PROCS.lock();
            let me = t.current();
            if t.get(me).is_some_and(|p| p.killed.is_some()) {
                return Err(Errno::EINTR);
            }
            t.reap(me, want)?
        };
        match reaped {
            Some(p) => {
                mm::free_kernel_stack(p.res.stack);
                let table::State::Zombie(status) = p.state else {
                    unreachable!("reap takes only zombies")
                };
                return Ok(Some((p.pid, status)));
            }
            None if nohang => return Ok(None),
            None => block(Blocked::Wait),
        }
    }
}

/// Gives the in-kernel shell fresh standard output and error for the
/// command it starts next: its hook answers only what that command and its
/// descendants write (they inherit these files), so an orphan of an
/// earlier command, which holds that command's, writes to the screen.
pub fn renew_outputs() {
    let mut t = PROCS.lock();
    let me = t.current();
    if let Some(p) = t.get_mut(me) {
        p.res.fds.set(1, Arc::new(File::ShellOutput(1)));
        p.res.fds.set(2, Arc::new(File::ShellOutput(2)));
    }
}

/// Whether `file` is one of the in-kernel shell's outputs now, the ones its
/// current command has.
fn shell_output_now(file: &Arc<File>, fd: u32) -> bool {
    let t = PROCS.lock();
    t.get(table::INIT)
        .and_then(|p| p.res.fds.get(u64::from(fd)).ok())
        .is_some_and(|now| Arc::ptr_eq(now, file))
}

/// Collects the running process's children that have ended: for the
/// in-kernel shell, process 1, the orphans that passed to it. It does so
/// before each command it starts and after each it waited for, so their
/// zombies never fill the table (plan 4's `/bin/sh` does it before every
/// prompt).
pub fn collect_orphans() {
    while let Ok(Some(_)) = collect(Child::Any, true) {}
}

/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook); then it
/// collects the orphans that have ended. `ECHILD` if `pid` is not its
/// child.
pub fn wait(pid: u32, out: &mut shell::Output<'_>) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = collect(Child::Pid(pid), false);
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    collect_orphans();
    match ended? {
        Some((_, status)) => Ok(status),
        None => unreachable!("wait without nohang collects a child"),
    }
}

/// The running process ends with `status` (spec §5.4): its memory and fds
/// are given back at once, it stays a zombie until its parent waits for
/// it, and the CPU goes to the next process for good.
fn end(status: WaitStatus) -> ! {
    let (space, fds) = {
        let mut t = PROCS.lock();
        let me = t.current();
        let p = t.get_mut(me).expect("a running process ends");
        let taken = (p.res.space.take(), core::mem::take(&mut p.res.fds));
        t.end(me, status);
        taken
    };
    drop(fds);
    if let Some(space) = space {
        // Off its page tables before they go; the kernel stack is in the
        // kernel's half, which every table maps.
        context::use_tables(mm::kernel_pml4());
        mm::with_user_memory(|mem, _| space.destroy(mem));
    }
    reschedule();
    unreachable!("a zombie ran again")
}

/// The running process's side of the dispatcher.
struct Current;

impl Current {
    /// Runs `f` with the running process's address space.
    fn space<R>(
        &self,
        f: impl FnOnce(&AddressSpace, &mut mm::UserMem<'_>) -> Result<R, Errno>,
    ) -> Result<R, Errno> {
        let t = PROCS.lock();
        let space = t
            .get(t.current())
            .and_then(|p| p.res.space.as_ref())
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| f(space, mem))
    }

    /// The running process's file open as `fd`.
    fn file(&self, fd: u64) -> Result<Arc<File>, Errno> {
        let t = PROCS.lock();
        let p = t.get(t.current()).ok_or(Errno::EBADF)?;
        p.res.fds.get(fd).cloned()
    }
}

impl Caller for Current {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        self.space(|space, mem| slice.read(space, mem, offset, buf))
    }

    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
        self.space(|space, mem| slice.write(space, mem, offset, bytes))
    }

    fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno> {
        self.space(|space, mem| slice.check_writable(space, mem))
    }

    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno> {
        self.space(|space, mem| s.read(space, mem))
    }

    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno> {
        self.file(fd).map(|_| ())
    }

    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
        let file = self.file(fd)?;
        match *file {
            File::Console => console::write_output(bytes),
            File::ShellOutput(n) => {
                let out = SHELL_OUT.load(Ordering::Acquire);
                if out.is_null() || !shell_output_now(&file, n) {
                    // The shell waits for nobody, or for another command:
                    // this goes to the screen.
                    console::write_output(bytes);
                } else {
                    // SAFETY: set by the in-kernel shell's `wait`, which is
                    // blocked until its child has ended and clears it
                    // before it returns; nothing else calls it meanwhile.
                    return unsafe { (*out.cast::<Out<'_>>())(n, bytes) };
                }
            }
        }
        Ok(())
    }

    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno> {
        spawn(s)
    }

    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
        collect(child, nohang)
    }

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        PROCS.lock().kill(target, relay_abi::wait::KILLED_KILL)
    }

    fn pid(&self) -> u32 {
        PROCS.lock().current()
    }

    fn memory(&self) -> MemInfo {
        let s = mm::stats();
        MemInfo {
            ram_total: s.total_frames * mm::frame::FRAME_SIZE,
            ram_free: s.free_frames * mm::frame::FRAME_SIZE,
            heap_total: s.heap.total as u64,
            heap_used: s.heap.used as u64,
        }
    }

    fn time(&self) -> Time {
        let uptime = timer::tsc_time().unwrap_or_else(timer::uptime);
        Time {
            unix_seconds: rtc::now_unix().unwrap_or(0),
            uptime_ns: u64::try_from(uptime.as_nanos()).unwrap_or(u64::MAX),
        }
    }

    /// Without a ticking timer nothing would wake it, so it returns at once
    /// (nothing waits for ever).
    fn sleep(&mut self, ms: u64) {
        if ms == 0 || !timer::is_ticking() {
            return;
        }
        block(Blocked::Sleep(timer::sleep_until(timer::ticks(), ms)));
    }
}

/// A system call of the running process: its result register, or, for
/// `exit`, the end of it.
pub fn system_call(number: u64, args: [u64; 6]) -> u64 {
    match syscall::dispatch(&mut Current, number, args) {
        Outcome::Return(result) => result,
        Outcome::Exit(code) => end(WaitStatus::exited(code)),
    }
}

/// The running process caused an exception (spec §11.1): it is killed, and
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
    {
        let t = PROCS.lock();
        let me = t.current();
        let name = t.get(me).map_or("?", |p| p.name.as_str());
        klogln!("pid {me} ({name}): killed: {status}");
    }
    end(status)
}

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the process is killed as if the
/// return itself had faulted.
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
