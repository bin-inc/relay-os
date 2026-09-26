//! End-to-end scenarios: boot the full image in QEMU and drive it over the
//! serial port and QMP.
//!
//! Scenario files (`tests/e2e/*.txt`) contain one step per line:
//!
//! ```text
//! # comment
//! cmdline test=1 panic=pagefault   (before any other step; default "test=1")
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
//! timeout 20                       (seconds, for the following expects)
//! expect <regex>                   (waits for serial output, ANSI stripped)
//! send <text>                      (types <text> + Enter over serial)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
//! ```
//!
//! QEMU is killed at the end of every scenario.

use crate::build;
use crate::image::{self, Layout, esp_write, set_cmdline};
use crate::qemu::Qemu;
use crate::qmp::Qmp;
use crate::util::{out_dir, root};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_CMDLINE: &str = "test=1";

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Timeout(u64),
    Expect(String),
    Send(String),
    ScreenshotNonblank,
}

#[derive(Debug, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub cmdline: String,
    /// ESP files to overwrite before booting: (path, contents).
    pub esp_writes: Vec<(String, String)>,
    /// (line number, step)
    pub steps: Vec<(usize, Step)>,
}

pub fn parse_scenario(name: &str, text: &str) -> Result<Scenario> {
    let mut cmdline = DEFAULT_CMDLINE.to_string();
    let mut esp_writes = Vec::new();
    let mut steps = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let step = match word {
            "cmdline" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: cmdline must come before other steps");
                }
                cmdline = rest.to_string();
                continue;
            }
            "esp-write" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: esp-write must come before other steps");
                }
                let (path, contents) = rest.split_once(' ').unwrap_or((rest, ""));
                if !path.starts_with('/') {
                    bail!("{name}:{line_no}: esp-write path must be absolute");
                }
                esp_writes.push((path.to_string(), contents.to_string()));
                continue;
            }
            "timeout" => Step::Timeout(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "expect" => {
                Regex::new(rest).with_context(|| format!("{name}:{line_no}: bad regex"))?;
                Step::Expect(rest.to_string())
            }
            "send" => Step::Send(rest.to_string()),
            "screenshot-nonblank" => Step::ScreenshotNonblank,
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
        };
        steps.push((line_no, step));
    }
    Ok(Scenario {
        name: name.to_string(),
        cmdline,
        esp_writes,
        steps,
    })
}

/// Removes ANSI escape sequences and carriage returns.
pub fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[@-Z\\-_]|\r").unwrap();
    re.replace_all(s, "").into_owned()
}

/// True if the first `rows` pixel rows of a binary PPM (P6) contain at least
/// two different colours.
pub fn ppm_top_is_nonblank(ppm: &[u8], rows: usize) -> Result<bool> {
    // Header: "P6" whitespace width whitespace height whitespace maxval single-whitespace.
    let mut fields = Vec::new();
    let mut pos = 0;
    while fields.len() < 4 {
        while pos < ppm.len() && ppm[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while pos < ppm.len() && !ppm[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if start == pos {
            bail!("truncated PPM header");
        }
        fields.push(std::str::from_utf8(&ppm[start..pos])?.to_string());
    }
    if fields[0] != "P6" || fields[3] != "255" {
        bail!("unsupported PPM {} maxval {}", fields[0], fields[3]);
    }
    let width: usize = fields[1].parse()?;
    let height: usize = fields[2].parse()?;
    let data = &ppm[pos + 1..];
    let n = width * rows.min(height) * 3;
    if data.len() < n {
        bail!("truncated PPM data");
    }
    let (pixels, _) = data[..n].as_chunks::<3>();
    Ok(pixels.iter().any(|px| px != &pixels[0]))
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    serial: Arc<Mutex<Vec<u8>>>,
    qmp: Qmp,
    /// Offset in the stripped serial text up to which output was consumed.
    consumed: usize,
}

impl Running {
    fn text(&self) -> String {
        strip_ansi(&String::from_utf8_lossy(&self.serial.lock().unwrap()))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(image: &Path, layout: &Layout, scenario: &Scenario, run_dir: &Path) -> Result<Running> {
    let mut q = Qemu::prepare(image, run_dir)?;
    set_cmdline(&q.disk, layout.esp, &scenario.cmdline, run_dir)?;
    for (path, contents) in &scenario.esp_writes {
        esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?;
    }
    let sock = run_dir.join("qmp.sock");
    let _ = fs::remove_file(&sock);
    q.headless = true;
    q.qmp_socket = Some(sock.clone());
    let mut child = q
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(run_dir.join("qemu.stderr"))?)
        .spawn()
        .context("starting qemu-system-x86_64")?;
    let stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let serial = Arc::new(Mutex::new(Vec::new()));
    let sink = serial.clone();
    let mut log = fs::File::create(run_dir.join("serial.log"))?;
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
            let _ = log.write_all(&buf[..n]);
        }
    });
    let qmp = Qmp::connect(&sock, Duration::from_secs(10))?;
    Ok(Running {
        child,
        stdin,
        serial,
        qmp,
        consumed: 0,
    })
}

fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    match step {
        Step::Timeout(s) => *timeout = Duration::from_secs(*s),
        Step::Expect(pattern) => {
            let re = Regex::new(pattern)?;
            let deadline = Instant::now() + *timeout;
            loop {
                let text = r.text();
                if let Some(m) = re.find(&text[r.consumed.min(text.len())..]) {
                    r.consumed += m.end();
                    return Ok(());
                }
                if let Ok(Some(status)) = r.child.try_wait() {
                    bail!("QEMU exited ({status}) while waiting for /{pattern}/");
                }
                if Instant::now() > deadline {
                    bail!("timed out after {timeout:?} waiting for /{pattern}/");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Step::Send(text) => {
            r.stdin.write_all(text.as_bytes())?;
            r.stdin.write_all(b"\r")?;
            r.stdin.flush()?;
        }
        Step::ScreenshotNonblank => {
            let file = run_dir.join("screen.ppm");
            r.qmp
                .execute("screendump", serde_json::json!({ "filename": file }))?;
            let ppm = fs::read(&file)?;
            if !ppm_top_is_nonblank(&ppm, 64)? {
                bail!("screenshot {} is blank in its top rows", file.display());
            }
        }
    }
    Ok(())
}

fn tail(text: &str, lines: usize) -> String {
    let v: Vec<&str> = text.lines().collect();
    v[v.len().saturating_sub(lines)..].join("\n")
}

pub fn run_scenario(image: &Path, layout: &Layout, scenario: &Scenario) -> Result<()> {
    let run_dir = out_dir().join("e2e").join(&scenario.name);
    let mut r = start(image, layout, scenario, &run_dir)?;
    let mut timeout = Duration::from_secs(20);
    for (line, step) in &scenario.steps {
        if let Err(e) = run_step(&mut r, step, &mut timeout, &run_dir) {
            bail!(
                "scenario '{}' failed at line {line} ({step:?}): {e:#}\n--- last serial output ---\n{}\n--- full log: {} ---",
                scenario.name,
                tail(&r.text(), 40),
                run_dir.join("serial.log").display()
            );
        }
    }
    Ok(())
}

pub fn load_scenarios(only: Option<&str>) -> Result<Vec<Scenario>> {
    let dir = root().join("tests/e2e");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    files.retain(|p| p.extension().is_some_and(|e| e == "txt"));
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        if only.is_some_and(|o| o != name) {
            continue;
        }
        out.push(parse_scenario(&name, &fs::read_to_string(&f)?)?);
    }
    if out.is_empty() {
        bail!("no scenarios matched in {}", dir.display());
    }
    Ok(out)
}

/// Builds the image and runs every scenario (or only `only`).
pub fn run_all(only: Option<&str>) -> Result<()> {
    let scenarios = load_scenarios(only)?;
    let art = build::build()?;
    let img = image::build_image(&art, DEFAULT_CMDLINE)?;
    let layout = image::read_layout(&img)?;
    for s in &scenarios {
        println!("== e2e {}", s.name);
        run_scenario(&img, &layout, s)?;
    }
    println!("all {} scenario(s) passed", scenarios.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps_and_cmdline() {
        let s = parse_scenario(
            "x",
            "# hi\ncmdline test=1 panic=ud\n\ntimeout 5\nexpect \\[ ok \\] cpu\nsend ls -l\nscreenshot-nonblank\n",
        )
        .unwrap();
        assert_eq!(s.cmdline, "test=1 panic=ud");
        assert_eq!(
            s.steps,
            vec![
                (4, Step::Timeout(5)),
                (5, Step::Expect("\\[ ok \\] cpu".into())),
                (6, Step::Send("ls -l".into())),
                (7, Step::ScreenshotNonblank),
            ]
        );
    }

    #[test]
    fn parses_esp_writes() {
        let s =
            parse_scenario("x", "esp-write /EFI/RELAY/kernel.elf not an elf\nexpect x").unwrap();
        assert_eq!(
            s.esp_writes,
            vec![("/EFI/RELAY/kernel.elf".into(), "not an elf".into())]
        );
        assert!(parse_scenario("x", "esp-write relative x").is_err());
        assert!(parse_scenario("x", "expect a\nesp-write /x y").is_err());
    }

    #[test]
    fn default_cmdline_is_test_mode() {
        assert_eq!(parse_scenario("x", "expect a").unwrap().cmdline, "test=1");
    }

    #[test]
    fn rejects_unknown_steps_bad_regex_and_late_cmdline() {
        assert!(parse_scenario("x", "frobnicate").is_err());
        assert!(parse_scenario("x", "expect (").is_err());
        assert!(parse_scenario("x", "expect a\ncmdline test=1").is_err());
    }

    #[test]
    fn strips_colour_codes_and_cr() {
        assert_eq!(strip_ansi("[\x1b[32m ok \x1b[0m] cpu\r\n"), "[ ok ] cpu\n");
    }

    #[test]
    fn workflow_runs_the_scenarios() {
        let workflow = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
        assert!(workflow.contains("run: cargo xtask test --e2e-only"));
        assert!(
            workflow.contains("path: target/relay/e2e/"),
            "logs are uploaded on failure"
        );
    }

    #[test]
    fn ppm_blank_detection() {
        let mut blank = b"P6\n4 2\n255\n".to_vec();
        blank.extend(std::iter::repeat_n(0u8, 4 * 2 * 3));
        assert!(!ppm_top_is_nonblank(&blank, 64).unwrap());
        let mut text = blank.clone();
        let n = text.len();
        text[n - 1] = 0xFF;
        assert!(ppm_top_is_nonblank(&text, 64).unwrap());
        assert!(
            !ppm_top_is_nonblank(&text, 1).unwrap(),
            "only row 0 considered"
        );
        assert!(ppm_top_is_nonblank(b"P5\n1 1\n255\n\0", 1).is_err());
    }
}
