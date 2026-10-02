//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command or a program and syncing the filesystems after it
//! (spec §7.3, §8.3; user-space gate §8.2).

use crate::commands::{self, SCRIPT_MAX, Script};
use crate::ctx::{Ctx, JobControl, quote_if_needed};
use crate::editor::{Feed, LineEditor};
use crate::expand::{self, Vars};
use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::jobs::Jobs;
use crate::parser::{self, HOME};
use crate::reader::Reader;
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::{self, Transcript};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Vfs, path};

/// The name the shell uses in its own messages.
pub const NAME: &str = "relay-sh";
/// Exit status of an unknown command.
pub const NOT_FOUND: i32 = 127;
/// Exit status of a program that could not be started (bash's).
pub const CANNOT_RUN: i32 = 126;
/// Exit status of a line that does not parse.
pub const SYNTAX: i32 = 2;
/// Exit status after Ctrl-C.
pub const CANCELLED: i32 = 130;
/// How much of a line over 64 KiB `X | sh` keeps: its last bytes, between
/// this and twice it.
const LINE_TAIL: usize = 1024;
/// The most of `/etc/motd` shown at start.
const MOTD_MAX: usize = 16 * 1024;

pub struct Shell<'a> {
    vfs: &'a mut dyn Vfs,
    console: &'a mut dyn Console,
    system: &'a mut dyn System,
    runner: Runners<'a>,
    editor: LineEditor,
    status: i32,
    stopped: bool,
    /// The last command was `exit`.
    exited: bool,
    /// A script's lines are running (`sh`).
    in_script: bool,
    /// Where a running script's screen output is copied.
    transcript: Option<Transcript>,
    /// The in-process runner's standard input for its commands (a test's
    /// bytes); without it the input ends at once (`host-shell`). A
    /// spawning shell's commands read its fd 0 instead.
    input: Option<&'a mut dyn Stdin>,
    /// The background jobs (spec §9.2).
    jobs: Jobs,
    /// It reads commands at its prompt (`run`): it says a job's number
    /// when it starts one, and how jobs ended before each prompt. A script
    /// and `X | sh` say neither, as bash's do.
    prompting: bool,
    /// Its variables and arguments (spec §9.4).
    vars: Vars,
    /// An expansion failed in a way that abandons the rest of the line
    /// (programmable shell gate §5.1).
    abandoned: bool,
    /// A command of the line ended with Ctrl-C (spec §6.4), whatever a
    /// `!` made of its status; a status of 130 alone is none.
    cancelled: bool,
}

