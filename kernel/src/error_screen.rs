//! The error screen (user-space gate §11.2): what the machine shows when
//! it cannot run its shell, because `system.img` is missing, damaged or
//! of another ABI, `/bin/sh` cannot be started, or the shell ended three
//! times within 10 s. Unlike the red panic screen it is no kernel bug:
//! init shows it, in its own context, with interrupts on, so the idle task
//! polls the keyboards and COM1 meanwhile. It says why and shows the last
//! lines of the kernel log, waits for a key for as long as it takes (a
//! restart on its own would only come back here), then syncs, shuts the
//! filesystems down and restarts through ACPI (M1 §7.4).

use crate::mounts::KernelVfs;
use crate::system::SystemError;
use crate::{console, klog, power, proc, tty};
use alloc::format;
use alloc::vec::Vec;
use core::fmt;
use term::ansi::Action;
use vfs::{Errno, Vfs};

/// Lines of the kernel log it shows at most: with its own 7 lines, 27 of
/// the NUC's 33 rows when none wraps; fewer when some do (`fitting`).
pub const TAIL_LINES: usize = 20;

/// Why the machine cannot run its shell.
#[derive(Debug, PartialEq, Eq)]
pub enum Reason {
    /// The system archive, as its startup line says.
    System(SystemError),
    /// `spawn` of `/bin/sh` failed.
    CannotStart(Errno),
    /// The shell ended three times within 10 s.
    Ended,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::System(e) => write!(f, "system: {e}"),
            Reason::CannotStart(e) => {
                write!(f, "{} cannot start: {}", crate::init::SHELL, e.message())
            }
            Reason::Ended => write!(
                f,
                "{} ended {} times within {} s",
                crate::init::SHELL,
                crate::init::ENDS,
                crate::init::WINDOW.as_secs()
            ),
        }
    }
}

/// What follows the log lines.
const FOOTER: &[u8] = b"\nPress any key to reboot.\n";

/// What clears the console for the screen, whatever a program's output
/// left the terminal in: CAN first ends an escape a program left
/// unfinished (a lone ESC would swallow the next one, and the colours it
/// resets would stay), and prints nothing otherwise; then the colours,
/// the clear and the cursor's home.
const CLEAR: &[u8] = b"\x18\x1b[0m\x1b[2J\x1b[H";

/// The screen on a console of `size` (columns, rows): a heading, the
/// reason, the newest of the kernel log's last lines (`tail`, without its
/// colours) that fit without scrolling the heading away, and what to do.
pub fn text(reason: &Reason, tail: &[u8], size: (usize, usize)) -> Vec<u8> {
    let (cols, rows) = size;
    let mut tail = tail.to_vec();
    let n = klog::strip_ansi_in_place(&mut tail);
    tail.truncate(n);
    // Any other escape a program's bytes put in the log (`ESC c` resets
    // the terminal) is shown, not obeyed.
    for b in tail.iter_mut().filter(|b| **b == 0x1B) {
        *b = b'?';
    }
    if !tail.is_empty() && !tail.ends_with(b"\n") {
        tail.push(b'\n');
    }
    let mut out: Vec<u8> = format!(
        "*** Relay OS cannot run its shell ***\n\n{reason}\n\n--- last kernel log lines ---\n"
    )
    .into_bytes();
    // The cursor's row after the last line counts too: a line there would
    // scroll the first away.
    let left = rows.saturating_sub(rows_of(&out, cols) + rows_of(FOOTER, cols) + 1);
    out.extend_from_slice(fitting(&tail, cols, left));
    out.extend_from_slice(FOOTER);
    out
}

/// The rows `text` takes on a console `cols` wide, as the terminal moves
/// its cursor (`term`'s parser, and `Terminal`'s rules): each line at
/// least one, and one more each time a character comes after the last
/// column. A tab moves to the next multiple of 8 but never past the last
/// column, a carriage return to the first, a backspace one back.
fn rows_of(text: &[u8], cols: usize) -> usize {
    let cols = cols.max(1);
    let mut parser = term::ansi::Parser::new();
    let (mut rows, mut cx, mut open) = (0, 0, false);
    for &b in text {
        parser.advance(b, &mut |action| match action {
            Action::Print(_) => {
                if cx >= cols {
                    rows += 1;
                    cx = 0;
                }
                cx += 1;
                open = true;
            }
            Action::Control(b'\n') => {
                rows += 1;
                cx = 0;
                open = false;
            }
            Action::Control(b'\r') => {
                cx = 0;
                open = true;
            }
            Action::Control(0x08) => {
                cx = cx.min(cols - 1).saturating_sub(1);
                open = true;
            }
            Action::Control(b'\t') => {
                if cx < cols {
                    cx = ((cx / 8 + 1) * 8).min(cols - 1);
                }
                open = true;
            }
            Action::Control(_) | Action::Csi { .. } | Action::Reset => {}
        });
    }
    rows + usize::from(open)
}

