//! Normalising the raw bytes of \EFI\RELAY\cmdline.

use boot_info::CMDLINE_MAX;

/// The longest command line (spec §4.2: at most 255 bytes); it fits
/// `BootInfo`'s `CMDLINE_MAX`-byte field.
pub const LONGEST: usize = 255;
const _: () = assert!(LONGEST <= CMDLINE_MAX);

/// Turns the file contents into the command line handed to the kernel:
/// invalid UTF-8 is cut at the first bad byte, a leading byte-order mark and
/// surrounding whitespace
/// (including CR/LF from editors) is trimmed, and the result is shortened to
/// fit `LONGEST` bytes without splitting a character.
pub fn normalize(raw: &[u8]) -> &str {
    let text = match core::str::from_utf8(raw) {
        Ok(t) => t,
        Err(e) => core::str::from_utf8(&raw[..e.valid_up_to()]).unwrap_or(""),
    };
    // A UTF-8 byte-order mark (some Windows editors write one) is not
    // whitespace, so strip it explicitly.
    let mut t = text.trim_start_matches('\u{FEFF}').trim();
    if t.len() > LONGEST {
        let mut end = LONGEST;
        while !t.is_char_boundary(end) {
            end -= 1;
        }
        t = t[..end].trim_end();
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_crlf_and_spaces() {
        assert_eq!(
            normalize(b"  test=1 video=1280x720\r\n"),
            "test=1 video=1280x720"
        );
        assert_eq!(normalize(b""), "");
        assert_eq!(normalize(b"\n\n"), "");
    }

    /// Windows editors may save a UTF-8 byte-order mark; without stripping it
    /// the first word (e.g. `test=1`) is silently not recognised.
    #[test]
    fn strips_a_leading_utf8_byte_order_mark() {
        assert_eq!(
            normalize(b"\xEF\xBB\xBFtest=1 video=1280x720\r\n"),
            "test=1 video=1280x720"
        );
        assert_eq!(normalize(b"\xEF\xBB\xBF"), "");
    }

    #[test]
    fn stops_at_invalid_utf8() {
        assert_eq!(normalize(b"test=1 \xFF panic=ud"), "test=1");
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let mut long = "a".repeat(LONGEST - 1);
        long.push('\u{e9}'); // 2 bytes, would straddle the limit
        let n = normalize(long.as_bytes());
        assert_eq!(n.len(), LONGEST - 1);
        assert!(n.chars().all(|c| c == 'a'));
    }

    /// Spec §4.2: "one line, at most 255 bytes".
    #[test]
    fn keeps_at_most_255_bytes() {
        assert_eq!(normalize("a".repeat(256).as_bytes()).len(), 255);
        assert_eq!(normalize("a".repeat(255).as_bytes()).len(), 255);
    }
}
