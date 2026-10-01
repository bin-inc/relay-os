# Milestone 3 · Plan 1: Pipes and standard input — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs are connected with pipes. The kernel gains the `pipe` call (a 16 KiB ring, blocking both ways, `EPIPE`) and a child joining another child's process group (`SpawnArgs::pgid`, ABI 3); `relay-rt` ends a program whose output nobody reads quietly with 141; the shell parses `a | b | c` and runs a pipeline in one process group under `/bin/sh` and in memory under the in-process runner; `cat`, `wc` (with `-c`, `-l` and `-w`), `head` and `tail` read standard input; `grep`, `seq`, `sleep`, `true` and `false` join `/bin`. The spec gets §16 item 8 and milestone 3 its roadmap. It ends with `cargo xtask ci` green, the new scenarios `pipe_calls` and `pipes` among the 43.

**Architecture:** A pipe (`kernel/src/pipe.rs`) is a ring in four frames of user memory, with read and write ends that the fd table holds as `File::Pipe` and that close, waking the other side, when their last fd goes; its calls never wait: the dispatcher's `syscall/pipes.rs` waits through the process (`Caller::pipe_wait`, `Blocked::Pipe`). The process table's `insert` takes a `Group`, so `spawn` can put a child in the group of another child of its caller's. In the shell crate, `parser::parse` returns a `Pipeline`, a command's `Ctx` gains standard input (`Stdin`: a program's fd 0, or `Bytes` in memory), `Runner` gains `pipeline`, and `Programs` gains `pipe` and a `Group` for each `spawn`. `grep`'s matcher (`crates/shell/src/pattern.rs`) is an NFA over bytes. Host tests compare `wc`, `head`, `tail`, `grep` and `seq` with the host's own through `testing::host_tool`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model); for the comparisons, the host's GNU coreutils 9.4 and GNU grep 3.11 (`LC_ALL=C`), and bash 5 for the expectations.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.2, §6.4, §7.3, §7.4, §8.1–§8.5, §9.1, §12.1, §12.3, §13 step 6, §16 item 8)
**Roadmap:** `docs/superpowers/plans/2026-10-01-milestone-3-roadmap.md` — this is plan 1 of 4 of milestone 3 (spec §13 step 6).

## In brief

- **Size.** 23 tasks in three code pull requests, plus this plan as PR 1 (decision 1): PR 2 pipes in the kernel and the group a child joins; PR 3 pipelines and standard input in the shell; PR 4 the new programs. No NUC check: nothing here is particular to the NUC, and plan 4's `check5.sh` runs pipes there.
- **ABI 3** (decision 3). A pipeline's stages must share a process group, and `spawn` could not put a child in any group but its parent's or a new one; `SpawnArgs` gains `pgid` (and a reserved word), which changes its layout, so `relay_abi::VERSION` is 3. A spike ran every scenario with ABI 3 and a throwaway pipe under `/bin/sh` first: only the startup line, `t-abi`'s ABI, `check3-a.sh` and the recorded transcripts follow the number (the NUC's get `ABI 3` by hand until plan 4's NUC checks), and pipes need nothing else to change.
- **Pipes** (decision 2) live in frames of user memory, not the kernel's heap, so `ENOMEM` comes at the 8 MiB reserve and `free` counts them. A read waits only while the pipe is empty and a writer is open, a write only while nothing of it has gone in; a killed waiter ends; an end closes with the last fd that has it, in any process.
- **Pipelines** (decision 7): the first stage that starts gets a new group and the console, the others join it; the shell closes its copies of the pipe ends at once, reports a stage that cannot start or was killed (Ctrl-C once), and takes the last stage's status; a writer whose reader is gone ends quietly with 141 (decision 4). `cd`, `exit` and `help` cannot be in a pipeline (`relay-sh: cd: cannot be used in a pipeline`, not §9.1's `sh:`).
- **Standard tools, compared with the real ones** (decisions 5, 6, 9, 10): `wc`, `head`, `tail`, `grep` and `seq` are compared with the host's GNU tools; `grep` with every pattern of up to three symbols of a small alphabet. The comparison found that milestone 1's `wc` padded its columns for a missing file where GNU's does not.
- **Plan 5's deferred minors** (decision 13): the roadmap's `verify-usb` wording (PR 1) and `t-spawn fill`'s bound (Task 6); the other five go to plan 4.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran every crate's tests, throwaway host probes against the host's GNU tools and QEMU probe scenarios under KVM on the NUC's CPU model (an i7-1260P), and found no critical, 4 important and 7 minor real defects. Each is fixed in a task of its own after the task it concerns, with a test that fails first and the mutation checks that show what it guards, but the commit-message nits, fixed in the commits. It found correct the ring and its partial reads and writes, every place a pipe's end can be dropped (none under `PROCS` or `MEMORY`), the absence of a lost wake-up, the frames and their reserve, the group a child joins (an ended first stage, the re-check under the lock, the console with an ended first stage), the spawning runner's fds on every path, Ctrl-C while data flows through a pipeline (every stage killed), and further comparisons of `wc`, `head`, `tail` and `grep` with GNU's:

| Finding (review) | Decision |
|---|---|
| Important (I1): a redirection alone as a pipeline's last command (`echo hi \| > f`) made `/bin/sh` panic, and three such lines within 10 s reached the error screen | Fixed, Task 12: every command of a pipeline has a name; `a \| > f` is refused as unsupported syntax |
| Important (I2): output into a pipe waited in pieces of 4 KiB, so a line typed into `cat \| cat` reached the second only after Ctrl-D | Fixed, Task 14: a command writes what waits before it reads more input |
| Important (I4): `X \| sh` set the console to raw mode with nobody reading it, so Ctrl-C could no longer end the pipeline | Fixed, Task 15: a `/bin/sh` whose fd 0 is no console runs the lines it reads and never takes the console |
| Important (I3): `grep 1 f >> f` read its own output until the disk was full | Fixed, Task 17: GNU's `input file is also the output`, status 2 |
| Minor (M1): `> f \| b` got bash's syntax error, which bash does not give | Fixed with I1, Task 12: `unsupported syntax: > before \|` |
| Minor (M2): `grep`'s status after a write error was 1, GNU's is 2 | Fixed with I3, Task 17 |
| Minor (M3): `seq x` said `not a whole number` instead of GNU's `invalid floating point argument` | Fixed, Task 22: ours only for numbers GNU's takes and this one does not |
| Minor (M4): `-` was a file name, not standard input, in `cat`, `wc`, `head`, `tail` and `grep` | Fixed, Task 19 |
| Minor (M5): `[a-z-9]` was taken, where GNU grep says `Invalid range end` | Fixed, Task 18 |
| Minor (M6): `testing::host_tool` deadlocked on a big output | Fixed with M4, Task 19: the input is written from a thread |
| Minor (M7): a commit body line of 72 columns counted as 73, and a commit with a third module (`xtask`'s `host-shell`) | Reworded: Task 16's body rewrapped; Task 21 scoped `shell`, the trait's implementations belonging to its commit (Working conventions) |
| Declined to judge: the in-process runner holding an endless stage's output (host only); a pipe error's message after the started stages end; when `grep` calls a big input binary; GNU options outside §9.1 | Ruled: the in-process runner is decision 7's design for the tests and `host-shell`; a pipe error needs `ENOMEM` or `EMFILE` in the shell; `grep`'s binary rule matched GNU's on every input tried; the options are outside spec §9.1 |

## Where this plan fits

Plan 1 of milestone 3 implements spec §13 step 6. It builds on milestone 2 (0.3.0): processes and groups, the fd table, the console's line discipline, `/bin/sh` and its two runners. It leaves plan 2 (jobs, `ps` and `kill`) a shell whose pipelines are one process group each, which `&` will start without the console, and `spawn`'s `Group::Join` for `a | b &` (roadmap).

## Working conventions

- Plan 1 lands as **four pull requests** (table below). This plan, with milestone 3's roadmap, the spec's §16 item 8 (and the parts of its body it corrects) and the corrected line of milestone 2's roadmap, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-1-pipes-and-standard-input.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
  - Files holding escape bytes (the recorded check transcripts) are changed by a command instead.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`; test programs under `userland/tests/`, with the `relay-rt` calls they need), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 1, 2, 4, 5, 8, 13, 14, 15 and 23 have scenario red runs too. Task 6 has no failing run: it bounds a test program, and its introduction says what the mutation check showed. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m3p1/mutations.md`).
- **Bound every loop in a test, and remember that pipes block both ways.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; a pipe read or write can wait for ever. Every guest loop here has a bound and says `and no end` at it; host tests that feed endless input stop it themselves. Mutation checks run under an address-space limit and a timeout (`tmp/m3p1/mutate.py`).
- **The comparison tests run the host's own tools** (`wc`, `head`, `tail`, `seq`, `grep`) through `std::process::Command`, with `LC_ALL=C`, in directories under `target/like-host/`. GNU grep is `/usr/bin/grep`; a shell function of the same name in an interactive shell does not reach the tests. A missing tool fails the test; the CI runner has them all.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests, scenarios and transcripts that come with the change, and the implementations a changed trait forces elsewhere (`System::sleep` in `relay-rt` and `host-shell`), belong to its commit; Task 1's is a breaking change (`!` and a `BREAKING CHANGE:` footer). Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `d435aa1` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m3p1/plan` | — | Milestone 3's roadmap, the spec's §16 item 8 and the corrections to its body, milestone 2's roadmap's `verify-usb` line, this plan | `lint`, `unit`, `e2e` |
| 2 | `m3p1/kernel` | 1–6 | ABI 3 and a child joining a group; the pipe's ring and ends; the `pipe` call; the quiet 141; `t-spawn fill`'s bound | `lint`, `unit`, `e2e` |
| 3 | `m3p1/shell` | 7–15 | The parser's `|`; standard input and `cat`; `wc`'s options; `head` and `tail`; pipelines in both runners; output before input; `X \| sh` | `lint`, `unit`, `e2e` |
| 4 | `m3p1/utils` | 16–23 | `grep` (never its own output; GNU's ranges), `-` as standard input, `seq`, `sleep`, `true`, `false`, and their programs | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 1 adds no crate. The kernel has no dev-dependencies and does not depend on `shell`; `crates/usb` has no dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail, and keeps the same values on every architecture.
- Every command prints exactly what it prints in milestone 1 for files, and every milestone 1 scenario passes unchanged but for the startup line (gate §1.4). The new commands print what GNU's print, first lines of messages only (decision 11).
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. Nothing waits for ever but the error screen and a program waiting for its own input. Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table. A pipe's end is never dropped while `PROCS` is held.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 120×33 cells and no serial port.
- Shell messages follow GNU coreutils and bash; `/bin/sh` names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 8 in PR 1:

1. **Milestone 3's plans** (§13). Its roadmap keeps §13's four plans, named `m3-plan-1` to `m3-plan-4` (steps 6–9). Background jobs stay in milestone 3 (§1.2): nothing calls for the cut. Plan 1 is one plan in four pull requests: this plan; the kernel's pipes and the group a child joins; the shell's pipelines and standard input; the new programs. It has no NUC check: nothing it changes is particular to the NUC (QEMU's USB keyboard types into a pipeline), and plan 4's `check5.sh` runs pipes there.
2. **Pipes** (§7.3, §9.1; `kernel/src/pipe.rs`). `pipe` makes a 16 KiB ring in four contiguous frames of its own, not the kernel's heap, which would hold only about 1000 of them and which `spawn` needs for the programs it reads. The frames are user memory's: `ENOMEM` once fewer than 8 MiB of frames would be left (§11.1), and `free` counts them. The memory for the two fds is checked first (`EFAULT`), then two free fds (`EMFILE`), before anything is made; the ends take the lowest free fds, the read end first. A read gets what the pipe holds, up to its length (and the ring's), and waits only while it is empty and a writer is open; 0 once every write end has closed. A write takes what fits and waits only while nothing of it has gone in, so a full pipe gives fewer bytes than asked (which `relay-rt`'s `write_all` continues); `EPIPE` once every read end has closed. An end closes when the last fd that has it closes, in its process or any child `spawn` gave it to, or when a process ends, and the processes waiting on the pipe are woken. A process killed while it waits gets `EINTR`, which never reaches its program (it ends first, §16 item 3). `fstat` says a FIFO, with `dev` 0; `seek` is `EINVAL`, `read_dir` `ENOTDIR`; reading a write end or writing a read end is `EBADF`; a pipe is no tee (`EINVAL`, as the console: a tee is written from any process, where a full pipe could not wait). The ring's calls never wait; the dispatcher does, through the process (`Caller::pipe_wait`), on `Blocked::Pipe` of the pipe's id, and a pipe is made with the function that wakes its waiters, so the host tests wake no process table. A pipe's lock is held only inside its own calls, and an end is never dropped while the process table is locked (a debug assertion).
3. **Joining a group** (§6.4, §7.3, §7.4). `SpawnArgs` gains `pgid` and a reserved word that must be 0 (128 bytes): without `NEW_GROUP`, a `pgid` other than 0 is a group the child joins instead of its parent's. It must be the group of one of the caller's children, an ended one not yet collected included (`EPERM` otherwise, Linux's `setpgid` answer), so a pipeline's later stage can join a first stage that has ended already (`true | cat`); with `NEW_GROUP` it is `EINVAL`. The changed layout makes `relay_abi::VERSION` 3: `system: 40 programs, ABI 3`, `ABI 99, kernel wants 3`, and `t-abi` built for ABI 2 (`STALE_ABI` is the ABI before). The recorded NUC transcripts get `ABI 3` by hand until plan 4's NUC checks record real ones.
4. **`relay-rt`** (§8.1). `sys::pipe`, and `sys::spawn` takes the group to join. A write to fd 1 that fails with `EPIPE` ends the program with 141 (`BROKEN_PIPE`) and no message, in `sys::write` itself, so every way to fd 1 does so; a pipe of the program's own still sees `EPIPE`. `SysStdin` reads fd 0.
5. **Standard input** (§9.1). A command gets standard input (`shell::Stdin`) as it gets standard output: a program's fd 0, which is the console in line mode (a line a read, Ctrl-D its end, Ctrl-C for the whole group) or a pipe; or bytes in memory (`shell::Bytes`) in the in-process runner, whose first input is what a test gives (`Shell::with_input`) and in `host-shell` nothing, so `cat` alone ends at once there. `run_command` takes what a program works with as one `CommandIo`. `cat`, `wc`, `head` and `tail` without a file read it, and so does a file named `-` (`cat header - footer`), as GNU's do; `cat` no longer says `missing operand`. A command writes out what waits for its standard output before each read of its input, so a line typed into `cat | cat` reaches the second at Enter (the prototype's review found it held in 4 KiB pieces until Ctrl-D). `head` stops reading once its lines are out, so a writer before it gets `EPIPE`; `tail` keeps the last lines as they come, a line cut across reads joined. GNU's names for standard input in messages are kept (`cat: -: …`, `error reading 'standard input'`).
6. **`wc`** (§9.1, §12.3). `wc` gains `-c`, `-l` and `-w` (§12.3's `cat big | wc -c` needs `-c`), and its widths are GNU wc's: one count of one input is not padded; otherwise the columns fit the regular files' total size, with at least 7 digits when an input is something else (standard input is never a regular file here). A test compares it with the host's `wc`, which showed that a file that cannot be found takes no part in the widths: milestone 1's `wc` padded to 7 for one, and now does not.
7. **Pipelines in the shell** (§8.2, §9.1). An unquoted `|` joins commands; bash's syntax errors name a `|` with nothing before it (``syntax error near unexpected token `|'``) or after it (`syntax error: unexpected end of file`); `||` is refused as unsupported syntax, and so is a redirection on a command before the last (`unsupported syntax: > before |`), where bash would send that command's output into the file. `cd`, `exit` and `help` cannot be in a pipeline: `relay-sh: cd: cannot be used in a pipeline`, status 1 (§9.1 said `sh:`; the shell's own messages say `relay-sh:`, §16 item 5), and nothing of the line runs. Every command of a pipeline has a name: a redirection alone, which bash runs as a command, is refused (`> f | b`: `unsupported syntax: > before |`; `a | > f`: `unsupported syntax: | >`; the review found `/bin/sh` panicking on the second). `/bin/sh` starts the stages left to right in one group: the first that starts gets a new group and the console, the others join it (`Group::Join`), and a script's stay in the script's group. Each pipe is made just before the stage that writes it starts, and the shell closes its copies of the ends as soon as the stages have them, so a reader sees its end once its writer has ended. A stage that cannot start says so at once, its neighbours see an end, and the others run, as in bash; a pipe that cannot be made (`relay-sh: pipe error: …`, status 1) stops the starting. The shell waits for every stage, says how any killed one ended, a Ctrl-C once (`^C`), and takes the last one's status; a stage that ended with 141 says nothing. The in-process runner runs the stages one after another, each one's output kept in memory as the next one's input, and refuses `sh` in a pipeline too (it would run its script in that shell); a stage that is not found gives the next nothing, and Ctrl-C stops the rest.
8. **`X | sh`** (§8.3). `/bin/sh` without arguments whose fd 0 is no console runs the lines it reads there, as they come, as bash does: without a prompt, a trace or the line editor, so it never takes the console (the review found `cat | sh` leaving it in raw mode, where Ctrl-C could no longer end the pipeline). It ends at the input's end or `exit`; a line over 64 KiB or not UTF-8 is skipped with an `sh:` message.
9. **`grep`** (§9.1; `crates/shell/src/pattern.rs`, `commands/grep.rs`). `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` matches bytes as GNU grep does with `LC_ALL=C`: literal bytes, `.`, `*` (literal at the start), `^` at the start and `$` at the end (literal elsewhere), `[...]` with ranges, `^` and `]` first, `\` before a byte for that byte; `-i` folds ASCII letters only. The escapes that mean something else in GNU's basic expressions (groups, intervals, `\|`, `\+`, `\?`, word anchors, back-references) and a set's classes are refused (`grep: \( \) is not supported`, status 2) rather than matched differently; a malformed pattern gets GNU's message (`Unmatched [, [^, [:, [., or [=`, `Trailing backslash`, `Invalid range end`, `Invalid regular expression`). Matching runs every item at once over the line (an NFA), in time proportional to the line times the pattern, whatever the pattern. Several files put each one's name first; an input with a NUL byte is binary, and `grep: f: binary file matches` replaces its lines; a file that is also its output is refused (`grep: f: input file is also the output`), as GNU's does, since it would read its own output for ever; the status is 0 if a line was selected, 1 if none, 2 after an error, a write error included, as GNU's (`Ctx::set_write_error_status`; the other commands keep 1). A range cannot start where one ended (`[a-z-9]`: `Invalid range end`). A test runs every pattern of up to three symbols of a small alphabet (1885) and the `-i` ones through the host's GNU grep: the same lines, or the same refusal.
10. **`seq`, `sleep`, `true`, `false`** (§9.1). `seq [FIRST [INCREMENT]] LAST` prints whole numbers as GNU seq does (it takes GNU's INCREMENT too, rather than calling a third operand extra; negative numbers are operands, not options); GNU's also takes decimals, exponents, `inf`, hexadecimal and numbers beyond 64 bits, of which this one says `seq: not a whole number: '1.5'`, and the options `-f`, `-s` and `-w`, which are unknown here; what GNU's refuses gets GNU's first line (`invalid floating point argument`, `invalid option`). `sleep` waits for the sum of its times, in seconds or with GNU's `s`, `m`, `h` and `d`, a fraction counting to the millisecond, through the shell's `System`, which gains `sleep`. `true` and `false` ignore their arguments. With them and `t-pipe`, `/bin` holds 40 programs.
11. **Messages.** The new commands print the first line of GNU's message for what they refuse, without GNU's `Try '… --help'` line, as milestone 1's commands do; `grep` without a pattern prints GNU's two usage lines, which are its message.
12. **Tests** (§8.5, §12). `t-pipe` (`basic`, `room`, `child`, `eof`, `epipe`, `killed`, `many`) and the scenario `pipe_calls`; `t-spawn join` in `spawn`; `cat` reading the console in `console`; the scenario `pipes`. Host tests compare `wc`, `head`, `tail`, `grep` and `seq` with the host's own (`testing::host_tool`, run in a directory under `target/` with `LC_ALL=C`, its input written from a thread; a missing tool fails the test).
13. **Plan 5's deferred minors.** The roadmap's `verify-usb` wording is corrected in this plan's first pull request; `t-spawn fill` is bounded at the table's 64; the other five go to plan 4.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A pipeline whose stages end at different times:** a reader that stops early (`head`), a stage killed by a fault, a stage that cannot start, a first stage that ends before the next one starts, Ctrl-C while stages wait on a full or an empty pipe, a pipeline in a script. Expected: no stage is left waiting for ever, the prompt comes back, a writer whose reader is gone ends quietly with 141, a killed stage is reported, Ctrl-C says `^C` once, and the status is the last stage's. Tests: `pipes` (Task 13), `a_stage_that_cannot_start_says_so_and_the_others_run` and `a_pipeline_says_how_its_killed_stages_ended_and_ctrl_c_once` (Task 13), `a_pipeline_command_without_a_name_is_refused_by_both_runners` (Task 12), `t-spin | sh` in `pipes` (Task 15), `pipe_calls`' `eof`, `epipe` and `killed` (Tasks 4 and 5), `t-spawn join` (Task 2).
2. **Typing into a pipeline:** the first stage reading the console in line mode, Backspace, Ctrl-D on an empty line and after text, Ctrl-C while it waits, a program alone reading the console (`cat`), the prompt afterwards. Expected: each line echoed once and handed over at Enter, to every later stage too, Ctrl-D ends the input, Ctrl-C ends every stage, and the shell takes the console back raw. Tests: `pipes` (Tasks 13 and 14), `console` (Task 8), `what_a_program_read_reaches_its_pipe_before_it_reads_again` (Task 14).
3. **Pipes at the kernel's limits:** pipes until the fds run out, frames running low, 8 MiB through a 16 KiB ring, one write bigger than the ring, a read into a buffer partly not the program's, many pipes made and closed. Expected: `EMFILE`, `ENOMEM` and `EFAULT` as values with nothing half-made, no byte lost or doubled, and `free` the same before and after. Tests: `pipe_calls` (Task 4), `a_pipe_s_ends_are_the_lowest_free_fds_read_end_first`, `what_goes_in_one_end_comes_out_of_the_other` and `an_empty_pipe_waits_and_a_killed_wait_is_eintr` (Task 4), `a_pipe_takes_four_frames_of_user_memory` (Task 4), `pipes` (Task 13).
4. **Text the tools meet:** input without a final newline, empty input, lines cut across reads, NUL bytes, very long lines, a pattern full of special characters, files that cannot be read, `-` for standard input, a file that is also the output. Expected: what GNU `wc`, `head`, `tail`, `grep` and `seq` print, byte for byte, with their statuses. Tests: `wc_prints_what_gnu_wc_prints` (Task 9), `head_and_tail_of_standard_input_print_what_gnu_s_print` and `tail_of_standard_input_joins_lines_a_pipe_cuts` (Task 10), `it_matches_what_gnu_grep_matches`, `it_ignores_case_as_gnu_grep_does` and `grep_prints_what_gnu_grep_prints` (Task 16), `grep_never_reads_its_own_output` (Task 17), `dashes_in_a_set_are_what_gnu_grep_makes_of_them` (Task 18), `cat_prints_what_gnu_cat_prints` (Task 19), `seq_prints_what_gnu_seq_prints` (Task 20), `seq_refuses_what_gnu_seq_refuses_as_it_does` (Task 22).
5. **Process groups:** a stage joining the group of a first stage that has ended, a program asking to join a group that is not one of its children's, a pipeline killed by Ctrl-C or by `kill` of its group. Expected: the join works for an ended child not yet collected, `EPERM` for any other group, and every stage ends. Tests: `a_child_may_join_the_group_of_another_child_of_its_parent` and `spawn` (Task 2), `pipes` (Task 13).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/relay-abi/src/{spawn,lib}.rs` | `SpawnArgs::{pgid, reserved}`, ABI 3 |
| `kernel/src/proc/table.rs`, `kernel/src/proc.rs`, `kernel/src/syscall.rs`, `kernel/src/init.rs` | `Group`, `may_join`; `spawn` into another child's group; `Blocked::Pipe`, `wake_pipe`, the process's `new_pipe`, `pipe_wait`, `pipe_wake` |
| `kernel/src/pipe.rs` | The pipe: a 16 KiB ring, its read and write ends, waking on close |
| `kernel/src/syscall/pipes.rs`, `kernel/src/syscall/testing.rs` | The `pipe` call, reading and writing an end, and the dispatcher's fake of them |
| `kernel/src/{fd,tty,mm/mod}.rs`, `kernel/src/syscall/files.rs` | `File::Pipe`; a pipe is no tee; the ring's frames under the reserve; `fstat`, `seek`, `read` of an end |
| `crates/vfs/src/errno.rs` | `EPIPE` |
| `crates/relay-rt/src/{sys,sysio,sysvfs,lib}.rs` | `sys::pipe`, `sys::spawn`'s group, the quiet 141, `SysStdin`, `SysPrograms`' pipes and groups, `SysSystem::sleep` |
| `crates/shell/src/{parser,io,ctx,program,runner,shell,lib}.rs` | `Pipeline`; `Stdin`, `Bytes`, `Group`, `Programs::pipe`, `System::sleep`; `Ctx::read_input`; `CommandIo`; pipelines in both runners |
| `crates/shell/src/commands/{text,grep,seq,system,basic,mod}.rs`, `crates/shell/src/pattern.rs` | `cat`, `wc`, `head`, `tail` on standard input, `wc -c -l -w`; `grep` and its matcher; `seq`; `sleep`; `true`, `false` |
| `crates/shell/src/testing.rs` | `Harness::stdin`, `like_host`, `host_tool`; `FakePrograms`' pipes and groups; `TestSystem::slept` |
| `userland/tests/src/bin/{t-pipe,t-spawn,…}.rs`, `userland/utils/src/bin/{grep,seq,sleep,true,false}.rs` | `t-pipe`; `t-spawn join` and `fill`'s bound; the five new programs |
| `xtask/src/{userland,host_shell}.rs`, `xtask/fixtures/checks/*.log`, `rootfs/root/checks/check3-a.sh` | `t-abi` built for ABI 2; `host-shell`'s `sleep`; `ABI 3` |
| `tests/e2e/{pipe_calls,pipes,spawn,console,system,boot,utils,system_nosh,system_abi,stale_program}.txt` | The new scenarios, and what ABI 3 and the new programs change |
| `docs/hardware-test.md`, `kernel/src/system.rs` | `system: 40 programs, ABI 3` |

---

## PR 1: Milestone 3's roadmap, the spec's revisions and this plan

Milestone 3's roadmap; the spec's §16 item 8 with the parts of its body it corrects (the status line, §4.3's startup line, §7.3's `spawn`, `fstat` and `pipe`, §7.4's version, §8.4's programs, §8.5's `t-spawn` and `t-pipe`, §9.1's pipelines, messages and programs, §12.3's `pipe_calls`, §13's plan names); milestone 2's roadmap's `verify-usb` line (plan 5's deferred minor 1); and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The roadmap and the spec's changes are the prototype's first commit, `docs: milestone 3's roadmap, and the spec's revisions from planning its plan 1`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p1/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m3p1/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-1-pipes-and-standard-input.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-01-m3-plan-1-pipes-and-standard-input.md
git commit -m "docs(plan): add milestone 3 plan 1, pipes and standard input"
cargo xtask lint
git push -u origin m3p1/plan
gh pr create --base main --head m3p1/plan --title "docs(roadmap,plan): add milestone 3's roadmap and its plan 1" --body-file - <<'EOF2'
## What

Milestone 3's roadmap (four plans, spec §13 steps 6–9; background jobs stay), the implementation plan of its plan 1 ("pipes and standard input"), and the spec's §16 item 8 with its decisions: pipes in frames of user memory; `SpawnArgs::pgid`, so a pipeline's stages share a process group (ABI 3); the quiet exit with 141; standard input for commands; `wc`'s options and GNU's widths; pipelines in both runners; `grep`, `seq`, `sleep`, `true` and `false`, compared with GNU's; the corrections to the spec's body they bring. Milestone 2's roadmap no longer says `verify-usb` compares a transcript with its script (plan 5's deferred minor 1).

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m3p1/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m3p1/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-plan` and continue with PR 2.

---

## PR 2: Pipes in the kernel, and the group a child joins (Tasks 1–6)

The kernel and `relay-rt`'s side of pipes: `SpawnArgs` gains the group a child joins (ABI 3), a pipe's ring and ends, the `pipe` call with its blocking and waking, the quiet exit with 141, and plan 5's deferred `t-spawn fill` bound. No shell runs a pipeline yet; `t-pipe` and `t-spawn join` show it all in QEMU.

Branch `m3p1/kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p1/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p1/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p1/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-kernel m3p1/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p1/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: `SpawnArgs` gains a group to join, and the ABI is 3

Spec §7.3, §7.4 and decision 3: a pipeline's later stages must join the first stage's process group, so that Ctrl-C and `kill` reach them all, and `spawn` could only put a child in its parent's group or a new one. `SpawnArgs` gains `pgid` and a reserved word (128 bytes); this task only makes room for them: the kernel refuses a `pgid` or a reserved word other than 0 (`EINVAL`) until Task 2 gives `pgid` its meaning. The changed layout makes `relay_abi::VERSION` 3 (spec §7.4), and a spike showed exactly what follows it: the kernel's tests of the startup line and the error screen's `ABI 99, kernel wants 3`, xtask's `t-abi` test (`t-abi` is `t-args` built for the ABI before, now 2), the scenarios `boot`, `system`, `utils`, `system_nosh`, `system_abi` and `stale_program`, `check3-a.sh`'s startup line, and the recorded transcripts, which hold escape bytes and are changed by `sed` (the NUC's get `ABI 3` by hand until plan 4's NUC checks record real ones). The red runs are `relay-abi`'s layout test and the kernel's tests, which cannot compile against the old struct, and `boot`, which expects `ABI 3`. Mutation checks: dropping either new refusal fails the dispatcher's test.

**Files:**
- Modify: `crates/relay-abi/src/lib.rs`
- Modify: `crates/relay-abi/src/spawn.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/error_screen.rs`
- Modify: `kernel/src/syscall.rs`
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
- Consumes: plan 4a's `SpawnArgs`, `FOREGROUND`.
- Produces: `SpawnArgs::{pgid, reserved}` (`SpawnArgs::SIZE` 128), `relay_abi::VERSION` 3.

- [ ] **Step 1: Add the failing tests to `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 120);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
````

with:

````rust
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 128);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
````

Replace:

````rust
        assert_eq!(offset_of!(SpawnArgs, flags), 116);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
````

with:

````rust
        assert_eq!(offset_of!(SpawnArgs, flags), 116);
        assert_eq!(offset_of!(SpawnArgs, pgid), 120);
        assert_eq!(offset_of!(SpawnArgs, reserved), 124);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
````

Replace:

````rust
            flags: NEW_GROUP,
        };
````

with:

````rust
            flags: NEW_GROUP,
            pgid: 44,
            reserved: 55,
        };
````

- [ ] **Step 2: Add the failing tests to `kernel/src/error_screen.rs`**

In `kernel/src/error_screen.rs`, replace:

````rust
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 2"
        );
````

with:

````rust
            Reason::System(SystemError::Abi(99)).to_string(),
            "system: ABI 99, kernel wants 3"
        );
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            flags: NEW_GROUP,
        };
````

with:

````rust
            flags: NEW_GROUP,
            pgid: 0,
            reserved: 0,
        };
````

Replace:

````rust
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(
````

with:

````rust
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(refused(&mut f, b"x\0", |a| a.pgid = 2), Err(errno::EINVAL));
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.reserved = 1),
            Err(errno::EINVAL),
            "reserved"
        );
        assert_eq!(
````

- [ ] **Step 4: Add the failing tests to `kernel/src/system.rs`**

In `kernel/src/system.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 2");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
````

with:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 3");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
````

Replace:

````rust
            mount_into(&mut one, archive(relay_abi::VERSION, &["t-args"])).unwrap(),
            "1 program, ABI 2"
        );
````

with:

````rust
            mount_into(&mut one, archive(relay_abi::VERSION, &["t-args"])).unwrap(),
            "1 program, ABI 3"
        );
````

Replace:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 2");
        assert_eq!(
````

with:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 3");
        assert_eq!(
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 2
expect \nWelcome to Relay OS\.\n
````

with:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 3
expect \nWelcome to Relay OS\.\n
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/stale_program.txt`**

In `tests/e2e/stale_program.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# A program built for another ABI (user-space gate §5.2, §8.5, §12.3):
# t-abi is t-args whose note says ABI 1, as a program an older build left
# on the disk would. spawn refuses it with ENOEXEC, from /bin or copied to
````

with:

````text
# A program built for another ABI (user-space gate §5.2, §8.5, §12.3):
# t-abi is t-args whose note says ABI 2, as a program an older build left
# on the disk would. spawn refuses it with ENOEXEC, from /bin or copied to
````

Replace:

````text
send dmesg
expect \nspawn /root/t-abi: built for ABI 1\n
send t-args still
````

with:

````text
send dmesg
expect \nspawn /root/t-abi: built for ABI 2\n
send t-args still
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, replace:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 2
expect root@relay:~# $
````

with:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 3
expect root@relay:~# $
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/system_abi.txt`**

In `tests/e2e/system_abi.txt`, replace:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 2
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: ABI 99, kernel wants 2\n
expect \nPress any key to reboot\.\n
````

with:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 3
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\nsystem: ABI 99, kernel wants 3\n
expect \nPress any key to reboot\.\n
````

- [ ] **Step 9: Expect the new lines in `tests/e2e/system_nosh.txt`**

In `tests/e2e/system_nosh.txt`, replace:

````text
timeout 30
expect \[ ok \] system: \d+ programs, ABI 2\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh cannot start: No such file or directory\n\n--- last kernel log lines ---\n
expect \[ ok \] system: \d+ programs, ABI 2\n
expect \nPress any key to reboot\.\n
````

with:

````text
timeout 30
expect \[ ok \] system: \d+ programs, ABI 3\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh cannot start: No such file or directory\n\n--- last kernel log lines ---\n
expect \[ ok \] system: \d+ programs, ABI 3\n
expect \nPress any key to reboot\.\n
````

- [ ] **Step 10: Expect the new lines in `tests/e2e/utils.txt`**

In `tests/e2e/utils.txt`, replace:

````text
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 2\n
send /bin/sync
````

with:

````text
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 3\n
send /bin/sync
````

- [ ] **Step 11: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
            .collect();
        assert_eq!(differ.len(), 1, "one byte of the version: 2 -> 1");
        assert_eq!(STALE_ABI, 1);
        let e = check_program(path("t-abi")).unwrap_err().to_string();
        assert!(e.contains("built for ABI 1, this is ABI 2"), "{e}");
        assert_eq!(
            kernel_check(path("t-abi")).unwrap_err().to_string(),
            "the kernel's check: built for ABI 1"
        );
````

with:

````rust
            .collect();
        assert_eq!(differ.len(), 1, "one byte of the version: 3 -> 2");
        assert_eq!(STALE_ABI, 2);
        let e = check_program(path("t-abi")).unwrap_err().to_string();
        assert!(e.contains("built for ABI 2, this is ABI 3"), "{e}");
        assert_eq!(
            kernel_check(path("t-abi")).unwrap_err().to_string(),
            "the kernel's check: built for ABI 2"
        );
````

- [ ] **Step 12: Run the tests to see them fail**

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` no field `pgid` on type `spawn::SpawnArgs` ``; `` no field `reserved` on type `spawn::SpawnArgs` ``.

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` struct `relay_abi::SpawnArgs` has no field named `pgid` ``; `` struct `relay_abi::SpawnArgs` has no field named `reserved` ``.

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: FAIL: scenario `boot` stops at line 28, timed out waiting for `\[ ok \] system: \d+ programs?, ABI 3`.

- [ ] **Step 13: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
/// header and into every program's ELF note, and checked by the kernel.
pub const VERSION: u32 = 2;

````

with:

````rust
/// header and into every program's ELF note, and checked by the kernel.
/// 3 since `SpawnArgs` gained `pgid` (milestone 3, spec §16 item 8).
pub const VERSION: u32 = 3;

````

- [ ] **Step 14: Change `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! What `spawn` and `wait` take (spec §7.3): the new program's path,
//! arguments, working directory and file descriptors, and `wait`'s flags.

````

with:

````rust
//! What `spawn` and `wait` take (spec §7.3): the new program's path,
//! arguments, working directory, file descriptors and process group, and
//! `wait`'s flags.

````

Replace:

````rust
    pub flags: u32,
}
````

with:

````rust
    pub flags: u32,
    /// Without [`NEW_GROUP`], the process group the child joins instead of
    /// its parent's (a pipeline's later stages join the first one's, spec
    /// §9.1); 0 for the parent's.
    pub pgid: u32,
    /// 0.
    pub reserved: u32,
}
````

Replace:

````rust
            flags: u32_at(116),
        }
````

with:

````rust
            flags: u32_at(116),
            pgid: u32_at(120),
            reserved: u32_at(124),
        }
````

- [ ] **Step 15: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 34 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 34 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 16: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
    let foreground = a.flags & FOREGROUND != 0;
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && a.flags & NEW_GROUP == 0)
        || a.fd_count as usize > SPAWN_FDS
    {
````

with:

````rust
    let foreground = a.flags & FOREGROUND != 0;
    // A group to join comes with milestone 3's pipelines.
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && a.flags & NEW_GROUP == 0)
        || a.fd_count as usize > SPAWN_FDS
        || a.pgid != 0
        || a.reserved != 0
    {
````

- [ ] **Step 17: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 18: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 2
#> ...
````

with:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 3
#> ...
````

- [ ] **Step 19: Change `xtask/fixtures/checks/check3-a.nuc.log` (it holds escape bytes)**

Run:

````bash
sed -i 's/ABI 2/ABI 3/g' xtask/fixtures/checks/check3-a.nuc.log
````

- [ ] **Step 20: Change `xtask/fixtures/checks/check3-a.qemu.log` (it holds escape bytes)**

Run:

````bash
sed -i 's/ABI 2/ABI 3/g' xtask/fixtures/checks/check3-a.qemu.log
````

- [ ] **Step 21: Change `xtask/fixtures/checks/check3-b.nuc.log` (it holds escape bytes)**

Run:

````bash
sed -i 's/ABI 2/ABI 3/g' xtask/fixtures/checks/check3-b.nuc.log
````

- [ ] **Step 22: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 23 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 356 tests.

Run: `cargo test -p xtask userland`

Expected: PASS: 8 tests.

Run: `cargo test -p xtask checks`

Expected: PASS: 12 tests.

- [ ] **Step 23: Run the `boot`, `system`, `utils`, `system_nosh`, `system_abi`, `stale_program`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_nosh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario stale_program`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 24: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 25: Commit**

````bash
git add crates docs kernel rootfs tests xtask
git commit -F - <<'EOF'
feat(relay-abi,kernel)!: give SpawnArgs a group to join (ABI 3)

A pipeline's later stages must join the first stage's process group,
which spawn could not express. SpawnArgs gains pgid and a reserved
word (128 bytes); the kernel refuses both until pgid gets its meaning.
The changed layout makes relay_abi::VERSION 3: the startup line, t-abi
(built for ABI 2), the scenarios, check3-a.sh and the recorded
transcripts follow.

BREAKING CHANGE: programs built for ABI 2 must be rebuilt; the kernel
refuses their ABI note and a system.img of ABI 2.
EOF
````


### Task 2: A child joins the group of another child of its parent's

Decision 3: without `NEW_GROUP`, `SpawnArgs::pgid` names the group the child joins. It must be the group of one of the caller's children, an ended one not yet collected included (`EPERM` otherwise, Linux's `setpgid` answer), so that a pipeline's later stage can join a first stage that has ended already (`true | cat`); with `NEW_GROUP` it is `EINVAL`. The table's `insert` takes a `Group` (`Parent`, `New`, `Join`), `proc::spawn` checks the group before it reads the program and again under the lock where it inserts, giving back what it took if the second check fails, and `relay-rt`'s `sys::spawn` takes the group (every caller passes 0 but `t-spawn join`). `t-spawn join` starts a dozing child, a second in its group, refusals for group 1 and for a new group with another, kills the group with `kill`, and joins the group of a child that has ended alone in it; `spawn` runs it. `relay-rt`'s `sys.rs` and `sysio.rs` come before the red run, since the test programs call the new `sys::spawn`. The red runs are the table's and the dispatcher's tests, which cannot find `Group`, and `spawn`, where Task 1's kernel refuses the `pgid`. Mutation checks: `may_join` without the parent, the join check off, the joined group replaced by the child's own, and `pgid` with `NEW_GROUP` allowed each fail a test; ruling out zombies survived at first, because a live member was still in the group, so the test makes the ended first stage alone in it.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `kernel/src/init.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `tests/e2e/spawn.txt`
- Modify: `userland/tests/src/bin/t-files.rs`
- Modify: `userland/tests/src/bin/t-mem.rs`
- Modify: `userland/tests/src/bin/t-read.rs`
- Modify: `userland/tests/src/bin/t-spawn.rs`
- Modify: `userland/tests/src/bin/t-tee.rs`

**Interfaces:**
- Consumes: Task 1's `SpawnArgs::pgid`; plan 3a's `Table::insert`.
- Produces: `table::Group::{Parent, New, Join(u32)}`, `Table::may_join(&self, parent: u32, pgid: u32) -> bool`, `Table::insert(&mut self, ppid: u32, group: Group, name: String, res: R)`; `syscall::Spawn::group`; `relay_rt::sys::spawn(path, args, cwd, fds, flags, pgid: u32)`.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// `flags` it starts a process group of its own, which `FOREGROUND` also
/// gives the console, in line mode. Its pid.
pub fn spawn(path: &[u8], args: &[u8], cwd: &[u8], fds: &[FdMap], flags: u32) -> Result<u32, u16> {
    if fds.len() > SPAWN_FDS {
````

with:

````rust
/// `flags` it starts a process group of its own, which `FOREGROUND` also
/// gives the console, in line mode; without it, a `pgid` other than 0 is
/// the group of another child of this program's that it joins. Its pid.
pub fn spawn(
    path: &[u8],
    args: &[u8],
    cwd: &[u8],
    fds: &[FdMap],
    flags: u32,
    pgid: u32,
) -> Result<u32, u16> {
    if fds.len() > SPAWN_FDS {
````

Replace:

````rust
        flags,
        ..SpawnArgs::default()
````

with:

````rust
        flags,
        pgid,
        ..SpawnArgs::default()
````

- [ ] **Step 2: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        };
        sys::spawn(path, &arg_bytes(args), b"", &command_fds(stdout), flags)
            .map_err(Errno::from_number)
````

with:

````rust
        };
        sys::spawn(path, &arg_bytes(args), b"", &command_fds(stdout), flags, 0)
            .map_err(Errno::from_number)
````

- [ ] **Step 3: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    fn add(t: &mut Table<()>, ppid: u32, new_group: bool) -> u32 {
        t.insert(ppid, new_group, String::from("p"), ()).unwrap()
    }
````

with:

````rust
    fn add(t: &mut Table<()>, ppid: u32, new_group: bool) -> u32 {
        let group = if new_group { Group::New } else { Group::Parent };
        t.insert(ppid, group, String::from("p"), ()).unwrap()
    }
````

Replace:

````rust
        assert_eq!(
            t.insert(0, true, String::from("x"), ()),
            Err(Errno::EAGAIN),
````

with:

````rust
        assert_eq!(
            t.insert(0, Group::New, String::from("x"), ()),
            Err(Errno::EAGAIN),
````

Replace:

````rust
        assert!(t.reap(0, Want::Pid(u32::MAX)).unwrap().is_some());
        assert_eq!(t.insert(0, true, String::from("x"), ()), Err(Errno::EAGAIN));
    }
````

with:

````rust
        assert!(t.reap(0, Want::Pid(u32::MAX)).unwrap().is_some());
        assert_eq!(
            t.insert(0, Group::New, String::from("x"), ()),
            Err(Errno::EAGAIN)
        );
    }
````

Replace:

````rust
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN)
        );
        t.end(64, WaitStatus::exited(0));
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN),
````

with:

````rust
        assert_eq!(
            t.insert(1, Group::Parent, String::from("x"), ()),
            Err(Errno::EAGAIN)
        );
        t.end(64, WaitStatus::exited(0));
        assert_eq!(
            t.insert(1, Group::Parent, String::from("x"), ()),
            Err(Errno::EAGAIN),
````

Replace:

````rust
        assert_eq!(t.get(child).unwrap().ppid, cmd);
    }
````

with:

````rust
        assert_eq!(t.get(child).unwrap().ppid, cmd);
    }

    #[test]
    fn a_child_may_join_the_group_of_another_child_of_its_parent() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let shell = add(&mut t, init, true);
        let first = add(&mut t, shell, true);
        let join = |t: &mut Table<()>, ppid, pgid| {
            t.insert(ppid, Group::Join(pgid), String::from("p"), ())
        };
        // An ended child that was not collected still counts, alone in its
        // group: `true | cat`, whose `true` may end before `cat` starts.
        t.end(first, WaitStatus::exited(0));
        let second = join(&mut t, shell, first).unwrap();
        assert_eq!(t.get(second).unwrap().pgid, first);
        assert_eq!(t.get(second).unwrap().ppid, shell);
        let third = join(&mut t, shell, first).unwrap();
        assert_eq!(t.get(third).unwrap().pgid, first);
        // Not a group of its parent's children: its own, its parent's,
        // another process's, one nobody is in, one of a grandchild.
        let len = t.len();
        let grandchild = add(&mut t, second, true);
        for pgid in [shell, init, grandchild, 999] {
            assert_eq!(join(&mut t, shell, pgid), Err(Errno::EPERM), "{pgid}");
        }
        assert_eq!(t.len(), len + 1, "nothing added");
        // Once collected, the group is gone with it.
        t.end(second, WaitStatus::exited(0));
        t.end(third, WaitStatus::exited(0));
        for pid in [first, second, third] {
            assert!(t.reap(shell, Want::Pid(pid)).unwrap().is_some());
        }
        assert_eq!(join(&mut t, shell, first), Err(Errno::EPERM));
    }
````

- [ ] **Step 4: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
                ],
                new_group: true,
                foreground: false,
