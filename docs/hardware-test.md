# Hardware test checklist — Intel NUC 12 Pro

Target: NUC 12 Pro `NUC12WSHi7` (Core i7-1260P, firmware
`WSADL357.0090.2023.0821.1714`), monitor on HDMI, Logitech K120 USB keyboard,
Kingston DataTraveler 3.0 test stick (serial `08606E6D413FB27127135F8E`).
The NUC also runs Linux Mint, which is where the stick is written.

## One-time setup (in Linux Mint)

1. `cargo xtask setup-udev` and run the three `sudo` commands it prints.
2. Unplug and replug the stick.
3. `cargo xtask verify-usb` must no longer say "no access". (Before the first
   `flash --full` it may fail because the stick has no Relay OS layout yet.)

## Booting the stick

Reboot, press **F10** repeatedly while the Intel logo is shown, and pick the
UEFI entry for the Kingston DataTraveler 3.0. The Linux Mint boot order is
not changed. To get back to Mint, hold the power button for 4 s and power on
normally.

## Check 1 — text on HDMI (plan 1)

1. In Mint: `cargo xtask flash --full` and type `ERASE` when asked.
2. Boot the stick (see above).
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.1.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
     native resolution (1920×1200 on the ASUS PA248QV); the terminal uses at
     most the top-left 1920×1080 of it, so wider or taller screens have a
     black margin.
   - `[ ok ] cpu tables`
   - `[ ok ] boot info: N MiB usable in M regions, cmdline ''` — N should be
     roughly 15000.
   - `relay: early boot complete`, followed by a solid block cursor.
4. Panic screen: in Mint run `cargo xtask flash --kernel --cmdline panic=pagefault`,
   boot the stick, and check for a red screen with
   `CPU exception 14: page fault` and `CR2=0x00007fffdead0000`.
5. Restore: `cargo xtask flash --kernel`.

### If it fails

The NUC has no serial port, and the firmware console stops drawing once the
loader holds the display, so the loader and the early kernel paint **progress
squares** along the top-right edge of the screen, stage 1 rightmost. The
kernel console clears them once it works, so seeing them means boot stopped
at the leftmost one:

| # | Colour | Reached when… |
|---|---|---|
| 1 | red | video mode set |
| 2 | orange | boot partition and ACPI lookups done |
| 3 | yellow | kernel file loaded |
| 4 | green | page tables for RAM and screen built |
| 5 | cyan | kernel stack and jump code mapped |
| 6 | blue | boot-info memory reserved |
| 7 | magenta | last loader message printed; next is ExitBootServices |
| 8 | white | ExitBootServices returned |
| 9 | grey | boot info written; next is the jump to the kernel |
| 10 | pink | kernel running on its own page tables |
| 11 | light green | kernel serial probe done; next is the console |

| What you see | Likely cause | Next step |
|---|---|---|
| No Kingston entry under F10 | Stick not FAT32/GPT, or not detected | Re-run `flash --full`; try another USB port |
| `[PANIC] relay-boot: …` and `System halted` | Loader error (text says which); the machine stays on | Photograph the screen, then hold the power button |
| Loader text, then squares, then nothing | Boot stopped at the leftmost square | Note the count and colour of the last square |
| Garbled or blue-tinted text | Pixel format mismatch | Photograph the screen; note W×H |

## Check 1b — kernel core (plan 2, optional)

One boot answers everything: memory, ACPI, timer, RTC, PCI and the timer
self-check.

1. In Mint: `cargo xtask flash --kernel --cmdline check=timer`.
2. Boot the stick. After about 5 s the screen shows, below the check 1 lines
   (values from the NUC's firmware tables and Linux's view of the machine):
   - `[ ok ] memory: N MiB free of 15948 MiB, heap 32 MiB` — N a little
     below 15948.
   - `[ ok ] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0`
   - `[ ok ] timer: TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)`
   - `[ ok ] rtc: <date and time>` — the current UTC time (Mint keeps the
     RTC in UTC; `timedatectl` says "RTC in local TZ: no").
   - `timer check: ok, N ticks in 3 RTC seconds` — N close to 3000.
   - `pci: 00:0d.0 8086:461e 0c0330 USB xHCI, bar0 mem64 0x603d190000 64K`
   - `pci: 00:14.0 8086:51ed 0c0330 USB xHCI, bar0 mem64 0x603d180000 64K`
   - `[ ok ] pci: 24 devices on buses 00 01 72, xHCI at 00:0d.0 00:14.0` —
     `lspci | wc -l` in Mint also says 24.
   - `relay: early boot complete`
3. Photograph the screen, then restore: `cargo xtask flash --kernel`.

A `[FAIL]` line names the step and the reason; boot carries on after it
(except for memory, which stops the machine).

## Results log

| Date | Check | Commit | Result | Notes (W×H, N MiB, …) |
|---|---|---|---|---|
| 2026-09-26 | 1 (runs 1–5) | `6026534`…`f8bc08e` | Fail | Loader stopped inside ExitBootServices (square 7 of 9, no reset). Two loader bugs found: exclusive protocol opens stopped the firmware text console; kernel pages in an OS-defined memory type (`0x80005245`) made ExitBootServices hang. Fixed in `f8bc08e`, `8b8f13e`. |
| 2026-09-26 | 1, `video=1920x1200` | `8b8f13e` | Pass | `console 1920x1200 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, default cmdline | `8b8f13e` | Pass | Loader switched the ASUS PA248QV from native 1920×1200 to 1920×1080: `console 1920x1080 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, default cmdline (after review fix) | `8cd8ec7` | Pass | Native mode kept: `console 1920x1200 (120x33 cells)`, 15948 MiB usable in 32 regions, no progress squares left. The earlier default-cmdline row switched to 1920×1080 against spec §4.2.3 (fixed in `b7817b3`). |
| 2026-09-26 | 1, `panic=pagefault` | `8b8f13e` | Pass | Red panic screen: `CPU exception 14: page fault (error code 0x0)`, `CR2=0x00007fffdead0000`, log tail shown, `System halted.` |
| 2026-09-26 | 1b, `check=timer` | `ba9dedf` | Pass | `memory: 15915 MiB free of 15948 MiB, heap 32 MiB`; `acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0`; `timer: TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)`; `rtc: 2026-09-26 13:45:11 UTC`; `timer check: ok, 3000 ticks in 3 RTC seconds`; xHCI `00:0d.0` (bar0 `0x603d190000` 64K) and `00:14.0` (bar0 `0x603d180000` 64K); `pci: 24 devices on buses 00 01 72` (matches `lspci`). Every step `[ ok ]`. |
