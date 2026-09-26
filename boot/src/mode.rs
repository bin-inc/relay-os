//! Video mode selection rules (pure; see `video.rs` for the UEFI side).

pub const MAX_W: usize = 1920;
pub const MAX_H: usize = 1080;

/// Native resolution from the EDID's first detailed timing descriptor.
pub fn edid_native(edid: &[u8]) -> Option<(usize, usize)> {
    if edid.len() < 128 || edid[0..8] != [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0] {
        return None;
    }
    let d = &edid[54..72];
    if d[0] == 0 && d[1] == 0 {
        return None; // not a timing descriptor
    }
    let w = d[2] as usize | ((d[4] as usize & 0xF0) << 4);
    let h = d[5] as usize | ((d[7] as usize & 0xF0) << 4);
    (w > 0 && h > 0).then_some((w, h))
}

/// Parses `video=WxH` from the command line.
pub fn cmdline_mode(cmdline: &str) -> Option<(usize, usize)> {
    let v = cmdline
        .split_whitespace()
        .find_map(|w| w.strip_prefix("video="))?;
    let (w, h) = v.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Picks a mode index from `(index, width, height)` candidates (all already
/// linear 32-bpp), following spec §4.2.3:
///
/// 1. the mode requested with `video=WxH`, if present;
/// 2. the `current` (firmware-selected) mode if it is the monitor's native
///    mode, whatever its size — the terminal caps itself to 1920x1080;
/// 3. the native mode if it is present and fits in MAX_W x MAX_H;
/// 4. the largest mode that fits in MAX_W x MAX_H.
///
/// `None` keeps the current mode.
pub fn choose(
    modes: &[(usize, usize, usize)],
    requested: Option<(usize, usize)>,
    native: Option<(usize, usize)>,
    current: (usize, usize),
) -> Option<usize> {
    let find = |want: (usize, usize)| modes.iter().find(|m| (m.1, m.2) == want).map(|m| m.0);
    let fits = |(w, h): (usize, usize)| w <= MAX_W && h <= MAX_H;
    if let Some(i) = requested.and_then(find) {
        return Some(i);
    }
    if let Some(i) = native.filter(|&n| n == current).and_then(find) {
        return Some(i);
    }
    if let Some(i) = native.filter(|&n| fits(n)).and_then(find) {
        return Some(i);
    }
    modes
        .iter()
        .filter(|m| fits((m.1, m.2)))
        .max_by_key(|m| m.1 * m.2)
        .map(|m| m.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_with(w: usize, h: usize) -> [u8; 128] {
        let mut e = [0u8; 128];
        e[0..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        e[54] = 0x02; // non-zero pixel clock
        e[56] = (w & 0xFF) as u8;
        e[58] = ((w >> 8) << 4) as u8;
        e[59] = (h & 0xFF) as u8;
        e[61] = ((h >> 8) << 4) as u8;
        e
    }

    #[test]
    fn edid_native_resolution() {
        assert_eq!(edid_native(&edid_with(1920, 1080)), Some((1920, 1080)));
        assert_eq!(edid_native(&edid_with(3840, 2160)), Some((3840, 2160)));
        assert_eq!(edid_native(&[0u8; 128]), None);
        assert_eq!(edid_native(&[0u8; 10]), None);
    }

    #[test]
    fn video_cmdline() {
        assert_eq!(cmdline_mode("test=1 video=1280x720"), Some((1280, 720)));
        assert_eq!(cmdline_mode("video=bad"), None);
        assert_eq!(cmdline_mode(""), None);
    }

    const MODES: &[(usize, usize, usize)] = &[
        (0, 800, 600),
        (1, 1280, 720),
        (2, 1920, 1080),
        (3, 2560, 1440),
        (4, 1024, 768),
    ];

    const VGA: (usize, usize) = (1024, 768);

    #[test]
    fn prefers_requested_then_native_then_largest_fitting() {
        assert_eq!(
            choose(MODES, Some((800, 600)), Some((1920, 1080)), VGA),
            Some(0)
        );
        assert_eq!(
            choose(MODES, Some((640, 480)), Some((1280, 720)), VGA),
            Some(1)
        );
        assert_eq!(choose(MODES, None, None, VGA), Some(2));
    }

    /// Spec §4.2.3: the firmware's mode is kept when it is the monitor's
    /// native mode, whatever its size (the terminal caps itself to
    /// 1920x1080). A 16:10 monitor must not be switched to a stretched 16:9
    /// mode, as happened on the NUC's 1920x1200 ASUS PA248QV.
    #[test]
    fn keeps_the_current_mode_when_it_is_native_even_above_the_cap() {
        let modes = &[(0, 1920, 1080), (1, 1920, 1200), (2, 1024, 768)];
        assert_eq!(
            choose(modes, None, Some((1920, 1200)), (1920, 1200)),
            Some(1)
        );
        assert_eq!(
            choose(MODES, None, Some((2560, 1440)), (2560, 1440)),
            Some(3)
        );
    }

    #[test]
    fn a_native_mode_that_is_not_current_must_fit_the_cap() {
        // Firmware left the screen at 1024x768: switch to native if it fits,
        // otherwise to the largest mode that fits.
        assert_eq!(choose(MODES, None, Some((1280, 720)), VGA), Some(1));
        assert_eq!(
            choose(MODES, None, Some((2560, 1440)), VGA),
            Some(2),
            "native too big"
        );
    }

    #[test]
    fn a_requested_mode_beats_the_native_one() {
        assert_eq!(
            choose(MODES, Some((1280, 720)), Some((2560, 1440)), (2560, 1440)),
            Some(1)
        );
    }

    #[test]
    fn none_when_nothing_fits() {
        assert_eq!(choose(&[(0, 3840, 2160)], None, None, (3840, 2160)), None);
        assert_eq!(choose(&[], None, None, VGA), None);
    }
}
