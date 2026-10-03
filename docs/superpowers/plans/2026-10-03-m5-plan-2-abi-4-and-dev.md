# Milestone 5 · Plan 2: ABI 4 and `/dev` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs get an environment (spec §8.1–§8.3): ABI 4's `SpawnArgs::env` and its checks, the block on the new stack below the arguments, its address, length and count in `rcx`, `r8` and `r9`, `relay_rt::env` to read it, `/bin/sh` handing its own on, init's `HOME=/root`, and `t-env` with the scenario `env_calls`; `statfs` says which filesystems are read-only, in the same bump, and `test -w` answers from it as GNU's does; `/dev/null` exists (§8.4), an in-kernel filesystem mounted at `/dev` with its startup line, read by `cp` and used by the `redirect` scenario. The spec's §15 item 6 and the roadmap's notes land with this plan. It ends with `cargo xtask ci` green, 51 scenarios (the new `env_calls` among them), `system: 45 programs, ABI 4`, and no NUC check.

**Architecture:** `relay_abi::VERSION` becomes 4 for two layouts: `SpawnArgs` gains `env` and `env_len`, `StatFs` gains `flags` with `STATFS_READ_ONLY`. The kernel's `spawn` copies the block in (64 KiB apart from the arguments) and `exec::load`, which now takes the arguments and the environment as `Strings`, lays it just below the arguments at the top of the stack; `arch::user::enter` loads its address, length and count into the entry function's fourth to sixth argument registers, as `x3`–`x5` will be on aarch64. relay-rt's `start` keeps them, and a new module, `relay_rt::env`, reads them (`Block`, `var`, `vars`, `block`, `raw`); `sys::spawn_env` gives a child an environment and `SysPrograms` passes the program's own. `vfs::StatFs` gains `read_only`, which every filesystem fills in and the kernel and relay-rt carry. A new module, `crates/vfs/src/devfs.rs`, is `/dev`'s filesystem, mounted by a new kernel module, `kernel/src/dev.rs`, and by `host-shell`; the kernel's open file of a character device gives 0 to `seek`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; bash 5.2.21, GNU coreutils 9.4 and glibc for comparison; `unshare` (util-linux) for root's answers on a read-only mount; QEMU 8.2 under KVM with `-cpu max`.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§8.1–§8.4, §10, §11, §12, §13, §15 items 3, 5 and 6)
**Roadmap:** `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md` — this is plan 2 of 4 of milestone 5.

## In brief

- **Size.** 17 tasks in two code pull requests, plus this plan as PR 1: PR 2 ABI 4 (the bump, the kernel's checks and stack, relay-rt's `env`, init's `HOME`, `t-env` and `env_calls`, the registers shown as they came, the read-only flag, `test -w` from it and compared with GNU's); PR 3 `/dev/null` (`DevFs`, `cp` reading it, the in-process runner's output node, a device's `seek` and its `whence`, the mount at startup, `host-shell`, the scenarios' lines).
- **The spike** (decision 1): `VERSION` bumped alone and every scenario run: only the version's text failed, in 13 files. A second spike logged each program's deepest stack use at its end over every scenario and a probe of `/bin/sh` at the nesting bound with pipelines and redirections inside: `/bin/sh`'s 40 KiB was the deepest, against the 892 KiB both full blocks leave, so the stack stays 255 pages (decision 3).
- **The maintainer's decisions:** `df` keeps listing only `/`, as GNU's `df` 9.4 hides `/dev` (decision 8); `sh /dev/null` stays refused, `sh` running only a regular file (decision 10).
- **`test -w`** (decision 6): false on any filesystem `statfs` says is read-only, but for a device, a FIFO or a socket, as Linux's `access` answers GNU's `test`; compared with GNU's on a read-only bind mount in `unshare -rm`.
- **The prototype's review.** A fresh reviewer read the whole prototype (then twelve tasks), ran the unit tests, `cargo xtask ci` on PR 2 alone (all 51 scenarios green), throwaway tests against GNU, and a throwaway QEMU scenario of about 50 `/dev/null` and environment lines under `/bin/sh`. It found 0 critical, 1 important and 5 minor real defects (and a long doc line), each settled in a task of its own after the task it concerns, with a test that fails first, or for the two test gaps a mutant it now catches; one design call came to the user (decision 10). It found correct: `spawn`'s checks and their order (`E2BIG` before anything is read), each copy bounded, the stack's layout without underflow and the registers' order in `enter`, `Block`'s parsing of odd blocks, the environment through pipelines, jobs, `X | sh` and `sh FILE`, ABI 4's ripples with no `ABI 3` left, every filesystem's read-only answer, `/dev/null` under `/bin/sh` with `>`, `>>`, `<`, `2>`, `cat`, `head`, `tail`, `wc`, `grep`, `test`, `mkdir`, `rm`, `mv`, `cd`, nothing taking it for the console, the mount over QEMU's `/dev` and over the empty root, nothing depending on `/bin` being filesystem 2, and every commit's widths. It declined to judge `EPERM` in `/dev`, the missing device numbers (both decided), and ext2's lack of a read-only remount after a later `EIO` (out of this plan).

| Finding (review) | Decision |
|---|---|
| Important (I-1): `cp /dev/null f` refused (`Invalid argument`), where GNU's empties `f` | Fixed, Task 11 |
| Minor (M-1): `sh /dev/null` fails, status 1, where bash runs nothing, status 0 | A decided difference (the user's choice, decision 10): `sh` runs only a regular file |
| Minor (M-2): in the in-process runner `grep x /dev/null > /dev/null` says `input file is also the output` | Fixed, Task 12 |
| Minor (M-3): a character device's `seek` gave 0 for an unknown `whence` | Fixed, Task 14 |
| Minor (M-4): `t-env` counted the entries itself, so a wrong count register went unseen | Fixed, Task 6 |
| Minor (M-5): spec §10 still listed `test -w`'s difference | Fixed in PR 1's §10 |
| Minor (M-6): `test -w`'s read-only answers written by hand, not compared with GNU's | Fixed, Task 9 |

## Where this plan fits

Plan 2 of milestone 5 is the gate's sixth step (spec §12). It builds on plan 1's redirection (the fd context, the `redirect` scenario, `cat` and `grep` refusing an input that is the output on regular files) and on milestone 4's `test`. Plan 3 (the shell's environment) builds on this plan's `relay_rt::env` and `sys::spawn_env`: it imports the block as the shell's exported variables, passes only those on where this plan passes the whole block, and gives relay-rt's test double its environment. Plan 4 runs `check7.sh` and NUC checks 3–7, recording check 3's new lines on the NUC. The roadmap's notes carry what this plan leaves for them.

## Working conventions

- Plan 2 lands as **three pull requests** (table below). This plan, with the spec's §15 item 6 (with the §10 lines and status line it changes) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-03-m5-plan-2-abi-4-and-dev.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
  The NUC transcripts hold escape bytes, which a markdown block cannot carry: they are changed by a command step.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; test programs under `userland/tests/`; scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 5, 9 and 17 add tests only: they have no failing run; each names the mutants of the code its tests catch. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m5p2/mutations.md`).
- **The expectations are the real tools'.** glibc's `getenv` (`tmp/m5p2/probes/p1-getenv.txt`), GNU's `test -w` as root on a read-only bind mount (`p2-test-w-ro.txt`, and Task 9's test), GNU's `cp`, `grep` and `cat` with `/dev/null`, bash 5.2 for the scenarios' lines, Linux's `lseek` on `/dev/null`. In this session's shell `grep` may be a wrapper of another tool: any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop in a test.** A program's environment and arguments are untrusted: the kernel bounds each copy by its length before reading it (64 KiB each), and nothing that follows from them may panic or overflow. Mutation checks of the scenarios run one QEMU scenario each.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the ABI change is marked `!` with a `BREAKING CHANGE:` footer; the review's fixes carry `Refs: review <id>`. Every commit body line is within 72 columns, counted in characters. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. The scenario parser refuses an input that no `expect` of the prompt paces. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `0ba3934` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m5p2/plan` | — | The spec's §15 item 6 with its §10 lines and status line, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m5p2/abi` | 1–9 | ABI 4: the environment from `spawn` to `relay_rt::env`, init's `HOME`, `t-env` and `env_calls`; `statfs`'s read-only flag and `test -w` | `lint`, `unit`, `e2e` |
| 3 | `m5p2/dev` | 10–17 | `/dev/null`: `DevFs`, `cp`, the output node, `seek`, the mount at startup and in `host-shell`, the scenarios' lines | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 2 adds no crate. `crates/shell` uses only `vfs` and `relay-abi`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (UG §3.1). `crates/relay-abi` keeps the same values on every architecture: the entry's environment registers are its fourth to sixth argument registers.
- **One ABI bump:** `relay_abi::VERSION` 4, in Task 1, a `!` commit with a `BREAKING CHANGE:` footer; no other task changes a value, struct or call of `relay-abi`.
- Every scenario and check script of milestone 5's plan 1 passes, changed only where the version, the program count, `/dev` and `[ -w ]` change what they print (each listed in its task); the scenarios gain `env_calls`. Check 3's NUC transcripts change by hand only (`ABI 4`, the `dev` line).
- What a program passes is untrusted (AGENTS.md): no panic, unchecked index, overflow, unbounded kernel allocation or endless loop may follow from `SpawnArgs` and what it points at.
- The shell follows bash 5.2 and commands GNU word for word; the only decided differences are spec §10's and §15's.
- Missing tools fail tests, never skip them: bash, GNU's coreutils and util-linux's `unshare` and `mount` are on CI's runner.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 6 in PR 1:

1. **The pull requests.** Plan 2 is three: its plan; ABI 4 (the bump, the environment from `spawn` to `relay_rt::env`, init's `HOME`, `t-env`, and the read-only flag with `test -w`); and `/dev/null`. A spike bumped `VERSION` alone and ran everything: only the version's text failed (the startup line `ABI 3` in four scenarios and check 3, `kernel wants 3`, `t-abi`'s `built for ABI 2`), in 13 files. It has no NUC check: check 3's `ABI 4` and `dev` lines reach its NUC transcript by hand until plan 4's run.
2. **One bump.** `SpawnArgs` gains `env` and `env_len` (offsets 128 and 136; 144 bytes) and `StatFs` a `flags` field (offset 48; 56 bytes) whose bit `STATFS_READ_ONLY`, 1 as Linux's `ST_RDONLY`, says the filesystem is read-only: one `VERSION` 4 in one `!` commit with the version's ripples, the code that fills them in following in commits of their own.
3. **The block on the stack** (§8.1, §13). The kernel copies the environment just below the arguments, which keep the top of the stack, and the stack pointer goes 16-byte aligned below both; with no environment `rcx`, `r8` and `r9` are 0. Each entry is counted by its NUL; the kernel checks only §8.1's size and NUL, so an empty entry or one without `=` passes. The stack stays 255 pages: the spike logged each program's deepest stack at its end over every scenario and `/bin/sh` at the nesting bound with pipelines and redirections inside, and the deepest was `/bin/sh`'s 40 KiB, against the 892 KiB both full blocks leave. A kernel test loads both at 64 KiB, and `t-env` starts `sh` at the nesting bound with both full.
4. **`relay_rt::env`** (§8.3). `var(name)` and `vars()` read the block as glibc and Rust's `std` do: an entry is split at its first `=` after its first byte, one without is skipped, and the first of two equal names wins; `block()` gives it whole, and `raw()` the three registers as they came. `sys::spawn` still starts a child with no environment, its callers unchanged, and `sys::spawn_env` gives one; `SysPrograms` passes the program's own, so `/bin/sh` hands its whole environment to every command until plan 3's export decides what goes. The test double of §8.3 comes with plan 3, the first to use it; plan 2 tests the block as `Args` is tested.
5. **`t-env`** (§11.3). It prints the block it got, `[n] entry` per entry, after a line with the count and length the registers gave (the prototype's review: a count the entries did not show went unseen), and a child started with none shows all three registers 0; its kinds start a child with a given block (non-ASCII text, an entry without `=`, an empty one), with none, with 64 KiB and with one byte more (`E2BIG`), with 64 KiB of arguments as well, with no final NUL (`EINVAL`), `sh` reading `t-env` from a pipe (a grandchild), and `sh deep.sh` with both blocks full. From the prompt it shows init's `HOME=/root`. `/bin` holds 45 programs.
6. **The read-only flag** (§15 item 3). ext2 says read-only when it was mounted so (after an `EIO`) or shut down, `MemFs` when made so, `system.img` always, `/dev` never. `test -w` is false on any filesystem `statfs` says is read-only, so on a root mounted read-only too, ending §10's difference, but for a device, a FIFO or a socket, as Linux's `access` answers GNU's `test`; a test compares each kind with GNU's on a read-only bind mount in `unshare -rm` (the prototype's review). A `/bin` that is a directory of the root (`host-shell`) is writable, as before.
7. **`/dev/null`** (§8.4). `vfs::DevFs` holds a root directory (`0755`) and `null`, a character device (`0666`, user and group 0, its times the mount's): a read gives the end of input, a write takes everything, truncating and `touch` succeed and change nothing. Creating, removing or renaming anything in `/dev` is `EPERM` (`Operation not permitted`), as on a Linux filesystem that has no such operation (Linux says `EACCES` for a file, which `vfs` lacks). The kernel mounts it right after `/`, before `/bin` (so `/dev` is filesystem 2 and `/bin` 3), with `[ ok ] dev: /dev/null`, or `[FAIL] dev: cannot mount /dev: <reason>` and the boot goes on. A character device's `seek` gives 0 for any offset, after refusing an unknown `whence` as Linux does (the prototype's review). Its `dev` is its mount's number, never 0, so `relay_rt::sysio::is_console` does not take it for the console: `[ -t 1 ] > /dev/null` is false, `[ -c /dev/null ]` and `[ -w /dev/null ]` true. `host-shell` mounts it too, where `> /dev/null` wrote a file into the image. `cp` reads a character device as a source, so `cp /dev/null f` empties `f` as GNU's does, where milestone 1 refused every source but a regular file; and the in-process runner's `grep` takes an input for its output only when it is a regular file, as GNU does (the prototype's review).
8. **`df` lists only `/`** (§8.4, §11.5). GNU's `df` 9.4 leaves `/dev` (devtmpfs) out unless given `-a`, and a filesystem of no blocks too, so `df` stays as it is and nothing of it ripples (the maintainer, 2026-10-03).
9. **No device numbers.** `Stat` has no `rdev`, so `ls -l /dev/null` shows its size, 0, where GNU shows `1, 3` (§10).
10. **`sh` and `/dev/null`.** `sh FILE 2> /dev/null` stays refused (§15 item 5): the rule is the fd, not the file. `sh` runs only a regular file, so `sh /dev/null` is `sh: /dev/null: Invalid argument`, status 1, where bash runs nothing, status 0; its transcript, `/dev/null.log`, could not be made either (the maintainer, 2026-10-03; §10).
11. **Plan 1's deferred minors.** §15 item 5's input that is the output: `cat` refuses an input that has bytes left to read, but `grep`, as GNU's 3.11, refuses the output's file whatever is left of it, and not with `-c` or `-q`, which write no line. A failed redirection on a pipeline's earlier stage after `2>&1` (`cat 2>&1 < /nope | wc -l`) tells its message on the screen, not into the pipe, and a refusal drops an explicit default fd (`1> f | cat` says `> before |`): both go to plan 4.

## Review Focus

1. **`/dev/null` at the prompt and in scripts under `/bin/sh`**, against bash 5.2 and GNU: `>`, `2>`, `>>`, `<` with it, `cat`, `head`, `wc`, `grep` and `cp` reading it, `cp` to it, `test -c`/`-w`/`-t` with it, `ls /dev`, making or removing anything in `/dev`, `sh /dev/null`. Expected: GNU's output and statuses but for decided differences (spec §10, §15 item 6); nothing takes it for the console or for a regular file where that matters. Tests: the `devfs` module's (Task 10), `cp_reads_dev_null_as_gnu_s_does` (Task 11), `a_device_is_never_the_input_that_is_the_output` (Task 12), the scenario `redirect` (Task 17).
2. **The environment end to end:** `spawn`'s checks and their order (`E2BIG` by length before reading, `EFAULT`, `EINVAL`), 64 KiB and one byte more, both blocks full, an empty block, the stack's layout and alignment, the registers at entry (all 0 for none), relay-rt's reading of odd blocks, `/bin/sh` handing its block on to programs, pipelines, jobs, `X | sh` and `sh FILE`. Tests: `spawn_refuses_what_is_not_a_valid_request`, `spawn_copies_in_everything_its_struct_names`, `both_blocks_at_64_kib_leave_the_stack_its_room` (Task 2), the `env` module's (Tasks 3, 6), `the_shell_starts_with_home_set` (Task 4), the scenario `env_calls` (Tasks 5, 6).
3. **ABI 4's ripples:** a program built for ABI 3 refused, `t-abi`, the startup lines, check 3's expectations and both of its transcripts, `docs/hardware-test.md`, `ls /bin`, 45 programs. Tests: relay-abi's layouts (Task 1), xtask's `t_abi_is_t_args_built_for_the_abi_before`, the scenarios `boot`, `system`, `system_abi`, `stale_program`, `checks`.
4. **The read-only flag and `test -w`:** each filesystem's answer (ext2 mounted read-only, after `shutdown`, its fallback; `MemFs`; `system.img`; `DevFs`), `test -w` on each kind of file there, `/bin` as a mount or a directory (`host-shell`). Tests: `statfs_says_which_filesystems_are_read_only` and each filesystem's (Task 7), `nothing_on_a_read_only_filesystem_is_writable_but_a_device` (Task 8), `on_a_read_only_filesystem_as_gnu_s_test` (Task 9), `mount_fail`'s `[ -w / ]` (Task 17).
5. **The mount and boot:** `/dev` over QEMU's `/dev` and over the empty root, a `/dev` that is a file, `/bin` now filesystem 3, the startup line's place, a device's `seek`. Tests: the `dev` module's (Task 15), `a_character_device_s_seek_gives_0` (Tasks 13, 14), the scenarios `boot` and `mount_fail`.

---

## PR 1: The spec's §15 item 6, the roadmap's notes and this plan

The spec's §15 item 6, the decisions of this plan, with the §10 lines and the status line it changes; the roadmap's status and its notes for plans 2 to 4; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and the roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 5 plan 2's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p2/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m5p2/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-03-m5-plan-2-abi-4-and-dev.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-03-m5-plan-2-abi-4-and-dev.md
git commit -m "docs(plan): add milestone 5 plan 2, ABI 4 and /dev"
cargo xtask lint
git push -u origin m5p2/plan
gh pr create --base main --head m5p2/plan --title "docs(spec,plan): add milestone 5 plan 2, ABI 4 and /dev" --body-file - <<'EOF2'
## What

The implementation plan of milestone 5's plan 2 ("ABI 4 and /dev"), with the spec's §15 item 6: three pull requests and the spike; one ABI bump for `SpawnArgs::env` and `StatFs::flags`; the environment below the arguments on a stack that stays 255 pages (the spike's measurement); `relay_rt::env`, `sys::spawn_env` and `/bin/sh` handing its block on until plan 3; `t-env`; the read-only flag and `test -w` as GNU's (a device or a FIFO stays writable); `/dev/null` (`DevFs`, `EPERM` for changes in `/dev`, a character device's `seek`, `cp` reading it, the mount before `/bin`); `df` listing only `/`; no device numbers; `sh` running only a regular file; plan 1's deferred minors (one corrected here, two for plan 4). §10 gains two lines and loses `test -w`'s; the roadmap's notes carry what plans 3 and 4 inherit.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (the harness's `bind_pr`); never poll CI. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m5p2/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-plan` (`.superpowers` is not linked there) and continue with PR 2.

---

## PR 2: ABI 4, the environment and the read-only flag (Tasks 1–9)

ABI 4 in one bump: `SpawnArgs` gains the environment and `StatFs` a read-only flag. The kernel checks the environment and lays it below the arguments, the entry passes it in `rcx`, `r8` and `r9`, relay-rt reads it, init gives the shell `HOME=/root`, and `t-env` and the scenario `env_calls` check it end to end; each filesystem says whether it is read-only, and `test -w` answers from it, compared with GNU's.

Branch `m5p2/abi`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-abi`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p2/abi /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-abi origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-abi
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p2/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p2/abi /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-abi m5p2/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p2/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: ABI 4: `SpawnArgs::env` and `StatFs::flags`

Decisions 1 and 2: one `VERSION` bump for both layouts. `SpawnArgs` gains `env` and `env_len` (offsets 128 and 136; 144 bytes), `StatFs` gains `flags` (offset 48; 56 bytes) with `STATFS_READ_ONLY = 1`, Linux's `ST_RDONLY`, and `relay_abi::VERSION` becomes 4. The version ripples as plan 2's spike found, every scenario passing once they are made: the startup line `ABI 4` in `boot`, `system`, `system_nosh` and `utils`, `kernel wants 4` in `system_abi` and the error screen's test, `t-abi` built for ABI 3 (`stale_program`, xtask's `STALE_ABI` test), check 3's expectations and its transcripts (the NUC's by hand, by a command: they hold escape bytes), `docs/hardware-test.md`. Until Task 2 lays the environment on the stack, the kernel refuses one (`EINVAL`), as milestone 3's bump refused `pgid` until it had a meaning; the kernel and relay-rt's test double fill `flags` with 0 until Task 7. The red runs are relay-abi's tests and the kernel's: the fields are missing. Mutation check (1): the kernel's refusal of an environment, removed, fails a test.

**Files:**
- Modify: `crates/relay-abi/src/file.rs`
- Modify: `crates/relay-abi/src/lib.rs`
- Modify: `crates/relay-abi/src/spawn.rs`
- Modify: `crates/relay-rt/src/testing.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/error_screen.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/system.rs`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/stale_program.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `tests/e2e/system_abi.txt`
- Modify: `tests/e2e/system_nosh.txt`
- Modify: `tests/e2e/utils.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `relay_abi::SpawnArgs::{env, env_len}`, `relay_abi::StatFs::flags`, `relay_abi::file::STATFS_READ_ONLY`, `relay_abi::VERSION == 4`.

- [ ] **Step 1: Add the failing tests to `crates/relay-abi/src/file.rs`**

In `crates/relay-abi/src/file.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
        assert_eq!(offset_of!(Stat, dev), 72);
        assert_eq!(StatFs::SIZE, 48);
        assert_eq!(offset_of!(StatFs, block_size), 0);
````

with:

````rust
        assert_eq!(offset_of!(Stat, dev), 72);
        assert_eq!(StatFs::SIZE, 56);
        assert_eq!(offset_of!(StatFs, block_size), 0);
````

Replace:

````rust
        assert_eq!(offset_of!(StatFs, free_files), 40);
        assert_eq!(size_of::<DirEntry>(), 16);
````

with:

````rust
        assert_eq!(offset_of!(StatFs, free_files), 40);
        assert_eq!(offset_of!(StatFs, flags), 48);
        assert_eq!(size_of::<DirEntry>(), 16);
````

Replace:

````rust
        assert_eq!(STAT_NOFOLLOW, 1);
        assert_eq!(
````

with:

````rust
        assert_eq!(STAT_NOFOLLOW, 1);
        assert_eq!(STATFS_READ_ONLY, 1, "Linux's ST_RDONLY");
        assert_eq!(
````

Replace:

````rust
            free_files: 6,
        };
````

with:

````rust
            free_files: 6,
            flags: 7,
        };
````

- [ ] **Step 2: Add the failing tests to `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub const NOTE_TYPE: u32 = 1;
````

with:

````rust
pub const NOTE_TYPE: u32 = 1;

#[cfg(test)]
mod tests {
    #[test]
    fn the_version_is_4() {
        assert_eq!(super::VERSION, 4);
    }
}
````

- [ ] **Step 3: Add the failing tests to `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 128);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
````

with:

````rust
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 144);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
````

Replace:

````rust
        assert_eq!(offset_of!(SpawnArgs, reserved), 124);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
````

with:

````rust
        assert_eq!(offset_of!(SpawnArgs, reserved), 124);
        assert_eq!(offset_of!(SpawnArgs, env), 128);
        assert_eq!(offset_of!(SpawnArgs, env_len), 136);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
````

Replace:

````rust
            reserved: 55,
        };
````

with:

````rust
            reserved: 55,
            env: 0x40_4000,
            env_len: 66,
        };
````

- [ ] **Step 4: Extend the test support in `crates/relay-rt/src/testing.rs`**

In `crates/relay-rt/src/testing.rs`, replace:

````rust
            free_files: s.free_files,
        })
````

with:

````rust
            free_files: s.free_files,
            flags: 0,
        })
````

- [ ] **Step 5: Add the failing tests to `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, replace:

````rust
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 3"
        );
````

with:

````rust
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 4"
        );
````

- [ ] **Step 6: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            reserved: 0,
        };
````

with:

````rust
            reserved: 0,
            env: 0,
            env_len: 0,
        };
````

Replace:

````rust
            "reserved"
        );
````

with:

````rust
            "reserved"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.env_len = 2),
            Err(errno::EINVAL),
            "an environment, until the kernel lays it out"
        );
````

- [ ] **Step 7: Add the failing tests to `kernel/src/system.rs`**

In `kernel/src/system.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 3");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
````

with:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 4");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
````

Replace:

````rust
            mount_into(&mut one, archive(relay_abi::VERSION, &["t-args"])).unwrap(),
            "1 program, ABI 3"
        );
````

with:

````rust
            mount_into(&mut one, archive(relay_abi::VERSION, &["t-args"])).unwrap(),
            "1 program, ABI 4"
        );
````

Replace:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 3");
        assert_eq!(
````

with:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 4");
        assert_eq!(
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 3
expect \nWelcome to Relay OS\.\n
````

with:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 4
expect \nWelcome to Relay OS\.\n
````

- [ ] **Step 9: Expect the new lines in `tests/e2e/stale_program.txt`**

In `tests/e2e/stale_program.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# A program built for another ABI (user-space gate §5.2, §8.5, §12.3):
# t-abi is t-args whose note says ABI 2, as a program an older build left
# on the disk would. spawn refuses it with ENOEXEC, from /bin or copied to
````

with:

````text
# A program built for another ABI (user-space gate §5.2, §8.5, §12.3):
# t-abi is t-args whose note says ABI 3, as a program an older build left
# on the disk would. spawn refuses it with ENOEXEC, from /bin or copied to
````

Replace:

````text
send dmesg
expect \nspawn /root/t-abi: built for ABI 2\n
expect root@relay:~# $
````

with:

````text
send dmesg
expect \nspawn /root/t-abi: built for ABI 3\n
expect root@relay:~# $
````

- [ ] **Step 10: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, replace:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 3
expect root@relay:~# $
````

with:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 4
expect root@relay:~# $
````

- [ ] **Step 11: Expect the new lines in `tests/e2e/system_abi.txt`**

In `tests/e2e/system_abi.txt`, replace:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 3
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: ABI 99, kernel wants 3\n
expect \nPress any key to reboot\.\n
````

with:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 4
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: ABI 99, kernel wants 4\n
expect \nPress any key to reboot\.\n
````

- [ ] **Step 12: Expect the new lines in `tests/e2e/system_nosh.txt`**

In `tests/e2e/system_nosh.txt`, replace:

````text
timeout 30
expect \[ ok \] system: \d+ programs, ABI 3\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh cannot start: No such file or directory\n\n--- last kernel log lines ---\n
expect \[ ok \] system: \d+ programs, ABI 3\n
expect \nPress any key to reboot\.\n
````

with:

````text
timeout 30
expect \[ ok \] system: \d+ programs, ABI 4\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh cannot start: No such file or directory\n\n--- last kernel log lines ---\n
expect \[ ok \] system: \d+ programs, ABI 4\n
expect \nPress any key to reboot\.\n
````

- [ ] **Step 13: Expect the new lines in `tests/e2e/utils.txt`**

In `tests/e2e/utils.txt`, replace:

````text
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 3\n
expect root@relay:~# $
````

with:

````text
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 4\n
expect root@relay:~# $
````

- [ ] **Step 14: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
        assert_eq!(differ.len(), 1, "one byte of the version: 3 -> 2");
        assert_eq!(STALE_ABI, 2);
        let e = check_program(path("t-abi")).unwrap_err().to_string();
        assert!(e.contains("built for ABI 2, this is ABI 3"), "{e}");
        assert_eq!(
            kernel_check(path("t-abi")).unwrap_err().to_string(),
            "the kernel's check: built for ABI 2"
        );
````

with:

````rust
        assert_eq!(differ.len(), 1, "one byte of the version: 3 -> 2");
        assert_eq!(STALE_ABI, 3);
        let e = check_program(path("t-abi")).unwrap_err().to_string();
        assert!(e.contains("built for ABI 3, this is ABI 4"), "{e}");
        assert_eq!(
            kernel_check(path("t-abi")).unwrap_err().to_string(),
            "the kernel's check: built for ABI 3"
        );
````

- [ ] **Step 15: Run the tests to see them fail**

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` cannot find value `STATFS_READ_ONLY` in this scope ``; `` no field `flags` on type `file::StatFs` ``.

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` struct `relay_abi::SpawnArgs` has no field named `env` ``; `` struct `relay_abi::SpawnArgs` has no field named `env_len` ``.

- [ ] **Step 16: Change `crates/relay-abi/src/file.rs`**

In `crates/relay-abi/src/file.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub const STAT_NOFOLLOW: u32 = 1;

````

with:

````rust
pub const STAT_NOFOLLOW: u32 = 1;

/// [`StatFs::flags`]: the filesystem is read-only (Linux's `ST_RDONLY`).
pub const STATFS_READ_ONLY: u64 = 1;

````

Replace:

````rust
    pub free_files: u64,
}
````

with:

````rust
    pub free_files: u64,
    /// [`STATFS_READ_ONLY`] or 0.
    pub flags: u64,
}
````

Replace:

````rust
            self.free_files,
        ];
````

with:

````rust
            self.free_files,
            self.flags,
        ];
````

- [ ] **Step 17: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
/// header and into every program's ELF note, and checked by the kernel.
/// 3 since `SpawnArgs` gained `pgid` (milestone 3, spec §16 item 8).
pub const VERSION: u32 = 3;

````

with:

````rust
/// header and into every program's ELF note, and checked by the kernel.
/// 3 since `SpawnArgs` gained `pgid` (milestone 3, spec §16 item 8); 4
/// since it gained `env` and `StatFs` gained `flags` (milestone 5,
/// programmable shell gate §8.1, §15 item 6).
pub const VERSION: u32 = 4;

````

- [ ] **Step 18: Change `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub reserved: u32,
}
````

with:

````rust
    pub reserved: u32,
    /// The child's environment: `NAME=value` entries, each followed by a
    /// NUL (programmable shell gate §8.1); empty for none.
    pub env: u64,
    pub env_len: u64,
}
````

Replace:

````rust
            reserved: u32_at(124),
        }
````

with:

````rust
            reserved: u32_at(124),
            env: u64_at(128),
            env_len: u64_at(136),
        }
