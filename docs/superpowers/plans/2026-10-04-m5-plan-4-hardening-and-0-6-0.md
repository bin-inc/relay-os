# Milestone 5 · Plan 4: Hardening and 0.6.0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Milestone 5, and with it the programmable shell gate, ends: `check7.sh` runs redirection, `/dev/null`, the environment and `cd -` on the NUC and in the `checks` scenario; the deferred minors of milestone 5's plans 1–3 and of milestone 4's plan 4 are settled, among them a `/bin/sh` whose own output is a file (`X | sh > f`) and a pipeline stage's message after `2>&1`; version 0.6.0; and NUC checks 3 to 7 pass on a stick written by `flash --full`, every NUC transcript recorded again. The spec's §15 item 8 lands with this plan. It ends with `cargo xtask ci` green, 52 scenarios, the NUC's transcripts recorded, and the user's tag `v0.6.0`.

**Architecture:** Small changes in many places, no new module. The parser keeps whether each redirection's fd was typed (`Parts::explicit`); `export` refuses `NAME+=value`; `Vars::held_back` counts what a built-in's held assignments would bring back; a pipeline stage's message rides in `runner::Stage::into_pipe` to the pipe after it, capped at `runner::PIPED_MESSAGE_MAX` so the shell never waits on a pipe whose reader has not started; `Shell::with_output_redirected` holds `/bin/sh`'s fd 1 as a redirection file when it is not the console; a built-in's write error is named `relay-sh:`. xtask's `image::failure` says how mcopy ended; the `environment` and `redirect` scenarios grow. A new check script, `rootfs/root/checks/check7.sh`, with its QEMU transcript; the version and the documents.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; bash 5.2.21 and GNU coreutils 9.4 for comparison; mtools; QEMU 8.2 under KVM with `-cpu max`; the NUC 12 Pro and its Kingston stick for the last pull request.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§7, §8.5, §9.2, §10, §11.4, §11.5, §12, §15 items 5–8)
**Roadmap:** `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md` — this is plan 4 of 4 of milestone 5, the last of the gate.

## In brief

- **Size.** 20 tasks in three code pull requests, plus this plan as PR 1: PR 2 the shell's deferred minors (refusals naming the fd, `export NAME+=value`, held assignments, a stage's message into its pipe, `/bin/sh`'s own fd 1, a built-in's write error, three comments) with the review's fixes; PR 3 xtask's and the scenarios' (`verify-usb`, t-abi's message, `env` with Ctrl-C and a killed command); PR 4 `check7.sh`, version 0.6.0 and the documents, a draft until the user's NUC checks 3 to 7, whose transcripts and results-log row go into a commit of it.
- **The spike** (decision 1): version 0.6.0 and a first `check7.sh` in the `checks` scenario: besides the version's places, only the fixtures test's list, the instructions test and one line of the draft that §7.3 refuses failed.
- **The maintainer's decisions:** the pull-request split, and every carried minor fixed, `X | sh > f` and the pipeline's message included (decisions 1, 3, 4); a built-in's write error named as the shell's, `help` and `jobs` still saying it (decision 8); `Shell::with_output_redirected` a `feat` (decision 3).
- **bash 5.2 and GNU decide the details** (probes in `tmp/m5p4/progress.md`): check 7's every line, a failed redirection's message in the pipe, what `X | bash > f` sends where, `export A+=b` typed and expanded, `export -p`'s `OLDPWD`.
- **The prototype's review.** A fresh reviewer read the whole prototype (then sixteen tasks), ran `cargo xtask ci` on PR 2 alone, the `checks` scenario and the `environment` scenario thirteen times (three under TCG), throwaway unit tests and throwaway QEMU scenarios, compared check 7 with bash 5.2 and GNU's programs line by line, and probed bash in a pty. It found 0 critical, 1 important and 3 minor real defects, and beside them two older items and a nit. The important one, older than this plan (a stage that cannot start writes its message into a pipe whose reader has not started, so a 16 KiB command name hung `/bin/sh` for good), and two minors (a held name the built-in set still kept room; `< f &` refused as `> &`) are settled in tasks of their own after the tasks they concern, each with a test that fails first; the third minor (an unanchored scenario step) and the nit (a paragraph to reflow) are folded into their tasks. A built-in's write error without `relay-sh:` came to the user, who chose the prefix (decision 8), and so did the commit type of `Shell::with_output_redirected` (decision 3). It found correct: `explicit` kept beside every redirection, `export`'s refusal before anything is exported, `held_back`'s balance on every path, both runners' piped message and its order, the shell's fd 1 never closed on any path (refusals, compound commands, background jobs, scripts), the prompt and job reports on fd 2, EPIPE ending the shell as bash's ends, the new scenario lines not vacuous and the Ctrl-C step stable, check 7 against bash (a second run starts afresh, its transcript byte for byte the disk's, 39 commands), the checklist's numbering, the version in every place, every commit's widths and scopes, and PR 2 green on its own. It declined to judge whether Task 8 is a `feat`, the expanded word in `export`'s refusal, and a commit body's wording (both settled at the rebuild).

| Finding (review) | Decision |
|---|---|
| Important (I-1): a pipeline stage that cannot start wrote its message into the next stage's pipe before that stage started; one over 16 KiB (a 16 KiB command name) blocked `/bin/sh` for good, Ctrl-C only a byte | Fixed, Task 7 (the same 8 KiB cap as Task 6, both runners) |
| Minor (m-1): Task 4 kept room for held names the built-in set or exported, which `release` keeps, so `A=1 export A B=<40K>` was refused | Fixed, Task 5 |
| Minor (m-2): a redirection alone before `&` was refused as `> &` whatever was typed (`< f &`) | Fixed, Task 2 |
| Minor (m-3): the `env t-fault` step did not check that nothing but the kernel's line comes before `139` | Fixed in Task 15 (anchored at the command's echo) |
| Older (P-1): a built-in's write error said `cd: write error: …`, where bash says `bash: cd: write error: …` | Fixed, Task 9 (the user: the prefix; `help` and `jobs` keep reporting it, §10) |
| Nit (P-2): a line of Task 12's paragraph not reflowed | Fixed in Task 12 |

## Where this plan fits

Plan 4 of milestone 5 is the gate's eighth and last step (spec §12). It builds on plans 1–3, whose redirection, ABI 4, `/dev/null` and environment its `check7.sh` runs on the NUC, and settles what they and milestone 4's plan 4 left for it (the roadmap's notes). The next gate is not chosen yet.

## Working conventions

- Plan 4 lands as **four pull requests** (table below). This plan, with the spec's §15 item 8 (with the §10 line and status line it changes) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-04-m5-plan-4-hardening-and-0-6-0.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
  The check-3 transcripts' version lines change by a command step: the NUC's hold escape bytes, which a markdown block cannot carry.
- Each task first adds its failing tests, runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 10, 11, 12, 14, 15, 16, 17 and 20 have no failing run (tests of behaviour that exists, a message, or documents); each says why. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m5p4/mutations.md`); a review fix's red run is its own mutant.
- **The expectations are the real tools'.** bash 5.2 with `env -i HOME=/root LC_ALL=C`, in a pty where it matters, and with `/root` bound to a scratch directory under `unshare -rm` for check 7 (`tmp/m5p4/probes/`); GNU coreutils 9.4; mtools. In this session's shell `grep` may be a wrapper of another tool: any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop and every wait.** What a person types is untrusted: nothing that follows from it may panic, overflow, allocate without bound or wait for good, and a write into a pipe whose reader has not started must fit what the pipe takes at once.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; `test` only for a commit of tests alone; a new public API is `feat`; the review's fixes carry `Refs: review <id>`, earlier reviews' `Refs: plan <n> final review <id>`. Every commit body line is within 72 columns, counted in characters. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. The scenario parser refuses an input that no `expect` of the prompt paces. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `d687751` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request, review and NUC steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m5p4/plan` | — | The spec's §15 item 8 with its §10 line and status line, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m5p4/shell` | 1–12 | The shell's deferred minors and the review's fixes; three comments | `lint`, `unit`, `e2e` |
| 3 | `m5p4/xtask` | 13–15 | `verify-usb`'s reason, t-abi's message, `env` with Ctrl-C and a killed command | `lint`, `unit`, `e2e` |
| 4 | `m5p4/release` | 16–20 | Check 7 in the checklist and `check7.sh`; version 0.6.0; the documents; NUC checks 3 to 7 (a draft until the user's run) | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline; no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 4 adds no crate.
- **No ABI change:** `relay_abi` is untouched; `relay_abi::VERSION` stays 4.
- Every scenario and check script of milestone 5's plan 3 passes, changed only where this plan's fixes change what they print (each listed in its task); the `checks` scenario gains check 7, `shell` the version.
- What a person types and what a program passes are untrusted (AGENTS.md): no panic, unchecked index, overflow, unbounded allocation, endless loop or endless wait may follow.
- The shell follows bash 5.2 and commands GNU word for word; the only decided differences are spec §10's and §15's.
- Missing tools fail tests, never skip them: bash, GNU's coreutils, mtools and debugfs are on CI's runner.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 8 in PR 1:

1. **The pull requests.** Plan 4 is four: its plan; the deferred minors in the shell, with plan 2's two comments in the kernel and the vfs and the fixes the prototype's review asked for; xtask's and the scenarios' (`verify-usb`'s reason, a test's message, `/bin/env` with Ctrl-C and a killed command on target); `check7.sh`, version 0.6.0 and NUC checks 3 to 7, a draft until the maintainer's run, whose transcripts copied off the stick and results-log row go into a commit of that pull request. Milestone 5 and the gate end with it. A spike set the version to 0.6.0 and ran a first `check7.sh` in the `checks` scenario: besides the version's places (AGENTS.md), only the fixtures test's list of scripts, the test that the instructions run every script, and one line of the draft that §7.3 refuses (`> before |`) failed.
2. **Check 7** (§11.4). `check7.sh` is started in `/root/checks` (`cd checks`, then `sh check7.sh`) and starts with `cd /root`, so its `OLDPWD` shows where it was started. Its 39 commands run `[ -c /dev/null ]`, every standard stream redirected to and from `/dev/null` and files (`2>&1` in both orders and into a pipe, `>>`, `2>>`, a `for` loop's output); `export`, `env` with `-i` and `-u`, `B=2 env` and `$B` afterwards, a nested `sh` that sees exported and assigned values, `unset`; `cd -`, `PWD` and `OLDPWD`, `cd ..` and `cd`. Its output is bash 5.2's with GNU's programs (but `cd`'s message, named `relay-sh`, and the nested script's trace), the same on both machines, and it has no steps by hand; its NUC transcript is a copy of QEMU's until the NUC run. The test that the instructions run every check script takes check 7's `cd checks` and `sh check7.sh`.
3. **A shell whose standard output is not the screen** (§7.5). A `/bin/sh` whose own fd 1 is not the console (`X | sh > f`, `X | sh | cat`, `sh > f` typed at a prompt) holds it as a file it opened, for good, so a built-in's standard output and the shell's and a built-in's messages after `2>&1` go there, as bash's do; until now they reached the screen, since relay-rt's console writes fd 2. Its other messages, its prompt and its job reports stay on the screen (plan 1's review, M-3; the maintainer, 2026-10-04). `Shell::with_output_redirected` is new public API, so its commit is a `feat` (the maintainer).
4. **A message into a pipe before its reader starts** (§7.3). A stage's failed redirection is told where its fd 2 stood, which after `2>&1` is the pipe: `cat 2>&1 < /nope | wc -l` prints `1`, as bash's does (plan 1's final review, M-1; the maintainer, 2026-10-04). Such a message, and that of a stage that cannot start, is written into the fresh pipe before the next stage starts, so one over 8 KiB, half of what a pipe holds, is told on the screen instead and the shell never waits for good: only a path longer than `PATH_MAX` or a command name of 8 KiB makes one (the prototype's review found a 16 KiB name hanging `/bin/sh`, I-1).
5. **Refusals name the fd as typed** (§7.3): `1> f | cat` is `unsupported syntax: 1> before |`, `0< f | cat` `0< before |`, and a redirection alone before `&` names itself (`< f &` is `< &`, where it said `> &`) (plan 1's final review, M-3; the prototype's review, m-2).
6. **Held assignments** (§8.5, item 7). While a built-in holds an assignment, a held name the built-in has not set or exported counts the larger of its earlier and its current size in the variables' 64 KiB, so the variables never hold more than `VARS_MAX` once it is released; one it set keeps its value and counts only that (plan 3's final review, m-1; the prototype's review, m-1).
7. **`export NAME+=value`** (§8.5) is `unsupported syntax: A+=b`, status 2, as `NAME+=value` is elsewhere, typed or expanded (bash appends in both), and `export` then exports nothing; the message shows the word as expanded. A bad name before `+=` keeps ``not a valid identifier``, status 1, as bash's (plan 3's final review, m-2).
8. **A built-in's write error** is the shell's, as bash's: `relay-sh: export: write error: No space left on device`; a program's keeps GNU's `echo: write error: …`. `help` and `jobs` still say it, status 1, where bash's say nothing (§10; the prototype's review, P-1; the maintainer, 2026-10-04).
9. **Item 7's `OLDPWD`.** Item 7 says bash lists no `export -p` line for an `OLDPWD` without a value; it lists `declare -x OLDPWD` at the start and hides it only when an imported `OLDPWD` names no directory (plan 3's final review, m-4). The shell lists it in both cases, as before.
10. **Plan 2's and plan 3's other deferred minors.** `kernel/src/file.rs`'s comment on a device's `seek` and offsets (m-2) and the `FileSystem` contract's sentences for devices (m-3) are corrected; xtask's `3 -> 2` is built from the constants (m-4); every NUC transcript, edited by hand since plan 2, is recorded again (m-5). The scenario `environment` runs `env t-spin` with Ctrl-C and `env t-fault`, its kernel line anchored (plan 3's m-5), and `verify-usb` says how mcopy ended when it gives no reason (milestone 4's plan 4, M-3).
11. **The prototype's review** found no critical defect, one important (I-1, above) and three minor ones (m-1, m-2, above; m-3, the anchored `env t-fault` step), each fixed in a task of its own with a test that fails first or folded into the task it concerns, and beside them a built-in's write error (P-1, above) and a comment to reflow.

## Review Focus

The final whole-branch review should look hardest where this plan touches what the shell writes and where, most likely first:

1. **The shell's own fd 1 held as a file** (`X | sh > f`, `X | sh | cat`, `sh > f`): built-ins' output and errors, `2>&1` and `1>&2`, compound commands with redirections, pipelines and jobs, scripts, `exit`, Ctrl-C; the entry never closed; the prompt, line editor and job reports on the screen; against `X | bash > f`.
2. **Messages written into a pipe before its reader starts:** a failed redirection's and a stage's that cannot start, in both runners, the 8 KiB cap, the order with the stage's output, every fd closed; no other write the shell makes before a reader exists.
3. **Held assignments and `VARS_MAX`:** `held_back` on every path that grows the variables while a name is held, names the built-in changed passed over, nothing refused that fits.
4. **Refusals and `export NAME+=value`** against bash and §15 item 8, and a built-in's write error (`relay-sh:` for built-ins only).
5. **Check 7 and the release:** `check7.sh` against bash 5.2 with GNU's programs, a second run, its transcripts, `checks.txt`, the checklist, the version's places, the documents' claims, no results-log row changed.
6. **xtask and the `environment` lines:** `verify-usb`'s reason, the anchored `env` steps.
7. **Each pull request green and safe on its own**, and every commit against `CONTRIBUTING.md`.

## Release notes (proposed for `v0.6.0`)

To be proposed to the user after PR 4's NUC checks pass, as the GitHub release "Relay OS 0.6.0 — milestone 5"; neither the tag nor the release is made without their word.

> Relay OS 0.6.0 ends milestone 5, "Redirection and environment", and with it the programmable shell gate.
>
> - **Redirection:** `<`, `>`, `>>`, `2>`, `2>>`, `2>&1`, `1>&2`, several per command made left to right, on compound commands too (`for …; done > f`), and `a 2>&1 | b`.
> - **`/dev/null`**, in a filesystem of the kernel's at `/dev`.
> - **Environments (ABI 4):** every program gets one; the shell imports its own, exports variables (`export`, `unset`), runs `A=1 cmd`, and `/bin/env` shows and changes one, as GNU's.
> - **`cd`** follows `HOME`, `PWD` and `OLDPWD` (`cd -`); `~` and the prompt follow `HOME`; the check scripts pass from any directory.
> - NUC checks 3 to 7 (`docs/hardware-test.md`) passed on the NUC (on the date of the results-log row PR 4 adds), on a stick written by `cargo xtask flash --full`: six scripts of 84, 5, 33, 29, 46 and 39 commands, the steps by hand, and the error screen. `cargo xtask verify-usb` checked their transcripts.

---

## PR 1: The spec's §15 item 8, the roadmap's notes and this plan

The spec's §15 item 8, the decisions of this plan, with the §10 line and the status line it changes; the roadmap's status and its notes on plan 3's minors; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and the roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 5 plan 4's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p4/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m5p4/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-04-m5-plan-4-hardening-and-0-6-0.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-04-m5-plan-4-hardening-and-0-6-0.md
git commit -m "docs(plan): add milestone 5 plan 4, hardening and 0.6.0"
cargo xtask lint
git push -u origin m5p4/plan
gh pr create --base main --head m5p4/plan --title "docs(spec,plan): add milestone 5 plan 4, hardening and 0.6.0" --body-file - <<'EOF2'
## What

The implementation plan of milestone 5's plan 4 ("Hardening and 0.6.0"), the last of the programmable shell gate, with the spec's §15 item 8: four pull requests, the last a draft until NUC checks 3 to 7, and the spike; check 7, started in `/root/checks`; a `/bin/sh` whose own fd 1 is not the screen writing there (`X | sh > f`); a stage's message into its pipe after `2>&1`, never one long enough to block the shell (the prototype's review found a 16 KiB name hanging it); refusals naming the fd as typed; held assignments within `VARS_MAX`; `export NAME+=value` refused; a built-in's write error named as the shell's; item 7's `OLDPWD` sentence corrected; the other deferred minors; version 0.6.0. §10 gains `help`'s and `jobs`' write error; the roadmap's notes carry plan 3's minors.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (the harness's `bind_pr`); never poll CI. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m5p4/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-plan` (`.superpowers` is not linked there) and continue with PR 2.

---

## PR 2: The shell's deferred minors (Tasks 1–12)

Refusals name a redirection's fd as typed; `export NAME+=value` is unsupported syntax; a built-in's held assignments keep room under `VARS_MAX`; a pipeline stage's message goes into its pipe after `2>&1`, never one long enough to block the shell; a `/bin/sh` whose fd 1 is a file or a pipe writes there; a built-in's write error is named as the shell's; three stale comments.

Branch `m5p4/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p4/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p4/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-shell m5p4/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p4/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: A refusal names a redirection's fd as typed

