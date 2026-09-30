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
use relay_abi::file::{KIND_CHAR_DEVICE, OPEN_APPEND, OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Programs, Stdout, System};
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

/// `/bin/sh`'s way to its commands (user-space gate §8.2, §8.3): `spawn`,
/// `wait`, and the tees of a script's transcript.
pub struct SysPrograms;

/// `open`'s flags for a redirection: `>` empties the file, `>>` writes at
/// its end.
pub fn output_flags(append: bool) -> u32 {
    OPEN_WRITE | OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE }
}

/// `spawn`'s arguments: each followed by a NUL.
pub fn arg_bytes(args: &[&[u8]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for a in args {
        bytes.extend_from_slice(a);
        bytes.push(0);
    }
    bytes
}

/// A command's fds: the shell's 0 and 2, and `stdout` or the shell's 1.
pub fn command_fds(stdout: Option<u32>) -> [FdMap; 3] {
    [(0, 0), (1, stdout.unwrap_or(1)), (2, 2)].map(|(child, parent)| FdMap { child, parent })
}

impl Programs for SysPrograms {
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno> {
        sys::open(path, output_flags(append)).map_err(Errno::from_number)
    }

    fn close(&mut self, fd: u32) {
        let _ = sys::close(fd);
    }

    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
        let flags = if foreground {
            NEW_GROUP | FOREGROUND
        } else {
            0
        };
        sys::spawn(path, &arg_bytes(args), b"", &command_fds(stdout), flags)
            .map_err(Errno::from_number)
    }

    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        match sys::wait(i64::from(pid), false) {
            Ok(Some((_, status))) => Ok(status),
            Ok(None) => Err(Errno::ECHILD),
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    /// The tee holds the open file (spec §16 item 4), so the fd goes.
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno> {
        let fd = sys::open(path, OPEN_WRITE).map_err(Errno::from_number)?;
        let pushed = sys::console_tee_push(fd).map_err(Errno::from_number);
        let _ = sys::close(fd);
        pushed
    }

    fn tee_pop(&mut self) -> Result<(), Errno> {
        sys::console_tee_pop().map_err(Errno::from_number)
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
    fn a_redirection_is_created_and_emptied_or_appended_to() {
        assert_eq!(
            output_flags(false),
            OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE
        );
        assert_eq!(output_flags(true), OPEN_WRITE | OPEN_CREATE | OPEN_APPEND);
    }

    #[test]
    fn a_command_gets_the_shell_s_fds_but_its_redirection() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds(None)), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds(Some(5))), [(0, 0), (1, 5), (2, 2)]);
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
        let args = Args::new(b"cat\0a b\0\xff\0\0", 4);
        assert_eq!(words(&args), ["a b", "\u{fffd}", ""]);
    }
}
