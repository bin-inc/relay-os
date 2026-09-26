//! The built-in commands (spec §7.3): a table from name to function, so
//! each command is small and tested on its own.

use crate::ctx::Ctx;
use alloc::string::String;

mod basic;
mod change;
mod ls;
mod stat;
mod text;

/// One built-in command.
pub struct Builtin {
    pub name: &'static str,
    /// One line for `help`.
    pub help: &'static str,
    /// Runs the command with its arguments (without the name); returns the
    /// exit status.
    pub run: fn(&mut Ctx<'_>, &[String]) -> i32,
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
        name: "echo",
        help: "print the arguments",
        run: basic::echo,
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
        name: "pwd",
        help: "print the current directory",
        run: basic::pwd,
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
        name: "stat",
        help: "show everything about a file",
        run: stat::stat,
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

pub fn find(name: &str) -> Option<&'static Builtin> {
    COMMANDS.iter().find(|b| b.name == name)
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
}
