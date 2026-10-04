# Milestone 5 · Plan 3: The shell's environment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The shell keeps an environment (spec §8.5, §8.6, §9): it imports the block it was started with as exported variables, `export` and `unset` change them, every program gets exactly the exported ones, `A=1 cmd` gives one command its own, `PWD` and `OLDPWD` are set as bash sets them, `cd` follows `HOME` and `OLDPWD` (`cd -`), `~` and the prompt follow `HOME`, `/bin/env` lists and changes an environment as GNU's does, and every check script starts with `cd /root`. The spec's §15 item 7 and the roadmap's notes land with this plan. It ends with `cargo xtask ci` green, 52 scenarios (the new `environment` among them), `system: 46 programs, ABI 4`, and no NUC check.

**Architecture:** A new module, `crates/shell/src/vars.rs`, holds the shell's variables (moved from `expand.rs`): each a value or none and, exported, a place in the order of export; it imports a block (`Vars::import`), builds a program's (`Vars::environment_with`, a command's assignments over the exported variables), holds a built-in's assignments while it runs (`hold`/`release`) and sets `PWD` and `OLDPWD` when a shell starts (`start`). `Programs::spawn` takes the block, which `SysPrograms` passes to `sys::spawn_env`; the parser gives a command's leading assignments as `Command::assigns`, expanded after its words (`expand::command_parts`); `~` is a piece of a word, `Param::Home`. The built-ins `export` and `unset` (`commands/export.rs`) and `cd` reach the variables through `JobControl`; `env` (`commands/env.rs`) reads `Ctx::environment` and starts its command through `Ctx::programs`, given by `run_program`, which relay-rt's `run_command` now calls for every program of `/bin`. relay-rt's environment is per test thread under `cfg(test)`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; bash 5.2.21 (in a pty, and with exact environments) and GNU coreutils 9.4's `env` for comparison; QEMU 8.2 under KVM with `-cpu max`.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§8.5, §8.6, §9, §10, §11, §12, §15 items 5–7)
**Roadmap:** `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md` — this is plan 3 of 4 of milestone 5.

## In brief

- **Size.** 31 tasks in four code pull requests, plus this plan as PR 1: PR 2 exported variables (relay-rt's test double, `export` and `unset`, the import, children's environments, the scenario `environment`); PR 3 `A=1 cmd` (before programs, before built-ins by bash's rule, the 64 KiB limit); PR 4 `cd`, `PWD`, `OLDPWD`, `HOME` and `~` (with `host-shell`'s `HOME`, a script's `cd`, the check scripts' `cd /root` and the corpus); PR 5 `/bin/env` (the command, `run_program`, the program, the counts, the docs).
- **The spike** (decision 1): `cd /root` in every check script, `check3-a.sh` run from `/root/checks`, and children given only `HOME` and `PWD`: only the transcripts' missing `+ cd /root` lines and `env_calls`'s lines failed.
- **The maintainer's decisions:** the pull-request split and the carried minors (m-7 and m-8 here, m-2 to m-5 in plan 4, decision 15); assignments before a built-in follow bash's rule (decision 5); `cd` says when `PWD` does not fit the variables, and `env` leaves a killed command's status as the shell's (decisions 8 and 11).
- **bash 5.2 and GNU env 9.4 decide the details** (probes in `tmp/m5p3/progress.md`): words expanded before assignments, the import's last-wins, `export -p`'s quoting, `cd -`'s empty line and `cd -@`, the prompt as bash's `\w`, `OLDPWD` after `cd`, every `env` message.
- **The prototype's review.** A fresh reviewer read the whole prototype (then twenty-three tasks), ran the shell's unit tests, the scenarios `environment` and `checks`, throwaway unit tests and a throwaway QEMU scenario, and compared the shell with bash 5.2 in a pty and with exact environments, and `env` with GNU's 9.4. It found 0 critical, 1 important and 7 minor real defects; a gap it left unprobed (a quoted `export`) was one more. Each is settled in a task of its own after the task it concerns, with a test that fails first, or folded into the task it concerns (a ripple in `docs/hardware-test.md`, a commit's type); two design calls came to the user (decisions 8 and 11). It found correct: the words expanded before the assignments and the overlay, every runner path's environment, the hold and release of a built-in's assignments (always released, even after `exit`, Ctrl-C or a failure), `export -p`'s quoting of every byte 0x01–0x7f and several non-ASCII values, sorting, `cd`'s other rows, the prompt's `\w` rule, `~` with `HOME` empty or `/a/`, the import of odd blocks, `env`'s options, `putenv` and statuses, a background `env sleep 1 &` and Ctrl-C of `env t-spin` in QEMU, the check scripts and their transcripts, every growth bounded. It declined to judge `cd ..` in a removed directory (the vfs's path-based working directory, unchanged here) and two lists stale before this plan.

| Finding (review) | Decision |
|---|---|
| Important (I-1): with `PWD` unset `cd` kept `OLDPWD`, and `cd ""`, an empty `HOME` or `OLDPWD` did not set it, where bash's `cd` does both | Fixed, Task 16; §15 item 7 corrected |
| Minor (m-1): the in-process `env` ran its command with an environment over 64 KiB | Fixed, Task 25 |
| Minor (m-2): `env ''` said `Permission denied`, 126, where GNU says `No such file or directory`, 127 | Fixed, Task 27 |
| Minor (m-3): `export -n -x` named `-n`, where bash, reading every option first, names `-x` | Fixed, Task 3 |
| Minor (m-4): `unset -n` was an invalid option, though bash knows it | Fixed, Task 4 |
| Minor (m-5): a `PWD` or `OLDPWD` that did not fit the variables was dropped without a word | Fixed, Task 18 (the user's choice, decisions 7 and 8: said, status 1) |
| Minor (m-6): `cd -` printed `getcwd`'s path where bash prints `$OLDPWD` as written | Fixed, Task 17 |
| Minor (m-7): `docs/hardware-test.md` still listed six built-ins | Fixed in Task 31 |
| Minor (m-8): the test double's commit was typed `test` but changes code outside tests | Typed `refactor(relay-rt)`, Task 1 |
| Gap (m-9): a quoted `export` did not expand its assignments as bash's does | Fixed, Task 5 |
| Declined: a command `env` runs that is killed exits 139 with nothing said (no `exec`) | Kept, a decided difference (the user's choice, decision 11; §10) |

## Where this plan fits

Plan 3 of milestone 5 is the gate's seventh step (spec §12). It builds on plan 2's ABI 4 (`spawn`'s environment, `relay_rt::env`, `sys::spawn_env`, init's `HOME=/root`, `t-env` and `env_calls`) and on plan 1's redirection. Plan 4 runs `check7.sh` and NUC checks 3–7, recording the check scripts' `+ cd /root` lines on the NUC, and settles the deferred findings. The roadmap's notes carry what this plan leaves for it.

## Working conventions

- Plan 3 lands as **five pull requests** (table below). This plan, with the spec's §15 item 7 (with the §10 lines and status line it changes) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-04-m5-plan-3-the-shell-s-environment.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
  The check scripts' transcripts change by a command step: two of them hold escape bytes, which a markdown block cannot carry.
- Each task first adds its failing tests, runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 8, 9, 13, 23, 28, 29, 30 and 31 have no failing run (tests only, a refactor, or documents); each says why. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m5p3/mutations.md`); a review fix's red run is its own mutant.
- **The expectations are the real tools'.** bash 5.2 with `env -i HOME=/root LC_ALL=C` in a pty (`tmp/m5p3/probes/pty_env.py`) and with exact environments (`tmp/m5p3/probes/execenv`), GNU coreutils 9.4's `env` (Task 24's tests run it), bash in the corpus. In this session's shell `grep` may be a wrapper of another tool: any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop in a test.** What a person types and the block a program starts with are untrusted: the variables, the assignments, a program's environment and `env`'s entries are each bounded by 64 KiB, and nothing that follows from them may panic or overflow.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; `test` only for a commit of tests alone; a new public API is `feat`; the review's fixes carry `Refs: review <id>`, plan 2's final review's `Refs: final review <id>`. Every commit body line is within 72 columns, counted in characters. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. The scenario parser refuses an input that no `expect` of the prompt paces. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `5a6d74d` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m5p3/plan` | — | The spec's §15 item 7 with its §10 lines and status line, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m5p3/export` | 1–8 | Exported variables: `export`, `unset`, the import, children's environments, relay-rt's test double, the scenario `environment` | `lint`, `unit`, `e2e` |
| 3 | `m5p3/assign` | 9–13 | `A=1 cmd` before programs and built-ins, the 64 KiB limit | `lint`, `unit`, `e2e` |
| 4 | `m5p3/cd` | 14–23 | `PWD`, `OLDPWD`, `cd`, `HOME` and `~`, `host-shell`, a script's `cd`, the check scripts' `cd /root`, the corpus | `lint`, `unit`, `e2e` |
| 5 | `m5p3/env` | 24–31 | `env` and `/bin/env`, the counts, the docs | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline; no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 3 adds no crate; relay-rt's test double uses `std` only under `cfg(test)`. `crates/shell` uses only `vfs` and `relay-abi`.
- **No ABI change:** `relay_abi` is untouched; `spawn` already takes an environment (plan 2).
- Every scenario and check script of milestone 5's plan 2 passes, changed only where `PWD`, `cd /root`, `/bin/env` and `help` change what they print (each listed in its task); the scenarios gain `environment`. The NUC transcripts change by command only (`+ cd /root`).
- What a person types and what a program passes are untrusted (AGENTS.md): no panic, unchecked index, overflow, unbounded allocation or endless loop may follow.
- The shell follows bash 5.2 and commands GNU word for word; the only decided differences are spec §10's and §15's.
- Missing tools fail tests, never skip them: bash and GNU's coreutils are on CI's runner.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 7 in PR 1:

1. **The pull requests.** Plan 3 is five: its plan; the exported variables (the import, `export`, `unset`, children's environments, relay-rt's test double); `A=1 cmd` and the 64 KiB limit; `cd`, `PWD`, `OLDPWD`, `HOME`, `~` and the check scripts' `cd /root`; `/bin/env`. The scenario `environment` starts with the second and each later one adds to it. A spike added `cd /root` to every check script, ran `check3-a.sh` from `/root/checks` and gave children only `HOME` and `PWD`: only the recorded transcripts' missing `+ cd /root` lines and `env_calls`'s lines failed. It has no NUC check: the NUC transcripts get their `+ cd /root` line by hand until plan 4's run. The bash corpus cannot run a nested `sh` (the in-process runner's `sh` prints its trace, and `X | sh` needs programs), so export into a nested `sh` is tested by unit tests and the scenario `environment`, and the corpus compares `export`, `unset`, `A=1 cmd`, `cd` and `cd -`.
2. **The variables** (§8.5). Each variable holds a value or none (`export B` before any `B=`, `OLDPWD` at the start), and an exported one its place in the order of export. `VARS_MAX` counts names and values as before. A variable exported again or given a new value keeps its place; one unset and exported again goes last. A child's environment is the exported variables that have a value, in that order, each `NAME=value` and a NUL.
3. **The import** (§8.5). A shell imports the entries of its block whose name is a name and that are UTF-8 text, in the block's order, each exported; of two with the same name the last wins, as in bash (relay-rt's `env::var` keeps the first, as glibc's `getenv` does). `sh FILE`, `X | sh` and a nested `sh` import the block they were started with, and a script the in-process runner runs imports the block a program would have got.
4. **`export` and `unset`** (§8.5), as bash 5.2's: `export NAME[=value]...` marks each name and goes on past a bad one, ``relay-sh: export: `1A=x': not a valid identifier``, status 1; `export` and `export -p` list `declare -x NAME="value"` sorted, `"`, `$`, `` ` `` and `\` escaped with `\`, and a value holding a control character or a byte past ASCII written as `$'…'` with octal escapes, as bash quotes it in the C locale (`$'\303\251'` for `é`); `export -p NAME` exports NAME, as bash's does. `export -n` and `export -f` are `relay-sh: export: -n: not supported`, status 1; another option is bash's `invalid option`, status 2, without its usage line, judged once every option is read, as bash's are (`export -n -x` names `-x`, the prototype's review). `unset NAME` ignores a name that is not one, status 0, as bash's does (it looks for a function too), and `unset -v` names it, ``not a valid identifier``, status 1; `unset -f` and `unset -n` are `not supported`. Both are built-ins: eight in all. After `export`, quoted or not (`"export"`), a word shaped like an assignment expands as one (`export A=~/x`), as bash expands a declaration command's.
5. **`A=1 cmd`** (§8.5). As in bash, the command's words are expanded first, then the assignments, left to right, each seeing the ones before (`A=1 B=$A env` gives `B=1`), within the line's 64 KiB of expansion. A program gets the exported variables with the assignments over them: an exported name keeps its place with the new value, and the others follow in the order typed. Before a built-in they are set while it runs, and then each comes back as it was, unless the built-in set or exported it, which keeps what it did: `A=1 export A` leaves `A=1` exported, `PWD=/x cd d` leaves `cd`'s `PWD` and `OLDPWD=/x`, `HOME=/d cd` goes to `/d` and `HOME` comes back, `C=1 unset C` brings back the old `C`, as bash's do (the maintainer, 2026-10-03); `export -p` lists the values from before, as bash's does. Assignments before a command whose words expand to nothing (`A=1 $E`) stay set, as a line of assignments does. Assignments alone in a pipeline or with `&`, and `NAME+=value`, stay refused.
6. **The limit** (§8.5). The shell builds each program's environment and refuses one over 64 KiB before it starts, under both runners: `relay-sh: <name>: Argument list too long`, status 126; in a pipeline only that command fails, as one that cannot start. It is checked before the command is looked up, as the kernel's `spawn` checks it, so a command not found with too big an environment says it too, where bash says `command not found`. A built-in gets no environment, so no limit; the names it holds count in the variables' 64 KiB.
7. **`PWD` and `OLDPWD` at the start** (§8.5). The shell sets `PWD` from `getcwd` and exports it, in its place if it was imported; an imported `OLDPWD` that names a directory is kept, and otherwise `OLDPWD` is exported without a value (bash then lists no line for it in `export -p`, a quirk not copied). When the variables cannot hold them, the shell says so once, as an assignment that does not fit does (the maintainer, 2026-10-04).
8. **`cd`** (§9.1), against bash 5.2 in a pty. On success, staying where it is too (`cd ""`, an empty `HOME` or `OLDPWD`), `OLDPWD` takes the variable `PWD`'s value, or loses its value when `PWD` has none, as bash's `cd` binds it, and `PWD` the new directory from `getcwd`; each keeps whether it is exported, and one `cd` makes is not exported (the prototype's review). `cd -` prints `$OLDPWD` as it is written, as bash's does, and with an empty `OLDPWD` an empty line, status 0 (§9.1 said it prints nothing); `cd -- -` is `cd -` and `cd --` is `cd`; `cd -@` is bash's `invalid option`, status 2, as Ubuntu's bash 5.2, built without `-@`, says (§9.1 said `not supported`); `cd -e` stays `not supported`. `cd //tmp` sets `PWD` to `/tmp`, where bash keeps `//tmp`. When the variables cannot hold the new `PWD` or `OLDPWD`, `cd` goes there all the same and says the variables would hold more than 64 KiB, status 1 (the maintainer, 2026-10-04).
9. **`~` and the prompt** (§8.5). `~` becomes a part of the word, expanded with its variables: `$HOME`, or `/root` when `HOME` is unset; an empty `HOME` makes `~` an empty word and `~/x` `/x`, as in bash. The prompt shows `~` for `$HOME`'s directory and below as bash's `\w` does: only when `HOME` is set and longer than one byte, so with `HOME` unset or empty it shows the whole path (§8.5 said `/root` when unset; bash uses the password file for `~` but not for `\w`).
10. **A script's `cd`** (§9.2). Under `/bin/sh` a script is a process of its own; the in-process runner now goes back to the directory it ran the script from, so a script's `cd` stays in it under both.
11. **`/bin/env`** (§8.6). A command function like the others: it reads the environment its command was given, and starts its command through the program's `Programs` as a program, waiting for it, its status the command's; in the in-process runner it runs the command's function with the new environment. Its options stop at the first word that is not one, as GNU's do (`env A=1 -i` runs `-i`): `-i` and `-`, `-u NAME` (``env: cannot unset 'A=B': Invalid argument``, status 125, for a name holding `=`), `--`; `NAME=value` replaces an entry in its place or adds one at the end, as GNU's `putenv` does. Long options are refused, as every command here refuses them. A command not found is `env: 'X': No such file or directory`, status 127 (an empty name too), one that cannot run status 126, a directory `Permission denied` as Linux's `execve` says, and an environment over 64 KiB `Argument list too long`, under both runners (the prototype's review). With no `exec`, `env` waits for its command: one that is killed leaves `env` the status the shell would give it (139 for a page fault), and nothing is printed but the kernel's log line, where bash names the signal itself (§10; the maintainer, 2026-10-04). `/bin` holds 46 programs.
12. **relay-rt's test double** (§8.3). Under `cfg(test)` a program's environment is the test thread's own, so tests that give one never see each other's (plan 2's final review, m-8).
13. **`t-env` and `env_calls`.** `t-env raw` says `1 entry` (m-8). From the prompt `t-env` shows `HOME=/root` and `PWD=/root`, and through `sh` the grandchild gets `sh`'s exported variables.
14. **The prototype's review** found no critical defect, one important (`cd`'s `OLDPWD`, above) and eight minor ones, each fixed in a task of its own with a test that fails first, or folded into the task it concerns (a ripple in `docs/hardware-test.md`, a commit's type).
15. **Plan 2's deferred minors.** Plan 3 settles m-8 with the test double and m-7, README's crate table for `DevFs` and the environment; m-2 (`kernel/src/file.rs`'s comment on a device's `seek`), m-3 (the `FileSystem` contract for devices), m-4 (`xtask`'s `3 -> 2`) and m-5 (the NUC transcripts edited by hand) go to plan 4 (the maintainer, 2026-10-03).

## Review Focus

The final whole-branch review should look hardest where a person or a script meets the environment, most likely first:

1. **Assignments before a command** against bash 5.2: before each of the eight built-ins, programs, `sh FILE`, `env`, in pipelines, jobs and compound commands, with redirections; the hold and release rule (a built-in that fails, `exit`, Ctrl-C, the same name twice, `HOME`, `PWD` and `OLDPWD` held for `cd`); nothing temporary left behind, nothing real lost.
2. **What a child gets**, end to end under `/bin/sh`: init → sh → program, `sh FILE`, `X | sh`, a nested `sh`, jobs, pipeline stages, `/bin/env`; the order, a name without a value, the 64 KiB boundary under both runners.
3. **`cd`, `PWD`, `OLDPWD`, `~` and the prompt** against bash 5.2 in a pty, every row of §9.1 and §15 item 7, and the variables full.
4. **`export`, `unset` and the import** against bash 5.2: quoting, options, bad names, odd blocks.
5. **`/bin/env`** against GNU env 9.4: options, assignments, messages, statuses, a killed command, Ctrl-C, its child's group.
6. **The check scripts:** `cd /root` in each, the ten transcripts (the NUC's edited by command), `checks.txt`.
7. **Each pull request green and safe on its own**, and every commit against `CONTRIBUTING.md`.

---

## PR 1: The spec's §15 item 7, the roadmap's notes and this plan

The spec's §15 item 7, the decisions of this plan, with the §10 lines and the status line it changes; the roadmap's status and its notes for plans 3 and 4; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and the roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 5 plan 3's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p3/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m5p3/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-04-m5-plan-3-the-shell-s-environment.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-04-m5-plan-3-the-shell-s-environment.md
git commit -m "docs(plan): add milestone 5 plan 3, the shell's environment"
cargo xtask lint
git push -u origin m5p3/plan
gh pr create --base main --head m5p3/plan --title "docs(spec,plan): add milestone 5 plan 3, the shell's environment" --body-file - <<'EOF2'
## What

The implementation plan of milestone 5's plan 3 ("The shell's environment"), with the spec's §15 item 7: five pull requests and the spike; the variables with export state and order; the import (last of two names wins, a bad name or bytes not UTF-8 dropped); `export` and `unset` as bash 5.2's; `A=1 cmd`, its words expanded first, and bash's rule for a built-in's assignments (the maintainer's choice); a program's environment at most 64 KiB; `PWD` and `OLDPWD` at the start; `cd` against bash 5.2 (`cd -`, `HOME`, `OLDPWD` after a `cd`, a `PWD` that does not fit said); `~` and the prompt following `HOME` as bash's `\w`; a script's `cd` kept in it; `/bin/env` as GNU's, a killed command's status left as the shell's; relay-rt's test double; the prototype's review; plan 2's deferred minors (m-7 and m-8 here, the rest for plan 4). §10 gains three lines; the roadmap's notes carry what plan 4 inherits.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (the harness's `bind_pr`); never poll CI. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m5p3/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-plan` (`.superpowers` is not linked there) and continue with PR 2.

---

## PR 2: Exported variables (Tasks 1–8)

relay-rt's test double gives each test thread its own environment; the shell's variables keep whether and in what order they are exported, `export` and `unset` are built-ins, a shell imports the environment it was started with, and every program gets exactly the exported variables, where `/bin/sh` handed its whole block on; the scenario `environment` starts.

Branch `m5p3/export`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-export`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p3/export /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-export origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-export
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p3/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p3/export /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-export m5p3/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p3/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: relay-rt's test double: an environment per test thread

Decision 12 (spec §8.3; plan 2's final review, m-8): the program's environment lived in three statics, so a test that set it could change what another test saw, and `the_program_s_block_is_empty_until_set` passed only while no other test called `env::set`. Under `cfg(test)` the block's address, length and count are now the test thread's own (a `thread_local`); on Relay OS they stay in the statics, moved into a module `raw` with `store` and `load`. The red run is relay-rt's tests: the new test of two threads fails, and so does the old one, which now meets the other test's block. No guard to mutate: a `store` that does nothing fails every environment test.

**Files:**
- Modify: `crates/relay-rt/src/env.rs`

**Interfaces:**
- Consumes: plan 2's `relay_rt::env`.
- Produces: nothing new outside relay-rt; `env::set`, `raw()`, `program()` unchanged.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/env.rs`**

In `crates/relay-rt/src/env.rs`, replace:

````rust
        assert_eq!(vars().count(), 2);
    }
}
````

with:

````rust
        assert_eq!(vars().count(), 2);
    }

    #[test]
    fn each_test_thread_has_an_environment_of_its_own() {
        let b: &'static [u8] = b"A=1\0";
        // SAFETY: a static block.
        unsafe { set(b.as_ptr(), b.len(), 1) };
        let other = std::thread::spawn(|| {
            let before = block();
            let c: &'static [u8] = b"B=2\0";
            // SAFETY: a static block.
            unsafe { set(c.as_ptr(), c.len(), 1) };
            (before, var(b"B"))
        });
        assert_eq!(other.join().unwrap(), (&b""[..], Some(&b"2"[..])));
        assert_eq!(
            block(),
            b"A=1\0",
            "the other thread's set changed nothing here"
        );
        assert_eq!(var(b"B"), None);
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: 2 tests fail: `env::tests::the_program_s_block_is_empty_until_set`, `env::tests::each_test_thread_has_an_environment_of_its_own`.

- [ ] **Step 3: Change `crates/relay-rt/src/env.rs`**

In `crates/relay-rt/src/env.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! and Rust's `std` read `environ`.

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

````

with:

````rust
//! and Rust's `std` read `environ`.

````

Replace:

````rust
/// The program's block, as the registers gave it before `main` ran.
static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

````

with:

````rust
/// The program's block, as the registers gave it before `main` ran.
#[cfg(not(test))]
mod raw {
    use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

    static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
    static LEN: AtomicUsize = AtomicUsize::new(0);
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    pub fn store(ptr: *const u8, len: usize, count: usize) {
        PTR.store(ptr.cast_mut(), Ordering::Relaxed);
        LEN.store(len, Ordering::Relaxed);
        COUNT.store(count, Ordering::Relaxed);
    }

    pub fn load() -> (*const u8, usize, usize) {
        (
            PTR.load(Ordering::Relaxed),
            LEN.load(Ordering::Relaxed),
            COUNT.load(Ordering::Relaxed),
        )
    }
}

/// The test double (programmable shell gate §8.3): each test thread has a
/// program's environment of its own, so tests that give one never see
/// each other's.
#[cfg(test)]
mod raw {
    use core::cell::Cell;

    std::thread_local! {
        static RAW: Cell<(usize, usize, usize)> = const { Cell::new((0, 0, 0)) };
    }

    pub fn store(ptr: *const u8, len: usize, count: usize) {
        RAW.with(|r| r.set((ptr as usize, len, count)));
    }

    pub fn load() -> (*const u8, usize, usize) {
        let (ptr, len, count) = RAW.with(Cell::get);
        (ptr as *const u8, len, count)
    }
}

````

Replace:

````rust
pub(crate) unsafe fn set(ptr: *const u8, len: usize, count: usize) {
    PTR.store(ptr.cast_mut(), Ordering::Relaxed);
    LEN.store(len, Ordering::Relaxed);
    COUNT.store(count, Ordering::Relaxed);
}
````

with:

````rust
pub(crate) unsafe fn set(ptr: *const u8, len: usize, count: usize) {
    raw::store(ptr, len, count);
}
````

Replace:

````rust
pub fn raw() -> (usize, usize, usize) {
    let p = PTR.load(Ordering::Relaxed);
    (
        p as usize,
        LEN.load(Ordering::Relaxed),
        COUNT.load(Ordering::Relaxed),
    )
}

/// The program's environment.
pub fn program() -> Block {
    let p = PTR.load(Ordering::Relaxed);
    let bytes: &'static [u8] = if p.is_null() {
        &[]
    } else {
        // SAFETY: as `set` was promised.
        unsafe { core::slice::from_raw_parts(p, LEN.load(Ordering::Relaxed)) }
    };
    Block::new(bytes, COUNT.load(Ordering::Relaxed))
}
````

with:

````rust
pub fn raw() -> (usize, usize, usize) {
    let (p, len, count) = raw::load();
    (p as usize, len, count)
}

/// The program's environment.
pub fn program() -> Block {
    let (p, len, count) = raw::load();
    let bytes: &'static [u8] = if p.is_null() {
        &[]
    } else {
        // SAFETY: as `set` was promised.
        unsafe { core::slice::from_raw_parts(p, len) }
    };
    Block::new(bytes, count)
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 35 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(relay-rt): give each test thread an environment of its own

The program's environment lived in one static, so a test that set it
could change what another test saw: the empty-until-set test passed
only while no other test called env::set. Under cfg(test) the block's
address, length and count are now the test thread's own, the test
double of the spec's §8.3; on Relay OS they stay in the statics.

Refs: final review m8
EOF
````


### Task 2: `export` and `unset`, and the variables they keep

Decisions 2 and 4 (spec §8.5): `Vars` moves out of `expand.rs` into a module of its own, `crates/shell/src/vars.rs`, and each variable holds a value or none (`export B` before any `B=`) and, once exported, its place in the order of export; `VARS_MAX` still counts names and values, so a variable exported without a value counts its name. `export [-p] [NAME[=value]...]` marks names and lists the exported ones as bash 5.2's `declare -x` does in the C locale: in double quotes with `"`, `$`, `` ` `` and `\` escaped, or in `$'…'` with octal escapes for a control character or a byte past ASCII (probed byte by byte with `env -i HOME=/root LC_ALL=C bash`); each bad name is told and the rest still done; `-n` and `-f` are not supported, another option is bash's `invalid option`, status 2. `unset [-v] NAME...` removes variables, passing over a bad name unless given `-v`, as bash does (it looks for a function too). Both are the shell's own commands, reaching its variables through `JobControl`, now eight. `export`'s words shaped like an assignment expand as an assignment's value (`export A=~/x`, `D=$@` joined by blanks), as bash expands a declaration command's; a plain argument keeps its `~` (user-space gate §16 item 11). The red run is the shell's tests: `export` and `unset` are missing. Mutation checks (12): the 64 KiB bound, a variable without a value counting nothing, `unset` freeing nothing, a bad name taken, `-v` always or never, refused and unknown options allowed, DEL taken as printable, every command's assignments expanded as `export`'s, `--` not ending the options, each fail a test; `named` resetting the variables is caught once Task 6 imports before it.

**Files:**
- Create: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`
- Create: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: the shell's expansion (`expand.rs`), `JobControl`.
- Produces: `crate::vars::{Vars, VARS_MAX}` (`Vars::{new, script, set_name, positional, value, set, export, unset, exported}`), the built-ins `export` and `unset` (`commands::{export, unset}`), `Ctx::vars()`, `JobControl::vars`, `Word::is_plain`, `expand::command_words`.

- [ ] **Step 1: Write the failing tests for `crates/shell/src/commands/export.rs`**

Create `crates/shell/src/commands/export.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::quoted;
    use crate::testing::Harness;

    #[test]
    fn export_marks_variables_and_lists_them_as_bash_does() {
        // `env -i HOME=/root bash` in a pty, but for the variables bash
        // sets itself (`OLDPWD`, `PWD`, `SHLVL`).
        let mut h = Harness::new();
        let (status, out) = h.lines(&[
            "A=1",
            "export A B",
            "export C=3 D= E='a \"b\" $c `d` \\e'",
            "export",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "declare -x A=\"1\"\n\
             declare -x B\n\
             declare -x C=\"3\"\n\
             declare -x D=\"\"\n\
             declare -x E=\"a \\\"b\\\" \\$c \\`d\\` \\\\e\"\n"
        );
        assert_eq!(
            h.lines(&["export Z=1", "export -p"]).1,
            "declare -x Z=\"1\"\n"
        );
        // `-p` with names exports them, and `--` ends the options.
        assert_eq!(
            h.lines(&["export -p Y", "export -- X=-", "export"]).1,
            "declare -x X=\"-\"\ndeclare -x Y\n"
        );
    }

    #[test]
    fn a_value_bash_would_not_print_as_it_is_is_written_with_escapes() {
        // As bash 5.2's `export -p` in the C locale.
        assert_eq!(quoted("plain words"), "\"plain words\"");
        assert_eq!(quoted("x\ny"), "$'x\\ny'");
        assert_eq!(quoted("\t"), "$'\\t'");
        assert_eq!(quoted("été"), "$'\\303\\251t\\303\\251'");
        assert_eq!(
            quoted("\x07\x08\x1b\x0c\n\r\t\x0b\x7f\x01"),
            "$'\\a\\b\\E\\f\\n\\r\\t\\v\\177\\001'"
        );
        assert_eq!(quoted("é$`\""), "$'\\303\\251$`\"'");
        assert_eq!(quoted("\t'\\\\"), "$'\\t\\'\\\\\\\\'");
        assert_eq!(quoted("it's"), "\"it's\"");
    }

    #[test]
    fn export_names_each_bad_identifier_and_goes_on() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["export 1A=x B=2 A-B - =x", "export"]),
            (
                0,
                "relay-sh: export: `1A=x': not a valid identifier\n\
                 relay-sh: export: `A-B': not a valid identifier\n\
                 relay-sh: export: `-': not a valid identifier\n\
                 relay-sh: export: `=x': not a valid identifier\n\
                 declare -x B=\"2\"\n"
                    .into()
            )
        );
        assert_eq!(h.run("export 1A").0, 1);
    }

    #[test]
    fn export_s_other_options_are_refused() {
        let mut h = Harness::new();
        for (line, status, message) in [
            ("export -n A", 1, "relay-sh: export: -n: not supported\n"),
            ("export -f f", 1, "relay-sh: export: -f: not supported\n"),
            ("export -pn A", 1, "relay-sh: export: -n: not supported\n"),
            // bash's, without its usage line.
            ("export -x A", 2, "relay-sh: export: -x: invalid option\n"),
            ("export -nx A", 2, "relay-sh: export: -x: invalid option\n"),
        ] {
            assert_eq!(h.lines(&[line, "export"]), (0, message.into()), "{line}");
            assert_eq!(h.run(line).0, status, "{line}");
        }
    }

    #[test]
    fn unset_removes_variables_exported_or_not() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&[
                "A=1 B=2 C=3",
                "export B D",
                "unset A B",
                "unset -v -- D NEVER",
                "echo [$A$B$C]",
                "export",
            ]),
            (0, "[3]\n".into())
        );
        assert_eq!(h.run("unset"), (0, "".into()));
        // Options end at the first name: a later `-v` is a name.
        assert_eq!(h.run("unset A -v"), (0, "".into()));
    }

    #[test]
    fn unset_passes_over_a_bad_name_unless_given_v() {
        // bash looks for a function of that name too, without `-v`.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["A=1", "unset 1A A=1 A", "echo [$A]"]),
            (0, "[]\n".into())
        );
        assert_eq!(
            h.lines(&["A=1 B=2", "unset -v 1A A A=1 B", "echo $? [$A$B]"]),
            (
                0,
                "relay-sh: unset: `1A': not a valid identifier\n\
                 relay-sh: unset: `A=1': not a valid identifier\n\
                 1 []\n"
                    .into()
            )
        );
        for (line, status, message) in [
            ("unset -f f", 1, "relay-sh: unset: -f: not supported\n"),
            ("unset -fv A", 1, "relay-sh: unset: -f: not supported\n"),
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
        ] {
            assert_eq!(h.run(line), (status, message.into()), "{line}");
        }
    }

    #[test]
    fn export_s_assignments_expand_as_assignments() {
        // A `~` after `=` or `:`, and `$@` joined by blanks, as bash's
        // `export` (a declaration command) has them; a plain argument
        // keeps its `~` (user-space gate §16 item 11).
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"export A=~/x B=x:~ \"C=~/y\" D=$@\nexport\necho a=~/x\n",
        );
        let (status, said) = h.run("sh /tmp/s.sh 1 '2 3'");
        assert_eq!(status, 0);
        assert!(
            said.contains(
                "declare -x A=\"/root/x\"\n\
                 declare -x B=\"x:/root\"\n\
                 declare -x C=\"~/y\"\n\
                 declare -x D=\"1 2 3\"\n"
            ),
            "{said}"
        );
        assert!(said.ends_with("a=~/x\n"), "{said}");
    }

    #[test]
    fn a_variable_export_cannot_hold_fails() {
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 100);
        assert_eq!(
            h.lines(&[
                &alloc::format!("B={big}"),
                &alloc::format!("export A={}", &big[..200]),
                "export"
            ]),
            (
                0,
                "relay-sh: A: the variables would hold more than 64 KiB\n".into()
            )
        );
    }
}
````

