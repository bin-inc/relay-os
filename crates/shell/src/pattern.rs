//! `grep`'s patterns (user-space gate §9.1): a subset of POSIX basic
//! regular expressions, over bytes as GNU grep does with `LC_ALL=C`.
//!
//! A pattern is literal bytes, `.` (any byte), `*` (the item before it any
//! number of times; literal at the start), `^` at the start and `$` at the
//! end (anchors; literal elsewhere), and `[...]` (a set of bytes, with
//! ranges, `^` first for its complement, and `]` first as a member). A `\`
//! makes the next byte literal; the escapes that are something else in
//! GNU's basic expressions (groups, intervals, alternation, word and
//! back-references) are refused, as are character classes in a set, so
//! that no pattern quietly matches what GNU grep would not.
//!
//! Matching runs every item at once over the line (an NFA simulation), so
//! it takes time in proportion to the line's length times the pattern's,
//! whatever the pattern.

use alloc::vec::Vec;
use core::fmt;

/// One position of a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Byte(u8),
    Any,
    /// Inclusive byte ranges; `negated` matches every byte outside them.
    Set {
        ranges: Vec<(u8, u8)>,
        negated: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Piece {
    item: Item,
    /// Followed by `*`.
    repeated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pieces: Vec<Piece>,
    at_start: bool,
    at_end: bool,
    ignore_case: bool,
}

/// Why a pattern is refused; the messages are GNU grep's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// A `[` or `[^` with nothing after it.
    Invalid,
    UnmatchedBracket,
    TrailingBackslash,
    InvalidRangeEnd,
    /// A GNU feature this grep does not have, as written.
    Unsupported(&'static str),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternError::Invalid => f.write_str("Invalid regular expression"),
            PatternError::UnmatchedBracket => f.write_str("Unmatched [, [^, [:, [., or [="),
            PatternError::TrailingBackslash => f.write_str("Trailing backslash"),
            PatternError::InvalidRangeEnd => f.write_str("Invalid range end"),
            PatternError::Unsupported(what) => write!(f, "{what} is not supported"),
        }
    }
}

/// The escapes GNU's basic expressions give a meaning, which this grep
/// does not have.
fn unsupported_escape(b: u8) -> Option<&'static str> {
    Some(match b {
        b'(' | b')' => "\\( \\)",
        b'{' | b'}' => "\\{ \\}",
        b'|' => "\\|",
        b'+' => "\\+",
        b'?' => "\\?",
        b'<' | b'>' | b'b' | b'B' | b'w' | b'W' | b's' | b'S' | b'`' | b'\'' => {
            "\\<, \\b, \\w and the like"
        }
        b'1'..=b'9' => "a back-reference",
        _ => return None,
    })
}

impl Pattern {
    /// The pattern `p`; with `ignore_case`, ASCII letters match either case.
    pub fn parse(p: &[u8], ignore_case: bool) -> Result<Pattern, PatternError> {
        let mut pieces: Vec<Piece> = Vec::new();
        let at_start = p.first() == Some(&b'^');
        let mut i = usize::from(at_start);
        let mut at_end = false;
        while i < p.len() {
            let b = p[i];
            i += 1;
            let item = match b {
                b'\\' => {
                    let Some(&c) = p.get(i) else {
                        return Err(PatternError::TrailingBackslash);
                    };
                    i += 1;
                    if let Some(what) = unsupported_escape(c) {
                        return Err(PatternError::Unsupported(what));
                    }
                    Item::Byte(c)
                }
                b'.' => Item::Any,
                // `*` repeats the piece before it; at the start it is
                // literal, and a second one changes nothing.
                b'*' => match pieces.last_mut() {
                    Some(last) => {
                        last.repeated = true;
                        continue;
                    }
                    None => Item::Byte(b'*'),
                },
                b'$' if i == p.len() => {
                    at_end = true;
                    continue;
                }
                b'[' => {
                    let (set, next) = parse_set(p, i)?;
                    i = next;
                    set
                }
                _ => Item::Byte(b),
            };
            pieces.push(Piece {
                item,
                repeated: false,
            });
        }
        Ok(Pattern {
            pieces,
            at_start,
            at_end,
            ignore_case,
        })
    }

    /// Whether `line` (without its newline) holds a match.
    pub fn is_match(&self, line: &[u8]) -> bool {
        let n = self.pieces.len();
        // The states are how many pieces have matched; a repeated piece
        // may also match nothing.
        let mut now = alloc::vec![false; n + 1];
        let mut next = alloc::vec![false; n + 1];
        self.enter(&mut now, 0);
        for &c in line {
            if now[n] && !self.at_end {
                return true;
            }
            next.fill(false);
            for (k, piece) in self.pieces.iter().enumerate() {
                if now[k] && self.matches(&piece.item, c) {
                    let to = if piece.repeated { k } else { k + 1 };
                    self.enter(&mut next, to);
                }
            }
            if !self.at_start {
                self.enter(&mut next, 0);
            }
            core::mem::swap(&mut now, &mut next);
        }
        now[n]
    }

