# Milestone 2 · Plan 5: Hardening and 0.3.0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Milestone 2 ends. What earlier plans deferred to plan 5 is fixed or ruled out: the error screen ends every other process before it draws and counts rows as the terminal moves; code that nothing uses goes; `read_dir` leaves its own buffer out of the heap it allows; the TLB is flushed under `arch/`; the guest tests' loops are bounded; milestone 1's last findings (a bouncing USB port, QMP, the loader-rules test, `verify-usb`, `parse_fadt`) are fixed; the e2e runner gains `system-drop` and `reset-key`. The spec's §16 gets item 7, and the version becomes 0.3.0. It ends with `cargo xtask ci` green and NUC checks 3 and 4 passed on a stick written by `flash --full` (spec §1.4).

**Architecture:** No new component. The kernel's process table gains `kill_all_but_init`, which the error screen calls through `proc::kill_others`; `error_screen::rows_of` moves a cursor over `term`'s own parser, and the screen's clear starts with CAN; `arch::tlb` wraps the TLB instructions, and a test keeps every other `tlb` path out of the kernel. `crates/usb` gains `UsbError::Unstable`. xtask's QMP client keeps a partial message, `verify-usb` compares a transcript's time with the build time of the `system.img` on the stick's ESP (`image::esp_read`), and the e2e runner gains a before-boot step (`system-drop`, over `userland::without`) and a step (`reset-key`).

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model), e2fsprogs 1.47 (`debugfs`), binutils' `readelf` for tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.4, §3.1, §8.5, §10, §11.2, §12.3, §12.4, §13 step 5, §14, §16 item 7)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 5 of 7, the last of milestone 2.

## In brief

- **Size.** 17 tasks in three code pull requests, plus this plan as PR 1: PR 2 the kernel's fixes and the guest tests' bounds; PR 3 xtask's fixes and two new e2e steps; PR 4 version 0.3.0 and the docs, a draft until NUC checks 3 and 4 pass, whose transcripts go into it (decision 1). Nothing waits for milestone 3.
- **NUC checks 3 and 4 of plan 4b needed no fix.** Everything here was deferred by earlier plans' reviews (roadmap, "What plan 4b leaves for plan 5", and "Deferred findings of milestone 1"); decision 13 says which earlier deferred lines are settled and how.
- **The error screen** ends every other process before it draws (decision 2), counts a line's rows as the terminal moves and shows an escape instead of obeying it (decision 3); `system_nosh` shows it for a `/bin/sh` that cannot start, and `system_missing` leaves it with a key on the USB keyboard (decision 9).
- **The tick poll's cost** (decision 10) was measured on the NUC with a throwaway kernel built without the poll on ticks that interrupt ring 3: `t-spin 1` made 4677697536 iterations without it and 4594860032 with it, so the poll takes 1.8 % of a spinning program's time. It stays on every tick.
- **Version 0.3.0** (decision 11): a spike bumped it first and ran everything: only `uname`'s test, `shell`, `check3-a.sh` and the recorded transcripts depend on it. Milestone 2's definition of done is met item by item (decision 12).
- **The prototype's review.** A fresh reviewer read the whole prototype, ran every crate's tests, throwaway probes and a QEMU replay of check 4's new step under KVM on the NUC's CPU model (an i7-1260P), and found no critical, 1 important and 6 minor real defects. Two are fixed in a task of their own after the task they concern, with the mutation checks that show what they guard; one by redesigning the task it concerns; four in the tasks that own them (a docs step, the stick's README, a scenario's literal, a test-only function). It found correct the kill before the screen (zombies, the first reason, the blocked woken, no lock across a switch, the killed lines in the log only, the tail taken first, the tees flushed by the screen's sync), the row count against `Terminal::apply` at every edge, the removals, `read_dir`'s boundary, `Unstable`'s reach and the boot line, the bounds, QMP's partial buffer, the loader scanner on the real sources, debugfs's two time forms, `system-drop` and `reset-key`, and the version's reach:

| Finding (review) | Decision |
|---|---|
| Important: NUC check 4's new error-screen step, right after `check4.sh`, expects wrapped `xhci` lines, but the log's last 20 lines are then `check4.sh`'s, none wider than the screen: the NUC would never check the row count | Fixed in Task 17: the step comes after a `reboot`, and names the 122-column line the NUC's real boot log holds |
| Minor: `only_arch_flushes_the_tlb` looked for `instructions::tlb` only; a `tlb` imported in braces passed it | Fixed, Task 7 |
| Minor: `verify-usb` compared a transcript with its script, which `flash --full` writes while it erases the transcripts: the check could only fail on a NUC clock running behind, and missed the transcripts `flash --kernel` leaves | Fixed in Task 13, redesigned: a transcript is compared with the `system.img` on the stick |
| Minor: a program's lone ESC swallows the error screen's colour reset, and the screen is drawn black on black | Fixed, Task 3; the red panic screen has the same weakness and is left, a kernel bug's screen |
| Minor: `system_nosh` hard-codes `33 programs`, which milestone 3's programs would break | Fixed in Task 14 (`\d+`) |
| Minor: `PageTables::translate` has test callers only; the scan of decision 4 missed it | Fixed in Task 4 (`#[cfg(test)]`, with `Cache::of_entry`) |
| Minor: the stick's `/root/README` still says milestone 1 | Fixed in Task 17 |
| Declined to judge: the tick poll's cost (then pending the NUC); a device that settles just after 2 s of bouncing waits for a replug; the NUC's clock against the host's; `respawn`'s orphan under TCG | Ruled: the NUC's counts are decision 10's (1.8 %, kept); the second is Linux's connect-debounce rule and decision 8's; the NUC's clock is UTC, as the stick's times show; TCG is a local convenience only |

## Where this plan fits

Plan 5 implements spec §13 step 5, milestone 2's last. It builds on plan 4b: process 1 is the kernel's init, the error screen, the kernel without the shell, and NUC check 4 (roadmap, "What plan 4b leaves for plan 5"). It leaves milestone 3 the console calls that refuse a caller outside the foreground group and a test of the error screen taking the console back (roadmap, "What plan 5 leaves for milestone 3").

## Working conventions

- Plan 5 lands as **four pull requests** (table below). This plan, with the spec's §16 item 7 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. PR 4 needs NUC checks 3 and 4: it goes up as a draft, and after the user reports a pass, the real transcripts and the results-log row go into a commit of the same PR, which is then marked ready. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-5-hardening.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 1 and 16 have scenario red runs too (`respawn` and `shell`). Task 15's scenario uses the step the task adds, so its red run is the parser's test. Four tasks have no failing run, and their introductions say why: Task 4 removes code and ports tests of correct code, Task 8 changes no behaviour, Task 10 only bounds test programs (the mutation checks, each a kernel that never says stop, show what the bounds do), and Task 17 changes documents only. The mutation checks the prototype ran are named in each task's introduction.
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; it happened twice while plan 3b's prototype was made, under a mutant. Mutation checks run under an address-space limit and a timeout (`tmp/m2p5/mutate.py`).
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `967d532` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request, NUC and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p5/plan` | — | The spec's §16 item 7 and the corrections to its body, the roadmap's notes for plan 5 and milestone 3, this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p5/kernel` | 1–10 | The error screen ends other processes and counts rows as the terminal moves; dead code; `read_dir`'s heap; `arch::tlb`; `parse_fadt`; a bouncing USB port tried once; bounded guest loops | `lint`, `unit`, `e2e` |
| 3 | `m2p5/xtask` | 11–15 | QMP's cut messages; the loader-rules test; `verify-usb`'s stale transcripts; `system-drop` and `system_nosh`; `reset-key` | `lint`, `unit`, `e2e` |
| 4 | `m2p5/release` | 16–17 | Version 0.3.0; the README and `docs/hardware-test.md`; NUC checks 3 and 4 (draft until they pass) | `lint`, `unit`, `e2e`; NUC checks 3 and 4 |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 5 adds no crate. The kernel has no dev-dependencies and does not depend on `shell`; `crates/usb` has no dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- Every command prints exactly what it prints in milestone 1, and every milestone 1 scenario passes unchanged but for the startup line and the version (gate §1.4).
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. Nothing waits for ever but the error screen, which waits for a person's key. Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 120×33 cells and no serial port.
- Shell messages follow GNU coreutils and bash; `/bin/sh` names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- A plan that changes a check script ends with `cargo xtask flash --full` for its NUC check, and the recorded NUC transcripts in `xtask/fixtures/checks/` are replaced by the real ones copied off the stick, in a commit of the plan's last PR, with the results-log row.
- No tag or GitHub release without the user's word: PR 4 proposes the text of `v0.3.0` and its release.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 7 in PR 1:

1. **Plan 5 is one plan** (§13), in four pull requests: this plan; the kernel's fixes and the guest tests' bounds; xtask's fixes and the runner's new steps; version 0.3.0, with NUC checks 3 and 4 on a stick written by `flash --full`. No item waits for milestone 3.
2. **The error screen ends every other process first** (§11.2, corrected in its body). Before it draws, init kills every process but itself, as `kill` does: each ends before it runs another instruction of its program, and says so in the kernel log only (`pid <n> (<path>): killed: kill`), so nothing writes over the screen or scrolls its heading away (plan 4b's final review found an orphan's `t-spin` line under `Press any key to reboot.`). Killing them, rather than dropping their output, also keeps a spinning program from sharing the CPU with the wait; their zombies stay, since the machine restarts. `respawn` leaves a `t-spin 1` orphan running at the third `exit`.
3. **The error screen counts rows as the terminal moves** (§11.2). A line's rows come from `term`'s own parser and the terminal's cursor rules: a tab moves to the next multiple of 8 but never past the last column, a carriage return to the first, a backspace one back, and a character after the last column starts a row. A test checks each kind of line against `term::Terminal` itself at the NUC's 120×33. An escape that is not a colour, which a program's bytes can leave in the kernel log (`ESC c` resets the terminal), is shown as `?`, not obeyed; and the screen's clear starts with CAN, so an escape a program left unfinished cannot swallow the reset of its colours (the prototype's review drew the screen black on black after a lone ESC).
4. **What nothing uses goes** (§16 item 6). `exec::arg_bytes` (a test helper now), `InputQueue::take_interrupt` (its tests check the queue's bytes instead), `tty::is_line_mode` and `InputQueue::is_line_mode`; a scan for `pub` functions with test callers only also found `OpenFile::is_dir` (since plan 3b) and `arch::idle_forever` (since milestone 1's shell ended the boot), and the prototype's review `PageTables::translate`, which only tests use (`#[cfg(test)]` now).
5. **`read_dir`'s heap** (§16 item 4). The room it allows the directory's list leaves out its own buffer, up to 64 KiB of the same heap.
6. **The TLB is `arch`'s** (§3.1). `mm` flushes it through `arch::tlb`, and a host test walks the kernel's sources and fails if a file outside `arch/` names a `tlb` path other than `arch::tlb`, however imported, or `invlpg` (the prototype's review fooled a first test, which looked for `instructions::tlb` only, with an import in braces). `mm::init`'s CR3 and PAT setup and milestone 1's port I/O stay where they are, for the aarch64 port's architecture layer after the gate (§15).
7. **Every loop in a guest test is bounded** (§8.5). `t-files`' opens (64) and `read_dir` calls (64), `t-tee`'s reads (64 KiB) and pushes (16): a kernel that never says stop makes the scenario fail at once (`and no end`), instead of a program looping on (plan 3b's finding).
8. **Milestone 1's plan-5 findings** (M1 §15 item 12). A connection that is still bouncing when the debounce gives up after 2 s is `UsbError::Unstable` (`connection not stable`), which the host does not try again (it cost 3 × 2 s); a QMP message cut at a wait's deadline is kept and read whole later, and the wait sets the timeout on the reader's own socket; the loader-rules test finds comments outside string and character literals only; `verify-usb` fails a transcript older than the `system.img` on the stick's ESP (`the transcript is older than the system on the stick (system.img built …): run the script again`), since it shows what an older kernel and programs did: `flash --kernel` keeps the transcripts, and `flash --full` erases them (the prototype compared with the script, which the review found could only fail when the NUC's clock runs behind; it is UTC, as the stick's times show); `parse_fadt` reads the 32-bit DSDT field once.
9. **The e2e runner** (§10, §12.3). `system-drop NAME` is a before-boot step (`system.img` without that program), and the scenario `system_nosh` sees the error screen say `/bin/sh cannot start: No such file or directory`: §16 item 6 said a host test covered it, but only the reason's text was. `reset-key` leaves the error screen with Enter on the USB keyboard, as a person does on the NUC; `system_missing` uses it.
10. **The tick poll's cost** (§14, corrected in its body). On the NUC, `t-spin 1` makes 4594860032 iterations with the console polled on every tick that interrupts ring 3 (as in plans 3b and 4b), and 4677697536 with a throwaway kernel built without that poll (never merged): the poll takes 1.8 % of a spinning program's time, about 18 µs of each 1 ms tick. That is not too much for milestone 2, so the poll stays on every tick; §14's fallback, every 4th tick, would save about 1.3 % and poll the keyboards a quarter as often, and waits for a workload that calls for it.
11. **Version 0.3.0** (§1.4, §2). The workspace's version, `uname -a` in `shell` (milestone 1's scenario, changed only by the version, as milestone 1's own bump did) and in `check3-a.sh`, and the recorded transcripts. A spike bumped the version first: nothing else depends on it. Milestone 2 ends with NUC checks 3 and 4 on a stick written by `flash --full`; check 4 gains the error screen by hand (§12.4, corrected in its body): after a `reboot`, so that the kernel log's last lines are the boot's and one of them is wider than the screen (right after `check4.sh` none is, the prototype's review found), three quick `exit`s, and a key on the K120 restarts the machine. The stick's `/root/README` no longer says milestone 1.
12. **Milestone 2's definition of done** (§1.4). Item 1: `cargo xtask ci` runs 41 scenarios, milestone 1's unchanged but for the startup lines and `shell`'s version. Item 2: NUC checks 3 and 4, in plan 5's last pull request. Item 3: an xtask test walks `cargo metadata`'s resolved graph (since plan 4b), and every command but `cd`, `exit` and `help` runs from `/bin`. Nothing is unmet.
13. **Earlier deferred findings, settled.** Plan 3a's: orphans piling up (plans 4a and 4b); §5.1's `stac`/`clac` (its body says no copy needs them) and §8.5's table (corrected in its body: `t-sys`'s kinds, and the kinds a program's children run are left out); a program seeing its redirection's write error end to end (`diskfull`'s `t-files full > more` under `/bin/sh`). Plan 3b's: `EISDIR`, zombie groups, `POWER_FORCE`, the tees before a shutdown and the readers woken (plan 4b); `flush_pages`, the guest loops and `read_dir` (above). Plan 4a's `ENOSPC` for a write that takes nothing (plan 4b). Left for milestone 3: the console calls refusing a caller outside the foreground group, and a test of the error screen taking the console back, which only milestone 3's `kill` can reach; milestone 1's "still out of the gate" list stays out (§15).

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **The error screen while other programs run, and what the kernel log holds:** an orphan still printing (`t-spawn orphan`), a program spinning without system calls, one blocked reading the console or in `wait` or `sleep`, a key already queued; log lines holding tabs, carriage returns, backspaces, escapes (`ESC c`), bytes that are no UTF-8, and lines wider than the console. Expected: every other process ends before it can print, the heading stays at the top of the NUC's 33 rows, no escape is obeyed, and a key typed after the screen came, on the USB keyboard too, restarts the machine. Tests: `respawn` (Task 1), `every_process_but_1_can_be_killed_at_once` (Task 1), `a_line_takes_the_rows_the_terminal_gives_it` and `the_heading_stays_on_the_nuc_s_terminal_whatever_the_log_holds` (Task 2), `the_screen_has_its_colours_whatever_a_program_left_the_terminal_in` (Task 3), `system_missing` with `reset-key` (Task 15), `system_nosh` (Task 14).
2. **A machine whose devices or archive misbehave at boot:** a USB port that keeps bouncing past the debounce's 2 s (connected or not at the end), one that is unplugged during setup, a `system.img` without `sh`. Expected: one attach that gives up after 2 s with `connection not stable` and the boot going on, no retry; the error screen naming `/bin/sh cannot start: No such file or directory`. Tests: `a_connection_that_keeps_bouncing_is_given_up_once` and `a_connection_that_keeps_bouncing_is_given_up_after_2_s_without_a_reset` (Task 9), `system_nosh` (Task 14).
3. **Programs at the kernel's limits:** `read_dir` of a big directory with a big buffer when the heap is short, the 33rd fd, the fifth tee, a file read to its end, memory given back in many pages. Expected: `ENOMEM` instead of a kernel heap panic; `EMFILE`, `EBUSY` and the end of a file as before, and a test program that stops at its bound if the kernel never says stop; the TLB entries of what was given back dropped. Tests: `read_dir_s_own_buffer_is_not_room_for_the_directory` (Task 5), `files` and `tees` with bounded loops (Task 10), `only_arch_flushes_the_tlb` and `memory` (Task 6), `the_scan_sees_every_way_to_name_the_tlb` (Task 7).
4. **NUC checks 3 and 4 on 0.3.0:** a stick written by `flash --full`, then the scripts; transcripts left by a later `flash --kernel`; a run in the same second as the build; the error screen by hand after a reboot. Expected: `uname -a` says `Relay relay 0.3.0 x86_64`, `verify-usb` passes a fresh run and fails a stale transcript with a line that says what to do. Tests: `a_transcript_older_than_the_system_on_the_stick_fails_the_check` and `a_file_of_the_esp_reads_back` (Task 13), `uname_prints_the_system`, `shell`, `the_check_scripts_pass_on_both_machines` and `checks` (Task 16).
5. **The test tools themselves:** a QMP event cut at a wait's deadline, a loader source with `//` in a string or a quote in a character literal, a scenario with `reset-key` or `system-drop`. Expected: no false failure, no false pass. Tests: `an_event_cut_at_the_deadline_is_read_whole_later` (Task 11), `a_string_is_code_even_with_slashes_in_it` (Task 12), `parses_reboot_and_poweroff_steps` (Task 15), `parses_esp_deletes_other_abis_and_programs_left_out` and `a_program_can_be_left_out_of_the_archive` (Task 14).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `kernel/src/error_screen.rs`, `kernel/src/proc.rs`, `kernel/src/proc/table.rs` | The error screen: every other process killed first (`kill_others`, `kill_all_but_init`); a line's rows as the terminal moves; an escape shown as `?`; `CLEAR` with CAN first |
| `kernel/src/{exec,input,tty,file}.rs`, `kernel/src/arch/mod.rs`, `kernel/src/mm/paging.rs` | What nothing used, gone; `translate` the tests' |
| `kernel/src/syscall/files.rs` | `read_dir`'s heap room without its own buffer |
| `kernel/src/arch/tlb.rs`, `kernel/src/mm/mod.rs` | The TLB flushed under `arch/`, and the test that keeps it there |
| `kernel/src/acpi/tables.rs` | `parse_fadt`'s DSDT field read once |
| `crates/usb/src/{error,host}.rs`, `crates/usb/src/xhci/device.rs` | `UsbError::Unstable`, not tried again |
| `userland/tests/src/bin/{t-files,t-tee}.rs` | Bounded loops |
| `xtask/src/{qmp,flash,image,e2e,userland}.rs`, `crates/shell/src/lib.rs`, `boot/src/lib.rs` | QMP's partial messages; `verify-usb`'s stale transcripts (`esp_read`, `shell::time` public); `system-drop`, `reset-key`, `userland::without`; the loader-rules test's comments |
| `tests/e2e/{respawn,system_missing,system_nosh,shell}.txt` | An orphan at the error screen; the USB key; no `/bin/sh`; `uname -a` at 0.3.0 |
| `Cargo.toml`, `Cargo.lock`, `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/*.log`, `crates/{shell,relay-abi}/src/…` | Version 0.3.0 |
| `README.md`, `docs/hardware-test.md`, `rootfs/root/README` | Milestone 2 done; NUC check 4 with the error screen by hand after a reboot; the stick's README |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 7 (with the parts of its body it corrects: the status line, §8.5's `t-sys` and the children's kinds, §11.2's error screen, §12.3's `system_nosh`, §12.4's check 4, §14's tick poll), the roadmap's row for plan 5 and its notes "What plan 5 leaves for milestone 3" and "Deferred findings of milestone 1", and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 5, and the roadmap's notes for it`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p5/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p5/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-5-hardening.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-30-m2-plan-5-hardening.md
git commit -m "docs: plan 5 of milestone 2, hardening and 0.3.0"
cargo xtask lint
git push -u origin m2p5/plan
gh pr create --base main --head m2p5/plan --title "Milestone 2, plan 5: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 5 ("hardening and 0.3.0"), the spec's §16 item 7 with its decisions (among them: the error screen ends every other process first and counts rows as the terminal moves; what nothing uses goes; the TLB under `arch/`; bounded guest loops; milestone 1's last findings; the e2e steps `system-drop` and `reset-key`; the tick poll's cost measured on the NUC; version 0.3.0; milestone 2's definition of done item by item), the corrections to the spec's body they bring, and the roadmap's row and notes for plan 5 and milestone 3.

## How it was tested

- [x] Every task of plan 5 was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p5/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p5/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-plan` and continue with PR 2.

---

## PR 2: The kernel's fixes, and the guest tests' bounds (Tasks 1–10)

What plan 4b's final review and plans 3b and 4a left for plan 5 in the kernel, milestone 1's findings in the kernel and the USB stack, and the guest tests' loops (roadmap, "What plan 4b leaves for plan 5"): the error screen ends every other process and counts rows as the terminal moves; what nothing uses goes; `read_dir`'s heap; the TLB under `arch/`; `parse_fadt`; a bouncing USB port tried once; bounded guest loops.

Branch `m2p5/kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p5/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p5/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p5/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-kernel m2p5/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p5/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: The error screen ends every other process before it draws

Plan 4b's final review (deferred minor 1) and decision 2: other processes kept running while the error screen waited, and an orphan's `t-spin` line landed under `Press any key to reboot.`; enough output would scroll the heading away. `Table::kill_all_but_init` marks every process but process 1 as killed (as `kill` does: the first reason stays, zombies are left as they are, the blocked are woken to end), and `error_screen::show` calls it through `proc::kill_others` before it takes the log's last lines and draws. Each process ends before it runs another instruction of its program and says so in the kernel log only. `respawn` starts `t-spawn orphan` (a `t-spin 1` left running) before the third `exit` and expects the orphan's `killed: kill` line after the screen. The red runs are the table's test, which cannot find the function, and `respawn`, where the orphan prints its iterations under the screen and the killed line never comes. Mutation checks: the kill doing nothing, the blocked not woken, and init killed too each fail the test.

**Files:**
- Modify: `kernel/src/error_screen.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `tests/e2e/respawn.txt`

**Interfaces:**
- Consumes: plan 3a's `Table::{kill, wake}`; plan 4b's `error_screen::show`.
- Produces: `Table::kill_all_but_init(&mut self, reason: u32)`, `Table::mark` (private); `proc::kill_others()`.

- [ ] **Step 1: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

with:

````rust
    #[test]
    fn every_process_but_1_can_be_killed_at_once() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let shell = add(&mut t, init, true);
        let orphan = add(&mut t, init, true);
        let blocked = add(&mut t, shell, false);
        let zombie = add(&mut t, shell, false);
        t.end(zombie, WaitStatus::exited(0));
        assert_eq!(turns(&mut t, 4), [init, shell, orphan, blocked]);
        t.block(Blocked::Console);
        t.kill_all_but_init(KILLED_KILL);
        for pid in [shell, orphan, blocked] {
            assert_eq!(t.get(pid).unwrap().killed, Some(KILLED_KILL), "{pid}");
        }
        assert_eq!(state(&t, blocked), State::Ready, "woken to end");
        assert_eq!(t.get(init).unwrap().killed, None, "not process 1");
        assert_eq!(t.get(zombie).unwrap().killed, None, "it has ended already");
        assert_eq!(state(&t, zombie), State::Zombie(WaitStatus::exited(0)));
    }

    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/respawn.txt`**

In `tests/e2e/respawn.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# key typed after it came (not the LF of a terminal's CR LF before it),
# and a key restarts the machine.
timeout 30
````

with:

````text
# key typed after it came (not the LF of a terminal's CR LF before it),
# and a key restarts the machine. Every other process ends first.
timeout 30
````

Replace:

````text
expect \ninit: /bin/sh \(pid \d+\) exited with 3; starting it again\nroot@relay:~# $
send-crlf exit
expect \ninit: /bin/sh \(pid \d+\) exited with 0\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh ended 3 times within 10 s\n\n--- last kernel log lines ---\n
expect \nPress any key to reboot\.\n
alive 2
````

with:

````text
expect \ninit: /bin/sh \(pid \d+\) exited with 3; starting it again\nroot@relay:~# $
# An orphan still running when the screen comes (t-spin 1) is killed
# before it can print over it.
send t-spawn orphan
send-crlf exit
expect \ninit: /bin/sh \(pid \d+\) exited with 0\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh ended 3 times within 10 s\n\n--- last kernel log lines ---\n
expect \nPress any key to reboot\.\n
expect pid \d+ \(\S*t-spin\): killed: kill\n
alive 2
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `kill_all_but_init` found for struct `table::Table<R>` in the current scope ``.

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: FAIL: scenario `respawn` stops at line 24, timed out waiting for `pid \d+ \(\S*t-spin\): killed: kill\n`.

- [ ] **Step 4: Change `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, replace:

````rust

/// Shows the screen for `reason`, waits for a key, and restarts the
/// machine. Only process 1 calls it.
pub fn show(reason: &Reason) -> ! {
    let mut tail = [0u8; 4096];
````

with:

````rust

/// Ends every other process, shows the screen for `reason`, waits for a
/// key, and restarts the machine. Only process 1 calls it.
pub fn show(reason: &Reason) -> ! {
    // Nothing else writes to the console while the screen is up: an
    // orphan's output would land under it and could scroll it away.
    proc::kill_others();
    let mut tail = [0u8; 4096];
````

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
    PROCS.lock().wake_all(Blocked::Console);
}
````

with:

````rust
    PROCS.lock().wake_all(Blocked::Console);
}

/// Kills every process but process 1 (the error screen's, so that none
/// of them writes over it): each ends before it runs another instruction
/// of its program, as after `kill`.
pub fn kill_others() {
    PROCS.lock().kill_all_but_init(relay_abi::wait::KILLED_KILL);
}
````

- [ ] **Step 6: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        }
        let mut woken = [0u32; MAX];
````

with:

````rust
        }
        self.mark(hit, reason);
        Ok(())
    }

    /// Kills every process but process 1 for `reason`, as `kill` does
    /// (the error screen's, so that nothing else writes over it).
    pub fn kill_all_but_init(&mut self, reason: u32) {
        self.mark(|p| p.pid != INIT, reason);
    }

    /// Marks the processes `hit` names, but the ones that have ended, as
    /// killed for `reason` (the first reason stays), and wakes the
    /// blocked ones so that they end.
    fn mark(&mut self, hit: impl Fn(&Process<R>) -> bool, reason: u32) {
        let mut woken = [0u32; MAX];
````

Replace:

````rust
        }
        Ok(())
    }