- [ ] **Step 2: Add the failing tests and the module declaration to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod control;
mod grep;
````

with:

````rust
mod control;
mod export;
mod grep;
````

Replace:

````rust
    #[test]
    fn the_shell_s_own_commands_are_cd_exit_help_and_the_job_commands() {
        let own: alloc::vec::Vec<_> = COMMANDS
````

with:

````rust
    #[test]
    fn the_shell_s_own_commands_are_cd_exit_export_help_unset_and_the_job_commands() {
        let own: alloc::vec::Vec<_> = COMMANDS
````

Replace:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs", "kill", "wait"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

with:

````rust
            .collect();
        assert_eq!(
            own,
            [
                "cd", "exit", "export", "help", "jobs", "kill", "unset", "wait"
            ]
        );
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
    #[test]
    fn the_variables_hold_at_most_64_kib() {
        let mut v = Vars::new("sh");
        let big = "x".repeat(VARS_MAX - 1);
        v.set("A", big.clone()).unwrap();
        assert_eq!(v.set("B", String::new()), Err(Error::Full("B".into())));
        assert_eq!(v.set("A", big.clone() + "y"), Err(Error::Full("A".into())));
        assert_eq!(v.get("A"), big, "unchanged");
        // A smaller value makes room again.
        v.set("A", String::from("1")).unwrap();
        v.set("B", "x".repeat(VARS_MAX - 3)).unwrap();
        assert_eq!(
            Error::Full("B".into()).to_string(),
            "B: the variables would hold more than 64 KiB"
        );
    }

    #[test]
    fn a_line_expands_to_at_most_64_kib() {
````

with:

````rust
    #[test]
    fn a_line_expands_to_at_most_64_kib() {
````

- [ ] **Step 4: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod transcript;

````

with:

````rust
mod transcript;
mod vars;

````

- [ ] **Step 5: Write the failing tests for `crates/shell/src/vars.rs`**

Create `crates/shell/src/vars.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variables_hold_at_most_64_kib() {
        let mut v = Vars::new("sh");
        let big = "x".repeat(VARS_MAX - 1);
        v.set("A", big.clone()).unwrap();
        assert_eq!(v.set("B", String::new()), Err(Error::Full("B".into())));
        assert_eq!(v.set("A", big.clone() + "y"), Err(Error::Full("A".into())));
        assert_eq!(v.get("A"), big, "unchanged");
        // A smaller value makes room again.
        v.set("A", String::from("1")).unwrap();
        v.set("B", "x".repeat(VARS_MAX - 3)).unwrap();
        assert_eq!(
            Error::Full("B".into()).to_string(),
            "B: the variables would hold more than 64 KiB"
        );
    }

    #[test]
    fn an_exported_variable_without_a_value_counts_its_name() {
        let mut v = Vars::new("sh");
        v.set("A", "x".repeat(VARS_MAX - 2)).unwrap();
        v.export("B", None).unwrap();
        assert_eq!(v.export("C", None), Err(Error::Full("C".into())));
        assert_eq!(v.exported().count(), 1, "nothing of C was kept");
        // Unset, a variable frees its room.
        v.unset("A");
        v.export("C", Some("x".repeat(VARS_MAX - 2))).unwrap();
    }

    #[test]
    fn exported_lists_every_exported_variable_by_name() {
        let mut v = Vars::new("sh");
        v.export("OLDPWD", None).unwrap();
        v.export("B", Some(String::from("1"))).unwrap();
        v.set("A", String::from("x")).unwrap();
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [("B", Some("1")), ("OLDPWD", None)]
        );
        assert_eq!((v.get("OLDPWD"), v.value("OLDPWD")), ("", None));
        assert_eq!((v.get("B"), v.value("B")), ("1", Some("1")));
        assert_eq!(v.value("UNSET"), None);
    }

    #[test]
    fn unset_removes_a_variable_exported_or_not() {
        let mut v = Vars::new("sh");
        v.set("A", String::from("1")).unwrap();
        v.export("B", Some(String::from("2"))).unwrap();
        v.unset("A");
        v.unset("B");
        v.unset("NEVER");
        assert_eq!((v.value("A"), v.value("B")), (None, None));
        assert_eq!(v.exported().count(), 0);
        assert_eq!(v.size, 0);
    }
}
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `VARS_MAX` in module `crate::vars` ``; `` cannot find type `Vars` in this scope ``.

- [ ] **Step 7: Implement `crates/shell/src/commands/export.rs`**

Insert this at the top of `crates/shell/src/commands/export.rs`, above `#[cfg(test)]`:

````rust
//! `export` and `unset`, the shell's commands for its variables
//! (programmable shell gate §8.5), as bash 5.2's.

use crate::ctx::{Ctx, outln};
use crate::parser::is_name;
use crate::shell::NAME;
use alloc::format;
use alloc::string::String;

/// `export [-p] [NAME[=value]...]`: marks each NAME for the environment of
/// the programs the shell starts, giving it the value if one follows, and
/// goes on past a name that is not one (status 1). With no NAME it lists
/// the exported variables as bash's `declare -x` does. `-n` and `-f`
/// (bash's un-export and functions) are not supported.
pub fn export(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let names = match options(ctx, "export", args, "fnp", "fn") {
        Ok((_, names)) => names,
        Err(status) => return status,
    };
    if names.is_empty() {
        return list(ctx);
    }
    let mut status = 0;
    for arg in names {
        let (name, value) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(String::from(value))),
            None => (arg.as_str(), None),
        };
        if !is_name(name) {
            status = ctx.fail(
                NAME,
                format_args!("export: `{arg}': not a valid identifier"),
            );
            continue;
        }
        if let Err(e) = ctx.vars().export(name, value) {
            status = ctx.fail(NAME, format_args!("{e}"));
        }
    }
    status
}

/// `unset [-v] [NAME...]`: removes each variable, exported or not. As in
/// bash, without `-v` a name that is not one is passed over (bash looks
/// for a function of that name too); with it, it fails (status 1). `-f`
/// (functions) is not supported.
pub fn unset(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (letters, names) = match options(ctx, "unset", args, "fv", "f") {
        Ok(read) => read,
        Err(status) => return status,
    };
    let only_variables = letters.contains('v');
    let mut status = 0;
    for name in names {
        if is_name(name) {
            ctx.vars().unset(name);
        } else if only_variables {
            status = ctx.fail(
                NAME,
                format_args!("unset: `{name}': not a valid identifier"),
            );
        }
    }
    status
}

/// A built-in's option letters and the words after them, as bash reads
/// them: options come first, a word `-` or `--` ends them (`--` dropped),
/// and a letter not in `known` is bash's `invalid option` (status 2,
/// without its usage line); one of `refused`, which bash knows, is not
/// supported (status 1).
fn options<'a>(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &'a [String],
    known: &str,
    refused: &str,
) -> Result<(String, &'a [String]), i32> {
    let mut first = 0;
    let mut seen = String::new();
    for arg in args {
        if arg == "--" {
            first += 1;
            break;
        }
        let Some(letters) = arg.strip_prefix('-').filter(|l| !l.is_empty()) else {
            break;
        };
        if let Some(bad) = letters.chars().find(|c| !known.contains(*c)) {
            ctx.fail(NAME, format_args!("{name}: -{bad}: invalid option"));
            return Err(2);
        }
        if let Some(no) = letters.chars().find(|c| refused.contains(*c)) {
            return Err(ctx.fail(NAME, format_args!("{name}: -{no}: not supported")));
        }
        seen.push_str(letters);
        first += 1;
    }
    Ok((seen, &args[first..]))
}

/// The exported variables, sorted by name, as bash's `export -p`.
fn list(ctx: &mut Ctx<'_>) -> i32 {
    let lines: alloc::vec::Vec<String> = ctx
        .vars()
        .exported()
        .map(|(name, value)| match value {
            Some(v) => format!("declare -x {name}={}", quoted(v)),
            None => format!("declare -x {name}"),
        })
        .collect();
    for line in lines {
        outln!(ctx, "{line}");
    }
    0
}

/// A value as bash 5.2 writes it after `declare -x NAME=` in the C locale:
/// in double quotes, `"`, `$`, `` ` `` and `\` escaped, unless it holds a
/// byte that is no printable ASCII; then in `$'…'`, as bash's
/// `ansic_quote` writes it.
fn quoted(value: &str) -> String {
    let printable = |b: u8| (b' '..=b'~').contains(&b);
    let mut out = String::new();
    if value.bytes().all(printable) {
        out.push('"');
        for c in value.chars() {
            if matches!(c, '"' | '$' | '`' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        return out;
    }
    out.push_str("$'");
    for b in value.bytes() {
        match b {
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x1b => out.push_str("\\E"),
            0x0c => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x0b => out.push_str("\\v"),
            b'\'' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            b if printable(b) => out.push(char::from(b)),
            b => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push('\'');
    out
}

````

- [ ] **Step 8: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
pub use control::{jobs, kill, wait};
pub use grep::grep;
````

with:

````rust
pub use control::{jobs, kill, wait};
pub use export::{export, unset};
pub use grep::grep;
````

Replace:

````rust
        run: basic::exit,
    },
````

with:

````rust
        run: basic::exit,
    },
    Builtin {
        name: "export",
        help: "give variables to the programs the shell starts",
        run: export::export,
    },
````

Replace:

````rust
    Builtin {
        name: "wait",
````

with:

````rust
    Builtin {
        name: "unset",
        help: "remove variables",
        run: export::unset,
    },
    Builtin {
        name: "wait",
````

Replace:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs", "kill", "wait"];

````

with:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &[
    "cd", "exit", "export", "help", "jobs", "kill", "unset", "wait",
];

````

- [ ] **Step 9: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::transcript::Transcript;
use alloc::format;
````

with:

````rust
use crate::transcript::Transcript;
use crate::vars::Vars;
use alloc::format;
````

Replace:

````rust

/// What the shell's job commands work with: its jobs, and its programs
/// (none in the in-process runner, which starts no job).
pub(crate) struct JobControl<'a> {
    pub jobs: &'a mut Jobs,
    pub programs: Option<&'a mut dyn Programs>,
````

with:

````rust

/// What the shell's own commands work with: its jobs, its programs (none
/// in the in-process runner, which starts no job) and its variables.
pub(crate) struct JobControl<'a> {
    pub jobs: &'a mut Jobs,
    pub vars: &'a mut Vars,
    pub programs: Option<&'a mut dyn Programs>,
````

Replace:

````rust
        self.write_error_status = status;
    }
````

with:

````rust
        self.write_error_status = status;
    }

    /// The shell's variables, for its own commands.
    pub(crate) fn vars(&mut self) -> &mut Vars {
        let Some(control) = self.control.as_mut() else {
            unreachable!("the shell's own commands run with its variables")
        };
        control.vars
    }
````

- [ ] **Step 10: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
use crate::parser::{Command, Param, Piece, Redirect, RedirectOp, Word};
use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
````

with:

````rust
use crate::parser::{Command, Param, Piece, Redirect, RedirectOp, Word};
pub(crate) use crate::vars::Vars;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
````

Replace:

````rust
pub const EXPANSION_MAX: usize = 64 * 1024;

/// The most a shell's variables hold, their names' and values' bytes.
pub const VARS_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
    args: Vec<String>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            size: 0,
            args: alloc::vec![String::from(name)],
        }
    }

    /// A script's: none set, `$0` its `name` and `args` after it.
    pub fn script(name: &str, args: &[String]) -> Vars {
        let mut vars = Vars::new(name);
        vars.args.extend_from_slice(args);
        vars
    }

    /// The arguments after `$0`, which `"$@"` gives.
    pub fn positional(&self) -> &[String] {
        self.args.get(1..).unwrap_or(&[])
    }

    /// The variable `name`'s value; an unset one is empty.
    pub fn get(&self, name: &str) -> &str {
        self.names.get(name).map_or("", String::as_str)
    }

    /// Sets the variable `name` to `value`, unless the variables would
    /// then hold more than `VARS_MAX`.
    pub fn set(&mut self, name: &str, value: String) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| name.len() + v.len());
        let size = self.size - old + name.len() + value.len();
        if size > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = size;
        self.names.insert(String::from(name), value);
        Ok(())
    }
}

````

with:

````rust
pub const EXPANSION_MAX: usize = 64 * 1024;

````

Replace:

````rust
    TooLong,
    /// The variable would make the variables hold more than `VARS_MAX`.
    Full(String),
````

with:

````rust
    TooLong,
    /// The variable would make the variables hold more than `crate::vars::VARS_MAX`.
    Full(String),
````

Replace:

````rust

/// `words` expanded as a command's arguments are (a `for`'s list).
````

with:

````rust

/// A command's `words` expanded: `export`'s assignments as assignments
/// are (`export A=~/x`), as bash expands a declaration command's.
pub(crate) fn command_words(
    words: &[Word],
    vars: &Vars,
    status: i32,
) -> Result<Vec<String>, Error> {
    Expander::new(vars, status).words(words)
}

/// `words` expanded as a command's arguments are (a `for`'s list).
````

Replace:

````rust
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    Ok(Expander::new(vars, status)
        .fields(word)?
        .into_iter()
        .map(|f| f.text)
        .collect::<Vec<_>>()
        .join(" "))
}
````

with:

````rust
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    Expander::new(vars, status).joined(word)
}
````

Replace:

````rust
    fn command(&mut self, c: &Command<Word>) -> Result<Command, Error> {
        let mut words = Vec::new();
        for w in &c.words {
            words.extend(self.word(w)?);
        }
        let mut redirects = Vec::new();
````

with:

````rust
    fn command(&mut self, c: &Command<Word>) -> Result<Command, Error> {
        let words = self.words(&c.words)?;
        let mut redirects = Vec::new();
````

Replace:

````rust
        Ok(Command { words, redirects })
    }
````

with:

````rust
        Ok(Command { words, redirects })
    }

    /// A command's words. After an unquoted `export`, a word shaped like an
    /// assignment is one word, `NAME=` and its value expanded as an
    /// assignment's (a `~` after the `=` or a `:`, `$@` joined by blanks).
    fn words(&mut self, words: &[Word]) -> Result<Vec<String>, Error> {
        let export = words.first().is_some_and(|w| w.is_plain("export"));
        let mut out = Vec::new();
        for (i, w) in words.iter().enumerate() {
            match w.assignment().filter(|_| export && i > 0) {
                Some((name, value)) => {
                    let value = self.joined(&value)?;
                    out.push(alloc::format!("{name}={value}"));
                }
                None => out.extend(self.word(w)?),
            }
        }
        Ok(out)
    }

    /// What `word` expands to as one string, `$@`'s words joined by blanks.
    fn joined(&mut self, word: &Word) -> Result<String, Error> {
        Ok(self
            .fields(word)?
            .into_iter()
            .map(|f| f.text)
            .collect::<Vec<_>>()
            .join(" "))
    }
````

Replace:

````rust
    expand(commands, &Vars::new(crate::shell::NAME), 0)
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        let mut vars = Vars::new("");
        for (n, v) in names {
            vars.set(n, String::from(*v)).unwrap();
        }
        vars.args = args.iter().map(|a| String::from(*a)).collect();
        vars
    }
}
````

with:

````rust
    expand(commands, &Vars::new(crate::shell::NAME), 0)
}
````

- [ ] **Step 11: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

````

with:

````rust
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

    /// The word is `text`, unquoted, and nothing else.
    pub fn is_plain(&self, text: &str) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t == text)
    }

````

- [ ] **Step 12: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub fn named(mut self, name: &str) -> Shell<'a> {
        self.vars = Vars::new(name);
        self
````

with:

````rust
    pub fn named(mut self, name: &str) -> Shell<'a> {
        self.vars.set_name(name);
        self
````

Replace:

````rust
        let typed = &commands[0];
        let words = match expand::words(&typed.words, &self.vars, self.status) {
            Ok(words) => words,
````

with:

````rust
        let typed = &commands[0];
        let words = match expand::command_words(&typed.words, &self.vars, self.status) {
            Ok(words) => words,
````

Replace:

````rust
                        jobs: &mut self.jobs,
                        programs: self.runner.programs(),
````

with:

````rust
                        jobs: &mut self.jobs,
                        vars: &mut self.vars,
                        programs: self.runner.programs(),
````

- [ ] **Step 13: Implement `crates/shell/src/vars.rs`**

Insert this at the top of `crates/shell/src/vars.rs`, above `#[cfg(test)]`:

````rust
//! A shell's variables and arguments (user-space gate §9.4), and which
//! variables it exports (programmable shell gate §8.5): an exported
//! variable goes into the environment of every program the shell starts,
//! in the order the variables were exported.

use crate::expand::Error;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The most a shell's variables hold, their names' and values' bytes.
pub const VARS_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, Var>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
    pub(crate) args: Vec<String>,
    /// The place the next variable exported takes.
    next: u64,
}

/// One variable: its value, none for one exported before it was given
/// one (`export B`, `OLDPWD` when the shell starts), and its place among
/// the exported ones.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Var {
    value: Option<String>,
    export: Option<u64>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            size: 0,
            args: alloc::vec![String::from(name)],
            next: 0,
        }
    }

    /// A script's: none set, `$0` its `name` and `args` after it.
    pub fn script(name: &str, args: &[String]) -> Vars {
        let mut vars = Vars::new(name);
        vars.args.extend_from_slice(args);
        vars
    }

    /// `$0` becomes `name`.
    pub fn set_name(&mut self, name: &str) {
        self.args[0] = String::from(name);
    }

    /// The arguments after `$0`, which `"$@"` gives.
    pub fn positional(&self) -> &[String] {
        self.args.get(1..).unwrap_or(&[])
    }

    /// The variable `name`'s value; an unset one, or one without a value,
    /// is empty.
    pub fn get(&self, name: &str) -> &str {
        self.value(name).unwrap_or("")
    }

    /// The variable `name`'s value, if it has one.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.names.get(name).and_then(|v| v.value.as_deref())
    }

    /// Sets the variable `name` to `value`, exported or not as it was,
    /// unless the variables would then hold more than `VARS_MAX`.
    pub fn set(&mut self, name: &str, value: String) -> Result<(), Error> {
        let export = self.names.get(name).and_then(|v| v.export);
        self.put(
            name,
            Var {
                value: Some(value),
                export,
            },
        )
    }

    /// Exports `name`, with `value` if one is given, or the value it has.
    /// One already exported keeps its place; another goes after the rest.
    pub fn export(&mut self, name: &str, value: Option<String>) -> Result<(), Error> {
        let old = self.names.get(name);
        let export = match old.and_then(|v| v.export) {
            Some(place) => place,
            None => self.next,
        };
        let value = value.or_else(|| old.and_then(|v| v.value.clone()));
        self.put(
            name,
            Var {
                value,
                export: Some(export),
            },
        )?;
        if export == self.next {
            self.next += 1;
        }
        Ok(())
    }

    /// Removes the variable `name`, exported or not.
    pub fn unset(&mut self, name: &str) {
        if let Some(old) = self.names.remove(name) {
            self.size -= size(name, &old);
        }
    }

    /// Every exported variable, by name: its value, if it has one.
    pub fn exported(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.names
            .iter()
            .filter(|(_, v)| v.export.is_some())
            .map(|(n, v)| (n.as_str(), v.value.as_deref()))
    }

    /// Stores `var` as `name`, unless the variables would then hold more
    /// than `VARS_MAX`.
    fn put(&mut self, name: &str, var: Var) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| size(name, v));
        let new = self.size - old + size(name, &var);
        if new > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = new;
        self.names.insert(String::from(name), var);
        Ok(())
    }
}

/// What a variable counts towards `VARS_MAX`: its name and its value.
fn size(name: &str, var: &Var) -> usize {
    name.len() + var.value.as_ref().map_or(0, String::len)
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        let mut vars = Vars::new("");
        for (n, v) in names {
            vars.set(n, String::from(*v)).unwrap();
        }
        vars.args = args.iter().map(|a| String::from(*a)).collect();
        vars
    }
}

````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 422 tests.

- [ ] **Step 15: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 16: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add export and unset

Each variable now holds a value or none and, once exported, its place
in the order of export, in a module of its own (vars.rs) that still
counts names and values against 64 KiB. export marks names, with a
value or not, lists the exported ones as bash's declare -x quotes them
in the C locale, and names each bad identifier; unset removes
variables, passing over a bad name unless given -v, as bash does. Both
are the shell's own commands, and export's assignments expand as
assignments do (export A=~/x), as bash expands a declaration command's.
-n and -f are not supported. Programs get the exported variables in a
later commit.
EOF
````


### Task 3: A built-in's options judged once all are read

The prototype's review (m-3): `export` and `unset` judged each option word as they read it, so `export -n -x A` said `-n: not supported`, status 1, where bash 5.2, which reads every option first, says `-x: invalid option`, status 2 (and `unset -f -x A` the same). `options` now reads every option word, then names an unknown letter, else one bash knows but this shell does not support. The red run is the shell's tests: the two new cases get `not supported`. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`

**Interfaces:**
- Consumes: Task 2's `options`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            ("export -nx A", 2, "relay-sh: export: -x: invalid option\n"),
        ] {
````

with:

````rust
            ("export -nx A", 2, "relay-sh: export: -x: invalid option\n"),
            // bash reads every option before it judges them.
            (
                "export -n -x A",
                2,
                "relay-sh: export: -x: invalid option\n",
            ),
        ] {
````

Replace:

````rust
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
        ] {
````

with:

````rust
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
            ("unset -f -x A", 2, "relay-sh: unset: -x: invalid option\n"),
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::export::tests::unset_passes_over_a_bad_name_unless_given_v`, `commands::export::tests::export_s_other_options_are_refused`.

- [ ] **Step 3: Change `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// them: options come first, a word `-` or `--` ends them (`--` dropped),
/// and a letter not in `known` is bash's `invalid option` (status 2,
/// without its usage line); one of `refused`, which bash knows, is not
/// supported (status 1).
fn options<'a>(
````

with:

````rust
/// them: options come first, a word `-` or `--` ends them (`--` dropped),
/// and once all are read a letter not in `known` is bash's `invalid
/// option` (status 2, without its usage line), else one of `refused`,
/// which bash knows, is not supported (status 1).
fn options<'a>(
````

Replace:

````rust
        };
        if let Some(bad) = letters.chars().find(|c| !known.contains(*c)) {
            ctx.fail(NAME, format_args!("{name}: -{bad}: invalid option"));
            return Err(2);
        }
        if let Some(no) = letters.chars().find(|c| refused.contains(*c)) {
            return Err(ctx.fail(NAME, format_args!("{name}: -{no}: not supported")));
        }
        seen.push_str(letters);
        first += 1;
    }
````

with:

````rust
        };
        seen.push_str(letters);
        first += 1;
    }
    // Every option is read before any is judged, as bash's are.
    if let Some(bad) = seen.chars().find(|c| !known.contains(*c)) {
        ctx.fail(NAME, format_args!("{name}: -{bad}: invalid option"));
        return Err(2);
    }
    if let Some(no) = seen.chars().find(|c| refused.contains(*c)) {
        return Err(ctx.fail(NAME, format_args!("{name}: -{no}: not supported")));
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 422 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): judge a built-in's options once all are read

export and unset judged each option word as they read it, so export -n
-x said -n was not supported where bash, which reads every option
first, says -x is an invalid option, status 2.

Refs: review m3
EOF
````


### Task 4: `unset -n` is not supported

The prototype's review (m-4): bash 5.2's `unset` takes `-n` (name references), so by decision 4's rule it is an option bash knows and this shell does not support, status 1, not an `invalid option`, status 2. The red run is the shell's tests: `unset -n A` says `invalid option`. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`

**Interfaces:**
- Consumes: Task 2's `unset`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
            ("unset -fv A", 1, "relay-sh: unset: -f: not supported\n"),
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
````

with:

````rust
            ("unset -fv A", 1, "relay-sh: unset: -f: not supported\n"),
            // bash's `-n` (name references) is not supported either.
            ("unset -n A", 1, "relay-sh: unset: -n: not supported\n"),
            ("unset -x A", 2, "relay-sh: unset: -x: invalid option\n"),
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::export::tests::unset_passes_over_a_bad_name_unless_given_v`.

- [ ] **Step 3: Change `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
/// for a function of that name too); with it, it fails (status 1). `-f`
/// (functions) is not supported.
pub fn unset(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (letters, names) = match options(ctx, "unset", args, "fv", "f") {
        Ok(read) => read,
````

with:

````rust
/// for a function of that name too); with it, it fails (status 1). `-f`
/// (functions) and `-n` (name references) are not supported.
pub fn unset(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (letters, names) = match options(ctx, "unset", args, "fnv", "fn") {
        Ok(read) => read,
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 422 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): say unset -n is not supported

bash's unset takes -n, for name references, which this shell has not:
it is an option bash knows, so it is not supported (status 1), as -f
is, rather than an invalid option (status 2).

Refs: review m4
EOF
````


### Task 5: A quoted `export` is `export`

A gap found while checking the prototype's review (m-9): bash takes `"export"` and `\export` for the declaration command too, so `"export" E=~/z` exports `E=/root/z`; Task 2 expanded the assignments only after an unquoted `export`. `Word::is_plain` becomes `Word::is_text`: the word's text once its quotes are removed, with no parameter in it. The red run is the shell's tests: `E` keeps `~/z`. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 2's expansion of `export`'s words.
- Produces: `Word::is_text(text)` in place of `Word::is_plain`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        // `export` (a declaration command) has them; a plain argument
        // keeps its `~` (user-space gate §16 item 11).
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"export A=~/x B=x:~ \"C=~/y\" D=$@\nexport\necho a=~/x\n",
        );
````

with:

````rust
        // `export` (a declaration command) has them; a plain argument
        // keeps its `~` (user-space gate §16 item 11). A quoted `export`
        // is `export` too, as in bash.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"export A=~/x B=x:~ \"C=~/y\" D=$@\n\"export\" E=~/z\nexport\necho a=~/x\n",
        );
````

Replace:

````rust
                 declare -x C=\"~/y\"\n\
                 declare -x D=\"1 2 3\"\n"
            ),
````

with:

````rust
                 declare -x C=\"~/y\"\n\
                 declare -x D=\"1 2 3\"\n\
                 declare -x E=\"/root/z\"\n"
            ),
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::export::tests::export_s_assignments_expand_as_assignments`.

