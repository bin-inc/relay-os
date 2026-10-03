//! `grep [-i] [-v] [-n] [-c] [-q] PATTERN [FILE...]` (user-space gate
//! §9.1): the lines of each file, or of standard input, that hold a match
//! of `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`
//! (a file `-` is standard input too).
//! Several files put each one's name before its lines; `-i` ignores case,
//! `-v` selects the lines that do not match, `-n` numbers them, `-c` counts
//! them instead. An input that holds a NUL byte is binary: once one of its
//! lines is selected, grep says so (`grep: f: binary file matches`)
//! instead of printing it; it never reads the file its output goes to.
//! The status is 0 if a line was selected, 1 if none was, and 2 after an
//! error, a write error included.
//!
//! `-q` (spec §15 item 3), GNU's quiet mode, prints nothing and ends at the
//! first selected line, reading no further and opening no later file, with
//! status 0 even after an error; it writes nothing, so its output may be an
//! input.

use crate::ctx::{Ctx, getopt};
use crate::pattern::Pattern;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType};

/// Input is read in pieces of this size.
const CHUNK: usize = 64 * 1024;

/// What `-n`, `-v`, `-c`, `-q` and the number of inputs ask for.
struct Options {
    invert: bool,
    number: bool,
    count: bool,
    quiet: bool,
    /// Each line starts with its input's name.
    names: bool,
}

/// How an input went.
enum Outcome {
    /// Whether a line was selected.
    Read(bool),
    Failed,
}

pub fn grep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    ctx.set_write_error_status(2);
    let opts = match getopt(args, "cinqv", "") {
        Ok(o) => o,
        Err(e) => return usage(ctx, Some(format!("{e}"))),
    };
    let Some((pattern, files)) = opts.operands.split_first() else {
        return usage(ctx, None);
    };
    let pattern = match Pattern::parse(pattern.as_bytes(), opts.has('i')) {
        Ok(p) => p,
        Err(e) => {
            ctx.fail("grep", format_args!("{e}"));
            return 2;
        }
    };
    let options = Options {
        invert: opts.has('v'),
        number: opts.has('n'),
        count: opts.has('c'),
        quiet: opts.has('q'),
        names: files.len() > 1,
    };
    let mut outcomes = Vec::new();
    if files.is_empty() {
        outcomes.push(search(ctx, &pattern, &options, None));
    }
    for file in files {
        let found = || outcomes.iter().any(|o| matches!(o, Outcome::Read(true)));
        if ctx.out_failed() || ctx.interrupted() || (options.quiet && found()) {
            break;
        }
        outcomes.push(search(ctx, &pattern, &options, Some(file)));
    }
    let found = outcomes.iter().any(|o| matches!(o, Outcome::Read(true)));
    if options.quiet && found {
        0
    } else if outcomes.iter().any(|o| matches!(o, Outcome::Failed)) {
        2
    } else if found {
        0
    } else {
        1
    }
}

/// GNU's two lines for a command line it cannot use; status 2.
fn usage(ctx: &mut Ctx<'_>, error: Option<String>) -> i32 {
    if let Some(e) = error {
        ctx.err(format!("grep: {e}\n").as_bytes());
    }
    ctx.err(
        b"Usage: grep [OPTION]... PATTERNS [FILE]...\nTry 'grep --help' for more information.\n",
    );
    2
}

/// Searches one input: the file `file`, or standard input.
fn search(
    ctx: &mut Ctx<'_>,
    pattern: &Pattern,
    options: &Options,
    file: Option<&String>,
) -> Outcome {
    // `-` is standard input, as no file is.
    let file = file.filter(|f| *f != "-");
    let name = file.map_or("(standard input)", String::as_str);
    // It would read what it wrote, for ever (`grep x f >> f`): GNU refuses
    // a file that is the output, whatever is left in it, unless it writes
    // no lines (`-q`, `-c`).
    let checks = !options.quiet && !options.count;
    let mut source = match file {
        Some(path) => match open(ctx, path) {
            Ok(node) if checks && ctx.output_node() == Some(node) => {
                ctx.fail(
                    "grep",
                    format_args!("{path}: input file is also the output"),
                );
                return Outcome::Failed;
            }
            Ok(node) => Source::File { node, offset: 0 },
            Err(e) => {
                ctx.fail("grep", format_args!("{path}: {e}"));
                return Outcome::Failed;
            }
        },
        None => {
            let input = ctx.input_file().map(|(node, ..)| node);
            if checks && input.is_some() && input == ctx.output_node() {
                ctx.fail(
                    "grep",
                    format_args!("(standard input): input file is also the output"),
                );
                return Outcome::Failed;
            }
            Source::Input
        }
    };
    let mut lines = Lines {
        pattern,
        options,
        name,
        number: 0,
        selected: 0,
        binary: false,
    };
    let mut buf = vec![0; CHUNK];
    let mut line: Vec<u8> = Vec::new();
    loop {
        if ctx.interrupted() || ctx.out_failed() {
            return Outcome::Read(lines.selected > 0);
        }
        let n = match source.read(ctx, &mut buf) {
            Ok(n) => n,
            Err(e) => {
                ctx.fail("grep", format_args!("{name}: {e}"));
                return Outcome::Failed;
            }
        };
        if n == 0 {
            // What is left is the last line, without its newline.
            if !line.is_empty() && !lines.take(ctx, &line) {
                return Outcome::Read(true);
            }
            break;
        }
        lines.binary |= buf[..n].contains(&0);
        for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
            line.extend_from_slice(piece);
            if line.pop_if(|b| *b == b'\n').is_some() {
                if !lines.take(ctx, &line) {
                    return Outcome::Read(true);
                }
                line.clear();
            }
        }
    }
    if options.count && !options.quiet {
        let prefix = if options.names {
            format!("{name}:")
        } else {
            String::new()
        };
        ctx.out(format!("{prefix}{}\n", lines.selected).as_bytes());
    }
    Outcome::Read(lines.selected > 0)
}

