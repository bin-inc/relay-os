//! The US keyboard layout (spec §6.3): HID usage codes (HID Usage Tables,
//! page 7) to keys, with Shift and Caps Lock applied. Keypad keys always
//! give digits and operators (Num Lock is not tracked).

/// A key as the console sees it. Characters come with Shift and Caps Lock
/// already applied; Ctrl and Alt are left to the console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable ASCII character, or the space.
    Char(u8),
    Enter,
    Escape,
    Backspace,
    Tab,
    CapsLock,
    /// F1 to F12.
    F(u8),
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    Right,
    Left,
    Down,
    Up,
    NumLock,
    Menu,
}

/// The modifier state a key is read with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    /// Recognised, but has no function yet (spec §6.3).
    pub alt: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    /// From a boot report's modifier byte (left Ctrl, Shift, Alt, GUI in
    /// bits 0-3, the right ones in bits 4-7) and the Caps Lock state.
    pub fn from_report(byte: u8, caps_lock: bool) -> Modifiers {
        Modifiers {
            ctrl: byte & 0x11 != 0,
            shift: byte & 0x22 != 0,
            alt: byte & 0x44 != 0,
            caps_lock,
        }
    }
}

/// Unshifted and shifted characters of usages 0x1E-0x38: the digit row and
/// the punctuation keys.
const ROW: &[u8; 27] = b"1234567890\n\x1b\x08\t -=[]\\\\;'`,./";
const SHIFTED: &[u8; 27] = b"!@#$%^&*()\n\x1b\x08\t _+{}||:\"~<>?";

/// The key for `usage`, or `None` for usages this layout does not have.
pub fn key(usage: u8, m: Modifiers) -> Option<Key> {
    Some(match usage {
        0x04..=0x1D => {
            let c = b'a' + (usage - 0x04);
            Key::Char(if m.shift != m.caps_lock {
                c.to_ascii_uppercase()
            } else {
                c
            })
        }
        0x28 | 0x58 => Key::Enter,
        0x29 => Key::Escape,
        0x2A => Key::Backspace,
        0x2B => Key::Tab,
        // 0x32 is the non-US `#` key, which Linux's US map treats as `\`.
        0x1E..=0x27 | 0x2C..=0x38 => {
            let i = (usage - 0x1E) as usize;
            Key::Char(if m.shift { SHIFTED[i] } else { ROW[i] })
        }
        0x39 => Key::CapsLock,
        0x3A..=0x45 => Key::F(usage - 0x3A + 1),
        0x46 => Key::PrintScreen,
        0x47 => Key::ScrollLock,
        0x48 => Key::Pause,
        0x49 => Key::Insert,
        0x4A => Key::Home,
        0x4B => Key::PageUp,
        0x4C => Key::Delete,
        0x4D => Key::End,
        0x4E => Key::PageDown,
        0x4F => Key::Right,
        0x50 => Key::Left,
        0x51 => Key::Down,
        0x52 => Key::Up,
        0x53 => Key::NumLock,
        0x54 => Key::Char(b'/'),
        0x55 => Key::Char(b'*'),
        0x56 => Key::Char(b'-'),
        0x57 => Key::Char(b'+'),
        0x59..=0x61 => Key::Char(b'1' + (usage - 0x59)),
        0x62 => Key::Char(b'0'),
        0x63 => Key::Char(b'.'),
        // The ISO key between left Shift and Z, as Linux's US console map.
        0x64 => Key::Char(if m.shift { b'>' } else { b'<' }),
        0x65 => Key::Menu,
        0x67 => Key::Char(b'='),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        caps_lock: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..PLAIN
    };
    const CAPS: Modifiers = Modifiers {
        caps_lock: true,
        ..PLAIN
    };
    const SHIFT_CAPS: Modifiers = Modifiers {
        shift: true,
        caps_lock: true,
        ..PLAIN
    };

    fn text(usages: &[u8], m: Modifiers) -> String {
        usages
            .iter()
            .map(|&u| match key(u, m) {
                Some(Key::Char(c)) => c as char,
                other => panic!("usage {u:#x}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn letters_follow_shift_and_caps_lock() {
        assert_eq!(text(&[0x04, 0x05, 0x1D], PLAIN), "abz");
        assert_eq!(text(&[0x04, 0x05, 0x1D], SHIFT), "ABZ");
        assert_eq!(text(&[0x04, 0x05, 0x1D], CAPS), "ABZ");
        assert_eq!(text(&[0x04, 0x05, 0x1D], SHIFT_CAPS), "abz");
    }

    #[test]
    fn the_digit_row_and_punctuation_follow_shift_only() {
        let row: Vec<u8> = (0x1E..=0x27).chain(0x2C..=0x38).collect();
        assert_eq!(text(&row, PLAIN), "1234567890 -=[]\\\\;'`,./");
        assert_eq!(text(&row, SHIFT), "!@#$%^&*() _+{}||:\"~<>?");
        assert_eq!(text(&row, CAPS), "1234567890 -=[]\\\\;'`,./");
    }

    #[test]
    fn control_keys_and_the_navigation_block() {
        assert_eq!(key(0x28, PLAIN), Some(Key::Enter));
        assert_eq!(key(0x29, PLAIN), Some(Key::Escape));
        assert_eq!(key(0x2A, SHIFT), Some(Key::Backspace));
        assert_eq!(key(0x2B, PLAIN), Some(Key::Tab));
        assert_eq!(key(0x39, PLAIN), Some(Key::CapsLock));
        assert_eq!(key(0x3A, PLAIN), Some(Key::F(1)));
        assert_eq!(key(0x45, PLAIN), Some(Key::F(12)));
        assert_eq!(key(0x4A, PLAIN), Some(Key::Home));
        assert_eq!(key(0x4C, PLAIN), Some(Key::Delete));
        assert_eq!(key(0x4D, PLAIN), Some(Key::End));
        assert_eq!(
            [0x4F, 0x50, 0x51, 0x52].map(|u| key(u, PLAIN).unwrap()),
            [Key::Right, Key::Left, Key::Down, Key::Up]
        );
    }

    #[test]
    fn the_keypad_gives_digits_and_operators() {
        let pad: Vec<u8> = (0x54..=0x57).chain(0x59..=0x63).collect();
        assert_eq!(text(&pad, PLAIN), "/*-+1234567890.");
        assert_eq!(text(&pad, SHIFT), "/*-+1234567890.");
        assert_eq!(key(0x58, PLAIN), Some(Key::Enter));
    }

    #[test]
    fn usages_the_layout_lacks_are_none() {
        for u in [0x00, 0x01, 0x02, 0x03, 0x66, 0x68, 0xE0, 0xFF] {
            assert_eq!(key(u, PLAIN), None, "usage {u:#x}");
        }
    }

    #[test]
    fn modifier_bytes_merge_left_and_right() {
        assert_eq!(Modifiers::from_report(0x00, false), PLAIN);
        assert_eq!(Modifiers::from_report(0x02, false), SHIFT);
        assert_eq!(Modifiers::from_report(0x20, true), SHIFT_CAPS);
        let m = Modifiers::from_report(0x11 | 0x40, false);
        assert!(m.ctrl && m.alt && !m.shift);
    }
}
