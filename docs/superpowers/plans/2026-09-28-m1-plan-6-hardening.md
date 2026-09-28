# Milestone 1 · Plan 6: Hardening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Milestone 1 done. Every deferred finding of plans 1–5's final reviews is fixed, with a regression test that fails first, or ruled out of milestone 1 with its reason; CodeQL's alerts 1–10 close; relay-sh gains scripts (`sh FILE`) so a NUC check runs from a file instead of being typed, and `cargo xtask verify-usb` checks what the scripts printed; `docs/hardware-test.md`, the README quick start and the spec agree with the code; the version is 0.2.0. It ends with `cargo xtask test` green and the full hardware checklist green on a stick written by `cargo xtask flash --full` (spec §1.4).

**Architecture:** Each fix lands in the unit that owns the code, test first: the heap, ACPI parsing and the input queue in the kernel's library, the loader's command line in its library, the xHCI attach path in `crates/usb` against a stricter fake controller, the shell's commands in `crates/shell`, the e2e runner in `xtask`. Scripts are relay-sh itself reading a file: the built-in `sh` reads it, `Shell` runs its lines through the same `execute` as typed lines and copies what reaches the screen into a transcript; the transcript checker is host code in `xtask`, used both by `verify-usb` on the stick and by a QEMU scenario that runs the very same check scripts from the image, so CI proves the NUC's check before the NUC runs it.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 (QMP `device_del`), e2fsprogs 1.47 (`debugfs`, `mke2fs`, `e2fsck`) for tests.

**Spec:** `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md` (§1.3, §1.4, §3, §4.2, §5.2, §5.4, §6.2, §7.2–7.4, §9.1–9.5, §10, §11 step 7, §14, §15)
**Roadmap:** `docs/superpowers/plans/2026-09-26-milestone-1-roadmap.md` — this is plan 6 of 6, the last.

## In brief

- **Size.** 33 tasks in six code pull requests, plus this plan as PR 1. None of it touches the loader's or the kernel's hardware glue beyond a changed log line; every fix is tested on the host or in QEMU.
- **The script feature** is Tasks 18–30 (PR 6), about a third of the plan with the review's fixes to it, and it belongs here: the final hardware checklist, which ends milestone 1, is the first check to run from it. Your picture of what is possible holds: there are no processes and no ELF loading (spec §1.3, §14), so no `bash` can run; relay-sh reading commands from a file can. The proposal is kept with three changes: there is no `boot.log`, because a `dmesg` line in the script puts the startup lines into its transcript; there is no `nuc-check` command, because `verify-usb` checks every transcript it finds in `/root/checks`; and `flash --kernel` does not refresh the scripts, so it keeps its promise to leave the ext2 root alone (the final check uses `flash --full`). The scripts live in `rootfs/root/checks/` and a QEMU scenario runs them (your choice). NUC check 3 becomes: boot, `sh checks/check3-a.sh`, `reboot`, boot, `sh checks/check3-b.sh`, `poweroff`, then `cargo xtask verify-usb` in Mint; only the boot screen needs a photo.
- **The prototype's review.** A fresh reviewer read the whole prototype and found 1 critical, 1 important and 5 minor real defects, almost all in the new script feature; each is fixed in a task of its own with a test that fails first (the last rows of the table below). The critical one could not show in QEMU: the check scripts would have failed on a correct NUC, which a unit test with a NUC transcript now pins.
- **Triage.** Every deferred finding, with where it went:

| Finding (review) | Decision |
|---|---|
| Plan 1 #5: the EDID from the first `EdidDiscovered` handle | Ruled out: loader glue only real firmware exercises; the NUC keeps its native mode with its one monitor (decision 13) |
| Plan 1 #6: a `set_mode` error ends the boot | Ruled out, as #5 |
| Plan 1 #7b: the command line keeps 256 bytes, the spec says 255 | Fixed, Task 5 |
| Plan 1 #9: a predictable QMP socket in the abstract namespace | Fixed, Task 15 |
| Plan 1 #11: the loader-rules test reads three files | Fixed, Task 6 |
| Plan 2: a failed `timer::init` leaves the PIC unmasked | Ruled out: interrupts are enabled only on success, so nothing fires |
| Plan 2: the interrupt-storm count never resets | Ruled out: nothing but the timer interrupts in milestone 1 |
| Plan 2: vectors 32–47 get no LAPIC EOI | Ruled out, as the storm count |
| Plan 2: `parse_fadt` reads offset 40 unguarded | Fixed, Task 2 |
| Plan 2: ACPI table lengths and XSDT pointers unchecked | Length cap fixed, Task 2; the pointers' memory-map check ruled out (the cap bounds the cost) |
| Plan 2: one failed ECAM window drops the others | Ruled out: one window on the NUC and in QEMU; the failure is a visible `[FAIL] pci` |
| Plan 2: `check=timer` blames the RTC after a timer failure | Ruled out: a diagnostic after a failure that never happens here |
| Plan 2: a 64-bit BAR in the last slot shows as `mem32` | Ruled out: only on hardware that breaks the PCI rules; a misreport, not a fault |
| Plan 2: plan 2's text for `timer::sleep` is stale | Ruled out: plans are records; the code is the reference |
| Plan 3: `ls` and `rm -r` quadratic, `read_dir` whole directories | Ruled out: milestone 1 cannot make thousands of files |
| Plan 3: `rm -r` of a path holding the current directory | Fixed, Task 11 |
| Plan 3: `cat` reads on after a write error | Fixed, Task 12 |
| Plan 3: `cp` empties the destination before failing | Fixed, Task 13 |
| Plan 3: an unquoted `#` and a `$` in double quotes pass through | Fixed, Tasks 14 and 18 |
| Plan 4 M1: a full input queue drops Ctrl-C | Fixed, Task 3 |
| Plan 4 M5: a controller that does not start is freed unhalted | Fixed, Task 7 |
| Plan 4 M6: the first scan logs empty ports as disconnected | Fixed, Task 8 |
| Plan 4 M7: the debounce is a fixed sleep | Fixed, Task 9 |
| Plan 5: a stick with a wrong residue fails every request | Ruled out: the Kingston reports none (check 3) |
| Plan 5: the 5 s bulk timeout | Kept (spec §6.2); the check scripts write an 8 MiB file on the Kingston, so the final check measures it (Task 26) |
| Plan 5: `/` is not mounted again after a replug | Ruled out: spec §15 item 11, decision 3 |
| Plan 5: disks over 2^32 blocks | Ruled out: 2 TiB |
| Plan 5: the fakes model no packet-level behaviour | Ruled out: plan 5's final-review ruling |
| Plan 5 final review: the `reboot` step does not join its reader | Fixed, Task 16 |
| Plan 5 final review: the size rule is written twice | Fixed, Task 10 |
| Plan 5 final review: no scenario unplugs the stick | Fixed, Task 17 |
| CodeQL alerts 1–10 (`heap.rs`) | Fixed, Task 1 |
| Plan 6 prototype review, critical: the check scripts fail on a correct NUC (a `#>` and a `#nuc>` line for one output line) | Fixed, Task 27 |
| Plan 6 prototype review, important: a script holds a command's whole output on the heap | Fixed, Task 22 |
| Plan 6 prototype review: the `+` line is not synced before its command runs | Fixed, Task 23 |
| Plan 6 prototype review: `verify-usb` passes a script that was not run, and old transcripts | Fixed, Task 29 |
| Plan 6 prototype review: an escape sequence over serial is still cut in two | Fixed, Task 4 |
| Plan 6 prototype review: a failed check does not say which expectation failed | Fixed, Task 25 |
| Plan 6 prototype review: script lines are trimmed before they run | Fixed, Task 20 |
| Plan 6 prototype review: `unplug` takes the first `DEVICE_DELETED` event, maybe a child's; a script saved with a byte-order mark | Fixed in Tasks 17 and 19 themselves |
| Plan 6 prototype review: a FADT without a DSDT address makes the kernel look at address 0 | Ruled out: the checksum fails and it is logged (decision 13) |

## Where this plan fits

Plan 6 implements spec §11 step 7, "hardening". NUC checks 1–3 passed, so no hardware bug is open; what they showed stays in mind: the NUC's PCH xHCI uses 32-byte contexts and 34 scratchpads, the Kingston was ready at its first TEST UNIT READY and answered every command without a residue, the root was found by the boot partition's GUID, and `poweroff` worked through S5 7/0. It builds on every earlier plan: the heap and ACPI of plan 2, the shell of plan 3, the xHCI driver and its fake of plan 4, the storage driver, the e2e runner's `reboot`/`poweroff` and `verify-usb` of plan 5. At its end milestone 1 is done (spec §1.4), at version 0.2.0.

## Working conventions

- Plan 6 lands as **seven pull requests** (table below). This plan itself is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-28-m1-plan-6-hardening.md`, to every task, and all tasks share one workspace directory.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module; test support under `crates/usb/src/testing/` and `crates/shell/src/testing.rs`, which are compiled only for tests; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 1 and 6 have no red run: the first is a refactor under the existing tests, the second only changes a test; their intros name the mutation checks that show their tests bite. Tasks 30–32 change only documents.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats (it throttled at 99 °C during plan 4) before blaming code. Under TCG they are still fast (`bigfile` takes 3 s; `checks` about a minute).
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `08eed91` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `plan6/plan` | — | This plan, spec §15 item 12, roadmap notes | `lint`, `unit`, `e2e` |
| 2 | `plan6/kernel` | 1–6 | Heap links as `Option<NonNull>` (CodeQL 1–10); ACPI length cap and short FADT; Ctrl-C through a full input queue, escape sequences whole from the keyboard and serial; the loader's 255-byte command line and rules test | `lint`, `unit`, `e2e` |
| 3 | `plan6/usb` | 7–10 | A controller that does not start halted before its memory is freed; a quiet first scan; a debounce that waits for a stable connection; one size rule | `lint`, `unit`, `e2e` |
| 4 | `plan6/shell` | 11–14 | `rm -r` of a tree holding the current directory; `cat` stopping at a write error; `cp` from a link; `$` in double quotes | `lint`, `unit`, `e2e` |
| 5 | `plan6/e2e` | 15–17 | A private QMP socket; the `reboot` step joining its reader; the `unplug` step and scenario | `lint`, `unit`, `e2e` |
| 6 | `plan6/scripts` | 18–30 | `#` comments, `sh FILE` with transcripts, the transcript checker, NUC check 3 as scripts run in QEMU by `checks`, `verify-usb` checking them, check 3 in `docs/hardware-test.md` | `lint`, `unit`, `e2e` |
| 7 | `plan6/release` | 31–33 | README quick start, the spec's body and header brought up to date, version 0.2.0; the full hardware checklist | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (spec §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. `crates/usb` has no dependencies at all; the kernel has no dev-dependencies. Plan 6 adds no crate.
- The kernel stack is 64 KiB (spec §4.2): no big arrays on the stack; buffers are `Vec`/`Box` or DMA memory. A script is read onto the heap (at most 64 KiB).
- Errors are values (spec §10): device failures are `UsbError`, `IoError` or `Errno`, never panics; nothing waits forever; `poll` never waits. Nothing a device, a disk's contents, a controller or the firmware sends may make an `unwrap`, an index or an arithmetic overflow fail (the kernel builds with overflow checks), or allocate or map without bound. A device-level failure prints `[FAIL] <step>: <reason>` and the boot continues. Panics are for kernel bugs only.
- Memory the controller may still write is never freed or reused (plan 4, decision 6).
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils; errors go to the screen, never into a redirection file (spec §7.3), and a running script's transcript gets everything the screen gets.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action in the workflow stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR (`Analyze (rust)`, `Analyze (actions)`, `CodeQL`): alerts 1–10 close with Task 1; triage any new alert, and ask the user before dismissing any.
- Tagging `v0.2.0` or creating a GitHub release is outward-facing: only if the user asks.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 12 in PR 1:

1. **Scripts** (revises §7.3 and §14). Milestone 1 has no processes and no ELF loading (§1.3), so no `bash` can run; what runs is relay-sh reading commands from a file. The built-in `sh FILE` runs the file's lines one by one through the same parser and commands as typed lines, each shown first as `+ <line>` (like `set -x`), so a photo or a transcript shows which command printed what. Blank and comment lines are skipped; a failing command or a line that does not parse does not stop the script; Ctrl-C does, and so does `reboot`/`poweroff` where they return. Every line is synced as a typed command is. There are no variables, arguments, loops, conditions or pipes, a script cannot run another, and the output of `sh` cannot be redirected. A line runs exactly as written (only its trace is trimmed). A script is at most 64 KiB of UTF-8 text, with a byte-order mark and CRLF line ends accepted as Windows editors write them; its exit status is its last command's. USB devices plugged in while a script runs are set up when it ends, since that work runs from the prompt's idle loop. Reason: the NUC's K120 is its only input, and commands cannot be pasted into it, so every hardware check was typed by hand.
2. **Transcripts.** Everything a script shows on the screen, errors included (unlike a redirection file, §7.3), also goes into a transcript next to it: `x.sh` becomes `x.log`, other names get `.log` added. It is emptied when the script starts, and written and synced as each line starts and ends, so a machine that hangs or restarts leaves every line up to the one it stopped in, that one's `+` line included; a command's output reaches it every 4 KiB, so a big `cat` is never held on the heap. If the transcript cannot be created the script does not run; if a write to it fails it ends there with a message, and the script goes on.
3. **Comments and double quotes** (§7.3; plan 3's final-review minor). An unquoted `#` at the start of a word begins a comment that runs to the end of the line, as in bash; inside a word, quoted or escaped it is a character. Inside double quotes a `$` or `` ` `` is refused as outside them (bash would expand it); `\$` and `` \` `` stand for the character.
4. **Check scripts** (§9.4). The NUC's check 3 is two scripts in `rootfs/root/checks/` (`check3-a.sh` before the `reboot`, `check3-b.sh` after it), which `image` and `flash --full` copy to `/root/checks/`; `flash --kernel` still leaves the ext2 root alone, scripts included. Under each command the script says what it must print: `#> <regex>` one whole output line, in order; `#> ...` any number of lines; `#nuc>` and `#qemu>` lines apply only to that machine, so an output line that differs is written twice, `#qemu>` then `#nuc>`; `#!> <regex>` must match no line; a command with no `#>` line must print nothing. `cargo xtask verify-usb` checks every transcript in `/root/checks` as written on the NUC and fails on a mismatch, naming the script line, the command, the expectation that failed and the line printed there, and on a script that was not run; it shows when each transcript was written, since transcripts of an earlier run stay on the stick after `flash --kernel`. A unit test checks the real scripts against a QEMU transcript and one with the NUC's recorded startup lines; the QEMU scenario `checks` runs the same scripts, with a `reboot` between them, and checks their transcripts on the disk (the e2e step `check-script <path>`). A `dmesg` line in the script checks the startup lines, so only the boot screen needs a look.
5. **Version 0.2.0** marks milestone 1 as done (0.1.0 is Cargo's default). `uname -a` takes the version from the crate's `CARGO_PKG_VERSION`, so it cannot drift from `Cargo.toml` again.
6. **The e2e runner** (§9.3). QMP listens on a socket file in the scenario's run directory, which only its owner can enter, not in the abstract namespace that every user of the machine can reach; QEMU binds the bare name in that directory and xtask connects through the directory's `/proc/self/fd` entry, so a deep checkout is no problem (Unix socket addresses hold 108 bytes). The `reboot` step waits until the serial reader has copied everything QEMU printed before it matches the pattern and continues the log. New: the step `unplug` (QMP `device_del` of the stick, id `stick-usb`, then QEMU's `DEVICE_DELETED` event for that id), the scenario `unplug` (after the stick is pulled, commands fail at once with `EIO` and `poweroff` refuses to go on without a clean shutdown), and the step `check-script`.
7. **USB** (§6.2; plan 4's M5, M6, M7). A controller whose start times out is halted before its memory is freed; if it does not halt, its memory is kept (and logged), since it may still write it. The first port scan reports only connected ports. A device is set up only after its connection was stable for 100 ms (USB 2.0 7.1.7.3): the port is looked at every 25 ms, a change of the connection or a connect change starts the 100 ms again, and after 2 s of bouncing the attach gives up. The disk-size rule of decision 3 of item 11 is one function, `usb::host::Size`, computed exactly for every size.
8. **ACPI** (§5.4). A table longer than 16 MiB is refused (the NUC's biggest, its DSDT, is 469,477 bytes), so a corrupt length is never mapped or summed; a FADT too short for its DSDT field has none.
9. **Kernel heap** (§5.2). Free-list links are `Option<NonNull<…>>`, so the compiler checks every list end; CodeQL's alerts 1–10 were false positives on the old null pointers and close with this change.
10. **Console input** (§7.2; plan 4's M1). A Ctrl-C always gets into the input queue, even a full one, and drops what was typed before it; an editing key's escape sequence goes in whole or not at all, from the keyboard and from COM1, where a sequence waits until its last byte has arrived (at most 8 bytes; Ctrl-C ends it).
11. **Loader** (§4.2; plan 1's #7b and #11). The command line keeps at most 255 bytes, as §4.2 says (`BootInfo`'s field stays 256 bytes); the test that bans exclusive protocol opens reads every loader file, without comments.
12. **Shell** (plan 3's minors). `rm -r` of a relative path works from `/`, so removing a tree that holds the current directory removes all of it, as GNU does; `cat` stops at the first write error; `cp` refuses a source it cannot read (a symbolic link, which milestone 1 does not follow) before it empties the destination.
13. **Deferred findings ruled out of milestone 1**, with the reason:
   - plan 1 #5 (the EDID from the first handle) and #6 (a `set_mode` error ends the boot): loader glue only real firmware exercises; with its one monitor the NUC keeps its native mode (check 1);
   - plan 2: a failed `timer::init` leaves the PIC unmasked, but interrupts are enabled only on success, so nothing fires; the interrupt-storm count never resets and vectors 32–47 get no LAPIC EOI, but nothing besides the timer is programmed to interrupt in milestone 1; one failed ECAM window drops the others, but the NUC and QEMU have one each and the failure is a visible `[FAIL] pci`; `check=timer` blames the RTC after a timer failure that never happens here; a 64-bit BAR in the last slot is shown as `mem32` only on hardware that breaks the PCI rules; plan 2's text for `timer::sleep` is a historical record (the code is the reference); the XSDT's pointers are not checked against the memory map (the length cap above bounds what a bad one costs), and a FADT without a DSDT address makes the kernel look for one at address 0, which fails its checksum and is logged;
   - plan 3: `ls` and `rm -r` are quadratic in big directories and `read_dir` holds a whole directory in memory: milestone 1 cannot make thousands of files (scripts have no loops), and a directory made elsewhere with 20,000 files still lists in 3 s;
   - plan 5: a stick with a wrong residue (Linux's IGNORE_RESIDUE): the Kingston reports none (check 3); the 5 s bulk timeout stays (§6.2): the check scripts write an 8 MiB file on the Kingston, so the final check shows whether it suffices; `/` is not mounted again after a replug (item 11, decision 3); disks over 2^32 blocks (2 TiB); the fakes' missing packet-level behaviour (plan 5's final review ruled that no current command's data can be an exact multiple of the packet size); plan 5's final-review minors are all fixed (the reboot step, the size rule, the unplug scenario).

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **Check scripts that do not go as planned:** a line that does not parse, a command that fails or prints an error, a script saved with CRLF line ends, a very long script, Ctrl-C during a command or between lines, `reboot` inside a script, a script run from another directory, a transcript that cannot be written (read-only `/`, a directory in its place, a full disk). Expected: every command shown as `+ <line>` before its output, the script going on past failures and stopping at Ctrl-C or a restart, the transcript on the disk up to the last line that started, a clear message instead of a silent loss, no nested scripts, no panic. Tests: `each_line_runs_after_its_trace`, `a_line_that_does_not_parse_does_not_stop_the_script`, `ctrl_c_stops_the_script`, `reboot_ends_the_script`, `a_script_cannot_run_a_script`, `a_script_saved_on_windows_runs`, `sh_errors` (Task 19), `a_line_runs_exactly_as_typed` (Task 20), `the_transcript_is_written_as_each_line_starts_and_ends`, `a_transcript_that_cannot_be_written`, `transcript_names` (Task 21), `a_command_that_prints_much_is_written_as_it_goes` (Task 22), `every_line_is_synced_as_it_starts_and_ends` (Task 23).
2. **A transcript that does not show what the script expects:** an error message where nothing was expected, lines missing, extra or out of order, a machine that hung or restarted in the middle, a transcript left by an older version of the script, output that only the other machine prints. Expected: `FAILED` naming the script line, the command and what it printed; never a pass for a transcript that does not match. Tests: `every_output_line_must_be_expected`, `a_line_that_must_not_appear_fails_the_command`, `a_transcript_that_ends_early_or_goes_astray`, `lines_tagged_for_the_other_machine_do_not_count` (Task 24), `a_failure_names_the_expectation_and_the_line` (Task 25), `the_check_scripts_pass_on_both_machines` (Task 27), `transcripts_of_the_check_scripts_are_checked_as_on_the_nuc` (Task 28), `a_script_that_was_not_run_fails_the_check` (Task 29), the `checks` scenario (Task 26).
3. **USB devices that misbehave while being attached:** a controller that does not start or does not stop, a plug that bounces, ports that show a stale connect change at boot, a stick pulled out while `/` is mounted. Expected: memory a controller may still write is never freed, a device is set up only on a connection stable for 100 ms, bounded waits, quiet boot lines, requests failing at once with `EIO` after an unplug. Tests: `a_controller_that_neither_runs_nor_halts_keeps_its_memory` (Task 7), `an_empty_port_with_a_connect_change_is_not_in_the_first_port_changes` (Task 8), the bounce tests (Task 9), the `unplug` scenario (Task 17).
4. **Typing while a long command runs:** a key held down or text pasted over serial during a big `cp` or `cat`, then Ctrl-C; arrow keys, on the keyboard or over serial, when the queue is nearly full. Expected: Ctrl-C always stops the command; the shell never sees half an escape sequence. Tests: `a_ctrl_c_gets_in_when_the_queue_is_full`, `a_key_sequence_goes_in_whole_or_not_at_all` (Task 3), `a_sequence_over_serial_goes_in_whole_or_not_at_all`, `a_ctrl_c_over_serial_ends_a_sequence` (Task 4).
5. **Firmware and shell input that lies or surprises:** an ACPI table whose length is absurd or a FADT shorter than its fields, a command line of 256 bytes, `$` or a backquote inside double quotes, `rm -r` of a tree holding the current directory, `cp` from a symbolic link, `cat` into a full disk. Expected: no panic and no huge mapping, the spec's 255-byte limit, the same refusals as outside quotes, the GNU behaviour. Tests: `a_table_longer_than_any_real_one_is_not_read`, `a_fadt_too_short_for_its_dsdt_field_has_none` (Task 2), `keeps_at_most_255_bytes` (Task 5), `dollar_and_backquote_inside_double_quotes_are_unsupported` (Task 14), `rm_r_removes_a_tree_holding_the_current_directory` (Task 11), `cp_of_a_symlink_leaves_the_destination_alone` (Task 13), `cat_stops_at_the_first_write_error` (Task 12).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `kernel/src/mm/heap.rs` | Free-list links as `Option<NonNull>` |
| `kernel/src/acpi/tables.rs` | `MAX_TABLE`, `AcpiError::TooLong`, a FADT without its DSDT field |
| `kernel/src/{input,session}.rs` | Ctrl-C always gets in; sequences whole, also over serial (`push_serial`) |
| `boot/src/{cmdline,lib}.rs` | `LONGEST = 255`; the loader-rules test over every file |
| `crates/usb/src/xhci/{start,port,device}.rs`, `crates/usb/src/testing/xhci/**` | Halt before free; the quiet first scan; the debounce; the fake's `starts_late` |
| `crates/usb/src/host{,/boot_line}.rs`, `kernel/src/storage.rs` | `usb::host::Size`, used by the `mount /` line |
| `crates/shell/src/{parser,ctx,shell,transcript}.rs` | Comments, double quotes; `Ctx::{out_failed, script, in_script, transcript}`; running scripts; the transcript, written every 4 KiB |
| `crates/shell/src/commands/{change,text,script,mod,basic}.rs` | `rm -r`, `cp`, `cat`; the built-in `sh`; `uname` from Cargo |
| `crates/shell/src/testing.rs` | Test support: `SpyState::{fail_unlink, reads, largest_write}`, `TestConsole::interrupt_after` |
| `xtask/src/{qemu,qmp,e2e}.rs` | The private QMP socket, `wait_event`, the `unplug` and `check-script` steps, the joined reader |
| `xtask/src/{checks,flash,main}.rs` | The transcript checker; `verify-usb` checking the stick's transcripts |
| `rootfs/root/checks/check3-{a,b}.sh` | NUC check 3 as scripts |
| `xtask/fixtures/checks/check3-{a,b}.{qemu,nuc}.log` | Test data: a transcript of each script on each machine |
| `tests/e2e/{unplug,checks,fileops,shell}.txt` | The unplug and checks scenarios; `/root` with `checks`; version 0.2.0 |
| `docs/hardware-test.md`, `README.md`, the spec | Check 3 by script; the quick start; the body and header up to date |
| `Cargo.toml`, `Cargo.lock` | Version 0.2.0 |

---

## PR 1: The plan (already committed)

This plan, the spec's §15 item 12 and the roadmap notes are committed on branch `plan6/plan` (worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-plan`). They change no code, but CI runs on every pull request. Run the commands below from that worktree.

- [ ] **Push and open the pull request**

````bash
git push -u origin plan6/plan
gh pr create --base main --head plan6/plan --title "Plan 6: Hardening (plan document)" --body-file - <<'EOF'
## What

Milestone 1, plan 6 (hardening), the last plan of the milestone: the full implementation plan, the spec's §15 item 12 (decisions made while planning, among them relay-sh scripts for the NUC checks and the triage of every deferred review finding) and roadmap notes.

## How it was tested

- [x] Every task was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks plan6/plan --watch`). Ask the user to review and merge. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-plan` and continue with PR 2.

---

## PR 2: Kernel and loader (Tasks 1–6)

The heap's free lists as `Option<NonNull>`, which closes CodeQL alerts 1–10; firmware tables that lie about their length; Ctrl-C through a full input queue; the loader's 255-byte command line and its rules test over every file.

Branch `plan6/kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-kernel plan6/plan`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: Heap free-list links as `Option<NonNull>`

Roadmap note "Heap free-list links as `Option<NonNull<…>>`": CodeQL's `rust/access-invalid-pointer` reports ten alerts in `kernel/src/mm/heap.rs` (code-scanning alerts 1–10). They are false positives: the only invalid pointers are the `ptr::null_mut()` list ends, and every dereference sits behind an `is_null()` check that CodeQL does not model. The free-list links (`Heap::classes`, `Heap::large`, `FreeBlock::next`, the `prev` pointers of the list walks) become `Link<T> = Option<NonNull<T>>`, so the compiler makes every list end a `None` that must be matched, and a small block is a `SmallBlock { next }` instead of a bare pointer word. The layout stays the same (the null niche keeps `FreeBlock` at 16 bytes, `MIN_BLOCK`), so no behaviour changes: this is a refactor under the existing tests, above all `random_mix_never_overlaps` (20,000 seeded operations with every block's contents and alignment checked), and it has no red run. Mutation checks (dropping either merge in `insert_free`, dropping the pop in `alloc_small`) each fail those tests. The alerts close when this lands.

**Files:**
- Modify: `kernel/src/mm/heap.rs`

**Interfaces:**
- Consumes: plan 2's `Heap`.
- Produces: no interface change; `Heap`'s links are `Option<NonNull<…>>`.

- [ ] **Step 1: Change `kernel/src/mm/heap.rs`**

In `kernel/src/mm/heap.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};
use spin::Mutex;
````

with:

````rust
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use spin::Mutex;
````

Replace:

````rust
    size: usize,
    next: *mut FreeBlock,
}

````

with:

````rust
    size: usize,
    next: Link<FreeBlock>,
}

/// A free small block: only the link to the next one of its class.
struct SmallBlock {
    next: Link<SmallBlock>,
}

/// A free-list link; `None` ends the list. Every block on a list is free
/// heap memory, so a `Some` may be read and written.
type Link<T> = Option<NonNull<T>>;

````

Replace:

````rust
    end: usize,
    classes: [*mut u8; SIZE_CLASSES.len()],
    large: *mut FreeBlock,
    used: usize,
}

// SAFETY: the raw pointers point into the heap region, which the Heap owns.
unsafe impl Send for Heap {}
````

with:

````rust
    end: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
    large: Link<FreeBlock>,
    used: usize,
}

// SAFETY: the links point into the heap region, which the Heap owns.
unsafe impl Send for Heap {}
````

Replace:

````rust
            end: 0,
            classes: [ptr::null_mut(); SIZE_CLASSES.len()],
            large: ptr::null_mut(),
            used: 0,
````

with:

````rust
            end: 0,
            classes: [None; SIZE_CLASSES.len()],
            large: None,
            used: 0,
````

Replace:

````rust
        match class_of(layout) {
            Some(c) => unsafe {
                *(addr as *mut *mut u8) = self.classes[c];
                self.classes[c] = addr as *mut u8;
                self.used -= SIZE_CLASSES[c];
            },
            None => {
````

with:

````rust
        match class_of(layout) {
            Some(c) => {
                let block = ptr.cast::<SmallBlock>();
                unsafe {
                    block.write(SmallBlock {
                        next: self.classes[c],
                    })
                };
                self.classes[c] = Some(block);
                self.used -= SIZE_CLASSES[c];
            }
            None => {
````

Replace:

````rust
        let (mut free_large, mut largest_free) = (0, 0);
        let mut b = self.large;
        while !b.is_null() {
            let size = unsafe { (*b).size };
            free_large += size;
            largest_free = largest_free.max(size);
            b = unsafe { (*b).next };
        }
````

with:

````rust
        let (mut free_large, mut largest_free) = (0, 0);
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size, next } = unsafe { b.read() };
            free_large += size;
            largest_free = largest_free.max(size);
            link = next;
        }
````

Replace:

````rust
    fn alloc_small(&mut self, c: usize) -> Option<usize> {
        if self.classes[c].is_null() {
            let slab = self.alloc_large(SLAB, SLAB)?;
            let size = SIZE_CLASSES[c];
            // Push the blocks in reverse so they are handed out in address
            // order.
            for block in (slab..slab + SLAB).step_by(size).rev() {
                unsafe { *(block as *mut *mut u8) = self.classes[c] };
                self.classes[c] = block as *mut u8;
            }
        }
        let block = self.classes[c];
        self.classes[c] = unsafe { *(block as *mut *mut u8) };
        self.used += SIZE_CLASSES[c];
        Some(block as usize)
    }
````

with:

````rust
    fn alloc_small(&mut self, c: usize) -> Option<usize> {
        let block = match self.classes[c] {
            Some(block) => block,
            None => self.refill(c)?,
        };
        self.classes[c] = unsafe { block.read() }.next;
        self.used += SIZE_CLASSES[c];
        Some(block.as_ptr() as usize)
    }

    /// Carves a new slab into blocks of class `c` and returns the first.
    fn refill(&mut self, c: usize) -> Option<NonNull<SmallBlock>> {
        let slab = self.alloc_large(SLAB, SLAB)?;
        let size = SIZE_CLASSES[c];
        // Push the blocks in reverse so they are handed out in address
        // order.
        for addr in (slab..slab + SLAB).step_by(size).rev() {
            let block = block_at::<SmallBlock>(addr);
            unsafe {
                block.write(SmallBlock {
                    next: self.classes[c],
                })
            };
            self.classes[c] = Some(block);
        }
        self.classes[c]
    }
````

Replace:

````rust
    fn alloc_large(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: *mut FreeBlock = ptr::null_mut();
        let mut b = self.large;
        while !b.is_null() {
            let (start, bsize, next) = unsafe { (b as usize, (*b).size, (*b).next) };
            let aligned = start.next_multiple_of(align);
            if aligned + size <= start + bsize {
                // Unlink, then return the unused head and tail.
                if prev.is_null() {
                    self.large = next;
                } else {
                    unsafe { (*prev).next = next };
                }
````

with:

````rust
    fn alloc_large(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: Link<FreeBlock> = None;
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size: bsize, next } = unsafe { b.read() };
            let start = b.as_ptr() as usize;
            let aligned = start.next_multiple_of(align);
            if aligned + size <= start + bsize {
                // Unlink, then return the unused head and tail.
                match prev {
                    None => self.large = next,
                    Some(mut p) => unsafe { p.as_mut().next = next },
                }
````

Replace:

````rust
            }
            prev = b;
            b = next;
        }
````

with:

````rust
            }
            prev = link;
            link = next;
        }
````

Replace:

````rust
    unsafe fn insert_free(&mut self, addr: usize, size: usize) {
        let mut prev: *mut FreeBlock = ptr::null_mut();
        let mut next = self.large;
        while !next.is_null() && (next as usize) < addr {
            prev = next;
            next = unsafe { (*next).next };
        }
        unsafe {
            let block = addr as *mut FreeBlock;
            block.write(FreeBlock { size, next });
            if !next.is_null() && addr + size == next as usize {
                (*block).size += (*next).size;
                (*block).next = (*next).next;
            }
            if prev.is_null() {
                self.large = block;
            } else if prev as usize + (*prev).size == addr {
                (*prev).size += (*block).size;
                (*prev).next = (*block).next;
            } else {
                (*prev).next = block;
            }
        }
    }
}
````

with:

````rust
    unsafe fn insert_free(&mut self, addr: usize, size: usize) {
        let mut prev: Link<FreeBlock> = None;
        let mut next = self.large;
        while let Some(n) = next.filter(|n| (n.as_ptr() as usize) < addr) {
            prev = next;
            next = unsafe { n.read() }.next;
        }
        let mut block = block_at::<FreeBlock>(addr);
        unsafe { block.write(FreeBlock { size, next }) };
        let b = unsafe { block.as_mut() };
        if let Some(n) = next.filter(|n| addr + size == n.as_ptr() as usize) {
            let n = unsafe { n.read() };
            b.size += n.size;
            b.next = n.next;
        }
        match prev {
            None => self.large = Some(block),
            Some(mut p) => {
                let p = unsafe { p.as_mut() };
                if p as *mut FreeBlock as usize + p.size == addr {
                    p.size += b.size;
                    p.next = b.next;
                } else {
                    p.next = Some(block);
                }
            }
        }
    }
}

/// The block at `addr`, a heap address (never 0: the heap is in the
/// kernel's upper half).
fn block_at<T>(addr: usize) -> NonNull<T> {
    NonNull::new(addr as *mut T).expect("heap block at address 0")
}
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 178 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add kernel
git commit -m "kernel: heap free-list links are Option<NonNull>, so every dereference is checked"
````


### Task 2: ACPI tables that lie about their length

Two findings of plan 2's final review, both in `acpi/tables.rs`. `read_table` maps and sums as many bytes as a table's header claims, up to 4 GiB, so one corrupt length makes the kernel map memory it does not own and loop over it for minutes; a table longer than 16 MiB is now refused as `TooLong` (the biggest table seen, the NUC's DSDT, is 469,477 bytes; decision 8). And `parse_fadt` reads the DSDT address at offset 40 of any FADT that passed `read_table`, which only guarantees the 36-byte header: a short FADT with a valid checksum panicked the kernel at boot. Now it has no DSDT. Firmware data must never make the kernel fault (spec §10).

**Files:**
- Modify: `kernel/src/acpi/tables.rs`

**Interfaces:**
- Consumes: plan 2's `acpi::tables`.
- Produces: `acpi::tables::{MAX_TABLE = 16 MiB, AcpiError::TooLong([u8; 4], usize)}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/acpi/tables.rs`**

In `kernel/src/acpi/tables.rs`, replace:

````rust
    #[test]
    fn a_table_shorter_than_its_header_is_rejected() {
````

with:

````rust
    #[test]
    fn a_table_longer_than_any_real_one_is_not_read() {
        // A length of 4 GiB would be mapped and summed byte by byte.
        let mut m = FakeMem::default();
        let mut t = vec![0u8; 36];
        t[..4].copy_from_slice(b"DSDT");
        t[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        m.put(0x1000, &t);
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::TooLong(*b"DSDT", u32::MAX as usize))
        );
        assert_eq!(
            AcpiError::TooLong(*b"DSDT", u32::MAX as usize).to_string(),
            "DSDT claims 4294967295 bytes"
        );
        // The NUC's DSDT, the biggest table seen, is 469,477 bytes.
        t[4..8].copy_from_slice(&(MAX_TABLE as u32).to_le_bytes());
        m.put(0x1000, &t);
        assert_eq!(
            read_table(&mut m, 0x1000),
            Err(AcpiError::Unreadable(0x1000))
        );
    }

    #[test]
    fn a_fadt_too_short_for_its_dsdt_field_has_none() {
        // A valid 36-byte table: header only.
        let mut t = fixture!("qemu", "FACP")[..36].to_vec();
        t[4..8].copy_from_slice(&36u32.to_le_bytes());
        let fadt = parse_fadt(&with_checksum(t, 9));
        assert_eq!(fadt.dsdt, 0);
        assert_eq!((fadt.pm1a_cnt, fadt.reset, fadt.century), (None, None, 0));
    }

    #[test]
    fn a_table_shorter_than_its_header_is_rejected() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `MAX_TABLE` in this scope ``; `` no variant, associated function, or constant named `TooLong` found for enum `acpi::tables::AcpiError` in the current scope ``.

- [ ] **Step 3: Change `kernel/src/acpi/tables.rs`**

In `kernel/src/acpi/tables.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    TooShort([u8; 4]),
    BadChecksum([u8; 4]),
````

with:

````rust
    TooShort([u8; 4]),
    /// A table's header says it is longer than `MAX_TABLE`.
    TooLong([u8; 4], usize),
    BadChecksum([u8; 4]),
````

Replace:

````rust
            AcpiError::TooShort(s) => write!(f, "{} is too short", sig(s)),
            AcpiError::BadChecksum(s) => write!(f, "{} checksum is wrong", sig(s)),
````

with:

````rust
            AcpiError::TooShort(s) => write!(f, "{} is too short", sig(s)),
            AcpiError::TooLong(s, len) => write!(f, "{} claims {len} bytes", sig(s)),
            AcpiError::BadChecksum(s) => write!(f, "{} checksum is wrong", sig(s)),
````

Replace:

````rust

/// Reads the table at `phys` and checks its length and checksum.
````

with:

````rust

/// The longest table read: 16 MiB, far above any real one (the NUC's
/// DSDT is 469,477 bytes), so a corrupt length is never mapped or summed.
pub const MAX_TABLE: usize = 16 << 20;

/// Reads the table at `phys` and checks its length and checksum.
````

Replace:

````rust
        return Err(AcpiError::TooShort(signature));
    }
````

with:

````rust
        return Err(AcpiError::TooShort(signature));
    }
    if len > MAX_TABLE {
        return Err(AcpiError::TooLong(signature, len));
    }
````

Replace:

````rust
        true => u64_at(t, 140),
        false => u32_at(t, 40) as u64,
    };
````

with:

````rust
        true => u64_at(t, 140),
        false => t.get(40..44).map_or(0, |_| u32_at(t, 40) as u64),
    };
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 180 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: ACPI tables longer than 16 MiB are refused, and a short FADT has no DSDT"
````


### Task 3: Ctrl-C gets into a full input queue

Plan 4's final-review minor M1. The console's input queue holds 4 KiB of type-ahead and drops what does not fit, so a key held down (30 repeats a second) or text pasted over COM1 during a long `cp` fills it, and then a Ctrl-C is dropped too: the command cannot be stopped. A full queue could also keep only `ESC` or `ESC [` of an arrow key, which the line editor would then read as other keys. A Ctrl-C now always gets in, dropping what was typed before it as `take_interrupt` would anyway (a terminal flushes its input on an interrupt), and an escape sequence goes in whole or not at all (decision 10).

**Files:**
- Modify: `kernel/src/input.rs`

**Interfaces:**
- Consumes: plan 4's `input::InputQueue`.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    #[test]
    fn a_ctrl_c_gets_in_when_the_queue_is_full() {
        // A key held down during a long command fills the queue; Ctrl-C
        // must still stop the command.
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX]);
        q.push_key(&press(Key::Char(b'c'), true));
        assert!(q.take_interrupt());
        assert!(q.is_empty());
        q.push(&[b'a'; QUEUE_MAX]);
        q.push(b"x\x03y");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"y");
    }

    #[test]
    fn a_key_sequence_goes_in_whole_or_not_at_all() {
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 2]);
        q.push_key(&press(Key::Up, false));
        assert_eq!(drain(&mut q), [b'a'; QUEUE_MAX - 2]);
        q.push(&[b'a'; QUEUE_MAX - 3]);
        q.push_key(&press(Key::Up, false));
        assert!(drain(&mut q).ends_with(b"a\x1b[A"));
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 2 tests fail: `input::tests::a_ctrl_c_gets_in_when_the_queue_is_full`, `input::tests::a_key_sequence_goes_in_whole_or_not_at_all`.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Adds input; what does not fit is dropped.
    pub fn push(&mut self, bytes: &[u8]) {
        let room = QUEUE_MAX - self.bytes.len();
````

with:

````rust

    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would.
    pub fn push(&mut self, bytes: &[u8]) {
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.clear();
                &bytes[i..]
            }
            None => bytes,
        };
        let room = QUEUE_MAX - self.bytes.len();