impl<'a> Shell<'a> {
    /// A shell that runs every command in its own process (the in-process
    /// runner, user-space gate §8.2).
    pub fn new(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
    ) -> Shell<'a> {
        Shell::with_runner(vfs, console, system, Runners::InProcess(runner::InProcess))
    }

    /// A shell whose commands are programs: `/bin/sh` (the spawning
    /// runner, user-space gate §8.2). Only the shell's own commands
    /// ([`commands::BUILTINS`]) run in it.
    pub fn spawning(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        programs: &'a mut dyn Programs,
    ) -> Shell<'a> {
        let runner = Runners::Spawning(runner::Spawning { programs });
        Shell::with_runner(vfs, console, system, runner)
    }

    fn with_runner(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        runner: Runners<'a>,
    ) -> Shell<'a> {
        Shell {
            vfs,
            console,
            system,
            runner,
            editor: LineEditor::new(),
            status: 0,
            stopped: false,
            exited: false,
            in_script: false,
            transcript: None,
            input: None,
            jobs: Jobs::new(),
            prompting: false,
            vars: Vars::new(NAME),
            abandoned: false,
            cancelled: false,
        }
    }

    /// The same shell, its `$0` `name`: its argument 0, as bash's is
    /// (`relay-sh` otherwise).
    pub fn named(mut self, name: &str) -> Shell<'a> {
        self.vars = Vars::new(name);
        self
    }

    /// The same shell, its in-process commands reading `input`.
    pub fn with_input(mut self, input: &'a mut dyn Stdin) -> Shell<'a> {
        self.input = Some(input);
        self
    }

    /// The exit status of the last command.
    pub fn status(&self) -> i32 {
        self.status
    }

    /// `root@relay:<cwd># `, with `/root` shown as `~`.
    pub fn prompt(&self) -> String {
        let cwd = path::display(&self.vfs.cwd());
        let dir = match cwd.strip_prefix(HOME) {
            Some("") => String::from("~"),
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => cwd,
        };
        format!("root@relay:{dir}# ")
    }

    /// Reads and runs commands until the input ends, `exit` or
    /// `reboot`/`poweroff` return.
    pub fn run(&mut self) {
        self.stopped = false;
        self.prompting = true;
        while !self.stopped {
            self.collect_jobs();
            for line in self.jobs.report() {
                self.say(line.as_bytes());
            }
            let mut out = Vec::new();
            let prompt = self.prompt();
            self.editor.start(&prompt, self.console.columns(), &mut out);
            self.console.write(&out);
            loop {
                let Some(byte) = self.console.read_byte() else {
                    return;
                };
                out.clear();
                let feed = self.editor.feed(byte, &mut out);
                self.console.write(&out);
                match feed {
                    Feed::Pending => {}
                    Feed::Cancelled => {
                        self.status = CANCELLED;
                        break;
                    }
                    Feed::Line(line) => {
                        // What ended while it was typed frees its slot in
                        // the process table before the line runs; it is
                        // reported at the next prompt.
                        self.collect_jobs();
                        self.execute(&line);
                        break;
                    }
                }
            }
        }
    }

    /// Shows `/etc/motd` and goes to `/root`, as init does when the machine
    /// starts (`cargo xtask host-shell` calls it).
    pub fn greet(&mut self) {
        if let Ok(node) = self.vfs.lookup(b"/etc/motd") {
            let mut buf = alloc::vec![0; MOTD_MAX];
            if let Ok(n) = self.vfs.read_at(node, 0, &mut buf) {
                self.console.write(&buf[..n]);
            }
        }
        // Without a /root the shell starts in /.
        let _ = self.vfs.chdir(HOME.as_bytes());
    }

    /// Runs one command line as if it had been typed; returns its exit
    /// status. Every command is followed by a sync, so its changes are on
    /// the disk when the prompt comes back.
    pub fn execute(&mut self, line: &str) -> i32 {
        let list = match parser::parse_line(line) {
            Ok(list) => list,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        self.run_list(&list)
    }

    /// Runs a list's items one after another (programmable shell gate
    /// §5.1); its status is the last one's. An empty list keeps the last
    /// status.
    /// `exit`, Ctrl-C (status 130, spec §6.4) and an expansion that
    /// abandons the line stop the rest.
    fn run_list(&mut self, list: &parser::List<parser::Word>) -> i32 {
        self.abandoned = false;
        self.cancelled = false;
        // A background job needs programs: the in-process runner refuses
        // a line that holds one, before any of it runs.
        if self.runner.programs().is_none() && list.items.iter().any(|i| i.background.is_some()) {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
        }
        let mut status = self.status;
        for item in &list.items {
            status = match &item.background {
                // The parser makes sure a background item is one pipeline.
                Some(text) => self.run_pipeline(&item.and_or.first, Some(text)),
                None => self.run_and_or(&item.and_or),
            };
            if self.ends_line() {
                break;
            }
        }
        status
    }

    /// Runs an and-or list's first pipeline, then each one whose `&&` or
    /// `||` the status so far allows (programmable shell gate §5.1); the
    /// status is the last one's that ran, and one that does not run leaves
    /// `$?` alone.
    fn run_and_or(&mut self, and_or: &parser::AndOr<parser::Word>) -> i32 {
        let mut status = self.run_pipeline(&and_or.first, None);
        for (connector, pipeline) in &and_or.rest {
            if self.ends_line() {
                break;
            }
            let runs = match connector {
                parser::Connector::And => status == 0,
                parser::Connector::Or => status != 0,
            };
            if runs {
                status = self.run_pipeline(pipeline, None);
            }
        }
        status
    }

    /// The command that ran last ends the rest of the line: `exit`,
    /// Ctrl-C or an expansion that abandons it.
    fn ends_line(&self) -> bool {
        self.stopped || self.cancelled || self.abandoned
    }

    /// An expansion that failed: the command fails with status 1. In a
    /// command the shell runs itself (`alone`, not one of a pipeline nor a
    /// background job, which bash expands in shells of their own), a bad
    /// substitution, or a line that would expand past 64 KiB, abandons the
    /// rest of the line too, as interactive bash abandons it; a
    /// redirection target that is not one word, or a variable that does
    /// not fit, fails only its command.
    fn not_expanded(&mut self, e: expand::Error, alone: bool) -> i32 {
        self.abandoned = alone
            && matches!(
                e,
                expand::Error::BadSubstitution(_) | expand::Error::TooLong
            );
        self.finish(1, format!("{NAME}: {e}\n"))
    }

    /// Runs one pipeline, or starts it in the background with the job's
    /// text `background`. After a `!` its status is negated, as bash's is:
    /// 0 becomes 1 and anything else 0, even 130 after Ctrl-C, which still
    /// ends the line. Neither a background job's start nor `exit` is, nor
    /// a command whose expansion abandoned the line, which never ran.
    fn run_pipeline(
        &mut self,
        typed: &parser::Pipeline<parser::Word>,
        background: Option<&str>,
    ) -> i32 {
        let status = self.run_commands(typed, background);
        if !typed.negated || background.is_some() || self.stopped || self.abandoned {
            return status;
        }
        self.status = i32::from(status == 0);
        self.status
    }

    /// Runs a pipeline's commands, or starts them in the background, their
    /// words expanded just before.
    fn run_commands(
        &mut self,
        typed: &parser::Pipeline<parser::Word>,
        background: Option<&str>,
    ) -> i32 {
        let assigns = typed
            .commands
            .iter()
            .find_map(|c| c.words.first().filter(|w| w.assignment().is_some()));
        if let Some(first) = assigns {
            // Alone on its line; bash's changes nothing elsewhere.
            let place = match (background, typed.commands.len()) {
                (Some(_), _) => "the background",
                (None, 1) => return self.assign(&typed.commands[0]),
                (None, _) => "a pipeline",
            };
            let message = format!("{NAME}: {}: cannot be used in {place}\n", first.typed);
            return self.finish(1, message);
        }
        let mut pipeline = match expand::expand(typed, &self.vars, self.status) {
            Ok(p) => match background {
                Some(text) => return self.background(&p.commands, text),
                None => p.commands,
            },
            Err(e) => {
                let alone = typed.commands.len() == 1 && background.is_none();
                return self.not_expanded(e, alone);
            }
        };
        if pipeline.len() > 1 {
            return self.pipeline(&pipeline);
        }
        let cmd = pipeline.remove(0);
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            // Its words expanded to nothing: bash's status 0.
            return self.finish(0, String::new());
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: match &mut self.input {
                Some(input) => Some(&mut **input),
                None => None,
            },
        };
        let ran = match cmd.words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
                Some(builtin) => {
                    let control = JobControl {
                        jobs: &mut self.jobs,
                        programs: self.runner.programs(),
                        report: self.prompting && !self.in_script,
                    };
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file, Some(control)),
                        Err(ran) => ran,
                    }
                }
                None => self
                    .runner
                    .get()
                    .run(parts, name, args, cmd.redirect.as_ref()),
            },
            // A bare `> file` just creates or empties the file.
            None => match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            },
        };
        self.stopped = ran.stop;
        self.exited = ran.exited;
        self.cancelled |= ran.cancelled;
        let mut status = ran.status;
        if let Some(script) = ran.script {
            status = self.run_script(*script);
        }
        self.finish(status, ran.message)
    }

    /// A line of assignments (spec §9.4): each sets its variable in turn,
    /// so a later one reads an earlier one, and the status is 0. A
    /// redirection after them makes its file, as bash's does.
    fn assign(&mut self, cmd: &parser::Command<parser::Word>) -> i32 {
        for (name, value) in cmd.words.iter().filter_map(parser::Word::assignment) {
            let set =
                expand::value(&value, &self.vars, self.status).and_then(|v| self.vars.set(name, v));
            if let Err(e) = set {
                return self.not_expanded(e, true);
            }
        }
        let redirect = match cmd.redirect.as_ref() {
            Some(r) => match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => Some(r),
                Err(e) => return self.not_expanded(e, true),
            },
            None => None,
        };
        match runner::redirect_to(&mut *self.vfs, redirect.as_ref()) {
            Ok(_) => self.finish(0, String::new()),
            Err(ran) => self.finish(ran.status, ran.message),
        }
    }

    /// Runs a pipeline (user-space gate §9.1): its status is the last
    /// command's. The shell's own commands cannot be in one.
    fn pipeline(&mut self, stages: &[parser::Command]) -> i32 {
        if let Some(name) = builtin_in(stages) {
            let ran = runner::in_a_pipeline(name);
            return self.finish(ran.status, ran.message);
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: match &mut self.input {
                Some(input) => Some(&mut **input),
                None => None,
            },
        };
        let ran = self.runner.get().pipeline(parts, stages);
        self.cancelled |= ran.cancelled;
        self.finish(ran.status, ran.message)
    }

    /// Starts `stages` as a background job (user-space gate §9.2), whose
    /// text is `text`: at the prompt the shell says `[<number>] <pid of its
    /// last process>`. Its status is 0 once anything of it started. The
    /// shell's own commands cannot be in one.
    fn background(&mut self, stages: &[parser::Command], text: &str) -> i32 {
        if let Some(name) = builtin_in(stages) {
            let message = format!("{NAME}: {name}: cannot be used in the background\n");
            return self.finish(1, message);
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: None,
        };
        let started = self.runner.get().background(parts, stages);
        if let Some(pgid) = started.pgid {
            let number = self.jobs.add(pgid, &started.pids, text);
            if self.prompting && !self.in_script {
                let last = started.pids.last().copied().unwrap_or(pgid);
                self.say(format!("[{number}] {last}\n").as_bytes());
            }
        }
        self.finish(started.ran.status, started.ran.message)
    }

    /// Collects the background jobs' processes that have ended.
    fn collect_jobs(&mut self) {
        if let Some(programs) = self.runner.programs() {
            self.jobs.collect(programs);
        }
    }

    /// Writes to the screen and, while a script runs, its transcript.
    fn say(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.end_transcript(e);
        }
    }

    /// Writes what the screen showed to the transcript. If that fails the
    /// transcript ends there, with a message; the script goes on.
    fn write_transcript(&mut self) {
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.write(&mut *self.vfs)
        {
            self.end_transcript(e);
        }
    }

    fn end_transcript(&mut self, e: vfs::Errno) {
        if let Some(t) = self.transcript.take() {
            self.console.write(t.ended(e).as_bytes());
        }
    }

    /// Runs a script `sh` read, in this shell (the in-process runner): its
    /// transcript is written by the shell. A script cannot run another.
    /// Its `exit` ends only the script, as it does under `/bin/sh`, where a
    /// script is a shell of its own; and it has variables and arguments of
    /// its own, and starts with `$?` 0, as there.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let outer = core::mem::replace(&mut self.vars, Vars::script(&script.name, &script.args));
        self.status = 0;
        let status = self.run_lines(&script.text);
        self.vars = outer;
        if self.exited {
            self.stopped = false;
        }
        // What abandoned a line of the script leaves the line that ran it.
        self.abandoned = false;
        self.write_transcript();
        self.transcript = None;
        status
    }

    /// `/bin/sh FILE`: runs the script `args` names (user-space gate §8.3)
    /// as `sh` does, in a spawning shell (`Shell::spawning`), whose
    /// commands then run in its process group. The transcript is a console
    /// tee, so it gets the output of the script's programs and of any
    /// script it runs (whose own transcript is pushed on top); a write
    /// that failed is reported when the script ends. `stdout` is the
    /// shell's fd 1: a script's output cannot be redirected.
    pub fn run_file(&mut self, args: &[String], stdout: &mut dyn Stdout) -> i32 {
        let Some(sh) = commands::find("sh") else {
            unreachable!("sh is in the command table")
        };
        let mut ctx = Ctx::program(
            &mut *self.vfs,
            &mut *self.system,
            &mut *self.console,
            stdout,
        );
        let status = (sh.run)(&mut ctx, args);
        let Some(script) = ctx.script.take() else {
            return status;
        };
        let Some(programs) = self.runner.programs() else {
            unreachable!("run_file needs a spawning shell")
        };
        self.vars = Vars::script(&script.name, &script.args);
        let log = script.transcript_name;
        if let Err(e) = programs.tee_push(log.as_bytes()) {
            let shown = quote_if_needed(&path::display(log.as_bytes()));
            let message = format!("sh: cannot write the transcript {shown}: {e}\n");
            self.console.write(message.as_bytes());
            return 1;
        }
        let status = self.run_lines(&script.text);
        if let Some(Err(e)) = self.runner.programs().map(|p| p.tee_pop()) {
            self.console.write(transcript::ended(&log, e).as_bytes());
        }
        status
    }

    /// `X | sh`: a shell whose standard input is no console runs the
    /// commands it reads there (user-space gate §9.1, §16 item 8), each as
    /// soon as a line finishes it, without a prompt, a trace or the line
    /// editor, so it never takes the console; it ends at the input's end or
    /// `exit`. Input that ends inside a command is bash's `unexpected end
    /// of file`. It reads a byte at a time, as bash reads a pipe, so that a
    /// command it runs reads what follows its line (`printf 'cat\nx\n' |
    /// sh` gives `cat` the `x`). A line over 64 KiB, or not UTF-8, is
    /// skipped with a message, as `sh` refuses such a script. Returns the
    /// last status.
    pub fn run_input(&mut self, input: &mut dyn Stdin) -> i32 {
        let mut line: Vec<u8> = Vec::new();
        let mut too_long = false;
        let mut reader = Reader::new();
        self.stopped = false;
        loop {
            let mut byte = [0];
            match input.read(&mut byte) {
                Ok(0) => {
                    if !line.is_empty() || too_long {
                        self.input_line(&mut reader, &line, too_long);
                    }
                    if !self.stopped
                        && let Some(e) = reader.end()
                    {
                        return self.finish(SYNTAX, format!("{NAME}: {e}\n"));
                    }
                    return self.status;
                }
                Ok(_) => {}
                Err(e) => {
                    let message = format!("sh: standard input: {e}\n");
                    return self.finish(1, message);
                }
            }
            if byte[0] == b'\n' {
                self.input_line(&mut reader, &line, too_long);
                line.clear();
                too_long = false;
                if self.stopped {
                    return self.status;
                }
            } else if !too_long {
                line.push(byte[0]);
                if line.len() as u64 > SCRIPT_MAX {
                    too_long = true;
                    line.clear();
                }
            } else {
                // Only its last bytes, which say whether the command it is
                // in goes on after it.
                line.push(byte[0]);
                if line.len() == 2 * LINE_TAIL {
                    line.drain(..LINE_TAIL);
                }
            }
        }
    }

    /// One line `run_input` read: the command it finishes run, or said
    /// why not. A line that cannot be read drops the command it was in.
    fn input_line(&mut self, reader: &mut Reader, line: &[u8], too_long: bool) {
        self.collect_jobs();
        match core::str::from_utf8(line) {
            _ if too_long => {
                reader.drop_line(&String::from_utf8_lossy(line));
                self.finish(1, String::from("sh: standard input: a line over 64 KiB\n"));
            }
            Ok(text) => {
                self.read_line(reader, text);
            }
            Err(_) => {
                reader.drop_line(&String::from_utf8_lossy(line));
                self.finish(1, String::from("sh: standard input: not a text line\n"));
            }
        }
    }

    /// Adds a line to the command `reader` holds, and runs the command
    /// once the line finishes it (programmable shell gate §4.3); a line
    /// that does not parse drops it, with bash's message and status 2.
    /// Returns the status, or `None` while the command needs more lines.
    fn read_line(&mut self, reader: &mut Reader, line: &str) -> Option<i32> {
        match reader.add(line) {
            Ok(Some(list)) => Some(self.run_list(&list)),
            Ok(None) => None,
            Err(e) => Some(self.finish(SYNTAX, format!("{NAME}: {e}\n"))),
        }
    }

    /// Runs a script's lines (spec §15 item 12): each line is shown as
    /// `+ <line>` as it is read, and a command runs, and is synced, as if
    /// typed once a line finishes it (programmable shell gate §5.4). Blank
    /// and comment lines are skipped. Ctrl-C, `exit`, or
    /// `reboot`/`poweroff` returning, ends the script; failing commands
    /// do not, nor does a line that does not parse. A script that ends
    /// inside a command is bash's `unexpected end of file`. Returns the
    /// last status.
    fn run_lines(&mut self, text: &str) -> i32 {
        self.in_script = true;
        let mut status = 0;
        let mut reader = Reader::new();
        for line in text.lines() {
            // Blank as typed: one whose words expand to nothing is traced
            // and runs.
            if parser::parse_line(line).is_ok_and(|l| l.items.is_empty()) {
                continue;
            }
            if self.console.interrupted() {
                self.say(b"^C\n");
                status = CANCELLED;
                self.cancelled = true;
                break;
            }
            self.collect_jobs();
            // The line runs as written; only its trace is trimmed.
            self.say(format!("+ {}\n", line.trim()).as_bytes());
            // On the disk before the command runs: a command that hangs
            // leaves at least its name.
            self.write_transcript();
            self.sync();
            let Some(ran) = self.read_line(&mut reader, line) else {
                continue;
            };
            status = ran;
            if self.cancelled || self.stopped {
                break;
            }
        }
        if !(self.cancelled || self.stopped)
            && let Some(e) = reader.end()
        {
            status = self.finish(SYNTAX, format!("{NAME}: {e}\n"));
        }
        self.in_script = false;
        status
    }

    /// Prints `message`, adds the line's output to a running script's
    /// transcript, syncs, and records `status`.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        self.say(message.as_bytes());
        self.write_transcript();
        self.sync();
        self.status = status;
        status
    }

    fn sync(&mut self) {
        if let Err(e) = self.vfs.sync() {
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
    }
}