````

with:

````rust
        }
    }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 350 tests.

- [ ] **Step 8: Run the `respawn` scenario**

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add kernel tests
git commit -m "Error screen: every other process is killed before it draws, so nothing writes over it"
````


### Task 2: The error screen counts a line's rows as the terminal moves, and shows an escape instead of obeying it

Plan 4b's final review (deferred minor 3) and decision 3: `rows_of` counted characters, while the terminal moves a tab to the next multiple of 8 (never past the last column), a carriage return to the first column and a backspace one back, so a kernel-log line holding a program's path full of tabs took more rows than counted and could scroll the heading away. `rows_of` now runs `term`'s own `ansi::Parser` and moves a cursor by `Terminal`'s rules. A program's bytes can also leave an escape the colour strip does not remove in the log: `ESC c` resets the terminal and wipes the screen, so `text` shows every `ESC` left as `?`. The tests write each awkward kind of line into `term::Terminal` itself at the NUC's 1920×1080 area (120×33 cells): `rows_of` must give the row the terminal's cursor reaches, and the heading must stay on the first row with the footer on screen. The red run: tabs and `ESC c` scroll or wipe the heading, and the counts differ. Mutation checks: a tab not capped, a tab as one column, a carriage return or a backspace as a character, a wrap one column late, and an `ESC` kept each fail a test (a test of the heading alone let the first four survive, since counting too many rows is safe; hence the exact test).

**Files:**
- Modify: `kernel/src/error_screen.rs`

**Interfaces:**
- Consumes: plan 4b's `error_screen::{text, rows_of, fitting}`; `term::{Terminal, ansi::Parser, geometry}`.
- Produces: nothing new (`rows_of` keeps its signature).

