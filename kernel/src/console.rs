//! The kernel console: framebuffer terminal + serial mirror + kernel log.

use crate::{klog, serial};
use boot_info::{FramebufferInfo, PHYS_OFFSET, PixelFormat as BootFormat};
use core::fmt::{self, Write};
use spin::Mutex;
use term::{Cell, PixelFormat, Terminal};

/// The terminal never covers more than this; a larger framebuffer keeps a
/// black margin (bounds the static shadow buffer).
pub const MAX_W: usize = 1920;
pub const MAX_H: usize = 1080;
const MAX_CELLS: usize = (MAX_W / term::GLYPH_W) * (MAX_H / term::GLYPH_H);

static mut SHADOW: [u32; MAX_W * MAX_H] = [0; MAX_W * MAX_H];
static mut CELLS: [Cell; MAX_CELLS] = [Cell::BLANK; MAX_CELLS];

pub struct Console {
    term: Terminal<'static>,
    fb: &'static mut [u32],
    stride: usize,
}

pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

/// Sets up the framebuffer terminal. Must be called once.
pub fn init(fb: &FramebufferInfo) {
    let width = (fb.width as usize).min(MAX_W);
    let height = (fb.height as usize).min(MAX_H);
    let format = match fb.format {
        BootFormat::Rgb => PixelFormat::Rgb,
        BootFormat::Bgr => PixelFormat::Bgr,
    };
    let stride = fb.stride as usize;
    // SAFETY: the loader mapped the framebuffer in the linear map; init runs
    // once, so these are the only references to the static buffers.
    let (fb_slice, cells, shadow) = unsafe {
        (
            core::slice::from_raw_parts_mut(
                (PHYS_OFFSET + fb.phys_addr) as *mut u32,
                stride * fb.height as usize,
            ),
            core::slice::from_raw_parts_mut((&raw mut CELLS).cast::<Cell>(), MAX_CELLS),
            core::slice::from_raw_parts_mut((&raw mut SHADOW).cast::<u32>(), MAX_W * MAX_H),
        )
    };
    let mut term = Terminal::new(width, height, format, cells, shadow);
    term.flush(fb_slice, stride);
    *CONSOLE.lock() = Some(Console {
        term,
        fb: fb_slice,
        stride,
    });
}

/// Grid size of the console, if initialised.
pub fn size() -> Option<(usize, usize)> {
    CONSOLE
        .lock()
        .as_ref()
        .map(|c| (c.term.cols(), c.term.rows()))
}

/// Writes raw bytes (UTF-8 + ANSI) to every console sink.
pub fn write_bytes(bytes: &[u8]) {
    klog::KLOG.lock().write(bytes);
    serial::write(bytes);
    if let Some(c) = CONSOLE.lock().as_mut() {
        c.term.write_bytes(bytes);
        c.term.flush(c.fb, c.stride);
    }
}

struct Sink;

impl Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_bytes(s.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = Sink.write_fmt(args);
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => { $crate::console::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => { $crate::kprint!("{}\n", format_args!($($arg)*)) };
}

/// `[ ok ] <step>` status line.
pub fn ok(step: fmt::Arguments) {
    crate::kprintln!("[\x1b[32m ok \x1b[0m] {step}");
}

/// `[FAIL] <step>: <reason>` status line.
pub fn fail(step: &str, reason: fmt::Arguments) {
    crate::kprintln!("[\x1b[31mFAIL\x1b[0m] {step}: {reason}");
}

/// Releases every console lock unconditionally. Only for the panic path,
/// where the lock holder will never run again.
///
/// # Safety
/// No other code may be using the console concurrently (single core, and
/// the interrupted holder never resumes).
pub unsafe fn force_unlock() {
    unsafe {
        if CONSOLE.is_locked() {
            CONSOLE.force_unlock();
        }
        if serial::SERIAL.is_locked() {
            serial::SERIAL.force_unlock();
        }
        if klog::KLOG.is_locked() {
            klog::KLOG.force_unlock();
        }
    }
}
