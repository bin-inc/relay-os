//! Kernel log: a ring buffer of everything printed to the console. `dmesg`
//! prints it, and the panic screen shows its tail.

use spin::Mutex;

pub const KLOG_SIZE: usize = 64 * 1024;

pub struct Ring<const N: usize> {
    buf: [u8; N],
    /// Index of the oldest byte.
    start: usize,
    len: usize,
}

impl<const N: usize> Ring<N> {
    pub const fn new() -> Self {
        Ring {
            buf: [0; N],
            start: 0,
            len: 0,
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            let end = (self.start + self.len) % N;
            self.buf[end] = b;
            if self.len == N {
                self.start = (self.start + 1) % N;
            } else {
                self.len += 1;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn byte(&self, i: usize) -> u8 {
        self.buf[(self.start + i) % N]
    }

    /// Copies the newest `n` complete-or-partial lines into `out` (oldest
    /// first) and returns the number of bytes written. If they do not fit,
    /// the oldest bytes are dropped.
    pub fn tail_lines(&self, n: usize, out: &mut [u8]) -> usize {
        // Walk back from the end counting newlines; a trailing newline does
        // not start a new line.
        let mut from = self.len;
        let mut lines = 0;
        let mut i = self.len;
        while i > 0 {
            i -= 1;
            if self.byte(i) == b'\n' && i + 1 != self.len {
                lines += 1;
                if lines == n {
                    break;
                }
            }
            from = i;
        }
        let count = (self.len - from).min(out.len());
        let first = self.len - count;
        for (k, slot) in out[..count].iter_mut().enumerate() {
            *slot = self.byte(first + k);
        }
        count
    }
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}

pub static KLOG: Mutex<Ring<KLOG_SIZE>> = Mutex::new(Ring::new());

/// Removes `ESC [ ... final` sequences in place; returns the new length.
/// Used when replaying log text on a differently coloured screen.
pub fn strip_ansi_in_place(buf: &mut [u8]) -> usize {
    let (mut r, mut w) = (0, 0);
    while r < buf.len() {
        if buf[r] == 0x1B && buf.get(r + 1) == Some(&b'[') {
            r += 2;
            while r < buf.len() && !(0x40..=0x7E).contains(&buf[r]) {
                r += 1;
            }
            r += 1; // the final byte
            continue;
        }
        buf[w] = buf[r];
        w += 1;
        r += 1;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail<const N: usize>(r: &Ring<N>, n: usize) -> String {
        let mut out = [0u8; 256];
        let k = r.tail_lines(n, &mut out);
        String::from_utf8_lossy(&out[..k]).into_owned()
    }

    #[test]
    fn keeps_newest_bytes_when_full() {
        let mut r: Ring<4> = Ring::new();
        r.write(b"abcdef");
        assert_eq!(r.len(), 4);
        assert_eq!(tail(&r, 10), "cdef");
    }

    #[test]
    fn tail_returns_last_n_lines() {
        let mut r: Ring<64> = Ring::new();
        r.write(b"one\ntwo\nthree\nfour\n");
        assert_eq!(tail(&r, 2), "three\nfour\n");
        assert_eq!(tail(&r, 10), "one\ntwo\nthree\nfour\n");
        r.write(b"partial");
        assert_eq!(tail(&r, 2), "four\npartial");
    }

    #[test]
    fn tail_truncates_to_output_buffer() {
        let mut r: Ring<64> = Ring::new();
        r.write(b"0123456789\n");
        let mut out = [0u8; 4];
        let k = r.tail_lines(1, &mut out);
        assert_eq!(&out[..k], b"789\n");
    }

    #[test]
    fn strips_colour_sequences() {
        let mut b = *b"[\x1b[32m ok \x1b[0m] x\x1b[";
        let n = strip_ansi_in_place(&mut b);
        assert_eq!(&b[..n], b"[ ok ] x");
    }

    #[test]
    fn empty_ring() {
        let r: Ring<8> = Ring::new();
        assert!(r.is_empty());
        assert_eq!(tail(&r, 3), "");
    }
}
