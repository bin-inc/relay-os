//! The command-line parser (spec §7.3).
//!
//! Words are split on spaces and tabs. `'…'` is literal; `"…"` is literal
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, and a bare `$` or `` ` `` in it is refused as outside quotes
//! (bash would expand it); outside quotes `\` makes the next character
//! literal. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
//! `` ` ``, `(` or `)` is an error naming the character, instead of being
//! passed on as if it were plain text; so is `2>` (another stream).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The home directory `~` stands for.
pub const HOME: &str = "/root";

/// One command: its words and where its output goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<String>,
    pub redirect: Option<Redirect>,
}

/// `> path` (truncate) or `>> path` (append).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    pub path: String,
    pub append: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A shell feature the parser does not support, as typed.
    Unsupported(String),
    UnterminatedQuote,
    /// A `\` with nothing after it.
    TrailingBackslash,
    /// A redirection without a file name; holds what came instead.
    MissingTarget(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Unsupported(t) => write!(f, "unsupported syntax: {t}"),
            ParseError::UnterminatedQuote => f.write_str("syntax error: unterminated quote"),
            ParseError::TrailingBackslash => f.write_str("syntax error: nothing after \\"),
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
        }
    }
}

const UNSUPPORTED: &[char] = &['|', ';', '&', '$', '*', '?', '<', '`', '(', ')'];

/// A word being built.
#[derive(Default)]
struct Word {
    text: String,
    /// Something (even `''`) was seen, so the word exists even if empty.
    started: bool,
    /// The word began with an unquoted `~`.
    tilde: bool,
    /// Part of the word was quoted or escaped.
    quoted: bool,
}

impl Word {
    fn finish(self) -> Option<String> {
        if !self.started {
            return None;
        }
        let t = self.text;
        if self.tilde && (t == "~" || t.starts_with("~/")) {
            Some(alloc::format!("{HOME}{}", &t[1..]))
        } else {
            Some(t)
        }
    }
}

/// Words and redirection collected so far.
#[derive(Default)]
struct Parts {
    words: Vec<String>,
    redirect: Option<Redirect>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
    pending: Option<bool>,
}

impl Parts {
    /// Ends a word: it becomes the pending redirection's target or the next
    /// word.
    fn end_word(&mut self, word: &mut Word) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish() else {
            return Ok(());
        };
        match self.pending.take() {
            Some(_) if self.redirect.is_some() => return Err(ParseError::Unsupported(">".into())),
            Some(append) => self.redirect = Some(Redirect { path: w, append }),
            None => self.words.push(w),
        }
        Ok(())
    }
}

