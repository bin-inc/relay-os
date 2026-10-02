//! `/bin/test` and `/bin/[` (spec §6.1, §15 item 3): the shell's command
//! function run as a program. xtask packs it under both names, since Cargo
//! refuses `[` as a binary's, and it is `[` when started by that name.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    let (name, run) = shell::commands::named(args.get(0).unwrap_or(b""));
    relay_rt::sysio::run_command(name, run, &args)
}