````

- [ ] **Step 19: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 44 programs, ABI 3`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

with:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 44 programs, ABI 4`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

Replace:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 44 programs, ABI 3` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

with:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 44 programs, ABI 4` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

- [ ] **Step 20: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
        || a.reserved != 0
    {
````

with:

````rust
        || a.reserved != 0
        // An environment comes once the kernel lays it on the stack (§8.1).
        || a.env_len != 0
    {
````

- [ ] **Step 21: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
        free_files: f.free_files,
    };
````

with:

````rust
        free_files: f.free_files,
        flags: 0,
    };
````

- [ ] **Step 22: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 44 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 44 programs, ABI 4` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 23: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 3
#> ...
````

with:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 4
#> ...
````

- [ ] **Step 24: Change `check3-a.nuc.log`'s startup line**

Run:

````bash
sed -i 's/\(system: 44 programs, \)ABI 3/\1ABI 4/' xtask/fixtures/checks/check3-a.nuc.log
grep -c 'ABI 4' xtask/fixtures/checks/check3-a.nuc.log
````

- [ ] **Step 25: Change `xtask/fixtures/checks/check3-a.qemu.log`**

In `xtask/fixtures/checks/check3-a.qemu.log`, replace:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] system: 1 program, ABI 3
+ ls -l /bin
````

with:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] system: 1 program, ABI 4
+ ls -l /bin
````

- [ ] **Step 26: Change `check3-b.nuc.log`'s startup line**

Run:

````bash
sed -i 's/\(system: 44 programs, \)ABI 3/\1ABI 4/' xtask/fixtures/checks/check3-b.nuc.log
grep -c 'ABI 4' xtask/fixtures/checks/check3-b.nuc.log
````

- [ ] **Step 27: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 26 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 397 tests.

- [ ] **Step 28: Run the `boot`, `system`, `system_abi`, `system_nosh`, `utils`, `stale_program`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_nosh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario stale_program`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 29: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 30: Commit**

````bash
git add crates docs kernel rootfs tests xtask
git commit -F - <<'EOF'
feat(relay-abi,kernel)!: give spawn an environment and statfs a flag

SpawnArgs gains env and env_len, the child's environment block
(offsets 128 and 136; 144 bytes), and StatFs gains flags, whose
STATFS_READ_ONLY (1, Linux's ST_RDONLY) says the filesystem is
read-only (offset 48; 56 bytes); the kernel refuses an environment
until it lays one on the stack. Both layouts change, so VERSION
becomes 4: the startup line says ABI 4, t-abi is built for ABI 3, and
check 3's expectations and transcripts follow. The kernel and the
runtime fill the new fields in later commits (programmable shell gate
§8.1, §15 item 6).

BREAKING CHANGE: programs built against ABI 3 must be rebuilt; the
kernel refuses them and a system.img of ABI 3.
EOF
````


### Task 2: The kernel lays the environment below the arguments

Decision 3 (spec §8.1, §8.2, §13): `spawn` copies the environment in after the working directory, at most 64 KiB apart from the arguments (`E2BIG`, decided by its length before anything is read), and a block that is not empty must end with a NUL (`EINVAL`); its entries are counted by their NULs, and nothing else is checked (an empty entry or one without `=` passes). `exec::load` takes the arguments and the environment as `Strings` (bytes and a count) and puts the environment just below the arguments, which keep the top of the stack, the stack pointer 16-byte aligned below both; `Entry` gains `env`, `env_len` and `envc` (all 0 for no environment), and `arch::user::enter` loads them into `rcx`, `r8` and `r9`, the entry function's fourth to sixth arguments. The stack stays 255 pages: the spike's 40 KiB for `/bin/sh` at the nesting bound against 892 KiB left with both blocks full, which a test pins. The red run is the kernel's tests: the fields and `ENV_MAX` are missing. Mutation checks (8): the bound doubled in `spawn` and in `load`, the final-NUL check, the count, the environment's place, its address when there is none, the stack pointer, and the count passed on, each broken, fail a test; the registers are checked end to end by Tasks 5 and 6.

**Files:**
- Modify: `kernel/src/arch/user.rs`
- Modify: `kernel/src/exec.rs`
- Modify: `kernel/src/init.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`

**Interfaces:**
- Consumes: Task 1's `SpawnArgs::{env, env_len}`.
- Produces: `exec::ENV_MAX`, `exec::Strings { bytes, count }` (`Strings::NONE`), `exec::load(space, mem, file, program, args: Strings, env: Strings)`, `exec::Entry::{env, env_len, envc}`, `syscall::Spawn::{env, envc}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, make these 10 replacements, top to bottom:

Replace:

````rust

    fn space(m: &mut FakeMem) -> AddressSpace {
````

with:

````rust

    fn strings(bytes: &[u8], count: u64) -> Strings<'_> {
        Strings { bytes, count }
    }

    fn space(m: &mut FakeMem) -> AddressSpace {
````

Replace:

````rust
        let args = arg_bytes(&[b"/bin/t"]);
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
        // Code from 0x401010, zeroes before it on its page.
````

with:

````rust
        let args = arg_bytes(&[b"/bin/t"]);
        load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE).unwrap();
        // Code from 0x401010, zeroes before it on its page.
````

Replace:

````rust
        let (file, p) = program();
        load(&mut s, &mut m, &file, &p, &arg_bytes(&[b"x"]), 1).unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
````

with:

````rust
        let (file, p) = program();
        load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&arg_bytes(&[b"x"]), 1),
            Strings::NONE,
        )
        .unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
````

Replace:

````rust
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
        let e = load(&mut s, &mut m, &file, &p, &args, 4).unwrap();
        assert_eq!(e.ip, 0x40_1010);
````

with:

````rust
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
        let e = load(&mut s, &mut m, &file, &p, strings(&args, 4), Strings::NONE).unwrap();
        assert_eq!(e.ip, 0x40_1010);
````

Replace:

````rust
        assert!(e.sp <= e.args && e.args - e.sp < 16);
        s.destroy(&mut m);
````

with:

````rust
        assert!(e.sp <= e.args && e.args - e.sp < 16);
        // No environment: its address, length and count are all 0.
        assert_eq!((e.env, e.env_len, e.envc), (0, 0, 0));
        s.destroy(&mut m);
    }

    #[test]
    fn the_environment_is_just_below_the_arguments() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-env", b"x"]);
        let env = arg_bytes(&[b"HOME=/root", "A=\u{e9}t\u{e9}".as_bytes(), b""]);
        let e = load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&args, 2),
            strings(&env, 3),
        )
        .unwrap();
        assert_eq!(e.args + e.args_len, STACK_TOP, "the arguments keep the top");
        assert_eq!((e.env_len, e.envc), (env.len() as u64, 3));
        assert_eq!(e.env + e.env_len, e.args, "just below them");
        assert_eq!(read(&mut m, &s, e.env, env.len()), env);
        assert_eq!(read(&mut m, &s, e.args, args.len()), args);
        assert_eq!(e.sp % 16, 0);
        assert!(e.sp <= e.env && e.env - e.sp < 16);
        s.destroy(&mut m);
````

Replace:

````rust
        let (file, p) = program();
        let e = load(&mut s, &mut m, &file, &p, &args, 2).unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
````

with:

````rust
        let (file, p) = program();
        let e = load(&mut s, &mut m, &file, &p, strings(&args, 2), Strings::NONE).unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
````

Replace:

````rust
        assert_eq!(
            load(&mut s2, &mut m, &file, &p, &too_many, 1),
            Err(Errno::E2BIG)
````

with:

````rust
        assert_eq!(
            load(
                &mut s2,
                &mut m,
                &file,
                &p,
                strings(&too_many, 1),
                Strings::NONE
            ),
            Err(Errno::E2BIG)
        );
        s.destroy(&mut m);
        s2.destroy(&mut m);
    }

    #[test]
    fn both_blocks_at_64_kib_leave_the_stack_its_room() {
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]);
        let entry = [&b"A="[..], &vec![b'v'; ENV_MAX - 3]].concat();
        let env = arg_bytes(&[&entry]);
        assert_eq!((args.len(), env.len()), (ARGS_MAX, ENV_MAX));
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let e = load(
            &mut s,
            &mut m,
            &file,
            &p,
            strings(&args, 2),
            strings(&env, 1),
        )
        .unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
        assert_eq!(e.env, e.args - ENV_MAX as u64);
        assert_eq!(read(&mut m, &s, e.env, 3), b"A=v");
        assert_eq!(read(&mut m, &s, e.args - 2, 4), b"v\0p\0");
        // 892 KiB below them: /bin/sh at its nesting bound used 40 KiB
        // (programmable shell gate §15 item 6).
        assert_eq!(e.sp, e.env);
        assert_eq!(e.sp - STACK_BOTTOM, 892 * 1024);
        let mut s2 = space(&mut m);
        let too_much = vec![1; ENV_MAX + 1];
        assert_eq!(
            load(
                &mut s2,
                &mut m,
                &file,
                &p,
                strings(&args, 2),
                strings(&too_much, 0)
            ),
            Err(Errno::E2BIG)
````

Replace:

````rust
        assert_eq!(
            load(&mut s, &mut m, &file[..0x3000], &p, &args, 1),
            Err(Errno::ENOEXEC)
````

with:

````rust
        assert_eq!(
            load(
                &mut s,
                &mut m,
                &file[..0x3000],
                &p,
                strings(&args, 1),
                Strings::NONE
            ),
            Err(Errno::ENOEXEC)
````

Replace:

````rust
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
        let needed = m.frames() - before;
````

with:

````rust
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE).unwrap();
        let needed = m.frames() - before;
````

Replace:

````rust
            assert_eq!(
                load(&mut s, &mut m, &file, &p, &args, 1),
                Err(Errno::ENOMEM)
````

with:

````rust
            assert_eq!(
                load(&mut s, &mut m, &file, &p, strings(&args, 1), Strings::NONE),
                Err(Errno::ENOMEM)
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

    /// A `SpawnArgs` at `W` naming a path, arguments and a working
    /// directory stored after it; `edit` changes it first.
    fn spawn_args(f: &mut Fake, args: &[u8], edit: impl FnOnce(&mut SpawnArgs)) -> u64 {
````

with:

````rust

    /// The environment `spawn_args` stores, which a test gives the child
    /// by setting `env_len`.
    const ENV: &[u8] = b"HOME=/root\0A=\xc3\xa9\0";

    /// A `SpawnArgs` at `W` naming a path, arguments and a working
    /// directory stored after it, and no environment; `edit` changes it
    /// first.
    fn spawn_args(f: &mut Fake, args: &[u8], edit: impl FnOnce(&mut SpawnArgs)) -> u64 {
````

Replace:

````rust
        put(f, W + 400, args);
        let mut fds = [FdMap::default(); SPAWN_FDS];
````

with:

````rust
        put(f, W + 400, args);
        put(f, W + 800, ENV);
        let mut fds = [FdMap::default(); SPAWN_FDS];
````

Replace:

````rust
            reserved: 0,
            env: 0,
            env_len: 0,
````

with:

````rust
            reserved: 0,
            env: W + 800,
            env_len: 0,
````

Replace:

````rust
                foreground: false,
            }]
````

with:

````rust
                foreground: false,
                env: Vec::new(),
                envc: 0,
            }]
````

Replace:

````rust
        assert_eq!(f.spawned[3].group, Group::Join(102));
        // The caller's refusal.
````

with:

````rust
        assert_eq!(f.spawned[3].group, Group::Join(102));
        // An environment, counted by its NULs.
        let a = spawn_args(&mut f, b"x\0", |a| a.env_len = ENV.len() as u64);
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(105));
        assert_eq!((&f.spawned[4].env[..], f.spawned[4].envc), (ENV, 2));
        // The caller's refusal.
````

Replace:

````rust
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.env_len = 2),
            Err(errno::EINVAL),
            "an environment, until the kernel lays it out"
        );
````

with:

````rust
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.env_len = 4),
            Err(errno::EINVAL),
            "an environment that does not end with a NUL"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.env_len = ENV_MAX as u64 + 1),
            Err(errno::E2BIG),
            "an environment past 64 KiB, apart from the arguments"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| {
                a.env = 0;
                a.env_len = 2;
            }),
            Err(errno::EFAULT),
            "an environment at null"
        );
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Strings` in this scope ``; `` cannot find struct, variant or union type `Strings` in this scope ``.

- [ ] **Step 4: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
/// stack, `rdi` = the arguments' address, `rsi` their length, `rdx` their
/// count, interrupts on, every other register zero. The kernel stack this
/// is called on is where the program's system calls, interrupts and
````

with:

````rust
/// stack, `rdi` = the arguments' address, `rsi` their length, `rdx` their
/// count, `rcx`, `r8` and `r9` the environment's (programmable shell gate
/// §8.2), interrupts on, every other register zero. The kernel stack this
/// is called on is where the program's system calls, interrupts and
````

Replace:

````rust
        "mov rdx, [rdi + {argc}]",
        "mov rdi, [rdi + {args}]",
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
````

with:

````rust
        "mov rdx, [rdi + {argc}]",
        "mov rcx, [rdi + {env}]",
        "mov r8, [rdi + {env_len}]",
        "mov r9, [rdi + {envc}]",
        "mov rdi, [rdi + {args}]",
        "xor eax, eax", "xor ebx, ebx", "xor ebp, ebp",
        "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
````

Replace:

````rust
        argc = const core::mem::offset_of!(Entry, argc),
    )
