//! What the shell needs from its surroundings besides files (spec §7.3).
//! `relay-rt` implements `Console`, `System`, `Stdin`, `Stdout` and
//! `Programs` over system calls; `xtask host-shell` `Console` over the host
//! terminal and `System` over the host; the tests over buffers and fakes. `Programs`
//! is `/bin/sh`'s way to its commands. `Stdin` is a command's standard
//! input: a program's fd 0, or bytes in memory (`Bytes`).

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Node};

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
    /// The interactive shell takes the console back (its own group, raw
    /// mode) as it does before it reads, once a command it gave the
    /// console to has ended, so that a Ctrl-C typed before its next prompt
    /// is its own (programmable shell gate §15 item 2). The default does
    /// nothing.
    fn take_back(&mut self) {}
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
    /// Every process, by pid, for `ps`; `None` where there are none to
    /// show (on the host).
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>>;
    /// Waits `ms` milliseconds (`sleep`).
    fn sleep(&mut self, ms: u64);
    /// Restarts the machine, going ahead with `force` when the filesystems
    /// cannot be shut down cleanly. Returns only where it cannot (on the
    /// host, in tests), and the shell then stops; or with the error that
    /// kept the machine up (a program's `power` call, which shuts the
    /// filesystems down itself).
    fn reboot(&mut self, force: bool) -> Result<(), Errno>;
    /// Turns the machine off, as `reboot` restarts it.
    fn poweroff(&mut self, force: bool) -> Result<(), Errno>;
}

/// A command's standard input (user-space gate §9.1): a program's fd 0 (the
/// console, in line mode, or a pipe), or bytes in memory.
pub trait Stdin {
    /// Reads some bytes into `buf`: how many, 0 at the end of the input.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
}

/// Standard input that is bytes in memory: what a test gives a command, or
/// what the stage before wrote, in the in-process runner's pipelines.
pub struct Bytes {
    data: Vec<u8>,
    at: usize,
}

impl Bytes {
    pub fn new(data: Vec<u8>) -> Bytes {
        Bytes { data, at: 0 }
    }
}

impl Stdin for Bytes {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        let n = buf.len().min(self.data.len() - self.at);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// A program's standard output, fd 1 (user-space gate §8.1): the console
/// or a file the shell opened for it.
pub trait Stdout {
    /// Writes all of `bytes`; the error that stopped it (`ENOSPC` for a
    /// write that took nothing).
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno>;
    /// Whether it is the console (`ls` lays out columns then).
    fn is_tty(&self) -> bool;
    /// The file it is, as the `Vfs` names files (`cat f >> f` must not
    /// read its own output).
    fn node(&self) -> Option<Node>;
}

/// The process group a program starts in (user-space gate §6.4, §9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// The shell's own: a script's commands, so that Ctrl-C ends the
    /// script with them.
    Shell,
    /// A new one, which gets the console: a command at the prompt, or a
    /// pipeline's first.
    New,
    /// The group of the pipeline's first command (its pid), which has
    /// the console already, or, in a background job, does not.
    Join(u32),
    /// A new one, without the console: a background job's first command
    /// (§9.2), at the prompt and in a script alike.
    Background,
}

/// Programs, for a shell that runs its commands as programs (`/bin/sh`,
/// user-space gate §8.2): `relay-rt`'s system calls there, a fake in the
/// tests.
pub trait Programs {
    /// Opens a redirection target for writing: created if missing, emptied
    /// or, with `append`, written at its end. Its fd.
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    fn close(&mut self, fd: u32);
    /// Makes a pipe: its read end and its write end.
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) in
    /// `group`; its pid. It gets `stdin` (or the shell's fd 0) as its fd 0,
    /// `stdout` (or the shell's fd 1) as its fd 1, and the shell's fd 2.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
    ) -> Result<u32, Errno>;
    /// Waits for the child `pid` to end.
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// A child that has ended, collected, if one has: a background job's
    /// process (`wait(-1, NOHANG)`).
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Kills the process `target`, or the process group `-target`
    /// (`kill`): `ESRCH` if there is none, `EPERM` for process 1.
    fn kill(&mut self, target: i64) -> Result<(), Errno>;
    /// Waits for the child `pid` to end, as `wait` does, unless a Ctrl-C
    /// is typed at the shell's prompt meanwhile (`EINTR`, `WAIT_CTRL_C`).
    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// Copies the console into the file at `path` from now on (a script's
    /// transcript, spec §6.5), written at its end.
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno>;
    /// Stops the copy `tee_push` started; the error of a write that failed
    /// meanwhile, which ended the copying there.
    fn tee_pop(&mut self) -> Result<(), Errno>;
}
