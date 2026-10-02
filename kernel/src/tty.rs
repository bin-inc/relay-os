//! The console's input (user-space gate §6.3–§6.5): one queue for every
//! source, the USB keyboards and COM1, filled by `poll`. `poll` never waits,
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program or ends a system call during which
//! a tick passed, in a program's console read, and at the error screen.
//!
//! The console has a foreground process group and a mode (spec §6.4). In
//! raw mode a Ctrl-C is input like any other byte (the shell's line editor
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`), and what is typed goes through the
//! line discipline, which echoes it as it comes (`input`).

use crate::fd::File;
use crate::input::InputQueue;
use crate::tee::TeeStack;
use crate::{console, klogln, mounts, serial, timer, usb};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;
use vfs::{Change, Errno};

/// The foreground process group; process 1's at first.
static FOREGROUND: AtomicU32 = AtomicU32::new(1);

/// The console's mode: raw (`false`) or line (`true`); the previous one.
/// What the switch hands the line discipline is echoed.
pub fn set_line_mode(line: bool) -> bool {
    let (was, echo) = {
        let mut input = INPUT.lock();
        (input.set_line_mode(line), input.take_echo())
    };
    output(&echo);
    was
}

/// Makes `pgid` the console's foreground group.
pub fn set_foreground(pgid: u32) {
    FOREGROUND.store(pgid, Ordering::Relaxed);
}

/// The group that reads the console (spec §6.4).
pub fn foreground() -> u32 {
    FOREGROUND.load(Ordering::Relaxed)
}

/// The foreground group a Ctrl-C typed in line mode is for, if one was
/// typed: what was typed before it is dropped (spec §6.4). Only while the
/// group is `alive`: once it has ended, the Ctrl-C waits for the group
/// that takes the console back.
pub fn ctrl_c(alive: impl FnOnce(u32) -> bool) -> Option<u32> {
    let pgid = FOREGROUND.load(Ordering::Relaxed);
    let alive = alive(pgid);
    INPUT.lock().take_line_interrupt_for(alive).then_some(pgid)
}

/// Whether a Ctrl-C typed in raw mode waits to be read.
pub fn has_raw_ctrl_c() -> bool {
    INPUT.lock().has_raw_interrupt()
}

/// Takes a Ctrl-C typed in raw mode out of the input; whether one waited.
pub fn take_raw_ctrl_c() -> bool {
    INPUT.lock().take_raw_interrupt()
}

/// What a program reads (spec §6.5): in line mode the next line, in raw
/// mode what was typed, up to `buf`'s length; `None` if nothing waits.
pub fn read(buf: &mut [u8]) -> Option<usize> {
    INPUT.lock().read(buf)
}

/// The line discipline's echo, to the screen and the tees. It may come
/// from a tick, where no file can be written: the tees keep it for the
/// next write or read of a process (`flush_due_tees`), up to a limit.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
        TEES.lock().add_echo(echo);
    }
}

/// Writes the tees' copies that are due; for a process that reads the
/// console, whose echo would otherwise wait for its next write.
pub fn flush_due_tees() {
    flush(&mut TEES.lock(), false);
}

/// The console's tees (spec §6.5).
static TEES: Mutex<TeeStack<Arc<File>>> = Mutex::new(TeeStack::new());

/// Writes what a process or init gives the console: the
/// screen, and a copy for every tee, written once 4 KiB of it waits.
pub fn write(bytes: &[u8]) {
    console::write_output(bytes);
    let mut tees = TEES.lock();
    if tees.is_copying() {
        tees.add(bytes);
        flush(&mut tees, false);
    }
}

/// Writes what waits for the tees: all of it with `all` (a `sync`), or
/// what is due.
fn flush(tees: &mut TeeStack<Arc<File>>, all: bool) {
    for (owner, e) in tees.flush(all, &mut write_tee) {
        klogln!("pid {owner}: a console tee failed: {e}; it is removed");
    }
}

/// A tee's copies to its file.
fn write_tee(file: &Arc<File>, bytes: &[u8]) -> Result<(), Errno> {
    match &**file {
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console | File::Pipe(_) => Err(Errno::EINVAL),
    }
}

/// Writes everything that waits for the tees (at every `sync`).
pub fn sync_tees() {
    flush(&mut TEES.lock(), true);
}

/// Pushes `file` as a tee of process `owner`: a file of the VFS it has
/// open for writing (`EBADF` otherwise, `EINVAL` for the console);
/// `EBUSY` if 4 are pushed.
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
    tee_target(&file)?;
    TEES.lock().push(owner, file)
}

/// Whether `file` can be a tee: a file of the VFS open for writing
/// (`EBADF` otherwise), not the console or a pipe (`EINVAL`: a tee is
/// written from any process's context, where a full pipe could not wait).
fn tee_target(file: &File) -> Result<(), Errno> {
    match file {
        File::Vfs(open) if open.is_writable() => Ok(()),
        File::Vfs(_) => Err(Errno::EBADF),
        File::Console | File::Pipe(_) => Err(Errno::EINVAL),
    }
}

/// Pops `owner`'s newest tee after writing what waits for it; the error of
/// a write that failed for it, now or before.
pub fn pop_tee(owner: u32) -> Result<(), Errno> {
    TEES.lock().pop(owner, &mut write_tee)
}

/// Process `owner` ended: its tees end too (written at the next flush).
pub fn end_tees(owner: u32) {
    TEES.lock().end(owner);
}

/// A removal elsewhere freed an inode: a tee of it is gone.
pub fn follow_tees(changes: &[Change]) {
    let tees = TEES.lock();
    for file in tees.files() {
        for c in changes {
            if let File::Vfs(open) = &**file
                && c.removed() == Some(open.node())
            {
                open.mark_gone();
            }
        }
    }
}

/// Whether the tee stack is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn tees_locked() -> bool {
    TEES.is_locked()
}

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Moves whatever the input devices have into the queue. Never waits.
pub fn poll() {
    let mut input = INPUT.lock();
    usb::poll(&mut input);
    let now = now_ms();
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b, now),
            None => break,
        }
    }
    input.expire(now);
    let echo = input.take_echo();
    drop(input);
    output(&echo);
}

/// Milliseconds since the machine started, for serial escape sequences:
/// from the TSC, which runs even when the timer could not start.
fn now_ms() -> u64 {
    let t = timer::tsc_time().unwrap_or_else(timer::uptime);
    u64::try_from(t.as_millis()).unwrap_or(u64::MAX)
}

/// The oldest byte typed.
pub fn pop() -> Option<u8> {
    INPUT.lock().pop()
}

/// Whether anything typed waits to be read: raw input, or a line.
pub fn has_input() -> bool {
    !INPUT.lock().is_empty()
}

/// Whether the input queue is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    INPUT.is_locked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file;
    use alloc::boxed::Box;
    use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_WRITE};
    use vfs::{Env, MemFs, MountTable};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    #[test]
    fn a_tee_is_a_file_open_for_writing() {
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock))));
        let open = |t: &mut MountTable, flags| File::Vfs(file::open(t, b"/log", flags).unwrap());
        let written = open(&mut t, OPEN_WRITE | OPEN_CREATE);
        let read_write = open(&mut t, OPEN_READ | OPEN_WRITE);
        let read_only = open(&mut t, OPEN_READ);
        assert_eq!(tee_target(&written), Ok(()));
        assert_eq!(tee_target(&read_write), Ok(()));
        assert_eq!(tee_target(&read_only), Err(Errno::EBADF));
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
        struct Ring(alloc::boxed::Box<[u8; crate::pipe::SIZE]>);
        impl crate::pipe::Memory for Ring {
            fn bytes(&mut self) -> &mut [u8; crate::pipe::SIZE] {
                &mut self.0
            }
        }
        let (r, w) = crate::pipe::new(Box::new(Ring(Box::new([0; crate::pipe::SIZE]))), |_| {});
        for end in [r, w] {
            assert_eq!(tee_target(&File::Pipe(end)), Err(Errno::EINVAL));
        }
    }
}