````

with:

````rust
        argc = const core::mem::offset_of!(Entry, argc),
        env = const core::mem::offset_of!(Entry, env),
        env_len = const core::mem::offset_of!(Entry, env_len),
        envc = const core::mem::offset_of!(Entry, envc),
    )
````

- [ ] **Step 5: Change `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! of the lower half with an unmapped guard page beneath it, and its
//! arguments at the top of the stack. Architecture-neutral; the registers
//! the entry state goes into are the `arch` module's business.

````

with:

````rust
//! of the lower half with an unmapped guard page beneath it, and its
//! arguments at the top of the stack with its environment just below them
//! (programmable shell gate §8.1). Architecture-neutral; the registers the
//! entry state goes into are the `arch` module's business.

````

Replace:

````rust
pub const ARGS_MAX: usize = 64 * 1024;

/// Where a program starts: its first instruction, its stack pointer (16-byte
/// aligned, below the arguments), and its arguments' address, length and
/// count (spec §5.3). `arch::user::enter` reads it (`repr(C)`) and puts
/// each where its architecture says.
#[repr(C)]
````

with:

````rust
pub const ARGS_MAX: usize = 64 * 1024;
/// The most environment bytes a program gets, counted apart from the
/// arguments (programmable shell gate §8.1).
pub const ENV_MAX: usize = 64 * 1024;

/// Where a program starts: its first instruction, its stack pointer (16-byte
/// aligned, below the arguments and the environment), its arguments'
/// address, length and count (spec §5.3), and its environment's, all 0
/// for none (programmable shell gate §8.2). `arch::user::enter` reads it
/// (`repr(C)`) and puts each where its architecture says.
#[repr(C)]
````

Replace:

````rust
    pub argc: u64,
}
````

with:

````rust
    pub argc: u64,
    pub env: u64,
    pub env_len: u64,
    pub envc: u64,
}
````

Replace:

````rust

/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` (`argc` of them, each ending in its NUL) at the top
/// of the stack. On an error the caller destroys `space`, which gives
/// back whatever was mapped.
pub fn load(
````

with:

````rust

/// Strings for the new stack, the arguments or the environment: `count` of
/// them, each followed by a NUL, in `bytes`.
#[derive(Clone, Copy, Debug)]
pub struct Strings<'a> {
    pub bytes: &'a [u8],
    pub count: u64,
}

impl Strings<'_> {
    /// No strings: no environment.
    pub const NONE: Strings<'static> = Strings {
        bytes: &[],
        count: 0,
    };
}

/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` at the top of the stack and `env` just below
/// them. On an error the caller destroys `space`, which gives back whatever
/// was mapped.
pub fn load(
````

Replace:

````rust
    program: &Program,
    args: &[u8],
    argc: u64,
) -> Result<Entry, Errno> {
````

with:

````rust
    program: &Program,
    args: Strings<'_>,
    env: Strings<'_>,
) -> Result<Entry, Errno> {
````

Replace:

````rust
        .map_err(errno)?;
    if args.len() > ARGS_MAX {
        return Err(Errno::E2BIG);
    }
    let at = STACK_TOP - args.len() as u64;
    space.fill(mem, at, args).map_err(errno)?;
    Ok(Entry {
        ip: program.entry,
        sp: at & !15,
        args: at,
        args_len: args.len() as u64,
        argc,
    })
````

with:

````rust
        .map_err(errno)?;
    if args.bytes.len() > ARGS_MAX || env.bytes.len() > ENV_MAX {
        return Err(Errno::E2BIG);
    }
    let at = STACK_TOP - args.bytes.len() as u64;
    space.fill(mem, at, args.bytes).map_err(errno)?;
    let env_at = at - env.bytes.len() as u64;
    space.fill(mem, env_at, env.bytes).map_err(errno)?;
    Ok(Entry {
        ip: program.entry,
        sp: env_at & !15,
        args: at,
        args_len: args.bytes.len() as u64,
        argc: args.count,
        env: if env.bytes.is_empty() { 0 } else { env_at },
        env_len: env.bytes.len() as u64,
        envc: env.count,
    })
````

- [ ] **Step 6: Change `kernel/src/init.rs`**

In `kernel/src/init.rs`, replace:

````rust
        foreground: true,
    })
````

with:

````rust
        foreground: true,
        env: Vec::new(),
        envc: 0,
    })
````

- [ ] **Step 7: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(&mut space, mem, &file, &program, &s.args, s.argc) {
            Ok(entry) => Ok((space, entry)),
````

with:

````rust
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(
            &mut space,
            mem,
            &file,
            &program,
            exec::Strings {
                bytes: &s.args,
                count: s.argc,
            },
            exec::Strings {
                bytes: &s.env,
                count: s.envc,
            },
        ) {
            Ok(entry) => Ok((space, entry)),
````

- [ ] **Step 8: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

use crate::exec::ARGS_MAX;
use crate::fd::{FdTable, File};
````

with:

````rust

use crate::exec::{ARGS_MAX, ENV_MAX};
use crate::fd::{FdTable, File};
````

Replace:

````rust
    pub foreground: bool,
}
````

with:

````rust
    pub foreground: bool,
    /// The environment, each entry followed by a NUL, and how many entries
    /// there are; empty for none (programmable shell gate §8.1).
    pub env: Vec<u8>,
    pub envc: u64,
}
````

Replace:

````rust
/// `spawn(&SpawnArgs)` (spec §7.3): the struct, then the path, the
/// arguments and the working directory it points at, each checked and
/// copied in before anything starts.
fn spawn(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
````

with:

````rust
/// `spawn(&SpawnArgs)` (spec §7.3): the struct, then the path, the
/// arguments, the working directory and the environment it points at, each
/// checked and copied in before anything starts.
fn spawn(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
````

Replace:

````rust
        || a.reserved != 0
        // An environment comes once the kernel lays it on the stack (§8.1).
        || a.env_len != 0
    {
````

with:

````rust
        || a.reserved != 0
    {
````

Replace:

````rust
    )?)?;
    let s = Spawn {
````

with:

````rust
    )?)?;
    // At most 64 KiB apart from the arguments, and each entry ends with
    // its NUL (programmable shell gate §8.1).
    let env = caller.read_str(&UserStr::new(a.env, a.env_len, ENV_MAX, Errno::E2BIG)?)?;
    if !env.is_empty() && env.last() != Some(&0) {
        return Err(Errno::EINVAL);
    }
    let envc = env.iter().filter(|&&b| b == 0).count() as u64;
    let s = Spawn {
````

Replace:

````rust
        foreground,
    };
````

with:

````rust
        foreground,
        env,
        envc,
    };
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 399 tests.

- [ ] **Step 10: Run the `programs`, `spawn`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
feat(kernel): lay a program's environment below its arguments

spawn copies in SpawnArgs::env: at most 64 KiB apart from the
arguments (E2BIG), and a block that is not empty ends with a NUL
(EINVAL); each entry is counted by its NUL. exec::load puts it just
below the arguments, which keep the top of the stack, with the stack
pointer 16-byte aligned below both, and the program starts with its
address, length and count in rcx, r8 and r9, all 0 for none
(programmable shell gate §8.1, §8.2). Both blocks at 64 KiB leave the
stack 892 KiB.
EOF
````


### Task 3: relay-rt reads the environment

Decision 4 (spec §8.3): `_start` passes `rcx`, `r8` and `r9` on to `start` untouched, which keeps the block before `main` runs; a new module, `relay_rt::env`, reads it as glibc and Rust's `std` read `environ`: `Block::vars` splits an entry at its first `=` after its first byte (so a name may start with `=`), skips one without, and `var` gives the first of two equal names (glibc's `getenv` probed, `tmp/m5p2/probes/p1-getenv.txt`); `block()` gives the block whole. `sys::spawn` keeps its arguments and starts a child with no environment, so its callers do not change; `sys::spawn_env` takes one, and `SysPrograms`, through which `/bin/sh` starts every program, passes the program's own, so the shell hands its whole environment on until plan 3's `export`. The red run is relay-rt's tests: the module is missing. Mutation checks (6): splitting at byte 0, the count unbounded, bytes after the last NUL, the last of two names, the value keeping its `=`, and the count kept, each broken, fail a test (the duplicate name's test came from its surviving mutant).

**Files:**
- Modify: `crates/relay-rt/src/arch.rs`
- Create: `crates/relay-rt/src/env.rs`
- Modify: `crates/relay-rt/src/lib.rs`
- Modify: `crates/relay-rt/src/start.rs`
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `crates/relay-rt/src/sysio.rs`

**Interfaces:**
- Consumes: Task 2's entry registers; Task 1's `SpawnArgs::{env, env_len}`.
- Produces: `relay_rt::env::{Block, program, block, vars, var}`, `Block::{new, entries, vars, var}`, `relay_rt::sys::spawn_env(path, args, env, cwd, fds, flags, pgid)`; `start` takes six arguments.

- [ ] **Step 1: Write the failing tests for `crates/relay-rt/src/env.rs`**

Create `crates/relay-rt/src/env.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(b: Block) -> Vec<(&'static [u8], &'static [u8])> {
        b.vars().collect()
    }

    #[test]
    fn each_entry_ends_with_a_nul() {
        let b = Block::new(b"HOME=/root\0A=\xc3\xa9t\xc3\xa9\0B=\0", 3);
        assert_eq!(
            b.entries().collect::<Vec<_>>(),
            [&b"HOME=/root"[..], "A=été".as_bytes(), b"B="]
        );
        assert_eq!(
            pairs(b),
            [
                (&b"HOME"[..], &b"/root"[..]),
                (b"A", "été".as_bytes()),
                (b"B", b"")
            ]
        );
        assert_eq!(b.var(b"HOME"), Some(&b"/root"[..]));
        assert_eq!(b.var(b"B"), Some(&b""[..]));
        assert_eq!(b.var(b"HOM"), None);
        assert_eq!(b.var(b"C"), None);
    }

    #[test]
    fn a_name_is_split_at_its_first_equals_sign_after_its_first_byte() {
        // As glibc and Rust's std read `environ`: an entry without `=`
        // after its first byte is skipped, and a name may start with one.
        let b = Block::new(b"A=1=2\0=x=y\0none\0\0=\0", 5);
        assert_eq!(pairs(b), [(&b"A"[..], &b"1=2"[..]), (b"=x", b"y")]);
        assert_eq!(b.entries().count(), 5, "every entry is in the block");
        assert_eq!(b.var(b"=x"), Some(&b"y"[..]));
        assert_eq!(b.var(b"none"), None);
        assert_eq!(b.var(b""), None);
        // A name given twice is the first one's, as `getenv` finds it.
        let twice = Block::new(b"A=1\0A=2\0", 2);
        assert_eq!(twice.var(b"A"), Some(&b"1"[..]));
        assert_eq!(twice.vars().count(), 2);
    }

    #[test]
    fn the_count_and_the_last_nul_bound_the_block() {
        let b = Block::new(b"A=1\0B=2\0C=3", 9);
        assert_eq!(pairs(b), [(&b"A"[..], &b"1"[..]), (b"B", b"2")]);
        assert_eq!(pairs(Block::new(b"A=1\0B=2\0", 1)).len(), 1);
        assert_eq!(pairs(Block::new(b"", 0)), []);
        assert_eq!(Block::new(b"A=1\0", 0).var(b"A"), None);
    }

    #[test]
    fn the_program_s_block_is_empty_until_set() {
        assert_eq!(block(), b"");
        assert_eq!(var(b"HOME"), None);
        set(Block::new(b"HOME=/root\0X=1\0", 2));
        assert_eq!(block(), b"HOME=/root\0X=1\0");
        assert_eq!(var(b"X"), Some(&b"1"[..]));
        assert_eq!(vars().count(), 2);
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust
mod args;
// Off Relay OS only the tests use these.
````

with:

````rust
mod args;
pub mod env;
// Off Relay OS only the tests use these.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find type `Block` in this scope ``; `` cannot find function `var` in this scope ``.

- [ ] **Step 4: Change `crates/relay-rt/src/arch.rs`**

In `crates/relay-rt/src/arch.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! clobbers `rcx` and `r11`. And `_start`, which the kernel jumps to with
//! the arguments' address, length and count in `rdi`, `rsi`, `rdx`.

````

with:

````rust
//! clobbers `rcx` and `r11`. And `_start`, which the kernel jumps to with
//! the arguments' address, length and count in `rdi`, `rsi`, `rdx`, and the
//! environment's in `rcx`, `r8`, `r9`.

````

Replace:

````rust
/// (spec §5.3); a call leaves it as the SysV ABI expects on function
/// entry. `rdi`, `rsi` and `rdx` pass through to `start` untouched.
#[unsafe(naked)]
````

with:

````rust
/// (spec §5.3); a call leaves it as the SysV ABI expects on function
/// entry. `rdi`, `rsi`, `rdx`, `rcx`, `r8` and `r9` pass through to `start`
/// untouched, its six arguments.
#[unsafe(naked)]
````

- [ ] **Step 5: Implement `crates/relay-rt/src/env.rs`**

Insert this at the top of `crates/relay-rt/src/env.rs`, above `#[cfg(test)]`:

````rust
//! The environment a program was started with (programmable shell gate
//! §8.1–§8.3): `count` entries, each followed by a NUL, which the kernel
//! put just below the arguments. Entries are `NAME=value` by convention
//! only: the kernel checks nothing else, and a program reads them as glibc
//! and Rust's `std` read `environ`.

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// An environment block.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    bytes: &'static [u8],
    count: usize,
}

impl Block {
    pub fn new(bytes: &'static [u8], count: usize) -> Block {
        Block { bytes, count }
    }

