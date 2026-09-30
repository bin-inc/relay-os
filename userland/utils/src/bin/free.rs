//! `/bin/free`: show memory use (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("free", shell::commands::free, &args)
}
