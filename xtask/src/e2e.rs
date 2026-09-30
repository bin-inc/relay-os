//! End-to-end scenarios: boot the full image in QEMU and drive it over the
//! serial port and QMP.
//!
//! Scenario files (`tests/e2e/*.txt`) contain one step per line:
//!
//! ```text
//! # comment
//! cmdline test=1 panic=pagefault   (before any other step; default "test=1")
//! disk small                       (before any other step: the 32 MiB root)
//! break-root                       (before any other step: the root's ext2
//!                                   magic is zeroed until the scenario ends)
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
//! esp-delete /EFI/RELAY/system.img  (before boot: remove an ESP file)
//! system-abi 99                    (before boot: system.img, rewritten with
//!                                   another ABI version)
//! timeout 20                       (seconds, for the following expects)
//! expect <regex>                   (waits for serial output, ANSI stripped)
//! expect-same <name> <regex>       (as expect; the regex's first group must
//!                                   capture what it did the first time a
//!                                   step of that name matched)
//! send <text>                      (types <text> + Enter over serial)
//! key <text>                       (types <text> + Enter on the USB keyboard,
//!                                   QMP send-key; {up}, {ctrl-c}: see keys.rs)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
//! alive 12                         (fails if QEMU exits within 12 seconds)
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! reboot [<regex>]                 (types `reboot`; QEMU must exit as after a
//!                                   reset, having printed <regex> first, then
//!                                   starts again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
//!                                   isa-debug-exit, test mode's power-off)
//! unplug                           (pulls the USB stick out: QMP device_del,
//!                                   then QEMU's DEVICE_DELETED event)
//! check-script /root/checks/a.sh   (after poweroff: the script's transcript,
//!                                   read with debugfs, shows what the script
//!                                   expects on QEMU; see checks.rs)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
//!                                   debugfs, is "abc\n" repeated to 8388608
//!                                   bytes)
//! ```
//!
//! QEMU is killed at the end of every scenario, and `e2fsck -fn` must find
//! the ext2 root on the disk it leaves clean (spec §9.3).

use crate::build;
use crate::checks;
use crate::image::{self, Layout, Partition, esp_delete, esp_write, set_cmdline};
use crate::keys;
use crate::qemu::{self, Qemu};
use crate::qmp::Qmp;
use crate::userland;
use crate::util::{out_dir, root};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const DEFAULT_CMDLINE: &str = "test=1";
/// How long each `key` press is held, in milliseconds: long enough for
/// the guest to poll the keyboard (every 8 ms), far below the 500 ms
/// repeat delay.
const KEY_HOLD_MS: u64 = 30;
/// Time for the guest to see the last release after QEMU has played it.
const KEY_SETTLE_MS: u64 = 100;
/// QEMU's exit status after a guest reset under `-no-reboot`.
pub const EXIT_RESET: i32 = 0;
/// QEMU's exit status after test mode's `poweroff` wrote 0x10 to
/// `isa-debug-exit`: (0x10 << 1) | 1.
pub const EXIT_POWEROFF: i32 = 33;

/// How long QEMU takes to play `presses` presses: it queues them and holds
/// each for `KEY_HOLD_MS`. The `key` step waits that long, so its Enter
/// cannot land in the middle of what a following `send` types.
pub fn typing_time(presses: usize) -> Duration {
    Duration::from_millis(KEY_HOLD_MS * presses as u64 + KEY_SETTLE_MS)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Timeout(u64),
    Expect(String),
    /// As `Expect`; the first group must capture what it captured the first
    /// time a step with this name matched (free memory before and after).
    ExpectSame {
        name: String,
        pattern: String,
    },
    Send(String),
    /// Text typed on the emulated USB keyboard, then Enter.
    Key(String),
    /// Text typed on the emulated USB keyboard, and nothing after it.
    Type(String),
    ScreenshotNonblank,
    /// QEMU must still be running after this many seconds (for example after
    /// a loader error, which must not power the machine off).
    Alive(u64),
    /// Pixel (x, y) of a fresh screenshot must have this RGB colour.
    ScreenshotPixel {
        x: usize,
        y: usize,
        rgb: [u8; 3],
    },
    /// Restart the machine and boot again on the same disk; the pattern
    /// must appear in what the machine printed before it went down.
    Reboot(Option<String>),
    /// Switch the machine off; no later step talks to it.
    Poweroff,
    /// Pull the USB stick out (QMP `device_del`), waiting until QEMU has
    /// removed it.
    Unplug,
    /// The transcript `sh` wrote for the script at this path on the ext2
    /// root shows what the script expects (`checks.rs`). Checked on the
    /// disk once the machine is off.
    CheckScript(String),
    /// The file at `path` on the ext2 root is `line` and a newline, again
    /// and again, `bytes` in all. Checked on the disk once the machine is
    /// off.
    FileLines {
        path: String,
        bytes: usize,
        line: String,
    },
}

/// A change to the ESP before booting.
#[derive(Debug, PartialEq)]
pub enum EspEdit {
    /// `esp-write`: the file at this path gets these contents.
    Write(String, String),
    /// `esp-delete`: the file at this path is removed.
    Delete(String),
    /// `system-abi`: `system.img` holds the same programs under another
    /// ABI version.
    SystemAbi(u32),
}

#[derive(Debug, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub cmdline: String,
    /// `disk small`: boot the image with the 32 MiB root.
    pub small_disk: bool,
    /// `break-root`: the root filesystem is unrecognisable while the
    /// scenario runs.
    pub break_root: bool,
    /// Changes to the ESP before booting, in order.
    pub esp_edits: Vec<EspEdit>,
    /// (line number, step)
    pub steps: Vec<(usize, Step)>,
}

