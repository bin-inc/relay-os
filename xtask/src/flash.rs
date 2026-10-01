//! Writing to and checking the physical USB stick. The stick is only ever
//! found through its serial-pinned by-id link, and several independent checks
//! run before any write. These commands never call sudo.

use crate::build::Artifacts;
use crate::checks::{self, Machine};
use crate::config::{STICK_ROOT_SECTORS, USB_BY_ID, USB_MAX_BYTES, USB_SERIAL};
use crate::image::{self, Layout, Partition, e2fs_target};
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

/// `flash --kernel`: replace loader, kernel, `system.img` and cmdline on the
/// ESP only.
pub fn flash_kernel(art: &Artifacts, cmdline: &str) -> Result<()> {
    let stick = resolve()?;
    unmount_all(&stick)?;
    ensure_access(&stick, true)?;
    let layout = image::read_layout(Stick::target())
        .context("stick has no Relay OS layout; run `cargo xtask flash --full` first")?;
    image::write_esp(Stick::target(), layout.esp, art, cmdline, false)?;
    sync()?;
    println!(
        "updated loader, kernel and system.img on {} ({})",
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
    image::partition(Stick::target(), false, Some(STICK_ROOT_SECTORS))?;
    let layout = image::read_layout(Stick::target())?;
    image::write_esp(Stick::target(), layout.esp, art, cmdline, true)?;
    let staging = image::stage_rootfs()?;
    println!(
        "creating ext2 on {:.1} GiB...",
        layout.root.bytes() as f64 / (1u64 << 30) as f64
    );
    image::make_ext2(Stick::target(), layout.root, &staging)?;
    sync()?;
    image::fsck(Stick::target(), layout.root)?;
    println!(
        "done. Linux still sees the old partition table: replug the stick before mounting it."
    );
    Ok(())
}

/// One entry of an ext2 directory, as debugfs lists it.
struct Entry {
    name: String,
    mode: u32,
    uid: String,
    gid: String,
    size: String,
}

/// The entries of `dir` with debugfs (`ls -p` output is
/// `/inode/mode/uid/gid/name/size/`), without `.`, `..` and `lost+found`.
/// A missing directory has none (debugfs reports it on stderr).
fn list_dir(target: &str, dir: &str) -> Result<Vec<Entry>> {
    let text = run_stdout(
        Command::new("debugfs")
            .arg("-R")
            .arg(format!("ls -p \"{dir}\""))
            .arg(target),
    )?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('/').collect();
            if f.len() < 7 || f[5] == "." || f[5] == ".." || f[5] == "lost+found" {
                return None;
            }
            Some(Entry {
                name: f[5].to_string(),
                mode: u32::from_str_radix(f[2], 8).unwrap_or(0),
                uid: f[3].to_string(),
                gid: f[4].to_string(),
                size: f[6].to_string(),
            })
        })
        .collect())
}

/// Recursively lists an ext2 tree with debugfs.
fn list_tree(target: &str, dir: &str, out: &mut Vec<String>) -> Result<()> {
    for e in list_dir(target, dir)? {
        let path = if dir == "/" {
            format!("/{}", e.name)
        } else {
            format!("{dir}/{}", e.name)
        };
        let is_dir = e.mode & 0o170000 == 0o040000;
        out.push(format!(
            "{:06o} {:>4}:{:<4} {:>10}  {path}{}",
            e.mode,
            e.uid,
            e.gid,
            e.size,
            if is_dir { "/" } else { "" }
        ));
        if is_dir {
            list_tree(target, &path, out)?;
        }
    }
    Ok(())
}

/// Where the NUC's check scripts are (`rootfs/root/checks/`).
pub const CHECKS_DIR: &str = "/root/checks";

