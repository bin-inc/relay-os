//! `export` and `unset`, the shell's commands for its variables
//! (programmable shell gate §8.5), as bash 5.2's.

use crate::ctx::{Ctx, outln};
use crate::parser::is_name;
use crate::shell::NAME;
use alloc::format;
use alloc::string::String;

/// `export [-p] [NAME[=value]...]`: marks each NAME for the environment of
/// the programs the shell starts, giving it the value if one follows, and
/// goes on past a name that is not one (status 1). With no NAME it lists
/// the exported variables as bash's `declare -x` does. `-n` and `-f`
/// (bash's un-export and functions) are not supported.
pub fn export(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let names = match options(ctx, "export", args, "fnp", "fn") {
        Ok((_, names)) => names,
        Err(status) => return status,
    };
    if names.is_empty() {
        return list(ctx);
    }
    let mut status = 0;
    for arg in names {
        let (name, value) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(String::from(value))),
            None => (arg.as_str(), None),
        };
        if !is_name(name) {
            status = ctx.fail(
                NAME,
                format_args!("export: `{arg}': not a valid identifier"),
            );
            continue;
        }
        if let Err(e) = ctx.vars().export(name, value) {
            status = ctx.fail(NAME, format_args!("{e}"));
        }
    }
    status
}

/// `unset [-v] [NAME...]`: removes each variable, exported or not. As in
/// bash, without `-v` a name that is not one is passed over (bash looks
/// for a function of that name too); with it, it fails (status 1). `-f`
/// (functions) and `-n` (name references) are not supported.
pub fn unset(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (letters, names) = match options(ctx, "unset", args, "fnv", "fn") {
        Ok(read) => read,
        Err(status) => return status,
    };
    let only_variables = letters.contains('v');
    let mut status = 0;
    for name in names {
        if is_name(name) {
            ctx.vars().unset(name);
        } else if only_variables {
            status = ctx.fail(
                NAME,
                format_args!("unset: `{name}': not a valid identifier"),
            );
        }
    }
    status
}

/// A built-in's option letters and the words after them, as bash reads
/// them: options come first, a word `-` or `--` ends them (`--` dropped),
/// and once all are read a letter not in `known` is bash's `invalid
/// option` (status 2, without its usage line), else one of `refused`,
/// which bash knows, is not supported (status 1).
pub(super) fn options<'a>(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &'a [String],
    known: &str,
    refused: &str,
) -> Result<(String, &'a [String]), i32> {
    let mut first = 0;
    let mut seen = String::new();
    for arg in args {
        if arg == "--" {
            first += 1;
            break;
        }
        let Some(letters) = arg.strip_prefix('-').filter(|l| !l.is_empty()) else {
            break;
        };
        seen.push_str(letters);
        first += 1;
    }
    // Every option is read before any is judged, as bash's are.
    if let Some(bad) = seen.chars().find(|c| !known.contains(*c)) {
        ctx.fail(NAME, format_args!("{name}: -{bad}: invalid option"));
        return Err(2);
    }
    if let Some(no) = seen.chars().find(|c| refused.contains(*c)) {
        return Err(ctx.fail(NAME, format_args!("{name}: -{no}: not supported")));
    }
    Ok((seen, &args[first..]))
}

/// The exported variables, sorted by name, as bash's `export -p`.
fn list(ctx: &mut Ctx<'_>) -> i32 {
    let lines: alloc::vec::Vec<String> = ctx
        .vars()
        .exported()
        .map(|(name, value)| match value {
            Some(v) => format!("declare -x {name}={}", quoted(v)),
            None => format!("declare -x {name}"),
        })
        .collect();
    for line in lines {
        outln!(ctx, "{line}");
    }
    0
}

