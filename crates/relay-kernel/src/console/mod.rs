mod framebuffer;
mod mirror;

use core::{
    cell::UnsafeCell,
    fmt::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use relay_abi::FramebufferInfo as BootFramebufferInfo;
use relay_core::console::{Console, FramebufferInfo, Mirror, PixelFormat, TextOutput};

use framebuffer::VolatileFramebuffer;
use mirror::SerialOutput;

type KernelConsole = Mirror<Console<VolatileFramebuffer>, SerialOutput>;

struct ConsoleStorage(UnsafeCell<Option<KernelConsole>>);

// `CONSOLE` is borrowed only while `WRITING` is true. Reentrant NMI, exception, and panic output
// cannot take that token and must use serial-only output, so they never alias the mutable console.
unsafe impl Sync for ConsoleStorage {}

static CONSOLE: ConsoleStorage = ConsoleStorage(UnsafeCell::new(None));
static WRITING: AtomicBool = AtomicBool::new(false);

/// # Safety
/// `info` must describe the framebuffer validated at kernel entry. Its physical range must remain
/// mapped through the boot direct map and exclusively owned by this kernel console.
pub unsafe fn initialize(info: &BootFramebufferInfo) -> Result<(), ()> {
    let byte_len = usize::try_from(info.byte_len).map_err(|_| ())?;
    // SAFETY: entry validation established the supplied physical range is mapped and in bounds.
    let bytes =
        unsafe { crate::arch::x86_64::memory::framebuffer_slice(info.physical_base, byte_len) };
    let framebuffer = VolatileFramebuffer::new(bytes);
    let console = Console::from_framebuffer(
        framebuffer,
        FramebufferInfo {
            width: info.width,
            height: info.height,
            stride_pixels: info.stride_pixels,
            bytes_per_pixel: info.bytes_per_pixel,
            pixel_format: match info.pixel_format {
                0 => PixelFormat::Rgb,
                1 => PixelFormat::Bgr,
                2 => PixelFormat::Bitmask,
                _ => return Err(()),
            },
            red_mask: info.red_mask,
            green_mask: info.green_mask,
            blue_mask: info.blue_mask,
            reserved_mask: info.reserved_mask,
        },
        byte_len,
    )
    .map_err(|_| ())?;
    // SAFETY: single-core startup initializes this slot exactly once before publishing output.
    unsafe {
        *CONSOLE.0.get() = Some(Mirror {
            primary: console,
            diagnostic: SerialOutput,
        });
    }
    Ok(())
}

pub fn write(bytes: &[u8]) {
    let Ok(_) = WRITING.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed) else {
        // A nested diagnostic must not borrow the active framebuffer console.
        crate::serial::write(bytes);
        return;
    };
    let _guard = WriteGuard;
    // SAFETY: `WriteGuard` holds the sole mutable-console token until this call completes.
    if let Some(console) = unsafe { (&mut *CONSOLE.0.get()).as_mut() } {
        console.write_bytes(bytes);
    } else {
        crate::serial::write(bytes);
    }
}

struct WriteGuard;

impl Drop for WriteGuard {
    fn drop(&mut self) {
        WRITING.store(false, Ordering::Release);
    }
}

pub fn write_fmt(arguments: fmt::Arguments<'_>) {
    let _ = ConsoleFormatter.write_fmt(arguments);
}

#[cfg(target_os = "none")]
pub fn panic_write(info: &core::panic::PanicInfo<'_>) {
    write_fmt(format_args!("[relay] panic: {info}\n"));
}

struct ConsoleFormatter;

impl fmt::Write for ConsoleFormatter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        write(value.as_bytes());
        Ok(())
    }
}
