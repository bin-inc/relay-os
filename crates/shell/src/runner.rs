//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2), and a pipeline (§9.1): the `Runner`. The
//! in-process runner runs the command functions against the shell's `Vfs`,
//! `Console` and `System`, as milestone 1 does (the unit tests and `cargo
//! xtask host-shell`, where there are no programs), a pipeline's stages one
//! after another, each one's output kept in memory as the next one's
//! input; the spawning runner starts `/bin/<name>` for every one
//! (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::{Ctx, JobControl};
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
use crate::killed;
use crate::parser::{Command, Redirect};
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
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
    /// Standard input for a command run in the shell's process.
    pub input: Option<&'s mut dyn Stdin>,
}

/// How a command went.
pub(crate) struct Ran {
    pub status: i32,
    /// What the shell says after it: a write error, `^C`, how a program
    /// was killed.
    pub message: String,
    /// `exit`, or `reboot`/`poweroff` returning: the shell stops.
    pub stop: bool,
    /// It was `exit`, which stops only a script the shell runs itself.
    pub exited: bool,
    /// Set by `sh`: the script the shell runs next.
    pub script: Option<Script>,
}

impl Ran {
    pub fn said(status: i32, message: String) -> Ran {
        Ran {
            status,
            message,
            stop: false,
            exited: false,
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

    /// Runs a pipeline of two or more commands, none of them a built-in,
    /// the last one's output going to its redirection if it has one.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran;

    /// Starts a pipeline of one or more commands, none of them a built-in,
    /// in the background (spec §9.2), and does not wait for it.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started;
}

/// A background job's start.
pub(crate) struct Started {
    /// Its process group, if any of its commands started.
    pub pgid: Option<u32>,
    /// The processes that started, in order.
    pub pids: Vec<u32>,
    /// What could not start, and the status if nothing did.
    pub ran: Ran,
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

    /// The spawning runner's programs, which push and pop tees.
    pub fn programs(&mut self) -> Option<&mut dyn Programs> {
        match self {
            Runners::InProcess(_) => None,
            Runners::Spawning(r) => Some(&mut *r.programs),
        }
    }
}

/// The command functions of `commands::COMMANDS`, run in the shell's
/// process; any other name is not found.
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
            Some(command) => run_function(parts, command, args, file, None),
            None => not_found(name),
        }
    }

    /// Each stage runs to its end before the next starts, its output kept
    /// as the next one's input; a stage that is not found, or whose words
    /// expanded to nothing, gives the next one nothing, as bash's does (the
    /// first says so). Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
        if stages
            .iter()
            .any(|c| c.words.first().is_some_and(|w| w == "sh"))
        {
            return in_a_pipeline("sh");
        }
        let Parts {
            vfs,
            console,
            system,
            transcript,
            in_script,
            status,
            mut input,
        } = parts;
        let (last, before) = stages.split_last().expect("a pipeline has stages");
        let mut piped: Option<Bytes> = None;
        for stage in before {
            let mut out = Collected(Vec::new());
            let Some((name, args)) = stage.words.split_first() else {
                piped = Some(Bytes::new(out.0));
                continue;
            };
            if let Some(command) = commands::find(name) {
                let mut ctx = Ctx::program(&mut *vfs, &mut *system, &mut *console, &mut out);
                match (&mut piped, &mut input) {
                    (Some(bytes), _) => ctx.set_input(bytes),
                    (None, Some(first)) => ctx.set_input(&mut **first),
                    (None, None) => {}
                }
                ctx.transcript = transcript.take();
                (command.run)(&mut ctx, args);
                let _ = ctx.finish();
                let cancelled = ctx.cancelled;
                *transcript = ctx.transcript.take();
                if cancelled {
                    return Ran::said(CANCELLED, String::from("^C\n"));
                }
            } else {
                let message = not_found(name).message;
                console.write(message.as_bytes());
                if let Some(t) = transcript.as_mut() {
                    let _ = t.add(&mut *vfs, message.as_bytes());
                }
            }
            piped = Some(Bytes::new(out.0));
        }
        let mut piped = piped.expect("a stage before the last");
        let Some((name, args)) = last.words.split_first() else {
            return match redirect_to(&mut *vfs, last.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            };
        };
        let parts = Parts {
            vfs,
            console,
            system,
            transcript,
            in_script,
            status,
            input: Some(&mut piped),
        };
        self.run(parts, name, args, last.redirect.as_ref())
    }

    fn background(&mut self, _: Parts<'_>, _: &[Command]) -> Started {
        InProcess::refuse_background()
    }
}

