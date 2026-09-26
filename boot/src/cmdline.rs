//! Normalising the raw bytes of \EFI\RELAY\cmdline.

use boot_info::CMDLINE_MAX;

/// Turns the file contents into the command line handed to the kernel:
/// invalid UTF-8 is cut at the first bad byte, surrounding whitespace
/// (including CR/LF from editors) is trimmed, and the result is shortened to
/// fit `CMDLINE_MAX` bytes without splitting a character.
pub fn normalize(raw: &[u8]) -> &str {
    let text = match core::str::from_utf8(raw) {
        Ok(t) => t,
        Err(e) => core::str::from_utf8(&raw[..e.valid_up_to()]).unwrap_or(""),
    };
    let mut t = text.trim();
    if t.len() > CMDLINE_MAX {
        let mut end = CMDLINE_MAX;
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

    #[test]
    fn stops_at_invalid_utf8() {
        assert_eq!(normalize(b"test=1 \xFF panic=ud"), "test=1");
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let mut long = "a".repeat(CMDLINE_MAX - 1);
        long.push('\u{e9}'); // 2 bytes, would straddle the limit
        let n = normalize(long.as_bytes());
        assert_eq!(n.len(), CMDLINE_MAX - 1);
        assert!(n.chars().all(|c| c == 'a'));
    }
}
