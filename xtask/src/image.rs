//! Disk layout: GPT, a FAT32 ESP and an ext2 root, written into either an
//! image file or the whole-disk USB device at partition offsets. Nothing here
//! needs root, and nothing uses partition device nodes (they can be stale
//! after repartitioning, and re-reading them needs CAP_SYS_ADMIN).

use crate::build::Artifacts;
use crate::config::*;
use crate::util::{mtools, out_dir, root, run, run_stdout};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partition {
    pub start_lba: u64,
    pub sectors: u64,
}

impl Partition {
    pub fn offset(&self) -> u64 {
        self.start_lba * SECTOR
    }
    pub fn bytes(&self) -> u64 {
        self.sectors * SECTOR
    }
}

pub struct Layout {
    pub esp: Partition,
    pub root: Partition,
}

/// The sfdisk script for our two-partition layout. `fixed_guids` pins the
/// disk and partition GUIDs (QEMU image); otherwise sfdisk picks random ones.
pub fn sfdisk_script(fixed_guids: bool) -> String {
    let (disk, esp, rootp) = if fixed_guids {
        (
            format!("label-id: {IMAGE_DISK_GUID}\n"),
            format!(", uuid={IMAGE_ESP_GUID}"),
            format!(", uuid={IMAGE_ROOT_GUID}"),
        )
    } else {
        Default::default()
    };
    format!(
        "label: gpt\n{disk}first-lba: {ESP_START_LBA}\n\
         start={ESP_START_LBA}, size={ESP_SECTORS}, type={ESP_TYPE_GUID}{esp}, name=\"{ESP_LABEL}\"\n\
         start={ROOT_START_LBA}, type={LINUX_FS_TYPE_GUID}{rootp}, name=\"{ROOT_LABEL}\"\n"
    )
}

pub fn partition(target: &Path, fixed_guids: bool) -> Result<()> {
    let mut child = Command::new("sfdisk")
        .args([
            "--quiet",
            "--no-reread",
            "--no-tell-kernel",
            "--wipe",
            "always",
        ])
        .arg(target)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting sfdisk")?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(sfdisk_script(fixed_guids).as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("sfdisk failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

/// Parses `sfdisk --json` output into our layout (ESP + Linux partition).
pub fn parse_layout(json: &str) -> Result<Layout> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let parts = v["partitiontable"]["partitions"]
        .as_array()
        .context("no partitions")?;
    let find = |type_guid: &str| -> Result<Partition> {
        let p = parts
            .iter()
            .find(|p| {
                p["type"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case(type_guid))
            })
            .with_context(|| format!("no partition of type {type_guid}"))?;
        Ok(Partition {
            start_lba: p["start"].as_u64().context("start")?,
            sectors: p["size"].as_u64().context("size")?,
        })
    };
    Ok(Layout {
        esp: find(ESP_TYPE_GUID)?,
        root: find(LINUX_FS_TYPE_GUID)?,
    })
}

pub fn read_layout(target: &Path) -> Result<Layout> {
    parse_layout(&run_stdout(
        Command::new("sfdisk").arg("--json").arg(target),
    )?)
}

fn mtools_target(target: &Path, p: Partition) -> String {
    format!("{}@@{}", target.display(), p.offset())
}

/// Writes the loader, kernel and cmdline into the ESP. With `format` the
/// partition is first formatted as FAT32.
pub fn write_esp(
    target: &Path,
    esp: Partition,
    art: &Artifacts,
    cmdline: &str,
    format: bool,
) -> Result<()> {
    let img = mtools_target(target, esp);
    if format {
        run(mtools("mformat").args([
            "-i",
            &img,
            "-F",
            "-T",
            &esp.sectors.to_string(),
            "-v",
            ESP_LABEL,
            "::",
        ]))?;
        run(mtools("mmd").args(["-i", &img, "::/EFI", "::/EFI/BOOT", "::/EFI/RELAY"]))?;
    }
    let cmdline_file = out_dir().join("cmdline");
    fs::create_dir_all(out_dir())?;
    fs::write(&cmdline_file, cmdline)?;
    for (src, dst) in [
        (art.bootx64.as_path(), "::/EFI/BOOT/BOOTX64.EFI"),
        (art.kernel.as_path(), "::/EFI/RELAY/kernel.elf"),
        (cmdline_file.as_path(), "::/EFI/RELAY/cmdline"),
    ] {
        run(mtools("mcopy").args(["-o", "-i", &img]).arg(src).arg(dst))?;
    }
    Ok(())
}

/// Replaces one file (absolute ESP path, `/` separators) on an existing ESP.
pub fn esp_write(
    target: &Path,
    esp: Partition,
    path: &str,
    contents: &[u8],
    scratch: &Path,
) -> Result<()> {
    let file = scratch.join("esp-write.tmp");
    fs::write(&file, contents)?;
    run(mtools("mcopy")
        .args(["-o", "-i", &mtools_target(target, esp)])
        .arg(&file)
        .arg(format!("::{path}")))
}

/// Replaces only the cmdline file on an existing ESP.
pub fn set_cmdline(target: &Path, esp: Partition, cmdline: &str, scratch: &Path) -> Result<()> {
    esp_write(
        target,
        esp,
        "/EFI/RELAY/cmdline",
        cmdline.as_bytes(),
        scratch,
    )
}

