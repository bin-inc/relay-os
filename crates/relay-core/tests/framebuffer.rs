use relay_core::console::{FramebufferConsole, FramebufferInfo, Mirror, PixelFormat, TextOutput};

const WIDTH: u32 = 48;
const HEIGHT: u32 = 64;
const STRIDE: u32 = 64;
const BYTES_PER_PIXEL: usize = 4;
const BACKGROUND: u8 = 0xaa;

#[test]
fn console_wraps_scrolls_and_respects_stride() {
    let mut pixels = vec![BACKGROUND; STRIDE as usize * HEIGHT as usize * BYTES_PER_PIXEL];
    let mut console = test_console(&mut pixels);

    console.write_bytes(b"a\nb\nc\nd\n");

    let mut expected_pixels = vec![BACKGROUND; pixels.len()];
    {
        let mut expected = test_console(&mut expected_pixels);
        expected.write_bytes(b"b\nc\nd\n");
    }
    clear_bottom_row(&mut expected_pixels);

    assert_eq!(pixels, expected_pixels);
    assert!(padding_bytes_unchanged(&pixels));
}

#[test]
fn console_handles_carriage_return_tab_and_clips_partial_cells() {
    let mut pixels = vec![BACKGROUND; STRIDE as usize * HEIGHT as usize * BYTES_PER_PIXEL];
    let mut console = test_console(&mut pixels);

    console.write_bytes(b"ab\rc\tde");

    assert!(pixels.iter().any(|&byte| byte != BACKGROUND));
    assert!(padding_bytes_unchanged(&pixels));
}

#[test]
fn bitmask_framebuffer_renders_visible_pixels_with_nonzero_channel_masks() {
    let mut pixels = vec![0; WIDTH as usize * HEIGHT as usize * BYTES_PER_PIXEL];
    let mut console = FramebufferConsole::new(
        &mut pixels,
        FramebufferInfo {
            width: WIDTH,
            height: HEIGHT,
            stride_pixels: WIDTH,
            bytes_per_pixel: BYTES_PER_PIXEL as u32,
            pixel_format: PixelFormat::Bitmask,
            red_mask: 0x00ff_0000,
            green_mask: 0x0000_ff00,
            blue_mask: 0x0000_00ff,
            reserved_mask: 0xff00_0000,
        },
    )
    .unwrap();

    console.write_bytes(b"A");

    let (pixels, remainder) = pixels.as_chunks::<BYTES_PER_PIXEL>();
    assert!(remainder.is_empty());
    assert!(
        pixels
            .iter()
            .map(|&pixel| u32::from_le_bytes(pixel))
            .any(|pixel| pixel & 0x00ff_ffff != 0)
    );
}

#[test]
fn mirror_writes_identical_bytes_to_both_outputs() {
    let mut mirror = Mirror {
        primary: Recorder::default(),
        diagnostic: Recorder::default(),
    };

    mirror.write_bytes(b"[relay] phase=kernel-runtime status=ok\n");

    assert_eq!(
        mirror.primary.0,
        b"[relay] phase=kernel-runtime status=ok\n"
    );
    assert_eq!(mirror.primary, mirror.diagnostic);
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Recorder(Vec<u8>);

impl TextOutput for Recorder {
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

fn test_console(pixels: &mut [u8]) -> FramebufferConsole<'_> {
    FramebufferConsole::new(
        pixels,
        FramebufferInfo {
            width: WIDTH,
            height: HEIGHT,
            stride_pixels: STRIDE,
            bytes_per_pixel: BYTES_PER_PIXEL as u32,
            pixel_format: PixelFormat::Bgr,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        },
    )
    .unwrap()
}

fn padding_bytes_unchanged(pixels: &[u8]) -> bool {
    for row in 0..HEIGHT as usize {
        let start = (row * STRIDE as usize + WIDTH as usize) * BYTES_PER_PIXEL;
        let end = (row + 1) * STRIDE as usize * BYTES_PER_PIXEL;
        if pixels[start..end].iter().any(|&byte| byte != BACKGROUND) {
            return false;
        }
    }
    true
}

fn clear_bottom_row(pixels: &mut [u8]) {
    for row in HEIGHT as usize - 16..HEIGHT as usize {
        let start = row * STRIDE as usize * BYTES_PER_PIXEL;
        let end = start + WIDTH as usize * BYTES_PER_PIXEL;
        pixels[start..end].fill(0);
    }
}