/// The name of the first of `stages` that is one of the shell's own
/// commands (a command whose words expanded to nothing is none).
fn builtin_in(stages: &[parser::Command]) -> Option<&str> {
    stages
        .iter()
        .filter_map(|c| c.words.first())
        .find(|name| commands::builtin(name).is_some())
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
    use relay_abi::WaitStatus;
    use vfs::Errno;

    #[test]
    fn the_prompt_shows_home_as_a_tilde() {
        let mut h = Harness::new();
        let prompt =
            |h: &mut Harness| Shell::new(&mut h.vfs, &mut h.console, &mut h.system).prompt();
        assert_eq!(prompt(&mut h), "root@relay:/# ");
        h.run("cd /root");
        assert_eq!(prompt(&mut h), "root@relay:~# ");
        h.dir("/root/notes");
        h.run("cd notes");
        assert_eq!(prompt(&mut h), "root@relay:~/notes# ");
        h.dir("/rootless");
        h.run("cd /rootless");
        assert_eq!(prompt(&mut h), "root@relay:/rootless# ");
    }

    #[test]
    fn a_pipeline_hands_each_command_s_output_to_the_next() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"one\ntwo\nthree\n");
        // What bash prints for each (`bash -c '…'`).
        assert_eq!(h.run("echo hello world | wc -c"), (0, "12\n".into()));
        assert_eq!(
            h.run("cat /tmp/f | head -n 2 | tail -n 1"),
            (0, "two\n".into())
        );
        // Not the screen: one name a line.
        assert_eq!(h.run("ls /etc | cat"), (0, "hostname\nmotd\n".into()));
        // The last command's redirection; and its status is the line's.
        assert_eq!(h.run("cat /tmp/f | wc -l > /tmp/n"), (0, "".into()));
        assert_eq!(h.get("/tmp/n"), b"3\n");
        assert_eq!(
            h.run("cat /nope | wc -l"),
            (0, "cat: /nope: No such file or directory\n0\n".into())
        );
        assert_eq!(
            h.run("echo x | cat /nope"),
            (1, "cat: /nope: No such file or directory\n".into())
        );
        // A command that is not found gives the next nothing, as in bash.
        assert_eq!(
            h.run("nosuch | wc -l"),
            (0, "relay-sh: nosuch: command not found\n0\n".into())
        );
        assert_eq!(
            h.run("echo x | nosuch"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        // The first command reads the shell's standard input.
        h.stdin = b"a\nb\n".to_vec();
        assert_eq!(h.run("cat | wc -l"), (0, "2\n".into()));
    }

    #[test]
    fn the_shell_s_own_commands_cannot_be_in_a_pipeline() {
        let mut h = Harness::new();
        for (line, name) in [
            ("cd /tmp | cat", "cd"),
            ("echo a | exit 3", "exit"),
            ("help | wc", "help"),
            ("ls | sh x | wc", "sh"),
        ] {
            assert_eq!(
                h.run(line),
                (
                    1,
                    alloc::format!("relay-sh: {name}: cannot be used in a pipeline\n")
                ),
                "{line}"
            );
        }
        // Nothing ran: not even the commands before it.
        assert_eq!(h.run("pwd"), (0, "/\n".into()));
        h.put("/tmp/x", b"");
        assert_eq!(
            h.run("echo a > /tmp/x | cd /"),
            (2, "relay-sh: unsupported syntax: > before |\n".into())
        );
    }

    #[test]
    fn a_pipeline_command_without_a_name_is_refused_by_both_runners() {
        let mut h = Harness::new();
        for line in ["echo hi | > /tmp/f", "> /tmp/f | cat"] {
            let said = if line.starts_with('>') {
                "> before |"
            } else {
                "| >"
            };
            let want = (2, alloc::format!("relay-sh: unsupported syntax: {said}\n"));
            assert_eq!(h.run(line), want, "{line}");
            assert_eq!(h.spawning(line), want, "{line}");
        }
        assert!(!h.exists("/tmp/f"), "nothing of the line ran");
        assert!(h.programs.spawned.is_empty());
    }

    #[test]
    fn a_shell_whose_input_is_no_console_runs_the_lines_it_reads() {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        let text = b"t-args a\n\nt-args 'b c'\n\xff\nexit 3\nt-args never\n".to_vec();
        let mut input = crate::Bytes::new(text);
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(status, 3, "exit's");
        let args: Vec<Vec<String>> = h.programs.spawned.iter().map(|s| s.args.clone()).collect();
        assert_eq!(args, [vec!["t-args", "a"], vec!["t-args", "b c"]]);
        // No prompt, no trace: only what is wrong.
        assert_eq!(h.console.take(), "sh: standard input: not a text line\n");
        assert!(
            h.console.input.is_empty()
                && h.programs
                    .spawned
                    .iter()
                    .all(|s| s.group == crate::Group::New)
        );
        // The last line needs no newline; a line over 64 KiB is skipped.
        let mut long = alloc::vec![b'x'; 70_000];
        long.extend_from_slice(b"\nt-args c");
        let mut input = crate::Bytes::new(long);
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            (status, h.console.take()),
            (0, "sh: standard input: a line over 64 KiB\n".into())
        );
        assert_eq!(h.programs.spawned.last().unwrap().args, ["t-args", "c"]);
    }

    #[test]
    fn a_shell_reading_its_input_reads_a_command_across_lines() {
        let mut h = spawning();
        let mut input =
            crate::Bytes::new(b"t-args a &&\nt-args b\nt-args c |\n\nt-args d\n".to_vec());
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(status, 3);
        let args: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(args, ["a", "c", "d"], "t-args a failed, so b did not run");
        assert_eq!(h.console.take(), "");
        // The end of input inside a command runs none of it.
        let mut input = crate::Bytes::new(b"t-args e ||".to_vec());
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            (status, h.console.take()),
            (2, "relay-sh: syntax error: unexpected end of file\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 3);
    }

    #[test]
    fn a_line_that_cannot_be_read_drops_the_command_it_was_in() {
        for bad in [alloc::vec![b'x'; 70_000], b"\xff".to_vec()] {
            let mut h = spawning();
            let mut text = b"t-args a ||\n".to_vec();
            text.extend_from_slice(&bad);
            text.extend_from_slice(b"\nt-args b\n");
            let mut input = crate::Bytes::new(text);
            Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                .run_input(&mut input);
            let args: Vec<&str> = h
                .programs
                .spawned
                .iter()
                .map(|s| s.args[1].as_str())
                .collect();
            assert_eq!(args, ["b"], "t-args a || never ran");
        }
    }

    /// The programs `X | sh` started for `text`, their arguments after
    /// argument 0.
    fn piped(text: &[u8]) -> Vec<String> {
        let mut h = spawning();
        let mut input = crate::Bytes::new(text.to_vec());
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        h.programs
            .spawned
            .iter()
            .map(|s| s.args[1..].join(" "))
            .collect()
    }

    #[test]
    fn a_dropped_command_runs_none_of_its_later_lines() {
        // The review found the end of an `&&` chain run after its start
        // was dropped: too long, or with a line that is no text.
        let long = alloc::format!("t-args {} &&\n", "x".repeat(40_000));
        let text = alloc::format!("t-args a &&\n{long}{long}t-args ran\nt-args next\n");
        assert_eq!(piped(text.as_bytes()), ["next"]);
        assert_eq!(
            piped(b"t-args a &&\n\xff &&\nt-args ran\nt-args next\n"),
            ["next"]
        );
        // A line over 64 KiB ending in `&&` too; one finishing its command
        // drops nothing after it.
        let mut text = b"t-args a\n".to_vec();
        text.extend(alloc::vec![b'x'; 70_000]);
        text.extend_from_slice(b" &&\nt-args ran\nt-args next\n");
        assert_eq!(piped(&text), ["a", "next"]);
        let mut text = alloc::vec![b'x'; 70_000];
        text.extend_from_slice(b"\nt-args next\n");
        assert_eq!(piped(&text), ["next"]);
    }

    #[test]
    fn a_command_read_across_lines_holds_at_most_64_kib() {
        let mut h = spawning();
        let line = alloc::format!("t-args {} &&\n", "x".repeat(40_000));
        let mut text = line.repeat(2).into_bytes();
        text.extend_from_slice(b"t-args after\nt-args next\n");
        let mut input = crate::Bytes::new(text);
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            h.console.take(),
            "relay-sh: the command would be longer than 64 KiB\n"
        );
        let args: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(args, ["next"], "its end dropped with it");
    }

    /// Input that records how much each read asked for.
    struct Asked {
        bytes: crate::Bytes,
        asked: Vec<usize>,
    }

    impl crate::Stdin for Asked {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
            self.asked.push(buf.len());
            self.bytes.read(buf)
        }
    }

    #[test]
    fn a_shell_reading_a_pipe_takes_no_more_than_each_line() {
        // bash reads a pipe a byte at a time, so `printf 'cat\nx\n' | bash`
        // gives `cat` the `x`; here a command reads the same fd 0.
        let mut h = spawning();
        let mut input = Asked {
            bytes: crate::Bytes::new(b"t-args a\nt-args b\n".to_vec()),
            asked: Vec::new(),
        };
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(h.programs.spawned.len(), 2);
        assert!(input.asked.iter().all(|&n| n == 1), "{:?}", input.asked);
        assert_eq!(input.asked.len(), 19, "18 bytes and the end");
    }

    #[test]
    fn ctrl_c_stops_a_pipeline() {
        let mut h = Harness::new();
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big | wc -c"), (130, "^C\n".into()));
        // The rest does not run, even what would not have asked.
        assert_eq!(h.run("cat /tmp/big | echo after"), (130, "^C\n".into()));
    }

    #[test]
    fn a_list_runs_its_items_one_after_another() {
        let mut h = Harness::new();
        // What bash prints for each (an interactive bash 5.2).
        assert_eq!(h.run("echo a;echo b;"), (0, "a\nb\n".into()));
        // Each item reads the status of the one before.
        assert_eq!(
            h.run("false; echo $?; nope; echo $?"),
            (0, "1\nrelay-sh: nope: command not found\n127\n".into())
        );
        assert_eq!(h.run("echo a; false"), (1, "a\n".into()));
        // An assignment is an item too, and the next one reads it.
        assert_eq!(h.run("A=1; echo $A"), (0, "1\n".into()));
        assert_eq!(h.run("cd /etc; pwd"), (0, "/etc\n".into()));
    }

    #[test]
    fn exit_and_ctrl_c_stop_the_rest_of_a_list() {
        let mut h = Harness::new();
        assert_eq!(h.run("exit 3; echo no"), (3, "".into()));
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big | wc -c; echo no"), (130, "^C\n".into()));
    }

    #[test]
    fn a_bad_substitution_abandons_the_rest_of_the_line() {
        // As interactive bash: the rest of the line does not run. A
        // redirection that cannot be made fails only its own command.
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo ${1A}; echo after"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("echo x > $E; echo after"),
            (0, "relay-sh: $E: ambiguous redirect\nafter\n".into())
        );
        // So in an assignment.
        assert_eq!(
            h.run("A=${1A}; echo after"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("A=1 > $E; echo after"),
            (0, "relay-sh: $E: ambiguous redirect\nafter\n".into())
        );
        let big = "x".repeat(40_000);
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.execute(&alloc::format!("A={big}"));
        assert_eq!(shell.execute("echo $A $A; echo after"), 1);
        // Only that line: the next one runs whole.
        assert_eq!(shell.execute("echo a; echo b"), 0);
        assert_eq!(
            h.console.take(),
            "relay-sh: the line would expand to more than 64 KiB\na\nb\n"
        );
    }

    #[test]
    fn a_line_a_script_abandons_leaves_the_line_that_ran_it() {
        // bash's script is a shell of its own; so is the in-process
        // runner's, as far as the lines after it go.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo ${1A}\n");
        assert_eq!(
            h.run("sh /tmp/s.sh; echo after"),
            (
                0,
                "+ echo ${1A}\nrelay-sh: ${1A}: bad substitution\nafter\n".into()
            )
        );
    }

    #[test]
    fn and_and_or_run_a_pipeline_on_the_status_so_far() {
        let mut h = Harness::new();
        // What bash prints for each (an interactive bash 5.2).
        assert_eq!(h.run("true && echo a"), (0, "a\n".into()));
        assert_eq!(h.run("false && echo a"), (1, "".into()));
        assert_eq!(h.run("false || echo b"), (0, "b\n".into()));
        assert_eq!(h.run("true || echo b"), (0, "".into()));
        assert_eq!(
            h.run("false || echo or && echo and"),
            (0, "or\nand\n".into())
        );
        assert_eq!(h.run("true || false && echo d"), (0, "d\n".into()));
        // A pipeline that does not run leaves `$?` as it was.
        assert_eq!(
            h.lines(&[
                "false && echo x; echo $?",
                "true && false || echo c; echo $?"
            ]),
            (0, "1\nc\n0\n".into())
        );
        assert_eq!(
            h.run("nope || echo $?"),
            (0, "relay-sh: nope: command not found\n127\n".into())
        );
        assert_eq!(h.run("exit 4 || echo no"), (4, "".into()));
    }

    #[test]
    fn ctrl_c_and_a_bad_substitution_stop_an_and_or_list() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("true && echo ${1A} || echo no; echo no"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        // 130 is no success, but `||` does not run after Ctrl-C.
        assert_eq!(
            h.run("cat /tmp/big | wc -c || echo no"),
            (130, "^C\n".into())
        );
    }

    #[test]
    fn a_pipeline_that_does_not_run_starts_no_program() {
        let mut h = spawning();
        assert_eq!(h.spawning("t-args a && t-args b"), (3, "".into()));
        assert_eq!(h.spawning("t-args c || t-args d"), (3, "".into()));
        let args: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(args, ["a", "c", "d"]);
    }

    #[test]
    fn a_bang_negates_a_pipeline_s_status() {
        let mut h = Harness::new();
        // What bash prints for each (an interactive bash 5.2).
        assert_eq!(h.run("! true"), (1, "".into()));
        assert_eq!(h.run("! false"), (0, "".into()));
        assert_eq!(
            h.run("! nope"),
            (0, "relay-sh: nope: command not found\n".into())
        );
        assert_eq!(h.run("! ! false; echo $?"), (0, "1\n".into()));
        assert_eq!(h.run("! ; echo $?"), (0, "1\n".into()));
        assert_eq!(h.run("! false && echo t"), (0, "t\n".into()));
        assert_eq!(h.run("! true | false; echo $?"), (0, "0\n".into()));
        assert_eq!(h.run("! A=1; echo $? $A"), (0, "1 1\n".into()));
        // `exit` stops the shell with its own status.
        assert_eq!(h.run("! exit 3"), (3, "".into()));
    }

    #[test]
    fn only_ctrl_c_ends_a_line_not_a_status_of_130() {
        // A script or program that exits with 130 by itself was not
        // interrupted: bash goes on (the review found the line ended).
        let mut h = Harness::new();
        h.programs.known.push(("/bin/sh", WaitStatus::exited(130)));
        h.programs.known.push(("/bin/t-x", WaitStatus::exited(130)));
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        assert_eq!(h.spawning("sh s.sh; t-args after"), (0, "".into()));
        assert_eq!(h.spawning("t-x || t-args or"), (0, "".into()));
        let args: Vec<String> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args.join(" "))
            .collect();
        assert_eq!(args, ["sh s.sh", "t-args after", "t-x", "t-args or"]);
        // Killed by Ctrl-C, it ends the line, whatever its status says.
        assert_eq!(h.spawning("t-spin; t-args no"), (130, "^C\n".into()));
        assert_eq!(h.programs.spawned.len(), 5);
        // A pipeline a stage of which Ctrl-C killed, too.
        assert_eq!(
            h.spawning("t-spin | t-args x; t-args no"),
            (0, "^C\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 7);
        // So in a script: a program's 130 goes on, a Ctrl-C ends it.
        h.put("/tmp/s.sh", b"t-x\nt-args b\nt-spin\nt-args c\n");
        let mut out = FakeStdout::console();
        h.sh(&["/tmp/s.sh"], &mut out);
        let args: Vec<&str> = h.programs.spawned[7..]
            .iter()
            .map(|s| s.path.as_str())
            .collect();
        assert_eq!(args, ["/bin/t-x", "/bin/t-args", "/bin/t-spin"]);
        // The in-process runner's `exit 130` in a script likewise.
        let mut h = Harness::new();
        h.put("/tmp/e.sh", b"exit 130\n");
        assert_eq!(
            h.run("sh /tmp/e.sh; echo after $?"),
            (0, "+ exit 130\nafter 130\n".into())
        );
    }

    #[test]
    fn a_negated_command_that_does_not_expand_fails() {
        // bash's `$?` is 1: the command never ran, so there is nothing to
        // negate (the review found 0).
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["! echo ${1A}", "echo $?", "! A=${1A}", "echo $?"]),
            (
                0,
                "relay-sh: ${1A}: bad substitution\n1\n\
                 relay-sh: ${1A}: bad substitution\n1\n"
                    .into()
            )
        );
    }

    #[test]
    fn ctrl_c_ends_a_negated_pipeline_s_line_and_script() {
        let mut h = Harness::new();
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        // `$?` is the negated 130, as in bash, and the rest does not run.
        h.console.interrupt = true;
        assert_eq!(h.run("! cat /tmp/big | wc -c; echo no"), (0, "^C\n".into()));
        // Only that line: the next one runs whole.
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.execute("cat /tmp/big");
        assert_eq!(shell.execute("echo a; echo b"), 0);
        assert_eq!(h.console.take(), "^C\na\nb\n");
        h.console.interrupt = false;
        h.put("/tmp/s.sh", b"! cat /tmp/big | wc -c\necho no\n");
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("sh /tmp/s.sh").1,
            "+ ! cat /tmp/big | wc -c\n^C\n",
            "the script ends there"
        );
    }

    #[test]
    fn a_negated_background_job_starts_with_status_0() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["! sleep 5 &", "t-args $?", ""]);
        assert!(
            out.starts_with("root@relay:/# ! sleep 5 &\n[1] 101\n"),
            "{out}"
        );
        assert_eq!(h.programs.spawned[1].args, ["t-args", "0"], "as bash's");
        assert!(out.contains("Done                    sleep 5\n"), "{out}");
    }

    #[test]
    fn a_bad_substitution_in_a_pipeline_or_a_job_fails_only_it() {
        // bash expands a pipeline's commands, and a job's, in shells of
        // their own, so the error ends only them and the line goes on (the
        // review found the rest of the line dropped). The pipeline's status
        // is 1: it is expanded whole before any of it starts (user-space
        // gate §16 item 10), where bash runs its other commands.
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo ${1A} | cat; echo after $?"),
            (0, "relay-sh: ${1A}: bad substitution\nafter 1\n".into())
        );
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep ${1A} & t-args after"]);
        assert!(out.contains("relay-sh: ${1A}: bad substitution\n"), "{out}");
        let args: Vec<String> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args.join(" "))
            .collect();
        assert_eq!(args, ["t-args after"], "no job started; t-args ran");
    }

    #[test]
    fn a_compound_command_runs_none_of_its_parts() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo a; if true; then echo b; fi"),
            (2, "relay-sh: unsupported syntax: if\n".into())
        );
    }

    #[test]
    fn each_item_of_a_list_is_synced() {
        let mut h = Harness::new();
        h.run("pwd; pwd; pwd");
        assert_eq!(h.spy.syncs.get(), 3);
    }

    #[test]
    fn unknown_commands_and_syntax_errors() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("frobnicate x"),
            (127, "relay-sh: frobnicate: command not found\n".into())
        );
        assert_eq!(
            h.run("ls | ;"),
            (
                2,
                "relay-sh: syntax error near unexpected token `;'\n".into()
            )
        );
        assert_eq!(
            h.run("ls |"),
            (2, "relay-sh: syntax error: unexpected end of file\n".into())
        );
        assert_eq!(
            h.run("echo 'open"),
            (2, "relay-sh: syntax error: unterminated quote\n".into())
        );
    }

    #[test]
    fn the_in_process_runner_runs_no_programs() {
        let mut h = Harness::new();
        h.dir("/bin");
        h.put("/bin/t-args", b"\x7fELF");
        assert_eq!(
            h.run("t-args"),
            (127, "relay-sh: t-args: command not found\n".into())
        );
        assert_eq!(
            h.run("/bin/t-args"),
            (127, "relay-sh: /bin/t-args: command not found\n".into())
        );
    }

    /// `/bin/t-args` exits with 3 and `/bin/sh` with 0, for a spawning
    /// shell.
    fn spawning() -> Harness {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(3)));
        h.programs.known.push(("/bin/sh", WaitStatus::exited(0)));
        h
    }

    /// What an interactive `/bin/sh` prints for the lines typed, each
    /// ending with Enter, until the input ends.
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs).run();
        h.console.take()
    }

    /// `sleep` and `t-spin` run on through `collect`'s first round (the
    /// prompt after they start).
    fn with_jobs() -> Harness {
        let mut h = spawning();
        for p in ["/bin/sleep", "/bin/t-spin", "/bin/cat"] {
            h.programs.known.push((p, WaitStatus::exited(0)));
            h.programs.lives.push((p, 1));
        }
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        h
    }

    #[test]
    fn a_background_job_says_its_number_and_how_it_ended_before_a_prompt() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 &", "", "false &", ""]);
        assert_eq!(
            out,
            "root@relay:/# sleep 5 &\n[1] 101\n\
             root@relay:/# \n\
             [1]+  Done                    sleep 5\n\
             root@relay:/# false &\n[1] 102\n\
             [1]+  Exit 1                  false\n\
             root@relay:/# \nroot@relay:/# "
        );
        assert_eq!(
            h.programs.spawned[0].group,
            crate::Group::Background,
            "without the console"
        );
        assert!(h.programs.children().is_empty(), "every one collected");
    }

    #[test]
    fn a_job_that_ended_while_a_line_was_typed_is_collected_before_it_runs() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 &", "t-args"]);
        assert_eq!(
            h.programs.alive_at_spawn,
            [0, 0],
            "sleep's slot was free when t-args started"
        );
        // Reported at the next prompt, as bash's.
        assert!(
            out.ends_with("# t-args\n[1]+  Done                    sleep 5\nroot@relay:/# "),
            "{out}"
        );
    }

    #[test]
    fn a_background_pipeline_is_one_job_in_one_group() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["cat f | t-args x > out &", ""]);
        assert!(
            out.starts_with("root@relay:/# cat f | t-args x > out &\n[1] 102\n"),
            "{out}"
        );
        assert!(
            out.contains("[1]+  Exit 3                  cat f | t-args x > out\n"),
            "{out}"
        );
        let groups: Vec<crate::Group> = h.programs.spawned.iter().map(|s| s.group).collect();
        assert_eq!(groups, [crate::Group::Background, crate::Group::Join(101)]);
        assert_eq!(h.programs.spawned[1].stdout, Some(4), "the redirection");
        let (r, w) = h.programs.pipes[0];
        assert!(
            [r, w, 4].iter().all(|fd| h.programs.closed.contains(fd)),
            "the shell's copies"
        );
    }

    #[test]
    fn a_background_job_that_cannot_start_is_no_job() {
        let mut h = with_jobs();
        assert_eq!(
            h.spawning("nosuch &"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        // One stage of two that starts is a job, the status 0, as bash's.
        let out = typed(&mut h, &["nosuch | sleep 1 &", ""]);
        assert!(
            out.starts_with(
                "root@relay:/# nosuch | sleep 1 &\nrelay-sh: nosuch: command not found\n[1] 101\n"
            ),
            "{out}"
        );
        assert_eq!(
            h.spawning("sleep 1 | nosuch &"),
            (0, "relay-sh: nosuch: command not found\n".into()),
            "even the last"
        );
        for line in ["cd / &", "exit &", "help | cat &"] {
            let name = line.split([' ', '&']).next().unwrap();
            assert_eq!(
                h.spawning(line),
                (
                    1,
                    alloc::format!("relay-sh: {name}: cannot be used in the background\n")
                ),
                "{line}"
            );
        }
        assert_eq!(h.programs.spawned.len(), 2, "nothing else started");
    }

    #[test]
    fn a_script_and_x_into_sh_say_nothing_of_their_jobs() {
        let mut h = with_jobs();
        h.put("/tmp/s.sh", b"sleep 5 &\nt-args\nt-args\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (3, "+ sleep 5 &\n+ t-args\n+ t-args\n".into())
        );
        assert_eq!(
            h.programs.spawned[0].group,
            crate::Group::Background,
            "not the script's group"
        );
        assert!(
            h.programs.children().is_empty(),
            "collected before the later lines"
        );
        let mut h = with_jobs();
        let mut input = crate::Bytes::new(b"sleep 5 &\nt-args\nt-args\n".to_vec());
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(h.console.take(), "");
        assert!(
            h.programs.children().is_empty(),
            "collected before the later lines"
        );
    }

    #[test]
    fn the_in_process_runner_runs_no_background_job() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo hi &"),
            (2, "relay-sh: unsupported syntax: &\n".into())
        );
        // Nothing of a line that holds one runs.
        assert_eq!(
            h.run("echo a; echo b & echo c"),
            (2, "relay-sh: unsupported syntax: &\n".into())
        );
    }

    #[test]
    fn an_ampersand_mid_line_starts_a_job_and_goes_on() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 & t-args a", "sleep 6 & sleep 7 &", ""]);
        assert!(
            out.starts_with(
                "root@relay:/# sleep 5 & t-args a\n[1] 101\n\
                 root@relay:/# sleep 6 & sleep 7 &\n[2] 103\n[3] 104\n"
            ),
            "{out}"
        );
        assert!(out.contains("Done                    sleep 6\n"), "{out}");
        assert!(out.contains("Done                    sleep 7\n"), "{out}");
        let groups: Vec<crate::Group> = h.programs.spawned.iter().map(|s| s.group).collect();
        assert_eq!(
            groups,
            [
                crate::Group::Background,
                crate::Group::New,
                crate::Group::Background,
                crate::Group::Background
            ],
            "t-args has the console"
        );
    }

    #[test]
    fn bin_sh_runs_a_script_s_commands_in_its_own_group_with_its_transcript_a_tee() {
        let mut h = spawning();
        h.put("/tmp/s.log", b"an old transcript");
        h.put(
            "/tmp/s.sh",
            b"t-args a\n# a comment\ncd /etc\nnosuch\nt-args b\n",
        );
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                3,
                "+ t-args a\n+ cd /etc\n+ nosuch\nrelay-sh: nosuch: command not found\n+ t-args b\n"
                    .into()
            )
        );
        assert!(
            h.programs
                .spawned
                .iter()
                .all(|s| s.group == crate::Group::Shell),
            "a script's commands run in its group, so Ctrl-C ends it with them"
        );
        assert_eq!(h.programs.pushed, ["/tmp/s.log"]);
        assert!(h.programs.tees.is_empty(), "popped at the end");
        assert_eq!(h.get("/tmp/s.log"), b"", "emptied; the tee writes it");
        assert_eq!(h.run("pwd").1, "/etc\n", "the script's own directory");
    }

    #[test]
    fn a_script_run_by_bin_sh_may_run_another() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"sh /tmp/t.sh\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (0, "+ sh /tmp/t.sh\n".into())
        );
        assert_eq!(h.programs.spawned[0].path, "/bin/sh");
        assert_eq!(h.programs.spawned[0].args, ["sh", "/tmp/t.sh"]);
    }

    #[test]
    fn a_script_has_its_own_variables_and_arguments() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo $? \"[$A]\" $1\nB=in\nexit 4\n");
        // As bash's, without `export`: nothing passes in or out.
        assert_eq!(
            h.lines(&["A=out", "nope", "sh /tmp/s.sh x", "echo $? $A [$B] $1"]),
            (
                0,
                "relay-sh: nope: command not found\n+ echo $? \"[$A]\" $1\n0 [] x\n\
                 + B=in\n+ exit 4\n4 out []\n"
                    .into()
            )
        );
    }

    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args $0 $# \"$@\"\n");
        let mut out = FakeStdout::console();
        h.sh(&["/tmp/s.sh", "a", "b c", ""], &mut out);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "/tmp/s.sh", "3", "a", "b c", ""]
        );
        // A script it runs gets its arguments as typed, expanded.
        h.put("/tmp/r.sh", b"A='x y'\nsh /tmp/s.sh \"$A\" $1\n");
        h.sh(&["/tmp/r.sh", "z"], &mut out);
        assert_eq!(h.programs.spawned[1].args, ["sh", "/tmp/s.sh", "x y", "z"]);
        assert_eq!(h.programs.spawned[1].path, "/bin/sh");
    }

    #[test]
    fn a_transcript_that_failed_is_reported_when_the_script_ends() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\nt-args\n");
        h.programs.pop_error = Some(Errno::ENOSPC);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                3,
                "+ t-args\n+ t-args\nsh: /tmp/s.log: No space left on device; the transcript ends here\n"
                    .into()
            )
        );
    }

    #[test]
    fn a_transcript_that_cannot_be_pushed_runs_nothing() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\n");
        h.programs.push_error = Some(Errno::EBUSY);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                1,
                "sh: cannot write the transcript /tmp/s.log: Device or resource busy\n".into()
            )
        );
        assert!(h.programs.spawned.is_empty());
        // Quoted as `sh` quotes it when it cannot empty the transcript.
        h.put("/tmp/my s.sh", b"t-args\n");
        assert_eq!(
            h.sh(&["/tmp/my s.sh"], &mut out),
            (
                1,
                "sh: cannot write the transcript '/tmp/my s.log': Device or resource busy\n".into()
            )
        );
    }

    #[test]
    fn bin_sh_keeps_sh_s_rules() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\n");
        let mut out = FakeStdout::file(None);
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (1, "sh: a script's output cannot be redirected\n".into())
        );
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/nope.sh"], &mut out),
            (1, "sh: /tmp/nope.sh: No such file or directory\n".into())
        );
        assert!(h.programs.spawned.is_empty() && h.programs.pushed.is_empty());
    }

    #[test]
    fn dollar_question_is_the_last_line_s_status() {
        let mut h = Harness::new();
        // What bash prints for each.
        assert_eq!(
            h.lines(&["nope", "echo $?", "echo \"$?\"", "echo 'open", "echo ${?}"]),
            (
                0,
                "relay-sh: nope: command not found\n127\n0\n\
                 relay-sh: syntax error: unterminated quote\n2\n"
                    .into()
            )
        );
        // A pipeline's is its last command's; a blank line keeps it.
        assert_eq!(
            h.lines(&["cat /nope | wc -l", "echo $?", "cat /nope", "", "echo $?"]),
            (
                0,
                "cat: /nope: No such file or directory\n0\n0\n\
                 cat: /nope: No such file or directory\n1\n"
                    .into()
            )
        );
    }

    #[test]
    fn the_shell_has_no_arguments_and_no_variables_set() {
        let mut h = Harness::new();
        // bash's `$0` is its own name; this shell's is `relay-sh` in the
        // in-process runner. bash sets `HOME` and others: no variable is
        // set here.
        assert_eq!(
            h.run(r#"echo $0 $# [$1] [$@] [$HOME] "[$UNSET]""#),
            (0, "relay-sh 0 [] [] [] []\n".into())
        );
        let mut h = spawning();
        h.spawning(r#"t-args $? "$E" $E ${E}x "~/$E" ~/$E"#);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "0", "", "x", "~/", "/root/"]
        );
    }

    #[test]
    fn a_shell_s_name_is_its_argument_0() {
        let mut h = spawning();
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .named("/bin/sh")
            .execute("t-args $0 $#");
        assert_eq!(h.programs.spawned[0].args, ["t-args", "/bin/sh", "0"]);
        // A script's is its file, whatever the shell's.
        h.put("/tmp/s.sh", b"t-args $0\n");
        let mut out = FakeStdout::console();
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .named("sh")
            .run_file(&["/tmp/s.sh".into()], &mut out);
        assert_eq!(h.programs.spawned[1].args, ["t-args", "/tmp/s.sh"]);
    }

    #[test]
    fn a_line_that_does_not_expand_runs_nothing() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo hi > ${1A}"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        // bash's: an unquoted target that is no word.
        assert_eq!(
            h.run("echo hi > $NONE"),
            (1, "relay-sh: $NONE: ambiguous redirect\n".into())
        );
        assert_eq!(
            h.run(r#"echo hi > "$NONE""#),
            (1, "relay-sh: : No such file or directory\n".into())
        );
        let mut h = spawning();
        assert_eq!(
            h.spawning("t-args | t-args > $NONE"),
            (1, "relay-sh: $NONE: ambiguous redirect\n".into())
        );
        assert!(h.programs.spawned.is_empty(), "nothing of it started");
    }

    #[test]
    fn a_command_that_expands_to_nothing_runs_nothing() {
        let mut h = Harness::new();
        // bash's: status 0, and a redirection alone still makes its file.
        assert_eq!(h.lines(&["nope", "$E"]).0, 0);
        assert_eq!(h.run("$E ${E} > /tmp/f"), (0, "".into()));
        assert!(h.exists("/tmp/f"));
        // In a pipeline it reads nothing and gives the next one nothing.
        assert_eq!(h.run("$E | wc -c"), (0, "0\n".into()));
        assert_eq!(h.run("echo hi | $E"), (0, "".into()));
        assert_eq!(h.run("echo hi | $E | $E > /tmp/g"), (0, "".into()));
        assert_eq!(h.get("/tmp/g"), b"");
        // A script traces it and runs it.
        h.put("/tmp/s.sh", b"nope\n$E\necho $?\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ nope\nrelay-sh: nope: command not found\n+ $E\n+ echo $?\n0\n".into()
            )
        );
    }

    #[test]
    fn a_program_s_neighbour_that_expands_to_nothing_is_an_end() {
        let mut h = spawning();
        assert_eq!(h.spawning("$E | t-args"), (3, "".into()));
        let s = &h.programs.spawned[0];
        assert_eq!((s.args.len(), s.group), (1, crate::Group::New));
        let (r, w) = h.programs.pipes[0];
        assert_eq!(s.stdin, Some(r), "the pipe it reads");
        assert!(h.programs.closed.contains(&w), "with no writer");
        assert_eq!(h.spawning("t-args | $E > /tmp/o"), (0, "".into()));
        let (path, append, fd) = h.programs.opened.last().unwrap();
        assert_eq!(
            (path.as_str(), *append),
            ("/tmp/o", false),
            "made, as bash's"
        );
        assert!(h.programs.closed.contains(fd));
        // In the background: no job, or one of what started.
        let mut h = with_jobs();
        let out = typed(&mut h, &["$E &", "$E | sleep 1 &", ""]);
        assert!(
            out.starts_with("root@relay:/# $E &\nroot@relay:/# $E | sleep 1 &\n[1] 101\n"),
            "{out}"
        );
        assert!(
            out.contains("[1]+  Done                    $E | sleep 1\n"),
            "{out}"
        );
    }

    #[test]
    fn an_assignment_sets_a_variable_for_the_lines_after_it() {
        let mut h = Harness::new();
        // What bash prints for each.
        assert_eq!(
            h.lines(&[
                "A=1 B=$A C=",
                "echo $A $B [$C] \"[$C]\"",
                "nope",
                "D=$? E=\"a  b\" F=~/x",
                "echo $? $D \"$E\" $F",
                "A=${A}2",
                "echo $A",
            ]),
            (
                0,
                "1 1 [] []\nrelay-sh: nope: command not found\n0 127 a  b /root/x\n12\n".into()
            )
        );
        // A value with blanks stays one word (bash would split it).
        let mut h = spawning();
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.execute("A='a b' P=t-args");
        assert_eq!(shell.execute("$P $A"), 3);
        assert_eq!(h.programs.spawned[0].args, ["t-args", "a b"]);
    }

    #[test]
    fn a_variable_may_name_a_built_in() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["C=cd D=/etc", "$C $D", "pwd"]),
            (0, "/etc\n".into())
        );
    }

    #[test]
    fn an_assignment_with_a_redirection_makes_its_file() {
        let mut h = Harness::new();
        // bash's: the variable is set even when the file cannot be made.
        assert_eq!(h.lines(&["A=1 > /tmp/f", "echo $A"]), (0, "1\n".into()));
        assert!(h.exists("/tmp/f"));
        assert_eq!(
            h.lines(&["B=2 > /nope/f", "echo $? $B"]),
            (
                0,
                "relay-sh: /nope/f: No such file or directory\n1 2\n".into()
            )
        );
        assert_eq!(
            h.lines(&["B=3 > $NONE", "echo $? $B"]),
            (0, "relay-sh: $NONE: ambiguous redirect\n1 3\n".into())
        );
    }

    #[test]
    fn an_assignment_cannot_be_in_a_pipeline_or_the_background() {
        // bash runs it in a shell of its own there, so it sets nothing.
        let mut h = spawning();
        for (line, said) in [
            ("A=1 | t-args", "A=1: cannot be used in a pipeline"),
            ("t-args | A='x y'", "A='x y': cannot be used in a pipeline"),
            ("A=1 &", "A=1: cannot be used in the background"),
            (
                "A=1 B=2 | t-args &",
                "A=1: cannot be used in the background",
            ),
        ] {
            assert_eq!(
                h.spawning(line),
                (1, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert!(h.programs.spawned.is_empty(), "nothing of it ran");
        // A word that is no assignment is a command, as in bash.
        assert_eq!(
            h.run("1A=x"),
            (127, "relay-sh: 1A=x: command not found\n".into())
        );
    }

    #[test]
    fn a_line_beyond_the_limits_runs_nothing() {
        let mut h = spawning();
        let big = "x".repeat(40_000);
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        assert_eq!(shell.execute(&alloc::format!("A={big}")), 0);
        assert_eq!(shell.execute(&alloc::format!("B={big} C=1")), 1);
        assert_eq!(shell.execute("t-args $A $A"), 1);
        assert_eq!(
            h.console.take(),
            "relay-sh: B: the variables would hold more than 64 KiB\n\
             relay-sh: the line would expand to more than 64 KiB\n"
        );
        assert!(h.programs.spawned.is_empty(), "nothing started");
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.execute(&alloc::format!("A={big}"));
        shell.execute(&alloc::format!("B={big} C=1"));
        shell.execute(r#"t-args "[$B$C]""#);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "[]"],
            "neither B nor what came after it"
        );
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
        let mut h = Harness::new();
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.execute("nope");
        assert_eq!(shell.execute("   "), 127);
        // So does a comment, and neither is followed by a sync.
        assert_eq!(shell.execute("# echo hi"), 127);
        assert_eq!(h.spy.syncs.get(), 1);
        assert_eq!(h.console.text(), "relay-sh: nope: command not found\n");
    }

    #[test]
    fn redirection_truncates_or_appends() {
        let mut h = Harness::new();
        assert_eq!(h.run("echo one > /tmp/f"), (0, String::new()));
        h.run("echo two >> /tmp/f");
        assert_eq!(h.get("/tmp/f"), b"one\ntwo\n");
        h.run("echo three > /tmp/f");
        assert_eq!(h.get("/tmp/f"), b"three\n");
        // A bare redirection creates or empties the file.
        h.run("> /tmp/f");
        assert_eq!(h.get("/tmp/f"), b"");
        h.run(">> /tmp/new");
        assert_eq!(h.get("/tmp/new"), b"");
    }

    #[test]
    fn errors_go_to_the_screen_not_into_the_file() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("cd /missing > /tmp/out"),
            (
                1,
                "relay-sh: cd: /missing: No such file or directory\n".into()
            )
        );
        assert_eq!(h.get("/tmp/out"), b"");
    }

    #[test]
    fn a_redirection_that_cannot_open_stops_the_command() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo x > /tmp"),
            (1, "relay-sh: /tmp: Is a directory\n".into())
        );
        assert_eq!(
            h.run("echo x > /nope/f"),
            (1, "relay-sh: /nope/f: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("echo x > /etc/motd/f"),
            (1, "relay-sh: /etc/motd/f: Not a directory\n".into())
        );
    }

    #[test]
    fn a_full_disk_is_a_write_error() {
        // The standard tree's two files take two of the three 4 KiB chunks.
        let mut h = Harness::with_capacity(3 * 4096);
        let big = "x".repeat(3000);
        assert_eq!(h.run(&alloc::format!("echo {big} > /tmp/a")).0, 0);
        assert_eq!(
            h.run(&alloc::format!("echo {big} > /tmp/b")),
            (1, "echo: write error: No space left on device\n".into())
        );
    }

    #[test]
    fn a_filesystem_that_writes_nothing_is_a_write_error_not_a_hang() {
        let mut h = Harness::new();
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("echo x > /tmp/f"),
            (1, "echo: write error: No space left on device\n".into())
        );
    }

    #[test]
    fn every_command_is_followed_by_a_sync() {
        let mut h = Harness::new();
        h.run("pwd");
        h.run("nope");
        h.run("echo 'unterminated");
        assert_eq!(h.spy.syncs.get(), 3);
        h.run("");
        assert_eq!(h.spy.syncs.get(), 3, "a blank line is not a command");
        h.spy.fail_sync.set(Some(Errno::EIO));
        assert_eq!(
            h.run("pwd").1,
            "/\nrelay-sh: sync failed: Input/output error\n"
        );
    }

    #[test]
    fn greet_shows_the_motd_and_goes_home() {
        let mut h = Harness::new();
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).greet();
        assert_eq!(h.console.take(), "Welcome to Relay OS.\n");
        assert_eq!(h.run("pwd").1, "/root\n");
    }

    #[test]
    fn run_reads_lines_until_input_ends() {
        let mut h = Harness::new();
        h.console.type_in(b"echo hi\rpwd\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(
            h.console.text(),
            "root@relay:/# echo hi\nhi\nroot@relay:/# pwd\n/\nroot@relay:/# "
        );
    }

    #[test]
    fn ctrl_c_gives_a_fresh_prompt() {
        let mut h = Harness::new();
        h.console.type_in(b"echo no\x03pwd\r");
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 0);
        assert!(
            h.console
                .text()
                .contains("echo no^C\nroot@relay:/# pwd\n/\n")
        );
    }

    #[test]
    fn without_motd_or_root_the_shell_starts_in_slash() {
        let mut h = Harness::empty();
        h.console.type_in(b"pwd\r");
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.greet();
        shell.run();
        assert_eq!(h.console.text(), "root@relay:/# pwd\n/\nroot@relay:/# ");
    }
}