/// Builds the staging tree for `/`: the fixed directories plus `rootfs/`.
pub fn stage_rootfs() -> Result<PathBuf> {
    let staging = out_dir().join("rootfs-staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    copy_tree(&root().join("rootfs"), &staging)?;
    for (dir, mode) in ROOT_DIRS {
        let p = staging.join(dir);
        fs::create_dir_all(&p)?;
        fs::set_permissions(&p, fs::Permissions::from_mode(*mode))?;
    }
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))?;
    Ok(staging)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            fs::set_permissions(&to, fs::Permissions::from_mode(0o755))?;
            copy_tree(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
            fs::set_permissions(&to, fs::Permissions::from_mode(0o644))?;
        }
    }
    Ok(())
}

/// Every path in the staging tree, as absolute paths inside the new fs.
fn staged_paths(staging: &Path) -> Result<Vec<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let p = entry?.path();
            out.push(format!("/{}", p.strip_prefix(base)?.display()));
            if p.is_dir() {
                walk(base, &p, out)?;
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    walk(staging, staging, &mut v)?;
    v.sort();
    Ok(v)
}

/// The path syntax e2fsprogs uses for a filesystem at a byte offset.
pub fn e2fs_target(target: &Path, p: Partition) -> String {
    format!("{}?offset={}", target.display(), p.offset())
}

/// Creates the ext2 root in `part`, populated from `staging`, owned by root.
pub fn make_ext2(target: &Path, part: Partition, staging: &Path) -> Result<()> {
    let blocks = part.bytes() / EXT2_BLOCK;
    run(Command::new("mke2fs")
        .args([
            "-q",
            "-F",
            "-t",
            "ext2",
            "-b",
            &EXT2_BLOCK.to_string(),
            "-I",
            "256",
        ])
        .args(["-O", MKE2FS_FEATURES, "-L", ROOT_LABEL])
        .arg("-E")
        .arg(format!("offset={},root_owner=0:0", part.offset()))
        .arg("-d")
        .arg(staging)
        .arg(target)
        .arg(blocks.to_string()))?;
    // mke2fs -d copies the host uid/gid; make everything root-owned.
    let mut script = String::new();
    for p in staged_paths(staging)? {
        script.push_str(&format!(
            "set_inode_field \"{p}\" uid 0\nset_inode_field \"{p}\" gid 0\n"
        ));
    }
    let script_file = out_dir().join("chown.debugfs");
    fs::write(&script_file, script)?;
    run(Command::new("debugfs")
        .arg("-w")
        .arg("-f")
        .arg(&script_file)
        .arg(e2fs_target(target, part)))
}

/// `e2fsck -fn`: read-only full check. Returns the checker output on failure.
pub fn fsck(target: &Path, part: Partition) -> Result<()> {
    run(Command::new("e2fsck")
        .arg("-fn")
        .arg(e2fs_target(target, part)))
}

/// Builds `target/relay/relay-os.img` with the given kernel command line.
pub fn build_image(art: &Artifacts, cmdline: &str) -> Result<PathBuf> {
    let img = out_dir().join("relay-os.img");
    fs::create_dir_all(out_dir())?;
    let _ = fs::remove_file(&img);
    fs::File::create(&img)?.set_len(IMAGE_BYTES)?;
    partition(&img, true)?;
    let layout = read_layout(&img)?;
    write_esp(&img, layout.esp, art, cmdline, true)?;
    let staging = stage_rootfs()?;
    make_ext2(&img, layout.root, &staging)?;
    fsck(&img, layout.root)?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_script_pins_guids() {
        let s = sfdisk_script(true);
        assert!(s.contains(&format!("label-id: {IMAGE_DISK_GUID}")));
        assert!(s.contains(&format!("uuid={IMAGE_ESP_GUID}")));
        assert!(s.contains("start=2048, size=131072"));
        assert!(s.contains("start=133120, type=0FC63DAF"));
    }

    #[test]
    fn random_script_has_no_guids() {
        assert!(!sfdisk_script(false).contains("uuid="));
        assert!(!sfdisk_script(false).contains("label-id"));
    }

    #[test]
    fn parses_sfdisk_json() {
        let json = r#"{"partitiontable":{"partitions":[
            {"start":2048,"size":131072,"type":"C12A7328-F81F-11D2-BA4B-00A0C93EC93B"},
            {"start":133120,"size":389120,"type":"0fc63daf-8483-4772-8e79-3d69d8477de4"}]}}"#;
        let l = parse_layout(json).unwrap();
        assert_eq!(
            l.esp,
            Partition {
                start_lba: 2048,
                sectors: 131072
            }
        );
        assert_eq!(l.root.offset(), 133120 * 512);
    }

    #[test]
    fn layout_requires_both_partitions() {
        let json = r#"{"partitiontable":{"partitions":[
            {"start":2048,"size":131072,"type":"C12A7328-F81F-11D2-BA4B-00A0C93EC93B"}]}}"#;
        assert!(parse_layout(json).is_err());
    }
}
