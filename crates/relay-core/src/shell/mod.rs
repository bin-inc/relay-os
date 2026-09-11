mod commands;
mod parser;

pub use commands::{Command, parse_command};
pub use parser::{ParseError, tokenize};

use crate::vfs::VfsError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellError {
    Parse(ParseError),
    Vfs(VfsError),
    Unavailable,
    Allocation,
}