````

Replace:

````rust
        if let Some(seq) = sequence(e.key) {
            self.push(seq);
        } else if let Some(b) = byte(e) {
````

with:

````rust
        if let Some(seq) = sequence(e.key) {
            // Half a sequence would reach the shell as other keys.
            if self.bytes.len() + seq.len() <= QUEUE_MAX {
                self.push(seq);
            }
        } else if let Some(b) = byte(e) {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 182 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: a Ctrl-C gets into a full input queue, and key sequences go in whole"
````


### Task 4: An escape sequence over serial goes in whole

Review finding (minor): Task 3 keeps a key's escape sequence whole, but COM1's bytes arrive one at a time, and a nearly full queue could still keep only `ESC [` of an arrow key sent over serial; the line editor then took the next key, even an Enter, as the sequence's end. `InputQueue::push_serial` holds a sequence (`ESC` and one byte, or `ESC [` up to its final byte) until it is complete and then puts it in whole or not at all; Ctrl-C ends it, and one longer than 8 bytes is dropped (decision 10). The NUC has no serial port, so this matters to QEMU and the e2e runner.

**Files:**
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/session.rs`

**Interfaces:**
- Consumes: Task 3.
- Produces: `InputQueue::push_serial(&mut self, u8)`; the console's poll uses it for COM1.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    }

    fn serial(q: &mut InputQueue, bytes: &[u8]) {
        for &b in bytes {
            q.push_serial(b);
        }
    }

    #[test]
    fn a_sequence_over_serial_goes_in_whole_or_not_at_all() {
        // COM1 bytes arrive one at a time; the queue must not keep only
        // the start of an arrow key.
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 2]);
        serial(&mut q, b"\x1b[A");
        assert_eq!(drain(&mut q), [b'a'; QUEUE_MAX - 2]);
        q.push(&[b'a'; QUEUE_MAX - 3]);
        serial(&mut q, b"\x1b[Ax");
        assert!(drain(&mut q).ends_with(b"a\x1b[A"));
        serial(&mut q, b"l\x1b[3~s\x1bx");
        assert_eq!(drain(&mut q), b"l\x1b[3~s\x1bx");
    }

    #[test]
    fn a_ctrl_c_over_serial_ends_a_sequence() {
        let mut q = InputQueue::new();
        serial(&mut q, b"ls\x1b[\x03pwd");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd");
        // A sequence that never ends is not kept for ever: its start is
        // dropped and what follows is plain input again.
        serial(&mut q, b"\x1b[11111111111111111111");
        serial(&mut q, b"ok");
        let got = drain(&mut q);
        assert!(!got.contains(&0x1b) && got.ends_with(b"1ok"), "{got:?}");
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `push_serial` found for mutable reference `&mut input::InputQueue` in the current scope ``.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use alloc::collections::VecDeque;
use usb::hid::{Key, KeyEvent};
````

with:

````rust
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use usb::hid::{Key, KeyEvent};
````

Replace:

````rust

pub struct InputQueue {
    bytes: VecDeque<u8>,
}
````

with:

````rust

/// The longest escape sequence kept while it arrives over serial; longer
/// ones are garbage and dropped.
const SEQUENCE_MAX: usize = 8;

pub struct InputQueue {
    bytes: VecDeque<u8>,
    /// An escape sequence arriving over serial, until it is complete.
    sequence: Vec<u8>,
}
````

Replace:

````rust
            bytes: VecDeque::new(),
        }
````

with:

````rust
            bytes: VecDeque::new(),
            sequence: Vec::new(),
        }
    }

    /// Adds one byte from COM1. An escape sequence (`ESC` and one byte, or
    /// `ESC [` up to its final byte) waits until it is complete and then
    /// goes in whole or not at all, like a key's; Ctrl-C ends it.
    pub fn push_serial(&mut self, b: u8) {
        if b == INTERRUPT {
            self.sequence.clear();
            self.push(&[b]);
        } else if self.sequence.is_empty() && b != 0x1B {
            self.push(&[b]);
        } else {
            self.sequence.push(b);
            let s = &self.sequence;
            let done = s.len() == 2 && s[1] != b'[' || s.len() > 2 && (0x40..=0x7E).contains(&b);
            if done {
                let seq = core::mem::take(&mut self.sequence);
                if self.bytes.len() + seq.len() <= QUEUE_MAX {
                    self.push(&seq);
                }
            } else if s.len() >= SEQUENCE_MAX {
                self.sequence.clear();
            }
        }
````

- [ ] **Step 4: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust
            match serial::read_byte() {
                Some(b) => self.input.push(&[b]),
                None => break,
````

with:

````rust
            match serial::read_byte() {
                Some(b) => self.input.push_serial(b),
                None => break,
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 184 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: an escape sequence over serial goes into the input queue whole"
````


### Task 5: The command line keeps at most 255 bytes

Plan 1's final-review finding #7b: spec §4.2 says the `cmdline` file is "one line, at most 255 bytes", but the loader keeps up to `CMDLINE_MAX` = 256, the size of `BootInfo`'s field. The loader now cuts at `LONGEST` = 255 (still on a character boundary); `BootInfo` keeps its 256-byte field, so its layout does not change.

**Files:**
- Modify: `boot/src/cmdline.rs`
- Modify: `boot/src/main.rs`

**Interfaces:**
- Consumes: plan 1's `boot::cmdline::normalize`.
- Produces: `boot::cmdline::LONGEST = 255`.

- [ ] **Step 1: Add the failing tests to `boot/src/cmdline.rs`**

In `boot/src/cmdline.rs`, replace:

````rust
    fn truncates_on_a_char_boundary() {
        let mut long = "a".repeat(CMDLINE_MAX - 1);
        long.push('\u{e9}'); // 2 bytes, would straddle the limit
        let n = normalize(long.as_bytes());
        assert_eq!(n.len(), CMDLINE_MAX - 1);
        assert!(n.chars().all(|c| c == 'a'));
    }
}
````

with:

````rust
    fn truncates_on_a_char_boundary() {
        let mut long = "a".repeat(LONGEST - 1);
        long.push('\u{e9}'); // 2 bytes, would straddle the limit
        let n = normalize(long.as_bytes());
        assert_eq!(n.len(), LONGEST - 1);
        assert!(n.chars().all(|c| c == 'a'));
    }

    /// Spec §4.2: "one line, at most 255 bytes".
    #[test]
    fn keeps_at_most_255_bytes() {
        assert_eq!(normalize("a".repeat(256).as_bytes()).len(), 255);
        assert_eq!(normalize("a".repeat(255).as_bytes()).len(), 255);
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-boot --lib`

Expected: FAIL: compile errors such as `` cannot find value `LONGEST` in this scope ``.

- [ ] **Step 3: Change `boot/src/cmdline.rs`**

In `boot/src/cmdline.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use boot_info::CMDLINE_MAX;

````

with:

````rust
use boot_info::CMDLINE_MAX;

/// The longest command line (spec §4.2: at most 255 bytes); it fits
/// `BootInfo`'s `CMDLINE_MAX`-byte field.
pub const LONGEST: usize = 255;
const _: () = assert!(LONGEST <= CMDLINE_MAX);

````

Replace:

````rust
/// (including CR/LF from editors) is trimmed, and the result is shortened to
/// fit `CMDLINE_MAX` bytes without splitting a character.
pub fn normalize(raw: &[u8]) -> &str {
````

with:

````rust
/// (including CR/LF from editors) is trimmed, and the result is shortened to
/// fit `LONGEST` bytes without splitting a character.
pub fn normalize(raw: &[u8]) -> &str {
````

Replace:

````rust
    let mut t = text.trim_start_matches('\u{FEFF}').trim();
    if t.len() > CMDLINE_MAX {
        let mut end = CMDLINE_MAX;
        while !t.is_char_boundary(end) {
````

with:

````rust
    let mut t = text.trim_start_matches('\u{FEFF}').trim();
    if t.len() > LONGEST {
        let mut end = LONGEST;
        while !t.is_char_boundary(end) {
````

- [ ] **Step 4: Change `boot/src/main.rs`**

In `boot/src/main.rs`, replace:

````rust
    let mut cmd = [0u8; CMDLINE_MAX];
    let n = cmdline.len(); // normalize() guarantees n <= CMDLINE_MAX
    cmd[..n].copy_from_slice(cmdline.as_bytes());
````

with:

````rust
    let mut cmd = [0u8; CMDLINE_MAX];
    let n = cmdline.len(); // normalize() keeps at most 255 bytes
    cmd[..n].copy_from_slice(cmdline.as_bytes());
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-boot --lib`

Expected: PASS: 19 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add boot
git commit -m "boot: the command line keeps at most 255 bytes, as the spec says"
````


### Task 6: The loader-rules test reads every loader file

Plan 1's final-review finding #11. The test that keeps the loader off firmware features the Linux EFI stub never uses (exclusive protocol opens, which stopped the NUC's firmware console, and OS-defined memory types, which hung its `ExitBootServices`; spec §15 item 7) read only `main.rs`, `video.rs` and `paging.rs`, and missed `proto.rs`, the file that opens protocols. It now reads every file in `boot/src` but `lib.rs` (which holds the banned words itself), and a second test fails when a file is added without being listed. Comments are left out before the check: `proto.rs` explains in a comment why exclusive opens are banned. This task only changes a test, so it has no red run; its mutation checks are that dropping `proto.rs` from the list, dropping the comment stripping, or adding an `open_protocol_exclusive` call to `proto.rs` each fail it.

**Files:**
- Modify: `boot/src/lib.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `boot/src/lib.rs`**

Replace the whole of `boot/src/lib.rs` with:

````rust
//! Host-testable parts of the Relay OS UEFI loader.
#![cfg_attr(not(test), no_std)]

pub mod cmdline;
pub mod elf;
pub mod memmap;
pub mod mode;

#[cfg(test)]
mod tests {
    /// Every source file of the loader but this one, which names the banned
    /// words itself.
    const SOURCES: [(&str, &str); 8] = [
        ("cmdline.rs", include_str!("cmdline.rs")),
        ("elf.rs", include_str!("elf.rs")),
        ("main.rs", include_str!("main.rs")),
        ("memmap.rs", include_str!("memmap.rs")),
        ("mode.rs", include_str!("mode.rs")),
        ("paging.rs", include_str!("paging.rs")),
        ("proto.rs", include_str!("proto.rs")),
        ("video.rs", include_str!("video.rs")),
    ];

    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature.
    fn code(src: &str) -> String {
        src.lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_source_file_is_checked() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f != "lib.rs")
            .collect();
        files.sort();
        let checked: Vec<&str> = SOURCES.iter().map(|(f, _)| *f).collect();
        assert_eq!(files, checked);
    }

    #[test]
    fn comments_do_not_count() {
        assert_eq!(
            code("let x = 1; // Exclusive\n/// Exclusive"),
            "let x = 1; \n"
        );
    }

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
    /// drivers (the text console stops drawing once the GOP is held) and a
    /// later ExitBootServices never returns. Like the Linux EFI stub, the
    /// loader only uses non-exclusive GET_PROTOCOL opens (`proto::get`).
    #[test]
    fn loader_avoids_firmware_features_linux_does_not_use() {
        for (file, src) in SOURCES {
            let src = code(src);
            // OS-defined memory types (0x8000_0000+) are legal but never used
            // by the Linux EFI stub; the loader sticks to standard types.
            for banned in [
                "open_protocol_exclusive",
                "get_image_file_system",
                "Exclusive",
                "MemoryType::custom",
            ] {
                assert!(!src.contains(banned), "{file} uses {banned}");
            }
        }
    }
}
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p relay-boot --lib`

Expected: PASS: 21 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add boot
git commit -m "boot: the loader-rules test reads every loader file, without its comments"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 18 scenario(s) passed`.

````bash
git push -u origin plan6/kernel
gh pr create --base main --head plan6/kernel --title "Plan 6: Kernel and loader" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 1–6: the heap's free-list links as `Option<NonNull>` (CodeQL alerts 1–10 close); ACPI tables over 16 MiB refused and a short FADT without a DSDT (plan 2's review); a Ctrl-C always gets into the input queue and escape sequences go in whole, from the keyboard and over serial (plan 4's M1); the loader keeps at most 255 bytes of command line (plan 1's #7b) and its rules test reads every loader file (plan 1's #11).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: USB attach (Tasks 7–10)

Plan 4's minors M5, M6 and M7 and plan 5's size rule: memory a controller may still write is kept, the first scan is quiet, a device is set up only on a stable connection, and one function prints disk sizes.

Branch `plan6/usb`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-usb`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/usb /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-usb origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-usb
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/kernel` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/usb /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-usb plan6/kernel`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/kernel>` and re-run `cargo xtask ci` before pushing.

### Task 7: A controller that does not start is halted before its memory is freed

Plan 4's final-review minor M5. When a controller does not show HCH = 0 within 1 s of R/S = 1, `run` wrote USBCMD back without R/S and `Xhci::new` freed the DCBAA, the rings and the scratchpads at once, without confirming the controller had halted. A controller that starts late could then write into frames the kernel hands out again. Now it is halted (`init::halt`: R/S = 0 and up to 1 s for HCH = 1) first; its memory is freed only if the halt is confirmed and is otherwise kept, with the log line `memory kept: the controller did not halt` (plan 4, decision 6: memory a controller may still write is never freed). The fake controller gains the knob `starts_late`: R/S = 1 never clears HCH, and after R/S = 0 it halts only after `halt_time`.

**Files:**
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Modify: `crates/usb/src/testing/xhci/rings.rs`
- Modify: `crates/usb/src/xhci/start.rs`

**Interfaces:**
- Consumes: plan 4's `xhci::{start, init::halt}` and the fake controller.
- Produces: `Memory::keep` (private to `xhci::start`); fake knob `FakeConfig::starts_late`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub run_time: Option<Duration>,
    /// Knob: commands of this TRB type never complete.
````

with:

````rust
    pub run_time: Option<Duration>,
    /// Knob: a controller that starts late. R/S = 1 does not clear HCH
    /// however long the driver waits; the controller starts as R/S is
    /// cleared again (HCH reads 0), and halts only `halt_time` later
    /// (`None`: never).
    pub starts_late: bool,
    /// Knob: commands of this TRB type never complete.
````

Replace:

````rust
            run_time: Some(Duration::ZERO),
            hang_command: None,
````

with:

````rust
            run_time: Some(Duration::ZERO),
            starts_late: false,
            hang_command: None,
````

Replace:

````rust
            run_time: Some(Duration::from_micros(500)),
            hang_command: None,
````

with:

````rust
            run_time: Some(Duration::from_micros(500)),
            starts_late: false,
            hang_command: None,
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, replace:

````rust
        if was & RUN != 0 && value & RUN == 0 {
            if let Some(delay) = self.config.halt_time {
````

with:

````rust
        if was & RUN != 0 && value & RUN == 0 {
            if self.config.starts_late {
                self.usbsts &= !HCH;
            }
            if let Some(delay) = self.config.halt_time {
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/rings.rs`**

In `crates/usb/src/testing/xhci/rings.rs`, replace:

````rust
        self.scratchpad_pages = self.read_scratchpads(dma);
        if let Some(delay) = self.config.run_time {
            self.after(delay, |x, _| x.usbsts &= !HCH);
````

with:

````rust
        self.scratchpad_pages = self.read_scratchpads(dma);
        if let Some(delay) = self.config.run_time.filter(|_| !self.config.starts_late) {
            self.after(delay, |x, _| x.usbsts &= !HCH);
````

- [ ] **Step 4: Add the failing tests to `crates/usb/src/xhci/start.rs`**

In `crates/usb/src/xhci/start.rs`, replace:

````rust
    #[test]
    fn a_failed_halt_or_reset_allocates_nothing() {
````

with:

````rust
    #[test]
    fn a_controller_that_neither_runs_nor_halts_keeps_its_memory() {
        let mut config = FakeConfig::intel();
        // Found halted, so the first halt does not need it to halt.
        config.running = false;
        config.starts_late = true;
        config.halt_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert!(hal.fake().running(), "it started after all");
        // The scratchpads and their array, the DCBAA, both rings and the
        // ERST: the controller may still write any of them.
        assert_eq!(hal.outstanding_dma(), 7);
        let log = hal.log_text();
        assert!(log.contains("USBSTS.HCH still 0 1000 ms after R/S = 0"));
        assert!(log.ends_with("xhci 00:14.0: memory kept: the controller did not halt"));
    }

    #[test]
    fn a_controller_that_does_not_run_but_halts_when_asked_is_freed() {
        let mut config = FakeConfig::intel();
        config.starts_late = true;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert!(!hal.fake().running());
        assert_eq!(hal.outstanding_dma(), 0);
        assert!(!hal.log_text().contains("memory kept"));
    }

    #[test]
    fn a_failed_halt_or_reset_allocates_nothing() {
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 2 tests fail: `xhci::start::tests::a_controller_that_neither_runs_nor_halts_keeps_its_memory`, `xhci::start::tests::a_controller_that_does_not_run_but_halts_when_asked_is_freed`.

- [ ] **Step 6: Change `crates/usb/src/xhci/start.rs`**

In `crates/usb/src/xhci/start.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            s.free(hal);
        }
    }
}

/// Programs the rings and starts the controller (xHCI 4.2): slots, DCBAA,
````

with:

````rust
            s.free(hal);
        }
    }

    /// Leaks all of it: a controller that did not halt may still write any
    /// of it (spec §15 item 10: nothing of a dead controller is freed).
    fn keep<H: Hal>(self, hal: &H, name: &str) {
        xlog!(hal, name, "memory kept: the controller did not halt");
        core::mem::forget(self);
    }
}

/// Programs the rings and starts the controller (xHCI 4.2): slots, DCBAA,
````

Replace:

````rust
    /// (the PCI address, "00:14.0") starts every log line. On an error
    /// everything allocated is freed again.
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
````

with:

````rust
    /// (the PCI address, "00:14.0") starts every log line. On an error
    /// everything allocated is freed again, unless the controller did not
    /// start and then did not halt either: then its memory is kept.
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
````

Replace:

````rust
        if let Err(e) = run(&hal, &regs, &params, &mem, name) {
            mem.free(&hal);
            return Err(e);
````

with:

````rust
        if let Err(e) = run(&hal, &regs, &params, &mem, name) {
            // It may yet start (late): its memory is freed only once it
            // is halted.
            match halt(&hal, &regs, name) {
                Ok(()) => mem.free(&hal),
                Err(_) => mem.keep(&hal, name),
            }
            return Err(e);
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 330 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -m "usb: a controller that does not start is halted before its memory is freed"
````


### Task 8: The first port scan does not report empty ports

Plan 4's final-review minor M6. On the first scan every port is looked at, and an empty port whose connect-change bit was set (a device unplugged before the driver started, or firmware that left CSC behind) was logged as `port N: disconnected` and returned as a change, which the boot screen shows with `debug=usb` and `dmesg` always shows. On the first scan only connected ports count; afterwards a connect change counts as before.

**Files:**
- Modify: `crates/usb/src/xhci/port.rs`

**Interfaces:**
- Consumes: plan 4's `Xhci::port_changes`.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/xhci/port.rs`**

In `crates/usb/src/xhci/port.rs`, replace:

````rust
    #[test]
    fn plugs_and_unplugs_are_listed_once_each() {
````

with:

````rust
    #[test]
    fn an_empty_port_with_a_connect_change_is_not_in_the_first_port_changes() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        // In and out again before the first look: CSC set, no connection.
        hal.fake().plug(2, FakeUsbDevice::k120());
        hal.fake().unplug(2);
        assert_eq!(hal.fake().portsc(2) & (CCS | CSC), CSC);
        assert_eq!(xhci.port_changes(), vec![]);
        assert!(!hal.log_text().contains("port 2: disconnected"));
        assert_eq!(hal.fake().portsc(2) & CHANGE_BITS, 0, "CSC cleared");
        assert_eq!(xhci.port_changes(), vec![]);
        // Later a disconnect counts again.
        hal.fake().plug(2, FakeUsbDevice::k120());
        assert_eq!(xhci.port_changes(), vec![change(2, true, true)]);
        hal.fake().unplug(2);
        assert_eq!(xhci.port_changes(), vec![change(2, false, true)]);
        assert!(hal.log_text().contains("port 2: disconnected, PORTSC"));
    }

    #[test]
    fn a_connection_at_the_first_look_counts_without_a_connect_change() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.fake().plug(2, FakeUsbDevice::k120());
        xhci.clear_changes(2, xhci.portsc(2), CSC);
        assert_eq!(xhci.port_changes(), vec![change(2, true, false)]);
    }

    #[test]
    fn plugs_and_unplugs_are_listed_once_each() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 1 test fails: `xhci::port::tests::an_empty_port_with_a_connect_change_is_not_in_the_first_port_changes`.

- [ ] **Step 3: Change `crates/usb/src/xhci/port.rs`**

In `crates/usb/src/xhci/port.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// their change bits cleared. Right after `new` every port that is
    /// connected counts as changed, so the caller attaches boot-time devices
    /// and later hot-plugged ones the same way. Calls `poll` first.
    pub fn port_changes(&mut self) -> Vec<PortChange> {
````

with:

````rust
    /// their change bits cleared. Right after `new` every port that is
    /// connected counts as changed, and only those, so the caller attaches
    /// boot-time devices and later hot-plugged ones the same way. Calls
    /// `poll` first.
    pub fn port_changes(&mut self) -> Vec<PortChange> {
````

Replace:

````rust
            let reconnected = sc & CSC != 0;
            if self.ports[i].is_none() || !(reconnected || first && connected) {
                continue;
````

with:

````rust
            let reconnected = sc & CSC != 0;
            // At the first look only a connection counts: a CSC on an empty
            // port (a device gone before the driver started) is no news.
            let counts = if first { connected } else { reconnected };
            if self.ports[i].is_none() || !counts {
                continue;
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 332 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "usb: the first port scan does not report empty ports as disconnected"
````


### Task 9: A device is set up only after its connection was stable for 100 ms

Plan 4's final-review minor M7. `attach` slept the 100 ms debounce and then looked at the connection once, so a plug that bounced inside that window (a stick pushed in slowly) was reset while its contacts were still settling. USB 2.0 7.1.7.3 wants the connection stable for 100 ms; like Linux's `hub_port_debounce`, the port is now looked at every 25 ms, and a change of the connection or a connect change (which is cleared) starts the 100 ms again. After 2 s of bouncing the attach gives up: `Disconnected` if the port showed no connection at the last look, `Timeout` if it did; every outcome is logged, including `connection stable after N ms`. Clearing the connect change here must neither make the next scan report a reconnect that did not happen nor hide an unplug after the device is set up (decision 7).

**Files:**
- Modify: `crates/usb/src/xhci/device.rs`
- Modify: `crates/usb/src/xhci/port.rs`

**Interfaces:**
- Consumes: plan 4's `Xhci::attach`.
- Produces: `xhci::device`'s `DEBOUNCE_STEP = 25 ms` and `DEBOUNCE_LIMIT = 2 s`; `Xhci::clear_changes` is `pub(super)`.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, replace:

````rust
    }

    #[test]
    fn ports_without_a_device_or_a_protocol_are_refused() {
````

with:

````rust
    }

    /// When the port reset of `port` began (the fake's reset takes 10 ms).
    fn reset_began(hal: &FakeHal, port: u8) -> Duration {
        hal.fake().reset_done_at(port).expect("the port was reset") - Duration::from_millis(10)
    }

    #[test]
    fn a_steady_connection_is_reset_after_100_ms() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let start = hal.clock();
        xhci.attach(1).unwrap();
        let waited = reset_began(&hal, 1) - start;
        assert!(waited >= DEBOUNCE && waited < DEBOUNCE + Duration::from_millis(5));
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 1: connection stable after 100 ms")
        );
    }

    #[test]
    fn a_device_that_bounces_is_reset_100_ms_after_its_last_change() {
        // Out at 50 ms and in at 60 ms; and out and in again between two
        // looks, where only CSC shows it.
        for (out, back) in [(50, 60), (30, 40)] {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let start = hal.clock();
            let again = k120.clone();
            hal.fake()
                .after(Duration::from_millis(out), |x, _| x.unplug(1));
            hal.fake()
                .after(Duration::from_millis(back), move |x, _| x.plug(1, again));
            assert_eq!(xhci.attach(1).map(|d| d.port), Ok(1));
            let settled = Duration::from_millis(back) + DEBOUNCE;
            assert!(
                reset_began(&hal, 1) - start >= settled,
                "bounce at {out}-{back} ms: reset {:?} after the attach began",
                reset_began(&hal, 1) - start
            );
            assert!(hal.clock() - start >= settled);
            // The bounce is not taken for a replug later.
            assert_eq!(xhci.port_changes(), vec![], "bounce at {out}-{back} ms");
            // A real unplug still is.
            hal.fake().unplug(1);
            let changes = xhci.port_changes();
            assert_eq!(changes.len(), 1);
            assert_eq!(
                (changes[0].connected, changes[0].reconnected),
                (false, true)
            );
        }
    }

    #[test]
    fn a_replug_while_the_unplugs_csc_is_cleared_still_counts() {
        // Out at 40 ms; in again just as the debounce, looking at 50 ms,
        // clears the unplug's CSC, which clears the replug's too: only CCS
        // shows the device is back. One of these instants is the one.
        for us in 0..=10 {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let again = k120.clone();
            hal.fake()
                .after(Duration::from_millis(40), |x, _| x.unplug(1));
            let back = Duration::from_millis(50) + Duration::from_micros(us);
            hal.fake().after(back, move |x, _| x.plug(1, again));
            assert_eq!(xhci.attach(1).map(|d| d.port), Ok(1), "back at {back:?}");
        }
    }

    #[test]
    fn a_connection_that_keeps_bouncing_is_given_up_after_2_s_without_a_reset() {
        // Every 40 ms out for 20 ms, for 3 s; at 2 s it is in, or out.
        for (phase, outcome) in [(10, UsbError::Timeout), (30, UsbError::Disconnected)] {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let start = hal.clock();
            for n in 0..75 {
                let again = k120.clone();
                let out = Duration::from_millis(phase + 40 * n);
                hal.fake().after(out, |x, _| x.unplug(1));
                let back = out + Duration::from_millis(20);
                hal.fake().after(back, move |x, _| x.plug(1, again));
            }
            assert_eq!(xhci.attach(1).err(), Some(outcome), "phase {phase} ms");
            let took = hal.clock() - start;
            assert!(took >= Duration::from_secs(2) && took < Duration::from_millis(2050));
            assert_eq!(hal.fake().reset_done_at(1), None, "no reset");
            assert!(!hal.fake().slot_enabled(1));
            let log = hal.log_text();
            match outcome {
                UsbError::Timeout => {
                    assert!(log.contains("port 1: connection not stable after 2000 ms"))
                }
                _ => assert!(log.contains("port 1: not connected after debounce")),
            }
        }
    }

    #[test]
    fn ports_without_a_device_or_a_protocol_are_refused() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 3 tests fail, among them `xhci::device::tests::a_connection_that_keeps_bouncing_is_given_up_after_2_s_without_a_reset`, `xhci::device::tests::a_device_that_bounces_is_reset_100_ms_after_its_last_change`.

- [ ] **Step 3: Change `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use super::context::{CONTROL, EndpointContext, Input, Output, SlotContext, speed_id};
use super::regs::CCS;
use super::transfer::DATA_BUFFER_SIZE;
````

with:

````rust
use super::context::{CONTROL, EndpointContext, Input, Output, SlotContext, speed_id};
use super::regs::{CCS, CSC};
use super::transfer::DATA_BUFFER_SIZE;
````

Replace:

````rust
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long a device gets after SET_ADDRESS before its next request.
````

with:

````rust
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How often the debounce looks at the port (Linux: 25 ms).
const DEBOUNCE_STEP: Duration = Duration::from_millis(25);
/// How long a connection may bounce before the debounce gives up (Linux:
/// 2 s).
const DEBOUNCE_LIMIT: Duration = Duration::from_secs(2);
/// How long a device gets after SET_ADDRESS before its next request.
````

Replace:

````rust
    /// Sets up the device on `port` (spec §6.2 steps 1-5): debounce (the
    /// port must still be connected 100 ms after this call starts), port
    /// reset, Enable Slot, Address Device, device descriptor (fixing EP0's
    /// packet size), configuration descriptor. On an error the slot is
    /// disabled and everything allocated for it freed.
    pub fn attach(&mut self, port: u8) -> Result<Device, UsbError> {
````

with:

````rust
    /// Sets up the device on `port` (spec §6.2 steps 1-5): debounce (the
    /// connection must be stable for 100 ms), port reset, Enable Slot,
    /// Address Device, device descriptor (fixing EP0's packet size),
    /// configuration descriptor. On an error the slot is disabled and
    /// everything allocated for it freed.
    pub fn attach(&mut self, port: u8) -> Result<Device, UsbError> {
````

Replace:

````rust
        }
        let start = self.hal.now();
        if self.connected(port) {
            let waited = self.hal.now() - start;
            self.hal.sleep(DEBOUNCE.saturating_sub(waited));
        }
        if !self.connected(port) {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: not connected after debounce"
            );
            return Err(UsbError::Disconnected);
        }
        let speed = self.reset_port(port)?;
````

with:

````rust
        }
        self.debounce(port)?;
        let speed = self.reset_port(port)?;
````

Replace:

````rust
                Err(e)
            }
````

with:

````rust
                Err(e)
            }
        }
    }

    /// Waits until the connection on `port` has been stable for 100 ms
    /// (USB 2.0 7.1.7.3), as Linux's `hub_port_debounce` does: it looks
    /// every 25 ms, and a change of CCS, or a CSC, starts the 100 ms again.
    /// It clears CSC, so the next `port_changes` does not take the bounce
    /// for a replug; a change after the debounce sets it again. A
    /// connection stable for 100 ms is `Ok`, none for 100 ms is
    /// `Disconnected`. After 2 s of bouncing it gives up: `Disconnected` if
    /// the last look showed no connection (the next connect is a change
    /// again), `Timeout` if it did (the host tries again).
    fn debounce(&self, port: u8) -> Result<(), UsbError> {
        let start = self.hal.now();
        let (mut since, mut connected) = (start, None);
        let stable = loop {
            let sc = self.regs.portsc(&self.hal, port);
            let now = self.hal.now();
            let ccs = sc & CCS != 0;
            if sc & CSC != 0 || connected != Some(ccs) {
                self.clear_changes(port, sc, CSC);
                (since, connected) = (now, Some(ccs));
            } else if now - since >= DEBOUNCE {
                break true;
            }
            if now - start >= DEBOUNCE_LIMIT {
                break false;
            }
            self.hal.sleep(DEBOUNCE_STEP);
        };
        let waited = (self.hal.now() - start).as_millis();
        match (connected, stable) {
            (Some(true), true) => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: connection stable after {waited} ms"
                );
                Ok(())
            }
            (Some(true), false) => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: connection not stable after {waited} ms"
                );
                Err(UsbError::Timeout)
            }
            _ => {
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: not connected after debounce"
                );
                Err(UsbError::Disconnected)
            }
