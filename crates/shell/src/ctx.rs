//! What a command gets to work with: the filesystem, the system, standard
//! input (none, a program's fd 0, or bytes in memory), standard output
//! (the screen, a redirection file, `/bin/sh`'s fd for one, or a program's
//! fd 1) and errors (the screen, or a redirection file); plus the helpers
//! every command shares for options and GNU-style messages.

use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::jobs::Jobs;
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use vfs::{Errno, Node, Vfs};

/// Output to a file is collected up to this size before it is written.
const FILE_BUFFER: usize = 4096;

pub struct Ctx<'a> {
    pub vfs: &'a mut dyn Vfs,
    pub system: &'a mut dyn System,
    console: &'a mut dyn Console,
    out: Output<'a>,
    /// Where errors go (programmable shell gate §7.5).
    err: To,
    /// Standard input; without one, the input ends at once.
    input: Option<&'a mut dyn Stdin>,
    /// The exit status when standard output could not be written (1, as
    /// milestone 1 said; GNU grep's is 2).
    pub(crate) write_error_status: i32,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
    /// not go away: the shell stops.
    pub(crate) exit: bool,
    /// Ctrl-C stopped the command.
    pub(crate) cancelled: bool,
    /// Set by `sh`: the script the shell runs next.
    pub(crate) script: Option<crate::commands::Script>,
    /// The command is a line of a script.
    pub(crate) in_script: bool,
    /// A running script's transcript, which gets what the screen gets.
    pub(crate) transcript: Option<Transcript>,
    /// The last command's exit status, for `exit`.
    pub(crate) status: i32,
    /// Set by `exit`: in a script the shell runs itself, only the script
    /// stops.
    pub(crate) exited: bool,
    /// The shell's jobs, for its own commands `jobs`, `wait` and `kill`.
    pub(crate) control: Option<JobControl<'a>>,
}

/// What the shell's job commands work with: its jobs, and its programs
/// (none in the in-process runner, which starts no job).
pub(crate) struct JobControl<'a> {
    pub jobs: &'a mut Jobs,
    pub programs: Option<&'a mut dyn Programs>,
    /// The shell reads commands at its prompt, so `wait %n` says how the
    /// job ended, as bash's interactive shell does.
    pub report: bool,
}

impl JobControl<'_> {
    /// Collects what has ended of the jobs.
    pub fn collect(&mut self) {
        if let Some(programs) = self.programs.as_deref_mut() {
            self.jobs.collect(programs);
        }
    }
}

/// Where a command the shell runs itself writes (programmable shell gate
/// §7.5): the screen, a file of the in-process runner's at an offset, or
/// an fd of `/bin/sh`'s, which its `Programs` write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum To {
    Console,
    File(Node, u64),
    Fd(u32),
}

enum Output<'a> {
    Console,
    /// An fd of `/bin/sh`'s, written through its `Programs` in pieces of
    /// 4 KiB, so that a built-in shares the file's offset with programs.
    Fd {
        fd: u32,
        buf: Vec<u8>,
        error: Option<Errno>,
    },
    File {
        node: Node,
        offset: u64,
        buf: Vec<u8>,
        /// The first write error; later output is dropped.
        error: Option<Errno>,
    },
    /// A program's standard output (user-space gate §8.1): written at once
    /// when it is the console, so that it keeps its place among the
    /// errors, and in pieces of 4 KiB when it is a file.
    Program {
        stdout: &'a mut dyn Stdout,
        tty: bool,
        buf: Vec<u8>,
        error: Option<Errno>,
    },
}

