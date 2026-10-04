//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4). A parameter is replaced by its value, which is never split
//! into words (bash splits an unquoted one): a value with blanks stays one
//! word. A word that holds nothing quoted and expands to nothing is no
//! word; a quoted empty one is an empty word. `$@` gives each argument as
//! a word of its own, the text before it joined to the first and the text
//! after it to the last, as bash's `"$@"` does; an empty argument is kept
//! only in quotes.

use crate::parser::{Command, Param, Piece, Redirect, RedirectOp, Word};
pub(crate) use crate::vars::Vars;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// The most a line expands to, its words' bytes and one for each word, as
/// `spawn` takes at most 64 KiB of arguments (spec §11.1): so nothing a
/// person types grows the shell's heap without bound.
pub const EXPANSION_MAX: usize = 64 * 1024;

/// Why a line could not be expanded; the shell says so, status 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A `${…}` naming no parameter, as typed.
    BadSubstitution(String),
    /// A redirection whose target is not one word, as typed.
    AmbiguousRedirect(String),
    /// The line would expand to more than `EXPANSION_MAX`.
    TooLong,
    /// The variable would make the variables hold more than `crate::vars::VARS_MAX`.
    Full(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSubstitution(t) => write!(f, "{t}: bad substitution"),
            Error::AmbiguousRedirect(t) => write!(f, "{t}: ambiguous redirect"),
            Error::TooLong => f.write_str("the line would expand to more than 64 KiB"),
            Error::Full(n) => write!(f, "{n}: the variables would hold more than 64 KiB"),
        }
    }
}

/// A pipeline's commands with the words they get, `status` being `$?`.
/// The pipeline is expanded whole before any of it runs, and each one of
/// a list just before it runs, so it reads the `$?` of the one before.
pub(crate) fn expand(
    commands: &[Command<Word>],
    vars: &Vars,
    status: i32,
) -> Result<Vec<Command>, Error> {
    let mut x = Expander::new(vars, status);
    commands.iter().map(|c| x.command(c)).collect()
}

/// A command's `words` expanded: `export`'s assignments as assignments
/// are (`export A=~/x`), as bash expands a declaration command's.
pub(crate) fn command_words(
    words: &[Word],
    vars: &Vars,
    status: i32,
) -> Result<Vec<String>, Error> {
    Expander::new(vars, status).words(words)
}

/// `words` expanded as a command's arguments are (a `for`'s list).
pub(crate) fn words(words: &[Word], vars: &Vars, status: i32) -> Result<Vec<String>, Error> {
    let mut x = Expander::new(vars, status);
    let mut out = Vec::new();
    for w in words {
        out.extend(x.word(w)?);
    }
    Ok(out)
}

/// An assignment's value: one string, however it expands (`$@` joined by
/// blanks, as bash joins it there).
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    Expander::new(vars, status).joined(word)
}

/// A redirection's target, which must expand to one word.
pub(crate) fn redirect(r: &Redirect<Word>, vars: &Vars, status: i32) -> Result<Redirect, Error> {
    Expander::new(vars, status).redirect(r)
}

struct Expander<'v> {
    vars: &'v Vars,
    status: i32,
    /// What may still be made: bytes, and one for each word.
    room: usize,
}

/// A word being made, and whether anything quoted went into it.
struct Field {
    text: String,
    quoted: bool,
}

impl<'v> Expander<'v> {
    fn new(vars: &'v Vars, status: i32) -> Expander<'v> {
        Expander {
            vars,
            status,
            room: EXPANSION_MAX,
        }
    }

    /// Takes `n` from the room left, before what it is for is made.
    fn take(&mut self, n: usize) -> Result<(), Error> {
        self.room = self.room.checked_sub(n).ok_or(Error::TooLong)?;
        Ok(())
    }

    fn command(&mut self, c: &Command<Word>) -> Result<Command, Error> {
        let words = self.words(&c.words)?;
        let mut redirects = Vec::new();
        for r in &c.redirects {
            redirects.push(self.redirect(r)?);
        }
        Ok(Command { words, redirects })
    }

    /// A command's words. After an unquoted `export`, a word shaped like an
    /// assignment is one word, `NAME=` and its value expanded as an
    /// assignment's (a `~` after the `=` or a `:`, `$@` joined by blanks).
    fn words(&mut self, words: &[Word]) -> Result<Vec<String>, Error> {
        let export = words.first().is_some_and(|w| w.is_plain("export"));
        let mut out = Vec::new();
        for (i, w) in words.iter().enumerate() {
            match w.assignment().filter(|_| export && i > 0) {
                Some((name, value)) => {
                    let value = self.joined(&value)?;
                    out.push(alloc::format!("{name}={value}"));
                }
                None => out.extend(self.word(w)?),
            }
        }
        Ok(out)
    }

    /// What `word` expands to as one string, `$@`'s words joined by blanks.
    fn joined(&mut self, word: &Word) -> Result<String, Error> {
        Ok(self
            .fields(word)?
            .into_iter()
            .map(|f| f.text)
            .collect::<Vec<_>>()
            .join(" "))
    }

    fn redirect(&mut self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let op = match &r.op {
            RedirectOp::Read(path) => RedirectOp::Read(self.target(path)?),
            RedirectOp::Write(path) => RedirectOp::Write(self.target(path)?),
            RedirectOp::Append(path) => RedirectOp::Append(self.target(path)?),
            RedirectOp::Copy(fd) => RedirectOp::Copy(*fd),
        };
        Ok(Redirect { fd: r.fd, op })
    }

    /// A redirection's file, which must expand to one word.
    fn target(&mut self, path: &Word) -> Result<String, Error> {
        let mut fields = self.word(path)?;
        if fields.len() != 1 {
            return Err(Error::AmbiguousRedirect(path.typed.clone()));
        }
        Ok(fields.remove(0))
    }

    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&mut self, word: &Word) -> Result<Vec<String>, Error> {
        Ok(self
            .fields(word)?
            .into_iter()
            .filter(|f| f.quoted || !f.text.is_empty())
            .map(|f| f.text)
            .collect())
    }

