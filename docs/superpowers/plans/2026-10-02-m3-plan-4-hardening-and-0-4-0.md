# Milestone 3 · Plan 4: Hardening and 0.4.0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Milestone 3 and the user-space gate end. What the reviews of plans 1–3 and of milestone 2's plan 5 deferred to plan 4 is fixed or ruled out: in the shell (`$[`, quotes inside `${…}`, a value copied before its room is taken, `cd ""`, `seq`'s zero increment, `[a-a-`, a stray prompt from an interactive `sh` in a background script), in `verify-usb` (one date form, an unreadable `system.img` fails) and in two source scans (raw strings, `invpcid`). `check5.sh` runs pipes, a background job and `kill`, and script arguments and variables in the `checks` scenario and on the NUC. The spec's §16 gets item 11, and the version becomes 0.4.0. It ends with `cargo xtask ci` green and NUC checks 3, 4 and 5 passed on a stick written by `flash --full` (spec §1.4).

**Architecture:** No new component; the kernel and the ABI do not change. The parser reads a `${…}`'s text as bash does (`brace_text`) and refuses `$[`; the expander borrows a parameter's value (`Value`); `cd`, `seq` and `grep`'s sets change at one point each; `/bin/sh` asks to set the console's mode before its first prompt. xtask's `check_transcripts` takes what reading `system.img` gave and reports it first; the loader's scanner reads raw strings; the TLB scan sees `invpcid`. `check5.sh` joins the check scripts and their recorded transcripts.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model); bash 5.2, GNU seq 9.4 and GNU grep 3.11 for the expectations; e2fsprogs (`debugfs`) and mtools for xtask's tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.4, §2, §8.5, §9, §10, §12.4, §13 step 9, §15, §16 items 7–11)
**Roadmap:** `docs/superpowers/plans/2026-10-01-milestone-3-roadmap.md` — this is plan 4 of 4 of milestone 3 (spec §13 step 9), the last of the user-space gate.

## In brief

- **Size.** 18 tasks in three code pull requests, plus this plan as PR 1 (decision 1): PR 2 the shell's deferred fixes; PR 3 xtask's and the scans' deferred fixes; PR 4 `check5.sh`, version 0.4.0 and the docs, a draft until NUC checks 3, 4 and 5 pass, whose transcripts go into it.
- **A spike first** (decision 8). Version 0.4.0 and a draft `check5.sh` in the `checks` scenario: every one of the 47 scenarios passed, and nothing but `uname`'s test, `shell`, `check3-a.sh` and the recorded transcripts depends on the version.
- **`check5.sh`** (decision 2): 29 commands whose lines are the same on the NUC and in QEMU: pipes, a background job and `kill`, a script passing `"$@"` with an empty argument and one with blanks to another. A script says nothing about its jobs, not even one a signal ended, which bash's script would report (decision 3, the user chose), so nothing races the prompt.
- **The shell's deferred findings** (decision 4), each fixed with a test that fails first (but the borrowed value, a refactor the existing tests and mutation checks cover), against bash 5.2 or GNU's tools: `$[` refused; quotes, escapes and `${…}` inside `${…}` read as bash reads them; a value borrowed before its room is taken; `cd ""` a no-op; `seq`'s zero increment refused in GNU's order; `[a-a-` an invalid range end; an interactive `sh` in a background script ending before its prompt.
- **Two ruled out** (decision 5, the user agreed): `~` after an assignment-shaped argument's `=` (bash's POSIX mode keeps it too), and bash's `(wd: …)` note.
- **xtask's and the scans' deferred findings** (decision 6): `verify-usb` shows every time as `date` does and fails on an unreadable `system.img`; the loader's scanner reads raw strings rather than assuming there are none; the TLB scan sees `invpcid`, and both instructions in any case.
- **The spec** (decisions 7, 9, 10): §8.5's table, §12.4's check 5, item 9's words on a script's jobs and item 10's on `~` are corrected; milestone 3's definition of done is met item by item.
- **The prototype's review.** A fresh reviewer (Opus) read the whole prototype, ran every crate's tests, throwaway QEMU scenarios under KVM on the NUC's CPU model (`check5.sh` typed on the USB keyboard after `check4.sh`, then again over serial: byte for byte the recorded transcript) and fuzzed the fixes against bash 5.2 (17 841 lines with `${`, quotes and escapes), GNU seq 9.4 (10 127 calls) and GNU grep 3.11 (5 425 sets), and found no critical, no important and 3 minor real defects, plus three nits. Each minor is fixed in a task of its own after the task it concerns; one nit, the TLB scan's case, is fixed too, and the other two are ruled out in decision 5. It found correct every other fix against bash or GNU, `/bin/sh`'s start under init, at a prompt, in a script and in a pipeline, `check5.sh`'s determinism (no killed line reaches a tee, `grep -c` counts only the job), every path of `verify-usb`, CI's tools, the scanners, the version's reach and the docs:

| Finding (review) | Decision |
|---|---|
| Minor (M1): the README's quick start typed three check scripts, so following it left `check5.sh` unrun and `verify-usb` failed | Fixed, Task 17, with a test that the README and `docs/hardware-test.md` type every script |
| Minor (M2): `check5.sh`'s nested script began at home, so a `cd "$9"` that went home passed it | Fixed, Task 14: it goes to `/root/checks` first (29 commands) |
| Minor (M3): two of check 5's failure rows asked for `ps` where no prompt comes back, and for a count a failed `kill` never reaches | Fixed, Task 18 |
| Nit: the TLB scan was case-sensitive (`INVLPG` passed) | Fixed, Task 12 |
| Nits: `seq 1 1e-5000 5` (GNU's `long double` underflows to a zero increment); rare `${…}` forms (`${$${A}`, ``${A`}`}``) refused with another error than bash's | Ruled out in decision 5: each is still refused, and matching them needs `long double` or bash's whole parameter grammar |
| Declined to judge: the pipes' timing on the NUC; a NUC clock behind the host; Ctrl-C ending a foreground script's nested interactive shell with its command | The transcript's lines do not depend on timing; the clock is UTC, as the stick's times show (§16 item 7); the third is §16 item 4's choice, unchanged here |

## Where this plan fits

Plan 4 of milestone 3 implements spec §13 step 9, the last step of the user-space gate. It builds on plans 1–3 (pipes, jobs, `ps` and `kill`, script arguments and variables), none of which had a NUC check: the stick still holds 0.3.0, and the NUC's recorded transcripts say `ABI 3` by hand. The aarch64 port and the ABI's notes for other architectures are left for after the gate (spec §15).

## Working conventions

- Plan 4 lands as **four pull requests** (table below). This plan, with the spec's §16 item 11 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR, binding it to the session and waiting for green CI. The user reviews and merges every PR; never merge yourself. PR 4 needs NUC checks 3, 4 and 5: it goes up as a draft, and after the user reports a pass, the real transcripts and the results-log row go into a commit of the same PR, which is then marked ready. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-02-m3-plan-4-hardening-and-0-4-0.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 3, 14, 16 and 18 have no failing run: the first changes no behaviour, the second strengthens a check script (a mutation check is its evidence), the others document only; their introductions say why. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m3p4/mutations.md`).
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; a console read, a pipe or a `wait` can wait for ever. Every loop here has a bound (`seq`'s comparison with GNU's stops a zero increment after 10 000 lines). Mutation checks run under an address-space limit and a timeout (`tmp/m3p4/mutate.py`).
- **The expectations are bash 5.2's and GNU's.** `bash FILE a b`, `bash -c '…' x a b`, an interactive bash in a pty (`tmp/m3p4/pty_bash.py`), and the host's GNU `seq` and `grep` through `testing::host_tool`; what they printed is in `tmp/m3p4/progress.md` and `tmp/m3p4/bash/`. Where this shell departs, the test or the decision says so.
- **The NUC check scripts are tests too.** `check3-a.sh`'s `uname -a` line changes with the version (Task 15), `check5.sh` is new (Task 13), and the `checks` scenario runs them all against their recorded QEMU transcripts.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests and scenarios that come with the change belong to its commit; a task that only adds tests names the module whose files it changes. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `47b549e` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request, NUC and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m3p4/plan` | — | The spec's §16 item 11 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m3p4/shell` | 1–7 | `$[`; quotes inside `${…}`; a value borrowed; `cd ""`; `seq`'s zero increment; `[a-a-`; the stray prompt | `lint`, `unit`, `e2e` |
| 3 | `m3p4/xtask` | 8–12 | `verify-usb`'s times and an unreadable `system.img`; raw strings in the loader's scanner; `invpcid`, in any case, in the TLB scan | `lint`, `unit`, `e2e` |
| 4 | `m3p4/release` | 13–18 | `check5.sh`; version 0.4.0; the README, `docs/hardware-test.md` and the stick's README, and the review's fixes to them; NUC checks 3, 4 and 5 (draft until they pass) | `lint`, `unit`, `e2e`; NUC checks 3, 4 and 5 |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 4 adds no crate. The kernel has no dev-dependencies and does not depend on `shell`; `crates/usb` has no dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail, and keeps the same values on every architecture. Plan 4 changes neither the kernel's behaviour nor the ABI: `relay_abi::VERSION` stays 3.
- Every command prints exactly what it prints in milestone 1 for files, and every milestone 1 scenario passes unchanged but for the startup line and the version (gate §1.4).
- What a person types, and everything programs pass to the kernel, is untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the shell or the kernel panic, index out of bounds, overflow, allocate without bound or loop forever. `/bin/sh`'s heap ends the program with 134 when it runs out. Nothing waits for ever but the error screen and a program waiting for its own input. Panics are for bugs only.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 120×33 cells and no serial port.
- Shell messages follow bash 5.2 and GNU coreutils; `/bin/sh` names itself `relay-sh`, a script's own messages say `sh:`; unsupported features are `relay-sh: unsupported syntax: <what>` (status 2).
- Missing tools fail tests, never skip them; a test that needs a host tool needs one CI's workflow installs (xtask's tests use `debugfs`, `mke2fs` and mtools, which the unit job has).
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- A plan that changes a check script ends with `cargo xtask flash --full` for its NUC check, and the recorded NUC transcripts in `xtask/fixtures/checks/` are replaced by the real ones copied off the stick, in a commit of the plan's last PR, with the results-log row.
- No tag or GitHub release without the user's word: PR 4 proposes the text of `v0.4.0` and its release (below).

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 11 in PR 1:

1. **Plan 4 is one plan** (§13) in four pull requests: this plan; the shell's deferred fixes; xtask's and the source scans' deferred fixes; `check5.sh`, version 0.4.0 and the docs, with NUC checks 3, 4 and 5 on a stick written by `flash --full`. It ends milestone 3 and the user-space gate.
2. **`check5.sh`** (§12.4, corrected in its body). 29 commands, run after `check4.sh` and before check 4's `exit` by hand, whose lines are the same on the NUC and in QEMU: pipes (`seq 1000000 | wc -c`, 6.9 MB through a 16 KiB pipe; `seq 1000000 | head -n 1`, a reader that ends first; `seq 1000 | grep 7 | wc -l`; a pipeline's status; `X | sh`), a background job (`t-spin &`, `jobs`, `ps | grep -c t-spin`, `kill %1`, `wait %1` and its 137), and a script given an empty argument and one with two blanks that passes `"$@"` on to another (`$0`, `$#`, `cd "$9"` away from home, a value with blanks never split, an unquoted empty one no word, `$?`). A script prints no `[1] <pid>`, no Done line and no line for a job a signal ended, and the kernel's `killed` lines go to its log only, so nothing in the transcript races the prompt; `ps` is read through `grep -c`, whose count holds no pid or time. It needs no file of the earlier checks. The `checks` scenario runs it after `check4.sh`, its QEMU transcript is recorded, and tests check that every check script on the stick has both transcripts and that the README and `docs/hardware-test.md` type it (the prototype's review found the README's quick start running three).
3. **A script says nothing about its jobs** (§9.2; item 9 corrected). Item 9 said a script and `X | sh` report no job "as bash's non-interactive shells do". That holds for `[1] <pid>` and the Done lines, but bash 5.2's script does say how a job a signal ended (`s.sh: line 3: 2805657 Killed  sleep 100`, on standard error, when it collects the job). This shell stays silent there too (the user chose): the line would come wherever the job is collected, with its pid, so a transcript could not expect it.
4. **The shell's deferred findings** (§9.4; items 8–10). `$[`, bash's old arithmetic (`$[1+1]` is 2 there), is `unsupported syntax: $[`, quoted or not, rather than text. Inside `${…}`, quotes, escapes and further `${…}` are read whole while the `}` is looked for, as bash reads them: `${A"}"}` and `${A\}}` are one bad substitution, `${A:-${B}}` one refusal, and a quote left open there (`"${A"`) is this shell's `syntax error: unterminated quote` (bash: matching `` `"' ``) rather than a missing `}`. A parameter's value is borrowed, not copied, before its room is taken, and `$@` is no value of its own. `cd ""`, and so a script's `cd "$1"` without an argument, stays where it is with status 0, as bash's does (`cd $E` unquoted is `cd` alone). `seq` refuses a zero increment as soon as it has read it, in any form (`0.0`, `-0`, `0x0`), and an operand that is no number before one that is not whole, in GNU's order (`seq 1 0 x`: `invalid Zero increment value: '0'`). A `-` after a range at a pattern's end is `Invalid range end` (`[a-a-`), as GNU grep 3.11 says. An interactive `/bin/sh` whose group was never given the console ends before its prompt whether it leads a group (`sh &`) or is in a background script's, where it printed one stray prompt: it asks first to set the console's mode, which only a group holding the console may.
5. **Two findings ruled out** (the user agreed). bash expands a `~` after the `=` of an argument shaped like an assignment, and after a `:` in it (`echo A=~/x` prints `A=/root/x`); bash's POSIX mode and milestone 1 do not, nor does this shell, as item 10 now says. bash's job lines add `(wd: DIR)` when a job's directory is not the shell's, and its notices a line `(wd now: DIR)`: that needs each job's directory and a second line, for a job that outlives a `cd`, and is left out. Of the prototype's review's nits, two are left too, each still refused as bash refuses it: an increment GNU's `long double` takes as zero (`seq 1 1e-5000 5`: `not a whole number` here, `invalid Zero increment value` there), and a few rare `${…}` (`${$${A}`, ``${A`}`}``) that get another error than bash's.
6. **xtask's and the scans' deferred findings** (§10; item 7). `verify-usb` shows a transcript's time as `date` shows one, as it shows `system.img`'s build time (`(run Mon Sep 28 14:09:17 UTC 2026)`), and fails when the stick's `system.img` cannot be read or is no archive (`system.img: FAILED, …`, the check's first line), since no transcript can then be compared with it. The loader-rules test reads raw strings as Rust does (`r#"…"#`), so its scanner no longer assumes there are none; the scan that keeps the TLB in `arch/` sees `invpcid` too, and either instruction in any case (`mm::init`'s CR3 write stays, as item 7 says).
7. **§8.5's table** (corrected in its body): `t-spawn ctrl-c-apart` and `t-tee owners`, and among the kinds left out those only a program's children run (`t-pipe drain`, `hold` and `flood`, `t-proc kill-grandparent`, `t-read refused-child`, `t-spawn child`, `nap`, `doze` and `ctrl-c-child`, `t-tee owner`).
8. **Version 0.4.0** (§1.4, §2). The workspace's version, `uname -a` in `shell` (milestone 1's scenario, changed only by the version) and in `check3-a.sh`, and the recorded transcripts' version lines. A spike bumped it first and ran every scenario with a draft `check5.sh`: nothing else depends on it. The README says the gate is done; the stick's `/root/README` names the built-ins as built-ins and shows a pipeline, a job, a script's arguments and a variable. The error screen's visit by hand stays in check 4.
9. **Milestone 3's definition of done** (§1.4). Item 1: `cargo xtask ci` runs 47 scenarios, milestone 1's unchanged but for the startup lines and `shell`'s version, with §12.3's milestone 3 scenarios (`pipe_calls`, `pipes`, `proc_calls`, `screen_console`, `jobs`, `script_vars`). Item 2: NUC checks 3, 4 and 5 in plan 4's last pull request, their transcripts checked by `verify-usb`. Item 3: an xtask test walks the resolved graph (since plan 4b), and every command but the six built-ins runs from `/bin`. Nothing is unmet.
10. **Earlier deferred findings, settled.** Milestone 2's plan 5's minors 3–7 and the deferred findings of plans 1–3 (above); `|&`, `X | sh`'s reads and `pipe()`'s inserts were settled by plans 2 and 3. What stays out of the gate is §15's list.

## Review Focus

# Review Focus (m3-plan-4)

1. **NUC check 5 and `verify-usb` on a real stick:** `check5.sh` run after `check4.sh` (its `t-spawn fill` orphans napping, its orphan `t-spin 1`), run twice, typed on the K120 rather than over serial, its transcript left by a later `flash --kernel`, and a stick whose `system.img` is missing or damaged. Expected: 29 of 29 commands on both machines with the same lines, `jobs` and `ps | grep -c t-spin` counting only the job's `t-spin`, `wait %1` 137, no line racing the prompt; `verify-usb` lists `system.img: built …` first and fails a stale transcript or an unreadable `system.img`. Tests: `the_check_scripts_pass_on_both_machines` and the scenario `checks` (Tasks 13 and 14), `the_nuc_s_instructions_run_every_check_script` (Task 17), `a_system_img_that_cannot_be_read_fails_the_check` and `a_transcript_older_than_the_system_on_the_stick_fails_the_check` (Task 9), `transcripts_of_the_check_scripts_are_checked_as_on_the_nuc` (Task 8).
2. **Quoting a person types around `$`:** `"${A"`, `${A"}"}`, `${A\}}`, `${A'}'}`, `${A:-${B}}`, `${A"\` at a line's end, `$[1+1]` quoted or not, `a$[`, in scripts and at the prompt, with multi-byte characters. Expected: bash's `bad substitution`, an unterminated quote or a missing `}` as bash sees them, `unsupported syntax: $[`; nothing passed on as text, no panic. Tests: `quotes_and_escapes_inside_a_substitution_are_skipped_as_bash_does` and `a_brace_without_its_end_is_a_syntax_error` (Task 2), `bash_s_other_parameters_and_operators_are_unsupported` (Task 1).
3. **A script's `cd` given what its caller did not pass:** `cd "$1"` without arguments, `cd ''`, `cd "$E"`, `cd $E`, `cd '' ''`, under both runners. Expected: an empty directory leaves `cd` where it is with 0, no word goes home, two words are too many. Tests: `cd_to_an_empty_directory_stays_where_it_is` (Task 4), the scenario `checks`, whose nested script runs `cd "$9"` away from home (Task 14).
4. **An interactive shell with no console to read:** `sh &`, a background script holding `sh`, a nested `sh` typed at the prompt or run by a foreground script, `X | sh`. Expected: the first two end before any prompt and their jobs are done; the others prompt and take the console as before, Ctrl-C reaching their commands. Tests: the scenarios `sh` (Task 7), `console`, `jobs`, `ctrlc` and `script_vars`.
5. **`seq` and `grep` given what GNU refuses:** a zero increment in any form (`0.0`, `-0`, ` +0`, `0x0`, `0e5`) before a bad last operand, a decimal before a bad one, a zero first or last operand; a `-` after a range at a pattern's end. Expected: GNU's first line and status for each, and nothing printed for ever. Tests: `seq_refuses_what_gnu_seq_refuses_as_it_does`, `seq_prints_what_gnu_seq_prints` and `what_seq_refuses` (Task 5), `dashes_in_a_set_are_what_gnu_grep_makes_of_them` (Task 6).

## Release notes (proposed for `v0.4.0`)

To be proposed to the user after PR 4's NUC checks pass, as the GitHub release "Relay OS 0.4.0 — milestone 3"; neither the tag nor the release is made without their word.

> Milestone 3 of Relay OS, an operating system written from scratch in Rust, and the end of its user-space gate: the programs of milestone 2 are connected. Commands are joined by pipes, run in the background, listed and killed, and scripts take arguments and variables.
>
> **What it does**
>
> - **Pipes:** `a | b | c` in one process group, through a 16 KiB pipe in frames of its own that blocks both ways; `cat`, `wc` (with `-c`, `-l`, `-w`), `head` and `tail` read standard input; new programs `grep` (literal characters, `.`, `*`, `^`, `$`, sets; `-i`, `-v`, `-n`, `-c`), `seq`, `sleep`, `true` and `false`; `X | sh` runs the lines it reads. A child may join another child's group (ABI 3).
> - **Jobs:** `cmd &` and `a | b &` run in the background with bash's job lines; the built-ins `jobs`, `wait` and `kill` (`%n`, a pid, a whole job); `/bin/ps` shows each process's state, memory and CPU time; a Ctrl-C at the prompt ends a `wait`. Only the group that holds the console may change it.
> - **Scripts and variables:** `sh FILE a b` gives a script `$0`, `$1` on, `$#`, `$@`; `NAME=value`, `$NAME`, `${NAME}` and `$?` at the prompt and in scripts; a value is never split into words (unlike bash); what bash does that this shell does not is refused as unsupported syntax.
>
> **Tested**
>
> - `cargo xtask test`: host unit tests and 47 QEMU scenarios under KVM, each followed by `e2fsck`.
> - NUC checks 3, 4 and 5 (`docs/hardware-test.md`) passed on the NUC (on the date of the results-log row PR 4 adds), on a stick written by `cargo xtask flash --full`: four scripts of 84, 5, 33 and 29 commands, the steps by hand, and the error screen. `cargo xtask verify-usb` checked their transcripts.
>
> **Try it**
>
> See the quick start in `README.md`. At the prompt: `seq 1000 | grep 7 | wc -l`, `t-spin &`, `jobs`, `ps`, `kill %1`.
>
> Design: `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (milestones 2 and 3); milestone 3's four plans are in `docs/superpowers/plans/`.

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/shell/src/parser.rs` | `$[` refused; `brace_text`: a `${…}`'s text read as bash reads it |
| `crates/shell/src/expand.rs` | `Value`: a parameter's value borrowed, `$@` a word per argument |
| `crates/shell/src/commands/basic.rs` | `cd ""` stays; `uname -a` at 0.4.0 |
| `crates/shell/src/commands/seq.rs` | GNU's refusals first, `is_zero` |
| `crates/shell/src/pattern.rs` | A `-` after a range at the pattern's end |
| `userland/sh/src/main.rs` | `console_mode` before the first prompt |
| `xtask/src/flash.rs`, `xtask/src/image.rs` | `verify-usb`'s times, `system.img` first and failing; `blank_esp` for tests |
| `boot/src/lib.rs`, `kernel/src/arch/tlb.rs` | Raw strings in the loader's scanner; `invpcid` in the TLB scan |
| `rootfs/root/checks/check5.sh` (new), `xtask/fixtures/checks/check5.{qemu,nuc}.log` (new), `xtask/src/checks.rs`, `tests/e2e/checks.txt` | NUC check 5, its transcripts; every script has its two transcripts and is run by the README and `docs/hardware-test.md` |
| `tests/e2e/sh.txt`, `tests/e2e/shell.txt` | The stray prompt; `uname -a` at 0.4.0 |
| `Cargo.toml`, `Cargo.lock`, `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/check3-*.log`, `crates/relay-abi/src/info.rs` | Version 0.4.0 |
| `README.md`, `docs/hardware-test.md`, `rootfs/root/README` | Milestone 3 done; check 5; the stick's README |

---

## PR 1: The spec's revisions, the roadmap's notes and this plan

The spec's §16 item 11 with the parts of its body it corrects (the status line, §8.5's test programs, §12.4's check 5, item 9's words on a script's jobs, item 10's on `~`); the roadmap's notes on plan 3's deferred minors and on plan 4; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record plan 4's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p4/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m3p4/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-02-m3-plan-4-hardening-and-0-4-0.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-02-m3-plan-4-hardening-and-0-4-0.md
git commit -m "docs(plan): add milestone 3 plan 4, hardening and 0.4.0"
cargo xtask lint
git push -u origin m3p4/plan
gh pr create --base main --head m3p4/plan --title "docs(spec,plan): add milestone 3's plan 4, hardening and 0.4.0" --body-file - <<'EOF2'
## What

The implementation plan of milestone 3's plan 4 ("hardening and 0.4.0"), the last of the user-space gate, and the spec's §16 item 11 with its decisions: `check5.sh` (pipes, a background job and `kill`, script arguments and variables, the same lines on the NUC and in QEMU); a script silent about its jobs; each finding deferred by plans 1–3 and milestone 2's plan 5, fixed or ruled out; version 0.4.0; milestone 3's definition of done; the corrections to the spec's body they bring (§8.5, §12.4, items 9 and 10). The roadmap places plan 3's deferred minors and says what plan 4 ends with.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only (the plan's last pull request runs NUC checks 3, 4 and 5)
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (`bind_pr`); its monitor reports CI, which is never polled. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m3p4/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-plan` (its `.superpowers/sdd` and `.superpowers/.gitignore` removed by name first, if made) and continue with PR 2.

---

## PR 2: The shell's deferred fixes (Tasks 1–7)

What the reviews of plans 1–3 left for plan 4 in the shell and `/bin/sh`: `$[` refused; quotes and escapes inside `${…}` read as bash reads them; a parameter's value borrowed before its room is taken; `cd ""` staying where it is; `seq`'s zero increment refused as GNU's is; a `-` after a range at a pattern's end; and an interactive `sh` a background script starts ending before its prompt.

Branch `m3p4/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p4/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-shell m3p4/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p4/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: `$[`, bash's old arithmetic, is unsupported syntax

Decision 4 (plan 3's final review): `$[1+1]` is bash's old form of arithmetic expansion (2 in bash), and the parser kept its `$` as text, so `echo $[1+1]` printed `$[1+1]`, against the gate's rule that a feature this shell lacks is refused rather than passed on as text. A `$[`, quoted or not, is now `unsupported syntax: $[` (status 2), as `$((` is; `'$['` and `\$[` stay text. The red run is the shell's tests: the refusal test gets `Ok` for `echo $[1+1]`. Mutation check: the new arm removed fails the refusal test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: plan 3's `parser::parameter`, `ParseError::Unsupported`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
            ("echo $'a'", "$'"),
            ("echo $\"a\"", "$\""),
````

with:

````rust
            ("echo $'a'", "$'"),
            // bash's old arithmetic, `$[1+1]` (2 in bash), quoted or not.
            ("echo $[1+1]", "$["),
            ("echo \"$[1+1]\"", "$["),
            ("echo a$[", "$["),
            ("echo $\"a\"", "$\""),
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::bash_s_other_parameters_and_operators_are_unsupported`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! `$!`, `$-`, `$_`), its operators (`${A:-x}`, `${#A}`) and, outside
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`
//! and `$((`; a `${` without its `}` is bash's syntax error, and a `${…}`
//! that names nothing expands to its `bad substitution`. A `$` before
````

with:

````rust
//! `$!`, `$-`, `$_`), its operators (`${A:-x}`, `${#A}`) and, outside
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`,
//! `$((` and `$[`; a `${` without its `}` is bash's syntax error, and a `${…}`
//! that names nothing expands to its `bad substitution`. A `$` before
````

Replace:

````rust
        }
        // `$'…'` and `$"…"` are bash's quotes of other kinds.
````

with:

````rust
        }
        // bash's old arithmetic, `$[1+1]`.
        '[' => return Err(ParseError::Unsupported("$[".into())),
        // `$'…'` and `$"…"` are bash's quotes of other kinds.
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 262 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse bash's old arithmetic $[ as unsupported syntax

`$[1+1]` is bash's old form of arithmetic expansion (2 in bash). The
parser kept its `$` as text, so `echo $[1+1]` printed `$[1+1]` where
bash prints `2`, against the gate's rule that a feature this shell
lacks is refused rather than passed on as text. A `$[`, quoted or not,
is now `unsupported syntax: $[` (status 2), as `$((` is.

Plan 3's final review found it (deferred to plan 4).
EOF
````


### Task 2: Quotes and escapes inside `${…}` are read as bash reads them

Decision 4 (plan 3's final review): the parser took a `${…}` to end at the first `}`, whatever came before it, while bash reads quotes, escapes and further `${…}` inside it whole as it looks for the `}`. So `echo "${A"` said a `}` was missing where bash says a quote is (matching `` `"' ``), `${A"}"}` and `${A\}}` were cut at their first `}` where bash calls each one bad substitution, and `${A:-${B}}` was refused in part. `brace_text` now reads a `${…}`'s text as bash does: a `'…'` to its closing quote, a `"…"` to its closing quote with `\` escaping the next character, a `\` with the next character, and a `${` counted until its `}`; a quote left open there is this shell's `syntax error: unterminated quote` (status 2), as any other open quote is (this shell has never used bash's ``unexpected EOF while looking for matching `"'`` wording), and a missing `}` is bash's syntax error as before. The test that pinned `"${A"` to a missing `}` changes with it. The red runs are the shell's tests: `"${A"` still says a `}` is missing, and `${A\}` is read as a bad substitution instead. Mutation checks: the quote arm, the escape arm or the nested `${` arm removed, `\` escaping inside single quotes, no escape inside double quotes, an open quote reported as a missing `}` (in either place) and a nested `}` not counted each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: plan 3's `parser::braced`, `Cursor`, `ParseError::{UnclosedBrace, UnterminatedQuote}`.
- Produces: `parser::brace_text(&mut Cursor) -> Result<String, ParseError>` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn a_brace_without_its_end_is_a_syntax_error() {
        for line in ["echo ${A", "echo \"${A\"", "echo ${"] {
            let e = one(line).unwrap_err();
````

with:

````rust
    fn a_brace_without_its_end_is_a_syntax_error() {
        // A `}` quoted or escaped inside does not end it, nor does the
        // first `}` after a `${` inside it, as in bash.
        for line in [
            "echo ${A",
            "echo \"${A",
            "echo ${",
            "echo ${A\\",
            "echo ${A\\}",
            "echo ${A\"}\"",
            "echo ${A\"\\\"\"",
            "echo ${A:-${B}",
        ] {
            let e = one(line).unwrap_err();
````

Replace:

````rust
                "syntax error: unexpected EOF while looking for matching `}'"
            );
        }
    }

    /// `word`'s assignment, the value's pieces joined.
````

with:

````rust
                "syntax error: unexpected EOF while looking for matching `}'"
            );
        }
    }

    #[test]
    fn quotes_and_escapes_inside_a_substitution_are_skipped_as_bash_does() {
        // Inside `${…}` bash reads quotes, escapes and further `${…}`
        // whole while it looks for the `}`, so a quote left open there is
        // unterminated (bash: ``unexpected EOF while looking for matching
        // `"'``; this shell keeps its own words for an open quote).
        for line in [
            "echo \"${A\"",
            "echo ${A\"",
            "echo \"${A'",
            "echo ${A'}",
            "echo ${A\"x",
            "echo ${A\"\\",
        ] {
            assert_eq!(one(line), Err(ParseError::UnterminatedQuote), "{line}");
        }
        // What it reads is the substitution's text, and bash's `bad
        // substitution` or this shell's refusal names it whole.
        let bad = |t: &str, q: bool| vec![Piece::Param(Param::Bad(t.into()), q)];
        assert_eq!(
            pieces(r#"${A"}"} ${A\}} ${A'\'} "${A"\""}""#),
            [
                bad(r#"${A"}"}"#, false),
                bad(r"${A\}}", false),
                bad(r"${A'\'}", false),
                bad(r#"${A"\""}"#, true)
            ]
        );
        assert_eq!(
            one("echo ${A:-${B}}"),
            Err(ParseError::Unsupported("${A:-${B}}".into()))
        );
        assert_eq!(
            one("echo ${A:-\"x}\"}"),
            Err(ParseError::Unsupported("${A:-\"x}\"}".into()))
        );
    }

    /// `word`'s assignment, the value's pieces joined.
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::a_brace_without_its_end_is_a_syntax_error`, `parser::tests::quotes_and_escapes_inside_a_substitution_are_skipped_as_bash_does`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`,
//! `$((` and `$[`; a `${` without its `}` is bash's syntax error, and a `${…}`
//! that names nothing expands to its `bad substitution`. A `$` before
//! anything else is a `$`.
//!
````

with:

````rust
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`,
//! `$((` and `$[`; a `${` without its `}` is bash's syntax error (quotes,
//! escapes and `${…}` inside it are read whole, as bash reads them), and a
//! `${…}` that names nothing expands to its `bad substitution`. A `$`
//! before anything else is a `$`.
//!
````

Replace:

````rust

/// A name starts with a letter or `_`.
````

with:

````rust

/// The text of a `${…}` up to its `}`, which it takes. As bash does, it
/// reads quotes, escapes and further `${…}` whole, so a `}` among them
/// does not end it; a quote left open is unterminated.
fn brace_text(cur: &mut Cursor<'_>) -> Result<String, ParseError> {
    let mut text = String::new();
    // The `${` opened inside and not yet closed.
    let mut open = 0usize;
    loop {
        let c = cur.next().ok_or(ParseError::UnclosedBrace)?;
        if c == '}' && open == 0 {
            return Ok(text);
        }
        text.push(c);
        match c {
            '}' => open -= 1,
            '$' if cur.next_if_eq('{') => {
                text.push('{');
                open += 1;
            }
            '\\' => text.push(cur.next().ok_or(ParseError::UnclosedBrace)?),
            '\'' | '"' => loop {
                let q = cur.next().ok_or(ParseError::UnterminatedQuote)?;
                text.push(q);
                if q == c {
                    break;
                }
                if q == '\\' && c == '"' {
                    text.push(cur.next().ok_or(ParseError::UnterminatedQuote)?);
                }
            },
            _ => {}
        }
    }
}

/// A name starts with a letter or `_`.
````

Replace:

````rust
fn braced(cur: &mut Cursor<'_>) -> Result<Param, ParseError> {
    let mut inside = String::new();
    loop {
        match cur.next() {
            Some('}') => break,
            Some(c) => inside.push(c),
            None => return Err(ParseError::UnclosedBrace),
        }
    }
    let typed = format!("${{{inside}}}");
````

with:

````rust
fn braced(cur: &mut Cursor<'_>) -> Result<Param, ParseError> {
    let inside = brace_text(cur)?;
    let typed = format!("${{{inside}}}");
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 263 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read quotes and escapes inside ${…} as bash does

The parser took a `${…}` to end at the first `}`, whatever came before
it. bash reads quotes, escapes and further `${…}` inside it whole while
it looks for the `}`, so its errors differ: `echo "${A"` leaves a quote
open (bash: matching `"`), where this shell said a `}` was missing;
`${A"}"}` and `${A\}}` are one bad substitution, where this shell cut
them at the first `}`; and `${A:-${B}}` is refused whole. The text of
a `${…}` is now read as bash reads it, and a quote left open in it is
this shell's `syntax error: unterminated quote` (status 2), as any
other open quote is.

Plan 3's final review found it (deferred to plan 4).
EOF
````


### Task 3: A parameter's value is borrowed before its room is taken

Decision 4 (plan 3's final review): the expander copied a parameter's value (up to 64 KiB, a variable or an argument) into a new string before taking its room from what the line may still expand to, so a line already at the limit made a copy it then refused; its `$@` arm, which joined the arguments, could not be reached, since `$@` is expanded an argument at a time before it got there. A parameter now gives a `Value`: text borrowed from the variables where it is theirs (`Value::One`), or `$@`'s arguments (`Value::Args`), which the word's loop expands a word each, so no arm is left over and the room is taken before anything is copied. Nothing a line expands to changes, so the task has no failing run: the existing tests of expansion and its limits pass as they are, and the mutation checks show they still guard the rewritten loop (`$@`'s count for each argument removed, `$@` taken from the second argument, the room never taken: each fails a test).

**Files:**
- Modify: `crates/shell/src/expand.rs`

**Interfaces:**
- Consumes: plan 3's `expand::Expander`, `Field`.
- Produces: `expand::Value<'v>::{One(Cow<'v, str>), Args(&'v [String])}` (private); `Expander::value(&self, &Param) -> Result<Value<'v>, Error>`.

- [ ] **Step 1: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::parser::{Command, Line, Param, Piece, Redirect, Word};
use alloc::collections::BTreeMap;
````

with:

````rust
use crate::parser::{Command, Line, Param, Piece, Redirect, Word};
use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
````

Replace:

````rust
        for piece in &word.pieces {
            match piece {
                Piece::Param(Param::All, quoted) => {
                    let Some((first, rest)) = self.vars.args[1..].split_first() else {
                        continue;
````

with:

````rust
        for piece in &word.pieces {
            let (value, quoted) = match piece {
                Piece::Text(t, quoted) => (Value::One(Cow::Borrowed(t.as_str())), quoted),
                Piece::Param(p, quoted) => (self.value(p)?, quoted),
            };
            match value {
                Value::One(text) => self.push(&mut fields, &text, *quoted)?,
                Value::Args(args) => {
                    let Some((first, rest)) = args.split_first() else {
                        continue;
````

Replace:

````rust
                    }
                }
                Piece::Text(t, quoted) => self.push(&mut fields, t, *quoted)?,
                Piece::Param(p, quoted) => {
                    let value = self.value(p)?;
                    self.push(&mut fields, &value, *quoted)?;
                }
````

with:

````rust
                    }
                }
````

Replace:

````rust

    /// A parameter's value (`$@`'s joined by blanks).
    fn value(&self, p: &Param) -> Result<String, Error> {
        let args = &self.vars.args;
        Ok(match p {
            Param::Name(n) => String::from(self.vars.get(n)),
            Param::Arg(i) => args.get(*i).cloned().unwrap_or_default(),
            Param::Count => (args.len() - 1).to_string(),
            Param::Status => self.status.to_string(),
            Param::All => args[1..].join(" "),
            Param::Bad(t) => return Err(Error::BadSubstitution(t.clone())),
        })
    }
}
````

with:

````rust

    /// A parameter's value, borrowed from the variables where it is
    /// theirs, so that its room is taken before any of it is copied.
    fn value(&self, p: &Param) -> Result<Value<'v>, Error> {
        let args = &self.vars.args;
        Ok(match p {
            Param::Name(n) => Value::One(Cow::Borrowed(self.vars.get(n))),
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
            Param::Count => Value::One(Cow::Owned((args.len() - 1).to_string())),
            Param::Status => Value::One(Cow::Owned(self.status.to_string())),
            Param::All => Value::Args(&args[1..]),
            Param::Bad(t) => return Err(Error::BadSubstitution(t.clone())),
        })
    }
}

/// What a piece of a word gives: text, or `$@`'s arguments, a word each.
enum Value<'v> {
    One(Cow<'v, str>),
    Args(&'v [String]),
}
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 263 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell): borrow a parameter's value before taking its room

The expander copied a parameter's value (up to 64 KiB, a variable or an
argument) into a new string before taking its room from what a line may
still expand to, so a line already at the limit made a copy that it
then refused. Its `$@` arm, which joined the arguments, was unreachable:
`$@` is expanded an argument at a time before it got there. A
parameter's value is now borrowed from the variables where it is
theirs (`Value::One`), and `$@` gives its arguments (`Value::Args`),
which the word's loop expands a word each, so no arm is left over and
the room is taken before anything is copied. Nothing a line expands to
changes.

Plan 3's final review found it (deferred to plan 4).
EOF
````


### Task 4: `cd ""` stays where it is

Decision 4 (plan 3's final review): `cd ""`, and so a script's `cd "$1"` run without an argument, said `relay-sh: cd: : No such file or directory` (status 1), where bash's does nothing and returns 0. It now stays in its directory with status 0, under both runners; an unquoted `cd $E` that expands to no word still goes home, as `cd` alone does, and two empty arguments are still too many, as in bash. `check5.sh` runs a script's `cd "$9"` on the NUC (Task 13). The red run is the shell's tests: `cd ""` prints the error. Mutation check: the new arm removed fails the test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`

**Interfaces:**
- Consumes: milestone 1's `commands::basic::cd`; plan 3's `Harness::lines`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    #[test]
    fn cd_errors() {
````

with:

````rust
    #[test]
    fn cd_to_an_empty_directory_stays_where_it_is() {
        // As bash's does (status 0), so a script's `cd "$1"` without an
        // argument does nothing; `cd $E` unquoted gets no word and goes
        // home, as `cd` alone.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["cd /etc", "cd \"\"", "pwd", "cd \"$1\"", "pwd"]),
            (0, "/etc\n/etc\n".into())
        );
        assert_eq!(h.lines(&["cd /etc", "E=", "cd $E", "pwd"]).1, "/root\n");
        assert_eq!(
            h.run("cd '' ''"),
            (1, "relay-sh: cd: too many arguments\n".into())
        );
    }

    #[test]
    fn cd_errors() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::cd_to_an_empty_directory_stays_where_it_is`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust

/// `cd [dir]`: no argument goes to `/root`. `cd -` is not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let dir = match args {
        [] => HOME,
        [dir] if dir == "-" => {
````

with:

````rust

/// `cd [dir]`: no argument goes to `/root`, an empty one nowhere (as in
/// bash). `cd -` is not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let dir = match args {
        [] => HOME,
        [dir] if dir.is_empty() => return 0,
        [dir] if dir == "-" => {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 264 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): leave cd where it is when its directory is empty

`cd ""`, and so a script's `cd "$1"` run without an argument, said
`relay-sh: cd: : No such file or directory` (status 1), where bash's
does nothing and returns 0. It now stays in the directory it is in,
with status 0, under both runners. An unquoted `cd $E` that expands to
no word still goes home, as `cd` alone does, and two empty arguments
are still too many.

Plan 3's final review found it (deferred to plan 4); check5.sh runs a
script's `cd "$9"` on the NUC.
EOF
````


### Task 5: `seq` refuses a zero increment as soon as it has read it

Decision 4 (plan 1's final review): `seq` checked every operand before it looked at the increment, and asked for a whole number before it asked for a number, so `seq 1 0 x` said `invalid floating point argument: 'x'` and `seq 1 0 1.5` `not a whole number: '1.5'`, where GNU's says `invalid Zero increment value: '0'` for both: it refuses a zero increment as soon as it has read it. GNU's refusals now come first, in GNU's order (each operand a number, the increment of three not zero in any form, `is_zero`), and only then this seq's own refusal of a number that is not whole, so `seq 1.5 x` says what GNU's does too. The test that compares refusals with the host's GNU seq gains these cases and a Ctrl-C after 10 000 lines, since a zero increment taken would print for ever; the printing test gains a zero first and last operand. The red run is the shell's tests: `seq 1 0 x` gets the old first line. Mutation checks: the zero check removed, made for any operand or for two operands, or reading a hexadecimal `e` as an exponent, and the exponent kept in the mantissa, each fail a test (three of them survived until the printing and refusal tests gained `seq 0 2 6`, `seq -2 0` and `seq 1 0x0e 5`).

**Files:**
- Modify: `crates/shell/src/commands/seq.rs`

**Interfaces:**
- Consumes: plan 1's `commands::seq::{seq, is_number, is_nan, whole}`; `testing::host_tool`, `Harness::like_host`.
- Produces: `commands::seq::is_zero(&str) -> bool` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/seq.rs`**

In `crates/shell/src/commands/seq.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            &["seq", "10", "-3", "1"],
            &["seq", "0"],
````

with:

````rust
            &["seq", "10", "-3", "1"],
            // Only the increment may not be zero.
            &["seq", "0", "2", "6"],
            &["seq", "3", "-1", "0"],
            &["seq", "-2", "0"],
            &["seq", "0"],
````

Replace:

````rust
            ("seq 0x10", "seq: not a whole number: '0x10'\n"),
            // GNU's options are not this seq's.
````

with:

````rust
            ("seq 0x10", "seq: not a whole number: '0x10'\n"),
            // A hexadecimal increment's `e` is a digit (14), not zero.
            ("seq 1 0x0e 5", "seq: not a whole number: '0x0e'\n"),
            // GNU's options are not this seq's.
````

Replace:

````rust
            &["seq", "nan"],
        ] {
            let mut h = Harness::new();
            let (status, out, err) = host_tool(args, &[], b"");
````

with:

````rust
            &["seq", "nan"],
            // GNU refuses a zero increment as soon as it has read it,
            // before the last operand, in any form a number takes.
            &["seq", "1", "0", "x"],
            &["seq", "1", "0", "1.5"],
            &["seq", "1", "0", "nan"],
            &["seq", "1.5", "0", "x"],
            &["seq", "0x10", "0", "x"],
            &["seq", "1", "0.0", "5"],
            &["seq", "1", "-0", "5"],
            &["seq", "1", " +0", "5"],
            &["seq", "1", ".0", "5"],
            &["seq", "1", "0x0", "5"],
            &["seq", "1", "0x.0p3", "5"],
            &["seq", "1", "0e5", "x"],
            &["seq", "x", "0", "1"],
            &["seq", "nan", "0", "1"],
            // And every operand is a number before this seq asks for a
            // whole one.
            &["seq", "1.5", "x"],
            &["seq", "1e3", "nan"],
        ] {
            let mut h = Harness::new();
            // A zero increment taken would print for ever: a Ctrl-C ends
            // it, and the test fails.
            h.console.interrupt_after = Some(10_000);
            let (status, out, err) = host_tool(args, &[], b"");
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::seq::tests::seq_refuses_what_gnu_seq_refuses_as_it_does`.

- [ ] **Step 3: Change `crates/shell/src/commands/seq.rs`**

In `crates/shell/src/commands/seq.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    }
    let mut numbers = Vec::new();
    for op in ops {
        match whole(op) {
            Some(n) => numbers.push(n),
            None if is_nan(op) => {
                return ctx.fail(
                    "seq",
                    format_args!("invalid 'not-a-number' argument: {}", quote(op)),
                );
            }
            None if is_number(op) => {
                return ctx.fail("seq", format_args!("not a whole number: {}", quote(op)));
            }
            None => {
                let message = format_args!("invalid floating point argument: {}", quote(op));
                return ctx.fail("seq", message);
            }
````

with:

````rust
    }
    // GNU's refusals first, in its order: each operand must be a number,
    // and the increment of three not zero, which GNU checks as soon as it
    // has read it.
    for (i, op) in ops.iter().enumerate() {
        if is_nan(op) {
            let message = format_args!("invalid 'not-a-number' argument: {}", quote(op));
            return ctx.fail("seq", message);
        }
        if !is_number(op) {
            let message = format_args!("invalid floating point argument: {}", quote(op));
            return ctx.fail("seq", message);
        }
        if i == 1 && ops.len() == 3 && is_zero(op) {
            let message = format_args!("invalid Zero increment value: {}", quote(op));
            return ctx.fail("seq", message);
        }
    }
    // Then this seq's own: whole numbers only.
    let mut numbers = Vec::new();
    for op in ops {
        match whole(op) {
            Some(n) => numbers.push(n),
            None => {
                return ctx.fail("seq", format_args!("not a whole number: {}", quote(op)));
            }
````

Replace:

````rust
    };
    if step == 0 {
        let message = format_args!("invalid Zero increment value: {}", quote(&ops[1]));
        return ctx.fail("seq", message);
    }
    let mut n = first;
````

with:

````rust
    };
    let mut n = first;
````

Replace:

````rust
    mantissa_ok && exponent_ok
}
````

with:

````rust
    mantissa_ok && exponent_ok
}

/// Whether a number (one `is_number` takes) is zero: all its digits
/// before any exponent are `0`.
fn is_zero(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    let s = s.strip_prefix(['+', '-']).unwrap_or(s).to_ascii_lowercase();
    let (digits, exponent_mark) = match s.strip_prefix("0x") {
        Some(hex) => (hex, 'p'),
        None => (s.as_str(), 'e'),
    };
    let mantissa = digits.split(exponent_mark).next().unwrap_or("");
    mantissa.chars().all(|c| c == '0' || c == '.')
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 264 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse seq's zero increment as soon as it is read

seq checked every operand before it looked at the increment, and asked
for a whole number before it asked for a number, so `seq 1 0 x` said
`invalid floating point argument: 'x'` and `seq 1 0 1.5` `not a whole
number: '1.5'`, where GNU's seq says `invalid Zero increment value:
'0'` for both: it refuses a zero increment as soon as it has read it.
GNU's refusals now come first, in GNU's order (each operand a number,
the increment of three not zero in any form, `0.0`, `-0`, `0x0`,
`0e5`), and only then this seq's own refusal of a number that is not
whole, so `seq 1.5 x` also says what GNU's does.

The host test that compares refusals with GNU seq's gains these cases
and a bound: a zero increment taken would print for ever.

Plan 1's final review found it (deferred to plan 4).
EOF
````


### Task 6: A `-` after a range at a pattern's end is an invalid range end

Decision 4 (plan 1's final review): after a range, `grep`'s pattern refused a `-` that starts another range (`[a-z-9]`: `Invalid range end`) only when a character followed it, so at the pattern's end (`[a-a-`, `[a-z-`) the `-` was taken as a member and the missing `]` reported instead, `Unmatched [, [^, [:, [., or [=`. GNU grep 3.11 says `Invalid range end` there (status 2 both), and so does this one now; `[a-` and `[a-]` are unchanged. The red run is the shell's tests: the comparison with the host's grep names the four patterns. Mutation check: the old condition back fails the comparison.

**Files:**
- Modify: `crates/shell/src/pattern.rs`

**Interfaces:**
- Consumes: plan 1's `pattern::compile`'s sets.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/pattern.rs`**

In `crates/shell/src/pattern.rs`, replace:

````rust
            "[a-c-e-g]",
        ]
````

with:

````rust
            "[a-c-e-g]",
            // A `-` after a range at the pattern's end starts another,
            // without its end.
            "[a-a-",
            "[a-b-",
            "[a-z-",
            "[^a-b-",
            "[a-",
        ]
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `pattern::tests::dashes_in_a_set_are_what_gnu_grep_makes_of_them`.

- [ ] **Step 3: Change `crates/shell/src/pattern.rs`**

In `crates/shell/src/pattern.rs`, replace:

````rust
            i += 3;
            // A range cannot start where one ended (`[a-z-9]`); a last `-`
            // is a member.
            if p.get(i) == Some(&b'-') && p.get(i + 1).is_some_and(|&e| e != b']') {
                return Err(PatternError::InvalidRangeEnd);
````

with:

````rust
            i += 3;
            // A range cannot start where one ended (`[a-z-9]`, and `[a-z-`
            // at the pattern's end); a last `-` is a member.
            if p.get(i) == Some(&b'-') && p.get(i + 1) != Some(&b']') {
                return Err(PatternError::InvalidRangeEnd);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 264 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): call a range's dash at the pattern's end an invalid end

After a range, grep's pattern refused a `-` that starts another range
(`[a-z-9]`: `Invalid range end`) only when a character followed it, so
at the pattern's end (`[a-a-`, `[a-z-`) the `-` was taken as a member
and the missing `]` reported instead: `Unmatched [, [^, [:, [., or [=`.
GNU grep 3.11 says `Invalid range end` there (status 2 both), and so
does this one now. `[a-` and `[a-]` are unchanged.

Plan 1's final review found it (deferred to plan 4).
EOF
````


### Task 7: An interactive `sh` a background script starts ends at once

Decision 4 (plan 2's final review): an interactive `/bin/sh` whose group was never given the console ends before its first prompt; it told by asking for the console for a group numbered after itself, which says `EPERM` for `sh &`. A shell that a background script starts is in the script's group, though, and no group has its number, so the call said `ESRCH`: the shell printed a prompt over the screen, read end of input and ended. It now asks first to set the console's mode, which the kernel refuses (`EPERM`) to a process whose group does not hold the console, both cases alike, and ends on that; a shell that holds the console sets raw mode, as it does before every read anyway. The scenario `sh` runs `sh /root/k.sh &` with a script that holds `sh`: the screen must show the script's trace and one prompt. The red run is `sh`, which sees the stray prompt. Mutation checks: no `console_mode` test fails `sh` at `sh &`, and the old test (`console_foreground`'s `EPERM`) back fails it at the new lines.

**Files:**
- Modify: `tests/e2e/sh.txt`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: `relay_rt::sys::{console_mode, console_foreground}`, `relay_abi::console::MODE_RAW`, plan 2's chain of console holders.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, replace:

````text
expect \Aecho next\nnext\n\[1\]\+  Done                    sh\n
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log
send cd /tmp
````

with:

````text
expect \Aecho next\nnext\n\[1\]\+  Done                    sh\n
# Nor does one a background script starts: it ends before its prompt, so
# the screen shows the script's trace and this shell's one prompt
# (milestone 3, plan 4).
send echo sh > /root/k.sh
expect root@relay:~# $
send sh /root/k.sh &
expect \n\[1\] \d+\n
alive 1
expect \A(root@relay:~# \+ sh\n|\+ sh\nroot@relay:~# )$
send echo next
expect \Aecho next\nnext\n\[1\]\+  Done                    sh /root/k\.sh\n
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log /root/k.sh /root/k.log
send cd /tmp
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: FAIL: scenario `sh` stops at line 122, timed out waiting for `\A(root@relay:~# \+ sh\n|\+ sh\nroot@relay:~# )$`.

- [ ] **Step 3: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use alloc::string::String;
use relay_rt::sysio::words;
````

with:

````rust
use alloc::string::String;
use relay_abi::console::MODE_RAW;
use relay_rt::sysio::words;
````

Replace:

````rust
    } else if words.is_empty() {
        // An interactive shell leads a process group of its own when it
        // was started at a prompt (the group is numbered after it); one a
        // script started is in the script's group. One in a group of its
        // own that was never given the console (`sh &`) has no console to
        // read: it ends before its first prompt.
        let leader = match sys::console_foreground(sys::getpid()) {
            Ok(()) => true,
            Err(relay_abi::errno::EPERM) => return 0,
            Err(_) => false,
        };
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
````

with:

````rust
    } else if words.is_empty() {
        // A shell whose group was never given the console (`sh &`, or one
        // a background script started) may not set its mode, and has no
        // console to read: it ends before its first prompt. Otherwise it
        // leads a process group of its own when it was started at a prompt
        // (the group is numbered after it); one a script started is in the
        // script's group, and no group has its number.
        if sys::console_mode(MODE_RAW) == Err(relay_abi::errno::EPERM) {
            return 0;
        }
        let leader = sys::console_foreground(sys::getpid()).is_ok();
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
````

- [ ] **Step 4: Run the `sh`, `console`, `jobs`, `script_vars` scenarios**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario script_vars`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add tests userland
git commit -F - <<'EOF'
fix(sh): end an interactive sh a background script starts at once

An interactive `/bin/sh` whose group was never given the console ends
before its first prompt; it told by asking for the console for a group
numbered after itself, which says `EPERM` for `sh &`. A shell that a
background script starts is in the script's group, though, and no group
has its number, so the call said `ESRCH` instead: the shell printed a
prompt over the screen, read end of input and ended. It now asks first
to set the console's mode, which the kernel refuses (`EPERM`) to any
process whose group does not hold the console, both cases alike, and
ends at once on that; a shell given the console sets raw mode, as it
does before every read.

The scenario `sh` runs `sh /root/k.sh &` with a script that holds
`sh`: the screen shows the script's trace and one prompt.

Plan 2's final review found it (deferred to plan 4).
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 47 scenario(s) passed`.

````bash
git push -u origin m3p4/shell
gh pr create --base main --head m3p4/shell --title "fix(shell,sh): settle the shell's deferred findings" --body-file - <<'EOF'
## What

Milestone 3, plan 4, tasks 1–7: `$[` is `unsupported syntax: $[`; inside `${…}` quotes, escapes and further `${…}` are read as bash reads them, a quote left open there an unterminated quote; a parameter's value is borrowed before its room is taken; `cd ""` stays where it is with 0, as bash's; `seq` refuses a zero increment as soon as it has read it, in GNU's order; a `-` after a range at a pattern's end is `Invalid range end`, as GNU grep's; an interactive `sh` a background script starts ends before its prompt.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3, 4 and 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: xtask's and the scans' deferred fixes (Tasks 8–12)

Milestone 2's plan 5's minors 4–7: `verify-usb` shows every time as `date` does and fails when the stick's `system.img` cannot be read; the loader-rules test reads raw strings; the TLB scan sees `invpcid`, and both instructions in any case.

Branch `m3p4/xtask`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-xtask`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-xtask origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-xtask
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p4/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-xtask m3p4/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p4/shell>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 8: `verify-usb` shows a transcript's time as `date` shows one

Decision 6 (milestone 2's plan 5's final review, minor 4): `verify-usb` printed `system.img: built Tue Sep 29 10:00:00 UTC 2026`, as `date` shows a time, and on the next lines each transcript's `(run Mon Sep 28 14:09:17 2026 UTC)`, as debugfs shows one: two forms to compare at a glance. A transcript's time is now taken from debugfs's number alone and shown as `date` shows it, `(run Mon Sep 28 14:09:17 UTC 2026)`; `modified` returns the seconds only. The red run is xtask's tests of the transcripts, which expect the new form. Mutation check: a time shown an hour off fails both.

**Files:**
- Modify: `xtask/src/flash.rs`

**Interfaces:**
- Consumes: milestone 2's `flash::{check_transcripts, modified}`, `shell::time::date`.
- Produces: `flash::modified(&Path, Partition, &str) -> Result<u64>` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
````

with:

````rust
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
                "/root/checks/b.sh: FAILED, 0 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
                "  line 1: `cat /root/notes/a`: expected /remember me/, printed `forgotten` (line 1)",
````

Replace:

````rust
            [
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "/root/checks/b.sh: FAILED, not run (no /root/checks/b.log)",
````

with:

````rust
            [
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
                "/root/checks/b.sh: FAILED, not run (no /root/checks/b.log)",
````

Replace:

````rust
            [
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 2026 UTC)",
                "  the transcript is older than the system on the stick (system.img built Tue Sep 29 10:00:00 UTC 2026): run the script again",
                "/root/checks/b.sh: ok, 1 of 1 commands as expected (run Tue Sep 29 10:00:00 2026 UTC)",
            ]
````

with:

````rust
            [
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
                "  the transcript is older than the system on the stick (system.img built Tue Sep 29 10:00:00 UTC 2026): run the script again",
                "/root/checks/b.sh: ok, 1 of 1 commands as expected (run Tue Sep 29 10:00:00 UTC 2026)",
            ]
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask flash`

Expected: FAIL: 3 tests fail, among them `flash::tests::a_script_that_was_not_run_fails_the_check`, `flash::tests::a_transcript_older_than_the_system_on_the_stick_fails_the_check`.

- [ ] **Step 3: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        // `flash --kernel`) shows what an older kernel and programs did.
        let (run, run_text) = modified(target, root, &log)?;
        let stale = system_built.filter(|&built| run < built);
        let passed = report.ok() && stale.is_none();
        let verdict = if passed { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {run_text})",
            report.passed, report.commands,
        ));
````

with:

````rust
        // `flash --kernel`) shows what an older kernel and programs did.
        let run = modified(target, root, &log)?;
        let stale = system_built.filter(|&built| run < built);
        let passed = report.ok() && stale.is_none();
        let verdict = if passed { "ok" } else { "FAILED" };
        out.push(format!(
            "{path}: {verdict}, {} of {} commands as expected (run {})",
            report.passed,
            report.commands,
            shell::time::date(run),
        ));
````

Replace:

````rust

/// When the file at `path` was last changed: in seconds since 1970, and
/// in UTC as debugfs shows it (`Mon Sep 28 14:09:17 2026 UTC`).
fn modified(target: &Path, root: Partition, path: &str) -> Result<(u64, String)> {
    let text = run_stdout(
        Command::new("debugfs")
            .env("TZ", "UTC")
            .arg("-R")
````

with:

````rust

/// When the file at `path` was last changed, in seconds since 1970 (shown
/// as `date` shows a time, as `system.img`'s build time is).
fn modified(target: &Path, root: Partition, path: &str) -> Result<u64> {
    let text = run_stdout(
        Command::new("debugfs")
            .arg("-R")
````

Replace:

````rust
        .and_then(|l| {
            let (number, when) = l.split_once(" -- ")?;
            let secs = u64::from_str_radix(number.split(':').next()?, 16).ok()?;
            Some((secs, format!("{when} UTC")))
        })
````

with:

````rust
        .and_then(|l| {
            let number = l.split([' ', ':']).next()?;
            u64::from_str_radix(number, 16).ok()
        })
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask flash`

Expected: PASS: 8 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
fix(xtask): show verify-usb's run times as date shows a time

`verify-usb` printed `system.img: built Tue Sep 29 10:00:00 UTC 2026`,
as `date` shows a time, and on the next lines each transcript's
`(run Mon Sep 28 14:09:17 2026 UTC)`, as debugfs shows one: two forms
of the same thing, to be compared with each other at a glance. A
transcript's time is now taken from debugfs's number alone and shown
as `date` shows it, `(run Mon Sep 28 14:09:17 UTC 2026)`.

Milestone 2's plan 5's final review found it (minor 4, deferred to
plan 4).
EOF
````


### Task 9: `verify-usb` fails when the stick's `system.img` cannot be read

Decision 6 (milestone 2's plan 5's final review, minor 5): `verify-usb` fails a transcript older than the `system.img` on the stick's ESP, whose build time it reads; when that file could not be read, or was no archive, it printed a note and passed every transcript unchecked, so a run from before the last `flash --kernel` passed unseen. `system_built` now says why it has no time (`cannot read /EFI/RELAY/system.img on the ESP`, or the archive's own error, `only 7 bytes`), and `check_transcripts` takes what it gave: the build time, shown as its first line (`system.img: built …`), or the reason, `system.img: FAILED, …`, which fails the check after the transcripts are listed. `docs/hardware-test.md`'s failure table says what to do. The test makes an empty ESP (`image::blank_esp`, a test helper that the ESP's read-back test now shares), then a damaged `system.img` and a real archive. The red run is xtask's tests, which cannot compile against the old `check_transcripts` and find no `blank_esp`. Mutation checks: an unreadable `system.img` passing, the empty-stick line looked for in an empty list, and the build time never compared each fail a test.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `xtask/src/flash.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: milestone 2's `image::{esp_read, esp_write}`, `sysimg::{write, Archive::parse}`.
- Produces: `flash::check_transcripts(&Path, Partition, Machine, &Path, &Result<u64, String>) -> Result<(Vec<String>, bool)>`; `flash::system_built(&Path, Partition, &Path) -> Result<u64, String>` (private); `image::blank_esp(&Path) -> (PathBuf, Partition)` (tests only).

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 7 replacements, top to bottom:

Replace:

````rust

    /// An ext2 image holding `files` (path, contents) under its root.
````

with:

````rust

    /// A system built before every transcript of these tests.
    const BUILT: u64 = 1_790_000_000;

    /// An ext2 image holding `files` (path, contents) under its root.
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, &Ok(BUILT)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "system.img: built Mon Sep 21 14:13:20 UTC 2026",
                "/root/checks/a.sh: ok, 2 of 2 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir, None).unwrap();
        assert!(!ok);
        assert!(
            lines[0].starts_with("/root/checks/a.sh: FAILED, 1 of 2"),
            "{lines:?}"
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Qemu, &dir, &Ok(BUILT)).unwrap();
        assert!(!ok);
        assert!(
            lines[1].starts_with("/root/checks/a.sh: FAILED, 1 of 2"),
            "{lines:?}"
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, &Ok(BUILT)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "system.img: built Mon Sep 21 14:13:20 UTC 2026",
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

Replace:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, Some(built)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

with:

````rust
        );
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, &Ok(built)).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "system.img: built Tue Sep 29 10:00:00 UTC 2026",
                "/root/checks/a.sh: FAILED, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
````

Replace:

````rust
        );
        let (_, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok, "no system to compare with");
    }
````

with:

````rust
        );
    }

    #[test]
    fn a_system_img_that_cannot_be_read_fails_the_check() {
        // Without its build time no transcript can be shown to be older
        // than the system on the stick: the check fails, rather than
        // passing them unchecked.
        let dir = out_dir().join("verify-usb-selftest-system");
        let (esp_img, esp) = image::blank_esp(&dir.join("esp"));
        assert_eq!(
            system_built(&esp_img, esp, &dir),
            Err("cannot read /EFI/RELAY/system.img on the ESP".into())
        );
        image::esp_write(&esp_img, esp, "/EFI/RELAY/system.img", b"damaged", &dir).unwrap();
        assert_eq!(
            system_built(&esp_img, esp, &dir),
            Err("only 7 bytes".into())
        );
        let archive = sysimg::write(relay_abi::VERSION, BUILT, &[]).unwrap();
        image::esp_write(&esp_img, esp, "/EFI/RELAY/system.img", &archive, &dir).unwrap();
        assert_eq!(system_built(&esp_img, esp, &dir), Ok(BUILT));

        let (img, root) = ext2_with(
            &dir.join("root"),
            &[
                ("/root/checks/a.sh", "uname\n#> Relay\n"),
                ("/root/checks/a.log", "+ uname\nRelay\n"),
            ],
        );
        let unread = Err(String::from("only 7 bytes"));
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, &unread).unwrap();
        assert!(!ok);
        assert_eq!(
            lines,
            [
                "system.img: FAILED, only 7 bytes",
                "/root/checks/a.sh: ok, 1 of 1 commands as expected (run Mon Sep 28 14:09:17 UTC 2026)",
            ]
        );
    }
````

Replace:

````rust
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, None).unwrap();
        assert!(ok);
        assert_eq!(lines, ["no check scripts in /root/checks"]);
    }
````

with:

````rust
        let (img, root) = ext2_with(&dir, &[("/root/README", "hi")]);
        let (lines, ok) = check_transcripts(&img, root, Machine::Nuc, &dir, &Ok(BUILT)).unwrap();
        assert!(ok);
        assert_eq!(
            lines,
            [
                "system.img: built Mon Sep 21 14:13:20 UTC 2026",
                "no check scripts in /root/checks"
            ]
        );
    }
````

- [ ] **Step 2: Add the failing tests to `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
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
````

with:

````rust
        let dir = out_dir().join("esp-read-selftest");
        let (img, esp) = blank_esp(&dir);
        esp_write(&img, esp, "/EFI/RELAY/system.img", b"archive", &dir).unwrap();
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask flash`

Expected: FAIL: compile errors such as `` cannot find function `blank_esp` in module `image` ``; `` cannot find function `blank_esp` in this scope ``.

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
| `verify-usb`: `the transcript is older than the system on the stick` | The script ran before the last `flash --kernel` | Run the script again on the NUC |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

with:

````markdown
| `verify-usb`: `the transcript is older than the system on the stick` | The script ran before the last `flash --kernel` | Run the script again on the NUC |
| `verify-usb`: `system.img: FAILED, …` | The stick's `\EFI\RELAY\system.img` cannot be read, or is no archive: no transcript can be compared with it | Do not flash yet: report the line; `cargo xtask flash --kernel` writes it again, and the scripts must then run again |
| `verify-usb`: `… are not in the transcript` | The script stopped (Ctrl-C, a hang, a restart) before that command | Photograph the screen where it stopped; the transcript ends with the last command that ran |
````

- [ ] **Step 5: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
/// `flash --kernel` leaves the transcripts of an earlier run on the stick,
/// so one older than the `system.img` built at `system_built` (seconds
/// since 1970, if known) fails; `flash --full` erases them.
pub fn check_transcripts(
````

with:

````rust
/// `flash --kernel` leaves the transcripts of an earlier run on the stick,
/// so one older than the stick's `system.img`, built at `system` (seconds
/// since 1970), fails; `flash --full` erases them. A `system.img` that
/// cannot be read (`system` says why) fails the check, since no transcript
/// can then be compared with it.
pub fn check_transcripts(
````

Replace:

````rust
    scratch: &Path,
    system_built: Option<u64>,
) -> Result<(Vec<String>, bool)> {
````

with:

````rust
    scratch: &Path,
    system: &Result<u64, String>,
) -> Result<(Vec<String>, bool)> {
````

Replace:

````rust
    scripts.sort();
    let (mut out, mut ok) = (Vec::new(), true);
    for name in scripts {
````

with:

````rust
    scripts.sort();
    let (mut out, mut ok) = match system {
        Ok(built) => (
            vec![format!("system.img: built {}", shell::time::date(*built))],
            true,
        ),
        Err(why) => (vec![format!("system.img: FAILED, {why}")], false),
    };
    let system_built = system.as_ref().ok().copied();
    for name in scripts {
````

Replace:

````rust
    }
    if out.is_empty() {
        out.push(format!("no check scripts in {CHECKS_DIR}"));
````

with:

````rust
    }
    if out.len() == 1 {
        out.push(format!("no check scripts in {CHECKS_DIR}"));
````

Replace:

````rust
/// When the `system.img` on the ESP `esp` of `target` was built (seconds
/// since 1970), if it can be read.
fn system_built(target: &Path, esp: Partition, scratch: &Path) -> Option<u64> {
    let image = image::esp_read(target, esp, "/EFI/RELAY/system.img", scratch).ok()?;
    sysimg::Archive::parse(&image).ok().map(|a| a.build_time())
}
````

with:

````rust
/// When the `system.img` on the ESP `esp` of `target` was built (seconds
/// since 1970), or why that cannot be read.
fn system_built(target: &Path, esp: Partition, scratch: &Path) -> Result<u64, String> {
    let path = "/EFI/RELAY/system.img";
    let image = image::esp_read(target, esp, path, scratch)
        .map_err(|_| format!("cannot read {path} on the ESP"))?;
    sysimg::Archive::parse(&image)
        .map(|a| a.build_time())
        .map_err(|e| e.to_string())
}
````

Replace:

````rust
    let built = system_built(Stick::target(), esp, &scratch);
    match built {
        Some(secs) => println!("system.img: built {}", shell::time::date(secs)),
        None => println!("system.img: not readable on the stick; transcripts not compared with it"),
    }
    let (report, ok) = check_transcripts(Stick::target(), root, Machine::Nuc, &scratch, built)?;
    for l in report {
````

with:

````rust
    let built = system_built(Stick::target(), esp, &scratch);
    let (report, ok) = check_transcripts(Stick::target(), root, Machine::Nuc, &scratch, &built)?;
    for l in report {
````

- [ ] **Step 6: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust

#[cfg(test)]
````

with:

````rust

/// An empty FAT32 ESP of 64 MiB with `/EFI/RELAY`, in a new file under
/// `dir` (tests).
#[cfg(test)]
pub fn blank_esp(dir: &Path) -> (PathBuf, Partition) {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();
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
    (img, esp)
}

#[cfg(test)]
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask flash`

Expected: PASS: 9 tests.

Run: `cargo test -p xtask image`

Expected: PASS: 10 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add docs xtask
git commit -F - <<'EOF'
fix(xtask): fail verify-usb when the stick's system.img cannot be read

`verify-usb` fails a transcript older than the `system.img` on the
stick's ESP, which it reads for its build time. When that file could
not be read, or was no archive, it printed a note and passed every
transcript without comparing it, so a run from before the last
`flash --kernel` passed unseen. The check of the transcripts now takes
what reading `system.img` gave: its build time, which it shows as the
first line (`system.img: built …`), or why it could not be read
(`system.img: FAILED, cannot read /EFI/RELAY/system.img on the ESP`,
`… FAILED, only 7 bytes`), which fails the check after listing the
transcripts. `docs/hardware-test.md`'s failure table says what to do.

A test makes an empty ESP (`image::blank_esp`, which the ESP's
read-back test now shares) with no `system.img`, a damaged one and a
real archive.

Milestone 2's plan 5's final review found it (minor 5, deferred to
plan 4).
EOF
````


### Task 10: The loader-rules scanner reads raw strings

Decision 6 (milestone 2's plan 5's final review, minor 6): the test that keeps firmware features Linux does not use out of the loader looks only at code, not at `//` comments, and reads string and character literals to know where a comment starts. Its comment said the loader has no raw strings, and nothing checked it: a raw string holding `\` before its closing quote (`r"\"`) would have been read as open to the end of the file and a banned word after it missed, and one holding `"#//` would have lost its end to a comment. The scanner now reads a raw string (`r"…"`, `r#"…"#`, `br##"…"##`) as Rust does: no escapes, and to the `"` followed by as many `#` as began it; a name ending in `r`, a raw identifier (`r#type`) and `r"` inside a string are not raw strings. The loader has none today, so the banned-feature test passes as before. The red run is the boot crate's tests: the first raw string's comment is kept. Mutation checks: the raw-string arm removed, its closing `#`s not required, its opening ones not counted or not skipped each fail the test (a first version also checked that the `r` began no word, which only invalid Rust reaches; it went).

**Files:**
- Modify: `boot/src/lib.rs`

**Interfaces:**
- Consumes: milestone 2's `boot` tests' `code`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `boot/src/lib.rs`**

In `boot/src/lib.rs`, replace:

````rust

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
````

with:

````rust

    #[test]
    fn a_raw_string_is_code_whatever_it_holds() {
        // Milestone 2's plan 5's deferred finding: the scanner assumed the
        // loader had no raw strings. A raw string has no escapes and ends
        // at a `"` followed by as many `#` as it began with.
        let raw = r##"let a = r"\"; f(); // x"##;
        assert_eq!(code(raw), r##"let a = r"\"; f(); "##);
        let hashes = r###"let b = r#"a"//"#; g(); // y"###;
        assert_eq!(code(hashes), r###"let b = r#"a"//"#; g(); "###);
        let bytes = r###"let c = br##"x"#//"##; h(); // z"###;
        assert_eq!(code(bytes), r###"let c = br##"x"#//"##; h(); "###);
        // Not raw strings: a name ending in `r`, a raw identifier, and
        // `r"` inside a string.
        let others = r#"let d = (bar, r#type, "r\"//"); // w"#;
        assert_eq!(code(others), r#"let d = (bar, r#type, "r\"//"); "#);
    }

    /// On the NUC 12 firmware, exclusive opens stop the firmware's own
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-boot --lib`

Expected: FAIL: 1 test fails: `tests::a_raw_string_is_code_whatever_it_holds`.

- [ ] **Step 3: Change `boot/src/lib.rs`**

In `boot/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature. A `//` in a string literal is code, and
    /// so is a `"` in a character literal (`'"'`); a lifetime (`'a`) is
    /// neither. The loader has no raw strings.
    fn code(src: &str) -> String {
````

with:

````rust
    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature. A `//` in a string literal is code, raw
    /// (`r#"…"#`) or not, and so is a `"` in a character literal (`'"'`); a
    /// lifetime (`'a`) is neither.
    fn code(src: &str) -> String {
````

Replace:

````rust
                    while chars.next_if(|&n| n != '\n').is_some() {}
                }
````

with:

````rust
                    while chars.next_if(|&n| n != '\n').is_some() {}
                }
                'r' if {
                    let mut ahead = chars.clone();
                    while ahead.next_if_eq(&'#').is_some() {}
                    ahead.next() == Some('"')
                } =>
                {
                    // To the `"` followed by as many `#` as began it.
                    out.push(c);
                    let mut hashes = 0;
                    while let Some(h) = chars.next_if_eq(&'#') {
                        out.push(h);
                        hashes += 1;
                    }
                    out.extend(chars.next());
                    while let Some(n) = chars.next() {
                        out.push(n);
                        if n == '"' {
                            let mut seen = 0;
                            while seen < hashes {
                                let Some(h) = chars.next_if_eq(&'#') else {
                                    break;
                                };
                                out.push(h);
                                seen += 1;
                            }
                            if seen == hashes {
                                break;
                            }
                        }
                    }
                }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-boot --lib`

Expected: PASS: 23 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add boot
git commit -F - <<'EOF'
test(boot): read raw strings in the loader-rules scanner

The test that keeps firmware features Linux does not use out of the
loader looks only at code, not at `//` comments, and so reads string
and character literals to know where a comment starts. Its comment said
the loader has no raw strings, which nothing checked: a raw string
holding `\` before its closing quote (`r"\"`) would have been read as
open to the end of the file, and a banned word after it missed; one
holding `"#//` would have lost its end to a comment. The scanner now
reads a raw string (`r"…"`, `r#"…"#`, `br##"…"##`) as Rust does: no
escapes, and to the `"` followed by as many `#` as began it.

Milestone 2's plan 5's final review found it (minor 6, deferred to
plan 4).
EOF
````


### Task 11: The TLB scan sees `invpcid`

Decision 6 (milestone 2's plan 5's final review, minor 7): a host test walks the kernel's sources outside `arch/` and fails if one flushes the TLB itself, by a `tlb` path however imported or the `invlpg` instruction (which `invlpga` and `invlpgb` hold too). It missed `invpcid`, which drops TLB entries as well, in an `asm!` of its own; it now sees it. `mm::init`'s CR3 write stays where it is (spec §16 item 7). The red run is the kernel's tests: the scan does not see the `invpcid` case. Mutation check: the new test removed fails it.

**Files:**
- Modify: `kernel/src/arch/tlb.rs`

**Interfaces:**
- Consumes: milestone 2's `arch::tlb` tests' `flushes_the_tlb`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
            "unsafe { asm!(\"invlpg [{}]\", in(reg) v) };",
        ] {
````

with:

````rust
            "unsafe { asm!(\"invlpg [{}]\", in(reg) v) };",
            // Milestone 2's plan 5's deferred finding: `invpcid` drops
            // entries too (and `invlpga`, `invlpgb` hold `invlpg`).
            "unsafe { asm!(\"invpcid {}, [{}]\", in(reg) kind, in(reg) d) };",
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `arch::tlb::tests::the_scan_sees_every_way_to_name_the_tlb`.

- [ ] **Step 3: Change `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
    /// Whether `code` names the TLB other than through `arch::tlb`: a
    /// `tlb` path however it is imported, or the `invlpg` instruction.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        code.contains("invlpg")
            || code.match_indices("tlb").any(|(i, _)| {
````

with:

````rust
    /// Whether `code` names the TLB other than through `arch::tlb`: a
    /// `tlb` path however it is imported, or the `invlpg` or `invpcid`
    /// instruction.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        code.contains("invlpg")
            || code.contains("invpcid")
            || code.match_indices("tlb").any(|(i, _)| {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 391 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
test(kernel): see invpcid in the scan that keeps the TLB in arch

A host test walks the kernel's sources outside `arch/` and fails if one
flushes the TLB itself: a `tlb` path however it is imported, or the
`invlpg` instruction (which `invlpga` and `invlpgb` hold too). It
missed `invpcid`, which drops TLB entries as well, in an `asm!` of its
own. It now sees it.

Milestone 2's plan 5's final review found it (minor 7, deferred to
plan 4).
EOF
````


### Task 12: The TLB scan sees its instructions in any case

The prototype's review (a nit) and decision 6: the scan looked for `invlpg` and `invpcid` as written, but the assembler takes a mnemonic in any case, so `asm!("INVLPG [{}]")` outside `arch/` passed it. It now looks for both in the lowered text; a `tlb` path stays case-sensitive, as Rust's paths are. The red run is the kernel's tests: the scan misses the upper-case cases. Mutation check: the text not lowered fails the test.

**Files:**
- Modify: `kernel/src/arch/tlb.rs`

**Interfaces:**
- Consumes: Task 11's `flushes_the_tlb`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
            "unsafe { asm!(\"invpcid {}, [{}]\", in(reg) kind, in(reg) d) };",
        ] {
````

with:

````rust
            "unsafe { asm!(\"invpcid {}, [{}]\", in(reg) kind, in(reg) d) };",
            // The prototype's review: the assembler takes either case.
            "unsafe { asm!(\"INVLPG [{}]\", in(reg) v) };",
            "unsafe { asm!(\"InvPcid {}, [{}]\", in(reg) kind, in(reg) d) };",
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `arch::tlb::tests::the_scan_sees_every_way_to_name_the_tlb`.

- [ ] **Step 3: Change `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
    /// `tlb` path however it is imported, or the `invlpg` or `invpcid`
    /// instruction.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        code.contains("invlpg")
            || code.contains("invpcid")
            || code.match_indices("tlb").any(|(i, _)| {
````

with:

````rust
    /// `tlb` path however it is imported, or the `invlpg` or `invpcid`
    /// instruction in any case.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let lower = code.to_ascii_lowercase();
        lower.contains("invlpg")
            || lower.contains("invpcid")
            || code.match_indices("tlb").any(|(i, _)| {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 391 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
test(kernel): see TLB instructions in any case in the scan

The scan that keeps the TLB in `arch/` looked for `invlpg` and
`invpcid` as written, but the assembler takes a mnemonic in any case,
so `asm!("INVLPG [{}]")` outside `arch/` passed it. It now looks for
both in the lowered text; a `tlb` path stays case-sensitive, as Rust's
paths are.

The prototype's review found it.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 47 scenario(s) passed`.

````bash
git push -u origin m3p4/xtask
gh pr create --base main --head m3p4/xtask --title "fix(xtask): settle verify-usb's and the scans' deferred findings" --body-file - <<'EOF'
## What

Milestone 3, plan 4, tasks 8–12: `verify-usb` shows a transcript's time as `date` shows one, as it shows `system.img`'s build time, and fails when the stick's `system.img` cannot be read or is no archive (`system.img: FAILED, …`); the loader-rules test reads raw strings as Rust does; the TLB scan sees `invpcid`, and both instructions in any case.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3, 4 and 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-xtask
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `check5.sh`, version 0.4.0, and NUC checks 3, 4 and 5 (Tasks 13–18)

Milestone 3 is done: `check5.sh` in the `checks` scenario, version 0.4.0, the README, `docs/hardware-test.md`'s check 5 and the stick's `/root/README`, and NUC checks 3, 4 and 5 on a stick written by `flash --full`. This pull request goes up as a draft; after the user's NUC checks pass, the real transcripts and the results-log row go into a commit of it.

Branch `m3p4/release`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-release`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-release origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-release
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p4/xtask` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-release m3p4/xtask`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p4/xtask>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 13: `check5.sh`, NUC check 5

Spec §12.4 and decision 2: milestone 3's NUC check, 28 commands (29 after Task 14) whose lines are the same on the NUC and in QEMU: pipes (`seq 1000000 | wc -c`, 6888896 bytes through a 16 KiB pipe; `seq 1000000 | head -n 1`; `seq 1000 | grep 7 | wc -l`; a pipeline's status; `X | sh`), a background job (`t-spin &`, `jobs`, `ps | grep -c t-spin`, `kill %1`, `wait %1` and its 137) and a script given an empty argument and one with two blanks, passing `"$@"` on to another with `cd "$9"` (Task 4), a value with blanks never split and an unquoted empty one no word. A script prints no `[1] <pid>`, no Done line and no line for a job a signal ended (decision 3), the kernel's `killed` lines go to its log only and `ps` is read through `grep -c`, so nothing in the transcript races the prompt or names a pid. It needs no file of the earlier checks. The `checks` scenario runs it after `check4.sh`; its QEMU transcript is recorded as the scenario's disk holds it, and the NUC's is a copy until NUC check 5 records the real one; the test of the recorded transcripts now also checks that every check script has its two. The red runs are xtask's tests, which cannot read the new transcripts, and `checks`, which finds no `check5.sh`. Mutation checks: a changed expectation (`#> 143` for `#> 137`) fails the test, and Task 4's arm removed fails `checks` at the nested script's `cd "$9"`.

**Files:**
- Create: `rootfs/root/checks/check5.sh`
- Modify: `tests/e2e/checks.txt`
- Create: `xtask/fixtures/checks/check5.nuc.log`
- Create: `xtask/fixtures/checks/check5.qemu.log`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: plan 4b's `checks::{parse, check}`, the `check-script` step; Task 4.
- Produces: `rootfs/root/checks/check5.sh`; `xtask/fixtures/checks/check5.{qemu,nuc}.log`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/checks.txt`**

In `tests/e2e/checks.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# check 4. Afterwards their transcripts on the disk must show what the
# scripts expect.
timeout 30
````

with:

````text
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# checks 4 and 5. Afterwards their transcripts on the disk must show what
# the scripts expect.
timeout 30
````

Replace:

````text
expect root@relay:~# $
poweroff
check-script /root/checks/check3-a.sh
check-script /root/checks/check3-b.sh
check-script /root/checks/check4.sh
````

with:

````text
expect root@relay:~# $
send sh checks/check5.sh
expect \n\+ rm -f /root/check5-a\.sh .*\n
expect \n\+ false\n\+ echo \$\?\n1\n
expect root@relay:~# $
poweroff
check-script /root/checks/check3-a.sh
check-script /root/checks/check3-b.sh
check-script /root/checks/check4.sh
check-script /root/checks/check5.sh
````

- [ ] **Step 2: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
            (
                include_str!("../../rootfs/root/checks/check3-a.sh"),
````

with:

````rust
            (
                "check3-a.sh",
                include_str!("../../rootfs/root/checks/check3-a.sh"),
````

Replace:

````rust
            (
                include_str!("../../rootfs/root/checks/check3-b.sh"),
````

with:

````rust
            (
                "check3-b.sh",
                include_str!("../../rootfs/root/checks/check3-b.sh"),
````

Replace:

````rust
            (
                include_str!("../../rootfs/root/checks/check4.sh"),
                include_str!("../fixtures/checks/check4.qemu.log"),
                include_str!("../fixtures/checks/check4.nuc.log"),
            ),
        ];
        for (i, (script, qemu, nuc)) in parts.iter().enumerate() {
            for (machine, log) in [(Machine::Qemu, qemu), (Machine::Nuc, nuc)] {
````

with:

````rust
            (
                "check4.sh",
                include_str!("../../rootfs/root/checks/check4.sh"),
                include_str!("../fixtures/checks/check4.qemu.log"),
                include_str!("../fixtures/checks/check4.nuc.log"),
            ),
            (
                "check5.sh",
                include_str!("../../rootfs/root/checks/check5.sh"),
                include_str!("../fixtures/checks/check5.qemu.log"),
                include_str!("../fixtures/checks/check5.nuc.log"),
            ),
        ];
        // Every check script on the stick has its two transcripts here.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../rootfs/root/checks");
        let mut scripts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        scripts.sort();
        let named: Vec<&str> = parts.iter().map(|p| p.0).collect();
        assert_eq!(scripts, named);
        for (i, (_, script, qemu, nuc)) in parts.iter().enumerate() {
            for (machine, log) in [(Machine::Qemu, qemu), (Machine::Nuc, nuc)] {
````

Replace:

````rust
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's (check 4 has none: it runs programs only).
            if script.contains("#nuc>") {
````

with:

````rust
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's (checks 4 and 5 have none: they run programs
            // only).
            if script.contains("#nuc>") {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: compile errors such as `` couldn't read `xtask/src/../../rootfs/root/checks/check5.sh`: No such file or directory (os error 2) ``; `` couldn't read `xtask/src/../fixtures/checks/check5.qemu.log`: No such file or directory (os error 2) ``.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: FAIL: scenario `checks` stops at line 23, timed out waiting for `\n\+ rm -f /root/check5-a\.sh .*\n`.

- [ ] **Step 4: Create `rootfs/root/checks/check5.sh`**

Create `rootfs/root/checks/check5.sh`:

````bash
# NUC check 5 (docs/hardware-test.md; milestone 3, plan 4): after
# check4.sh, `sh checks/check5.sh`. Pipes, a background job and `kill`,
# script arguments and variables, run by /bin/sh. Its transcript is
# check5.log; the `#>` lines are explained in check3-a.sh. Nothing here
# differs between the NUC and QEMU.

# Start afresh, so the check can be run again.
rm -f /root/check5-a.sh /root/check5-a.log /root/check5-b.sh /root/check5-b.log

# Pipes (plan 1): 6.9 MB through a 16 KiB pipe, a reader that ends first
# (seq then ends quietly), three commands, the last command's status,
# and a shell reading its commands from a pipe.
seq 1000000 | wc -c
#> 6888896
seq 1000000 | head -n 1
#> 1
seq 1000 | grep 7 | wc -l
#> 271
seq 5 | grep -c 9
#> 0
echo $?
#> 1
echo 'echo piped $#' | sh
#> piped 0

# A background job (plan 2). A script prints no `[1] <pid>` and no Done
# line; `kill` ends the job, `wait` collects it and gives its status.
t-spin &
jobs
#> \[1\]\+  Running                 t-spin &
ps | grep -c t-spin
#> 1
kill %1
wait %1
echo $?
#> 137
jobs
ps | grep -c t-spin
#> 0

# Script arguments and variables (plan 3): a script given three
# arguments, one empty and one with two blanks, passes them on to
# another with one more. A value is never split into words (bash would
# split `$A`), an unquoted empty one is no word, and `cd "$9"` without a
# ninth argument stays where it is.
echo 'echo "$0: $# arguments"' > /root/check5-a.sh
echo 't-args "$@"' >> /root/check5-a.sh
echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
echo 'cd "$9"' >> /root/check5-b.sh
echo 'echo "cd: $?"' >> /root/check5-b.sh
echo 'pwd' >> /root/check5-b.sh
sh /root/check5-a.sh one '' 'two  words'
#> \+ echo "\$0: \$# arguments"
#> /root/check5-a\.sh: 3 arguments
#> \+ t-args "\$@"
#> \[1\] one
#> \[2\]\x20
#> \[3\] two  words
#> \+ sh /root/check5-b\.sh "\$@" last
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root
cat /root/check5-b.log
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root
A='a  b'
t-args $A "$A" '$A' ${A}c $E "$E"
#> \[1\] a  b
#> \[2\] a  b
#> \[3\] \$A
#> \[4\] a  bc
#> \[5\]\x20
false
echo $?
#> 1
````

- [ ] **Step 5: Create `xtask/fixtures/checks/check5.nuc.log`**

Create `xtask/fixtures/checks/check5.nuc.log`:

````text
+ rm -f /root/check5-a.sh /root/check5-a.log /root/check5-b.sh /root/check5-b.log
+ seq 1000000 | wc -c
6888896
+ seq 1000000 | head -n 1
1
+ seq 1000 | grep 7 | wc -l
271
+ seq 5 | grep -c 9
0
+ echo $?
1
+ echo 'echo piped $#' | sh
piped 0
+ t-spin &
+ jobs
[1]+  Running                 t-spin &
+ ps | grep -c t-spin
1
+ kill %1
+ wait %1
+ echo $?
137
+ jobs
+ ps | grep -c t-spin
0
+ echo 'echo "$0: $# arguments"' > /root/check5-a.sh
+ echo 't-args "$@"' >> /root/check5-a.sh
+ echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
+ echo 'echo "cd: $?"' >> /root/check5-b.sh
+ echo 'pwd' >> /root/check5-b.sh
+ sh /root/check5-a.sh one '' 'two  words'
+ echo "$0: $# arguments"
/root/check5-a.sh: 3 arguments
+ t-args "$@"
[1] one
[2] 
[3] two  words
+ sh /root/check5-b.sh "$@" last
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ A='a  b'
+ t-args $A "$A" '$A' ${A}c $E "$E"
[1] a  b
[2] a  b
[3] $A
[4] a  bc
[5] 
+ false
+ echo $?
1
````

- [ ] **Step 6: Create `xtask/fixtures/checks/check5.qemu.log`**

Create `xtask/fixtures/checks/check5.qemu.log`:

````text
+ rm -f /root/check5-a.sh /root/check5-a.log /root/check5-b.sh /root/check5-b.log
+ seq 1000000 | wc -c
6888896
+ seq 1000000 | head -n 1
1
+ seq 1000 | grep 7 | wc -l
271
+ seq 5 | grep -c 9
0
+ echo $?
1
+ echo 'echo piped $#' | sh
piped 0
+ t-spin &
+ jobs
[1]+  Running                 t-spin &
+ ps | grep -c t-spin
1
+ kill %1
+ wait %1
+ echo $?
137
+ jobs
+ ps | grep -c t-spin
0
+ echo 'echo "$0: $# arguments"' > /root/check5-a.sh
+ echo 't-args "$@"' >> /root/check5-a.sh
+ echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
+ echo 'echo "cd: $?"' >> /root/check5-b.sh
+ echo 'pwd' >> /root/check5-b.sh
+ sh /root/check5-a.sh one '' 'two  words'
+ echo "$0: $# arguments"
/root/check5-a.sh: 3 arguments
+ t-args "$@"
[1] one
[2] 
[3] two  words
+ sh /root/check5-b.sh "$@" last
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ A='a  b'
+ t-args $A "$A" '$A' ${A}c $E "$E"
[1] a  b
[2] a  b
[3] $A
[4] a  bc
[5] 
+ false
+ echo $?
1
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 8: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add rootfs tests xtask
git commit -F - <<'EOF'
test(checks): add check5.sh, NUC check 5 of milestone 3

Milestone 3's NUC check (spec §12.4): pipes, a background job and
`kill`, script arguments and variables, all run by `/bin/sh`, in 28
commands whose lines are the same on the NUC and in QEMU:

- `seq 1000000 | wc -c` (6.9 MB through a 16 KiB pipe), `seq 1000000 |
  head -n 1` (a reader that ends first), `seq 1000 | grep 7 | wc -l`,
  a pipeline's status (its last command's), and `X | sh`;
- `t-spin &`, `jobs`, `ps | grep -c t-spin`, `kill %1`, `wait %1` and
  its status 137; a script prints no `[1] <pid>`, no Done line and no
  line for a job a signal ended, so nothing races the prompt;
- a script given an empty argument and one with two blanks, passing
  `"$@"` on to another with `cd "$9"`; a value with blanks never split
  into words, an unquoted empty one no word, `$?`.

The `checks` scenario runs it after check4.sh. Its QEMU transcript is
recorded, and the NUC's is a copy of it until NUC check 5 records the
real one. The test of the recorded transcripts now also checks that
every script on the stick has its two.
EOF
````


### Task 14: check5's `cd` that stays is told from one that goes home

The prototype's review (minor M2) and decision 2: `check5.sh`'s nested script ran `cd "$9"` and then `pwd` to show that a `cd` to an empty directory stays where it is, but it started in /root, which is home, so a `cd` that went home there printed the same `/root`: a regression to that (`cd ""` going home), or an expander dropping the quoted empty word (leaving `cd` alone), would pass the check on the NUC and in QEMU, caught only by Task 4's host test. The nested script now goes to /root/checks first, and `pwd` must print `/root/checks`; `check5.sh` has 29 commands. Its QEMU transcript is recorded again, and the NUC's is still a copy of it. The task strengthens a check, so its evidence is a mutation check rather than a red run: `cd ""` going home passed `check5.sh` before it, and fails `checks` after it (27 of 29, `/root` printed for `/root/checks`).

**Files:**
- Modify: `rootfs/root/checks/check5.sh`
- Modify: `xtask/fixtures/checks/check5.nuc.log`
- Modify: `xtask/fixtures/checks/check5.qemu.log`

**Interfaces:**
- Consumes: Task 13's `check5.sh` and transcripts.
- Produces: nothing new.

- [ ] **Step 1: Change `rootfs/root/checks/check5.sh`**

In `rootfs/root/checks/check5.sh`, make these 2 replacements, top to bottom:

Replace:

````bash
# split `$A`), an unquoted empty one is no word, and `cd "$9"` without a
# ninth argument stays where it is.
echo 'echo "$0: $# arguments"' > /root/check5-a.sh
echo 't-args "$@"' >> /root/check5-a.sh
echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
echo 'cd "$9"' >> /root/check5-b.sh
````

with:

````bash
# split `$A`), an unquoted empty one is no word, and `cd "$9"` without a
# ninth argument stays where it is (not home, where it started).
echo 'echo "$0: $# arguments"' > /root/check5-a.sh
echo 't-args "$@"' >> /root/check5-a.sh
echo 'sh /root/check5-b.sh "$@" last' >> /root/check5-a.sh
echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
echo 'cd /root/checks' >> /root/check5-b.sh
echo 'cd "$9"' >> /root/check5-b.sh
````

Replace:

````bash
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root
cat /root/check5-b.log
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root
A='a  b'
````

with:

````bash
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd /root/checks
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root/checks
cat /root/check5-b.log
#> \+ echo "b: \$#, \[\$1\] \[\$2\] \[\$3\] \[\$4\]"
#> b: 4, \[one\] \[\] \[two  words\] \[last\]
#> \+ cd /root/checks
#> \+ cd "\$9"
#> \+ echo "cd: \$\?"
#> cd: 0
#> \+ pwd
#> /root/checks
A='a  b'
````

- [ ] **Step 2: Change `xtask/fixtures/checks/check5.nuc.log`**

In `xtask/fixtures/checks/check5.nuc.log`, make these 2 replacements, top to bottom:

Replace:

````text
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
````

with:

````text
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd /root/checks' >> /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
````

Replace:

````text
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ A='a  b'
````

with:

````text
b: 4, [one] [] [two  words] [last]
+ cd /root/checks
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root/checks
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd /root/checks
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root/checks
+ A='a  b'
````

- [ ] **Step 3: Change `xtask/fixtures/checks/check5.qemu.log`**

In `xtask/fixtures/checks/check5.qemu.log`, make these 2 replacements, top to bottom:

Replace:

````text
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
````

with:

````text
+ echo 'echo "b: $#, [$1] [$2] [$3] [$4]"' > /root/check5-b.sh
+ echo 'cd /root/checks' >> /root/check5-b.sh
+ echo 'cd "$9"' >> /root/check5-b.sh
````

Replace:

````text
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root
+ A='a  b'
````

with:

````text
b: 4, [one] [] [two  words] [last]
+ cd /root/checks
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root/checks
+ cat /root/check5-b.log
+ echo "b: $#, [$1] [$2] [$3] [$4]"
b: 4, [one] [] [two  words] [last]
+ cd /root/checks
+ cd "$9"
+ echo "cd: $?"
cd: 0
+ pwd
/root/checks
+ A='a  b'
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 5: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add rootfs xtask
git commit -F - <<'EOF'
test(checks): tell check5's cd that stays from one that goes home

check5.sh's nested script ran `cd "$9"` without a ninth argument and
then `pwd`, to show that `cd` with an empty directory stays where it
is. It started in /root, which is home, so a `cd` that went home there
printed the same `/root`: a regression to it, or an expander that
dropped the quoted empty word (leaving `cd` alone), would pass the
check on the NUC and in QEMU. The nested script now goes to
/root/checks first, and `pwd` must print `/root/checks`. check5.sh
has 29 commands; its QEMU transcript is recorded again, and the NUC's
is still a copy of it.

The prototype's review found it (minor M2).
EOF
````


### Task 15: Version 0.4.0

Spec §1.4 and decision 8: milestone 3 is version 0.4.0, as milestone 2 was 0.3.0. A spike bumped the version first and ran every scenario: only `uname`'s unit test, the `shell` scenario (milestone 1's, changed only by the version, as each bump before), `check3-a.sh`'s `uname -a` line and the recorded transcripts depend on it. The transcripts' `Relay relay 0.4.0 x86_64` and `Relay OS 0.4.0` lines are changed by `sed`, since the NUC's hold escape bytes: QEMU's as the `checks` scenario's disk now holds them, the NUC's until NUC check 3 records them. `relay-abi`'s comment and `docs/hardware-test.md`'s check 1 name the new version; the results log keeps its rows. The red runs are `uname`'s test and `shell`; `Cargo.toml` goes in after them, and cargo updates `Cargo.lock`.

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
- Produces: version 0.4.0.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 2 is version 0.3.0 (user-space gate §16 item 7), from
        // Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.3.0 x86_64\n".into()));
        assert_eq!(
````

with:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 3 is version 0.4.0 (user-space gate §16 item 11), from
        // Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.4.0 x86_64\n".into()));
        assert_eq!(
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/shell.txt`**

In `tests/e2e/shell.txt`, replace:

````text
send uname -a
expect \nRelay relay 0\.3\.0 x86_64\n
send cat /etc/hostname
````

with:

````text
send uname -a
expect \nRelay relay 0\.4\.0 x86_64\n
send cat /etc/hostname
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::uname_prints_the_system`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: FAIL: scenario `shell` stops at line 8, timed out waiting for `\nRelay relay 0\.4\.0 x86_64\n`.

- [ ] **Step 4: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
[workspace.package]
version = "0.3.0"
edition = "2024"
````

with:

````toml
[workspace.package]
version = "0.4.0"
edition = "2024"
````

- [ ] **Step 5: Change `crates/relay-abi/src/info.rs`**

In `crates/relay-abi/src/info.rs`, replace:

````rust
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.3.0`.
    pub release: [u8; UNAME_FIELD],
````

with:

````rust
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.4.0`.
    pub release: [u8; UNAME_FIELD],
````

- [ ] **Step 6: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.3.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

with:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.4.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

- [ ] **Step 7: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
uname -a
#> Relay relay 0\.3\.0 x86_64
date
````

with:

````bash
uname -a
#> Relay relay 0\.4\.0 x86_64
date
````

- [ ] **Step 8: Change `xtask/fixtures/checks/check3-a.nuc.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.3\.0 x86_64$/Relay relay 0.4.0 x86_64/; s/^Relay OS 0\.3\.0$/Relay OS 0.4.0/' xtask/fixtures/checks/check3-a.nuc.log
````

- [ ] **Step 9: Change `xtask/fixtures/checks/check3-a.qemu.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.3\.0 x86_64$/Relay relay 0.4.0 x86_64/; s/^Relay OS 0\.3\.0$/Relay OS 0.4.0/' xtask/fixtures/checks/check3-a.qemu.log
````

- [ ] **Step 10: Change `xtask/fixtures/checks/check3-b.nuc.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.3\.0 x86_64$/Relay relay 0.4.0 x86_64/; s/^Relay OS 0\.3\.0$/Relay OS 0.4.0/' xtask/fixtures/checks/check3-b.nuc.log
````

- [ ] **Step 11: Change `xtask/fixtures/checks/check3-b.qemu.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.3\.0 x86_64$/Relay relay 0.4.0 x86_64/; s/^Relay OS 0\.3\.0$/Relay OS 0.4.0/' xtask/fixtures/checks/check3-b.qemu.log
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 264 tests.

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
git commit -F - <<'EOF'
chore(release): 0.4.0

Milestone 3 is version 0.4.0 (spec §1.4, §2), as milestone 2 was
0.3.0: the workspace's version, from which the kernel's banner and
`uname -a` take theirs (`Relay relay 0.4.0 x86_64`). The `shell`
scenario (milestone 1's, changed only by the version, as each bump
before) and check3-a.sh's `uname -a` line follow, and so do the
recorded transcripts' `uname -a` and `Relay OS` lines, QEMU's as the
`checks` scenario's disk now holds them and the NUC's until NUC check
3 records them; relay-abi's comment and docs/hardware-test.md's check
1 name it. A spike bumped the version first and ran every scenario:
nothing else depends on it.
EOF
````


### Task 16: Docs: milestone 3 is done, and NUC check 5

Decisions 2, 8 and 9: the README says what milestone 3 added and that 0.4.0 ends the user-space gate, and names the shell's six built-ins. `docs/hardware-test.md`'s check 3 gains check 5 as step 8, `sh checks/check5.sh` after `check4.sh` and before `exit` by hand, which moves into it; `verify-usb`'s step lists its transcript, 29 of 29 (Task 14), after the line for `system.img`; the failure table gains a pipeline that stops and a `t-spin` count that differs. The stick's `/root/README` listed `cd` and `help` as programs and `sh FILE` alone: it names the built-ins as such and shows a pipeline, a job, a script's arguments and a variable, within the NUC's 120 columns (milestone 2's plan 5's minor 3 and plan 3's final review). No test reads these documents, so the task has no failing run.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/README`

**Interfaces:**
- Consumes: Tasks 13 and 15.
- Produces: nothing new.

- [ ] **Step 1: Change `README.md`**

In `README.md`, replace:

````markdown
shell's command prints, and the shell itself is one too: `/bin/sh` runs
every command but `cd`, `exit` and `help` as a program, and its scripts
may run scripts. Process 1 is the kernel's init: it starts `/bin/sh` at
boot and again whenever it ends, and a machine that cannot run its shell
(no `system.img`, or a shell that keeps ending) shows an error screen and
restarts at a key. The kernel holds no shell of its own any more.

Milestone 1 is version 0.2.0 and milestone 2 version 0.3.0; milestone 3
(pipes, background jobs, `ps` and `kill`, script variables) comes next.

````

with:

````markdown
shell's command prints, and the shell itself is one too: `/bin/sh` runs
every command but its built-ins (`cd`, `exit`, `help`, `jobs`, `wait`,
`kill`) as a program, and its scripts may run scripts. Process 1 is the
kernel's init: it starts `/bin/sh` at boot and again whenever it ends, and
a machine that cannot run its shell (no `system.img`, or a shell that
keeps ending) shows an error screen and restarts at a key. The kernel
holds no shell of its own any more.

Milestone 3 connects the programs: pipes (`seq 1000 | grep 7 | wc -l`),
with `grep`, `seq`, `sleep`, `true` and `false`; jobs in the background
(`t-spin &`), with `jobs`, `wait`, `kill %1` and `ps`; and scripts with
arguments and variables (`sh FILE a b`, `$1`, `"$@"`, `$?`, `NAME=value`).

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0 and milestone 3
version 0.4.0, which ends the user-space gate.

````

- [ ] **Step 2: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 5 replacements, top to bottom:

Replace:

````markdown

## Check 3 — files on the stick (plans 5 and 6; milestone 2), and check 4

````

with:

````markdown

## Check 3 — files on the stick (plans 5 and 6; milestone 2), and checks 4 and 5

````

Replace:

````markdown
`check4.sh` (NUC check 4 of the user-space gate, spec §12.4), after which
the error screen is visited by hand (plan 5). Every command is a program
now, and `/bin/sh` runs them, which the kernel's init starts as process 2:
````

with:

````markdown
`check4.sh` (NUC check 4 of the user-space gate, spec §12.4), after which
the error screen is visited by hand (plan 5), and since milestone 3's
plan 4 a fourth, `check5.sh` (NUC check 5). Every command is a program
now, and `/bin/sh` runs them, which the kernel's init starts as process 2:
````

Replace:

````markdown
   file into itself (refused), a script that runs another, and `free` again
   (the same memory in use). Then one step by hand: `exit` at the prompt.
   `init: /bin/sh (pid <n>) exited with 0; starting it again` and a new
   prompt `root@relay:~# ` come at once, without the motd: the shell is a
   program, and process 1 starts another. Photograph the screen.
8. The error screen (milestone 2, plan 5): `reboot`, and choose the stick
   again with F10, so that the kernel log's last lines are the boot's. At
````

with:

````markdown
   file into itself (refused), a script that runs another, and `free` again
   (the same memory in use).
8. Check 5 (milestone 3, plan 4): type `sh checks/check5.sh`. It runs for
   a few seconds: pipes (`seq 1000000 | wc -c`, 6888896 bytes through a
   16 KiB pipe; `seq 1000000 | head -n 1`, which ends at once;
   `seq 1000 | grep 7 | wc -l`; a pipeline's status; `X | sh`), a
   background job (`t-spin &`, `jobs`, `ps | grep -c t-spin`, `kill %1`,
   `wait %1` and its status 137), and a script given three arguments, one
   empty and one with two blanks, that passes them on to another
   (`"$@"`, `$#`, `cd "$9"`, a variable whose value has blanks, `$?`). A
   script prints no `[1] <pid>` and no Done line. The prompt comes back
   after `+ false`, `+ echo $?` and `1`. Then one step by hand: `exit` at
   the prompt. `init: /bin/sh (pid <n>) exited with 0; starting it again`
   and a new prompt `root@relay:~# ` come at once, without the motd: the
   shell is a program, and process 1 starts another. Photograph the
   screen.
9. The error screen (milestone 2, plan 5): `reboot`, and choose the stick
   again with F10, so that the kernel log's last lines are the boot's. At
````

Replace:

````markdown
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
````

with:

````markdown
   with F10: the motd and the prompt.
10. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
11. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are `system.img: built <time>`,
   `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`,
   `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run <time>)`,
   `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run
   <time>)` and `/root/checks/check5.sh: ok, 29 of 29 commands as
   expected (run <time>)`, with the UTC times of the four runs. A
   transcript older than the stick's
   `system.img` (a run before the last `flash --kernel`, which keeps the
````

Replace:

````markdown
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

with:

````markdown
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| `check5.sh` stops at a pipeline, and Ctrl-C ends it | A pipe's reader or writer was not woken on this machine | Photograph the screen; `ps` shows each process's state (`pipe` while it waits on one) |
| `check5.sh`: `ps \| grep -c t-spin` prints another count than 1, then 0 | A `t-spin` of an earlier command still runs, or `kill %1` did not end the job | `ps` lists the processes; `dmesg` shows `pid <n> (/bin/t-spin): killed: kill` |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

- [ ] **Step 3: Change `rootfs/root/README`**

Replace the whole of `rootfs/root/README` with:

````text
Relay OS

Files live on the USB stick and survive reboots. Every command but the
shell's built-ins (cd, exit, help, jobs, wait, kill) is a program in /bin
(ls /bin), and the shell is /bin/sh. Useful commands:
  ls, cd, pwd, cat, echo text > file, mkdir, rm, cp, mv, grep, help
Pipes and jobs:
  seq 1000 | grep 7 | wc -l    t-spin &    jobs    ps    kill %1    wait
Scripts and variables:
  sh FILE a b    the script reads $0, $1 to $9, ${10}, $#, "$@" and $?
  NAME=value     then $NAME or ${NAME}; a value is never split into words
````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add README.md docs rootfs
git commit -F - <<'EOF'
docs: mark milestone 3 done and add NUC check 5

The README says what milestone 3 added (pipes and the programs they
use, background jobs with `jobs`, `wait`, `kill` and `ps`, script
arguments and variables), names the shell's six built-ins, which plans
2 and 3 made three more of, and says 0.4.0 ends the user-space gate.

docs/hardware-test.md's check 3 gains check 5, `sh checks/check5.sh`,
after check4.sh and before `exit` by hand; `verify-usb` lists its
transcript, 29 of 29, after the line for `system.img`; the failure table
gains a pipeline that stops and a `t-spin` count that differs.

The stick's `/root/README` listed `cd` and `help` as programs and `sh
FILE` alone: it now names the built-ins as such and shows a pipeline, a
job, a script's arguments and a variable (milestone 2's plan 5's final
review, minor 3, and plan 3's final review, deferred to plan 4). No
test reads it.
EOF
````


### Task 17: The README's quick start runs every check script

The prototype's review (minor M1) and decision 2: the README's steps for the NUC still typed three check scripts and said `verify-usb` checks "the three scripts", so following them on a 0.4.0 stick left `check5.sh` unrun, and `verify-usb` fails a script that was not run (`check5.sh: FAILED, not run`). The quick start now types all four, and says every script's output is checked. A test reads the README and `docs/hardware-test.md` and fails if either does not type `sh checks/<name>` for every check script on the stick. The red run is xtask's tests: the README does not run `check5.sh`. Mutation check: the README's old step back is the red run; the test weakened to the bare name survives only because both documents name every script anyway.

**Files:**
- Modify: `README.md`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Task 16's README; Task 13's scripts.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
    #[test]
    fn many_wildcards_stay_fast() {
````

with:

````rust
    #[test]
    fn the_nuc_s_instructions_run_every_check_script() {
        // The prototype's review: the README's quick start ran three of
        // the four scripts, and `verify-usb` fails a script left unrun.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let dir = format!("{root}/rootfs/root/checks");
        let mut scripts: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.ends_with(".sh"))
            .collect();
        scripts.sort();
        assert!(scripts.len() >= 4, "{scripts:?}");
        for doc in ["README.md", "docs/hardware-test.md"] {
            let text = std::fs::read_to_string(format!("{root}/{doc}")).unwrap();
            for s in &scripts {
                assert!(
                    text.contains(&format!("sh checks/{s}")),
                    "{doc} does not run {s}"
                );
            }
        }
    }

    #[test]
    fn many_wildcards_stay_fast() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_nuc_s_instructions_run_every_check_script`.

- [ ] **Step 3: Change `README.md`**

In `README.md`, replace:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh` and `sh checks/check4.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of the three scripts.

````

with:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh`, `sh checks/check4.sh` and `sh checks/check5.sh`,
   then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
   the output of every script.

````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 13 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add README.md xtask
git commit -F - <<'EOF'
docs: run check5.sh in the README's quick start

The README's steps for the NUC still typed three check scripts, and
said `verify-usb` checks "the three scripts": following them on a 0.4.0
stick left check5.sh unrun, and `verify-usb` fails a script that was
not run (`check5.sh: FAILED, not run`). The quick start now types all
four and says every script's output is checked. A test reads the
README and docs/hardware-test.md and fails if either does not type
`sh checks/<name>` for every check script on the stick.

The prototype's review found it (minor M1).
EOF
````


### Task 18: Check 5's failure rows give steps a person can take

The prototype's review (minor M3): two rows Task 16 gave `docs/hardware-test.md`'s failure table asked for what the NUC cannot give. A pipeline that stops leaves no prompt to type `ps` at, and the Ctrl-C that ends it ends its processes too; a `kill %1` that does not end the job stops the script at `wait %1` for ever, so it never reaches the count the row named. The first row now says to press Ctrl-C and read `dmesg`'s `killed: Ctrl-C` lines; a row of its own covers a script stopped at `wait %1` (Ctrl-C ends the script but not the job, which runs in a group of its own: `ps` shows it, and `kill` ends it); the count's row keeps what a count can say. No test reads the table, so the task has no failing run.

**Files:**
- Modify: `docs/hardware-test.md`

**Interfaces:**
- Consumes: Task 16's failure table.
- Produces: nothing new.

- [ ] **Step 1: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| `check5.sh` stops at a pipeline, and Ctrl-C ends it | A pipe's reader or writer was not woken on this machine | Photograph the screen; `ps` shows each process's state (`pipe` while it waits on one) |
| `check5.sh`: `ps \| grep -c t-spin` prints another count than 1, then 0 | A `t-spin` of an earlier command still runs, or `kill %1` did not end the job | `ps` lists the processes; `dmesg` shows `pid <n> (/bin/t-spin): killed: kill` |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

with:

````markdown
| `check4.sh`: `t-spawn fill` fills the table with fewer than 60 children, or the second `free` differs | A process of an earlier command was left over (a zombie init did not collect, or a program still running) | `verify-usb` names the line; `dmesg` shows the `pid N (…)` lines of the programs killed |
| `check5.sh` stops after a pipeline's `+` line: no output, no prompt | A pipe's reader or writer was not woken on this machine | Photograph the screen, then Ctrl-C: the prompt comes back, and `dmesg` ends with a `pid <n> (…): killed: Ctrl-C` line for each process of the pipeline still there (and the script's `/bin/sh`) |
| `check5.sh` stops after `+ wait %1` | `kill %1` did not end the job, so the `t-spin` that `wait` waits for spins on | Photograph the screen, then Ctrl-C: it ends the script but not the job, which runs in its own group; `dmesg` shows no `pid <n> (/bin/t-spin): killed: kill` line, and `ps` shows the `t-spin`: end it with `kill <pid>` |
| `check5.sh`: `ps \| grep -c t-spin` prints another count than 1 before `kill %1`, or than 0 after `wait %1` | A `t-spin` of an earlier command still runs, or the job's did not start | `verify-usb` names the line; `ps` at the prompt lists the processes |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add docs
git commit -F - <<'EOF'
docs: give check 5's failure rows steps a person can take

Two rows docs/hardware-test.md's failure table gained for check 5 asked
for what the NUC cannot give: a pipeline that stops leaves no prompt to
type `ps` at, and the Ctrl-C that ends it ends its processes too; and a
`kill %1` that does not end the job stops the script at `wait %1` for
ever, so it never reaches the count the row named. The first now says
to Ctrl-C and read `dmesg`'s `killed: Ctrl-C` lines; a row of its own
covers a script stopped at `wait %1` (Ctrl-C ends the script but not
the job, in a group of its own: `ps` shows it and `kill` ends it); the
count row keeps only what a count can say.

The prototype's review found it (minor M3).
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 47 scenario(s) passed`.

````bash
git push -u origin m3p4/release
gh pr create --draft --base main --head m3p4/release --title "chore(release): 0.4.0, check5.sh and NUC checks 3–5" --body-file - <<'EOF'
## What

Milestone 3, plan 4, tasks 13–18: `check5.sh` (pipes, a background job and `kill`, script arguments and variables; 29 commands, the same lines on the NUC and in QEMU) in the `checks` scenario, its QEMU transcript recorded; version 0.4.0 (`uname -a` says `Relay relay 0.4.0 x86_64`); the README says milestone 3 is done, its quick start running every check script; `docs/hardware-test.md` gains check 5; the stick's `/root/README` names the built-ins and shows a pipeline, a job, a script's arguments and a variable.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC checks 3, 4 and 5 of `docs/hardware-test.md` (check 3's scripts and steps by hand, `check4.sh`, `check5.sh`, `exit` by hand and the error screen), run by the user with `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for NUC checks 3, 4 and 5**

Ask the user to run NUC checks 3, 4 and 5 (`docs/hardware-test.md`, check 3's steps 1–11: check 3's scripts, the three steps by hand, `check4.sh`, `check5.sh`, `exit` by hand, the error screen by hand, `poweroff` and `verify-usb`) with `cargo xtask flash --full` from this worktree, and to report their results; the pull request stays a draft until they do. The stick is then usually plugged into this machine, its root mounted at `/media/maw/relayroot`.

- [ ] **Record the NUC's transcripts in this pull request**

After a pass: check the stick with `cargo xtask verify-usb` (it only reads it), copy the real transcripts over the recorded ones (they replace those edited by hand to say `ABI 3` and `0.4.0`, and `check5.nuc.log`, a copy of QEMU's), and add the results-log row of `docs/hardware-test.md` (date, `3, 4 and 5 (milestone 3 done, 0.4.0)`, this branch's commit, `Pass`, and the notes: the startup lines, the four scripts' counts, the steps by hand, the error screen and the K120 key, anything the user saw):

````bash
cargo xtask verify-usb
cp /media/maw/relayroot/root/checks/check3-a.log xtask/fixtures/checks/check3-a.nuc.log
cp /media/maw/relayroot/root/checks/check3-b.log xtask/fixtures/checks/check3-b.nuc.log
cp /media/maw/relayroot/root/checks/check4.log xtask/fixtures/checks/check4.nuc.log
cp /media/maw/relayroot/root/checks/check5.log xtask/fixtures/checks/check5.nuc.log
cargo test -p xtask checks
````

Expected: `verify-usb` ends with `system.img: built …` and the four scripts' `ok` lines, and the tests pass. Then `git diff` shows only the transcripts and the new row (check that no earlier row changed), and:

````bash
cargo xtask ci
git add xtask/fixtures/checks docs/hardware-test.md
git commit -m "test(checks): record NUC checks 3, 4 and 5 on 0.4.0"
git push
gh pr ready m3p4/release
````

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. Propose the text of the `v0.4.0` tag and the GitHub release "Relay OS 0.4.0 — milestone 3" (as milestone 2's `v0.3.0`; the plan's "Release notes" section has a draft), and make neither unless the user asks. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p4-release
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
