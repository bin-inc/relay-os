//! `sh FILE`: runs the commands in a file, one line at a time, as if each
//! had been typed (spec §15 item 12). There are no variables, loops or
//! conditions: a script is a list of commands. Each command is shown as
//! `+ <line>` before its output, as `set -x` does, so a photo of the
//! screen shows which command printed what. A failing command does not
//! stop the script; Ctrl-C does. Everything the script shows on the screen,
//! errors included, also goes into a transcript next to it (`x.sh` →
//! `x.log`), written and synced as each line starts and ends, so it can be
//! checked afterwards (`cargo xtask verify-usb`). Under `/bin/sh` the
//! transcript is a console tee, and a script may run another. A command of
//! the script that redirects into the script's own transcript garbles it,
//! as it would under bash: the redirection writes from the file's start,
//! the transcript goes on where it was.

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::format;
use alloc::string::String;
use vfs::{Errno, FileType, Node, path};

/// The largest script, in bytes: far more than a check needs, and read
/// whole onto the heap.
pub const SCRIPT_MAX: u64 = 64 * 1024;

/// A script `sh` has read, for the shell to run.
pub(crate) struct Script {
    pub text: String,
    /// The transcript file, emptied, and its name as `sh` was given it.
    pub transcript: Node,
    pub transcript_name: String,
}

/// The transcript of `script`: `.sh` becomes `.log`; other names get
/// `.log` added.
pub fn transcript_name(script: &str) -> String {
    match script.strip_suffix(".sh") {
        Some(stem) => format!("{stem}.log"),
        None => format!("{script}.log"),
    }
}

/// `sh FILE`: checks and reads the file and empties its transcript; the
/// shell then runs its lines (`Shell::execute`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("sh", format_args!("{e}")),
    };
    let file = match &opts.operands[..] {
        [] => return ctx.fail("sh", format_args!("missing operand")),
        [file] => file,
        [_, extra, ..] => return ctx.fail("sh", format_args!("extra operand {}", quote(extra))),
    };
    if ctx.in_script {
        return ctx.fail("sh", format_args!("a script cannot run another script"));
    }
    if !ctx.is_tty() {
        return ctx.fail("sh", format_args!("a script's output cannot be redirected"));
    }
    let name = quote_if_needed(file);
    let text = match read(ctx, file) {
        Ok(text) => text,
        Err(Error::Errno(e)) => return ctx.fail("sh", format_args!("{name}: {e}")),
        Err(Error::NotText) => return ctx.fail("sh", format_args!("{name}: not a text file")),
    };
    let log = transcript_name(file);
    match empty_file(ctx, &log) {
        Ok(transcript) => {
            ctx.script = Some(Script {
                text,
                transcript,
                transcript_name: log,
            });
            0
        }
        Err(e) => ctx.fail(
            "sh",
            format_args!(
                "cannot write the transcript {}: {e}",
                quote_if_needed(&path::display(log.as_bytes()))
            ),
        ),
    }
}

/// Creates the file at `name`, or empties it.
fn empty_file(ctx: &mut Ctx<'_>, name: &str) -> Result<Node, Errno> {
    match ctx.vfs.lookup(name.as_bytes()) {
        Ok(node) => {
            match ctx.vfs.stat(node)?.kind {
                FileType::Regular => {}
                FileType::Directory => return Err(Errno::EISDIR),
                _ => return Err(Errno::EINVAL),
            }
            ctx.vfs.truncate(node, 0)?;
            Ok(node)
        }
        Err(Errno::ENOENT) => ctx.vfs.create(name.as_bytes()),
        Err(e) => Err(e),
    }
}

enum Error {
    Errno(Errno),
    NotText,
}

impl From<Errno> for Error {
    fn from(e: Errno) -> Error {
        Error::Errno(e)
    }
}

