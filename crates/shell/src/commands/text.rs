//! `cat`, `head`, `tail` and `wc` (spec §7.3). Without a file, and for a
//! file named `-`, they read standard input (user-space gate §9.1), as
//! GNU's do.

use crate::ctx::{Ctx, getopt, outln, quote, quote_if_needed};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node};

/// Files are read in pieces of this size (on the heap: a program's stack
/// is small).
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
        if ctx.out_failed() || ctx.interrupted() {
            break;
        }
        if op == "-" {
            status = status.max(cat_input(ctx));
            continue;
        }
        let name = quote_if_needed(op);
        let node = match ctx.vfs.lookup(op.as_bytes()) {
            Ok(node) => node,
            Err(e) => {
                status = ctx.fail("cat", format_args!("{name}: {e}"));
                continue;
            }
        };
        // `cat f >> f` would read its own output forever: GNU's check, the
        // output the same regular file and something left to read in it
        // (`cat f > f` has emptied `f` and reads nothing).
        if ctx.output_node() == Some(node) && ctx.vfs.stat(node).is_ok_and(|st| st.size > 0) {
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
/// shell reports). A file that is the output, with bytes left to read, is
/// refused, as for an operand (`cat < f >> f`).
fn cat_input(ctx: &mut Ctx<'_>) -> i32 {
    if let Some((node, at, size)) = ctx.input_file()
        && ctx.output_node() == Some(node)
        && at < size
    {
        return ctx.fail("cat", format_args!("-: input file is output file"));
    }
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

/// The line count and the one file of `head`/`tail`: `[-n N] [file]`, also
/// `-N`; no file, or `-`, is standard input.
fn lines_and_file(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
) -> Result<(u64, Option<(Node, String)>), i32> {
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
        [] => return Ok((count, None)),
        [file] if file == "-" => return Ok((count, None)),
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
    Ok((count, Some((node, file.clone()))))
}

/// How much of `bytes`, the next piece of an input, is within the `left`
/// lines still wanted; counts off the lines it ends.
fn within(bytes: &[u8], left: &mut u64) -> usize {
    let mut end = 0;
    while *left > 0 && end < bytes.len() {
        match bytes[end..].iter().position(|&b| b == b'\n') {
            Some(i) => {
                end += i + 1;
                *left -= 1;
            }
            None => end = bytes.len(),
        }
    }
    end
}

/// `head [-n N] [file]`: the first N lines (10 by default).
pub fn head(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, file) = match lines_and_file(ctx, "head", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let mut left = count;
    let Some((node, file)) = file else {
        return head_input(ctx, left);
    };
    let result = stream(ctx, node, 0, |ctx, bytes| {
        let end = within(bytes, &mut left);
        ctx.out(&bytes[..end]);
        left > 0
    });
    match result {
        Ok(()) => 0,
        Err(e) => ctx.fail("head", format_args!("error reading {}: {e}", quote(&file))),
    }
}

/// `head` of standard input: no more of it is read once the lines are out,
/// so a pipe's writer gets `EPIPE` once `head` has ended.
fn head_input(ctx: &mut Ctx<'_>, mut left: u64) -> i32 {
    let mut buf = vec![0; CHUNK];
    while left > 0 && !ctx.interrupted() && !ctx.out_failed() {
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let end = within(&buf[..n], &mut left);
                ctx.out(&buf[..end]);
            }
            Err(e) => return ctx.fail("head", format_args!("error reading 'standard input': {e}")),
        }
    }
    0
}

