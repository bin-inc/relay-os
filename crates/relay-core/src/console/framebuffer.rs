use noto_sans_mono_bitmap::{FontWeight, RasterHeight, get_raster, get_raster_width};

use super::TextOutput;

const FONT_HEIGHT: usize = RasterHeight::Size16.val();
const FONT_WIDTH: usize = get_raster_width(FontWeight::Regular, RasterHeight::Size16);
const TAB_WIDTH: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgb,
    Bgr,
    Bitmask,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferInfo {
    pub width: u32,
    pub height: u32,
    pub stride_pixels: u32,
    pub bytes_per_pixel: u32,
    pub pixel_format: PixelFormat,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FramebufferError {
    InvalidGeometry,
    BufferTooSmall,
}

/// A byte-addressable framebuffer surface. Implementations keep the hardware-specific
/// memory access policy (ordinary host memory or volatile MMIO) outside the text model.
pub trait Framebuffer {
    fn read_byte(&self, offset: usize) -> u8;
    fn write_byte(&mut self, offset: usize, value: u8);
}

pub struct SliceFramebuffer<'a> {
    bytes: &'a mut [u8],
}

impl<'a> SliceFramebuffer<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
}

impl Framebuffer for SliceFramebuffer<'_> {
    fn read_byte(&self, offset: usize) -> u8 {
        self.bytes[offset]
    }

    fn write_byte(&mut self, offset: usize, value: u8) {
        self.bytes[offset] = value;
    }
}

pub struct Console<B> {
    framebuffer: B,
    info: FramebufferInfo,
    columns: usize,
    rows: usize,
    column: usize,
    row: usize,
}

pub type FramebufferConsole<'a> = Console<SliceFramebuffer<'a>>;

impl<'a> Console<SliceFramebuffer<'a>> {
    pub fn new(bytes: &'a mut [u8], info: FramebufferInfo) -> Result<Self, FramebufferError> {
        let byte_len = bytes.len();
        Self::from_framebuffer(SliceFramebuffer::new(bytes), info, byte_len)
    }
}

impl<B: Framebuffer> Console<B> {
    pub fn from_framebuffer(
        framebuffer: B,
        info: FramebufferInfo,
        byte_len: usize,
    ) -> Result<Self, FramebufferError> {
        if info.width == 0
            || info.height == 0
            || info.stride_pixels < info.width
            || info.bytes_per_pixel != 4
        {
            return Err(FramebufferError::InvalidGeometry);
        }
        let row_bytes = usize::try_from(info.stride_pixels)
            .ok()
            .and_then(|stride| stride.checked_mul(usize::try_from(info.bytes_per_pixel).ok()?))
            .ok_or(FramebufferError::InvalidGeometry)?;
        let required = usize::try_from(info.height)
            .ok()
            .and_then(|height| height.checked_mul(row_bytes))
            .ok_or(FramebufferError::InvalidGeometry)?;
        let columns = usize::try_from(info.width).map_err(|_| FramebufferError::InvalidGeometry)?
            / FONT_WIDTH;
        let rows = usize::try_from(info.height).map_err(|_| FramebufferError::InvalidGeometry)?
            / FONT_HEIGHT;
        if required > byte_len || columns == 0 || rows == 0 {
            return Err(FramebufferError::BufferTooSmall);
        }
        Ok(Self {
            framebuffer,
            info,
            columns,
            rows,
            column: 0,
            row: 0,
        })
    }

    pub fn into_framebuffer(self) -> B {
        self.framebuffer
    }

    fn write_character(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\r' => self.column = 0,
            b'\t' => {
                let spaces = TAB_WIDTH - (self.column % TAB_WIDTH);
                for _ in 0..spaces {
                    self.write_character(b' ');
                }
            }
            0x20..=0x7e => {
                if self.column == self.columns {
                    self.new_line();
                }
                self.draw_glyph(byte as char);
                self.column += 1;
            }
            _ => self.write_character(b'?'),
        }
    }

    fn new_line(&mut self) {
        self.column = 0;
        self.row += 1;
        if self.row == self.rows {
            self.scroll();
            self.row -= 1;
        }
    }

    fn scroll(&mut self) {
        let visible_width = usize::try_from(self.info.width).unwrap() * 4;
        let stride = usize::try_from(self.info.stride_pixels).unwrap() * 4;
        let height = usize::try_from(self.info.height).unwrap();
        for y in FONT_HEIGHT..height {
            let source = y * stride;
            let destination = (y - FONT_HEIGHT) * stride;
            for offset in 0..visible_width {
                let value = self.framebuffer.read_byte(source + offset);
                self.framebuffer.write_byte(destination + offset, value);
            }
        }
        for y in height - FONT_HEIGHT..height {
            let start = y * stride;
            for offset in 0..visible_width {
                self.framebuffer.write_byte(start + offset, 0);
            }
        }
    }

    fn draw_glyph(&mut self, character: char) {
        let glyph = get_raster(character, FontWeight::Regular, RasterHeight::Size16)
            .or_else(|| get_raster('?', FontWeight::Regular, RasterHeight::Size16))
            .unwrap();
        let x = self.column * FONT_WIDTH;
        let y = self.row * FONT_HEIGHT;
        for (glyph_y, raster_row) in glyph.raster().iter().enumerate() {
            for (glyph_x, &intensity) in raster_row.iter().enumerate() {
                self.write_pixel(x + glyph_x, y + glyph_y, intensity);
            }
        }
    }

    fn write_pixel(&mut self, x: usize, y: usize, intensity: u8) {
        let width = usize::try_from(self.info.width).unwrap();
        let height = usize::try_from(self.info.height).unwrap();
        if x >= width || y >= height {
            return;
        }
        let offset = (y * usize::try_from(self.info.stride_pixels).unwrap() + x) * 4;
        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.framebuffer.write_byte(offset, intensity);
                self.framebuffer.write_byte(offset + 1, intensity);
                self.framebuffer.write_byte(offset + 2, intensity);
                self.framebuffer.write_byte(offset + 3, 0);
            }
            PixelFormat::Bgr => {
                self.framebuffer.write_byte(offset, intensity);
                self.framebuffer.write_byte(offset + 1, intensity);
                self.framebuffer.write_byte(offset + 2, intensity);
                self.framebuffer.write_byte(offset + 3, 0);
            }
            PixelFormat::Bitmask => {
                let pixel = scale_to_mask(intensity, self.info.red_mask)
                    | scale_to_mask(intensity, self.info.green_mask)
                    | scale_to_mask(intensity, self.info.blue_mask);
                for byte in 0..4 {
                    self.framebuffer
                        .write_byte(offset + byte, (pixel >> (byte * 8)) as u8);
                }
            }
        }
    }
}

impl<B: Framebuffer> TextOutput for Console<B> {
    fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_character(byte);
        }
    }
}

fn scale_to_mask(intensity: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    (((u32::from(intensity) * max + 127) / 255) << shift) & mask
}