pub fn parse(line: &str) -> Result<Command, ParseError> {
    let mut parts = Parts::default();
    let mut word = Word::default();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => parts.end_word(&mut word)?,
            '>' => {
                // `2>` redirects another stream in a real shell.
                if word.started && !word.quoted && word.text.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ParseError::Unsupported(alloc::format!("{}>", word.text)));
                }
                parts.end_word(&mut word)?;
                if parts.pending.is_some() {
                    return Err(ParseError::MissingTarget(">"));
                }
                parts.pending = Some(chars.next_if_eq(&'>').is_some());
            }
            '\'' => {
                word.started = true;
                word.quoted = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.text.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '"' => {
                word.started = true;
                word.quoted = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.text.push(chars.next().expect("peeked"));
                        }
                        Some(c @ ('$' | '`')) => return Err(ParseError::Unsupported(c.into())),
                        Some(c) => word.text.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '\\' => match chars.next() {
                Some(c) => {
                    word.started = true;
                    word.quoted = true;
                    word.text.push(c);
                }
                None => return Err(ParseError::TrailingBackslash),
            },
            // A comment runs to the end of the line.
            '#' if !word.started => break,
            c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
            c => {
                if !word.started && c == '~' {
                    word.tilde = true;
                }
                word.started = true;
                word.text.push(c);
            }
        }
    }
    parts.end_word(&mut word)?;
    if parts.pending.is_some() {
        return Err(ParseError::MissingTarget("newline"));
    }
    Ok(Command {
        words: parts.words,
        redirect: parts.redirect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        let c = parse(line).unwrap();
        assert_eq!(c.redirect, None);
        c.words
    }

    #[test]
    fn words_split_on_spaces_and_tabs() {
        assert_eq!(words("  ls\t-l   /etc "), ["ls", "-l", "/etc"]);
        assert!(words("").is_empty());
        assert!(words(" \t ").is_empty());
    }

    #[test]
    fn a_hash_at_the_start_of_a_word_begins_a_comment() {
        assert_eq!(words("echo a # b | c; $d"), ["echo", "a"]);
        assert!(words("# a whole line").is_empty());
        assert!(words("   #").is_empty());
        // As in bash: inside a word, quoted or escaped it is a character.
        assert_eq!(
            words(r##"echo a#b '#' "#" \#"##),
            ["echo", "a#b", "#", "#", "#"]
        );
        assert_eq!(
            parse("echo x >> # f"),
            Err(ParseError::MissingTarget("newline"))
        );
        let c = parse("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }

    #[test]
    fn single_quotes_are_literal() {
        assert_eq!(
            words(r"echo 'a  b' 'c\d' '|;$*'"),
            ["echo", "a  b", r"c\d", "|;$*"]
        );
        assert_eq!(words("echo ''"), ["echo", ""]);
        assert_eq!(words("echo a'b c'd"), ["echo", "ab cd"]);
    }

    #[test]
    fn double_quotes_take_four_escapes() {
        assert_eq!(
            words(r#"echo "say \"hi\"" "a\\b" "c\d" "x'y" "* ?""#),
            ["echo", r#"say "hi""#, r"a\b", r"c\d", "x'y", "* ?"]
        );
        assert_eq!(
            words(r#"echo "\$HOME costs \`1\`""#),
            ["echo", "$HOME costs `1`"]
        );
    }

    #[test]
    fn dollar_and_backquote_inside_double_quotes_are_unsupported() {
        // Bash expands them there too; passing them on as text would
        // print something bash never prints.
        assert_eq!(
            parse(r#"echo "$HOME""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            parse(r#"echo "a $ b""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            parse(r#"echo "`date`""#),
            Err(ParseError::Unsupported("`".into()))
        );
        assert_eq!(words(r"echo '$HOME `x`'"), ["echo", "$HOME `x`"]);
    }

    #[test]
    fn backslash_escapes_one_character_outside_quotes() {
        assert_eq!(
            words(r"echo a\ b \| \> \\"),
            ["echo", "a b", "|", ">", r"\"]
        );
        assert_eq!(parse(r"echo a\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn redirections_truncate_or_append() {
        let c = parse("echo hi > out.txt").unwrap();
        assert_eq!(c.words, ["echo", "hi"]);
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "out.txt".into(),
                append: false
            })
        );
        let c = parse("echo hi>>'my log'").unwrap();
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "my log".into(),
                append: true
            })
        );
        // The redirection can come first.
        let c = parse(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }

    #[test]
    fn redirection_errors() {
        assert_eq!(parse("echo >"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(parse("echo > > f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(
            parse("echo > a > b"),
            Err(ParseError::Unsupported(">".into()))
        );
        // Other streams are not supported; a quoted or spaced digit is a word.
        assert_eq!(
            parse("cat f 2>err"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(
            parse("echo a 2>>g"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(parse("echo 2 > g").unwrap().words, ["echo", "2"]);
        assert_eq!(parse("echo '2'> g").unwrap().words, ["echo", "2"]);
        assert_eq!(parse("echo x2> g").unwrap().words, ["echo", "x2"]);
        assert_eq!(
            parse("echo >").unwrap_err().to_string(),
            "syntax error near unexpected token `newline'"
        );
    }

    #[test]
    fn unsupported_syntax_names_the_character() {
        for (line, c) in [
            ("ls | wc", '|'),
            ("a; b", ';'),
            ("a && b", '&'),
            ("echo $HOME", '$'),
            ("ls *.txt", '*'),
            ("ls file?", '?'),
            ("cat < f", '<'),
            ("echo `x`", '`'),
            ("(ls)", '('),
        ] {
            assert_eq!(
                parse(line),
                Err(ParseError::Unsupported(c.into())),
                "{line}"
            );
        }
        assert_eq!(
            parse("ls | wc").unwrap_err().to_string(),
            "unsupported syntax: |"
        );
    }

    #[test]
    fn unterminated_quotes_are_errors() {
        assert_eq!(parse("echo 'abc"), Err(ParseError::UnterminatedQuote));
        assert_eq!(parse("echo \"abc\\\""), Err(ParseError::UnterminatedQuote));
    }

    #[test]
    fn tilde_at_the_start_of_a_word_is_home() {
        assert_eq!(words("cd ~"), ["cd", "/root"]);
        assert_eq!(
            words("ls ~/notes a~ ~x '~' \\~"),
            ["ls", "/root/notes", "a~", "~x", "~", "~"]
        );
        let c = parse("echo x > ~/out").unwrap();
        assert_eq!(c.redirect.unwrap().path, "/root/out");
    }
}