- [ ] **Step 1: Add the failing tests to `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, replace:

````rust
    }

    #[test]
    fn the_screen_never_scrolls_its_heading_away() {
````

with:

````rust
    }

    /// A console of the NUC's size, 120 by 33, after `text` was written to
    /// the real terminal: its rows, blanks at the end cut, and the
    /// cursor's row.
    fn on_the_nuc(text: &[u8]) -> (Vec<String>, usize) {
        use term::{Cell, PixelFormat, Terminal};
        let (w, h) = (1920, 1080);
        assert_eq!(term::geometry(w, h), (120, 33, 2));
        let mut cells = alloc::vec![Cell::BLANK; 120 * 33];
        let mut shadow = alloc::vec![0u32; w * h];
        let mut t = Terminal::new(w, h, PixelFormat::Bgr, &mut cells, &mut shadow);
        t.write_bytes(text);
        let rows = (0..t.rows())
            .map(|r| {
                let row: String = (0..t.cols()).map(|c| t.cell(c, r).ch as char).collect();
                row.trim_end().to_string()
            })
            .collect();
        (rows, t.cursor().1)
    }

    /// Lines a program's path can put in the kernel log, each one where a
    /// rule of the terminal's makes a difference: tabs, which move to the
    /// next multiple of 8 but never past the last column; a carriage
    /// return; backspaces after the last column; exactly one character
    /// too many; bytes that are no UTF-8; a terminal reset (`ESC c`) the
    /// colour strip leaves in.
    fn awkward_lines() -> Vec<Vec<u8>> {
        let mut bad_utf8 = b"\xC3".to_vec();
        bad_utf8.extend("\u{e9}".repeat(119).bytes());
        [
            "x\t".repeat(20),
            "\t".repeat(16) + "y",
            "a".repeat(100) + "\r" + &"b".repeat(100),
            "b".repeat(120) + "\x08\x08cc",
            "c".repeat(121),
            "\u{e9}".repeat(121),
            "pid 7 (/root/\x1bc): killed: kill".to_string(),
        ]
        .into_iter()
        .map(String::into_bytes)
        .chain([bad_utf8])
        .map(|mut l| {
            l.push(b'\n');
            l
        })
        .collect()
    }

    #[test]
    fn a_line_takes_the_rows_the_terminal_gives_it() {
        for line in awkward_lines() {
            let mut shown = line.clone();
            let n = klog::strip_ansi_in_place(&mut shown);
            shown.truncate(n);
            shown
                .iter_mut()
                .filter(|b| **b == 0x1B)
                .for_each(|b| *b = b'?');
            let (_, row) = on_the_nuc(&shown);
            assert_eq!(
                rows_of(&shown, 120),
                row,
                "{:?}",
                String::from_utf8_lossy(&line)
            );
        }
    }

    #[test]
    fn the_heading_stays_on_the_nuc_s_terminal_whatever_the_log_holds() {
        for line in awkward_lines() {
            let (screen, _) = on_the_nuc(&text(&Reason::Ended, &line.repeat(20), NUC));
            assert_eq!(
                screen[0], "*** Relay OS cannot run its shell ***",
                "{screen:?}"
            );
            assert!(
                screen.iter().any(|r| r == "Press any key to reboot."),
                "{screen:?}"
            );
            assert!(!screen.iter().any(|r| r.contains('\x1b')));
        }
    }

    #[test]
    fn the_screen_never_scrolls_its_heading_away() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 2 tests fail: `error_screen::tests::a_line_takes_the_rows_the_terminal_gives_it`, `error_screen::tests::the_heading_stays_on_the_nuc_s_terminal_whatever_the_log_holds`.

- [ ] **Step 3: Change `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use vfs::{Errno, Vfs};
````

with:

````rust
use core::fmt;
use term::ansi::Action;
use vfs::{Errno, Vfs};
````

Replace:

````rust
    tail.truncate(n);
    if !tail.is_empty() && !tail.ends_with(b"\n") {
````

with:

````rust
    tail.truncate(n);
    // Any other escape a program's bytes put in the log (`ESC c` resets
    // the terminal) is shown, not obeyed.
    for b in tail.iter_mut().filter(|b| **b == 0x1B) {
        *b = b'?';
    }
    if !tail.is_empty() && !tail.ends_with(b"\n") {
````

Replace:

````rust

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
````

with:

````rust

/// The rows `text` takes on a console `cols` wide, as the terminal moves
/// its cursor (`term`'s parser, and `Terminal`'s rules): each line at
/// least one, and one more each time a character comes after the last
/// column. A tab moves to the next multiple of 8 but never past the last
/// column, a carriage return to the first, a backspace one back.
fn rows_of(text: &[u8], cols: usize) -> usize {
    let cols = cols.max(1);
    let mut parser = term::ansi::Parser::new();
    let (mut rows, mut cx, mut open) = (0, 0, false);
    for &b in text {
        parser.advance(b, &mut |action| match action {
            Action::Print(_) => {
                if cx >= cols {
                    rows += 1;
                    cx = 0;
                }
                cx += 1;
                open = true;
            }
            Action::Control(b'\n') => {
                rows += 1;
                cx = 0;
                open = false;
            }
            Action::Control(b'\r') => {
                cx = 0;
                open = true;
            }
            Action::Control(0x08) => {
                cx = cx.min(cols - 1).saturating_sub(1);
                open = true;
            }
            Action::Control(b'\t') => {
                if cx < cols {
                    cx = ((cx / 8 + 1) * 8).min(cols - 1);
                }
                open = true;
            }
            Action::Control(_) | Action::Csi { .. } | Action::Reset => {}
        });
    }
    rows + usize::from(open)
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 352 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "Error screen: a line takes the rows the terminal gives it (tabs, carriage returns, backspaces), and an escape in the log is shown, not obeyed"
````


### Task 3: The error screen's clear starts with CAN, so a lone ESC a program left cannot swallow its colours' reset

The prototype's review (minor, M3), decision 3: the screen began with `ESC [0m ESC [2J ESC [H`, as if the terminal's parser were at rest; a program whose last output set black on black and ended with a lone `ESC` left it in its escape state, which swallowed the screen's first `ESC`, so `[0m` printed as text, the colours stayed, and the whole screen was drawn black on black (unlikely: any echo or `init:` line in between clears the state, but a shell that ends without output leaves it). `CLEAR` starts with CAN (0x18), which ends the escape state and prints nothing from rest; half a CSI or half a UTF-8 character needs nothing (an `ESC` starts a CSI over; a cut UTF-8 sequence prints `?` and ends), and the test keeps them as cases. The test writes each program's leftovers, `CLEAR` and the screen into `term::Terminal` at the NUC's size and checks that the heading is there in colours that differ. The red run is a compile error (`CLEAR` is new); with the old bytes as `CLEAR`, the heading is black on black. Mutation check: CAN removed fails the test; a backslash instead of CAN survives, since any byte but `[` and `c` ends the escape state. The red panic screen has the same weakness, and is a kernel bug's screen: left as it is.

**Files:**
- Modify: `kernel/src/error_screen.rs`

**Interfaces:**
- Consumes: Task 2's test helper `on_the_nuc`.
- Produces: `error_screen::CLEAR` (private); the test helper `nuc_terminal(text, f)`.

- [ ] **Step 1: Add the failing tests to `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn on_the_nuc(text: &[u8]) -> (Vec<String>, usize) {
        use term::{Cell, PixelFormat, Terminal};
````

with:

````rust
    fn on_the_nuc(text: &[u8]) -> (Vec<String>, usize) {
        nuc_terminal(text, |t| {
            let rows = (0..t.rows())
                .map(|r| {
                    let row: String = (0..t.cols()).map(|c| t.cell(c, r).ch as char).collect();
                    row.trim_end().to_string()
                })
                .collect();
            (rows, t.cursor().1)
        })
    }

    /// `f` of the real terminal at the NUC's size, after `text`.
    fn nuc_terminal<R>(text: &[u8], f: impl FnOnce(&term::Terminal) -> R) -> R {
        use term::{Cell, PixelFormat, Terminal};
````

Replace:

````rust
        t.write_bytes(text);
        let rows = (0..t.rows())
            .map(|r| {
                let row: String = (0..t.cols()).map(|c| t.cell(c, r).ch as char).collect();
                row.trim_end().to_string()
            })
            .collect();
        (rows, t.cursor().1)
    }
````

with:

````rust
        t.write_bytes(text);
        f(&t)
    }

    #[test]
    fn the_screen_has_its_colours_whatever_a_program_left_the_terminal_in() {
        // A program's last bytes before the screen: black on black, then
        // a sequence it never finished (a lone ESC, half a CSI, half a
        // UTF-8 character), which would swallow the screen's own reset.
        for left in [
            &b"hello \x1b[30;40mdark\x1b"[..],
            b"\x1b[30;40m\x1b[1;",
            b"\x1b[30;40m\xC3",
            b"\x1b[30;40m",
        ] {
            let mut screen = left.to_vec();
            screen.extend_from_slice(CLEAR);
            screen.extend_from_slice(&text(&Reason::Ended, b"x\n", NUC));
            nuc_terminal(&screen, |t| {
                let (first, rows) = (t.cell(0, 0), t.rows());
                let row: String = (0..t.cols()).map(|c| t.cell(c, 0).ch as char).collect();
                assert_eq!(
                    row.trim_end(),
                    "*** Relay OS cannot run its shell ***",
                    "{left:?}"
                );
                assert_ne!(first.fg, first.bg, "{left:?}: the heading can be seen");
                assert!(rows == 33);
            });
        }
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `CLEAR` in this scope ``.

- [ ] **Step 3: Change `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
const FOOTER: &[u8] = b"\nPress any key to reboot.\n";

````

with:

````rust
const FOOTER: &[u8] = b"\nPress any key to reboot.\n";

/// What clears the console for the screen, whatever a program's output
/// left the terminal in: CAN first ends an escape a program left
/// unfinished (a lone ESC would swallow the next one, and the colours it
/// resets would stay), and prints nothing otherwise; then the colours,
/// the clear and the cursor's home.
const CLEAR: &[u8] = b"\x18\x1b[0m\x1b[2J\x1b[H";

````

Replace:

````rust
    while tty::pop().is_some() {}
    console::write_bytes(b"\x1b[0m\x1b[2J\x1b[H");
    let size = console::size().unwrap_or((80, 25));
````

with:

````rust
    while tty::pop().is_some() {}
    console::write_bytes(CLEAR);
    let size = console::size().unwrap_or((80, 25));
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 353 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "Error screen: its clear starts with CAN, so a lone ESC a program left cannot swallow the colours' reset (review M3)"
````


### Task 4: What only the in-kernel shell used, or nothing uses, goes

Plan 4b's final review (deferred minor 2) and decision 4: `exec::arg_bytes` and `InputQueue::take_interrupt` had only test callers since the in-kernel shell went, and so did `tty::is_line_mode` (and `InputQueue::is_line_mode` behind it). A scan for `pub` functions of the kernel with no caller outside tests found two more: `OpenFile::is_dir` (since plan 3b) and `arch::idle_forever` (since milestone 1's shell ended the boot); the prototype's review (minor, M5) found a third, `PageTables::translate`, which the tests of what maps and unmaps use, and which becomes `#[cfg(test)]` with `Cache::of_entry` behind it. `arg_bytes` becomes a helper of `exec`'s tests (the kernel's own checks of arguments are `spawn`'s and `load`'s, which keep their tests); the input queue's tests check the queue's bytes instead of `take_interrupt` (stricter: the Ctrl-C and what follows it, nothing before); `file.rs`'s test reads a directory for `EISDIR` instead of asking `is_dir`. The task removes code and ports tests of correct code, so it has no failing run. Mutation check: a raw Ctrl-C that does not drop what came before it fails the ported tests.

**Files:**
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/exec.rs`
- Modify: `kernel/src/file.rs`
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/mm/paging.rs`
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: no `exec::arg_bytes`, `InputQueue::{take_interrupt, is_line_mode}`, `tty::is_line_mode`, `OpenFile::is_dir`, `arch::idle_forever`; `PageTables::translate` and `Cache::of_entry` for tests only.

- [ ] **Step 1: Add the failing tests to `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
    use crate::mm::testing::FakeMem;
    use elf::Segment;

````

with:

````rust
    use crate::mm::testing::FakeMem;
    use alloc::vec::Vec;
    use elf::Segment;

    /// The argument bytes for `args`: each argument, then a NUL.
    fn arg_bytes(args: &[&[u8]]) -> Vec<u8> {
        args.iter()
            .flat_map(|a| a.iter().copied().chain([0]))
            .collect()
    }

````

Replace:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t"]).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
````

with:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t"]);
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
````

Replace:

````rust
        let (file, p) = program();
        load(&mut s, &mut m, &file, &p, &arg_bytes(&[b"x"]).unwrap(), 1).unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
````

with:

````rust
        let (file, p) = program();
        load(&mut s, &mut m, &file, &p, &arg_bytes(&[b"x"]), 1).unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
````

Replace:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-args", b"a", b"b c", b""]).unwrap();
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
````

with:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-args", b"a", b"b c", b""]);
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
````

Replace:

````rust
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]).unwrap();
        assert_eq!(args.len(), ARGS_MAX);
        assert_eq!(arg_bytes(&[b"pp", &long]), Err(Errno::E2BIG));
        assert_eq!(arg_bytes(&[b"p", b"a\0b"]), Err(Errno::EINVAL));
        // The most still fits, and lands at the top.
````

with:

````rust
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]);
        assert_eq!(args.len(), ARGS_MAX);
        // The most still fits, and lands at the top.
````

Replace:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]).unwrap();
        assert_eq!(
````

with:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]);
        assert_eq!(
````

Replace:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]).unwrap();
        let mut k = PageTables::new(&mut m).unwrap();
````

with:

````rust
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]);
        let mut k = PageTables::new(&mut m).unwrap();
````

- [ ] **Step 2: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
        let d = open(&mut t, b"/root", OPEN_READ | OPEN_DIRECTORY).unwrap();
        assert!(d.is_dir() && d.is_readable() && !d.is_writable());
        assert!(open(&mut t, b"/root", OPEN_READ).unwrap().is_dir());
        assert!(open(&mut t, b"/bin/prog", OPEN_READ).is_ok());
        let f = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert!(!f.is_readable() && f.is_writable() && !f.is_dir());
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
````

with:

````rust
        let d = open(&mut t, b"/root", OPEN_READ | OPEN_DIRECTORY).unwrap();
        assert!(d.is_readable() && !d.is_writable());
        let mut buf = [0u8; 8];
        assert_eq!(d.read(&mut t, &mut buf), Err(Errno::EISDIR), "a directory");
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(
            d.read(&mut t, &mut buf),
            Err(Errno::EISDIR),
            "without DIRECTORY too"
        );
        assert!(open(&mut t, b"/bin/prog", OPEN_READ).is_ok());
        let f = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert!(!f.is_readable() && f.is_writable());
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
````

- [ ] **Step 3: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
        q.push_key(&press(Key::Char(b'c'), true));
        assert!(q.take_interrupt());
        assert!(q.is_empty());
        q.push(&[b'a'; QUEUE_MAX]);
        q.push(b"x\x03y");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"y");
    }
````

with:

````rust
        q.push_key(&press(Key::Char(b'c'), true));
        assert_eq!(drain(&mut q), [INTERRUPT], "only the Ctrl-C is left");
        q.push(&[b'a'; QUEUE_MAX]);
        q.push(b"x\x03y");
        assert_eq!(drain(&mut q), b"\x03y");
    }
````

Replace:

````rust
        serial(&mut q, b"ls\x1b[\x03pwd");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd");
        // A sequence that never ends is not kept for ever: its start is
````

with:

````rust
        serial(&mut q, b"ls\x1b[\x03pwd");
        assert_eq!(drain(&mut q), b"\x03pwd");
        // A sequence that never ends is not kept for ever: its start is
````

Replace:

````rust
        serial(&mut q, b"A");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"A");
    }
````

with:

````rust
        serial(&mut q, b"A");
        assert_eq!(drain(&mut q), b"\x03A");
    }
````

Replace:

````rust
        let mut q = line_mode();
        assert!(q.is_line_mode());
        q.push_key(&press(Key::Char(b'h'), false));
````

with:

````rust
        let mut q = line_mode();
        q.push_key(&press(Key::Char(b'h'), false));
````

Replace:

````rust
        assert_eq!(read(&mut q, 10).unwrap(), b"A\n", "and the half sequence");
        assert!(!q.take_interrupt(), "not a raw Ctrl-C");
    }
````

with:

````rust
        assert_eq!(read(&mut q, 10).unwrap(), b"A\n", "and the half sequence");
        assert_eq!(drain(&mut q), b"", "not a raw Ctrl-C");
    }
````

Replace:

````rust
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        q.push(b"e");
````

with:

````rust
        q.set_line_mode(false);
        assert_eq!(q.pop(), Some(INTERRUPT));
        q.push(b"e");
````

Replace:

````rust
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"de");
    }
````

with:

````rust
        q.set_line_mode(false);
        assert_eq!(drain(&mut q), b"\x03de");
    }
````

Replace:

````rust
        q.push(b"rm x\x03 ls\x03pwd\r");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd\r");
        q.push(b"echo");
        assert!(!q.take_interrupt());
        assert_eq!(drain(&mut q), b"echo");
````

with:

````rust
        q.push(b"rm x\x03 ls\x03pwd\r");
        assert_eq!(drain(&mut q), b"\x03pwd\r");
        q.push(b"echo");
        assert_eq!(drain(&mut q), b"echo");
````

- [ ] **Step 4: Declare the new module in `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, replace:

````rust

    fn of_entry(e: u64) -> Cache {
````

with:

````rust

    #[cfg(test)]
    fn of_entry(e: u64) -> Cache {
````

- [ ] **Step 5: Change `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust

/// Waits for interrupts forever: the CPU sleeps between timer ticks.
pub fn idle_forever() -> ! {
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

/// `e_machine` of the programs this kernel runs.
````

with:

````rust

/// `e_machine` of the programs this kernel runs.
````

- [ ] **Step 6: Change `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::mm::space::AddressSpace;
use alloc::vec::Vec;
use elf::{Access, Program};
````

with:

````rust
use crate::mm::space::AddressSpace;
use elf::{Access, Program};
````

Replace:

````rust

/// The argument bytes for `args` (argument 0 first): each argument, then a
/// NUL. An argument holding a NUL is `EINVAL`; more than `ARGS_MAX` bytes
/// is `E2BIG`.
pub fn arg_bytes(args: &[&[u8]]) -> Result<Vec<u8>, Errno> {
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    if len > ARGS_MAX {
        return Err(Errno::E2BIG);
    }
    let mut bytes = Vec::with_capacity(len);
    for a in args {
        if a.contains(&0) {
            return Err(Errno::EINVAL);
        }
        bytes.extend_from_slice(a);
        bytes.push(0);
    }
    Ok(bytes)
}

fn perm(access: Access) -> Perm {
````

with:

````rust

fn perm(access: Access) -> Perm {
````

Replace:

````rust
/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` (from `arg_bytes`, `argc` of them) at the top
/// of the stack. On an error the caller destroys `space`, which gives
````

with:

````rust
/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` (`argc` of them, each ending in its NUL) at the top
/// of the stack. On an error the caller destroys `space`, which gives
````

- [ ] **Step 7: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
        self.write
    }

    pub fn is_dir(&self) -> bool {
        self.dir
    }
````

with:

````rust
        self.write
    }
````

- [ ] **Step 8: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    pub fn is_line_mode(&self) -> bool {
        self.line_mode
    }

    /// Whether a Ctrl-C was typed in line mode since the last call.
````

with:

````rust

    /// Whether a Ctrl-C was typed in line mode since the last call.
````

Replace:

````rust
    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would, and a
    /// half-arrived serial sequence, wherever the Ctrl-C came from.
    pub fn push(&mut self, bytes: &[u8]) {
````

with:

````rust
    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as a terminal flushes its input
    /// on an interrupt, and a half-arrived serial sequence, wherever the
    /// Ctrl-C came from.
    pub fn push(&mut self, bytes: &[u8]) {
````

Replace:

````rust
        self.bytes.is_empty() && !self.line.has_line()
    }

    /// Whether a Ctrl-C is waiting. If one is, it and everything typed
    /// before it are dropped, as a terminal flushes its input on an
    /// interrupt; what was typed after it stays.
    pub fn take_interrupt(&mut self) -> bool {
        match self.bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.drain(..=i);
                self.echoed = self.echoed.saturating_sub(i + 1);
                true
            }
            None => false,
        }
    }
````

with:

````rust
        self.bytes.is_empty() && !self.line.has_line()
    }
````

- [ ] **Step 9: Change `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, replace:

````rust

    /// The physical address, cache type and page size `virt` maps to.
    pub fn translate(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Cache, u64)> {
````

with:

````rust

    /// The physical address, cache type and page size `virt` maps to (for
    /// the tests of what maps and unmaps).
    #[cfg(test)]
    pub fn translate(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Cache, u64)> {
````

- [ ] **Step 10: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
    was
}

pub fn is_line_mode() -> bool {
    INPUT.lock().is_line_mode()
}
````

with:

````rust
    was
}
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 353 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add kernel
git commit -m "kernel: what only the in-kernel shell or nothing used goes (arg_bytes, take_interrupt, is_line_mode, OpenFile::is_dir, idle_forever), and PageTables::translate is the tests'"
````


### Task 5: `read_dir` leaves its own buffer out of the heap room it allows the directory

Plan 3b's deferred minor, and decision 5: `read_dir` measured `heap_room` and then allocated its buffer (up to 64 KiB) from the same heap, so a directory whose list fit only beside a buffer that was no longer there could still exhaust the heap, which panics. The room it passes on is now the heap's room less the buffer. The test takes a directory of the fake with a size (asserted, or the check is vacuous) and a heap with room for its list plus the buffer less one byte: `ENOMEM`; one byte more: the records. The red run: the call succeeds where it must say `ENOMEM`. Mutation check: half the buffer subtracted fails the test.

**Files:**
- Modify: `kernel/src/syscall/files.rs`

**Interfaces:**
- Consumes: plan 3b's `syscall::files::read_dir`, `Caller::heap_room`, `OpenFile::read_dir`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    #[test]
    fn the_calls_on_a_path_do_what_their_vfs_operation_does() {
````

with:

````rust
    #[test]
    fn read_dir_s_own_buffer_is_not_room_for_the_directory() {
        // The directory's list takes up to ten times its size while it is
        // made (`file::DIR_MEMORY_FACTOR`), after the call has taken its
        // own buffer from the same heap.
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Stat, b"/root", [0, W]), Ok(0));
        let size = u64_at(&get(&mut f, W, 72), 8) as usize;
        assert!(size > 0, "a directory with a size, or the check is vacuous");
        let d = open(&mut f, b"/root", OPEN_READ).unwrap();
        let len = PAGE as usize;
        f.heap_room = 10 * size + len - 1;
        assert_eq!(
            call(&mut f, Call::ReadDir, [d, W, len as u64]),
            Err(errno::ENOMEM),
            "room for the list, but not beside the buffer"
        );
        f.heap_room = 10 * size + len;
        assert!(call(&mut f, Call::ReadDir, [d, W, len as u64]).unwrap() > 0);
    }

    #[test]
    fn the_calls_on_a_path_do_what_their_vfs_operation_does() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `syscall::files::tests::read_dir_s_own_buffer_is_not_room_for_the_directory`.

- [ ] **Step 3: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    caller.writable(&slice)?;
    let room = caller.heap_room();
    let mut buf = alloc::vec![0u8; len as usize];
````

with:

````rust
    caller.writable(&slice)?;
    // The buffer comes from the heap the directory's list needs too.
    let room = caller.heap_room().saturating_sub(len as usize);
    let mut buf = alloc::vec![0u8; len as usize];
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 354 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "read_dir: the heap room it allows the directory leaves out its own buffer"
````


### Task 6: The TLB is flushed through `arch::tlb`

Plan 3b's deferred minor, and decision 6: `mm::flush_pages` (and `free_kernel_stack` and `map_mmio`) used `x86_64::instructions::tlb` outside `kernel/src/arch/`, where spec §3.1 keeps the page-table format and so its cache. `arch::tlb::{flush, flush_all}` wrap them, and `mm` calls those. A host test walks the kernel's sources and fails if any file outside `arch/` names `instructions::tlb`; it first checks that the walk found `mm/mod.rs`, so it cannot pass by reading nothing. The red run is that test, naming `mm/mod.rs`. `memory` runs `t-mem`, whose `mem_unmap` flushes. Mutation checks: the walk skipping `mm/` instead of `arch/`, and a walk that finds no `.rs` file, each fail the test.

**Files:**
- Modify: `kernel/src/arch/mod.rs`
- Create: `kernel/src/arch/tlb.rs`
- Modify: `kernel/src/mm/mod.rs`

**Interfaces:**
- Consumes: plan 3b's `mm::flush_pages`.
- Produces: `arch::tlb::flush(virt: u64)`, `arch::tlb::flush_all()`.

- [ ] **Step 1: Declare the new module in `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust
pub mod pic;
pub mod user;
````

with:

````rust
pub mod pic;
pub mod tlb;
pub mod user;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/arch/tlb.rs`**

Create `kernel/src/arch/tlb.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The kernel's source files outside `arch/`, with their paths.
    fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "arch" {
                    sources(&path, out);
                }
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }

    #[test]
    fn only_arch_flushes_the_tlb() {
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(
            files.iter().any(|(p, _)| p.ends_with("mm/mod.rs")),
            "the walk found the sources"
        );
        for (path, text) in files {
            assert!(
                !text.contains("instructions::tlb"),
                "{path} flushes the TLB itself"
            );
        }
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `arch::tlb::tests::only_arch_flushes_the_tlb`.

- [ ] **Step 4: Implement `kernel/src/arch/tlb.rs`**

Insert this at the top of `kernel/src/arch/tlb.rs`, above `#[cfg(test)]`:

````rust
//! The TLB: dropping the entries of pages the page tables no longer map
//! (user-space gate §3.1: the page-table format and its caches stay under
//! `arch/`).

/// Drops the TLB's entry for the page at `virt`.
pub fn flush(virt: u64) {
    x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
}

/// Drops every entry of the TLB that is not global.
pub fn flush_all() {
    x86_64::instructions::tlb::flush_all();
}

````

- [ ] **Step 5: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use boot_info::{BootInfo, HEAP_BASE, HEAP_SIZE, PHYS_MAP_MAX, PHYS_OFFSET};
````

with:

````rust

use crate::arch;
use boot_info::{BootInfo, HEAP_BASE, HEAP_SIZE, PHYS_MAP_MAX, PHYS_OFFSET};
````

Replace:

````rust
    for virt in pages {
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
    }
````

with:

````rust
    for virt in pages {
        arch::tlb::flush(virt);
    }
````

Replace:

````rust
    if pages > 64 {
        x86_64::instructions::tlb::flush_all();
        return;
    }
    for k in 0..pages {
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt + k * paging::PAGE));
    }
````

with:

````rust
    if pages > 64 {
        arch::tlb::flush_all();
        return;
    }
    for k in 0..pages {
        arch::tlb::flush(virt + k * paging::PAGE);
    }
````

Replace:

````rust
        .map(&mut mem, PHYS_OFFSET + start, start, end - start, cache)?;
    x86_64::instructions::tlb::flush_all();
    Ok((PHYS_OFFSET + phys) as *mut u8)
````

with:

````rust
        .map(&mut mem, PHYS_OFFSET + start, start, end - start, cache)?;
    arch::tlb::flush_all();
    Ok((PHYS_OFFSET + phys) as *mut u8)
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 355 tests.

- [ ] **Step 7: Run the `memory` scenario**

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel
git commit -m "mm: the TLB is flushed through arch::tlb, and a test keeps its instructions under arch/"
````


### Task 7: The TLB test flags any `tlb` path outside `arch::tlb`, however imported, and `invlpg`

The prototype's review (minor, M1), decision 6: Task 6's test looked for the text `instructions::tlb`, so `use x86_64::instructions::{interrupts, tlb}; tlb::flush(…)` in `mm/`, an alias, or `asm!("invlpg …")` passed it (the reviewer's probe did). `flushes_the_tlb` flags every `tlb` that is a whole word and not reached through `arch::`, and the word `invlpg`; a self-test runs it over the forms that must and must not count. The red run is a compile error (the function is new); the old scan as a mutant fails the self-test on the brace import. Mutation checks: the `arch::` exemption dropped (both tests), the word boundary after `tlb` dropped (`tlbs`), and `invlpg` not seen each fail a test, and the reviewer's probe, a brace-imported `tlb::flush` in `mm/mod.rs`, fails `only_arch_flushes_the_tlb`.

**Files:**
- Modify: `kernel/src/arch/tlb.rs`

**Interfaces:**
- Consumes: Task 6's test module (`sources`, `only_arch_flushes_the_tlb`).
- Produces: the test helper `flushes_the_tlb(code: &str) -> bool`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
        for (path, text) in files {
            assert!(
                !text.contains("instructions::tlb"),
                "{path} flushes the TLB itself"
            );
        }
````

with:

````rust
        for (path, text) in files {
            assert!(!flushes_the_tlb(&text), "{path} flushes the TLB itself");
        }
    }

    #[test]
    fn the_scan_sees_every_way_to_name_the_tlb() {
        // The prototype's review: a `tlb` imported in braces passed the
        // first scan, which looked for `instructions::tlb` only.
        for code in [
            "x86_64::instructions::tlb::flush(v);",
            "use x86_64::instructions::{interrupts, tlb};\ntlb::flush(v);",
            "use x86_64::instructions::tlb as t;",
            "unsafe { asm!(\"invlpg [{}]\", in(reg) v) };",
        ] {
            assert!(flushes_the_tlb(code), "{code}");
        }
        for code in [
            "arch::tlb::flush(virt);",
            "arch::tlb::flush_all();",
            "let tlbs = 1;",
        ] {
            assert!(!flushes_the_tlb(code), "{code}");
        }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `flushes_the_tlb` in this scope ``.

- [ ] **Step 3: Change `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
            }
        }
    }

````

with:

````rust
            }
        }
    }

    /// Whether `code` names the TLB other than through `arch::tlb`: a
    /// `tlb` path however it is imported, or the `invlpg` instruction.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        code.contains("invlpg")
            || code.match_indices("tlb").any(|(i, _)| {
                let before = code[..i].chars().next_back();
                let after = code[i + 3..].chars().next();
                !before.is_some_and(word)
                    && !after.is_some_and(word)
                    && !code[..i].ends_with("arch::")
            })
    }

````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 356 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "TLB test: any tlb path outside arch::tlb, however imported, and invlpg count as flushing the TLB (review M1)"
````


### Task 8: `parse_fadt` reads the 32-bit DSDT field once

Milestone 1's deferred minor (roadmap, "Deferred findings of milestone 1"), decision 8: the fallback to the FADT's 32-bit DSDT field checked that bytes 40..44 exist with `get` and then read them again with `u32_at`, which indexes. It now reads the range `get` returned. No behaviour changes, so the task has no failing run; `an_acpi_1_fadt_uses_the_32_bit_fields` and `a_fadt_too_short_for_its_dsdt_field_has_none` cover it. Mutation checks: the field read as 0, and a range one byte too long, each fail a test.

**Files:**
- Modify: `kernel/src/acpi/tables.rs`

**Interfaces:**
- Consumes: milestone 1's `acpi::tables::parse_fadt`.
- Produces: nothing new.

- [ ] **Step 1: Change `kernel/src/acpi/tables.rs`**

In `kernel/src/acpi/tables.rs`, replace:

````rust
        true => u64_at(t, 140),
        false => t.get(40..44).map_or(0, |_| u32_at(t, 40) as u64),
    };
````

with:

````rust
        true => u64_at(t, 140),
        false => t
            .get(40..44)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| u64::from(u32::from_le_bytes(b))),
    };
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 356 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add kernel
git commit -m "acpi: parse_fadt reads the 32-bit DSDT field once"
````


### Task 9: A USB connection that kept bouncing for the debounce's 2 s is not tried again

Milestone 1's deferred minor, decision 8: the debounce gives up after 2 s of bouncing with `Timeout` when the last look showed a connection, and `Host::attach` tries every `Timeout` again, three times in all, so a port that keeps bouncing cost up to 3 × 2 s per attach. The debounce now says `UsbError::Unstable` (`connection not stable`) then, and `attach` does not try it again, as it does not for `Disconnected` or a dead controller. The host test bounces a keyboard for 8 s: one attach, which gives up within 2.1 s of virtual time, one `connection not stable after` line and no `trying again`. The red run is a compile error (the variant is new); the old behaviour as a mutant (`Unstable` tried again) makes the host test take 6.000249 s. Mutation check: the debounce saying `Timeout` again fails both bounce tests.

**Files:**
- Modify: `crates/usb/src/error.rs`
- Modify: `crates/usb/src/host.rs`
- Modify: `crates/usb/src/xhci/device.rs`

**Interfaces:**
- Consumes: milestone 1's `xhci::Xhci::debounce`, `Host::attach`, `ATTACH_TRIES`.
- Produces: `UsbError::Unstable` (`Display`: `connection not stable`).

- [ ] **Step 1: Add the failing tests to `crates/usb/src/error.rs`**

In `crates/usb/src/error.rs`, replace:

````rust
        assert_eq!(UsbError::Timeout.to_string(), "timed out");
        assert_eq!(
````

with:

````rust
        assert_eq!(UsbError::Timeout.to_string(), "timed out");
        assert_eq!(UsbError::Unstable.to_string(), "connection not stable");
        assert_eq!(
````

- [ ] **Step 2: Add the failing tests to `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, replace:

````rust
    #[test]
    fn typing_on_a_keyboard_gives_key_events() {
````

with:

````rust
    #[test]
    fn a_connection_that_keeps_bouncing_is_given_up_once() {
        // In for 20 ms, out for 20 ms, for 8 s: the debounce gives up after
        // 2 s, and trying again would only bounce as long again (milestone
        // 1's deferred finding: 3 × 2 s per attach).
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        let start = hal.clock();
        hal.fake().plug(3, k120.clone());
        for n in 0..200 {
            let again = k120.clone();
            let out = Duration::from_millis(10 + 40 * n);
            hal.fake().after(out, |x, _| x.unplug(3));
            hal.fake()
                .after(out + Duration::from_millis(20), move |x, _| {
                    x.plug(3, again)
                });
        }
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert!(attached[0].outcome.is_err());
        let took = hal.clock() - start;
        assert!(took < Duration::from_millis(2100), "{took:?}");
        let log = hal.log_text();
        assert!(!log.contains("trying again"), "{log}");
        assert_eq!(
            log.matches("connection not stable after").count(),
            1,
            "{log}"
        );
    }

    #[test]
    fn typing_on_a_keyboard_gives_key_events() {
````

- [ ] **Step 3: Add the failing tests to `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        // Every 40 ms out for 20 ms, for 3 s; at 2 s it is in, or out.
        for (phase, outcome) in [(10, UsbError::Timeout), (30, UsbError::Disconnected)] {
            let k120 = FakeUsbDevice::k120();
````

with:

````rust
        // Every 40 ms out for 20 ms, for 3 s; at 2 s it is in, or out.
        for (phase, outcome) in [(10, UsbError::Unstable), (30, UsbError::Disconnected)] {
            let k120 = FakeUsbDevice::k120();
````

Replace:

````rust
            match outcome {
                UsbError::Timeout => {
                    assert!(log.contains("port 1: connection not stable after 2000 ms"))
````

with:

````rust
            match outcome {
                UsbError::Unstable => {
                    assert!(log.contains("port 1: connection not stable after 2000 ms"))
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `Unstable` found for enum `error::UsbError` in the current scope ``.

- [ ] **Step 5: Change `crates/usb/src/error.rs`**

In `crates/usb/src/error.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    Disconnected,
    /// The controller stopped working (a command timed out or it reported a
````

with:

````rust
    Disconnected,
    /// The connection kept bouncing for as long as the debounce waits:
    /// trying again would only wait as long again.
    Unstable,
    /// The controller stopped working (a command timed out or it reported a
````

Replace:

````rust
            UsbError::Disconnected => write!(f, "device disconnected"),
            UsbError::ControllerDead => write!(f, "controller stopped working"),
````

with:

````rust
            UsbError::Disconnected => write!(f, "device disconnected"),
            UsbError::Unstable => write!(f, "connection not stable"),
            UsbError::ControllerDead => write!(f, "controller stopped working"),
````

- [ ] **Step 6: Change `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Sets up the device on `port`, [`ATTACH_TRIES`] times at most (each
    /// try resets the port afresh), unless it is gone or the controller
    /// died. Only the final outcome is reported.
    fn attach(&mut self, port: u8) -> Attached {
````

with:

````rust
    /// Sets up the device on `port`, [`ATTACH_TRIES`] times at most (each
    /// try resets the port afresh), unless it is gone, its connection kept
    /// bouncing, or the controller died. Only the final outcome is
    /// reported.
    fn attach(&mut self, port: u8) -> Attached {
````

Replace:

````rust
                    if tries < ATTACH_TRIES
                        && !matches!(e, UsbError::Disconnected | UsbError::ControllerDead) =>
                {
````

with:

````rust
                    if tries < ATTACH_TRIES
                        && !matches!(
                            e,
                            UsbError::Disconnected | UsbError::Unstable | UsbError::ControllerDead
                        ) =>
                {
````

- [ ] **Step 7: Change `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// the last look showed no connection (the next connect is a change
    /// again), `Timeout` if it did (the host tries again).
    fn debounce(&self, port: u8) -> Result<(), UsbError> {
````

with:

````rust
    /// the last look showed no connection (the next connect is a change
    /// again), `Unstable` if it did (which the host does not try again: it
    /// would bounce as long again).
    fn debounce(&self, port: u8) -> Result<(), UsbError> {
````

Replace:

````rust
                );
                Err(UsbError::Timeout)
            }
````

with:

````rust
                );
                Err(UsbError::Unstable)
            }
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 337 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -m "usb: a connection that kept bouncing for the debounce's 2 s is not tried again (UsbError::Unstable)"
````


### Task 10: Every loop of a guest test that waits for the kernel to say stop is bounded

Plan 3b's deferred minor, and decision 7: `t-files`' loop opening fds until `EMFILE` and its `read_dir` loop until 0, and `t-tee`'s read loop until end of file and its push loop until `EBUSY`, never ended if the kernel never said stop; QEMU's fixed memory bounded them, and a mutant failed messily. They now stop at 64 opens, 64 calls, 256 reads of 256 bytes (64 KiB; the logs are a few KiB) and 16 pushes, and say `and no end` where the kernel gave no limit. Their output is unchanged otherwise, so the task has no failing run; `files` and `tees` pass as before. Mutation checks, each a kernel that never says stop, which the bound turns into a scenario that fails at once: no fd limit (`fds up to 31, and no end`), a `read_dir` that never ends (64 calls of `./`), no tee limit (`pushed 16, and no end`), a file read that never ends (`65536 bytes`).

**Files:**
- Modify: `userland/tests/src/bin/t-files.rs`
- Modify: `userland/tests/src/bin/t-tee.rs`

**Interfaces:**
- Consumes: plan 3b's `t-files` and `t-tee`.
- Produces: nothing new.

- [ ] **Step 1: Change the test program `userland/tests/src/bin/t-files.rs`**

In `userland/tests/src/bin/t-files.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    // 32 fds at most.
    let mut last = 0;
    let full = loop {
        match sys::open(TMP, rw) {
            Ok(fd) => last = fd,
            Err(e) => break e,
        }
    };
    let _ = writeln!(Fd(1), "fds up to {last}, then {}", name(full));
    for fd in 3..=last {
````

with:

````rust

    // 32 fds at most; a kernel without the limit is stopped at 64.
    let mut last = 0;
    let mut full = None;
    for _ in 0..64 {
        match sys::open(TMP, rw) {
            Ok(fd) => last = fd,
            Err(e) => {
                full = Some(e);
                break;
            }
        }
    }
    let _ = match full {
        Some(e) => writeln!(Fd(1), "fds up to {last}, then {}", name(e)),
        None => writeln!(Fd(1), "fds up to {last}, and no end"),
    };
    for fd in 3..=last {
````

Replace:

````rust
    let _ = write!(Fd(1), "entries:");
    loop {
        let n = sys::read_dir(d, &mut buf)?;
````

with:

````rust
    let _ = write!(Fd(1), "entries:");
    // Five entries: a read_dir that never ends is stopped at 64 calls.
    while calls < 64 {
        let n = sys::read_dir(d, &mut buf)?;
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-tee.rs`**

In `userland/tests/src/bin/t-tee.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    let (mut len, mut total) = (0, 0);
    loop {
        let n = sys::read(fd, &mut buf)?;
````

with:

````rust
    let (mut len, mut total) = (0, 0);
    // The logs are a few KiB: a file that never ends is cut at 64 KiB.
    for _ in 0..256 {
        let n = sys::read(fd, &mut buf)?;
````

Replace:

````rust
    show_ok("push an fd not open", sys::console_tee_push(30));
    let mut pushed = 0;
    let full = loop {
        match sys::console_tee_push(log) {
            Ok(()) => pushed += 1,
            Err(e) => break e,
        }
    };
    let _ = writeln!(Fd(1), "pushed {pushed}, then {}", name(full));
    for _ in 0..pushed {
````

with:

````rust
    show_ok("push an fd not open", sys::console_tee_push(30));
    // 4 tees at most; a kernel without the limit is stopped at 16.
    let mut pushed = 0;
    let mut full = None;
    while pushed < 16 && full.is_none() {
        match sys::console_tee_push(log) {
            Ok(()) => pushed += 1,
            Err(e) => full = Some(e),
        }
    }
    let _ = match full {
        Some(e) => writeln!(Fd(1), "pushed {pushed}, then {}", name(e)),
        None => writeln!(Fd(1), "pushed {pushed}, and no end"),
    };
    for _ in 0..pushed {
````

- [ ] **Step 3: Run the `files`, `tees` scenarios**

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add userland
git commit -m "Guest tests: every loop that waits for the kernel to say stop is bounded (t-files' fds and read_dir, t-tee's reads and pushes)"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 40 scenario(s) passed`.

````bash
git push -u origin m2p5/kernel
gh pr create --base main --head m2p5/kernel --title "Milestone 2, plan 5: The kernel's fixes, and the guest tests' bounds" --body-file - <<'EOF'
## What

Milestone 2, plan 5, tasks 1–10: the error screen kills every other process before it draws (`respawn` leaves an orphan running) and counts a line's rows as the terminal moves (tabs, carriage returns, backspaces; tested against `term::Terminal` at the NUC's 120×33), showing an escape instead of obeying it; `exec::arg_bytes`, `InputQueue::take_interrupt`, `is_line_mode`, `OpenFile::is_dir` and `arch::idle_forever` go; `read_dir` leaves its own buffer out of the heap room it allows; the TLB is flushed through `arch::tlb`, and a test keeps its instructions there; `parse_fadt` reads the DSDT field once; a USB connection still bouncing after the debounce's 2 s is `UsbError::Unstable` and not tried again; the guest tests' loops are bounded.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing the NUC runs changes but what the QEMU scenarios cover; NUC checks 3 and 4 (the last pull request of this plan) run it all
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p5/kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: xtask's fixes, and two new e2e steps (Tasks 11–15)

Milestone 1's findings in the tools (QMP, the loader-rules test, `verify-usb`), and the e2e steps `system-drop` and `reset-key` with the scenarios that use them (plan 4b's final review).

