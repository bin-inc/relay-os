//! `cat`, `head`, `tail` and `wc` (spec §7.3). Without a file they read
//! standard input (user-space gate §9.1), which GNU calls `-` in its
//! messages.

use crate::ctx::{Ctx, getopt, outln, quote, quote_if_needed};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node};

/// Files are read in pieces of this size (on the heap: the kernel stack is
/// small).
const CHUNK: usize = 64 * 1024;
const DEFAULT_LINES: u64 = 10;

/// Reads `node` from `offset` to the end it had when the reading started,
/// handing each piece to `f` until it returns false. Stopping at that end
/// matters when the output goes to the same file (`tail f >> f`): the
/// command must not read its own output back forever.
fn stream(
    ctx: &mut Ctx<'_>,
    node: Node,
    offset: u64,
    mut f: impl FnMut(&mut Ctx<'_>, &[u8]) -> bool,
) -> Result<(), Errno> {
    let end = ctx.vfs.stat(node)?.size;
    let mut buf = vec![0; CHUNK];
    let mut offset = offset;
    while offset < end && !ctx.interrupted() {
        let want = buf.len().min((end - offset) as usize);
        let n = ctx.vfs.read_at(node, offset, &mut buf[..want])?;
        if n == 0 || !f(ctx, &buf[..n]) {
            break;
        }
        offset += n as u64;
    }
    Ok(())
}

/// `cat [file…]`
pub fn cat(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("cat", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return cat_input(ctx);
    }
    let mut status = 0;
    for op in &opts.operands {
        // The shell reports the write error when the command ends.
        if ctx.out_failed() {
            break;
        }
        let name = quote_if_needed(op);
        let node = match ctx.vfs.lookup(op.as_bytes()) {
            Ok(node) => node,
            Err(e) => {
                status = ctx.fail("cat", format_args!("{name}: {e}"));
                continue;
            }
        };
        // `cat f >> f` would read its own output forever.
        if ctx.output_node() == Some(node) {
            status = ctx.fail("cat", format_args!("{name}: input file is output file"));
            continue;
        }
        if let Err(e) = stream(ctx, node, 0, |ctx, bytes| {
            ctx.out(bytes);
            !ctx.out_failed()
        }) {
            status = ctx.fail("cat", format_args!("{name}: {e}"));
        }
    }
    status
}

/// `cat` of standard input, to its end, Ctrl-C or a write error (which the
/// shell reports).
fn cat_input(ctx: &mut Ctx<'_>) -> i32 {
    let mut buf = vec![0; CHUNK];
    while !ctx.interrupted() && !ctx.out_failed() {
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => ctx.out(&buf[..n]),
            Err(e) => return ctx.fail("cat", format_args!("-: {e}")),
        }
    }
    0
}

/// The line count and the one file of `head`/`tail`: `[-n N] file`, also
/// `-N`.
fn lines_and_file(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
) -> Result<(u64, Node, String), i32> {
    // A first argument `-5` means `-n 5`, as in GNU head and tail.
    let mut args = args.to_vec();
    if let Some(first) = args.first_mut()
        && let Some(n) = first.strip_prefix('-')
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
    {
        *first = alloc::format!("-n{n}");
    }
    let opts = getopt(&args, "", "n").map_err(|e| ctx.fail(name, format_args!("{e}")))?;
    // Plain digits only: GNU's `+N` (from line N) and `-N` (all but the
    // last N) are not supported, and must not pass as a count.
    let count = match opts.value('n') {
        None => DEFAULT_LINES,
        Some(v) => match v
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| v.parse().ok())
        {
            Some(Some(n)) => n,
            _ => {
                let message = format_args!("invalid number of lines: {}", quote(v));
                return Err(ctx.fail(name, message));
            }
        },
    };
    let file = match &opts.operands[..] {
        [] => return Err(ctx.fail(name, format_args!("missing operand"))),
        [file] => file,
        [_, extra, ..] => {
            return Err(ctx.fail(name, format_args!("extra operand {}", quote(extra))));
        }
    };
    let node = ctx.vfs.lookup(file.as_bytes()).map_err(|e| {
        ctx.fail(
            name,
            format_args!("cannot open {} for reading: {e}", quote(file)),
        )
    })?;
    Ok((count, node, file.clone()))
}

