# Milestone 1 Roadmap

**Spec:** `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

The milestone is split into six plans. Each one ends with software that can be tested by itself. Each plan is written just before it is executed, so it builds on the code that actually exists and on what the previous NUC check showed. Plans 1 and 2 are written in full; plans 3–6 are summarised here so their scope and order are agreed up front.

```
Plan 1 ──► Plan 2 ──► Plan 4 ──► Plan 5 ──► Plan 6
               Plan 3 ───┘ (shell)   ▲
                  └──────────────────┘ (ext2)
```

Plan 3 has no dependency on plans 1–2 beyond the workspace. It can be executed in parallel with plan 2.

| Plan | Spec §11 step | Delivers | Ends with |
|---|---|---|---|
| 1 · Boot to screen | 1–2 | Workspace and toolchain; CI gate (`lint`, `unit`, `e2e` on every PR); `xtask` build/image/qemu/test/flash; UEFI loader → higher-half kernel hand-off; framebuffer console; GDT/IDT; panic screen | 7 QEMU scenarios green; **NUC check 1**: text on HDMI |
| 2 · Kernel core | 3 | Frame allocator; kernel heap (`#[global_allocator]`); kernel-owned page tables with `map_mmio` and a write-combining framebuffer (PAT); ACPI tables (RSDP/XSDT/MCFG/HPET/FADT, `\_S5` scan); TSC/HPET/LAPIC timer with the first interrupt handlers; CMOS RTC; PCI ECAM enumeration | `[ ok ]` lines for memory, acpi, timer, rtc and pci in QEMU; PCI list printed; optional NUC re-check |
| 3 · Filesystem and shell (host) | 4 | `vfs` (paths, errno, traits, mount table), `ext2` (read/write, allocation, block cache), `shell` (line editor, parser, built-in commands); `xtask host-shell <img>` | `mke2fs` → our changes → `e2fsck -fn` clean; randomized model test; every command tested against an in-memory `Vfs` |
| 4 · USB keyboard | 5 | `usb` crate: `Hal`, xHCI (BIOS handoff, reset, scratchpad, 32/64-byte contexts, USB 2/3 port reset, enumeration), HID boot keyboard with US keymap and repeat; kernel `Hal`; console input; interactive shell over an empty read-only `/` | QEMU `keyboard` scenario over QMP `send-key`; **NUC check 2**: typing on the K120 |
| 5 · USB storage and `/` | 6 | BOT/SCSI mass storage; `BlockDevice`; GPT; root selection by boot partition GUID; ext2 mounted at `/`; per-command sync; `reboot`/`poweroff` via ACPI; test-mode exit | QEMU scenarios `fileops`, `persist`, `bigfile`, `display`, `diskfull`, each followed by `e2fsck`; **NUC check 3**: full hardware checklist |
| 6 · Hardening | 7 | Fixes from the NUC checks, full `docs/hardware-test.md`, README quick start, final spec/code consistency pass | `cargo xtask test` green; hardware checklist green; milestone done |

## Notes carried forward