Branch `m2p5/xtask`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-xtask`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p5/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-xtask origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-xtask
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p5/kernel` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p5/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-xtask m2p5/kernel`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p5/kernel>` and re-run `cargo xtask ci` before pushing.

### Task 11: QMP: a message cut at a wait's deadline is read whole later

Milestone 1's deferred minor, decision 8: `Qmp::wait_event` set its deadline as the read timeout through the writer's duplicate of the socket (which works only because both share it), and a read that timed out in the middle of a message dropped what it had read, so the next read began mid-message and failed to parse. `read_message` now reads with `read_until` into a buffer kept in the `Qmp`, which keeps every byte read before an error, so the next read goes on where it stopped; the wait sets the timeout on the reader's own socket. The test's fake QEMU sends half an event, waits until the client's wait has timed out, then sends the rest and another event. The red run: the rest parses alone (`expected ident at line 1 column 2`). Mutation check: the kept bytes dropped before each read fails the test.

**Files:**
- Modify: `xtask/src/qmp.rs`

**Interfaces:**
- Consumes: milestone 1's `qmp::Qmp::{connect, wait_event, execute}`.
- Produces: `Qmp.partial: Vec<u8>` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, replace:

````rust

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
````

with:

````rust

    /// A fake QEMU on a socket of its own in `name`'s directory: greets,
    /// accepts `qmp_capabilities`, then runs `then` on its end.
    fn fake_qemu(
        name: &str,
        then: impl FnOnce(UnixStream) + Send + 'static,
    ) -> (Qmp, std::thread::JoinHandle<()>) {
        let dir = out_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("qmp.sock");
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut w = stream.try_clone().unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            writeln!(w, r#"{{"QMP": {{"version": {{}}, "capabilities": []}}}}"#).unwrap();
            r.read_line(&mut line).unwrap();
            writeln!(w, r#"{{"return": {{}}}}"#).unwrap();
            then(stream);
        });
        (
            Qmp::connect(&socket, Duration::from_secs(5)).unwrap(),
            server,
        )
    }

    #[test]
    fn an_event_cut_at_the_deadline_is_read_whole_later() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let (mut q, server) = fake_qemu("qmp-selftest-cut", move |mut w| {
            write!(w, r#"{{"event": "RESET", "da"#).unwrap();
            // The rest after the client gave up waiting.
            rx.recv().unwrap();
            writeln!(w, r#"ta": {{"guest": true}}}}"#).unwrap();
            writeln!(w, r#"{{"event": "SHUTDOWN"}}"#).unwrap();
            let _ = rx.recv();
        });
        assert!(
            q.wait_event("RESET", |_| true, Duration::from_millis(200))
                .is_err(),
            "only half of it came"
        );
        tx.send(()).unwrap();
        let e = q
            .wait_event("RESET", |_| true, Duration::from_secs(5))
            .unwrap();
        assert_eq!(e["data"]["guest"], true);
        q.wait_event("SHUTDOWN", |_| true, Duration::from_secs(5))
            .unwrap();
        drop((q, tx));
        server.join().unwrap();
    }

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask qmp`

Expected: FAIL: 1 test fails: `qmp::tests::an_event_cut_at_the_deadline_is_read_whole_later`.

- [ ] **Step 3: Change `xtask/src/qmp.rs`**

In `xtask/src/qmp.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    events: Vec<Value>,
}
````

with:

````rust
    events: Vec<Value>,
    /// The start of a message whose read timed out, for the next read.
    partial: Vec<u8>,
}
````