    /// What `word` expands to, before an unquoted empty word is removed.
    fn fields(&mut self, word: &Word) -> Result<Vec<Field>, Error> {
        self.take(1)?;
        let mut fields = alloc::vec![Field {
            text: String::new(),
            quoted: false,
        }];
        for piece in &word.pieces {
            let (value, quoted) = match piece {
                Piece::Text(t, quoted) => (Value::One(Cow::Borrowed(t.as_str())), quoted),
                Piece::Param(p, quoted) => (self.value(p)?, quoted),
            };
            match value {
                Value::One(text) => self.push(&mut fields, &text, *quoted)?,
                Value::Args(args) => {
                    let Some((first, rest)) = args.split_first() else {
                        continue;
                    };
                    self.push(&mut fields, first, *quoted)?;
                    for a in rest {
                        self.take(1)?;
                        fields.push(Field {
                            text: String::new(),
                            quoted: false,
                        });
                        self.push(&mut fields, a, *quoted)?;
                    }
                }
            }
        }
        Ok(fields)
    }

    /// Adds `text` to the last of `fields`.
    fn push(&mut self, fields: &mut [Field], text: &str, quoted: bool) -> Result<(), Error> {
        self.take(text.len())?;
        let last = fields.last_mut().expect("a field");
        last.text.push_str(text);
        last.quoted |= quoted;
        Ok(())
    }

    /// A parameter's value, borrowed from the variables where it is
    /// theirs, so that its room is taken before any of it is copied.
    fn value(&self, p: &Param) -> Result<Value<'v>, Error> {
        let args = &self.vars.args;
        Ok(match p {
            Param::Name(n) => Value::One(Cow::Borrowed(self.vars.get(n))),
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
            Param::Count => Value::One(Cow::Owned((args.len() - 1).to_string())),
            Param::Status => Value::One(Cow::Owned(self.status.to_string())),
            Param::All => Value::Args(&args[1..]),
            Param::Bad(t) => return Err(Error::BadSubstitution(t.clone())),
        })
    }
}

