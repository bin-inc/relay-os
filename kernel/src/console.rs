//! The kernel console: framebuffer terminal + serial mirror + kernel log.

use crate::{klog, serial};
use boot_info::{FramebufferInfo, PHYS_OFFSET, PixelFormat as BootFormat};
use core::fmt::{self, Write};
use spin::{Mutex, Once};
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

/// The framebuffer the console draws on, recorded before anything can fault
/// so the panic screen can start the console itself (see `init_if_needed`).
static FRAMEBUFFER: Once<FramebufferInfo> = Once::new();

/// Records the framebuffer for `init` and `init_if_needed`. The first call
/// wins.
pub fn set_framebuffer(fb: &FramebufferInfo) {
    FRAMEBUFFER.call_once(|| *fb);
}

/// Sets up the framebuffer terminal on the framebuffer recorded with
/// `set_framebuffer`. Does nothing if none was recorded.
pub fn init() {
    if let Some(fb) = FRAMEBUFFER.get() {
        init_on(fb);
    }
}

/// Starts the console if it is not running yet. For the panic path: a fault
/// before `init` must still reach the screen, because the NUC has no serial
/// port.
pub fn init_if_needed() {
    if CONSOLE.lock().is_none() {
        init();
    }
}

fn init_on(fb: &FramebufferInfo) {
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
    // Clear the whole framebuffer, not just the terminal area: the loader's
    // progress squares sit at the right edge, beyond 1920 px on wide modes,
    // and must not survive a successful boot. Black is 0 in both formats.
    fb_slice.fill(0);
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
        // The framebuffer is write-combining: drain the CPU's WC buffers so
        // the text is on screen even if the CPU halts right after (the
        // panic screen's last line).
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}

/// Writes to the kernel log and serial only, not the screen: detail that
/// would scroll the startup lines away (`dmesg` shows it later).
pub fn log_bytes(bytes: &[u8]) {
    klog::KLOG.lock().write(bytes);
    serial::write(bytes);
}

struct LogSink;

impl Write for LogSink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        log_bytes(s.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _log(args: fmt::Arguments) {
    let _ = LogSink.write_fmt(args);
}

/// Like `kprintln!`, but to the kernel log and serial only.
#[macro_export]
macro_rules! klogln {
    ($($arg:tt)*) => { $crate::console::_log(format_args!("{}\n", format_args!($($arg)*))) };
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