    /// Adds state `k`, and those a repeated piece lets it skip to.
    fn enter(&self, states: &mut [bool], mut k: usize) {
        loop {
            states[k] = true;
            match self.pieces.get(k) {
                Some(p) if p.repeated => k += 1,
                _ => return,
            }
        }
    }

    fn matches(&self, item: &Item, c: u8) -> bool {
        let one = |c: u8| match item {
            Item::Byte(b) => *b == c,
            Item::Any => true,
            Item::Set { ranges, negated } => {
                ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != *negated
            }
        };
        if !self.ignore_case || !c.is_ascii_alphabetic() {
            return one(c);
        }
        match item {
            // A negated set matches a letter only if neither case is in it.
            Item::Set { negated: true, .. } => {
                one(c.to_ascii_lowercase()) && one(c.to_ascii_uppercase())
            }
            _ => one(c.to_ascii_lowercase()) || one(c.to_ascii_uppercase()),
        }
    }
}

/// The set that starts after the `[` at `p[i - 1]`, and where the pattern
/// goes on after its `]`.
fn parse_set(p: &[u8], mut i: usize) -> Result<(Item, usize), PatternError> {
    let negated = p.get(i) == Some(&b'^');
    i += usize::from(negated);
    if i == p.len() {
        return Err(PatternError::Invalid);
    }
    let mut ranges = Vec::new();
    let mut first = true;
    loop {
        let Some(&b) = p.get(i) else {
            return Err(PatternError::UnmatchedBracket);
        };
        if b == b']' && !first {
            return Ok((Item::Set { ranges, negated }, i + 1));
        }
        if b == b'['
            && let Some(&kind @ (b':' | b'.' | b'=')) = p.get(i + 1)
        {
            // GNU's classes and collating elements: refused, once closed.
            let closed = p[i + 2..].windows(2).any(|w| w == [kind, b']']);
            return Err(if closed {
                PatternError::Unsupported("[: :], [. .] and [= =] in a set")
            } else {
                PatternError::UnmatchedBracket
            });
        }
        first = false;
        // `a-z`, unless the `-` is the set's last.
        if p.get(i + 1) == Some(&b'-') && p.get(i + 2).is_some_and(|&e| e != b']') {
            let end = p[i + 2];
            if end < b {
                return Err(PatternError::InvalidRangeEnd);
            }
            ranges.push((b, end));
            i += 3;
        } else {
            ranges.push((b, b));
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    fn m(p: &str, line: &str) -> bool {
        Pattern::parse(p.as_bytes(), false)
            .unwrap()
            .is_match(line.as_bytes())
    }

    #[test]
    fn literal_bytes_match_anywhere_in_the_line() {
        assert!(m("bc", "abcd"));
        assert!(!m("bd", "abcd"));
        assert!(m("", "anything"), "the empty pattern matches every line");
        assert!(m("", ""));
        assert!(!m("a", ""));
    }

    #[test]
    fn dot_star_and_anchors() {
        assert!(m("a.c", "xabcx"));
        assert!(!m("a.c", "ac"));
        assert!(m("ab*c", "ac") && m("ab*c", "abbbc") && !m("ab*c", "adc"));
        assert!(m("^ab", "abc") && !m("^ab", "cab"));
        assert!(m("bc$", "abc") && !m("bc$", "bca"));
        assert!(m("^$", "") && !m("^$", "x"));
        assert!(m("^a.*z$", "a to z") && !m("^a.*z$", "a to z!"));
        // Literal where they are not special.
        assert!(m("*a", "x*a") && !m("*a", "xa"));
        assert!(m("a^b", "a^b") && m("a$b", "a$b"));
        assert!(m("^*", "*x"), "a star after the anchor is literal");
        assert!(m("a**", "b"), "a second star changes nothing");
    }

    #[test]
    fn sets_with_ranges_and_complements() {
        assert!(m("[abc]x", "bx") && !m("[abc]x", "dx"));
        assert!(m("[a-c0-9]", "7") && !m("[a-c0-9]", "z"));
        assert!(m("[^a-c]", "abcd") && !m("[^a-c]", "abc"));
        assert!(m("[]a]", "]") && m("[^]a]", "b") && !m("[^]a]", "]"));
        assert!(m("[a-]", "-") && m("[-a]", "-"));
        assert!(
            m("[.*]", "*") && !m("[.*]", "x"),
            "special characters are members"
        );
        assert!(m("[\\]", "\\"), "a backslash is a member");
        assert!(m("x[0-9]*y", "xy") && m("x[0-9]*y", "x123y"));
    }

    #[test]
    fn a_backslash_makes_the_next_byte_literal() {
        assert!(m("a\\.c", "a.c") && !m("a\\.c", "abc"));
        assert!(m("\\*", "*") && m("\\[", "[") && m("\\^a", "^a") && m("a\\$", "a$x"));
        assert!(m("\\\\", "\\"));
    }

    #[test]
    fn ignoring_case_folds_ascii_letters_only() {
        let p = Pattern::parse(b"ab[c-d][^x]", true).unwrap();
        assert!(p.is_match(b"ABDy") && p.is_match(b"aBcY"));
        assert!(!p.is_match(b"ABDX"), "x, either case, is outside the set");
        let p = Pattern::parse("é".as_bytes(), true).unwrap();
        assert!(
            !p.is_match("É".as_bytes()),
            "bytes beyond ASCII are themselves"
        );
    }

    #[test]
    fn what_is_refused_says_why_as_gnu_grep_does() {
        let e = |p: &str| Pattern::parse(p.as_bytes(), false).unwrap_err().to_string();
        assert_eq!(e("[ab"), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("[]"), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("a["), "Invalid regular expression");
        assert_eq!(e("[^"), "Invalid regular expression");
        assert_eq!(e("[[."), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("ab\\"), "Trailing backslash");
        assert_eq!(e("[b-a]"), "Invalid range end");
        for p in [
            "\\(a\\)",
            "a\\{2\\}",
            "a\\|b",
            "a\\+",
            "a\\?",
            "\\<a",
            "\\w",
            "\\1",
            "[[:alpha:]]",
            "[[.a.]]",
        ] {
            assert!(e(p).ends_with("is not supported"), "{p}");
        }
    }

    #[test]
    fn long_lines_and_many_stars_take_no_backtracking() {
        let line = vec![b'a'; 100_000];
        let p = Pattern::parse(b"a*a*a*a*a*a*a*a*b", false).unwrap();
        assert!(!p.is_match(&line));
        let mut line = line;
        line.push(b'b');
        assert!(p.is_match(&line));
    }

    /// Every pattern of up to `symbols` of `alphabet`, the empty one first.
    fn every_pattern(alphabet: &[&str], symbols: usize) -> Vec<String> {
        let mut patterns: Vec<String> = vec![String::new()];
        let mut last = vec![String::new()];
        for _ in 0..symbols {
            let mut longer = Vec::new();
            for p in &last {
                for s in alphabet {
                    longer.push(alloc::format!("{p}{s}"));
                }
            }
            patterns.extend(longer.iter().cloned());
            last = longer;
        }
        patterns
    }

    /// The patterns whose lines (or refusal) differ from GNU grep's
    /// (`LC_ALL=C`) over `lines`.
    fn unlike_gnu(patterns: &[String], lines: &[&str], ignore_case: bool) -> Vec<String> {
        let input: String = lines.iter().map(|l| alloc::format!("{l}\n")).collect();
        let mut differ = Vec::new();
        for p in patterns {
            let ours = match Pattern::parse(p.as_bytes(), ignore_case) {
                Ok(pat) => {
                    let got: Vec<&str> = lines
                        .iter()
                        .copied()
                        .filter(|l| pat.is_match(l.as_bytes()))
                        .collect();
                    Ok(got)
                }
                Err(e) => Err(e),
            };
            let args: &[&str] = if ignore_case {
                &["grep", "-i", "--", p]
            } else {
                &["grep", "--", p]
            };
            let (status, out, err) = crate::testing::host_tool(args, &[], input.as_bytes());
            let theirs: Vec<&str> = out.lines().collect();
            let same = match (&ours, status) {
                (Ok(got), 0 | 1) => *got == theirs,
                (Err(e), 2) => err == alloc::format!("grep: {e}\n"),
                _ => false,
            };
            if !same {
                differ.push(alloc::format!(
                    "{p:?}: ours {ours:?}, grep's {status} {theirs:?} {err:?}"
                ));
            }
        }
        differ
    }

    /// Every pattern of a small alphabet against lines of another, and what
    /// GNU grep says of each: the same lines, or the same refusal.
    #[test]
    fn it_matches_what_gnu_grep_matches() {
        let lines = [
            "", "a", "b", "ab", "ba", "aab", "abb", "a.b", "a*b", "[a]", "^a", "a$", "-", "]",
            "x y", "abab",
        ];
        let alphabet = [
            "a", "b", ".", "*", "^", "$", "[", "]", "-", "\\.", "[^a]", "[a-b]",
        ];
        // Up to three symbols: 1 + 12 + 144 + 1728 patterns.
        let patterns = every_pattern(&alphabet, 3);
        let differ = unlike_gnu(&patterns, &lines, false);
        assert!(
            differ.is_empty(),
            "{} of {}:\n{}",
            differ.len(),
            patterns.len(),
            differ.join("\n")
        );
    }

    /// The same with `-i`, over lines in either case.
    #[test]
    fn it_ignores_case_as_gnu_grep_does() {
        let lines = ["", "a", "A", "Ab", "aB", "AB", "b", "Z", "ba", "B.A", "_"];
        let alphabet = ["a", "B", ".", "*", "^", "[^a]", "[A-b]", "[b-z]", "[^B-Z]"];
        let patterns = every_pattern(&alphabet, 2);
        let differ = unlike_gnu(&patterns, &lines, true);
        assert!(
            differ.is_empty(),
            "{} of {}:\n{}",
            differ.len(),
            patterns.len(),
            differ.join("\n")
        );
    }
}
