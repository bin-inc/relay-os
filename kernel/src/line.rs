//! The console's line discipline (user-space gate §6.5): in line mode what
//! is typed is echoed and edited here, and a program reads it a line at a
//! time. Enter ends a line (the reader gets it with a `\n`), Backspace
//! removes the last character, Ctrl-D on an empty line is end of input and
//! on a line with something in it hands that over without a `\n`, as Linux
//! does. Keys that send escape sequences (arrows, Delete) and other control
//! characters do nothing. Ctrl-C drops everything typed and not read (the
//! kernel then kills the foreground group, §6.4).
//!
//! It works on keys, not bytes: the input queue hands over each key's
//! bytes together, so an escape sequence is one key.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// The bytes typed and not read that the discipline keeps, the lines that
/// wait and the one being typed together, each line's `\n` included (as
/// Linux's 4096). Beyond it, what is typed is dropped; Enter and Ctrl-C
/// always get in.
pub const LINE_MAX: usize = 4096;

/// Ctrl-C, Ctrl-D, Backspace (and Ctrl-H), Escape.
const INTERRUPT: u8 = 0x03;
const END: u8 = 0x04;
const ERASE: [u8; 2] = [0x7F, 0x08];
const ESC: u8 = 0x1B;

#[derive(Default)]
pub struct LineDiscipline {
    /// The line being typed.
    line: Vec<u8>,
    /// Lines ended with Enter or Ctrl-D, oldest first; `None` is end of
    /// input.
    ready: VecDeque<Option<Vec<u8>>>,
    /// The bytes in `ready`, an end of input counting as one.
    waiting: usize,
    /// The last key was a CR: a LF right after it is the same Enter (a
    /// terminal that sends CR LF).
    after_cr: bool,
}

impl LineDiscipline {
    pub const fn new() -> LineDiscipline {
        LineDiscipline {
            line: Vec::new(),
            ready: VecDeque::new(),
            waiting: 0,
            after_cr: false,
        }
    }

    /// One key's bytes. What the screen should show is added to `echo`.
    /// Whether it was Ctrl-C, which dropped everything typed and not read.
    pub fn input(&mut self, key: &[u8], echo: &mut Vec<u8>) -> bool {
        let after_cr = core::mem::take(&mut self.after_cr);
        let &[b] = key else {
            // An escape sequence.
            return false;
        };
        match b {
            INTERRUPT => {
                self.line.clear();
                self.ready.clear();
                self.waiting = 0;
                return true;
            }
            b'\n' if after_cr => {}
            b'\r' | b'\n' => {
                self.after_cr = b == b'\r';
                // Enter always fits: a character is taken only with room
                // for the `\n` after it.
                if self.waiting + self.line.len() < LINE_MAX {
                    self.line.push(b'\n');
                    self.hand_over();
                    echo.push(b'\n');
                }
            }
            END => {
                if self.line.is_empty() {
                    if self.waiting < LINE_MAX {
                        self.ready.push_back(None);
                        self.waiting += 1;
                    }
                } else {
                    self.hand_over();
                }
            }
            _ if ERASE.contains(&b) => {
                if self.line.is_empty() {
                    return false;
                }
                // A whole character: its continuation bytes, then its
                // first byte.
                while self.line.pop().is_some_and(|c| c & 0xC0 == 0x80) {}
                echo.extend_from_slice(b"\x08 \x08");
            }
            ESC => {}
            b'\t' | 0x20..=0x7E | 0x80.. if self.waiting + self.line.len() + 2 <= LINE_MAX => {
                self.line.push(b);
                echo.push(b);
            }
            // Other control characters, and what does not fit.
            _ => {}
        }
        false
    }

    /// Moves the line being typed to the ones that wait.
    fn hand_over(&mut self) {
        self.waiting += self.line.len();
        self.ready.push_back(Some(core::mem::take(&mut self.line)));
    }

    /// Whether a line, or end of input, waits to be read.
    pub fn has_line(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Reads from the oldest line that waits into `buf`: `None` if none
    /// does; `Some(0)` at end of input (which it takes); otherwise the
    /// bytes read, never more than one line. What does not fit in `buf`
    /// waits for the next read.
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if buf.is_empty() {
            return Some(0);
        }
        let first = self.ready.front_mut()?;
        let Some(line) = first else {
            self.ready.pop_front();
            self.waiting -= 1;
            return Some(0);
        };
        let n = buf.len().min(line.len());
        buf[..n].copy_from_slice(&line[..n]);
        if n == line.len() {
            self.ready.pop_front();
        } else {
            line.drain(..n);
        }
        self.waiting -= n;
        Some(n)
    }

    /// Everything typed and not read, as the bytes a raw reader would have
    /// had (the console going back to raw mode): the lines that wait, each
    /// with its `\n`, then the line being typed. Ends of input are dropped.
    pub fn take_all(&mut self) -> Vec<u8> {
        let mut out: Vec<u8> = self.ready.drain(..).flatten().flatten().collect();
        out.append(&mut self.line);
        self.waiting = 0;
        self.after_cr = false;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `bytes`, one key each; the echo.
    fn typed(d: &mut LineDiscipline, bytes: &[u8]) -> Vec<u8> {
        let mut echo = Vec::new();
        for &b in bytes {
            d.input(&[b], &mut echo);
        }
        echo
    }

    /// Reads with a buffer of `n` bytes.
    fn read(d: &mut LineDiscipline, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0; n];
        d.read(&mut buf).map(|k| buf[..k].to_vec())
    }