/// A value as bash 5.2 writes it after `declare -x NAME=` in the C locale:
/// in double quotes, `"`, `$`, `` ` `` and `\` escaped, unless it holds a
/// byte that is no printable ASCII; then in `$'…'`, as bash's
/// `ansic_quote` writes it.
fn quoted(value: &str) -> String {
    let printable = |b: u8| (b' '..=b'~').contains(&b);
    let mut out = String::new();
    if value.bytes().all(printable) {
        out.push('"');
        for c in value.chars() {
            if matches!(c, '"' | '$' | '`' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        return out;
    }
    out.push_str("$'");
    for b in value.bytes() {
        match b {
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x1b => out.push_str("\\E"),
            0x0c => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x0b => out.push_str("\\v"),
            b'\'' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            b if printable(b) => out.push(char::from(b)),
            b => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::quoted;
    use crate::testing::Harness;

    #[test]
    fn export_marks_variables_and_lists_them_as_bash_does() {
        // `env -i HOME=/root bash` in a pty, from `/`, but for `HOME` and
        // `SHLVL`.
        let mut h = Harness::new();
        let (status, out) = h.lines(&[
            "A=1",
            "export A B",
            "export C=3 D= E='a \"b\" $c `d` \\e'",
            "export",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "declare -x A=\"1\"\n\
             declare -x B\n\
             declare -x C=\"3\"\n\
             declare -x D=\"\"\n\
             declare -x E=\"a \\\"b\\\" \\$c \\`d\\` \\\\e\"\n\
             declare -x OLDPWD\n\
             declare -x PWD=\"/\"\n"
        );
        assert_eq!(
            h.lines(&["export Z=1", "unset OLDPWD PWD", "export -p"]).1,
            "declare -x Z=\"1\"\n"
        );
        // `-p` with names exports them, and `--` ends the options.
        assert_eq!(
            h.lines(&["export -p Y", "export -- X=-", "unset OLDPWD PWD", "export"])
                .1,
            "declare -x X=\"-\"\ndeclare -x Y\n"
        );
    }

    #[test]
    fn a_value_bash_would_not_print_as_it_is_is_written_with_escapes() {
        // As bash 5.2's `export -p` in the C locale.
        assert_eq!(quoted("plain words"), "\"plain words\"");
        assert_eq!(quoted("x\ny"), "$'x\\ny'");
        assert_eq!(quoted("\t"), "$'\\t'");
        assert_eq!(quoted("été"), "$'\\303\\251t\\303\\251'");
        assert_eq!(
            quoted("\x07\x08\x1b\x0c\n\r\t\x0b\x7f\x01"),
            "$'\\a\\b\\E\\f\\n\\r\\t\\v\\177\\001'"
        );
        assert_eq!(quoted("é$`\""), "$'\\303\\251$`\"'");
        assert_eq!(quoted("\t'\\\\"), "$'\\t\\'\\\\\\\\'");
        assert_eq!(quoted("it's"), "\"it's\"");
    }

    #[test]
    fn export_names_each_bad_identifier_and_goes_on() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["export 1A=x B=2 A-B - =x", "export"]),
            (
                0,
                "relay-sh: export: `1A=x': not a valid identifier\n\
                 relay-sh: export: `A-B': not a valid identifier\n\
                 relay-sh: export: `-': not a valid identifier\n\
                 relay-sh: export: `=x': not a valid identifier\n\
                 declare -x B=\"2\"\n\
                 declare -x OLDPWD\n\
                 declare -x PWD=\"/\"\n"
                    .into()
            )
        );
        assert_eq!(h.run("export 1A").0, 1);
    }

    #[test]
    fn export_s_other_options_are_refused() {
        let mut h = Harness::new();
        for (line, status, message) in [
            ("export -n A", 1, "relay-sh: export: -n: not supported\n"),
            ("export -f f", 1, "relay-sh: export: -f: not supported\n"),
            ("export -pn A", 1, "relay-sh: export: -n: not supported\n"),
            // bash's, without its usage line.
            ("export -x A", 2, "relay-sh: export: -x: invalid option\n"),
            ("export -nx A", 2, "relay-sh: export: -x: invalid option\n"),
            // bash reads every option before it judges them.
            (
                "export -n -x A",
                2,
                "relay-sh: export: -x: invalid option\n",
            ),
        ] {
            assert_eq!(
                h.lines(&[line, "unset OLDPWD PWD", "export"]),
                (0, message.into()),
                "{line}"
            );
            assert_eq!(h.run(line).0, status, "{line}");
        }
    }

    #[test]
    fn unset_removes_variables_exported_or_not() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&[
                "A=1 B=2 C=3",
                "export B D",
                "unset A B",
                "unset -v -- D NEVER OLDPWD PWD",
                "echo [$A$B$C]",
                "export",
            ]),
            (0, "[3]\n".into())
        );
        assert_eq!(h.run("unset"), (0, "".into()));
        // Options end at the first name: a later `-v` is a name.
        assert_eq!(h.run("unset A -v"), (0, "".into()));
    }

    #[test]
    fn unset_passes_over_a_bad_name_unless_given_v() {
        // bash looks for a function of that name too, without `-v`.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["A=1", "unset 1A A=1 A", "echo [$A]"]),
            (0, "[]\n".into())
        );
        assert_eq!(
            h.lines(&["A=1 B=2", "unset -v 1A A A=1 B", "echo $? [$A$B]"]),
            (
                0,
                "relay-sh: unset: `1A': not a valid identifier\n\
                 relay-sh: unset: `A=1': not a valid identifier\n\
                 1 []\n"
                    .into()
            )
        );
        for (line, status, message) in [
            ("unset -f f", 1, "relay-sh: unset: -f: not supported\n"),
            ("unset -fv A", 1, "relay-sh: unset: -f: not supported\n"),
            // bash's `-n` (name references) is not supported either.
            ("unset -n A", 1, "relay-sh: unset: -n: not supported\n"),
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
            ("unset -f -x A", 2, "relay-sh: unset: -x: invalid option\n"),
        ] {
            assert_eq!(h.run(line), (status, message.into()), "{line}");
        }
    }

    #[test]
    fn export_s_assignments_expand_as_assignments() {
        // A `~` after `=` or `:`, and `$@` joined by blanks, as bash's
        // `export` (a declaration command) has them; a plain argument
        // keeps its `~` (user-space gate §16 item 11). A quoted `export`
        // is `export` too, as in bash.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"export A=~/x B=x:~ \"C=~/y\" D=$@\n\"export\" E=~/z\nexport\necho a=~/x\n",
        );
        let (status, said) = h.run("sh /tmp/s.sh 1 '2 3'");
        assert_eq!(status, 0);
        assert!(
            said.contains(
                "declare -x A=\"/root/x\"\n\
                 declare -x B=\"x:/root\"\n\
                 declare -x C=\"~/y\"\n\
                 declare -x D=\"1 2 3\"\n\
                 declare -x E=\"/root/z\"\n"
            ),
            "{said}"
        );
        assert!(said.ends_with("a=~/x\n"), "{said}");
    }

    #[test]
    fn a_variable_export_cannot_hold_fails() {
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 100);
        assert_eq!(
            h.lines(&[
                &alloc::format!("B={big}"),
                &alloc::format!("export A={}", &big[..200]),
                "unset OLDPWD PWD",
                "export"
            ]),
            (
                0,
                "relay-sh: A: the variables would hold more than 64 KiB\n".into()
            )
        );
    }
}
