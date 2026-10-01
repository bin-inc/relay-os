//! `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` (user-space gate §9.1):
//! the lines of each file, or of standard input, that hold a match of
//! `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`.
//! Several files put each one's name before its lines; `-i` ignores case,
//! `-v` selects the lines that do not match, `-n` numbers them, `-c` counts
//! them instead. An input that holds a NUL byte is binary: once one of its
//! lines is selected, grep says so (`grep: f: binary file matches`)
//! instead of printing it; it never reads the file its output goes to.
//! The status is 0 if a line was selected, 1 if none was, and 2 after an
//! error, a write error included.

use crate::ctx::{Ctx, getopt};
use crate::pattern::Pattern;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType};

/// Input is read in pieces of this size.
const CHUNK: usize = 64 * 1024;

/// What `-n`, `-v`, `-c` and the number of inputs ask for.
struct Options {
    invert: bool,
    number: bool,
    count: bool,
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
    let opts = match getopt(args, "cinv", "") {
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
        names: files.len() > 1,
    };
    let mut outcomes = Vec::new();
    if files.is_empty() {
        outcomes.push(search(ctx, &pattern, &options, None));
    }
    for file in files {
        if ctx.out_failed() || ctx.interrupted() {
            break;
        }
        outcomes.push(search(ctx, &pattern, &options, Some(file)));
    }
    if outcomes.iter().any(|o| matches!(o, Outcome::Failed)) {
        2
    } else if outcomes.iter().any(|o| matches!(o, Outcome::Read(true))) {
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
    let name = file.map_or("(standard input)", String::as_str);
    let mut source = match file {
        Some(path) => match open(ctx, path) {
            // It would read what it wrote, for ever (`grep x f >> f`).
            Ok(node) if ctx.output_node() == Some(node) => {
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
        None => Source::Input,
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
    if options.count {
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
    /// instead, and the rest of the input is not read.
    fn take(&mut self, ctx: &mut Ctx<'_>, line: &[u8]) -> bool {
        self.number += 1;
        if self.pattern.is_match(line) == self.options.invert {
            return true;
        }
        self.selected += 1;
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
        h.console.interrupt = true;
        assert_eq!(h.run("grep x /tmp/big"), (130, "^C\n".into()));
    }
}