/// An input's lines, as they come.
struct Lines<'p> {
    pattern: &'p Pattern,
    options: &'p Options,
    name: &'p str,
    /// Lines so far, and those selected.
    number: u64,
    selected: u64,
    /// A NUL byte has come.
    binary: bool,
}

impl Lines<'_> {
    /// The next line, without its newline: printed if it is selected.
    /// False once a binary input has had a line selected, which is said
    /// instead, and the rest of the input is not read; and under `-q` once
    /// a line is selected at all.
    fn take(&mut self, ctx: &mut Ctx<'_>, line: &[u8]) -> bool {
        self.number += 1;
        if self.pattern.is_match(line) == self.options.invert {
            return true;
        }
        self.selected += 1;
        if self.options.quiet {
            return false;
        }
        if self.options.count {
            return true;
        }
        if self.binary {
            ctx.err(format!("grep: {}: binary file matches\n", self.name).as_bytes());
            return false;
        }
        print(ctx, self.options, self.name, self.number, line);
        true
    }
}

/// A selected line, with its input's name and number when asked for.
fn print(ctx: &mut Ctx<'_>, options: &Options, name: &str, number: u64, line: &[u8]) {
    let mut out = Vec::with_capacity(line.len() + name.len() + 24);
    if options.names {
        out.extend_from_slice(name.as_bytes());
        out.push(b':');
    }
    if options.number {
        out.extend_from_slice(format!("{number}:").as_bytes());
    }
    out.extend_from_slice(line);
    out.push(b'\n');
    ctx.out(&out);
}

/// A file to read: a directory is GNU's `Is a directory`.
fn open(ctx: &mut Ctx<'_>, path: &str) -> Result<vfs::Node, Errno> {
    let node = ctx.vfs.lookup(path.as_bytes())?;
    if ctx.vfs.stat(node)?.kind == FileType::Directory {
        return Err(Errno::EISDIR);
    }
    Ok(node)
}

/// Where the lines come from.
enum Source {
    File { node: vfs::Node, offset: u64 },
    Input,
}