````

- [ ] **Step 4: Change `crates/usb/src/xhci/port.rs`**

In `crates/usb/src/xhci/port.rs`, replace:

````rust
    /// Clears the change bits in `bits` (RW1C) and nothing else.
    fn clear_changes(&self, port: u8, portsc: u32, bits: u32) {
        if portsc & bits != 0 {
````

with:

````rust
    /// Clears the change bits in `bits` (RW1C) and nothing else.
    pub(super) fn clear_changes(&self, port: u8, portsc: u32, bits: u32) {
        if portsc & bits != 0 {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 336 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "usb: a device is set up only after its connection was stable for 100 ms"
````


### Task 10: One rule for disk sizes

Plan 5's final-review minor: the rule for sizes on the boot lines (whole MiB under 1 GiB, GiB with one decimal from there, rounded down; spec §15 item 11, decision 3) was written twice, as `crates/usb/src/host/boot_line.rs` `Size` and `kernel/src/storage.rs` `size_text`, and the two disagreed at the top of the range (`Size` saturated in `u64`). `usb::host::Size(pub u64)` is now public, computes in `u128` so every size is right, and the kernel's `mount /` line uses it; `size_text` is gone.

**Files:**
- Modify: `crates/usb/src/host.rs`
- Modify: `crates/usb/src/host/boot_line.rs`
- Modify: `kernel/src/storage.rs`

**Interfaces:**
- Consumes: plan 5's `host::boot_line::Size`, `storage::root_text`.
- Produces: `usb::host::Size(pub u64)` (`Display`); `kernel::storage::size_text` is removed.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/host/boot_line.rs`**

In `crates/usb/src/host/boot_line.rs`, replace:

````rust
        assert_eq!(size(4096, 1 << 32), "16384.0 GiB");
    }
````

with:

````rust
        assert_eq!(size(4096, 1 << 32), "16384.0 GiB");
        // Byte by byte: around 1 GiB, the Kingston stick's root partition,
        // and the most a u64 holds, without saturating.
        assert_eq!(size(1, (1 << 30) - 1), "1023 MiB");
        assert_eq!(size(1, 1 << 30), "1.0 GiB");
        assert_eq!(size(1, 3 << 29), "1.5 GiB");
        assert_eq!(size(1, 15_501_000_000), "14.4 GiB");
        assert_eq!(size(1, u64::MAX), "17179869183.9 GiB");
    }
````

- [ ] **Step 2: Add the failing tests to `kernel/src/storage.rs`**

In `kernel/src/storage.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn sizes_read_as_mib_then_gib() {
        assert_eq!(size_text(256 << 20), "256 MiB");
        assert_eq!(size_text((1 << 30) - 1), "1023 MiB");
        assert_eq!(size_text(1 << 30), "1.0 GiB");
        // The Kingston stick's root partition, and the whole stick.
        assert_eq!(size_text(15_501_000_000), "14.4 GiB");
        assert_eq!(size_text(30_277_632 * 512), "14.4 GiB");
        assert_eq!(size_text(u64::MAX), "17179869183.9 GiB");
    }

    #[test]
    fn how_the_root_was_chosen_is_said() {
````

with:

````rust
    #[test]
    fn how_the_root_was_chosen_is_said() {
````

Replace:

````rust
            "ext2 on 00:02.0 port 2 partition 2, 190 MiB"
        );
    }
}
````

with:

````rust
            "ext2 on 00:02.0 port 2 partition 2, 190 MiB"
        );
        // The size is the boot line's (`usb::host::Size`, tested there).
        assert_eq!(
            root_text("00:14.0 port 15", 2, u64::MAX),
            "ext2 on 00:14.0 port 15 partition 2, 17179869183.9 GiB"
        );
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 1 test fails: `host::boot_line::tests::disk_sizes_are_whole_mib_then_gib_with_one_decimal_rounded_down`.

- [ ] **Step 4: Change `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, replace:

````rust

pub use boot_line::{Attached, DiskId, DiskInfo, Found};

````

with:

````rust

pub use boot_line::{Attached, DiskId, DiskInfo, Found, Size};

````

- [ ] **Step 5: Change `crates/usb/src/host/boot_line.rs`**

In `crates/usb/src/host/boot_line.rs`, replace:

````rust

/// A disk's size: whole MiB under 1 GiB, else GiB with one decimal, both
/// rounded down.
struct Size(u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const MIB: u64 = 1 << 20;
        const GIB: u64 = 1 << 30;
        if self.0 < GIB {
            write!(f, "{} MiB", self.0 / MIB)
        } else {
            let tenths = self.0.saturating_mul(10) / GIB;
            write!(f, "{}.{} GiB", tenths / 10, tenths % 10)
````

with:

````rust

/// A size in bytes as the boot line and the `mount /` line give it (spec
/// §15 item 11): whole MiB under 1 GiB, else GiB with one decimal, both
/// rounded down. Right for every `u64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size(pub u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const MIB: u64 = 1 << 20;
        const GIB: u128 = 1 << 30;
        if (self.0 as u128) < GIB {
            write!(f, "{} MiB", self.0 / MIB)
        } else {
            let tenths = self.0 as u128 * 10 / GIB;
            write!(f, "{}.{} GiB", tenths / 10, tenths % 10)
````

- [ ] **Step 6: Change `kernel/src/storage.rs`**

In `kernel/src/storage.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::{console, klogln, kprintln};
use alloc::boxed::Box;
````

with:

````rust
use crate::{console, klogln, kprintln};
use ::usb::host::Size;
use alloc::boxed::Box;
````

Replace:

````rust

/// `256 MiB`, or `14.4 GiB` from 1 GiB on (rounded down).
pub fn size_text(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    if bytes < GIB {
        format!("{} MiB", bytes >> 20)
    } else {
        let tenths = (bytes as u128 * 10 / GIB as u128) as u64;
        format!("{}.{} GiB", tenths / 10, tenths % 10)
    }
}

/// What the `mount /` status line says about the root: `ext2 on 00:02.0
/// port 2 partition 2, 190 MiB`.
pub fn root_text(disk: &str, number: u32, bytes: u64) -> String {
    format!("ext2 on {disk} partition {number}, {}", size_text(bytes))
}
````

with:

````rust

/// What the `mount /` status line says about the root: `ext2 on 00:02.0
/// port 2 partition 2, 190 MiB`, the size as on the disk's boot line.
pub fn root_text(disk: &str, number: u32, bytes: u64) -> String {
    format!("ext2 on {disk} partition {number}, {}", Size(bytes))
}
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 336 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 183 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates kernel
git commit -m "usb, kernel: one rule for disk sizes"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 18 scenario(s) passed`.

````bash
git push -u origin plan6/usb
gh pr create --base main --head plan6/usb --title "Plan 6: USB attach" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 7–10: a controller whose start times out is halted before its memory is freed, and its memory is kept if it does not halt (M5); the first port scan reports only connected ports (M6); a device is set up after its connection was stable for 100 ms, as USB 2.0 7.1.7.3 and Linux do (M7); `usb::host::Size` is the one rule for disk sizes, exact for every size.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests against the fake xHCI controller

## Hardware

- [x] Not needed now: the attach path runs on the NUC in the final check (PR 7)
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/usb --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-usb
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Shell fixes (Tasks 11–14)

Plan 3's shell minors, which the real `/` of plan 5 made reachable.

Branch `plan6/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/usb` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-shell plan6/usb`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/usb>` and re-run `cargo xtask ci` before pushing.

### Task 11: `rm -r` removes a tree that holds the current directory

Plan 3's final-review minor. `rm -r ../../p` from `/root/p/sub` removed `sub` first, and from then on every relative path was `ENOENT` (spec §15 item 9: a removed current directory makes relative paths fail until the next `cd`), so the rest of the tree stayed and `rm` failed. GNU removes it all. `remove_tree` now walks from `/`: the operand is joined to the current directory with `.` and `..` taken by name, as the mount table walks them; the messages still name the paths as given. The test support gains `SpyState::fail_unlink` to check those messages.

**Files:**
- Modify: `crates/shell/src/commands/change.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: plan 3's `commands::change::remove_tree`, `Vfs::cwd`.
- Produces: test support `SpyState::fail_unlink`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use alloc::string::String;

````

with:

````rust
    use alloc::string::String;
    use vfs::Errno;

````

Replace:

````rust
        assert!(!h.exists("/tmp/t"));
    }
````

with:

````rust
        assert!(!h.exists("/tmp/t"));
    }

    #[test]
    fn rm_r_removes_a_tree_holding_the_current_directory() {
        // GNU removes it all; the current directory goes on the way, and a
        // relative path must not depend on it afterwards.
        let mut h = Harness::new();
        h.run("mkdir -p /tmp/p/sub/deep");
        h.put("/tmp/p/a", b"");
        h.put("/tmp/p/z", b"");
        h.run("cd /tmp/p/sub");
        assert_eq!(h.run("rm -r ../../p"), (0, "".into()));
        assert!(!h.exists("/tmp/p"));
        // Messages still name the paths as given.
        h.run("mkdir -p /tmp/q/sub");
        h.put("/tmp/q/sub/f", b"");
        h.run("cd /tmp/q");
        h.spy.fail_unlink.set(Some(Errno::EIO));
        assert_eq!(
            h.run("rm -r sub"),
            (
                1,
                "rm: cannot remove 'sub/f': Input/output error\n\
                 rm: cannot remove 'sub': Directory not empty\n"
                    .into()
            )
        );
    }
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub zero_writes: Cell<bool>,
}
````

with:

````rust
    pub zero_writes: Cell<bool>,
    pub fail_unlink: Cell<Option<Errno>>,
}
````

Replace:

````rust
    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.fs.unlink(dir, name)
    }
````

with:

````rust
    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        match self.state.fail_unlink.get() {
            Some(e) => Err(e),
            None => self.fs.unlink(dir, name),
        }
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::change::tests::rm_r_removes_a_tree_holding_the_current_directory`.

- [ ] **Step 4: Change `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// Removes a directory and everything below it, reporting each failure.
/// Depth-first with an explicit stack: a deep tree must not use up the
/// kernel's small stack. Returns whether everything went.
fn remove_tree(ctx: &mut Ctx<'_>, top: &[u8]) -> bool {
    let mut ok = true;
    let above = ctx.vfs.lookup(&path::join(top, b".."));
    let mut stack = match above.and_then(|parent| children(ctx, top, parent)) {
        Ok((node, names)) => vec![(top.to_vec(), node, names)],
        Err(e) => {
            ctx.fail(
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(top))),
            );
            return false;
        }
    };
    while let Some((dir, node, pending)) = stack.last_mut() {
        if ctx.interrupted() {
            return false;
        }
        let Some(name) = pending.pop() else {
            let (dir, _, _) = stack.pop().expect("not empty");
            if let Err(e) = ctx.vfs.rmdir(&dir) {
                ctx.fail(
                    "rm",
                    format_args!("cannot remove {}: {e}", quote(&path::display(&dir))),
                );
````

with:

````rust

/// `p` as a path from `/`, with `.` and `..` taken by name as the mount
/// table walks them. The tree `rm -r` removes may hold the current
/// directory, and once that is gone a relative path leads nowhere.
fn from_root(cwd: &[u8], p: &[u8]) -> Result<Vec<u8>, Errno> {
    let p = path::parse(p)?;
    let cwd = path::parse(cwd)?;
    let start = if p.absolute {
        &[][..]
    } else {
        &cwd.components[..]
    };
    let mut names: Vec<&[u8]> = Vec::new();
    for c in start.iter().chain(&p.components) {
        match c {
            path::Component::Current => {}
            path::Component::Parent => {
                names.pop();
            }
            path::Component::Name(n) => names.push(n),
        }
    }
    let mut out = b"/".to_vec();
    out.extend_from_slice(&names.join(&b'/'));
    Ok(out)
}

/// Removes a directory and everything below it, reporting each failure
/// with the path as given. Depth-first with an explicit stack: a deep tree
/// must not use up the kernel's small stack. Returns whether everything
/// went.
fn remove_tree(ctx: &mut Ctx<'_>, given: &[u8]) -> bool {
    let mut ok = true;
    let top = match from_root(&ctx.vfs.cwd(), given) {
        Ok(top) => top,
        Err(e) => {
            ctx.fail(
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(given))),
            );
            return false;
        }
    };
    let above = ctx.vfs.lookup(&path::join(&top, b".."));
    let mut stack = match above.and_then(|parent| children(ctx, &top, parent)) {
        Ok((node, names)) => vec![(top, given.to_vec(), node, names)],
        Err(e) => {
            ctx.fail(
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(given))),
            );
            return false;
        }
    };
    while let Some((dir, shown, node, pending)) = stack.last_mut() {
        if ctx.interrupted() {
            return false;
        }
        let Some(name) = pending.pop() else {
            let (dir, shown, _, _) = stack.pop().expect("not empty");
            if let Err(e) = ctx.vfs.rmdir(&dir) {
                ctx.fail(
                    "rm",
                    format_args!("cannot remove {}: {e}", quote(&path::display(&shown))),
                );
````

Replace:

````rust
        let child = path::join(dir, &name);
        let is_dir = ctx
````

with:

````rust
        let child = path::join(dir, &name);
        let child_shown = path::join(shown, &name);
        let is_dir = ctx
````

Replace:

````rust
            Ok(true) => children(ctx, &child, parent)
                .map(|(node, names)| stack.push((child.clone(), node, names))),
            Ok(false) => ctx.vfs.unlink(&child),
````

with:

````rust
            Ok(true) => children(ctx, &child, parent)
                .map(|(node, names)| stack.push((child.clone(), child_shown.clone(), node, names))),
            Ok(false) => ctx.vfs.unlink(&child),
````

Replace:

````rust
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(&child))),
            );
````

with:

````rust
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(&child_shown))),
            );
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 98 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "shell: rm -r removes a tree holding the current directory, as GNU does"
````


### Task 12: `cat` stops at the first write error

Plan 3's final-review minor. When `cat`'s output file filled the disk, `Ctx` kept the first write error and dropped later output, but `cat` went on reading its inputs to the end, over USB for nothing, and reported errors for later operands. GNU stops at the first write error. `Ctx::out_failed` says so, and `cat` stops its current file and the operands after it; the shell still prints `cat: write error: No space left on device` at the end. The test support gains `SpyState::reads`, counting `read_at` calls.

**Files:**
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: plan 3's `Ctx`, `commands::text::cat`.
- Produces: `Ctx::out_failed(&self) -> bool`; test support `SpyState::reads`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
    #[test]
    fn cat_refuses_to_append_a_file_to_itself() {
````

with:

````rust
    #[test]
    fn cat_stops_at_the_first_write_error() {
        // GNU stops once its output cannot be written: the other inputs
        // are not read, and their errors are not reported.
        let mut h = Harness::with_capacity(5 * 4096);
        h.put("/tmp/big", &[b'x'; 8192]);
        assert_eq!(
            h.run("cat /tmp/big /tmp/nope /tmp/big > /tmp/out"),
            (1, "cat: write error: No space left on device\n".into())
        );
        assert_eq!(h.get("/tmp/out").len(), 4096);
        // Nor is the rest of a big input read over USB for nothing.
        let mut h = Harness::with_capacity(4 * 4096 + 4 * super::CHUNK as u64);
        h.put("/tmp/big", &vec![b'x'; 4 * super::CHUNK]);
        h.spy.reads.set(0);
        assert_eq!(h.run("cat /tmp/big > /tmp/out").0, 1);
        assert_eq!(h.spy.reads.get(), 1);
    }

    #[test]
    fn cat_refuses_to_append_a_file_to_itself() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub fail_unlink: Cell<Option<Errno>>,
}
````

with:

````rust
    pub fail_unlink: Cell<Option<Errno>>,
    /// `read_at` calls.
    pub reads: Cell<u32>,
}
````

Replace:

````rust
    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.fs.read_at(ino, offset, buf)
````

with:

````rust
    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.state.reads.set(self.state.reads.get() + 1);
        self.fs.read_at(ino, offset, buf)
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::text::tests::cat_stops_at_the_first_write_error`.

- [ ] **Step 4: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    for op in &opts.operands {
        let name = quote_if_needed(op);
````

with:

````rust
    for op in &opts.operands {
        // The shell reports the write error when the command ends.
        if ctx.out_failed() {
            break;
        }
        let name = quote_if_needed(op);
````

Replace:

````rust
            ctx.out(bytes);
            true
        }) {
````

with:

````rust
            ctx.out(bytes);
            !ctx.out_failed()
        }) {
````

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust
        matches!(self.out, Output::Console)
    }
````

with:

````rust
        matches!(self.out, Output::Console)
    }

    /// Whether writing standard output to its file has failed; later output
    /// is dropped, so a command may as well stop.
    pub fn out_failed(&self) -> bool {
        matches!(self.out, Output::File { error: Some(_), .. })
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 99 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -m "shell: cat stops at the first write error, as GNU does"
````


### Task 13: `cp` refuses a source it cannot read before emptying the destination

Plan 3's final-review minor. `cp` refused only directories as sources; for a symbolic link (which milestone 1 does not follow, spec §8.1) it emptied the destination and only then failed to read the source, so `cp link f` destroyed `f`. A source that is not a regular file is now refused first: `cp: cannot open 'link' for reading: Invalid argument`.

**Files:**
- Modify: `crates/shell/src/commands/change.rs`

**Interfaces:**
- Consumes: plan 3's `commands::change::copy`.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, replace:

````rust
    #[test]
    fn rm_f_ignores_missing_files() {
````

with:

````rust
    #[test]
    fn cp_of_a_symlink_leaves_the_destination_alone() {
        // Links are not followed in milestone 1 (spec §8.1), so the source
        // cannot be read; the destination must not be emptied first.
        let mut fs = memfs();
        let root = vfs::FileSystem::root(&fs);
        fs.symlink(root, b"link", b"/etc/motd").unwrap();
        let mut h = Harness::on(fs);
        h.put("/tmp/b", b"keep");
        assert_eq!(
            h.run("cp /link /tmp/b"),
            (
                1,
                "cp: cannot open '/link' for reading: Invalid argument\n".into()
            )
        );
        assert_eq!(h.get("/tmp/b"), b"keep");
        assert!(!h.exists("/tmp/link"));
        h.run("cp /link /tmp");
        assert!(!h.exists("/tmp/link"));
    }

    #[test]
    fn rm_f_ignores_missing_files() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::change::tests::cp_of_a_symlink_leaves_the_destination_alone`.

- [ ] **Step 3: Change `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, replace:

````rust
                format_args!("-r not specified; omitting directory {}", quote(src)),
            );
````

with:

````rust
                format_args!("-r not specified; omitting directory {}", quote(src)),
            );
            return Err(());
        }
        // A symbolic link (not followed in milestone 1) or a special
        // file cannot be read: say so before the destination is touched.
        Ok((_, st)) if st.kind != FileType::Regular => {
            ctx.fail(
                "cp",
                format_args!("cannot open {} for reading: {}", quote(src), Errno::EINVAL),
            );
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 100 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: cp refuses a source it cannot read before emptying the destination"
````


### Task 14: `$` and `` ` `` inside double quotes are refused

