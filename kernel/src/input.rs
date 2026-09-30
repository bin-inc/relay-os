//! Console input (spec §7.2): bytes from every source (the USB keyboard,
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it. Key
//! events become the bytes a terminal sends, so the shell cannot tell the
//! keyboard from a serial line.
//!
//! In line mode (user-space gate §6.5) every key goes to the line
//! discipline as it is typed, which echoes it at once; a program reads
//! the lines. Back in raw mode what was typed and not read is raw input
//! again, as a Linux terminal's is.

use crate::line::LineDiscipline;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use usb::hid::{Key, KeyEvent};

/// Ctrl-C.
pub const INTERRUPT: u8 = 0x03;
/// Bytes typed ahead beyond this are dropped (a stuck key during a long
/// command must not fill the heap).
pub const QUEUE_MAX: usize = 4096;

/// The longest escape sequence kept while it arrives over serial; longer
/// ones are garbage and dropped.
const SEQUENCE_MAX: usize = 8;

/// How long the rest of an escape sequence may take to arrive over serial.
/// A terminal sends a sequence at once, so an `ESC` with nothing after it
/// for this long is the Escape key (milestone 1's deferred finding).
pub const ESC_TIMEOUT_MS: u64 = 50;

pub struct InputQueue {
    /// Raw input.
    bytes: VecDeque<u8>,
    /// How many of `bytes`, from the first, the line discipline echoed
    /// already (typed in line mode and handed back): they go through it
    /// again without being echoed again.
    echoed: usize,
    /// An escape sequence arriving over serial, until it is complete.
    sequence: Vec<u8>,
    /// When its `ESC` came, in milliseconds.
    sequence_since: u64,
    /// Line mode: keys go to `line`.
    line_mode: bool,
    line: LineDiscipline,
    /// What the line discipline echoed, for the screen.
    echo: Vec<u8>,
    /// A Ctrl-C typed in line mode, for the foreground group.
    interrupted: bool,
}

impl InputQueue {
    pub const fn new() -> InputQueue {
        InputQueue {
            bytes: VecDeque::new(),
            echoed: 0,
            sequence: Vec::new(),
            sequence_since: 0,
            line_mode: false,
            line: LineDiscipline::new(),
            echo: Vec::new(),
            interrupted: false,
        }
    }

    /// Line mode (`true`) or raw mode; the previous one. Going to line mode
    /// hands what was typed ahead to the line discipline (a Ctrl-C in it is
    /// for the new foreground group); going back hands what was typed and
    /// not read back as raw input, and a Ctrl-C nobody took as a raw one.
    pub fn set_line_mode(&mut self, line: bool) -> bool {
        let was = core::mem::replace(&mut self.line_mode, line);
        if line && !was {
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            let quiet = core::mem::take(&mut self.echoed).min(ahead.len());
            let mut shown = Vec::new();
            for key in keys(&ahead[..quiet]) {
                self.interrupted |= self.line.input(key, &mut shown);
            }
            self.push(&ahead[quiet..]);
        } else if !line && was {
            let typed = self.line.take_all();
            if core::mem::take(&mut self.interrupted) {
                self.push(&[INTERRUPT]);
            }
            self.push(&typed);
            self.echoed = self.bytes.len();
        }
        was
    }

    pub fn is_line_mode(&self) -> bool {
        self.line_mode
    }

    /// Whether a Ctrl-C was typed in line mode since the last call.
    pub fn take_line_interrupt(&mut self) -> bool {
        core::mem::take(&mut self.interrupted)
    }

