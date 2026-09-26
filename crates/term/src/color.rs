//! The 16-colour palette and conversion to framebuffer pixels.

/// VGA-style palette as 0x00RRGGBB. Index 0-7 normal, 8-15 bright.
pub const PALETTE: [u32; 16] = [
    0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA, 0x555555,
    0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
];

pub const DEFAULT_FG: u8 = 7;
pub const DEFAULT_BG: u8 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Bytes in memory R, G, B, x  → little-endian u32 0x00BBGGRR.
    Rgb,
    /// Bytes in memory B, G, R, x  → little-endian u32 0x00RRGGBB.
    Bgr,
}

/// Converts a palette index to the u32 the framebuffer expects.
pub fn pixel(format: PixelFormat, index: u8) -> u32 {
    let rgb = PALETTE[(index & 0x0F) as usize];
    match format {
        PixelFormat::Bgr => rgb,
        PixelFormat::Rgb => {
            let r = (rgb >> 16) & 0xFF;
            let g = (rgb >> 8) & 0xFF;
            let b = rgb & 0xFF;
            (b << 16) | (g << 8) | r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgr_is_palette_value() {
        assert_eq!(pixel(PixelFormat::Bgr, 1), 0xAA0000);
    }

    #[test]
    fn rgb_swaps_red_and_blue() {
        assert_eq!(pixel(PixelFormat::Rgb, 1), 0x0000AA);
        assert_eq!(pixel(PixelFormat::Rgb, 3), 0x0055AA);
    }

    #[test]
    fn index_is_masked_to_16_colours() {
        assert_eq!(pixel(PixelFormat::Bgr, 0x1F), PALETTE[15]);
    }
}
