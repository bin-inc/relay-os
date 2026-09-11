use alloc::{string::String, vec::Vec};

use super::{ParseError, tokenize};

pub enum Command {
    Help,
    Pwd,
    Cd { path: String },
    Ls { path: Option<String> },
    Cat { path: String },
    Echo { text: Vec<String> },
    Touch { path: String },
    Write { path: String, text: String },
    Append { path: String, text: String },
    Mkdir { path: String },
    Rm { path: String },
    Rmdir { path: String },
    Sync,
    Shutdown,
}

pub fn parse_command(line: &str) -> Result<Option<Command>, ParseError> {
    let mut tokens = tokenize(line)?.into_iter();
    let Some(name) = tokens.next() else {
        return Ok(None);
    };

    let command = match name.as_str() {
        "help" => {
            require_no_arguments(&mut tokens, "help")?;
            Command::Help
        }
        "pwd" => {
            require_no_arguments(&mut tokens, "pwd")?;
            Command::Pwd
        }
        "cd" => Command::Cd {
            path: require_one_argument(&mut tokens, "cd")?,
        },
        "ls" => Command::Ls {
            path: optional_argument(&mut tokens, "ls")?,
        },
        "cat" => Command::Cat {
            path: require_one_argument(&mut tokens, "cat")?,
        },
        "echo" => Command::Echo {
            text: collect_arguments(&mut tokens)?,
        },
        "touch" => Command::Touch {
            path: require_one_argument(&mut tokens, "touch")?,
        },
        "write" => {
            let (path, text) = require_argument_then_join(&mut tokens, "write")?;
            Command::Write { path, text }
        }
        "append" => {
            let (path, text) = require_argument_then_join(&mut tokens, "append")?;
            Command::Append { path, text }
        }
        "mkdir" => Command::Mkdir {
            path: require_one_argument(&mut tokens, "mkdir")?,
        },
        "rm" => Command::Rm {
            path: require_one_argument(&mut tokens, "rm")?,
        },
        "rmdir" => Command::Rmdir {
            path: require_one_argument(&mut tokens, "rmdir")?,
        },
        "sync" => {
            require_no_arguments(&mut tokens, "sync")?;
            Command::Sync
        }
        "shutdown" => {
            require_no_arguments(&mut tokens, "shutdown")?;
            Command::Shutdown
        }
        _ => return Err(ParseError::UnknownCommand),
    };
    Ok(Some(command))
}

fn require_one_argument(
    arguments: &mut alloc::vec::IntoIter<String>,
    command: &'static str,
) -> Result<String, ParseError> {
    let argument = arguments.next().ok_or(ParseError::Arity { command })?;
    require_no_arguments(arguments, command)?;
    Ok(argument)
}

fn optional_argument(
    arguments: &mut alloc::vec::IntoIter<String>,
    command: &'static str,
) -> Result<Option<String>, ParseError> {
    let argument = arguments.next();
    require_no_arguments(arguments, command)?;
    Ok(argument)
}

fn require_no_arguments(
    arguments: &mut alloc::vec::IntoIter<String>,
    command: &'static str,
) -> Result<(), ParseError> {
    if arguments.next().is_some() {
        Err(ParseError::Arity { command })
    } else {
        Ok(())
    }
}

fn collect_arguments(
    arguments: &mut alloc::vec::IntoIter<String>,
) -> Result<Vec<String>, ParseError> {
    let mut collected = Vec::new();
    for argument in arguments {
        collected
            .try_reserve(1)
            .map_err(|_| ParseError::Allocation)?;
        collected.push(argument);
    }
    Ok(collected)
}

fn require_argument_then_join(
    arguments: &mut alloc::vec::IntoIter<String>,
    command: &'static str,
) -> Result<(String, String), ParseError> {
    let path = arguments.next().ok_or(ParseError::Arity { command })?;
    let text = join_arguments(arguments)?;
    Ok((path, text))
}

fn join_arguments(arguments: &mut alloc::vec::IntoIter<String>) -> Result<String, ParseError> {
    let remaining = arguments.as_slice();
    let length =
        remaining.iter().map(String::len).sum::<usize>() + remaining.len().saturating_sub(1);
    let mut text = String::new();
    text.try_reserve_exact(length)
        .map_err(|_| ParseError::Allocation)?;
    for (index, argument) in arguments.enumerate() {
        if index != 0 {
            text.push(' ');
        }
        text.push_str(&argument);
    }
    Ok(text)
}