/// Checks the transcript of every script in `CHECKS_DIR` on the ext2 root
/// `root` of `target` as written on `machine` (spec §15 item 12). Returns
/// the lines to print and whether every script ran and its transcript
/// passed. Each line gives the time the transcript was last written:
/// `flash --kernel` leaves the transcripts of an earlier run on the stick,
/// so one older than the `system.img` built at `system_built` (seconds
/// since 1970, if known) fails; `flash --full` erases them.
pub fn check_transcripts(
    target: &Path,
    root: Partition,
    machine: Machine,
    scratch: &Path,
    system_built: Option<u64>,
) -> Result<(Vec<String>, bool)> {
    let mut scripts: Vec<String> = list_dir(&e2fs_target(target, root), CHECKS_DIR)?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.ends_with(".sh"))
        .collect();
    scripts.sort();
    let (mut out, mut ok) = (Vec::new(), true);
    for name in scripts {
        let path = format!("{CHECKS_DIR}/{name}");
        let log = shell::commands::transcript_name(&path);
        let read = |p: &str| -> Result<Option<String>> {
            Ok(image::read_ext2_file(target, root, p, scratch)?
                .map(|d| String::from_utf8_lossy(&d).into_owned()))
        };
        let script = read(&path)?.with_context(|| format!("{path}: cannot read it"))?;
        let Some(transcript) = read(&log)? else {
            out.push(format!("{path}: FAILED, not run (no {log})"));
            ok = false;
            continue;
        };
        let report = checks::check(&checks::parse(&script, machine)?, &transcript);
        // A transcript from before the system on the stick (a later
        // `flash --kernel`) shows what an older kernel and programs did.
        let (run, run_text) = modified(target, root, &log)?;
        let stale = system_built.filter(|&built| run < built);
        let passed = report.ok() && stale.is_none();
        let verdict = if passed { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {run_text})",
            report.passed, report.commands,
        ));
        if let Some(built) = stale {
            out.push(format!(
                "  the transcript is older than the system on the stick (system.img built {}): run the script again",
                shell::time::date(built)
            ));
        }
        out.extend(report.failures.iter().map(|f| format!("  {f}")));
        ok &= passed;
    }
    if out.is_empty() {
        out.push(format!("no check scripts in {CHECKS_DIR}"));
    }
    Ok((out, ok))
}

/// When the file at `path` was last changed: in seconds since 1970, and
/// in UTC as debugfs shows it (`Mon Sep 28 14:09:17 2026 UTC`).
fn modified(target: &Path, root: Partition, path: &str) -> Result<(u64, String)> {
    let text = run_stdout(
        Command::new("debugfs")
            .env("TZ", "UTC")
            .arg("-R")
            .arg(format!("stat \"{path}\""))
            .arg(e2fs_target(target, root)),
    )?;
    // `mtime: 0x68d9407d -- Mon Sep 28 14:09:17 2026`, with `:<nanoseconds>`
    // after the number on a filesystem with big inodes.
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix("mtime: 0x"))
        .and_then(|l| {
            let (number, when) = l.split_once(" -- ")?;
            let secs = u64::from_str_radix(number.split(':').next()?, 16).ok()?;
            Some((secs, format!("{when} UTC")))
        })
        .with_context(|| format!("{path}: debugfs shows no mtime"))
}

/// When the `system.img` on the ESP `esp` of `target` was built (seconds
/// since 1970), if it can be read.
fn system_built(target: &Path, esp: Partition, scratch: &Path) -> Option<u64> {
    let image = image::esp_read(target, esp, "/EFI/RELAY/system.img", scratch).ok()?;
    sysimg::Archive::parse(&image).ok().map(|a| a.build_time())
}

