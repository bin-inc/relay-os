//! The command-line parser (spec §7.3).
//!
//! Words are split on spaces and tabs. `'…'` is literal; `"…"` is literal
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, parameters expand in it, and a bare `` ` `` in it is refused
//! as outside quotes (bash would run it); outside quotes `\` makes the
//! next character literal. Each word is given as typed: its pieces, quoted
//! or not, for expansion (`crate::expand`), and its text for messages.
//!
//! Outside single quotes a parameter, `$NAME`, `${NAME}`, `$0`…`$9`,
//! `${N}`, `$#`, `$@` or `$?`, is a piece of its word that expansion
//! replaces (user-space gate §9.4). bash's other parameters (`$*`, `$$`,
//! `$!`, `$-`, `$_`), its operators (`${A:-x}`, `${#A}`) and, outside
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`,
//! `$((` and `$[`; a `${` without its `}` is bash's syntax error (quotes,
//! escapes and `${…}` inside it are read whole, as bash reads them), and a
//! `${…}` that names nothing expands to its `bad substitution`. A `$`
//! before anything else is a `$`.
//!
//! A word whose unquoted start is a name and `=` is an assignment
//! (`Word::assignment`), its value's `~` at its start or after a `:` made
//! `/root`, as bash's is; an assignment before a command, which would give
//! bash's command an environment, and bash's `NAME+=value` are refused.
//!
//! `> file` and `>> file` redirect standard output, and `2> file` and
//! `2>> file` standard error (programmable shell gate §7.1), any number of
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. `2>&1`, `1>&2` and `>&2`
//! make one of them a copy of the other as it is at that point; any other
//! word after `>&` is refused, as are bash's `&>` and `>|`. `< file` and
//! `0< file` read the file as standard input; bash's other fds,
//! here-documents, `<&` and `<>` are refused. An unquoted `~` alone, or
//! before `/` in the same unquoted piece, at the start of a word means
//! `/root`, as in Linux. An unquoted `#` at the start of a word begins a
//! comment, which runs to the end of the line. An unquoted `|` joins
//! commands into a pipeline (user-space gate §9.1); each has a name, only
//! the last may redirect its output and only the first its input, and
//! bash's syntax errors name a `|` with no command before it or none after.
//! A line is a [`List`] (programmable shell gate §4.1): an unquoted `;`
//! ends one of its items, and an unquoted `&` ends one that runs in the
//! background (user-space gate §9.2), the line going on after either; `&&`
//! and `||` join pipelines into an and-or list. bash's syntax errors name
//! any of them with no command before it, and an and-or list ending with
//! `&` is refused. An unquoted `!` word at a pipeline's start negates its
//! status. A newline ends an item as `;` does, but after `|`, `&&` or `||`
//! the command goes on to the next line; text that ends there is
//! [`ParseError::Incomplete`], and a reader asks for more.
//! `if … then … [elif … then …] [else …] fi`, `while … do … done`,
//! `until … do … done` and `for NAME [in WORD…] do … done` are compound
//! commands, their words keywords only unquoted, whole and where a command
//! name would stand (and `in` and `do` where a `for` takes them; after `fi`
//! or `done`, only a keyword or an operator may follow); one cannot stand
//! in a pipeline of several or before `&`, and redirections after its end
//! hold for all of it (§7.4), after which only an operator may come. A
//! `for`'s header takes no operator but the `;` or newline that ends its
//! words. bash's other reserved words (`case`, `{`, …) are refused where a
//! command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `` ` ``, `(`
//! or `)` is an error naming the character, instead of being passed on as
//! if it were plain text; so is `|&` (the errors into the pipe too).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::iter::Peekable;
use core::str::CharIndices;

/// The most a command being read across lines holds (programmable shell
/// gate §4.5), as a script does.
pub const COMMAND_MAX: usize = 64 * 1024;

/// How deep compound commands may nest (programmable shell gate §4.5):
/// each counts a level, and one that follows `&&` or `||` one more, as the
/// walker, which recurses on `/bin/sh`'s fixed stack, has the and-or list's
/// frame under it.
pub const NESTING_MAX: usize = 32;

/// The home directory `~` stands for.
pub const HOME: &str = "/root";

/// What a command line holds (programmable shell gate §4.1): its items,
/// run one after another. A line of nothing but blanks or a comment holds
/// none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List<W = String> {
    pub items: Vec<Item<W>>,
}

/// One item of a list: an and-or list, and, if it ends with `&`, what was
/// typed of it (a background job's text, user-space gate §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<W = String> {
    pub and_or: AndOr<W>,
    pub background: Option<String>,
}

/// Pipelines joined by `&&` and `||`: the first, then each one with what
/// joins it to the ones before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndOr<W = String> {
    pub first: Pipeline<W>,
    pub rest: Vec<(Connector, Pipeline<W>)>,
}

/// What joins a pipeline to the ones before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    /// `&&`: it runs if the status so far is 0.
    And,
    /// `||`: it runs if the status so far is not 0.
    Or,
}

/// What a pipeline runs and, after a `!`, its status negated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline<W = String> {
    pub negated: bool,
    pub run: Run<W>,
    /// A compound command's redirections; a simple command holds its own.
    pub redirects: Vec<Redirect<W>>,
}

/// What a pipeline runs (programmable shell gate §4.1): one command, or
/// several joined by `|`, each one's output the next one's input; or one
/// compound command, which cannot stand in a pipeline of several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run<W = String> {
    Commands(Vec<Command<W>>),
    Compound(Box<Compound<W>>),
}

/// A compound command (programmable shell gate §4.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compound<W = String> {
    If(If<W>),
    While(Loop<W>),
    Until(Loop<W>),
    For(For<W>),
}

/// `if`: each condition (the `if`'s, then each `elif`'s) with the body it
/// runs, and the `else` body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct If<W = String> {
    pub branches: Vec<(List<W>, List<W>)>,
    pub otherwise: Option<List<W>>,
}

/// `while` or `until`: the condition run before each pass, and the body
/// run while its status is 0 (`while`) or not (`until`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loop<W = String> {
    pub condition: List<W>,
    pub body: List<W>,
}

/// `for`: its variable's name as typed, the words it takes in turn (none
/// for `"$@"`), and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct For<W = String> {
    pub name: W,
    pub words: Option<Vec<W>>,
    pub body: List<W>,
}

impl<W> List<W> {
    /// Whether a background job stands anywhere in it, in a compound
    /// command too.
    pub fn has_job(&self) -> bool {
        self.items.iter().any(|item| {
            item.background.is_some()
                || core::iter::once(&item.and_or.first)
                    .chain(item.and_or.rest.iter().map(|(_, p)| p))
                    .any(|p| matches!(&p.run, Run::Compound(c) if c.has_job()))
        })
    }
}

impl<W> Compound<W> {
    fn has_job(&self) -> bool {
        match self {
            Compound::If(i) => {
                i.branches.iter().any(|(c, b)| c.has_job() || b.has_job())
                    || i.otherwise.as_ref().is_some_and(List::has_job)
            }
            Compound::While(l) | Compound::Until(l) => l.condition.has_job() || l.body.has_job(),
            Compound::For(f) => f.body.has_job(),
        }
    }

    /// The word that ends it, which messages about it name.
    fn end(&self) -> &'static str {
        match self {
            Compound::If(_) => "fi",
            Compound::While(_) | Compound::Until(_) | Compound::For(_) => "done",
        }
    }
}

/// One command: its words and its redirections, in the order typed. The
/// parser gives them as typed ([`Word`]), and expansion as the strings a
/// command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<W>,
    pub redirects: Vec<Redirect<W>>,
}

impl<W> Redirect<W> {
    /// Its operator, as a refusal names it (`<`, `2>>`, `>&2`).
    fn operator(&self) -> String {
        let fd = |default| {
            if self.fd == default {
                String::new()
            } else {
                format!("{}", self.fd)
            }
        };
        match &self.op {
            RedirectOp::Read(_) => format!("{}<", fd(0)),
            RedirectOp::Write(_) => format!("{}>", fd(1)),
            RedirectOp::Append(_) => format!("{}>>", fd(1)),
            RedirectOp::Copy(from) => format!("{}>&{from}", fd(1)),
        }
    }

    /// `> file` or `>> file` on fd 1.
    fn is_output_file(&self) -> bool {
        self.fd == 1 && matches!(self.op, RedirectOp::Write(_) | RedirectOp::Append(_))
    }
}

impl<W> Command<W> {
    /// The file its standard output goes to, and whether it is appended
    /// to.
    pub fn output(&self) -> Option<(&W, bool)> {
        self.redirects.iter().rev().find_map(|r| match &r.op {
            RedirectOp::Write(path) if r.fd == 1 => Some((path, false)),
            RedirectOp::Append(path) if r.fd == 1 => Some((path, true)),
            _ => None,
        })
    }
}

/// A redirection (programmable shell gate §7.1): what fd `fd` becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect<W = String> {
    pub fd: u32,
    pub op: RedirectOp<W>,
}

/// What a redirection makes of its fd.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedirectOp<W = String> {
    /// `< path`: the file, read from its start.
    Read(W),
    /// `> path`: the file, created or emptied, written from its start.
    Write(W),
    /// `>> path`: the file, created if missing, written at its end.
    Append(W),
    /// `>&N`: a copy of fd N (1 or 2) as it is at that point.
    Copy(u32),
}

/// A word as typed: its pieces of text, each quoted (or escaped) or not,
/// and the parameters in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub pieces: Vec<Piece>,
    /// The word as it was typed (`$f`), for messages about it.
    pub typed: String,
}

/// A piece of a word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text, and whether it was quoted or escaped.
    Text(String, bool),
    /// A parameter, and whether it was in double quotes.
    Param(Param, bool),
}

/// A parameter expansion replaces (user-space gate §9.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Param {
    /// `$NAME` or `${NAME}`: a variable.
    Name(String),
    /// `$N` or `${N}`: argument N, `$0` the script's (or shell's) name.
    Arg(usize),
    /// `$#`: how many arguments there are.
    Count,
    /// `$@`: every argument, each one word.
    All,
    /// `$?`: the last status.
    Status,
    /// A `${…}` that names no parameter (`${1A}`), as typed: bash's `bad
    /// substitution` when it expands.
    Bad(String),
}

impl Word {
    /// Adds `c`, quoted or not, to the word's last piece, or a new one.
    fn push(&mut self, c: char, quoted: bool) {
        match self.pieces.last_mut() {
            Some(Piece::Text(text, q)) if *q == quoted => text.push(c),
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

    /// The word is `text`, unquoted, and nothing else.
    pub fn is_plain(&self, text: &str) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t == text)
    }

    /// The word as an assignment, `NAME=value`, if its name and `=` are
    /// unquoted (`"A"=x` is none, as in bash): the name, and the value as a
    /// word of its own, a `~` at its start or after a `:` made `/root`, as
    /// bash's is.
    pub fn assignment(&self) -> Option<(&str, Word)> {
        let Some(Piece::Text(first, false)) = self.pieces.first() else {
            return None;
        };
        let (name, rest) = first.split_once('=')?;
        if !is_name(name) {
            return None;
        }
        let mut pieces = Vec::new();
        if !rest.is_empty() {
            pieces.push(Piece::Text(rest.into(), false));
        }
        pieces.extend(self.pieces[1..].iter().cloned());
        let count = pieces.len();
        for (i, piece) in pieces.iter_mut().enumerate() {
            if let Piece::Text(text, false) = piece {
                *text = value_tildes(text, i == 0, i + 1 == count);
            }
        }
        let typed = String::from(self.typed.get(name.len() + 1..).unwrap_or(""));
        Some((name, Word { pieces, typed }))
    }

    /// The word appends to a variable, `NAME+=value`, as bash's does.
    fn appends(&self) -> bool {
        let Some(Piece::Text(first, false)) = self.pieces.first() else {
            return false;
        };
        first
            .split_once('=')
            .and_then(|(before, _)| before.strip_suffix('+'))
            .is_some_and(is_name)
    }

