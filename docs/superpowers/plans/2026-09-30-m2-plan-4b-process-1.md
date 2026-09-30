# Milestone 2 · Plan 4b: Process 1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The machine starts `/bin/sh` itself. Process 1 becomes the kernel's init: it prints the motd, starts `/bin/sh` in `/root` with the console, collects every orphan as it ends, and starts the shell again when it ends, until it has ended three times within 10 s; then, or when `system.img` cannot be mounted or the shell cannot start, it shows an error screen that says why and restarts the machine at a key. The in-kernel shell goes, and `relay-kernel` no longer depends on the `shell` crate (spec §1.4 item 3): every command the user types is a program. Every scenario runs under `/bin/sh`, milestone 1's unchanged; the kernel's findings carried forward from plans 3a–4a are fixed; NUC check 4 (`check4.sh`) runs on the NUC. It ends with `cargo xtask ci` green and NUC check 4 passed.

**Architecture:** `kernel/src/init.rs` is process 1: a host-tested rule (`Respawn`, over a clock) and log line, and the glue that starts `/bin/sh` through `proc::spawn` and waits for it with `proc::wait_collecting`. `kernel/src/error_screen.rs` is the error screen: its text host-tested, shown by init in its own context with interrupts on, so the idle task polls the keyboards while it waits. `system::mount` returns why the archive could not be mounted, and init shows the screen for it. The in-kernel shell's glue (`session.rs`, `File::ShellOutput`, the output hook, the console handover) and the shell crate's way to programs for it (`System::spawn`/`wait`) go. xtask gains `t-abi`, the e2e step `reset`, and `#same>` for check scripts.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model), e2fsprogs 1.47, binutils' `readelf` for tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.4, §4.3, §6.4–§6.6, §8.2–§8.5, §10, §11, §12.3, §12.4, §13 step 4, §16 item 6)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 4b of 7.

## In brief

- **Size.** 19 tasks in four code pull requests, plus this plan as PR 1: PR 2 the kernel's findings carried forward and `t-abi`; PR 3 init, the error screen and every scenario under `/bin/sh`; PR 4 the kernel without the shell; PR 5 `check4.sh`, a draft until NUC check 4 passes, whose transcripts go into it (decision 1).
- **Process 1 is init** (decision 2): a process of the kernel's own, which starts `/bin/sh` (pid 2) with the console, says `init: /bin/sh (pid 2) exited with 0; starting it again` on the screen and in the kernel log when it ends, and starts another; the third end within 10 s reaches the error screen.
- **The error screen** (decision 3) is not the red panic screen: init shows it, with the reason, the kernel log's last lines and `Press any key to reboot.`, waits for a key for as long as it takes, then syncs and restarts through ACPI. `system_missing`, `system_abi` and `system_empty` reach it; `mount_fail` still reaches a prompt.
- **Milestone 1's scenarios are unchanged** (decision 5): a throwaway spike ran every scenario under `/bin/sh` before the tasks were written, and all of milestone 1's passed. What changed is later plans' lines that described the in-kernel shell.
- **The kernel has no shell** (decision 4): an xtask test checks that `relay-kernel` does not depend on `shell`.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran throwaway probes and scenarios under KVM on the NUC's CPU model (an i7-1260P), and found no critical, no important and 7 minor real defects; four are fixed in a task of their own after the task they concern (two of them stricter tests, with the mutation checks that show what they guard), one is fixed in the tasks that own the stale comments, one is a correction to the spec and one a ruling. It found correct the respawn rule at its edges, init's loop (the motd once, the shell's group and console, orphans collected, `kill` of process 1 refused), a dead shell's reader, CR LF after `exit`, the error screen's text, drain, sync and restart, `reset`, the unplugged disk and `expect` after `poweroff`, `#same>` and `check4.sh` (60 children every time, the orphan's line, a second run), the poll on the way back to ring 3 (no lock, no race), every carry-forward, `t-abi`, and nothing of the in-kernel shell left in the kernel:

| Finding (review) | Decision |
|---|---|
| Minor: init holds its record of the archive locked through the error screen, which waits and so switches (a guard made in `if let` lives to the end of the block); the switch's check did not cover that lock | Fixed, Task 12 |
| Minor: the test that the kernel does not depend on `shell` sees only direct dependencies: one through `relay-rt` passes it | Fixed (a stricter test), Task 16 |
| Minor: init's fallback to `/` without a `/root` is untested: `mount_fail` never reaches it | Fixed (a test), Task 13 |
| Minor: the error screen counts lines, not rows: the NUC's real lines wrap, and long ones would scroll the heading away; its test used short lines | Fixed, Task 9 |
| Minor: the error screen's `take_console` is untested: removing it passes every scenario | Ruled: unreachable in milestone 2 (nothing a person runs ends the shell while a command has the console); milestone 3's `kill` tests it (decision 3, roadmap) |
| Minor: stale comments (`seek`'s "the shell's outputs", `power.rs`'s "the shell has shut the filesystems down", `t-mem`'s doc comments swapped) | Fixed in Tasks 5 and 15 |
| Minor: spec §14 gives check 4 the tick poll's cost measured with and without, and §12.4 check 4's step by hand as `t-spin` and Ctrl-C | Fixed in the spec (PR 1, decision 9): the step is `exit`; the measurement without the poll is plan 5's if the counts call for it |
| Declined to judge: `free` before and after on the NUC (only QEMU's transcript so far); a Ctrl-C in the milliseconds between the new shell's spawn and its first prompt; a lone ESC over COM1 just before the error screen counting as the key; `t-mem churn`'s 3 s bound under TCG | Ruled: NUC check 4 shows the first; the others are too narrow to reach (the reviewer could not produce the second), and TCG is a local convenience only |

## Where this plan fits

Plan 4b implements the second half of spec §13 step 4. It builds on plan 4a: `/bin/sh` and a program per command, run by path from the in-kernel shell, still process 1 (roadmap, "What plan 4a leaves for plan 4b"). It leaves plan 5 the NUC check's fixes, the spec's §16 up to date and version 0.3.0 (roadmap, "What plan 4b leaves for plan 5").

## Working conventions

- Plan 4b lands as **five pull requests** (table below). This plan, with the spec's §16 item 6 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. PR 5 needs NUC check 4: it goes up as a draft, and after the user reports a pass, the real transcripts and the results-log row go into a commit of the same PR, which is then marked ready. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-4b-process-1.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 5, 11 and 12 fix what only the machine shows, and their red runs are scenarios (`ctrlc`, `respawn` and `system_missing`). Task 11's other scenarios use the step `reset`, which Task 10 adds on its own for that reason, with a unit red run. Four tasks have no failing run, and their introductions say why: Task 3 moves a check and adds a test of correct code, Tasks 13 and 16 add stricter tests of what the code already does, and Task 17 only removes code; the mutation checks show what their tests guard. The mutation checks the prototype ran are named in each task's introduction where a guard could pass vacuously.
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; it happened twice while plan 3b's prototype was made, under a mutant. Mutation checks run under an address-space limit and a timeout (`tmp/m2p4b/mutate.py`). `t-mem churn` stops by itself after 20 s.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `2223aee` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request, NUC and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p4b/plan` | — | The spec's §16 item 6 and the corrections to its body, the roadmap's notes for plans 4b and 5, this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p4b/kernel` | 1–6 | `console_foreground` of zombies, `open`'s `EISDIR`, a tee's checks, `ENOSPC`, the console polled on the way back to ring 3; `t-abi` and `stale_program` | `lint`, `unit`, `e2e` |
| 3 | `m2p4b/init` | 7–14 | Init, the error screen, the step `reset`, every scenario under `/bin/sh`; `-f` says why | `lint`, `unit`, `e2e` |
| 4 | `m2p4b/noshell` | 15–17 | The kernel without `shell`; the shell crate without the in-kernel shell's way to programs | `lint`, `unit`, `e2e` |
| 5 | `m2p4b/check4` | 18–19 | `#same>`; `check4.sh`; NUC check 4 (draft until it passes) | `lint`, `unit`, `e2e`; NUC check 4 |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 4b adds no crate; `relay-kernel` loses `shell`. The kernel has no dev-dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- The command functions in `crates/shell` do not change to become programs (gate §3.1); every command prints exactly what it prints in milestone 1, and every milestone 1 scenario passes unchanged but for the startup line (gate §1.4).
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. Nothing waits for ever but the error screen, which waits for a person's key (decision 3). Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils and bash; `/bin/sh` names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- A plan that changes a check script ends with `cargo xtask flash --full` for its NUC check, and the recorded NUC transcripts in `xtask/fixtures/checks/` are replaced by the real ones copied off the stick, in a commit of the plan's last PR, with the results-log row.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 6 in PR 1:

1. **Plan 4b is one plan** (§13), in five pull requests: this plan; the kernel's findings carried forward and `t-abi`; process 1 as init, the error screen, and every scenario under `/bin/sh`; the kernel and the shell crate without the in-kernel shell's glue; `check4.sh` with NUC check 4.
2. **Process 1 is init** (§5.4, §6.6; `kernel/src/init.rs`). A process of the kernel's own, without a program. It prints `/etc/motd` once, at boot, and starts `/bin/sh` with no argument (argument 0 its path) in `/root`, or in `/` without one, chosen again at each start; in a new process group that gets the console (`NEW_GROUP | FOREGROUND`, so the next shell takes the console back whatever the last one left it in), its fds 0-2 the console. The shell is pid 2, then whatever pid is next. Init waits for it, collecting every orphan that ends meanwhile. When the shell ends init says how, on the screen and in the kernel log (`init: /bin/sh (pid 2) exited with 0; starting it again`, or `killed: <how>`), and starts another; the third end within 10 s (by the TSC's clock, the tick's without one) reaches the error screen, as does a shell that cannot be started. The rule is host-tested over a clock; `kill` still refuses process 1. Test mode (`test=1`, power-off exits QEMU) is set at startup, before process 1. The `noroot` scenario removes `/root` and sees the next shell start in `/`.
3. **The error screen** (§11.2; `kernel/src/error_screen.rs`). Init shows it, in its own context with interrupts on, so the idle task polls the keyboards and COM1 while it waits: it is no kernel bug, and not the red panic screen. The screen is cleared and shows `*** Relay OS cannot run its shell ***`, the reason (`system: <the startup line's reason>`, `/bin/sh cannot start: <error>` or `/bin/sh ended 3 times within 10 s`), the kernel log's last 20 lines (27 of the NUC's 33 rows in all when none wraps; of those, the newest that fit the console's rows, a line wider than it taking more than one and the cursor's row after the last counting too, so the heading never scrolls away: the prototype's review found the NUC's real lines wrapping), and `Press any key to reboot.` What was typed before it came (the LF of a terminal's CR LF after the last `exit`) is dropped; then it waits for a key as long as it takes, since a restart on its own would come back to it. After the key it syncs, shuts the filesystems down (a failure does not keep the machine up: nothing can be fixed here) and restarts through the ACPI path (M1 §7.4). A `system.img` that cannot be mounted (startup step 10) is recorded, and init takes the record out before it shows the screen for it, starting nothing (the review found it held the record's lock across the wait; the switch's check now covers that lock); `mount_fail`'s empty root still reaches a prompt, in `/`, without a motd. The screen takes the console back (raw mode, init's group) and wakes its readers, which no test shows: in milestone 2 the console is init's or the last shell's, in raw mode, whenever the screen comes, since nothing a person can run ends the shell while a command has the console (ruled; milestone 3's `kill` can, and its plan tests it).
4. **The kernel without the shell** (§1.4 item 3, §8.2). `relay-kernel` no longer depends on `shell`, even through another crate, which a test checks by walking `cargo metadata`'s resolved graph (the review found one that looked at direct dependencies only). `session.rs` goes (its `KernelEnv`, the filesystems' clock and log, moves to `storage.rs`), and with it `File::ShellOutput`, `FdTable::shell`, the output hook (`SHELL_OUT`, `proc::wait`), `renew_outputs`, `collect_orphans` and `give_console`; process 1's fds are the console. `take_console` stays, for the error screen, and wakes the console's readers, which plan 3b found it did not; both callers of `KernelVfs::shutdown` (`power` and the error screen) write the tees first. The shell crate's `System::spawn`, `System::wait`, `Output`, `run_program` and `Ctx::wait_program` go: the in-process runner (`host-shell`, the tests) runs no programs, and names it does not know are `command not found`.
5. **Milestone 1's scenarios under `/bin/sh`** (§1.4 item 1, §12.3). A prototype of init starting `/bin/sh` ran every scenario first: all of milestone 1's passed unchanged. What changed is later plans': `diskfull` expected a program's redirection write error to be reported by the in-kernel shell (`t-files: write error: …`), and under `/bin/sh` a program reports its own, as under bash; `t-spawn fill` finds room for 61 children, 60 under a nested shell, since the shell is a process now (`sh`, `spawn`); `sh` runs by path from the shell init started, and `utils` sees a program write lines into a file and refuse `cat f >> f`.
6. **`t-abi`** (§8.5, §12.3). xtask makes it from `t-args`, its note saying ABI 1, and checks that the build's checks and the kernel's refuse it for that alone (`built for ABI 1`); it is in `/bin`, so `system: 34 programs, ABI 2`. `stale_program` runs it from `/bin` and copied to `/root` (`relay-sh: …: Exec format error`, `spawn …: built for ABI 1` in the kernel log).
7. **Findings carried forward** (plans 3a, 3b, 4a). A program that spends its time in system calls has the console polled on its way back to ring 3 when a tick passed during the call (`t-mem churn` maps and gives back nearly all free memory over and over, and Ctrl-C must stop it within 3 s: without the poll it did not, in three runs of three); `console_foreground` of a group whose members are all zombies is `ESRCH`; `open` with `CREATE` of a directory is `EISDIR`, as on Linux; what a tee may be has a host test; a write that takes nothing is `ENOSPC` in `relay-rt`, as gnulib's `full_write` says (and `shell::Stdout`). `reboot -f` and `poweroff -f` ask `power` without `POWER_FORCE` first, so they say milestone 1's `cannot shut the filesystems down cleanly: …` before going ahead (plan 4a's final review), and `unplug` runs both to the end.
8. **The e2e runner** (§10). `reset [TEXT]` types the text and Enter (a key at the error screen, `reboot -f`), and the machine must restart as after `reboot`; after `unplug` the root's clean flag is not checked, since the disk keeps what it had when it was pulled out; an `expect` after `poweroff` reads what the machine printed before it went away.
9. **Check scripts** (§12.4). `#same> NAME REGEX` is a line whose group must capture what it captured the first time (`free` before and after). `check4.sh` (33 commands) runs after `check3-b.sh`: `t-spawn fill` and `1000`, `free`, every `t-fault` kind, `t-spawn kill-new` and `orphan`, `t-args`, `t-abi`, `ls /bin` into a file and `cat` of a file into itself, a script that runs another, and `free` again; then `exit` by hand, which restarts the shell, and `poweroff`. `check3-a.sh` and `check3-b.sh` run under `/bin/sh` unchanged, their transcripts tees. The recorded NUC transcript of `check4.sh` is QEMU's until NUC check 4 records the NUC's. §14's measurement of the tick poll's cost "with and without" is not check 4's (§12.4 and §14, corrected in their bodies): `t-spin`'s count with the poll is in check 3's and check 4's transcripts, and a count without it needs a kernel without it, which plan 5 builds if those counts call for it.
10. **Left to plan 5**: the NUC check's fixes, the tick poll's cost without the poll if the NUC's counts call for it, `flush_pages` outside `arch/`, the guest tests' unbounded loops, `read_dir`'s heap measurement, and milestone 1's plan-5 findings (roadmap). A `/bin/sh` that cannot be started reaches the error screen only in a host test (no scenario builds an archive without it). The console calls refusing a caller outside the foreground group wait for milestone 3.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **The shell ending:** `exit` with and without a code, three quick `exit`s, a shell that is killed, a shell that cannot start, a clock that stands still, a terminal that sends CR LF after the last `exit`, a key typed before the error screen came, `mount_fail`'s empty root (no `/root`, no motd). Expected: init's line with the status and at once a new prompt in `/root`, without the motd; pids counting on; the error screen after the third end within 10 s, which waits for a key typed after it, then restarts the machine with its filesystems shut down; a prompt in `/` on the empty root, and in `/` after `/root` was removed. Tests: the `respawn` scenario, with `send-crlf exit` and `alive 2` (Task 11), `the_shell_is_started_again_until_it_ends_three_times_within_10_s`, `a_clock_that_stands_still_stops_it_at_the_third_end`, `init_says_how_the_shell_ended` (Task 7), `mount_fail` (Task 11), `noroot` (Task 13).
2. **A machine without its programs:** no `system.img`, an empty one, one of another ABI, a damaged one; a kernel log whose last line was cut; the NUC's 33 rows. Expected: the startup line, then the error screen naming the same reason, the log's last lines without their colours, and `Press any key to reboot.`; a key restarts the machine, which comes back to the same screen. Tests: `system_missing` (with `alive 2` before the key), `system_abi`, `system_empty` (Task 11), `the_screen_says_why_shows_the_log_and_what_to_do`, `each_reason_is_named`, `it_fits_the_nuc_s_33_rows` (Task 8), `the_screen_never_scrolls_its_heading_away` with the NUC's real lines (Task 9), `system_missing` with the switch's lock check (Task 12), `parses_reboot_and_poweroff_steps` (Task 10).
3. **Milestone 1's commands and scripts, now all under `/bin/sh`:** a program's write error into a redirection, `ls` into a file, `cat f >> f`, Ctrl-C of a program that spends its time in system calls, orphans that pass to init (`t-spawn fill` twice, `t-spawn orphan`), `reboot -f` and `poweroff -f` after the stick was pulled out, `sync failed` after it. Expected: milestone 1's messages and statuses, `^C` within a second, the table as empty the second time, and milestone 1's `cannot shut the filesystems down cleanly: …` before the restart or power-off. Tests: every milestone 1 scenario (Task 11), `diskfull`, `utils`, `sh`, `spawn` (Task 11), `ctrlc` with `t-mem churn` (Task 5), `unplug` and `reboot_and_poweroff_say_why_the_machine_stayed_up` (Task 14), `stale_program` (Task 6).
4. **The kernel without the in-kernel shell:** a program's fds 0-2 that are the console (read, write, `fstat`, `seek`, a tee of the console), `console_foreground` of a group of zombies, `open(READ|CREATE)` of a directory, `kill 1`, a write that takes nothing, a name the in-process runner does not know. Expected: the console a character device, `EINVAL` for a seek or a tee, `ESRCH`, `EISDIR`, `EPERM`, `ENOSPC`, and `command not found` on the host. Tests: `process_1_has_the_console_as_0_1_and_2` (Task 11), `write_copies_the_buffer_to_the_fd`, `the_kernel_does_not_depend_on_the_shell` (Tasks 15 and 16), `a_group_of_zombies_is_no_group_to_give_the_console` (Task 1), `open_s_flags_are_checked` (Task 2), `a_tee_is_a_file_open_for_writing` (Task 3), `a_write_that_takes_nothing_is_enospc` (Task 4), `the_in_process_runner_runs_no_programs` (Task 17).
5. **NUC check 4 and its checker:** a `#same>` value that changed, a `#same>` without a group or without a name, `check4.sh` run a second time, an orphan's output landing in the next command's, `t-spawn fill` beside two shells, `free` while a program still runs. Expected: `verify-usb` naming the command and both values; the script starting afresh; `33 of 33 commands as expected`. Tests: `a_value_must_be_what_it_was_the_first_time` (Task 18), `the_check_scripts_pass_on_both_machines` and the `checks` scenario (Task 19).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `kernel/src/init.rs` | Process 1: the respawn rule and log line; the motd, starting `/bin/sh`, waiting and collecting orphans |
| `kernel/src/error_screen.rs` | The error screen: its reasons and text; waiting for a key, syncing and restarting |
| `kernel/src/{lib,system,proc,fd,tty,storage,mounts,file}.rs`, `kernel/src/proc/table.rs`, `kernel/src/syscall.rs`, `kernel/src/syscall/{files,testing}.rs`, `kernel/Cargo.toml` | The boot sequence ending in init; the archive's failure returned; the poll on the way back to ring 3, `wait_collecting`, `take_console`; the console as process 1's fds; `has_group`; `tee_target`; `KernelEnv`; `open`'s `EISDIR`; no shell |
| `crates/relay-rt/src/sys.rs` | `write_all_by`, `ENOSPC` |
| `crates/shell/src/{io,ctx,runner,shell,testing,lib,program}.rs`, `crates/shell/src/commands/system.rs` | No `System::spawn`/`wait`; `reboot -f`/`poweroff -f` ask `power` without `-f` first |
| `userland/tests/src/bin/t-mem.rs` | `t-mem churn` |
| `xtask/src/{userland,e2e,checks,ci}.rs` | `t-abi`; the step `reset`, the unplugged disk, `expect` after `poweroff`; `#same>`; the kernel without `shell` |
| `tests/e2e/*.txt`, `rootfs/root/checks/check4.sh`, `xtask/fixtures/checks/check4.*.log` | `respawn`, `stale_program`, the `system_*` scenarios' error screen, every scenario under `/bin/sh`; NUC check 4 |
| `README.md`, `docs/hardware-test.md` | Process 1 and the error screen; NUC check 4 |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 6 (with the parts of its body it corrects: §4.3's startup line, §6.6's process 1, §8.5's `t-mem`, §11.2's error screen), the roadmap's row for plan 4b and its note "What plan 4b leaves for plan 5", and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 4b, and the roadmap's notes for it`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4b/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p4b/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-4b-process-1.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-30-m2-plan-4b-process-1.md
git commit -m "docs: plan 4b of milestone 2, process 1"
cargo xtask lint
git push -u origin m2p4b/plan
gh pr create --base main --head m2p4b/plan --title "Milestone 2, plan 4b: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 4b ("process 1"), the spec's §16 item 6 with its decisions (among them: process 1 is the kernel's init, which starts `/bin/sh` and starts it again, and gives up after three ends within 10 s; the error screen, shown by init, waits for a key and restarts the machine; the kernel drops `shell`; every milestone 1 scenario unchanged under `/bin/sh`; `t-abi`; `#same>` and `check4.sh`), the corrections to the spec's body they bring, and the roadmap's row and notes for plans 4b and 5.

## How it was tested

- [x] Every task of plan 4b was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p4b/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p4b/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-plan` and continue with PR 2.

---

## PR 2: The kernel's findings carried forward, and `t-abi` (Tasks 1–6)

What plans 3a, 3b and 4a left for plan 4b that needs no init (roadmap, "What plan 4a leaves for plan 4b"): `console_foreground` of a group of zombies, `open` with `CREATE` of a directory, a host test of what a tee may be, `ENOSPC` for a write that takes nothing, the console polled on the way back to ring 3; and `t-abi` with the `stale_program` scenario. The in-kernel shell is still process 1.

Branch `m2p4b/kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4b/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4b/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4b/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-kernel m2p4b/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4b/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: `console_foreground` of a group whose members are all zombies is `ESRCH`

Plan 3b's finding, carried forward (roadmap, "What plan 4a leaves for plan 4b"): `console_foreground` accepted any group some entry of the table was in, zombies included, so the console could go to a group nothing of which can read it again. `Table::has_group(pgid)` is true only while a process of the group has not ended, and the call uses it. Mutation check: a group of zombies counted fails the test.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`

**Interfaces:**
- Consumes: plan 3a's process table (`kernel/src/proc/table.rs`).
- Produces: `Table::has_group(&self, pgid: u32) -> bool`.

- [ ] **Step 1: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
        assert_eq!(t.get(zombie).unwrap().killed, None);
    }
}
````