/// `head [-n N] file`: the first N lines (10 by default).
pub fn head(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, node, file) = match lines_and_file(ctx, "head", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let mut left = count;
    let result = stream(ctx, node, 0, |ctx, bytes| {
        let mut end = 0;
        while left > 0 && end < bytes.len() {
            match bytes[end..].iter().position(|&b| b == b'\n') {
                Some(i) => {
                    end += i + 1;
                    left -= 1;
                }
                None => end = bytes.len(),
            }
        }
        ctx.out(&bytes[..end]);
        left > 0
    });
    match result {
        Ok(()) => 0,
        Err(e) => ctx.fail("head", format_args!("error reading {}: {e}", quote(&file))),
    }
}

/// `tail [-n N] file`: the last N lines (10 by default). It reads backwards
/// from the end, so a big file costs only what is shown.
pub fn tail(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, node, file) = match lines_and_file(ctx, "tail", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    match tail_start(ctx, node, count).and_then(|start| {
        stream(ctx, node, start, |ctx, bytes| {
            ctx.out(bytes);
            true
        })
    }) {
        Ok(()) => 0,
        Err(e) => ctx.fail("tail", format_args!("error reading {}: {e}", quote(&file))),
    }
}

/// Where the last `count` lines of `node` start.
fn tail_start(ctx: &mut Ctx<'_>, node: Node, count: u64) -> Result<u64, Errno> {
    let size = ctx.vfs.stat(node)?.size;
    if count == 0 {
        return Ok(size);
    }
    let mut buf = vec![0; CHUNK];
    let mut end = size;
    // A newline at the very end finishes the last line; it does not start
    // another one.
    if size > 0 {
        let mut last = [0u8];
        ctx.vfs.read_at(node, size - 1, &mut last)?;
        if last[0] == b'\n' {
            end -= 1;
        }
    }
    let mut found = 0;
    while end > 0 {
        let start = end.saturating_sub(CHUNK as u64);
        let piece = &mut buf[..(end - start) as usize];
        let n = ctx.vfs.read_at(node, start, piece)?;
        if n < piece.len() {
            return Err(Errno::EIO);
        }
        for (i, &b) in piece.iter().enumerate().rev() {
            if b == b'\n' {
                found += 1;
                if found == count {
                    return Ok(start + i as u64 + 1);
                }
            }
        }
        end = start;
    }
    Ok(0)
}

#[derive(Clone, Copy, Default)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
}

/// `wc file…`: lines, words and bytes, and a total for several files.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("wc", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return ctx.fail("wc", format_args!("missing operand"));
    }
    // GNU wc sizes the columns from the files' total size, with at least
    // 7 digits when something is not a regular file.
    let mut total_size = 0;
    let mut odd = false;
    let mut found = Vec::new();
    for op in &opts.operands {
        let node = ctx.vfs.lookup(op.as_bytes());
        match node.and_then(|n| Ok((n, ctx.vfs.stat(n)?))) {
            Ok((n, st)) => {
                // Saturating: a corrupt size must not overflow.
                if st.kind == FileType::Regular {
                    total_size = u64::saturating_add(total_size, st.size);
                } else {
                    odd = true;
                }
                found.push(Ok(n));
            }
            Err(e) => {
                odd = true;
                found.push(Err(e));
            }
        }
    }
    let digits = total_size.max(1).ilog10() as usize + 1;
    let width = if odd { digits.max(7) } else { digits };
    let mut status = 0;
    let mut total = Counts::default();
    for (op, node) in opts.operands.iter().zip(found) {
        let name = quote_if_needed(op);
        let counts = match node.and_then(|n| count(ctx, n)) {
            Ok(c) => c,
            Err(Errno::EISDIR) => {
                status = ctx.fail("wc", format_args!("{name}: Is a directory"));
                Counts::default()
            }
            Err(e) => {
                status = ctx.fail("wc", format_args!("{name}: {e}"));
                continue;
            }
        };
        total.lines = total.lines.saturating_add(counts.lines);
        total.words = total.words.saturating_add(counts.words);
        total.bytes = total.bytes.saturating_add(counts.bytes);
        let c = counts;
        outln!(
            ctx,
            "{:>width$} {:>width$} {:>width$} {name}",
            c.lines,
            c.words,
            c.bytes
        );
    }
    if opts.operands.len() > 1 {
        let c = total;
        outln!(
            ctx,
            "{:>width$} {:>width$} {:>width$} total",
            c.lines,
            c.words,
            c.bytes
        );
    }
    status
}

