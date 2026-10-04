//! `/bin/env`: print the environment, or run a command with a changed one
//! (programmable shell gate §8.6), the shell's command function run as a
//! program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("env", shell::commands::env, &args)
}
