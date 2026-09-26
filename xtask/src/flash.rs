//! Writing to and checking the physical USB stick. The stick is only ever
//! found through its serial-pinned by-id link, and several independent checks
//! run before any write. These commands never call sudo.

use crate::build::Artifacts;
use crate::config::{USB_BY_ID, USB_MAX_BYTES, USB_SERIAL};
use crate::image::{self, Layout, e2fs_target};
use crate::util::{out_dir, run, run_stdout};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Stick {
    /// Whole-disk device the by-id link resolved to, e.g. /dev/sda. Used only
    /// for the sysfs and mount checks; tools are given `Stick::target()`.
    pub dev: PathBuf,
    /// Kernel name, e.g. sda.
    pub name: String,
    pub bytes: u64,
}

impl Stick {
    /// What every tool opens: the serial-pinned by-id link, resolved by the
    /// kernel on each open. If the stick is pulled, the link disappears and
    /// the tool fails; another disk that inherits the /dev/sdX name is never
    /// reachable through it.
    pub fn target() -> &'static Path {
        Path::new(USB_BY_ID)
    }
}

/// Facts about a block device, read from sysfs; split out so the safety
/// rules can be unit-tested.
pub struct DeviceFacts<'a> {
    pub name: &'a str,
    pub removable: bool,
    pub bytes: u64,
    pub sysfs_path: &'a str,
    /// Mount points of the disk and its partitions.
    pub mounts: Vec<&'a str>,
}

pub fn check_safe(f: &DeviceFacts) -> Result<()> {
    if f.name.starts_with("nvme") {
        bail!("{} is an NVMe disk", f.name);
    }
    if !f.removable {
        bail!("{} is not removable", f.name);
    }
    if !f.sysfs_path.contains("/usb") {
        bail!("{} is not attached over USB ({})", f.name, f.sysfs_path);
    }
    if f.bytes == 0 || f.bytes > USB_MAX_BYTES {
        bail!("{} has unexpected size {} bytes", f.name, f.bytes);
    }
    if let Some(m) = f
        .mounts
        .iter()
        .find(|m| matches!(**m, "/" | "/boot" | "/boot/efi"))
    {
        bail!("{} is mounted at {m}", f.name);
    }
    Ok(())
}

fn sysfs(name: &str, file: &str) -> Result<String> {
    Ok(fs::read_to_string(format!("/sys/block/{name}/{file}"))?
        .trim()
        .to_string())
}

/// (device, mount point) pairs from /proc/mounts text for `dev` (e.g.
/// /dev/sda) and its partitions. Mount points are unescaped (`\040` = space).
pub fn parse_mounts(text: &str, dev: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.replace("\\040", " ")))
        })
        .filter(|(d, _)| {
            d.strip_prefix(dev)
                .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
        })
        .collect()
}

fn mounts_of(dev: &Path) -> Result<Vec<(String, String)>> {
    Ok(parse_mounts(
        &fs::read_to_string("/proc/mounts")?,
        &dev.display().to_string(),
    ))
}

pub fn resolve() -> Result<Stick> {
    let dev = fs::canonicalize(USB_BY_ID).with_context(|| {
        format!("USB stick with serial {USB_SERIAL} not found ({USB_BY_ID}); is it plugged in?")
    })?;
    let name = dev.file_name().unwrap().to_string_lossy().to_string();
    let bytes = sysfs(&name, "size")?.parse::<u64>()? * 512;
    let sysfs_path = fs::canonicalize(format!("/sys/block/{name}"))?
        .display()
        .to_string();
    let mounts = mounts_of(&dev)?;
    check_safe(&DeviceFacts {
        name: &name,
        removable: sysfs(&name, "removable")? == "1",
        bytes,
        sysfs_path: &sysfs_path,
        mounts: mounts.iter().map(|(_, m)| m.as_str()).collect(),
    })?;
    Ok(Stick { dev, name, bytes })
}

fn unmount_all(stick: &Stick) -> Result<()> {
    for (dev, mnt) in mounts_of(&stick.dev)? {
        println!("unmounting {dev} ({mnt})");
        run(Command::new("udisksctl").args(["unmount", "--no-user-interaction", "-b", &dev]))?;
    }
    Ok(())
}