Plan 3's final-review minor, first half. The parser refused an unquoted `$` or `` ` ``, but passed them on as text inside double quotes, where bash expands them: `echo "$HOME"` printed `$HOME`, which bash never prints. Inside double quotes they are now refused as outside them, and `\$` and `` \` `` stand for the character, as in bash (decision 3). The other half, `#`, becomes a comment in Task 18.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: plan 3's `parser::parse`.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn double_quotes_take_two_escapes() {
        assert_eq!(
            words(r#"echo "say \"hi\"" "a\\b" "c\d" "x'y""#),
            ["echo", r#"say "hi""#, r"a\b", r"c\d", "x'y"]
        );
        assert_eq!(words(r#"echo "$ * ?""#), ["echo", "$ * ?"]);
    }
````

with:

````rust
    #[test]
    fn double_quotes_take_four_escapes() {
        assert_eq!(
            words(r#"echo "say \"hi\"" "a\\b" "c\d" "x'y" "* ?""#),
            ["echo", r#"say "hi""#, r"a\b", r"c\d", "x'y", "* ?"]
        );
        assert_eq!(
            words(r#"echo "\$HOME costs \`1\`""#),
            ["echo", "$HOME costs `1`"]
        );
    }

    #[test]
    fn dollar_and_backquote_inside_double_quotes_are_unsupported() {
        // Bash expands them there too; passing them on as text would
        // print something bash never prints.
        assert_eq!(
            parse(r#"echo "$HOME""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            parse(r#"echo "a $ b""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            parse(r#"echo "`date`""#),
            Err(ParseError::Unsupported("`".into()))
        );
        assert_eq!(words(r"echo '$HOME `x`'"), ["echo", "$HOME `x`"]);
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::dollar_and_backquote_inside_double_quotes_are_unsupported`, `parser::tests::double_quotes_take_four_escapes`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! Words are split on spaces and tabs. `'…'` is literal; `"…"` is literal
//! except that `\"` and `\\` stand for `"` and `\`; outside quotes `\`
//! makes the next character literal. `> file` and `>> file` redirect
//! standard output (at most one per command). An unquoted `~` alone or
//! before `/` at the start of a word means `/root`, as in Linux. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
````

with:

````rust
//! Words are split on spaces and tabs. `'…'` is literal; `"…"` is literal
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, and a bare `$` or `` ` `` in it is refused as outside quotes
//! (bash would expand it); outside quotes `\` makes the next character
//! literal. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
````

Replace:

````rust
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\')) => {
                            word.text.push(chars.next().expect("peeked"));
                        }
                        Some(c) => word.text.push(c),
````

with:

````rust
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.text.push(chars.next().expect("peeked"));
                        }
                        Some(c @ ('$' | '`')) => return Err(ParseError::Unsupported(c.into())),
                        Some(c) => word.text.push(c),
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 101 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m 'shell: $ and ` inside double quotes are refused, as outside them'
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 18 scenario(s) passed`.

````bash
git push -u origin plan6/shell
gh pr create --base main --head plan6/shell --title "Plan 6: Shell fixes" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 11–14: `rm -r` removes a tree that holds the current directory; `cat` stops at the first write error; `cp` refuses a source it cannot read before emptying the destination; `$` and `` ` `` are refused inside double quotes, as outside them.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: The e2e runner (Tasks 15–17)

A QMP socket only its owner can reach, a `reboot` step that sees all the output, and a scenario that pulls the stick out.

Branch `plan6/e2e`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-e2e`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/e2e /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-e2e origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-e2e
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/e2e /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-e2e plan6/shell`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/shell>` and re-run `cargo xtask ci` before pushing.

### Task 15: QMP listens in the private run directory

Plan 1's final-review finding #9. The e2e runner's QMP socket was a predictable name in Linux's abstract socket namespace, which every user of the machine can reach, and QMP can make QEMU run commands. It is now a socket file, `qmp.sock`, in the scenario's run directory, which `Qemu::prepare` makes private (mode 0700). Unix socket addresses hold 108 bytes, less than a deep checkout's paths (the reason the abstract namespace was used), so QEMU runs in the run directory and binds the bare name, and xtask connects through the open directory's `/proc/self/fd` entry (decision 6).

**Files:**
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/qemu.rs`
- Modify: `xtask/src/qmp.rs`

**Interfaces:**
- Consumes: plan 1's `qemu::Qemu`, `qmp::Qmp`.
- Produces: `Qemu::qmp: Option<PathBuf>` (replaces `qmp_name`; `qemu::qmp_name` is removed); `Qmp::connect(socket: &Path, timeout)`; `qmp::reachable(socket: &Path) -> Result<(File, PathBuf)>`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
            &mut child,
            "relay-qmp-selftest-nobody-listens",
            Duration::from_secs(10),
````

with:

````rust
            &mut child,
            &dir.join("nobody-listens.sock"),
            Duration::from_secs(10),
````

- [ ] **Step 2: Add the failing tests to `xtask/src/qemu.rs`**

In `xtask/src/qemu.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    /// Unix socket paths are limited to 108 bytes, which a deep checkout
    /// exceeded; QMP therefore listens on an abstract socket (a name, no path).
    #[test]
    fn qmp_listens_on_an_abstract_socket() {
        let deep = Path::new("/very/deep").join("x".repeat(200));
````

with:

````rust

    /// QMP can make QEMU run any command, so its socket is a file in the
    /// run directory, which only its owner can enter, not a name in the
    /// abstract namespace, which every user can reach. Unix socket paths
    /// hold 108 bytes, less than a deep checkout's: QEMU runs in the run
    /// directory and binds the bare name.
    #[test]
    fn qmp_listens_in_the_private_run_directory() {
        let deep = Path::new("/very/deep").join("x".repeat(200));
````

Replace:

````rust
            headless: true,
            qmp_name: Some("relay-qmp-42-boot".into()),
        };
        let args: Vec<String> = q
            .command()
            .get_args()
````

with:

````rust
            headless: true,
            qmp: Some(deep.join("qmp.sock")),
        };
        let c = q.command();
        let args: Vec<String> = c
            .get_args()
````

Replace:

````rust
            args[at + 1],
            "socket,id=qmp,path=relay-qmp-42-boot,server=on,wait=off,abstract=on"
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["-mon", "chardev=qmp,mode=control"])
        );
        assert!(!args.iter().any(|a| a == "-qmp"));
    }

    #[test]
    fn qmp_names_are_short_and_unique_per_scenario() {
        let a = qmp_name("boot");
        assert!(a.starts_with("relay-qmp-") && a.ends_with("-boot"));
        assert!(a.contains(&std::process::id().to_string()));
        assert_ne!(a, qmp_name("panic_ud"));
        assert!(qmp_name(&"s".repeat(300)).len() <= 100);
    }
````

with:

````rust
            args[at + 1],
            "socket,id=qmp,path=qmp.sock,server=on,wait=off"
        );
        assert_eq!(c.get_current_dir(), Some(deep.as_path()));
        assert!(
            args.windows(2)
                .any(|w| w == ["-mon", "chardev=qmp,mode=control"])
        );
        assert!(!args.iter().any(|a| a == "-qmp" || a.contains("abstract")));
    }

    #[test]
    fn the_run_directory_is_private() {
        let dir = crate::util::out_dir().join("qemu-selftest");
        let image = dir.join("image");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&image, b"").unwrap();
        let run = dir.join("run");
        let _ = fs::remove_dir_all(&run);
        Qemu::prepare(&image, &run).unwrap();
        let mode = fs::metadata(&run).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
````

- [ ] **Step 3: Add the failing tests to `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
    /// answers one command.
    #[test]
    fn talks_to_qemu_over_an_abstract_socket() {
        let name = format!("relay-qmp-test-{}", std::process::id());
        let addr = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let listener = UnixListener::bind_addr(&addr).unwrap();
        let server = std::thread::spawn(move || {
````

with:

````rust
    use super::*;
    use crate::util::out_dir;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
    /// answers one command. Its socket is deeper than a Unix socket address
    /// can name.
    #[test]
    fn talks_to_qemu_over_a_socket_in_a_deep_directory() {
        let dir = out_dir().join("qmp-selftest").join("d".repeat(150));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("qmp.sock");
        let _ = std::fs::remove_file(&socket);
        assert!(socket.as_os_str().len() > 108);
        let (_dir, path) = reachable(&socket).unwrap();
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
````

Replace:

````rust
        });
        let mut q = Qmp::connect(&name, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
````

with:

````rust
        });
        let mut q = Qmp::connect(&socket, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find function `reachable` in this scope ``; `mismatched types`.

- [ ] **Step 5: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::keys;
use crate::qemu::{self, Qemu};
use crate::qmp::Qmp;
````

with:

````rust
use crate::keys;
use crate::qemu::Qemu;
use crate::qmp::Qmp;
````

Replace:

````rust
    q.headless = true;
    q.qmp_name = Some(qemu::qmp_name(&scenario.name));
    let log = fs::File::create(run_dir.join("serial.log"))?;
````

with:

````rust
    q.headless = true;
    q.qmp = Some(run_dir.join("qmp.sock"));
    let log = fs::File::create(run_dir.join("serial.log"))?;
````

Replace:

````rust
fn launch(q: Qemu, root: Partition, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let qmp_name = q.qmp_name.clone().context("QMP socket name")?;
    let mut child = q
````

with:

````rust
fn launch(q: Qemu, root: Partition, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let socket = q.qmp.clone().context("QMP socket")?;
    // A socket left by an earlier run would refuse QEMU's bind.
    let _ = fs::remove_file(&socket);
    let mut child = q
````

Replace:

````rust
        &mut child,
        &qmp_name,
        Duration::from_secs(10),
````

with:

````rust
        &mut child,
        &socket,
        Duration::from_secs(10),
````

Replace:

````rust
        headless: true,
        qmp_name: r.qemu.qmp_name.clone(),
    };
````

with:

````rust
        headless: true,
        qmp: r.qemu.qmp.clone(),
    };
````

Replace:

````rust
/// instead of a bare "Connection refused" after the full timeout.
fn wait_for_qmp(child: &mut Child, name: &str, timeout: Duration, stderr: &Path) -> Result<Qmp> {
    let deadline = Instant::now() + timeout;
````

with:

````rust
/// instead of a bare "Connection refused" after the full timeout.
fn wait_for_qmp(child: &mut Child, socket: &Path, timeout: Duration, stderr: &Path) -> Result<Qmp> {
    let deadline = Instant::now() + timeout;
````

Replace:

````rust
        }
        match Qmp::connect(name, Duration::from_millis(200)) {
            Ok(q) => return Ok(q),
````

with:

````rust
        }
        match Qmp::connect(socket, Duration::from_millis(200)) {
            Ok(q) => return Ok(q),
````

- [ ] **Step 6: Change `xtask/src/qemu.rs`**

In `xtask/src/qemu.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use std::fs;
use std::path::{Path, PathBuf};
````

with:

````rust
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
````

Replace:

````rust
    pub headless: bool,
    /// Abstract-namespace socket name for QMP (see `qmp_name`).
    pub qmp_name: Option<String>,
}

impl Qemu {
    /// Copies the image and a fresh OVMF variable store into `run_dir`.
    pub fn prepare(image: &Path, run_dir: &Path) -> Result<Qemu> {
        fs::create_dir_all(run_dir)?;
        let disk = run_dir.join("disk.img");
````

with:

````rust
    pub headless: bool,
    /// Where QEMU listens for QMP: a socket in the run directory, which
    /// only its owner can enter (QMP can make QEMU run commands).
    pub qmp: Option<PathBuf>,
}

impl Qemu {
    /// Copies the image and a fresh OVMF variable store into `run_dir`,
    /// which becomes private to this user.
    pub fn prepare(image: &Path, run_dir: &Path) -> Result<Qemu> {
        fs::create_dir_all(run_dir)?;
        fs::set_permissions(run_dir, fs::Permissions::from_mode(0o700))?;
        let disk = run_dir.join("disk.img");
````

Replace:

````rust
            headless: false,
            qmp_name: None,
        })
````

with:

````rust
            headless: false,
            qmp: None,
        })
````

Replace:

````rust
        }
        if let Some(name) = &self.qmp_name {
            c.arg("-chardev")
                .arg(format!(
                    "socket,id=qmp,path={name},server=on,wait=off,abstract=on"
                ))
                .args(["-mon", "chardev=qmp,mode=control"]);
        }
        c
    }
}

/// A QMP socket name for one QEMU run. It lives in Linux's abstract socket
/// namespace, so it has no filesystem path and no dependence on how deep the
/// checkout is (Unix socket paths are limited to 108 bytes).
pub fn qmp_name(tag: &str) -> String {
    let mut name = format!("relay-qmp-{}-{tag}", std::process::id());
    name.truncate(100);
    name
}
````

with:

````rust
        }
        if let Some(socket) = &self.qmp {
            // Unix socket addresses hold 108 bytes, less than a deep
            // checkout's path: QEMU binds the name in its own directory.
            let name = socket.file_name().unwrap_or_default().to_string_lossy();
            if let Some(dir) = socket.parent() {
                c.current_dir(dir);
            }
            c.arg("-chardev")
                .arg(format!("socket,id=qmp,path={name},server=on,wait=off"))
                .args(["-mon", "chardev=qmp,mode=control"]);
        }
        c
    }
}
````

- [ ] **Step 7: Change `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::{Duration, Instant};
````

with:

````rust
use serde_json::{Value, json};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
````

Replace:

````rust
impl Qmp {
    /// Connects to the abstract socket `name` (retrying while QEMU starts)
    /// and negotiates capabilities.
    pub fn connect(name: &str, timeout: Duration) -> Result<Qmp> {
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect_addr(&addr) {
                Ok(s) => break s,
````

with:

````rust
impl Qmp {
    /// Connects to the socket at `socket` (retrying while QEMU starts) and
    /// negotiates capabilities.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Qmp> {
        let (_dir, path) = reachable(socket)?;
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect(&path) {
                Ok(s) => break s,
````

Replace:

````rust

#[cfg(test)]
````

with:

````rust

/// A path to `socket` short enough for a Unix socket address (108 bytes)
/// however deep the checkout is: through the `/proc/self/fd` entry of its
/// open directory. The path works while the returned `File` is open.
pub fn reachable(socket: &Path) -> Result<(File, PathBuf)> {
    let dir = socket.parent().context("socket path without a directory")?;
    let name = socket.file_name().context("socket path without a name")?;
    let dir = File::open(dir).with_context(|| format!("opening {}", dir.display()))?;
    let path = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name);
    Ok((dir, path))
}

#[cfg(test)]
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 47 tests.

- [ ] **Step 9: Run the `keyboard` scenario**

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add xtask
git commit -m "xtask: QMP listens on a socket in the private run directory, not in the abstract namespace"
````


### Task 16: The reboot step sees everything the machine printed

Plan 5's final-review minor. After `reboot` the runner matched its pattern against the serial output as soon as QEMU had exited, while the thread copying QEMU's output could still hold the last bytes, and it opened a second handle on `serial.log` while the old thread might still write through its own. The reader now hands its log file back when QEMU's output ends; `exit_with` joins it before anything looks at the output, and the next boot continues the same file (decision 6).

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: plan 5's e2e `launch`, `exit_with`, `reboot`.
- Produces: `e2e::read_serial(stdout, serial, log) -> JoinHandle<File>`; `exit_with` returns the log file.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    }

    #[test]
    fn default_cmdline_is_test_mode() {
````

with:

````rust
    }

    /// A reboot matches what the machine printed before it went down, then
    /// continues the log: both need every byte QEMU wrote before it exited.
    #[test]
    fn the_serial_reader_hands_over_everything_printed() {
        let dir = out_dir().join("e2e-selftest");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reader.log");
        let mut child = std::process::Command::new("sh")
            .args(["-c", "head -c 1000000 /dev/zero | tr '\\0' x; printf END"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let serial = Arc::new(Mutex::new(Vec::new()));
        let reader = read_serial(
            child.stdout.take().unwrap(),
            serial.clone(),
            fs::File::create(&path).unwrap(),
        );
        child.wait().unwrap();
        let mut log = reader.join().unwrap();
        assert_eq!(serial.lock().unwrap().len(), 1_000_003);
        assert!(serial.lock().unwrap().ends_with(b"xEND"));
        log.write_all(b"MARK").unwrap();
        drop(log);
        let text = fs::read(&path).unwrap();
        assert_eq!(text.len(), 1_000_007);
        assert!(text.ends_with(b"xENDMARK"));
    }

    #[test]
    fn default_cmdline_is_test_mode() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find function `read_serial` in this scope ``.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
````

with:

````rust
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
````

Replace:

````rust
    serial: Arc<Mutex<Vec<u8>>>,
    qmp: Qmp,
````

with:

````rust
    serial: Arc<Mutex<Vec<u8>>>,
    /// Copies QEMU's serial output into `serial` and the log file until
    /// QEMU exits, then returns the log file.
    reader: Option<JoinHandle<fs::File>>,
    qmp: Qmp,
````

Replace:

````rust
        let _ = self.child.wait();
    }
````

with:

````rust
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
````

Replace:

````rust
/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, root: Partition, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let socket = q.qmp.clone().context("QMP socket")?;
````

with:

````rust
/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, root: Partition, run_dir: &Path, log: fs::File) -> Result<Running> {
    let socket = q.qmp.clone().context("QMP socket")?;
````

Replace:

````rust
    let stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let serial = Arc::new(Mutex::new(Vec::new()));
    let sink = serial.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
            let _ = log.write_all(&buf[..n]);
        }
    });
    let qmp = wait_for_qmp(
````

with:

````rust
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let serial = Arc::new(Mutex::new(Vec::new()));
    let reader = read_serial(stdout, serial.clone(), log);
    let qmp = wait_for_qmp(
````

Replace:

````rust
        serial,
        qmp,
````

with:

````rust
        serial,
        reader: Some(reader),
        qmp,
````

Replace:

````rust

/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<()> {
    r.stdin.write_all(command.as_bytes())?;
````

with:

````rust

/// Copies `stdout` into `serial` and `log` until it ends (QEMU exited),
/// then hands the log file back.
fn read_serial(
    mut stdout: impl Read + Send + 'static,
    serial: Arc<Mutex<Vec<u8>>>,
    mut log: fs::File,
) -> JoinHandle<fs::File> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            serial.lock().unwrap().extend_from_slice(&buf[..n]);
            let _ = log.write_all(&buf[..n]);
        }
        log
    })
}

/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean. Returns the serial log once everything QEMU printed is in it.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    r.stdin.write_all(command.as_bytes())?;
````

Replace:

````rust
        if let Some(s) = r.child.try_wait()? {
            if s.code() != Some(status) {
````

with:

````rust
        if let Some(s) = r.child.try_wait()? {
            // The pipe ends with QEMU, so this waits only for the last
            // bytes to be copied.
            let log = r
                .reader
                .take()
                .context("serial reader already joined")?
                .join()
                .map_err(|_| anyhow::anyhow!("the serial reader panicked"))?;
            if s.code() != Some(status) {
````

Replace:

````rust
            }
            return Ok(());
        }
````

with:

````rust
            }
            return Ok(log);
        }
````

Replace:

````rust
fn reboot(r: &mut Running, last: Option<&str>, timeout: Duration) -> Result<()> {
    exit_with(r, "reboot", EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
````

with:

````rust
fn reboot(r: &mut Running, last: Option<&str>, timeout: Duration) -> Result<()> {
    let mut log = exit_with(r, "reboot", EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
````

Replace:

````rust
    }
    let mut log = fs::OpenOptions::new()
        .append(true)
        .open(r.run_dir.join("serial.log"))?;
    log.write_all(b"\n--- e2e: reboot ---\n")?;
````

with:

````rust
    }
    log.write_all(b"\n--- e2e: reboot ---\n")?;
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 48 tests.

- [ ] **Step 5: Run the `power`, `persist` scenarios**

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario persist`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add xtask
git commit -m "xtask: the reboot step joins the serial reader, so it sees everything the machine printed"
````


### Task 17: The unplug step and scenario

Plan 5's final-review minor: no scenario pulled the stick out, so decision 3 of spec §15 item 11 (requests fail at once with `EIO`, `/` is not mounted again) was checked only on the host. The stick's QEMU device gets the id `stick-usb`; the step `unplug` removes it with QMP `device_del` and waits for QEMU's `DEVICE_DELETED` event (`Qmp` keeps the events it sees while waiting for a command's answer). The scenario `unplug` writes a file, pulls the stick, waits for the driver to release the slot, and then expects `touch` to fail with `Input/output error` within 3 s (three tries of 5 s each would take 15), the shell to keep working, and `poweroff` to refuse without a clean shutdown (decision 6).

**Files:**
- Create: `tests/e2e/unplug.txt`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/qemu.rs`
- Modify: `xtask/src/qmp.rs`

**Interfaces:**
- Consumes: Task 15; plan 5's runner.
- Produces: `qemu::STICK_DEVICE = "stick-usb"`; `Qmp::wait_event(&mut self, name: &str, timeout: Duration) -> Result<Value>`; e2e step `unplug` (`Step::Unplug`); scenario `tests/e2e/unplug.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/unplug.txt`**

Create `tests/e2e/unplug.txt`:

````text
# The stick pulled out while / is mounted (spec §15 item 11, decision 3):
# every request fails at once with EIO instead of waiting for a device that
# is gone, the shell keeps running, and poweroff will not go on without a
# clean shutdown.
timeout 30
expect root@relay:~# $
send echo before > /root/f
send cat /root/f
expect \nbefore\n
unplug
expect port \d: slot \d+ released
# Three tries of 5 s each would be 15 s per request.
timeout 3
send touch /root/g
expect \nrelay-sh: sync failed: Input/output error\n
send echo still here
expect \nstill here\n
send poweroff
expect \npoweroff: cannot shut the filesystems down cleanly: Input/output error\npoweroff: use 'poweroff -f' to go ahead anyway\n
expect root@relay:~# $
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_key_steps() {
````

with:

````rust
    #[test]
    fn parses_the_unplug_step() {
        let s = parse_scenario("x", "unplug\nsend ls").unwrap();
        assert_eq!(s.steps[0], (1, Step::Unplug));
        assert!(parse_scenario("x", "unplug now").is_err());
    }

    #[test]
    fn parses_key_steps() {
````

- [ ] **Step 3: Add the failing tests to `xtask/src/qemu.rs`**

In `xtask/src/qemu.rs`, replace:

````rust
        assert!(!args.iter().any(|a| a == "-qmp" || a.contains("abstract")));
    }
````

with:

````rust
        assert!(!args.iter().any(|a| a == "-qmp" || a.contains("abstract")));
        assert!(args.contains(&"usb-storage,bus=xhci.0,drive=stick,id=stick-usb".into()));
    }
````

- [ ] **Step 4: Add the failing tests to `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, replace:

````rust
            writeln!(w, r#"{{"return": {{"status": "running"}}}}"#).unwrap();
        });
        let mut q = Qmp::connect(&socket, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
        assert_eq!(status["status"], "running");
        server.join().unwrap();
````

with:

````rust
            writeln!(w, r#"{{"return": {{"status": "running"}}}}"#).unwrap();
            line.clear();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("device_del"));
            writeln!(w, r#"{{"return": {{}}}}"#).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            // QEMU announces the device's own children first.
            writeln!(
                w,
                r#"{{"event": "DEVICE_DELETED", "data": {{"path": "/machine/peripheral/stick-usb/child"}}}}"#
            )
            .unwrap();
            writeln!(
                w,
                r#"{{"event": "DEVICE_DELETED", "data": {{"device": "stick-usb"}}}}"#
            )
            .unwrap();
            line.clear();
            // Holds the connection open until the client is done.
            let _ = r.read_line(&mut line);
        });
        let mut q = Qmp::connect(&socket, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
        assert_eq!(status["status"], "running");
        // An event that came before the answer is kept...
        assert_eq!(
            q.wait_event("RESUME", |_| true, Duration::ZERO).unwrap()["event"],
            "RESUME"
        );
        // ...and one that comes later is waited for.
        q.execute("device_del", serde_json::json!({ "id": "stick-usb" }))
            .unwrap();
        let e = q
            .wait_event(
                "DEVICE_DELETED",
                |d| d["device"] == "stick-usb",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(e["data"]["device"], "stick-usb");
        assert!(
            q.wait_event("RESUME", |_| true, Duration::from_millis(50))
                .is_err()
        );
        drop(q);
        server.join().unwrap();
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `Unplug` found for enum `e2e::Step` in the current scope ``; `` no method named `wait_event` found for struct `qmp::Qmp` in the current scope ``.

- [ ] **Step 6: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//!                                   isa-debug-exit, test mode's power-off)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
````

with:

````rust
//!                                   isa-debug-exit, test mode's power-off)
//! unplug                           (pulls the USB stick out: QMP device_del,
//!                                   then QEMU's DEVICE_DELETED event)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
````

Replace:

````rust
use crate::keys;
use crate::qemu::Qemu;
use crate::qmp::Qmp;
````

with:

````rust
use crate::keys;
use crate::qemu::{self, Qemu};
use crate::qmp::Qmp;
````

Replace:

````rust
    Poweroff,
    /// The file at `path` on the ext2 root is `line` and a newline, again
````

with:

````rust
    Poweroff,
    /// Pull the USB stick out (QMP `device_del`), waiting until QEMU has
    /// removed it.
    Unplug,
    /// The file at `path` on the ext2 root is `line` and a newline, again
````

Replace:

````rust
            "poweroff" => Step::Poweroff,
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
````

with:

````rust
            "poweroff" => Step::Poweroff,
            "unplug" if rest.is_empty() => Step::Unplug,
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
````

Replace:

````rust
            r.off = true;
        }
````

with:

````rust
            r.off = true;
        }
        Step::Unplug => {
            r.qmp.execute(
                "device_del",
                serde_json::json!({ "id": qemu::STICK_DEVICE }),
            )?;
            r.qmp.wait_event(
                "DEVICE_DELETED",
                |d| d["device"] == qemu::STICK_DEVICE,
                *timeout,
            )?;
        }
````

- [ ] **Step 7: Change `xtask/src/qemu.rs`**

In `xtask/src/qemu.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use std::process::Command;

````

with:

````rust
use std::process::Command;

/// The QEMU id of the USB stick, for QMP `device_del`.
pub const STICK_DEVICE: &str = "stick-usb";

````

Replace:

````rust
            ))
            .args(["-device", "usb-storage,bus=xhci.0,drive=stick"])
            .args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"])
````

with:

````rust
            ))
            .arg("-device")
            .arg(format!(
                "usb-storage,bus=xhci.0,drive=stick,id={STICK_DEVICE}"
            ))
            .args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"])
````

- [ ] **Step 8: Change `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    writer: UnixStream,
}
````

with:

````rust
    writer: UnixStream,
    /// Events that arrived while a command waited for its answer.
    events: Vec<Value>,
}
````

Replace:

````rust
            writer: stream,
        };
````

with:

````rust
            writer: stream,
            events: Vec::new(),
        };
````

Replace:

````rust

    /// Runs a command and returns its `return` value. Events are skipped.
    pub fn execute(&mut self, command: &str, arguments: Value) -> Result<Value> {
````

with:

````rust

    /// Runs a command and returns its `return` value. Events are kept for
    /// `wait_event`.
    pub fn execute(&mut self, command: &str, arguments: Value) -> Result<Value> {
````

Replace:

````rust
            if v.get("event").is_some() {
                continue;
````

with:

````rust
            if v.get("event").is_some() {
                self.events.push(v);
                continue;
````

Replace:

````rust
            return Ok(v.get("return").cloned().unwrap_or(Value::Null));
        }
````

with:

````rust
            return Ok(v.get("return").cloned().unwrap_or(Value::Null));
        }
    }
}

impl Qmp {
    /// Waits up to `timeout` for an event `name` whose `data` passes
    /// `wanted` (the same event can come for several devices), which may
    /// have arrived already, and returns it.
    pub fn wait_event(
        &mut self,
        name: &str,
        wanted: impl Fn(&Value) -> bool,
        timeout: Duration,
    ) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(i) = self
                .events
                .iter()
                .position(|e| e["event"] == name && wanted(&e["data"]))
            {
                return Ok(self.events.remove(i));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("no QMP event {name} within {timeout:?}");
            }
            self.writer.set_read_timeout(Some(left))?;
            let read = self.read_message();
            self.writer
                .set_read_timeout(Some(Duration::from_secs(10)))?;
            match read {
                Ok(v) if v.get("event").is_some() => self.events.push(v),
                Ok(_) => {}
                Err(_) if Instant::now() >= deadline => {
                    bail!("no QMP event {name} within {timeout:?}")
                }
                Err(e) => return Err(e),
            }
        }
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 49 tests.

- [ ] **Step 10: Run the `unplug` scenario**

Run: `cargo xtask test --e2e-only --scenario unplug`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add tests xtask
git commit -m "xtask: the unplug step and scenario: requests after the stick is pulled fail at once"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 19 scenario(s) passed`.

````bash
git push -u origin plan6/e2e
gh pr create --base main --head plan6/e2e --title "Plan 6: The e2e runner" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 15–17: QMP listens on a socket file in the private run directory instead of the abstract namespace (plan 1's #9); the `reboot` step joins the serial reader before it matches (plan 5's review); the `unplug` step and scenario: after the stick is pulled, requests fail at once with `EIO` and `poweroff` refuses without a clean shutdown (plan 5's review).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/e2e --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-e2e
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 6: Scripts and the NUC check (Tasks 18–30)

relay-sh scripts, so a NUC check runs without typing every command, and `verify-usb` checks what they printed.

Branch `plan6/scripts`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-scripts`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/scripts /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-scripts origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-scripts
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/e2e` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/scripts /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-scripts plan6/e2e`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/e2e>` and re-run `cargo xtask ci` before pushing.

### Task 18: `#` begins a comment

Plan 3's final-review minor, second half, and the first piece of scripts (decision 3): an unquoted `#` at the start of a word begins a comment that runs to the end of the line, as in bash. Inside a word (`a#b`), quoted or escaped it is a character. A line that is only a comment is like a blank line: nothing runs, nothing is synced, and the last status stays.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 14.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn single_quotes_are_literal() {
````

with:

````rust
    #[test]
    fn a_hash_at_the_start_of_a_word_begins_a_comment() {
        assert_eq!(words("echo a # b | c; $d"), ["echo", "a"]);
        assert!(words("# a whole line").is_empty());
        assert!(words("   #").is_empty());
        // As in bash: inside a word, quoted or escaped it is a character.
        assert_eq!(
            words(r##"echo a#b '#' "#" \#"##),
            ["echo", "a#b", "#", "#", "#"]
        );
        assert_eq!(
            parse("echo x >> # f"),
            Err(ParseError::MissingTarget("newline"))
        );
        let c = parse("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }

    #[test]
    fn single_quotes_are_literal() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(shell.execute("   "), 127);
    }
````

with:

````rust
        assert_eq!(shell.execute("   "), 127);
        // So does a comment, and neither is followed by a sync.
        assert_eq!(shell.execute("# echo hi"), 127);
        assert_eq!(h.spy.syncs.get(), 1);
        assert_eq!(h.console.text(), "relay-sh: nope: command not found\n");
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::a_hash_at_the_start_of_a_word_begins_a_comment`, `shell::tests::a_blank_line_keeps_the_last_status`.

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
````

with:

````rust
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
````

Replace:

````rust
            },
            c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
````

with:

````rust
            },
            // A comment runs to the end of the line.
            '#' if !word.started => break,
            c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 102 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "shell: # at the start of a word begins a comment"
````


### Task 19: `sh FILE` runs a file of commands

The script feature (decision 1). Milestone 1 has no processes, so no `bash` can run on the NUC, but relay-sh can read its commands from a file. The new built-in `sh FILE` checks and reads the file (a regular file of UTF-8 text, at most 64 KiB) and hands it to the shell through `Ctx::script`; the shell then runs it line by line with `execute`, exactly as typed lines, each shown first as `+ <line>`, so a photo of the screen shows which command printed what. Blank and comment lines are skipped; a failing command or a line that does not parse does not stop the script; Ctrl-C does (the shell asks `Console::interrupted` before each line, and a command that Ctrl-C stopped ends it), and so does a `reboot` or `poweroff` that returns. Every line is synced as it ends. A script cannot run another (`Ctx::in_script`), and the output of `sh` cannot be redirected. The test console gains `interrupt_after`, so a test can press Ctrl-C in the middle of a script.

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Create: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 18; plan 3's `Shell::execute`, `Ctx`, the command table.
- Produces: built-in `sh` (`commands::script::{sh, SCRIPT_MAX = 64 KiB}`); `Ctx::{script, in_script}` (crate-private); `Shell::run_script`; test support `TestConsole::interrupt_after`.

- [ ] **Step 1: Declare the new module in `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
mod ls;
mod stat;
````

with:

````rust
mod ls;
mod script;
mod stat;
````

- [ ] **Step 2: Write the failing tests for `crates/shell/src/commands/script.rs`**

Create `crates/shell/src/commands/script.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::Harness;
    use alloc::string::String;

    #[test]
    fn each_line_runs_after_its_trace() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo one\n\n  # a comment\necho two > /tmp/o # into o\n  cat /tmp/o\t\nnope\n",
        );
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                127,
                "+ echo one\none\n+ echo two > /tmp/o # into o\n+ cat /tmp/o\ntwo\n\
                 + nope\nrelay-sh: nope: command not found\n"
                    .into()
            )
        );
        // The exit status is the last command's.
        h.put("/tmp/ok.sh", b"nope\necho fine");
        assert_eq!(h.run("sh /tmp/ok.sh").0, 0);
    }

    #[test]
    fn a_line_that_does_not_parse_does_not_stop_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo 'open\nls | wc\necho after\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ echo 'open\nrelay-sh: syntax error: unterminated quote\n\
                 + ls | wc\nrelay-sh: unsupported syntax: |\n+ echo after\nafter\n"
                    .into()
            )
        );
    }

    #[test]
    fn every_line_is_synced_as_it_ends() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo a > /tmp/a\n# nothing\necho b > /tmp/b\n",
        );
        h.run("sh /tmp/s.sh");
        // Two commands, then `sh` itself.
        assert_eq!(h.spy.syncs.get(), 3);
        assert_eq!(h.get("/tmp/b"), b"b\n");
    }

    #[test]
    fn relative_paths_and_cd_work_as_typed() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cd /etc\ncat hostname\n");
        h.run("cd /tmp");
        assert_eq!(
            h.run("sh s.sh"),
            (0, "+ cd /etc\n+ cat hostname\nrelay\n".into())
        );
        assert_eq!(h.run("pwd").1, "/etc\n");
    }

    #[test]
    fn ctrl_c_stops_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo a\necho b\necho c\n");
        // Asked once before each line: the second time Ctrl-C was pressed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
        // A command that Ctrl-C stopped stops the script too.
        h.put("/tmp/big", &alloc::vec![b'x'; 200_000]);
        h.put("/tmp/s.sh", b"cat /tmp/big > /tmp/copy\necho after\n");
        h.console.interrupt = false;
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (130, "+ cat /tmp/big > /tmp/copy\n^C\n".into())
        );
    }

    #[test]
    fn reboot_ends_the_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"reboot\necho after\n");
        assert_eq!(h.run("sh /tmp/s.sh"), (0, "+ reboot\n".into()));
        assert_eq!(h.system.reboots, 1);
    }

    #[test]
    fn a_script_cannot_run_a_script() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"sh /tmp/s.sh\necho after\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ sh /tmp/s.sh\nsh: a script cannot run another script\n+ echo after\nafter\n"
                    .into()
            )
        );
    }

    #[test]
    fn a_script_saved_on_windows_runs() {
        // CRLF line ends and a UTF-8 byte-order mark, as Windows editors
        // save; without dropping the mark the first command is not found.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"\xEF\xBB\xBFecho one\r\necho two\r\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (0, "+ echo one\none\n+ echo two\ntwo\n".into())
        );
    }

    #[test]
    fn sh_errors() {
        let mut h = Harness::new();
        let run = |h: &mut Harness, line: &str| -> (i32, String) { h.run(line) };
        assert_eq!(run(&mut h, "sh"), (1, "sh: missing operand\n".into()));
        h.put("/tmp/s.sh", b"echo hi\n");
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh x"),
            (1, "sh: extra operand 'x'\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp/nope"),
            (1, "sh: /tmp/nope: No such file or directory\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp"),
            (1, "sh: /tmp: Is a directory\n".into())
        );
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh > /tmp/out"),
            (1, "sh: a script's output cannot be redirected\n".into())
        );
        h.put("/tmp/bin.sh", b"echo \xff\n");
        assert_eq!(
            run(&mut h, "sh /tmp/bin.sh"),
            (1, "sh: /tmp/bin.sh: not a text file\n".into())
        );
        h.put(
            "/tmp/huge.sh",
            &alloc::vec![b'#'; super::SCRIPT_MAX as usize + 1],
        );
        assert_eq!(
            run(&mut h, "sh /tmp/huge.sh"),
            (1, "sh: /tmp/huge.sh: File too large\n".into())
        );
        h.put(
            "/tmp/max.sh",
            &alloc::vec![b'#'; super::SCRIPT_MAX as usize],
        );
        assert_eq!(run(&mut h, "sh /tmp/max.sh"), (0, "".into()));
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub interrupt: bool,
}
````

with:

````rust
    pub interrupt: bool,
    /// Ctrl-C is pressed once `interrupted` has answered false this many
    /// times.
    pub interrupt_after: Option<usize>,
}
````

Replace:

````rust
            interrupt: false,
        }
````

with:

````rust
            interrupt: false,
            interrupt_after: None,
        }
````

Replace:

````rust
    fn interrupted(&mut self) -> bool {
        self.interrupt
````

with:

````rust
    fn interrupted(&mut self) -> bool {
        match &mut self.interrupt_after {
            Some(0) => self.interrupt = true,
            Some(n) => *n -= 1,
            None => {}
        }
        self.interrupt
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `SCRIPT_MAX` in module `super` ``.

- [ ] **Step 5: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
    Builtin {
        name: "stat",
````

with:

````rust
    Builtin {
        name: "sh",
        help: "run the commands in a file",
        run: script::sh,
    },
    Builtin {
        name: "stat",
````

- [ ] **Step 6: Implement `crates/shell/src/commands/script.rs`**

Insert this at the top of `crates/shell/src/commands/script.rs`, above `#[cfg(test)]`:

````rust
//! `sh FILE`: runs the commands in a file, one line at a time, as if each
//! had been typed (spec §15 item 12). There are no variables, loops or
//! conditions: a script is a list of commands. Each command is shown as
//! `+ <line>` before its output, as `set -x` does, so a photo of the
//! screen shows which command printed what. A failing command does not
//! stop the script; Ctrl-C does.

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::string::String;
use vfs::{Errno, FileType};

/// The largest script, in bytes: far more than a check needs, and read
/// whole onto the heap.
pub const SCRIPT_MAX: u64 = 64 * 1024;

/// `sh FILE`: checks and reads the file; the shell then runs its lines
/// (`Shell::execute`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("sh", format_args!("{e}")),
    };
    let file = match &opts.operands[..] {
        [] => return ctx.fail("sh", format_args!("missing operand")),
        [file] => file,
        [_, extra, ..] => return ctx.fail("sh", format_args!("extra operand {}", quote(extra))),
    };
    if ctx.in_script {
        return ctx.fail("sh", format_args!("a script cannot run another script"));
    }
    if !ctx.is_tty() {
        return ctx.fail("sh", format_args!("a script's output cannot be redirected"));
    }
    let name = quote_if_needed(file);
    match read(ctx, file) {
        Ok(text) => {
            ctx.script = Some(text);
            0
        }
        Err(Error::Errno(e)) => ctx.fail("sh", format_args!("{name}: {e}")),
        Err(Error::NotText) => ctx.fail("sh", format_args!("{name}: not a text file")),
    }
}

enum Error {
    Errno(Errno),
    NotText,
}

impl From<Errno> for Error {
    fn from(e: Errno) -> Error {
        Error::Errno(e)
    }
}

/// The whole file, which must be a regular file of UTF-8 text no larger
/// than `SCRIPT_MAX`.
fn read(ctx: &mut Ctx<'_>, file: &str) -> Result<String, Error> {
    let node = ctx.vfs.lookup(file.as_bytes())?;
    let st = ctx.vfs.stat(node)?;
    match st.kind {
        FileType::Regular => {}
        FileType::Directory => return Err(Errno::EISDIR.into()),
        _ => return Err(Errno::EINVAL.into()),
    }
    if st.size > SCRIPT_MAX {
        return Err(Errno::EFBIG.into());
    }
    let mut buf = alloc::vec![0; st.size as usize];
    let mut done = 0;
    while done < buf.len() {
        match ctx.vfs.read_at(node, done as u64, &mut buf[done..])? {
            0 => break,
            n => done += n,
        }
    }
    buf.truncate(done);
    let text = String::from_utf8(buf).map_err(|_| Error::NotText)?;
    // Windows editors may start the file with a byte-order mark.
    match text.strip_prefix('\u{FEFF}') {
        Some(rest) => Ok(String::from(rest)),
        None => Ok(text),
    }
}

````

- [ ] **Step 7: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub(crate) cancelled: bool,
}
````

with:

````rust
    pub(crate) cancelled: bool,
    /// Set by `sh`: the script the shell runs next.
    pub(crate) script: Option<String>,
    /// The command is a line of a script.
    pub(crate) in_script: bool,
}
````

Replace:

````rust
            cancelled: false,
        }
````

with:

````rust
            cancelled: false,
            script: None,
            in_script: false,
        }
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    stopped: bool,
}
````

with:

````rust
    stopped: bool,
    /// A script's lines are running (`sh`).
    in_script: bool,
}
````

Replace:

````rust
            stopped: false,
        }
````

with:

````rust
            stopped: false,
            in_script: false,
        }
````

Replace:

````rust
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
````

with:

````rust
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.in_script = self.in_script;
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
````

Replace:

````rust
        self.stopped = ctx.exit;
        self.finish(status, message)
    }
````

with:

````rust
        self.stopped = ctx.exit;
        if let Some(script) = ctx.script.take() {
            status = self.run_script(&script);
        }
        self.finish(status, message)
    }

    /// Runs a script's lines (spec §15 item 12): each command is shown as
    /// `+ <line>`, then runs and is synced as if typed. Blank and comment
    /// lines are skipped. Ctrl-C, or `reboot`/`poweroff` returning, ends
    /// the script; failing commands do not. Returns the last status.
    fn run_script(&mut self, script: &str) -> i32 {
        self.in_script = true;
        let mut status = 0;
        for line in script.lines() {
            let line = line.trim();
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
                continue;
            }
            if self.console.interrupted() {
                self.console.write(b"^C\n");
                status = CANCELLED;
                break;
            }
            self.console.write(format!("+ {line}\n").as_bytes());
            status = self.execute(line);
            if status == CANCELLED || self.stopped {
                break;
            }
        }
        self.in_script = false;
        status
    }
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 111 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -m "shell: sh FILE runs a file of commands, each shown before its output"
````