impl<'a> Ctx<'a> {
    /// Standard output goes `to` and errors to `err` (an fd through
    /// `control`'s programs).
    pub(crate) fn new(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        to: To,
        err: To,
    ) -> Ctx<'a> {
        let out = match to {
            To::Console => Output::Console,
            To::File(node, offset) => Output::File {
                node,
                offset,
                buf: Vec::new(),
                error: None,
            },
            To::Fd(fd) => Output::Fd {
                fd,
                buf: Vec::new(),
                error: None,
            },
        };
        let mut ctx = Ctx::with_output(vfs, system, console, out);
        ctx.err = err;
        ctx
    }

    /// A command run as a program: standard output is `stdout`, errors go
    /// to `console`.
    pub(crate) fn program(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        stdout: &'a mut dyn Stdout,
    ) -> Ctx<'a> {
        let tty = stdout.is_tty();
        let out = Output::Program {
            stdout,
            tty,
            buf: Vec::new(),
            error: None,
        };
        Ctx::with_output(vfs, system, console, out)
    }

    fn with_output(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        out: Output<'a>,
    ) -> Ctx<'a> {
        Ctx {
            vfs,
            system,
            console,
            out,
            err: To::Console,
            input: None,
            write_error_status: 1,
            exit: false,
            cancelled: false,
            script: None,
            in_script: false,
            transcript: None,
            status: 0,
            exited: false,
            control: None,
        }
    }

    /// The exit status a write error on standard output gives, instead of
    /// 1.
    pub fn set_write_error_status(&mut self, status: i32) {
        self.write_error_status = status;
    }

    /// Gives the command standard input.
    pub(crate) fn set_input(&mut self, input: &'a mut dyn Stdin) {
        self.input = Some(input);
    }

    /// Reads standard input into `buf`: how many bytes, 0 at its end. What
    /// waits for standard output is written first, so that what came of
    /// the last read reaches a pipe before the next one waits (a line typed
    /// into `cat | cat` reaches the second `cat` at Enter).
    pub fn read_input(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        self.streams().flush();
        match &mut self.input {
            Some(input) => input.read(buf),
            None => Ok(0),
        }
    }

    /// Where output goes, borrowed apart from the system.
    fn streams(&mut self) -> Streams<'_, 'a> {
        Streams {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            out: &mut self.out,
            transcript: &mut self.transcript,
            programs: self
                .control
                .as_mut()
                .and_then(|c| c.programs.as_deref_mut()),
        }
    }

    /// Standard output. A write error is kept for `finish` (and
    /// `out_failed`); later output is dropped.
    pub fn out(&mut self, bytes: &[u8]) {
        let _ = self.streams().out(bytes);
    }

    /// Errors: to the screen, or where they are redirected. A write that
    /// fails is lost, as there is nowhere left to say so.
    pub fn err(&mut self, bytes: &[u8]) {
        match &mut self.err {
            To::Console => self.streams().screen(bytes),
            To::File(node, offset) => {
                let mut done = 0;
                while done < bytes.len() {
                    match self.vfs.write_at(*node, *offset, &bytes[done..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                    }
                }
            }
            To::Fd(fd) => {
                if let Some(programs) = self
                    .control
                    .as_mut()
                    .and_then(|c| c.programs.as_deref_mut())
                {
                    let _ = programs.write(*fd, bytes);
                }
            }
        }
    }

    /// Whether Ctrl-C has stopped the command. Long loops (reading a file,
    /// copying, removing a tree) ask between pieces and give up; the shell
    /// then prints `^C` and the exit status is 130. Once true it stays true.
    pub fn interrupted(&mut self) -> bool {
        if !self.cancelled && self.console.interrupted() {
            self.cancelled = true;
        }
        self.cancelled
    }

    pub fn columns(&self) -> usize {
        self.console.columns()
    }

    /// Whether standard output is the screen (`ls` then lays out columns).
    pub fn is_tty(&self) -> bool {
        match self.out {
            Output::Console => true,
            Output::File { .. } | Output::Fd { .. } => false,
            Output::Program { tty, .. } => tty,
        }
    }

    /// Whether writing standard output to its file has failed; later output
    /// is dropped, so a command may as well stop.
    pub fn out_failed(&self) -> bool {
        matches!(
            self.out,
            Output::File { error: Some(_), .. }
                | Output::Fd { error: Some(_), .. }
                | Output::Program { error: Some(_), .. }
        )
    }

    /// The file standard output goes to, if any.
    pub fn output_node(&self) -> Option<Node> {
        match &self.out {
            Output::File { node, .. } => Some(*node),
            Output::Program { stdout, .. } => stdout.node(),
            Output::Console | Output::Fd { .. } => None,
        }
    }

    /// Writes what is left of the output; the first write error, if any.
    pub(crate) fn finish(&mut self) -> Result<(), Errno> {
        self.streams().flush();
        match self.out {
            Output::File { error: Some(e), .. }
            | Output::Fd { error: Some(e), .. }
            | Output::Program { error: Some(e), .. } => Err(e),
            _ => Ok(()),
        }
    }

    /// Prints `name: message` on the screen and returns exit status 1.
    pub fn fail(&mut self, name: &str, message: fmt::Arguments<'_>) -> i32 {
        self.err(format!("{name}: {message}\n").as_bytes());
        1
    }
}

