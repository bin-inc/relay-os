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
use vfs::{Errno, Vfs};

/// Lines of the kernel log it shows: with its own 7 lines, 27 of the
/// NUC's 33 rows.
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

/// The screen: a heading, the reason, the kernel log's last lines (`tail`,
/// without its colours) and what to do.
pub fn text(reason: &Reason, tail: &[u8]) -> Vec<u8> {
    let mut tail = tail.to_vec();
    let n = klog::strip_ansi_in_place(&mut tail);
    tail.truncate(n);
    if !tail.is_empty() && !tail.ends_with(b"\n") {
        tail.push(b'\n');
    }
    let mut out: Vec<u8> = format!(
        "*** Relay OS cannot run its shell ***\n\n{reason}\n\n--- last kernel log lines ---\n"
    )
    .into_bytes();
    out.extend_from_slice(&tail);
    out.extend_from_slice(b"\nPress any key to reboot.\n");
    out
}

/// Shows the screen for `reason`, waits for a key, and restarts the
/// machine. Only process 1 calls it.
pub fn show(reason: &Reason) -> ! {
    let mut tail = [0u8; 4096];
    let n = klog::KLOG.lock().tail_lines(TAIL_LINES, &mut tail);
    // The console is init's, in raw mode, and what was typed before the
    // screen came is not the key.
    proc::take_console();
    tty::poll();
    while tty::pop().is_some() {}
    console::write_bytes(b"\x1b[0m\x1b[2J\x1b[H");
    console::write_bytes(&text(reason, &tail[..n]));
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

    fn lines(reason: &Reason, tail: &[u8]) -> Vec<String> {
        String::from_utf8(text(reason, tail))
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
}
