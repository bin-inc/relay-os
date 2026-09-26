//! The terminal state machine: a cell grid, cursor and colours, rendered
//! glyph by glyph into the shadow buffer.

use crate::ansi::{Action, Parser};
use crate::color::{self, DEFAULT_BG, DEFAULT_FG, PixelFormat};
use crate::font::{self, GLYPH_H, GLYPH_W};

/// Upper bound on text rows (keeps the dirty-row table a fixed array).
pub const MAX_ROWS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Latin-1 character code.
    pub ch: u8,
    /// Palette index 0-15 (bold already folded in).
    pub fg: u8,
    pub bg: u8,
}

impl Cell {
    pub const BLANK: Cell = Cell {
        ch: b' ',
        fg: DEFAULT_FG,
        bg: DEFAULT_BG,
    };
}

/// Grid geometry for a pixel area: `(cols, rows, scale)`.
/// Glyphs are drawn at 2x when the area is at least 1600 px wide.
pub fn geometry(width: usize, height: usize) -> (usize, usize, usize) {
    let scale = if width >= 1600 { 2 } else { 1 };
    let cols = width / (GLYPH_W * scale);
    let rows = (height / (GLYPH_H * scale)).min(MAX_ROWS);
    (cols, rows, scale)
}

pub struct Terminal<'a> {
    cells: &'a mut [Cell],
    shadow: &'a mut [u32],
    format: PixelFormat,
    width: usize,
    height: usize,
    scale: usize,
    cols: usize,
    rows: usize,
    cx: usize,
    cy: usize,
    fg: u8,
    bg: u8,
    bold: bool,
    drawn_cursor: Option<(usize, usize)>,
    parser: Parser,
    dirty: [bool; MAX_ROWS],
    flush_all: bool,
}

impl<'a> Terminal<'a> {
    /// Creates a terminal covering a `width` x `height` pixel area.
    ///
    /// Panics if the area is smaller than one cell or the buffers are too
    /// small (`cells` needs `cols * rows`, `shadow` needs `width * height`).
    pub fn new(
        width: usize,
        height: usize,
        format: PixelFormat,
        cells: &'a mut [Cell],
        shadow: &'a mut [u32],
    ) -> Self {
        let (cols, rows, scale) = geometry(width, height);
        assert!(cols > 0 && rows > 0, "framebuffer smaller than one cell");
        assert!(cells.len() >= cols * rows, "cell buffer too small");
        assert!(shadow.len() >= width * height, "shadow buffer too small");
        let mut t = Terminal {
            cells,
            shadow,
            format,
            width,
            height,
            scale,
            cols,
            rows,
            cx: 0,
            cy: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
            drawn_cursor: None,
            parser: Parser::new(),
            dirty: [false; MAX_ROWS],
            flush_all: true,
        };
        t.reset();
        t
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Cursor position `(col, row)`. `col` may equal `cols()` right after
    /// the last column was written (the wrap happens on the next character).
    pub fn cursor(&self) -> (usize, usize) {
        (self.cx, self.cy)
    }

    pub fn cell(&self, col: usize, row: usize) -> Cell {
        self.cells[row * self.cols + col]
    }

    /// Reads back one pixel of the shadow buffer (for tests and diagnostics).
    pub fn shadow_pixel(&self, x: usize, y: usize) -> u32 {
        self.shadow[y * self.width + x]
    }

    /// Interprets `bytes` (UTF-8 text with ANSI escapes) and updates the
    /// shadow buffer. Call [`flush`](Self::flush) to make it visible.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.erase_cursor();
        let mut parser = core::mem::take(&mut self.parser);
        for &b in bytes {
            parser.advance(b, &mut |a| self.apply(a));
        }
        self.parser = parser;
        self.draw_cursor();
    }

    /// Copies dirty rows of the shadow buffer into `fb`, whose rows are
    /// `stride` pixels apart.
    pub fn flush(&mut self, fb: &mut [u32], stride: usize) {
        assert!(stride >= self.width);
        assert!(
            fb.len() >= (self.height - 1) * stride + self.width,
            "framebuffer too small"
        );
        let ch = GLYPH_H * self.scale;
        if self.flush_all {
            for y in 0..self.height {
                self.copy_line(fb, stride, y);
            }
            self.flush_all = false;
            self.dirty = [false; MAX_ROWS];
            return;
        }
        for row in 0..self.rows {
            if core::mem::take(&mut self.dirty[row]) {
                for y in row * ch..(row + 1) * ch {
                    self.copy_line(fb, stride, y);
                }
            }
        }
    }