````

with:

````rust
                ],
                group: Group::New,
                foreground: false,
````

Replace:

````rust
        let s = &f.spawned[1];
        assert!(!s.new_group && s.cwd.is_empty() && s.fds.is_empty());
        assert_eq!(s.argc, 1);
        // A group of its own that gets the console.
        let a = spawn_args(&mut f, b"x\0", |a| a.flags = NEW_GROUP | FOREGROUND);
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(103));
        assert!(f.spawned[2].new_group && f.spawned[2].foreground);
        // The caller's refusal.
````

with:

````rust
        let s = &f.spawned[1];
        assert!(s.group == Group::Parent && s.cwd.is_empty() && s.fds.is_empty());
        assert_eq!(s.argc, 1);
        // A group of its own that gets the console.
        let a = spawn_args(&mut f, b"x\0", |a| a.flags = NEW_GROUP | FOREGROUND);
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(103));
        assert!(f.spawned[2].group == Group::New && f.spawned[2].foreground);
        // Another child's group (whether it may is the table's to say).
        let a = spawn_args(&mut f, b"x\0", |a| {
            a.flags = 0;
            a.pgid = 102;
        });
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(104));
        assert_eq!(f.spawned[3].group, Group::Join(102));
        // The caller's refusal.
````

Replace:

````rust
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(refused(&mut f, b"x\0", |a| a.pgid = 2), Err(errno::EINVAL));
        assert_eq!(
````

with:

````rust
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.pgid = 2),
            Err(errno::EINVAL),
            "a new group and another child's"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| {
                a.flags = FOREGROUND;
                a.pgid = 2;
            }),
            Err(errno::EINVAL),
            "the console goes to a new group only"
        );
        assert_eq!(
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# its parent sleeps, process 1 cannot be killed, and an orphan passes to
# process 1, which collects it.
timeout 60
````

with:

````text
# its parent sleeps, process 1 cannot be killed, and an orphan passes to
# process 1, which collects it. A child may join the group of another
# child, ended or not, as a pipeline's stages do (milestone 3).
timeout 60
````

Replace:

````text
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
# A program killed before it has run runs none of its code: the kernel log
````

with:

````text
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
send t-spawn join
expect \njoined a dozing child's group\ngroup 1: EPERM\na new group and another: EINVAL\n
expect \nfirst: kill\nsecond: kill\njoined an ended child's group\nonce collected: EPERM\n
# A program killed before it has run runs none of its code: the kernel log
````

- [ ] **Step 6: Change the test program `userland/tests/src/bin/t-files.rs`**

In `userland/tests/src/bin/t-files.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0child\0", b"", &fds, 0)?;
    sys::wait(i64::from(pid), false)?;
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0child\0", b"", &fds, 0, 0)?;
    sys::wait(i64::from(pid), false)?;
````

Replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0pwd\0", cwd, &fds, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0pwd\0", cwd, &fds, 0, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
````

Replace:

````rust
        &fds,
        0,
````

with:

````rust
        &fds,
        0,
        0,
````

- [ ] **Step 7: Change the test program `userland/tests/src/bin/t-mem.rs`**

In `userland/tests/src/bin/t-mem.rs`, replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-mem", b"t-mem\0hog\0", b"", &fds, 0)?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-mem", b"t-mem\0hog\0", b"", &fds, 0, 0)?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
````

- [ ] **Step 8: Change the test program `userland/tests/src/bin/t-read.rs`**

In `userland/tests/src/bin/t-read.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            NEW_GROUP,
        )
````

with:

````rust
            NEW_GROUP,
            0,
        )
````

Replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-read", b"t-read\0", b"", &fds, NEW_GROUP)?;
    sys::wait(i64::from(pid), false).map(|_| ())
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-read", b"t-read\0", b"", &fds, NEW_GROUP, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
````

- [ ] **Step 9: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//!   to process 1;
//! - `t-spawn sleepers` starts three children that sleep for a minute in
````

with:

````rust
//!   to process 1;
//! - `t-spawn join` starts children in the group of another child, which
//!   a pipeline's stages do (spec §9.1): one that dozes, then one that
//!   exited and was not collected yet; `kill` of the group ends both
//!   dozers; joining process 1's group, or a new group and another at
//!   once, is refused;
//! - `t-spawn sleepers` starts three children that sleep for a minute in
````

Replace:

````rust
        Some(b"sleepers") => sleepers(),
        Some(b"kill") => kill(),
        Some(b"kill-new") => kill_new(),
        Some(b"orphan") => sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0).map(|_| ()),
        Some(n) => match parse(n) {
````

with:

````rust
        Some(b"sleepers") => sleepers(),
        Some(b"join") => join(),
        Some(b"kill") => kill(),
        Some(b"kill-new") => kill_new(),
        Some(b"orphan") => {
            sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0, 0).map(|_| ())
        }
        Some(n) => match parse(n) {
````

Replace:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan|fill|sleepers\n");
    2
````

with:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(
        2,
        b"usage: t-spawn N|kill|kill-new|orphan|fill|sleepers|join\n",
    );
    2
````

Replace:

````rust
fn child() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0)?;
    match sys::wait(i64::from(pid), false)? {
````

with:

````rust
fn child() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, 0)?;
    match sys::wait(i64::from(pid), false)? {
````

Replace:

````rust
fn kill() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, 0)?;
    // It spins, and this one sleeps: the tick wakes it all the same.
````

with:

````rust
fn kill() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, 0, 0)?;
    // It spins, and this one sleeps: the tick wakes it all the same.
````

Replace:

````rust
    for _ in 0..3 {
        sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], 0)?;
    }
    let apart = relay_abi::spawn::NEW_GROUP;
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], apart)?;
    sys::wait(i64::from(pid), false)?;
    let _ = sys::write_all(1, b"t-spawn: the sleeper woke\n");
    Ok(())
````

with:

````rust
    for _ in 0..3 {
        sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], 0, 0)?;
    }
    let apart = relay_abi::spawn::NEW_GROUP;
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], apart, 0)?;
    sys::wait(i64::from(pid), false)?;
    let _ = sys::write_all(1, b"t-spawn: the sleeper woke\n");
    Ok(())
}

/// Children in another child's group, and the groups no child may join.
fn join() -> Result<(), u16> {
    use relay_abi::spawn::NEW_GROUP;
    let name = |e: u16| relay_abi::errno::name(e).unwrap_or("?");
    let doze = |flags, pgid| sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], flags, pgid);
    let first = doze(NEW_GROUP, 0)?;
    let second = doze(0, first)?;
    let _ = writeln!(Fd(1), "joined a dozing child's group");
    for (what, flags, pgid) in [
        ("group 1", 0, 1),
        ("a new group and another", NEW_GROUP, first),
    ] {
        let said = doze(flags, pgid).map_or_else(name, |_| "started");
        let _ = writeln!(Fd(1), "{what}: {said}");
    }
    sys::kill(-i64::from(first))?;
    for pid in [first, second] {
        if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
            let _ = writeln!(
                Fd(1),
                "{}: {w}",
                if pid == first { "first" } else { "second" }
            );
        }
    }
    // A child that has ended, and was not collected, still has its group.
    let ended = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], NEW_GROUP, 0)?;
    sys::sleep(200);
    let late = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, ended)?;
    let _ = writeln!(Fd(1), "joined an ended child's group");
    for pid in [ended, late] {
        sys::wait(i64::from(pid), false)?;
    }
    let said = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0, ended)
        .map_or_else(name, |_| "started");
    let _ = writeln!(Fd(1), "once collected: {said}");
    Ok(())
````

Replace:

````rust
    loop {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0) {
            Ok(_) => n += 1,
````

with:

````rust
    loop {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0, 0) {
            Ok(_) => n += 1,
````

Replace:

````rust
fn kill_new() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0SHOULD-NOT-PRINT\0", b"", &STD, 0)?;
    sys::kill(i64::from(pid))?;
````

with:

````rust
fn kill_new() -> Result<(), u16> {
    let pid = sys::spawn(
        b"/bin/t-args",
        b"t-args\0SHOULD-NOT-PRINT\0",
        b"",
        &STD,
        0,
        0,
    )?;
    sys::kill(i64::from(pid))?;
````

- [ ] **Step 10: Change the test program `userland/tests/src/bin/t-tee.rs`**

In `userland/tests/src/bin/t-tee.rs`, replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0from a child\0", b"", &fds, 0)?;
    sys::wait(i64::from(pid), false)?;
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0from a child\0", b"", &fds, 0, 0)?;
    sys::wait(i64::from(pid), false)?;
````

- [ ] **Step 11: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Group` in this scope ``; `` struct `syscall::Spawn` has no field named `group` ``.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: FAIL: scenario `spawn` stops at line 23, timed out waiting for `\njoined a dozing child's group\ngroup 1: EPERM\na new group and another: EINVAL\n`.

- [ ] **Step 12: Change `kernel/src/init.rs`**

In `kernel/src/init.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
````

with:

````rust
use crate::mounts::KernelVfs;
use crate::proc::table::Group;
use crate::syscall::Spawn;
````

Replace:

````rust
        fds: fds.to_vec(),
        new_group: true,
        foreground: true,
````

with:

````rust
        fds: fds.to_vec(),
        group: Group::New,
        foreground: true,
````

- [ ] **Step 13: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use spin::Mutex;
use table::{Blocked, Table, Want};
use vfs::{Cwd, Errno, FileType, Vfs};
````

with:

````rust
use spin::Mutex;
use table::{Blocked, Group, Table, Want};
use vfs::{Cwd, Errno, FileType, Vfs};
````

Replace:

````rust
        .lock()
        .insert(0, true, String::from("init"), res)
        .unwrap_or_else(|_| unreachable!("the table is empty"));
````

with:

````rust
        .lock()
        .insert(0, Group::New, String::from("init"), res)
        .unwrap_or_else(|_| unreachable!("the table is empty"));
````

Replace:

````rust
/// and group. `EAGAIN` when the table is full or every pid has been
/// used; the fds and the working directory are checked before the program
/// is read.
pub fn spawn(s: &Spawn) -> Result<u32, Errno> {
````

with:

````rust
/// and group. `EAGAIN` when the table is full or every pid has been
/// used, `EPERM` for a group it may not join; those, the fds and the
/// working directory are checked before the program is read.
pub fn spawn(s: &Spawn) -> Result<u32, Errno> {
````

Replace:

````rust
            return Err(Errno::EAGAIN);
        }
````

with:

````rust
            return Err(Errno::EAGAIN);
        }
        if let Group::Join(g) = s.group
            && !t.may_join(t.current(), g)
        {
            return Err(Errno::EPERM);
        }
````

Replace:

````rust
    let mut t = PROCS.lock();
    if !t.has_room() {
        // Checked above, and nothing else ran since; but a refusal here
````

with:

````rust
    let mut t = PROCS.lock();
    let me = t.current();
    let joinable = match s.group {
        Group::Join(g) => t.may_join(me, g),
        _ => true,
    };
    if !t.has_room() || !joinable {
        // Checked above, and nothing else ran since; but a refusal here
````

Replace:

````rust
        mm::with_user_memory(|mem, _| space.destroy(mem));
        return Err(Errno::EAGAIN);
    }
````

with:

````rust
        mm::with_user_memory(|mem, _| space.destroy(mem));
        return Err(if joinable {
            Errno::EAGAIN
        } else {
            Errno::EPERM
        });
    }
````

Replace:

````rust
    };
    let me = t.current();
    let pid = t
        .insert(me, s.new_group, name.into_owned(), res)
        .unwrap_or_else(|e| unreachable!("has_room was checked under this lock: {e}"));
    drop(t);
````

with:

````rust
    };
    let pid = t
        .insert(me, s.group, name.into_owned(), res)
        .unwrap_or_else(|e| unreachable!("room and group were checked under this lock: {e}"));
    drop(t);
````

- [ ] **Step 14: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   its children pass to process 1.
//! - Round-robin: a process made ready joins the end of the queue, and one
````

with:

````rust
//!   its children pass to process 1.
//! - A new process joins its parent's group, starts one of its own, or
//!   joins the group of another child of its parent (a pipeline's later
//!   stages join the first one's).
//! - Round-robin: a process made ready joins the end of the queue, and one
````

Replace:

````rust
pub const INIT: u32 = 1;
/// Timer ticks in a time slice (spec §6.1).
````

with:

````rust
pub const INIT: u32 = 1;
/// The process group a new process goes into (spec §6.4, §16 item 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Its parent's.
    Parent,
    /// A new one, numbered with its own pid.
    New,
    /// This one, which must be the group of another child of its parent
    /// (a zombie not yet collected counts).
    Join(u32),
}

/// Timer ticks in a time slice (spec §6.1).
````

Replace:

````rust

    /// Adds a ready process, a child of `ppid`, in its parent's group or,
    /// with `new_group` (or no parent), in a new group numbered with its
    /// own pid; its pid. `EAGAIN` when [`MAX`] processes exist, or every
    /// pid has been used.
    pub fn insert(
        &mut self,
        ppid: u32,
        new_group: bool,
        name: String,
        res: R,
    ) -> Result<u32, Errno> {
        if !self.has_room() {
            return Err(Errno::EAGAIN);
        }
        let pid = self.next_pid;
        self.next_pid = pid.checked_add(1).unwrap_or(0);
        let pgid = match self.get(ppid) {
            Some(parent) if !new_group => parent.pgid,
            _ => pid,
````

with:

````rust

    /// Whether `pgid` is the group of a child of `parent`'s, ended or not:
    /// the only groups a new child of `parent` may join.
    pub fn may_join(&self, parent: u32, pgid: u32) -> bool {
        self.procs
            .iter()
            .any(|p| p.ppid == parent && p.pgid == pgid)
    }

    /// Adds a ready process, a child of `ppid`, in the group `group` says
    /// (a new one, numbered with its own pid, when it has no parent); its
    /// pid. `EAGAIN` when [`MAX`] processes exist, or every pid has been
    /// used; `EPERM` for a group it may not join.
    pub fn insert(&mut self, ppid: u32, group: Group, name: String, res: R) -> Result<u32, Errno> {
        if !self.has_room() {
            return Err(Errno::EAGAIN);
        }
        if let Group::Join(g) = group
            && !self.may_join(ppid, g)
        {
            return Err(Errno::EPERM);
        }
        let pid = self.next_pid;
        self.next_pid = pid.checked_add(1).unwrap_or(0);
        let pgid = match (group, self.get(ppid)) {
            (Group::Join(g), _) => g,
            (Group::Parent, Some(parent)) => parent.pgid,
            _ => pid,
````

- [ ] **Step 15: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::mm::user::{UserSlice, UserStr};
use alloc::sync::Arc;
````

with:

````rust
use crate::mm::user::{UserSlice, UserStr};
use crate::proc::table::Group;
use alloc::sync::Arc;
````

Replace:

````rust
    pub fds: Vec<FdMap>,
    pub new_group: bool,
    /// The new group gets the console (`FOREGROUND`).
````

with:

````rust
    pub fds: Vec<FdMap>,
    /// Its parent's group, a new one, or another child's (`SpawnArgs::pgid`).
    pub group: Group,
    /// The new group gets the console (`FOREGROUND`).
````

Replace:

````rust
    let foreground = a.flags & FOREGROUND != 0;
    // A group to join comes with milestone 3's pipelines.
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && a.flags & NEW_GROUP == 0)
        || a.fd_count as usize > SPAWN_FDS
        || a.pgid != 0
        || a.reserved != 0
````

with:

````rust
    let foreground = a.flags & FOREGROUND != 0;
    let new_group = a.flags & NEW_GROUP != 0;
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && !new_group)
        || (new_group && a.pgid != 0)
        || a.fd_count as usize > SPAWN_FDS
        || a.reserved != 0
````

Replace:

````rust
        fds: a.fds[..a.fd_count as usize].to_vec(),
        new_group: a.flags & NEW_GROUP != 0,
        foreground,
````

with:

````rust
        fds: a.fds[..a.fd_count as usize].to_vec(),
        group: match a.pgid {
            _ if new_group => Group::New,
            0 => Group::Parent,
            g => Group::Join(g),
        },
        foreground,
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 357 tests.

- [ ] **Step 17: Run the `spawn`, `files`, `memory`, `console`, `tees` scenarios**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 18: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 19: Commit**

````bash
git add crates kernel tests userland
git commit -F - <<'EOF'
feat(kernel,relay-rt): let a child join another child's process group

A pipeline's stages must share one group, so that Ctrl-C and kill reach
them all. With SpawnArgs::pgid a child joins the group of another child
of its parent's, ended or not (true | cat); any other group is EPERM,
as Linux's setpgid says, and pgid with NEW_GROUP is EINVAL. relay-rt's
sys::spawn takes the group, and t-spawn join shows it in QEMU.
EOF
````


### Task 3: The pipe's ring and its two ends

Spec §9.1 and decision 2: a pipe is a ring of 16 KiB between the processes that have its read end and those that have its write end (`kernel/src/pipe.rs`). Its calls never wait: a read gets what the ring holds, up to its length, or `None` while it is empty and a write end is open (the caller waits), and 0 once every write end has closed; a write takes what fits, or `None` while the ring is full, and `EPIPE` once every read end has closed. An `End` closes when the last fd that has it closes (it is dropped), and wakes the pipe's waiters through the function the pipe was made with: the kernel's will be `proc::wake_pipe` (Task 4); host tests pass one that records, since the process table is a static that other tests lock in parallel threads. The ring lives in `Memory` of its own: four frames in the kernel (Task 4), a buffer in the tests. `vfs::Errno` gains `EPIPE` ("Broken pipe", checked against the host C library like the others). The red runs are the kernel's and `vfs`'s tests, which cannot find the new items. Mutation checks: an empty pipe read as its end, `EPIPE` never given, the wrap-around cut wrong, the start not wrapped, a full ring taking nothing as if written, and no wake on close each fail a test.

**Files:**
- Modify: `crates/vfs/src/errno.rs`
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/pipe.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pipe::SIZE` (16 KiB), `pipe::Memory` (`fn bytes(&mut self) -> &mut [u8; SIZE]`), `pipe::Pipe::{id, read(&self, &mut [u8]) -> Option<usize>, write(&self, &[u8]) -> Result<Option<usize>, Errno>}`, `pipe::End::{pipe, side}`, `pipe::Side::{Read, Write}`, `pipe::new(memory: Box<dyn Memory>, wake: fn(u64)) -> (End, End)`; `vfs::Errno::EPIPE`.

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, replace:

````rust
        assert_eq!(Errno::EFBIG.to_string(), "File too large");
    }
````

with:

````rust
        assert_eq!(Errno::EFBIG.to_string(), "File too large");
        assert_eq!(Errno::EPIPE.to_string(), "Broken pipe");
    }
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod pci;
pub mod power;
````

with:

````rust
pub mod pci;
pub mod pipe;
pub mod power;
````

- [ ] **Step 3: Write the failing tests for `kernel/src/pipe.rs`**

Create `kernel/src/pipe.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    struct Buffer(Box<[u8; SIZE]>);

    impl Memory for Buffer {
        fn bytes(&mut self) -> &mut [u8; SIZE] {
            &mut self.0
        }
    }

    std::thread_local! {
        /// The ids `wake` was called with, in this test's thread.
        static WOKEN: core::cell::RefCell<Vec<u64>> = const { core::cell::RefCell::new(Vec::new()) };
    }

    fn wake(id: u64) {
        WOKEN.with(|w| w.borrow_mut().push(id));
    }

    fn woken() -> Vec<u64> {
        WOKEN.with(|w| core::mem::take(&mut *w.borrow_mut()))
    }

    fn pipe() -> (End, End) {
        new(Box::new(Buffer(Box::new([0; SIZE]))), wake)
    }

    #[test]
    fn what_is_written_is_read_in_order_in_any_pieces() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(b"hello, "), Ok(Some(7)));
        assert_eq!(w.pipe().write(b"world"), Ok(Some(5)));
        let mut buf = [0; 4];
        assert_eq!(r.pipe().read(&mut buf), Some(4));
        assert_eq!(&buf, b"hell");
        let mut buf = [0; 100];
        assert_eq!(r.pipe().read(&mut buf), Some(8), "what is there, no more");
        assert_eq!(&buf[..8], b"o, world");
        assert_eq!(r.pipe().read(&mut buf), None, "empty: the reader waits");
        assert_eq!(r.pipe().read(&mut []), None);
        assert_eq!(w.pipe().write(b""), Ok(Some(0)), "nothing to write");
    }

    #[test]
    fn a_full_ring_takes_what_fits_then_makes_the_writer_wait() {
        let (r, w) = pipe();
        let data: Vec<u8> = (0..SIZE + 1000).map(|i| (i % 251) as u8).collect();
        assert_eq!(w.pipe().write(&data[..SIZE - 10]), Ok(Some(SIZE - 10)));
        assert_eq!(
            w.pipe().write(&data[SIZE - 10..]),
            Ok(Some(10)),
            "what fits"
        );
        assert_eq!(w.pipe().write(&data[SIZE..]), Ok(None), "full: it waits");
        // Room again, across the ring's end.
        let mut buf = vec![0; 3000];
        assert_eq!(r.pipe().read(&mut buf), Some(3000));
        assert_eq!(buf, data[..3000]);
        assert_eq!(w.pipe().write(&data[SIZE..]), Ok(Some(1000)));
        let mut got = Vec::new();
        let mut buf = vec![0; 5000];
        // Bounded: a pipe that never empties ends the loop anyway.
        for _ in 0..10 {
            match r.pipe().read(&mut buf) {
                Some(n) if n > 0 => got.extend_from_slice(&buf[..n]),
                _ => break,
            }
        }
        assert_eq!(got, data[3000..], "every byte once, in order");
    }

    #[test]
    fn the_ring_wraps_around_many_times() {
        let (r, w) = pipe();
        let mut next = 0u8;
        let mut want = 0u8;
        let mut buf = vec![0; 7000];
        for round in 0..50 {
            let piece: Vec<u8> = (0..6001)
                .map(|_| {
                    next = next.wrapping_add(1);
                    next
                })
                .collect();
            assert_eq!(w.pipe().write(&piece), Ok(Some(6001)), "{round}");
            assert_eq!(r.pipe().read(&mut buf), Some(6001), "{round}");
            for &b in &buf[..6001] {
                want = want.wrapping_add(1);
                assert_eq!(b, want, "{round}");
            }
        }
    }

    #[test]
    fn closing_the_write_end_is_the_end_of_the_data_after_what_is_left() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(b"last"), Ok(Some(4)));
        drop(w);
        let mut buf = [0; 10];
        assert_eq!(r.pipe().read(&mut buf), Some(4));
        assert_eq!(&buf[..4], b"last");
        assert_eq!(r.pipe().read(&mut buf), Some(0), "the end, not a wait");
        assert_eq!(r.pipe().read(&mut buf), Some(0));
    }

    #[test]
    fn closing_the_read_end_makes_every_write_epipe() {
        let (r, w) = pipe();
        assert_eq!(w.pipe().write(&[7; SIZE]), Ok(Some(SIZE)));
        assert_eq!(w.pipe().write(b"x"), Ok(None));
        drop(r);
        assert_eq!(
            w.pipe().write(b"x"),
            Err(Errno::EPIPE),
            "full, but no reader"
        );
        assert_eq!(w.pipe().write(b""), Err(Errno::EPIPE));
    }

    #[test]
    fn closing_an_end_wakes_the_pipe_s_waiters_once() {
        let (r, w) = pipe();
        let id = r.pipe().id();
        assert_eq!(w.pipe().write(b"x"), Ok(Some(1)));
        assert_eq!(r.pipe().read(&mut [0; 2]), Some(1));
        assert_eq!(woken(), [], "reads and writes do not: their callers do");
        drop(w);
        assert_eq!(woken(), [id]);
        drop(r);
        assert_eq!(woken(), [id]);
    }

    #[test]
    fn ends_know_their_side_and_pipe() {
        let (r, w) = pipe();
        assert_eq!((r.side(), w.side()), (Side::Read, Side::Write));
        assert_eq!(r.pipe().id(), w.pipe().id());
        let (r2, _w2) = pipe();
        assert_ne!(r.pipe().id(), r2.pipe().id());
        assert_ne!(r, r2);
        assert_eq!(r, r);
    }
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `SIZE` in this scope ``; `` cannot find trait `Memory` in this scope ``.

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `EPIPE` found for enum `errno::Errno` in the current scope ``.

- [ ] **Step 5: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    ERANGE,
}

impl Errno {
    /// Every error number.
    pub const ALL: [Errno; 26] = [
        Errno::ENOENT,
````

with:

````rust
    ERANGE,
    /// A write to a pipe nobody reads any more.
    EPIPE,
}

impl Errno {
    /// Every error number.
    pub const ALL: [Errno; 27] = [
        Errno::ENOENT,
````

Replace:

````rust
        Errno::ERANGE,
    ];
````

with:

````rust
        Errno::ERANGE,
        Errno::EPIPE,
    ];
````

Replace:

````rust
            Errno::ERANGE => "Numerical result out of range",
        }
````

with:

````rust
            Errno::ERANGE => "Numerical result out of range",
            Errno::EPIPE => "Broken pipe",
        }
````

Replace:

````rust
            Errno::ERANGE => n::ERANGE,
        }
````

with:

````rust
            Errno::ERANGE => n::ERANGE,
            Errno::EPIPE => n::EPIPE,
        }
````

- [ ] **Step 6: Implement `kernel/src/pipe.rs`**

Insert this at the top of `kernel/src/pipe.rs`, above `#[cfg(test)]`:

````rust
//! Pipes (user-space gate §9.1, §16 item 8): a ring of 16 KiB between the
//! processes that have its read end and those that have its write end.
//!
//! A read gets what the ring holds, up to its length, and must wait only
//! while the ring is empty and a write end is open; once every write end
//! has closed it gets 0, the end of the data. A write takes what fits and
//! must wait only while nothing fits; once every read end has closed it is
//! `EPIPE`. The waiting is the caller's: these calls never block, they say
//! when the caller would have to. An end closes when the last fd that has
//! it closes, and the processes waiting on the pipe are woken, to find
//! the end of the data or `EPIPE`.
//!
//! The ring lives in memory of its own (`Memory`): four frames in the
//! kernel, a buffer in the tests. Waking is the kernel's too: a pipe is
//! made with the function that wakes its waiters (`proc::wake_pipe`).

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use spin::Mutex;
use vfs::Errno;

/// The ring's size.
pub const SIZE: usize = 16 * 1024;

/// Where a pipe's bytes are: [`SIZE`] bytes that are the pipe's alone, for
/// as long as it lives.
pub trait Memory: Send {
    fn bytes(&mut self) -> &mut [u8; SIZE];
}

pub struct Pipe {
    ring: Mutex<Ring>,
    /// Wakes the processes waiting on the pipe whose id it is given.
    wake: fn(u64),
}

struct Ring {
    memory: Box<dyn Memory>,
    /// Where the oldest byte is, and how many there are.
    start: usize,
    len: usize,
    readers: u32,
    writers: u32,
}

impl Pipe {
    /// What the processes waiting on this pipe wait for.
    pub fn id(&self) -> u64 {
        self as *const Pipe as u64
    }

    /// Takes up to `buf.len()` bytes into `buf`: `Some(n)` with `n` bytes,
    /// `Some(0)` at the end of the data (every write end closed), `None`
    /// when the ring is empty and a write end is open (the caller waits).
    pub fn read(&self, buf: &mut [u8]) -> Option<usize> {
        let mut r = self.ring.lock();
        if r.len == 0 {
            return (r.writers == 0).then_some(0);
        }
        let n = buf.len().min(r.len);
        let start = r.start;
        let ring = r.memory.bytes();
        let first = n.min(SIZE - start);
        buf[..first].copy_from_slice(&ring[start..start + first]);
        buf[first..n].copy_from_slice(&ring[..n - first]);
        r.start = (start + n) % SIZE;
        r.len -= n;
        Some(n)
    }

    /// Puts as much of `bytes` as fits: `Ok(Some(n))` with `n` bytes taken
    /// (0 only for an empty `bytes`), `Ok(None)` when the ring is full (the
    /// caller waits), `EPIPE` once every read end has closed.
    pub fn write(&self, bytes: &[u8]) -> Result<Option<usize>, Errno> {
        let mut r = self.ring.lock();
        if r.readers == 0 {
            return Err(Errno::EPIPE);
        }
        let n = bytes.len().min(SIZE - r.len);
        if n == 0 && !bytes.is_empty() {
            return Ok(None);
        }
        let at = (r.start + r.len) % SIZE;
        let ring = r.memory.bytes();
        let first = n.min(SIZE - at);
        ring[at..at + first].copy_from_slice(&bytes[..first]);
        ring[..n - first].copy_from_slice(&bytes[first..n]);
        r.len += n;
        Ok(Some(n))
    }
}

/// Which end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Read,
    Write,
}

