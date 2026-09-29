//! The user programs (spec §8 of the user-space gate): building the
//! packages under `userland/` for Relay OS, checking that every binary is
//! a program the kernel will load (spec §5.2), and packing them into
//! `system.img` (spec §4.2).

use crate::config::{KERNEL_TARGET, USER_PACKAGES, USER_PROFILE};
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lowest and end of the addresses a program's segments may use (spec
/// §5.1): above the unmapped first 4 MiB, below the `mem_map` area.
pub const PROGRAM_BASE: u64 = 0x40_0000;
pub const PROGRAM_END: u64 = 0x1000_0000_0000;

/// One built program: its name in `/bin` and the binary.
#[derive(Clone, Debug)]
pub struct Program {
    pub name: String,
    pub path: PathBuf,
}

/// Builds every package of `USER_PACKAGES` and returns its binaries,
/// sorted by name. They have their own target directory, so a build from
/// inside `cargo test` does not wait for the outer build's lock.
pub fn build() -> Result<Vec<Program>> {
    let mut programs = Vec::new();
    for package in USER_PACKAGES {
        let out = cargo()
            .args([
                "build",
                "--profile",
                USER_PROFILE,
                "--package",
                package,
                "--target",
                KERNEL_TARGET,
                "--target-dir",
            ])
            .arg(root().join("target").join("user"))
            .args(["--message-format", "json-render-diagnostics"])
            .stderr(Stdio::inherit())
            .output()?;
        ensure!(out.status.success(), "building {package} failed");
        for line in String::from_utf8(out.stdout)?.lines() {
            let msg: serde_json::Value = serde_json::from_str(line)?;
            if msg["reason"] == "compiler-artifact"
                && let Some(exe) = msg["executable"].as_str()
            {
                let path = PathBuf::from(exe);
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                programs.push(Program { name, path });
            }
        }
    }
    programs.sort_by(|a, b| a.name.cmp(&b.name));
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}

/// What `readelf` says about a program. `readelf` (binutils) reads the ELF
/// independently of our own code.
fn readelf(path: &Path) -> Result<String> {
    run_stdout(Command::new("readelf").args(["-hlnW"]).arg(path))
}

/// Checks the rules of spec §5.2 that the build decides: a static x86_64
/// executable whose loadable segments lie in the program area and are never
/// both writable and executable, whose entry point is in an executable
/// segment, and whose `Relay` note holds this ABI's version.
pub fn check_program(path: &Path) -> Result<()> {
    let text = readelf(path)?;
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    ensure!(
        field("Type:").starts_with("EXEC"),
        "type {}, not EXEC",
        field("Type:")
    );
    ensure!(
        field("Machine:").contains("X86-64"),
        "machine {}",
        field("Machine:")
    );
    let entry = hex(&field("Entry point address:"))?;
    let mut loads = 0;
    let mut entry_ok = false;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first() {
            Some(&"LOAD") => {}
            Some(&"DYNAMIC") | Some(&"INTERP") | Some(&"TLS") => {
                bail!("has a {} segment", words[0])
            }
            _ => continue,
        }
        // LOAD offset vaddr paddr filesz memsz flags... align
        ensure!(words.len() >= 8, "unexpected readelf line {line:?}");
        let vaddr = hex(words[2])?;
        let memsz = hex(words[5])?;
        let flags: String = words[6..words.len() - 1].concat();
        let end = vaddr.checked_add(memsz).context("segment end overflows")?;
        ensure!(
            vaddr >= PROGRAM_BASE && end <= PROGRAM_END,
            "segment {vaddr:#x}..{end:#x} is outside {PROGRAM_BASE:#x}..{PROGRAM_END:#x}"
        );
        ensure!(
            !(flags.contains('W') && flags.contains('E')),
            "segment at {vaddr:#x} is writable and executable"
        );
        if flags.contains('E') && (vaddr..end).contains(&entry) {
            entry_ok = true;
        }
        loads += 1;
    }
    ensure!(loads > 0, "no loadable segment");
    ensure!(
        entry_ok,
        "entry point {entry:#x} is not in an executable segment"
    );
    let note = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Relay ") && l.contains("description data:"))
        .context("no Relay note")?;
    let data: Vec<u8> = note
        .split("description data:")
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<_, _>>()?;
    ensure!(
        data.len() == 4,
        "the Relay note holds {} bytes, not 4",
        data.len()
    );
    let abi = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ensure!(
        abi == relay_abi::VERSION,
        "built for ABI {abi}, this is ABI {}",
        relay_abi::VERSION
    );
    Ok(())
}

fn hex(s: &str) -> Result<u64> {
    let digits = s
        .strip_prefix("0x")
        .with_context(|| format!("{s:?} is not hex"))?;
    Ok(u64::from_str_radix(digits, 16)?)
}

