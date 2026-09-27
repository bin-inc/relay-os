//! Text for the e2e `key` step, as QMP `send-key` presses: each press is the
//! QEMU key codes (qcodes) held down together, Shift plus a letter for a
//! capital. `{name}` is a key without a character: `{up}`, `{down}`,
//! `{left}`, `{right}`, `{home}`, `{end}`, `{delete}`, `{backspace}`,
//! `{tab}`, `{esc}`, `{ret}`, `{caps_lock}` and `{ctrl-<letter>}`. Enter
//! follows the text, as with `send`.

use anyhow::{Result, bail};

const LETTERS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
/// Characters on the US layout that are not letters or digits: the
/// character, its key, and whether Shift is needed.
const SYMBOLS: [(char, &str, bool); 32] = [
    (' ', "spc", false),
    ('-', "minus", false),
    ('_', "minus", true),
    ('=', "equal", false),
    ('+', "equal", true),
    ('[', "bracket_left", false),
    ('{', "bracket_left", true),
    (']', "bracket_right", false),
    ('}', "bracket_right", true),
    ('\\', "backslash", false),
    ('|', "backslash", true),
    (';', "semicolon", false),
    (':', "semicolon", true),
    ('\'', "apostrophe", false),
    ('"', "apostrophe", true),
    ('`', "grave_accent", false),
    ('~', "grave_accent", true),
    (',', "comma", false),
    ('<', "comma", true),
    ('.', "dot", false),
    ('>', "dot", true),
    ('/', "slash", false),
    ('?', "slash", true),
    ('!', "1", true),
    ('@', "2", true),
    ('#', "3", true),
    ('$', "4", true),
    ('%', "5", true),
    ('^', "6", true),
    ('&', "7", true),
    ('*', "8", true),
    ('(', "9", true),
];

/// The key for `c` and whether it needs Shift.
fn char_key(c: char) -> Option<(&'static str, bool)> {
    match c {
        'a'..='z' => Some((LETTERS[c as usize - 'a' as usize], false)),
        'A'..='Z' => Some((LETTERS[c as usize - 'A' as usize], true)),
        '0'..='9' => Some((DIGITS[c as usize - '0' as usize], false)),
        ')' => Some(("0", true)),
        _ => SYMBOLS
            .iter()
            .find(|s| s.0 == c)
            .map(|&(_, key, shift)| (key, shift)),
    }
}

/// Keys named in braces; the names are QEMU's qcodes.
const NAMED: [&str; 12] = [
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "delete",
    "backspace",
    "tab",
    "esc",
    "ret",
    "caps_lock",
];

fn special(name: &str) -> Option<Vec<&'static str>> {
    if let Some(&key) = NAMED.iter().find(|&&k| k == name) {
        return Some(vec![key]);
    }
    let mut letter = name.strip_prefix("ctrl-")?.chars();
    match (letter.next(), letter.next()) {
        (Some(c @ 'a'..='z'), None) => Some(vec!["ctrl", LETTERS[c as usize - 'a' as usize]]),
        _ => None,
    }
}

/// The presses that type `text` and then Enter.
pub fn presses(text: &str) -> Result<Vec<Vec<&'static str>>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            let Some(end) = rest.find('}') else {
                bail!("unterminated {{ in key text {text:?}");
            };
            let name = &rest[1..end];
            match special(name) {
                Some(keys) => out.push(keys),
                None => bail!("unknown key {{{name}}} in {text:?}"),
            }
            rest = &rest[end + 1..];
            continue;
        }
        match char_key(c) {
            Some((key, true)) => out.push(vec!["shift", key]),
            Some((key, false)) => out.push(vec![key]),
            None => bail!("no key types {c:?} on a US keyboard"),
        }
        rest = &rest[c.len_utf8()..];
    }
    out.push(vec!["ret"]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_become_keys_with_shift_where_needed() {
        assert_eq!(
            presses("echo Hi!").unwrap(),
            [
                vec!["e"],
                vec!["c"],
                vec!["h"],
                vec!["o"],
                vec!["spc"],
                vec!["shift", "h"],
                vec!["i"],
                vec!["shift", "1"],
                vec!["ret"],
            ]
        );
        assert_eq!(presses("").unwrap(), [vec!["ret"]]);
        let all = "abcxyzABCXYZ0123456789 -_=+[]\\|;:'\"`~,<.>/?!@#$%^&*()";
        assert_eq!(presses(all).unwrap().len(), all.len() + 1);
        assert_eq!(presses(")").unwrap()[0], ["shift", "0"]);
        assert_eq!(presses("|").unwrap()[0], ["shift", "backslash"]);
    }

    #[test]
    fn braces_name_keys_without_characters() {
        assert_eq!(
            presses("ls{backspace}{up}{caps_lock}{ctrl-c}").unwrap(),
            [
                vec!["l"],
                vec!["s"],
                vec!["backspace"],
                vec!["up"],
                vec!["caps_lock"],
                vec!["ctrl", "c"],
                vec!["ret"],
            ]
        );
    }

    #[test]
    fn unknown_keys_and_characters_are_errors() {
        assert!(presses("{nope}").is_err());
        assert!(presses("{ctrl-}").is_err());
        assert!(presses("{ctrl-ab}").is_err());
        assert!(presses("{up").is_err());
        assert!(presses("é").is_err());
        assert!(presses("\t").is_err());
    }
}
