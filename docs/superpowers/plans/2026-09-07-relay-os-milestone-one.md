# Relay OS Milestone One Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust-first x86_64 OS that boots from one GPT USB image in QEMU and on the specified Intel NUC, then persistently manipulates an ext2 root filesystem through a framebuffer shell and native USB keyboard/storage drivers.

**Architecture:** Use a project-owned Rust UEFI loader and a modular monolithic `no_std` kernel. Keep hardware-independent block, GPT, ext2, VFS, shell, USB protocol, and xHCI bookkeeping code in a host-testable `relay-core` crate; keep privileged MMIO, DMA, PCI, controller, and framebuffer adapters in `relay-kernel`.

**Tech Stack:** Ubuntu on WSL2 with WSLg for local development, Ubuntu 24.04 GitHub-hosted CI, Rust 1.98.1, `x86_64-unknown-uefi`, `x86_64-unknown-none`, UEFI, ELF64, GPT, FAT32, ext2 revision 1, PCIe ECAM, xHCI, USB HID boot protocol, USB BOT, SCSI, QEMU q35/OVMF, e2fsprogs 1.47.2, GPT fdisk 1.0.10.

**Spec:** `docs/superpowers/specs/2026-09-07-relay-os-milestone-one-design.md`

## Global Constraints

- The physical acceptance target is Intel NUC 12 Pro `RNUC12WSHI70000`, Core i7-1260P, with Secure Boot disabled.
- UEFI must exit boot services before kernel entry and must not provide keyboard or storage I/O afterward.
- Project-owned code is Rust except for documented, architecture-required assembly in entry/exception transitions.
- The kernel is single-core, single-address-space, polling-based, and `no_std`; processes, scheduling, and user mode are excluded.
- Supported USB is one directly connected HID boot-protocol keyboard and one directly connected BOT/SCSI flash drive through xHCI; hubs, UAS, and hot-plug recovery are excluded.
- The USB drive exposes 512-byte logical sectors and contains a 64 MiB FAT32 ESP plus one 128 MiB ext2 root partition.
- Ext2 is revision 1, 4 KiB blocks, 256-byte inodes, one block group, `FILETYPE` as the only feature bit, direct plus singly indirect blocks, and a maximum file size of 4,243,456 bytes.
- Runtime dependencies require documented version, features, license, maintenance, RustSec status, and unsafe-code review in `docs/dependencies.md`.
- Every unsafe block must be isolated to an architecture/driver boundary and document the invariant its caller and implementation uphold.
- USB descriptors, GPT bytes, ext2 metadata, shell input, and every external length/count are untrusted and require checked arithmetic and bounds validation.
- Every mutation must preserve repairability under an injected write/flush failure; a failed write permanently disables later mutation for that mount.
- Milestone completion requires automated QEMU persistence checks and recorded physical NUC acceptance.
- Ubuntu on WSL2 is the primary local development environment; local GUI QEMU uses WSLg, while automated and CI QEMU runs are headless.
- Task 1 establishes required GitHub Actions checks. Every later task extends those checks in the same PR that adds its capability.
- Each task is intended to be delivered as one reviewable PR and must pass all CI checks available at that point.

## Work Packages

1. Tasks 1-5: reproducible workspace, image, UEFI handoff, kernel runtime, and console.
2. Tasks 6-10: host-tested block/GPT/ext2/VFS/shell vertical slice.
3. Tasks 11-13: ACPI/PCI/DMA, xHCI, USB enumeration, and HID input.
4. Tasks 14-15: BOT/SCSI block storage and full persistence integration.
5. Task 16: negative matrix, dependency evidence, and NUC release acceptance.

Do not start a later work package until the preceding package's final gate passes. In particular, do not build filesystem behavior against USB until host mutation images pass `e2fsck`, and do not build BOT on xHCI until a NUC probe confirms the controller assumptions.

## File Map

```text
Cargo.toml                         Workspace members and shared dependency versions
Cargo.lock                         Pinned dependency graph
rust-toolchain.toml                Rust 1.98.1 and both x86_64 targets
.cargo/config.toml                 xtask alias and target runner settings
deny.toml                          Advisory, license, source, and duplicate policy
docs/dependencies.md               Reviewed runtime dependency ledger
docs/development.md                Ubuntu/WSL setup, QEMU modes, and USB workflow
docs/acceptance/nuc-m1.md          Physical test procedure and recorded evidence

crates/relay-abi/                  Stable loader-to-kernel C-layout ABI
crates/relay-loader/               UEFI app, config/ELF validation, paging, handoff
crates/relay-core/                 no_std host-testable OS logic
crates/relay-kernel/               Kernel binary and privileged hardware adapters
xtask/                             Image construction, verification, QEMU/QMP harness
assets/root/                       Seeded ext2 root contents
tools/mke2fs.conf                  Controlled ext2 formatter configuration
.github/workflows/ci.yml           Host, supply-chain, image, and QEMU gates
```

### Task 1: Workspace, Toolchain, And Dependency Policy

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.cargo/config.toml`
- Create: `deny.toml`
- Create: `docs/dependencies.md`
- Create: `docs/development.md`
- Create: `.github/workflows/ci.yml`
- Create: `crates/relay-abi/Cargo.toml`
- Create: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-core/Cargo.toml`
- Create: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-loader/Cargo.toml`
- Create: `crates/relay-loader/src/lib.rs`
- Create: `crates/relay-loader/src/main.rs`
- Create: `crates/relay-kernel/Cargo.toml`
- Create: `crates/relay-kernel/src/main.rs`
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/lib.rs`
- Create: `xtask/src/main.rs`
- Create: `xtask/src/doctor.rs`
- Test: `xtask/tests/doctor.rs`

**Interfaces:**
- Consumes: Rust 1.98.1 and installed `rustfmt`, `clippy`, `x86_64-unknown-uefi`, and `x86_64-unknown-none` components.
- Produces: `cargo xtask doctor`, a locked Cargo workspace, crates that compile for their intended targets, and required baseline GitHub Actions checks for every PR.

- [ ] **Step 1: Write the failing tool-check test**

```rust
#[test]
fn doctor_reports_every_required_tool() {
    let report = relay_xtask::doctor::inspect_with(|name| match name {
        "rustc" => Some("rustc 1.98.1"),
        "mke2fs" => Some("mke2fs 1.47.2"),
        "sgdisk" => Some("GPT fdisk 1.0.10"),
        "qemu-system-x86_64" => None,
        _ => None,
    });
    assert_eq!(report.missing, ["qemu-system-x86_64"]);
    assert!(!report.ready());
}
```

- [ ] **Step 2: Run the test and confirm the crate is absent**

Run: `cargo test -p relay-xtask --test doctor --locked`

Expected: FAIL because the workspace and `relay_xtask::doctor` do not exist.

- [ ] **Step 3: Create the workspace and exact target policy**

Use resolver 2, Rust edition 2024, and these reviewed initial versions: `uefi 0.40.0`, `elf 0.8.0`, `x86_64 0.15.5`, `uart_16550 0.8.0`, `pci_types 0.10.1`, `xhci 0.9.2`, `crc 3.4.0`, `noto-sans-mono-bitmap 0.3.2`, `gpt 4.1.0`, `fatfs 0.3.6`, and `ovmf-prebuilt 0.2.9`. Disable default features on target-side crates and enable only required features. Add this crate root pattern to `relay-abi` and `relay-core`:

```rust
#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;
```

Implement `xtask/src/doctor.rs` with a `ToolReport { missing: Vec<&'static str> }`, `ready()`, production command probing, and the injected probe used by the test. Make `cargo xtask doctor` print versions and fail if any required tool or Rust target is missing.

- [ ] **Step 4: Document the Ubuntu/WSL development setup**

Document these local prerequisites in `docs/development.md`:

```bash
sudo apt update
sudo apt install --yes build-essential e2fsprogs gdisk mtools qemu-system-x86 qemu-system-gui ovmf
rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy --target x86_64-unknown-uefi,x86_64-unknown-none
cargo install --locked cargo-deny --version 0.20.2
cargo install --locked cargo-audit --version 0.22.2
```

