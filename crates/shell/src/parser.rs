//! The command-line parser (spec §7.3).
//!
//! Words are split on spaces and tabs. `'…'` is literal; `"…"` is literal
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, and a bare `$` or `` ` `` in it is refused as outside quotes
//! (bash would expand it); outside quotes `\` makes the next character
//! literal. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. An unquoted `|`
//! joins commands into a pipeline (user-space gate §9.1); each has a name,
//! only the last may redirect its output, and bash's syntax errors name a
//! `|` with no command before it or none after. An unquoted `&` at the end
//! of the line (a comment may follow) runs it in the background (§9.2).
//! Every other shell feature is refused: an unquoted `;`, `&` before more,
//! `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an error naming the
//! character, instead of being passed on as if it were plain text; so are
//! `||`, `&&` and `2>` (another stream).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The home directory `~` stands for.
pub const HOME: &str = "/root";

/// A line's commands: one, or several joined by `|`, each one's output the
/// next one's input. A blank line is one command without words.
pub type Pipeline = Vec<Command>;

/// A command line: its pipeline, and, if it ends with `&`, what was typed
/// before the `&` (a background job's text, spec §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pipeline: Pipeline,
    pub background: Option<String>,
}

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
    /// A redirection without a file name, or a `|` without a command
    /// before it; holds what came instead.
    MissingTarget(&'static str),
    /// A `|` without a command after it.
    UnexpectedEnd,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Unsupported(t) => write!(f, "unsupported syntax: {t}"),
            ParseError::UnterminatedQuote => f.write_str("syntax error: unterminated quote"),
            ParseError::TrailingBackslash => f.write_str("syntax error: nothing after \\"),
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
        }
    }
}

const UNSUPPORTED: &[char] = &[';', '$', '*', '?', '<', '`', '(', ')'];

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

    /// The command so far, ended by a `|`, which needs one before it.
    /// Every command of a pipeline has a name: a redirection alone, which
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
            return Err(ParseError::MissingTarget("|"));
        }
        if self.redirect.is_some() {
            return Err(ParseError::Unsupported("> before |".into()));
        }
        let p = core::mem::take(self);
        Ok(Command {
            words: p.words,
            redirect: None,
        })
    }
}

/// The commands of `line`, whether or not it ends with `&`.
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| l.pipeline)
}