    /// Every entry as it is, in the block's order. Bytes after the last NUL
    /// and entries beyond `count` do not count.
    pub fn entries(&self) -> impl Iterator<Item = &'static [u8]> + use<> {
        let bytes: &'static [u8] = self.bytes;
        bytes
            .split_inclusive(|&b| b == 0)
            .filter_map(|e| e.strip_suffix(&[0]))
            .take(self.count)
    }

    /// Each entry's name and value, split at the first `=` after its first
    /// byte (so a name may start with `=`); an entry without one is
    /// skipped, as glibc and Rust's `std` skip it.
    pub fn vars(&self) -> impl Iterator<Item = (&'static [u8], &'static [u8])> + use<> {
        self.entries().filter_map(|e| {
            let at = 1 + e.get(1..)?.iter().position(|&b| b == b'=')?;
            Some((&e[..at], &e[at + 1..]))
        })
    }

    /// The value of the first entry named `name`.
    pub fn var(&self, name: &[u8]) -> Option<&'static [u8]> {
        self.vars().find(|&(n, _)| n == name).map(|(_, v)| v)
    }
}

/// The program's block, set before `main` runs.
static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Off Relay OS only the tests set it.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
pub(crate) fn set(b: Block) {
    PTR.store(b.bytes.as_ptr().cast_mut(), Ordering::Relaxed);
    LEN.store(b.bytes.len(), Ordering::Relaxed);
    COUNT.store(b.count, Ordering::Relaxed);
}

/// The program's environment.
pub fn program() -> Block {
    let p = PTR.load(Ordering::Relaxed);
    let bytes: &'static [u8] = if p.is_null() {
        &[]
    } else {
        // SAFETY: set from a `&'static [u8]` in `set`.
        unsafe { core::slice::from_raw_parts(p, LEN.load(Ordering::Relaxed)) }
    };
    Block::new(bytes, COUNT.load(Ordering::Relaxed))
}

/// The program's whole block, as `spawn` takes one: empty for none.
pub fn block() -> &'static [u8] {
    program().bytes
}

/// The program's environment variables, in the block's order.
pub fn vars() -> impl Iterator<Item = (&'static [u8], &'static [u8])> {
    program().vars()
}

/// The value of the program's variable `name`.
pub fn var(name: &[u8]) -> Option<&'static [u8]> {
    program().var(name)
}

````

- [ ] **Step 6: Change `crates/relay-rt/src/start.rs`**

In `crates/relay-rt/src/start.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use crate::args::Args;
    use crate::sys;

    /// Runs the program: the arguments the kernel laid out at `ptr` (`len`
    /// bytes, `count` arguments), then `main`, whose result is the exit status.
    ///
    /// # Safety
    /// `ptr` and `len` must describe memory that stays readable for the whole
    /// run (the kernel puts the arguments above the stack).
    pub unsafe extern "sysv64" fn start(ptr: *const u8, len: usize, count: usize) -> ! {
        unsafe extern "Rust" {
````

with:

````rust
    use crate::args::Args;
    use crate::{env, sys};

    /// Runs the program: the arguments the kernel laid out at `ptr` (`len`
    /// bytes, `count` arguments) and the environment at `env` (`env_len`
    /// bytes, `env_count` entries; programmable shell gate §8.2), then
    /// `main`, whose result is the exit status.
    ///
    /// # Safety
    /// `ptr` and `len`, and `env` and `env_len`, must describe memory that
    /// stays readable for the whole run (the kernel puts both above the
    /// stack).
    pub unsafe extern "sysv64" fn start(
        ptr: *const u8,
        len: usize,
        count: usize,
        env: *const u8,
        env_len: usize,
        env_count: usize,
    ) -> ! {
        unsafe extern "Rust" {
````

Replace:

````rust
        set_name(args.name());
        let code = unsafe { __relay_main(args) };
````

with:

````rust
        set_name(args.name());
        let env_bytes: &'static [u8] = if env.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(env, env_len) }
        };
        env::set(env::Block::new(env_bytes, env_count));
        let code = unsafe { __relay_main(args) };
````

- [ ] **Step 7: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// Starts the program at `path` with `args` (each followed by a NUL,
/// argument 0 first) in `cwd` (empty: this program's), giving it the fds
/// `fds` names (child, parent) and closing its others; with `NEW_GROUP` in
/// `flags` it starts a process group of its own, which `FOREGROUND` also
/// gives the console, in line mode; without it, a `pgid` other than 0 is
/// the group of another child of this program's that it joins. Its pid.
pub fn spawn(
    path: &[u8],
    args: &[u8],
    cwd: &[u8],
````

with:

````rust
/// Starts the program at `path` with `args` (each followed by a NUL,
/// argument 0 first) and no environment in `cwd` (empty: this program's),
/// giving it the fds `fds` names (child, parent) and closing its others;
/// with `NEW_GROUP` in `flags` it starts a process group of its own, which
/// `FOREGROUND` also gives the console, in line mode; without it, a `pgid`
/// other than 0 is the group of another child of this program's that it
/// joins. Its pid.
pub fn spawn(
    path: &[u8],
    args: &[u8],
    cwd: &[u8],
    fds: &[FdMap],
    flags: u32,
    pgid: u32,
) -> Result<u32, u16> {
    spawn_env(path, args, b"", cwd, fds, flags, pgid)
}

/// [`spawn`] with the environment `env`: entries each followed by a NUL,
/// at most 64 KiB (programmable shell gate §8.1); `crate::env::block()`
/// passes on this program's own.
pub fn spawn_env(
    path: &[u8],
    args: &[u8],
    env: &[u8],
    cwd: &[u8],
````

Replace:

````rust
        pgid,
        ..SpawnArgs::default()
````

with:

````rust
        pgid,
        env: env.as_ptr() as u64,
        env_len: env.len() as u64,
        ..SpawnArgs::default()
````

- [ ] **Step 8: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        let fds = command_fds(fds);
        let pid = sys::spawn(path, &arg_bytes(args), b"", &fds, flags, pgid)
            .map_err(Errno::from_number)?;
````

with:

````rust
        let fds = command_fds(fds);
        // This program's own environment, until the shell exports
        // variables (programmable shell gate §15 item 6).
        let env = crate::env::block();
        let pid = sys::spawn_env(path, &arg_bytes(args), env, b"", &fds, flags, pgid)
            .map_err(Errno::from_number)?;
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 33 tests.

- [ ] **Step 10: Run the `programs`, `sh`, `pipes`, `jobs` scenarios**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(relay-rt): read the environment a program starts with

_start passes rcx, r8 and r9 on to start, which keeps the block for
relay_rt::env: var(name) and vars() read it as glibc and Rust's std
read environ (a name ends at the first = after its first byte, an
entry without one is skipped, the first of two names wins), block()
gives it whole. sys::spawn_env gives a child an environment and
sys::spawn none; SysPrograms passes the program's own, so /bin/sh
hands its environment to every command until it exports variables
(programmable shell gate §8.3, §15 item 6).
EOF
````


### Task 4: init starts the shell with `HOME=/root`

Spec §8.5: init gives `/bin/sh` the environment `HOME=/root`, which the shell hands on to its commands (Task 3). The request init makes becomes a function of its own, `init::shell_request`, tested on the host. The red run is the kernel's tests: the function is missing.

**Files:**
- Modify: `kernel/src/init.rs`

**Interfaces:**
- Consumes: Task 2's `Spawn::{env, envc}`.
- Produces: `init::shell_request() -> Spawn`.

- [ ] **Step 1: Add the failing tests to `kernel/src/init.rs`**

In `kernel/src/init.rs`, replace:

````rust
    #[test]
    fn init_says_how_the_shell_ended() {
````

with:

````rust
    #[test]
    fn the_shell_starts_with_home_set() {
        let s = shell_request();
        assert_eq!(
            (&s.path[..], &s.args[..], s.argc),
            (&b"/bin/sh"[..], &b"/bin/sh\0"[..], 1)
        );
        assert_eq!((&s.env[..], s.envc), (&b"HOME=/root\0"[..], 1));
        assert!(s.cwd.is_empty() && s.group == Group::New && s.foreground);
        let fds: Vec<(u32, u32)> = s.fds.iter().map(|m| (m.child, m.parent)).collect();
        assert_eq!(fds, [(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn init_says_how_the_shell_ended() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `shell_request` in this scope ``.

- [ ] **Step 3: Change `kernel/src/init.rs`**

In `kernel/src/init.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! kernel's own, without a program. It prints `/etc/motd`, starts
//! `/bin/sh` in `/root` (or `/` without one) as its child, collects every
//! orphan as it ends, and when the shell ends says how and starts another.
````

with:

````rust
//! kernel's own, without a program. It prints `/etc/motd`, starts
//! `/bin/sh` in `/root` (or `/` without one) as its child with the
//! environment `HOME=/root` (programmable shell gate §8.5), collects every
//! orphan as it ends, and when the shell ends says how and starts another.
````

Replace:

````rust
const HOME: &[u8] = b"/root";
/// The most of `/etc/motd` shown.
````

with:

````rust
const HOME: &[u8] = b"/root";
/// The shell's environment (programmable shell gate §8.5).
const ENV: &[u8] = b"HOME=/root\0";
/// The most of `/etc/motd` shown.
````

Replace:

````rust

/// Starts `/bin/sh` without arguments in `/root`, or in `/` without one
/// (the empty read-only root the kernel falls back to), as a group of its
/// own with the console, its fds 0-2 the console; its pid.
fn start_shell() -> Result<u32, Errno> {
````

with:

````rust

/// Starts `/bin/sh` in `/root`, or in `/` without one (the empty read-only
/// root the kernel falls back to): [`shell_request`]; its pid.
fn start_shell() -> Result<u32, Errno> {
````

Replace:

````rust
    }
    let fds = [0, 1, 2].map(|fd| FdMap {
````

with:

````rust
    }
    proc::spawn(&shell_request())
}

/// `/bin/sh` without arguments, in init's current directory, as a group of
/// its own with the console, its fds 0-2 the console, and its environment
/// [`ENV`].
fn shell_request() -> Spawn {
    let fds = [0, 1, 2].map(|fd| FdMap {
````

Replace:

````rust
    args.push(0);
    proc::spawn(&Spawn {
        path: SHELL.into(),
````

with:

````rust
    args.push(0);
    Spawn {
        path: SHELL.into(),
````

Replace:

````rust
        foreground: true,
        env: Vec::new(),
        envc: 0,
    })
}
````

with:

````rust
        foreground: true,
        env: ENV.into(),
        envc: 1,
    }
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 400 tests.

- [ ] **Step 5: Run the `boot`, `shell` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
feat(kernel): start the shell with HOME=/root

init gives /bin/sh the environment HOME=/root (programmable shell gate
§8.5), which the shell hands on to the commands it starts. The request
init makes is a function of its own, tested on the host.
EOF
````


### Task 5: `t-env` and the scenario `env_calls`

Decision 5 (spec §11.3): a test program, `t-env`, prints the environment it started with, a line with its count and length and then `[n] entry` for each; its kinds start children with a given block (non-ASCII text, an entry without `=`, an empty one), with none, with 64 KiB and one byte more (`E2BIG`), with 64 KiB of arguments as well, without a final NUL (`EINVAL`), `sh` reading `t-env` from a pipe (a grandchild), and `sh` running a script at the 32-level nesting bound with both blocks full. The scenario `env_calls` runs them all, and `t-env` from the prompt shows init's `HOME=/root` through `/bin/sh`. `/bin` holds 45 programs: `ls /bin` in `system` changes, and the count in `kernel/src/system.rs` and `docs/hardware-test.md`. Tests only: no failing run. Mutation checks (5, against `env_calls`): `r8` and `r9` swapped, `rcx` given the arguments' address, `SysPrograms` passing no environment, `start` dropping the count, and init giving none, each fail the scenario.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`
- Create: `tests/e2e/env_calls.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-env.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces: the program `/bin/t-env`; the scenario `env_calls`.

- [ ] **Step 1: Add the scenario `tests/e2e/env_calls.txt`**

Create `tests/e2e/env_calls.txt`:

````text
# The environment a program starts with (programmable shell gate
# §8.1–§8.3, §11.3; milestone 5, plan 2): init's HOME=/root reaching a
# command through /bin/sh; a child's block with non-ASCII text, an entry
# without = and an empty one; none; 64 KiB, and E2BIG one byte past it;
# 64 KiB of arguments and of environment at once; EINVAL for a block
# without its final NUL; a grandchild's through sh; and a script at the
# shell's nesting bound with both blocks full.
timeout 60
expect root@relay:~# $
send t-env
expect \n1 entry, 11 bytes\n\[0\] HOME=/root\nroot@relay:~# $
send t-env var HOME; t-env var NOPE
expect \n/root\nunset\nroot@relay:~# $
send t-env child
expect \n5 entries, 30 bytes\n\[0\] A=1\n\[1\] B=two words\n\[2\] C=été\n\[3\] none\n\[4\] \nroot@relay:~# $
send t-env none
expect \n0 entries, 0 bytes\nroot@relay:~# $
send t-env limits
expect \n64 KiB: 2 arguments, 12 bytes; 1 entry, 65536 bytes\none byte more: E2BIG\n64 KiB of each: 3 arguments, 65536 bytes; 1 entry, 65536 bytes\nno final NUL: EINVAL\nroot@relay:~# $
send t-env sh
expect \n2 entries, 15 bytes\n\[0\] X=1\n\[1\] HOME=/root\nroot@relay:~# $
send echo 'if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then echo deep; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi' > env-deep.sh
expect \nroot@relay:~# $
send t-env deep env-deep.sh
expect \n\+ if true; then if true; .* fi; fi\ndeep\nroot@relay:~# $
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
expect root@relay:~# $
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-files  t-proc   t-spin  tail   true\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-env    t-mem    t-read   t-sys   test   uname\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-fault  t-pipe   t-spawn  t-tee   touch  wc\n
expect root@relay:~# $
````

Replace:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-files  t-proc   t-spin  tail   true\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-env    t-mem    t-read   t-sys   test   uname\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-fault  t-pipe   t-spawn  t-tee   touch  wc\n
````

- [ ] **Step 3: Add the test program `userland/tests/src/bin/t-env.rs`**

Create `userland/tests/src/bin/t-env.rs`:

````rust
//! `t-env`: prints the environment it was started with (programmable shell
//! gate §8.1–§8.3, §11.3), a line with its count and length, as the
//! registers gave them, then `[n] <entry>` for each, so a scenario sees
//! exactly what `spawn` passed. More kinds:
//!
//! - `t-env var NAME` prints `relay_rt::env::var(NAME)`, or `unset`;
//! - `t-env sizes` prints its arguments' count and length and its
//!   environment's;
//! - `t-env child` starts `t-env` with an environment of non-ASCII text,
//!   an entry without `=` and an empty one; `t-env none` with none;
//! - `t-env limits` starts `t-env sizes` with 64 KiB of environment, then
//!   one byte more (`E2BIG`), then 64 KiB of arguments as well, then a
//!   block without its final NUL (`EINVAL`);
//! - `t-env sh` starts `/bin/sh` with an environment, reading `t-env` from
//!   a pipe: the grandchild's environment is what the shell got;
//! - `t-env deep FILE` starts `sh FILE` with 64 KiB of arguments and 64 KiB
//!   of environment at once, so a script at the shell's nesting bound runs
//!   on what is left of the stack.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
use relay_abi::{FdMap, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// The most a block holds (programmable shell gate §8.1).
const MAX: usize = 64 * 1024;

/// Standard input, output and error, as this program has them.
const STD: [FdMap; 3] = [
    FdMap {
        child: 0,
        parent: 0,
    },
    FdMap {
        child: 1,
        parent: 1,
    },
    FdMap {
        child: 2,
        parent: 2,
    },
];

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        None => {
            show();
            Ok(())
        }
        Some(b"var") => match args.get(2) {
            Some(name) => {
                let value = relay_rt::env::var(name).unwrap_or(b"unset");
                sys::write_all(1, value).and_then(|()| sys::write_all(1, b"\n"))
            }
            None => return usage(),
        },
        Some(b"sizes") => {
            sizes(&args);
            Ok(())
        }
        Some(b"child") => run(b"A=1\0B=two words\0C=\xc3\xa9t\xc3\xa9\0none\0\0", None),
        Some(b"none") => run(b"", None),
        Some(b"limits") => limits(),
        Some(b"sh") => sh(),
        Some(b"deep") => match args.get(2) {
            Some(file) => deep(file),
            None => return usage(),
        },
        Some(_) => return usage(),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-env: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-env [var NAME|sizes|child|none|limits|sh|deep FILE]\n",
    );
    2
}

/// `1 entry` or `N entries`.
fn entries(n: usize) -> &'static str {
    if n == 1 { "entry" } else { "entries" }
}

/// The count and length, then each entry.
fn show() {
    let env = relay_rt::env::program();
    let all: Vec<&[u8]> = env.entries().collect();
    let n = all.len();
    let _ = writeln!(
        Fd(1),
        "{n} {}, {} bytes",
        entries(n),
        relay_rt::env::block().len()
    );
    for (i, e) in all.iter().enumerate() {
        let _ = write!(Fd(1), "[{i}] ");
        let _ = sys::write_all(1, e);
        let _ = sys::write_all(1, b"\n");
    }
}

fn sizes(args: &Args) {
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    let n = relay_rt::env::program().entries().count();
    let _ = writeln!(
        Fd(1),
        "{} arguments, {len} bytes; {n} {}, {} bytes",
        args.len(),
        entries(n),
        relay_rt::env::block().len()
    );
}

/// Starts `t-env` (or `t-env <kind>`) with `env` and waits for it.
fn run(env: &[u8], kind: Option<&[u8]>) -> Result<(), u16> {
    let mut args = Vec::from(&b"t-env\0"[..]);
    if let Some(k) = kind {
        args.extend_from_slice(k);
        args.push(0);
    }
    start(b"/bin/t-env", &args, env, &STD)
}

/// Starts `path` and waits for it.
fn start(path: &[u8], args: &[u8], env: &[u8], fds: &[FdMap]) -> Result<(), u16> {
    let pid = sys::spawn_env(path, args, env, b"", fds, 0, 0)?;
    sys::wait(i64::from(pid), false)?;
    Ok(())
}

/// One entry, `A=vvv…`, of `len` bytes with its NUL.
fn entry_of(len: usize) -> Vec<u8> {
    let mut e = Vec::from(&b"A="[..]);
    e.resize(len - 1, b'v');
    e.push(0);
    e
}

/// `t-env sizes` and, after it, `len` bytes of arguments in all.
fn sizes_args(len: usize) -> Vec<u8> {
    let mut a = Vec::from(&b"t-env\0sizes\0"[..]);
    a.resize(len - 1, b'x');
    a.push(0);
    a
}

/// Each case as `<what>: `, then the child's `sizes` line or the error's
/// name.
fn limits() -> Result<(), u16> {
    let sizes = &b"t-env\0sizes\0"[..];
    let cases: [(&str, Vec<u8>, Vec<u8>); 4] = [
        ("64 KiB", sizes.into(), entry_of(MAX)),
        ("one byte more", sizes.into(), entry_of(MAX + 1)),
        ("64 KiB of each", sizes_args(MAX), entry_of(MAX)),
        ("no final NUL", sizes.into(), b"A=1".into()),
    ];
    for (what, args, env) in &cases {
        let _ = write!(Fd(1), "{what}: ");
        if let Err(e) = start(b"/bin/t-env", args, env, &STD) {
            let _ = writeln!(Fd(1), "{}", errno::name(e).unwrap_or("?"));
        }
    }
    Ok(())
}

/// `/bin/sh` with `X=1` and `HOME=/root`, reading `t-env` from a pipe.
fn sh() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let fds = [
        FdMap {
            child: 0,
            parent: r,
        },
        STD[1],
        STD[2],
    ];
    let pid = sys::spawn_env(b"/bin/sh", b"sh\0", b"X=1\0HOME=/root\0", b"", &fds, 0, 0)?;
    sys::close(r)?;
    sys::write_all(w, b"t-env\n")?;
    sys::close(w)?;
    sys::wait(i64::from(pid), false)?;
    Ok(())
}

