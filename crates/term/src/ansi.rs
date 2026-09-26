//! Byte-stream parser for UTF-8 text plus the small ANSI escape subset the
//! terminal understands. It turns bytes into [`Action`]s; it never panics on
//! malformed input.

pub const MAX_PARAMS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A character to draw, as a Latin-1 code (unsupported characters are `?`).
    Print(u8),
    /// One of `\n`, `\r`, `\x08`, `\t`.
    Control(u8),
    /// `ESC [ params final`. Missing parameters are 0.
    Csi {
        params: [u16; MAX_PARAMS],
        len: usize,
        private: bool,
        final_byte: u8,
    },
    /// `ESC c`: full reset.
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    /// Inside a UTF-8 sequence: code point so far and bytes still expected.
    Utf8 {
        cp: u32,
        remaining: u8,
    },
    Escape,
    Csi,
}

pub struct Parser {
    state: State,
    params: [u16; MAX_PARAMS],
    len: usize,
    private: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub const fn new() -> Self {
        Self {
            state: State::Ground,
            params: [0; MAX_PARAMS],
            len: 0,
            private: false,
        }
    }

    /// Feeds one byte; calls `out` for each resulting action (zero, one or two).
    pub fn advance(&mut self, b: u8, out: &mut impl FnMut(Action)) {
        if let Some(a) = self.step(b, out) {
            out(a);
        }
    }

    fn step(&mut self, b: u8, out: &mut impl FnMut(Action)) -> Option<Action> {
        match self.state {
            State::Ground => self.ground(b),
            State::Utf8 { cp, remaining } => {
                if b & 0xC0 != 0x80 {
                    // Truncated sequence: emit a replacement, then treat `b` fresh.
                    self.state = State::Ground;
                    out(Action::Print(b'?'));
                    return self.ground(b);
                }
                let cp = (cp << 6) | (b & 0x3F) as u32;
                if remaining > 1 {
                    self.state = State::Utf8 {
                        cp,
                        remaining: remaining - 1,
                    };
                    None
                } else {
                    self.state = State::Ground;
                    Some(Action::Print(latin1(cp)))
                }
            }
            State::Escape => match b {
                b'[' => {
                    self.params = [0; MAX_PARAMS];
                    self.len = 0;
                    self.private = false;
                    self.state = State::Csi;
                    None
                }
                b'c' => {
                    self.state = State::Ground;
                    Some(Action::Reset)
                }
                _ => {
                    self.state = State::Ground;
                    None
                }
            },
            State::Csi => self.csi(b),
        }
    }

    fn ground(&mut self, b: u8) -> Option<Action> {
        match b {
            0x1B => {
                self.state = State::Escape;
                None
            }
            b'\n' | b'\r' | 0x08 | b'\t' => Some(Action::Control(b)),
            0x20..=0x7E => Some(Action::Print(b)),
            0xC2..=0xDF => {
                self.state = State::Utf8 {
                    cp: (b & 0x1F) as u32,
                    remaining: 1,
                };
                None
            }
            0xE0..=0xEF => {
                self.state = State::Utf8 {
                    cp: (b & 0x0F) as u32,
                    remaining: 2,
                };
                None
            }
            0xF0..=0xF4 => {
                self.state = State::Utf8 {
                    cp: (b & 0x07) as u32,
                    remaining: 3,
                };
                None
            }
            0x80..=0xFF => Some(Action::Print(b'?')),
            _ => None, // other C0 controls are ignored
        }
    }

    fn csi(&mut self, b: u8) -> Option<Action> {
        match b {
            b'0'..=b'9' => {
                if self.len == 0 {
                    self.len = 1;
                }
                if let Some(p) = self.params.get_mut(self.len - 1) {
                    *p = p.saturating_mul(10).saturating_add((b - b'0') as u16);
                }
                None
            }
            b';' => {
                if self.len == 0 {
                    self.len = 1;
                }
                self.len = (self.len + 1).min(MAX_PARAMS + 1);
                None
            }
            b'?' | b'>' | b'=' => {
                self.private = true;
                None
            }
            0x40..=0x7E => {
                self.state = State::Ground;
                Some(Action::Csi {
                    params: self.params,
                    len: self.len.min(MAX_PARAMS),
                    private: self.private,
                    final_byte: b,
                })
            }
            0x1B => {
                self.state = State::Escape;
                None
            }
            _ => None, // intermediate bytes are ignored
        }
    }
}

fn latin1(cp: u32) -> u8 {
    match cp {
        0x20..=0x7E | 0xA0..=0xFF => cp as u8,
        _ => b'?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(bytes: &[u8]) -> Vec<Action> {
        let mut p = Parser::new();
        let mut v = Vec::new();
        for &b in bytes {
            p.advance(b, &mut |a| v.push(a));
        }
        v
    }

    #[test]
    fn ascii_and_controls() {
        assert_eq!(
            run(b"a\n\r\x08\t\x07"),
            vec![
                Action::Print(b'a'),
                Action::Control(b'\n'),
                Action::Control(b'\r'),
                Action::Control(0x08),
                Action::Control(b'\t'),
            ]
        );
    }

    #[test]
    fn utf8_latin1_and_unsupported() {
        assert_eq!(
            run("é€".as_bytes()),
            vec![Action::Print(0xE9), Action::Print(b'?')]
        );
    }

    #[test]
    fn truncated_utf8_becomes_question_mark_then_resumes() {
        assert_eq!(
            run(&[0xC3, b'A']),
            vec![Action::Print(b'?'), Action::Print(b'A')]
        );
        assert_eq!(
            run(&[0xC3, 0xC3, 0xA9]),
            vec![Action::Print(b'?'), Action::Print(0xE9)]
        );
        assert_eq!(run(&[0xFF]), vec![Action::Print(b'?')]);
    }

    #[test]
    fn csi_with_params() {
        let a = run(b"\x1b[12;34H");
        let mut params = [0; MAX_PARAMS];
        params[0] = 12;
        params[1] = 34;
        assert_eq!(
            a,
            vec![Action::Csi {
                params,
                len: 2,
                private: false,
                final_byte: b'H'
            }]
        );
    }

    #[test]
    fn csi_without_params_and_private() {
        assert_eq!(
            run(b"\x1b[m\x1b[?25l"),
            vec![
                Action::Csi {
                    params: [0; MAX_PARAMS],
                    len: 0,
                    private: false,
                    final_byte: b'm'
                },
                Action::Csi {
                    params: {
                        let mut p = [0; MAX_PARAMS];
                        p[0] = 25;
                        p
                    },
                    len: 1,
                    private: true,
                    final_byte: b'l'
                },
            ]
        );
    }

    #[test]
    fn huge_and_too_many_params_do_not_panic() {
        let a = run(b"\x1b[99999999;1;2;3;4;5;6;7;8;9;10m");
        match a.as_slice() {
            [Action::Csi { params, len, .. }] => {
                assert_eq!(params[0], u16::MAX);
                assert_eq!(*len, MAX_PARAMS);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reset_and_unknown_escape() {
        assert_eq!(
            run(b"\x1bc\x1bZx"),
            vec![Action::Reset, Action::Print(b'x')]
        );
    }
}
