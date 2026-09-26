//! Setting the GOP video mode chosen by `relay_boot::mode`.

use boot_info::{FramebufferInfo, PixelFormat};
use relay_boot::mode::{choose, cmdline_mode, edid_native};
use uefi::boot;
use uefi::proto::console::gop::{self, GraphicsOutput};

fn pixel_format(f: gop::PixelFormat) -> Option<PixelFormat> {
    match f {
        gop::PixelFormat::Rgb => Some(PixelFormat::Rgb),
        gop::PixelFormat::Bgr => Some(PixelFormat::Bgr),
        _ => None,
    }
}

/// Sets the best mode and returns the framebuffer description.
pub fn setup(cmdline: &str) -> uefi::Result<FramebufferInfo> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>()?;
    let native = boot::get_handle_for_protocol::<gop::EdidDiscovered>()
        .and_then(crate::proto::get::<gop::EdidDiscovered>)
        .ok()
        .and_then(|e| e.edid().and_then(edid_native));
    let mut gop = crate::proto::get::<GraphicsOutput>(handle)?;
    let modes: alloc::vec::Vec<_> = gop.modes().collect();
    let candidates: alloc::vec::Vec<(usize, usize, usize)> = modes
        .iter()
        .enumerate()
        .filter(|(_, m)| pixel_format(m.info().pixel_format()).is_some())
        .map(|(i, m)| (i, m.info().resolution().0, m.info().resolution().1))
        .collect();
    let current = gop.current_mode_info().resolution();
    if let Some(i) = choose(&candidates, cmdline_mode(cmdline), native, current)
        && modes[i].info().resolution() != current
    {
        gop.set_mode(&modes[i])?;
    }
    let info = gop.current_mode_info();
    let format = pixel_format(info.pixel_format()).ok_or(uefi::Status::UNSUPPORTED)?;
    let mut fb = gop.frame_buffer();
    uefi::println!(
        "video: {}x{} native={native:?}",
        info.resolution().0,
        info.resolution().1
    );
    Ok(FramebufferInfo {
        phys_addr: fb.as_mut_ptr() as u64,
        size: fb.size() as u64,
        width: info.resolution().0 as u32,
        height: info.resolution().1 as u32,
        stride: info.stride() as u32,
        format,
    })
}