/// A command's standard output, the screen and a script's transcript.
struct Streams<'s, 'a> {
    vfs: &'s mut dyn Vfs,
    console: &'s mut dyn Console,
    out: &'s mut Output<'a>,
    transcript: &'s mut Option<Transcript>,
    /// `/bin/sh`'s, for `Output::Fd`.
    programs: Option<&'s mut (dyn Programs + 'a)>,
}

impl Streams<'_, '_> {
    /// Standard output; the file's first write error, once there is one.
    fn out(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::Program {
                stdout,
                tty: true,
                error,
                ..
            } => {
                if error.is_none()
                    && let Err(e) = stdout.write(bytes)
                {
                    *error = Some(e);
                }
            }
            Output::File { buf, error, .. }
            | Output::Fd { buf, error, .. }
            | Output::Program { buf, error, .. } => {
                if let Some(e) = error {
                    return Err(*e);
                }
                buf.extend_from_slice(bytes);
                if buf.len() >= FILE_BUFFER {
                    self.flush();
                }
            }
        }
        match &*self.out {
            Output::File { error: Some(e), .. }
            | Output::Fd { error: Some(e), .. }
            | Output::Program { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
        }
    }

    /// Writes to the screen and a running script's transcript.
    fn screen(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut *self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.console.write(t.ended(e).as_bytes());
            *self.transcript = None;
        }
    }

    fn flush(&mut self) {
        match &mut *self.out {
            Output::File {
                node,
                offset,
                buf,
                error,
            } => {
                let mut done = 0;
                while error.is_none() && done < buf.len() {
                    match self.vfs.write_at(*node, *offset, &buf[done..]) {
                        // Nothing written would loop forever; the contract
                        // says that is ENOSPC.
                        Ok(0) => *error = Some(Errno::ENOSPC),
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                buf.clear();
            }
            Output::Program {
                stdout, buf, error, ..
            } => {
                if error.is_none()
                    && !buf.is_empty()
                    && let Err(e) = stdout.write(buf)
                {
                    *error = Some(e);
                }
                buf.clear();
            }
            Output::Fd { fd, buf, error } => {
                if error.is_none() && !buf.is_empty() {
                    let written = match self.programs.as_deref_mut() {
                        Some(programs) => programs.write(*fd, buf),
                        None => Err(Errno::EBADF),
                    };
                    if let Err(e) = written {
                        *error = Some(e);
                    }
                }
                buf.clear();
            }
            Output::Console => {}
        }
    }
}

/// `outln!(ctx, "…", args)`: a line on standard output.
macro_rules! outln {
    ($ctx:expr) => { $ctx.out(b"\n") };
    ($ctx:expr, $($arg:tt)*) => {{
        let mut line = alloc::format!($($arg)*);
        line.push('\n');
        $ctx.out(line.as_bytes());
    }};
}
pub(crate) use outln;

/// A command's parsed options.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Opts {
    /// Every option in order, with its value if it takes one.
    pub flags: Vec<(char, Option<String>)>,
    pub operands: Vec<String>,
}

impl Opts {
    pub fn has(&self, c: char) -> bool {
        self.flags.iter().any(|(f, _)| *f == c)
    }

    /// The value of the last `c` option.
    pub fn value(&self, c: char) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(f, _)| *f == c)
            .and_then(|(_, v)| v.as_deref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptError {
    Invalid(char),
    Unrecognized(String),
    MissingValue(char),
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptError::Invalid(c) => write!(f, "invalid option -- '{c}'"),
            OptError::Unrecognized(s) => write!(f, "unrecognized option '{s}'"),
            OptError::MissingValue(c) => write!(f, "option requires an argument -- '{c}'"),
        }
    }
}

