//! The arguments a program was started with (spec §5.3): `count` byte
//! strings, each followed by a NUL; argument 0 is what the program that
//! started it gave (the shells give the command's name as typed, as bash
//! does).

#[derive(Clone, Copy, Debug)]
pub struct Args {
    bytes: &'static [u8],
    count: usize,
}

impl Args {
    pub fn new(bytes: &'static [u8], count: usize) -> Args {
        Args { bytes, count }
    }

    /// Every argument, argument 0 first. Bytes after the last NUL and
    /// arguments beyond `count` do not count.
    pub fn iter(&self) -> impl Iterator<Item = &'static [u8]> + use<> {
        let bytes: &'static [u8] = self.bytes;
        bytes
            .split_inclusive(|&b| b == 0)
            .filter_map(|a| a.strip_suffix(&[0]))
            .take(self.count)
    }

    pub fn get(&self, i: usize) -> Option<&'static [u8]> {
        self.iter().nth(i)
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The last part of argument 0's path, as messages name the program
    /// (`ls: cannot access …`).
    pub fn name(&self) -> &'static [u8] {
        let path = self.get(0).unwrap_or(b"");
        match path.iter().rposition(|&b| b == b'/') {
            Some(i) => &path[i + 1..],
            None => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(a: Args) -> Vec<&'static [u8]> {
        a.iter().collect()
    }

    #[test]
    fn each_argument_ends_with_a_nul() {
        let a = Args::new(b"/bin/t-args\0a\0b c\0\0", 4);
        assert_eq!(all(a), [&b"/bin/t-args"[..], b"a", b"b c", b""]);
        assert_eq!(a.len(), 4);
        assert_eq!(a.get(2), Some(&b"b c"[..]));
        assert_eq!(a.get(4), None);
    }

    #[test]
    fn the_count_and_the_last_nul_bound_the_list() {
        let bytes = b"one\0two\0three\0";
        assert_eq!(all(Args::new(bytes, 2)), [&b"one"[..], b"two"]);
        assert_eq!(all(Args::new(bytes, 9)).len(), 3, "no more than there are");
        assert_eq!(
            all(Args::new(b"one\0partial", 2)),
            [&b"one"[..]],
            "no NUL, no argument"
        );
        assert!(Args::new(b"", 0).is_empty());
        assert!(Args::new(b"x\0", 0).is_empty());
    }

    #[test]
    fn the_name_is_the_last_part_of_the_path() {
        assert_eq!(Args::new(b"/bin/t-args\0", 1).name(), b"t-args");
        assert_eq!(Args::new(b"t-args\0x\0", 2).name(), b"t-args");
        assert_eq!(Args::new(b"./a/b/\0", 1).name(), b"");
        assert_eq!(Args::new(b"", 0).name(), b"");
    }
}
