//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command or a program and syncing the filesystems after it
//! (spec §7.3, §8.3; user-space gate §8.2).

use crate::commands::{self, Script};
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, System};
use crate::parser::{self, HOME};
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::Transcript;
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
    /// A script's lines are running (`sh`).
    in_script: bool,
    /// Where a running script's screen output is copied.
    transcript: Option<Transcript>,
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
    /// runner, user-space gate §8.2). Only `cd`, `exit` and `help` run in
    /// it.
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

    /// Reads and runs commands until the input ends, `exit` or
    /// `reboot`/`poweroff` return.
    pub fn run(&mut self) {
        self.stopped = false;
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

    /// Shows `/etc/motd` and goes to `/root`, as the shell does when the
    /// machine starts.
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
        let cmd = match parser::parse(line) {
            Ok(cmd) => cmd,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            return self.status;
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
        };
        let ran = match cmd.words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
                Some(builtin) => {
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file),
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
        let mut status = ran.status;
        if let Some(script) = ran.script {
            status = self.run_script(script);
        }
        self.finish(status, ran.message)
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

    /// Runs a script's lines (spec §15 item 12): each command is shown as
    /// `+ <line>`, then runs and is synced as if typed. Blank and comment
    /// lines are skipped. Ctrl-C, `exit`, or `reboot`/`poweroff`
    /// returning, ends the script; failing commands do not. Returns the
    /// last status.
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
                b"t-args".to_vec(),
                b"a".to_vec(),
                b"b c".to_vec(),
                b"".to_vec()
            ]],
            "argument 0 is the name as typed, as in bash"
        );
        // A path runs as given, and is argument 0 as typed.
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
    fn a_name_too_long_for_a_file_is_not_found() {
        let mut h = with_programs();
        let long = "x".repeat(300);
        assert_eq!(
            h.run(&long),
            (127, format!("relay-sh: {long}: command not found\n")),
            "as bash says"
        );
        // Given as a path, the error is the path's.
        assert_eq!(
            h.run(&format!("/{long}")),
            (126, format!("relay-sh: /{long}: File name too long\n"))
        );
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
    fn a_program_sees_its_redirection_s_write_error() {
        let mut h = with_programs();
        static BIG: [u8; 5000] = [b'x'; 5000];
        h.system.programs[0].writes =
            vec![(1, b"small\n"), (1, &BIG), (1, b"more\n"), (2, b"note\n")];
        h.spy.zero_writes.set(true);
        let (status, out) = h.run("t-args > /tmp/out");
        assert_eq!(
            h.system.answers,
            [Ok(()), Err(Errno::ENOSPC), Err(Errno::ENOSPC), Ok(())],
            "buffered until 4 KiB, then the disk's error, for good; the screen takes fd 2"
        );
        assert_eq!(status, 1);
        assert!(
            out.ends_with("t-args: write error: No space left on device\n"),
            "{out}"
        );
        // To the screen, every write is fine.
        h.system.answers.clear();
        h.run("t-args");
        assert!(h.system.answers.iter().all(Result::is_ok));
    }

    #[test]
    fn a_program_stopped_by_ctrl_c_says_only_so_and_ends_a_script() {
        let mut h = with_programs();
        h.system.programs[0].status = WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C);
        assert_eq!(
            h.run("t-args"),
            (130, "[1] a\nt-args: note\n[2] b c\n^C\n".into())
        );
        h.put("/root/s.sh", b"t-args\necho after\n");
        let (status, out) = h.run("sh /root/s.sh");
        assert_eq!(status, 130);
        assert!(out.ends_with("[2] b c\n^C\n"), "{out}");
        assert!(!out.contains("after"), "the script stops: {out}");
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
        // A fault, redirected: the message goes to the screen.
        h.system.programs[0].status = WaitStatus::fault(
            relay_abi::wait::FAULT_PAGE,
            relay_abi::wait::ACCESS_READ,
            0,
            0x40_1a2c,
        );
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nrelay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
            )
        );
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        // A write error too: both are reported, and the kill's status stays.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nt-args: write error: No space left on device\n\
                 relay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
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
