//! QEMU command lines for interactive runs and end-to-end tests.

use crate::config::{OVMF_CODE, OVMF_VARS};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Qemu {
    pub disk: PathBuf,
    pub vars: PathBuf,
    pub headless: bool,
    pub qmp_socket: Option<PathBuf>,
}

impl Qemu {
    /// Copies the image and a fresh OVMF variable store into `run_dir`.
    pub fn prepare(image: &Path, run_dir: &Path) -> Result<Qemu> {
        fs::create_dir_all(run_dir)?;
        let disk = run_dir.join("disk.img");
        let vars = run_dir.join("OVMF_VARS.fd");
        fs::copy(image, &disk).context("copying disk image")?;
        fs::copy(OVMF_VARS, &vars).with_context(|| format!("copying {OVMF_VARS}"))?;
        Ok(Qemu {
            disk,
            vars,
            headless: false,
            qmp_socket: None,
        })
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new("qemu-system-x86_64");
        c.args(["-machine", "q35"]);
        for accel in accelerators(std::env::var("RELAY_QEMU_ACCEL").ok().as_deref()) {
            c.args(["-accel", accel]);
        }
        c.args(["-cpu", "max"]);
        c.args(["-smp", "1", "-m", "1G", "-no-reboot", "-nic", "none"])
            .arg("-drive")
            .arg(format!("if=pflash,format=raw,readonly=on,file={OVMF_CODE}"))
            .arg("-drive")
            .arg(format!("if=pflash,format=raw,file={}", self.vars.display()))
            .args([
                "-device",
                "qemu-xhci,id=xhci",
                "-device",
                "usb-kbd,bus=xhci.0",
            ])
            .arg("-drive")
            .arg(format!(
                "if=none,id=stick,format=raw,file={}",
                self.disk.display()
            ))
            .args(["-device", "usb-storage,bus=xhci.0,drive=stick"])
            .args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"])
            .args(["-serial", "stdio", "-monitor", "none"]);
        if self.headless {
            c.args(["-display", "none"]);
        }
        if let Some(sock) = &self.qmp_socket {
            c.arg("-qmp")
                .arg(format!("unix:{},server=on,wait=off", sock.display()));
        }
        c
    }
}

/// QEMU tries `-accel` options in order. By default KVM, falling back to TCG
/// (software emulation) where /dev/kvm is unavailable; `RELAY_QEMU_ACCEL=tcg`
/// forces TCG.
pub fn accelerators(env: Option<&str>) -> Vec<&str> {
    match env {
        Some(list) if !list.trim().is_empty() => list.split(',').map(str::trim).collect(),
        _ => vec!["kvm", "tcg"],
    }
}

/// `cargo xtask qemu`: interactive run with a window and serial on stdio.
pub fn run_interactive(image: &Path, run_dir: &Path, serial_only: bool) -> Result<()> {
    let mut q = Qemu::prepare(image, run_dir)?;
    q.headless = serial_only;
    let status = q.command().status()?;
    println!("qemu exited: {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerator_selection() {
        assert_eq!(accelerators(None), vec!["kvm", "tcg"]);
        assert_eq!(accelerators(Some("")), vec!["kvm", "tcg"]);
        assert_eq!(accelerators(Some("tcg")), vec!["tcg"]);
        assert_eq!(accelerators(Some("kvm, tcg")), vec!["kvm", "tcg"]);
    }
}
