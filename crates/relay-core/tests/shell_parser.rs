use relay_core::shell::{Command, ParseError, parse_command, tokenize};

fn tokens(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).into()).collect()
}

#[test]
fn quoted_fragments_and_empty_arguments_are_preserved() {
    assert_eq!(
        tokenize("write a x\" y\" ''").unwrap(),
        tokens(&["write", "a", "x y", ""])
    );
    assert!(matches!(
        parse_command("write a ''").unwrap(),
        Some(Command::Write { path, text }) if path == "a" && text.is_empty()
    ));
}

#[test]
fn quotes_and_backslashes_follow_shell_states() {
    assert_eq!(
        tokenize("echo a' b'c \"d\\ e\" 'f\\g'").unwrap(),
        tokens(&["echo", "a bc", "d e", "f\\g"])
    );
    assert!(matches!(
        tokenize("echo 'unterminated"),
        Err(ParseError::UnterminatedQuote)
    ));
    assert!(matches!(
        tokenize("echo abc\\"),
        Err(ParseError::InvalidEscape)
    ));
}

#[test]
fn whitespace_separates_tokens_and_empty_lines_have_no_command() {
    assert_eq!(
        tokenize("\t echo\rhello\nworld ").unwrap(),
        tokens(&["echo", "hello", "world"])
    );
    assert_eq!(tokenize(" \t\r\n ").unwrap(), Vec::<String>::new());
    assert!(parse_command("\t\r\n").unwrap().is_none());
}

#[test]
fn empty_quotes_and_adjacent_fragments_construct_arguments() {
    assert_eq!(
        tokenize("echo '' \"\" a\" b\"' c'd").unwrap(),
        tokens(&["echo", "", "", "a b cd"])
    );
}

#[test]
fn escapes_require_printable_ascii_outside_and_inside_double_quotes() {
    assert_eq!(
        tokenize("echo a\\ b \"c\\ d\"").unwrap(),
        tokens(&["echo", "a b", "c d"])
    );
    assert!(matches!(
        tokenize("echo \\\t"),
        Err(ParseError::InvalidEscape)
    ));
    assert!(matches!(
        tokenize("echo \"\\\n\""),
        Err(ParseError::InvalidEscape)
    ));
}

#[test]
fn invalid_input_categories_are_typed() {
    assert!(matches!(
        tokenize("echo cafe\u{e9}"),
        Err(ParseError::NonAscii)
    ));
    assert!(matches!(tokenize("echo\0x"), Err(ParseError::ControlByte)));
    assert!(matches!(
        tokenize(&"a".repeat(513)),
        Err(ParseError::InputTooLong)
    ));
}

#[test]
fn parser_returns_owned_read_commands_with_exact_arities() {
    assert!(matches!(
        parse_command("help").unwrap(),
        Some(Command::Help)
    ));
    assert!(matches!(parse_command("pwd").unwrap(), Some(Command::Pwd)));
    assert!(matches!(
        parse_command("cd docs").unwrap(),
        Some(Command::Cd { path }) if path == "docs"
    ));
    assert!(matches!(
        parse_command("ls").unwrap(),
        Some(Command::Ls { path: None })
    ));
    assert!(matches!(
        parse_command("ls docs").unwrap(),
        Some(Command::Ls { path: Some(path) }) if path == "docs"
    ));
    assert!(matches!(
        parse_command("cat readme").unwrap(),
        Some(Command::Cat { path }) if path == "readme"
    ));
    assert!(matches!(
        parse_command("echo one two").unwrap(),
        Some(Command::Echo { text }) if text == tokens(&["one", "two"])
    ));
    assert!(matches!(
        parse_command("echo").unwrap(),
        Some(Command::Echo { text }) if text.is_empty()
    ));

    for line in [
        "help extra",
        "pwd extra",
        "cd",
        "cd a b",
        "ls a b",
        "cat",
        "cat a b",
    ] {
        assert!(
            matches!(parse_command(line), Err(ParseError::Arity { .. })),
            "{line}"
        );
    }
}

#[test]
fn parser_returns_owned_mutation_variants_without_execution() {
    assert!(matches!(
        parse_command("touch file").unwrap(),
        Some(Command::Touch { path }) if path == "file"
    ));
    assert!(matches!(
        parse_command("write file several words").unwrap(),
        Some(Command::Write { path, text }) if path == "file" && text == "several words"
    ));
    assert!(matches!(
        parse_command("write file").unwrap(),
        Some(Command::Write { path, text }) if path == "file" && text.is_empty()
    ));
    assert!(matches!(
        parse_command("append file more words").unwrap(),
        Some(Command::Append { path, text }) if path == "file" && text == "more words"
    ));
    assert!(matches!(
        parse_command("append file").unwrap(),
        Some(Command::Append { path, text }) if path == "file" && text.is_empty()
    ));
    assert!(matches!(
        parse_command("mkdir dir").unwrap(),
        Some(Command::Mkdir { path }) if path == "dir"
    ));
    assert!(matches!(
        parse_command("rm file").unwrap(),
        Some(Command::Rm { path }) if path == "file"
    ));
    assert!(matches!(
        parse_command("rmdir dir").unwrap(),
        Some(Command::Rmdir { path }) if path == "dir"
    ));
    assert!(matches!(
        parse_command("sync").unwrap(),
        Some(Command::Sync)
    ));
    assert!(matches!(
        parse_command("shutdown").unwrap(),
        Some(Command::Shutdown)
    ));

    for line in [
        "touch",
        "touch a b",
        "write",
        "append",
        "mkdir",
        "mkdir a b",
        "rm",
        "rm a b",
        "rmdir",
        "rmdir a b",
        "sync now",
        "shutdown now",
    ] {
        assert!(
            matches!(parse_command(line), Err(ParseError::Arity { .. })),
            "{line}"
        );
    }
}

#[test]
fn parser_rejects_unknown_command_names() {
    assert!(matches!(
        parse_command("relay"),
        Err(ParseError::UnknownCommand)
    ));
}