    /// What the line discipline echoed since the last call.
    pub fn take_echo(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.echo)
    }

    /// In line mode, the next line (`LineDiscipline::read`); in raw mode as
    /// many bytes as there are, up to `buf`'s length. `None` if nothing
    /// waits.
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.line_mode {
            return self.line.read(buf);
        }
        if buf.is_empty() {
            return Some(0);
        }
        if self.bytes.is_empty() {
            return None;
        }
        let n = buf.len().min(self.bytes.len());
        for (b, x) in buf.iter_mut().zip(self.bytes.drain(..n)) {
            *b = x;
        }
        self.echoed = self.echoed.saturating_sub(n);
        Some(n)
    }

    /// Adds one byte from COM1 that came at `now` (milliseconds). An escape
    /// sequence (`ESC` and one byte, or `ESC [` up to its final byte) waits
    /// until it is complete and then goes in whole or not at all, like a
    /// key's; Ctrl-C ends it, and so does `expire`.
    pub fn push_serial(&mut self, b: u8, now: u64) {
        if b == INTERRUPT || self.sequence.is_empty() && b != 0x1B {
            self.push(&[b]);
        } else {
            if self.sequence.is_empty() {
                self.sequence_since = now;
            }
            self.sequence.push(b);
            let s = &self.sequence;
            let done = s.len() == 2 && s[1] != b'[' || s.len() > 2 && (0x40..=0x7E).contains(&b);
            if done {
                let seq = core::mem::take(&mut self.sequence);
                if self.bytes.len() + seq.len() <= QUEUE_MAX {
                    self.push(&seq);
                }
            } else if s.len() >= SEQUENCE_MAX {
                self.sequence.clear();
            }
        }
    }

    /// Ends an escape sequence arriving over serial whose rest has not come
    /// `ESC_TIMEOUT_MS` after its `ESC`: an `ESC` alone is the Escape key
    /// and goes in; the start of a longer one is garbage and is dropped.
    pub fn expire(&mut self, now: u64) {
        if self.sequence.is_empty() || now.saturating_sub(self.sequence_since) < ESC_TIMEOUT_MS {
            return;
        }
        let seq = core::mem::take(&mut self.sequence);
        if seq == [0x1B] {
            self.push(&seq);
        }
    }

    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would, and a
    /// half-arrived serial sequence, wherever the Ctrl-C came from.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.line_mode {
            if bytes.contains(&INTERRUPT) {
                self.sequence.clear();
            }
            for key in keys(bytes) {
                self.interrupted |= self.line.input(key, &mut self.echo);
            }
            return;
        }
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.clear();
                self.echoed = 0;
                self.sequence.clear();
                &bytes[i..]
            }
            None => bytes,
        };
        let room = QUEUE_MAX - self.bytes.len();
        self.bytes.extend(&bytes[..bytes.len().min(room)]);
    }

    /// The oldest byte.
    pub fn pop(&mut self) -> Option<u8> {
        let b = self.bytes.pop_front()?;
        self.echoed = self.echoed.saturating_sub(1);
        Some(b)
    }

    /// Whether nothing waits to be read: no raw input, and no line.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty() && !self.line.has_line()
    }

    /// Whether a Ctrl-C is waiting. If one is, it and everything typed
    /// before it are dropped, as a terminal flushes its input on an
    /// interrupt; what was typed after it stays.
    pub fn take_interrupt(&mut self) -> bool {
        match self.bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.drain(..=i);
                self.echoed = self.echoed.saturating_sub(i + 1);
                true
            }
            None => false,
        }
    }
}

/// `bytes` cut into keys: an escape sequence (`ESC` and one byte, or
/// `ESC [` up to its final byte) is one, every other byte is one.
fn keys(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = bytes;
    core::iter::from_fn(move || {
        let n = match rest {
            [] => return None,
            [0x1B, b'[', tail @ ..] => {
                2 + tail
                    .iter()
                    .position(|b| (0x40..=0x7E).contains(b))
                    .map_or(tail.len(), |i| i + 1)
            }
            [0x1B, _, ..] => 2,
            _ => 1,
        };
        let (key, after) = rest.split_at(n);
        rest = after;
        Some(key)
    })
}

/// The escape sequence an editing key sends, as a Linux terminal does.
fn sequence(key: Key) -> Option<&'static [u8]> {
    Some(match key {
        Key::Up => b"\x1b[A",
        Key::Down => b"\x1b[B",
        Key::Right => b"\x1b[C",
        Key::Left => b"\x1b[D",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::Insert => b"\x1b[2~",
        Key::Delete => b"\x1b[3~",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        _ => return None,
    })
}

/// The single byte a key press sends, if it sends one: characters as
/// ASCII, Ctrl with a letter (or `[ \\ ] ^ _ ?`) as its control code, Enter
/// as CR, Backspace as DEL.
fn byte(e: &KeyEvent) -> Option<u8> {
    match e.key {
        Key::Char(c) if e.modifiers.ctrl => Some(match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'[' | b'\\' | b']' | b'^' | b'_' => c & 0x1F,
            b'?' => 0x7F,
            // Ctrl with a digit is the digit, as on a Linux console.
            _ => c,
        }),
        Key::Char(c) => Some(c),
        Key::Enter => Some(b'\r'),
        Key::Tab => Some(b'\t'),
        Key::Backspace => Some(0x7F),
        Key::Escape => Some(0x1B),
        _ => None,
    }
}

