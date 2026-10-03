//! The shell's `Console` and `System`, and a program's standard output,
//! over system calls (user-space gate §8.1), and the `main` of `/bin`'s
//! command programs.

use crate::Args;
use crate::sys;
use crate::sysvfs::{SysVfs, node_of};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use relay_abi::console::{MODE_LINE, MODE_RAW};
use relay_abi::file::{
    KIND_CHAR_DEVICE, KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
    SEEK_CURRENT,
};
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_abi::{FdMap, Stat, WaitStatus};
use shell::{Console, Group, MemInfo, Programs, Stdin, Stdout, System};
use vfs::{Errno, Node};

/// The console: what is typed on fd 0, and the screen on fd 2 (a
/// program's errors, and the shell's prompt and messages, as bash writes
/// them).
pub struct SysConsole {
    /// An interactive shell's: it takes the console back, in raw mode,
    /// before every read.
    interactive: bool,
    /// The process group that takes it (the shell's own, spec §6.4).
    group: Option<u32>,
}

impl SysConsole {
    /// The console of a program that does not read it.
    pub fn new() -> SysConsole {
        SysConsole {
            interactive: false,
            group: None,
        }
    }

    /// The console of an interactive shell, which takes it back, in raw
    /// mode, before every read, whatever a program left it in (spec §6.4,
    /// §16 item 4): for its process group `group` when it leads one, which
    /// its commands' groups had; otherwise it shares a group, a script's,
    /// with its commands, which never take the console from it.
    pub fn interactive(group: Option<u32>) -> SysConsole {
        SysConsole {
            interactive: true,
            group,
        }
    }
}

impl Default for SysConsole {
    fn default() -> SysConsole {
        SysConsole::new()
    }
}

impl Console for SysConsole {
    fn read_byte(&mut self) -> Option<u8> {
        self.take_back();
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

    fn is_screen(&self) -> bool {
        sys::fstat(2).is_ok_and(|st| is_console(&st))
    }

    /// Only an interactive shell keeps the console in raw mode while its
    /// own commands run, a Ctrl-C there being input to ask the kernel for;
    /// another program's group has it in line mode, where Ctrl-C kills it.
    fn interrupted(&mut self) -> bool {
        self.interactive && sys::take_ctrl_c()
    }

    fn take_back(&mut self) {
        if self.interactive {
            let _ = sys::console_mode(MODE_RAW);
        }
        if let Some(pgid) = self.group {
            let _ = sys::console_foreground(pgid);
        }
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

    fn is_terminal(&self, fd: u32) -> bool {
        sys::fstat(fd).is_ok_and(|st| is_console(&st))
    }

    fn kernel_log(&self) -> Vec<u8> {
        let mut buf = vec![0; LOG_MAX];
        let n = sys::kernel_log(&mut buf).unwrap_or(0);
        buf.truncate(n);
        buf
    }

    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        let mut buf = [relay_abi::ProcInfo::new(0, 0, 0, 0, 0, 0, b""); relay_abi::proc::PROC_MAX];
        let n = sys::proc_list(&mut buf).ok()?;
        Some(buf[..n.min(buf.len())].to_vec())
    }

    fn sleep(&mut self, ms: u64) {
        sys::sleep(ms);
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

/// Whether `fstat` says this is the console: the character device on
/// filesystem 0, the only one there is.
pub fn is_console(st: &Stat) -> bool {
    st.kind == u32::from(KIND_CHAR_DEVICE) && st.dev == 0
}

/// Standard input: fd 0, the console (in line mode, as the shell gives it
/// to its command) or a pipe.
pub struct SysStdin;

impl SysStdin {
    /// Whether fd 0 is the console (`fstat` says a character device).
    pub fn is_console() -> bool {
        sys::fstat(0).is_ok_and(|st| is_console(&st))
    }
}

impl Stdin for SysStdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        sys::read(0, buf).map_err(Errno::from_number)
    }

    fn file(&mut self) -> Option<(Node, u64, u64)> {
        let st = sys::fstat(0)
            .ok()
            .filter(|st| st.kind == u32::from(KIND_REGULAR))?;
        let at = sys::seek(0, 0, SEEK_CURRENT).ok()?;
        Some((node_of(&st), at, st.size))
    }

    fn seek_back(&mut self, n: u64) -> Result<(), Errno> {
        let back = i64::try_from(n).map_err(|_| Errno::EINVAL)?;
        sys::seek(0, -back, SEEK_CURRENT)
            .map(|_| ())
            .map_err(Errno::from_number)
    }
}

/// Standard output: fd 1, the console or the file the shell opened.
pub struct SysStdout {
    tty: bool,
}

impl SysStdout {
    pub fn new() -> SysStdout {
        let tty = sys::fstat(1).is_ok_and(|st| is_console(&st));
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
            .filter(|st| st.kind == u32::from(KIND_REGULAR))
            .map(|st| node_of(&st))
    }
}

/// `/bin/sh`'s way to its commands (user-space gate §8.2, §8.3): `spawn`,
/// `wait`, and the tees of a script's transcript. A command always starts
/// with the console in line mode, which `FOREGROUND` sets for one in a
/// group of its own.
pub struct SysPrograms {
    /// The shell leads a process group of its own, so a command at its
    /// prompt may have one too, with the console; a shell in a script's
    /// group keeps its commands there (and the console with them).
    own_group: bool,
    /// The last child started in a group of its own, which a pipeline's
    /// later commands join.
    leader: Option<u32>,
}

impl SysPrograms {
    pub fn new(own_group: bool) -> SysPrograms {
        SysPrograms {
            own_group,
            leader: None,
        }
    }
}

/// `open`'s flags for a redirection: `>` empties the file, `>>` writes at
/// its end.
pub fn output_flags(append: bool) -> u32 {
    OPEN_WRITE | OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE }
}