    /// The word is `!`, unquoted.
    fn is_bang(&self) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t == "!")
    }

    /// The keyword the word is, unquoted and whole.
    fn keyword(&self) -> Option<Keyword> {
        let [Piece::Text(t, false)] = &self.pieces[..] else {
            return None;
        };
        KEYWORDS.iter().find(|(w, _)| w == t).map(|&(_, k)| k)
    }

    /// The word is one of bash's reserved words, unquoted.
    fn is_reserved(&self) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if RESERVED.contains(&t.as_str()))
    }

    /// The word if it is one unquoted piece of text, all ASCII digits (`2`
    /// in `2>`).
    fn digits(&self) -> Option<&str> {
        match &self.pieces[..] {
            [Piece::Text(t, false)] if t.bytes().all(|b| b.is_ascii_digit()) => Some(t),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A shell feature the parser does not support, as typed.
    Unsupported(String),
    UnterminatedQuote,
    /// A `\` with nothing after it.
    TrailingBackslash,
    /// A token where bash's grammar allows none: after a redirection
    /// that has no file name yet, or an operator with no command before
    /// it. Holds the token, as bash's message names it.
    MissingTarget(&'static str),
    /// A word where bash's grammar allows none (after `fi`), as typed.
    Unexpected(String),
    /// The text is the start of a command that needs more lines: it ends
    /// after a `|`, `&&` or `||` (programmable shell gate §4.3). A reader
    /// asks for the next line; where none can come it is bash's error.
    Incomplete,
    /// A command being read would be longer than [`COMMAND_MAX`].
    TooLong,
    /// A `${` without its `}`.
    UnclosedBrace,
    /// From `parse` only: the line parses, but does not expand (why).
    Expansion(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Unsupported(t) => write!(f, "unsupported syntax: {t}"),
            ParseError::UnterminatedQuote => f.write_str("syntax error: unterminated quote"),
            ParseError::TrailingBackslash => f.write_str("syntax error: nothing after \\"),
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::Unexpected(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::Incomplete => f.write_str("syntax error: unexpected end of file"),
            ParseError::TooLong => f.write_str("the command would be longer than 64 KiB"),
            ParseError::Expansion(why) => f.write_str(why),
            ParseError::UnclosedBrace => {
                f.write_str("syntax error: unexpected EOF while looking for matching `}'")
            }
        }
    }
}

const UNSUPPORTED: &[char] = &['*', '?', '`', '(', ')'];

/// bash's other reserved words, and its loop built-ins `break` and
/// `continue`, refused where a command name could stand (programmable
/// shell gate §4.2, §14).
const RESERVED: &[&str] = &[
    "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break",
    "continue",
];

/// A compound command's word, where a command name would stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    If,
    Then,
    Elif,
    Else,
    Fi,
    While,
    Until,
    Do,
    Done,
    For,
    In,
}

const KEYWORDS: &[(&str, Keyword)] = &[
    ("if", Keyword::If),
    ("then", Keyword::Then),
    ("elif", Keyword::Elif),
    ("else", Keyword::Else),
    ("fi", Keyword::Fi),
    ("while", Keyword::While),
    ("until", Keyword::Until),
    ("do", Keyword::Do),
    ("done", Keyword::Done),
    ("for", Keyword::For),
    ("in", Keyword::In),
];

impl Keyword {
    /// As typed.
    fn token(self) -> &'static str {
        KEYWORDS
            .iter()
            .find(|&&(_, k)| k == self)
            .map_or("", |&(w, _)| w)
    }

    /// The compound command it opens, if it opens one.
    fn opens(self) -> Option<Kind> {
        match self {
            Keyword::If => Some(Kind::If),
            Keyword::While => Some(Kind::While),
            Keyword::Until => Some(Kind::Until),
            Keyword::For => Some(Kind::For),
            _ => None,
        }
    }
}

/// What compound command is being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    If,
    While,
    Until,
    For,
}

/// Where a compound command being read is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// An `if`'s or an `elif`'s condition, up to its `then`.
    Condition,
    /// The body after `then`.
    Then,
    /// The body after `else`.
    Else,
    /// A loop's body, after `do`.
    Body,
    /// A `for`'s header: the next word is its name; then `in` or `do` may
    /// come, on a later line too; then its words, to a `;` or a newline;
    /// then, past blank lines, its `do`.
    ForName,
    ForAfterName,
    ForWords,
    ForBeforeDo,
}

/// A compound command being read, and what was being read around it.
struct Open {
    /// The list it stands in, put back when it closes.
    outer: Items,
    /// The levels of nesting it counts ([`NESTING_MAX`]).
    levels: usize,
    /// The `!`s before it.
    bangs: usize,
    kind: Kind,
    stage: Stage,
    /// Its lists so far: each condition and the body after it.
    lists: Vec<List<Word>>,
    /// A `for`'s name and words.
    name: Option<Word>,
    words: Option<Vec<Word>>,
}

/// The characters a line is read from, and where each is.
struct Cursor<'l> {
    line: &'l str,
    /// Where `chars` starts in `line`.
    from: usize,
    chars: Peekable<CharIndices<'l>>,
}

impl<'l> Cursor<'l> {
    /// From byte `from` of `line` on.
    fn starting(line: &'l str, from: usize) -> Cursor<'l> {
        Cursor {
            line,
            from,
            chars: line[from..].char_indices().peekable(),
        }
    }

    fn next(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| c)
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    /// Takes the next character if it is `c`.
    fn next_if_eq(&mut self, c: char) -> bool {
        self.chars.next_if(|&(_, n)| n == c).is_some()
    }

    /// Where the next character is (the line's length at its end).
    fn pos(&mut self) -> usize {
        self.chars
            .peek()
            .map_or(self.line.len(), |&(i, _)| self.from + i)
    }
}

/// The text of a `${…}` up to its `}`, which it takes. As bash does, it
/// reads quotes, escapes and further `${…}` whole, so a `}` among them
/// does not end it; a quote left open is unterminated.
fn brace_text(cur: &mut Cursor<'_>) -> Result<String, ParseError> {
    let mut text = String::new();
    // The `${` opened inside and not yet closed.
    let mut open = 0usize;
    loop {
        let c = cur.next().ok_or(ParseError::UnclosedBrace)?;
        if c == '}' && open == 0 {
            return Ok(text);
        }
        text.push(c);
        match c {
            '}' => open -= 1,
            '$' if cur.next_if_eq('{') => {
                text.push('{');
                open += 1;
            }
            '\\' => text.push(cur.next().ok_or(ParseError::UnclosedBrace)?),
            '\'' | '"' => loop {
                let q = cur.next().ok_or(ParseError::UnterminatedQuote)?;
                text.push(q);
                if q == c {
                    break;
                }
                if q == '\\' && c == '"' {
                    text.push(cur.next().ok_or(ParseError::UnterminatedQuote)?);
                }
            },
            _ => {}
        }
    }
}

/// A name starts with a letter or `_`.
fn starts_name(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn in_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `s` is a name (`[A-Za-z_][A-Za-z0-9_]*`).
pub fn is_name(s: &str) -> bool {
    s.chars().next().is_some_and(starts_name) && s.chars().all(in_name)
}

/// bash's parameters that are not supported (`$*`, `$$`, `$!`, `$-`).
const OTHER_SPECIALS: &[char] = &['*', '$', '!', '-'];

/// The parameter after a `$` (inside double quotes if `quoted`), or none
/// for a `$` that stands for itself.
fn parameter(cur: &mut Cursor<'_>, quoted: bool) -> Result<Option<Param>, ParseError> {
    let Some(c) = cur.peek() else {
        return Ok(None);
    };
    let param = match c {
        c if starts_name(c) => {
            let mut name = String::new();
            while let Some(c) = cur.peek().filter(|&c| in_name(c)) {
                cur.next();
                name.push(c);
            }
            if name == "_" {
                // bash's last argument of the command before.
                return Err(ParseError::Unsupported("$_".into()));
            }
            return Ok(Some(Param::Name(name)));
        }
        '0'..='9' => Param::Arg(usize::from(c as u8 - b'0')),
        '#' => Param::Count,
        '@' => Param::All,
        '?' => Param::Status,
        '{' => {
            cur.next();
            return braced(cur).map(Some);
        }
        c if OTHER_SPECIALS.contains(&c) => {
            return Err(ParseError::Unsupported(format!("${c}")));
        }
        '(' => {
            cur.next();
            let what = if cur.peek() == Some('(') { "$((" } else { "$(" };
            return Err(ParseError::Unsupported(what.into()));
        }
        // bash's old arithmetic, `$[1+1]`.
        '[' => return Err(ParseError::Unsupported("$[".into())),
        // `$'…'` and `$"…"` are bash's quotes of other kinds.
        '\'' | '"' if !quoted => return Err(ParseError::Unsupported(format!("${c}"))),
        _ => return Ok(None),
    };
    cur.next();
    Ok(Some(param))
}

/// The parameter of a `${…}`, its `{` taken: a name, a number or one of
/// `#`, `@`, `?`. bash's operators (`${A:-x}`, `${#A}`) are unsupported,
/// and anything else is a bad substitution.
fn braced(cur: &mut Cursor<'_>) -> Result<Param, ParseError> {
    let inside = brace_text(cur)?;
    let typed = format!("${{{inside}}}");
    let head = match inside.chars().next() {
        Some(c) if starts_name(c) => inside.find(|c| !in_name(c)),
        Some('0'..='9') => inside.find(|c: char| !c.is_ascii_digit()),
        Some(c) => Some(c.len_utf8()),
        None => return Ok(Param::Bad(typed)),
    }
    .unwrap_or(inside.len());
    let (name, rest) = inside.split_at(head);
    if !rest.is_empty() {
        let operator =
            rest.starts_with([':', '-', '=', '+', '?', '%', '/', '^', ',', '#', '[', '@']);
        return if operator || name == "#" || name == "!" {
            Err(ParseError::Unsupported(typed))
        } else {
            Ok(Param::Bad(typed))
        };
    }
    Ok(match name {
        "#" => Param::Count,
        "@" => Param::All,
        "?" => Param::Status,
        "_" => return Err(ParseError::Unsupported(typed)),
        n if is_name(n) => Param::Name(n.into()),
        // A number too big for any argument names none.
        n if n.bytes().all(|b| b.is_ascii_digit()) => Param::Arg(n.parse().unwrap_or(usize::MAX)),
        n if n.chars().all(|c| OTHER_SPECIALS.contains(&c)) => {
            return Err(ParseError::Unsupported(typed));
        }
        _ => Param::Bad(typed),
    })
}

/// An unquoted piece of an assignment's value with each `~` made `/root`
/// that is at the value's start (`first`) or after a `:`, and before a `/`,
/// a `:` or the value's end (`last`).
fn value_tildes(text: &str, first: bool, last: bool) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut after_colon = first;
    while let Some(c) = chars.next() {
        let ends = match chars.peek() {
            Some(&n) => n == '/' || n == ':',
            None => last,
        };
        if c == '~' && after_colon && ends {
            out.push_str(HOME);
        } else {
            out.push(c);
        }
        after_colon = c == ':';
    }
    out
}

/// A command of `words` and `redirects`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(words: Vec<Word>, redirects: Vec<Redirect<Word>>) -> Result<Command<Word>, ParseError> {
    let mut leading = words
        .iter()
        .take_while(|w| w.assignment().is_some() || w.appends());
    if let Some(append) = leading.find(|w| w.appends()) {
        return Err(ParseError::Unsupported(append.typed.clone()));
    }
    if let Some(first) = words.first()
        && first.assignment().is_some()
        && words.iter().any(|w| w.assignment().is_none())
    {
        return Err(ParseError::Unsupported(format!(
            "{} before a command",
            first.typed
        )));
    }
    Ok(Command { words, redirects })
}

/// A word being built.
#[derive(Default)]
struct Building {
    word: Word,
    /// Something (even `''`) was seen, so the word exists even if empty.
    started: bool,
    /// The word began with an unquoted `~`.
    tilde: bool,
    /// Where it starts and ends in the line.
    start: usize,
    end: usize,
    /// How many characters and parameters went into it.
    added: usize,
}

impl Building {
    /// Adds a parameter, in double quotes or not.
    fn param(&mut self, param: Param, quoted: bool) {
        self.started = true;
        self.added += 1;
        self.word.pieces.push(Piece::Param(param, quoted));
    }

    /// A quote opens: the word exists. Returns how much it holds.
    fn open_quote(&mut self) -> usize {
        self.started = true;
        self.added
    }

    /// The quote opened at `before` closes: with nothing in it (`''`), the
    /// word still has a quoted piece, so it is a word even when empty
    /// (`"$@"`, with no arguments, adds nothing and is none).
    fn close_quote(&mut self, before: usize) {
        if self.added == before && !matches!(self.word.pieces.last(), Some(Piece::Text(_, true))) {
            self.word.pieces.push(Piece::Text(String::new(), true));
        }
    }

    /// Adds a quoted or escaped character.
    fn quoted(&mut self, c: char) {
        self.started = true;
        self.added += 1;
        self.word.push(c, true);
    }

    /// The word, its `~` (alone, or before a `/` in the same unquoted
    /// piece) made `/root`, as bash's is (`~"/x"` and `~$A` keep it).
    fn finish(self, line: &str) -> Option<Word> {
        if !self.started {
            return None;
        }
        let mut word = self.word;
        let alone = word.pieces.len() == 1;
        if self.tilde
            && let Some(Piece::Text(first, false)) = word.pieces.first_mut()
            && ((alone && first == "~") || first.starts_with("~/"))
        {
            first.replace_range(..1, HOME);
        }
        word.typed = String::from(&line[self.start..self.end]);
        Some(word)
    }
}

/// Words and redirection collected so far.
#[derive(Default)]
struct Parts {
    words: Vec<Word>,
    redirects: Vec<Redirect<Word>>,
    /// A redirection waiting for its word.
    pending: Option<Pending>,
    /// How many `!` stood before the pipeline's first command.
    bangs: usize,
    /// The command is not the pipeline's first, so a `!` cannot stand
    /// before it.
    later: bool,
    /// The compound command just read, which only an operator or a
    /// keyword may follow.
    compound: Option<Compound<Word>>,
}

/// A redirection operator waiting for its word.
#[derive(Clone, Copy)]
enum Pending {
    /// `<` on fd 0: a file name.
    Read,
    /// `>` or `>>` (`append`) on `fd`: a file name.
    File { fd: u32, append: bool },
    /// `>&` on `fd`, as typed (`2>&` or `>&`): the fd it copies.
    Copy { fd: u32, typed: &'static str },
}

impl Parts {
    /// Nothing of a command has been read (a `!` may have been).
    fn is_empty(&self) -> bool {
        self.words.is_empty() && self.redirects.is_empty() && self.compound.is_none()
    }

