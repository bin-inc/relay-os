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
    /// Abstract-namespace socket name for QMP (see `qmp_name`).
    pub qmp_name: Option<String>,
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
            qmp_name: None,
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
        if let Some(name) = &self.qmp_name {
            c.arg("-chardev")
                .arg(format!(
                    "socket,id=qmp,path={name},server=on,wait=off,abstract=on"
                ))
                .args(["-mon", "chardev=qmp,mode=control"]);
        }
        c
    }
}

/// A QMP socket name for one QEMU run. It lives in Linux's abstract socket
/// namespace, so it has no filesystem path and no dependence on how deep the
/// checkout is (Unix socket paths are limited to 108 bytes).
pub fn qmp_name(tag: &str) -> String {
    let mut name = format!("relay-qmp-{}-{tag}", std::process::id());
    name.truncate(100);
    name
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

    /// Unix socket paths are limited to 108 bytes, which a deep checkout
    /// exceeded; QMP therefore listens on an abstract socket (a name, no path).
    #[test]
    fn qmp_listens_on_an_abstract_socket() {
        let deep = Path::new("/very/deep").join("x".repeat(200));
        let q = Qemu {
            disk: deep.join("disk.img"),
            vars: deep.join("OVMF_VARS.fd"),
            headless: true,
            qmp_name: Some("relay-qmp-42-boot".into()),
        };
        let args: Vec<String> = q
            .command()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = args.iter().position(|a| a == "-chardev").expect("-chardev");
        assert_eq!(
            args[at + 1],
            "socket,id=qmp,path=relay-qmp-42-boot,server=on,wait=off,abstract=on"
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["-mon", "chardev=qmp,mode=control"])
        );
        assert!(!args.iter().any(|a| a == "-qmp"));
    }

    #[test]
    fn qmp_names_are_short_and_unique_per_scenario() {
        let a = qmp_name("boot");
        assert!(a.starts_with("relay-qmp-") && a.ends_with("-boot"));
        assert!(a.contains(&std::process::id().to_string()));
        assert_ne!(a, qmp_name("panic_ud"));
        assert!(qmp_name(&"s".repeat(300)).len() <= 100);
    }
}
