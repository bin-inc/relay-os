//! `test EXPRESSION` and `[ EXPRESSION ]` (spec §6, §15 item 3): GNU
//! coreutils 9.4's grammar rule for rule, its messages word for word in
//! the C locale, status 0 if the expression is true, 1 if it is false and
//! 2 after an error. One evaluator under two names: called as `[`, the last
//! argument must be `]`, and `[ --help` and `[ --version` are refused
//! where GNU prints its help.
//!
//! The rules by argument count come first, for 1 to 4 arguments, then `-o`
//! over `-a` over terms; a term is a run of `!`, then `(` and an expression
//! of up to 4 arguments before its `)` (or of all the rest), a binary
//! operator if the argument after next is one, a unary operator of the form
//! `-X`, or a string. Strings are compared as bytes; GNU's program has no
//! `<` or `>`, which bash's built-in has. Integers are read as GNU reads
//! them (blanks around, a sign, any number of digits) and compared as digit
//! strings, so nothing overflows; `-l STRING` stands for STRING's length.
//! Files are answered from `stat`, as for root, with one decided difference
//! (spec §10): a symbolic link is never followed, so `-e`, `-f` and `-d`
//! look at the link itself. `-w` is false on a filesystem `statfs` says is
//! read-only, but for a device or a FIFO, as Linux's `access` answers (spec
//! §15 item 6). The evaluator never recurses: each open `(` waits on a
//! stack of its own, so it nests as deep as the arguments go, as GNU does
//! on its larger stack.

use crate::commands::Run;
use crate::ctx::Ctx;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;
use vfs::FileType;

pub fn test(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    evaluate(ctx, "test", args, None)
}

pub fn bracket(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let [only] = args
        && (only == "--help" || only == "--version")
    {
        ctx.err(format!("[: unrecognized option '{only}'\n").as_bytes());
        return 2;
    }
    match args.split_last() {
        Some((last, rest)) if last == "]" => evaluate(ctx, "[", rest, Some("]")),
        _ => {
            ctx.err(b"[: missing ']'\n");
            2
        }
    }
}

/// `/bin/test`'s name and command function: `[` when the last part of the
/// path it was started by is `[`, as the shells give a command's name as
/// typed (one program under two names, spec §15 item 3).
pub fn named(arg0: &[u8]) -> (&'static str, Run) {
    match arg0.rsplit(|&b| b == b'/').next() {
        Some(b"[") => ("[", bracket),
        _ => ("test", test),
    }
}

/// Runs the expression `args`; `past` is what GNU's `[` still sees past
/// its end (the `]`).
fn evaluate(ctx: &mut Ctx<'_>, name: &str, args: &[String], past: Option<&str>) -> i32 {
    if args.is_empty() {
        return 1;
    }
    let mut e = Eval {
        ctx,
        args,
        past,
        pos: 0,
    };
    let answer = e
        .expression(args.len())
        .and_then(|v| match args.get(e.pos) {
            Some(extra) => Err(format!("extra argument {}", quote(extra))),
            None => Ok(v),
        });
    match answer {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            e.ctx.err(format!("{name}: {message}\n").as_bytes());
            2
        }
    }
}

/// A truth value, or GNU's message for an expression it cannot read.
type Answer = Result<bool, String>;

/// What waits for a value: an `-o` or `-a` list (its value so far), a `(`
/// (whether a `!` came before it), or the `!` of the 4-argument rule.
enum Frame {
    Or(bool),
    And(bool),
    Paren(bool),
    Not,
}

/// What to read next: an expression of so many arguments (GNU's
/// `posixtest`), the 3-argument rule, an `-o` list, or a term.
enum Step {
    Count(usize),
    Three,
    Expr,
    Term,
}

struct Eval<'e, 'c> {
    ctx: &'e mut Ctx<'c>,
    args: &'e [String],
    past: Option<&'e str>,
    /// The next argument.
    pos: usize,
}

