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
     native resolution if that is at most 1920×1080, otherwise the largest
     mode up to 1920×1080.
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
| `relay-boot 0.1.0` then a panic message | Loader error (text says which) | Photograph the screen |
| Loader text, then squares, then nothing | Boot stopped at the leftmost square | Note the count and colour of the last square |
| Garbled or blue-tinted text | Pixel format mismatch | Photograph the screen; note W×H |

## Results log

| Date | Check | Commit | Result | Notes (W×H, N MiB, …) |
|---|---|---|---|---|
| 2026-09-26 | 1 (runs 1–5) | `6026534`…`f8bc08e` | Fail | Loader stopped inside ExitBootServices (square 7 of 9, no reset). Two loader bugs found: exclusive protocol opens stopped the firmware text console; kernel pages in an OS-defined memory type (`0x80005245`) made ExitBootServices hang. Fixed in `f8bc08e`, `8b8f13e`. |
| 2026-09-26 | 1, `video=1920x1200` | `8b8f13e` | Pass | `console 1920x1200 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, default cmdline | `8b8f13e` | Pass | Loader switched the ASUS PA248QV from native 1920×1200 to 1920×1080: `console 1920x1080 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, `panic=pagefault` | `8b8f13e` | Pass | Red panic screen: `CPU exception 14: page fault (error code 0x0)`, `CR2=0x00007fffdead0000`, log tail shown, `System halted.` |