/// Splits `args` into options and operands, GNU style: options may come
/// anywhere, several may share one `-` (`-la`), `--` ends them, and `-` on
/// its own is an operand. `flags` lists the options without a value,
/// `valued` those that take one (`-n 5` or `-n5`).
pub fn getopt(args: &[String], flags: &str, valued: &str) -> Result<Opts, OptError> {
    let mut opts = Opts::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            opts.operands.extend(args.cloned());
            break;
        }
        if arg.starts_with("--") {
            return Err(OptError::Unrecognized(arg.clone()));
        }
        let Some(cluster) = arg.strip_prefix('-').filter(|c| !c.is_empty()) else {
            opts.operands.push(arg.clone());
            continue;
        };
        for (i, c) in cluster.char_indices() {
            if flags.contains(c) {
                opts.flags.push((c, None));
            } else if valued.contains(c) {
                let rest = &cluster[i + c.len_utf8()..];
                let value = if rest.is_empty() {
                    args.next().cloned().ok_or(OptError::MissingValue(c))?
                } else {
                    String::from(rest)
                };
                opts.flags.push((c, Some(value)));
                break;
            } else {
                return Err(OptError::Invalid(c));
            }
        }
    }
    Ok(opts)
}

/// A name in single quotes, as GNU tools print it in messages (`'my file'`);
/// double quotes if it contains a single quote.
pub fn quote(name: &str) -> String {
    if name.contains('\'') {
        format!("\"{name}\"")
    } else {
        format!("'{name}'")
    }
}

/// A name quoted only if the shell would need it (GNU's `cat: foo: …` but
/// `cat: 'my file': …`).
pub fn quote_if_needed(name: &str) -> String {
    const SPECIAL: &str = " \t'\"\\$*?[]{}()<>|&;#!`";
    if name.is_empty() || name.chars().any(|c| SPECIAL.contains(c) || c.is_control()) {
        quote(name)
    } else {
        String::from(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn options_can_be_clustered_and_mixed_with_operands() {
        let o = getopt(&args(&["-l", "dir", "-a", "x"]), "la", "").unwrap();
        assert!(o.has('l') && o.has('a'));
        assert_eq!(o.operands, ["dir", "x"]);
        let o = getopt(&args(&["-la"]), "la", "").unwrap();
        assert_eq!(o.flags, vec![('l', None), ('a', None)]);
    }

    #[test]
    fn double_dash_ends_options_and_a_lone_dash_is_an_operand() {
        let o = getopt(&args(&["-r", "--", "-f", "-"]), "rf", "").unwrap();
        assert!(o.has('r') && !o.has('f'));
        assert_eq!(o.operands, ["-f", "-"]);
        let o = getopt(&args(&["-"]), "", "").unwrap();
        assert_eq!(o.operands, ["-"]);
    }

    #[test]
    fn options_with_values() {
        let o = getopt(&args(&["-n", "5", "f"]), "", "n").unwrap();
        assert_eq!(o.flags, vec![('n', Some("5".into()))]);
        assert_eq!(o.operands, ["f"]);
        let o = getopt(&args(&["-n3", "-n7"]), "", "n").unwrap();
        assert_eq!(
            o.flags,
            vec![('n', Some("3".into())), ('n', Some("7".into()))]
        );
        assert_eq!(
            getopt(&args(&["-n"]), "", "n"),
            Err(OptError::MissingValue('n'))
        );
    }

    #[test]
    fn unknown_options_are_errors_with_gnu_wording() {
        let e = getopt(&args(&["-lz"]), "la", "").unwrap_err();
        assert_eq!(e.to_string(), "invalid option -- 'z'");
        let e = getopt(&args(&["--all"]), "la", "").unwrap_err();
        assert_eq!(e.to_string(), "unrecognized option '--all'");
        let e = getopt(&args(&["-n"]), "", "n").unwrap_err();
        assert_eq!(e.to_string(), "option requires an argument -- 'n'");
    }

    #[test]
    fn quoting_like_gnu() {
        assert_eq!(quote("notes"), "'notes'");
        assert_eq!(quote("it's"), "\"it's\"");
        assert_eq!(quote_if_needed("notes.txt"), "notes.txt");
        assert_eq!(quote_if_needed("my file"), "'my file'");
        assert_eq!(quote_if_needed(""), "''");
    }
}
