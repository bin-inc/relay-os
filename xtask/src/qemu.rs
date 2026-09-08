use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};

use crate::qmp;

pub const KERNEL_ENTRY_MARKER: &str = "[relay] phase=kernel-entry status=ok";
const UEFI_FALLBACK_MARKER: &str = "phase=uefi-fallback";

pub struct QemuRun {
    child: Child,
    deadline: Instant,
    serial: PathBuf,
    qmp: PathBuf,
    qmp_log: PathBuf,
    framebuffer: PathBuf,
}

impl QemuRun {
    pub fn boot_production_image(timeout: Duration) -> Result<Self, String> {
        let image = Path::new("target/qemu-boot-test.img");
        crate::image::build(image).map_err(|error| error.to_string())?;
        Self::boot(image, "none", "tcg", timeout)
    }

    pub fn boot(
        image: &Path,
        display: &str,
        accel: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        if !image.is_file() {
            return Err(format!("image does not exist: {}", image.display()));
        }
        if display != "none" || accel != "tcg" {
            return Err("QEMU boot requires `--display none --accel tcg`".into());
        }

        let artifacts = Path::new("target/qemu");
        fs::create_dir_all(artifacts)
            .map_err(|error| format!("create QEMU artifact directory: {error}"))?;
        let serial = artifacts.join("serial.log");
        let stderr = artifacts.join("stderr.log");
        let qmp = artifacts.join("qmp.sock");
        let qmp_log = artifacts.join("qmp.log");
        let framebuffer = artifacts.join("framebuffer.ppm");
        let debug = artifacts.join("qemu-debug.log");
        let vars = artifacts.join("OVMF_VARS.fd");
        for path in [&serial, &stderr, &qmp, &qmp_log, &framebuffer, &debug] {
            let _ = fs::remove_file(path);
        }

        let firmware = Prebuilt::fetch(Source::EDK2_STABLE202408_R1, artifacts.join("ovmf"))
            .map_err(|error| format!("fetch OVMF firmware: {error}"))?;
        let code = firmware.get_file(Arch::X64, FileType::Code);
        fs::copy(firmware.get_file(Arch::X64, FileType::Vars), &vars)
            .map_err(|error| format!("prepare writable OVMF variables: {error}"))?;

        let qmp_argument = format!("unix:{},server=on,wait=off", qmp.display());
        let code_argument = format!("if=pflash,format=raw,readonly=on,file={}", code.display());
        let vars_argument = format!("if=pflash,format=raw,file={}", vars.display());
        let image_argument = format!("format=raw,file={}", image.display());
        let serial_argument = format!("file:{}", serial.display());
        let child = Command::new("qemu-system-x86_64")
            .args([
                "-machine",
                "q35",
                "-accel",
                accel,
                "-display",
                display,
                "-no-reboot",
                "-drive",
                &code_argument,
                "-drive",
                &vars_argument,
                "-drive",
                &image_argument,
                "-serial",
                &serial_argument,
                "-qmp",
                &qmp_argument,
                "-d",
                "int,cpu_reset",
                "-D",
                debug.to_str().ok_or("QEMU debug log path is not UTF-8")?,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::from(File::create(&stderr).map_err(|error| {
                format!("create QEMU stderr log {}: {error}", stderr.display())
            })?))
            .spawn()
            .map_err(|error| format!("could not start qemu-system-x86_64: {error}"))?;

        Ok(Self {
            child,
            deadline: Instant::now() + timeout,
            serial,
            qmp,
            qmp_log,
            framebuffer,
        })
    }

    pub fn wait_for_marker(&mut self, marker: &str) -> Result<(), String> {
        while Instant::now() < self.deadline {
            let serial = self.serial_log();
            match serial_marker_status(&serial, marker) {
                Err(error) => {
                    self.capture_diagnostics();
                    return Err(error);
                }
                Ok(true) => {
                    self.capture_diagnostics();
                    return Ok(());
                }
                Ok(false) => {}
            }
            let status = match self.child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    self.capture_diagnostics();
                    return Err(error.to_string());
                }
            };
            if let Some(status) = status {
                self.capture_diagnostics();
                return Err(format!("QEMU exited before marker `{marker}`: {status}"));
            }
            thread::sleep(Duration::from_millis(50));
        }
        self.capture_diagnostics();
        Err(format!("timed out waiting for serial marker `{marker}`"))
    }

    pub fn serial_log(&self) -> String {
        let mut serial = String::new();
        if let Ok(mut file) = File::open(&self.serial) {
            let _ = file.read_to_string(&mut serial);
        }
        serial
    }

    fn capture_diagnostics(&self) {
        let qmp_result = qmp::screendump(&self.qmp, &self.framebuffer);
        let _ = fs::write(&self.qmp_log, qmp_result.unwrap_or_else(|error| error));
    }
}

pub fn validate_serial_log(serial: &str) -> Result<(), String> {
    serial_marker_status(serial, KERNEL_ENTRY_MARKER)?
        .then_some(())
        .ok_or_else(|| format!("serial log lacks marker `{KERNEL_ENTRY_MARKER}`"))
}

fn serial_marker_status(serial: &str, marker: &str) -> Result<bool, String> {
    if serial.contains(UEFI_FALLBACK_MARKER) {
        Err("serial log contains phase=uefi-fallback".into())
    } else {
        Ok(serial.contains(marker))
    }
}

impl Drop for QemuRun {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