pub fn parse_scenario(name: &str, text: &str) -> Result<Scenario> {
    let mut cmdline = DEFAULT_CMDLINE.to_string();
    let mut small_disk = false;
    let mut break_root = false;
    let mut esp_edits = Vec::new();
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
            "disk" => {
                if !steps.is_empty() || rest != "small" {
                    bail!("{name}:{line_no}: expected `disk small` before other steps");
                }
                small_disk = true;
                continue;
            }
            "break-root" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: break-root must come before other steps");
                }
                break_root = true;
                continue;
            }
            "esp-write" | "esp-delete" | "system-abi" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: {word} must come before other steps");
                }
                let edit =
                    match word {
                        "system-abi" => EspEdit::SystemAbi(rest.parse().with_context(|| {
                            format!("{name}:{line_no}: system-abi needs a number")
                        })?),
                        _ => {
                            let (path, contents) = rest.split_once(' ').unwrap_or((rest, ""));
                            if !path.starts_with('/') {
                                bail!("{name}:{line_no}: {word} path must be absolute");
                            }
                            if word == "esp-write" {
                                EspEdit::Write(path.to_string(), contents.to_string())
                            } else {
                                EspEdit::Delete(path.to_string())
                            }
                        }
                    };
                esp_edits.push(edit);
                continue;
            }
            "timeout" => Step::Timeout(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "expect" => {
                Regex::new(rest).with_context(|| format!("{name}:{line_no}: bad regex"))?;
                Step::Expect(rest.to_string())
            }
            "expect-same" => {
                parse_expect_same(rest).with_context(|| format!("{name}:{line_no}"))?
            }
            "send" => Step::Send(rest.to_string()),
            "key" => {
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
            }
            "type" => {
                keys::typed(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Type(rest.to_string())
            }
            "screenshot-nonblank" => Step::ScreenshotNonblank,
            "screenshot-pixel" => {
                parse_pixel_step(rest).with_context(|| format!("{name}:{line_no}"))?
            }
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "reboot" if rest.is_empty() => Step::Reboot(None),
            "reboot" => {
                Regex::new(rest).with_context(|| format!("{name}:{line_no}: bad regex"))?;
                Step::Reboot(Some(rest.to_string()))
            }
            "poweroff" => Step::Poweroff,
            "unplug" if rest.is_empty() => Step::Unplug,
            "check-script" if rest.starts_with('/') => Step::CheckScript(rest.to_string()),
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
        };
        steps.push((line_no, step));
    }
    Ok(Scenario {
        name: name.to_string(),
        cmdline,
        small_disk,
        break_root,
        esp_edits,
        steps,
    })
}

fn parse_file_lines(rest: &str) -> Result<Step> {
    let mut parts = rest.splitn(3, ' ');
    let (Some(path), Some(bytes), Some(line)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("expected: file-lines <path> <bytes> <line>");
    };
    let bytes: usize = bytes.parse().context("bytes")?;
    if !path.starts_with('/') || line.is_empty() || !bytes.is_multiple_of(line.len() + 1) {
        bail!("file-lines needs an absolute path, a line, and a size that is whole lines");
    }
    Ok(Step::FileLines {
        path: path.to_string(),
        bytes,
        line: line.to_string(),
    })
}

