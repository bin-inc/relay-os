//! Host-testable parts of the Relay OS UEFI loader.
#![cfg_attr(not(test), no_std)]

pub mod cmdline;
pub mod elf;
pub mod memmap;
pub mod mode;

#[cfg(test)]
mod tests {
    /// Every source file of the loader but this one, which names the banned
    /// words itself.
    const SOURCES: [(&str, &str); 8] = [
        ("cmdline.rs", include_str!("cmdline.rs")),
        ("elf.rs", include_str!("elf.rs")),
        ("main.rs", include_str!("main.rs")),
        ("memmap.rs", include_str!("memmap.rs")),
        ("mode.rs", include_str!("mode.rs")),
        ("paging.rs", include_str!("paging.rs")),
        ("proto.rs", include_str!("proto.rs")),
        ("video.rs", include_str!("video.rs")),
    ];

    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature. A `//` in a string literal is code, and
    /// so is a `"` in a character literal (`'"'`); a lifetime (`'a`) is
    /// neither. The loader has no raw strings.
    fn code(src: &str) -> String {
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
                '/' if chars.peek() == Some(&'/') => {
                    while chars.next_if(|&n| n != '\n').is_some() {}
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

    #[test]
    fn every_source_file_is_checked() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f != "lib.rs")
            .collect();
        files.sort();
        let checked: Vec<&str> = SOURCES.iter().map(|(f, _)| *f).collect();
        assert_eq!(files, checked);
    }

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

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
    /// drivers (the text console stops drawing once the GOP is held) and a
    /// later ExitBootServices never returns. Like the Linux EFI stub, the
    /// loader only uses non-exclusive GET_PROTOCOL opens (`proto::get`).
    #[test]
    fn loader_avoids_firmware_features_linux_does_not_use() {
        for (file, src) in SOURCES {
            let src = code(src);
            // OS-defined memory types (0x8000_0000+) are legal but never used
            // by the Linux EFI stub; the loader sticks to standard types.
            for banned in [
                "open_protocol_exclusive",
                "get_image_file_system",
                "Exclusive",
                "MemoryType::custom",
            ] {
                assert!(!src.contains(banned), "{file} uses {banned}");
            }
        }
    }
}
