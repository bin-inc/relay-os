//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2): the `Runner`. The in-process runner runs the
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests, `cargo xtask host-shell` and the
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, System};
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
pub(crate) enum Runners {
    InProcess(InProcess),
}

impl Runners {
    pub fn get(&mut self) -> &mut dyn Runner {
        match self {
            Runners::InProcess(r) => r,
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