/// `tail [-n N] [file]`: the last N lines (10 by default). It reads a file
/// backwards from the end, so a big file costs only what is shown;
/// standard input is read to its end, keeping the last N lines.
pub fn tail(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, file) = match lines_and_file(ctx, "tail", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let Some((node, file)) = file else {
        return tail_input(ctx, count);
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

/// `tail` of standard input: the last `count` lines, the newest perhaps
/// without its newline, kept as they come.
fn tail_input(ctx: &mut Ctx<'_>, count: u64) -> i32 {
    let mut lines: alloc::collections::VecDeque<Vec<u8>> = alloc::collections::VecDeque::new();
    let mut buf = vec![0; CHUNK];
    loop {
        if ctx.interrupted() {
            return 0;
        }
        let n = match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return ctx.fail("tail", format_args!("error reading 'standard input': {e}")),
        };
        for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
            match lines.back_mut() {
                Some(last) if last.last() != Some(&b'\n') => last.extend_from_slice(piece),
                _ => lines.push_back(piece.to_vec()),
            }
            if lines.len() as u64 > count {
                lines.pop_front();
            }
        }
    }
    for line in lines {
        ctx.out(&line);
    }
    0
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

impl Counts {
    /// Counts `bytes`, the next piece of an input; `in_word` says whether
    /// the piece before ended inside a word.
    fn add(&mut self, bytes: &[u8], in_word: &mut bool) {
        self.bytes += bytes.len() as u64;
        for &b in bytes {
            if b == b'\n' {
                self.lines += 1;
            }
            let space = matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
            if !space && !*in_word {
                self.words += 1;
            }
            *in_word = !space;
        }
    }

    fn plus(self, o: Counts) -> Counts {
        Counts {
            lines: self.lines.saturating_add(o.lines),
            words: self.words.saturating_add(o.words),
            bytes: self.bytes.saturating_add(o.bytes),
        }
    }
}

/// Which counts `wc` prints: those its options name (`-l`, `-w`, `-c`), or
/// all three, always in that order.
#[derive(Clone, Copy)]
struct Shown {
    lines: bool,
    words: bool,
    bytes: bool,
}

impl Shown {
    fn how_many(self) -> usize {
        [self.lines, self.words, self.bytes]
            .iter()
            .filter(|&&s| s)
            .count()
    }

    /// The shown counts, each right-aligned to `width`, then the name.
    fn line(self, c: Counts, width: usize, name: Option<&str>) -> String {
        let mut parts: Vec<String> = [
            (self.lines, c.lines),
            (self.words, c.words),
            (self.bytes, c.bytes),
        ]
        .iter()
        .filter(|(shown, _)| *shown)
        .map(|(_, n)| alloc::format!("{n:>width$}"))
        .collect();
        parts.extend(name.map(String::from));
        parts.join(" ")
    }
}

/// `wc [-clw] [file…]`: lines, words and bytes, and a total for several
/// files; standard input without a file, and for `-`.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "clw", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("wc", format_args!("{e}")),
    };
    let (lines, words, bytes) = (opts.has('l'), opts.has('w'), opts.has('c'));
    let any = lines || words || bytes;
    let shown = Shown {
        lines: lines || !any,
        words: words || !any,
        bytes: bytes || !any,
    };
    if opts.operands.is_empty() {
        return wc_input(ctx, shown);
    }
    // GNU's widths: one count of one file is not padded; otherwise the
    // columns fit the regular files' total size, with at least 7 digits
    // when one of the files is something else. A file that cannot be found
    // takes no part (milestone 1 counted it as something else).
    let mut total_size = 0;
    let mut odd = false;
    let mut found = Vec::new();
    for op in &opts.operands {
        // `-` is standard input, never a regular file here.
        if op == "-" {
            odd = true;
            found.push(Ok(None));
            continue;
        }
        let node = ctx.vfs.lookup(op.as_bytes());
        match node.and_then(|n| Ok((n, ctx.vfs.stat(n)?))) {
            Ok((n, st)) => {
                // Saturating: a corrupt size must not overflow.
                if st.kind == FileType::Regular {
                    total_size = u64::saturating_add(total_size, st.size);
                } else {
                    odd = true;
                }
                found.push(Ok(Some(n)));
            }
            Err(e) => found.push(Err(e)),
        }
    }
    let digits = total_size.max(1).ilog10() as usize + 1;
    let unpadded = found.len() == 1 && shown.how_many() == 1;
    let width = match (unpadded, odd) {
        (true, _) => 1,
        (false, true) => digits.max(7),
        (false, false) => digits,
    };
    let mut status = 0;
    let mut total = Counts::default();
    for (op, node) in opts.operands.iter().zip(found) {
        let name = quote_if_needed(op);
        let counted = match node {
            Ok(Some(n)) => count(ctx, n),
            Ok(None) => match count_input(ctx) {
                None => return 0,
                Some((c, None)) => Ok(c),
                Some((c, Some(e))) => {
                    status = ctx.fail("wc", format_args!("-: {e}"));
                    Ok(c)
                }
            },
            Err(e) => Err(e),
        };
        let counts = match counted {
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
        total = total.plus(counts);
        outln!(ctx, "{}", shown.line(counts, width, Some(&name)));
    }
    if opts.operands.len() > 1 {
        outln!(ctx, "{}", shown.line(total, width, Some("total")));
    }
    status
}

