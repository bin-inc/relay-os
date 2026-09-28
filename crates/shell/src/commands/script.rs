//! `sh FILE`: runs the commands in a file, one line at a time, as if each
//! had been typed (spec §15 item 12). There are no variables, loops or
//! conditions: a script is a list of commands. Each command is shown as
//! `+ <line>` before its output, as `set -x` does, so a photo of the
//! screen shows which command printed what. A failing command does not
//! stop the script; Ctrl-C does.

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::string::String;
use vfs::{Errno, FileType};

/// The largest script, in bytes: far more than a check needs, and read
/// whole onto the heap.
pub const SCRIPT_MAX: u64 = 64 * 1024;

/// `sh FILE`: checks and reads the file; the shell then runs its lines
/// (`Shell::execute`).
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
    match read(ctx, file) {
        Ok(text) => {
            ctx.script = Some(text);
            0
        }
        Err(Error::Errno(e)) => ctx.fail("sh", format_args!("{name}: {e}")),
        Err(Error::NotText) => ctx.fail("sh", format_args!("{name}: not a text file")),
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
    fn every_line_is_synced_as_it_ends() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo a > /tmp/a\n# nothing\necho b > /tmp/b\n",
        );
        h.run("sh /tmp/s.sh");
        // Two commands, then `sh` itself.
        assert_eq!(h.spy.syncs.get(), 3);
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
    fn reboot_ends_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"reboot\necho after\n");
        assert_eq!(h.run("sh /tmp/s.sh"), (0, "+ reboot\n".into()));
        assert_eq!(h.system.reboots, 1);
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
