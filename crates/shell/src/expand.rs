//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4). A parameter is replaced by its value, which is never split
//! into words (bash splits an unquoted one): a value with blanks stays one
//! word. A word that holds nothing quoted and expands to nothing is no
//! word; a quoted empty one is an empty word. `$@` gives each argument as
//! a word of its own, the text before it joined to the first and the text
//! after it to the last, as bash's `"$@"` does; an empty argument is kept
//! only in quotes.

use crate::parser::{Command, Param, Piece, Pipeline, Redirect, Word};
use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// The most a line expands to, its words' bytes and one for each word, as
/// `spawn` takes at most 64 KiB of arguments (spec §11.1): so nothing a
/// person types grows the shell's heap without bound.
pub const EXPANSION_MAX: usize = 64 * 1024;

/// The most a shell's variables hold, their names' and values' bytes.
pub const VARS_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
    args: Vec<String>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            size: 0,
            args: alloc::vec![String::from(name)],
        }
    }

    /// A script's: none set, `$0` its `name` and `args` after it.
    pub fn script(name: &str, args: &[String]) -> Vars {
        let mut vars = Vars::new(name);
        vars.args.extend_from_slice(args);
        vars
    }

    /// The variable `name`'s value; an unset one is empty.
    pub fn get(&self, name: &str) -> &str {
        self.names.get(name).map_or("", String::as_str)
    }

    /// Sets the variable `name` to `value`, unless the variables would
    /// then hold more than `VARS_MAX`.
    pub fn set(&mut self, name: &str, value: String) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| name.len() + v.len());
        let size = self.size - old + name.len() + value.len();
        if size > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = size;
        self.names.insert(String::from(name), value);
        Ok(())
    }
}

/// Why a line could not be expanded; the shell says so, status 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A `${…}` naming no parameter, as typed.
    BadSubstitution(String),
    /// A redirection whose target is not one word, as typed.
    AmbiguousRedirect(String),
    /// The line would expand to more than `EXPANSION_MAX`.
    TooLong,
    /// The variable would make the variables hold more than `VARS_MAX`.
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
    pipeline: &Pipeline<Word>,
    vars: &Vars,
    status: i32,
) -> Result<Pipeline, Error> {
    let mut x = Expander::new(vars, status);
    let mut commands = Vec::new();
    for c in &pipeline.commands {
        commands.push(x.command(c)?);
    }
    Ok(Pipeline { commands })
}

/// An assignment's value: one string, however it expands (`$@` joined by
/// blanks, as bash joins it there).
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    Ok(Expander::new(vars, status)
        .fields(word)?
        .into_iter()
        .map(|f| f.text)
        .collect::<Vec<_>>()
        .join(" "))
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
        let mut words = Vec::new();
        for w in &c.words {
            words.extend(self.word(w)?);
        }
        let redirect = match &c.redirect {
            Some(r) => Some(self.redirect(r)?),
            None => None,
        };
        Ok(Command { words, redirect })
    }

    fn redirect(&mut self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let mut fields = self.word(&r.path)?;
        if fields.len() != 1 {
            return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
        }
        Ok(Redirect {
            path: fields.remove(0),
            append: r.append,
        })
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

/// The words of `pipeline` with nothing set: for callers that run no
/// shell (tests).
pub(crate) fn plain(pipeline: &Pipeline<Word>) -> Result<Pipeline, Error> {
    expand(pipeline, &Vars::new(crate::shell::NAME), 0)
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        let mut vars = Vars::new("");
        for (n, v) in names {
            vars.set(n, String::from(*v)).unwrap();
        }
        vars.args = args.iter().map(|a| String::from(*a)).collect();
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    /// The first pipeline of `line`, as typed.
    fn typed(line: &str) -> Pipeline<Word> {
        parse_line(line).unwrap().items.remove(0).and_or.first
    }

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        expand(&typed(line), vars, 3).map(|mut p| p.commands.remove(0).words)
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
            let (_, value) = p.commands[0].words[0].assignment().unwrap();
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
    fn the_variables_hold_at_most_64_kib() {
        let mut v = Vars::new("sh");
        let big = "x".repeat(VARS_MAX - 1);
        v.set("A", big.clone()).unwrap();
        assert_eq!(v.set("B", String::new()), Err(Error::Full("B".into())));
        assert_eq!(v.set("A", big.clone() + "y"), Err(Error::Full("A".into())));
        assert_eq!(v.get("A"), big, "unchanged");
        // A smaller value makes room again.
        v.set("A", String::from("1")).unwrap();
        v.set("B", "x".repeat(VARS_MAX - 3)).unwrap();
        assert_eq!(
            Error::Full("B".into()).to_string(),
            "B: the variables would hold more than 64 KiB"
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
        let target = |line: &str| {
            expand(&typed(line), &v, 0).map(|mut p| p.commands.remove(0).redirect.unwrap().path)
        };
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