/// What a piece of a word gives: text, or `$@`'s arguments, a word each.
enum Value<'v> {
    One(Cow<'v, str>),
    Args(&'v [String]),
}

/// The words of a pipeline's `commands` with nothing set: for callers
/// that run no shell (tests).
pub(crate) fn plain(commands: &[Command<Word>]) -> Result<Vec<Command>, Error> {
    expand(commands, &Vars::new(crate::shell::NAME), 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    /// The commands of the first pipeline of `line`, as typed.
    fn typed(line: &str) -> Vec<Command<Word>> {
        let first = parse_line(line).unwrap().items.remove(0).and_or.first;
        first.commands().to_vec()
    }

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        expand(&typed(line), vars, 3).map(|mut c| c.remove(0).words)
    }

    /// A script `s.sh` run as `sh s.sh one 'two three' '' four`, with `A`
    /// set to `a  b` and `E` to nothing.
    fn script() -> Vars {
        Vars::of(
            &[("A", "a  b"), ("E", "")],
            &["s.sh", "one", "two three", "", "four"],
        )
    }

    #[test]
    fn parameters_expand_to_their_values() {
        // What bash prints for each, but where a value has blanks.
        let v = script();
        assert_eq!(
            words(r#"echo $0 $1 "$2" $4 $# $? ${1}x $10 ${10} $9"#, &v).unwrap(),
            [
                "echo",
                "s.sh",
                "one",
                "two three",
                "four",
                "4",
                "3",
                "onex",
                "one0"
            ]
        );
        assert_eq!(
            words(r#"echo "$A" ${A}! "<$E>" '$A' \$A $UNSET"#, &v).unwrap(),
            ["echo", "a  b", "a  b!", "<>", "$A", "$A"]
        );
    }

    #[test]
    fn a_value_with_blanks_stays_one_word() {
        // bash splits an unquoted `$A` into `a` and `b`, and `$2` into
        // `two` and `three`: here a value is never split.
        let v = script();
        assert_eq!(
            words("echo $A $2", &v).unwrap(),
            ["echo", "a  b", "two three"]
        );
    }

    #[test]
    fn an_unquoted_empty_expansion_is_no_word_a_quoted_one_is() {
        let v = script();
        // As bash's.
        assert_eq!(
            words(r#"echo $E $3 $UNSET "$E" ''$E "$3" x$E"#, &v).unwrap(),
            ["echo", "", "", "", "x"]
        );
        assert_eq!(words("$E $UNSET", &v).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn all_arguments_are_a_word_each() {
        let v = script();
        // `"$@"` as bash's; unquoted, bash would also split `two three`.
        assert_eq!(
            words(r#"echo "$@""#, &v).unwrap(),
            ["echo", "one", "two three", "", "four"]
        );
        assert_eq!(
            words("echo $@", &v).unwrap(),
            ["echo", "one", "two three", "four"]
        );
        assert_eq!(
            words(r#"echo "a$@b" x$@"#, &v).unwrap(),
            [
                "echo",
                "aone",
                "two three",
                "",
                "fourb",
                "xone",
                "two three",
                "four"
            ]
        );
        // None, without arguments; text joined to it stays.
        let none = Vars::of(&[], &["s.sh"]);
        assert_eq!(
            words(r#"echo "$@" $@ "x$@" ''"$@" $#"#, &none).unwrap(),
            ["echo", "x", "", "0"]
        );
        // An empty argument alone, as bash's.
        let empty = Vars::of(&[], &["s.sh", ""]);
        assert_eq!(words(r#"echo $@ "$@""#, &empty).unwrap(), ["echo", ""]);
    }

    #[test]
    fn an_assignment_s_value_is_one_string() {
        // As bash sets it: `$@` joined by blanks, nothing removed.
        let v = script();
        let value = |line: &str| {
            let p = typed(line);
            let (_, value) = p[0].words[0].assignment().unwrap();
            super::value(&value, &v, 0)
        };
        assert_eq!(value("A=$@").unwrap(), "one two three  four");
        assert_eq!(value(r#"A="<$@>""#).unwrap(), "<one two three  four>");
        assert_eq!(value("A=$E$UNSET").unwrap(), "");
        assert_eq!(value("A=$A.$#").unwrap(), "a  b.4");
        assert_eq!(
            value("A=${1A}").unwrap_err(),
            Error::BadSubstitution("${1A}".into())
        );
    }

    #[test]
    fn a_line_expands_to_at_most_64_kib() {
        let half = "x".repeat(EXPANSION_MAX / 2);
        let v = Vars::of(&[("A", &half)], &["s.sh"]);
        assert_eq!(words("echo $A", &v).unwrap()[1].len(), half.len());
        assert_eq!(words("echo $A $A", &v), Err(Error::TooLong));
        assert_eq!(words("echo \"$A$A\"", &v), Err(Error::TooLong));
        // Each word counts too: many empty arguments, many times.
        let mut args = alloc::vec!["s.sh"];
        args.extend(core::iter::repeat_n("", 20_000));
        let v = Vars::of(&[], &args);
        assert_eq!(words(r#"echo "$@""#, &v).unwrap().len(), 20_001);
        assert_eq!(
            words(r#"echo "$@" "$@" "$@" "$@""#, &v),
            Err(Error::TooLong)
        );
        // And so does each word of the line itself.
        let empties = "'' ".repeat(EXPANSION_MAX);
        assert_eq!(words(&empties, &v).unwrap().len(), EXPANSION_MAX);
        assert_eq!(words(&(empties + "''"), &v), Err(Error::TooLong));
        assert_eq!(
            Error::TooLong.to_string(),
            "the line would expand to more than 64 KiB"
        );
    }

    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
        let v = script();
        for (line, typed) in [
            ("echo ${1A}", "${1A}"),
            ("echo \"${}\"", "${}"),
            ("echo ${ A}", "${ A}"),
            ("echo ${A B}", "${A B}"),
            ("echo ${é}", "${é}"),
        ] {
            let e = words(line, &v).unwrap_err();
            assert_eq!(e, Error::BadSubstitution(typed.into()), "{line}");
            assert_eq!(e.to_string(), alloc::format!("{typed}: bad substitution"));
        }
    }

    #[test]
    fn a_redirection_target_must_expand_to_one_word() {
        let v = script();
        let target =
            |line: &str| expand(&typed(line), &v, 0).map(|c| c[0].output().unwrap().0.clone());
        assert_eq!(target("echo > $1.txt").unwrap(), "one.txt");
        assert_eq!(target("echo > $A").unwrap(), "a  b");
        assert_eq!(target(r#"echo > "$E""#).unwrap(), "");
        // bash's message names the target as typed.
        for (line, typed) in [
            ("echo hi > $E", "$E"),
            ("echo hi >> $UNSET$E", "$UNSET$E"),
            ("echo hi > $@", "$@"),
        ] {
            let e = target(line).unwrap_err();
            assert_eq!(e.to_string(), alloc::format!("{typed}: ambiguous redirect"));
        }
    }
}
