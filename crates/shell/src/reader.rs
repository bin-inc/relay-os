//! The lines a command is read from (programmable shell gate §4.3): a line
//! that leaves a command unfinished (`a &&`, `a |`) is kept until the lines
//! after it finish it. The prompt, `sh FILE` and `X | sh` all read through
//! one.

use crate::parser::{self, COMMAND_MAX, List, ParseError, Parser, Word};

/// What has been read of the command so far.
#[derive(Default)]
pub(crate) struct Reader {
    /// What has been read of the command, kept between its lines.
    parser: Parser,
    /// A command was dropped before its end: its later lines are dropped
    /// too, up to one that finishes it, so that none of them runs without
    /// what came before (the end of an `&&` chain without its guard).
    dropping: bool,
}

impl Reader {
    pub fn new() -> Reader {
        Reader::default()
    }

    /// Adds a line: the command it finishes, none while the command needs
    /// more lines, or why the lines do not parse, which drops them. A
    /// command longer than [`COMMAND_MAX`] is dropped too.
    pub fn add(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        if self.dropping {
            self.dropping = matches!(alone(line), Alone::GoesOn | Alone::Nothing);
            return Ok(None);
        }
        if self.parser.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line);
            return Err(ParseError::TooLong);
        }
        // The parser keeps what it has read, so each line is read once, in
        // its context.
        self.parser.line(line).inspect_err(|_| {
            // A line refused while it leaves the command open drops the
            // rest of the command with it, as `drop_line` does.
            self.parser = Parser::new();
            self.dropping = parser::ends_open(line);
        })
    }

    /// Drops the command a line is part of that cannot be added (too
    /// long, or no text: `line` is what can be read of it, or its last
    /// bytes): its later lines are dropped too unless this one plainly
    /// finishes it.
    pub fn drop_line(&mut self, line: &str) {
        let reading = self.reading();
        self.parser = Parser::new();
        // What does not parse alone might go on (a line's last bytes may
        // start inside a quote): it is dropped to be safe.
        self.dropping = match alone(line) {
            Alone::GoesOn | Alone::Wrong => true,
            Alone::Nothing => reading,
            Alone::Ends => false,
        };
    }

    /// Part of a command has been read, or is being dropped.
    pub fn reading(&self) -> bool {
        !self.parser.is_empty() || self.dropping
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
        self.dropping = false;
    }
}

/// What a line, read alone, does to an unfinished command before it.
enum Alone {
    /// It ends after `|`, `&&` or `||`: the command goes on after it.
    GoesOn,
    /// Blanks or a comment: the command goes on after it.
    Nothing,
    /// A command: it may finish the one before.
    Ends,
    /// It does not parse alone.
    Wrong,
}

fn alone(line: &str) -> Alone {
    match parser::parse_line(line) {
        Err(ParseError::Incomplete) => Alone::GoesOn,
        Ok(list) if list.items.is_empty() => Alone::Nothing,
        Ok(_) => Alone::Ends,
        Err(_) => Alone::Wrong,
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
        // Its text decides, or what is left of it: one that plainly
        // finishes a command ends it; one that goes on, or might, does not.
        r.add("a &&").unwrap();
        r.drop_line("x");
        assert!(!r.reading());
        r.drop_line("x &&");
        assert!(r.reading());
        r.add("b").unwrap();
        assert!(!r.reading());
        r.drop_line("x' &&");
        assert!(r.reading(), "a quote cut in two: it might go on");
        r.clear();
        // A blank one ends nothing, and starts nothing.
        r.add("a ||").unwrap();
        r.drop_line("   ");
        assert!(r.reading());
        r.clear();
        r.drop_line("   ");
        assert!(!r.reading());
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
