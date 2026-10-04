# Relay OS

[![CI](https://github.com/bin-inc/relay-os/actions/workflows/ci.yml/badge.svg)](https://github.com/bin-inc/relay-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Release](https://img.shields.io/github/v/release/bin-inc/relay-os)](https://github.com/bin-inc/relay-os/releases)

**A small operating system written from scratch in Rust.** It boots through
UEFI on real hardware (an Intel NUC 12 Pro) and in QEMU, runs programs in
ring 3, each in its own address space, and ships a shell you can program.

```text
root@relay:~# seq 1000 | grep 7 | wc -l
271
```

Try it in QEMU in two commands (details under [Quick start](#quick-start)):

```sh
cargo xtask test      # unit tests, then every QEMU scenario
cargo xtask qemu      # boot it in a window; type `help` at the prompt
```

## Status

A small operating system written from scratch in Rust. Milestone 1 boots
from a USB stick on an Intel NUC 12 Pro, shows a terminal over HDMI and
stores files on the stick's ext2 root filesystem. Milestone 2 runs programs
from `/bin` in ring 3, each in its own address space: the shell runs a
name it does not know (`t-args a b`) as a program, programs start
programs, the timer shares the CPU among them, and Ctrl-C stops the
command that runs. Programs open, read and write files, map memory (their
runtime gives them a heap), read the console a line at a time or as it is
typed, and copy what the console shows into files (tees). Every command
but the shell's built-ins (`cd`, `exit`, `export`, `help`, `jobs`,
`kill`, `unset`, `wait`) is a program of its own in `/bin` (`/bin/ls`),
which prints what the shell's command prints, and the shell itself is
one too: `/bin/sh` runs each of those commands as a program, and its
scripts may run scripts.
Process 1 is the kernel's init: it starts `/bin/sh` at boot and again
whenever it ends, and a machine that cannot run its shell (no
`system.img`, or a shell that keeps ending) shows an error screen and
restarts at a key. The kernel holds no shell of its own any more.

Milestone 3 connects the programs: pipes (`seq 1000 | grep 7 | wc -l`),
with `grep`, `seq`, `sleep`, `true` and `false`; jobs in the background
(`t-spin &`), with `jobs`, `wait`, `kill %1` and `ps`; and scripts with
arguments and variables (`sh FILE a b`, `$1`, `"$@"`, `$?`, `NAME=value`).

Milestone 4 makes the shell one to program in: lists (`a; b`,
`a && b`, `a || b`, `! a`, `a & b`), commands typed across lines with
bash's `> ` prompt, `if`, `while`, `until` and `for`, and `test` and `[`
(`[ -f FILE ]`) as programs, with `grep -q`.

Milestone 5 adds redirection and the environment: every standard stream
can be redirected (`< in`, `2> err`, `2>> err`, `> f 2>&1`, `a 2>&1 | b`),
compound commands too (`for …; done > f`), and `/dev/null` exists;
programs get an environment, the shell exports variables (`export`,
`unset`, `A=1 cmd`) and `/bin/env` shows and changes one; `cd` follows
`HOME`, `PWD` and `OLDPWD` (`cd -`), and the check scripts pass from any
directory.

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0, milestone 3
version 0.4.0, which ends the user-space gate, milestone 4 version 0.5.0
and milestone 5 version 0.6.0, which ends the programmable shell gate.

Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
(milestone 1), `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`
(milestones 2 and 3: the shell and its commands as programs in ring 3)
and `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md`
(milestones 4 and 5: control flow, redirection and the environment).

## Quick start

In QEMU, on any Linux machine with the tools below:

```sh
cargo xtask test      # unit tests, then every QEMU scenario (a minute or two)
cargo xtask qemu      # boot it in a window; type `help` at the prompt
```

On the Intel NUC 12 Pro with the Kingston test stick (`docs/hardware-test.md`
has the whole checklist):

1. Once: `cargo xtask setup-udev`, run the three `sudo` commands it prints,
   and replug the stick.
2. `cargo xtask flash --full` and type `ERASE` (this erases the stick).
3. Reboot, press F10 and choose the UEFI entry for the Kingston stick. Every
   startup line says `[ ok ]` and the prompt `root@relay:~# ` follows.
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh`, `sh checks/check4.sh`, `sh checks/check5.sh`
   and `sh checks/check6.sh`, then `cd checks` and `sh check7.sh`, then
   `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of every script.

After a code change, `cargo xtask flash --kernel` replaces only the loader,
the kernel, the programs of `/bin` (`system.img`) and the kernel command
line (empty unless `--cmdline` is given) and keeps the files on the stick.

## Requirements (host)

Linux with `rustup`, a C toolchain (the host tools link with `cc`),
`qemu-system-x86_64`, OVMF (`/usr/share/OVMF/OVMF_CODE_4M.fd` and
`OVMF_VARS_4M.fd`), `mtools`, `e2fsprogs`, `sfdisk`, `udisks2`, `binutils`
(`readelf` checks every user program) and `linux-libc-dev` (the tests
check the error numbers against Linux's headers). The tests of `test` run
GNU's as root through `unshare -r`, which needs unprivileged user
namespaces: Ubuntu 24.04 restricts them unless
`kernel.apparmor_restrict_unprivileged_userns` is 0 (Linux Mint allows
them). On Ubuntu or Linux Mint:

```sh
sudo apt install build-essential qemu-system-x86 ovmf mtools e2fsprogs fdisk udisks2 binutils linux-libc-dev
```

The Rust toolchain and targets are installed automatically from
`rust-toolchain.toml`.

## Common commands

| Command | What it does |
|---|---|
| `cargo xtask ci` | Everything a pull request must pass: lint, unit tests, QEMU scenarios |
| `cargo xtask lint` | `cargo fmt --check` and clippy (warnings are errors) on every crate |
| `cargo xtask unit` | Host unit tests |
| `cargo xtask test` | Host unit tests, then every QEMU scenario in `tests/e2e/`, up to 4 at once (`--jobs 1` for one at a time) |
| `cargo xtask test --scenario NAME` | Host unit tests, then one scenario (`tests/e2e/NAME.txt`); `--e2e-only` skips the unit tests |
| `cargo xtask build` | Build the loader, the kernel and `system.img` |
| `cargo xtask image` | Build `target/relay/relay-os.img` |
| `cargo xtask qemu` | Boot the image in a QEMU window (serial on this terminal; `--serial-only` for no window) |
| `cargo xtask host-shell <img>` | Run the shell on this machine over the image's ext2 partition (changes it in place; `exit` leaves) |
| `cargo xtask setup-udev` | Write a udev rule that gives you access to the test stick, and print the three `sudo` commands that install it |
| `cargo xtask flash --full` | Erase and write the Kingston test stick (`--yes` skips typing `ERASE`) |
| `cargo xtask flash --kernel` | Update loader, kernel, `system.img` and command line on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick, list its files and check the transcripts of the check scripts |
| `cargo xtask gen-font <bdf>` | Regenerate `crates/term/src/font.rs` from a Spleen BDF file |

`image`, `qemu` and `flash` take `--cmdline '…'`, the kernel command line
written to the ESP (`docs/hardware-test.md` lists the options).

`RELAY_QEMU_ACCEL` is the list of QEMU accelerators to try, comma-separated;
it defaults to `kvm,tcg`, so QEMU falls back to software emulation without
`/dev/kvm`. Set `RELAY_QEMU_ACCEL=tcg` to force it.

## Continuous integration

Every pull request to `main`, and every push to it, runs
`.github/workflows/ci.yml` with three jobs (`lint`, `unit`, `e2e`). The jobs
only install tools and call the `cargo xtask` subcommands above, so
`cargo xtask ci` locally checks what these jobs check. If a scenario fails,
the scenarios' run directories (`target/relay/e2e/`: serial logs, screenshots
and disk images) are attached to the run as the `e2e-logs` artefact. GitHub's
CodeQL default setup also scans every pull request; it has no local
equivalent. Dependabot (`.github/dependabot.yml`) proposes weekly updates of
the crates, the Rust toolchain and the actions. Hardware checks on the NUC
stay manual (`docs/hardware-test.md`); the pull-request template asks about
them.

## Contributing

Commit messages and pull-request titles follow Conventional Commits
(`feat(kernel): …`, `fix(usb): …`); the types, scopes and rules are in
`CONTRIBUTING.md`. AI coding agents start from `AGENTS.md`: the rules the
code keeps, how tests are written and how plans are made and executed.

Questions and ideas are welcome in
[Discussions](https://github.com/bin-inc/relay-os/discussions). Report
vulnerabilities as described in [SECURITY.md](SECURITY.md). Everyone taking
part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

If you like the project, you can
[buy the maintainer a coffee](https://buymeacoffee.com/maw629).

## Layout

| Path | Contents |
|---|---|
| `boot/` | `relay-boot`, the UEFI loader (`BOOTX64.EFI`); it also loads `system.img` |
| `kernel/` | `relay-kernel`, the higher-half kernel |
| `crates/boot-info` | Loader → kernel hand-off structure |
| `crates/term` | Framebuffer text terminal |
| `crates/vfs` | Error numbers, block-device and filesystem traits, paths, mount table, in-memory filesystem, `DevFs` (`/dev/null`) |
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser, expansion (`$1`, `$NAME`, `~`), variables and the environment (`export`, `A=1 cmd`), redirection, pipelines, jobs, scripts (`sh FILE`) and every command's function; used by `/bin/sh`, each program of `/bin` and `host-shell` |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel and of user programs |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, ELF note, call numbers, error numbers, result encoding, and the structs the calls pass (`Stat`, `SpawnArgs`, `ProcInfo`, `WaitStatus`, …); no architecture detail |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, environment, system calls, heap, panic handler, ABI note, linker script; the shell's `Vfs`, `Console`, `System`, `Stdin`, `Stdout` and `Programs` over system calls |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `sh/` the shell (`/bin/sh`), `utils/` one per command (`cat`, `ls`, …), `tests/` the `t-*` test programs (xtask adds `t-abi`, a copy of `t-args` stamped with an older ABI) |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img`; `xtask/fixtures/checks/` holds the check scripts' recorded transcripts (QEMU and NUC) |
| `kernel/fixtures/acpi/` | ACPI tables of QEMU and the NUC for the kernel's unit tests |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
| `tests/e2e/` | QEMU end-to-end scenarios |
| `docs/hardware-test.md` | Manual checklist for the NUC, and the log of its results |
| `docs/superpowers/` | Design specs (`specs/`), milestone roadmaps and the implementation plans (`plans/`) |
| `.github/` | CI workflow, Dependabot configuration and pull-request template |
| `.cargo/config.toml`, `rust-toolchain.toml` | The `cargo xtask` alias and static relocation for everything built for `x86_64-unknown-none` (the kernel and the programs); the pinned Rust toolchain and targets |
| `CONTRIBUTING.md` | Commit-message and pull-request rules |
| `AGENTS.md` | Context and working rules for AI coding agents (`CLAUDE.md` points Claude Code at it) |

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you state otherwise, any
contribution you submit is dual licensed as above, without further terms.
