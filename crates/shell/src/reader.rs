//! The lines a command is read from (programmable shell gate §4.3): a line
//! that leaves a command unfinished (`a &&`, `a |`) is kept until the lines
//! after it finish it. The prompt, `sh FILE` and `X | sh` all read through
//! one.

use crate::parser::{COMMAND_MAX, List, ParseError, Parser, Word};
use crate::scan::Scan;

/// What has been read of the command so far.
#[derive(Default)]
pub(crate) struct Reader {
    /// What has been read of the command, kept between its lines.
    parser: Parser,
    /// A command was dropped before its end: what its lines open and
    /// close, while its later lines are dropped too, up to the one that
    /// closes what it opened (a construct's `fi` or `done`, or the line
    /// that finishes an `&&` chain), so that none of them runs without what
    /// came before.
    dropping: Option<Scan>,
}

impl Reader {
    pub fn new() -> Reader {
        Reader::default()
    }

    /// Adds a line: the command it finishes, none while the command needs
    /// more lines, or why the lines do not parse, which drops them. A
    /// command longer than [`COMMAND_MAX`] is dropped too.
    pub fn add(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        if let Some(scan) = &mut self.dropping {
            scan.line(line.as_bytes());
            if scan.done() {
                self.dropping = None;
            }
            return Ok(None);
        }
        if self.parser.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line.as_bytes());
            return Err(ParseError::TooLong);
        }
        // The parser keeps what it has read, so each line is read once, in
        // its context.
        self.parser.line(line).inspect_err(|_| {
            // A line that does not parse drops the rest of its command
            // with it: what has been read, the line too, says how far.
            let scan = self.drop_start();
            if !scan.done() {
                self.dropping = Some(scan);
            }
        })
    }

    /// Drops the command a line that cannot be added is part of (too
    /// long, or no text), and the lines after it up to its end.
    pub fn drop_line(&mut self, line: &[u8]) {
        self.drop_bytes(line);
        self.drop_end();
    }

    /// Drops the command the line being read is part of, `bytes` being
    /// more of the line: so a line too long to keep is counted whole.
    pub fn drop_bytes(&mut self, bytes: &[u8]) {
        let mut scan = self.drop_start();
        scan.bytes(bytes);
        self.dropping = Some(scan);
    }

    /// The line [`Reader::drop_bytes`] took ends.
    pub fn drop_end(&mut self) {
        if let Some(scan) = &mut self.dropping {
            scan.bytes(b"\n");
            if scan.done() {
                self.dropping = None;
            }
        }
    }

    /// What the command being dropped opens and closes: as far as it has
    /// been read, the lines before this one first.
    fn drop_start(&mut self) -> Scan {
        let scan = self.dropping.take().unwrap_or_else(|| {
            let mut scan = Scan::new();
            scan.bytes(self.parser.text().as_bytes());
            scan
        });
        self.parser = Parser::new();
        scan
    }

    /// Part of a command has been read, or is being dropped.
    pub fn reading(&self) -> bool {
        !self.parser.is_empty() || self.dropping.is_some()
    }

    /// The input ended: a command read only in part is bash's `unexpected
    /// end of file`, and is dropped; one being dropped was already told.
    pub fn end(&mut self) -> Option<ParseError> {
        let reading = !self.parser.is_empty();
        self.clear();
        reading.then_some(ParseError::Incomplete)
    }

    /// Drops what has been read (Ctrl-C at the `> ` prompt).
    pub fn clear(&mut self) {
        self.parser = Parser::new();
        self.dropping = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first command's name of each pipeline of `list`.
    fn names(list: &List<Word>) -> alloc::vec::Vec<&str> {
        let mut names = alloc::vec::Vec::new();
        for item in &list.items {
            names.push(item.and_or.first.commands()[0].words[0].typed.as_str());
            for (_, p) in &item.and_or.rest {
                names.push(p.commands()[0].words[0].typed.as_str());
            }
        }
        names
    }

    #[test]
    fn a_line_that_finishes_a_command_gives_it() {
        let mut r = Reader::new();
        assert_eq!(
            names(&r.add("echo a; echo b").unwrap().unwrap()),
            ["echo", "echo"]
        );
        assert!(!r.reading());
        // Blank and comment lines finish a command of nothing.
        assert_eq!(r.add("  # c").unwrap().unwrap().items, []);
    }

    #[test]
    fn a_command_goes_on_until_a_line_finishes_it() {
        let mut r = Reader::new();
        assert_eq!(r.add("a &&"), Ok(None));
        assert!(r.reading());
        assert_eq!(r.add(""), Ok(None));
        assert_eq!(r.add("# c"), Ok(None));
        assert_eq!(r.add("b |"), Ok(None));
        assert_eq!(names(&r.add("c").unwrap().unwrap()), ["a", "b"]);
        assert!(!r.reading());
    }

    #[test]
    fn lines_that_do_not_parse_are_dropped() {
        let mut r = Reader::new();
        assert_eq!(r.add("a &&"), Ok(None));
        assert_eq!(r.add("|| b"), Err(ParseError::MissingTarget("||")));
        assert!(!r.reading());
        assert_eq!(names(&r.add("c").unwrap().unwrap()), ["c"]);
    }

    #[test]
    fn a_command_holds_at_most_64_kib() {
        let mut r = Reader::new();
        let half = alloc::format!("{} &&", "x".repeat(COMMAND_MAX / 2));
        assert_eq!(r.add(&half), Ok(None));
        assert_eq!(r.add(&half), Err(ParseError::TooLong));
        assert!(r.reading(), "dropped, up to its end");
        assert_eq!(r.add("x"), Ok(None));
        assert!(!r.reading());
        assert_eq!(
            ParseError::TooLong.to_string(),
            "the command would be longer than 64 KiB"
        );
        // Up to the limit, newlines counted, it is read.
        let fits = "x".repeat(COMMAND_MAX - 1);
        assert_eq!(r.add(&fits).unwrap().unwrap().items.len(), 1);
    }

    #[test]
    fn each_line_is_read_once() {
        // Plan 1's review found each line parsing all the text before it:
        // 64 KiB of `a |` lines took over a minute. The parser keeps what
        // it has read, so each line is read once.
        let mut r = Reader::new();
        for _ in 0..1000 {
            assert_eq!(r.add("a |"), Ok(None));
            assert_eq!(r.add(""), Ok(None));
            assert_eq!(r.add("  # c"), Ok(None));
        }
        assert_eq!(r.parser.read, 1000 * ("a |\n\n  # c\n".len()));
        assert_eq!(names(&r.add("b").unwrap().unwrap()).len(), 1);
    }

    #[test]
    fn an_error_is_told_on_the_line_that_has_it() {
        // Plan 1 judged a line alone, and told these a line late or as
        // the end of the file.
        let mut r = Reader::new();
        assert_eq!(r.add("a |"), Ok(None));
        assert_eq!(r.add("! b |"), Err(ParseError::MissingTarget("!")));
        let mut r = Reader::new();
        assert_eq!(r.add("a &&"), Ok(None));
        assert_eq!(
            r.add("b & c &&"),
            Err(ParseError::Unsupported("& after &&".into()))
        );
    }

    #[test]
    fn a_command_too_long_is_dropped_to_its_end() {
        // The review found the rest of a dropped `&&` chain run without
        // its guard: the lines after the limit, up to one that finishes the
        // command, are dropped too.
        let mut r = Reader::new();
        let half = alloc::format!("{} &&", "x".repeat(COMMAND_MAX / 2));
        assert_eq!(r.add("false &&"), Ok(None));
        assert_eq!(r.add(&half), Ok(None));
        assert_eq!(r.add(&half), Err(ParseError::TooLong));
        assert!(r.reading(), "still inside the dropped command");
        assert_eq!(r.add("true &&"), Ok(None));
        assert_eq!(r.add(""), Ok(None));
        assert_eq!(r.add("echo ran"), Ok(None), "its last line, dropped");
        assert!(!r.reading());
        assert_eq!(names(&r.add("echo next").unwrap().unwrap()), ["echo"]);
        // A line that finishes the command it makes too long ends it.
        let long = "x".repeat(COMMAND_MAX / 2);
        assert_eq!(r.add(&half), Ok(None));
        assert_eq!(r.add(&long), Err(ParseError::TooLong));
        assert!(!r.reading());
        // The input's end, or Ctrl-C, while one is dropped says no more.
        r.add(&half).unwrap();
        r.add(&half).unwrap_err();
        assert_eq!(r.end(), None);
        r.add(&half).unwrap();
        r.add(&half).unwrap_err();
        r.clear();
        assert!(!r.reading());
    }

    #[test]
    fn a_line_that_cannot_be_read_drops_its_command_to_its_end() {
        let mut r = Reader::new();
        // Its bytes decide, all of them: one that finishes a command ends
        // it; one that goes on does not.
        r.add("a &&").unwrap();
        r.drop_line(b"x");
        assert!(!r.reading());
        r.drop_line(b"x \xff &&");
        assert!(r.reading());
        r.add("b").unwrap();
        assert!(!r.reading());
        // A quote goes on to the line that closes it, as bash reads it.
        r.drop_line(b"x' &&");
        assert!(r.reading());
        assert_eq!(r.add("y'"), Ok(None));
        assert!(!r.reading());
        // A blank one ends nothing, and starts nothing.
        r.add("a ||").unwrap();
        r.drop_line(b"   ");
        assert!(r.reading());
        r.clear();
        r.drop_line(b"   ");
        assert!(!r.reading());
        // One that opens a construct drops it to its end.
        r.drop_line(b"while \xff; do");
        for line in ["echo a", "if b; then", "fi"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
        }
        assert!(r.reading());
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
    }

    #[test]
    fn a_line_too_long_to_keep_is_counted_whole() {
        // Plan 1 judged such a line by its last bytes, and dropped the
        // next command to be safe when they did not parse alone.
        let mut r = Reader::new();
        r.drop_bytes(b"if a; then ");
        for _ in 0..100 {
            r.drop_bytes(&[b'x'; 1024]);
        }
        assert!(r.reading());
        r.drop_end();
        assert!(r.reading(), "inside its `if`");
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
        r.drop_bytes(&[b'x'; 70 * 1024]);
        r.drop_bytes(b" '");
        r.drop_end();
        assert!(r.reading(), "a quote goes on to the next line");
        assert_eq!(r.add("'"), Ok(None));
        assert!(!r.reading());
        assert_eq!(names(&r.add("b").unwrap().unwrap()), ["b"]);
    }

    #[test]
    fn a_refused_construct_is_dropped_to_its_end() {
        // Plan 1 dropped each refused line alone, so a script's `if`
        // written across lines ran its body (its final review's ruling):
        // its lines are dropped up to its `fi`, counting those inside.
        let mut r = Reader::new();
        assert_eq!(r.add("while true; do"), Ok(None));
        assert_eq!(
            r.add("coproc a"),
            Err(ParseError::Unsupported("coproc".into()))
        );
        assert!(r.reading());
        for line in ["echo a", "if b", "then c", "fi", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
        assert_eq!(names(&r.add("echo e").unwrap().unwrap()), ["echo"]);
        // Opened on a later line of the command, and refused there.
        assert_eq!(r.add("a &&"), Ok(None));
        assert!(r.add("until $(b); do").is_err());
        assert_eq!(r.add("echo c"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
    }

    #[test]
    fn a_construct_nested_too_deep_is_dropped_to_its_end() {
        let mut r = Reader::new();
        for _ in 0..32 {
            assert_eq!(r.add("if b; then"), Ok(None));
        }
        assert_eq!(
            r.add("if b; then"),
            Err(ParseError::Unsupported(
                "more than 32 levels of nesting".into()
            ))
        );
        for _ in 0..33 {
            assert!(r.reading());
            assert_eq!(r.add("echo c; fi"), Ok(None));
        }
        assert!(!r.reading());
    }

    #[test]
    fn an_if_with_an_error_inside_is_dropped_to_its_fi() {
        // Nothing of it runs, the lines after the error included; the line
        // after its `fi` starts afresh (programmable shell gate §15 item 2).
        let mut r = Reader::new();
        assert_eq!(r.add("if a; then"), Ok(None));
        assert_eq!(r.add("if b; then c"), Ok(None));
        assert_eq!(r.add("d; then"), Err(ParseError::MissingTarget("then")));
        for line in ["e", "fi", "f"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
        // p2: the error at its `fi` drops nothing after it, as in bash.
        assert_eq!(r.add("if true"), Ok(None));
        assert_eq!(r.add("then"), Ok(None));
        assert_eq!(r.add("fi"), Err(ParseError::MissingTarget("fi")));
        assert!(!r.reading());
        assert_eq!(names(&r.add("echo b").unwrap().unwrap()), ["echo"]);
    }

    #[test]
    fn a_line_that_does_not_parse_drops_its_command_to_its_end() {
        // The final review found the end of an `&&` chain run after a line
        // of it was refused (`cd build 2> /dev/null &&`, then `rm -r out`).
        let mut r = Reader::new();
        assert_eq!(r.add("a &&"), Ok(None));
        assert_eq!(
            r.add("b 2> f &&"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert!(r.reading(), "still inside the dropped command");
        assert_eq!(r.add("c"), Ok(None), "its last line, dropped");
        assert_eq!(names(&r.add("d").unwrap().unwrap()), ["d"]);
        // So from its first line, past a comment after the operator, and
        // with a `#` a quote holds.
        for line in ["a $(x) && # c", "a $(x) ' #' &&"] {
            assert!(r.add(line).is_err(), "{line}");
            assert!(r.reading(), "{line}");
            r.clear();
        }
        // A refused line that does not end after an operator ends the
        // command there: the next line starts afresh.
        for line in [
            "a $(x)",
            "echo '&&' $(x)",
            "echo \"a ||\" $(x)",
            "a $(x) \\|",
            "a $(x) # &&",
            "|| b",
            "a |& b",
        ] {
            assert!(r.add(line).is_err(), "{line}");
            assert!(!r.reading(), "{line}");
        }
    }

    #[test]
    fn the_end_of_input_inside_a_command_is_an_error() {
        let mut r = Reader::new();
        assert_eq!(r.end(), None);
        r.add("a |").unwrap();
        assert_eq!(r.end(), Some(ParseError::Incomplete));
        assert!(!r.reading());
        r.add("a &&").unwrap();
        r.clear();
        assert!(!r.reading());
    }
}
