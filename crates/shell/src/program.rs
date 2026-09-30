//! A command run as a program of its own (user-space gate §8.4): one of
//! `/bin`'s programs runs its command function here, with standard output
//! on its fd 1 and errors on the console, and says what the shell said in
//! milestone 1 when the output could not be written.

use crate::commands::Run;
use crate::ctx::Ctx;
use crate::io::{Console, Stdout, System};
use alloc::format;
use alloc::string::String;
use vfs::Vfs;

/// Runs the command `name`, whose function is `run` (one of
/// `commands`'), with `args` (without the name), as its program does;
/// returns the exit status. A write error on standard output is reported
/// as `<name>: write error: <message>` with status 1. Each program names
/// its own function, so it holds no other command's code.
pub fn run_command(
    name: &str,
    run: Run,
    args: &[String],
    vfs: &mut dyn Vfs,
    console: &mut dyn Console,
    system: &mut dyn System,
    stdout: &mut dyn Stdout,
) -> i32 {
    let mut ctx = Ctx::program(vfs, system, console, stdout);
    let mut status = run(&mut ctx, args);
    if let Err(e) = ctx.finish() {
        ctx.err(format!("{name}: write error: {e}\n").as_bytes());
        status = 1;
    }
    status
}

#[cfg(test)]
mod tests {
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
    use vfs::{Errno, Vfs};

    #[test]
    fn output_goes_to_stdout_and_errors_to_the_console() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"in a\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.program("cat /tmp/a /tmp/nope /tmp/a", &mut out),
            (1, "cat: /tmp/nope: No such file or directory\n".into())
        );
        // To the console it is written at once, so that it keeps its place
        // among the errors.
        assert_eq!(out.writes, [b"in a\n".to_vec(), b"in a\n".to_vec()]);
    }

    #[test]
    fn output_to_a_file_is_written_in_pieces_of_4_kib() {
        let mut h = Harness::new();
        let big = alloc::vec![b'x'; 10_000];
        h.put("/tmp/big", &big);
        let mut out = FakeStdout::file(None);
        assert_eq!(h.program("cat /tmp/big", &mut out), (0, String::new()));
        assert_eq!(out.text().len(), 10_000);
        assert!(out.writes.len() <= 3, "{} writes", out.writes.len());
        // And a line at a time is not a write each.
        let mut out = FakeStdout::file(None);
        h.program("ls /etc", &mut out);
        assert_eq!(out.writes, [b"hostname\nmotd\n".to_vec()]);
    }

    #[test]
    fn ls_lays_out_columns_only_on_the_console() {
        let mut h = Harness::new();
        let mut out = FakeStdout::console();
        h.program("ls /etc", &mut out);
        assert_eq!(out.text(), "hostname  motd\n");
    }

    #[test]
    fn a_write_error_is_reported_as_the_shell_did() {
        let mut h = Harness::new();
        h.put("/tmp/big", &alloc::vec![b'x'; 10_000]);
        for mut out in [FakeStdout::file(None), FakeStdout::console()] {
            out.fail_after = Some((5000, Errno::ENOSPC));
            assert_eq!(
                h.program("cat /tmp/big", &mut out),
                (1, "cat: write error: No space left on device\n".into())
            );
        }
    }

    #[test]
    fn a_file_is_not_its_own_output() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"f\n");
        let node = h.vfs.lookup(b"/tmp/f").unwrap();
        let mut out = FakeStdout::file(Some(node));
        assert_eq!(
            h.program("cat /tmp/f", &mut out),
            (1, "cat: /tmp/f: input file is output file\n".into())
        );
    }

    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
        let mut h = Harness::new();
        h.system.power_error = Some(Errno::EIO);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.program("reboot", &mut out),
            (
                1,
                "reboot: cannot shut the filesystems down cleanly: Input/output error\n\
                 reboot: use 'reboot -f' to go ahead anyway\n"
                    .into()
            )
        );
        assert_eq!(h.system.reboots, 0);
        // -f goes ahead, having said why the filesystems were not shut down
        // cleanly, as milestone 1's did: `power` is asked without it first.
        assert_eq!(
            h.program("poweroff -f", &mut out),
            (
                0,
                "poweroff: cannot shut the filesystems down cleanly: Input/output error\n".into()
            )
        );
        assert_eq!(
            (h.system.poweroffs, &h.system.forced[..]),
            (1, &[false, false, true][..])
        );
        assert_eq!(
            h.spy.shutdowns.get(),
            2,
            "the Vfs's own shutdown comes first"
        );
        // When they can be, -f changes nothing.
        let mut h = Harness::new();
        assert_eq!(h.program("reboot -f", &mut out), (0, String::new()));
        assert_eq!((h.system.reboots, &h.system.forced[..]), (1, &[false][..]));
    }
}