Decision 5 (spec §7.3; plan 1's final review, M-3): a refusal in a pipeline named a redirection's operator without its fd when the fd was the default, so `1> f | cat` said `> before |` and `a | cat 0< f` `< after |`. The parser now keeps, beside each redirection of a command (`Parts::explicit`), whether its fd was typed, and every pipeline refusal names the operator as typed: `1> before |`, `0< before |`, `0< after |`, `| 1>`, `| 1>&2`. The red run is the shell's tests: the refusals drop the typed fd. Mutation checks (5): the operator ignoring `explicit`, and each of `<`, `>`/`>>` and `>&` never explicit, and `0< after |` not looked for, each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: the parser's `Parts` and `Redirect::operator`.
- Produces: `Parts::typed(which)`, `Redirect::operator(explicit)` (private to the parser).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            ("cat | cat < f | wc", "< after |"),
        ] {
````

with:

````rust
            ("cat | cat < f | wc", "< after |"),
            ("cat | cat 0< f", "0< after |"),
            ("cat | cat 0< f | wc", "0< after |"),
        ] {
````

Replace:

````rust
            ("a >> f | b", ">>"),
            ("a 1> f | b", ">"),
            ("a 2>&1 > f | b", ">"),
        ] {
````

with:

````rust
            ("a >> f | b", ">>"),
            ("a 1> f | b", "1>"),
            ("a 2>&1 > f | b", ">"),
            ("a 2>&1 1>> f | b", "1>>"),
        ] {
````

Replace:

````rust
            ("a | 2>&1", "| 2>&1"),
        ] {
````

with:

````rust
            ("a | 2>&1", "| 2>&1"),
            // An fd typed before its operator is named too, though it is
            // the default (plan 1's final review, M-3).
            ("1> f | cat", "1> before |"),
            ("1>> f | cat", "1>> before |"),
            ("0< f | cat", "0< before |"),
            ("1>&2 | cat", "1>&2 before |"),
            ("a | 1> f", "| 1>"),
            ("a | 0< f", "| 0<"),
            ("a | 1>&2", "| 1>&2"),
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `parser::tests::a_bar_needs_a_command_on_each_side`, `parser::tests::every_command_of_a_pipeline_has_a_name`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 14 replacements, top to bottom:

Replace:

````rust
impl<W> Redirect<W> {
    /// Its operator, as a refusal names it (`<`, `2>>`, `>&2`).
    fn operator(&self) -> String {
        let fd = |default| {
            if self.fd == default {
                String::new()
````

with:

````rust
impl<W> Redirect<W> {
    /// Its operator, as a refusal names it (`<`, `2>>`, `>&2`): its fd
    /// too when that was typed (`explicit`) or is not the default.
    fn operator(&self, explicit: bool) -> String {
        let fd = |default| {
            if self.fd == default && !explicit {
                String::new()
````

Replace:

````rust
    redirects: Vec<Redirect<Word>>,
    /// A redirection waiting for its word.
````

with:

````rust
    redirects: Vec<Redirect<Word>>,
    /// Whether each redirection's fd was typed (`1>`, `0<`), which a
    /// refusal names.
    explicit: Vec<bool>,
    /// A redirection waiting for its word.
````

Replace:

````rust
enum Pending {
    /// `<` on fd 0: a file name.
    Read,
    /// `>` or `>>` (`append`) on `fd`: a file name.
    File { fd: u32, append: bool },
    /// `>&` on `fd`, as typed (`2>&` or `>&`): the fd it copies.
````

with:

````rust
enum Pending {
    /// `<` on fd 0, which was typed (`explicit`) or not: a file name.
    Read { explicit: bool },
    /// `>` or `>>` (`append`) on `fd`, typed or not: a file name.
    File {
        fd: u32,
        append: bool,
        explicit: bool,
    },
    /// `>&` on `fd`, as typed (`2>&` or `>&`): the fd it copies.
````

Replace:

````rust
        match self.pending.take() {
            Some(Pending::Read) => self.redirects.push(Redirect {
                fd: 0,
                op: RedirectOp::Read(w),
            }),
            Some(Pending::File { fd, append }) => {
                let op = if append {
````

with:

````rust
        match self.pending.take() {
            Some(Pending::Read { explicit }) => {
                self.redirects.push(Redirect {
                    fd: 0,
                    op: RedirectOp::Read(w),
                });
                self.explicit.push(explicit);
            }
            Some(Pending::File {
                fd,
                append,
                explicit,
            }) => {
                let op = if append {
````

Replace:

````rust
                self.redirects.push(Redirect { fd, op });
            }
````

with:

````rust
                self.redirects.push(Redirect { fd, op });
                self.explicit.push(explicit);
            }
````

Replace:

````rust
                    op: RedirectOp::Copy(copied),
                });
            }
            // After a compound command, as in bash, only a keyword that
````

with:

````rust
                    op: RedirectOp::Copy(copied),
                });
                self.explicit.push(typed != ">&");
            }
            // After a compound command, as in bash, only a keyword that
````

Replace:

````rust

    /// The command so far, ended by a `|`, which needs one before it.
````

with:

````rust

    /// The operator, as typed, of the first redirection that `which`.
    fn typed(&self, which: impl Fn(&Redirect<Word>) -> bool) -> Option<String> {
        let i = self.redirects.iter().position(which)?;
        let explicit = self.explicit.get(i).is_some_and(|&e| e);
        Some(self.redirects[i].operator(explicit))
    }

    /// The command so far, ended by a `|`, which needs one before it.
````

Replace:

````rust
        // (programmable shell gate §7.3); errors may go anywhere.
        if self.later && self.redirects.iter().any(|r| r.fd == 0) {
            return Err(ParseError::Unsupported("< after |".into()));
        }
        // A redirection alone, which bash runs, is refused like an output
        // file: each names what was typed.
        let refused = match self.redirects.first() {
            Some(first) if self.words.is_empty() => Some(first),
            _ => self.redirects.iter().find(|r| r.is_output_file()),
        };
        if let Some(r) = refused {
            return Err(ParseError::Unsupported(format!(
                "{} before |",
                r.operator()
            )));
        }
````

with:

````rust
        // (programmable shell gate §7.3); errors may go anywhere.
        if self.later
            && let Some(read) = self.typed(|r| r.fd == 0)
        {
            return Err(ParseError::Unsupported(format!("{read} after |")));
        }
        // A redirection alone, which bash runs, is refused like an output
        // file: each names what was typed.
        let refused = if self.words.is_empty() {
            self.typed(|_| true)
        } else {
            self.typed(Redirect::is_output_file)
        };
        if let Some(op) = refused {
            return Err(ParseError::Unsupported(format!("{op} before |")));
        }
````

Replace:

````rust
            (false, false) => {
                let first = parts.redirects.first().map(Redirect::operator);
                return Err(ParseError::Unsupported(format!(
````

with:

````rust
            (false, false) => {
                let first = parts.typed(|_| true);
                return Err(ParseError::Unsupported(format!(
````

Replace:

````rust
    }
    let p = core::mem::take(parts);
    if !pipeline.is_empty() && p.redirects.iter().any(|r| r.fd == 0) {
        return Err(ParseError::Unsupported("< after |".into()));
    }
    pipeline.push(command(p.words, p.redirects)?);
````

with:

````rust
    }
    if !pipeline.is_empty()
        && let Some(read) = parts.typed(|r| r.fd == 0)
    {
        return Err(ParseError::Unsupported(format!("{read} after |")));
    }
    let p = core::mem::take(parts);
    pipeline.push(command(p.words, p.redirects)?);
````

Replace:

````rust
                    } else {
                        Pending::File { fd, append }
                    });
````

with:

````rust
                    } else {
                        Pending::File {
                            fd,
                            append,
                            explicit: !typed.is_empty(),
                        }
                    });
````

Replace:

````rust
                    // are refused, as are its here-documents, `<&` and `<>`.
                    if let Some(digits) = self.fd_word()?
                        && digits != "0"
````

with:

````rust
                    // are refused, as are its here-documents, `<&` and `<>`.
                    let digits = self.fd_word()?;
                    if let Some(digits) = &digits
                        && digits != "0"
````

Replace:

````rust
                    }
                    self.parts.pending = Some(Pending::Read);
                }
````

with:

````rust
                    }
                    self.parts.pending = Some(Pending::Read {
                        explicit: digits.is_some(),
                    });
                }
````

Replace:

````rust
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. } | Pending::Read) => {
                return Err(ParseError::Unexpected(digits));
````

with:

````rust
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. } | Pending::Read { .. }) => {
                return Err(ParseError::Unexpected(digits));
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 461 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): name a refused redirection's fd as typed

A refusal in a pipeline named a redirection's operator without its fd
when the fd was the default, so `1> f | cat` said `> before |` and
`a | cat 0< f` `< after |`. Keep whether each fd was typed and name it
as it was typed: `1> before |`, `0< after |`, `| 1>&2`.

Refs: plan 1 final review M3
EOF
````


### Task 2: A redirection alone before `&` is named as typed

The prototype's review (m-2), the same rule as Task 1's: a command of redirections alone before `&` was refused as `> &` whatever was typed, so `< f &` named an operator that was not there. It now names the first redirection as typed (`< &`, `2> &`, `1> &`, `2>&1 &`). The red run is the shell's tests: each says `> &`. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 1's `Parts::typed`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
            ("> f &", "> &"),
            // bash runs this (the review found it called its syntax
````

with:

````rust
            ("> f &", "> &"),
            // Each names the redirection as typed (the prototype's review,
            // m-2).
            ("< f &", "< &"),
            ("2> e &", "2> &"),
            ("1> f &", "1> &"),
            ("2>&1 &", "2>&1 &"),
            // bash runs this (the review found it called its syntax
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::an_ampersand_anywhere_else_is_bash_s_error_or_unsupported`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
                    if self.parts.words.is_empty() {
                        // `> f &`: a background job is a program.
                        return Err(ParseError::Unsupported("> &".into()));
                    }
````

with:

````rust
                    if self.parts.words.is_empty() {
                        // `> f &`: a background job is a program. Named as
                        // typed (`< f &`, `2> e &`).
                        let op = self.parts.typed(|_| true).unwrap_or_default();
                        return Err(ParseError::Unsupported(format!("{op} &")));
                    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 461 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): name a redirection alone before & as typed

A command of redirections alone before `&` was refused as `> &`
whatever was typed, so `< f &` named an operator that was not there.
It now names the first redirection as typed: `< &`, `2> &`, `1> &`.

Refs: review m2
EOF
````


### Task 3: `export NAME+=value` is unsupported syntax

Decision 7 (spec §8.5; plan 3's final review, m-2): `export A+=b` said ``export: `A+=b': not a valid identifier``, status 1, where the shell's rule for bash's `NAME+=value`, which appends, is `unsupported syntax: A+=b`, status 2. bash appends whether the word is typed or comes from an expansion (`X=A+=b; export $X`, probe p5), so `export` refuses any word `NAME+=value` with a valid name before it exports anything; a bad name before `+=` (`1A+=b`) keeps `not a valid identifier`, as bash's. The red run is the shell's tests: each says `not a valid identifier`. Mutation checks (3): any stem taken, status 1, and no refusal, each fail the test.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`

**Interfaces:**
- Consumes: `commands::export::export`, `shell::SYNTAX`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
    #[test]
    fn export_s_other_options_are_refused() {
````

with:

````rust
    #[test]
    fn appending_with_export_is_unsupported_and_exports_nothing() {
        // bash appends, typed or expanded (probes p3, p5); here it is the
        // shell's rule for `NAME+=value`, status 2, before anything is
        // exported (plan 3's final review, m-2).
        let mut h = Harness::new();
        for (line, said) in [
            ("export A+=b", "A+=b"),
            ("export B=1 A+=b C", "A+=b"),
            ("export -p A+=b", "A+=b"),
            ("X=A+=b; export $X", "A+=b"),
            ("export A+=", "A+="),
        ] {
            let message = alloc::format!("relay-sh: unsupported syntax: {said}\n");
            assert_eq!(
                h.lines(&["A=a", line, "unset OLDPWD PWD", "export", "echo $A"]),
                (0, alloc::format!("{message}a\n")),
                "{line}"
            );
            assert_eq!(h.run(line).0, 2, "{line}");
        }
        // A bad name before `+=` is not a valid identifier, as in bash.
        assert_eq!(
            h.run("export 1A+=b"),
            (
                1,
                "relay-sh: export: `1A+=b': not a valid identifier\n".into()
            )
        );
    }

    #[test]
    fn export_s_other_options_are_refused() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::export::tests::appending_with_export_is_unsupported_and_exports_nothing`.

- [ ] **Step 3: Change `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::parser::is_name;
use crate::shell::NAME;
use alloc::format;
````

with:

````rust
use crate::parser::is_name;
use crate::shell::{NAME, SYNTAX};
use alloc::format;
````

Replace:

````rust
/// the exported variables as bash's `declare -x` does. `-n` and `-f`
/// (bash's un-export and functions) are not supported.
pub fn export(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust
/// the exported variables as bash's `declare -x` does. `-n` and `-f`
/// (bash's un-export and functions) are not supported, nor is bash's
/// `NAME+=value`, which appends: a word of that shape exports nothing.
pub fn export(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
        return list(ctx);
    }
````

with:

````rust
        return list(ctx);
    }
    // The shell's rule for `NAME+=value` (programmable shell gate §8.5),
    // typed or expanded.
    let appends = |arg: &&String| {
        arg.split_once('=')
            .and_then(|(before, _)| before.strip_suffix('+'))
            .is_some_and(is_name)
    };
    if let Some(append) = names.iter().find(appends) {
        ctx.fail(NAME, format_args!("unsupported syntax: {append}"));
        return SYNTAX;
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 462 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse export's NAME+=value as unsupported syntax

`export A+=b` said `not a valid identifier`, status 1, where the
shell's rule for bash's NAME+=value, which appends, is
`unsupported syntax: A+=b`, status 2. bash appends whether the word is
typed or expanded, so export refuses any such word before it exports
anything; a bad name before += stays not a valid identifier, as in
bash.

Refs: plan 3 final review m2
EOF
````


### Task 4: A held assignment keeps room for the value it put aside

Decision 6 (spec §8.5, §15 item 7; plan 3's final review, m-1): an assignment before a built-in puts the old value back when the built-in ends, but while it ran the variables counted only the new one, so `A=<40K>; A= export B=<40K>` left them past `VARS_MAX` once `A` came back, and nothing more could be set until an `unset`. `Vars::put` now adds what releasing the held names would bring back (`Vars::held_back`): each counts the larger of its size before and its size now. The red run is the shell's tests: `B` is exported, then `C=1` does not fit. Mutation checks (4): the held names not counted, the size before counted whole, the new value of the held name ignored (a survivor at first, killed by the case of a held name given its old value again) and the larger size counted twice, each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: `Vars::{hold, release, put}`.
- Produces: `Vars::held_back` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
    #[test]
    fn a_variable_export_cannot_hold_fails() {
````

with:

````rust
    #[test]
    fn a_name_held_for_export_keeps_its_room() {
        // Plan 3's final review, m-1: `A` comes back after `export`, so `B`
        // may not take its room, and later assignments still fit.
        let mut h = Harness::new();
        let big = "x".repeat(40 * 1024);
        assert_eq!(
            h.lines(&[
                &alloc::format!("A={big}"),
                &alloc::format!("A= export B={big}"),
                "C=1",
                "echo $C $B",
            ]),
            (
                0,
                "relay-sh: B: the variables would hold more than 64 KiB\n1\n".into()
            )
        );
    }

    #[test]
    fn a_variable_export_cannot_hold_fails() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
````

with:

````rust
    #[test]
    fn a_held_name_keeps_room_for_what_it_held() {
        // `A=<40K>; A= export B=<40K>` left the variables over `VARS_MAX`
        // once `A` came back, and nothing more could be set (plan 3's final
        // review, m-1): while held, a name counts the larger of its sizes.
        let mut v = Vars::new("sh");
        let big = "x".repeat(40 * 1024);
        v.set("A", big.clone()).unwrap();
        v.hold(&[String::from("A=")]).unwrap();
        assert_eq!(
            v.export("B", Some(big.clone())),
            Err(Error::Full("B".into()))
        );
        v.set("B", "x".repeat(VARS_MAX - big.len() - 2)).unwrap();
        v.release();
        assert_eq!((v.value("A"), v.size), (Some(big.as_str()), VARS_MAX));
        // Removed while held, it still comes back.
        v.unset("B");
        v.hold(&[String::from("A=")]).unwrap();
        v.unset("A");
        assert_eq!(v.set("C", big.clone()), Err(Error::Full("C".into())));
        v.release();
        assert_eq!(v.value("A"), Some(big.as_str()));
        // Given its old value again while held, it fits as it did.
        v.hold(&[String::from("A=")]).unwrap();
        v.set("A", big.clone()).unwrap();
        v.release();
        assert_eq!(v.value("A"), Some(big.as_str()));
        // A held name given more counts what it holds now, once: the rest
        // fits exactly.
        v.hold(&[alloc::format!("A={big}y")]).unwrap();
        v.set("D", "x".repeat(VARS_MAX - big.len() - 3)).unwrap();
        assert_eq!(v.size, VARS_MAX);
        v.release();
        assert_eq!((v.value("A"), v.size), (Some(big.as_str()), VARS_MAX - 1));
    }

    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::export::tests::a_name_held_for_export_keeps_its_room`, `vars::tests::a_held_name_keeps_room_for_what_it_held`.

- [ ] **Step 4: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Stores `var` as `name`, unless the variables would then hold more
    /// than `VARS_MAX`.
    fn put(&mut self, name: &str, var: Var) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| size(name, v));
        let new = self.size - old + size(name, &var);
        if new > VARS_MAX {
            return Err(Error::Full(String::from(name)));
````

with:

````rust
    /// Stores `var` as `name`, unless the variables would then hold more
    /// than `VARS_MAX`, or would once a built-in releases what it holds.
    fn put(&mut self, name: &str, var: Var) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| size(name, v));
        let new = self.size - old + size(name, &var);
        if new + self.held_back(name, &var) > VARS_MAX {
            return Err(Error::Full(String::from(name)));
````

Replace:

````rust
        self.names.insert(String::from(name), var);
        Ok(())
    }
}
````

with:

````rust
        self.names.insert(String::from(name), var);
        Ok(())
    }
}

impl Vars {
    /// What releasing the names a built-in holds would add, were `name` to
    /// hold `var`: each held name counts the larger of its size before and
    /// its size now, so that it fits when it comes back.
    fn held_back(&self, name: &str, var: &Var) -> usize {
        self.held
            .iter()
            .map(|h| {
                let before = h.before.as_ref().map_or(0, |v| size(&h.name, v));
                let now = if h.name == name {
                    size(name, var)
                } else {
                    self.names.get(&h.name).map_or(0, |v| size(&h.name, v))
                };
                before.saturating_sub(now)
            })
            .sum()
    }
}
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 464 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): keep room for what a built-in's assignment holds

An assignment before a built-in puts the old value back when the
built-in ends, but while it ran the variables counted only the new
one: `A=<40K>; A= export B=<40K>` left them past VARS_MAX once A came
back, and nothing could be set until an unset. While a name is held,
count the larger of its size before and its size now.

Refs: plan 3 final review m1
EOF
````


### Task 5: A held name the built-in set keeps no room

The prototype's review (m-1): `release` keeps the value of a held name the built-in set or exported, so its old value never comes back, but Task 4 still kept room for it: `A=1 export A B=<40K>`, with a 40 KiB `A`, refused `B`, which bash and the shell before took. `held_back` passes over the held names the built-in changed. The red run is the shell's tests: `B` and `C` are refused. Mutation checks (2): changed names counted and nothing counted each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 4's `held_back`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
    #[test]
    fn a_variable_export_cannot_hold_fails() {
````

with:

````rust
    #[test]
    fn a_name_export_sets_while_held_gives_its_room_back() {
        // The prototype's review, m-1: `export` keeps the `A` it set, so
        // `B` fits, as in bash.
        let mut h = Harness::new();
        let big = "x".repeat(40 * 1024);
        assert_eq!(
            h.lines(&[
                &alloc::format!("A={big}"),
                &alloc::format!("A=1 export A B={big}"),
                "echo $A",
                "unset B",
                &alloc::format!("A={big}"),
                &alloc::format!("A= export A=x C={big}"),
                "echo $A",
            ]),
            (0, "1\nx\n".into())
        );
    }

    #[test]
    fn a_variable_export_cannot_hold_fails() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
````

with:

````rust
    #[test]
    fn a_held_name_the_built_in_set_keeps_no_room() {
        // `release` keeps what the built-in set or exported, so its old
        // value never comes back (the prototype's review, m-1): `A=1
        // export A B=<40K>` fits, as in bash.
        let mut v = Vars::new("sh");
        let big = "x".repeat(40 * 1024);
        v.set("A", big.clone()).unwrap();
        v.hold(&[String::from("A=1")]).unwrap();
        v.export("A", None).unwrap();
        v.set("B", big.clone()).unwrap();
        v.release();
        assert_eq!((v.get("A"), v.value("B")), ("1", Some(big.as_str())));
        assert!(v.size <= VARS_MAX);
    }

    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::export::tests::a_name_export_sets_while_held_gives_its_room_back`, `vars::tests::a_held_name_the_built_in_set_keeps_no_room`.

- [ ] **Step 4: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    /// hold `var`: each held name counts the larger of its size before and
    /// its size now, so that it fits when it comes back.
    fn held_back(&self, name: &str, var: &Var) -> usize {
        self.held
            .iter()
            .map(|h| {
````

with:

````rust
    /// hold `var`: each held name counts the larger of its size before and
    /// its size now, so that it fits when it comes back; one the built-in
    /// set or exported keeps what it holds, and counts nothing more.
    fn held_back(&self, name: &str, var: &Var) -> usize {
        self.held
            .iter()
            .filter(|h| !h.changed)
            .map(|h| {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 466 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): keep no room for a held name the built-in set

A built-in that sets or exports a name it holds keeps its value when
it ends, so the old value never comes back; the room kept for it
refused assignments that fit, as `A=1 export A B=<40K>` with a 40 KiB
A, which bash and the shell before took. A held name the built-in
changed counts only what it holds now.

Refs: review m1
EOF
````


### Task 6: A stage's failed redirection is told into its pipe

Decision 4 (spec §7.3; plan 1's final review, M-1): a pipeline's redirections are made before its pipes (`Shell::stage_fds`), so a stage's failed redirection was told on the screen even when its fd 2 stood on the pipe after it: `cat 2>&1 < /nope | wc -l` printed the message and `0`, where bash prints `1` (probe p1). `stage_fds` keeps such a message with the stage (`runner::Stage::into_pipe`), and each runner sends it into the pipe: `/bin/sh` writes it into the fresh pipe before the next stage starts, so a message over `PIPED_MESSAGE_MAX` (8 KiB, half of the 16 KiB a pipe takes at once; only a path longer than `PATH_MAX` makes one) is told on the screen instead, and the shell never waits on the pipe. The red run is the shell's tests, which cannot compile without the constant; the prototype wrote this fix with its test, and its mutant that never pipes, the old behaviour, fails them. Mutation checks (5): never piped, no cap, the spawning runner writing nothing (unit test and the `redirect` scenario) and the in-process runner piping nothing, each fail.

**Files:**
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `tests/e2e/redirect.txt`

**Interfaces:**
- Consumes: `Shell::redirect`, `Files::redirect`, `Spawning::start`, `InProcess::pipeline`.
- Produces: `Shell::made` (private), `runner::Stage::into_pipe`, `PIPED_MESSAGE_MAX` in `shell.rs` (Task 7 moves it to `runner.rs`).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod tests {
    use crate::Shell;
````

with:

````rust
mod tests {
    use super::PIPED_MESSAGE_MAX;
    use crate::Shell;
````

Replace:

````rust
        );
        // Under /bin/sh: each command gets the pipes and its files.
````

with:

````rust
        );
        // After `2>&1` the message goes into the pipe, as bash's does
        // (probe p1; plan 1's final review, M-1); a failure before `2>&1`
        // is told where fd 2 was then.
        assert_eq!(h.run("cat 2>&1 < /nope | wc -l"), (0, "1\n".into()));
        assert_eq!(
            h.run("echo a | cat 2>&1 2> /nodir/e | cat"),
            (0, "relay-sh: /nodir/e: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("cat < /nope 2>&1 | wc -l"),
            (0, "relay-sh: /nope: No such file or directory\n0\n".into())
        );
        // One longer than a pipe takes at once stays on the screen.
        let long = alloc::format!("/{}", "x".repeat(PIPED_MESSAGE_MAX));
        assert_eq!(
            h.run(&alloc::format!("cat 2>&1 < {long} | wc -l")),
            (
                0,
                alloc::format!("relay-sh: {long}: File name too long\n0\n")
            )
        );
        // Under /bin/sh: each command gets the pipes and its files.
````

Replace:

````rust
        assert_eq!(h.programs.spawned.len(), 4, "the first ran");
    }
````

with:

````rust
        assert_eq!(h.programs.spawned.len(), 4, "the first ran");
        // A failed redirection after `2>&1`: its message into the pipe.
        h.programs.written.clear();
        assert_eq!(
            h.spawning("t-args 2>&1 < /nope | t-args"),
            (3, String::new())
        );
        let (_, write) = *h.programs.pipes.last().unwrap();
        assert_eq!(
            h.programs.written,
            [(
                write,
                b"relay-sh: /nope: No such file or directory\n".to_vec()
            )]
        );
        assert!(h.programs.closed.contains(&write));
        let long = alloc::format!("/{}", "x".repeat(PIPED_MESSAGE_MAX));
        assert_eq!(
            h.spawning(&alloc::format!("t-args 2>&1 < {long} | t-args")),
            (
                3,
                alloc::format!("relay-sh: {long}: No such file or directory\n")
            )
        );
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/redirect.txt`**

In `tests/e2e/redirect.txt`, replace:

````text
expect \n2\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

with:

````text
expect \n2\nroot@relay:~# $
# A redirection that fails after `2>&1` tells it into the pipe (plan 4).
send cat 2>&1 < r-nope | wc -l
expect \n1\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` unresolved import `super::PIPED_MESSAGE_MAX` ``.

- [ ] **Step 4: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub assigns: &'c [String],
}
````

with:

````rust
    pub assigns: &'c [String],
    /// The message of its redirection that failed, which goes into the
    /// pipe after it.
    pub into_pipe: Option<&'c str>,
}
````

Replace:

````rust
            let (Some(fds), Some((name, args))) = (stage.fds, stage.words.split_first()) else {
                piped = Some(Bytes::new(out.0));
                continue;
````

with:

````rust
            let (Some(fds), Some((name, args))) = (stage.fds, stage.words.split_first()) else {
                let message = stage.into_pipe.unwrap_or_default();
                piped = Some(Bytes::new(message.as_bytes().to_vec()));
                continue;
````

Replace:

````rust
                // Its words expanded to nothing or a redirection of it
                // failed: it runs nothing, and its neighbours see an end.
                for fd in [stdin, stdout].into_iter().flatten() {
````

with:

````rust
                // Its words expanded to nothing or a redirection of it
                // failed: it runs nothing, its message goes into the pipe
                // after it if its fd 2 was that (a fresh pipe takes it at
                // once), and its neighbours see an end.
                if let (Some(message), Some(fd)) = (stage.into_pipe, stdout) {
                    let _ = self.programs.write(fd, message.as_bytes());
                }
                for fd in [stdin, stdout].into_iter().flatten() {
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
const MOTD_MAX: usize = 16 * 1024;

````

with:

````rust
const MOTD_MAX: usize = 16 * 1024;
/// The longest message of a failed redirection that goes into the pipe
/// after its command, before the pipe's reader starts: half of the 16 KiB
/// a pipe holds, so the write never waits. Any path the vfs takes
/// (`PATH_MAX`, 4 KiB) fits; a longer one's (`File name too long`) is told
/// on the screen.
const PIPED_MESSAGE_MAX: usize = 8 * 1024;

````

Replace:

````rust
        if ran.own
            && let Some(Some(fds)) = all.last()
        {
            self.say_on(*fds, message.as_bytes());
            message.clear();
        }
        for fds in all.into_iter().flatten() {
            self.release(fds);
````

with:

````rust
        if ran.own
            && let Some((Some(fds), _)) = all.last()
        {
            self.say_on(*fds, message.as_bytes());
            message.clear();
        }
        for fds in all.into_iter().filter_map(|(fds, _)| fds) {
            self.release(fds);
````

Replace:

````rust
        let started = self.runner.get().background(parts, &staged, &self.vars);
        for fds in all.into_iter().flatten() {
            self.release(fds);
````

with:

````rust
        let started = self.runner.get().background(parts, &staged, &self.vars);
        for fds in all.into_iter().filter_map(|(fds, _)| fds) {
            self.release(fds);
````

Replace:

````rust
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Option<Fds> {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        match self.files.redirect(base, redirects, &mut opener) {
            Ok(fds) => Some(fds),
            Err(failed) => {
                let message = format!("{NAME}: {}: {}\n", failed.path, failed.error);
                self.say_on(failed.fds, message.as_bytes());
                self.release(failed.fds);
                None
            }
        }
    }
````

with:

````rust
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Option<Fds> {
        match self.made(base, redirects) {
            Ok(fds) => Some(fds),
            Err((fds, message)) => {
                self.say_on(fds, message.as_bytes());
                self.release(fds);
                None
            }
        }
    }

    /// The context `redirects` make over `base`, or what was made of it
    /// when one cannot be made, and its message.
    fn made(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Result<Fds, (Fds, String)> {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        self.files
            .redirect(base, redirects, &mut opener)
            .map_err(|failed| {
                let message = format!("{NAME}: {}: {}\n", failed.path, failed.error);
                (failed.fds, message)
            })
    }
````

Replace:

````rust
    /// its redirections made over them; none for a stage whose redirection
    /// could not be made, which runs nothing.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Vec<Option<Fds>> {
        let last = stages.len() - 1;
````

with:

````rust
    /// its redirections made over them; none for a stage whose redirection
    /// could not be made, which runs nothing. Its message is told on its
    /// fd 2 as it stood then, which after `2>&1` is the pipe: the runner
    /// writes it there (`cat 2>&1 < /nope | wc -l`, as bash's).
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Vec<(Option<Fds>, Option<String>)> {
        let last = stages.len() - 1;
````

Replace:

````rust
            let base = Fds([input, output, self.fds.0[2]]);
            all.push(self.redirect(base, &stage.redirects));
        }
````

with:

````rust
            let base = Fds([input, output, self.fds.0[2]]);
            all.push(match self.made(base, &stage.redirects) {
                Ok(fds) => (Some(fds), None),
                Err((fds, message)) => {
                    let piped = fds.0[2] == Slot::PipeOut && message.len() <= PIPED_MESSAGE_MAX;
                    if !piped {
                        self.say_on(fds, message.as_bytes());
                    }
                    self.release(fds);
                    (None, piped.then_some(message))
                }
            });
        }
````

Replace:

````rust
/// A pipeline's stages for the runner: each one's words, fds and
/// assignments.
fn runner_stages<'c>(stages: &'c [parser::Command], fds: &[Option<Fds>]) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .map(|(c, fds)| runner::Stage {
            words: &c.words,
            fds: *fds,
            assigns: &c.assigns,
        })
````

with:

````rust
/// A pipeline's stages for the runner: each one's words, fds and
/// assignments, and the message it sends into the pipe.
fn runner_stages<'c>(
    stages: &'c [parser::Command],
    fds: &'c [(Option<Fds>, Option<String>)],
) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .map(|(c, (fds, into_pipe))| runner::Stage {
            words: &c.words,
            fds: *fds,
            assigns: &c.assigns,
            into_pipe: into_pipe.as_deref(),
        })
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 466 tests.

- [ ] **Step 7: Run the `redirect` scenario**

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
fix(shell,e2e): tell a stage's failed redirection into its pipe

A pipeline's redirections are made before its pipes, so a stage's
failed redirection was told on the screen even when its fd 2 stood on
the pipe after it: `cat 2>&1 < /nope | wc -l` printed the message and
0, where bash prints 1. Keep the message with the stage and let each
runner send it into the pipe. A fresh pipe takes 16 KiB at once; a
message over 8 KiB, which only a path longer than PATH_MAX makes, is
told on the screen, so the shell never waits on the pipe.

Refs: plan 1 final review M1
EOF
````


### Task 7: A long message of a stage that cannot start stays out of the pipe

The prototype's review (I-1), decision 4: a pipeline stage that cannot start tells why on its fd 2, which after `2>&1` is the fresh pipe to the next stage; that stage has not started and the shell holds the pipe's read end, so a message longer than a pipe holds (a 16 KiB command name: `$B 2>&1 | wc -c`) blocked `/bin/sh` for good, with the console raw and Ctrl-C only a byte. As a failed redirection's (Task 6), a message over `PIPED_MESSAGE_MAX` is now told on the screen, in both runners; the constant moves to `runner.rs`, which both use. The red run is the shell's tests (the message goes into the pipe) and the `redirect` scenario, which waits until its timeout. Mutation checks (3): the spawning runner piping the long message (unit test and scenario) and the in-process runner piping it, each fail.

**Files:**
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `tests/e2e/redirect.txt`

**Interfaces:**
- Consumes: Task 6's `PIPED_MESSAGE_MAX`.
- Produces: `runner::PIPED_MESSAGE_MAX` (moved from `shell.rs`).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
mod tests {
    use super::PIPED_MESSAGE_MAX;
    use crate::Shell;
    use crate::testing::{FakeStdout, Harness};
````

with:

````rust
mod tests {
    use crate::Shell;
    use super::PIPED_MESSAGE_MAX;
    use crate::testing::{FakeStdout, Harness};
````

Replace:

````rust
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
    }
````

with:

````rust
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
        // So is a stage that cannot start: its message into a pipe whose
        // reader has not started would block the shell for good (the
        // prototype's review, I-1).
        let name = "x".repeat(PIPED_MESSAGE_MAX);
        assert_eq!(
            h.spawning(&alloc::format!("{name} 2>&1 | t-args")),
            (3, alloc::format!("relay-sh: {name}: command not found\n"))
        );
        assert_eq!(h.programs.written.len(), 1, "nothing more written");
        // In the in-process runner too.
        let mut h = Harness::new();
        assert_eq!(
            h.run(&alloc::format!("{name} 2>&1 | wc -c")),
            (
                0,
                alloc::format!("relay-sh: {name}: command not found\n0\n")
            )
        );
        assert_eq!(h.run("nope 2>&1 | wc -c"), (0, "34\n".into()));
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/redirect.txt`**

In `tests/e2e/redirect.txt`, replace:

````text
expect \n1\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

with:

````text
expect \n1\nroot@relay:~# $
# A stage that cannot start, its message longer than a pipe takes before
# its reader starts: on the screen, so the shell does not wait for good
# (the prototype's review, I-1).
send B=yyyyyyyy; for i in 1 2 3 4 5 6 7 8 9 10 11; do B=$B$B; done
expect root@relay:~# $
send $B 2>&1 | wc -c
expect \nrelay-sh: y{16384}: command not found\n0\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_pipeline_s_commands_redirect_over_its_pipes`.

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: FAIL: scenario `redirect` stops at line 39, timed out waiting for `\nrelay-sh: y{16384}: command not found\n0\nroot@relay:~# $`.

- [ ] **Step 4: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
                match fds.0[2] {
                    Slot::PipeOut => err_pipe = message.into_bytes(),
                    Slot::File(i) => {
````

with:

````rust
                match fds.0[2] {
                    Slot::PipeOut if message.len() <= PIPED_MESSAGE_MAX => {
                        err_pipe = message.into_bytes()
                    }
                    Slot::File(i) => {
````

Replace:

````rust
    }
}

/// A command the shell runs itself, refused in a pipeline (§9.1).
````

with:

````rust
    }
}

/// The longest message the shell writes into the pipe after a command,
/// before the pipe's reader starts (a failed redirection's, or that of a
/// command that cannot start): half of the 16 KiB a pipe holds, so the
/// write never waits. Any path the vfs takes (`PATH_MAX`, 4 KiB) fits; a
/// longer message (`File name too long`, a name of 8 KiB) is told on the
/// screen.
pub(crate) const PIPED_MESSAGE_MAX: usize = 8 * 1024;

/// A command the shell runs itself, refused in a pipeline (§9.1).
````

Replace:

````rust
                Err(e) => {
                    // On its fd 2, the pipe after it too (`nope 2>&1 | b`).
                    let refused = cannot_start(name, e);
                    match fds.0[2] {
                        Slot::Shell(_) => parts.console.write(refused.message.as_bytes()),
                        _ => {
````

with:

````rust
                Err(e) => {
                    // On its fd 2, the pipe after it too (`nope 2>&1 | b`),
                    // unless it is too long for that pipe to take at once.
                    let refused = cannot_start(name, e);
                    let long = refused.message.len() > PIPED_MESSAGE_MAX;
                    match fds.0[2] {
                        Slot::Shell(_) => parts.console.write(refused.message.as_bytes()),
                        Slot::PipeOut if long => parts.console.write(refused.message.as_bytes()),
                        _ => {
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::reader::Reader;
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::{self, Transcript};
````

with:

````rust
use crate::reader::Reader;
use crate::runner::{self, PIPED_MESSAGE_MAX, Parts, Ran, Runners};
use crate::transcript::{self, Transcript};
````

Replace:

````rust
const MOTD_MAX: usize = 16 * 1024;
/// The longest message of a failed redirection that goes into the pipe
/// after its command, before the pipe's reader starts: half of the 16 KiB
/// a pipe holds, so the write never waits. Any path the vfs takes
/// (`PATH_MAX`, 4 KiB) fits; a longer one's (`File name too long`) is told
/// on the screen.
const PIPED_MESSAGE_MAX: usize = 8 * 1024;

````

with:

````rust
const MOTD_MAX: usize = 16 * 1024;

````

Replace:

````rust
    use crate::Shell;
    use super::PIPED_MESSAGE_MAX;
    use crate::testing::{FakeStdout, Harness};
````

with:

````rust
    use crate::Shell;
    use crate::runner::PIPED_MESSAGE_MAX;
    use crate::testing::{FakeStdout, Harness};
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 466 tests.

- [ ] **Step 7: Run the `redirect` scenario**

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
fix(shell,e2e): keep a long cannot-start message out of the pipe

A pipeline stage that cannot start tells why on its fd 2, which after
2>&1 is the fresh pipe to the next stage; that stage has not started
and the shell holds the pipe's read end, so a message longer than the
16 KiB a pipe holds (a 16 KiB command name) blocked /bin/sh for good,
with the console raw and Ctrl-C only a byte. As a failed redirection's,
a message over 8 KiB is now told on the screen, in both runners.

Refs: review I1
EOF
````


### Task 8: `/bin/sh` writes to its own fd 1 when that is not the screen

Decision 3 (spec §7.5; plan 1's review, M-3, the maintainer's choice to settle it here): a shell whose own fd 1 is a file or a pipe (`X | sh > f`) wrote a built-in's standard output, and its messages after `2>&1`, to the screen, since relay-rt's console writes fd 2. `Shell::with_output_redirected` makes the shell hold its fd 1 as a file it opened (`Slot::File(Handle::Fd(1))`, never released), so everything that goes to fd 1 goes there, as bash's does (probe p2); its other messages, its prompt and its job reports stay on the screen, and programs get fd 1 as before. `/bin/sh` calls it when its fd 1 is not the console, as `X | sh` and as an interactive shell. It is new public API, so a `feat` (the maintainer). The red run is the shell's tests, against the method doing nothing: nothing reaches fd 1. Mutation checks: the method doing nothing fails the unit test; `/bin/sh` not calling it fails Task 10's scenario lines (checked there).

**Files:**
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: `Files`, `Handle::Fd`, `relay_rt::SysStdout::is_tty`.
- Produces: `Shell::with_output_redirected()`, `Files::add` made visible to the shell.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        }
        self
````

with:

````rust
        }
        self
    }

    /// The same `/bin/sh`, its own fd 1 not the screen (`X | sh > f`,
    /// `X | sh | cat`, `sh > f` at a prompt).
    pub fn with_output_redirected(self) -> Shell<'a> {
        self
````

Replace:

````rust

    /// `sleep` and `t-spin` run on through `collect`'s first round (the
````

with:

````rust

    #[test]
    fn a_shell_whose_output_is_not_the_screen_writes_there() {
        // `X | sh > f` (plan 1's review, M-3; probe p2): a built-in's
        // output, and the shell's and a built-in's messages after `2>&1`,
        // go to the shell's fd 1, as bash's do; its other messages go to
        // the screen, and a program gets fd 1 as before.
        let mut h = spawning();
        h.dir("/d");
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .with_output_redirected();
        for line in [
            "cd /d; cd /; cd -",
            "cd /nope 2>&1",
            "nope 2>&1",
            "cd /nope",
            "nope",
            "t-args",
            "t-args 2>&1",
            "nope 2>&1 | t-args",
        ] {
            shell.execute(line);
        }
        drop(shell);
        let written: Vec<(u32, String)> = h
            .programs
            .written
            .iter()
            .map(|(fd, b)| (*fd, String::from_utf8_lossy(b).into_owned()))
            .collect();
        let pipe = h.programs.pipes[0].1;
        assert_eq!(
            written,
            [
                (1, "/d\n".into()),
                (1, "relay-sh: cd: /nope: No such file or directory\n".into()),
                (1, "relay-sh: nope: command not found\n".into()),
                (pipe, "relay-sh: nope: command not found\n".into()),
            ]
        );
        assert_eq!(
            h.console.take(),
            "relay-sh: cd: /nope: No such file or directory\n\
             relay-sh: nope: command not found\n"
        );
        let fds: Vec<[u32; 3]> = h.programs.spawned.iter().map(|s| s.fds).collect();
        assert_eq!(fds, [[0, 1, 2], [0, 1, 1], [pipe - 1, 1, 2]]);
        // Not redirected, all of it reaches the screen, as before.
        let mut h = spawning();
        assert_eq!(
            h.spawning("cd /nope 2>&1"),
            (1, "relay-sh: cd: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.programs.written, []);
    }

    /// `sleep` and `t-spin` run on through `collect`'s first round (the
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_shell_whose_output_is_not_the_screen_writes_there`.

- [ ] **Step 3: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, replace:

````rust
    /// A file just opened, held by one fd: its place.
    fn add(&mut self, handle: Handle) -> usize {
        let entry = Some((handle, 1));
````

with:

````rust
    /// A file just opened, held by one fd: its place.
    pub fn add(&mut self, handle: Handle) -> usize {
        let entry = Some((handle, 1));
````

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    /// The same `/bin/sh`, its own fd 1 not the screen (`X | sh > f`,
    /// `X | sh | cat`, `sh > f` at a prompt).
    pub fn with_output_redirected(self) -> Shell<'a> {
        self
````

with:

````rust
    /// The same `/bin/sh`, its own fd 1 not the screen (`X | sh > f`,
    /// `X | sh | cat`, `sh > f` at a prompt): fd 1 is then held as a file
    /// the shell opened, for good, so a built-in's output and the
    /// messages after `2>&1` go there, as bash's do, where relay-rt's
    /// console would show them (programmable shell gate §15 item 8).
    pub fn with_output_redirected(mut self) -> Shell<'a> {
        if self.runner.programs().is_some() {
            self.fds.0[1] = Slot::File(self.files.add(Handle::Fd(1)));
        }
        self
````

- [ ] **Step 5: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! `$0` is the shell's argument 0, as bash's is. Every shell imports the
//! environment it was started with as exported variables.
#![no_std]
````

with:

````rust
//! `$0` is the shell's argument 0, as bash's is. Every shell imports the
//! environment it was started with as exported variables. One whose own
//! fd 1 is not the console (`X | sh > f`) writes its built-ins' output
//! there.
#![no_std]
````

Replace:

````rust
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, env, sys};
use shell::Shell;

````

with:

````rust
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, env, sys};
use shell::{Shell, Stdout};

````

Replace:

````rust
            .named(&name);
        shell.run_input(&mut SysStdin) as u8
````

with:

````rust
            .named(&name);
        if !SysStdout::new().is_tty() {
            shell = shell.with_output_redirected();
        }
        shell.run_input(&mut SysStdin) as u8
````

Replace:

````rust
            .named(&name);
        shell.run();
````

with:

````rust
            .named(&name);
        if !SysStdout::new().is_tty() {
            shell = shell.with_output_redirected();
        }
        shell.run();
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 467 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates userland
git commit -F - <<'EOF'
feat(shell,sh): write to /bin/sh's fd 1 when it is not the screen

A shell whose own fd 1 is a file or a pipe (`X | sh > f`) wrote a
built-in's output, and its messages after 2>&1, to the screen, since
relay-rt's console writes fd 2. Such a shell now holds its fd 1 as a
file it opened, for good, so everything that goes to fd 1 goes there,
as bash's does; its other messages stay on the screen, and programs get
fd 1 as before.

Refs: plan 1 review M3
EOF
````


### Task 9: A built-in's write error is named as the shell's

Decision 8 (the prototype's review, P-1; the maintainer's choice of the prefix only): a built-in whose output could not be written said `cd: write error: …`, a program's form, which Task 8 makes reachable from `X | sh > f` on a full disk. bash 5.2 names its built-ins' errors as its own (`bash: export: write error: No space left on device`), so the shell says `relay-sh: export: write error: …`; a program keeps GNU's `echo: write error: …`. `help` and `jobs` still report it, status 1, where bash's say nothing (spec §10). The red run is the shell's tests: the messages lack the prefix. Mutation checks (2): no prefix, and a prefix for programs too, each fail tests.

**Files:**
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `runner::run_function`, `commands::BUILTINS`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
        assert!(!h.exists("/tmp/h"));
        // A write that fails is the built-in's write error.
        h.programs.write_error = Some((5, Errno::ENOSPC));
        assert_eq!(
            h.spawning("help > /tmp/h"),
            (1, "help: write error: No space left on device\n".into())
        );
````

with:

````rust
        assert!(!h.exists("/tmp/h"));
        // A write that fails is the built-in's write error, named by the
        // shell as bash names it (the prototype's review, P-1).
        h.programs.write_error = Some((5, Errno::ENOSPC));
        assert_eq!(
            h.spawning("help > /tmp/h"),
            (
                1,
                "relay-sh: help: write error: No space left on device\n".into()
            )
        );
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            h.programs.written_to("/tmp/g"),
            b"help: write error: No space left on device\n"
        );
````

with:

````rust
            h.programs.written_to("/tmp/g"),
            b"relay-sh: help: write error: No space left on device\n"
        );
````

Replace:

````rust
            h.run("echo x > /tmp/f"),
            (1, "echo: write error: No space left on device\n".into())
        );
    }
````

with:

````rust
            h.run("echo x > /tmp/f"),
            (1, "echo: write error: No space left on device\n".into())
        );
        // A built-in's is the shell's, as bash 5.2's `bash: export: write
        // error: …` (the prototype's review, P-1); a program's keeps its
        // own name, as GNU's.
        assert_eq!(
            h.run("export -p > /tmp/f"),
            (
                1,
                "relay-sh: export: write error: No space left on device\n".into()
            )
        );
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `runner::tests::a_built_in_writes_its_redirection_through_the_shell_s_fd`, `shell::tests::a_filesystem_that_writes_nothing_is_a_write_error_not_a_hang`.

- [ ] **Step 4: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
    } else if let Err(e) = finished {
        let message = format!("{}: write error: {e}\n", command.name);
        (ctx.write_error_status, message, true)
````

with:

````rust
    } else if let Err(e) = finished {
        // A built-in's is the shell's, as bash's (`bash: cd: write
        // error: …`); a program's its own, as GNU's.
        let message = if commands::BUILTINS.contains(&command.name) {
            format!("{NAME}: {}: write error: {e}\n", command.name)
        } else {
            format!("{}: write error: {e}\n", command.name)
        };
        (ctx.write_error_status, message, true)
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 467 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): name a built-in's write error as the shell's

A built-in whose output could not be written said `cd: write error:
...`, a program's form; bash 5.2 names its built-ins' errors as its
own (`bash: export: write error: No space left on device`), so the
shell now says `relay-sh: export: write error: ...`. A program keeps
GNU's `echo: write error: ...`. help and jobs still report the error,
status 1, where bash's say nothing.

Refs: review P1
EOF
````


### Task 10: The `redirect` scenario runs a `/bin/sh` whose output is a file or a pipe

Decision 3: the `redirect` scenario runs `X | sh > f` (a built-in's output and its message after `2>&1` in the file, another message on the screen), `X | sh | grep -c` (`export -p` through the pipe) and `sh > f` typed at the prompt (`cd -`'s output in the file). Tests only, of Task 8's behaviour, so no failing run; its mutants are here: `/bin/sh` not calling `with_output_redirected` as `X | sh`, or as an interactive shell, each fail the scenario.

**Files:**
- Modify: `tests/e2e/redirect.txt`

**Interfaces:**
- Consumes: Task 8.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/redirect.txt`**

In `tests/e2e/redirect.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# errors sent to a file kept out of a script's transcript; Ctrl-C
# reaching a program whose input is a file; and /dev/null (plan 2).
timeout 60
````

with:

````text
# errors sent to a file kept out of a script's transcript; Ctrl-C
# reaching a program whose input is a file; and /dev/null (plan 2); a
# failed redirection's message in a pipe and a shell whose output is a
# file or a pipe (plan 4).
timeout 60
````

Replace:

````text
expect \nrelay-sh: y{16384}: command not found\n0\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

with:

````text
expect \nrelay-sh: y{16384}: command not found\n0\nroot@relay:~# $
# A shell whose own output is a file or a pipe writes its built-ins'
# output, and messages after 2>&1, there; its other messages on the
# screen (plan 4).
send echo 'cd /nope1 2>&1; cd /; cd -; cd /nope2' | sh > r-x; cat r-x
expect \nrelay-sh: cd: /nope2: No such file or directory\nrelay-sh: cd: /nope1: No such file or directory\n/root\nroot@relay:~# $
send echo export -p | sh | grep -c declare
expect \n3\nroot@relay:~# $
send sh > r-i
expect root@relay:~# $
send cd /; cd -
expect root@relay:~# $
send exit
expect root@relay:~# $
send cat r-i
expect \n/root\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
````

- [ ] **Step 2: Run the `redirect` scenario**

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): run a /bin/sh whose output is a file or a pipe

The redirect scenario runs `X | sh > f`, `X | sh | grep` and `sh > f`
typed at the prompt: a built-in's output and its message after 2>&1
reach the file or the pipe, and its other messages the screen.
EOF
````


### Task 11: The parser's comment on `~` and `A=1 cmd`

Plan 3's final review (m-3): `crates/shell/src/parser.rs`'s module comment still said that an assignment's `~` and a word's `~` mean `/root` and that assignments before a command are refused; since plan 3 both `~` are the home directory (`Param::Home`: `$HOME`, `/root` when unset) and `A=1 cmd` runs. Documentation only.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new.

- [ ] **Step 1: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! A word whose unquoted start is a name and `=` is an assignment
//! (`Word::assignment`), its value's `~` at its start or after a `:` made
//! `/root`, as bash's is; an assignment before a command, which would give
//! bash's command an environment, and bash's `NAME+=value` are refused.
//!
````

with:

````rust
//! A word whose unquoted start is a name and `=` is an assignment
//! (`Word::assignment`), its value's `~` at its start or after a `:` the
//! home directory, as bash's is. Assignments before a command's name are
//! kept apart from its words (`A=1 cmd`, programmable shell gate §8.5);
//! bash's `NAME+=value`, which appends, is refused.
//!
````

Replace:

````rust
//! here-documents, `<&` and `<>` are refused. An unquoted `~` alone, or
//! before `/` in the same unquoted piece, at the start of a word means
//! `/root`, as in Linux. An unquoted `#` at the start of a word begins a
//! comment, which runs to the end of the line. An unquoted `|` joins
//! commands into a pipeline (user-space gate §9.1); each has a name, only
````

with:

````rust
//! here-documents, `<&` and `<>` are refused. An unquoted `~` alone, or
//! before `/` in the same unquoted piece, at the start of a word is the
//! home directory: `Param::Home`, which expands to `$HOME`, or to `/root`
//! when `HOME` is unset (§8.5). An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. An unquoted `|` joins
//! commands into a pipeline (user-space gate §9.1); each has a name, only
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add crates
git commit -F - <<'EOF'
docs(shell): say how the parser reads ~ and A=1 cmd now

The parser's module comment still said that an assignment's ~ and a
word's ~ mean /root and that assignments before a command are refused;
since plan 3 both ~ are the home directory, $HOME or /root when unset,
and A=1 cmd runs.

Refs: plan 3 final review m3
EOF
````


### Task 12: What a device's offset and `DevFs` keep

Plan 2's final review (m-2, m-3): `kernel/src/file.rs` said a character device's `seek` gives 0 whatever it is asked and that its reads and writes ignore the offset; `seek` wants a known `whence`, and the reads and writes move the offset, which `DevFs` ignores. The `FileSystem` contract in `crates/vfs/src/fs.rs` gains a paragraph on where `DevFs` differs from it (a device's reads, writes and truncation; `EPERM` for new names, removals and renames; a `shutdown` that leaves it writable), reflowed (the prototype's review's nit). Documentation only.

**Files:**
- Modify: `crates/vfs/src/fs.rs`
- Modify: `kernel/src/file.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new.

- [ ] **Step 1: Change `crates/vfs/src/fs.rs`**

In `crates/vfs/src/fs.rs`, replace:

````rust
/// ctime; adding or removing a directory entry sets the directory's.
pub trait FileSystem {
````

with:

````rust
/// ctime; adding or removing a directory entry sets the directory's.
///
/// A filesystem of devices, [`DevFs`](crate::DevFs), keeps the order of
/// errors but not the rest where its devices differ from files: a device's
/// reads, writes and truncation do what the device does (`null`'s ignore
/// the offset and the size), making a new name and removing or moving one
/// are `EPERM` (a rename of a name onto itself too), and `shutdown` leaves
/// it writable. The model test holds only ext2 and `MemFs` to the contract.
pub trait FileSystem {
````

- [ ] **Step 2: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
//! each `open` makes a new one with an offset of its own, as on Linux. A
//! character device's `seek` gives 0, whatever it is asked, as Linux's
//! `/dev/null`'s does (programmable shell gate §8.4); its reads and writes
//! ignore the offset.
//!
````

with:

````rust
//! each `open` makes a new one with an offset of its own, as on Linux. A
//! character device's `seek` gives 0 for any offset and a known `whence`,
//! as Linux's `/dev/null`'s does (programmable shell gate §8.4); its reads
//! and writes move the offset as a file's do, and its filesystem
//! (`DevFs`) ignores it.
//!
````

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates kernel
git commit -F - <<'EOF'
docs(kernel,vfs): say what a device's offset and DevFs keep

kernel/src/file.rs said a device's seek gives 0 whatever it is asked
and that its reads and writes ignore the offset; seek wants a known
whence, and the reads and writes move the offset, which DevFs ignores.
The FileSystem contract now says where DevFs, a filesystem of devices,
differs from it: a device's reads, writes and truncation, EPERM for new
names, removals and renames, and a shutdown that leaves it writable.

Refs: plan 2 final review m2, plan 2 final review m3
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p4/shell
gh pr create --base main --head m5p4/shell --title "fix(shell): settle milestone 5's deferred minors" --body-file - <<'EOF'
## What

Milestone 5, plan 4, tasks 1–12: a refusal names a redirection's fd as typed (`1> before |`, `0< after |`, `< &`); `export NAME+=value`, typed or expanded, is `unsupported syntax`, status 2, and exports nothing; while a built-in holds assignments, a held name it did not set counts the larger of its sizes, so the variables never pass `VARS_MAX`; a stage's failed redirection after `2>&1` is told into its pipe (`cat 2>&1 < /nope | wc -l` prints `1`), and a message over 8 KiB, a failed redirection's or a stage's that cannot start, goes to the screen, so `/bin/sh` never blocks on a pipe whose reader has not started (the prototype's review, I-1); a `/bin/sh` whose own fd 1 is not the console (`X | sh > f`) writes its built-ins' output and messages after `2>&1` there (`Shell::with_output_redirected`); a built-in's write error is `relay-sh: export: write error: …`; the parser's, `kernel/src/file.rs`'s and the `FileSystem` contract's comments.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3 to 7
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: xtask and the scenarios (Tasks 13–15)

`verify-usb`'s reason when mcopy says nothing, t-abi's message, and `env` with Ctrl-C and a killed command in QEMU.

Branch `m5p4/xtask`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-xtask`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-xtask origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-xtask
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p4/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p4/xtask /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-xtask m5p4/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p4/shell>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 13: `verify-usb` says how mcopy ended when it gives no reason

Milestone 4's plan 4's final review (M-3), decision 10: `verify-usb` keeps mcopy's words when `system.img` cannot be read, but an mcopy that failed without a word left `…on the ESP: ` with nothing after it, and blank lines of its output became `; ;`. `image::failure` drops blank lines and, with nothing left, says how the tool ended: `mcopy failed (exit status: 1)`. The red run is xtask's tests, which cannot compile before `failure` exists (the prototype's red run was against a `failure` that only joined the lines: blank lines kept, nothing said). Mutation checks (3): blank lines kept, blank lines not trimmed and no status, each fail the test.

**Files:**
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: `image::esp_read`.
- Produces: `image::failure(tool, status, stderr)` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
    #[test]
    fn a_file_of_the_esp_reads_back() {
````

with:

````rust
    #[test]
    fn a_tool_s_failure_is_its_own_words_or_its_status() {
        // Milestone 4's plan 4, M-3: an mcopy that failed saying nothing
        // left `…on the ESP: ` with nothing after it.
        use std::os::unix::process::ExitStatusExt;
        let one = std::process::ExitStatus::from_raw(1 << 8);
        assert_eq!(
            failure(
                "mcopy",
                one,
                b"init :: non DOS media\nCannot initialize '::'\n"
            ),
            "init :: non DOS media; Cannot initialize '::'"
        );
        assert_eq!(failure("mcopy", one, b"a\n\n  \nb\n"), "a; b");
        assert_eq!(failure("mcopy", one, b""), "mcopy failed (exit status: 1)");
        assert_eq!(
            failure("mcopy", one, b" \n"),
            "mcopy failed (exit status: 1)"
        );
        let killed = std::process::ExitStatus::from_raw(9);
        assert_eq!(
            failure("mcopy", killed, b""),
            "mcopy failed (signal: 9 (SIGKILL))"
        );
    }

    #[test]
    fn a_file_of_the_esp_reads_back() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: compile errors such as `` cannot find function `failure` in this scope ``.

- [ ] **Step 3: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
    if !out.status.success() {
        // mtools' own words, which tell a missing file, a damaged FAT and
        // a stick that cannot be opened apart, on one line.
        let said = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = said.lines().collect();
        bail!("{}", lines.join("; "));
    }
    Ok(fs::read(&file)?)
}
````

with:

````rust
    if !out.status.success() {
        bail!("{}", failure("mcopy", out.status, &out.stderr));
    }
    Ok(fs::read(&file)?)
}

/// Why a tool failed: its own words, which tell a missing file, a damaged
/// FAT and a stick that cannot be opened apart, on one line; or, when it
/// said nothing, how it ended.
fn failure(tool: &str, status: std::process::ExitStatus, stderr: &[u8]) -> String {
    let said = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return format!("{tool} failed ({status})");
    }
    lines.join("; ")
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 96 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
fix(xtask): say how mcopy ended when it gives no reason

verify-usb keeps mcopy's words when system.img cannot be read, but an
mcopy that failed without a word left `...on the ESP: ` with nothing
after it, and blank lines of its output became `; ;`. Blank lines are
dropped, and with nothing left the reason is how mcopy ended:
`mcopy failed (exit status: 1)`.

Refs: m4 plan 4 final review M3
EOF
````


### Task 14: t-abi's assertion message from the constants

Plan 2's final review (m-4), decision 10: the message of `t_abi_is_t_args_built_for_the_abi_before` said `3 -> 2`, milestone 3's change; it now names `relay_abi::VERSION` and `STALE_ABI` (`4 -> 3` today), so it cannot go stale again. A test's message only, so no failing run.

**Files:**
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: `relay_abi::VERSION`, `STALE_ABI`.
- Produces: nothing new.

- [ ] **Step 1: Change the tests in `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
            .collect();
        assert_eq!(differ.len(), 1, "one byte of the version: 3 -> 2");
        assert_eq!(STALE_ABI, 3);
````

with:

````rust
            .collect();
        assert_eq!(
            differ.len(),
            1,
            "one byte of the version: {} -> {STALE_ABI}",
            relay_abi::VERSION
        );
        assert_eq!(STALE_ABI, 3);
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 96 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
test(xtask): name t-abi's version change from the constants

The assertion's message said `3 -> 2`, the change of milestone 3; it
now says the ABI and STALE_ABI it compares, `4 -> 3` today.

Refs: plan 2 final review m4
EOF
````


### Task 15: `env` with Ctrl-C and a killed command in QEMU

Plan 3's final review (m-5), decision 10: `env` waits for its command, having no `exec` (§10), and no scenario showed it on target. The `environment` scenario runs `env t-spin` with Ctrl-C (`^C`, `$?` 130, both killed in `env`'s group, no `t-spin` left) and `env t-fault null-read` (139, and only the kernel's line before it: the step is anchored at the command's echo, the prototype's review, m-3). Tests only, of behaviour plan 3 delivered, so no failing run. Mutation checks (3): `env`'s command started in a group of its own, `env` printing a message for the killed command, and `env` exiting 0, each fail the scenario.

**Files:**
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: plan 3's `/bin/env`.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/environment.txt`**

In `tests/e2e/environment.txt`, replace:

````text
expect \n\+ echo "\[\$Q\]"\n\[1\]\nroot@relay:~# $
````

with:

````text
expect \n\+ echo "\[\$Q\]"\n\[1\]\nroot@relay:~# $
# env waits for its command, having no exec (§15 item 7): Ctrl-C ends
# both, in env's group, and a command that is killed leaves env the status
# the shell gives it, only the kernel's log naming the fault (plan 3's
# final review, m-5).
send env t-spin
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
send echo $?; dmesg | grep -c 'killed: Ctrl-C'; ps | grep -c t-spin
expect \n130\n2\n0\nroot@relay:~# $
send env t-fault null-read; echo $?
expect \Aenv t-fault null-read; echo \$\?\npid \d+ \(/bin/t-fault\): killed: page fault at 0x0[^\n]*\n139\nroot@relay:~# $
````

- [ ] **Step 2: Run the `environment` scenario**

Run: `cargo xtask test --e2e-only --scenario environment`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): run env with Ctrl-C and a killed command in QEMU

env waits for its command, having no exec: the environment scenario
now shows on target that Ctrl-C ends both env and its command, in
env's group, and that a command killed by a page fault leaves env 139
with only the kernel's log line.

Refs: plan 3 final review m5, review m3
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p4/xtask
gh pr create --base main --head m5p4/xtask --title "fix(xtask,e2e): settle xtask's and the scenarios' deferred minors" --body-file - <<'EOF'
## What

Milestone 5, plan 4, tasks 13–15: `verify-usb` drops mcopy's blank lines and, when it says nothing, says how it ended (`mcopy failed (exit status: 1)`); t-abi's assertion message names `VERSION` and `STALE_ABI`; the `environment` scenario runs `env t-spin` with Ctrl-C and `env t-fault`, its kernel line anchored.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `environment`

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's last pull request runs NUC checks 3 to 7
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-xtask
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `check7.sh`, version 0.6.0, and NUC checks 3 to 7 (Tasks 16–20)

Milestone 5 and the programmable shell gate are done: `check7.sh` in the `checks` scenario and in the NUC checklist, version 0.6.0, the documents, and NUC checks 3 to 7 on a stick written by `flash --full`. This pull request goes up as a draft; after the user's NUC checks pass, the real transcripts and the results-log row go into a commit of it.

Branch `m5p4/release`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-release`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-release origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-release
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p4/xtask` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p4/release /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-release m5p4/xtask`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p4/xtask>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 16: Check 7 in the NUC checklist

Spec §11.4 and decision 2: `docs/hardware-test.md` gains check 7 (step 10): started with `cd checks` and `sh check7.sh`, so that its `OLDPWD` shows `/root/checks`; what it runs; the prompt it comes back to; its `verify-usb` line (`39 of 39`) and two failure rows; checks 3 to 7 are now one run of six scripts, and the later steps are renumbered, the results log untouched. The README's quick start runs it, and the stick's `/root/README` names the eight built-ins and shows redirection and the environment. It comes before the script, since xtask's test that the instructions run every check script fails once Task 18 adds it. Documentation only.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/README`

**Interfaces:**
- Consumes: nothing.
- Produces: the checklist's step 10.

- [ ] **Step 1: Change `README.md`**

In `README.md`, replace:

````markdown
   `sh checks/check3-b.sh`, `sh checks/check4.sh`, `sh checks/check5.sh`
   and `sh checks/check6.sh`, then `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
````

with:

````markdown
   `sh checks/check3-b.sh`, `sh checks/check4.sh`, `sh checks/check5.sh`
   and `sh checks/check6.sh`, then `cd checks` and `sh check7.sh`, then
   `poweroff`.
5. Back in Linux Mint: `cargo xtask verify-usb` checks the filesystem and
````

- [ ] **Step 2: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 7 replacements, top to bottom:

Replace:

````markdown
Checks 1, 1b and 2 test the boot, the display and the keyboard, and are run by
hand. Checks 3 to 6 are one run of five scripts on the stick, with a few
steps by hand; it is the check every release gets. The results log at the
````

with:

````markdown
Checks 1, 1b and 2 test the boot, the display and the keyboard, and are run by
hand. Checks 3 to 7 are one run of six scripts on the stick, with a few
steps by hand; it is the check every release gets. The results log at the
````

Replace:

````markdown

## Checks 3 to 6 — files, programs, pipes, jobs and control flow

````

with:

````markdown

## Checks 3 to 7 — files, programs, pipes, jobs, control flow and the environment

````

Replace:

````markdown

Most of the run is five scripts on the stick, in `/root/checks/` (in the
repository under `rootfs/root/checks/`): `check3-a.sh` and `check3-b.sh`
(check 3, either side of a restart), `check4.sh`, `check5.sh` and
`check6.sh`. The
`/bin/sh` that init (process 1) starts as process 2 runs them: `sh FILE`
````

with:

````markdown

Most of the run is six scripts on the stick, in `/root/checks/` (in the
repository under `rootfs/root/checks/`): `check3-a.sh` and `check3-b.sh`
(check 3, either side of a restart), `check4.sh`, `check5.sh`, `check6.sh`
and `check7.sh`, the last started in `/root/checks`. The
`/bin/sh` that init (process 1) starts as process 2 runs them: `sh FILE`
````

Replace:

````markdown

### The error screen, power off and verify-usb

10. The error screen: `reboot`, and choose the stick again with F10, so
    that the kernel log's last lines are the boot's. At the prompt type
````

with:

````markdown

### Check 7 — redirection and the environment

10. Type `cd checks` and `sh check7.sh`: check 7 starts in `/root/checks`,
    which its `OLDPWD` shows. It runs for a few seconds:
    - `/dev/null` (`[ -c /dev/null ]`, output and errors sent to it,
      `wc -c < /dev/null`), and every standard stream redirected to files:
      `>`, `2>`, `>>`, `2>>`, both orders of `2>&1`, `2>&1` into a pipe, a
      `for` loop's output and `wc -l < file`;
    - the environment: `export A=1` seen by `env`, `B=2 env` and `$B`
      empty afterwards, `env -i` and `env -u HOME`, a script it writes
      that sees `A` and an assigned `B` (a nested `sh`), and neither after
      `unset A`;
    - `cd`: `$PWD` and `$OLDPWD` (`/root /root/checks`: the script's
      `cd /root` left `/root/checks`), `cd /bin`, `cd -` printing `/root`,
      `cd ..` and `cd` back to `HOME`.

    The prompt `root@relay:~/checks# ` comes back after
    `+ echo "$PWD $OLDPWD"` and `/root /`: the script's `cd` stays in it.
    Type `cd` to go home.

### The error screen, power off and verify-usb

11. The error screen: `reboot`, and choose the stick again with F10, so
    that the kernel log's last lines are the boot's. At the prompt type
````

Replace:

````markdown
    stick again with F10: the motd and the prompt.
11. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
    screen says `System halted. It is now safe to power off.` instead, note
    the `relay:` line above it and hold the power button.
12. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
    lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
````

with:

````markdown
    stick again with F10: the motd and the prompt.
12. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
    screen says `System halted. It is now safe to power off.` instead, note
    the `relay:` line above it and hold the power button.
13. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
    lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
````

Replace:

````markdown
    `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run <time>)`,
    `/root/checks/check5.sh: ok, 29 of 29 commands as expected (run <time>)`
    and `/root/checks/check6.sh: ok, 46 of 46 commands as expected (run <time>)`
    (each line of a loop written across lines counts), with the UTC times
    of the five runs. A transcript older than the
    stick's `system.img` (a run before the last `flash --kernel`, which
````

with:

````markdown
    `/root/checks/check4.sh: ok, 33 of 33 commands as expected (run <time>)`,
    `/root/checks/check5.sh: ok, 29 of 29 commands as expected (run <time>)`,
    `/root/checks/check6.sh: ok, 46 of 46 commands as expected (run <time>)`
    (each line of a loop written across lines counts) and
    `/root/checks/check7.sh: ok, 39 of 39 commands as expected (run <time>)`,
    with the UTC times of the six runs. A transcript older than the
    stick's `system.img` (a run before the last `flash --kernel`, which
````

Replace:

````markdown
| `check6.sh`'s two `date` lines are more than 10 s apart | A sync after every command is slow on the stick | Note both times in the results log: a later plan syncs once per command line instead (the programmable shell gate's §13) |
| Ctrl-C at `> ` by hand gives no `^C`, or no prompt | The line editor did not get the K120's Ctrl-C | Photograph the screen; Enter, or a second Ctrl-C, shows whether the shell still reads the keyboard |
````

with:

````markdown
| `check6.sh`'s two `date` lines are more than 10 s apart | A sync after every command is slow on the stick | Note both times in the results log: a later plan syncs once per command line instead (the programmable shell gate's §13) |
| `check7.sh`'s first `echo "$PWD $OLDPWD"` prints `/root /root` | The script was started from `~`, not from `/root/checks` | `cd checks`, then `sh check7.sh` again (it starts afresh) |
| `check7.sh`: `env`, `env -u HOME` or the nested script shows a variable it should not, or lacks one | A variable did not reach a program's environment as exported, assigned or unset | `verify-usb` names the line; `env` at the prompt lists what the shell exports |
| Ctrl-C at `> ` by hand gives no `^C`, or no prompt | The line editor did not get the K120's Ctrl-C | Photograph the screen; Enter, or a second Ctrl-C, shows whether the shell still reads the keyboard |
````

- [ ] **Step 3: Change `rootfs/root/README`**

In `rootfs/root/README`, make these 2 replacements, top to bottom:

Replace:

````text
Files live on the USB stick and survive reboots. Every command but the
shell's built-ins (cd, exit, help, jobs, wait, kill) is a program in /bin
(ls /bin), and the shell is /bin/sh. Useful commands:
  ls, cd, pwd, cat, echo text > file, mkdir, rm, cp, mv, grep, help
````

with:

````text
Files live on the USB stick and survive reboots. Every command but the
shell's built-ins (cd, exit, export, help, jobs, kill, unset, wait) is a
program in /bin (ls /bin), and the shell is /bin/sh. Useful commands:
  ls, cd, pwd, cat, echo text > file, mkdir, rm, cp, mv, grep, help
````

Replace:

````text
  while test -f FILE; do a; done    for x in one two; do echo $x; done
````

with:

````text
  while test -f FILE; do a; done    for x in one two; do echo $x; done
Redirection and the environment:
  a < in > out 2> err    a > f 2>&1    a 2>&1 | b    a > /dev/null
  export NAME=value    A=1 env    env -i a    unset NAME    cd -
````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add README.md docs rootfs
git commit -F - <<'EOF'
docs: add check 7, redirection and the environment, to the checklist

docs/hardware-test.md gains check 7: started with `cd checks` and
`sh check7.sh`, so that its OLDPWD shows /root/checks; what it runs
(/dev/null, every stream redirected, export, env -i and -u, A=1 cmd, a
nested sh, unset, cd - with PWD and OLDPWD), its verify-usb line and
two failure rows; checks 3 to 7 are now one run of six scripts. The
README's quick start runs it, and the stick's /root/README names the
eight built-ins and shows redirection and the environment.
EOF
````


### Task 17: The instructions test takes a script started in `/root/checks`

Decision 2: the test that the README and the checklist run every check script looked only for `sh checks/X`; check 7 is started with `cd checks` and `sh check7.sh`, and the test takes that form too. A test's change with no script yet to need it, so no failing run; its mutant (only `sh checks/X`) fails once Task 18 adds the script.

**Files:**
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Task 16's wording.
- Produces: nothing new.

- [ ] **Step 1: Change the tests in `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
            for s in &scripts {
                assert!(
                    text.contains(&format!("sh checks/{s}")),
                    "{doc} does not run {s}"
````

with:

````rust
            for s in &scripts {
                // From `~`, or from `/root/checks` as check 7 starts
                // (programmable shell gate §11.4).
                let from_home = format!("sh checks/{s}");
                let from_checks = format!("`cd checks` and `sh {s}`");
                assert!(
                    text.contains(&from_home) || text.contains(&from_checks),
                    "{doc} does not run {s}"
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 96 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
test(xtask): take a check script started in /root/checks as run

The test that the README and the checklist run every check script
looked only for `sh checks/X`; check 7 is started with `cd checks` and
`sh check7.sh`, so that its OLDPWD shows where it started, and the
test takes that form too.
EOF
````


### Task 18: `check7.sh`, NUC check 7

Spec §11.4 and decision 2: milestone 5's NUC check, 39 commands, the same on the NUC and in QEMU: `[ -c /dev/null ]`, output and errors sent to `/dev/null`, every standard stream redirected (`>`, `2>`, both orders of `2>&1`, `2>&1` into a pipe, a `for` loop's output, `>>`, `2>>`, `wc -l < f`); `export A=1` seen by `env`, `B=2 env` and `$B` empty afterwards, `env -i` and `env -u HOME`, a helper it writes that a nested `sh` runs, seeing `A` and an assigned `B`, and neither after `unset A`; `cd`: `$PWD $OLDPWD` (`/root /root/checks`, as it started in `/root/checks`), `cd /bin`, `cd -`, `cd ..`, `cd`. Its output is bash 5.2's with GNU's programs (probes c1, c2), but `cd`'s message, named `relay-sh`, and the nested script's trace. The `checks` scenario runs it after check 6, from `/root/checks`, and checks its transcript; that QEMU transcript is recorded as the scenario's disk holds it, and the NUC's is a copy until the NUC run records it. The fixtures test lists it, and its comment says the NUC transcripts are those of checks 3 to 7 (milestone 4's plan 4, M-4). The red run is xtask's tests, which cannot read the new script and transcripts. Mutation checks (2): a wrong `OLDPWD` expectation fails the fixtures test, and Task 17's form left out fails the instructions test.

**Files:**
- Create: `rootfs/root/checks/check7.sh`
- Modify: `tests/e2e/checks.txt`
- Create: `xtask/fixtures/checks/check7.nuc.log`
- Create: `xtask/fixtures/checks/check7.qemu.log`
- Modify: `xtask/src/checks.rs`

**Interfaces:**
- Consumes: Task 17.
- Produces: `rootfs/root/checks/check7.sh`, `xtask/fixtures/checks/check7.{qemu,nuc}.log`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/checks.txt`**

In `tests/e2e/checks.txt`, make these 3 replacements, top to bottom:

Replace:

````text
# script starts with cd /root (programmable shell gate §9.2), a reboot,
# part 2, then checks 4, 5 and 6. Afterwards their transcripts on the disk
# must show what the scripts expect.
timeout 30
````

with:

````text
# script starts with cd /root (programmable shell gate §9.2), a reboot,
# part 2, then checks 4, 5 and 6, and check 7 from /root/checks (§11.4).
# Afterwards their transcripts on the disk must show what the scripts
# expect.
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
send cd checks
expect root@relay:~/checks# $
send sh check7.sh
expect \n\+ cd /root\n\+ rm -f /root/check7-a\.sh .*\n
expect \n\+ echo "\$PWD \$OLDPWD"\n/root /\n
expect root@relay:~/checks# $
poweroff
````

Replace:

````text
check-script /root/checks/check6.sh
````

with:

````text
check-script /root/checks/check6.sh
check-script /root/checks/check7.sh
````

- [ ] **Step 2: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// from the `checks` scenario, the NUC's as the NUC wrote them in the
    /// last NUC run of checks 3 to 6 (`docs/hardware-test.md`'s results
    /// log), copied off the stick unchanged. A `#nuc>` line must not need
    /// a line of its own next to the `#>` line for the same output, which
````

with:

````rust
    /// from the `checks` scenario, the NUC's as the NUC wrote them in the
    /// last NUC run of checks 3 to 7 (`docs/hardware-test.md`'s results
    /// log), copied off the stick unchanged; until that run check 7's is a
    /// copy of QEMU's and the others hold the lines milestone 5 changed by
    /// hand (`ABI 4`, `dev`, `+ cd /root`). A `#nuc>` line must not need
    /// a line of its own next to the `#>` line for the same output, which
````

Replace:

````rust
                include_str!("../fixtures/checks/check6.nuc.log"),
            ),
````

with:

````rust
                include_str!("../fixtures/checks/check6.nuc.log"),
            ),
            (
                "check7.sh",
                include_str!("../../rootfs/root/checks/check7.sh"),
                include_str!("../fixtures/checks/check7.qemu.log"),
                include_str!("../fixtures/checks/check7.nuc.log"),
            ),
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: compile errors such as `` couldn't read `xtask/src/../../rootfs/root/checks/check7.sh`: No such file or directory (os error 2) ``; `` couldn't read `xtask/src/../fixtures/checks/check7.qemu.log`: No such file or directory (os error 2) ``.

- [ ] **Step 4: Create `rootfs/root/checks/check7.sh`**

Create `rootfs/root/checks/check7.sh`:

````bash
# NUC check 7 (docs/hardware-test.md; milestone 5, plan 4): after
# check6.sh, `cd checks` and `sh check7.sh`, so that it starts in
# /root/checks. Redirection, /dev/null, the environment and cd, run by
# /bin/sh. Its transcript is check7.log; the `#>` lines are explained in
# check3-a.sh. Nothing here differs between the NUC and QEMU.

# Run from any directory (programmable shell gate §9.2): where it was
# started is OLDPWD below.
cd /root

# Start afresh, so the check can be run again.
rm -f /root/check7-a.sh /root/check7-a.log /root/check7-out /root/check7-err

# /dev/null, and every standard stream redirected: both orders of
# 2>&1, into a pipe, a loop's output, appending.
[ -c /dev/null ] && echo null is a character device
#> null is a character device
echo lost > /dev/null
cat /dev/null | wc -c
#> 0
ls /nope 2> /dev/null; echo $?
#> 2
wc -c < /dev/null
#> 0
ls /root/checks/check7.sh /nope > /root/check7-out 2> /root/check7-err
cat /root/check7-out /root/check7-err
#> /root/checks/check7\.sh
#> ls: cannot access '/nope': No such file or directory
ls /nope /root/checks/check7.sh > /root/check7-out 2>&1
cat /root/check7-out
#> ls: cannot access '/nope': No such file or directory
#> /root/checks/check7\.sh
ls /nope /root/checks/check7.sh 2>&1 > /root/check7-out
#> ls: cannot access '/nope': No such file or directory
cat /root/check7-out
#> /root/checks/check7\.sh
ls /nope /root/checks/check7.sh 2>&1 | wc -l
#> 2
for w in a b c; do echo $w; done > /root/check7-out
wc -l < /root/check7-out
#> 3
echo more >> /root/check7-out
cd /nope 2>> /root/check7-out
tail -n 2 /root/check7-out
#> more
#> relay-sh: cd: /nope: No such file or directory

# export, env -i and -u, an assignment before a command, a nested sh
# that sees what is exported, and unset.
export A=1
env | grep ^A=
#> A=1
B=2 env | grep ^B=
#> B=2
echo "[$B]"
#> \[\]
env -i C=3 D=4 env
#> C=3
#> D=4
env -u HOME env | grep -c ^HOME=
#> 0
echo 'echo "nested: [$A] [$B]"' > /root/check7-a.sh
B=5 sh /root/check7-a.sh
#> \+ echo "nested: \[\$A\] \[\$B\]"
#> nested: \[1\] \[5\]
unset A
env | grep -c ^A=
#> 0
sh /root/check7-a.sh
#> \+ echo "nested: \[\$A\] \[\$B\]"
#> nested: \[\] \[\]

# cd -, PWD and OLDPWD: the script started in /root/checks.
echo "$PWD $OLDPWD"
#> /root /root/checks
cd /bin
echo "$PWD $OLDPWD"
#> /bin /root
cd -
#> /root
pwd
#> /root
cd ..
pwd
#> /
cd
echo "$PWD $OLDPWD"
#> /root /
````

- [ ] **Step 5: Create `xtask/fixtures/checks/check7.nuc.log`**

Create `xtask/fixtures/checks/check7.nuc.log`:

````text
+ cd /root
+ rm -f /root/check7-a.sh /root/check7-a.log /root/check7-out /root/check7-err
+ [ -c /dev/null ] && echo null is a character device
null is a character device
+ echo lost > /dev/null
+ cat /dev/null | wc -c
0
+ ls /nope 2> /dev/null; echo $?
2
+ wc -c < /dev/null
0
+ ls /root/checks/check7.sh /nope > /root/check7-out 2> /root/check7-err
+ cat /root/check7-out /root/check7-err
/root/checks/check7.sh
ls: cannot access '/nope': No such file or directory
+ ls /nope /root/checks/check7.sh > /root/check7-out 2>&1
+ cat /root/check7-out
ls: cannot access '/nope': No such file or directory
/root/checks/check7.sh
+ ls /nope /root/checks/check7.sh 2>&1 > /root/check7-out
ls: cannot access '/nope': No such file or directory
+ cat /root/check7-out
/root/checks/check7.sh
+ ls /nope /root/checks/check7.sh 2>&1 | wc -l
2
+ for w in a b c; do echo $w; done > /root/check7-out
+ wc -l < /root/check7-out
3
+ echo more >> /root/check7-out
+ cd /nope 2>> /root/check7-out
+ tail -n 2 /root/check7-out
more
relay-sh: cd: /nope: No such file or directory
+ export A=1
+ env | grep ^A=
A=1
+ B=2 env | grep ^B=
B=2
+ echo "[$B]"
[]
+ env -i C=3 D=4 env
C=3
D=4
+ env -u HOME env | grep -c ^HOME=
0
+ echo 'echo "nested: [$A] [$B]"' > /root/check7-a.sh
+ B=5 sh /root/check7-a.sh
+ echo "nested: [$A] [$B]"
nested: [1] [5]
+ unset A
+ env | grep -c ^A=
0
+ sh /root/check7-a.sh
+ echo "nested: [$A] [$B]"
nested: [] []
+ echo "$PWD $OLDPWD"
/root /root/checks
+ cd /bin
+ echo "$PWD $OLDPWD"
/bin /root
+ cd -
/root
+ pwd
/root
+ cd ..
+ pwd
/
+ cd
+ echo "$PWD $OLDPWD"
/root /
````

- [ ] **Step 6: Create `xtask/fixtures/checks/check7.qemu.log`**

Create `xtask/fixtures/checks/check7.qemu.log`:

````text
+ cd /root
+ rm -f /root/check7-a.sh /root/check7-a.log /root/check7-out /root/check7-err
+ [ -c /dev/null ] && echo null is a character device
null is a character device
+ echo lost > /dev/null
+ cat /dev/null | wc -c
0
+ ls /nope 2> /dev/null; echo $?
2
+ wc -c < /dev/null
0
+ ls /root/checks/check7.sh /nope > /root/check7-out 2> /root/check7-err
+ cat /root/check7-out /root/check7-err
/root/checks/check7.sh
ls: cannot access '/nope': No such file or directory
+ ls /nope /root/checks/check7.sh > /root/check7-out 2>&1
+ cat /root/check7-out
ls: cannot access '/nope': No such file or directory
/root/checks/check7.sh
+ ls /nope /root/checks/check7.sh 2>&1 > /root/check7-out
ls: cannot access '/nope': No such file or directory
+ cat /root/check7-out
/root/checks/check7.sh
+ ls /nope /root/checks/check7.sh 2>&1 | wc -l
2
+ for w in a b c; do echo $w; done > /root/check7-out
+ wc -l < /root/check7-out
3
+ echo more >> /root/check7-out
+ cd /nope 2>> /root/check7-out
+ tail -n 2 /root/check7-out
more
relay-sh: cd: /nope: No such file or directory
+ export A=1
+ env | grep ^A=
A=1
+ B=2 env | grep ^B=
B=2
+ echo "[$B]"
[]
+ env -i C=3 D=4 env
C=3
D=4
+ env -u HOME env | grep -c ^HOME=
0
+ echo 'echo "nested: [$A] [$B]"' > /root/check7-a.sh
+ B=5 sh /root/check7-a.sh
+ echo "nested: [$A] [$B]"
nested: [1] [5]
+ unset A
+ env | grep -c ^A=
0
+ sh /root/check7-a.sh
+ echo "nested: [$A] [$B]"
nested: [] []
+ echo "$PWD $OLDPWD"
/root /root/checks
+ cd /bin
+ echo "$PWD $OLDPWD"
/bin /root
+ cd -
/root
+ pwd
/root
+ cd ..
+ pwd
/
+ cd
+ echo "$PWD $OLDPWD"
/root /
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 96 tests.

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
test(checks,e2e): add check7.sh, redirection and the environment

NUC check 7 (programmable shell gate §11.4), started in /root/checks
with `cd checks` and `sh check7.sh`: /dev/null, every standard stream
redirected (both orders of 2>&1, into a pipe, a loop's output, >> and
2>>), export, env -i and -u, A=1 before a command, a nested sh that
sees exported and assigned values, unset, and cd - with PWD and
OLDPWD, whose first value shows where it started. Its 39 commands
print what bash 5.2 prints with GNU's programs, but cd's message names
relay-sh. The checks scenario runs it after check 6, from
/root/checks; its NUC transcript is QEMU's until the NUC run.
EOF
````


### Task 19: Version 0.6.0

Spec §1.4 and decision 1: milestone 5 is version 0.6.0, as milestone 4 was 0.5.0. The spike bumped it first and ran every scenario: only `uname`'s unit test, the `shell` scenario, `check3-a.sh`'s `uname -a` line and the recorded transcripts depend on it. The transcripts' `Relay relay 0.6.0 x86_64` and `Relay OS 0.6.0` lines are changed in place by `sed`, since the NUC's hold escape bytes; QEMU's then match what the `checks` scenario's disk holds, and the NUC's hold them until the NUC run records them. `docs/hardware-test.md`'s check 1 names the new version; the results log keeps its rows. The red runs are `uname`'s test and `shell`; `Cargo.toml` goes in after them, and cargo updates `Cargo.lock`.

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
- Produces: version 0.6.0.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 4 is version 0.5.0 (programmable shell gate §15 item
        // 4), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.5.0 x86_64\n".into()));
        assert_eq!(
````

with:

````rust
        assert_eq!(h.run("uname"), (0, "Relay\n".into()));
        // Milestone 5 is version 0.6.0 (programmable shell gate §15 item
        // 8), from Cargo.toml.
        assert_eq!(h.run("uname -a"), (0, "Relay relay 0.6.0 x86_64\n".into()));
        assert_eq!(
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/shell.txt`**

In `tests/e2e/shell.txt`, replace:

````text
send uname -a
expect \nRelay relay 0\.5\.0 x86_64\n
expect root@relay:~# $
````

with:

````text
send uname -a
expect \nRelay relay 0\.6\.0 x86_64\n
expect root@relay:~# $
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::uname_prints_the_system`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: FAIL: scenario `shell` stops at line 9, timed out waiting for `\nRelay relay 0\.6\.0 x86_64\n`.

- [ ] **Step 4: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
[workspace.package]
version = "0.5.0"
edition = "2024"
````

with:

````toml
[workspace.package]
version = "0.6.0"
edition = "2024"
````

- [ ] **Step 5: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.5.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

with:

````markdown
3. Within about 5 s the monitor must show, on black:
   - `Relay OS 0.6.0`
   - `[ ok ] console WxH (CxR cells)` — note W×H. It should be the monitor's
````

- [ ] **Step 6: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
uname -a
#> Relay relay 0\.5\.0 x86_64
date
````

with:

````bash
uname -a
#> Relay relay 0\.6\.0 x86_64
date
````

- [ ] **Step 7: Change the check-3 transcripts' version lines (they may hold escape bytes)**

Run:

````bash
sed -i 's/^Relay relay 0\.5\.0 x86_64$/Relay relay 0.6.0 x86_64/; s/^Relay OS 0\.5\.0$/Relay OS 0.6.0/' xtask/fixtures/checks/check3-a.nuc.log xtask/fixtures/checks/check3-a.qemu.log xtask/fixtures/checks/check3-b.nuc.log xtask/fixtures/checks/check3-b.qemu.log
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 467 tests.

- [ ] **Step 9: Run the `shell`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add Cargo.lock Cargo.toml crates docs rootfs tests xtask
git commit -F - <<'EOF'
chore(release): 0.6.0

Milestone 5 is version 0.6.0 (programmable shell gate §1.4, §15 item
8), as milestone 4 was 0.5.0: the workspace's version, from which the
kernel's banner and `uname -a` take theirs (`Relay relay 0.6.0
x86_64`). The shell scenario and check3-a.sh's `uname -a` line follow,
and so do the recorded transcripts' `uname -a` and `Relay OS` lines,
changed in place (QEMU's then match what the checks scenario's disk
holds, and the NUC's until the NUC run records them), and
docs/hardware-test.md's check 1.
EOF
````


### Task 20: The documents say milestone 5 is done

Decision 1: `AGENTS.md`'s status line and the README say what milestone 5 added and that version 0.6.0 ends the programmable shell gate; the gate's spec says it was implemented, and milestone 5's roadmap that its four plans were executed. Documentation only.

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md`
- Modify: `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md`

**Interfaces:**
- Consumes: Task 19.
- Produces: nothing new.

- [ ] **Step 1: Change `AGENTS.md`**

In `AGENTS.md`, replace:

````markdown

Status: milestones 1 to 4 are done, at version 0.5.0 (tag `v0.5.0`). The
kernel boots, drives xHCI (keyboard and USB storage), mounts an ext2 root
and runs programs in ring 3 from a read-only `/bin` (`system.img`).
Process 1 is the kernel's init, which runs `/bin/sh`; the shell has
pipes, background jobs, scripts with arguments and variables, lists
(`;`, `&&`, `||`, `!`), `if`, `while`, `until` and `for`, commands read
across lines with bash's `> ` prompt, and eight built-ins (`cd`, `exit`,
`export`, `help`, `jobs`, `kill`, `unset`, `wait`). Every other command is
a program, `test`, `[` and `env` too. The programmable shell gate goes on
with milestone 5 (redirection and environment, 0.6.0). The aarch64 port
comes later.

````

with:

````markdown

Status: milestones 1 to 5 are done, at version 0.6.0 (tag `v0.6.0`),
which ends the programmable shell gate. The kernel boots, drives xHCI
(keyboard and USB storage), mounts an ext2 root, a `/dev` with
`/dev/null`, and runs programs in ring 3 from a read-only `/bin`
(`system.img`), each given an environment (ABI 4). Process 1 is the
kernel's init, which runs `/bin/sh`; the shell has pipes, background
jobs, scripts with arguments and variables, lists (`;`, `&&`, `||`, `!`),
`if`, `while`, `until` and `for`, commands read across lines with bash's
`> ` prompt, redirection of every standard stream (`<`, `2>`, `2>&1`,
compound commands too), exported variables, `A=1 cmd`, `cd -` with
`HOME`, `PWD` and `OLDPWD`, and eight built-ins (`cd`, `exit`, `export`,
`help`, `jobs`, `kill`, `unset`, `wait`). Every other command is a
program, `test`, `[` and `env` too. The next gate is not chosen yet; the
aarch64 port comes later.

````

- [ ] **Step 2: Change `README.md`**

In `README.md`, replace:

````markdown

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0, milestone 3
version 0.4.0, which ends the user-space gate, and milestone 4 version
0.5.0.

````

with:

````markdown

Milestone 5 adds redirection and the environment: every standard stream
can be redirected (`< in`, `2> err`, `2>> err`, `> f 2>&1`, `a 2>&1 | b`),
compound commands too (`for …; done > f`), and `/dev/null` exists;
programs get an environment, the shell exports variables (`export`,
`unset`, `A=1 cmd`) and `/bin/env` shows and changes one; `cd` follows
`HOME`, `PWD` and `OLDPWD` (`cd -`), and the check scripts pass from any
directory.

Milestone 1 is version 0.2.0, milestone 2 version 0.3.0, milestone 3
version 0.4.0, which ends the user-space gate, milestone 4 version 0.5.0
and milestone 5 version 0.6.0, which ends the programmable shell gate.

````

- [ ] **Step 3: Change `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md`**

In `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md`, replace:

````markdown

**Status:** Plans 1 to 3 are done (#116–#127); plan 4 is planned and lands with these notes.

````

with:

````markdown

**Status:** Done. All four plans were executed; milestone 5, and with it the programmable shell gate, ended at version 0.6.0 (tag `v0.6.0`).

````

- [ ] **Step 4: Change `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md`**

In `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md`, replace:

````markdown
- **Date:** 2026-10-02
- **Status:** Approved 2026-10-02; revised while planning milestone 4's plans 1 to 4 and milestone 5's plans 1 to 4 (see §15)
- **Builds on:** the user-space gate (version 0.4.0,
````

with:

````markdown
- **Date:** 2026-10-02
- **Status:** Approved 2026-10-02; revised while planning milestone 4's plans 1 to 4 and milestone 5's plans 1 to 4 (see §15); implemented: milestone 4 ended at version 0.5.0 and milestone 5, which ends the gate, at version 0.6.0 (tags `v0.5.0`, `v0.6.0`)
- **Builds on:** the user-space gate (version 0.4.0,
````

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add AGENTS.md README.md docs
git commit -F - <<'EOF'
docs: say that milestone 5 is done, at version 0.6.0

AGENTS.md's status line and the README say what milestone 5 added
(redirection of every standard stream, compound commands too,
/dev/null, programs' environments, export, unset, A=1 cmd, /bin/env,
cd - with HOME, PWD and OLDPWD) and that version 0.6.0 ends the
programmable shell gate; the gate's spec says it was implemented, and
milestone 5's roadmap that all four plans were executed.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p4/release
gh pr create --draft --base main --head m5p4/release --title "chore(release): 0.6.0, check7.sh and NUC checks 3–7" --body-file - <<'EOF'
## What

Milestone 5, plan 4, tasks 16–20: `check7.sh` (`/dev/null`, every standard stream redirected, `export`, `env -i` and `-u`, `A=1 cmd`, a nested `sh`, `unset`, `cd -` with `PWD` and `OLDPWD`; 39 commands, started in `/root/checks`) in the `checks` scenario and in `docs/hardware-test.md`, its QEMU transcript recorded; version 0.6.0 (`uname -a` says `Relay relay 0.6.0 x86_64`); the README, `AGENTS.md`, the roadmap and the spec say milestone 5 and the gate are done.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `checks`

## Hardware

- [x] Needed: NUC checks 3 to 7 of `docs/hardware-test.md` (check 3's scripts and steps by hand, `check4.sh`, `check5.sh` and `exit` by hand, `check6.sh` and its steps by hand, `check7.sh` started in `/root/checks`, the error screen), run by the user on a stick written by `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for NUC checks 3 to 7**

Ask the user to run NUC checks 3 to 7 (`docs/hardware-test.md`, steps 1–13: check 3's scripts and its three steps by hand, `check4.sh`, `check5.sh` and `exit` by hand, `check6.sh` and its three steps by hand, `cd checks` and `sh check7.sh`, the error screen, `poweroff` and `verify-usb`) on a stick written by `cargo xtask flash --full` from this worktree, and to report their results, with the two times `check6.sh`'s `date` lines printed; the pull request stays a draft until they do. The stick is then usually plugged into this machine, its root mounted at `/media/maw/relayroot`.

- [ ] **Record the NUC's transcripts in this pull request**

After a pass: check the stick with `cargo xtask verify-usb` (it only reads it), copy the real transcripts over the recorded ones (they replace every NUC transcript: those edited by hand since milestone 5's plan 2, with `ABI 4`, `dev`, `+ cd /root` and `0.6.0`, and `check7.nuc.log`, a copy of QEMU's), say in the fixtures test's comment (`xtask/src/checks.rs`, `the_check_scripts_pass_on_both_machines`) that they are the last run's, dropping its sentence on the copy and the hand edits, and add the results-log row of `docs/hardware-test.md` (date, `3 to 7 (milestone 5 done, 0.6.0)`, this branch's commit, `Pass`, and the notes: the startup lines, the six scripts' counts, the steps by hand, check 6's two times, check 7, the error screen and the K120 key, anything the user saw).

````bash
cargo xtask verify-usb
cp /media/maw/relayroot/root/checks/check3-a.log xtask/fixtures/checks/check3-a.nuc.log
cp /media/maw/relayroot/root/checks/check3-b.log xtask/fixtures/checks/check3-b.nuc.log
cp /media/maw/relayroot/root/checks/check4.log xtask/fixtures/checks/check4.nuc.log
cp /media/maw/relayroot/root/checks/check5.log xtask/fixtures/checks/check5.nuc.log
cp /media/maw/relayroot/root/checks/check6.log xtask/fixtures/checks/check6.nuc.log
cp /media/maw/relayroot/root/checks/check7.log xtask/fixtures/checks/check7.nuc.log
cargo test -p xtask --bin xtask checks
````

Expected: `verify-usb` ends with `system.img: built …` and the six scripts' `ok` lines (84, 5, 33, 29, 46 and 39 commands), and the tests pass. Then `git diff` shows only the transcripts, the comment and the new row (check that no earlier row changed), and:

````bash
cargo xtask ci
git add xtask/fixtures/checks xtask/src/checks.rs docs/hardware-test.md
git commit -m "test(checks): record NUC checks 3 to 7 on 0.6.0"
git push
gh pr ready m5p4/release
````

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. Propose the text of the `v0.6.0` tag and the GitHub release "Relay OS 0.6.0 — milestone 5" (as milestone 4's `v0.5.0`; the plan's "Release notes" section has a draft), and make neither unless the user asks. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p4-release
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