/// Why `data` is not `line` and a newline repeated to `bytes` bytes.
pub fn lines_mismatch(data: &[u8], bytes: usize, line: &str) -> Option<String> {
    if data.len() != bytes {
        return Some(format!("is {} bytes, expected {bytes}", data.len()));
    }
    let unit = [line.as_bytes(), b"\n"].concat();
    let bad = data.chunks(unit.len()).position(|c| c != unit)?;
    Some(format!(
        "line {} (byte {}) is {:?}",
        bad + 1,
        bad * unit.len(),
        String::from_utf8_lossy(
            &data[bad * unit.len()..][..unit.len().min(data.len() - bad * unit.len())]
        )
    ))
}

fn parse_pixel_step(rest: &str) -> Result<Step> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let [x, y, colour] = parts[..] else {
        bail!("expected: screenshot-pixel <x> <y> <#rrggbb>");
    };
    let hex = colour
        .strip_prefix('#')
        .filter(|h| h.len() == 6)
        .context("colour must be #rrggbb")?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).context("colour must be #rrggbb");
    Ok(Step::ScreenshotPixel {
        x: x.parse()?,
        y: y.parse()?,
        rgb: [byte(0)?, byte(2)?, byte(4)?],
    })
}

/// Removes ANSI escape sequences and carriage returns.
pub fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[@-Z\\-_]|\r").unwrap();
    re.replace_all(s, "").into_owned()
}

/// Parses a binary PPM (P6, maxval 255) into `(width, height, pixel data)`.
fn parse_ppm(ppm: &[u8]) -> Result<(usize, usize, &[u8])> {
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
    let data = &ppm[(pos + 1).min(ppm.len())..];
    if data.len() < width * height * 3 {
        bail!("truncated PPM data");
    }
    Ok((width, height, data))
}

/// True if the first `rows` pixel rows of a binary PPM (P6) contain at least
/// two different colours.
pub fn ppm_top_is_nonblank(ppm: &[u8], rows: usize) -> Result<bool> {
    let (width, height, data) = parse_ppm(ppm)?;
    let n = width * rows.min(height) * 3;
    let (pixels, _) = data[..n].as_chunks::<3>();
    Ok(pixels.iter().any(|px| px != &pixels[0]))
}

/// The RGB value of pixel (x, y) of a binary PPM (P6).
pub fn ppm_pixel(ppm: &[u8], x: usize, y: usize) -> Result<[u8; 3]> {
    let (width, height, data) = parse_ppm(ppm)?;
    if x >= width || y >= height {
        bail!("pixel ({x}, {y}) is outside the {width}x{height} screenshot");
    }
    let o = (y * width + x) * 3;
    Ok([data[o], data[o + 1], data[o + 2]])
}

struct Running {
    /// The machine, kept to start it again after a reboot.
    qemu: Qemu,
    /// Where the ext2 root is on the disk.
    root: Partition,
    run_dir: PathBuf,
    child: Child,
    stdin: ChildStdin,
    serial: Arc<Mutex<Vec<u8>>>,
    /// Copies QEMU's serial output into `serial` and the log file until
    /// QEMU exits, then returns the log file.
    reader: Option<JoinHandle<fs::File>>,
    qmp: Qmp,
    /// Offset in the stripped serial text up to which output was consumed.
    consumed: usize,
    /// The machine switched itself off.
    off: bool,
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
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn start(image: &Path, layout: &Layout, scenario: &Scenario, run_dir: &Path) -> Result<Running> {
    let mut q = Qemu::prepare(image, run_dir)?;
    set_cmdline(&q.disk, layout.esp, &scenario.cmdline, run_dir)?;
    for edit in &scenario.esp_edits {
        match edit {
            EspEdit::Write(path, contents) => {
                esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?
            }
            EspEdit::Delete(path) => esp_delete(&q.disk, layout.esp, path)?,
            EspEdit::SystemAbi(abi) => {
                let image = fs::read(out_dir().join("system.img"))?;
                let other = userland::with_abi(&image, *abi)?;
                esp_write(
                    &q.disk,
                    layout.esp,
                    "/EFI/RELAY/system.img",
                    &other,
                    run_dir,
                )?;
            }
        }
    }
    if scenario.break_root {
        image::set_ext2_magic(&q.disk, layout.root, false)?;
    }
    q.headless = true;
    q.qmp = Some(run_dir.join("qmp.sock"));
    let log = fs::File::create(run_dir.join("serial.log"))?;
    launch(q, layout.root, run_dir, log)
}

/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, root: Partition, run_dir: &Path, log: fs::File) -> Result<Running> {
    let socket = q.qmp.clone().context("QMP socket")?;
    // A socket left by an earlier run would refuse QEMU's bind.
    let _ = fs::remove_file(&socket);
    let mut child = q
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(run_dir.join("qemu.stderr"))?)
        .spawn()
        .context("starting qemu-system-x86_64")?;
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let serial = Arc::new(Mutex::new(Vec::new()));
    let reader = read_serial(stdout, serial.clone(), log);
    let qmp = wait_for_qmp(
        &mut child,
        &socket,
        Duration::from_secs(10),
        &run_dir.join("qemu.stderr"),
    )?;
    Ok(Running {
        qemu: q,
        root,
        run_dir: run_dir.to_path_buf(),
        child,
        stdin,
        serial,
        reader: Some(reader),
        qmp,
        consumed: 0,
        off: false,
    })
}