impl InputQueue {
    /// Adds what a key press sends (spec §7.2). Releases, lock keys and
    /// function keys send nothing; Alt has no function yet (spec §6.3).
    pub fn push_key(&mut self, e: &KeyEvent) {
        if !e.pressed {
            return;
        }
        if let Some(seq) = sequence(e.key) {
            // Half a sequence would reach the shell as other keys.
            if self.bytes.len() + seq.len() <= QUEUE_MAX {
                self.push(seq);
            }
        } else if let Some(b) = byte(e) {
            self.push(&[b]);
        }
    }
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use usb::hid::Modifiers;

    fn drain(q: &mut InputQueue) -> Vec<u8> {
        core::iter::from_fn(|| q.pop()).collect()
    }

    #[test]
    fn bytes_come_out_in_order() {
        let mut q = InputQueue::new();
        assert!(q.is_empty());
        q.push(b"ls");
        q.push(b" -l\r");
        assert_eq!(drain(&mut q), b"ls -l\r");
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn typing_ahead_is_bounded() {
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 1]);
        q.push(b"bcd");
        let got = drain(&mut q);
        assert_eq!(got.len(), QUEUE_MAX);
        assert_eq!(got[QUEUE_MAX - 1], b'b');
    }

    fn press(key: Key, ctrl: bool) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers {
                ctrl,
                ..Modifiers::default()
            },
            pressed: true,
        }
    }

    fn sent(key: Key, ctrl: bool) -> Vec<u8> {
        let mut q = InputQueue::new();
        q.push_key(&press(key, ctrl));
        drain(&mut q)
    }

    #[test]
    fn characters_and_control_codes() {
        assert_eq!(sent(Key::Char(b'a'), false), b"a");
        assert_eq!(sent(Key::Char(b'~'), false), b"~");
        assert_eq!(sent(Key::Char(b' '), false), b" ");
        assert_eq!(sent(Key::Char(b'c'), true), b"\x03");
        assert_eq!(sent(Key::Char(b'C'), true), b"\x03");
        assert_eq!(sent(Key::Char(b'a'), true), b"\x01");
        assert_eq!(sent(Key::Char(b'l'), true), b"\x0c");
        assert_eq!(sent(Key::Char(b'['), true), b"\x1b");
        assert_eq!(sent(Key::Char(b'?'), true), b"\x7f");
        // Ctrl with a digit is the digit, as on a Linux console.
        assert_eq!(sent(Key::Char(b'1'), true), b"1");
        // Ctrl-@ and Ctrl-space would be NUL, which the shell has no use for.
        assert_eq!(sent(Key::Char(b'@'), true), b"@");
    }

    #[test]
    fn editing_keys_send_terminal_sequences() {
        assert_eq!(sent(Key::Enter, false), b"\r");
        assert_eq!(sent(Key::Backspace, false), b"\x7f");
        assert_eq!(sent(Key::Tab, false), b"\t");
        assert_eq!(sent(Key::Escape, false), b"\x1b");
        assert_eq!(sent(Key::Up, false), b"\x1b[A");
        assert_eq!(sent(Key::Down, false), b"\x1b[B");
        assert_eq!(sent(Key::Right, false), b"\x1b[C");
        assert_eq!(sent(Key::Left, false), b"\x1b[D");
        assert_eq!(sent(Key::Home, false), b"\x1b[H");
        assert_eq!(sent(Key::End, false), b"\x1b[F");
        assert_eq!(sent(Key::Delete, false), b"\x1b[3~");
        assert_eq!(sent(Key::PageDown, true), b"\x1b[6~");
    }

    #[test]
    fn releases_and_keys_without_a_meaning_send_nothing() {
        let mut q = InputQueue::new();
        let mut release = press(Key::Char(b'a'), false);
        release.pressed = false;
        q.push_key(&release);
        for key in [Key::CapsLock, Key::F(1), Key::NumLock, Key::Menu] {
            q.push_key(&press(key, false));
        }
        assert!(q.is_empty());
    }

    #[test]
    fn a_ctrl_c_gets_in_when_the_queue_is_full() {
        // A key held down during a long command fills the queue; Ctrl-C
        // must still stop the command.
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX]);
        q.push_key(&press(Key::Char(b'c'), true));
        assert!(q.take_interrupt());
        assert!(q.is_empty());
        q.push(&[b'a'; QUEUE_MAX]);
        q.push(b"x\x03y");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"y");
    }

    #[test]
    fn a_key_sequence_goes_in_whole_or_not_at_all() {
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 2]);
        q.push_key(&press(Key::Up, false));
        assert_eq!(drain(&mut q), [b'a'; QUEUE_MAX - 2]);
        q.push(&[b'a'; QUEUE_MAX - 3]);
        q.push_key(&press(Key::Up, false));
        assert!(drain(&mut q).ends_with(b"a\x1b[A"));
    }

    fn serial(q: &mut InputQueue, bytes: &[u8]) {
        for &b in bytes {
            q.push_serial(b, 1000);
        }
    }

    #[test]
    fn a_sequence_over_serial_goes_in_whole_or_not_at_all() {
        // COM1 bytes arrive one at a time; the queue must not keep only
        // the start of an arrow key.
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 2]);
        serial(&mut q, b"\x1b[A");
        assert_eq!(drain(&mut q), [b'a'; QUEUE_MAX - 2]);
        q.push(&[b'a'; QUEUE_MAX - 3]);
        serial(&mut q, b"\x1b[Ax");
        assert!(drain(&mut q).ends_with(b"a\x1b[A"));
        serial(&mut q, b"l\x1b[3~s\x1bx");
        assert_eq!(drain(&mut q), b"l\x1b[3~s\x1bx");
    }

    #[test]
    fn a_ctrl_c_over_serial_ends_a_sequence() {
        let mut q = InputQueue::new();
        serial(&mut q, b"ls\x1b[\x03pwd");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd");
        // A sequence that never ends is not kept for ever: its start is
        // dropped and what follows is plain input again.
        serial(&mut q, b"\x1b[11111111111111111111");
        serial(&mut q, b"ok");
        let got = drain(&mut q);
        assert!(!got.contains(&0x1b) && got.ends_with(b"1ok"), "{got:?}");
    }

    #[test]
    fn an_escape_alone_over_serial_goes_in_once_nothing_follows() {
        // Milestone 1's deferred finding: the Escape key over COM1 waited
        // for the next byte, which it then swallowed.
        let mut q = InputQueue::new();
        q.push_serial(0x1B, 1000);
        q.expire(1000 + ESC_TIMEOUT_MS - 1);
        assert!(q.is_empty(), "the rest of a sequence may still come");
        q.expire(1000 + ESC_TIMEOUT_MS);
        assert_eq!(drain(&mut q), b"\x1b");
        q.push_serial(b'x', 2000);
        assert_eq!(drain(&mut q), b"x", "what follows is plain input");
        // A sequence that arrives in time is not cut.
        serial(&mut q, b"\x1b[");
        q.expire(1000 + ESC_TIMEOUT_MS - 1);
        serial(&mut q, b"A");
        assert_eq!(drain(&mut q), b"\x1b[A");
        // The start of a longer one that never ends is dropped, counting
        // from its `ESC`.
        q.push_serial(0x1B, 3000);
        q.push_serial(b'[', 3000 + ESC_TIMEOUT_MS - 1);
        q.expire(3000 + ESC_TIMEOUT_MS);
        q.push_serial(b'y', 5000);
        assert_eq!(drain(&mut q), b"y");
        q.expire(0);
        assert!(q.is_empty(), "nothing waits");
    }

    #[test]
    fn a_ctrl_c_from_the_keyboard_ends_a_serial_sequence() {
        // Milestone 1's deferred finding: the keyboard's Ctrl-C left the
        // half sequence, and the next serial byte finished it.
        let mut q = InputQueue::new();
        serial(&mut q, b"\x1b[");
        q.push_key(&press(Key::Char(b'c'), true));
        serial(&mut q, b"A");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"A");
    }

    /// A queue in line mode.
    fn line_mode() -> InputQueue {
        let mut q = InputQueue::new();
        q.set_line_mode(true);
        q
    }

    fn read(q: &mut InputQueue, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0; n];
        q.read(&mut buf).map(|k| buf[..k].to_vec())
    }

    #[test]
    fn in_line_mode_keys_are_echoed_and_read_a_line_at_a_time() {
        let mut q = line_mode();
        assert!(q.is_line_mode());
        q.push_key(&press(Key::Char(b'h'), false));
        serial(&mut q, b"i\x7f\x7fok");
        assert_eq!(read(&mut q, 10), None, "no line yet");
        assert!(q.is_empty());
        q.push_key(&press(Key::Enter, false));
        assert_eq!(q.take_echo(), b"hi\x08 \x08\x08 \x08ok\n");
        assert!(q.take_echo().is_empty(), "taken");
        assert!(!q.is_empty());
        assert_eq!(read(&mut q, 10).unwrap(), b"ok\n");
        assert_eq!(q.pop(), None, "nothing raw");
    }

    #[test]
    fn an_escape_sequence_is_one_key_in_line_mode() {
        let mut q = line_mode();
        q.push_key(&press(Key::Up, false));
        serial(&mut q, b"a\x1b[3~b\x1bxc\r");
        q.push(b"d\x1b[1;5Ce\r");
        assert_eq!(
            read(&mut q, 10).unwrap(),
            b"abc\n",
            "Delete and Alt-x do nothing"
        );
        assert_eq!(read(&mut q, 10).unwrap(), b"de\n");
        assert_eq!(q.take_echo(), b"abc\nde\n");
    }

    #[test]
    fn a_ctrl_c_in_line_mode_is_for_the_foreground_group() {
        let mut q = line_mode();
        q.push(b"one\rtw");
        assert!(!q.take_line_interrupt());
        serial(&mut q, b"\x1b[");
        q.push_key(&press(Key::Char(b'c'), true));
        assert!(q.take_line_interrupt());
        assert!(!q.take_line_interrupt(), "taken");
        assert!(q.is_empty(), "what was typed is dropped");
        serial(&mut q, b"A\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"A\n", "and the half sequence");
        assert!(!q.take_interrupt(), "not a raw Ctrl-C");
    }

    #[test]
    fn what_was_typed_ahead_goes_to_the_line_discipline_and_back() {
        let mut q = InputQueue::new();
        q.push(b"ls\r\x1b[Ap");
        assert!(!q.set_line_mode(true), "it was raw");
        assert_eq!(q.take_echo(), b"ls\np", "echoed when it goes in");
        assert_eq!(read(&mut q, 10).unwrap(), b"ls\n");
        q.push(b"wd\rec");
        assert_eq!(q.take_echo(), b"wd\nec");
        assert!(q.set_line_mode(false));
        assert_eq!(drain(&mut q), b"pwd\nec", "unread lines and the line typed");
        assert!(!q.set_line_mode(false), "already raw: nothing moves");
        // A Ctrl-C typed ahead kills the group the console goes to.
        q.push(b"x\x03y");
        q.set_line_mode(true);
        assert!(q.take_line_interrupt());
        assert_eq!(q.take_echo(), b"y");
    }

    #[test]
    fn what_was_typed_ahead_is_echoed_once() {
        // Found by the prototype's review: text typed during a script was
        // echoed again at the start of every later command, until the
        // shell read it.
        let mut q = line_mode();
        q.push(b"abc");
        assert_eq!(q.take_echo(), b"abc");
        q.set_line_mode(false);
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"", "echoed once already");
        q.push(b"d\r");
        assert_eq!(q.take_echo(), b"d\n");
        assert_eq!(read(&mut q, 10).unwrap(), b"abcd\n");
        // What the shell took is gone; what came after is echoed.
        q.push(b"xyz");
        q.take_echo();
        q.set_line_mode(false);
        assert_eq!(q.pop(), Some(b'x'));
        q.push(b"w");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"w", "yz were echoed, w was not");
        q.push(b"\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"yzw\n");
        // A raw read takes echoed bytes too.
        q.push(b"12345");
        assert_eq!(q.take_echo(), b"\n12345");
        q.set_line_mode(false);
        assert_eq!(q.read(&mut [0; 2]), Some(2));
        q.push(b"6");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"6");
        // And so does a Ctrl-C nobody took, handed back before them.
        let mut q = line_mode();
        q.push(b"ab\x03cd");
        q.take_echo();
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        q.push(b"e");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"e");
        q.push(b"\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"cde\n");
    }

    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
        // Typed the moment a command ended: the shell's line editor and a
        // script see it, instead of nobody.
        let mut q = line_mode();
        q.push(b"abc\x03de");
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"de");
    }

    #[test]
    fn a_raw_read_takes_what_there_is() {
        let mut q = InputQueue::new();
        assert_eq!(read(&mut q, 10), None);
        q.push(b"abc\x1b[A");
        assert_eq!(read(&mut q, 2).unwrap(), b"ab");
        assert_eq!(read(&mut q, 0).unwrap(), b"");
        assert_eq!(read(&mut q, 10).unwrap(), b"c\x1b[A");
        assert!(q.is_empty());
    }

    #[test]
    fn keys_are_cut_where_a_terminal_sends_them() {
        let got: Vec<&[u8]> = keys(b"a\x1b[1;5Cb\x1bxc\x1b[").collect();
        assert_eq!(
            got,
            [&b"a"[..], b"\x1b[1;5C", b"b", b"\x1bx", b"c", b"\x1b["]
        );
        assert_eq!(keys(b"\x1b").collect::<Vec<_>>(), [&b"\x1b"[..]]);
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
        let mut q = InputQueue::new();
        q.push(b"rm x\x03 ls\x03pwd\r");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd\r");
        q.push(b"echo");
        assert!(!q.take_interrupt());
        assert_eq!(drain(&mut q), b"echo");
    }
}