    fn copy_line(&self, fb: &mut [u32], stride: usize, y: usize) {
        let src = &self.shadow[y * self.width..(y + 1) * self.width];
        fb[y * stride..y * stride + self.width].copy_from_slice(src);
    }

    fn reset(&mut self) {
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.bold = false;
        self.cx = 0;
        self.cy = 0;
        self.drawn_cursor = None;
        let bg = color::pixel(self.format, DEFAULT_BG);
        self.shadow[..self.width * self.height].fill(bg);
        self.cells[..self.cols * self.rows].fill(Cell::BLANK);
        self.flush_all = true;
        self.draw_cursor();
    }

    fn blank(&self) -> Cell {
        Cell {
            ch: b' ',
            fg: self.effective_fg(),
            bg: self.bg,
        }
    }

    fn effective_fg(&self) -> u8 {
        if self.bold && self.fg < 8 {
            self.fg + 8
        } else {
            self.fg
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Print(ch) => {
                if self.cx >= self.cols {
                    self.cx = 0;
                    self.line_feed();
                }
                let cell = Cell {
                    ch,
                    fg: self.effective_fg(),
                    bg: self.bg,
                };
                self.put(self.cx, self.cy, cell);
                self.cx += 1;
            }
            Action::Control(b'\n') => {
                self.cx = 0;
                self.line_feed();
            }
            Action::Control(b'\r') => self.cx = 0,
            Action::Control(0x08) => self.cx = self.cx.min(self.cols - 1).saturating_sub(1),
            Action::Control(b'\t') => {
                if self.cx < self.cols {
                    self.cx = ((self.cx / 8 + 1) * 8).min(self.cols - 1);
                }
            }
            Action::Control(_) => {}
            Action::Csi {
                params,
                len,
                private,
                final_byte,
            } => {
                if !private {
                    self.csi(&params[..len], final_byte);
                }
            }
            Action::Reset => self.reset(),
        }
    }

    fn csi(&mut self, p: &[u16], final_byte: u8) {
        let arg = |i: usize, default: usize| match p.get(i) {
            Some(&v) if v != 0 => v as usize,
            _ => default,
        };
        let (cols, rows) = (self.cols, self.rows);
        match final_byte {
            b'm' => {
                if p.is_empty() {
                    self.sgr(0);
                }
                for &v in p {
                    self.sgr(v);
                }
            }
            b'J' => match arg(0, 0) {
                0 => {
                    self.clear_span(self.cy, self.cx.min(cols), cols);
                    for r in self.cy + 1..rows {
                        self.clear_span(r, 0, cols);
                    }
                }
                1 => {
                    for r in 0..self.cy {
                        self.clear_span(r, 0, cols);
                    }
                    self.clear_span(self.cy, 0, (self.cx + 1).min(cols));
                }
                2 | 3 => {
                    for r in 0..rows {
                        self.clear_span(r, 0, cols);
                    }
                }
                _ => {}
            },
            b'K' => match arg(0, 0) {
                0 => self.clear_span(self.cy, self.cx.min(cols), cols),
                1 => self.clear_span(self.cy, 0, (self.cx + 1).min(cols)),
                2 => self.clear_span(self.cy, 0, cols),
                _ => {}
            },
            b'H' | b'f' => {
                self.cy = (arg(0, 1) - 1).min(rows - 1);
                self.cx = (arg(1, 1) - 1).min(cols - 1);
            }
            b'A' => self.cy = self.cy.saturating_sub(arg(0, 1)),
            b'B' => self.cy = (self.cy + arg(0, 1)).min(rows - 1),
            b'C' => self.cx = (self.cx + arg(0, 1)).min(cols - 1),
            b'D' => self.cx = self.cx.min(cols - 1).saturating_sub(arg(0, 1)),
            _ => {}
        }
    }

    fn sgr(&mut self, v: u16) {
        match v {
            0 => {
                self.fg = DEFAULT_FG;
                self.bg = DEFAULT_BG;
                self.bold = false;
            }
            1 => self.bold = true,
            22 => self.bold = false,
            30..=37 => self.fg = (v - 30) as u8,
            39 => self.fg = DEFAULT_FG,
            40..=47 => self.bg = (v - 40) as u8,
            49 => self.bg = DEFAULT_BG,
            90..=97 => self.fg = (v - 90 + 8) as u8,
            100..=107 => self.bg = (v - 100 + 8) as u8,
            _ => {}
        }
    }