with:

````rust
        assert_eq!(t.get(zombie).unwrap().killed, None);
    }

    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
        let mut t = table();
        add(&mut t, 0, true);
        let leader = add(&mut t, 1, true);
        let other = add(&mut t, leader, false);
        assert!(t.has_group(leader));
        assert!(!t.has_group(99), "nobody's group");
        assert!(!t.has_group(0));
        t.end(leader, WaitStatus::exited(0));
        assert!(t.has_group(leader), "one of it still runs");
        t.end(other, WaitStatus::exited(0));
        assert!(!t.has_group(leader), "only zombies are left of it");
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `has_group` found for struct `table::Table<R>` in the current scope ``.

- [ ] **Step 3: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        let mut t = PROCS.lock();
        if !t.iter().any(|p| p.pgid == pgid) {
            return Err(Errno::ESRCH);
````

with:

````rust
        let mut t = PROCS.lock();
        if !t.has_group(pgid) {
            return Err(Errno::ESRCH);
````

- [ ] **Step 4: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
        self.procs.iter_mut()
    }
````

with:

````rust
        self.procs.iter_mut()
    }

    /// Whether a process of group `pgid` still runs (or waits): the console
    /// can be given to it. A group whose members are all zombies is none,
    /// since nothing of it can read the console again.
    pub fn has_group(&self, pgid: u32) -> bool {
        self.procs
            .iter()
            .any(|p| p.pgid == pgid && !matches!(p.state, State::Zombie(_)))
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 342 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "Kernel: console_foreground of a group whose members are all zombies is ESRCH"
````


### Task 2: `open` with `CREATE` of a directory is `EISDIR`, as on Linux

Plan 3b's finding, carried forward: `open(READ | CREATE)` of a directory that exists opened it, as if `CREATE` meant nothing there; Linux refuses it with `EISDIR` (checked with Python's `os.open(d, O_RDONLY | O_CREAT)` on this machine), since `open` never creates a directory. The refusal of a directory opened for writing now covers `CREATE` too. Mutation check: `CREATE` dropped from the check fails the test.

**Files:**
- Modify: `kernel/src/file.rs`

**Interfaces:**
- Consumes: plan 3b's `file::open` (`kernel/src/file.rs`).
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f", OPEN_READ | OPEN_DIRECTORY).err(),
````

with:

````rust
            Some(Errno::EISDIR)
        );
        // A directory is never created by `open`, so asking for that of one
        // is refused as Linux refuses it.
        assert_eq!(
            open(&mut t, b"/root", OPEN_READ | OPEN_CREATE).err(),
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f", OPEN_READ | OPEN_DIRECTORY).err(),
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `file::tests::open_s_flags_are_checked`.

- [ ] **Step 3: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
    let dir = !created && vfs.stat(node)?.kind == FileType::Directory;
    if dir && has(OPEN_WRITE) {
        return Err(Errno::EISDIR);
````

with:

````rust
    let dir = !created && vfs.stat(node)?.kind == FileType::Directory;
    // Linux's rule: a directory is neither written nor created by `open`.
    if dir && (has(OPEN_WRITE) || has(OPEN_CREATE)) {
        return Err(Errno::EISDIR);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 342 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "Kernel: open with CREATE of a directory is EISDIR, as on Linux"
````


### Task 3: A host test of what a console tee may be

Plan 3b's finding, carried forward: `tty::push_tee`'s checks (a file of the VFS open for writing; `EBADF` for one that is not, `EINVAL` for the console and the in-kernel shell's outputs) had no host test, since `push_tee` also pushes onto the kernel's one tee stack. They move into `tee_target`, which the new test calls over a `MountTable`. The task only moves the checks and adds a test of correct code, so it has no failing run; the mutation checks show what it guards: a read-only file or the console accepted each fail it.

**Files:**
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: plan 3b's `tty::push_tee`, `file::open`.
- Produces: `tty::tee_target(&File) -> Result<(), Errno>` (private).

- [ ] **Step 1: Add the failing tests to `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
    INPUT.is_locked()
}
````

with:

````rust
    INPUT.is_locked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file;
    use alloc::boxed::Box;
    use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_WRITE};
    use vfs::{Env, MemFs, MountTable};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    #[test]
    fn a_tee_is_a_file_open_for_writing() {
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock))));
        let open = |t: &mut MountTable, flags| File::Vfs(file::open(t, b"/log", flags).unwrap());
        let written = open(&mut t, OPEN_WRITE | OPEN_CREATE);
        let read_write = open(&mut t, OPEN_READ | OPEN_WRITE);
        let read_only = open(&mut t, OPEN_READ);
        assert_eq!(tee_target(&written), Ok(()));
        assert_eq!(tee_target(&read_write), Ok(()));
        assert_eq!(tee_target(&read_only), Err(Errno::EBADF));
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
        assert_eq!(tee_target(&File::ShellOutput(1)), Err(Errno::EINVAL));
    }
}
````

- [ ] **Step 2: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
    match &*file {
        File::Vfs(open) if open.is_writable() => {}
        File::Vfs(_) => return Err(Errno::EBADF),
        File::Console | File::ShellOutput(_) => return Err(Errno::EINVAL),
    }
    TEES.lock().push(owner, file)
}
````

with:

````rust
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
    tee_target(&file)?;
    TEES.lock().push(owner, file)
}

/// Whether `file` can be a tee: a file of the VFS open for writing
/// (`EBADF` otherwise), not the console or the shell's outputs (`EINVAL`).
fn tee_target(file: &File) -> Result<(), Errno> {
    match file {
        File::Vfs(open) if open.is_writable() => Ok(()),
        File::Vfs(_) => Err(Errno::EBADF),
        File::Console | File::ShellOutput(_) => Err(Errno::EINVAL),
    }
}
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 343 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add kernel
git commit -m "Kernel: a host test of what a console tee may be"
````


### Task 4: A write that takes nothing is `ENOSPC` in `relay-rt`

Plan 4a's deferred minor: `sys::write_all` reported a write that took nothing as `EIO`, while `shell::Stdout`'s contract (and gnulib's `full_write`, which coreutils use) says `ENOSPC`, so a program's `SysStdout` could print `write error: Input/output error` for a full disk. With ext2 it does not happen (a full disk gives a short count, then `ENOSPC`), but a filesystem that answers 0 would. The loop moves into `write_all_by`, over any write function, which the new test drives. Mutation check: `EIO` again fails the test.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`

**Interfaces:**
- Consumes: plan 3a's `sys::write`.
- Produces: `sys::write_all_by(write: impl FnMut(&[u8]) -> Result<usize, u16>, bytes: &[u8]) -> Result<(), u16>` (private); `write_all` over it.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
        write_all(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}
````

with:

````rust
        write_all(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use relay_abi::errno::{EIO, ENOSPC};

    #[test]
    fn a_write_that_takes_nothing_is_enospc() {
        let mut got = Vec::new();
        let mut room = 5;
        let r = write_all_by(
            |b| {
                let n = b.len().min(room).min(3);
                room -= n;
                got.extend_from_slice(&b[..n]);
                Ok(n)
            },
            b"abcdefgh",
        );
        assert_eq!(r, Err(ENOSPC));
        assert_eq!(got, b"abcde", "everything that fitted, in pieces");
        assert_eq!(
            write_all_by(|_| Err(EIO), b"x"),
            Err(EIO),
            "the call's own error"
        );
        assert_eq!(write_all_by(|b| Ok(b.len()), b"all"), Ok(()));
        assert_eq!(write_all_by(|_| Ok(0), b""), Ok(()), "nothing to write");
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find function `write_all_by` in this scope ``.

- [ ] **Step 3: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
/// Writes all of `bytes` to `fd`, however many calls that takes.
pub fn write_all(fd: u32, mut bytes: &[u8]) -> Result<(), u16> {
    while !bytes.is_empty() {
        match write(fd, bytes)? {
            0 => return Err(relay_abi::errno::EIO),
            n => bytes = &bytes[n.min(bytes.len())..],
````

with:

````rust
/// Writes all of `bytes` to `fd`, however many calls that takes.
pub fn write_all(fd: u32, bytes: &[u8]) -> Result<(), u16> {
    write_all_by(|b| write(fd, b), bytes)
}

/// Writes all of `bytes` through `write`; a write that takes nothing is
/// `ENOSPC`, as gnulib's `full_write` says of one (and `shell::Stdout`).
fn write_all_by(
    mut write: impl FnMut(&[u8]) -> Result<usize, u16>,
    mut bytes: &[u8],
) -> Result<(), u16> {
    while !bytes.is_empty() {
        match write(bytes)? {
            0 => return Err(relay_abi::errno::ENOSPC),
            n => bytes = &bytes[n.min(bytes.len())..],
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 26 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "relay-rt: a write that takes nothing is ENOSPC, as shell::Stdout says"
````


### Task 5: A program that spends its time in system calls has the console polled on its way back to ring 3

Plan 3a's note, carried forward: the console is polled by the ticks that interrupt ring 3 and by the idle task, and a tick that interrupts the kernel only counts. A program that spends nearly all its time in system calls (`cat` and `cp` now run in ring 3) is almost never interrupted in ring 3, and the idle task never runs while it is ready: a Ctrl-C then waits, and a real HID keyboard with one transfer outstanding can lose a key meanwhile. `proc::before_user` now polls the console, and acts on what came (a Ctrl-C, a reader to wake), when a tick passed during the call. `t-mem churn` maps and gives back nearly all free memory over and over (at most 20 s), zeroing and freeing pages in the kernel almost all the time, and the `ctrlc` scenario expects Ctrl-C to stop it within 3 s. The red run is the scenario's: without the poll it timed out in three runs of three (64 MiB per call was not enough; nearly all of memory is).

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `userland/tests/src/bin/t-mem.rs`

**Interfaces:**
- Consumes: plan 3a's `proc::{before_user, console_input, KERNEL_TICKS}`, `tty::poll`.
- Produces: `t-mem churn`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, replace:

````text
expect \npid \d+ \(/bin/t-spin\): killed: Ctrl-C\n
# A Ctrl-C typed before the command has even started is the command's too:
````

with:

````text
expect \npid \d+ \(/bin/t-spin\): killed: Ctrl-C\n
# A program that spends its time in system calls, where a tick only
# counts, is polled on its way back to ring 3 after a tick passed: Ctrl-C
# stops it too, although a tick almost never finds it in ring 3.
send t-mem churn
alive 1
key {ctrl-c}
timeout 3
expect \n\^C\nroot@relay:~# 
timeout 30
send dmesg
expect \npid \d+ \(/bin/t-mem\): killed: Ctrl-C\n
# A Ctrl-C typed before the command has even started is the command's too:
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-mem.rs`**

In `userland/tests/src/bin/t-mem.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!   none (`t-mem: out of memory`), and says how it ended: status 134.
#![no_std]
````

with:

````rust
//!   none (`t-mem: out of memory`), and says how it ended: status 134.
//! - `t-mem churn`: maps and gives back nearly all free memory over and
//!   over for at most 20 s, so that it spends its time in the kernel,
//!   zeroing and freeing pages, and almost never in ring 3; then `churn:
//!   not stopped`. Ctrl-C must stop it all the same.
#![no_std]
````

Replace:

````rust
        Some(b"hog") => hog(),
        _ => return usage(),
````

with:

````rust
        Some(b"hog") => hog(),
        Some(b"churn") => churn(),
        _ => return usage(),
````

Replace:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-mem map|unmapped|unmapped-many|grow N|oom\n");
    2
````

with:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-mem map|unmapped|unmapped-many|grow N|oom|churn\n",
    );
    2
````

Replace:

````rust

/// Takes the heap a mebibyte at a time until `relay-rt` says it is out.
````

with:

````rust

/// How long `churn` goes on if nothing stops it.
const CHURN_NS: u64 = 20_000_000_000;

fn churn() -> Result<(), u16> {
    // As much as leaves 16 MiB (8 MiB above the kernel's reserve), at most
    // 1 GiB: each call zeroes or frees it all, for tens of milliseconds.
    let len = (free()? * 4096).saturating_sub(16 << 20).min(1 << 30) as usize & !(PAGE - 1);
    let start = sys::time()?.uptime_ns;
    loop {
        for _ in 0..16 {
            let a = sys::mem_map(len)?;
            // SAFETY: nothing uses it.
            unsafe { sys::mem_unmap(a, len)? };
        }
        if sys::time()?.uptime_ns - start >= CHURN_NS {
            break;
        }
    }
    let _ = writeln!(Fd(1), "churn: not stopped");
    Ok(())
}

/// Takes the heap a mebibyte at a time until `relay-rt` says it is out.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: FAIL: scenario `ctrlc` stops at line 30, timed out waiting for `\n\^C\nroot@relay:~#`.

- [ ] **Step 4: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
/// are counted, the program gives up the CPU if its slice is used up, and
/// it ends if it was killed meanwhile.
pub fn before_user() {
    let used_up = settle_ticks(&mut PROCS.lock(), 0);
    if used_up {
````

with:

````rust
/// are counted, the program gives up the CPU if its slice is used up, and
/// it ends if it was killed meanwhile. If a tick passed during the call,
/// the console is polled as that tick would have polled it in ring 3: a
/// program that spends its time in system calls (`cat`, `cp`) is almost
/// never interrupted in ring 3, and a keyboard with one transfer
/// outstanding could lose a key meanwhile (plan 3a's finding).
pub fn before_user() {
    let ticked = KERNEL_TICKS.load(Ordering::Relaxed) != 0;
    if ticked {
        tty::poll();
    }
    let used_up = {
        let mut t = PROCS.lock();
        if ticked {
            console_input(&mut t);
        }
        settle_ticks(&mut t, 0)
    };
    if used_up {
````

- [ ] **Step 5: Run the `ctrlc`, `memory` scenarios**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel tests userland
git commit -m "Kernel: a program that spends its time in system calls has the console polled on its way back to ring 3, so Ctrl-C stops it; t-mem churn"
````


### Task 6: `/bin` holds `t-abi`, `t-args` built for ABI 1; the `stale_program` scenario

Spec §8.5, §10, §12.3 and decision 6: `t-abi`, the program whose note has another ABI's version, was never built. xtask now makes it from `t-args`, changing the note's version to 1 (`STALE_ABI`, the ABI before this one, as a program an older build left on a disk would have), and checks that both the build's check (`readelf`) and the kernel's refuse it for that reason alone (`built for ABI 1`); `build_system_image` puts it in `/bin`, which now holds 34 programs. The `stale_program` scenario runs it from `/bin` and copied to `/root`: `relay-sh: …: Exec format error`, the kernel log's `spawn …: built for ABI 1`, and the shell carries on. `system` lists the new `/bin`. The red run is xtask's test, which cannot find the new functions.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`
- Create: `tests/e2e/stale_program.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: plan 1's `userland::{build, check_program, kernel_check, system_image}`.
- Produces: `userland::{STALE_ABI, with_note_abi(&[u8], u32) -> Result<Vec<u8>>, with_stale_program(Vec<Program>) -> Result<Vec<Program>>}`; `t-abi` in `/bin`.

- [ ] **Step 1: Add the scenario `tests/e2e/stale_program.txt`**

Create `tests/e2e/stale_program.txt`:

````text
# A program built for another ABI (user-space gate §5.2, §8.5, §12.3):
# t-abi is t-args whose note says ABI 1, as a program an older build left
# on the disk would. spawn refuses it with ENOEXEC, from /bin or copied to
# /root, the kernel log says why, and the shell carries on.
timeout 30
expect root@relay:~# $
send cp /bin/t-abi /root/t-abi
send /root/t-abi
expect \nrelay-sh: /root/t-abi: Exec format error\n
send t-abi
expect \nrelay-sh: t-abi: Exec format error\n
send dmesg
expect \nspawn /root/t-abi: built for ABI 1\n
send t-args still
expect \n\[1\] still\n
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat    t-fault  t-read   t-sys  touch\nclear  df     free  mkdir  pwd       rmdir  sync    t-files  t-spawn  t-tee  uname\ncp     dmesg  head  mv     reboot    sh     t-args  t-mem    t-spin   tail   wc\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem    t-spin  tail   wc\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-read   t-sys   touch\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-spawn  t-tee   uname\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat    t-fault  t-read   t-sys  touch\nclear  df     free  mkdir  pwd       rmdir  sync    t-files  t-spawn  t-tee  uname\ncp     dmesg  head  mv     reboot    sh     t-args  t-mem    t-spin   tail   wc\n
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem    t-spin  tail   wc\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-read   t-sys   touch\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-spawn  t-tee   uname\n
````

- [ ] **Step 3: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
    #[test]
    fn the_system_image_holds_every_program() {
````

with:

````rust
    #[test]
    fn t_abi_is_t_args_built_for_the_abi_before() {
        let programs = with_stale_program(build().unwrap()).unwrap();
        let path = |name: &str| &programs.iter().find(|p| p.name == name).unwrap().path;
        let (t_args, t_abi) = (
            fs::read(path("t-args")).unwrap(),
            fs::read(path("t-abi")).unwrap(),
        );
        assert_eq!(t_abi.len(), t_args.len());
        let differ: Vec<usize> = (0..t_abi.len())
            .filter(|&i| t_abi[i] != t_args[i])
            .collect();
        assert_eq!(differ.len(), 1, "one byte of the version: 2 -> 1");
        assert_eq!(STALE_ABI, 1);
        let e = check_program(path("t-abi")).unwrap_err().to_string();
        assert!(e.contains("built for ABI 1, this is ABI 2"), "{e}");
        assert_eq!(
            kernel_check(path("t-abi")).unwrap_err().to_string(),
            "the kernel's check: built for ABI 1"
        );
        let names: Vec<&str> = programs.iter().map(|p| p.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "still sorted, as system.img wants");
        assert!(with_note_abi(b"no note here", 1).is_err());
    }

    #[test]
    fn the_system_image_holds_every_program() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask userland`

Expected: FAIL: compile errors such as `` cannot find value `STALE_ABI` in this scope ``; `` cannot find function `with_stale_program` in this scope ``.

- [ ] **Step 5: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 33 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 34 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 6: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 33 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.
````

- [ ] **Step 7: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust

/// Builds the programs and writes `target/relay/system.img`.
pub fn build_system_image() -> Result<PathBuf> {
    let image = system_image(&build()?)?;
    fs::create_dir_all(out_dir())?;
````

with:

````rust

/// The ABI `t-abi` is built for: the one before this (spec §8.5), as a
/// program left on a disk by an older build would be.
pub const STALE_ABI: u32 = relay_abi::VERSION - 1;

/// `program`'s bytes with its `Relay` note saying ABI `abi`: the note's
/// name, padded to 8 bytes, then its 4-byte version, which must occur
/// exactly once.
pub fn with_note_abi(program: &[u8], abi: u32) -> Result<Vec<u8>> {
    let mut note = b"Relay\0\0\0".to_vec();
    note.extend_from_slice(&relay_abi::VERSION.to_le_bytes());
    let mut at = program
        .windows(note.len())
        .enumerate()
        .filter(|(_, w)| *w == note)
        .map(|(i, _)| i);
    let (Some(at), None) = (at.next(), at.next()) else {
        bail!("the program does not hold its Relay note exactly once");
    };
    let mut bytes = program.to_vec();
    bytes[at + 8..at + 12].copy_from_slice(&abi.to_le_bytes());
    Ok(bytes)
}

/// `programs` and `t-abi`, `t-args` built for [`STALE_ABI`] (spec §8.5),
/// sorted by name: the program `spawn` must refuse with `ENOEXEC`, which
/// the build checks both its checks refuse for that reason alone.
pub fn with_stale_program(mut programs: Vec<Program>) -> Result<Vec<Program>> {
    let t_args = programs
        .iter()
        .find(|p| p.name == "t-args")
        .context("t-args is not built")?;
    let path = t_args.path.with_file_name("t-abi");
    fs::write(&path, with_note_abi(&fs::read(&t_args.path)?, STALE_ABI)?)?;
    let why = format!("built for ABI {STALE_ABI}");
    for e in [check_program(&path).err(), kernel_check(&path).err()] {
        let e = e.map(|e| e.to_string()).unwrap_or_default();
        ensure!(
            e.contains(&why),
            "t-abi is refused for another reason: {e:?}"
        );
    }
    programs.push(Program {
        name: "t-abi".into(),
        path,
    });
    programs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(programs)
}

/// Builds the programs and writes `target/relay/system.img`.
pub fn build_system_image() -> Result<PathBuf> {
    let image = system_image(&with_stale_program(build()?)?)?;
    fs::create_dir_all(out_dir())?;
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 7 tests.

- [ ] **Step 9: Run the `stale_program`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario stale_program`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add docs kernel tests xtask
git commit -m "Build: /bin holds t-abi, t-args built for ABI 1, and the stale_program scenario runs it"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 38 scenario(s) passed`.

````bash
git push -u origin m2p4b/kernel
gh pr create --base main --head m2p4b/kernel --title "Milestone 2, plan 4b: The kernel's findings carried forward, and t-abi" --body-file - <<'EOF'
## What

Milestone 2, plan 4b, tasks 1–6: `console_foreground` of a group whose members are all zombies is `ESRCH`; `open` with `CREATE` of a directory is `EISDIR`, as on Linux; a host test of what a console tee may be; a write that takes nothing is `ENOSPC` in `relay-rt`; a program that spends its time in system calls has the console polled on its way back to ring 3, so Ctrl-C stops it (`t-mem churn`, the `ctrlc` scenario); `/bin` holds `t-abi`, `t-args` built for ABI 1, which `stale_program` runs.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing the NUC runs changes but what the QEMU scenarios cover; NUC check 4 (the last pull request of this plan) runs it all
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4b/kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Process 1 is init (Tasks 7–14)

Init and its rule, the error screen, the e2e step `reset`, init as process 1 with every scenario under `/bin/sh`, and `reboot -f`/`poweroff -f` saying why before going ahead. The in-kernel shell no longer runs; its glue goes in PR 4.

Branch `m2p4b/init`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-init`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4b/init /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-init origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-init
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4b/kernel` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4b/init /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-init m2p4b/kernel`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4b/kernel>` and re-run `cargo xtask ci` before pushing.

### Task 7: Init's rule for starting the shell again, and what it says when the shell ends

Spec §6.6 and decision 2: `kernel/src/init.rs` begins with the part of process 1 that needs no machine: `Respawn`, which remembers when the shell last ended and says whether to start it again (not after its third end within 10 s, `ENDS` and `WINDOW`), and `ended_line`, what init says: `init: /bin/sh (pid 2) exited with 0; starting it again`, or `killed: <how>` in the words of `WaitStatus`'s `Display`. A clock that stands still gives up at the third end. Task 11 runs them. Mutation checks: the window closed (`>=`), the ends not moved on, a first end refused, and "starting it again" said when it is not each fail a test.

**Files:**
- Create: `kernel/src/init.rs`
- Modify: `kernel/src/lib.rs`

**Interfaces:**
- Consumes: `relay_abi::WaitStatus` and its `Display`.
- Produces: `init::{SHELL, ENDS, WINDOW, Respawn::ended(&mut self, now: Duration) -> bool, ended_line(pid: u32, status: &WaitStatus, again: bool) -> String}`.

- [ ] **Step 1: Write the failing tests for `kernel/src/init.rs`**

Create `kernel/src/init.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_KILL};

    fn s(secs: u64, ms: u64) -> Duration {
        Duration::from_secs(secs) + Duration::from_millis(ms)
    }

    #[test]
    fn the_shell_is_started_again_until_it_ends_three_times_within_10_s() {
        let mut r = Respawn::default();
        assert!(r.ended(s(0, 0)), "a first end");
        assert!(r.ended(s(1, 0)), "a second");
        assert!(!r.ended(s(10, 0)), "a third, 10 s after the first");
        let mut r = Respawn::default();
        assert!(r.ended(s(0, 0)));
        assert!(r.ended(s(9, 0)));
        assert!(r.ended(s(10, 1)), "just over 10 s after the first");
        assert!(
            r.ended(s(19, 500)),
            "the first is forgotten: 9.5 s after the second"
        );
        assert!(
            !r.ended(s(20, 0)),
            "three within 10 s: 9, 10.001 and 19.5 ... 20"
        );
    }

    #[test]
    fn ends_far_apart_never_stop_it() {
        let mut r = Respawn::default();
        for i in 0..100 {
            assert!(r.ended(s(i * 6, 0)), "end {i}");
        }
    }

    #[test]
    fn a_clock_that_stands_still_stops_it_at_the_third_end() {
        let mut r = Respawn::default();
        assert!(r.ended(Duration::ZERO));
        assert!(r.ended(Duration::ZERO));
        assert!(!r.ended(Duration::ZERO));
    }

    #[test]
    fn init_says_how_the_shell_ended() {
        assert_eq!(
            ended_line(2, &WaitStatus::exited(0), true),
            "init: /bin/sh (pid 2) exited with 0; starting it again"
        );
        assert_eq!(
            ended_line(7, &WaitStatus::killed(KILLED_KILL), true),
            "init: /bin/sh (pid 7) killed: kill; starting it again"
        );
        assert_eq!(
            ended_line(
                3,
                &WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x401000),
                false
            ),
            "init: /bin/sh (pid 3) killed: page fault at 0x0, read, ip 0x401000"
        );
        assert_eq!(
            ended_line(4, &WaitStatus::exited(3), false),
            "init: /bin/sh (pid 4) exited with 3"
        );
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod file;
pub mod input;
````

with:

````rust
pub mod file;
pub mod init;
pub mod input;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Duration` in this scope ``; `` cannot find type `Respawn` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/init.rs`**

Insert this at the top of `kernel/src/init.rs`, above `#[cfg(test)]`:

````rust
//! Process 1, init (user-space gate §6.6, §16 item 6): a process of the
//! kernel's own, without a program. It prints `/etc/motd`, starts
//! `/bin/sh` in `/root` (or `/` without one) as its child, collects every
//! orphan as it ends, and when the shell ends says how and starts another.
//! Three ends within 10 s, or a shell that cannot be started, reach the
//! error screen instead.

use alloc::format;
use alloc::string::String;
use core::time::Duration;
use relay_abi::WaitStatus;
use relay_abi::wait::EXITED;

/// The program init starts.
pub const SHELL: &str = "/bin/sh";
/// How many ends of the shell within [`WINDOW`] make init give up.
pub const ENDS: usize = 3;
/// See [`ENDS`].
pub const WINDOW: Duration = Duration::from_secs(10);

/// When the shell ended lately, to tell whether to start it again (spec
/// §6.6): not after its [`ENDS`]th end within [`WINDOW`], since a shell
/// that ends at once would otherwise be started for ever.
#[derive(Debug, Default)]
pub struct Respawn {
    /// The last ends' times, oldest first.
    ends: [Option<Duration>; ENDS],
}

impl Respawn {
    /// The shell ended at `now` (time since the machine started); whether
    /// to start it again.
    pub fn ended(&mut self, now: Duration) -> bool {
        self.ends.rotate_left(1);
        self.ends[ENDS - 1] = Some(now);
        match self.ends[0] {
            Some(first) => now.saturating_sub(first) > WINDOW,
            None => true,
        }
    }
}

/// What init says when the shell `pid` ended with `status`, on the screen
/// and in the kernel log: `init: /bin/sh (pid 2) exited with 0; starting
/// it again`, or `killed: <how>` for a shell that was killed.
pub fn ended_line(pid: u32, status: &WaitStatus, again: bool) -> String {
    let how = if status.how == EXITED {
        format!("{status}")
    } else {
        format!("killed: {status}")
    };
    let then = if again { "; starting it again" } else { "" };
    format!("init: {SHELL} (pid {pid}) {how}{then}")
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 347 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "Kernel: init's rule for starting the shell again, and what it says when the shell ends"
````


### Task 8: The error screen

Spec §11.2 and decision 3: `kernel/src/error_screen.rs`. `Reason` is why the machine cannot run its shell (`system: <the startup line's reason>`, `/bin/sh cannot start: <error>`, `/bin/sh ended 3 times within 10 s`); `text` is the screen: `*** Relay OS cannot run its shell ***`, the reason, the kernel log's last 20 lines without their colours (a line cut short ended), and `Press any key to reboot.`, 27 of the NUC's 33 rows. `show` is what init runs (Task 11; Task 9 fits the log lines to the console's rows): the console taken back (raw, process 1's group), what was typed before the screen came dropped, the screen drawn, a key waited for as long as it takes (the idle task polls meanwhile), then `sync`, the filesystems shut down and the restart through ACPI. It is not the red panic screen: nothing is wrong with the kernel. Mutation checks: colours kept, a cut line left open, an empty log given a line, and 30 lines of log each fail a test (two of them only after the tests compared whole lines: comparing one line let them by).

**Files:**
- Create: `kernel/src/error_screen.rs`
- Modify: `kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 7's `init::{SHELL, ENDS, WINDOW}`; `system::SystemError`; `klog`, `tty`, `proc::{take_console, wait_for_input}`, `KernelVfs`, `power::reboot`.
- Produces: `error_screen::{TAIL_LINES, Reason::{System(SystemError), CannotStart(Errno), Ended}, text(&Reason, tail: &[u8]) -> Vec<u8>, show(&Reason) -> !}`.

- [ ] **Step 1: Write the failing tests for `kernel/src/error_screen.rs`**

Create `kernel/src/error_screen.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    fn lines(reason: &Reason, tail: &[u8]) -> Vec<String> {
        String::from_utf8(text(reason, tail))
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_screen_says_why_shows_the_log_and_what_to_do() {
        let tail = b"[ ok ] mount /: ext2\n[\x1b[31mFAIL\x1b[0m] system: no system.img\n";
        assert_eq!(
            lines(&Reason::System(SystemError::Missing), tail),
            [
                "*** Relay OS cannot run its shell ***",
                "",
                "system: no system.img",
                "",
                "--- last kernel log lines ---",
                "[ ok ] mount /: ext2",
                "[FAIL] system: no system.img",
                "",
                "Press any key to reboot.",
            ]
        );
        // A tail cut in the middle of its last line still ends it.
        assert_eq!(
            lines(&Reason::Ended, b"init: /bin/sh")[5..],
            ["init: /bin/sh", "", "Press any key to reboot."]
        );
        assert_eq!(
            lines(&Reason::Ended, b"")[5..],
            ["", "Press any key to reboot."],
            "no log at all"
        );
    }

    #[test]
    fn each_reason_is_named() {
        assert_eq!(
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 2"
        );
        assert_eq!(
            Reason::CannotStart(Errno::ENOEXEC).to_string(),
            "/bin/sh cannot start: Exec format error"
        );
        assert_eq!(
            Reason::Ended.to_string(),
            "/bin/sh ended 3 times within 10 s"
        );
    }

    #[test]
    fn it_fits_the_nuc_s_33_rows() {
        let mut log = klog::Ring::<4096>::new();
        for i in 0..100 {
            log.write(alloc::format!("line {i}\n").as_bytes());
        }
        let mut tail = [0u8; 4096];
        let n = log.tail_lines(TAIL_LINES, &mut tail);
        let screen = lines(&Reason::Ended, &tail[..n]);
        assert_eq!(screen.len(), 7 + TAIL_LINES);
        assert!(screen.len() <= 33);
        assert_eq!(screen[5], "line 80");
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod console;
pub mod exec;
````

with:

````rust
pub mod console;
pub mod error_screen;
pub mod exec;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Reason` in this scope ``; `` cannot find function `text` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/error_screen.rs`**

Insert this at the top of `kernel/src/error_screen.rs`, above `#[cfg(test)]`:

````rust
//! The error screen (user-space gate §11.2): what the machine shows when
//! it cannot run its shell, because `system.img` is missing, damaged or
//! of another ABI, `/bin/sh` cannot be started, or the shell ended three
//! times within 10 s. Unlike the red panic screen it is no kernel bug:
//! init shows it, in its own context, with interrupts on, so the idle task
//! polls the keyboards and COM1 meanwhile. It says why and shows the last
//! lines of the kernel log, waits for a key for as long as it takes (a
//! restart on its own would only come back here), then syncs, shuts the
//! filesystems down and restarts through ACPI (M1 §7.4).

use crate::mounts::KernelVfs;
use crate::system::SystemError;
use crate::{console, klog, power, proc, tty};
use alloc::format;
use alloc::vec::Vec;
use core::fmt;
use vfs::{Errno, Vfs};

/// Lines of the kernel log it shows: with its own 7 lines, 27 of the
/// NUC's 33 rows.
pub const TAIL_LINES: usize = 20;

/// Why the machine cannot run its shell.
#[derive(Debug, PartialEq, Eq)]
pub enum Reason {
    /// The system archive, as its startup line says.
    System(SystemError),
    /// `spawn` of `/bin/sh` failed.
    CannotStart(Errno),
    /// The shell ended three times within 10 s.
    Ended,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::System(e) => write!(f, "system: {e}"),
            Reason::CannotStart(e) => {
                write!(f, "{} cannot start: {}", crate::init::SHELL, e.message())
            }
            Reason::Ended => write!(
                f,
                "{} ended {} times within {} s",
                crate::init::SHELL,
                crate::init::ENDS,
                crate::init::WINDOW.as_secs()
            ),
        }
    }
}

/// The screen: a heading, the reason, the kernel log's last lines (`tail`,
/// without its colours) and what to do.
pub fn text(reason: &Reason, tail: &[u8]) -> Vec<u8> {
    let mut tail = tail.to_vec();
    let n = klog::strip_ansi_in_place(&mut tail);
    tail.truncate(n);
    if !tail.is_empty() && !tail.ends_with(b"\n") {
        tail.push(b'\n');
    }
    let mut out: Vec<u8> = format!(
        "*** Relay OS cannot run its shell ***\n\n{reason}\n\n--- last kernel log lines ---\n"
    )
    .into_bytes();
    out.extend_from_slice(&tail);
    out.extend_from_slice(b"\nPress any key to reboot.\n");
    out
}

/// Shows the screen for `reason`, waits for a key, and restarts the
/// machine. Only process 1 calls it.
pub fn show(reason: &Reason) -> ! {
    let mut tail = [0u8; 4096];
    let n = klog::KLOG.lock().tail_lines(TAIL_LINES, &mut tail);
    // The console is init's, in raw mode, and what was typed before the
    // screen came is not the key.
    proc::take_console();
    tty::poll();
    while tty::pop().is_some() {}
    console::write_bytes(b"\x1b[0m\x1b[2J\x1b[H");
    console::write_bytes(&text(reason, &tail[..n]));
    loop {
        tty::poll();
        if tty::pop().is_some() {
            break;
        }
        proc::wait_for_input();
    }
    // As `reboot` does (M1 §7.4): what can be written is, and a failure
    // does not keep the machine up, since nothing here can be fixed.
    let _ = KernelVfs.sync();
    let _ = KernelVfs.shutdown();
    power::reboot()
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 350 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "Kernel: the error screen says why the machine cannot run its shell, shows the kernel log's last lines, and restarts after a key"
````


### Task 9: The error screen shows the newest log lines that fit its rows

The prototype's review (minor): the error screen's text counted lines, not rows, and its test used short lines: the NUC's real last lines at a failed boot (its check 3 transcript) hold xHCI lines of 122 and 131 columns, which wrap on its 120, so the screen took more rows than decision 3 said; 20 lines as long as the longest the log holds would scroll the heading and the reason off the top. `text` now takes the console's size and keeps the newest whole lines that fit the rows left, a line counted by its characters, and the cursor's row after the last line counted too (a line there would scroll the first away). The test feeds the NUC's real lines (all 20 fit, in 29 rows), lines of 200 columns (12 fit, 32 rows), a line of two-byte characters and a console with no room for the log. Mutation checks: no fitting, the cursor's row not counted, bytes counted as columns, and one row too many allowed each fail the test.

**Files:**
- Modify: `kernel/src/error_screen.rs`

**Interfaces:**
- Consumes: Task 8's `text`.
- Produces: `error_screen::text(&Reason, tail: &[u8], size: (usize, usize)) -> Vec<u8>` (columns, rows); `rows_of`, `fitting` (private).

- [ ] **Step 1: Add the failing tests to `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    fn lines(reason: &Reason, tail: &[u8]) -> Vec<String> {
        String::from_utf8(text(reason, tail))
            .unwrap()
````

with:

````rust

    /// The NUC's console: 120 columns, 33 rows.
    const NUC: (usize, usize) = (120, 33);

    fn lines(reason: &Reason, tail: &[u8]) -> Vec<String> {
        String::from_utf8(text(reason, tail, NUC))
            .unwrap()
````

Replace:

````rust
        assert_eq!(screen[5], "line 80");
    }
}
````

with:

````rust
        assert_eq!(screen[5], "line 80");
    }

    /// The NUC's last lines before a `[FAIL] system` line, as its check 3
    /// transcript has them: some are wider than its 120 columns.
    const NUC_BOOT: &str = "\
xhci 00:14.0: slot 3: interface 1 class 224/1/1, endpoints 0x03 isochronous 0 bytes interval 1, 0x83 isochronous 0 bytes interval 1\n\
xhci 00:14.0: slot 3: no driver for this device\n\
xhci 00:14.0: port 15: connection stable after 100 ms\n\
xhci 00:14.0: port 15: reset done, SuperSpeed\n\
xhci 00:14.0: port 15: slot 4 at address 4, 0951:1666 USB 3.10, ep0 512 bytes, 1 configuration\n\
xhci 00:14.0: slot 4: interface 0 class 8/6/80, endpoints 0x81 bulk 1024 bytes interval 0, 0x02 bulk 1024 bytes interval 0\n\
xhci 00:14.0: slot 4: endpoint 0x81 configured (DCI 3, interval exponent 0)\n\
xhci 00:14.0: slot 4: endpoint 0x02 configured (DCI 4, interval exponent 0)\n\
storage: slot 4: vendor \"Kingston\", product \"DataTraveler 3.0\", revision \"PMAP\", removable\n\
storage: slot 4: 30277632 blocks of 512 bytes\n\
usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard\n\
usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard\n\
usb: 00:14.0 port 10: 8087:0033 full-speed, not claimed\n\
usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB\n\
[ ok ] usb: 2 controllers, 4 devices\n\
[ ok ] keyboard: 2 keyboards\n\
storage: 00:14.0 port 15: GPT with 2 partitions\n\
storage: root on 00:14.0 port 15, the disk with the boot partition 4A7D166A-7C33-42FF-A52D-CAE8368D7935\n\
[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB\n\
[FAIL] system: no system.img\n\
";

    /// The rows `screen` takes on a console `cols` wide, with the cursor's
    /// after its last line.
    fn rows(screen: &[u8], cols: usize) -> usize {
        String::from_utf8_lossy(screen)
            .split_terminator('\n')
            .map(|l| l.chars().count().div_ceil(cols).max(1))
            .sum::<usize>()
            + 1
    }

    #[test]
    fn the_screen_never_scrolls_its_heading_away() {
        let reason = Reason::System(SystemError::Missing);
        let screen = text(&reason, NUC_BOOT.as_bytes(), NUC);
        assert!(rows(&screen, 120) <= 33, "{}", rows(&screen, 120));
        let shown = lines(&reason, NUC_BOOT.as_bytes());
        assert_eq!(shown.len(), 7 + 20, "all 20 lines fit, two of them wrapped");
        // Lines of 200 columns take two rows each: only the newest that
        // fit are shown, whole.
        let long: String = (0..20)
            .map(|i| format!("{i:02} {}\n", "x".repeat(197)))
            .collect();
        let screen = text(&Reason::Ended, long.as_bytes(), NUC);
        assert_eq!(
            rows(&screen, 120),
            32,
            "12 lines of two rows: one row left over"
        );
        let shown = lines(&Reason::Ended, long.as_bytes());
        assert!(
            shown[5].starts_with("08 ") && shown[16].starts_with("19 "),
            "{shown:?}"
        );
        assert_eq!(shown[0], "*** Relay OS cannot run its shell ***");
        // A character is a column, however many bytes it takes.
        let wide = "é".repeat(120) + "\n";
        let screen = text(&Reason::Ended, wide.as_bytes(), (120, 9));
        assert_eq!(rows(&screen, 120), 9, "one row: it fits in exactly");
        // A console with no room for the log shows none of it.
        let screen = String::from_utf8(text(&Reason::Ended, long.as_bytes(), (80, 5))).unwrap();
        assert!(!screen.contains("xxx") && screen.ends_with("Press any key to reboot.\n"));
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `this function takes 2 arguments but 3 arguments were supplied`.

- [ ] **Step 3: Change `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// Lines of the kernel log it shows: with its own 7 lines, 27 of the
/// NUC's 33 rows.
pub const TAIL_LINES: usize = 20;
````

with:

````rust

/// Lines of the kernel log it shows at most: with its own 7 lines, 27 of
/// the NUC's 33 rows when none wraps; fewer when some do (`fitting`).
pub const TAIL_LINES: usize = 20;
````

Replace:

````rust

/// The screen: a heading, the reason, the kernel log's last lines (`tail`,
/// without its colours) and what to do.
pub fn text(reason: &Reason, tail: &[u8]) -> Vec<u8> {
    let mut tail = tail.to_vec();
````

with:

````rust

/// What follows the log lines.
const FOOTER: &[u8] = b"\nPress any key to reboot.\n";

/// The screen on a console of `size` (columns, rows): a heading, the
/// reason, the newest of the kernel log's last lines (`tail`, without its
/// colours) that fit without scrolling the heading away, and what to do.
pub fn text(reason: &Reason, tail: &[u8], size: (usize, usize)) -> Vec<u8> {
    let (cols, rows) = size;
    let mut tail = tail.to_vec();
````

Replace:

````rust
    .into_bytes();
    out.extend_from_slice(&tail);
    out.extend_from_slice(b"\nPress any key to reboot.\n");
    out
}
````

with:

````rust
    .into_bytes();
    // The cursor's row after the last line counts too: a line there would
    // scroll the first away.
    let left = rows.saturating_sub(rows_of(&out, cols) + rows_of(FOOTER, cols) + 1);
    out.extend_from_slice(fitting(&tail, cols, left));
    out.extend_from_slice(FOOTER);
    out
}

/// The rows `text`'s lines take on a console `cols` wide: each line at
/// least one, a line of more characters than that one more per `cols`.
fn rows_of(text: &[u8], cols: usize) -> usize {
    let cols = cols.max(1);
    text.split_inclusive(|&b| b == b'\n')
        .map(|line| {
            let line = line.strip_suffix(b"\n").unwrap_or(line);
            // Characters, not bytes: a UTF-8 continuation byte adds none.
            let chars = line.iter().filter(|&&b| b & 0xC0 != 0x80).count();
            chars.div_ceil(cols).max(1)
        })
        .sum()
}

/// The newest whole lines of `tail` that take at most `rows` rows.
fn fitting(tail: &[u8], cols: usize, rows: usize) -> &[u8] {
    let (mut start, mut used) = (tail.len(), 0);
    for line in tail.split_inclusive(|&b| b == b'\n').rev() {
        let r = rows_of(line, cols);
        if used + r > rows {
            break;
        }
        used += r;
        start -= line.len();
    }
    &tail[start..]
}
````

Replace:

````rust
    console::write_bytes(b"\x1b[0m\x1b[2J\x1b[H");
    console::write_bytes(&text(reason, &tail[..n]));
    loop {
````

with:

````rust
    console::write_bytes(b"\x1b[0m\x1b[2J\x1b[H");
    let size = console::size().unwrap_or((80, 25));
    console::write_bytes(&text(reason, &tail[..n], size));
    loop {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 351 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "Kernel: the error screen shows the newest log lines that fit its rows, so long lines never scroll its heading away"
````


### Task 10: The e2e step `reset [TEXT]`

Decision 8: the scenarios of Task 11 need a key at the error screen that restarts the machine, and Task 14 a `reboot -f`: `reset TEXT` types the text and Enter over serial (Enter alone without text), and QEMU must exit as after a reset, having shut the root down cleanly, and start again on the same disk, as `reboot` does (which now shares its code). Nothing uses it yet; Task 11's scenarios do.

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: the runner's `exit_with`, `reboot`.
- Produces: `Step::Reset(String)`; `reboot(r, command, last, timeout)`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff",
        )
````

with:

````rust
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff\nreset\nreset reboot -f",
        )
````

Replace:

````rust
                (3, Step::Poweroff("poweroff".into())),
                (4, Step::Poweroff("t-sys poweroff".into()))
            ]
````

with:

````rust
                (3, Step::Poweroff("poweroff".into())),
                (4, Step::Poweroff("t-sys poweroff".into())),
                (5, Step::Reset(String::new())),
                (6, Step::Reset("reboot -f".into()))
            ]
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask e2e`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `Reset` found for enum `e2e::Step` in the current scope ``.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//!                                   starts again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
````

with:

````rust
//!                                   starts again on the same disk)
//! reset [<text>]                   (types <text> + Enter over serial, or Enter
//!                                   alone: a key at the error screen; QEMU
//!                                   must exit as after a reset, then starts
//!                                   again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
````

Replace:

````rust
    Reboot(Option<String>),
    /// Switch the machine off with a command (`poweroff` if none is
````

with:

````rust
    Reboot(Option<String>),
    /// Type this text (or nothing) and Enter over the serial console, and
    /// the machine restarts as with `Reboot` (a key at the error screen).
    Reset(String),
    /// Switch the machine off with a command (`poweroff` if none is
````

Replace:

````rust
            "reboot" if rest.is_empty() => Step::Reboot(None),
            "reboot" => {
````

with:

````rust
            "reboot" if rest.is_empty() => Step::Reboot(None),
            "reset" => Step::Reset(rest.to_string()),
            "reboot" => {
````

Replace:

````rust

/// `reboot`: the machine resets, and QEMU (`-no-reboot`) exits, after
/// printing `last` if given; then the same disk boots again, with the serial
/// log continued.
fn reboot(r: &mut Running, last: Option<&str>, timeout: Duration) -> Result<()> {
    let mut log = exit_with(r, "reboot", EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
````

with:

````rust

/// `reboot` (or `reset`): after `command`, the machine resets, and QEMU
/// (`-no-reboot`) exits, after printing `last` if given; then the same disk
/// boots again, with the serial log continued.
fn reboot(r: &mut Running, command: &str, last: Option<&str>, timeout: Duration) -> Result<()> {
    let mut log = exit_with(r, command, EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
````

Replace:

````rust
        }
        Step::Reboot(last) => reboot(r, last.as_deref(), *timeout)?,
        Step::FileLines { path, bytes, line } => {
````

with:

````rust
        }
        Step::Reboot(last) => reboot(r, "reboot", last.as_deref(), *timeout)?,
        Step::Reset(text) => reboot(r, text, None, *timeout)?,
        Step::FileLines { path, bytes, line } => {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask e2e`

Expected: PASS: 27 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "e2e: the step reset [TEXT], a key at the error screen that restarts the machine"
````


### Task 11: Process 1 is init

Spec §4.3, §6.6, §11.2 and decisions 2, 3 and 5: process 1 is `init::run` instead of the in-kernel shell. It shows the error screen at once if `system.img` could not be mounted (startup step 10 records why: `system::mount` returns it, `init::system_failed` keeps it); otherwise it prints `/etc/motd` once, and starts `/bin/sh` with no argument in `/root` (or `/`), in a group of its own with the console (`NEW_GROUP | FOREGROUND`), its fds 0-2 the console (`FdTable::console`); waits for it with `proc::wait_collecting`, which collects every orphan as it ends; says how it ended on the screen and in the kernel log, and starts another, until `Respawn` gives up (the TSC's clock). Test mode is set by `kernel_main`. The in-kernel shell's entry, `session::shell`, goes (the rest of its glue goes in Task 15), and `take_console` wakes the console's readers. `init::is_locked` says whether init's record of the archive is locked, as the other modules with a lock do (Task 12 puts it into the switch's check). Every scenario now runs under `/bin/sh`: milestone 1's pass unchanged (a spike showed it before the tasks were written); `respawn` is new (three `exit`s, the third with a CR LF, the error screen that waits, `reset`); `system_missing`, `system_abi` and `system_empty` reach the error screen, and `system_missing` sees it wait two seconds for its key; `diskfull` no longer expects the in-kernel shell's `t-files: write error` line; `t-spawn fill` finds room for 61 (`spawn`, `sh`) and 60 under a nested shell; `sh`, `utils` and `programs` say where they run, and `utils` sees a program write lines into a file and refuse `cat f >> f`. The README and `docs/hardware-test.md` say what the machine starts. The red run is `respawn`'s: under the in-kernel shell `exit` gives a prompt, with no init line. Mutation checks: the motd printed at every start, the rule ignored, `wait_collecting` waiting for the shell alone (the table fills with zombies: `sh` fails), and the error screen's drain removed (the LF after the last `exit` restarts the machine at once: `alive 2` fails) each fail a scenario.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/fd.rs`
- Modify: `kernel/src/init.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/system.rs`
- Modify: `tests/e2e/diskfull.txt`
- Modify: `tests/e2e/programs.txt`
- Create: `tests/e2e/respawn.txt`
- Modify: `tests/e2e/sh.txt`
- Modify: `tests/e2e/spawn.txt`
- Modify: `tests/e2e/system_abi.txt`
- Modify: `tests/e2e/system_empty.txt`
- Modify: `tests/e2e/system_missing.txt`
- Modify: `tests/e2e/utils.txt`

**Interfaces:**
- Consumes: Tasks 7 (`Respawn`, `ended_line`, `SHELL`), 8 (`error_screen::{show, Reason}`), 10 (`reset`); plan 3a's `proc::{start, spawn}`, plan 4a's `FOREGROUND`.
- Produces: `init::{system_failed(SystemError), is_locked() -> bool, run(u64) -> !}`; `system::mount(&BootInfo, &mut MountTable) -> Result<(), SystemError>`; `FdTable::console() -> FdTable`; `proc::wait_collecting(pid: u32) -> Result<WaitStatus, Errno>` (public); `proc::start(init, arg, cwd)` names process 1 `init`; the `respawn` scenario.

- [ ] **Step 1: Add the failing tests to `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
````

with:

````rust
    #[test]
    fn process_1_has_the_console_as_0_1_and_2() {
        let t = FdTable::console();
        for fd in 0..3 {
            assert_eq!(**t.get(fd).unwrap(), File::Console);
        }
        assert_eq!(t.open(), 3);
    }

    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/diskfull.txt`**

In `tests/e2e/diskfull.txt`, replace:

````text
expect \nwriting standard output: ENOSPC\n
expect writing a file of its own: ENOSPC\nt-files: write error: No space left on device\n
# A console tee that cannot be written: its pop says why.
````

with:

````text
expect \nwriting standard output: ENOSPC\n
expect writing a file of its own: ENOSPC\nroot@relay:~# $
# A console tee that cannot be written: its pop says why.
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/programs.txt`**

In `tests/e2e/programs.txt`, replace:

````text
# Programs run in ring 3 (user-space gate §5, §6.2; milestone 2, plan 2):
# the in-kernel shell runs a name it does not know from /bin, or a path as
# given, and waits for it. What the program writes on fd 1 follows the
````

with:

````text
# Programs run in ring 3 (user-space gate §5, §6.2; milestone 2, plan 2):
# the shell runs a name it does not know from /bin, or a path as
# given, and waits for it. What the program writes on fd 1 follows the
````

- [ ] **Step 4: Add the scenario `tests/e2e/respawn.txt`**

Create `tests/e2e/respawn.txt`:

````text
# Process 1 is init (user-space gate §6.6, §11.2, §16 item 6): it prints
# the motd once, starts /bin/sh in /root as its child (pid 2), and when the
# shell ends it says how and starts another, in /root again, with no motd.
# The third end within 10 s reaches the error screen, which waits for a
# key typed after it came (not the LF of a terminal's CR LF before it),
# and a key restarts the machine.
timeout 30
expect \nWelcome to Relay OS\.\n
expect root@relay:~# $
send cd /
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
send echo alive
expect \nalive\n
send exit 3
expect \ninit: /bin/sh \(pid \d+\) exited with 3; starting it again\nroot@relay:~# $
send-crlf exit
expect \ninit: /bin/sh \(pid \d+\) exited with 0\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh ended 3 times within 10 s\n\n--- last kernel log lines ---\n
expect \nPress any key to reboot\.\n
alive 2
reset
expect Relay OS \d+\.\d+\.\d+
expect root@relay:~# $
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, make these 3 replacements, top to bottom:

Replace:

````text
# /bin/sh, the shell as a program (user-space gate §8.2, §8.3; milestone
# 2, plan 4a), started here from the in-kernel shell. Every command but cd,
# exit and help is a program; a redirection is a file the program writes
````

with:

````text
# /bin/sh, the shell as a program (user-space gate §8.2, §8.3; milestone
# 2, plan 4a), started here by path from the /bin/sh that init started
# (so a shell of its own, and the one init waits for). Every command but cd,
# exit and help is a program; a redirection is a file the program writes
````

Replace:

````text
# Every kernel-stack slot's page tables made first (they stay), so memory
# in use compares after them (and after the in-kernel shell collected the
# orphans, which it does before it starts a program).
send t-spawn fill
expect \nfilled the table with 62 children\n
alive 1
````

with:

````text
# Every kernel-stack slot's page tables made first (they stay), so memory
# in use compares after them (and after init collected the orphans, which
# it does as each ends): init, /bin/sh and t-spawn leave room for 61.
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
````

Replace:

````text
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
# Orphans that end while the in-kernel shell waits for /bin/sh are
# collected as they end (a Ctrl-C'd script leaves its command's zombie to
# process 1 too): the table is as empty the second time.
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
````

with:

````text
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
# Orphans that end while init waits for its shell are collected as they
# end (a Ctrl-C'd script leaves its command's zombie to process 1 too):
# the table is as empty the second time, with two shells in it now.
send t-spawn fill
expect \nfilled the table with 60 children\n
alive 1
send t-spawn fill
expect \nfilled the table with 60 children\n
alive 1
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, replace:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# Orphans that end while the shell waits for nobody are collected before
# its next command starts: a table full of their zombies would refuse
# every program until a reboot.
send t-spawn fill
expect \nfilled the table with 62 children\n
alive 1
````

with:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# Orphans pass to init, which collects each as it ends: a table full of
# their zombies would refuse every program until a reboot. (Init, /bin/sh
# and t-spawn leave room for 61.)
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/system_abi.txt`**

Replace the whole of `tests/e2e/system_abi.txt` with:

````text
# An archive built for another ABI is refused before anything runs from it
# (user-space gate spec §4.3, §7.4), and the line names both versions; the
# error screen says the same (§11.2), and a key restarts the machine.
system-abi 99
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 2
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: ABI 99, kernel wants 2\n
expect \nPress any key to reboot\.\n
reset
expect Relay OS \d+\.\d+\.\d+
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/system_empty.txt`**

Replace the whole of `tests/e2e/system_empty.txt` with:

````text
# An empty \EFI\RELAY\system.img (a copy cut short) is named as such, not
# reported missing (user-space gate spec §4.3), on the startup line and the
# error screen (§11.2).
esp-write /EFI/RELAY/system.img
timeout 30
expect \[FAIL\] system: system\.img: only 0 bytes
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: system\.img: only 0 bytes\n
expect \nPress any key to reboot\.\n
reset
expect Relay OS \d+\.\d+\.\d+
````

- [ ] **Step 9: Expect the new lines in `tests/e2e/system_missing.txt`**

Replace the whole of `tests/e2e/system_missing.txt` with:

````text
# Without \EFI\RELAY\system.img the startup line says so, and with no
# /bin/sh to run the machine shows the error screen (user-space gate spec
# §4.3, §11.2): the reason and the kernel log's last lines; it waits, a key
# restarts it, and it comes back to the same screen.
esp-delete /EFI/RELAY/system.img
timeout 30
expect \[ ok \] mount /: ext2 on .*
expect \[FAIL\] system: no system\.img
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: no system\.img\n\n--- last kernel log lines ---\n
expect \[FAIL\] system: no system\.img\n\nPress any key to reboot\.\n
# It waits for the key, however long that takes.
alive 2
reset
expect Relay OS \d+\.\d+\.\d+
expect \nsystem: no system\.img\n
````

- [ ] **Step 10: Expect the new lines in `tests/e2e/utils.txt`**

In `tests/e2e/utils.txt`, make these 3 replacements, top to bottom:

Replace:

````text
# One program per command (user-space gate §8.4; milestone 2, plan 4a):
# each /bin program prints exactly what the in-kernel shell's command of
# the same name prints, run here by path, since the in-kernel shell's own
# commands come first. Output follows a redirection, errors stay on the
# screen, and memory in use is the same before and after. (Under the
# in-kernel shell a program's standard output is the shell's own output
# hook, never a file, so what depends on writing into one is in the `sh`
# scenario, under /bin/sh.)
timeout 30
````

with:

````text
# One program per command (user-space gate §8.4; milestone 2, plans 4a
# and 4b): each /bin program prints exactly what milestone 1's command of
# the same name printed, run here by path, as a bare name runs it too.
# Output follows a redirection, into which a program writes lines and
# which it never reads back; errors stay on the screen, and memory in use
# is the same before and after.
timeout 30
````

Replace:

````text
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
send /bin/rmdir /root/a/b
````

with:

````text
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
send /bin/ls /root/a > /root/list
send /bin/cat /root/list
expect \nb\nf\n
send /bin/cat /root/list >> /root/list
expect \ncat: /root/list: input file is output file\n
send /bin/rmdir /root/a/b
````

Replace:

````text
expect /bin/cp /bin/t-args /root/t\nroot@relay:~# $
send /bin/rm /root/a/f /root/t /root/out
send /bin/rmdir /root/a
````

with:

````text
expect /bin/cp /bin/t-args /root/t\nroot@relay:~# $
send /bin/rm /root/a/f /root/t /root/out /root/list
send /bin/rmdir /root/a
````

- [ ] **Step 11: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: FAIL: scenario `respawn` stops at line 12, timed out waiting for `\ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $`.

- [ ] **Step 12: Change `README.md`**

In `README.md`, replace:

````markdown
every command but `cd`, `exit` and `help` as a program, and its scripts
may run scripts.

````

with:

````markdown
every command but `cd`, `exit` and `help` as a program, and its scripts
may run scripts. Process 1 is the kernel's init: it starts `/bin/sh` at
boot and again whenever it ends, and a machine that cannot run its shell
(no `system.img`, or a shell that keeps ending) shows an error screen and
restarts at a key.

````

- [ ] **Step 13: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] system: no system.img` | The loader could not read `\EFI\RELAY\system.img` (it is missing, or the FAT is damaged) | `cargo xtask flash --kernel` writes it with the loader and the kernel |
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …` | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
````

with:

````markdown
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] system: no system.img`, then the error screen (`*** Relay OS cannot run its shell ***`) | The loader could not read `\EFI\RELAY\system.img` (it is missing, or the FAT is damaged) | `cargo xtask flash --kernel` writes it with the loader and the kernel |
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …`, then the error screen | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
````

- [ ] **Step 14: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
            slots: [const { None }; FDS],
        }
    }

````

with:

````rust
            slots: [const { None }; FDS],
        }
    }

    /// Process 1's fds: the console as 0, 1 and 2, which the shell it
    /// starts gets.
    pub fn console() -> FdTable {
        let mut t = FdTable::new();
        for slot in &mut t.slots[..3] {
            *slot = Some(Arc::new(File::Console));
        }
        t
    }

````

- [ ] **Step 15: Change `kernel/src/init.rs`**

In `kernel/src/init.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use alloc::format;
use alloc::string::String;
use core::time::Duration;
use relay_abi::WaitStatus;
use relay_abi::wait::EXITED;

````

with:

````rust

use crate::error_screen::{self, Reason};
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
use crate::system::SystemError;
use crate::{kprintln, proc, timer, tty};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;
use relay_abi::wait::EXITED;
use relay_abi::{FdMap, WaitStatus};
use spin::Mutex;
use vfs::{Errno, Vfs};

````

Replace:

````rust
pub const WINDOW: Duration = Duration::from_secs(10);

````

with:

````rust
pub const WINDOW: Duration = Duration::from_secs(10);
/// Where the shell starts (spec §6.6).
const HOME: &[u8] = b"/root";
/// The most of `/etc/motd` shown.
const MOTD_MAX: usize = 16 * 1024;

/// Why `system.img` could not be mounted, if it could not (startup step
/// 10): init shows the error screen for it instead of starting a shell.
static SYSTEM: Mutex<Option<SystemError>> = Mutex::new(None);

/// The system archive could not be mounted: there is no `/bin/sh`.
pub fn system_failed(e: SystemError) {
    *SYSTEM.lock() = Some(e);
}

/// Whether init's record of the archive is locked now (for the kernel's
/// checks that no lock is held across a switch).
pub fn is_locked() -> bool {
    SYSTEM.is_locked()
}

/// Process 1 (spec §4.4 step 11). Never returns.
pub extern "C" fn run(_: u64) -> ! {
    if let Some(e) = SYSTEM.lock().take() {
        error_screen::show(&Reason::System(e));
    }
    motd();
    let mut respawn = Respawn::default();
    loop {
        let pid = start_shell().unwrap_or_else(|e| error_screen::show(&Reason::CannotStart(e)));
        // Every orphan that ends meanwhile is collected too. The next
        // shell's `FOREGROUND` gives it the console, whatever this one left
        // it in.
        let status = proc::wait_collecting(pid).expect("init waits for its own child");
        let again = respawn.ended(timer::tsc_time().unwrap_or_else(timer::uptime));
        kprintln!("{}", ended_line(pid, &status, again));
        if !again {
            error_screen::show(&Reason::Ended);
        }
    }
}

/// Shows `/etc/motd`, as the shell did in milestone 1.
fn motd() {
    let mut vfs = KernelVfs;
    if let Ok(node) = vfs.lookup(b"/etc/motd") {
        let mut buf = alloc::vec![0; MOTD_MAX];
        if let Ok(n) = vfs.read_at(node, 0, &mut buf) {
            tty::write(&buf[..n]);
        }
    }
}

/// Starts `/bin/sh` without arguments in `/root`, or in `/` without one
/// (the empty read-only root the kernel falls back to), as a group of its
/// own with the console, its fds 0-2 the console; its pid.
fn start_shell() -> Result<u32, Errno> {
    let mut vfs = KernelVfs;
    if vfs.chdir(HOME).is_err() {
        let _ = vfs.chdir(b"/");
    }
    let fds = [0, 1, 2].map(|fd| FdMap {
        child: fd,
        parent: fd,
    });
    let mut args: Vec<u8> = SHELL.into();
    args.push(0);
    proc::spawn(&Spawn {
        path: SHELL.into(),
        args,
        argc: 1,
        cwd: Vec::new(),
        fds: fds.to_vec(),
        new_group: true,
        foreground: true,
    })
}

````

- [ ] **Step 16: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    let mut vfs = vfs::MountTable::new(root);
    system::mount(info, &mut vfs);
    let root = mounts::init(vfs);
    proc::start(session::shell, u64::from(cmdline.test_mode), root)
}
````

with:

````rust
    let mut vfs = vfs::MountTable::new(root);
    if let Err(e) = system::mount(info, &mut vfs) {
        init::system_failed(e);
    }
    let root = mounts::init(vfs);
    // `test=1`: `poweroff` makes QEMU exit, whoever asks for it (spec §7.4).
    power::set_test_mode(cmdline.test_mode);
    proc::start(init::run, 0, root)
}
````

- [ ] **Step 17: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! nothing is ready, polls the console and the USB hosts, and sleeps until
//! the next tick. Process 1 is the in-kernel shell, a process without a
//! program, which blocks in `wait` while its commands run and on the
//! console while it waits for a line (plan 4 replaces it with `/bin/sh`).
//!
````

with:

````rust
//! nothing is ready, polls the console and the USB hosts, and sleeps until
//! the next tick. Process 1 is init (`init.rs`), a process without a
//! program, which blocks in `wait` while the shell it started runs.
//!
````

Replace:

````rust

/// Gives the console back to the in-kernel shell: its own group, raw mode.
pub fn take_console() {
    tty::set_line_mode(false);
    tty::set_foreground(table::INIT);
}
````

with:

````rust

/// Gives the console back to process 1: its own group, raw mode. A reader
/// of another group that is still blocked wakes, to find it has lost it.
pub fn take_console() {
    tty::set_line_mode(false);
    tty::set_foreground(table::INIT);
    PROCS.lock().wake_all(Blocked::Console);
}
````

Replace:

````rust

/// Starts the in-kernel shell as process 1 in `cwd`, running
/// `shell(arg)`, and becomes the idle task. Never returns.
pub fn start(shell: extern "C" fn(u64) -> !, arg: u64, cwd: Cwd) -> ! {
    let stack = mm::alloc_kernel_stack().expect("a kernel stack for process 1");
    prepare(&stack, shell, arg);
    let res = Res {
        stack,
        space: None,
        entry: None,
        fds: FdTable::shell(),
        cwd: Some(cwd),
    };
    let pid = PROCS
        .lock()
        .insert(0, true, String::from("relay-sh"), res)
        .unwrap_or_else(|_| unreachable!("the table is empty"));
````

with:

````rust

/// Starts process 1 in `cwd`, running `init(arg)`, with the console as its
/// fds 0-2, and becomes the idle task. Never returns.
pub fn start(init: extern "C" fn(u64) -> !, arg: u64, cwd: Cwd) -> ! {
    let stack = mm::alloc_kernel_stack().expect("a kernel stack for process 1");
    prepare(&stack, init, arg);
    let res = Res {
        stack,
        space: None,
        entry: None,
        fds: FdTable::console(),
        cwd: Some(cwd),
    };
    let pid = PROCS
        .lock()
        .insert(0, true, String::from("init"), res)
        .unwrap_or_else(|_| unreachable!("the table is empty"));
````

Replace:

````rust
/// Waits for the child `pid`, collecting any other child that ends
/// meanwhile.
fn wait_collecting(pid: u32) -> Result<WaitStatus, Errno> {
    // `ECHILD` before anything else is collected.
````

with:

````rust
/// Waits for the child `pid`, collecting any other child that ends
/// meanwhile: for process 1, the orphans that pass to it.
pub fn wait_collecting(pid: u32) -> Result<WaitStatus, Errno> {
    // `ECHILD` before anything else is collected.
````

- [ ] **Step 18: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
use crate::{console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Output, Shell, System};
use vfs::{Env, Errno, Vfs};
````

with:

````rust
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::syscall::Spawn;
use crate::{console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Output, System};
use vfs::{Env, Errno, Vfs};
````

Replace:

````rust

/// Process 1 (spec §4.4 step 11): the shell over the kernel's mount table
/// (the root at `/`, the programs at `/bin`); `test_mode` is 1 for
/// `test=1`. Never returns.
pub extern "C" fn shell(test_mode: u64) -> ! {
    power::set_test_mode(test_mode != 0);
    let mut vfs = KernelVfs;
    let mut console = KernelConsole;
    let mut system = KernelSystem {
        test_mode: test_mode != 0,
    };
    let mut shell = Shell::new(&mut vfs, &mut console, &mut system);
    shell.greet();
    // `run` returns after `exit` (or if `reboot` or `poweroff` did, which
    // they do not): the in-kernel shell reads on at a new prompt.
    loop {
        shell.run();
    }
}

#[cfg(test)]
````

with:

````rust

#[cfg(test)]
````

- [ ] **Step 19: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.

````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).

````

Replace:

````rust
/// Startup step 10: mounts the loader's archive at `/bin` and prints the
/// `system` line.
pub fn mount(info: &BootInfo, vfs: &mut MountTable) {
    let result = (|| {
````

with:

````rust
/// Startup step 10: mounts the loader's archive at `/bin` and prints the
/// `system` line; why it could not.
pub fn mount(info: &BootInfo, vfs: &mut MountTable) -> Result<(), SystemError> {
    let result = (|| {
````

Replace:

````rust
    match result {
        Ok(text) => console::ok(format_args!("system: {text}")),
        Err(e) => console::fail("system", format_args!("{e}")),
    }
````

with:

````rust
    match result {
        Ok(text) => {
            console::ok(format_args!("system: {text}"));
            Ok(())
        }
        Err(e) => {
            console::fail("system", format_args!("{e}"));
            Err(e)
        }
    }
````

- [ ] **Step 20: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 352 tests.

- [ ] **Step 21: Run the `respawn`, `system_missing`, `system_abi`, `system_empty`, `diskfull`, `sh`, `spawn`, `utils`, `programs`, `mount_fail`, `boot` scenarios**

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_empty`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario mount_fail`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 22: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 23: Commit**

````bash
git add README.md docs kernel tests
git commit -m "Kernel: process 1 is init, which starts /bin/sh and starts it again, and shows the error screen when the machine cannot run its shell"
````


### Task 12: Init holds no lock while the error screen waits

The prototype's review (minor): `if let Some(e) = SYSTEM.lock().take() { error_screen::show(…) }` keeps the lock's guard to the end of the block in Rust 2024, so init held the record of the archive's failure locked through the error screen, which waits for a key and so switches: a lock across a switch, which nothing noticed, since the switch's debug assertion (`no_lock_held`) did not look at that lock and nothing else takes it. The check now covers it (`init::is_locked`, which Task 11 added), and init takes the record out before it shows the screen. The red run is `system_missing`'s: with the check and without the fix, the kernel stops on the panic screen, `a lock held across a switch`, once the error screen waits (the scenario's `alive 2` makes sure it does: a key the runner sends as soon as the screen's text reaches the serial line can be there before the screen first looks, and then it never waits).

**Files:**
- Modify: `kernel/src/init.rs`
- Modify: `kernel/src/proc.rs`

**Interfaces:**
- Consumes: Task 11's `init::{run, is_locked}`.
- Produces: `proc::no_lock_held` covering init's lock.

- [ ] **Step 1: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        && !mm::is_locked()
}
````

with:

````rust
        && !mm::is_locked()
        && !crate::init::is_locked()
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: FAIL: scenario `system_missing` stops at line 13: `` QEMU still runs 30s after `` ``; the serial log shows the panic screen, `a lock held across a switch`.

- [ ] **Step 3: Change `kernel/src/init.rs`**

In `kernel/src/init.rs`, replace:

````rust
pub extern "C" fn run(_: u64) -> ! {
    if let Some(e) = SYSTEM.lock().take() {
        error_screen::show(&Reason::System(e));
````

with:

````rust
pub extern "C" fn run(_: u64) -> ! {
    // Taken out first: a guard made in the `if let` would stay locked
    // through the error screen, which waits, and so switches.
    let failed = SYSTEM.lock().take();
    if let Some(e) = failed {
        error_screen::show(&Reason::System(e));
````

- [ ] **Step 4: Run the `system_missing`, `system_abi`, `respawn` scenarios**

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "Kernel: init holds no lock while the error screen waits, and the switch's check covers init's lock"
````


### Task 13: With `/root` removed, the next shell starts in `/`

The prototype's review (minor): init's fallback to `/` when there is no `/root` had no test: `mount_fail`'s empty root has no `/root`, but init is already in `/` there, so removing the fallback passed every scenario, and the next shell would start in a removed directory. The `noroot` scenario removes `/root`, ends the shell and sees the next one in `/`. It is a scenario of its own, since it removes the check scripts too. The task adds only a test of what Task 11 does, so it has no failing run; the mutation check shows what it guards: without the fallback, the next prompt is `root@relay:~#`.

**Files:**
- Create: `tests/e2e/noroot.txt`

**Interfaces:**
- Consumes: Task 11's init.
- Produces: the `noroot` scenario.

- [ ] **Step 1: Add the scenario `tests/e2e/noroot.txt`**

Create `tests/e2e/noroot.txt`:

````text
# Init starts each shell in /root, or in / when there is none (user-space
# gate §6.6, §16 item 6): with /root removed, the next shell starts in /,
# not in the removed directory. (The empty read-only root of mount_fail has
# no /root either, but init is already in / there.)
timeout 30
expect root@relay:~# $
send cd /
send rm -r /root
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:/# $
send pwd
expect \n/\n
send ls /
expect \nbin  dev  etc  home  lost\+found  tmp  usr  var\n
````

- [ ] **Step 2: Run the `noroot` scenario**

Run: `cargo xtask test --e2e-only --scenario noroot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -m "Tests: with /root removed, the next shell init starts is in /"
````


### Task 14: `reboot -f` and `poweroff -f` say why before going ahead, as programs too

Plan 4a's final-review ruling, to be checked under `/bin/sh`, and plan 3b's carry-forward (`POWER_FORCE` and `POWER_REBOOT` end to end): milestone 1's `reboot -f` printed `reboot: cannot shut the filesystems down cleanly: …` and went ahead; as a program it went ahead without a word, since the kernel's `power` with `POWER_FORCE` shuts down what it can and goes. `restart` now asks `power` without `-f` first, and only after its error, said as milestone 1 said it, with `-f` (decision 5); a machine whose filesystems shut down cleanly is asked once. The `unplug` scenario goes on after milestone 1's refusal: `reset reboot -f` restarts the machine (its root not clean, which the next boot says, and mounts), the stick is pulled out again, and `poweroff poweroff -f` switches it off after the line. For that the runner does not check the root's clean flag after an `unplug` (the disk keeps what it had when it was pulled out), and an `expect` after `poweroff` reads what the machine printed before it went away. The red run is the shell's test. Mutation checks: `power` asked with `-f` at once fails the test and the scenario; the clean check kept after an unplug fails the scenario.

**Files:**
- Modify: `crates/shell/src/commands/system.rs`
- Modify: `crates/shell/src/program.rs`
- Modify: `tests/e2e/unplug.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: plan 4a's `restart` and `TestSystem::{power_error, forced}`; Task 10's `reset`.
- Produces: `Running::unplugged` in the runner; `expect` allowed once the machine is off.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(h.system.reboots, 0);
        // -f goes ahead: the kernel's `power` shuts down what it can.
        assert_eq!(h.program("poweroff -f", &mut out), (0, String::new()));
        assert_eq!(
            (h.system.poweroffs, &h.system.forced[..]),
            (1, &[false, true][..])
        );
````

with:

````rust
        assert_eq!(h.system.reboots, 0);
        // -f goes ahead, having said why the filesystems were not shut down
        // cleanly, as milestone 1's did: `power` is asked without it first.
        assert_eq!(
            h.program("poweroff -f", &mut out),
            (
                0,
                "poweroff: cannot shut the filesystems down cleanly: Input/output error\n".into()
            )
        );
        assert_eq!(
            (h.system.poweroffs, &h.system.forced[..]),
            (1, &[false, false, true][..])
        );
````

Replace:

````rust
            "the Vfs's own shutdown comes first"
        );
    }
}
````

with:

````rust
            "the Vfs's own shutdown comes first"
        );
        // When they can be, -f changes nothing.
        let mut h = Harness::new();
        assert_eq!(h.program("reboot -f", &mut out), (0, String::new()));
        assert_eq!((h.system.reboots, &h.system.forced[..]), (1, &[false][..]));
    }
}
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/unplug.txt`**

In `tests/e2e/unplug.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# is gone, the shell keeps running, and poweroff will not go on without a
# clean shutdown.
timeout 30
````

with:

````text
# is gone, the shell keeps running, and poweroff will not go on without a
# clean shutdown, unless forced.
timeout 30
````

Replace:

````text
expect \npoweroff: cannot shut the filesystems down cleanly: Input/output error\npoweroff: use 'poweroff -f' to go ahead anyway\n
expect root@relay:~# $
````

with:

````text
expect \npoweroff: cannot shut the filesystems down cleanly: Input/output error\npoweroff: use 'poweroff -f' to go ahead anyway\n
expect root@relay:~# $
# -f goes ahead all the same, having said why (spec §7.4): `power` with
# POWER_FORCE restarts the machine, which comes back with the stick in.
# Its root was not shut down cleanly, and mounts all the same.
send echo after
expect \nafter\n
reset reboot -f
expect Relay OS \d+\.\d+\.\d+
expect \[ ok \] mount /: ext2 on .*
expect root@relay:~# $
send cat /root/f
expect \nbefore\n
unplug
expect port \d: slot \d+ released
poweroff poweroff -f
expect \npoweroff: cannot shut the filesystems down cleanly: Input/output error\n
expect \nrelay: powering off\n
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `program::tests::reboot_and_poweroff_say_why_the_machine_stayed_up`.

- [ ] **Step 4: Change `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// (spec §7.4). If that fails the machine stays up, so nothing is lost
/// silently; `-f` goes ahead anyway. (A program's filesystems are shut down
/// by the kernel's `power`, which returns the error instead.)
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
````

with:

````rust
/// (spec §7.4). If that fails the machine stays up, so nothing is lost
/// silently; `-f` goes ahead anyway, after saying so. (A program's
/// filesystems are shut down by the kernel's `power`, which returns the
/// error instead: it is asked without `-f` first, so that `-f` says the
/// same.)
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
````

Replace:

````rust
    }
    let went = if name == "reboot" {
        ctx.system.reboot(force)
    } else {
        ctx.system.poweroff(force)
    };
    if let Err(e) = went {
        return unclean(ctx, e);
    }
````

with:

````rust
    }
    let go = |ctx: &mut Ctx<'_>, force| {
        if name == "reboot" {
            ctx.system.reboot(force)
        } else {
            ctx.system.poweroff(force)
        }
    };
    match go(ctx, false) {
        Ok(()) => {}
        Err(e) if force => {
            unclean(ctx, e);
            if go(ctx, true).is_err() {
                return 1;
            }
        }
        Err(e) => return unclean(ctx, e),
    }
````

- [ ] **Step 5: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//!                                   again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
//!                                   isa-debug-exit, test mode's power-off)
//! unplug                           (pulls the USB stick out: QMP device_del,
//!                                   then QEMU's DEVICE_DELETED event)
//! check-script /root/checks/a.sh   (after poweroff: the script's transcript,
````

with:

````rust
//!                                   again on the same disk)
//! poweroff [<command>]             (types `poweroff`, or <command>; QEMU must
//!                                   exit through isa-debug-exit, test mode's
//!                                   power-off; an `expect` after it reads
//!                                   what the machine printed before)
//! unplug                           (pulls the USB stick out: QMP device_del,
//!                                   then QEMU's DEVICE_DELETED event; the
//!                                   root need not be clean after it)
//! check-script /root/checks/a.sh   (after poweroff: the script's transcript,
````

Replace:

````rust
    off: bool,
}
````

with:

````rust
    off: bool,
    /// The stick was pulled out: the disk keeps what it had then, so its
    /// clean flag says nothing about how the machine went down.
    unplugged: bool,
}
````

Replace:

````rust
        off: false,
    })
````

with:

````rust
        off: false,
        unplugged: false,
    })
````

Replace:

````rust
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean. Returns the serial log once everything QEMU printed is in it.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
````

with:

````rust
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean, unless the stick was pulled out. Returns the serial log once everything QEMU printed is in it.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
````

Replace:

````rust
            let state = image::ext2_state(&r.qemu.disk, r.root)?;
            if state != "clean" {
                bail!("after `{command}` the root filesystem is `{state}`, not `clean`");
````

with:

````rust
            let state = image::ext2_state(&r.qemu.disk, r.root)?;
            if state != "clean" && !r.unplugged {
                bail!("after `{command}` the root filesystem is `{state}`, not `clean`");
````

Replace:

````rust
        step,
        Step::Timeout(_) | Step::FileLines { .. } | Step::CheckScript(_)
    );
````

with:

````rust
        step,
        Step::Timeout(_) | Step::Expect(_) | Step::FileLines { .. } | Step::CheckScript(_)
    );
````

Replace:

````rust
            )?;
        }
````

with:

````rust
            )?;
            r.unplugged = true;
        }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 153 tests.

- [ ] **Step 7: Run the `unplug` scenario**

Run: `cargo xtask test --e2e-only --scenario unplug`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates tests xtask
git commit -m "Shell: reboot -f and poweroff -f say why the filesystems were not shut down cleanly before going ahead, as a program too; the unplug scenario runs both"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 40 scenario(s) passed`.

````bash
git push -u origin m2p4b/init
gh pr create --base main --head m2p4b/init --title "Milestone 2, plan 4b: Process 1 is init" --body-file - <<'EOF'
## What

Milestone 2, plan 4b, tasks 7–14: process 1 is the kernel's init: it prints the motd once, starts `/bin/sh` in `/root` (or `/`) with the console, collects every orphan as it ends, says how the shell ended and starts another; the third end within 10 s, a shell that cannot start, or a `system.img` that could not be mounted reach the error screen, which says why, shows the kernel log's last lines, waits for a key and restarts the machine; every scenario runs under `/bin/sh` (`respawn` is new; the `system_*` scenarios reach the error screen); `reboot -f` and `poweroff -f` say why the filesystems were not shut down cleanly before going ahead (`unplug` runs both); the e2e step `reset`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing the NUC runs changes but what the QEMU scenarios cover; NUC check 4 (the last pull request of this plan) runs it all
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4b/init --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-init
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: The kernel without the shell (Tasks 15–17)

The in-kernel shell's glue goes, and with it the kernel's dependency on the `shell` crate; the shell crate loses the in-kernel shell's way to programs.

Branch `m2p4b/noshell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-noshell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4b/noshell /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-noshell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-noshell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4b/init` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4b/noshell /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-noshell m2p4b/init`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4b/init>` and re-run `cargo xtask ci` before pushing.

### Task 15: The kernel has no shell

Spec §1.4 item 3 and decision 4: `relay-kernel` no longer depends on the `shell` crate, which a new xtask test checks through `cargo metadata` (Task 16 makes it walk the whole graph). What only the in-kernel shell used goes: `session.rs` (`KernelConsole`, `KernelSystem`, `mem_info`; `KernelEnv`, the filesystems' clock and log, moves to `storage.rs`), `File::ShellOutput`, `FdTable::shell`, the output hook (`SHELL_OUT`, `proc::wait`, `Caller::shell_output`), `renew_outputs`, `collect_orphans`, `give_console` and `tty::take_interrupt`. The dispatcher's fake has the console as fds 0-2 and records what is written to it; its fd `FULL` (a shell output that failed every write) goes with them, since a full disk's short write and `ENOSPC` have their own test. The red run is xtask's test, which lists `shell` among the kernel's dependencies.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/fd.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/mounts.rs`
- Modify: `kernel/src/power.rs`
- Modify: `kernel/src/proc.rs`
- Delete: `kernel/src/session.rs`
- Modify: `kernel/src/storage.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/tty.rs`
- Modify: `xtask/src/ci.rs`

**Interfaces:**
- Consumes: Task 11's init.
- Produces: `testing::text(&Fake) -> Vec<u8>` (the console's output), `Fake::written: Vec<Vec<u8>>`; `ci::tests::the_kernel_does_not_depend_on_the_shell`.

- [ ] **Step 1: Add the failing tests to `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn the_shell_has_the_console_and_its_outputs() {
        let t = FdTable::shell();
        assert_eq!(**t.get(0).unwrap(), File::Console);
        assert_eq!(**t.get(1).unwrap(), File::ShellOutput(1));
        assert_eq!(**t.get(2).unwrap(), File::ShellOutput(2));
        assert_eq!(t.open(), 3);
````

with:

````rust
    #[test]
    fn process_1_has_the_console_as_0_1_and_2() {
        let t = FdTable::console();
        for fd in 0..3 {
            assert_eq!(**t.get(fd).unwrap(), File::Console);
        }
        assert_eq!(t.open(), 3);
````

Replace:

````rust
    #[test]
    fn process_1_has_the_console_as_0_1_and_2() {
        let t = FdTable::console();
        for fd in 0..3 {
            assert_eq!(**t.get(fd).unwrap(), File::Console);
        }
        assert_eq!(t.open(), 3);
    }

    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
        let parent = FdTable::shell();
        let child = parent
````

with:

````rust
    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
        let parent = FdTable::console();
        let child = parent
````

Replace:

````rust
        assert!(Arc::ptr_eq(child.get(1).unwrap(), parent.get(2).unwrap()));
        assert_eq!(**child.get(31).unwrap(), File::ShellOutput(1));
        assert_eq!(child.get(2).err(), Some(Errno::EBADF), "not given");
````

with:

````rust
        assert!(Arc::ptr_eq(child.get(1).unwrap(), parent.get(2).unwrap()));
        assert!(Arc::ptr_eq(child.get(31).unwrap(), parent.get(1).unwrap()));
        assert_eq!(child.get(2).err(), Some(Errno::EBADF), "not given");
````

Replace:

````rust
    fn a_file_put_in_replaces_the_old_one_for_later_children_only() {
        let mut shell = FdTable::shell();
        let before = shell.for_child(&[map(1, 1)]).unwrap();
        shell.set(1, Arc::new(File::ShellOutput(1)));
        let after = shell.for_child(&[map(1, 1)]).unwrap();
        assert!(!Arc::ptr_eq(before.get(1).unwrap(), after.get(1).unwrap()));
        assert!(Arc::ptr_eq(after.get(1).unwrap(), shell.get(1).unwrap()));
        assert_eq!(**before.get(1).unwrap(), File::ShellOutput(1));
    }

    #[test]
    fn a_file_opened_takes_the_lowest_free_fd_up_to_32() {
        let mut t = FdTable::shell();
        let console = || Arc::new(File::Console);
        assert_eq!(t.insert(console()), Ok(3));
        assert_eq!(t.remove(1).map(|f| *f == File::ShellOutput(1)), Ok(true));
        assert_eq!(t.insert(console()), Ok(1), "the lowest free one");
````

with:

````rust
    fn a_file_put_in_replaces_the_old_one_for_later_children_only() {
        let mut parent = FdTable::console();
        let old = Arc::clone(parent.get(1).unwrap());
        let before = parent.for_child(&[map(1, 1)]).unwrap();
        parent.set(1, Arc::new(File::Console));
        let after = parent.for_child(&[map(1, 1)]).unwrap();
        assert!(!Arc::ptr_eq(before.get(1).unwrap(), after.get(1).unwrap()));
        assert!(Arc::ptr_eq(after.get(1).unwrap(), parent.get(1).unwrap()));
        assert!(Arc::ptr_eq(before.get(1).unwrap(), &old));
    }

    #[test]
    fn a_file_opened_takes_the_lowest_free_fd_up_to_32() {
        let mut t = FdTable::console();
        let console = || Arc::new(File::Console);
        assert_eq!(t.insert(console()), Ok(3));
        assert_eq!(t.remove(1).map(|f| *f == File::Console), Ok(true));
        assert_eq!(t.insert(console()), Ok(1), "the lowest free one");
````

Replace:

````rust
        let b = Arc::new(File::Vfs(file::open(&mut t, b"/b", rw).unwrap()));
        let mut fds = FdTable::shell();
        fds.insert(Arc::clone(&a)).unwrap();
````

with:

````rust
        let b = Arc::new(File::Vfs(file::open(&mut t, b"/b", rw).unwrap()));
        let mut fds = FdTable::console();
        fds.insert(Arc::clone(&a)).unwrap();
````

Replace:

````rust
    fn a_bad_mapping_is_refused() {
        let parent = FdTable::shell();
        assert_eq!(parent.for_child(&[map(0, 3)]).err(), Some(Errno::EBADF));
````

with:

````rust
    fn a_bad_mapping_is_refused() {
        let parent = FdTable::console();
        assert_eq!(parent.for_child(&[map(0, 3)]).err(), Some(Errno::EBADF));
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
        assert_eq!(call(&mut f, Call::Write, [2, U, 3]), Ok(3));
        assert_eq!(call(&mut f, Call::Write, [0, U, 1]), Ok(1), "the console");
        assert_eq!(text(&f, 1), [10, 11, 12, 13, 14]);
        assert_eq!(text(&f, 2), [0, 1, 2]);
        // Across pages, in pieces of at most 4 KiB.
        f.written.clear();
        assert_eq!(call(&mut f, Call::Write, [1, U + 100, 10_000]), Ok(10_000));
        let want: Vec<u8> = (100..10_100).map(|i| (i % 251) as u8).collect();
        assert_eq!(text(&f, 1), want);
        assert!(f.written.iter().all(|(_, b)| b.len() <= 4096));
        assert_eq!(
````

with:

````rust
        assert_eq!(call(&mut f, Call::Write, [2, U, 3]), Ok(3));
        assert_eq!(call(&mut f, Call::Write, [0, U, 1]), Ok(1), "fd 0 too");
        assert_eq!(text(&f), [10, 11, 12, 13, 14, 0, 1, 2, 0], "the console");
        // Across pages, in pieces of at most 4 KiB.
        f.written.clear();
        assert_eq!(call(&mut f, Call::Write, [1, U + 100, 10_000]), Ok(10_000));
        let want: Vec<u8> = (100..10_100).map(|i| (i % 251) as u8).collect();
        assert_eq!(text(&f), want);
        assert!(f.written.iter().all(|b| b.len() <= 4096));
        assert_eq!(
````

Replace:

````rust
        assert!(f.written.is_empty());
    }

    #[test]
    fn a_file_s_error_reaches_the_program() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [FULL, U, 10]), Err(errno::ENOSPC));
    }
````

with:

````rust
        assert!(f.written.is_empty());
    }
````

Replace:

````rust
            .collect();
        assert_eq!(text(&f, 1), want);
        f.written.clear();
````

with:

````rust
            .collect();
        assert_eq!(text(&f), want);
        f.written.clear();
````

Replace:

````rust
        );
        assert_eq!(text(&f, 1).len(), 904);
    }
````

with:

````rust
        );
        assert_eq!(text(&f).len(), 904);
    }
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        }
        // Up to 32 (7 is `FULL`); the 33rd creates nothing.
        for fd in (5..32).filter(|&fd| fd != FULL) {
            assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(fd));
````

with:

````rust
        }
        // Up to 32; the 33rd creates nothing.
        for fd in 5..32 {
            assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(fd));
````

Replace:

````rust
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(
            call(&mut f, Call::Read, [1, W, 1]),
            Err(errno::EBADF),
            "an output"
        );
        assert_eq!(
````

with:

````rust
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(
````

- [ ] **Step 4: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (the shell's output redirected to a
/// full disk).
pub const FULL: u64 = 7;
/// What the fake kernel log holds.
````

with:

````rust
pub const W: u64 = U + 3 * PAGE;
/// What the fake kernel log holds.
````

Replace:

````rust
/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; the console as fd 0,
/// the in-kernel shell's outputs as fds 1 and 2 and one to a full disk as
/// `FULL`; the files `/root/f` ("hello", `/root` its current directory)
/// and `/full` (8 KiB of room); what it wrote to the console (as fd 0) or
/// the outputs (as 1 and 2), started, killed, and how long it slept.
pub struct Fake {
````

with:

````rust
/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; the console as fds 0,
/// 1 and 2; the files `/root/f` ("hello", `/root` its current directory)
/// and `/full` (8 KiB of room); what it wrote to the console, started,
/// killed, and how long it slept.
pub struct Fake {
````

Replace:

````rust
    pub powered: Vec<(bool, bool)>,
    pub written: Vec<(u64, Vec<u8>)>,
    pub slept: Vec<u64>,
````

with:

````rust
    pub powered: Vec<(bool, bool)>,
    /// Each write to the console.
    pub written: Vec<Vec<u8>>,
    pub slept: Vec<u64>,
````

Replace:

````rust
    fn console_write(&mut self, bytes: &[u8]) {
        self.written.push((0, bytes.to_vec()));
    }
````

with:

````rust
    fn console_write(&mut self, bytes: &[u8]) {
        self.written.push(bytes.to_vec());
    }
````

Replace:

````rust
        self.foreground = pgid;
        Ok(())
    }
    fn shell_output(&mut self, _: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
        if u64::from(n) == FULL {
            return Err(Errno::ENOSPC);
        }
        self.written.push((u64::from(n), bytes.to_vec()));
        Ok(())
````

with:

````rust
        self.foreground = pgid;
        Ok(())
````

Replace:

````rust
    space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
    let mut fds = FdTable::shell();
    fds.set(FULL as usize, Arc::new(File::ShellOutput(FULL as u32)));
    Fake {
        mem,
        space,
        fds,
        vfs: files(),
````

with:

````rust
    space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
    Fake {
        mem,
        space,
        fds: FdTable::console(),
        vfs: files(),
````

Replace:

````rust

/// Everything written, joined, per fd.
pub fn text(f: &Fake, fd: u64) -> Vec<u8> {
    f.written
        .iter()
        .filter(|(d, _)| *d == fd)
        .flat_map(|(_, b)| b.clone())
        .collect()
}
````

with:

````rust

/// Everything written to the console, joined.
pub fn text(f: &Fake) -> Vec<u8> {
    f.written.concat()
}
````

- [ ] **Step 5: Add the failing tests to `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
        assert_eq!(tee_target(&File::ShellOutput(1)), Err(Errno::EINVAL));
    }
````

with:

````rust
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
    }
````

- [ ] **Step 6: Add the failing tests to `xtask/src/ci.rs`**

In `xtask/src/ci.rs`, replace:

````rust
        assert!(workflow.contains("run: cargo xtask unit"));
    }
}
````

with:

````rust
        assert!(workflow.contains("run: cargo xtask unit"));
    }

    /// The user-space gate's definition of done (§1.4 item 3): every
    /// command the user types runs as a program, so the kernel does not
    /// link the shell.
    #[test]
    fn the_kernel_does_not_depend_on_the_shell() {
        let out = crate::util::cargo()
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--offline",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let kernel = meta["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "relay-kernel")
            .expect("the kernel is a package");
        let deps: Vec<&str> = kernel["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(deps.contains(&"vfs"), "the kernel's dependencies: {deps:?}");
        assert!(
            !deps.contains(&"shell"),
            "the kernel's dependencies: {deps:?}"
        );
    }
}
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p xtask ci::`

Expected: FAIL: 1 test fails: `ci::tests::the_kernel_does_not_depend_on_the_shell`.

- [ ] **Step 8: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
term.workspace = true
shell.workspace = true
vfs.workspace = true
````

with:

````toml
term.workspace = true
vfs.workspace = true
````

- [ ] **Step 9: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! uses the same file as its parent, offset and all. The files are the
//! console, the in-kernel shell's output and files of the VFS; milestone 3
//! adds pipe ends.

````

with:

````rust
//! uses the same file as its parent, offset and all. The files are the
//! console and files of the VFS; milestone 3 adds pipe ends.

````

Replace:

````rust
    Console,
    /// Standard output (1) or standard error (2) of the in-kernel shell:
    /// what is written goes to the shell, which sends it where the command
    /// line says (a redirection, the screen, a script's transcript). Plan 4
    /// replaces the in-kernel shell and this with it.
    ShellOutput(u32),
    /// A file of the VFS.
````

with:

````rust
    Console,
    /// A file of the VFS.
````

Replace:

````rust
        }
        t
    }

    /// The in-kernel shell's fds: the console to read, and its standard
    /// output and error.
    pub fn shell() -> FdTable {
        let mut t = FdTable::new();
        t.slots[0] = Some(Arc::new(File::Console));
        t.slots[1] = Some(Arc::new(File::ShellOutput(1)));
        t.slots[2] = Some(Arc::new(File::ShellOutput(2)));
        t
````

with:

````rust
        }
        t
````

- [ ] **Step 10: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod serial;
pub mod session;
pub mod storage;
````

with:

````rust
pub mod serial;
pub mod storage;
````

- [ ] **Step 11: Change `kernel/src/mounts.rs`**

In `kernel/src/mounts.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! The mount table (user-space gate §7.3): one for the whole kernel,
//! shared by the in-kernel shell and every process's calls, with each
//! caller's current directory put in while the kernel works for it
````

with:

````rust
//! The mount table (user-space gate §7.3): one for the whole kernel,
//! shared by init, `spawn` and every process's calls, with each
//! caller's current directory put in while the kernel works for it
````

Replace:

````rust

/// The mount table as the in-kernel shell's `Vfs`, with the current
/// directory of the process it works for.
pub struct KernelVfs;
````

with:

````rust

/// The mount table as a `Vfs` (for init, `spawn` reading a program, and
/// the calls on files), with the current directory of the process it
/// works for.
pub struct KernelVfs;
````

- [ ] **Step 12: Change `kernel/src/power.rs`**

In `kernel/src/power.rs`, replace:

````rust
//! Restarting and switching off (spec §7.4), through the registers the
//! FADT describes and the `\_S5` values of the DSDT. The shell has already
//! shut the filesystems down when these run. Which registers to write, and
//! what, is worked out here and tested on the host; the writes themselves
````

with:

````rust
//! Restarting and switching off (spec §7.4), through the registers the
//! FADT describes and the `\_S5` values of the DSDT. The filesystems are
//! shut down before these run (by the `power` call, and by the error
//! screen). Which registers to write, and
//! what, is worked out here and tested on the host; the writes themselves
````

- [ ] **Step 13: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::{MemInfo, Time, WaitStatus};
````

with:

````rust
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use relay_abi::{MemInfo, Time, WaitStatus};
````

Replace:

````rust
    stack: KernelStack,
    /// Its program's memory; `None` for the in-kernel shell, and once the
    /// process has ended.
    space: Option<AddressSpace>,
````

with:

````rust
    stack: KernelStack,
    /// Its program's memory; `None` for init, and once the process has
    /// ended.
    space: Option<AddressSpace>,
````

Replace:

````rust
static IDLE: AtomicU64 = AtomicU64::new(0);

/// The in-kernel shell's output while it waits for a command (plan 2's
/// hook, `System::wait`): what its children write to fds 1 and 2, and the
/// answer to each write (a redirection file's error).
type Out<'a> = &'a mut shell::Output<'a>;
static SHELL_OUT: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

````

with:

````rust
static IDLE: AtomicU64 = AtomicU64::new(0);

````

Replace:

````rust

/// Gives the console to the process group of the running process's child
/// `pid`, in line mode, while the in-kernel shell waits for it (spec
/// §6.4). A Ctrl-C typed before the command started waits in the input
/// queue, and the next poll in line mode finds it: it is the command's.
pub fn give_console(pid: u32) {
    if let Some(p) = PROCS.lock().get(pid) {
        tty::set_foreground(p.pgid);
        tty::set_line_mode(true);
    }
}

/// Gives the console back to process 1: its own group, raw mode. A reader
````

with:

````rust

/// Gives the console back to process 1: its own group, raw mode. A reader
````

Replace:

````rust

/// The running process (the in-kernel shell) waits until something is
/// typed.
pub fn wait_for_input() {
````

with:

````rust

/// The running process (init, at the error screen) waits until something
/// is typed.
pub fn wait_for_input() {
````

Replace:

````rust

/// Gives the in-kernel shell fresh standard output and error for the
/// command it starts next: its hook answers only what that command and its
/// descendants write (they inherit these files), so an orphan of an
/// earlier command, which holds that command's, writes to the screen.
pub fn renew_outputs() {
    let mut t = PROCS.lock();
    let me = t.current();
    if let Some(p) = t.get_mut(me) {
        p.res.fds.set(1, Arc::new(File::ShellOutput(1)));
        p.res.fds.set(2, Arc::new(File::ShellOutput(2)));
    }
}

/// Whether `file` is one of the in-kernel shell's outputs now, the ones its
/// current command has.
fn shell_output_now(file: &Arc<File>, fd: u32) -> bool {
    let t = PROCS.lock();
    t.get(table::INIT)
        .and_then(|p| p.res.fds.get(u64::from(fd)).ok())
        .is_some_and(|now| Arc::ptr_eq(now, file))
}

/// Collects the running process's children that have ended: for the
/// in-kernel shell, process 1, the orphans that passed to it. It does so
/// before each command it starts and after each it waited for, so their
/// zombies never fill the table (plan 4's `/bin/sh` does it before every
/// prompt).
pub fn collect_orphans() {
    while let Ok(Some(_)) = collect(Child::Any, true) {}
}

/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook). The orphans
/// that pass to it end while it waits too (a script `/bin/sh` runs leaves
/// its command's zombie when Ctrl-C kills them both), and it collects each
/// as it ends, so their zombies never fill the table while a long command
/// runs. `ECHILD` if `pid` is not its child.
pub fn wait(pid: u32, out: &mut shell::Output<'_>) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = wait_collecting(pid);
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    ended
}

/// Waits for the child `pid`, collecting any other child that ends
/// meanwhile: for process 1, the orphans that pass to it.
pub fn wait_collecting(pid: u32) -> Result<WaitStatus, Errno> {
````

with:

````rust

/// Waits for the child `pid`, collecting any other child that ends
/// meanwhile: for process 1, the orphans that pass to it, so their
/// zombies never fill the table while the shell runs. `ECHILD` if `pid` is
/// not a child of the running process.
pub fn wait_collecting(pid: u32) -> Result<WaitStatus, Errno> {
````

Replace:

````rust
        Ok(())
    }

    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
        let out = SHELL_OUT.load(Ordering::Acquire);
        if out.is_null() || !shell_output_now(file, n) {
            // The shell waits for nobody, or for another command: this
            // goes to the screen.
            tty::write(bytes);
            return Ok(());
        }
        // SAFETY: set by the in-kernel shell's `wait`, which is blocked
        // until its child has ended and clears it before it returns;
        // nothing else calls it meanwhile.
        unsafe { (*out.cast::<Out<'_>>())(n, bytes) }
    }
````

with:

````rust
        Ok(())
    }
````

- [ ] **Step 14: Delete `kernel/src/session.rs`**

Delete `kernel/src/session.rs` (it moved):

````bash
git rm -q kernel/src/session.rs
````

- [ ] **Step 15: Change `kernel/src/storage.rs`**

In `kernel/src/storage.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::block::root::{self, RootChoice};
use crate::session::KernelEnv;
use crate::usb::{self, UsbDisk};
use crate::{console, klogln, kprintln};
use ::usb::host::Size;
````

with:

````rust
use crate::block::root::{self, RootChoice};
use crate::usb::{self, UsbDisk};
use crate::{console, klogln, kprintln, rtc};
use ::usb::host::Size;
````

Replace:

````rust
use vfs::{BlockDevice, Env, Errno, FileSystem, MemFs};

````

with:

````rust
use vfs::{BlockDevice, Env, Errno, FileSystem, MemFs};

/// The RTC and the kernel log, for filesystems.
struct KernelEnv;

impl Env for KernelEnv {
    fn now(&self) -> u64 {
        rtc::now_unix().unwrap_or(0)
    }

    fn log(&self, line: &str) {
        klogln!("{line}");
    }
}

````

- [ ] **Step 16: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn power(&mut self, reboot: bool, force: bool) -> Errno;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
    /// 2), which sends them where the command line says; its error, if any.
    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno>;
    /// Starts a child; its pid.
````

with:

````rust
    fn power(&mut self, reboot: bool, force: bool) -> Errno;
    /// Starts a child; its pid.
````

Replace:

````rust
        }
        File::ShellOutput(n) => caller.shell_output(file, *n, bytes).map(|()| bytes.len()),
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
````

with:

````rust
        }
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
````

- [ ] **Step 17: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        File::Vfs(open) if open.is_readable() => open,
        File::Vfs(_) | File::ShellOutput(_) => return Err(Errno::EBADF),
        File::Console => return console_read(caller, addr, len),
````

with:

````rust
        File::Vfs(open) if open.is_readable() => open,
        File::Vfs(_) => return Err(Errno::EBADF),
        File::Console => return console_read(caller, addr, len),
````

Replace:

````rust

/// `seek(fd, offset, whence)`: the new offset. The console and the shell's
/// outputs have none (`EINVAL`).
pub(super) fn seek(
````

with:

````rust

/// `seek(fd, offset, whence)`: the new offset. The console has none
/// (`EINVAL`).
pub(super) fn seek(
````

Replace:

````rust

/// `fstat(fd, &mut Stat)`. The console and the shell's outputs are
/// character devices, with nothing else to say.
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
````

with:

````rust

/// `fstat(fd, &mut Stat)`. The console is a character device, with
/// nothing else to say.
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
````

Replace:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.stat(v))?,
        File::Console | File::ShellOutput(_) => Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
````

with:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.stat(v))?,
        File::Console => Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
````

Replace:

````rust
/// written; `ERANGE` if the buffer is too short. A directory that was
/// removed keeps the path it had, as the in-kernel shell's `pwd` shows it.
pub(super) fn getcwd(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
````

with:

````rust
/// written; `ERANGE` if the buffer is too short. A directory that was
/// removed keeps the path it had, as milestone 1's `pwd` showed it.
pub(super) fn getcwd(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
````

- [ ] **Step 18: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program, and whenever the in-kernel shell
//! reads or asks whether Ctrl-C was pressed.
//!
````

with:

````rust
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program or ends a system call during which
//! a tick passed, in a program's console read, and at the error screen.
//!
````

Replace:

````rust

/// Writes what a process or the in-kernel shell gives the console: the
/// screen, and a copy for every tee, written once 4 KiB of it waits.
````

with:

````rust

/// Writes what a process or init gives the console: the
/// screen, and a copy for every tee, written once 4 KiB of it waits.
````

Replace:

````rust
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console | File::ShellOutput(_) => Err(Errno::EINVAL),
    }
````

with:

````rust
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console => Err(Errno::EINVAL),
    }
````

Replace:

````rust
/// Pushes `file` as a tee of process `owner`: a file of the VFS it has
/// open for writing (`EBADF` otherwise, `EINVAL` for the console or the
/// shell's outputs); `EBUSY` if 4 are pushed.
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
````

with:

````rust
/// Pushes `file` as a tee of process `owner`: a file of the VFS it has
/// open for writing (`EBADF` otherwise, `EINVAL` for the console);
/// `EBUSY` if 4 are pushed.
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
````

Replace:

````rust
/// Whether `file` can be a tee: a file of the VFS open for writing
/// (`EBADF` otherwise), not the console or the shell's outputs (`EINVAL`).
fn tee_target(file: &File) -> Result<(), Errno> {
    match file {
        File::Vfs(open) if open.is_writable() => Ok(()),
        File::Vfs(_) => Err(Errno::EBADF),
        File::Console | File::ShellOutput(_) => Err(Errno::EINVAL),
    }
````

with:

````rust
/// Whether `file` can be a tee: a file of the VFS open for writing
/// (`EBADF` otherwise), not the console (`EINVAL`).
fn tee_target(file: &File) -> Result<(), Errno> {
    match file {
        File::Vfs(open) if open.is_writable() => Ok(()),
        File::Vfs(_) => Err(Errno::EBADF),
        File::Console => Err(Errno::EINVAL),
    }
````

Replace:

````rust

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
/// dropped (`InputQueue::take_interrupt`).
pub fn take_interrupt() -> bool {
    INPUT.lock().take_interrupt()
}

/// Whether the input queue is locked now (for the kernel's checks that no
````

with:

````rust

/// Whether the input queue is locked now (for the kernel's checks that no
````

- [ ] **Step 19: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 349 tests.

Run: `cargo test -p xtask ci::`

Expected: PASS: 5 tests.

- [ ] **Step 20: Run the `boot`, `sh`, `files`, `tees` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 21: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 22: Commit**

````bash
git add Cargo.lock kernel xtask
git commit -m "Kernel: no shell inside; process 1's fds are the console, and the in-kernel shell's output hook, console handover and orphan collection go"
````


### Task 16: The kernel links no shell through any crate it depends on

The prototype's review (minor): Task 15's test asked `cargo metadata --no-deps`, which lists a package's direct dependencies only: a kernel that depended on `relay-rt`, which depends on `shell`, would link the shell again and pass. The test now walks the resolved graph from `relay-kernel` over its normal and build dependencies. It is a stricter test of what the kernel already is, so it has no failing run; the mutation check shows what it guards: `relay-rt` added to the kernel's dependencies fails it (the old test passed it).

**Files:**
- Modify: `xtask/src/ci.rs`

**Interfaces:**
- Consumes: Task 15's test.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `xtask/src/ci.rs`**

In `xtask/src/ci.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// command the user types runs as a program, so the kernel does not
    /// link the shell.
    #[test]
    fn the_kernel_does_not_depend_on_the_shell() {
        let out = crate::util::cargo()
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--offline",
            ])
            .output()
````

with:

````rust
    /// command the user types runs as a program, so the kernel does not
    /// link the shell, not even through another crate (`relay-rt` depends
    /// on it): the resolved graph is walked from the kernel over its normal
    /// and build dependencies.
    #[test]
    fn the_kernel_does_not_depend_on_the_shell() {
        let out = crate::util::cargo()
            .args(["metadata", "--format-version", "1", "--offline"])
            .output()
````

Replace:

````rust
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let kernel = meta["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "relay-kernel")
            .expect("the kernel is a package");
        let deps: Vec<&str> = kernel["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(deps.contains(&"vfs"), "the kernel's dependencies: {deps:?}");
        assert!(
            !deps.contains(&"shell"),
            "the kernel's dependencies: {deps:?}"
````

with:

````rust
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let name = |id: &serde_json::Value| {
            meta["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == *id)
                .map(|p| p["name"].as_str().unwrap().to_string())
                .unwrap()
        };
        let nodes = meta["resolve"]["nodes"].as_array().unwrap();
        let kernel = nodes
            .iter()
            .find(|n| name(&n["id"]) == "relay-kernel")
            .expect("the kernel is a package");
        let (mut todo, mut seen) = (vec![kernel], Vec::new());
        while let Some(node) = todo.pop() {
            for dep in node["deps"].as_array().unwrap() {
                let linked = dep["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null() || k["kind"] == "build");
                if linked && !seen.contains(&dep["pkg"]) {
                    seen.push(dep["pkg"].clone());
                    todo.push(nodes.iter().find(|n| n["id"] == dep["pkg"]).unwrap());
                }
            }
        }
        let deps: Vec<String> = seen.iter().map(name).collect();
        assert!(
            deps.contains(&"vfs".into()),
            "the kernel's dependencies: {deps:?}"
        );
        assert!(
            !deps.contains(&"shell".into()),
            "the kernel's dependencies: {deps:?}"
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p xtask ci::`

Expected: PASS: 5 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add xtask
git commit -m "Tests: the kernel links no shell through any crate it depends on"
````


### Task 17: The shell crate loses the in-kernel shell's way to programs

Decision 4: with the in-kernel shell gone, `System::spawn`, `System::wait`, `Output`, the in-process runner's `run_program` and `Ctx::wait_program` have no user: the in-process runner (`host-shell`, the tests) runs no programs, and a name that is not a command is `command not found`. The tests of programs run in-process go with `TestSystem`'s programs; two that are the spawning runner's too move there (a bare name that is a directory in `/bin`, a name too long for a file), and one says the in-process runner runs none. The task only removes code and moves tests, so it has no failing run; the mutation checks show what the moved tests guard: `ENAMETOOLONG` or `EISDIR` taken out of `cannot_start`'s bare-name rule each fail one.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 15.
- Produces: `shell::System` without `spawn`/`wait`; no `shell::Output`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use crate::testing::{Harness, Spawned};
    use alloc::string::String;
````

with:

````rust
    use crate::testing::{Harness, Spawned};
    use alloc::format;
    use alloc::string::String;
````

Replace:

````rust
    #[test]
    fn a_killed_program_is_reported_and_ctrl_c_is_only_so() {
````

with:

````rust
    #[test]
    fn names_of_directories_in_bin_are_not_commands() {
        let mut h = with_programs();
        for dir in ["/bin/..", "/bin/.", "/bin/"] {
            h.programs.refusals.push((dir, Errno::EISDIR));
        }
        h.programs.refusals.push(("./", Errno::EISDIR));
        for name in ["..", ".", "''"] {
            let shown = if name == "''" { "" } else { name };
            assert_eq!(
                h.spawning(name),
                (127, format!("relay-sh: {shown}: command not found\n")),
                "{name}"
            );
        }
        // Given as a path, a directory still says so.
        assert_eq!(
            h.spawning("./"),
            (126, "relay-sh: ./: Is a directory\n".into())
        );
    }

    #[test]
    fn a_name_too_long_for_a_file_is_not_found() {
        let mut h = with_programs();
        let long = "x".repeat(300);
        let (bare, path) = (format!("/bin/{long}"), format!("/{long}"));
        h.programs.refusals.push((bare.leak(), Errno::ENAMETOOLONG));
        h.programs
            .refusals
            .push((path.clone().leak(), Errno::ENAMETOOLONG));
        assert_eq!(
            h.spawning(&long),
            (127, format!("relay-sh: {long}: command not found\n")),
            "as bash says"
        );
        // Given as a path, the error is the path's.
        assert_eq!(
            h.spawning(&path),
            (126, format!("relay-sh: {path}: File name too long\n"))
        );
    }

    #[test]
    fn a_killed_program_is_reported_and_ctrl_c_is_only_so() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    use crate::Shell;
    use crate::testing::{FakeProgram, FakeStdout, Harness};
    use alloc::string::String;
````

with:

````rust
    use crate::Shell;
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
````

Replace:

````rust

    /// `t-args` in `/bin`, printing its arguments on fd 1 and a line on
    /// fd 2, and exiting with 3.
    fn with_programs() -> Harness {
        let mut h = Harness::new();
        h.dir("/bin");
        h.put("/bin/t-args", b"\x7fELF");
        h.put("/root/text", b"not a program");
        h.system.programs.push(FakeProgram {
            path: "/bin/t-args",
            writes: vec![(1, b"[1] a\n"), (2, b"t-args: note\n"), (1, b"[2] b c\n")],
            status: WaitStatus::exited(3),
        });
        h
    }

    #[test]
    fn a_name_that_is_no_built_in_runs_from_bin() {
        let mut h = with_programs();
        assert_eq!(
            h.run("t-args a 'b c' ''"),
            (3, "[1] a\nt-args: note\n[2] b c\n".into())
        );
        assert_eq!(
            h.system.spawned,
            [vec![
                b"t-args".to_vec(),
                b"a".to_vec(),
                b"b c".to_vec(),
                b"".to_vec()
            ]],
            "argument 0 is the name as typed, as in bash"
        );
        // A path runs as given, and is argument 0 as typed.
        assert_eq!(h.run("/bin/t-args").0, 3);
        h.run("cd /bin");
        assert_eq!(h.run("./t-args x").0, 3);
        assert_eq!(h.system.spawned[2], [b"./t-args".to_vec(), b"x".to_vec()]);
        // Built-ins come first.
        h.put("/bin/echo", b"\x7fELF");
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
        assert_eq!(h.system.spawned.len(), 3);
    }

    #[test]
    fn a_program_s_output_follows_the_redirection_and_its_errors_the_screen() {
        let mut h = with_programs();
        assert_eq!(h.run("t-args > /tmp/out"), (3, "t-args: note\n".into()));
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        h.run("t-args >> /tmp/out");
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n[1] a\n[2] b c\n");
        // A full disk is a write error, as for a built-in.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                1,
                "t-args: note\nt-args: write error: No space left on device\n".into()
            )
        );
    }

    #[test]
    fn what_cannot_run_says_why() {
        let mut h = with_programs();
        assert_eq!(
            h.run("nosuch x"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(
            h.run("/root/nosuch"),
            (
                127,
                "relay-sh: /root/nosuch: No such file or directory\n".into()
            )
        );
        assert_eq!(
            h.run("/root/text"),
            (126, "relay-sh: /root/text: Exec format error\n".into())
        );
        assert_eq!(
            h.run("/root"),
            (126, "relay-sh: /root: Is a directory\n".into())
        );
        // In a script too; the script goes on.
        h.put("/root/s.sh", b"nosuch\nt-args\n");
        let (status, out) = h.run("sh /root/s.sh");
        assert_eq!(status, 3);
        assert!(
            out.contains("+ nosuch\nrelay-sh: nosuch: command not found\n+ t-args\n[1] a\n"),
            "{out}"
        );
        let transcript = String::from_utf8(h.get("/root/s.log")).unwrap();
        assert!(
            transcript.contains("+ t-args\n[1] a\nt-args: note\n[2] b c\n"),
            "{transcript}"
        );
    }

    #[test]
    fn names_of_directories_in_bin_are_not_commands() {
        let mut h = with_programs();
        for name in ["..", ".", "''"] {
            let shown = if name == "''" { "" } else { name };
            assert_eq!(
                h.run(name),
                (127, format!("relay-sh: {shown}: command not found\n")),
                "{name}"
            );
        }
        // Given as a path, a directory still says so.
        assert_eq!(h.run("./"), (126, "relay-sh: ./: Is a directory\n".into()));
    }

    #[test]
    fn a_name_too_long_for_a_file_is_not_found() {
        let mut h = with_programs();
        let long = "x".repeat(300);
        assert_eq!(
            h.run(&long),
            (127, format!("relay-sh: {long}: command not found\n")),
            "as bash says"
        );
        // Given as a path, the error is the path's.
        assert_eq!(
            h.run(&format!("/{long}")),
            (126, format!("relay-sh: /{long}: File name too long\n"))
        );
    }

    #[test]
    fn without_programs_every_unknown_name_is_not_found() {
        let mut h = with_programs();
        h.system.no_programs = true;
        assert_eq!(
````

with:

````rust

    #[test]
    fn the_in_process_runner_runs_no_programs() {
        let mut h = Harness::new();
        h.dir("/bin");
        h.put("/bin/t-args", b"\x7fELF");
        assert_eq!(
````

Replace:

````rust
            (127, "relay-sh: /bin/t-args: command not found\n".into())
        );
    }

    #[test]
    fn a_program_sees_its_redirection_s_write_error() {
        let mut h = with_programs();
        static BIG: [u8; 5000] = [b'x'; 5000];
        h.system.programs[0].writes =
            vec![(1, b"small\n"), (1, &BIG), (1, b"more\n"), (2, b"note\n")];
        h.spy.zero_writes.set(true);
        let (status, out) = h.run("t-args > /tmp/out");
        assert_eq!(
            h.system.answers,
            [Ok(()), Err(Errno::ENOSPC), Err(Errno::ENOSPC), Ok(())],
            "buffered until 4 KiB, then the disk's error, for good; the screen takes fd 2"
        );
        assert_eq!(status, 1);
        assert!(
            out.ends_with("t-args: write error: No space left on device\n"),
            "{out}"
        );
        // To the screen, every write is fine.
        h.system.answers.clear();
        h.run("t-args");
        assert!(h.system.answers.iter().all(Result::is_ok));
    }

    #[test]
    fn a_program_stopped_by_ctrl_c_says_only_so_and_ends_a_script() {
        let mut h = with_programs();
        h.system.programs[0].status = WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C);
        assert_eq!(
            h.run("t-args"),
            (130, "[1] a\nt-args: note\n[2] b c\n^C\n".into())
        );
        h.put("/root/s.sh", b"t-args\necho after\n");
        let (status, out) = h.run("sh /root/s.sh");
        assert_eq!(status, 130);
        assert!(out.ends_with("[2] b c\n^C\n"), "{out}");
        assert!(!out.contains("after"), "the script stops: {out}");
    }

    #[test]
    fn a_killed_program_is_reported() {
        let mut h = with_programs();
        h.system.programs[0].status = WaitStatus {
            how: relay_abi::wait::KILLED,
            ..Default::default()
        };
        assert_eq!(
            h.run("t-args"),
            (
                137,
                "[1] a\nt-args: note\n[2] b c\nrelay-sh: t-args: killed\n".into()
            )
        );
        // A fault, redirected: the message goes to the screen.
        h.system.programs[0].status = WaitStatus::fault(
            relay_abi::wait::FAULT_PAGE,
            relay_abi::wait::ACCESS_READ,
            0,
            0x40_1a2c,
        );
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nrelay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
            )
        );
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        // A write error too: both are reported, and the kill's status stays.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nt-args: write error: No space left on device\n\
                 relay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
            )
        );
````

with:

````rust
            (127, "relay-sh: /bin/t-args: command not found\n".into())
        );
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, Programs, Stdout, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
````

Replace:

````rust
use relay_abi::WaitStatus;
use vfs::{
    DirEntry, Env, Errno, FileSystem, FileType, Ino, MemFs, MountTable, Node, Stat, StatFs, Vfs,
};

````

with:

````rust
use relay_abi::WaitStatus;
use vfs::{DirEntry, Env, Errno, FileSystem, Ino, MemFs, MountTable, Node, Stat, StatFs, Vfs};

````

Replace:

````rust

/// A program the test system runs: what it writes, on which fd, and how
/// it ends.
pub struct FakeProgram {
    pub path: &'static str,
    pub writes: Vec<(u32, &'static [u8])>,
    pub status: WaitStatus,
}

pub struct TestSystem {
````

with:

````rust

pub struct TestSystem {
````

Replace:

````rust
    pub poweroffs: u32,
    /// The programs `spawn` knows, by path; any other file is not one.
    pub programs: Vec<FakeProgram>,
    /// Programs cannot run at all (as on the host).
    pub no_programs: bool,
    /// The arguments of every program started.
    pub spawned: Vec<Vec<Vec<u8>>>,
    /// What each of the programs' writes was answered.
    pub answers: Vec<Result<(), Errno>>,
    /// The program started and not yet waited for.
    child: Option<usize>,
    /// What `reboot` and `poweroff` say, unforced, as a program's `power`
````

with:

````rust
    pub poweroffs: u32,
    /// What `reboot` and `poweroff` say, unforced, as a program's `power`
````

Replace:

````rust
            poweroffs: 0,
            programs: Vec::new(),
            no_programs: false,
            spawned: Vec::new(),
            answers: Vec::new(),
            child: None,
            power_error: None,
````

with:

````rust
            poweroffs: 0,
            power_error: None,
````

Replace:

````rust
        Ok(())
    }
    /// As the kernel does: the file must exist and be a program.
    fn spawn(
        &mut self,
        vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        if self.no_programs {
            return None;
        }
        let found = vfs.lookup(path).and_then(|node| {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            self.programs
                .iter()
                .position(|p| vfs.lookup(p.path.as_bytes()) == Ok(node))
                .ok_or(Errno::ENOEXEC)
        });
        Some(found.map(|i| {
            self.spawned.push(args.iter().map(|a| a.to_vec()).collect());
            self.child = Some(i);
            i as u32 + 1
        }))
    }
    fn wait(&mut self, pid: u32, out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        let i = self
            .child
            .take()
            .filter(|&i| i as u32 + 1 == pid)
            .ok_or(Errno::ECHILD)?;
        let p = &self.programs[i];
        for (fd, bytes) in &p.writes {
            self.answers.push(out(*fd, bytes));
        }
        Ok(p.status)
    }
````

with:

````rust
        Ok(())
    }
````

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use relay_abi::WaitStatus;
use vfs::{Errno, Node, Vfs};
````

with:

````rust
use core::fmt;
use vfs::{Errno, Node, Vfs};
````

Replace:

````rust
        self.streams().screen(bytes);
    }

    /// Waits for the program `pid` (`System::spawn`): what it writes to fd 1
    /// is standard output, to fd 2 the screen. A redirection file's write
    /// error is the program's too, from the write that met it on (output
    /// to a file is written in pieces of 4 KiB, so a short one meets it
    /// only when the command ends, and the shell reports it then).
    pub(crate) fn wait_program(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let mut streams = Streams {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            out: &mut self.out,
            transcript: &mut self.transcript,
        };
        self.system.wait(pid, &mut |fd, bytes| {
            if fd == 1 {
                streams.out(bytes)
            } else {
                streams.screen(bytes);
                Ok(())
            }
        })
    }
````

with:

````rust
        self.streams().screen(bytes);
    }
````

- [ ] **Step 5: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements `Console` and `System` over its console, clock,
//! memory manager, log, ACPI and programs; `xtask host-shell` over the
//! host terminal; `relay-rt` over system calls; the tests over buffers.
//! `Programs` is `/bin/sh`'s way to its commands.

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Node, Vfs};

/// Where a program's output goes (`System::wait`): what it writes, on fd
/// 1 or 2, and the write's error, if any.
pub type Output<'a> = dyn FnMut(u32, &[u8]) -> Result<(), Errno> + 'a;

````

with:

````rust
//! What the shell needs from its surroundings besides files (spec §7.3).
//! `relay-rt` implements `Console` and `System` over system calls; `xtask
//! host-shell` over the host terminal; the tests over buffers. `Programs`
//! is `/bin/sh`'s way to its commands.

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Node};

````

Replace:

````rust
    fn poweroff(&mut self, force: bool) -> Result<(), Errno>;
    /// Starts the program at `path`, read through `vfs`, with `args`
    /// (argument 0 is the path); its pid. `None` where programs cannot run
    /// (on the host). The in-kernel shell's way to programs until the shell
    /// itself becomes one (user-space gate plan 4).
    fn spawn(
        &mut self,
        _vfs: &mut dyn Vfs,
        _path: &[u8],
        _args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        None
    }
    /// Runs the program `spawn` started until it ends, giving what it
    /// writes to fds 1 and 2 to `out`, whose answer (a redirection file's
    /// write error) is the program's, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
    }
}
````

with:

````rust
    fn poweroff(&mut self, force: bool) -> Result<(), Errno>;
}
````

- [ ] **Step 6: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, Programs, Stdout, System};
pub use program::run_command;
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Programs, Stdout, System};
pub use program::run_command;
````

- [ ] **Step 7: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests, `cargo xtask host-shell` and the
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`); the spawning runner starts `/bin/<name>` for every
//! one (`/bin/sh`).

````

with:

````rust
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests and `cargo xtask host-shell`, where
//! there are no programs); the spawning runner starts `/bin/<name>` for
//! every one (`/bin/sh`).

````

Replace:

````rust
/// The command functions of `commands::COMMANDS`, run in the shell's
/// process; any other name is a program, through `System::spawn`.
pub(crate) struct InProcess;
````

with:

````rust
/// The command functions of `commands::COMMANDS`, run in the shell's
/// process; any other name is not found.
pub(crate) struct InProcess;
````

Replace:

````rust
            Some(command) => run_function(parts, command, args, file),
            None => run_program(parts, name, args, file),
        }
````

with:

````rust
            Some(command) => run_function(parts, command, args, file),
            None => not_found(name),
        }
````

Replace:

````rust
    Ran::said(status, message)
}

/// Runs a program through `System::spawn` (user-space gate §8.2), with
/// `name` as argument 0 and `args` as the others, as bash does. Its fd 1
/// is standard output (the redirection file, if any), its fd 2 the screen;
/// a running script's transcript gets both.
fn run_program(parts: Parts<'_>, name: &str, args: &[String], file: Option<(Node, u64)>) -> Ran {
    let path = program_path(name);
    let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
    argv.extend(args.iter().map(|w| w.as_bytes()));
    let pid = match parts.system.spawn(&mut *parts.vfs, path.as_bytes(), &argv) {
        Some(Ok(pid)) => pid,
        // No programs here (the host).
        None => return not_found(name),
        Some(Err(e)) => return cannot_start(name, e),
    };
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.transcript = parts.transcript.take();
    let mut ran = match ctx.wait_program(pid) {
        Ok(w) => ended(name, &w),
        Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
    };
    if let Err(e) = ctx.finish() {
        // A program that did not exit keeps its report and status.
        if ran.message.is_empty() {
            ran.status = 1;
        }
        ran.message
            .insert_str(0, &format!("{name}: write error: {e}\n"));
    }
    *parts.transcript = ctx.transcript.take();
    ran
}
````

with:

````rust
    Ran::said(status, message)
}
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 147 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -m "Shell: System::spawn and wait, the in-kernel shell's way to programs, go; the in-process runner runs none"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 40 scenario(s) passed`.

````bash
git push -u origin m2p4b/noshell
gh pr create --base main --head m2p4b/noshell --title "Milestone 2, plan 4b: The kernel without the shell" --body-file - <<'EOF'
## What

Milestone 2, plan 4b, tasks 15–17: `relay-kernel` no longer depends on `shell` (an xtask test checks it): `session.rs`, `File::ShellOutput`, `FdTable::shell`, the output hook, `renew_outputs`, `collect_orphans` and `give_console` go, and process 1's fds are the console; the shell crate's `System::spawn`/`wait`, `Output`, `run_program` and `Ctx::wait_program` go, and the in-process runner runs no programs.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing the NUC runs changes but what the QEMU scenarios cover; NUC check 4 (the last pull request of this plan) runs it all
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4b/noshell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-noshell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: NUC check 4 (Tasks 18–19)

`#same>` for check scripts, and `check4.sh`, run in QEMU by the `checks` scenario and on the NUC by the user. This pull request goes up as a draft; after the user's NUC check passes, the real transcripts and the results-log row go into a commit of it.

Branch `m2p4b/check4`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-check4`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4b/check4 /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-check4 origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-check4
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4b/noshell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4b/check4 /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-check4 m2p4b/noshell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4b/noshell>` and re-run `cargo xtask ci` before pushing.

### Task 18: Check scripts: `#same> NAME REGEX`

Spec §12.4 and decision 9: check 4 compares `free` before and after, as the `userfault` scenario does with `expect-same`. `#same> NAME REGEX` is one whole output line, as `#>` is, whose regex's group must capture what the first `#same>` of that name captured, in the same command or an earlier one; the value is taken from the first line the regex matches. A `#same>` without a name or a group is an error. Mutation checks: the comparison never failing, never made, the first value never kept, and a regex without a group accepted each fail the test.

**Files:**
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: plan 6's (milestone 1) `checks::{parse, check}`.
- Produces: `Expect::Same { name, re }`.

- [ ] **Step 1: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
    #[test]
    fn a_failure_names_the_expectation_and_the_line() {
````

with:

````rust
    #[test]
    fn a_value_must_be_what_it_was_the_first_time() {
        let script = "\
free
#> \\s+total\\s+used
#same> used Mem:\\s+\\d+\\s+(\\d+)
t-fault null-read
#> .*killed.*
free
#> \\s+total\\s+used
#same> used Mem:\\s+\\d+\\s+(\\d+)
";
        let c = parse(script, Machine::Qemu).unwrap();
        let log = |after: &str| {
            format!(
                "+ free\n  total  used\nMem: 100 40\n+ t-fault null-read\nkilled\n\
                 + free\n  total  used\nMem: 100 {after}\n"
            )
        };
        let r = check(&c, &log("40"));
        assert!(r.ok(), "{:?}", r.failures);
        assert_eq!(r.passed, 3);
        let r = check(&c, &log("44"));
        assert_eq!(r.passed, 2);
        assert_eq!(
            r.failures,
            ["line 6: `free`: `used` is 44, but it was 40 the first time"]
        );
        // As a line, it must match as `#>` does.
        let r = check(&c, &log("x"));
        assert_eq!(
            r.failures,
            ["line 6: `free`: expected /Mem:\\s+\\d+\\s+(\\d+)/, printed `Mem: 100 x` (line 2)"]
        );
        // Its regex needs a group to compare, and the name comes first.
        let e = parse("free\n#same> used Mem: \\d+\n", Machine::Qemu).unwrap_err();
        assert!(e.to_string().contains("needs a group"), "{e}");
        let e = parse("free\n#same> (\\d+)\n", Machine::Qemu).unwrap_err();
        assert!(e.to_string().contains("a name and a regex"), "{e}");
    }

    #[test]
    fn a_failure_names_the_expectation_and_the_line() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::a_value_must_be_what_it_was_the_first_time`.

- [ ] **Step 3: Change `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
//! mkdir /root/x           nothing expected: it must print nothing
//! ```
````

with:

````rust
//! mkdir /root/x           nothing expected: it must print nothing
//! free
//! #same> used Mem: +\d+ +(\d+).*
//!                         a line as `#>`, whose group must capture what the
//!                         first `#same>` named `used` captured
//! ```
````

Replace:

````rust
    Never(Regex),
}
````

with:

````rust
    Never(Regex),
    /// One whole output line, whose first group must be what the first
    /// `Same` of this name captured, in this command or an earlier one.
    Same { name: String, re: Regex },
}
````

Replace:

````rust
        let (on, never) = match tag {
            "" => (true, false),
            "!" => (true, true),
````

with:

````rust
        let (on, never) = match tag {
            "" | "same" => (true, false),
            "!" => (true, true),
````

Replace:

````rust
        let pattern = pattern.strip_prefix(' ').unwrap_or(pattern);
        let whole = || {
            Regex::new(&format!("^(?:{pattern})$")).with_context(|| format!("line {n}: bad regex"))
        };
        command.expect.push(match (never, pattern) {
````

with:

````rust
        let pattern = pattern.strip_prefix(' ').unwrap_or(pattern);
        let whole = |pattern: &str| {
            Regex::new(&format!("^(?:{pattern})$")).with_context(|| format!("line {n}: bad regex"))
        };
        if tag == "same" {
            let (name, pattern) = pattern
                .split_once(' ')
                .filter(|(name, _)| !name.contains(['(', '\\', '[']))
                .with_context(|| format!("line {n}: `#same>` needs a name and a regex"))?;
            let re = whole(pattern)?;
            if re.captures_len() < 2 {
                bail!("line {n}: `#same>`'s regex needs a group to compare");
            }
            command.expect.push(Expect::Same {
                name: name.to_string(),
                re,
            });
            continue;
        }
        let whole = || whole(pattern);
        command.expect.push(match (never, pattern) {
````

Replace:

````rust
    let mut at = 0;
    for (i, cmd) in commands.iter().enumerate() {
````

with:

````rust
    let mut at = 0;
    // What each `#same>` name captured first.
    let mut seen: Vec<(&str, String)> = Vec::new();
    for (i, cmd) in commands.iter().enumerate() {
````

Replace:

````rust
        let end = found.map_or(lines.len(), |k| at + k);
        match mismatch(&cmd.expect, &lines[at..end]) {
            None => report.passed += 1,
````

with:

````rust
        let end = found.map_or(lines.len(), |k| at + k);
        match mismatch(&cmd.expect, &lines[at..end])
            .or_else(|| not_the_same(&cmd.expect, &lines[at..end], &mut seen))
        {
            None => report.passed += 1,
````

Replace:

````rust

/// The pattern as written in the script.
````

with:

````rust

/// For each `#same>` of a command whose lines matched: the value its group
/// captures on the first line it matches, if that is not what the name
/// captured the first time.
fn not_the_same<'s>(
    expect: &'s [Expect],
    output: &[&str],
    seen: &mut Vec<(&'s str, String)>,
) -> Option<String> {
    for e in expect {
        let Expect::Same { name, re } = e else {
            continue;
        };
        let value = output
            .iter()
            .find_map(|l| re.captures(l))
            .and_then(|c| c.get(1))
            .map_or("", |m| m.as_str());
        match seen.iter().find(|(n, _)| n == name) {
            Some((_, first)) if first != value => {
                return Some(format!(
                    "`{name}` is {value}, but it was {first} the first time"
                ));
            }
            Some(_) => {}
            None => seen.push((name.as_str(), value.to_string())),
        }
    }
    None
}

/// The pattern as written in the script.
````

Replace:

````rust
            }
            Expect::Line(re) => {
                for j in 0..output.len() {
````

with:

````rust
            }
            Expect::Line(re) | Expect::Same { re, .. } => {
                for j in 0..output.len() {
````

Replace:

````rust
        }
        if let Expect::Line(re) = p
            && !next.contains(&true)
````

with:

````rust
        }
        if let Expect::Line(re) | Expect::Same { re, .. } = p
            && !next.contains(&true)
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "Check scripts: #same> NAME REGEX, a line whose value must be what it was the first time (free before and after)"
````


### Task 19: NUC check 4

Spec §12.4 and decision 9: `check4.sh` runs after `check3-b.sh`, 33 commands: `t-spawn fill` (60 children beside init and two shells, which also makes every kernel-stack slot's page tables), `t-spin 1` (the napping children end), `t-spawn 1000`, `free`, every `t-fault` kind, `t-spawn kill-new` and `orphan` (whose `t-spin 1` prints its line during the next `t-spin 1`, and ends before the last `free`), `t-args`, `t-abi`, `ls /bin` into a file, `cat` of a file into itself, a script that runs another, and `free` again (`#same>`). The `checks` scenario runs it after check 3's scripts, and xtask's test checks the recorded transcripts: QEMU's, taken from that scenario's disk, and the NUC's, QEMU's until NUC check 4 records it (the test's check that the NUC's lines are not QEMU's applies to scripts that have `#nuc>` lines, which check 4 does not). `docs/hardware-test.md` adds the step, the `exit` by hand and their failures; the README the new script. The red run is xtask's test, which cannot read the new transcripts.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Create: `rootfs/root/checks/check4.sh`
- Modify: `tests/e2e/checks.txt`
- Create: `xtask/fixtures/checks/check4.nuc.log`
- Create: `xtask/fixtures/checks/check4.qemu.log`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Tasks 18 (`#same>`), 6 (`t-abi`), 11 (init), 5 (`t-spin`, `t-spawn`).
- Produces: `rootfs/root/checks/check4.sh`; `xtask/fixtures/checks/check4.{qemu,nuc}.log`.

- [ ] **Step 1: Create `rootfs/root/checks/check4.sh`**

Create `rootfs/root/checks/check4.sh`:

````bash
# NUC check 4 (docs/hardware-test.md; milestone 2, plan 4b): after
# check3-b.sh, `sh checks/check4.sh`. Every command here is a program in
# ring 3, run by /bin/sh, which init started as process 2. Its transcript
# is check4.log; the `#>` lines are explained in check3-a.sh, and
# `#same> NAME regex` is a line whose group must capture what it captured
# the first time.

# Start afresh, so the check can be run again.
rm -f /root/check4.list /root/check4-a.sh /root/check4-a.log /root/check4-b.sh /root/check4-b.log

# Filling the process table makes the page tables of every kernel-stack
# slot, which stay: memory in use compares after it. Init, this shell and
# the script's leave room for 60 children, and init collects them as they
# end, a second later.
t-spawn fill
#> filled the table with 60 children
t-spin 1
#> \d+ iterations
t-spawn 1000
#> free frames before: \d+
#> free frames after: \d+
#> frames lost: 0
free
#> \s+total\s+used\s+free
#same> used Mem:\s+\d+\s+(\d+)\s+\d+
#> Heap:\s+\d+\s+\d+\s+\d+

# Every kind of fault kills the program, not the kernel, with its message
# and the kernel log's line (milestone 2, plans 2 and 3a).
t-fault null-read
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)
t-fault null-write
#> relay-sh: t-fault: killed \(page fault at 0x0, write, ip 0x4[0-9a-f]+\)
t-fault write-code
#> relay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, write, ip 0x4[0-9a-f]+\)
t-fault exec-data
#> relay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, execute, ip 0x4[0-9a-f]+\)
t-fault ud
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault div0
#> relay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)
t-fault sse
#> relay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)
t-fault gsbase
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault stack
#> relay-sh: t-fault: killed \(stack overflow at 0x7fffffeff[0-9a-f]{3}, ip 0x4[0-9a-f]+\)
t-fault kernel-read
#> relay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)
t-fault flags-exit
t-fault flags-ud
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault flags-tf
#> relay-sh: t-fault: killed \(CPU exception 1, ip 0x4[0-9a-f]+\)
t-fault flags-ac
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)

# Processes: one killed before it ran, and an orphan (a t-spin of a
# second, whose line comes during the next one), which init collects when
# it ends; a program built for another ABI is refused (plans 3a and 4b).
t-spawn kill-new
#> t-args: kill
t-spawn orphan
t-spin 1
#> \d+ iterations
#> \d+ iterations
t-args a 'b c' ''
#> \[1\] a
#> \[2\] b c
#> \[3\]\x20
t-abi
#> relay-sh: t-abi: Exec format error

# Under /bin/sh a program's standard output is the file itself: lines,
# not columns, and never its own input (plan 4b).
ls /bin > /root/check4.list
cat /root/check4.list
#> cat
#> clear
#> cp
#> ...
#> wc
cat /root/check4.list >> /root/check4.list
#> cat: /root/check4.list: input file is output file

# A script that runs another: both transcripts get the inner one's lines,
# this one included (plan 4a).
echo 'echo one' > /root/check4-a.sh
echo 'sh /root/check4-b.sh' >> /root/check4-a.sh
echo 'echo two' > /root/check4-b.sh
sh /root/check4-a.sh
#> \+ echo one
#> one
#> \+ sh /root/check4-b.sh
#> \+ echo two
#> two
cat /root/check4-b.log
#> \+ echo two
#> two

# The same memory in use as before all of it.
free
#> \s+total\s+used\s+free
#same> used Mem:\s+\d+\s+(\d+)\s+\d+
#> Heap:\s+\d+\s+\d+\s+\d+
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/checks.txt`**

Replace the whole of `tests/e2e/checks.txt` with:

````text
# The NUC checks' scripts (rootfs/root/checks/, spec §15 item 12) run in
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# check 4. Afterwards their transcripts on the disk must show what the
# scripts expect.
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
send sh checks/check4.sh
expect \n\+ rm -f /root/check4\.list .*\n
expect \n\+ free\n(.*\n){3}\+ t-fault null-read\n
expect \n\+ cat /root/check4-b\.log\n
expect root@relay:~# $
poweroff
check-script /root/checks/check3-a.sh
check-script /root/checks/check3-b.sh
check-script /root/checks/check4.sh
````

- [ ] **Step 3: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    /// check 3 of milestone 2's plan 3b on 2026-09-30
    /// (`docs/hardware-test.md`), copied off the stick unchanged. A `#nuc>` line must not need a line
    /// of its own next to the `#>` line for the same output, which QEMU
    /// alone cannot show.
    #[test]
````

with:

````rust
    /// check 3 of milestone 2's plan 3b on 2026-09-30
    /// (`docs/hardware-test.md`), copied off the stick unchanged; check 4's
    /// is QEMU's until NUC check 4 records the NUC's. A `#nuc>` line must
    /// not need a line of its own next to the `#>` line for the same
    /// output, which QEMU alone cannot show.
    #[test]
````

Replace:

````rust
            ),
        ];
````

with:

````rust
            ),
            (
                include_str!("../../rootfs/root/checks/check4.sh"),
                include_str!("../fixtures/checks/check4.qemu.log"),
                include_str!("../fixtures/checks/check4.nuc.log"),
            ),
        ];
````

Replace:

````rust
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's.
            let r = check(&parse(script, Machine::Nuc).unwrap(), qemu);
            assert!(!r.ok(), "part {i}");
        }
````

with:

````rust
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's (check 4 has none: it runs programs only).
            if script.contains("#nuc>") {
                let r = check(&parse(script, Machine::Nuc).unwrap(), qemu);
                assert!(!r.ok(), "part {i}");
            }
        }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: compile errors such as `` couldn't read `xtask/src/../fixtures/checks/check4.qemu.log`: No such file or directory (os error 2) ``; `` couldn't read `xtask/src/../fixtures/checks/check4.nuc.log`: No such file or directory (os error 2) ``.

- [ ] **Step 5: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
(no `system.img`, or a shell that keeps ending) shows an error screen and
restarts at a key.

````

with:

````markdown
(no `system.img`, or a shell that keeps ending) shows an error screen and
restarts at a key. The kernel holds no shell of its own any more.

````

Replace:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of both scripts.

````

with:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh` and `sh checks/check4.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of the three scripts.

````

- [ ] **Step 6: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 5 replacements, top to bottom:

Replace:

````markdown

## Check 3 — files on the stick (plans 5 and 6; milestone 2)

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`). Since plan 6 the
commands come from two scripts on the stick, `/root/checks/check3-a.sh` and
`check3-b.sh` (in the repository under `rootfs/root/checks/`): `sh` runs
each line as if it were typed, shows it as `+ <command>` before its output,
````

with:

````markdown

## Check 3 — files on the stick (plans 5 and 6; milestone 2), and check 4

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`). Since plan 6 the
commands come from two scripts on the stick, `/root/checks/check3-a.sh` and
`check3-b.sh` (in the repository under `rootfs/root/checks/`), and since
milestone 2's plan 4b a third, `check4.sh` (NUC check 4 of the user-space
gate, spec §12.4). Every command is a program now, and `/bin/sh` runs
them, which the kernel's init starts as process 2: `sh` runs
each line as if it were typed, shows it as `+ <command>` before its output,
````

Replace:

````markdown
     add programs)
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.

````

with:

````markdown
     add programs)
   - the motd (`Welcome to Relay OS.`), which init prints, and the prompt
     `root@relay:~# ` of the `/bin/sh` it started.

````

Replace:

````markdown
   restart are read back.
7. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
8. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
   <time>)`, with the UTC times of the two runs (after `flash --kernel` the
   transcripts of an earlier run stay on the stick, so check the times). A
````

with:

````markdown
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
8. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
9. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`,
   `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run <time>)`
   and `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run
   <time>)`, with the UTC times of the three runs (after `flash --kernel` the
   transcripts of an earlier run stay on the stick, so check the times). A
````

Replace:

````markdown
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …`, then the error screen | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

with:

````markdown
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …`, then the error screen | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `exit` at the prompt gives no new prompt, or the error screen | Init does not see the shell end, or its clock runs fast (three ends within 10 s) | Photograph the screen: the log's lines on it end with init's `init: /bin/sh (pid N) …` lines |
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

Replace:

````markdown
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |
| `verify-usb`: `check3-a.sh: FAILED` | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

with:

````markdown
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |
| `verify-usb`: `check3-a.sh: FAILED` (or another script) | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

- [ ] **Step 7: Create `xtask/fixtures/checks/check4.nuc.log`**

Create `xtask/fixtures/checks/check4.nuc.log`:

````text
+ rm -f /root/check4.list /root/check4-a.sh /root/check4-a.log /root/check4-b.sh /root/check4-b.log
+ t-spawn fill
filled the table with 60 children
+ t-spin 1
4493148160 iterations
+ t-spawn 1000
free frames before: 247463
free frames after: 247463
frames lost: 0
+ free
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
+ t-fault null-read
relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x4001e9)
+ t-fault null-write
relay-sh: t-fault: killed (page fault at 0x0, write, ip 0x40026b)
+ t-fault write-code
relay-sh: t-fault: killed (page fault at 0x400010, write, ip 0x400431)
+ t-fault exec-data
relay-sh: t-fault: killed (page fault at 0x403000, execute, ip 0x403000)
+ t-fault ud
relay-sh: t-fault: killed (invalid opcode, ip 0x400053)
+ t-fault div0
relay-sh: t-fault: killed (divide error, ip 0x40008a)
+ t-fault sse
relay-sh: t-fault: killed (FPU/SSE instruction, ip 0x40012b)
+ t-fault gsbase
relay-sh: t-fault: killed (invalid opcode, ip 0x4002ed)
+ t-fault stack
relay-sh: t-fault: killed (stack overflow at 0x7fffffeffff8, ip 0x4002a8)
+ t-fault kernel-read
relay-sh: t-fault: killed (page fault at 0xffff800000000000, read, ip 0x40036e)
+ t-fault flags-exit
+ t-fault flags-ud
relay-sh: t-fault: killed (invalid opcode, ip 0x400506)
+ t-fault flags-tf
relay-sh: t-fault: killed (CPU exception 1, ip 0x40049d)
+ t-fault flags-ac
relay-sh: t-fault: killed (invalid opcode, ip 0x40010a)
+ t-spawn kill-new
t-args: kill
+ t-spawn orphan
+ t-spin 1
2300575744 iterations
2312110080 iterations
+ t-args a 'b c' ''
[1] a
[2] b c
[3] 
+ t-abi
relay-sh: t-abi: Exec format error
+ ls /bin > /root/check4.list
+ cat /root/check4.list
cat
clear
cp
date
df
dmesg
echo
free
head
ls
mkdir
mv
poweroff
pwd
reboot
rm
rmdir
sh
stat
sync
t-abi
t-args
t-fault
t-files
t-mem
t-read
t-spawn
t-spin
t-sys
t-tee
tail
touch
uname
wc
+ cat /root/check4.list >> /root/check4.list
cat: /root/check4.list: input file is output file
+ echo 'echo one' > /root/check4-a.sh
+ echo 'sh /root/check4-b.sh' >> /root/check4-a.sh
+ echo 'echo two' > /root/check4-b.sh
+ sh /root/check4-a.sh
+ echo one
one
+ sh /root/check4-b.sh
+ echo two
two
+ cat /root/check4-b.log
+ echo two
two
+ free
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
````

- [ ] **Step 8: Create `xtask/fixtures/checks/check4.qemu.log`**

Create `xtask/fixtures/checks/check4.qemu.log`:

````text
+ rm -f /root/check4.list /root/check4-a.sh /root/check4-a.log /root/check4-b.sh /root/check4-b.log
+ t-spawn fill
filled the table with 60 children
+ t-spin 1
4493148160 iterations
+ t-spawn 1000
free frames before: 247463
free frames after: 247463
frames lost: 0
+ free
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
+ t-fault null-read
relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x4001e9)
+ t-fault null-write
relay-sh: t-fault: killed (page fault at 0x0, write, ip 0x40026b)
+ t-fault write-code
relay-sh: t-fault: killed (page fault at 0x400010, write, ip 0x400431)
+ t-fault exec-data
relay-sh: t-fault: killed (page fault at 0x403000, execute, ip 0x403000)
+ t-fault ud
relay-sh: t-fault: killed (invalid opcode, ip 0x400053)
+ t-fault div0
relay-sh: t-fault: killed (divide error, ip 0x40008a)
+ t-fault sse
relay-sh: t-fault: killed (FPU/SSE instruction, ip 0x40012b)
+ t-fault gsbase
relay-sh: t-fault: killed (invalid opcode, ip 0x4002ed)
+ t-fault stack
relay-sh: t-fault: killed (stack overflow at 0x7fffffeffff8, ip 0x4002a8)
+ t-fault kernel-read
relay-sh: t-fault: killed (page fault at 0xffff800000000000, read, ip 0x40036e)
+ t-fault flags-exit
+ t-fault flags-ud
relay-sh: t-fault: killed (invalid opcode, ip 0x400506)
+ t-fault flags-tf
relay-sh: t-fault: killed (CPU exception 1, ip 0x40049d)
+ t-fault flags-ac
relay-sh: t-fault: killed (invalid opcode, ip 0x40010a)
+ t-spawn kill-new
t-args: kill
+ t-spawn orphan
+ t-spin 1
2300575744 iterations
2312110080 iterations
+ t-args a 'b c' ''
[1] a
[2] b c
[3] 
+ t-abi
relay-sh: t-abi: Exec format error
+ ls /bin > /root/check4.list
+ cat /root/check4.list
cat
clear
cp
date
df
dmesg
echo
free
head
ls
mkdir
mv
poweroff
pwd
reboot
rm
rmdir
sh
stat
sync
t-abi
t-args
t-fault
t-files
t-mem
t-read
t-spawn
t-spin
t-sys
t-tee
tail
touch
uname
wc
+ cat /root/check4.list >> /root/check4.list
cat: /root/check4.list: input file is output file
+ echo 'echo one' > /root/check4-a.sh
+ echo 'sh /root/check4-b.sh' >> /root/check4-a.sh
+ echo 'echo two' > /root/check4-b.sh
+ sh /root/check4-a.sh
+ echo one
one
+ sh /root/check4-b.sh
+ echo two
two
+ cat /root/check4-b.log
+ echo two
two
+ free
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 10: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add README.md docs rootfs tests xtask
git commit -m "Check scripts: NUC check 4 runs the fault kinds, t-spawn, free before and after, t-args, t-abi, output into files and a script that runs another; exit by hand restarts the shell"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 40 scenario(s) passed`.

````bash
git push -u origin m2p4b/check4
gh pr create --draft --base main --head m2p4b/check4 --title "Milestone 2, plan 4b: NUC check 4" --body-file - <<'EOF'
## What

Milestone 2, plan 4b, tasks 18–19: `#same> NAME REGEX` in check scripts (a value the same as the first time); `check4.sh`: every `t-fault` kind, `t-spawn`, `free` before and after, `t-args`, `t-abi`, output into files and a script that runs another, run by the `checks` scenario after check 3's scripts; `docs/hardware-test.md` gains check 4 with `exit` by hand.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 4 of `docs/hardware-test.md` (check 3's scripts, `check4.sh` and `exit` by hand), run by the user with `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4b/check4 --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for NUC check 4**

Ask the user to run NUC check 4 (`docs/hardware-test.md`, check 3's steps and step 7) with `cargo xtask flash --full` from this worktree, and to report its results; the pull request stays a draft until they do. The stick is then usually plugged into this machine, its root mounted at `/media/maw/relayroot`.

- [ ] **Record the NUC's transcripts in this pull request**

After a pass: check the stick with `cargo xtask verify-usb` (it only reads it), copy the real transcripts over the recorded ones, and add the results-log row of `docs/hardware-test.md` (date, `4`, this branch's commit, `Pass`, and the notes: the three scripts' counts, the `exit` step, anything the user saw):

````bash
cargo xtask verify-usb
cp /media/maw/relayroot/root/checks/check3-a.log xtask/fixtures/checks/check3-a.nuc.log
cp /media/maw/relayroot/root/checks/check3-b.log xtask/fixtures/checks/check3-b.nuc.log
cp /media/maw/relayroot/root/checks/check4.log xtask/fixtures/checks/check4.nuc.log
cargo test -p xtask checks
````

Expected: `verify-usb` ends with the three scripts' `ok` lines, and the tests pass. Then `git diff` shows only the transcripts and the new row (check that no earlier row changed), and:

````bash
cargo xtask ci
git add xtask/fixtures/checks docs/hardware-test.md
git commit -m "Check scripts: the transcripts of NUC check 4, and its row in the results log"
git push
gh pr ready m2p4b/check4
````

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4b-check4
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
