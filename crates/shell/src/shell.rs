//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command or a program and syncing the filesystems after it
//! (spec §7.3, §8.3; user-space gate §8.2).

use crate::commands::{self, SCRIPT_MAX, Script};
use crate::ctx::{Ctx, JobControl, quote_if_needed};
use crate::editor::{Feed, LineEditor};
use crate::expand::{self, Vars};
use crate::fds::{self, Fds, Files, Handle, Opener, Slot};
use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::jobs::Jobs;
use crate::parser::{self, HOME};
use crate::reader::Reader;
use crate::runner::{self, PIPED_MESSAGE_MAX, Parts, Ran, Runners};
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
/// The prompt while a command needs more lines, bash's `PS2`.
const CONTINUE: &str = "> ";
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
    /// What fds 0, 1 and 2 are where the walker stands (programmable shell
    /// gate §7.4).
    fds: Fds,
    /// The files redirections have open.
    files: Files,
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
            fds: Fds::SHELL,
            files: Files::default(),
        }
    }

    /// The same shell, its `$0` `name`: its argument 0, as bash's is
    /// (`relay-sh` otherwise).
    pub fn named(mut self, name: &str) -> Shell<'a> {
        self.vars.set_name(name);
        self
    }

    /// The same shell, its environment `block` imported as exported
    /// variables, and `PWD` and `OLDPWD` set as a shell sets them when it
    /// starts (programmable shell gate §8.5).
    pub fn with_environment(mut self, block: &[u8]) -> Shell<'a> {
        self.vars.import(block);
        // Said once, as an assignment that does not fit is.
        if let Err(e) = start_variables(&mut self.vars, &mut *self.vfs) {
            self.console.write(format!("{NAME}: {e}\n").as_bytes());
        }
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

    /// `root@relay:<cwd># `, `$HOME` and below shown as `~`, as bash's `\w`
    /// shows them: only when `HOME` is set and longer than one byte
    /// (programmable shell gate §15 item 7).
    pub fn prompt(&self) -> String {
        let cwd = path::display(&self.vfs.cwd());
        let home = self.vars.value("HOME").filter(|h| h.len() > 1);
        let dir = match home.and_then(|h| cwd.strip_prefix(h)) {
            Some("") => String::from("~"),
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => cwd,
        };
        format!("root@relay:{dir}# ")
    }

    /// Reads and runs commands until the input ends, `exit` or
    /// `reboot`/`poweroff` return. While a command needs more lines the
    /// prompt is bash's `> ` (programmable shell gate §5.5): Ctrl-C there
    /// drops the command, and the input's end is bash's `unexpected end of
    /// file`. Jobs that ended are reported before a full prompt only.
    pub fn run(&mut self) {
        self.stopped = false;
        self.prompting = true;
        let mut reader = Reader::new();
        while !self.stopped {
            let prompt = if reader.reading() {
                String::from(CONTINUE)
            } else {
                self.collect_jobs();
                for line in self.jobs.report() {
                    self.say(line.as_bytes());
                }
                self.prompt()
            };
            let mut out = Vec::new();
            self.editor.start(&prompt, self.console.columns(), &mut out);
            self.console.write(&out);
            loop {
                let Some(byte) = self.console.read_byte() else {
                    if let Some(e) = reader.end() {
                        self.finish(SYNTAX, format!("{NAME}: {e}\n"));
                    }
                    return;
                };
                out.clear();
                let feed = self.editor.feed(byte, &mut out);
                self.console.write(&out);
                match feed {
                    Feed::Pending => {}
                    Feed::Cancelled => {
                        reader.clear();
                        self.status = CANCELLED;
                        break;
                    }
                    Feed::Line(line) => {
                        // What ended while it was typed frees its slot in
                        // the process table before the line runs; it is
                        // reported at the next prompt.
                        self.collect_jobs();
                        self.read_line(&mut reader, &line);
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
        if self.vfs.chdir(HOME.as_bytes()).is_ok() {
            let _ = self.vars.set("PWD", String::from(HOME));
        }
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

    /// Runs a command line's list (programmable shell gate §5.1).
    fn run_list(&mut self, list: &parser::List<parser::Word>) -> i32 {
        self.abandoned = false;
        self.cancelled = false;
        // A background job needs programs: the in-process runner refuses
        // a line that holds one, anywhere, before any of it runs.
        if self.runner.programs().is_none() && list.has_job() {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
        }
        let status = self.run_items(list);
        // bash's `$?` after Ctrl-C ends a command line is 130, but for a
        // lone pipeline of simple commands, which keeps its own (`! sleep
        // 5` gives 0).
        if self.cancelled && !lone_pipeline(list) {
            self.status = CANCELLED;
            return CANCELLED;
        }
        status
    }

    /// Runs a list's items one after another; its status is the last
    /// one's. An empty list keeps the last status.
    /// `exit`, Ctrl-C (status 130, spec §6.4) and an expansion that
    /// abandons the line stop the rest.
    fn run_items(&mut self, list: &parser::List<parser::Word>) -> i32 {
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
        // A Ctrl-C typed while the shell runs its own commands, which no
        // program's end reports: a loop of them ends too (§5.2).
        if self.console.interrupted() {
            self.cancelled = true;
            return self.finish(CANCELLED, String::from("^C\n"));
        }
        let status = match &typed.run {
            parser::Run::Commands(commands) => self.run_commands(commands, background),
            parser::Run::Compound(c) if typed.redirects.is_empty() => {
                self.status = self.run_compound(c);
                self.status
            }
            parser::Run::Compound(c) => {
                self.status = self.run_redirected(c, &typed.redirects);
                self.status
            }
        };
        if !typed.negated || background.is_some() || self.stopped || self.abandoned {
            return status;
        }
        self.status = i32::from(status == 0);
        self.status
    }

    /// Runs a compound command with its redirections (programmable shell
    /// gate §7.4), made once before it starts: every command inside starts
    /// from the context they make, and the files are closed after its
    /// last. One that cannot be made runs nothing of it, status 1.
    fn run_redirected(
        &mut self,
        c: &parser::Compound<parser::Word>,
        redirects: &[parser::Redirect<parser::Word>],
    ) -> i32 {
        let Some(fds) = self.make(self.fds, redirects) else {
            return self.finish(1, String::new());
        };
        let outer = core::mem::replace(&mut self.fds, fds);
        let status = self.run_compound(c);
        self.fds = outer;
        self.release(fds);
        status
    }

    /// Runs a compound command (programmable shell gate §5.1).
    fn run_compound(&mut self, c: &parser::Compound<parser::Word>) -> i32 {
        match c {
            parser::Compound::If(i) => self.run_if(i),
            parser::Compound::While(l) => self.run_loop(l, false),
            parser::Compound::Until(l) => self.run_loop(l, true),
            parser::Compound::For(f) => self.run_for(f),
        }
    }

    /// Runs a `for`: its body once for each of its words, expanded as
    /// arguments are (or `"$@"`'s), its variable set to the word first
    /// (programmable shell gate §5.1). Its status is the body's last, 0
    /// if it never ran. A name that is not one is bash's error, as it is
    /// typed; a value the variables cannot hold fails the loop.
    fn run_for(&mut self, f: &parser::For<parser::Word>) -> i32 {
        let name = f.name.typed.as_str();
        if !parser::is_name(name) {
            return self.finish(1, format!("{NAME}: `{name}': not a valid identifier\n"));
        }
        let words = match &f.words {
            Some(words) => match expand::words(words, &self.vars, self.status) {
                Ok(words) => words,
                Err(e) => return self.not_expanded(e, true),
            },
            None => self.vars.positional().to_vec(),
        };
        let mut status = 0;
        for word in words {
            if let Err(e) = self.vars.set(name, word) {
                return self.not_expanded(e, false);
            }
            status = self.run_items(&f.body);
            if self.ends_line() {
                return status;
            }
        }
        status
    }

    /// Runs a `while` loop, or an `until` one: the condition, then the
    /// body while its status is 0 (or, `until`, not 0). The status is the
    /// body's last, 0 if it never ran. A loop may run for ever: Ctrl-C
    /// ends it, as the walker asks before each command (§5.2).
    fn run_loop(&mut self, l: &parser::Loop<parser::Word>, until: bool) -> i32 {
        let mut status = 0;
        loop {
            let condition = self.run_items(&l.condition);
            if self.ends_line() {
                return condition;
            }
            if (condition == 0) == until {
                return status;
            }
            status = self.run_items(&l.body);
            if self.ends_line() {
                return status;
            }
        }
    }

    /// Runs an `if`: each condition in turn, up to one whose status is 0,
    /// then its body; or, if none is, the `else` body. Its status is the
    /// body's, 0 when none runs.
    fn run_if(&mut self, i: &parser::If<parser::Word>) -> i32 {
        for (condition, body) in &i.branches {
            let status = self.run_items(condition);
            if self.ends_line() {
                return status;
            }
            if status == 0 {
                return self.run_items(body);
            }
        }
        match &i.otherwise {
            Some(body) => self.run_items(body),
            None => 0,
        }
    }

    /// Runs a pipeline's commands, or starts them in the background, their
    /// words expanded just before.
    fn run_commands(
        &mut self,
        commands: &[parser::Command<parser::Word>],
        background: Option<&str>,
    ) -> i32 {
        // What ended frees its slot in the process table before anything
        // starts, so a loop that starts jobs never fills it; they are
        // reported at the next prompt.
        self.collect_jobs();
        let alone = commands
            .iter()
            .filter(|c| c.words.is_empty())
            .find_map(|c| c.assigns.first());
        if let Some(first) = alone {
            // Alone on its line; bash's changes nothing elsewhere.
            let place = match (background, commands.len()) {
                (Some(_), _) => "the background",
                (None, 1) => return self.assign(&commands[0]),
                (None, _) => "a pipeline",
            };
            let message = format!("{NAME}: {}: cannot be used in {place}\n", first.typed);
            return self.finish(1, message);
        }
        // A pipeline or a job is expanded whole before it starts (§10); a
        // lone command's words first, then each redirection's target as
        // it is reached, as bash expands them.
        if commands.len() > 1 || background.is_some() {
            return match expand::expand(commands, &self.vars, self.status) {
                Ok(p) => match background {
                    Some(text) => self.background(&p, text),
                    None => self.pipeline(&p),
                },
                Err(e) => self.not_expanded(e, false),
            };
        }
        let typed = &commands[0];
        let (words, assigns) = match expand::command_parts(typed, &self.vars, self.status) {
            Ok(parts) => parts,
            Err(e) => return self.not_expanded(e, true),
        };
        if words.is_empty() {
            // Before words that expanded to nothing the assignments stay
            // set, as a line of assignments does in bash.
            for (name, value) in assigns.iter().filter_map(|a| a.split_once('=')) {
                if let Err(e) = self.vars.set(name, String::from(value)) {
                    return self.not_expanded(e, true);
                }
            }
        }
        if words.is_empty() && typed.redirects.is_empty() {
            // Its words expanded to nothing: bash's status 0.
            return self.finish(0, String::new());
        }
        let Some(fds) = self.make(self.fds, &typed.redirects) else {
            return self.finish(1, String::new());
        };
        let env = self.vars.environment_with(&assigns);
        // Before a built-in the assignments are set while it runs.
        let builtin = words.first().and_then(|name| commands::builtin(name));
        if builtin.is_some()
            && let Err(e) = self.vars.hold(&assigns)
        {
            self.release(fds);
            return self.not_expanded(e, true);
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
            files: &mut self.files,
            env: &env,
        };
        let ran = match words.split_first() {
            Some((name, args)) => match builtin {
                Some(builtin) => {
                    let control = JobControl {
                        jobs: &mut self.jobs,
                        vars: &mut self.vars,
                        programs: self.runner.programs(),
                        report: self.prompting && !self.in_script,
                    };
                    runner::run_function(parts, builtin, args, fds, Some(control))
                }
                None => {
                    let ran = self.runner.get().run(parts, name, args, fds);
                    self.console.take_back();
                    ran
                }
            },
            // A bare `> file` just creates or empties the file.
            None => Ran::said(0, String::new()),
        };
        self.vars.release();
        let mut message = ran.message;
        if ran.own {
            self.say_on(fds, message.as_bytes());
            message.clear();
        }
        self.stopped = ran.stop;
        self.exited = ran.exited;
        self.cancelled |= ran.cancelled;
        let mut status = ran.status;
        // A script `sh` read runs with its redirections: its commands read
        // a file it was given as input.
        if let Some(script) = ran.script {
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script);
            self.fds = outer;
        }
        self.release(fds);
        self.finish(status, message)
    }

    /// A line of assignments (spec §9.4): each sets its variable in turn,
    /// so a later one reads an earlier one, and the status is 0. A
    /// redirection after them makes its file, as bash's does.
    fn assign(&mut self, cmd: &parser::Command<parser::Word>) -> i32 {
        for (name, value) in cmd.assigns.iter().filter_map(parser::Word::assignment) {
            let set =
                expand::value(&value, &self.vars, self.status).and_then(|v| self.vars.set(name, v));
            if let Err(e) = set {
                return self.not_expanded(e, true);
            }
        }
        match self.make(self.fds, &cmd.redirects) {
            Some(fds) => {
                self.release(fds);
                self.finish(0, String::new())
            }
            None => self.finish(1, String::new()),
        }
    }

    /// Runs a pipeline (user-space gate §9.1): its status is the last
    /// command's. The shell's own commands cannot be in one.
    fn pipeline(&mut self, stages: &[parser::Command]) -> i32 {
        if let Some(name) = builtin_in(stages) {
            let ran = runner::in_a_pipeline(name);
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let env = Vec::new();
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
            files: &mut self.files,
            env: &env,
        };
        let ran = self.runner.get().pipeline(parts, &staged, &self.vars);
        // The last command's own message (the in-process runner's) on its
        // fd 2, as a lone command's.
        let mut message = ran.message;
        if ran.own
            && let Some((Some(fds), _)) = all.last()
        {
            self.say_on(*fds, message.as_bytes());
            message.clear();
        }
        for fds in all.into_iter().filter_map(|(fds, _)| fds) {
            self.release(fds);
        }
        self.console.take_back();
        self.cancelled |= ran.cancelled;
        self.finish(ran.status, message)
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
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let env = Vec::new();
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: None,
            files: &mut self.files,
            env: &env,
        };
        let started = self.runner.get().background(parts, &staged, &self.vars);
        for fds in all.into_iter().filter_map(|(fds, _)| fds) {
            self.release(fds);
        }
        if let Some(pgid) = started.pgid {
            let number = self.jobs.add(pgid, &started.pids, text);
            // On fd 2 of the context, as bash says it (`for …; do a &
            // done 2> e` writes it into `e`).
            if self.prompting && !self.in_script {
                let last = started.pids.last().copied().unwrap_or(pgid);
                self.say_on(self.fds, format!("[{number}] {last}\n").as_bytes());
            }
        }
        self.finish(started.ran.status, started.ran.message)
    }

    /// The context the typed `redirects` make over `base`, each target
    /// expanded as it is reached, as bash does (programmable shell gate
    /// §15 item 5): a target that does not expand is told on fd 2 as it
    /// stands then (`2> e > $E` writes `$E: ambiguous redirect` into `e`),
    /// and a bad substitution abandons the line too, as in bash.
    fn make(&mut self, base: Fds, redirects: &[parser::Redirect<parser::Word>]) -> Option<Fds> {
        let mut expanded = Vec::new();
        let mut failed = None;
        for r in redirects {
            match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => expanded.push(r),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        let fds = self.redirect(base, &expanded)?;
        let Some(e) = failed else {
            return Some(fds);
        };
        self.abandoned = matches!(
            e,
            expand::Error::BadSubstitution(_) | expand::Error::TooLong
        );
        self.say_on(fds, format!("{NAME}: {e}\n").as_bytes());
        self.release(fds);
        None
    }

    /// The context `redirects` make over `base` (programmable shell gate
    /// §7.2). One that cannot be made is told on fd 2 as it stands then,
    /// as bash tells it (`cat 2> e < missing` writes into `e`), and what
    /// was made of it is closed.
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Option<Fds> {
        match self.made(base, redirects) {
            Ok(fds) => Some(fds),
            Err((fds, message)) => {
                self.say_on(fds, message.as_bytes());
                self.release(fds);
                None
            }
        }
    }

    /// The context `redirects` make over `base`, or what was made of it
    /// when one cannot be made, and its message.
    fn made(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Result<Fds, (Fds, String)> {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        self.files
            .redirect(base, redirects, &mut opener)
            .map_err(|failed| {
                let message = format!("{NAME}: {}: {}\n", failed.path, failed.error);
                (failed.fds, message)
            })
    }

    /// Writes where fd 2 of `fds` goes: the screen (and a running
    /// script's transcript), or a file. A write that fails is lost, as
    /// there is nowhere left to say so.
    fn say_on(&mut self, fds: Fds, bytes: &[u8]) {
        let Slot::File(i) = fds.0[2] else {
            return self.say(bytes);
        };
        match self.files.handle(i) {
            Handle::Fd(fd) => {
                if let Some(programs) = self.runner.programs() {
                    let _ = programs.write(fd, bytes);
                }
            }
            Handle::Node { node, offset } => {
                let (at, _) = fds::write_file(&mut *self.vfs, node, offset, bytes);
                self.files.set_offset(i, at);
            }
        }
    }

    /// A context made by `redirect` ends: the files it held last are
    /// closed.
    fn release(&mut self, fds: Fds) {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        self.files.release(fds, &mut opener);
    }

    /// Each stage's fds (programmable shell gate §7.3): the pipe from the
    /// one before, the pipe to the one after, the context's for the rest,
    /// its redirections made over them; none for a stage whose redirection
    /// could not be made, which runs nothing. Its message is told on its
    /// fd 2 as it stood then, which after `2>&1` is the pipe: the runner
    /// writes it there (`cat 2>&1 < /nope | wc -l`, as bash's).
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Vec<(Option<Fds>, Option<String>)> {
        let last = stages.len() - 1;
        let mut all = Vec::new();
        for (i, stage) in stages.iter().enumerate() {
            let input = if i == 0 { self.fds.0[0] } else { Slot::PipeIn };
            let output = if i == last {
                self.fds.0[1]
            } else {
                Slot::PipeOut
            };
            let base = Fds([input, output, self.fds.0[2]]);
            all.push(match self.made(base, &stage.redirects) {
                Ok(fds) => (Some(fds), None),
                Err((fds, message)) => {
                    let piped = fds.0[2] == Slot::PipeOut && message.len() <= PIPED_MESSAGE_MAX;
                    if !piped {
                        self.say_on(fds, message.as_bytes());
                    }
                    self.release(fds);
                    (None, piped.then_some(message))
                }
            });
        }
        all
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
    /// its own, the exported variables imported, and starts with `$?` 0, as
    /// there; and its `cd` stays in it.
    fn run_script(&mut self, script: Script) -> i32 {
        let cwd = self.vfs.cwd();
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(&script.environment);
        let started = start_variables(&mut vars, &mut *self.vfs);
        let outer = core::mem::replace(&mut self.vars, vars);
        self.status = 0;
        if let Err(e) = started {
            self.say(format!("{NAME}: {e}\n").as_bytes());
        }
        let status = self.run_lines(&script.text);
        self.vars = outer;
        let _ = self.vfs.chdir(&cwd);
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
        self.vars.set_args(&script.name, &script.args);
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
                    // Not kept, but counted whole: it says how much of what
                    // follows is dropped with it.
                    too_long = true;
                    reader.drop_bytes(&line);
                    line.clear();
                }
            } else {
                reader.drop_bytes(&byte);
            }
        }
    }

    /// One line `run_input` read: the command it finishes run, or said
    /// why not. A line that cannot be read drops the command it was in.
    fn input_line(&mut self, reader: &mut Reader, line: &[u8], too_long: bool) {
        self.collect_jobs();
        match core::str::from_utf8(line) {
            _ if too_long => {
                reader.drop_end();
                self.finish(1, String::from("sh: standard input: a line over 64 KiB\n"));
            }
            Ok(text) => {
                self.read_line(reader, text);
            }
            Err(_) => {
                reader.drop_line(line);
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
    /// transcript, syncs, and records `status`. The message is the shell's
    /// report, on fd 2 of the context (programmable shell gate §7.5), but
    /// for a `^C` at its end, the key's echo, which stays on the screen.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        let (report, ctrl_c) = match message.strip_suffix("^C\n") {
            Some(report) => (report, true),
            None => (message.as_str(), false),
        };
        self.say_on(self.fds, report.as_bytes());
        if ctrl_c {
            self.say(b"^C\n");
        }
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

/// A pipeline's stages for the runner: each one's words, fds and
/// assignments, and the message it sends into the pipe.
fn runner_stages<'c>(
    stages: &'c [parser::Command],
    fds: &'c [(Option<Fds>, Option<String>)],
) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .map(|(c, (fds, into_pipe))| runner::Stage {
            words: &c.words,
            fds: *fds,
            assigns: &c.assigns,
            into_pipe: into_pipe.as_deref(),
        })
        .collect()
}

/// Sets `PWD` and `OLDPWD` in a shell's `vars` as it starts, from the
/// current directory and whether the `OLDPWD` it imported is a directory.
fn start_variables(vars: &mut Vars, vfs: &mut dyn Vfs) -> Result<(), expand::Error> {
    let oldpwd_is_dir = vars.value("OLDPWD").is_some_and(|old| {
        vfs.lookup(old.as_bytes())
            .and_then(|node| vfs.stat(node))
            .is_ok_and(|s| s.kind == vfs::FileType::Directory)
    });
    vars.start(&path::display(&vfs.cwd()), oldpwd_is_dir)
}

/// Whether `list` is one pipeline of simple commands, and nothing else.
fn lone_pipeline(list: &parser::List<parser::Word>) -> bool {
    match &list.items[..] {
        [item] => {
            item.and_or.rest.is_empty() && matches!(item.and_or.first.run, parser::Run::Commands(_))
        }
        _ => false,
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
    use crate::runner::PIPED_MESSAGE_MAX;
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
    use relay_abi::WaitStatus;
    use vfs::Errno;

    #[test]
    fn the_prompt_shows_home_as_a_tilde() {
        let mut h = Harness::new();
        let prompt = |h: &mut Harness| {
            Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
                .with_environment(&h.env)
                .prompt()
        };
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(prompt(&mut h), "root@relay:/# ");
        h.run("cd /root");
        assert_eq!(prompt(&mut h), "root@relay:~# ");
        h.dir("/root/notes");
        h.run("cd notes");
        assert_eq!(prompt(&mut h), "root@relay:~/notes# ");
        h.dir("/rootless");
        h.run("cd /rootless");
        assert_eq!(prompt(&mut h), "root@relay:/rootless# ");
        // As bash's `\w`: `HOME` unset, empty, `/` or with a `/` at its end
        // shows none.
        h.run("cd /root/notes");
        for env in [&b""[..], b"HOME=\0", b"HOME=/\0", b"HOME=/root/\0"] {
            h.env = env.to_vec();
            assert_eq!(prompt(&mut h), "root@relay:/root/notes# ", "{env:?}");
        }
        h.env = b"HOME=/root/notes\0".to_vec();
        assert_eq!(prompt(&mut h), "root@relay:~# ");
        h.env = b"HOME=/\0".to_vec();
        h.run("cd /");
        assert_eq!(prompt(&mut h), "root@relay:/# ", "bash's `\\w` for HOME=/");
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
    fn a_refused_construct_runs_none_of_its_lines() {
        // Plan 1's final review ruled that a script's `if` written across
        // lines ran its body, each refused line dropped alone: the lines
        // of a construct with a refused line in it are dropped up to its
        // end, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo coproc d\ndone\nt-args e\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
        // Opened by a line too long, or no text.
        let mut text = b"while t-args a; do ".to_vec();
        text.extend(alloc::vec![b'x'; 70_000]);
        text.extend_from_slice(b"\nt-args b\ndone\nt-args next\n");
        assert_eq!(piped(&text), ["next"]);
        assert_eq!(
            piped(b"for x in \xff; do\nt-args b\ndone\nt-args next\n"),
            ["next"]
        );
        // In a script, each line traced as it is read.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"if true\nthen echo a\nwhile true\ndo coproc b\ndone\nfi\necho next\n",
        );
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ if true\n+ then echo a\n+ while true\n+ do coproc b\n\
                 relay-sh: unsupported syntax: coproc\n+ done\n+ fi\n+ echo next\nnext\n"
                    .into()
            )
        );
        // At the prompt, with `> ` until its end.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &["while t-args a", "do coproc b", "done", "t-args next"],
        );
        let args: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(args, ["next"]);
        assert_eq!(out.matches("\n> ").count(), 2, "{out}");
    }

    #[test]
    fn a_dropped_construct_s_refused_syntax_hides_none_of_its_end() {
        // The prototype's review ran each tail without its loop: a `fi` or
        // `done` that the refused syntax around it hides from bash was
        // counted, or an opener it shows to bash was not.
        let head = b"while t-args a; do\n";
        for inner in [
            &b"time for f in b\ndo t-args c\ndone\n"[..],
            b"select x in a b; do\nt-args c\ndone\n",
            b"{ if t-args b\nthen t-args c\nfi\n}\n",
            b"t-args ` if t-args b; then t-args c; fi `\n",
            b"t-args copy of $(hostname) done\n",
            b"case b in\ndone) t-args c;;\nesac\n",
            // The final review: a string, a `case` in `$(…)` and a `(` in
            // `${…}`, each read as bash reads them.
            b"t-args \"a\nfi b\"\n",
            b"v=$(case b in b) t-args 1;; esac; t-args 2)\n",
            b"v=$(case b in\nb) t-args 1;;\nesac\n)\n",
            b"t-args ${s//(/x}; if t-args b; then\nt-args c\nfi\n",
        ] {
            let mut text = head.to_vec();
            text.extend_from_slice(inner);
            text.extend_from_slice(b"t-args tail\ndone\nt-args next\n");
            assert_eq!(piped(&text), ["next"], "{}", String::from_utf8_lossy(inner));
        }
    }

    #[test]
    fn a_line_bash_ends_in_error_drops_no_more() {
        // The prototype's review: bash runs `next` after each.
        for text in [
            &b"t-args a && >\nt-args next\n"[..],
            b"if t-args a; then t-args b; fi if t-args c\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_dropped_command_s_redirections_end_its_drop_where_bash_s_end() {
        // Programmable shell gate §15 item 5: the drop scan reads every
        // form of §7.1, refused or not, as bash does: a redirection after
        // `fi` or `done` (its fd, `>&`, `<&`, `&>`, `>|`), `<<<` with no
        // body; a line ending after `&&` or `||` goes on.
        for text in [
            &b"if true; then\nt-args 3> x\nfi 2> /tmp/e\nt-args next\n"[..],
            b"while true; do\nt-args 3> x\ndone < /tmp/f\nt-args next\n",
            b"for x in a; do\nt-args 3> x\ndone 2>&1\nt-args next\n",
            b"for x in a; do\nt-args 3> x\ndone >&2\nt-args next\n",
            b"t-args a <<< x\nt-args next\n",
            b"if true; then\nt-args <<< x\nt-args body\nfi\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 2>&1 > /tmp/o\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi > /tmp/o 2>&1 &&\nt-args tail\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 2>&1 ||\nt-args tail\nt-args next\n",
            b"t-args 3>&1 &&\nt-args tail\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 0<&3\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi &> /tmp/o\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi >| /tmp/o\nt-args next\n",
            // A target is a file's name, never a keyword.
            b"if true; then\nt-args 3> x\n< fi t-args\nt-args body\nfi\nt-args next\n",
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn two_subshells_dropped_as_arithmetic_end_their_drop() {
        // Milestone 4's deferred minor M-2: `((a); b)`, two subshells to
        // bash, dropped the rest of the input.
        assert_eq!(piped(b"((t-args a); t-args b)\nt-args next\n"), ["next"]);
        assert_eq!(
            piped(b"t-args 3> x &&\n((t-args a) ; if t-args b\nfi )\nt-args next\n"),
            ["next"]
        );
    }

    #[test]
    fn a_group_after_time_s_options_or_coproc_is_dropped_whole() {
        // Milestone 4's deferred gap: the scan took `{` after `time -p` or
        // `coproc` for a word, so the group's lines ran without their
        // guard.
        for text in [
            &b"t-args 3> x &&\ntime -p {\nt-args body\n}\nt-args next\n"[..],
            b"t-args 3> x &&\ncoproc {\nt-args body\n}\nt-args next\n",
            b"t-args 3> x &&\ncoproc N {\nt-args body\n}\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_quote_in_a_substitution_in_double_quotes_ends_no_drop() {
        // Milestone 4's deferred gap: the scan took the `"` inside `$(…)`
        // for the end of the outer quotes, so the `fi` in the string ended
        // the drop and the line after it ran.
        let text = b"if true; then\nt-args 3> x\necho \"$(echo \"\nfi\nt-args body\n\")\"\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
    }

    #[test]
    fn a_here_document_s_body_does_not_run() {
        // The prototype's review: `cat > x <<EOF`, refused, then ran each
        // line of its body as a command.
        for text in [
            &b"t-args a <<EOF\nt-args body\nEOF\nt-args next\n"[..],
            b"while t-args a; do\nt-args <<EOF\ndone\nEOF\nt-args tail\ndone\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
        let mut h = Harness::new();
        h.run("mkdir /tmp/d");
        h.put(
            "/tmp/s.sh",
            b"cat > /tmp/x <<EOF\nrm -r /tmp/d\nEOF\necho next\n",
        );
        let (_, out) = h.run("sh /tmp/s.sh");
        assert!(out.ends_with("+ EOF\n+ echo next\nnext\n"), "{out}");
        assert!(h.exists("/tmp/d"));
    }

    #[test]
    fn arithmetic_is_dropped_alone() {
        // Plan 2's deferred minor: `((` is refused, and the drop read its
        // `<<` as a here-document's, so the rest of the script was dropped
        // without a word. bash runs `next` after each.
        for text in [
            &b"(( x = 1 << 2 ))\nt-args next\n"[..],
            b"for ((i = 0; i << 1; i++))\ndo t-args body\ndone\nt-args next\n",
            b"for((i = 0; i << 1; i++)); do\nt-args body\ndone\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
        // Two subshells with a here-document (the prototype's review): none
        // of its body runs, nor anything after it.
        let text = b"((cat <<EOF)\n)\nt-args ran\nEOF\n)\nt-args next\n";
        assert!(piped(text).is_empty());
    }

    #[test]
    fn a_dropped_group_runs_none_of_it() {
        // The prototype's review: a refused `a || {`, `a && (` or `f() {`
        // ended its drop at once, so the lines of its body ran without
        // their guard, `exit` among them. bash runs none of them here.
        for text in [
            &b"t-args a && {\nt-args ran\n}\nt-args next\n"[..],
            b"t-args a && (\nt-args ran\n)\nt-args next\n",
            b"f() {\nt-args ran\n}\nt-args next\n",
            b"t-args a || {\nt-args ran\nexit 1\n}\nt-args next\n",
            // The final review: a `)` whose `(` opened no subshell, the
            // `function f {` form, and a stray keyword's closer.
            b"function f {\nt-args ran\n}\nt-args next\n",
            b"t-args a || function f {\nt-args ran\n}\nt-args next\n",
            b"t-args a || (\narr=(x y)\nt-args ran\n)\nt-args next\n",
            b"t-args a || (\n[[ ( -f x ) ]] && t-args x\nt-args ran\n)\nt-args next\n",
            b"t-args a || (\nf() { t-args x; }\nt-args ran\n)\nt-args next\n",
            b"t-args a || (\nls @(a|b)\nt-args ran\n)\nt-args next\n",
            b"t-args a || {\nfunction g {\nt-args g\n}\nt-args ran\n}\nt-args next\n",
            b"t-args a || {\nfi\nt-args ran\n}\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn an_if_dropped_any_way_runs_none_of_it() {
        // Plan 1's lesson: its worst defects ran part of a command without
        // its guard. However an `if` is dropped, none of it runs, the line
        // after its end does.
        let tail = b"fi\nt-args next\n";
        let with = |head: &[u8], body: &[u8]| -> Vec<u8> {
            let mut text = b"if t-args a; then\n".to_vec();
            text.extend_from_slice(head);
            text.extend_from_slice(body);
            text.extend_from_slice(tail);
            text
        };
        let mut long = alloc::vec![b'x'; 70_000];
        long.push(b'\n');
        let deep: Vec<u8> = b"if t-args a; then\n".repeat(33);
        for (text, said) in [
            // A syntax error in an `if` inside it.
            (
                with(
                    b"if t-args b; then t-args c\nt-args d; then\nt-args e\nfi\n",
                    b"",
                ),
                "relay-sh: syntax error near unexpected token `then'\n",
            ),
            // Too long, a line that is no text, a line over 64 KiB.
            (
                with(&b"t-args b\n".repeat(8000), b""),
                "relay-sh: the command would be longer than 64 KiB\n",
            ),
            (
                with(b"\xff\n", b"t-args b\n"),
                "sh: standard input: not a text line\n",
            ),
            (
                with(&long, b"t-args b\n"),
                "sh: standard input: a line over 64 KiB\n",
            ),
            // Refused syntax, and nesting past the bound.
            (
                with(b"t-args $(b)\n", b"t-args c\n"),
                "relay-sh: unsupported syntax: $(\n",
            ),
            (
                with(&deep, &b"fi\n".repeat(33)),
                "relay-sh: unsupported syntax: more than 32 levels of nesting\n",
            ),
        ] {
            let mut h = spawning();
            let mut input = crate::Bytes::new(text.clone());
            Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                .run_input(&mut input);
            let ran: Vec<String> = h
                .programs
                .spawned
                .iter()
                .map(|s| s.args[1..].join(" "))
                .collect();
            assert_eq!(
                (ran, h.console.take()),
                (alloc::vec![String::from("next")], String::from(said)),
                "{said}"
            );
        }
        // The input's end inside it.
        let mut h = spawning();
        let mut input = crate::Bytes::new(b"if t-args a; then\nt-args b\n".to_vec());
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            (status, h.console.take(), h.programs.spawned.len()),
            (
                2,
                "relay-sh: syntax error: unexpected end of file\n".into(),
                0
            )
        );
    }

    #[test]
    fn a_script_s_if_is_traced_whole_before_it_runs_or_is_dropped() {
        // §5.4: each line once, as read; blank and comment lines not.
        let mut h = Harness::new();
        for (script, out, status) in [
            (
                &b"if true\nthen echo a\n\n  # c\nfi\necho b\n"[..],
                "+ if true\n+ then echo a\n+ fi\na\n+ echo b\nb\n",
                0,
            ),
            (
                b"if true; then\necho a; then\necho b\nfi\necho next\n",
                "+ if true; then\n+ echo a; then\n\
                 relay-sh: syntax error near unexpected token `then'\n\
                 + echo b\n+ fi\n+ echo next\nnext\n",
                0,
            ),
            (
                b"if true; then\necho a\n",
                "+ if true; then\n+ echo a\nrelay-sh: syntax error: unexpected end of file\n",
                2,
            ),
        ] {
            h.put("/tmp/s.sh", script);
            assert_eq!(h.run("sh /tmp/s.sh"), (status, String::from(out)));
        }
        // Ctrl-C between its lines ends the script and runs none of it.
        h.put("/tmp/s.sh", b"if true; then\necho a\nfi\necho b\n");
        h.console.interrupt_after = Some(2);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ if true; then\n^C\n".into()));
    }

    #[test]
    fn an_if_typed_at_the_prompt_runs_only_whole() {
        // Ctrl-C at `> ` drops it; a `fi` after it is bash's syntax error.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &["if t-args a; then", "t-args b", "\x03", "fi", "t-args next"],
        );
        let ran: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(ran, ["next"]);
        assert!(
            out.contains("^C") && out.contains("syntax error near unexpected token `fi'"),
            "{out}"
        );
        // A syntax error inside it drops it to its `fi`, with `> ` until then.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &[
                "if t-args a; then",
                "t-args b; then",
                "t-args c",
                "fi",
                "t-args next",
            ],
        );
        let ran: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(ran, ["next"]);
        assert_eq!(out.matches("\n> ").count(), 3, "{out}");
        // The input's end at `> `.
        let mut h = spawning();
        let out = typed(&mut h, &["if t-args a; then", "t-args b"]);
        assert!(h.programs.spawned.is_empty());
        assert!(
            out.ends_with("relay-sh: syntax error: unexpected end of file\n"),
            "{out}"
        );
    }

    #[test]
    fn jobs_shows_a_job_typed_across_lines_on_one_line() {
        let mut h = with_jobs();
        let mut input = crate::Bytes::new(b"sleep 5 |\n# c\ncat &\njobs\n".to_vec());
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        let out = h.console.take();
        assert!(
            out.contains("Done                    sleep 5 | cat\n"),
            "{out}"
        );
    }

    #[test]
    fn a_refused_line_drops_the_rest_of_its_chain() {
        // Under `X | sh` and `sh FILE` alike, the end of the chain never
        // runs without its guard (the final review found it run).
        assert_eq!(
            piped(b"t-args a $(x) &&\nt-args tail\nt-args next\n"),
            ["next"]
        );
        let mut h = spawning();
        h.put(
            "/tmp/s.sh",
            b"t-args a 3> log &&\nt-args tail\nt-args next\n",
        );
        let mut out = FakeStdout::console();
        h.sh(&["/tmp/s.sh"], &mut out);
        let args: Vec<String> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1..].join(" "))
            .collect();
        assert_eq!(args, ["next"]);
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
        // Typed once the shell's check before the pipeline has passed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big | wc -c"), (130, "^C\n".into()));
        // The rest does not run, even what would not have asked.
        h.console.interrupt_after = Some(1);
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
        h.console.interrupt_after = Some(1);
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
        h.console.interrupt_after = Some(1);
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
        // A pipeline a stage of which Ctrl-C killed, too; the line's `$?`
        // is then bash's 130.
        assert_eq!(
            h.spawning("t-spin | t-args x; t-args no"),
            (130, "^C\n".into())
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
        // `$?` is 130, as in bash, and the rest does not run.
        h.console.interrupt_after = Some(2);
        assert_eq!(
            h.run("! cat /tmp/big | wc -c; echo no"),
            (130, "^C\n".into())
        );
        h.console.interrupt_after = Some(2);
        assert_eq!(h.run("! cat /tmp/big | wc -c"), (0, "^C\n".into()), "alone");
        // Only that line: the next one runs whole.
        h.console.interrupt_after = Some(2);
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.execute("cat /tmp/big");
        assert_eq!(shell.execute("echo a; echo b"), 0);
        assert_eq!(h.console.take(), "^C\na\nb\n");
        h.console.interrupt = false;
        h.put("/tmp/s.sh", b"! cat /tmp/big | wc -c\necho no\n");
        h.console.interrupt_after = Some(3);
        assert_eq!(
            h.run("sh /tmp/s.sh").1,
            "+ ! cat /tmp/big | wc -c\n^C\n",
            "the script ends there"
        );
    }

    #[test]
    fn ctrl_c_ends_the_shell_s_own_commands_between_them() {
        // §5.2: the walker asks before each command, so a list or a
        // loop of built-ins ends too, with `^C` and 130.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cd; cd; cd; echo no"), (130, "^C\n".into()));
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("if cd; then cd; echo no; fi; echo no"),
            (130, "^C\n".into())
        );
        // Taken by the shell: the next line runs whole.
        h.console.interrupt_after = None;
        assert_eq!(h.run("cd; echo a"), (0, "a\n".into()));
    }

    #[test]
    fn the_shell_takes_the_console_back_after_each_program() {
        // The prototype's review: once a program had ended, its group kept
        // the console until the next prompt, so a Ctrl-C was lost for the
        // rest of the line. Built-ins never give it away.
        let mut h = spawning();
        h.spawning("t-args a; cd; t-args b | t-args c; cd");
        assert_eq!(h.console.taken_back, 2);
        h.console.taken_back = 0;
        h.spawning("cd; cd");
        assert_eq!(h.console.taken_back, 0);
    }

    #[test]
    fn after_ctrl_c_ends_a_command_line_its_status_is_130() {
        // Plan 1's deferred minor: bash's `$?` is 130 after Ctrl-C ends a
        // list, an and-or list (c1, c3, c8, c9, c17, c18) or a compound
        // command (c4, c7, c10–c16), and a lone pipeline's own (c2:
        // `! sleep 5` is 0). bash's lone `if` goes on to its `else` (c6);
        // §5.2 ends it.
        let mut h = Harness::new();
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        for (line, status) in [
            ("t-spin; t-args no", 130),
            ("! t-spin; t-args no", 130),
            ("t-spin && t-args no", 130),
            ("t-spin || t-args no", 130),
            ("! t-spin && t-args no", 130),
            ("! t-spin || t-args no", 130),
            ("if t-spin; then t-args no; else t-args no; fi", 130),
            ("! if t-spin; then t-args no; fi", 130),
            ("t-spin", 130),
            ("! t-spin", 0),
        ] {
            assert_eq!(h.spawning(line), (status, "^C\n".into()), "{line}");
        }
        assert!(h.programs.spawned.iter().all(|s| s.path == "/bin/t-spin"));
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
    fn an_if_runs_the_body_of_the_first_condition_that_succeeds() {
        let mut h = Harness::new();
        for (line, ran) in [
            ("if true; then echo a; fi", (0, "a\n")),
            (
                "if false; then echo a; elif true; then echo b; else echo c; fi",
                (0, "b\n"),
            ),
            (
                "if false; then echo a; elif false; then echo b; else echo c; fi",
                (0, "c\n"),
            ),
            (
                "if true; then echo a; if false; then echo b; else echo c; fi fi",
                (0, "a\nc\n"),
            ),
            // Its status is the body's (g6), 0 when none runs (g4, g5,
            // g13), and a condition's status is `$?` in the body.
            ("if true; then false; fi", (1, "")),
            ("if false; then true; fi", (0, "")),
            ("false; if false; then true; fi; echo $?", (0, "0\n")),
            ("if true && false; then echo a; fi", (0, "")),
            ("if false; then true; else echo $?; fi", (0, "1\n")),
            ("if ! true; then echo a; else echo b; fi", (0, "b\n")),
            (
                "if nosuch; then echo a; else echo b; fi",
                (0, "relay-sh: nosuch: command not found\nb\n"),
            ),
            // f3: negated, and in an and-or list (g11).
            ("! if false; then true; fi", (1, "")),
            (
                "echo a && if true; then echo b; fi || echo c",
                (0, "a\nb\n"),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
    }

    #[test]
    fn while_and_until_run_their_body_while_the_condition_allows() {
        // Each loop here ends by itself: on a file it removes or makes.
        let mut h = Harness::new();
        h.put("/tmp/f", b"f\n");
        assert_eq!(
            h.run("while cat /tmp/f; do rm /tmp/f; done"),
            (0, "f\ncat: /tmp/f: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("until cat /tmp/g; do echo g > /tmp/g; done"),
            (0, "cat: /tmp/g: No such file or directory\ng\n".into())
        );
        // The status is the body's last (r8), 0 when it never runs (g3,
        // g7), whatever the condition's or `$?` before.
        h.put("/tmp/f", b"f\n");
        assert_eq!(
            h.run("while cat /tmp/f; do rm /tmp/f; false; done; echo $?"),
            (0, "f\ncat: /tmp/f: No such file or directory\n1\n".into())
        );
        for (line, ran) in [
            ("while false; do echo no; done", (0, "")),
            ("false; while false; do echo no; done; echo $?", (0, "0\n")),
            ("until true; do echo no; done", (0, "")),
            ("! while false; do echo no; done", (1, "")),
            (
                "true && until true; do echo no; done && echo yes",
                (0, "yes\n"),
            ),
            // A condition's status is `$?` in the body.
            ("until false; do echo $?; exit 7; done", (7, "1\n")),
            // `exit` deep inside.
            (
                "while true; do if true; then exit 4; fi; echo no; done",
                (4, ""),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
        // A bad substitution abandons the whole line, or `exit` ends it,
        // in the body or the condition: no body runs after it.
        assert_eq!(
            h.run("while true; do echo ${1A}; echo no; done; echo no"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("until echo ${1A}; do echo no; done"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(h.run("until exit 5; do echo no; done"), (5, "".into()));
        // The in-process runner refuses a job in a loop too.
        for line in [
            "while false; do echo a & done",
            "until echo a & do echo b; done",
        ] {
            assert_eq!(
                h.run(line),
                (2, "relay-sh: unsupported syntax: &\n".into()),
                "{line}"
            );
        }
    }

    #[test]
    fn ctrl_c_ends_a_loop_of_built_ins_or_of_programs() {
        // A loop a person writes may run forever; Ctrl-C ends it (§5.2).
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.console.interrupt_after = Some(50);
        assert_eq!(
            h.run("while true; do cd; done; echo no"),
            (130, "^C\n".into())
        );
        h.console.interrupt_after = Some(50);
        assert_eq!(
            h.run("until false; do A=x; done; echo no"),
            (130, "^C\n".into())
        );
        let mut h = spawning();
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        // `t-args` exits with 3 here; a Ctrl-C in the condition runs no
        // body.
        assert_eq!(
            h.spawning("until t-args a; do t-spin; done; t-args no"),
            (130, "^C\n".into())
        );
        assert_eq!(
            h.spawning("until t-spin; do t-args no; done"),
            (130, "^C\n".into())
        );
        let ran: Vec<&str> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(ran, ["/bin/t-args", "/bin/t-spin", "/bin/t-spin"]);
    }

    #[test]
    fn a_for_runs_its_body_once_for_each_word() {
        let mut h = Harness::new();
        for (line, ran) in [
            ("for x in a b c; do echo $x; done", (0, "a\nb\nc\n")),
            // f9, r4, r5: a shell variable, its last value kept, none set
            // when there is no pass.
            ("for x in a b; do echo $x; done; echo $x", (0, "a\nb\nb\n")),
            ("for x in a; do x=z; done; echo $x", (0, "z\n")),
            ("x=y; for x in; do echo no; done; echo $x", (0, "y\n")),
            // r3: words expand as arguments do.
            (
                "for x in $E a '' \"$E\"; do echo [$x]; done",
                (0, "[a]\n[]\n[]\n"),
            ),
            // r8, r11: the body's last status, 0 with no pass.
            ("for x in a b; do false; done", (1, "")),
            ("false; for x in; do true; done; echo $?", (0, "0\n")),
            (
                "for x in a b; do echo $x; exit 6; done; echo no",
                (6, "a\n"),
            ),
            // g1, r1, r2: a name checked when it runs, as typed.
            (
                "echo a; for 1x in a; do echo no; done; echo b",
                (0, "a\nrelay-sh: `1x': not a valid identifier\nb\n"),
            ),
            (
                "for \"x\" in a; do echo no; done",
                (1, "relay-sh: `\"x\"': not a valid identifier\n"),
            ),
            (
                "for $E in a; do echo no; done",
                (1, "relay-sh: `$E': not a valid identifier\n"),
            ),
            // A bad substitution in its words abandons the line.
            (
                "for x in ${1A}; do echo no; done; echo no",
                (1, "relay-sh: ${1A}: bad substitution\n"),
            ),
            (
                "for x in a; do echo a & done",
                (2, "relay-sh: unsupported syntax: &\n"),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
        // A value the variables cannot hold fails the loop; the line goes on.
        let line = alloc::format!(
            "A={}; for x in $A; do echo no; done; echo next",
            "a".repeat(40_000)
        );
        assert_eq!(
            h.run(&line),
            (
                0,
                "relay-sh: x: the variables would hold more than 64 KiB\nnext\n".into()
            )
        );
        // Ctrl-C ends it, before the first `echo` here.
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("for x in a b c; do echo $x; done"),
            (130, "^C\n".into())
        );
    }

    #[test]
    fn a_for_without_words_loops_over_the_script_s_arguments() {
        // f10: `for NAME; do`, `for NAME do` and `in "$@"` alike.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"for x; do echo [$x]; done\nfor y do echo [$y]; done\nfor z in \"$@\"; do echo [$z]; done\n",
        );
        let (status, out) = h.run("sh /tmp/s.sh p 'q r' ''");
        let printed: Vec<&str> = out.lines().filter(|l| !l.starts_with('+')).collect();
        assert_eq!(status, 0);
        assert_eq!(printed, ["[p]", "[q r]", "[]"].repeat(3));
        // With none, no pass.
        assert_eq!(h.run("for x; do echo no; done"), (0, "".into()));
    }

    #[test]
    fn a_loop_that_starts_jobs_collects_those_that_ended() {
        // The final review: jobs started in a loop were collected only at
        // the next prompt, so after about 62 passes the process table was
        // full and every program failed. Each start collects first.
        let mut h = spawning();
        h.spawning("for x in 1 2 3 4 5 6; do t-args a & t-args b; done");
        assert_eq!(h.programs.spawned.len(), 12);
        assert!(
            h.programs.alive_at_spawn.iter().all(|&n| n <= 1),
            "{:?}",
            h.programs.alive_at_spawn
        );
    }

    #[test]
    fn an_if_32_levels_deep_runs() {
        let mut h = Harness::new();
        let line = alloc::format!(
            "{}echo deep{}",
            "if true; then ".repeat(32),
            "; fi".repeat(32)
        );
        assert_eq!(h.run(&line), (0, "deep\n".into()));
    }

    #[test]
    fn exit_ctrl_c_and_a_bad_substitution_end_an_if_and_its_line() {
        let mut h = Harness::new();
        // r7: `exit` deep inside stops the shell at once.
        assert_eq!(
            h.run("if true; then if true; then exit 3; fi; echo no; fi; echo no"),
            (3, "".into())
        );
        // A bad substitution abandons the whole line (§5.1).
        assert_eq!(
            h.run("if true; then echo ${1A}; echo no; fi; echo no"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("if echo ${1A}; then echo no; else echo no; fi"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        // Ctrl-C ends it, in its condition or its body.
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        for (line, ran) in [
            (
                "if t-spin; then t-args no; else t-args no; fi; t-args no",
                &["t-spin"][..],
            ),
            (
                "if t-args a; then t-spin; t-args no; fi; t-args no",
                &["t-args a", "t-spin"],
            ),
        ] {
            let before = h.programs.spawned.len();
            assert_eq!(h.spawning(line).1, "^C\n", "{line}");
            let args: Vec<String> = h.programs.spawned[before..]
                .iter()
                .map(|s| s.args.join(" "))
                .collect();
            assert_eq!(args, ran, "{line}");
        }
        // The in-process runner refuses a job at any depth, before any of
        // the line runs.
        for line in [
            "echo a; if true; then echo b & fi",
            "echo a; if echo b & then echo c; fi",
            "echo a; if false; then echo b; else echo c & fi",
        ] {
            assert_eq!(
                h.run(line),
                (2, "relay-sh: unsupported syntax: &\n".into()),
                "{line}"
            );
        }
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
        assert_eq!(h.programs.spawned[1].fds[1], 4, "the redirection");
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
        // Each start collects what ended: `sleep` runs on through more
        // rounds here, so the first job is still running at the second
        // prompt.
        h.programs.lives.retain(|&(p, _)| p != "/bin/sleep");
        h.programs.lives.push(("/bin/sleep", 3));
        let out = typed(
            &mut h,
            &["sleep 5 & t-args a", "sleep 6 & sleep 7 &", "", ""],
        );
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
    fn a_script_s_cd_stays_in_it() {
        // Under /bin/sh a script is a process of its own; the in-process
        // runner goes back where it was (programmable shell gate §9.2).
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cd /etc\npwd\n");
        assert_eq!(
            h.lines(&["cd /tmp", "sh s.sh", "pwd", "echo $PWD"]).1,
            "+ cd /etc\n+ pwd\n/etc\n/tmp\n/tmp\n"
        );
    }

    #[test]
    fn a_script_starts_with_the_exported_variables() {
        // As bash's: a script imports them, and what it sets or exports
        // stays in it.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.put(
            "/tmp/s.sh",
            b"echo [$A][$B][$HOME]\nexport C=in B=changed\n",
        );
        assert_eq!(
            h.lines(&["A=1", "export B=2", "sh /tmp/s.sh", "echo [$B][$C]"]),
            (
                0,
                "+ echo [$A][$B][$HOME]\n[][2][/root]\n+ export C=in B=changed\n[2][]\n".into()
            )
        );
    }

    #[test]
    fn a_program_gets_exactly_the_exported_variables() {
        // In the order of export, the imported ones first; one without a
        // value is none (programmable shell gate §8.5).
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        {
            let mut shell =
                Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                    .with_environment(b"HOME=/root\0X=1\0");
            for line in [
                "A=1",
                "export B=2 NONE",
                "t-args",
                "unset X",
                "t-args | cat",
                "t-args &",
            ] {
                shell.execute(line);
            }
        }
        let envs: Vec<_> = h.programs.spawned.iter().map(|s| s.env.clone()).collect();
        assert_eq!(
            envs,
            [
                b"HOME=/root\0X=1\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
            ],
            "a lone command, each command of a pipeline, a background job"
        );
        // With nothing imported, `PWD` alone.
        h.spawning("t-args");
        assert_eq!(h.programs.spawned.last().unwrap().env, b"PWD=/\0");
    }

    #[test]
    fn assignments_before_a_program_are_in_its_environment_only() {
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        {
            let mut shell =
                Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                    .with_environment(b"HOME=/root\0B=2\0");
            for line in [
                "B=3 A=1 t-args",
                "t-args",
                "A=1 t-args | X=2 cat",
                "Y=$B t-args &",
                "B=4 t-args $B",
            ] {
                shell.execute(line);
            }
        }
        let envs: Vec<_> = h.programs.spawned.iter().map(|s| s.env.clone()).collect();
        assert_eq!(
            envs,
            [
                b"HOME=/root\0B=3\0PWD=/\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0X=2\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0Y=2\0".to_vec(),
                b"HOME=/root\0B=4\0PWD=/\0".to_vec(),
            ]
        );
        assert_eq!(
            h.programs.spawned[5].args,
            ["t-args", "2"],
            "the words read the old B"
        );
    }

    #[test]
    fn assignments_before_a_built_in_hold_while_it_runs() {
        // What bash 5.2 prints for each (programmable shell gate §15 item
        // 7): a name comes back as it was unless the built-in set or
        // exported it, and `export -p` lists the values from before.
        let mut h = Harness::new();
        let (status, out) = h.lines(&[
            "A=1 export A",
            "C=0",
            "C=1 unset C",
            "B=1 unset B",
            "W=1 export W=2",
            "G=1 cd /tmp > /tmp/o",
            "export Z=0",
            "X=1 Z=1 export",
            "C=1 C=2 cd /",
            "echo [$A][$B][$C][$W][$G][$Z][$X]",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "declare -x A=\"1\"\ndeclare -x OLDPWD=\"/\"\ndeclare -x PWD=\"/tmp\"\ndeclare -x W=\"2\"\n\
             declare -x Z=\"0\"\n[1][][0][2][][0][]\n"
        );
    }

    #[test]
    fn an_assignment_a_built_in_cannot_hold_runs_nothing() {
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 30);
        let (status, out) = h.lines(&[
            &format!("A={big}"),
            &format!("B={} C=1 cd /tmp", &big[..20]),
            "pwd",
            "echo [$C]",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "relay-sh: B: the variables would hold more than 64 KiB\n/\n[]\n"
        );
    }

    #[test]
    fn a_program_s_environment_holds_at_most_64_kib() {
        // Past it bash's words for `E2BIG`, status 126, under both
        // runners; a built-in gets no environment. Exported, `X=…` and
        // `PWD=/` take 40,009 bytes; `Y=…` before a command fills the
        // rest of 64 KiB, or one byte more.
        let mut env = b"X=".to_vec();
        env.extend(core::iter::repeat_n(b'x', 40_000));
        env.push(0);
        let rest = crate::vars::ENVIRONMENT_MAX - 40_003 - 6 - 3;
        let fits = format!("Y={}", "y".repeat(rest));
        let over = format!("Y={}", "y".repeat(rest + 1));
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.env = env;
        let too_long = |name: &str| format!("relay-sh: {name}: Argument list too long\n");
        assert_eq!(
            h.spawning(&format!("{fits} t-args")),
            (3, "".into()),
            "64 KiB exactly"
        );
        assert_eq!(
            h.programs.spawned[0].env.len(),
            crate::vars::ENVIRONMENT_MAX
        );
        assert_eq!(
            h.spawning(&format!("{over} t-args")),
            (126, too_long("t-args"))
        );
        assert_eq!(
            h.spawning(&format!("{over} t-args | cat")),
            (0, too_long("t-args"))
        );
        let paths: Vec<_> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["/bin/t-args", "/bin/cat"]);
        assert_eq!(h.run(&format!("{over} echo hi")), (126, too_long("echo")));
        assert_eq!(
            h.run(&format!("echo hi | {over} cat")),
            (126, too_long("cat"))
        );
        assert_eq!(
            h.run(&format!("{over} echo hi | cat")),
            (0, too_long("echo"))
        );
        assert_eq!(h.run(&format!("{fits} echo hi")), (0, "hi\n".into()));
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
    }

    #[test]
    fn a_built_in_gets_no_environment_so_no_limit() {
        // 11,000 empty variables hold under 64 KiB of names, but their
        // entries, `V123=` and a NUL each, make more than 64 KiB.
        let mut env = Vec::new();
        for i in 0..11_000 {
            env.extend_from_slice(format!("V{i}=\0").as_bytes());
        }
        let mut h = spawning();
        h.env = env;
        let too_long = "relay-sh: t-args: Argument list too long\n";
        assert_eq!(h.spawning("t-args"), (126, too_long.into()));
        assert_eq!(h.spawning("cd /tmp"), (0, "".into()));
    }

    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo [$A][$B]\n");
        assert_eq!(
            h.lines(&["B=2", "A=1 sh /tmp/s.sh", "echo [$A]"]),
            (0, "+ echo [$A][$B]\n[1][]\n[]\n".into())
        );
    }

    #[test]
    fn assignments_before_words_that_expand_to_nothing_stay_set() {
        // As bash's `A=1 $E`: a line of assignments after all.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["A=1 $E", "B=2 $E > /tmp/f", "echo $A $B"]),
            (0, "1 2\n".into())
        );
        assert!(h.exists("/tmp/f"));
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
    fn a_shell_imports_its_environment_as_exported_variables() {
        // `/bin/sh` imports the block it was started with (programmable
        // shell gate §8.5), its name given after.
        let mut h = Harness::new();
        let (status, out) = {
            let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
                .with_environment(b"HOME=/root\0A=x  y\0")
                .named("/bin/sh");
            (
                shell.execute("echo $0 $HOME [$A]; export"),
                h.console.take(),
            )
        };
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "/bin/sh /root [x  y]\ndeclare -x A=\"x  y\"\ndeclare -x HOME=\"/root\"\n\
             declare -x OLDPWD\ndeclare -x PWD=\"/\"\n"
        );
    }

    #[test]
    fn a_shell_sets_pwd_and_keeps_an_oldpwd_that_names_a_directory() {
        // As bash 5.2 does when it starts (`env -i PWD=… OLDPWD=… bash`).
        let mut h = Harness::new();
        vfs::Vfs::chdir(&mut h.vfs, b"/tmp").unwrap();
        for (env, out) in [
            (&b"PWD=/nowhere\0OLDPWD=/etc\0"[..], "[/tmp][/etc]\n"),
            (b"OLDPWD=/nonexistent\0", "[/tmp][]\n"),
            (b"OLDPWD=/etc/motd\0", "[/tmp][]\n"),
            (b"OLDPWD=\0", "[/tmp][]\n"),
        ] {
            h.env = env.to_vec();
            assert_eq!(h.run("echo \"[$PWD][$OLDPWD]\""), (0, out.into()));
        }
        // A script the in-process runner runs starts so too, where it runs.
        h.env = b"OLDPWD=/etc\0".to_vec();
        h.put("/tmp/s.sh", b"echo \"[$PWD][$OLDPWD]\"\n");
        vfs::Vfs::chdir(&mut h.vfs, b"/").unwrap();
        assert_eq!(
            h.lines(&["PWD=/x", "export OLDPWD=/nonexistent", "sh /tmp/s.sh"])
                .1,
            "+ echo \"[$PWD][$OLDPWD]\"\n[/][]\n"
        );
    }

    #[test]
    fn a_shell_says_once_when_pwd_does_not_fit_its_variables() {
        let mut h = Harness::new();
        let mut env = b"A=".to_vec();
        env.extend(core::iter::repeat_n(b'x', crate::vars::VARS_MAX - 3));
        env.push(0);
        h.env = env;
        assert_eq!(
            h.run("echo \"[$PWD]\""),
            (
                0,
                "relay-sh: PWD: the variables would hold more than 64 KiB\n[]\n".into()
            )
        );
    }

    #[test]
    fn a_script_bin_sh_runs_keeps_the_variables_it_imported() {
        let mut h = spawning();
        h.env = b"HOME=/root\0".to_vec();
        h.put("/tmp/s.sh", b"export\n");
        let mut out = FakeStdout::console();
        let (status, said) = h.sh(&["/tmp/s.sh", "x"], &mut out);
        assert_eq!(status, 0);
        assert_eq!(
            said,
            "+ export\ndeclare -x HOME=\"/root\"\ndeclare -x OLDPWD\ndeclare -x PWD=\"/\"\n"
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
        assert_eq!(s.fds[0], r, "the pipe it reads");
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
    fn several_redirections_are_made_left_to_right() {
        // bash 5.2: each file is made, and the command writes to the last
        // (tmp/m5p1/probes/p1.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"old\n");
        assert_eq!(h.run("echo a > /tmp/f > /tmp/g"), (0, String::new()));
        assert_eq!(
            (h.get("/tmp/f"), h.get("/tmp/g")),
            (b"".to_vec(), b"a\n".to_vec())
        );
        assert_eq!(h.run("echo b >> /tmp/g > /tmp/f"), (0, String::new()));
        assert_eq!(
            (h.get("/tmp/f"), h.get("/tmp/g")),
            (b"b\n".to_vec(), b"a\n".to_vec())
        );
        // One that cannot be made stops the command, after those before.
        assert_eq!(
            h.run("echo c > /tmp/h > /nodir/x > /tmp/i"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert!(h.exists("/tmp/h") && !h.exists("/tmp/i"));
        // Under /bin/sh each file a later one replaces is closed at once,
        // and the program gets the last.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/a > /tmp/b").0, 3);
        assert_eq!(
            h.programs.closed[0], 4,
            "the first, which the second replaced"
        );
        assert_eq!(h.programs.spawned[0].fds, [0, 5, 2]);
        assert_eq!(h.programs.closed, [4, 5]);
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
    fn standard_error_may_go_to_a_file() {
        // bash 5.2 (tmp/m5p1/probes/p1.txt): a command's own messages go to
        // its fd 2, a built-in's and the shell's of it alike.
        let mut h = Harness::new();
        assert_eq!(h.run("cd /missing 2> /tmp/e"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: cd: /missing: No such file or directory\n"
        );
        assert_eq!(h.run("nope 2>> /tmp/e"), (127, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: cd: /missing: No such file or directory\n\
              relay-sh: nope: command not found\n"
        );
        assert_eq!(h.run("ls /tmp/e /nope 2> /tmp/e2"), (2, "/tmp/e\n".into()));
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        // Each error after the one before.
        assert_eq!(h.run("ls /nope /nope2 2> /tmp/e2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n\
              ls: cannot access '/nope2': No such file or directory\n"
        );
        // Output stays where it was; an fd 2 with nothing to say is made.
        assert_eq!(h.run("echo a 2> /tmp/e3"), (0, "a\n".into()));
        assert_eq!(h.get("/tmp/e3"), b"");
        // A redirection that cannot be made is told on fd 2 as it stands
        // then.
        assert_eq!(h.run("echo a 2> /tmp/e4 > /nodir/x"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e4"),
            b"relay-sh: /nodir/x: No such file or directory\n"
        );
        assert_eq!(
            h.run("echo a > /nodir/x 2> /tmp/e5"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert!(!h.exists("/tmp/e5"));
    }

    #[test]
    fn a_file_appended_to_is_written_at_its_end_each_time() {
        // The prototype's review (M-1): two fds that append to one file, as
        // the kernel's append does (bash 5.2, tmp/m5p1/probes/p14.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"x\n");
        h.put("/tmp/g", b"old\n");
        assert_eq!(
            h.run("ls /tmp/f /nope >> /tmp/g 2>> /tmp/g"),
            (2, String::new())
        );
        assert_eq!(
            h.get("/tmp/g"),
            b"old\nls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        // Each write of one fd at the end too.
        h.put("/tmp/g2", b"old\n");
        assert_eq!(h.run("ls /nope /nope2 2>> /tmp/g2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/g2"),
            b"old\nls: cannot access '/nope': No such file or directory\n\
              ls: cannot access '/nope2': No such file or directory\n"
        );
        // Wherever another fd wrote meanwhile.
        assert_eq!(h.run("cd /nope >> /tmp/g 2>> /tmp/g"), (1, String::new()));
        assert!(
            h.get("/tmp/g")
                .ends_with(b"/tmp/f\nrelay-sh: cd: /nope: No such file or directory\n")
        );
    }

    #[test]
    fn one_output_may_be_made_a_copy_of_the_other() {
        // bash 5.2 (tmp/m5p1/probes/p1.txt, p2.txt): the copy is of the
        // fd as it is at that point.
        let mut h = Harness::new();
        h.put("/tmp/f", b"");
        assert_eq!(h.run("ls /tmp/f /nope > /tmp/o 2>&1"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/o"),
            b"ls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        assert_eq!(
            h.run("ls /tmp/f /nope 2>&1 > /tmp/o2"),
            (
                2,
                "ls: cannot access '/nope': No such file or directory\n".into()
            )
        );
        assert_eq!(h.get("/tmp/o2"), b"/tmp/f\n");
        // Output to fd 2.
        assert_eq!(h.run("echo a 2> /tmp/e >&2"), (0, String::new()));
        assert_eq!(h.get("/tmp/e"), b"a\n");
        assert_eq!(h.run("echo a >&2 2> /tmp/e2"), (0, "a\n".into()));
        assert_eq!(h.get("/tmp/e2"), b"");
        // A built-in's errors and output, in order.
        assert_eq!(h.run("cd /nope > /tmp/c 2>&1"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/c"),
            b"relay-sh: cd: /nope: No such file or directory\n"
        );
        // Under /bin/sh both fds are the one file, opened once.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/o 2>&1").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 4, 4]);
        assert_eq!(h.spawning("t-args 2>&1 > /tmp/o").0, 3);
        assert_eq!(h.programs.spawned[1].fds, [0, 5, 1]);
        assert_eq!(h.spawning("t-args 2> /tmp/e 1>&2 2>&1").0, 3);
        assert_eq!(h.programs.spawned[2].fds, [0, 6, 6]);
        assert_eq!(h.programs.closed, [4, 5, 6], "each once, after the program");
        assert_eq!(h.spawning("nope > /tmp/n 2>&1"), (127, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/n"),
            b"relay-sh: nope: command not found\n"
        );
    }

    #[test]
    fn a_copy_of_an_fd_onto_itself_keeps_its_file() {
        // The prototype's review (M-5): a copy holds its file before the
        // slot it replaces lets it go, or `> f 1>&1` would close `f` before
        // the command writes it (bash 5.2: a copy onto itself changes
        // nothing).
        let mut h = Harness::new();
        assert_eq!(h.run("echo a > /tmp/f 1>&1"), (0, String::new()));
        assert_eq!(h.get("/tmp/f"), b"a\n");
        assert_eq!(h.run("ls /nope 2> /tmp/e 2>&2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/o 1>&1").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 4, 2]);
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_pipeline_s_commands_redirect_over_its_pipes() {
        // bash 5.2 (tmp/m5p1/probes/p10.txt): `2>&1` sends the errors into
        // the pipe, and a command's own messages go there too.
        let mut h = Harness::new();
        h.put("/tmp/f", b"x\n");
        assert_eq!(h.run("ls /tmp/f /nope 2>&1 | wc -l"), (0, "2\n".into()));
        assert_eq!(h.run("ls /nope 2> /tmp/e | wc -l"), (0, "0\n".into()));
        assert_eq!(
            h.get("/tmp/e"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        assert_eq!(h.run("nope 2>&1 | wc -l"), (0, "1\n".into()));
        // The last command's own message on its fd 2 (the prototype's
        // review, M-2).
        assert_eq!(h.run("echo a | nope 2> /tmp/n"), (127, String::new()));
        assert_eq!(h.get("/tmp/n"), b"relay-sh: nope: command not found\n");
        // Output sent where the errors go leaves the pipe empty, a file's
        // too.
        assert_eq!(
            h.run("ls /tmp/f /nope 2> /tmp/e2 1>&2 | wc -l"),
            (0, "0\n".into())
        );
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        assert_eq!(
            h.run("ls /tmp/f /nope 1>&2 | wc -l"),
            (
                0,
                "ls: cannot access '/nope': No such file or directory\n/tmp/f\n0\n".into()
            )
        );
        assert_eq!(
            h.run("cat /tmp/f | ls /nope 2>&1"),
            (
                2,
                "ls: cannot access '/nope': No such file or directory\n".into()
            )
        );
        // A command whose redirection fails runs nothing and leaves its
        // neighbours an end; the pipeline fails only when it is the last.
        assert_eq!(
            h.run("cat < /nope | wc -l"),
            (0, "relay-sh: /nope: No such file or directory\n0\n".into())
        );
        assert_eq!(
            h.run("echo a 2> /nodir/y | cat"),
            (0, "relay-sh: /nodir/y: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("echo a | cat > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        // After `2>&1` the message goes into the pipe, as bash's does
        // (probe p1; plan 1's final review, M-1); a failure before `2>&1`
        // is told where fd 2 was then.
        assert_eq!(h.run("cat 2>&1 < /nope | wc -l"), (0, "1\n".into()));
        assert_eq!(
            h.run("echo a | cat 2>&1 2> /nodir/e | cat"),
            (0, "relay-sh: /nodir/e: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("cat < /nope 2>&1 | wc -l"),
            (0, "relay-sh: /nope: No such file or directory\n0\n".into())
        );
        // One longer than a pipe takes at once stays on the screen.
        let long = alloc::format!("/{}", "x".repeat(PIPED_MESSAGE_MAX));
        assert_eq!(
            h.run(&alloc::format!("cat 2>&1 < {long} | wc -l")),
            (
                0,
                alloc::format!("relay-sh: {long}: File name too long\n0\n")
            )
        );
        // Under /bin/sh: each command gets the pipes and its files.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args 2>&1 | t-args 2> /tmp/e").0, 3);
        assert_eq!(h.programs.pipes, [(5, 6)]);
        assert_eq!(h.programs.spawned[0].fds, [0, 6, 6]);
        assert_eq!(h.programs.spawned[1].fds, [5, 1, 4]);
        assert_eq!(
            h.spawning("nope 2>&1 | t-args"),
            (3, String::new()),
            "the message goes into the pipe"
        );
        assert_eq!(
            h.programs.written,
            [(8, b"relay-sh: nope: command not found\n".to_vec())]
        );
        h.programs.open_error = Some(vfs::Errno::ENOENT);
        assert_eq!(
            h.spawning("t-args | t-args > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 4, "the first ran");
        // A failed redirection after `2>&1`: its message into the pipe.
        h.programs.written.clear();
        assert_eq!(
            h.spawning("t-args 2>&1 < /nope | t-args"),
            (3, String::new())
        );
        let (_, write) = *h.programs.pipes.last().unwrap();
        assert_eq!(
            h.programs.written,
            [(
                write,
                b"relay-sh: /nope: No such file or directory\n".to_vec()
            )]
        );
        assert!(h.programs.closed.contains(&write));
        let long = alloc::format!("/{}", "x".repeat(PIPED_MESSAGE_MAX));
        assert_eq!(
            h.spawning(&alloc::format!("t-args 2>&1 < {long} | t-args")),
            (
                3,
                alloc::format!("relay-sh: {long}: No such file or directory\n")
            )
        );
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
        // So is a stage that cannot start: its message into a pipe whose
        // reader has not started would block the shell for good (the
        // prototype's review, I-1).
        let name = "x".repeat(PIPED_MESSAGE_MAX);
        assert_eq!(
            h.spawning(&alloc::format!("{name} 2>&1 | t-args")),
            (3, alloc::format!("relay-sh: {name}: command not found\n"))
        );
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
        // In the in-process runner too.
        let mut h = Harness::new();
        assert_eq!(
            h.run(&alloc::format!("{name} 2>&1 | wc -c")),
            (
                0,
                alloc::format!("relay-sh: {name}: command not found\n0\n")
            )
        );
        assert_eq!(h.run("nope 2>&1 | wc -c"), (0, "34\n".into()));
    }

    #[test]
    fn a_redirection_s_target_is_expanded_when_it_is_reached() {
        // bash 5.2 (tmp/m5p1/probes/p11.txt, p12.txt): a target that does
        // not expand is told on fd 2 as it stands, and the redirections
        // after it are not made.
        let mut h = Harness::new();
        assert_eq!(h.run("E="), (0, String::new()));
        assert_eq!(h.run("echo a 2> /tmp/e1 > $E"), (1, String::new()));
        assert_eq!(h.get("/tmp/e1"), b"relay-sh: $E: ambiguous redirect\n");
        assert_eq!(
            h.run("echo a > $E 2> /tmp/e2"),
            (1, "relay-sh: $E: ambiguous redirect\n".into())
        );
        assert!(!h.exists("/tmp/e2"));
        // A bad substitution abandons the line too.
        assert_eq!(
            h.run("echo a 2> /tmp/e3 > ${1A}; echo after"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/e3"), b"relay-sh: ${1A}: bad substitution\n");
        // The words come first: one that does not expand makes no file.
        assert_eq!(
            h.run("echo ${1A} 2> /tmp/e4"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert!(!h.exists("/tmp/e4"));
    }

    #[test]
    fn a_compound_command_s_redirections_hold_for_everything_inside() {
        // bash 5.2 (tmp/m5p1/probes/p3.txt, p4.txt, p11.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"l1\nl2\nl3\n");
        assert_eq!(
            h.run("for x in a b; do echo $x; cd /nope; done > /tmp/fo 2> /tmp/fe"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/fo"), b"a\nb\n");
        assert_eq!(
            h.get("/tmp/fe"),
            b"relay-sh: cd: /nope: No such file or directory\n\
              relay-sh: cd: /nope: No such file or directory\n"
        );
        // One file, whose offset every command inside shares.
        assert_eq!(
            h.run("for x in 1 2; do echo s$x; cd /nope 2>&1; echo e$x; done > /tmp/mix"),
            (0, String::new())
        );
        assert_eq!(
            h.get("/tmp/mix"),
            b"s1\nrelay-sh: cd: /nope: No such file or directory\ne1\n\
              s2\nrelay-sh: cd: /nope: No such file or directory\ne2\n"
        );
        assert_eq!(
            h.run("if cat; then echo t; fi < /tmp/f"),
            (0, "l1\nl2\nl3\nt\n".into())
        );
        // Each `head` reads on where the one before stopped.
        assert_eq!(
            h.run("for x in 1 2; do head -n 1; done < /tmp/f"),
            (0, "l1\nl2\n".into())
        );
        // A pipeline inside reads on from where the one before stopped.
        assert_eq!(
            h.run("for x in 1 2; do head -n 1 | cat; done < /tmp/f"),
            (0, "l1\nl2\n".into())
        );
        // Messages follow one another in the file.
        assert_eq!(
            h.run("for x in 1 2; do nope; nope | cat; done 2> /tmp/n"),
            (0, String::new())
        );
        assert_eq!(
            h.get("/tmp/n"),
            b"relay-sh: nope: command not found\n".repeat(4)
        );
        // Ctrl-C's `^C` is the key's echo, on the screen.
        h.console.interrupt_after = Some(3);
        assert_eq!(
            h.run("while true; do true; done 2> /tmp/c"),
            (130, "^C\n".into())
        );
        assert_eq!(h.get("/tmp/c"), b"");
        // Two fds that append to one file, each at its end (the prototype's
        // review, M-1).
        assert_eq!(
            h.run("for x in 1 2; do echo out$x; cd /nope; done >> /tmp/ap 2>> /tmp/ap"),
            (1, String::new())
        );
        assert_eq!(
            h.get("/tmp/ap"),
            b"out1\nrelay-sh: cd: /nope: No such file or directory\n\
              out2\nrelay-sh: cd: /nope: No such file or directory\n"
        );
        // They end with the construct.
        assert_eq!(
            h.run("for x in 1; do echo in; done > /tmp/o; echo after"),
            (0, "after\n".into())
        );
        // A redirection inside goes over them.
        assert_eq!(
            h.run("for x in 1; do echo in > /tmp/in; echo out; done > /tmp/o"),
            (0, String::new())
        );
        assert_eq!(
            (h.get("/tmp/in"), h.get("/tmp/o")),
            (b"in\n".to_vec(), b"out\n".to_vec())
        );
        // The construct's own errors and the shell's go to its fd 2.
        assert_eq!(
            h.run("for 1x in a; do echo; done 2> /tmp/ie"),
            (1, String::new())
        );
        assert_eq!(
            h.get("/tmp/ie"),
            b"relay-sh: `1x': not a valid identifier\n"
        );
        assert_eq!(
            h.run("for x in 1; do echo ${1A}; done 2> /tmp/be; echo after"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/be"), b"relay-sh: ${1A}: bad substitution\n");
        // One that cannot be made runs nothing of it.
        assert_eq!(
            h.run("for x in 1; do echo a; done > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("while true; do echo a; done < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("E="), (0, String::new()));
        assert_eq!(
            h.run("for x in 1; do echo a; done 2> /tmp/ae > $E"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/ae"), b"relay-sh: $E: ambiguous redirect\n");
        // Under /bin/sh the file is opened once, for every program and
        // built-in inside, and closed after the last.
        let mut h = spawning();
        assert_eq!(
            h.spawning("for x in 1 2; do t-args; help; done > /tmp/o"),
            (0, String::new())
        );
        assert_eq!(h.programs.opened, [("/tmp/o".into(), false, 4)]);
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.fds).collect();
        assert_eq!(fds, [[0, 4, 2], [0, 4, 2]]);
        assert!(
            h.programs
                .written_to("/tmp/o")
                .starts_with(b"Programs in /bin:")
        );
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_job_s_number_goes_to_the_fd_2_around_it() {
        // bash 5.2 (tmp/m5p1/probes/p4.txt): `[1] 42` into the file.
        let mut h = with_jobs();
        let out = typed(&mut h, &["for x in 1; do t-spin & done 2> /tmp/je"]);
        assert!(!out.contains("[1]"), "{out}");
        assert_eq!(h.programs.written_to("/tmp/je"), b"[1] 101\n");
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
        let mut h = spawning();
        assert_eq!(h.spawning("t-args 2> /tmp/e").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 1, 4]);
        assert_eq!(h.programs.closed, [4]);
        // Its own messages, and a built-in's errors, go through the fd.
        assert_eq!(h.spawning("nope 2> /tmp/e"), (127, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/e"),
            b"relay-sh: nope: command not found\n"
        );
        assert_eq!(h.spawning("cd /missing 2>> /tmp/f"), (1, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/f"),
            b"relay-sh: cd: /missing: No such file or directory\n"
        );
        // A built-in's write error is its own message too.
        h.programs.write_error = Some((7, Errno::ENOSPC));
        assert_eq!(h.spawning("help > /tmp/h 2> /tmp/g"), (1, String::new()));
        assert_eq!(h.programs.opened[3], ("/tmp/h".into(), false, 7));
        assert_eq!(
            h.programs.written_to("/tmp/g"),
            b"help: write error: No space left on device\n"
        );
    }

    #[test]
    fn standard_input_may_be_a_file() {
        // bash 5.2 (tmp/m5p1/probes/p9.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"l1\nl2\n");
        assert_eq!(h.run("cat < /tmp/f"), (0, "l1\nl2\n".into()));
        assert_eq!(h.run("wc -l 0</tmp/f"), (0, "2\n".into()));
        assert_eq!(h.run("cat < /tmp/f | wc -l"), (0, "2\n".into()));
        // Read whole, a piece after the other.
        h.put("/tmp/big", &alloc::vec![b'x'; 10_000]);
        assert_eq!(h.run("wc -c < /tmp/big"), (0, "10000\n".into()));
        assert_eq!(
            h.run("cat < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("cat 2> /tmp/e < /nope"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: /nope: No such file or directory\n"
        );
        assert_eq!(h.run("cat < /tmp"), (1, "cat: -: Is a directory\n".into()));
        // Alone it opens the file and runs nothing.
        assert_eq!(h.run("< /tmp/f"), (0, String::new()));
        assert_eq!(
            h.run("< /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        // Under /bin/sh the program gets the file as fd 0, a pipeline's
        // first command too.
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        assert_eq!(h.spawning("cat < /tmp/f").0, 0);
        assert_eq!(h.programs.inputs, [("/tmp/f".into(), 4)]);
        assert_eq!(h.programs.spawned[0].fds, [4, 1, 2]);
        assert_eq!(h.spawning("cat < /tmp/g | t-args").0, 3);
        assert_eq!(h.programs.pipes, [(6, 7)]);
        assert_eq!(h.programs.spawned[1].fds, [5, 7, 2]);
        assert_eq!(h.programs.spawned[2].fds, [6, 1, 2]);
        // The pipe's ends as each stage has them, the files after the
        // pipeline.
        assert_eq!(h.programs.closed, [4, 7, 6, 5]);
        h.programs.open_error = Some(vfs::Errno::ENOENT);
        assert_eq!(
            h.spawning("cat < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 3);
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
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system).with_environment(b"");
        shell.greet();
        // `PWD` follows it there.
        shell.execute("echo $PWD");
        assert_eq!(h.console.take(), "Welcome to Relay OS.\n/root\n");
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
    fn a_command_typed_across_lines_shows_the_continuation_prompt() {
        // bash's `> `, for each line the command needs.
        let mut h = Harness::new();
        h.console.type_in(b"echo a &&\recho b |\r\rwc -c\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(
            h.console.text(),
            "root@relay:/# echo a &&\n> echo b |\n> \n> wc -c\na\n2\nroot@relay:/# "
        );
    }

    #[test]
    fn ctrl_c_at_the_continuation_prompt_drops_the_command() {
        let mut h = Harness::new();
        h.console.type_in(b"echo a &&\recho no\x03echo $?\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(
            h.console.text(),
            "root@relay:/# echo a &&\n> echo no^C\nroot@relay:/# echo $?\n130\nroot@relay:/# "
        );
    }

    #[test]
    fn the_end_of_input_at_the_continuation_prompt_is_an_error() {
        // As bash's, which then ends.
        let mut h = Harness::new();
        h.console.type_in(b"echo a ||\r");
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 2);
        assert_eq!(
            h.console.text(),
            "root@relay:/# echo a ||\n> relay-sh: syntax error: unexpected end of file\n"
        );
    }

    #[test]
    fn a_job_is_reported_only_at_a_full_prompt() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 &", "t-args a &&", "t-args b"]);
        assert!(
            out.ends_with(
                "# t-args a &&\n> t-args b\n[1]+  Done                    sleep 5\nroot@relay:/# "
            ),
            "{out}"
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
