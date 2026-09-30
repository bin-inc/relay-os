//! The shell's `Console` and `System`, and a program's standard output,
//! over system calls (user-space gate §8.1), and the `main` of `/bin`'s
//! command programs.

use crate::Args;
use crate::sys;
use crate::sysvfs::{SysVfs, node_of};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use relay_abi::console::MODE_RAW;
use relay_abi::file::KIND_CHAR_DEVICE;
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use shell::{Console, MemInfo, Stdout, System};
use vfs::{Errno, Node};

/// The console: what is typed on fd 0, and the screen on fd 2 (a
/// program's errors, and the shell's prompt and messages, as bash writes
/// them).
pub struct SysConsole {
    /// The process group that takes the console before every read (an
    /// interactive shell's own, spec §6.4).
    owner: Option<u32>,
}

impl SysConsole {
    /// The console of a program that does not read it.
    pub fn new() -> SysConsole {
        SysConsole { owner: None }
    }

    /// The console of an interactive shell, whose process group `pgid`
    /// takes it back, in raw mode, before every read, whatever a program
    /// left it in (spec §6.4, §16 item 4).
    pub fn owned_by(pgid: u32) -> SysConsole {
        SysConsole { owner: Some(pgid) }
    }
}

impl Default for SysConsole {
    fn default() -> SysConsole {
        SysConsole::new()
    }
}

impl Console for SysConsole {
    fn read_byte(&mut self) -> Option<u8> {
        if let Some(pgid) = self.owner {
            let _ = sys::console_mode(MODE_RAW);
            let _ = sys::console_foreground(pgid);
        }
        let mut byte = [0];
        match sys::read(0, &mut byte) {
            Ok(1) => Some(byte[0]),
            _ => None,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        let _ = sys::write_all(2, bytes);
    }

    fn columns(&self) -> usize {
        sys::console_size().0 as usize
    }
}

/// The clock, memory figures, kernel log and `power`.
pub struct SysSystem;

/// The kernel's memory figures as `free` shows them.
pub fn mem_info(m: relay_abi::MemInfo) -> MemInfo {
    MemInfo {
        ram_total: m.ram_total,
        ram_free: m.ram_free,
        heap_total: m.heap_total,
        heap_used: m.heap_used,
    }
}

/// `power`'s flags for `reboot -f` and `poweroff -f`.
pub fn power_flags(force: bool) -> u32 {
    if force { POWER_FORCE } else { 0 }
}

impl System for SysSystem {
    fn now(&self) -> u64 {
        sys::time().map_or(0, |t| t.unix_seconds)
    }

    fn memory(&self) -> Option<MemInfo> {
        sys::memory().ok().map(mem_info)
    }

    fn kernel_log(&self) -> Vec<u8> {
        let mut buf = vec![0; LOG_MAX];
        let n = sys::kernel_log(&mut buf).unwrap_or(0);
        buf.truncate(n);
        buf
    }

    /// `power` returns only when the machine stays up.
    fn reboot(&mut self, force: bool) -> Result<(), Errno> {
        Err(Errno::from_number(sys::power(
            POWER_REBOOT,
            power_flags(force),
        )))
    }

    fn poweroff(&mut self, force: bool) -> Result<(), Errno> {
        Err(Errno::from_number(sys::power(
            POWER_POWEROFF,
            power_flags(force),
        )))
    }
}

/// Standard output: fd 1, the console or the file the shell opened.
pub struct SysStdout {
    tty: bool,
}

impl SysStdout {
    pub fn new() -> SysStdout {
        let tty = sys::fstat(1).is_ok_and(|st| st.kind == u32::from(KIND_CHAR_DEVICE));
        SysStdout { tty }
    }
}

impl Default for SysStdout {
    fn default() -> SysStdout {
        SysStdout::new()
    }
}

impl Stdout for SysStdout {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        sys::write_all(1, bytes).map_err(Errno::from_number)
    }

    fn is_tty(&self) -> bool {
        self.tty
    }

    /// A file's `stat` names its filesystem (the console's is 0).
    fn node(&self) -> Option<Node> {
        sys::fstat(1)
            .ok()
            .filter(|st| st.dev != 0)
            .map(|st| node_of(&st))
    }
}

/// A program's arguments after argument 0, as the command functions take
/// them (bytes that are not UTF-8 are replaced, as they could not be typed
/// at the shell's prompt).
pub fn words(args: &Args) -> Vec<String> {
    args.iter()
        .skip(1)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect()
}

/// The `main` of one of `/bin`'s commands (user-space gate §8.4): runs the
/// command `name`, whose function is `run`, with the program's arguments,
/// printing exactly what it prints in the shell; its exit status.
pub fn run_command(name: &str, run: shell::commands::Run, args: &Args) -> u8 {
    let status = shell::run_command(
        name,
        run,
        &words(args),
        &mut SysVfs::new(),
        &mut SysConsole::new(),
        &mut SysSystem,
        &mut SysStdout::new(),
    );
    status as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_figures_pass_through() {
        let m = relay_abi::MemInfo {
            ram_total: 1,
            ram_free: 2,
            heap_total: 3,
            heap_used: 4,
        };
        assert_eq!(
            mem_info(m),
            MemInfo {
                ram_total: 1,
                ram_free: 2,
                heap_total: 3,
                heap_used: 4,
            }
        );
    }

    #[test]
    fn dash_f_is_power_force() {
        assert_eq!(power_flags(true), POWER_FORCE);
        assert_eq!(power_flags(false), 0);
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
        let args = Args::new(b"cat\0a b\0\xff\0\0", 4);
        assert_eq!(words(&args), ["a b", "\u{fffd}", ""]);
    }
}
