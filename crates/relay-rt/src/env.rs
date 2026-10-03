//! The environment a program was started with (programmable shell gate
//! §8.1–§8.3): `count` entries, each followed by a NUL, which the kernel
//! put just below the arguments. Entries are `NAME=value` by convention
//! only: the kernel checks nothing else, and a program reads them as glibc
//! and Rust's `std` read `environ`.

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// An environment block.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    bytes: &'static [u8],
    count: usize,
}

impl Block {
    pub fn new(bytes: &'static [u8], count: usize) -> Block {
        Block { bytes, count }
    }

    /// Every entry as it is, in the block's order. Bytes after the last NUL
    /// and entries beyond `count` do not count.
    pub fn entries(&self) -> impl Iterator<Item = &'static [u8]> + use<> {
        let bytes: &'static [u8] = self.bytes;
        bytes
            .split_inclusive(|&b| b == 0)
            .filter_map(|e| e.strip_suffix(&[0]))
            .take(self.count)
    }

    /// Each entry's name and value, split at the first `=` after its first
    /// byte (so a name may start with `=`); an entry without one is
    /// skipped, as glibc and Rust's `std` skip it.
    pub fn vars(&self) -> impl Iterator<Item = (&'static [u8], &'static [u8])> + use<> {
        self.entries().filter_map(|e| {
            let at = 1 + e.get(1..)?.iter().position(|&b| b == b'=')?;
            Some((&e[..at], &e[at + 1..]))
        })
    }

    /// The value of the first entry named `name`.
    pub fn var(&self, name: &[u8]) -> Option<&'static [u8]> {
        self.vars().find(|&(n, _)| n == name).map(|(_, v)| v)
    }
}

/// The program's block, set before `main` runs.
static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Off Relay OS only the tests set it.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
pub(crate) fn set(b: Block) {
    PTR.store(b.bytes.as_ptr().cast_mut(), Ordering::Relaxed);
    LEN.store(b.bytes.len(), Ordering::Relaxed);
    COUNT.store(b.count, Ordering::Relaxed);
}

/// The program's environment.
pub fn program() -> Block {
    let p = PTR.load(Ordering::Relaxed);
    let bytes: &'static [u8] = if p.is_null() {
        &[]
    } else {
        // SAFETY: set from a `&'static [u8]` in `set`.
        unsafe { core::slice::from_raw_parts(p, LEN.load(Ordering::Relaxed)) }
    };
    Block::new(bytes, COUNT.load(Ordering::Relaxed))
}

/// The program's whole block, as `spawn` takes one: empty for none.
pub fn block() -> &'static [u8] {
    program().bytes
}

/// The program's environment variables, in the block's order.
pub fn vars() -> impl Iterator<Item = (&'static [u8], &'static [u8])> {
    program().vars()
}

/// The value of the program's variable `name`.
pub fn var(name: &[u8]) -> Option<&'static [u8]> {
    program().var(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(b: Block) -> Vec<(&'static [u8], &'static [u8])> {
        b.vars().collect()
    }

    #[test]
    fn each_entry_ends_with_a_nul() {
        let b = Block::new(b"HOME=/root\0A=\xc3\xa9t\xc3\xa9\0B=\0", 3);
        assert_eq!(
            b.entries().collect::<Vec<_>>(),
            [&b"HOME=/root"[..], "A=été".as_bytes(), b"B="]
        );
        assert_eq!(
            pairs(b),
            [
                (&b"HOME"[..], &b"/root"[..]),
                (b"A", "été".as_bytes()),
                (b"B", b"")
            ]
        );
        assert_eq!(b.var(b"HOME"), Some(&b"/root"[..]));
        assert_eq!(b.var(b"B"), Some(&b""[..]));
        assert_eq!(b.var(b"HOM"), None);
        assert_eq!(b.var(b"C"), None);
    }

    #[test]
    fn a_name_is_split_at_its_first_equals_sign_after_its_first_byte() {
        // As glibc and Rust's std read `environ`: an entry without `=`
        // after its first byte is skipped, and a name may start with one.
        let b = Block::new(b"A=1=2\0=x=y\0none\0\0=\0", 5);
        assert_eq!(pairs(b), [(&b"A"[..], &b"1=2"[..]), (b"=x", b"y")]);
        assert_eq!(b.entries().count(), 5, "every entry is in the block");
        assert_eq!(b.var(b"=x"), Some(&b"y"[..]));
        assert_eq!(b.var(b"none"), None);
        assert_eq!(b.var(b""), None);
        // A name given twice is the first one's, as `getenv` finds it.
        let twice = Block::new(b"A=1\0A=2\0", 2);
        assert_eq!(twice.var(b"A"), Some(&b"1"[..]));
        assert_eq!(twice.vars().count(), 2);
    }

    #[test]
    fn the_count_and_the_last_nul_bound_the_block() {
        let b = Block::new(b"A=1\0B=2\0C=3", 9);
        assert_eq!(pairs(b), [(&b"A"[..], &b"1"[..]), (b"B", b"2")]);
        assert_eq!(pairs(Block::new(b"A=1\0B=2\0", 1)).len(), 1);
        assert_eq!(pairs(Block::new(b"", 0)), []);
        assert_eq!(Block::new(b"A=1\0", 0).var(b"A"), None);
    }

    #[test]
    fn the_program_s_block_is_empty_until_set() {
        assert_eq!(block(), b"");
        assert_eq!(var(b"HOME"), None);
        set(Block::new(b"HOME=/root\0X=1\0", 2));
        assert_eq!(block(), b"HOME=/root\0X=1\0");
        assert_eq!(var(b"X"), Some(&b"1"[..]));
        assert_eq!(vars().count(), 2);
    }
}