    /// Ends a word: it becomes the pending redirection's target or the next
    /// word, or it is the keyword it returns, standing where a command name
    /// would.
    fn end_word(&mut self, word: &mut Building, line: &str) -> Result<Option<Keyword>, ParseError> {
        let Some(w) = core::mem::take(word).finish(line) else {
            return Ok(None);
        };
        match self.pending.take() {
            Some(Pending::Read) => self.redirects.push(Redirect {
                fd: 0,
                op: RedirectOp::Read(w),
            }),
            Some(Pending::File { fd, append }) => {
                let op = if append {
                    RedirectOp::Append(w)
                } else {
                    RedirectOp::Write(w)
                };
                self.redirects.push(Redirect { fd, op });
            }
            // Only a bare `1` or `2`: bash expands the word, and takes a
            // file, `-` or another fd too (§15 item 5).
            Some(Pending::Copy { fd, typed }) => {
                let copied = match w.digits() {
                    Some("1") => 1,
                    Some("2") => 2,
                    _ => return Err(ParseError::Unsupported(format!("{typed}{}", w.typed))),
                };
                self.redirects.push(Redirect {
                    fd,
                    op: RedirectOp::Copy(copied),
                });
            }
            // After a compound command, as in bash, only a keyword that
            // closes or goes on with the one around it may come (`fi fi`,
            // `fi then`), and after its redirections no word at all (`fi >
            // f fi` is bash's error, probes/p11.txt).
            None if self.compound.is_some() => {
                return match w.keyword() {
                    Some(k) if k.opens().is_none() && self.redirects.is_empty() => Ok(Some(k)),
                    _ => Err(ParseError::Unexpected(w.typed)),
                };
            }
            // A `!` before anything of the command negates the pipeline
            // (programmable shell gate §4.1), only the first command's.
            None if w.is_bang() && self.words.is_empty() && self.redirects.is_empty() => {
                if self.later {
                    return Err(ParseError::MissingTarget("!"));
                }
                self.bangs += 1;
            }
            None if self.words.is_empty() && self.redirects.is_empty() => {
                if let Some(k) = w.keyword() {
                    return Ok(Some(k));
                }
                if w.is_reserved() {
                    return Err(ParseError::Unsupported(w.typed));
                }
                self.words.push(w);
            }
            None => self.words.push(w),
        }
        Ok(None)
    }

    /// The command so far, ended by a `|`, which needs one before it.
    /// Every command of a pipeline has a name: a redirection alone, which
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command<Word>, ParseError> {
        // bash runs it in a subshell.
        if let Some(c) = &self.compound {
            return Err(ParseError::Unsupported(format!("| after {}", c.end())));
        }
        if self.pending.is_some() || (self.words.is_empty() && self.redirects.is_empty()) {
            return Err(ParseError::MissingTarget("|"));
        }
        // `<` may stand on the first command only, `>` and `>>` on the last
        // (programmable shell gate §7.3); errors may go anywhere.
        if self.later && self.redirects.iter().any(|r| r.fd == 0) {
            return Err(ParseError::Unsupported("< after |".into()));
        }
        // A redirection alone, which bash runs, is refused like an output
        // file: each names what was typed.
        let refused = match self.redirects.first() {
            Some(first) if self.words.is_empty() => Some(first),
            _ => self.redirects.iter().find(|r| r.is_output_file()),
        };
        if let Some(r) = refused {
            return Err(ParseError::Unsupported(format!(
                "{} before |",
                r.operator()
            )));
        }
        let p = core::mem::take(self);
        self.bangs = p.bangs;
        self.later = true;
        command(p.words, p.redirects)
    }
}

/// The commands of the first pipeline of `line`, whether or not it ends
/// with `&`, their words expanded with no variables set (for callers that
/// run no shell: tests); a line that does not expand is
/// `ParseError::Expansion`. A blank line is one command without words; a
/// compound command has none of its own.
pub fn parse(line: &str) -> Result<Vec<Command>, ParseError> {
    let list = parse_line(line)?;
    let Some(item) = list.items.first() else {
        return Ok(alloc::vec![Command {
            words: Vec::new(),
            redirects: Vec::new(),
        }]);
    };
    let Run::Commands(commands) = &item.and_or.first.run else {
        return Ok(Vec::new());
    };
    crate::expand::plain(commands).map_err(|e| ParseError::Expansion(e.to_string()))
}

/// The pipeline the command so far ends, at `end` (`;` or `&`, as
/// bash's error names it); none if nothing was typed since the last item.
fn end_pipeline(
    parts: &mut Parts,
    pipeline: &mut Vec<Command<Word>>,
    end: &'static str,
) -> Result<Option<Pipeline<Word>>, ParseError> {
    if parts.pending.is_some() {
        return Err(ParseError::MissingTarget(end));
    }
    if let Some(c) = parts.compound.take() {
        let p = core::mem::take(parts);
        return Ok(Some(Pipeline {
            negated: p.bangs % 2 == 1,
            run: Run::Compound(Box::new(c)),
            redirects: p.redirects,
        }));
    }
    if parts.words.is_empty() {
        match (pipeline.is_empty(), parts.redirects.is_empty()) {
            // `!` alone is a command that does nothing, negated.
            (true, true) if parts.bangs == 0 => return Ok(None),
            (true, _) => {}
            (false, true) => return Err(ParseError::MissingTarget(end)),
            (false, false) => {
                let first = parts.redirects.first().map(Redirect::operator);
                return Err(ParseError::Unsupported(format!(
                    "| {}",
                    first.unwrap_or_default()
                )));
            }
        }
    }
    let p = core::mem::take(parts);
    if !pipeline.is_empty() && p.redirects.iter().any(|r| r.fd == 0) {
        return Err(ParseError::Unsupported("< after |".into()));
    }
    pipeline.push(command(p.words, p.redirects)?);
    Ok(Some(Pipeline {
        negated: p.bangs % 2 == 1,
        run: Run::Commands(core::mem::take(pipeline)),
        redirects: Vec::new(),
    }))
}

impl Connector {
    /// As typed.
    fn token(self) -> &'static str {
        match self {
            Connector::And => "&&",
            Connector::Or => "||",
        }
    }
}

/// The items read so far, and the and-or list being read.
#[derive(Default)]
struct Items {
    items: Vec<Item<Word>>,
    and_or: Option<AndOr<Word>>,
    /// The `&&` or `||` after the and-or list, waiting for its next
    /// pipeline.
    connector: Option<Connector>,
}

impl Items {
    /// A pipeline that ended: the start of an and-or list, or the one the
    /// waiting connector joins to it.
    fn pipeline(&mut self, p: Pipeline<Word>) {
        match (&mut self.and_or, self.connector.take()) {
            (Some(and_or), Some(c)) => and_or.rest.push((c, p)),
            _ => {
                self.and_or = Some(AndOr {
                    first: p,
                    rest: Vec::new(),
                })
            }
        }
    }

    /// The and-or list ends an item; `background` is its text if it
    /// ended with `&`.
    fn end(&mut self, background: Option<String>) {
        if let Some(and_or) = self.and_or.take() {
            self.items.push(Item { and_or, background });
        }
    }
}

/// A command's text being parsed (programmable shell gate §4.3): what has
/// been read of it, kept from one call to the next, so that a reader can
/// give it a command's lines one at a time and each one is read once, in
/// its context.
#[derive(Default)]
pub struct Parser {
    /// The text read so far, which words and a background job's text are
    /// cut from.
    text: String,
    items: Items,
    /// Where the item being read starts in the text.
    item_start: usize,
    /// Where each comment starts and ends in the text.
    comments: Vec<(usize, usize)>,
    pipeline: Vec<Command<Word>>,
    parts: Parts,
    word: Building,
    /// The compound commands being read, the innermost last.
    open: Vec<Open>,
    /// How many bytes it has read (the tests count them).
    #[cfg(test)]
    pub(crate) read: usize,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// How long the text read so far is.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Nothing has been read.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The text read so far, with the newline of each line.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Reads `line` and its newline: the list the text makes if the line
    /// finishes it, none while it needs more lines (it ends after `|`,
    /// `&&` or `||`), or why it does not parse. Once it gives a list the
    /// parser starts afresh.
    pub fn line(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        let from = self.text.len();
        self.text.push_str(line);
        self.text.push('\n');
        self.read(from)?;
        if !self.pipeline.is_empty() || self.items.connector.is_some() || !self.open.is_empty() {
            return Ok(None);
        }
        let items = core::mem::take(&mut self.items.items);
        *self = Parser::new();
        Ok(Some(List { items }))
    }

    /// Reads the text from `from` on.
    fn read(&mut self, from: usize) -> Result<(), ParseError> {
        let text = core::mem::take(&mut self.text);
        #[cfg(test)]
        {
            self.read += text.len() - from;
        }
        let read = self.read_text(&text, from);
        self.text = text;
        read
    }