- [ ] **Step 3: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust

    /// A command's words. After an unquoted `export`, a word shaped like an
    /// assignment is one word, `NAME=` and its value expanded as an
    /// assignment's (a `~` after the `=` or a `:`, `$@` joined by blanks).
    fn words(&mut self, words: &[Word]) -> Result<Vec<String>, Error> {
        let export = words.first().is_some_and(|w| w.is_plain("export"));
        let mut out = Vec::new();
````

with:

````rust

    /// A command's words. After `export`, quoted or not, a word shaped like an
    /// assignment is one word, `NAME=` and its value expanded as an
    /// assignment's (a `~` after the `=` or a `:`, `$@` joined by blanks).
    fn words(&mut self, words: &[Word]) -> Result<Vec<String>, Error> {
        let export = words.first().is_some_and(|w| w.is_text("export"));
        let mut out = Vec::new();
````

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust

    /// The word is `text`, unquoted, and nothing else.
    pub fn is_plain(&self, text: &str) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t == text)
    }
````

with:

````rust

    /// The word is `text` once its quotes are removed (`export`,
    /// `"export"`, `\export`), with no parameter in it.
    pub fn is_text(&self, text: &str) -> bool {
        let mut rest = text;
        for piece in &self.pieces {
            let Piece::Text(t, _) = piece else {
                return false;
            };
            let Some(after) = rest.strip_prefix(t.as_str()) else {
                return false;
            };
            rest = after;
        }
        rest.is_empty()
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 422 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): take a quoted export as export

export's assignments expand as assignments only after a plain export;
bash takes "export" or \export for the declaration command too, so
"export" E=~/z exported E=~/z where bash exports E=/root/z. A word is
now export once its quotes are removed.

Refs: review m9
EOF
````


### Task 6: The import: a shell takes its environment as exported variables

Decision 3 (spec §8.5, §10): `Vars::import` takes each entry of a block whose name is a name and that is UTF-8 text, in the block's order, as an exported variable; of two with one name the last wins, as bash's import does (relay-rt's `env::var` keeps the first, as glibc's `getenv`); `1A=x` and `A-B=y`, which bash passes on, are dropped (§10). `Shell::with_environment` imports a block, `/bin/sh` imports its own in all three of its modes, and `named` now changes only `$0`, so an import before it stays; `run_file` keeps the imports when it gives the script its arguments. The tests' `Harness` gains `env`, the block each shell it makes starts with. The red run is the shell's tests: `with_environment` and `import` are missing. Mutation checks (4): any name taken, a lossy reading of bytes that are not UTF-8, the first of two names winning, and `sh FILE` dropping the imports, each fail a test; so does Task 2's `named` resetting the variables.

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `crates/shell/src/vars.rs`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: Task 2's `Vars::export`; plan 2's `relay_rt::env::block()`.
- Produces: `Vars::{import, set_args}`, `Shell::with_environment(block)`, `Harness::env`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_shell_s_name_is_its_argument_0() {
````

with:

````rust
    #[test]
    fn a_shell_imports_its_environment_as_exported_variables() {
        // `/bin/sh` imports the block it was started with (programmable
        // shell gate §8.5), its name given after.
        let mut h = Harness::new();
        let (status, out) = {
            let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
                .with_environment(b"HOME=/root\0A=x  y\0")
                .named("/bin/sh");
            (
                shell.execute("echo $0 $HOME [$A]; export"),
                h.console.take(),
            )
        };
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "/bin/sh /root [x  y]\ndeclare -x A=\"x  y\"\ndeclare -x HOME=\"/root\"\n"
        );
    }

    #[test]
    fn a_script_bin_sh_runs_keeps_the_variables_it_imported() {
        let mut h = spawning();
        h.env = b"HOME=/root\0".to_vec();
        h.put("/tmp/s.sh", b"export\n");
        let mut out = FakeStdout::console();
        let (status, said) = h.sh(&["/tmp/s.sh", "x"], &mut out);
        assert_eq!(status, 0);
        assert_eq!(said, "+ export\ndeclare -x HOME=\"/root\"\n");
    }

    #[test]
    fn a_shell_s_name_is_its_argument_0() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
    pub stdin: Vec<u8>,
}
````

with:

````rust
    pub stdin: Vec<u8>,
    /// The environment each shell starts with (`with_environment`).
    pub env: Vec<u8>,
}
````

Replace:

````rust
            spy: state,
            stdin: Vec::new(),
        }
    }
````

with:

````rust
            spy: state,
            stdin: Vec::new(),
            env: Vec::new(),
        }
    }
````

Replace:

````rust
            stdin: Vec::new(),
        }
````

with:

````rust
            stdin: Vec::new(),
            env: Vec::new(),
        }
````

Replace:

````rust
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system)
            .with_input(&mut input)
````

with:

````rust
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system)
            .with_environment(&self.env)
            .with_input(&mut input)
````

Replace:

````rust
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let mut shell =
            Shell::new(&mut self.vfs, &mut self.console, &mut self.system).with_input(&mut input);
        let mut status = 0;
````

with:

````rust
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let mut shell = Shell::new(&mut self.vfs, &mut self.console, &mut self.system)
            .with_environment(&self.env)
            .with_input(&mut input);
        let mut status = 0;
````

Replace:

````rust
        )
        .execute(line);
````

with:

````rust
        )
        .with_environment(&self.env)
        .execute(line);
````

Replace:

````rust
        )
        .run_file(&args, stdout);
````

with:

````rust
        )
        .with_environment(&self.env)
        .run_file(&args, stdout);
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn unset_removes_a_variable_exported_or_not() {
````

with:

````rust
    #[test]
    fn an_environment_is_imported_as_exported_variables() {
        let mut v = Vars::new("sh");
        v.import(b"HOME=/root\0A=1=2\0C=\xc3\xa9t\xc3\xa9\0E=\0");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [
                ("A", Some("1=2")),
                ("C", Some("été")),
                ("E", Some("")),
                ("HOME", Some("/root"))
            ]
        );
    }

    #[test]
    fn an_entry_without_a_name_or_not_text_is_dropped() {
        // bash passes `1A=x` and `A-B=y` on to its programs (§10); `=z`
        // and an entry without `=` it drops too.
        let mut v = Vars::new("sh");
        v.import(b"1A=x\0A-B=y\0=z\0noeq\0\0B=\xff\0OK=1\0");
        assert_eq!(v.exported().collect::<Vec<_>>(), [("OK", Some("1"))]);
    }

    #[test]
    fn of_two_entries_with_one_name_the_last_wins() {
        // As bash's import (glibc's `getenv` finds the first).
        let mut v = Vars::new("sh");
        v.import(b"A=1\0B=2\0A=3\0");
        assert_eq!((v.get("A"), v.get("B")), ("3", "2"));
    }

    #[test]
    fn unset_removes_a_variable_exported_or_not() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `with_environment` found for struct `Shell<'a>` in the current scope ``; `` no method named `import` found for struct `vars::Vars` in the current scope ``.

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        self.vars.set_name(name);
        self
````

with:

````rust
        self.vars.set_name(name);
        self
    }

    /// The same shell, its environment `block` imported as exported
    /// variables (programmable shell gate §8.5).
    pub fn with_environment(mut self, block: &[u8]) -> Shell<'a> {
        self.vars.import(block);
        self
````

Replace:

````rust
        };
        self.vars = Vars::script(&script.name, &script.args);
        let log = script.transcript_name;
````

with:

````rust
        };
        self.vars.set_args(&script.name, &script.args);
        let log = script.transcript_name;
````

- [ ] **Step 6: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::expand::Error;
use alloc::collections::BTreeMap;
````

with:

````rust
use crate::expand::Error;
use crate::parser::is_name;
use alloc::collections::BTreeMap;
````

Replace:

````rust
        self.args[0] = String::from(name);
    }
````

with:

````rust
        self.args[0] = String::from(name);
    }

    /// `$0` becomes `name`, and `args` come after it (a script's).
    pub fn set_args(&mut self, name: &str, args: &[String]) {
        self.args = alloc::vec![String::from(name)];
        self.args.extend_from_slice(args);
    }

    /// Imports an environment (programmable shell gate §8.5): each entry of
    /// `block`, `NAME=value` and a NUL, whose name is one and that is UTF-8
    /// text, becomes an exported variable, in the block's order; of two
    /// with one name the last wins, as in bash. A block `spawn` took holds
    /// at most 64 KiB, so its variables fit.
    pub fn import(&mut self, block: &[u8]) {
        for entry in block.split(|&b| b == 0) {
            let Ok(entry) = core::str::from_utf8(entry) else {
                continue;
            };
            if let Some((name, value)) = entry.split_once('=')
                && is_name(name)
            {
                let _ = self.export(name, Some(String::from(value)));
            }
        }
    }
````

- [ ] **Step 7: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! tee. Every command but the shell's own is a program. Outside a script
//! `$0` is the shell's argument 0, as bash's is.
#![no_std]
````

with:

````rust
//! tee. Every command but the shell's own is a program. Outside a script
//! `$0` is the shell's argument 0, as bash's is. Every shell imports the
//! environment it was started with as exported variables.
#![no_std]
````

Replace:

````rust
use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, sys};
use shell::Shell;
````

with:

````rust
use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, env, sys};
use shell::Shell;
````

Replace:

````rust
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell =
            Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs).named(&name);
        shell.run_input(&mut SysStdin) as u8
````

with:

````rust
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block())
            .named(&name);
        shell.run_input(&mut SysStdin) as u8
````

Replace:

````rust
        let mut programs = SysPrograms::new(leader);
        let mut shell =
            Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs).named(&name);
        shell.run();
        shell.status() as u8
    } else {
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_file(&words, &mut SysStdout::new()) as u8
````

with:

````rust
        let mut programs = SysPrograms::new(leader);
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block())
            .named(&name);
        shell.run();
        shell.status() as u8
    } else {
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs)
            .with_environment(env::block());
        shell.run_file(&words, &mut SysStdout::new()) as u8
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 427 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates userland
git commit -F - <<'EOF'
feat(shell,sh): import the environment as exported variables

A shell now takes the environment it was started with as exported
variables, in the block's order: an entry whose name is not one, or
that is not UTF-8 text, is dropped (where bash passes it on), and of
two with one name the last wins, as in bash. /bin/sh imports its block
at the prompt, as X | sh and for sh FILE, whose script keeps them. The
tests' harness gives each shell the environment a test sets.
EOF
````


### Task 7: Programs get only the exported variables

Decisions 2 and 3 (spec §8.5): `Programs::spawn` gains the environment, and the shell builds it for every program from its exported variables that have a value, in the order of export (`Vars::environment`): a lone command, each command of a pipeline and a background job alike. `SysPrograms` passes that block to `sys::spawn_env` instead of the program's own, so `/bin/sh` no longer hands its whole environment on (the roadmap's note for plan 3). A script the in-process runner runs imports the same block, as a nested `/bin/sh` does. `FakePrograms` records each child's block. `env_calls` does not change: `/bin/sh` imported `HOME=/root` and passes it on. The red run is the shell's tests: `Spawned::env` and `Vars::environment` are missing. Mutation checks (9): no order, a variable exported again moving, a variable without a value passed, the in-process script importing nothing, either runner path passing none, each fail a unit test; `SysPrograms` passing none, and `/bin/sh` importing nothing at its prompt or as `X | sh`, each fail `env_calls`.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 6's import; plan 2's `relay_rt::sys::spawn_env`.
- Produces: `Programs::spawn(path, args, env, fds, group)`, `Vars::environment()`, `Parts::env`, `Spawned::env`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                fds: [0, 1, 2],
````

with:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                env: Vec::new(),
                fds: [0, 1, 2],
````

Replace:

````rust
            args: words(args),
            fds: [fds.0.unwrap_or(0), fds.1.unwrap_or(1), 2],
````

with:

````rust
            args: words(args),
            env: Vec::new(),
            fds: [fds.0.unwrap_or(0), fds.1.unwrap_or(1), 2],
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
````

with:

````rust
    #[test]
    fn a_script_starts_with_the_exported_variables() {
        // As bash's: a script imports them, and what it sets or exports
        // stays in it.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.put(
            "/tmp/s.sh",
            b"echo [$A][$B][$HOME]\nexport C=in B=changed\n",
        );
        assert_eq!(
            h.lines(&["A=1", "export B=2", "sh /tmp/s.sh", "echo [$B][$C]"]),
            (
                0,
                "+ echo [$A][$B][$HOME]\n[][2][/root]\n+ export C=in B=changed\n[2][]\n".into()
            )
        );
    }

    #[test]
    fn a_program_gets_exactly_the_exported_variables() {
        // In the order of export, the imported ones first; one without a
        // value is none (programmable shell gate §8.5).
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        {
            let mut shell =
                Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                    .with_environment(b"HOME=/root\0X=1\0");
            for line in [
                "A=1",
                "export B=2 NONE",
                "t-args",
                "unset X",
                "t-args | cat",
                "t-args &",
            ] {
                shell.execute(line);
            }
        }
        let envs: Vec<_> = h.programs.spawned.iter().map(|s| s.env.clone()).collect();
        assert_eq!(
            envs,
            [
                b"HOME=/root\0X=1\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
            ],
            "a lone command, each command of a pipeline, a background job"
        );
        // With nothing exported, none.
        h.spawning("t-args");
        assert_eq!(h.programs.spawned.last().unwrap().env, b"");
    }

    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub args: Vec<String>,
    /// The shell's fds it got as its fds 0, 1 and 2.
````

with:

````rust
    pub args: Vec<String>,
    /// Its environment, as `spawn` takes it.
    pub env: Vec<u8>,
    /// The shell's fds it got as its fds 0, 1 and 2.
````

Replace:

````rust
        args: &[&[u8]],
        fds: [u32; 3],
````

with:

````rust
        args: &[&[u8]],
        env: &[u8],
        fds: [u32; 3],
````

Replace:

````rust
                .collect(),
            fds,
````

with:

````rust
                .collect(),
            env: env.to_vec(),
            fds,
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn exported_lists_every_exported_variable_by_name() {
````

with:

````rust
    #[test]
    fn a_program_gets_the_exported_variables_with_a_value_in_export_order() {
        let mut v = Vars::new("sh");
        v.set("Z", String::from("1")).unwrap();
        v.set("NOT", String::from("kept")).unwrap();
        v.export("B", Some(String::from("two words"))).unwrap();
        v.export("Z", None).unwrap();
        v.export("NONE", None).unwrap();
        v.export("A", Some(String::from("été"))).unwrap();
        assert_eq!(
            v.environment(),
            "B=two words\0Z=1\0A=été\0".as_bytes(),
            "in the order of export, not of names"
        );
        // A new value, or exporting it again, keeps a variable's place.
        v.set("B", String::from("2")).unwrap();
        v.export("Z", Some(String::from("3"))).unwrap();
        // Given a value, a variable exported without one goes where it was
        // exported.
        v.set("NONE", String::new()).unwrap();
        assert_eq!(v.environment(), b"B=2\0Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0");
        // Unset and exported again, it goes last.
        v.unset("B");
        v.export("B", Some(String::from("4"))).unwrap();
        assert_eq!(v.environment(), b"Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0B=4\0");
        assert_eq!(v.get("NOT"), "kept");
        assert!(!v.exported().any(|(n, _)| n == "NOT"));
    }

    #[test]
    fn exported_lists_every_exported_variable_by_name() {
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `spawn` has 6 parameters but the declaration in trait `io::Programs::spawn` has 5 ``; `` no method named `environment` found for struct `vars::Vars` in the current scope ``.

- [ ] **Step 6: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        args: &[&[u8]],
        fds: [u32; 3],
````

with:

````rust
        args: &[&[u8]],
        env: &[u8],
        fds: [u32; 3],
````

Replace:

````rust
        let fds = command_fds(fds);
        // This program's own environment, until the shell exports
        // variables (programmable shell gate §15 item 6).
        let env = crate::env::block();
        let pid = sys::spawn_env(path, &arg_bytes(args), env, b"", &fds, flags, pgid)
````

with:

````rust
        let fds = command_fds(fds);
        let pid = sys::spawn_env(path, &arg_bytes(args), env, b"", &fds, flags, pgid)
````

- [ ] **Step 7: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) in
    /// `group`; its pid. Its fds 0, 1 and 2 are the shell's `fds`.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        fds: [u32; 3],
````

with:

````rust
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) and the
    /// environment `env` (entries, each followed by a NUL) in `group`; its
    /// pid. Its fds 0, 1 and 2 are the shell's `fds`.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        env: &[u8],
        fds: [u32; 3],
````

- [ ] **Step 8: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    pub files: &'s mut Files,
}
````

with:

````rust
    pub files: &'s mut Files,
    /// The environment of a program it starts (programmable shell gate
    /// §8.5).
    pub env: &'s [u8],
}
````

Replace:

````rust
            files,
        } = parts;
````

with:

````rust
            files,
            env,
        } = parts;
````

Replace:

````rust
            files,
        };
````

with:

````rust
            files,
            env,
        };
````

Replace:

````rust
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot, None, None));
        match self.programs.spawn(path.as_bytes(), &argv, fds, group) {
            Ok(pid) => match self.programs.wait(pid) {
````

with:

````rust
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot, None, None));
        match self
            .programs
            .spawn(path.as_bytes(), &argv, parts.env, fds, group)
        {
            Ok(pid) => match self.programs.wait(pid) {
````

Replace:

````rust
                .programs
                .spawn(path.as_bytes(), &argv, shell_fds, group);
            match pid {
````

with:

````rust
                .programs
                .spawn(path.as_bytes(), &argv, parts.env, shell_fds, group);
            match pid {
````

- [ ] **Step 9: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
        };
        let parts = Parts {
````

with:

````rust
        };
        let env = self.vars.environment();
        let parts = Parts {
````

Replace:

````rust
            },
            files: &mut self.files,
        };
        let ran = match words.split_first() {
````

with:

````rust
            },
            files: &mut self.files,
            env: &env,
        };
        let ran = match words.split_first() {
````

Replace:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
````

with:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let env = self.vars.environment();
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
````

Replace:

````rust
            },
            files: &mut self.files,
        };
        let ran = self.runner.get().pipeline(parts, &staged);
````

with:

````rust
            },
            files: &mut self.files,
            env: &env,
        };
        let ran = self.runner.get().pipeline(parts, &staged);
````

Replace:

````rust
        let staged = runner_stages(stages, &all);
        let parts = Parts {
````

with:

````rust
        let staged = runner_stages(stages, &all);
        let env = self.vars.environment();
        let parts = Parts {
````

Replace:

````rust
            files: &mut self.files,
        };
````

with:

````rust
            files: &mut self.files,
            env: &env,
        };
````

Replace:

````rust
    /// script is a shell of its own; and it has variables and arguments of
    /// its own, and starts with `$?` 0, as there.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let outer = core::mem::replace(&mut self.vars, Vars::script(&script.name, &script.args));
        self.status = 0;
````

with:

````rust
    /// script is a shell of its own; and it has variables and arguments of
    /// its own, the exported variables imported, and starts with `$?` 0, as
    /// there.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(&self.vars.environment());
        let outer = core::mem::replace(&mut self.vars, vars);
        self.status = 0;
````

- [ ] **Step 10: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
            .map(|(n, v)| (n.as_str(), v.value.as_deref()))
    }
````

with:

````rust
            .map(|(n, v)| (n.as_str(), v.value.as_deref()))
    }

    /// The environment of a program the shell starts: each exported
    /// variable that has a value, `NAME=value` and a NUL, in the order of
    /// export.
    pub fn environment(&self) -> Vec<u8> {
        let mut exported: Vec<(u64, &str, &str)> = self
            .names
            .iter()
            .filter_map(|(n, v)| Some((v.export?, n.as_str(), v.value.as_deref()?)))
            .collect();
        exported.sort_unstable_by_key(|&(place, _, _)| place);
        let mut block = Vec::new();
        for (_, name, value) in exported {
            block.extend_from_slice(name.as_bytes());
            block.push(b'=');
            block.extend_from_slice(value.as_bytes());
            block.push(0);
        }
        block
    }
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 430 tests.

- [ ] **Step 12: Run the `env_calls`, `programs`, `pipes`, `jobs` scenarios**

Run: `cargo xtask test --e2e-only --scenario env_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): give programs only the exported variables

Programs::spawn now takes the environment, and the shell builds it for
every program from its exported variables that have a value, in the
order of export: a lone command, each command of a pipeline and a
background job alike. SysPrograms passes that block to spawn instead of
the program's own, so /bin/sh no longer hands its whole environment on.
A script the in-process runner runs imports the same block, as a
nested /bin/sh does.
EOF
````


### Task 8: The scenario `environment`, and `t-env raw`'s `1 entry`

Decisions 12 and 13 (spec §11.3): the scenario `environment` starts here, and each later pull request adds to it: a variable reaches `t-env` only once exported, in the order of export, one exported without a value not at all; `export` lists them, `unset` removes one, and a script, `X | sh` and a nested `sh` start with the exported ones while nothing they set comes back. `t-env raw` says `1 entry` (plan 2's final review, m-8), and `env_calls` shows it. Tests only: no failing run; the scenario catches Task 7's three `env_calls` mutants too.

**Files:**
- Modify: `tests/e2e/env_calls.txt`
- Create: `tests/e2e/environment.txt`
- Modify: `userland/tests/src/bin/t-env.rs`

**Interfaces:**
- Consumes: Tasks 2–7.
- Produces: the scenario `environment`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/env_calls.txt`**

In `tests/e2e/env_calls.txt`, replace:

````text
expect \n1 entry, 11 bytes\n\[0\] HOME=/root\nroot@relay:~# $
send t-env var HOME; t-env var NOPE
````

with:

````text
expect \n1 entry, 11 bytes\n\[0\] HOME=/root\nroot@relay:~# $
send t-env raw
expect \nan address, 11 bytes, 1 entry\nroot@relay:~# $
send t-env var HOME; t-env var NOPE
````

- [ ] **Step 2: Add the scenario `tests/e2e/environment.txt`**

Create `tests/e2e/environment.txt`:

````text
# The shell's environment (programmable shell gate §8.5, §11.3; milestone
# 5, plan 3): a variable reaches a program only once exported, in the order
# of export, and one exported without a value not at all; export lists
# them as bash's declare -x; unset removes one; a script, X | sh and a
# nested sh start with the exported ones, and nothing comes back out.
timeout 30
expect root@relay:~# $
send A=1; t-env
expect \n1 entry, 11 bytes\n\[0\] HOME=/root\nroot@relay:~# $
send export A B; t-env
expect \n2 entries, 15 bytes\n\[0\] HOME=/root\n\[1\] A=1\nroot@relay:~# $
send export C='two words'; export
expect \ndeclare -x A="1"\ndeclare -x B\ndeclare -x C="two words"\ndeclare -x HOME="/root"\nroot@relay:~# $
send unset A; t-env
expect \n2 entries, 23 bytes\n\[0\] HOME=/root\n\[1\] C=two words\nroot@relay:~# $
send export 1A=x; echo $?
expect \nrelay-sh: export: `1A=x': not a valid identifier\n1\nroot@relay:~# $
send echo 'echo "[$C][$D]"; export D=in; t-env' > /root/env.sh
expect root@relay:~# $
send D=out; sh /root/env.sh
expect \n\+ echo "\[\$C\]\[\$D\]"; export D=in; t-env\n\[two words\]\[\]\n3 entries, 28 bytes\n\[0\] HOME=/root\n\[1\] C=two words\n\[2\] D=in\nroot@relay:~# $
send echo "[$D]"; t-env
expect \n\[out\]\n2 entries, 23 bytes\n\[0\] HOME=/root\n\[1\] C=two words\nroot@relay:~# $
send echo 'echo "[$C]"; t-env' | sh
expect \n\[two words\]\n2 entries, 23 bytes\n\[0\] HOME=/root\n\[1\] C=two words\nroot@relay:~# $
send sh
expect \nroot@relay:~# $
send export E=nested; t-env
expect \n3 entries, 32 bytes\n\[0\] HOME=/root\n\[1\] C=two words\n\[2\] E=nested\nroot@relay:~# $
send exit
expect root@relay:~# $
send echo "[$E]"
expect \n\[\]\nroot@relay:~# $
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-env.rs`**

In `userland/tests/src/bin/t-env.rs`, replace:

````rust
            let at = if at == 0 { "no address" } else { "an address" };
            let _ = writeln!(Fd(1), "{at}, {len} bytes, {count} entries");
            Ok(())
````

with:

````rust
            let at = if at == 0 { "no address" } else { "an address" };
            let _ = writeln!(Fd(1), "{at}, {len} bytes, {count} {}", entries(count));
            Ok(())
````

- [ ] **Step 4: Run the `environment`, `env_calls` scenarios**

Run: `cargo xtask test --e2e-only --scenario environment`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario env_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add tests userland
git commit -F - <<'EOF'
test(userland,e2e): check the exported variables in QEMU

The scenario environment starts here: a variable reaches t-env only
once exported, in the order of export, one exported without a value
not at all; export lists them, unset removes one, and a script, X | sh
and a nested sh start with the exported ones while nothing they set
comes back. t-env raw says "1 entry", and env_calls shows it.

Refs: final review m8
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p3/export
gh pr create --base main --head m5p3/export --title "feat(shell): export variables to programs" --body-file - <<'EOF'
## What

Milestone 5, plan 3, tasks 1–8: the shell's variables keep whether and in what order they are exported (`crates/shell/src/vars.rs`); `export [-p] [NAME[=value]...]` lists as bash 5.2's `declare -x` and `unset [-v] NAME...` removes, both built-ins (eight now), their options read as bash's (`unset -n` not supported), `export`'s assignments expanded as assignments (a quoted `export` too); a shell imports its environment as exported variables (last of two names wins, a bad name or bytes not UTF-8 dropped); `Programs::spawn` takes the environment and every program gets exactly the exported variables that have a value, in export order, where `/bin/sh` handed its whole block on; relay-rt's test double gives each test thread its own environment (plan 2's final review, m-8); the scenario `environment` starts, and `t-env raw` says `1 entry`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests
- [x] New scenario `environment`

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-export
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: `A=1 cmd` (Tasks 9–13)

Assignments before a command go into that command's environment: before a program over the exported variables, before a built-in held while it runs by bash's rule; a program's environment over 64 KiB is refused.

Branch `m5p3/assign`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-assign`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p3/assign /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-assign origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-assign
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p3/export` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p3/assign /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-assign m5p3/export`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p3/export>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 9: A command's assignments apart from its words

A refactor for decision 5: the parser gives a command's leading assignments as `Command::assigns` and its name and arguments as `words`, so the next task can run `A=1 cmd`. A line of assignments has `assigns` and no `words`; `A=1 cmd` is still refused. No behaviour changes: no failing run, no guard.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: the parser's `command`.
- Produces: `Command::assigns`.

- [ ] **Step 1: Change the tests in `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
            let p = typed(line);
            let (_, value) = p[0].words[0].assignment().unwrap();
            super::value(&value, &v, 0)
````

with:

````rust
            let p = typed(line);
            let (_, value) = p[0].assigns[0].assignment().unwrap();
            super::value(&value, &v, 0)
````

