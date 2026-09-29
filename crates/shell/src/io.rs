//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements both traits over its console, clock, memory
//! manager, log, ACPI and programs; `xtask host-shell` over the host
//! terminal; the tests over buffers.

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Vfs};

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
    /// writes to fds 1 and 2 to `out`, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
    }
}