/// `system.img` for these programs: every one executable (0755), with the
/// time of the build as their file times.
pub fn system_image(programs: &[Program]) -> Result<Vec<u8>> {
    let data: Vec<Vec<u8>> = programs
        .iter()
        .map(|p| fs::read(&p.path))
        .collect::<Result<_, _>>()?;
    let entries: Vec<sysimg::Entry<'_>> = programs
        .iter()
        .zip(&data)
        .map(|(p, d)| sysimg::Entry {
            name: p.name.as_bytes(),
            mode: 0o755,
            data: d,
        })
        .collect();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    sysimg::write(relay_abi::VERSION, now, &entries).map_err(|e| anyhow::anyhow!("system.img: {e}"))
}

/// Builds the programs and writes `target/relay/system.img`.
pub fn build_system_image() -> Result<PathBuf> {
    let image = system_image(&build()?)?;
    fs::create_dir_all(out_dir())?;
    let path = out_dir().join("system.img");
    fs::write(&path, image)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t_args() -> Program {
        build()
            .unwrap()
            .into_iter()
            .find(|p| p.name == "t-args")
            .expect("t-args is built")
    }

    #[test]
    fn every_program_is_built_for_ring_3() {
        let programs = build().unwrap();
        assert!(programs.iter().any(|p| p.name == "t-args"));
        for p in &programs {
            check_program(&p.path).unwrap();
            let text = readelf(&p.path).unwrap();
            assert!(
                text.contains("Entry point address:               0x4"),
                "{}",
                p.name
            );
        }
    }

    /// `t-args` with one ELF field changed, and what `check_program` says.
    fn patched(name: &str, edit: impl Fn(&mut Vec<u8>)) -> String {
        let mut bytes = fs::read(t_args().path).unwrap();
        edit(&mut bytes);
        let dir = out_dir().join("xtask-tests");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, &bytes).unwrap();
        match check_program(&path) {
            Ok(()) => "accepted".into(),
            Err(e) => e.to_string(),
        }
    }

    fn u64_at(b: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
    }

    /// The file offset of the first `PT_LOAD` program header (ELF64:
    /// `e_phoff` at 0x20, 56-byte entries, `p_type` 1).
    fn first_load(b: &[u8]) -> usize {
        let phoff = u64_at(b, 0x20) as usize;
        (0..16)
            .map(|i| phoff + i * 56)
            .find(|&at| b[at..at + 4] == 1u32.to_le_bytes())
            .unwrap()
    }

    #[test]
    fn check_program_refuses_what_the_kernel_would() {
        assert_eq!(patched("same", |_| {}), "accepted");
        let host = check_program(Path::new("/bin/true"))
            .unwrap_err()
            .to_string();
        assert!(host.contains("not EXEC"), "a Linux program: {host}");
        let e = patched("dyn", |b| b[0x10] = 3);
        assert!(e.contains("type DYN"), "{e}");
        let e = patched("rwx", |b| {
            let at = first_load(b);
            b[at + 4] |= 2;
        });
        assert!(e.contains("writable and executable"), "{e}");
        let e = patched("low", |b| {
            let at = first_load(b) + 16;
            b[at..at + 8].copy_from_slice(&0x1000u64.to_le_bytes());
        });
        assert!(e.contains("is outside 0x400000..0x100000000000"), "{e}");
        let e = patched("entry", |b| {
            b[0x18..0x20].copy_from_slice(&0x7000_0000u64.to_le_bytes())
        });
        assert!(e.contains("is not in an executable segment"), "{e}");
        let e = patched("dynamic", |b| {
            let at = first_load(b);
            b[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
        });
        assert!(e.contains("has a DYNAMIC segment"), "{e}");
        let e = patched("abi", |b| {
            let mut note = b"Relay\0\0\0".to_vec();
            note.extend_from_slice(&relay_abi::VERSION.to_le_bytes());
            let at = b
                .windows(note.len())
                .position(|w| w == note)
                .expect("the note");
            b[at + 8] = b[at + 8].wrapping_add(1);
        });
        assert!(
            e.contains(&format!("built for ABI {}", relay_abi::VERSION + 1)),
            "{e}"
        );
        let e = patched("no-note", |b| {
            let at = b.windows(8).position(|w| w == b"Relay\0\0\0").unwrap();
            b[at] = b'X';
        });
        assert!(e.contains("no Relay note"), "{e}");
    }

    #[test]
    fn the_system_image_holds_every_program() {
        let programs = build().unwrap();
        let image = system_image(&programs).unwrap();
        let archive = sysimg::Archive::parse(&image).unwrap();
        assert_eq!(archive.abi(), relay_abi::VERSION);
        assert_eq!(archive.len(), programs.len());
        let t = archive.entry(archive.find(b"t-args").unwrap()).unwrap();
        assert_eq!(t.mode, 0o755);
        assert_eq!(t.data, fs::read(t_args().path).unwrap());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(archive.build_time().abs_diff(now) < 600);
    }
}