    #[test]
    fn a_line_is_echoed_and_read_with_its_newline_once_enter_ends_it() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"ls -l"), b"ls -l");
        assert!(!d.has_line());
        assert_eq!(read(&mut d, 100), None, "no line yet: the reader waits");
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        assert!(d.has_line());
        assert_eq!(read(&mut d, 100).unwrap(), b"ls -l\n");
        assert_eq!(read(&mut d, 100), None);
    }

    #[test]
    fn a_read_returns_at_most_one_line_and_keeps_the_rest() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\rtwo\r");
        assert_eq!(read(&mut d, 100).unwrap(), b"one\n");
        assert_eq!(read(&mut d, 2).unwrap(), b"tw");
        assert_eq!(read(&mut d, 2).unwrap(), b"o\n");
        assert_eq!(read(&mut d, 0).unwrap(), b"", "an empty read takes nothing");
        assert_eq!(read(&mut d, 1), None);
        // An empty line is a line.
        typed(&mut d, b"\r");
        assert_eq!(read(&mut d, 10).unwrap(), b"\n");
    }

    #[test]
    fn a_terminal_s_cr_lf_is_one_enter_and_a_lone_lf_is_one_too() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"a\r\nb\n\n"), b"a\nb\n\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"a\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"b\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"\n");
        assert_eq!(read(&mut d, 10), None);
    }

    #[test]
    fn backspace_removes_the_last_character_whole() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"cat\x7f\x7fp"), b"cat\x08 \x08\x08 \x08p");
        // A character of two bytes goes with one Backspace; Ctrl-H erases
        // too.
        assert_eq!(typed(&mut d, "é".as_bytes()), "é".as_bytes());
        assert_eq!(typed(&mut d, b"\x08"), b"\x08 \x08");
        assert_eq!(
            typed(&mut d, b"\x7f\x7f\x7f"),
            b"\x08 \x08\x08 \x08",
            "nothing left to erase"
        );
        typed(&mut d, b"x\r");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
    }

    #[test]
    fn ctrl_d_ends_input_on_an_empty_line_and_hands_over_a_started_one() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"\x04"), b"", "not echoed");
        assert_eq!(read(&mut d, 10).unwrap(), b"", "end of input");
        assert_eq!(read(&mut d, 10), None, "taken by that read");
        typed(&mut d, b"abc\x04");
        assert_eq!(read(&mut d, 10).unwrap(), b"abc", "without a newline");
        assert_eq!(read(&mut d, 10), None);
        typed(&mut d, b"x\r\x04\x04");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"");
        assert_eq!(read(&mut d, 10).unwrap(), b"");
    }

    #[test]
    fn escape_sequences_and_other_control_keys_do_nothing() {
        let mut d = LineDiscipline::new();
        let mut echo = Vec::new();
        for key in [
            &b"\x1b[A"[..],
            b"\x1b[3~",
            b"\x1b",
            b"\x01",
            b"\x0c",
            b"\x00",
        ] {
            assert!(!d.input(key, &mut echo));
        }
        assert!(echo.is_empty());
        assert_eq!(typed(&mut d, b"a\tb\r"), b"a\tb\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"a\tb\n");
    }

    #[test]
    fn ctrl_c_drops_everything_typed_and_not_read() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\rtwo\x04thr");
        let mut echo = Vec::new();
        assert!(d.input(b"\x03", &mut echo));
        assert!(echo.is_empty(), "the shell says ^C");
        assert!(!d.has_line());
        assert_eq!(typed(&mut d, b"x\r"), b"x\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
    }

    #[test]
    fn what_waits_is_bounded_and_enter_still_gets_in() {
        let mut d = LineDiscipline::new();
        let long = vec![b'a'; LINE_MAX + 10];
        let echo = typed(&mut d, &long);
        assert_eq!(echo.len(), LINE_MAX - 1, "room is kept for the newline");
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        assert_eq!(read(&mut d, 2 * LINE_MAX).unwrap().len(), LINE_MAX);
        // Lines that wait take room from the next one.
        typed(&mut d, &[b'b'; 2000]);
        typed(&mut d, b"\r");
        assert_eq!(typed(&mut d, &[b'c'; 3000]).len(), LINE_MAX - 2001 - 1);
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        // Full: an Enter no longer fits, nor an end of input.
        assert_eq!(typed(&mut d, b"\r\x04"), b"");
        assert_eq!(read(&mut d, LINE_MAX).unwrap().len(), 2001);
        assert_eq!(read(&mut d, LINE_MAX).unwrap().len(), LINE_MAX - 2001);
        assert_eq!(read(&mut d, 10), None);
        // Everything read: the whole room again.
        assert_eq!(typed(&mut d, &long).len(), LINE_MAX - 1);
    }

    #[test]
    fn going_back_to_raw_hands_over_what_was_typed() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\r\x04two\rthr");
        assert_eq!(d.take_all(), b"one\ntwo\nthr");
        assert!(!d.has_line());
        assert_eq!(d.take_all(), b"");
        // The room is back.
        assert_eq!(typed(&mut d, &[b'a'; LINE_MAX]).len(), LINE_MAX - 1);
    }
}
