//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2): the `Runner`. The in-process runner runs the
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests, `cargo xtask host-shell` and the
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`); the spawning runner starts `/bin/<name>` for every
//! one (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, Programs, System};
use crate::killed;
use crate::parser::Redirect;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND};
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs};

/// What a runner works with: the shell's surroundings and its script.
pub(crate) struct Parts<'s> {
    pub vfs: &'s mut dyn Vfs,
    pub console: &'s mut dyn Console,
    pub system: &'s mut dyn System,
    /// A running script's transcript, which gets what the screen gets.
    pub transcript: &'s mut Option<Transcript>,
    /// The command is a line of a script.
    pub in_script: bool,
    /// The last command's exit status, for `exit`.
    pub status: i32,
}

/// How a command went.
pub(crate) struct Ran {
    pub status: i32,
    /// What the shell says after it: a write error, `^C`, how a program
    /// was killed.
    pub message: String,
    /// `exit`, or `reboot`/`poweroff` returning: the shell stops.
    pub stop: bool,
    /// Set by `sh`: the script the shell runs next.
    pub script: Option<Script>,
}

impl Ran {
    pub fn said(status: i32, message: String) -> Ran {
        Ran {
            status,
            message,
            stop: false,
            script: None,
        }
    }
}

/// Runs the commands that are not the shell's own (`cd`, `exit`, `help`).
pub(crate) trait Runner {
    /// Runs `name` with `args`, its standard output going to `redirect`
    /// if there is one.
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran;
}

/// The runners a shell may have. (A shell holds its runner in this rather
/// than in a box, so that it holds no destructor that would keep what it
/// borrows until it is dropped.)
pub(crate) enum Runners<'a> {
    InProcess(InProcess),
    Spawning(Spawning<'a>),
}

impl Runners<'_> {
    pub fn get(&mut self) -> &mut dyn Runner {
        match self {
            Runners::InProcess(r) => r,
            Runners::Spawning(r) => r,
        }
    }
}

/// The command functions of `commands::COMMANDS`, run in the shell's
/// process; any other name is a program, through `System::spawn`.
pub(crate) struct InProcess;

impl Runner for InProcess {
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let file = match redirect_to(&mut *parts.vfs, redirect) {
            Ok(file) => file,
            Err(ran) => return ran,
        };
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, file),
            None => run_program(parts, name, args, file),
        }
    }
}

/// Every command a program: `/bin/<name>`, or the path as given.
pub(crate) struct Spawning<'a> {
    pub programs: &'a mut dyn Programs,
}

impl Runner for Spawning<'_> {
    /// Redirections are opened in the shell and passed to the program as
    /// its fd 1, which the shell closes once the program has it. At the
    /// prompt a program gets a process group of its own and the console;
    /// in a script it runs in the shell's group, so that Ctrl-C ends the
    /// script with it (spec §6.4).
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let stdout = match redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
            None => None,
        };
        let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
        argv.extend(args.iter().map(|w| w.as_bytes()));
        let path = program_path(name);
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, stdout, !parts.in_script);
        if let Some(fd) = stdout {
            self.programs.close(fd);
        }
        match started {
            Ok(pid) => match self.programs.wait(pid) {
                Ok(w) => ended(name, &w),
                Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
            },
            Err(e) => cannot_start(name, e),
        }
    }
}

/// The redirection target, opened; a target that cannot be opened stops
/// the command.
pub(crate) fn redirect_to(
    vfs: &mut dyn Vfs,
    redirect: Option<&Redirect>,
) -> Result<Option<(Node, u64)>, Ran> {
    match redirect {
        Some(r) => match open_redirect(vfs, r) {
            Ok(file) => Ok(Some(file)),
            Err(e) => Err(Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
        },
        None => Ok(None),
    }
}

/// Opens a redirection target: created if missing, emptied for `>`,
/// written at its end for `>>`.
fn open_redirect(vfs: &mut dyn Vfs, r: &Redirect) -> Result<(Node, u64), Errno> {
    let path = r.path.as_bytes();
    let node = match vfs.lookup(path) {
        Ok(node) => {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            if !r.append {
                vfs.truncate(node, 0)?;
            }
            node
        }
        Err(Errno::ENOENT) => vfs.create(path)?,
        Err(e) => return Err(e),
    };
    let offset = if r.append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
}

/// Runs a command function with its standard output going to `file`, if
/// any.
pub(crate) fn run_function(
    parts: Parts<'_>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.in_script = parts.in_script;
    ctx.status = parts.status;
    ctx.transcript = parts.transcript.take();
    let mut status = (command.run)(&mut ctx, args);
    let mut message = String::new();
    if let Err(e) = ctx.finish() {
        message = format!("{}: write error: {e}\n", command.name);
        status = 1;
    }
    if ctx.cancelled {
        message = String::from("^C\n");
        status = CANCELLED;
    }
    *parts.transcript = ctx.transcript.take();
    Ran {
        status,
        message,
        stop: ctx.exit,
        script: ctx.script.take(),
    }
}

/// The status and message of a command that could not be started (user-
/// space gate §8.2): a bare name that names no program in `/bin` is not
/// found, as in bash; a path says why.
pub(crate) fn cannot_start(name: &str, e: Errno) -> Ran {
    // `..`, `.` and `''` name directories in /bin, which a search for a
    // command skips, as bash's does; a name too long for a file name is
    // no file in /bin either.
    if !name.contains('/') && matches!(e, Errno::ENOENT | Errno::EISDIR | Errno::ENAMETOOLONG) {
        return not_found(name);
    }
    let status = if e == Errno::ENOENT {
        NOT_FOUND
    } else {
        CANNOT_RUN
    };
    Ran::said(status, format!("{NAME}: {name}: {e}\n"))
}

pub(crate) fn not_found(name: &str) -> Ran {
    Ran::said(NOT_FOUND, format!("{NAME}: {name}: command not found\n"))
}

/// The program's path: `/bin/<name>`, or `name` itself when it holds a
/// `/`.
pub(crate) fn program_path(name: &str) -> String {
    if name.contains('/') {
        String::from(name)
    } else {
        format!("/bin/{name}")
    }
}

/// The status of a program that ended, and what the shell says if it did
/// not exit by itself: Ctrl-C says only `^C`, as for a built-in, and stops
/// a script (spec §6.4).
pub(crate) fn ended(name: &str, w: &relay_abi::WaitStatus) -> Ran {
    if w.how == relay_abi::wait::EXITED {
        return Ran::said(w.code as i32, String::new());
    }
    let (what, status) = killed::killed(w);
    let message = if status == CANCELLED {
        String::from("^C\n")
    } else {
        format!("{NAME}: {name}: {what}\n")
    };
    Ran::said(status, message)
}

/// Runs a program through `System::spawn` (user-space gate §8.2), with
/// `name` as argument 0 and `args` as the others, as bash does. Its fd 1
/// is standard output (the redirection file, if any), its fd 2 the screen;
/// a running script's transcript gets both.
fn run_program(parts: Parts<'_>, name: &str, args: &[String], file: Option<(Node, u64)>) -> Ran {
    let path = program_path(name);
    let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
    argv.extend(args.iter().map(|w| w.as_bytes()));
    let pid = match parts.system.spawn(&mut *parts.vfs, path.as_bytes(), &argv) {
        Some(Ok(pid)) => pid,
        // No programs here (the host).
        None => return not_found(name),
        Some(Err(e)) => return cannot_start(name, e),
    };
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.transcript = parts.transcript.take();
    let mut ran = match ctx.wait_program(pid) {
        Ok(w) => ended(name, &w),
        Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
    };
    if let Err(e) = ctx.finish() {
        // A program that did not exit keeps its report and status.
        if ran.message.is_empty() {
            ran.status = 1;
        }
        ran.message
            .insert_str(0, &format!("{name}: write error: {e}\n"));
    }
    *parts.transcript = ctx.transcript.take();
    ran
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, Spawned};
    use alloc::string::String;
    use alloc::vec::Vec;
    use relay_abi::WaitStatus;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_CTRL_C};
    use vfs::Errno;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| String::from(*s)).collect()
    }

    /// `/bin/t-args` exits with 3; `/bin/cat` with 0.
    fn with_programs() -> Harness {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(3)));
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h
    }

    #[test]
    fn every_command_but_cd_exit_and_help_is_a_program() {
        let mut h = with_programs();
        assert_eq!(h.spawning("t-args a 'b c' ''"), (3, String::new()));
        assert_eq!(
            h.programs.spawned,
            [Spawned {
                path: "/bin/t-args".into(),
                args: words(&["t-args", "a", "b c", ""]),
                stdout: None,
                foreground: true,
            }],
            "argument 0 is the name as typed; at the prompt it gets the console"
        );
        // `cat` is a program too, not the command function.
        h.put("/tmp/f", b"in the file\n");
        assert_eq!(h.spawning("cat /tmp/f"), (0, String::new()));
        assert_eq!(h.programs.spawned[1].path, "/bin/cat");
        // The shell's own commands run in it.
        assert_eq!(h.spawning("cd /tmp"), (0, String::new()));
        assert_eq!(h.spawning("help").0, 0);
        assert_eq!(h.programs.spawned.len(), 2);
        assert_eq!(h.run("pwd").1, "/tmp\n");
    }

    #[test]
    fn a_path_runs_as_given() {
        let mut h = with_programs();
        h.programs.known.push(("./t-args", WaitStatus::exited(5)));
        assert_eq!(h.spawning("./t-args x").0, 5);
        assert_eq!(h.programs.spawned[0].args, words(&["./t-args", "x"]));
        assert_eq!(h.spawning("/bin/t-args").0, 3);
    }

    #[test]
    fn a_redirection_is_opened_in_the_shell_and_passed_as_fd_1() {
        let mut h = with_programs();
        assert_eq!(h.spawning("t-args > /tmp/out").0, 3);
        h.spawning("t-args >> /tmp/out");
        assert_eq!(
            h.programs.opened,
            [("/tmp/out".into(), false, 4), ("/tmp/out".into(), true, 5)]
        );
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.stdout).collect();
        assert_eq!(fds, [Some(4), Some(5)]);
        assert_eq!(h.programs.closed, [4, 5], "the shell keeps no copy");
        // Opened before the program starts, and closed when it cannot.
        assert_eq!(h.spawning("nosuch > /tmp/o").0, 127);
        assert_eq!(h.programs.closed, [4, 5, 6]);
    }

    #[test]
    fn a_redirection_that_cannot_open_starts_nothing() {
        let mut h = with_programs();
        h.programs.open_error = Some(Errno::EISDIR);
        assert_eq!(
            h.spawning("t-args > /tmp"),
            (1, "relay-sh: /tmp: Is a directory\n".into())
        );
        assert!(h.programs.spawned.is_empty());
        // A bare redirection is the shell's alone.
        h.programs.open_error = None;
        assert_eq!(h.spawning("> /tmp/new"), (0, String::new()));
        assert!(h.exists("/tmp/new") && h.programs.opened.is_empty());
    }

    #[test]
    fn what_cannot_start_says_why() {
        let mut h = with_programs();
        h.programs.refusals.push(("/root/text", Errno::ENOEXEC));
        h.programs.refusals.push(("/root", Errno::EISDIR));
        h.programs.refusals.push(("/bin/..", Errno::EISDIR));
        for (line, status, said) in [
            ("nosuch", 127, "relay-sh: nosuch: command not found\n"),
            ("..", 127, "relay-sh: ..: command not found\n"),
            (
                "/root/nosuch",
                127,
                "relay-sh: /root/nosuch: No such file or directory\n",
            ),
            (
                "/root/text",
                126,
                "relay-sh: /root/text: Exec format error\n",
            ),
            ("/root", 126, "relay-sh: /root: Is a directory\n"),
        ] {
            assert_eq!(h.spawning(line), (status, said.into()), "{line}");
        }
    }

    #[test]
    fn a_killed_program_is_reported_and_ctrl_c_is_only_so() {
        let mut h = Harness::new();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1a2c);
        h.programs.known.push(("/bin/t-fault", fault));
        h.programs
            .known
            .push(("/bin/t-spin", WaitStatus::killed(KILLED_CTRL_C)));
        assert_eq!(
            h.spawning("t-fault null-read > /tmp/o"),
            (
                139,
                "relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x401a2c)\n".into()
            )
        );
        assert_eq!(h.spawning("t-spin"), (130, "^C\n".into()));
    }

    #[test]
    fn every_program_is_followed_by_a_sync() {
        let mut h = with_programs();
        h.spawning("t-args");
        h.spawning("nosuch");
        assert_eq!(h.spy.syncs.get(), 2);
    }
}
