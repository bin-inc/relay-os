//! What a command being dropped opens and closes (programmable shell gate
//! §15 item 2). A reader that drops a command before its end goes on
//! dropping its lines until the constructs it opened are closed and no line
//! ends after `|`, `&&` or `||`, so that nothing of it runs without what
//! came before. The scan reads quotes, escapes and comments as the parser
//! does, but never fails, and takes a line a byte at a time, so that a line
//! too long to keep, or not text, still counts whole. It runs after a
//! refusal, so it reads what the parser refuses as bash reads it: `$(…)`
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! arithmetic (`((…))`) whole, and a here-document's body, data up to its
//! delimiter's line.

/// Where a `for` is, while its name and words are read.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum For {
    #[default]
    No,
    /// The next word is its name.
    Name,
    /// After its name: `in` or `do` may follow.
    AfterName,
}

/// The longest keyword the scan looks for, `select`.
const KEYWORD_MAX: usize = 6;

/// The most here-documents the scan keeps that one line starts; the
/// bodies of any more are read as lines.
const HEREDOCS_MAX: usize = 8;

/// How much of a here-document's delimiter, or of a line of its body, the
/// scan keeps: a line as long, starting with those bytes, ends the body.
const DELIM_MAX: usize = 64;

/// A here-document's delimiter, or a line of its body: its first bytes,
/// its length, and (a delimiter's) whether the body's leading tabs are
/// stripped (`<<-`).
#[derive(Clone, Copy)]
struct Delim {
    bytes: [u8; DELIM_MAX],
    len: usize,
    tabs: bool,
}

impl Delim {
    const fn new(tabs: bool) -> Delim {
        Delim {
            bytes: [0; DELIM_MAX],
            len: 0,
            tabs,
        }
    }

    fn push(&mut self, b: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = b;
        }
        self.len = self.len.saturating_add(1);
    }

    fn same(&self, other: &Delim) -> bool {
        let n = self.len.min(DELIM_MAX);
        self.len == other.len && self.bytes[..n] == other.bytes[..n]
    }
}

/// A here-document's delimiter being read after `<<`: its quotes are
/// removed, as bash removes them.
#[derive(Clone, Copy)]
struct Reading {
    delim: Delim,
    quote: Option<u8>,
    escaped: bool,
    started: bool,
}

/// How many levels of `${…}` and `$(…)` the scan tells apart; deeper
/// ones are taken for `$(…)`.
const LEVELS_KEPT: usize = 64;

/// The words that open a construct.
const OPENERS: &[&[u8]] = &[b"if", b"while", b"until", b"for", b"select", b"case"];

