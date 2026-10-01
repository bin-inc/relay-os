//! `/bin/seq`: print a sequence of whole numbers (user-space gate §8.4,
//! §9.1), the shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("seq", shell::commands::seq, &args)
}
