//! A text terminal that renders into a caller-supplied pixel buffer.
//!
//! The terminal owns no memory: the caller passes the cell grid and a shadow
//! pixel buffer, so it works before the kernel has a heap and inside the
//! panic handler. Pixels are only ever *written* to the real framebuffer by
//! [`Terminal::flush`]; it is never read.
#![cfg_attr(not(test), no_std)]

pub mod ansi;
pub mod color;
pub mod font;
mod terminal;

pub use color::PixelFormat;
pub use font::{GLYPH_H, GLYPH_W};
pub use terminal::{Cell, MAX_ROWS, Terminal, geometry};

#[cfg(test)]
mod tests {
    use super::font;

    #[test]
    fn font_has_spleen_glyphs() {
        assert_eq!(font::GLYPHS[b'A' as usize][2], 0x7C);
        // Control codes fall back to '?'.
        assert_eq!(font::GLYPHS[0], font::GLYPHS[b'?' as usize]);
        // Latin-1 e-acute differs from '?'.
        assert_ne!(font::GLYPHS[0xE9], font::GLYPHS[b'?' as usize]);
    }
}