/// `line`'s commands, and whether it runs in the background.
pub fn parse_line(line: &str) -> Result<Line, ParseError> {
    let mut background = None;
    let mut pipeline = Vec::new();
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
            '|' => {
                if chars.next_if_eq(&'|').is_some() {
                    return Err(ParseError::Unsupported("||".into()));
                }
                parts.end_word(&mut word)?;
                pipeline.push(parts.take_before_pipe()?);
            }
            '&' => {
                if chars.next_if_eq(&'&').is_some() {
                    return Err(ParseError::Unsupported("&&".into()));
                }
                parts.end_word(&mut word)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
                    return Err(ParseError::MissingTarget("&"));
                }
                if parts.words.is_empty() {
                    // `> f &`: a background job is a program.
                    return Err(ParseError::Unsupported("> &".into()));
                }
                let rest: String = chars.clone().collect();
                let after = rest.trim_start_matches([' ', '\t']);
                if after.starts_with('&') {
                    return Err(ParseError::MissingTarget("&"));
                }
                if !after.is_empty() && !after.starts_with('#') {
                    // `a & b` runs both in bash.
                    return Err(ParseError::Unsupported("&".into()));
                }
                let typed = &line[..line.len() - rest.len() - 1];
                background = Some(String::from(typed.trim_matches([' ', '\t'])));
                break;
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
    if !pipeline.is_empty() && parts.words.is_empty() {
        return Err(match parts.redirect {
            None => ParseError::UnexpectedEnd,
            Some(_) => ParseError::Unsupported("| >".into()),
        });
    }
    pipeline.push(Command {
        words: parts.words,
        redirect: parts.redirect,
    });
    Ok(Line {
        pipeline,
        background,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one command of a line that has no `|`.
    fn one(line: &str) -> Result<Command, ParseError> {
        parse(line).map(|mut p| {
            assert_eq!(p.len(), 1, "{line}");
            p.remove(0)
        })
    }

    fn words(line: &str) -> Vec<String> {
        let c = one(line).unwrap();
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
            one("echo x >> # f"),
            Err(ParseError::MissingTarget("newline"))
        );
        let c = one("echo x > f # to f").unwrap();
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
            one(r#"echo "$HOME""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            one(r#"echo "a $ b""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            one(r#"echo "`date`""#),
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
        assert_eq!(one(r"echo a\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn redirections_truncate_or_append() {
        let c = one("echo hi > out.txt").unwrap();
        assert_eq!(c.words, ["echo", "hi"]);
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "out.txt".into(),
                append: false
            })
        );
        let c = one("echo hi>>'my log'").unwrap();
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "my log".into(),
                append: true
            })
        );
        // The redirection can come first.
        let c = one(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }

    #[test]
    fn redirection_errors() {
        assert_eq!(one("echo >"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(one("echo > > f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(
            one("echo > a > b"),
            Err(ParseError::Unsupported(">".into()))
        );
        // Other streams are not supported; a quoted or spaced digit is a word.
        assert_eq!(
            one("cat f 2>err"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(
            one("echo a 2>>g"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(one("echo 2 > g").unwrap().words, ["echo", "2"]);
        assert_eq!(one("echo '2'> g").unwrap().words, ["echo", "2"]);
        assert_eq!(one("echo x2> g").unwrap().words, ["echo", "x2"]);
        assert_eq!(
            one("echo >").unwrap_err().to_string(),
            "syntax error near unexpected token `newline'"
        );
    }

    #[test]
    fn unsupported_syntax_names_the_character() {
        for (line, c) in [
            ("a; b", ';'),
            ("echo $HOME", '$'),
            ("ls *.txt", '*'),
            ("ls file?", '?'),
            ("cat < f", '<'),
            ("echo `x`", '`'),
            ("(ls)", '('),
        ] {
            assert_eq!(one(line), Err(ParseError::Unsupported(c.into())), "{line}");
        }
        assert_eq!(
            one("echo a 2>f").unwrap_err().to_string(),
            "unsupported syntax: 2>"
        );
    }

    #[test]
    fn a_bar_joins_commands_into_a_pipeline() {
        let p = parse("cat f | grep -c 'a | b' |wc -l>out").unwrap();
        let words: Vec<&[String]> = p.iter().map(|c| &c.words[..]).collect();
        assert_eq!(
            words,
            [&["cat", "f"][..], &["grep", "-c", "a | b"], &["wc", "-l"]]
        );
        assert_eq!(p[0].redirect, None);
        assert_eq!(p[2].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
        assert_eq!(
            parse(r#"echo '|' "|" \| # | x"#).unwrap()[0].words,
            ["echo", "|", "|", "|"]
        );
        assert_eq!(
            parse("").unwrap(),
            [Command {
                words: Vec::new(),
                redirect: None
            }]
        );
    }

    #[test]
    fn a_bar_needs_a_command_on_each_side() {
        // bash's messages (`bash -c '| a'`, `bash -c 'a |'`).
        for line in ["| a", "a | | b", "a || | b", "echo > | b", " |"] {
            let e = parse(line).unwrap_err();
            if line.contains("||") {
                assert_eq!(e, ParseError::Unsupported("||".into()), "{line}");
            } else {
                assert_eq!(
                    e.to_string(),
                    "syntax error near unexpected token `|'",
                    "{line}"
                );
            }
        }
        for line in ["a |", "a | b |  ", "a | # b"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "syntax error: unexpected end of file",
                "{line}"
            );
        }
        assert_eq!(parse("a || b"), Err(ParseError::Unsupported("||".into())));
        // Only the last command redirects (spec §9.1): bash would send the
        // first one's output into the file and the second nothing.
        assert_eq!(
            parse("a > f | b").unwrap_err().to_string(),
            "unsupported syntax: > before |"
        );
    }

    #[test]
    fn every_command_of_a_pipeline_has_a_name() {
        // bash runs a redirection alone as a command; here a pipeline's
        // commands are programs, so one without a name is refused.
        for (line, what) in [
            ("> f | b", "> before |"),
            (">> f | b | c", "> before |"),
            ("a | > f", "| >"),
            ("a | b | >> f", "| >"),
        ] {
            assert_eq!(
                parse(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        for line in ["a | b", "a|b>f", "x | y | z >> f", "a # | > f"] {
            let p = parse(line).unwrap();
            assert!(p.iter().all(|c| !c.words.is_empty()), "{line}");
        }
        // A redirection alone, without a pipeline, still makes the file.
        assert_eq!(one("> f").unwrap().words, Vec::<String>::new());
    }

    #[test]
    fn a_line_ending_with_an_ampersand_runs_in_the_background() {
        let l = parse_line("sleep 5 &").unwrap();
        assert_eq!(l.pipeline[0].words, ["sleep", "5"]);
        assert_eq!(l.background.as_deref(), Some("sleep 5"));
        // The text is what was typed before the `&`, without the blanks
        // around it; a comment may follow.
        for (line, text) in [
            ("  cat f |  wc -l>out& ", "cat f |  wc -l>out"),
            ("echo 'a  b' \\& &\t# later", "echo 'a  b' \\&"),
            ("t-spin&", "t-spin"),
            ("grep x f | head -n 1 & # one", "grep x f | head -n 1"),
        ] {
            let l = parse_line(line).unwrap();
            assert_eq!(l.background.as_deref(), Some(text), "{line}");
        }
        let l = parse_line("cat f | wc -l > out &").unwrap();
        assert_eq!(l.pipeline.len(), 2);
        assert_eq!(l.pipeline[1].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
        for line in ["echo '&' \"&\" \\&", "echo a # &", "echo a"] {
            assert_eq!(parse_line(line).unwrap().background, None, "{line}");
        }
        assert_eq!(words("echo '&' \\& # &"), ["echo", "&", "&"]);
    }

    #[test]
    fn an_ampersand_anywhere_else_is_bash_s_error_or_unsupported() {
        // bash's messages (`bash -c '&'`, `bash -c 'a | &'`, …).
        for line in ["&", " & ", "a | &", "echo > &", "a & &", "a &&&"] {
            let e = parse_line(line).unwrap_err();
            if line.contains("&&") {
                assert_eq!(e, ParseError::Unsupported("&&".into()), "{line}");
            } else {
                assert_eq!(
                    e.to_string(),
                    "syntax error near unexpected token `&'",
                    "{line}"
                );
            }
        }
        // bash runs `a & b`, `a && b` and `> f &`; they are not supported.
        for (line, what) in [
            ("a & b", "&"),
            ("a &b", "&"),
            ("a & | b", "&"),
            ("a && b", "&&"),
            ("a &&", "&&"),
            ("> f &", "> &"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn unterminated_quotes_are_errors() {
        assert_eq!(one("echo 'abc"), Err(ParseError::UnterminatedQuote));
        assert_eq!(one("echo \"abc\\\""), Err(ParseError::UnterminatedQuote));
    }

    #[test]
    fn tilde_at_the_start_of_a_word_is_home() {
        assert_eq!(words("cd ~"), ["cd", "/root"]);
        assert_eq!(
            words("ls ~/notes a~ ~x '~' \\~"),
            ["ls", "/root/notes", "a~", "~x", "~", "~"]
        );
        let c = one("echo x > ~/out").unwrap();
        assert_eq!(c.redirect.unwrap().path, "/root/out");
    }
}
