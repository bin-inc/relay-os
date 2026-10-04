//! `cd`, `exit`, `pwd`, `echo`, `clear`, `help` and `uname`.

use super::export::options;
use super::{BUILTINS, COMMANDS};
use crate::ctx::{Ctx, getopt, outln};
use crate::shell::NAME;
use alloc::string::String;
use vfs::path;

/// The system name `uname` prints, and all of `uname -a`.
pub const UNAME: &str = "Relay";
pub const UNAME_ALL: &str = concat!("Relay relay ", env!("CARGO_PKG_VERSION"), " x86_64");

/// `cd [-L|-P] [--] [dir]` (programmable shell gate §9.1), as bash 5.2's:
/// no directory goes to `$HOME`, `-` to `$OLDPWD`, printing it, and an
/// empty one, or an empty `HOME` or `OLDPWD`, nowhere (`cd -` printing an
/// empty line). On success, staying where it is too, `OLDPWD` takes
/// `PWD`'s value, or none when `PWD` has none, and `PWD` the new
/// directory, each exported or not as it was. `-L` and
/// `-P` change nothing, as no symbolic link is followed; bash's `-e` is
/// not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (_, args) = match options(ctx, "cd", args, "LPe", "e") {
        Ok(read) => read,
        Err(status) => return status,
    };
    let (dir, show) = match args {
        [] => match ctx.vars().value("HOME") {
            None => return ctx.fail(NAME, format_args!("cd: HOME not set")),
            Some(home) => (String::from(home), false),
        },
        [dir] if dir == "-" => match ctx.vars().value("OLDPWD") {
            None => return ctx.fail(NAME, format_args!("cd: OLDPWD not set")),
            Some(old) => (String::from(old), true),
        },
        [dir] => (dir.clone(), false),
        _ => return ctx.fail(NAME, format_args!("cd: too many arguments")),
    };
    // An empty directory stays where it is, a cd that succeeded.
    if !dir.is_empty()
        && let Err(e) = ctx.vfs.chdir(dir.as_bytes())
    {
        return ctx.fail(NAME, format_args!("cd: {dir}: {e}"));
    }
    let cwd = path::display(&ctx.vfs.cwd());
    let vars = ctx.vars();
    let oldpwd = match vars.value("PWD").map(String::from) {
        Some(old) => vars.set("OLDPWD", old),
        None => vars.clear("OLDPWD"),
    };
    let pwd = vars.set("PWD", cwd);
    // It went there all the same, as bash would; the variables say not.
    let status = match oldpwd.and(pwd) {
        Ok(()) => 0,
        Err(e) => ctx.fail(NAME, format_args!("{e}")),
    };
    // `cd -` says `$OLDPWD` as it is written, as bash's does.
    if show {
        outln!(ctx, "{dir}");
    }
    status
}

/// `exit [code]`: the shell stops with `code` (modulo 256), or the last
/// command's status. As in bash, a code that is not a 64-bit number is
/// status 2 and the shell still stops; more than one argument stops
/// nothing; `--` before the code is dropped.
pub fn exit(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let args = match args {
        [dashes, rest @ ..] if dashes == "--" => rest,
        _ => args,
    };
    let status = match args {
        [] => ctx.status,
        [code] => match parse_code(code) {
            Some(n) => n,
            None => {
                ctx.fail(
                    NAME,
                    format_args!("exit: {code}: numeric argument required"),
                );
                2
            }
        },
        _ => return ctx.fail(NAME, format_args!("exit: too many arguments")),
    };
    ctx.exit = true;
    ctx.exited = true;
    status
}

/// A decimal number with an optional sign and blanks around it, as bash
/// takes it: one that fits in 64 bits, modulo 256.
fn parse_code(code: &str) -> Option<i32> {
    let code = code.trim_matches([' ', '\t']);
    let digits = code.strip_prefix(['-', '+']).unwrap_or(code);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = code.parse().ok()?;
    Some(n.rem_euclid(256) as i32)
}