/// What the lines read so far open and close.
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until`, `for`, `select`
    /// and `case` where a command name would stand counts one, and each
    /// `fi`, `done` and `esac` there one less.
    depth: usize,
    /// What was read ends after `|`, `&&` or `||` (blank and comment lines
    /// after it leave it so).
    open: bool,
    /// The quote a word is in.
    quote: Option<u8>,
    /// After a `\`, which takes the next character.
    escaped: bool,
    comment: bool,
    /// How many `${`, `$(` and `(` inside them are open: what is in them
    /// is part of a word.
    nested: usize,
    /// Which of the open levels are `${` (a bit each, the first
    /// [`LEVELS_KEPT`]), the others `$(` or `(`.
    braces: u64,
    /// A word inside a `$(…)`: its first bytes, how many it has, whether
    /// it stands where a command name would, and how many `case`s it has
    /// opened.
    inner: [u8; 4],
    inner_len: usize,
    inner_command: bool,
    cases: usize,
    /// The byte before, outside quotes.
    last: u8,
    /// The `(` before stood where a command name would: a `(` right after
    /// it starts arithmetic.
    paren: bool,
    /// The open levels are arithmetic's (`((`), until its first `)` shows
    /// whether bash reads them so: the byte after it decides
    /// (`arithmetic_check`).
    arithmetic: bool,
    arithmetic_check: bool,
    /// What was read cannot be told apart any more: the drop goes on to the
    /// end of the input.
    lost: bool,
    /// The word being read: its first bytes, and how many it has. A quote
    /// or a `\` is one of them, so a word with one is no keyword.
    word: [u8; KEYWORD_MAX],
    len: usize,
    in_word: bool,
    /// The next word stands where a command name would.
    command: bool,
    /// The next word is a redirection's target.
    target: bool,
    /// The word before was a closer (`fi`, `done`, `esac`, `}`), after
    /// which an opener is bash's error, no construct.
    closed: bool,
    /// How many unquoted `<` came last.
    lt: usize,
    /// The delimiter being read after `<<`.
    reading: Option<Reading>,
    /// The here-documents the line started, and the one whose body is
    /// being read, if one is.
    heredocs: [Delim; HEREDOCS_MAX],
    pending: usize,
    body: Option<usize>,
    /// The body's line being read.
    body_line: Delim,
    for_: For,
}

impl Scan {
    pub fn new() -> Scan {
        Scan {
            depth: 0,
            open: false,
            quote: None,
            escaped: false,
            comment: false,
            nested: 0,
            braces: 0,
            inner: [0; 4],
            inner_len: 0,
            inner_command: true,
            cases: 0,
            last: b'\n',
            paren: false,
            arithmetic: false,
            arithmetic_check: false,
            lost: false,
            word: [0; KEYWORD_MAX],
            len: 0,
            in_word: false,
            command: true,
            target: false,
            closed: false,
            lt: 0,
            reading: None,
            heredocs: [Delim::new(false); HEREDOCS_MAX],
            pending: 0,
            body: None,
            body_line: Delim::new(false),
            for_: For::No,
        }
    }

    /// Reads some bytes of a line.
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    fn byte(&mut self, b: u8) {
        if self.body.is_some() {
            self.body_byte(b);
            return;
        }
        if self.reading.is_some() && self.delim_byte(b) {
            return;
        }
        // An arithmetic `((` whose first `)` is not followed by another is
        // two subshells, as bash reads it, which the scan read as words:
        // a here-document or a keyword in them went unseen.
        if core::mem::take(&mut self.arithmetic_check) && b != b')' {
            self.arithmetic = false;
            self.lost = true;
        }
        if b == b'\n' {
            // As bash reads on (the final review): a `\` before it joins
            // the lines, a quote and a `$(…)` or `${…}` go on past it.
            if core::mem::take(&mut self.escaped) || self.quote.is_some() {
                return;
            }
            if self.nested > 0 {
                self.inner_end();
                self.inner_command = true;
                return;
            }
            self.end_word();
            self.newline();
            return;
        }
        if self.comment {
            return;
        }
        // What is escaped or quoted is part of a word that is no keyword.
        if self.escaped {
            self.escaped = false;
            return;
        }
        if let Some(q) = self.quote {
            match b {
                _ if b == q => self.quote = None,
                b'\\' if q != b'\'' => self.escaped = true,
                _ => {}
            }
            return;
        }
        // `<<` and `<<-` start a here-document, whose delimiter follows;
        // `<<<` does not.
        if self.lt > 0 && b != b'<' && core::mem::take(&mut self.lt) == 2 {
            self.target = false;
            self.reading = Some(Reading {
                delim: Delim::new(b == b'-'),
                quote: None,
                escaped: false,
                started: false,
            });
            if b == b'-' || self.delim_byte(b) {
                return;
            }
        }
        let last = core::mem::replace(&mut self.last, b);
        if self.nested > 0 {
            self.nested_byte(b, last);
            return;
        }
        match b {
            b' ' | b'\t' => self.end_word(),
            b'\'' | b'"' | b'`' => {
                self.add(b);
                self.quote = Some(b);
            }
            b'\\' => {
                self.add(b);
                self.escaped = true;
            }
            b'{' | b'(' if last == b'$' && self.in_word => {
                self.add(b);
                self.open_level(b == b'{');
            }
            b'#' if !self.in_word => self.comment = true,
            // `&&` and `||` go on to the next line, as `|` does.
            b'&' if last == b'&' => self.open = true,
            b'|' => {
                self.operator();
                self.open = true;
            }
            // The word before a `)` is a `case` pattern, no keyword.
            b')' => {
                self.command = false;
                self.operator();
            }
            // `((` where a command name stands is bash's arithmetic
            // command, read to its `))` as `$((…))` is: a `<<` in it is a
            // shift, no here-document. After it a keyword may follow, as
            // after `fi`.
            b'(' if last == b'(' && self.paren => {
                self.open_level(false);
                self.open_level(false);
                self.closed = true;
                self.arithmetic = true;
            }
            b'(' => {
                // A keyword before it (`for((`, `!((`) ends here.
                self.end_word();
                self.paren = self.command;
                self.operator();
            }
            b';' | b'&' => self.operator(),
            b'>' => {
                self.end_word();
                self.target = true;
            }
            b'<' => {
                self.end_word();
                self.target = true;
                self.lt += 1;
            }
            _ => self.add(b),
        }
    }

    /// A byte of a here-document's delimiter, after `<<`: whether it was
    /// one, or ended the delimiter and is the line's again.
    fn delim_byte(&mut self, b: u8) -> bool {
        let Some(r) = &mut self.reading else {
            return false;
        };
        if core::mem::take(&mut r.escaped) {
            r.delim.push(b);
            return true;
        }
        if let Some(q) = r.quote {
            match b {
                b'\n' => {}
                _ if b == q => r.quote = None,
                b'\\' if q == b'"' => r.escaped = true,
                _ => r.delim.push(b),
            }
            if b != b'\n' {
                return true;
            }
        }
        match b {
            b' ' | b'\t' if !r.started => return true,
            b'\'' | b'"' => r.quote = Some(b),
            b'\\' => r.escaped = true,
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => {
                let delim = r.delim;
                let started = r.started;
                self.reading = None;
                if started && let Some(slot) = self.heredocs.get_mut(self.pending) {
                    *slot = delim;
                    self.pending += 1;
                }
                return false;
            }
            _ => r.delim.push(b),
        }
        r.started = true;
        true
    }

    /// A byte of a here-document's body: a line equal to its delimiter
    /// (its leading tabs stripped after `<<-`) ends it, and the next one's
    /// starts.
    fn body_byte(&mut self, b: u8) {
        let Some(i) = self.body else {
            return;
        };
        let Some(delim) = self.heredocs.get(i).copied() else {
            self.body = None;
            return;
        };
        if b == b'\n' {
            if self.body_line.same(&delim) {
                self.body = Some(i + 1).filter(|&n| n < self.pending);
                if self.body.is_none() {
                    self.pending = 0;
                }
            }
            self.body_line = Delim::new(false);
            return;
        }
        if self.body_line.len == 0 && delim.tabs && b == b'\t' {
            return;
        }
        self.body_line.push(b);
    }

    /// Adds a byte to the word being read.
    fn add(&mut self, b: u8) {
        if !self.in_word {
            self.in_word = true;
            self.len = 0;
        }
        if let Some(slot) = self.word.get_mut(self.len) {
            *slot = b;
        }
        self.len = self.len.saturating_add(1);
    }

    /// `;`, `&`, `|`, `(` or `)`: the next word stands where a command
    /// name would, and a `for`'s words end.
    fn operator(&mut self) {
        self.end_word();
        self.open = false;
        self.command = true;
        self.target = false;
        self.closed = false;
        self.for_ = For::No;
    }

    fn newline(&mut self) {
        self.comment = false;
        self.last = b'\n';
        self.lt = 0;
        self.command = true;
        // The here-documents the line started take the next lines.
        if self.pending > 0 {
            self.body = Some(0);
            self.body_line = Delim::new(false);
        }
        // A redirection with no target is the line's error: its command
        // goes on no further, as bash's does not.
        if core::mem::take(&mut self.target) {
            self.open = false;
        }
        self.closed = false;
        // A `for`'s words end with their line; an `in` on a later line is
        // a word that makes the rest of its line arguments, as the
        // parser's `in` would.
        self.for_ = For::No;
    }

    /// The word being read ends: a keyword where one may stand opens or
    /// closes a construct.
    fn end_word(&mut self) {
        if !core::mem::take(&mut self.in_word) {
            return;
        }
        self.open = false;
        // After a redirection, as in the parser, a word is a command's
        // name, never a keyword.
        if core::mem::take(&mut self.target) {
            self.command = false;
            return;
        }
        let word = self.word.get(..self.len).unwrap_or(b"");
        match self.for_ {
            For::Name => {
                self.for_ = For::AfterName;
                return;
            }
            // After `in` its words, up to a `;` or a newline, are no
            // command's name; after `do` one stands.
            For::AfterName => {
                self.for_ = For::No;
                self.command = word == b"do";
                return;
            }
            For::No => {}
        }
        if !self.command {
            return;
        }
        let closed = core::mem::take(&mut self.closed);
        match word {
            _ if closed && OPENERS.contains(&word) => self.command = false,
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
            b"for" | b"select" => {
                self.depth = self.depth.saturating_add(1);
                self.for_ = For::Name;
            }
            // Its word and `in` are no command's name; a pattern is
            // followed by `)`.
            b"case" => {
                self.depth = self.depth.saturating_add(1);
                self.command = false;
            }
            b"fi" | b"done" | b"esac" => {
                self.depth = self.depth.saturating_sub(1);
                self.closed = true;
            }
            b"}" => self.closed = true,
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" | b"{" => {}
            _ => self.command = false,
        }
    }

    /// Reads a whole line and its newline.
    pub fn line(&mut self, line: &[u8]) {
        self.bytes(line);
        self.bytes(b"\n");
    }

    /// Everything read is closed: no construct is open and the last line
    /// does not end after `|`, `&&` or `||`.
    pub fn done(&self) -> bool {
        self.depth == 0
            && !self.lost
            && !self.open
            && self.body.is_none()
            && self.quote.is_none()
            && !self.escaped
            && self.nested == 0
    }

    /// A `${` (`brace`) or a `$(` or `(` inside a `$(…)` opens a level of
    /// a word's expansion.
    fn open_level(&mut self, brace: bool) {
        if self.nested < LEVELS_KEPT {
            let bit = 1u64 << self.nested;
            self.braces = if brace {
                self.braces | bit
            } else {
                self.braces & !bit
            };
        }
        self.nested = self.nested.saturating_add(1);
        self.inner_end();
        self.inner_command = true;
    }

    /// Whether the innermost open level is a `${`.
    fn in_braces(&self) -> bool {
        self.nested <= LEVELS_KEPT && self.braces >> (self.nested - 1) & 1 == 1
    }

    /// A byte inside a `${…}` or `$(…)`, part of a word: only the levels
    /// count, and in a `$(…)` a `case`, whose patterns' `)` close nothing.
    fn nested_byte(&mut self, b: u8, last: u8) {
        match b {
            b'\'' | b'"' | b'`' => {
                self.inner_dirty();
                self.quote = Some(b);
            }
            b'\\' => {
                self.inner_dirty();
                self.escaped = true;
            }
            b'{' if last == b'$' => self.open_level(true),
            b'}' if self.in_braces() => {
                self.inner_end();
                self.nested -= 1;
            }
            _ if self.in_braces() => {}
            b'(' => self.open_level(false),
            b')' => {
                self.inner_end();
                if self.cases > 0 {
                    // A pattern's: the commands of its branch follow.
                    self.inner_command = true;
                } else {
                    self.nested -= 1;
                }
                if self.arithmetic {
                    self.arithmetic_check = self.nested == 1;
                    self.arithmetic = self.nested > 0;
                }
            }
            b' ' | b'\t' => self.inner_end(),
            b';' | b'&' | b'|' => {
                self.inner_end();
                self.inner_command = true;
            }
            _ => {
                if let Some(slot) = self.inner.get_mut(self.inner_len) {
                    *slot = b;
                }
                self.inner_len = self.inner_len.saturating_add(1);
            }
        }
    }

    /// The word inside a `$(…)` has a quote or an escape: it is no keyword.
    fn inner_dirty(&mut self) {
        self.inner_len = usize::MAX;
    }

    /// A word inside a `$(…)` ends: a `case` or an `esac` where a command
    /// name would stand counts.
    fn inner_end(&mut self) {
        let len = core::mem::take(&mut self.inner_len);
        if len == 0 {
            return;
        }
        let word = self.inner.get(..len).unwrap_or(b"");
        if self.inner_command && word == b"case" {
            self.cases = self.cases.saturating_add(1);
        } else if self.inner_command && word == b"esac" {
            self.cases = self.cases.saturating_sub(1);
        }
        self.inner_command = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many constructs `lines`, each with its newline, leave open, and
    /// whether the last ends after `|`, `&&` or `||`.
    fn after(lines: &[&str]) -> (usize, bool) {
        let mut s = Scan::new();
        for l in lines {
            s.line(l.as_bytes());
        }
        (s.depth, s.open)
    }

    #[test]
    fn openers_and_closers_count_where_a_command_name_stands() {
        for (lines, depth) in [
            (&["if a; then b; fi"][..], 0),
            (&["if a; then"], 1),
            (&["while a", "do if b", "then c"], 2),
            (&["until a; do b; done; for x in y; do"], 1),
            (&["a && while b; do"], 1),
            (&["a || until b"], 1),
            (&["a & if b"], 1),
            (&["! if a; then"], 1),
            (&["if a; then b; else if c; then"], 2),
            (&["if a; then b; elif c; then while d"], 2),
            (&["if a; then b; elif while c; do d; done; then"], 1),
            // After `fi` or `done` a keyword still stands there.
            (&["if a; then if b; then c; fi fi"], 0),
            (&["while a; do if b; then c; fi done"], 0),
            // A closer with nothing open counts nothing.
            (&["done", "fi", "if a; then"], 1),
            // Blanks and tabs around them.
            (&["\tif  a ;then"], 1),
            // A `#` inside a word, a redirection without its target, and a
            // comment, end nothing on the next line.
            (&["echo a#b; if c"], 1),
            (&["a >; if b"], 1),
            (&["a >", "if b"], 1),
            (&["# c", "if a"], 1),
            (&["echo a", "if b"], 1),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
    }

    #[test]
    fn a_keyword_elsewhere_or_quoted_counts_nothing() {
        for line in [
            "echo if while until for",
            "echo a; echo fi done",
            "'if' a",
            "\"while\" a",
            "\\if a",
            "i\\f a",
            "if'' a",
            "ifx a",
            "x=if",
            "> if a",
            "> f while a",
            "a > while",
            "a >> until",
            "echo a # if",
            "#if a",
            "echo ${if}",
            "echo \"$a\" if",
        ] {
            assert_eq!(after(&[line]), (0, false), "{line}");
        }
    }

    #[test]
    fn a_for_s_name_and_words_are_no_keywords() {
        for (lines, depth) in [
            (&["for x in if while until; do"][..], 1),
            (&["for in in in; do"], 1),
            (&["for done in a; do"], 1),
            (&["for if do"], 1),
            (&["for x do"], 1),
            (&["for x do if a"], 2),
            (&["for x", "do"], 1),
            (&["for x", "in a b", "do"], 1),
            (&["for x in a b do done", "done"], 0),
            (&["for x; do echo $x; done"], 0),
            (&["for x in a; do for y in b; do"], 2),
            // A `for` alone, which the parser refuses, still opens one:
            // dropped to its `done`, to be safe.
            (&["for"], 1),
            (&["for", "x in a; do b", "done"], 0),
            // A word after the name that is neither makes the rest words.
            (&["for x a if b"], 1),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
    }

    #[test]
    fn a_line_ending_after_a_pipe_or_connector_leaves_it_open() {
        for (lines, open) in [
            (&["a |"][..], true),
            (&["a &&"], true),
            (&["a ||"], true),
            (&["a | # c"], true),
            (&["a |", "", "  # c"], true),
            (&["if a; then b &&"], true),
            (&["a |", "b"], false),
            (&["a |& b"], false),
            (&["a |&"], false),
            (&["a ||b"], false),
            (&["a && b"], false),
            (&["a \\|"], false),
            (&["echo '&&'"], false),
            (&["echo \"a ||\""], false),
            (&["a # &&"], false),
            (&["a &"], false),
            (&["a ;"], false),
        ] {
            assert_eq!(after(lines).1, open, "{lines:?}");
        }
    }

    #[test]
    fn quotes_and_escapes_go_on_across_lines_as_bash_s_do() {
        // The final review: the parser refuses a quote left open at a
        // line's end, but bash reads on, so a `fi` in the string's next
        // line is no closer; nor is one after a `\` that joins the lines.
        assert_eq!(done_after(&["echo 'a", "if b'", "c"]), [false, true, true]);
        assert_eq!(
            done_after(&["echo \"a", "while b\"", "c"]),
            [false, true, true]
        );
        assert_eq!(done_after(&["echo `a", "fi`"]), [false, true]);
        assert_eq!(after(&["a \\", "until b"]), (0, false));
        assert_eq!(after(&["echo x \\", " if y"]), (0, false));
        assert_eq!(after(&["echo \"a", "b\" if c"]), (0, false));
        assert_eq!(done_after(&["echo ${a", "if b}"]), [false, true]);
        assert_eq!(after(&["if a; then", "echo \"x", "fi y\""]), (1, false));
        // Inside a quote, a `"`'s `\` takes the next character.
        assert_eq!(after(&["echo \"a\\\" if\" b; if c"]), (1, false));
        // `${…}` is read whole, its quotes, escapes and `${…}` too.
        assert_eq!(after(&["echo ${a:-'}'} ; if b"]), (1, false));
        assert_eq!(after(&["echo ${x;if}; if c"]), (1, false));
        assert_eq!(after(&["echo ${a:-${b}; if x}; if c"]), (1, false));
        assert_eq!(after(&["echo ${a:-\\}; if x}; if c"]), (1, false));
    }

    #[test]
    fn refused_syntax_is_read_as_bash_reads_it() {
        // The prototype's review: the scan runs after a refusal, so it
        // must count across what was refused as bash would.
        for (lines, depth) in [
            // A command name stands after `time`, `{` and `}`.
            (&["time if a; then"][..], 1),
            (&["{ while a; do"], 1),
            (&["while a; do { b; } done"], 0),
            // `select` opens as `for` does, `case` as `esac` closes.
            (&["select x in if; do"], 1),
            (&["select x in a; do b; done"], 0),
            (&["case a in"], 1),
            (&["case done in"], 1),
            (&["case a in", "b) c;;", "esac"], 0),
            // A word before `)` is a `case` pattern.
            (&["case a in", "done) b;;", "fi) c;;"], 1),
            // `$(…)`, `$((…))` and backquotes are read whole.
            (&["echo $(if a; then b; fi) done"], 0),
            (&["echo $(a; fi) done; if b"], 1),
            (&["echo $((1 + (2))) fi; while a"], 1),
            (&["echo $(a (b) ; if c) x; while d"], 1),
            (&["echo $(a `)` b) fi; if c"], 1),
            (&["echo ` if a; then b; fi `; until c"], 1),
            (&["echo `a \\` fi` b; for x in y; do"], 1),
            (&["echo \"$(a) `b`\" done"], 0),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
        // `$(…)` goes on across lines too; a `case` in it has patterns
        // whose `)` closes nothing, and a `(` in `${…}` opens nothing.
        assert_eq!(done_after(&["echo $(a", "if b)"]), [false, true]);
        assert_eq!(done_after(&["echo $(echo case x in x) y)"]), [true]);
        assert_eq!(done_after(&["echo $(a }", "b)"]), [false, true]);
        for (lines, depth) in [
            (&["if a; then", "v=$(case x in x) a;; esac; b)"][..], 1),
            (&["if a; then", "v=$(case x in", "x) a;;", "esac", ")"], 1),
            (&["if a; then", "v=$(echo case x in x) y)"], 1),
            (
                &[
                    "if a; then",
                    "v=$(echo a",
                    "case x in",
                    "x) b;;",
                    "esac",
                    ")",
                ],
                1,
            ),
            (&["echo ${s//(/x}; if b"], 1),
            (&["echo ${s//)/x}; if b"], 1),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
    }

    #[test]
    fn a_line_bash_ends_in_error_leaves_nothing_open() {
        // The prototype's review: bash runs the next line after these, so
        // the scan drops nothing more.
        // A redirection with no target ends the line's command.
        assert_eq!(after(&["a && >"]), (0, false));
        assert_eq!(after(&["a | >>"]), (0, false));
        // An opener right after a closer is an error, no construct (fiif,
        // brace_if); a closer there still closes.
        for line in [
            "if a; then b; fi if c; then",
            "while a; do b; done while c; do",
            "{ a; } if b; then",
            "case a in b) c;; esac for x in y; do",
        ] {
            assert_eq!(after(&[line]), (0, false), "{line}");
        }
        assert_eq!(after(&["if a; then if b; then c; fi fi"]), (0, false));
    }

    /// Whether the scan is done after each of `lines`.
    fn done_after(lines: &[&str]) -> Vec<bool> {
        let mut s = Scan::new();
        lines
            .iter()
            .map(|l| {
                s.line(l.as_bytes());
                s.done()
            })
            .collect()
    }

    #[test]
    fn a_here_document_s_body_is_read_to_its_delimiter() {
        // The prototype's review: `<<` is refused, and its body ran as
        // commands. The body is data up to its delimiter's line.
        assert_eq!(
            done_after(&["cat <<EOF", "if a", "fi", "done", "EOF", "b"]),
            [false, false, false, false, true, true]
        );
        // `<<-` strips leading tabs; the word's quotes are removed; a blank
        // before the word is allowed, before the delimiter's line not.
        assert_eq!(
            done_after(&["cat <<-EOF", "\tif a", "\tEOF"]),
            [false, false, true]
        );
        for (head, end) in [
            ("cat <<'E F'", "E F"),
            ("cat <<E\"O\"F", "EOF"),
            ("cat <<\\EOF", "EOF"),
            ("cat << EOF", "EOF"),
            ("a<<EOF", "EOF"),
        ] {
            assert_eq!(
                done_after(&[head, "x", end]),
                [false, false, true],
                "{head}"
            );
        }
        assert_eq!(
            done_after(&["cat <<EOF", " EOF", "EOF"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["cat <<EOF", "\tEOF", "EOF"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["cat <<E\\ F", "E", "E F"]),
            [false, false, true]
        );
        // No word after it, or a `<` on the next line, is no here-document.
        assert_eq!(done_after(&["a <<", "b"]), [true, true]);
        assert_eq!(done_after(&["a << ;", "b"]), [true, true]);
        assert_eq!(done_after(&["a <", "<b", "c"]), [true, true, true]);
        // Several on one line take their bodies in turn.
        assert_eq!(
            done_after(&["cat <<A <<B", "B", "A", "A", "B"]),
            [false, false, false, false, true]
        );
        // `<<<` has no body, nor a quoted `<<`.
        assert_eq!(
            done_after(&["cat <<< if", "echo '<<EOF'", "a \\<<EOF"]),
            [true, true, true]
        );
        // Inside a construct, its `done` in the body closes nothing.
        assert_eq!(
            done_after(&["while a; do", "cat <<EOF", "done", "EOF", "done"]),
            [false, false, false, false, true]
        );
        // A delimiter longer than the scan keeps whole still ends it.
        let long = "x".repeat(100);
        let head = alloc::format!("cat <<{long}");
        let other = alloc::format!("{long}y");
        assert_eq!(done_after(&[&head, &other, &long]), [false, false, true]);
    }

    #[test]
    fn arithmetic_holds_no_here_document() {
        // Plan 2's deferred minor: `((` where a command name stands is
        // bash's arithmetic command, read to its `))` as `$((…))` is, so a
        // `<<` in it is a shift.
        assert_eq!(done_after(&["(( x = 1 << 2 ))", "b"]), [true, true]);
        assert_eq!(done_after(&["echo $(( 1 << 2 ))", "b"]), [true, true]);
        assert_eq!(done_after(&["a && (( (1) << 2 ))", "b"]), [true, true]);
        assert_eq!(
            done_after(&["if (( 1 << 2 )); then", "b", "fi"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["for ((i = 0; i << 1; i++))", "do b", "done"]),
            [false, false, true]
        );
        // It may go on across lines, as bash's does.
        assert_eq!(done_after(&["((", "1 << 2 ))", "b"]), [false, true, true]);
        // After its `))` a keyword may follow, as after `fi`, and an
        // opener is bash's error.
        assert_eq!(done_after(&["if a; then (( 1 )) fi", "b"]), [true, true]);
        assert_eq!(done_after(&["(( 1 )) if a", "b"]), [true, true]);
        assert_eq!(done_after(&["while (( 0 )) do b; done", "c"]), [true, true]);
        // Two `(` apart, after a word or where no command name stands, are
        // no arithmetic.
        assert_eq!(
            done_after(&["( (cat <<EOF", "EOF", "b"]),
            [false, true, true]
        );
        assert_eq!(done_after(&["a((b <<EOF", "EOF", "c"]), [false, true, true]);
        // Right after a keyword, with no blank, too (the prototype's
        // review).
        assert_eq!(
            done_after(&["for((i = 0; i << 1; i++)); do", "b", "done", "c"]),
            [false, false, true, true]
        );
        assert_eq!(
            done_after(&["if((1 << 2)); then", "b", "fi"]),
            [false, false, true]
        );
        assert_eq!(done_after(&["!((x = 1 << 2))", "b"]), [true, true]);
        // A `((` whose first `)` is not followed by another is two
        // subshells, as bash reads it (`((cat <<EOF)`), which the scan
        // cannot read again: the drop goes on to the end (the prototype's
        // review).
        assert_eq!(
            done_after(&["((cat <<EOF)", ")", "ran", "EOF", ")", "b"]),
            [false; 6]
        );
        assert_eq!(done_after(&["((a) << 1)", "b", "1", "c"]), [false; 4]);
        assert_eq!(done_after(&["((a) )", "b"]), [false, false]);
        // After its `))`, a `$(…)` nested in a word is a word's again.
        assert_eq!(done_after(&["(( 1 )); echo $($(a) b)", "c"]), [true, true]);
        assert_eq!(
            done_after(&["echo ((b <<EOF", "EOF", "c"]),
            [false, true, true]
        );
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
        let mut s = Scan::new();
        s.bytes(b"wh");
        s.bytes(b"ile a; do");
        s.bytes(b" \xff\xfe |\n");
        assert_eq!((s.depth, s.open), (1, true));
        assert!(!s.done());
        s.line(b"b; done");
        assert!(s.done());
    }
}
