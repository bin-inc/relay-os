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
    require_tool("readelf", "binutils")?;
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}

/// Fails, naming the tool and the package it comes in, if `program`
/// cannot be started, so a missing tool is not blamed on what it checks.
fn require_tool(program: &str, package: &str) -> Result<()> {
    match Command::new(program).arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) => bail!("cannot run {program} ({e}): install {package}"),
    }
}

/// What `readelf` says about a program's ELF header and program headers.
/// `readelf` (binutils) reads the ELF independently of our own code;
/// `LC_ALL=C` keeps its labels in English.
fn readelf(path: &Path) -> Result<String> {
    run_stdout(
        Command::new("readelf")
            .env("LC_ALL", "C")
            .args(["-hlW"])
            .arg(path),
    )
}

/// One row of `readelf -l`'s program headers.
struct Segment {
    kind: String,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    flags: String,
}

/// The rows between `Program Headers:` and the blank line after them:
/// `Type Offset VirtAddr PhysAddr FileSiz MemSiz Flg Align`, where the
/// flags are one to three words (`R E`).
fn segments(text: &str) -> Result<Vec<Segment>> {
    let mut rows = text
        .lines()
        .skip_while(|l| !l.starts_with("Program Headers:"))
        .skip(2)
        .take_while(|l| !l.trim().is_empty());
    let mut out = Vec::new();
    for line in rows.by_ref() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first().is_some_and(|w| w.starts_with('[')) {
            continue; // "[Requesting program interpreter: …]"
        }
        ensure!(words.len() >= 8, "unexpected readelf line {line:?}");
        out.push(Segment {
            kind: words[0].to_string(),
            offset: hex(words[1])?,
            vaddr: hex(words[2])?,
            filesz: hex(words[4])?,
            memsz: hex(words[5])?,
            flags: words[6..words.len() - 1].concat(),
        });
    }
    Ok(out)
}

