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
//! `> file` and `>> file` redirect standard output (at most one per
//! command). An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
//! `#` at the start of a word begins a comment, which runs to the end of
//! the line. An unquoted `|` joins commands into a pipeline (user-space
//! gate §9.1); each has a name, only the last may redirect its output, and
//! bash's syntax errors name a `|` with no command before it or none after.
//! An unquoted `&` at the end of the line (a comment may follow) runs it in
//! the background (§9.2). A line is a [`List`] (programmable shell gate
//! §4.1): an unquoted `;` ends one of its items, and `&&` and `||` join
//! pipelines into an and-or list; bash's syntax errors name one with no
//! command before it, and an and-or list ending with `&` is refused. An
//! unquoted `!` word at a pipeline's start negates its status. Every
//! other shell feature is refused: an unquoted `&` before more, `*`, `?`,
//! `<`, `` ` ``, `(` or `)` is an error naming the character, instead of
//! being passed on as if it were plain text; so are `|&` (the errors into
//! the pipe too), `>&` and `2>` (another stream).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::iter::Peekable;
use core::str::CharIndices;

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

/// One command, or several joined by `|`, each one's output the next
/// one's input; after a `!`, its status negated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline<W = String> {
    pub negated: bool,
    pub commands: Vec<Command<W>>,
}

/// One command: its words and where its output goes. The parser gives
/// them as typed ([`Word`]), and expansion as the strings a command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<W>,
    pub redirect: Option<Redirect<W>>,
}

/// `> path` (truncate) or `>> path` (append).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect<W = String> {
    pub path: W,
    pub append: bool,
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
    /// A `|` without a command after it.
    UnexpectedEnd,
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
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
            ParseError::Expansion(why) => f.write_str(why),
            ParseError::UnclosedBrace => {
                f.write_str("syntax error: unexpected EOF while looking for matching `}'")
            }
        }
    }
}

const UNSUPPORTED: &[char] = &['*', '?', '<', '`', '(', ')'];

/// The characters a line is read from, and where each is.
struct Cursor<'l> {
    line: &'l str,
    chars: Peekable<CharIndices<'l>>,
}

impl<'l> Cursor<'l> {
    fn new(line: &'l str) -> Cursor<'l> {
        Cursor {
            line,
            chars: line.char_indices().peekable(),
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
        self.chars.peek().map_or(self.line.len(), |&(i, _)| i)
    }

    /// What is left of the line.
    fn rest(&mut self) -> &'l str {
        let at = self.pos();
        &self.line[at..]
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

/// A command of `words` and `redirect`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
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
    Ok(Command { words, redirect })
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
    redirect: Option<Redirect<Word>>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
    pending: Option<bool>,
    /// How many `!` stood before the pipeline's first command.
    bangs: usize,
    /// The command is not the pipeline's first, so a `!` cannot stand
    /// before it.
    later: bool,
}

impl Parts {
    /// Ends a word: it becomes the pending redirection's target or the next
    /// word.
    fn end_word(&mut self, word: &mut Building, line: &str) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish(line) else {
            return Ok(());
        };
        match self.pending.take() {
            Some(_) if self.redirect.is_some() => return Err(ParseError::Unsupported(">".into())),
            Some(append) => self.redirect = Some(Redirect { path: w, append }),
            // A `!` before anything of the command negates the pipeline
            // (programmable shell gate §4.1), only the first command's.
            None if w.is_bang() && self.words.is_empty() && self.redirect.is_none() => {
                if self.later {
                    return Err(ParseError::MissingTarget("!"));
                }
                self.bangs += 1;
            }
            None => self.words.push(w),
        }
        Ok(())
    }

    /// The command so far, ended by a `|`, which needs one before it.
    /// Every command of a pipeline has a name: a redirection alone, which
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command<Word>, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
            return Err(ParseError::MissingTarget("|"));
        }
        if self.redirect.is_some() {
            return Err(ParseError::Unsupported("> before |".into()));
        }
        let p = core::mem::take(self);
        self.bangs = p.bangs;
        self.later = true;
        command(p.words, None)
    }
}

