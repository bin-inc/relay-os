//! `cd`, `pwd`, `echo`, `clear`, `help` and `uname`.

use super::COMMANDS;
use crate::ctx::{Ctx, getopt, outln};
use crate::parser::HOME;
use crate::shell::NAME;
use alloc::string::String;
use vfs::path;

/// The system name `uname` prints, and all of `uname -a`.
pub const UNAME: &str = "Relay";
pub const UNAME_ALL: &str = concat!("Relay relay ", env!("CARGO_PKG_VERSION"), " x86_64");

/// `cd [dir]`: no argument goes to `/root`. `cd -` is not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let dir = match args {
        [] => HOME,
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

/// `pwd`: arguments are ignored, as in bash.
pub fn pwd(ctx: &mut Ctx<'_>, _: &[String]) -> i32 {
    let cwd = path::display(&ctx.vfs.cwd());
    outln!(ctx, "{cwd}");
    0
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

pub fn help(ctx: &mut Ctx<'_>, _: &[String]) -> i32 {
    outln!(ctx, "Built-in commands:");
    for b in COMMANDS {
        outln!(ctx, "  {:<9} {}", b.name, b.help);
    }
    outln!(
        ctx,
        "Send output to a file with `> file` (replace) or `>> file` (append)."
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
        assert!(text.starts_with("Built-in commands:\n"));
        for b in super::COMMANDS {
            assert!(
                text.contains(&alloc::format!("  {:<9} {}\n", b.name, b.help)),
                "{}",
                b.name
            );
        }
        assert!(text.contains("  cd        change the current directory\n"));
    }

    #[test]
    fn uname_prints_the_system() {
        let mut h = Harness::new();
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 1 is version 0.2.0 (spec §15 item 12), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.2.0 x86_64\n".into()));
        assert_eq!(
            h.run("uname -r"),
            (1, "uname: invalid option -- 'r'\n".into())
        );
        assert_eq!(h.run("uname x"), (1, "uname: extra operand 'x'\n".into()));
    }
}