    /// `;` ends an item, and `&&` and `||` join pipelines, as bash's do;
    /// one with nothing typed before it is bash's syntax error naming it.
    fn read_text(&mut self, line: &str, from: usize) -> Result<(), ParseError> {
        let mut cur = Cursor::starting(line, from);
        loop {
            let at = cur.pos();
            let Some(c) = cur.next() else {
                break;
            };
            // The word before an operator ends at it (the `>` arm first
            // looks for an fd, as in `2>`); in a `for`'s header, which that
            // word may open or end, the operator is the header's.
            if matches!(c, '>' | '<' | '|' | ';' | '&' | '\n') {
                let redirection = matches!(c, '>' | '<');
                if !redirection || !(self.word.started && self.word.word.digits().is_some()) {
                    self.end_word(line, at)?;
                }
                if let Some(stage) = self.for_header() {
                    // A redirection there is bash's error, naming its fd if
                    // it has one (`for x in a 2>f`).
                    if redirection
                        && self.word.started
                        && let Some(digits) = self.word.word.digits()
                    {
                        return Err(ParseError::Unexpected(digits.into()));
                    }
                    self.for_operator(stage, c, &mut cur)?;
                    self.item_start = cur.pos();
                    continue;
                }
            }
            match c {
                ' ' | '\t' => self.end_word(line, at)?,
                '>' => {
                    // A word of digits just before it is its fd, as in bash,
                    // even where a file name is awaited (`>2>f` is bash's
                    // error naming the `2`); only 1 and 2 are taken.
                    let (fd, typed) = match self.fd_word()? {
                        Some(digits) => match digits.as_str() {
                            "1" => (1, "1"),
                            "2" => (2, "2"),
                            _ => return Err(ParseError::Unsupported(format!("{digits}>"))),
                        },
                        None => (1, ""),
                    };
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget(">"));
                    }
                    let append = cur.next_if_eq('>');
                    // `>&N` copies fd N; `>>&` and `> &` are bash's syntax
                    // errors (the `&` then meets a redirection without its
                    // word). bash's `>|` ignores `noclobber`.
                    let copy = !append && cur.next_if_eq('&');
                    if !append && !copy && cur.next_if_eq('|') {
                        return Err(ParseError::Unsupported(">|".into()));
                    }
                    self.parts.pending = Some(if copy {
                        let typed = match typed {
                            "1" => "1>&",
                            "2" => "2>&",
                            _ => ">&",
                        };
                        Pending::Copy { fd, typed }
                    } else {
                        Pending::File { fd, append }
                    });
                }
                '|' if cur.next_if_eq('|') => {
                    self.end_word(line, at)?;
                    join(
                        &mut self.items,
                        &mut self.parts,
                        &mut self.pipeline,
                        Connector::Or,
                    )?;
                }
                '|' => {
                    // bash's `|&` pipes the errors too.
                    if cur.next_if_eq('&') {
                        return Err(ParseError::Unsupported("|&".into()));
                    }
                    self.end_word(line, at)?;
                    self.pipeline.push(self.parts.take_before_pipe()?);
                }
                ';' => {
                    // bash's `;;`, `;&` and `;;&` end a `case` branch.
                    if cur.next_if_eq(';') {
                        let token = if cur.next_if_eq('&') { ";;&" } else { ";;" };
                        return Err(ParseError::MissingTarget(token));
                    }
                    if cur.next_if_eq('&') {
                        return Err(ParseError::MissingTarget(";&"));
                    }
                    self.end_word(line, at)?;
                    match end_pipeline(&mut self.parts, &mut self.pipeline, ";")? {
                        Some(p) => self.items.pipeline(p),
                        None => return Err(ParseError::MissingTarget(";")),
                    }
                    self.items.end(None);
                    self.item_start = cur.pos();
                }
                '&' if cur.next_if_eq('&') => {
                    self.end_word(line, at)?;
                    join(
                        &mut self.items,
                        &mut self.parts,
                        &mut self.pipeline,
                        Connector::And,
                    )?;
                }
                '<' => {
                    // As at `>`: `0<` is fd 0; bash's `1<`, `2<` and others
                    // are refused, as are its here-documents, `<&` and `<>`.
                    if let Some(digits) = self.fd_word()?
                        && digits != "0"
                    {
                        return Err(ParseError::Unsupported(format!("{digits}<")));
                    }
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("<"));
                    }
                    if cur.next_if_eq('<') {
                        let op = if cur.next_if_eq('<') { "<<<" } else { "<<" };
                        return Err(ParseError::Unsupported(op.into()));
                    }
                    for (c, op) in [('&', "<&"), ('>', "<>")] {
                        if cur.next_if_eq(c) {
                            return Err(ParseError::Unsupported(op.into()));
                        }
                    }
                    self.parts.pending = Some(Pending::Read);
                }
                // bash's `&>` sends both outputs to a file.
                '&' if cur.peek() == Some('>') => {
                    return Err(ParseError::Unsupported("&>".into()));
                }
                '&' => {
                    self.end_word(line, at)?;
                    // bash runs it in a subshell of its own.
                    if let Some(c) = &self.parts.compound {
                        return Err(ParseError::Unsupported(format!("& after {}", c.end())));
                    }
                    if self.parts.pending.is_some()
                        || self.parts.words.is_empty() && self.parts.redirects.is_empty()
                    {
                        return Err(ParseError::MissingTarget("&"));
                    }
                    if self.parts.words.is_empty() {
                        // `> f &`: a background job is a program.
                        return Err(ParseError::Unsupported("> &".into()));
                    }
                    // bash runs the whole and-or list in the background, in a
                    // shell of its own.
                    if let Some(c) = self.items.connector {
                        return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                    }
                    // Without its `!`, as bash's `jobs` shows it.
                    let typed = job_text(line, self.item_start, at, &self.comments);
                    let mut text = typed.as_str();
                    for _ in 0..self.parts.bangs {
                        text = text
                            .strip_prefix('!')
                            .unwrap_or(text)
                            .trim_start_matches([' ', '\t']);
                    }
                    let text = String::from(text);
                    if let Some(p) = end_pipeline(&mut self.parts, &mut self.pipeline, "&")? {
                        self.items.pipeline(p);
                    }
                    // The line goes on after it, as bash's does (`a & b`).
                    self.items.end(Some(text));
                    self.item_start = cur.pos();
                }
                '\'' => {
                    let before = self.word.open_quote();
                    loop {
                        match cur.next() {
                            Some('\'') => break,
                            Some(c) => self.word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
                    }
                    self.word.close_quote(before);
                }
                '"' => {
                    let before = self.word.open_quote();
                    loop {
                        match cur.next() {
                            Some('"') => break,
                            Some('\\') if matches!(cur.peek(), Some('"' | '\\' | '$' | '`')) => {
                                self.word.quoted(cur.next().expect("peeked"));
                            }
                            Some('$') => match parameter(&mut cur, true)? {
                                Some(p) => self.word.param(p, true),
                                None => self.word.quoted('$'),
                            },
                            Some('`') => return Err(ParseError::Unsupported('`'.into())),
                            Some(c) => self.word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
                    }
                    self.word.close_quote(before);
                }
                // bash joins a line ending in `\` to the next; here, as before
                // a command could go on to another line, it is an error.
                '\\' => match cur.next() {
                    Some('\n') | None => return Err(ParseError::TrailingBackslash),
                    Some(c) => self.word.quoted(c),
                },
                '$' => match parameter(&mut cur, false)? {
                    Some(p) => self.word.param(p, false),
                    None => {
                        self.word.started = true;
                        self.word.added += 1;
                        self.word.word.push('$', false);
                    }
                },
                // A comment runs to the end of the line.
                '#' if !self.word.started => {
                    while cur.peek().is_some_and(|c| c != '\n') {
                        cur.next();
                    }
                    self.comments.push((at, cur.pos()));
                }
                '\n' => {
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("newline"));
                    }
                    // After `|`, `&&` or `||` the command goes on, past blank
                    // and comment lines, as bash's does.
                    // (A `!` counts only before a pipeline's first command.)
                    let nothing = self.parts.is_empty()
                        && (self.parts.bangs == 0 || !self.pipeline.is_empty());
                    if nothing && (!self.pipeline.is_empty() || self.items.connector.is_some()) {
                        continue;
                    }
                    if let Some(p) = end_pipeline(&mut self.parts, &mut self.pipeline, "newline")? {
                        self.items.pipeline(p);
                    }
                    self.items.end(None);
                    self.item_start = cur.pos();
                }
                c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
                c => {
                    if !self.word.started && c == '~' {
                        self.word.tilde = true;
                    }
                    self.word.started = true;
                    self.word.added += 1;
                    self.word.word.push(c, false);
                }
            }
            if self.word.started {
                if self.word.end == 0 {
                    self.word.start = at;
                }
                self.word.end = cur.pos();
            }
        }
        Ok(())
    }

    /// The word being read, if it is all unquoted digits and so the fd of
    /// the redirection operator after it (programmable shell gate §7.1);
    /// it is taken. After an operator that awaits its file name it is
    /// bash's syntax error naming it.
    fn fd_word(&mut self) -> Result<Option<String>, ParseError> {
        if !self.word.started {
            return Ok(None);
        }
        let Some(digits) = self.word.word.digits() else {
            return Ok(None);
        };
        let digits = String::from(digits);
        match self.parts.pending {
            // The fd a `>&` copies (`2>&1>f`).
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. } | Pending::Read) => {
                return Err(ParseError::Unexpected(digits));
            }
            None => {}
        }
        self.word = Building::default();
        Ok(Some(digits))
    }

    /// The stage of the `for` whose header is being read.
    fn for_header(&self) -> Option<Stage> {
        self.open.last().map(|o| o.stage).filter(|s| {
            matches!(
                s,
                Stage::ForName | Stage::ForAfterName | Stage::ForWords | Stage::ForBeforeDo
            )
        })
    }

    /// A word of a `for`'s header (stage `stage`), which ends at `at`: its
    /// name, `in`, a word of its list or `do`; anything else is bash's
    /// error naming it.
    fn for_word(&mut self, stage: Stage, w: Word, at: usize) -> Result<(), ParseError> {
        let Some(open) = self.open.last_mut() else {
            return Err(ParseError::Unexpected(w.typed));
        };
        match (stage, w.keyword()) {
            (Stage::ForName, _) => {
                open.name = Some(w);
                open.stage = Stage::ForAfterName;
            }
            (Stage::ForAfterName, Some(Keyword::In)) => {
                open.words = Some(Vec::new());
                open.stage = Stage::ForWords;
            }
            (Stage::ForWords, _) => open.words.get_or_insert_with(Vec::new).push(w),
            (Stage::ForAfterName | Stage::ForBeforeDo, Some(Keyword::Do)) => {
                open.stage = Stage::Body;
                // A job's text starts after it, as after other keywords.
                self.item_start = at;
            }
            _ => return Err(ParseError::Unexpected(w.typed)),
        }
        Ok(())
    }

    /// An operator, `c` and what follows it in `cur`, in a `for`'s header
    /// (stage `stage`): a `;` or a newline ends its words, a newline right
    /// after `for` is bash's error, and every other one is.
    fn for_operator(
        &mut self,
        stage: Stage,
        c: char,
        cur: &mut Cursor<'_>,
    ) -> Result<(), ParseError> {
        let token = match c {
            '\n' => "newline",
            ';' if cur.next_if_eq(';') => {
                if cur.next_if_eq('&') {
                    ";;&"
                } else {
                    ";;"
                }
            }
            ';' if cur.next_if_eq('&') => ";&",
            ';' => ";",
            '|' if cur.next_if_eq('|') => "||",
            '&' if cur.next_if_eq('&') => "&&",
            '>' if cur.next_if_eq('>') => ">>",
            '<' if cur.next_if_eq('<') => "<<",
            '|' => "|",
            '&' => "&",
            '<' => "<",
            _ => ">",
        };
        let next = match (stage, token) {
            (Stage::ForAfterName | Stage::ForBeforeDo, "newline") => stage,
            (Stage::ForWords, "newline") => Stage::ForBeforeDo,
            (Stage::ForAfterName | Stage::ForWords, ";") => Stage::ForBeforeDo,
            _ => return Err(ParseError::MissingTarget(token)),
        };
        if let Some(open) = self.open.last_mut() {
            open.stage = next;
        }
        Ok(())
    }

    /// Ends the word being read, which ends at `at`; a keyword where one
    /// may stand opens, goes on with or closes a compound command.
    fn end_word(&mut self, line: &str, at: usize) -> Result<(), ParseError> {
        if let Some(stage) = self.for_header() {
            return match core::mem::take(&mut self.word).finish(line) {
                Some(w) => self.for_word(stage, w, at),
                None => Ok(()),
            };
        }
        match self.parts.end_word(&mut self.word, line)? {
            Some(k) => self.keyword(k, at),
            None => Ok(()),
        }
    }

    /// A keyword, ending at `at`, where a command name would stand.
    fn keyword(&mut self, k: Keyword, at: usize) -> Result<(), ParseError> {
        if let Some(kind) = k.opens() {
            // bash runs it in a subshell.
            if !self.pipeline.is_empty() {
                return Err(ParseError::Unsupported(format!("{} after |", k.token())));
            }
            let levels = 1 + usize::from(self.items.connector.is_some());
            if self.open.iter().map(|o| o.levels).sum::<usize>() + levels > NESTING_MAX {
                return Err(ParseError::Unsupported(format!(
                    "more than {NESTING_MAX} levels of nesting"
                )));
            }
            let bangs = core::mem::take(&mut self.parts).bangs;
            self.open.push(Open {
                outer: core::mem::take(&mut self.items),
                levels,
                bangs,
                kind,
                stage: match kind {
                    Kind::For => Stage::ForName,
                    _ => Stage::Condition,
                },
                lists: Vec::new(),
                name: None,
                words: None,
            });
            self.item_start = at;
            return Ok(());
        }
        // The stage the keyword moves the innermost one on to, or none if
        // it closes it.
        let next = match self.open.last().map(|o| (o.kind, o.stage, k)) {
            Some((Kind::If, Stage::Condition, Keyword::Then)) => Some(Stage::Then),
            Some((Kind::If, Stage::Then, Keyword::Elif)) => Some(Stage::Condition),
            Some((Kind::If, Stage::Then, Keyword::Else)) => Some(Stage::Else),
            Some((Kind::If, Stage::Then | Stage::Else, Keyword::Fi)) => None,
            Some((Kind::While | Kind::Until, Stage::Condition, Keyword::Do)) => Some(Stage::Body),
            Some((Kind::While | Kind::Until | Kind::For, Stage::Body, Keyword::Done)) => None,
            _ => return Err(ParseError::MissingTarget(k.token())),
        };
        let list = self.end_list(k.token())?;
        self.item_start = at;
        let Some(open) = self.open.last_mut() else {
            return Err(ParseError::MissingTarget(k.token()));
        };
        if let Some(next) = next {
            open.lists.push(list);
            open.stage = next;
            return Ok(());
        }
        let Some(open) = self.open.pop() else {
            return Err(ParseError::MissingTarget(k.token()));
        };
        let mut lists = open.lists;
        let compound = match open.kind {
            Kind::If => {
                let otherwise = match open.stage {
                    Stage::Else => Some(list),
                    _ => {
                        lists.push(list);
                        None
                    }
                };
                let mut branches = Vec::new();
                let mut lists = lists.into_iter();
                while let (Some(condition), Some(body)) = (lists.next(), lists.next()) {
                    branches.push((condition, body));
                }
                Compound::If(If {
                    branches,
                    otherwise,
                })
            }
            Kind::While | Kind::Until => {
                let Some(condition) = lists.pop() else {
                    return Err(ParseError::MissingTarget(k.token()));
                };
                let l = Loop {
                    condition,
                    body: list,
                };
                match open.kind {
                    Kind::Until => Compound::Until(l),
                    _ => Compound::While(l),
                }
            }
            Kind::For => {
                let Some(name) = open.name else {
                    return Err(ParseError::MissingTarget(k.token()));
                };
                Compound::For(For {
                    name,
                    words: open.words,
                    body: list,
                })
            }
        };
        self.items = open.outer;
        self.parts.bangs = open.bangs;
        self.parts.compound = Some(compound);
        Ok(())
    }

    /// The list a keyword (`token`) ends: it must hold something, and
    /// leave nothing open (bash's error names the keyword).
    fn end_list(&mut self, token: &'static str) -> Result<List<Word>, ParseError> {
        if self.parts.bangs > 0 && self.parts.is_empty() {
            return Err(ParseError::MissingTarget(token));
        }
        match end_pipeline(&mut self.parts, &mut self.pipeline, token)? {
            Some(p) => self.items.pipeline(p),
            None if self.items.connector.is_some() => return Err(ParseError::MissingTarget(token)),
            None => {}
        }
        self.items.end(None);
        let items = core::mem::take(&mut self.items).items;
        if items.is_empty() {
            return Err(ParseError::MissingTarget(token));
        }
        Ok(List { items })
    }

    /// The text ends: its list, or why it does not parse (a command that
    /// needs more lines is `ParseError::Incomplete`).
    fn end(mut self) -> Result<List<Word>, ParseError> {
        let text = core::mem::take(&mut self.text);
        self.end_word(&text, text.len())?;
        if !self.open.is_empty() {
            return Err(ParseError::Incomplete);
        }
        let Parser {
            items,
            pipeline,
            parts,
            ..
        } = &mut self;
        if parts.pending.is_some() {
            return Err(ParseError::MissingTarget("newline"));
        }
        if !pipeline.is_empty() && parts.words.is_empty() && parts.redirects.is_empty() {
            return Err(ParseError::Incomplete);
        }
        // Nothing after the last `;` (or at all) is no item.
        match end_pipeline(parts, pipeline, "newline")? {
            Some(p) => items.pipeline(p),
            None if items.connector.is_some() => return Err(ParseError::Incomplete),
            None => {}
        }
        items.end(None);
        Ok(List {
            items: core::mem::take(&mut items.items),
        })
    }
}

/// `line`'s list, its words as typed: the text read whole by a
/// [`Parser`].
pub fn parse_line(line: &str) -> Result<List<Word>, ParseError> {
    let mut parser = Parser {
        text: String::from(line),
        ..Parser::default()
    };
    parser.read(0)?;
    parser.end()
}

/// What was typed of a background job from `start` to `end` (its `&`), as
/// bash's `jobs` shows it: one line, its lines joined by a blank, without
/// its comments or blank lines.
fn job_text(line: &str, start: usize, end: usize, comments: &[(usize, usize)]) -> String {
    let mut typed = String::new();
    let mut from = start;
    for &(c, e) in comments.iter().filter(|&&(c, _)| c >= start && c < end) {
        typed.push_str(&line[from..c]);
        from = e;
    }
    typed.push_str(&line[from..end]);
    let lines: Vec<&str> = typed
        .split('\n')
        .map(|l| l.trim_matches([' ', '\t']))
        .filter(|l| !l.is_empty())
        .collect();
    lines.join(" ")
}

