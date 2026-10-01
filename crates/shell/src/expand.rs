//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4).

use crate::parser::{Command, Line, Redirect, Word};

/// `line`'s commands with the words they get.
pub(crate) fn expand(line: &Line<Word>) -> Line {
    Line {
        pipeline: line.pipeline.iter().map(command).collect(),
        background: line.background.clone(),
    }
}

fn command(c: &Command<Word>) -> Command {
    Command {
        words: c.words.iter().map(Word::text).collect(),
        redirect: c.redirect.as_ref().map(|r| Redirect {
            path: r.path.text(),
            append: r.append,
        }),
    }
}
