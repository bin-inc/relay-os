# Relay OS

A small operating system written from scratch in Rust. Milestone 1 boots
from a USB stick on an Intel NUC 12 Pro, shows a terminal over HDMI and
stores files on the stick's ext2 root filesystem.

Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

## Requirements (host)

Linux with `rustup`, `qemu-system-x86_64`, OVMF (`/usr/share/OVMF`),
`mtools`, `e2fsprogs`, `util-linux` (`sfdisk`) and `udisks2`. The Rust
toolchain and targets are installed automatically from `rust-toolchain.toml`.

## Common commands

| Command | What it does |
|---|---|
| `cargo xtask ci` | Everything a pull request must pass: lint, unit tests, QEMU scenarios |
| `cargo xtask lint` | `cargo fmt --check` and clippy (warnings are errors) on every crate |
| `cargo xtask unit` | Host unit tests |
| `cargo xtask test` | Host unit tests, then every QEMU scenario in `tests/e2e/` |
| `cargo xtask qemu` | Boot the image in a QEMU window (serial on this terminal) |
| `cargo xtask image` | Build `target/relay/relay-os.img` |
| `cargo xtask flash --full` | Erase and write the Kingston test stick |
| `cargo xtask flash --kernel` | Update loader and kernel on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick and list its files |

Set `RELAY_QEMU_ACCEL=tcg` to run QEMU without KVM.

## Continuous integration

Every pull request to `main` runs `.github/workflows/ci.yml` with three jobs
(`lint`, `unit`, `e2e`). The jobs only install tools and call the `cargo xtask`
subcommands above, so `cargo xtask ci` locally checks exactly what CI checks.
If a scenario fails, its serial log and screenshot are attached to the run as
the `e2e-logs` artefact. Hardware checks on the NUC stay manual
(`docs/hardware-test.md`); the pull-request template asks about them.

## Layout

| Path | Contents |
|---|---|
| `boot/` | `relay-boot`, the UEFI loader (`BOOTX64.EFI`) |
| `kernel/` | `relay-kernel`, the higher-half kernel |
| `crates/boot-info` | Loader → kernel hand-off structure |
| `crates/term` | Framebuffer text terminal |
| `xtask/` | Build, image, QEMU, test and flash tool |
| `rootfs/` | Files copied into `/` |
| `tests/e2e/` | QEMU end-to-end scenarios |
| `docs/hardware-test.md` | Manual checklist for the NUC |
| `.github/` | CI workflow and pull-request template |