/// Copies `stdout` into `serial` and `log` until it ends (QEMU exited),
/// then hands the log file back.
fn read_serial(
    mut stdout: impl Read + Send + 'static,
    serial: Arc<Mutex<Vec<u8>>>,
    mut log: fs::File,
) -> JoinHandle<fs::File> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            serial.lock().unwrap().extend_from_slice(&buf[..n]);
            let _ = log.write_all(&buf[..n]);
        }
        log
    })
}

/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean. Returns the serial log once everything QEMU printed is in it.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    r.stdin.write_all(command.as_bytes())?;
    r.stdin.write_all(b"\r")?;
    r.stdin.flush()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(s) = r.child.try_wait()? {
            // The pipe ends with QEMU, so this waits only for the last
            // bytes to be copied.
            let log = r
                .reader
                .take()
                .context("serial reader already joined")?
                .join()
                .map_err(|_| anyhow::anyhow!("the serial reader panicked"))?;
            if s.code() != Some(status) {
                bail!("QEMU exited with {s} after `{command}`, expected exit status {status}");
            }
            let state = image::ext2_state(&r.qemu.disk, r.root)?;
            if state != "clean" {
                bail!("after `{command}` the root filesystem is `{state}`, not `clean`");
            }
            return Ok(log);
        }
        if Instant::now() > deadline {
            bail!("QEMU still runs {timeout:?} after `{command}`");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `reboot`: the machine resets, and QEMU (`-no-reboot`) exits, after
/// printing `last` if given; then the same disk boots again, with the serial
/// log continued.
fn reboot(r: &mut Running, last: Option<&str>, timeout: Duration) -> Result<()> {
    let mut log = exit_with(r, "reboot", EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
        let text = r.text();
        if !Regex::new(pattern)?.is_match(&text[r.consumed.min(text.len())..]) {
            bail!("the machine restarted without printing /{pattern}/");
        }
    }
    log.write_all(b"\n--- e2e: reboot ---\n")?;
    let q = Qemu {
        disk: r.qemu.disk.clone(),
        vars: r.qemu.vars.clone(),
        headless: true,
        qmp: r.qemu.qmp.clone(),
    };
    let fresh = launch(q, r.root, &r.run_dir.clone(), log)?;
    *r = fresh;
    Ok(())
}

/// Connects to QEMU's QMP socket, giving up early if QEMU exits first (a bad
/// option, a missing firmware file): then the error shows QEMU's stderr
/// instead of a bare "Connection refused" after the full timeout.
fn wait_for_qmp(child: &mut Child, socket: &Path, timeout: Duration, stderr: &Path) -> Result<Qmp> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            let text = fs::read_to_string(stderr).unwrap_or_default();
            bail!(
                "QEMU exited ({status}) during start-up; its stderr ({}):\n{}",
                stderr.display(),
                text.trim_end()
            );
        }
        match Qmp::connect(socket, Duration::from_millis(200)) {
            Ok(q) => return Ok(q),
            Err(e) if Instant::now() > deadline => return Err(e),
            Err(_) => {}
        }
    }
}

/// `expect-same NAME REGEX`: a name without spaces, then a regex with at
/// least one group.
fn parse_expect_same(rest: &str) -> Result<Step> {
    let (name, pattern) = rest
        .split_once(' ')
        .context("expect-same needs a name and a regex")?;
    let re = Regex::new(pattern).context("bad regex")?;
    if re.captures_len() < 2 {
        bail!("expect-same's regex needs a group to compare");
    }
    Ok(Step::ExpectSame {
        name: name.to_string(),
        pattern: pattern.to_string(),
    })
}