Explain that `cargo xtask qemu boot target/relay-os.img --display gui --accel auto` opens a WSLg window and uses KVM only when `/dev/kvm` is readable and writable; otherwise it uses TCG. Document adding the user to the `kvm` group followed by `wsl --shutdown` from PowerShell. Document `--display none --accel tcg` as the reproducible headless mode. For physical USB media, prefer building in WSL and flashing the image with an explicit Windows disk-imaging tool; document `usbipd-win` attachment as an advanced alternative, never as a build requirement.

- [ ] **Step 5: Create the baseline pull-request workflow**

Create `.github/workflows/ci.yml` triggered by `pull_request` and pushes to `main`, with concurrency cancellation per branch and these required job names:

- `host`: checkout, install Rust 1.98.1, run `cargo fmt --all --check`, host Clippy, and workspace tests with `--locked`.
- `targets`: build the UEFI loader and freestanding kernel for their exact targets with `--locked`.
- `supply-chain`: install the pinned `cargo-deny` and `cargo-audit`, then run both against `Cargo.lock`.

Use `ubuntu-24.04`, `actions/checkout@v4`, least-privilege `contents: read`, and no write-capable pull-request token. Add the status names to `docs/development.md` as the branch-protection checks to require after the workflow first runs.

- [ ] **Step 6: Record dependency decisions**

For each runtime crate, add a table row to `docs/dependencies.md` containing version, enabled features, target/host scope, license, repository, maintenance evidence date, RustSec result, unsafe surface, and decision. Explicitly record that `xhci` supplies definitions rather than a complete driver and that `fatfs`, `gpt`, and `ovmf-prebuilt` are host-only.

- [ ] **Step 7: Run workspace policy checks**

Run:

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo build -p relay-loader --target x86_64-unknown-uefi --locked
cargo build -p relay-kernel --target x86_64-unknown-none --locked
cargo deny check advisories bans licenses sources
```

Expected: all commands pass. Before QEMU is installed, `cargo xtask doctor` may report it missing on a developer host, but its unit tests must pass. Validate the workflow syntax with `actionlint` when available and inspect that each job invokes only the documented locked commands.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml .cargo/config.toml deny.toml docs/dependencies.md docs/development.md .github/workflows/ci.yml crates xtask
git commit -m "build: establish Relay OS workspace"
```

### Task 2: Boot ABI And Deterministic Disk Layout

**Files:**
- Create: `crates/relay-abi/src/boot.rs`
- Create: `crates/relay-abi/src/guid.rs`
- Modify: `crates/relay-abi/src/lib.rs`
- Create: `xtask/src/layout.rs`
- Create: `xtask/src/image.rs`
- Create: `xtask/src/verify.rs`
- Modify: `xtask/src/lib.rs`
- Modify: `xtask/src/main.rs`
- Create: `tools/mke2fs.conf`
- Create: `assets/root/README.txt`
- Modify: `.github/workflows/ci.yml`
- Test: `crates/relay-abi/tests/layout.rs`
- Test: `xtask/tests/image.rs`

**Interfaces:**
- Consumes: workspace from Task 1.
- Produces: `BootInfo`, canonical `GptGuid`, fixed image constants, `cargo xtask image --output PATH`, and `cargo xtask verify-image PATH`.

- [ ] **Step 1: Write failing ABI and image-layout tests**

```rust
#[test]
fn boot_info_has_stable_layout() {
    assert_eq!(relay_abi::BOOT_INFO_MAGIC, 0x5245_4c41_5942_4f4f);
    assert_eq!(relay_abi::BOOT_ABI_VERSION, 1);
    assert_eq!(core::mem::align_of::<relay_abi::BootInfo>(), 8);
    assert_eq!(core::mem::size_of::<relay_abi::GptGuid>(), 16);
}

#[test]
fn image_geometry_is_fixed() {
    assert_eq!(relay_xtask::layout::SECTOR_SIZE, 512);
    assert_eq!(relay_xtask::layout::ESP_RANGE, 2_048..133_120);
    assert_eq!(relay_xtask::layout::ROOT_RANGE, 133_120..395_264);
    assert_eq!(relay_xtask::layout::DISK_SECTORS, 397_312);
}
```

- [ ] **Step 2: Run tests to verify missing APIs**

Run: `cargo test -p relay-abi -p relay-xtask --locked`

Expected: FAIL because ABI and layout symbols are undefined.

- [ ] **Step 3: Define the loader/kernel ABI**

Use only `#[repr(C)]` integer fields and addresses across the boundary:

```rust
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub abi_version: u32,
    pub struct_size: u32,
    pub memory_map: MemoryMap,
    pub framebuffer: FramebufferInfo,
    pub root_partition_guid: GptGuid,
    pub physical_memory_offset: u64,
    pub acpi_rsdp_phys: u64,
}

#[repr(C)]
pub struct MemoryMap {
    pub entries_address: u64,
    pub entry_count: u64,
}

#[repr(C)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: u32,
    pub reserved: u32,
}

#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GptGuid(pub [u8; 16]);

#[repr(C)]
pub struct FramebufferInfo {
    pub physical_base: u64,
    pub byte_len: u64,
    pub width: u32,
    pub height: u32,
    pub stride_pixels: u32,
    pub bytes_per_pixel: u32,
    pub pixel_format: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}
```

Assert `size_of::<MemoryRegion>() == 24`, `size_of::<FramebufferInfo>() == 56`, and `size_of::<BootInfo>() == 120`. Implement canonical ASCII GUID parsing/display and explicit GPT mixed-endian conversion with a known-vector test in `relay-abi`, so loader, GPT, and image tooling all use one representation.

- [ ] **Step 4: Build and verify the fixed image**

Use fixed identifiers:

```rust
pub const DISK_GUID: &str = "52454c41-5900-4000-8000-000000000001";
pub const ESP_GUID: &str = "52454c41-5900-4000-8000-000000000002";
pub const ROOT_GUID: &str = "52454c41-5900-4000-8000-000000000003";
pub const EXT2_UUID: &str = "52454c41-5900-4000-8000-000000000004";
```

Create the ESP and ext2 partition as standalone files, assemble them into a zero-filled raw GPT disk without loop devices, and write `/EFI/BOOT/BOOTX64.EFI`, `/EFI/RELAY/kernel.elf`, and `/EFI/RELAY/relay.cfg`. Run `mke2fs -F -t ext2 -b 4096 -g 32768 -I 256 -N 4096 -m 0 -O none,filetype -E lazy_itable_init=0,nodiscard,root_owner=0:0,root_perms=0755 -U 52454c41-5900-4000-8000-000000000004 target/root.ext2 32768` under the repository `MKE2FS_CONFIG` and fixed fake time. Verification must parse GPT and ext2 itself, run `sgdisk --verify`, extract root by LBA, and run `LC_ALL=C e2fsck -fn` requiring exit status 0.

- [ ] **Step 5: Prove deterministic output**

Run two clean image builds with `SOURCE_DATE_EPOCH=1788739200`, compare SHA-256 values and bytes, and assert the boot config's canonical root GUID equals the GPT entry's unique GUID.

Run: `cargo test -p relay-abi -p relay-xtask --locked && cargo xtask image --output target/a.img && cargo xtask image --output target/b.img && cmp target/a.img target/b.img`

Expected: PASS and no `cmp` output.

- [ ] **Step 6: Add the image CI gate**

Extend `.github/workflows/ci.yml` with an `image` job that installs `e2fsprogs`, `gdisk`, and `mtools`, builds the image twice, compares it byte-for-byte, runs `cargo xtask verify-image`, and uploads verification logs plus the image SHA-256. Make it depend on `host`, `targets`, and `supply-chain`.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-abi xtask tools/mke2fs.conf assets/root .github/workflows/ci.yml
git commit -m "feat: define boot ABI and disk image"
```

### Task 3: Strict Loader Configuration And ELF Planning

**Files:**
- Create: `crates/relay-loader/src/config.rs`
- Create: `crates/relay-loader/src/elf.rs`
- Create: `crates/relay-loader/src/error.rs`
- Modify: `crates/relay-loader/src/lib.rs`
- Modify: `crates/relay-loader/src/main.rs`
- Test: `crates/relay-loader/tests/config.rs`
- Test: `crates/relay-loader/tests/elf.rs`
- Test fixtures: `crates/relay-loader/tests/fixtures/`

**Interfaces:**
- Consumes: `relay_abi::GptGuid`.
- Produces: `parse_config(bytes) -> Result<LoaderConfig, ConfigError>` and `parse_load_plan(bytes) -> Result<LoadPlan, ElfLoadError>`.

- [ ] **Step 1: Write malformed-input tests first**

```rust
#[test]
fn config_requires_exactly_one_root_guid() {
    assert!(matches!(parse_config(b""), Err(ConfigError::MissingRootGuid)));
    assert!(matches!(
        parse_config(b"root-guid=bad\n"),
        Err(ConfigError::InvalidRootGuid)
    ));
}