Replace:

````rust
            events: Vec::new(),
        };
````

with:

````rust
            events: Vec::new(),
            partial: Vec::new(),
        };
````

Replace:

````rust

    fn read_message(&mut self) -> Result<Value> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            bail!("QMP connection closed");
        }
        Ok(serde_json::from_str(&line)?)
    }
````

with:

````rust

    /// The next message. A read that times out keeps what came of the
    /// message so far (`read_until` leaves every byte it read in the
    /// buffer), so the next read goes on where it stopped.
    fn read_message(&mut self) -> Result<Value> {
        if self.reader.read_until(b'\n', &mut self.partial)? == 0 {
            bail!("QMP connection closed");
        }
        let line = std::mem::take(&mut self.partial);
        Ok(serde_json::from_slice(&line)?)
    }
````

Replace:

````rust
            }
            self.writer.set_read_timeout(Some(left))?;
            let read = self.read_message();
            self.writer
                .set_read_timeout(Some(Duration::from_secs(10)))?;
````

with:

````rust
            }
            self.reader.get_ref().set_read_timeout(Some(left))?;
            let read = self.read_message();
            self.reader
                .get_ref()
                .set_read_timeout(Some(Duration::from_secs(10)))?;
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask qmp`

Expected: PASS: 3 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "QMP: a message cut at a wait's deadline is read whole later, and the wait sets the timeout on the reader's own socket"
````


### Task 12: The loader-rules test sees comments outside string and character literals only

Milestone 1's deferred minor, decision 8: the loader-rules test cut every line at its first `//`, so a banned word after a `//` inside a string literal went unseen. `code` now scans the whole text: `//` starts a comment only outside a string (skipping each escaped character in it) and outside a character literal (`'"'`, `'\''`), while a lifetime (`'a`) is neither; the loader has no raw strings. The red run: `"a//b"` cut at its `//`. Mutation checks: no escapes in strings, no character literals, an escaped character literal not seen, and every `'x` taken for a character literal (a lifetime before a string holding `it's`) each fail the test; the escaped character's own consumption removed survives, since in valid Rust the escaped quote then ends the literal early and the real closing quote starts nothing.

**Files:**
- Modify: `boot/src/lib.rs`

**Interfaces:**
- Consumes: milestone 1's `boot/src/lib.rs` tests (`code`, `SOURCES`).
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `boot/src/lib.rs`**

