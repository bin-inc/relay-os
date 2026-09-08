use std::time::Duration;

use relay_xtask::qemu::{KERNEL_ENTRY_MARKER, QemuRun};

#[test]
fn production_image_reaches_kernel_after_exit_boot_services() {
    let mut run = QemuRun::boot_production_image(Duration::from_secs(20)).unwrap();
    run.wait_for_marker(KERNEL_ENTRY_MARKER).unwrap();
    assert!(!run.serial_log().contains("phase=uefi-fallback"));
}

#[test]
fn serial_validation_rejects_fallback_after_kernel_marker() {
    let log = "[relay] phase=kernel-entry status=ok\n[relay] phase=uefi-fallback\n";

    let error = relay_xtask::qemu::validate_serial_log(log).unwrap_err();

    assert_eq!(error, "serial log contains phase=uefi-fallback");
}