/// `sh FILE` with both blocks full.
fn deep(file: &[u8]) -> Result<(), u16> {
    let mut args = Vec::from(&b"sh\0"[..]);
    args.extend_from_slice(file);
    args.push(0);
    args.resize(MAX - 1, b'x');
    args.push(0);
    start(b"/bin/sh", &args, &entry_of(MAX), &STD)
}
````

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 44 programs, ABI 4`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

with:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 45 programs, ABI 4`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

Replace:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 44 programs, ABI 4` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

with:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 45 programs, ABI 4` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

- [ ] **Step 5: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 44 programs, ABI 4` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 45 programs, ABI 4` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 6: Run the `env_calls`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario env_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add docs kernel tests userland
git commit -F - <<'EOF'
test(e2e): check the environment with t-env and env_calls

t-env prints the environment it started with, its count and length as
the registers gave them, then each entry; its kinds start children with
a given block: non-ASCII text, an entry without = and an empty one,
none, 64 KiB and one byte more (E2BIG), 64 KiB of arguments as well,
no final NUL (EINVAL), sh reading t-env from a pipe, and sh running a
script at the nesting bound with both blocks full. The scenario
env_calls runs them, and HOME=/root from the prompt (programmable
shell gate §11.3). /bin holds 45 programs.
EOF
````


### Task 6: `t-env` shows the registers as they came

Decision 5 (the prototype's review, M-4): `t-env` counted the entries itself, so a count register the entries did not show went unseen (the reviewer's `add r9, 3` after the load passed `env_calls`). `relay_rt::env::raw()` now gives the address, length and count `start` got, and `env::set` keeps them as they were; `t-env` prints its count and length from them, and `t-env none` starts `t-env raw`, which says the address register was 0 and the length and count 0. The red run is relay-rt's tests: `raw` is missing. Mutation checks (3, against `env_calls`): `r9` plus 3, `r8` plus 1, and an `rcx` never 0, each fail the scenario.

**Files:**
- Modify: `crates/relay-rt/src/env.rs`
- Modify: `crates/relay-rt/src/start.rs`
- Modify: `tests/e2e/env_calls.txt`
- Modify: `userland/tests/src/bin/t-env.rs`

**Interfaces:**
- Consumes: Task 3's `env` module; Task 5's `t-env`.
- Produces: `relay_rt::env::raw() -> (usize, usize, usize)`; `env::set(ptr, len, count)` (unsafe, crate-private); `t-env raw`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/env.rs`**

In `crates/relay-rt/src/env.rs`, replace:

````rust
        assert_eq!(var(b"HOME"), None);
        set(Block::new(b"HOME=/root\0X=1\0", 2));
        assert_eq!(block(), b"HOME=/root\0X=1\0");
````

with:

````rust
        assert_eq!(var(b"HOME"), None);
        assert_eq!(raw(), (0, 0, 0));
        let b: &'static [u8] = b"HOME=/root\0X=1\0";
        // SAFETY: a static block.
        unsafe { set(b.as_ptr(), b.len(), 2) };
        assert_eq!(
            raw(),
            (b.as_ptr() as usize, 15, 2),
            "as the registers gave them"
        );
        assert_eq!(block(), b"HOME=/root\0X=1\0");
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/env_calls.txt`**

In `tests/e2e/env_calls.txt`, replace:

````text
send t-env none
expect \n0 entries, 0 bytes\nroot@relay:~# $
send t-env limits
````

with:

````text
send t-env none
expect \nno address, 0 bytes, 0 entries\nroot@relay:~# $
send t-env limits
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-env.rs`**

In `userland/tests/src/bin/t-env.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! - `t-env var NAME` prints `relay_rt::env::var(NAME)`, or `unset`;
//! - `t-env sizes` prints its arguments' count and length and its
//!   environment's;
//! - `t-env child` starts `t-env` with an environment of non-ASCII text,
//!   an entry without `=` and an empty one; `t-env none` with none;
//! - `t-env limits` starts `t-env sizes` with 64 KiB of environment, then
````

with:

````rust
//! - `t-env var NAME` prints `relay_rt::env::var(NAME)`, or `unset`;
//! - `t-env raw` prints whether the address register was 0, the length and
//!   the count;
//! - `t-env sizes` prints its arguments' count and length and its
//!   environment's;
//! - `t-env child` starts `t-env` with an environment of non-ASCII text,
//!   an entry without `=` and an empty one; `t-env none` starts `t-env raw`
//!   with none;
//! - `t-env limits` starts `t-env sizes` with 64 KiB of environment, then
````

Replace:

````rust
        },
        Some(b"sizes") => {
````

with:

````rust
        },
        Some(b"raw") => {
            let (at, len, count) = relay_rt::env::raw();
            let at = if at == 0 { "no address" } else { "an address" };
            let _ = writeln!(Fd(1), "{at}, {len} bytes, {count} entries");
            Ok(())
        }
        Some(b"sizes") => {
````

Replace:

````rust
        Some(b"child") => run(b"A=1\0B=two words\0C=\xc3\xa9t\xc3\xa9\0none\0\0", None),
        Some(b"none") => run(b"", None),
        Some(b"limits") => limits(),
````

with:

````rust
        Some(b"child") => run(b"A=1\0B=two words\0C=\xc3\xa9t\xc3\xa9\0none\0\0", None),
        Some(b"none") => run(b"", Some(b"raw")),
        Some(b"limits") => limits(),
````

Replace:

````rust
        2,
        b"usage: t-env [var NAME|sizes|child|none|limits|sh|deep FILE]\n",
    );
````

with:

````rust
        2,
        b"usage: t-env [var NAME|raw|sizes|child|none|limits|sh|deep FILE]\n",
    );
````

Replace:

````rust

/// The count and length, then each entry.
fn show() {
    let env = relay_rt::env::program();
    let all: Vec<&[u8]> = env.entries().collect();
    let n = all.len();
    let _ = writeln!(
        Fd(1),
        "{n} {}, {} bytes",
        entries(n),
        relay_rt::env::block().len()
    );
    for (i, e) in all.iter().enumerate() {
        let _ = write!(Fd(1), "[{i}] ");
````

with:

````rust

/// The count and length the registers gave, then each entry.
fn show() {
    let (_, len, n) = relay_rt::env::raw();
    let _ = writeln!(Fd(1), "{n} {}, {len} bytes", entries(n));
    for (i, e) in relay_rt::env::program().entries().enumerate() {
        let _ = write!(Fd(1), "[{i}] ");
````

Replace:

````rust
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    let n = relay_rt::env::program().entries().count();
    let _ = writeln!(
        Fd(1),
        "{} arguments, {len} bytes; {n} {}, {} bytes",
        args.len(),
        entries(n),
        relay_rt::env::block().len()
    );
````

with:

````rust
    let len: usize = args.iter().map(|a| a.len() + 1).sum();
    let (_, env_len, n) = relay_rt::env::raw();
    let _ = writeln!(
        Fd(1),
        "{} arguments, {len} bytes; {n} {}, {env_len} bytes",
        args.len(),
        entries(n)
    );
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find function `raw` in this scope ``; `this function takes 1 argument but 3 arguments were supplied`.

- [ ] **Step 5: Change `crates/relay-rt/src/env.rs`**

In `crates/relay-rt/src/env.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// The program's block, set before `main` runs.
static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Off Relay OS only the tests set it.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
pub(crate) fn set(b: Block) {
    PTR.store(b.bytes.as_ptr().cast_mut(), Ordering::Relaxed);
    LEN.store(b.bytes.len(), Ordering::Relaxed);
    COUNT.store(b.count, Ordering::Relaxed);
}
````

with:

````rust

/// The program's block, as the registers gave it before `main` ran.
static PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Keeps the block's address, length and count as `start` got them.
/// Off Relay OS only the tests set it.
///
/// # Safety
/// `ptr` is null, or it and `len` describe memory that stays readable for
/// the whole run.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
pub(crate) unsafe fn set(ptr: *const u8, len: usize, count: usize) {
    PTR.store(ptr.cast_mut(), Ordering::Relaxed);
    LEN.store(len, Ordering::Relaxed);
    COUNT.store(count, Ordering::Relaxed);
}

/// The address, length and count the program started with, as they were
/// (all 0 for no environment): for a test of the entry state.
pub fn raw() -> (usize, usize, usize) {
    let p = PTR.load(Ordering::Relaxed);
    (
        p as usize,
        LEN.load(Ordering::Relaxed),
        COUNT.load(Ordering::Relaxed),
    )
}
````

Replace:

````rust
    } else {
        // SAFETY: set from a `&'static [u8]` in `set`.
        unsafe { core::slice::from_raw_parts(p, LEN.load(Ordering::Relaxed)) }
````

with:

````rust
    } else {
        // SAFETY: as `set` was promised.
        unsafe { core::slice::from_raw_parts(p, LEN.load(Ordering::Relaxed)) }
````

- [ ] **Step 6: Change `crates/relay-rt/src/start.rs`**

In `crates/relay-rt/src/start.rs`, replace:

````rust
        set_name(args.name());
        let env_bytes: &'static [u8] = if env.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(env, env_len) }
        };
        env::set(env::Block::new(env_bytes, env_count));
        let code = unsafe { __relay_main(args) };
````

with:

````rust
        set_name(args.name());
        unsafe { env::set(env, env_len, env_count) };
        let code = unsafe { __relay_main(args) };
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 33 tests.

- [ ] **Step 8: Run the `env_calls` scenario**

Run: `cargo xtask test --e2e-only --scenario env_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates tests userland
git commit -F - <<'EOF'
test(relay-rt,e2e): show the environment's registers as they came

relay_rt::env::raw() gives the address, length and count start got,
and t-env prints the count and length from it, where it counted the
entries itself: a count the entries did not show went unseen (the
prototype's review, M-4). t-env none starts t-env raw, which says the
address register was 0 and the length and count are 0.

Refs: review M-4
EOF
````


### Task 7: `statfs` says which filesystems are read-only

Decision 6 (spec §15 item 3): `vfs::StatFs` gains `read_only`: ext2 sets it when it was mounted read-only (an unsupported feature, or the kernel's mount after an error) or shut down, `MemFs` when made read-only or shut down, `system.img` always. The kernel's `statfs` gives it to programs as `STATFS_READ_ONLY` in `StatFs::flags`, and relay-rt's `SysVfs` reads it back. The red run is vfs's tests: the field is missing. Mutation checks (5): each filesystem's answer and both translations, broken, fail a test.

**Files:**
- Modify: `crates/ext2/src/lib.rs`
- Modify: `crates/ext2/tests/clean.rs`
- Modify: `crates/relay-rt/src/sysvfs.rs`
- Modify: `crates/relay-rt/src/testing.rs`
- Modify: `crates/sysimg/src/fs.rs`
- Modify: `crates/vfs/src/fs.rs`
- Modify: `crates/vfs/src/memfs.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/syscall/testing.rs`

**Interfaces:**
- Consumes: Task 1's `StatFs::flags`.
- Produces: `vfs::StatFs::read_only`; `syscall::testing::Clock` (public for the kernel's tests).

- [ ] **Step 1: Add the failing tests to `crates/ext2/tests/clean.rs`**

In `crates/ext2/tests/clean.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
        assert!(!fs.is_read_only());
        // Mounting itself wrote the superblock and flushed.
````

with:

````rust
        assert!(!fs.is_read_only());
        assert!(!fs.statfs().unwrap().read_only);
        // Mounting itself wrote the superblock and flushed.
````

Replace:

````rust
        fs.shutdown().unwrap();
        assert!(fs.is_read_only());
        assert_eq!(fs.touch(fs.root()), Err(Errno::EROFS));
        assert_eq!(state(&img), "clean");
````

with:

````rust
        fs.shutdown().unwrap();
        assert!(fs.is_read_only());
        assert!(fs.statfs().unwrap().read_only, "statfs says so");
        assert_eq!(fs.touch(fs.root()), Err(Errno::EROFS));
        assert_eq!(state(&img), "clean");
````

Replace:

````rust
    fs.read_at(f, 0, &mut buf).unwrap();
    fs.statfs().unwrap();
    fs.sync().unwrap();
````

with:

````rust
    fs.read_at(f, 0, &mut buf).unwrap();
    assert!(fs.statfs().unwrap().read_only);
    fs.sync().unwrap();
````

Replace:

````rust
    assert!(fs.is_read_only());
    fs.shutdown().unwrap();
````

with:

````rust
    assert!(fs.is_read_only());
    assert!(fs.statfs().unwrap().read_only, "fell back to read-only");
    fs.shutdown().unwrap();
````

- [ ] **Step 2: Add the failing tests to `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, replace:

````rust
    #[test]
    fn a_file_is_one_node_whatever_its_path() {
````

with:

````rust
    #[test]
    fn statfs_says_which_filesystems_are_read_only() {
        let mut t = tree();
        t.mkdir(b"/ro").unwrap();
        t.mount(b"/ro", Box::new(memfs().read_only())).unwrap();
        let mut v = SysVfs::with(FakeCalls::new(t));
        assert!(v.statfs(b"/ro").unwrap().read_only);
        assert!(!v.statfs(b"/").unwrap().read_only);
        assert!(!v.statfs(b"/bin").unwrap().read_only);
    }

    #[test]
    fn a_file_is_one_node_whatever_its_path() {
````

- [ ] **Step 3: Extend the test support in `crates/relay-rt/src/testing.rs`**

In `crates/relay-rt/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_TRUNCATE, OPEN_WRITE,
    put_dir_entry,
};
````

with:

````rust
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_TRUNCATE, OPEN_WRITE,
    STATFS_READ_ONLY, put_dir_entry,
};
````

Replace:

````rust
            free_files: s.free_files,
            flags: 0,
        })
````

with:

````rust
            free_files: s.free_files,
            flags: if s.read_only { STATFS_READ_ONLY } else { 0 },
        })
````

- [ ] **Step 4: Add the failing tests to `crates/sysimg/src/fs.rs`**

In `crates/sysimg/src/fs.rs`, replace:

````rust
                free_files: 0,
            })
````

with:

````rust
                free_files: 0,
                read_only: true,
            })
````

- [ ] **Step 5: Add the failing tests to `crates/vfs/src/memfs.rs`**

In `crates/vfs/src/memfs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let f = fs.create(ROOT, b"f").unwrap();
        let mut fs = fs.read_only();
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
````

with:

````rust
        let f = fs.create(ROOT, b"f").unwrap();
        assert!(!fs.statfs().unwrap().read_only);
        let mut fs = fs.read_only();
        assert!(fs.statfs().unwrap().read_only, "statfs says so");
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
````

Replace:

````rust
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
    }
````

with:

````rust
        assert_eq!(fs.create(ROOT, b"f"), Err(Errno::EROFS));
        assert!(fs.statfs().unwrap().read_only);
    }
````

- [ ] **Step 6: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_END,
        SEEK_START,
    };
````

with:

````rust
        KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_END,
        SEEK_START, STATFS_READ_ONLY,
    };
````

Replace:

````rust
        assert_eq!(on(&mut f, Call::Statfs, b"/full", [W, 0]), Ok(0));
        let b = get(&mut f, W, 48);
        let want = vfs::Vfs::statfs(&mut f.vfs, b"/full").unwrap();
        assert_eq!(
````

with:

````rust
        assert_eq!(on(&mut f, Call::Statfs, b"/full", [W, 0]), Ok(0));
        let b = get(&mut f, W, 56);
        let want = vfs::Vfs::statfs(&mut f.vfs, b"/full").unwrap();
        assert_eq!(u64_at(&b, 48), 0, "not read-only");
        assert_eq!(
````

Replace:

````rust
            ]
        );
        assert_eq!(
            on(&mut f, Call::Statfs, b"/nope", [W, 0]),
````

with:

````rust
            ]
        );
        // A read-only filesystem says so in `flags`.
        let ro = vfs::MemFs::new(alloc::boxed::Box::new(Clock)).read_only();
        f.vfs.mount(b"/ro", alloc::boxed::Box::new(ro)).unwrap();
        assert_eq!(on(&mut f, Call::Statfs, b"/ro", [W, 0]), Ok(0));
        assert_eq!(u64_at(&get(&mut f, W, 56), 48), STATFS_READ_ONLY);
        assert_eq!(
            on(&mut f, Call::Statfs, b"/nope", [W, 0]),
````

- [ ] **Step 7: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, replace:

````rust

struct Clock;

````

with:

````rust

/// The tests' time, 1000.
pub struct Clock;

````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` no field `read_only` on type `fs::StatFs` ``.

- [ ] **Step 9: Change `crates/ext2/src/lib.rs`**

In `crates/ext2/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Like Linux's ext2: sizes without the metadata, and space reserved
    /// for root is free but not available.
    fn fs_stat(&self) -> StatFs {
````

with:

````rust
    /// Like Linux's ext2: sizes without the metadata, and space reserved
    /// for root is free but not available; read-only when mounted so (an
    /// unsupported feature, or the kernel after an error) or shut down.
    fn fs_stat(&self) -> StatFs {
````

Replace:

````rust
            free_files: self.sb.free_inodes_count() as u64,
        }
````

with:

````rust
            free_files: self.sb.free_inodes_count() as u64,
            read_only: self.read_only,
        }
````

- [ ] **Step 10: Change `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_START,
    STAT_NOFOLLOW, dir_entries,
};
````

with:

````rust
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_START,
    STAT_NOFOLLOW, STATFS_READ_ONLY, dir_entries,
};
````

Replace:

````rust
            free_files: s.free_files,
        })
````

with:

````rust
            free_files: s.free_files,
            read_only: s.flags & STATFS_READ_ONLY != 0,
        })
````

- [ ] **Step 11: Change `crates/sysimg/src/fs.rs`**

In `crates/sysimg/src/fs.rs`, replace:

````rust
            free_files: 0,
        })
````

with:

````rust
            free_files: 0,
            read_only: true,
        })
````

- [ ] **Step 12: Change `crates/vfs/src/fs.rs`**

In `crates/vfs/src/fs.rs`, replace:

````rust
    pub free_files: u64,
}
````

with:

````rust
    pub free_files: u64,
    /// Every change is refused (`EROFS`): mounted so, or shut down.
    pub read_only: bool,
}
````

- [ ] **Step 13: Change `crates/vfs/src/memfs.rs`**

In `crates/vfs/src/memfs.rs`, replace:

````rust
            free_files: u32::MAX as u64 - self.nodes.len() as u64,
        })
````

with:

````rust
            free_files: u32::MAX as u64 - self.nodes.len() as u64,
            read_only: self.read_only,
        })
````

- [ ] **Step 14: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, KIND_FIFO, STAT_NOFOLLOW, Stat};
use vfs::{Errno, Vfs};
````

with:

````rust
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, KIND_FIFO, STAT_NOFOLLOW, STATFS_READ_ONLY, Stat};
use vfs::{Errno, Vfs};
````

Replace:

````rust
        free_files: f.free_files,
        flags: 0,
    };
````

with:

````rust
        free_files: f.free_files,
        flags: if f.read_only { STATFS_READ_ONLY } else { 0 },
    };
````

- [ ] **Step 15: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 56 tests.

