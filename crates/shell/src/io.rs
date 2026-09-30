//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements `Console` and `System` over its console, clock,
//! memory manager, log, ACPI and programs; `xtask host-shell` over the
//! host terminal; `relay-rt` over system calls; the tests over buffers.
//! `Programs` is `/bin/sh`'s way to its commands.

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Vfs};

/// Where a program's output goes (`System::wait`): what it writes, on fd
/// 1 or 2, and the write's error, if any.
pub type Output<'a> = dyn FnMut(u32, &[u8]) -> Result<(), Errno> + 'a;

/// The screen and keyboard.
pub trait Console {
    /// The next input byte, waiting for one. `None` when input has ended,
    /// which only happens on the host (stdin closed).
    fn read_byte(&mut self) -> Option<u8>;
    /// Writes to the screen. `\n` starts a new line: the console adds the
    /// carriage return.
    fn write(&mut self, bytes: &[u8]);
    /// The screen's width in characters.
    fn columns(&self) -> usize;
    /// Whether Ctrl-C was pressed while a command runs. Long commands ask
    /// between pieces of work, so it must not wait for input. The default
    /// never interrupts.
    fn interrupted(&mut self) -> bool {
        false
    }
}

/// Memory figures for `free`, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemInfo {
    pub ram_total: u64,
    pub ram_free: u64,
    pub heap_total: u64,
    pub heap_used: u64,
}

/// The rest of the machine.
pub trait System {
    /// Wall-clock time in seconds since 1970, UTC.
    fn now(&self) -> u64;
    /// `None` where there are no figures to show (on the host).
    fn memory(&self) -> Option<MemInfo>;
    /// The kernel log, for `dmesg`.
    fn kernel_log(&self) -> Vec<u8>;
    /// Restarts the machine. Returns only where it cannot (on the host, in
    /// tests); the shell then stops.
    fn reboot(&mut self);
    /// Turns the machine off. Returns only where it cannot; the shell then
    /// stops.
    fn poweroff(&mut self);
    /// Starts the program at `path`, read through `vfs`, with `args`
    /// (argument 0 is the path); its pid. `None` where programs cannot run
    /// (on the host). The in-kernel shell's way to programs until the shell
    /// itself becomes one (user-space gate plan 4).
    fn spawn(
        &mut self,
        _vfs: &mut dyn Vfs,
        _path: &[u8],
        _args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        None
    }
    /// Runs the program `spawn` started until it ends, giving what it
    /// writes to fds 1 and 2 to `out`, whose answer (a redirection file's
    /// write error) is the program's, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
    }
}

/// Programs, for a shell that runs its commands as programs (`/bin/sh`,
/// user-space gate §8.2): `relay-rt`'s system calls there, a fake in the
/// tests.
pub trait Programs {
    /// Opens a redirection target for writing: created if missing, emptied
    /// or, with `append`, written at its end. Its fd.
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    fn close(&mut self, fd: u32);
    /// Starts the program at `path` with `args` (argument 0 first); its
    /// pid. It gets the shell's fds 0 and 2, and `stdout` (or the shell's
    /// fd 1) as its fd 1. With `foreground` it runs in a process group of
    /// its own, which gets the console (an interactive shell's command,
    /// spec §6.4); otherwise in the shell's group (a script's).
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno>;
    /// Waits for the child `pid` to end.
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
}