- [ ] **Step 2: Change the tests in `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let c = typed(word);
        let (name, value) = c[0].words[0].assignment()?;
        let text = value
````

with:

````rust
        let c = typed(word);
        let (name, value) = c[0].assigns.first().or(c[0].words.first())?.assignment()?;
        let text = value
````

Replace:

````rust
            [Command {
                words: Vec::new(),
````

with:

````rust
            [Command {
                assigns: Vec::new(),
                words: Vec::new(),
````

- [ ] **Step 3: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
        let words = self.words(&c.words)?;
        let mut redirects = Vec::new();
        for r in &c.redirects {
            redirects.push(self.redirect(r)?);
        }
        Ok(Command { words, redirects })
    }
````

with:

````rust
        let words = self.words(&c.words)?;
        let mut assigns = Vec::new();
        for w in &c.assigns {
            assigns.extend(self.word(w)?);
        }
        let mut redirects = Vec::new();
        for r in &c.redirects {
            redirects.push(self.redirect(r)?);
        }
        Ok(Command {
            assigns,
            words,
            redirects,
        })
    }
````

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// One command: its words and its redirections, in the order typed. The
/// parser gives them as typed ([`Word`]), and expansion as the strings a
/// command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<W>,
````

with:

````rust

/// One command: its assignments, its words and its redirections, in the
/// order typed. The parser gives them as typed ([`Word`]), and expansion
/// as the strings a command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The assignments before the command's name, `NAME=value` each.
    pub assigns: Vec<W>,
    /// The command name first, then its arguments. Empty for a blank line
    /// and for a line of assignments.
    pub words: Vec<W>,
````

Replace:

````rust
/// appends.
fn command(words: Vec<Word>, redirects: Vec<Redirect<Word>>) -> Result<Command<Word>, ParseError> {
    let mut leading = words
````

with:

````rust
/// appends.
fn command(
    mut words: Vec<Word>,
    redirects: Vec<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
    let mut leading = words
````

Replace:

````rust
    }
    if let Some(first) = words.first()
        && first.assignment().is_some()
        && words.iter().any(|w| w.assignment().is_none())
    {
        return Err(ParseError::Unsupported(format!(
            "{} before a command",
            first.typed
        )));
    }
    Ok(Command { words, redirects })
}
````

with:

````rust
    }
    let count = words
        .iter()
        .take_while(|w| w.assignment().is_some())
        .count();
    if count > 0 && count < words.len() {
        return Err(ParseError::Unsupported(format!(
            "{} before a command",
            words[0].typed
        )));
    }
    let assigns = words.drain(..count).collect();
    Ok(Command {
        assigns,
        words,
        redirects,
    })
}
````

Replace:

````rust
        return Ok(alloc::vec![Command {
            words: Vec::new(),
````

with:

````rust
        return Ok(alloc::vec![Command {
            assigns: Vec::new(),
            words: Vec::new(),
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        self.collect_jobs();
        let assigns = commands
            .iter()
            .find_map(|c| c.words.first().filter(|w| w.assignment().is_some()));
        if let Some(first) = assigns {
````

with:

````rust
        self.collect_jobs();
        let assigns = commands.iter().find_map(|c| c.assigns.first());
        if let Some(first) = assigns {
````

Replace:

````rust
    fn assign(&mut self, cmd: &parser::Command<parser::Word>) -> i32 {
        for (name, value) in cmd.words.iter().filter_map(parser::Word::assignment) {
            let set =
````

with:

````rust
    fn assign(&mut self, cmd: &parser::Command<parser::Word>) -> i32 {
        for (name, value) in cmd.assigns.iter().filter_map(parser::Word::assignment) {
            let set =
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 430 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell): keep a command's assignments apart from its words

The parser now gives a command's leading assignments as assigns, and
its name and arguments as words, so that a later commit can run
A=1 cmd. A line of assignments has assigns and no words; A=1 cmd is
still refused.
EOF
````


### Task 10: `A=1 cmd`: assignments before a program

Decision 5 (spec §8.5): `A=1 cmd` is no longer refused. As bash 5.2 expands them, the command's words come first, then its assignments left to right, each reading the ones before through the `Expander`'s overlay (`A=1 B=$A env` gives `B=1`, `A=1 echo $A` the old `A`, a bad substitution in the words told first), all within the line's 64 KiB (`expand::command_parts`). A program gets the exported variables with its assignments over them (`Vars::environment_with`): an exported name in its place, the others in the order typed, the last value of a name assigned twice; each command of a pipeline and a background job its own (`Stage::env`); a script the in-process runner runs imports them. Before words that expand to nothing the assignments stay set, as bash's `A=1 $E` does. Before a built-in they are refused until the next task. `help` says how to export and assign for one command. The red run is the shell's tests: `command_parts` and `environment_with` are missing. Mutation checks (10): the assignments expanded before the words (its test came from the surviving mutant), no overlay, the overlay kept across commands, its first value winning, the shell's own value over the assigned one, an exported name twice, the first assigned value winning, nothing kept before no words, a stage's or a lone command's assignments not passed, each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 9's `Command::assigns`; Task 7's environment.
- Produces: `expand::command_parts(command, vars, status) -> (words, assigns)` (in place of `command_words`), `Vars::environment_with(assigns)` (in place of `environment`), `runner::Stage::env`; `run_script` takes the command's environment.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
            "NAME=value",
            "`;`",
````

with:

````rust
            "NAME=value",
            "export NAME",
            "NAME=value cmd",
            "`;`",
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
    #[test]
    fn a_redirection_target_must_expand_to_one_word() {
````

with:

````rust
    #[test]
    fn a_command_s_words_expand_before_its_assignments() {
        // As bash's: `A=1 echo $A` prints the old `A`, `A=1 B=$A env`
        // gives `B=1`, and a bad substitution in the words is told first.
        let v = script();
        let parts = |line: &str| expand(&typed(line), &v, 0).map(|mut c| c.remove(0));
        let c = parts("A=1 echo $A $E").unwrap();
        assert_eq!(c.words, ["echo", "a  b"], "the words read the old values");
        assert_eq!(c.assigns, ["A=1"]);
        let c = parts("A=1 B=$A A=2 C=$A$1 env").unwrap();
        assert_eq!(c.assigns, ["A=1", "B=1", "A=2", "C=2one"]);
        let lone = command_parts(&typed("A=${1A} echo ${2B}")[0], &v, 0);
        assert_eq!(lone, Err(Error::BadSubstitution("${2B}".into())));
        assert_eq!(
            parts("A=${1A} echo ${2B}").unwrap_err(),
            Error::BadSubstitution("${2B}".into())
        );
        // Each command of a pipeline reads only its own.
        let p = expand(&typed("A=1 x | echo $A"), &v, 0).unwrap();
        assert_eq!(p[1].words, ["echo", "a  b"]);
        // An assignment's value as an assignment's: `~` and `$@`.
        let c = parts("A=~/x B=$@ cmd").unwrap();
        assert_eq!(c.assigns, ["A=/root/x", "B=one two three  four"]);
    }

    #[test]
    fn a_command_s_assignments_share_its_room() {
        let half = "x".repeat(EXPANSION_MAX / 2);
        let v = Vars::of(&[("A", &half)], &["s.sh"]);
        let c = typed("B=$A cmd $A");
        assert_eq!(command_parts(&c[0], &v, 0), Err(Error::TooLong));
        assert!(command_parts(&typed("B=$A cmd")[0], &v, 0).is_ok());
    }

    #[test]
    fn a_redirection_target_must_expand_to_one_word() {
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn an_assignment_before_a_command_is_unsupported() {
        // bash gives the command an environment, which programs have not.
        for (line, what) in [
            ("A=1 echo hi", "A=1 before a command"),
            ("A='a b' B=2 cat f", "A='a b' before a command"),
            ("ls | A=1 wc", "A=1 before a command"),
            ("A=1 echo &", "A=1 before a command"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        // An argument that looks like one is one; assignments alone parse.
        assert_eq!(words("echo A=1"), ["echo", "A=1"]);
        assert!(parse_line("A=1 B=2 > f").is_ok());
    }
````

with:

````rust
    #[test]
    fn assignments_before_a_command_are_its_own() {
        // bash gives them to the command's environment (§8.5).
        let c = &typed("A=1 B='a b' cat f")[0];
        let as_typed: Vec<_> = c.assigns.iter().map(|w| w.typed.as_str()).collect();
        assert_eq!(as_typed, ["A=1", "B='a b'"]);
        assert_eq!(c.words.len(), 2);
        for line in ["ls | A=1 wc", "A=1 echo &", "A=1 > f cat"] {
            assert!(parse_line(line).is_ok(), "{line}");
        }
        // An argument that looks like one is one; assignments alone have
        // no words.
        assert_eq!(words("echo A=1"), ["echo", "A=1"]);
        let alone = &typed("A=1 B=2 > f")[0];
        assert_eq!((alone.assigns.len(), alone.words.len()), (2, 0));
    }
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
````

with:

````rust
    #[test]
    fn assignments_before_a_program_are_in_its_environment_only() {
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        {
            let mut shell =
                Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                    .with_environment(b"HOME=/root\0B=2\0");
            for line in [
                "B=3 A=1 t-args",
                "t-args",
                "A=1 t-args | X=2 cat",
                "Y=$B t-args &",
                "B=4 t-args $B",
            ] {
                shell.execute(line);
            }
        }
        let envs: Vec<_> = h.programs.spawned.iter().map(|s| s.env.clone()).collect();
        assert_eq!(
            envs,
            [
                b"HOME=/root\0B=3\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0X=2\0".to_vec(),
                b"HOME=/root\0B=2\0Y=2\0".to_vec(),
                b"HOME=/root\0B=4\0".to_vec(),
            ]
        );
        assert_eq!(
            h.programs.spawned[5].args,
            ["t-args", "2"],
            "the words read the old B"
        );
    }

    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo [$A][$B]\n");
        assert_eq!(
            h.lines(&["B=2", "A=1 sh /tmp/s.sh", "echo [$A]"]),
            (0, "+ echo [$A][$B]\n[1][]\n[]\n".into())
        );
    }

    #[test]
    fn assignments_before_words_that_expand_to_nothing_stay_set() {
        // As bash's `A=1 $E`: a line of assignments after all.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["A=1 $E", "B=2 $E > /tmp/f", "echo $A $B"]),
            (0, "1 2\n".into())
        );
        assert!(h.exists("/tmp/f"));
    }

    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
````

- [ ] **Step 5: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(
            v.environment(),
            "B=two words\0Z=1\0A=été\0".as_bytes(),
````

with:

````rust
        assert_eq!(
            v.environment_with(&[]),
            "B=two words\0Z=1\0A=été\0".as_bytes(),
````

Replace:

````rust
        v.set("NONE", String::new()).unwrap();
        assert_eq!(v.environment(), b"B=2\0Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0");
        // Unset and exported again, it goes last.
        v.unset("B");
        v.export("B", Some(String::from("4"))).unwrap();
        assert_eq!(v.environment(), b"Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0B=4\0");
        assert_eq!(v.get("NOT"), "kept");
        assert!(!v.exported().any(|(n, _)| n == "NOT"));
    }
````

with:

````rust
        v.set("NONE", String::new()).unwrap();
        assert_eq!(
            v.environment_with(&[]),
            b"B=2\0Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0"
        );
        // Unset and exported again, it goes last.
        v.unset("B");
        v.export("B", Some(String::from("4"))).unwrap();
        assert_eq!(
            v.environment_with(&[]),
            b"Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0B=4\0"
        );
        assert_eq!(v.get("NOT"), "kept");
        assert!(!v.exported().any(|(n, _)| n == "NOT"));
    }

    #[test]
    fn assignments_before_a_program_go_over_the_exported_variables() {
        let mut v = Vars::new("sh");
        v.export("HOME", Some(String::from("/root"))).unwrap();
        v.export("B", None).unwrap();
        v.export("X", Some(String::from("1"))).unwrap();
        v.set("L", String::from("local")).unwrap();
        let assigns = ["X=2", "N=new", "B=b", "L=l=m", "N=last"].map(String::from);
        assert_eq!(
            v.environment_with(&assigns),
            b"HOME=/root\0B=b\0X=2\0N=last\0L=l=m\0",
            "exported names in their places, the others as typed, the last value"
        );
        // The shell's own are as they were.
        assert_eq!((v.get("X"), v.value("B"), v.get("L")), ("1", None, "local"));
    }
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `command_parts` in this scope ``; `` no method named `environment_with` found for struct `vars::Vars` in the current scope ``.

- [ ] **Step 7: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
         round. `sh FILE ARG...` runs a script, which reads its arguments as\n\
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
         `NAME=value` sets `$NAME`. `if a; then b; fi` runs b if a succeeds,\n\
         with `elif c; then d;` and `else e;` before `fi` for other cases;\n\
         `while a; do b; done` repeats b while a succeeds, `until` while it\n\
         fails; `for x in w...; do b; done` runs b with each w as `$x`, and\n\
         `for x; do` with each argument. Each may go on across lines, at `> `."
    );
````

with:

````rust
         round. `sh FILE ARG...` runs a script, which reads its arguments as\n\
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status.\n\
         `NAME=value` sets `$NAME`, `export NAME` gives it to the programs the\n\
         shell starts, and `NAME=value cmd` gives it to cmd alone.\n\
         `if a; then b; fi` runs b if a succeeds, with `elif c; then d;` and\n\
         `else e;` before `fi` for other cases; `while a; do b; done` repeats b\n\
         while a succeeds, `until` while it fails; `for x in w...; do b; done`\n\
         runs b with each w as `$x`, and `for x; do` with each argument. Each\n\
         may go on across lines, at `> `."
    );
````

- [ ] **Step 8: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

/// A command's `words` expanded: `export`'s assignments as assignments
/// are (`export A=~/x`), as bash expands a declaration command's.
pub(crate) fn command_words(
    words: &[Word],
    vars: &Vars,
    status: i32,
) -> Result<Vec<String>, Error> {
    Expander::new(vars, status).words(words)
}
````

with:

````rust

/// A command's words and then its assignments expanded, as bash expands
/// them, sharing the line's room: `export`'s assignments are expanded as
/// assignments are (`export A=~/x`), as bash expands a declaration
/// command's. Its redirections are expanded as they are reached.
pub(crate) fn command_parts(
    c: &Command<Word>,
    vars: &Vars,
    status: i32,
) -> Result<(Vec<String>, Vec<String>), Error> {
    let mut x = Expander::new(vars, status);
    let words = x.words(&c.words)?;
    let assigns = x.assigns(&c.assigns)?;
    Ok((words, assigns))
}
````

Replace:

````rust
    vars: &'v Vars,
    status: i32,
````

with:

````rust
    vars: &'v Vars,
    /// The assignments of the command being expanded, made so far: a
    /// later one reads an earlier one (`A=1 B=$A cmd`).
    made: Vec<(String, String)>,
    status: i32,
````

Replace:

````rust
            vars,
            status,
````

with:

````rust
            vars,
            made: Vec::new(),
            status,
````

Replace:

````rust
        let words = self.words(&c.words)?;
        let mut assigns = Vec::new();
        for w in &c.assigns {
            assigns.extend(self.word(w)?);
        }
        let mut redirects = Vec::new();
````

with:

````rust
        let words = self.words(&c.words)?;
        let assigns = self.assigns(&c.assigns)?;
        let mut redirects = Vec::new();
````

Replace:

````rust
        }
        Ok(out)
````

with:

````rust
        }
        Ok(out)
    }

    /// A command's assignments, `NAME=value` each, expanded after its words
    /// as bash expands them: left to right, each reading the ones before
    /// (programmable shell gate §15 item 7).
    fn assigns(&mut self, assigns: &[Word]) -> Result<Vec<String>, Error> {
        let mut out = Vec::new();
        for (name, value) in assigns.iter().filter_map(Word::assignment) {
            let value = self.joined(&value)?;
            out.push(alloc::format!("{name}={value}"));
            match self.made.iter_mut().find(|(n, _)| n == name) {
                Some(made) => made.1 = value,
                None => self.made.push((String::from(name), value)),
            }
        }
        self.made.clear();
        Ok(out)
````

Replace:

````rust
        Ok(match p {
            Param::Name(n) => Value::One(Cow::Borrowed(self.vars.get(n))),
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
````

with:

````rust
        Ok(match p {
            Param::Name(n) => match self.made.iter().find(|(m, _)| m == n) {
                Some((_, value)) => Value::One(Cow::Owned(value.clone())),
                None => Value::One(Cow::Borrowed(self.vars.get(n))),
            },
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
````

- [ ] **Step 9: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// A command of `words` and `redirects`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(
````

with:

````rust

/// A command of `words` and `redirects`, the assignments before its name
/// (`A=1 cmd`) apart. bash's `NAME+=value`, which appends, is not
/// supported.
fn command(
````

Replace:

````rust
        .count();
    if count > 0 && count < words.len() {
        return Err(ParseError::Unsupported(format!(
            "{} before a command",
            words[0].typed
        )));
    }
    let assigns = words.drain(..count).collect();
````

with:

````rust
        .count();
    let assigns = words.drain(..count).collect();
````

- [ ] **Step 10: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    pub fds: Option<Fds>,
}
````

with:

````rust
    pub fds: Option<Fds>,
    /// Its environment: the exported variables with its assignments.
    pub env: &'c [u8],
}
````

Replace:

````rust
            files,
            env,
        } = parts;
````

with:

````rust
            files,
            env: _,
        } = parts;
````

Replace:

````rust
            files,
            env,
        };
````

with:

````rust
            files,
            env: last.env,
        };
````

Replace:

````rust
                .programs
                .spawn(path.as_bytes(), &argv, parts.env, shell_fds, group);
            match pid {
````

with:

````rust
                .programs
                .spawn(path.as_bytes(), &argv, stage.env, shell_fds, group);
            match pid {
````

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
        self.collect_jobs();
        let assigns = commands.iter().find_map(|c| c.assigns.first());
        if let Some(first) = assigns {
            // Alone on its line; bash's changes nothing elsewhere.
````

with:

````rust
        self.collect_jobs();
        let alone = commands
            .iter()
            .filter(|c| c.words.is_empty())
            .find_map(|c| c.assigns.first());
        if let Some(first) = alone {
            // Alone on its line; bash's changes nothing elsewhere.
````

Replace:

````rust
        let typed = &commands[0];
        let words = match expand::command_words(&typed.words, &self.vars, self.status) {
            Ok(words) => words,
            Err(e) => return self.not_expanded(e, true),
        };
        if words.is_empty() && typed.redirects.is_empty() {
````

with:

````rust
        let typed = &commands[0];
        let (words, assigns) = match expand::command_parts(typed, &self.vars, self.status) {
            Ok(parts) => parts,
            Err(e) => return self.not_expanded(e, true),
        };
        if words.is_empty() {
            // Before words that expanded to nothing the assignments stay
            // set, as a line of assignments does in bash.
            for (name, value) in assigns.iter().filter_map(|a| a.split_once('=')) {
                if let Err(e) = self.vars.set(name, String::from(value)) {
                    return self.not_expanded(e, true);
                }
            }
        } else if !assigns.is_empty() && commands::builtin(&words[0]).is_some() {
            let first = &typed.assigns[0].typed;
            let message = format!("{NAME}: unsupported syntax: {first} before a built-in\n");
            return self.finish(SYNTAX, message);
        }
        if words.is_empty() && typed.redirects.is_empty() {
````

Replace:

````rust
        };
        let env = self.vars.environment();
        let parts = Parts {
````

with:

````rust
        };
        let env = self.vars.environment_with(&assigns);
        let parts = Parts {
````

Replace:

````rust
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script);
            self.fds = outer;
````

with:

````rust
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script, &env);
            self.fds = outer;
````

Replace:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let env = self.vars.environment();
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
````

with:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let envs = self.stage_environments(stages);
        let staged = runner_stages(stages, &all, &envs);
        let env = Vec::new();
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
````

Replace:

````rust
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let env = self.vars.environment();
        let parts = Parts {
````

with:

````rust
        let all = self.stage_fds(stages);
        let envs = self.stage_environments(stages);
        let staged = runner_stages(stages, &all, &envs);
        let env = Vec::new();
        let parts = Parts {
````

Replace:

````rust

    /// Collects the background jobs' processes that have ended.
````

with:

````rust

    /// Each stage's environment: the exported variables with its own
    /// assignments (programmable shell gate §8.5).
    fn stage_environments(&self, stages: &[parser::Command]) -> Vec<Vec<u8>> {
        stages
            .iter()
            .map(|c| self.vars.environment_with(&c.assigns))
            .collect()
    }

    /// Collects the background jobs' processes that have ended.
````

Replace:

````rust
    /// there.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(&self.vars.environment());
        let outer = core::mem::replace(&mut self.vars, vars);
````

with:

````rust
    /// there.
    fn run_script(&mut self, script: Script, env: &[u8]) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(env);
        let outer = core::mem::replace(&mut self.vars, vars);
````

Replace:

````rust
/// A pipeline's stages for the runner: each one's words and fds.
fn runner_stages<'c>(stages: &'c [parser::Command], fds: &[Option<Fds>]) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .map(|(c, fds)| runner::Stage {
            words: &c.words,
            fds: *fds,
        })
````

with:

````rust
/// A pipeline's stages for the runner: each one's words and fds.
fn runner_stages<'c>(
    stages: &'c [parser::Command],
    fds: &[Option<Fds>],
    envs: &'c [Vec<u8>],
) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .zip(envs)
        .map(|((c, fds), env)| runner::Stage {
            words: &c.words,
            fds: *fds,
            env,
        })
````

- [ ] **Step 12: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// The environment of a program the shell starts: each exported
    /// variable that has a value, `NAME=value` and a NUL, in the order of
    /// export.
    pub fn environment(&self) -> Vec<u8> {
        let mut exported: Vec<(u64, &str, &str)> = self
            .names
            .iter()
            .filter_map(|(n, v)| Some((v.export?, n.as_str(), v.value.as_deref()?)))
            .collect();
        exported.sort_unstable_by_key(|&(place, _, _)| place);
        let mut block = Vec::new();
        for (_, name, value) in exported {
            block.extend_from_slice(name.as_bytes());
````

with:

````rust

    /// The environment of a program the shell starts (programmable shell
    /// gate §8.5): each exported variable that has a value, `NAME=value` and
    /// a NUL, in the order of export; with `assigns` before it (`A=1 cmd`,
    /// `NAME=value` each), an exported name keeps its place with the value
    /// assigned, and the others follow in the order typed, of a name
    /// assigned twice the last value.
    pub fn environment_with(&self, assigns: &[String]) -> Vec<u8> {
        let mut over: Vec<(&str, &str)> = Vec::new();
        for a in assigns {
            let Some((name, value)) = a.split_once('=') else {
                continue;
            };
            match over.iter_mut().find(|(n, _)| *n == name) {
                Some(o) => o.1 = value,
                None => over.push((name, value)),
            }
        }
        let assigned = |name: &str| over.iter().find(|(n, _)| *n == name).map(|o| o.1);
        let mut exported: Vec<(u64, &str, &str)> = self
            .names
            .iter()
            .filter_map(|(n, v)| {
                let value = assigned(n).or(v.value.as_deref())?;
                Some((v.export?, n.as_str(), value))
            })
            .collect();
        exported.sort_unstable_by_key(|&(place, _, _)| place);
        let rest = over.iter().filter(|(n, _)| !self.is_exported(n));
        let mut block = Vec::new();
        for (name, value) in exported
            .iter()
            .map(|&(_, n, v)| (n, v))
            .chain(rest.copied())
        {
            block.extend_from_slice(name.as_bytes());
````

Replace:

````rust
        block
    }
````

with:

````rust
        block
    }

    /// Whether `name` is exported.
    fn is_exported(&self, name: &str) -> bool {
        self.names.get(name).is_some_and(|v| v.export.is_some())
    }
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 436 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): give a program the assignments typed before it

A=1 cmd is no longer refused: the command's words are expanded first,
then its assignments left to right, each reading the ones before, all
within the line's 64 KiB, as bash expands them. A program, any command
of a pipeline and a background job get the exported variables with
their own assignments over them (an exported name in its place, the
others in the order typed); a script the in-process runner runs gets
them as variables. Before words that expand to nothing they stay set,
as bash's do. Before a built-in they are refused until the next
commit. help says how to export and assign for one command.
EOF
````


### Task 11: `A=1 cmd` before a built-in: held while it runs

Decision 5 (the maintainer, 2026-10-03): before a built-in the assignments are set while it runs (`Vars::hold`), then each name comes back as it was unless the built-in set or exported it (`Vars::release`), as bash 5.2's do: `A=1 export A` keeps `A=1` exported, `C=0; C=1 unset C` brings back `C=0`, `W=1 export W=2` keeps 2, `G=1 cd /tmp > f` drops `G`, and `export -p` lists the values from before (`Z=1 export -p` shows the old `Z`, probed). One that does not fit the variables runs nothing and puts back those before it. The red run is the shell's tests: `hold` and `release` are missing. Mutation checks (11): never putting back, putting back what the built-in set, a set or an export not noted, the flags left as the hold set them, no release after a failed hold, listing the held values, keeping each name's first earlier state, no hold, no release, each fail a test; listing a held name the built-in changed proved unobservable and was dropped, and the walk that hid the one-entry-per-name rule now goes forward (`C=1 C=2 cd /`). Writing the tests found a bug before it was committed: a failed hold kept the names set so far.

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 10's assignments.
- Produces: `Vars::{hold, release}`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
````

with:

````rust
    #[test]
    fn assignments_before_a_built_in_hold_while_it_runs() {
        // What bash 5.2 prints for each (programmable shell gate §15 item
        // 7): a name comes back as it was unless the built-in set or
        // exported it, and `export -p` lists the values from before.
        let mut h = Harness::new();
        let (status, out) = h.lines(&[
            "A=1 export A",
            "C=0",
            "C=1 unset C",
            "B=1 unset B",
            "W=1 export W=2",
            "G=1 cd /tmp > /tmp/o",
            "export Z=0",
            "X=1 Z=1 export",
            "C=1 C=2 cd /",
            "echo [$A][$B][$C][$W][$G][$Z][$X]",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "declare -x A=\"1\"\ndeclare -x W=\"2\"\ndeclare -x Z=\"0\"\n[1][][0][2][][0][]\n"
        );
    }

    #[test]
    fn an_assignment_a_built_in_cannot_hold_runs_nothing() {
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 10);
        let (status, out) = h.lines(&[
            &format!("A={big}"),
            &format!("B={} C=1 cd /tmp", &big[..20]),
            "pwd",
            "echo [$C]",
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            out,
            "relay-sh: B: the variables would hold more than 64 KiB\n/\n[]\n"
        );
    }

    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn exported_lists_every_exported_variable_by_name() {
````

with:

````rust
    #[test]
    fn assignments_held_for_a_built_in_come_back_unless_it_set_them() {
        let mut v = Vars::new("sh");
        v.set("C", String::from("0")).unwrap();
        v.export("H", Some(String::from("/root"))).unwrap();
        v.hold(&["H=/d", "C=1", "N=new", "S=x", "E=y"].map(String::from))
            .unwrap();
        assert_eq!((v.get("H"), v.get("C"), v.get("N")), ("/d", "1", "new"));
        // Listed as they were before.
        assert_eq!(v.exported().collect::<Vec<_>>(), [("H", Some("/root"))]);
        // What the built-in does: `unset C`, `S=set`, `export E`.
        v.unset("C");
        v.set("S", String::from("set")).unwrap();
        v.export("E", None).unwrap();
        v.release();
        assert_eq!(
            (v.value("H"), v.value("C"), v.value("N")),
            (Some("/root"), Some("0"), None),
            "put back"
        );
        assert_eq!((v.get("S"), v.get("E")), ("set", "y"), "kept");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [("E", Some("y")), ("H", Some("/root"))]
        );
        assert_eq!(v.size, "C0H/rootSsetEy".len());
    }

    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
        let mut v = Vars::new("sh");
        v.set("A", String::from("old")).unwrap();
        let big = alloc::format!("B={}", "x".repeat(VARS_MAX));
        assert_eq!(
            v.hold(&[String::from("A=new"), big]),
            Err(Error::Full("B".into()))
        );
        assert_eq!((v.get("A"), v.value("B")), ("old", None));
        assert_eq!(v.size, 4);
        // Nothing is held any more.
        v.set("A", String::from("x")).unwrap();
        v.release();
        assert_eq!(v.get("A"), "x");
    }

    #[test]
    fn exported_lists_every_exported_variable_by_name() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `hold` found for struct `vars::Vars` in the current scope ``; `` no method named `release` found for struct `vars::Vars` in the current scope ``.

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
            }
        } else if !assigns.is_empty() && commands::builtin(&words[0]).is_some() {
            let first = &typed.assigns[0].typed;
            let message = format!("{NAME}: unsupported syntax: {first} before a built-in\n");
            return self.finish(SYNTAX, message);
        }
````

with:

````rust
            }
        }
````

Replace:

````rust
        let env = self.vars.environment_with(&assigns);
        let parts = Parts {
````

with:

````rust
        let env = self.vars.environment_with(&assigns);
        // Before a built-in the assignments are set while it runs.
        let builtin = words.first().and_then(|name| commands::builtin(name));
        if builtin.is_some()
            && let Err(e) = self.vars.hold(&assigns)
        {
            self.release(fds);
            return self.not_expanded(e, true);
        }
        let parts = Parts {
````

Replace:

````rust
        let ran = match words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
                Some(builtin) => {
````

with:

````rust
        let ran = match words.split_first() {
            Some((name, args)) => match builtin {
                Some(builtin) => {
````

Replace:

````rust
        };
        let mut message = ran.message;
````

with:

````rust
        };
        self.vars.release();
        let mut message = ran.message;
````

- [ ] **Step 5: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    next: u64,
}
````

with:

````rust
    next: u64,
    /// While a built-in runs with assignments before it: each assigned
    /// name with what it was before, and whether the built-in has set or
    /// exported it since.
    held: Vec<Held>,
}

/// An assignment before a built-in, held while it runs.
struct Held {
    name: String,
    before: Option<Var>,
    changed: bool,
}
````

Replace:

````rust
            next: 0,
        }
````

with:

````rust
            next: 0,
            held: Vec::new(),
        }
````

Replace:

````rust
            },
        )
    }
````

with:

````rust
            },
        )?;
        self.changed(name);
        Ok(())
    }
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
        self.changed(name);
        Ok(())
    }

    /// Sets the assignments before a built-in, `NAME=value` each, while it
    /// runs (programmable shell gate §15 item 7); [`Vars::release`] puts
    /// back what they replaced. One that does not fit puts back those
    /// before it.
    pub fn hold(&mut self, assigns: &[String]) -> Result<(), Error> {
        let mut set = Ok(());
        for (name, value) in assigns.iter().filter_map(|a| a.split_once('=')) {
            if !self.held.iter().any(|h| h.name == name) {
                let before = self.names.get(name).cloned();
                self.held.push(Held {
                    name: String::from(name),
                    before,
                    changed: false,
                });
            }
            set = self.set(name, String::from(value));
            if set.is_err() {
                break;
            }
        }
        // Setting them was not the built-in's doing.
        for h in &mut self.held {
            h.changed = false;
        }
        if set.is_err() {
            self.release();
        }
        set
    }

    /// The built-in has ended: each name it held is as it was before,
    /// unless the built-in set or exported it, as bash's are (`A=1 export
    /// A` keeps `A=1`, `C=1 unset C` brings back the old `C`).
    pub fn release(&mut self) {
        for h in core::mem::take(&mut self.held) {
            if h.changed {
                continue;
            }
            if let Some(old) = self.names.remove(&h.name) {
                self.size -= size(&h.name, &old);
            }
            if let Some(before) = h.before {
                // It fitted before, so it is put back whatever the
                // built-in added.
                self.size += size(&h.name, &before);
                self.names.insert(h.name, before);
            }
        }
    }

    /// Notes that `name` was set or exported, for a built-in holding it.
    fn changed(&mut self, name: &str) {
        if let Some(h) = self.held.iter_mut().find(|h| h.name == name) {
            h.changed = true;
        }
    }
````

Replace:

````rust

    /// Every exported variable, by name: its value, if it has one.
    pub fn exported(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.names
            .iter()
            .filter(|(_, v)| v.export.is_some())
            .map(|(n, v)| (n.as_str(), v.value.as_deref()))
    }
````

with:

````rust

    /// Every exported variable, by name: its value, if it has one. A name
    /// a built-in holds is as it was before, as bash's `export -p` lists it.
    pub fn exported(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.names.iter().filter_map(|(n, v)| {
            let v = match self.held.iter().find(|h| h.name == *n) {
                Some(h) => h.before.as_ref()?,
                None => v,
            };
            v.export?;
            Some((n.as_str(), v.value.as_deref()))
        })
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 440 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): hold assignments before a built-in while it runs

A=1 before a built-in sets A while it runs; then each name comes back
as it was, unless the built-in set or exported it, as bash's do: A=1
export A keeps A=1 exported, C=1 unset C brings back the old C, and
export -p lists the values from before. One that does not fit runs
nothing and puts back those before it.
EOF
````


### Task 12: A program's environment holds at most 64 KiB

Decision 6 (spec §8.5): the exported variables hold 64 KiB of names and values and the assignments before a command 64 KiB more, while `spawn` takes at most 64 KiB of environment. The shell refuses a bigger one before the program starts, under both runners (`runner::startable`): `relay-sh: <name>: Argument list too long`, status 126, only that command of a pipeline failing; it is checked before the command is looked up, as the kernel's `spawn` checks it. A built-in gets no environment. The red run is the shell's tests: one more byte than 64 KiB starts the program. Mutation checks (6): the bound off by one or doubled, and each of the four places unchecked, each fail a test.

**Files:**
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 10's environments.
- Produces: `vars::ENVIRONMENT_MAX`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
````

with:

````rust
    #[test]
    fn a_program_s_environment_holds_at_most_64_kib() {
        // Past it bash's words for `E2BIG`, status 126, under both
        // runners; a built-in gets no environment.
        let mut env = b"X=".to_vec();
        env.extend(core::iter::repeat_n(b'x', crate::vars::ENVIRONMENT_MAX - 3));
        env.push(0);
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.env = env;
        let too_long = |name: &str| format!("relay-sh: {name}: Argument list too long\n");
        assert_eq!(h.spawning("t-args"), (3, "".into()), "64 KiB exactly");
        assert_eq!(h.spawning("Y= t-args"), (126, too_long("t-args")));
        assert_eq!(h.spawning("Y= t-args | cat"), (0, too_long("t-args")));
        let paths: Vec<_> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["/bin/t-args", "/bin/cat"]);
        assert_eq!(h.spawning("Y= cd /tmp"), (0, "".into()));
        assert_eq!(h.run("Y= echo hi"), (126, too_long("echo")));
        assert_eq!(h.run("echo hi | Y= cat"), (126, too_long("cat")));
        assert_eq!(h.run("Y= echo hi | cat"), (0, too_long("echo")));
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
    }

    #[test]
    fn assignments_before_a_script_are_in_its_variables_only() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `ENVIRONMENT_MAX` in module `crate::vars` ``.

- [ ] **Step 3: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::transcript::Transcript;
use alloc::boxed::Box;
````

with:

````rust
use crate::transcript::Transcript;
use crate::vars::ENVIRONMENT_MAX;
use alloc::boxed::Box;
````

Replace:

````rust
    fn run(&mut self, parts: Parts<'_>, name: &str, args: &[String], fds: Fds) -> Ran {
        match commands::find(name) {
````

with:

````rust
    fn run(&mut self, parts: Parts<'_>, name: &str, args: &[String], fds: Fds) -> Ran {
        if let Err(e) = startable(parts.env) {
            return cannot_start(name, e);
        }
        match commands::find(name) {
````

Replace:

````rust
            let mut err_pipe = Vec::new();
            if let Some(command) = commands::find(name) {
                let mut ctx = match fds.0[1] {
````

with:

````rust
            let mut err_pipe = Vec::new();
            let refused = startable(stage.env).err();
            if let (None, Some(command)) = (refused, commands::find(name)) {
                let mut ctx = match fds.0[1] {
````

Replace:

````rust
                // On its fd 2: the screen, a file, or the pipe.
                let message = not_found(name).message;
                match fds.0[2] {
````

with:

````rust
                // On its fd 2: the screen, a file, or the pipe.
                let message = match refused {
                    Some(e) => cannot_start(name, e).message,
                    None => not_found(name).message,
                };
                match fds.0[2] {
````

Replace:

````rust
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot, None, None));
        match self
            .programs
            .spawn(path.as_bytes(), &argv, parts.env, fds, group)
        {
            Ok(pid) => match self.programs.wait(pid) {
````

with:

````rust
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot, None, None));
        let spawned = startable(parts.env).and_then(|()| {
            self.programs
                .spawn(path.as_bytes(), &argv, parts.env, fds, group)
        });
        match spawned {
            Ok(pid) => match self.programs.wait(pid) {
````

Replace:

````rust
            let shell_fds = fds.0.map(|slot| shell_fd(parts.files, slot, stdin, stdout));
            let pid = self
                .programs
                .spawn(path.as_bytes(), &argv, stage.env, shell_fds, group);
            match pid {
````

with:

````rust
            let shell_fds = fds.0.map(|slot| shell_fd(parts.files, slot, stdin, stdout));
            let pid = startable(stage.env).and_then(|()| {
                self.programs
                    .spawn(path.as_bytes(), &argv, stage.env, shell_fds, group)
            });
            match pid {
````

Replace:

````rust
    Ran::own(status, format!("{NAME}: {name}: {e}\n"))
}
````

with:

````rust
    Ran::own(status, format!("{NAME}: {name}: {e}\n"))
}

/// A program may start with the environment `env`: one over 64 KiB is
/// refused before it starts, as `spawn` refuses it (`E2BIG`), so that both
/// runners say bash's `Argument list too long`.
fn startable(env: &[u8]) -> Result<(), Errno> {
    if env.len() > ENVIRONMENT_MAX {
        return Err(Errno::E2BIG);
    }
    Ok(())
}
````

- [ ] **Step 4: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
pub const VARS_MAX: usize = 64 * 1024;

````

with:

````rust
pub const VARS_MAX: usize = 64 * 1024;

/// The most a program's environment holds, as `spawn` takes it
/// (programmable shell gate §8.1, §8.5).
pub const ENVIRONMENT_MAX: usize = 64 * 1024;

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 441 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): refuse a program's environment over 64 KiB

The exported variables may hold 64 KiB of names and values, and the
assignments before a command 64 KiB more, while spawn takes at most
64 KiB of environment. The shell now refuses a bigger one before the
program starts, under both runners, with bash's words for E2BIG:
relay-sh: <name>: Argument list too long, status 126, only that command
of a pipeline failing. A built-in gets no environment, so no limit.
EOF
````


### Task 13: Assignments before a command in QEMU

Decisions 5 and 6: the scenario `environment` goes on: assignments before `t-env` go over the exported variables, one exported in its place; each command of a pipeline has its own; `unset` and `export` before a built-in keep bash's rule; and a 40 KiB exported variable assigned again before `t-env` makes an environment past 64 KiB, refused with status 126 (the kernel would refuse it too, so the shell's own check is caught by the unit tests). Tests only: no failing run.

**Files:**
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: Tasks 10–12.
- Produces: the scenario's lines.

- [ ] **Step 1: Expect the new lines in `tests/e2e/environment.txt`**

In `tests/e2e/environment.txt`, replace:

````text
expect \n\[\]\nroot@relay:~# $
````

with:

````text
expect \n\[\]\nroot@relay:~# $
# Assignments before a command are its own: before a program they go
# over the exported variables, an exported name in its place; before a
# built-in they hold while it runs, unless it sets or exports them.
send X=1 C=new t-env; echo "[$X][$C]"
expect \n3 entries, 21 bytes\n\[0\] HOME=/root\n\[1\] C=new\n\[2\] X=1\n\[\]\[two words\]\nroot@relay:~# $
send t-env | Y=2 t-env
expect \n3 entries, 27 bytes\n\[0\] HOME=/root\n\[1\] C=two words\n\[2\] Y=2\nroot@relay:~# $
send C=1 unset C; A=5 export A; echo "[$C]"; t-env
expect \n\[two words\]\n3 entries, 27 bytes\n\[0\] HOME=/root\n\[1\] C=two words\n\[2\] A=5\nroot@relay:~# $
# A program's environment holds at most 64 KiB: X holds 40 KiB.
send X=xxxxxxxxxx; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X
expect root@relay:~# $
send export X; t-env raw
expect \nan address, 40990 bytes, 4 entries\nroot@relay:~# $
send Y=$X t-env; echo $?
expect \nrelay-sh: t-env: Argument list too long\n126\nroot@relay:~# $
send unset X
expect root@relay:~# $
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
test(e2e): run assignments before a command in QEMU

The scenario environment goes on: assignments before t-env go over the
exported variables, one exported in its place; each command of a
pipeline has its own; unset and export before a built-in keep bash's
rule; and a 40 KiB exported variable assigned again before t-env makes
an environment past 64 KiB, refused with status 126.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p3/assign
gh pr create --base main --head m5p3/assign --title "feat(shell): give a command the assignments typed before it" --body-file - <<'EOF'
## What

Milestone 5, plan 3, tasks 9–13: `A=1 cmd` is no longer refused: the words expand first, then the assignments left to right, each reading the ones before, within the line's 64 KiB; a program, each command of a pipeline and a background job get the exported variables with their assignments over them, an exported name in its place; before a built-in they hold while it runs and come back unless it set or exported them, as bash's do; before words that expand to nothing they stay set; a program's environment over 64 KiB is refused before it starts, `Argument list too long`, status 126, under both runners; `help` says how to export and assign; the scenario `environment` runs them.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-assign
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `cd`, `PWD`, `OLDPWD`, `HOME` and `~` (Tasks 14–23)

A shell starts with `PWD` and `OLDPWD` as bash's; `cd` follows `HOME`, `OLDPWD` and `PWD` (`cd -`) and sets them as bash's does; `~` and the prompt follow `HOME`; `host-shell` starts with `HOME=/root`; a script's `cd` stays in it; every check script starts with `cd /root`; the bash corpus compares the environment.

Branch `m5p3/cd`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-cd`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p3/cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-cd origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-cd
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p3/assign` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p3/cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-cd m5p3/assign`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p3/assign>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 14: `PWD` and `OLDPWD` when a shell starts

Decision 7 (spec §8.5): after its import a shell exports `PWD`, the current directory, in its place if it was imported, and exports `OLDPWD` without a value unless it imported one naming a directory, as bash 5.2 does (probed with exact environments); a script the in-process runner runs starts so too. Every program the shell starts now gets `PWD`, so `env_calls` and `environment` show it after `HOME`, and the unit tests that list `export` or a child's block gain `PWD` and `OLDPWD`, as bash shows them. The red run is the shell's tests: `Vars::start` is missing. Mutation checks (6): no `PWD`, `OLDPWD` always or never reset, `OLDPWD` keeping a value, any file taken for a directory, and the in-process script not starting so, each fail a test (the last after its test was given a stale `PWD`).

**Files:**
- Modify: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`
- Modify: `tests/e2e/env_calls.txt`
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: Task 6's `with_environment`.
- Produces: `Vars::start(cwd, oldpwd_is_dir)`, `shell::start_variables`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
    fn export_marks_variables_and_lists_them_as_bash_does() {
        // `env -i HOME=/root bash` in a pty, but for the variables bash
        // sets itself (`OLDPWD`, `PWD`, `SHLVL`).
        let mut h = Harness::new();
````

with:

````rust
    fn export_marks_variables_and_lists_them_as_bash_does() {
        // `env -i HOME=/root bash` in a pty, from `/`, but for `HOME` and
        // `SHLVL`.
        let mut h = Harness::new();
````

Replace:

````rust
             declare -x D=\"\"\n\
             declare -x E=\"a \\\"b\\\" \\$c \\`d\\` \\\\e\"\n"
        );
        assert_eq!(
            h.lines(&["export Z=1", "export -p"]).1,
            "declare -x Z=\"1\"\n"
        );
        // `-p` with names exports them, and `--` ends the options.
        assert_eq!(
            h.lines(&["export -p Y", "export -- X=-", "export"]).1,
            "declare -x X=\"-\"\ndeclare -x Y\n"
````

with:

````rust
             declare -x D=\"\"\n\
             declare -x E=\"a \\\"b\\\" \\$c \\`d\\` \\\\e\"\n\
             declare -x OLDPWD\n\
             declare -x PWD=\"/\"\n"
        );
        assert_eq!(
            h.lines(&["export Z=1", "unset OLDPWD PWD", "export -p"]).1,
            "declare -x Z=\"1\"\n"
        );
        // `-p` with names exports them, and `--` ends the options.
        assert_eq!(
            h.lines(&["export -p Y", "export -- X=-", "unset OLDPWD PWD", "export"])
                .1,
            "declare -x X=\"-\"\ndeclare -x Y\n"
````

Replace:

````rust
                 relay-sh: export: `=x': not a valid identifier\n\
                 declare -x B=\"2\"\n"
                    .into()
````

with:

````rust
                 relay-sh: export: `=x': not a valid identifier\n\
                 declare -x B=\"2\"\n\
                 declare -x OLDPWD\n\
                 declare -x PWD=\"/\"\n"
                    .into()
````

Replace:

````rust
        ] {
            assert_eq!(h.lines(&[line, "export"]), (0, message.into()), "{line}");
            assert_eq!(h.run(line).0, status, "{line}");
````

with:

````rust
        ] {
            assert_eq!(
                h.lines(&[line, "unset OLDPWD PWD", "export"]),
                (0, message.into()),
                "{line}"
            );
            assert_eq!(h.run(line).0, status, "{line}");
````

Replace:

````rust
                "unset A B",
                "unset -v -- D NEVER",
                "echo [$A$B$C]",
````

with:

````rust
                "unset A B",
                "unset -v -- D NEVER OLDPWD PWD",
                "echo [$A$B$C]",
````

Replace:

````rust
                &alloc::format!("export A={}", &big[..200]),
                "export"
````

with:

````rust
                &alloc::format!("export A={}", &big[..200]),
                "unset OLDPWD PWD",
                "export"
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                env: Vec::new(),
                fds: [0, 1, 2],
````

with:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                env: b"PWD=/\0".to_vec(),
                fds: [0, 1, 2],
````

Replace:

````rust
            args: words(args),
            env: Vec::new(),
            fds: [fds.0.unwrap_or(0), fds.1.unwrap_or(1), 2],
````

with:

````rust
            args: words(args),
            env: b"PWD=/\0".to_vec(),
            fds: [fds.0.unwrap_or(0), fds.1.unwrap_or(1), 2],
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
            [
                b"HOME=/root\0X=1\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
            ],
            "a lone command, each command of a pipeline, a background job"
        );
        // With nothing exported, none.
        h.spawning("t-args");
        assert_eq!(h.programs.spawned.last().unwrap().env, b"");
    }
````

with:

````rust
            [
                b"HOME=/root\0X=1\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
                b"HOME=/root\0PWD=/\0B=2\0".to_vec(),
            ],
            "a lone command, each command of a pipeline, a background job"
        );
        // With nothing imported, `PWD` alone.
        h.spawning("t-args");
        assert_eq!(h.programs.spawned.last().unwrap().env, b"PWD=/\0");
    }
````

Replace:

````rust
            [
                b"HOME=/root\0B=3\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0".to_vec(),
                b"HOME=/root\0B=2\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0X=2\0".to_vec(),
                b"HOME=/root\0B=2\0Y=2\0".to_vec(),
                b"HOME=/root\0B=4\0".to_vec(),
            ]
````

with:

````rust
            [
                b"HOME=/root\0B=3\0PWD=/\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0A=1\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0X=2\0".to_vec(),
                b"HOME=/root\0B=2\0PWD=/\0Y=2\0".to_vec(),
                b"HOME=/root\0B=4\0PWD=/\0".to_vec(),
            ]
````

Replace:

````rust
            out,
            "declare -x A=\"1\"\ndeclare -x W=\"2\"\ndeclare -x Z=\"0\"\n[1][][0][2][][0][]\n"
        );
````

with:

````rust
            out,
            "declare -x A=\"1\"\ndeclare -x OLDPWD\ndeclare -x PWD=\"/\"\ndeclare -x W=\"2\"\n\
             declare -x Z=\"0\"\n[1][][0][2][][0][]\n"
        );
````

Replace:

````rust
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 10);
        let (status, out) = h.lines(&[
````

with:

````rust
        let mut h = Harness::new();
        let big = "x".repeat(crate::vars::VARS_MAX - 30);
        let (status, out) = h.lines(&[
````

Replace:

````rust
            out,
            "/bin/sh /root [x  y]\ndeclare -x A=\"x  y\"\ndeclare -x HOME=\"/root\"\n"
        );
````

with:

````rust
            out,
            "/bin/sh /root [x  y]\ndeclare -x A=\"x  y\"\ndeclare -x HOME=\"/root\"\n\
             declare -x OLDPWD\ndeclare -x PWD=\"/\"\n"
        );
    }

    #[test]
    fn a_shell_sets_pwd_and_keeps_an_oldpwd_that_names_a_directory() {
        // As bash 5.2 does when it starts (`env -i PWD=… OLDPWD=… bash`).
        let mut h = Harness::new();
        vfs::Vfs::chdir(&mut h.vfs, b"/tmp").unwrap();
        for (env, out) in [
            (&b"PWD=/nowhere\0OLDPWD=/etc\0"[..], "[/tmp][/etc]\n"),
            (b"OLDPWD=/nonexistent\0", "[/tmp][]\n"),
            (b"OLDPWD=/etc/motd\0", "[/tmp][]\n"),
            (b"OLDPWD=\0", "[/tmp][]\n"),
        ] {
            h.env = env.to_vec();
            assert_eq!(h.run("echo \"[$PWD][$OLDPWD]\""), (0, out.into()));
        }
        // A script the in-process runner runs starts so too, where it runs.
        h.env = b"OLDPWD=/etc\0".to_vec();
        h.put("/tmp/s.sh", b"echo \"[$PWD][$OLDPWD]\"\n");
        vfs::Vfs::chdir(&mut h.vfs, b"/").unwrap();
        assert_eq!(
            h.lines(&["PWD=/x", "export OLDPWD=/nonexistent", "sh /tmp/s.sh"])
                .1,
            "+ echo \"[$PWD][$OLDPWD]\"\n[/][]\n"
        );
````

Replace:

````rust
        assert_eq!(status, 0);
        assert_eq!(said, "+ export\ndeclare -x HOME=\"/root\"\n");
    }
````

with:

````rust
        assert_eq!(status, 0);
        assert_eq!(
            said,
            "+ export\ndeclare -x HOME=\"/root\"\ndeclare -x OLDPWD\ndeclare -x PWD=\"/\"\n"
        );
    }
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    #[test]
    fn unset_removes_a_variable_exported_or_not() {
````

with:

````rust
    #[test]
    fn a_shell_starts_with_pwd_and_oldpwd_exported() {
        let mut v = Vars::new("sh");
        v.import(b"HOME=/root\0");
        v.start("/tmp", false);
        assert_eq!(v.environment_with(&[]), b"HOME=/root\0PWD=/tmp\0");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [
                ("HOME", Some("/root")),
                ("OLDPWD", None),
                ("PWD", Some("/tmp"))
            ]
        );
        // An imported `PWD` keeps its place with the directory's path; an
        // imported `OLDPWD` is kept if it names a directory.
        let mut v = Vars::new("sh");
        v.import(b"PWD=/elsewhere\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", true);
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", false);
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0A=1\0");
        assert_eq!(v.value("OLDPWD"), None);
    }

    #[test]
    fn unset_removes_a_variable_exported_or_not() {
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/env_calls.txt`**

In `tests/e2e/env_calls.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send t-env
expect \n1 entry, 11 bytes\n\[0\] HOME=/root\nroot@relay:~# $
send t-env raw
expect \nan address, 11 bytes, 1 entry\nroot@relay:~# $
send t-env var HOME; t-env var NOPE
````

with:

````text
send t-env
expect \n2 entries, 21 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\nroot@relay:~# $
send t-env raw
expect \nan address, 21 bytes, 2 entries\nroot@relay:~# $
send t-env var HOME; t-env var NOPE
````

Replace:

````text
send t-env sh
expect \n2 entries, 15 bytes\n\[0\] X=1\n\[1\] HOME=/root\nroot@relay:~# $
send echo 'if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then echo deep; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi' > env-deep.sh
````

with:

````text
send t-env sh
expect \n3 entries, 25 bytes\n\[0\] X=1\n\[1\] HOME=/root\n\[2\] PWD=/root\nroot@relay:~# $
send echo 'if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then echo deep; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi' > env-deep.sh
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/environment.txt`**

Replace the whole of `tests/e2e/environment.txt` with:

````text
# The shell's environment (programmable shell gate §8.5, §11.3; milestone
# 5, plan 3): a variable reaches a program only once exported, in the order
# of export, and one exported without a value not at all; export lists
# them as bash's declare -x; unset removes one; a script, X | sh and a
# nested sh start with the exported ones, and nothing comes back out;
# PWD and OLDPWD are set as bash sets them.
timeout 30
expect root@relay:~# $
# The shell starts with PWD exported and OLDPWD exported without a value.
send echo "[$PWD][$OLDPWD]"
expect \n\[/root\]\[\]\nroot@relay:~# $
send A=1; t-env
expect \n2 entries, 21 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\nroot@relay:~# $
send export A B; t-env
expect \n3 entries, 25 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] A=1\nroot@relay:~# $
send export C='two words'; export
expect \ndeclare -x A="1"\ndeclare -x B\ndeclare -x C="two words"\ndeclare -x HOME="/root"\ndeclare -x OLDPWD\ndeclare -x PWD="/root"\nroot@relay:~# $
send unset A; t-env
expect \n3 entries, 33 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\nroot@relay:~# $
send export 1A=x; echo $?
expect \nrelay-sh: export: `1A=x': not a valid identifier\n1\nroot@relay:~# $
send echo 'echo "[$C][$D]"; export D=in; t-env' > /root/env.sh
expect root@relay:~# $
send D=out; sh /root/env.sh
expect \n\+ echo "\[\$C\]\[\$D\]"; export D=in; t-env\n\[two words\]\[\]\n4 entries, 38 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\n\[3\] D=in\nroot@relay:~# $
send echo "[$D]"; t-env
expect \n\[out\]\n3 entries, 33 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\nroot@relay:~# $
send echo 'echo "[$C]"; t-env' | sh
expect \n\[two words\]\n3 entries, 33 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\nroot@relay:~# $
send sh
expect \nroot@relay:~# $
send export E=nested; t-env
expect \n4 entries, 42 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\n\[3\] E=nested\nroot@relay:~# $
send exit
expect root@relay:~# $
send echo "[$E]"
expect \n\[\]\nroot@relay:~# $
# Assignments before a command are its own: before a program they go
# over the exported variables, an exported name in its place; before a
# built-in they hold while it runs, unless it sets or exports them.
send X=1 C=new t-env; echo "[$X][$C]"
expect \n4 entries, 31 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=new\n\[3\] X=1\n\[\]\[two words\]\nroot@relay:~# $
send t-env | Y=2 t-env
expect \n4 entries, 37 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\n\[3\] Y=2\nroot@relay:~# $
send C=1 unset C; A=5 export A; echo "[$C]"; t-env
expect \n\[two words\]\n4 entries, 37 bytes\n\[0\] HOME=/root\n\[1\] PWD=/root\n\[2\] C=two words\n\[3\] A=5\nroot@relay:~# $
# A program's environment holds at most 64 KiB: X holds 40 KiB.
send X=xxxxxxxxxx; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X; X=$X$X
expect root@relay:~# $
send export X; t-env raw
expect \nan address, 41000 bytes, 5 entries\nroot@relay:~# $
send Y=$X t-env; echo $?
expect \nrelay-sh: t-env: Argument list too long\n126\nroot@relay:~# $
send unset X
expect root@relay:~# $
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `start` found for struct `vars::Vars` in the current scope ``.

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    /// The same shell, its environment `block` imported as exported
    /// variables (programmable shell gate §8.5).
    pub fn with_environment(mut self, block: &[u8]) -> Shell<'a> {
        self.vars.import(block);
        self
````

with:

````rust
    /// The same shell, its environment `block` imported as exported
    /// variables, and `PWD` and `OLDPWD` set as a shell sets them when it
    /// starts (programmable shell gate §8.5).
    pub fn with_environment(mut self, block: &[u8]) -> Shell<'a> {
        self.vars.import(block);
        start_variables(&mut self.vars, &mut *self.vfs);
        self
````

Replace:

````rust
        vars.import(env);
        let outer = core::mem::replace(&mut self.vars, vars);
````

with:

````rust
        vars.import(env);
        start_variables(&mut vars, &mut *self.vfs);
        let outer = core::mem::replace(&mut self.vars, vars);
````

Replace:

````rust
        .collect()
}
````

with:

````rust
        .collect()
}

/// Sets `PWD` and `OLDPWD` in a shell's `vars` as it starts, from the
/// current directory and whether the `OLDPWD` it imported is a directory.
fn start_variables(vars: &mut Vars, vfs: &mut dyn Vfs) {
    let oldpwd_is_dir = vars.value("OLDPWD").is_some_and(|old| {
        vfs.lookup(old.as_bytes())
            .and_then(|node| vfs.stat(node))
            .is_ok_and(|s| s.kind == vfs::FileType::Directory)
    });
    vars.start(&path::display(&vfs.cwd()), oldpwd_is_dir);
}
````

- [ ] **Step 9: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
        self.names.get(name).and_then(|v| v.value.as_deref())
    }
````

with:

````rust
        self.names.get(name).and_then(|v| v.value.as_deref())
    }

    /// What a shell sets when it starts, after its import (programmable
    /// shell gate §8.5): `PWD`, exported, the current directory `cwd`, in
    /// its place if it was imported; and `OLDPWD` exported without a value,
    /// unless it was imported naming a directory, as bash's are.
    pub fn start(&mut self, cwd: &str, oldpwd_is_dir: bool) {
        let _ = self.export("PWD", Some(String::from(cwd)));
        if !oldpwd_is_dir {
            self.unset("OLDPWD");
            let _ = self.export("OLDPWD", None);
        }
    }
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 443 tests.

- [ ] **Step 11: Run the `environment`, `env_calls` scenarios**

Run: `cargo xtask test --e2e-only --scenario environment`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario env_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,e2e): set PWD and OLDPWD when a shell starts

After its import a shell exports PWD, the current directory, in its
place if it was imported, and exports OLDPWD without a value unless it
imported one naming a directory, as bash does; a script the in-process
runner runs starts so too. Every program the shell starts now gets PWD,
so env_calls and environment show it after HOME.
EOF
````


### Task 15: `cd [-L|-P] [--] [dir]` follows `HOME`, `OLDPWD` and `PWD`

Decision 8 (spec §9.1), against bash 5.2 in a pty: `cd` takes `-L` and `-P`, which change nothing as no symbolic link is followed, and `--`, through Task 2's `options` (`-e` not supported, `-@` and others bash's `invalid option`, status 2); without a directory it goes to `$HOME`, and `cd -` goes to `$OLDPWD` and prints it (an empty one prints an empty line); `HOME` or `OLDPWD` unset is bash's `not set`, empty stays where it is. On success `OLDPWD` takes `PWD`'s value and `PWD` the new directory from `getcwd`, each exported or not as it was; a failed `cd` changes neither. Tests that run `cd` without a directory get `HOME`. The scenario `environment` runs them. The red run is the shell's tests: `cd -` is `not supported` and `HOME` is ignored. Mutation checks (7): `OLDPWD` from the current directory, no `PWD`, an empty directory moving, `cd -` silent, an empty `OLDPWD` silent, `-e` allowed, a failed `cd` setting them, each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/commands/export.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: Task 14's variables; Task 2's `options` (now `pub(super)`).
- Produces: nothing new outside `cd`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let mut h = Harness::new();
        assert_eq!(h.run("cd /etc"), (0, "".into()));
````

with:

````rust
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(h.run("cd /etc"), (0, "".into()));
````

Replace:

````rust
        // home, as `cd` alone.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["cd /etc", "cd \"\"", "pwd", "cd \"$1\"", "pwd"]),
````

with:

````rust
        // home, as `cd` alone.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /etc", "cd \"\"", "pwd", "cd \"$1\"", "pwd"]),
````

Replace:

````rust
        );
        assert_eq!(
            h.run("cd -"),
            (1, "relay-sh: cd: -: not supported\n".into())
        );
        assert_eq!(
            h.run("cd -P"),
            (2, "relay-sh: cd: -P: invalid option\n".into())
        );
        assert_eq!(h.run("pwd").1, "/\n", "nothing changed the directory");
    }
````

with:

````rust
        );
        assert_eq!(h.run("pwd").1, "/\n", "nothing changed the directory");
    }

    #[test]
    fn cd_s_options_are_bash_s() {
        // bash 5.2's, without the usage line; `-@` Ubuntu's bash does not
        // have, and its `-e` is not supported here.
        let mut h = Harness::new();
        for (line, status, said) in [
            ("cd -x /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
            ("cd -Lx /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
            ("cd -@ /tmp", 2, "relay-sh: cd: -@: invalid option\n"),
            ("cd -e /tmp", 1, "relay-sh: cd: -e: not supported\n"),
            ("cd -Pe /tmp", 1, "relay-sh: cd: -e: not supported\n"),
            ("cd -e -x /tmp", 2, "relay-sh: cd: -x: invalid option\n"),
        ] {
            assert_eq!(h.run(line), (status, said.into()), "{line}");
        }
        assert_eq!(h.run("pwd").1, "/\n");
        for line in ["cd -L /tmp", "cd -P /etc", "cd -LP -- /tmp", "cd -LL /etc"] {
            assert_eq!(h.run(line), (0, "".into()), "{line}");
        }
        assert_eq!(h.run("pwd").1, "/etc\n");
    }

    #[test]
    fn cd_without_home_or_with_it_empty() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["cd /tmp", "cd", "pwd"]),
            (0, "relay-sh: cd: HOME not set\n/tmp\n".into())
        );
        assert_eq!(
            h.lines(&["HOME=", "cd /tmp", "cd", "echo $?", "pwd"]).1,
            "0\n/tmp\n"
        );
        assert_eq!(h.lines(&["HOME=/etc", "cd --", "pwd"]).1, "/etc\n");
    }

    #[test]
    fn cd_dash_goes_back_and_says_where() {
        // As bash 5.2 in a pty.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&[
                "cd -",
                "echo $?",
                "cd /tmp",
                "cd -",
                "cd -- -",
                "OLDPWD=",
                "cd -",
                "echo $? $PWD",
                "unset OLDPWD",
                "cd -",
            ]),
            (
                1,
                "relay-sh: cd: OLDPWD not set\n1\n/\n/tmp\n\n0 /tmp\n\
                 relay-sh: cd: OLDPWD not set\n"
                    .into()
            )
        );
        assert_eq!(
            h.lines(&["OLDPWD=/nope", "cd -"]),
            (1, "relay-sh: cd: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("cd - x").1, "relay-sh: cd: too many arguments\n");
    }

    #[test]
    fn cd_sets_oldpwd_and_pwd_keeping_whether_they_are_exported() {
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /root/../tmp", "echo $PWD $OLDPWD", "export"])
                .1,
            "/tmp /\n\
             declare -x HOME=\"/root\"\n\
             declare -x OLDPWD=\"/\"\n\
             declare -x PWD=\"/tmp\"\n"
        );
        // A failed cd changes neither.
        assert_eq!(
            h.lines(&["cd /etc", "cd /nope", "echo $PWD $OLDPWD"]).1,
            "relay-sh: cd: /nope: No such file or directory\n/etc /tmp\n"
        );
        // OLDPWD takes the variable PWD's value, and none without one.
        assert_eq!(h.lines(&["PWD=/x", "cd /tmp", "echo $OLDPWD"]).1, "/x\n");
        assert_eq!(
            h.lines(&["cd /etc", "unset PWD", "cd /tmp", "echo \"[$OLDPWD]\""])
                .1,
            "[/tmp]\n",
            "OLDPWD keeps what it was, not /etc"
        );
        // Made by cd, neither is exported (bash's `declare --`).
        assert_eq!(
            h.lines(&[
                "unset PWD OLDPWD",
                "cd /etc",
                "cd /tmp",
                "export",
                "echo $PWD $OLDPWD"
            ])
            .1,
            "declare -x HOME=\"/root\"\n/tmp /etc\n"
        );
    }
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let mut h = Harness::new();
        h.console.interrupt_after = Some(1);
````

with:

````rust
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.console.interrupt_after = Some(1);
````

Replace:

````rust
        let mut h = Harness::new();
        h.console.interrupt_after = Some(50);
````

with:

````rust
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.console.interrupt_after = Some(50);
````

Replace:

````rust
            out,
            "declare -x A=\"1\"\ndeclare -x OLDPWD\ndeclare -x PWD=\"/\"\ndeclare -x W=\"2\"\n\
             declare -x Z=\"0\"\n[1][][0][2][][0][]\n"
````

with:

````rust
            out,
            "declare -x A=\"1\"\ndeclare -x OLDPWD=\"/\"\ndeclare -x PWD=\"/tmp\"\ndeclare -x W=\"2\"\n\
             declare -x Z=\"0\"\n[1][][0][2][][0][]\n"
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/environment.txt`**

In `tests/e2e/environment.txt`, replace:

````text
send unset X
expect root@relay:~# $
````

with:

````text
send unset X
expect root@relay:~# $
# cd sets OLDPWD and PWD; cd - goes back and says where; cd follows HOME.
send cd /tmp; cd -; echo "$PWD $OLDPWD"
expect \n/root\n/root /tmp\nroot@relay:~# $
send cd /root/../tmp; echo $PWD; cd
expect \n/tmp\nroot@relay:~# $
send HOME=/etc cd; pwd; echo $HOME
expect \n/etc\n/root\nroot@relay:/etc# $
send unset HOME; cd; echo $?; export HOME=/root; cd
expect \nrelay-sh: cd: HOME not set\n1\nroot@relay:~# $
send cd -e /tmp; cd -x; echo $?
expect \nrelay-sh: cd: -e: not supported\nrelay-sh: cd: -x: invalid option\n2\nroot@relay:~# $
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 5 tests fail, among them `commands::basic::tests::cd_s_options_are_bash_s`, `commands::basic::tests::cd_sets_oldpwd_and_pwd_keeping_whether_they_are_exported`.

- [ ] **Step 5: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use super::{BUILTINS, COMMANDS};
use crate::ctx::{Ctx, getopt, outln};
use crate::parser::HOME;
use crate::shell::NAME;
````

with:

````rust

use super::export::options;
use super::{BUILTINS, COMMANDS};
use crate::ctx::{Ctx, getopt, outln};
use crate::shell::NAME;
````

Replace:

````rust

/// `cd [dir]`: no argument goes to `/root`, an empty one nowhere (as in
/// bash). `cd -` is not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let dir = match args {
        [] => HOME,
        [dir] if dir.is_empty() => return 0,
        [dir] if dir == "-" => {
            return ctx.fail(NAME, format_args!("cd: -: not supported"));
        }
        [dir] if dir.starts_with('-') => {
            ctx.fail(NAME, format_args!("cd: {dir}: invalid option"));
            return 2;
        }
        [dir] => dir.as_str(),
        _ => return ctx.fail(NAME, format_args!("cd: too many arguments")),
    };
    match ctx.vfs.chdir(dir.as_bytes()) {
        Ok(()) => 0,
        Err(e) => ctx.fail(NAME, format_args!("cd: {dir}: {e}")),
    }
}
````

with:

````rust

/// `cd [-L|-P] [--] [dir]` (programmable shell gate §9.1), as bash 5.2's:
/// no directory goes to `$HOME`, `-` to `$OLDPWD`, printing it, and an
/// empty one, or an empty `HOME` or `OLDPWD`, nowhere (`cd -` printing an
/// empty line). On success `OLDPWD` takes `PWD`'s value, if it has one,
/// and `PWD` the new directory, each exported or not as it was. `-L` and
/// `-P` change nothing, as no symbolic link is followed; bash's `-e` is
/// not supported.
pub fn cd(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (_, args) = match options(ctx, "cd", args, "LPe", "e") {
        Ok(read) => read,
        Err(status) => return status,
    };
    let (dir, show) = match args {
        [] => match ctx.vars().value("HOME") {
            None => return ctx.fail(NAME, format_args!("cd: HOME not set")),
            Some(home) => (String::from(home), false),
        },
        [dir] if dir == "-" => match ctx.vars().value("OLDPWD") {
            None => return ctx.fail(NAME, format_args!("cd: OLDPWD not set")),
            Some(old) => (String::from(old), true),
        },
        [dir] => (dir.clone(), false),
        _ => return ctx.fail(NAME, format_args!("cd: too many arguments")),
    };
    if dir.is_empty() {
        if show {
            outln!(ctx, "");
        }
        return 0;
    }
    if let Err(e) = ctx.vfs.chdir(dir.as_bytes()) {
        return ctx.fail(NAME, format_args!("cd: {dir}: {e}"));
    }
    let cwd = path::display(&ctx.vfs.cwd());
    let vars = ctx.vars();
    if let Some(old) = vars.value("PWD").map(String::from) {
        let _ = vars.set("OLDPWD", old);
    }
    let _ = vars.set("PWD", cwd.clone());
    if show {
        outln!(ctx, "{cwd}");
    }
    0
}
````

- [ ] **Step 6: Change `crates/shell/src/commands/export.rs`**

In `crates/shell/src/commands/export.rs`, replace:

````rust
/// which bash knows, is not supported (status 1).
fn options<'a>(
    ctx: &mut Ctx<'_>,
````

with:

````rust
/// which bash knows, is not supported (status 1).
pub(super) fn options<'a>(
    ctx: &mut Ctx<'_>,
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 447 tests.

- [ ] **Step 8: Run the `environment` scenario**

Run: `cargo xtask test --e2e-only --scenario environment`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,e2e): follow HOME, OLDPWD and PWD in cd

cd now takes bash 5.2's -L and -P, which change nothing as no symbolic
link is followed, and --; -e is not supported and another option is
bash's invalid option. Without a directory it goes to $HOME, and cd -
goes to $OLDPWD and prints it; HOME or OLDPWD unset is bash's "not
set", empty stays where it is. On success OLDPWD takes PWD's value and
PWD the new directory from getcwd, each exported or not as it was; a
failed cd changes neither. The environment scenario runs them.
EOF
````


### Task 16: `cd` sets `OLDPWD` as bash's does

Decision 8 (the prototype's review, I-1): with `PWD` unset, `cd` left `OLDPWD` as it was, so `cd -` went back to an old directory where bash 5.2 says `OLDPWD not set`: bash's `cd` takes `OLDPWD`'s value away, keeping whether it is exported (`Vars::clear`). And `cd` to an empty directory (`cd ""`, an empty `HOME` or `OLDPWD`) is a `cd` that succeeded in bash, which sets `OLDPWD` to `$PWD`, where `cd` changed nothing. The red run is the shell's tests: both cases. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Task 15's `cd`.
- Produces: `Vars::clear(name)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn cd_dash_goes_back_and_says_where() {
````

with:

````rust
    #[test]
    fn cd_to_an_empty_directory_still_sets_oldpwd() {
        // bash counts it a cd that succeeded: OLDPWD takes PWD's value.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(
            h.lines(&["cd /tmp", "cd /etc", "cd \"\"", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "[/etc][/etc]\n"
        );
        assert_eq!(
            h.lines(&["cd /tmp", "OLDPWD=", "cd -", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "\n[/tmp][/tmp]\n"
        );
        assert_eq!(
            h.lines(&["cd /tmp", "HOME=", "cd", "echo \"[$PWD][$OLDPWD]\""])
                .1,
            "[/tmp][/tmp]\n"
        );
    }

    #[test]
    fn cd_dash_goes_back_and_says_where() {
````

Replace:

````rust
        );
        // OLDPWD takes the variable PWD's value, and none without one.
        assert_eq!(h.lines(&["PWD=/x", "cd /tmp", "echo $OLDPWD"]).1, "/x\n");
        assert_eq!(
            h.lines(&["cd /etc", "unset PWD", "cd /tmp", "echo \"[$OLDPWD]\""])
                .1,
            "[/tmp]\n",
            "OLDPWD keeps what it was, not /etc"
        );
````

with:

````rust
        );
        // OLDPWD takes the variable PWD's value; without one it stays a
        // variable with no value, exported as it was, as bash's does.
        assert_eq!(h.lines(&["PWD=/x", "cd /tmp", "echo $OLDPWD"]).1, "/x\n");
        assert_eq!(
            h.lines(&[
                "cd /etc",
                "unset PWD",
                "cd /tmp",
                "cd -",
                "echo $?",
                "export"
            ])
            .1,
            "relay-sh: cd: OLDPWD not set\n1\n\
             declare -x HOME=\"/root\"\n\
             declare -x OLDPWD\n"
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::basic::tests::cd_to_an_empty_directory_still_sets_oldpwd`, `commands::basic::tests::cd_sets_oldpwd_and_pwd_keeping_whether_they_are_exported`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// empty one, or an empty `HOME` or `OLDPWD`, nowhere (`cd -` printing an
/// empty line). On success `OLDPWD` takes `PWD`'s value, if it has one,
/// and `PWD` the new directory, each exported or not as it was. `-L` and
/// `-P` change nothing, as no symbolic link is followed; bash's `-e` is
````

with:

````rust
/// empty one, or an empty `HOME` or `OLDPWD`, nowhere (`cd -` printing an
/// empty line). On success, staying where it is too, `OLDPWD` takes
/// `PWD`'s value, or none when `PWD` has none, and `PWD` the new
/// directory, each exported or not as it was. `-L` and
/// `-P` change nothing, as no symbolic link is followed; bash's `-e` is
````

Replace:

````rust
    };
    if dir.is_empty() {
        if show {
            outln!(ctx, "");
        }
        return 0;
    }
    if let Err(e) = ctx.vfs.chdir(dir.as_bytes()) {
        return ctx.fail(NAME, format_args!("cd: {dir}: {e}"));
    }
    let cwd = path::display(&ctx.vfs.cwd());
    let vars = ctx.vars();
    if let Some(old) = vars.value("PWD").map(String::from) {
        let _ = vars.set("OLDPWD", old);
    }
    let _ = vars.set("PWD", cwd.clone());
    if show {
        outln!(ctx, "{cwd}");
    }
````

with:

````rust
    };
    // An empty directory stays where it is, a cd that succeeded.
    if !dir.is_empty()
        && let Err(e) = ctx.vfs.chdir(dir.as_bytes())
    {
        return ctx.fail(NAME, format_args!("cd: {dir}: {e}"));
    }
    let cwd = path::display(&ctx.vfs.cwd());
    let vars = ctx.vars();
    let _ = match vars.value("PWD").map(String::from) {
        Some(old) => vars.set("OLDPWD", old),
        None => vars.clear("OLDPWD"),
    };
    let _ = vars.set("PWD", cwd.clone());
    if show {
        let shown = if dir.is_empty() { "" } else { cwd.as_str() };
        outln!(ctx, "{shown}");
    }
````

- [ ] **Step 4: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
                value: Some(value),
                export,
````

with:

````rust
                value: Some(value),
                export,
            },
        )?;
        self.changed(name);
        Ok(())
    }

    /// Takes `name`'s value away, exported or not as it was, as bash's
    /// `cd` does to `OLDPWD` when `PWD` has none: it stays a variable.
    pub fn clear(&mut self, name: &str) -> Result<(), Error> {
        let export = self.names.get(name).and_then(|v| v.export);
        self.put(
            name,
            Var {
                value: None,
                export,
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 448 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): set OLDPWD as bash's cd does

With PWD unset, cd left OLDPWD as it was, so cd - went back to an old
directory where bash says OLDPWD not set: bash's cd takes OLDPWD's
value away, keeping whether it is exported. And cd to an empty
directory (cd "", an empty HOME or OLDPWD) is a cd that succeeded in
bash, which sets OLDPWD to $PWD, where cd changed nothing.

Refs: review I1
EOF
````


### Task 17: `cd -` prints `$OLDPWD` as it is written

Decision 8 (the prototype's review, m-6): `cd -` printed the directory it reached, from `getcwd`; bash 5.2 prints `$OLDPWD` as written (`OLDPWD=/tmp/../etc/` prints `/tmp/../etc/`, `OLDPWD=etc` from `/` prints `etc`), which differs only when `OLDPWD` was set by hand or imported. `PWD` stays `getcwd`'s. The red run is the shell's tests. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`

**Interfaces:**
- Consumes: Task 16's `cd`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    #[test]
    fn cd_to_an_empty_directory_still_sets_oldpwd() {
````

with:

````rust
    #[test]
    fn cd_dash_prints_oldpwd_as_it_is_written() {
        // As bash's; PWD is still getcwd's.
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["OLDPWD=/tmp/../etc/", "cd -", "echo $PWD"]).1,
            "/tmp/../etc/\n/etc\n"
        );
        assert_eq!(
            h.lines(&["cd /", "OLDPWD=etc", "cd -", "pwd"]).1,
            "etc\n/etc\n"
        );
    }

    #[test]
    fn cd_to_an_empty_directory_still_sets_oldpwd() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::cd_dash_prints_oldpwd_as_it_is_written`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    };
    let _ = vars.set("PWD", cwd.clone());
    if show {
        let shown = if dir.is_empty() { "" } else { cwd.as_str() };
        outln!(ctx, "{shown}");
    }
````

with:

````rust
    };
    let _ = vars.set("PWD", cwd);
    // `cd -` says `$OLDPWD` as it is written, as bash's does.
    if show {
        outln!(ctx, "{dir}");
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 449 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): print OLDPWD as written after cd -

cd - printed the directory it reached, from getcwd; bash prints $OLDPWD
as it is written, which differs only when OLDPWD was set by hand or
imported (OLDPWD=etc from /, or /tmp/../etc/). PWD stays getcwd's.

Refs: review m6
EOF
````


### Task 18: Say when `PWD` or `OLDPWD` does not fit the variables

Decisions 7 and 8 (the prototype's review, m-5; the maintainer, 2026-10-04): `cd` and a starting shell set `PWD` and `OLDPWD` without a word when the variables could not hold them, so `pwd` and `$PWD` disagreed and children got the old `PWD`. `cd` now goes there all the same and says the variables would hold more than 64 KiB, status 1, and a shell says it once when it starts (`Vars::start` returns the error). Task 12's test of a 64 KiB environment filled it with a near-full import, which now says so: it fills it with an assignment instead, and a test of its own shows a built-in has no limit (11,000 empty variables make an environment over 64 KiB that fits the variables). The red run is the shell's tests: they do not compile while `Vars::start` returns nothing; reverting the fix, so that nothing is said, is its mutation check.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`

**Interfaces:**
- Consumes: Tasks 14 and 17.
- Produces: `Vars::start` returns `Result<(), expand::Error>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        assert_eq!(h.lines(&["HOME=/etc", "cd --", "pwd"]).1, "/etc\n");
    }
````

with:

````rust
        assert_eq!(h.lines(&["HOME=/etc", "cd --", "pwd"]).1, "/etc\n");
    }

    #[test]
    fn cd_says_when_pwd_does_not_fit_the_variables() {
        // It goes there all the same, status 1 (the maintainer,
        // 2026-10-04).
        let mut h = Harness::new();
        let fill = alloc::format!("A={}", "x".repeat(crate::vars::VARS_MAX - 13));
        assert_eq!(
            h.lines(&[&fill, "cd /tmp", "echo $? \"[$PWD]\"", "pwd"]).1,
            "relay-sh: PWD: the variables would hold more than 64 KiB\n1 [/]\n/tmp\n"
        );
    }
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        // Past it bash's words for `E2BIG`, status 126, under both
        // runners; a built-in gets no environment.
        let mut env = b"X=".to_vec();
        env.extend(core::iter::repeat_n(b'x', crate::vars::ENVIRONMENT_MAX - 3));
        env.push(0);
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.env = env;
        let too_long = |name: &str| format!("relay-sh: {name}: Argument list too long\n");
        assert_eq!(h.spawning("t-args"), (3, "".into()), "64 KiB exactly");
        assert_eq!(h.spawning("Y= t-args"), (126, too_long("t-args")));
        assert_eq!(h.spawning("Y= t-args | cat"), (0, too_long("t-args")));
        let paths: Vec<_> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["/bin/t-args", "/bin/cat"]);
        assert_eq!(h.spawning("Y= cd /tmp"), (0, "".into()));
        assert_eq!(h.run("Y= echo hi"), (126, too_long("echo")));
        assert_eq!(h.run("echo hi | Y= cat"), (126, too_long("cat")));
        assert_eq!(h.run("Y= echo hi | cat"), (0, too_long("echo")));
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
    }
````

with:

````rust
        // Past it bash's words for `E2BIG`, status 126, under both
        // runners; a built-in gets no environment. Exported, `X=…` and
        // `PWD=/` take 40,009 bytes; `Y=…` before a command fills the
        // rest of 64 KiB, or one byte more.
        let mut env = b"X=".to_vec();
        env.extend(core::iter::repeat_n(b'x', 40_000));
        env.push(0);
        let rest = crate::vars::ENVIRONMENT_MAX - 40_003 - 6 - 3;
        let fits = format!("Y={}", "y".repeat(rest));
        let over = format!("Y={}", "y".repeat(rest + 1));
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.env = env;
        let too_long = |name: &str| format!("relay-sh: {name}: Argument list too long\n");
        assert_eq!(
            h.spawning(&format!("{fits} t-args")),
            (3, "".into()),
            "64 KiB exactly"
        );
        assert_eq!(
            h.programs.spawned[0].env.len(),
            crate::vars::ENVIRONMENT_MAX
        );
        assert_eq!(
            h.spawning(&format!("{over} t-args")),
            (126, too_long("t-args"))
        );
        assert_eq!(
            h.spawning(&format!("{over} t-args | cat")),
            (0, too_long("t-args"))
        );
        let paths: Vec<_> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["/bin/t-args", "/bin/cat"]);
        assert_eq!(h.run(&format!("{over} echo hi")), (126, too_long("echo")));
        assert_eq!(
            h.run(&format!("echo hi | {over} cat")),
            (126, too_long("cat"))
        );
        assert_eq!(
            h.run(&format!("{over} echo hi | cat")),
            (0, too_long("echo"))
        );
        assert_eq!(h.run(&format!("{fits} echo hi")), (0, "hi\n".into()));
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
    }

    #[test]
    fn a_built_in_gets_no_environment_so_no_limit() {
        // 11,000 empty variables hold under 64 KiB of names, but their
        // entries, `V123=` and a NUL each, make more than 64 KiB.
        let mut env = Vec::new();
        for i in 0..11_000 {
            env.extend_from_slice(format!("V{i}=\0").as_bytes());
        }
        let mut h = spawning();
        h.env = env;
        let too_long = "relay-sh: t-args: Argument list too long\n";
        assert_eq!(h.spawning("t-args"), (126, too_long.into()));
        assert_eq!(h.spawning("cd /tmp"), (0, "".into()));
    }
````

Replace:

````rust
            "+ echo \"[$PWD][$OLDPWD]\"\n[/][]\n"
        );
````

with:

````rust
            "+ echo \"[$PWD][$OLDPWD]\"\n[/][]\n"
        );
    }

    #[test]
    fn a_shell_says_once_when_pwd_does_not_fit_its_variables() {
        let mut h = Harness::new();
        let mut env = b"A=".to_vec();
        env.extend(core::iter::repeat_n(b'x', crate::vars::VARS_MAX - 3));
        env.push(0);
        h.env = env;
        assert_eq!(
            h.run("echo \"[$PWD]\""),
            (
                0,
                "relay-sh: PWD: the variables would hold more than 64 KiB\n[]\n".into()
            )
        );
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        v.import(b"HOME=/root\0");
        v.start("/tmp", false);
        assert_eq!(v.environment_with(&[]), b"HOME=/root\0PWD=/tmp\0");
````

with:

````rust
        v.import(b"HOME=/root\0");
        v.start("/tmp", false).unwrap();
        assert_eq!(v.environment_with(&[]), b"HOME=/root\0PWD=/tmp\0");
````

Replace:

````rust
        v.import(b"PWD=/elsewhere\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", true);
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", false);
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0A=1\0");
````

with:

````rust
        v.import(b"PWD=/elsewhere\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", true).unwrap();
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", false).unwrap();
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0A=1\0");
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `unwrap` found for unit type `()` in the current scope ``.

- [ ] **Step 5: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    let vars = ctx.vars();
    let _ = match vars.value("PWD").map(String::from) {
        Some(old) => vars.set("OLDPWD", old),
        None => vars.clear("OLDPWD"),
    };
    let _ = vars.set("PWD", cwd);
    // `cd -` says `$OLDPWD` as it is written, as bash's does.
    if show {
        outln!(ctx, "{dir}");
    }
    0
}
````

with:

````rust
    let vars = ctx.vars();
    let oldpwd = match vars.value("PWD").map(String::from) {
        Some(old) => vars.set("OLDPWD", old),
        None => vars.clear("OLDPWD"),
    };
    let pwd = vars.set("PWD", cwd);
    // It went there all the same, as bash would; the variables say not.
    let status = match oldpwd.and(pwd) {
        Ok(()) => 0,
        Err(e) => ctx.fail(NAME, format_args!("{e}")),
    };
    // `cd -` says `$OLDPWD` as it is written, as bash's does.
    if show {
        outln!(ctx, "{dir}");
    }
    status
}
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
        self.vars.import(block);
        start_variables(&mut self.vars, &mut *self.vfs);
        self
````

with:

````rust
        self.vars.import(block);
        // Said once, as an assignment that does not fit is.
        if let Err(e) = start_variables(&mut self.vars, &mut *self.vfs) {
            self.console.write(format!("{NAME}: {e}\n").as_bytes());
        }
        self
````

Replace:

````rust
        vars.import(env);
        start_variables(&mut vars, &mut *self.vfs);
        let outer = core::mem::replace(&mut self.vars, vars);
        self.status = 0;
        let status = self.run_lines(&script.text);
````

with:

````rust
        vars.import(env);
        let started = start_variables(&mut vars, &mut *self.vfs);
        let outer = core::mem::replace(&mut self.vars, vars);
        self.status = 0;
        if let Err(e) = started {
            self.say(format!("{NAME}: {e}\n").as_bytes());
        }
        let status = self.run_lines(&script.text);
````

Replace:

````rust
/// current directory and whether the `OLDPWD` it imported is a directory.
fn start_variables(vars: &mut Vars, vfs: &mut dyn Vfs) {
    let oldpwd_is_dir = vars.value("OLDPWD").is_some_and(|old| {
````

with:

````rust
/// current directory and whether the `OLDPWD` it imported is a directory.
fn start_variables(vars: &mut Vars, vfs: &mut dyn Vfs) -> Result<(), expand::Error> {
    let oldpwd_is_dir = vars.value("OLDPWD").is_some_and(|old| {
````

Replace:

````rust
    });
    vars.start(&path::display(&vfs.cwd()), oldpwd_is_dir);
}
````

with:

````rust
    });
    vars.start(&path::display(&vfs.cwd()), oldpwd_is_dir)
}
````

- [ ] **Step 7: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, replace:

````rust
    /// its place if it was imported; and `OLDPWD` exported without a value,
    /// unless it was imported naming a directory, as bash's are.
    pub fn start(&mut self, cwd: &str, oldpwd_is_dir: bool) {
        let _ = self.export("PWD", Some(String::from(cwd)));
        if !oldpwd_is_dir {
            self.unset("OLDPWD");
            let _ = self.export("OLDPWD", None);
        }
    }
````

with:

````rust
    /// its place if it was imported; and `OLDPWD` exported without a value,
    /// unless it was imported naming a directory, as bash's are. The first
    /// that does not fit `VARS_MAX` is the error, the other still set.
    pub fn start(&mut self, cwd: &str, oldpwd_is_dir: bool) -> Result<(), Error> {
        let pwd = self.export("PWD", Some(String::from(cwd)));
        let mut oldpwd = Ok(());
        if !oldpwd_is_dir {
            self.unset("OLDPWD");
            oldpwd = self.export("OLDPWD", None);
        }
        pwd.and(oldpwd)
    }
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 452 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): say when PWD or OLDPWD does not fit the variables

cd and a starting shell set PWD and OLDPWD without a word when the
variables could not hold them, so pwd and $PWD disagreed and children
got the old PWD. cd now goes there all the same and says the variables
would hold more than 64 KiB, status 1, and a shell says it once when it
starts (the maintainer, 2026-10-04). The test of a 64 KiB environment
fills it with an assignment instead of a near-full import, which no
longer leaves PWD out without a word.

Refs: review m5
EOF
````


### Task 19: `host-shell` starts with `HOME=/root`

Decision 8: `host-shell`'s shell started with no environment, so with `cd` and (next task) `~` and the prompt following `HOME` it would have no home. It starts with init's `HOME=/root`, as `/bin/sh` does on Relay OS. The red run is xtask's tests: `echo $HOME` prints nothing in the session. Mutation check (1): no `HOME` fails the session's test.

**Files:**
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: Task 6's `with_environment`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            input: b"mkdir /root/notes\recho hello > /root/notes/a\rcat /root/notes/a\r\
                echo gone > /dev/null\r[ -c /dev/null ] && echo device\rpoweroff\r"
                .iter()
````

with:

````rust
            input: b"mkdir /root/notes\recho hello > /root/notes/a\rcat /root/notes/a\r\
                echo gone > /dev/null\r[ -c /dev/null ] && echo device\recho $HOME\rpoweroff\r"
                .iter()
````

Replace:

````rust
        assert!(screen.contains("&& echo device\ndevice\n"), "{screen}");

````

with:

````rust
        assert!(screen.contains("&& echo device\ndevice\n"), "{screen}");
        // HOME is init's.
        assert!(screen.contains("# echo $HOME\n/root\n"), "{screen}");

````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `host_shell::tests::a_session_changes_the_image_and_leaves_it_clean`.

- [ ] **Step 3: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! trying the filesystem and the shell's commands without a machine: the
//! in-process runner runs the command functions (no programs, no `&`). It
//! changes the image in place.

````

with:

````rust
//! trying the filesystem and the shell's commands without a machine: the
//! in-process runner runs the command functions (no programs, no `&`), its
//! environment init's `HOME=/root`. It changes the image in place.

````

Replace:

````rust
    let mut system = HostSystem(log);
    let mut shell = Shell::new(&mut vfs, console, &mut system);
    shell.greet();
````

with:

````rust
    let mut system = HostSystem(log);
    // As init starts /bin/sh (programmable shell gate §8.5).
    let mut shell = Shell::new(&mut vfs, console, &mut system).with_environment(b"HOME=/root\0");
    shell.greet();
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 95 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
feat(xtask): start host-shell with HOME=/root

host-shell's shell started with no environment, so with cd and ~
following HOME it would have no home. It now starts with init's
HOME=/root, as /bin/sh does on Relay OS.
EOF
````


### Task 20: `~` and the prompt follow `HOME`

Decision 9 (spec §8.5, §15 item 7): a `~` at a word's start, or in an assignment's value at its start or after a `:`, was made `/root` as it was parsed; it is now a piece of the word, `Param::Home`, that expands to `$HOME`, or `/root` when `HOME` is unset, as bash reads the password file; it is a quoted piece, so an empty `HOME` makes an empty word, as bash's (`set -- ~; echo $#` gives 1). An assignment of `HOME` before a command is read by its later assignments (`HOME=/x A=~ env` gives `A=/x`, probed). The prompt shows `$HOME` and below as `~` only when `HOME` is set and longer than one byte, as bash's `\w` does (probed: unset, empty, `/` and `/root/` show the whole path), and `greet` sets `PWD` when it goes to `/root`. The red run is the shell's tests: `Param::Home` is missing. Mutation checks (7): `~` always `/root`, an unquoted piece, no overlay, a one-byte `HOME` in the prompt, the prompt falling back to `/root`, `greet` not setting `PWD`, a value's text after `~` lost, each fail a test (the prompt's one-byte case after a `HOME=/` at `/` test was added).

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/vars.rs`
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: Tasks 15 and 19.
- Produces: `parser::Param::Home`; `parser::HOME` is the fallback only; `Vars::get` only for tests.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
    #[test]
    fn an_assignment_s_value_is_one_string() {
````

with:

````rust
    #[test]
    fn a_tilde_is_home_or_root_when_home_is_unset() {
        // As bash 5.2's (programmable shell gate §8.5): without `HOME` it
        // reads the password file, here `/root`.
        let set = Vars::of(&[("HOME", "/h")], &["sh"]);
        let unset = Vars::of(&[], &["sh"]);
        let empty = Vars::of(&[("HOME", "")], &["sh"]);
        assert_eq!(
            words(r#"echo ~ ~/x a~ "~" ~x"#, &set).unwrap(),
            ["echo", "/h", "/h/x", "a~", "~", "~x"]
        );
        assert_eq!(
            words("echo ~ ~/x", &unset).unwrap(),
            ["echo", "/root", "/root/x"]
        );
        // An empty `HOME` makes an empty word, which stays.
        assert_eq!(words("echo ~ ~/x", &empty).unwrap(), ["echo", "", "/x"]);
        let value = |line: &str, v: &Vars| {
            let p = typed(line);
            let (_, value) = p[0].assigns[0].assignment().unwrap();
            super::value(&value, v, 0).unwrap()
        };
        assert_eq!(value("A=~/x:~:a~", &set), "/h/x:/h:a~");
        assert_eq!(value("A=~", &empty), "");
        // An assignment of `HOME` before a command: its later assignments
        // read it, its words the shell's, as bash's.
        let c = expand(&typed("HOME=/x A=~ cmd ~"), &set, 0)
            .unwrap()
            .remove(0);
        assert_eq!(
            (c.words, c.assigns),
            (
                alloc::vec![String::from("cmd"), String::from("/h")],
                alloc::vec![String::from("HOME=/x"), String::from("A=/x")]
            )
        );
    }

    #[test]
    fn an_assignment_s_value_is_one_string() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
                &[text("", true)],
                &[text("/root/x", false)],
            ]
````

with:

````rust
                &[text("", true)],
                &[Piece::Param(Param::Home, true), text("/x", false)],
            ]
````

Replace:

````rust
    fn a_tilde_in_a_value_is_home_at_its_start_or_after_a_colon() {
        // What bash sets for each.
        for (word, value) in [
            ("A=~/x:~/y:~:a~", "/root/x:/root/y:/root:a~"),
            ("A=~", "/root"),
            ("A=x:~", "x:/root"),
            ("A=~x", "~x"),
````

with:

````rust
    fn a_tilde_in_a_value_is_home_at_its_start_or_after_a_colon() {
        // Where bash sets the home directory.
        for (word, value) in [
            ("A=~/x:~/y:~:a~", "<Home>/x:<Home>/y:<Home>:a~"),
            ("A=~", "<Home>"),
            ("A=x:~", "x:<Home>"),
            ("A=~x", "~x"),
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let mut h = Harness::new();
        let prompt =
            |h: &mut Harness| Shell::new(&mut h.vfs, &mut h.console, &mut h.system).prompt();
        assert_eq!(prompt(&mut h), "root@relay:/# ");
````

with:

````rust
        let mut h = Harness::new();
        let prompt = |h: &mut Harness| {
            Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
                .with_environment(&h.env)
                .prompt()
        };
        h.env = b"HOME=/root\0".to_vec();
        assert_eq!(prompt(&mut h), "root@relay:/# ");
````

Replace:

````rust
        assert_eq!(prompt(&mut h), "root@relay:/rootless# ");
    }
````

with:

````rust
        assert_eq!(prompt(&mut h), "root@relay:/rootless# ");
        // As bash's `\w`: `HOME` unset, empty, `/` or with a `/` at its end
        // shows none.
        h.run("cd /root/notes");
        for env in [&b""[..], b"HOME=\0", b"HOME=/\0", b"HOME=/root/\0"] {
            h.env = env.to_vec();
            assert_eq!(prompt(&mut h), "root@relay:/root/notes# ", "{env:?}");
        }
        h.env = b"HOME=/root/notes\0".to_vec();
        assert_eq!(prompt(&mut h), "root@relay:~# ");
        h.env = b"HOME=/\0".to_vec();
        h.run("cd /");
        assert_eq!(prompt(&mut h), "root@relay:/# ", "bash's `\\w` for HOME=/");
    }
````

Replace:

````rust
        let mut h = Harness::new();
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).greet();
        assert_eq!(h.console.take(), "Welcome to Relay OS.\n");
        assert_eq!(h.run("pwd").1, "/root\n");
````

with:

````rust
        let mut h = Harness::new();
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system).with_environment(b"");
        shell.greet();
        // `PWD` follows it there.
        shell.execute("echo $PWD");
        assert_eq!(h.console.take(), "Welcome to Relay OS.\n/root\n");
        assert_eq!(h.run("pwd").1, "/root\n");
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/environment.txt`**

In `tests/e2e/environment.txt`, replace:

````text
expect \nrelay-sh: cd: -e: not supported\nrelay-sh: cd: -x: invalid option\n2\nroot@relay:~# $
````

with:

````text
expect \nrelay-sh: cd: -e: not supported\nrelay-sh: cd: -x: invalid option\n2\nroot@relay:~# $
# ~ and the prompt follow HOME; with HOME unset ~ is /root and the prompt
# shows the whole path, as bash's do.
send HOME=/tmp; echo ~ ~/x; cd
expect \n/tmp /tmp/x\nroot@relay:~# $
send pwd; HOME=/root; cd
expect \n/tmp\nroot@relay:~# $
send unset HOME; echo ~
expect \n/root\nroot@relay:/root# $
send export HOME=/root
expect root@relay:~# $
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `Home` found for enum `parser::Param` in the current scope ``.

- [ ] **Step 6: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

use crate::parser::{Command, Param, Piece, Redirect, RedirectOp, Word};
pub(crate) use crate::vars::Vars;
````

with:

````rust

use crate::parser::{Command, HOME, Param, Piece, Redirect, RedirectOp, Word};
pub(crate) use crate::vars::Vars;
````

Replace:

````rust

    /// A parameter's value, borrowed from the variables where it is
````

with:

````rust

    /// The variable `name`'s value, if it has one: an assignment of the
    /// command's made so far, or the shell's.
    fn variable(&self, name: &str) -> Option<Cow<'v, str>> {
        match self.made.iter().find(|(m, _)| m == name) {
            Some((_, value)) => Some(Cow::Owned(value.clone())),
            None => self.vars.value(name).map(Cow::Borrowed),
        }
    }

    /// A parameter's value, borrowed from the variables where it is
````

Replace:

````rust
        Ok(match p {
            Param::Name(n) => match self.made.iter().find(|(m, _)| m == n) {
                Some((_, value)) => Value::One(Cow::Owned(value.clone())),
                None => Value::One(Cow::Borrowed(self.vars.get(n))),
            },
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
````

with:

````rust
        Ok(match p {
            Param::Name(n) => Value::One(self.variable(n).unwrap_or(Cow::Borrowed(""))),
            Param::Home => Value::One(self.variable("HOME").unwrap_or(Cow::Borrowed(HOME))),
            Param::Arg(i) => Value::One(Cow::Borrowed(args.get(*i).map_or("", String::as_str))),
````

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 8 replacements, top to bottom:

Replace:

````rust

/// The home directory `~` stands for.
pub const HOME: &str = "/root";
````

with:

````rust

/// The home directory `~` stands for when `HOME` is unset, as bash takes
/// it from the password file (programmable shell gate §8.5).
pub const HOME: &str = "/root";
````

Replace:

````rust
    Bad(String),
}
````

with:

````rust
    Bad(String),
    /// A `~` that stands for the home directory: `$HOME`, or [`HOME`] when
    /// it is unset. It is a quoted piece, as bash's result is, so an empty
    /// `HOME` makes an empty word.
    Home,
}
````

Replace:

````rust
    /// unquoted (`"A"=x` is none, as in bash): the name, and the value as a
    /// word of its own, a `~` at its start or after a `:` made `/root`, as
    /// bash's is.
    pub fn assignment(&self) -> Option<(&str, Word)> {
````

with:

````rust
    /// unquoted (`"A"=x` is none, as in bash): the name, and the value as a
    /// word of its own, a `~` at its start or after a `:` standing for the
    /// home directory, as bash's does.
    pub fn assignment(&self) -> Option<(&str, Word)> {
````

Replace:

````rust
        let count = pieces.len();
        for (i, piece) in pieces.iter_mut().enumerate() {
            if let Piece::Text(text, false) = piece {
                *text = value_tildes(text, i == 0, i + 1 == count);
            }
        }
        let typed = String::from(self.typed.get(name.len() + 1..).unwrap_or(""));
````

with:

````rust
        let count = pieces.len();
        let pieces = pieces
            .into_iter()
            .enumerate()
            .flat_map(|(i, piece)| match piece {
                Piece::Text(text, false) => value_tildes(&text, i == 0, i + 1 == count),
                piece => alloc::vec![piece],
            })
            .collect();
        let typed = String::from(self.typed.get(name.len() + 1..).unwrap_or(""));
````

Replace:

````rust

/// An unquoted piece of an assignment's value with each `~` made `/root`
/// that is at the value's start (`first`) or after a `:`, and before a `/`,
/// a `:` or the value's end (`last`).
fn value_tildes(text: &str, first: bool, last: bool) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
````

with:

````rust

/// An unquoted piece of an assignment's value as pieces, each `~` that is
/// at the value's start (`first`) or after a `:`, and before a `/`, a `:`
/// or the value's end (`last`), the home directory.
fn value_tildes(text: &str, first: bool, last: bool) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut chars = text.chars().peekable();
````

Replace:

````rust
        if c == '~' && after_colon && ends {
            out.push_str(HOME);
        } else {
            out.push(c);
        }
        after_colon = c == ':';
    }
````

with:

````rust
        if c == '~' && after_colon && ends {
            if !run.is_empty() {
                out.push(Piece::Text(core::mem::take(&mut run), false));
            }
            out.push(Piece::Param(Param::Home, true));
        } else {
            run.push(c);
        }
        after_colon = c == ':';
    }
    if !run.is_empty() || out.is_empty() {
        out.push(Piece::Text(run, false));
    }
````

Replace:

````rust
    /// The word, its `~` (alone, or before a `/` in the same unquoted
    /// piece) made `/root`, as bash's is (`~"/x"` and `~$A` keep it).
    fn finish(self, line: &str) -> Option<Word> {
````

with:

````rust
    /// The word, its `~` (alone, or before a `/` in the same unquoted
    /// piece) the home directory, as bash's is (`~"/x"` and `~$A` keep it).
    fn finish(self, line: &str) -> Option<Word> {
````

Replace:

````rust
        {
            first.replace_range(..1, HOME);
        }
````

with:

````rust
        {
            first.remove(0);
            if first.is_empty() {
                word.pieces.remove(0);
            }
            word.pieces.insert(0, Piece::Param(Param::Home, true));
        }
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// `root@relay:<cwd># `, with `/root` shown as `~`.
    pub fn prompt(&self) -> String {
        let cwd = path::display(&self.vfs.cwd());
        let dir = match cwd.strip_prefix(HOME) {
            Some("") => String::from("~"),
````

with:

````rust

    /// `root@relay:<cwd># `, `$HOME` and below shown as `~`, as bash's `\w`
    /// shows them: only when `HOME` is set and longer than one byte
    /// (programmable shell gate §15 item 7).
    pub fn prompt(&self) -> String {
        let cwd = path::display(&self.vfs.cwd());
        let home = self.vars.value("HOME").filter(|h| h.len() > 1);
        let dir = match home.and_then(|h| cwd.strip_prefix(h)) {
            Some("") => String::from("~"),
````

Replace:

````rust
        // Without a /root the shell starts in /.
        let _ = self.vfs.chdir(HOME.as_bytes());
    }
````

with:

````rust
        // Without a /root the shell starts in /.
        if self.vfs.chdir(HOME.as_bytes()).is_ok() {
            let _ = self.vars.set("PWD", String::from(HOME));
        }
    }
````

- [ ] **Step 9: Change `crates/shell/src/vars.rs`**

In `crates/shell/src/vars.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        self.args.get(1..).unwrap_or(&[])
    }

    /// The variable `name`'s value; an unset one, or one without a value,
    /// is empty.
    pub fn get(&self, name: &str) -> &str {
        self.value(name).unwrap_or("")
    }
````

with:

````rust
        self.args.get(1..).unwrap_or(&[])
    }
````

Replace:

````rust
impl Vars {
    /// `names` set, and `args` (`$0` first).
````

with:

````rust
impl Vars {
    /// The variable `name`'s value; an unset one, or one without a value,
    /// is empty.
    pub fn get(&self, name: &str) -> &str {
        self.value(name).unwrap_or("")
    }

    /// `names` set, and `args` (`$0` first).
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 453 tests.

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 95 tests.

- [ ] **Step 12: Run the `environment` scenario**

Run: `cargo xtask test --e2e-only --scenario environment`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,e2e): make ~ and the prompt follow HOME

A ~ at a word's start, or in an assignment's value at its start or
after a :, was made /root as it was parsed. It is now a piece of the
word that expands to $HOME, or /root when HOME is unset, as bash reads
the password file; an empty HOME makes an empty word, as bash's. The
prompt shows $HOME and below as ~ only when HOME is set and longer than
one byte, as bash's \w does, and greet (host-shell) sets PWD when it
goes to /root.
EOF
````


### Task 21: A script's `cd` stays in the script

Decision 10 (spec §9.2): under `/bin/sh` a script is a shell process of its own, so its `cd` never reached the shell that ran it; the in-process runner (`host-shell` and the tests) ran it in the same shell and stayed wherever the script went. It now goes back to the directory it ran the script from. The red run is the shell's tests: `pwd` after the script shows its directory. Mutation check (1): not going back fails two tests.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: the in-process runner's `run_script`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
        );
        assert_eq!(h.run("pwd").1, "/etc\n");
    }
````

with:

````rust
        );
        // The script's cd stays in it, as under /bin/sh.
        assert_eq!(h.run("pwd").1, "/tmp\n");
    }
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_script_starts_with_the_exported_variables() {
````

with:

````rust
    #[test]
    fn a_script_s_cd_stays_in_it() {
        // Under /bin/sh a script is a process of its own; the in-process
        // runner goes back where it was (programmable shell gate §9.2).
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cd /etc\npwd\n");
        assert_eq!(
            h.lines(&["cd /tmp", "sh s.sh", "pwd", "echo $PWD"]).1,
            "+ cd /etc\n+ pwd\n/etc\n/tmp\n/tmp\n"
        );
    }

    #[test]
    fn a_script_starts_with_the_exported_variables() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::script::tests::relative_paths_and_cd_work_as_typed`, `shell::tests::a_script_s_cd_stays_in_it`.

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// its own, the exported variables imported, and starts with `$?` 0, as
    /// there.
    fn run_script(&mut self, script: Script, env: &[u8]) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
````

with:

````rust
    /// its own, the exported variables imported, and starts with `$?` 0, as
    /// there; and its `cd` stays in it.
    fn run_script(&mut self, script: Script, env: &[u8]) -> i32 {
        let cwd = self.vfs.cwd();
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
````

Replace:

````rust
        self.vars = outer;
        if self.exited {
````

with:

````rust
        self.vars = outer;
        let _ = self.vfs.chdir(&cwd);
        if self.exited {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 454 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): keep a script's cd in the script

Under /bin/sh a script is a shell process of its own, so its cd never
reached the shell that ran it; the in-process runner (host-shell and
the tests) ran it in the same shell and stayed wherever the script
went. It now goes back to the directory it ran the script from, so a
check script's cd /root stays in it under both.
EOF
````


### Task 22: Every check script starts with `cd /root`

Decision 1 (spec §9.2): a check script's paths were relative to the directory it was run from. Each now starts with `cd /root`, which stays in the script, so it passes from any directory, and every transcript starts with `+ cd /root`: the NUC transcripts by hand until milestone 5's NUC run, by one command, as two of them hold escape bytes. The `checks` scenario runs `check3-a.sh` from `/root/checks`. The red run is xtask's tests: the transcripts lack the line the scripts now print (the spike's first failure). Tests and transcripts only.

**Files:**
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `rootfs/root/checks/check3-b.sh`
- Modify: `rootfs/root/checks/check4.sh`
- Modify: `rootfs/root/checks/check5.sh`
- Modify: `rootfs/root/checks/check6.sh`
- Modify: `tests/e2e/checks.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`
- Modify: `xtask/fixtures/checks/check3-b.qemu.log`
- Modify: `xtask/fixtures/checks/check4.nuc.log`
- Modify: `xtask/fixtures/checks/check4.qemu.log`
- Modify: `xtask/fixtures/checks/check5.nuc.log`
- Modify: `xtask/fixtures/checks/check5.qemu.log`
- Modify: `xtask/fixtures/checks/check6.nuc.log`
- Modify: `xtask/fixtures/checks/check6.qemu.log`

**Interfaces:**
- Consumes: Task 21 (a script's `cd` stays in it).
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/checks.txt`**

In `tests/e2e/checks.txt`, replace:

````text
# The NUC checks' scripts (rootfs/root/checks/, spec §15 item 12) run in
# QEMU as they do on the NUC: check 3's part 1, a reboot, part 2, then
# checks 4, 5 and 6. Afterwards their transcripts on the disk must show
# what the scripts expect.
timeout 30
expect root@relay:~# $
send sh checks/check3-a.sh
expect \n\+ rm -rf /root/notes\n
timeout 120
expect \n\+ df\n
expect root@relay:~# $
````

with:

````text
# The NUC checks' scripts (rootfs/root/checks/, spec §15 item 12) run in
# QEMU as they do on the NUC: check 3's part 1, from /root/checks as each
# script starts with cd /root (programmable shell gate §9.2), a reboot,
# part 2, then checks 4, 5 and 6. Afterwards their transcripts on the disk
# must show what the scripts expect.
timeout 30
expect root@relay:~# $
send cd checks
expect root@relay:~/checks# $
send sh check3-a.sh
expect \n\+ cd /root\n\+ rm -rf /root/notes\n
timeout 120
expect \n\+ df\n
expect root@relay:~/checks# $
send cd
expect root@relay:~# $
````

- [ ] **Step 2: Add `+ cd /root` to every recorded transcript**

Run:

````bash
for f in xtask/fixtures/checks/*.log; do sed -i '1i + cd /root' "$f"; done
head -n 1 xtask/fixtures/checks/*.log | /usr/bin/grep -c '^+ cd /root$'
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 4: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
# must print nothing.

````

with:

````bash
# must print nothing.

# Run from any directory (programmable shell gate §9.2).
cd /root

````

- [ ] **Step 5: Change `rootfs/root/checks/check3-b.sh`**

In `rootfs/root/checks/check3-b.sh`, replace:

````bash
# check3-b.log; the `#>` lines are explained in check3-a.sh.

````

with:

````bash
# check3-b.log; the `#>` lines are explained in check3-a.sh.

# Run from any directory (programmable shell gate §9.2).
cd /root

````

- [ ] **Step 6: Change `rootfs/root/checks/check4.sh`**

In `rootfs/root/checks/check4.sh`, replace:

````bash
# the first time.

````

with:

````bash
# the first time.

# Run from any directory (programmable shell gate §9.2).
cd /root

````

- [ ] **Step 7: Change `rootfs/root/checks/check5.sh`**

In `rootfs/root/checks/check5.sh`, replace:

````bash
# differs between the NUC and QEMU.

````

with:

````bash
# differs between the NUC and QEMU.

# Run from any directory (programmable shell gate §9.2).
cd /root

````

- [ ] **Step 8: Change `rootfs/root/checks/check6.sh`**

In `rootfs/root/checks/check6.sh`, replace:

````bash
# between the NUC and QEMU.

````

with:

````bash
# between the NUC and QEMU.

# Run from any directory (programmable shell gate §9.2).
cd /root

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 95 tests.

- [ ] **Step 10: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add rootfs tests xtask
git commit -F - <<'EOF'
test(checks,e2e): start every check script with cd /root

A check script's paths were relative to the directory it was run from.
Each now starts with cd /root, which stays in the script, so it passes
from any directory, and every transcript starts with + cd /root; the
NUC transcripts get the line by hand until milestone 5's NUC run. The
checks scenario runs check3-a.sh from /root/checks.
EOF
````


### Task 23: The bash corpus: `export`, `unset`, `A=1 cmd`, `cd` and `cd -`

Decision 1 (spec §11.2): a corpus script exports, unsets and assigns before commands, and goes into a directory and back with `cd` and `cd -`, printing only what follows from the directory, which differs between the two shells (`target/corpus/…` and `/tmp/corpus`), `OLDPWD` after `cd ""` among it (the prototype's review). The in-process runner now starts with the environment bash gets (`LC_ALL=C`, `PATH`). A nested `sh` cannot be run by the corpus (the in-process `sh` prints its trace), so export into one stays with the unit tests and the scenario. Tests only: no failing run. Mutation check (1): the in-process runner without bash's environment fails the corpus.

**Files:**
- Modify: `crates/shell/src/corpus.rs`
- Create: `crates/shell/tests/corpus/environment.sh`

**Interfaces:**
- Consumes: Tasks 2–21.
- Produces: `crates/shell/tests/corpus/environment.sh`.

- [ ] **Step 1: Change `crates/shell/src/corpus.rs`**

In `crates/shell/src/corpus.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

/// What bash 5.2 prints for `script` and its status, in the C locale with
````

with:

````rust

/// The environment both shells start with.
const ENVIRONMENT: &[u8] = b"LC_ALL=C\0PATH=/usr/bin:/bin\0";

/// What bash 5.2 prints for `script` and its status, in the C locale with
````

Replace:

````rust
/// What the in-process runner prints for `text` and its status, in an
/// empty directory.
fn relay(text: &[u8]) -> (i32, String) {
````

with:

````rust
/// What the in-process runner prints for `text` and its status, in an
/// empty directory, with bash's environment.
fn relay(text: &[u8]) -> (i32, String) {
````

Replace:

````rust
    let mut input = Bytes::new(text.to_vec());
    let status = Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run_input(&mut input);
    (status, h.console.take())
````

with:

````rust
    let mut input = Bytes::new(text.to_vec());
    let status = Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
        .with_environment(ENVIRONMENT)
        .run_input(&mut input);
    (status, h.console.take())
````

- [ ] **Step 2: Create `crates/shell/tests/corpus/environment.sh`**

Create `crates/shell/tests/corpus/environment.sh`:

````bash
# The shell's environment (milestone 5's plan 3, programmable shell gate
# §8.5, §9.1): export, unset, assignments before a command, cd and cd -,
# as bash 5.2 runs them. The directory differs, so only what follows
# from it is printed.
A=1
export A B
B=2
export C=3 D
unset A
echo "[$A][$B][$C][$D]"
X=1 true
echo "[$X]"
C=4 true
echo "$C"
E=5 $UNSET
echo "$E"
start=$PWD
mkdir d
cd d
test "$OLDPWD" = "$start" && echo oldpwd
test "$PWD" = "$start/d" && echo pwd
cd - > ../out
test "$PWD" = "$start" && echo back
wc -l < out
cd ""
test "$OLDPWD" = "$start" && echo oldpwd-again
test "$PWD" = "$start" && echo stays
OLDPWD=
cd - > out2
wc -c < out2
cd d/..
test "$PWD" = "$start" && echo canonical
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 454 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): compare export, unset, A=1 cmd and cd with bash

A corpus script exports, unsets and assigns before commands, and goes
into a directory and back with cd and cd -, printing only what follows
from the directory, which differs between the two shells. The
in-process runner now starts with the environment bash gets.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p3/cd
gh pr create --base main --head m5p3/cd --title "feat(shell): follow HOME, PWD and OLDPWD in cd and ~" --body-file - <<'EOF'
## What

Milestone 5, plan 3, tasks 14–23: a shell exports `PWD` from `getcwd` when it starts and `OLDPWD` without a value unless it imported a directory; `cd [-L|-P] [--] [dir]` goes to `$HOME` or, with `-`, to `$OLDPWD` (printed as written), and sets `OLDPWD` and `PWD` as bash 5.2's `cd` does, saying so when the variables cannot hold them; `~` expands to `$HOME` (`/root` when unset, an empty word when empty) and the prompt shows `~` as bash's `\w` does; `host-shell` starts with `HOME=/root`; a script's `cd` stays in it under the in-process runner too; every check script starts with `cd /root` and every transcript with `+ cd /root` (the NUC's by hand), check 3-a run from `/root/checks`; a corpus script compares `export`, `unset`, `A=1 cmd`, `cd` and `cd -` with bash.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4; the NUC transcripts get their `+ cd /root` line by hand until plan 4's run
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-cd
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: `/bin/env` (Tasks 24–31)

`env` as GNU's, in the shell and as the 46th program of `/bin`; the counts, README, AGENTS.md and `docs/hardware-test.md`.

Branch `m5p3/env`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-env`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p3/env /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-env origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-env
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p3/cd` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p3/env /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-env m5p3/cd`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p3/cd>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 24: `env`: print the environment, or run a command with a changed one

Decision 11 (spec §8.6): `env` is GNU coreutils 9.4's without `PATH`, compared line for line with GNU's: it starts from the environment its command was given (`Ctx::environment`), or none with `-i` or `-`, removes each `-u NAME`, sets each `NAME=value` in its place or after the rest, as GNU's `putenv` does, then prints it, or runs a command with it, its status the command's. Its options stop at the first word that is not one; `-u A=B` and `-u ''` are GNU's `cannot unset`, status 125; GNU's other options and long options are refused with the first line of GNU's message for an option it does not know. A command not found is GNU's `No such file or directory`, 127; one that cannot run 126, a directory `Permission denied`, as Linux's `execve` says. In the in-process runner the command's function runs with the new block on the same `Ctx` (a built-in is no program, 127); a script `sh` reads keeps the environment `sh` was given (`Script::environment`), which `run_script` imports. Programs start in Task 26. `help` lists `env`. The red run is the shell's tests: `env` is missing. Mutation checks (14): `-i`, `-`, `-u` ignored, appending always, `-u A=B` allowed, options read past an operand, built-ins runnable, the block not swapped in, a directory's own error, 126 for not found, a killed command's raw code, `Script::environment` from the shell, a pipeline stage's and the last stage's environment, each fail a test.

**Files:**
- Create: `crates/shell/src/commands/env.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 10's environments.
- Produces: `commands::env`, `Ctx::{environment, programs}`, `Script::environment`, `Harness::program_env`.

- [ ] **Step 1: Write the failing tests for `crates/shell/src/commands/env.rs`**

Create `crates/shell/src/commands/env.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::{FakeStdout, Harness, host_tool};
    use alloc::string::String;

    /// `env` run with `args` and the environment `env`, as a program of
    /// the in-process runner: its status, output and errors.
    fn env(args: &[&str], env: &[u8]) -> (i32, String, String) {
        let mut h = Harness::new();
        let mut out = FakeStdout::file(None);
        let mut words = alloc::vec!["env"];
        words.extend_from_slice(args);
        let (status, errors) = h.program_env(&words, env, &mut out);
        (status, out.text(), errors)
    }

    /// GNU's `env` with `args`, started with the environment `env`; its
    /// errors' first line, as a refused option prints only that.
    fn gnu(args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
        let mut words = alloc::vec!["env", "-i"];
        let sets: alloc::vec::Vec<String> =
            env.iter().map(|(n, v)| alloc::format!("{n}={v}")).collect();
        words.extend(sets.iter().map(String::as_str));
        words.push("env");
        words.extend_from_slice(args);
        let (status, out, err) = host_tool(&words, &[], b"");
        let first = err
            .lines()
            .next()
            .map(|l| alloc::format!("{l}\n"))
            .unwrap_or_default();
        (status, out, first)
    }

    const BLOCK: &[u8] = b"HOME=/root\0PWD=/\0A=1\0";
    const PAIRS: &[(&str, &str)] = &[("HOME", "/root"), ("PWD", "/"), ("A", "1")];

    #[test]
    fn env_prints_and_changes_its_environment_as_gnu_s_does() {
        for args in [
            &[][..],
            &["-i"],
            &["-i", "B=2", "C=x y"],
            &["-u", "A"],
            &["-uA", "-u", "NOPE"],
            &["-u", "HOME", "HOME=/x"],
            &["HOME=/x", "B=2"],
            &["B=1", "B=2"],
            &["=x"],
            &["a/b=c"],
            &["--", "B=2"],
            &["-", "B=2"],
            &["-i", "-", "B=2"],
            &["-iu", "A"],
            &["C=été"],
        ] {
            assert_eq!(env(args, BLOCK), gnu(args, PAIRS), "{args:?}");
        }
    }

    #[test]
    fn env_refuses_what_gnu_s_refuses() {
        for args in [&["-x"][..], &["-u"], &["--x"], &["-u", "A=B"], &["-u", ""]] {
            let (status, out, err) = env(args, BLOCK);
            assert_eq!((status, out, err), gnu(args, PAIRS), "{args:?}");
            assert_eq!(status, 125, "{args:?}");
        }
    }

    #[test]
    fn env_refuses_gnu_s_other_options() {
        // GNU's `-0`, `-C`, `-S`, `-v` and long options, refused with the
        // first line of GNU's message for an option it does not know, as
        // every command here refuses an option it does not have.
        for (args, said) in [
            (&["-0"][..], "env: invalid option -- '0'\n"),
            (&["-C", "/"], "env: invalid option -- 'C'\n"),
            (&["-S", "A=1"], "env: invalid option -- 'S'\n"),
            (&["-v"], "env: invalid option -- 'v'\n"),
            (&["--unset=A"], "env: unrecognized option '--unset=A'\n"),
            (
                &["--ignore-environment"],
                "env: unrecognized option '--ignore-environment'\n",
            ),
        ] {
            assert_eq!(
                env(args, BLOCK),
                (125, String::new(), said.into()),
                "{args:?}"
            );
        }
    }

    #[test]
    fn env_prints_what_the_shell_gave_it() {
        // The exported variables, with the assignments before it, in the
        // order of export; in a pipeline too; a script it runs imports
        // what it changed.
        let mut h = Harness::new();
        h.env = b"HOME=/root\0".to_vec();
        h.put("/tmp/s.sh", b"echo [$A][$HOME]\n");
        assert_eq!(
            h.lines(&[
                "export B=2",
                "C=3",
                "A=1 env",
                "env | grep -c =",
                "env -u HOME A=1 sh /tmp/s.sh",
            ])
            .1,
            "HOME=/root\nPWD=/\nB=2\nA=1\n3\n+ echo [$A][$HOME]\n[1][]\n"
        );
    }

    #[test]
    fn env_runs_a_command_with_its_environment() {
        // In the in-process runner, the command's function.
        assert_eq!(
            env(&["-i", "A=1", "env"], BLOCK),
            (0, "A=1\n".into(), String::new())
        );
        assert_eq!(
            env(&["-u", "HOME", "echo", "hi", "-u"], BLOCK),
            (0, "hi -u\n".into(), String::new())
        );
        assert_eq!(env(&["false"], BLOCK).0, 1);
        // Options stop at the first word that is not one.
        assert_eq!(env(&["A=2", "-i"], BLOCK), gnu(&["A=2", "-i"], PAIRS));
        assert_eq!(
            env(&["nope"], BLOCK),
            (
                127,
                String::new(),
                "env: 'nope': No such file or directory\n".into()
            )
        );
        // The shell's own commands are not programs.
        assert_eq!(env(&["cd"], BLOCK).0, 127);
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
mod control;
mod export;
````

with:

````rust
mod control;
mod env;
mod export;
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust

    /// Creates (or replaces) a file.
````

with:

````rust

    /// As [`Harness::program_args`], the command given the environment
    /// `env`.
    pub fn program_env(
        &mut self,
        args: &[&str],
        env: &[u8],
        stdout: &mut FakeStdout,
    ) -> (i32, String) {
        let words: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let mut ctx =
            crate::Ctx::program(&mut self.vfs, &mut self.system, &mut self.console, stdout);
        ctx.set_input(&mut input);
        ctx.environment = env.to_vec();
        let run = crate::commands::find(&words[0]).unwrap().run;
        let mut status = run(&mut ctx, &words[1..]);
        if ctx.finish().is_err() {
            status = ctx.write_error_status;
        }
        (status, self.console.take())
    }

    /// Creates (or replaces) a file.
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no field `environment` on type `Ctx<'_>` ``.

- [ ] **Step 5: Implement `crates/shell/src/commands/env.rs`**

Insert this at the top of `crates/shell/src/commands/env.rs`, above `#[cfg(test)]`:

````rust
//! `env`, GNU coreutils 9.4's without `PATH` (programmable shell gate
//! §8.6): it prints its environment, or runs a command with a changed one.

use super::BUILTINS;
use crate::ctx::{Ctx, OptError, quote};
use crate::io::Group;
use crate::killed;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use vfs::Errno;

/// GNU's status when `env` itself fails.
const FAILED: i32 = 125;
/// GNU's status for a command that cannot run, and one not found.
const CANNOT_RUN: i32 = 126;
const NOT_FOUND: i32 = 127;

/// `env [-i] [-u NAME]... [-] [NAME=value]... [COMMAND [ARG]...]`: starts
/// from the environment its command was given, or none (`-i`, `-`),
/// removes each `-u NAME`, sets each `NAME=value` (in its place, or after
/// the rest, as GNU's `putenv` does), then prints it, one entry a line,
/// or runs COMMAND with it: `/bin/COMMAND`, or the path as given, waiting
/// for it, its status the command's. Options stop at the first word that
/// is not one, as GNU's do.
pub fn env(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let mut clear = false;
    let mut unset: Vec<&str> = Vec::new();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        i += 1;
        if arg == "--" {
            break;
        }
        if arg.starts_with("--") {
            return refuse(ctx, OptError::Unrecognized(arg.clone()));
        }
        let Some(letters) = arg.strip_prefix('-').filter(|l| !l.is_empty()) else {
            i -= 1;
            break;
        };
        for (at, c) in letters.char_indices() {
            match c {
                'i' => clear = true,
                'u' => {
                    let rest = &letters[at + 1..];
                    let name = if !rest.is_empty() {
                        rest
                    } else if let Some(next) = args.get(i) {
                        i += 1;
                        next.as_str()
                    } else {
                        return refuse(ctx, OptError::MissingValue('u'));
                    };
                    unset.push(name);
                    break;
                }
                c => return refuse(ctx, OptError::Invalid(c)),
            }
        }
    }
    if args.get(i).is_some_and(|a| a == "-") {
        clear = true;
        i += 1;
    }
    let mut entries: Vec<Vec<u8>> = if clear {
        Vec::new()
    } else {
        ctx.environment
            .split(|&b| b == 0)
            .filter(|e| !e.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    for name in unset {
        if name.is_empty() || name.contains('=') {
            let message = format!("cannot unset {}: Invalid argument", quote(name));
            ctx.fail("env", format_args!("{message}"));
            return FAILED;
        }
        entries.retain(|e| name_of(e) != name.as_bytes());
    }
    while let Some(set) = args.get(i).filter(|a| a.contains('=')) {
        i += 1;
        let set = set.as_bytes();
        match entries.iter_mut().find(|e| name_of(e) == name_of(set)) {
            Some(e) => *e = set.to_vec(),
            None => entries.push(set.to_vec()),
        }
    }
    let Some((name, args)) = args[i..].split_first() else {
        for e in entries {
            ctx.out(&e);
            ctx.out(b"\n");
        }
        return 0;
    };
    let mut block = Vec::new();
    for e in entries {
        block.extend_from_slice(&e);
        block.push(0);
    }
    run(ctx, name, args, block)
}

/// An entry's name: what comes before its first `=`.
fn name_of(entry: &[u8]) -> &[u8] {
    entry.split(|&b| b == b'=').next().unwrap_or(entry)
}

/// The first line of GNU's message for a refused option, status 125.
fn refuse(ctx: &mut Ctx<'_>, e: OptError) -> i32 {
    ctx.fail("env", format_args!("{e}"));
    FAILED
}

/// Runs `name` with `args` and the environment `block`: as a program,
/// started through the program's `Programs` and waited for; in the
/// in-process runner, which has none, the command's function, with the
/// block for its environment.
fn run(ctx: &mut Ctx<'_>, name: &str, args: &[String], block: Vec<u8>) -> i32 {
    let Some(programs) = ctx.programs.as_deref_mut() else {
        let Some(command) = super::find(name).filter(|c| !BUILTINS.contains(&c.name)) else {
            return cannot_run(ctx, name, Errno::ENOENT);
        };
        let outer = core::mem::replace(&mut ctx.environment, block);
        let status = (command.run)(ctx, args);
        ctx.environment = outer;
        return status;
    };
    let path = if name.contains('/') {
        String::from(name)
    } else {
        format!("/bin/{name}")
    };
    let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
    argv.extend(args.iter().map(|a| a.as_bytes()));
    let waited = programs
        .spawn(path.as_bytes(), &argv, &block, [0, 1, 2], Group::Shell)
        .and_then(|pid| programs.wait(pid));
    match waited {
        Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
        Ok(w) => killed::killed(&w).1,
        Err(e) => cannot_run(ctx, name, e),
    }
}

/// GNU's message for a command that could not run: 127 if it is not
/// there, 126 otherwise; a directory is `Permission denied`, as Linux's
/// `execve` says (`EACCES`, which `vfs` lacks).
fn cannot_run(ctx: &mut Ctx<'_>, name: &str, e: Errno) -> i32 {
    let why = match e {
        Errno::EISDIR => String::from("Permission denied"),
        e => e.to_string(),
    };
    ctx.fail("env", format_args!("{}: {why}", quote(name)));
    if e == Errno::ENOENT {
        NOT_FOUND
    } else {
        CANNOT_RUN
    }
}

````

- [ ] **Step 6: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub use control::{jobs, kill, wait};
pub use export::{export, unset};
````

with:

````rust
pub use control::{jobs, kill, wait};
pub use env::env;
pub use export::{export, unset};
````

Replace:

````rust
        run: basic::echo,
    },
````

with:

````rust
        run: basic::echo,
    },
    Builtin {
        name: "env",
        help: "print the environment, or run a command with a changed one",
        run: env::env,
    },
````

- [ ] **Step 7: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub transcript_name: String,
}
````

with:

````rust
    pub transcript_name: String,
    /// The environment `sh` was given, which the script imports.
    pub environment: Vec<u8>,
}
````

Replace:

````rust
                transcript_name: log,
            });
````

with:

````rust
                transcript_name: log,
                environment: ctx.environment.clone(),
            });
````

- [ ] **Step 8: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub(crate) control: Option<JobControl<'a>>,
}
````

with:

````rust
    pub(crate) control: Option<JobControl<'a>>,
    /// The environment the command was given (programmable shell gate
    /// §8.5): entries, each followed by a NUL.
    pub(crate) environment: Vec<u8>,
    /// What starts programs, for `env` run as a program; none in the
    /// in-process runner.
    pub(crate) programs: Option<&'a mut dyn Programs>,
}
````

Replace:

````rust
            control: None,
        }
````

with:

````rust
            control: None,
            environment: Vec::new(),
            programs: None,
        }
````

- [ ] **Step 9: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
                }
                ctx.transcript = transcript.take();