/// One end of a pipe, as the fd table holds it (shared by every fd that
/// has it). Dropping it closes it.
pub struct End {
    pipe: Arc<Pipe>,
    side: Side,
}

impl End {
    pub fn pipe(&self) -> &Pipe {
        &self.pipe
    }

    pub fn side(&self) -> Side {
        self.side
    }
}

impl fmt::Debug for End {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "End({:#x}, {:?})", self.pipe.id(), self.side)
    }
}

/// Ends are the same only if they are one.
impl PartialEq for End {
    fn eq(&self, other: &End) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for End {}

impl Drop for End {
    /// The last fd of this end closed: the other side's waiters wake, to
    /// find the end of the data or `EPIPE`.
    fn drop(&mut self) {
        {
            let mut r = self.pipe.ring.lock();
            match self.side {
                Side::Read => r.readers -= 1,
                Side::Write => r.writers -= 1,
            }
        }
        (self.pipe.wake)(self.pipe.id());
    }
}

/// A new pipe in `memory`, whose waiters `wake` wakes: its read end and
/// its write end.
pub fn new(memory: Box<dyn Memory>, wake: fn(u64)) -> (End, End) {
    let pipe = Arc::new(Pipe {
        ring: Mutex::new(Ring {
            memory,
            start: 0,
            len: 0,
            readers: 1,
            writers: 1,
        }),
        wake,
    });
    let read = End {
        pipe: Arc::clone(&pipe),
        side: Side::Read,
    };
    (
        read,
        End {
            pipe,
            side: Side::Write,
        },
    )
}

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 364 tests.

Run: `cargo test -p vfs`

Expected: PASS: 54 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates kernel
git commit -F - <<'EOF'
feat(kernel,vfs): add the pipe's 16 KiB ring and its two ends

A pipe is a ring of 16 KiB in memory of its own, with read and write
ends that count their fds' holders. Its calls never block: they say
when the caller must wait, and closing an end wakes the waiters through
the function the pipe was made with. vfs::Errno gains EPIPE.
EOF
````


### Task 4: The `pipe` call

Spec §7.3, §9.1 and decision 2. `pipe(&mut [u32; 2])` checks the memory for the two fds (`EFAULT`), then that two fds are free (`EMFILE`), then makes the ring in four contiguous frames of user memory (`mm::alloc_pipe_frames`: `ENOMEM` once fewer than 8 MiB of frames would be left; not the kernel's heap, which would hold only about 1000 rings and which `spawn` needs), and gives its read and write ends the two lowest free fds. The fd table holds an end as `File::Pipe`. `kernel/src/syscall/pipes.rs` reads and writes an end: a read takes what the pipe holds, a page at a time, up to the buffer's length (and the ring's), checking the whole buffer writable first so nothing taken is lost, and waits through `Caller::pipe_wait` only while nothing has come; a write puts a page at a time and waits only while nothing of it has gone in, so a full pipe gives fewer bytes than asked; each wakes the other side (`Caller::pipe_wake`). The process's `pipe_wait` returns `EINTR` once the process is killed, and otherwise blocks on `Blocked::Pipe` of the pipe's id; `proc::wake_pipe`, which an end's drop calls, asserts that `PROCS` is not held. `fstat` says FIFO with `dev` 0, `seek` is `EINVAL`, `read_dir` `ENOTDIR`, a pipe is no tee (`EINVAL`, as the console), and reading a write end or writing a read end is `EBADF`. `relay-rt` gains `sys::pipe` (before the red run: `t-pipe` needs it), and `t-pipe` runs every kind in the new `pipe_calls` scenario, with `free` the same before and after; `system` lists `/bin` with `t-pipe` (35 programs). The red runs are the kernel's tests, which cannot compile, and `pipe_calls`, which gets `ENOSYS`. Mutation checks: on the host, a read or a write that waits after taking some, the `EMFILE` check, the memory check first, `EPIPE` with nothing written and the frames' reserve each fail a test; in QEMU (`pipe_calls`), `pipe_wait` without the kill check (the blocked reader never ends), no wake after a read or a write (`child` hangs), a pipe made with a wake that does nothing (`eof` hangs) and frames never given back (`free` differs) each fail the scenario.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/fd.rs`
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/files.rs`
- Create: `kernel/src/syscall/pipes.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/system.rs`
- Modify: `kernel/src/tty.rs`
- Create: `tests/e2e/pipe_calls.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-pipe.rs`

**Interfaces:**
- Consumes: Task 3's `pipe`; plan 3b's fd table and dispatcher.
- Produces: `fd::File::Pipe(pipe::End)`; `Caller::{new_pipe(&mut self) -> Result<(End, End), Errno>, pipe_wait(&mut self, id: u64) -> Result<(), Errno>, pipe_wake(&mut self, id: u64)}`; `table::Blocked::Pipe(u64)`; `proc::wake_pipe(id: u64)`; `mm::{PIPE_FRAMES, pipe_may_take, PipeFrames, alloc_pipe_frames}`; `relay_rt::sys::pipe() -> Result<(u32, u32), u16>`; the test program `t-pipe`.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
    .map(|fd| fd as u32)
}
````

with:

````rust
    .map(|fd| fd as u32)
}

/// Makes a pipe (spec §9.1): its read end and its write end, the two
/// lowest free fds.
pub fn pipe() -> Result<(u32, u32), u16> {
    let mut fds = [0u32; 2];
    call(Call::Pipe, &[&raw mut fds as u64]).map(|_| (fds[0], fds[1]))
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
````

with:

````rust
    #[test]
    fn a_pipe_takes_four_frames_of_user_memory() {
        assert_eq!(PIPE_FRAMES, 4);
        assert!(pipe_may_take(2052), "the four taken leave 2048");
        assert!(!pipe_may_take(2051));
        assert!(!pipe_may_take(0));
    }

    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
````

- [ ] **Step 3: Add the failing tests and the module declaration to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
mod files;
#[cfg(test)]
````

with:

````rust
mod files;
mod pipes;
#[cfg(test)]
````

Replace:

````rust
            Call::Power,
        ];
````

with:

````rust
            Call::Power,
            Call::Pipe,
        ];
````

- [ ] **Step 4: Write the failing tests for `kernel/src/syscall/pipes.rs`**

Create `kernel/src/syscall/pipes.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::*;
    use relay_abi::errno;
    use relay_abi::file::{KIND_FIFO, SEEK_START};

    /// A new pipe's fds.
    fn new_pipe(f: &mut Fake) -> (u64, u64) {
        assert_eq!(call(f, Call::Pipe, [W, 0, 0]), Ok(0));
        let fds = get(f, W, 8);
        let n = |b: &[u8]| u64::from(u32::from_ne_bytes(b.try_into().unwrap()));
        (n(&fds[..4]), n(&fds[4..]))
    }

    #[test]
    fn a_pipe_s_ends_are_the_lowest_free_fds_read_end_first() {
        let mut f = fake();
        assert_eq!(new_pipe(&mut f), (3, 4));
        assert_eq!(call(&mut f, Call::Close, [1, 0, 0]), Ok(0));
        assert_eq!(new_pipe(&mut f), (1, 5));
        // The memory first, then two free fds, then the ring's frames.
        assert_eq!(
            call(&mut f, Call::Pipe, [U, 0, 0]),
            Err(errno::EFAULT),
            "read-only"
        );
        assert_eq!(
            call(&mut f, Call::Pipe, [W + PAGE - 4, 0, 0]),
            Err(errno::EFAULT)
        );
        assert_eq!(f.pipes_made, 2);
        for _ in 6..31 {
            open_any(&mut f);
        }
        assert_eq!(f.fds.open(), 31);
        assert_eq!(
            call(&mut f, Call::Pipe, [W, 0, 0]),
            Err(errno::EMFILE),
            "one left"
        );
        assert_eq!(f.fds.open(), 31, "none taken");
        assert_eq!(f.pipes_made, 2, "no ring made");
        assert_eq!(call(&mut f, Call::Close, [30, 0, 0]), Ok(0));
        f.no_pipe_memory = true;
        assert_eq!(call(&mut f, Call::Pipe, [W, 0, 0]), Err(errno::ENOMEM));
        assert_eq!(f.fds.open(), 30);
    }

    /// Opens `/root/f` as the next fd.
    fn open_any(f: &mut Fake) {
        put(f, W + 512, b"/root/f");
        let flags = u64::from(relay_abi::file::OPEN_READ);
        call(f, Call::Open, [W + 512, 7, flags]).unwrap();
    }

    #[test]
    fn what_goes_in_one_end_comes_out_of_the_other() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // `U` holds the pattern 0, 1, 2, ...
        assert_eq!(call(&mut f, Call::Write, [w, U + 10, 5]), Ok(5));
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 100]),
            Ok(5),
            "what is there"
        );
        assert_eq!(get(&mut f, W, 5), [10, 11, 12, 13, 14]);
        // Across pages, in one call.
        assert_eq!(call(&mut f, Call::Write, [w, U + 100, 10_000]), Ok(10_000));
        assert_eq!(call(&mut f, Call::Read, [r, W, 3000]), Ok(3000));
        assert_eq!(get(&mut f, W, 3), [100, 101, 102]);
        let rest = W + PAGE - 7000;
        assert!(
            call(&mut f, Call::Read, [r, rest, 7000]).is_err(),
            "not all writable"
        );
        assert_eq!(f.waits, [], "nothing waited");
        assert!(!f.woken.is_empty(), "the other side is woken");
    }

    #[test]
    fn an_empty_pipe_waits_and_a_killed_wait_is_eintr() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // The fake's wait ends as a kill would end it.
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Err(errno::EINTR));
        assert_eq!(f.waits.len(), 1);
        // Full: the next write waits (and is killed), having taken nothing.
        assert_eq!(call(&mut f, Call::Write, [w, U, 3 * PAGE]), Ok(3 * PAGE));
        assert_eq!(
            call(&mut f, Call::Write, [w, U, 3 * PAGE]),
            Ok(PAGE),
            "what fits"
        );
        assert_eq!(f.waits.len(), 1, "it took some, so it did not wait");
        assert_eq!(call(&mut f, Call::Write, [w, U, 1]), Err(errno::EINTR));
        assert_eq!(f.waits.len(), 2);
        assert_eq!(f.waits[0], f.waits[1], "the same pipe");
    }

    #[test]
    fn the_last_writer_closing_is_the_end_of_the_data() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Write, [w, U, 3]), Ok(3));
        assert_eq!(call(&mut f, Call::Close, [w, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(3));
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(0), "no wait");
        assert_eq!(f.waits, []);
    }

    #[test]
    fn the_last_reader_closing_makes_writes_epipe() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Close, [r, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Write, [w, U, 3]), Err(errno::EPIPE));
        assert_eq!(f.waits, []);
    }

    #[test]
    fn an_end_shared_by_two_fds_closes_with_the_last() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        // As a child given the write end would hold it.
        let held = Arc::clone(f.fds.get(w).unwrap());
        assert_eq!(call(&mut f, Call::Close, [w, 0, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::Read, [r, W, 10]),
            Err(errno::EINTR),
            "it waits"
        );
        drop(held);
        assert_eq!(call(&mut f, Call::Read, [r, W, 10]), Ok(0));
    }

    #[test]
    fn what_a_pipe_s_end_is_not() {
        let mut f = fake();
        let (r, w) = new_pipe(&mut f);
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(call(&mut f, Call::Write, [r, U, 1]), Err(errno::EBADF));
        for fd in [r, w] {
            assert_eq!(
                call(&mut f, Call::Seek, [fd, 0, u64::from(SEEK_START)]),
                Err(errno::EINVAL)
            );
            assert_eq!(
                call(&mut f, Call::ReadDir, [fd, W, 100]),
                Err(errno::ENOTDIR)
            );
            assert_eq!(call(&mut f, Call::Fstat, [fd, W, 0]), Ok(0));
            let b = get(&mut f, W, relay_abi::Stat::SIZE);
            let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
            let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
            // `kind` at 48, `size` at 8, `dev` at 72.
            assert_eq!(
                (u32_at(48), u64_at(8), u64_at(72)),
                (u32::from(KIND_FIFO), 0, 0)
            );
        }
        assert_eq!(f.waits, []);
    }
}
````

- [ ] **Step 5: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub killed: Vec<i64>,
}
````

with:

````rust
    pub killed: Vec<i64>,
    /// The pipes made, whether there is memory for another, the pipes
    /// waited on (each wait ends as a kill would: `EINTR`) and woken.
    pub pipes_made: u32,
    pub no_pipe_memory: bool,
    pub waits: Vec<u64>,
    pub woken: Vec<u64>,
}

/// A pipe's ring on the heap.
struct Ring(Box<[u8; crate::pipe::SIZE]>);

impl crate::pipe::Memory for Ring {
    fn bytes(&mut self) -> &mut [u8; crate::pipe::SIZE] {
        &mut self.0
    }
}
````

Replace:

````rust
        FAKE_LOG.to_vec()
    }
````

with:

````rust
        FAKE_LOG.to_vec()
    }
    fn new_pipe(&mut self) -> Result<(crate::pipe::End, crate::pipe::End), Errno> {
        if self.no_pipe_memory {
            return Err(Errno::ENOMEM);
        }
        self.pipes_made += 1;
        Ok(crate::pipe::new(
            Box::new(Ring(Box::new([0; crate::pipe::SIZE]))),
            |_| {},
        ))
    }
    fn pipe_wait(&mut self, id: u64) -> Result<(), Errno> {
        self.waits.push(id);
        Err(Errno::EINTR)
    }
    fn pipe_wake(&mut self, id: u64) {
        self.woken.push(id);
    }
````

Replace:

````rust
        killed: Vec::new(),
    }
````

with:

````rust
        killed: Vec::new(),
        pipes_made: 0,
        no_pipe_memory: false,
        waits: Vec::new(),
        woken: Vec::new(),
    }
````

- [ ] **Step 6: Add the failing tests to `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
    }
````

with:

````rust
        assert_eq!(tee_target(&File::Console), Err(Errno::EINVAL));
        struct Ring(alloc::boxed::Box<[u8; crate::pipe::SIZE]>);
        impl crate::pipe::Memory for Ring {
            fn bytes(&mut self) -> &mut [u8; crate::pipe::SIZE] {
                &mut self.0
            }
        }
        let (r, w) = crate::pipe::new(Box::new(Ring(Box::new([0; crate::pipe::SIZE]))), |_| {});
        for end in [r, w] {
            assert_eq!(tee_target(&File::Pipe(end)), Err(Errno::EINVAL));
        }
    }
````

- [ ] **Step 7: Add the scenario `tests/e2e/pipe_calls.txt`**

Create `tests/e2e/pipe_calls.txt`:

````text
# The pipe call from ring 3 (user-space gate §9.1; milestone 3, plan 1):
# t-pipe's kinds, each answer the kernel gave. A read waits for data and
# a write for room, an end closes with the last process that has it, a
# killed reader or writer ends, the fds run out cleanly, and a pipe's
# frames come back: free shows the same memory in use before and after.
timeout 60
expect root@relay:~# $
# Every kernel-stack slot's page tables made first (they stay), so memory
# in use compares after them.
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-pipe basic
expect \npipe: 3 4\nwrite: 5\nread: hello\nfstat: fifo, dev 0\nseek: EINVAL\nread the write end: EBADF\nwrite the read end: EBADF\ntee: EINVAL\nafter the writer closed: 0\n
send t-pipe room
expect \nwrite 20000: 16384\nread back: 16384, all as written: true\n
# 1 MiB through 16 KiB: the writer and the reader block in turn.
send t-pipe child
expect-same sum \ndrained 1048576 bytes, checksum (\d+)\n
expect-same sum sent 1048576 bytes, checksum (\d+)\n
send t-pipe eof
expect \nend of data once the child ended: 0\n
send t-pipe epipe
expect \nwrite with no reader: EPIPE\n
send t-pipe killed
expect \na reader blocked on an empty pipe: kill\n
expect \na writer blocked on a full pipe: kill\n
send dmesg
expect \npid \d+ \(/bin/t-pipe\): killed: kill\npid \d+ \(/bin/t-pipe\): killed: kill\n
send t-pipe many
expect \npipes: 14, then EMFILE\nthe last fd: 31\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem    t-spin  tail   wc\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-read   t-sys   touch\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-spawn  t-tee   uname\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem   t-spawn  t-tee  uname\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-pipe  t-spin   tail   wc\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-read  t-sys    touch\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem    t-spin  tail   wc\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-read   t-sys   touch\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-spawn  t-tee   uname\n
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem   t-spawn  t-tee  uname\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-pipe  t-spin   tail   wc\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-read  t-sys    touch\n
````

- [ ] **Step 9: Add the test program `userland/tests/src/bin/t-pipe.rs`**

Create `userland/tests/src/bin/t-pipe.rs`:

````rust
//! `t-pipe KIND`: the `pipe` call from ring 3 (spec §9.1, §16 item 8),
//! each answer printed as `<what>: <value or error name>`. Every loop that
//! waits for the kernel to say stop is bounded, and says `and no end` when
//! it never does.
//!
//! - `t-pipe basic`: a pipe's fds, a write and a read, `fstat`, and what an
//!   end is not (`seek`, the wrong direction, a tee); the end of the data
//!   once the write end is closed.
//! - `t-pipe room`: a write into an empty pipe takes 16 KiB of 20000 bytes,
//!   and the bytes read back are those.
//! - `t-pipe child`: 1 MiB through a pipe to a child that reads it
//!   (`drain`), both blocking in turn; both print a checksum.
//! - `t-pipe eof`: the write end is held by a child that naps, so the end
//!   of the data comes only when it ends.
//! - `t-pipe epipe`: a write once the read end is closed.
//! - `t-pipe killed`: a child blocked reading an empty pipe, and one
//!   blocked writing a full one, are killed.
//! - `t-pipe many`: pipes until the fds run out (`EMFILE`, no fd taken).
//! - `t-pipe drain` reads fd 0 to its end and prints how much and its
//!   checksum; `hold` naps with fd 3; `flood` writes to fd 3 until it is
//!   stopped (the children of the kinds above).
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::fmt::Write;
use relay_abi::file::{KIND_FIFO, OPEN_DIRECTORY, OPEN_READ, SEEK_START};
use relay_abi::{FdMap, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// How much `child` sends.
const SENT: usize = 1 << 20;

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"room") => room(),
        Some(b"child") => child(),
        Some(b"eof") => eof(),
        Some(b"epipe") => epipe(),
        Some(b"killed") => killed(),
        Some(b"many") => many(),
        Some(b"drain") => drain(),
        Some(b"hold") => {
            sys::sleep(300);
            Ok(())
        }
        Some(b"flood") => flood(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-pipe basic|room|child|eof|epipe|killed|many\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-pipe: {}", name(e));
            1
        }
    }
}

/// An error's name.
fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

/// `<what>: <value>` or `<what>: <error>`.
fn show<T: core::fmt::Display>(what: &str, r: Result<T, u16>) {
    let _ = match r {
        Ok(v) => writeln!(Fd(1), "{what}: {v}"),
        Err(e) => writeln!(Fd(1), "{what}: {}", name(e)),
    };
}

/// Starts this program as `kind` with `fds` (child, parent).
fn start(kind: &[u8], fds: &[(u32, u32)]) -> Result<u32, u16> {
    let mut args = vec![];
    args.extend_from_slice(b"t-pipe\0");
    args.extend_from_slice(kind);
    args.push(0);
    let maps: alloc::vec::Vec<FdMap> = fds
        .iter()
        .map(|&(child, parent)| FdMap { child, parent })
        .collect();
    sys::spawn(b"/bin/t-pipe", &args, b"", &maps, 0, 0)
}

/// Waits for `pid` and prints how it ended as `<what>: <how>`.
fn reap(what: &str, pid: u32) -> Result<(), u16> {
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "{what}: {w}");
    }
    Ok(())
}

fn basic() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let _ = writeln!(Fd(1), "pipe: {r} {w}");
    show("write", sys::write(w, b"hello"));
    let mut buf = [0u8; 100];
    let n = sys::read(r, &mut buf)?;
    let _ = writeln!(
        Fd(1),
        "read: {}",
        core::str::from_utf8(&buf[..n]).unwrap_or("?")
    );
    let st = sys::fstat(r)?;
    let kind = if st.kind == u32::from(KIND_FIFO) {
        "fifo"
    } else {
        "?"
    };
    let _ = writeln!(Fd(1), "fstat: {kind}, dev {}", st.dev);
    show("seek", sys::seek(r, 0, SEEK_START));
    show("read the write end", sys::read(w, &mut buf));
    show("write the read end", sys::write(r, b"x"));
    show("tee", sys::console_tee_push(w).map(|()| "pushed"));
    sys::close(w)?;
    show("after the writer closed", sys::read(r, &mut buf));
    sys::close(r)
}

fn room() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let data = vec![b'x'; 20_000];
    show("write 20000", sys::write(w, &data));
    let mut buf = vec![0u8; 20_000];
    let mut got = 0;
    // Exactly what went in: one read more would wait for ever.
    for _ in 0..8 {
        if got >= 16_384 {
            break;
        }
        got += sys::read(r, &mut buf[got..16_384])?;
    }
    let all_x = buf[..got].iter().all(|&b| b == b'x');
    let _ = writeln!(Fd(1), "read back: {got}, all as written: {all_x}");
    sys::close(w)?;
    sys::close(r)
}

/// The checksum both sides print.
fn checksum(sum: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(sum, |s, &b| s.wrapping_mul(31).wrapping_add(u64::from(b)))
}

fn child() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let pid = start(b"drain", &[(0, r), (1, 1), (2, 2)])?;
    sys::close(r)?;
    let mut sum = 0;
    let mut piece = vec![0u8; 64 * 1024];
    for k in 0..SENT / piece.len() {
        for (i, b) in piece.iter_mut().enumerate() {
            *b = ((k * 7 + i) % 251) as u8;
        }
        sum = checksum(sum, &piece);
        sys::write_all(w, &piece)?;
    }
    sys::close(w)?;
    sys::wait(i64::from(pid), false)?;
    let _ = writeln!(Fd(1), "sent {SENT} bytes, checksum {sum}");
    Ok(())
}

/// Reads fd 0 to its end.
fn drain() -> Result<(), u16> {
    let mut buf = vec![0u8; 10_000];
    let (mut total, mut sum) = (0usize, 0);
    // 4 MiB at most, in reads of at least a byte.
    for _ in 0..4 << 20 {
        match sys::read(0, &mut buf)? {
            0 => {
                let _ = writeln!(Fd(1), "drained {total} bytes, checksum {sum}");
                return Ok(());
            }
            n => {
                sum = checksum(sum, &buf[..n]);
                total += n;
            }
        }
        if total > 4 << 20 {
            break;
        }
    }
    let _ = writeln!(Fd(1), "drained {total} bytes, and no end");
    Ok(())
}

fn eof() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    let pid = start(b"hold", &[(3, w), (2, 2)])?;
    sys::close(w)?;
    let mut buf = [0u8; 10];
    show("end of data once the child ended", sys::read(r, &mut buf));
    sys::wait(i64::from(pid), false)?;
    sys::close(r)
}

fn epipe() -> Result<(), u16> {
    let (r, w) = sys::pipe()?;
    sys::close(r)?;
    show("write with no reader", sys::write(w, b"x"));
    sys::close(w)
}

/// Writes to fd 3 until stopped: 64 MiB at most.
fn flood() -> Result<(), u16> {
    let piece = [b'f'; 4096];
    for _ in 0..(64 << 20) / piece.len() {
        sys::write_all(3, &piece)?;
    }
    let _ = writeln!(Fd(1), "flooded 64 MiB, and no end");
    Ok(())
}

fn killed() -> Result<(), u16> {
    // This program keeps both ends of each pipe, so neither child ever
    // sees an end: one waits for data, the other for room.
    let (r, w) = sys::pipe()?;
    let reader = start(b"drain", &[(0, r), (1, 1), (2, 2)])?;
    let (r2, w2) = sys::pipe()?;
    let writer = start(b"flood", &[(3, w2), (1, 1), (2, 2)])?;
    sys::sleep(200);
    sys::kill(i64::from(reader))?;
    reap("a reader blocked on an empty pipe", reader)?;
    sys::kill(i64::from(writer))?;
    reap("a writer blocked on a full pipe", writer)?;
    for fd in [r, w, r2, w2] {
        sys::close(fd)?;
    }
    Ok(())
}

fn many() -> Result<(), u16> {
    let mut ends = vec![];
    let mut said = None;
    // 32 fds: never more than 16 pipes.
    for _ in 0..17 {
        match sys::pipe() {
            Ok((r, w)) => ends.extend([r, w]),
            Err(e) => {
                said = Some(e);
                break;
            }
        }
    }
    let _ = writeln!(
        Fd(1),
        "pipes: {}, then {}",
        ends.len() / 2,
        said.map_or("no end", name)
    );
    show("the last fd", sys::open(b"/", OPEN_READ | OPEN_DIRECTORY));
    for fd in ends {
        sys::close(fd)?;
    }
    Ok(())
}
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `PIPE_FRAMES` in this scope ``; `` cannot find function `pipe_may_take` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario pipe_calls`

Expected: FAIL: scenario `pipe_calls` stops at line 16, timed out waiting for `\npipe: 3 4\nwrite: 5\nread: hello\nfstat: fifo, dev 0\nseek: EINVAL\nread the write end: EBADF\nwrite the read end: EBADF\ntee: EINVAL\nafter the writer closed: 0\n`.

- [ ] **Step 11: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 34 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 35 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 12: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! uses the same file as its parent, offset and all. The files are the
//! console and files of the VFS; milestone 3 adds pipe ends.

````

with:

````rust
//! uses the same file as its parent, offset and all. The files are the
//! console, files of the VFS and pipes' ends.

````

Replace:

````rust
    Vfs(OpenFile),
}
````

with:

````rust
    Vfs(OpenFile),
    /// An end of a pipe (spec §9.1).
    Pipe(crate::pipe::End),
}
````

- [ ] **Step 13: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
    free > USER_RESERVE_FRAMES
}
````

with:

````rust
    free > USER_RESERVE_FRAMES
}

/// The frames a pipe's ring takes (spec §16 item 8): user memory's, so
/// that pipes never use up the kernel's heap.
pub const PIPE_FRAMES: u64 = crate::pipe::SIZE as u64 / FRAME_SIZE;

/// Whether a pipe's ring may be made while `free` frames are free: it
/// leaves the reserve user memory leaves.
pub fn pipe_may_take(free: u64) -> bool {
    free >= USER_RESERVE_FRAMES + PIPE_FRAMES
}

/// A pipe's ring: [`PIPE_FRAMES`] contiguous frames, reached through the
/// linear map, given back when the pipe goes.
pub struct PipeFrames {
    phys: u64,
}

impl crate::pipe::Memory for PipeFrames {
    fn bytes(&mut self) -> &mut [u8; crate::pipe::SIZE] {
        // SAFETY: the frames are this ring's alone until it is dropped, and
        // the linear map covers every frame of RAM.
        unsafe { &mut *((PHYS_OFFSET + self.phys) as *mut [u8; crate::pipe::SIZE]) }
    }
}

impl Drop for PipeFrames {
    fn drop(&mut self) {
        if let Some(m) = MEMORY.lock().as_mut() {
            m.frames.free(self.phys, PIPE_FRAMES as usize);
        }
    }
}

/// Frames for a new pipe's ring; `ENOMEM` when they would eat into the
/// reserve, or no four contiguous frames are free.
pub fn alloc_pipe_frames() -> Result<PipeFrames, vfs::Errno> {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    if !pipe_may_take(m.frames.free_frames()) {
        return Err(vfs::Errno::ENOMEM);
    }
    let phys = m
        .frames
        .alloc(PIPE_FRAMES as usize, 1)
        .ok_or(vfs::Errno::ENOMEM)?;
    Ok(PipeFrames { phys })
}
````

- [ ] **Step 14: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::syscall::{self, Caller, Child, Outcome, Spawn};
use crate::{arch, console, klogln, mm, mounts, rtc, timer, tty, usb};
use alloc::string::String;
````

with:

````rust
use crate::syscall::{self, Caller, Child, Outcome, Spawn};
use crate::{arch, console, klogln, mm, mounts, pipe, rtc, timer, tty, usb};
use alloc::boxed::Box;
use alloc::string::String;
````

Replace:

````rust
    PROCS.lock().kill_all_but_init(relay_abi::wait::KILLED_KILL);
}
````

with:

````rust
    PROCS.lock().kill_all_but_init(relay_abi::wait::KILLED_KILL);
}

/// Wakes every process waiting on the pipe `id` (`pipe::Pipe::id`): data
/// or room came, or an end closed. Never while the table is locked, since
/// it locks it: a pipe's end is never dropped under `PROCS`, nor is a
/// pipe written.
pub fn wake_pipe(id: u64) {
    debug_assert!(!PROCS.is_locked(), "a pipe woken while PROCS is held");
    PROCS.lock().wake_all(Blocked::Pipe(id));
}
````

Replace:

````rust

    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno> {
````

with:

````rust

    fn new_pipe(&mut self) -> Result<(pipe::End, pipe::End), Errno> {
        let frames = mm::alloc_pipe_frames()?;
        Ok(pipe::new(Box::new(frames), wake_pipe))
    }

    fn pipe_wait(&mut self, id: u64) -> Result<(), Errno> {
        {
            let t = PROCS.lock();
            if t.get(t.current()).is_some_and(|p| p.killed.is_some()) {
                return Err(Errno::EINTR);
            }
        }
        // Data, room, a closed end or a kill wakes it.
        block(Blocked::Pipe(id));
        Ok(())
    }

    fn pipe_wake(&mut self, id: u64) {
        wake_pipe(id);
    }

    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno> {
````

- [ ] **Step 15: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    Sleep(u64),
}
````

with:

````rust
    Sleep(u64),
    /// The pipe with this id (`pipe::Pipe::id`) to change: data or room
    /// in it, or an end closed.
    Pipe(u64),
}
````

- [ ] **Step 16: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`; `proc_list` and `pipe` are
//! `ENOSYS` until milestone 3.

````

with:

````rust
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`, the pipe's in `pipes`;
//! `proc_list` is `ENOSYS` until milestone 3's jobs.

````

Replace:

````rust
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
    /// Line mode (`true`) or raw mode; the previous one.
````

with:

````rust
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
    /// A new pipe: its read end and its write end (spec §9.1); `ENOMEM`
    /// when its ring's frames would eat into the reserve.
    fn new_pipe(&mut self) -> Result<(crate::pipe::End, crate::pipe::End), Errno>;
    /// Waits until the pipe `id` changes (data, room, an end closed);
    /// `EINTR` if the program was killed meanwhile (or before).
    fn pipe_wait(&mut self, id: u64) -> Result<(), Errno>;
    /// Wakes whoever waits on the pipe `id`.
    fn pipe_wake(&mut self, id: u64);
    /// Line mode (`true`) or raw mode; the previous one.
````

Replace:

````rust
        Some(Call::Power) => power(caller, args[0], args[1]),
        _ => Err(Errno::ENOSYS),
````

with:

````rust
        Some(Call::Power) => power(caller, args[0], args[1]),
        Some(Call::Pipe) => pipes::pipe(caller, args[0]),
        _ => Err(Errno::ENOSYS),
````

Replace:

````rust
    let file = file(caller, fd)?;
    if let File::Vfs(open) = &*file
        && !open.is_writable()
    {
        return Err(Errno::EBADF);
    }
````

with:

````rust
    let file = file(caller, fd)?;
    match &*file {
        File::Vfs(open) if !open.is_writable() => return Err(Errno::EBADF),
        File::Pipe(end) => return pipes::write(caller, end, addr, len),
        _ => {}
    }
````

Replace:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
````

with:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
        File::Pipe(_) => unreachable!("write takes pipes apart"),
    }
````

- [ ] **Step 17: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, STAT_NOFOLLOW, Stat};
use vfs::{Errno, Vfs};
````

with:

````rust
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, KIND_FIFO, STAT_NOFOLLOW, Stat};
use vfs::{Errno, Vfs};
````

Replace:

````rust
        File::Console => return console_read(caller, addr, len),
    };
````

with:

````rust
        File::Console => return console_read(caller, addr, len),
        File::Pipe(end) => return super::pipes::read(caller, end, addr, len),
    };
````

Replace:

````rust

/// `seek(fd, offset, whence)`: the new offset. The console has none
/// (`EINVAL`).
pub(super) fn seek(
````

with:

````rust

/// `seek(fd, offset, whence)`: the new offset. The console and pipes have
/// none (`EINVAL`).
pub(super) fn seek(
````

Replace:

````rust

/// `fstat(fd, &mut Stat)`. The console is a character device, with
/// nothing else to say.
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
````

with:

````rust

/// `fstat(fd, &mut Stat)`. The console is a character device and a pipe a
/// FIFO, with nothing else to say (`dev` 0).
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
````

Replace:

````rust
            kind: u32::from(KIND_CHAR_DEVICE),
            ..Stat::default()
````

with:

````rust
            kind: u32::from(KIND_CHAR_DEVICE),
            ..Stat::default()
        },
        File::Pipe(_) => Stat {
            kind: u32::from(KIND_FIFO),
            ..Stat::default()
````

- [ ] **Step 18: Implement `kernel/src/syscall/pipes.rs`**

Insert this at the top of `kernel/src/syscall/pipes.rs`, above `#[cfg(test)]`:

````rust
//! The `pipe` call, and reading and writing a pipe's ends (user-space gate
//! §7.3, §9.1, §16 item 8). The ring never blocks (`pipe::Pipe`); the
//! waiting is here, through the `Caller`: a read waits only while the pipe
//! is empty and a writer is open, a write only while nothing of it has
//! gone in. A process killed while it waits gets `EINTR` from the wait,
//! and ends before its program sees it.

use super::Caller;
use crate::fd::{FDS, File};
use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use crate::pipe::{self, End, Side};
use alloc::sync::Arc;
use vfs::Errno;

/// `pipe(&mut [u32; 2])`: a new pipe, its read end and its write end as
/// the two lowest free fds. The memory is checked writable, and two fds
/// free (`EMFILE`), before anything is made; `ENOMEM` when the ring's
/// frames would eat into the reserve.
pub(super) fn pipe(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let slice = UserSlice::new(addr, 8)?;
    caller.writable(&slice)?;
    if caller.with_fds(|t| FDS - t.open()) < 2 {
        return Err(Errno::EMFILE);
    }
    let (read, write) = caller.new_pipe()?;
    let (read, write) = (Arc::new(File::Pipe(read)), Arc::new(File::Pipe(write)));
    let fds = caller.with_fds(|t| Ok::<_, Errno>([t.insert(read)?, t.insert(write)?]))?;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&fds[0].to_ne_bytes());
    bytes[4..].copy_from_slice(&fds[1].to_ne_bytes());
    caller.write(&slice, 0, &bytes)?;
    Ok(0)
}

/// Reads a read end into the program's buffer: what the pipe holds, up to
/// the buffer's length (at most the ring's size), waiting only while it
/// holds nothing and a writer is open; 0 at the end of the data. The
/// buffer is checked writable first, so nothing taken is lost.
pub(super) fn read(caller: &mut impl Caller, end: &End, addr: u64, len: u64) -> Result<u64, Errno> {
    if end.side() != Side::Read {
        return Err(Errno::EBADF);
    }
    let len = len.min(pipe::SIZE as u64);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let want = (len - done).min(PAGE) as usize;
        match end.pipe().read(&mut buf[..want]) {
            Some(0) => break,
            Some(n) => {
                caller.pipe_wake(end.pipe().id());
                caller.write(&slice, done, &buf[..n])?;
                done += n as u64;
            }
            None if done > 0 => break,
            None => caller.pipe_wait(end.pipe().id())?,
        }
    }
    Ok(done)
}

