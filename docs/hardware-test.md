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
   - `Relay OS 0.3.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
     native resolution (1920×1200 on the ASUS PA248QV); the terminal uses at
     most the top-left 1920×1080 of it, so wider or taller screens have a
     black margin.
   - `[ ok ] cpu tables`
   - `[ ok ] boot info: N MiB usable in M regions, cmdline ''` — N should be
     roughly 15000.
   - Since plan 4 more lines follow (checks 1b and 2), and the last one is
     the shell prompt `root@relay:/# ` with a solid block cursor.
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
| 3 | yellow | kernel file loaded; next is reading `system.img` |
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
3. Photograph the screen, then restore: `cargo xtask flash --kernel`.

A `[FAIL]` line names the step and the reason; boot carries on after it
(except for memory, which stops the machine).

## Check 2 — typing on the K120 (plan 4)

The K120 must be on the port it has in Mint (`lsusb -t`: bus 3 port 3), the
Unifying receiver on port 1 and the stick on a USB 3 port.

1. In Mint: `cargo xtask flash --kernel`.
2. Boot the stick. After the check 1b lines the screen shows:
   - `usb: 00:0d.0 xHCI 1.20, 4 ports (1 USB 2, 3 USB 3), 32-byte contexts,
     34 scratchpads` (the Thunderbolt controller; nothing is plugged into
     it). If the firmware left it powered off the line is
     `usb: 00:0d.0: controller not responding` instead; that is fine too.
   - `usb: 00:14.0 xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 32-byte
     contexts, 34 scratchpads`. A nonzero scratchpad count is a path QEMU
     cannot test (34 needs both halves of the count field). The NUC uses
     32-byte contexts like QEMU; 64-byte ones are tested on the host only.
   - one line per device, in port order:
     - `usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard` (the receiver)
     - `usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard` (the K120)
     - `usb: 00:14.0 port 10: 8087:0033 full-speed, not claimed` (the
       NUC's internal Bluetooth)
     - `usb: 00:14.0 port 15: 0951:1666 SuperSpeed, not claimed` (the
       stick on bus 4 port 3, the third USB 3 port; its driver comes with
       plan 5)

     The boot waits for them: at least 100 ms, and up to 1 s while a USB 3
     link is still training (`dmesg`: `ports settled after N ms`).
   - `[ ok ] usb: 2 controllers, 4 devices` (or 1 controller),
     `[ ok ] keyboard: 2 keyboards`, `[FAIL] mount /: no storage driver
     yet`, and the prompt `root@relay:/# `. (Since plan 5 the stick is a
     disk, `/` is mounted and the prompt is `root@relay:~# `; see check 3.)
3. On the K120, type and check each result:
   - `echo hello`, Enter → `hello`.
   - `echo Hello, World!` with Shift → `Hello, World!`.
   - Caps Lock: type `echo ` with Caps Lock off, press Caps Lock (its light
     goes on), type `abc` and Enter → `ABC`; Caps Lock again turns the light
     off. (Typed with Caps Lock on, the command itself becomes `ECHO`, and
     `relay-sh: ECHO: command not found` is right: command names are
     case-sensitive, as in bash.)
   - Backspace, ←/→, Home/End while editing a line; ↑/↓ for history.
   - Hold a letter: after half a second it repeats, about 30 times a
     second, and stops when released.
   - `echo nope` then Ctrl-C → `^C` and a fresh prompt.
   - `uname -a`, `date` (the current UTC time), `free`, `dmesg` (the
     `xhci 00:14.0:` lines of every step).
4. Unplug the K120, plug it back into the same port, and type `echo back`:
   it works again (hot-plug; `dmesg` shows `port 3: disconnected, PORTSC …`,
   then `port 3: connected` and the device set up again, possibly in the
   same slot number).
5. Photograph the screen after step 2 and after `dmesg`.

### If it fails

The NUC has no serial port, so a keyboard that does not work cannot run
`dmesg`. Boot with the USB log on screen instead: in Mint
`cargo xtask flash --kernel --cmdline debug=usb`, boot, and photograph the
`xhci 00:14.0:` lines (they scroll; take several photos). Restore with
`cargo xtask flash --kernel`.

| What you see | Likely cause | Next step |
|---|---|---|
| `usb: 00:14.0: timed out` | Handoff or reset did not finish | `debug=usb`: the handoff and reset lines name the register |
| `port 3: setup failed: …` | The K120's enumeration failed | `debug=usb`: the last `port 3` / `slot` line before the failure |
| `port 3: … keyboard not started: …` | The K120 refused the boot protocol | Note the reason; `debug=usb` shows the `hid:` lines |
| `[FAIL] keyboard: no USB keyboard found` and no `port 3` line | The K120 connected after the boot's wait, or not at all | Type anyway: a late keyboard is set up when it appears. If nothing works, `debug=usb`: a `port 3: connected` line means it appeared late, none means the port never saw it; the `ports settled` line shows the wait |
| Keys show up twice or not at all | Firmware still emulating a keyboard (handoff) | `debug=usb`: the `legacy support` line |

## Check 3 — files on the stick (plans 5 and 6; milestone 2), and check 4

The full checklist of spec §9.4. The K120 and the stick sit on the ports
of check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`). Since plan 6
the commands come from two scripts on the stick,
`/root/checks/check3-a.sh` and `check3-b.sh` (in the repository under
`rootfs/root/checks/`), and since milestone 2's plan 4b a third,
`check4.sh` (NUC check 4 of the user-space gate, spec §12.4), after which
the error screen is visited by hand (plan 5). Every command is a program
now, and `/bin/sh` runs them, which the kernel's init starts as process 2:
`sh` runs each line as if it were typed, shows it as `+ <command>` before
its output, and writes everything it shows into a transcript next to the
script (`check3-a.log`). `verify-usb` checks the transcripts in Mint
against the output the scripts expect (their `#>` lines), so only the boot
screen needs a look. The QEMU scenario `checks` runs the same scripts on
every pull request.

1. In Mint: `cargo xtask flash --full` and type `ERASE` when asked (this
   erases the files of earlier runs and writes the current scripts;
   `flash --kernel` leaves the scripts on the stick as they were).
2. Reboot, press F10 and choose the UEFI entry for the Kingston stick.
3. The screen shows every startup line `[ ok ]`, at the monitor's native
   resolution (`console 1920x1200` on the ASUS PA248QV). After check 2's
   `usb:` lines, which now end:
   - `usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston
     DataTraveler 3.0, 14.4 GiB`
   - `[ ok ] usb: 2 controllers, 4 devices`, `[ ok ] keyboard: 2 keyboards`
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB` (the
     root is 2 GiB since milestone 2's plan 2; a stick written by an older
     `flash --full` still shows `14.3 GiB`, which the check scripts refuse:
     run `flash --full` again)
   - `[ ok ] system: 34 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
     add programs)
   - the motd (`Welcome to Relay OS.`), which init prints, and the prompt
     `root@relay:~# ` of the `/bin/sh` it started.

   Photograph the screen.