/// `pwd`: arguments are ignored, as in bash.
pub fn pwd(ctx: &mut Ctx<'_>, _: &[String]) -> i32 {
    let cwd = path::display(&ctx.vfs.cwd());
    outln!(ctx, "{cwd}");
    0
}

/// `true`: nothing, successfully; its arguments are ignored, as GNU's are.
pub fn r#true(_: &mut Ctx<'_>, _: &[String]) -> i32 {
    0
}

/// `false`: nothing, unsuccessfully.
pub fn r#false(_: &mut Ctx<'_>, _: &[String]) -> i32 {
    1
}

/// `echo [-n] args…`: the arguments separated by spaces; `-n` drops the
/// newline. Anything else starting with `-` is printed.
pub fn echo(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let is_n = |a: &String| a.len() > 1 && a.starts_with('-') && a[1..].bytes().all(|b| b == b'n');
    let skip = args.iter().take_while(|a| is_n(a)).count();
    let mut line = args[skip..].join(" ");
    if skip == 0 {
        line.push('\n');
    }
    ctx.out(line.as_bytes());
    0
}

pub fn clear(ctx: &mut Ctx<'_>, _: &[String]) -> i32 {
    ctx.out(b"\x1b[H\x1b[2J");
    0
}

/// `help`: the programs of `/bin`, then the shell's built-ins and its
/// syntax, so that those stay on a screen too short for the whole list.
pub fn help(ctx: &mut Ctx<'_>, _: &[String]) -> i32 {
    for (heading, builtin) in [("Programs in /bin:", false), ("Shell built-ins:", true)] {
        outln!(ctx, "{heading}");
        for b in COMMANDS
            .iter()
            .filter(|b| BUILTINS.contains(&b.name) == builtin)
        {
            outln!(ctx, "  {:<9} {}", b.name, b.help);
        }
    }
    outln!(
        ctx,
        "Send output to a file with `> file` (replace) or `>> file` (append),\n\
         errors with `2> file` or `2>> file` (both with `> file 2>&1`), or output\n\
         into another program with `| cmd` (built-ins cannot be in a pipeline);\n\
         `>&2` sends output to the errors, and `< file` reads a file as input.\n\
         End a command with `&` to run it in the background, or with `;` to run\n\
         the next one after it. `a && b` runs b if a succeeds, `a || b` if it\n\
         fails, and `! a` turns a's status round. `sh FILE ARG...` runs a script,\n\
         which reads its arguments as `$1`...`$9`, `${{10}}`..., `$#` and \"$@\".\n\
         `$?` is the last command's status. `NAME=value` sets `$NAME`,\n\
         `export NAME` gives it to the programs the shell starts, and\n\
         `NAME=value cmd` gives it to cmd alone. `~` is $HOME: `cd` alone goes\n\
         there, and `cd -` back to $OLDPWD. `if a; then b; fi` runs b if a\n\
         succeeds, with `elif c; then d;` and `else e;` before `fi` for other\n\
         cases; `while a; do b; done` repeats b while a succeeds, `until` while\n\
         it fails; `for x in w...; do b; done` runs b with each w as `$x`, and\n\
         `for x; do` with each argument. Each may go on across lines, at `> `,\n\
         and redirections after `done` or `fi` hold for the whole command."
    );
    0
}

/// `uname [-a]`.
pub fn uname(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "a", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("uname", format_args!("{e}")),
    };
    if let Some(extra) = opts.operands.first() {
        return ctx.fail(
            "uname",
            format_args!("extra operand {}", crate::ctx::quote(extra)),
        );
    }
    outln!(ctx, "{}", if opts.has('a') { UNAME_ALL } else { UNAME });
    0
}

#[cfg(test)]
mod tests {
    use crate::testing::Harness;

    #[test]
    fn true_and_false_ignore_their_arguments() {
        let mut h = Harness::new();
        assert_eq!(h.run("true"), (0, "".into()));
        assert_eq!(h.run("true --help x"), (0, "".into()));
        assert_eq!(h.run("false"), (1, "".into()));
        assert_eq!(h.run("false -x"), (1, "".into()));
    }