fn count(ctx: &mut Ctx<'_>, node: Node) -> Result<Counts, Errno> {
    let mut c = Counts::default();
    let mut in_word = false;
    stream(ctx, node, 0, |_, bytes| {
        c.bytes += bytes.len() as u64;
        for &b in bytes {
            if b == b'\n' {
                c.lines += 1;
            }
            let space = matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
            if !space && !in_word {
                c.words += 1;
            }
            in_word = !space;
        }
        true
    })?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use crate::testing::Harness;
    use alloc::format;
    use alloc::string::String;

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }

    #[test]
    fn cat_prints_files_one_after_another() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"one\n");
        h.put("/tmp/b", b"two");
        assert_eq!(h.run("cat /tmp/a /tmp/b"), (0, "one\ntwo".into()));
        assert_eq!(h.run("cat /tmp/a > /tmp/c"), (0, "".into()));
        assert_eq!(h.get("/tmp/c"), b"one\n");
    }

    #[test]
    fn cat_reports_errors_and_goes_on() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"one\n");
        assert_eq!(
            h.run("cat /tmp/nope /tmp /tmp/a"),
            (
                1,
                "cat: /tmp/nope: No such file or directory\ncat: /tmp: Is a directory\none\n"
                    .into()
            )
        );
        assert_eq!(
            h.run("cat 'a b'"),
            (1, "cat: 'a b': No such file or directory\n".into())
        );
    }

    #[test]
    fn cat_without_a_file_copies_its_standard_input() {
        let mut h = Harness::new();
        h.stdin = b"typed\nlines".to_vec();
        assert_eq!(h.run("cat"), (0, "typed\nlines".into()));
        assert_eq!(h.run("cat"), (0, "".into()), "an input that has ended");
        // With a file, standard input is not read.
        h.stdin = b"unread".to_vec();
        assert_eq!(h.run("cat /etc/hostname"), (0, "relay\n".into()));
        // More than its buffer, into a file.
        let big = numbered(20_000);
        assert!(big.len() > 2 * super::CHUNK);
        h.stdin = big.clone().into_bytes();
        assert_eq!(h.run("cat > /tmp/copy"), (0, "".into()));
        assert_eq!(h.get("/tmp/copy"), big.as_bytes());
    }

    #[test]
    fn cat_of_standard_input_stops_at_ctrl_c_and_at_write_and_read_errors() {
        let mut h = Harness::new();
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat > /tmp/out"), (130, "^C\n".into()));
        assert!(h.get("/tmp/out").len() <= super::CHUNK, "one piece at most");
        let mut h = Harness::with_capacity(5 * 4096);
        h.stdin = numbered(20_000).into_bytes();
        assert_eq!(
            h.run("cat > /tmp/out"),
            (1, "cat: write error: No space left on device\n".into())
        );
        // Nor is the rest of the input read for nothing once the output
        // cannot be written. (A hundred pieces, then the end: a cat that
        // reads on fails the test instead of hanging it.)
        struct Endless(usize);
        impl crate::Stdin for Endless {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                self.0 += 1;
                if self.0 > 100 {
                    return Ok(0);
                }
                buf.fill(b'x');
                Ok(buf.len())
            }
        }
        let (mut input, mut out) = (Endless(0), crate::testing::FakeStdout::file(None));
        out.fail_after = Some((10_000, vfs::Errno::ENOSPC));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut input,
            stdout: &mut out,
        };
        assert_eq!(crate::run_command("cat", super::cat, &[], io), 1);
        assert_eq!(input.0, 1, "one piece, then the write error");
        h.console.take();
        // A read that fails, as a program's fd 0 can.
        struct Broken;
        impl crate::Stdin for Broken {
            fn read(&mut self, _: &mut [u8]) -> Result<usize, vfs::Errno> {
                Err(vfs::Errno::EIO)
            }
        }
        let mut out = crate::testing::FakeStdout::console();
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut Broken,
            stdout: &mut out,
        };
        let status = crate::run_command("cat", super::cat, &[], io);
        assert_eq!(
            (status, h.console.take()),
            (1, "cat: -: Input/output error\n".into())
        );
    }

    #[test]
    fn cat_stops_at_the_first_write_error() {
        // GNU stops once its output cannot be written: the other inputs
        // are not read, and their errors are not reported.
        let mut h = Harness::with_capacity(5 * 4096);
        h.put("/tmp/big", &[b'x'; 8192]);
        assert_eq!(
            h.run("cat /tmp/big /tmp/nope /tmp/big > /tmp/out"),
            (1, "cat: write error: No space left on device\n".into())
        );
        assert_eq!(h.get("/tmp/out").len(), 4096);
        // Nor is the rest of a big input read over USB for nothing.
        let mut h = Harness::with_capacity(4 * 4096 + 4 * super::CHUNK as u64);
        h.put("/tmp/big", &vec![b'x'; 4 * super::CHUNK]);
        h.spy.reads.set(0);
        assert_eq!(h.run("cat /tmp/big > /tmp/out").0, 1);
        assert_eq!(h.spy.reads.get(), 1);
    }

    #[test]
    fn cat_refuses_to_append_a_file_to_itself() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"one\n");
        assert_eq!(
            h.run("cat /tmp/a >> /tmp/a"),
            (1, "cat: /tmp/a: input file is output file\n".into())
        );
        assert_eq!(h.get("/tmp/a"), b"one\n");
    }

    #[test]
    fn cat_streams_files_bigger_than_its_buffer() {
        let mut h = Harness::new();
        let big: String = numbered(20_000);
        assert!(big.len() > 2 * super::CHUNK);
        h.put("/tmp/big", big.as_bytes());
        h.run("cat /tmp/big > /tmp/copy");
        assert_eq!(h.get("/tmp/copy"), big.as_bytes());
    }

    #[test]
    fn ctrl_c_stops_a_long_cat() {
        let mut h = Harness::new();
        h.put("/tmp/big", numbered(20_000).as_bytes());
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big /etc/motd"), (130, "^C\n".into()));
        h.console.interrupt = false;
        assert_eq!(h.run("cat /etc/hostname"), (0, "relay\n".into()));
    }

    #[test]
    fn head_shows_the_first_lines() {
        let mut h = Harness::new();
        h.put("/tmp/f", numbered(12).as_bytes());
        assert_eq!(h.run("head /tmp/f"), (0, numbered(10)));
        assert_eq!(h.run("head -n 3 /tmp/f"), (0, numbered(3)));
        assert_eq!(h.run("head -n2 /tmp/f"), (0, numbered(2)));
        assert_eq!(h.run("head -1 /tmp/f"), (0, numbered(1)));
        assert_eq!(h.run("head -n 0 /tmp/f"), (0, "".into()));
        assert_eq!(h.run("head -n 50 /tmp/f"), (0, numbered(12)));
        h.put("/tmp/g", b"no newline");
        assert_eq!(h.run("head /tmp/g"), (0, "no newline".into()));
    }

    #[test]
    fn tail_shows_the_last_lines() {
        let mut h = Harness::new();
        h.put("/tmp/f", numbered(12).as_bytes());
        assert_eq!(
            h.run("tail /tmp/f").1,
            numbered(12).split_off(numbered(2).len())
        );
        assert_eq!(h.run("tail -n 1 /tmp/f"), (0, "line 12\n".into()));
        assert_eq!(h.run("tail -n 0 /tmp/f"), (0, "".into()));
        assert_eq!(h.run("tail -n 99 /tmp/f"), (0, numbered(12)));
        h.put("/tmp/g", b"a\nb\nlast without newline");
        assert_eq!(
            h.run("tail -n 2 /tmp/g"),
            (0, "b\nlast without newline".into())
        );
        h.put("/tmp/empty", b"");
        assert_eq!(h.run("tail /tmp/empty"), (0, "".into()));
    }

    #[test]
    fn tail_of_a_big_file_reads_backwards_across_pieces() {
        let mut h = Harness::new();
        let big = numbered(30_000);
        h.put("/tmp/big", big.as_bytes());
        assert_eq!(h.run("tail -n 2 /tmp/big").1, "line 29999\nline 30000\n");
        let expected: String = (20_001..=30_000).map(|i| format!("line {i}\n")).collect();
        assert_eq!(h.run("tail -n 10000 /tmp/big").1, expected);
    }

    #[test]
    fn appending_a_file_to_itself_reads_only_what_was_there() {
        // Enough output to be flushed to the file while it is being read.
        let mut h = Harness::with_capacity(64 * 4096);
        let text = numbered(2000);
        h.put("/tmp/f", text.as_bytes());
        assert_eq!(h.run("tail -n 1000 /tmp/f >> /tmp/f"), (0, "".into()));
        let tail: String = (1001..=2000).map(|i| format!("line {i}\n")).collect();
        assert_eq!(h.get("/tmp/f"), format!("{text}{tail}").as_bytes());
        h.put("/tmp/g", text.as_bytes());
        assert_eq!(h.run("head -n 5000 /tmp/g >> /tmp/g"), (0, "".into()));
        assert_eq!(h.get("/tmp/g"), format!("{text}{text}").as_bytes());
    }

    #[test]
    fn head_and_tail_errors() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("head /nope"),
            (
                1,
                "head: cannot open '/nope' for reading: No such file or directory\n".into()
            )
        );
        assert_eq!(
            h.run("tail -n x /etc/motd"),
            (1, "tail: invalid number of lines: 'x'\n".into())
        );
        assert_eq!(
            h.run("head -n -3 /etc/motd"),
            (1, "head: invalid number of lines: '-3'\n".into())
        );
        assert_eq!(
            h.run("tail -n +2 /etc/motd"),
            (1, "tail: invalid number of lines: '+2'\n".into())
        );
        assert_eq!(h.run("head"), (1, "head: missing operand\n".into()));
        assert_eq!(h.run("tail a b"), (1, "tail: extra operand 'b'\n".into()));
        assert_eq!(
            h.run("head /tmp"),
            (1, "head: error reading '/tmp': Is a directory\n".into())
        );
        assert_eq!(
            h.run("tail -z f"),
            (1, "tail: invalid option -- 'z'\n".into())
        );
    }

    #[test]
    fn wc_counts_lines_words_and_bytes() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"hello world\nsecond line here\n");
        assert_eq!(h.run("wc /tmp/a"), (0, " 2  5 29 /tmp/a\n".into()));
        h.put("/tmp/b", b"  x\ty  ");
        assert_eq!(
            h.run("wc /tmp/a /tmp/b"),
            (
                0,
                " 2  5 29 /tmp/a\n 0  2  7 /tmp/b\n 2  7 36 total\n".into()
            )
        );
    }

    #[test]
    fn wc_reports_missing_files_and_directories() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"x\n");
        let expected = concat!(
            "      1       1       2 /tmp/a\n",
            "wc: /nope: No such file or directory\n",
            "wc: /tmp: Is a directory\n",
            "      0       0       0 /tmp\n",
            "      1       1       2 total\n",
        );
        assert_eq!(h.run("wc /tmp/a /nope /tmp"), (1, expected.into()));
    }
}
