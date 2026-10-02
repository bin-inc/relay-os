# Relay OS

A small operating system written from scratch in Rust. Milestone 1 boots
from a USB stick on an Intel NUC 12 Pro, shows a terminal over HDMI and
stores files on the stick's ext2 root filesystem. Milestone 2 runs programs
from `/bin` in ring 3, each in its own address space: the shell runs a
name it does not know (`t-args a b`) as a program, programs start
programs, the timer shares the CPU among them, and Ctrl-C stops the
command that runs. Programs open, read and write files, map memory (their
runtime gives them a heap), read the console a line at a time or as it is
typed, and copy what the console shows into files (tees). Every command
is also a program of its own in `/bin` (`/bin/ls`), which prints what the
shell's command prints, and the shell itself is one too: `/bin/sh` runs
every command but its built-ins (`cd`, `exit`, `help`, `jobs`, `wait`,
`kill`) as a program, and its scripts may run scripts. Process 1 is the
kernel's init: it starts `/bin/sh` at boot and again whenever it ends, and
a machine that cannot run its shell (no `system.img`, or a shell that
keeps ending) shows an error screen and restarts at a key. The kernel
holds no shell of its own any more.

Milestone 3 connects the programs: pipes (`seq 1000 | grep 7 | wc -l`),
with `grep`, `seq`, `sleep`, `true` and `false`; jobs in the background
(`t-spin &`), with `jobs`, `wait`, `kill %1` and `ps`; and scripts with
arguments and variables (`sh FILE a b`, `$1`, `"$@"`, `$?`, `NAME=value`).

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0 and milestone 3
version 0.4.0, which ends the user-space gate.

Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
(milestone 1) and `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`
(milestones 2 and 3: the shell and its commands as programs in ring 3).

## Quick start

In QEMU, on any Linux machine with the tools below:

```sh
cargo xtask test      # unit tests, then every QEMU scenario (a few minutes)
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
   `sh checks/check3-b.sh`, `sh checks/check4.sh` and `sh checks/check5.sh`,
   then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of every script.

After a code change, `cargo xtask flash --kernel` replaces only the loader,
the kernel and the programs of `/bin` (`system.img`) and keeps the files on
the stick.

## Requirements (host)

Linux with `rustup`, `qemu-system-x86_64`, OVMF (`/usr/share/OVMF`),
`mtools`, `e2fsprogs`, `util-linux` (`sfdisk`), `udisks2`, `binutils`
(`readelf` checks every user program) and `linux-libc-dev` (the tests check
the error numbers against Linux's headers). The Rust toolchain and targets
are installed automatically from `rust-toolchain.toml`.

## Common commands

| Command | What it does |
|---|---|
| `cargo xtask ci` | Everything a pull request must pass: lint, unit tests, QEMU scenarios |
| `cargo xtask lint` | `cargo fmt --check` and clippy (warnings are errors) on every crate |
| `cargo xtask unit` | Host unit tests |
| `cargo xtask test` | Host unit tests, then every QEMU scenario in `tests/e2e/` |
| `cargo xtask qemu` | Boot the image in a QEMU window (serial on this terminal) |
| `cargo xtask image` | Build `target/relay/relay-os.img` |
| `cargo xtask host-shell <img>` | Run the shell on this machine over the image's ext2 partition (changes it in place; `poweroff` leaves) |
| `cargo xtask flash --full` | Erase and write the Kingston test stick |
| `cargo xtask flash --kernel` | Update loader, kernel and `system.img` on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick, list its files and check the transcripts of the check scripts |

Set `RELAY_QEMU_ACCEL=tcg` to run QEMU without KVM.

## Continuous integration

Every pull request to `main` runs `.github/workflows/ci.yml` with three jobs
(`lint`, `unit`, `e2e`). The jobs only install tools and call the `cargo xtask`
subcommands above, so `cargo xtask ci` locally checks exactly what CI checks.
If a scenario fails, its serial log and screenshot are attached to the run as
the `e2e-logs` artefact. Hardware checks on the NUC stay manual
(`docs/hardware-test.md`); the pull-request template asks about them.

## Contributing

Commit messages and pull-request titles follow Conventional Commits
(`feat(kernel): …`, `fix(usb): …`); the types, scopes and rules are in
`CONTRIBUTING.md`.

## Layout

| Path | Contents |
|---|---|
| `boot/` | `relay-boot`, the UEFI loader (`BOOTX64.EFI`); it also loads `system.img` |
| `kernel/` | `relay-kernel`, the higher-half kernel |
| `crates/boot-info` | Loader → kernel hand-off structure |
| `crates/term` | Framebuffer text terminal |
| `crates/vfs` | Error numbers, block-device and filesystem traits, paths, mount table, in-memory filesystem |
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser, built-in commands and scripts (`sh FILE`) |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel and of user programs |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, heap, panic handler, ABI note, linker script; the shell's `Vfs`, `Console` and `System` over system calls |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `sh/` the shell (`/bin/sh`), `utils/` one per command (`cat`, `ls`, …), `tests/` the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
| `tests/e2e/` | QEMU end-to-end scenarios |
| `docs/hardware-test.md` | Manual checklist for the NUC |
| `.github/` | CI workflow and pull-request template |
| `CONTRIBUTING.md` | Commit-message and pull-request rules |