/// The ABI version in the `Relay` note of `notes` (the bytes of a
/// `PT_NOTE` segment), if it has one: each note is `namesz`, `descsz`,
/// `type`, then the name and the descriptor, each padded to 4 bytes.
fn relay_note(notes: &[u8]) -> Result<Option<u32>> {
    let u32_at = |at: usize| -> Option<u32> {
        notes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let mut at = 0;
    while at + 12 <= notes.len() {
        let (namesz, descsz, kind) = (
            u32_at(at).unwrap() as usize,
            u32_at(at + 4).unwrap() as usize,
            u32_at(at + 8).unwrap(),
        );
        let name_at = at + 12;
        let desc_at = name_at + namesz.next_multiple_of(4);
        let next = desc_at + descsz.next_multiple_of(4);
        ensure!(next <= notes.len(), "a note runs past its segment");
        let mut name = &notes[name_at..name_at + namesz];
        name = name.strip_suffix(&[0]).unwrap_or(name);
        if name == relay_abi::NOTE_NAME {
            ensure!(
                kind == relay_abi::NOTE_TYPE,
                "the Relay note has type {kind}, not {}",
                relay_abi::NOTE_TYPE
            );
            ensure!(descsz == 4, "the Relay note holds {descsz} bytes, not 4");
            return Ok(u32_at(desc_at));
        }
        at = next;
    }
    Ok(None)
}

/// Checks the rules of spec §5.2 that the build decides, as the kernel will
/// read the program: a static x86_64 executable with only `PT_LOAD`,
/// `PT_NOTE` and `PT_GNU_STACK` program headers; loadable segments in the
/// program area, not overlapping, with offsets and addresses congruent
/// modulo 4 KiB, no more file than memory, never writable and executable;
/// the entry point in an executable segment; and a `PT_NOTE` segment
/// holding the `Relay` note with this ABI's version.
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
    let segments = segments(&text)?;
    for s in &segments {
        ensure!(
            matches!(s.kind.as_str(), "LOAD" | "NOTE" | "GNU_STACK"),
            "has a {} segment; only LOAD, NOTE and GNU_STACK are allowed",
            s.kind
        );
    }
    let mut loads: Vec<&Segment> = segments.iter().filter(|s| s.kind == "LOAD").collect();
    ensure!(!loads.is_empty(), "no loadable segment");
    loads.sort_by_key(|s| s.vaddr);
    let mut entry_ok = false;
    let mut previous_end = 0;
    for s in &loads {
        let (vaddr, flags) = (s.vaddr, &s.flags);
        let end = vaddr
            .checked_add(s.memsz)
            .context("segment end overflows")?;
        ensure!(
            vaddr >= PROGRAM_BASE && end <= PROGRAM_END,
            "segment {vaddr:#x}..{end:#x} is outside {PROGRAM_BASE:#x}..{PROGRAM_END:#x}"
        );
        ensure!(
            !(flags.contains('W') && flags.contains('E')),
            "segment at {vaddr:#x} is writable and executable"
        );
        ensure!(
            s.filesz <= s.memsz,
            "segment at {vaddr:#x} has more file ({:#x}) than memory ({:#x})",
            s.filesz,
            s.memsz
        );
        ensure!(
            s.offset % 4096 == vaddr % 4096,
            "segment at {vaddr:#x}: offset {:#x} and address are not congruent modulo 4 KiB",
            s.offset
        );
        ensure!(
            vaddr & !0xFFF >= previous_end,
            "segment at {vaddr:#x} overlaps the one before it (pages up to {previous_end:#x})"
        );
        previous_end = end.next_multiple_of(4096);
        if flags.contains('E') && (vaddr..end).contains(&entry) {
            entry_ok = true;
        }
    }
    ensure!(
        entry_ok,
        "entry point {entry:#x} is not in an executable segment"
    );
    let file = fs::read(path)?;
    let mut abi = None;
    for s in segments.iter().filter(|s| s.kind == "NOTE") {
        let notes = usize::try_from(s.offset)
            .ok()
            .zip(usize::try_from(s.filesz).ok())
            .and_then(|(o, n)| file.get(o..o.checked_add(n)?))
            .context("a PT_NOTE segment lies outside the file")?;
        if let Some(v) = relay_note(notes)? {
            abi = Some(v);
        }
    }
    let abi = abi.context("no PT_NOTE segment holds the Relay note")?;
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

/// `image` with its programs under ABI version `abi` instead (the e2e step
/// `system-abi`).
pub fn with_abi(image: &[u8], abi: u32) -> Result<Vec<u8>> {
    let archive = sysimg::Archive::parse(image).map_err(|e| anyhow::anyhow!("system.img: {e}"))?;
    let entries: Vec<sysimg::Entry<'_>> = archive.entries().collect();
    sysimg::write(abi, archive.build_time(), &entries)
        .map_err(|e| anyhow::anyhow!("system.img: {e}"))
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
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
    }

    /// The file offset of the `n`th program header of type `kind`.
    fn phdr(b: &[u8], kind: u32, n: usize) -> usize {
        let phoff = u64_at(b, 0x20) as usize;
        (0..16)
            .map(|i| phoff + i * 56)
            .filter(|&at| b[at..at + 4] == kind.to_le_bytes())
            .nth(n)
            .unwrap()
    }

    const PT_NOTE: u32 = 4;

    #[test]
    fn the_kernel_s_segment_rules_are_the_build_s() {
        let set_type = |kind: u32| {
            move |b: &mut Vec<u8>| {
                let at = phdr(b, PT_NOTE, 0);
                b[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            }
        };
        let e = patched("null", set_type(0));
        assert!(
            e.contains("has a NULL segment; only LOAD, NOTE and GNU_STACK"),
            "{e}"
        );
        let e = patched("relro", set_type(0x6474_e552));
        assert!(e.contains("has a GNU_RELRO segment"), "{e}");
        let e = patched("phdr", set_type(6));
        assert!(e.contains("has a PHDR segment"), "{e}");
        // The note's section is still there, but no PT_NOTE holds it: the
        // kernel reads segments, not sections.
        let e = patched("no-note-segment", set_type(0x6474_e551));
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
        let e = patched("note-type", |b| {
            let at = b.windows(8).position(|w| w == b"Relay\0\0\0").unwrap();
            b[at - 4] = 2;
        });
        assert!(e.contains("the Relay note has type 2, not 1"), "{e}");
        let e = patched("overlap", |b| {
            let (first, second) = (phdr(b, 1, 0), phdr(b, 1, 1));
            let vaddr = u64_at(b, first + 16);
            b[second + 16..second + 24].copy_from_slice(&vaddr.to_le_bytes());
        });
        assert!(e.contains("overlaps the one before it"), "{e}");
        let e = patched("congruence", |b| {
            let at = phdr(b, 1, 0) + 8;
            b[at..at + 8].copy_from_slice(&0x1800u64.to_le_bytes());
        });
        assert!(e.contains("are not congruent modulo 4 KiB"), "{e}");
        let e = patched("filesz", |b| {
            let at = phdr(b, 1, 0);
            let memsz = u64_at(b, at + 40);
            b[at + 32..at + 40].copy_from_slice(&(memsz + 1).to_le_bytes());
        });
        assert!(e.contains("has more file"), "{e}");
    }

    #[test]
    fn a_missing_tool_is_named() {
        require_tool("readelf", "binutils").unwrap();
        let e = require_tool("readelf-that-is-not-installed", "binutils")
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("cannot run readelf-that-is-not-installed ("),
            "{e}"
        );
        assert!(e.ends_with("): install binutils"), "{e}");
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