/// `verify-usb`: e2fsck the stick's root, print its file tree, and check
/// the transcripts of the check scripts run on the NUC.
pub fn verify_usb() -> Result<()> {
    let stick = resolve()?;
    ensure_access(&stick, false)?;
    let Layout { esp, root } = image::read_layout(Stick::target())?;
    image::fsck(Stick::target(), root).context("filesystem check FAILED")?;
    println!("e2fsck: clean");
    let mut lines = Vec::new();
    list_tree(&e2fs_target(Stick::target(), root), "/", &mut lines)?;
    for l in lines {
        println!("{l}");
    }
    let scratch = out_dir().join("verify-usb");
    let built = system_built(Stick::target(), esp, &scratch);
    match built {
        Some(secs) => println!("system.img: built {}", shell::time::date(secs)),
        None => println!("system.img: not readable on the stick; transcripts not compared with it"),
    }
    let (report, ok) = check_transcripts(Stick::target(), root, Machine::Nuc, &scratch, built)?;
    for l in report {
        println!("{l}");
    }
    if !ok {
        bail!("transcript check FAILED");
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
    use crate::image::Partition;

    /// An ext2 image holding `files` (path, contents) under its root.
    fn ext2_with(dir: &Path, files: &[(&str, &str)]) -> (PathBuf, Partition) {
        let dated: Vec<_> = files
            .iter()
            .map(|&(path, text)| (path, text, "2026-09-28 14:09:17 UTC"))
            .collect();
        ext2_dated(dir, &dated)
    }

    /// An ext2 image holding `files` (path, contents, time changed) under
    /// its root.
    fn ext2_dated(dir: &Path, files: &[(&str, &str, &str)]) -> (PathBuf, Partition) {
        let _ = fs::remove_dir_all(dir);
        for (path, text, when) in files {
            let p = dir.join("staging").join(path.trim_start_matches('/'));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            run(Command::new("touch").args(["-d", when]).arg(&p)).unwrap();
        }
        fs::create_dir_all(dir.join("staging")).unwrap();
        let img = dir.join("fs.img");
        run(Command::new("mke2fs")
            .args(["-q", "-F", "-t", "ext2", "-d"])
            .arg(dir.join("staging"))
            .arg(&img)
            .arg("1M"))
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        (img, whole)
    }

    #[test]
    fn transcripts_of_the_check_scripts_are_checked_as_on_the_nuc() {
        let dir = out_dir().join("verify-usb-selftest");
        let (img, root) = ext2_with(
            &dir,
            &[
                (
                    "/root/checks/a.sh",
                    "uname\n#> Relay\ndmesg\n#nuc> .*Kingston.*\n#qemu> .*QEMU.*\n",
                ),
                (
                    "/root/checks/a.log",
                    "+ uname\nRelay\n+ dmesg\ndisk Kingston DataTraveler 3.0\n",
                ),
                ("/root/checks/b.sh", "cat /root/notes/a\n#> remember me\n"),
                ("/root/checks/b.log", "+ cat /root/notes/a\nforgotten\n"),
                ("/root/checks/c.sh", "ls\n"),
                ("/root/checks/notes.txt", "not a script"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
                "/root/checks/c.sh: FAILED, not run (no /root/checks/c.log)",
            ]
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir, None).unwrap();
        assert!(!ok);
        assert!(
            lines[0].starts_with("/root/checks/a.sh: FAILED, 1 of 2"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_script_that_was_not_run_fails_the_check() {
        // Part 2 forgotten after the restart, or `sh` refused to start it.
        let dir = out_dir().join("verify-usb-selftest-not-run");
        let (img, root) = ext2_with(
            &dir,
            &[
                ("/root/checks/a.sh", "uname\n#> Relay\n"),
                ("/root/checks/a.log", "+ uname\nRelay\n"),
                ("/root/checks/b.sh", "ls\n"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, not run (no /root/checks/b.log)",
            ]
        );
    }

    #[test]
    fn a_transcript_older_than_the_system_on_the_stick_fails_the_check() {
        // `flash --kernel` wrote a system built at 2026-09-29 10:00:00 UTC
        // and left the transcripts: one from before it shows what an older
        // kernel did. A run in the same second is not older.
        let built = 1_790_676_000;
        let dir = out_dir().join("verify-usb-selftest-stale");
        let (img, root) = ext2_dated(
            &dir,
            &[
                (
                    "/root/checks/a.sh",
                    "uname\n#> Relay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/a.log",
                    "+ uname\nRelay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/b.sh",
                    "uname\n#> Relay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/b.log",
                    "+ uname\nRelay\n",
                    "2026-09-29 10:00:00 UTC",
                ),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, Some(built)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  the transcript is older than the system on the stick (system.img built Tue Sep 29 10:00:00 UTC 2026): run the script again",
                "/root/checks/b.sh: ok, 1 of 1 commands as expected (run Tue Sep 29 10:00:00 2026 UTC)",
            ]
        );
        let (_, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok, "no system to compare with");
    }

    #[test]
    fn a_stick_without_check_scripts_passes() {
        let dir = out_dir().join("verify-usb-selftest-empty");
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok);
        assert_eq!(lines, ["no check scripts in /root/checks"]);
    }

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