/// `wc` of standard input, which is never a regular file here (the
/// console or a pipe): 7 digits, unless only one count is shown.
fn wc_input(ctx: &mut Ctx<'_>, shown: Shown) -> i32 {
    let Some((c, error)) = count_input(ctx) else {
        return 0;
    };
    let status = error.map_or(0, |e| ctx.fail("wc", format_args!("-: {e}")));
    let width = if shown.how_many() == 1 { 1 } else { 7 };
    outln!(ctx, "{}", shown.line(c, width, None));
    status
}

/// Standard input's counts, to its end or a read error (with the counts
/// so far); `None` once Ctrl-C has stopped the command.
fn count_input(ctx: &mut Ctx<'_>) -> Option<(Counts, Option<Errno>)> {
    let mut buf = vec![0; CHUNK];
    let (mut c, mut in_word) = (Counts::default(), false);
    loop {
        if ctx.interrupted() {
            return None;
        }
        match ctx.read_input(&mut buf) {
            Ok(0) => return Some((c, None)),
            Ok(n) => c.add(&buf[..n], &mut in_word),
            Err(e) => return Some((c, Some(e))),
        }
    }
}

fn count(ctx: &mut Ctx<'_>, node: Node) -> Result<Counts, Errno> {
    let mut c = Counts::default();
    let mut in_word = false;
    stream(ctx, node, 0, |_, bytes| {
        c.add(bytes, &mut in_word);
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
    fn cat_prints_what_gnu_cat_prints() {
        let files: &[(&str, &[u8])] = &[("t", b"text\n"), ("d/", b"")];
        let big = alloc::vec![b'x'; 1 << 20];
        let cases: &[(&[&str], &[u8])] = &[
            (&["cat", "t"], b""),
            (&["cat"], b"in\n"),
            (&["cat", "-"], b"in\n"),
            (&["cat", "nope", "-"], b"in\n"),
            (&["cat", "t", "-", "t"], b"in\n"),
            (&["cat", "-", "-"], b"once\n"),
            (&["cat", "d", "t"], b""),
            (&["cat"], &big),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, files, stdin),
                crate::testing::host_tool(args, files, stdin),
                "{args:?}"
            );
        }
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
        // GNU's condition, standard input too (the prototype's review,
        // I-1; GNU 9.4, tmp/m5p1/probes/p15.txt): the same regular file,
        // with something left to read; `>` has emptied it.
        assert_eq!(
            h.run("cat < /tmp/a >> /tmp/a"),
            (1, "cat: -: input file is output file\n".into())
        );
        assert_eq!(h.get("/tmp/a"), b"one\n");
        assert_eq!(
            h.run("cat - /tmp/a < /tmp/a >> /tmp/a"),
            (
                1,
                "cat: -: input file is output file\ncat: /tmp/a: input file is output file\n"
                    .into()
            )
        );
        assert_eq!(h.run("cat < /tmp/a > /tmp/a"), (0, String::new()));
        assert_eq!(h.get("/tmp/a"), b"");
        h.put("/tmp/a", b"one\n");
        assert_eq!(h.run("cat /tmp/a > /tmp/a"), (0, String::new()));
        assert_eq!(h.get("/tmp/a"), b"");
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
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big /etc/motd"), (130, "^C\n".into()));
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
    fn head_and_tail_of_standard_input_print_what_gnu_s_print() {
        let numbered_bytes = numbered(30);
        let ten: &[u8] = numbered_bytes.as_bytes();
        let unended: &[u8] = b"one\ntwo\nthree";
        let big_text = numbered(20_000);
        let big: &[u8] = big_text.as_bytes();
        let cases: &[(&[&str], &[u8])] = &[
            (&["head"], ten),
            (&["head", "-n", "3"], ten),
            (&["head", "-3"], ten),
            (&["head", "-n", "0"], ten),
            (&["head", "-n", "5"], unended),
            (&["head", "-n", "2"], unended),
            (&["head"], b""),
            (&["head", "-n", "2"], big),
            (&["tail"], ten),
            (&["tail", "-n", "3"], ten),
            (&["tail", "-2"], unended),
            (&["tail", "-n", "1"], unended),
            (&["tail", "-n", "0"], ten),
            (&["tail", "-n", "100"], ten),
            (&["tail"], b""),
            (&["tail", "-n", "5"], big),
            (&["tail", "-n", "3"], b"\n\n\n\n"),
            (&["head", "-n", "2", "-"], ten),
            (&["tail", "-1", "-"], ten),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &[], stdin),
                crate::testing::host_tool(args, &[], stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn head_of_standard_input_reads_no_more_than_its_lines() {
        // A line a read, as a pipe may give them; a hundred, then the end
        // (a head that reads on fails the test instead of hanging it).
        struct Lines(usize);
        impl crate::Stdin for Lines {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                self.0 += 1;
                if self.0 > 100 {
                    return Ok(0);
                }
                buf[..2].copy_from_slice(b"x\n");
                Ok(2)
            }
        }
        let mut h = Harness::new();
        let (mut input, mut out) = (Lines(0), crate::testing::FakeStdout::file(None));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut input,
            stdout: &mut out,
        };
        let args = [String::from("-n"), String::from("3")];
        assert_eq!(crate::run_command("head", super::head, &args, io), 0);
        assert_eq!((out.text().as_str(), input.0), ("x\nx\nx\n", 3));
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("tail -n 1"), (130, "^C\n".into()));
    }

    #[test]
    fn tail_of_standard_input_joins_lines_a_pipe_cuts() {
        // Three bytes a read, as a pipe may give them.
        struct Cut(&'static [u8]);
        impl crate::Stdin for Cut {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                let n = self.0.len().min(3).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let mut h = Harness::new();
        let mut out = crate::testing::FakeStdout::file(None);
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut Cut(b"one\ntwo\nthree\nfour"),
            stdout: &mut out,
        };
        let args = [String::from("-n"), String::from("2")];
        assert_eq!(crate::run_command("tail", super::tail, &args, io), 0);
        assert_eq!(out.text(), "three\nfour");
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
    fn wc_prints_what_gnu_wc_prints() {
        let a: &[u8] = b"hello world\nsecond line here\n";
        let b: &[u8] = b"  x\ty  ";
        let files = [("a", a), ("b", b), ("d/", &b""[..])];
        let cases: &[(&[&str], &[u8])] = &[
            (&["wc"], a),
            (&["wc", "-c"], a),
            (&["wc", "-l"], a),
            (&["wc", "-w"], b),
            (&["wc", "-lw"], a),
            (&["wc", "-cl"], b),
            (&["wc", "-l", "-c", "-w"], a),
            (&["wc"], b""),
            (&["wc", "-c"], b""),
            (&["wc", "a"], b""),
            (&["wc", "-c", "a"], b""),
            (&["wc", "a", "b"], b""),
            (&["wc", "-l", "a", "b"], b""),
            (&["wc", "-wc", "b", "a"], b""),
            (&["wc", "a", "nope"], b""),
            (&["wc", "nope", "a"], b""),
            (&["wc", "-c", "nope", "a"], b""),
            (&["wc", "-l", "nope"], b""),
            (&["wc", "-l", "d", "a"], b""),
            (&["wc", "a", "d", "nope"], b"not read"),
            (&["wc", "-"], a),
            (&["wc", "-c", "-"], b),
            (&["wc", "-", "a"], b),
            (&["wc", "-l", "a", "-"], a),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &files, stdin),
                crate::testing::host_tool(args, &files, stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn wc_of_standard_input_stops_at_ctrl_c() {
        let mut h = Harness::new();
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("wc -l"), (130, "^C\n".into()));
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