/// Writes the program's buffer to a write end, a page at a time: as much
/// as goes in, waiting only while nothing of it has (so a full pipe gives
/// fewer bytes than asked). `EPIPE` once every read end has closed, unless
/// some of the buffer went in first; a page that is not the program's ends
/// it the same way (`EFAULT`).
pub(super) fn write(
    caller: &mut impl Caller,
    end: &End,
    addr: u64,
    len: u64,
) -> Result<u64, Errno> {
    if end.side() != Side::Write {
        return Err(Errno::EBADF);
    }
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done == 0 { Err(e) } else { Ok(done) };
        }
        let mut put = 0;
        while put < n {
            match end.pipe().write(&buf[put..n]) {
                Ok(Some(k)) => {
                    put += k;
                    caller.pipe_wake(end.pipe().id());
                }
                Ok(None) if done + put as u64 > 0 => return Ok(done + put as u64),
                Ok(None) => caller.pipe_wait(end.pipe().id())?,
                Err(e) if done + put as u64 == 0 => return Err(e),
                Err(_) => return Ok(done + put as u64),
            }
        }
        done += n as u64;
    }
    Ok(done)
}

````

- [ ] **Step 19: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 34 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 35 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 20: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console => Err(Errno::EINVAL),
    }
````

with:

````rust
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console | File::Pipe(_) => Err(Errno::EINVAL),
    }
````

Replace:

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

with:

````rust
/// Whether `file` can be a tee: a file of the VFS open for writing
/// (`EBADF` otherwise), not the console or a pipe (`EINVAL`: a tee is
/// written from any process's context, where a full pipe could not wait).
fn tee_target(file: &File) -> Result<(), Errno> {
    match file {
        File::Vfs(open) if open.is_writable() => Ok(()),
        File::Vfs(_) => Err(Errno::EBADF),
        File::Console | File::Pipe(_) => Err(Errno::EINVAL),
    }
````

- [ ] **Step 21: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 372 tests.

- [ ] **Step 22: Run the `pipe_calls`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario pipe_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 23: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 24: Commit**

````bash
git add crates docs kernel tests userland
git commit -F - <<'EOF'
feat(kernel,relay-rt): add the pipe call

pipe makes a 16 KiB ring in four frames of user memory (ENOMEM before
the 8 MiB reserve) and gives its read and write ends the two lowest
free fds (EMFILE unless two are free, before anything is made). A read
waits only while the pipe is empty and a writer is open, a write only
while nothing of it has gone in; a killed waiter gets EINTR and ends.
fstat says FIFO, seek is EINVAL, and a pipe is no tee. relay-rt gains
sys::pipe, and t-pipe with the pipe_calls scenario shows it in QEMU.
EOF
````


### Task 5: A program ends with 141 when nobody reads its output

Spec §8.1 and decision 4: a write to fd 1 that fails with `EPIPE` ends the program at once with 141 (bash's status for a program SIGPIPE ended) and no message, so that `cat big | head -n 1` ends as soon as `head` has its line. It is in `sys::write` itself, so every way to fd 1 does it; any other fd's `EPIPE` is the program's to see (`t-pipe` sees its own). `t-pipe epipe` also starts `t-args` writing to a pipe whose reader is gone, and `pipe_calls` expects it to end with 141. The red runs are `relay-rt`'s test, which cannot find `ends_quietly`, and `pipe_calls`, where `t-args` exits with 1 instead. Mutation check: without the exit, `t-args` exits with 1 and `pipe_calls` fails.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `tests/e2e/pipe_calls.txt`
- Modify: `userland/tests/src/bin/t-pipe.rs`

**Interfaces:**
- Consumes: Task 4's `pipe`.
- Produces: `relay_rt::sys::BROKEN_PIPE` (141); `sys::write` ends the program on fd 1's `EPIPE`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
    #[test]
    fn a_write_that_takes_nothing_is_enospc() {
````

with:

````rust
    #[test]
    fn only_a_broken_pipe_on_standard_output_ends_the_program() {
        use relay_abi::errno::EPIPE;
        assert!(ends_quietly(1, Err(EPIPE)));
        assert!(!ends_quietly(3, Err(EPIPE)), "a pipe of its own");
        assert!(!ends_quietly(2, Err(EPIPE)));
        assert!(!ends_quietly(1, Err(EIO)));
        assert!(!ends_quietly(1, Ok(0)));
        assert_eq!(BROKEN_PIPE, 128 + 13);
    }

    #[test]
    fn a_write_that_takes_nothing_is_enospc() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/pipe_calls.txt`**

In `tests/e2e/pipe_calls.txt`, replace:

````text
send t-pipe epipe
expect \nwrite with no reader: EPIPE\n
send t-pipe killed
````

with:

````text
send t-pipe epipe
expect \nwrite with no reader: EPIPE\nt-args with nobody reading: exited with 141\n
send t-pipe killed
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-pipe.rs`**

In `userland/tests/src/bin/t-pipe.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//!   of the data comes only when it ends.
//! - `t-pipe epipe`: a write once the read end is closed.
//! - `t-pipe killed`: a child blocked reading an empty pipe, and one
````

with:

````rust
//!   of the data comes only when it ends.
//! - `t-pipe epipe`: a write once the read end is closed; and `t-args`
//!   writing to a pipe nobody reads, which ends it quietly with 141.
//! - `t-pipe killed`: a child blocked reading an empty pipe, and one
````

Replace:

````rust
    show("write with no reader", sys::write(w, b"x"));
    sys::close(w)
}
````

with:

````rust
    show("write with no reader", sys::write(w, b"x"));
    let args = b"t-args\0to nobody\0";
    let fds = [
        FdMap {
            child: 1,
            parent: w,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-args", args, b"", &fds, 0, 0)?;
    sys::close(w)?;
    reap("t-args with nobody reading", pid)
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find value `BROKEN_PIPE` in this scope ``; `` cannot find function `ends_quietly` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario pipe_calls`

Expected: FAIL: scenario `pipe_calls` stops at line 26, timed out waiting for `\nwrite with no reader: EPIPE\nt-args with nobody reading: exited with 141\n`.

- [ ] **Step 5: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Writes some of `bytes` to `fd`; returns how many.
pub fn write(fd: u32, bytes: &[u8]) -> Result<usize, u16> {
````

with:

````rust

/// The status a program ends with once nobody reads its standard output
/// (spec §8.1): bash's for a program SIGPIPE ended, 128 + 13.
pub const BROKEN_PIPE: u8 = 141;

/// Writes some of `bytes` to `fd`; returns how many. A write to fd 1 that
/// fails with `EPIPE` ends the program at once with [`BROKEN_PIPE`] and no
/// message, as SIGPIPE ends one on Linux: nobody reads its output any
/// more (`cat big | head -n 1`).
pub fn write(fd: u32, bytes: &[u8]) -> Result<usize, u16> {
````

Replace:

````rust
    ];
    decode(unsafe { syscall(Call::Write, args) }).map(|n| n as usize)
}
````

with:

````rust
    ];
    let r = decode(unsafe { syscall(Call::Write, args) }).map(|n| n as usize);
    if ends_quietly(fd, r) {
        exit(BROKEN_PIPE);
    }
    r
}

/// Whether a write's result ends the program: a broken pipe on standard
/// output. Any other fd's `EPIPE` is the program's to see.
fn ends_quietly(fd: u32, r: Result<usize, u16>) -> bool {
    fd == 1 && r == Err(relay_abi::errno::EPIPE)
}
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 27 tests.

- [ ] **Step 7: Run the `pipe_calls` scenario**

Run: `cargo xtask test --e2e-only --scenario pipe_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates tests userland
git commit -F - <<'EOF'
feat(relay-rt): end a program with 141 when nobody reads its output

A write to fd 1 that fails with EPIPE ends the program at once with
141 and no message, as SIGPIPE does on Linux, so `cat big | head -n 1`
ends as soon as head has its line. It is in sys::write itself, so
every way to fd 1 does it; a pipe of the program's own still sees
EPIPE.
EOF
````


### Task 6: `t-spawn fill`'s loop is bounded

Plan 5's final review (deferred minor 2) and decision 13: spec §16 item 7 says every loop of a guest test that waits for the kernel to say stop is bounded, and `t-spawn fill` looped until `EAGAIN`. It now stops after the table's 64, with `started N children, and no end` if the kernel never said the table was full. The task bounds a test program, so it has no failing run. Mutation check: a kernel that pretends to start `t-spawn nap` children without starting them, under a throwaway scenario, gets the bound's line instead of a program that never ends.

**Files:**
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: plan 3a's `t-spawn fill`.
- Produces: nothing new.

- [ ] **Step 1: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//!   table is full, and ends without waiting for them: their zombies pass
//!   to process 1;
//! - `t-spawn join` starts children in the group of another child, which
````

with:

````rust
//!   table is full, and ends without waiting for them: their zombies pass
//!   to process 1 (64 at most: a kernel that never says the table is full
//!   gets `and no end`);
//! - `t-spawn join` starts children in the group of another child, which
````

Replace:

````rust

/// Starts napping children until the table is full, and leaves them.
fn fill() -> Result<(), u16> {
    let mut n = 0;
    loop {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0, 0) {
            Ok(_) => n += 1,
            Err(relay_abi::errno::EAGAIN) => break,
            Err(e) => return Err(e),
        }
    }
    let _ = writeln!(Fd(1), "filled the table with {n} children");
    Ok(())
````

with:

````rust

/// The most processes the table holds (spec §11.1).
const TABLE: usize = 64;

/// Starts napping children until the table is full, and leaves them.
fn fill() -> Result<(), u16> {
    let mut n = 0;
    for _ in 0..TABLE {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0, 0) {
            Ok(_) => n += 1,
            Err(relay_abi::errno::EAGAIN) => {
                let _ = writeln!(Fd(1), "filled the table with {n} children");
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }
    let _ = writeln!(Fd(1), "started {n} children, and no end");
    Ok(())
````

- [ ] **Step 2: Run the `spawn`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add userland
git commit -F - <<'EOF'
test(userland): bound t-spawn fill's loop at the table's 64

Every loop of a guest test that waits for the kernel to say stop is
bounded (spec §16 item 7), and t-spawn fill's was not: a kernel that
never says the table is full now gets `and no end` instead of a
program that never ends.

Refs: plan 5's final review, minor 2
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 42 scenario(s) passed`.

````bash
git push -u origin m3p1/kernel
gh pr create --base main --head m3p1/kernel --title "feat(kernel,relay-rt)!: add pipes and joining a child's process group" --body-file - <<'EOF'
## What

Milestone 3, plan 1, tasks 1–6: `SpawnArgs` gains `pgid` (ABI 3): a child joins the group of another child of its parent's, ended or not, `EPERM` for any other; a pipe is a 16 KiB ring in four frames of user memory, its read and write ends in the fd table; `pipe` makes one (`EFAULT`, `EMFILE`, `ENOMEM` before anything is made), a read waits only while it is empty and a writer is open, a write only while nothing of it has gone in, a killed waiter ends, `EPIPE` once no reader is left; a program whose standard output's reader is gone ends quietly with 141; `t-spawn fill` is bounded. Scenarios `pipe_calls` (new) and `spawn`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types into a pipeline); plan 4's NUC check 5 runs pipes there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p1/kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Pipelines and standard input in the shell (Tasks 7–15)

The shell's side: the parser's `|`, standard input for commands (`cat`, `wc` with its new options, `head`, `tail`), and pipelines under both runners, `/bin/sh`'s as programs in one process group; with the review's fixes: a pipeline command without a name, output written before more input is read, and `sh` reading a pipe.

Branch `m3p1/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p1/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p1/kernel` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p1/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-shell m3p1/kernel`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p1/kernel>` and re-run `cargo xtask ci` before pushing.

### Task 7: The parser reads a pipeline

Spec §9.1 and decision 7: `parser::parse` returns a line's `Pipeline`, its commands joined by unquoted `|` (one for a line without one, and a command without words for a blank line). bash's syntax errors name a `|` with no command before it (``syntax error near unexpected token `|'``, also after a `>` without a file) or none after it (`syntax error: unexpected end of file`, as `bash -c 'a |'` says); `||` is refused as unsupported syntax, and so is a redirection on a command before the last (`unsupported syntax: > before |`), where bash would send that command's output into the file. The shell refuses a pipeline as before (`unsupported syntax: |`) until Tasks 11 and 13 run them; its other callers take the line's first command. The parser's existing tests go through a helper that expects one command. The red run is the shell's tests, which cannot compile against the old `parse`. Mutation checks: the `> before |` refusal, the missing file before `|`, the unexpected end and `||` each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysvfs.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: milestone 1's `parser::{Command, ParseError}`.
- Produces: `parser::Pipeline` (`Vec<Command>`), `parse(line: &str) -> Result<Pipeline, ParseError>`, `ParseError::UnexpectedEnd`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, replace:

````rust
    fn run(vfs: &mut dyn Vfs, line: &str) -> (i32, String, String) {
        let words = shell::parser::parse(line).unwrap().words;
        let (mut errors, mut out) = (Screen::default(), Screen::default());
````

with:

````rust
    fn run(vfs: &mut dyn Vfs, line: &str) -> (i32, String, String) {
        let words = shell::parser::parse(line).unwrap().remove(0).words;
        let (mut errors, mut out) = (Screen::default(), Screen::default());
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 11 replacements, top to bottom:

Replace:

````rust

    fn words(line: &str) -> Vec<String> {
        let c = parse(line).unwrap();
        assert_eq!(c.redirect, None);
````

with:

````rust

    /// The one command of a line that has no `|`.
    fn one(line: &str) -> Result<Command, ParseError> {
        parse(line).map(|mut p| {
            assert_eq!(p.len(), 1, "{line}");
            p.remove(0)
        })
    }

    fn words(line: &str) -> Vec<String> {
        let c = one(line).unwrap();
        assert_eq!(c.redirect, None);
````

Replace:

````rust
        assert_eq!(
            parse("echo x >> # f"),
            Err(ParseError::MissingTarget("newline"))
        );
        let c = parse("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
````

with:

````rust
        assert_eq!(
            one("echo x >> # f"),
            Err(ParseError::MissingTarget("newline"))
        );
        let c = one("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
````

Replace:

````rust
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
````

with:

````rust
        assert_eq!(
            one(r#"echo "$HOME""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            one(r#"echo "a $ b""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            one(r#"echo "`date`""#),
            Err(ParseError::Unsupported("`".into()))
````

Replace:

````rust
        );
        assert_eq!(parse(r"echo a\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn redirections_truncate_or_append() {
        let c = parse("echo hi > out.txt").unwrap();
        assert_eq!(c.words, ["echo", "hi"]);
````

with:

````rust
        );
        assert_eq!(one(r"echo a\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn redirections_truncate_or_append() {
        let c = one("echo hi > out.txt").unwrap();
        assert_eq!(c.words, ["echo", "hi"]);
````

Replace:

````rust
        );
        let c = parse("echo hi>>'my log'").unwrap();
        assert_eq!(
````

with:

````rust
        );
        let c = one("echo hi>>'my log'").unwrap();
        assert_eq!(
````

Replace:

````rust
        // The redirection can come first.
        let c = parse(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
````

with:

````rust
        // The redirection can come first.
        let c = one(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
````

Replace:

````rust
    fn redirection_errors() {
        assert_eq!(parse("echo >"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(parse("echo > > f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(
            parse("echo > a > b"),
            Err(ParseError::Unsupported(">".into()))
        );
        // Other streams are not supported; a quoted or spaced digit is a word.
        assert_eq!(
            parse("cat f 2>err"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(
            parse("echo a 2>>g"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(parse("echo 2 > g").unwrap().words, ["echo", "2"]);
        assert_eq!(parse("echo '2'> g").unwrap().words, ["echo", "2"]);
        assert_eq!(parse("echo x2> g").unwrap().words, ["echo", "x2"]);
        assert_eq!(
            parse("echo >").unwrap_err().to_string(),
            "syntax error near unexpected token `newline'"
````

with:

````rust
    fn redirection_errors() {
        assert_eq!(one("echo >"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(one("echo > > f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(
            one("echo > a > b"),
            Err(ParseError::Unsupported(">".into()))
        );
        // Other streams are not supported; a quoted or spaced digit is a word.
        assert_eq!(
            one("cat f 2>err"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(
            one("echo a 2>>g"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(one("echo 2 > g").unwrap().words, ["echo", "2"]);
        assert_eq!(one("echo '2'> g").unwrap().words, ["echo", "2"]);
        assert_eq!(one("echo x2> g").unwrap().words, ["echo", "x2"]);
        assert_eq!(
            one("echo >").unwrap_err().to_string(),
            "syntax error near unexpected token `newline'"
````

Replace:

````rust
        for (line, c) in [
            ("ls | wc", '|'),
            ("a; b", ';'),
````

with:

````rust
        for (line, c) in [
            ("a; b", ';'),
````

Replace:

````rust
        ] {
            assert_eq!(
                parse(line),
                Err(ParseError::Unsupported(c.into())),
                "{line}"
            );
        }
        assert_eq!(
            parse("ls | wc").unwrap_err().to_string(),
            "unsupported syntax: |"
        );
````

with:

````rust
        ] {
            assert_eq!(one(line), Err(ParseError::Unsupported(c.into())), "{line}");
        }
        assert_eq!(
            one("echo a 2>f").unwrap_err().to_string(),
            "unsupported syntax: 2>"
        );
    }

    #[test]
    fn a_bar_joins_commands_into_a_pipeline() {
        let p = parse("cat f | grep -c 'a | b' |wc -l>out").unwrap();
        let words: Vec<&[String]> = p.iter().map(|c| &c.words[..]).collect();
        assert_eq!(
            words,
            [&["cat", "f"][..], &["grep", "-c", "a | b"], &["wc", "-l"]]
        );
        assert_eq!(p[0].redirect, None);
        assert_eq!(p[2].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
        assert_eq!(
            parse(r#"echo '|' "|" \| # | x"#).unwrap()[0].words,
            ["echo", "|", "|", "|"]
        );
        assert_eq!(
            parse("").unwrap(),
            [Command {
                words: Vec::new(),
                redirect: None
            }]
        );
    }

    #[test]
    fn a_bar_needs_a_command_on_each_side() {
        // bash's messages (`bash -c '| a'`, `bash -c 'a |'`).
        for line in ["| a", "a | | b", "a || | b", "echo > | b", " |"] {
            let e = parse(line).unwrap_err();
            if line.contains("||") {
                assert_eq!(e, ParseError::Unsupported("||".into()), "{line}");
            } else {
                assert_eq!(
                    e.to_string(),
                    "syntax error near unexpected token `|'",
                    "{line}"
                );
            }
        }
        for line in ["a |", "a | b |  ", "a | # b"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "syntax error: unexpected end of file",
                "{line}"
            );
        }
        assert_eq!(parse("a || b"), Err(ParseError::Unsupported("||".into())));
        // Only the last command redirects (spec §9.1): bash would send the
        // first one's output into the file and the second nothing.
        assert_eq!(
            parse("a > f | b").unwrap_err().to_string(),
            "unsupported syntax: > before |"
        );
````

Replace:

````rust
    fn unterminated_quotes_are_errors() {
        assert_eq!(parse("echo 'abc"), Err(ParseError::UnterminatedQuote));
        assert_eq!(parse("echo \"abc\\\""), Err(ParseError::UnterminatedQuote));
    }
````

with:

````rust
    fn unterminated_quotes_are_errors() {
        assert_eq!(one("echo 'abc"), Err(ParseError::UnterminatedQuote));
        assert_eq!(one("echo \"abc\\\""), Err(ParseError::UnterminatedQuote));
    }
````

Replace:

````rust
        );
        let c = parse("echo x > ~/out").unwrap();
        assert_eq!(c.redirect.unwrap().path, "/root/out");
````

with:

````rust
        );
        let c = one("echo x > ~/out").unwrap();
        assert_eq!(c.redirect.unwrap().path, "/root/out");
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust
    pub fn program(&mut self, line: &str, stdout: &mut FakeStdout) -> (i32, String) {
        let words = crate::parser::parse(line).unwrap().words;
        let status = crate::run_command(
````

with:

````rust
    pub fn program(&mut self, line: &str, stdout: &mut FakeStdout) -> (i32, String) {
        let words = crate::parser::parse(line).unwrap().remove(0).words;
        let status = crate::run_command(
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `len` found for struct `parser::Command` in the current scope ``; `` no method named `remove` found for struct `parser::Command` in the current scope ``.

- [ ] **Step 5: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. Every other
//! shell feature is refused: an unquoted `|`, `;`, `&`, `$`, `*`, `?`, `<`,
//! `` ` ``, `(` or `)` is an error naming the character, instead of being
//! passed on as if it were plain text; so is `2>` (another stream).

````

with:

````rust
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. An unquoted `|`
//! joins commands into a pipeline (user-space gate §9.1); only the last
//! may redirect its output, and bash's syntax errors name a `|` with no
//! command before it or none after. Every other shell feature is refused:
//! an unquoted `;`, `&`, `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an
//! error naming the character, instead of being passed on as if it were
//! plain text; so are `||` and `2>` (another stream).

````

Replace:

````rust
pub const HOME: &str = "/root";

````

with:

````rust
pub const HOME: &str = "/root";

/// A line's commands: one, or several joined by `|`, each one's output the
/// next one's input. A blank line is one command without words.
pub type Pipeline = Vec<Command>;

````

Replace:

````rust
    TrailingBackslash,
    /// A redirection without a file name; holds what came instead.
    MissingTarget(&'static str),
}
````

with:

````rust
    TrailingBackslash,
    /// A redirection without a file name, or a `|` without a command
    /// before it; holds what came instead.
    MissingTarget(&'static str),
    /// A `|` without a command after it.
    UnexpectedEnd,
}
````

Replace:

````rust
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
        }
    }
}

const UNSUPPORTED: &[char] = &['|', ';', '&', '$', '*', '?', '<', '`', '(', ')'];

````

with:

````rust
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
        }
    }
}

const UNSUPPORTED: &[char] = &[';', '&', '$', '*', '?', '<', '`', '(', ')'];

````

Replace:

````rust
    }
}

pub fn parse(line: &str) -> Result<Command, ParseError> {
    let mut parts = Parts::default();
````

with:

````rust
    }

    /// The command so far, ended by a `|`, which needs one before it.
    fn take_before_pipe(&mut self) -> Result<Command, ParseError> {
        if self.pending.is_some() || self.words.is_empty() {
            return Err(ParseError::MissingTarget("|"));
        }
        if self.redirect.is_some() {
            return Err(ParseError::Unsupported("> before |".into()));
        }
        let p = core::mem::take(self);
        Ok(Command {
            words: p.words,
            redirect: None,
        })
    }
}

pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    let mut pipeline = Vec::new();
    let mut parts = Parts::default();
````

Replace:

````rust
                parts.pending = Some(chars.next_if_eq(&'>').is_some());
            }
````

with:

````rust
                parts.pending = Some(chars.next_if_eq(&'>').is_some());
            }
            '|' => {
                if chars.next_if_eq(&'|').is_some() {
                    return Err(ParseError::Unsupported("||".into()));
                }
                parts.end_word(&mut word)?;
                pipeline.push(parts.take_before_pipe()?);
            }
````

Replace:

````rust
    }
    Ok(Command {
        words: parts.words,
        redirect: parts.redirect,
    })
}
````

with:

````rust
    }
    if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
        return Err(ParseError::UnexpectedEnd);
    }
    pipeline.push(Command {
        words: parts.words,
        redirect: parts.redirect,
    });
    Ok(pipeline)
}
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let cmd = match parser::parse(line) {
            Ok(cmd) => cmd,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        if cmd.words.is_empty() && cmd.redirect.is_none() {
````

with:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse(line) {
            Ok(pipeline) => pipeline,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        if pipeline.len() > 1 {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: |\n"));
        }
        let cmd = pipeline.remove(0);
        if cmd.words.is_empty() && cmd.redirect.is_none() {
````

Replace:

````rust
        for line in text.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
                continue;
````

with:

````rust
        for line in text.lines() {
            if matches!(parser::parse(line), Ok(p) if p.len() == 1 && p[0].words.is_empty() && p[0].redirect.is_none())
            {
                continue;
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 149 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 27 tests.

- [ ] **Step 8: Run the `shell`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): parse a pipeline of commands joined by |

parse returns a line's pipeline: one command, or several joined by an
unquoted |. A | with no command before it or none after it gets bash's
syntax errors, || is refused like the other unsupported syntax, and only
the last command may redirect (spec §9.1). The shells still refuse a
pipeline until their runners can run one.
EOF
````


### Task 8: Commands get standard input, and `cat` reads it

Spec §9.1 and decision 5: a command gets standard input (`shell::Stdin`) as it gets standard output. A program's is its fd 0 (`relay-rt`'s `SysStdin`), which is the console in line mode, where the shell gives it to its command, or a pipe; the in-process runner's is bytes in memory (`shell::Bytes`), which a test gives the shell (`Shell::with_input`, `Harness::stdin`) and `host-shell` does not, so `cat` alone ends at once there. `Ctx` gains `read_input` (0 at the end, and at once without an input). `run_command` takes what a program works with as one `CommandIo`, since an eighth argument is more than clippy allows. `cat` without a file copies its input to its end, Ctrl-C or a write error (which the shell reports), instead of saying `missing operand`; a read error is GNU's `cat: -: …`. The `console` scenario types two lines into `cat` and ends it with Ctrl-D. The red runs are the shell's tests, which cannot compile, and `console`, where `cat` says `missing operand`. Mutation checks: `read_input` ignoring the input, the in-process runner not giving it, and Ctrl-C not stopping `cat` each fail a test; `cat` reading on after a write error survived at first (the output was the same), then hung on an endless input, so the test's input counts its reads and ends after 100.

**Files:**
- Modify: `crates/relay-rt/src/lib.rs`
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/relay-rt/src/sysvfs.rs`
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/program.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `tests/e2e/console.txt`

**Interfaces:**
- Consumes: plan 4a's `Ctx`, `run_command`, `Parts`.
- Produces: `shell::{Stdin, Bytes, CommandIo}`; `Ctx::{set_input, read_input(&mut self, buf: &mut [u8]) -> Result<usize, Errno>}`; `Shell::with_input(self, input: &'a mut dyn Stdin) -> Shell<'a>`; `run_command(name: &str, run: Run, args: &[String], io: CommandIo<'_>) -> i32`; `Parts::input`; `relay_rt::SysStdin`; the test harness's `Harness::stdin`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, replace:

````rust
        let run = shell::commands::find(&words[0]).unwrap().run;
        let status = shell::run_command(
            &words[0],
            run,
            &words[1..],
            vfs,
            &mut errors,
            &mut Clock,
            &mut out,
        );
        let text = |s: Screen| String::from_utf8(s.0).unwrap();
````

with:

````rust
        let run = shell::commands::find(&words[0]).unwrap().run;
        let io = shell::CommandIo {
            vfs,
            console: &mut errors,
            system: &mut Clock,
            stdin: &mut shell::Bytes::new(Vec::new()),
            stdout: &mut out,
        };
        let status = shell::run_command(&words[0], run, &words[1..], io);
        let text = |s: Screen| String::from_utf8(s.0).unwrap();
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
        );
        assert_eq!(h.run("cat"), (1, "cat: missing operand\n".into()));
    }
````

with:

````rust
        );
    }

    #[test]
    fn cat_without_a_file_copies_its_standard_input() {
        let mut h = Harness::new();
        h.stdin = b"typed\nlines".to_vec();
        assert_eq!(h.run("cat"), (0, "typed\nlines".into()));
        assert_eq!(h.run("cat"), (0, "".into()), "an input that has ended");
        // With a file, standard input is not read.
        h.stdin = b"unread".to_vec();
        assert_eq!(h.run("cat /etc/hostname"), (0, "relay\n".into()));
        // More than its buffer, into a file.
        let big = numbered(20_000);
        assert!(big.len() > 2 * super::CHUNK);
        h.stdin = big.clone().into_bytes();
        assert_eq!(h.run("cat > /tmp/copy"), (0, "".into()));
        assert_eq!(h.get("/tmp/copy"), big.as_bytes());
    }

    #[test]
    fn cat_of_standard_input_stops_at_ctrl_c_and_at_write_and_read_errors() {
        let mut h = Harness::new();
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat > /tmp/out"), (130, "^C\n".into()));
        assert!(h.get("/tmp/out").len() <= super::CHUNK, "one piece at most");
        let mut h = Harness::with_capacity(5 * 4096);
        h.stdin = numbered(20_000).into_bytes();
        assert_eq!(
            h.run("cat > /tmp/out"),
            (1, "cat: write error: No space left on device\n".into())
        );
        // Nor is the rest of the input read for nothing once the output
        // cannot be written. (A hundred pieces, then the end: a cat that
        // reads on fails the test instead of hanging it.)
        struct Endless(usize);
        impl crate::Stdin for Endless {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                self.0 += 1;
                if self.0 > 100 {
                    return Ok(0);
                }
                buf.fill(b'x');
                Ok(buf.len())
            }
        }
        let (mut input, mut out) = (Endless(0), crate::testing::FakeStdout::file(None));
        out.fail_after = Some((10_000, vfs::Errno::ENOSPC));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut input,
            stdout: &mut out,
        };
        assert_eq!(crate::run_command("cat", super::cat, &[], io), 1);
        assert_eq!(input.0, 1, "one piece, then the write error");
        h.console.take();
        // A read that fails, as a program's fd 0 can.
        struct Broken;
        impl crate::Stdin for Broken {
            fn read(&mut self, _: &mut [u8]) -> Result<usize, vfs::Errno> {
                Err(vfs::Errno::EIO)
            }
        }
        let mut out = crate::testing::FakeStdout::console();
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut Broken,
            stdout: &mut out,
        };
        let status = crate::run_command("cat", super::cat, &[], io);
        assert_eq!(
            (status, h.console.take()),
            (1, "cat: -: Input/output error\n".into())
        );
    }
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, replace:

````rust
    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
````

with:

````rust
    #[test]
    fn a_program_reads_its_standard_input() {
        let mut h = Harness::new();
        let mut out = FakeStdout::console();
        h.stdin = b"from fd 0\n".to_vec();
        assert_eq!(h.program("cat", &mut out), (0, String::new()));
        assert_eq!(out.text(), "from fd 0\n");
    }

    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
````

- [ ] **Step 4: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Bytes, Console, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
````

Replace:

````rust
    pub spy: Rc<SpyState>,
}
````

with:

````rust
    pub spy: Rc<SpyState>,
    /// The next command's standard input (in-process and as a program).
    pub stdin: Vec<u8>,
}
````

Replace:

````rust
            spy: state,
        }
````

with:

````rust
            spy: state,
            stdin: Vec::new(),
        }
````

Replace:

````rust
            spy,
        }
````

with:

````rust
            spy,
            stdin: Vec::new(),
        }
````

Replace:

````rust
    pub fn run(&mut self, line: &str) -> (i32, String) {
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system).execute(line);
        (status, self.console.take())
````

with:

````rust
    pub fn run(&mut self, line: &str) -> (i32, String) {
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system)
            .with_input(&mut input)
            .execute(line);
        (status, self.console.take())
````

Replace:

````rust
            &words[1..],
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            stdout,
        );
````

with:

````rust
            &words[1..],
            crate::CommandIo {
                vfs: &mut self.vfs,
                console: &mut self.console,
                system: &mut self.system,
                stdin: &mut Bytes::new(core::mem::take(&mut self.stdin)),
                stdout,
            },
        );
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, replace:

````text
expect end of input\nroot@relay:~# $
# Typed while a program does not read: echoed at once, and the shell's
````

with:

````text
expect end of input\nroot@relay:~# $
# cat without a file reads its standard input, the console: a line at a
# time, each echoed as typed and printed when Enter ends it, until Ctrl-D
# (milestone 3).
send cat
alive 1
key first line
expect \nfirst line\nfirst line\n
key second
expect second\nsecond\n
type {ctrl-d}
expect root@relay:~# $
# Typed while a program does not read: echoed at once, and the shell's
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find trait `Stdin` in the crate root ``; `` cannot find struct, variant or union type `CommandIo` in the crate root ``.

Run: `cargo xtask test --e2e-only --scenario console`

Expected: FAIL: scenario `console` stops at line 28, timed out waiting for `\nfirst line\nfirst line\n`.

- [ ] **Step 7: Change `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust
pub use start::name;
pub use sysio::{SysConsole, SysPrograms, SysStdout, SysSystem};
pub use sysvfs::SysVfs;
````

with:

````rust
pub use start::name;
pub use sysio::{SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem};
pub use sysvfs::SysVfs;
````

- [ ] **Step 8: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Programs, Stdout, System};
use vfs::{Errno, Node};
````

with:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Programs, Stdin, Stdout, System};
use vfs::{Errno, Node};
````

Replace:

````rust
            power_flags(force),
        )))
    }
}
````

with:

````rust
            power_flags(force),
        )))
    }
}

/// Standard input: fd 0, the console (in line mode, as the shell gives it
/// to its command) or a pipe.
pub struct SysStdin;

impl Stdin for SysStdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        sys::read(0, buf).map_err(Errno::from_number)
    }
}
````

Replace:

````rust
pub fn run_command(name: &str, run: shell::commands::Run, args: &Args) -> u8 {
    let status = shell::run_command(
        name,
        run,
        &words(args),
        &mut SysVfs::new(),
        &mut SysConsole::new(),
        &mut SysSystem,
        &mut SysStdout::new(),
    );
    status as u8
````

with:

````rust
pub fn run_command(name: &str, run: shell::commands::Run, args: &Args) -> u8 {
    let io = shell::CommandIo {
        vfs: &mut SysVfs::new(),
        console: &mut SysConsole::new(),
        system: &mut SysSystem,
        stdin: &mut SysStdin,
        stdout: &mut SysStdout::new(),
    };
    let status = shell::run_command(name, run, &words(args), io);
    status as u8
````

- [ ] **Step 9: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! `cat`, `head`, `tail` and `wc` (spec §7.3).

````

with:

````rust
//! `cat`, `head`, `tail` and `wc` (spec §7.3). Without a file they read
//! standard input (user-space gate §9.1), which GNU calls `-` in its
//! messages.

````

Replace:

````rust

/// `cat file…`
pub fn cat(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust

/// `cat [file…]`
pub fn cat(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
    if opts.operands.is_empty() {
        return ctx.fail("cat", format_args!("missing operand"));
    }
````

with:

````rust
    if opts.operands.is_empty() {
        return cat_input(ctx);
    }
````

Replace:

````rust
        }
    }
    status
}

/// The line count and the one file of `head`/`tail`: `[-n N] file`, also
````

with:

````rust
        }
    }
    status
}

/// `cat` of standard input, to its end, Ctrl-C or a write error (which the
/// shell reports).
fn cat_input(ctx: &mut Ctx<'_>) -> i32 {
    let mut buf = vec![0; CHUNK];
    while !ctx.interrupted() && !ctx.out_failed() {
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => ctx.out(&buf[..n]),
            Err(e) => return ctx.fail("cat", format_args!("-: {e}")),
        }
    }
    0
}

/// The line count and the one file of `head`/`tail`: `[-n N] file`, also
````

- [ ] **Step 10: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! What a command gets to work with: the filesystem, the system, standard
//! output (the screen, a redirection file, or a program's fd 1) and the
//! screen for errors; plus the helpers every command shares for options
//! and GNU-style messages.

use crate::io::{Console, Stdout, System};
use crate::transcript::Transcript;
````

with:

````rust
//! What a command gets to work with: the filesystem, the system, standard
//! input (none, a program's fd 0, or bytes in memory), standard output
//! (the screen, a redirection file, or a program's fd 1) and the screen
//! for errors; plus the helpers every command shares for options and
//! GNU-style messages.

use crate::io::{Console, Stdin, Stdout, System};
use crate::transcript::Transcript;
````

Replace:

````rust
    out: Output<'a>,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

with:

````rust
    out: Output<'a>,
    /// Standard input; without one, the input ends at once.
    input: Option<&'a mut dyn Stdin>,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

Replace:

````rust
            out,
            exit: false,
````

with:

````rust
            out,
            input: None,
            exit: false,
````

Replace:

````rust
            exited: false,
        }
````

with:

````rust
            exited: false,
        }
    }

    /// Gives the command standard input.
    pub(crate) fn set_input(&mut self, input: &'a mut dyn Stdin) {
        self.input = Some(input);
    }

    /// Reads standard input into `buf`: how many bytes, 0 at its end.
    pub fn read_input(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        match &mut self.input {
            Some(input) => input.read(buf),
            None => Ok(0),
        }
````

- [ ] **Step 11: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! host-shell` over the host terminal; the tests over buffers. `Programs`
//! is `/bin/sh`'s way to its commands.

````

with:

````rust
//! host-shell` over the host terminal; the tests over buffers. `Programs`
//! is `/bin/sh`'s way to its commands. `Stdin` is a command's standard
//! input: a program's fd 0, or bytes in memory (`Bytes`).

````

Replace:

````rust

/// A program's standard output, fd 1 (user-space gate §8.1): the console
````

with:

````rust

/// A command's standard input (user-space gate §9.1): a program's fd 0 (the
/// console, in line mode, or a pipe), or bytes in memory.
pub trait Stdin {
    /// Reads some bytes into `buf`: how many, 0 at the end of the input.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
}

/// Standard input that is bytes in memory: what a test gives a command, or
/// what the stage before wrote, in the in-process runner's pipelines.
pub struct Bytes {
    data: Vec<u8>,
    at: usize,
}

impl Bytes {
    pub fn new(data: Vec<u8>) -> Bytes {
        Bytes { data, at: 0 }
    }
}

impl Stdin for Bytes {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        let n = buf.len().min(self.data.len() - self.at);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// A program's standard output, fd 1 (user-space gate §8.1): the console
````

- [ ] **Step 12: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Programs, Stdout, System};
pub use program::run_command;
pub use shell::Shell;
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Bytes, Console, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command};
pub use shell::Shell;
````

