//! The code of a Rust source file without its comments, for the tests
//! that scan sources for what they must not use: the loader's banned
//! firmware features, and the kernel's TLB instructions outside `arch/`
//! (which includes this file by its path, since the kernel has no
//! dev-dependencies).

/// The code of a source file without its `//` comments, which may
/// explain a banned feature. A `//` in a string literal is code, raw
/// (`r#"…"#`) or not, and so is a `"` in a character literal (`'"'`); a
/// lifetime (`'a`) is neither.
pub fn code(src: &str) -> String {
    let mut out = String::new();
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&n| n != '\n').is_some() {},
            'r' if {
                let mut ahead = chars.clone();
                while ahead.next_if_eq(&'#').is_some() {}
                ahead.next() == Some('"')
            } =>
            {
                // To the `"` followed by as many `#` as began it.
                out.push(c);
                let mut hashes = 0;
                while let Some(h) = chars.next_if_eq(&'#') {
                    out.push(h);
                    hashes += 1;
                }
                out.extend(chars.next());
                while let Some(n) = chars.next() {
                    out.push(n);
                    if n == '"' {
                        let mut seen = 0;
                        while seen < hashes {
                            let Some(h) = chars.next_if_eq(&'#') else {
                                break;
                            };
                            out.push(h);
                            seen += 1;
                        }
                        if seen == hashes {
                            break;
                        }
                    }
                }
            }
            '"' => {
                in_string = true;
                out.push(c);
            }
            '\'' => {
                out.push(c);
                let mut ahead = chars.clone();
                let literal = match ahead.next() {
                    Some('\\') => true,
                    Some(_) => ahead.next() == Some('\''),
                    None => false,
                };
                if literal {
                    // Its character (escaped or not) and closing quote.
                    let first = chars.next();
                    out.extend(first);
                    if first == Some('\\') {
                        out.extend(chars.next());
                    }
                    for n in chars.by_ref() {
                        out.push(n);
                        if n == '\'' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_do_not_count() {
        assert_eq!(
            code("let x = 1; // Exclusive\n/// Exclusive"),
            "let x = 1; \n"
        );
    }

    #[test]
    fn a_string_is_code_even_with_slashes_in_it() {
        // Milestone 1's deferred finding: `//` in a string was cut as a
        // comment, and a banned word after it was not seen.
        let line = r#"let p = "a//b"; open("Exclusive"); // Exclusive"#;
        assert_eq!(code(line), r#"let p = "a//b"; open("Exclusive"); "#);
        let escaped = r#"let s = "\"//"; x(); // y"#;
        assert_eq!(code(escaped), r#"let s = "\"//"; x(); "#);
        let chars = r#"let q = '"'; let e = '\''; let d = '\"'; f(); // "g""#;
        assert_eq!(
            code(chars),
            r#"let q = '"'; let e = '\''; let d = '\"'; f(); "#
        );
        let lifetime = r#"fn f(s: &'static str) -> &str { "it's" } // x"#;
        assert_eq!(
            code(lifetime),
            r#"fn f(s: &'static str) -> &str { "it's" } "#
        );
        let two_lines = "let s = \"a\n//b\"; // c\nd";
        assert_eq!(code(two_lines), "let s = \"a\n//b\"; \nd");
    }

    #[test]
    fn a_raw_string_is_code_whatever_it_holds() {
        // Milestone 2's plan 5's deferred finding: the scanner assumed the
        // loader had no raw strings. A raw string has no escapes and ends
        // at a `"` followed by as many `#` as it began with.
        let raw = r##"let a = r"\"; f(); // x"##;
        assert_eq!(code(raw), r##"let a = r"\"; f(); "##);
        let hashes = r###"let b = r#"a"//"#; g(); // y"###;
        assert_eq!(code(hashes), r###"let b = r#"a"//"#; g(); "###);
        let bytes = r###"let c = br##"x"#//"##; h(); // z"###;
        assert_eq!(code(bytes), r###"let c = br##"x"#//"##; h(); "###);
        // Not raw strings: a name ending in `r`, a raw identifier, and
        // `r"` inside a string.
        let others = r#"let d = (bar, r#type, "r\"//"); // w"#;
        assert_eq!(code(others), r#"let d = (bar, r#type, "r\"//"); "#);
    }
}