In `boot/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature.
    fn code(src: &str) -> String {
````

with:

````rust
    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature. A `//` in a string literal is code, and
    /// so is a `"` in a character literal (`'"'`); a lifetime (`'a`) is
    /// neither. The loader has no raw strings.
    fn code(src: &str) -> String {
````

Replace:

````rust

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
````

with:

````rust

    #[test]
    fn a_string_is_code_even_with_slashes_in_it() {
        // Milestone 1's deferred finding: `//` in a string was cut as a
        // comment, and a banned word after it was not seen.
        let line = r#"let p = "a//b"; open("Exclusive"); // Exclusive"#;
        assert_eq!(code(line), r#"let p = "a//b"; open("Exclusive"); "#);
        let escaped = r#"let s = "\"//"; x(); // y"#;
        assert_eq!(code(escaped), r#"let s = "\"//"; x(); "#);
        let chars = r#"let q = '"'; let e = '\''; let d = '\"'; f(); // "g""#;
        assert_eq!(
            code(chars),
            r#"let q = '"'; let e = '\''; let d = '\"'; f(); "#
        );
        let lifetime = r#"fn f(s: &'static str) -> &str { "it's" } // x"#;
        assert_eq!(
            code(lifetime),
            r#"fn f(s: &'static str) -> &str { "it's" } "#
        );
        let two_lines = "let s = \"a\n//b\"; // c\nd";
        assert_eq!(code(two_lines), "let s = \"a\n//b\"; \nd");
    }

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-boot --lib`

Expected: FAIL: 1 test fails: `tests::a_string_is_code_even_with_slashes_in_it`.

- [ ] **Step 3: Change `boot/src/lib.rs`**

In `boot/src/lib.rs`, replace:

````rust
    fn code(src: &str) -> String {
        src.lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }
````

with:

````rust
    fn code(src: &str) -> String {
        let mut out = String::new();
        let mut chars = src.chars().peekable();
        let mut in_string = false;
        while let Some(c) = chars.next() {
            if in_string {
                out.push(c);
                match c {
                    '\\' => out.extend(chars.next()),
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '/' if chars.peek() == Some(&'/') => {
                    while chars.next_if(|&n| n != '\n').is_some() {}
                }
                '"' => {
                    in_string = true;
                    out.push(c);
                }
                '\'' => {
                    out.push(c);
                    let mut ahead = chars.clone();
                    let literal = match ahead.next() {
                        Some('\\') => true,
                        Some(_) => ahead.next() == Some('\''),
                        None => false,
                    };
                    if literal {
                        // Its character (escaped or not) and closing quote.
                        let first = chars.next();
                        out.extend(first);
                        if first == Some('\\') {
                            out.extend(chars.next());
                        }
                        for n in chars.by_ref() {
                            out.push(n);
                            if n == '\'' {
                                break;
                            }
                        }
                    }
                }
                _ => out.push(c),
            }
        }
        out
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-boot --lib`

Expected: PASS: 22 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add boot
git commit -m "Loader rules test: a // inside a string or a quote inside a character literal is code, not a comment"
````


### Task 13: `verify-usb` fails a transcript older than the `system.img` on the stick

Milestone 1's deferred minor, decision 8: `verify-usb` showed when each transcript was written but compared it with nothing, and `flash --kernel` leaves the transcripts of an earlier run on the stick, where they pass or fail for a kernel and programs that are no longer there. `verify-usb` now reads `\EFI\RELAY\system.img` off the stick's ESP (`image::esp_read`, with `mcopy`), prints `system.img: built <time>`, and `check_transcripts` fails a transcript older than that build time with `the transcript is older than the system on the stick (system.img built …): run the script again`; one from the same second is not older, and without a readable archive nothing is compared. `flash --full` erases the transcripts. The prototype compared a transcript with its script instead, which the review found could never catch a real case (minor, M2): `flash --full` writes the scripts at the flash's time and erases the transcripts, and `flash --kernel` keeps the scripts, so only a NUC clock running behind could fail it. The NUC's clock is UTC: on plan 4b's stick `system.img` was built at 13:39:32 UTC and the transcripts written at 14:45–14:48 UTC, which this `verify-usb` passes (84, 5 and 33 of them). The time is shown by `shell::time::date`, which becomes public; the test's helper gets a time per file. The red run is a compile error (`check_transcripts` takes the build time, `esp_read` is new). Mutation checks: the same second counted stale, never stale, stale but still passed, and `esp_read` ignoring `mcopy`'s failure each fail a test.

**Files:**
- Modify: `crates/shell/src/lib.rs`
- Modify: `xtask/src/flash.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: milestone 1's `flash::{check_transcripts, modified, verify_usb}`, `image::{esp_write, mtools_target}`; `sysimg::Archive::build_time`.
- Produces: `check_transcripts(target, root, machine, scratch, system_built: Option<u64>)`; `image::esp_read(target: &Path, esp: Partition, path: &str, scratch: &Path) -> Result<Vec<u8>>`; `pub mod shell::time`; the test helper `ext2_dated`.

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    fn ext2_with(dir: &Path, files: &[(&str, &str)]) -> (PathBuf, Partition) {
        let _ = fs::remove_dir_all(dir);
        for (path, text) in files {
            let p = dir.join("staging").join(path.trim_start_matches('/'));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            run(Command::new("touch")
                .args(["-d", "2026-09-28 14:09:17 UTC"])
                .arg(&p))
            .unwrap();
        }
````

with:

````rust
    fn ext2_with(dir: &Path, files: &[(&str, &str)]) -> (PathBuf, Partition) {
        let dated: Vec<_> = files
            .iter()
            .map(|&(path, text)| (path, text, "2026-09-28 14:09:17 UTC"))
            .collect();
        ext2_dated(dir, &dated)
    }

    /// An ext2 image holding `files` (path, contents, time changed) under
    /// its root.
    fn ext2_dated(dir: &Path, files: &[(&str, &str, &str)]) -> (PathBuf, Partition) {
        let _ = fs::remove_dir_all(dir);
        for (path, text, when) in files {
            let p = dir.join("staging").join(path.trim_start_matches('/'));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            run(Command::new("touch").args(["-d", when]).arg(&p)).unwrap();
        }
````

Replace:

````rust
                ("/root/checks/notes.txt", "not a script"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
````

with:

````rust
                ("/root/checks/notes.txt", "not a script"),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir).unwrap();
        assert!(!ok);
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir, None).unwrap();
        assert!(!ok);
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(!ok);
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
````

Replace:

````rust
    #[test]
    fn a_stick_without_check_scripts_passes() {
        let dir = out_dir().join("verify-usb-selftest-empty");
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir).unwrap();
        assert!(ok);
````

with:

````rust
    #[test]
    fn a_transcript_older_than_the_system_on_the_stick_fails_the_check() {
        // `flash --kernel` wrote a system built at 2026-09-29 10:00:00 UTC
        // and left the transcripts: one from before it shows what an older
        // kernel did. A run in the same second is not older.
        let built = 1_790_676_000;
        let dir = out_dir().join("verify-usb-selftest-stale");
        let (img, root) = ext2_dated(
            &dir,
            &[
                (
                    "/root/checks/a.sh",
                    "uname\n#> Relay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/a.log",
                    "+ uname\nRelay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/b.sh",
                    "uname\n#> Relay\n",
                    "2026-09-28 14:09:17 UTC",
                ),
                (
                    "/root/checks/b.log",
                    "+ uname\nRelay\n",
                    "2026-09-29 10:00:00 UTC",
                ),
            ],
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, Some(built)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  the transcript is older than the system on the stick (system.img built Tue Sep 29 10:00:00 UTC 2026): run the script again",
                "/root/checks/b.sh: ok, 1 of 1 commands as expected (run Tue Sep 29 10:00:00 2026 UTC)",
            ]
        );
        let (_, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok, "no system to compare with");
    }

    #[test]
    fn a_stick_without_check_scripts_passes() {
        let dir = out_dir().join("verify-usb-selftest-empty");
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok);
````

- [ ] **Step 2: Add the failing tests to `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
    #[test]
    fn fixed_script_pins_guids() {
````

with:

````rust
    #[test]
    fn a_file_of_the_esp_reads_back() {
        let dir = out_dir().join("esp-read-selftest");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("esp.img");
        let esp = Partition {
            start_lba: 0,
            sectors: 131072,
        };
        fs::File::create(&img)
            .unwrap()
            .set_len(esp.sectors * 512)
            .unwrap();
        let fat = mtools_target(&img, esp);
        run(mtools("mformat").args(["-i", &fat, "-F", "-T", "131072", "::"])).unwrap();
        run(mtools("mmd").args(["-i", &fat, "::/EFI", "::/EFI/RELAY"])).unwrap();
        esp_write(&img, esp, "/EFI/RELAY/system.img", b"archive", &dir).unwrap();
        assert_eq!(
            esp_read(&img, esp, "/EFI/RELAY/system.img", &dir).unwrap(),
            b"archive"
        );
        assert!(esp_read(&img, esp, "/EFI/RELAY/none", &dir).is_err());
    }

    #[test]
    fn fixed_script_pins_guids() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask flash`

Expected: FAIL: compile errors such as `` cannot find function `esp_read` in this scope ``; `this function takes 4 arguments but 5 arguments were supplied`.

- [ ] **Step 4: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod testing;
mod time;
mod transcript;
````

with:

````rust
mod testing;
pub mod time;
mod transcript;
````

- [ ] **Step 5: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
/// the lines to print and whether every script ran and its transcript
/// passed. Each line gives the time the transcript was last written, since
/// `flash --kernel` leaves the transcripts of an earlier run on the stick.
pub fn check_transcripts(
````

with:

````rust
/// the lines to print and whether every script ran and its transcript
/// passed. Each line gives the time the transcript was last written:
/// `flash --kernel` leaves the transcripts of an earlier run on the stick,
/// so one older than the `system.img` built at `system_built` (seconds
/// since 1970, if known) fails; `flash --full` erases them.
pub fn check_transcripts(
````

Replace:

````rust
    scratch: &Path,
) -> Result<(Vec<String>, bool)> {
````

with:

````rust
    scratch: &Path,
    system_built: Option<u64>,
) -> Result<(Vec<String>, bool)> {
````

Replace:

````rust
        let report = checks::check(&checks::parse(&script, machine)?, &transcript);
        let verdict = if report.ok() { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {})",
            report.passed,
            report.commands,
            modified(target, root, &log)?
        ));
        out.extend(report.failures.iter().map(|f| format!("  {f}")));
        ok &= report.ok();
    }
````

with:

````rust
        let report = checks::check(&checks::parse(&script, machine)?, &transcript);
        // A transcript from before the system on the stick (a later
        // `flash --kernel`) shows what an older kernel and programs did.
        let (run, run_text) = modified(target, root, &log)?;
        let stale = system_built.filter(|&built| run < built);
        let passed = report.ok() && stale.is_none();
        let verdict = if passed { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {run_text})",
            report.passed, report.commands,
        ));
        if let Some(built) = stale {
            out.push(format!(
                "  the transcript is older than the system on the stick (system.img built {}): run the script again",
                shell::time::date(built)
            ));
        }
        out.extend(report.failures.iter().map(|f| format!("  {f}")));
        ok &= passed;
    }
````

Replace:

````rust

/// When the file at `path` was last changed, in UTC, as debugfs shows it:
/// `Mon Sep 28 14:09:17 2026 UTC`.
fn modified(target: &Path, root: Partition, path: &str) -> Result<String> {
    let text = run_stdout(
````

with:

````rust

/// When the file at `path` was last changed: in seconds since 1970, and
/// in UTC as debugfs shows it (`Mon Sep 28 14:09:17 2026 UTC`).
fn modified(target: &Path, root: Partition, path: &str) -> Result<(u64, String)> {
    let text = run_stdout(
````

Replace:

````rust
    )?;
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix("mtime: "))
        .and_then(|l| l.split_once(" -- "))
        .map(|(_, when)| format!("{when} UTC"))
        .with_context(|| format!("{path}: debugfs shows no mtime"))
}
````

with:

````rust
    )?;
    // `mtime: 0x68d9407d -- Mon Sep 28 14:09:17 2026`, with `:<nanoseconds>`
    // after the number on a filesystem with big inodes.
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix("mtime: 0x"))
        .and_then(|l| {
            let (number, when) = l.split_once(" -- ")?;
            let secs = u64::from_str_radix(number.split(':').next()?, 16).ok()?;
            Some((secs, format!("{when} UTC")))
        })
        .with_context(|| format!("{path}: debugfs shows no mtime"))
}

/// When the `system.img` on the ESP `esp` of `target` was built (seconds
/// since 1970), if it can be read.
fn system_built(target: &Path, esp: Partition, scratch: &Path) -> Option<u64> {
    let image = image::esp_read(target, esp, "/EFI/RELAY/system.img", scratch).ok()?;
    sysimg::Archive::parse(&image).ok().map(|a| a.build_time())
}
````

Replace:

````rust
    ensure_access(&stick, false)?;
    let Layout { root, .. } = image::read_layout(Stick::target())?;
    image::fsck(Stick::target(), root).context("filesystem check FAILED")?;
````

with:

````rust
    ensure_access(&stick, false)?;
    let Layout { esp, root } = image::read_layout(Stick::target())?;
    image::fsck(Stick::target(), root).context("filesystem check FAILED")?;
````

Replace:

````rust
    }
    let (report, ok) = check_transcripts(
        Stick::target(),
        root,
        Machine::Nuc,
        &out_dir().join("verify-usb"),
    )?;
    for l in report {
````

with:

````rust
    }
    let scratch = out_dir().join("verify-usb");
    let built = system_built(Stick::target(), esp, &scratch);
    match built {
        Some(secs) => println!("system.img: built {}", shell::time::date(secs)),
        None => println!("system.img: not readable on the stick; transcripts not compared with it"),
    }
    let (report, ok) = check_transcripts(Stick::target(), root, Machine::Nuc, &scratch, built)?;
    for l in report {
````

- [ ] **Step 6: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
        .args(["-i", &mtools_target(target, esp)])
        .arg(format!("::{path}")))
}

````

with:

````rust
        .args(["-i", &mtools_target(target, esp)])
        .arg(format!("::{path}")))
}

/// The bytes of one file (absolute ESP path, `/` separators) of an
/// existing ESP.
pub fn esp_read(target: &Path, esp: Partition, path: &str, scratch: &Path) -> Result<Vec<u8>> {
    fs::create_dir_all(scratch)?;
    let file = scratch.join("esp-read.tmp");
    run(mtools("mcopy")
        .args(["-o", "-i", &mtools_target(target, esp)])
        .arg(format!("::{path}"))
        .arg(&file))?;
    Ok(fs::read(&file)?)
}

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask flash`

Expected: PASS: 8 tests.

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask a_file_of_the_esp_reads_back`

Expected: PASS: 1 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates xtask
git commit -m "verify-usb: a transcript older than the system.img on the stick fails the check (it shows what an older kernel did)"
````


### Task 14: The e2e step `system-drop`, and `system_nosh`: without `/bin/sh` the error screen says it cannot start

Plan 4b's final review (deferred minor 5) and decision 9: spec §16 item 6 and the roadmap said a `/bin/sh` that cannot be started reaches the error screen "in a host test", while only the reason's text was tested; init's `CannotStart` branch had never run. The before-boot step `system-drop NAME` rewrites `system.img` without that program (`userland::without`, which refuses a name it does not hold), and `system_nosh` drops `sh`: the archive mounts (its count of programs left open, the review's M4, since milestone 3 adds programs), the error screen says `/bin/sh cannot start: No such file or directory`, waits, and comes back after a key. The red run is a compile error (the step and the function are new); the scenario passes on arrival, since the branch was right. Mutation checks: the branch showing `Reason::Ended` fails `system_nosh`; `without` keeping every program fails on its own refusal.

**Files:**
- Create: `tests/e2e/system_nosh.txt`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: plan 1's `userland::with_abi`, `sysimg::{Archive, write}`; the e2e runner's `EspEdit`.
- Produces: `userland::without(image: &[u8], name: &str) -> Result<Vec<u8>>`; `EspEdit::SystemDrop(String)`; the step `system-drop`; `tests/e2e/system_nosh.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/system_nosh.txt`**

Create `tests/e2e/system_nosh.txt`:

````text
# A system.img without /bin/sh mounts, but init cannot start a shell
# (user-space gate §6.6, §11.2): the error screen says why and shows the
# kernel log's last lines; it waits, a key restarts the machine, and it
# comes back to the same screen.
system-drop sh
timeout 30
expect \[ ok \] system: \d+ programs, ABI 2\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh cannot start: No such file or directory\n\n--- last kernel log lines ---\n
expect \[ ok \] system: \d+ programs, ABI 2\n
expect \nPress any key to reboot\.\n
# It waits for the key, however long that takes.
alive 2
reset
expect Relay OS \d+\.\d+\.\d+
expect \n/bin/sh cannot start: No such file or directory\n
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn parses_esp_deletes_and_other_abis() {
        let s = parse_scenario(
            "x",
            "esp-delete /EFI/RELAY/system.img\nsystem-abi 99\nexpect x",
        )
````

with:

````rust
    #[test]
    fn parses_esp_deletes_other_abis_and_programs_left_out() {
        let s = parse_scenario(
            "x",
            "esp-delete /EFI/RELAY/system.img\nsystem-abi 99\nsystem-drop sh\nexpect x",
        )
````

Replace:

````rust
                EspEdit::Delete("/EFI/RELAY/system.img".into()),
                EspEdit::SystemAbi(99)
            ]
        );
        assert!(parse_scenario("x", "esp-delete relative").is_err());
````

with:

````rust
                EspEdit::Delete("/EFI/RELAY/system.img".into()),
                EspEdit::SystemAbi(99),
                EspEdit::SystemDrop("sh".into())
            ]
        );
        assert!(parse_scenario("x", "system-drop").is_err(), "which one");
        assert!(parse_scenario("x", "system-drop a b").is_err());
        assert!(parse_scenario("x", "expect a\nsystem-drop sh").is_err());
        assert!(parse_scenario("x", "esp-delete relative").is_err());
````

- [ ] **Step 3: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust

    fn t_args() -> Program {
````

with:

````rust

    #[test]
    fn a_program_can_be_left_out_of_the_archive() {
        let entries = [
            sysimg::Entry {
                name: b"sh",
                mode: 0o755,
                data: b"one",
            },
            sysimg::Entry {
                name: b"ls",
                mode: 0o755,
                data: b"two",
            },
        ];
        let image = sysimg::write(relay_abi::VERSION, 7, &entries).unwrap();
        let less = without(&image, "sh").unwrap();
        let archive = sysimg::Archive::parse(&less).unwrap();
        let names: Vec<&[u8]> = archive.entries().map(|e| e.name).collect();
        assert_eq!(names, [&b"ls"[..]]);
        assert_eq!(archive.build_time(), 7);
        assert!(without(&image, "cat").is_err(), "not in the archive");
    }

    fn t_args() -> Program {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask e2e`

Expected: FAIL: compile errors such as `` cannot find function `without` in this scope ``; `` no variant, associated function, or constant named `SystemDrop` found for enum `e2e::EspEdit` in the current scope ``.

- [ ] **Step 5: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//!                                   another ABI version)
//! timeout 20                       (seconds, for the following expects)
````

with:

````rust
//!                                   another ABI version)
//! system-drop sh                   (before boot: system.img, rewritten
//!                                   without that program)
//! timeout 20                       (seconds, for the following expects)
````

Replace:

````rust
    SystemAbi(u32),
}
````

with:

````rust
    SystemAbi(u32),
    /// `system-drop`: `system.img` holds its programs but this one.
    SystemDrop(String),
}
````

Replace:

````rust
            }
            "esp-write" | "esp-delete" | "system-abi" => {
                if !steps.is_empty() {
````

with:

````rust
            }
            "esp-write" | "esp-delete" | "system-abi" | "system-drop" => {
                if !steps.is_empty() {
````

Replace:

````rust
                        })?),
                        _ => {
````

with:

````rust
                        })?),
                        "system-drop" => {
                            if rest.is_empty() || rest.contains(' ') {
                                bail!("{name}:{line_no}: system-drop needs one program's name");
                            }
                            EspEdit::SystemDrop(rest.to_string())
                        }
                        _ => {
````

Replace:

````rust
            EspEdit::Delete(path) => esp_delete(&q.disk, layout.esp, path)?,
            EspEdit::SystemAbi(abi) => {
                let image = fs::read(out_dir().join("system.img"))?;
                let other = userland::with_abi(&image, *abi)?;
                esp_write(
````

with:

````rust
            EspEdit::Delete(path) => esp_delete(&q.disk, layout.esp, path)?,
            EspEdit::SystemAbi(_) | EspEdit::SystemDrop(_) => {
                let image = fs::read(out_dir().join("system.img"))?;
                let other = match edit {
                    EspEdit::SystemAbi(abi) => userland::with_abi(&image, *abi)?,
                    EspEdit::SystemDrop(program) => userland::without(&image, program)?,
                    _ => unreachable!(),
                };
                esp_write(
````

- [ ] **Step 6: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
    sysimg::write(abi, archive.build_time(), &entries)
        .map_err(|e| anyhow::anyhow!("system.img: {e}"))
````

with:

````rust
    sysimg::write(abi, archive.build_time(), &entries)
        .map_err(|e| anyhow::anyhow!("system.img: {e}"))
}

/// `image` without the program `name` (the e2e step `system-drop`); an
/// error if it holds none of that name.
pub fn without(image: &[u8], name: &str) -> Result<Vec<u8>> {
    let archive = sysimg::Archive::parse(image).map_err(|e| anyhow::anyhow!("system.img: {e}"))?;
    let entries: Vec<sysimg::Entry<'_>> = archive
        .entries()
        .filter(|e| e.name != name.as_bytes())
        .collect();
    if entries.len() == archive.entries().count() {
        bail!("system.img has no program {name}");
    }
    sysimg::write(archive.abi(), archive.build_time(), &entries)
        .map_err(|e| anyhow::anyhow!("system.img: {e}"))
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask e2e`

Expected: PASS: 27 tests.

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 8 tests.

- [ ] **Step 9: Run the `system_nosh` scenario**

Run: `cargo xtask test --e2e-only --scenario system_nosh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add tests xtask
git commit -m "e2e: the before-boot step system-drop, and the system_nosh scenario: without /bin/sh the error screen says it cannot start"
````


### Task 15: The e2e step `reset-key`: a key on the USB keyboard at the error screen

Plan 4b's final review (deferred minor 4 and 5) and decision 9: no scenario pressed the error screen's key on the USB keyboard (`reset` types over COM1). `reset-key` presses Enter through QMP's `send-key` (the USB keyboard) and expects the machine to restart, as `reset` does; it takes no text, since after the first key the machine is on its way down. `exit_with` splits into typing and `wait_exit`, `reboot` into `exit_with` and `boot_again`, and the 105-column comment the review found goes with the split. `system_missing` leaves the screen with it. An e2e red run is impossible (the scenario uses the step this task adds): the red run is the parser's test, a compile error. Mutation check: no key pressed fails `system_missing` (`QEMU still runs 30s after Enter on the USB keyboard`).

**Files:**
- Modify: `tests/e2e/system_missing.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: the e2e runner's `exit_with`, `reboot`, `KEY_HOLD_MS`, `Qmp::execute`.
- Produces: `Step::ResetKey`; `wait_exit(r, command, status, timeout) -> Result<fs::File>`, `boot_again(r, log, last) -> Result<()>` (private).

- [ ] **Step 1: Expect the new lines in `tests/e2e/system_missing.txt`**

In `tests/e2e/system_missing.txt`, replace:

````text
expect \[FAIL\] system: no system\.img\n\nPress any key to reboot\.\n
# It waits for the key, however long that takes.
alive 2
reset
expect Relay OS \d+\.\d+\.\d+
````

with:

````text
expect \[FAIL\] system: no system\.img\n\nPress any key to reboot\.\n
# It waits for the key, however long that takes; the key comes from the
# USB keyboard, as on the NUC.
alive 2
reset-key
expect Relay OS \d+\.\d+\.\d+
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff\nreset\nreset reboot -f",
        )
````

with:

````rust
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff\nreset\nreset reboot -f\nreset-key",
        )
````

Replace:

````rust
                (5, Step::Reset(String::new())),
                (6, Step::Reset("reboot -f".into()))
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
    }
````

with:

````rust
                (5, Step::Reset(String::new())),
                (6, Step::Reset("reboot -f".into())),
                (7, Step::ResetKey)
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
        assert!(
            parse_scenario("x", "reset-key x").is_err(),
            "Enter alone: after the first key the machine is gone"
        );
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask e2e`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `ResetKey` found for enum `e2e::Step` in the current scope ``.

- [ ] **Step 4: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//!                                   again on the same disk)
//! poweroff [<command>]             (types `poweroff`, or <command>; QEMU must
````

with:

````rust
//!                                   again on the same disk)
//! reset-key                        (as `reset`, with Enter pressed on the USB
//!                                   keyboard: QMP send-key)
//! poweroff [<command>]             (types `poweroff`, or <command>; QEMU must
````

Replace:

````rust
    Reset(String),
    /// Switch the machine off with a command (`poweroff` if none is
````

with:

````rust
    Reset(String),
    /// As `Reset`, with Enter pressed on the USB keyboard instead.
    ResetKey,
    /// Switch the machine off with a command (`poweroff` if none is
````

Replace:

````rust
            "reset" => Step::Reset(rest.to_string()),
            "reboot" => {
````

with:

````rust
            "reset" => Step::Reset(rest.to_string()),
            "reset-key" if rest.is_empty() => Step::ResetKey,
            "reboot" => {
````

Replace:

````rust

/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean, unless the stick was pulled out. Returns the serial log once everything QEMU printed is in it.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    r.stdin.write_all(command.as_bytes())?;
    r.stdin.write_all(b"\r")?;
    r.stdin.flush()?;
    let deadline = Instant::now() + timeout;
````

with:

````rust

/// Types `command` over serial and waits for QEMU to exit with `status`
/// (`wait_exit`).
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    r.stdin.write_all(command.as_bytes())?;
    r.stdin.write_all(b"\r")?;
    r.stdin.flush()?;
    wait_exit(r, command, status, timeout)
}

/// Waits (up to `timeout`) for QEMU to exit with `status` after `command`;
/// the machine shut the filesystem down first, so it must be marked clean,
/// unless the stick was pulled out. Returns the serial log once everything
/// QEMU printed is in it.
fn wait_exit(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<fs::File> {
    let deadline = Instant::now() + timeout;
````

Replace:

````rust
fn reboot(r: &mut Running, command: &str, last: Option<&str>, timeout: Duration) -> Result<()> {
    let mut log = exit_with(r, command, EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
````

with:

````rust
fn reboot(r: &mut Running, command: &str, last: Option<&str>, timeout: Duration) -> Result<()> {
    let log = exit_with(r, command, EXIT_RESET, timeout)?;
    boot_again(r, log, last)
}

/// After a reset: the machine printed `last` if given, and the same disk
/// boots again, with the serial log `log` continued.
fn boot_again(r: &mut Running, mut log: fs::File, last: Option<&str>) -> Result<()> {
    if let Some(pattern) = last {
````

Replace:

````rust
        Step::Reset(text) => reboot(r, text, None, *timeout)?,
        Step::FileLines { path, bytes, line } => {
````

with:

````rust
        Step::Reset(text) => reboot(r, text, None, *timeout)?,
        Step::ResetKey => {
            let enter = serde_json::json!([{ "type": "qcode", "data": "ret" }]);
            r.qmp.execute(
                "send-key",
                serde_json::json!({ "keys": enter, "hold-time": KEY_HOLD_MS }),
            )?;
            let log = wait_exit(r, "Enter on the USB keyboard", EXIT_RESET, *timeout)?;
            boot_again(r, log, None)?;
        }
        Step::FileLines { path, bytes, line } => {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask e2e`

Expected: PASS: 27 tests.

- [ ] **Step 6: Run the `system_missing` scenario**

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add tests xtask
git commit -m "e2e: the step reset-key presses Enter on the USB keyboard, and system_missing leaves the error screen with it"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 41 scenario(s) passed`.

````bash
git push -u origin m2p5/xtask
gh pr create --base main --head m2p5/xtask --title "Milestone 2, plan 5: xtask's fixes, and two new e2e steps" --body-file - <<'EOF'
## What

Milestone 2, plan 5, tasks 11–15: a QMP message cut at a wait's deadline is read whole later, and the wait sets the timeout on the reader's own socket; the loader-rules test sees comments outside string and character literals only; `verify-usb` fails a transcript older than its script; the before-boot step `system-drop` and the scenario `system_nosh` (the error screen for a `/bin/sh` that cannot start); the step `reset-key` (Enter on the USB keyboard), which `system_missing` uses.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing the NUC runs changes but what the QEMU scenarios cover; NUC checks 3 and 4 (the last pull request of this plan) run it all
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p5/xtask --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-xtask
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Version 0.3.0, and NUC checks 3 and 4 (Tasks 16–17)

Milestone 2 is done: version 0.3.0, the README and `docs/hardware-test.md` (check 4 visits the error screen by hand), and NUC checks 3 and 4 on a stick written by `flash --full`. This pull request goes up as a draft; after the user's NUC checks pass, the real transcripts and the results-log row go into a commit of it.

Branch `m2p5/release`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-release`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p5/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-release origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-release
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p5/xtask` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p5/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-release m2p5/xtask`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p5/xtask>` and re-run `cargo xtask ci` before pushing.

### Task 16: Version 0.3.0

Spec §1.4 and decision 11: milestone 2 is version 0.3.0, as milestone 1 was 0.2.0. A spike bumped the version first and ran everything: only `uname`'s unit test, the `shell` scenario (milestone 1's, changed only by the version, as milestone 1's own bump did), `check3-a.sh`'s `uname -a` line and the recorded transcripts depend on it. The transcripts' `Relay relay 0.3.0 x86_64` and `Relay OS 0.3.0` lines are what the `checks` scenario's disk holds now (QEMU's), and the NUC's until NUC check 3 records them. `relay-abi`'s comment and `docs/hardware-test.md`'s check 1 name the new version; the results log keeps its rows. The red runs are `uname`'s test and `shell`; `Cargo.toml` goes in after them.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Modify: `crates/relay-abi/src/info.rs`
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/shell.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`
- Modify: `xtask/fixtures/checks/check3-b.qemu.log`

**Interfaces:**
- Consumes: `env!("CARGO_PKG_VERSION")` (the kernel's banner and `uname`, the shell's `UNAME_ALL`).
- Produces: version 0.3.0.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 1 is version 0.2.0 (spec §15 item 12), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.2.0 x86_64\n".into()));
        assert_eq!(
````

with:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 2 is version 0.3.0 (user-space gate §16 item 7), from
        // Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.3.0 x86_64\n".into()));
        assert_eq!(
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/shell.txt`**

In `tests/e2e/shell.txt`, replace:

````text
send uname -a
expect \nRelay relay 0\.2\.0 x86_64\n
send cat /etc/hostname
````

with:

````text
send uname -a
expect \nRelay relay 0\.3\.0 x86_64\n
send cat /etc/hostname
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::uname_prints_the_system`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: FAIL: scenario `shell` stops at line 8, timed out waiting for `\nRelay relay 0\.3\.0 x86_64\n`.

- [ ] **Step 4: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
[workspace.package]
version = "0.2.0"
edition = "2024"
````

with:

````toml
[workspace.package]
version = "0.3.0"
edition = "2024"
````

- [ ] **Step 5: Change `crates/relay-abi/src/info.rs`**

In `crates/relay-abi/src/info.rs`, replace:

````rust
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.2.0`.
    pub release: [u8; UNAME_FIELD],
````

with:

````rust
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.3.0`.
    pub release: [u8; UNAME_FIELD],
````

- [ ] **Step 6: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.2.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

with:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.3.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

- [ ] **Step 7: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
uname -a
#> Relay relay 0\.2\.0 x86_64
date
````

with:

````bash
uname -a
#> Relay relay 0\.3\.0 x86_64
date
````

- [ ] **Step 8: Change `xtask/fixtures/checks/check3-a.nuc.log`**

In `xtask/fixtures/checks/check3-a.nuc.log`, make these 2 replacements, top to bottom:

Replace:

````text
+ uname -a
Relay relay 0.2.0 x86_64
+ date
Wed Sep 30 14:43:08 UTC 2026
+ dmesg
Relay OS 0.2.0
[[32m ok [0m] console 1920x1200 (120x33 cells)
````

with:

````text
+ uname -a
Relay relay 0.3.0 x86_64
+ date
Wed Sep 30 14:43:08 UTC 2026
+ dmesg
Relay OS 0.3.0
[[32m ok [0m] console 1920x1200 (120x33 cells)
````

Replace:

````text
+ t-sys uname
Relay relay 0.2.0 x86_64
+ t-read apart
````

with:

````text
+ t-sys uname
Relay relay 0.3.0 x86_64
+ t-read apart
````

- [ ] **Step 9: Change `xtask/fixtures/checks/check3-a.qemu.log`**

In `xtask/fixtures/checks/check3-a.qemu.log`, make these 2 replacements, top to bottom:

Replace:

````text
+ uname -a
Relay relay 0.2.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1080 (120x33 cells)
````

with:

````text
+ uname -a
Relay relay 0.3.0 x86_64
+ date
Mon Sep 28 14:09:17 UTC 2026
+ dmesg
Relay OS 0.3.0
[ ok ] console 1920x1080 (120x33 cells)
````

Replace:

````text
+ t-sys uname
Relay relay 0.2.0 x86_64
+ t-read apart
````

with:

````text
+ t-sys uname
Relay relay 0.3.0 x86_64
+ t-read apart
````

- [ ] **Step 10: Change `xtask/fixtures/checks/check3-b.nuc.log`**

In `xtask/fixtures/checks/check3-b.nuc.log`, replace:

````text
+ dmesg
Relay OS 0.2.0
[[32m ok [0m] console 1920x1200 (120x33 cells)
````

with:

````text
+ dmesg
Relay OS 0.3.0
[[32m ok [0m] console 1920x1200 (120x33 cells)
````

- [ ] **Step 11: Change `xtask/fixtures/checks/check3-b.qemu.log`**

In `xtask/fixtures/checks/check3-b.qemu.log`, replace:

````text
+ dmesg
Relay OS 0.2.0
[ ok ] console 1920x1080 (120x33 cells)
````

with:

````text
+ dmesg
Relay OS 0.3.0
[ ok ] console 1920x1080 (120x33 cells)
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 147 tests.

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 13: Run the `shell`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add Cargo.lock Cargo.toml crates docs rootfs tests xtask
git commit -m "Version 0.3.0: milestone 2 is done (uname -a, the startup line, the check scripts and their transcripts)"
````


### Task 17: Docs: milestone 2 is done, and NUC check 4 visits the error screen

Decisions 11 and 12: the README says milestone 2 is 0.3.0 and milestone 3 comes next. `docs/hardware-test.md`'s check 4 gains a step by hand after `exit`: after a `reboot`, three quick `exit`s reach the error screen (photographed), and a key on the K120 restarts the machine. The reboot comes first because the screen shows the kernel log's last 20 lines: right after `check4.sh` they are its kill lines, none wider than the screen, as the prototype's review found (important, I1), while after a boot the NUC's `xhci 00:14.0: slot 4: interface 0 class 8/6/80, …` line, 122 columns, is among them and must take two rows (its real `dmesg` in `check3-b.nuc.log` shows the lines). `verify-usb`'s step explains the new stale check; the failure table gains a key that does nothing, a heading scrolled away and a stale transcript. The stick's `/root/README` no longer says milestone 1 (the review's M6; no test reads it). The task changes documents only.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/README`

**Interfaces:**
- Consumes: Tasks 1–16.
- Produces: nothing new.

- [ ] **Step 1: Change `README.md`**

In `README.md`, replace:

````markdown
restarts at a key. The kernel holds no shell of its own any more.

````

with:

````markdown
restarts at a key. The kernel holds no shell of its own any more.

Milestone 1 is version 0.2.0 and milestone 2 version 0.3.0; milestone 3
(pipes, background jobs, `ps` and `kill`, script variables) comes next.

````

- [ ] **Step 2: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 5 replacements, top to bottom:

Replace:

````markdown

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`). Since plan 6 the
commands come from two scripts on the stick, `/root/checks/check3-a.sh` and
`check3-b.sh` (in the repository under `rootfs/root/checks/`), and since
milestone 2's plan 4b a third, `check4.sh` (NUC check 4 of the user-space
gate, spec §12.4). Every command is a program now, and `/bin/sh` runs
them, which the kernel's init starts as process 2: `sh` runs
each line as if it were typed, shows it as `+ <command>` before its output,
and writes everything it shows into a transcript next to the script
(`check3-a.log`). `verify-usb` checks the transcripts in Mint against the
output the scripts expect (their `#>` lines), so only the boot screen needs
a look. The QEMU scenario `checks` runs the same scripts on every pull
request.

````

with:

````markdown

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

````

Replace:

````markdown
   program, and process 1 starts another. Photograph the screen.
8. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
9. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
````

with:

````markdown
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
````

Replace:

````markdown
   and `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run
   <time>)`, with the UTC times of the three runs (after `flash --kernel` the
   transcripts of an earlier run stay on the stick, so check the times). A
   `FAILED` line is followed by the script line, the expectation that
   failed and what the command printed there; a script that was not run is
   `FAILED, not run`.

````

with:

````markdown
   and `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run
   <time>)`, with the UTC times of the three runs, after the line
   `system.img: built <time>`. A transcript older than the stick's
   `system.img` (a run before the last `flash --kernel`, which keeps the
   transcripts; `flash --full` erases them) fails. A `FAILED` line is
   followed by the script line, the expectation that failed and what the
   command printed there; a script that was not run is `FAILED, not run`.

````

Replace:

````markdown
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
````

with:

````markdown
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
| A key on the K120 at the error screen does nothing | The keyboard is not polled while init waits (the idle task polls it) | Photograph the screen and hold the power button |
| The error screen's heading has scrolled away, or a program's line shows under `Press any key` | A log line takes more rows than counted, or a process still ran | Photograph the screen |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
````

Replace:

````markdown
| `verify-usb`: `check3-a.sh: FAILED` (or another script) | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

with:

````markdown
| `verify-usb`: `check3-a.sh: FAILED` (or another script) | A command printed something else than the script expects | The line after it names the command, the expectation and the line it printed instead; the whole transcript is `/root/checks/check3-a.log` on the stick (`cat checks/check3-a.log` on the NUC) |
| `verify-usb`: `the transcript is older than the system on the stick` | The script ran before the last `flash --kernel` | Run the script again on the NUC |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

- [ ] **Step 3: Change `rootfs/root/README`**

Replace the whole of `rootfs/root/README` with:

````text
Relay OS

Files live on the USB stick and survive reboots. Every command is a
program in /bin (ls /bin), and the shell is /bin/sh. Useful commands:
  ls, cd, pwd, cat, echo text > file, mkdir, rm, cp, mv, sh FILE, help
````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add README.md docs rootfs
git commit -m "Docs: milestone 2 is done (0.3.0); NUC check 4 visits the error screen after a reboot and leaves it with a key on the K120; the stick's README"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 41 scenario(s) passed`.

````bash
git push -u origin m2p5/release
gh pr create --draft --base main --head m2p5/release --title "Milestone 2, plan 5: Version 0.3.0, and NUC checks 3 and 4" --body-file - <<'EOF'
## What

Milestone 2, plan 5, tasks 16–17: version 0.3.0 (`uname -a` says `Relay relay 0.3.0 x86_64`; `shell`, `check3-a.sh` and the recorded transcripts follow); the README says milestone 2 is done; NUC check 4 visits the error screen and leaves it with a key on the K120.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC checks 3 and 4 of `docs/hardware-test.md` (check 3's scripts and steps by hand, `check4.sh`, `exit` by hand and the error screen), run by the user with `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p5/release --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for NUC checks 3 and 4**

Ask the user to run NUC checks 3 and 4 (`docs/hardware-test.md`, check 3's steps 1–10: check 3's scripts, the three steps by hand, `check4.sh`, `exit` by hand and the error screen by hand) with `cargo xtask flash --full` from this worktree, and to report their results; the pull request stays a draft until they do. The stick is then usually plugged into this machine, its root mounted at `/media/maw/relayroot`.

- [ ] **Record the NUC's transcripts in this pull request**

After a pass: check the stick with `cargo xtask verify-usb` (it only reads it), copy the real transcripts over the recorded ones, and add the results-log row of `docs/hardware-test.md` (date, `3 and 4 (milestone 2 done, 0.3.0)`, this branch's commit, `Pass`, and the notes: the startup lines, the three scripts' counts, the steps by hand, the error screen and the K120 key, anything the user saw):

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
git commit -m "Check scripts: the transcripts of NUC checks 3 and 4 on 0.3.0, and their row in the results log"
git push
gh pr ready m2p5/release
````

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. Propose the text of the `v0.3.0` tag and the GitHub release "Relay OS 0.3.0 — milestone 2" (as milestone 1's `v0.2.0`), and make neither unless the user asks. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p5-release
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