#[test]
fn elf_rejects_writable_executable_segment() {
    let image = fixture_with_segment_flags(ELF_PF_R | ELF_PF_W | ELF_PF_X);
    assert_eq!(parse_load_plan(&image), Err(ElfLoadError::WritableExecutable));
}
```

- [ ] **Step 2: Run loader host tests and confirm failure**

Run: `cargo test -p relay-loader --tests --locked`

Expected: FAIL because both parsers are absent.

- [ ] **Step 3: Implement strict config parsing**

Accept one ASCII line `root-guid=<canonical-guid>`, one trailing newline, and no duplicate or unknown keys. Reject NUL, non-ASCII, whitespace around the key/value, malformed GUIDs, duplicate keys, missing newline separation, and files larger than 4 KiB.

- [ ] **Step 4: Implement the constrained ELF64 load plan**

```rust
pub struct LoadPlan {
    pub entry: u64,
    pub segments: alloc::vec::Vec<LoadSegment>,
}

pub struct LoadSegment {
    pub file_range: core::ops::Range<usize>,
    pub virtual_start: u64,
    pub memory_len: u64,
    pub flags: SegmentFlags,
}
```

Accept only little-endian ELF64, `EM_X86_64`, `ET_EXEC`, fixed high-half `PT_LOAD` segments, `p_filesz <= p_memsz`, page-congruent offsets, non-overlapping virtual ranges, no W+X segment, and an entry inside an executable segment. Check every addition, conversion, and file range. Ignore non-load headers and reject dynamic relocation requirements.

- [ ] **Step 5: Run parser tests and fuzz-sized corpus cases**

Run: `cargo test -p relay-loader --tests --locked`

Expected: all valid fixtures produce exact load plans; truncated and malformed fixtures return typed errors without panic.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-loader
git commit -m "feat: validate loader config and kernel ELF"
```

### Task 4: UEFI Handoff And Post-Boot-Services Kernel Entry

**Files:**
- Create: `crates/relay-loader/src/files.rs`
- Create: `crates/relay-loader/src/memory.rs`
- Create: `crates/relay-loader/src/paging.rs`
- Create: `crates/relay-loader/src/handoff.rs`
- Modify: `crates/relay-loader/src/lib.rs`
- Modify: `crates/relay-loader/src/main.rs`
- Create: `crates/relay-kernel/build.rs`
- Create: `crates/relay-kernel/x86_64-relay.ld`
- Create: `crates/relay-kernel/src/entry.rs`
- Create: `crates/relay-kernel/src/serial.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Create: `xtask/src/qemu.rs`
- Create: `xtask/src/qmp.rs`
- Modify: `xtask/src/lib.rs`
- Modify: `xtask/src/main.rs`
- Modify: `.github/workflows/ci.yml`
- Test: `xtask/tests/qemu_boot.rs`

**Interfaces:**
- Consumes: `BootInfo`, validated loader config/ELF plan, deterministic image.
- Produces: `unsafe extern "C" fn _start(*const BootInfo) -> !` and `cargo xtask qemu boot`.

- [ ] **Step 1: Write the failing post-exit boot test**

```rust
#[test]
fn production_image_reaches_kernel_after_exit_boot_services() {
    let run = QemuRun::boot_production_image(Duration::from_secs(20)).unwrap();
    run.wait_for_marker("[relay] phase=kernel-entry status=ok").unwrap();
    assert!(!run.serial_log().contains("phase=uefi-fallback"));
}
```

- [ ] **Step 2: Run the QEMU test and confirm no marker**

Run: `cargo test -p relay-xtask --test qemu_boot --locked -- --nocapture`

Expected: FAIL with a bounded missing-marker or missing-QEMU diagnostic.

- [ ] **Step 3: Load files and construct mappings**

Read config and kernel from the loader's own filesystem. Allocate and zero pages for each ELF segment, copy file bytes, map segment permissions, reserve loader/kernel/stack/page-table/boot-data pages, map usable RAM at `0xffff_8000_0000_0000`, and map the framebuffer with an appropriate non-normal cache attribute. Expose a checked kernel mapping API for PCI BARs discovered later rather than pre-mapping unknown MMIO. Reject address overflow and overlapping mappings.

- [ ] **Step 4: Capture platform data and exit UEFI**

Capture GOP metadata and ACPI 2.0 RSDP, falling back to ACPI 1.0. Obtain the final UEFI memory map in preallocated storage, call `exit_boot_services`, normalize entries without allocation, and build `BootInfo`. Keep runtime, ACPI NVS, MMIO, unknown, and all loader-owned pages reserved.

- [ ] **Step 5: Transfer to the kernel through the documented ABI**

Use the smallest required `naked_asm!` trampoline to load CR3/RSP, clear the direction flag, preserve the SysV argument register, and jump to the ELF entry with interrupts disabled. At `_start`, validate magic, ABI version, structure size, pointer alignment, memory ordering/ranges, framebuffer bounds, root GUID, physical offset, and RSDP before printing the serial marker.

- [ ] **Step 6: Run the production boot gate**

Run:

```bash
cargo build -p relay-loader --release --target x86_64-unknown-uefi --locked
cargo build -p relay-kernel --release --target x86_64-unknown-none --locked
cargo xtask image --output target/relay-os.img
cargo xtask qemu boot target/relay-os.img --display none --accel tcg
```

Expected: QEMU reports `[relay] phase=kernel-entry status=ok` after the loader reports exit from boot services.

- [ ] **Step 7: Add the headless QEMU CI gate**

Extend `.github/workflows/ci.yml` with `qemu-boot`. Install `qemu-system-x86`, run `cargo xtask qemu boot target/relay-os.img --display none --accel tcg`, enforce the same 20-second marker deadline as the host test, and upload serial, QMP, QEMU stderr, and framebuffer artifacts on failure. Keep GUI output disabled in CI.

- [ ] **Step 8: Commit**

```bash
git add crates/relay-loader crates/relay-kernel xtask .github/workflows/ci.yml
git commit -m "feat: enter kernel from UEFI"
```

### Task 5: Kernel Runtime, Exceptions, And Framebuffer Console

**Files:**
- Create: `crates/relay-core/src/console/mod.rs`
- Create: `crates/relay-core/src/console/framebuffer.rs`
- Create: `crates/relay-core/src/memory.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-kernel/src/arch/mod.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/mod.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/exceptions.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/exception_stubs.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/memory.rs`
- Create: `crates/relay-kernel/src/allocator.rs`
- Create: `crates/relay-kernel/src/console/mod.rs`
- Create: `crates/relay-kernel/src/console/framebuffer.rs`
- Create: `crates/relay-kernel/src/console/mirror.rs`
- Create: `docs/acceptance/nuc-m1.md`
- Test: `crates/relay-core/tests/framebuffer.rs`
- Test: `crates/relay-core/tests/memory.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Modify: `xtask/tests/qemu_boot.rs`

**Interfaces:**
- Consumes: validated `BootInfo` and mapped framebuffer.
- Produces: frame/page allocators, heap, exception diagnostics, `TextOutput`, and identical framebuffer/serial text.

- [ ] **Step 1: Write framebuffer model tests**

```rust
#[test]
fn console_wraps_scrolls_and_respects_stride() {
    let mut pixels = vec![0xaa; 8 * 6 * 4];
    let mut console = TestConsole::new(&mut pixels, 6, 4, 8);
    console.write_bytes(b"a\nb\nc\nd\n");
    assert_eq!(console.visible_lines(), ["b", "c", "d"]);
    assert!(console.padding_bytes_unchanged(0xaa));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test framebuffer --locked`

Expected: FAIL because the console model is absent.

- [ ] **Step 3: Add exception and allocation foundations**

