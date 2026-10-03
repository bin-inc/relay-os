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
//! system-drop sh                   (before boot: system.img, rewritten
//!                                   without that program)
//! timeout 20                       (seconds, for the following expects)
//! expect <regex>                   (waits for serial output, ANSI stripped)
//! expect-same <name> <regex>       (as expect; the regex's first group must
//!                                   capture what it did the first time a
//!                                   step of that name matched)
//! send <text>                      (types <text> + Enter over serial)
//! send-crlf <text>                 (as send, ending with CR LF)
//! key <text>                       (types <text> + Enter on the USB keyboard,
//!                                   QMP send-key; {up}, {ctrl-c}: see keys.rs)
//! type <text>                      (as key, without the Enter)
//! send-ahead <text>                (as send, while something runs on purpose:
//!                                   input for a program, a line typed ahead;
//!                                   send-crlf-ahead, key-ahead and type-ahead
//!                                   likewise)
//! ```
//!
//! Every `send`, `send-crlf`, `key` and `type` waits for the prompt: since
//! the input before it (or the start, a reboot or a reset), an expect must
//! have ended at one (`root@relay:~# `, `root@relay:~# $` or `> $`), or the
//! scenario is refused. A line sent while a command runs is echoed twice,
//! by the line discipline and by the shell's editor, and lands inside the
//! output an expect waits for (programmable shell gate §15 item 3); input
//! sent while something runs on purpose is marked `-ahead`.
//!
//! ```text
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
//! alive 12                         (fails if QEMU exits within 12 seconds)
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! reboot [<regex>]                 (types `reboot`; QEMU must exit as after a
//!                                   reset, having printed <regex> first, then
//!                                   starts again on the same disk)
//! reset [<text>]                   (types <text> + Enter over serial, or Enter
//!                                   alone: a key at the error screen; QEMU
//!                                   must exit as after a reset, then starts
//!                                   again on the same disk)
//! reset-key                        (as `reset`, with Enter pressed on the USB
//!                                   keyboard: QMP send-key)
//! poweroff [<command>]             (types `poweroff`, or <command>; QEMU must
//!                                   exit through isa-debug-exit, test mode's
//!                                   power-off; an `expect` after it reads
//!                                   what the machine printed before)
//! unplug                           (pulls the USB stick out: QMP device_del,
//!                                   then QEMU's DEVICE_DELETED event; the
//!                                   root need not be clean after it)
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
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// Text sent over the serial console with CR LF after it, as some
    /// terminals send Enter.
    SendCrLf(String),
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
    /// Type this text (or nothing) and Enter over the serial console, and
    /// the machine restarts as with `Reboot` (a key at the error screen).
    Reset(String),
    /// As `Reset`, with Enter pressed on the USB keyboard instead.
    ResetKey,
    /// Switch the machine off with a command (`poweroff` if none is
    /// given); no later step talks to it.
    Poweroff(String),
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
    /// `system-drop`: `system.img` holds its programs but this one.
    SystemDrop(String),
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
    // An expect of the prompt has come since the last input.
    let mut paced = false;
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let input = matches!(word, "send" | "send-crlf" | "key" | "type");
        if input && !paced {
            bail!(
                "{name}:{line_no}: {word} does not wait for the prompt: an expect that \
                 ends at it must come first, or it is {word}-ahead (sent while \
                 something runs)"
            );
        }
        if word.ends_with("-ahead") && paced {
            bail!(
                "{name}:{line_no}: {word} after an expect of the prompt: it waits for \
                 nothing, so it is {}",
                word.trim_end_matches("-ahead")
            );
        }
        if input || word.ends_with("-ahead") || matches!(word, "reboot" | "reset" | "reset-key") {
            paced = false;
        }
        if matches!(word, "expect" | "expect-same") && ends_at_prompt(rest) {
            paced = true;
        }
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
            "esp-write" | "esp-delete" | "system-abi" | "system-drop" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: {word} must come before other steps");
                }
                let edit =
                    match word {
                        "system-abi" => EspEdit::SystemAbi(rest.parse().with_context(|| {
                            format!("{name}:{line_no}: system-abi needs a number")
                        })?),
                        "system-drop" => {
                            if rest.is_empty() || rest.contains(' ') {
                                bail!("{name}:{line_no}: system-drop needs one program's name");
                            }
                            EspEdit::SystemDrop(rest.to_string())
                        }
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
            "send" | "send-ahead" => Step::Send(rest.to_string()),
            "send-crlf" | "send-crlf-ahead" => Step::SendCrLf(rest.to_string()),
            "key" | "key-ahead" => {
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
            }
            "type" | "type-ahead" => {
                keys::typed(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Type(rest.to_string())
            }
            "screenshot-nonblank" => Step::ScreenshotNonblank,
            "screenshot-pixel" => {
                parse_pixel_step(rest).with_context(|| format!("{name}:{line_no}"))?
            }
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "reboot" if rest.is_empty() => Step::Reboot(None),
            "reset" => Step::Reset(rest.to_string()),
            "reset-key" if rest.is_empty() => Step::ResetKey,
            "reboot" => {
                Regex::new(rest).with_context(|| format!("{name}:{line_no}: bad regex"))?;
                Step::Reboot(Some(rest.to_string()))
            }
            "poweroff" if rest.is_empty() => Step::Poweroff("poweroff".to_string()),
            "poweroff" => Step::Poweroff(rest.to_string()),
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

/// Whether an expect's pattern ends at a prompt: the shell's
/// (`root@relay:~# `, its blank trimmed with the line's, or `# $`) or the
/// `> ` of a command that goes on (`> $`).
fn ends_at_prompt(pattern: &str) -> bool {
    static PROMPT: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"root@relay:\S*#( \$)?$|> \$$").unwrap());
    PROMPT.is_match(pattern)
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
    /// The stick was pulled out: the disk keeps what it had then, so its
    /// clean flag says nothing about how the machine went down.
    unplugged: bool,
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
            EspEdit::SystemAbi(_) | EspEdit::SystemDrop(_) => {
                let image = fs::read(out_dir().join("system.img"))?;
                let other = match edit {
                    EspEdit::SystemAbi(abi) => userland::with_abi(&image, *abi)?,
                    EspEdit::SystemDrop(program) => userland::without(&image, program)?,
                    _ => unreachable!(),
                };
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
        unplugged: false,
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

/// Types `command` over serial and waits for QEMU to exit with `status`
/// (`wait_exit`).
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    r.stdin.write_all(command.as_bytes())?;
    r.stdin.write_all(b"\r")?;
    r.stdin.flush()?;
    wait_exit(r, command, status, timeout)
}

/// Waits (up to `timeout`) for QEMU to exit with `status` after `command`;
/// the machine shut the filesystem down first, so it must be marked clean,
/// unless the stick was pulled out. Returns the serial log once everything
/// QEMU printed is in it.
fn wait_exit(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
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
            if state != "clean" && !r.unplugged {
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

/// `reboot` (or `reset`): after `command`, the machine resets, and QEMU
/// (`-no-reboot`) exits, after printing `last` if given; then the same disk
/// boots again, with the serial log continued.
fn reboot(r: &mut Running, command: &str, last: Option<&str>, timeout: Duration) -> Result<()> {
    let log = exit_with(r, command, EXIT_RESET, timeout)?;
    boot_again(r, log, last)
}

/// After a reset: the machine printed `last` if given, and the same disk
/// boots again, with the serial log `log` continued.
fn boot_again(r: &mut Running, mut log: fs::File, last: Option<&str>) -> Result<()> {
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
        Step::Timeout(_) | Step::Expect(_) | Step::FileLines { .. } | Step::CheckScript(_)
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
        Step::SendCrLf(text) => {
            r.stdin.write_all(text.as_bytes())?;
            r.stdin.write_all(b"\r\n")?;
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
        Step::Reboot(last) => reboot(r, "reboot", last.as_deref(), *timeout)?,
        Step::Reset(text) => reboot(r, text, None, *timeout)?,
        Step::ResetKey => {
            let enter = serde_json::json!([{ "type": "qcode", "data": "ret" }]);
            r.qmp.execute(
                "send-key",
                serde_json::json!({ "keys": enter, "hold-time": KEY_HOLD_MS }),
            )?;
            let log = wait_exit(r, "Enter on the USB keyboard", EXIT_RESET, *timeout)?;
            boot_again(r, log, None)?;
        }
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
        Step::Poweroff(command) => {
            exit_with(r, command, EXIT_POWEROFF, *timeout)?;
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
            r.unplugged = true;
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

/// Scenarios run at once by default: one per core, at most 4. Each machine
/// has one CPU and 1 GiB, so 4 fit CI's runner (4 CPUs, 16 GiB), which
/// then runs the scenarios in a third of the time one at a time takes.
pub fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(4))
}

/// Runs `f` on every item, `jobs` at a time (0 means 1), each thread taking
/// the next item not yet started. A failure stops nothing: the failures
/// come back, in the items' order.
fn run_queue<T: Sync>(
    items: &[T],
    jobs: usize,
    f: impl Fn(&T) -> Result<()> + Sync,
) -> Vec<anyhow::Error> {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if let Err(e) = f(item) {
                        failures.lock().unwrap().push((i, e));
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort_by_key(|(i, _)| *i);
    failures.into_iter().map(|(_, e)| e).collect()
}

/// Builds the image and runs every scenario (or only `only`), `jobs` at
/// once; each has its own copy of the disk and its own run directory.
pub fn run_all(only: Option<&str>, jobs: usize) -> Result<()> {
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
    let started = Instant::now();
    let failures = run_queue(&scenarios, jobs, |s| {
        println!("== e2e {}", s.name);
        let t = Instant::now();
        let result = match (&small, s.small_disk) {
            (Some((img, layout)), true) => run_scenario(img, layout, s),
            _ => run_scenario(&img, &layout, s),
        };
        let verdict = if result.is_ok() { "ok" } else { "FAILED" };
        println!("{verdict} {} {:.1}s", s.name, t.elapsed().as_secs_f64());
        result
    });
    if !failures.is_empty() {
        let all: Vec<String> = failures.iter().map(|e| format!("{e:#}")).collect();
        bail!(
            "{} of {} scenario(s) failed:\n\n{}",
            failures.len(),
            scenarios.len(),
            all.join("\n\n")
        );
    }
    println!(
        "all {} scenario(s) passed in {:.1}s with {} job(s)",
        scenarios.len(),
        started.elapsed().as_secs_f64(),
        jobs.clamp(1, scenarios.len())
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps_and_cmdline() {
        let s = parse_scenario(
            "x",
            "# hi\ncmdline test=1 panic=ud\n\ntimeout 5\nexpect \\[ ok \\] cpu\nsend-ahead ls -l\nscreenshot-nonblank\n",
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
    fn input_waits_for_a_prompt_unless_typed_ahead() {
        let at_prompt = |text: &str| {
            parse_scenario("x", text)
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        let prompt = "expect root@relay:~# $\n";
        for ok in [
            "expect root@relay:~# $\nsend a\n",
            "expect \\na\\nroot@relay:~# $\nsend a\n",
            "expect root@relay:/tmp# $\nkey a\n",
            "expect \\^C\\nroot@relay:~# \ntype a\n",
            "expect root@relay:\\S+# $\nsend-crlf a\n",
            "expect \\n> $\nsend a\n",
            "expect-same m (\\d+)\\nroot@relay:~# $\nsend a\n",
            // An expect of the prompt, then others, before the input.
            "expect root@relay:~# $\nexpect x\nalive 1\nsend a\n",
            "send-ahead a\nkey-ahead b\ntype-ahead c\nsend-crlf-ahead d\n",
        ] {
            assert_eq!(at_prompt(ok), Ok(()), "{ok:?}");
        }
        for (text, line) in [
            ("send a\n", 1),
            ("expect x\nsend a\n", 2),
            ("expect root@relay:~# $\nsend a\nsend b\n", 3),
            ("expect root@relay:~# $\nsend a\nexpect \\na\\n\nkey b\n", 4),
            ("expect root@relay:~# $\nsend a\nsend-ahead b\ntype c\n", 4),
            ("expect root@relay:~# $\nreboot\nsend a\n", 3),
            ("expect root@relay:~# $\nreset\nsend a\n", 3),
            ("expect root@relay:~# $\nreset-key\nsend a\n", 3),
            // A prompt with something after it is not the end of the text.
            ("expect root@relay:~# x\nsend a\n", 2),
            ("expect > x\nsend a\n", 2),
            ("expect a> \nsend a\n", 2),
        ] {
            let e = at_prompt(text).unwrap_err();
            assert!(e.starts_with(&format!("x:{line}: ")), "{text:?}: {e}");
            assert!(e.contains("wait for the prompt"), "{e}");
        }
        assert!(at_prompt(&format!("{prompt}send a")).is_ok());
        // Marked typed ahead right after the prompt, an input waits for
        // nothing (the review).
        for (text, word) in [
            ("expect root@relay:~# $\nsend-ahead a\n", "send-ahead"),
            ("expect x\nexpect \\n> $\nkey-ahead a\n", "key-ahead"),
        ] {
            let e = at_prompt(text).unwrap_err();
            assert!(
                e.contains(&format!("{word} after an expect of the prompt")),
                "{e}"
            );
        }
    }

    #[test]
    fn every_scenario_waits_for_its_prompts() {
        assert!(load_scenarios(None).unwrap().len() >= 49);
    }

    #[test]
    fn input_typed_ahead_is_sent_as_other_input_is() {
        let s = parse_scenario(
            "x",
            "send-ahead a b\nkey-ahead {ctrl-c}\ntype-ahead q\nsend-crlf-ahead c\n",
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Send("a b".into())),
                (2, Step::Key("{ctrl-c}".into())),
                (3, Step::Type("q".into())),
                (4, Step::SendCrLf("c".into())),
            ]
        );
        // A key name is checked as for `key` and `type`.
        assert!(parse_scenario("x", "key-ahead {nope}").is_err());
        assert!(parse_scenario("x", "type-ahead {nope}").is_err());
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
    fn parses_esp_deletes_other_abis_and_programs_left_out() {
        let s = parse_scenario(
            "x",
            "esp-delete /EFI/RELAY/system.img\nsystem-abi 99\nsystem-drop sh\nexpect x",
        )
        .unwrap();
        assert_eq!(
            s.esp_edits,
            vec![
                EspEdit::Delete("/EFI/RELAY/system.img".into()),
                EspEdit::SystemAbi(99),
                EspEdit::SystemDrop("sh".into())
            ]
        );
        assert!(parse_scenario("x", "system-drop").is_err(), "which one");
        assert!(parse_scenario("x", "system-drop a b").is_err());
        assert!(parse_scenario("x", "expect a\nsystem-drop sh").is_err());
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
        let s = parse_scenario("x", "unplug\nsend-ahead ls").unwrap();
        assert_eq!(s.steps[0], (1, Step::Unplug));
        assert!(parse_scenario("x", "unplug now").is_err());
    }

    #[test]
    fn parses_key_steps() {
        let s = parse_scenario("x", "key-ahead echo {up}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Key("echo {up}".into()))]);
        let bad =
            |t: &str| parse_scenario("x", &format!("expect root@relay:~# $\n{t}")).unwrap_err();
        assert!(format!("{:#}", bad("key {bogus}")).contains("bogus"));
        let s = parse_scenario("x", "send-crlf-ahead ls").unwrap();
        assert_eq!(s.steps, vec![(1, Step::SendCrLf("ls".into()))]);
        let s = parse_scenario("x", "type-ahead {ctrl-d}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Type("{ctrl-d}".into()))]);
        assert!(format!("{:#}", bad("type {bogus}")).contains("bogus"));
    }

    #[test]
    fn a_key_step_waits_until_qemu_has_played_it() {
        // "ls" and Enter: three presses of 30 ms, then the settling time.
        assert_eq!(typing_time(3), Duration::from_millis(190));
    }

    #[test]
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario(
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff\nreset\nreset reboot -f\nreset-key",
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Reboot(None)),
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff("poweroff".into())),
                (4, Step::Poweroff("t-sys poweroff".into())),
                (5, Step::Reset(String::new())),
                (6, Step::Reset("reboot -f".into())),
                (7, Step::ResetKey)
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
        assert!(
            parse_scenario("x", "reset-key x").is_err(),
            "Enter alone: after the first key the machine is gone"
        );
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

    #[test]
    fn the_queue_runs_every_item_once() {
        let ran = Mutex::new(Vec::new());
        let failures = run_queue(&(0..20).collect::<Vec<_>>(), 4, |i| {
            ran.lock().unwrap().push(*i);
            Ok(())
        });
        assert!(failures.is_empty());
        let mut ran = ran.into_inner().unwrap();
        ran.sort();
        assert_eq!(ran, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn the_queue_runs_at_most_jobs_items_at_once() {
        let (active, most) = (Mutex::new(0), Mutex::new(0));
        run_queue(&[(); 12], 3, |_| {
            {
                let mut a = active.lock().unwrap();
                *a += 1;
                let mut m = most.lock().unwrap();
                *m = (*m).max(*a);
            }
            std::thread::sleep(Duration::from_millis(20));
            *active.lock().unwrap() -= 1;
            Ok(())
        });
        assert_eq!(most.into_inner().unwrap(), 3);
    }

    /// A failure does not stop the others: every item runs, and every
    /// failure comes back, in the items' order.
    #[test]
    fn the_queue_runs_on_past_failures_and_returns_them_all() {
        let ran = Mutex::new(0);
        let failures = run_queue(&(0..10).collect::<Vec<_>>(), 4, |i| {
            *ran.lock().unwrap() += 1;
            if *i == 3 || *i == 7 {
                bail!("item {i}");
            }
            Ok(())
        });
        assert_eq!(ran.into_inner().unwrap(), 10);
        let messages: Vec<String> = failures.iter().map(|e| e.to_string()).collect();
        assert_eq!(messages, ["item 3", "item 7"]);
    }

    /// One job (and zero, which means one) runs the items in their order.
    #[test]
    fn one_job_runs_the_items_in_order() {
        for jobs in [0, 1] {
            let ran = Mutex::new(Vec::new());
            run_queue(&(0..10).collect::<Vec<_>>(), jobs, |i| {
                ran.lock().unwrap().push(*i);
                Ok(())
            });
            assert_eq!(ran.into_inner().unwrap(), (0..10).collect::<Vec<_>>());
        }
    }

    #[test]
    fn default_jobs_is_one_to_four() {
        assert!((1..=4).contains(&default_jobs()));
    }
}
