//! Console input (spec §7.2): bytes from every source (the USB keyboard,
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it. Key
//! events become the bytes a terminal sends, so the shell cannot tell the
//! keyboard from a serial line.

use alloc::collections::VecDeque;
use usb::hid::{Key, KeyEvent};

/// Ctrl-C.
pub const INTERRUPT: u8 = 0x03;
/// Bytes typed ahead beyond this are dropped (a stuck key during a long
/// command must not fill the heap).
pub const QUEUE_MAX: usize = 4096;

pub struct InputQueue {
    bytes: VecDeque<u8>,
}

impl InputQueue {
    pub const fn new() -> InputQueue {
        InputQueue {
            bytes: VecDeque::new(),
        }
    }

    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would.
    pub fn push(&mut self, bytes: &[u8]) {
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.clear();
                &bytes[i..]
            }
            None => bytes,
        };
        let room = QUEUE_MAX - self.bytes.len();
        self.bytes.extend(&bytes[..bytes.len().min(room)]);
    }

    /// The oldest byte.
    pub fn pop(&mut self) -> Option<u8> {
        self.bytes.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Whether a Ctrl-C is waiting. If one is, it and everything typed
    /// before it are dropped, as a terminal flushes its input on an
    /// interrupt; what was typed after it stays.
    pub fn take_interrupt(&mut self) -> bool {
        match self.bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.drain(..=i);
                true
            }
            None => false,
        }
    }
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