/// A `&&` or `||` (`connector`) ends the pipeline before it, which must
/// hold something, and waits for the next one.
fn join(
    items: &mut Items,
    parts: &mut Parts,
    pipeline: &mut Vec<Command<Word>>,
    connector: Connector,
) -> Result<(), ParseError> {
    // Not even after a `!`, as bash's grammar has it.
    if parts.is_empty() && pipeline.is_empty() {
        return Err(ParseError::MissingTarget(connector.token()));
    }
    match end_pipeline(parts, pipeline, connector.token())? {
        Some(p) => items.pipeline(p),
        None => return Err(ParseError::MissingTarget(connector.token())),
    }
    items.connector = Some(connector);
    Ok(())
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

    impl<W> Pipeline<W> {
        /// The commands of a pipeline that runs commands.
        pub fn commands(&self) -> &[Command<W>] {
            match &self.run {
                Run::Commands(c) => c,
                Run::Compound(_) => panic!("a compound command"),
            }
        }
    }

    /// The command name of each pipeline of each item of `list`, a compound
    /// command's as its first word (`if`).
    fn names_of(list: &List<Word>) -> Vec<String> {
        let mut names = Vec::new();
        for item in &list.items {
            for p in
                core::iter::once(&item.and_or.first).chain(item.and_or.rest.iter().map(|(_, p)| p))
            {
                names.push(match &p.run {
                    Run::Commands(c) => c[0].words[0].typed.clone(),
                    Run::Compound(c) => match **c {
                        Compound::If(_) => String::from("if"),
                        Compound::While(_) => String::from("while"),
                        Compound::Until(_) => String::from("until"),
                        Compound::For(_) => String::from("for"),
                    },
                });
            }
        }
        names
    }

    /// The `if` of `line`'s one item, and whether it is negated.
    fn if_of(line: &str) -> (bool, If<Word>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        match p.run {
            Run::Compound(c) => match *c {
                Compound::If(i) => (p.negated, i),
                _ => panic!("{line}: no if"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    /// The loop of `line`'s one item, `until` or not, its condition's and
    /// body's command names.
    fn loop_of(line: &str) -> (bool, Vec<String>, Vec<String>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        match p.run {
            Run::Compound(c) => match *c {
                Compound::While(l) => (false, names_of(&l.condition), names_of(&l.body)),
                Compound::Until(l) => (true, names_of(&l.condition), names_of(&l.body)),
                _ => panic!("{line}: no while or until"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    /// The `for` of `line`'s one item: its name and words as typed (none
    /// for `"$@"`), and its body's command names.
    fn for_of(line: &str) -> (String, Option<Vec<String>>, Vec<String>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        let Run::Compound(c) = p.run else {
            panic!("{line}: no compound command");
        };
        let Compound::For(f) = *c else {
            panic!("{line}: no for");
        };
        let words = f.words.map(|w| w.into_iter().map(|w| w.typed).collect());
        (f.name.typed, words, names_of(&f.body))
    }

    #[test]
    fn a_for_holds_its_name_its_words_and_its_body() {
        let s = |v: &[&str]| -> Vec<String> { v.iter().map(|x| String::from(*x)).collect() };
        let c = s(&["c"]);
        for (line, name, words) in [
            ("for x in a b; do c; done", "x", Some(s(&["a", "b"]))),
            // f11, l3: none; f10, r9, l10: over "$@".
            ("for x in; do c; done", "x", Some(s(&[]))),
            ("for x in\ndo c; done", "x", Some(s(&[]))),
            ("for x; do c; done", "x", None),
            ("for x do c; done", "x", None),
            ("for x\ndo c\ndone", "x", None),
            ("for x;\ndo c; done", "x", None),
            // p4, l4, l9: across lines.
            ("for x\nin a b\ndo c\ndone", "x", Some(s(&["a", "b"]))),
            ("for x in a\n\n# d\ndo c; done", "x", Some(s(&["a"]))),
            ("for x in a b;\ndo c; done", "x", Some(s(&["a", "b"]))),
            ("for x in a # d\ndo c; done", "x", Some(s(&["a"]))),
            // r6, f11: keywords among the words; f15: any word a name.
            (
                "for x in if then do; do c; done",
                "x",
                Some(s(&["if", "then", "do"])),
            ),
            ("for in in in; do c; done", "in", Some(s(&["in"]))),
            ("for done in a; do c; done", "done", Some(s(&["a"]))),
            (
                "for \"x\" in \"$@\" ${a}b; do c; done",
                "\"x\"",
                Some(s(&["\"$@\"", "${a}b"])),
            ),
        ] {
            assert_eq!(
                for_of(line),
                (String::from(name), words, c.clone()),
                "{line}"
            );
        }
        assert_eq!(
            for_of("for x in a; do for y in b; do c; done done").2,
            ["for"]
        );
        for text in [
            "for x",
            "for x in a b",
            "for x in a b;",
            "for x; do",
            "for x in a; do b",
        ] {
            assert_eq!(parse_line(text), Err(ParseError::Incomplete), "{text}");
        }
    }

    #[test]
    fn a_job_in_a_for_s_body_has_its_own_text() {
        // The prototype's review: the header went into the job's text, and
        // taking a `!` off it a byte at a time panicked past a non-ASCII
        // name.
        for (line, text) in [
            ("for x in a; do sleep 5 & done", "sleep 5"),
            ("for x do sleep 1 & done", "sleep 1"),
            ("for x in a; do ! sleep 1 & done", "sleep 1"),
            ("for \u{e9} do ! sleep 1 & done", "sleep 1"),
            ("for x in a b\ndo sleep 2 &\ndone", "sleep 2"),
        ] {
            let p = parse_line(line).unwrap().items.remove(0).and_or.first;
            let Run::Compound(c) = p.run else {
                panic!("{line}");
            };
            let Compound::For(f) = *c else {
                panic!("{line}");
            };
            assert_eq!(f.body.items[0].background.as_deref(), Some(text), "{line}");
        }
    }

    #[test]
    fn a_for_s_header_takes_no_operator() {
        for (line, token) in [
            ("for\n", "newline"),
            ("for\nx in a; do echo; done", "newline"),
            ("for x a; do echo; done", "a"),
            ("for in a; do echo; done", "a"),
            ("for x in a b do echo $x; done", "done"),
            ("for x in a\nb", "b"),
            ("for x in a # c\necho", "echo"),
            ("for x in a | b; do echo; done", "|"),
            ("for x in a || b; do echo; done", "||"),
            ("for x in a > f; do echo; done", ">"),
            ("for x > f; do echo; done", ">"),
            ("for x in a && b; do echo; done", "&&"),
            ("for x in a & do echo; done", "&"),
            ("for x |", "|"),
            ("for ; do echo; done", ";"),
            ("for x; ; do echo; done", ";"),
            ("for x; in a; do echo; done", "in"),
            ("for x;; do echo; done", ";;"),
            ("for x in a;; do echo; done", ";;"),
            ("for x in a; do; echo; done", ";"),
            ("for x in a; then echo; done", "then"),
            ("for x in a; do echo; fi", "fi"),
            // g16, g10, k10: `in` where a command name stands.
            ("in", "in"),
            ("if true; then echo in; in; fi", "in"),
            ("if a; then b; fi in", "in"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        for (line, what) in [
            ("for x in a; do b; done | cat", "| after done"),
            ("echo x | for x in a; do b; done", "for after |"),
            ("for x in a; do b; done &", "& after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn while_and_until_hold_a_condition_and_a_body() {
        let s = |v: &[&str]| -> Vec<String> { v.iter().map(|x| String::from(*x)).collect() };
        assert_eq!(
            loop_of("while a; b; do c; d; done"),
            (false, s(&["a", "b"]), s(&["c", "d"]))
        );
        assert_eq!(
            loop_of("until a && b; do c | d; done"),
            (true, s(&["a", "b"]), s(&["c"]))
        );
        // Across lines (l2, p5), nested with `if` (p8, p9), and after `done`
        // a keyword with no `;`.
        assert_eq!(
            loop_of("while\na\n\n# c\ndo b\ndone"),
            (false, s(&["a"]), s(&["b"]))
        );
        assert_eq!(
            loop_of("while a; do if b; then c; fi done"),
            (false, s(&["a"]), s(&["if"]))
        );
        assert_eq!(
            loop_of("until while a; do b; done do c; done"),
            (true, s(&["while"]), s(&["c"]))
        );
        let (_, i) = if_of("if a; then while b; do c; done fi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["while"]]);
        // `!`, lists and and-or lists.
        assert!(
            parse_line("! while a; do b; done").unwrap().items[0]
                .and_or
                .first
                .negated
        );
        let list = parse_line("a && while b; do c; done || d; until e; do f; done").unwrap();
        assert_eq!(names_of(&list), ["a", "while", "d", "until"]);
        // As words where no command name stands.
        assert_eq!(
            parse("echo while until do done").unwrap()[0].words,
            ["echo", "while", "until", "do", "done"]
        );
        for text in [
            "while",
            "while a",
            "while a;",
            "while a; do",
            "until a; do b",
            "while a; do b; done &&",
        ] {
            assert_eq!(parse_line(text), Err(ParseError::Incomplete), "{text}");
        }
    }

    #[test]
    fn a_loop_s_keyword_out_of_place_is_bash_s_syntax_error() {
        for (line, token) in [
            ("done", "done"),
            ("do", "do"),
            ("while true; done", "done"),
            ("while do echo; done", "do"),
            ("while false; do echo; done done", "done"),
            ("while a; do; echo; done", ";"),
            ("while a; then b; done", "then"),
            ("until a; do b; fi", "fi"),
            ("if a; then b; done", "done"),
            ("if a; do b; fi", "do"),
            ("while a; do b; done while", "while"),
            ("while a; do b; done echo", "echo"),
            ("while a; do b; else c; done", "else"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        for (line, what) in [
            ("while a; do b; done | cat", "| after done"),
            ("echo x | while a; do b; done", "while after |"),
            ("echo x | until a; do b; done", "until after |"),
            ("while a; do b; done &", "& after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    /// Each of an `if`'s lists, its command names.
    fn branches(i: &If<Word>) -> Vec<Vec<String>> {
        let mut lists: Vec<Vec<String>> = Vec::new();
        for (c, b) in &i.branches {
            lists.push(names_of(c));
            lists.push(names_of(b));
        }
        lists.extend(i.otherwise.iter().map(names_of));
        lists
    }

    #[test]
    fn an_if_holds_its_conditions_and_bodies() {
        let (negated, i) = if_of("if a; then b; c; elif d; then e; else f; fi");
        assert!(!negated);
        assert_eq!(
            branches(&i),
            [["a"].as_slice(), &["b", "c"], &["d"], &["e"], &["f"]]
        );
        assert!(i.otherwise.is_some());
        let (_, i) = if_of("if a && b; then c | d; fi");
        assert_eq!(branches(&i), [["a", "b"].as_slice(), &["c"]]);
        assert!(i.otherwise.is_none());
        // f3: `!` negates it.
        assert!(if_of("! if a; then b; fi").0);
        // Across lines (p1, l1), blank and comment lines among them (p5).
        let (_, i) = if_of("if\na\n\n# c\nthen b\nelse\nc\nfi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["b"], &["c"]]);
        // Nested, and after `fi` a keyword with no `;` (k1, k2).
        let (_, i) = if_of("if a; then if b; then c; fi fi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["if"]]);
        let (_, i) = if_of("if if a; then b; fi then c; fi");
        assert_eq!(branches(&i), [["if"].as_slice(), &["c"]]);
        // In a list and an and-or list (f2, g11, k4).
        let list = parse_line("if a; then b; fi; c && if d; then e; fi || f").unwrap();
        assert_eq!(names_of(&list), ["if", "c", "if", "f"]);
        let list = parse_line("a && if b; then c; fi\nd").unwrap();
        assert_eq!(names_of(&list), ["a", "if", "d"]);
        // h11: a background job in a body.
        let (_, i) = if_of("if a; then b & fi");
        assert_eq!(i.branches[0].1.items[0].background.as_deref(), Some("b"));
        let (_, i) = if_of("if a & then b; fi");
        assert_eq!(i.branches[0].0.items[0].background.as_deref(), Some("a"));
    }

    #[test]
    fn if_words_are_keywords_only_where_a_command_name_stands() {
        // f8, h8: as an argument, quoted or a redirection's target they
        // are words.
        for (line, words) in [
            (
                "echo if then elif else fi",
                &["echo", "if", "then", "elif", "else", "fi"][..],
            ),
            ("\"if\" x", &["if", "x"]),
            ("'then' x", &["then", "x"]),
            ("\\fi x", &["fi", "x"]),
            ("> then fi", &["fi"]),
            ("iffy", &["iffy"]),
        ] {
            assert_eq!(parse(line).unwrap()[0].words, words, "{line}");
        }
        let (_, i) = if_of("if a > then; then b; fi");
        assert_eq!(
            i.branches[0].0.items[0].and_or.first.commands()[0]
                .output()
                .unwrap()
                .0
                .typed,
            "then"
        );
    }

    #[test]
    fn an_if_not_finished_needs_more_lines() {
        for text in [
            "if",
            "if a",
            "if a;",
            "if a; then",
            "if a; then b",
            "if a; then b; elif c",
            "if a; then b; else",
            "if a; then b; else c",
            "if a; then if b; then c; fi",
            "if a; then b; fi &&",
            "a && if b; then c; fi |",
        ] {
            let e = parse_line(text);
            if text.ends_with('|') {
                assert_eq!(
                    e,
                    Err(ParseError::Unsupported("| after fi".into())),
                    "{text}"
                );
            } else {
                assert_eq!(e, Err(ParseError::Incomplete), "{text}");
            }
        }
        // A reader's lines: each line once, the `if` given at its `fi`.
        let mut p = Parser::new();
        assert_eq!(p.line("if a"), Ok(None));
        assert_eq!(p.line("then b"), Ok(None));
        assert_eq!(names_of(&p.line("fi; c").unwrap().unwrap()), ["if", "c"]);
        assert!(p.is_empty());
    }

    #[test]
    fn a_keyword_out_of_place_is_bash_s_syntax_error() {
        for (line, token) in [
            ("if then echo a; fi", "then"),
            ("if true; then fi", "fi"),
            ("if true; fi", "fi"),
            ("fi", "fi"),
            ("then", "then"),
            ("elif", "elif"),
            ("if true; then echo a; else fi", "fi"),
            ("if true; then echo a; elif then echo b; fi", "then"),
            ("if true; then echo a; fi; then", "then"),
            ("echo; else", "else"),
            ("if true; then echo a; fi if", "if"),
            ("if true; then echo a; fi ! true", "!"),
            ("if ! then echo a; fi", "then"),
            ("if true; then ! fi", "fi"),
            (
                "if true; then echo a; else echo b; elif true; then echo c; fi",
                "elif",
            ),
            ("if true; then echo a; else echo b; else echo c; fi", "else"),
            ("if true && then echo; fi", "then"),
            ("if true |\nthen echo; fi", "then"),
            ("if true; then echo a; fi; fi", "fi"),
            ("if true\nthen\nfi", "fi"),
            ("if a; then b; fi echo b", "echo"),
            ("if a; then b; fi 'x y'", "'x y'"),
            ("if a; then b; fi \"$x\"", "\"$x\""),
            ("if a; then b; fi ${x}y", "${x}y"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // Told on the line that has it.
        let mut p = Parser::new();
        assert_eq!(p.line("if true"), Ok(None));
        assert_eq!(p.line("then"), Ok(None));
        assert_eq!(p.line("fi"), Err(ParseError::MissingTarget("fi")));
    }

    /// `n` `if`s nested, each after `a &&` if `chained`.
    fn nested(n: usize, chained: bool) -> String {
        let open = if chained {
            "a && if b; then "
        } else {
            "if b; then "
        };
        alloc::format!("{}c{}", open.repeat(n), "; fi".repeat(n))
    }

    #[test]
    fn compound_commands_nest_at_most_32_levels_deep() {
        // The walker recurses on a fixed stack (§4.5): each one counts a
        // level, and one after `&&` or `||` one more.
        let too_deep = ParseError::Unsupported("more than 32 levels of nesting".into());
        let refused = Err(too_deep.clone());
        assert!(parse_line(&nested(32, false)).is_ok());
        assert_eq!(parse_line(&nested(33, false)), refused);
        assert!(parse_line(&nested(16, true)).is_ok());
        assert_eq!(parse_line(&nested(17, true)), refused);
        let mixed = alloc::format!("{}a || {}", "if b; then ".repeat(31), nested(1, false));
        assert_eq!(parse_line(&(mixed + &"; fi".repeat(31))), refused);
        // Levels close with their constructs: one after another is none.
        assert!(parse_line(&"if b; then c; fi; ".repeat(100)).is_ok());
        assert_eq!(
            too_deep.to_string(),
            "unsupported syntax: more than 32 levels of nesting"
        );
    }

    #[test]
    fn an_if_in_a_pipeline_or_with_ampersand_is_refused() {
        // bash runs them in a subshell (f4–f7).
        for (line, what) in [
            ("if true; then echo a; fi | cat", "| after fi"),
            ("echo x | if true; then cat; fi", "if after |"),
            ("if true; then echo a; fi &", "& after fi"),
            ("if true; then echo a; fi & b", "& after fi"),
            ("if a; then if b; then c; fi | d; fi", "| after fi"),
            ("if true; then echo a; fi > f | cat", "| after fi"),
            ("for x in a; do b; done 2>&1 &", "& after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn a_compound_command_takes_redirections_after_its_end() {
        // Programmable shell gate §7.4, bash 5.2 (tmp/m5p1/probes/p11.txt).
        let redirects = |line: &str| -> Vec<String> {
            let first = parse_line(line).unwrap().items.remove(0).and_or.first;
            assert!(matches!(first.run, Run::Compound(_)), "{line}");
            first
                .redirects
                .iter()
                .map(|r| match &r.op {
                    RedirectOp::Read(w) => format!("{}<{}", r.fd, w.typed),
                    RedirectOp::Write(w) => format!("{}>{}", r.fd, w.typed),
                    RedirectOp::Append(w) => format!("{}>>{}", r.fd, w.typed),
                    RedirectOp::Copy(from) => format!("{}>&{from}", r.fd),
                })
                .collect()
        };
        assert_eq!(redirects("if true; then echo a; fi > f"), ["1>f"]);
        assert_eq!(redirects("while a; do b; done < f 2>&1"), ["0<f", "2>&1"]);
        assert_eq!(
            redirects("for x in a; do b; done >> f 2> e"),
            ["1>>f", "2>e"]
        );
        assert_eq!(redirects("until a; do b; done 2>e"), ["2>e"]);
        assert!(parse_line("if a; then for x in y; do b; done > f; fi").is_ok());
        assert!(parse_line("if a; then b; fi > f && c").is_ok());
        // Only an operator may follow them, and they need their words.
        for (line, token) in [
            ("if a; then if b; then c; fi > f fi", "fi"),
            ("if a; then b; fi > f x", "x"),
            ("if a; then b; fi 2 > f", "2"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unexpected(token.into())),
                "{line}"
            );
        }
        for (line, token) in [
            ("if a; then b; fi >", "newline"),
            ("if a; then b; fi > ; c", ";"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::MissingTarget(token)),
                "{line}"
            );
        }
    }
    /// The commands of `line`'s one pipeline, as typed.
    fn typed(line: &str) -> Vec<Command<Word>> {
        parse_line(line)
            .unwrap()
            .items
            .remove(0)
            .and_or
            .first
            .commands()
            .to_vec()
    }

    /// The background text of `line`'s one item.
    fn background(line: &str) -> Option<String> {
        parse_line(line).unwrap().items.remove(0).background
    }

    fn words(line: &str) -> Vec<String> {
        let c = one(line).unwrap();
        assert_eq!(c.redirects, []);
        c.words
    }

    #[test]
    fn a_line_is_a_list_of_items() {
        // Blanks or a comment hold none; a pipeline is one item, with the
        // text of a background job.
        assert_eq!(parse_line(" \t# x").unwrap().items, []);
        let list = parse_line("cat f | wc &").unwrap();
        assert_eq!(list.items.len(), 1);
        let item = &list.items[0];
        assert_eq!(item.background.as_deref(), Some("cat f | wc"));
        assert_eq!(item.and_or.rest, []);
        let names: Vec<&str> = item
            .and_or
            .first
            .commands()
            .iter()
            .map(|c| c.words[0].typed.as_str())
            .collect();
        assert_eq!(names, ["cat", "wc"]);
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
        assert_eq!(c.output().unwrap().0, "f");
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
    fn a_word_keeps_which_of_its_pieces_were_quoted() {
        let text = |t: &str, quoted| Piece::Text(t.into(), quoted);
        let c = typed(r#"a'b c'\d"e" '' "" ~/x"#);
        let pieces: Vec<&[Piece]> = c[0].words.iter().map(|w| &w.pieces[..]).collect();
        assert_eq!(
            pieces,
            [
                &[text("a", false), text("b cde", true)][..],
                &[text("", true)],
                &[text("", true)],
                &[text("/root/x", false)],
            ]
        );
        let c = &typed("echo >'o'ut")[0];
        assert_eq!(
            c.output().unwrap().0.pieces,
            [text("o", true), text("ut", false)]
        );
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
    fn backquotes_are_unsupported_inside_double_quotes_too() {
        // Bash runs the command there too; passing it on as text would
        // print something bash never prints.
        assert_eq!(
            one(r#"echo "`date`""#),
            Err(ParseError::Unsupported("`".into()))
        );
        assert_eq!(words(r"echo '$HOME `x`'"), ["echo", "$HOME `x`"]);
    }

    /// The pieces of `line`'s words.
    fn pieces(line: &str) -> Vec<Vec<Piece>> {
        typed(line)[0]
            .words
            .iter()
            .map(|w| w.pieces.clone())
            .collect()
    }

    #[test]
    fn parameters_are_pieces_of_their_words() {
        let text = |t: &str, quoted| Piece::Text(t.into(), quoted);
        let name = |n: &str, quoted| Piece::Param(Param::Name(n.into()), quoted);
        assert_eq!(
            pieces(r#"$A ${B}x "$C1 $_D" $1$# "$@" $? $10 ${10} ${99999999999999999999}"#),
            [
                vec![name("A", false)],
                vec![name("B", false), text("x", false)],
                vec![name("C1", true), text(" ", true), name("_D", true)],
                vec![
                    Piece::Param(Param::Arg(1), false),
                    Piece::Param(Param::Count, false)
                ],
                vec![Piece::Param(Param::All, true)],
                vec![Piece::Param(Param::Status, false)],
                vec![Piece::Param(Param::Arg(1), false), text("0", false)],
                vec![Piece::Param(Param::Arg(10), false)],
                vec![Piece::Param(Param::Arg(usize::MAX), false)],
            ]
        );
        // `${#}`, `${@}` and `${?}` are `$#`, `$@` and `$?`.
        assert_eq!(
            pieces("${#} ${@} ${?}"),
            [
                [Piece::Param(Param::Count, false)],
                [Piece::Param(Param::All, false)],
                [Piece::Param(Param::Status, false)],
            ]
        );
    }

    #[test]
    fn parse_says_why_a_line_does_not_expand() {
        // The review found it panicking, though it is public.
        assert_eq!(
            parse("echo ${1A}"),
            Err(ParseError::Expansion("${1A}: bad substitution".into()))
        );
        assert_eq!(
            parse("echo > $E").unwrap_err().to_string(),
            "$E: ambiguous redirect"
        );
        assert_eq!(parse("echo $E x").unwrap()[0].words, ["echo", "x"]);
    }

    #[test]
    fn a_substitution_starting_with_any_character_is_read_whole() {
        // The review found `${é}` panicking the shell (a multi-byte
        // character cut in two); bash's `bad substitution`.
        let bad = |t: &str| vec![Piece::Param(Param::Bad(t.into()), false)];
        assert_eq!(
            pieces("${é} ${€x} ${ é}"),
            [bad("${é}"), bad("${€x}"), bad("${ é}")]
        );
        assert_eq!(
            pieces("\"${…}\""),
            [vec![Piece::Param(Param::Bad("${…}".into()), true)]]
        );
    }

    #[test]
    fn a_dollar_before_no_parameter_is_a_dollar() {
        // As in bash.
        assert_eq!(
            words(r#"echo $ a$ $% $/x $. "$" "a $ b" "$'" $=1 $:"#),
            [
                "echo", "$", "a$", "$%", "$/x", "$.", "$", "a $ b", "$'", "$=1", "$:"
            ]
        );
        // Escaped or in single quotes it is one anyway.
        assert_eq!(words(r#"echo \$A '$A' "\$A""#), ["echo", "$A", "$A", "$A"]);
    }

    #[test]
    fn bash_s_other_parameters_and_operators_are_unsupported() {
        for (line, what) in [
            ("echo $*", "$*"),
            ("echo \"$$\"", "$$"),
            ("kill $!", "$!"),
            ("echo $-", "$-"),
            ("echo $_", "$_"),
            ("echo $(date)", "$("),
            ("echo \"$((1 + 2))\"", "$(("),
            ("echo $'a'", "$'"),
            // bash's old arithmetic, `$[1+1]` (2 in bash), quoted or not.
            ("echo $[1+1]", "$["),
            ("echo \"$[1+1]\"", "$["),
            ("echo a$[", "$["),
            ("echo $\"a\"", "$\""),
            ("echo ${A:-x}", "${A:-x}"),
            ("echo ${A-x}", "${A-x}"),
            ("echo ${1:+y}", "${1:+y}"),
            ("echo ${#A}", "${#A}"),
            ("echo ${!A}", "${!A}"),
            ("echo ${A[0]}", "${A[0]}"),
            ("echo ${A%.sh}", "${A%.sh}"),
            ("echo ${*}", "${*}"),
            ("echo ${_}", "${_}"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn a_brace_without_its_end_is_a_syntax_error() {
        // A `}` quoted or escaped inside does not end it, nor does the
        // first `}` after a `${` inside it, as in bash.
        for line in [
            "echo ${A",
            "echo \"${A",
            "echo ${",
            "echo ${A\\",
            "echo ${A\\}",
            "echo ${A\"}\"",
            "echo ${A\"\\\"\"",
            "echo ${A:-${B}",
        ] {
            let e = one(line).unwrap_err();
            assert_eq!(e, ParseError::UnclosedBrace, "{line}");
            // bash's words (`bash -c 'echo ${A'`).
            assert_eq!(
                e.to_string(),
                "syntax error: unexpected EOF while looking for matching `}'"
            );
        }
    }

    #[test]
    fn quotes_and_escapes_inside_a_substitution_are_skipped_as_bash_does() {
        // Inside `${…}` bash reads quotes, escapes and further `${…}`
        // whole while it looks for the `}`, so a quote left open there is
        // unterminated (bash: ``unexpected EOF while looking for matching
        // `"'``; this shell keeps its own words for an open quote).
        for line in [
            "echo \"${A\"",
            "echo ${A\"",
            "echo \"${A'",
            "echo ${A'}",
            "echo ${A\"x",
            "echo ${A\"\\",
        ] {
            assert_eq!(one(line), Err(ParseError::UnterminatedQuote), "{line}");
        }
        // What it reads is the substitution's text, and bash's `bad
        // substitution` or this shell's refusal names it whole.
        let bad = |t: &str, q: bool| vec![Piece::Param(Param::Bad(t.into()), q)];
        assert_eq!(
            pieces(r#"${A"}"} ${A\}} ${A'\'} "${A"\""}""#),
            [
                bad(r#"${A"}"}"#, false),
                bad(r"${A\}}", false),
                bad(r"${A'\'}", false),
                bad(r#"${A"\""}"#, true)
            ]
        );
        assert_eq!(
            one("echo ${A:-${B}}"),
            Err(ParseError::Unsupported("${A:-${B}}".into()))
        );
        assert_eq!(
            one("echo ${A:-\"x}\"}"),
            Err(ParseError::Unsupported("${A:-\"x}\"}".into()))
        );
    }

    /// `word`'s assignment, the value's pieces joined.
    fn assignment(word: &str) -> Option<(String, String)> {
        let c = typed(word);
        let (name, value) = c[0].words[0].assignment()?;
        let text = value
            .pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t, _) => t.clone(),
                Piece::Param(p, _) => format!("<{p:?}>"),
            })
            .collect();
        Some((name.into(), text))
    }

    #[test]
    fn a_word_with_an_unquoted_name_and_equals_sign_is_an_assignment() {
        for (word, name, value) in [
            ("A=1", "A", "1"),
            ("A=", "A", ""),
            ("_x9=a=b", "_x9", "a=b"),
            (r#"A="a b""#, "A", "a b"),
            ("A=$B", "A", "<Name(\"B\")>"),
            ("A=x$1", "A", "x<Arg(1)>"),
            ("A=''", "A", ""),
        ] {
            assert_eq!(
                assignment(word),
                Some((name.into(), value.into())),
                "{word}"
            );
        }
        // Command names in bash (`1A=x: command not found`).
        for word in [
            "1A=x", "A-B=x", r#""A"=x"#, r"A\=x", "=x", r#"A"="x"#, "$A=x", "a",
        ] {
            assert_eq!(assignment(word), None, "{word}");
        }
    }

    #[test]
    fn a_tilde_in_a_value_is_home_at_its_start_or_after_a_colon() {
        // What bash sets for each.
        for (word, value) in [
            ("A=~/x:~/y:~:a~", "/root/x:/root/y:/root:a~"),
            ("A=~", "/root"),
            ("A=x:~", "x:/root"),
            ("A=~x", "~x"),
            ("A='~'/x", "~/x"),
            (r#"A=~"/x""#, "~/x"),
            ("A=~$B", "~<Name(\"B\")>"),
        ] {
            assert_eq!(assignment(word).unwrap().1, value, "{word}");
        }
    }

    #[test]
    fn an_assignment_before_a_command_is_unsupported() {
        // bash gives the command an environment, which programs have not.
        for (line, what) in [
            ("A=1 echo hi", "A=1 before a command"),
            ("A='a b' B=2 cat f", "A='a b' before a command"),
            ("ls | A=1 wc", "A=1 before a command"),
            ("A=1 echo &", "A=1 before a command"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        // An argument that looks like one is one; assignments alone parse.
        assert_eq!(words("echo A=1"), ["echo", "A=1"]);
        assert!(parse_line("A=1 B=2 > f").is_ok());
    }

    #[test]
    fn appending_to_a_variable_is_unsupported() {
        // bash's `A+=x` appends; here it is refused rather than run as a
        // command (the review found `A+=2: command not found`).
        for (line, what) in [
            ("A+=2", "A+=2"),
            ("PATH+=:/x", "PATH+=:/x"),
            ("A=1 B+=\"x y\"", "B+=\"x y\""),
            ("A+=1 echo hi", "A+=1"),
            ("ls | A+=1", "A+=1"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        // An argument, or a word that is no name's, is not one.
        assert_eq!(words("echo A+=1"), ["echo", "A+=1"]);
        assert!(parse_line("1+=x").is_ok() && parse_line("A\\+=x").is_ok());
    }

    #[test]
    fn a_word_is_kept_as_typed() {
        let c = &typed(r#"echo  a"b c"$D  > '$f'x# 2"#)[0];
        let typed: Vec<&str> = c.words.iter().map(|w| w.typed.as_str()).collect();
        assert_eq!(typed, ["echo", r#"a"b c"$D"#, "2"]);
        assert_eq!(c.output().unwrap().0.typed, "'$f'x#");
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
            c.redirects,
            [Redirect {
                fd: 1,
                op: RedirectOp::Write("out.txt".into())
            }]
        );
        let c = one("echo hi>>'my log'").unwrap();
        assert_eq!(
            c.redirects,
            [Redirect {
                fd: 1,
                op: RedirectOp::Append("my log".into())
            }]
        );
        // The redirection can come first.
        let c = one(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.output(), Some((&"f".into(), false)));
    }

    #[test]
    fn redirection_errors() {
        assert_eq!(one("echo >"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(one("echo > > f"), Err(ParseError::MissingTarget(">")));
        // Several are made left to right (programmable shell gate §7.2).
        let c = one("echo > a >> b x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(
            c.redirects,
            [
                Redirect {
                    fd: 1,
                    op: RedirectOp::Write("a".into())
                },
                Redirect {
                    fd: 1,
                    op: RedirectOp::Append("b".into())
                }
            ]
        );
        assert_eq!(c.output(), Some((&"b".into(), true)));
        // A word of digits just before the operator is its fd, 1 or 2
        // (programmable shell gate §7.1); a quoted or spaced digit, or one
        // in a longer word, is a word.
        for (line, fd, op) in [
            ("cat f 2>err", 2, RedirectOp::Write("err".into())),
            ("echo a 2>>g", 2, RedirectOp::Append("g".into())),
            ("echo a 1> g", 1, RedirectOp::Write("g".into())),
            ("echo a 1>>g", 1, RedirectOp::Append("g".into())),
        ] {
            assert_eq!(
                one(line).unwrap().redirects,
                [Redirect { fd, op }],
                "{line}"
            );
        }
        for (line, refused) in [
            ("echo a 3> g", "3>"),
            ("echo a 0> g", "0>"),
            ("echo a 02> g", "02>"),
            ("echo a 10> g", "10>"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // Even where a file name is awaited (bash 5.2, probes/p8.txt).
        for line in ["echo a >2>f", "echo a 2>2>f", "cat <2>f"] {
            assert_eq!(
                one(line).unwrap_err().to_string(),
                "syntax error near unexpected token `2'",
                "{line}"
            );
        }
        assert_eq!(one("echo a 2> >f"), Err(ParseError::MissingTarget(">")));
        // `>&` copies fd 1 or 2, a blank before the fd or not (bash 5.2,
        // probes/p1.txt, p6.txt).
        let copy = |fd, from| Redirect {
            fd,
            op: RedirectOp::Copy(from),
        };
        for (line, redirects) in [
            ("echo a 2>&1", alloc::vec![copy(2, 1)]),
            ("echo a 1>&2", alloc::vec![copy(1, 2)]),
            ("echo a >&2", alloc::vec![copy(1, 2)]),
            ("echo a >& 2", alloc::vec![copy(1, 2)]),
            ("echo a 2>& 1", alloc::vec![copy(2, 1)]),
            ("echo a 2>&2 1>&1", alloc::vec![copy(2, 2), copy(1, 1)]),
        ] {
            let c = one(line).unwrap();
            assert_eq!(
                (c.words, c.redirects),
                (words("echo a"), redirects),
                "{line}"
            );
        }
        let c = one("echo a 2>&1>f").unwrap();
        assert_eq!(
            c.redirects,
            [
                copy(2, 1),
                Redirect {
                    fd: 1,
                    op: RedirectOp::Write("f".into())
                }
            ]
        );
        // Anything else bash takes there is refused: another fd, a file,
        // `-`, a word quoted or expanded.
        for (line, refused) in [
            ("echo a 2>&3", "2>&3"),
            ("echo a >&0", ">&0"),
            ("echo a 0>&1", "0>"),
            ("echo a >&-", ">&-"),
            ("echo a >&f", ">&f"),
            ("echo a 2>&1x", "2>&1x"),
            ("echo a 2>&01", "2>&01"),
            ("echo a 2>&\"1\"", "2>&\"1\""),
            ("echo a 2>&$N", "2>&$N"),
            ("echo a &> f", "&>"),
            ("echo a >| f", ">|"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // `<` and `0<` read a file as fd 0 (bash 5.2, probes/p9.txt).
        let read = |path: &str| Redirect {
            fd: 0,
            op: RedirectOp::Read(path.into()),
        };
        for line in ["cat < f", "cat 0< f", "cat <f", "<f cat", "cat 0<f"] {
            let c = one(line).unwrap();
            assert_eq!(
                (c.words, c.redirects),
                (words("cat"), alloc::vec![read("f")]),
                "{line}"
            );
        }
        assert_eq!(parse("cat < f | wc -l").unwrap()[0].redirects, [read("f")]);
        for (line, refused) in [
            ("cat 1< f", "1<"),
            ("cat 2< f", "2<"),
            ("cat 00< f", "00<"),
            ("cat << EOF", "<<"),
            ("cat <<-EOF", "<<"),
            ("cat <<< x", "<<<"),
            ("cat <&0", "<&"),
            ("cat <> f", "<>"),
            // Only a pipeline's first command reads a file (§7.3).
            ("cat | cat < f", "< after |"),
            ("cat | cat < f | wc", "< after |"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // A `>&` not followed by a bare fd names the fd as typed.
        assert_eq!(
            one("echo a 1>&x"),
            Err(ParseError::Unsupported("1>&x".into()))
        );
        // In a `for`'s words a redirection is bash's error, naming its fd
        // if it has one (the prototype's review, M-4).
        for (line, token) in [
            ("for x in a 2>f; do b; done", "2"),
            ("for x in a 0<f; do b; done", "0"),
            ("for x in a >f; do b; done", ">"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // bash's syntax errors.
        for (line, token) in [
            ("cat < < f", "<"),
            ("cat <", "newline"),
            ("for x in a < b; do echo; done", "<"),
            ("echo a 2>>&1", "&"),
            ("echo a > &2", "&"),
            ("echo a 2>&", "newline"),
        ] {
            assert_eq!(one(line), Err(ParseError::MissingTarget(token)), "{line}");
        }
        assert_eq!(one("echo a 2>>"), Err(ParseError::MissingTarget("newline")));
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
            ("ls *.txt", '*'),
            ("ls file?", '?'),
            ("echo `x`", '`'),
            ("(ls)", '('),
        ] {
            assert_eq!(one(line), Err(ParseError::Unsupported(c.into())), "{line}");
        }
        assert_eq!(
            one("echo a 3>f").unwrap_err().to_string(),
            "unsupported syntax: 3>"
        );
    }

    #[test]
    fn a_semicolon_ends_an_item() {
        let list = parse_line("echo a;echo b ; cat f | wc;").unwrap();
        let firsts: Vec<&str> = list
            .items
            .iter()
            .map(|i| i.and_or.first.commands()[0].words[0].typed.as_str())
            .collect();
        assert_eq!(firsts, ["echo", "echo", "cat"]);
        assert!(list.items.iter().all(|i| i.background.is_none()));
        // Quoted, escaped or in a comment it is a character.
        assert_eq!(
            parse(r#"echo ';' ";" \; # ; x"#).unwrap()[0].words,
            ["echo", ";", ";", ";"]
        );
        // A background job's text is its own item's.
        let list = parse_line("echo a; sleep 5 &").unwrap();
        assert_eq!(list.items[1].background.as_deref(), Some("sleep 5"));
    }

    #[test]
    fn a_semicolon_needs_a_command_before_it() {
        // bash's messages (an interactive bash 5.2, for each line).
        for (line, token) in [
            (";", ";"),
            ("; echo a", ";"),
            ("echo a; ;", ";"),
            ("echo a;;", ";;"),
            ("echo a ;; echo b", ";;"),
            // bash's other `case` terminators (the review found them
            // named `&` and `;;`).
            ("echo a ;& echo b", ";&"),
            ("echo a;&", ";&"),
            ("echo a ;;& echo b", ";;&"),
            ("echo > ;", ";"),
            ("ls | ;", ";"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        assert_eq!(
            parse_line("a | > f; b"),
            Err(ParseError::Unsupported("| >".into()))
        );
    }

    /// The first command's name of each pipeline of `line`'s one and-or
    /// list, and what joins each to the one before.
    fn and_or(line: &str) -> (Vec<String>, Vec<Connector>) {
        let ao = parse_line(line).unwrap().items.remove(0).and_or;
        let name = |p: &Pipeline<Word>| p.commands()[0].words[0].typed.clone();
        let mut names = alloc::vec![name(&ao.first)];
        names.extend(ao.rest.iter().map(|(_, p)| name(p)));
        (names, ao.rest.iter().map(|&(c, _)| c).collect())
    }

    #[test]
    fn double_ampersands_and_bars_join_pipelines() {
        use Connector::{And, Or};
        assert_eq!(
            and_or("a && b|c || d&&e"),
            (
                alloc::vec!["a".into(), "b".into(), "d".into(), "e".into()],
                alloc::vec![And, Or, And]
            )
        );
        let list = parse_line("a && b; c || d").unwrap();
        assert_eq!(list.items.len(), 2);
        // Quoted, escaped or in a comment they are characters.
        assert_eq!(
            parse(r"echo '&&' \|\| # && x").unwrap()[0].words,
            ["echo", "&&", "||"]
        );
    }

    #[test]
    fn a_double_ampersand_or_bar_needs_a_pipeline_on_each_side() {
        // bash's messages (an interactive bash 5.2, for each line).
        for (line, token) in [
            ("&& a", "&&"),
            ("|| a", "||"),
            ("a && && b", "&&"),
            ("a || && b", "&&"),
            ("a | && b", "&&"),
            ("echo > && b", "&&"),
            ("a && | b", "|"),
            ("a && ; b", ";"),
            ("a ||; b", ";"),
            ("a && &", "&"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // `bash -c 'a &&'`.
        for line in ["a &&", "a || ", "a && # b"] {
            assert_eq!(parse_line(line), Err(ParseError::Incomplete), "{line}");
        }
        // bash runs the whole and-or list in the background, in a shell
        // of its own (programmable shell gate §4.1).
        assert_eq!(
            parse_line("a && b &"),
            Err(ParseError::Unsupported("& after &&".into()))
        );
        assert_eq!(
            parse_line("a && b || c &"),
            Err(ParseError::Unsupported("& after ||".into()))
        );
    }

    /// The first pipeline of `line`, as typed.
    fn first(line: &str) -> Pipeline<Word> {
        parse_line(line).unwrap().items.remove(0).and_or.first
    }

    #[test]
    fn a_bang_before_a_pipeline_negates_it() {
        let p = first("! a | b");
        assert!(p.negated);
        assert_eq!(p.commands().len(), 2);
        // Each `!` turns it again, as bash's does.
        assert!(!first("! ! a").negated && first("! ! ! a").negated);
        assert!(!first("a").negated);
        // Alone before `;` or the line's end, it negates nothing.
        let list = parse_line("! ; !").unwrap();
        assert_eq!(list.items.len(), 2);
        for item in &list.items {
            let p = &item.and_or.first;
            assert!(p.negated && p.commands()[0].words.is_empty());
        }
        // Each pipeline of an and-or list has its own.
        let ao = parse_line("! a && ! b || c")
            .unwrap()
            .items
            .remove(0)
            .and_or;
        let negated: Vec<bool> = core::iter::once(&ao.first)
            .chain(ao.rest.iter().map(|(_, p)| p))
            .map(|p| p.negated)
            .collect();
        assert_eq!(negated, [true, true, false]);
        // Only a whole unquoted word at a pipeline's start: elsewhere, as
        // bash's, it is a word (after a redirection, a command's name).
        for (line, words) in [
            ("echo !", &["echo", "!"][..]),
            ("'!' a", &["!", "a"]),
            ("\\! a", &["!", "a"]),
            ("!a b", &["!a", "b"]),
            ("> f ! a", &["!", "a"]),
        ] {
            assert!(!first(line).negated, "{line}");
            assert_eq!(parse(line).unwrap()[0].words, words, "{line}");
        }
    }

    #[test]
    fn a_bang_stands_only_at_a_pipeline_s_start() {
        // bash's messages (an interactive bash 5.2, for each line).
        for (line, token) in [
            ("a | ! b", "!"),
            ("! | a", "|"),
            ("! && a", "&&"),
            ("! || a", "||"),
            ("! &", "&"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // A background job's text leaves it out, as bash's `jobs` does.
        assert_eq!(background("! sleep 5 &").as_deref(), Some("sleep 5"));
        assert_eq!(background("! ! sleep 5 &").as_deref(), Some("sleep 5"));
    }

    #[test]
    fn a_reserved_word_where_a_command_name_stands_is_unsupported() {
        // bash's other reserved words are refused where they would be one
        // (programmable shell gate §4.2), rather than run as commands that
        // are not found; the loops' until they come.
        // The words of the spec's §4.2, written out apart from `RESERVED`.
        for word in [
            "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break",
            "continue",
        ] {
            for line in [
                alloc::format!("{word} x"),
                alloc::format!("a; {word}"),
                alloc::format!("a && {word} b"),
                alloc::format!("a | {word}"),
                alloc::format!("! {word}"),
                alloc::format!("a &\n{word}"),
            ] {
                assert_eq!(
                    parse_line(&line),
                    Err(ParseError::Unsupported(String::from(word))),
                    "{line:?}"
                );
            }
        }
        // Anywhere else, quoted or escaped, or after a redirection (a
        // command's name in bash), it is a word.
        for (line, words) in [
            ("echo case esac", &["echo", "case", "esac"][..]),
            ("'for' x", &["for", "x"]),
            ("\\while x", &["while", "x"]),
            ("> f done", &["done"]),
            ("iffy", &["iffy"]),
        ] {
            assert_eq!(parse(line).unwrap()[0].words, words, "{line}");
        }
        assert!(parse_line("if=1").is_ok(), "an assignment");
    }

    #[test]
    fn a_newline_ends_an_item_but_not_after_an_operator() {
        let names = |line: &str| -> Vec<Vec<String>> {
            parse_line(line)
                .unwrap()
                .items
                .iter()
                .map(|i| {
                    let mut ps = alloc::vec![&i.and_or.first];
                    ps.extend(i.and_or.rest.iter().map(|(_, p)| p));
                    ps.iter()
                        .flat_map(|p| p.commands().iter().map(|c| c.words[0].typed.clone()))
                        .collect()
                })
                .collect()
        };
        assert_eq!(names("a\nb\n"), [["a"], ["b"]]);
        // Blank and comment lines hold nothing.
        assert_eq!(names("\na\n\n  # c\n\tb # d\n"), [["a"], ["b"]]);
        // After `|`, `&&` or `||` the command goes on, past them, as bash's.
        assert_eq!(names("a &&\nb\nc"), [&["a", "b"][..], &["c"]]);
        assert_eq!(names("a |\n\n# c\n b"), [["a", "b"]]);
        assert_eq!(names("a ||  # c\nb && \n c"), [["a", "b", "c"]]);
        // A `!` before the pipeline is no command typed after its `|`.
        assert_eq!(names("! a |\n b"), [["a", "b"]]);
        // A background job on a later line has its own text.
        let list = parse_line("a\nsleep 5 &\nb").unwrap();
        assert_eq!(list.items[1].background.as_deref(), Some("sleep 5"));
    }

    #[test]
    fn a_job_typed_across_lines_has_one_line_of_text() {
        // As bash's `jobs` shows it: the lines joined by a blank, without
        // comments or blank lines (the review found them kept).
        for (text, job) in [
            ("sleep 5 |\n# c\ncat &", "sleep 5 | cat"),
            ("sleep 5 |   # c\n\n  cat  &", "sleep 5 | cat"),
            ("! sleep 5 |\n  cat &", "sleep 5 | cat"),
            ("a\nsleep 5 &", "sleep 5"),
            ("echo '#' \\# |\n cat&", "echo '#' \\# | cat"),
        ] {
            let list = parse_line(text).unwrap();
            let last = list.items.last().unwrap();
            assert_eq!(last.background.as_deref(), Some(job), "{text:?}");
        }
    }

    #[test]
    fn a_command_that_needs_more_lines_is_incomplete() {
        for text in ["a &&", "a &&\n", "a |\n# c\n", "a ||\n\n", "a\nb |"] {
            assert_eq!(parse_line(text), Err(ParseError::Incomplete), "{text:?}");
        }
        assert_eq!(
            ParseError::Incomplete.to_string(),
            "syntax error: unexpected end of file"
        );
        // A newline is no target, and starts no item before `;` or `&&`.
        for (text, token) in [("echo >\nb", "newline"), ("a\n;", ";"), ("a\n&& b", "&&")] {
            assert_eq!(
                parse_line(text).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{text:?}"
            );
        }
        // A `\` at a line's end is still an error: bash would join the
        // lines.
        assert_eq!(
            parse_line("echo a \\\nb"),
            Err(ParseError::TrailingBackslash)
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
        assert_eq!(p[0].redirects, []);
        assert_eq!(p[2].output().unwrap().0, "out");
        // Quoted, escaped or in a comment it is a character.
        assert_eq!(
            parse(r#"echo '|' "|" \| # | x"#).unwrap()[0].words,
            ["echo", "|", "|", "|"]
        );
        assert_eq!(
            parse("").unwrap(),
            [Command {
                words: Vec::new(),
                redirects: Vec::new()
            }]
        );
    }

    #[test]
    fn a_bar_needs_a_command_on_each_side() {
        // bash's messages (`bash -c '| a'`, `bash -c 'a |'`).
        for line in ["| a", "a | | b", "a || | b", "a ||| b", "echo > | b", " |"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "syntax error near unexpected token `|'",
                "{line}"
            );
        }
        for line in ["a |", "a | b |  ", "a | # b"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "syntax error: unexpected end of file",
                "{line}"
            );
        }
        // bash's `|&` sends the errors into the pipe too; `| &` is its
        // syntax error.
        assert_eq!(
            parse("a |& b").unwrap_err().to_string(),
            "unsupported syntax: |&"
        );
        assert_eq!(parse("a|&b"), Err(ParseError::Unsupported("|&".into())));
        // Only the last command redirects its output (spec §9.1): bash
        // would send the first one's output into the file and the second
        // nothing.
        for (line, op) in [
            ("a > f | b", ">"),
            ("a >> f | b", ">>"),
            ("a 1> f | b", ">"),
            ("a 2>&1 > f | b", ">"),
        ] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                alloc::format!("unsupported syntax: {op} before |"),
                "{line}"
            );
        }
        // Errors may go anywhere (programmable shell gate §7.3), and a copy
        // is made over the pipes.
        for line in [
            "a 2> e | b",
            "a 2>> e | b 2> e2",
            "a 2>&1 | b",
            "a 1>&2 | b",
            "a < f 2>&1 | b | c 2>&1 > g",
        ] {
            assert!(parse(line).is_ok(), "{line}");
        }
        let p = parse("a 2>&1 | b").unwrap();
        assert_eq!(
            p[0].redirects,
            [Redirect {
                fd: 2,
                op: RedirectOp::Copy(1)
            }]
        );
    }

    #[test]
    fn every_command_of_a_pipeline_has_a_name() {
        // bash runs a redirection alone as a command; here a pipeline's
        // commands are programs, so one without a name is refused.
        for (line, what) in [
            ("> f | b", "> before |"),
            (">> f | b | c", ">> before |"),
            ("a | > f", "| >"),
            ("a | b | >> f", "| >>"),
            // Each names what was typed (the prototype's review, M-4).
            ("< f | cat", "< before |"),
            ("2> e | cat", "2> before |"),
            ("a 2> e >> f | b", ">> before |"),
            ("a | < f", "| <"),
            ("a | 2> e", "| 2>"),
            ("a | 2>&1", "| 2>&1"),
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
        assert_eq!(parse("sleep 5 &").unwrap()[0].words, ["sleep", "5"]);
        assert_eq!(background("sleep 5 &").as_deref(), Some("sleep 5"));
        // The text is what was typed before the `&`, without the blanks
        // around it; a comment may follow.
        for (line, text) in [
            ("  cat f |  wc -l>out& ", "cat f |  wc -l>out"),
            ("echo 'a  b' \\& &\t# later", "echo 'a  b' \\&"),
            ("t-spin&", "t-spin"),
            ("grep x f | head -n 1 & # one", "grep x f | head -n 1"),
        ] {
            assert_eq!(background(line).as_deref(), Some(text), "{line}");
        }
        let p = parse("cat f | wc -l > out &").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].output().unwrap().0, "out");
        // Quoted, escaped or in a comment it is a character.
        for line in ["echo '&' \"&\" \\&", "echo a # &", "echo a"] {
            assert_eq!(background(line), None, "{line}");
        }
        assert_eq!(words("echo '&' \\& # &"), ["echo", "&", "&"]);
    }

    #[test]
    fn an_ampersand_ends_an_item_that_runs_in_the_background() {
        let backgrounds = |line: &str| -> Vec<Option<String>> {
            parse_line(line)
                .unwrap()
                .items
                .into_iter()
                .map(|i| i.background)
                .collect()
        };
        let some = |t: &str| Some(String::from(t));
        assert_eq!(
            backgrounds("sleep 5 & echo a &t-spin&"),
            [some("sleep 5"), some("echo a"), some("t-spin")]
        );
        assert_eq!(backgrounds("a & b"), [some("a"), None]);
        assert_eq!(
            backgrounds("a | b & c; d & # e"),
            [some("a | b"), None, some("d")]
        );
        // bash's messages (an interactive bash 5.2, for each line).
        for (line, token) in [
            ("a & &", "&"),
            ("a & && b", "&&"),
            ("a & ;", ";"),
            ("a & | b", "|"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
    }

    #[test]
    fn an_ampersand_anywhere_else_is_bash_s_error_or_unsupported() {
        // bash's messages (`bash -c '&'`, `bash -c 'a | &'`, …).
        for line in [
            "&",
            " & ",
            "a | &",
            "echo > &",
            "echo >>&2",
            "a & &",
            "a &&&",
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                "syntax error near unexpected token `&'",
                "{line}"
            );
        }
        // bash runs `> f &`; it is not supported.
        for (line, what) in [
            ("> f &", "> &"),
            // bash runs this (the review found it called its syntax
            // error); `>&2` copies fd 2 (programmable shell gate §7.1).
            ("echo hi >& f", ">&f"),
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
        assert_eq!(c.output().unwrap().0, "/root/out");
        // As bash's: a quoted `/` after it, or a parameter, keeps it.
        assert_eq!(
            words(r#"echo ~"/x" ~\/x ~$E ~"""#),
            ["echo", "~/x", "~/x", "~", "~"]
        );
    }
}
