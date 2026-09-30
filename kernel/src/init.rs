//! Process 1, init (user-space gate §6.6, §16 item 6): a process of the
//! kernel's own, without a program. It prints `/etc/motd`, starts
//! `/bin/sh` in `/root` (or `/` without one) as its child, collects every
//! orphan as it ends, and when the shell ends says how and starts another.
//! Three ends within 10 s, or a shell that cannot be started, reach the
//! error screen instead.

use crate::error_screen::{self, Reason};
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
use crate::system::SystemError;
use crate::{kprintln, proc, timer, tty};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;
use relay_abi::wait::EXITED;
use relay_abi::{FdMap, WaitStatus};
use spin::Mutex;
use vfs::{Errno, Vfs};

/// The program init starts.
pub const SHELL: &str = "/bin/sh";
/// How many ends of the shell within [`WINDOW`] make init give up.
pub const ENDS: usize = 3;
/// See [`ENDS`].
pub const WINDOW: Duration = Duration::from_secs(10);
/// Where the shell starts (spec §6.6).
const HOME: &[u8] = b"/root";
/// The most of `/etc/motd` shown.
const MOTD_MAX: usize = 16 * 1024;

/// Why `system.img` could not be mounted, if it could not (startup step
/// 10): init shows the error screen for it instead of starting a shell.
static SYSTEM: Mutex<Option<SystemError>> = Mutex::new(None);

/// The system archive could not be mounted: there is no `/bin/sh`.
pub fn system_failed(e: SystemError) {
    *SYSTEM.lock() = Some(e);
}

/// Whether init's record of the archive is locked now (for the kernel's
/// checks that no lock is held across a switch).
pub fn is_locked() -> bool {
    SYSTEM.is_locked()
}

/// Process 1 (spec §4.4 step 11). Never returns.
pub extern "C" fn run(_: u64) -> ! {
    // Taken out first: a guard made in the `if let` would stay locked
    // through the error screen, which waits, and so switches.
    let failed = SYSTEM.lock().take();
    if let Some(e) = failed {
        error_screen::show(&Reason::System(e));
    }
    motd();
    let mut respawn = Respawn::default();
    loop {
        let pid = start_shell().unwrap_or_else(|e| error_screen::show(&Reason::CannotStart(e)));
        // Every orphan that ends meanwhile is collected too. The next
        // shell's `FOREGROUND` gives it the console, whatever this one left
        // it in.
        let status = proc::wait_collecting(pid).expect("init waits for its own child");
        let again = respawn.ended(timer::tsc_time().unwrap_or_else(timer::uptime));
        kprintln!("{}", ended_line(pid, &status, again));
        if !again {
            error_screen::show(&Reason::Ended);
        }
    }
}

/// Shows `/etc/motd`, as the shell did in milestone 1.
fn motd() {
    let mut vfs = KernelVfs;
    if let Ok(node) = vfs.lookup(b"/etc/motd") {
        let mut buf = alloc::vec![0; MOTD_MAX];
        if let Ok(n) = vfs.read_at(node, 0, &mut buf) {
            tty::write(&buf[..n]);
        }
    }
}

/// Starts `/bin/sh` without arguments in `/root`, or in `/` without one
/// (the empty read-only root the kernel falls back to), as a group of its
/// own with the console, its fds 0-2 the console; its pid.
fn start_shell() -> Result<u32, Errno> {
    let mut vfs = KernelVfs;
    if vfs.chdir(HOME).is_err() {
        let _ = vfs.chdir(b"/");
    }
    let fds = [0, 1, 2].map(|fd| FdMap {
        child: fd,
        parent: fd,
    });
    let mut args: Vec<u8> = SHELL.into();
    args.push(0);
    proc::spawn(&Spawn {
        path: SHELL.into(),
        args,
        argc: 1,
        cwd: Vec::new(),
        fds: fds.to_vec(),
        new_group: true,
        foreground: true,
    })
}

/// When the shell ended lately, to tell whether to start it again (spec
/// §6.6): not after its [`ENDS`]th end within [`WINDOW`], since a shell
/// that ends at once would otherwise be started for ever.
#[derive(Debug, Default)]
pub struct Respawn {
    /// The last ends' times, oldest first.
    ends: [Option<Duration>; ENDS],
}

impl Respawn {
    /// The shell ended at `now` (time since the machine started); whether
    /// to start it again.
    pub fn ended(&mut self, now: Duration) -> bool {
        self.ends.rotate_left(1);
        self.ends[ENDS - 1] = Some(now);
        match self.ends[0] {
            Some(first) => now.saturating_sub(first) > WINDOW,
            None => true,
        }
    }
}

/// What init says when the shell `pid` ended with `status`, on the screen
/// and in the kernel log: `init: /bin/sh (pid 2) exited with 0; starting
/// it again`, or `killed: <how>` for a shell that was killed.
pub fn ended_line(pid: u32, status: &WaitStatus, again: bool) -> String {
    let how = if status.how == EXITED {
        format!("{status}")
    } else {
        format!("killed: {status}")
    };
    let then = if again { "; starting it again" } else { "" };
    format!("init: {SHELL} (pid {pid}) {how}{then}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_KILL};

    fn s(secs: u64, ms: u64) -> Duration {
        Duration::from_secs(secs) + Duration::from_millis(ms)
    }

    #[test]
    fn the_shell_is_started_again_until_it_ends_three_times_within_10_s() {
        let mut r = Respawn::default();
        assert!(r.ended(s(0, 0)), "a first end");
        assert!(r.ended(s(1, 0)), "a second");
        assert!(!r.ended(s(10, 0)), "a third, 10 s after the first");
        let mut r = Respawn::default();
        assert!(r.ended(s(0, 0)));
        assert!(r.ended(s(9, 0)));
        assert!(r.ended(s(10, 1)), "just over 10 s after the first");
        assert!(
            r.ended(s(19, 500)),
            "the first is forgotten: 9.5 s after the second"
        );
        assert!(
            !r.ended(s(20, 0)),
            "three within 10 s: 9, 10.001 and 19.5 ... 20"
        );
    }

    #[test]
    fn ends_far_apart_never_stop_it() {
        let mut r = Respawn::default();
        for i in 0..100 {
            assert!(r.ended(s(i * 6, 0)), "end {i}");
        }
    }

    #[test]
    fn a_clock_that_stands_still_stops_it_at_the_third_end() {
        let mut r = Respawn::default();
        assert!(r.ended(Duration::ZERO));
        assert!(r.ended(Duration::ZERO));
        assert!(!r.ended(Duration::ZERO));
    }

    #[test]
    fn init_says_how_the_shell_ended() {
        assert_eq!(
            ended_line(2, &WaitStatus::exited(0), true),
            "init: /bin/sh (pid 2) exited with 0; starting it again"
        );
        assert_eq!(
            ended_line(7, &WaitStatus::killed(KILLED_KILL), true),
            "init: /bin/sh (pid 7) killed: kill; starting it again"
        );
        assert_eq!(
            ended_line(
                3,
                &WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x401000),
                false
            ),
            "init: /bin/sh (pid 3) killed: page fault at 0x0, read, ip 0x401000"
        );
        assert_eq!(
            ended_line(4, &WaitStatus::exited(3), false),
            "init: /bin/sh (pid 4) exited with 3"
        );
    }
}
