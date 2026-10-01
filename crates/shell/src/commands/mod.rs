//! The built-in commands (spec §7.3): a table from name to function, so
//! each command is small and tested on its own.

use crate::ctx::Ctx;
use alloc::string::String;

mod basic;
mod change;
mod grep;
mod ls;
mod script;
mod seq;
mod stat;
mod system;
mod text;

pub use basic::{clear, echo, r#false, pwd, r#true, uname};
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use grep::grep;
pub use ls::ls;
pub(crate) use script::Script;
pub use script::{SCRIPT_MAX, transcript_name};
pub use seq::seq;
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, reboot, sleep, sync};
pub use text::{cat, head, tail, wc};

/// A command function: runs the command with its arguments (without the
/// name); returns the exit status.
pub type Run = fn(&mut Ctx<'_>, &[String]) -> i32;

/// One built-in command.
pub struct Builtin {
    pub name: &'static str,
    /// One line for `help`.
    pub help: &'static str,
    pub run: Run,
}

/// Every command, sorted by name.
pub const COMMANDS: &[Builtin] = &[
    Builtin {
        name: "cat",
        help: "print files",
        run: text::cat,
    },
    Builtin {
        name: "cd",
        help: "change the current directory",
        run: basic::cd,
    },
    Builtin {
        name: "clear",
        help: "clear the screen",
        run: basic::clear,
    },
    Builtin {
        name: "cp",
        help: "copy files",
        run: change::cp,
    },
    Builtin {
        name: "date",
        help: "print the date and time (UTC)",
        run: system::date,
    },
    Builtin {
        name: "df",
        help: "show the free space on /",
        run: system::df,
    },
    Builtin {
        name: "dmesg",
        help: "print the kernel log",
        run: system::dmesg,
    },
    Builtin {
        name: "echo",
        help: "print the arguments",
        run: basic::echo,
    },
    Builtin {
        name: "exit",
        help: "leave the shell",
        run: basic::exit,
    },
    Builtin {
        name: "false",
        help: "do nothing, unsuccessfully",
        run: basic::r#false,
    },
    Builtin {
        name: "free",
        help: "show memory use",
        run: system::free,
    },
    Builtin {
        name: "grep",
        help: "print the lines that match a pattern",
        run: grep::grep,
    },
    Builtin {
        name: "head",
        help: "print the first lines of a file",
        run: text::head,
    },
    Builtin {
        name: "help",
        help: "list the commands",
        run: basic::help,
    },
    Builtin {
        name: "ls",
        help: "list directory contents",
        run: ls::ls,
    },
    Builtin {
        name: "mkdir",
        help: "make directories",
        run: change::mkdir,
    },
    Builtin {
        name: "mv",
        help: "move or rename files",
        run: change::mv,
    },
    Builtin {
        name: "poweroff",
        help: "sync and turn the machine off",
        run: system::poweroff,
    },
    Builtin {
        name: "pwd",
        help: "print the current directory",
        run: basic::pwd,
    },
    Builtin {
        name: "reboot",
        help: "sync and restart the machine",
        run: system::reboot,
    },
    Builtin {
        name: "rm",
        help: "remove files or directories",
        run: change::rm,
    },
    Builtin {
        name: "rmdir",
        help: "remove empty directories",
        run: change::rmdir,
    },
    Builtin {
        name: "seq",
        help: "print a sequence of whole numbers",
        run: seq::seq,
    },
    Builtin {
        name: "sh",
        help: "run the commands in a file",
        run: script::sh,
    },
    Builtin {
        name: "sleep",
        help: "wait for a number of seconds",
        run: system::sleep,
    },
    Builtin {
        name: "stat",
        help: "show everything about a file",
        run: stat::stat,
    },
    Builtin {
        name: "sync",
        help: "write cached changes to the disk",
        run: system::sync,
    },
    Builtin {
        name: "tail",
        help: "print the last lines of a file",
        run: text::tail,
    },
    Builtin {
        name: "touch",
        help: "create files or update their times",
        run: change::touch,
    },
    Builtin {
        name: "true",
        help: "do nothing, successfully",
        run: basic::r#true,
    },
    Builtin {
        name: "uname",
        help: "print the system name",
        run: basic::uname,
    },
    Builtin {
        name: "wc",
        help: "count lines, words and bytes",
        run: text::wc,
    },
];

/// The shell's own commands (user-space gate §8.3); every other one is a
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help"];

pub fn find(name: &str) -> Option<&'static Builtin> {
    COMMANDS.iter().find(|b| b.name == name)
}

/// One of the shell's own commands.
pub fn builtin(name: &str) -> Option<&'static Builtin> {
    find(name).filter(|b| BUILTINS.contains(&b.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_names_are_unique() {
        for pair in COMMANDS.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "{} before {}",
                pair[0].name,
                pair[1].name
            );
        }
        assert!(find("cd").is_some());
        assert!(find("CD").is_none());
    }

    #[test]
    fn the_shell_s_own_commands_are_cd_exit_and_help() {
        let own: alloc::vec::Vec<_> = COMMANDS
            .iter()
            .filter(|b| builtin(b.name).is_some())
            .map(|b| b.name)
            .collect();
        assert_eq!(own, ["cd", "exit", "help"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
    }
}
