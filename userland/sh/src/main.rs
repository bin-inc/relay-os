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
    let (mut vfs, mut system, mut programs) = (SysVfs::new(), SysSystem, SysPrograms);
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell is started in a process group of its own.
        let mut console = SysConsole::owned_by(sys::getpid());
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
        shell.status() as u8
    } else {
        let mut console = SysConsole::new();
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_file(&words, &mut SysStdout::new()) as u8
    }
}
