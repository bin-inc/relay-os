//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4). A parameter is replaced by its value, which is never split
//! into words (bash splits an unquoted one): a value with blanks stays one
//! word. A word that holds nothing quoted and expands to nothing is no
//! word; a quoted empty one is an empty word. `$@` gives each argument as
//! a word of its own, the text before it joined to the first and the text
//! after it to the last, as bash's `"$@"` does; an empty argument is kept
//! only in quotes.

use crate::parser::{Command, Line, Param, Piece, Redirect, Word};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// `$0`, then `$1` on.
    args: Vec<String>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            args: alloc::vec![String::from(name)],
        }
    }

    /// The variable `name`'s value; an unset one is empty.
    pub fn get(&self, name: &str) -> &str {
        self.names.get(name).map_or("", String::as_str)
    }
}

/// Why a line could not be expanded; the shell says so, status 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A `${…}` naming no parameter, as typed.
    BadSubstitution(String),
    /// A redirection whose target is not one word, as typed.
    AmbiguousRedirect(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSubstitution(t) => write!(f, "{t}: bad substitution"),
            Error::AmbiguousRedirect(t) => write!(f, "{t}: ambiguous redirect"),
        }
    }
}

/// `line`'s commands with the words they get, `status` being `$?`.
pub(crate) fn expand(line: &Line<Word>, vars: &Vars, status: i32) -> Result<Line, Error> {
    let x = Expander { vars, status };
    let mut pipeline = Vec::new();
    for c in &line.pipeline {
        pipeline.push(x.command(c)?);
    }
    Ok(Line {
        pipeline,
        background: line.background.clone(),
    })
}

struct Expander<'v> {
    vars: &'v Vars,
    status: i32,
}

/// A word being made, and whether anything quoted went into it.
struct Field {
    text: String,
    quoted: bool,
}

impl Expander<'_> {
    fn command(&self, c: &Command<Word>) -> Result<Command, Error> {
        let mut words = Vec::new();
        for w in &c.words {
            words.extend(self.word(w)?);
        }
        let redirect = match &c.redirect {
            Some(r) => {
                let mut fields = self.word(&r.path)?;
                if fields.len() != 1 {
                    return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
                }
                Some(Redirect {
                    path: fields.remove(0),
                    append: r.append,
                })
            }
            None => None,
        };
        Ok(Command { words, redirect })
    }

    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&self, word: &Word) -> Result<Vec<String>, Error> {
        let mut fields = alloc::vec![Field {
            text: String::new(),
            quoted: false,
        }];
        for piece in &word.pieces {
            let last = fields.last_mut().expect("a field");
            match piece {
                Piece::Text(t, quoted) => {
                    last.text.push_str(t);
                    last.quoted |= quoted;
                }
                Piece::Param(Param::All, quoted) => {
                    let Some((first, rest)) = self.vars.args[1..].split_first() else {
                        continue;
                    };
                    last.text.push_str(first);
                    last.quoted |= quoted;
                    fields.extend(rest.iter().map(|a| Field {
                        text: a.clone(),
                        quoted: *quoted,
                    }));
                }
                Piece::Param(p, quoted) => {
                    last.text.push_str(&self.value(p)?);
                    last.quoted |= quoted;
                }
            }
        }
        Ok(fields
            .into_iter()
            .filter(|f| f.quoted || !f.text.is_empty())
            .map(|f| f.text)
            .collect())
    }

    /// A parameter's value (not `$@`'s).
    fn value(&self, p: &Param) -> Result<String, Error> {
        let args = &self.vars.args;
        Ok(match p {
            Param::Name(n) => String::from(self.vars.get(n)),
            Param::Arg(i) => args.get(*i).cloned().unwrap_or_default(),
            Param::Count => (args.len() - 1).to_string(),
            Param::Status => self.status.to_string(),
            Param::All => args[1..].join(" "),
            Param::Bad(t) => return Err(Error::BadSubstitution(t.clone())),
        })
    }
}

/// The words of `line` with nothing set: for callers that run no shell
/// (tests).
pub(crate) fn plain(line: &Line<Word>) -> Result<Line, Error> {
    expand(line, &Vars::new(crate::shell::NAME), 0)
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        Vars {
            names: names
                .iter()
                .map(|(n, v)| (String::from(*n), String::from(*v)))
                .collect(),
            args: args.iter().map(|a| String::from(*a)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        let l = parse_line(line).unwrap();
        expand(&l, vars, 3).map(|mut l| l.pipeline.remove(0).words)
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
            let l = parse_line(line).unwrap();
            expand(&l, &v, 0).map(|mut l| l.pipeline.remove(0).redirect.unwrap().path)
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