- **CI and pull requests (all plans).** From plan 1's second PR on, every pull request to `main` must pass the `lint`, `unit` and (from PR 5) `e2e` checks. Every plan is split into PRs that are each green on their own, one branch and worktree per PR. New crates and scenarios are picked up by `cargo xtask ci` automatically. A plan only touches `xtask/src/ci.rs` or the workflow when it needs a new kind of check. (Plan 3's filesystem cross-checks turned out to need none: the ubuntu-24.04 runner already has e2fsprogs and fdisk.)
- **ACPI test data (plan 2).** The NUC's ACPI tables under `/sys/firmware/acpi/tables/` are root-only. Plan 2 will ask the user to copy them once (`sudo cp -r /sys/firmware/acpi/tables /home/maw/src/bin-inc/relay-os-impl/tmp/nuc-acpi && sudo chown -R maw …`). They then become host-test fixtures for the MCFG/FADT/`\_S5` parsers alongside tables captured from QEMU.
- **Interrupts (plan 2).** Plan 1 never enables interrupts. Plan 2 adds IRQ vectors 32+, masks the legacy PIC, and handles LAPIC EOI. The e2e scenarios gain a timer-tick check.
- **Paths not covered by QEMU (plans 4–5).** 64-byte contexts, scratchpad buffers, BIOS handoff and SuperSpeed port reset only get host unit tests (against a fake register file) plus the NUC checks. Plan 4 must log each of these steps to `dmesg` in enough detail to debug from a photo of the screen.
- **The e2e runner grows** with steps `key` (QMP `send-key`), `reboot` (relaunch on the same disk) and `poweroff`, plus a filesystem check after every scenario, when plans 4–5 need them.
- **Two xHCI controllers on the NUC (plan 4).** `00:0d.0` (Thunderbolt 4) enumerates before `00:14.0` (PCH), and only `00:14.0` has the keyboard and the stick. Plan 2 reports both (`pci::devices()`, `PciDevice::is_xhci`). Plan 4 must choose a controller, for example the one with connected ports, instead of taking the first, and then call `pci::enable_memory_and_bus_master` on it.
- **Services plan 2 leaves for later plans.** `mm::map_mmio` and the frame allocator (DMA buffers need a small `alloc_dma` on top in plan 4), `timer::{uptime, sleep, ticks}`, `rtc::now_unix`, and `acpi::get()` with the FADT reset and PM1 registers and `\_S5` (plan 5's `reboot` and `poweroff`). `klogln!` writes to the kernel log without the screen.
- **What plan 3 leaves for plans 4 and 5.** The crates `vfs`, `ext2` and `shell` are `no_std + alloc` and keep big buffers on the heap (64 KiB pieces in the shell, the 8 MiB block cache), so they suit the 64 KiB kernel stack and 32 MiB heap.
  - Plan 4 implements `shell::Console` (terminal and serial output, keyboard and serial input as bytes, with arrow keys as `ESC [ A`…) and `shell::System` (`rtc::now_unix`, `mm::stats` as `MemInfo` in bytes, the `klog` ring for `dmesg`, and `reboot`/`poweroff`, which may simply halt until plan 5). It implements `vfs::Env` over `rtc::now_unix` and `klogln!`, and runs `Shell::run` over `MountTable::new(Box::new(MemFs::new(env).read_only()))`, the empty read-only `/` of spec §10.
  - Plan 5 wraps each GPT partition as a `vfs::BlockDevice` (`vfs::check_request` does the bounds check) and mounts `Ext2::mount(partition, env, MountOptions::default())` at `/`, falling back to the read-only `MemFs` if that fails. The shell already syncs after every command and shuts the filesystems down before calling `System::reboot`/`poweroff`, so plan 5 only adds the ACPI reset and sleep writes.
  - `cargo xtask host-shell <img>` runs the same shell and driver on the host over an image, for trying scenarios before they are written as e2e tests.
- **Deferred findings from plan 1's final review (plan 6).**
  - #5: the EDID comes from the first `EdidDiscovered` handle, not the GOP's own. Switching could lose native-mode detection on the NUC.
  - #6: a `set_mode` error aborts boot with a misleading message instead of keeping the current mode.
  - #7b: the cmdline limit is 256 bytes; the spec says at most 255.
  - #9: the QMP abstract socket name is predictable (no access control on shared hosts).
  - #11: the loader-rules test greps only `main.rs`, `video.rs` and `paging.rs`.

  Plan 2 fixed #8 (early GDT/IDT) and #10 (QEMU start-up failures in the e2e runner).
- **Deferred findings from plan 2's final review (plan 6).** Kernel minors; none can trigger on the NUC or in QEMU today.
  - A failed `timer::init` returns before masking the PIC, yet interrupts are enabled.
  - The interrupt-storm count never resets.
  - Vectors 32–47 delivered through the LAPIC get no LAPIC EOI.
  - `parse_fadt` reads offset 40 without a length guard.
  - ACPI table lengths and XSDT pointers are mapped with no size cap or MMIO check.
  - One failed ECAM window discards devices found in earlier windows.
  - `check=timer` after a timer failure blames the RTC.
  - A 64-bit BAR in the last BAR slot is reported as `mem32`.
  - Plan 2's text for `timer::sleep` is stale: a review fix (`e27389a`) made it busy-wait on the TSC; the code is the reference.