````

with:

````rust
                }
                ctx.environment = stage.env.to_vec();
                ctx.transcript = transcript.take();
````

Replace:

````rust
    ctx.control = control;
    if let Some(input) = parts.input {
````

with:

````rust
    ctx.control = control;
    ctx.environment = parts.env.to_vec();
    if let Some(input) = parts.input {
````

- [ ] **Step 10: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script, &env);
            self.fds = outer;
````

with:

````rust
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script);
            self.fds = outer;
````

Replace:

````rust
    /// there; and its `cd` stays in it.
    fn run_script(&mut self, script: Script, env: &[u8]) -> i32 {
        let cwd = self.vfs.cwd();
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(env);
        let started = start_variables(&mut vars, &mut *self.vfs);
````

with:

````rust
    /// there; and its `cd` stays in it.
    fn run_script(&mut self, script: Script) -> i32 {
        let cwd = self.vfs.cwd();
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut vars = Vars::script(&script.name, &script.args);
        vars.import(&script.environment);
        let started = start_variables(&mut vars, &mut *self.vfs);
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 459 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add env

env is GNU coreutils 9.4's without PATH: it starts from the environment
its command was given, or none with -i or -, removes each -u NAME, sets
each NAME=value in its place or after the rest, as GNU's putenv does,
then prints it, or runs a command with it, its status the command's.
Its options stop at the first word that is not one; GNU's other options
are refused. A command function now gets its environment, and a script
sh reads keeps the one sh was given. The in-process runner runs the
command's function; programs start in the next commits.
EOF
````


### Task 25: The in-process `env` holds its command to 64 KiB

Decision 11 (the prototype's review, m-1): `/bin/env`'s `spawn` refuses an environment over 64 KiB (`E2BIG`), but the in-process runner's `env` ran its command's function with any size of block. It now says `env: 'COMMAND': Argument list too long`, status 126, as `/bin/env` does. The red run is the shell's tests. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/env.rs`

**Interfaces:**
- Consumes: Task 24; Task 12's `ENVIRONMENT_MAX`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/env.rs`**

In `crates/shell/src/commands/env.rs`, replace:

````rust
    #[test]
    fn env_runs_a_command_with_its_environment() {
````

with:

````rust
    #[test]
    fn env_s_command_gets_at_most_64_kib_in_the_in_process_runner_too() {
        // As `spawn` refuses one more byte (`E2BIG`) for `/bin/env`.
        let fits = alloc::format!("A={}", "x".repeat(crate::vars::ENVIRONMENT_MAX - 3));
        let over = alloc::format!("A={}", "x".repeat(crate::vars::ENVIRONMENT_MAX - 2));
        assert_eq!(env(&["-i", &fits, "true"], BLOCK).0, 0);
        assert_eq!(
            env(&["-i", &over, "true"], BLOCK),
            (
                126,
                String::new(),
                "env: 'true': Argument list too long\n".into()
            )
        );
    }

    #[test]
    fn env_runs_a_command_with_its_environment() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::env::tests::env_s_command_gets_at_most_64_kib_in_the_in_process_runner_too`.

- [ ] **Step 3: Change `crates/shell/src/commands/env.rs`**

In `crates/shell/src/commands/env.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::killed;
use alloc::format;
````

with:

````rust
use crate::killed;
use crate::vars::ENVIRONMENT_MAX;
use alloc::format;
````

Replace:

````rust
        };
        let outer = core::mem::replace(&mut ctx.environment, block);
````

with:

````rust
        };
        // What `spawn` would refuse.
        if block.len() > ENVIRONMENT_MAX {
            return cannot_run(ctx, name, Errno::E2BIG);
        }
        let outer = core::mem::replace(&mut ctx.environment, block);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 460 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): hold env's command to 64 KiB in the in-process runner

/bin/env's spawn refuses an environment over 64 KiB (E2BIG), but the
in-process runner's env ran its command's function with any size of
block. It now says env: 'COMMAND': Argument list too long, status 126,
as /bin/env does.

Refs: review m1
EOF
````


### Task 26: A command run as a program gets its environment and `Programs`

Decision 11: `run_program` runs a command function as `run_command` does, giving it the program's environment and a `Programs` to start others; relay-rt's `run_command`, the `main` of every program of `/bin`, uses it with the block the program started with and `SysPrograms`, which start a child in the program's own group. So `env` run as a program starts `/bin/COMMAND`, or the path given, on its fds 0 to 2 and waits for it; a command a fault kills leaves `env` the shell's status for it (139), nothing printed (§10; the maintainer, 2026-10-04). The red run is the shell's tests: `run_program` is missing. Mutation checks (2): `run_program` dropping the environment fails a unit test; relay-rt passing none fails `environment` (once `/bin/env` exists, Task 28).

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/program.rs`

**Interfaces:**
- Consumes: Task 24.
- Produces: `shell::run_program(name, run, args, io, env, programs)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, replace:

````rust
        assert_eq!(out.text(), "hostname  motd\n");
    }
