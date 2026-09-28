//! QEMU command lines for interactive runs and end-to-end tests.

use crate::config::{OVMF_CODE, OVMF_VARS};
use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The QEMU id of the USB stick, for QMP `device_del`.
pub const STICK_DEVICE: &str = "stick-usb";

pub struct Qemu {
    pub disk: PathBuf,
    pub vars: PathBuf,
    pub headless: bool,
    /// Where QEMU listens for QMP: a socket in the run directory, which
    /// only its owner can enter (QMP can make QEMU run commands).
    pub qmp: Option<PathBuf>,
}

impl Qemu {
    /// Copies the image and a fresh OVMF variable store into `run_dir`,
    /// which becomes private to this user.
    pub fn prepare(image: &Path, run_dir: &Path) -> Result<Qemu> {
        fs::create_dir_all(run_dir)?;
        fs::set_permissions(run_dir, fs::Permissions::from_mode(0o700))?;
        let disk = run_dir.join("disk.img");
        let vars = run_dir.join("OVMF_VARS.fd");
        fs::copy(image, &disk).context("copying disk image")?;
        fs::copy(OVMF_VARS, &vars).with_context(|| format!("copying {OVMF_VARS}"))?;
        Ok(Qemu {
            disk,
            vars,
            headless: false,
            qmp: None,
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
            .arg("-device")
            .arg(format!(
                "usb-storage,bus=xhci.0,drive=stick,id={STICK_DEVICE}"
            ))
            .args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"])
            .args(["-serial", "stdio", "-monitor", "none"]);
        if self.headless {
            c.args(["-display", "none"]);
        }
        if let Some(socket) = &self.qmp {
            // Unix socket addresses hold 108 bytes, less than a deep
            // checkout's path: QEMU binds the name in its own directory.
            let name = socket.file_name().unwrap_or_default().to_string_lossy();
            if let Some(dir) = socket.parent() {
                c.current_dir(dir);
            }
            c.arg("-chardev")
                .arg(format!("socket,id=qmp,path={name},server=on,wait=off"))
                .args(["-mon", "chardev=qmp,mode=control"]);
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

    /// QMP can make QEMU run any command, so its socket is a file in the
    /// run directory, which only its owner can enter, not a name in the
    /// abstract namespace, which every user can reach. Unix socket paths
    /// hold 108 bytes, less than a deep checkout's: QEMU runs in the run
    /// directory and binds the bare name.
    #[test]
    fn qmp_listens_in_the_private_run_directory() {
        let deep = Path::new("/very/deep").join("x".repeat(200));
        let q = Qemu {
            disk: deep.join("disk.img"),
            vars: deep.join("OVMF_VARS.fd"),
            headless: true,
            qmp: Some(deep.join("qmp.sock")),
        };
        let c = q.command();
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = args.iter().position(|a| a == "-chardev").expect("-chardev");
        assert_eq!(
            args[at + 1],
            "socket,id=qmp,path=qmp.sock,server=on,wait=off"
        );
        assert_eq!(c.get_current_dir(), Some(deep.as_path()));
        assert!(
            args.windows(2)
                .any(|w| w == ["-mon", "chardev=qmp,mode=control"])
        );
        assert!(!args.iter().any(|a| a == "-qmp" || a.contains("abstract")));
        assert!(args.contains(&"usb-storage,bus=xhci.0,drive=stick,id=stick-usb".into()));
    }

    #[test]
    fn the_run_directory_is_private() {
        let dir = crate::util::out_dir().join("qemu-selftest");
        let image = dir.join("image");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&image, b"").unwrap();
        let run = dir.join("run");
        let _ = fs::remove_dir_all(&run);
        Qemu::prepare(&image, &run).unwrap();
        let mode = fs::metadata(&run).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