- [ ] **Step 13: Change `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! A command run as a program of its own (user-space gate §8.4): one of
//! `/bin`'s programs runs its command function here, with standard output
//! on its fd 1 and errors on the console, and says what the shell said in
//! milestone 1 when the output could not be written.

use crate::commands::Run;
use crate::ctx::Ctx;
use crate::io::{Console, Stdout, System};
use alloc::format;
use alloc::string::String;
use vfs::Vfs;

````

with:

````rust
//! A command run as a program of its own (user-space gate §8.4): one of
//! `/bin`'s programs runs its command function here, with standard input
//! on its fd 0, standard output on its fd 1 and errors on the console, and
//! says what the shell said in milestone 1 when the output could not be
//! written.

use crate::commands::Run;
use crate::ctx::Ctx;
use crate::io::{Console, Stdin, Stdout, System};
use alloc::format;
use alloc::string::String;
use vfs::Vfs;

/// What a command run as a program works with: the files, the console for
/// its errors, the system, and its fds 0 and 1.
pub struct CommandIo<'a> {
    pub vfs: &'a mut dyn Vfs,
    pub console: &'a mut dyn Console,
    pub system: &'a mut dyn System,
    pub stdin: &'a mut dyn Stdin,
    pub stdout: &'a mut dyn Stdout,
}

````

Replace:

````rust
/// its own function, so it holds no other command's code.
pub fn run_command(
    name: &str,
    run: Run,
    args: &[String],
    vfs: &mut dyn Vfs,
    console: &mut dyn Console,
    system: &mut dyn System,
    stdout: &mut dyn Stdout,
) -> i32 {
    let mut ctx = Ctx::program(vfs, system, console, stdout);
    let mut status = run(&mut ctx, args);
````

with:

````rust
/// its own function, so it holds no other command's code.
pub fn run_command(name: &str, run: Run, args: &[String], io: CommandIo<'_>) -> i32 {
    let mut ctx = Ctx::program(io.vfs, io.system, io.console, io.stdout);
    ctx.set_input(io.stdin);
    let mut status = run(&mut ctx, args);
````

- [ ] **Step 14: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::ctx::Ctx;
use crate::io::{Console, Programs, System};
use crate::killed;
````

with:

````rust
use crate::ctx::Ctx;
use crate::io::{Console, Programs, Stdin, System};
use crate::killed;
````

Replace:

````rust
    pub status: i32,
}
````

with:

````rust
    pub status: i32,
    /// Standard input for a command run in the shell's process.
    pub input: Option<&'s mut dyn Stdin>,
}
````

Replace:

````rust
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.in_script = parts.in_script;
````

with:

````rust
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    if let Some(input) = parts.input {
        ctx.set_input(input);
    }
    ctx.in_script = parts.in_script;
````

- [ ] **Step 15: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, Stdout, System};
use crate::parser::{self, HOME};
````

with:

````rust
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::parser::{self, HOME};
````

Replace:

````rust
    transcript: Option<Transcript>,
}
````

with:

````rust
    transcript: Option<Transcript>,
    /// The in-process runner's standard input for its commands (a test's
    /// bytes); without it the input ends at once (`host-shell`). A
    /// spawning shell's commands read its fd 0 instead.
    input: Option<&'a mut dyn Stdin>,
}
````

Replace:

````rust
            transcript: None,
        }
    }
````

with:

````rust
            transcript: None,
            input: None,
        }
    }

    /// The same shell, its in-process commands reading `input`.
    pub fn with_input(mut self, input: &'a mut dyn Stdin) -> Shell<'a> {
        self.input = Some(input);
        self
    }
````

Replace:

````rust
            status: self.status,
        };
````

with:

````rust
            status: self.status,
            input: match &mut self.input {
                Some(input) => Some(&mut **input),
                None => None,
            },
        };
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 152 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 27 tests.

- [ ] **Step 17: Run the `console`, `utils` scenarios**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 18: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 19: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,relay-rt): give commands standard input, and read it in cat

A command gets standard input (shell::Stdin) as it gets standard
output: a program's fd 0 (relay-rt's SysStdin), which is the console in
line mode or a pipe, or bytes in memory (shell::Bytes) for the
in-process runner. run_command takes what a program works with as one
CommandIo. cat without a file copies its input to its end, Ctrl-C or a
write error, which it no longer calls a missing operand.
EOF
````


### Task 9: `wc` reads standard input, and has `-c`, `-l` and `-w`

Spec §9.1, §12.3 and decision 6: `wc` without a file counts its standard input, and `-c`, `-l` and `-w` choose the counts (§12.3's `cat big | wc -c` needs `-c`; milestone 1's `wc` took no options). Its widths are GNU wc's: one count of one input is not padded; otherwise the columns fit the regular files' total size, with at least 7 digits when an input is something else, and standard input never is a regular file here. The test helpers `testing::host_tool` and `Harness::like_host` run the host's own tool and ours on the same files and input (in a directory under `target/`, never `/tmp`, with `LC_ALL=C`; a name ending in `/` is a directory; a missing tool fails the test); the comparison covers standard input, every option, several files, a directory and files that cannot be found, and found that milestone 1's `wc` padded to 7 digits for a missing file where GNU's does not. The red run is the comparison and the Ctrl-C test. Mutation checks: one count of one file padded, standard input not 7 wide, a missing file counted as something else, `-l` alone printing all three, and Ctrl-C still printing the counts each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 8's `Ctx::read_input`.
- Produces: the test helpers `testing::host_tool(args, files, stdin) -> (i32, String, String)` and `Harness::like_host(&mut self, args, files, stdin) -> (i32, String, String)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
    #[test]
    fn wc_reports_missing_files_and_directories() {
````

with:

````rust
    #[test]
    fn wc_prints_what_gnu_wc_prints() {
        let a: &[u8] = b"hello world\nsecond line here\n";
        let b: &[u8] = b"  x\ty  ";
        let files = [("a", a), ("b", b), ("d/", &b""[..])];
        let cases: &[(&[&str], &[u8])] = &[
            (&["wc"], a),
            (&["wc", "-c"], a),
            (&["wc", "-l"], a),
            (&["wc", "-w"], b),
            (&["wc", "-lw"], a),
            (&["wc", "-cl"], b),
            (&["wc", "-l", "-c", "-w"], a),
            (&["wc"], b""),
            (&["wc", "-c"], b""),
            (&["wc", "a"], b""),
            (&["wc", "-c", "a"], b""),
            (&["wc", "a", "b"], b""),
            (&["wc", "-l", "a", "b"], b""),
            (&["wc", "-wc", "b", "a"], b""),
            (&["wc", "a", "nope"], b""),
            (&["wc", "nope", "a"], b""),
            (&["wc", "-c", "nope", "a"], b""),
            (&["wc", "-l", "nope"], b""),
            (&["wc", "-l", "d", "a"], b""),
            (&["wc", "a", "d", "nope"], b"not read"),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &files, stdin),
                crate::testing::host_tool(args, &files, stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn wc_of_standard_input_stops_at_ctrl_c() {
        let mut h = Harness::new();
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("wc -l"), (130, "^C\n".into()));
    }

    #[test]
    fn wc_reports_missing_files_and_directories() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Creates (or replaces) a file.
````

with:

````rust

    /// Runs `args` (the command's name first) as its program does, in a
    /// fresh directory holding `files` (a name ending in `/` is a
    /// directory), with `stdin` as its input: its exit
    /// status, standard output and standard error, to compare with
    /// [`host_tool`]'s.
    pub fn like_host(
        &mut self,
        args: &[&str],
        files: &[(&str, &[u8])],
        stdin: &[u8],
    ) -> (i32, String, String) {
        let dir = std::format!("/tmp/host{}", next_dir());
        self.dir(&dir);
        for (name, data) in files {
            match name.strip_suffix('/') {
                Some(d) => self.dir(&std::format!("{dir}/{d}")),
                None => self.put(&std::format!("{dir}/{name}"), data),
            }
        }
        self.vfs.chdir(dir.as_bytes()).unwrap();
        self.stdin = stdin.to_vec();
        let mut out = FakeStdout::file(None);
        let line = args
            .iter()
            .map(|a| crate::ctx::quote_if_needed(a))
            .collect::<Vec<_>>()
            .join(" ");
        let (status, errors) = self.program(&line, &mut out);
        self.vfs.chdir(b"/").unwrap();
        (status, out.text(), errors)
    }

    /// Creates (or replaces) a file.
````

Replace:

````rust
        self.vfs.lookup(path.as_bytes()).is_ok()
    }
}
````

with:

````rust
        self.vfs.lookup(path.as_bytes()).is_ok()
    }
}

/// A number for each fresh directory a test asks for.
fn next_dir() -> usize {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What the host's own tool prints for `args` (its name first), run in a
/// fresh directory under the workspace's `target/` holding `files` (a name
/// ending in `/` is a directory), with
/// `stdin` as its input and `LC_ALL=C`: its exit status, standard output
/// and standard error, to compare with [`Harness::like_host`]'s. A tool
/// that is missing fails the test.
pub fn host_tool(args: &[&str], files: &[(&str, &[u8])], stdin: &[u8]) -> (i32, String, String) {
    use std::io::Write;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/like-host")
        .join(std::format!("{}-{}", std::process::id(), next_dir()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, data) in files {
        match name.strip_suffix('/') {
            Some(d) => std::fs::create_dir(dir.join(d)).unwrap(),
            None => std::fs::write(dir.join(name), data).unwrap(),
        }
    }
    let mut child = std::process::Command::new(args[0])
        .args(&args[1..])
        .current_dir(&dir)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    // A tool that stops reading early (head) closes the pipe: not an error.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    let out = child.wait_with_output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    (
        out.status.code().unwrap_or(-1),
        text(out.stdout),
        text(out.stderr),
    )
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::text::tests::wc_prints_what_gnu_wc_prints`, `commands::text::tests::wc_of_standard_input_stops_at_ctrl_c`.

- [ ] **Step 4: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// `wc file…`: lines, words and bytes, and a total for several files.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("wc", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return ctx.fail("wc", format_args!("missing operand"));
    }
    // GNU wc sizes the columns from the files' total size, with at least
    // 7 digits when something is not a regular file.
    let mut total_size = 0;
````

with:

````rust

impl Counts {
    /// Counts `bytes`, the next piece of an input; `in_word` says whether
    /// the piece before ended inside a word.
    fn add(&mut self, bytes: &[u8], in_word: &mut bool) {
        self.bytes += bytes.len() as u64;
        for &b in bytes {
            if b == b'\n' {
                self.lines += 1;
            }
            let space = matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
            if !space && !*in_word {
                self.words += 1;
            }
            *in_word = !space;
        }
    }

    fn plus(self, o: Counts) -> Counts {
        Counts {
            lines: self.lines.saturating_add(o.lines),
            words: self.words.saturating_add(o.words),
            bytes: self.bytes.saturating_add(o.bytes),
        }
    }
}

/// Which counts `wc` prints: those its options name (`-l`, `-w`, `-c`), or
/// all three, always in that order.
#[derive(Clone, Copy)]
struct Shown {
    lines: bool,
    words: bool,
    bytes: bool,
}

impl Shown {
    fn how_many(self) -> usize {
        [self.lines, self.words, self.bytes]
            .iter()
            .filter(|&&s| s)
            .count()
    }

    /// The shown counts, each right-aligned to `width`, then the name.
    fn line(self, c: Counts, width: usize, name: Option<&str>) -> String {
        let mut parts: Vec<String> = [
            (self.lines, c.lines),
            (self.words, c.words),
            (self.bytes, c.bytes),
        ]
        .iter()
        .filter(|(shown, _)| *shown)
        .map(|(_, n)| alloc::format!("{n:>width$}"))
        .collect();
        parts.extend(name.map(String::from));
        parts.join(" ")
    }
}

/// `wc [-clw] [file…]`: lines, words and bytes, and a total for several
/// files; standard input without a file.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "clw", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("wc", format_args!("{e}")),
    };
    let (lines, words, bytes) = (opts.has('l'), opts.has('w'), opts.has('c'));
    let any = lines || words || bytes;
    let shown = Shown {
        lines: lines || !any,
        words: words || !any,
        bytes: bytes || !any,
    };
    if opts.operands.is_empty() {
        return wc_input(ctx, shown);
    }
    // GNU's widths: one count of one file is not padded; otherwise the
    // columns fit the regular files' total size, with at least 7 digits
    // when one of the files is something else. A file that cannot be found
    // takes no part (milestone 1 counted it as something else).
    let mut total_size = 0;
````

Replace:

````rust
            }
            Err(e) => {
                odd = true;
                found.push(Err(e));
            }
        }
    }
    let digits = total_size.max(1).ilog10() as usize + 1;
    let width = if odd { digits.max(7) } else { digits };
    let mut status = 0;
````

with:

````rust
            }
            Err(e) => found.push(Err(e)),
        }
    }
    let digits = total_size.max(1).ilog10() as usize + 1;
    let unpadded = found.len() == 1 && shown.how_many() == 1;
    let width = match (unpadded, odd) {
        (true, _) => 1,
        (false, true) => digits.max(7),
        (false, false) => digits,
    };
    let mut status = 0;
````

Replace:

````rust
        };
        total.lines = total.lines.saturating_add(counts.lines);
        total.words = total.words.saturating_add(counts.words);
        total.bytes = total.bytes.saturating_add(counts.bytes);
        let c = counts;
        outln!(
            ctx,
            "{:>width$} {:>width$} {:>width$} {name}",
            c.lines,
            c.words,
            c.bytes
        );
    }
    if opts.operands.len() > 1 {
        let c = total;
        outln!(
            ctx,
            "{:>width$} {:>width$} {:>width$} total",
            c.lines,
            c.words,
            c.bytes
        );
    }
    status
````

with:

````rust
        };
        total = total.plus(counts);
        outln!(ctx, "{}", shown.line(counts, width, Some(&name)));
    }
    if opts.operands.len() > 1 {
        outln!(ctx, "{}", shown.line(total, width, Some("total")));
    }
    status
}

/// `wc` of standard input, which is never a regular file here (the
/// console or a pipe): 7 digits, unless only one count is shown.
fn wc_input(ctx: &mut Ctx<'_>, shown: Shown) -> i32 {
    let mut buf = vec![0; CHUNK];
    let (mut c, mut in_word) = (Counts::default(), false);
    let mut status = 0;
    loop {
        if ctx.interrupted() {
            return 0;
        }
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => c.add(&buf[..n], &mut in_word),
            Err(e) => {
                status = ctx.fail("wc", format_args!("-: {e}"));
                break;
            }
        }
    }
    let width = if shown.how_many() == 1 { 1 } else { 7 };
    outln!(ctx, "{}", shown.line(c, width, None));
    status
````

Replace:

````rust
    stream(ctx, node, 0, |_, bytes| {
        c.bytes += bytes.len() as u64;
        for &b in bytes {
            if b == b'\n' {
                c.lines += 1;
            }
            let space = matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
            if !space && !in_word {
                c.words += 1;
            }
            in_word = !space;
        }
        true
````

with:

````rust
    stream(ctx, node, 0, |_, bytes| {
        c.add(bytes, &mut in_word);
        true
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 154 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): read standard input in wc, and give it -c, -l and -w

wc without a file counts its standard input; -c, -l and -w choose the
counts (§12.3's `cat big | wc -c` needs -c). Its widths are GNU wc's,
which a test now compares with the host's own wc over standard input,
files, missing files and a directory: a file that cannot be found takes
no part in them, where milestone 1's padded to 7 digits.
EOF
````


### Task 10: `head` and `tail` read standard input

Spec §9.1 and decision 5: `head` and `tail` without a file read standard input. `head` stops reading once its lines are out, so the writer before it in a pipeline gets `EPIPE`; `tail` reads to the end, keeping the last lines as they come, and joins a line a pipe cut across two reads. GNU's message names standard input (`error reading 'standard input'`). The tests compare both with the host's on 17 inputs (no final newline, empty, blank lines, more than a read), count the reads `head` takes, feed `tail` three bytes a read, and stop `tail` with Ctrl-C. The red run is those tests. Mutation checks: `head` reading on after its lines, `tail` keeping one line too many, and `tail` ignoring Ctrl-C each fail a test; `tail` not joining a cut line survived at first (the comparisons' inputs came in big pieces), hence the three-byte reads.

**Files:**
- Modify: `crates/shell/src/commands/text.rs`

**Interfaces:**
- Consumes: Task 8's `Ctx::read_input`; Task 9's `host_tool`, `like_host`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn head_and_tail_errors() {
````

with:

````rust
    #[test]
    fn head_and_tail_of_standard_input_print_what_gnu_s_print() {
        let numbered_bytes = numbered(30);
        let ten: &[u8] = numbered_bytes.as_bytes();
        let unended: &[u8] = b"one\ntwo\nthree";
        let big_text = numbered(20_000);
        let big: &[u8] = big_text.as_bytes();
        let cases: &[(&[&str], &[u8])] = &[
            (&["head"], ten),
            (&["head", "-n", "3"], ten),
            (&["head", "-3"], ten),
            (&["head", "-n", "0"], ten),
            (&["head", "-n", "5"], unended),
            (&["head", "-n", "2"], unended),
            (&["head"], b""),
            (&["head", "-n", "2"], big),
            (&["tail"], ten),
            (&["tail", "-n", "3"], ten),
            (&["tail", "-2"], unended),
            (&["tail", "-n", "1"], unended),
            (&["tail", "-n", "0"], ten),
            (&["tail", "-n", "100"], ten),
            (&["tail"], b""),
            (&["tail", "-n", "5"], big),
            (&["tail", "-n", "3"], b"\n\n\n\n"),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &[], stdin),
                crate::testing::host_tool(args, &[], stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn head_of_standard_input_reads_no_more_than_its_lines() {
        // A line a read, as a pipe may give them; a hundred, then the end
        // (a head that reads on fails the test instead of hanging it).
        struct Lines(usize);
        impl crate::Stdin for Lines {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                self.0 += 1;
                if self.0 > 100 {
                    return Ok(0);
                }
                buf[..2].copy_from_slice(b"x\n");
                Ok(2)
            }
        }
        let mut h = Harness::new();
        let (mut input, mut out) = (Lines(0), crate::testing::FakeStdout::file(None));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut input,
            stdout: &mut out,
        };
        let args = [String::from("-n"), String::from("3")];
        assert_eq!(crate::run_command("head", super::head, &args, io), 0);
        assert_eq!((out.text().as_str(), input.0), ("x\nx\nx\n", 3));
        h.stdin = numbered(20_000).into_bytes();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("tail -n 1"), (130, "^C\n".into()));
    }

    #[test]
    fn tail_of_standard_input_joins_lines_a_pipe_cuts() {
        // Three bytes a read, as a pipe may give them.
        struct Cut(&'static [u8]);
        impl crate::Stdin for Cut {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                let n = self.0.len().min(3).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let mut h = Harness::new();
        let mut out = crate::testing::FakeStdout::file(None);
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut Cut(b"one\ntwo\nthree\nfour"),
            stdout: &mut out,
        };
        let args = [String::from("-n"), String::from("2")];
        assert_eq!(crate::run_command("tail", super::tail, &args, io), 0);
        assert_eq!(out.text(), "three\nfour");
    }

    #[test]
    fn head_and_tail_errors() {
````

Replace:

````rust
        );
        assert_eq!(h.run("head"), (1, "head: missing operand\n".into()));
        assert_eq!(h.run("tail a b"), (1, "tail: extra operand 'b'\n".into()));
````

with:

````rust
        );
        assert_eq!(h.run("tail a b"), (1, "tail: extra operand 'b'\n".into()));
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `commands::text::tests::tail_of_standard_input_joins_lines_a_pipe_cuts`, `commands::text::tests::head_of_standard_input_reads_no_more_than_its_lines`.

- [ ] **Step 3: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

/// The line count and the one file of `head`/`tail`: `[-n N] file`, also
/// `-N`.
fn lines_and_file(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
) -> Result<(u64, Node, String), i32> {
    // A first argument `-5` means `-n 5`, as in GNU head and tail.
````

with:

````rust

/// The line count and the one file of `head`/`tail`: `[-n N] [file]`, also
/// `-N`; no file is standard input.
fn lines_and_file(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
) -> Result<(u64, Option<(Node, String)>), i32> {
    // A first argument `-5` means `-n 5`, as in GNU head and tail.
````

Replace:

````rust
    let file = match &opts.operands[..] {
        [] => return Err(ctx.fail(name, format_args!("missing operand"))),
        [file] => file,
````

with:

````rust
    let file = match &opts.operands[..] {
        [] => return Ok((count, None)),
        [file] => file,
````

Replace:

````rust
    })?;
    Ok((count, node, file.clone()))
}

/// `head [-n N] file`: the first N lines (10 by default).
pub fn head(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, node, file) = match lines_and_file(ctx, "head", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let mut left = count;
    let result = stream(ctx, node, 0, |ctx, bytes| {
        let mut end = 0;
        while left > 0 && end < bytes.len() {
            match bytes[end..].iter().position(|&b| b == b'\n') {
                Some(i) => {
                    end += i + 1;
                    left -= 1;
                }
                None => end = bytes.len(),
            }
        }
        ctx.out(&bytes[..end]);
````

with:

````rust
    })?;
    Ok((count, Some((node, file.clone()))))
}

/// How much of `bytes`, the next piece of an input, is within the `left`
/// lines still wanted; counts off the lines it ends.
fn within(bytes: &[u8], left: &mut u64) -> usize {
    let mut end = 0;
    while *left > 0 && end < bytes.len() {
        match bytes[end..].iter().position(|&b| b == b'\n') {
            Some(i) => {
                end += i + 1;
                *left -= 1;
            }
            None => end = bytes.len(),
        }
    }
    end
}

/// `head [-n N] [file]`: the first N lines (10 by default).
pub fn head(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, file) = match lines_and_file(ctx, "head", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let mut left = count;
    let Some((node, file)) = file else {
        return head_input(ctx, left);
    };
    let result = stream(ctx, node, 0, |ctx, bytes| {
        let end = within(bytes, &mut left);
        ctx.out(&bytes[..end]);
````

Replace:

````rust

/// `tail [-n N] file`: the last N lines (10 by default). It reads backwards
/// from the end, so a big file costs only what is shown.
pub fn tail(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, node, file) = match lines_and_file(ctx, "tail", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
````

with:

````rust

/// `head` of standard input: no more of it is read once the lines are out,
/// so a pipe's writer gets `EPIPE` once `head` has ended.
fn head_input(ctx: &mut Ctx<'_>, mut left: u64) -> i32 {
    let mut buf = vec![0; CHUNK];
    while left > 0 && !ctx.interrupted() && !ctx.out_failed() {
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let end = within(&buf[..n], &mut left);
                ctx.out(&buf[..end]);
            }
            Err(e) => return ctx.fail("head", format_args!("error reading 'standard input': {e}")),
        }
    }
    0
}

/// `tail [-n N] [file]`: the last N lines (10 by default). It reads a file
/// backwards from the end, so a big file costs only what is shown;
/// standard input is read to its end, keeping the last N lines.
pub fn tail(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let (count, file) = match lines_and_file(ctx, "tail", args) {
        Ok(x) => x,
        Err(status) => return status,
    };
    let Some((node, file)) = file else {
        return tail_input(ctx, count);
    };
````

Replace:

````rust
        Err(e) => ctx.fail("tail", format_args!("error reading {}: {e}", quote(&file))),
    }
}

````

with:

````rust
        Err(e) => ctx.fail("tail", format_args!("error reading {}: {e}", quote(&file))),
    }
}

/// `tail` of standard input: the last `count` lines, the newest perhaps
/// without its newline, kept as they come.
fn tail_input(ctx: &mut Ctx<'_>, count: u64) -> i32 {
    let mut lines: alloc::collections::VecDeque<Vec<u8>> = alloc::collections::VecDeque::new();
    let mut buf = vec![0; CHUNK];
    loop {
        if ctx.interrupted() {
            return 0;
        }
        let n = match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return ctx.fail("tail", format_args!("error reading 'standard input': {e}")),
        };
        for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
            match lines.back_mut() {
                Some(last) if last.last() != Some(&b'\n') => last.extend_from_slice(piece),
                _ => lines.push_back(piece.to_vec()),
            }
            if lines.len() as u64 > count {
                lines.pop_front();
            }
        }
    }
    for line in lines {
        ctx.out(&line);
    }
    0
}

````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 157 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): read standard input in head and tail

head and tail without a file read standard input: head stops reading
once its lines are out, so a pipe's writer then gets EPIPE; tail reads
to the end, keeping the last lines, a line cut across reads joined.
Tests compare both with the host's own head and tail.
EOF
````


### Task 11: Pipelines in the in-process runner

Decision 7: the tests and `host-shell` run a pipeline's commands one after another, each one's output kept in memory as the next one's input, the first reading the shell's input; a command that is not found says so and gives the next nothing, Ctrl-C stops the rest, and the status is the last command's, as bash's (each expectation was checked with `bash -c`). `Runner` gains `pipeline`; the spawning runner still refuses one until Task 13. `cd`, `exit` and `help` cannot be in a pipeline (`relay-sh: cd: cannot be used in a pipeline`, status 1, and nothing of the line runs), nor can `sh` in the in-process runner, which would run its script in that shell. A script's test used `ls | wc` as a line that does not parse; it uses `ls; wc` now. The red run is the shell's tests. Mutation checks: a stage not given the one before's output, the first stage not given the shell's input, the built-in and `sh` refusals, and a stage's Ctrl-C not stopping the rest each fail a test (the last survived at first, since the last stage saw the interrupt too: `cat big | echo after` tells them apart).

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 7's `Pipeline`; Task 8's `Bytes`, `Parts::input`.
- Produces: `Runner::pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran`; `runner::in_a_pipeline(name) -> Ran`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo 'open\nls | wc\necho after\n");
        assert_eq!(
````

with:

````rust
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo 'open\nls; wc\necho after\n");
        assert_eq!(
````

Replace:

````rust
                "+ echo 'open\nrelay-sh: syntax error: unterminated quote\n\
                 + ls | wc\nrelay-sh: unsupported syntax: |\n+ echo after\nafter\n"
                    .into()
````

with:

````rust
                "+ echo 'open\nrelay-sh: syntax error: unterminated quote\n\
                 + ls; wc\nrelay-sh: unsupported syntax: ;\n+ echo after\nafter\n"
                    .into()
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn unknown_commands_and_syntax_errors() {
````

with:

````rust
    #[test]
    fn a_pipeline_hands_each_command_s_output_to_the_next() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"one\ntwo\nthree\n");
        // What bash prints for each (`bash -c '…'`).
        assert_eq!(h.run("echo hello world | wc -c"), (0, "12\n".into()));
        assert_eq!(
            h.run("cat /tmp/f | head -n 2 | tail -n 1"),
            (0, "two\n".into())
        );
        // Not the screen: one name a line.
        assert_eq!(h.run("ls /etc | cat"), (0, "hostname\nmotd\n".into()));
        // The last command's redirection; and its status is the line's.
        assert_eq!(h.run("cat /tmp/f | wc -l > /tmp/n"), (0, "".into()));
        assert_eq!(h.get("/tmp/n"), b"3\n");
        assert_eq!(
            h.run("cat /nope | wc -l"),
            (0, "cat: /nope: No such file or directory\n0\n".into())
        );
        assert_eq!(
            h.run("echo x | cat /nope"),
            (1, "cat: /nope: No such file or directory\n".into())
        );
        // A command that is not found gives the next nothing, as in bash.
        assert_eq!(
            h.run("nosuch | wc -l"),
            (0, "relay-sh: nosuch: command not found\n0\n".into())
        );
        assert_eq!(
            h.run("echo x | nosuch"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        // The first command reads the shell's standard input.
        h.stdin = b"a\nb\n".to_vec();
        assert_eq!(h.run("cat | wc -l"), (0, "2\n".into()));
    }

    #[test]
    fn the_shell_s_own_commands_cannot_be_in_a_pipeline() {
        let mut h = Harness::new();
        for (line, name) in [
            ("cd /tmp | cat", "cd"),
            ("echo a | exit 3", "exit"),
            ("help | wc", "help"),
            ("ls | sh x | wc", "sh"),
        ] {
            assert_eq!(
                h.run(line),
                (
                    1,
                    alloc::format!("relay-sh: {name}: cannot be used in a pipeline\n")
                ),
                "{line}"
            );
        }
        // Nothing ran: not even the commands before it.
        assert_eq!(h.run("pwd"), (0, "/\n".into()));
        h.put("/tmp/x", b"");
        assert_eq!(
            h.run("echo a > /tmp/x | cd /"),
            (2, "relay-sh: unsupported syntax: > before |\n".into())
        );
    }

    #[test]
    fn ctrl_c_stops_a_pipeline() {
        let mut h = Harness::new();
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big | wc -c"), (130, "^C\n".into()));
        // The rest does not run, even what would not have asked.
        assert_eq!(h.run("cat /tmp/big | echo after"), (130, "^C\n".into()));
    }

    #[test]
    fn unknown_commands_and_syntax_errors() {
````

Replace:

````rust
        assert_eq!(
            h.run("ls | wc"),
            (2, "relay-sh: unsupported syntax: |\n".into())
        );
````

with:

````rust
        assert_eq!(
            h.run("ls | ;"),
            (2, "relay-sh: unsupported syntax: ;\n".into())
        );
        assert_eq!(
            h.run("ls |"),
            (2, "relay-sh: syntax error: unexpected end of file\n".into())
        );
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `shell::tests::a_pipeline_hands_each_command_s_output_to_the_next`, `shell::tests::ctrl_c_stops_a_pipeline`.

- [ ] **Step 4: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2): the `Runner`. The in-process runner runs the
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests and `cargo xtask host-shell`, where
//! there are no programs); the spawning runner starts `/bin/<name>` for
//! every one (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, Programs, Stdin, System};
use crate::killed;
use crate::parser::Redirect;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND};
````

with:

````rust
//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2), and a pipeline (§9.1): the `Runner`. The
//! in-process runner runs the command functions against the shell's `Vfs`,
//! `Console` and `System`, as milestone 1 does (the unit tests and `cargo
//! xtask host-shell`, where there are no programs), a pipeline's stages one
//! after another, each one's output kept in memory as the next one's
//! input; the spawning runner starts `/bin/<name>` for every one
//! (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Bytes, Console, Programs, Stdin, Stdout, System};
use crate::killed;
use crate::parser::{Command, Redirect};
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND};
````

Replace:

````rust
    ) -> Ran;
}
````

with:

````rust
    ) -> Ran;

    /// Runs a pipeline of two or more commands, none of them a built-in,
    /// the last one's output going to its redirection if it has one.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran;
}
````

Replace:

````rust
            None => not_found(name),
        }
    }
}

/// Every command a program: `/bin/<name>`, or the path as given.
````

with:

````rust
            None => not_found(name),
        }
    }

    /// Each stage runs to its end before the next starts, its output kept
    /// as the next one's input; a stage that is not found says so and
    /// gives the next one nothing, as bash's does. Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
        if stages.iter().any(|c| c.words[0] == "sh") {
            return in_a_pipeline("sh");
        }
        let Parts {
            vfs,
            console,
            system,
            transcript,
            in_script,
            status,
            mut input,
        } = parts;
        let (last, before) = stages.split_last().expect("a pipeline has stages");
        let mut piped: Option<Bytes> = None;
        for stage in before {
            let (name, args) = (&stage.words[0], &stage.words[1..]);
            let mut out = Collected(Vec::new());
            if let Some(command) = commands::find(name) {
                let mut ctx = Ctx::program(&mut *vfs, &mut *system, &mut *console, &mut out);
                match (&mut piped, &mut input) {
                    (Some(bytes), _) => ctx.set_input(bytes),
                    (None, Some(first)) => ctx.set_input(&mut **first),
                    (None, None) => {}
                }
                ctx.transcript = transcript.take();
                (command.run)(&mut ctx, args);
                let _ = ctx.finish();
                let cancelled = ctx.cancelled;
                *transcript = ctx.transcript.take();
                if cancelled {
                    return Ran::said(CANCELLED, String::from("^C\n"));
                }
            } else {
                let message = not_found(name).message;
                console.write(message.as_bytes());
                if let Some(t) = transcript.as_mut() {
                    let _ = t.add(&mut *vfs, message.as_bytes());
                }
            }
            piped = Some(Bytes::new(out.0));
        }
        let mut piped = piped.expect("a stage before the last");
        let parts = Parts {
            vfs,
            console,
            system,
            transcript,
            in_script,
            status,
            input: Some(&mut piped),
        };
        self.run(
            parts,
            &last.words[0],
            &last.words[1..],
            last.redirect.as_ref(),
        )
    }
}

/// A stage's output in the in-process runner: kept whole for the next.
struct Collected(Vec<u8>);

impl Stdout for Collected {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn is_tty(&self) -> bool {
        false
    }
    fn node(&self) -> Option<Node> {
        None
    }
}

/// A command the shell runs itself, refused in a pipeline (§9.1).
pub(crate) fn in_a_pipeline(name: &str) -> Ran {
    Ran::said(1, format!("{NAME}: {name}: cannot be used in a pipeline\n"))
}

/// Every command a program: `/bin/<name>`, or the path as given.
````

Replace:

````rust
            Err(e) => cannot_start(name, e),
        }
    }
}
````

with:

````rust
            Err(e) => cannot_start(name, e),
        }
    }

    fn pipeline(&mut self, _parts: Parts<'_>, _stages: &[Command]) -> Ran {
        Ran::said(
            crate::shell::SYNTAX,
            format!("{NAME}: unsupported syntax: |\n"),
        )
    }
}
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        if pipeline.len() > 1 {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: |\n"));
        }
````

with:

````rust
        if pipeline.len() > 1 {
            return self.pipeline(&pipeline);
        }
````

Replace:

````rust
        self.finish(status, ran.message)
    }
````

with:

````rust
        self.finish(status, ran.message)
    }

    /// Runs a pipeline (user-space gate §9.1): its status is the last
    /// command's. The shell's own commands cannot be in one.
    fn pipeline(&mut self, stages: &[parser::Command]) -> i32 {
        let builtin = stages
            .iter()
            .find(|c| commands::builtin(&c.words[0]).is_some());
        if let Some(c) = builtin {
            let ran = runner::in_a_pipeline(&c.words[0]);
            return self.finish(ran.status, ran.message);
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: match &mut self.input {
                Some(input) => Some(&mut **input),
                None => None,
            },
        };
        let ran = self.runner.get().pipeline(parts, stages);
        self.finish(ran.status, ran.message)
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 160 tests.

- [ ] **Step 7: Run the `shell`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): run pipelines in the in-process runner

The tests and host-shell run a pipeline's commands one after another,
each one's output kept in memory as the next one's input; the first
reads the shell's input. Its status is the last command's, a command
that is not found gives the next nothing, and Ctrl-C stops the rest, as
in bash. cd, exit, help (and sh, which runs its script in this shell)
cannot be in a pipeline: `relay-sh: cd: cannot be used in a pipeline`.
The spawning runner still refuses pipelines.
EOF
````


### Task 12: Every command of a pipeline has a name