impl InProcess {
    /// A background job needs programs: the in-process runner keeps
    /// refusing `&`, as milestone 1's shell did.
    fn refuse_background() -> Started {
        Started {
            pgid: None,
            pids: Vec::new(),
            ran: Ran::said(SYNTAX, format!("{NAME}: unsupported syntax: &\n")),
        }
    }
}

/// A stage's output in the in-process runner: kept whole for the next.
struct Collected(Vec<u8>);

impl Stdout for Collected {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn is_tty(&self) -> bool {
        false
    }
    fn node(&self) -> Option<Node> {
        None
    }
}

/// A command the shell runs itself, refused in a pipeline (§9.1).
pub(crate) fn in_a_pipeline(name: &str) -> Ran {
    Ran::said(1, format!("{NAME}: {name}: cannot be used in a pipeline\n"))
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
        let group = if parts.in_script {
            Group::Shell
        } else {
            Group::New
        };
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, None, stdout, group);
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

    /// Every stage is a program, started left to right in one process
    /// group (the first one's, which gets the console; a script's own),
    /// the pipe between two stages made just before the first of them
    /// starts. The shell closes its copies of a pipe's ends as soon as the
    /// stage that needs them has them, so that a reader sees the end of
    /// its input once its writer has ended, and a writer `EPIPE` once its
    /// reader has. A stage that cannot start says so at once and leaves
    /// its neighbours an end; the shell then waits for every stage, says
    /// how any killed one ended (a Ctrl-C once), and takes the last one's
    /// status.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        let in_script = parts.in_script;
        let (started, mut ran) = self.start(parts, stages, in_script, false);
        let mut cancelled = false;
        for (name, pid) in &started.pids {
            let ended = match self.programs.wait(*pid) {
                Ok(w) => ended(name, &w),
                Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
            };
            if Some(*pid) == started.last {
                ran.status = ended.status;
            }
            if ended.status == CANCELLED {
                cancelled = true;
            } else {
                ran.message.push_str(&ended.message);
            }
        }
        if cancelled {
            ran.message.push_str("^C\n");
        }
        ran
    }

    /// The stages start as a foreground pipeline's do, the first that
    /// starts in a new group without the console (at the prompt and in a
    /// script alike, so that Ctrl-C of the script does not reach it), the
    /// others joining it; nothing waits for them.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started {
        let (started, mut ran) = self.start(parts, stages, false, true);
        let pgid = started.pids.first().map(|&(_, pid)| pid);
        if pgid.is_some() {
            ran.status = 0;
        }
        Started {
            pgid,
            pids: started.pids.iter().map(|&(_, pid)| pid).collect(),
            ran,
        }
    }
}

/// The stages a pipeline started: their names and pids, and the last
/// stage's pid if it started.
struct Stages<'c> {
    pids: Vec<(&'c String, u32)>,
    last: Option<u32>,
}

