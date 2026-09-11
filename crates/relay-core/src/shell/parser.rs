use alloc::{string::String, vec::Vec};

const MAX_INPUT_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    NonAscii,
    InputTooLong,
    ControlByte,
    UnterminatedQuote,
    InvalidEscape,
    UnknownCommand,
    Allocation,
    Arity { command: &'static str },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Unquoted,
    SingleQuoted,
    DoubleQuoted,
}

pub fn tokenize(line: &str) -> Result<Vec<String>, ParseError> {
    let bytes = line.as_bytes();
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(ParseError::InputTooLong);
    }
    for &byte in bytes {
        if !byte.is_ascii() {
            return Err(ParseError::NonAscii);
        }
        if is_control(byte) && !is_whitespace(byte) {
            return Err(ParseError::ControlByte);
        }
    }

    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut token_started = false;
    let mut state = State::Unquoted;
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        match state {
            State::Unquoted if is_whitespace(byte) => {
                finish_token(&mut tokens, &mut token, &mut token_started)?;
            }
            State::Unquoted if byte == b'\'' => {
                state = State::SingleQuoted;
                token_started = true;
            }
            State::Unquoted if byte == b'\"' => {
                state = State::DoubleQuoted;
                token_started = true;
            }
            State::Unquoted if byte == b'\\' => {
                index += 1;
                push_escaped(bytes.get(index).copied(), &mut token)?;
                token_started = true;
            }
            State::SingleQuoted if byte == b'\'' => state = State::Unquoted,
            State::SingleQuoted => {
                push_byte(&mut token, byte)?;
            }
            State::DoubleQuoted if byte == b'\"' => state = State::Unquoted,
            State::DoubleQuoted if byte == b'\\' => {
                index += 1;
                push_escaped(bytes.get(index).copied(), &mut token)?;
            }
            State::DoubleQuoted => {
                push_byte(&mut token, byte)?;
            }
            State::Unquoted => {
                push_byte(&mut token, byte)?;
                token_started = true;
            }
        }
        index += 1;
    }

    if state != State::Unquoted {
        return Err(ParseError::UnterminatedQuote);
    }
    finish_token(&mut tokens, &mut token, &mut token_started)?;
    Ok(tokens)
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_control(byte: u8) -> bool {
    byte < b' ' || byte == 0x7f
}

fn push_escaped(byte: Option<u8>, token: &mut String) -> Result<(), ParseError> {
    let byte = byte.filter(|byte| matches!(*byte, b' '..=b'~'));
    match byte {
        Some(byte) => push_byte(token, byte),
        None => Err(ParseError::InvalidEscape),
    }
}

fn push_byte(token: &mut String, byte: u8) -> Result<(), ParseError> {
    token.try_reserve(1).map_err(|_| ParseError::Allocation)?;
    token.push(char::from(byte));
    Ok(())
}

fn finish_token(
    tokens: &mut Vec<String>,
    token: &mut String,
    token_started: &mut bool,
) -> Result<(), ParseError> {
    if *token_started {
        tokens.try_reserve(1).map_err(|_| ParseError::Allocation)?;
        tokens.push(core::mem::take(token));
        *token_started = false;
    }
    Ok(())
}