    fn line_feed(&mut self) {
        if self.cy + 1 < self.rows {
            self.cy += 1;
        } else {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        let (cols, rows) = (self.cols, self.rows);
        self.cells.copy_within(cols..cols * rows, 0);
        let row_px = self.width * GLYPH_H * self.scale;
        self.shadow.copy_within(row_px..rows * row_px, 0);
        self.dirty[..rows].fill(true);
        self.clear_span(rows - 1, 0, cols);
    }

    fn clear_span(&mut self, row: usize, from: usize, to: usize) {
        let blank = self.blank();
        for col in from..to {
            self.put(col, row, blank);
        }
    }

    fn put(&mut self, col: usize, row: usize, cell: Cell) {
        self.cells[row * self.cols + col] = cell;
        self.render(col, row, false);
    }

    fn erase_cursor(&mut self) {
        if let Some((col, row)) = self.drawn_cursor.take() {
            self.render(col, row, false);
        }
    }

    fn draw_cursor(&mut self) {
        let pos = (self.cx.min(self.cols - 1), self.cy);
        self.render(pos.0, pos.1, true);
        self.drawn_cursor = Some(pos);
    }

    fn render(&mut self, col: usize, row: usize, invert: bool) {
        let cell = self.cells[row * self.cols + col];
        let (mut fg, mut bg) = (cell.fg, cell.bg);
        if invert {
            core::mem::swap(&mut fg, &mut bg);
        }
        let fg = color::pixel(self.format, fg);
        let bg = color::pixel(self.format, bg);
        let s = self.scale;
        let glyph = &font::GLYPHS[cell.ch as usize];
        let x0 = col * GLYPH_W * s;
        let y0 = row * GLYPH_H * s;
        for (gy, bits) in glyph.iter().enumerate() {
            for sy in 0..s {
                let start = (y0 + gy * s + sy) * self.width + x0;
                let line = &mut self.shadow[start..start + GLYPH_W * s];
                for gx in 0..GLYPH_W {
                    let px = if bits & (0x80 >> gx) != 0 { fg } else { bg };
                    line[gx * s..(gx + 1) * s].fill(px);
                }
            }
        }
        self.dirty[row] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::pixel;

    struct Fixture {
        cells: Vec<Cell>,
        shadow: Vec<u32>,
        w: usize,
        h: usize,
    }

    impl Fixture {
        fn new(w: usize, h: usize) -> Self {
            Fixture {
                cells: vec![Cell::BLANK; 4096],
                shadow: vec![0; w * h],
                w,
                h,
            }
        }
        fn term(&mut self) -> Terminal<'_> {
            Terminal::new(
                self.w,
                self.h,
                PixelFormat::Bgr,
                &mut self.cells,
                &mut self.shadow,
            )
        }
    }

    fn row_text(t: &Terminal, row: usize) -> String {
        (0..t.cols())
            .map(|c| t.cell(c, row).ch as char)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn geometry_scales_at_1600_px() {
        assert_eq!(geometry(1920, 1080), (120, 33, 2));
        assert_eq!(geometry(1024, 768), (128, 48, 1));
        assert_eq!(geometry(1280, 800), (160, 50, 1));
    }

    #[test]
    fn prints_and_moves_cursor() {
        let mut f = Fixture::new(80, 64); // 10 x 4 cells
        let mut t = f.term();
        t.write_bytes(b"AB");
        assert_eq!(row_text(&t, 0), "AB");
        assert_eq!(t.cursor(), (2, 0));
    }

    #[test]
    fn newline_wrap_and_scroll() {
        let mut f = Fixture::new(80, 64);
        let mut t = f.term();
        t.write_bytes(b"0123456789X\nl2\nl3\nl4");
        // "0123456789" filled row 0, "X" wrapped to row 1, then 3 more lines
        // forced one scroll.
        assert_eq!(row_text(&t, 0), "X");
        assert_eq!(row_text(&t, 1), "l2");
        assert_eq!(row_text(&t, 3), "l4");
        assert_eq!(t.cursor(), (2, 3));
    }

    #[test]
    fn tab_carriage_return_backspace() {
        let mut f = Fixture::new(160, 32); // 20 x 2
        let mut t = f.term();
        t.write_bytes(b"a\tb");
        assert_eq!(t.cell(8, 0).ch, b'b');
        t.write_bytes(b"\rZ\x08\x08Y");
        assert_eq!(row_text(&t, 0), "Y       b");
    }

    #[test]
    fn sgr_colours_and_bold() {
        let mut f = Fixture::new(80, 16);
        let mut t = f.term();
        t.write_bytes(b"\x1b[31;44mR\x1b[1mB\x1b[0mN");
        assert_eq!(
            t.cell(0, 0),
            Cell {
                ch: b'R',
                fg: 1,
                bg: 4
            }
        );
        assert_eq!(
            t.cell(1, 0),
            Cell {
                ch: b'B',
                fg: 9,
                bg: 4
            }
        );
        assert_eq!(
            t.cell(2, 0),
            Cell {
                ch: b'N',
                fg: 7,
                bg: 0
            }
        );
    }

    #[test]
    fn erase_display_and_line() {
        let mut f = Fixture::new(80, 32); // 10 x 2
        let mut t = f.term();
        t.write_bytes(b"abcdefghij\nklmnop");
        t.write_bytes(b"\x1b[1;4H\x1b[K");
        assert_eq!(row_text(&t, 0), "abc");
        assert_eq!(row_text(&t, 1), "klmnop");
        t.write_bytes(b"\x1b[2J");
        assert_eq!(row_text(&t, 0), "");
        assert_eq!(row_text(&t, 1), "");
    }

    #[test]
    fn cursor_positioning_is_clamped() {
        let mut f = Fixture::new(80, 32);
        let mut t = f.term();
        t.write_bytes(b"\x1b[99;99H");
        assert_eq!(t.cursor(), (9, 1));
        t.write_bytes(b"\x1b[5D\x1b[A");
        assert_eq!(t.cursor(), (4, 0));
    }

    #[test]
    fn glyph_pixels_are_rendered_into_shadow() {
        let mut f = Fixture::new(80, 32);
        let mut t = f.term();
        t.write_bytes(b"A\n");
        // Spleen 'A' row 2 is 0x7C = .XXXXX..
        let fg = pixel(PixelFormat::Bgr, 7);
        let bg = pixel(PixelFormat::Bgr, 0);
        assert_eq!(t.shadow_pixel(0, 2), bg);
        assert_eq!(t.shadow_pixel(1, 2), fg);
        assert_eq!(t.shadow_pixel(5, 2), fg);
        assert_eq!(t.shadow_pixel(6, 2), bg);
    }

    #[test]
    fn cursor_is_drawn_inverted() {
        let mut f = Fixture::new(80, 32);
        let t = f.term();
        // Blank cell at (0,0) rendered inverted: background pixel is fg colour.
        assert_eq!(t.shadow_pixel(0, 0), pixel(PixelFormat::Bgr, 7));
    }

    #[test]
    fn flush_honours_stride_and_only_copies_dirty_rows() {
        let (w, h, stride) = (80, 32, 100);
        let mut f = Fixture::new(w, h);
        let mut t = f.term();
        let mut fb = vec![0xDEAD_BEEFu32; stride * h];
        t.flush(&mut fb, stride);
        assert_eq!(fb[w], 0xDEAD_BEEF, "padding beyond width untouched");
        fb.fill(0x1234_5678);
        t.write_bytes(b"\x1b[2;1Hx"); // only touches row 1 (and cursor rows 0/1)
        t.flush(&mut fb, stride);
        assert_eq!(fb[16 * stride], t.shadow_pixel(0, 16));
        fb.fill(0x1234_5678);
        t.flush(&mut fb, stride);
        assert_eq!(
            fb[16 * stride],
            0x1234_5678,
            "clean rows are not copied again"
        );
    }

    #[test]
    fn scale_two_draws_2x2_blocks() {
        let mut f = Fixture::new(1600, 32);
        let t = f.term();
        assert_eq!(t.cols(), 100);
        assert_eq!(t.rows(), 1);
        // Inverted blank cursor cell: 16x32 block of fg colour.
        assert_eq!(t.shadow_pixel(15, 31), pixel(PixelFormat::Bgr, 7));
        assert_eq!(t.shadow_pixel(16, 0), pixel(PixelFormat::Bgr, 0));
    }

    #[test]
    fn garbage_input_never_panics() {
        let mut f = Fixture::new(80, 32);
        let mut t = f.term();
        let junk: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        t.write_bytes(&junk);
        let (cx, cy) = t.cursor();
        assert!(cx <= t.cols() && cy < t.rows());
    }
}