Build GDT/IDT state with stable Rust plus minimal assembly stubs, install handlers before enabling fault-prone initialization, and print vector/error/RIP diagnostics before halting. Initialize a physical frame allocator only from usable normalized regions, reserve boot structures, then initialize a bounded kernel heap. Test allocator range splitting and exhaustion in `relay-core` with synthetic maps.

- [ ] **Step 4: Implement console boundaries**

```rust
pub trait TextOutput {
    fn write_bytes(&mut self, bytes: &[u8]);
}

pub struct Mirror<A, B> {
    pub primary: A,
    pub diagnostic: B,
}
```

Keep glyph placement, clipping, CR/LF/tab, wrapping, scrolling, pixel masks, and stride calculations in safe host-tested logic. Construct the volatile framebuffer slice in one documented unsafe adapter. Add a lock-independent panic writer.

- [ ] **Step 5: Verify QEMU console mirroring**

Run: `cargo test -p relay-core --locked && cargo xtask qemu boot target/relay-os.img`

Expected: host tests pass; QEMU serial contains the same stable boot banner rendered to the framebuffer, and a harness screenshot artifact is non-empty.

- [ ] **Step 6: Perform the first NUC boot gate**

Flash the image, disable Secure Boot, and record firmware version plus a photo/hash showing the post-UEFI kernel banner in `docs/acceptance/nuc-m1.md`. Do not start xHCI work if this gate fails.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core crates/relay-kernel xtask docs/acceptance/nuc-m1.md
git commit -m "feat: add kernel runtime and console"
```

### Task 6: Block Device, GUID, And GPT Selection

**Files:**
- Create: `crates/relay-core/src/block/mod.rs`
- Create: `crates/relay-core/src/block/partition.rs`
- Create: `crates/relay-core/src/gpt.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/tests/support/mod.rs`
- Create: `crates/relay-core/tests/support/memory_device.rs`
- Test: `crates/relay-core/tests/block.rs`
- Test: `crates/relay-core/tests/gpt.rs`

**Interfaces:**
- Consumes: canonical `relay_abi::GptGuid` and 512-byte sector profile.
- Produces: synchronous `BlockDevice`, bounded `PartitionDevice`, and `find_partition_by_unique_guid`.

- [ ] **Step 1: Write exact-I/O and GPT endian tests**

```rust
#[test]
fn partition_rejects_end_overflow_without_io() {
    let inner = MemoryDevice::new(512, 16);
    let mut part = PartitionDevice::new(inner, PartitionRange { first_lba: 8, sector_count: 8 }).unwrap();
    assert_eq!(part.read_sectors(7, &mut [0; 1024]), Err(BlockError::OutOfRange));
    assert_eq!(part.inner().read_count(), 0);
}

#[test]
fn guid_decodes_gpt_mixed_endian_fields() {
    let guid = relay_abi::GptGuid::from_gpt_bytes([
        0x41, 0x4c, 0x45, 0x52, 0x59, 0x00, 0x00, 0x40,
        0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
    ]);
    assert_eq!(guid.to_string(), "52454c41-5900-4000-8000-000000000003");
}
```

- [ ] **Step 2: Run tests and confirm missing types**

Run: `cargo test -p relay-core --test block --test gpt --locked`

Expected: FAIL because block/GPT modules are absent.

- [ ] **Step 3: Define synchronous exact block I/O**

```rust
pub struct BlockGeometry {
    pub logical_sector_size: u32,
    pub sector_count: u64,
}

pub trait BlockDevice {
    fn geometry(&self) -> BlockGeometry;
    fn read_sectors(&mut self, first_lba: u64, dst: &mut [u8]) -> Result<(), BlockError>;
    fn write_sectors(&mut self, first_lba: u64, src: &[u8]) -> Result<(), BlockError>;
    fn flush(&mut self) -> Result<(), BlockError>;
}
```

Require exact sector multiples, checked `lba + count`, and typed invalid-request, out-of-range, short-transfer, timeout, transport, read-only, and flush errors. Bound partition-relative accesses before forwarding them.

- [ ] **Step 4: Parse and cross-check both GPT copies**

Validate signatures, revisions, header sizes/CRCs, current/alternate LBAs, usable ranges, entry sizes/counts, complete entry-array CRCs, and all arithmetic. Match exactly one non-empty entry by unique GUID, not type GUID. Accept one valid GPT copy with a degraded diagnostic; reject conflicting valid copies or duplicate GUID matches.

- [ ] **Step 5: Run focused and image round-trip tests**

Run: `cargo test -p relay-core --test block --test gpt --locked && cargo xtask verify-image target/relay-os.img`

Expected: valid generated image selects `133120..395264`; every CRC, truncation, overflow, duplicate, and wrong-GUID fixture returns its exact error.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-core xtask
git commit -m "feat: add block and GPT abstractions"
```

### Task 7: Ext2 Profile Validation And Read Path

**Files:**
- Create: `crates/relay-core/src/fs.rs`
- Create: `crates/relay-core/src/ext2/mod.rs`
- Create: `crates/relay-core/src/ext2/on_disk.rs`
- Create: `crates/relay-core/src/ext2/validate.rs`
- Create: `crates/relay-core/src/ext2/inode.rs`
- Create: `crates/relay-core/src/ext2/directory.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/tests/support/file_device.rs`
- Create: `crates/relay-core/tests/support/ext2_image.rs`
- Test: `crates/relay-core/tests/ext2_mount.rs`
- Test: `crates/relay-core/tests/ext2_read.rs`

**Interfaces:**
- Consumes: `BlockDevice` over the extracted root partition.
- Produces: `Ext2<D>::mount`, opaque `NodeId`, metadata, lookup, directory listing, and offset reads.

- [ ] **Step 1: Write profile rejection and block-boundary tests**

```rust
#[test]
fn mount_rejects_any_unapproved_feature_bit() {
    for (field, bit) in unsupported_feature_bits() {
        let image = fixture_with_feature(field, bit);
        assert_eq!(Ext2::mount(image, MountMode::ReadOnly), Err(Ext2Error::UnsupportedFeature));
    }
}

#[test]
fn read_crosses_direct_to_single_indirect() {
    let mut fs = fixture_with_pattern_file(13 * 4096);
    let node = fs.lookup(fs.root(), name("boundary")).unwrap();
    let mut bytes = [0; 2];
    assert_eq!(fs.read_at(node, 12 * 4096 - 1, &mut bytes).unwrap(), 2);
    assert_eq!(bytes, [0xff, 0x00]);
}
```

- [ ] **Step 2: Run ext2 tests and verify failure**

Run: `cargo test -p relay-core --test ext2_mount --test ext2_read --locked`

Expected: FAIL because ext2 is absent.

- [ ] **Step 3: Parse disk records without packed references**

Read little-endian fields from byte slices with checked offsets. Enforce magic, revision 1, 4 KiB block/fragment size, 32,768 blocks and blocks/group, one group, 256-byte inodes, exact feature masks, valid metadata ranges, root inode 2, supported inode modes/flags, and no double/triple indirect pointers or sparse ranges below `i_size`. Refuse read-write mount when ext2 is dirty or error-marked so the user must repair it with host `e2fsck` first.

- [ ] **Step 4: Implement inode and directory reads**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub(crate) u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name(alloc::vec::Vec<u8>);

pub struct Metadata {
    pub kind: NodeKind,
    pub len: u64,
    pub mode: u16,
}

pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

impl<D: BlockDevice> Ext2<D> {
    pub fn mount(device: D, mode: MountMode) -> Result<Self, Ext2Error>;
    pub fn root(&self) -> NodeId;
    pub fn metadata(&mut self, node: NodeId) -> Result<Metadata, Ext2Error>;
    pub fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, Ext2Error>;
    pub fn read_at(&mut self, node: NodeId, offset: u64, dst: &mut [u8]) -> Result<usize, Ext2Error>;
}
```

Support twelve direct blocks and one singly indirect block. Validate every referenced block before reading. Parse linear `FILETYPE` directory records with aligned nonzero `rec_len`, bounded `name_len`, no cross-block entry, valid inode number, and regular/directory type only. Preserve all 256 inode bytes on updates in later tasks.

- [ ] **Step 5: Verify host read compatibility**

Run: `cargo test -p relay-core --test ext2_mount --test ext2_read --locked && LC_ALL=C e2fsck -fn target/root.ext2`

Expected: all tests pass and `e2fsck` exits 0.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-core
git commit -m "feat: read controlled ext2 filesystems"
```