impl Spawning<'_> {
    /// Starts `stages` left to right with pipes between them: in the
    /// shell's group (`in_script`), or the first that starts in a new one
    /// (with the console unless `background`) and the others joining it.
    /// What started, and what did not with its message (the status that
    /// of a last stage that could not start, or of a pipe that could not
    /// be made).
    fn start<'c>(
        &mut self,
        parts: Parts<'_>,
        stages: &'c [Command],
        in_script: bool,
        background: bool,
    ) -> (Stages<'c>, Ran) {
        let (last, _) = stages.split_last().expect("a pipeline has stages");
        let mut started = Stages {
            pids: Vec::new(),
            last: None,
        };
        let last_out = match &last.redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return (started, Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
            },
            None => None,
        };
        let (mut stdin, mut first) = (None, None);
        let mut ran = Ran::said(0, String::new());
        for (i, stage) in stages.iter().enumerate() {
            let is_last = i + 1 == stages.len();
            let (next, stdout) = if is_last {
                (None, last_out)
            } else {
                match self.programs.pipe() {
                    Ok((read, write)) => (Some(read), Some(write)),
                    Err(e) => {
                        for fd in [stdin, last_out].into_iter().flatten() {
                            self.programs.close(fd);
                        }
                        ran = Ran::said(1, format!("{NAME}: pipe error: {e}\n"));
                        break;
                    }
                }
            };
            let Some((name, args)) = stage.words.split_first() else {
                // Its words expanded to nothing: it runs nothing, and its
                // neighbours see an end.
                for fd in [stdin, stdout].into_iter().flatten() {
                    self.programs.close(fd);
                }
                stdin = next;
                continue;
            };
            let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
            argv.extend(args.iter().map(|w| w.as_bytes()));
            let group = match (in_script, background, first) {
                (_, true, None) => Group::Background,
                (true, false, _) => Group::Shell,
                (_, _, None) => Group::New,
                (_, _, Some(pgid)) => Group::Join(pgid),
            };
            let path = program_path(name);
            let pid = self
                .programs
                .spawn(path.as_bytes(), &argv, stdin, stdout, group);
            for fd in [stdin, stdout].into_iter().flatten() {
                self.programs.close(fd);
            }
            stdin = next;
            match pid {
                Ok(pid) => {
                    first.get_or_insert(pid);
                    started.pids.push((name, pid));
                    if is_last {
                        started.last = Some(pid);
                    }
                }
                Err(e) => {
                    let refused = cannot_start(name, e);
                    parts.console.write(refused.message.as_bytes());
                    if is_last {
                        ran.status = refused.status;
                    }
                }
            }
        }
        (started, ran)
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
pub(crate) fn run_function<'s>(
    parts: Parts<'s>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
    control: Option<JobControl<'s>>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.control = control;
    if let Some(input) = parts.input {
        ctx.set_input(input);
    }
    ctx.in_script = parts.in_script;
    ctx.status = parts.status;
    ctx.transcript = parts.transcript.take();
    let mut status = (command.run)(&mut ctx, args);
    let mut message = String::new();
    if let Err(e) = ctx.finish() {
        message = format!("{}: write error: {e}\n", command.name);
        status = ctx.write_error_status;
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
        exited: ctx.exited,
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

#[cfg(test)]
mod tests {
    use crate::Group;
    use crate::testing::{Harness, Spawned};
    use alloc::format;
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
                stdin: None,
                stdout: None,
                group: Group::New,
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
    fn names_of_directories_in_bin_are_not_commands() {
        let mut h = with_programs();
        for dir in ["/bin/..", "/bin/.", "/bin/"] {
            h.programs.refusals.push((dir, Errno::EISDIR));
        }
        h.programs.refusals.push(("./", Errno::EISDIR));
        for name in ["..", ".", "''"] {
            let shown = if name == "''" { "" } else { name };
            assert_eq!(
                h.spawning(name),
                (127, format!("relay-sh: {shown}: command not found\n")),
                "{name}"
            );
        }
        // Given as a path, a directory still says so.
        assert_eq!(
            h.spawning("./"),
            (126, "relay-sh: ./: Is a directory\n".into())
        );
    }

    #[test]
    fn a_name_too_long_for_a_file_is_not_found() {
        let mut h = with_programs();
        let long = "x".repeat(300);
        let (bare, path) = (format!("/bin/{long}"), format!("/{long}"));
        h.programs.refusals.push((bare.leak(), Errno::ENAMETOOLONG));
        h.programs
            .refusals
            .push((path.clone().leak(), Errno::ENAMETOOLONG));
        assert_eq!(
            h.spawning(&long),
            (127, format!("relay-sh: {long}: command not found\n")),
            "as bash says"
        );
        // Given as a path, the error is the path's.
        assert_eq!(
            h.spawning(&path),
            (126, format!("relay-sh: {path}: File name too long\n"))
        );
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

    /// `/bin/cat` exits with 0, `/bin/wc` with 0, `/bin/false` with 1.
    fn with_stages() -> Harness {
        let mut h = Harness::new();
        for (path, code) in [("/bin/cat", 0), ("/bin/wc", 0), ("/bin/false", 1)] {
            h.programs.known.push((path, WaitStatus::exited(code)));
        }
        h
    }

    fn stage(path: &str, args: &[&str], fds: (Option<u32>, Option<u32>), group: Group) -> Spawned {
        Spawned {
            path: path.into(),
            args: words(args),
            stdin: fds.0,
            stdout: fds.1,
            group,
        }
    }

    #[test]
    fn a_pipeline_s_stages_share_a_group_and_the_pipes_between_them() {
        let mut h = with_stages();
        assert_eq!(
            h.spawning("cat f | wc -l | false >> /tmp/o"),
            (1, String::new())
        );
        // The pipes, 4 and 5 then 6 and 7; the redirection, opened first.
        assert_eq!(h.programs.opened, [("/tmp/o".into(), true, 4)]);
        assert_eq!(h.programs.pipes, [(5, 6), (7, 8)]);
        let first = 101;
        assert_eq!(
            h.programs.spawned,
            [
                stage("/bin/cat", &["cat", "f"], (None, Some(6)), Group::New),
                stage(
                    "/bin/wc",
                    &["wc", "-l"],
                    (Some(5), Some(8)),
                    Group::Join(first)
                ),
                stage(
                    "/bin/false",
                    &["false"],
                    (Some(7), Some(4)),
                    Group::Join(first)
                ),
            ],
            "the first gets a group and the console, the others join it"
        );
        // Each end closed as soon as its stage has it.
        assert_eq!(h.programs.closed, [6, 5, 8, 7, 4]);
        // In a script, the script's group.
        let mut h = with_stages();
        h.put("/tmp/s.sh", b"cat f | wc\n");
        let mut out = crate::testing::FakeStdout::console();
        assert_eq!(h.sh(&["/tmp/s.sh"], &mut out).0, 0);
        let groups: Vec<Group> = h.programs.spawned.iter().map(|s| s.group).collect();
        assert_eq!(groups, [Group::Shell, Group::Shell]);
    }

    #[test]
    fn a_stage_that_cannot_start_says_so_and_the_others_run() {
        let mut h = with_stages();
        // As bash: `seq 5 | nosuch | wc -l` prints the error and 0.
        assert_eq!(
            h.spawning("cat f | nosuch | wc -l"),
            (0, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 2);
        assert_eq!(h.programs.pipes, [(4, 5), (6, 7)]);
        assert_eq!(
            h.programs.spawned[1].stdin,
            Some(6),
            "the second pipe's read end"
        );
        // Every end is closed all the same, so the others see their ends.
        let mut closed = h.programs.closed.clone();
        closed.sort();
        assert_eq!(closed, [4, 5, 6, 7]);
        // The first that starts leads the group.
        let mut h = with_stages();
        h.spawning("nosuch | cat | wc");
        assert_eq!(h.programs.spawned[0].group, Group::New);
        assert_eq!(h.programs.spawned[1].group, Group::Join(101));
        // The last one's refusal is the status.
        let mut h = with_stages();
        assert_eq!(
            h.spawning("cat | nosuch"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(
            h.spawning("cat | /no/such"),
            (
                127,
                "relay-sh: /no/such: No such file or directory\n".into()
            )
        );
    }

    #[test]
    fn a_pipeline_says_how_its_killed_stages_ended_and_ctrl_c_once() {
        let mut h = Harness::new();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1a2c);
        h.programs.known.push(("/bin/t-fault", fault));
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.programs
            .known
            .push(("/bin/t-spin", WaitStatus::killed(KILLED_CTRL_C)));
        // A writer that ended because nobody read on: 141, said by nobody.
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(141)));
        assert_eq!(
            h.spawning("t-fault null-read | cat"),
            (
                0,
                "relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x401a2c)\n".into()
            )
        );
        assert_eq!(h.spawning("t-spin | t-spin | t-spin"), (130, "^C\n".into()));
        assert_eq!(h.spawning("t-args x | cat"), (0, String::new()));
        assert_eq!(h.spawning("cat | t-args x"), (141, String::new()));
    }

    #[test]
    fn a_pipeline_that_cannot_be_made_starts_no_more() {
        let mut h = with_stages();
        h.programs.pipe_error = Some((1, Errno::EMFILE));
        assert_eq!(
            h.spawning("cat | wc | false > /tmp/o"),
            (1, "relay-sh: pipe error: Too many open files\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 1, "only the first");
        let mut closed = h.programs.closed.clone();
        closed.sort();
        assert_eq!(closed, [4, 5, 6], "the redirection and both ends");
        // A redirection that cannot be opened starts nothing.
        let mut h = with_stages();
        h.programs.open_error = Some(Errno::EISDIR);
        assert_eq!(
            h.spawning("cat | wc > /tmp"),
            (1, "relay-sh: /tmp: Is a directory\n".into())
        );
        assert!(h.programs.spawned.is_empty() && h.programs.pipes.is_empty());
    }

    #[test]
    fn every_program_is_followed_by_a_sync() {
        let mut h = with_programs();
        h.spawning("t-args");
        h.spawning("nosuch");
        assert_eq!(h.spy.syncs.get(), 2);
    }
}
