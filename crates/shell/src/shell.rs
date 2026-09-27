//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command and syncing the filesystems after it (spec §7.3, §8.3).

use crate::commands;
use crate::ctx::Ctx;
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, System};
use crate::parser::{self, HOME, Redirect};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs, path};

/// The name the shell uses in its own messages.
pub const NAME: &str = "relay-sh";
/// Exit status of an unknown command.
pub const NOT_FOUND: i32 = 127;
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
    editor: LineEditor,
    status: i32,
    stopped: bool,
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
            return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
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
        self.stopped = ctx.exit;
        self.finish(status, message)
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

    /// Prints `message`, syncs, and records `status`.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        self.console.write(message.as_bytes());
        if let Err(e) = self.vfs.sync() {
            self.console
                .write(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
        self.status = status;
        status
    }
}

#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::Harness;
    use alloc::string::String;
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

    #[test]
    fn a_blank_line_keeps_the_last_status() {
        let mut h = Harness::new();
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.execute("nope");
        assert_eq!(shell.execute("   "), 127);
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
