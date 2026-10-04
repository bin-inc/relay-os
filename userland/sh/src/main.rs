//! `/bin/sh` (user-space gate §8.3): the shell as a program. Without
//! arguments it is the interactive shell, which takes the console at its
//! prompt and gives it to each command it starts, unless its standard input
//! is no console (`X | sh`): then it runs the lines it reads there and
//! leaves the console alone. `sh FILE [ARG]...` runs a script, its
//! commands in the shell's own process group and its transcript a console
//! tee. Every command but the shell's own is a program. Outside a script
//! `$0` is the shell's argument 0, as bash's is. Every shell imports the
//! environment it was started with as exported variables.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use relay_abi::console::MODE_RAW;
use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, env, sys};
use shell::Shell;

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let (mut vfs, mut system) = (SysVfs::new(), SysSystem);
    let words = words(&args);
    let name = args
        .iter()
        .next()
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .unwrap_or_default();
    if words.is_empty() && !SysStdin::is_console() {
        // In a pipeline, in its group: commands read from the pipe, and the
        // console stays with the group, in line mode, so Ctrl-C ends them.
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block())
            .named(&name);
        shell.run_input(&mut SysStdin) as u8
    } else if words.is_empty() {
        // A shell whose group was never given the console (`sh &`, or one
        // a background script started) may not set its mode, and has no
        // console to read: it ends before its first prompt. Otherwise it
        // leads a process group of its own when it was started at a prompt
        // (the group is numbered after it); one a script started is in the
        // script's group, and no group has its number.
        if sys::console_mode(MODE_RAW) == Err(relay_abi::errno::EPERM) {
            return 0;
        }
        let leader = sys::console_foreground(sys::getpid()).is_ok();
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
        let mut programs = SysPrograms::new(leader);
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block())
            .named(&name);
        shell.run();
        shell.status() as u8
    } else {
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block());
        shell.run_file(&words, &mut SysStdout::new()) as u8
    }
}
