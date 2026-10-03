# Milestone 4 · Plan 4: Hardening and 0.5.0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Milestone 4 ends: `check6.sh` runs lists, `if`/`elif`/`else`, `while`, `until`, `for` and `test` on the NUC and in the `checks` scenario, with 100 commands timed; the deferred minors of milestone 3 and of plans 2 and 3 are settled, among them the "extra empty prompt after `wait`", which was a scenario's late Enter; version 0.5.0; and NUC checks 3 to 6 pass on a stick written by `flash --full`. The spec's §15 item 4 lands with this plan. It ends with `cargo xtask ci` green, 49 scenarios, the NUC's transcripts recorded, and the user's tag `v0.5.0`.

**Architecture:** Small changes in many places, no new module. The shell's drop scan (`crates/shell/src/scan.rs`) reads `((…))` as bash's arithmetic command and keeps the kind of each open construct, so that groups (`{ … }`, function bodies) and subshells are dropped to their end; a kernel test drives `proc::console_input` with the global input queue; xtask's `verify-usb` keeps mtools' words, its scenario parser (`xtask/src/e2e.rs`) holds `poweroff`, `reboot` and `reset <command>` to the prompt, knows only four `-ahead` words and refuses a `key` step that ends with Ctrl-C or Ctrl-D; the loader tests' comment-free reader of a source file moves to `boot/src/code.rs`, which the kernel's TLB scan includes by its path. A new check script, `rootfs/root/checks/check6.sh`, with its QEMU transcript; the version and the documents.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; bash 5.2.21 and GNU coreutils 9.4 for comparison; mtools (`mcopy`, `mformat`); QEMU 8.2 under KVM with `-cpu max`; the NUC 12 Pro and its Kingston stick for the last pull request.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§5.3, §10, §11.3–§11.5, §12, §13, §15 items 1–4)
**Roadmap:** `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md` — this is plan 4 of 4 of milestone 4.

## In brief

- **Size.** 14 tasks in three code pull requests, plus this plan as PR 1: PR 2 plan 2's two deferred minors (the drop scan's arithmetic, the Ctrl-C wiring's test) and the drop scan's groups and subshells, the review's important finding; PR 3 milestone 3's three deferred minors, plan 3's two, and the refused `key` step that ends with Ctrl-C or Ctrl-D; PR 4 `check6.sh`, version 0.5.0 and the documents, a draft until the user's NUC checks 3 to 6, whose transcripts and results-log row go into a commit of it.
- **The empty prompt after `wait` was the scenario's** (decision 6): the spike logged every key event, mode switch and read to serial and found that `jobs`'s `key echo nope{ctrl-c}` pressed Enter after the Ctrl-C, an Enter that QEMU delivered after the scenario had moved on. `/bin/sh` was right; fourteen such steps become `type` steps, and the parser refuses the form.
- **Check 6** (decisions 2, 3, 4): 46 commands whose output is bash 5.2's with GNU's programs (but `date`'s time zone); its `"$@"` loop runs in a helper, since a script run inside itself empties its own transcript (the spike); 100 commands timed between two `date` lines, any time passing, the results log recording it, more than 10 seconds a note for milestone 5.
- **The pacing rule completed** (decision 5, the user's choice): eleven `poweroff`, `reboot` and `reset` steps gain an `expect` of the prompt; a bare `reset` and `reset-key` press a key at the error screen and stay exempt.
- **The prototype's review.** A fresh reviewer read the whole prototype (then thirteen tasks), ran its tests and `cargo xtask ci`, throwaway `piped` tests against bash 5.2 scripts, a differential test of the drop scan before and after Task 1 (300,000 random inputs) and a fuzz (200,000, and 10,000-deep `((`). It found 0 critical, 1 important and 5 minor real defects. The important one, older than this plan, is settled in a task of its own after the task it concerns, with a test that fails first (I1, the user's choice to fix it here); the minors are fixed in the commits they concern. It found correct: check 6 for the NUC (the `-nt` pair a whole second apart by the kernel's own clock, the `date` regex, `ls`'s message, `-w` on both mounts, the helper's transcript, a second run, `46 of 46`, the steps by hand against `control.txt`, the results log untouched), the drop scan's arithmetic in every form with a blank, the kernel test's isolation, `verify-usb` with CI's mtools, the shared reader's `#[path]`, the 25 changed scenario steps, the version's places and every commit's widths. It declined to judge other mtools versions' wording, the time of 100 programs on the NUC, and check 1b's measured MiB.

| Finding (review) | Decision |
|---|---|
| Important (I1): since plan 2 the drop scan counted no `{ … }`, `( … )` or function body, so a refused `a || {` ended its drop at once and the group's lines ran without their guard (`exit 1`, `rm -r` included) | Fixed, Task 2 (the user's choice: in this plan) |
| Minor (M1): Task 1 read `((cat <<EOF)`, two subshells to bash, as arithmetic, so a line of its here-document ran | Fixed in Task 1: such a `((` drops the rest of the script, the safe side |
| Minor (M2): `for((`, `if((`, `while((`, `!((` without a blank were not arithmetic (the minor's own symptom) | Fixed in Task 1 (the keyword's word ends first) |
| Minor (M3): `key …{ctrl-d}` has the same late Enter as `{ctrl-c}` | Fixed in Task 9 (refused too) |
| Minor (M4): two of check 6's failure rows named the wrong symptom or cause, and the checklist did not name the gate's §11.4 | Fixed in Task 11 |
| Minor (M5): Task 11's scope `docs(checks)` covered more than two modules | Fixed: `docs:` |

## Where this plan fits

Plan 4 of milestone 4 is the gate's fourth step (spec §12), the last of milestone 4. It builds on plans 1–3, whose lists, compound commands and `test` its `check6.sh` runs on the NUC, and settles what they and milestone 3 left for it (the roadmap's notes). Milestone 5 (redirection, ABI 4, `/dev/null`, the environment, `cd`) comes next; the roadmap's notes carry what this plan found for it.

## Working conventions

