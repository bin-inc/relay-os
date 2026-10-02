//! The lines a command is read from (programmable shell gate §4.3): a line
//! that leaves a command unfinished (`a &&`, `a |`) is kept until the lines
//! after it finish it. The prompt, `sh FILE` and `X | sh` all read through
//! one.

use crate::parser::{self, COMMAND_MAX, List, ParseError, Word};
use alloc::string::String;

/// What has been read of the command so far.
#[derive(Default)]
pub(crate) struct Reader {
    text: String,
}

impl Reader {
    pub fn new() -> Reader {
        Reader::default()
    }

    /// Adds a line: the command it finishes, none while the command needs
    /// more lines, or why the lines do not parse, which drops them. A
    /// command longer than [`COMMAND_MAX`] is dropped too.
    pub fn add(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        if self.text.len() + line.len() + 1 > COMMAND_MAX {
            self.text.clear();
            return Err(ParseError::TooLong);
        }
        self.text.push_str(line);
        self.text.push('\n');
        match parser::parse_line(&self.text) {
            Err(ParseError::Incomplete) => Ok(None),
            done => {
                self.text.clear();
                done.map(Some)
            }
        }
    }

    /// Part of a command has been read.
    pub fn reading(&self) -> bool {
        !self.text.is_empty()
    }

    /// The input ended: a command read only in part is bash's `unexpected
    /// end of file`, and is dropped.
    pub fn end(&mut self) -> Option<ParseError> {
        let reading = self.reading();
        self.text.clear();
        reading.then_some(ParseError::Incomplete)
    }

    /// Drops what has been read (Ctrl-C at the `> ` prompt).
    pub fn clear(&mut self) {
        self.text.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first command's name of each pipeline of `list`.
    fn names(list: &List<Word>) -> alloc::vec::Vec<&str> {
        let mut names = alloc::vec::Vec::new();
        for item in &list.items {
            names.push(item.and_or.first.commands[0].words[0].typed.as_str());
            for (_, p) in &item.and_or.rest {
                names.push(p.commands[0].words[0].typed.as_str());
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
        assert!(!r.reading(), "dropped");
        assert_eq!(
            ParseError::TooLong.to_string(),
            "the command would be longer than 64 KiB"
        );
        // Up to the limit, newlines counted, it is read.
        let fits = "x".repeat(COMMAND_MAX - 1);
        assert_eq!(r.add(&fits).unwrap().unwrap().items.len(), 1);
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