fn ensure_access(stick: &Stick, write: bool) -> Result<()> {
    match fs::OpenOptions::new()
        .read(true)
        .write(write)
        .open(Stick::target())
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => bail!(
            "no access to {}. Run `cargo xtask setup-udev` once and follow its instructions.",
            stick.dev.display()
        ),
        Err(e) => Err(e.into()),
    }
}

fn sync() -> Result<()> {
    run(&mut Command::new("sync"))
}

/// `flash --kernel`: replace loader, kernel and cmdline on the ESP only.
pub fn flash_kernel(art: &Artifacts, cmdline: &str) -> Result<()> {
    let stick = resolve()?;
    unmount_all(&stick)?;
    ensure_access(&stick, true)?;
    let layout = image::read_layout(Stick::target())
        .context("stick has no Relay OS layout; run `cargo xtask flash --full` first")?;
    image::write_esp(Stick::target(), layout.esp, art, cmdline, false)?;
    sync()?;
    println!(
        "updated loader and kernel on {} ({})",
        stick.dev.display(),
        stick.name
    );
    Ok(())
}

/// `flash --full`: repartition the whole stick and rebuild both filesystems.
pub fn flash_full(art: &Artifacts, cmdline: &str, yes: bool) -> Result<()> {
    let stick = resolve()?;
    println!(
        "About to ERASE {} ({}, serial {USB_SERIAL}, {:.1} GB).",
        stick.dev.display(),
        stick.name,
        stick.bytes as f64 / 1e9
    );
    if !yes {
        print!("Type ERASE to continue: ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().lock().read_line(&mut answer)?;
        if answer.trim() != "ERASE" {
            bail!("aborted");
        }
    }
    // The prompt can wait indefinitely: check everything again before writing.
    let stick = resolve()?;
    unmount_all(&stick)?;
    ensure_access(&stick, true)?;
    image::partition(Stick::target(), false)?;
    let layout = image::read_layout(Stick::target())?;
    image::write_esp(Stick::target(), layout.esp, art, cmdline, true)?;
    let staging = image::stage_rootfs()?;
    println!(
        "creating ext2 on {:.1} GB (takes a minute)...",
        layout.root.bytes() as f64 / 1e9
    );
    image::make_ext2(Stick::target(), layout.root, &staging)?;
    sync()?;
    image::fsck(Stick::target(), layout.root)?;
    println!(
        "done. Linux still sees the old partition table: replug the stick before mounting it."
    );
    Ok(())
}

/// Recursively lists an ext2 tree with debugfs (`ls -p` output is
/// `/inode/mode/uid/gid/name/size/`).
fn list_tree(target: &str, dir: &str, out: &mut Vec<String>) -> Result<()> {
    let text = run_stdout(
        Command::new("debugfs")
            .arg("-R")
            .arg(format!("ls -p \"{dir}\""))
            .arg(target),
    )?;
    for line in text.lines() {
        let f: Vec<&str> = line.split('/').collect();
        if f.len() < 7 || f[5] == "." || f[5] == ".." || f[5] == "lost+found" {
            continue;
        }
        let mode = u32::from_str_radix(f[2], 8).unwrap_or(0);
        let path = if dir == "/" {
            format!("/{}", f[5])
        } else {
            format!("{dir}/{}", f[5])
        };
        let is_dir = mode & 0o170000 == 0o040000;
        out.push(format!(
            "{:06o} {:>4}:{:<4} {:>10}  {path}{}",
            mode,
            f[3],
            f[4],
            f[6],
            if is_dir { "/" } else { "" }
        ));
        if is_dir {
            list_tree(target, &path, out)?;
        }
    }
    Ok(())
}

/// `verify-usb`: e2fsck the stick's root and print its file tree.
pub fn verify_usb() -> Result<()> {
    let stick = resolve()?;
    ensure_access(&stick, false)?;
    let Layout { root, .. } = image::read_layout(Stick::target())?;
    image::fsck(Stick::target(), root).context("filesystem check FAILED")?;
    println!("e2fsck: clean");
    let mut lines = Vec::new();
    list_tree(&e2fs_target(Stick::target(), root), "/", &mut lines)?;
    for l in lines {
        println!("{l}");
    }
    Ok(())
}

/// `setup-udev`: write a rule granting the current user access to this one
/// stick, and print the commands to install it.
pub fn setup_udev() -> Result<()> {
    let user = std::env::var("USER").context("USER not set")?;
    let rule = format!(
        "# Relay OS development: give {user} access to the Kingston test stick only.\n\
         SUBSYSTEM==\"block\", ENV{{ID_SERIAL_SHORT}}==\"{USB_SERIAL}\", OWNER=\"{user}\", MODE=\"0660\"\n"
    );
    fs::create_dir_all(out_dir())?;
    let file = out_dir().join("70-relay-usb.rules");
    fs::write(&file, &rule)?;
    println!("Wrote {}:\n\n{rule}", file.display());
    println!("Install it with:\n");
    println!(
        "  sudo install -m 0644 {} /etc/udev/rules.d/70-relay-usb.rules",
        file.display()
    );
    println!("  sudo udevadm control --reload");
    println!("  sudo udevadm trigger --subsystem-match=block --action=change");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_facts() -> DeviceFacts<'static> {
        DeviceFacts {
            name: "sda",
            removable: true,
            bytes: 15_472_047_104,
            sysfs_path: "/sys/devices/pci0000:00/0000:00:14.0/usb4/4-3/4-3:1.0/host2/target2:0:0/2:0:0:0/block/sda",
            mounts: vec!["/media/maw/52454c41"],
        }
    }

    #[test]
    fn parses_mounts_of_the_disk_and_its_partitions_only() {
        let text = "/dev/sda2 /media/maw/My\\040Stick ext2 rw 0 0\n\
                    /dev/sda1 /media/maw/RELAYESP vfat rw 0 0\n\
                    /dev/sdab1 /mnt/other ext4 rw 0 0\n\
                    /dev/nvme0n1p2 / ext4 rw 0 0\n";
        assert_eq!(
            parse_mounts(text, "/dev/sda"),
            vec![
                ("/dev/sda2".to_string(), "/media/maw/My Stick".to_string()),
                ("/dev/sda1".to_string(), "/media/maw/RELAYESP".to_string()),
            ]
        );
    }

    /// Tools open the stick through the serial-pinned by-id link on every
    /// run, so a disk that inherits the stick's /dev/sdX name (stick pulled
    /// during the ERASE prompt) can never be written.
    #[test]
    fn tools_address_the_stick_by_id_not_by_dev_name() {
        let src = include_str!("flash.rs");
        let code = &src[..src.find("#[cfg(test)]").unwrap()];
        for call in ["image::", "e2fs_target("] {
            for line in code.lines().filter(|l| l.contains(call)) {
                assert!(
                    !line.contains("stick.dev"),
                    "uses /dev/sdX: {}",
                    line.trim()
                );
            }
        }
        assert_eq!(Stick::target(), std::path::Path::new(USB_BY_ID));
    }

    #[test]
    fn accepts_the_test_stick() {
        check_safe(&ok_facts()).unwrap();
    }

    #[test]
    fn rejects_nvme_fixed_non_usb_huge_and_system_mounts() {
        let mut f = ok_facts();
        f.name = "nvme0n1";
        assert!(check_safe(&f).is_err());
        let mut f = ok_facts();
        f.removable = false;
        assert!(check_safe(&f).is_err());
        let mut f = ok_facts();
        f.sysfs_path = "/sys/devices/pci0000:00/0000:00:17.0/ata1/host0/block/sda";
        assert!(check_safe(&f).is_err());
        let mut f = ok_facts();
        f.bytes = 1 << 40;
        assert!(check_safe(&f).is_err());
        let mut f = ok_facts();
        f.mounts = vec!["/boot/efi"];
        assert!(check_safe(&f).is_err());
    }
}