/// `spawn`'s flags and group for a command in `group`, from a shell that
/// leads a group of its own (`own_group`) or not, whose last child in a
/// new group was `leader`: a new group with the console only from a
/// leading shell; a background job's without it, from any shell; a
/// pipeline's later stages in the group its first one started, or else
/// the shell's.
pub fn spawn_group(group: Group, own_group: bool, leader: Option<u32>) -> (u32, u32) {
    match group {
        Group::New if own_group => (NEW_GROUP | FOREGROUND, 0),
        // Without the console, which stays where it is (spec §9.2).
        Group::Background => (NEW_GROUP, 0),
        Group::Join(pgid) if leader == Some(pgid) => (0, pgid),
        _ => (0, 0),
    }
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

/// A command's fds 0, 1 and 2: the shell's `fds`.
pub fn command_fds(fds: [u32; 3]) -> [FdMap; 3] {
    [0, 1, 2].map(|child| FdMap {
        child,
        parent: fds[child as usize],
    })
}

impl Programs for SysPrograms {
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno> {
        sys::open(path, output_flags(append)).map_err(Errno::from_number)
    }

    fn open_input(&mut self, path: &[u8]) -> Result<u32, Errno> {
        sys::open(path, OPEN_READ).map_err(Errno::from_number)
    }

    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
        let mut done = 0;
        while done < bytes.len() {
            match sys::write(fd, &bytes[done..]) {
                Ok(0) => return Err(Errno::ENOSPC),
                Ok(n) => done += n,
                Err(e) => return Err(Errno::from_number(e)),
            }
        }
        Ok(())
    }

    fn close(&mut self, fd: u32) {
        let _ = sys::close(fd);
    }

    fn pipe(&mut self) -> Result<(u32, u32), Errno> {
        sys::pipe().map_err(Errno::from_number)
    }

    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        fds: [u32; 3],
        group: Group,
    ) -> Result<u32, Errno> {
        let (flags, pgid) = spawn_group(group, self.own_group, self.leader);
        if flags & NEW_GROUP == 0 && pgid == 0 {
            // A command in the shell's own group reads the console in line
            // mode too, which is also where Ctrl-C ends it (and the group):
            // an interactive shell that leads no group left it raw at its
            // prompt.
            let _ = sys::console_mode(MODE_LINE);
        }
        let fds = command_fds(fds);
        // This program's own environment, until the shell exports
        // variables (programmable shell gate §15 item 6).
        let env = crate::env::block();
        let pid = sys::spawn_env(path, &arg_bytes(args), env, b"", &fds, flags, pgid)
            .map_err(Errno::from_number)?;
        if flags & NEW_GROUP != 0 {
            self.leader = Some(pid);
        }
        Ok(pid)
    }

    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        match sys::wait(i64::from(pid), false) {
            Ok(Some((_, status))) => Ok(status),
            Ok(None) => Err(Errno::ECHILD),
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        sys::kill(target).map_err(Errno::from_number)
    }

    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        match sys::wait_with(i64::from(pid), relay_abi::spawn::WAIT_CTRL_C) {
            Ok(Some((_, status))) => Ok(status),
            Ok(None) => Err(Errno::ECHILD),
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
        sys::wait(relay_abi::spawn::WAIT_ANY, true).ok().flatten()
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
    let io = shell::CommandIo {
        vfs: &mut SysVfs::new(),
        console: &mut SysConsole::new(),
        system: &mut SysSystem,
        stdin: &mut SysStdin,
        stdout: &mut SysStdout::new(),
    };
    let status = shell::run_command(name, run, &words(args), io);
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
    fn the_console_is_the_character_device_of_filesystem_0() {
        let console = Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
            dev: 0,
            ..Stat::default()
        };
        assert!(is_console(&console));
        // A device on a disk, and anything else on filesystem 0.
        assert!(!is_console(&Stat { dev: 1, ..console }));
        let fifo = u32::from(relay_abi::file::KIND_FIFO);
        assert!(!is_console(&Stat {
            kind: fifo,
            ..console
        }));
        assert!(!is_console(&Stat::default()));
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
    fn a_command_gets_the_shell_s_fds_it_is_given() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds([0, 1, 2])), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds([4, 7, 7])), [(0, 4), (1, 7), (2, 7)]);
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
    }

    #[test]
    fn a_command_s_group_is_the_shell_s_a_new_one_or_its_first_stage_s() {
        // At the prompt of a shell leading its group: a command gets the
        // console; a pipeline's later stages join its first.
        assert_eq!(
            spawn_group(Group::New, true, None),
            (NEW_GROUP | FOREGROUND, 0)
        );
        assert_eq!(spawn_group(Group::Join(7), true, Some(7)), (0, 7));
        // A background job never gets the console, from any shell.
        for own in [true, false] {
            assert_eq!(spawn_group(Group::Background, own, None), (NEW_GROUP, 0));
            assert_eq!(
                spawn_group(Group::Join(9), own, Some(9)),
                (0, 9),
                "its later stages"
            );
        }
        // A shell in a script's group keeps its commands there, a
        // pipeline's later stages too (their first did not lead a group).
        assert_eq!(spawn_group(Group::New, false, None), (0, 0));
        assert_eq!(spawn_group(Group::Join(12), false, Some(9)), (0, 0));
        assert_eq!(spawn_group(Group::Shell, true, Some(12)), (0, 0));
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
        let args = Args::new(b"cat\0a b\0\xff\0\0", 4);
        assert_eq!(words(&args), ["a b", "\u{fffd}", ""]);
    }
}