### Task 8: VFS Traversal And Read-Only Shell

**Files:**
- Create: `crates/relay-core/src/vfs/mod.rs`
- Create: `crates/relay-core/src/vfs/path.rs`
- Create: `crates/relay-core/src/shell/mod.rs`
- Create: `crates/relay-core/src/shell/parser.rs`
- Create: `crates/relay-core/src/shell/commands.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Test: `crates/relay-core/tests/vfs.rs`
- Test: `crates/relay-core/tests/shell_parser.rs`
- Test: `crates/relay-core/tests/shell_read.rs`

**Interfaces:**
- Consumes: ext2 node operations and `TextOutput`.
- Produces: `FileSystem`, `Vfs<F>`, `Cwd`, `parse_command`, and `Shell<F, O>` for `help`, `pwd`, `cd`, `ls`, `cat`, and `echo`.

- [ ] **Step 1: Write traversal and parser tests**

```rust
#[test]
fn traversal_does_not_lexically_cancel_missing_component() {
    let mut vfs = fixture_vfs();
    let cwd = vfs.initial_cwd();
    assert_eq!(vfs.resolve(&cwd, "missing/../file"), Err(VfsError::NotFound));
}

#[test]
fn quoted_fragments_and_empty_arguments_are_preserved() {
    assert_eq!(tokenize("write a x\" y\" ''").unwrap(), ["write", "a", "x y", ""]);
    assert_eq!(parse_command("write a ''").unwrap(), Some(Command::Write { path: "a".into(), text: "".into() }));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test vfs --test shell_parser --test shell_read --locked`

Expected: FAIL because VFS/shell types are absent.

- [ ] **Step 3: Define filesystem and path contracts**

```rust
pub trait FileSystem {
    fn root(&self) -> NodeId;
    fn metadata(&mut self, node: NodeId) -> Result<Metadata, FsError>;
    fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, FsError>;
    fn read_dir(&mut self, dir: NodeId) -> Result<Vec<DirEntry>, FsError>;
    fn read_at(&mut self, node: NodeId, offset: u64, dst: &mut [u8]) -> Result<usize, FsError>;
    fn write_at(&mut self, node: NodeId, offset: u64, src: &[u8]) -> Result<(), FsError>;
    fn truncate(&mut self, node: NodeId, len: u64) -> Result<(), FsError>;
    fn create_file(&mut self, parent: NodeId, name: &Name) -> Result<NodeId, FsError>;
    fn create_dir(&mut self, parent: NodeId, name: &Name) -> Result<NodeId, FsError>;
    fn unlink_file(&mut self, parent: NodeId, name: &Name) -> Result<(), FsError>;
    fn remove_dir(&mut self, parent: NodeId, name: &Name) -> Result<(), FsError>;
    fn sync(&mut self) -> Result<(), FsError>;
    fn unmount(&mut self) -> Result<(), FsError>;
}
```

Walk components against the filesystem. Clamp root `..`; reject invalid final creation names, NUL, non-printable/non-ASCII bytes, 256-byte names, and regular-file paths with trailing slash. Keep canonical absolute components in `Cwd`.

- [ ] **Step 4: Parse and dispatch read commands**

Implement exact quote/backslash rules from the spec, typed arity errors, empty-line `Ok(None)`, and command enum variants for all milestone commands. Dispatch the six read-only commands now. Render `cat` printable ASCII, LF, CR, and tab directly; render every other byte as `?`.

- [ ] **Step 5: Run the read-only host vertical slice**

Run: `cargo test -p relay-core --test vfs --test shell_parser --test shell_read --locked`

Expected: tests drive a file-backed ext2 fixture through `pwd`, `cd`, `ls`, `cat`, `echo`, and `help` and compare exact output.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-core
git commit -m "feat: add VFS and read-only shell"
```

### Task 9: Ext2 Mutation, Ordering, And Failure Poisoning

**Files:**
- Create: `crates/relay-core/src/ext2/allocator.rs`
- Create: `crates/relay-core/src/ext2/mutation.rs`
- Modify: `crates/relay-core/src/ext2/mod.rs`
- Create: `crates/relay-core/tests/support/fault_device.rs`
- Create: `crates/relay-core/tests/support/power_cut_device.rs`
- Test: `crates/relay-core/tests/ext2_files.rs`
- Test: `crates/relay-core/tests/ext2_directories.rs`
- Test: `crates/relay-core/tests/ext2_faults.rs`

**Interfaces:**
- Consumes: ext2 read path and full `FileSystem` trait.
- Produces: create/truncate/write/append/mkdir/unlink/rmdir/sync/unmount and `MountHealth` poisoning.

- [ ] **Step 1: Write mutation and poison tests first**

```rust
#[test]
fn failed_metadata_write_disables_later_mutation() {
    let device = FaultDevice::fail_write(known_clean_fixture(), 3);
    let mut fs = Ext2::mount(device, MountMode::ReadWrite).unwrap();
    assert!(fs.create_file(fs.root(), name("a")).is_err());
    assert_eq!(fs.health(), MountHealth::WriteFailed);
    assert_eq!(fs.create_file(fs.root(), name("b")), Err(FsError::WriteDisabled));
}

#[test]
fn maximum_file_size_is_exact() {
    assert!(write_fixture(4_243_456).is_ok());
    assert_eq!(write_fixture(4_243_457), Err(FsError::FileTooLarge));
}
```

- [ ] **Step 2: Run mutation tests and verify failure**

Run: `cargo test -p relay-core --test ext2_files --test ext2_directories --test ext2_faults --locked`

Expected: FAIL because mutation is absent.

- [ ] **Step 3: Implement ordered allocation and file mutation**

On read-write mount, clear `EXT2_VALID_FS` and flush before exposing mutation. Allocate bitmap ownership before publishing pointers; initialize and flush data/indirect blocks before inode pointers; publish pointers before increasing size. On truncate/removal, remove references and flush before freeing bits. Count `i_blocks` in 512-byte sectors and include the indirect block. Update superblock and group free counters together.

- [ ] **Step 4: Implement directory mutation**

Create an inode and initialized contents before inserting its parent entry. For `mkdir`, write `.`/`..`, set link counts and `bg_used_dirs_count`, flush, then expose it. For removal, delete/coalesce the parent entry and flush before clearing inode pointers and allocation bits. Reject non-empty directories, root removal, current directory, and cwd ancestor removal.

- [ ] **Step 5: Enforce health and clean unmount**

```rust
pub enum MountHealth {
    ReadOnly,
    Writable,
    WriteFailed,
    Unmounted,
}
```

Route every mutation through one health guard. Any uncertain write or flush changes health to `WriteFailed`; do not attempt rollback. `sync` orders dirty metadata then calls device flush. Clean unmount performs sync, flushes, sets `EXT2_VALID_FS`, writes the superblock, flushes again, and only then changes health to `Unmounted`.

- [ ] **Step 6: Sweep all persistence cut points**

For each mutation, fail every write/flush index and simulate power cuts by discarding unflushed overlays. On successful cases require `e2fsck -fn` status 0 after reopen. On cut cases run repair on a disposable copy, then require a second `e2fsck -fn` status 0 and no duplicate allocated block ownership.

Run: `cargo test -p relay-core --test ext2_files --test ext2_directories --test ext2_faults --locked -- --nocapture`

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core
git commit -m "feat: add durable ext2 mutation"
```

### Task 10: Mutating Shell And Line Editor

**Files:**
- Modify: `crates/relay-core/src/shell/commands.rs`
- Modify: `crates/relay-core/src/console/mod.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/src/console/line_editor.rs`
- Test: `crates/relay-core/tests/shell_mutation.rs`
- Test: `crates/relay-core/tests/line_editor.rs`

**Interfaces:**
- Consumes: mutating `FileSystem`, `Vfs`, and `TextOutput`.
- Produces: every documented shell command and USB-independent `LineEditor` consuming semantic `Key` values.

- [ ] **Step 1: Write end-to-end command and editing tests**

```rust
#[test]
fn commands_persist_after_reopen() {
    let mut shell = shell_on_fixture();
    shell.run("mkdir /docs").unwrap();
    shell.run("write /docs/hello 'relay os'").unwrap();
    shell.run("append /docs/hello '!'").unwrap();
    shell.run("sync").unwrap();
    assert_eq!(reopen_and_read("/docs/hello"), b"relay os!");
}

#[test]
fn editor_inserts_and_moves_cursor() {
    let mut editor = LineEditor::new(128);
    feed(&mut editor, [key('a'), key('c'), Key::Left, key('b'), Key::Enter]);
    assert_eq!(editor.take_submitted(), Some("abc"));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test shell_mutation --test line_editor --locked`

Expected: FAIL because mutation dispatch and line editing are incomplete.

- [ ] **Step 3: Dispatch all mutation commands**

Implement `touch`, `write`, `append`, `mkdir`, `rm`, `rmdir`, `sync`, and `shutdown` with exact arity and errors. `touch` creates a missing `0644` root-owned file and leaves an existing regular file unchanged. `write` truncates before writing joined text; `append` starts at metadata length; both create absent files and add no newline. `mkdir` creates a root-owned `0755` directory. `shutdown` returns a distinct control-flow outcome only after `unmount()` succeeds.

- [ ] **Step 4: Implement USB-independent line editing**

```rust
pub enum Key {
    Printable(u8),
    Backspace,
    Enter,
    Left,
    Right,
}

pub enum EditAction {
    Redraw,
    Submitted(alloc::string::String),
    Ignored,
}
```

Enforce a fixed 512-byte input limit, insert at cursor, delete before cursor, and keep cursor within ASCII byte boundaries. Rendering code redraws the prompt and line through `TextOutput`; it does not know USB or framebuffer geometry.

- [ ] **Step 5: Run shell, reopen, and fsck gates**

Run: `cargo test -p relay-core --test shell_mutation --test line_editor --locked && LC_ALL=C e2fsck -fn target/root.ext2`

Expected: all tests pass and fsck exits 0.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-core
git commit -m "feat: complete shell file commands"
```

### Task 11: ACPI MCFG, PCI Discovery, And DMA Boundary

**Files:**
- Create: `crates/relay-core/src/acpi.rs`
- Create: `crates/relay-core/src/pci.rs`
- Create: `crates/relay-core/src/dma.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/mmio.rs`
- Create: `crates/relay-kernel/src/arch/x86_64/dma.rs`
- Create: `crates/relay-kernel/src/pci.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Modify: `docs/acceptance/nuc-m1.md`
- Test: `crates/relay-core/tests/acpi.rs`
- Test: `crates/relay-core/tests/pci.rs`
- Test: `crates/relay-core/tests/dma.rs`

**Interfaces:**
- Consumes: boot RSDP physical address, physical mapper, and frame allocator.
- Produces: checked MCFG regions, `PciConfig`, `XhciPciDevice`, and `DmaAllocator`.

- [ ] **Step 1: Write malformed-table, BAR, and DMA tests**

```rust
#[test]
fn mcfg_rejects_bad_child_checksum() {
    assert_eq!(parse_mcfg(&memory_with_bad_mcfg(), RSDP_ADDR), Err(AcpiError::Checksum));
}

#[test]
fn bar_probe_restores_registers_on_error() {
    let config = RecordingPciConfig::with_64_bit_bar_above_4g();
    let result = probe_xhci_bar(&config, XHCI_ADDRESS);
    assert!(result.is_ok());
    assert!(config.original_command_and_bars_restored_before_enable());
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test acpi --test pci --test dma --locked`

Expected: FAIL because platform parsers are absent.

- [ ] **Step 3: Parse ACPI and enumerate PCIe ECAM**

Validate RSDP, XSDT/RSDT, and MCFG signatures, lengths, checksums, segment/bus ranges, and ECAM arithmetic. Enumerate functions including multifunction devices. Identify xHCI by class `0x0c`, subclass `0x03`, programming interface `0x30`. Report a specific unsupported-platform error when MCFG is unavailable.

- [ ] **Step 4: Probe and map the xHCI BAR safely**

Save command/BAR registers, disable memory decode and bus mastering, probe and restore both halves of 64-bit BARs on every return path, reject I/O/reserved/zero/overflow/unaligned BARs, map MMIO uncached, then enable memory decode. Delay bus mastering until controller DMA structures are ready.

- [ ] **Step 5: Implement the only DMA allocation boundary**

```rust
pub struct DmaLayout {
    pub size: usize,
    pub align: usize,
    pub max_address: u64,
    pub zeroed: bool,
}

pub trait DmaAllocator {
    fn allocate(&mut self, layout: DmaLayout) -> Result<DmaAllocation, DmaError>;
}

pub struct DmaAllocation {
    pub cpu_address: core::ptr::NonNull<u8>,
    pub device_address: u64,
    pub len: usize,
}

pub trait PhysicalMemory {
    fn read_exact(&self, physical: u64, output: &mut [u8]) -> Result<(), MemoryError>;
}

pub trait PciConfig {
    fn read_u32(&self, address: PciAddress, offset: u16) -> Result<u32, PciError>;
    unsafe fn write_u32(&self, address: PciAddress, offset: u16, value: u32) -> Result<(), PciError>;
}
```

Guarantee contiguous physical pages, alignment, address limit, zeroing, stable CPU/device addresses, and allocation lifetime. Use long-lived DMA bounce buffers rather than arbitrary kernel slices.

- [ ] **Step 6: Run host tests and a NUC platform probe**

The diagnostic kernel records MCFG range, xHCI BDF/BAR width/address, VT-d firmware state, context size, scratchpad count, address-width capability, legacy ownership bits, and USB2/USB3 port-protocol ranges in `docs/acceptance/nuc-m1.md`. If VT-d translation remains active and blocks DMA, stop execution and obtain explicit design approval before either adding an identity-mapped DMA domain or requiring VT-d to be disabled in firmware.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core crates/relay-kernel docs/acceptance
git commit -m "feat: discover xHCI platform resources"
```

### Task 12: xHCI Rings And Polling Controller

**Files:**
- Create: `crates/relay-core/src/xhci/mod.rs`
- Create: `crates/relay-core/src/xhci/ring.rs`
- Create: `crates/relay-core/src/xhci/context.rs`
- Create: `crates/relay-kernel/src/xhci/mod.rs`
- Create: `crates/relay-kernel/src/xhci/controller.rs`
- Create: `crates/relay-kernel/src/xhci/port.rs`
- Create: `crates/relay-kernel/src/xhci/transfer.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Test: `crates/relay-core/tests/xhci_ring.rs`
- Test: `crates/relay-core/tests/xhci_context.rs`
- Test: `xtask/tests/qemu_xhci.rs`

**Interfaces:**
- Consumes: `XhciPciDevice`, MMIO mapper, DMA allocator, and monotonic `Clock`.
- Produces: initialized polling `XhciController` with command, control, bulk, interrupt, and root-port operations.

- [ ] **Step 1: Write ring wrap and context tests**

```rust
#[test]
fn producer_wrap_toggles_cycle_after_link_trb() {
    let mut ring = TestRing::new(4);
    ring.push(noop()).unwrap();
    ring.push(noop()).unwrap();
    ring.push(noop()).unwrap();
    assert_eq!(ring.enqueue_index(), 0);
    assert!(!ring.producer_cycle());
}

#[test]
fn context_offset_honors_controller_context_size() {
    assert_eq!(device_context_offset(3, ContextSize::Bytes32), 96);
    assert_eq!(device_context_offset(3, ContextSize::Bytes64), 192);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test xhci_ring --test xhci_context --locked`

Expected: FAIL because xHCI bookkeeping is absent.

- [ ] **Step 3: Implement bounded rings and contexts**

Use one command ring, one event-ring segment, one interrupter, and one transfer ring per endpoint. Handle Link TRB toggle-cycle, full-ring detection, event consumer wrap, stale-cycle rejection, command/transfer correlation, 32/64-byte contexts, DCI calculation, EP0 packet sizes, scratchpad count, and controller page-size selection.

- [ ] **Step 4: Initialize the controller in hardware order**

Walk extended capabilities with a cycle bound; perform BIOS/OS ownership handoff; disable legacy SMI after ownership; halt/reset/wait-not-ready with deadlines; allocate scratchpads/DCBAA/rings/ERST; program registers; set bounded `MaxSlotsEn`; enable bus mastering; start and verify non-halted state.

- [ ] **Step 5: Expose blocking polling transfers**

Define the controller boundary:

```rust
pub struct Deadline(pub u64);

pub trait Clock {
    fn now_ticks(&self) -> u64;
    fn ticks_per_second(&self) -> u64;
}

#[derive(Clone, Copy)]
pub struct DeviceHandle {
    pub slot_id: u8,
    pub root_port: u8,
    pub speed: UsbSpeed,
}

pub struct EndpointConfig {
    pub address: EndpointAddress,
    pub transfer_type: TransferType,
    pub max_packet_size: u16,
    pub interval: u8,
    pub max_burst: u8,
}

impl XhciController {
    pub fn initialize(pci: XhciPciDevice, dma: &mut impl DmaAllocator, clock: &impl Clock) -> Result<Self, XhciError>;
    pub fn poll(&mut self) -> Result<usize, XhciError>;
    pub fn connected_root_ports(&self, output: &mut [u8]) -> Result<usize, XhciError>;
    pub fn reset_and_address(&mut self, port: u8, deadline: Deadline) -> Result<DeviceHandle, XhciError>;
    pub fn configure_endpoints(&mut self, device: DeviceHandle, endpoints: &[EndpointConfig], deadline: Deadline) -> Result<(), XhciError>;
}
```

Control, bulk, and interrupt methods take a `DeviceHandle`, endpoint, setup packet where applicable, a DMA allocation, and a deadline. Each method queues only DMA-backed buffers, repeatedly calls `poll()`, returns only its correlated completion, and times out with a typed error. Implement root-port reset/address and endpoint configuration without USB class policy.

- [ ] **Step 6: Run QEMU and NUC controller gates**

Run: `cargo test -p relay-core --test xhci_ring --test xhci_context --locked && cargo xtask qemu xhci target/relay-os.img`

Expected: host tests pass; QEMU reports ownership, reset, capabilities, and directly connected ports without controller errors. Repeat the diagnostic on the NUC and compare all previously recorded capability assumptions.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core crates/relay-kernel xtask
git commit -m "feat: initialize polling xHCI controller"
```

### Task 13: USB Enumeration, HID Keyboard, And Shell Input

**Files:**
- Create: `crates/relay-core/src/usb/mod.rs`
- Create: `crates/relay-core/src/usb/descriptor.rs`
- Create: `crates/relay-core/src/usb/hid.rs`
- Create: `crates/relay-kernel/src/usb/mod.rs`
- Create: `crates/relay-kernel/src/usb/enumerate.rs`
- Create: `crates/relay-kernel/src/usb/keyboard.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Modify: `docs/acceptance/nuc-m1.md`
- Test: `crates/relay-core/tests/usb_descriptor.rs`
- Test: `crates/relay-core/tests/hid.rs`
- Test: `xtask/tests/qemu_keyboard.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: xHCI root-port/control/interrupt API and `LineEditor` through semantic `Key` values.
- Produces: validated device/configuration descriptors and one boot keyboard feeding shell lines.

- [ ] **Step 1: Write descriptor and HID transition tests**

```rust
#[test]
fn parser_keeps_endpoints_with_their_interface() {
    let config = parse_configuration(&composite_fixture()).unwrap();
    let keyboard = config.boot_keyboard().unwrap();
    assert_eq!(keyboard.interrupt_in.address, 0x83);
}

#[test]
fn held_key_is_not_repeated_without_a_new_report_transition() {
    let mut keyboard = BootKeyboard::new();
    assert_eq!(keyboard.parse_report(&[0, 0, 4, 0, 0, 0, 0, 0]).unwrap().len(), 1);
    assert_eq!(keyboard.parse_report(&[0, 0, 4, 0, 0, 0, 0, 0]).unwrap().len(), 0);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test usb_descriptor --test hid --locked`

Expected: FAIL because USB protocol modules are absent.

- [ ] **Step 3: Enumerate every connected root port**

Reset/address each port, read the first 8 device bytes, update EP0 max packet size, read full device/config descriptors with validated `wTotalLength`, preserve interfaces and alternate settings, select supported interfaces, send `SET_CONFIGURATION`, and configure matching endpoint contexts. Do not assume the first internal NUC USB device is keyboard or storage.

- [ ] **Step 4: Bind the narrow keyboard profile**

Require class 3, subclass 1, protocol 1, and one interrupt-IN endpoint. Send HID `SET_PROTOCOL(boot)`. Parse exact 8-byte reports, reject rollover, compare prior/current key sets, map US Shift and printable keys plus Backspace/Enter/Left/Right, and feed semantic keys to `LineEditor`.

- [ ] **Step 5: Exercise only USB keyboard input in QEMU**

Configure explicit `qemu-xhci` ports, connect QMP, issue `qmp_capabilities`, continue, and inject key presses with QMP `send-key`. Type `help`, an edited `echo`, quoted text, and escaped characters. Require stable mirrored markers and exact shell output; serial input must remain disabled.

Run: `cargo test -p relay-core --test usb_descriptor --test hid --locked && cargo xtask qemu keyboard target/relay-os.img --display none --accel tcg`

Extend the required QEMU workflow command to include the keyboard scenario in the same PR, still using TCG and no display.

- [ ] **Step 6: Verify keyboard on the NUC**

Record keyboard model and VID:PID, physical port, successful prompt editing keys, and output photo/log in `docs/acceptance/nuc-m1.md`.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core crates/relay-kernel xtask docs/acceptance/nuc-m1.md .github/workflows/ci.yml
git commit -m "feat: drive shell with USB keyboard"
```

### Task 14: USB BOT, SCSI, And Block Adapter

**Files:**
- Create: `crates/relay-core/src/usb/bot.rs`
- Create: `crates/relay-core/src/usb/scsi.rs`
- Create: `crates/relay-kernel/src/usb/storage.rs`
- Modify: `crates/relay-core/src/usb/mod.rs`
- Modify: `crates/relay-kernel/src/usb/mod.rs`
- Modify: `crates/relay-kernel/src/main.rs`
- Modify: `docs/acceptance/nuc-m1.md`
- Test: `crates/relay-core/tests/bot.rs`
- Test: `crates/relay-core/tests/scsi.rs`
- Test: `xtask/tests/qemu_storage.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: enumerated USB interfaces, xHCI control/bulk API, and `BlockDevice`.
- Produces: BOT `ScsiTransport` and a USB `BlockDevice` whose `flush` reaches `SYNCHRONIZE CACHE (10)`.

- [ ] **Step 1: Write BOT validation and SCSI capacity tests**

```rust
#[test]
fn csw_requires_matching_signature_tag_and_residue() {
    let expected = BotTag(7);
    assert_eq!(parse_csw(&bad_tag_csw(), expected, 512), Err(BotError::TagMismatch));
    assert_eq!(parse_csw(&excess_residue_csw(), expected, 512), Err(BotError::InvalidResidue));
}

#[test]
fn capacity_is_last_lba_plus_one() {
    assert_eq!(decode_capacity10([0, 0, 0, 15, 0, 0, 2, 0]).unwrap(), BlockGeometry { logical_sector_size: 512, sector_count: 16 });
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p relay-core --test bot --test scsi --locked`

Expected: FAIL because BOT/SCSI are absent.

- [ ] **Step 3: Implement strict BOT framing and one reset recovery**

Require interface class `0x08`, subclass `0x06`, protocol `0x50`, and bulk IN/OUT endpoints belonging to that interface. Serialize exact 31-byte CBWs; accept exact 13-byte CSWs only; validate signature `0x53425355`, tag, residue, and status. On eligible stall/phase failures, perform Mass Storage Reset, clear both endpoint halts, reset xHCI endpoint/dequeue state, and retry once only when the command is safe to repeat.

- [ ] **Step 4: Implement the narrow SCSI set**

```rust
pub enum ScsiData<'a> {
    In(&'a mut [u8]),
    Out(&'a [u8]),
    None,
}

pub trait ScsiTransport {
    fn command(&mut self, cdb: &[u8], data: ScsiData<'_>, deadline: Deadline) -> Result<usize, ScsiError>;
}
```

Implement TEST UNIT READY, REQUEST SENSE, INQUIRY, READ CAPACITY (10), READ CAPACITY (16) fallback, READ (10), WRITE (10), and SYNCHRONIZE CACHE (10). Check `last_lba + 1`, operation ranges, transfer lengths, sense status, and exact 512-byte logical sectors. Unsupported/failed cache synchronization makes `BlockDevice::flush` fail.

- [ ] **Step 5: Boot from and re-enumerate QEMU USB storage**

Attach the production image as `usb-storage` with `bootindex=1`. Require UEFI boot, post-exit xHCI reset, BOT re-enumeration, capacity match, GPT root GUID match, ext2 mount, and `cat /README.txt` typed through USB keyboard.

Run: `cargo test -p relay-core --test bot --test scsi --locked && cargo xtask qemu storage target/relay-os.img --display none --accel tcg`

Extend the required QEMU workflow command to include the storage scenario in the same PR.

- [ ] **Step 6: Verify read-only storage on the NUC**

Record flash drive model, VID:PID, physical port, negotiated speed, capacity, GPT root GUID, and seeded file output. Do not enable physical mutations until this read gate passes.

- [ ] **Step 7: Commit**

```bash
git add crates/relay-core crates/relay-kernel xtask docs/acceptance/nuc-m1.md .github/workflows/ci.yml
git commit -m "feat: mount USB storage through BOT"
```

### Task 15: Kernel Shell Integration And Reboot Persistence

**Files:**
- Modify: `crates/relay-kernel/src/main.rs`
- Create: `crates/relay-kernel/src/runtime.rs`
- Create: `crates/relay-kernel/src/root.rs`
- Modify: `xtask/src/qemu.rs`
- Create: `xtask/src/scenario.rs`
- Test: `xtask/tests/qemu_persistence.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: native keyboard, USB block device, GPT selector, ext2/VFS/shell, and console mirror.
- Produces: boot-to-prompt production path and repeatable two-boot persistence scenario.

- [ ] **Step 1: Write the failing two-boot scenario**

```rust
#[test]
fn shell_mutations_survive_qemu_restart() {
    let mut image = disposable_production_image();
    qemu(&mut image).type_lines([
        "mkdir /docs",
        "write /docs/hello 'relay os'",
        "append /docs/hello ' persists'",
        "sync",
        "shutdown",
    ]).expect_clean_shutdown();
    qemu(&mut image).type_lines([
        "cat /docs/hello",
        "shutdown",
    ]).expect_output("relay os persists").expect_clean_shutdown();
    assert_clean_ext2(&image);
}
```

- [ ] **Step 2: Run the persistence test and verify failure**

Run: `cargo test -p relay-xtask --test qemu_persistence --locked -- --nocapture`

Expected: FAIL before the kernel wires the layers together.

- [ ] **Step 3: Wire the single-threaded runtime**

Initialize console, ACPI/PCI, DMA, xHCI, devices, USB block, GPT root partition, ext2, VFS, line editor, and shell in dependency order. Poll xHCI continuously; route keyboard events to editor; dispatch submitted lines synchronously; redraw prompt after recoverable errors. Use stable phase/status markers mirrored to serial without exposing diagnostics as shell input.

- [ ] **Step 4: Implement safe shutdown control flow**

Only print `[relay] phase=unmount status=clean` after ext2 clean marking and final device flush succeed. On failure, keep the system halted with an explicit unsafe-to-remove diagnostic and never claim clean shutdown. On success, halt CPUs; the QEMU harness waits for the marker and then sends QMP `quit`.

- [ ] **Step 5: Verify every shell command and persistence**

The QMP scenario exercises `help`, `pwd`, `cd`, `ls`, `cat`, `touch`, `write`, `append`, `mkdir`, `rm`, `rmdir`, `echo`, `sync`, `shutdown`, quoting, errors, the exact max-file boundary, and filesystem-full handling. Restart QEMU with the same mutable image and verify retained content. Extract root and require clean state plus `e2fsck -fn` exit 0 after each clean boot.

Run: `cargo xtask qemu persistence target/relay-os.img --display none --accel tcg`

Add a required `qemu-persistence` CI job in this PR. It uses a disposable image copy, TCG, no display, hard deadlines, two QEMU processes, and offline `e2fsck` after both clean shutdowns.

- [ ] **Step 6: Commit**

```bash
git add crates/relay-kernel xtask .github/workflows/ci.yml
git commit -m "feat: persist shell changes on USB"
```

### Task 16: Hardening Matrix, Final CI, And NUC Acceptance

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `xtask/src/negative.rs`
- Create: `xtask/tests/qemu_negative.rs`
- Modify: `xtask/src/lib.rs`
- Modify: `xtask/src/main.rs`
- Modify: `xtask/src/qemu.rs`
- Modify: `docs/dependencies.md`
- Complete: `docs/acceptance/nuc-m1.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: complete production image and all stable diagnostic markers.
- Produces: release-blocking host, supply-chain, image, QEMU, and recorded NUC gates.

- [ ] **Step 1: Write the negative-case table test**

```rust
#[test]
fn every_required_negative_case_has_a_bounded_scenario() {
    let names = negative_scenarios().map(|case| case.name).collect::<Vec<_>>();
    assert_eq!(names, [
        "missing-xhci", "missing-keyboard", "missing-storage",
        "bad-gpt-crc", "wrong-root-guid", "unsupported-ext2-feature",
        "malformed-usb-descriptor", "bot-tag-mismatch", "bot-phase-error",
        "transfer-timeout", "filesystem-full", "write-failure-poisons-mount",
    ]);
    assert!(negative_scenarios().all(|case| case.timeout <= Duration::from_secs(30)));
}
```

- [ ] **Step 2: Run the negative suite and verify missing scenarios**

Run: `cargo test -p relay-xtask --test qemu_negative --locked`

Expected: FAIL until every named scenario and diagnostic is implemented.

- [ ] **Step 3: Implement bounded negative QEMU scenarios**

Create disposable image mutations and explicit QEMU topologies for each case. On timeout, save serial log, QMP transcript, QEMU stderr, image hash, `query-status`, USB query output, and framebuffer screenshot. Match stable `phase`, `status`, and error-code markers rather than incidental prose.

- [ ] **Step 4: Complete the CI gates**

Retain the existing host, target, supply-chain, image, QEMU boot/storage/keyboard, and persistence gates; add the QEMU negative job and final release aggregation. Every Cargo command uses `--locked`; every QEMU wait has a hard timeout. Cache only immutable dependency/tool artifacts, not generated disk images.

Run locally:

```bash
cargo fmt --all --check
cargo clippy -p relay-abi -p relay-core -p relay-xtask --all-targets --locked -- -D warnings
cargo clippy -p relay-loader --target x86_64-unknown-uefi --locked -- -D warnings
cargo clippy -p relay-kernel --target x86_64-unknown-none --locked -- -D warnings
cargo test --workspace --locked
cargo build -p relay-loader --release --target x86_64-unknown-uefi --locked
cargo build -p relay-kernel --release --target x86_64-unknown-none --locked
cargo deny check advisories bans licenses sources
cargo audit --deny warnings
cargo xtask image --output target/relay-os.img
cargo xtask verify-image target/relay-os.img
cargo xtask qemu all target/relay-os.img --display none --accel tcg
```

- [ ] **Step 5: Perform and record the physical NUC acceptance test**

Record commit, image SHA-256, NUC BIOS version, Secure Boot and VT-d state, keyboard/drive models and VID:PIDs, physical ports, and each numbered spec acceptance result. Boot to framebuffer prompt without serial; exercise every command; create and append retained content; sync; shut down; boot again; read retained content; shut down; extract root on the host; require clean ext2 state and `e2fsck -fn` exit 0. Attach photos/logs by repository-relative evidence paths recorded in the document.

- [ ] **Step 6: Update user documentation**

Document exact prerequisites, image build command, QEMU command, safe flash procedure with device verification, NUC firmware prerequisites, supported device profile, shell command table, shutdown requirement, known non-goals, and troubleshooting markers in `README.md`.

- [ ] **Step 7: Run the release gate from a clean checkout**

Run all commands from Step 4 and compare the resulting image SHA-256 with the NUC-tested image. Verify `git status --short` is empty after generated artifacts remain under ignored `target/` paths.

- [ ] **Step 8: Commit**

```bash
git add .github/workflows/ci.yml xtask docs/dependencies.md docs/acceptance/nuc-m1.md README.md
git commit -m "test: gate Relay OS milestone one"
```