/// The whole file, which must be a regular file of UTF-8 text no larger
/// than `SCRIPT_MAX`.
fn read(ctx: &mut Ctx<'_>, file: &str) -> Result<String, Error> {
    let node = ctx.vfs.lookup(file.as_bytes())?;
    let st = ctx.vfs.stat(node)?;
    match st.kind {
        FileType::Regular => {}
        FileType::Directory => return Err(Errno::EISDIR.into()),
        _ => return Err(Errno::EINVAL.into()),
    }
    if st.size > SCRIPT_MAX {
        return Err(Errno::EFBIG.into());
    }
    let mut buf = alloc::vec![0; st.size as usize];
    let mut done = 0;
    while done < buf.len() {
        match ctx.vfs.read_at(node, done as u64, &mut buf[done..])? {
            0 => break,
            n => done += n,
        }
    }
    buf.truncate(done);
    let text = String::from_utf8(buf).map_err(|_| Error::NotText)?;
    // Windows editors may start the file with a byte-order mark.
    match text.strip_prefix('\u{FEFF}') {
        Some(rest) => Ok(String::from(rest)),
        None => Ok(text),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::Harness;
    use alloc::string::String;

    #[test]
    fn each_line_runs_after_its_trace() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo one\n\n  # a comment\necho two > /tmp/o # into o\n  cat /tmp/o\t\nnope\n",
        );
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                127,
                "+ echo one\none\n+ echo two > /tmp/o # into o\n+ cat /tmp/o\ntwo\n\
                 + nope\nrelay-sh: nope: command not found\n"
                    .into()
            )
        );
        // The exit status is the last command's.
        h.put("/tmp/ok.sh", b"nope\necho fine");
        assert_eq!(h.run("sh /tmp/ok.sh").0, 0);
    }

    #[test]
    fn a_line_runs_exactly_as_typed() {
        // Only the trace is trimmed; `\ ` at the end of a line is a space.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"  echo a\\ \n\techo 'b '  \n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (0, "+ echo a\\\na \n+ echo 'b '\nb \n".into())
        );
    }

    #[test]
    fn a_line_that_does_not_parse_does_not_stop_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo 'open\nls | wc\necho after\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ echo 'open\nrelay-sh: syntax error: unterminated quote\n\
                 + ls | wc\nrelay-sh: unsupported syntax: |\n+ echo after\nafter\n"
                    .into()
            )
        );
    }

    #[test]
    fn every_line_is_synced_as_it_starts_and_ends() {
        // The trace reaches the disk before the command runs, so a machine
        // that hangs in it leaves the command's name in the transcript.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo a > /tmp/a\n# nothing\necho b > /tmp/b\n",
        );
        h.run("sh /tmp/s.sh");
        // Two per command, then `sh` itself.
        assert_eq!(h.spy.syncs.get(), 5);
        assert_eq!(h.get("/tmp/b"), b"b\n");
    }

    #[test]
    fn relative_paths_and_cd_work_as_typed() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cd /etc\ncat hostname\n");
        h.run("cd /tmp");
        assert_eq!(
            h.run("sh s.sh"),
            (0, "+ cd /etc\n+ cat hostname\nrelay\n".into())
        );
        assert_eq!(h.run("pwd").1, "/etc\n");
    }

    #[test]
    fn ctrl_c_stops_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo a\necho b\necho c\n");
        // Asked once before each line: the second time Ctrl-C was pressed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
        assert_eq!(h.get("/tmp/s.log"), b"+ echo a\na\n^C\n");
        // A command that Ctrl-C stopped stops the script too.
        h.put("/tmp/big", &alloc::vec![b'x'; 200_000]);
        h.put("/tmp/s.sh", b"cat /tmp/big > /tmp/copy\necho after\n");
        h.console.interrupt = false;
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (130, "+ cat /tmp/big > /tmp/copy\n^C\n".into())
        );
    }

    #[test]
    fn exit_ends_the_script_not_the_shell() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"exit 4\necho after\n");
        assert_eq!(h.run("sh /tmp/s.sh"), (4, "+ exit 4\n".into()));
        // The shell reads on, as when /bin/sh ran the script as its child.
        h.console.type_in(b"sh /tmp/s.sh\recho still\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 0);
        assert!(
            h.console
                .text()
                .ends_with("+ exit 4\nroot@relay:/# echo still\nstill\nroot@relay:/# ")
        );
    }

    #[test]
    fn reboot_ends_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"reboot\necho after\n");
        assert_eq!(h.run("sh /tmp/s.sh"), (0, "+ reboot\n".into()));
        assert_eq!(h.system.reboots, 1);
        // The transcript names the command that restarted the machine.
        assert_eq!(h.get("/tmp/s.log"), b"+ reboot\n");
    }

    #[test]
    fn a_script_cannot_run_a_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"sh /tmp/s.sh\necho after\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ sh /tmp/s.sh\nsh: a script cannot run another script\n+ echo after\nafter\n"
                    .into()
            )
        );
    }

    #[test]
    fn a_script_saved_on_windows_runs() {
        // CRLF line ends and a UTF-8 byte-order mark, as Windows editors
        // save; without dropping the mark the first command is not found.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"\xEF\xBB\xBFecho one\r\necho two\r\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (0, "+ echo one\none\n+ echo two\ntwo\n".into())
        );
    }

    #[test]
    fn the_transcript_holds_what_the_screen_showed() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo one\n# not shown\necho two > /tmp/o\ncat /tmp/nope\nnope\ncat /tmp/o\n",
        );
        let (status, screen) = h.run("sh /tmp/s.sh");
        assert_eq!(status, 0);
        assert_eq!(
            screen,
            "+ echo one\none\n+ echo two > /tmp/o\n+ cat /tmp/nope\n\
             cat: /tmp/nope: No such file or directory\n+ nope\n\
             relay-sh: nope: command not found\n+ cat /tmp/o\ntwo\n"
        );
        // Errors too, which never go into a redirection file.
        assert_eq!(String::from_utf8(h.get("/tmp/s.log")).unwrap(), screen);
        // Running a script again starts a new transcript.
        h.put("/tmp/s.sh", b"echo again\n");
        h.run("sh /tmp/s.sh");
        assert_eq!(h.get("/tmp/s.log"), b"+ echo again\nagain\n");
    }

    #[test]
    fn the_transcript_is_written_as_each_line_starts_and_ends() {
        // A machine that hangs in a command leaves the lines before it and
        // the command's own name.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo one\ncat /tmp/s.log\n");
        assert_eq!(
            h.run("sh /tmp/s.sh").1,
            "+ echo one\none\n+ cat /tmp/s.log\n+ echo one\none\n+ cat /tmp/s.log\n"
        );
    }

    #[test]
    fn transcript_names() {
        let mut h = Harness::new();
        h.dir("/tmp/d.sh");
        for (script, log) in [
            ("/tmp/a.sh", "/tmp/a.log"),
            ("/tmp/b", "/tmp/b.log"),
            ("/tmp/d.sh/c.txt", "/tmp/d.sh/c.txt.log"),
            ("/tmp/d.sh/.sh", "/tmp/d.sh/.log"),
        ] {
            h.put(script, b"echo x\n");
            h.run(&alloc::format!("sh {script}"));
            assert_eq!(h.get(log), b"+ echo x\nx\n", "{script}");
        }
        // A relative name is taken from where `sh` ran, whatever the
        // script's `cd` does.
        h.put("/tmp/r.sh", b"cd /\necho y\n");
        h.run("cd /tmp");
        h.run("sh r.sh");
        assert_eq!(h.get("/tmp/r.log"), b"+ cd /\n+ echo y\ny\n");
    }

    #[test]
    fn a_transcript_that_cannot_be_written() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo a > /tmp/a\n");
        h.dir("/tmp/s.log");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                1,
                "sh: cannot write the transcript /tmp/s.log: Is a directory\n".into()
            )
        );
        assert!(!h.exists("/tmp/a"), "nothing ran");
        // A disk that fills up ends the transcript, not the script.
        let mut h = Harness::with_capacity(3 * 4096);
        h.put("/tmp/s.sh", b"echo a\necho b\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ echo a\nsh: /tmp/s.log: No space left on device; the transcript ends here\n\
                 a\n+ echo b\nb\n"
                    .into()
            )
        );
    }

    #[test]
    fn sh_errors() {
        let mut h = Harness::new();
        let run = |h: &mut Harness, line: &str| -> (i32, String) { h.run(line) };
        assert_eq!(run(&mut h, "sh"), (1, "sh: missing operand\n".into()));
        h.put("/tmp/s.sh", b"echo hi\n");
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh x"),
            (1, "sh: extra operand 'x'\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp/nope"),
            (1, "sh: /tmp/nope: No such file or directory\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp"),
            (1, "sh: /tmp: Is a directory\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh > /tmp/out"),
            (1, "sh: a script's output cannot be redirected\n".into())
        );
        h.put("/tmp/bin.sh", b"echo \xff\n");
        assert_eq!(
            run(&mut h, "sh /tmp/bin.sh"),
            (1, "sh: /tmp/bin.sh: not a text file\n".into())
        );
        h.put(
            "/tmp/huge.sh",
            &alloc::vec![b'#'; super::SCRIPT_MAX as usize + 1],
        );
        assert_eq!(
            run(&mut h, "sh /tmp/huge.sh"),
            (1, "sh: /tmp/huge.sh: File too large\n".into())
        );
        h.put(
            "/tmp/max.sh",
            &alloc::vec![b'#'; super::SCRIPT_MAX as usize],
        );
        assert_eq!(run(&mut h, "sh /tmp/max.sh"), (0, "".into()));
    }
}
