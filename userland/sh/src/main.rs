//! `/bin/sh` (user-space gate §8.3): the shell as a program. Without
//! arguments it is the interactive shell, which takes the console at its
//! prompt and gives it to each command it starts; `sh FILE` runs a script,
//! its commands in the shell's own process group and its transcript a
//! console tee. Every command but `cd`, `exit` and `help` is a program.
#![no_std]
#![no_main]

use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdout, SysSystem, SysVfs, sys};
use shell::Shell;

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let (mut vfs, mut system) = (SysVfs::new(), SysSystem);
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell leads a process group of its own when it
        // was started at a prompt (the group is numbered after it); one a
        // script started is in the script's group.
        let leader = sys::console_foreground(sys::getpid()).is_ok();
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
        let mut programs = SysPrograms::new(leader);
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
        shell.status() as u8
    } else {
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_file(&words, &mut SysStdout::new()) as u8
    }
}
