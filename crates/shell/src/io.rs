//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements both traits over its console, clock, memory
//! manager, log and ACPI; `xtask host-shell` over the host terminal; the
//! tests over buffers.

use alloc::vec::Vec;

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
}