4. On the K120, type `sh checks/check3-a.sh`. It runs for about a minute:
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), `ls -l /bin` (the programs of the system archive),
   `t-args a 'b c' ''` and `t-fault null-read` (programs in ring 3: three
   arguments printed, then `relay-sh: t-fault: killed (page fault at 0x0,
   read, ip …)` and the script goes on), `t-spin 1` (a program that never
   makes a system call ends after its second: the timer takes the CPU from
   it), `t-spawn 100` (`frames lost: 0`), `t-spawn kill` (a child killed
   while its parent sleeps; process 1 cannot be killed), `t-fault sse`,
   `t-fault flags-ac` and `t-fault gsbase` (an SSE instruction, a spin with
   the alignment check flag set and a program setting its own `gs` base,
   all killed), plan 3b's programs (`t-files basic`, `dir`, `cwd` and
   `gone`: the file calls on the stick's ext2 root, and another process's
   removal of a program's working directory and open file; `t-mem map`
   and `grow 64`: memory from `mem_map` and a heap that grows, `frames
   lost: 0`; `t-tee end`: a console tee; `t-sys uname`; `t-read apart`:
   a program outside the console's group reads nothing), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
   `echo >`/`>>`, `cat`, `ls -l`, `cp`, `mv`, `rmdir` of a full directory,
   `rm -r`, `touch`, `stat`, `head`, `tail`, `wc`, the errors of `cat` and
   `rm -r /`), an 8 MiB file built by doubling (each step writes up to
   4 MiB to the stick and syncs), and `df`. Ctrl-C stops it. The prompt
   comes back after `+ df` and its two lines.
   `dmesg`'s lines include `cpu: SMEP on, SMAP on`: the kernel would fault
   if it touched a program's page.