/// The commands of the first pipeline of `line`, whether or not it ends
/// with `&`, their words expanded with no variables set (for callers that
/// run no shell: tests); a line that does not expand is
/// `ParseError::Expansion`. A blank line is one command without words.
pub fn parse(line: &str) -> Result<Vec<Command>, ParseError> {
    let list = parse_line(line)?;
    let Some(item) = list.items.first() else {
        return Ok(alloc::vec![Command {
            words: Vec::new(),
            redirect: None,
        }]);
    };
    match crate::expand::plain(&item.and_or.first) {
        Ok(p) => Ok(p.commands),
        Err(e) => Err(ParseError::Expansion(e.to_string())),
    }
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
    if parts.words.is_empty() {
        match (pipeline.is_empty(), &parts.redirect) {
            // `!` alone is a command that does nothing, negated.
            (true, None) if parts.bangs == 0 => return Ok(None),
            (true, None) => {}
            (true, Some(_)) => {}
            (false, None) => return Err(ParseError::MissingTarget(end)),
            (false, Some(_)) => return Err(ParseError::Unsupported("| >".into())),
        }
    }
    let p = core::mem::take(parts);
    pipeline.push(command(p.words, p.redirect)?);
    Ok(Some(Pipeline {
        negated: p.bangs % 2 == 1,
        commands: core::mem::take(pipeline),
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

/// `line`'s list, its words as typed. `;` ends an item, and `&&` and `||`
/// join pipelines, as bash's do; one with nothing typed before it is
/// bash's syntax error naming it.
pub fn parse_line(line: &str) -> Result<List<Word>, ParseError> {
    let mut items = Items::default();
    // Where the item being read starts in the line.
    let mut item_start = 0;
    let mut background = None;
    let mut pipeline = Vec::new();
    let mut parts = Parts::default();
    let mut word = Building::default();
    let mut cur = Cursor::new(line);
    loop {
        let at = cur.pos();
        let Some(c) = cur.next() else {
            break;
        };
        match c {
            ' ' | '\t' => parts.end_word(&mut word, line)?,
            '>' => {
                // `2>` redirects another stream in a real shell.
                if word.started
                    && let Some(digits) = word.word.digits()
                {
                    return Err(ParseError::Unsupported(format!("{digits}>")));
                }
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() {
                    return Err(ParseError::MissingTarget(">"));
                }
                let append = cur.next_if_eq('>');
                // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                // `> &` are its syntax errors.
                if !append && cur.peek() == Some('&') {
                    return Err(ParseError::Unsupported(">&".into()));
                }
                parts.pending = Some(append);
            }
            '|' if cur.next_if_eq('|') => {
                parts.end_word(&mut word, line)?;
                join(&mut items, &mut parts, &mut pipeline, Connector::Or)?;
            }
            '|' => {
                // bash's `|&` pipes the errors too.
                if cur.next_if_eq('&') {
                    return Err(ParseError::Unsupported("|&".into()));
                }
                parts.end_word(&mut word, line)?;
                pipeline.push(parts.take_before_pipe()?);
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
                parts.end_word(&mut word, line)?;
                match end_pipeline(&mut parts, &mut pipeline, ";")? {
                    Some(p) => items.pipeline(p),
                    None => return Err(ParseError::MissingTarget(";")),
                }
                items.end(None);
                item_start = cur.pos();
            }
            '&' if cur.next_if_eq('&') => {
                parts.end_word(&mut word, line)?;
                join(&mut items, &mut parts, &mut pipeline, Connector::And)?;
            }
            '&' => {
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
                    return Err(ParseError::MissingTarget("&"));
                }
                if parts.words.is_empty() {
                    // `> f &`: a background job is a program.
                    return Err(ParseError::Unsupported("> &".into()));
                }
                // bash runs the whole and-or list in the background, in a
                // shell of its own.
                if let Some(c) = items.connector {
                    return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                }
                let after = cur.rest().trim_start_matches([' ', '\t']);
                if after.starts_with('&') {
                    return Err(ParseError::MissingTarget("&"));
                }
                if !after.is_empty() && !after.starts_with('#') {
                    // `a & b` runs both in bash.
                    return Err(ParseError::Unsupported("&".into()));
                }
                // Without its `!`, as bash's `jobs` shows it.
                let mut text = line[item_start..at].trim_matches([' ', '\t']);
                for _ in 0..parts.bangs {
                    text = text[1..].trim_start_matches([' ', '\t']);
                }
                background = Some(String::from(text));
                break;
            }
            '\'' => {
                let before = word.open_quote();
                loop {
                    match cur.next() {
                        Some('\'') => break,
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
                word.close_quote(before);
            }
            '"' => {
                let before = word.open_quote();
                loop {
                    match cur.next() {
                        Some('"') => break,
                        Some('\\') if matches!(cur.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.quoted(cur.next().expect("peeked"));
                        }
                        Some('$') => match parameter(&mut cur, true)? {
                            Some(p) => word.param(p, true),
                            None => word.quoted('$'),
                        },
                        Some('`') => return Err(ParseError::Unsupported('`'.into())),
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
                word.close_quote(before);
            }
            '\\' => match cur.next() {
                Some(c) => word.quoted(c),
                None => return Err(ParseError::TrailingBackslash),
            },
            '$' => match parameter(&mut cur, false)? {
                Some(p) => word.param(p, false),
                None => {
                    word.started = true;
                    word.added += 1;
                    word.word.push('$', false);
                }
            },
            // A comment runs to the end of the line.
            '#' if !word.started => break,
            c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
            c => {
                if !word.started && c == '~' {
                    word.tilde = true;
                }
                word.started = true;
                word.added += 1;
                word.word.push(c, false);
            }
        }
        if word.started {
            if word.end == 0 {
                word.start = at;
            }
            word.end = cur.pos();
        }
    }
    parts.end_word(&mut word, line)?;
    if parts.pending.is_some() {
        return Err(ParseError::MissingTarget("newline"));
    }
    if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
        return Err(ParseError::UnexpectedEnd);
    }
    // Nothing after the last `;` (or at all) is no item.
    match end_pipeline(&mut parts, &mut pipeline, "newline")? {
        Some(p) => items.pipeline(p),
        None if items.connector.is_some() => return Err(ParseError::UnexpectedEnd),
        None => {}
    }
    items.end(background);
    Ok(List { items: items.items })
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
    if parts.words.is_empty() && parts.redirect.is_none() && pipeline.is_empty() {
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

    /// The commands of `line`'s one pipeline, as typed.
    fn typed(line: &str) -> Vec<Command<Word>> {
        parse_line(line)
            .unwrap()
            .items
            .remove(0)
            .and_or
            .first
            .commands
    }

    /// The background text of `line`'s one item.
    fn background(line: &str) -> Option<String> {
        parse_line(line).unwrap().items.remove(0).background
    }

    fn words(line: &str) -> Vec<String> {
        let c = one(line).unwrap();
        assert_eq!(c.redirect, None);
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
            .commands
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
            c.redirect.as_ref().unwrap().path.pieces,
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
        assert_eq!(c.redirect.as_ref().unwrap().path.typed, "'$f'x#");
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
    fn a_semicolon_ends_an_item() {
        let list = parse_line("echo a;echo b ; cat f | wc;").unwrap();
        let firsts: Vec<&str> = list
            .items
            .iter()
            .map(|i| i.and_or.first.commands[0].words[0].typed.as_str())
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
        let name = |p: &Pipeline<Word>| p.commands[0].words[0].typed.clone();
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
            assert_eq!(parse_line(line), Err(ParseError::UnexpectedEnd), "{line}");
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
        assert_eq!(p.commands.len(), 2);
        // Each `!` turns it again, as bash's does.
        assert!(!first("! ! a").negated && first("! ! ! a").negated);
        assert!(!first("a").negated);
        // Alone before `;` or the line's end, it negates nothing.
        let list = parse_line("! ; !").unwrap();
        assert_eq!(list.items.len(), 2);
        for item in &list.items {
            let p = &item.and_or.first;
            assert!(p.negated && p.commands[0].words.is_empty());
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
        assert_eq!(p[1].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
        for line in ["echo '&' \"&\" \\&", "echo a # &", "echo a"] {
            assert_eq!(background(line), None, "{line}");
        }
        assert_eq!(words("echo '&' \\& # &"), ["echo", "&", "&"]);
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
        // bash runs `a & b` and `> f &`; they are not supported.
        for (line, what) in [
            ("a & b", "&"),
            ("a &b", "&"),
            ("a & | b", "&"),
            ("> f &", "> &"),
            // bash runs these (the review found them called its syntax
            // error).
            ("echo hi >&2", ">&"),
            ("echo hi >& f", ">&"),
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
        // As bash's: a quoted `/` after it, or a parameter, keeps it.
        assert_eq!(
            words(r#"echo ~"/x" ~\/x ~$E ~"""#),
            ["echo", "~/x", "~/x", "~", "~"]
        );
    }
}