````

with:

````rust
        assert_eq!(out.text(), "hostname  motd\n");
    }

    #[test]
    fn env_run_as_a_program_starts_its_command_and_waits_for_it() {
        // In the program's group, on its fds 0 to 2, with the environment
        // env made; its status the command's (programmable shell gate
        // §8.6).
        use crate::Group;
        use crate::testing::FakePrograms;
        use relay_abi::WaitStatus;
        use relay_abi::wait::{ACCESS_READ, FAULT_PAGE};
        let mut h = Harness::new();
        let mut programs = FakePrograms::new();
        programs.known.push(("/bin/t-args", WaitStatus::exited(3)));
        programs.known.push(("/tmp/x", WaitStatus::exited(0)));
        programs.known.push((
            "/bin/t-fault",
            WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0),
        ));
        programs.refusals.push(("/etc", Errno::EISDIR));
        programs.refusals.push(("/tmp/text", Errno::ENOEXEC));
        let mut run = |args: &[&str]| {
            let words: alloc::vec::Vec<String> = args.iter().map(|a| String::from(*a)).collect();
            let mut out = FakeStdout::console();
            let status = crate::run_program(
                "env",
                crate::commands::env,
                &words,
                crate::CommandIo {
                    vfs: &mut h.vfs,
                    console: &mut h.console,
                    system: &mut h.system,
                    stdin: &mut crate::Bytes::new(alloc::vec::Vec::new()),
                    stdout: &mut out,
                },
                b"HOME=/root\0A=1\0",
                &mut programs,
            );
            (status, h.console.take())
        };
        assert_eq!(run(&["-u", "A", "B=2", "t-args", "x"]), (3, String::new()));
        assert_eq!(run(&["/tmp/x"]), (0, String::new()));
        assert_eq!(run(&["t-fault"]), (139, String::new()));
        assert_eq!(
            run(&["nope"]),
            (127, "env: 'nope': No such file or directory\n".into())
        );
        assert_eq!(
            run(&["/etc"]),
            (126, "env: '/etc': Permission denied\n".into())
        );
        assert_eq!(
            run(&["/tmp/text"]),
            (126, "env: '/tmp/text': Exec format error\n".into())
        );
        let first = &programs.spawned[0];
        assert_eq!(
            (
                first.path.as_str(),
                &first.args[..],
                &first.env[..],
                first.fds,
                first.group
            ),
            (
                "/bin/t-args",
                &["t-args".into(), "x".into()][..],
                &b"HOME=/root\0B=2\0"[..],
                [0, 1, 2],
                Group::Shell
            )
        );
        assert_eq!(programs.spawned[1].path, "/tmp/x");
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `run_program` in the crate root ``.

- [ ] **Step 3: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// The `main` of one of `/bin`'s commands (user-space gate §8.4): runs the
/// command `name`, whose function is `run`, with the program's arguments,
/// printing exactly what it prints in the shell; its exit status.
pub fn run_command(name: &str, run: shell::commands::Run, args: &Args) -> u8 {
````

with:

````rust
/// The `main` of one of `/bin`'s commands (user-space gate §8.4): runs the
/// command `name`, whose function is `run`, with the program's arguments
/// and environment, printing exactly what it prints in the shell; its
/// exit status.
pub fn run_command(name: &str, run: shell::commands::Run, args: &Args) -> u8 {
````

Replace:

````rust
    };
    let status = shell::run_command(name, run, &words(args), io);
    status as u8
````

with:

````rust
    };
    // `env` starts its command in this program's group, as a script's are.
    let mut programs = SysPrograms::new(false);
    let env = crate::env::block();
    let status = shell::run_program(name, run, &words(args), io, env, &mut programs);
    status as u8
````

- [ ] **Step 4: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use io::{Bytes, Console, Group, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command};
pub use shell::Shell;
````

with:

````rust
pub use io::{Bytes, Console, Group, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command, run_program};
pub use shell::Shell;
````

- [ ] **Step 5: Change `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::ctx::Ctx;
use crate::io::{Console, Stdin, Stdout, System};
use alloc::format;
````

with:

````rust
use crate::ctx::Ctx;
use crate::io::{Console, Programs, Stdin, Stdout, System};
use alloc::format;
````

Replace:

````rust
pub fn run_command(name: &str, run: Run, args: &[String], io: CommandIo<'_>) -> i32 {
    let mut ctx = Ctx::program(io.vfs, io.system, io.console, io.stdout);
    ctx.set_input(io.stdin);
    let mut status = run(&mut ctx, args);
````

with:

````rust
pub fn run_command(name: &str, run: Run, args: &[String], io: CommandIo<'_>) -> i32 {
    let ctx = Ctx::program(io.vfs, io.system, io.console, io.stdout);
    finish(name, run, args, ctx, io.stdin)
}

/// As [`run_command`], the command given its program's environment `env`
/// and `programs` to start others (`env`'s command, programmable shell
/// gate §8.6).
pub fn run_program(
    name: &str,
    run: Run,
    args: &[String],
    io: CommandIo<'_>,
    env: &[u8],
    programs: &mut dyn Programs,
) -> i32 {
    let mut ctx = Ctx::program(io.vfs, io.system, io.console, io.stdout);
    ctx.environment = env.to_vec();
    ctx.programs = Some(programs);
    finish(name, run, args, ctx, io.stdin)
}

/// Runs the command on `ctx`, reading `stdin`, and says a write error.
fn finish<'a>(
    name: &str,
    run: Run,
    args: &[String],
    mut ctx: Ctx<'a>,
    stdin: &'a mut dyn Stdin,
) -> i32 {
    ctx.set_input(stdin);
    let mut status = run(&mut ctx, args);
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 461 tests.

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 35 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): give a command run as a program its environment

run_program runs a command function as run_command does, giving it the
program's environment and a Programs to start others; relay-rt's
run_command, the main of every program of /bin, now uses it with the
block the program started with and SysPrograms, which start a child in
the program's own group. So env run as a program starts /bin/COMMAND,
or the path given, on its fds 0 to 2 and waits for it.
EOF
````


### Task 27: `env ''` finds no program

Decision 11 (the prototype's review, m-2): as a program, `env ''` started the path `/bin/`, a directory, and said GNU's words for one, `Permission denied`, status 126, where GNU says `env: '': No such file or directory`, status 127: `execvp` finds no program of an empty name. `FakePrograms` refuses `/bin/` as the kernel does. The red run is the shell's tests. The red run is also this fix's mutation check.

**Files:**
- Modify: `crates/shell/src/commands/env.rs`
- Modify: `crates/shell/src/program.rs`

**Interfaces:**
- Consumes: Task 26.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        programs.refusals.push(("/etc", Errno::EISDIR));
        programs.refusals.push(("/tmp/text", Errno::ENOEXEC));
````

with:

````rust
        programs.refusals.push(("/etc", Errno::EISDIR));
        // As the kernel answers for the directory `/bin/`.
        programs.refusals.push(("/bin/", Errno::EISDIR));
        programs.refusals.push(("/tmp/text", Errno::ENOEXEC));
````

Replace:

````rust
            (127, "env: 'nope': No such file or directory\n".into())
        );
````

with:

````rust
            (127, "env: 'nope': No such file or directory\n".into())
        );
        // An empty name is no program, as GNU's execvp finds none, not the
        // directory `/bin/`.
        assert_eq!(
            run(&[""]),
            (127, "env: '': No such file or directory\n".into())
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `program::tests::env_run_as_a_program_starts_its_command_and_waits_for_it`.

- [ ] **Step 3: Change `crates/shell/src/commands/env.rs`**

In `crates/shell/src/commands/env.rs`, replace:

````rust
fn run(ctx: &mut Ctx<'_>, name: &str, args: &[String], block: Vec<u8>) -> i32 {
    let Some(programs) = ctx.programs.as_deref_mut() else {
````

with:

````rust
fn run(ctx: &mut Ctx<'_>, name: &str, args: &[String], block: Vec<u8>) -> i32 {
    // An empty name names no program, as GNU's `execvp` finds none.
    if name.is_empty() {
        return cannot_run(ctx, name, Errno::ENOENT);
    }
    let Some(programs) = ctx.programs.as_deref_mut() else {
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
fix(shell): find no program for env ''

As a program, env '' started the path /bin/, a directory, and said
GNU's words for one, env: '': Permission denied, status 126, where GNU
says env: '': No such file or directory, status 127: execvp finds no
program of an empty name.

Refs: review m2
EOF
````


### Task 28: `/bin/env`

Decision 11 (spec §8.6, §11.5): `/bin/env` is the shell's `env` run as a program. `/bin` holds 46 programs, and the `system` scenario's `ls /bin` lays them out again (taken from the scenario's serial log). No failing run of its own: the scenario `system` fails on its `ls /bin` lines with the program and passes with them.

**Files:**
- Modify: `tests/e2e/system.txt`
- Create: `userland/utils/src/bin/env.rs`

**Interfaces:**
- Consumes: Task 26.
- Produces: the program `/bin/env`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-files  t-proc   t-spin  tail   true\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-env    t-mem    t-read   t-sys   test   uname\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-fault  t-pipe   t-spawn  t-tee   touch  wc\n
expect root@relay:~# $
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  false  head   mv        pwd     rmdir  sleep  t-abi   t-fault  t-pipe  t-spawn  t-tee  touch  wc\ncat    date  echo   free   ls     poweroff  reboot  seq    stat   t-args  t-files  t-proc  t-spin   tail   true\nclear  df    env    grep   mkdir  ps        rm      sh     sync   t-env   t-mem    t-read  t-sys    test   uname\n
expect root@relay:~# $
````

Replace:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-files  t-proc   t-spin  tail   true\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-env    t-mem    t-read   t-sys   test   uname\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-fault  t-pipe   t-spawn  t-tee   touch  wc\n
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  false  head   mv        pwd     rmdir  sleep  t-abi   t-fault  t-pipe  t-spawn  t-tee  touch  wc\ncat    date  echo   free   ls     poweroff  reboot  seq    stat   t-args  t-files  t-proc  t-spin   tail   true\nclear  df    env    grep   mkdir  ps        rm      sh     sync   t-env   t-mem    t-read  t-sys    test   uname\n
````

- [ ] **Step 2: Create `userland/utils/src/bin/env.rs`**

Create `userland/utils/src/bin/env.rs`:

````rust
//! `/bin/env`: print the environment, or run a command with a changed one
//! (programmable shell gate §8.6), the shell's command function run as a
//! program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("env", shell::commands::env, &args)
}
````

- [ ] **Step 3: Run the `system` scenario**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests userland
git commit -F - <<'EOF'
feat(utils,e2e): add /bin/env

/bin/env is the shell's env run as a program (programmable shell gate
§8.6): it prints the environment it started with, or starts /bin/COMMAND
with a changed one and waits for it. /bin holds 46 programs, and the
system scenario's ls /bin lays them out again.
EOF
````


### Task 29: 46 programs in `/bin`

Decision 11 (spec §11.5): the startup line's count in `kernel/src/system.rs`'s comment and in `docs/hardware-test.md`'s checklist follows; the results log is history and keeps its counts. Documentation only: no run but lint.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`

**Interfaces:**
- Consumes: Task 28.
- Produces: nothing new.

- [ ] **Step 1: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 45 programs, ABI 4`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

with:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 46 programs, ABI 4`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

Replace:

````markdown
   - `[ ok ] dev: /dev/null` (`/dev`, a filesystem of the kernel's)
   - `[ ok ] system: 45 programs, ABI 4` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

with:

````markdown
   - `[ ok ] dev: /dev/null` (`/dev`, a filesystem of the kernel's)
   - `[ ok ] system: 46 programs, ABI 4` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

- [ ] **Step 2: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 45 programs, ABI 4` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 46 programs, ABI 4` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add docs kernel
git commit -F - <<'EOF'
docs(kernel,checks): count 46 programs in /bin

/bin holds env now (spec §11.5): the startup line's count in
kernel/src/system.rs's comment and in docs/hardware-test.md's checklist
follows. The results log is history and keeps its counts.
EOF
````


### Task 30: `env` in QEMU

Decision 11: the scenario `environment` ends with `/bin/env`: it lists what it got, `-u` removes, `-i` empties, `NAME=value` sets one in its place or adds it, a command (`t-env`, `sh` with a script) gets the result, and a command not found, a directory and a refused option give GNU's words and statuses (QEMU's `spawn` of `/etc` is `EISDIR`, said as `Permission denied`). Tests only: no failing run; it catches Task 26's relay-rt mutant.

**Files:**
- Modify: `tests/e2e/environment.txt`

**Interfaces:**
- Consumes: Tasks 24–28.
- Produces: the scenario's lines.

- [ ] **Step 1: Expect the new lines in `tests/e2e/environment.txt`**

In `tests/e2e/environment.txt`, replace:

````text
send export HOME=/root
expect root@relay:~# $
````

with:

````text
send export HOME=/root
expect root@relay:~# $
# env prints its environment, or runs a command with a changed one, as
# GNU's does.
send env -u C -u A -u OLDPWD
expect \nPWD=/root\nHOME=/root\nroot@relay:~# $
send env -i A=1 B='two words' env
expect \nA=1\nB=two words\nroot@relay:~# $
send Z=1 env -u Z HOME=/x X=1 t-env
expect \n6 entries, 50 bytes\n\[0\] PWD=/root\n\[1\] OLDPWD=/tmp\n\[2\] C=two words\n\[3\] A=5\n\[4\] HOME=/x\n\[5\] X=1\nroot@relay:~# $
send env -i env; env -i t-env
expect \n0 entries, 0 bytes\nroot@relay:~# $
send env nope; echo $?; env /etc; echo $?; env -x; echo $?
expect \nenv: 'nope': No such file or directory\n127\nenv: '/etc': Permission denied\n126\nenv: invalid option -- 'x'\n125\nroot@relay:~# $
send echo 'echo "[$Q]"' > /root/q.sh; env Q=1 sh /root/q.sh
expect \n\+ echo "\[\$Q\]"\n\[1\]\nroot@relay:~# $
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
test(e2e): run env in QEMU

The scenario environment ends with /bin/env: it lists what it got, -u
removes, -i empties, NAME=value sets one in its place or adds it, a
command (t-env, sh with a script) gets the result, and a command not
found, a directory and a refused option give GNU's words and statuses.
EOF
````


### Task 31: README, AGENTS.md and `docs/hardware-test.md`

Decision 15 (plan 2's final review, m-7; the prototype's review, m-7): README's crate table says `vfs` holds `DevFs` and relay-rt reads a program's environment, and that the shell keeps variables, the environment and redirection; README, AGENTS.md and `docs/hardware-test.md` list the eight built-ins, `export` and `unset` among them, and `env` with the programs. Documentation only: no run but lint.

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/hardware-test.md`

**Interfaces:**
- Consumes: everything before.
- Produces: nothing new.

- [ ] **Step 1: Change `AGENTS.md`**

In `AGENTS.md`, replace:

````markdown
(`;`, `&&`, `||`, `!`), `if`, `while`, `until` and `for`, commands read
across lines with bash's `> ` prompt, and six built-ins (`cd`, `exit`,
`help`, `jobs`, `kill`, `wait`). Every other command is a program, `test`
and `[` too. The programmable shell gate goes on with milestone 5
(redirection and environment, 0.6.0). The aarch64 port comes later.

````

with:

````markdown
(`;`, `&&`, `||`, `!`), `if`, `while`, `until` and `for`, commands read
across lines with bash's `> ` prompt, and eight built-ins (`cd`, `exit`,
`export`, `help`, `jobs`, `kill`, `unset`, `wait`). Every other command is
a program, `test`, `[` and `env` too. The programmable shell gate goes on
with milestone 5 (redirection and environment, 0.6.0). The aarch64 port
comes later.

````

- [ ] **Step 2: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
typed, and copy what the console shows into files (tees). Every command
but the shell's built-ins (`cd`, `exit`, `help`, `jobs`, `kill`, `wait`)
is a program of its own in `/bin` (`/bin/ls`), which prints what the
shell's command prints, and the shell itself is one too: `/bin/sh` runs
each of those commands as a program, and its scripts may run scripts.
Process 1 is the kernel's init: it starts `/bin/sh` at boot and again
````

with:

````markdown
typed, and copy what the console shows into files (tees). Every command
but the shell's built-ins (`cd`, `exit`, `export`, `help`, `jobs`,
`kill`, `unset`, `wait`) is a program of its own in `/bin` (`/bin/ls`),
which prints what the shell's command prints, and the shell itself is
one too: `/bin/sh` runs each of those commands as a program, and its
scripts may run scripts.
Process 1 is the kernel's init: it starts `/bin/sh` at boot and again
````

Replace:

````markdown
| `crates/term` | Framebuffer text terminal |
| `crates/vfs` | Error numbers, block-device and filesystem traits, paths, mount table, in-memory filesystem |
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser, expansion (`$1`, `$NAME`), pipelines, jobs, scripts (`sh FILE`) and every command's function; used by `/bin/sh`, each program of `/bin` and `host-shell` |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel and of user programs |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, ELF note, call numbers, error numbers, result encoding, and the structs the calls pass (`Stat`, `SpawnArgs`, `ProcInfo`, `WaitStatus`, …); no architecture detail |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, heap, panic handler, ABI note, linker script; the shell's `Vfs`, `Console`, `System`, `Stdin`, `Stdout` and `Programs` over system calls |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
````

with:

````markdown
| `crates/term` | Framebuffer text terminal |
| `crates/vfs` | Error numbers, block-device and filesystem traits, paths, mount table, in-memory filesystem, `DevFs` (`/dev/null`) |
| `crates/ext2` | ext2 driver with its block cache |
| `crates/shell` | Line editor, parser, expansion (`$1`, `$NAME`, `~`), variables and the environment (`export`, `A=1 cmd`), redirection, pipelines, jobs, scripts (`sh FILE`) and every command's function; used by `/bin/sh`, each program of `/bin` and `host-shell` |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel and of user programs |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, ELF note, call numbers, error numbers, result encoding, and the structs the calls pass (`Stat`, `SpawnArgs`, `ProcInfo`, `WaitStatus`, …); no architecture detail |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, environment, system calls, heap, panic handler, ABI note, linker script; the shell's `Vfs`, `Console`, `System`, `Stdin`, `Stdout` and `Programs` over system calls |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
````

- [ ] **Step 3: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown

Every command but the shell's built-ins (`cd`, `exit`, `help`, `jobs`,
`kill`, `wait`) is a program in `/bin`.

````

with:

````markdown

Every command but the shell's built-ins (`cd`, `exit`, `export`, `help`,
`jobs`, `kill`, `unset`, `wait`) is a program in `/bin`, `env` among them.

````

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add AGENTS.md README.md docs
git commit -F - <<'EOF'
docs: name the environment, DevFs and eight built-ins

README's crate table says vfs holds DevFs and relay-rt reads a
program's environment, and that the shell keeps variables, the
environment and redirection; README, AGENTS.md and
docs/hardware-test.md list the eight built-ins, export and unset among
them, and env with the programs.

Refs: final review m7, review m7
EOF
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 52 scenario(s) passed`.

````bash
git push -u origin m5p3/env
gh pr create --base main --head m5p3/env --title "feat(shell,utils): add /bin/env" --body-file - <<'EOF'
## What

Milestone 5, plan 3, tasks 24–31: `env [-i] [-u NAME]... [-] [NAME=value]... [COMMAND [ARG]...]` as GNU coreutils 9.4's without `PATH`, compared line for line: it prints its environment, or starts `/bin/COMMAND` (or the path) with a changed one and waits for it, GNU's messages and statuses 125/126/127 (`env ''` not found, a directory `Permission denied`, over 64 KiB `Argument list too long`, under both runners); a command run as a program gets its environment and a `Programs` (`run_program`); `/bin` holds 46 programs; the scenario `environment` ends with it; README's crate table (plan 2's final review, m-7) and the built-ins in README, AGENTS.md and `docs/hardware-test.md`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p3-env
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
