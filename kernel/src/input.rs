//! Console input (spec §7.2): bytes from every source (the USB keyboard,
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it.

use alloc::collections::VecDeque;

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

    /// Adds input; what does not fit is dropped.
    pub fn push(&mut self, bytes: &[u8]) {
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

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
