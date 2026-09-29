//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command or a program and syncing the filesystems after it
//! (spec §7.3, §8.3; user-space gate §8.2).

use crate::commands::{self, Script};
use crate::ctx::Ctx;
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, System};
use crate::parser::{self, HOME, Redirect};
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs, path};

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
/// Exit status of a program that was killed (bash's for SIGKILL).
pub const KILLED: i32 = 137;
/// The most of `/etc/motd` shown at start.
const MOTD_MAX: usize = 16 * 1024;

pub struct Shell<'a> {
    vfs: &'a mut dyn Vfs,
    console: &'a mut dyn Console,
    system: &'a mut dyn System,
    editor: LineEditor,
    status: i32,
    stopped: bool,
    /// A script's lines are running (`sh`).
    in_script: bool,
    /// Where a running script's screen output is copied.
    transcript: Option<Transcript>,
}

impl<'a> Shell<'a> {
    pub fn new(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
    ) -> Shell<'a> {
        Shell {
            vfs,
            console,
            system,
            editor: LineEditor::new(),
            status: 0,
            stopped: false,
            in_script: false,
            transcript: None,
        }
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

    /// Shows `/etc/motd`, goes to `/root`, then reads and runs commands
    /// until the input ends or `reboot`/`poweroff` return.
    pub fn run(&mut self) {
        self.greet();
        while !self.stopped {
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
                        self.execute(&line);
                        break;
                    }
                }
            }
        }
    }

    fn greet(&mut self) {
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
        let cmd = match parser::parse(line) {
            Ok(cmd) => cmd,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            return self.status;
        }
        let file = match &cmd.redirect {
            Some(r) => match self.open_redirect(r) {
                Ok(file) => Some(file),
                Err(e) => return self.finish(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
            None => None,
        };
        let Some(name) = cmd.words.first() else {
            // A bare `> file` just creates or empties the file.
            return self.finish(0, String::new());
        };
        let Some(builtin) = commands::find(name) else {
            return self.run_program(name, &cmd.words[1..], file);
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.in_script = self.in_script;
        ctx.transcript = self.transcript.take();
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
        let mut message = String::new();
        if let Err(e) = ctx.finish() {
            message = format!("{name}: write error: {e}\n");
            status = 1;
        }
        if ctx.cancelled {
            message = String::from("^C\n");
            status = CANCELLED;
        }
        self.transcript = ctx.transcript.take();
        self.stopped = ctx.exit;
        if let Some(script) = ctx.script.take() {
            status = self.run_script(script);
        }
        self.finish(status, message)
    }

    /// Runs a program (user-space gate §8.2): `/bin/<name>`, or `name`
    /// itself when it holds a `/`, with the words after it as arguments.
    /// Its fd 1 is standard output (the redirection file, if any), its fd 2
    /// the screen; a running script's transcript gets both.
    fn run_program(&mut self, name: &str, words: &[String], file: Option<(Node, u64)>) -> i32 {
        let path = if name.contains('/') {
            String::from(name)
        } else {
            format!("/bin/{name}")
        };
        let mut args: Vec<&[u8]> = alloc::vec![path.as_bytes()];
        args.extend(words.iter().map(|w| w.as_bytes()));
        let started = self.system.spawn(&mut *self.vfs, path.as_bytes(), &args);
        let pid = match started {
            Some(Ok(pid)) => pid,
            // No programs here (the host), or none by that name in /bin:
            // `..`, `.` and `''` name directories there, which a search
            // for a command skips, as bash's does.
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            Some(Err(Errno::ENOENT | Errno::EISDIR)) if !name.contains('/') => {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
            }
            Some(Err(e)) => {
                let status = if e == Errno::ENOENT {
                    NOT_FOUND
                } else {
                    CANNOT_RUN
                };
                return self.finish(status, format!("{NAME}: {name}: {e}\n"));
            }
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.transcript = self.transcript.take();
        let ended = ctx.wait_program(pid);
        let mut message = String::new();
        let mut status = match ended {
            Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
            Ok(_) => {
                message = format!("{NAME}: {name}: killed\n");
                KILLED
            }
            Err(e) => {
                message = format!("{NAME}: {name}: {e}\n");
                CANNOT_RUN
            }
        };
        if let Err(e) = ctx.finish() {
            message = format!("{name}: write error: {e}\n");
            status = 1;
        }
        self.transcript = ctx.transcript.take();
        self.finish(status, message)
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

    fn end_transcript(&mut self, e: Errno) {
        if let Some(t) = self.transcript.take() {
            self.console.write(t.ended(e).as_bytes());
        }
    }

    /// Runs a script's lines (spec §15 item 12): each command is shown as
    /// `+ <line>`, then runs and is synced as if typed. Blank and comment
    /// lines are skipped. Ctrl-C, or `reboot`/`poweroff` returning, ends
    /// the script; failing commands do not. Returns the last status.
    fn run_script(&mut self, script: Script) -> i32 {
        self.in_script = true;
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut status = 0;
        for line in script.text.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
                continue;
            }
            if self.console.interrupted() {
                self.say(b"^C\n");
                status = CANCELLED;
                break;
            }
            // The line runs as written; only its trace is trimmed.
            self.say(format!("+ {}\n", line.trim()).as_bytes());
            // On the disk before the command runs: a command that hangs
            // leaves at least its name.
            self.write_transcript();
            self.sync();
            status = self.execute(line);
            if status == CANCELLED || self.stopped {
                break;
            }
        }
        self.write_transcript();
        self.transcript = None;
        self.in_script = false;
        status
    }

    /// Opens a redirection target: created if missing, emptied for `>`,
    /// written at its end for `>>`.
    fn open_redirect(&mut self, r: &Redirect) -> Result<(Node, u64), Errno> {
        let path = r.path.as_bytes();
        let node = match self.vfs.lookup(path) {
            Ok(node) => {
                if self.vfs.stat(node)?.kind == FileType::Directory {
                    return Err(Errno::EISDIR);
                }
                if !r.append {
                    self.vfs.truncate(node, 0)?;
                }
                node
            }
            Err(Errno::ENOENT) => self.vfs.create(path)?,
            Err(e) => return Err(e),
        };
        let offset = if r.append {
            self.vfs.stat(node)?.size
        } else {
            0
        };
        Ok((node, offset))
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

#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::{FakeProgram, Harness};
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
    fn unknown_commands_and_syntax_errors() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("frobnicate x"),
            (127, "relay-sh: frobnicate: command not found\n".into())
        );
        assert_eq!(
            h.run("ls | wc"),
            (2, "relay-sh: unsupported syntax: |\n".into())
        );
        assert_eq!(
            h.run("echo 'open"),
            (2, "relay-sh: syntax error: unterminated quote\n".into())
        );
    }

    /// `t-args` in `/bin`, printing its arguments on fd 1 and a line on
    /// fd 2, and exiting with 3.
    fn with_programs() -> Harness {
        let mut h = Harness::new();
        h.dir("/bin");
        h.put("/bin/t-args", b"\x7fELF");
        h.put("/root/text", b"not a program");
        h.system.programs.push(FakeProgram {
            path: "/bin/t-args",
            writes: vec![(1, b"[1] a\n"), (2, b"t-args: note\n"), (1, b"[2] b c\n")],
            status: WaitStatus::exited(3),
        });
        h
    }

    #[test]
    fn a_name_that_is_no_built_in_runs_from_bin() {
        let mut h = with_programs();
        assert_eq!(
            h.run("t-args a 'b c' ''"),
            (3, "[1] a\nt-args: note\n[2] b c\n".into())
        );
        assert_eq!(
            h.system.spawned,
            [vec![
                b"/bin/t-args".to_vec(),
                b"a".to_vec(),
                b"b c".to_vec(),
                b"".to_vec()
            ]]
        );
        // A path runs as given; argument 0 is the path as typed.
        assert_eq!(h.run("/bin/t-args").0, 3);
        h.run("cd /bin");
        assert_eq!(h.run("./t-args x").0, 3);
        assert_eq!(h.system.spawned[2], [b"./t-args".to_vec(), b"x".to_vec()]);
        // Built-ins come first.
        h.put("/bin/echo", b"\x7fELF");
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
        assert_eq!(h.system.spawned.len(), 3);
    }

    #[test]
    fn a_program_s_output_follows_the_redirection_and_its_errors_the_screen() {
        let mut h = with_programs();
        assert_eq!(h.run("t-args > /tmp/out"), (3, "t-args: note\n".into()));
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        h.run("t-args >> /tmp/out");
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n[1] a\n[2] b c\n");
        // A full disk is a write error, as for a built-in.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                1,
                "t-args: note\nt-args: write error: No space left on device\n".into()
            )
        );
    }

    #[test]
    fn what_cannot_run_says_why() {
        let mut h = with_programs();
        assert_eq!(
            h.run("nosuch x"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(
            h.run("/root/nosuch"),
            (
                127,
                "relay-sh: /root/nosuch: No such file or directory\n".into()
            )
        );
        assert_eq!(
            h.run("/root/text"),
            (126, "relay-sh: /root/text: Exec format error\n".into())
        );
        assert_eq!(
            h.run("/root"),
            (126, "relay-sh: /root: Is a directory\n".into())
        );
        // In a script too; the script goes on.
        h.put("/root/s.sh", b"nosuch\nt-args\n");
        let (status, out) = h.run("sh /root/s.sh");
        assert_eq!(status, 3);
        assert!(
            out.contains("+ nosuch\nrelay-sh: nosuch: command not found\n+ t-args\n[1] a\n"),
            "{out}"
        );
        let transcript = String::from_utf8(h.get("/root/s.log")).unwrap();
        assert!(
            transcript.contains("+ t-args\n[1] a\nt-args: note\n[2] b c\n"),
            "{transcript}"
        );
    }

    #[test]
    fn names_of_directories_in_bin_are_not_commands() {
        let mut h = with_programs();
        for name in ["..", ".", "''"] {
            let shown = if name == "''" { "" } else { name };
            assert_eq!(
                h.run(name),
                (127, format!("relay-sh: {shown}: command not found\n")),
                "{name}"
            );
        }
        // Given as a path, a directory still says so.
        assert_eq!(h.run("./"), (126, "relay-sh: ./: Is a directory\n".into()));
    }

    #[test]
    fn without_programs_every_unknown_name_is_not_found() {
        let mut h = with_programs();
        h.system.no_programs = true;
        assert_eq!(
            h.run("t-args"),
            (127, "relay-sh: t-args: command not found\n".into())
        );
        assert_eq!(
            h.run("/bin/t-args"),
            (127, "relay-sh: /bin/t-args: command not found\n".into())
        );
    }

    #[test]
    fn a_killed_program_is_reported() {
        let mut h = with_programs();
        h.system.programs[0].status = WaitStatus {
            how: relay_abi::wait::KILLED,
            ..Default::default()
        };
        assert_eq!(
            h.run("t-args"),
            (
                137,
                "[1] a\nt-args: note\n[2] b c\nrelay-sh: t-args: killed\n".into()
            )
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
    fn run_greets_goes_home_and_reads_lines_until_input_ends() {
        let mut h = Harness::new();
        h.console.type_in(b"echo hi\rpwd\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(
            h.console.text(),
            "Welcome to Relay OS.\nroot@relay:~# echo hi\nhi\nroot@relay:~# pwd\n/root\nroot@relay:~# "
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
                .contains("echo no^C\nroot@relay:~# pwd\n/root\n")
        );
    }

    #[test]
    fn without_motd_or_root_the_shell_starts_in_slash() {
        let mut h = Harness::empty();
        h.console.type_in(b"pwd\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(h.console.text(), "root@relay:/# pwd\n/\nroot@relay:/# ");
    }
}