The prototype's review (important, I1) and decision 7: the parser accepted a redirection alone as a pipeline's last command (`echo hi | > f`), and both runners then took the name of a command without words: `/bin/sh` panicked (`index out of bounds`), init started it again, and three such lines within 10 s reached the error screen. bash runs such a line (it creates the file); here a pipeline's commands are programs, so the parser refuses one without a name: `a | > f` is `unsupported syntax: | >`, and `> f | b` gets the `> before |` that decision 7 gave a redirection before `|` (it said bash's syntax error, which bash does not give: the review's minor M1). The red run is the shell's tests: the parser accepts both lines, and the shell's test panics. Mutation checks: accepting the last command without a name, and taking a redirection alone before `|` as nothing, each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 7's `parse`; Task 11's `Shell::pipeline`.
- Produces: nothing new (every command of a parsed pipeline of two or more has a word).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn unterminated_quotes_are_errors() {
````

with:

````rust
    #[test]
    fn every_command_of_a_pipeline_has_a_name() {
        // bash runs a redirection alone as a command; here a pipeline's
        // commands are programs, so one without a name is refused.
        for (line, what) in [
            ("> f | b", "> before |"),
            (">> f | b | c", "> before |"),
            ("a | > f", "| >"),
            ("a | b | >> f", "| >"),
        ] {
            assert_eq!(
                parse(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        for line in ["a | b", "a|b>f", "x | y | z >> f", "a # | > f"] {
            let p = parse(line).unwrap();
            assert!(p.iter().all(|c| !c.words.is_empty()), "{line}");
        }
        // A redirection alone, without a pipeline, still makes the file.
        assert_eq!(one("> f").unwrap().words, Vec::<String>::new());
    }

    #[test]
    fn unterminated_quotes_are_errors() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn ctrl_c_stops_a_pipeline() {
````

with:

````rust
    #[test]
    fn a_pipeline_command_without_a_name_is_refused_by_both_runners() {
        let mut h = Harness::new();
        for line in ["echo hi | > /tmp/f", "> /tmp/f | cat"] {
            let said = if line.starts_with('>') {
                "> before |"
            } else {
                "| >"
            };
            let want = (2, alloc::format!("relay-sh: unsupported syntax: {said}\n"));
            assert_eq!(h.run(line), want, "{line}");
            assert_eq!(h.spawning(line), want, "{line}");
        }
        assert!(!h.exists("/tmp/f"), "nothing of the line ran");
        assert!(h.programs.spawned.is_empty());
    }

    #[test]
    fn ctrl_c_stops_a_pipeline() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::every_command_of_a_pipeline_has_a_name`, `shell::tests::a_pipeline_command_without_a_name_is_refused_by_both_runners`.

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! begins a comment, which runs to the end of the line. An unquoted `|`
//! joins commands into a pipeline (user-space gate §9.1); only the last
//! may redirect its output, and bash's syntax errors name a `|` with no
//! command before it or none after. Every other shell feature is refused:
//! an unquoted `;`, `&`, `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an
````

with:

````rust
//! begins a comment, which runs to the end of the line. An unquoted `|`
//! joins commands into a pipeline (user-space gate §9.1); each has a name,
//! only the last may redirect its output, and bash's syntax errors name a
//! `|` with no command before it or none after. Every other shell feature is refused:
//! an unquoted `;`, `&`, `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an
````

Replace:

````rust
    /// The command so far, ended by a `|`, which needs one before it.
    fn take_before_pipe(&mut self) -> Result<Command, ParseError> {
        if self.pending.is_some() || self.words.is_empty() {
            return Err(ParseError::MissingTarget("|"));
````

with:

````rust
    /// The command so far, ended by a `|`, which needs one before it.
    /// Every command of a pipeline has a name: a redirection alone, which
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
            return Err(ParseError::MissingTarget("|"));
````

Replace:

````rust
    }
    if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
        return Err(ParseError::UnexpectedEnd);
    }
````

with:

````rust
    }
    if !pipeline.is_empty() && parts.words.is_empty() {
        return Err(match parts.redirect {
            None => ParseError::UnexpectedEnd,
            Some(_) => ParseError::Unsupported("| >".into()),
        });
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 162 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse a pipeline command without a name

A redirection alone as a pipeline's command (`echo hi | > f`) reached
the shell with no words, and /bin/sh panicked on its name: three such
lines within 10 s took the machine to the error screen. The parser now
refuses it, as it refuses a redirection before |, and gives `> f | b`
that message too instead of bash's syntax error, which bash does not
give.

Refs: review I1, M1
EOF
````


### Task 13: Pipelines of programs in `/bin/sh`

Spec §9.1 and decision 7: `/bin/sh` runs a pipeline's stages as programs in one process group. `Programs` gains `pipe`, and its `spawn` a standard input and a `Group` instead of a foreground flag: `New` for the first stage that starts (`NEW_GROUP | FOREGROUND`: it gets the console), `Join` for the others (Task 2), `Shell` in a script (its own group, as before); `SysPrograms` maps them, and gives every command line mode, as before, in a shell that leads no group. The spawning runner opens the last stage's redirection first, makes each pipe just before the stage that writes it starts, and closes its copies of the ends as soon as the stages have them, so that a reader sees its end once its writer has ended and a writer gets `EPIPE` once its reader has. A stage that cannot start says so at once, and its neighbours see an end; a pipe that cannot be made stops the starting (`relay-sh: pipe error: …`, status 1). The shell waits for every stage, says how any killed one ended and a Ctrl-C once, and takes the last one's status (a stage that ended with 141 says nothing). The `pipes` scenario sends 8 MiB through `cat` and `wc -c`, ends `cat big | head -n 1` at once, starts a first stage that ends before the next, types into `cat | wc -l` with Ctrl-D, stops `t-spin | cat | cat` with Ctrl-C (the kernel log names all three), runs a script's pipeline into its transcript, and compares `free` before and after. The red runs are the shell's tests, which cannot compile, and `pipes`, which `/bin/sh` refuses. Mutation checks: on the host, the shell keeping its pipe ends, later stages in new groups, `^C` once per stage, every stage's status taken, the pipe error's closes and the first pid not kept each fail a test; in QEMU, `SysPrograms` not joining (the later stages outlive Ctrl-C) and the pipe not given as fd 0 (`wc` never ends) each fail `pipes`.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Create: `tests/e2e/pipes.txt`

**Interfaces:**
- Consumes: Task 2's `sys::spawn` with a group; Task 4's `sys::pipe`; Task 11's `Runner::pipeline`.
- Produces: `shell::Group::{Shell, New, Join(u32)}`; `Programs::{pipe(&mut self) -> Result<(u32, u32), Errno>, spawn(&mut self, path, args, stdin: Option<u32>, stdout: Option<u32>, group: Group) -> Result<u32, Errno>}`; `relay_rt::sysio::command_fds(stdin, stdout)`; the test fake's `FakePrograms::{pipes, pipe_error}` and `Spawned::{stdin, group}`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
    #[test]
    fn a_command_gets_the_shell_s_fds_but_its_redirection() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds(None)), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds(Some(5))), [(0, 0), (1, 5), (2, 2)]);
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
````

with:

````rust
    #[test]
    fn a_command_gets_the_shell_s_fds_but_its_pipes_and_redirection() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds(None, None)), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds(None, Some(5))), [(0, 0), (1, 5), (2, 2)]);
        assert_eq!(
            pairs(command_fds(Some(4), Some(7))),
            [(0, 4), (1, 7), (2, 2)]
        );
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod tests {
    use crate::testing::{Harness, Spawned};
````

with:

````rust
mod tests {
    use crate::Group;
    use crate::testing::{Harness, Spawned};
````

Replace:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                stdout: None,
                foreground: true,
            }],
````

with:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                stdin: None,
                stdout: None,
                group: Group::New,
            }],
````

Replace:

````rust
    }

    #[test]
    fn every_program_is_followed_by_a_sync() {
````

with:

````rust
    }

    /// `/bin/cat` exits with 0, `/bin/wc` with 0, `/bin/false` with 1.
    fn with_stages() -> Harness {
        let mut h = Harness::new();
        for (path, code) in [("/bin/cat", 0), ("/bin/wc", 0), ("/bin/false", 1)] {
            h.programs.known.push((path, WaitStatus::exited(code)));
        }
        h
    }

    fn stage(path: &str, args: &[&str], fds: (Option<u32>, Option<u32>), group: Group) -> Spawned {
        Spawned {
            path: path.into(),
            args: words(args),
            stdin: fds.0,
            stdout: fds.1,
            group,
        }
    }

    #[test]
    fn a_pipeline_s_stages_share_a_group_and_the_pipes_between_them() {
        let mut h = with_stages();
        assert_eq!(
            h.spawning("cat f | wc -l | false >> /tmp/o"),
            (1, String::new())
        );
        // The pipes, 4 and 5 then 6 and 7; the redirection, opened first.
        assert_eq!(h.programs.opened, [("/tmp/o".into(), true, 4)]);
        assert_eq!(h.programs.pipes, [(5, 6), (7, 8)]);
        let first = 101;
        assert_eq!(
            h.programs.spawned,
            [
                stage("/bin/cat", &["cat", "f"], (None, Some(6)), Group::New),
                stage(
                    "/bin/wc",
                    &["wc", "-l"],
                    (Some(5), Some(8)),
                    Group::Join(first)
                ),
                stage(
                    "/bin/false",
                    &["false"],
                    (Some(7), Some(4)),
                    Group::Join(first)
                ),
            ],
            "the first gets a group and the console, the others join it"
        );
        // Each end closed as soon as its stage has it.
        assert_eq!(h.programs.closed, [6, 5, 8, 7, 4]);
        // In a script, the script's group.
        let mut h = with_stages();
        h.put("/tmp/s.sh", b"cat f | wc\n");
        let mut out = crate::testing::FakeStdout::console();
        assert_eq!(h.sh(&["/tmp/s.sh"], &mut out).0, 0);
        let groups: Vec<Group> = h.programs.spawned.iter().map(|s| s.group).collect();
        assert_eq!(groups, [Group::Shell, Group::Shell]);
    }

    #[test]
    fn a_stage_that_cannot_start_says_so_and_the_others_run() {
        let mut h = with_stages();
        // As bash: `seq 5 | nosuch | wc -l` prints the error and 0.
        assert_eq!(
            h.spawning("cat f | nosuch | wc -l"),
            (0, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 2);
        assert_eq!(h.programs.pipes, [(4, 5), (6, 7)]);
        assert_eq!(
            h.programs.spawned[1].stdin,
            Some(6),
            "the second pipe's read end"
        );
        // Every end is closed all the same, so the others see their ends.
        let mut closed = h.programs.closed.clone();
        closed.sort();
        assert_eq!(closed, [4, 5, 6, 7]);
        // The first that starts leads the group.
        let mut h = with_stages();
        h.spawning("nosuch | cat | wc");
        assert_eq!(h.programs.spawned[0].group, Group::New);
        assert_eq!(h.programs.spawned[1].group, Group::Join(101));
        // The last one's refusal is the status.
        let mut h = with_stages();
        assert_eq!(
            h.spawning("cat | nosuch"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(
            h.spawning("cat | /no/such"),
            (
                127,
                "relay-sh: /no/such: No such file or directory\n".into()
            )
        );
    }

    #[test]
    fn a_pipeline_says_how_its_killed_stages_ended_and_ctrl_c_once() {
        let mut h = Harness::new();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1a2c);
        h.programs.known.push(("/bin/t-fault", fault));
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h.programs
            .known
            .push(("/bin/t-spin", WaitStatus::killed(KILLED_CTRL_C)));
        // A writer that ended because nobody read on: 141, said by nobody.
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(141)));
        assert_eq!(
            h.spawning("t-fault null-read | cat"),
            (
                0,
                "relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x401a2c)\n".into()
            )
        );
        assert_eq!(h.spawning("t-spin | t-spin | t-spin"), (130, "^C\n".into()));
        assert_eq!(h.spawning("t-args x | cat"), (0, String::new()));
        assert_eq!(h.spawning("cat | t-args x"), (141, String::new()));
    }

    #[test]
    fn a_pipeline_that_cannot_be_made_starts_no_more() {
        let mut h = with_stages();
        h.programs.pipe_error = Some((1, Errno::EMFILE));
        assert_eq!(
            h.spawning("cat | wc | false > /tmp/o"),
            (1, "relay-sh: pipe error: Too many open files\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 1, "only the first");
        let mut closed = h.programs.closed.clone();
        closed.sort();
        assert_eq!(closed, [4, 5, 6], "the redirection and both ends");
        // A redirection that cannot be opened starts nothing.
        let mut h = with_stages();
        h.programs.open_error = Some(Errno::EISDIR);
        assert_eq!(
            h.spawning("cat | wc > /tmp"),
            (1, "relay-sh: /tmp: Is a directory\n".into())
        );
        assert!(h.programs.spawned.is_empty() && h.programs.pipes.is_empty());
    }

    #[test]
    fn every_program_is_followed_by_a_sync() {
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert!(
            h.programs.spawned.iter().all(|s| !s.foreground),
            "a script's commands run in its group, so Ctrl-C ends it with them"
````

with:

````rust
        assert!(
            h.programs
                .spawned
                .iter()
                .all(|s| s.group == crate::Group::Shell),
            "a script's commands run in its group, so Ctrl-C ends it with them"
````

- [ ] **Step 4: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Bytes, Console, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Bytes, Console, Group, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
````

Replace:

````rust
    pub args: Vec<String>,
    /// The redirection's fd, as fd 1.
    pub stdout: Option<u32>,
    pub foreground: bool,
}
````

with:

````rust
    pub args: Vec<String>,
    /// What it got as fd 0 (a pipe's read end) and fd 1 (a pipe's write
    /// end or a redirection), instead of the shell's.
    pub stdin: Option<u32>,
    pub stdout: Option<u32>,
    pub group: Group,
}
````

Replace:

````rust
    pub open_error: Option<Errno>,
    pub closed: Vec<u32>,
````

with:

````rust
    pub open_error: Option<Errno>,
    /// Every pipe made, (read end, write end); what `pipe` fails with
    /// after this many, if anything.
    pub pipes: Vec<(u32, u32)>,
    pub pipe_error: Option<(usize, Errno)>,
    pub closed: Vec<u32>,
````

Replace:

````rust
            open_error: None,
            closed: Vec::new(),
````

with:

````rust
            open_error: None,
            pipes: Vec::new(),
            pipe_error: None,
            closed: Vec::new(),
````

Replace:

````rust
    }
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
````

with:

````rust
    }
    fn pipe(&mut self) -> Result<(u32, u32), Errno> {
        if let Some((after, e)) = self.pipe_error
            && self.pipes.len() >= after
        {
            return Err(e);
        }
        let ends = (self.next_fd + 1, self.next_fd + 2);
        self.next_fd += 2;
        self.pipes.push(ends);
        Ok(ends)
    }
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
    ) -> Result<u32, Errno> {
````

Replace:

````rust
                .collect(),
            stdout,
            foreground,
        });
````

with:

````rust
                .collect(),
            stdin,
            stdout,
            group,
        });
````

- [ ] **Step 5: Add the scenario `tests/e2e/pipes.txt`**

Create `tests/e2e/pipes.txt`:

````text
# Pipelines under /bin/sh (user-space gate §9.1, §12.3; milestone 3, plan
# 1): every stage a program in one process group, each one's output the
# next one's input. 8 MiB through cat and wc arrive whole; head ends a
# pipeline at once, its writer ending quietly; the console goes to the
# first stage in line mode, and Ctrl-C ends every stage; a script's
# pipeline is traced and its output in the transcript; and free shows the
# same memory in use before and after.
timeout 120
expect root@relay:~# $
# Every kernel-stack slot's page tables made first (they stay), so memory
# in use compares after them.
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
send cat l l l l l l l l l l l l l l l l > m
send cat m m m m > l
send cat l l l l l l l l l l l l l l l l > m
send cat m m m m m m m m m m m m m m m m > l
send cat l l l l l l l l > big
send cat big | wc -c
expect \n8388608\n
send cat big | cat | cat | wc -l
expect \n131072\n
# head has its line and ends; cat, writing to nobody, ends quietly.
send cat big | head -n 1
expect \n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\nroot@relay:~# $
# A first stage that ends before the next starts still leads the group.
send t-args first | cat
expect \n\[1\] first\nroot@relay:~# $
# Not the screen: one name a line; the last stage's redirection.
send ls /etc | cat > out
send cat out
expect \nhostname\nmotd\nroot@relay:~# $
send cat out nosuch | wc -l
expect \ncat: nosuch: No such file or directory\n2\n
send nosuch | wc -l
expect \nrelay-sh: nosuch: command not found\n0\n
send cd / | cat
expect \nrelay-sh: cd: cannot be used in a pipeline\n
# The first stage reads the console, a line at a time, to Ctrl-D.
send cat | wc -l
alive 1
key one
key two
type {ctrl-d}
expect \ntwo\n2\nroot@relay:~# $
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
send t-spin | cat | cat
alive 1
key {ctrl-c}
expect \^C\nroot@relay:~# 
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: Ctrl-C\npid \d+ \(/bin/cat\): killed: Ctrl-C\npid \d+ \(/bin/cat\): killed: Ctrl-C\n
# A script's pipeline, in the script's group; its transcript gets what
# the screen gets.
send echo 'echo piped | cat' > p.sh
send sh p.sh
expect \n\+ echo piped \| cat\npiped\nroot@relay:~# $
send cat p.log
expect \n\+ echo piped \| cat\npiped\nroot@relay:~# $
send rm big l m out p.sh p.log
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find `Group` in `crate` ``; `` unresolved import `crate::Group` ``.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: FAIL: scenario `pipes` stops at line 24, timed out waiting for `\n8388608\n`.

- [ ] **Step 7: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Programs, Stdin, Stdout, System};
use vfs::{Errno, Node};
````

with:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, Group, MemInfo, Programs, Stdin, Stdout, System};
use vfs::{Errno, Node};
````

Replace:

````rust

/// A command's fds: the shell's 0 and 2, and `stdout` or the shell's 1.
pub fn command_fds(stdout: Option<u32>) -> [FdMap; 3] {
    [(0, 0), (1, stdout.unwrap_or(1)), (2, 2)].map(|(child, parent)| FdMap { child, parent })
}
````

with:

````rust

/// A command's fds: `stdin` or the shell's 0, `stdout` or the shell's 1,
/// and the shell's 2.
pub fn command_fds(stdin: Option<u32>, stdout: Option<u32>) -> [FdMap; 3] {
    [(0, stdin.unwrap_or(0)), (1, stdout.unwrap_or(1)), (2, 2)]
        .map(|(child, parent)| FdMap { child, parent })
}
````

Replace:

````rust

    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
        let flags = if foreground && self.own_group {
            NEW_GROUP | FOREGROUND
        } else {
            // A command in the shell's own group reads the console in line
            // mode too, which is also where Ctrl-C ends it (and the group):
            // an interactive shell that leads no group left it raw at its
            // prompt.
            let _ = sys::console_mode(MODE_LINE);
            0
        };
        sys::spawn(path, &arg_bytes(args), b"", &command_fds(stdout), flags, 0)
            .map_err(Errno::from_number)
    }
````

with:

````rust

    fn pipe(&mut self) -> Result<(u32, u32), Errno> {
        sys::pipe().map_err(Errno::from_number)
    }

    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
    ) -> Result<u32, Errno> {
        let (flags, pgid) = match group {
            Group::New if self.own_group => (NEW_GROUP | FOREGROUND, 0),
            Group::Join(pgid) if self.own_group => (0, pgid),
            _ => {
                // A command in the shell's own group reads the console in
                // line mode too, which is also where Ctrl-C ends it (and
                // the group): an interactive shell that leads no group left
                // it raw at its prompt.
                let _ = sys::console_mode(MODE_LINE);
                (0, 0)
            }
        };
        let fds = command_fds(stdin, stdout);
        sys::spawn(path, &arg_bytes(args), b"", &fds, flags, pgid).map_err(Errno::from_number)
    }
````

- [ ] **Step 8: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Programs, for a shell that runs its commands as programs (`/bin/sh`,
````

with:

````rust

/// The process group a program starts in (user-space gate §6.4, §9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// The shell's own: a script's commands, so that Ctrl-C ends the
    /// script with them.
    Shell,
    /// A new one, which gets the console: a command at the prompt, or a
    /// pipeline's first.
    New,
    /// The group of the pipeline's first command (its pid), which has
    /// the console already.
    Join(u32),
}

/// Programs, for a shell that runs its commands as programs (`/bin/sh`,
````

Replace:

````rust
    fn close(&mut self, fd: u32);
    /// Starts the program at `path` with `args` (argument 0 first); its
    /// pid. It gets the shell's fds 0 and 2, and `stdout` (or the shell's
    /// fd 1) as its fd 1. With `foreground` it runs in a process group of
    /// its own, which gets the console (an interactive shell's command,
    /// spec §6.4); otherwise in the shell's group (a script's).
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno>;
````

with:

````rust
    fn close(&mut self, fd: u32);
    /// Makes a pipe: its read end and its write end.
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) in
    /// `group`; its pid. It gets `stdin` (or the shell's fd 0) as its fd 0,
    /// `stdout` (or the shell's fd 1) as its fd 1, and the shell's fd 2.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
    ) -> Result<u32, Errno>;
````

- [ ] **Step 9: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Bytes, Console, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command};
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Bytes, Console, Group, MemInfo, Programs, Stdin, Stdout, System};
pub use program::{CommandIo, run_command};
````

- [ ] **Step 10: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::ctx::Ctx;
use crate::io::{Bytes, Console, Programs, Stdin, Stdout, System};
use crate::killed;
````

with:

````rust
use crate::ctx::Ctx;
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
use crate::killed;
````

Replace:

````rust
        let path = program_path(name);
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, stdout, !parts.in_script);
        if let Some(fd) = stdout {
````

with:

````rust
        let path = program_path(name);
        let group = if parts.in_script {
            Group::Shell
        } else {
            Group::New
        };
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, None, stdout, group);
        if let Some(fd) = stdout {
````

Replace:

````rust

    fn pipeline(&mut self, _parts: Parts<'_>, _stages: &[Command]) -> Ran {
        Ran::said(
            crate::shell::SYNTAX,
            format!("{NAME}: unsupported syntax: |\n"),
        )
    }
````

with:

````rust

    /// Every stage is a program, started left to right in one process
    /// group (the first one's, which gets the console; a script's own),
    /// the pipe between two stages made just before the first of them
    /// starts. The shell closes its copies of a pipe's ends as soon as the
    /// stage that needs them has them, so that a reader sees the end of
    /// its input once its writer has ended, and a writer `EPIPE` once its
    /// reader has. A stage that cannot start says so at once and leaves
    /// its neighbours an end; the shell then waits for every stage, says
    /// how any killed one ended (a Ctrl-C once), and takes the last one's
    /// status.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        let (last, _) = stages.split_last().expect("a pipeline has stages");
        let last_out = match &last.redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
            None => None,
        };
        let (mut stdin, mut first, mut last_pid) = (None, None, None);
        let mut started = Vec::new();
        let mut ran = Ran::said(0, String::new());
        for (i, stage) in stages.iter().enumerate() {
            let is_last = i + 1 == stages.len();
            let (next, stdout) = if is_last {
                (None, last_out)
            } else {
                match self.programs.pipe() {
                    Ok((read, write)) => (Some(read), Some(write)),
                    Err(e) => {
                        for fd in [stdin, last_out].into_iter().flatten() {
                            self.programs.close(fd);
                        }
                        ran = Ran::said(1, format!("{NAME}: pipe error: {e}\n"));
                        break;
                    }
                }
            };
            let name = &stage.words[0];
            let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
            argv.extend(stage.words[1..].iter().map(|w| w.as_bytes()));
            let group = match (parts.in_script, first) {
                (true, _) => Group::Shell,
                (false, None) => Group::New,
                (false, Some(pgid)) => Group::Join(pgid),
            };
            let path = program_path(name);
            let pid = self
                .programs
                .spawn(path.as_bytes(), &argv, stdin, stdout, group);
            for fd in [stdin, stdout].into_iter().flatten() {
                self.programs.close(fd);
            }
            stdin = next;
            match pid {
                Ok(pid) => {
                    first.get_or_insert(pid);
                    started.push((name, pid));
                    if is_last {
                        last_pid = Some(pid);
                    }
                }
                Err(e) => {
                    let refused = cannot_start(name, e);
                    parts.console.write(refused.message.as_bytes());
                    if is_last {
                        ran.status = refused.status;
                    }
                }
            }
        }
        let mut cancelled = false;
        for (name, pid) in started {
            let ended = match self.programs.wait(pid) {
                Ok(w) => ended(name, &w),
                Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
            };
            if Some(pid) == last_pid {
                ran.status = ended.status;
            }
            if ended.status == CANCELLED {
                cancelled = true;
            } else {
                ran.message.push_str(&ended.message);
            }
        }
        if cancelled {
            ran.message.push_str("^C\n");
        }
        ran
    }
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 166 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 27 tests.

- [ ] **Step 12: Run the `pipes`, `sh`, `shell` scenarios**

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,relay-rt): run pipelines of programs in /bin/sh

/bin/sh runs a pipeline's stages as programs in one process group: the
first one's, which gets the console, or a script's own. It makes each
pipe just before the stage that writes it starts and closes its copies
of the ends as soon as the stages have them, so a reader sees its end
and a writer EPIPE. A stage that cannot start says so and the others
run; the shell waits for every stage, says how any killed one ended (a
Ctrl-C once) and takes the last one's status. Programs gains pipe, and
spawn a standard input and a Group instead of a foreground flag. The
pipes scenario runs 8 MiB through it, head ending its writer, typing
into a pipeline, Ctrl-C and a script's pipeline.
EOF
````


### Task 14: A command writes its output before it reads more input

The prototype's review (important, I2) and decision 5: output that is not the console waits in pieces of 4 KiB until the command ends (milestone 1's rule, for files), so a line typed into `cat | cat`, `cat | grep x` or `cat | head` reached the next stage only after Ctrl-D, while GNU's tools write each read at once. `Ctx::read_input` now writes what waits first, so a line reaches the next stage at Enter. The host test gives `cat` and `head -n 5` a line a read and logs every read and write in order; `pipes` types two lines into `cat | cat`. The red runs are that test, where both lines are read before anything is written, and `pipes`, which waits for the second `cat`'s line. Mutation check: without the write, `pipes` fails at the typed line.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/program.rs`
- Modify: `tests/e2e/pipes.txt`

**Interfaces:**
- Consumes: Task 8's `Ctx::read_input`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, replace:

````rust
    }

    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
````

with:

````rust
    }

    /// What a program did, in order: each read of standard input and each
    /// write of standard output.
    type Log = alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<String>>>;

    /// Standard input a line a read, as the console in line mode and a
    /// pipe from a writer that writes lines give it.
    struct Lines(Log, alloc::vec::Vec<&'static str>);

    impl crate::Stdin for Lines {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
            let line = if self.1.is_empty() {
                ""
            } else {
                self.1.remove(0)
            };
            self.0.borrow_mut().push(alloc::format!("read {line:?}"));
            buf[..line.len()].copy_from_slice(line.as_bytes());
            Ok(line.len())
        }
    }

    /// Standard output that is a pipe.
    struct Pipe(Log);

    impl crate::Stdout for Pipe {
        fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            let text = String::from_utf8_lossy(bytes);
            self.0.borrow_mut().push(alloc::format!("write {text:?}"));
            Ok(())
        }
        fn is_tty(&self) -> bool {
            false
        }
        fn node(&self) -> Option<vfs::Node> {
            None
        }
    }

    #[test]
    fn what_a_program_read_reaches_its_pipe_before_it_reads_again() {
        // `cat | cat`: the second gets each line as it is typed, not 4 KiB
        // later.
        for (name, run, args) in [
            ("cat", crate::commands::cat as crate::commands::Run, &[][..]),
            ("head", crate::commands::head, &["-n", "5"][..]),
        ] {
            let mut h = Harness::new();
            let log = Log::default();
            let args: alloc::vec::Vec<String> = args.iter().map(|a| String::from(*a)).collect();
            let io = crate::CommandIo {
                vfs: &mut h.vfs,
                console: &mut h.console,
                system: &mut h.system,
                stdin: &mut Lines(log.clone(), alloc::vec!["one\n", "two\n"]),
                stdout: &mut Pipe(log.clone()),
            };
            assert_eq!(crate::run_command(name, run, &args, io), 0);
            assert_eq!(
                *log.borrow(),
                [
                    "read \"one\\n\"",
                    "write \"one\\n\"",
                    "read \"two\\n\"",
                    "write \"two\\n\"",
                    "read \"\""
                ],
                "{name}"
            );
        }
    }

    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/pipes.txt`**

In `tests/e2e/pipes.txt`, replace:

````text
expect \ntwo\n2\nroot@relay:~# $
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
````

with:

````text
expect \ntwo\n2\nroot@relay:~# $
# And each line reaches the next stage when Enter ends it.
send cat | cat
alive 1
key first line
expect \nfirst line\nfirst line\n
key second
expect second\nsecond\n
type {ctrl-d}
expect root@relay:~# $
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `program::tests::what_a_program_read_reaches_its_pipe_before_it_reads_again`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: FAIL: scenario `pipes` stops at line 54, timed out waiting for `\nfirst line\nfirst line\n`.

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust

    /// Reads standard input into `buf`: how many bytes, 0 at its end.
    pub fn read_input(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        match &mut self.input {
````

with:

````rust

    /// Reads standard input into `buf`: how many bytes, 0 at its end. What
    /// waits for standard output is written first, so that what came of
    /// the last read reaches a pipe before the next one waits (a line typed
    /// into `cat | cat` reaches the second `cat` at Enter).
    pub fn read_input(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        self.streams().flush();
        match &mut self.input {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 167 tests.

- [ ] **Step 6: Run the `pipes` scenario**

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
fix(shell): write a command's output before it reads more input

Output that is not the console waited in 4 KiB pieces until the
command ended, so a line typed into `cat | cat`, `cat | grep x` or
`cat | head` reached the next stage only after Ctrl-D. A command now
writes what waits before each read of its standard input, as GNU's
tools do: a line reaches the next stage at Enter.

Refs: review I2
EOF
````


### Task 15: `sh` reading a pipe runs its lines and leaves the console alone

The prototype's review (important, I4) and decision 8: without arguments `/bin/sh` was always the interactive shell, which takes the console back in raw mode before every read, even when its fd 0 was a pipe. In `cat | sh` the console went raw with nobody reading it: `cat` read raw bytes, and Ctrl-C arrived as a byte, so nothing could end the pipeline but a reboot; `echo ls | sh` printed prompts and the echoed line. A shell whose fd 0 is no console (`SysStdin::is_console`, from `fstat`) now runs the lines it reads there (`Shell::run_input`), as bash does: as they come, without a prompt, a trace or the line editor, so it never sets the console's mode; its commands run in its group, with the console in line mode as before. It ends at the input's end or `exit`; a line over 64 KiB (`SCRIPT_MAX`, scripts' limit) or not UTF-8 is skipped with an `sh:` message. `pipes` runs `echo ls /etc | sh` and stops `t-spin | sh` with Ctrl-C. The red runs are the shell's tests, which cannot find `run_input`, and `pipes`, where the inner shell prints its prompts. Mutation checks: `/bin/sh` interactive on a pipe again fails `pipes`; `exit` not stopping, the last line without a newline dropped, and no limit on a line each fail the host test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `tests/e2e/pipes.txt`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: Task 8's `Stdin`, `SysStdin`; plan 4a's `Shell::spawning`.
- Produces: `Shell::run_input(&mut self, input: &mut dyn Stdin) -> i32`; `relay_rt::SysStdin::is_console() -> bool`; `commands::SCRIPT_MAX` re-exported.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn ctrl_c_stops_a_pipeline() {
````

with:

````rust
    #[test]
    fn a_shell_whose_input_is_no_console_runs_the_lines_it_reads() {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        let text = b"t-args a\n\nt-args 'b c'\n\xff\nexit 3\nt-args never\n".to_vec();
        let mut input = crate::Bytes::new(text);
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(status, 3, "exit's");
        let args: Vec<Vec<String>> = h.programs.spawned.iter().map(|s| s.args.clone()).collect();
        assert_eq!(args, [vec!["t-args", "a"], vec!["t-args", "b c"]]);
        // No prompt, no trace: only what is wrong.
        assert_eq!(h.console.take(), "sh: standard input: not a text line\n");
        assert!(
            h.console.input.is_empty()
                && h.programs
                    .spawned
                    .iter()
                    .all(|s| s.group == crate::Group::New)
        );
        // The last line needs no newline; a line over 64 KiB is skipped.
        let mut long = alloc::vec![b'x'; 70_000];
        long.extend_from_slice(b"\nt-args c");
        let mut input = crate::Bytes::new(long);
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            (status, h.console.take()),
            (0, "sh: standard input: a line over 64 KiB\n".into())
        );
        assert_eq!(h.programs.spawned.last().unwrap().args, ["t-args", "c"]);
    }

    #[test]
    fn ctrl_c_stops_a_pipeline() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/pipes.txt`**

In `tests/e2e/pipes.txt`, replace:

````text
expect root@relay:~# $
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
````

with:

````text
expect root@relay:~# $
# A shell reading a pipe runs what it reads, without a prompt, and leaves
# the console alone: Ctrl-C still ends the pipeline.
send echo ls /etc | sh
expect \nhostname  motd\nroot@relay:~# $
send t-spin | sh
alive 1
key {ctrl-c}
expect \^C\nroot@relay:~# 
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `run_input` found for struct `Shell<'a>` in the current scope ``.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: FAIL: scenario `pipes` stops at line 62, timed out waiting for `\nhostname  motd\nroot@relay:~# $`.

- [ ] **Step 4: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
pub struct SysStdin;

````

with:

````rust
pub struct SysStdin;

impl SysStdin {
    /// Whether fd 0 is the console (`fstat` says a character device).
    pub fn is_console() -> bool {
        sys::fstat(0).is_ok_and(|st| st.kind == u32::from(KIND_CHAR_DEVICE))
    }
}

````

- [ ] **Step 5: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
pub(crate) use script::Script;
pub use script::transcript_name;
pub use stat::stat;
````

with:

````rust
pub(crate) use script::Script;
pub use script::{SCRIPT_MAX, transcript_name};
pub use stat::stat;
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use crate::commands::{self, Script};
use crate::ctx::{Ctx, quote_if_needed};
````

with:

````rust

use crate::commands::{self, SCRIPT_MAX, Script};
use crate::ctx::{Ctx, quote_if_needed};
````

Replace:

````rust
        }
        status
    }

````

with:

````rust
        }
        status
    }

    /// `X | sh`: a shell whose standard input is no console runs the
    /// commands it reads there (user-space gate §9.1, §16 item 8), a line
    /// at a time as they come, without a prompt, a trace or the line
    /// editor, so it never takes the console; it ends at the input's end or
    /// `exit`. A line over 64 KiB, or not UTF-8, is skipped with a message,
    /// as `sh` refuses such a script. Returns the last status.
    pub fn run_input(&mut self, input: &mut dyn Stdin) -> i32 {
        let mut buf = alloc::vec![0; 4096];
        let mut line: Vec<u8> = Vec::new();
        let mut too_long = false;
        self.stopped = false;
        loop {
            let n = match input.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    let message = format!("sh: standard input: {e}\n");
                    return self.finish(1, message);
                }
            };
            if n == 0 {
                if !line.is_empty() || too_long {
                    self.input_line(&line, too_long);
                }
                return self.status;
            }
            for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
                let ended = piece.last() == Some(&b'\n');
                let piece = &piece[..piece.len() - usize::from(ended)];
                if !too_long {
                    line.extend_from_slice(piece);
                    if line.len() as u64 > SCRIPT_MAX {
                        too_long = true;
                        line.clear();
                    }
                }
                if ended {
                    self.input_line(&line, too_long);
                    line.clear();
                    too_long = false;
                    if self.stopped {
                        return self.status;
                    }
                }
            }
        }
    }

    /// One line `run_input` read: run, or said why not.
    fn input_line(&mut self, line: &[u8], too_long: bool) {
        match core::str::from_utf8(line) {
            _ if too_long => {
                self.finish(1, String::from("sh: standard input: a line over 64 KiB\n"));
            }
            Ok(text) => {
                self.execute(text);
            }
            Err(_) => {
                self.finish(1, String::from("sh: standard input: not a text line\n"));
            }
        }
    }

````

- [ ] **Step 7: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! arguments it is the interactive shell, which takes the console at its
//! prompt and gives it to each command it starts; `sh FILE` runs a script,
//! its commands in the shell's own process group and its transcript a
//! console tee. Every command but `cd`, `exit` and `help` is a program.
#![no_std]
#![no_main]

use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdout, SysSystem, SysVfs, sys};
use shell::Shell;
````

with:

````rust
//! arguments it is the interactive shell, which takes the console at its
//! prompt and gives it to each command it starts, unless its standard input
//! is no console (`X | sh`): then it runs the lines it reads there and
//! leaves the console alone. `sh FILE` runs a script, its commands in the
//! shell's own process group and its transcript a console tee. Every
//! command but `cd`, `exit` and `help` is a program.
#![no_std]
#![no_main]

use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdin, SysStdout, SysSystem, SysVfs, sys};
use shell::Shell;
````

Replace:

````rust
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell leads a process group of its own when it
````

with:

````rust
    let words = words(&args);
    if words.is_empty() && !SysStdin::is_console() {
        // In a pipeline, in its group: commands read from the pipe, and the
        // console stays with the group, in line mode, so Ctrl-C ends them.
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_input(&mut SysStdin) as u8
    } else if words.is_empty() {
        // An interactive shell leads a process group of its own when it
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 168 tests.

- [ ] **Step 9: Run the `pipes`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates tests userland
git commit -F - <<'EOF'
fix(sh,shell): run the lines a pipe gives sh instead of taking the console

Without arguments /bin/sh was always the interactive shell, which sets
the console to raw mode before every read, even when its fd 0 was a
pipe: in `cat | sh` the console went raw with nobody reading it, and
Ctrl-C could no longer end the pipeline. A shell whose fd 0 is no
console now runs the lines it reads there, as bash does, without a
prompt or the line editor, and leaves the console alone (relay-rt's
SysStdin::is_console tells).

Refs: review I4
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 43 scenario(s) passed`.

````bash
git push -u origin m3p1/shell
gh pr create --base main --head m3p1/shell --title "feat(shell,relay-rt): run pipelines and read standard input" --body-file - <<'EOF'
## What

Milestone 3, plan 1, tasks 7–15: the parser reads `a | b | c` with bash's syntax errors, every command with a name; commands get standard input (a program's fd 0, or bytes in memory in the in-process runner), and `cat`, `wc`, `head` and `tail` read it without a file, writing their output before each read; `wc` gains `-c`, `-l` and `-w` with GNU's widths (compared with the host's); pipelines run in the in-process runner, and in `/bin/sh` as programs in one process group, the shell closing its pipe ends at once, reporting stages that cannot start or were killed (Ctrl-C once) and taking the last stage's status; `cd`, `exit` and `help` cannot be in a pipeline; `X | sh` runs the lines it reads and leaves the console alone. Scenario `pipes` (new) and `console`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types into a pipeline); plan 4's NUC check 5 runs pipes there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p1/shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `grep`, `seq`, `sleep`, `true` and `false` (Tasks 16–23)

The new programs of spec §9.1, each a command function compared with GNU's, and their place in `/bin`; with the review's fixes: `grep` never reads its own output and refuses GNU's invalid ranges, `-` is standard input, and `seq` says what GNU's says.

Branch `m3p1/utils`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-utils`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p1/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-utils origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-utils
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p1/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p1/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-utils m3p1/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p1/shell>` and re-run `cargo xtask ci` before pushing.

### Task 16: `grep`

Spec §9.1 and decision 9: `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` prints the lines that match, as GNU grep does with `LC_ALL=C`. The matcher (`crates/shell/src/pattern.rs`) takes literal bytes, `.`, `*` (literal at the start), `^` at the start and `$` at the end (literal elsewhere), and `[...]` with ranges, `^` and a leading `]`; `\` before a byte is that byte; `-i` folds ASCII letters. GNU's other escapes and a set's classes are refused (`grep: \( \) is not supported`, status 2) rather than matched differently, and a malformed pattern gets GNU's message. It runs every item at once over the line (an NFA), in time proportional to the line times the pattern. The command (`commands/grep.rs`) reads files or standard input in pieces, puts each file's name first when there are several, says `grep: f: binary file matches` instead of printing a selected line of an input holding a NUL byte, and returns 0, 1 or 2 as GNU's. The tests run every pattern of up to three symbols of a small alphabet (1885) and the `-i` ones through the host's GNU grep (the same lines or the same refusal), and compare the command's output, messages and status on 28 command lines; this caught two refusals that said something else than GNU's. The matcher and the command are one task, since the matcher alone would be dead code. The red run is the shell's tests, which cannot find `Pattern`. Mutation checks: the start anchor, `*` matching nothing, the end anchor, a negated set under `-i`, `]` first, a range ending in `]`, binary detection, the file names, the last line without its newline, status 2 and `-v` each fail a test.

**Files:**
- Create: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/lib.rs`
- Create: `crates/shell/src/pattern.rs`

**Interfaces:**
- Consumes: Task 8's `Ctx::read_input`; Task 9's `host_tool`, `like_host`.
- Produces: `pattern::{Pattern::{parse(p: &[u8], ignore_case: bool) -> Result<Pattern, PatternError>, is_match(&self, line: &[u8]) -> bool}, PatternError}`; `commands::grep` in the command table.

- [ ] **Step 1: Write the failing tests for `crates/shell/src/commands/grep.rs`**

Create `crates/shell/src/commands/grep.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::{Harness, host_tool};

    #[test]
    fn grep_prints_what_gnu_grep_prints() {
        let files: &[(&str, &[u8])] = &[
            ("t", b"abc\nxyz\nABC\nb\n"),
            ("u", b"one b\nno newline at the end"),
            ("bin", b"a\0b\nabc\n"),
            ("d/", b""),
        ];
        let cases: &[(&[&str], &[u8])] = &[
            (&["grep", "b", "t"], b""),
            (&["grep", "-i", "abc", "t"], b""),
            (&["grep", "-v", "b", "t"], b""),
            (&["grep", "-n", "b", "t"], b""),
            (&["grep", "-c", "b", "t"], b""),
            (&["grep", "-c", "b", "t", "u"], b""),
            (&["grep", "b", "t", "u"], b""),
            (&["grep", "-vn", "x", "t", "u"], b""),
            (&["grep", "-nc", "end$", "u"], b""),
            (&["grep", "end$", "u"], b""),
            (&["grep", "zzz", "t"], b""),
            (&["grep", "a", "bin"], b""),
            (&["grep", "-c", "a", "bin"], b""),
            (&["grep", "-v", "q", "bin"], b""),
            (&["grep", "a", "t", "bin"], b""),
            (&["grep", "x", "nope", "t"], b""),
            (&["grep", "x", "d"], b""),
            (&["grep", "[a", "t"], b""),
            (&["grep", "a\\", "t"], b""),
            (&["grep"], b""),
            (&["grep", "-j", "a", "t"], b""),
            (&["grep", "b"], b"abc\nb\nzzz"),
            (&["grep", "-c", "."], b"one\n\ntwo\n"),
            (&["grep", "-c", "."], b""),
            (&["grep", "-n", "x"], b"a\nx\nyx"),
            (&["grep", "a"], b"x\na\0\n"),
            (&["grep", "-v", "a"], b"a\nb\n"),
            (&["grep", "-", "t"], b""),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, files, stdin),
                host_tool(args, files, stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn a_long_input_is_searched_in_pieces_with_its_lines_whole() {
        let mut h = Harness::new();
        // Lines across the 64 KiB pieces of the read.
        let text: String = (0..40_000).map(|i| alloc::format!("line {i}\n")).collect();
        h.put("/tmp/big", text.as_bytes());
        assert_eq!(h.run("grep -c '9$' /tmp/big"), (0, "4000\n".into()));
        assert_eq!(
            h.run("grep '^line.39999$' /tmp/big"),
            (0, "line 39999\n".into())
        );
        h.console.interrupt = true;
        assert_eq!(h.run("grep x /tmp/big"), (130, "^C\n".into()));
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
mod change;
mod ls;
````

with:

````rust
mod change;
mod grep;
mod ls;
````

- [ ] **Step 3: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub mod parser;
mod program;
````

with:

````rust
pub mod parser;
mod pattern;
mod program;
````

- [ ] **Step 4: Write the failing tests for `crates/shell/src/pattern.rs`**

Create `crates/shell/src/pattern.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    fn m(p: &str, line: &str) -> bool {
        Pattern::parse(p.as_bytes(), false)
            .unwrap()
            .is_match(line.as_bytes())
    }

    #[test]
    fn literal_bytes_match_anywhere_in_the_line() {
        assert!(m("bc", "abcd"));
        assert!(!m("bd", "abcd"));
        assert!(m("", "anything"), "the empty pattern matches every line");
        assert!(m("", ""));
        assert!(!m("a", ""));
    }

    #[test]
    fn dot_star_and_anchors() {
        assert!(m("a.c", "xabcx"));
        assert!(!m("a.c", "ac"));
        assert!(m("ab*c", "ac") && m("ab*c", "abbbc") && !m("ab*c", "adc"));
        assert!(m("^ab", "abc") && !m("^ab", "cab"));
        assert!(m("bc$", "abc") && !m("bc$", "bca"));
        assert!(m("^$", "") && !m("^$", "x"));
        assert!(m("^a.*z$", "a to z") && !m("^a.*z$", "a to z!"));
        // Literal where they are not special.
        assert!(m("*a", "x*a") && !m("*a", "xa"));
        assert!(m("a^b", "a^b") && m("a$b", "a$b"));
        assert!(m("^*", "*x"), "a star after the anchor is literal");
        assert!(m("a**", "b"), "a second star changes nothing");
    }

    #[test]
    fn sets_with_ranges_and_complements() {
        assert!(m("[abc]x", "bx") && !m("[abc]x", "dx"));
        assert!(m("[a-c0-9]", "7") && !m("[a-c0-9]", "z"));
        assert!(m("[^a-c]", "abcd") && !m("[^a-c]", "abc"));
        assert!(m("[]a]", "]") && m("[^]a]", "b") && !m("[^]a]", "]"));
        assert!(m("[a-]", "-") && m("[-a]", "-"));
        assert!(
            m("[.*]", "*") && !m("[.*]", "x"),
            "special characters are members"
        );
        assert!(m("[\\]", "\\"), "a backslash is a member");
        assert!(m("x[0-9]*y", "xy") && m("x[0-9]*y", "x123y"));
    }

    #[test]
    fn a_backslash_makes_the_next_byte_literal() {
        assert!(m("a\\.c", "a.c") && !m("a\\.c", "abc"));
        assert!(m("\\*", "*") && m("\\[", "[") && m("\\^a", "^a") && m("a\\$", "a$x"));
        assert!(m("\\\\", "\\"));
    }

    #[test]
    fn ignoring_case_folds_ascii_letters_only() {
        let p = Pattern::parse(b"ab[c-d][^x]", true).unwrap();
        assert!(p.is_match(b"ABDy") && p.is_match(b"aBcY"));
        assert!(!p.is_match(b"ABDX"), "x, either case, is outside the set");
        let p = Pattern::parse("é".as_bytes(), true).unwrap();
        assert!(
            !p.is_match("É".as_bytes()),
            "bytes beyond ASCII are themselves"
        );
    }

    #[test]
    fn what_is_refused_says_why_as_gnu_grep_does() {
        let e = |p: &str| Pattern::parse(p.as_bytes(), false).unwrap_err().to_string();
        assert_eq!(e("[ab"), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("[]"), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("a["), "Invalid regular expression");
        assert_eq!(e("[^"), "Invalid regular expression");
        assert_eq!(e("[[."), "Unmatched [, [^, [:, [., or [=");
        assert_eq!(e("ab\\"), "Trailing backslash");
        assert_eq!(e("[b-a]"), "Invalid range end");
        for p in [
            "\\(a\\)",
            "a\\{2\\}",
            "a\\|b",
            "a\\+",
            "a\\?",
            "\\<a",
            "\\w",
            "\\1",
            "[[:alpha:]]",
            "[[.a.]]",
        ] {
            assert!(e(p).ends_with("is not supported"), "{p}");
        }
    }

    #[test]
    fn long_lines_and_many_stars_take_no_backtracking() {
        let line = vec![b'a'; 100_000];
        let p = Pattern::parse(b"a*a*a*a*a*a*a*a*b", false).unwrap();
        assert!(!p.is_match(&line));
        let mut line = line;
        line.push(b'b');
        assert!(p.is_match(&line));
    }

    /// Every pattern of up to `symbols` of `alphabet`, the empty one first.
    fn every_pattern(alphabet: &[&str], symbols: usize) -> Vec<String> {
        let mut patterns: Vec<String> = vec![String::new()];
        let mut last = vec![String::new()];
        for _ in 0..symbols {
            let mut longer = Vec::new();
            for p in &last {
                for s in alphabet {
                    longer.push(alloc::format!("{p}{s}"));
                }
            }
            patterns.extend(longer.iter().cloned());
            last = longer;
        }
        patterns
    }

    /// The patterns whose lines (or refusal) differ from GNU grep's
    /// (`LC_ALL=C`) over `lines`.
    fn unlike_gnu(patterns: &[String], lines: &[&str], ignore_case: bool) -> Vec<String> {
        let input: String = lines.iter().map(|l| alloc::format!("{l}\n")).collect();
        let mut differ = Vec::new();
        for p in patterns {
            let ours = match Pattern::parse(p.as_bytes(), ignore_case) {
                Ok(pat) => {
                    let got: Vec<&str> = lines
                        .iter()
                        .copied()
                        .filter(|l| pat.is_match(l.as_bytes()))
                        .collect();
                    Ok(got)
                }
                Err(e) => Err(e),
            };
            let args: &[&str] = if ignore_case {
                &["grep", "-i", "--", p]
            } else {
                &["grep", "--", p]
            };
            let (status, out, err) = crate::testing::host_tool(args, &[], input.as_bytes());
            let theirs: Vec<&str> = out.lines().collect();
            let same = match (&ours, status) {
                (Ok(got), 0 | 1) => *got == theirs,
                (Err(e), 2) => err == alloc::format!("grep: {e}\n"),
                _ => false,
            };
            if !same {
                differ.push(alloc::format!(
                    "{p:?}: ours {ours:?}, grep's {status} {theirs:?} {err:?}"
                ));
            }
        }
        differ
    }

    /// Every pattern of a small alphabet against lines of another, and what
    /// GNU grep says of each: the same lines, or the same refusal.
    #[test]
    fn it_matches_what_gnu_grep_matches() {
        let lines = [
            "", "a", "b", "ab", "ba", "aab", "abb", "a.b", "a*b", "[a]", "^a", "a$", "-", "]",
            "x y", "abab",
        ];
        let alphabet = [
            "a", "b", ".", "*", "^", "$", "[", "]", "-", "\\.", "[^a]", "[a-b]",
        ];
        // Up to three symbols: 1 + 12 + 144 + 1728 patterns.
        let patterns = every_pattern(&alphabet, 3);
        let differ = unlike_gnu(&patterns, &lines, false);
        assert!(
            differ.is_empty(),
            "{} of {}:\n{}",
            differ.len(),
            patterns.len(),
            differ.join("\n")
        );
    }

    /// The same with `-i`, over lines in either case.
    #[test]
    fn it_ignores_case_as_gnu_grep_does() {
        let lines = ["", "a", "A", "Ab", "aB", "AB", "b", "Z", "ba", "B.A", "_"];
        let alphabet = ["a", "B", ".", "*", "^", "[^a]", "[A-b]", "[b-z]", "[^B-Z]"];
        let patterns = every_pattern(&alphabet, 2);
        let differ = unlike_gnu(&patterns, &lines, true);
        assert!(
            differ.is_empty(),
            "{} of {}:\n{}",
            differ.len(),
            patterns.len(),
            differ.join("\n")
        );
    }
}
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Pattern` in this scope ``.

- [ ] **Step 6: Implement `crates/shell/src/commands/grep.rs`**

Insert this at the top of `crates/shell/src/commands/grep.rs`, above `#[cfg(test)]`:

````rust
//! `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` (user-space gate §9.1):
//! the lines of each file, or of standard input, that hold a match of
//! `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`.
//! Several files put each one's name before its lines; `-i` ignores case,
//! `-v` selects the lines that do not match, `-n` numbers them, `-c` counts
//! them instead. An input that holds a NUL byte is binary: once one of its
//! lines is selected, grep says so (`grep: f: binary file matches`)
//! instead of printing it. The status is 0 if a line was selected, 1 if
//! none was, and 2 after an error.

use crate::ctx::{Ctx, getopt};
use crate::pattern::Pattern;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType};

/// Input is read in pieces of this size.
const CHUNK: usize = 64 * 1024;

/// What `-n`, `-v`, `-c` and the number of inputs ask for.
struct Options {
    invert: bool,
    number: bool,
    count: bool,
    /// Each line starts with its input's name.
    names: bool,
}

/// How an input went.
enum Outcome {
    /// Whether a line was selected.
    Read(bool),
    Failed,
}

pub fn grep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "cinv", "") {
        Ok(o) => o,
        Err(e) => return usage(ctx, Some(format!("{e}"))),
    };
    let Some((pattern, files)) = opts.operands.split_first() else {
        return usage(ctx, None);
    };
    let pattern = match Pattern::parse(pattern.as_bytes(), opts.has('i')) {
        Ok(p) => p,
        Err(e) => {
            ctx.fail("grep", format_args!("{e}"));
            return 2;
        }
    };
    let options = Options {
        invert: opts.has('v'),
        number: opts.has('n'),
        count: opts.has('c'),
        names: files.len() > 1,
    };
    let mut outcomes = Vec::new();
    if files.is_empty() {
        outcomes.push(search(ctx, &pattern, &options, None));
    }
    for file in files {
        if ctx.out_failed() || ctx.interrupted() {
            break;
        }
        outcomes.push(search(ctx, &pattern, &options, Some(file)));
    }
    if outcomes.iter().any(|o| matches!(o, Outcome::Failed)) {
        2
    } else if outcomes.iter().any(|o| matches!(o, Outcome::Read(true))) {
        0
    } else {
        1
    }
}

/// GNU's two lines for a command line it cannot use; status 2.
fn usage(ctx: &mut Ctx<'_>, error: Option<String>) -> i32 {
    if let Some(e) = error {
        ctx.err(format!("grep: {e}\n").as_bytes());
    }
    ctx.err(
        b"Usage: grep [OPTION]... PATTERNS [FILE]...\nTry 'grep --help' for more information.\n",
    );
    2
}

/// Searches one input: the file `file`, or standard input.
fn search(
    ctx: &mut Ctx<'_>,
    pattern: &Pattern,
    options: &Options,
    file: Option<&String>,
) -> Outcome {
    let name = file.map_or("(standard input)", String::as_str);
    let mut source = match file {
        Some(path) => match open(ctx, path) {
            Ok(node) => Source::File { node, offset: 0 },
            Err(e) => {
                ctx.fail("grep", format_args!("{path}: {e}"));
                return Outcome::Failed;
            }
        },
        None => Source::Input,
    };
    let mut lines = Lines {
        pattern,
        options,
        name,
        number: 0,
        selected: 0,
        binary: false,
    };
    let mut buf = vec![0; CHUNK];
    let mut line: Vec<u8> = Vec::new();
    loop {
        if ctx.interrupted() || ctx.out_failed() {
            return Outcome::Read(lines.selected > 0);
        }
        let n = match source.read(ctx, &mut buf) {
            Ok(n) => n,
            Err(e) => {
                ctx.fail("grep", format_args!("{name}: {e}"));
                return Outcome::Failed;
            }
        };
        if n == 0 {
            // What is left is the last line, without its newline.
            if !line.is_empty() && !lines.take(ctx, &line) {
                return Outcome::Read(true);
            }
            break;
        }
        lines.binary |= buf[..n].contains(&0);
        for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
            line.extend_from_slice(piece);
            if line.pop_if(|b| *b == b'\n').is_some() {
                if !lines.take(ctx, &line) {
                    return Outcome::Read(true);
                }
                line.clear();
            }
        }
    }
    if options.count {
        let prefix = if options.names {
            format!("{name}:")
        } else {
            String::new()
        };
        ctx.out(format!("{prefix}{}\n", lines.selected).as_bytes());
    }
    Outcome::Read(lines.selected > 0)
}

/// An input's lines, as they come.
struct Lines<'p> {
    pattern: &'p Pattern,
    options: &'p Options,
    name: &'p str,
    /// Lines so far, and those selected.
    number: u64,
    selected: u64,
    /// A NUL byte has come.
    binary: bool,
}

impl Lines<'_> {
    /// The next line, without its newline: printed if it is selected.
    /// False once a binary input has had a line selected, which is said
    /// instead, and the rest of the input is not read.
    fn take(&mut self, ctx: &mut Ctx<'_>, line: &[u8]) -> bool {
        self.number += 1;
        if self.pattern.is_match(line) == self.options.invert {
            return true;
        }
        self.selected += 1;
        if self.options.count {
            return true;
        }
        if self.binary {
            ctx.err(format!("grep: {}: binary file matches\n", self.name).as_bytes());
            return false;
        }
        print(ctx, self.options, self.name, self.number, line);
        true
    }
}

/// A selected line, with its input's name and number when asked for.
fn print(ctx: &mut Ctx<'_>, options: &Options, name: &str, number: u64, line: &[u8]) {
    let mut out = Vec::with_capacity(line.len() + name.len() + 24);
    if options.names {
        out.extend_from_slice(name.as_bytes());
        out.push(b':');
    }
    if options.number {
        out.extend_from_slice(format!("{number}:").as_bytes());
    }
    out.extend_from_slice(line);
    out.push(b'\n');
    ctx.out(&out);
}

/// A file to read: a directory is GNU's `Is a directory`.
fn open(ctx: &mut Ctx<'_>, path: &str) -> Result<vfs::Node, Errno> {
    let node = ctx.vfs.lookup(path.as_bytes())?;
    if ctx.vfs.stat(node)?.kind == FileType::Directory {
        return Err(Errno::EISDIR);
    }
    Ok(node)
}

/// Where the lines come from.
enum Source {
    File { node: vfs::Node, offset: u64 },
    Input,
}

impl Source {
    /// The next piece; 0 at the end.
    fn read(&mut self, ctx: &mut Ctx<'_>, buf: &mut [u8]) -> Result<usize, Errno> {
        match self {
            Source::File { node, offset } => {
                let n = ctx.vfs.read_at(*node, *offset, buf)?;
                *offset += n as u64;
                Ok(n)
            }
            Source::Input => ctx.read_input(buf),
        }
    }
}

````

- [ ] **Step 7: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use ls::ls;
````

with:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use grep::grep;
pub use ls::ls;
````

Replace:

````rust
        run: system::free,
    },
````

with:

````rust
        run: system::free,
    },
    Builtin {
        name: "grep",
        help: "print the lines that match a pattern",
        run: grep::grep,
    },
````

- [ ] **Step 8: Implement `crates/shell/src/pattern.rs`**

Insert this at the top of `crates/shell/src/pattern.rs`, above `#[cfg(test)]`:

````rust
//! `grep`'s patterns (user-space gate §9.1): a subset of POSIX basic
//! regular expressions, over bytes as GNU grep does with `LC_ALL=C`.
//!
//! A pattern is literal bytes, `.` (any byte), `*` (the item before it any
//! number of times; literal at the start), `^` at the start and `$` at the
//! end (anchors; literal elsewhere), and `[...]` (a set of bytes, with
//! ranges, `^` first for its complement, and `]` first as a member). A `\`
//! makes the next byte literal; the escapes that are something else in
//! GNU's basic expressions (groups, intervals, alternation, word and
//! back-references) are refused, as are character classes in a set, so
//! that no pattern quietly matches what GNU grep would not.
//!
//! Matching runs every item at once over the line (an NFA simulation), so
//! it takes time in proportion to the line's length times the pattern's,
//! whatever the pattern.

use alloc::vec::Vec;
use core::fmt;

/// One position of a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Byte(u8),
    Any,
    /// Inclusive byte ranges; `negated` matches every byte outside them.
    Set {
        ranges: Vec<(u8, u8)>,
        negated: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Piece {
    item: Item,
    /// Followed by `*`.
    repeated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pieces: Vec<Piece>,
    at_start: bool,
    at_end: bool,
    ignore_case: bool,
}

/// Why a pattern is refused; the messages are GNU grep's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// A `[` or `[^` with nothing after it.
    Invalid,
    UnmatchedBracket,
    TrailingBackslash,
    InvalidRangeEnd,
    /// A GNU feature this grep does not have, as written.
    Unsupported(&'static str),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternError::Invalid => f.write_str("Invalid regular expression"),
            PatternError::UnmatchedBracket => f.write_str("Unmatched [, [^, [:, [., or [="),
            PatternError::TrailingBackslash => f.write_str("Trailing backslash"),
            PatternError::InvalidRangeEnd => f.write_str("Invalid range end"),
            PatternError::Unsupported(what) => write!(f, "{what} is not supported"),
        }
    }
}

/// The escapes GNU's basic expressions give a meaning, which this grep
/// does not have.
fn unsupported_escape(b: u8) -> Option<&'static str> {
    Some(match b {
        b'(' | b')' => "\\( \\)",
        b'{' | b'}' => "\\{ \\}",
        b'|' => "\\|",
        b'+' => "\\+",
        b'?' => "\\?",
        b'<' | b'>' | b'b' | b'B' | b'w' | b'W' | b's' | b'S' | b'`' | b'\'' => {
            "\\<, \\b, \\w and the like"
        }
        b'1'..=b'9' => "a back-reference",
        _ => return None,
    })
}

impl Pattern {
    /// The pattern `p`; with `ignore_case`, ASCII letters match either case.
    pub fn parse(p: &[u8], ignore_case: bool) -> Result<Pattern, PatternError> {
        let mut pieces: Vec<Piece> = Vec::new();
        let at_start = p.first() == Some(&b'^');
        let mut i = usize::from(at_start);
        let mut at_end = false;
        while i < p.len() {
            let b = p[i];
            i += 1;
            let item = match b {
                b'\\' => {
                    let Some(&c) = p.get(i) else {
                        return Err(PatternError::TrailingBackslash);
                    };
                    i += 1;
                    if let Some(what) = unsupported_escape(c) {
                        return Err(PatternError::Unsupported(what));
                    }
                    Item::Byte(c)
                }
                b'.' => Item::Any,
                // `*` repeats the piece before it; at the start it is
                // literal, and a second one changes nothing.
                b'*' => match pieces.last_mut() {
                    Some(last) => {
                        last.repeated = true;
                        continue;
                    }
                    None => Item::Byte(b'*'),
                },
                b'$' if i == p.len() => {
                    at_end = true;
                    continue;
                }
                b'[' => {
                    let (set, next) = parse_set(p, i)?;
                    i = next;
                    set
                }
                _ => Item::Byte(b),
            };
            pieces.push(Piece {
                item,
                repeated: false,
            });
        }
        Ok(Pattern {
            pieces,
            at_start,
            at_end,
            ignore_case,
        })
    }

    /// Whether `line` (without its newline) holds a match.
    pub fn is_match(&self, line: &[u8]) -> bool {
        let n = self.pieces.len();
        // The states are how many pieces have matched; a repeated piece
        // may also match nothing.
        let mut now = alloc::vec![false; n + 1];
        let mut next = alloc::vec![false; n + 1];
        self.enter(&mut now, 0);
        for &c in line {
            if now[n] && !self.at_end {
                return true;
            }
            next.fill(false);
            for (k, piece) in self.pieces.iter().enumerate() {
                if now[k] && self.matches(&piece.item, c) {
                    let to = if piece.repeated { k } else { k + 1 };
                    self.enter(&mut next, to);
                }
            }
            if !self.at_start {
                self.enter(&mut next, 0);
            }
            core::mem::swap(&mut now, &mut next);
        }
        now[n]
    }

    /// Adds state `k`, and those a repeated piece lets it skip to.
    fn enter(&self, states: &mut [bool], mut k: usize) {
        loop {
            states[k] = true;
            match self.pieces.get(k) {
                Some(p) if p.repeated => k += 1,
                _ => return,
            }
        }
    }

    fn matches(&self, item: &Item, c: u8) -> bool {
        let one = |c: u8| match item {
            Item::Byte(b) => *b == c,
            Item::Any => true,
            Item::Set { ranges, negated } => {
                ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != *negated
            }
        };
        if !self.ignore_case || !c.is_ascii_alphabetic() {
            return one(c);
        }
        match item {
            // A negated set matches a letter only if neither case is in it.
            Item::Set { negated: true, .. } => {
                one(c.to_ascii_lowercase()) && one(c.to_ascii_uppercase())
            }
            _ => one(c.to_ascii_lowercase()) || one(c.to_ascii_uppercase()),
        }
    }
}

/// The set that starts after the `[` at `p[i - 1]`, and where the pattern
/// goes on after its `]`.
fn parse_set(p: &[u8], mut i: usize) -> Result<(Item, usize), PatternError> {
    let negated = p.get(i) == Some(&b'^');
    i += usize::from(negated);
    if i == p.len() {
        return Err(PatternError::Invalid);
    }
    let mut ranges = Vec::new();
    let mut first = true;
    loop {
        let Some(&b) = p.get(i) else {
            return Err(PatternError::UnmatchedBracket);
        };
        if b == b']' && !first {
            return Ok((Item::Set { ranges, negated }, i + 1));
        }
        if b == b'['
            && let Some(&kind @ (b':' | b'.' | b'=')) = p.get(i + 1)
        {
            // GNU's classes and collating elements: refused, once closed.
            let closed = p[i + 2..].windows(2).any(|w| w == [kind, b']']);
            return Err(if closed {
                PatternError::Unsupported("[: :], [. .] and [= =] in a set")
            } else {
                PatternError::UnmatchedBracket
            });
        }
        first = false;
        // `a-z`, unless the `-` is the set's last.
        if p.get(i + 1) == Some(&b'-') && p.get(i + 2).is_some_and(|&e| e != b']') {
            let end = p[i + 2];
            if end < b {
                return Err(PatternError::InvalidRangeEnd);
            }
            ranges.push((b, end));
            i += 3;
        } else {
            ranges.push((b, b));
            i += 1;
        }
    }
}

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 179 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add grep and its pattern matcher

grep [-i] [-v] [-n] [-c] PATTERN [FILE...] prints the lines that
match (spec §9.1): literal bytes, ., *, ^, $ and [...] with ranges
and ^, over bytes as GNU grep with LC_ALL=C. GNU's other escapes and a
set's classes are refused rather than matched differently. Matching
runs every item at once, so a pattern costs the line's length times
its own. Tests compare every pattern of up to three symbols of a small
alphabet, -i, and the command's output, messages and statuses, binary
input included, with the host's GNU grep.
EOF
````


### Task 17: `grep` never reads its own output, and a write error is status 2

The prototype's review (important, I3, and minor, M2) and decision 9: `grep` read a file to whatever end it had, so `grep 1 f >> f` read the lines it appended, for ever, until the disk was full; `cat` refuses such a file, and GNU grep says `grep: f: input file is also the output`, status 2, whatever the redirection left in the file. `grep` now refuses it so. And GNU grep's status after a write error is 2, where every other command's is 1: `Ctx` gains the status a write error gives (`set_write_error_status`, 1 by default), which `run_command` and the in-process runner use. The tests bound a `grep` that reads on by a small disk and a Ctrl-C. The red run is those tests: the disk fills, and the status is 1. Mutation checks: no refusal, no status 2, and either runner using 1 each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/program.rs`
- Modify: `crates/shell/src/runner.rs`

**Interfaces:**
- Consumes: Task 16's `grep`; plan 4a's `Ctx::output_node`.
- Produces: `Ctx::set_write_error_status(&mut self, status: i32)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
    #[test]
    fn a_long_input_is_searched_in_pieces_with_its_lines_whole() {
````

with:

````rust
    #[test]
    fn grep_never_reads_its_own_output() {
        // GNU's refusal, whatever the redirection left in the file; a disk
        // that fills and a Ctrl-C bound a grep that reads on.
        let mut h = Harness::with_capacity(64 * 4096);
        h.console.interrupt_after = Some(10_000);
        let text: String = (0..2000).map(|i| alloc::format!("1 line {i}\n")).collect();
        h.put("/tmp/f", text.as_bytes());
        assert_eq!(
            h.run("grep 1 /tmp/f >> /tmp/f"),
            (2, "grep: /tmp/f: input file is also the output\n".into())
        );
        assert_eq!(h.get("/tmp/f"), text.as_bytes(), "unchanged");
        assert_eq!(
            h.run("grep 1 /tmp/nope /tmp/f > /tmp/f"),
            (
                2,
                "grep: /tmp/nope: No such file or directory\ngrep: /tmp/f: input file is also the output\n".into()
            )
        );
        // Another file into it is fine.
        h.put("/tmp/g", b"1\n");
        assert_eq!(h.run("grep 1 /tmp/g >> /tmp/f"), (0, "".into()));
    }

    #[test]
    fn a_write_error_is_status_2_as_gnu_grep_s() {
        let mut h = Harness::with_capacity(5 * 4096);
        let text: String = (0..20_000).map(|i| alloc::format!("line {i}\n")).collect();
        h.stdin = text.into_bytes();
        assert_eq!(
            h.run("grep line > /tmp/out"),
            (2, "grep: write error: No space left on device\n".into())
        );
        // As a program too; and nothing written, nothing failed: 1.
        let mut out = crate::testing::FakeStdout::file(None);
        out.fail_after = Some((10, vfs::Errno::ENOSPC));
        h.stdin = b"a\nb\nccccccccccccccccc\n".to_vec();
        assert_eq!(
            h.program("grep c", &mut out),
            (2, "grep: write error: No space left on device\n".into())
        );
        h.stdin = b"a\n".to_vec();
        assert_eq!(h.program("grep z", &mut out), (1, String::new()));
    }

    #[test]
    fn a_long_input_is_searched_in_pieces_with_its_lines_whole() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::grep::tests::a_write_error_is_status_2_as_gnu_grep_s`, `commands::grep::tests::grep_never_reads_its_own_output`.

- [ ] **Step 3: Change `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! lines is selected, grep says so (`grep: f: binary file matches`)
//! instead of printing it. The status is 0 if a line was selected, 1 if
//! none was, and 2 after an error.

````

with:

````rust
//! lines is selected, grep says so (`grep: f: binary file matches`)
//! instead of printing it; it never reads the file its output goes to.
//! The status is 0 if a line was selected, 1 if none was, and 2 after an
//! error, a write error included.

````

Replace:

````rust
pub fn grep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "cinv", "") {
````

with:

````rust
pub fn grep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    ctx.set_write_error_status(2);
    let opts = match getopt(args, "cinv", "") {
````

Replace:

````rust
        Some(path) => match open(ctx, path) {
            Ok(node) => Source::File { node, offset: 0 },
````

with:

````rust
        Some(path) => match open(ctx, path) {
            // It would read what it wrote, for ever (`grep x f >> f`).
            Ok(node) if ctx.output_node() == Some(node) => {
                ctx.fail(
                    "grep",
                    format_args!("{path}: input file is also the output"),
                );
                return Outcome::Failed;
            }
            Ok(node) => Source::File { node, offset: 0 },
````

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    input: Option<&'a mut dyn Stdin>,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

with:

````rust
    input: Option<&'a mut dyn Stdin>,
    /// The exit status when standard output could not be written (1, as
    /// milestone 1 said; GNU grep's is 2).
    pub(crate) write_error_status: i32,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

Replace:

````rust
            input: None,
            exit: false,
````

with:

````rust
            input: None,
            write_error_status: 1,
            exit: false,
````

Replace:

````rust
            exited: false,
        }
    }

````

with:

````rust
            exited: false,
        }
    }

    /// The exit status a write error on standard output gives, instead of
    /// 1.
    pub fn set_write_error_status(&mut self, status: i32) {
        self.write_error_status = status;
    }

````

- [ ] **Step 5: Change `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, replace:

````rust
        ctx.err(format!("{name}: write error: {e}\n").as_bytes());
        status = 1;
    }
````

with:

````rust
        ctx.err(format!("{name}: write error: {e}\n").as_bytes());
        status = ctx.write_error_status;
    }
````

- [ ] **Step 6: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
        message = format!("{}: write error: {e}\n", command.name);
        status = 1;
    }
````

with:

````rust
        message = format!("{}: write error: {e}\n", command.name);
        status = ctx.write_error_status;
    }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 181 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): keep grep from reading its own output

grep read a file to whatever end it had, so `grep 1 f >> f` read the
lines it appended, for ever, until the disk was full. It now refuses a
file that is also its output, with GNU grep's message and status 2.
And as GNU's, its status after a write error is 2, not every other
command's 1 (Ctx::set_write_error_status).

Refs: review I3, M2
EOF
````


### Task 18: A range cannot start where one ended in a `grep` pattern

The prototype's review (minor, M5) and decision 9: `[a-z-9]` was taken as a set of `a`–`z`, `-` and `9`, while GNU grep refuses it with `Invalid range end`: a pattern this grep matched differently from GNU's, which decision 9 rules out. A `-` right after a range is refused unless it is the set's last. A test runs fourteen sets with dashes around ranges through GNU grep (only those with a `-` after a range and before the `]` differed). The red run is that test. Mutation checks: no refusal, and refusing a last `-` too, each fail it.

**Files:**
- Modify: `crates/shell/src/pattern.rs`

**Interfaces:**
- Consumes: Task 16's `pattern::parse_set`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/pattern.rs`**

In `crates/shell/src/pattern.rs`, replace:

````rust

    /// The same with `-i`, over lines in either case.
````

with:

````rust

    /// Dashes around ranges in a set, as GNU grep takes or refuses them.
    #[test]
    fn dashes_in_a_set_are_what_gnu_grep_makes_of_them() {
        let lines = ["", "a", "b", "z", "-", "9", "]", "^", "a-z"];
        let patterns: Vec<String> = [
            "[a-z-9]",
            "[a-b-c]",
            "[a-b-]",
            "[a-b-]x",
            "[-a-b]",
            "[a-]",
            "[--a]",
            "[a--]",
            "[^a-b-]",
            "[^-a-b]",
            "[a-b-z]",
            "[]-a]",
            "[a-b]-",
            "[a-c-e-g]",
        ]
        .iter()
        .map(|p| String::from(*p))
        .collect();
        let differ = unlike_gnu(&patterns, &lines, false);
        assert!(differ.is_empty(), "{}", differ.join("\n"));
    }

    /// The same with `-i`, over lines in either case.
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `pattern::tests::dashes_in_a_set_are_what_gnu_grep_makes_of_them`.

- [ ] **Step 3: Change `crates/shell/src/pattern.rs`**

In `crates/shell/src/pattern.rs`, replace:

````rust
            i += 3;
        } else {
````

with:

````rust
            i += 3;
            // A range cannot start where one ended (`[a-z-9]`); a last `-`
            // is a member.
            if p.get(i) == Some(&b'-') && p.get(i + 1).is_some_and(|&e| e != b']') {
                return Err(PatternError::InvalidRangeEnd);
            }
        } else {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 182 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse a range that starts where one ended in grep

grep took `[a-z-9]` as a set of a-z, - and 9, while GNU grep refuses
it with `Invalid range end`: a pattern this grep matched differently
from GNU's. A test compares dashes around ranges with GNU grep.

Refs: review M5
EOF
````


### Task 19: A file named `-` is standard input

The prototype's review (minors M4 and M6) and decision 5: `cat`, `wc`, `head`, `tail` and `grep` took `-` for a file of that name, where GNU's read standard input for it (`cat header - footer`, `grep x f -`); `wc` counts it as something other than a regular file for its widths, and `grep` names it `(standard input)`. The comparisons with GNU's tools gain `-` (and `cat` a comparison of its own). `testing::host_tool` now writes the tool's input from a thread: it wrote it all before reading the output, so a tool that writes much before it has read all (`cat` of 1 MiB, which the new comparison has) waited on its full output while the helper waited on its input. The red run is the four comparisons. Mutation checks: each command without its `-` fails its comparison.

**Files:**
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Tasks 8, 9, 10 and 16.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
            (&["grep", "-", "t"], b""),
        ];
````

with:

````rust
            (&["grep", "-", "t"], b""),
            (&["grep", "a", "t", "-"], b"a\nz\n"),
            (&["grep", "-c", "b", "-", "t"], b"b\n"),
            (&["grep", "-n", "x", "-"], b"x\n"),
        ];
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn cat_stops_at_the_first_write_error() {
````

with:

````rust
    #[test]
    fn cat_prints_what_gnu_cat_prints() {
        let files: &[(&str, &[u8])] = &[("t", b"text\n"), ("d/", b"")];
        let big = alloc::vec![b'x'; 1 << 20];
        let cases: &[(&[&str], &[u8])] = &[
            (&["cat", "t"], b""),
            (&["cat"], b"in\n"),
            (&["cat", "-"], b"in\n"),
            (&["cat", "nope", "-"], b"in\n"),
            (&["cat", "t", "-", "t"], b"in\n"),
            (&["cat", "-", "-"], b"once\n"),
            (&["cat", "d", "t"], b""),
            (&["cat"], &big),
        ];
        for (args, stdin) in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, files, stdin),
                crate::testing::host_tool(args, files, stdin),
                "{args:?}"
            );
        }
    }

    #[test]
    fn cat_stops_at_the_first_write_error() {
````

Replace:

````rust
            (&["tail", "-n", "3"], b"\n\n\n\n"),
        ];
````

with:

````rust
            (&["tail", "-n", "3"], b"\n\n\n\n"),
            (&["head", "-n", "2", "-"], ten),
            (&["tail", "-1", "-"], ten),
        ];
````

Replace:

````rust
            (&["wc", "a", "d", "nope"], b"not read"),
        ];
````

with:

````rust
            (&["wc", "a", "d", "nope"], b"not read"),
            (&["wc", "-"], a),
            (&["wc", "-c", "-"], b),
            (&["wc", "-", "a"], b),
            (&["wc", "-l", "a", "-"], a),
        ];
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    // A tool that stops reading early (head) closes the pipe: not an error.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    let out = child.wait_with_output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
````

with:

````rust
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    // From a thread: a tool that writes much before it has read all would
    // otherwise wait on its full output while this waits on its input. A
    // tool that stops reading early (head) closes the pipe: not an error.
    let (mut pipe, data) = (child.stdin.take().unwrap(), stdin.to_vec());
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&data);
    });
    let out = child.wait_with_output().unwrap();
    let _ = writer.join();
    let _ = std::fs::remove_dir_all(&dir);
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 4 tests fail, among them `commands::text::tests::cat_prints_what_gnu_cat_prints`, `commands::text::tests::wc_prints_what_gnu_wc_prints`.

- [ ] **Step 5: Change `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! the lines of each file, or of standard input, that hold a match of
//! `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`.
//! Several files put each one's name before its lines; `-i` ignores case,
````

with:

````rust
//! the lines of each file, or of standard input, that hold a match of
//! `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`
//! (a file `-` is standard input too).
//! Several files put each one's name before its lines; `-i` ignores case,
````

Replace:

````rust
) -> Outcome {
    let name = file.map_or("(standard input)", String::as_str);
````

with:

````rust
) -> Outcome {
    // `-` is standard input, as no file is.
    let file = file.filter(|f| *f != "-");
    let name = file.map_or("(standard input)", String::as_str);
````

- [ ] **Step 6: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
//! `cat`, `head`, `tail` and `wc` (spec §7.3). Without a file they read
//! standard input (user-space gate §9.1), which GNU calls `-` in its
//! messages.

````

with:

````rust
//! `cat`, `head`, `tail` and `wc` (spec §7.3). Without a file, and for a
//! file named `-`, they read standard input (user-space gate §9.1), as
//! GNU's do.

````

Replace:

````rust
        // The shell reports the write error when the command ends.
        if ctx.out_failed() {
            break;
        }
````

with:

````rust
        // The shell reports the write error when the command ends.
        if ctx.out_failed() || ctx.interrupted() {
            break;
        }
        if op == "-" {
            status = status.max(cat_input(ctx));
            continue;
        }
````

Replace:

````rust
/// The line count and the one file of `head`/`tail`: `[-n N] [file]`, also
/// `-N`; no file is standard input.
fn lines_and_file(
````

with:

````rust
/// The line count and the one file of `head`/`tail`: `[-n N] [file]`, also
/// `-N`; no file, or `-`, is standard input.
fn lines_and_file(
````

Replace:

````rust
        [] => return Ok((count, None)),
        [file] => file,
````

with:

````rust
        [] => return Ok((count, None)),
        [file] if file == "-" => return Ok((count, None)),
        [file] => file,
````

Replace:

````rust
/// `wc [-clw] [file…]`: lines, words and bytes, and a total for several
/// files; standard input without a file.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust
/// `wc [-clw] [file…]`: lines, words and bytes, and a total for several
/// files; standard input without a file, and for `-`.
pub fn wc(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
    for op in &opts.operands {
        let node = ctx.vfs.lookup(op.as_bytes());
````

with:

````rust
    for op in &opts.operands {
        // `-` is standard input, never a regular file here.
        if op == "-" {
            odd = true;
            found.push(Ok(None));
            continue;
        }
        let node = ctx.vfs.lookup(op.as_bytes());
````

Replace:

````rust
                }
                found.push(Ok(n));
            }
````

with:

````rust
                }
                found.push(Ok(Some(n)));
            }
````

Replace:

````rust
        let name = quote_if_needed(op);
        let counts = match node.and_then(|n| count(ctx, n)) {
            Ok(c) => c,
````

with:

````rust
        let name = quote_if_needed(op);
        let counted = match node {
            Ok(Some(n)) => count(ctx, n),
            Ok(None) => match count_input(ctx) {
                None => return 0,
                Some((c, None)) => Ok(c),
                Some((c, Some(e))) => {
                    status = ctx.fail("wc", format_args!("-: {e}"));
                    Ok(c)
                }
            },
            Err(e) => Err(e),
        };
        let counts = match counted {
            Ok(c) => c,
````

Replace:

````rust
fn wc_input(ctx: &mut Ctx<'_>, shown: Shown) -> i32 {
    let mut buf = vec![0; CHUNK];
    let (mut c, mut in_word) = (Counts::default(), false);
    let mut status = 0;
    loop {
        if ctx.interrupted() {
            return 0;
        }
        match ctx.read_input(&mut buf) {
            Ok(0) => break,
            Ok(n) => c.add(&buf[..n], &mut in_word),
            Err(e) => {
                status = ctx.fail("wc", format_args!("-: {e}"));
                break;
            }
        }
    }
    let width = if shown.how_many() == 1 { 1 } else { 7 };
    outln!(ctx, "{}", shown.line(c, width, None));
    status
}
````

with:

````rust
fn wc_input(ctx: &mut Ctx<'_>, shown: Shown) -> i32 {
    let Some((c, error)) = count_input(ctx) else {
        return 0;
    };
    let status = error.map_or(0, |e| ctx.fail("wc", format_args!("-: {e}")));
    let width = if shown.how_many() == 1 { 1 } else { 7 };
    outln!(ctx, "{}", shown.line(c, width, None));
    status
}

/// Standard input's counts, to its end or a read error (with the counts
/// so far); `None` once Ctrl-C has stopped the command.
fn count_input(ctx: &mut Ctx<'_>) -> Option<(Counts, Option<Errno>)> {
    let mut buf = vec![0; CHUNK];
    let (mut c, mut in_word) = (Counts::default(), false);
    loop {
        if ctx.interrupted() {
            return None;
        }
        match ctx.read_input(&mut buf) {
            Ok(0) => return Some((c, None)),
            Ok(n) => c.add(&buf[..n], &mut in_word),
            Err(e) => return Some((c, Some(e))),
        }
    }
}
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 183 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read standard input for a file named -

cat, wc, head, tail and grep took `-` for a file of that name, while
GNU's read standard input for it (`cat header - footer`); the tests
now compare it with the host's tools. testing::host_tool writes the
tool's input from a thread, so that a tool's big output cannot wait
on an input not yet written.

Refs: review M4, M6
EOF
````


### Task 20: `seq`

Spec §9.1 and decision 10: `seq [FIRST [INCREMENT]] LAST` prints the whole numbers from FIRST to LAST as GNU seq does: a test compares 16 command lines with the host's (negative numbers, which are operands, not options; a `--`; a `+`; leading blanks; the ends of 64 bits). GNU's `INCREMENT` is taken too, rather than calling a third operand extra; GNU's also takes decimals, which this one refuses as `seq: not a whole number: '1.5'` (GNU's message would say the argument is no number). A zero increment, a missing and an extra operand get GNU's first line. Ctrl-C and a write error stop it. The red run is the shell's tests, where `seq` is no command. Mutation checks: the zero increment (the test meets a Ctrl-C after 10000 checks instead of hanging), the last number left out, either direction's end, the `--` and the blanks each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Create: `crates/shell/src/commands/seq.rs`

**Interfaces:**
- Consumes: Task 9's `host_tool`, `like_host`.
- Produces: `commands::seq` in the command table.

- [ ] **Step 1: Declare the new module in `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
mod script;
mod stat;
````

with:

````rust
mod script;
mod seq;
mod stat;
````

- [ ] **Step 2: Write the failing tests for `crates/shell/src/commands/seq.rs`**

Create `crates/shell/src/commands/seq.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::{Harness, host_tool};
    use alloc::string::String;

    #[test]
    fn seq_prints_what_gnu_seq_prints() {
        let cases: &[&[&str]] = &[
            &["seq", "5"],
            &["seq", "3", "5"],
            &["seq", "5", "3"],
            &["seq", "-2", "2"],
            &["seq", "1", "2", "9"],
            &["seq", "1", "2", "10"],
            &["seq", "10", "-3", "1"],
            &["seq", "0"],
            &["seq", "-3", "-1"],
            &["seq", "--", "5"],
            &["seq", "+3"],
            &["seq", " 4"],
            &["seq", "007"],
            &["seq", "9223372036854775806", "9223372036854775807"],
            &[
                "seq",
                "-9223372036854775807",
                "-1000000000000000000",
                "-9223372036854775808",
            ],
            &["seq", "1000"],
        ];
        for args in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &[], b""),
                host_tool(args, &[], b""),
                "{args:?}"
            );
        }
    }

    #[test]
    fn what_seq_refuses() {
        let mut h = Harness::new();
        // A seq that printed for ever would meet a Ctrl-C and fail the test.
        h.console.interrupt_after = Some(10_000);
        // GNU's first line; GNU also takes decimals, these do not.
        for (line, said) in [
            ("seq", "seq: missing operand\n"),
            ("seq 1 2 3 4", "seq: extra operand '4'\n"),
            ("seq 1 0 5", "seq: invalid Zero increment value: '0'\n"),
            ("seq x", "seq: not a whole number: 'x'\n"),
            ("seq 1.5 3", "seq: not a whole number: '1.5'\n"),
            (
                "seq 99999999999999999999",
                "seq: not a whole number: '99999999999999999999'\n",
            ),
            ("seq - 3", "seq: not a whole number: '-'\n"),
        ] {
            assert_eq!(h.run(line), (1, String::from(said)), "{line}");
        }
    }

    #[test]
    fn ctrl_c_and_a_write_error_stop_seq() {
        let mut h = Harness::new();
        h.console.interrupt_after = Some(3);
        assert_eq!(
            h.run("seq 9223372036854775807"),
            (130, "1\n2\n3\n^C\n".into())
        );
        let mut h = Harness::with_capacity(5 * 4096);
        assert_eq!(
            h.run("seq 9223372036854775807 > /tmp/o"),
            (1, "seq: write error: No space left on device\n".into())
        );
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `commands::seq::tests::ctrl_c_and_a_write_error_stop_seq`, `commands::seq::tests::seq_prints_what_gnu_seq_prints`.

- [ ] **Step 4: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub use script::{SCRIPT_MAX, transcript_name};
pub use stat::stat;
````

with:

````rust
pub use script::{SCRIPT_MAX, transcript_name};
pub use seq::seq;
pub use stat::stat;
````

Replace:

````rust
    Builtin {
        name: "sh",
````

with:

````rust
    Builtin {
        name: "seq",
        help: "print a sequence of whole numbers",
        run: seq::seq,
    },
    Builtin {
        name: "sh",
````

- [ ] **Step 5: Implement `crates/shell/src/commands/seq.rs`**

Insert this at the top of `crates/shell/src/commands/seq.rs`, above `#[cfg(test)]`:

````rust
//! `seq [FIRST [INCREMENT]] LAST` (user-space gate §9.1): the whole numbers
//! from FIRST (1) to LAST, INCREMENT (1) apart, one a line, as GNU seq
//! prints them. GNU's also take decimals; these do not, and say so.

use crate::ctx::{Ctx, outln, quote};
use alloc::string::String;
use alloc::vec::Vec;

pub fn seq(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    // Numbers may start with `-`: there are no options, only a `--`.
    let ops = match args.split_first() {
        Some((first, rest)) if first == "--" => rest,
        _ => args,
    };
    if let Some(extra) = ops.get(3) {
        return ctx.fail("seq", format_args!("extra operand {}", quote(extra)));
    }
    let mut numbers = Vec::new();
    for op in ops {
        match whole(op) {
            Some(n) => numbers.push(n),
            None => return ctx.fail("seq", format_args!("not a whole number: {}", quote(op))),
        }
    }
    let (first, step, last) = match numbers[..] {
        [] => return ctx.fail("seq", format_args!("missing operand")),
        [last] => (1, 1, last),
        [first, last] => (first, 1, last),
        [first, step, last] => (first, step, last),
        _ => unreachable!("at most three operands"),
    };
    if step == 0 {
        let message = format_args!("invalid Zero increment value: {}", quote(&ops[1]));
        return ctx.fail("seq", message);
    }
    let mut n = first;
    while !ctx.interrupted() && !ctx.out_failed() {
        if (step > 0 && n > last) || (step < 0 && n < last) {
            break;
        }
        outln!(ctx, "{n}");
        match n.checked_add(step) {
            Some(next) => n = next,
            None => break,
        }
    }
    0
}

/// A whole number: a sign or none, then digits (leading blanks allowed,
/// as GNU's); `None` for anything else, or beyond 64 bits.
fn whole(s: &str) -> Option<i64> {
    let s = s.trim_start_matches([' ', '\t']);
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.strip_prefix('+').unwrap_or(s).parse().ok()
}

````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 186 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add seq

seq [FIRST [INCREMENT]] LAST prints the whole numbers from FIRST to
LAST, as GNU seq does, which a test compares with the host's (negative
numbers, a -- and the ends of 64 bits included). GNU's also takes
decimals; this one says `not a whole number` instead. INCREMENT is
GNU's too, so three operands are no extra operand.
EOF
````


### Task 21: `sleep`, `true` and `false`

Spec §9.1 and decision 10: `sleep` waits for the sum of its times, in seconds or with GNU's suffixes `s`, `m`, `h` and `d`, a fraction counting to the millisecond and a total beyond 64 bits saturating, through the shell's `System`, which gains `sleep` (`relay-rt`'s is the `sleep` call, `host-shell`'s the host's, the tests' records it). A time that is not one is GNU's `sleep: invalid time interval 'x'`. `true` and `false` do nothing, with status 0 and 1, whatever their arguments, as GNU's. The red run is the shell's tests, which cannot compile against a `System` without `sleep`. Mutation checks: an empty number taken, no sleep, times not summed, and 3 fraction digits instead of 9 (which survived at first: seconds need only 3; `0.0001m` does not) each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/relay-rt/src/sysvfs.rs`
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/system.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: plan 3a's `sleep` call.
- Produces: `System::sleep(&mut self, ms: u64)`; `commands::{sleep, true, false}` in the command table; the test fake's `TestSystem::slept`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, replace:

````rust
        }
        fn reboot(&mut self, _: bool) -> Result<(), Errno> {
````

with:

````rust
        }
        fn sleep(&mut self, _: u64) {}
        fn reboot(&mut self, _: bool) -> Result<(), Errno> {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    use crate::testing::Harness;

````

with:

````rust
    use crate::testing::Harness;

    #[test]
    fn true_and_false_ignore_their_arguments() {
        let mut h = Harness::new();
        assert_eq!(h.run("true"), (0, "".into()));
        assert_eq!(h.run("true --help x"), (0, "".into()));
        assert_eq!(h.run("false"), (1, "".into()));
        assert_eq!(h.run("false -x"), (1, "".into()));
    }

````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, replace:

````rust
    #[test]
    fn date_prints_utc() {
````

with:

````rust
    #[test]
    fn sleep_waits_for_the_sum_of_its_times_to_the_millisecond() {
        let mut h = Harness::new();
        for (line, ms) in [
            ("sleep 2", 2000),
            ("sleep 1.5 2m", 121_500),
            ("sleep .25", 250),
            ("sleep 5.", 5000),
            ("sleep 0.0004", 0),
            ("sleep 1h 1d 0s", 90_000_000),
            ("sleep 0.123456789987", 123),
            ("sleep 0.0001m 0.00001h", 6 + 36),
            ("sleep 99999999999999999999 1", u64::MAX),
            ("sleep -- 1", 1000),
        ] {
            h.system.slept.clear();
            assert_eq!(h.run(line), (0, "".into()), "{line}");
            assert_eq!(h.system.slept, [ms], "{line}");
        }
    }

    #[test]
    fn what_sleep_refuses() {
        let mut h = Harness::new();
        for (line, said) in [
            ("sleep", "sleep: missing operand\n"),
            ("sleep x", "sleep: invalid time interval 'x'\n"),
            ("sleep 1x", "sleep: invalid time interval '1x'\n"),
            ("sleep .", "sleep: invalid time interval '.'\n"),
            ("sleep 1.2.3", "sleep: invalid time interval '1.2.3'\n"),
            ("sleep s", "sleep: invalid time interval 's'\n"),
            ("sleep 1 y", "sleep: invalid time interval 'y'\n"),
            ("sleep -1", "sleep: invalid option -- '1'\n"),
        ] {
            assert_eq!(h.run(line), (1, said.into()), "{line}");
        }
        assert!(h.system.slept.is_empty(), "nothing is waited for");
    }

    #[test]
    fn date_prints_utc() {
````

- [ ] **Step 4: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub forced: Vec<bool>,
}
````

with:

````rust
    pub forced: Vec<bool>,
    /// Every `sleep`, in milliseconds.
    pub slept: Vec<u64>,
}
````

Replace:

````rust
            forced: Vec::new(),
        }
````

with:

````rust
            forced: Vec::new(),
            slept: Vec::new(),
        }
````

Replace:

````rust
        self.log.clone()
    }
````

with:

````rust
        self.log.clone()
    }
    fn sleep(&mut self, ms: u64) {
        self.slept.push(ms);
    }
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `sleep` is not a member of trait `System` ``.

- [ ] **Step 6: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        buf
    }
````

with:

````rust
        buf
    }

    fn sleep(&mut self, ms: u64) {
        sys::sleep(ms);
    }
````

- [ ] **Step 7: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    outln!(ctx, "{cwd}");
    0
}

````

with:

````rust
    outln!(ctx, "{cwd}");
    0
}

/// `true`: nothing, successfully; its arguments are ignored, as GNU's are.
pub fn r#true(_: &mut Ctx<'_>, _: &[String]) -> i32 {
    0
}

/// `false`: nothing, unsuccessfully.
pub fn r#false(_: &mut Ctx<'_>, _: &[String]) -> i32 {
    1
}

````

- [ ] **Step 8: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

pub use basic::{clear, echo, pwd, uname};
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
````

with:

````rust

pub use basic::{clear, echo, r#false, pwd, r#true, uname};
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
````

Replace:

````rust
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, reboot, sync};
pub use text::{cat, head, tail, wc};
````

with:

````rust
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, reboot, sleep, sync};
pub use text::{cat, head, tail, wc};
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
        name: "false",
        help: "do nothing, unsuccessfully",
        run: basic::r#false,
    },
````

Replace:

````rust
    Builtin {
        name: "stat",
````

with:

````rust
    Builtin {
        name: "sleep",
        help: "wait for a number of seconds",
        run: system::sleep,
    },
    Builtin {
        name: "stat",
````

Replace:

````rust
        run: change::touch,
    },
````

with:

````rust
        run: change::touch,
    },
    Builtin {
        name: "true",
        help: "do nothing, successfully",
        run: basic::r#true,
    },
````

- [ ] **Step 9: Change `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, replace:

````rust
        None => Ok(opts),
    }
}

````

with:

````rust
        None => Ok(opts),
    }
}

/// `sleep NUMBER[SUFFIX]...`: waits for the sum of the times given, each in
/// seconds, or with GNU's suffixes `s`, `m`, `h` or `d`; a number may have
/// a fraction, which counts to the millisecond.
pub fn sleep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("sleep", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return ctx.fail("sleep", format_args!("missing operand"));
    }
    let mut total: u64 = 0;
    for op in &opts.operands {
        match millis(op) {
            Some(ms) => total = total.saturating_add(ms),
            None => return ctx.fail("sleep", format_args!("invalid time interval {}", quote(op))),
        }
    }
    ctx.system.sleep(total);
    0
}

/// `NUMBER[SUFFIX]` in milliseconds, at most `u64::MAX`.
fn millis(s: &str) -> Option<u64> {
    let (number, unit): (&str, u128) = match s.as_bytes().last()? {
        b's' => (&s[..s.len() - 1], 1000),
        b'm' => (&s[..s.len() - 1], 60_000),
        b'h' => (&s[..s.len() - 1], 3_600_000),
        b'd' => (&s[..s.len() - 1], 86_400_000),
        _ => (s, 1000),
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return None;
    }
    // Beyond 20 digits a number is more than u64 milliseconds anyway, and
    // beyond 9 a fraction is less than one.
    let value = |t: &str| {
        t.bytes().fold(0u128, |n, b| {
            n.saturating_mul(10).saturating_add(u128::from(b - b'0'))
        })
    };
    let fraction = &fraction[..fraction.len().min(9)];
    let ms = value(&whole[..whole.len().min(30)])
        .saturating_mul(unit)
        .saturating_add(value(fraction) * unit / 10u128.pow(fraction.len() as u32));
    Some(u64::try_from(ms).unwrap_or(u64::MAX))
}

````

- [ ] **Step 10: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Restarts the machine, going ahead with `force` when the filesystems
````

with:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Waits `ms` milliseconds (`sleep`).
    fn sleep(&mut self, ms: u64);
    /// Restarts the machine, going ahead with `force` when the filesystems
````

- [ ] **Step 11: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, replace:

````rust
        self.0.contents()
    }
````

with:

````rust
        self.0.contents()
    }
    fn sleep(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 189 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 27 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates xtask
git commit -F - <<'EOF'
feat(shell): add sleep, true and false

sleep waits for the sum of its times, in seconds or with GNU's s, m,
h and d, a fraction counting to the millisecond, through the shell's
System, which gains sleep: relay-rt's is the sleep call and
host-shell's the host's. true and false do nothing, with status 0 and
1, whatever their arguments.
EOF
````


### Task 22: `seq` says what GNU's says of what is no number

The prototype's review (minor, M3) and decision 10: `seq` said `not a whole number` of anything it could not take, also of `x`, of which GNU's says `invalid floating point argument: 'x'` (decision 11 keeps GNU's first lines). Now only what GNU's seq takes and this one does not (decimals, exponents, `inf`, hexadecimal, numbers beyond 64 bits) gets that message; anything else, an unknown option before the operands (`invalid option -- 'w'`, `unrecognized option '--foo'`: GNU's options `-f`, `-s` and `-w` are unknown here) and `nan` get GNU's first line, which a test compares with the host's. The red run is that test and the refusals' test. Mutation checks: no option check, no `nan`, every refusal GNU's, and an exponent without digits taken as a number each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/seq.rs`

**Interfaces:**
- Consumes: Task 20's `seq`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/seq.rs`**

In `crates/shell/src/commands/seq.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            ("seq 1 0 5", "seq: invalid Zero increment value: '0'\n"),
            ("seq x", "seq: not a whole number: 'x'\n"),
            ("seq 1.5 3", "seq: not a whole number: '1.5'\n"),
````

with:

````rust
            ("seq 1 0 5", "seq: invalid Zero increment value: '0'\n"),
            ("seq 1.5 3", "seq: not a whole number: '1.5'\n"),
````

Replace:

````rust
            ),
            ("seq - 3", "seq: not a whole number: '-'\n"),
        ] {
            assert_eq!(h.run(line), (1, String::from(said)), "{line}");
        }
````

with:

````rust
            ),
            ("seq 1e3", "seq: not a whole number: '1e3'\n"),
            ("seq -.5 1", "seq: not a whole number: '-.5'\n"),
            ("seq inf", "seq: not a whole number: 'inf'\n"),
            ("seq 0x10", "seq: not a whole number: '0x10'\n"),
            // GNU's options are not this seq's.
            ("seq -w 3", "seq: invalid option -- 'w'\n"),
        ] {
            assert_eq!(h.run(line), (1, String::from(said)), "{line}");
        }
    }

    /// What GNU seq refuses, it refuses with its first line here too.
    #[test]
    fn seq_refuses_what_gnu_seq_refuses_as_it_does() {
        for args in [
            &["seq", "x"][..],
            &["seq", "1", "x"],
            &["seq", "-"],
            &["seq", "+"],
            &["seq", "."],
            &["seq", "1e"],
            &["seq", "1", "-x"],
            &["seq", "-x", "3"],
            &["seq", "--foo", "3"],
            &["seq", "nan"],
        ] {
            let mut h = Harness::new();
            let (status, out, err) = host_tool(args, &[], b"");
            let first = err.lines().next().unwrap_or("");
            assert_eq!(
                h.like_host(args, &[], b""),
                (status, out, alloc::format!("{first}\n")),
                "{args:?}"
            );
        }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::seq::tests::what_seq_refuses`, `commands::seq::tests::seq_refuses_what_gnu_seq_refuses_as_it_does`.

- [ ] **Step 3: Change `crates/shell/src/commands/seq.rs`**

In `crates/shell/src/commands/seq.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! from FIRST (1) to LAST, INCREMENT (1) apart, one a line, as GNU seq
//! prints them. GNU's also take decimals; these do not, and say so.

````

with:

````rust
//! from FIRST (1) to LAST, INCREMENT (1) apart, one a line, as GNU seq
//! prints them. GNU's also takes decimals, `inf`, hexadecimal and numbers
//! beyond 64 bits, and the options `-f`, `-s` and `-w`; this one says it
//! does not. What GNU's refuses, it refuses with GNU's first line.

````

Replace:

````rust
pub fn seq(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    // Numbers may start with `-`: there are no options, only a `--`.
    let ops = match args.split_first() {
        Some((first, rest)) if first == "--" => rest,
        _ => args,
````

with:

````rust
pub fn seq(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    // Options come before the first operand, as GNU's; a number may start
    // with `-`.
    let ops = match args.split_first() {
        Some((first, rest)) if first == "--" => rest,
        Some((first, _)) if first.starts_with("--") => {
            return ctx.fail("seq", format_args!("unrecognized option {}", quote(first)));
        }
        Some((first, _)) if is_option(first) => {
            let c = first[1..].chars().next().unwrap_or('-');
            return ctx.fail("seq", format_args!("invalid option -- '{c}'"));
        }
        _ => args,
````

Replace:

````rust
            Some(n) => numbers.push(n),
            None => return ctx.fail("seq", format_args!("not a whole number: {}", quote(op))),
        }
````

with:

````rust
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
        }
````

Replace:

````rust
    0
}
````

with:

````rust
    0
}

/// An option: `-` and something that cannot start a number.
fn is_option(s: &str) -> bool {
    s.strip_prefix('-')
        .and_then(|rest| rest.bytes().next())
        .is_some_and(|b| !b.is_ascii_digit() && b != b'.')
}

/// A number GNU's seq takes: decimal with a fraction or an exponent,
/// hexadecimal, or `inf`, each with a sign or none (leading blanks allowed).
fn is_number(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    let s = s.strip_prefix(['+', '-']).unwrap_or(s).to_ascii_lowercase();
    if s == "inf" || s == "infinity" {
        return true;
    }
    let (digits, exponent_mark): (&str, char) = match s.strip_prefix("0x") {
        Some(hex) => (hex, 'p'),
        None => (&s, 'e'),
    };
    let is_digit = |c: char| {
        if exponent_mark == 'p' {
            c.is_ascii_hexdigit()
        } else {
            c.is_ascii_digit()
        }
    };
    let (mantissa, exponent) = match digits.split_once(exponent_mark) {
        Some((m, e)) => (m, Some(e)),
        None => (digits, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mantissa_ok = !(whole.is_empty() && fraction.is_empty())
        && whole.chars().all(is_digit)
        && fraction.chars().all(is_digit);
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.bytes().all(|b| b.is_ascii_digit())
    });
    mantissa_ok && exponent_ok
}

/// GNU's not-a-number (`nan`, either case, with a sign or none).
fn is_nan(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    s.strip_prefix(['+', '-'])
        .unwrap_or(s)
        .eq_ignore_ascii_case("nan")
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 190 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): give seq GNU's message for what is no number

seq said `not a whole number` for anything it could not take, also
for `x`, of which GNU's says `invalid floating point argument`. Now
only what GNU's seq takes and this one does not (decimals, exponents,
inf, hexadecimal, beyond 64 bits) gets that message; the rest, an
unknown option before the operands and nan get GNU's first line, which
a test compares with the host's. GNU's options -f, -s and -w are
unknown here.

Refs: review M3
EOF
````


### Task 23: `grep`, `seq`, `sleep`, `true` and `false` in `/bin`

Spec §8.4 and decision 10: each new command is a program of its own, a thin `main` over its command function, as the others are, so `/bin` holds 40 programs (`docs/hardware-test.md`'s startup line, the kernel's comment). `system` lists the new `/bin`; `pipes` gains §12.3's `seq 5 | grep -c .`, `seq 1000 | grep 7 | wc -l`, `grep -v`, `seq 1000000 | head -n 2` (seq ending quietly) and a `sleep` before a stage. The red runs are `system` and `pipes`, whose programs are not in `/bin` yet.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`
- Modify: `tests/e2e/pipes.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/utils/src/bin/false.rs`
- Create: `userland/utils/src/bin/grep.rs`
- Create: `userland/utils/src/bin/seq.rs`
- Create: `userland/utils/src/bin/sleep.rs`
- Create: `userland/utils/src/bin/true.rs`

**Interfaces:**
- Consumes: Tasks 16–21's command functions; plan 4a's `relay_rt::sysio::run_command`.
- Produces: `/bin/{grep,seq,sleep,true,false}`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/pipes.txt`**

In `tests/e2e/pipes.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# 1): every stage a program in one process group, each one's output the
# next one's input. 8 MiB through cat and wc arrive whole; head ends a
# pipeline at once, its writer ending quietly; the console goes to the
# first stage in line mode, and Ctrl-C ends every stage; a script's
# pipeline is traced and its output in the transcript; and free shows the
# same memory in use before and after.
timeout 120
````

with:

````text
# 1): every stage a program in one process group, each one's output the
# next one's input. 8 MiB through cat and wc arrive whole; seq and grep
# work in a pipeline (§12.3); head ends a pipeline at once, its writer
# ending quietly; the console goes to the first stage in line mode, and
# Ctrl-C ends every stage; a script's pipeline is traced and its output in
# the transcript; and free shows the same memory in use before and after.
timeout 120
````

Replace:

````text
expect \n\[1\] first\nroot@relay:~# $
# Not the screen: one name a line; the last stage's redirection.
````

with:

````text
expect \n\[1\] first\nroot@relay:~# $
# seq and grep (§12.3); seq ends quietly once head has its lines.
send seq 5 | grep -c .
expect \n5\n
send seq 1000 | grep 7 | wc -l
expect \n271\n
send seq 3 | grep -v 2
expect \n1\n3\nroot@relay:~# $
send seq 1000000 | head -n 2
expect \n1\n2\nroot@relay:~# $
send sleep 1 | t-args slept
expect \n\[1\] slept\nroot@relay:~# $
# Not the screen: one name a line; the last stage's redirection.
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem   t-spawn  t-tee  uname\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-pipe  t-spin   tail   wc\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-read  t-sys    touch\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-read   t-sys  touch  wc\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-spawn  t-tee  true\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spin   tail   uname\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat   t-args   t-mem   t-spawn  t-tee  uname\nclear  df     free  mkdir  pwd       rmdir  sync   t-fault  t-pipe  t-spin   tail   wc\ncp     dmesg  head  mv     reboot    sh     t-abi  t-files  t-read  t-sys    touch\n
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-read   t-sys  touch  wc\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-spawn  t-tee  true\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spin   tail   uname\n
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: FAIL: scenario `system` stops at line 8, timed out waiting for `\ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-read   t-sys  touch  wc\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-spawn  t-tee  true\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spin   tail   uname\n`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: FAIL: scenario `pipes` stops at line 35, timed out waiting for `\n5\n`.

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 35 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 40 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 5: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 35 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 40 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 6: Create `userland/utils/src/bin/false.rs`**

Create `userland/utils/src/bin/false.rs`:

````rust
//! `/bin/false`: do nothing, unsuccessfully (user-space gate §8.4, §9.1),
//! the shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("false", shell::commands::r#false, &args)
}
````

- [ ] **Step 7: Create `userland/utils/src/bin/grep.rs`**

Create `userland/utils/src/bin/grep.rs`:

````rust
//! `/bin/grep`: print the lines that match a pattern (user-space gate §8.4,
//! §9.1), the shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("grep", shell::commands::grep, &args)
}
````

- [ ] **Step 8: Create `userland/utils/src/bin/seq.rs`**

Create `userland/utils/src/bin/seq.rs`:

````rust
//! `/bin/seq`: print a sequence of whole numbers (user-space gate §8.4,
//! §9.1), the shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("seq", shell::commands::seq, &args)
}
````

- [ ] **Step 9: Create `userland/utils/src/bin/sleep.rs`**

Create `userland/utils/src/bin/sleep.rs`:

````rust
//! `/bin/sleep`: wait for a number of seconds (user-space gate §8.4, §9.1),
//! the shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("sleep", shell::commands::sleep, &args)
}
````

- [ ] **Step 10: Create `userland/utils/src/bin/true.rs`**

Create `userland/utils/src/bin/true.rs`:

````rust
//! `/bin/true`: do nothing, successfully (user-space gate §8.4, §9.1), the
//! shell's command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("true", shell::commands::r#true, &args)
}
````

- [ ] **Step 11: Run the `system`, `pipes`, `utils` scenarios**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add docs kernel tests userland
git commit -F - <<'EOF'
feat(utils): add grep, seq, sleep, true and false to /bin

Each is the shell's command function run as a program, as the other
commands are (spec §8.4), so /bin holds 40 programs. The pipes scenario
gains §12.3's seq 5 | grep -c . and seq ending once head has its lines;
system lists the new /bin.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 43 scenario(s) passed`.

````bash
git push -u origin m3p1/utils
gh pr create --base main --head m3p1/utils --title "feat(shell,utils): add grep, seq, sleep, true and false" --body-file - <<'EOF'
## What

Milestone 3, plan 1, tasks 16–23: `grep [-i] [-v] [-n] [-c]` with GNU's basic patterns' subset (compared with the host's GNU grep over every pattern of up to three symbols of a small alphabet), which never reads its own output; `-` is standard input in `cat`, `wc`, `head`, `tail` and `grep`; `seq [FIRST [INCREMENT]] LAST` in whole numbers, with GNU's messages for what it refuses; `sleep` with GNU's suffixes and fractions; `true` and `false`; `/bin` holds 40 programs, and `pipes` runs `seq 5 | grep -c .`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types into a pipeline); plan 4's NUC check 5 runs pipes there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p1/utils --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p1-utils
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
