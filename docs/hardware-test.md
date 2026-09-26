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

| What you see | Likely cause | Next step |
|---|---|---|
| No Kingston entry under F10 | Stick not FAT32/GPT, or not detected | Re-run `flash --full`; try another USB port |
| `relay-boot 0.1.0` then a panic message | Loader error (text says which) | Photograph the screen |
| `relay-boot: starting kernel`, then nothing | Kernel crashed before the console | Try `flash --kernel --cmdline video=1280x720` |
| Garbled or blue-tinted text | Pixel format mismatch | Photograph the screen; note W×H |

## Results log

| Date | Check | Commit | Result | Notes (W×H, N MiB, …) |
|---|---|---|---|---|
| | | | | |