5. Three steps by hand (milestone 2, plans 3a and 3b; a script cannot
   type):
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
     K120: the line shows as it is typed (milestone 2, plan 3b: the line
     discipline echoes it). After five seconds its `… iterations` line
     comes, then the prompt with `echo typed` again, then `typed`: the
     keyboard is polled on every tick that interrupts a program, and what
     the program did not read is the shell's, as in bash.
   - `t-spin`, then Ctrl-C: `^C` and the prompt come back at once, and
     `dmesg` ends with `pid <n> (/bin/t-spin): killed: Ctrl-C`.
   - `t-read`, then on the K120 `hello wrold`, four Backspaces, `orld`
     and Enter: the letters show as they are typed and the Backspaces
     erase them, and `t-read` prints `[12] hello world\n`. Then Ctrl-D:
     `end of input` and the prompt (the line discipline, milestone 2,
     plan 3b).

   Photograph the screen after each.
6. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10 and type `sh checks/check3-b.sh`: the files written before the
   restart are read back.
7. Check 4 (milestone 2, plan 4b): type `sh checks/check4.sh`. It runs for
   about ten seconds: `t-spawn fill` (60 children, the table's room beside
   init and two shells), `t-spawn 1000` (`frames lost: 0`), `free`, every
   `t-fault` kind (each killed with its message, the script going on),
   `t-spawn kill-new` and `orphan` (an orphan that init collects), `t-args`,
   `t-abi` (`relay-sh: t-abi: Exec format error`: a program built for
   another ABI), `ls /bin` into a file (one name a line) and `cat` of a
   file into itself (refused), a script that runs another, and `free` again
   (the same memory in use). Then one step by hand: `exit` at the prompt.
   `init: /bin/sh (pid <n>) exited with 0; starting it again` and a new
   prompt `root@relay:~# ` come at once, without the motd: the shell is a
   program, and process 1 starts another. Photograph the screen.
8. The error screen (milestone 2, plan 5): `reboot`, and choose the stick
   again with F10, so that the kernel log's last lines are the boot's. At
   the prompt type `exit` three times within 10 s. After the first two,
   init's line and a new prompt; after the third,
   `init: /bin/sh (pid <n>) exited with 0` and the error screen: `*** Relay
   OS cannot run its shell ***`, `/bin/sh ended 3 times within 10 s`, the
   kernel log's last 20 lines, from the `xhci 00:14.0: port 15:` lines to
   init's three, and `Press any key to reboot.`
   The line `xhci 00:14.0: slot 4: interface 0 class 8/6/80, …` is wider
   than the screen and takes two rows, and the heading stays at the top.
   Photograph it, wait a few seconds (it waits for the key, however long),
   then press a key on the K120: the NUC restarts. Choose the stick again
   with F10: the motd and the prompt.
9. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
10. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`,
   `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run <time>)`
   and `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run
   <time>)`, with the UTC times of the three runs, after the line
   `system.img: built <time>`. A transcript older than the stick's
   `system.img` (a run before the last `flash --kernel`, which keeps the
   transcripts; `flash --full` erases them) fails. A `FAILED` line is
   followed by the script line, the expectation that failed and what the
   command printed there; a script that was not run is `FAILED, not run`.

### If it fails

| What you see | Likely cause | Next step |
|---|---|---|
| `port 15: … disk not started: …` and `[FAIL] mount /: no USB disk` | The stick's setup failed (the reason says which command) | `debug=usb`: the `storage: slot N:` lines with the sense of each failure |
| `port 15: setup failed: …` | Enumeration failed three times | Replug the stick into the same port and reboot; `debug=usb` shows each try |
| `[FAIL] mount /: no disk with a GPT` | The stick was set up but its reads fail, or it has no GPT | `dmesg`: a `usb: 00:14.0 port 15: read at block N: …` line and the `storage: slot N:` lines with the sense mean the reads fail; `storage: 00:14.0 port 15: no valid GPT` means re-run `flash --full` |
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] system: no system.img`, then the error screen (`*** Relay OS cannot run its shell ***`) | The loader could not read `\EFI\RELAY\system.img` (it is missing, or the FAT is damaged) | `cargo xtask flash --kernel` writes it with the loader and the kernel |
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …`, then the error screen | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `exit` at the prompt gives no new prompt, or the error screen | Init does not see the shell end, or its clock runs fast (three ends within 10 s) | Photograph the screen: the log's lines on it end with init's `init: /bin/sh (pid N) …` lines |
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
| A key on the K120 at the error screen does nothing | The keyboard is not polled while init waits (the idle task polls it) | Photograph the screen and hold the power button |
| The error screen's heading has scrolled away, or a program's line shows under `Press any key` | A log line takes more rows than counted, or a process still ran | Photograph the screen |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
| `t-spin 1` never ends, or the keyboard stops during `t-spin 5` | The LAPIC timer's ticks do not reach ring 3 on this machine, or the tick's poll of the xHCI keyboard fails there | Photograph the screen; after a reboot, `dmesg` shows the `timer:` and `usb:` lines |
| The panic screen during `t-spawn` or `t-fault flags-ac` | A context switch, or an interrupt taken in ring 3, goes wrong on this CPU: the panic's message names the check that failed (`a program's flags reached a switch`, `TSS rsp0 and the syscall stack differ`, `an interrupt with the program's gs`) | Photograph the panic screen |
| `t-read` shows nothing as it is typed, a Backspace does not erase, or Ctrl-D does not end it | The line discipline gets the K120's keys otherwise than QEMU's keyboard sends them | Photograph the screen; `dmesg` shows the keyboard's `usb:` lines |
| A `t-files`, `t-mem` or `t-tee` line differs (`verify-usb` names it) | A file call, `mem_map` or a tee works otherwise on the stick's ext2 or the NUC's memory | The line after `FAILED` names the command, the expectation and what it printed; `dmesg` for `ext2:` and `usb:` lines |
| `t-fault gsbase` prints `t-fault: no fault` | CR4.FSGSBASE is set: the firmware left it, and the kernel did not clear it | Note it; a program could set a `gs` base that would pass to the next program |
| `cpu: SMEP not available, SMAP not available` in `dmesg` | CPUID reports neither (the i7-1260P has both) | Note the `dmesg` line; the check scripts expect both on the NUC |
| `relay-sh: t-args: Exec format error` | The kernel refused the program; `dmesg` shows `spawn /bin/t-args: <reason>` | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
| `[FAIL] mount /: …; mounted read-only` | The stick refused a write (worn out or write-protected) | The files can be read; note the `usb: … write at block N:` line in `dmesg` |
| `[FAIL] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB: Invalid argument` | The ext2 root is not what `flash --full` writes | `dmesg` shows the `ext2:` reason; re-run `flash --full` |
| A command prints `Input/output error` | A disk request failed after three tries | `dmesg`: the `storage:` and `usb:` lines name the command and block |
| `reboot` leaves the screen as it is | No reset method worked (unlikely: the last is a triple fault) | Photograph the screen; hold the power button |
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |
| `verify-usb`: `check3-a.sh: FAILED` (or another script) | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `the transcript is older than the system on the stick` | The script ran before the last `flash --kernel` | Run the script again on the NUC |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
| `sh: cannot write the transcript …` | `/` is read-only (see the `mount /` line) | Nothing ran; fix the mount first |

## Results log

| Date | Check | Commit | Result | Notes (W×H, N MiB, …) |
|---|---|---|---|---|
| 2026-09-26 | 1 (runs 1–5) | `6026534`…`f8bc08e` | Fail | Loader stopped inside ExitBootServices (square 7 of 9, no reset). Two loader bugs found: exclusive protocol opens stopped the firmware text console; kernel pages in an OS-defined memory type (`0x80005245`) made ExitBootServices hang. Fixed in `f8bc08e`, `8b8f13e`. |
| 2026-09-26 | 1, `video=1920x1200` | `8b8f13e` | Pass | `console 1920x1200 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, default cmdline | `8b8f13e` | Pass | Loader switched the ASUS PA248QV from native 1920×1200 to 1920×1080: `console 1920x1080 (120x33 cells)`, 15948 MiB usable in 32 regions. |
| 2026-09-26 | 1, default cmdline (after review fix) | `8cd8ec7` | Pass | Native mode kept: `console 1920x1200 (120x33 cells)`, 15948 MiB usable in 32 regions, no progress squares left. The earlier default-cmdline row switched to 1920×1080 against spec §4.2.3 (fixed in `b7817b3`). |
| 2026-09-26 | 1, `panic=pagefault` | `8b8f13e` | Pass | Red panic screen: `CPU exception 14: page fault (error code 0x0)`, `CR2=0x00007fffdead0000`, log tail shown, `System halted.` |
| 2026-09-26 | 1b, `check=timer` | `ba9dedf` | Pass | `memory: 15915 MiB free of 15948 MiB, heap 32 MiB`; `acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0`; `timer: TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)`; `rtc: 2026-09-26 13:45:11 UTC`; `timer check: ok, 3000 ticks in 3 RTC seconds`; xHCI `00:0d.0` (bar0 `0x603d190000` 64K) and `00:14.0` (bar0 `0x603d180000` 64K); `pci: 24 devices on buses 00 01 72` (matches `lspci`). Every step `[ ok ]`. |
| 2026-09-28 | 2 | `a3db39a` | Pass | `usb: 00:0d.0 xHCI 1.20, 4 ports (1 USB 2, 3 USB 3), 32-byte contexts, 34 scratchpads`; `usb: 00:14.0 xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 32-byte contexts, 34 scratchpads`; port 1 `046d:c534 full-speed, keyboard` (EP0 8 bytes), port 3 `046d:c31c low-speed, keyboard`, port 10 `8087:0033 full-speed, not claimed` (internal Bluetooth, isochronous endpoints), port 15 `0951:1666 SuperSpeed, not claimed` (hot reset done); `usb: 2 controllers, 4 devices`, `keyboard: 2 keyboards`, `[FAIL] mount /: no storage driver yet` (expected), prompt. Typing on the K120: Shift, Caps Lock with its light, Backspace, arrows, history, repeat after about half a second, Ctrl-C, `dmesg`; unplugging and replugging the K120, then `echo back` works. |
| 2026-09-28 | 3 | `3ad0688` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`; `usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB` (`storage: slot 4: vendor "Kingston", product "DataTraveler 3.0", revision "PMAP", removable`, `30277632 blocks of 512 bytes`, ready at the first TEST UNIT READY); `storage: root on 00:14.0 port 15, the disk with the boot partition 4EBD57DE-8DBB-4D34-B3B5-D18607581E70`; `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`; motd and `root@relay:~#`. The step 4 commands on the K120 gave the listed output (`ls -l`: `a` 19 bytes, `old`; `rmdir` refused, `rm -r` worked; `stat`, `head`, `tail`, `wc`, `cat` of a missing file, `rm -r /`, `df`). `reboot`, F10 again: `cat /root/notes/a` shows both lines, `ls /root/notes` shows `a  t`. `poweroff`: `relay: powering off` and the NUC switched itself off. In Mint `verify-usb`: `e2fsck: clean`, `/root/notes/a` (19 bytes) and `/root/notes/t` listed. |
| 2026-09-29 | 1–3, the full checklist (milestone 1 done) | `363f65e` | Pass | `flash --full` of 0.2.0, then F10: `Relay OS 0.2.0`, every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `boot info: 15947 MiB usable in 32 regions`, `acpi: 30 tables, … S5 7/0`, `timer: TSC 2496.000 MHz (CPUID 0x15)`, `pci: 24 devices on buses 00 01 72`; both xHCI controllers with 32-byte contexts and 34 scratchpads; ports 1, 3, 10 and 15 as in check 2, the Kingston `14.4 GiB`; `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`, root found by the boot partition GUID. `dmesg` shows `port N: connection stable after 100 ms` for each device and no empty port reported disconnected. Check 2 typed on the K120: Shift, Caps Lock with its light, Backspace, arrows, Home/End, history, key repeat, Ctrl-C, `uname -a` → `Relay relay 0.2.0 x86_64`, `date`, `free`, `dmesg`; unplugging and replugging the K120 works. Check 3 by script: `sh checks/check3-a.sh`, `reboot`, `sh checks/check3-b.sh`, `poweroff`; the root was left clean. In Mint `verify-usb`: `e2fsck: clean`, `/root/notes/a` (19 bytes), `/root/notes/big` (8388608 bytes, written in 4 MiB steps within the 5 s bulk timeout) and `/root/notes/t`; `check3-a.sh: ok, 63 of 63 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. |
| 2026-09-29 | 3 (milestone 2, plan 1) | `03aa632` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `boot info: 15947 MiB usable in 32 regions`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`, then the new `[ ok ] system: 1 program, ABI 1`: the loader read `\EFI\RELAY\system.img` from the NUC's ESP and the kernel mounted it at `/bin`. Check 3 by script: `ls -l /bin` listed `-rwxr-xr-x 1 root root 19688 … t-args`; `reboot`, `poweroff` as before. In Mint `verify-usb`: `e2fsck: clean`; `check3-a.sh: ok, 64 of 64 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. The transcripts replaced the hand-edited fixtures in `xtask/fixtures/checks/`. (PR #48 merged before this row; it lands in the follow-up.) |
| 2026-09-29 | 3 (milestone 2, plan 2) | `a809a6c` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `boot info: 15947 MiB usable in 34 regions`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`, `[ ok ] system: 2 programs, ABI 1`. Check 3 by script: `ls -l /bin` listed `t-args` (19688 bytes) and `t-fault` (19296); `t-args a 'b c' ''` printed `[1] a`, `[2] b c`, `[3] `, the first program run in ring 3 on the NUC; `t-fault null-read` gave `relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x4002e5)` and the script went on; `reboot`, `poweroff` as before. In Mint `verify-usb`: `e2fsck: clean`; `check3-a.sh: ok, 66 of 66 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. On the first boot the stick's first device-descriptor request on port 15 timed out; the setup's retry (milestone 1's plan 5) attached it on the second try (slot 5); the second boot attached it at once. The transcripts replaced the hand-edited fixtures in `xtask/fixtures/checks/`. |
| 2026-09-29 | 3 (the 2 GiB root) | `30b0756` | Pass | `flash --full` printed `creating ext2 on 2.0 GiB...`: the root is partition 2 of exactly 4194304 sectors, the rest of the stick unused. F10: every startup line `[ ok ]`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB`, `[ ok ] system: 2 programs, ABI 1`; the stick attached at the first try. Check 3 by script: `df` showed `/dev/root 2064208 8304 1951048 1% /`; `t-args` and `t-fault` as before; `reboot`, `poweroff` as before. In Mint `verify-usb`: `e2fsck: clean`; `check3-a.sh: ok, 66 of 66 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. `check3-a.sh` took about 2 min 10 s (24 s in the check before, with the same kernel): the stick's write speed varies from run to run. The transcripts replaced the hand-edited fixtures in `xtask/fixtures/checks/`. |
| 2026-09-30 | 3 (milestone 2, plan 3a) | `5fa17c9` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `cpu: SMEP on, SMAP on` in `dmesg` right after `[ ok ] cpu tables`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB`, `[ ok ] system: 4 programs, ABI 1`; the stick attached at the first try on both boots. Check 3 by script: `ls -l /bin` listed `t-args` (19688 bytes), `t-fault` (19296), `t-spawn` (28904) and `t-spin` (19560); `t-spin 1` made 4594860032 iterations and ended after its second, preempted by the real LAPIC timer; `t-spawn 100` showed 4073369 free frames before and after (`frames lost: 0`); `t-spawn kill` killed the spinning `t-spin` while it slept (`t-spin: kill`, `kill 1: EPERM`, `kill 999999: ESRCH`); `t-fault sse` gave `killed (FPU/SSE instruction, ip 0x40012b)`, `t-fault flags-ac` and `t-fault gsbase` `killed (invalid opcode, …)`, so CR4.FSGSBASE is clear on the NUC; the two steps by hand (typing on the K120 during `t-spin 5`, Ctrl-C of `t-spin`) passed; `reboot`, `poweroff` as before. In Mint `verify-usb`: `e2fsck: clean`; `check3-a.sh: ok, 72 of 72 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. The transcripts replaced the hand-edited fixtures in `xtask/fixtures/checks/`. |
| 2026-09-30 | 3 (milestone 2, plan 3b) | `0287f86` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `cpu: SMEP on, SMAP on`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB`, `[ ok ] system: 9 programs, ABI 1`. Check 3 by script: `ls -l /bin` listed the nine programs (`t-files` 50552 bytes, `t-mem` 39216, `t-read` 28232, `t-sys` 29496, `t-tee` 36832 among them); plan 3b's section ran on the stick's ext2 root as in QEMU, line for line: `t-files basic`, `dir`, `cwd` and `gone` (another process's removal of a program's working directory and open file: `ENOENT`, the new file kept `new`), `t-mem map` (`frames lost: 0`) and `grow 64` (`1928 blocks, 64 MiB, all there: true`), `t-tee end`, `t-sys uname` (`Relay relay 0.2.0 x86_64`) and `t-read apart` (`end of input`); `t-spin 1` made 4594860032 iterations, `t-spawn 100` lost no frame; the three steps by hand passed (typing during `t-spin 5`, now echoed as typed; Ctrl-C of `t-spin`; `hello wrold`, four Backspaces, `orld` into `t-read` gave `[12] hello world\n`, and Ctrl-D `end of input`); `reboot`, `poweroff` as before. In Mint `verify-usb`: `e2fsck: clean`; `check3-a.sh: ok, 84 of 84 commands as expected`, `check3-b.sh: ok, 5 of 5 commands as expected`. The transcripts replaced the hand-edited fixtures in `xtask/fixtures/checks/`. |
| 2026-09-30 | 4 (milestone 2, plan 4b) | `a0cef95` | Pass | `flash --full`, then F10: every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `cpu: SMEP on, SMAP on`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB`, `[ ok ] system: 34 programs, ABI 2`, then the motd printed by init and `/bin/sh`'s prompt (every command now a program; no shell in the kernel). Check 3 by script under `/bin/sh`: 84 of 84 and, after `reboot`, 5 of 5; the three steps by hand as before; `t-spin 1` made 4594860032 iterations, exactly plan 3b's count (4382 steps of 2^20), so the new poll on the way back to ring 3 costs a spinning program nothing. Check 4 by script: 33 of 33: `t-spawn fill` 60 children, `t-spawn 1000` `frames lost: 0` (4071601 free frames before and after), every `t-fault` kind killed, `t-abi` `Exec format error`, the nested script's lines in both transcripts, `free`'s used 41668 KiB before and after. `exit` by hand: `init: /bin/sh (pid 2) exited with 0; starting it again` and a new prompt without the motd; then three quick `exit`s reached the error screen (`*** Relay OS cannot run its shell ***`, `/bin/sh ended 3 times within 10 s`, the last 20 log lines with the wrapped ones fitting, `Press any key to reboot.`; photographed), and a K120 key restarted the machine. `verify-usb`: `e2fsck: clean`, 84 of 84, 5 of 5, 33 of 33. |
| 2026-10-01 | 3 and 4 (milestone 2 done, 0.3.0) | `be269cd` | Pass | `flash --full` of 0.3.0, then F10: `Relay OS 0.3.0`, every startup line `[ ok ]`, `console 1920x1200 (120x33 cells)`, `cpu: SMEP on, SMAP on`, `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB`, `[ ok ] system: 34 programs, ABI 2`. Check 3 by script: 84 of 84, `uname -a` → `Relay relay 0.3.0 x86_64`, `t-spin 1` 4594860032 iterations (the same as plans 3b and 4b; a kernel built without the poll on ticks from ring 3 made 4677697536 on 2026-10-01, so the poll costs 1.8 %); the three steps by hand as before; after `reboot` 5 of 5. Check 4 by script: 33 of 33 (`t-spawn fill` 60 children, `frames lost: 0`, every `t-fault` kind killed, `free`'s used 41668 KiB before and after); `exit` by hand restarted the shell. Then, after a `reboot`, three quick `exit`s reached the error screen (photographed): the heading at the top, `/bin/sh ended 3 times within 10 s`, the kernel log's last 20 lines from `xhci 00:14.0: port 15: reset done` to init's three, the 122-column `xhci 00:14.0: slot 4: interface 0 class 8/6/80, …` line taking two rows, and `Press any key to reboot.`; a K120 key restarted the machine. `verify-usb`: `e2fsck: clean`, `system.img: built Thu Oct  1 02:21:57 UTC 2026`, 84 of 84, 5 of 5, 33 of 33, each run after the build. |
