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
- **Paths not covered by QEMU (plans 4–5).** 64-byte contexts, scratchpad buffers, BIOS handoff and SuperSpeed port reset only get host unit tests (against a fake register file) plus the NUC checks. Plan 4 must log each of these steps to `dmesg` in enough detail to debug from a photo of the screen. (Plan 4 found that QEMU 8.2's `usb-storage` connects at SuperSpeed, so a SuperSpeed port is exercised in QEMU after all; the others stay host-tested, spec §15 item 10.)
- **The e2e runner grows** with steps `key` (QMP `send-key`), `reboot` (relaunch on the same disk) and `poweroff`, plus a filesystem check after every scenario, when plans 4–5 need them.
- **Two xHCI controllers on the NUC (plan 4).** `00:0d.0` (Thunderbolt 4) enumerates before `00:14.0` (PCH), and only `00:14.0` has the keyboard and the stick. Plan 2 reports both (`pci::devices()`, `PciDevice::is_xhci`). Plan 4 must choose a controller, for example the one with connected ports, instead of taking the first, and then call `pci::enable_memory_and_bus_master` on it.
- **Services plan 2 leaves for later plans.** `mm::map_mmio` and the frame allocator (DMA buffers need a small `alloc_dma` on top in plan 4), `timer::{uptime, sleep, ticks}`, `rtc::now_unix`, and `acpi::get()` with the FADT reset and PM1 registers and `\_S5` (plan 5's `reboot` and `poweroff`). `klogln!` writes to the kernel log without the screen.
- **What plan 3 leaves for plans 4 and 5.** The crates `vfs`, `ext2` and `shell` are `no_std + alloc` and keep big buffers on the heap (64 KiB pieces in the shell, the 8 MiB block cache), so they suit the 64 KiB kernel stack and 32 MiB heap.
  - Plan 4 implements `shell::Console` (terminal and serial output, keyboard and serial input as bytes, with arrow keys as `ESC [ A`…) and `shell::System` (`rtc::now_unix`, `mm::stats` as `MemInfo` in bytes, the `klog` ring for `dmesg`, and `reboot`/`poweroff`, which may simply halt until plan 5). `Console::interrupted` must report a Ctrl-C typed while a command runs, without waiting for input (poll the keyboard queue), so long commands can be stopped; the default never interrupts. It implements `vfs::Env` over `rtc::now_unix` and `klogln!`, and runs `Shell::run` over `MountTable::new(Box::new(MemFs::new(env).read_only()))`, the empty read-only `/` of spec §10.
  - Plan 5 wraps each GPT partition as a `vfs::BlockDevice` (`vfs::check_request` does the bounds check) and mounts `Ext2::mount(partition, env, MountOptions::default())` at `/`. If the read-write mount fails with `EIO` (a worn-out stick often fails into read-only mode), it retries with `read_only: true`, so the files can still be read; only if that fails too does it fall back to the read-only `MemFs`. The shell already syncs after every command and shuts the filesystems down before calling `System::reboot`/`poweroff`, so plan 5 only adds the ACPI reset and sleep writes.
  - `cargo xtask host-shell <img>` runs the same shell and driver on the host over an image, for trying scenarios before they are written as e2e tests.
- **What plan 4 leaves for plan 5.** The kernel keeps one `usb::host::Host` per running xHCI controller in `usb::HOSTS` (kernel `usb.rs`); the console polls them. The mass-storage driver joins `Host` next to the keyboards: it claims interfaces 8/6/0x50 in `Host::claim`, uses the same `Bus` (control requests; plan 5 adds bulk transfers, with `xhci` already configuring bulk endpoints with their SuperSpeed burst), and its `BlockDevice` locks `HOSTS` for each request. The Kingston stick already enumerates in plan 4 (`port N: 0951:1666 SuperSpeed, not claimed`), so NUC check 2 exercises the SuperSpeed path. `KernelSystem::reboot`/`poweroff` halt until plan 5 adds the ACPI writes and the test-mode exit; startup prints `[FAIL] mount /: no storage driver yet` until the mount replaces it.
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
- **Deferred findings from plan 3's final review (plan 6).** Shell and ext2 minors, none of which plan 4's empty `/` can reach:
  - big directories are quadratic in `ls` and `rm -r` (20,000 files: `ls` 2.9 s, `rm -r` 5.8 s), and `read_dir` holds a whole directory in memory;
  - `rm -r` of a relative path that contains the current directory stops partway (GNU removes everything);
  - `cat` keeps reading its inputs after its output file is full (GNU stops at the first write error);
  - `cp` empties the destination before finding that a non-regular source cannot be read;
  - the parser passes an unquoted `#` (there are no comments) and a `$` inside double quotes through literally instead of refusing them.
- **Heap free-list links as `Option<NonNull<…>>` (plan 6).** CodeQL's `rust/access-invalid-pointer` reports ten alerts in `kernel/src/mm/heap.rs` (code-scanning alerts 1–10). They are false positives: the only invalid pointers are the `ptr::null_mut()` list-end markers, and every dereference sits behind an `is_null()` check (a loop condition or a branch) that CodeQL does not model. Plan 6 changes the free-list links (`Heap::classes`, `Heap::large`, `FreeBlock::next` and the `prev` pointers of the list walks) to `Option<NonNull<…>>`, so the compiler enforces those checks and the alerts close for a real reason. The heap's randomized host test and every QEMU scenario must stay green. The alerts stay open until then.