### Task 20: A script line runs exactly as written

Review finding (minor): the shell trimmed each script line before running it, so a line ending in an escaped space (`echo a\ `) lost the space and failed with `nothing after \`, where the same line typed prints `a `. The line now runs exactly as written; only its `+` trace is trimmed, which is also what the transcript checker compares (decision 1).

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 19.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
        assert_eq!(h.run("sh /tmp/ok.sh").0, 0);
    }
````

with:

````rust
        assert_eq!(h.run("sh /tmp/ok.sh").0, 0);
    }

    #[test]
    fn a_line_runs_exactly_as_typed() {
        // Only the trace is trimmed; `\ ` at the end of a line is a space.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"  echo a\\ \n\techo 'b '  \n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (0, "+ echo a\\\na \n+ echo 'b '\nb \n".into())
        );
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::script::tests::a_line_runs_exactly_as_typed`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        for line in script.lines() {
            let line = line.trim();
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
````

with:

````rust
        for line in script.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
````

Replace:

````rust
            }
            self.console.write(format!("+ {line}\n").as_bytes());
            status = self.execute(line);
````

with:

````rust
            }
            // The line runs as written; only its trace is trimmed.
            self.console
                .write(format!("+ {}\n", line.trim()).as_bytes());
            status = self.execute(line);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 112 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: a script line runs exactly as written; only its trace is trimmed"
````


### Task 21: A script's screen output also goes into its transcript

Decision 2. Everything a script shows on the screen, errors included (which never go into a redirection file, spec §7.3), also goes into a transcript next to it: `x.sh` becomes `x.log`, other names get `.log` added. `sh` creates or empties it before anything runs, so a script whose transcript cannot be written (a read-only `/`, a directory in its place) does not run. While the script runs, the shell's console is wrapped in a `Tee` that copies what reaches the screen; the copy is written to the transcript after each line's `+` trace and again when the line ends, before its sync, so a machine that hangs or restarts leaves every line up to the one it stopped in, and `reboot`'s own line is on the disk before it shuts the filesystem down. If a write fails, the transcript ends there with `sh: x.log: <error>; the transcript ends here`, and the script goes on.

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 19.
- Produces: `shell::commands::transcript_name(script: &str) -> String`; `commands::Script { text, transcript, transcript_name }` (crate-private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
        // A command that Ctrl-C stopped stops the script too.
````

with:

````rust
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
        assert_eq!(h.get("/tmp/s.log"), b"+ echo a\na\n^C\n");
        // A command that Ctrl-C stopped stops the script too.
````

Replace:

````rust
        assert_eq!(h.system.reboots, 1);
    }
````

with:

````rust
        assert_eq!(h.system.reboots, 1);
        // The transcript names the command that restarted the machine.
        assert_eq!(h.get("/tmp/s.log"), b"+ reboot\n");
    }
````

Replace:

````rust
            (0, "+ echo one\none\n+ echo two\ntwo\n".into())
        );
````

with:

````rust
            (0, "+ echo one\none\n+ echo two\ntwo\n".into())
        );
    }

    #[test]
    fn the_transcript_holds_what_the_screen_showed() {
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"echo one\n# not shown\necho two > /tmp/o\ncat /tmp/nope\nnope\ncat /tmp/o\n",
        );
        let (status, screen) = h.run("sh /tmp/s.sh");
        assert_eq!(status, 0);
        assert_eq!(
            screen,
            "+ echo one\none\n+ echo two > /tmp/o\n+ cat /tmp/nope\n\
             cat: /tmp/nope: No such file or directory\n+ nope\n\
             relay-sh: nope: command not found\n+ cat /tmp/o\ntwo\n"
        );
        // Errors too, which never go into a redirection file.
        assert_eq!(String::from_utf8(h.get("/tmp/s.log")).unwrap(), screen);
        // Running a script again starts a new transcript.
        h.put("/tmp/s.sh", b"echo again\n");
        h.run("sh /tmp/s.sh");
        assert_eq!(h.get("/tmp/s.log"), b"+ echo again\nagain\n");
    }

    #[test]
    fn the_transcript_is_written_as_each_line_starts_and_ends() {
        // A machine that hangs in a command leaves the lines before it and
        // the command's own name.
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo one\ncat /tmp/s.log\n");
        assert_eq!(
            h.run("sh /tmp/s.sh").1,
            "+ echo one\none\n+ cat /tmp/s.log\n+ echo one\none\n+ cat /tmp/s.log\n"
        );
    }

    #[test]
    fn transcript_names() {
        let mut h = Harness::new();
        h.dir("/tmp/d.sh");
        for (script, log) in [
            ("/tmp/a.sh", "/tmp/a.log"),
            ("/tmp/b", "/tmp/b.log"),
            ("/tmp/d.sh/c.txt", "/tmp/d.sh/c.txt.log"),
            ("/tmp/d.sh/.sh", "/tmp/d.sh/.log"),
        ] {
            h.put(script, b"echo x\n");
            h.run(&alloc::format!("sh {script}"));
            assert_eq!(h.get(log), b"+ echo x\nx\n", "{script}");
        }
        // A relative name is taken from where `sh` ran, whatever the
        // script's `cd` does.
        h.put("/tmp/r.sh", b"cd /\necho y\n");
        h.run("cd /tmp");
        h.run("sh r.sh");
        assert_eq!(h.get("/tmp/r.log"), b"+ cd /\n+ echo y\ny\n");
    }

    #[test]
    fn a_transcript_that_cannot_be_written() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo a > /tmp/a\n");
        h.dir("/tmp/s.log");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                1,
                "sh: cannot write the transcript /tmp/s.log: Is a directory\n".into()
            )
        );
        assert!(!h.exists("/tmp/a"), "nothing ran");
        // A disk that fills up ends the transcript, not the script.
        let mut h = Harness::with_capacity(3 * 4096);
        h.put("/tmp/s.sh", b"echo a\necho b\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ echo a\nsh: /tmp/s.log: No space left on device; the transcript ends here\n\
                 a\n+ echo b\nb\n"
                    .into()
            )
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 6 tests fail, among them `commands::script::tests::a_transcript_that_cannot_be_written`, `commands::script::tests::ctrl_c_stops_the_script`.

- [ ] **Step 3: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
mod stat;
mod system;
````

with:

````rust
mod stat;

pub(crate) use script::Script;
mod system;
````

- [ ] **Step 4: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! screen shows which command printed what. A failing command does not
//! stop the script; Ctrl-C does.

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::string::String;
use vfs::{Errno, FileType};

````

with:

````rust
//! screen shows which command printed what. A failing command does not
//! stop the script; Ctrl-C does. Everything the script shows on the screen,
//! errors included, also goes into a transcript next to it (`x.sh` →
//! `x.log`), written as each line ends, so it can be checked afterwards
//! (`cargo xtask verify-usb`).

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::format;
use alloc::string::String;
use vfs::{Errno, FileType, Node, path};

````

Replace:

````rust

/// `sh FILE`: checks and reads the file; the shell then runs its lines
/// (`Shell::execute`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust

/// A script `sh` has read, for the shell to run.
pub(crate) struct Script {
    pub text: String,
    /// The transcript file, emptied, and its name as `sh` was given it.
    pub transcript: Node,
    pub transcript_name: String,
}

/// The transcript of `script`: `.sh` becomes `.log`; other names get
/// `.log` added.
pub fn transcript_name(script: &str) -> String {
    match script.strip_suffix(".sh") {
        Some(stem) => format!("{stem}.log"),
        None => format!("{script}.log"),
    }
}

/// `sh FILE`: checks and reads the file and empties its transcript; the
/// shell then runs its lines (`Shell::execute`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
    let name = quote_if_needed(file);
    match read(ctx, file) {
        Ok(text) => {
            ctx.script = Some(text);
            0
        }
        Err(Error::Errno(e)) => ctx.fail("sh", format_args!("{name}: {e}")),
        Err(Error::NotText) => ctx.fail("sh", format_args!("{name}: not a text file")),
    }
````

with:

````rust
    let name = quote_if_needed(file);
    let text = match read(ctx, file) {
        Ok(text) => text,
        Err(Error::Errno(e)) => return ctx.fail("sh", format_args!("{name}: {e}")),
        Err(Error::NotText) => return ctx.fail("sh", format_args!("{name}: not a text file")),
    };
    let log = transcript_name(file);
    match empty_file(ctx, &log) {
        Ok(transcript) => {
            ctx.script = Some(Script {
                text,
                transcript,
                transcript_name: log,
            });
            0
        }
        Err(e) => ctx.fail(
            "sh",
            format_args!(
                "cannot write the transcript {}: {e}",
                quote_if_needed(&path::display(log.as_bytes()))
            ),
        ),
    }
}

/// Creates the file at `name`, or empties it.
fn empty_file(ctx: &mut Ctx<'_>, name: &str) -> Result<Node, Errno> {
    match ctx.vfs.lookup(name.as_bytes()) {
        Ok(node) => {
            match ctx.vfs.stat(node)?.kind {
                FileType::Regular => {}
                FileType::Directory => return Err(Errno::EISDIR),
                _ => return Err(Errno::EINVAL),
            }
            ctx.vfs.truncate(node, 0)?;
            Ok(node)
        }
        Err(Errno::ENOENT) => ctx.vfs.create(name.as_bytes()),
        Err(e) => Err(e),
    }
````

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust
    /// Set by `sh`: the script the shell runs next.
    pub(crate) script: Option<String>,
    /// The command is a line of a script.
````

with:

````rust
    /// Set by `sh`: the script the shell runs next.
    pub(crate) script: Option<crate::commands::Script>,
    /// The command is a line of a script.
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 8 replacements, top to bottom:

Replace:

````rust

use crate::commands;
use crate::ctx::Ctx;
````

with:

````rust

use crate::commands::{self, Script};
use crate::ctx::Ctx;
````

Replace:

````rust
    in_script: bool,
}
````

with:

````rust
    in_script: bool,
    /// Where a running script's screen output is copied.
    transcript: Option<Transcript>,
}

/// A running script's transcript: what the screen showed since the last
/// line ended waits in `pending`.
struct Transcript {
    node: Node,
    offset: u64,
    pending: Vec<u8>,
    name: String,
}

/// The screen, and a copy of what is written to it for the transcript.
struct Tee<'c> {
    console: &'c mut dyn Console,
    copy: Option<&'c mut Vec<u8>>,
}

impl Console for Tee<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        self.console.read_byte()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(copy) = &mut self.copy {
            copy.extend_from_slice(bytes);
        }
    }
    fn columns(&self) -> usize {
        self.console.columns()
    }
    fn interrupted(&mut self) -> bool {
        self.console.interrupted()
    }
}
````

Replace:

````rust
            in_script: false,
        }
````

with:

````rust
            in_script: false,
            transcript: None,
        }
````

Replace:

````rust
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.in_script = self.in_script;
````

with:

````rust
        };
        let mut screen = Tee {
            console: &mut *self.console,
            copy: self.transcript.as_mut().map(|t| &mut t.pending),
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut screen, file);
        ctx.in_script = self.in_script;
````

Replace:

````rust
        if let Some(script) = ctx.script.take() {
            status = self.run_script(&script);
        }
        self.finish(status, message)
    }
````

with:

````rust
        if let Some(script) = ctx.script.take() {
            status = self.run_script(script);
        }
        self.finish(status, message)
    }

    /// Writes to the screen and, while a script runs, its transcript.
    fn say(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript {
            t.pending.extend_from_slice(bytes);
        }
    }

    /// Adds what the screen showed to the transcript. If that fails the
    /// transcript ends there, with a message; the script goes on.
    fn write_transcript(&mut self) {
        let Some(t) = &mut self.transcript else {
            return;
        };
        let mut done = 0;
        while done < t.pending.len() {
            let result = match self.vfs.write_at(t.node, t.offset, &t.pending[done..]) {
                Ok(0) => Err(Errno::ENOSPC),
                other => other,
            };
            match result {
                Ok(n) => {
                    done += n;
                    t.offset += n as u64;
                }
                Err(e) => {
                    let name = path::display(t.name.as_bytes());
                    self.transcript = None;
                    self.console
                        .write(format!("sh: {name}: {e}; the transcript ends here\n").as_bytes());
                    return;
                }
            }
        }
        t.pending.clear();
    }
````

Replace:

````rust
    /// the script; failing commands do not. Returns the last status.
    fn run_script(&mut self, script: &str) -> i32 {
        self.in_script = true;
        let mut status = 0;
        for line in script.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
                continue;
            }
            if self.console.interrupted() {
                self.console.write(b"^C\n");
                status = CANCELLED;
                break;
            }
            // The line runs as written; only its trace is trimmed.
            self.console
                .write(format!("+ {}\n", line.trim()).as_bytes());
            status = self.execute(line);
````

with:

````rust
    /// the script; failing commands do not. Returns the last status.
    fn run_script(&mut self, script: Script) -> i32 {
        self.in_script = true;
        self.transcript = Some(Transcript {
            node: script.transcript,
            offset: 0,
            pending: Vec::new(),
            name: script.transcript_name,
        });
        let mut status = 0;
        for line in script.text.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
                continue;
            }
            if self.console.interrupted() {
                self.say(b"^C\n");
                status = CANCELLED;
                break;
            }
            // The line runs as written; only its trace is trimmed.
            self.say(format!("+ {}\n", line.trim()).as_bytes());
            // Before the command runs: `reboot` shuts the disk down, and a
            // command that hangs leaves at least its name.
            self.write_transcript();
            status = self.execute(line);
````

Replace:

````rust
        }
        self.in_script = false;
````

with:

````rust
        }
        self.write_transcript();
        self.transcript = None;
        self.in_script = false;
````

Replace:

````rust

    /// Prints `message`, syncs, and records `status`.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        self.console.write(message.as_bytes());
        if let Err(e) = self.vfs.sync() {
            self.console
                .write(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
````

with:

````rust

    /// Prints `message`, adds the line's output to a running script's
    /// transcript, syncs, and records `status`.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        self.say(message.as_bytes());
        self.write_transcript();
        if let Err(e) = self.vfs.sync() {
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 116 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -m "shell: a script's screen output also goes into its transcript"
````


### Task 22: A transcript is written every 4 KiB

Review finding (important): the transcript kept everything a command showed on the heap until the line ended, so a script line `cat` of a file over about 16 MiB needed one 32 MiB allocation, which the 32 MiB kernel heap cannot give: the kernel would panic on data from the disk (spec §10). Typed at the prompt the same `cat` streams in pieces. The transcript moves into its own module, `transcript.rs`, and `Ctx` writes a command's screen output into it once 4 KiB wait, as it writes a redirection file (decision 2); the `Tee` console is gone. The test support gains `SpyState::largest_write`.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Create: `crates/shell/src/transcript.rs`

**Interfaces:**
- Consumes: Task 21.
- Produces: `shell::transcript::{CHUNK = 4096, Transcript { new, add, write, ended }}` (crate-private); `Ctx::transcript`; test support `SpyState::largest_write`.

- [ ] **Step 1: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod time;

````

with:

````rust
mod time;
mod transcript;

````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub reads: Cell<u32>,
}
````

with:

````rust
    pub reads: Cell<u32>,
    /// The most bytes one `write_at` was given.
    pub largest_write: Cell<usize>,
}
````

Replace:

````rust
    fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        if self.state.zero_writes.get() {
````

with:

````rust
    fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        let largest = self.state.largest_write.get().max(buf.len());
        self.state.largest_write.set(largest);
        if self.state.zero_writes.get() {
````

- [ ] **Step 3: Write the failing tests for `crates/shell/src/transcript.rs`**

Create `crates/shell/src/transcript.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::Harness;

    #[test]
    fn a_command_that_prints_much_is_written_as_it_goes() {
        // `cat` of a big file inside a script: its output reaches the
        // transcript in pieces, not all at once when the line ends.
        let mut h = Harness::new();
        let big = alloc::vec![b'x'; 200_000];
        h.put("/tmp/big", &big);
        h.put("/tmp/s.sh", b"cat /tmp/big\n");
        h.spy.largest_write.set(0);
        assert_eq!(h.run("sh /tmp/s.sh").0, 0);
        let log = h.get("/tmp/s.log");
        assert_eq!(&log[..15], b"+ cat /tmp/big\n");
        assert_eq!(&log[15..], &big[..]);
        // cat hands the screen 64 KiB at a time.
        assert!(h.spy.largest_write.get() <= 64 * 1024 + super::CHUNK);
    }
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `CHUNK` in module `super` ``.

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use crate::io::{Console, System};
use alloc::format;
````

with:

````rust
use crate::io::{Console, System};
use crate::transcript::Transcript;
use alloc::format;
````

Replace:

````rust
    pub(crate) in_script: bool,
}
````

with:

````rust
    pub(crate) in_script: bool,
    /// A running script's transcript, which gets what the screen gets.
    pub(crate) transcript: Option<Transcript>,
}
````

Replace:

````rust
            in_script: false,
        }
````

with:

````rust
            in_script: false,
            transcript: None,
        }
````

Replace:

````rust
        match &mut self.out {
            Output::Console => self.console.write(bytes),
            Output::File { buf, .. } => {
````

with:

````rust
        match &mut self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, .. } => {
````

Replace:

````rust
    pub fn err(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
    }
````

with:

````rust
    pub fn err(&mut self, bytes: &[u8]) {
        self.screen(bytes);
    }

    /// Writes to the screen and a running script's transcript.
    fn screen(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.console.write(t.ended(e).as_bytes());
            self.transcript = None;
        }
    }
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
use crate::parser::{self, HOME, Redirect};
use alloc::format;
````

with:

````rust
use crate::parser::{self, HOME, Redirect};
use crate::transcript::Transcript;
use alloc::format;
````

Replace:

````rust
    transcript: Option<Transcript>,
}

/// A running script's transcript: what the screen showed since the last
/// line ended waits in `pending`.
struct Transcript {
    node: Node,
    offset: u64,
    pending: Vec<u8>,
    name: String,
}

/// The screen, and a copy of what is written to it for the transcript.
struct Tee<'c> {
    console: &'c mut dyn Console,
    copy: Option<&'c mut Vec<u8>>,
}

impl Console for Tee<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        self.console.read_byte()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(copy) = &mut self.copy {
            copy.extend_from_slice(bytes);
        }
    }
    fn columns(&self) -> usize {
        self.console.columns()
    }
    fn interrupted(&mut self) -> bool {
        self.console.interrupted()
    }
}
````

with:

````rust
    transcript: Option<Transcript>,
}
````

Replace:

````rust
        };
        let mut screen = Tee {
            console: &mut *self.console,
            copy: self.transcript.as_mut().map(|t| &mut t.pending),
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut screen, file);
        ctx.in_script = self.in_script;
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
````

with:

````rust
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.in_script = self.in_script;
        ctx.transcript = self.transcript.take();
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
````

Replace:

````rust
        }
        self.stopped = ctx.exit;
````

with:

````rust
        }
        self.transcript = ctx.transcript.take();
        self.stopped = ctx.exit;
````

Replace:

````rust
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript {
            t.pending.extend_from_slice(bytes);
        }
    }

    /// Adds what the screen showed to the transcript. If that fails the
    /// transcript ends there, with a message; the script goes on.
    fn write_transcript(&mut self) {
        let Some(t) = &mut self.transcript else {
            return;
        };
        let mut done = 0;
        while done < t.pending.len() {
            let result = match self.vfs.write_at(t.node, t.offset, &t.pending[done..]) {
                Ok(0) => Err(Errno::ENOSPC),
                other => other,
            };
            match result {
                Ok(n) => {
                    done += n;
                    t.offset += n as u64;
                }
                Err(e) => {
                    let name = path::display(t.name.as_bytes());
                    self.transcript = None;
                    self.console
                        .write(format!("sh: {name}: {e}; the transcript ends here\n").as_bytes());
                    return;
                }
            }
        }
        t.pending.clear();
    }
````

with:

````rust
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.end_transcript(e);
        }
    }

    /// Writes what the screen showed to the transcript. If that fails the
    /// transcript ends there, with a message; the script goes on.
    fn write_transcript(&mut self) {
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.write(&mut *self.vfs)
        {
            self.end_transcript(e);
        }
    }

    fn end_transcript(&mut self, e: Errno) {
        if let Some(t) = self.transcript.take() {
            self.console.write(t.ended(e).as_bytes());
        }
    }
````

Replace:

````rust
        self.in_script = true;
        self.transcript = Some(Transcript {
            node: script.transcript,
            offset: 0,
            pending: Vec::new(),
            name: script.transcript_name,
        });
        let mut status = 0;
````

with:

````rust
        self.in_script = true;
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut status = 0;
````

- [ ] **Step 7: Implement `crates/shell/src/transcript.rs`**

Insert this at the top of `crates/shell/src/transcript.rs`, above `#[cfg(test)]`:

````rust
//! A running script's transcript (spec §15 item 12): what the screen shows,
//! written into a file next to the script as the script goes.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, Node, Vfs, path};

/// Screen output waits on the heap until this much has come, or the line
/// ends: a command that prints a big file must not hold all of it.
pub const CHUNK: usize = 4096;

pub(crate) struct Transcript {
    node: Node,
    offset: u64,
    pending: Vec<u8>,
    name: String,
}

impl Transcript {
    /// The transcript file `node`, empty, called `name` in messages.
    pub fn new(node: Node, name: String) -> Transcript {
        Transcript {
            node,
            offset: 0,
            pending: Vec::new(),
            name,
        }
    }

    /// Adds what the screen showed; writes it once `CHUNK` bytes wait.
    pub fn add(&mut self, vfs: &mut dyn Vfs, bytes: &[u8]) -> Result<(), Errno> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() >= CHUNK {
            self.write(vfs)?;
        }
        Ok(())
    }

    /// Writes everything that waits.
    pub fn write(&mut self, vfs: &mut dyn Vfs) -> Result<(), Errno> {
        let mut done = 0;
        while done < self.pending.len() {
            match vfs.write_at(self.node, self.offset, &self.pending[done..]) {
                // Nothing written would loop forever; the contract says
                // that is ENOSPC.
                Ok(0) => return Err(Errno::ENOSPC),
                Ok(n) => {
                    done += n;
                    self.offset += n as u64;
                }
                Err(e) => return Err(e),
            }
        }
        self.pending.clear();
        Ok(())
    }

    /// What the screen says when a write failed; the transcript ends there.
    pub fn ended(&self, e: Errno) -> String {
        let name = path::display(self.name.as_bytes());
        format!("sh: {name}: {e}; the transcript ends here\n")
    }
}

````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 117 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -m "shell: a transcript is written every 4 KiB, so a command's output is never held whole"
````


### Task 23: A script's trace line is synced before its command runs

Review finding (minor): the `+` line was written into the transcript before its command ran but only synced after it, and the block cache writes back, so a machine that hung in the command and was switched off lost that line, against decision 2. The shell now syncs after writing it.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 22.
- Produces: `Shell::sync`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn every_line_is_synced_as_it_ends() {
        let mut h = Harness::new();
````

with:

````rust
    #[test]
    fn every_line_is_synced_as_it_starts_and_ends() {
        // The trace reaches the disk before the command runs, so a machine
        // that hangs in it leaves the command's name in the transcript.
        let mut h = Harness::new();
````

Replace:

````rust
        h.run("sh /tmp/s.sh");
        // Two commands, then `sh` itself.
        assert_eq!(h.spy.syncs.get(), 3);
        assert_eq!(h.get("/tmp/b"), b"b\n");
````

with:

````rust
        h.run("sh /tmp/s.sh");
        // Two per command, then `sh` itself.
        assert_eq!(h.spy.syncs.get(), 5);
        assert_eq!(h.get("/tmp/b"), b"b\n");
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::script::tests::every_line_is_synced_as_it_starts_and_ends`.

- [ ] **Step 3: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
//! errors included, also goes into a transcript next to it (`x.sh` →
//! `x.log`), written as each line ends, so it can be checked afterwards
//! (`cargo xtask verify-usb`).

````

with:

````rust
//! errors included, also goes into a transcript next to it (`x.sh` →
//! `x.log`), written and synced as each line starts and ends, so it can be
//! checked afterwards (`cargo xtask verify-usb`).

````

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            self.say(format!("+ {}\n", line.trim()).as_bytes());
            // Before the command runs: `reboot` shuts the disk down, and a
            // command that hangs leaves at least its name.
            self.write_transcript();
            status = self.execute(line);
````

with:

````rust
            self.say(format!("+ {}\n", line.trim()).as_bytes());
            // On the disk before the command runs: a command that hangs
            // leaves at least its name.
            self.write_transcript();
            self.sync();
            status = self.execute(line);
````

Replace:

````rust
        self.write_transcript();
        if let Err(e) = self.vfs.sync() {
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
        self.status = status;
        status
    }
````

with:

````rust
        self.write_transcript();
        self.sync();
        self.status = status;
        status
    }

    fn sync(&mut self) {
        if let Err(e) = self.vfs.sync() {
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 117 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "shell: a script's trace line is synced before its command runs"
````


### Task 24: Transcripts are checked against the output their script expects

Decision 4. `xtask/src/checks.rs` reads the expectations a check script writes under each command: `#> <regex>` is one whole output line, in order; `#> ...` any number of lines; `#nuc>` and `#qemu>` lines apply only on that machine; `#!> <regex>` must match no line (a command with only `#!>` lines may print anything else); a command with no `#>` line must print nothing, so an unexpected error fails it. `check` walks the transcript by the commands' `+` traces and reports every command whose output differs, naming the script line, the command, what was expected and what it printed, and the first command that is missing (the machine hung or restarted, or the script changed). Matching is dynamic programming over (pattern, line), so many `...` stay cheap. The e2e step `check-script <path>` reads a script and its transcript off the disk once the machine is off and requires them to agree, as on QEMU.

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Create: `xtask/src/checks.rs`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/main.rs`

**Interfaces:**
- Consumes: Task 21 (`transcript_name`); `image::read_ext2_file`.
- Produces: `checks::{Machine { Qemu, Nuc }, Command, parse(script: &str, Machine) -> Result<Vec<Command>>, Report { commands, passed, failures } (ok), check(&[Command], transcript: &str) -> Report}`; e2e step `check-script <path>` (`Step::CheckScript`).

- [ ] **Step 1: Write the failing tests for `xtask/src/checks.rs`**

Create `xtask/src/checks.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "\
# A check.
mkdir -p /root/n
cat /root/n/a
#> remember me
#> and \\w+
dmesg
#> ...
#nuc> usb: 00:14\\.0 port 15: .*Kingston.*
#qemu> usb: 00:02\\.0 port 2: .*QEMU HARDDISK.*
#> ...
#!> \\[FAIL\\].*
ls /
#!> .*lost\\+found.*
";

    fn run(machine: Machine, transcript: &str) -> Report {
        check(&parse(SCRIPT, machine).unwrap(), transcript)
    }

    const QEMU_LOG: &str = "\
+ mkdir -p /root/n
+ cat /root/n/a
remember me
and me
+ dmesg
[ ok ] pci: 3 devices
usb: 00:02.0 port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB
[ ok ] mount /
+ ls /
bin  etc
";

    #[test]
    fn a_transcript_that_matches_passes() {
        let r = run(Machine::Qemu, QEMU_LOG);
        assert_eq!((r.commands, r.passed), (4, 4), "{:?}", r.failures);
        assert!(r.ok());
    }

    #[test]
    fn lines_tagged_for_the_other_machine_do_not_count() {
        let r = run(Machine::Nuc, QEMU_LOG);
        assert_eq!(r.passed, 3);
        assert_eq!(r.failures.len(), 1);
        assert!(
            r.failures[0].starts_with(
                "line 6: `dmesg`: expected [..., /usb: 00:14\\.0 port 15: .*Kingston.*/, ...]"
            ),
            "{}",
            r.failures[0]
        );
    }

    #[test]
    fn every_output_line_must_be_expected() {
        // A command with nothing expected prints nothing: an error fails it.
        let log = QEMU_LOG.replace(
            "+ mkdir -p /root/n\n",
            "+ mkdir -p /root/n\nmkdir: cannot create directory '/root/n': Read-only file system\n",
        );
        let r = run(Machine::Qemu, &log);
        assert_eq!(r.passed, 3);
        assert_eq!(
            r.failures,
            [
                "line 2: `mkdir -p /root/n`: expected [], printed [mkdir: cannot create directory '/root/n': Read-only file system]"
            ]
        );
        // Lines are whole lines, in order, and none may be left over.
        for bad in [
            "remember me\nand me\nextra\n",
            "and me\nremember me\n",
            "remember me!\nand me\n",
            "remember me\n",
        ] {
            let log = QEMU_LOG.replace("remember me\nand me\n", bad);
            assert_eq!(run(Machine::Qemu, &log).passed, 3, "{bad:?}");
        }
    }

    #[test]
    fn a_line_that_must_not_appear_fails_the_command() {
        let log = QEMU_LOG.replace("[ ok ] mount /", "[FAIL] mount /: no USB disk");
        let r = run(Machine::Qemu, &log);
        assert_eq!(
            r.failures,
            [
                "line 6: `dmesg`: printed `[FAIL] mount /: no USB disk`, which matches /\\[FAIL\\].*/"
            ]
        );
        // Alone, `#!>` lines allow any other output.
        let log = QEMU_LOG.replace("bin  etc", "bin  lost+found");
        assert_eq!(run(Machine::Qemu, &log).passed, 3);
    }

    #[test]
    fn a_transcript_that_ends_early_or_goes_astray() {
        let log = &QEMU_LOG[..QEMU_LOG.find("+ dmesg").unwrap()];
        let r = run(Machine::Qemu, log);
        assert_eq!(r.passed, 2);
        assert_eq!(
            r.failures,
            ["line 6: `dmesg` and the 1 command(s) after it are not in the transcript"]
        );
        let r = run(Machine::Qemu, &QEMU_LOG.replace("+ ls /", "+ ls /root"));
        assert_eq!(
            r.failures,
            ["line 12: `ls /` and the 0 command(s) after it are not in the transcript"]
        );
        assert_eq!(
            run(Machine::Qemu, "").failures,
            ["line 2: `mkdir -p /root/n` and the 3 command(s) after it are not in the transcript"]
        );
        assert_eq!(
            run(Machine::Qemu, "+ mkdir /root/n\n").failures,
            ["line 2: expected `+ mkdir -p /root/n` in the transcript, found `+ mkdir /root/n`"]
        );
    }

    #[test]
    fn many_wildcards_stay_fast() {
        let script = format!("dmesg\n{}#> never\n", "#> ...\n".repeat(40));
        let log = format!("+ dmesg\n{}", "x\n".repeat(2000));
        let r = check(&parse(&script, Machine::Qemu).unwrap(), &log);
        assert_eq!(r.passed, 0);
    }

    #[test]
    fn colour_codes_and_carriage_returns_are_ignored() {
        let log = QEMU_LOG.replace("remember me\n", "\x1b[1mremember me\x1b[0m\r\n");
        assert!(run(Machine::Qemu, &log).ok());
    }

    #[test]
    fn script_errors() {
        assert!(parse("#> x\nls\n", Machine::Qemu).is_err());
        assert!(parse("ls\n#> (\n", Machine::Qemu).is_err());
        assert!(parse("# only a comment\n", Machine::Qemu).is_err());
        // Comments, including ones with `>`, are not expectations.
        let c = parse("ls\n# see > there\n#other> x\n", Machine::Qemu).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c[0].expect.is_empty());
    }
}
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_the_unplug_step() {
````

with:

````rust
    #[test]
    fn parses_the_check_script_step() {
        let s = parse_scenario("x", "poweroff\ncheck-script /root/checks/a.sh").unwrap();
        assert_eq!(
            s.steps[1],
            (2, Step::CheckScript("/root/checks/a.sh".into()))
        );
        assert!(parse_scenario("x", "check-script").is_err());
        assert!(parse_scenario("x", "check-script root/a.sh").is_err());
    }

    #[test]
    fn parses_the_unplug_step() {
````

- [ ] **Step 3: Declare the new module in `xtask/src/main.rs`**

In `xtask/src/main.rs`, replace:

````rust
mod build;
mod ci;
````

with:

````rust
mod build;
mod checks;
mod ci;
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find type `Machine` in this scope ``; `` cannot find type `Report` in this scope ``.

- [ ] **Step 5: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
pub(crate) use script::Script;
mod system;
````

with:

````rust
pub(crate) use script::Script;
pub use script::transcript_name;
mod system;
````

- [ ] **Step 6: Implement `xtask/src/checks.rs`**

Insert this at the top of `xtask/src/checks.rs`, above `#[cfg(test)]`:

````rust
//! Checks a script's transcript against the output the script expects
//! (spec §15 item 12). The NUC's hardware checks are relay-sh scripts under
//! `/root/checks/`; `sh` writes what each shows into a transcript, and
//! `cargo xtask verify-usb` (on the stick) and the e2e step `check-script`
//! (in QEMU) check it here.
//!
//! Under each command, the script says what it must print:
//!
//! ```text
//! cat /root/notes/a
//! #> remember me          each `#>` line: a regex for one whole output line,
//! #> and me               in order
//! dmesg
//! #> ...                  any number of lines
//! #nuc> \[ ok \] usb: .*  only on the NUC (`#qemu>`: only in QEMU)
//! #> ...
//! #!> \[FAIL\].*          no output line may match
//! mkdir /root/x           nothing expected: it must print nothing
//! ```
//!
//! A command whose only expectations are `#!>` lines may print anything
//! else. Other `#` lines are comments.

use crate::e2e::strip_ansi;
use anyhow::{Context, Result, bail};
use regex::Regex;

/// Where a transcript was written: some output differs between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    Qemu,
    Nuc,
}

#[derive(Debug)]
enum Expect {
    /// One whole output line.
    Line(Regex),
    /// Any number of lines.
    Any,
    /// No output line matches.
    Never(Regex),
}

/// One command of a script and what it must print.
#[derive(Debug)]
pub struct Command {
    /// Line number in the script.
    pub line: usize,
    /// The command as `sh` shows it, after `+ `.
    pub text: String,
    expect: Vec<Expect>,
}

/// Reads a script: its commands, each with the expectations that follow it
/// and apply on `machine`.
pub fn parse(script: &str, machine: Machine) -> Result<Vec<Command>> {
    let mut commands: Vec<Command> = Vec::new();
    for (i, raw) in script.lines().enumerate() {
        let line = raw.trim();
        let n = i + 1;
        if line.is_empty() {
            continue;
        }
        let Some(comment) = line.strip_prefix('#') else {
            commands.push(Command {
                line: n,
                text: line.to_string(),
                expect: Vec::new(),
            });
            continue;
        };
        let Some((tag, pattern)) = comment.split_once('>') else {
            continue;
        };
        let (on, never) = match tag {
            "" => (true, false),
            "!" => (true, true),
            "nuc" => (machine == Machine::Nuc, false),
            "qemu" => (machine == Machine::Qemu, false),
            _ => continue,
        };
        let command = commands
            .last_mut()
            .with_context(|| format!("line {n}: an expectation before any command"))?;
        if !on {
            continue;
        }
        let pattern = pattern.strip_prefix(' ').unwrap_or(pattern);
        let whole = || {
            Regex::new(&format!("^(?:{pattern})$")).with_context(|| format!("line {n}: bad regex"))
        };
        command.expect.push(match (never, pattern) {
            (false, "...") => Expect::Any,
            (false, _) => Expect::Line(whole()?),
            (true, _) => Expect::Never(whole()?),
        });
    }
    if commands.is_empty() {
        bail!("the script has no commands");
    }
    Ok(commands)
}

/// How a transcript compares with its script.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub commands: usize,
    pub passed: usize,
    /// One entry per command that printed something else, then one if the
    /// transcript ended early or went astray.
    pub failures: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Compares `transcript` (what `sh` wrote) with `commands`.
pub fn check(commands: &[Command], transcript: &str) -> Report {
    let text = strip_ansi(transcript);
    let lines: Vec<&str> = text.lines().collect();
    let mut report = Report {
        commands: commands.len(),
        passed: 0,
        failures: Vec::new(),
    };
    let mut at = 0;
    for (i, cmd) in commands.iter().enumerate() {
        let trace = format!("+ {}", cmd.text);
        match lines.get(at) {
            None => {
                report.failures.push(missing(commands, i));
                return report;
            }
            Some(l) if *l != trace => {
                report.failures.push(format!(
                    "line {}: expected `{trace}` in the transcript, found `{l}`",
                    cmd.line
                ));
                return report;
            }
            Some(_) => at += 1,
        }
        // The output runs to the next command's trace, or to the end.
        let next = commands.get(i + 1).map(|c| format!("+ {}", c.text));
        let found = next
            .as_ref()
            .and_then(|n| lines[at..].iter().position(|l| l == n));
        let end = found.map_or(lines.len(), |k| at + k);
        match mismatch(&cmd.expect, &lines[at..end]) {
            None => report.passed += 1,
            Some(why) => report
                .failures
                .push(format!("line {}: `{}`: {why}", cmd.line, cmd.text)),
        }
        // Without the next trace, the next command finds the end.
        at = end;
    }
    report
}

/// The failure for command `i` and those after it not being in the
/// transcript (the machine hung or restarted, or the script changed).
fn missing(commands: &[Command], i: usize) -> String {
    let cmd = &commands[i];
    format!(
        "line {}: `{}` and the {} command(s) after it are not in the transcript",
        cmd.line,
        cmd.text,
        commands.len() - i - 1
    )
}

/// Why `output` does not meet `expect`, if it does not.
fn mismatch(expect: &[Expect], output: &[&str]) -> Option<String> {
    for e in expect {
        if let Expect::Never(re) = e
            && let Some(l) = output.iter().find(|l| re.is_match(l))
        {
            return Some(format!("printed `{l}`, which matches /{}/", show(re)));
        }
    }
    let mut lines: Vec<&Expect> = expect
        .iter()
        .filter(|e| !matches!(e, Expect::Never(_)))
        .collect();
    if lines.is_empty() && expect.iter().any(|e| matches!(e, Expect::Never(_))) {
        lines.push(&Expect::Any);
    }
    if matches_lines(&lines, output) {
        return None;
    }
    let wanted: Vec<String> = lines
        .iter()
        .map(|e| match e {
            Expect::Line(re) => format!("/{}/", show(re)),
            _ => String::from("..."),
        })
        .collect();
    let got: Vec<&str> = output.iter().take(12).copied().collect();
    Some(format!(
        "expected [{}], printed [{}]{}",
        wanted.join(", "),
        got.join(" | "),
        if output.len() > got.len() { " …" } else { "" }
    ))
}

/// The pattern as written in the script.
fn show(re: &Regex) -> &str {
    let s = re.as_str();
    &s[4..s.len() - 2]
}

/// Whether the patterns match all of `output`, line by line, `...`
/// matching any number of lines. Dynamic programming over (pattern, line),
/// so many `...` cost no more than one.
fn matches_lines(patterns: &[&Expect], output: &[&str]) -> bool {
    // ok[j]: the patterns so far match output[..j].
    let mut ok = vec![false; output.len() + 1];
    ok[0] = true;
    for p in patterns {
        let mut next = vec![false; output.len() + 1];
        match p {
            Expect::Any => {
                let mut reached = false;
                for j in 0..=output.len() {
                    reached |= ok[j];
                    next[j] = reached;
                }
            }
            Expect::Line(re) => {
                for j in 0..output.len() {
                    next[j + 1] = ok[j] && re.is_match(output[j]);
                }
            }
            Expect::Never(_) => next = ok.clone(),
        }
        ok = next;
    }
    ok[output.len()]
}

````

- [ ] **Step 7: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//!                                   then QEMU's DEVICE_DELETED event)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
````

with:

````rust
//!                                   then QEMU's DEVICE_DELETED event)
//! check-script /root/checks/a.sh   (after poweroff: the script's transcript,
//!                                   read with debugfs, shows what the script
//!                                   expects on QEMU; see checks.rs)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
````

Replace:

````rust
use crate::build;
use crate::image::{self, Layout, Partition, esp_write, set_cmdline};
````

with:

````rust
use crate::build;
use crate::checks;
use crate::image::{self, Layout, Partition, esp_write, set_cmdline};
````

Replace:

````rust
    Unplug,
    /// The file at `path` on the ext2 root is `line` and a newline, again
````

with:

````rust
    Unplug,
    /// The transcript `sh` wrote for the script at this path on the ext2
    /// root shows what the script expects (`checks.rs`). Checked on the
    /// disk once the machine is off.
    CheckScript(String),
    /// The file at `path` on the ext2 root is `line` and a newline, again
````

Replace:

````rust
            "unplug" if rest.is_empty() => Step::Unplug,
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
````

with:

````rust
            "unplug" if rest.is_empty() => Step::Unplug,
            "check-script" if rest.starts_with('/') => Step::CheckScript(rest.to_string()),
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
````

Replace:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    let offline = matches!(step, Step::Timeout(_) | Step::FileLines { .. });
    if r.off && !offline {
````

with:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    let offline = matches!(
        step,
        Step::Timeout(_) | Step::FileLines { .. } | Step::CheckScript(_)
    );
    if r.off && !offline {
````

Replace:

````rust
            r.off = true;
        }
````

with:

````rust
            r.off = true;
        }
        Step::CheckScript(path) => {
            if !r.off {
                bail!("check-script reads the disk: switch the machine off first (poweroff)");
            }
            let read = |p: &str| -> Result<String> {
                let data = image::read_ext2_file(&r.qemu.disk, r.root, p, run_dir)?
                    .with_context(|| format!("{p}: no such file on the disk"))?;
                Ok(String::from_utf8_lossy(&data).into_owned())
            };
            let script = read(path)?;
            let transcript = read(&shell::commands::transcript_name(path))?;
            let report =
                checks::check(&checks::parse(&script, checks::Machine::Qemu)?, &transcript);
            if !report.ok() {
                bail!(
                    "{path}: {} of {} commands as expected\n{}",
                    report.passed,
                    report.commands,
                    report.failures.join("\n")
                );
            }
        }
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 58 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates xtask
git commit -m "xtask: transcripts are checked against the output their script expects"
````


### Task 25: A failed check names the expectation and the line

Review finding (minor): a failed command's report listed all its patterns and the first 12 printed lines; for `dmesg`, with about 45 patterns and the mismatch usually far down, that did not say what was wrong. The matcher now reports the first expectation that cannot match and where: `expected /l30/, printed `oops` (line 30)`, `… on a line from line 2 on, but none matches` after a `...`, `… but nothing more was printed`, or `expected nothing more, printed … (line N)` (decision 4).

**Files:**
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Task 24.
- Produces: `checks::line_mismatch` (private), replacing `matches_lines`.

- [ ] **Step 1: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        assert_eq!(r.failures.len(), 1);
        assert!(
            r.failures[0].starts_with(
                "line 6: `dmesg`: expected [..., /usb: 00:14\\.0 port 15: .*Kingston.*/, ...]"
            ),
            "{}",
            r.failures[0]
        );
````

with:

````rust
        assert_eq!(r.failures.len(), 1);
        assert_eq!(
            r.failures[0],
            "line 6: `dmesg`: expected /usb: 00:14\\.0 port 15: .*Kingston.*/ \
             on a line from line 1 on, but none matches"
        );
````

Replace:

````rust
            [
                "line 2: `mkdir -p /root/n`: expected [], printed [mkdir: cannot create directory '/root/n': Read-only file system]"
            ]
````

with:

````rust
            [
                "line 2: `mkdir -p /root/n`: expected nothing more, printed \
                 `mkdir: cannot create directory '/root/n': Read-only file system` (line 1)"
            ]
````

Replace:

````rust
    #[test]
    fn many_wildcards_stay_fast() {
````

with:

````rust
    #[test]
    fn a_failure_names_the_expectation_and_the_line() {
        // A long output (dmesg) that is wrong at its 30th line.
        let script: String = core::iter::once("dmesg\n".to_string())
            .chain((1..=40).map(|i| format!("#> l{i}\n")))
            .collect();
        let mut log: Vec<String> = (1..=40).map(|i| format!("l{i}")).collect();
        log[29] = "oops".into();
        let c = parse(&script, Machine::Qemu).unwrap();
        let r = check(&c, &format!("+ dmesg\n{}\n", log.join("\n")));
        assert_eq!(
            r.failures,
            ["line 1: `dmesg`: expected /l30/, printed `oops` (line 30)"]
        );
        // After `...`, the line may be anywhere further on.
        let c = parse("dmesg\n#> l1\n#> ...\n#> l99\n", Machine::Qemu).unwrap();
        let r = check(&c, &format!("+ dmesg\n{}\n", log.join("\n")));
        assert_eq!(
            r.failures,
            ["line 1: `dmesg`: expected /l99/ on a line from line 2 on, but none matches"]
        );
        // Output that ends too soon.
        let c = parse("ls\n#> a\n#> b\n", Machine::Qemu).unwrap();
        assert_eq!(
            check(&c, "+ ls\na\n").failures,
            ["line 1: `ls`: expected /b/, but nothing more was printed"]
        );
    }

    #[test]
    fn many_wildcards_stay_fast() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: 3 tests fail, among them `checks::tests::every_output_line_must_be_expected`, `checks::tests::lines_tagged_for_the_other_machine_do_not_count`.

- [ ] **Step 3: Change `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    }
    if matches_lines(&lines, output) {
        return None;
    }
    let wanted: Vec<String> = lines
        .iter()
        .map(|e| match e {
            Expect::Line(re) => format!("/{}/", show(re)),
            _ => String::from("..."),
        })
        .collect();
    let got: Vec<&str> = output.iter().take(12).copied().collect();
    Some(format!(
        "expected [{}], printed [{}]{}",
        wanted.join(", "),
        got.join(" | "),
        if output.len() > got.len() { " …" } else { "" }
    ))
}
````

with:

````rust
    }
    line_mismatch(&lines, output)
}
````

Replace:

````rust
/// Whether the patterns match all of `output`, line by line, `...`
/// matching any number of lines. Dynamic programming over (pattern, line),
/// so many `...` cost no more than one.
fn matches_lines(patterns: &[&Expect], output: &[&str]) -> bool {
    // ok[j]: the patterns so far match output[..j].
````

with:

````rust
/// Whether the patterns match all of `output`, line by line, `...`
/// matching any number of lines; if not, the first pattern that cannot
/// match and where. Dynamic programming over (pattern, line), so many
/// `...` cost no more than one.
fn line_mismatch(patterns: &[&Expect], output: &[&str]) -> Option<String> {
    // ok[j]: the patterns so far match output[..j].
````

Replace:

````rust
        }
        ok = next;
    }
    ok[output.len()]
}
````

with:

````rust
        }
        if let Expect::Line(re) = p
            && !next.contains(&true)
        {
            // The lines where this pattern could have matched.
            let from: Vec<usize> = (0..=output.len()).filter(|&j| ok[j]).collect();
            return Some(match from[..] {
                [j] if j == output.len() => {
                    format!("expected /{}/, but nothing more was printed", show(re))
                }
                [j] => format!(
                    "expected /{}/, printed `{}` (line {})",
                    show(re),
                    output[j],
                    j + 1
                ),
                _ => format!(
                    "expected /{}/ on a line from line {} on, but none matches",
                    show(re),
                    from[0] + 1
                ),
            });
        }
        ok = next;
    }
    let last = (0..=output.len()).rev().find(|&j| ok[j])?;
    (last < output.len()).then(|| {
        format!(
            "expected nothing more, printed `{}` (line {})",
            output[last],
            last + 1
        )
    })
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 59 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "xtask: a failed check names the expectation that failed and the line it failed on"
````


### Task 26: NUC check 3 as two scripts, run in QEMU by the `checks` scenario

Decision 4. `rootfs/root/checks/check3-a.sh` (before the restart) and `check3-b.sh` (after it) are NUC check 3's steps as scripts, which `image` and `flash --full` copy to `/root/checks/`. Part 1 starts by removing `/root/notes`, so it can be run again, then checks `uname -a`, `date` and every startup line through `dmesg` (the NUC's values as `#nuc>` lines: 1920x1200, 15948 MiB, 30 ACPI tables, S5 7/0, the TSC, 24 PCI devices, 32-byte contexts and 34 scratchpads, the Kingston's INQUIRY and size, the four devices and the root), runs the `fileops` scenario's operations on `/root/notes`, builds an 8 MiB file by doubling as `bigfile` does (each step writes up to 4 MiB to the stick and syncs it; so the final check shows whether the 5 s bulk timeout suffices on the Kingston), copies it, and ends with `df`. Part 2 reads the files back after the restart. The scenario `checks` runs both, with a `reboot` between them, and checks their transcripts from the disk. `/root` now also holds `checks`, which the `fileops` scenario's `ls` sees.

**Files:**
- Create: `rootfs/root/checks/check3-a.sh`
- Create: `rootfs/root/checks/check3-b.sh`
- Create: `tests/e2e/checks.txt`
- Modify: `tests/e2e/fileops.txt`

**Interfaces:**
- Consumes: Tasks 19, 21 and 24.
- Produces: `rootfs/root/checks/check3-a.sh`, `rootfs/root/checks/check3-b.sh`; scenario `tests/e2e/checks.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/checks.txt`**

Create `tests/e2e/checks.txt`:

````text
# NUC check 3's scripts (rootfs/root/checks/, spec §15 item 12) run in
# QEMU as they do on the NUC: part 1, a reboot, part 2. Afterwards their
# transcripts on the disk must show what the scripts expect.
timeout 30
expect root@relay:~# $
send sh checks/check3-a.sh
expect \n\+ rm -rf /root/notes\n
timeout 120
expect \n\+ df\n
expect root@relay:~# $
reboot
expect root@relay:~# $
send sh checks/check3-b.sh
expect \n\+ dmesg\n
expect root@relay:~# $
poweroff
check-script /root/checks/check3-a.sh
check-script /root/checks/check3-b.sh
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/fileops.txt`**

In `tests/e2e/fileops.txt`, replace:

````text
send ls
expect \nREADME\n
# Error messages follow GNU coreutils.
````

with:

````text
send ls
expect \nREADME  checks\n
# Error messages follow GNU coreutils.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: FAIL: scenario `checks` stops at line 7, timed out waiting for `\n\+ rm -rf /root/notes\n`.

- [ ] **Step 4: Create `rootfs/root/checks/check3-a.sh`**

Create `rootfs/root/checks/check3-a.sh`:

````bash
# NUC check 3, part 1 (docs/hardware-test.md): `sh checks/check3-a.sh`,
# then `reboot` and `sh checks/check3-b.sh`. What each command prints goes
# into check3-a.log; in Linux Mint `cargo xtask verify-usb` checks it
# against the `#>` lines below. The `checks` QEMU scenario runs this file too.
#
# Under a command: `#> regex` is one whole line of its output, in order;
# `#> ...` any number of lines; `#nuc>` and `#qemu>` apply only on that
# machine; `#!> regex` must match no line. A command with no `#>` line
# must print nothing.

# Start afresh, so the check can be run again.
rm -rf /root/notes

uname -a
#> Relay relay 0\.1\.0 x86_64
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d

# Every startup line is [ ok ]; on the NUC, the values of checks 1-3.
dmesg
#> ...
#> \[ ok \] console \d+x\d+ \(\d+x\d+ cells\)
#nuc> \[ ok \] console 1920x1200 \(120x33 cells\)
#> \[ ok \] cpu tables
#> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
#nuc> \[ ok \] boot info: 159\d\d MiB usable in \d+ regions, cmdline ''
#> \[ ok \] memory: \d+ MiB free of \d+ MiB, heap 32 MiB
#> ...
#> \[ ok \] acpi: .*
#nuc> \[ ok \] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
#> \[ ok \] timer: .*
#nuc> \[ ok \] timer: TSC 2496\.000 MHz \(CPUID 0x15\), 1000 Hz tick \(xAPIC\)
#> \[ ok \] rtc: 20\d\d-\d\d-\d\d \d\d:\d\d:\d\d UTC
#> ...
#> \[ ok \] pci: .*
#nuc> \[ ok \] pci: 24 devices on buses 00 01 72, xHCI at 00:0d\.0 00:14\.0
#> ...
#nuc> usb: 00:14\.0 xHCI 1\.20, 16 ports \(12 USB 2, 4 USB 3\), 32-byte contexts, 34 scratchpads
#nuc> ...
#nuc> storage: slot \d+: vendor "Kingston", product "DataTraveler 3\.0", revision "PMAP", removable
#nuc> storage: slot \d+: 30277632 blocks of 512 bytes
#nuc> ...
#nuc> usb: 00:14\.0 port 1: 046d:c534 full-speed, keyboard
#nuc> usb: 00:14\.0 port 3: 046d:c31c low-speed, keyboard
#nuc> usb: 00:14\.0 port 10: 8087:0033 full-speed, not claimed
#nuc> usb: 00:14\.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3\.0, 14\.4 GiB
#> \[ ok \] usb: .*
#nuc> \[ ok \] usb: 2 controllers, 4 devices
#> \[ ok \] keyboard: .*
#nuc> \[ ok \] keyboard: 2 keyboards
#> ...
#nuc> storage: root on 00:14\.0 port 15, the disk with the boot partition [0-9A-F-]{36}
#> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
#> ...
#!> \[FAIL\].*

# The file operations of the fileops scenario.
mkdir -p /root/notes/old
echo remember me > /root/notes/a
echo and me >> /root/notes/a
cat /root/notes/a
#> remember me
#> and me
ls -l /root/notes
#> total 8
#> -rw-r--r-- 1 root root   19 \w{3} [ \d]\d \d\d:\d\d a
#> drwxr-xr-x 2 root root 4096 \w{3} [ \d]\d \d\d:\d\d old
cp /root/notes/a /root/notes/b
mv /root/notes/b /root/notes/old/c
ls /root/notes /root/notes/old
#> /root/notes:
#> a  old
#> 
#> /root/notes/old:
#> c
rmdir /root/notes/old
#> rmdir: failed to remove '/root/notes/old': Directory not empty
rm -r /root/notes/old
ls /root/notes
#> a
touch /root/notes/t
stat /root/notes/t
#>   File: /root/notes/t
#>   Size: 0 .*regular empty file
#> ...
head -n 1 /root/notes/a
#> remember me
tail -n 1 /root/notes/a
#> and me
wc /root/notes/a
#>  2  4 19 /root/notes/a
cat /root/nope
#> cat: /root/nope: No such file or directory
rm -r /
#> rm: it is dangerous to operate recursively on '/'

# An 8 MiB file (double-indirect blocks), built by doubling as in the
# bigfile scenario: each step writes and syncs up to 4 MiB on the stick.
echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
cat /root/notes/big > /root/notes/tmp
cat /root/notes/tmp >> /root/notes/big
wc /root/notes/big
#> \s*131072\s+131072\s+8388608 /root/notes/big
cp /root/notes/big /root/notes/copy
wc /root/notes/copy
#> \s*131072\s+131072\s+8388608 /root/notes/copy
rm /root/notes/tmp /root/notes/copy
ls /root/notes
#> a  big  t
df
#> Filesystem +1K-blocks +Used +Available +Use% Mounted on
#> /dev/root +\d+ +\d+ +\d+ +\d+% /
#nuc> /dev/root +1[45]\d{6} +\d+ +\d+ +\d+% /
````

- [ ] **Step 5: Create `rootfs/root/checks/check3-b.sh`**

Create `rootfs/root/checks/check3-b.sh`:

````bash
# NUC check 3, part 2 (docs/hardware-test.md): after check3-a.sh and a
# `reboot`, `sh checks/check3-b.sh`, then `poweroff`. Its transcript is
# check3-b.log; the `#>` lines are explained in check3-a.sh.

# The files written before the restart are still there.
cat /root/notes/a
#> remember me
#> and me
ls /root/notes
#> a  big  t
wc /root/notes/big
#> \s*131072\s+131072\s+8388608 /root/notes/big
tail -n 1 /root/notes/big
#> 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde

# The machine started again as before.
dmesg
#> ...
#> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
#> ...
#!> \[FAIL\].*
````

- [ ] **Step 6: Run the `checks`, `fileops` scenarios**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add rootfs tests
git commit -m "rootfs, tests: NUC check 3 as two scripts in /root/checks, run in QEMU by the checks scenario"
````


### Task 27: The check scripts pass on a NUC transcript

Review finding (critical): check 3 would have failed on a correct NUC. The scripts wrote each NUC fact as a generic `#>` line followed by a `#nuc>` line, but the checker takes every applying line as the next output line, so on the NUC each pair needed two lines where the machine prints one (`dmesg` and `df` in part 1, `dmesg` in part 2), while QEMU, which skips `#nuc>` lines, passed. The generic line of each pair becomes `#qemu>`, and the scripts' header says so (decision 4). A unit test now checks the real scripts against a transcript of each machine: QEMU's as the `checks` scenario wrote it, and the NUC's with the startup lines checks 1–3 recorded, in the kernel's order, under `xtask/fixtures/checks/`; QEMU's transcript must fail as the NUC's.

**Files:**
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `rootfs/root/checks/check3-b.sh`
- Create: `xtask/fixtures/checks/check3-a.nuc.log`
- Create: `xtask/fixtures/checks/check3-a.qemu.log`
- Create: `xtask/fixtures/checks/check3-b.nuc.log`
- Create: `xtask/fixtures/checks/check3-b.qemu.log`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Tasks 24 and 26.
- Produces: `xtask/fixtures/checks/check3-{a,b}.{qemu,nuc}.log`.

- [ ] **Step 1: Create `xtask/fixtures/checks/check3-a.nuc.log`**

Create `xtask/fixtures/checks/check3-a.nuc.log`:

````text
+ rm -rf /root/notes
+ uname -a
Relay relay 0.1.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1200 (120x33 cells)
[ ok ] cpu tables
[ ok ] boot info: 15948 MiB usable in 32 regions, cmdline ''
[ ok ] memory: 15915 MiB free of 15948 MiB, heap 32 MiB
acpi: tables FACP ...
[ ok ] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
[ ok ] timer: TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)
[ ok ] rtc: 2026-09-28 14:09:17 UTC
pci: 00:0d.0 8086:461e 0c0330 USB xHCI, bar0 mem64 0x603d190000 64K
[ ok ] pci: 24 devices on buses 00 01 72, xHCI at 00:0d.0 00:14.0
xhci 00:0d.0: running
usb: 00:0d.0 xHCI 1.20, 4 ports (1 USB 2, 3 USB 3), 32-byte contexts, 34 scratchpads
xhci 00:14.0: running
usb: 00:14.0 xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 32-byte contexts, 34 scratchpads
xhci 00:14.0: port 15: connection stable after 100 ms
storage: slot 4: vendor "Kingston", product "DataTraveler 3.0", revision "PMAP", removable
storage: slot 4: 30277632 blocks of 512 bytes
xhci 00:14.0: port 10: connection stable after 100 ms
usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard
usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard
usb: 00:14.0 port 10: 8087:0033 full-speed, not claimed
usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB
[ ok ] usb: 2 controllers, 4 devices
[ ok ] keyboard: 2 keyboards
storage: 00:14.0 port 15: GPT with 2 partitions
storage: root on 00:14.0 port 15, the disk with the boot partition 4EBD57DE-8DBB-4D34-B3B5-D18607581E70
[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB
+ mkdir -p /root/notes/old
+ echo remember me > /root/notes/a
+ echo and me >> /root/notes/a
+ cat /root/notes/a
remember me
and me
+ ls -l /root/notes
total 8
-rw-r--r-- 1 root root   19 Sep 28 14:09 a
drwxr-xr-x 2 root root 4096 Sep 28 14:09 old
+ cp /root/notes/a /root/notes/b
+ mv /root/notes/b /root/notes/old/c
+ ls /root/notes /root/notes/old
/root/notes:
a  old

/root/notes/old:
c
+ rmdir /root/notes/old
rmdir: failed to remove '/root/notes/old': Directory not empty
+ rm -r /root/notes/old
+ ls /root/notes
a
+ touch /root/notes/t
+ stat /root/notes/t
  File: /root/notes/t
  Size: 0         	Blocks: 0          IO Block: 4096   regular empty file
Inode: 24322       Links: 1
Access: (0644/-rw-r--r--)  Uid: (    0/    root)   Gid: (    0/    root)
Access: 2026-09-28 14:09:17.000000000 +0000
Modify: 2026-09-28 14:09:17.000000000 +0000
Change: 2026-09-28 14:09:17.000000000 +0000
 Birth: -
+ head -n 1 /root/notes/a
remember me
+ tail -n 1 /root/notes/a
and me
+ wc /root/notes/a
 2  4 19 /root/notes/a
+ cat /root/nope
cat: /root/nope: No such file or directory
+ rm -r /
rm: it is dangerous to operate recursively on '/'
+ echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ wc /root/notes/big
 131072  131072 8388608 /root/notes/big
+ cp /root/notes/big /root/notes/copy
+ wc /root/notes/copy
 131072  131072 8388608 /root/notes/copy
+ rm /root/notes/tmp /root/notes/copy
+ ls /root/notes
a  big  t
+ df
Filesystem     1K-blocks Used Available Use% Mounted on
/dev/root       14877000 8300  14100000   1% /
````

- [ ] **Step 2: Create `xtask/fixtures/checks/check3-a.qemu.log`**

Create `xtask/fixtures/checks/check3-a.qemu.log`:

````text
+ rm -rf /root/notes
+ uname -a
Relay relay 0.1.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1080 (120x33 cells)
[ ok ] cpu tables
[ ok ] boot info: 1008 MiB usable in 25 regions, cmdline 'test=1'
[ ok ] memory: 976 MiB free of 1008 MiB, heap 32 MiB
acpi: tables FACP APIC HPET MCFG WAET BGRT
acpi: FADT reset io 0xcf9 <- 0xf, PM1a_CNT io 0x604, PM1b_CNT none, century register 0x32
[ ok ] acpi: 6 tables, ECAM 0xe0000000 buses 0-255, HPET 0xfed00000, S5 0/0
[ ok ] timer: TSC 2495.896 MHz (HPET), 1000 Hz tick (xAPIC)
[ ok ] rtc: 2026-09-28 14:09:17 UTC
pci: 00:00.0 8086:29c0 060000 host bridge
pci: 00:01.0 1234:1111 030000 display, bar0 mem32 0x80000000 16M pf, bar2 mem32 0x81011000 4K
pci: 00:02.0 1b36:000d 0c0330 USB xHCI, bar0 mem64 0x7000000000 16K
pci: 00:1f.0 8086:2918 060100 ISA bridge
pci: 00:1f.2 8086:2922 010601 SATA, bar4 io 0x6040 32, bar5 mem32 0x81010000 4K
pci: 00:1f.3 8086:2930 0c0500 SMBus, bar4 io 0x6000 64
[ ok ] pci: 6 devices on bus 00, xHCI at 00:02.0
xhci 00:02.0: xHCI 1.00, 64 slots, 8 ports, 16 interrupters, 32-byte contexts, page size 4 KiB
xhci 00:02.0: no legacy support capability
xhci 00:02.0: already halted
xhci 00:02.0: reset after 1 ms
xhci 00:02.0: USB 2.0 ports 5-8, USB 3.0 ports 1-4
xhci 00:02.0: no scratchpad buffers
xhci 00:02.0: DCBAA 0x119000, command ring 0x11a000, event ring 0x11b000
xhci 00:02.0: running
xhci 00:02.0: ports settled after 98 ms
usb: 00:02.0 xHCI 1.00, 8 ports (4 USB 2, 4 USB 3), 32-byte contexts, 0 scratchpads
xhci 00:02.0: port 2: connected, PORTSC 0x00021203
xhci 00:02.0: port 5: connected, PORTSC 0x00020ee1
xhci 00:02.0: port 2: connection stable after 100 ms
xhci 00:02.0: port 2: reset done, SuperSpeed
xhci 00:02.0: port 2: slot 1 at address 1, 46f4:0001 USB 3.00, ep0 512 bytes, 1 configuration
xhci 00:02.0: slot 1: interface 0 class 8/6/80, endpoints 0x81 bulk 1024 bytes interval 0, 0x02 bulk 1024 bytes interval 0
xhci 00:02.0: slot 1: endpoint 0x81 configured (DCI 3, interval exponent 0)
xhci 00:02.0: slot 1: endpoint 0x02 configured (DCI 4, interval exponent 0)
storage: slot 1: vendor "QEMU", product "QEMU HARDDISK", revision "2.5+"
storage: slot 1: 524288 blocks of 512 bytes
xhci 00:02.0: port 5: connection stable after 100 ms
xhci 00:02.0: port 5: reset done, high-speed
xhci 00:02.0: port 5: slot 2 at address 2, 0627:0001 USB 2.00, ep0 64 bytes, 1 configuration
xhci 00:02.0: slot 2: interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 7
xhci 00:02.0: slot 2: endpoint 0x81 configured (DCI 3, interval exponent 6)
usb: 00:02.0 port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB
usb: 00:02.0 port 5: 0627:0001 high-speed, keyboard
[ ok ] usb: 1 controller, 2 devices
[ ok ] keyboard: 1 keyboard
storage: 00:02.0 port 2: GPT with 2 partitions
storage: root on 00:02.0 port 2, the disk with the boot partition 52454C41-5900-4000-8000-000000000002
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
+ mkdir -p /root/notes/old
+ echo remember me > /root/notes/a
+ echo and me >> /root/notes/a
+ cat /root/notes/a
remember me
and me
+ ls -l /root/notes
total 8
-rw-r--r-- 1 root root   19 Sep 28 14:09 a
drwxr-xr-x 2 root root 4096 Sep 28 14:09 old
+ cp /root/notes/a /root/notes/b
+ mv /root/notes/b /root/notes/old/c
+ ls /root/notes /root/notes/old
/root/notes:
a  old

/root/notes/old:
c
+ rmdir /root/notes/old
rmdir: failed to remove '/root/notes/old': Directory not empty
+ rm -r /root/notes/old
+ ls /root/notes
a
+ touch /root/notes/t
+ stat /root/notes/t
  File: /root/notes/t
  Size: 0         	Blocks: 0          IO Block: 4096   regular empty file
Inode: 24322       Links: 1
Access: (0644/-rw-r--r--)  Uid: (    0/    root)   Gid: (    0/    root)
Access: 2026-09-28 14:09:17.000000000 +0000
Modify: 2026-09-28 14:09:17.000000000 +0000
Change: 2026-09-28 14:09:17.000000000 +0000
 Birth: -
+ head -n 1 /root/notes/a
remember me
+ tail -n 1 /root/notes/a
and me
+ wc /root/notes/a
 2  4 19 /root/notes/a
+ cat /root/nope
cat: /root/nope: No such file or directory
+ rm -r /
rm: it is dangerous to operate recursively on '/'
+ echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ cat /root/notes/big > /root/notes/tmp
+ cat /root/notes/tmp >> /root/notes/big
+ wc /root/notes/big
 131072  131072 8388608 /root/notes/big
+ cp /root/notes/big /root/notes/copy
+ wc /root/notes/copy
 131072  131072 8388608 /root/notes/copy
+ rm /root/notes/tmp /root/notes/copy
+ ls /root/notes
a  big  t
+ df
Filesystem     1K-blocks Used Available Use% Mounted on
/dev/root         182368 8300    164340   5% /
````

- [ ] **Step 3: Create `xtask/fixtures/checks/check3-b.nuc.log`**

Create `xtask/fixtures/checks/check3-b.nuc.log`:

````text
+ cat /root/notes/a
remember me
and me
+ ls /root/notes
a  big  t
+ wc /root/notes/big
 131072  131072 8388608 /root/notes/big
+ tail -n 1 /root/notes/big
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1200 (120x33 cells)
[ ok ] cpu tables
[ ok ] boot info: 15948 MiB usable in 32 regions, cmdline ''
[ ok ] memory: 15915 MiB free of 15948 MiB, heap 32 MiB
acpi: tables FACP ...
[ ok ] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
[ ok ] timer: TSC 2496.000 MHz (CPUID 0x15), 1000 Hz tick (xAPIC)
[ ok ] rtc: 2026-09-28 14:09:17 UTC
pci: 00:0d.0 8086:461e 0c0330 USB xHCI, bar0 mem64 0x603d190000 64K
[ ok ] pci: 24 devices on buses 00 01 72, xHCI at 00:0d.0 00:14.0
xhci 00:0d.0: running
usb: 00:0d.0 xHCI 1.20, 4 ports (1 USB 2, 3 USB 3), 32-byte contexts, 34 scratchpads
xhci 00:14.0: running
usb: 00:14.0 xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 32-byte contexts, 34 scratchpads
xhci 00:14.0: port 15: connection stable after 100 ms
storage: slot 4: vendor "Kingston", product "DataTraveler 3.0", revision "PMAP", removable
storage: slot 4: 30277632 blocks of 512 bytes
xhci 00:14.0: port 10: connection stable after 100 ms
usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard
usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard
usb: 00:14.0 port 10: 8087:0033 full-speed, not claimed
usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB
[ ok ] usb: 2 controllers, 4 devices
[ ok ] keyboard: 2 keyboards
storage: 00:14.0 port 15: GPT with 2 partitions
storage: root on 00:14.0 port 15, the disk with the boot partition 4EBD57DE-8DBB-4D34-B3B5-D18607581E70
[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB
````

- [ ] **Step 4: Create `xtask/fixtures/checks/check3-b.qemu.log`**

Create `xtask/fixtures/checks/check3-b.qemu.log`:

````text
+ cat /root/notes/a
remember me
and me
+ ls /root/notes
a  big  t
+ wc /root/notes/big
 131072  131072 8388608 /root/notes/big
+ tail -n 1 /root/notes/big
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1080 (120x33 cells)
[ ok ] cpu tables
[ ok ] boot info: 1008 MiB usable in 25 regions, cmdline 'test=1'
[ ok ] memory: 976 MiB free of 1008 MiB, heap 32 MiB
acpi: tables FACP APIC HPET MCFG WAET BGRT
acpi: FADT reset io 0xcf9 <- 0xf, PM1a_CNT io 0x604, PM1b_CNT none, century register 0x32
[ ok ] acpi: 6 tables, ECAM 0xe0000000 buses 0-255, HPET 0xfed00000, S5 0/0
[ ok ] timer: TSC 2495.892 MHz (HPET), 1000 Hz tick (xAPIC)
[ ok ] rtc: 2026-09-28 14:09:18 UTC
pci: 00:00.0 8086:29c0 060000 host bridge
pci: 00:01.0 1234:1111 030000 display, bar0 mem32 0x80000000 16M pf, bar2 mem32 0x81011000 4K
pci: 00:02.0 1b36:000d 0c0330 USB xHCI, bar0 mem64 0x7000000000 16K
pci: 00:1f.0 8086:2918 060100 ISA bridge
pci: 00:1f.2 8086:2922 010601 SATA, bar4 io 0x6040 32, bar5 mem32 0x81010000 4K
pci: 00:1f.3 8086:2930 0c0500 SMBus, bar4 io 0x6000 64
[ ok ] pci: 6 devices on bus 00, xHCI at 00:02.0
xhci 00:02.0: xHCI 1.00, 64 slots, 8 ports, 16 interrupters, 32-byte contexts, page size 4 KiB
xhci 00:02.0: no legacy support capability
xhci 00:02.0: already halted
xhci 00:02.0: reset after 1 ms
xhci 00:02.0: USB 2.0 ports 5-8, USB 3.0 ports 1-4
xhci 00:02.0: no scratchpad buffers
xhci 00:02.0: DCBAA 0x119000, command ring 0x11a000, event ring 0x11b000
xhci 00:02.0: running
xhci 00:02.0: ports settled after 98 ms
usb: 00:02.0 xHCI 1.00, 8 ports (4 USB 2, 4 USB 3), 32-byte contexts, 0 scratchpads
xhci 00:02.0: port 2: connected, PORTSC 0x00021203
xhci 00:02.0: port 5: connected, PORTSC 0x00020ee1
xhci 00:02.0: port 2: connection stable after 100 ms
xhci 00:02.0: port 2: reset done, SuperSpeed
xhci 00:02.0: port 2: slot 1 at address 1, 46f4:0001 USB 3.00, ep0 512 bytes, 1 configuration
xhci 00:02.0: slot 1: interface 0 class 8/6/80, endpoints 0x81 bulk 1024 bytes interval 0, 0x02 bulk 1024 bytes interval 0
xhci 00:02.0: slot 1: endpoint 0x81 configured (DCI 3, interval exponent 0)
xhci 00:02.0: slot 1: endpoint 0x02 configured (DCI 4, interval exponent 0)
storage: slot 1: vendor "QEMU", product "QEMU HARDDISK", revision "2.5+"
storage: slot 1: 524288 blocks of 512 bytes
xhci 00:02.0: port 5: connection stable after 100 ms
xhci 00:02.0: port 5: reset done, high-speed
xhci 00:02.0: port 5: slot 2 at address 2, 0627:0001 USB 2.00, ep0 64 bytes, 1 configuration
xhci 00:02.0: slot 2: interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 7
xhci 00:02.0: slot 2: endpoint 0x81 configured (DCI 3, interval exponent 6)
usb: 00:02.0 port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB
usb: 00:02.0 port 5: 0627:0001 high-speed, keyboard
[ ok ] usb: 1 controller, 2 devices
[ ok ] keyboard: 1 keyboard
storage: 00:02.0 port 2: GPT with 2 partitions
storage: root on 00:02.0 port 2, the disk with the boot partition 52454C41-5900-4000-8000-000000000002
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
````

- [ ] **Step 5: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
    }

    #[test]
    fn many_wildcards_stay_fast() {
````

with:

````rust
    }

    /// The real check scripts against a transcript of each machine: QEMU's
    /// from the `checks` scenario, the NUC's with its startup lines as
    /// checks 1–3 recorded them (`docs/hardware-test.md`), in the kernel's
    /// order. A `#nuc>` line must not need a line of its own next to the
    /// `#>` line for the same output, which QEMU alone cannot show.
    #[test]
    fn the_check_scripts_pass_on_both_machines() {
        let parts = [
            (
                include_str!("../../rootfs/root/checks/check3-a.sh"),
                include_str!("../fixtures/checks/check3-a.qemu.log"),
                include_str!("../fixtures/checks/check3-a.nuc.log"),
            ),
            (
                include_str!("../../rootfs/root/checks/check3-b.sh"),
                include_str!("../fixtures/checks/check3-b.qemu.log"),
                include_str!("../fixtures/checks/check3-b.nuc.log"),
            ),
        ];
        for (i, (script, qemu, nuc)) in parts.iter().enumerate() {
            for (machine, log) in [(Machine::Qemu, qemu), (Machine::Nuc, nuc)] {
                let r = check(&parse(script, machine).unwrap(), log);
                assert!(r.ok(), "part {i} on {machine:?}: {:?}", r.failures);
            }
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's.
            let r = check(&parse(script, Machine::Nuc).unwrap(), qemu);
            assert!(!r.ok(), "part {i}");
        }
    }

    #[test]
    fn many_wildcards_stay_fast() {
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 7: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, make these 4 replacements, top to bottom:

Replace:

````bash
# `#> ...` any number of lines; `#nuc>` and `#qemu>` apply only on that
# machine; `#!> regex` must match no line. A command with no `#>` line
# must print nothing.
````

with:

````bash
# `#> ...` any number of lines; `#nuc>` and `#qemu>` apply only on that
# machine, so a line whose text differs is written twice, `#qemu>` then
# `#nuc>`; `#!> regex` must match no line. A command with no `#>` line
# must print nothing.
````

Replace:

````bash
#> ...
#> \[ ok \] console \d+x\d+ \(\d+x\d+ cells\)
#nuc> \[ ok \] console 1920x1200 \(120x33 cells\)
#> \[ ok \] cpu tables
#> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
#nuc> \[ ok \] boot info: 159\d\d MiB usable in \d+ regions, cmdline ''
#> \[ ok \] memory: \d+ MiB free of \d+ MiB, heap 32 MiB
#> ...
#> \[ ok \] acpi: .*
#nuc> \[ ok \] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
#> \[ ok \] timer: .*
#nuc> \[ ok \] timer: TSC 2496\.000 MHz \(CPUID 0x15\), 1000 Hz tick \(xAPIC\)
#> \[ ok \] rtc: 20\d\d-\d\d-\d\d \d\d:\d\d:\d\d UTC
#> ...
#> \[ ok \] pci: .*
#nuc> \[ ok \] pci: 24 devices on buses 00 01 72, xHCI at 00:0d\.0 00:14\.0
````

with:

````bash
#> ...
#qemu> \[ ok \] console \d+x\d+ \(\d+x\d+ cells\)
#nuc> \[ ok \] console 1920x1200 \(120x33 cells\)
#> \[ ok \] cpu tables
#qemu> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
#nuc> \[ ok \] boot info: 159\d\d MiB usable in \d+ regions, cmdline ''
#> \[ ok \] memory: \d+ MiB free of \d+ MiB, heap 32 MiB
#> ...
#qemu> \[ ok \] acpi: .*
#nuc> \[ ok \] acpi: 30 tables, ECAM 0xc0000000 buses 0-255, HPET 0xfed00000, S5 7/0
#qemu> \[ ok \] timer: .*
#nuc> \[ ok \] timer: TSC 2496\.000 MHz \(CPUID 0x15\), 1000 Hz tick \(xAPIC\)
#> \[ ok \] rtc: 20\d\d-\d\d-\d\d \d\d:\d\d:\d\d UTC
#> ...
#qemu> \[ ok \] pci: .*
#nuc> \[ ok \] pci: 24 devices on buses 00 01 72, xHCI at 00:0d\.0 00:14\.0
````

Replace:

````bash
#nuc> usb: 00:14\.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3\.0, 14\.4 GiB
#> \[ ok \] usb: .*
#nuc> \[ ok \] usb: 2 controllers, 4 devices
#> \[ ok \] keyboard: .*
#nuc> \[ ok \] keyboard: 2 keyboards
#> ...
#nuc> storage: root on 00:14\.0 port 15, the disk with the boot partition [0-9A-F-]{36}
#> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
````

with:

````bash
#nuc> usb: 00:14\.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3\.0, 14\.4 GiB
#qemu> \[ ok \] usb: .*
#nuc> \[ ok \] usb: 2 controllers, 4 devices
#qemu> \[ ok \] keyboard: .*
#nuc> \[ ok \] keyboard: 2 keyboards
#> ...
#nuc> storage: root on 00:14\.0 port 15, the disk with the boot partition [0-9A-F-]{36}
#qemu> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
````

Replace:

````bash
#> Filesystem +1K-blocks +Used +Available +Use% Mounted on
#> /dev/root +\d+ +\d+ +\d+ +\d+% /
#nuc> /dev/root +1[45]\d{6} +\d+ +\d+ +\d+% /
````

with:

````bash
#> Filesystem +1K-blocks +Used +Available +Use% Mounted on
#qemu> /dev/root +\d+ +\d+ +\d+ +\d+% /
#nuc> /dev/root +1[45]\d{6} +\d+ +\d+ +\d+% /
````

- [ ] **Step 8: Change `rootfs/root/checks/check3-b.sh`**

In `rootfs/root/checks/check3-b.sh`, replace:

````bash
#> ...
#> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
````

with:

````bash
#> ...
#qemu> \[ ok \] mount /: ext2 on .*
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 60 tests.

- [ ] **Step 10: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add rootfs xtask
git commit -m "rootfs, xtask: the check scripts pass on a NUC transcript, not only on QEMU's"
````


### Task 28: `verify-usb` checks the transcripts of the check scripts

Decision 4: back in Linux Mint, one command checks the whole run. After `e2fsck` and the tree, `verify-usb` reads every `*.sh` in `/root/checks` on the stick with `debugfs`, checks its transcript as written on the NUC, prints `<script>: ok, N of N commands as expected` or `FAILED` with the failures, or `not run` for a script without a transcript, and fails if any transcript does not match. The listing of `debugfs ls -p` moves into `list_dir`, shared with `list_tree`.

**Files:**
- Modify: `xtask/src/flash.rs`

**Interfaces:**
- Consumes: Task 24; plan 1's `flash::{verify_usb, list_tree}`.
- Produces: `flash::{CHECKS_DIR = "/root/checks", check_transcripts(target: &Path, root: Partition, Machine, scratch: &Path) -> Result<(Vec<String>, bool)>}`.

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, replace:

````rust
    use super::*;

````

with:

````rust
    use super::*;
    use crate::image::Partition;

    /// An ext2 image holding `files` (path, contents) under its root.
    fn ext2_with(dir: &Path, files: &[(&str, &str)]) -> (PathBuf, Partition) {
        let _ = fs::remove_dir_all(dir);
        for (path, text) in files {
            let p = dir.join("staging").join(path.trim_start_matches('/'));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        fs::create_dir_all(dir.join("staging")).unwrap();
        let img = dir.join("fs.img");
        run(Command::new("mke2fs")
            .args(["-q", "-F", "-t", "ext2", "-d"])
            .arg(dir.join("staging"))
            .arg(&img)
            .arg("1M"))
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        (img, whole)
    }

    #[test]
    fn transcripts_of_the_check_scripts_are_checked_as_on_the_nuc() {
        let dir = out_dir().join("verify-usb-selftest");
        let (img, root) = ext2_with(
            &dir,
            &[
                (
                    "/root/checks/a.sh",
                    "uname\n#> Relay\ndmesg\n#nuc> .*Kingston.*\n#qemu> .*QEMU.*\n",
                ),
                (
                    "/root/checks/a.log",
                    "+ uname\nRelay\n+ dmesg\ndisk Kingston DataTraveler 3.0\n",
                ),
                ("/root/checks/b.sh", "cat /root/notes/a\n#> remember me\n"),
                ("/root/checks/b.log", "+ cat /root/notes/a\nforgotten\n"),
                ("/root/checks/c.sh", "ls\n"),
                ("/root/checks/notes.txt", "not a script"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
                "/root/checks/c.sh: not run (no /root/checks/c.log)",
            ]
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir).unwrap();
        assert!(!ok);
        assert!(
            lines[0].starts_with("/root/checks/a.sh: FAILED, 1 of 2"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_stick_without_check_scripts_passes() {
        let dir = out_dir().join("verify-usb-selftest-empty");
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(ok);
        assert_eq!(lines, ["no check scripts in /root/checks"]);
    }

````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find type `Machine` in this scope ``; `` cannot find function `check_transcripts` in this scope ``.

- [ ] **Step 3: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use crate::build::Artifacts;
use crate::config::{USB_BY_ID, USB_MAX_BYTES, USB_SERIAL};
use crate::image::{self, Layout, e2fs_target};
use crate::util::{out_dir, run, run_stdout};
````

with:

````rust
use crate::build::Artifacts;
use crate::checks::{self, Machine};
use crate::config::{USB_BY_ID, USB_MAX_BYTES, USB_SERIAL};
use crate::image::{self, Layout, Partition, e2fs_target};
use crate::util::{out_dir, run, run_stdout};
````

Replace:

````rust

/// Recursively lists an ext2 tree with debugfs (`ls -p` output is
/// `/inode/mode/uid/gid/name/size/`).
fn list_tree(target: &str, dir: &str, out: &mut Vec<String>) -> Result<()> {
    let text = run_stdout(
````

with:

````rust

/// One entry of an ext2 directory, as debugfs lists it.
struct Entry {
    name: String,
    mode: u32,
    uid: String,
    gid: String,
    size: String,
}

/// The entries of `dir` with debugfs (`ls -p` output is
/// `/inode/mode/uid/gid/name/size/`), without `.`, `..` and `lost+found`.
/// A missing directory has none (debugfs reports it on stderr).
fn list_dir(target: &str, dir: &str) -> Result<Vec<Entry>> {
    let text = run_stdout(
````

Replace:

````rust
    )?;
    for line in text.lines() {
        let f: Vec<&str> = line.split('/').collect();
        if f.len() < 7 || f[5] == "." || f[5] == ".." || f[5] == "lost+found" {
            continue;
        }
        let mode = u32::from_str_radix(f[2], 8).unwrap_or(0);
        let path = if dir == "/" {
            format!("/{}", f[5])
        } else {
            format!("{dir}/{}", f[5])
        };
        let is_dir = mode & 0o170000 == 0o040000;
        out.push(format!(
            "{:06o} {:>4}:{:<4} {:>10}  {path}{}",
            mode,
            f[3],
            f[4],
            f[6],
            if is_dir { "/" } else { "" }
````

with:

````rust
    )?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('/').collect();
            if f.len() < 7 || f[5] == "." || f[5] == ".." || f[5] == "lost+found" {
                return None;
            }
            Some(Entry {
                name: f[5].to_string(),
                mode: u32::from_str_radix(f[2], 8).unwrap_or(0),
                uid: f[3].to_string(),
                gid: f[4].to_string(),
                size: f[6].to_string(),
            })
        })
        .collect())
}

/// Recursively lists an ext2 tree with debugfs.
fn list_tree(target: &str, dir: &str, out: &mut Vec<String>) -> Result<()> {
    for e in list_dir(target, dir)? {
        let path = if dir == "/" {
            format!("/{}", e.name)
        } else {
            format!("{dir}/{}", e.name)
        };
        let is_dir = e.mode & 0o170000 == 0o040000;
        out.push(format!(
            "{:06o} {:>4}:{:<4} {:>10}  {path}{}",
            e.mode,
            e.uid,
            e.gid,
            e.size,
            if is_dir { "/" } else { "" }
````

Replace:

````rust

/// `verify-usb`: e2fsck the stick's root and print its file tree.
pub fn verify_usb() -> Result<()> {
````

with:

````rust

/// Where the NUC's check scripts are (`rootfs/root/checks/`).
pub const CHECKS_DIR: &str = "/root/checks";

/// Checks the transcript of every script in `CHECKS_DIR` on the ext2 root
/// `root` of `target` as written on `machine` (spec §15 item 12). Returns
/// the lines to print and whether every transcript there passed; a script
/// without a transcript was not run, which is reported but not a failure.
pub fn check_transcripts(
    target: &Path,
    root: Partition,
    machine: Machine,
    scratch: &Path,
) -> Result<(Vec<String>, bool)> {
    let mut scripts: Vec<String> = list_dir(&e2fs_target(target, root), CHECKS_DIR)?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.ends_with(".sh"))
        .collect();
    scripts.sort();
    let (mut out, mut ok) = (Vec::new(), true);
    for name in scripts {
        let path = format!("{CHECKS_DIR}/{name}");
        let log = shell::commands::transcript_name(&path);
        let read = |p: &str| -> Result<Option<String>> {
            Ok(image::read_ext2_file(target, root, p, scratch)?
                .map(|d| String::from_utf8_lossy(&d).into_owned()))
        };
        let script = read(&path)?.with_context(|| format!("{path}: cannot read it"))?;
        let Some(transcript) = read(&log)? else {
            out.push(format!("{path}: not run (no {log})"));
            continue;
        };
        let report = checks::check(&checks::parse(&script, machine)?, &transcript);
        let verdict = if report.ok() { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected",
            report.passed, report.commands
        ));
        out.extend(report.failures.iter().map(|f| format!("  {f}")));
        ok &= report.ok();
    }
    if out.is_empty() {
        out.push(format!("no check scripts in {CHECKS_DIR}"));
    }
    Ok((out, ok))
}

/// `verify-usb`: e2fsck the stick's root, print its file tree, and check
/// the transcripts of the check scripts run on the NUC.
pub fn verify_usb() -> Result<()> {
````

Replace:

````rust
        println!("{l}");
    }
````

with:

````rust
        println!("{l}");
    }
    let (report, ok) = check_transcripts(
        Stick::target(),
        root,
        Machine::Nuc,
        &out_dir().join("verify-usb"),
    )?;
    for l in report {
        println!("{l}");
    }
    if !ok {
        bail!("transcript check FAILED");
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 62 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "xtask: verify-usb checks the transcripts of the check scripts run on the NUC"
````


### Task 29: `verify-usb` fails for a script that was not run, and says when each ran

Review finding (minor): `verify-usb` reported a script without a transcript as `not run` but passed, so forgetting part 2 of the check went unnoticed; and after `flash --kernel`, which leaves the ext2 root and so the transcripts of an earlier run on the stick, nothing showed that a transcript was old. A script that was not run is now `FAILED, not run`, and every line gives the time its transcript was last written (`(run Mon Sep 28 14:09:17 2026 UTC)`, from `debugfs stat` in UTC) (decision 4).

**Files:**
- Modify: `xtask/src/flash.rs`

**Interfaces:**
- Consumes: Task 28.
- Produces: `flash::modified(target, Partition, path) -> Result<String>` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
````

with:

````rust
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            run(Command::new("touch")
                .args(["-d", "2026-09-28 14:09:17 UTC"])
                .arg(&p))
            .unwrap();
        }
````

Replace:

````rust
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
                "/root/checks/c.sh: not run (no /root/checks/c.log)",
            ]
````

with:

````rust
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
                "/root/checks/c.sh: FAILED, not run (no /root/checks/c.log)",
            ]
````

Replace:

````rust
            "{lines:?}"
        );
````

with:

````rust
            "{lines:?}"
        );
    }

    #[test]
    fn a_script_that_was_not_run_fails_the_check() {
        // Part 2 forgotten after the restart, or `sh` refused to start it.
        let dir = out_dir().join("verify-usb-selftest-not-run");
        let (img, root) = ext2_with(
            &dir,
            &[
                ("/root/checks/a.sh", "uname\n#> Relay\n"),
                ("/root/checks/a.log", "+ uname\nRelay\n"),
                ("/root/checks/b.sh", "ls\n"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, not run (no /root/checks/b.log)",
            ]
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: 2 tests fail: `flash::tests::a_script_that_was_not_run_fails_the_check`, `flash::tests::transcripts_of_the_check_scripts_are_checked_as_on_the_nuc`.

- [ ] **Step 3: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
/// `root` of `target` as written on `machine` (spec §15 item 12). Returns
/// the lines to print and whether every transcript there passed; a script
/// without a transcript was not run, which is reported but not a failure.
pub fn check_transcripts(
````

with:

````rust
/// `root` of `target` as written on `machine` (spec §15 item 12). Returns
/// the lines to print and whether every script ran and its transcript
/// passed. Each line gives the time the transcript was last written, since
/// `flash --kernel` leaves the transcripts of an earlier run on the stick.
pub fn check_transcripts(
````

Replace:

````rust
        let Some(transcript) = read(&log)? else {
            out.push(format!("{path}: not run (no {log})"));
            continue;
````

with:

````rust
        let Some(transcript) = read(&log)? else {
            out.push(format!("{path}: FAILED, not run (no {log})"));
            ok = false;
            continue;
````

Replace:

````rust
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected",
            report.passed, report.commands
        ));
````

with:

````rust
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {})",
            report.passed,
            report.commands,
            modified(target, root, &log)?
        ));
````

Replace:

````rust
    Ok((out, ok))
}
````

with:

````rust
    Ok((out, ok))
}

/// When the file at `path` was last changed, in UTC, as debugfs shows it:
/// `Mon Sep 28 14:09:17 2026 UTC`.
fn modified(target: &Path, root: Partition, path: &str) -> Result<String> {
    let text = run_stdout(
        Command::new("debugfs")
            .env("TZ", "UTC")
            .arg("-R")
            .arg(format!("stat \"{path}\""))
            .arg(e2fs_target(target, root)),
    )?;
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix("mtime: "))
        .and_then(|l| l.split_once(" -- "))
        .map(|(_, when)| format!("{when} UTC"))
        .with_context(|| format!("{path}: debugfs shows no mtime"))
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 63 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "xtask: verify-usb fails for a check script that was not run, and says when each ran"
````


### Task 30: NUC check 3 in `docs/hardware-test.md` runs from the scripts

The checklist follows the scripts: check 3 now boots the stick, runs `sh checks/check3-a.sh`, restarts, runs `sh checks/check3-b.sh`, switches off, and lets `verify-usb` check both transcripts; only the boot screen needs a photo. The failure table gains the lines `verify-usb` and `sh` print when a check does not go through.

**Files:**
- Modify: `docs/hardware-test.md`

**Interfaces:**
- Consumes: Tasks 26 and 28.
- Produces: `docs/hardware-test.md` check 3.

- [ ] **Step 1: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 3 replacements, top to bottom:

Replace:

````markdown

## Check 3 — files on the stick (plan 5)

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`).

1. In Mint: `cargo xtask flash --full` and type `ERASE` when asked (this
   erases the files of earlier runs).
2. Reboot, press F10 and choose the UEFI entry for the Kingston stick.
````

with:

````markdown

## Check 3 — files on the stick (plans 5 and 6)

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`). Since plan 6 the
commands come from two scripts on the stick, `/root/checks/check3-a.sh` and
`check3-b.sh` (in the repository under `rootfs/root/checks/`): `sh` runs
each line as if it were typed, shows it as `+ <command>` before its output,
and writes everything it shows into a transcript next to the script
(`check3-a.log`). `verify-usb` checks the transcripts in Mint against the
output the scripts expect (their `#>` lines), so only the boot screen needs
a look. The QEMU scenario `checks` runs the same scripts on every pull
request.

1. In Mint: `cargo xtask flash --full` and type `ERASE` when asked (this
   erases the files of earlier runs and writes the current scripts;
   `flash --kernel` leaves the scripts on the stick as they were).
2. Reboot, press F10 and choose the UEFI entry for the Kingston stick.
````

Replace:

````markdown
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
4. On the K120, type these and check each result (the `fileops` scenario's
   operations):
   - `mkdir -p /root/notes/old`, `echo remember me > /root/notes/a`,
     `echo and me >> /root/notes/a`, `cat /root/notes/a` → the two lines.
   - `ls -l /root/notes` → `a` (19 bytes) and the directory `old`, owned
     by `root root`, with today's date.
   - `cp /root/notes/a /root/notes/b`, `mv /root/notes/b /root/notes/old/c`,
     `ls /root/notes /root/notes/old` → `a  old`, then `c`.
   - `rmdir /root/notes/old` → `rmdir: failed to remove '/root/notes/old':
     Directory not empty`; `rm -r /root/notes/old`; `ls /root/notes` → `a`.
   - `touch /root/notes/t`, `stat /root/notes/t` (size 0, mode 0644, the
     current UTC time), `head -n 1 /root/notes/a`, `tail -n 1
     /root/notes/a`, `wc /root/notes/a` → `2 4 19`.
   - `cat /root/nope` → `cat: /root/nope: No such file or directory`;
     `rm -r /` → `rm: it is dangerous to operate recursively on '/'`.
   - `df` shows `/dev/root` of about 15 million 1K-blocks; `dmesg` shows the
     `storage: slot N:` lines (`Kingston`, `DataTraveler 3.0`, `PMAP`,
     `30277632 blocks of 512 bytes`).
5. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10; `cat /root/notes/a` shows both lines and `ls /root/notes`
   shows `a  t`.
6. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
7. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, and the
   tree lists `/root/notes/a` and `/root/notes/t`.
8. Photograph the screen after step 3 and after `dmesg`.

````

with:

````markdown
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.

   Photograph the screen.
4. On the K120, type `sh checks/check3-a.sh`. It runs for about a minute:
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), the `fileops` scenario's operations on `/root/notes` (`mkdir -p`,
   `echo >`/`>>`, `cat`, `ls -l`, `cp`, `mv`, `rmdir` of a full directory,
   `rm -r`, `touch`, `stat`, `head`, `tail`, `wc`, the errors of `cat` and
   `rm -r /`), an 8 MiB file built by doubling (each step writes up to
   4 MiB to the stick and syncs), and `df`. Ctrl-C stops it. The prompt
   comes back after `+ df` and its two lines.
5. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10 and type `sh checks/check3-b.sh`: the files written before the
   restart are read back.
6. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
7. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 63 of 63 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
   <time>)`, with the UTC times of the two runs (after `flash --kernel` the
   transcripts of an earlier run stay on the stick, so check the times). A
   `FAILED` line is followed by the script line, the expectation that
   failed and what the command printed there; a script that was not run is
   `FAILED, not run`.

````

Replace:

````markdown
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |

````

with:

````markdown
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |
| `verify-usb`: `check3-a.sh: FAILED` | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
| `sh: cannot write the transcript …` | `/` is read-only (see the `mount /` line) | Nothing ran; fix the mount first |

````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add docs
git commit -m "docs: NUC check 3 runs from the check scripts, and verify-usb checks their transcripts"
````


### Finish PR 6

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 20 scenario(s) passed`.

````bash
git push -u origin plan6/scripts
gh pr create --base main --head plan6/scripts --title "Plan 6: Scripts and the NUC check" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 18–30: `#` comments; `sh FILE`, which runs a file's commands one by one as written, each shown as `+ <line>`, with Ctrl-C and without nesting; a transcript of everything the script shows, errors included, written every 4 KiB and synced as each line starts and ends; the transcript checker with `#>`/`#nuc>`/`#qemu>`/`#!>` expectations, naming the expectation that failed; NUC check 3 as `/root/checks/check3-{a,b}.sh`, run in QEMU by the `checks` scenario and checked against a NUC transcript in a unit test; `verify-usb` checks the transcripts, fails for a script not run and shows when each ran; `docs/hardware-test.md` check 3 runs from the scripts.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the `checks` QEMU scenario

## Hardware

- [x] Not needed now: the scripts run on the NUC in the final check (PR 7)
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/scripts --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-scripts
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 7: Milestone 1 done (Tasks 31–33)

The README quick start, the spec's body brought in line with its revisions, and version 0.2.0. The final whole-branch review's fixes go into this PR (before the version task) if earlier PRs have already merged. It ends with the full hardware checklist on a stick written by `cargo xtask flash --full` from this worktree.

Branch `plan6/release`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-release`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan6/release /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-release origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-release
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan6/scripts` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan6/release /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-release plan6/scripts`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan6/scripts>` and re-run `cargo xtask ci` before pushing.

### Task 31: A README quick start

Spec §11 step 7: `README.md` gets a quick start: `cargo xtask test` and `cargo xtask qemu` for QEMU, and for the NUC the five steps from `setup-udev` to `verify-usb`, with the check scripts. The command and layout tables mention the scripts.

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: Tasks 26 and 28.
- Produces: `README.md`.

- [ ] **Step 1: Change `README.md`**

In `README.md`, make these 3 replacements, top to bottom:

Replace:

````markdown
Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

````

with:

````markdown
Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

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
   `sh checks/check3-b.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of both scripts.

After a code change, `cargo xtask flash --kernel` replaces only the loader
and the kernel and keeps the files on the stick.

````

Replace:

````markdown
| `cargo xtask flash --kernel` | Update loader and kernel on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick and list its files |

````

with:

````markdown
| `cargo xtask flash --kernel` | Update loader and kernel on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick, list its files and check the transcripts of the check scripts |

````

Replace:

````markdown
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser and built-in commands |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
| `rootfs/` | Files copied into `/` |
| `tests/e2e/` | QEMU end-to-end scenarios |
````

with:

````markdown
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser, built-in commands and scripts (`sh FILE`) |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
| `tests/e2e/` | QEMU end-to-end scenarios |
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add README.md
git commit -m "docs: a README quick start for QEMU and the NUC"
````


### Task 32: The spec's body follows its revisions

The final spec/code consistency pass of spec §11 step 7. Where the body of the spec still says what a §15 revision changed, it now says what the code does, citing the revision: the loader's linear map (item 5) and the kernel's pages as `Bootloader` memory (item 7); the GDT and IDT before the console and no frames below 1 MiB (item 8); every xHCI controller started, the `Hal` with register access and the log, 32-byte contexts and 34 scratchpads on the NUC (item 10); one `FileSystem` trait keyed by inode number and the clean flag written back as found (item 9); the parser's syntax, `sh`, test mode's exit status, the QEMU command line with the QMP socket file, the e2e steps and scenarios, QEMU's real gaps, the hardware checklist with the check scripts, and scripts in §14 (items 9–12). The header's `Status` line says plans 1–6 implemented it, ending at 0.2.0, and the `Branch` line says `main`, one pull request at a time, instead of `milestone-1`.

**Files:**
- Modify: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

**Interfaces:**
- Consumes: every earlier task.
- Produces: the spec's body.

- [ ] **Step 1: Change `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`**

In `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`, make these 24 replacements, top to bottom:

Replace:

````markdown
- **Date:** 2026-09-26
- **Status:** Approved 2026-09-26; revised during planning (see §15)
- **Branch:** `milestone-1`

````

with:

````markdown
- **Date:** 2026-09-26
- **Status:** Approved 2026-09-26; revised during planning (see §15);
  implemented by plans 1–6 (`docs/superpowers/plans/`), which end milestone 1
  at version 0.2.0
- **Branch:** `main`, one pull request at a time (see the roadmap,
  `docs/superpowers/plans/2026-09-26-milestone-1-roadmap.md`)

````

Replace:

````markdown
   ├─ hardware-test.md
   └─ superpowers/specs/
```
````

with:

````markdown
   ├─ hardware-test.md
   └─ superpowers/{specs,plans}/
```
````

Replace:

````markdown
     the ELF flags (NX on data).
   - All RAM in the memory map, plus the framebuffer, mapped linearly at
     `PHYS_OFFSET = 0xFFFF_8000_0000_0000` using 2 MiB pages.
````

with:

````markdown
     the ELF flags (NX on data).
   - The RAM-type regions of the memory map (usable, loader, kernel, ACPI;
     §15 item 5), plus the framebuffer, mapped linearly at
     `PHYS_OFFSET = 0xFFFF_8000_0000_0000` using 2 MiB pages.
````

Replace:

````markdown
| `framebuffer` | Physical address, byte size, width, height, stride (in pixels), pixel format (RGB or BGR). |
| `memory_map` | Pointer and length of `MemoryRegion { start, len, kind }`, where `kind` is `Usable`, `Bootloader`, `AcpiReclaimable`, `AcpiNvs`, `Reserved`, `Mmio` or `Kernel`. UEFI boot-services code and data become `Usable`. Loader data becomes `Bootloader`. |
| `rsdp_addr` | Physical address of the RSDP. |
````

with:

````markdown
| `framebuffer` | Physical address, byte size, width, height, stride (in pixels), pixel format (RGB or BGR). |
| `memory_map` | Pointer and length of `MemoryRegion { start, len, kind }`, where `kind` is `Usable`, `Bootloader`, `AcpiReclaimable`, `AcpiNvs`, `Reserved`, `Mmio` or `Kernel`. UEFI boot-services code and data become `Usable`. Loader data becomes `Bootloader`, and so do the kernel's pages, which the loader allocates as loader data: `Kernel` is not produced (§15 item 7). |
| `rsdp_addr` | Physical address of the RSDP. |
````

Replace:

````markdown
2. **CPU tables:** GDT with TSS (an IST stack for double faults), then an IDT
   whose exception handlers draw the panic screen (§10).
3. **Memory:** frame allocator (§5.1), kernel heap (§5.2), then kernel-owned
````

with:

````markdown
2. **CPU tables:** GDT with TSS (an IST stack for double faults), then an IDT
   whose exception handlers draw the panic screen (§10). They are
   loaded before the console starts, so an early fault still reaches the
   panic screen; the status lines keep this order (§15 item 8).
3. **Memory:** frame allocator (§5.1), kernel heap (§5.2), then kernel-owned
````

Replace:

````markdown
  of milestone 1.

````

with:

````markdown
  of milestone 1.
- Frames below 1 MiB are never allocated (§15 item 8).

````

Replace:

````markdown
The result is a device list that `dmesg` prints and the xHCI driver uses to
find its controller (class `0x0C`, subclass `0x03`, prog-if `0x30`). Enable
memory-space decoding and bus mastering on that controller.

````

with:

````markdown
The result is a device list that `dmesg` prints and the xHCI driver uses to
find its controllers (class `0x0C`, subclass `0x03`, prog-if `0x30`). Every
xHCI controller is put into power state D0 and gets memory-space decoding
and bus mastering; the NUC has two (§15 items 8 and 10).

````

Replace:

````markdown
pub trait Hal {
    fn map_mmio(&self, phys: u64, len: usize) -> NonNull<u8>;
    fn alloc_dma(&self, size: usize, align: usize) -> DmaBuf; // virt + phys, zeroed
    fn free_dma(&self, buf: DmaBuf);
    fn now(&self) -> Duration;      // monotonic time since boot
    fn sleep(&self, d: Duration);   // busy-wait on the timer
}
```

````

with:

````markdown
pub trait Hal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize>;    // virtual address
    unsafe fn read32(&self, addr: usize) -> u32;                    // device registers
    unsafe fn write32(&self, addr: usize, value: u32);
    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf>; // zeroed, page-aligned
    fn free_dma(&self, buf: DmaBuf);
    fn now(&self) -> Duration;      // monotonic time since boot
    fn sleep(&self, d: Duration);   // busy-wait on the timer
    fn log(&self, args: fmt::Arguments); // a line in dmesg
}
```

(Register access and the log went into the `Hal` during planning, so host
tests can run the driver against a fake controller; §15 item 10.)

````

Replace:

````markdown
- **Scratchpad buffers:** read `Max Scratchpad Buffers` from HCSPARAMS2 and
  allocate the scratchpad array and pages. Intel requires them; QEMU requires
  none.
- **Context size:** `HCCPARAMS1.CSZ` chooses 32-byte (QEMU) or 64-byte (Intel)
  contexts. Every context accessor takes that stride as a parameter.
- **Supported Protocol capabilities:** tell us which ports are USB 2 and which
````

with:

````markdown
- **Scratchpad buffers:** read `Max Scratchpad Buffers` from HCSPARAMS2 and
  allocate the scratchpad array and pages. The NUC's controllers ask for 34;
  QEMU's for none.
- **Context size:** `HCCPARAMS1.CSZ` chooses 32-byte or 64-byte contexts.
  QEMU and both of the NUC's controllers use 32-byte ones (§15 item 10), so
  64-byte contexts are tested on the host only. Every context accessor takes
  that stride as a parameter.
- **Supported Protocol capabilities:** tell us which ports are USB 2 and which
````

Replace:

````markdown
**Device setup sequence:**
1. Detect the connection change on the port and reset the port.
2. Enable Slot.
````

with:

````markdown
**Device setup sequence:**
1. Detect the connection change on the port, wait until the connection has
   been stable for 100 ms (§15 item 12), and reset the port.
2. Enable Slot.
````

Replace:

````markdown
- **Kernel log:** a 64 KiB ring buffer holding every `[ ok ]` / `[FAIL]` line,
  driver messages and warnings. `dmesg` prints it.

````

with:

````markdown
- **Kernel log:** a 64 KiB ring buffer holding every `[ ok ]` / `[FAIL]` line,
  driver messages and warnings. `dmesg` prints it. The shell's output goes to
  the screen and serial, not into the log (§15 item 10).

````

Replace:

````markdown
- **Parser:** words are split on whitespace. Supported syntax:
  - `'…'` (literal) and `"…"` (supports `\"` and `\\` inside);
  - `\` escapes outside quotes;
  - `> file` (truncate) and `>> file` (append) on any command's standard
    output;
  - error messages always go to the screen, never into a redirect file.

  Anything else is an error: `|`, `;`, `&`, `$`, `*` and `?` (when not
  quoted) print `relay-sh: unsupported syntax: <token>`.
- **Command results:** every command returns an exit status. Error messages
````

with:

````markdown
- **Parser:** words are split on whitespace. Supported syntax:
  - `'…'` (literal) and `"…"` (supports `\"`, `\\`, `\$` and `` \` `` inside;
    a bare `$` or `` ` `` in it is refused, §15 item 12);
  - `\` escapes outside quotes;
  - `#` at the start of a word begins a comment (§15 item 12);
  - `> file` (truncate) and `>> file` (append) on any command's standard
    output;
  - error messages always go to the screen, never into a redirect file.

  Anything else is an error: `|`, `;`, `&`, `$`, `*`, `?`, `<`, `` ` ``,
  `(`, `)` and `2>` (when not quoted) print
  `relay-sh: unsupported syntax: <token>` (§15 item 9).
- **Command results:** every command returns an exit status. Error messages
````

Replace:

````markdown
| `sync` | flushes the block cache and the device |
| `reboot` / `poweroff` | see §7.4 |
````

with:

````markdown
| `sync` | flushes the block cache and the device |
| `sh` | `sh file`: runs the file's commands one by one, each shown as `+ <line>`, into a transcript (§15 item 12) |
| `reboot` / `poweroff` | see §7.4 |
````

Replace:

````markdown
  `System halted. It is now safe to power off.` and halt.
- **Test mode** (`cmdline` contains `test=1`): `poweroff` writes the exit code
  to the `isa-debug-exit` port `0xF4` before trying ACPI, so QEMU exits
  straight away.

````

with:

````markdown
  `System halted. It is now safe to power off.` and halt.
- **Test mode** (`cmdline` contains `test=1`): `poweroff` writes 0x10 to the
  `isa-debug-exit` port `0xF4` before trying ACPI, so QEMU exits straight
  away, with status 33 (§15 item 11).

````

Replace:

````markdown

- **Traits:** `FileSystem` (root inode, statfs, sync) and `Inode` (stat,
  lookup, read_at, write_at, truncate, create, mkdir, unlink, rmdir, rename,
  readdir).
- **Mount table:** one entry for now, ext2 at `/`. Path lookup already
````

with:

````markdown

- **Traits:** one `FileSystem` trait whose operations name inodes by number
  (root, stat, lookup, read_at, write_at, truncate, create, mkdir, unlink,
  rmdir, rename, read_dir, read_link, touch, statfs, sync, shutdown), and a
  path-level `Vfs` trait, which the mount table implements, for the shell
  (§15 item 9).
- **Mount table:** one entry for now, ext2 at `/`. Path lookup already
````

Replace:

````markdown
  filesystem not clean), increment `s_mnt_count` and write the superblock.
- A clean unmount (from `reboot` or `poweroff`) sets `EXT2_VALID_FS` again.
- Every superblock write also updates the backup copies in groups that hold
````

with:

````markdown
  filesystem not clean), increment `s_mnt_count` and write the superblock.
- A clean unmount (from `reboot` or `poweroff`) writes back the state the
  filesystem had when it was mounted: one that was clean is marked clean
  again, one that was not stays so until `e2fsck` has checked it (§15
  item 9).
- Every superblock write also updates the backup copies in groups that hold
````

Replace:

````markdown
| `cargo xtask flash --full` | After a typed confirmation: writes a new GPT, formats the ESP, runs `mke2fs` over the rest of the stick (same options as `image`), fills it, sets ownership, then writes the ESP files. |
| `cargo xtask verify-usb` | Runs `e2fsck -fn` on the stick's ext2 partition and prints its file tree using `debugfs`. |
| `cargo xtask setup-udev` | Writes a udev rule that gives the invoking user read/write access to the disk and partitions **with this stick's serial only**, and prints the three `sudo` commands that install it (§15). |
````

with:

````markdown
| `cargo xtask flash --full` | After a typed confirmation: writes a new GPT, formats the ESP, runs `mke2fs` over the rest of the stick (same options as `image`), fills it, sets ownership, then writes the ESP files. |
| `cargo xtask verify-usb` | Runs `e2fsck -fn` on the stick's ext2 partition, prints its file tree using `debugfs`, and checks the transcripts of the check scripts in `/root/checks` (§15 item 12). |
| `cargo xtask setup-udev` | Writes a udev rule that gives the invoking user read/write access to the disk and partitions **with this stick's serial only**, and prints the three `sudo` commands that install it (§15). |
````

Replace:

````markdown
```
qemu-system-x86_64 -machine q35 -accel kvm:tcg -m 1G -cpu max -smp 1 -no-reboot
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE_4M.fd
````

with:

````markdown
```
qemu-system-x86_64 -machine q35 -accel kvm -accel tcg -cpu max -smp 1 -m 1G
  -no-reboot -nic none
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE_4M.fd
````

Replace:

````markdown
  -drive if=none,id=stick,format=raw,file=<per-run copy of relay-os.img>
  -device usb-storage,bus=xhci.0,drive=stick
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
  -serial <pipe> -qmp unix:<sock>,server,nowait -display none
```

````

with:

````markdown
  -drive if=none,id=stick,format=raw,file=<per-run copy of relay-os.img>
  -device usb-storage,bus=xhci.0,drive=stick,id=stick-usb
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
  -serial stdio -monitor none -display none
  -chardev socket,id=qmp,path=qmp.sock,server=on,wait=off
  -mon chardev=qmp,mode=control
```

QEMU runs in the scenario's run directory, which only its owner can
enter, and binds the QMP socket there (§15 item 12).

````

Replace:

````markdown
  - `expect <regex>`: waits for matching serial output, with a timeout;
  - `screenshot`: saves a QMP `screendump` and checks it isn't a single
    colour;
  - `reboot`: sends `reboot`, waits for QEMU to exit (`-no-reboot`), then
    starts it again on the same disk;
  - `poweroff`.
- After every scenario, `e2fsck -fn` must pass on the ext2 partition extracted
````

with:

````markdown
  - `expect <regex>`: waits for matching serial output, with a timeout;
  - `screenshot-nonblank`: saves a QMP `screendump` and checks it isn't a
    single colour;
  - `reboot [<regex>]`: sends `reboot`, waits for QEMU to exit
    (`-no-reboot`), then starts it again on the same disk;
  - `poweroff`;
  - and the steps added during planning (§15 items 4, 9, 11 and 12):
    `cmdline`, `disk small`, `break-root`, `esp-write`, `timeout`, `alive`,
    `screenshot-pixel`, `file-lines`, `unplug` and `check-script`, listed at
    the top of `xtask/src/e2e.rs`.
- After every scenario, `e2fsck -fn` must pass on the ext2 partition extracted
````

Replace:

````markdown
| 7 | `diskfull` | Fill a small test image (32 MiB ext2) until `ENOSPC`. The shell reports `No space left on device` and `e2fsck` is still clean. |

**Known gaps in QEMU** (covered by the hardware checklist instead):
- 64-byte contexts, scratchpad buffers and BIOS handoff. QEMU's xHCI needs
  none of them, so those code paths only get host unit tests.
- SuperSpeed port reset. QEMU's `usb-storage` connects at high speed.
- Real GOP modes on HDMI.
````

with:

````markdown
| 7 | `diskfull` | Fill a small test image (32 MiB ext2) until `ENOSPC`. The shell reports `No space left on device` and `e2fsck` is still clean. |
| 8 | `power` | `reboot` through the FADT reset register and `poweroff` through test mode's exit, each leaving the root clean (§15 item 11). |
| 9 | `mount_fail` | A root that cannot be mounted leaves the empty read-only `/` (§10). |
| 10 | `unplug` | After the stick is pulled out, commands fail at once with `EIO` and `poweroff` refuses to go on without a clean shutdown (§15 item 12). |
| 11 | `checks` | NUC check 3's scripts run with a `reboot` between them, and their transcripts show what the scripts expect (§15 item 12). |

Besides these, the earlier plans' scenarios check the loader and the kernel
on their own: `boot_bigmode`, `keyboard`, `shell`, `timer`,
`loader_bad_kernel` and the five `panic_*` scenarios.

**Known gaps in QEMU** (covered by the hardware checklist instead):
- 64-byte contexts, scratchpad buffers, the BIOS handoff, port power
  control, low- and full-speed devices and a USB 3 port that needs a warm
  reset. QEMU's xHCI needs none of them, so those code paths only get host
  unit tests (QEMU's `usb-storage` does connect at SuperSpeed; §15 item
  10).
- Real GOP modes on HDMI.
````

Replace:

````markdown
3. Check the screen: native resolution, every startup line `[ ok ]`, the xHCI
   log line shows 64-byte contexts and the scratchpad count.
4. Run the scripted command list in the document (the same operations as the
   `fileops` scenario), typed on the K120.
5. `reboot`, choose the stick again with F10, and check the files are still
   there.
6. `poweroff`. The machine should turn off, or show the safe-to-power-off
   message.
7. Boot into Mint and run `cargo xtask verify-usb`. It must be clean and list
   the files from step 4.

````

with:

````markdown
3. Check the screen: native resolution, every startup line `[ ok ]`, the xHCI
   log lines show the context size and the scratchpad count (32-byte
   contexts and 34 scratchpads on the NUC).
4. Type `sh checks/check3-a.sh` on the K120: the check script runs the
   `fileops` scenario's operations and more, into a transcript (§15 item
   12).
5. `reboot`, choose the stick again with F10, and type
   `sh checks/check3-b.sh`, which reads the files back.
6. `poweroff`. The machine should turn off, or show the safe-to-power-off
   message.
7. Boot into Mint and run `cargo xtask verify-usb`. It must be clean, list
   the files from step 4, and find both transcripts as the scripts expect.

````

Replace:

````markdown
  - path resolution and the parser;
  - each shell command against an in-memory `Vfs`;
  - terminal escape parsing and rendering.
````

with:

````markdown
  - path resolution and the parser;
  - each shell command against an in-memory `Vfs`, and scripts (`sh`);
  - the transcript checker of the check scripts;
  - terminal escape parsing and rendering.
````

Replace:

````markdown
- USB hubs, mice, other USB classes, USB 3 streams/UAS.
- A text editor, pipes, environment variables, globbing, scripts.
- Symlink following, hard-link creation, permission enforcement, multiple
````

with:

````markdown
- USB hubs, mice, other USB classes, USB 3 streams/UAS.
- A text editor, pipes, environment variables, globbing, and scripts beyond a
  list of commands (`sh` has no variables, arguments, loops or conditions;
  §15 item 12).
- Symlink following, hard-link creation, permission enforcement, multiple
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add docs
git commit -m "docs: the spec's body follows its §15 revisions, with its status and branch lines up to date"
````


### Task 33: Version 0.2.0

Decision 5: milestone 1 is done, and its version is 0.2.0 (0.1.0 is Cargo's default). The workspace version in `Cargo.toml`, which every crate inherits and `Cargo.lock` follows, becomes 0.2.0; the loader (`relay-boot 0.2.0`) and the kernel (`Relay OS 0.2.0`) already print `CARGO_PKG_VERSION`, and now `uname -a` does too instead of a hard-coded string, so it cannot drift again. The `shell` scenario, the check script, the checklist's check 1 and spec §7.3 say 0.2.0. This is the plan's last task; tagging `v0.2.0` or publishing a release is outward-facing and is only done if the user asks for it.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `docs/hardware-test.md`
- Modify: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/shell.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`
- Modify: `xtask/fixtures/checks/check3-b.qemu.log`

**Interfaces:**
- Consumes: every earlier task.
- Produces: `[workspace.package] version = "0.2.0"`; `commands::basic::UNAME_ALL` from `CARGO_PKG_VERSION`.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
[workspace.package]
version = "0.1.0"
edition = "2024"
````

with:

````toml
[workspace.package]
version = "0.2.0"
edition = "2024"
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.1.0 x86_64\n".into()));
        assert_eq!(
````

with:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 1 is version 0.2.0 (spec §15 item 12), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.2.0 x86_64\n".into()));
        assert_eq!(
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/shell.txt`**

In `tests/e2e/shell.txt`, replace:

````text
send uname -a
expect \nRelay relay 0\.1\.0 x86_64\n
send cat /etc/hostname
````

with:

````text
send uname -a
expect \nRelay relay 0\.2\.0 x86_64\n
send cat /etc/hostname
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::uname_prints_the_system`.

- [ ] **Step 5: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
pub const UNAME: &str = "Relay";
pub const UNAME_ALL: &str = "Relay relay 0.1.0 x86_64";

````

with:

````rust
pub const UNAME: &str = "Relay";
pub const UNAME_ALL: &str = concat!("Relay relay ", env!("CARGO_PKG_VERSION"), " x86_64");

````

- [ ] **Step 6: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.1.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

with:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.2.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

- [ ] **Step 7: Change `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`**

In `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`, replace:

````markdown
| `help` | lists commands with a one-line description |
| `uname` | `uname [-a]` → `Relay relay 0.1.0 x86_64` |
| `date` | wall clock from the RTC, printed in UTC |
````

with:

````markdown
| `help` | lists commands with a one-line description |
| `uname` | `uname [-a]` → `Relay relay 0.2.0 x86_64` (the version from `Cargo.toml`) |
| `date` | wall clock from the RTC, printed in UTC |
````

- [ ] **Step 8: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
uname -a
#> Relay relay 0\.1\.0 x86_64
date
````

with:

````bash
uname -a
#> Relay relay 0\.2\.0 x86_64
date
````

- [ ] **Step 9: Change `xtask/fixtures/checks/check3-a.nuc.log`**

In `xtask/fixtures/checks/check3-a.nuc.log`, replace:

````text
+ uname -a
Relay relay 0.1.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1200 (120x33 cells)
````

with:

````text
+ uname -a
Relay relay 0.2.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1200 (120x33 cells)
````

- [ ] **Step 10: Change `xtask/fixtures/checks/check3-a.qemu.log`**

In `xtask/fixtures/checks/check3-a.qemu.log`, replace:

````text
+ uname -a
Relay relay 0.1.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1080 (120x33 cells)
````

with:

````text
+ uname -a
Relay relay 0.2.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1080 (120x33 cells)
````

- [ ] **Step 11: Change `xtask/fixtures/checks/check3-b.nuc.log`**

In `xtask/fixtures/checks/check3-b.nuc.log`, replace:

````text
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1200 (120x33 cells)
````

with:

````text
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1200 (120x33 cells)
````

- [ ] **Step 12: Change `xtask/fixtures/checks/check3-b.qemu.log`**

In `xtask/fixtures/checks/check3-b.qemu.log`, replace:

````text
+ dmesg
Relay OS 0.1.0
[ ok ] console 1920x1080 (120x33 cells)
````

with:

````text
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1080 (120x33 cells)
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 117 tests.

- [ ] **Step 14: Run the `shell`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 15: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 16: Commit**

````bash
git add Cargo.lock Cargo.toml crates docs rootfs tests xtask
git commit -m "Version 0.2.0: milestone 1 is done; uname takes the version from Cargo"
````


### Finish PR 7

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 20 scenario(s) passed`.

````bash
git push -u origin plan6/release
gh pr create --base main --head plan6/release --title "Plan 6: Milestone 1 done (version 0.2.0)" --body-file - <<'EOF'
## What

Milestone 1, plan 6, tasks 31–33: a README quick start; the spec's body follows its §15 revisions, with its status and branch lines; version 0.2.0, with `uname -a` taking it from Cargo.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Needed: the full hardware checklist (checks 1–3 of `docs/hardware-test.md`), run by the user with `cargo xtask flash --full` from this worktree; the result goes into the results log
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan6/release --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan6-release
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