impl<'e> Eval<'e, '_> {
    /// GNU's `posixtest(n)`, its recursion turned into `stack`.
    fn expression(&mut self, n: usize) -> Answer {
        let mut stack = Vec::new();
        let mut step = Step::Count(n);
        loop {
            let mut value = match step {
                Step::Count(1) => self.one(),
                Step::Count(2) => self.two()?,
                Step::Count(3) => {
                    step = Step::Three;
                    continue;
                }
                Step::Count(4) if self.is(self.pos, "!") => {
                    self.advance(true)?;
                    stack.push(Frame::Not);
                    step = Step::Three;
                    continue;
                }
                Step::Count(4) if self.is(self.pos, "(") && self.is(self.pos + 3, ")") => {
                    self.pos += 1;
                    let v = self.two()?;
                    self.pos += 1;
                    v
                }
                Step::Count(_) => {
                    step = Step::Expr;
                    continue;
                }
                Step::Three => match self.three()? {
                    Some(v) => v,
                    None => {
                        step = Step::Expr;
                        continue;
                    }
                },
                Step::Expr => {
                    if self.pos >= self.args.len() {
                        return Err(self.beyond());
                    }
                    stack.push(Frame::Or(false));
                    stack.push(Frame::And(true));
                    step = Step::Term;
                    continue;
                }
                Step::Term => {
                    let mut negated = false;
                    while self.is(self.pos, "!") {
                        self.advance(true)?;
                        negated = !negated;
                    }
                    if self.pos >= self.args.len() {
                        return Err(self.beyond());
                    }
                    if self.is(self.pos, "(") {
                        self.advance(true)?;
                        stack.push(Frame::Paren(negated));
                        step = Step::Count(self.in_parens());
                        continue;
                    }
                    negated ^ self.operand()?
                }
            };
            // Hand the value back until a list takes another term.
            loop {
                match stack.pop() {
                    None => return Ok(value),
                    Some(Frame::Not) => value = !value,
                    Some(Frame::Paren(negated)) => {
                        self.close()?;
                        value ^= negated;
                    }
                    Some(Frame::And(so_far)) => {
                        let so_far = so_far & value;
                        if self.is(self.pos, "-a") {
                            self.pos += 1;
                            stack.push(Frame::And(so_far));
                            step = Step::Term;
                            break;
                        }
                        value = so_far;
                    }
                    Some(Frame::Or(so_far)) => {
                        let so_far = so_far | value;
                        if self.is(self.pos, "-o") {
                            self.pos += 1;
                            stack.push(Frame::Or(so_far));
                            stack.push(Frame::And(true));
                            step = Step::Term;
                            break;
                        }
                        value = so_far;
                    }
                }
            }
        }
    }

    fn arg(&self, i: usize) -> Option<&'e str> {
        self.args.get(i).map(String::as_str)
    }

    fn is(&self, i: usize, word: &str) -> bool {
        self.arg(i) == Some(word)
    }

    /// Moves to the next argument; with `needed`, one must be there.
    fn advance(&mut self, needed: bool) -> Result<(), String> {
        self.pos += 1;
        if needed && self.pos >= self.args.len() {
            return Err(self.beyond());
        }
        Ok(())
    }

    fn beyond(&self) -> String {
        let last = self.args.last().map_or("", String::as_str);
        format!("missing argument after {}", quote(last))
    }

    fn one(&mut self) -> bool {
        let v = self.arg(self.pos).is_some_and(|a| !a.is_empty());
        self.pos += 1;
        v
    }

    fn two(&mut self) -> Answer {
        if self.is(self.pos, "!") {
            self.pos += 1;
            return Ok(!self.one());
        }
        if unary_form(self.arg(self.pos)) {
            return self.unary();
        }
        Err(self.beyond())
    }

    /// GNU's 3-argument rule; `None` where it reads an `-a` or `-o` list.
    fn three(&mut self) -> Result<Option<bool>, String> {
        let p = self.pos;
        if binop(self.arg(p + 1)) {
            return self.binary(false).map(Some);
        }
        if self.is(p, "!") {
            self.advance(true)?;
            return self.two().map(|v| Some(!v));
        }
        if self.is(p, "(") && self.is(p + 2, ")") {
            self.pos += 1;
            let v = self.one();
            self.pos += 1;
            return Ok(Some(v));
        }
        if self.is(p + 1, "-a") || self.is(p + 1, "-o") {
            return Ok(None);
        }
        let op = self.arg(p + 1).unwrap_or("");
        Err(format!("{}: binary operator expected", quote(op)))
    }

    /// How many arguments the expression after a `(` has: up to its `)`
    /// if one comes within 4, else all the rest.
    fn in_parens(&self) -> usize {
        let mut n = 1;
        while self.pos + n < self.args.len() && !self.is(self.pos + n, ")") {
            if n == 4 {
                return self.args.len() - self.pos;
            }
            n += 1;
        }
        n
    }

    /// The `)` after an expression in parentheses.
    fn close(&mut self) -> Result<(), String> {
        let at = match self.arg(self.pos) {
            None if self.pos == self.args.len() => self.past,
            found => found,
        };
        match at {
            None => Err(format!("{} expected", quote(")"))),
            Some(")") => {
                self.pos += 1;
                Ok(())
            }
            Some(other) => Err(format!("{} expected, found {}", quote(")"), quote(other))),
        }
    }

    /// A term after its `!`s and `(`: a binary or unary operator's, or a
    /// string.
    fn operand(&mut self) -> Answer {
        let (p, left) = (self.pos, self.args.len() - self.pos);
        if left >= 4 && self.is(p, "-l") && binop(self.arg(p + 2)) {
            return self.binary(true);
        }
        if left >= 3 && binop(self.arg(p + 1)) {
            return self.binary(false);
        }
        if unary_form(self.arg(p)) {
            return self.unary();
        }
        Ok(self.one())
    }

    /// The operand after a unary operator, which must be there.
    fn unary_operand(&mut self) -> Result<&'e str, String> {
        self.advance(true)?;
        self.pos += 1;
        Ok(&self.args[self.pos - 1])
    }

    fn unary(&mut self) -> Answer {
        let op = &self.args[self.pos];
        let file: fn(&vfs::Stat) -> bool = match op.as_bytes()[1] {
            b'n' => return Ok(!self.unary_operand()?.is_empty()),
            b'z' => return Ok(self.unary_operand()?.is_empty()),
            b't' => {
                let fd = Number::parse(self.unary_operand()?)?;
                return Ok(fd.fd().is_some_and(|fd| self.ctx.system.is_terminal(fd)));
            }
            b'w' => {
                let path = self.unary_operand()?;
                return Ok(self
                    .stat(path)
                    .is_some_and(|(_, s)| !self.read_only(path, &s)));
            }
            // As for root, which reads every file.
            b'e' | b'r' => |_| true,
            // As for root: any execute bit, or a directory to search.
            b'x' => |s| s.perm & 0o111 != 0 || s.kind == FileType::Directory,
            b'f' => |s| s.kind == FileType::Regular,
            b'd' => |s| s.kind == FileType::Directory,
            b'h' | b'L' => |s| s.kind == FileType::Symlink,
            b'p' => |s| s.kind == FileType::Fifo,
            b'S' => |s| s.kind == FileType::Socket,
            b'b' => |s| s.kind == FileType::BlockDev,
            b'c' => |s| s.kind == FileType::CharDev,
            b's' => |s| s.size > 0,
            b'u' => |s| s.perm & 0o4000 != 0,
            b'g' => |s| s.perm & 0o2000 != 0,
            b'k' => |s| s.perm & 0o1000 != 0,
            // Every program runs as root, user and group 0.
            b'O' => |s| s.uid == 0,
            b'G' => |s| s.gid == 0,
            b'N' => |s| s.mtime > s.atime,
            _ => return Err(format!("{}: unary operator expected", quote(op))),
        };
        let path = self.unary_operand()?;
        Ok(self.stat(path).is_some_and(|(_, s)| file(&s)))
    }

    /// Whether writing `path`, whose status is `s`, is refused because its
    /// filesystem is read-only, as `statfs` tells (spec §15 item 6): as
    /// Linux's `access` says `EROFS` for a regular file, a directory or a
    /// symbolic link there, and not for a device or a FIFO.
    fn read_only(&mut self, path: &str, s: &vfs::Stat) -> bool {
        matches!(
            s.kind,
            FileType::Regular | FileType::Directory | FileType::Symlink
        ) && self
            .ctx
            .vfs
            .statfs(path.as_bytes())
            .is_ok_and(|f| f.read_only)
    }

    /// GNU's `binary_operator`, at the left operand (or at the `-l`
    /// before it, `left_is_l`).
    fn binary(&mut self, left_is_l: bool) -> Answer {
        if left_is_l {
            self.pos += 1;
        }
        let op = self.pos + 1;
        let right_is_l = op + 2 < self.args.len() && self.is(op + 1, "-l");
        if right_is_l {
            self.pos += 1;
        }
        let which = self.args[op].as_str();
        if let Some(compare) = integer_operator(which) {
            let left = match left_is_l {
                true => Number::length(&self.args[op - 1]),
                false => Number::parse(&self.args[op - 1])?,
            };
            let right = match right_is_l {
                true => Number::length(&self.args[op + 2]),
                false => Number::parse(&self.args[op + 1])?,
            };
            self.pos += 3;
            return Ok(compare(left.cmp(&right)));
        }
        if which.starts_with('-') {
            self.pos += 3;
            if left_is_l || right_is_l {
                return Err(format!("{which} does not accept -l"));
            }
            let (a, b) = (&self.args[op - 1], &self.args[op + 1]);
            return Ok(match which {
                "-nt" => self.newer(a, b),
                "-ot" => self.newer(b, a),
                _ => self.same_file(a, b),
            });
        }
        // GNU compares the arguments at pos and pos + 2, which a `-l` on
        // the right has moved.
        let same = self.args[self.pos] == self.args[self.pos + 2];
        self.pos += 3;
        Ok(if which == "!=" { !same } else { same })
    }

    /// Whether `a` is newer than `b`: `a` exists and `b` does not, or is
    /// older (GNU's `-nt`; `-ot` is it turned round).
    fn newer(&mut self, a: &str, b: &str) -> bool {
        let mtime = |e: &mut Self, path| e.stat(path).map(|(_, s)| s.mtime);
        match (mtime(self, a), mtime(self, b)) {
            (Some(a), Some(b)) => a > b,
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Whether `a` and `b` are one file: one filesystem, one inode.
    fn same_file(&mut self, a: &str, b: &str) -> bool {
        match (self.stat(a), self.stat(b)) {
            (Some((a, _)), Some((b, _))) => a == b,
            _ => false,
        }
    }

    /// `path`'s node and `stat`, if it exists; a symbolic link is not
    /// followed (§10).
    fn stat(&mut self, path: &str) -> Option<(vfs::Node, vfs::Stat)> {
        let node = self.ctx.vfs.lookup(path.as_bytes()).ok()?;
        let stat = self.ctx.vfs.stat(node).ok()?;
        Some((node, stat))
    }
}

/// Whether `arg` has a unary operator's form: `-` and one byte.
fn unary_form(arg: Option<&str>) -> bool {
    arg.is_some_and(|a| a.len() == 2 && a.starts_with('-'))
}

/// GNU's binary operators.
fn binop(arg: Option<&str>) -> bool {
    matches!(
        arg,
        Some(
            "=" | "!="
                | "=="
                | "-nt"
                | "-ot"
                | "-ef"
                | "-eq"
                | "-ne"
                | "-lt"
                | "-le"
                | "-gt"
                | "-ge"
        )
    )
}

/// What an integer operator asks of the comparison.
fn integer_operator(op: &str) -> Option<fn(Ordering) -> bool> {
    Some(match op {
        "-eq" => Ordering::is_eq,
        "-ne" => Ordering::is_ne,
        "-lt" => Ordering::is_lt,
        "-le" => Ordering::is_le,
        "-gt" => Ordering::is_gt,
        "-ge" => Ordering::is_ge,
        _ => return None,
    })
}

/// An integer as GNU's `find_int` reads it: its sign and its digits
/// without leading zeros (none for zero, which has no sign).
#[derive(PartialEq, Eq)]
struct Number {
    negative: bool,
    digits: String,
}

impl Number {
    /// Blanks (spaces and tabs), then `+` or `-`, at least one digit, then
    /// blanks; GNU's `invalid integer 'X'` for anything else.
    fn parse(s: &str) -> Result<Number, String> {
        let blank = |c: char| c == ' ' || c == '\t';
        let rest = s.trim_start_matches(blank);
        let (negative, rest) = match rest.as_bytes().first() {
            Some(b'+') => (false, &rest[1..]),
            Some(b'-') => (true, &rest[1..]),
            _ => (false, rest),
        };
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let (digits, after) = rest.split_at(end);
        if digits.is_empty() || !after.trim_start_matches(blank).is_empty() {
            return Err(format!("invalid integer {}", quote(s)));
        }
        let digits = digits.trim_start_matches('0');
        Ok(Number {
            negative: negative && !digits.is_empty(),
            digits: String::from(digits),
        })
    }

    /// The fd this names, if any can be: from 0 to `int`'s largest, as
    /// GNU's `-t` takes them.
    fn fd(&self) -> Option<u32> {
        match (self.negative, self.digits.as_str()) {
            (true, _) => None,
            (false, "") => Some(0),
            (false, digits) => digits.parse().ok().filter(|&fd| fd <= i32::MAX as u32),
        }
    }

    /// `-l`'s length of `s`, in bytes.
    fn length(s: &str) -> Number {
        Number::parse(&s.len().to_string()).unwrap_or(Number {
            negative: false,
            digits: String::new(),
        })
    }
}

impl Ord for Number {
    fn cmp(&self, other: &Number) -> Ordering {
        let size = |n: &Number| (n.digits.len(), n.digits.clone());
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => size(self).cmp(&size(other)),
            (true, true) => size(other).cmp(&size(self)),
        }
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Number) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `arg` as GNU's `quote()` shows it in the C locale: in single quotes, a
/// quote or backslash escaped, control bytes by their C names, and every
/// other byte outside printable ASCII in octal (`'\303\251'` for `é`).
fn quote(arg: &str) -> String {
    let mut q = String::from("'");
    for &b in arg.as_bytes() {
        match b {
            b'\'' => q.push_str("\\'"),
            b'\\' => q.push_str("\\\\"),
            0x07 => q.push_str("\\a"),
            0x08 => q.push_str("\\b"),
            0x0c => q.push_str("\\f"),
            b'\n' => q.push_str("\\n"),
            b'\r' => q.push_str("\\r"),
            b'\t' => q.push_str("\\t"),
            0x0b => q.push_str("\\v"),
            0x20..=0x7e => q.push(char::from(b)),
            _ => q.push_str(&format!("\\{b:03o}")),
        }
    }
    q.push('\'');
    q
}

#[cfg(test)]
mod tests {
    use super::named;
    use crate::testing::{Harness, TestFile, host_files, host_tool, like_host_files, memfs};
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec::Vec;
    use vfs::{FileSystem, FileType, Vfs};

    /// Every unary operator on files, `-t` aside.
    const FILE_OPERATORS: [&str; 19] = [
        "-e", "-f", "-d", "-s", "-r", "-w", "-x", "-O", "-G", "-N", "-u", "-g", "-k", "-p", "-S",
        "-b", "-c", "-h", "-L",
    ];

    /// `op path` under both names, as GNU answers it (as root, with
    /// `files` made on the host) and as ours does.
    fn unary_like_gnu(files: &[TestFile<'_>], op: &str, path: &str) {
        for name in ["test", "["] {
            let mut line = alloc::vec![name, op, path];
            if name == "[" {
                line.push("]");
            }
            assert_eq!(
                like_host_files(&line, files),
                host_files(&line, files, true),
                "{line:?}"
            );
        }
    }

    /// `args` (without the name) under `name`, as GNU's program and as
    /// ours; for `[` the `]` is added.
    fn both(name: &str, args: &[&str]) -> ((i32, String, String), (i32, String, String)) {
        let mut line: Vec<&str> = [name].into_iter().chain(args.iter().copied()).collect();
        if name == "[" {
            line.push("]");
        }
        (
            Harness::new().like_host(&line, &[], b""),
            host_tool(&line, &[], b""),
        )
    }

    fn like_gnu(cases: &[&[&str]]) {
        for args in cases {
            for name in ["test", "["] {
                let (ours, gnu) = both(name, args);
                assert_eq!(ours, gnu, "{name} {args:?}");
            }
        }
    }

    #[test]
    fn the_rules_by_argument_count_are_gnu_s() {
        // probes/grammar.txt a0–a4h: 0 to 4 arguments.
        like_gnu(&[
            &[],
            &[""],
            &["x"],
            &["-n"],
            &["!"],
            &["("],
            &[")"],
            &["-a"],
            &["--help"],
            &["--version"],
            &["!", ""],
            &["!", "x"],
            &["-n", ""],
            &["-z", ""],
            &["-z", "x"],
            &["-q", "x"],
            &["x", "y"],
            &["-", "x"],
            &["--", "x"],
            &["(", "x"],
            &["-n", "-n"],
            &["!", "!"],
            &["!", "="],
            &["(", ")"],
            &["a", "=", "a"],
            &["a", "==", "b"],
            &["a", "!=", "b"],
            &["a", "!=", "a"],
            &["!", "-n", ""],
            &["(", "x", ")"],
            &["(", "", ")"],
            &["x", "-a", ""],
            &["", "-o", "x"],
            &["a", "b", "c"],
            &["!", "!", "x"],
            &["-n", "=", "-n"],
            &["!", "=", "!"],
            &["(", "=", ")"],
            &["a", "<", "b"],
            &["b", ">", "a"],
            &["(", "!", ")"],
            &["-a", "-a", "-a"],
            &["-o", "-o", "-o"],
            &["!", "a", "b"],
            &["!", "a", "=", "a"],
            &["(", "a", "=", ")"],
            &["(", "-n", "x", ")"],
            &["!", "!", "!", "x"],
            &["a", "=", "a", "b"],
            &["!", "(", "x", ")"],
            &["(", "a", "b", ")"],
            &["-n", "x", "-a", "y"],
            &["(", "!", "x", ")"],
            &["(", "(", "x", ")"],
        ]);
    }

    #[test]
    fn not_and_or_and_parentheses_are_gnu_s() {
        // probes/grammar.txt a5a–a5s: 5 arguments and more, GNU's
        // precedence (`-a` before `-o`) and its errors.
        like_gnu(&[
            &["", "-o", "x", "-a", ""],
            &["x", "-o", "", "-a", ""],
            &["!", "", "-a", "!", ""],
            &["(", "", "-o", "x", ")", "-a", "x"],
            &["!", "(", "x", ")", "-o", "x"],
            &["(", "x", "-a", "y"],
            &["x", "-a"],
            &["x", "-a", "y", "-o"],
            &["a", "b", "c", "d", "e"],
            &["x", ")", "y", "z", "w"],
            &["!", "!", "!", "!", "x"],
            &["-n", "x", "-a", "-z", "x"],
            &["(", "(", "x", ")", ")"],
            &["a", "=", "a", "-a", "-n"],
            &["-z", "-a", "-z", "-a", "-z"],
            &["x", "-a", "(", "y"],
            &["=", "=", "=", "-a", "x"],
            &["(", "x", ")", "-a", "(", "", ")"],
            &["(", "x", "-a", "", ")", "-o", "y"],
            &["(", "(", "(", "x", ")", ")", ")"],
            &["(", "!", "(", "x", ")", ")"],
            &["(", "a", "=", "b", "-o", "c", ")"],
            &["(", "a", "b", "c", "d", ")"],
            &["(", "x", ")", "y"],
            &["x", "-o", "(", "y", ")", "-a", "!", "z"],
            &["!", "x", "-o", "!", "", "-a", "x"],
            &["a", "-a", "b", "-a", "c", "-a", "d"],
            &["", "-o", "", "-o", "", "-o", "x"],
            &["(", "-a", ")"],
            &["(", "-o", "x", ")"],
            // Found by a fuzz against GNU: the 4-argument rule inside `(`.
            &["x", "-a", "(", "!", "=", "x", "1", ")"],
            &["(", "!", "(", "-l", "-o", ")", ""],
            // Too few arguments left for a binary operator (the review).
            &["x", "-a", "y", "="],
            &["x", "-o", "="],
        ]);
    }

    #[test]
    fn strings_compare_bytes_and_have_no_less_or_greater() {
        // probes/grammar.txt s1–s7: GNU's program has no `<` or `>`
        // (spec §15 item 3).
        like_gnu(&[
            &["é", "=", "é"],
            &["é", "=", "e"],
            &["é", "!=", "e"],
            &["-n", "é"],
            &["-z", "é"],
            &["日本"],
            &["a", "<", "b"],
            &["é", ">", "z"],
            &["a", "=", "b", "=", "c"],
            &["a b", "=", "a b"],
            &["=", "=", "="],
            &["!=", "!=", "!="],
            &["-", "=", "-"],
        ]);
    }

    #[test]
    fn integers_are_read_as_gnu_reads_them_and_have_any_length() {
        // probes/grammar.txt i1–i23: blanks around, a sign, any length.
        like_gnu(&[
            &["1", "-eq", "x"],
            &[" +12 ", "-eq", "12"],
            &["-0", "-eq", "+0"],
            &["99999999999999999999999", "-gt", "99999999999999999999998"],
            &["-99999999999999999999999", "-lt", "1"],
            &[
                "-99999999999999999999999",
                "-lt",
                "-99999999999999999999998",
            ],
            &[
                "123456789012345678901234567890",
                "-eq",
                "0123456789012345678901234567890",
            ],
            &["", "-eq", "0"],
            &[" ", "-eq", "0"],
            &["1 2", "-eq", "1"],
            &["\t1\n", "-eq", "1"],
            &["\t1\t", "-eq", "1"],
            &["0x1", "-eq", "1"],
            &["1", "-eq", "1.0"],
            &["007", "-eq", "7"],
            &["+", "-eq", "0"],
            &["-", "-eq", "0"],
            &["--1", "-eq", "1"],
            &["+-1", "-eq", "1"],
            &["١", "-eq", "1"],
            &["1", "-ne", "2"],
            &["2", "-le", "2"],
            &["3", "-le", "2"],
            &["3", "-ge", "4"],
            &["4", "-ge", "4"],
            &["-5", "-gt", "-6"],
            &["-5", "-lt", "-6"],
            &["\u{b}1", "-eq", "1"],
            &["1\r", "-eq", "1"],
            &["- 1", "-eq", "-1"],
            &["1", "-eq", "1", "-a", "2", "-lt", "1"],
            &["!", "1", "-eq", "1"],
            &["1", "-EQ", "1"],
            &["1", "-eqq", "1"],
        ]);
    }

    #[test]
    fn dash_l_stands_for_a_string_s_length() {
        like_gnu(&[
            &["-l", "abc", "-eq", "3"],
            &["-l", "", "-eq", "0"],
            &["-l", "é", "-eq", "2"],
            &["3", "-eq", "-l", "abc"],
            &["-l", "ab", "-lt", "-l", "abc"],
            &["1", "-lt", "-l"],
            &["-l", "abc", "-eq", "x"],
            &["-l", "abc", "=", "abc"],
            &["a", "=", "-l", "a"],
            &["=", "=", "-l", "b"],
            &["-l", "a", "-nt", "b"],
            &["a", "-ot", "-l", "b"],
            &["-l", "a", "-ef", "a"],
            &["-l"],
            &["-l", "x"],
            &["-l", "x", "y"],
            &["!", "-l", "abc", "-eq", "3"],
            &["-l", "abc", "-eq", "3", "-a", "x"],
            // Too few arguments left for `-l` and an operator.
            &["x", "-a", "-l", "a", "="],
        ]);
    }

    #[test]
    fn errors_quote_their_argument_as_gnu_s_quote_does() {
        // probes/quote.txt qt0–qt13: C escapes in plain quotes, other
        // bytes in octal.
        like_gnu(&[
            &["a'b", "-eq", "1"],
            &["a\\b", "-eq", "1"],
            &["a\"b", "-eq", "1"],
            &["é", "-eq", "1"],
            &["\u{7f}", "-eq", "1"],
            &["\u{1}", "-eq", "1"],
            &["\u{1b}", "-eq", "1"],
            &["\u{7}\u{8}\u{c}\r\u{b}", "-eq", "1"],
            &["a?b??=c", "-eq", "1"],
            &["€", "-eq", "1"],
            &["-é"],
            &["-é", "x"],
            &["x", "é", "y"],
            &["a", "b", "c", "d", "é"],
        ]);
    }

    #[test]
    fn parentheses_nest_as_deep_as_gnu_s_without_recursion() {
        // probes/deep.txt d30000e: GNU nests 30000 levels; this evaluator
        // keeps its open parentheses on a stack of its own, so a small
        // stack is enough.
        let mut line = alloc::vec![String::from("test")];
        for _ in 0..30_000 {
            line.extend(["(", "x", "-a"].map(String::from));
        }
        line.push(String::from("x"));
        line.extend((0..30_000).map(|_| String::from(")")));
        let args: Vec<&str> = line.iter().map(String::as_str).collect();
        let gnu = host_tool(&args, &[], b"");
        let ours = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let args: Vec<&str> = line.iter().map(String::as_str).collect();
                Harness::new().like_host(&args, &[], b"")
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!((ours, gnu.0), ((0, String::new(), String::new()), 0));
    }

    #[test]
    fn bracket_needs_its_bracket_and_refuses_help_and_version() {
        let mut h = Harness::new();
        for (line, said) in [
            ("[", "[: missing ']'\n"),
            ("[ x", "[: missing ']'\n"),
            ("[ ] x", "[: missing ']'\n"),
            ("[ x ]]", "[: missing ']'\n"),
            // GNU 9.4 prints its help or version here (probes g1); these
            // are refused as other commands refuse an option (§15 item 3).
            ("[ --help", "[: unrecognized option '--help'\n"),
            ("[ --version", "[: unrecognized option '--version'\n"),
        ] {
            assert_eq!(h.run(line), (2, String::from(said)), "{line}");
        }
        // Not alone, they are strings, as in GNU.
        like_gnu(&[&["--help"], &["--version"], &["--help", "=", "--help"]]);
        assert_eq!(h.run("test --help"), (0, String::new()));
        // GNU's `[` sees the `]` past the expression's end (probes g6).
        assert_eq!(
            h.run("[ '(' x -a y ]"),
            (2, String::from("[: ')' expected, found ']'\n"))
        );
        assert_eq!(
            h.run("test '(' x -a y"),
            (2, String::from("test: ')' expected\n"))
        );
    }

    #[test]
    fn newer_older_and_the_same_file_are_gnu_s() {
        let files = [
            TestFile::file("old", b"o").times(100, 1_000),
            TestFile::file("new", b"n").times(100, 2_000),
            TestFile::file("same", b"s").times(100, 2_000),
            TestFile::hard_link("link", "old"),
            TestFile::dir("d").times(100, 1_500),
        ];
        for args in [
            ["new", "-nt", "old"],
            ["old", "-nt", "new"],
            ["new", "-nt", "same"],
            ["new", "-nt", "nope"],
            ["nope", "-nt", "new"],
            ["nope", "-nt", "nope"],
            ["old", "-ot", "new"],
            ["new", "-ot", "old"],
            ["old", "-ot", "nope"],
            ["nope", "-ot", "old"],
            ["d", "-nt", "old"],
            ["old", "-ef", "link"],
            ["link", "-ef", "old"],
            ["old", "-ef", "old"],
            ["old", "-ef", "new"],
            ["old", "-ef", "nope"],
            ["d", "-ef", "d/."],
            ["d", "-ef", "."],
            [".", "-ef", "d/.."],
        ] {
            for name in ["test", "["] {
                let mut line = alloc::vec![name];
                line.extend(args);
                if name == "[" {
                    line.push("]");
                }
                assert_eq!(
                    like_host_files(&line, &files),
                    host_files(&line, &files, false),
                    "{line:?}"
                );
            }
        }
    }

    #[test]
    fn file_operators_answer_as_gnu_s_do_for_root() {
        // GNU's answers as root, through `unshare -r` (spec §15 item 3):
        // probe u1, `-r` and `-w` true without the bits, `-x` true for a
        // directory. Every file has its times, so that `-N` does not
        // depend on the host's clock.
        let files = [
            TestFile::file("f", b"data"),
            TestFile::file("empty", b""),
            TestFile::file("x", b"#").mode(0o100),
            TestFile::file("g", b"#").mode(0o010),
            TestFile::file("none", b"x").mode(0o000),
            TestFile::file("su", b"x").mode(0o4755),
            TestFile::file("sg", b"x").mode(0o2644),
            TestFile::file("read", b"x").times(1_000, 2_000),
            TestFile::file("unread", b"x").times(2_000, 1_000),
            TestFile::file("same", b"x").times(2_000, 2_000),
            // Each directory holds a file, so that `-s` does not depend on
            // the size the host's filesystem gives an empty one.
            TestFile::dir("d"),
            TestFile::file("d/in", b""),
            TestFile::dir("closed").mode(0o600),
            TestFile::file("closed/in", b""),
            TestFile::dir("search").mode(0o100),
            TestFile::file("search/in", b""),
            TestFile::dir("sticky").mode(0o1777),
            TestFile::file("sticky/in", b""),
            TestFile::fifo("p"),
            TestFile::socket("s"),
        ];
        let names = [
            "f", "empty", "x", "g", "none", "su", "sg", "read", "unread", "same", "d", "closed",
            "search", "sticky", "p", "s", "nope", "", "d/", "f/",
        ];
        let files = files.map(|f| match f.times {
            Some(_) => f,
            None => f.times(5_000, 5_000),
        });
        for op in FILE_OPERATORS {
            for path in names {
                unary_like_gnu(&files, op, path);
            }
        }
        // Missing their file: GNU's message.
        like_gnu(&[
            &["-e"],
            &["!", "-f"],
            &["-d", "-a", "x"],
            &["-x", "-o", "-f"],
        ]);
    }

    #[test]
    fn a_socket_is_made_however_deep_its_directory_is() {
        // A Unix socket's address holds 108 bytes: the review found the
        // harness's sockets failing in a checkout 57 characters deep.
        let deep = "d".repeat(60);
        let socket = alloc::format!("{deep}/s");
        let files = [TestFile::dir(&deep), TestFile::socket(&socket)];
        for line in [
            ["test", "-S", socket.as_str()],
            ["test", "-e", deep.as_str()],
        ] {
            assert_eq!(
                like_host_files(&line, &files),
                (0, String::new(), String::new())
            );
            assert_eq!(
                host_files(&line, &files, true),
                (0, String::new(), String::new())
            );
        }
    }

    #[test]
    fn devices_answer_as_gnu_s_do() {
        // The host's /dev/null and a block device of its own stand for
        // the harness's.
        let block = std::fs::read_dir("/dev")
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| {
                use std::os::unix::fs::FileTypeExt;
                e.file_type().is_ok_and(|t| t.is_block_device())
            })
            .expect("a block device in the host's /dev")
            .path();
        let block = block.to_str().unwrap();
        for (host, kind) in [
            ("/dev/null", FileType::CharDev),
            (block, FileType::BlockDev),
        ] {
            for op in ["-e", "-f", "-d", "-p", "-S", "-b", "-c", "-h", "-L"] {
                let mut fs = memfs();
                let root = fs.root();
                fs.special(root, b"dev", kind).unwrap();
                let mut h = Harness::on(fs);
                let line = alloc::format!("test {op} /dev");
                let gnu = host_tool(&["test", op, host], &[], b"");
                assert_eq!(h.run(&line), (gnu.0, gnu.2), "{op} {host}");
            }
        }
    }

    #[test]
    fn a_symbolic_link_is_never_followed() {
        // A decided difference (spec §6.2, §10): GNU's operators but -h
        // and -L look at what the link names; these look at the link.
        let files = [
            TestFile::file("f", b"data"),
            TestFile::dir("d"),
            TestFile::link("lf", "f"),
            TestFile::link("ld", "d"),
            TestFile::link("dangling", "nope"),
        ];
        for path in ["lf", "ld", "dangling"] {
            unary_like_gnu(&files, "-h", path);
            unary_like_gnu(&files, "-L", path);
        }
        for (op, path, ours, gnu) in [
            ("-e", "dangling", 0, 1),
            ("-f", "lf", 1, 0),
            ("-d", "ld", 1, 0),
            ("-r", "dangling", 0, 1),
            ("-x", "lf", 0, 1),
            ("-s", "dangling", 0, 1),
        ] {
            let line = ["test", op, path];
            assert_eq!(host_files(&line, &files, true).0, gnu, "{line:?}");
            assert_eq!(like_host_files(&line, &files).0, ours, "{line:?}");
        }
        let line = ["test", "lf", "-ef", "f"];
        assert_eq!(host_files(&line, &files, false).0, 0);
        assert_eq!(like_host_files(&line, &files).0, 1);
    }

    #[test]
    fn nothing_on_bin_s_filesystem_is_writable() {
        // `/bin` is read-only (system.img): a file on its filesystem is
        // not writable (spec §15 items 3 and 6).
        let mut programs = memfs();
        let root = programs.root();
        let ls = programs.create(root, b"ls").unwrap();
        programs.set_mode(ls, 0o755).unwrap();
        let mut h = Harness::new();
        h.vfs.mkdir(b"/bin").unwrap();
        h.vfs
            .mount(b"/bin", Box::new(programs.read_only()))
            .unwrap();
        for (line, status) in [
            ("test -w /bin/ls", 1),
            ("[ -w /bin ]", 1),
            ("test -w /bin/.", 1),
            ("test -w /bin/..", 0),
            ("test -w /root", 0),
            ("test -w /etc/motd", 0),
            ("test -r /bin/ls", 0),
            ("test -x /bin/ls", 0),
            ("test -f /bin/ls", 0),
            // One inode number, two filesystems.
            ("test /bin/ls -ef /etc", 1),
            ("test /bin/ls -ef /bin/ls", 0),
            ("test /bin -ef /bin/.", 0),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
        let etc = h.vfs.lookup(b"/etc").unwrap();
        assert_eq!(etc.ino, h.vfs.lookup(b"/bin/ls").unwrap().ino);
    }

    #[test]
    fn nothing_on_a_read_only_filesystem_is_writable_but_a_device() {
        // `statfs` says which filesystems are read-only (spec §15 item 6):
        // a root mounted so too, and a `/bin` mounted writable is
        // writable. As Linux's `access`, which GNU asks, a device or a
        // FIFO stays writable there.
        let mut h = Harness::new();
        let mut bin = memfs();
        let root = bin.root();
        bin.create(root, b"ls").unwrap();
        h.vfs.mkdir(b"/bin").unwrap();
        h.vfs.mount(b"/bin", Box::new(bin)).unwrap();
        for line in ["test -w /bin/ls", "[ -w /bin ]", "test -w /etc/motd"] {
            assert_eq!(h.run(line), (0, String::new()), "{line}");
        }
        let mut ro = memfs();
        let root = ro.root();
        ro.create(root, b"f").unwrap();
        ro.mkdir(root, b"d").unwrap();
        ro.symlink(root, b"l", b"f").unwrap();
        ro.special(root, b"p", FileType::Fifo).unwrap();
        ro.special(root, b"c", FileType::CharDev).unwrap();
        h.vfs = vfs::MountTable::new(Box::new(ro.read_only()));
        for (line, status) in [
            ("test -w /f", 1),
            ("test -w /d", 1),
            ("test -w /", 1),
            ("test -w /l", 1),
            ("test -w /p", 0),
            ("test -w /c", 0),
            ("test -r /f", 0),
            ("test -w /nope", 1),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
    }

    #[test]
    fn a_plain_bin_directory_is_not_read_only() {
        // `host-shell` mounts only the image's root, whose `/bin` is a
        // directory of it (the final review): every file is then writable,
        // as GNU says for root.
        let mut h = Harness::new();
        h.vfs.mkdir(b"/bin").unwrap();
        h.put("/bin/ls", b"");
        for line in [
            "test -w /etc/motd",
            "test -w /bin",
            "test -w /bin/ls",
            "[ -w / ]",
        ] {
            assert_eq!(h.run(line), (0, String::new()), "{line}");
        }
    }

    #[test]
    fn owned_by_another_is_not_owned_by_root() {
        // Relay OS runs every program as root; a file another made (on a
        // disk written elsewhere) is not its.
        let mut fs = memfs();
        let root = fs.root();
        for (name, uid, gid) in [
            (&b"theirs"[..], 1000, 0),
            (b"group", 0, 100),
            (b"mine", 0, 0),
        ] {
            let ino = fs.create(root, name).unwrap();
            fs.set_owner(ino, uid, gid).unwrap();
        }
        let mut h = Harness::on(fs);
        for (line, status) in [
            ("test -O /theirs", 1),
            ("test -G /theirs", 0),
            ("test -O /group", 0),
            ("test -G /group", 1),
            ("test -O /mine", 0),
            ("test -G /mine", 0),
            ("test -O /nope", 1),
            ("test -G /nope", 1),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
    }

    #[test]
    fn dash_t_reads_its_fd_as_gnu_reads_an_integer() {
        // The host's tool has pipes, and the harness no console: false,
        // or GNU's message.
        like_gnu(&[
            &["-t", "0"],
            &["-t", "1"],
            &["-t", "2"],
            &["-t", "-1"],
            &["-t", "x"],
            &["-t", ""],
            &["-t", "0x0"],
            &["-t", "99999999999999999999"],
            &["-t", "é"],
            &["-t"],
            &["-t", "0", "-a", "x"],
            &["!", "-t", "1"],
        ]);
        // On a console: probes/tty.txt (fds 0 and 1 on a pty).
        let mut h = Harness::new();
        h.system.terminals = alloc::vec![0, 1];
        for (fd, status) in [
            ("0", 0),
            ("1", 0),
            ("2", 1),
            ("3", 1),
            ("-0", 0),
            ("00", 0),
            ("+0", 0),
            ("' 1 '", 0),
            ("-1", 1),
            ("2147483647", 1),
            ("2147483648", 1),
            ("99999999999999999999", 1),
        ] {
            let line = alloc::format!("test -t {fd}");
            assert_eq!(h.run(&line), (status, String::new()), "{line}");
        }
        // GNU takes fds up to `int`'s largest, as an fd is.
        h.system.terminals = alloc::vec![2_147_483_647, 2_147_483_648];
        assert_eq!(h.run("[ -t 2147483647 ]"), (0, String::new()));
        assert_eq!(h.run("[ -t 2147483648 ]"), (1, String::new()));
    }

    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
        let name = |arg0: &[u8]| named(arg0).0;
        assert_eq!(name(b"["), "[");
        assert_eq!(name(b"/bin/["), "[");
        assert_eq!(name(b"../bin/["), "[");
        assert_eq!(name(b"test"), "test");
        assert_eq!(name(b"/bin/test"), "test");
        assert_eq!(name(b"[/test"), "test");
        assert_eq!(name(b"[["), "test");
        assert_eq!(name(b""), "test");
    }
}
