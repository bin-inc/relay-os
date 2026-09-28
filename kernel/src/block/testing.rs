//! Test support for the block module: an in-memory disk and partitioned
//! images written by util-linux, so the parser is checked against what Linux
//! writes. `sfdisk` 2.39 cannot write 4096-byte sectors to a file (it
//! recalculates the script for the file's 512), so those images come from
//! `fdisk -b 4096`, which loads the same script format with its `I` command.

use super::gpt::{Gpt, GptPartition, Guid};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use vfs::{BlockDevice, IoError, check_request};

/// A disk in memory.
pub struct MemDisk {
    pub data: Vec<u8>,
    block_size: usize,
    /// Reads that touch one of these blocks fail with `IoError::Device`.
    pub bad: Range<u64>,
}

impl MemDisk {
    pub fn new(data: Vec<u8>, block_size: usize) -> MemDisk {
        assert!(data.len().is_multiple_of(block_size));
        MemDisk {
            data,
            block_size,
            bad: 0..0,
        }
    }

    /// Byte range of a checked request.
    fn span(&self, lba: u64, len: usize) -> Result<Range<usize>, IoError> {
        check_request(self.block_size, self.block_count(), lba, len)?;
        let start = lba as usize * self.block_size;
        Ok(start..start + len)
    }
}

impl BlockDevice for MemDisk {
    fn block_size(&self) -> usize {
        self.block_size
    }
    fn block_count(&self) -> u64 {
        (self.data.len() / self.block_size) as u64
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let span = self.span(lba, buf.len())?;
        let end = lba + (buf.len() / self.block_size) as u64;
        if lba < self.bad.end && self.bad.start < end {
            return Err(IoError::Device);
        }
        buf.copy_from_slice(&self.data[span]);
        Ok(())
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        let span = self.span(lba, buf.len())?;
        self.data[span].copy_from_slice(buf);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// A fresh path `<workspace>/target/tmp/block/<name>-<n>`, unique within the
/// test binary. Kernel unit tests get no `CARGO_TARGET_TMPDIR`.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/tmp/block"));
    fs::create_dir_all(dir).unwrap();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{name}-{n}"));
    let _ = fs::remove_file(&path);
    path
}

/// A util-linux command. Missing tools fail the test: checking against
/// them is the point, so they are never skipped.
fn tool(name: &str) -> Command {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain(["/usr/sbin".into(), "/sbin".into()]);
    for dir in dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let mut cmd = Command::new(candidate);
            cmd.env("LC_ALL", "C");
            return cmd;
        }
    }
    panic!("install util-linux: {name} not found");
}

/// Runs `cmd` with `input` on stdin and returns its stdout.
fn run(mut cmd: Command, input: &str) -> String {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("starting {cmd:?}: {e}"));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        panic!(
            "{cmd:?} failed ({}):\n{stdout}{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    stdout
}

fn image_file(name: &str, bytes: u64) -> PathBuf {
    let path = scratch(name);
    fs::File::create(&path).unwrap().set_len(bytes).unwrap();
    path
}

fn take(path: &Path) -> Vec<u8> {
    let data = fs::read(path).unwrap();
    fs::remove_file(path).unwrap();
    data
}

/// A disk of `bytes` partitioned by `sfdisk` from `script` (512-byte sectors).
pub fn sfdisk_image(script: &str, bytes: u64) -> Vec<u8> {
    let path = image_file("sfdisk.img", bytes);
    let mut cmd = tool("sfdisk");
    cmd.args([
        "--quiet",
        "--no-reread",
        "--no-tell-kernel",
        "--wipe",
        "always",
    ])
    .arg(&path);
    run(cmd, script);
    take(&path)
}

/// A disk of `bytes` with `sector_size`-byte sectors, partitioned by
/// `fdisk -b <sector_size>` loading the sfdisk-format `script`.
pub fn fdisk_image(script: &str, bytes: u64, sector_size: usize) -> Vec<u8> {
    let path = image_file("fdisk.img", bytes);
    let script_path = scratch("fdisk.script");
    fs::write(&script_path, script).unwrap();
    let mut cmd = tool("fdisk");
    cmd.args(["-b", &sector_size.to_string(), "--wipe", "always"])
        .arg(&path);
    let out = run(cmd, &format!("I\n{}\nw\n", script_path.display()));
    fs::remove_file(&script_path).unwrap();
    // fdisk carries on (and writes an empty DOS label) if the script fails.
    assert!(out.contains("Script successfully applied"), "{out}");
    take(&path)
}

/// The GPT of `image` as util-linux reads it: `sfdisk --dump` for 512-byte
/// sectors, the same dump from fdisk's `O` command for other sizes.
pub fn expected_gpt(image: &[u8], sector_size: usize) -> Gpt {
    let path = scratch("dump.img");
    fs::write(&path, image).unwrap();
    let dump = if sector_size == 512 {
        let mut cmd = tool("sfdisk");
        cmd.arg("--dump").arg(&path);
        run(cmd, "")
    } else {
        let dump_path = scratch("fdisk.dump");
        let mut cmd = tool("fdisk");
        cmd.args(["-b", &sector_size.to_string()]).arg(&path);
        run(cmd, &format!("O\n{}\nq\n", dump_path.display()));
        let dump = fs::read_to_string(&dump_path).unwrap();
        fs::remove_file(&dump_path).unwrap();
        dump
    };
    fs::remove_file(&path).unwrap();
    parse_dump(&dump)
}

/// Parses the sfdisk dump format:
/// `label-id: <guid>` and `<node><n> : start=…, size=…, type=…, uuid=…`.
fn parse_dump(dump: &str) -> Gpt {
    let mut disk_guid = None;
    let mut partitions = Vec::new();
    for line in dump.lines() {
        if let Some(id) = line.strip_prefix("label-id: ") {
            disk_guid = Some(guid(id));
        }
        let Some((node, fields)) = line.split_once(" : ") else {
            continue;
        };
        let digits = node.len() - node.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let number = node[node.len() - digits..].parse().unwrap();
        let field = |key: &str| {
            fields
                .split(", ")
                .find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
                .unwrap_or_else(|| panic!("no {key} in {line}"))
                .trim()
        };
        let start: u64 = field("start").parse().unwrap();
        let size: u64 = field("size").parse().unwrap();
        partitions.push(GptPartition {
            number,
            type_guid: guid(field("type")),
            unique_guid: guid(field("uuid")),
            first_lba: start,
            last_lba: start + size - 1,
        });
    }
    Gpt {
        disk_guid: disk_guid.expect("no label-id in the dump"),
        partitions,
        used_backup: false,
    }
}

/// A GUID from its canonical text form.
pub fn guid(text: &str) -> Guid {
    let parts: Vec<&str> = text.split('-').collect();
    assert_eq!(parts.len(), 5, "{text}");
    let hex = |s: &str| u64::from_str_radix(s, 16).unwrap();
    let tail = hex(parts[3]) << 48 | hex(parts[4]);
    Guid::from_fields(
        hex(parts[0]) as u32,
        hex(parts[1]) as u16,
        hex(parts[2]) as u16,
        tail.to_be_bytes(),
    )
}