- Plan 4 lands as **four pull requests** (table below). This plan, with the spec's §15 item 4 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-03-m4-plan-4-hardening-and-0-5-0.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 10, 11 and 14 have no failing run: they change documents only. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m4p4/mutations.md`).
- **The expectations are bash's and GNU's.** `check6.sh`'s output was compared with bash 5.2 running GNU's programs (`tmp/m4p4/probes/c6.bash`); the drop scan's arithmetic with bash's scripts (`tmp/m4p4/probes/arith.sh`); `verify-usb`'s reasons with mcopy's own messages. In this session's shell `grep` may be a wrapper of another tool: any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop in a test.** The drop scan's loops are bounded by its input, and it keeps the kinds of 64 open constructs at most; the kernel test of the Ctrl-C types two keys. Mutation checks run under an address-space limit and a timeout (`tmp/m4p4/mutate.py`).
- **The NUC check scripts:** `check6.sh` is new, its NUC transcript a copy of QEMU's until the NUC run (Task 12); `check3-a.sh`'s `uname -a` line and the check-3 transcripts' version lines change with 0.5.0 (Task 13). The NUC run replaces every NUC transcript, `check4.nuc.log` edited by hand in plan 3 among them.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests, transcripts and scenarios that come with the change belong to its commit, and documents that are not tests take a commit of their own (Tasks 10, 11 and 14). Every commit body line is within 72 columns. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. The scenario parser refuses an input that no `expect` of the prompt paces (`send-ahead` and its kin mark input sent while something runs on purpose), from Task 8 on `poweroff`, `reboot` and `reset <command>` too, and from Task 9 a `key` step that ends with Ctrl-C or Ctrl-D. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `4e7042a` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request, review and NUC steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m4p4/plan` | — | The spec's §15 item 4 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m4p4/shell` | 1–3 | The drop scan's arithmetic, groups and subshells; the test of the Ctrl-C's wiring | `lint`, `unit`, `e2e` |
| 3 | `m4p4/xtask` | 4–10 | `verify-usb`'s reason; the `.sh` files of `checks/`; the TLB scan without comments; the four `-ahead` words; `poweroff`, `reboot` and `reset` paced; the refused `key …{ctrl-c}` and `{ctrl-d}`; `AGENTS.md` | `lint`, `unit`, `e2e` |
| 4 | `m4p4/release` | 11–14 | Check 6 in the checklist and `check6.sh`; version 0.5.0; the documents; NUC checks 3 to 6 (a draft until the user's run) | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 4 adds no crate. `crates/shell` uses only `vfs` and `relay-abi` and is `no_std` outside its tests. The kernel has no dev-dependencies and does not depend on `shell`; its TLB test includes a file of the loader's tests by its path, which adds no dependency.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (UG §3.1); `relay-abi` holds no architecture detail and keeps the same values on every architecture. Plan 4 changes no ABI value: `relay_abi::VERSION` stays 3.
- Every scenario and check script of milestones 1–4 passes; the scenarios change only by the `expect`s of Task 8 and the `type` steps of Task 9, the `checks` scenario by check 6, `shell` by the version.
- What a person types is untrusted (AGENTS.md): nothing that follows from it may make the drop scan panic, index out of bounds, overflow, allocate without bound or loop forever; it never fails.
- The shell follows bash 5.2 and commands GNU word for word; the only decided differences are spec §10's and §15's.
- Missing tools fail tests, never skip them: bash, GNU's coreutils, mtools and debugfs are on CI's runner.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 4 in PR 1:

1. **The pull requests.** Plan 4 is four: its plan; plan 2's two deferred minors in the shell and the kernel, with the drop scan's groups and subshells (the prototype's review); xtask's and the scenarios' rules, with milestone 3's three deferred minors and plan 3's two; `check6.sh`, version 0.5.0 and NUC checks 3 to 6, a draft until the maintainer's run, whose transcripts copied off the stick and results-log row go into a commit of that pull request. Milestone 4 ends with it.
2. **Check 6's `"$@"`** (§11.4). `check6.sh` writes a helper, `/root/check6-a.sh`, and runs it with three arguments (one empty, one holding two blanks), as check 5 does, rather than running itself: a script run inside itself empties its own transcript, which the inner `sh` opens afresh and tees a second time, so the outer run's later lines land over the inner run's (the spike). The helper loops with `for a in "$@"` and with `for a do`.
3. **Constructs in a check script.** A construct written across lines is traced line by line before any of it runs (§5.4), so the checker needs no change: the `#>` lines of its output follow its last line, and each of its lines counts as one of the script's commands. `check6.nuc.log` is a copy of the QEMU transcript until the NUC run replaces it, as `check5.nuc.log` was.
4. **The timing** (§5.3, §13). Check 6 prints `date` before and after 100 programs, two nested `for` loops of 10 `true`, each followed by a sync; any time passes, and the results log records it. Over 10 seconds on the NUC, milestone 5's plan 4 syncs once per top-level command and after each program instead (a note for milestone 5); under it, §13's risk is closed (the maintainer, 2026-10-03).
5. **Input paced by its prompt** (§11.3, §15 item 3). `poweroff`, `reboot` and `reset` with a command type a line, so each follows an `expect` of the prompt, as `send` does; a bare `reset` and `reset-key` press a key at the error screen, where no prompt comes, and stay exempt (the maintainer, 2026-10-03). Only `send-ahead`, `send-crlf-ahead`, `key-ahead` and `type-ahead` mark input typed ahead; another word ending in `-ahead` is an unknown step.
6. **A `key` step whose last key is Ctrl-C** is refused (`type` sends it alone). `key` presses Enter after its text; after a Ctrl-C the next `expect` matches the prompt the Ctrl-C brought before QEMU has delivered that Enter, so the Enter lands later, wherever the shell next reads: the empty prompt after a `wait` that the `jobs` scenario has shown since milestone 3 was this Enter, not the shell's (the spike traced it; the maintainer, 2026-10-03). A last Ctrl-D, which ends a program's input the same way, is refused too (the prototype's review).
7. **The drop scan and arithmetic** (§15 item 2). `((` where a command name stands, right after a keyword (`for((`, `!((`) too, is bash's arithmetic command, read to its `))`, so a `<<` in it is a shift, not a here-document, and a keyword may follow it, as after `fi`; before, the rest of a dropped script was dropped without a word, on the safe side. When its first `)` is not followed by another, bash reads two subshells instead (`((cat <<EOF)`), which the scan cannot read again, so the rest of the script is dropped with them, as before, on the safe side (the prototype's review).
8. **The drop scan and groups** (§15 item 2; the prototype's review, the maintainer, 2026-10-03). The scan counted no `{ … }`, `( … )` or function body, so a dropped `a || {` ended its drop at once and the group's lines ran without their guard (`exit 1` included), since plan 2. It now also counts `{` and `}` where a command name stands, a function's body (`f() {`) among them, and the `(` and `)` of a subshell, and keeps the kind of each construct open: a `)` closes a subshell, ends a `case` pattern, or closes nothing, as in bash, and `}` closes only a group.
9. **Milestone 3's deferred minors.** `verify-usb` names mcopy's reason when it cannot read the stick's `system.img`; the test that every check script has its transcripts considers only `*.sh` files of `rootfs/root/checks`; the scan for TLB instructions outside `kernel/src/arch/` reads code without its comments.
10. **Version 0.5.0** (§11.5): `uname -a`, `check3-a.sh`, the `uname` unit test, the transcripts' version lines and `docs/hardware-test.md` change with it; NUC checks 3 to 6 run on a stick written by `flash --full`, and their transcripts replace those edited by hand (`check4.nuc.log`) or copied from QEMU (`check6.nuc.log`).

## Review Focus

1. **A dropped command in a script**, after a refused line: `a || {`, `a && (`, `f() {` and their bodies across lines, `{ … }` and `( … )` nested in `if`/`while`/`for`/`case`, `((…))` with and without blanks (`for((;;))`, `!((`), `$((…))`, `((cmd <<EOF)`, here-documents after them, a keyword after `))`, `)` or `}`. Expected: none of a dropped command's lines runs; the drop ends where bash's construct ends, or later (the safe side), never earlier; the scan never panics. Tests: `arithmetic_holds_no_here_document`, `arithmetic_is_dropped_alone` (Task 1), `groups_and_subshells_are_dropped_to_their_end`, `a_dropped_group_runs_none_of_it` (Task 2), plan 2's `refused_syntax_is_read_as_bash_reads_it`, `an_if_dropped_any_way_runs_none_of_it`.
2. **NUC check 6 on the stick:** a first run and a second, the two `date` lines and their gap, `-nt`/`-ot` of files made a second apart, `[ -w /bin/ls ]` and `[ -w /root ]`, the helper's arguments, `verify-usb`'s `46 of 46`; the three steps by hand. Expected: every expectation passes on the NUC as in QEMU; the docs say what the screen shows. Tests: `the_check_scripts_pass_on_both_machines`, `the_nuc_s_instructions_run_every_check_script`, the scenario `checks` (Task 12), `control` for the steps by hand.
3. **A scenario that races the machine:** a `poweroff`, `reboot` or `reset <command>` with no `expect` of the prompt, a misspelt `-ahead` step, a `key` step ending in `{ctrl-c}` or `{ctrl-d}`, a bare `reset` or `reset-key` at the error screen. Expected: the parser refuses the first three, naming the line and what to use, and allows the last; every scenario passes the rules and runs green. Tests: `input_waits_for_a_prompt_unless_typed_ahead`, `a_key_step_does_not_end_with_ctrl_c_or_ctrl_d`, `every_scenario_waits_for_its_prompts` (Tasks 7–9), every scenario.
4. **`verify-usb` on a stick whose `system.img` cannot be read:** a missing file, a damaged FAT, an image it may not open, mtools missing. Expected: `system.img: FAILED, cannot read /EFI/RELAY/system.img on the ESP: <mtools' words>`, and the check fails. Test: `a_system_img_that_cannot_be_read_fails_the_check` (Task 4).
5. **The kernel's and the loader's source scans:** a comment naming `INVPCID` outside `arch/`, a `//` in a string or raw string, the loader's banned words. Expected: comments never count, code always does. Tests: `the_scan_sees_every_way_to_name_the_tlb`, `only_arch_flushes_the_tlb`, `comments_do_not_count`, `a_raw_string_is_code_whatever_it_holds`, `every_source_file_is_checked` (Task 6); `a_line_mode_ctrl_c_is_for_a_foreground_group_that_is_left` (Task 3).

## Release notes (proposed for `v0.5.0`)

To be proposed to the user after PR 4's NUC checks pass, as the GitHub release "Relay OS 0.5.0 — milestone 4"; neither the tag nor the release is made without their word.

> Milestone 4 of Relay OS, an operating system written from scratch in Rust, and the first half of its programmable shell gate: `/bin/sh` is a shell one can program in.
>
> **What it does**
>
> - **Lists:** `a; b`, `a && b`, `a || b`, `! a` and `a & b` mid-line, with bash's statuses; a command typed across lines after `|`, `&&` or `||`, at the prompt with bash's `> `, in scripts and in `X | sh`.
> - **Compound commands:** `if`/`elif`/`else`, `while`, `until` and `for` (over words or `"$@"`), nested up to 32 levels, typed across lines or on one; Ctrl-C ends a loop of programs or of built-ins (the kernel's `wait` tells the shell of a Ctrl-C typed between its own commands); a script's construct is traced line by line before it runs.
> - **Conditions:** `/bin/test` and `/bin/[`, GNU coreutils 9.4's grammar rule for rule with its messages, files answered as for root (44 programs); `grep -q`.
> - **Decided differences from bash** are listed in the design's §10 (no word splitting, a quote never continues across lines, compound commands refused in pipelines and with `&`, …); what this shell does not have yet is refused as unsupported syntax.
>
> **Tested**
>
> - `cargo xtask test`: host unit tests (with a bash corpus and GNU's `test` compared case for case) and 49 QEMU scenarios under KVM, each followed by `e2fsck`; every scenario's input waits for its prompt.
> - NUC checks 3 to 6 (`docs/hardware-test.md`) passed on the NUC (on the date of the results-log row PR 4 adds), on a stick written by `cargo xtask flash --full`: five scripts of 84, 5, 33, 29 and 46 commands, the steps by hand, and the error screen; check 6 timed 100 commands. `cargo xtask verify-usb` checked their transcripts.
>
> **Try it**
>
> See the quick start in `README.md`. At the prompt: `for x in a b c; do echo $x; done`, `if [ -f /bin/ls ]; then echo yes; fi`, `while true; do sleep 1; done` and Ctrl-C.
>
> Design: `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (milestones 4 and 5); milestone 4's four plans are in `docs/superpowers/plans/`.

---

## PR 1: The spec's §15 item 4, the roadmap's notes and this plan

The spec's §15 item 4, the decisions of this plan, with the parts of its body it corrects (the status line, §10, §11.4); the roadmap's status and its notes on plan 3's minors, the empty prompt after `wait`, plan 4's NUC check and `/dev/null` for milestone 5; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 4 plan 4's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p4/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m4p4/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-03-m4-plan-4-hardening-and-0-5-0.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-03-m4-plan-4-hardening-and-0-5-0.md
git commit -m "docs(plan): add milestone 4 plan 4, hardening and 0.5.0"
cargo xtask lint
git push -u origin m4p4/plan
gh pr create --base main --head m4p4/plan --title "docs(spec,plan): add milestone 4 plan 4, hardening and 0.5.0" --body-file - <<'EOF2'
## What

The implementation plan of milestone 4's plan 4 ("Hardening and 0.5.0"), with the spec's §15 item 4: four pull requests, the last a draft until NUC checks 3 to 6; check 6's `"$@"` in a helper (a script run inside itself empties its own transcript); its constructs counted line by line; its 100 commands timed, more than 10 seconds a note for milestone 5; `poweroff`, `reboot` and `reset <command>` paced by the prompt and only four `-ahead` words; a `key` step that ends with Ctrl-C refused (its late Enter was the empty prompt after `wait`); the drop scan's arithmetic; milestone 3's minors; version 0.5.0. §10 gains that quotes do not continue across lines, and §11.4 the helper. The roadmap's notes: plan 3's minors, the empty prompt after `wait`, plan 4's NUC check, and `/dev/null` against `is_console` for milestone 5.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (the harness's `bind_pr`); never poll CI. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m4p4/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-plan` (`.superpowers` is not linked there) and continue with PR 2.

---

## PR 2: Plan 2's deferred minors in the shell and the kernel (Tasks 1–3)

The drop scan reads `((…))` as bash's arithmetic command, so its `<<` no longer drops the rest of a script, and drops a group or a subshell to its end, so that a refused `a || {` no longer runs its body (the prototype's review); a kernel test pins the line-mode Ctrl-C's wiring to the process table.

Branch `m4p4/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p4/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-shell m4p4/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p4/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: The drop scan reads arithmetic whole

Decision 7 (plan 2's deferred minor): a command dropped before its end, after a syntax error or a refused line, is dropped to its end by `crates/shell/src/scan.rs`, which never fails. The parser refuses `(`, so `(( x = 1 << 2 ))` is dropped, and the scan read its `<<` as a here-document whose delimiter is `2`: the rest of the script was dropped without a word (the safe side, but wrong). `((` where a command name stands, right after a keyword too (`for((`, `!((`: the review's M2), is bash's arithmetic command: the scan now opens two levels at the second `(`, as it does for `$((`, so the `<<` inside is a shift and the drop ends at the `))`, after which a keyword may follow and an opener is bash's error, as after `fi` (`if true; then (( 1 )) fi`, `while (( 0 )) do …; done`; `tmp/m4p4/probes/arith.sh`). `$((…))` was already read so; two `(` apart, after a word or after a command's name are not arithmetic. A `((` whose first `)` is not followed by another is two subshells to bash (`((cat <<EOF)`, the review's M1), which the scan has read as words and cannot read again, so it is lost: the rest of the script is dropped, on the safe side (a first fix that did so only after a `<<` inside missed `((a) << 1)`, whose here-document comes after the inner `)`). The red run is the shell's tests: the scan never ends the drop, and the shell runs nothing after the arithmetic line. Mutation checks (12): the `(` before, the command-name place, the keyword's end, the two levels, the closer state, the arithmetic state, the check of the byte after the first `)` (three ways), its end, and the lost state, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `scan::Scan` (`open_level`, `operator`).
- Produces: `Scan.paren`, `Scan.arithmetic`, `Scan.arithmetic_check`, `Scan.lost` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

with:

````rust
    #[test]
    fn arithmetic_holds_no_here_document() {
        // Plan 2's deferred minor: `((` where a command name stands is
        // bash's arithmetic command, read to its `))` as `$((…))` is, so a
        // `<<` in it is a shift.
        assert_eq!(done_after(&["(( x = 1 << 2 ))", "b"]), [true, true]);
        assert_eq!(done_after(&["echo $(( 1 << 2 ))", "b"]), [true, true]);
        assert_eq!(done_after(&["a && (( (1) << 2 ))", "b"]), [true, true]);
        assert_eq!(
            done_after(&["if (( 1 << 2 )); then", "b", "fi"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["for ((i = 0; i << 1; i++))", "do b", "done"]),
            [false, false, true]
        );
        // It may go on across lines, as bash's does.
        assert_eq!(done_after(&["((", "1 << 2 ))", "b"]), [false, true, true]);
        // After its `))` a keyword may follow, as after `fi`, and an
        // opener is bash's error.
        assert_eq!(done_after(&["if a; then (( 1 )) fi", "b"]), [true, true]);
        assert_eq!(done_after(&["(( 1 )) if a", "b"]), [true, true]);
        assert_eq!(done_after(&["while (( 0 )) do b; done", "c"]), [true, true]);
        // Two `(` apart, after a word or where no command name stands, are
        // no arithmetic.
        assert_eq!(
            done_after(&["( (cat <<EOF", "EOF", "b"]),
            [false, true, true]
        );
        assert_eq!(done_after(&["a((b <<EOF", "EOF", "c"]), [false, true, true]);
        // Right after a keyword, with no blank, too (the prototype's
        // review).
        assert_eq!(
            done_after(&["for((i = 0; i << 1; i++)); do", "b", "done", "c"]),
            [false, false, true, true]
        );
        assert_eq!(
            done_after(&["if((1 << 2)); then", "b", "fi"]),
            [false, false, true]
        );
        assert_eq!(done_after(&["!((x = 1 << 2))", "b"]), [true, true]);
        // A `((` whose first `)` is not followed by another is two
        // subshells, as bash reads it (`((cat <<EOF)`), which the scan
        // cannot read again: the drop goes on to the end (the prototype's
        // review).
        assert_eq!(
            done_after(&["((cat <<EOF)", ")", "ran", "EOF", ")", "b"]),
            [false; 6]
        );
        assert_eq!(done_after(&["((a) << 1)", "b", "1", "c"]), [false; 4]);
        assert_eq!(done_after(&["((a) )", "b"]), [false, false]);
        // After its `))`, a `$(…)` nested in a word is a word's again.
        assert_eq!(done_after(&["(( 1 )); echo $($(a) b)", "c"]), [true, true]);
        assert_eq!(
            done_after(&["echo ((b <<EOF", "EOF", "c"]),
            [false, true, true]
        );
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert!(h.exists("/tmp/d"));
    }
````

with:

````rust
        assert!(h.exists("/tmp/d"));
    }

    #[test]
    fn arithmetic_is_dropped_alone() {
        // Plan 2's deferred minor: `((` is refused, and the drop read its
        // `<<` as a here-document's, so the rest of the script was dropped
        // without a word. bash runs `next` after each.
        for text in [
            &b"(( x = 1 << 2 ))\nt-args next\n"[..],
            b"for ((i = 0; i << 1; i++))\ndo t-args body\ndone\nt-args next\n",
            b"for((i = 0; i << 1; i++)); do\nt-args body\ndone\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
        // Two subshells with a here-document (the prototype's review): none
        // of its body runs, nor anything after it.
        let text = b"((cat <<EOF)\n)\nt-args ran\nEOF\n)\nt-args next\n";
        assert!(piped(text).is_empty());
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::arithmetic_holds_no_here_document`, `shell::tests::arithmetic_is_dropped_alone`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! and a here-document's body, data up to its delimiter's line.

````

with:

````rust
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! arithmetic (`((…))`) whole, and a here-document's body, data up to its
//! delimiter's line.

````

Replace:

````rust
    last: u8,
    /// The word being read: its first bytes, and how many it has. A quote
````

with:

````rust
    last: u8,
    /// The `(` before stood where a command name would: a `(` right after
    /// it starts arithmetic.
    paren: bool,
    /// The open levels are arithmetic's (`((`), until its first `)` shows
    /// whether bash reads them so: the byte after it decides
    /// (`arithmetic_check`).
    arithmetic: bool,
    arithmetic_check: bool,
    /// What was read cannot be told apart any more: the drop goes on to the
    /// end of the input.
    lost: bool,
    /// The word being read: its first bytes, and how many it has. A quote
````

Replace:

````rust
            last: b'\n',
            word: [0; KEYWORD_MAX],
````

with:

````rust
            last: b'\n',
            paren: false,
            arithmetic: false,
            arithmetic_check: false,
            lost: false,
            word: [0; KEYWORD_MAX],
````

Replace:

````rust
        if self.reading.is_some() && self.delim_byte(b) {
            return;
        }
        if b == b'\n' {
````

with:

````rust
        if self.reading.is_some() && self.delim_byte(b) {
            return;
        }
        // An arithmetic `((` whose first `)` is not followed by another is
        // two subshells, as bash reads it, which the scan read as words:
        // a here-document or a keyword in them went unseen.
        if core::mem::take(&mut self.arithmetic_check) && b != b')' {
            self.arithmetic = false;
            self.lost = true;
        }
        if b == b'\n' {
````

Replace:

````rust
            }
            b';' | b'&' | b'(' => self.operator(),
            b'>' => {
````

with:

````rust
            }
            // `((` where a command name stands is bash's arithmetic
            // command, read to its `))` as `$((…))` is: a `<<` in it is a
            // shift, no here-document. After it a keyword may follow, as
            // after `fi`.
            b'(' if last == b'(' && self.paren => {
                self.open_level(false);
                self.open_level(false);
                self.closed = true;
                self.arithmetic = true;
            }
            b'(' => {
                // A keyword before it (`for((`, `!((`) ends here.
                self.end_word();
                self.paren = self.command;
                self.operator();
            }
            b';' | b'&' => self.operator(),
            b'>' => {
````

Replace:

````rust
        self.depth == 0
            && !self.open
````

with:

````rust
        self.depth == 0
            && !self.lost
            && !self.open
````

Replace:

````rust
                    self.nested -= 1;
                }
````

with:

````rust
                    self.nested -= 1;
                }
                if self.arithmetic {
                    self.arithmetic_check = self.nested == 1;
                    self.arithmetic = self.nested > 0;
                }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 382 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read arithmetic whole when dropping a command

The drop scan read `(( x = 1 << 2 ))`, which the parser refuses, as a
here-document whose delimiter is `2`, so the rest of a dropped script
was dropped without a word. `((` where a command name stands, right
after a keyword too (`for((`), is bash's arithmetic command: the scan
reads it to its `))` as it reads `$((…))`, and a keyword may follow
it, as after `fi`. A `((` whose first `)` is not followed by another
is two subshells to bash, which the scan cannot read again, so the
rest of the script is dropped with them, on the safe side.
EOF
````


### Task 2: The drop scan drops a group or a subshell to its end

Decision 8 (the prototype's review, important I1, the user's choice to fix it in this plan): since plan 2 the drop scan counted no `{ … }`, `( … )` or function body, so a refused `a || {` ended its drop at once, and the group's lines ran without their guard: `t-args a || {` / `t-args ran` / `exit 1` / `}` ran `ran` and ended the script, and `[ "$1" = clean ] && {` / `rm -r /root/notes` / `}` deleted every time; bash runs none of them (`tmp/m4p4/probes/groups.txt`). The scan now keeps the kind of each open construct (`Kind`: a keyword's, a group, a subshell; the first 64, deeper ones closing nothing, the safe side): `{` where a command name stands opens a group, a function's body (`f() {`) among them, and `}` there closes only a group; a `(` where a command name stands opens a subshell (the arithmetic `((` closes it again), and a `)` closes a subshell, which a keyword may follow, as `fi` (an opener after it is bash's error), or in a `case` ends a pattern. Four expectations of plan 2's and Task 1's tests change with it: `{ while a; do` leaves 2 constructs open, `( (cat <<EOF` waits for its two `)`, and after `a(` or `echo (` a second `(` opens a subshell (bash's syntax error), dropped to its `)`. The red run is the shell's tests: the group's body runs, and the four changed expectations fail. Mutation checks (11): the subshell's close, its kind, the keyword after it, the arithmetic's close, the subshell's open (two ways), the group's close (two ways) and open, the kinds kept, and the depth past them, each broken, fail a test; a throwaway fuzz of 200,000 random inputs found no panic.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 1's `Scan`.
- Produces: `scan::Kind`, `KINDS_KEPT`, `Scan.kinds`, `Scan::{open, close, top}` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        for (lines, depth) in [
            // A command name stands after `time`, `{` and `}`.
            (&["time if a; then"][..], 1),
            (&["{ while a; do"], 1),
            (&["while a; do { b; } done"], 0),
````

with:

````rust
        for (lines, depth) in [
            // A command name stands after `time`, `{` and `}`; a `{`
            // opens a group too (the prototype's review).
            (&["time if a; then"][..], 1),
            (&["{ while a; do"], 2),
            (&["while a; do { b; } done"], 0),
````

Replace:

````rust
        assert_eq!(
            done_after(&["( (cat <<EOF", "EOF", "b"]),
            [false, true, true]
        );
        assert_eq!(done_after(&["a((b <<EOF", "EOF", "c"]), [false, true, true]);
        // Right after a keyword, with no blank, too (the prototype's
````

with:

````rust
        assert_eq!(
            done_after(&["( (cat <<EOF", "EOF", ") )", "b"]),
            [false, false, true, true]
        );
        // After `a(` a command name stands, so the second `(` opens a
        // subshell (bash's syntax error): the drop goes on to its `)`.
        assert_eq!(
            done_after(&["a((b <<EOF", "EOF", ")", "c"]),
            [false, false, true, true]
        );
        // Right after a keyword, with no blank, too (the prototype's
````

Replace:

````rust
        assert_eq!(
            done_after(&["echo ((b <<EOF", "EOF", "c"]),
            [false, true, true]
        );
    }
````

with:

````rust
        assert_eq!(
            done_after(&["echo ((b <<EOF", "EOF", ")", "c"]),
            [false, false, true, true]
        );
    }

    #[test]
    fn groups_and_subshells_are_dropped_to_their_end() {
        // The prototype's review: `{ … }`, `( … )` and a function's body
        // counted nothing, so the drop of `a || {` ended at once.
        for lines in [
            &["a && {", "b", "}", "c"][..],
            &["a || (", "b", ")", "c"],
            &["f() {", "b", "}", "c"],
            &["(a", "b)", "c"],
            &["if a; then {", "b", "}; fi", "c"],
            &["( case x in x) b;; esac", ")", "c"],
        ] {
            let mut want = vec![false; lines.len()];
            want[lines.len() - 2..].fill(true);
            assert_eq!(done_after(lines), want, "{lines:?}");
        }
        // A `}` closes only a group where a command name stands; a `)` a
        // subshell, or ends a `case` pattern, as bash reads them.
        assert_eq!(
            done_after(&["f ()", "{", "b", "}"]),
            [true, false, false, true]
        );
        assert_eq!(done_after(&["{ a }", "b", "}"]), [false, false, true]);
        assert_eq!(done_after(&["echo }", "b"]), [true, true]);
        assert_eq!(done_after(&["{ a; } }", "b"]), [true, true]);
        assert_eq!(done_after(&["(a; }", "b)"]), [false, true]);
        assert_eq!(done_after(&["x=1 {", "b"]), [true, true]);
        assert_eq!(done_after(&["case x in x) (a) ;; esac", "b"]), [true, true]);
        assert_eq!(done_after(&["case x in (x) a;; esac", "b"]), [true, true]);
        assert_eq!(
            done_after(&["case x in", "x) {", "a; }", "esac"]),
            [false, false, false, true]
        );
        // After its `)` a keyword may follow, as after `fi`, and an opener
        // is bash's error.
        assert_eq!(done_after(&["if a; then (b) fi", "c"]), [true, true]);
        assert_eq!(done_after(&["(a) if b", "c"]), [true, true]);
        // Arithmetic opens no subshell.
        assert_eq!(done_after(&["(( 1 ))", "b"]), [true, true]);
        // Deeper than the scan keeps the kinds, nothing closes: the safe
        // side.
        let deep: Vec<&str> = core::iter::repeat_n("{", 70)
            .chain(core::iter::repeat_n("}", 70))
            .collect();
        assert!(done_after(&deep).iter().all(|d| !d));
    }
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert!(piped(text).is_empty());
    }
````

with:

````rust
        assert!(piped(text).is_empty());
    }

    #[test]
    fn a_dropped_group_runs_none_of_it() {
        // The prototype's review: a refused `a || {`, `a && (` or `f() {`
        // ended its drop at once, so the lines of its body ran without
        // their guard, `exit` among them. bash runs none of them here.
        for text in [
            &b"t-args a && {\nt-args ran\n}\nt-args next\n"[..],
            b"t-args a && (\nt-args ran\n)\nt-args next\n",
            b"f() {\nt-args ran\n}\nt-args next\n",
            b"t-args a || {\nt-args ran\nexit 1\n}\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 4 tests fail, among them `scan::tests::groups_and_subshells_are_dropped_to_their_end`, `scan::tests::arithmetic_holds_no_here_document`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! arithmetic (`((…))`) whole, and a here-document's body, data up to its
````

with:

````rust
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! groups (`{ … }`, a function's body too) and subshells (`( … )`),
//! arithmetic (`((…))`) whole, and a here-document's body, data up to its
````

Replace:

````rust

/// What the lines read so far open and close.
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until`, `for`, `select`
    /// and `case` where a command name would stand counts one, and each
    /// `fi`, `done` and `esac` there one less.
    depth: usize,
    /// What was read ends after `|`, `&&` or `||` (blank and comment lines
````

with:

````rust

/// What an open construct is: a `)` closes a subshell (in a `case`, it
/// ends a pattern), and a `}` closes a group.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `if`, `while`, `until`, `for`, `select` and `case`, closed by their
    /// keyword.
    Keyword,
    Group,
    Subshell,
}

/// How many open constructs the scan knows the kind of; a `)` or `}`
/// deeper closes nothing, which drops more, the safe side.
const KINDS_KEPT: usize = 64;

/// What the lines read so far open and close.
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until`, `for`, `select`,
    /// `case` and `{` where a command name would stand counts one, and so
    /// does a subshell's `(`; each `fi`, `done` and `esac` there one less,
    /// a `}` closing a group and a `)` a subshell.
    depth: usize,
    /// The kinds of the first [`KINDS_KEPT`] of them.
    kinds: [Kind; KINDS_KEPT],
    /// What was read ends after `|`, `&&` or `||` (blank and comment lines
````

Replace:

````rust
            depth: 0,
            open: false,
````

with:

````rust
            depth: 0,
            kinds: [Kind::Keyword; KINDS_KEPT],
            open: false,
````

Replace:

````rust
            }
            // The word before a `)` is a `case` pattern, no keyword.
            b')' => {
                self.command = false;
                self.operator();
            }
````

with:

````rust
            }
            // The word before a `)` is a `case` pattern, no keyword. It
            // ends a subshell, which a keyword may follow, as `fi`; or a
            // pattern, or nothing (bash's error).
            b')' => {
                self.command = false;
                self.operator();
                if self.top() == Some(Kind::Subshell) {
                    self.close();
                    self.closed = true;
                }
            }
````

Replace:

````rust
            b'(' if last == b'(' && self.paren => {
                self.open_level(false);
````

with:

````rust
            b'(' if last == b'(' && self.paren => {
                // The first `(` opened no subshell after all.
                self.close();
                self.open_level(false);
````

Replace:

````rust
                self.operator();
            }
````

with:

````rust
                self.operator();
                if self.paren {
                    self.open(Kind::Subshell);
                }
            }
````

Replace:

````rust
            _ if closed && OPENERS.contains(&word) => self.command = false,
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
            b"for" | b"select" => {
                self.depth = self.depth.saturating_add(1);
                self.for_ = For::Name;
````

with:

````rust
            _ if closed && OPENERS.contains(&word) => self.command = false,
            b"if" | b"while" | b"until" => self.open(Kind::Keyword),
            b"for" | b"select" => {
                self.open(Kind::Keyword);
                self.for_ = For::Name;
````

Replace:

````rust
            b"case" => {
                self.depth = self.depth.saturating_add(1);
                self.command = false;
            }
            b"fi" | b"done" | b"esac" => {
                self.depth = self.depth.saturating_sub(1);
                self.closed = true;
            }
            b"}" => self.closed = true,
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" | b"{" => {}
            _ => self.command = false,
        }
    }
````

with:

````rust
            b"case" => {
                self.open(Kind::Keyword);
                self.command = false;
            }
            b"fi" | b"done" | b"esac" => {
                self.close();
                self.closed = true;
            }
            b"}" => {
                if self.top() == Some(Kind::Group) {
                    self.close();
                }
                self.closed = true;
            }
            b"{" => self.open(Kind::Group),
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" => {}
            _ => self.command = false,
        }
    }

    /// A construct of kind `kind` opens.
    fn open(&mut self, kind: Kind) {
        if let Some(slot) = self.kinds.get_mut(self.depth) {
            *slot = kind;
        }
        self.depth = self.depth.saturating_add(1);
    }

    /// The innermost open construct closes.
    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// The kind of the innermost open construct, if the scan knows it.
    fn top(&self) -> Option<Kind> {
        self.kinds.get(self.depth.checked_sub(1)?).copied()
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 384 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): drop a group or a subshell to its end

The drop scan counted no `{ … }`, `( … )` or function body, so a
refused `a || {` ended its drop at once and the group's lines ran
without their guard, `exit` among them, since plan 2. The scan now
counts `{` and `}` where a command name stands, a function's body
among them, and a subshell's `(` and `)`, and keeps the kind of each
construct open: a `)` closes a subshell, or in a `case` ends a
pattern, and a `}` closes only a group, as in bash.
EOF
````


### Task 3: A test pins the line-mode Ctrl-C to a group that is left

Plan 2's deferred minor: a Ctrl-C typed in line mode is for the console's foreground group, and the input queue keeps it, as a raw one for the group that takes the console back, when that group has no process left (`InputQueue::take_line_interrupt_for`, tested). Nothing tested its wiring, `tty::ctrl_c(|g| t.has_group(g))` in `kernel/src/proc.rs`'s `console_input`. That function now takes a table of any resources, so a kernel test can drive it with a `Table<()>` and the global input queue (`tty::type_for_test`, a test-only helper): the Ctrl-C kills a live foreground group, and after the group has ended it is kept for the group that takes the console back. The test pins code that exists, so its red run is the compile error of a test that calls `console_input` with a `Table<()>`. Mutation checks (3): the table's answer replaced by `true` or by `false`, and the kill removed, each fail the test.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: `proc::table::{Table, Group}`, `tty::{set_foreground, set_line_mode, take_raw_ctrl_c}`, `input::INTERRUPT`.
- Produces: `proc::console_input<R>(&mut Table<R>)` (private, now generic); `tty::type_for_test(&[u8])` (tests only).

- [ ] **Step 1: Add the failing tests to `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
    #[test]
    fn a_file_that_is_no_program_says_why() {
````

with:

````rust
    #[test]
    fn a_line_mode_ctrl_c_is_for_a_foreground_group_that_is_left() {
        // Plan 2's deferred minor: the input queue keeps a Ctrl-C for a
        // group that has ended, as its tests show; this pins the wiring
        // that asks the process table whether the group is left.
        use crate::input::INTERRUPT;
        use relay_abi::wait::KILLED_CTRL_C;
        let mut t = Table::<()>::new();
        let init = t.insert(0, Group::New, String::from("init"), ()).unwrap();
        let cmd = t.insert(init, Group::New, String::from("cmd"), ()).unwrap();
        tty::set_foreground(cmd);
        tty::set_line_mode(true);
        tty::type_for_test(&[INTERRUPT]);
        console_input(&mut t);
        assert_eq!(t.get(cmd).unwrap().killed, Some(KILLED_CTRL_C));
        // Once it has ended, the Ctrl-C waits, as a raw one, for the group
        // that takes the console back.
        t.end(cmd, WaitStatus::killed(KILLED_CTRL_C));
        tty::type_for_test(&[INTERRUPT]);
        console_input(&mut t);
        tty::set_line_mode(false);
        tty::set_foreground(init);
        assert!(tty::take_raw_ctrl_c());
    }

    #[test]
    fn a_file_that_is_no_program_says_why() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `type_for_test` in module `tty` ``; `mismatched types`.

- [ ] **Step 3: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
/// takes the console back reads it as raw input.
fn console_input(t: &mut Table<Res>) {
    if let Some(pgid) = tty::ctrl_c(|g| t.has_group(g)) {
````

with:

````rust
/// takes the console back reads it as raw input.
fn console_input<R>(t: &mut Table<R>) {
    if let Some(pgid) = tty::ctrl_c(|g| t.has_group(g)) {
````

- [ ] **Step 4: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust

/// Whether the input queue is locked now (for the kernel's checks that no
````

with:

````rust

/// Types `bytes` into the console's input, as `poll` adds what came.
#[cfg(test)]
pub fn type_for_test(bytes: &[u8]) {
    INPUT.lock().push(bytes);
}

/// Whether the input queue is locked now (for the kernel's checks that no
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 394 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
test(kernel): pin the line-mode Ctrl-C to a group that is left

The input queue's rule for a Ctrl-C typed in line mode, that a group
with no process left gets none and it waits as a raw one, was tested,
but not its wiring to the process table. console_input takes any
table, so a test drives it with the global queue: the Ctrl-C kills a
live foreground group, and after the group has ended it is kept for
the group that takes the console back.
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p4/shell
gh pr create --base main --head m4p4/shell --title "fix(shell,kernel): settle plan 2's deferred minors" --body-file - <<'EOF'
## What

Milestone 4, plan 4, tasks 1–3: the drop scan reads `((…))` where a command name stands as bash's arithmetic command, to its `))`, so a `<<` in it is a shift and the rest of a dropped script runs, and a keyword may follow it, as after `fi`; it drops a group (`{ … }`, a function's body) or a subshell to its end, keeping each construct's kind, so a refused `a || {` no longer runs its body, `exit` included, as it did since plan 2; a kernel test drives `console_input` with the global input queue, pinning the line-mode Ctrl-C to a foreground group that is left.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3 to 6
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: xtask's and the scenarios' rules (Tasks 4–10)

Milestone 3's three deferred minors (`verify-usb`'s reason, the `.sh` files of `checks/`, the TLB scan's comments), plan 3's two (`poweroff`, `reboot` and `reset` paced by the prompt, the four `-ahead` words) and the refused `key` step that ends with Ctrl-C or Ctrl-D, whose late Enter was the empty prompt after `wait`.

Branch `m4p4/xtask`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-xtask`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-xtask origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-xtask
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p4/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-xtask m4p4/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p4/shell>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 4: `verify-usb` keeps mcopy's reason when `system.img` cannot be read

Decision 9 (milestone 3's deferred minor): `verify-usb` said only `system.img: FAILED, cannot read /EFI/RELAY/system.img on the ESP` for a missing file, a damaged FAT, a stick it may not open and missing mtools alike. `image::esp_read` now fails with mtools' own words, the lines of its standard error joined by `; ` (or `cannot start mcopy` and why), and `system_built` puts them after its own: `… on the ESP: mcopy: File "::/EFI/RELAY/system.img" not found`, `…: init :: non DOS media; Cannot initialize '::'`, `…: Can't open …: No such file or directory; …` (`tmp/m4p4/progress.md`, t3). The red run is xtask's tests: the reason is missing. Mutation checks (3): the reason dropped, only its first line kept, and the exit status ignored, each fail a test.

**Files:**
- Modify: `xtask/src/flash.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: `image::esp_read`, `util::mtools`.
- Produces: `esp_read`'s error is mtools' standard error.

- [ ] **Step 1: Add the failing tests to `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, replace:

````rust
        // passing them unchecked.
        let dir = out_dir().join("verify-usb-selftest-system");
        let (esp_img, esp) = image::blank_esp(&dir.join("esp"));
        assert_eq!(
            system_built(&esp_img, esp, &dir),
            Err("cannot read /EFI/RELAY/system.img on the ESP".into())
        );
````

with:

````rust
        // passing them unchecked.
        // Milestone 3's deferred minor: mcopy's reason is kept, so a
        // missing file, a damaged FAT and a stick that cannot be opened are
        // told apart.
        let dir = out_dir().join("verify-usb-selftest-system");
        let (esp_img, esp) = image::blank_esp(&dir.join("esp"));
        assert_eq!(
            system_built(&esp_img, esp, &dir),
            Err("cannot read /EFI/RELAY/system.img on the ESP: \
                 mcopy: File \"::/EFI/RELAY/system.img\" not found"
                .into())
        );
        let zeros = dir.join("zeros.img");
        std::fs::write(&zeros, vec![0; 1 << 20]).unwrap();
        assert_eq!(
            system_built(&zeros, esp, &dir),
            Err("cannot read /EFI/RELAY/system.img on the ESP: \
                 init :: non DOS media; Cannot initialize '::'"
                .into())
        );
        let missing = dir.join("missing.img");
        assert_eq!(
            system_built(&missing, esp, &dir),
            Err(format!(
                "cannot read /EFI/RELAY/system.img on the ESP: \
                 Can't open {}: No such file or directory; Cannot initialize '::'",
                missing.display()
            ))
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `flash::tests::a_system_img_that_cannot_be_read_fails_the_check`.

- [ ] **Step 3: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, replace:

````rust
    let image = image::esp_read(target, esp, path, scratch)
        .map_err(|_| format!("cannot read {path} on the ESP"))?;
    sysimg::Archive::parse(&image)
````

with:

````rust
    let image = image::esp_read(target, esp, path, scratch)
        .map_err(|e| format!("cannot read {path} on the ESP: {e:#}"))?;
    sysimg::Archive::parse(&image)
````

- [ ] **Step 4: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
    let file = scratch.join("esp-read.tmp");
    run(mtools("mcopy")
        .args(["-o", "-i", &mtools_target(target, esp)])
        .arg(format!("::{path}"))
        .arg(&file))?;
    Ok(fs::read(&file)?)
````

with:

````rust
    let file = scratch.join("esp-read.tmp");
    let out = mtools("mcopy")
        .args(["-o", "-i", &mtools_target(target, esp)])
        .arg(format!("::{path}"))
        .arg(&file)
        .output()
        .context("cannot start mcopy")?;
    if !out.status.success() {
        // mtools' own words, which tell a missing file, a damaged FAT and
        // a stick that cannot be opened apart, on one line.
        let said = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = said.lines().collect();
        bail!("{}", lines.join("; "));
    }
    Ok(fs::read(&file)?)
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 93 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
fix(xtask): keep mcopy's reason when system.img cannot be read

verify-usb said only "cannot read /EFI/RELAY/system.img on the ESP"
for a missing file, a damaged FAT, a stick it may not open and missing
mtools alike. esp_read now fails with mtools' own words, its lines
joined on one line, and verify-usb shows them after its own.
EOF
````


### Task 5: Only the `.sh` files of `checks/` are check scripts

Decision 9 (milestone 3's deferred minor): the test that every check script has its two transcripts listed every file of `rootfs/root/checks`, so an editor's swap file beside the scripts failed it with a confusing list. Both tests of the scripts now list them through one helper, `scripts_in`, which keeps the `*.sh` files, as `verify-usb` does. The red run is xtask's tests: the helper's own test lists `.b.sh.swp` and `notes.txt`. Mutation checks (2): the filter and the sort, each removed, fail a test.

**Files:**
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: the two tests of the check scripts in `xtask/src/checks.rs`.
- Produces: `checks::tests::scripts_in(&str) -> Vec<String>` (tests only).

- [ ] **Step 1: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    /// The real check scripts against a transcript of each machine: QEMU's
````

with:

````rust

    /// The check scripts in `dir`, sorted: its `*.sh` files, as
    /// `verify-usb` takes them.
    fn scripts_in(dir: &str) -> Vec<String> {
        let mut scripts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        scripts.sort();
        scripts
    }

    #[test]
    fn the_check_scripts_are_the_sh_files() {
        // Milestone 3's deferred minor: an editor's swap file beside the
        // scripts was taken for one, and failed the test of their
        // transcripts with a confusing list.
        let dir = crate::util::out_dir().join("checks-selftest-scripts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["b.sh", ".b.sh.swp", "a.sh", "notes.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(scripts_in(dir.to_str().unwrap()), ["a.sh", "b.sh"]);
    }

    /// The real check scripts against a transcript of each machine: QEMU's
````

Replace:

````rust
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../rootfs/root/checks");
        let mut scripts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        scripts.sort();
        let named: Vec<&str> = parts.iter().map(|p| p.0).collect();
````

with:

````rust
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../rootfs/root/checks");
        let scripts = scripts_in(dir);
        let named: Vec<&str> = parts.iter().map(|p| p.0).collect();
````

Replace:

````rust
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let dir = format!("{root}/rootfs/root/checks");
        let mut scripts: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.ends_with(".sh"))
            .collect();
        scripts.sort();
        assert!(scripts.len() >= 4, "{scripts:?}");
````

with:

````rust
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let scripts = scripts_in(&format!("{root}/rootfs/root/checks"));
        assert!(scripts.len() >= 4, "{scripts:?}");
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_are_the_sh_files`.

- [ ] **Step 3: Change `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
````

with:

````rust
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.ends_with(".sh"))
            .collect();
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 94 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
test(xtask): take only the .sh files of checks/ as check scripts

The test that every check script has its two transcripts listed every
file of rootfs/root/checks, so an editor's swap file beside them failed
it with a confusing list. Both tests of the scripts now list them
through one helper that keeps the *.sh files, as verify-usb does.
EOF
````


### Task 6: The TLB scan reads code, not comments

Decision 9 (milestone 3's deferred minor): the kernel's test that only `kernel/src/arch/` flushes the TLB lowered the whole of each file, comments included, so a comment naming `INVPCID` outside `arch/` would fail it. The loader's tests already read a source file's code without its `//` comments, keeping a `//` in a string literal, raw or not, and a `"` in a character literal (`code()`, milestone 1's and milestone 2's findings). That reader moves, with its tests, to `boot/src/code.rs`, a test-only module of the loader (whose banned-words test skips it, as its tests name the words); the kernel's TLB test includes the same file with `#[path]`, since the kernel has no dev-dependencies (the user's choice), and scans `code(src)`. The red run is the kernel's tests, which cannot find the module. Mutation checks (2): the kernel scanning the raw text, and the loader's list taking `code.rs`, each fail a test.

**Files:**
- Create: `boot/src/code.rs`
- Modify: `boot/src/lib.rs`
- Modify: `kernel/src/arch/tlb.rs`

**Interfaces:**
- Consumes: the loader tests' `code()`; the kernel's `flushes_the_tlb`.
- Produces: `boot/src/code.rs` with `pub fn code(&str) -> String` (tests only), included by `kernel/src/arch/tlb.rs`'s tests.

- [ ] **Step 1: Write the failing tests for `boot/src/code.rs`**

Create `boot/src/code.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_do_not_count() {
        assert_eq!(
            code("let x = 1; // Exclusive\n/// Exclusive"),
            "let x = 1; \n"
        );
    }

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
}
````

- [ ] **Step 2: Add the failing tests and the module declaration to `boot/src/lib.rs`**

In `boot/src/lib.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
pub mod cmdline;
pub mod elf;
````

with:

````rust
pub mod cmdline;
#[cfg(test)]
mod code;
pub mod elf;
````

Replace:

````rust
mod tests {
    /// Every source file of the loader but this one, which names the banned
    /// words itself.
    const SOURCES: [(&str, &str); 8] = [
````

with:

````rust
mod tests {
    use crate::code::code;

    /// Every source file of the loader but this one, which names the banned
    /// words itself, and `code.rs`, whose tests do.
    const SOURCES: [(&str, &str); 8] = [
````

Replace:

````rust

    /// The code of a source file without its `//` comments, which may
    /// explain a banned feature. A `//` in a string literal is code, raw
    /// (`r#"…"#`) or not, and so is a `"` in a character literal (`'"'`); a
    /// lifetime (`'a`) is neither.
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

    #[test]
````

with:

````rust

    #[test]
````

Replace:

````rust
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
````

with:

````rust
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f != "lib.rs" && f != "code.rs")
            .collect();
        files.sort();
        let checked: Vec<&str> = SOURCES.iter().map(|(f, _)| *f).collect();
        assert_eq!(files, checked);
    }
````

- [ ] **Step 3: Add the failing tests to `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Whether `code` names the TLB other than through `arch::tlb`: a
    /// `tlb` path however it is imported, or the `invlpg` or `invpcid`
    /// instruction in any case.
    fn flushes_the_tlb(code: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
````

with:

````rust

    /// Whether `src` names the TLB other than through `arch::tlb` in its
    /// code, not its comments: a `tlb` path however it is imported, or the
    /// `invlpg` or `invpcid` instruction in any case.
    fn flushes_the_tlb(src: &str) -> bool {
        let code = &super::code::code(src);
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
````

Replace:

````rust
            "let tlbs = 1;",
        ] {
````

with:

````rust
            "let tlbs = 1;",
            // Milestone 3's deferred minor: a comment is no code, whatever
            // it names.
            "// INVPCID would drop them all.\nlet x = 1;",
            "/// Like `invlpg`, through the tlb module.\nfn f() {}",
        ] {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find `code` in `super` ``.

- [ ] **Step 5: Implement `boot/src/code.rs`**

Insert this at the top of `boot/src/code.rs`, above `#[cfg(test)]`:

````rust
//! The code of a Rust source file without its comments, for the tests
//! that scan sources for what they must not use: the loader's banned
//! firmware features, and the kernel's TLB instructions outside `arch/`
//! (which includes this file by its path, since the kernel has no
//! dev-dependencies).

/// The code of a source file without its `//` comments, which may
/// explain a banned feature. A `//` in a string literal is code, raw
/// (`r#"…"#`) or not, and so is a `"` in a character literal (`'"'`); a
/// lifetime (`'a`) is neither.
pub fn code(src: &str) -> String {
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
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&n| n != '\n').is_some() {},
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

- [ ] **Step 6: Change `kernel/src/arch/tlb.rs`**

In `kernel/src/arch/tlb.rs`, replace:

````rust
    x86_64::instructions::tlb::flush_all();
}

#[cfg(test)]
````

with:

````rust
    x86_64::instructions::tlb::flush_all();
}

/// The loader's reader of a source file's code without its comments.
#[cfg(test)]
#[path = "../../../boot/src/code.rs"]
mod code;

#[cfg(test)]
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 397 tests.

Run: `cargo test -p relay-boot --lib`

Expected: PASS: 23 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add boot kernel
git commit -F - <<'EOF'
test(kernel,boot): scan the code, not comments, for TLB instructions

The scan for TLB instructions outside kernel/src/arch/ lowered the
whole of each file, so a comment naming INVPCID there would fail it.
The loader's tests already read a source file's code without its
comments, keeping a `//` in a string, raw or not: that reader moves to
boot/src/code.rs, a test-only module, and the kernel's test includes
the same file by its path, since the kernel has no dev-dependencies.
EOF
````


### Task 7: Only the four `-ahead` steps mark input typed ahead

Decision 5 (plan 3's final review): the scenario parser took any word ending in `-ahead` for a mark of input typed ahead, so a misspelt one right after an `expect` of the prompt was told that it waits for nothing, instead of that it is no step. Only `send-ahead`, `send-crlf-ahead`, `key-ahead` and `type-ahead` are marks (`AHEAD`), refused right after a prompt; any other word is an unknown step. The red run is the parser's tests: `sned-ahead after an expect of the prompt`. Mutation checks (5): the old suffix test, and each of the four words dropped from the list, each fail a test.

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: `e2e::parse_scenario`.
- Produces: `e2e::AHEAD` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
            );
        }
    }

````

with:

````rust
            );
        }
        for word in ["send-ahead", "send-crlf-ahead", "key-ahead", "type-ahead"] {
            let e = at_prompt(&format!("{prompt}{word} a\n")).unwrap_err();
            assert!(
                e.contains(&format!("{word} after an expect of the prompt")),
                "{e}"
            );
        }
        // Only the four marks are marks: a misspelt one is an unknown step,
        // wherever it stands (plan 3's final review).
        for text in ["expect root@relay:~# $\nsned-ahead a\n", "sned-ahead a\n"] {
            let e = at_prompt(text).unwrap_err();
            assert!(e.ends_with("unknown step 'sned-ahead'"), "{e}");
        }
    }

````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `e2e::tests::input_waits_for_a_prompt_unless_typed_ahead`.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

pub fn parse_scenario(name: &str, text: &str) -> Result<Scenario> {
````

with:

````rust

/// The steps that send input typed ahead, while something runs.
const AHEAD: [&str; 4] = ["send-ahead", "send-crlf-ahead", "key-ahead", "type-ahead"];

pub fn parse_scenario(name: &str, text: &str) -> Result<Scenario> {
````

Replace:

````rust
        let input = matches!(word, "send" | "send-crlf" | "key" | "type");
        if input && !paced {
````

with:

````rust
        let input = matches!(word, "send" | "send-crlf" | "key" | "type");
        let ahead = AHEAD.contains(&word);
        if input && !paced {
````

Replace:

````rust
        }
        if word.ends_with("-ahead") && paced {
            bail!(
````

with:

````rust
        }
        if ahead && paced {
            bail!(
````

Replace:

````rust
        }
        if input || word.ends_with("-ahead") || matches!(word, "reboot" | "reset" | "reset-key") {
            paced = false;
````

with:

````rust
        }
        if input || matches!(word, "reboot" | "reset" | "reset-key") {
            paced = false;
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 94 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
fix(xtask): take only the four -ahead steps as typed ahead

The scenario parser took any word ending in -ahead as a mark of input
typed ahead, so a misspelt one right after an expect of the prompt was
told that it waits for nothing instead of that it is no step. Only
send-ahead, send-crlf-ahead, key-ahead and type-ahead are marks now.
EOF
````


### Task 8: `poweroff`, `reboot` and `reset` with a command wait for the prompt

Decision 5 (plan 3's final review, the user's choice): `poweroff`, `poweroff <command>`, `reboot`, `reboot <regex>` and `reset <command>` type a line, as `send` does, but the parser let them go without an `expect` of the prompt: eleven such steps followed only a command's output (`tmp/m4p4/pacing/census.txt`). Each now waits for the prompt, with its own message and no form typed ahead; the eleven gain an `expect root@relay:~# $` (in `unplug`, before the `unplug` step, since the kernel's `released` line comes after the prompt). A bare `reset` and `reset-key` press a key at the error screen, where no prompt comes, and stay as they are; the module comment says so. The red run is the parser's tests. Mutation checks (6): each of the three words, the `reset` with a command alone, `reset-key` taken in, and the refusal removed, each fail a test.

**Files:**
- Modify: `tests/e2e/bigfile.txt`
- Modify: `tests/e2e/diskfull.txt`
- Modify: `tests/e2e/fileops.txt`
- Modify: `tests/e2e/persist.txt`
- Modify: `tests/e2e/power.txt`
- Modify: `tests/e2e/sysinfo.txt`
- Modify: `tests/e2e/unplug.txt`
- Modify: `tests/e2e/utils.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Task 7's `parse_scenario`.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/bigfile.txt`**

In `tests/e2e/bigfile.txt`, replace:

````text
expect \n-rw-r--r-- 1 root root 8388608 .* big\n-rw-r--r-- 1 root root 8388608 .* copy\n
poweroff
````

with:

````text
expect \n-rw-r--r-- 1 root root 8388608 .* big\n-rw-r--r-- 1 root root 8388608 .* copy\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/diskfull.txt`**

In `tests/e2e/diskfull.txt`, replace:

````text
expect \nmore\n
poweroff
````

with:

````text
expect \nmore\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/fileops.txt`**

In `tests/e2e/fileops.txt`, replace:

````text
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
poweroff
````

with:

````text
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/persist.txt`**

In `tests/e2e/persist.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \na  b\n
reboot
````

with:

````text
expect \na  b\n
expect root@relay:~# $
reboot
````

Replace:

````text
expect \na  b\n
poweroff
````

with:

````text
expect \na  b\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/power.txt`**

In `tests/e2e/power.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \nbefore\n
reboot relay: restarting through the FADT reset register
````

with:

````text
expect \nbefore\n
expect root@relay:~# $
reboot relay: restarting through the FADT reset register
````

Replace:

````text
expect \nafter\n
poweroff
````

with:

````text
expect \nafter\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/sysinfo.txt`**

In `tests/e2e/sysinfo.txt`, replace:

````text
expect \n\d+ bytes, the first line: Relay OS \d+\.\d+\.\d+\nthe newest 16 bytes: 16 of them, the log's end: true\n
poweroff t-sys poweroff
````

with:

````text
expect \n\d+ bytes, the first line: Relay OS \d+\.\d+\.\d+\nthe newest 16 bytes: 16 of them, the log's end: true\n
expect root@relay:~# $
poweroff t-sys poweroff
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/unplug.txt`**

In `tests/e2e/unplug.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \nafter\n
reset reboot -f
````

with:

````text
expect \nafter\n
expect root@relay:~# $
reset reboot -f
````

Replace:

````text
expect \nbefore\n
unplug
````

with:

````text
expect \nbefore\n
expect root@relay:~# $
unplug
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/utils.txt`**

In `tests/e2e/utils.txt`, replace:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
poweroff /bin/poweroff
````

with:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
poweroff /bin/poweroff
````

- [ ] **Step 9: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        }
        // Only the four marks are marks: a misspelt one is an unknown step,
````

with:

````rust
        }
        // A step that types a command line waits for the prompt too: it
        // has no form typed ahead. A bare `reset` or `reset-key` presses a
        // key at the error screen, where no prompt comes (plan 3's final
        // review).
        for ok in [
            "expect root@relay:~# $\npoweroff\n",
            "expect root@relay:~# $\npoweroff t-sys poweroff\n",
            "expect root@relay:~# $\nreboot\n",
            "expect root@relay:~# $\nreboot relay: .*\n",
            "expect root@relay:~# $\nreset reboot -f\n",
            "expect x\nreset\n",
            "expect x\nreset-key\n",
        ] {
            assert_eq!(at_prompt(ok), Ok(()), "{ok:?}");
        }
        for text in [
            "expect x\npoweroff\n",
            "expect x\npoweroff t-sys poweroff\n",
            "expect x\nreboot\n",
            "expect x\nreboot relay: .*\n",
            "expect x\nreset reboot -f\n",
            "expect root@relay:~# $\nsend a\npoweroff\n",
        ] {
            let e = at_prompt(text).unwrap_err();
            let word = text.lines().last().unwrap().split(' ').next().unwrap();
            assert!(
                e.ends_with(&format!(
                    "{word} does not wait for the prompt: an expect that ends at it must come first"
                )),
                "{text:?}: {e}"
            );
        }
        // Only the four marks are marks: a misspelt one is an unknown step,
````

Replace:

````rust
    fn parses_the_check_script_step() {
        let s = parse_scenario("x", "poweroff\ncheck-script /root/checks/a.sh").unwrap();
        assert_eq!(
            s.steps[1],
            (2, Step::CheckScript("/root/checks/a.sh".into()))
        );
````

with:

````rust
    fn parses_the_check_script_step() {
        let s = parse_scenario(
            "x",
            "expect root@relay:~# $\npoweroff\ncheck-script /root/checks/a.sh",
        )
        .unwrap();
        assert_eq!(
            s.steps[2],
            (3, Step::CheckScript("/root/checks/a.sh".into()))
        );
````

Replace:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario(
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff\nreset\nreset reboot -f\nreset-key",
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Reboot(None)),
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff("poweroff".into())),
                (4, Step::Poweroff("t-sys poweroff".into())),
                (5, Step::Reset(String::new())),
                (6, Step::Reset("reboot -f".into())),
                (7, Step::ResetKey)
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
        assert!(
````

with:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let p = "expect root@relay:~# $";
        let text = format!(
            "{p}\nreboot\n{p}\nreboot relay: restarting\n{p}\npoweroff\n{p}\npoweroff t-sys poweroff\nreset\n{p}\nreset reboot -f\nreset-key"
        );
        let s = parse_scenario("x", &text).unwrap();
        let steps: Vec<_> = s
            .steps
            .into_iter()
            .filter(|(_, step)| !matches!(step, Step::Expect(_)))
            .collect();
        assert_eq!(
            steps,
            vec![
                (2, Step::Reboot(None)),
                (4, Step::Reboot(Some("relay: restarting".into()))),
                (6, Step::Poweroff("poweroff".into())),
                (8, Step::Poweroff("t-sys poweroff".into())),
                (9, Step::Reset(String::new())),
                (11, Step::Reset("reboot -f".into())),
                (12, Step::ResetKey)
            ]
        );
        let bad = parse_scenario("x", &format!("{p}\nreboot (")).unwrap_err();
        assert!(bad.to_string().contains("bad regex"), "{bad}");
        assert!(
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `e2e::tests::input_waits_for_a_prompt_unless_typed_ahead`.

- [ ] **Step 11: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! have ended at one (`root@relay:~# `, `root@relay:~# $` or `> $`), or the
//! scenario is refused. A line sent while a command runs is echoed twice,
//! by the line discipline and by the shell's editor, and lands inside the
//! output an expect waits for (programmable shell gate §15 item 3); input
//! sent while something runs on purpose is marked `-ahead`.
//!
````

with:

````rust
//! have ended at one (`root@relay:~# `, `root@relay:~# $` or `> $`), or the
//! scenario is refused. So do `poweroff`, `reboot` and `reset <text>`,
//! which type a command line; a bare `reset` and `reset-key` press a key
//! at the error screen, where no prompt comes. A line sent while a command
//! runs is echoed twice, by the line discipline and by the shell's editor,
//! and lands inside the output an expect waits for (programmable shell gate
//! §15 item 3); input sent while something runs on purpose is marked
//! `-ahead`.
//!
````

Replace:

````rust
        let ahead = AHEAD.contains(&word);
        if input && !paced {
````

with:

````rust
        let ahead = AHEAD.contains(&word);
        // These type a command line too; a bare `reset` presses a key at
        // the error screen, where no prompt comes.
        let command = matches!(word, "poweroff" | "reboot") || word == "reset" && !rest.is_empty();
        if input && !paced {
````

Replace:

````rust
                 something runs)"
            );
````

with:

````rust
                 something runs)"
            );
        }
        if command && !paced {
            bail!(
                "{name}:{line_no}: {word} does not wait for the prompt: an expect that \
                 ends at it must come first"
            );
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 94 tests.

- [ ] **Step 13: Run the `bigfile`, `diskfull`, `fileops`, `persist`, `power`, `sysinfo`, `unplug`, `utils` scenarios**

Run: `cargo xtask test --e2e-only --scenario bigfile`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario persist`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sysinfo`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario unplug`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add tests xtask
git commit -F - <<'EOF'
feat(xtask,e2e): pace poweroff, reboot and reset by the prompt

poweroff, reboot and reset with a command type a line, as send does,
but the scenario parser let them go without an expect of the prompt:
eleven such steps followed only a command's output. Each now waits for
the prompt, with no form typed ahead, and those steps gain the expect.
A bare reset and reset-key press a key at the error screen, where no
prompt comes, and stay as they are.
EOF
````


### Task 9: A `key` step does not end with Ctrl-C or Ctrl-D

Decision 6: `key` presses Enter after its text. After a Ctrl-C, the scenario's next `expect` matched the prompt the Ctrl-C brought before QEMU had delivered that Enter, and the next line sent over serial overtook it, so the Enter landed later, wherever the shell read next: after the blocking `wait %1` in `jobs`, it was the "extra empty prompt after `wait`" seen since milestone 3, and during a program it was echoed by the line discipline (the spike traced it: key events, mode switches and reads logged to serial). The parser refuses a `key` or `key-ahead` step whose last key is `{ctrl-c}`, or `{ctrl-d}`, which ends a program's input the same way (the review's M3; no scenario has one), with `use type`; the fourteen Ctrl-C steps there were become `type` or `type-ahead` steps, each followed by an `expect` that ends at the one prompt the Ctrl-C brings (`$`), and `jobs`'s `expect jobs\nroot@relay:~# $` is anchored (`\A`). The red run is the parser's tests. Mutation checks (6): the first key for the last, Ctrl-C alone, Ctrl-D alone, any Ctrl key, the refusal removed, and the suggested step, each fail a test.

**Files:**
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `tests/e2e/jobs.txt`
- Modify: `tests/e2e/keyboard.txt`
- Modify: `tests/e2e/pipes.txt`
- Modify: `tests/e2e/script_vars.txt`
- Modify: `tests/e2e/sh.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: `keys::typed`; Task 8's `parse_scenario`.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, make these 4 replacements, top to bottom:

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
send dmesg
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
send dmesg
````

Replace:

````text
alive 1
key-ahead {ctrl-c}
timeout 3
expect \n\^C\nroot@relay:~# 
timeout 30
````

with:

````text
alive 1
type-ahead {ctrl-c}
timeout 3
expect \n\^C\nroot@relay:~# $
timeout 30
````

Replace:

````text
send t-spin
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
# Ctrl-C ends a group that waits in the kernel: t-spawn blocked in wait
````

with:

````text
send t-spin
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
# Ctrl-C ends a group that waits in the kernel: t-spawn blocked in wait
````

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect t-spawn sleepers\n(pid \d+ \(/bin/t-spawn\): killed: Ctrl-C\n){4}\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
key echo nope{ctrl-c}
expect \^C\n
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect t-spawn sleepers\n(pid \d+ \(/bin/t-spawn\): killed: Ctrl-C\n){4}\^C\nroot@relay:~# $
# At the prompt, Ctrl-C only cancels the line being typed.
type echo nope{ctrl-c}
expect \^C\n
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/jobs.txt`**

In `tests/e2e/jobs.txt`, make these 3 replacements, top to bottom:

Replace:

````text
expect root@relay:~# $
key echo nope{ctrl-c}
expect \^C\n
````

with:

````text
expect root@relay:~# $
type echo nope{ctrl-c}
expect \^C\n
````

Replace:

````text
send jobs
expect jobs\nroot@relay:~# $
# Ctrl-C ends a wait at the prompt; the job runs on until it is killed.
````

with:

````text
send jobs
expect \Ajobs\nroot@relay:~# $
# Ctrl-C ends a wait at the prompt; the job runs on until it is killed.
````

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect \n\^C\n
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \n\^C\n
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/keyboard.txt`**

In `tests/e2e/keyboard.txt`, replace:

````text
expect root@relay:~# $
key echo nope{ctrl-c}
expect \^C\n
````

with:

````text
expect root@relay:~# $
type echo nope{ctrl-c}
expect \^C\n
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/pipes.txt`**

In `tests/e2e/pipes.txt`, replace:

````text
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
send t-spin | cat | cat
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
send dmesg
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \^C\nroot@relay:~# $
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
send t-spin | cat | cat
alive 1
type-ahead {ctrl-c}
expect \^C\nroot@relay:~# $
send dmesg
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/script_vars.txt`**

In `tests/e2e/script_vars.txt`, replace:

````text
expect root@relay:~# $
key echo no{ctrl-c}
expect \^C\n
````

with:

````text
expect root@relay:~# $
type echo no{ctrl-c}
expect \^C\n
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, make these 3 replacements, top to bottom:

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
# Scripts: the transcript gets what the screen gets, a script's programs'
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
# Scripts: the transcript gets what the screen gets, a script's programs'
````

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
send echo back
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \^C\nroot@relay:~# $
send echo back
````

Replace:

````text
alive 1
key-ahead {ctrl-c}
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
# Orphans that end while init waits for its shell are collected as they
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# $
# Orphans that end while init waits for its shell are collected as they
````

- [ ] **Step 7: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn input_typed_ahead_is_sent_as_other_input_is() {
        let s = parse_scenario(
            "x",
            "send-ahead a b\nkey-ahead {ctrl-c}\ntype-ahead q\nsend-crlf-ahead c\n",
        )
````

with:

````rust
    #[test]
    fn a_key_step_does_not_end_with_ctrl_c_or_ctrl_d() {
        // Plan 3's empty prompt after `wait`: `key` presses Enter after its
        // text, and after a Ctrl-C that Enter came once the scenario had
        // moved on, to land wherever the shell read next. A Ctrl-D that
        // ends a program's input is the same (the prototype's review).
        for (text, word, key, instead) in [
            (
                "expect root@relay:~# $\nkey echo no{ctrl-c}\n",
                "key",
                "ctrl-c",
                "type",
            ),
            ("key-ahead {ctrl-c}\n", "key-ahead", "ctrl-c", "type-ahead"),
            ("key-ahead a{ctrl-d}\n", "key-ahead", "ctrl-d", "type-ahead"),
            (
                "expect root@relay:~# $\nkey {ctrl-d}\n",
                "key",
                "ctrl-d",
                "type",
            ),
        ] {
            let e = parse_scenario("x", text).unwrap_err().to_string();
            assert!(
                e.ends_with(&format!(
                    "{word} presses Enter after its last key, {{{key}}}, and that Enter \
                     comes after the prompt the key brings: use {instead}"
                )),
                "{e}"
            );
        }
        for ok in [
            "key-ahead {ctrl-c}x\n",
            "key-ahead {ctrl-d}x\n",
            "key-ahead a{ctrl-e}\n",
            "type-ahead {ctrl-c}\n",
            "type-ahead {ctrl-d}\n",
            "expect root@relay:~# $\ntype echo no{ctrl-c}\n",
        ] {
            assert!(parse_scenario("x", ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn input_typed_ahead_is_sent_as_other_input_is() {
        let s = parse_scenario(
            "x",
            "send-ahead a b\nkey-ahead {ctrl-c}x\ntype-ahead q\nsend-crlf-ahead c\n",
        )
````

Replace:

````rust
                (1, Step::Send("a b".into())),
                (2, Step::Key("{ctrl-c}".into())),
                (3, Step::Type("q".into())),
````

with:

````rust
                (1, Step::Send("a b".into())),
                (2, Step::Key("{ctrl-c}x".into())),
                (3, Step::Type("q".into())),
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `e2e::tests::a_key_step_does_not_end_with_ctrl_c_or_ctrl_d`.

- [ ] **Step 9: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! key <text>                       (types <text> + Enter on the USB keyboard,
//!                                   QMP send-key; {up}, {ctrl-c}: see keys.rs)
//! type <text>                      (as key, without the Enter)
````

with:

````rust
//! key <text>                       (types <text> + Enter on the USB keyboard,
//!                                   QMP send-key; {up}, {ctrl-c}: see keys.rs;
//!                                   not ending with {ctrl-c} or {ctrl-d},
//!                                   after which the Enter would come late:
//!                                   type it)
//! type <text>                      (as key, without the Enter)
````

Replace:

````rust
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
````

with:

````rust
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                // A Ctrl-C or a Ctrl-D ends what reads it, and the next
                // expect matches the prompt that brings before QEMU has
                // delivered the Enter, which then lands wherever the shell
                // reads next.
                let last = keys::typed(rest)?.pop();
                if let Some(["ctrl", c @ ("c" | "d")]) = last.as_deref() {
                    bail!(
                        "{name}:{line_no}: {word} presses Enter after its last key, \
                         {{ctrl-{c}}}, and that Enter comes after the prompt the key \
                         brings: use {}",
                        word.replacen("key", "type", 1)
                    );
                }
                Step::Key(rest.to_string())
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 95 tests.

- [ ] **Step 11: Run the `ctrlc`, `jobs`, `keyboard`, `pipes`, `script_vars`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario script_vars`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add tests xtask
git commit -F - <<'EOF'
fix(xtask,e2e): refuse a key step that ends with Ctrl-C or Ctrl-D

key presses Enter after its text. After a Ctrl-C the next expect
matched the prompt the Ctrl-C brought before QEMU had delivered that
Enter, so the Enter landed later, wherever the shell read next: the
empty prompt after wait that the jobs scenario has shown since
milestone 3 was this Enter, not the shell's. A Ctrl-D that ends a
program's input is the same. The scenario parser refuses such a step,
and the fourteen there were become type steps, each followed by an
expect of the one prompt the Ctrl-C brings.
EOF
````


### Task 10: AGENTS.md says that `poweroff` and `reboot` wait for the prompt

Decisions 5 and 6: `AGENTS.md`'s e2e expectations say that `poweroff`, `reboot` and `reset <command>` wait for the prompt too, and that a Ctrl-C or a Ctrl-D ends a `type` step, never a `key` step. Documentation only.

**Files:**
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: Tasks 8 and 9.
- Produces: nothing new.

- [ ] **Step 1: Change `AGENTS.md`**

In `AGENTS.md`, replace:

````markdown
  `{`, which opens a key name (`{ctrl-c}`), so send it over serial
  (`send`). Every `send`, `key` and `type` waits for the prompt: an
  `expect` that ends at it must come since the input before, or the
  scenario is refused; input for a running program, a Ctrl-C, or a line
  beside a background job's output is `send-ahead`, `key-ahead` or
  `type-ahead`.
- **Mutation checks:** for a guard, break it and see a test fail. A
````

with:

````markdown
  `{`, which opens a key name (`{ctrl-c}`), so send it over serial
  (`send`). Every `send`, `key` and `type`, and every `poweroff`, `reboot`
  and `reset <command>`, waits for the prompt: an `expect` that ends at it
  must come since the input before, or the scenario is refused; input for
  a running program, a Ctrl-C, or a line beside a background job's output
  is `send-ahead`, `key-ahead` or `type-ahead`. A Ctrl-C or a Ctrl-D is
  the last key of a `type` step, never of a `key` step, whose Enter would
  come after the prompt the key brings.
- **Mutation checks:** for a guard, break it and see a test fail. A
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add AGENTS.md
git commit -F - <<'EOF'
docs: say that poweroff and reboot wait for the prompt too

AGENTS.md's bullet on e2e expectations says that poweroff, reboot and
reset with a command wait for the prompt, as send does, and that a
Ctrl-C or a Ctrl-D ends a type step, never a key step, whose Enter
would come after the prompt the key brings.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p4/xtask
gh pr create --base main --head m4p4/xtask --title "fix(xtask,e2e): settle the scenarios' and xtask's deferred minors" --body-file - <<'EOF'
## What

Milestone 4, plan 4, tasks 4–10: `verify-usb` keeps mcopy's reason for an unreadable `system.img`; only the `.sh` files of `rootfs/root/checks` are check scripts; the TLB scan reads code without comments, through the loader tests' reader, now `boot/src/code.rs`; only the four `-ahead` words mark input typed ahead; `poweroff`, `reboot` and `reset` with a command wait for the prompt (eleven steps gain an `expect`); a `key` step whose last key is Ctrl-C or Ctrl-D is refused, and the fourteen there were are `type` steps: their late Enter was the empty prompt after `wait` that `jobs` showed since milestone 3.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3 to 6
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-xtask
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `check6.sh`, version 0.5.0, and NUC checks 3 to 6 (Tasks 11–14)

Milestone 4 is done: `check6.sh` in the `checks` scenario and in the NUC checklist, version 0.5.0, the documents, and NUC checks 3 to 6 on a stick written by `flash --full`. This pull request goes up as a draft; after the user's NUC checks pass, the real transcripts and the results-log row go into a commit of it.

Branch `m4p4/release`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-release`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-release origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-release
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p4/xtask` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-release m4p4/xtask`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p4/xtask>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 11: Check 6 in the NUC checklist

Spec §11.4 and decisions 2 and 4: `docs/hardware-test.md` gains check 6 (step 9): what `check6.sh` runs, its two `date` lines for the results log and the 10 seconds that would make a sync after every command too slow (spec §13), the three steps by hand (a `for` typed across three lines, Ctrl-C at `> `, Ctrl-C in a running `while`; the `control` scenario runs each in QEMU), `verify-usb`'s line for it (`46 of 46`: each line of a construct written across lines counts, decision 3) and four failure rows (a loop whose file stays prints its line over and over; a Ctrl-C at `> ` is the line editor's, one in the loop `sleep`'s or the shell's check; the review's M4); the checklist names the gate's §11.4; the steps after it are renumbered, the results log untouched. The README's quick start runs `check6.sh`, and the stick's `/root/README` shows the control flow within the NUC's 120 columns. It comes before the script, since xtask's test that the NUC's instructions run every check script fails once Task 12 adds it. No test reads the new text, so the task has no failing run.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/README`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new.

- [ ] **Step 1: Change `README.md`**

In `README.md`, replace:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh`, `sh checks/check4.sh` and `sh checks/check5.sh`,
   then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
````

with:

````markdown
4. Type `sh checks/check3-a.sh`, then `reboot`, boot the stick again, type
   `sh checks/check3-b.sh`, `sh checks/check4.sh`, `sh checks/check5.sh`
   and `sh checks/check6.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
````

- [ ] **Step 2: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 4 replacements, top to bottom:

Replace:

````markdown
Checks 1, 1b and 2 test the boot, the display and the keyboard, and are run by
hand. Checks 3, 4 and 5 are one run of four scripts on the stick, with a
few steps by hand; it is the check every release gets. The results log at
the end records each run.

````

with:

````markdown
Checks 1, 1b and 2 test the boot, the display and the keyboard, and are run by
hand. Checks 3 to 6 are one run of five scripts on the stick, with a few
steps by hand; it is the check every release gets. The results log at the
end records each run.

````

Replace:

````markdown

## Checks 3, 4 and 5 — files, programs, pipes and jobs

The full checklist of the milestone 1 spec's §9.4 and the user-space gate
spec's §12.4, in one run. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`).

Most of the run is four scripts on the stick, in `/root/checks/` (in the
repository under `rootfs/root/checks/`): `check3-a.sh` and `check3-b.sh`
(check 3, either side of a restart), `check4.sh` and `check5.sh`. The
`/bin/sh` that init (process 1) starts as process 2 runs them: `sh FILE`
````

with:

````markdown

## Checks 3 to 6 — files, programs, pipes, jobs and control flow

The full checklist of the milestone 1 spec's §9.4, the user-space gate
spec's §12.4 and the programmable shell gate spec's §11.4, in one run.
The K120 and the stick sit on the ports of check 2 (the stick on bus 4
port 3 in Mint's `lsusb -t`).

Most of the run is five scripts on the stick, in `/root/checks/` (in the
repository under `rootfs/root/checks/`): `check3-a.sh` and `check3-b.sh`
(check 3, either side of a restart), `check4.sh`, `check5.sh` and
`check6.sh`. The
`/bin/sh` that init (process 1) starts as process 2 runs them: `sh FILE`
````

Replace:

````markdown

### The error screen, power off and verify-usb

9. The error screen: `reboot`, and choose the stick again with F10, so
   that the kernel log's last lines are the boot's. At the prompt type
   `exit` three times within 10 s. After the first two, init's line and a
   new prompt; after the third, `init: /bin/sh (pid <n>) exited with
   <status>` (0 unless the command before `exit` failed) and the error
   screen: `*** Relay OS cannot run its shell ***`,
   `/bin/sh ended 3 times within 10 s`, the kernel log's last 20 lines,
   from the `xhci 00:14.0: port 15:` lines to init's three, and
   `Press any key to reboot.`
   The line `xhci 00:14.0: slot 4: interface 0 class 8/6/80, …` is wider
   than the screen and takes two rows, and the heading stays at the top.
   Photograph it, wait a few seconds (it waits for the key, however long),
   then press a key on the K120: the NUC restarts. Choose the stick again
   with F10: the motd and the prompt.
10. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
    screen says `System halted. It is now safe to power off.` instead, note
    the `relay:` line above it and hold the power button.
11. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
    lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
    `/root/notes/t`, and the last lines are `system.img: built <time>`,
    `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`,
    `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run <time>)`,
    `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run <time>)`
    and `/root/checks/check5.sh: ok, 29 of 29 commands as expected (run <time>)`,
    with the UTC times of the four runs. A transcript older than the
    stick's `system.img` (a run before the last `flash --kernel`, which
````

with:

````markdown

### Check 6 — control flow

9. Type `sh checks/check6.sh`. It runs for a few seconds:
   - `if`, `elif` and `else` written across lines inside a `for`, and an
     `if` that takes no branch (status 0);
   - a `while` and an `until` loop, each ended by a file it removes or
     makes;
   - `for` over words, and over `"$@"` in a script it writes and runs
     with three arguments, one empty and one with two blanks
     (`for a in "$@"` and `for a do`);
   - the statuses of `&&`, `||` and `!`;
   - `test` and `[` on the stick's files: `/bin` is read-only
     (`[ -w /bin/ls ]` is false) and `/root` is not, `-nt` and `-ot` of
     two files made a second apart, and an invalid integer
     (`[: invalid integer 'x'`, status 2);
   - 100 programs (`true`) in two nested `for` loops of 10 words between
     two `date` lines. Note both times for the results log: more than
     10 s apart means that a sync after every command is too slow on the
     stick (the programmable shell gate's §13), so say so.

   The prompt comes back after `+ done` and the second `date`.

   Then three steps by hand:
   - `for x in a b c` and Enter: the prompt `> ` asks for more. Then
     `do echo $x` (`> ` again) and `done`: `a`, `b` and `c`, each on a
     line of its own.
   - `if true` and Enter, then Ctrl-C at `> `: `^C` and the prompt
     `root@relay:~# `; `echo $?` prints `130`.
   - `while true; do sleep 1; done`, and after a few seconds Ctrl-C: `^C`
     and the prompt come back at once; `echo $?` prints `130`.

   Photograph the screen.

### The error screen, power off and verify-usb

10. The error screen: `reboot`, and choose the stick again with F10, so
    that the kernel log's last lines are the boot's. At the prompt type
    `exit` three times within 10 s. After the first two, init's line and
    a new prompt; after the third, `init: /bin/sh (pid <n>) exited with
    <status>` (0 unless the command before `exit` failed) and the error
    screen: `*** Relay OS cannot run its shell ***`,
    `/bin/sh ended 3 times within 10 s`, the kernel log's last 20 lines,
    from the `xhci 00:14.0: port 15:` lines to init's three, and
    `Press any key to reboot.`
    The line `xhci 00:14.0: slot 4: interface 0 class 8/6/80, …` is wider
    than the screen and takes two rows, and the heading stays at the top.
    Photograph it, wait a few seconds (it waits for the key, however
    long), then press a key on the K120: the NUC restarts. Choose the
    stick again with F10: the motd and the prompt.
11. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
    screen says `System halted. It is now safe to power off.` instead, note
    the `relay:` line above it and hold the power button.
12. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
    lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
    `/root/notes/t`, and the last lines are `system.img: built <time>`,
    `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`,
    `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run <time>)`,
    `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run <time>)`,
    `/root/checks/check5.sh: ok, 29 of 29 commands as expected (run <time>)`
    and `/root/checks/check6.sh: ok, 46 of 46 commands as expected (run <time>)`
    (each line of a loop written across lines counts), with the UTC times
    of the five runs. A transcript older than the
    stick's `system.img` (a run before the last `flash --kernel`, which
````

Replace:

````markdown
| `check5.sh`: `ps \| grep -c t-spin` prints another count than 1 before `kill %1`, or than 0 after `wait %1` | A `t-spin` of an earlier command still runs, or the job's did not start | `verify-usb` names the line; `ps` at the prompt lists the processes |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

with:

````markdown
| `check5.sh`: `ps \| grep -c t-spin` prints another count than 1 before `kill %1`, or than 0 after `wait %1` | A `t-spin` of an earlier command still runs, or the job's did not start | `verify-usb` names the line; `ps` at the prompt lists the processes |
| `check6.sh` prints `while once` or `until once` over and over, with an `rm` or `touch` error | The loop's file was not removed or made, so the loop runs on | Photograph the screen, then Ctrl-C: `^C` and the prompt come back; `ls /root/check6-go /root/check6-stop` shows which file is left |
| `check6.sh`'s two `date` lines are more than 10 s apart | A sync after every command is slow on the stick | Note both times in the results log: a later plan syncs once per command line instead (the programmable shell gate's §13) |
| Ctrl-C at `> ` by hand gives no `^C`, or no prompt | The line editor did not get the K120's Ctrl-C | Photograph the screen; Enter, or a second Ctrl-C, shows whether the shell still reads the keyboard |
| Ctrl-C in the `while` loop by hand gives no `^C`, or no prompt | The Ctrl-C reached neither `sleep` nor the shell's check between its own commands (`wait` for a Ctrl-C), or was lost after `sleep` ended | Photograph the screen; a second Ctrl-C shows whether the loop can end; `dmesg` after it shows a `pid <n> (/bin/sleep): killed: Ctrl-C` line if the first reached `sleep` |
| The error screen with `/bin/sh cannot start: …` or `/bin/sh ended 3 times within 10 s` | The shell cannot be loaded, or ends as soon as it starts (its `init: /bin/sh (pid N) …` lines, in the log's lines on the screen, say how) | Photograph the screen; a key restarts the machine |
````

- [ ] **Step 3: Change `rootfs/root/README`**

In `rootfs/root/README`, replace:

````text
  NAME=value     then $NAME or ${NAME}; a value is never split into words
````

with:

````text
  NAME=value     then $NAME or ${NAME}; a value is never split into words
Control flow (help shows the forms):
  a; b    a && b    a || b    ! a    if [ -f FILE ]; then a; else b; fi
  while test -f FILE; do a; done    for x in one two; do echo $x; done
````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add README.md docs rootfs
git commit -F - <<'EOF'
docs: add check 6, control flow, to the NUC checklist

docs/hardware-test.md gains check 6: what check6.sh runs, its two
times for the results log and the 10 seconds that would make a sync
after every command too slow, the three steps by hand (a for across
lines, Ctrl-C at > and in a while loop), its verify-usb line and its
failure rows; its checklist names the programmable shell gate's spec.
The README's quick start runs check6.sh, and the stick's /root/README
shows the control flow.
EOF
````


### Task 12: `check6.sh`, NUC check 6

Spec §11.4 and decisions 2, 3 and 4: milestone 4's NUC check, 46 commands whose lines are the same on the NUC and in QEMU but for the two times: `if`, `elif` and `else` across lines in a `for`; `while` and `until` ended by a file each makes or removes; `for` over words and over `"$@"` in a helper it writes and runs with three arguments (one empty, one holding two blanks), not in itself, since a script run inside itself empties its own transcript; the statuses of `&&`, `||` and `!`; `test` and `[` on the stick's files (`/bin` read-only, `/root` not, `-nt` and `-ot` of files made a second apart, an invalid integer); and 100 programs in two nested loops between two `date` lines. Its output, but `date`'s time zone, is bash 5.2's with GNU's programs (`tmp/m4p4/probes/c6.bash`). The `checks` scenario runs it after `check5.sh` and checks its transcript; that QEMU transcript is recorded as the scenario's disk holds it, and the NUC's is a copy until the NUC run records it. The red run is xtask's tests, which cannot read the new script and transcripts. Mutation checks (5): a changed `$?`, message, count and a dropped line of the helper's output each fail the test.

**Files:**
- Create: `rootfs/root/checks/check6.sh`
- Modify: `tests/e2e/checks.txt`
- Create: `xtask/fixtures/checks/check6.nuc.log`
- Create: `xtask/fixtures/checks/check6.qemu.log`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Task 11's instructions; `checks::{parse, check}`; the `checks` scenario.
- Produces: `rootfs/root/checks/check6.sh`, `xtask/fixtures/checks/check6.{qemu,nuc}.log`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/checks.txt`**

In `tests/e2e/checks.txt`, make these 3 replacements, top to bottom:

Replace:

````text
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# checks 4 and 5. Afterwards their transcripts on the disk must show what
# the scripts expect.
timeout 30
````

with:

````text
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# checks 4, 5 and 6. Afterwards their transcripts on the disk must show
# what the scripts expect.
timeout 30
````

Replace:

````text
expect root@relay:~# $
poweroff
````

with:

````text
expect root@relay:~# $
send sh checks/check6.sh
expect \n\+ rm -f /root/check6-a\.sh .*\n
expect \n\+ date\n.*\n\+ for a in .*\n(.*\n){3}\+ date\n
expect root@relay:~# $
poweroff
````

Replace:

````text
check-script /root/checks/check5.sh
````

with:

````text
check-script /root/checks/check5.sh
check-script /root/checks/check6.sh
````

- [ ] **Step 2: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// which has `[` and `test` by hand until the next NUC run (spec §15
    /// item 3). A `#nuc>` line must
    /// not need a line of its own next to the `#>` line for the same
````

with:

````rust
    /// which has `[` and `test` by hand until the next NUC run (spec §15
    /// item 3), and check 6's, a copy of QEMU's until its first NUC run
    /// (spec §15 item 4). A `#nuc>` line must
    /// not need a line of its own next to the `#>` line for the same
````

Replace:

````rust
                include_str!("../fixtures/checks/check5.nuc.log"),
            ),
````

with:

````rust
                include_str!("../fixtures/checks/check5.nuc.log"),
            ),
            (
                "check6.sh",
                include_str!("../../rootfs/root/checks/check6.sh"),
                include_str!("../fixtures/checks/check6.qemu.log"),
                include_str!("../fixtures/checks/check6.nuc.log"),
            ),
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask checks`

Expected: FAIL: compile errors such as `` couldn't read `xtask/src/../../rootfs/root/checks/check6.sh`: No such file or directory (os error 2) ``; `` couldn't read `xtask/src/../fixtures/checks/check6.qemu.log`: No such file or directory (os error 2) ``.

- [ ] **Step 4: Create `rootfs/root/checks/check6.sh`**

Create `rootfs/root/checks/check6.sh`:

````bash
# NUC check 6 (docs/hardware-test.md; milestone 4, plan 4): after
# check5.sh, `sh checks/check6.sh`. Lists, compound commands and `test`,
# run by /bin/sh. Its transcript is check6.log; the `#>` lines are
# explained in check3-a.sh. A construct written across lines is traced
# line by line before it runs, so its output's `#>` lines follow its last
# line, and each of its lines counts as a command. Only the time differs
# between the NUC and QEMU.

# Start afresh, so the check can be run again.
rm -f /root/check6-a.sh /root/check6-a.log /root/check6-go /root/check6-stop /root/check6-old /root/check6-new

# if, elif and else, across lines and on one.
for n in 1 2 3
do
  if [ $n = 1 ]
  then
    echo one
  elif [ $n = 2 ]; then
    echo two
  else
    echo other $n
  fi
done
#> one
#> two
#> other 3
if false; then echo no; fi
echo $?
#> 0

# while and until, each ended by a file it makes or removes.
touch /root/check6-go
while [ -f /root/check6-go ]; do rm /root/check6-go; echo while once; done
#> while once
until [ -f /root/check6-stop ]; do touch /root/check6-stop; echo until once; done
#> until once
ls /root/check6-go /root/check6-stop
#> ls: cannot access '/root/check6-go': No such file or directory
#> /root/check6-stop

# for over words, and over "$@" in a script given arguments: another
# script, since one run inside itself would empty its own transcript.
for w in a 'b c' "$E" d
do echo "[$w]"
done
#> \[a\]
#> \[b c\]
#> \[\]
#> \[d\]
echo 'for a in "$@"; do echo "in [$a]"; done' > /root/check6-a.sh
echo 'for a do echo "do [$a]"; done' >> /root/check6-a.sh
sh /root/check6-a.sh one '' 'two  words'
#> \+ for a in "\$@"; do echo "in \[\$a\]"; done
#> in \[one\]
#> in \[\]
#> in \[two  words\]
#> \+ for a do echo "do \[\$a\]"; done
#> do \[one\]
#> do \[\]
#> do \[two  words\]

# The statuses of &&, || and !.
true && echo and
#> and
false || echo or
#> or
false && echo no; echo $?
#> 1
! true; echo $?
#> 1
! false; echo $?
#> 0
true && false || echo both
#> both

# test and [ on the stick's files.
[ -d /bin ] && [ -f /bin/ls ] && [ -x /bin/sh ] && echo bin
#> bin
test -s /root/checks/check6.sh && [ -w /root ] && echo root
#> root
[ -w /bin/ls ]; echo $?
#> 1
[ -e /root/nothing ] || [ ! -e /root/nothing ] && echo nothing
#> nothing
touch /root/check6-old
sleep 1
touch /root/check6-new
[ /root/check6-new -nt /root/check6-old ] && [ /root/check6-old -ot /root/check6-new ] && echo newer
#> newer
[ 3 -lt 10 ] && [ abc = abc ] && [ abc != abd ] && echo compare
#> compare
[ 1 -eq x ]; echo $?
#> \[: invalid integer 'x'
#> 2

# 100 programs, each followed by a sync, timed: the results log records
# the two times (spec §13).
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d
for a in 0 1 2 3 4 5 6 7 8 9
do
  for b in 0 1 2 3 4 5 6 7 8 9; do true; done
done
date
#> (Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d
````

- [ ] **Step 5: Create `xtask/fixtures/checks/check6.nuc.log`**

Create `xtask/fixtures/checks/check6.nuc.log`:

````text
+ rm -f /root/check6-a.sh /root/check6-a.log /root/check6-go /root/check6-stop /root/check6-old /root/check6-new
+ for n in 1 2 3
+ do
+ if [ $n = 1 ]
+ then
+ echo one
+ elif [ $n = 2 ]; then
+ echo two
+ else
+ echo other $n
+ fi
+ done
one
two
other 3
+ if false; then echo no; fi
+ echo $?
0
+ touch /root/check6-go
+ while [ -f /root/check6-go ]; do rm /root/check6-go; echo while once; done
while once
+ until [ -f /root/check6-stop ]; do touch /root/check6-stop; echo until once; done
until once
+ ls /root/check6-go /root/check6-stop
ls: cannot access '/root/check6-go': No such file or directory
/root/check6-stop
+ for w in a 'b c' "$E" d
+ do echo "[$w]"
+ done
[a]
[b c]
[]
[d]
+ echo 'for a in "$@"; do echo "in [$a]"; done' > /root/check6-a.sh
+ echo 'for a do echo "do [$a]"; done' >> /root/check6-a.sh
+ sh /root/check6-a.sh one '' 'two  words'
+ for a in "$@"; do echo "in [$a]"; done
in [one]
in []
in [two  words]
+ for a do echo "do [$a]"; done
do [one]
do []
do [two  words]
+ true && echo and
and
+ false || echo or
or
+ false && echo no; echo $?
1
+ ! true; echo $?
1
+ ! false; echo $?
0
+ true && false || echo both
both
+ [ -d /bin ] && [ -f /bin/ls ] && [ -x /bin/sh ] && echo bin
bin
+ test -s /root/checks/check6.sh && [ -w /root ] && echo root
root
+ [ -w /bin/ls ]; echo $?
1
+ [ -e /root/nothing ] || [ ! -e /root/nothing ] && echo nothing
nothing
+ touch /root/check6-old
+ sleep 1
+ touch /root/check6-new
+ [ /root/check6-new -nt /root/check6-old ] && [ /root/check6-old -ot /root/check6-new ] && echo newer
newer
+ [ 3 -lt 10 ] && [ abc = abc ] && [ abc != abd ] && echo compare
compare
+ [ 1 -eq x ]; echo $?
[: invalid integer 'x'
2
+ date
Sat Oct  3 01:33:13 UTC 2026
+ for a in 0 1 2 3 4 5 6 7 8 9
+ do
+ for b in 0 1 2 3 4 5 6 7 8 9; do true; done
+ done
+ date
Sat Oct  3 01:33:13 UTC 2026
````

- [ ] **Step 6: Create `xtask/fixtures/checks/check6.qemu.log`**

Create `xtask/fixtures/checks/check6.qemu.log`:

````text
+ rm -f /root/check6-a.sh /root/check6-a.log /root/check6-go /root/check6-stop /root/check6-old /root/check6-new
+ for n in 1 2 3
+ do
+ if [ $n = 1 ]
+ then
+ echo one
+ elif [ $n = 2 ]; then
+ echo two
+ else
+ echo other $n
+ fi
+ done
one
two
other 3
+ if false; then echo no; fi
+ echo $?
0
+ touch /root/check6-go
+ while [ -f /root/check6-go ]; do rm /root/check6-go; echo while once; done
while once
+ until [ -f /root/check6-stop ]; do touch /root/check6-stop; echo until once; done
until once
+ ls /root/check6-go /root/check6-stop
ls: cannot access '/root/check6-go': No such file or directory
/root/check6-stop
+ for w in a 'b c' "$E" d
+ do echo "[$w]"
+ done
[a]
[b c]
[]
[d]
+ echo 'for a in "$@"; do echo "in [$a]"; done' > /root/check6-a.sh
+ echo 'for a do echo "do [$a]"; done' >> /root/check6-a.sh
+ sh /root/check6-a.sh one '' 'two  words'
+ for a in "$@"; do echo "in [$a]"; done
in [one]
in []
in [two  words]
+ for a do echo "do [$a]"; done
do [one]
do []
do [two  words]
+ true && echo and
and
+ false || echo or
or
+ false && echo no; echo $?
1
+ ! true; echo $?
1
+ ! false; echo $?
0
+ true && false || echo both
both
+ [ -d /bin ] && [ -f /bin/ls ] && [ -x /bin/sh ] && echo bin
bin
+ test -s /root/checks/check6.sh && [ -w /root ] && echo root
root
+ [ -w /bin/ls ]; echo $?
1
+ [ -e /root/nothing ] || [ ! -e /root/nothing ] && echo nothing
nothing
+ touch /root/check6-old
+ sleep 1
+ touch /root/check6-new
+ [ /root/check6-new -nt /root/check6-old ] && [ /root/check6-old -ot /root/check6-new ] && echo newer
newer
+ [ 3 -lt 10 ] && [ abc = abc ] && [ abc != abd ] && echo compare
compare
+ [ 1 -eq x ]; echo $?
[: invalid integer 'x'
2
+ date
Sat Oct  3 01:33:13 UTC 2026
+ for a in 0 1 2 3 4 5 6 7 8 9
+ do
+ for b in 0 1 2 3 4 5 6 7 8 9; do true; done
+ done
+ date
Sat Oct  3 01:33:13 UTC 2026
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask checks`

Expected: PASS: 14 tests.

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
test(checks,e2e): add check6.sh, control flow on the NUC

Check 6 runs if, elif and else across lines, while and until loops
ended by files, for over words and over "$@" in a helper it writes and
runs with arguments, the statuses of &&, || and !, test and [ on the
stick's files, and 100 programs in two nested loops between two date
lines, whose times the results log records. The checks scenario runs
it after check 5; its QEMU transcript is recorded, and the NUC's is a
copy of it until the NUC runs it.
EOF
````


### Task 13: Version 0.5.0

Spec §1.4 and decision 10: milestone 4 is version 0.5.0, as milestone 3 was 0.4.0. The spike bumped it first and ran every scenario: only `uname`'s unit test, the `shell` scenario, `check3-a.sh`'s `uname -a` line and the recorded transcripts depend on it. The transcripts' `Relay relay 0.5.0 x86_64` and `Relay OS 0.5.0` lines are changed by `sed`, since the NUC's hold escape bytes: QEMU's as the `checks` scenario's disk now holds them, the NUC's until the NUC run records them. `docs/hardware-test.md`'s check 1 names the new version; the results log keeps its rows. The red runs are `uname`'s test and `shell`; `Cargo.toml` goes in after them, and cargo updates `Cargo.lock`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/shell.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`
- Modify: `xtask/fixtures/checks/check3-b.qemu.log`

**Interfaces:**
- Consumes: `env!("CARGO_PKG_VERSION")` (the kernel's banner and `uname`).
- Produces: version 0.5.0.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 3 is version 0.4.0 (user-space gate §16 item 11), from
        // Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.4.0 x86_64\n".into()));
        assert_eq!(
````

with:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 4 is version 0.5.0 (programmable shell gate §15 item
        // 4), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.5.0 x86_64\n".into()));
        assert_eq!(
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/shell.txt`**

In `tests/e2e/shell.txt`, replace:

````text
send uname -a
expect \nRelay relay 0\.4\.0 x86_64\n
expect root@relay:~# $
````

with:

````text
send uname -a
expect \nRelay relay 0\.5\.0 x86_64\n
expect root@relay:~# $
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::uname_prints_the_system`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: FAIL: scenario `shell` stops at line 9, timed out waiting for `\nRelay relay 0\.5\.0 x86_64\n`.

- [ ] **Step 4: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
[workspace.package]
version = "0.4.0"
edition = "2024"
````

with:

````toml
[workspace.package]
version = "0.5.0"
edition = "2024"
````

- [ ] **Step 5: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.4.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

with:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.5.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

- [ ] **Step 6: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
uname -a
#> Relay relay 0\.4\.0 x86_64
date
````

with:

````bash
uname -a
#> Relay relay 0\.5\.0 x86_64
date
````

- [ ] **Step 7: Change `xtask/fixtures/checks/check3-a.nuc.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.4\.0 x86_64$/Relay relay 0.5.0 x86_64/; s/^Relay OS 0\.4\.0$/Relay OS 0.5.0/' xtask/fixtures/checks/check3-a.nuc.log
````

- [ ] **Step 8: Change `xtask/fixtures/checks/check3-a.qemu.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.4\.0 x86_64$/Relay relay 0.5.0 x86_64/; s/^Relay OS 0\.4\.0$/Relay OS 0.5.0/' xtask/fixtures/checks/check3-a.qemu.log
````

- [ ] **Step 9: Change `xtask/fixtures/checks/check3-b.nuc.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.4\.0 x86_64$/Relay relay 0.5.0 x86_64/; s/^Relay OS 0\.4\.0$/Relay OS 0.5.0/' xtask/fixtures/checks/check3-b.nuc.log
````

- [ ] **Step 10: Change `xtask/fixtures/checks/check3-b.qemu.log` (it may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.4\.0 x86_64$/Relay relay 0.5.0 x86_64/; s/^Relay OS 0\.4\.0$/Relay OS 0.5.0/' xtask/fixtures/checks/check3-b.qemu.log
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 384 tests.

- [ ] **Step 12: Run the `shell`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add Cargo.lock Cargo.toml crates docs rootfs tests xtask
git commit -F - <<'EOF'
chore(release): 0.5.0

Milestone 4 is version 0.5.0 (programmable shell gate §1.4, §15 item
4), as milestone 3 was 0.4.0: the workspace's version, from which the
kernel's banner and `uname -a` take theirs (`Relay relay 0.5.0
x86_64`). The shell scenario and check3-a.sh's `uname -a` line follow,
and so do the recorded transcripts' `uname -a` and `Relay OS` lines,
QEMU's as the checks scenario's disk now holds them and the NUC's
until the NUC run records them, and docs/hardware-test.md's check 1.
EOF
````


### Task 14: The documents say milestone 4 is done

Decision 10: `AGENTS.md`'s status line and the README say what milestone 4 added and that it is version 0.5.0, the README names the programmable shell gate's spec, and milestone 4's roadmap says that its four plans were executed. Documentation only.

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md`

**Interfaces:**
- Consumes: Task 13.
- Produces: nothing new.

- [ ] **Step 1: Change `AGENTS.md`**

In `AGENTS.md`, replace:

````markdown

Status: milestones 1 to 3 are done, at version 0.4.0 (tag `v0.4.0`). The
kernel boots, drives xHCI (keyboard and USB storage), mounts an ext2 root
and runs programs in ring 3 from a read-only `/bin` (`system.img`).
Process 1 is the kernel's init, which runs `/bin/sh`; the shell has
pipes, background jobs, scripts with arguments and variables, and six
built-ins (`cd`, `exit`, `help`, `jobs`, `kill`, `wait`). Every other
command is a program. The next gate, being designed, is the programmable
shell: milestone 4 (control flow, 0.5.0) and milestone 5 (redirection and
environment, 0.6.0). The aarch64 port comes later.

````

with:

````markdown

Status: milestones 1 to 4 are done, at version 0.5.0 (tag `v0.5.0`). The
kernel boots, drives xHCI (keyboard and USB storage), mounts an ext2 root
and runs programs in ring 3 from a read-only `/bin` (`system.img`).
Process 1 is the kernel's init, which runs `/bin/sh`; the shell has
pipes, background jobs, scripts with arguments and variables, lists
(`;`, `&&`, `||`, `!`), `if`, `while`, `until` and `for`, commands read
across lines with bash's `> ` prompt, and six built-ins (`cd`, `exit`,
`help`, `jobs`, `kill`, `wait`). Every other command is a program, `test`
and `[` too. The programmable shell gate goes on with milestone 5
(redirection and environment, 0.6.0). The aarch64 port comes later.

````

- [ ] **Step 2: Change `README.md`**

In `README.md`, replace:

````markdown

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0 and milestone 3
version 0.4.0, which ends the user-space gate.

Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
(milestone 1) and `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`
(milestones 2 and 3: the shell and its commands as programs in ring 3).

````

with:

````markdown

Milestone 4 makes the shell one to program in: lists (`a; b`,
`a && b`, `a || b`, `! a`, `a & b`), commands typed across lines with
bash's `> ` prompt, `if`, `while`, `until` and `for`, and `test` and `[`
(`[ -f FILE ]`) as programs, with `grep -q`.

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0, milestone 3
version 0.4.0, which ends the user-space gate, and milestone 4 version
0.5.0.

Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
(milestone 1), `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`
(milestones 2 and 3: the shell and its commands as programs in ring 3)
and `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md`
(milestones 4 and 5: control flow, redirection and the environment).

````

- [ ] **Step 3: Change `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md`**

In `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md`, replace:

````markdown

**Status:** Plans 1 to 3 are done (#99–#101, #103–#106, #107–#111); plan 4 is planned and lands with these notes.

````

with:

````markdown

**Status:** Done. All four plans were executed; milestone 4 ended at version 0.5.0 (tag `v0.5.0`).

````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add AGENTS.md README.md docs
git commit -F - <<'EOF'
docs: say that milestone 4 is done, at version 0.5.0

AGENTS.md's status line and the README say what milestone 4 added
(lists, commands across lines with the > prompt, if, while, until and
for, test and [ as programs, grep -q) and that it is version 0.5.0; the
README names the programmable shell gate's spec, and milestone 4's
roadmap says that all four plans were executed.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p4/release
gh pr create --draft --base main --head m4p4/release --title "chore(release): 0.5.0, check6.sh and NUC checks 3–6" --body-file - <<'EOF'
## What

Milestone 4, plan 4, tasks 11–14: `check6.sh` (`if`/`elif`/`else`, `while` and `until` ended by files, `for` over words and `"$@"`, the statuses of `&&`, `||` and `!`, `test` on the stick's files, 100 programs timed; 46 commands) in the `checks` scenario and in `docs/hardware-test.md` with three steps by hand, its QEMU transcript recorded; version 0.5.0 (`uname -a` says `Relay relay 0.5.0 x86_64`); the README, `AGENTS.md` and the roadmap say milestone 4 is done.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `checks`

## Hardware

- [x] Needed: NUC checks 3 to 6 of `docs/hardware-test.md` (check 3's scripts and steps by hand, `check4.sh`, `check5.sh` and `exit` by hand, `check6.sh` and its steps by hand, the error screen), run by the user on a stick written by `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for NUC checks 3 to 6**

Ask the user to run NUC checks 3 to 6 (`docs/hardware-test.md`, steps 1–12: check 3's scripts and its three steps by hand, `check4.sh`, `check5.sh` and `exit` by hand, `check6.sh` and its three steps by hand, the error screen, `poweroff` and `verify-usb`) on a stick written by `cargo xtask flash --full` from this worktree, and to report their results, with the two times `check6.sh`'s `date` lines printed; the pull request stays a draft until they do. The stick is then usually plugged into this machine, its root mounted at `/media/maw/relayroot`.

- [ ] **Record the NUC's transcripts in this pull request**

After a pass: check the stick with `cargo xtask verify-usb` (it only reads it), copy the real transcripts over the recorded ones (they replace `check4.nuc.log`, edited by hand in plan 3, `check6.nuc.log`, a copy of QEMU's, and those edited by hand to say `0.5.0`), and add the results-log row of `docs/hardware-test.md` (date, `3 to 6 (milestone 4 done, 0.5.0)`, this branch's commit, `Pass`, and the notes: the startup lines, the five scripts' counts, the steps by hand, check 6's two times, the error screen and the K120 key, anything the user saw). If the two times are more than 10 seconds apart, add the roadmap's note for milestone 5 (decision 4: a later plan syncs once per top-level command and after each program); otherwise §13's risk is closed by the row.

````bash
cargo xtask verify-usb
cp /media/maw/relayroot/root/checks/check3-a.log xtask/fixtures/checks/check3-a.nuc.log
cp /media/maw/relayroot/root/checks/check3-b.log xtask/fixtures/checks/check3-b.nuc.log
cp /media/maw/relayroot/root/checks/check4.log xtask/fixtures/checks/check4.nuc.log
cp /media/maw/relayroot/root/checks/check5.log xtask/fixtures/checks/check5.nuc.log
cp /media/maw/relayroot/root/checks/check6.log xtask/fixtures/checks/check6.nuc.log
cargo test -p xtask checks
````

Expected: `verify-usb` ends with `system.img: built …` and the five scripts' `ok` lines, and the tests pass. Then `git diff` shows only the transcripts and the new row (check that no earlier row changed), and:

````bash
cargo xtask ci
git add xtask/fixtures/checks docs/hardware-test.md
git commit -m "test(checks): record NUC checks 3 to 6 on 0.5.0"
git push
gh pr ready m4p4/release
````

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. Propose the text of the `v0.5.0` tag and the GitHub release "Relay OS 0.5.0 — milestone 4" (as milestone 3's `v0.4.0`; the plan's "Release notes" section has a draft), and make neither unless the user asks. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p4-release
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
