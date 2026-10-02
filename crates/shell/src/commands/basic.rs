//! `cd`, `exit`, `pwd`, `echo`, `clear`, `help` and `uname`.

use super::{BUILTINS, COMMANDS};
use crate::ctx::{Ctx, getopt, outln};
use crate::parser::HOME;
use crate::shell::NAME;
use alloc::string::String;
use vfs::path;

/// The system name `uname` prints, and all of `uname -a`.
pub const UNAME: &str = "Relay";
pub const UNAME_ALL: &str = concat!("Relay relay ", env!("CARGO_PKG_VERSION"), " x86_64");

/// `cd [dir]`: no argument goes to `/root`, an empty one nowhere (as in
/// bash). `cd -` is not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let dir = match args {
        [] => HOME,
        [dir] if dir.is_empty() => return 0,
        [dir] if dir == "-" => {
            return ctx.fail(NAME, format_args!("cd: -: not supported"));
        }
        [dir] if dir.starts_with('-') => {
            ctx.fail(NAME, format_args!("cd: {dir}: invalid option"));
            return 2;
        }
        [dir] => dir.as_str(),
        _ => return ctx.fail(NAME, format_args!("cd: too many arguments")),
    };
    match ctx.vfs.chdir(dir.as_bytes()) {
        Ok(()) => 0,
        Err(e) => ctx.fail(NAME, format_args!("cd: {dir}: {e}")),
    }
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
         or into another program with `| cmd` (built-ins cannot be in a\n\
         pipeline). End a command with `&` to run it in the background, or\n\
         with `;` to run the next one after it. `a && b` runs b if a\n\
         succeeds, `a || b` if it fails, and `! a` turns a's status round.\n\
         `sh FILE ARG...` runs a script, which reads its arguments as\n\
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
         `NAME=value` sets `$NAME`. `if a; then b; fi` runs b if a succeeds,\n\
         with `elif c; then d;` and `else e;` before `fi` for other cases;\n\
         `while a; do b; done` repeats b while a succeeds, `until` while it\n\
         fails; `for x in w...; do b; done` runs b with each w as `$x`, and\n\
         `for x; do` with each argument. Each may go on across lines, at `> `."
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
        assert_eq!(
            h.run("cd -"),
            (1, "relay-sh: cd: -: not supported\n".into())
        );
        assert_eq!(
            h.run("cd -P"),
            (2, "relay-sh: cd: -P: invalid option\n".into())
        );
        assert_eq!(h.run("pwd").1, "/\n", "nothing changed the directory");
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
            "| ",
            "&",
            "sh FILE",
            "$1",
            "\"$@\"",
            "$?",
            "NAME=value",
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
        ] {
            assert!(builtins.contains(what), "{what}");
        }
        // No line wider than the NUC's 120 columns, nor 72.
        assert!(text.lines().all(|l| l.chars().count() <= 72), "{text}");
    }

    #[test]
    fn uname_prints_the_system() {
        let mut h = Harness::new();
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 3 is version 0.4.0 (user-space gate §16 item 11), from
        // Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.4.0 x86_64\n".into()));
        assert_eq!(
            h.run("uname -r"),
            (1, "uname: invalid option -- 'r'\n".into())
        );
        assert_eq!(h.run("uname x"), (1, "uname: extra operand 'x'\n".into()));
    }
}