Run: `cargo test -p ext2`

Expected: PASS: 124 tests.

Run: `cargo test -p sysimg`

Expected: PASS: 16 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 34 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 400 tests.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add crates kernel
git commit -F - <<'EOF'
feat(vfs,kernel): say in statfs which filesystems are read-only

vfs::StatFs gains read_only: ext2 sets it when mounted read-only (an
unsupported feature, or the kernel after an error) or shut down, MemFs
when made so or shut down, system.img always. The kernel's statfs
gives it to programs as STATFS_READ_ONLY in StatFs::flags, and
relay-rt's SysVfs reads it back (programmable shell gate §15 items 3
and 6).
EOF
````


### Task 8: `test -w` answers from the read-only flag

Decision 6: `test -w` was false only for a file on `/bin`'s filesystem, compared by mount (spec §15 item 3); it is now false on any filesystem `statfs` says is read-only, so on a root the kernel mounted read-only too, ending spec §10's difference. As Linux's `access`, which GNU's `test` asks, a device, a FIFO or a socket stays writable there (its `special_file` skips `EROFS`; a probe on a read-only bind mount, `tmp/m5p2/probes/p2-test-w-ro.txt`). The red run is the shell's tests: a writable filesystem mounted at `/bin` read as read-only. Mutation checks (4): every kind checked, never read-only, the root's `statfs` instead of the path's, and symbolic links writable, each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/test.rs`

**Interfaces:**
- Consumes: Task 7's `StatFs::read_only` through `Vfs::statfs`.
- Produces: nothing new (`Evaluator::read_only` replaces `on_bin`).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn nothing_on_bin_s_filesystem_is_writable() {
        // `/bin` is read-only (system.img), which no call tells: a file
        // on its filesystem is not writable (spec §15 item 3).
        let mut programs = memfs();
````

with:

````rust
    fn nothing_on_bin_s_filesystem_is_writable() {
        // `/bin` is read-only (system.img): a file on its filesystem is
        // not writable (spec §15 items 3 and 6).
        let mut programs = memfs();
````

Replace:

````rust
        assert_eq!(etc.ino, h.vfs.lookup(b"/bin/ls").unwrap().ino);
    }
````

with:

````rust
        assert_eq!(etc.ino, h.vfs.lookup(b"/bin/ls").unwrap().ino);
    }

    #[test]
    fn nothing_on_a_read_only_filesystem_is_writable_but_a_device() {
        // `statfs` says which filesystems are read-only (spec §15 item 6):
        // a root mounted so too, and a `/bin` mounted writable is
        // writable. As Linux's `access`, which GNU asks, a device or a
        // FIFO stays writable there.
        let mut h = Harness::new();
        let mut bin = memfs();
        let root = bin.root();
        bin.create(root, b"ls").unwrap();
        h.vfs.mkdir(b"/bin").unwrap();
        h.vfs.mount(b"/bin", Box::new(bin)).unwrap();
        for line in ["test -w /bin/ls", "[ -w /bin ]", "test -w /etc/motd"] {
            assert_eq!(h.run(line), (0, String::new()), "{line}");
        }
        let mut ro = memfs();
        let root = ro.root();
        ro.create(root, b"f").unwrap();
        ro.mkdir(root, b"d").unwrap();
        ro.symlink(root, b"l", b"f").unwrap();
        ro.special(root, b"p", FileType::Fifo).unwrap();
        ro.special(root, b"c", FileType::CharDev).unwrap();
        h.vfs = vfs::MountTable::new(Box::new(ro.read_only()));
        for (line, status) in [
            ("test -w /f", 1),
            ("test -w /d", 1),
            ("test -w /", 1),
            ("test -w /l", 1),
            ("test -w /p", 0),
            ("test -w /c", 0),
            ("test -r /f", 0),
            ("test -w /nope", 1),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell --lib test::tests`

Expected: FAIL: 1 test fails: `commands::test::tests::nothing_on_a_read_only_filesystem_is_writable_but_a_device`.

- [ ] **Step 3: Change `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!
//! The rules by argument count come first, for 1 to 4 arguments, then
//! `-o` over `-a` over terms; a term is a run of `!`, then `(` and an
//! expression of up to 4 arguments before its `)` (or of all the rest), a
//! binary operator if the argument after next is one, a unary operator of
//! the form `-X`, or a string. Strings are compared as bytes; GNU's program
//! has no `<` or `>`, which bash's built-in has. Integers are read as GNU
//! reads them (blanks around, a sign, any number of digits) and compared as
//! digit strings, so nothing overflows; `-l STRING` stands for STRING's
//! length. Files are answered from `stat`, as for root, with two decided
//! differences (spec §10): a symbolic link is never followed, so `-e`, `-f`
//! and `-d` look at the link itself, and `-w` is false only on `/bin`'s
//! filesystem, the read-only one a program can tell. The evaluator never
//! recurses: each open `(` waits on a stack of its own, so it nests as deep
//! as the arguments go, as GNU does on its larger stack.

````

with:

````rust
//!
//! The rules by argument count come first, for 1 to 4 arguments, then `-o`
//! over `-a` over terms; a term is a run of `!`, then `(` and an expression
//! of up to 4 arguments before its `)` (or of all the rest), a binary
//! operator if the argument after next is one, a unary operator of the form
//! `-X`, or a string. Strings are compared as bytes; GNU's program has no
//! `<` or `>`, which bash's built-in has. Integers are read as GNU reads
//! them (blanks around, a sign, any number of digits) and compared as digit
//! strings, so nothing overflows; `-l STRING` stands for STRING's length.
//! Files are answered from `stat`, as for root, with one decided difference
//! (spec §10): a symbolic link is never followed, so `-e`, `-f` and `-d`
//! look at the link itself. `-w` is false on a filesystem `statfs` says is
//! read-only, but for a device or a FIFO, as Linux's `access` answers (spec
//! §15 item 6). The evaluator never recurses: each open `(` waits on a
//! stack of its own, so it nests as deep as the arguments go, as GNU does
//! on its larger stack.

````

Replace:

````rust
                let path = self.unary_operand()?;
                return Ok(self.stat(path).is_some_and(|(node, _)| !self.on_bin(node)));
            }
````

with:

````rust
                let path = self.unary_operand()?;
                return Ok(self
                    .stat(path)
                    .is_some_and(|(_, s)| !self.read_only(path, &s)));
            }
````

Replace:

````rust

    /// Whether `node` is on `/bin`'s filesystem, `system.img`, which is
    /// read-only: no call says which filesystems are, and `open` for writing
    /// succeeds on them, so a root the kernel mounted read-only is not told
    /// (spec §10, §15 item 3).
    /// Only when `/bin` is a mount of its own: `host-shell`'s `/bin` is a
    /// directory of the image's root.
    fn on_bin(&mut self, node: vfs::Node) -> bool {
        let vfs = &mut self.ctx.vfs;
        match (vfs.lookup(b"/bin"), vfs.lookup(b"/")) {
            (Ok(bin), Ok(root)) => bin.mount != root.mount && bin.mount == node.mount,
            _ => false,
        }
    }
````

with:

````rust

    /// Whether writing `path`, whose status is `s`, is refused because its
    /// filesystem is read-only, as `statfs` tells (spec §15 item 6): as
    /// Linux's `access` says `EROFS` for a regular file, a directory or a
    /// symbolic link there, and not for a device or a FIFO.
    fn read_only(&mut self, path: &str, s: &vfs::Stat) -> bool {
        matches!(
            s.kind,
            FileType::Regular | FileType::Directory | FileType::Symlink
        ) && self
            .ctx
            .vfs
            .statfs(path.as_bytes())
            .is_ok_and(|f| f.read_only)
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 407 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): answer test -w from statfs's read-only flag

-w is false for a file on any filesystem statfs says is read-only, a
root the kernel mounted read-only too, where it was false only on
/bin's filesystem; a device or a FIFO stays writable there, as Linux's
access answers GNU's test. This
ends spec §10's difference for -w (programmable shell gate §15 item 6).
EOF
````


### Task 9: `test -w` on a read-only filesystem compared with GNU's