impl Source {
    /// The next piece; 0 at the end.
    fn read(&mut self, ctx: &mut Ctx<'_>, buf: &mut [u8]) -> Result<usize, Errno> {
        match self {
            Source::File { node, offset } => {
                let n = ctx.vfs.read_at(*node, *offset, buf)?;
                *offset += n as u64;
                Ok(n)
            }
            Source::Input => ctx.read_input(buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, host_tool};

    #[test]
    fn grep_prints_what_gnu_grep_prints() {
        let files: &[(&str, &[u8])] = &[
            ("t", b"abc\nxyz\nABC\nb\n"),
            ("u", b"one b\nno newline at the end"),
            ("bin", b"a\0b\nabc\n"),
            ("d/", b""),
        ];
        let cases: &[(&[&str], &[u8])] = &[
            (&["grep", "b", "t"], b""),
            (&["grep", "-i", "abc", "t"], b""),
            (&["grep", "-v", "b", "t"], b""),
            (&["grep", "-n", "b", "t"], b""),
            (&["grep", "-c", "b", "t"], b""),
            (&["grep", "-c", "b", "t", "u"], b""),
            (&["grep", "b", "t", "u"], b""),
            (&["grep", "-vn", "x", "t", "u"], b""),
            (&["grep", "-nc", "end$", "u"], b""),
            (&["grep", "end$", "u"], b""),
            (&["grep", "zzz", "t"], b""),
            (&["grep", "a", "bin"], b""),
            (&["grep", "-c", "a", "bin"], b""),
            (&["grep", "-v", "q", "bin"], b""),
            (&["grep", "a", "t", "bin"], b""),
            (&["grep", "x", "nope", "t"], b""),
            (&["grep", "x", "d"], b""),
            (&["grep", "[a", "t"], b""),
            (&["grep", "a\\", "t"], b""),
            (&["grep"], b""),
            (&["grep", "-j", "a", "t"], b""),
            (&["grep", "b"], b"abc\nb\nzzz"),
            (&["grep", "-c", "."], b"one\n\ntwo\n"),
            (&["grep", "-c", "."], b""),
            (&["grep", "-n", "x"], b"a\nx\nyx"),
            (&["grep", "a"], b"x\na\0\n"),
            (&["grep", "-v", "a"], b"a\nb\n"),
            (&["grep", "-", "t"], b""),
            (&["grep", "a", "t", "-"], b"a\nz\n"),
            (&["grep", "-c", "b", "-", "t"], b"b\n"),
            (&["grep", "-n", "x", "-"], b"x\n"),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, files, stdin),
                host_tool(args, files, stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn grep_q_says_only_whether_a_line_is_selected() {
        // probes/grepq.txt q1–q15 (GNU grep 3.11).
        let files: &[(&str, &[u8])] = &[
            ("f", b"a\nb\n"),
            ("g", b"zz\n"),
            ("bin", b"a\0b\n"),
            ("d/", b""),
        ];
        let cases: &[(&[&str], &[u8])] = &[
            (&["grep", "-q", "a", "f"], b""),
            (&["grep", "-q", "x", "f"], b""),
            // It stops at the first selected line: `nope` is never opened.
            (&["grep", "-q", "a", "f", "nope"], b""),
            (&["grep", "-q", "a", "f", "d"], b""),
            // An error before it is told; the status is still 0.
            (&["grep", "-q", "a", "nope", "f"], b""),
            (&["grep", "-q", "x", "nope", "f"], b""),
            (&["grep", "-q", "a", "d", "g", "f"], b""),
            (&["grep", "-qc", "a", "f"], b""),
            (&["grep", "-qc", "x", "f"], b""),
            (&["grep", "-qn", "a", "f"], b""),
            (&["grep", "-qv", "a", "f"], b""),
            (&["grep", "-qv", ".", "f"], b""),
            (&["grep", "-qi", "A", "f"], b""),
            (&["grep", "-q", "", "g"], b""),
            (&["grep", "-q", "a", "bin"], b""),
            (&["grep", "-q", "a", "f", "f"], b""),
            (&["grep", "-q", "a", "-", "f"], b"z\n"),
            (&["grep", "-q", "a"], b"x\na\n"),
            (&["grep", "-q", "a"], b""),
            (&["grep", "-q", "é"], "été\n".as_bytes()),
            (&["grep", "-q"], b""),
            (&["grep", "-q", "[a", "f"], b""),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, files, stdin),
                host_tool(args, files, stdin),
                "{args:?}"
            );
        }
    }

    /// Standard input that never ends by itself: `yes` lines, counted.
    struct Endless {
        reads: usize,
    }

    impl crate::Stdin for Endless {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
            self.reads += 1;
            if self.reads > 100 {
                return Ok(0);
            }
            let n = buf.len().min(4096) / 4 * 4;
            for line in buf[..n].chunks_mut(4) {
                line.copy_from_slice(b"yes\n");
            }
            Ok(n)
        }
    }

    #[test]
    fn grep_q_stops_reading_at_its_first_selected_line() {
        // As GNU's: `yes | grep -q y` ends at once (probe q17).
        for (args, status, reads) in [(&["-q", "y"][..], 0, 1), (&["-c", "y"], 0, 101)] {
            let mut h = Harness::new();
            let mut input = Endless { reads: 0 };
            let mut out = crate::testing::FakeStdout::file(None);
            let args: alloc::vec::Vec<String> = args.iter().map(|a| String::from(*a)).collect();
            let got = crate::run_command(
                "grep",
                super::grep,
                &args,
                crate::CommandIo {
                    vfs: &mut h.vfs,
                    console: &mut h.console,
                    system: &mut h.system,
                    stdin: &mut input,
                    stdout: &mut out,
                },
            );
            assert_eq!((got, input.reads), (status, reads), "{args:?}");
        }
    }

    #[test]
    fn grep_q_writes_nothing_so_its_output_may_be_an_input() {
        // GNU skips the check of an input that is the output file under -q,
        // which writes nothing (probes q16, q16c).
        let mut h = Harness::new();
        h.put("/tmp/f", b"a\nb\n");
        assert_eq!(h.run("grep -q a /tmp/f >> /tmp/f"), (0, "".into()));
        assert_eq!(h.run("grep -q x /tmp/f >> /tmp/f"), (1, "".into()));
        assert_eq!(h.get("/tmp/f"), b"a\nb\n");
    }

    #[test]
    fn a_device_is_never_the_input_that_is_the_output() {
        // GNU compares only regular files; /dev/null as both is no
        // refusal (the prototype's review, M-2).
        let mut h = Harness::new();
        h.vfs
            .mount(b"/dev", alloc::boxed::Box::new(vfs::DevFs::new(0)))
            .unwrap();
        for line in [
            "grep x /dev/null > /dev/null",
            "grep x < /dev/null > /dev/null",
            "grep -c x /dev/null >> /dev/null",
            "cat /dev/null - < /dev/null > /dev/null",
        ] {
            let (status, out, err) = host_tool(&["sh", "-c", line], &[], b"");
            assert_eq!(h.run(line), (status, out + &err), "{line}");
        }
    }

    #[test]
    fn grep_never_reads_its_own_output() {
        // GNU's refusal, whatever the redirection left in the file; a disk
        // that fills and a Ctrl-C bound a grep that reads on.
        let mut h = Harness::with_capacity(64 * 4096);
        h.console.interrupt_after = Some(10_000);
        let text: String = (0..2000).map(|i| alloc::format!("1 line {i}\n")).collect();
        h.put("/tmp/f", text.as_bytes());
        assert_eq!(
            h.run("grep 1 /tmp/f >> /tmp/f"),
            (2, "grep: /tmp/f: input file is also the output\n".into())
        );
        assert_eq!(h.get("/tmp/f"), text.as_bytes(), "unchanged");
        assert_eq!(
            h.run("grep 1 /tmp/nope /tmp/f > /tmp/f"),
            (
                2,
                "grep: /tmp/nope: No such file or directory\ngrep: /tmp/f: input file is also the output\n".into()
            )
        );
        // Standard input too, whatever is left in it, unless no line is
        // written (`-q`, `-c`), as GNU's (the prototype's review, I-1; GNU
        // grep 3.11, tmp/m5p1/probes/p15.txt).
        h.put("/tmp/s", b"1\n");
        let said = "grep: (standard input): input file is also the output\n";
        assert_eq!(h.run("grep 1 < /tmp/s >> /tmp/s"), (2, said.into()));
        assert_eq!(h.get("/tmp/s"), b"1\n");
        assert_eq!(h.run("grep 1 < /tmp/s > /tmp/s"), (2, said.into()));
        h.put("/tmp/s", b"1\n");
        assert_eq!(h.run("grep -c 1 < /tmp/s >> /tmp/s"), (0, String::new()));
        assert_eq!(h.run("grep -c 1 /tmp/s >> /tmp/s"), (0, String::new()));
        assert_eq!(h.get("/tmp/s"), b"1\n1\n2\n");
        assert_eq!(h.run("grep -q 1 < /tmp/s >> /tmp/s"), (0, String::new()));
        // Another file into it is fine.
        h.put("/tmp/g", b"1\n");
        assert_eq!(h.run("grep 1 /tmp/g >> /tmp/f"), (0, "".into()));
    }

    #[test]
    fn a_write_error_is_status_2_as_gnu_grep_s() {
        let mut h = Harness::with_capacity(5 * 4096);
        let text: String = (0..20_000).map(|i| alloc::format!("line {i}\n")).collect();
        h.stdin = text.into_bytes();
        assert_eq!(
            h.run("grep line > /tmp/out"),
            (2, "grep: write error: No space left on device\n".into())
        );
        // As a program too; and nothing written, nothing failed: 1.
        let mut out = crate::testing::FakeStdout::file(None);
        out.fail_after = Some((10, vfs::Errno::ENOSPC));
        h.stdin = b"a\nb\nccccccccccccccccc\n".to_vec();
        assert_eq!(
            h.program("grep c", &mut out),
            (2, "grep: write error: No space left on device\n".into())
        );
        h.stdin = b"a\n".to_vec();
        assert_eq!(h.program("grep z", &mut out), (1, String::new()));
    }

    #[test]
    fn a_long_input_is_searched_in_pieces_with_its_lines_whole() {
        let mut h = Harness::new();
        // Lines across the 64 KiB pieces of the read.
        let text: String = (0..40_000).map(|i| alloc::format!("line {i}\n")).collect();
        h.put("/tmp/big", text.as_bytes());
        assert_eq!(h.run("grep -c '9$' /tmp/big"), (0, "4000\n".into()));
        assert_eq!(
            h.run("grep '^line.39999$' /tmp/big"),
            (0, "line 39999\n".into())
        );
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("grep x /tmp/big"), (130, "^C\n".into()));
    }
}
