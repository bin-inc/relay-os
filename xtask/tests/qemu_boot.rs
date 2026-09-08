use std::time::Duration;
use std::{fs, path::Path};

use relay_xtask::qemu::{KERNEL_ENTRY_MARKER, QemuRun};

const RUNTIME_BANNER: &str = "[relay] phase=kernel-runtime status=ok";

#[test]
fn production_image_reaches_kernel_after_real_exit_boot_services() {
    let mut run = QemuRun::boot_production_image(Duration::from_secs(20)).unwrap();
    run.wait_for_marker(KERNEL_ENTRY_MARKER).unwrap();
    let serial = run.serial_log();
    assert!(!serial.contains("phase=uefi-fallback"));
    assert!(serial.contains(RUNTIME_BANNER));

    let screenshot = fs::read(Path::new("target/qemu/framebuffer.ppm")).unwrap();
    assert!(relay_xtask::qemu::ppm_region_has_foreground(&screenshot, 0, 0, 640, 48,).unwrap());
}

#[test]
fn ppm_banner_region_requires_a_pixel_different_from_the_background() {
    let uniform = b"P6\n4 2\n255\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    let foreground = b"P6\n4 2\n255\n\x00\x00\x00\xff\xff\xff\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

    assert!(!relay_xtask::qemu::ppm_region_has_foreground(uniform, 0, 0, 4, 2).unwrap());
    assert!(relay_xtask::qemu::ppm_region_has_foreground(foreground, 0, 0, 4, 2).unwrap());
}