Decision 6 (the prototype's review, M-6): the answers of Task 8 were written by hand from one probe, where the rest of `test`'s file operators are compared with GNU's. The test helpers gain `host_files_read_only`, which runs a host tool as root (`unshare -rm`) on its directory bound over itself read-only, and `like_host_files_read_only`, the shell's on a read-only `MemFs` mounted at `/w`; a test compares `-w`, `-r` and `-e` on a file, a directory, a symbolic link, a FIFO, a socket, the directory itself and a missing name. CI's unit job already allows user namespaces (`unshare -r`); this one also binds a mount in its own namespace. Tests only: no failing run. Mutation checks (4): Task 8's four mutants each fail this test alone.

**Files:**
- Modify: `crates/shell/src/commands/test.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: `testing::{host_files, like_host_files, TestFile}`.
- Produces: `testing::{host_files_read_only, like_host_files_read_only}`.

- [ ] **Step 1: Change the tests in `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::named;
    use crate::testing::{Harness, TestFile, host_files, host_tool, like_host_files, memfs};
    use alloc::boxed::Box;
````

with:

````rust
    use super::named;
    use crate::testing::{
        Harness, TestFile, host_files, host_files_read_only, host_tool, like_host_files,
        like_host_files_read_only, memfs,
    };
    use alloc::boxed::Box;
````

Replace:

````rust
    #[test]
    fn a_plain_bin_directory_is_not_read_only() {
````

with:

````rust
    #[test]
    fn on_a_read_only_filesystem_as_gnu_s_test() {
        // GNU's test as root on a directory bound read-only (unshare -rm),
        // against a read-only filesystem mounted at /w (spec §15 item 6):
        // a FIFO or a socket stays writable.
        let files = [
            TestFile::file("f", b"x"),
            TestFile::dir("d"),
            TestFile::link("l", "f"),
            TestFile::fifo("p"),
            TestFile::socket("s"),
        ];
        for op in ["-w", "-r", "-e"] {
            for name in ["f", "d", "l", "p", "s", ".", "nope"] {
                let line = ["test", op, name];
                assert_eq!(
                    like_host_files_read_only(&line, &files),
                    host_files_read_only(&line, &files),
                    "{line:?}"
                );
            }
        }
    }

    #[test]
    fn a_plain_bin_directory_is_not_read_only() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    let w = fs.mkdir(fs.root(), b"w").unwrap();
    let mut made: Vec<(&str, Ino)> = Vec::new();
    for f in files {
        let (dir, name) = match f.name.rsplit_once('/') {
            Some((d, n)) => (lookup_in(&mut fs, w, d), n),
            None => (w, f.name),
````

with:

````rust
    let w = fs.mkdir(fs.root(), b"w").unwrap();
    make_files(&mut fs, w, files);
    let mut h = Harness::on(fs);
    h.vfs.chdir(b"/w").unwrap();
    let mut out = FakeStdout::file(None);
    let (status, errors) = h.program_args(args, &mut out);
    (status, out.text(), errors)
}

/// [`like_host_files`] with `files` on a read-only filesystem mounted at
/// `/w`, to compare with [`host_files_read_only`]'.
pub fn like_host_files_read_only(args: &[&str], files: &[TestFile<'_>]) -> (i32, String, String) {
    let mut fs = memfs();
    let root = fs.root();
    make_files(&mut fs, root, files);
    let mut h = Harness::new();
    h.vfs.mkdir(b"/w").unwrap();
    h.vfs.mount(b"/w", Box::new(fs.read_only())).unwrap();
    h.vfs.chdir(b"/w").unwrap();
    let mut out = FakeStdout::file(None);
    let (status, errors) = h.program_args(args, &mut out);
    (status, out.text(), errors)
}

/// Makes `files` in the directory `w` of `fs`.
fn make_files(fs: &mut MemFs, w: Ino, files: &[TestFile<'_>]) {
    let mut made: Vec<(&str, Ino)> = Vec::new();
    for f in files {
        let (dir, name) = match f.name.rsplit_once('/') {
            Some((d, n)) => (lookup_in(fs, w, d), n),
            None => (w, f.name),
````

Replace:

````rust
    }
    let mut h = Harness::on(fs);
    h.vfs.chdir(b"/w").unwrap();
    let mut out = FakeStdout::file(None);
    let (status, errors) = h.program_args(args, &mut out);
    (status, out.text(), errors)
}
````

with:

````rust
    }
}
````

Replace:

````rust
pub fn host_files(args: &[&str], files: &[TestFile<'_>], root: bool) -> (i32, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
````

with:

````rust
pub fn host_files(args: &[&str], files: &[TestFile<'_>], root: bool) -> (i32, String, String) {
    host_run(args, files, if root { As::Root } else { As::User })
}

/// [`host_files`] as root with the directory bound over itself read-only,
/// in a mount namespace of its own (`unshare -rm`).
pub fn host_files_read_only(args: &[&str], files: &[TestFile<'_>]) -> (i32, String, String) {
    host_run(args, files, As::RootReadOnly)
}

/// Who runs a host tool, and on what.
#[derive(Clone, Copy, PartialEq, Eq)]
enum As {
    User,
    Root,
    RootReadOnly,
}

fn host_run(args: &[&str], files: &[TestFile<'_>], how: As) -> (i32, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
````

Replace:

````rust
    }
    let mut cmd = if root {
        let mut c = std::process::Command::new("unshare");
        c.arg("-r").args(args);
        c
    } else {
        let mut c = std::process::Command::new(args[0]);
        c.args(&args[1..]);
        c
    };
````

with:

````rust
    }
    let mut cmd = match how {
        As::User => {
            let mut c = std::process::Command::new(args[0]);
            c.args(&args[1..]);
            c
        }
        As::Root => {
            let mut c = std::process::Command::new("unshare");
            c.arg("-r").args(args);
            c
        }
        As::RootReadOnly => {
            // Into the mount just made: the working directory is the one
            // below it.
            let bind = "d=$PWD; mount --bind \"$d\" \"$d\" && \
                        mount -o remount,bind,ro \"$d\" && cd \"$d\" && exec \"$@\"";
            let mut c = std::process::Command::new("unshare");
            c.args(["-rm", "sh", "-c", bind, "sh"]).args(args);
            c
        }
    };
````

Replace:

````rust
    let stderr = text(out.stderr);
    if root {
        assert!(
            !stderr.starts_with("unshare:"),
            "unshare -r is needed (on Ubuntu 24.04, sysctl \
````

with:

````rust
    let stderr = text(out.stderr);
    if how != As::User {
        assert!(
            !stderr.starts_with("unshare:") && !stderr.starts_with("mount:"),
            "unshare -r is needed (on Ubuntu 24.04, sysctl \
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p shell --lib test::tests`

Expected: PASS: 20 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): compare test -w on a read-only filesystem with GNU's

The test helpers run a host tool as root on its directory bound over
itself read-only, in a mount namespace of its own (unshare -rm), and
the shell's on a read-only MemFs mounted at /w. test -w, -r and -e on
a file, a directory, a symbolic link, a FIFO, a socket, the directory
itself and a missing name now answer as GNU's do, where the answers
were written by hand (the prototype's review, M-6).

Refs: review M-6
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 51 scenario(s) passed`.

````bash
git push -u origin m5p2/abi
gh pr create --base main --head m5p2/abi --title "feat(relay-abi,kernel)!: give programs an environment (ABI 4)" --body-file - <<'EOF'
## What

Milestone 5, plan 2, tasks 1–9: ABI 4 (`relay_abi::VERSION` 4): `SpawnArgs` gains `env` and `env_len`, `StatFs` gains `flags` with `STATFS_READ_ONLY`; `spawn` copies the environment in (at most 64 KiB apart from the arguments, `E2BIG`; not ending in a NUL, `EINVAL`) and puts it just below the arguments on the new stack, which stays 255 pages; the program starts with its address, length and count in `rcx`, `r8` and `r9` (all 0 for none); `relay_rt::env` reads it as glibc does, `sys::spawn_env` gives a child one, and `/bin/sh` hands its own on; init starts the shell with `HOME=/root`; `t-env` and the scenario `env_calls` check it all, a script at the nesting bound with both blocks full among it; every filesystem says in `statfs` whether it is read-only, and `test -w` answers from it as GNU's does (a device or a FIFO stays writable), compared with GNU's on a read-only bind mount; `t-abi` is built for ABI 3, the startup line says `ABI 4`, `/bin` holds 45 programs.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests
- [x] New scenario `env_calls`

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4; check 3's NUC transcript gets the new startup lines by hand until plan 4's run

BREAKING CHANGE: programs built for ABI 3 must be rebuilt; the kernel refuses them and a system.img of ABI 3.
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-abi
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: `/dev/null` (Tasks 10–17)

A filesystem of one character device, mounted at `/dev` at startup and in `host-shell`; `cp` reads it, the in-process runner never takes it for an input that is the output, and its `seek` gives 0 as Linux's does; the `redirect` scenario uses it under `/bin/sh`, and `mount_fail` asks `test -w` of a read-only root.

Branch `m5p2/dev`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-dev`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p2/dev /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-dev origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-dev
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p2/abi` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p2/dev /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-dev m5p2/abi`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p2/abi>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 10: `vfs::DevFs`, a filesystem holding `/dev/null`

Decision 7 (spec §8.4): a new module, `crates/vfs/src/devfs.rs`, is a filesystem of a root directory (`0755`) and `null`, a character device (`0666`, user and group 0, its times the moment it was made): a read gives the end of input, a write takes every byte and keeps none, truncating and touching it succeed and change nothing. Nothing can be made, removed or moved in it: `EPERM`, as on a Linux filesystem without those operations, after the contract's `ENOENT` for a node that does not exist, the name's own error, `ENOTDIR`, and `EEXIST` or `ENOENT` for the name; its `statfs` says it is not read-only. Mounted on a name, it stands over what a disk has there. The red run is vfs's tests: the module is missing. Mutation checks (10): a read that gives bytes, a write that takes half, truncating refused, making or removing or moving allowed, read-only, its mode, its kind, and the errors' order, each fail a test.

**Files:**
- Create: `crates/vfs/src/devfs.rs`
- Modify: `crates/vfs/src/lib.rs`

**Interfaces:**
- Consumes: `vfs::{FileSystem, Stat, StatFs}` (Task 7's `read_only`).
- Produces: `vfs::DevFs::new(now)`.

- [ ] **Step 1: Write the failing tests for `crates/vfs/src/devfs.rs`**

Create `crates/vfs/src/devfs.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemFs, MountTable, Vfs};
    use alloc::boxed::Box;

    const NOW: u64 = 1_790_424_000;

    fn names(fs: &mut DevFs, dir: Ino) -> Vec<Vec<u8>> {
        let mut n: Vec<Vec<u8>> = fs
            .read_dir(dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        n.sort();
        n
    }

    #[test]
    fn null_reads_as_the_end_and_takes_every_write() {
        let mut fs = DevFs::new(NOW);
        let null = fs.lookup(fs.root(), b"null").unwrap();
        let mut buf = [7u8; 8];
        assert_eq!(fs.read_at(null, 0, &mut buf), Ok(0));
        assert_eq!(fs.read_at(null, 1 << 40, &mut buf), Ok(0));
        assert_eq!(buf, [7; 8], "nothing written into the buffer");
        assert_eq!(fs.write_at(null, 0, b"gone"), Ok(4));
        assert_eq!(fs.write_at(null, u64::MAX, &[0; 5000]), Ok(5000));
        assert_eq!(fs.truncate(null, 0), Ok(()), "`> /dev/null` truncates it");
        assert_eq!(fs.truncate(null, 9), Ok(()));
        assert_eq!(fs.touch(null), Ok(()));
        let st = fs.stat(null).unwrap();
        assert_eq!((st.size, st.blocks), (0, 0), "keeps nothing");
        assert_eq!((st.atime, st.mtime, st.ctime), (NOW, NOW, NOW));
    }

    #[test]
    fn null_is_a_character_device_anyone_may_read_and_write() {
        let mut fs = DevFs::new(NOW);
        let root = fs.root();
        let null = fs.lookup(root, b"null").unwrap();
        let st = fs.stat(null).unwrap();
        assert_eq!(
            (st.ino, st.kind, st.perm, st.nlink, st.uid, st.gid),
            (null, FileType::CharDev, 0o666, 1, 0, 0)
        );
        let dir = fs.stat(root).unwrap();
        assert_eq!(
            (dir.kind, dir.perm, dir.nlink),
            (FileType::Directory, 0o755, 2)
        );
        assert_eq!(fs.stat(99), Err(Errno::ENOENT));
        assert_eq!(names(&mut fs, root), [&b"."[..], b"..", b"null"]);
        assert_eq!(fs.lookup(root, b"."), Ok(root));
        assert_eq!(fs.lookup(root, b".."), Ok(root));
        assert_eq!(fs.lookup(root, b"zero"), Err(Errno::ENOENT));
        assert_eq!(fs.lookup(null, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(null), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_link(null), Err(Errno::EINVAL));
        let mut buf = [0u8; 4];
        assert_eq!(fs.read_at(root, 0, &mut buf), Err(Errno::EISDIR));
        assert_eq!(fs.write_at(root, 0, b"x"), Err(Errno::EISDIR));
        let s = fs.statfs().unwrap();
        assert!(!s.read_only, "null is written");
        assert_eq!((s.blocks, s.files), (0, 2));
    }

    #[test]
    fn nothing_is_made_removed_or_moved() {
        // As on a Linux filesystem without those operations: EPERM, after
        // the errors a missing or existing name gives.
        let mut fs = DevFs::new(NOW);
        let root = fs.root();
        let null = fs.lookup(root, b"null").unwrap();
        assert_eq!(fs.create(root, b"x"), Err(Errno::EPERM));
        assert_eq!(fs.create(root, b"null"), Err(Errno::EEXIST));
        assert_eq!(fs.mkdir(root, b"d"), Err(Errno::EPERM));
        assert_eq!(fs.mkdir(root, b"null"), Err(Errno::EEXIST));
        assert_eq!(fs.unlink(root, b"null"), Err(Errno::EPERM));
        assert_eq!(fs.unlink(root, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.rmdir(root, b"null"), Err(Errno::ENOTDIR));
        assert_eq!(fs.rmdir(root, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.rename(root, b"null", root, b"n"), Err(Errno::EPERM));
        assert_eq!(fs.rename(root, b"x", root, b"n"), Err(Errno::ENOENT));
        assert_eq!(fs.create(null, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.create(99, b"x"), Err(Errno::ENOENT));
        assert_eq!(fs.create(root, b"a/b"), Err(Errno::EINVAL));
        assert_eq!(fs.create(99, b"a/b"), Err(Errno::ENOENT), "the node first");
        assert_eq!(fs.create(null, b"a/b"), Err(Errno::EINVAL), "then the name");
        assert_eq!(names(&mut fs, root).len(), 3, "still just null");
    }

    #[test]
    fn mounted_at_dev_it_stands_over_what_the_disk_has_there() {
        let mut disk = MemFs::new(Box::new(Clock));
        let root = disk.root();
        let dev = disk.mkdir(root, b"dev").unwrap();
        disk.create(dev, b"x").unwrap();
        let mut t = MountTable::new(Box::new(disk));
        t.mount(b"/dev", Box::new(DevFs::new(NOW))).unwrap();
        let null = t.lookup(b"/dev/null").unwrap();
        assert_eq!(null.mount, 1);
        assert_eq!(t.write_at(null, 0, b"x"), Ok(1));
        assert_eq!(t.lookup(b"/dev/x"), Err(Errno::ENOENT));
        assert_eq!(t.create(b"/dev/x"), Err(Errno::EPERM));
        assert_eq!(t.unlink(b"/dev/null"), Err(Errno::EPERM));
        assert!(!t.statfs(b"/dev/null").unwrap().read_only);
        // Where the disk has no /dev, the name is a mount point all the same.
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock)).read_only()));
        t.mount(b"/dev", Box::new(DevFs::new(NOW))).unwrap();
        assert!(t.lookup(b"/dev/null").is_ok());
    }

    struct Clock;

    impl crate::Env for Clock {
        fn now(&self) -> u64 {
            NOW
        }
        fn log(&self, _: &str) {}
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/vfs/src/lib.rs`**

In `crates/vfs/src/lib.rs`, replace:

````rust
mod block;
mod errno;
````

with:

````rust
mod block;
mod devfs;
mod errno;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` cannot find type `DevFs` in this scope ``; `` cannot find type `Ino` in this scope ``.

- [ ] **Step 4: Implement `crates/vfs/src/devfs.rs`**

Insert this at the top of `crates/vfs/src/devfs.rs`, above `#[cfg(test)]`:

````rust
//! `/dev` (programmable shell gate §8.4): a filesystem of one node,
//! `null`, a character device that reads as the end of input at once and
//! takes every write, keeping nothing. Nothing can be made, removed or
//! moved in it (`EPERM`, as on a Linux filesystem without those
//! operations); truncating and touching `null` succeed and change nothing.
//! It is not read-only: `null` is written.

use crate::Errno;
use crate::fs::{DirEntry, FileSystem, FileType, Ino, Stat, StatFs};
use crate::path::check_name;
use alloc::vec;
use alloc::vec::Vec;

const ROOT: Ino = 1;
const NULL: Ino = 2;

/// The block size it reports.
const BLOCK: u32 = 4096;

pub struct DevFs {
    /// Every time of both nodes: when it was made.
    time: u64,
}

impl DevFs {
    /// Made at `now`, seconds since 1970.
    pub fn new(now: u64) -> DevFs {
        DevFs { time: now }
    }

    /// `ENOENT` for a number that is no node, `ENOTDIR` for `null`.
    fn dir(&self, ino: Ino) -> Result<(), Errno> {
        match ino {
            ROOT => Ok(()),
            NULL => Err(Errno::ENOTDIR),
            _ => Err(Errno::ENOENT),
        }
    }

    /// The node a name of the root names: `ENOENT` for any but `null`.
    /// The errors come in the contract's order (`FileSystem`): a number
    /// that is no node, then the name's, then `ENOTDIR`.
    fn entry(&self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        if dir != ROOT && dir != NULL {
            return Err(Errno::ENOENT);
        }
        check_name(name)?;
        self.dir(dir)?;
        match name {
            b"null" => Ok(NULL),
            _ => Err(Errno::ENOENT),
        }
    }

    /// `null` for its own operations, `EISDIR` for the root.
    fn null(&self, ino: Ino) -> Result<(), Errno> {
        match ino {
            NULL => Ok(()),
            ROOT => Err(Errno::EISDIR),
            _ => Err(Errno::ENOENT),
        }
    }

    /// Making `name` in `dir`: `EEXIST` for `null`, `EPERM` for any other.
    fn make(&self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        match self.entry(dir, name) {
            Ok(_) => Err(Errno::EEXIST),
            Err(Errno::ENOENT) if self.dir(dir).is_ok() => Err(Errno::EPERM),
            Err(e) => Err(e),
        }
    }
}

impl FileSystem for DevFs {
    fn root(&self) -> Ino {
        ROOT
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let (kind, perm, nlink) = match ino {
            ROOT => (FileType::Directory, 0o755, 2),
            NULL => (FileType::CharDev, 0o666, 1),
            _ => return Err(Errno::ENOENT),
        };
        Ok(Stat {
            ino,
            kind,
            perm,
            nlink,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            block_size: BLOCK,
            atime: self.time,
            mtime: self.time,
            ctime: self.time,
        })
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.dir(dir)?;
        match name {
            b"." | b".." => Ok(ROOT),
            _ => self.entry(dir, name),
        }
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        self.dir(dir)?;
        let entry = |name: &[u8], ino| DirEntry {
            name: name.to_vec(),
            ino,
        };
        Ok(vec![
            entry(b".", ROOT),
            entry(b"..", ROOT),
            entry(b"null", NULL),
        ])
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.stat(ino)?;
        Err(Errno::EINVAL)
    }

    fn read_at(&mut self, ino: Ino, _offset: u64, _buf: &mut [u8]) -> Result<usize, Errno> {
        self.null(ino)?;
        Ok(0)
    }

    fn write_at(&mut self, ino: Ino, _offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.null(ino)?;
        Ok(buf.len())
    }

    fn truncate(&mut self, ino: Ino, _size: u64) -> Result<(), Errno> {
        self.null(ino)
    }

    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        self.stat(ino).map(|_| ())
    }

    fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.make(dir, name)
    }

    fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.make(dir, name)
    }

    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.entry(dir, name)?;
        Err(Errno::EPERM)
    }

    fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.entry(dir, name)?;
        Err(Errno::ENOTDIR)
    }

    fn rename(&mut self, from_dir: Ino, from: &[u8], to_dir: Ino, to: &[u8]) -> Result<(), Errno> {
        self.entry(from_dir, from)?;
        check_name(to)?;
        self.dir(to_dir)?;
        Err(Errno::EPERM)
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        Ok(StatFs {
            block_size: u64::from(BLOCK),
            blocks: 0,
            free_blocks: 0,
            avail_blocks: 0,
            files: 2,
            free_files: 0,
            read_only: false,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

````

- [ ] **Step 5: Change `crates/vfs/src/lib.rs`**

In `crates/vfs/src/lib.rs`, replace:

````rust
pub use block::{BlockDevice, IoError, check_request};
pub use errno::Errno;
````

with:

````rust
pub use block::{BlockDevice, IoError, check_request};
pub use devfs::DevFs;
pub use errno::Errno;
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 60 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(vfs): add DevFs, a filesystem holding /dev/null

DevFs has a root directory (0755) and null, a character device (0666,
user and group 0, its times when it was made): a read gives the end of
input, a write takes every byte and keeps none, truncating and touching
it succeed and change nothing. Nothing can be made, removed or moved in
it (EPERM, as on a Linux filesystem without those operations, after
ENOENT or EEXIST for the name), and statfs says it is not read-only
(programmable shell gate §8.4, §15 item 6).
EOF
````


### Task 11: `cp` reads a character device as its source

Decision 7 (the prototype's review, I-1): `cp` refused every source but a regular file, a rule of milestone 1, when nothing else could be read, so `cp /dev/null f` failed with `Invalid argument` where GNU's empties `f`. A character device is now read to its end; `cp /dev/null /dev/null` is GNU's `are the same file`, and `cp f /dev/null` succeeds. A test mounts `DevFs` in the harness and compares each with GNU's `cp`. The red run is the shell's tests. Mutation checks (2): regular files only, and any kind, each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/change.rs`

**Interfaces:**
- Consumes: Task 10's `DevFs`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
mod tests {
    use crate::testing::{Harness, memfs};
    use alloc::string::String;
````

with:

````rust
mod tests {
    use crate::testing::{Harness, host_tool, memfs};
    use alloc::string::String;
````

Replace:

````rust
    #[test]
    fn cp_into_a_directory_keeps_the_names() {
````

with:

````rust
    #[test]
    fn cp_reads_dev_null_as_gnu_s_does() {
        // A character device is read until its end, so `cp /dev/null f`
        // empties f (the prototype's review, I-1).
        let mut h = Harness::new();
        h.vfs
            .mount(b"/dev", alloc::boxed::Box::new(vfs::DevFs::new(0)))
            .unwrap();
        h.put("/tmp/e", b"old");
        h.put("/tmp/f", b"kept");
        let gnu = |line: &str| {
            let (status, out, err) =
                host_tool(&["sh", "-c", line], &[("e", b"old"), ("f", b"kept")], b"");
            (status, out + &err)
        };
        h.run("cd /tmp");
        for line in [
            "cp /dev/null e",
            "cp /dev/null new",
            "cp f /dev/null",
            "cp /dev/null /dev/null",
        ] {
            let (status, said) = h.run(line);
            assert_eq!((status, said), gnu(line), "{line}");
        }
        assert_eq!(h.get("/tmp/e"), b"", "emptied");
        assert_eq!(h.get("/tmp/new"), b"");
        assert_eq!(gnu("cp /dev/null e; wc -c < e").0, 0);
        assert_eq!(gnu("cp /dev/null e; wc -c < e").1, "0\n");
    }

    #[test]
    fn cp_into_a_directory_keeps_the_names() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell --lib change::`

Expected: FAIL: 1 test fails: `commands::change::tests::cp_reads_dev_null_as_gnu_s_does`.

- [ ] **Step 3: Change `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// `cp src dst`, `cp src… dir`: regular files only.
pub fn cp(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust

/// `cp src dst`, `cp src… dir`: regular files, and character devices
/// read to their end.
pub fn cp(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
        }
        // A symbolic link (not followed in milestone 1) or a special
        // file cannot be read: say so before the destination is touched.
        Ok((_, st)) if st.kind != FileType::Regular => {
            ctx.fail(
````

with:

````rust
        }
        // A symbolic link (not followed in milestone 1) or a special file
        // but a character device (`/dev/null`, read to its end) cannot be
        // read: say so before the destination is touched.
        Ok((_, st)) if !matches!(st.kind, FileType::Regular | FileType::CharDev) => {
            ctx.fail(
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 409 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): let cp read a character device as its source

cp refused every source but a regular file, a rule of milestone 1,
when nothing else could be read, so cp /dev/null f failed with
Invalid argument where GNU's empties f. A character device is now read
to its end; cp /dev/null /dev/null is GNU's "are the same file", and
cp f /dev/null succeeds (the prototype's review, I-1).

Refs: review I-1
EOF
````


### Task 12: Only a regular file is the output's node

Decision 7 (the prototype's review, M-2): the in-process runner gave a redirection's node as standard output's whatever it was, so in `host-shell`, once it mounts `/dev` (Task 16), `grep x /dev/null > /dev/null` said `input file is also the output`, status 2, where GNU's `grep`, which compares only regular files, says nothing, status 1. `Ctx::output_node` names only a regular file, as relay-rt's `SysStdout::node` does under `/bin/sh`. The red run is the shell's tests. Mutation check (1): any kind as the output's node fails a test; a kind check on the input survived (a device input can only equal a device output) and was left out.

**Files:**
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/ctx.rs`

**Interfaces:**
- Consumes: Task 10's `DevFs`.
- Produces: `Ctx::output_node(&mut self)` (it now asks the node's kind).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
    #[test]
    fn grep_never_reads_its_own_output() {
````

with:

````rust
    #[test]
    fn a_device_is_never_the_input_that_is_the_output() {
        // GNU compares only regular files; /dev/null as both is no
        // refusal (the prototype's review, M-2).
        let mut h = Harness::new();
        h.vfs
            .mount(b"/dev", alloc::boxed::Box::new(vfs::DevFs::new(0)))
            .unwrap();
        for line in [
            "grep x /dev/null > /dev/null",
            "grep x < /dev/null > /dev/null",
            "grep -c x /dev/null >> /dev/null",
            "cat /dev/null - < /dev/null > /dev/null",
        ] {
            let (status, out, err) = host_tool(&["sh", "-c", line], &[], b"");
            assert_eq!(h.run(line), (status, out + &err), "{line}");
        }
    }

    #[test]
    fn grep_never_reads_its_own_output() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell --lib grep::`

Expected: FAIL: 1 test fails: `commands::grep::tests::a_device_is_never_the_input_that_is_the_output`.

- [ ] **Step 3: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use vfs::{Errno, Node, Vfs};

````

with:

````rust
use core::fmt;
use vfs::{Errno, FileType, Node, Vfs};

````

Replace:

````rust

    /// The file standard output goes to, if any.
    pub fn output_node(&self) -> Option<Node> {
        match &self.out {
            Output::File { node, .. } => Some(*node),
            Output::Program { stdout, .. } => stdout.node(),
````

with:

````rust

    /// The regular file standard output goes to, if any, as relay-rt's
    /// `SysStdout::node`: GNU's input-is-output checks are for regular
    /// files, and a device (`/dev/null`) is not one.
    pub fn output_node(&mut self) -> Option<Node> {
        match &self.out {
            Output::File { node, .. } => {
                let node = *node;
                let st = self.vfs.stat(node).ok()?;
                (st.kind == FileType::Regular).then_some(node)
            }
            Output::Program { stdout, .. } => stdout.node(),
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 410 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): take only a regular file for the output's node

The in-process runner gave a redirection's node as standard output's
whatever it was, so in host-shell grep x /dev/null > /dev/null said
the input file is also the output, status 2, where GNU's grep, which
compares only regular files, says nothing, status 1. Ctx::output_node
now names only a regular file, as relay-rt's SysStdout::node does (the
prototype's review, M-2).

Refs: review M-2
EOF
````


### Task 13: A character device's `seek` gives 0

Decision 7: an open file remembers whether it is a character device, and its `seek` then gives 0 whatever offset it is asked for, as Linux's `/dev/null`'s does; its reads and writes leave the offset to the device, which ignores it (guards there survived their mutants: nothing can see that offset). The red run is the kernel's tests. Mutation checks (3): never a device, the seek moving, and every node a device, each fail a test.

**Files:**
- Modify: `kernel/src/file.rs`

**Interfaces:**
- Consumes: Task 10's `DevFs`.
- Produces: `OpenFile`'s `device` (private).

- [ ] **Step 1: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
    #[test]
    fn what_a_file_was_not_opened_for_is_ebadf() {
````

with:

````rust
    #[test]
    fn a_character_device_s_seek_gives_0() {
        // As Linux's /dev/null: every seek gives 0, after writes too
        // (programmable shell gate §8.4).
        let mut t = table();
        t.mount(b"/dev", Box::new(vfs::DevFs::new(1_000))).unwrap();
        let f = open(&mut t, b"/dev/null", RW | OPEN_TRUNCATE).unwrap();
        assert_eq!(f.write(&mut t, b"gone"), Ok(4));
        assert_eq!(f.seek(&mut t, 0, SEEK_CURRENT), Ok(0));
        assert_eq!(f.seek(&mut t, 100, SEEK_START), Ok(0));
        assert_eq!(f.seek(&mut t, -5, SEEK_END), Ok(0));
        assert_eq!(f.seek(&mut t, 0, 9), Ok(0), "whatever is asked");
        let mut buf = [0u8; 8];
        assert_eq!(f.read(&mut t, &mut buf), Ok(0));
        let a = open(&mut t, b"/dev/null", OPEN_WRITE | OPEN_APPEND).unwrap();
        assert_eq!(a.write(&mut t, b"more"), Ok(4));
        assert_eq!(a.seek(&mut t, 0, SEEK_CURRENT), Ok(0));
        // A regular file's moves as before.
        let g = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(g.seek(&mut t, 100, SEEK_START), Ok(100));
    }

    #[test]
    fn what_a_file_was_not_opened_for_is_ebadf() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib file::`

Expected: FAIL: 1 test fails: `file::tests::a_character_device_s_seek_gives_0`.

- [ ] **Step 3: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! every fd sharing the open file shares (the fds `spawn` hands a child);
//! each `open` makes a new one with an offset of its own, as on Linux.
//!
````

with:

````rust
//! every fd sharing the open file shares (the fds `spawn` hands a child);
//! each `open` makes a new one with an offset of its own, as on Linux. A
//! character device's `seek` gives 0, whatever it is asked, as Linux's
//! `/dev/null`'s does (programmable shell gate §8.4); its reads and writes
//! ignore the offset.
//!
````

Replace:

````rust
    dir: bool,
    state: Mutex<State>,
````

with:

````rust
    dir: bool,
    /// A character device: `seek` gives 0.
    device: bool,
    state: Mutex<State>,
````

Replace:

````rust
    };
    let dir = !created && vfs.stat(node)?.kind == FileType::Directory;
    // Linux's rule: a directory is neither written nor created by `open`.
````

with:

````rust
    };
    let kind = if created {
        FileType::Regular
    } else {
        vfs.stat(node)?.kind
    };
    let dir = kind == FileType::Directory;
    // Linux's rule: a directory is neither written nor created by `open`.
````

Replace:

````rust
        dir,
        state: Mutex::new(State {
````

with:

````rust
        dir,
        device: kind == FileType::CharDev,
        state: Mutex::new(State {
````

Replace:

````rust
    /// unknown `whence` are `EINVAL`. A directory only goes back to its
    /// start, where `read_dir` begins again.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.dir {
````

with:

````rust
    /// unknown `whence` are `EINVAL`. A directory only goes back to its
    /// start, where `read_dir` begins again; a character device's `seek`
    /// gives 0.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.device {
            return Ok(0);
        }
        if self.dir {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 401 tests.

- [ ] **Step 5: Run the `files`, `fileops` scenarios**

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
feat(kernel): give a character device's seek 0

An open file remembers whether it is a character device, and its seek
then gives 0 whatever it is asked, as Linux's /dev/null's does; reads
and writes leave the offset to the device, which ignores it
(programmable shell gate §8.4).
EOF
````


### Task 14: A character device refuses an unknown `whence`

Decision 7 (the prototype's review, M-3): Task 13's `seek` gave 0 before it looked at `whence`, so an unknown one succeeded where Linux's `lseek` is `EINVAL` before it asks the device, and where the call's own rule says `EINVAL` (user-space gate §7.3). It now checks `whence` first. The red run is the kernel's tests. Mutation check (1): any `whence` accepted fails a test.

**Files:**
- Modify: `kernel/src/file.rs`

**Interfaces:**
- Consumes: Task 13.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn a_character_device_s_seek_gives_0() {
        // As Linux's /dev/null: every seek gives 0, after writes too
        // (programmable shell gate §8.4).
        let mut t = table();
````

with:

````rust
    fn a_character_device_s_seek_gives_0() {
        // As Linux's /dev/null: every seek gives 0, after writes too, and
        // to any offset (programmable shell gate §8.4).
        let mut t = table();
````

Replace:

````rust
        assert_eq!(f.seek(&mut t, -5, SEEK_END), Ok(0));
        assert_eq!(f.seek(&mut t, 0, 9), Ok(0), "whatever is asked");
        let mut buf = [0u8; 8];
````

with:

````rust
        assert_eq!(f.seek(&mut t, -5, SEEK_END), Ok(0));
        // But for an unknown whence, which Linux refuses before it asks
        // the device (the prototype's review, M-3).
        assert_eq!(f.seek(&mut t, 0, 9), Err(Errno::EINVAL));
        let mut buf = [0u8; 8];
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib file::`

Expected: FAIL: 1 test fails: `file::tests::a_character_device_s_seek_gives_0`.

- [ ] **Step 3: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
    /// start, where `read_dir` begins again; a character device's `seek`
    /// gives 0.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.device {
            return Ok(0);
        }
````

with:

````rust
    /// start, where `read_dir` begins again; a character device's `seek`
    /// gives 0 for any offset, a known `whence` first, as Linux's.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.device {
            return match whence {
                SEEK_START | SEEK_CURRENT | SEEK_END => Ok(0),
                _ => Err(Errno::EINVAL),
            };
        }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 401 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
fix(kernel): refuse an unknown whence on a character device

A character device's seek gave 0 before it looked at whence, so an
unknown one succeeded where Linux's lseek is EINVAL before it asks the
device, and where the seek call's own rule says EINVAL (user-space gate
§7.3). It now checks whence first (the prototype's review, M-3).

Refs: review M-3
EOF
````


### Task 15: The kernel mounts `/dev` at startup

Decision 7: a new module, `kernel/src/dev.rs`, mounts `DevFs` at `/dev` after the root (startup step 9) and before `/bin` (step 10), over whatever the disk has there, so `/dev` is filesystem 2 and `/bin` 3; its line is `[ ok ] dev: /dev/null`, or `[FAIL] dev: cannot mount /dev: <reason>` (a disk whose `/dev` is a file) and the boot goes on. The `boot` scenario expects the line, `mount_fail`'s empty root lists `dev` beside `bin`, check 3's `dmesg` expectations gain it, and so do both of its transcripts (the NUC's by a command, until milestone 5's NUC run) and the checklist. The red run is the kernel's tests: the module is missing.

**Files:**
- Modify: `docs/hardware-test.md`
- Create: `kernel/src/dev.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/mount_fail.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`

**Interfaces:**
- Consumes: Task 10's `DevFs`.
- Produces: `dev::{mount, mount_into, failure}`.

- [ ] **Step 1: Write the failing tests for `kernel/src/dev.rs`**

Create `kernel/src/dev.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use vfs::{Env, FileSystem, FileType, MemFs, Vfs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    #[test]
    fn dev_null_is_mounted_before_bin() {
        // The root's /dev, as QEMU's disk has one, or none at all.
        for with_dir in [true, false] {
            let mut fs = MemFs::new(Box::new(Clock));
            if with_dir {
                let root = fs.root();
                fs.mkdir(root, b"dev").unwrap();
            }
            let mut t = MountTable::new(Box::new(fs.read_only()));
            assert_eq!(mount_into(&mut t, 1_000), Ok(()));
            let null = t.lookup(b"/dev/null").unwrap();
            assert_eq!(null.mount, 1, "filesystem 2, before /bin");
            let st = t.stat(null).unwrap();
            assert_eq!((st.kind, st.mtime), (FileType::CharDev, 1_000));
        }
    }

    #[test]
    fn a_dev_that_is_a_file_fails_the_mount() {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        fs.create(root, b"dev").unwrap();
        let mut t = MountTable::new(Box::new(fs));
        let e = mount_into(&mut t, 0).unwrap_err();
        assert_eq!(e, Errno::ENOTDIR);
        assert_eq!(failure(e), "cannot mount /dev: Not a directory");
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod console;
pub mod error_screen;
````

with:

````rust
pub mod console;
pub mod dev;
pub mod error_screen;
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
````

with:

````text
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
# /dev, before /bin (programmable shell gate §8.4).
expect \[ ok \] dev: /dev/null
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/mount_fail.txt`**

In `tests/e2e/mount_fail.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# shell starts on the empty read-only /, where only the programs of /bin
# are (user-space gate spec §4.4). Afterwards the runner puts the
# magic back and e2fsck finds the filesystem untouched.
break-root
````

with:

````text
# shell starts on the empty read-only /, where only the programs of /bin
# are (user-space gate spec §4.4), and /dev (programmable shell gate
# §8.4). Afterwards the runner puts the magic back and e2fsck finds the
# filesystem untouched.
break-root
````

Replace:

````text
send ls -a /
expect \n\.\s+\.\.\s+bin\n
expect root@relay:/# $
````

with:

````text
send ls -a /
expect \n\.\s+\.\.\s+bin\s+dev\n
expect root@relay:/# $
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `mount_into` in this scope ``; `` cannot find type `MountTable` in this scope ``.

- [ ] **Step 6: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 45 programs, ABI 4` (the programs of `/bin`, read
````

with:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] dev: /dev/null` (`/dev`, a filesystem of the kernel's)
   - `[ ok ] system: 45 programs, ABI 4` (the programs of `/bin`, read
````

- [ ] **Step 7: Implement `kernel/src/dev.rs`**

Insert this at the top of `kernel/src/dev.rs`, above `#[cfg(test)]`:

````rust
//! `/dev` (programmable shell gate §8.4): after the root (startup step 9)
//! and before `/bin` (step 10), the kernel mounts a `vfs::DevFs` at `/dev`,
//! over whatever the disk has there, so `/dev` is filesystem 2. Its line
//! is `[ ok ] dev: /dev/null`, or `[FAIL] dev: cannot mount /dev: <reason>`
//! (a disk whose `/dev` is a file), and the boot goes on without it.

use crate::{console, rtc};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use vfs::{DevFs, Errno, MountTable};

/// Mounts `/dev`, made at `now`.
pub fn mount_into(vfs: &mut MountTable, now: u64) -> Result<(), Errno> {
    vfs.mount(b"/dev", Box::new(DevFs::new(now)))
}

/// The reason on the `[FAIL]` line.
pub fn failure(e: Errno) -> String {
    format!("cannot mount /dev: {}", e.message())
}

/// Mounts `/dev` and prints its startup line.
pub fn mount(vfs: &mut MountTable) {
    match mount_into(vfs, rtc::now_unix().unwrap_or(0)) {
        Ok(()) => console::ok(format_args!("dev: /dev/null")),
        Err(e) => console::fail("dev", format_args!("{}", failure(e))),
    }
}

````

- [ ] **Step 8: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    let mut vfs = vfs::MountTable::new(root);
    if let Err(e) = system::mount(info, &mut vfs) {
````

with:

````rust
    let mut vfs = vfs::MountTable::new(root);
    dev::mount(&mut vfs);
    if let Err(e) = system::mount(info, &mut vfs) {
````

- [ ] **Step 9: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 4
````

with:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] dev: /dev/null
#> \[ ok \] system: \d+ programs?, ABI 4
````

- [ ] **Step 10: Add the line to the NUC transcript**

Run:

````bash
python3 - <<'PY'
p = "xtask/fixtures/checks/check3-a.nuc.log"
b = open(p, "rb").read()
mount = b"] mount /: ext2 on 00:14.0 port 15 partition 2, 2.0 GiB\n"
assert b.count(mount) == 1
b = b.replace(mount, mount + b"[\x1b[32m ok \x1b[0m] dev: /dev/null\n")
open(p, "wb").write(b)
PY
````

- [ ] **Step 11: Change `xtask/fixtures/checks/check3-a.qemu.log`**

In `xtask/fixtures/checks/check3-a.qemu.log`, replace:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] system: 1 program, ABI 4
````

with:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] dev: /dev/null
[ ok ] system: 1 program, ABI 4
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 403 tests.

- [ ] **Step 13: Run the `boot`, `mount_fail`, `noroot`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario mount_fail`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario noroot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add docs kernel rootfs tests xtask
git commit -F - <<'EOF'
feat(kernel): mount /dev/null at startup

After the root and before /bin the kernel mounts DevFs at /dev, over
whatever the disk has there, so /dev is filesystem 2 and /bin 3. Its
line is [ ok ] dev: /dev/null, or [FAIL] dev: cannot mount /dev:
<reason> with the boot going on (programmable shell gate §8.4). The
boot scenario, check 3's dmesg lines and both of its transcripts (the
NUC's by hand until milestone 5's NUC run), the checklist and the
empty root's listing in mount_fail follow.
EOF
````


### Task 16: `host-shell` mounts `/dev`

Decision 7: `host-shell` mounts `DevFs` at `/dev` over the image's directory, as the kernel does, so `> /dev/null` discards what it is given where it wrote a regular file into the image, and `[ -c /dev/null ]` is true. Its session test types both and checks the image has no `/dev/null`. The red run is xtask's `host_shell` tests.

**Files:**
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: Task 10's `DevFs`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let mut console = Script {
            input: b"mkdir /root/notes\recho hello > /root/notes/a\rcat /root/notes/a\rpoweroff\r"
                .iter()
````

with:

````rust
        let mut console = Script {
            input: b"mkdir /root/notes\recho hello > /root/notes/a\rcat /root/notes/a\r\
                echo gone > /dev/null\r[ -c /dev/null ] && echo device\rpoweroff\r"
                .iter()
````

Replace:

````rust
        );

````

with:

````rust
        );
        // /dev/null is the kernel's DevFs here too, not a file of the image.
        assert!(screen.contains("&& echo device\ndevice\n"), "{screen}");

````

Replace:

````rust
        assert_eq!(cat, "hello\n");
        let header = run_stdout(Command::new("dumpe2fs").args(["-h", &target])).unwrap();
````

with:

````rust
        assert_eq!(cat, "hello\n");
        let ls = run_stdout(Command::new("debugfs").args(["-R", "ls /dev", &target])).unwrap();
        assert!(!ls.contains("null"), "{ls}");
        let header = run_stdout(Command::new("dumpe2fs").args(["-h", &target])).unwrap();
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask host_shell`

Expected: FAIL: 1 test fails: `host_shell::tests::a_session_changes_the_image_and_leaves_it_clean`.

- [ ] **Step 3: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! ext2 root partition of an image file (spec §9.1), through a file-backed
//! `BlockDevice`. For trying the filesystem and the shell's commands
//! without a machine: the in-process runner runs the command functions
//! (no programs, no `&`). It changes the image in place.

````

with:

````rust
//! ext2 root partition of an image file (spec §9.1), through a file-backed
//! `BlockDevice`, with the kernel's `/dev` mounted over the image's. For
//! trying the filesystem and the shell's commands without a machine: the
//! in-process runner runs the command functions (no programs, no `&`). It
//! changes the image in place.

````

Replace:

````rust
use std::time::{SystemTime, UNIX_EPOCH};
use vfs::{BlockDevice, Env, IoError, MountTable, Vfs, check_request};

````

with:

````rust
use std::time::{SystemTime, UNIX_EPOCH};
use vfs::{BlockDevice, DevFs, Env, IoError, MountTable, Vfs, check_request};

````

Replace:

````rust
    let mut vfs = MountTable::new(Box::new(fs));
    let mut system = HostSystem(log);
````

with:

````rust
    let mut vfs = MountTable::new(Box::new(fs));
    // The kernel's /dev (programmable shell gate §8.4), over the image's.
    if let Err(e) = vfs.mount(b"/dev", Box::new(DevFs::new(now()))) {
        console.write(format!("cannot mount /dev: {e}\n").as_bytes());
    }
    let mut system = HostSystem(log);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask host_shell`

Expected: PASS: 3 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
feat(xtask): mount /dev in host-shell

host-shell mounts DevFs at /dev over the image's directory, as the
kernel does, so `> /dev/null` discards what it is given where it wrote
a regular file into the image, and `[ -c /dev/null ]` is true
(programmable shell gate §8.4, §15 item 6).
EOF
````


### Task 17: `/dev/null` and a read-only root in QEMU

Decisions 7 and 6: the `redirect` scenario gains `/dev/null`'s lines under `/bin/sh`: errors sent to it, a write, an append and a read of it, `wc` and `head` reading it, `cp` emptying a file from it, `[ -c ]` and `[ -w ]` true and `[ -t 1 ]` false with it as the output, `ls /dev`, and `EPERM` for a new file in `/dev` and for removing `null`; `mount_fail`'s read-only root answers `[ -w / ]` with 1. Tests only: no failing run.

**Files:**
- Modify: `tests/e2e/mount_fail.txt`
- Modify: `tests/e2e/redirect.txt`

**Interfaces:**
- Consumes: Tasks 10–16, 8.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/mount_fail.txt`**

In `tests/e2e/mount_fail.txt`, replace:

````text
expect root@relay:/# $
send echo still here
````

with:

````text
expect root@relay:/# $
# test -w knows the root is read-only (programmable shell gate §15 item 6).
send [ -w / ]; echo "status $?"
expect \nstatus 1\nroot@relay:/# $
send echo still here
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/redirect.txt`**

In `tests/e2e/redirect.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# line; `sh FILE` refusing to send its errors to a file but reading one;
# errors sent to a file kept out of a script's transcript; and Ctrl-C
# reaching a program whose input is a file.
timeout 60
````

with:

````text
# line; `sh FILE` refusing to send its errors to a file but reading one;
# errors sent to a file kept out of a script's transcript; Ctrl-C
# reaching a program whose input is a file; and /dev/null (plan 2).
timeout 60
````

Replace:

````text
expect \n\+ ls r-nope 2> r-te\nls: cannot access 'r-nope': No such file or directory\nroot@relay:~# $
# A program whose input is a file still has the console's foreground.
````

with:

````text
expect \n\+ ls r-nope 2> r-te\nls: cannot access 'r-nope': No such file or directory\nroot@relay:~# $
# /dev/null (plan 2, §8.4): what goes to it is gone, a read of it ends at
# once, it is a character device that is not the console, and nothing
# else can be made or removed in /dev.
send ls r-f r-nope 2> /dev/null; echo "status $?"
expect \nr-f\nstatus 2\nroot@relay:~# $
send echo gone > /dev/null; echo more >> /dev/null; cat /dev/null; echo "status $?"
expect \nstatus 0\nroot@relay:~# $
send wc -c < /dev/null; head -n 1 < /dev/null; echo "status $?"
expect \n0\nstatus 0\nroot@relay:~# $
send echo old > r-n; cp /dev/null r-n; wc -c < r-n
expect \n0\nroot@relay:~# $
send [ -c /dev/null ] && [ -w /dev/null ] && echo device
expect \ndevice\nroot@relay:~# $
send [ -t 1 ] > /dev/null; echo "status $?"; [ -t 1 ]; echo "status $?"
expect \nstatus 1\nstatus 0\nroot@relay:~# $
send ls /dev
expect \nnull\nroot@relay:~# $
send echo a > /dev/x; rm /dev/null
expect \nrelay-sh: /dev/x: Operation not permitted\nrm: cannot remove '/dev/null': Operation not permitted\nroot@relay:~# $
# A program whose input is a file still has the console's foreground.
````

- [ ] **Step 3: Run the `redirect`, `mount_fail` scenarios**

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario mount_fail`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): run /dev/null and a read-only root's test -w in QEMU

The redirect scenario gains /dev/null's lines: errors sent to it, a
write, an append and a read of it, wc and head reading it, cp emptying
a file from it, [ -c ] and [ -w ] true and [ -t 1 ] false with it as
the output, ls /dev, and EPERM for a new file in /dev and for removing
null. mount_fail's read-only root answers [ -w / ] with 1
(programmable shell gate §8.4, §11.3, §15 item 6).
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 51 scenario(s) passed`.

````bash
git push -u origin m5p2/dev
gh pr create --base main --head m5p2/dev --title "feat(vfs,kernel): add /dev/null" --body-file - <<'EOF'
## What

Milestone 5, plan 2, tasks 10–17: `vfs::DevFs`, a root directory and `null` (a character device: reads end at once, writes keep nothing, nothing made or removed in `/dev`, `EPERM`), mounted at `/dev` after the root and before `/bin` with the startup line `[ ok ] dev: /dev/null`, and in `host-shell`; a character device's `seek` gives 0 after checking `whence`, as Linux's; `cp` reads a character device as its source (`cp /dev/null f` empties `f`, as GNU's); the in-process runner's output is a file only when it is a regular one, so `grep x /dev/null > /dev/null` is no input that is the output; the `redirect` scenario's `/dev/null` lines and `[ -w / ]` on `mount_fail`'s read-only root; check 3's `dmesg` lines and transcripts gain the startup line.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: no NUC check before milestone 5's plan 4; check 3's NUC transcript gets the new startup lines by hand until plan 4's run
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p2-dev
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