    #[test]
    fn cd_goes_home_without_an_argument() {
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(h.run("cd /etc"), (0, "".into()));
        assert_eq!(h.run("pwd").1, "/etc\n");
        assert_eq!(h.run("cd"), (0, "".into()));
        assert_eq!(h.run("pwd").1, "/root\n");
        h.run("cd ..");
        assert_eq!(h.run("pwd").1, "/\n");
        h.run("cd ~");
        assert_eq!(h.run("pwd").1, "/root\n");
    }

    #[test]
    fn cd_to_an_empty_directory_stays_where_it_is() {
        // As bash's does (status 0), so a script's `cd "$1"` without an
        // argument does nothing; `cd $E` unquoted gets no word and goes
        // home, as `cd` alone.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /etc", "cd \"\"", "pwd", "cd \"$1\"", "pwd"]),
            (0, "/etc\n/etc\n".into())
        );
        assert_eq!(h.lines(&["cd /etc", "E=", "cd $E", "pwd"]).1, "/root\n");
        assert_eq!(
            h.run("cd '' ''"),
            (1, "relay-sh: cd: too many arguments\n".into())
        );
    }

    #[test]
    fn cd_errors() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("cd /nope"),
            (1, "relay-sh: cd: /nope: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("cd /etc/motd"),
            (1, "relay-sh: cd: /etc/motd: Not a directory\n".into())
        );
        assert_eq!(
            h.run("cd a b"),
            (1, "relay-sh: cd: too many arguments\n".into())
        );
        assert_eq!(h.run("pwd").1, "/\n", "nothing changed the directory");
    }

    #[test]
    fn cd_s_options_are_bash_s() {
        // bash 5.2's, without the usage line; `-@` Ubuntu's bash does not
        // have, and its `-e` is not supported here.
        let mut h = Harness::new();
        for (line, status, said) in [
            ("cd -x /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
            ("cd -Lx /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
            ("cd -@ /tmp", 2, "relay-sh: cd: -@: invalid option\n"),
            ("cd -e /tmp", 1, "relay-sh: cd: -e: not supported\n"),
            ("cd -Pe /tmp", 1, "relay-sh: cd: -e: not supported\n"),
            ("cd -e -x /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
        ] {
            assert_eq!(h.run(line), (status, said.into()), "{line}");
        }
        assert_eq!(h.run("pwd").1, "/\n");
        for line in ["cd -L /tmp", "cd -P /etc", "cd -LP -- /tmp", "cd -LL /etc"] {
            assert_eq!(h.run(line), (0, "".into()), "{line}");
        }
        assert_eq!(h.run("pwd").1, "/etc\n");
    }

    #[test]
    fn cd_without_home_or_with_it_empty() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["cd /tmp", "cd", "pwd"]),
            (0, "relay-sh: cd: HOME not set\n/tmp\n".into())
        );
        assert_eq!(
            h.lines(&["HOME=", "cd /tmp", "cd", "echo $?", "pwd"]).1,
            "0\n/tmp\n"
        );
        assert_eq!(h.lines(&["HOME=/etc", "cd --", "pwd"]).1, "/etc\n");
    }

    #[test]
    fn cd_says_when_pwd_does_not_fit_the_variables() {
        // It goes there all the same, status 1 (the maintainer,
        // 2026-10-04).
        let mut h = Harness::new();
        let fill = alloc::format!("A={}", "x".repeat(crate::vars::VARS_MAX - 13));
        assert_eq!(
            h.lines(&[&fill, "cd /tmp", "echo $? \"[$PWD]\"", "pwd"]).1,
            "relay-sh: PWD: the variables would hold more than 64 KiB\n1 [/]\n/tmp\n"
        );
    }

    #[test]
    fn cd_dash_prints_oldpwd_as_it_is_written() {
        // As bash's; PWD is still getcwd's.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["OLDPWD=/tmp/../etc/", "cd -", "echo $PWD"]).1,
            "/tmp/../etc/\n/etc\n"
        );
        assert_eq!(
            h.lines(&["cd /", "OLDPWD=etc", "cd -", "pwd"]).1,
            "etc\n/etc\n"
        );
    }

    #[test]
    fn cd_to_an_empty_directory_still_sets_oldpwd() {
        // bash counts it a cd that succeeded: OLDPWD takes PWD's value.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /tmp", "cd /etc", "cd \"\"", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "[/etc][/etc]\n"
        );
        assert_eq!(
            h.lines(&["cd /tmp", "OLDPWD=", "cd -", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "\n[/tmp][/tmp]\n"
        );
        assert_eq!(
            h.lines(&["cd /tmp", "HOME=", "cd", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "[/tmp][/tmp]\n"
        );
    }

    #[test]
    fn cd_dash_goes_back_and_says_where() {
        // As bash 5.2 in a pty.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&[
                "cd -",
                "echo $?",
                "cd /tmp",
                "cd -",
                "cd -- -",
                "OLDPWD=",
                "cd -",
                "echo $? $PWD",
                "unset OLDPWD",
                "cd -",
            ]),
            (
                1,
                "relay-sh: cd: OLDPWD not set\n1\n/\n/tmp\n\n0 /tmp\n\
                 relay-sh: cd: OLDPWD not set\n"
                    .into()
            )
        );
        assert_eq!(
            h.lines(&["OLDPWD=/nope", "cd -"]),
            (1, "relay-sh: cd: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("cd - x").1, "relay-sh: cd: too many arguments\n");
    }

    #[test]
    fn cd_sets_oldpwd_and_pwd_keeping_whether_they_are_exported() {
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /root/../tmp", "echo $PWD $OLDPWD", "export"])
                .1,
            "/tmp /\n\
             declare -x HOME=\"/root\"\n\
             declare -x OLDPWD=\"/\"\n\
             declare -x PWD=\"/tmp\"\n"
        );
        // A failed cd changes neither.
        assert_eq!(
            h.lines(&["cd /etc", "cd /nope", "echo $PWD $OLDPWD"]).1,
            "relay-sh: cd: /nope: No such file or directory\n/etc /tmp\n"
        );
        // OLDPWD takes the variable PWD's value; without one it stays a
        // variable with no value, exported as it was, as bash's does.
        assert_eq!(h.lines(&["PWD=/x", "cd /tmp", "echo $OLDPWD"]).1, "/x\n");
        assert_eq!(
            h.lines(&[
                "cd /etc",
                "unset PWD",
                "cd /tmp",
                "cd -",
                "echo $?",
                "export"
            ])
            .1,
            "relay-sh: cd: OLDPWD not set\n1\n\
             declare -x HOME=\"/root\"\n\
             declare -x OLDPWD\n"
        );
        // Made by cd, neither is exported (bash's `declare --`).
        assert_eq!(
            h.lines(&[
                "unset PWD OLDPWD",
                "cd /etc",
                "cd /tmp",
                "export",
                "echo $PWD $OLDPWD"
            ])
            .1,
            "declare -x HOME=\"/root\"\n/tmp /etc\n"
        );
    }

    #[test]
    fn exit_stops_the_shell_with_its_code() {
        let mut h = Harness::new();
        h.console.type_in(b"exit 3\recho never\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 3);
        assert_eq!(h.console.take(), "root@relay:/# exit 3\n");
        // Modulo 256, as in bash, and a sign may come first.
        for (code, status) in [
            ("256", 0),
            ("257", 1),
            ("-1", 255),
            ("+7", 7),
            ("1000", 232),
            ("' 5 '", 5),
            ("010", 10),
            ("9223372036854775807", 255),
            ("-9223372036854775808", 0),
            ("-- 3", 3),
        ] {
            let line = alloc::format!("exit {code}");
            assert_eq!(h.run(&line), (status, "".into()), "{code}");
        }
    }

    #[test]
    fn exit_without_a_code_keeps_the_last_status() {
        let mut h = Harness::new();
        h.console.type_in(b"nope\rexit\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 127);
    }

    #[test]
    fn exit_errors() {
        let mut h = Harness::new();
        // Not a number: status 2, and the shell stops anyway.
        h.console.type_in(b"exit abc\recho never\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 2);
        assert_eq!(
            h.console.take(),
            "root@relay:/# exit abc\nrelay-sh: exit: abc: numeric argument required\n"
        );
        // Past what bash's 64-bit number holds, or not a number at all.
        for code in [
            "9223372036854775808",
            "99999999999999999999",
            "''",
            "-",
            "0x10",
        ] {
            let line = alloc::format!("exit {code}");
            assert_eq!(h.run(&line).0, 2, "{code}");
        }
        // `--` ends the options, as for any command.
        h.console.type_in(b"nope\rexit --\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 127);
        h.console.take();
        // Too many: nothing stops.
        h.console.type_in(b"exit 1 2\recho still\r");
        crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert!(
            h.console
                .take()
                .contains("relay-sh: exit: too many arguments\nroot@relay:/# echo still\nstill\n")
        );
    }

    #[test]
    fn pwd_ignores_arguments() {
        let mut h = Harness::new();
        assert_eq!(h.run("pwd extra"), (0, "/\n".into()));
    }

    #[test]
    fn echo_joins_its_arguments() {
        let mut h = Harness::new();
        assert_eq!(h.run("echo hello   world"), (0, "hello world\n".into()));
        assert_eq!(h.run("echo 'two  spaces'"), (0, "two  spaces\n".into()));
        assert_eq!(h.run("echo"), (0, "\n".into()));
        assert_eq!(h.run("echo -n no newline"), (0, "no newline".into()));
        assert_eq!(h.run("echo -nn -n x"), (0, "x".into()));
        assert_eq!(h.run("echo -x -"), (0, "-x -\n".into()));
    }

    #[test]
    fn clear_sends_the_clear_screen_sequence() {
        let mut h = Harness::new();
        assert_eq!(h.run("clear"), (0, "\x1b[H\x1b[2J".into()));
    }

    #[test]
    fn help_lists_every_command_with_its_description() {
        let mut h = Harness::new();
        let (status, text) = h.run("help");
        assert_eq!(status, 0);
        // The programs first, then the built-ins, so that on the NUC's 33
        // rows the built-ins and the syntax stay on the screen.
        let (programs, builtins) = text
            .strip_prefix("Programs in /bin:\n")
            .and_then(|t| t.split_once("Shell built-ins:\n"))
            .expect(&text);
        for b in super::COMMANDS {
            let line = alloc::format!("  {:<9} {}\n", b.name, b.help);
            let section = if super::BUILTINS.contains(&b.name) {
                builtins
            } else {
                programs
            };
            assert!(section.contains(&line), "{}", b.name);
        }
        assert!(builtins.starts_with("  cd        change the current directory\n"));
        // What the shell's syntax offers besides command names.
        for what in [
            "> file",
            ">> file",
            "2> file",
            "2>&1",
            "< file",
            "| ",
            "&",
            "sh FILE",
            "$1",
            "\"$@\"",
            "$?",
            "NAME=value",
            "export NAME",
            "NAME=value cmd",
            "`;`",
            "&&",
            "||",
            "`! ",
            "`if ",
            "elif",
            "else",
            "`while ",
            "`until`",
            "`for ",
            "`> `",
            "2>> file",
            ">&2",
            "`done` or `fi`",
            "`~`",
            "`cd -`",
            "${10}",
        ] {
            assert!(builtins.contains(what), "{what}");
        }
        // The built-ins and the syntax, with the prompt after them, fit on
        // the NUC's 33 rows.
        assert!(builtins.lines().count() + 2 <= 33, "{builtins}");
        // No line wider than the NUC's 120 columns, nor 72.
        assert!(text.lines().all(|l| l.chars().count() <= 72), "{text}");
    }

    #[test]
    fn uname_prints_the_system() {
        let mut h = Harness::new();
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 5 is version 0.6.0 (programmable shell gate §15 item
        // 8), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.6.0 x86_64\n".into()));
        assert_eq!(
            h.run("uname -r"),
            (1, "uname: invalid option -- 'r'\n".into())
        );
        assert_eq!(h.run("uname x"), (1, "uname: extra operand 'x'\n".into()));
    }
}
