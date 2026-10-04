//! `env`, GNU coreutils 9.4's without `PATH` (programmable shell gate
//! §8.6): it prints its environment, or runs a command with a changed one.

use super::BUILTINS;
use crate::ctx::{Ctx, OptError, quote};
use crate::io::Group;
use crate::killed;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use vfs::Errno;

/// GNU's status when `env` itself fails.
const FAILED: i32 = 125;
/// GNU's status for a command that cannot run, and one not found.
const CANNOT_RUN: i32 = 126;
const NOT_FOUND: i32 = 127;

/// `env [-i] [-u NAME]... [-] [NAME=value]... [COMMAND [ARG]...]`: starts
/// from the environment its command was given, or none (`-i`, `-`),
/// removes each `-u NAME`, sets each `NAME=value` (in its place, or after
/// the rest, as GNU's `putenv` does), then prints it, one entry a line,
/// or runs COMMAND with it: `/bin/COMMAND`, or the path as given, waiting
/// for it, its status the command's. Options stop at the first word that
/// is not one, as GNU's do.
pub fn env(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let mut clear = false;
    let mut unset: Vec<&str> = Vec::new();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        i += 1;
        if arg == "--" {
            break;
        }
        if arg.starts_with("--") {
            return refuse(ctx, OptError::Unrecognized(arg.clone()));
        }
        let Some(letters) = arg.strip_prefix('-').filter(|l| !l.is_empty()) else {
            i -= 1;
            break;
        };
        for (at, c) in letters.char_indices() {
            match c {
                'i' => clear = true,
                'u' => {
                    let rest = &letters[at + 1..];
                    let name = if !rest.is_empty() {
                        rest
                    } else if let Some(next) = args.get(i) {
                        i += 1;
                        next.as_str()
                    } else {
                        return refuse(ctx, OptError::MissingValue('u'));
                    };
                    unset.push(name);
                    break;
                }
                c => return refuse(ctx, OptError::Invalid(c)),
            }
        }
    }
    if args.get(i).is_some_and(|a| a == "-") {
        clear = true;
        i += 1;
    }
    let mut entries: Vec<Vec<u8>> = if clear {
        Vec::new()
    } else {
        ctx.environment
            .split(|&b| b == 0)
            .filter(|e| !e.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    for name in unset {
        if name.is_empty() || name.contains('=') {
            let message = format!("cannot unset {}: Invalid argument", quote(name));
            ctx.fail("env", format_args!("{message}"));
            return FAILED;
        }
        entries.retain(|e| name_of(e) != name.as_bytes());
    }
    while let Some(set) = args.get(i).filter(|a| a.contains('=')) {
        i += 1;
        let set = set.as_bytes();
        match entries.iter_mut().find(|e| name_of(e) == name_of(set)) {
            Some(e) => *e = set.to_vec(),
            None => entries.push(set.to_vec()),
        }
    }
    let Some((name, args)) = args[i..].split_first() else {
        for e in entries {
            ctx.out(&e);
            ctx.out(b"\n");
        }
        return 0;
    };
    let mut block = Vec::new();
    for e in entries {
        block.extend_from_slice(&e);
        block.push(0);
    }
    run(ctx, name, args, block)
}

/// An entry's name: what comes before its first `=`.
fn name_of(entry: &[u8]) -> &[u8] {
    entry.split(|&b| b == b'=').next().unwrap_or(entry)
}

/// The first line of GNU's message for a refused option, status 125.
fn refuse(ctx: &mut Ctx<'_>, e: OptError) -> i32 {
    ctx.fail("env", format_args!("{e}"));
    FAILED
}

/// Runs `name` with `args` and the environment `block`: as a program,
/// started through the program's `Programs` and waited for; in the
/// in-process runner, which has none, the command's function, with the
/// block for its environment.
fn run(ctx: &mut Ctx<'_>, name: &str, args: &[String], block: Vec<u8>) -> i32 {
    let Some(programs) = ctx.programs.as_deref_mut() else {
        let Some(command) = super::find(name).filter(|c| !BUILTINS.contains(&c.name)) else {
            return cannot_run(ctx, name, Errno::ENOENT);
        };
        let outer = core::mem::replace(&mut ctx.environment, block);
        let status = (command.run)(ctx, args);
        ctx.environment = outer;
        return status;
    };
    let path = if name.contains('/') {
        String::from(name)
    } else {
        format!("/bin/{name}")
    };
    let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
    argv.extend(args.iter().map(|a| a.as_bytes()));
    let waited = programs
        .spawn(path.as_bytes(), &argv, &block, [0, 1, 2], Group::Shell)
        .and_then(|pid| programs.wait(pid));
    match waited {
        Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
        Ok(w) => killed::killed(&w).1,
        Err(e) => cannot_run(ctx, name, e),
    }
}

/// GNU's message for a command that could not run: 127 if it is not
/// there, 126 otherwise; a directory is `Permission denied`, as Linux's
/// `execve` says (`EACCES`, which `vfs` lacks).
fn cannot_run(ctx: &mut Ctx<'_>, name: &str, e: Errno) -> i32 {
    let why = match e {
        Errno::EISDIR => String::from("Permission denied"),
        e => e.to_string(),
    };
    ctx.fail("env", format_args!("{}: {why}", quote(name)));
    if e == Errno::ENOENT {
        NOT_FOUND
    } else {
        CANNOT_RUN
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{FakeStdout, Harness, host_tool};
    use alloc::string::String;

    /// `env` run with `args` and the environment `env`, as a program of
    /// the in-process runner: its status, output and errors.
    fn env(args: &[&str], env: &[u8]) -> (i32, String, String) {
        let mut h = Harness::new();
        let mut out = FakeStdout::file(None);
        let mut words = alloc::vec!["env"];
        words.extend_from_slice(args);
        let (status, errors) = h.program_env(&words, env, &mut out);
        (status, out.text(), errors)
    }

    /// GNU's `env` with `args`, started with the environment `env`; its
    /// errors' first line, as a refused option prints only that.
    fn gnu(args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
        let mut words = alloc::vec!["env", "-i"];
        let sets: alloc::vec::Vec<String> =
            env.iter().map(|(n, v)| alloc::format!("{n}={v}")).collect();
        words.extend(sets.iter().map(String::as_str));
        words.push("env");
        words.extend_from_slice(args);
        let (status, out, err) = host_tool(&words, &[], b"");
        let first = err
            .lines()
            .next()
            .map(|l| alloc::format!("{l}\n"))
            .unwrap_or_default();
        (status, out, first)
    }

    const BLOCK: &[u8] = b"HOME=/root\0PWD=/\0A=1\0";
    const PAIRS: &[(&str, &str)] = &[("HOME", "/root"), ("PWD", "/"), ("A", "1")];

    #[test]
    fn env_prints_and_changes_its_environment_as_gnu_s_does() {
        for args in [
            &[][..],
            &["-i"],
            &["-i", "B=2", "C=x y"],
            &["-u", "A"],
            &["-uA", "-u", "NOPE"],
            &["-u", "HOME", "HOME=/x"],
            &["HOME=/x", "B=2"],
            &["B=1", "B=2"],
            &["=x"],
            &["a/b=c"],
            &["--", "B=2"],
            &["-", "B=2"],
            &["-i", "-", "B=2"],
            &["-iu", "A"],
            &["C=été"],
        ] {
            assert_eq!(env(args, BLOCK), gnu(args, PAIRS), "{args:?}");
        }
    }

    #[test]
    fn env_refuses_what_gnu_s_refuses() {
        for args in [&["-x"][..], &["-u"], &["--x"], &["-u", "A=B"], &["-u", ""]] {
            let (status, out, err) = env(args, BLOCK);
            assert_eq!((status, out, err), gnu(args, PAIRS), "{args:?}");
            assert_eq!(status, 125, "{args:?}");
        }
    }

    #[test]
    fn env_refuses_gnu_s_other_options() {
        // GNU's `-0`, `-C`, `-S`, `-v` and long options, refused with the
        // first line of GNU's message for an option it does not know, as
        // every command here refuses an option it does not have.
        for (args, said) in [
            (&["-0"][..], "env: invalid option -- '0'\n"),
            (&["-C", "/"], "env: invalid option -- 'C'\n"),
            (&["-S", "A=1"], "env: invalid option -- 'S'\n"),
            (&["-v"], "env: invalid option -- 'v'\n"),
            (&["--unset=A"], "env: unrecognized option '--unset=A'\n"),
            (
                &["--ignore-environment"],
                "env: unrecognized option '--ignore-environment'\n",
            ),
        ] {
            assert_eq!(
                env(args, BLOCK),
                (125, String::new(), said.into()),
                "{args:?}"
            );
        }
    }

    #[test]
    fn env_prints_what_the_shell_gave_it() {
        // The exported variables, with the assignments before it, in the
        // order of export; in a pipeline too; a script it runs imports
        // what it changed.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.put("/tmp/s.sh", b"echo [$A][$HOME]\n");
        assert_eq!(
            h.lines(&[
                "export B=2",
                "C=3",
                "A=1 env",
                "env | grep -c =",
                "env -u HOME A=1 sh /tmp/s.sh",
            ])
            .1,
            "HOME=/root\nPWD=/\nB=2\nA=1\n3\n+ echo [$A][$HOME]\n[1][]\n"
        );
    }

    #[test]
    fn env_runs_a_command_with_its_environment() {
        // In the in-process runner, the command's function.
        assert_eq!(
            env(&["-i", "A=1", "env"], BLOCK),
            (0, "A=1\n".into(), String::new())
        );
        assert_eq!(
            env(&["-u", "HOME", "echo", "hi", "-u"], BLOCK),
            (0, "hi -u\n".into(), String::new())
        );
        assert_eq!(env(&["false"], BLOCK).0, 1);
        // Options stop at the first word that is not one.
        assert_eq!(env(&["A=2", "-i"], BLOCK), gnu(&["A=2", "-i"], PAIRS));
        assert_eq!(
            env(&["nope"], BLOCK),
            (
                127,
                String::new(),
                "env: 'nope': No such file or directory\n".into()
            )
        );
        // The shell's own commands are not programs.
        assert_eq!(env(&["cd"], BLOCK).0, 127);
    }
}
