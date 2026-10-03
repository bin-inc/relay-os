//! A command run as a program of its own (user-space gate §8.4): one of
//! `/bin`'s programs runs its command function here, with standard input
//! on its fd 0, standard output on its fd 1 and errors on the console, and
//! says what the shell said in milestone 1 when the output could not be
//! written.

use crate::commands::Run;
use crate::ctx::Ctx;
use crate::io::{Console, Stdin, Stdout, System};
use alloc::format;
use alloc::string::String;
use vfs::Vfs;

/// What a command run as a program works with: the files, the console for
/// its errors, the system, and its fds 0 and 1.
pub struct CommandIo<'a> {
    pub vfs: &'a mut dyn Vfs,
    pub console: &'a mut dyn Console,
    pub system: &'a mut dyn System,
    pub stdin: &'a mut dyn Stdin,
    pub stdout: &'a mut dyn Stdout,
}

/// Runs the command `name`, whose function is `run` (one of
/// `commands`'), with `args` (without the name), as its program does;
/// returns the exit status. A write error on standard output is reported
/// as `<name>: write error: <message>` with status 1. Each program names
/// its own function, so it holds no other command's code.
pub fn run_command(name: &str, run: Run, args: &[String], io: CommandIo<'_>) -> i32 {
    let mut ctx = Ctx::program(io.vfs, io.system, io.console, io.stdout);
    ctx.set_input(io.stdin);
    let mut status = run(&mut ctx, args);
    if let Err(e) = ctx.finish() {
        ctx.err(format!("{name}: write error: {e}\n").as_bytes());
        status = ctx.write_error_status;
    }
    status
}

#[cfg(test)]
mod tests {
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
    use vfs::{Errno, Node, Vfs};

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
        // Its fd 0 that file too, read up to some point (`cat < f >> f`
        // under `/bin/sh`).
        struct File(Node, u64);
        impl crate::Stdin for File {
            fn read(&mut self, _: &mut [u8]) -> Result<usize, Errno> {
                unreachable!("refused before it reads")
            }
            fn file(&mut self) -> Option<(Node, u64, u64)> {
                Some((self.0, self.1, 2))
            }
        }
        let cat = crate::commands::find("cat").unwrap().run;
        let mut out = FakeStdout::file(Some(node));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut File(node, 1),
            stdout: &mut out,
        };
        assert_eq!(crate::run_command("cat", cat, &[], io), 1);
        assert_eq!(h.console.take(), "cat: -: input file is output file\n");
    }

    #[test]
    fn a_program_reads_its_standard_input() {
        let mut h = Harness::new();
        let mut out = FakeStdout::console();
        h.stdin = b"from fd 0\n".to_vec();
        assert_eq!(h.program("cat", &mut out), (0, String::new()));
        assert_eq!(out.text(), "from fd 0\n");
    }

    /// What a program did, in order: each read of standard input and each
    /// write of standard output.
    type Log = alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<String>>>;

    /// Standard input a line a read, as the console in line mode and a
    /// pipe from a writer that writes lines give it.
    struct Lines(Log, alloc::vec::Vec<&'static str>);

    impl crate::Stdin for Lines {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
            let line = if self.1.is_empty() {
                ""
            } else {
                self.1.remove(0)
            };
            self.0.borrow_mut().push(alloc::format!("read {line:?}"));
            buf[..line.len()].copy_from_slice(line.as_bytes());
            Ok(line.len())
        }
    }

    /// Standard output that is a pipe.
    struct Pipe(Log);

    impl crate::Stdout for Pipe {
        fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            let text = String::from_utf8_lossy(bytes);
            self.0.borrow_mut().push(alloc::format!("write {text:?}"));
            Ok(())
        }
        fn is_tty(&self) -> bool {
            false
        }
        fn node(&self) -> Option<vfs::Node> {
            None
        }
    }

    #[test]
    fn what_a_program_read_reaches_its_pipe_before_it_reads_again() {
        // `cat | cat`: the second gets each line as it is typed, not 4 KiB
        // later.
        for (name, run, args) in [
            ("cat", crate::commands::cat as crate::commands::Run, &[][..]),
            ("head", crate::commands::head, &["-n", "5"][..]),
        ] {
            let mut h = Harness::new();
            let log = Log::default();
            let args: alloc::vec::Vec<String> = args.iter().map(|a| String::from(*a)).collect();
            let io = crate::CommandIo {
                vfs: &mut h.vfs,
                console: &mut h.console,
                system: &mut h.system,
                stdin: &mut Lines(log.clone(), alloc::vec!["one\n", "two\n"]),
                stdout: &mut Pipe(log.clone()),
            };
            assert_eq!(crate::run_command(name, run, &args, io), 0);
            assert_eq!(
                *log.borrow(),
                [
                    "read \"one\\n\"",
                    "write \"one\\n\"",
                    "read \"two\\n\"",
                    "write \"two\\n\"",
                    "read \"\""
                ],
                "{name}"
            );
        }
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