/// Records `value` as what `name` captured, or checks it against what it
/// captured before.
fn same_as_before(seen: &mut HashMap<String, String>, name: &str, value: &str) -> Result<()> {
    match seen.get(name) {
        Some(first) if first != value => {
            bail!("{name} is {value}, but it was {first} the first time")
        }
        Some(_) => Ok(()),
        None => {
            seen.insert(name.to_string(), value.to_string());
            Ok(())
        }
    }
}

/// Waits until `re` matches the serial output after what earlier steps
/// consumed; its captures.
fn wait_for_match(r: &mut Running, re: &Regex, timeout: Duration) -> Result<Vec<String>> {
    let deadline = Instant::now() + timeout;
    loop {
        let text = r.text();
        let rest = &text[r.consumed.min(text.len())..];
        if let Some(c) = re.captures(rest) {
            let groups = c
                .iter()
                .map(|g| g.map_or(String::new(), |g| g.as_str().to_string()))
                .collect();
            r.consumed += c.get(0).unwrap().end();
            return Ok(groups);
        }
        if let Ok(Some(status)) = r.child.try_wait() {
            bail!("QEMU exited ({status}) while waiting for /{re}/");
        }
        if Instant::now() > deadline {
            bail!("timed out after {timeout:?} waiting for /{re}/");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_step(
    r: &mut Running,
    step: &Step,
    timeout: &mut Duration,
    run_dir: &Path,
    seen: &mut HashMap<String, String>,
) -> Result<()> {
    let offline = matches!(
        step,
        Step::Timeout(_) | Step::FileLines { .. } | Step::CheckScript(_)
    );
    if r.off && !offline {
        bail!("the machine was switched off by an earlier step");
    }
    match step {
        Step::Timeout(s) => *timeout = Duration::from_secs(*s),
        Step::Expect(pattern) => {
            wait_for_match(r, &Regex::new(pattern)?, *timeout)?;
        }
        Step::ExpectSame { name, pattern } => {
            let groups = wait_for_match(r, &Regex::new(pattern)?, *timeout)?;
            same_as_before(seen, name, &groups[1])?;
        }
        Step::Send(text) => {
            r.stdin.write_all(text.as_bytes())?;
            r.stdin.write_all(b"\r")?;
            r.stdin.flush()?;
        }
        Step::Key(_) | Step::Type(_) => {
            let presses = match step {
                Step::Key(text) => keys::presses(text)?,
                Step::Type(text) => keys::typed(text)?,
                _ => unreachable!(),
            };
            for press in &presses {
                let keys: Vec<_> = press
                    .iter()
                    .map(|k| serde_json::json!({ "type": "qcode", "data": k }))
                    .collect();
                r.qmp.execute(
                    "send-key",
                    serde_json::json!({ "keys": keys, "hold-time": KEY_HOLD_MS }),
                )?;
            }
            std::thread::sleep(typing_time(presses.len()));
        }
        Step::Alive(secs) => {
            let deadline = Instant::now() + Duration::from_secs(*secs);
            while Instant::now() < deadline {
                if let Ok(Some(status)) = r.child.try_wait() {
                    bail!(
                        "QEMU exited ({status}) within {secs} s; the machine powered off or reset"
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Step::ScreenshotPixel { x, y, rgb } => {
            let file = run_dir.join("screen.ppm");
            r.qmp
                .execute("screendump", serde_json::json!({ "filename": file }))?;
            let got = ppm_pixel(&fs::read(&file)?, *x, *y)?;
            if got != *rgb {
                bail!(
                    "pixel ({x}, {y}) is #{:02x}{:02x}{:02x}, expected #{:02x}{:02x}{:02x} ({})",
                    got[0],
                    got[1],
                    got[2],
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    file.display()
                );
            }
        }
        Step::Reboot(last) => reboot(r, last.as_deref(), *timeout)?,
        Step::FileLines { path, bytes, line } => {
            if !r.off {
                bail!("file-lines reads the disk: switch the machine off first (poweroff)");
            }
            let data = image::read_ext2_file(&r.qemu.disk, r.root, path, run_dir)?
                .with_context(|| format!("{path}: no such file on the disk"))?;
            if let Some(why) = lines_mismatch(&data, *bytes, line) {
                bail!("{path} {why}");
            }
        }
        Step::Poweroff => {
            exit_with(r, "poweroff", EXIT_POWEROFF, *timeout)?;
            r.off = true;
        }
        Step::CheckScript(path) => {
            if !r.off {
                bail!("check-script reads the disk: switch the machine off first (poweroff)");
            }
            let read = |p: &str| -> Result<String> {
                let data = image::read_ext2_file(&r.qemu.disk, r.root, p, run_dir)?
                    .with_context(|| format!("{p}: no such file on the disk"))?;
                Ok(String::from_utf8_lossy(&data).into_owned())
            };
            let script = read(path)?;
            let transcript = read(&shell::commands::transcript_name(path))?;
            let report =
                checks::check(&checks::parse(&script, checks::Machine::Qemu)?, &transcript);
            if !report.ok() {
                bail!(
                    "{path}: {} of {} commands as expected\n{}",
                    report.passed,
                    report.commands,
                    report.failures.join("\n")
                );
            }
        }
        Step::Unplug => {
            r.qmp.execute(
                "device_del",
                serde_json::json!({ "id": qemu::STICK_DEVICE }),
            )?;
            r.qmp.wait_event(
                "DEVICE_DELETED",
                |d| d["device"] == qemu::STICK_DEVICE,
                *timeout,
            )?;
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
    let mut seen = HashMap::new();
    for (line, step) in &scenario.steps {
        if let Err(e) = run_step(&mut r, step, &mut timeout, &run_dir, &mut seen) {
            bail!(
                "scenario '{}' failed at line {line} ({step:?}): {e:#}\n--- last serial output ---\n{}\n--- full log: {} ---",
                scenario.name,
                tail(&r.text(), 40),
                run_dir.join("serial.log").display()
            );
        }
    }
    let disk = r.qemu.disk.clone();
    drop(r);
    // Repaired, the broken root must be as the machine found it: nothing
    // was written to what it could not mount.
    if scenario.break_root {
        image::set_ext2_magic(&disk, layout.root, true)?;
    }
    image::fsck(&disk, layout.root).with_context(|| {
        format!(
            "scenario '{}': e2fsck -fn on the disk it left",
            scenario.name
        )
    })
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
    let small = if scenarios.iter().any(|s| s.small_disk) {
        let img = image::build_image_as(
            &art,
            DEFAULT_CMDLINE,
            "relay-os-small.img",
            crate::config::SMALL_IMAGE_BYTES,
        )?;
        let layout = image::read_layout(&img)?;
        Some((img, layout))
    } else {
        None
    };
    for s in &scenarios {
        println!("== e2e {}", s.name);
        match (&small, s.small_disk) {
            (Some((img, layout)), true) => run_scenario(img, layout, s)?,
            _ => run_scenario(&img, &layout, s)?,
        }
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
            s.esp_edits,
            vec![EspEdit::Write(
                "/EFI/RELAY/kernel.elf".into(),
                "not an elf".into()
            )]
        );
        assert!(parse_scenario("x", "esp-write relative x").is_err());
        assert!(parse_scenario("x", "expect a\nesp-write /x y").is_err());
    }

    #[test]
    fn parses_esp_deletes_and_other_abis() {
        let s = parse_scenario(
            "x",
            "esp-delete /EFI/RELAY/system.img\nsystem-abi 99\nexpect x",
        )
        .unwrap();
        assert_eq!(
            s.esp_edits,
            vec![
                EspEdit::Delete("/EFI/RELAY/system.img".into()),
                EspEdit::SystemAbi(99)
            ]
        );
        assert!(parse_scenario("x", "esp-delete relative").is_err());
        assert!(parse_scenario("x", "system-abi many").is_err());
        assert!(parse_scenario("x", "expect a\nsystem-abi 2").is_err());
        assert!(parse_scenario("x", "expect a\nesp-delete /x").is_err());
    }

    #[test]
    fn parses_the_check_script_step() {
        let s = parse_scenario("x", "poweroff\ncheck-script /root/checks/a.sh").unwrap();
        assert_eq!(
            s.steps[1],
            (2, Step::CheckScript("/root/checks/a.sh".into()))
        );
        assert!(parse_scenario("x", "check-script").is_err());
        assert!(parse_scenario("x", "check-script root/a.sh").is_err());
    }

    #[test]
    fn parses_the_expect_same_step() {
        let s = parse_scenario("x", r"expect-same mem Mem:\s+\d+\s+(\d+)").unwrap();
        assert_eq!(
            s.steps[0],
            (
                1,
                Step::ExpectSame {
                    name: "mem".into(),
                    pattern: r"Mem:\s+\d+\s+(\d+)".into()
                }
            )
        );
        assert!(parse_scenario("x", "expect-same mem").is_err(), "no regex");
        assert!(
            parse_scenario("x", r"expect-same mem Mem:\s+\d+").is_err(),
            "no group"
        );
        assert!(
            parse_scenario("x", "expect-same mem (").is_err(),
            "bad regex"
        );
    }

    #[test]
    fn expect_same_compares_with_the_first_value() {
        let mut seen = HashMap::new();
        same_as_before(&mut seen, "mem", "1024").unwrap();
        same_as_before(&mut seen, "mem", "1024").unwrap();
        same_as_before(&mut seen, "heap", "7").unwrap();
        let e = same_as_before(&mut seen, "mem", "1028").unwrap_err();
        assert_eq!(e.to_string(), "mem is 1028, but it was 1024 the first time");
        same_as_before(&mut seen, "mem", "1024").unwrap();
    }

    #[test]
    fn parses_the_unplug_step() {
        let s = parse_scenario("x", "unplug\nsend ls").unwrap();
        assert_eq!(s.steps[0], (1, Step::Unplug));
        assert!(parse_scenario("x", "unplug now").is_err());
    }

    #[test]
    fn parses_key_steps() {
        let s = parse_scenario("x", "key echo {up}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Key("echo {up}".into()))]);
        assert!(parse_scenario("x", "key {bogus}").is_err());
        let s = parse_scenario("x", "type {ctrl-d}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Type("{ctrl-d}".into()))]);
        assert!(parse_scenario("x", "type {bogus}").is_err());
    }

    #[test]
    fn a_key_step_waits_until_qemu_has_played_it() {
        // "ls" and Enter: three presses of 30 ms, then the settling time.
        assert_eq!(typing_time(3), Duration::from_millis(190));
    }

    #[test]
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario("x", "reboot\nreboot relay: restarting\npoweroff").unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Reboot(None)),
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff)
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
    }

    #[test]
    fn the_clean_flag_is_read_from_the_superblock() {
        let dir = out_dir().join("e2e-selftest").join("clean-flag");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2"])
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        assert_eq!(image::ext2_state(&img, whole).unwrap(), "clean");
        // As a machine killed with / mounted read-write leaves it.
        crate::util::run(
            std::process::Command::new("debugfs")
                .args(["-w", "-R", "ssv state 0"])
                .arg(&img),
        )
        .unwrap();
        assert_eq!(image::ext2_state(&img, whole).unwrap(), "not clean");
    }

    #[test]
    fn parses_break_root() {
        let s = parse_scenario("x", "break-root\nexpect a").unwrap();
        assert!(s.break_root);
        assert!(!parse_scenario("x", "expect a").unwrap().break_root);
        assert!(parse_scenario("x", "expect a\nbreak-root").is_err());
    }

    #[test]
    fn breaking_the_root_hides_it_until_it_is_restored() {
        let dir = out_dir().join("e2e-selftest").join("break-root");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2"])
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        image::set_ext2_magic(&img, whole, false).unwrap();
        assert!(image::fsck(&img, whole).is_err(), "no filesystem found");
        image::set_ext2_magic(&img, whole, true).unwrap();
        image::fsck(&img, whole).unwrap();
    }

    #[test]
    fn exit_statuses_match_the_kernel() {
        // relay_kernel::power::TEST_EXIT_CODE is 0x10.
        assert_eq!(EXIT_POWEROFF, (0x10 << 1) | 1);
    }

    #[test]
    fn parses_the_small_disk_and_file_lines() {
        let s = parse_scenario("x", "disk small\nfile-lines /root/big 8 abc").unwrap();
        assert!(s.small_disk);
        assert_eq!(
            s.steps,
            vec![(
                2,
                Step::FileLines {
                    path: "/root/big".into(),
                    bytes: 8,
                    line: "abc".into()
                }
            )]
        );
        assert!(!parse_scenario("x", "expect a").unwrap().small_disk);
        assert!(parse_scenario("x", "disk large").is_err());
        assert!(parse_scenario("x", "expect a\ndisk small").is_err());
        assert!(
            parse_scenario("x", "file-lines /f 7 abc").is_err(),
            "not whole lines"
        );
        assert!(parse_scenario("x", "file-lines f 8 abc").is_err());
        assert!(parse_scenario("x", "file-lines /f 8").is_err());
    }

    #[test]
    fn file_lines_finds_the_first_wrong_line() {
        assert_eq!(lines_mismatch(b"ab\nab\n", 6, "ab"), None);
        assert_eq!(
            lines_mismatch(b"ab\nab\n", 9, "ab"),
            Some("is 6 bytes, expected 9".into())
        );
        assert_eq!(
            lines_mismatch(b"ab\nax\nab\n", 9, "ab"),
            Some("line 2 (byte 3) is \"ax\\n\"".into())
        );
    }

    #[test]
    fn file_lines_reads_the_file_from_an_ext2_image() {
        let dir = out_dir().join("e2e-selftest").join("file-lines");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("staging/root")).unwrap();
        fs::write(dir.join("staging/root/big"), "abc\n".repeat(1000)).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2", "-d"])
                .arg(dir.join("staging"))
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        let data = image::read_ext2_file(&img, whole, "/root/big", &dir).unwrap();
        assert_eq!(lines_mismatch(&data.unwrap(), 4000, "abc"), None);
        let missing = image::read_ext2_file(&img, whole, "/root/nope", &dir).unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn parses_alive_step() {
        let s = parse_scenario("x", "alive 12").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Alive(12))]);
        assert!(parse_scenario("x", "alive soon").is_err());
    }

    #[test]
    fn parses_screenshot_pixel_step() {
        let s = parse_scenario("x", "screenshot-pixel 2540 20 #000000").unwrap();
        assert_eq!(
            s.steps,
            vec![(
                1,
                Step::ScreenshotPixel {
                    x: 2540,
                    y: 20,
                    rgb: [0, 0, 0]
                }
            )]
        );
        assert!(parse_scenario("x", "screenshot-pixel 1 2").is_err());
        assert!(parse_scenario("x", "screenshot-pixel 1 2 red").is_err());
    }

    #[test]
    fn reads_one_ppm_pixel() {
        let mut ppm = b"P6\n3 2\n255\n".to_vec();
        ppm.extend(std::iter::repeat_n(0u8, 3 * 2 * 3));
        let n = ppm.len();
        ppm[n - 3..].copy_from_slice(&[0xFF, 0x80, 0x00]); // (2, 1)
        assert_eq!(ppm_pixel(&ppm, 2, 1).unwrap(), [0xFF, 0x80, 0x00]);
        assert_eq!(ppm_pixel(&ppm, 0, 0).unwrap(), [0, 0, 0]);
        assert!(ppm_pixel(&ppm, 3, 0).is_err(), "x out of range");
    }

    #[test]
    fn qemu_dying_at_start_up_is_reported_with_its_stderr() {
        let dir = out_dir().join("e2e-selftest");
        fs::create_dir_all(&dir).unwrap();
        let stderr = dir.join("qemu.stderr");
        let mut child = std::process::Command::new("sh")
            .args(["-c", "echo 'qemu: could not load firmware' >&2; exit 1"])
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let err = wait_for_qmp(
            &mut child,
            &dir.join("nobody-listens.sock"),
            Duration::from_secs(10),
            &stderr,
        )
        .err()
        .expect("must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("exited"), "{msg}");
        assert!(msg.contains("could not load firmware"), "{msg}");
        assert!(started.elapsed() < Duration::from_secs(5), "gave up early");
    }

    /// A reboot matches what the machine printed before it went down, then
    /// continues the log: both need every byte QEMU wrote before it exited.
    #[test]
    fn the_serial_reader_hands_over_everything_printed() {
        let dir = out_dir().join("e2e-selftest");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reader.log");
        let mut child = std::process::Command::new("sh")
            .args(["-c", "head -c 1000000 /dev/zero | tr '\\0' x; printf END"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let serial = Arc::new(Mutex::new(Vec::new()));
        let reader = read_serial(
            child.stdout.take().unwrap(),
            serial.clone(),
            fs::File::create(&path).unwrap(),
        );
        child.wait().unwrap();
        let mut log = reader.join().unwrap();
        assert_eq!(serial.lock().unwrap().len(), 1_000_003);
        assert!(serial.lock().unwrap().ends_with(b"xEND"));
        log.write_all(b"MARK").unwrap();
        drop(log);
        let text = fs::read(&path).unwrap();
        assert_eq!(text.len(), 1_000_007);
        assert!(text.ends_with(b"xENDMARK"));
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
