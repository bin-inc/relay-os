# Development

## Ubuntu And WSL Setup

Install the host tools and pinned Rust toolchain:

```bash
sudo apt update
sudo apt install --yes build-essential e2fsprogs gdisk mtools qemu-system-x86 qemu-system-gui ovmf
rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy --target x86_64-unknown-uefi,x86_64-unknown-none
cargo install --locked cargo-deny --version 0.20.2
cargo install --locked cargo-audit --version 0.22.2
```

Run `cargo xtask doctor` before building. It prints each required host-tool
version and checks the two Rust targets.

## QEMU On WSL (Task 4)

QEMU commands are introduced in Task 4. Until then,
`cargo xtask qemu ...` exits with an unavailable-until-Task-4 error. Task 4's
`cargo xtask qemu boot target/relay-os.img --display gui --accel auto` opens a
WSLg window. It selects KVM only when `/dev/kvm` is readable and writable;
otherwise it uses TCG. To enable KVM, add the user to the `kvm` group, then run
`wsl --shutdown` from PowerShell before opening WSL again.

Task 4 also adds `cargo xtask qemu boot target/relay-os.img --display none --accel tcg`
for the reproducible headless mode.

For physical USB media, build the image in WSL and flash it with an explicit
Windows disk-imaging tool. Attaching a USB device with `usbipd-win` is an
advanced alternative and is never required to build Relay OS.

## Required PR Checks

Run the local supply-chain checks in this order so Cargo verifies the reviewed
lockfile before cargo-deny reads it:

```bash
cargo metadata --locked --format-version 1 --no-deps > /dev/null
cargo deny check advisories bans licenses sources
cargo audit --file Cargo.lock
```

After this workflow runs once, require these status checks for branch
protection: `host`, `targets`, and `supply-chain`.