/// The newest whole lines of `tail` that take at most `rows` rows.
fn fitting(tail: &[u8], cols: usize, rows: usize) -> &[u8] {
    let (mut start, mut used) = (tail.len(), 0);
    for line in tail.split_inclusive(|&b| b == b'\n').rev() {
        let r = rows_of(line, cols);
        if used + r > rows {
            break;
        }
        used += r;
        start -= line.len();
    }
    &tail[start..]
}

/// Ends every other process, shows the screen for `reason`, waits for a
/// key, and restarts the machine. Only process 1 calls it.
pub fn show(reason: &Reason) -> ! {
    // Nothing else writes to the console while the screen is up: an
    // orphan's output would land under it and could scroll it away.
    proc::kill_others();
    let mut tail = [0u8; 4096];
    let n = klog::KLOG.lock().tail_lines(TAIL_LINES, &mut tail);
    // The console is init's, in raw mode, and what was typed before the
    // screen came is not the key.
    proc::take_console();
    tty::poll();
    while tty::pop().is_some() {}
    console::write_bytes(CLEAR);
    let size = console::size().unwrap_or((80, 25));
    console::write_bytes(&text(reason, &tail[..n], size));
    loop {
        tty::poll();
        if tty::pop().is_some() {
            break;
        }
        proc::wait_for_input();
    }
    // As `reboot` does (M1 §7.4): what can be written is, and a failure
    // does not keep the machine up, since nothing here can be fixed.
    let _ = KernelVfs.sync();
    let _ = KernelVfs.shutdown();
    power::reboot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    /// The NUC's console: 120 columns, 33 rows.
    const NUC: (usize, usize) = (120, 33);

    fn lines(reason: &Reason, tail: &[u8]) -> Vec<String> {
        String::from_utf8(text(reason, tail, NUC))
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_screen_says_why_shows_the_log_and_what_to_do() {
        let tail = b"[ ok ] mount /: ext2\n[\x1b[31mFAIL\x1b[0m] system: no system.img\n";
        assert_eq!(
            lines(&Reason::System(SystemError::Missing), tail),
            [
                "*** Relay OS cannot run its shell ***",
                "",
                "system: no system.img",
                "",
                "--- last kernel log lines ---",
                "[ ok ] mount /: ext2",
                "[FAIL] system: no system.img",
                "",
                "Press any key to reboot.",
            ]
        );
        // A tail cut in the middle of its last line still ends it.
        assert_eq!(
            lines(&Reason::Ended, b"init: /bin/sh")[5..],
            ["init: /bin/sh", "", "Press any key to reboot."]
        );
        assert_eq!(
            lines(&Reason::Ended, b"")[5..],
            ["", "Press any key to reboot."],
            "no log at all"
        );
    }

    #[test]
    fn each_reason_is_named() {
        assert_eq!(
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 2"
        );
        assert_eq!(
            Reason::CannotStart(Errno::ENOEXEC).to_string(),
            "/bin/sh cannot start: Exec format error"
        );
        assert_eq!(
            Reason::Ended.to_string(),
            "/bin/sh ended 3 times within 10 s"
        );
    }

    #[test]
    fn it_fits_the_nuc_s_33_rows() {
        let mut log = klog::Ring::<4096>::new();
        for i in 0..100 {
            log.write(alloc::format!("line {i}\n").as_bytes());
        }
        let mut tail = [0u8; 4096];
        let n = log.tail_lines(TAIL_LINES, &mut tail);
        let screen = lines(&Reason::Ended, &tail[..n]);
        assert_eq!(screen.len(), 7 + TAIL_LINES);
        assert!(screen.len() <= 33);
        assert_eq!(screen[5], "line 80");
    }

    /// The NUC's last lines before a `[FAIL] system` line, as its check 3
    /// transcript has them: some are wider than its 120 columns.
    const NUC_BOOT: &str = "\
xhci 00:14.0: slot 3: interface 1 class 224/1/1, endpoints 0x03 isochronous 0 bytes interval 1, 0x83 isochronous 0 bytes interval 1\n\
xhci 00:14.0: slot 3: no driver for this device\n\
xhci 00:14.0: port 15: connection stable after 100 ms\n\
xhci 00:14.0: port 15: reset done, SuperSpeed\n\
xhci 00:14.0: port 15: slot 4 at address 4, 0951:1666 USB 3.10, ep0 512 bytes, 1 configuration\n\
xhci 00:14.0: slot 4: interface 0 class 8/6/80, endpoints 0x81 bulk 1024 bytes interval 0, 0x02 bulk 1024 bytes interval 0\n\
xhci 00:14.0: slot 4: endpoint 0x81 configured (DCI 3, interval exponent 0)\n\
xhci 00:14.0: slot 4: endpoint 0x02 configured (DCI 4, interval exponent 0)\n\
storage: slot 4: vendor \"Kingston\", product \"DataTraveler 3.0\", revision \"PMAP\", removable\n\
storage: slot 4: 30277632 blocks of 512 bytes\n\
usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard\n\
usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard\n\
usb: 00:14.0 port 10: 8087:0033 full-speed, not claimed\n\
usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB\n\
[ ok ] usb: 2 controllers, 4 devices\n\
[ ok ] keyboard: 2 keyboards\n\
storage: 00:14.0 port 15: GPT with 2 partitions\n\
storage: root on 00:14.0 port 15, the disk with the boot partition 4A7D166A-7C33-42FF-A52D-CAE8368D7935\n\
[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB\n\
[FAIL] system: no system.img\n\
";

    /// The rows `screen` takes on a console `cols` wide, with the cursor's
    /// after its last line.
    fn rows(screen: &[u8], cols: usize) -> usize {
        String::from_utf8_lossy(screen)
            .split_terminator('\n')
            .map(|l| l.chars().count().div_ceil(cols).max(1))
            .sum::<usize>()
            + 1
    }

    /// A console of the NUC's size, 120 by 33, after `text` was written to
    /// the real terminal: its rows, blanks at the end cut, and the
    /// cursor's row.
    fn on_the_nuc(text: &[u8]) -> (Vec<String>, usize) {
        nuc_terminal(text, |t| {
            let rows = (0..t.rows())
                .map(|r| {
                    let row: String = (0..t.cols()).map(|c| t.cell(c, r).ch as char).collect();
                    row.trim_end().to_string()
                })
                .collect();
            (rows, t.cursor().1)
        })
    }

    /// `f` of the real terminal at the NUC's size, after `text`.
    fn nuc_terminal<R>(text: &[u8], f: impl FnOnce(&term::Terminal) -> R) -> R {
        use term::{Cell, PixelFormat, Terminal};
        let (w, h) = (1920, 1080);
        assert_eq!(term::geometry(w, h), (120, 33, 2));
        let mut cells = alloc::vec![Cell::BLANK; 120 * 33];
        let mut shadow = alloc::vec![0u32; w * h];
        let mut t = Terminal::new(w, h, PixelFormat::Bgr, &mut cells, &mut shadow);
        t.write_bytes(text);
        f(&t)
    }

    #[test]
    fn the_screen_has_its_colours_whatever_a_program_left_the_terminal_in() {
        // A program's last bytes before the screen: black on black, then
        // a sequence it never finished (a lone ESC, half a CSI, half a
        // UTF-8 character), which would swallow the screen's own reset.
        for left in [
            &b"hello \x1b[30;40mdark\x1b"[..],
            b"\x1b[30;40m\x1b[1;",
            b"\x1b[30;40m\xC3",
            b"\x1b[30;40m",
        ] {
            let mut screen = left.to_vec();
            screen.extend_from_slice(CLEAR);
            screen.extend_from_slice(&text(&Reason::Ended, b"x\n", NUC));
            nuc_terminal(&screen, |t| {
                let (first, rows) = (t.cell(0, 0), t.rows());
                let row: String = (0..t.cols()).map(|c| t.cell(c, 0).ch as char).collect();
                assert_eq!(
                    row.trim_end(),
                    "*** Relay OS cannot run its shell ***",
                    "{left:?}"
                );
                assert_ne!(first.fg, first.bg, "{left:?}: the heading can be seen");
                assert!(rows == 33);
            });
        }
    }

    /// Lines a program's path can put in the kernel log, each one where a
    /// rule of the terminal's makes a difference: tabs, which move to the
    /// next multiple of 8 but never past the last column; a carriage
    /// return; backspaces after the last column; exactly one character
    /// too many; bytes that are no UTF-8; a terminal reset (`ESC c`) the
    /// colour strip leaves in.
    fn awkward_lines() -> Vec<Vec<u8>> {
        let mut bad_utf8 = b"\xC3".to_vec();
        bad_utf8.extend("\u{e9}".repeat(119).bytes());
        [
            "x\t".repeat(20),
            "\t".repeat(16) + "y",
            "a".repeat(100) + "\r" + &"b".repeat(100),
            "b".repeat(120) + "\x08\x08cc",
            "c".repeat(121),
            "\u{e9}".repeat(121),
            "pid 7 (/root/\x1bc): killed: kill".to_string(),
        ]
        .into_iter()
        .map(String::into_bytes)
        .chain([bad_utf8])
        .map(|mut l| {
            l.push(b'\n');
            l
        })
        .collect()
    }

    #[test]
    fn a_line_takes_the_rows_the_terminal_gives_it() {
        for line in awkward_lines() {
            let mut shown = line.clone();
            let n = klog::strip_ansi_in_place(&mut shown);
            shown.truncate(n);
            shown
                .iter_mut()
                .filter(|b| **b == 0x1B)
                .for_each(|b| *b = b'?');
            let (_, row) = on_the_nuc(&shown);
            assert_eq!(
                rows_of(&shown, 120),
                row,
                "{:?}",
                String::from_utf8_lossy(&line)
            );
        }
    }

    #[test]
    fn the_heading_stays_on_the_nuc_s_terminal_whatever_the_log_holds() {
        for line in awkward_lines() {
            let (screen, _) = on_the_nuc(&text(&Reason::Ended, &line.repeat(20), NUC));
            assert_eq!(
                screen[0], "*** Relay OS cannot run its shell ***",
                "{screen:?}"
            );
            assert!(
                screen.iter().any(|r| r == "Press any key to reboot."),
                "{screen:?}"
            );
            assert!(!screen.iter().any(|r| r.contains('\x1b')));
        }
    }

    #[test]
    fn the_screen_never_scrolls_its_heading_away() {
        let reason = Reason::System(SystemError::Missing);
        let screen = text(&reason, NUC_BOOT.as_bytes(), NUC);
        assert!(rows(&screen, 120) <= 33, "{}", rows(&screen, 120));
        let shown = lines(&reason, NUC_BOOT.as_bytes());
        assert_eq!(shown.len(), 7 + 20, "all 20 lines fit, two of them wrapped");
        // Lines of 200 columns take two rows each: only the newest that
        // fit are shown, whole.
        let long: String = (0..20)
            .map(|i| format!("{i:02} {}\n", "x".repeat(197)))
            .collect();
        let screen = text(&Reason::Ended, long.as_bytes(), NUC);
        assert_eq!(
            rows(&screen, 120),
            32,
            "12 lines of two rows: one row left over"
        );
        let shown = lines(&Reason::Ended, long.as_bytes());
        assert!(
            shown[5].starts_with("08 ") && shown[16].starts_with("19 "),
            "{shown:?}"
        );
        assert_eq!(shown[0], "*** Relay OS cannot run its shell ***");
        // A character is a column, however many bytes it takes.
        let wide = "é".repeat(120) + "\n";
        let screen = text(&Reason::Ended, wide.as_bytes(), (120, 9));
        assert_eq!(rows(&screen, 120), 9, "one row: it fits in exactly");
        // A console with no room for the log shows none of it.
        let screen = String::from_utf8(text(&Reason::Ended, long.as_bytes(), (80, 5))).unwrap();
        assert!(!screen.contains("xxx") && screen.ends_with("Press any key to reboot.\n"));
    }
}
