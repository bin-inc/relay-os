//! What a command being dropped opens and closes (programmable shell gate
//! §15 item 2). A reader that drops a command before its end goes on
//! dropping its lines until the constructs it opened are closed and no line
//! ends after `|`, `&&` or `||`, so that nothing of it runs without what
//! came before. The scan reads quotes, escapes and comments as the parser
//! does, but never fails, and takes a line a byte at a time, so that a line
//! too long to keep, or not text, still counts whole. It runs after a
//! refusal, so it reads what the parser refuses as bash reads it: `$(…)`
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`.

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

/// The words that open a construct.
const OPENERS: &[&[u8]] = &[b"if", b"while", b"until", b"for", b"select", b"case"];

/// What the lines read so far open and close.
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until` and `for` where a
    /// command name would stand counts one, and each `fi` and `done` there
    /// one less.
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
    /// The byte before, outside quotes.
    last: u8,
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
            last: b'\n',
            word: [0; KEYWORD_MAX],
            len: 0,
            in_word: false,
            command: true,
            target: false,
            closed: false,
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
        if b == b'\n' {
            // Nothing goes on past a line's end: a quote, an escape or a
            // `${` left open there is the line's error.
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
        let last = core::mem::replace(&mut self.last, b);
        if self.nested > 0 {
            match b {
                b'}' | b')' => self.nested -= 1,
                b'{' if last == b'$' => self.nested += 1,
                b'(' => self.nested += 1,
                b'\'' | b'"' | b'`' => self.quote = Some(b),
                b'\\' => self.escaped = true,
                _ => {}
            }
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
                self.nested = 1;
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
            b';' | b'&' | b'(' => self.operator(),
            b'>' | b'<' => {
                self.end_word();
                self.target = true;
            }
            _ => self.add(b),
        }
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
        self.quote = None;
        self.escaped = false;
        self.comment = false;
        self.nested = 0;
        self.last = b'\n';
        self.command = true;
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
        self.depth == 0 && !self.open
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
    fn quotes_and_escapes_end_with_their_line() {
        // As the parser reads them: a quote left open is that line's error,
        // and the next line starts afresh.
        assert_eq!(after(&["echo 'a", "if b"]), (1, false));
        assert_eq!(after(&["echo \"a", "while b"]), (1, false));
        assert_eq!(after(&["a \\", "until b"]), (1, false));
        assert_eq!(after(&["echo ${a", "if b"]), (1, false));
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
        // Left open at a line's end, they end with it as quotes do.
        assert_eq!(after(&["echo $(a", "if b"]), (1, false));
        assert_eq!(after(&["echo `a", "if b"]), (1, false));
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
