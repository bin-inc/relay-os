# Milestone 3 · Plan 2: Jobs, `ps` and `kill` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs run in the background. The kernel gains `proc_list` (a `ProcInfo` per process), refuses the console's calls to a process group that was never given the console, and lets a Ctrl-C end a `wait` that asks for it (`WAIT_CTRL_C`); `/bin/sh` runs `cmd &` and `a | b &` as jobs in groups of their own, numbers and reports them in bash's words, and gains the built-ins `jobs`, `wait` and `kill`; `/bin/ps` lists the processes. The spec gets §16 item 9. It ends with `cargo xtask ci` green, the new scenarios `proc_calls`, `screen_console` and `jobs` among the 46.

**Architecture:** The process table keeps the chain of groups that handed the console on (`kernel/src/proc/holders.rs`), from process 1's to the foreground group; a group may change the console only while it is in the chain, so a shell takes it back from its own commands and a background job, never given it, cannot. `proc_list` copies `Table::list` (each process's state saying what it waits for, its address space's frames counted by walking its tables). `wait`'s `WAIT_CTRL_C` takes a Ctrl-C typed in raw mode, the oldest byte when it waits, and the console's input wakes the waiting group. In the shell crate, `parser::parse_line` gives a line's pipeline and whether it ends with `&`; the spawning runner starts a job's commands without the console (`Group::Background`) and waits for none; `jobs::Jobs` keeps them, numbered and worded as bash 5.2 does; a built-in's `Ctx` gets the shell's job control (`JobControl`) for `jobs`, `wait` and `kill`; `Programs` gains `collect`, `wait_or_ctrl_c` and `kill`, and `System` gains `processes` for `ps`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model); bash 5.2 in a terminal (a pty) for the expectations, since an interactive bash numbers and reports jobs as `bash -c` and scripts do not.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§5.4, §6.4, §7.3, §7.4, §8.3–§8.5, §9.2, §9.3, §11.2, §12.1, §12.3, §13 step 7, §15, §16 item 9)
**Roadmap:** `docs/superpowers/plans/2026-10-01-milestone-3-roadmap.md` — this is plan 2 of 4 of milestone 3 (spec §13 step 7).

## In brief

- **Size.** 25 tasks in three code pull requests, plus this plan as PR 1 (decision 1): PR 2 the kernel's `proc_list`, the console's holders and `wait`'s Ctrl-C; PR 3 background jobs, `jobs`, `wait` and `kill` in the shell; PR 4 `ps`. No NUC check: nothing here is particular to the NUC, the stick keeps 0.3.0, and plan 4's `check5.sh` runs a background job and `kill` there.
- **Who may change the console** (decision 3). "Only the foreground group" cannot be the rule: `/bin/sh` takes the console back after its command's group has ended and been collected. The process table keeps the chain of groups that handed the console on; `console_mode`, `console_foreground` and `spawn`'s `FOREGROUND` are `EPERM` outside it. A spike ran every scenario under this rule first: all passed, and the one refusal was an orphan of `t-read leave`, which now shows it. The tee calls stay open to all, so a background script's transcript also gets what the screen shows meanwhile (ruled).
- **A Ctrl-C at `wait`** (decision 4). At the prompt the console is raw, where a Ctrl-C is a byte, so `wait` on a running job could never be stopped. `wait` gains `WAIT_CTRL_C`: a raw Ctrl-C ends it with `EINTR`, taken from the input. A new flag changes no meaning, so the ABI stays 3; so does the new struct `ProcInfo` (decision 2).
- **Jobs in bash's words** (decisions 6, 7, 8), checked against bash 5.2 in a terminal: a new job is numbered one past the highest in the table (not the lowest free number), `+` and `-` mark the newest two, the Done, Exit and Killed lines are padded as bash pads them (not spec §9.2's shorter form), Done lines come before every prompt, `wait %n` reports at once and `wait` alone tells only of jobs a signal ended, a fault in bash's words (`Segmentation fault`); a script and `X | sh` start jobs the same way but say neither `[n] pid` nor Done, as bash's non-interactive shells. `kill` takes `-9`, `-KILL` and `-s KILL`; killing is the only signal.
- **Tees for each process** (decision 5): a process pushes 4, the stack holds 64, so background scripts, a transcript's tee each, leave room for more (the review's important finding; the user chose the limits).
- **`ps`** (decision 9) shows what a blocked process waits for (`wait`, `read`, `sleep`, `pipe`), the KiB its address space holds and its CPU time; `/bin` holds 42 programs.
- **Milestone 2's leftovers and plan 1's deferred minor** (decisions 10, 11): the error screen is shown taking the console back from a command that holds it in line mode (Task 7); `pipe()` never closes an end while the table is locked (Task 6); plan 1's other minors go to plans 3 and 4.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran every crate's tests, throwaway QEMU probe scenarios under KVM on the NUC's CPU model (an i7-1260P) and bash 5.2 in a terminal for comparison, and found no critical, 1 important and 11 minor real defects. Each is fixed in a task of its own after the task it concerns, with a test that fails first (or, where an earlier fix settled it, a test that pins it) and the mutation checks that show what it guards; the commit scopes it found were reworded. Two calls it declined to judge went to the user: the tees' limit, and the words for a job a fault ended. It found correct the chain of console holders on every path it tried (nested shells, scripts, `sh | cat`, `cat | sh`, init restarting the shell, the error screen), a background job's refused console calls, `WAIT_CTRL_C` (no wake-up lost, the Ctrl-C taken once), `proc_list` (counts, partial writes, `EFAULT`, the lock order, the frame walk, a full table), `pipe()`'s ends, the spawning runner's fds on every path, the job numbers and marks, and Ctrl-C at the prompt and at `wait`:

| Finding (review) | Decision |
|---|---|
| Important (I1): four background scripts filled the 4-tee stack of the whole machine, so any further script refused to start (`Device or resource busy`) | Fixed, Task 8 (the user chose): a process pushes 4 tees, 64 in all |
| Minor (M1): a bare `wait` let a killed job leave the table without a word; bash tells of it | Fixed, Task 19 |
| Minor (M2): `wait %n`'s Done line went into a redirection; bash's goes to the screen | Fixed, Task 19 |
| Minor (M3): `kill %1` of a job that ended but was not collected was silent, status 0; bash says `No such process`; the fake hid it | Settled by Task 14; the fake corrected and the case pinned, Task 21 |
| Minor (M4): `wait PID` of a reported job's process said `not a child` (127); bash gives its status | Fixed, Task 19: the statuses of jobs gone from the table are kept |
| Minor (M5): `>&2` became bash's syntax error | Fixed, Task 10: `unsupported syntax: >&` |
| Minor (M6): `sh &` printed a stray prompt | Fixed, Task 15: it ends before its first prompt |
| Minor (M7): `jobs 2 %1` listed in the table's order, a repeat once | Fixed, Task 17 |
| Minor (M8): a job that ended while a line was typed kept its process slot while the line ran (EAGAIN at a full table) | Fixed, Task 14 |
| Minor (M9): no test of `holds_ctrl_c`'s group check | Fixed, Task 5: `t-spawn ctrl-c-apart` |
| Minor (M10): `%%`, `%+`, `%-`, `%` were no such job; `kill -0` and `-l` called invalid | Fixed, Task 22: bash's job specs; `-0` and `-l` not supported |
| Minor (M11): commit scopes: Task 4 changed three modules, Tasks 7 and 23 only tests | Reworded: `feat(relay-abi,kernel)` (relay-rt's wrapper of the flag goes with it), `test(e2e,userland)`, `test(e2e)` |
| Declined to judge: a faulted job said `Killed`; a raw Ctrl-C woke a plain waiter on every idle pass; a background script's transcript gets the prompt's typing | The user chose bash's words, Task 12; fixed, Task 5; ruled (decision 3) |

## Where this plan fits

Plan 2 of milestone 3 implements spec §13 step 7 (the cut line of §1.2, which stays in milestone 3). It builds on plan 1: pipelines in one process group, `spawn` joining a child's group, standard input. It leaves plan 3 (script arguments and variables) a shell whose `wait` and jobs set its last status, which `$?` will read, and plan 4 `check5.sh`'s background job and `kill`.

## Working conventions

- Plan 2 lands as **four pull requests** (table below). This plan, with the spec's §16 item 9 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-2-jobs-ps-and-kill.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`; test programs under `userland/tests/`, with the `relay-rt` calls they need), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 2, 3, 4, 15 and 25 have scenario red runs too. Tasks 7, 21 and 23 have no failing run: they test what exists, and their introductions say what the mutation checks showed. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m3p2/mutations.md`).
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; a console read or a `wait` can wait for ever. Every guest loop here has a bound; the shell's collecting stops after the table's 64. Mutation checks run under an address-space limit and a timeout (`tmp/m3p2/mutate.py`).
- **The expectations are bash 5.2's, in a terminal.** An interactive bash numbers jobs, prints Done lines and answers `wait` differently from `bash -c` and scripts; `tmp/m3p2/pty_bash.py` drives one in a pty, and what it printed is in `tmp/m3p2/progress.md`. Where this shell departs (killing is the only signal: `Killed`, never `Terminated`), the decision says so.
- **The NUC check scripts must not change**: this plan has no NUC check, and `check3-a.sh` runs `t-read apart` (its output stays as it was; the new refusals are `t-read refused`'s).
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests, scenarios and transcripts that come with the change, and the implementations a changed trait forces elsewhere (`System::processes` in `relay-rt` and `host-shell`), belong to its commit; a task that only adds tests names the module whose behaviour they show. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `f0b8662` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m3p2/plan` | — | The spec's §16 item 9 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m3p2/kernel` | 1–8 | `ProcInfo` and `proc_list`; the console's holders; `WAIT_CTRL_C` and the waits it wakes; `pipe()`'s ends; the error screen's test; 4 tees a process, 64 in all | `lint`, `unit`, `e2e` |
| 3 | `m3p2/shell` | 9–23 | The parser's `&` and `>&`; the job table and a fault's words; background jobs, collected before each line, and their Done lines; `sh &`; `jobs`, `wait`, `kill` and bash's job specs; the scenario `jobs` | `lint`, `unit`, `e2e` |
| 4 | `m3p2/utils` | 24–25 | `ps` and its program | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 2 adds no crate. The kernel has no dev-dependencies and does not depend on `shell`; `crates/usb` has no dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail, and keeps the same values on every architecture. `relay_abi::VERSION` stays 3: a new call's struct and a new flag change no meaning (gate §7.4).
- Every command prints exactly what it prints in milestone 1 for files, and every milestone 1 scenario passes unchanged but for the startup line (gate §1.4). The NUC check scripts and their recorded transcripts do not change.
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. Nothing waits for ever but the error screen and a program waiting for its own input. Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table. A pipe's end is never dropped while `PROCS` is held, nor is console output made under it (it may write the tees).
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 120×33 cells and no serial port.
- Shell messages follow bash 5.2 (an interactive one, for jobs) and GNU coreutils; `/bin/sh` names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 9 in PR 1:

1. **Plan 2 is one plan** (§13) in four pull requests: this plan; the kernel's `proc_list`, the console calls' refusal, `wait`'s Ctrl-C and the error screen's test; the shell's background jobs, `jobs`, `wait` and `kill`; `ps`. It has no NUC check: nothing it changes is particular to the NUC (QEMU's USB keyboard types the Ctrl-C a `wait` needs), the stick keeps 0.3.0 until plan 4's `flash --full`, and plan 4's `check5.sh` runs a background job and `kill` there.
2. **`proc_list`** (§7.3, §9.3; `relay_abi::proc`). It fills a buffer with a `ProcInfo` per process, by pid (96 bytes: pid, ppid, pgid and state as `u32`, frames and CPU ticks as `u64`, the name in 64 bytes padded with NULs), as many whole entries as fit, and returns how many processes there are, so a short buffer is no error and says what it missed (`PROC_MAX`, 64, always fits); nothing is written unless all that fits can be (`EFAULT`). A new struct changes no layout, and `VERSION` stays 3. The state says what a blocked process waits for: `STATE_RUN`, `READY`, `WAIT` (a child), `READ` (the console), `SLEEP`, `PIPE`, `ZOMBIE`. The frames are its address space's (its pages, their tables and its PML4, counted by walking the tables as they are given back), 0 for a zombie and for process 1, which has none; the name is the path `spawn` was given, cut at 64 bytes.
3. **Who may change the console** (§6.4, §7.3; `kernel/src/proc/holders.rs`). The process table keeps the chain of groups that handed the console on, from process 1's at the bottom to the foreground group at the top. `spawn`'s `FOREGROUND` and `console_foreground` put a group on top, cutting the chain back to the giver's group first, and a group taking the console back (its own, or one below it) is cut back to; the error screen gives it to process 1 alone; groups with no process left leave it, so it holds at most one entry per group. `console_mode`, `console_foreground` and a `spawn` with `FOREGROUND` are `EPERM` for a caller whose group is not in it (`console_foreground` says `ESRCH` first for a group with no live process). So a shell takes the console back after its command's group has ended and been collected (it switches the mode while that group is still on top), nested shells and scripts each take it from theirs, and a background job, never given it, can neither take it nor change its mode: `sh script &` and `sh &` leave the console alone. A spike ran every scenario under this rule first: all passed, and the one refusal was `t-read leave`'s orphan, which now shows it. The tee calls stay open to any process: a tee changes nothing a process reads, and refusing them would stop `sh s &` before its first line; so a background script's transcript also gets what the screen shows meanwhile, the prompt's typing included (ruled).
4. **`wait` and Ctrl-C** (§7.3, §9.2). At its prompt the shell has the console in raw mode, where a Ctrl-C is only a byte, so nothing could stop `wait` while a job ran on. `wait` gains `WAIT_CTRL_C`: a Ctrl-C typed while the caller's group has the console in raw mode ends the wait with `EINTR` and is taken from the input, and that group's waiters are woken when one comes. Without the flag a raw Ctrl-C stays input. A wait with the flag blocks in a state of its own, the only one a Ctrl-C wakes (the review found a plain waiter woken on every pass of the idle task while a raw Ctrl-C waited unread). A flag no program passed before (it was `EINVAL`) adds to the ABI and changes no meaning, so `VERSION` stays 3.
5. **Tees** (§6.5, corrected in its body; §16 items 4 and 5). A process pushes at most 4 tees, and the stack holds 64, one for each process the table can hold: with 4 for the whole machine, the review found four background scripts, a transcript's tee each, kept any further script from starting (`Device or resource busy`). So nested scripts are no longer stopped at four deep (§16 item 5).
6. **Background jobs** (§9.2; `crates/shell/src/jobs.rs`). An unquoted `&` at the end of a line (a comment may follow) runs it in the background; elsewhere it gets bash's syntax error (`&` alone, `a | &`, `echo > &`, `a & &`) or is refused as unsupported (`a & b`, `&&`, `> f &`, which bash runs). A job's commands start as a pipeline's do, the first that starts in a new group without the console (`Group::Background`), at the prompt and in a script alike, so a script's Ctrl-C does not reach them (bash's jobs ignore SIGINT there); the others join it, and nothing waits for them. At the prompt the shell says `[n] <pid of the last command>`; it collects what ended with `wait(-1, NOHANG)` before each prompt and before running each line typed (so a job that ended meanwhile leaves its slot in the process table to that line, which the review found refused at a full table), and says how each finished job ended before the next prompt (after a foreground command too); a script and `X | sh` collect before each line and say neither, as bash's non-interactive shells do. A job none of whose commands started is no job (the status the last one's, 127); once one started the status is 0. `cd`, `exit`, `help`, `jobs`, `wait` and `kill` cannot run in the background (`relay-sh: cd: cannot be used in the background`, status 1). The in-process runner, which has no programs, keeps refusing `&` (`unsupported syntax: &`). `>&2` and `>& f`, which bash runs, are `unsupported syntax: >&`, `>>&` and `> &` bash's syntax errors. `exit` leaves running jobs to process 1, as bash's does without `huponexit`. An interactive `/bin/sh` whose group was never given the console (`sh &`) ends at once with 0, before its first prompt (its leader test says `EPERM` exactly then); bash stops such a shell, which needs job control this gate leaves out.
7. **The job table** (§9.2). A new job is numbered one past the highest in the table, so numbers start again only once it is empty: bash 5.2 numbers 4 and 5 after jobs 1 and 2 ended, not the lowest free one. The newest job is `+`, the one before it `-`. A job has ended when every process it started has, and its status is its last one's. Its text is the line as typed, without the `&`. Its lines are bash's, the state padded to 24 columns (`[1]+  Done                    sleep 5`, `Exit 1`, `Running` and the text with ` &`), rather than §9.2's shorter form (corrected in its body). A job a signal ended says bash's words for it, which the statuses already follow: `Killed` for `kill`, `Segmentation fault` for a page fault, a protection fault or a stack overflow, `Illegal instruction`, `Floating point exception`, `Interrupt`; the kernel log keeps the fault's detail. bash pads to 24 and no further, so its `Floating point exception` meets the text; a blank stays between here (ruled). Job specs are bash's: `%n`, and `%%`, `%+` or `%` for the current job, `%-` for the previous one (the current one when it is alone).
8. **`jobs`, `wait` and `kill`** (§8.3, §9.2, §9.3). Built-ins, since they need the job table: a built-in's `Ctx` gets the shell's job control (its jobs and, under `/bin/sh`, its programs), and `help` lists them. `jobs [%n | n]...` lists every job or those named, in the order named (one named twice twice), and a job that has ended says how there once and leaves the table. `wait` waits for every job, which then leave the table, saying nothing of those that exited and telling of those a signal ended, as bash's does; `wait %n` or `wait PID` waits for one, its status that job's last process's or that process's own, and at the prompt a job that ends there says how at once. These notices go to the screen, never into a redirection. The table keeps the statuses of the processes of jobs that left it (the table's 64 at most), so `wait PID` of one answers once, as bash's does. A Ctrl-C at the prompt ends a `wait` (`^C`, 130), the jobs running on. `kill PID...` and `kill %n...` kill processes or a job's whole group (`kill(-pgid)`), the status 1 if any was not killed; killing is the only signal (§15): `-9`, `-KILL`, `-SIGKILL` and `-s KILL` are taken, another signal Linux has (and `-0`, which tests that a process exists, and `-l`, which lists the signals) is `not supported`, and a name it has not is an `invalid signal specification`. Their messages are bash's with `relay-sh:`: `kill: (1) - Operation not permitted`, `kill: (999) - No such process`, `kill: %3: no such job`, `kill: abc: arguments must be process or job IDs`, `wait: pid 9 is not a child of this shell` and `wait: %3: no such job` (127), ``wait: `abc': not a pid or valid job spec`` (1), and an option `invalid option` (2). Until plan 3's `$?`, their status is the shell's last status, which `exit` and a script's end take.
9. **`ps`** (§9.3; `commands/system.rs`, `/bin/ps`). A command function over the shell's `System`, which gains `processes` (from `proc_list`; none on the host, where `ps` says `processes are not available` as `free` says of memory figures), and a program like the others: `/bin` holds 42. Its columns are procps's, right-aligned numbers: `  PID  PPID STATE      MEM  TIME CMD`; STATE `run`, `ready`, `wait`, `read`, `sleep`, `pipe` or `zombie`; MEM the KiB of the frames its address space holds; TIME its CPU time as `m:ss`; CMD the path it was started from. Process 0, the idle task, is no process.
10. **The error screen takes the console back** (§11.2; milestone 2's leftover). The scenario `screen_console` ends the shell a third time within 10 s while its command, `t-proc end-shell`, reads the console in line mode and its child kills the shell; the screen ends every other process, gives the console to process 1 in raw mode, and a key restarts the machine, which it would not in line mode, where the key waits in the line discipline.
11. **Plan 1's deferred minors.** `pipe()` puts copies of its ends in the fd table, both or neither, so the last reference to an end that did not go in is dropped after the table is unlocked, by construction rather than because two free fds were checked first. `X | sh` reading its input 4 KiB at a time goes to plan 3, which reads scripts; `seq`'s zero increment and `[a-a-`'s message go to plan 4.
12. **Tests** (§8.5, §12). `t-proc` (`list`, `short`, `long`, `end-shell`) and the scenarios `proc_calls`, `screen_console` and `jobs`; `t-tee owners` in `tees`; `t-spawn ctrl-c` and `ctrl-c-apart` (a wait apart from the console never takes its Ctrl-C) in `ctrlc`, which now waits for the prompt before typing into the next command (a spike found the line could reach a command's group that had just ended); `t-read refused` and `leave` show the refusals in `console` (`t-read apart`, which `check3-a.sh` runs on the NUC, prints what it printed). Host tests: the chain of holders, the job table against bash 5.2's own lines, and the shell's jobs over `FakePrograms`, whose children can outlive a prompt.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **Typing at the prompt while jobs run:** Ctrl-C at the prompt, a job that reads the console, a job's output arriving while a line is half typed, the Done lines between prompts, a job that ends while the next line is typed, a job started in a script, `sh &`. Expected: Ctrl-C cancels only the line, a job reading the console gets end of input at once, nothing a job does takes the console from the prompt, each finished job is reported once, before a prompt, in bash's words (a script's never), and its slot in the process table is free for the next line. Tests: `jobs` (Task 23), `a_background_job_says_its_number_and_how_it_ended_before_a_prompt` and `a_script_and_x_into_sh_say_nothing_of_their_jobs` (Task 13), `a_job_that_ended_while_a_line_was_typed_is_collected_before_it_runs` (Task 14), `sh`'s `sh &` (Task 15), `console`'s `t-read apart` and `refused` (Task 3).
2. **Waiting for and ending jobs:** `wait` with Ctrl-C, `wait %n` and `wait PID` of a process that has already ended or was already reported, a bare `wait` after `kill`, `kill %n` of a pipeline, `kill` of a job that has ended, `kill 1`, `exit` with jobs running. Expected: Ctrl-C ends a `wait` (`^C`, 130) and the jobs run on; each status and notice is bash's, notices on the screen; every process of a killed job ends; nothing waits for ever; jobs left by `exit` pass to process 1. Tests: `ctrl_c_ends_a_wait_and_the_jobs_run_on` and `wait_for_a_job_or_a_pid_says_how_it_ended_and_takes_its_status` (Task 18), `a_bare_wait_tells_of_a_killed_job_on_the_screen` and `wait_for_a_reported_job_s_pid_gives_its_status_once` (Task 19), `kill_ends_a_job_s_whole_group_or_a_process` and `kill_says_what_it_could_not_kill_as_bash_s_does` (Task 20), `kill_of_a_job_that_has_ended_says_it_is_no_process_as_bash_s_does` (Task 21), `ctrlc`'s `t-spawn ctrl-c` (Task 4), `jobs` (Task 23).
3. **Who holds the console:** a nested shell, a script, `sh script &`, `sh &`, `X | sh`, a background job trying to change the console or waiting for a Ctrl-C, the shell after its command was killed, init starting a new shell while the last one's command still reads, the error screen. Expected: every shell takes the console back from its own commands, a group never given it gets `EPERM` and never takes its Ctrl-C, and the error screen gives it to process 1 in raw mode. Tests: `a_holder_gives_it_on_and_takes_it_back`, `a_group_never_given_the_console_cannot_take_it` and `groups_that_are_gone_leave_the_chain_but_process_1_s` (Task 3), `console` (Task 3), `ctrlc`'s `t-spawn ctrl-c-apart` (Task 5), `screen_console` (Task 7), `a_command_s_group_is_the_shell_s_a_new_one_or_its_first_stage_s` (Task 13).
4. **Job numbers and lines against bash's:** jobs started and ended in any order, a job whose first or last command cannot start, a pipeline's status, a job a fault ended, `jobs` naming jobs in any order, `%%`, `%+`, `%-`, markers `+` and `-`. Expected: bash 5.2's numbers, markers, padding and words. Tests: `jobs_are_numbered_one_past_the_highest_as_bash_numbers_them`, `the_lines_are_bash_s` and `a_pipeline_s_status_is_its_last_process_s` (Task 11), `a_job_a_fault_ended_says_bash_s_words_for_its_signal` (Task 12), `a_background_job_that_cannot_start_is_no_job` (Task 13), `jobs_names_jobs_as_bash_s_does` and `a_job_jobs_does_not_name_is_reported_at_the_prompt` (Tasks 16 and 17), `the_current_and_previous_jobs_are_named_as_bash_names_them` (Task 22).
5. **`ps` and the kernel's limits:** a full process table, a short buffer, a name longer than 64 bytes, zombies, a process's state while the list is made, big MEM and TIME figures, many background scripts each with a transcript. Expected: every process once, by pid, with what it waits for, columns that stay readable, and a fifth script still starting. Tests: `proc_list_gives_what_fits_and_how_many_there_are` and `proc_list_writes_nothing_into_memory_that_is_not_all_the_program_s` (Task 2), `the_list_has_every_process_by_pid_with_its_state` (Task 2), `proc_calls` (Task 2), `ps_lists_every_process_with_what_it_does` (Task 24), `other_processes_push_theirs_until_64_are_pushed` and `tees`' `t-tee owners` (Task 8), `jobs`' four background scripts (Task 23).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/relay-abi/src/{proc,spawn,lib}.rs` | `ProcInfo`, the states, `PROC_MAX`; `WAIT_CTRL_C` |
| `kernel/src/proc/holders.rs` | The chain of groups that may change the console |
| `kernel/src/proc/table.rs`, `kernel/src/proc.rs` | `State::number`, `Table::list`, the console's holders in the table, `wake_waiting`; `proc_list`'s processes, the console calls' refusals, `spawn`'s `FOREGROUND` check, `WAIT_CTRL_C` in `collect` |
| `kernel/src/syscall.rs`, `kernel/src/syscall/{testing,pipes}.rs` | The `proc_list` call, `console_mode`'s `EPERM`, `wait`'s flag; the dispatcher's fake; `pipe()`'s ends both or neither |
| `kernel/src/{input,tty,tee}.rs`, `kernel/src/mm/{paging,space}.rs` | A raw Ctrl-C taken from the input; 4 tees a process, 64 in all; an address space's frames |
| `crates/relay-rt/src/{sys,sysio,sysvfs}.rs` | `sys::proc_list`, `sys::wait_with`; `SysPrograms`' background groups (`spawn_group`), `collect`, `wait_or_ctrl_c`, `kill`; `SysSystem::processes` |
| `crates/shell/src/{parser,jobs,io,ctx,runner,shell,lib}.rs` | `parse_line` and `Line`; the job table; `Group::Background`, `Programs::{collect, wait_or_ctrl_c, kill}`, `System::processes`; `JobControl`; `Runner::background`; jobs at the prompt and in scripts |
| `crates/shell/src/commands/{control,system,mod}.rs`, `crates/shell/src/testing.rs` | `jobs`, `wait`, `kill`; `ps`; the built-ins and `help`; `FakePrograms`' children that outlive a prompt, `TestSystem::processes` |
| `userland/tests/src/bin/{t-proc,t-read,t-spawn,t-tee}.rs`, `userland/utils/src/bin/ps.rs`, `userland/sh/{Cargo.toml,src/main.rs}` | `t-proc`; `t-read refused` and `leave`; `t-spawn ctrl-c` and `ctrl-c-apart`; `t-tee owners`; `/bin/ps`; `sh &` ending before its prompt |
| `xtask/src/host_shell.rs` | `host-shell`'s `System::processes` |
| `tests/e2e/{proc_calls,screen_console,jobs,console,ctrlc,tees,sh,system}.txt` | The new scenarios, and what the refusals, `WAIT_CTRL_C`, the tees' limits, `sh &` and the new programs change |
| `docs/hardware-test.md`, `kernel/src/system.rs` | `system: 42 programs, ABI 3` |

---

## PR 1: The spec's revisions, the roadmap's notes and this plan

The spec's §16 item 9 with the parts of its body it corrects (the status line, §4.3's startup line, §6.4's foreground group, §7.3's `wait`, `proc_list`, `console_mode` and `console_foreground`, §8.5's `t-spawn`, `t-read` and `t-proc`, §9.2's Done and `jobs` lines, §9.3's `ps` and `kill`, §12.3's `proc_calls` and `screen_console`); the roadmap's notes on plan 1's deferred minors and plan 2's NUC check; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record plan 2's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p2/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m3p2/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-2-jobs-ps-and-kill.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-01-m3-plan-2-jobs-ps-and-kill.md
git commit -m "docs(plan): add milestone 3 plan 2, jobs, ps and kill"
cargo xtask lint
git push -u origin m3p2/plan
gh pr create --base main --head m3p2/plan --title "docs(spec,plan): add milestone 3's plan 2, jobs, ps and kill" --body-file - <<'EOF2'
## What

The implementation plan of milestone 3's plan 2 ("jobs, `ps` and `kill`"), and the spec's §16 item 9 with its decisions: `proc_list` and `ProcInfo`; the chain of process groups that may change the console, so that a background job can neither take it nor change its mode; `wait`'s `WAIT_CTRL_C`, so that a Ctrl-C at the prompt ends the `wait` built-in; background jobs and their table in bash 5.2's words; the built-ins `jobs`, `wait` and `kill`; `ps`; the error screen's test; the corrections to the spec's body they bring. The roadmap places plan 1's deferred minors and says plan 2 has no NUC check.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m3p2/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m3p2/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-plan` and continue with PR 2.

---

## PR 2: `proc_list`, the console's holders and `wait`'s Ctrl-C (Tasks 1–8)

The kernel and `relay-rt`'s side of jobs: `ProcInfo` and the `proc_list` call; the console changed only by a group it was handed to; `wait`'s `WAIT_CTRL_C`, whose Ctrl-C wakes only the waits that asked; a new pipe's ends never closed under the process table's lock (plan 1's deferred minor); the error screen shown taking the console back; and tees counted for each process, so that background scripts leave room for more. No shell starts a background job yet; `t-proc`, `t-read refused`, `t-spawn ctrl-c` and `t-tee owners` show it all in QEMU.

Branch `m3p2/kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p2/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p2/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p2/kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-kernel m3p2/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p2/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: `ProcInfo`, and the frames an address space holds

Spec §7.3, §9.3 and decision 2: `proc_list` reports one `relay_abi::ProcInfo` per process, 96 bytes with no padding: pid, ppid, pgid and state (`u32`), the frames its address space holds and the timer ticks it ran for (`u64`), and the path it was started from in 64 bytes padded with NULs, cut at 64 (`ProcInfo::new`, `name`, `to_bytes`, `from_bytes`). `relay_abi::proc` numbers the states so that `ps` can say what a blocked process waits for: `STATE_RUN`, `READY`, `WAIT` (a child), `READ` (the console), `SLEEP`, `PIPE`, `ZOMBIE`; `PROC_MAX` is the table's 64. A new struct changes no layout, so `VERSION` stays 3. The kernel's `AddressSpace::frames` counts its pages, their tables and its PML4 by walking the tables as `free_lower_half` gives them back (`PageTables::lower_half_frames`), so the count is what `destroy` will free. Nothing uses either yet. The red runs are `relay-abi`'s and the kernel's tests, which cannot find the new items. Mutation checks: a page table's pages not counted, a table not counting itself, the PML4 left out, the name not in the bytes and the name cut at 63 each fail a test.

**Files:**
- Modify: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-abi/src/proc.rs`
- Modify: `kernel/src/mm/paging.rs`
- Modify: `kernel/src/mm/space.rs`

**Interfaces:**
- Consumes: plan 3b's `AddressSpace`, `PageTables`.
- Produces: `relay_abi::ProcInfo` (`SIZE` 96; `new(pid, ppid, pgid, state, frames, ticks, name: &[u8])`, `name(&self) -> &[u8]`, `to_bytes`, `from_bytes`), `relay_abi::proc::{PROC_MAX, PROC_NAME, STATE_RUN, STATE_READY, STATE_WAIT, STATE_READ, STATE_SLEEP, STATE_PIPE, STATE_ZOMBIE}`; `AddressSpace::frames(&self, mem) -> u64`, `PageTables::lower_half_frames(&self, mem) -> u64`.

- [ ] **Step 1: Declare the new module in `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub mod power;
mod result;
````

with:

````rust
pub mod power;
pub mod proc;
mod result;
````

- [ ] **Step 2: Write the failing tests for `crates/relay-abi/src/proc.rs`**

Create `crates/relay-abi/src/proc.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<ProcInfo>(), 96);
        assert_eq!(ProcInfo::SIZE, 96);
        assert_eq!(offset_of!(ProcInfo, pid), 0);
        assert_eq!(offset_of!(ProcInfo, ppid), 4);
        assert_eq!(offset_of!(ProcInfo, pgid), 8);
        assert_eq!(offset_of!(ProcInfo, state), 12);
        assert_eq!(offset_of!(ProcInfo, frames), 16);
        assert_eq!(offset_of!(ProcInfo, ticks), 24);
        assert_eq!(offset_of!(ProcInfo, name), 32);
        assert_eq!((PROC_MAX, PROC_NAME), (64, 64));
        assert_eq!(
            [
                STATE_RUN,
                STATE_READY,
                STATE_WAIT,
                STATE_READ,
                STATE_SLEEP,
                STATE_PIPE,
                STATE_ZOMBIE
            ],
            [1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn its_bytes_are_the_struct_s_and_its_name_is_cut_at_64() {
        let p = ProcInfo::new(7, 2, 5, STATE_SLEEP, 41, 1234, b"/bin/sleep");
        assert_eq!(p.name(), b"/bin/sleep");
        // SAFETY: `repr(C)` of integers and bytes, no padding.
        let mem: [u8; ProcInfo::SIZE] = unsafe { core::mem::transmute(p) };
        assert_eq!(p.to_bytes(), mem);
        assert_eq!(ProcInfo::from_bytes(&mem), p);
        let long = [b'x'; 70];
        assert_eq!(
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 0, &long).name(),
            &long[..64]
        );
    }
}
````

- [ ] **Step 3: Add the failing tests to `kernel/src/mm/space.rs`**

In `kernel/src/mm/space.rs`, replace:

````rust
    #[test]
    fn running_out_of_frames_midway_leaks_nothing() {
````

with:

````rust
    #[test]
    fn a_space_counts_every_frame_it_holds() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames() as u64;
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        assert_eq!(s.frames(&mut m), 1, "only its PML4");
        s.map_zeroed(&mut m, U, 3, Perm::ReadExec).unwrap();
        // Another table of each level for a page far away.
        s.map_zeroed(&mut m, 0x7FFF_FFF0_0000, 1, Perm::ReadWrite)
            .unwrap();
        let a = s.map_area(&mut m, 600, u64::MAX).unwrap();
        assert_eq!(s.frames(&mut m), m.frames() as u64 - before);
        s.unmap_area(&mut m, a, 600).unwrap();
        assert_eq!(s.frames(&mut m), m.frames() as u64 - before);
        s.destroy(&mut m);
    }

    #[test]
    fn running_out_of_frames_midway_leaks_nothing() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` cannot find type `ProcInfo` in this scope ``; `` cannot find value `PROC_MAX` in this scope ``.

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `frames` found for struct `space::AddressSpace` in the current scope ``.

- [ ] **Step 5: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub use info::{MemInfo, Time, Uname};
pub use result::{MAX_ERRNO, decode, encode};
````

with:

````rust
pub use info::{MemInfo, Time, Uname};
pub use proc::ProcInfo;
pub use result::{MAX_ERRNO, decode, encode};
````

- [ ] **Step 6: Implement `crates/relay-abi/src/proc.rs`**

Insert this at the top of `crates/relay-abi/src/proc.rs`, above `#[cfg(test)]`:

````rust
//! What `proc_list` fills in (spec §7.3, §9.3): one [`ProcInfo`] per
//! process, for `ps`.

/// The most processes that exist at once (spec §5.4), so a buffer of this
/// many entries always holds every one.
pub const PROC_MAX: usize = 64;
/// The length of [`ProcInfo::name`].
pub const PROC_NAME: usize = 64;

/// [`ProcInfo::state`]: it runs now (the caller of `proc_list` does).
pub const STATE_RUN: u32 = 1;
/// [`ProcInfo::state`]: it waits for its turn on the CPU.
pub const STATE_READY: u32 = 2;
/// [`ProcInfo::state`]: it waits in `wait` for a child to end.
pub const STATE_WAIT: u32 = 3;
/// [`ProcInfo::state`]: it waits for what is typed on the console.
pub const STATE_READ: u32 = 4;
/// [`ProcInfo::state`]: it waits in `sleep`.
pub const STATE_SLEEP: u32 = 5;
/// [`ProcInfo::state`]: it waits for data in a pipe, or room in one.
pub const STATE_PIPE: u32 = 6;
/// [`ProcInfo::state`]: it has ended, and its parent has not collected it.
pub const STATE_ZOMBIE: u32 = 7;

/// One process, as `proc_list` reports it, `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    /// Its parent (process 1 for an orphan, 0 for process 1).
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
    /// One of the `STATE_*` numbers.
    pub state: u32,
    /// The frames of memory its address space holds: its pages and their
    /// page tables (0 for a zombie and for process 1, which has none).
    pub frames: u64,
    /// The timer ticks (milliseconds) it has run for.
    pub ticks: u64,
    /// The path it was started from, padded with NULs, cut at
    /// [`PROC_NAME`] bytes.
    pub name: [u8; PROC_NAME],
}

impl ProcInfo {
    pub const SIZE: usize = core::mem::size_of::<ProcInfo>();

    /// A `ProcInfo` whose name is `name`, cut at [`PROC_NAME`] bytes.
    pub fn new(
        pid: u32,
        ppid: u32,
        pgid: u32,
        state: u32,
        frames: u64,
        ticks: u64,
        name: &[u8],
    ) -> ProcInfo {
        let mut field = [0; PROC_NAME];
        let n = name.len().min(PROC_NAME);
        field[..n].copy_from_slice(&name[..n]);
        ProcInfo {
            pid,
            ppid,
            pgid,
            state,
            frames,
            ticks,
            name: field,
        }
    }

    /// Its name, without the padding.
    pub fn name(&self) -> &[u8] {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(PROC_NAME);
        &self.name[..end]
    }

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; ProcInfo::SIZE] {
        let mut b = [0; ProcInfo::SIZE];
        for (i, v) in [self.pid, self.ppid, self.pgid, self.state]
            .iter()
            .enumerate()
        {
            b[4 * i..4 * i + 4].copy_from_slice(&v.to_ne_bytes());
        }
        b[16..24].copy_from_slice(&self.frames.to_ne_bytes());
        b[24..32].copy_from_slice(&self.ticks.to_ne_bytes());
        b[32..].copy_from_slice(&self.name);
        b
    }

    /// The struct whose bytes are `b`.
    pub fn from_bytes(b: &[u8; ProcInfo::SIZE]) -> ProcInfo {
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        ProcInfo {
            pid: u32_at(0),
            ppid: u32_at(4),
            pgid: u32_at(8),
            state: u32_at(12),
            frames: u64_at(16),
            ticks: u64_at(24),
            name: b[32..].try_into().unwrap(),
        }
    }
}

````

- [ ] **Step 7: Change `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Maps one 2 MiB page. `Ok(false)` if that slot already holds a table
````

with:

````rust

    /// The frames of the lower half: every page `map_user` put there and
    /// every table below its PML4 entries (not the PML4 itself), as
    /// `free_lower_half` would give them back.
    pub fn lower_half_frames(&self, mem: &mut impl PhysMem) -> u64 {
        let mut frames = 0;
        for i in 0..256 {
            let pdpt = mem.table(self.pml4)[i];
            if pdpt & PRESENT != 0 {
                frames += table_frames(mem, pdpt & ADDR, 2);
            }
        }
        frames
    }

    /// Maps one 2 MiB page. `Ok(false)` if that slot already holds a table
````

Replace:

````rust
    mem.free_frame(table);
}
````

with:

````rust
    mem.free_frame(table);
}

/// The frames `free_table` would give back for the table at `table` of
/// `level`: it, the tables below it and, from a PT, the pages it maps.
fn table_frames(mem: &mut impl PhysMem, table: u64, level: u32) -> u64 {
    let mut frames = 1;
    for i in 0..512 {
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            continue;
        }
        frames += match level {
            0 => 1,
            _ => table_frames(mem, e & ADDR, level - 1),
        };
    }
    frames
}
````

- [ ] **Step 8: Change `kernel/src/mm/space.rs`**

In `kernel/src/mm/space.rs`, replace:

````rust
        &self.maps
    }
````

with:

````rust
        &self.maps
    }

    /// The frames it holds: its pages, their tables and its PML4, for
    /// `ps` (spec §9.3).
    pub fn frames(&self, mem: &mut impl PhysMem) -> u64 {
        1 + self.tables.lower_half_frames(mem)
    }
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 25 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 374 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates kernel
git commit -F - <<'EOF'
feat(relay-abi,kernel): add ProcInfo and count an address space's frames

proc_list (spec §7.3, §9.3) reports one ProcInfo per process: its pid,
parent, group, state, the frames its address space holds, the ticks it
ran for and its name, 96 bytes with no padding. relay_abi::proc numbers
the states, run, ready, wait, read, sleep, pipe and zombie, so that ps
can say what a blocked process waits for. An address space counts its
frames by walking its tables as free_lower_half gives them back: its
pages, their tables and its PML4.
EOF
````


### Task 2: The `proc_list` call

Spec §7.3, §9.3 and decision 2: `proc_list(buffer, length)`, `ENOSYS` until now, fills the buffer with a `ProcInfo` per process, by pid, as many whole entries as fit, and returns how many processes there are, so a short buffer is no error and says what it missed; nothing is written unless all that fits can be (`EFAULT`). The table lists its processes in the order they came, which is by pid since pids only count up (`Table::list`, with each state's number from `State::number`); the kernel's `Current::processes` takes `PROCS`, then `MEMORY` for the frames (the lock order everywhere), and process 1, which has no address space, holds 0. `relay-rt` gains `sys::proc_list` (before the red run: `t-proc` needs it). `t-proc list` starts four children, one sleeping, one that ends at once, one waiting on an empty pipe and one reading the console in its group, waits for each to get there (a second at most), and prints what the list says of them, of itself, its parent the shell and process 1; `short` gives a buffer of one entry and of none; `long` starts `/bin/sleep` by a path of 90 bytes (`/bin/` and forty `./`) and sees its first 64. The new scenario `proc_calls` runs them, and `system` lists `/bin` with `t-proc` (41 programs). The red runs are the kernel's tests, which cannot compile, and `proc_calls`, which gets `ENOSYS`. Mutation checks: the count replaced by what fit, an entry more than fits, the call not served, the console's or the running state misnumbered and the pid given as the group each fail a test; every process holding no frames fails `proc_calls`.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/system.rs`
- Create: `tests/e2e/proc_calls.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-proc.rs`

**Interfaces:**
- Consumes: Task 1's `ProcInfo` and `AddressSpace::frames`.
- Produces: `Caller::processes(&mut self) -> Vec<ProcInfo>`; `table::State::number(self) -> u32`; `Table::list(&self, frames: impl FnMut(&Process<R>) -> u64) -> Vec<ProcInfo>`; `relay_rt::sys::proc_list(buf: &mut [ProcInfo]) -> Result<usize, u16>`; the test program `t-proc` (`list`, `short`, `long`).

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Stat, StatFs, WaitStatus, decode};

````

with:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Stat, StatFs, WaitStatus, decode};

````

Replace:

````rust

/// This program's pid.
````

with:

````rust

/// Fills `buf` with a `ProcInfo` per process, by pid, as many as fit
/// (`relay_abi::proc::PROC_MAX` entries always hold them all); how many
/// processes there are.
pub fn proc_list(buf: &mut [ProcInfo]) -> Result<usize, u16> {
    let len = core::mem::size_of_val(buf) as u64;
    call(Call::ProcList, &[buf.as_mut_ptr() as u64, len]).map(|n| n as usize)
}

/// This program's pid.
````

- [ ] **Step 2: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

with:

````rust
    #[test]
    fn the_list_has_every_process_by_pid_with_its_state() {
        let mut t = table();
        let init = add(&mut t, 0, true);
        let a = t
            .insert(init, Group::New, String::from("/bin/a"), ())
            .unwrap();
        let b = add(&mut t, a, false);
        let c = add(&mut t, a, true);
        let d = add(&mut t, a, true);
        let e = add(&mut t, init, true);
        assert_eq!(t.schedule(), init);
        t.block(Blocked::Wait);
        assert_eq!(t.schedule(), a);
        t.tick();
        t.tick();
        t.block(Blocked::Console);
        assert_eq!(t.schedule(), b);
        t.block(Blocked::Sleep(99));
        assert_eq!(t.schedule(), c);
        t.block(Blocked::Pipe(7));
        assert_eq!(t.schedule(), d);
        t.end(d, WaitStatus::exited(0));
        assert_eq!(t.schedule(), e);
        let list = t.list(|p| u64::from(p.pid) * 10);
        let got: Vec<(u32, u32, u32, u32, u64, u64)> = list
            .iter()
            .map(|p| (p.pid, p.ppid, p.pgid, p.state, p.frames, p.ticks))
            .collect();
        assert_eq!(
            got,
            [
                (init, 0, init, STATE_WAIT, 10, 0),
                (a, init, a, STATE_READ, 20, 2),
                (b, a, a, STATE_SLEEP, 30, 0),
                (c, a, c, STATE_PIPE, 40, 0),
                (d, a, d, STATE_ZOMBIE, 50, 0),
                (e, init, e, STATE_RUN, 60, 0),
            ]
        );
        assert_eq!(list[1].name(), b"/bin/a");
        t.block(Blocked::Wait);
        t.wake(e);
        assert_eq!(t.list(|_| 0)[5].state, STATE_READY);
    }

    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn a_bad_status_pointer_loses_no_child() {
````

with:

````rust
    #[test]
    fn proc_list_gives_what_fits_and_how_many_there_are() {
        use relay_abi::proc::{STATE_RUN, STATE_SLEEP, STATE_WAIT};
        let mut f = fake();
        f.procs = alloc::vec![
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 5, b"init"),
            ProcInfo::new(2, 1, 2, STATE_WAIT, 300, 70, b"/bin/sh"),
            ProcInfo::new(9, 2, 9, STATE_RUN, 41, 2, b"/bin/ps"),
            ProcInfo::new(12, 2, 12, STATE_SLEEP, 40, 0, b"/bin/sleep"),
        ];
        let size = ProcInfo::SIZE as u64;
        let all: Vec<u8> = f.procs.iter().flat_map(|p| p.to_bytes()).collect();
        assert_eq!(call(&mut f, Call::ProcList, [W, 4 * size, 0]), Ok(4));
        assert_eq!(get(&mut f, W, all.len()), all);
        // A buffer too short gets what fits, whole entries only, and the
        // count of all.
        put(&mut f, W, &[0xAA; 4 * 96]);
        assert_eq!(call(&mut f, Call::ProcList, [W, 3 * size - 1, 0]), Ok(4));
        assert_eq!(get(&mut f, W, 2 * 96), all[..2 * 96]);
        assert_eq!(
            get(&mut f, W + 2 * size, 96),
            [0xAA; 96],
            "nothing of the third"
        );
        assert_eq!(
            call(&mut f, Call::ProcList, [0, 0, 0]),
            Ok(4),
            "just the count"
        );
        assert_eq!(call(&mut f, Call::ProcList, [0, size - 1, 0]), Ok(4));
        // More room than processes takes only what they need.
        assert_eq!(call(&mut f, Call::ProcList, [W, 40 * size, 0]), Ok(4));
    }

    #[test]
    fn proc_list_writes_nothing_into_memory_that_is_not_all_the_program_s() {
        let mut f = fake();
        f.procs = (1..=3)
            .map(|pid| ProcInfo::new(pid, 0, pid, 1, 0, 0, b"p"))
            .collect();
        let size = ProcInfo::SIZE as u64;
        assert_eq!(
            call(&mut f, Call::ProcList, [U, size, 0]),
            Err(errno::EFAULT),
            "read-only"
        );
        let end = W + PAGE - size - 10;
        put(&mut f, end, &[0xAA; 96]);
        assert_eq!(
            call(&mut f, Call::ProcList, [end, 2 * size, 0]),
            Err(errno::EFAULT),
            "the second runs past the page"
        );
        assert_eq!(get(&mut f, end, 96), [0xAA; 96], "not even the first");
        assert_eq!(
            call(&mut f, Call::ProcList, [0xFFFF_8000_0000_0000, size, 0]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn a_bad_status_pointer_loses_no_child() {
````

Replace:

````rust
            Call::Getpid,
            Call::MemMap,
````

with:

````rust
            Call::Getpid,
            Call::ProcList,
            Call::MemMap,
````

- [ ] **Step 4: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub woken: Vec<u64>,
}
````

with:

````rust
    pub woken: Vec<u64>,
    /// What `proc_list` reports.
    pub procs: Vec<ProcInfo>,
}
````

Replace:

````rust
    }
    fn memory(&self) -> MemInfo {
````

with:

````rust
    }
    fn processes(&mut self) -> Vec<ProcInfo> {
        self.procs.clone()
    }
    fn memory(&self) -> MemInfo {
````

Replace:

````rust
        woken: Vec::new(),
    }
````

with:

````rust
        woken: Vec::new(),
        procs: Vec::new(),
    }
````

- [ ] **Step 5: Add the scenario `tests/e2e/proc_calls.txt`**

Create `tests/e2e/proc_calls.txt`:

````text
# The proc_list call from ring 3 (user-space gate §9.3; milestone 3, plan
# 2): t-proc's kinds. Every process is listed by pid with its parent, its
# group, what it waits for and whether it holds memory; a buffer too
# short gets what fits and the count of all; a name is cut at 64 bytes.
timeout 30
expect root@relay:~# $
send t-proc list
expect \ninit: ppid 0, group 1\ninit: init, wait, no frames\nparent: /bin/sh, wait, frames\nme: /bin/t-proc, run, frames\n
expect \Asleeping child: /bin/sleep, sleep, frames\nended child: /bin/true, zombie, no frames\nchild on a pipe: /bin/cat, pipe, frames\nchild reading: /bin/cat, read, frames\nchildren in my group: true\nby pid: true\n
# The children it killed, in the order they ran next.
expect \A(pid \d+ \(/bin/(sleep|cat)\): killed: kill\n){3}root@relay:~# $
send t-proc short
expect \none entry: pid 1, of more than one: true\nno entry: the same count: true\n
send t-proc long
expect \na name of 90 bytes: 64 bytes, its start: true\n
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-read   t-sys  touch  wc\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-spawn  t-tee  true\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spin   tail   uname\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-proc   t-spin  tail   uname\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-read   t-sys   touch  wc\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spawn  t-tee   true\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-read   t-sys  touch  wc\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-spawn  t-tee  true\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spin   tail   uname\n
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-proc   t-spin  tail   uname\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-read   t-sys   touch  wc\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spawn  t-tee   true\n
````

- [ ] **Step 7: Add the test program `userland/tests/src/bin/t-proc.rs`**

Create `userland/tests/src/bin/t-proc.rs`:

````rust
//! `t-proc KIND`: the `proc_list` call from ring 3 (spec §9.3), each answer
//! printed as `<what>: <value or error name>`.
//!
//! - `t-proc list`: process 1, its parent (the shell), itself, and four
//!   children of its own as `proc_list` shows them: one sleeping, one
//!   ended and not yet collected, one waiting on an empty pipe and one
//!   reading the console; then that the list is by pid.
//! - `t-proc short`: a buffer of one entry gets the first process and the
//!   count of all; one of none only the count. (A buffer the program does
//!   not have is `EFAULT`: the dispatcher's tests, over the same checks.)
//! - `t-proc long`: a child started by a path longer than 64 bytes shows
//!   its first 64.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::proc::{
    PROC_MAX, PROC_NAME, STATE_PIPE, STATE_READ, STATE_READY, STATE_RUN, STATE_SLEEP, STATE_WAIT,
    STATE_ZOMBIE,
};
use relay_abi::{FdMap, ProcInfo, errno};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"list") => list(),
        Some(b"short") => short(),
        Some(b"long") => long(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-proc list|short|long\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-proc: {}", name(e));
            1
        }
    }
}

/// An error's name.
fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

/// A state's word, as `ps` says it.
fn state(s: u32) -> &'static str {
    match s {
        STATE_RUN => "run",
        STATE_READY => "ready",
        STATE_WAIT => "wait",
        STATE_READ => "read",
        STATE_SLEEP => "sleep",
        STATE_PIPE => "pipe",
        STATE_ZOMBIE => "zombie",
        _ => "?",
    }
}

/// Every process, and how many `proc_list` said there are.
fn processes(buf: &mut [ProcInfo; PROC_MAX]) -> Result<&[ProcInfo], u16> {
    let n = sys::proc_list(buf)?;
    Ok(&buf[..n.min(PROC_MAX)])
}

fn entry(all: &[ProcInfo], pid: u32) -> Option<&ProcInfo> {
    all.iter().find(|p| p.pid == pid)
}

/// `<what>: <name>, <state>` and whether it holds memory.
fn print(what: &str, p: Option<&ProcInfo>) {
    let _ = match p {
        Some(p) => writeln!(
            Fd(1),
            "{what}: {}, {}, {}",
            core::str::from_utf8(p.name()).unwrap_or("?"),
            state(p.state),
            if p.frames > 0 { "frames" } else { "no frames" }
        ),
        None => writeln!(Fd(1), "{what}: none"),
    };
}

/// Starts `path` with `args` (each followed by a NUL) and `fds`.
fn start(path: &[u8], args: &[u8], fds: &[(u32, u32)]) -> Result<u32, u16> {
    let maps: [FdMap; 3] = core::array::from_fn(|i| {
        let (child, parent) = fds.get(i).copied().unwrap_or((0, 0));
        FdMap { child, parent }
    });
    sys::spawn(path, args, b"", &maps[..fds.len()], 0, 0)
}

/// Whether `pid`'s entry says `want`, asked again every 10 ms for at most
/// a second (a child needs a moment to get there).
fn until(pid: u32, want: u32) -> Result<(), u16> {
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    for _ in 0..100 {
        if entry(processes(&mut buf)?, pid).is_some_and(|p| p.state == want) {
            return Ok(());
        }
        sys::sleep(10);
    }
    let _ = writeln!(Fd(1), "pid {pid} never {}, and no end", state(want));
    Ok(())
}

fn list() -> Result<(), u16> {
    let me = sys::getpid();
    let sleeper = start(b"/bin/sleep", b"sleep\x005\0", &[])?;
    let ended = start(b"/bin/true", b"true\0", &[])?;
    let (r, w) = sys::pipe()?;
    let piped = start(b"/bin/cat", b"cat\0", &[(0, r)])?;
    // In this program's group, which has the console.
    let reader = start(b"/bin/cat", b"cat\0", &[(0, 0)])?;
    for (pid, want) in [
        (sleeper, STATE_SLEEP),
        (ended, STATE_ZOMBIE),
        (piped, STATE_PIPE),
        (reader, STATE_READ),
    ] {
        until(pid, want)?;
    }
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    let all = processes(&mut buf)?;
    let mine = entry(all, me);
    let parent = mine.and_then(|p| entry(all, p.ppid));
    let init = entry(all, 1);
    let _ = writeln!(
        Fd(1),
        "init: ppid {}, group {}",
        init.map_or(99, |p| p.ppid),
        init.map_or(99, |p| p.pgid)
    );
    print("init", init);
    print("parent", parent);
    print("me", mine);
    print("sleeping child", entry(all, sleeper));
    print("ended child", entry(all, ended));
    print("child on a pipe", entry(all, piped));
    print("child reading", entry(all, reader));
    let groups = [sleeper, ended, piped, reader].map(|c| entry(all, c).map_or(0, |p| p.pgid));
    let _ = writeln!(
        Fd(1),
        "children in my group: {}",
        groups.iter().all(|&g| Some(g) == mine.map(|p| p.pgid))
    );
    let by_pid = all.windows(2).all(|w| w[0].pid < w[1].pid);
    let _ = writeln!(Fd(1), "by pid: {by_pid}");
    for pid in [sleeper, piped, reader] {
        sys::kill(i64::from(pid))?;
    }
    sys::close(r)?;
    sys::close(w)?;
    for pid in [sleeper, ended, piped, reader] {
        sys::wait(i64::from(pid), false)?;
    }
    Ok(())
}

fn short() -> Result<(), u16> {
    let mut one = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); 1];
    let n = sys::proc_list(&mut one)?;
    let _ = writeln!(
        Fd(1),
        "one entry: pid {}, of more than one: {}",
        one[0].pid,
        n > 1
    );
    let all = sys::proc_list(&mut [])?;
    let _ = writeln!(Fd(1), "no entry: the same count: {}", all == n);
    Ok(())
}

fn long() -> Result<(), u16> {
    // `/bin/` and 40 times `./`, then `sleep`: 90 bytes, /bin/sleep.
    let mut path = [0u8; 90];
    path[..5].copy_from_slice(b"/bin/");
    for i in 0..40 {
        path[5 + 2 * i..7 + 2 * i].copy_from_slice(b"./");
    }
    path[85..].copy_from_slice(b"sleep");
    let child = start(&path, b"sleep\x001\0", &[])?;
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    let all = processes(&mut buf)?;
    let shown = entry(all, child).map_or(&b""[..], |p| p.name());
    let _ = writeln!(
        Fd(1),
        "a name of {} bytes: {} bytes, its start: {}",
        path.len(),
        shown.len(),
        shown == &path[..PROC_NAME]
    );
    sys::kill(i64::from(child))?;
    sys::wait(i64::from(child), false)?;
    Ok(())
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `STATE_WAIT` in this scope ``; `` cannot find value `STATE_READ` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario proc_calls`

Expected: FAIL: scenario `proc_calls` stops at line 8, timed out waiting for `\ninit: ppid 0, group 1\ninit: init, wait, no frames\nparent: /bin/sh, wait, frames\nme: /bin/t-proc, run, frames\n`.

- [ ] **Step 9: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 40 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 41 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 10: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use core::sync::atomic::{AtomicU64, Ordering};
use relay_abi::{MemInfo, Time, WaitStatus};
use spin::Mutex;
````

with:

````rust
use core::sync::atomic::{AtomicU64, Ordering};
use relay_abi::{MemInfo, ProcInfo, Time, WaitStatus};
use spin::Mutex;
````

Replace:

````rust

    fn memory(&self) -> MemInfo {
````

with:

````rust

    /// `PROCS` before `MEMORY`, as everywhere.
    fn processes(&mut self) -> Vec<ProcInfo> {
        let t = PROCS.lock();
        mm::with_user_memory(|mem, _| t.list(|p| p.res.space.as_ref().map_or(0, |s| s.frames(mem))))
    }

    fn memory(&self) -> MemInfo {
````

- [ ] **Step 11: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::Errno;
````

with:

````rust
use alloc::vec::Vec;
use relay_abi::proc::{
    STATE_PIPE, STATE_READ, STATE_READY, STATE_RUN, STATE_SLEEP, STATE_WAIT, STATE_ZOMBIE,
};
use relay_abi::{ProcInfo, WaitStatus};
use vfs::Errno;
````

Replace:

````rust
    Zombie(WaitStatus),
}
````

with:

````rust
    Zombie(WaitStatus),
}

impl State {
    /// Its number in `ProcInfo::state` (spec §9.3): what `ps` says.
    pub fn number(self) -> u32 {
        match self {
            State::Running => STATE_RUN,
            State::Ready => STATE_READY,
            State::Blocked(Blocked::Wait) => STATE_WAIT,
            State::Blocked(Blocked::Console) => STATE_READ,
            State::Blocked(Blocked::Sleep(_)) => STATE_SLEEP,
            State::Blocked(Blocked::Pipe(_)) => STATE_PIPE,
            State::Zombie(_) => STATE_ZOMBIE,
        }
    }
}
````

Replace:

````rust
        self.procs.iter_mut()
    }
````

with:

````rust
        self.procs.iter_mut()
    }

    /// Every process, by pid (the table keeps them in the order they
    /// came, and pids only count up), as `proc_list` reports it (spec
    /// §9.3), with the frames `frames` says it holds.
    pub fn list(&self, mut frames: impl FnMut(&Process<R>) -> u64) -> Vec<ProcInfo> {
        self.procs
            .iter()
            .map(|p| {
                let state = p.state.number();
                ProcInfo::new(
                    p.pid,
                    p.ppid,
                    p.pgid,
                    state,
                    frames(p),
                    p.ticks,
                    p.name.as_bytes(),
                )
            })
            .collect()
    }
````

- [ ] **Step 12: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`, the pipe's in `pipes`;
//! `proc_list` is `ENOSYS` until milestone 3's jobs.

````

with:

````rust
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`, the pipe's in `pipes`.

````

Replace:

````rust
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::{Errno, Vfs};
````

with:

````rust
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::{Errno, Vfs};
````

Replace:

````rust
    fn pid(&self) -> u32;
    /// The memory figures of `free`.
````

with:

````rust
    fn pid(&self) -> u32;
    /// Every process, by pid, as `ps` shows it (spec §9.3).
    fn processes(&mut self) -> Vec<ProcInfo>;
    /// The memory figures of `free`.
````

Replace:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::MemMap) => mem_map(caller, args[0]),
````

with:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::ProcList) => proc_list(caller, args[0], args[1]),
        Some(Call::MemMap) => mem_map(caller, args[0]),
````

Replace:

````rust
    Ok(u64::from(pid))
}
````

with:

````rust
    Ok(u64::from(pid))
}

/// `proc_list(buffer, length)` (spec §7.3, §9.3): a `ProcInfo` for each
/// process, by pid, as many as the buffer holds; how many processes there
/// are, so a caller whose buffer was too short knows. Nothing is written
/// unless all that fits can be (`EFAULT`).
fn proc_list(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let procs = caller.processes();
    let size = ProcInfo::SIZE as u64;
    let fit = (len / size).min(procs.len() as u64);
    if fit > 0 {
        let slice = UserSlice::new(addr, fit * size)?;
        let bytes: Vec<u8> = procs[..fit as usize]
            .iter()
            .flat_map(|p| p.to_bytes())
            .collect();
        caller.write(&slice, 0, &bytes)?;
    }
    Ok(procs.len() as u64)
}
````

- [ ] **Step 13: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 40 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 41 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 377 tests.

- [ ] **Step 15: Run the `proc_calls`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario proc_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add crates docs kernel tests userland
git commit -F - <<'EOF'
feat(kernel,relay-rt): add the proc_list call

proc_list (spec §7.3, §9.3) fills a buffer with a ProcInfo for each
process, by pid, as many whole entries as fit, and returns how many
processes there are, so a short buffer is no error and says what it
missed; nothing is written unless all that fits can be (EFAULT). The
state says what a blocked process waits for, the frames are its address
space's, and process 1, which has none, shows 0. relay-rt gains
sys::proc_list, and t-proc shows the call from ring 3 in the scenario
proc_calls.
EOF
````


### Task 3: The console is changed only by a group it was handed to

Spec §6.4, §7.3 and decision 3: background jobs must not take the console or change its mode, and until now any process could. The rule cannot be "only the foreground group": `/bin/sh` takes the console back at its prompt after its command's group has ended and been collected, and switches the mode while that group is still on top. So the process table keeps the chain of groups that handed the console on (`proc::holders::Holders`): process 1's at the bottom, and on top each group given it by the one below; `FOREGROUND` and `console_foreground` cut the chain back to the giver's group and put the new one on top, or cut back to it if it holds the console already (taking it back); groups with no process left leave the chain, so it holds one entry per group at most; the error screen resets it to process 1's. `console_mode`, `console_foreground` and a `spawn` with `FOREGROUND` are `EPERM` for a caller whose group is not in it (`console_foreground` says `ESRCH` first for a group with no live process); `console_mode` checks under `PROCS` and switches the mode after, since the switch echoes, which may write the tees. The tee calls stay open to all. A spike ran every scenario under this rule first: all passed, and the only refusal was `t-read leave`'s orphan. `t-read leave`'s child now has fd 1 and says what it was told, and the new `t-read refused` starts a child in a group of its own that tries all three; `t-read apart`, which `check3-a.sh` runs on the NUC, prints what it printed. The red runs are the kernel's tests, which cannot find `Holders`, and `console`, where the refused calls succeed. Mutation checks: on the host, a giver below the top not cut back to (it survived at first: every test gave from the top; a test where process 1 starts a new shell while the last one's command still runs now kills it), taking the console back not cut back, pruning off, process 1's group pruned, a non-holder giving from the top, `reset` doing nothing and the check by pid instead of group each fail a test; in QEMU (`console`), `spawn`'s `FOREGROUND` check, `console_mode`'s and `console_foreground`'s refusals each fail the scenario.

**Files:**
- Modify: `kernel/src/proc.rs`
- Create: `kernel/src/proc/holders.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `tests/e2e/console.txt`
- Modify: `userland/tests/src/bin/t-read.rs`

**Interfaces:**
- Consumes: plan 4a's `FOREGROUND`, `console_foreground`; plan 4b's `proc::take_console`.
- Produces: `proc::holders::Holders` (`new`, `foreground`, `holds(pgid)`, `give(by, to, exists) -> Result<(), Errno>`, `reset`); `Table::{console_group(&self) -> u32, may_change_console(&self, pid) -> bool, give_console(&mut self, pid, to) -> Result<(), Errno>, take_console(&mut self)}`; `Caller::console_mode(&mut self, line: bool) -> Result<bool, Errno>`; the test fake's `Fake::holds_console`; `t-read refused`.

- [ ] **Step 1: Declare the new module in `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

pub mod table;
````

with:

````rust

pub mod holders;
pub mod table;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/proc/holders.rs`**

Create `kernel/src/proc/holders.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn chain(h: &Holders) -> &[u32] {
        &h.groups[..h.len]
    }

    #[test]
    fn process_1_holds_the_console_at_first() {
        let h = Holders::new();
        assert_eq!((chain(&h), h.foreground()), (&[1][..], 1));
        assert!(h.holds(1) && !h.holds(2));
    }

    #[test]
    fn a_holder_gives_it_on_and_takes_it_back() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        h.give(2, 5, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5]);
        assert_eq!(h.foreground(), 5);
        // A nested shell, and its command.
        h.give(5, 8, all).unwrap();
        h.give(8, 9, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5, 8, 9]);
        // The nested shell takes it back, then the outer one.
        h.give(8, 8, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5, 8]);
        h.give(2, 2, all).unwrap();
        assert_eq!(chain(&h), [1, 2]);
        // Given to a group below the giver: that one takes it back.
        h.give(2, 6, all).unwrap();
        h.give(6, 2, all).unwrap();
        assert_eq!(chain(&h), [1, 2]);
        h.give(2, 1, all).unwrap();
        assert_eq!(chain(&h), [1]);
    }

    #[test]
    fn a_group_never_given_the_console_cannot_take_it() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        // A background job of the shell's.
        assert_eq!(h.give(7, 7, all), Err(Errno::EPERM));
        assert_eq!(h.give(7, 2, all), Err(Errno::EPERM));
        assert!(!h.holds(7));
        assert_eq!(chain(&h), [1, 2], "nothing changed");
        // A group the console was taken back from is out of the chain.
        h.give(2, 5, all).unwrap();
        h.give(2, 2, all).unwrap();
        assert_eq!(h.give(5, 5, all), Err(Errno::EPERM));
        // Nor one above a giver further down: process 1 starts a new shell
        // while the last one's command still runs.
        h.give(2, 5, all).unwrap();
        h.give(1, 9, all).unwrap();
        assert_eq!(chain(&h), [1, 9]);
        assert_eq!(h.give(5, 5, all), Err(Errno::EPERM));
    }

    #[test]
    fn groups_that_are_gone_leave_the_chain_but_process_1_s() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        h.give(2, 5, all).unwrap();
        h.give(5, 8, all).unwrap();
        // Group 5 is gone (a shell that ended): 8 and 2 stay.
        h.give(8, 9, |g| g != 5).unwrap();
        assert_eq!(chain(&h), [1, 2, 8, 9]);
        h.give(2, 3, |g| g == 2 || g == 3).unwrap();
        assert_eq!(chain(&h), [1, 2, 3]);
        assert_eq!(h.give(2, 4, |g| g == 4), Err(Errno::EPERM), "2 is gone");
        assert_eq!(chain(&h), [1]);
        h.give(1, 4, |_| false).unwrap();
        assert_eq!(chain(&h), [1, 4]);
    }

    #[test]
    fn the_chain_holds_one_entry_per_group_at_most() {
        // Each group gives the console to a new one and ends: the chain
        // never grows past the groups that exist.
        let mut h = Holders::new();
        for g in 2..10_000u32 {
            h.give(g - 1, g, |x| x + 1 >= g).unwrap();
            assert!(h.len <= 3, "{g}: {:?}", chain(&h));
        }
        // And with every one still there, it is full at MAX groups.
        let mut h = Holders::new();
        for g in 2..=MAX as u32 {
            h.give(g - 1, g, |_| true).unwrap();
        }
        assert_eq!(h.len, MAX);
        assert_eq!(h.give(MAX as u32, 1000, |_| true), Err(Errno::EAGAIN));
        assert_eq!(h.foreground(), MAX as u32);
    }

    #[test]
    fn reset_gives_it_back_to_process_1() {
        let mut h = Holders::new();
        h.give(1, 2, |_| true).unwrap();
        h.give(2, 3, |_| true).unwrap();
        h.reset();
        assert_eq!(chain(&h), [1]);
        assert_eq!(h.give(2, 2, |_| true), Err(Errno::EPERM));
    }
}
````

- [ ] **Step 3: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

with:

````rust
    #[test]
    fn the_console_is_changed_by_the_groups_it_was_handed_to() {
        let mut t = table();
        let init = add(&mut t, 0, true);
        let sh = add(&mut t, init, true);
        assert!(t.may_change_console(init) && !t.may_change_console(sh));
        t.give_console(init, sh).unwrap();
        let cmd = add(&mut t, sh, true);
        let bg = add(&mut t, sh, true);
        let script_cmd = add(&mut t, cmd, false);
        t.give_console(sh, cmd).unwrap();
        assert_eq!(t.console_group(), cmd);
        assert!(t.may_change_console(script_cmd), "the group's every member");
        assert!(t.may_change_console(sh) && t.may_change_console(init));
        assert!(!t.may_change_console(bg), "a background job's");
        assert_eq!(t.give_console(bg, bg), Err(Errno::EPERM));
        assert_eq!(t.give_console(99, sh), Err(Errno::EPERM), "no such process");
        // The command ends and is collected: the shell takes it back.
        t.end(script_cmd, WaitStatus::exited(0));
        t.end(cmd, WaitStatus::exited(0));
        assert!(t.reap(sh, Want::Pid(cmd)).unwrap().is_some());
        assert!(t.reap(init, Want::Pid(script_cmd)).unwrap().is_some());
        t.give_console(sh, sh).unwrap();
        assert_eq!(t.console_group(), sh);
        t.take_console();
        assert_eq!(t.console_group(), init);
        assert!(!t.may_change_console(sh));
    }

    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

- [ ] **Step 4: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
    #[test]
    fn tees_are_pushed_by_fd_and_popped() {
````

with:

````rust
    #[test]
    fn the_console_is_changed_only_by_a_group_that_holds_it() {
        use relay_abi::console::MODE_LINE;
        let mut f = fake();
        f.holds_console = false;
        let line = u64::from(MODE_LINE);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [line, 0, 0]),
            Err(errno::EPERM)
        );
        assert!(!f.line_mode, "the mode stays");
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [7, 0, 0]),
            Err(errno::EINVAL),
            "a mode that is none, first"
        );
        assert_eq!(
            call(&mut f, Call::ConsoleForeground, [42, 0, 0]),
            Err(errno::EPERM)
        );
        assert_eq!(f.foreground, 1, "the group stays");
        // The tees are not the console's state: anyone may push one.
        assert_eq!(call(&mut f, Call::ConsoleTeePush, [1, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::ConsoleTeePop, [0, 0, 0]), Ok(0));
    }

    #[test]
    fn tees_are_pushed_by_fd_and_popped() {
````

- [ ] **Step 5: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    pub killed_while_reading: bool,
    /// The console's mode and foreground group (groups 1 and 42 exist).
    pub line_mode: bool,
    pub foreground: u32,
    /// The tees pushed, and the syncs.
````

with:

````rust
    pub killed_while_reading: bool,
    /// The console's mode and foreground group (groups 1 and 42 exist),
    /// and whether the program's group may change them.
    pub line_mode: bool,
    pub foreground: u32,
    pub holds_console: bool,
    /// The tees pushed, and the syncs.
````

Replace:

````rust
    }
    fn console_mode(&mut self, line: bool) -> bool {
        core::mem::replace(&mut self.line_mode, line)
    }
````

with:

````rust
    }
    fn console_mode(&mut self, line: bool) -> Result<bool, Errno> {
        if !self.holds_console {
            return Err(Errno::EPERM);
        }
        Ok(core::mem::replace(&mut self.line_mode, line))
    }
````

Replace:

````rust
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno> {
        if ![1, 42].contains(&pgid) {
````

with:

````rust
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno> {
        if !self.holds_console {
            return Err(Errno::EPERM);
        }
        if ![1, 42].contains(&pgid) {
````

Replace:

````rust
        foreground: 1,
        tees: Vec::new(),
````

with:

````rust
        foreground: 1,
        holds_console: true,
        tees: Vec::new(),
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \nend of input\nroot@relay:~# $
# A terminal that sends CR LF for Enter: the LF after the shell's line
````

with:

````text
expect \nend of input\nroot@relay:~# $
# Nor may it change the console's mode or group, or give it to a child of
# its own: its group was never given the console (milestone 3).
send t-read refused
expect \nmode: EPERM\nforeground of my own group: EPERM\na child with the console: EPERM\nroot@relay:~# $
# A terminal that sends CR LF for Enter: the LF after the shell's line
````

Replace:

````text
expect t-read\nend of input\nroot@relay:~# $
# A program that leaves the console in line mode, while the shell waits
# at its prompt, does not keep the shell from reading.
send t-read leave
alive 2
send echo ok
````

with:

````text
expect t-read\nend of input\nroot@relay:~# $
# A program outside the console's groups cannot put it in line mode
# while the shell waits at its prompt (milestone 3).
send t-read leave
expect \nroot@relay:~# line mode from outside: EPERM\n
send echo ok
````

- [ ] **Step 7: Change the test program `userland/tests/src/bin/t-read.rs`**

In `userland/tests/src/bin/t-read.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//!   is not the console's, so it gets end of input at once.
//! - `t-read size`: the console's columns and rows, and what
//!   `console_mode` and `console_foreground` refuse.
//! - `t-read leave`: leaves a child behind that puts the console in line
//!   mode half a second later, while the shell waits at its prompt.
#![no_std]
````

with:

````rust
//!   is not the console's, so it gets end of input at once.
//! - `t-read refused`: starts a child in a group of its own, which was
//!   never given the console, so it may not change its mode or group, or
//!   start a child with it (milestone 3).
//! - `t-read size`: the console's columns and rows, and what
//!   `console_mode` and `console_foreground` refuse.
//! - `t-read leave`: leaves a child behind that tries to put the console
//!   in line mode half a second later, while the shell waits at its
//!   prompt, and says what it was told.
#![no_std]
````

Replace:

````rust
use relay_abi::errno;
use relay_abi::spawn::NEW_GROUP;
use relay_rt::Args;
````

with:

````rust
use relay_abi::errno;
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_rt::Args;
````

Replace:

````rust
        Some(b"raw") => raw(),
        Some(b"apart") => apart(),
        Some(b"size") => size(),
````

with:

````rust
        Some(b"raw") => raw(),
        Some(b"apart") => apart(b"t-read\0"),
        Some(b"size") => size(),
````

Replace:

````rust
            b"",
            &[],
            NEW_GROUP,
````

with:

````rust
            b"",
            &[FdMap {
                child: 1,
                parent: 1,
            }],
            NEW_GROUP,
````

Replace:

````rust
            sys::sleep(500);
            sys::console_mode(MODE_LINE).map(|_| ())
        }
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|size|leave]\n");
            return 2;
````

with:

````rust
            sys::sleep(500);
            let r = sys::console_mode(MODE_LINE).map(|_| 0);
            let _ = writeln!(Fd(1), "line mode from outside: {}", name(r));
            Ok(())
        }
        Some(b"refused") => apart(b"t-read\0refused-child\0"),
        Some(b"refused-child") => {
            refused();
            Ok(())
        }
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|refused|size|leave]\n");
            return 2;
````

Replace:

````rust

fn apart() -> Result<(), u16> {
    let fds = [
````

with:

````rust

/// Runs `t-read` with `args` in a process group of its own.
fn apart(args: &[u8]) -> Result<(), u16> {
    let fds = [
````

Replace:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-read", b"t-read\0", b"", &fds, NEW_GROUP, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}
````

with:

````rust
    ];
    let pid = sys::spawn(b"/bin/t-read", args, b"", &fds, NEW_GROUP, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}

/// An error's name, or `ok`.
fn name(r: Result<u32, u16>) -> &'static str {
    r.map_or_else(|e| errno::name(e).unwrap_or("?"), |_| "ok")
}

/// What a program in a group that never had the console is told when it
/// tries to change it.
fn refused() {
    let me = sys::getpid();
    let _ = writeln!(Fd(1), "mode: {}", name(sys::console_mode(MODE_RAW)));
    let fg = sys::console_foreground(me).map(|()| 0);
    let _ = writeln!(Fd(1), "foreground of my own group: {}", name(fg));
    let fds = [FdMap {
        child: 1,
        parent: 1,
    }];
    let child = sys::spawn(
        b"/bin/t-args",
        b"t-args\0",
        b"",
        &fds,
        NEW_GROUP | FOREGROUND,
        0,
    );
    let _ = writeln!(Fd(1), "a child with the console: {}", name(child));
}
````

Replace:

````rust
    let _ = writeln!(Fd(1), "console: {columns}x{rows}");
    let name = |r: Result<u32, u16>| r.map_or_else(|e| errno::name(e).unwrap_or("?"), |_| "ok");
    let _ = writeln!(Fd(1), "mode 7: {}", name(sys::console_mode(7)));
````

with:

````rust
    let _ = writeln!(Fd(1), "console: {columns}x{rows}");
    let _ = writeln!(Fd(1), "mode 7: {}", name(sys::console_mode(7)));
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Holders` in this scope ``; `` cannot find value `MAX` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario console`

Expected: FAIL: scenario `console` stops at line 50, timed out waiting for `\nmode: EPERM\nforeground of my own group: EPERM\na child with the console: EPERM\nroot@relay:~# $`.

- [ ] **Step 9: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    tty::set_line_mode(false);
    tty::set_foreground(table::INIT);
    PROCS.lock().wake_all(Blocked::Console);
}
````

with:

````rust
    tty::set_line_mode(false);
    let mut t = PROCS.lock();
    t.take_console();
    tty::set_foreground(t.console_group());
    t.wake_all(Blocked::Console);
}
````

Replace:

````rust
        }
        let parent = t.get(t.current()).expect("a process spawns");
````

with:

````rust
        }
        // Only a group that holds the console gives it to a child's.
        if s.foreground && !t.may_change_console(t.current()) {
            return Err(Errno::EPERM);
        }
        let parent = t.get(t.current()).expect("a process spawns");
````

Replace:

````rust
        .unwrap_or_else(|e| unreachable!("room and group were checked under this lock: {e}"));
    drop(t);
    // Before the child can run: the kernel is not preemptible, and it has
    // not been switched to yet.
    if s.foreground {
        tty::set_foreground(pid);
        tty::set_line_mode(true);
````

with:

````rust
        .unwrap_or_else(|e| unreachable!("room and group were checked under this lock: {e}"));
    // Before the child can run: the kernel is not preemptible, and it has
    // not been switched to yet. Its parent's group held the console when
    // it was checked above, and nothing ran since.
    if s.foreground && t.give_console(me, pid).is_ok() {
        tty::set_foreground(pid);
        drop(t);
        tty::set_line_mode(true);
````

Replace:

````rust

    fn console_mode(&mut self, line: bool) -> bool {
        let was = tty::set_line_mode(line);
        PROCS.lock().wake_all(Blocked::Console);
        was
    }
````

with:

````rust

    fn console_mode(&mut self, line: bool) -> Result<bool, Errno> {
        {
            let t = PROCS.lock();
            if !t.may_change_console(t.current()) {
                return Err(Errno::EPERM);
            }
        }
        // Not under `PROCS`: the switch echoes, which may write the tees.
        let was = tty::set_line_mode(line);
        PROCS.lock().wake_all(Blocked::Console);
        Ok(was)
    }
````

Replace:

````rust
        }
        tty::set_foreground(pgid);
        t.wake_all(Blocked::Console);
````

with:

````rust
        }
        // `EPERM` unless the caller's group holds the console.
        let me = t.current();
        t.give_console(me, pgid)?;
        tty::set_foreground(t.console_group());
        t.wake_all(Blocked::Console);
````

- [ ] **Step 10: Implement `kernel/src/proc/holders.rs`**

Insert this at the top of `kernel/src/proc/holders.rs`, above `#[cfg(test)]`:

````rust
//! Who may change the console (spec §6.4, §16 item 9): the chain of
//! process groups that handed it on, from process 1's group at the bottom
//! to the foreground group at the top. A group gives the console to
//! another (`spawn`'s `FOREGROUND`, `console_foreground`) only while it is
//! in the chain, and the chain is then cut back to it before the other is
//! put on top; a group taking the console back is cut back to. So a shell
//! takes the console back after its command's group has ended, nested
//! shells and scripts each take it back from theirs, and a background job,
//! which was never given it, can neither take it nor change its mode.

use super::table::{INIT, MAX};
use vfs::Errno;

/// The groups, each given the console by the one below it.
pub struct Holders {
    groups: [u32; MAX],
    len: usize,
}

impl Default for Holders {
    fn default() -> Self {
        Holders::new()
    }
}

impl Holders {
    /// Process 1's group alone.
    pub const fn new() -> Holders {
        let mut groups = [0; MAX];
        groups[0] = INIT;
        Holders { groups, len: 1 }
    }

    /// The group at the top, which reads the console.
    pub fn foreground(&self) -> u32 {
        self.groups[self.len - 1]
    }

    /// Whether the group `pgid` may change the console.
    pub fn holds(&self, pgid: u32) -> bool {
        self.groups[..self.len].contains(&pgid)
    }

    /// The group `by` gives the console to the group `to`: the chain is
    /// cut back to `by`, then to `to` if it holds the console already (it
    /// takes it back), and otherwise `to` goes on top. First the groups
    /// that no longer exist (`exists`) leave the chain, process 1's never,
    /// so it holds at most one entry per group there is. `EPERM` if `by`
    /// holds none of the console.
    pub fn give(&mut self, by: u32, to: u32, exists: impl Fn(u32) -> bool) -> Result<(), Errno> {
        let mut kept = 1;
        for i in 1..self.len {
            let g = self.groups[i];
            if exists(g) {
                self.groups[kept] = g;
                kept += 1;
            }
        }
        self.len = kept;
        let at = self.position(by).ok_or(Errno::EPERM)?;
        self.len = at + 1;
        match self.position(to) {
            Some(below) => self.len = below + 1,
            // One entry per group there is, and `to` is one: it fits.
            None if self.len < MAX => {
                self.groups[self.len] = to;
                self.len += 1;
            }
            None => return Err(Errno::EAGAIN),
        }
        Ok(())
    }

    /// Process 1 takes the console back from everyone (the error screen).
    pub fn reset(&mut self) {
        self.len = 1;
    }

    fn position(&self, pgid: u32) -> Option<usize> {
        self.groups[..self.len].iter().position(|&g| g == pgid)
    }
}

````

- [ ] **Step 11: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use alloc::string::String;
````

with:

````rust

use super::holders::Holders;
use alloc::string::String;
````

Replace:

````rust
    slice_left: u32,
}
````

with:

````rust
    slice_left: u32,
    /// The groups that may change the console (spec §16 item 9).
    console: Holders,
}
````

Replace:

````rust
            slice_left: SLICE,
        }
````

with:

````rust
            slice_left: SLICE,
            console: Holders::new(),
        }
````

Replace:

````rust
            .any(|p| p.pgid == pgid && !matches!(p.state, State::Zombie(_)))
    }
````

with:

````rust
            .any(|p| p.pgid == pgid && !matches!(p.state, State::Zombie(_)))
    }

    /// The group that has the console, as the processes handed it on (spec
    /// §6.4, §16 item 9): process 1's at first.
    pub fn console_group(&self) -> u32 {
        self.console.foreground()
    }

    /// Whether the process `pid` may change the console (its mode and
    /// group, and a child's with `FOREGROUND`): its group was given it, and
    /// has not handed it back.
    pub fn may_change_console(&self, pid: u32) -> bool {
        self.get(pid).is_some_and(|p| self.console.holds(p.pgid))
    }

    /// The process `pid`'s group gives the console to the group `to` (its
    /// own takes it back); `EPERM` if it may not change the console.
    pub fn give_console(&mut self, pid: u32, to: u32) -> Result<(), Errno> {
        let by = self.get(pid).ok_or(Errno::EPERM)?.pgid;
        let procs = &self.procs;
        self.console
            .give(by, to, |g| procs.iter().any(|p| p.pgid == g))
    }

    /// Process 1 takes the console back (the error screen's).
    pub fn take_console(&mut self) {
        self.console.reset();
    }
````

- [ ] **Step 12: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn pipe_wake(&mut self, id: u64);
    /// Line mode (`true`) or raw mode; the previous one.
    fn console_mode(&mut self, line: bool) -> bool;
    /// The console's columns and rows.
    fn console_size(&self) -> (u32, u32);
    /// Makes `pgid` the foreground group; `ESRCH` if no process is in it.
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
````

with:

````rust
    fn pipe_wake(&mut self, id: u64);
    /// Line mode (`true`) or raw mode; the previous one. `EPERM` unless
    /// the program's group holds the console (spec §16 item 9).
    fn console_mode(&mut self, line: bool) -> Result<bool, Errno>;
    /// The console's columns and rows.
    fn console_size(&self) -> (u32, u32);
    /// Makes `pgid` the foreground group: `ESRCH` if no process is in
    /// `pgid`, `EPERM` unless the program's group holds the console.
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
````

Replace:

````rust
    };
    let was = caller.console_mode(line);
    Ok(u64::from(if was { MODE_LINE } else { MODE_RAW }))
````

with:

````rust
    };
    let was = caller.console_mode(line)?;
    Ok(u64::from(if was { MODE_LINE } else { MODE_RAW }))
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 385 tests.

- [ ] **Step 14: Run the `console`, `sh`, `spawn`, `respawn`, `pipes` scenarios**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 15: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 16: Commit**

````bash
git add kernel tests userland
git commit -F - <<'EOF'
feat(kernel): refuse the console to a group that was never given it

Background jobs (spec §9.2) must not take the console or change its
mode, and any process could. The process table now keeps the chain of
groups that handed the console on, from process 1's to the foreground
group (proc::holders): FOREGROUND and console_foreground put a group on
top, cutting the chain back to the giver, and a group taking the console
back is cut back to. console_mode, console_foreground and a spawn with
FOREGROUND are EPERM for a caller whose group is not in it. So a shell
takes the console back after its command's group has ended, nested
shells and scripts each from theirs, and the error screen gives it all
to process 1. The tee calls change nothing anyone reads and stay open
to all. t-read apart and leave show the refusals.
EOF
````


### Task 4: A Ctrl-C ends a `wait` that asks for it

Spec §7.3, §9.2 and decision 4: at its prompt the shell has the console in raw mode, where a Ctrl-C is only a byte, so nothing could stop the coming `wait` built-in while a background job runs on. `wait` gains the flag `WAIT_CTRL_C` (2): a Ctrl-C typed while the caller's group has the console in raw mode ends the wait with `EINTR` (once no child has ended) and is taken from the input; without the flag a raw Ctrl-C stays input. In raw mode a Ctrl-C drops what was typed before it, so it is the oldest byte when it waits (`InputQueue::{has_raw_interrupt, take_raw_interrupt}`, `tty::{has_raw_ctrl_c, take_raw_ctrl_c}`), and the console's input wakes that group's waiters when one comes (`Table::wake_waiting`). A flag no program passed before (it was `EINVAL`) adds to the ABI and changes no meaning, so `VERSION` stays 3. `relay-rt` gains `sys::wait_with`, and `t-spawn ctrl-c` sets raw mode, waits with the flag for a `t-spin` in a group of its own, and then reads what else was typed: `ctrlc` types Ctrl-C and `x`. `ctrlc` also waits for the prompt before typing into the command after a `dmesg`: the spike found the typed line could reach dmesg's ended group, still the console's in line mode, and be dropped by the Ctrl-C. `relay-abi`'s constant and `relay-rt`'s `sys.rs` come before the red run, since `t-spawn` needs them. The red runs are the kernel's tests, which cannot compile against the new `Caller::wait`, and `ctrlc`, where the kernel refuses the flag. Mutation checks: on the host, the Ctrl-C not taken, every group's waiters woken, the flag dropped by the dispatcher and a fourth flag bit taken each fail a test (the queue's line-mode check survived: in line mode no byte waits there, so it is gone); in QEMU (`ctrlc`), the Ctrl-C never taken and the waiter not woken each fail the scenario.

**Files:**
- Modify: `crates/relay-abi/src/spawn.rs`
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/tty.rs`
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: plan 3b's input queue and `collect`.
- Produces: `relay_abi::spawn::WAIT_CTRL_C`; `Caller::wait(&mut self, child, nohang: bool, ctrl_c: bool)`; `InputQueue::{has_raw_interrupt, take_raw_interrupt}`; `tty::{has_raw_ctrl_c, take_raw_ctrl_c}`; `Table::wake_waiting(&mut self, pgid)`; `relay_rt::sys::wait_with(pid: i64, flags: u32)`; `t-spawn ctrl-c`.

- [ ] **Step 1: Change `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const WAIT_NOHANG: u32 = 1;

````

with:

````rust
pub const WAIT_NOHANG: u32 = 1;
/// `wait`'s flags: a Ctrl-C typed while the caller's group has the console
/// in raw mode ends the wait with `EINTR`, and is taken from the input
/// (a shell's `wait` built-in, spec §9.2, §16 item 9). Without it a raw
/// Ctrl-C is only input, which the shell would read after the wait.
pub const WAIT_CTRL_C: u32 = 2;

````

Replace:

````rust
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
        assert_eq!(FOREGROUND, 2);
    }
````

with:

````rust
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
        assert_eq!((FOREGROUND, WAIT_CTRL_C), (2, 2));
    }
````

- [ ] **Step 2: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
pub fn wait(pid: i64, nohang: bool) -> Result<Option<(u32, WaitStatus)>, u16> {
    let mut w = WaitStatus::default();
    let flags = if nohang { WAIT_NOHANG } else { 0 };
    let args = [pid as u64, u64::from(flags), &raw mut w as u64, 0, 0, 0];
````

with:

````rust
pub fn wait(pid: i64, nohang: bool) -> Result<Option<(u32, WaitStatus)>, u16> {
    wait_with(pid, if nohang { WAIT_NOHANG } else { 0 })
}

/// [`wait`] with `relay_abi::spawn`'s `WAIT_*` flags: with `WAIT_CTRL_C`,
/// a Ctrl-C typed while this program's group has the console in raw mode
/// ends the wait with `EINTR`.
pub fn wait_with(pid: i64, flags: u32) -> Result<Option<(u32, WaitStatus)>, u16> {
    let mut w = WaitStatus::default();
    let args = [pid as u64, u64::from(flags), &raw mut w as u64, 0, 0, 0];
````

- [ ] **Step 3: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    #[test]
    fn a_raw_ctrl_c_can_be_taken_out_of_the_input() {
        let mut q = InputQueue::new();
        q.push(b"ab");
        assert!(!q.has_raw_interrupt() && !q.take_raw_interrupt());
        q.push(&[INTERRUPT]);
        q.push(b"x");
        assert!(q.has_raw_interrupt());
        assert!(q.take_raw_interrupt());
        assert_eq!(drain(&mut q), b"x", "only the Ctrl-C");
        // In line mode a Ctrl-C is the line discipline's.
        let mut q = InputQueue::new();
        q.set_line_mode(true);
        q.push(&[INTERRUPT]);
        assert!(!q.has_raw_interrupt() && !q.take_raw_interrupt());
        assert!(q.take_line_interrupt());
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 4: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

with:

````rust
    #[test]
    fn a_group_s_waiters_are_woken_and_no_one_else() {
        let mut t = table();
        let a = add(&mut t, 0, true);
        let b = add(&mut t, a, false);
        let c = add(&mut t, a, true);
        let d = add(&mut t, a, false);
        for (p, why) in [
            (a, Blocked::Wait),
            (b, Blocked::Console),
            (c, Blocked::Wait),
            (d, Blocked::Wait),
        ] {
            assert_eq!(t.schedule(), p);
            t.block(why);
        }
        t.wake_waiting(a);
        assert_eq!(state(&t, a), State::Ready);
        assert_eq!(state(&t, d), State::Ready);
        assert_eq!(
            state(&t, b),
            State::Blocked(Blocked::Console),
            "not waiting"
        );
        assert_eq!(state(&t, c), State::Blocked(Blocked::Wait), "another group");
    }

    #[test]
    fn a_group_of_zombies_is_no_group_to_give_the_console() {
````

- [ ] **Step 5: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn proc_list_gives_what_fits_and_how_many_there_are() {
````

with:

````rust
    #[test]
    fn wait_may_end_at_a_ctrl_c() {
        let mut f = fake();
        f.running = true;
        let flags = u64::from(WAIT_CTRL_C);
        f.ctrl_c_typed = true;
        assert_eq!(
            call(&mut f, Call::Wait, [7, flags, W]),
            Err(errno::EINTR),
            "a Ctrl-C was typed"
        );
        assert_eq!(f.ctrl_c_waits, [true]);
        f.ended = vec![(7, WaitStatus::exited(0))];
        assert_eq!(
            call(&mut f, Call::Wait, [7, flags | 1, W]),
            Ok(7),
            "with NOHANG too"
        );
        assert_eq!(f.ctrl_c_waits, [true, true]);
        f.ended = vec![(7, WaitStatus::exited(0))];
        assert_eq!(
            call(&mut f, Call::Wait, [7, 0, W]),
            Ok(7),
            "without it, a child"
        );
        assert_eq!(f.ctrl_c_waits, [true, true, false]);
        for bad in [4, 8, 1 << 32] {
            assert_eq!(
                call(&mut f, Call::Wait, [7, bad, W]),
                Err(errno::EINVAL),
                "{bad}"
            );
        }
    }

    #[test]
    fn proc_list_gives_what_fits_and_how_many_there_are() {
````

Replace:

````rust
        assert_eq!(
            call(&mut f, Call::Wait, [7, 2, 0]),
            Err(errno::EINVAL),
````

with:

````rust
        assert_eq!(
            call(&mut f, Call::Wait, [7, 4, 0]),
            Err(errno::EINVAL),
````

- [ ] **Step 6: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    pub procs: Vec<ProcInfo>,
}
````

with:

````rust
    pub procs: Vec<ProcInfo>,
    /// Whether a Ctrl-C was typed for a `WAIT_CTRL_C` wait to end at, and
    /// whether each wait asked for that.
    pub ctrl_c_typed: bool,
    pub ctrl_c_waits: Vec<bool>,
}
````

Replace:

````rust
    }
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
        let at = self
````

with:

````rust
    }
    fn wait(
        &mut self,
        child: Child,
        nohang: bool,
        ctrl_c: bool,
    ) -> Result<Option<(u32, WaitStatus)>, Errno> {
        self.ctrl_c_waits.push(ctrl_c);
        let at = self
````

Replace:

````rust
            None if self.running && nohang => Ok(None),
            None => Err(Errno::ECHILD),
````

with:

````rust
            None if self.running && nohang => Ok(None),
            None if self.running && ctrl_c && self.ctrl_c_typed => Err(Errno::EINTR),
            None => Err(Errno::ECHILD),
````

Replace:

````rust
        procs: Vec::new(),
    }
````

with:

````rust
        procs: Vec::new(),
        ctrl_c_typed: false,
        ctrl_c_waits: Vec::new(),
    }
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# A Ctrl-C typed before the command has even started is the command's too:
# it ends at once, whichever comes first.
send t-spin
````

with:

````text
# A Ctrl-C typed before the command has even started is the command's too:
# it ends at once, whichever comes first. (Once the prompt is back: typed
# while dmesg's group still had the console, the line would be dmesg's.)
expect root@relay:~# $
send t-spin
````

Replace:

````text
expect \nstill here\n
````

with:

````text
expect \nstill here\n
# A program that waits for a child with WAIT_CTRL_C while its group has
# the console in raw mode, as the shell's `wait` does (milestone 3): a
# Ctrl-C ends the wait, and is taken from what was typed, the rest of it
# left for the program to read. Its child, in a group of its own, spins on
# until the program kills it.
send t-spawn ctrl-c
expect \nwaiting for t-spin\n
alive 1
type {ctrl-c}x
expect \Await: EINTR\nthen read: "x"\n
expect root@relay:~# $
````

- [ ] **Step 8: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   kill can end;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
````

with:

````rust
//!   kill can end;
//! - `t-spawn ctrl-c` sets the console to raw mode, starts `t-spin` in a
//!   group of its own and waits for it with `WAIT_CTRL_C`: a Ctrl-C ends
//!   the wait, and is taken from what was typed, which the program then
//!   reads (milestone 3);
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
````

Replace:

````rust
        Some(b"join") => join(),
        Some(b"kill") => kill(),
````

with:

````rust
        Some(b"join") => join(),
        Some(b"ctrl-c") => ctrl_c(),
        Some(b"kill") => kill(),
````

Replace:

````rust
    })
}
````

with:

````rust
    })
}

fn ctrl_c() -> Result<(), u16> {
    use relay_abi::console::MODE_RAW;
    use relay_abi::spawn::{NEW_GROUP, WAIT_CTRL_C};
    let was = sys::console_mode(MODE_RAW)?;
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, NEW_GROUP, 0)?;
    let _ = writeln!(Fd(1), "waiting for t-spin");
    let r = sys::wait_with(i64::from(pid), WAIT_CTRL_C);
    let _ = writeln!(
        Fd(1),
        "wait: {}",
        r.map_or_else(|e| relay_abi::errno::name(e).unwrap_or("?"), |_| "a child")
    );
    let mut typed = [0u8; 16];
    let n = sys::read(0, &mut typed)?;
    let _ = writeln!(
        Fd(1),
        "then read: {:?}",
        core::str::from_utf8(&typed[..n]).unwrap_or("?")
    );
    sys::kill(i64::from(pid))?;
    sys::wait(i64::from(pid), false)?;
    sys::console_mode(was)?;
    Ok(())
}
````

- [ ] **Step 9: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `WAIT_CTRL_C` in this scope ``; `` method `wait` has 4 parameters but the declaration in trait `syscall::Caller::wait` has 3 ``.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: FAIL: scenario `ctrlc` stops at line 63, timed out waiting for `\Await: EINTR\nthen read: "x"\n`.

- [ ] **Step 10: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
        was
    }
````

with:

````rust
        was
    }

    /// Whether a Ctrl-C typed in raw mode waits to be read: it is the
    /// oldest byte then, since it dropped what came before it. (In line
    /// mode the line discipline has every key, and no byte waits here.)
    pub fn has_raw_interrupt(&self) -> bool {
        self.bytes.front() == Some(&INTERRUPT)
    }

    /// Takes a Ctrl-C typed in raw mode out of the input, if one waits to
    /// be read; whether one did.
    pub fn take_raw_interrupt(&mut self) -> bool {
        self.has_raw_interrupt() && self.pop().is_some()
    }
````

- [ ] **Step 11: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        t.wake_all(Blocked::Console);
    }
````

with:

````rust
        t.wake_all(Blocked::Console);
    }
    // A Ctrl-C at a shell's `wait` (`WAIT_CTRL_C`): the shell's group has
    // the console, in raw mode.
    if tty::has_raw_ctrl_c() {
        t.wake_waiting(tty::foreground());
    }
````

Replace:

````rust
/// `ECHILD` if there is no such child; `EINTR` if the running process was
/// killed while it waited (it ends on its way back to ring 3).
fn collect(child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
    let want = match child {
````

with:

````rust
/// `ECHILD` if there is no such child; `EINTR` if the running process was
/// killed while it waited (it ends on its way back to ring 3), or, with
/// `ctrl_c`, once a Ctrl-C is typed while its group has the console in raw
/// mode (the Ctrl-C is taken).
fn collect(child: Child, nohang: bool, ctrl_c: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
    let want = match child {
````

Replace:

````rust
            None if nohang => return Ok(None),
            None => block(Blocked::Wait),
        }
    }
}
````

with:

````rust
            None if nohang => return Ok(None),
            None if ctrl_c && holds_ctrl_c() => return Err(Errno::EINTR),
            None => block(Blocked::Wait),
        }
    }
}

/// Whether a Ctrl-C typed in raw mode is the running process's, its group
/// having the console: then it is taken.
fn holds_ctrl_c() -> bool {
    let mine = {
        let t = PROCS.lock();
        t.get(t.current())
            .is_some_and(|p| p.pgid == tty::foreground())
    };
    tty::poll();
    mine && tty::take_raw_ctrl_c()
}
````

Replace:

````rust
    // `ECHILD` before anything else is collected.
    if let Some((_, status)) = collect(Child::Pid(pid), true)? {
        return Ok(status);
    }
    loop {
        match collect(Child::Any, false)? {
            Some((ended, status)) if ended == pid => return Ok(status),
````

with:

````rust
    // `ECHILD` before anything else is collected.
    if let Some((_, status)) = collect(Child::Pid(pid), true, false)? {
        return Ok(status);
    }
    loop {
        match collect(Child::Any, false, false)? {
            Some((ended, status)) if ended == pid => return Ok(status),
````

Replace:

````rust

    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
        collect(child, nohang)
    }
````

with:

````rust

    fn wait(
        &mut self,
        child: Child,
        nohang: bool,
        ctrl_c: bool,
    ) -> Result<Option<(u32, WaitStatus)>, Errno> {
        collect(child, nohang, ctrl_c)
    }
````

- [ ] **Step 12: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
            .filter(|p| p.state == State::Blocked(why))
            .map(|p| p.pid)
````

with:

````rust
            .filter(|p| p.state == State::Blocked(why))
            .map(|p| p.pid)
            .collect();
        for pid in pids {
            self.wake(pid);
        }
    }

    /// Wakes the processes of group `pgid` that wait for a child: a Ctrl-C
    /// may be for them (`WAIT_CTRL_C`).
    pub fn wake_waiting(&mut self, pgid: u32) {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| p.pgid == pgid && p.state == State::Blocked(Blocked::Wait))
            .map(|p| p.pid)
````

- [ ] **Step 13: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Time, WaitStatus, encode};
````

with:

````rust
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_CTRL_C, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, ProcInfo, SpawnArgs, Time, WaitStatus, encode};
````

Replace:

````rust
    /// A child that has ended, with how; `None` if `nohang` and none has.
    /// `ECHILD` if there is no such child.
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
````

with:

````rust
    /// A child that has ended, with how; `None` if `nohang` and none has.
    /// `ECHILD` if there is no such child; with `ctrl_c`, `EINTR` once a
    /// Ctrl-C is typed while the program's group has the console in raw
    /// mode (`WAIT_CTRL_C`).
    fn wait(
        &mut self,
        child: Child,
        nohang: bool,
        ctrl_c: bool,
    ) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
````

Replace:

````rust
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
    if flags & !u64::from(WAIT_NOHANG) != 0 {
        return Err(Errno::EINVAL);
````

with:

````rust
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
    if flags & !u64::from(WAIT_NOHANG | WAIT_CTRL_C) != 0 {
        return Err(Errno::EINVAL);
````

Replace:

````rust
    }
    let Some((pid, w)) = caller.wait(child, flags & u64::from(WAIT_NOHANG) != 0)? else {
        return Ok(0);
````

with:

````rust
    }
    let nohang = flags & u64::from(WAIT_NOHANG) != 0;
    let ctrl_c = flags & u64::from(WAIT_CTRL_C) != 0;
    let Some((pid, w)) = caller.wait(child, nohang, ctrl_c)? else {
        return Ok(0);
````

- [ ] **Step 14: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
        .then(|| FOREGROUND.load(Ordering::Relaxed))
}
````

with:

````rust
        .then(|| FOREGROUND.load(Ordering::Relaxed))
}

/// Whether a Ctrl-C typed in raw mode waits to be read.
pub fn has_raw_ctrl_c() -> bool {
    INPUT.lock().has_raw_interrupt()
}

/// Takes a Ctrl-C typed in raw mode out of the input; whether one waited.
pub fn take_raw_ctrl_c() -> bool {
    INPUT.lock().take_raw_interrupt()
}
````

- [ ] **Step 15: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 25 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 388 tests.

- [ ] **Step 16: Run the `ctrlc` scenario**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 17: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 18: Commit**

````bash
git add crates kernel tests userland
git commit -F - <<'EOF'
feat(relay-abi,kernel): let a Ctrl-C end a wait that asks for it

At its prompt the shell has the console in raw mode, where Ctrl-C is
only a byte, so nothing could stop the coming `wait` built-in while a
background job runs on (spec §9.2). wait gains the flag WAIT_CTRL_C:
a Ctrl-C typed while the caller's group has the console in raw mode
ends it with EINTR and is taken from the input; the waiter's group is
woken when one comes. Without the flag a raw Ctrl-C stays input, and
EINTR still reaches no program but one that asks. The ABI gains a flag,
not a meaning, so its version stays 3. relay-rt gains the flag's
wrapper, sys::wait_with, and t-spawn ctrl-c shows it in the ctrlc
scenario.
EOF
````


### Task 5: A raw Ctrl-C wakes only the waits that asked for one

The prototype's review (minor, M9, and a declined finding) and decision 4: `console_input` woke every process of the console's group that waited for a child whenever a raw Ctrl-C waited to be read, so one that had not asked for `WAIT_CTRL_C` woke for nothing and blocked again on every pass of the idle task as long as the Ctrl-C sat unread. A wait with the flag now blocks as `Blocked::WaitCtrlC`, the only state `Table::wake_waiting` wakes, and a child's end wakes it as it wakes a plain wait (`ps` says `wait` for both). The review also found no test of the group check in `holds_ctrl_c`: its mutant passed every scenario. `t-spawn ctrl-c-apart` sets raw mode and sleeps while a child in a group of its own waits twice with the flag (for `sleep 2`, then `sleep 1`); `ctrlc` types Ctrl-C and `x` meanwhile, and the child's waits end by themselves while the parent reads both bytes. The red run is the kernel's tests, which cannot find `Blocked::WaitCtrlC`. Mutation checks: plain waiters woken, a `WaitCtrlC` parent not woken by its child's end and the flag's wait blocked as a plain one each fail a test; `holds_ctrl_c` without the group check fails `ctrlc` (the child's second wait takes the Ctrl-C).

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: Task 4's `WAIT_CTRL_C`, `Table::wake_waiting`.
- Produces: `table::Blocked::WaitCtrlC`; `t-spawn ctrl-c-apart`.

- [ ] **Step 1: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn a_group_s_waiters_are_woken_and_no_one_else() {
        let mut t = table();
````

with:

````rust
    #[test]
    fn a_ctrl_c_wakes_its_group_s_waiters_that_asked_for_it_and_no_one_else() {
        let mut t = table();
````

Replace:

````rust
        let d = add(&mut t, a, false);
        for (p, why) in [
            (a, Blocked::Wait),
            (b, Blocked::Console),
            (c, Blocked::Wait),
            (d, Blocked::Wait),
        ] {
````

with:

````rust
        let d = add(&mut t, a, false);
        let e = add(&mut t, a, false);
        for (p, why) in [
            (a, Blocked::WaitCtrlC),
            (b, Blocked::Console),
            (c, Blocked::WaitCtrlC),
            (d, Blocked::WaitCtrlC),
            (e, Blocked::Wait),
        ] {
````

Replace:

````rust
        );
        assert_eq!(state(&t, c), State::Blocked(Blocked::Wait), "another group");
    }
````

with:

````rust
        );
        assert_eq!(
            state(&t, c),
            State::Blocked(Blocked::WaitCtrlC),
            "another group"
        );
        // A wait that did not ask for a Ctrl-C would wake again and again
        // while one waits unread (the review's idle spin).
        assert_eq!(
            state(&t, e),
            State::Blocked(Blocked::Wait),
            "it did not ask"
        );
        assert_eq!(
            t.list(|_| 0)[4].state,
            STATE_WAIT,
            "ps says wait either way"
        );
    }

    #[test]
    fn a_child_s_end_wakes_a_parent_that_waits_for_a_ctrl_c_too() {
        let mut t = table();
        let parent = add(&mut t, 0, true);
        let child = add(&mut t, parent, true);
        assert_eq!(t.schedule(), parent);
        t.block(Blocked::WaitCtrlC);
        assert_eq!(t.schedule(), child);
        t.end(child, WaitStatus::exited(0));
        assert_eq!(state(&t, parent), State::Ready);
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, replace:

````text
expect \Await: EINTR\nthen read: "x"\n
expect root@relay:~# $
````

with:

````text
expect \Await: EINTR\nthen read: "x"\n
expect root@relay:~# $
# A program waiting with WAIT_CTRL_C in a group that does not have the
# console never takes a Ctrl-C: it is for the console's group, here the
# parent, which reads it after the child's two waits ended by themselves.
send t-spawn ctrl-c-apart
expect \nthe child waits apart\n
alive 1
type {ctrl-c}x
expect \Athe child's wait: a child\nthe child's wait: a child\nthen read: "\\u\{3\}x"\n
expect root@relay:~# $
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   the wait, and is taken from what was typed, which the program then
//!   reads (milestone 3);
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
````

with:

````rust
//!   the wait, and is taken from what was typed, which the program then
//!   reads (milestone 3); `t-spawn ctrl-c-apart` does the same but waits
//!   in a child in a group of its own, twice, for `sleep 2` and then
//!   `sleep 1`, while it sleeps 3.5 s itself: a Ctrl-C typed meanwhile is
//!   the parent's, which has the console, never the child's;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
````

Replace:

````rust
        Some(b"ctrl-c") => ctrl_c(),
        Some(b"kill") => kill(),
````

with:

````rust
        Some(b"ctrl-c") => ctrl_c(),
        Some(b"ctrl-c-apart") => ctrl_c_apart(),
        Some(b"ctrl-c-child") => ctrl_c_child(),
        Some(b"kill") => kill(),
````

Replace:

````rust
    sys::console_mode(was)?;
    Ok(())
}
````

with:

````rust
    sys::console_mode(was)?;
    Ok(())
}

/// `ctrl-c-apart`: the console raw and this program's, its child apart.
fn ctrl_c_apart() -> Result<(), u16> {
    use relay_abi::console::MODE_RAW;
    use relay_abi::spawn::NEW_GROUP;
    let was = sys::console_mode(MODE_RAW)?;
    let args = b"t-spawn\0ctrl-c-child\0";
    let child = sys::spawn(b"/bin/t-spawn", args, b"", &STD, NEW_GROUP, 0)?;
    let _ = writeln!(Fd(1), "the child waits apart");
    sys::sleep(3500);
    let mut typed = [0u8; 16];
    let n = sys::read(0, &mut typed)?;
    let _ = writeln!(
        Fd(1),
        "then read: {:?}",
        core::str::from_utf8(&typed[..n]).unwrap_or("?")
    );
    sys::wait(i64::from(child), false)?;
    sys::console_mode(was)?;
    Ok(())
}

/// `ctrl-c-child`: two waits with `WAIT_CTRL_C`, in a group without the
/// console.
fn ctrl_c_child() -> Result<(), u16> {
    use relay_abi::spawn::WAIT_CTRL_C;
    for args in [&b"sleep\x002\0"[..], b"sleep\x001\0"] {
        let pid = sys::spawn(b"/bin/sleep", args, b"", &STD, 0, 0)?;
        let r = sys::wait_with(i64::from(pid), WAIT_CTRL_C);
        let _ = writeln!(
            Fd(1),
            "the child's wait: {}",
            r.map_or_else(|e| relay_abi::errno::name(e).unwrap_or("?"), |_| "a child")
        );
        if r.is_err() {
            sys::wait(i64::from(pid), false)?;
        }
    }
    Ok(())
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `WaitCtrlC` found for enum `table::Blocked` in the current scope ``.

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
            None if ctrl_c && holds_ctrl_c() => return Err(Errno::EINTR),
            None => block(Blocked::Wait),
````

with:

````rust
            None if ctrl_c && holds_ctrl_c() => return Err(Errno::EINTR),
            None if ctrl_c => block(Blocked::WaitCtrlC),
            None => block(Blocked::Wait),
````

- [ ] **Step 6: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    Wait,
    /// Console input.
````

with:

````rust
    Wait,
    /// A child to end, or a Ctrl-C typed in raw mode while its group has
    /// the console (`WAIT_CTRL_C`).
    WaitCtrlC,
    /// Console input.
````

Replace:

````rust
            State::Ready => STATE_READY,
            State::Blocked(Blocked::Wait) => STATE_WAIT,
            State::Blocked(Blocked::Console) => STATE_READ,
````

with:

````rust
            State::Ready => STATE_READY,
            State::Blocked(Blocked::Wait | Blocked::WaitCtrlC) => STATE_WAIT,
            State::Blocked(Blocked::Console) => STATE_READ,
````

Replace:

````rust

    /// Wakes the processes of group `pgid` that wait for a child: a Ctrl-C
    /// may be for them (`WAIT_CTRL_C`).
    pub fn wake_waiting(&mut self, pgid: u32) {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| p.pgid == pgid && p.state == State::Blocked(Blocked::Wait))
            .map(|p| p.pid)
````

with:

````rust

    /// Wakes the processes of group `pgid` that wait for a child or a
    /// Ctrl-C (`WAIT_CTRL_C`): one was typed. The others it is no news to,
    /// and would wake for nothing as long as it waits to be read.
    pub fn wake_waiting(&mut self, pgid: u32) {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| p.pgid == pgid && p.state == State::Blocked(Blocked::WaitCtrlC))
            .map(|p| p.pid)
````

Replace:

````rust
        {
            if self.get(waiter).map(|p| p.state) == Some(State::Blocked(Blocked::Wait)) {
                self.wake(waiter);
````

with:

````rust
        {
            if matches!(
                self.get(waiter).map(|p| p.state),
                Some(State::Blocked(Blocked::Wait | Blocked::WaitCtrlC))
            ) {
                self.wake(waiter);
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 389 tests.

- [ ] **Step 8: Run the `ctrlc` scenario**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add kernel tests userland
git commit -F - <<'EOF'
fix(kernel): wake for a raw Ctrl-C only the waits that asked for one

console_input woke every process of the console's group that waited
for a child whenever a raw Ctrl-C waited to be read: one that had not
asked for WAIT_CTRL_C woke for nothing and blocked again, on every
pass of the idle task, as long as the Ctrl-C sat unread (the review's
idle spin). A wait with the flag now blocks as Blocked::WaitCtrlC, the
only state a Ctrl-C wakes, and a child's end wakes it as it wakes a
plain wait. t-spawn ctrl-c-apart waits twice with the flag in a group
that does not have the console: the Ctrl-C typed meanwhile stays its
parent's, which no test showed (the review's mutant without the group
check passed every scenario).

Refs: review M9
EOF
````


### Task 6: A new pipe's ends never close while the table is locked

Plan 1's final review (deferred minor 4) and decision 11: `pipe()` put its two ends in the fd table inside `with_fds`, which holds `PROCS` in the kernel; had the second insert failed, the end it consumed would have closed there, waking the pipe's waiters under `PROCS`, which `proc::wake_pipe` asserts never happens. Two free fds are checked first, so it could not fail, but the rule held by that guarantee alone. Now the table gets copies of the ends, both or neither (`insert_ends` takes the first out again if the second does not fit), and the last references drop after `with_fds` returns. The red run is the kernel's tests, which cannot find `insert_ends`. Mutation check: the first end left in the table fails the test.

**Files:**
- Modify: `kernel/src/syscall/pipes.rs`

**Interfaces:**
- Consumes: plan 1's `pipe` call.
- Produces: `syscall::pipes::insert_ends(t: &mut FdTable, read: &Arc<File>, write: &Arc<File>) -> Result<[u32; 2], Errno>` (private).

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall/pipes.rs`**

In `kernel/src/syscall/pipes.rs`, replace:

````rust

    /// Opens `/root/f` as the next fd.
````

with:

````rust

    #[test]
    fn a_new_pipe_s_ends_go_in_both_or_neither_and_never_close_inside() {
        let mut f = fake();
        let (read, write) = f.new_pipe().unwrap();
        let (read, write) = (Arc::new(File::Pipe(read)), Arc::new(File::Pipe(write)));
        for _ in 3..31 {
            open_any(&mut f);
        }
        assert_eq!(f.fds.open(), 31);
        assert_eq!(
            super::insert_ends(&mut f.fds, &read, &write),
            Err(Errno::EMFILE)
        );
        assert_eq!(f.fds.open(), 31, "neither end stays");
        assert_eq!(
            (Arc::strong_count(&read), Arc::strong_count(&write)),
            (1, 1),
            "the caller has the last references"
        );
        assert_eq!(call(&mut f, Call::Close, [30, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Close, [29, 0, 0]), Ok(0));
        assert_eq!(super::insert_ends(&mut f.fds, &read, &write), Ok([29, 30]));
        assert_eq!(
            (Arc::strong_count(&read), Arc::strong_count(&write)),
            (2, 2)
        );
    }

    /// Opens `/root/f` as the next fd.
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `insert_ends` in module `super` ``.

- [ ] **Step 3: Change `kernel/src/syscall/pipes.rs`**

In `kernel/src/syscall/pipes.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use super::Caller;
use crate::fd::{FDS, File};
use crate::mm::paging::PAGE;
````

with:

````rust
use super::Caller;
use crate::fd::{FDS, FdTable, File};
use crate::mm::paging::PAGE;
````

Replace:

````rust
    let (read, write) = (Arc::new(File::Pipe(read)), Arc::new(File::Pipe(write)));
    let fds = caller.with_fds(|t| Ok::<_, Errno>([t.insert(read)?, t.insert(write)?]))?;
    let mut bytes = [0u8; 8];
````

with:

````rust
    let (read, write) = (Arc::new(File::Pipe(read)), Arc::new(File::Pipe(write)));
    let fds = caller.with_fds(|t| insert_ends(t, &read, &write));
    // Here, outside `with_fds` (the kernel's holds `PROCS`), go the last
    // references to the ends if they did not go in: dropping an end wakes.
    drop((read, write));
    let fds = fds?;
    let mut bytes = [0u8; 8];
````

Replace:

````rust
    Ok(0)
}
````

with:

````rust
    Ok(0)
}

/// Puts both ends of a new pipe in `t`, at its lowest free fds, or neither:
/// what goes in, and what is taken out again, are copies, so the table
/// never holds the last reference to an end and dropping none of them
/// closes the pipe (and wakes its waiters) while it is locked.
fn insert_ends(t: &mut FdTable, read: &Arc<File>, write: &Arc<File>) -> Result<[u32; 2], Errno> {
    let r = t.insert(read.clone())?;
    match t.insert(write.clone()) {
        Ok(w) => Ok([r, w]),
        Err(e) => {
            let _ = t.remove(u64::from(r));
            Err(e)
        }
    }
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 390 tests.

- [ ] **Step 5: Run the `pipe_calls` scenario**

Run: `cargo xtask test --e2e-only --scenario pipe_calls`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
fix(kernel): never close a new pipe's end while the table is locked

pipe() put its two ends in the fd table inside with_fds, which holds
PROCS in the kernel; had the second insert failed, the end it consumed
would have closed there, waking the pipe's waiters under PROCS, which
proc::wake_pipe asserts never happens. Two free fds are checked first,
so it could not fail, but the rule held by that guarantee alone. Now
the table gets copies of the ends, both or neither (the first taken
out again if the second does not fit), and the last references drop
after with_fds returns.

Refs: plan 1's final review, minor 4
EOF
````


### Task 7: The error screen takes the console back from a command

Spec §11.2 and decision 10, milestone 2's leftover: the error screen takes the console back (raw mode, process 1's group, the chain reset), which nothing showed, since in milestone 2 nothing a person ran could end the shell while its command had the console. `kill` can now. `t-proc end-shell` reads the console, as the shell gave it, in line mode, while a child in a group of its own finds the shell in the list half a second later and kills it; the new scenario `screen_console` does that after two quick `exit`s, so that it is the third end within 10 s: the screen ends every other process and comes up, and a key restarts the machine, which it would not in line mode, where the key waits in the line discipline and the screen never sees it. The task adds a test of code that exists, so it has no failing run. Mutation check: `take_console` without the switch to raw mode fails the scenario (the key never reaches the screen).

**Files:**
- Create: `tests/e2e/screen_console.txt`
- Modify: `userland/tests/src/bin/t-proc.rs`

**Interfaces:**
- Consumes: Task 2's `t-proc` and `sys::proc_list`; plan 4b's error screen.
- Produces: `t-proc end-shell`.

- [ ] **Step 1: Add the scenario `tests/e2e/screen_console.txt`**

Create `tests/e2e/screen_console.txt`:

````text
# The error screen takes the console back (user-space gate §11.2, §16
# items 6 and 9; milestone 3, plan 2): the shell ends for the third time
# within 10 s while its command holds the console in line mode (t-proc
# reads it while a child of its own kills the shell). The screen ends
# every other process, gives the console to process 1 in raw mode, and a
# key alone restarts the machine: in line mode it would wait in the line
# discipline, where the screen never sees it.
timeout 40
expect root@relay:~# $
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
send exit
expect \ninit: /bin/sh \(pid \d+\) exited with 0; starting it again\nroot@relay:~# $
send t-proc end-shell
expect \nreading the console\nkilling pid \d+\n
expect \ninit: /bin/sh \(pid \d+\) killed: kill\n
expect \*\*\* Relay OS cannot run its shell \*\*\*\n\n/bin/sh ended 3 times within 10 s\n\n--- last kernel log lines ---\n
expect \nPress any key to reboot\.\n
expect pid \d+ \(/bin/t-proc\): killed: kill\n
alive 1
reset
expect Relay OS \d+\.\d+\.\d+
expect root@relay:~# $
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-proc.rs`**

In `userland/tests/src/bin/t-proc.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   its first 64.
#![no_std]
````

with:

````rust
//!   its first 64.
//! - `t-proc end-shell`: reads the console, as the shell gave it (in line
//!   mode), while a child in a group of its own finds the shell in the
//!   list and kills it half a second later: the shell ends while its
//!   command holds the console (the error screen must take it back).
#![no_std]
````

Replace:

````rust
        Some(b"long") => long(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-proc list|short|long\n");
            return 2;
````

with:

````rust
        Some(b"long") => long(),
        Some(b"end-shell") => end_shell(),
        Some(b"kill-grandparent") => kill_grandparent(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-proc list|short|long|end-shell\n");
            return 2;
````

Replace:

````rust
    sys::wait(i64::from(child), false)?;
    Ok(())
}
````

with:

````rust
    sys::wait(i64::from(child), false)?;
    Ok(())
}

fn end_shell() -> Result<(), u16> {
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let args = b"t-proc\0kill-grandparent\0";
    sys::spawn(
        b"/bin/t-proc",
        args,
        b"",
        &fds,
        relay_abi::spawn::NEW_GROUP,
        0,
    )?;
    let _ = writeln!(Fd(1), "reading the console");
    let mut buf = [0u8; 64];
    // Until the error screen ends it; 100 lines at most.
    for _ in 0..100 {
        if sys::read(0, &mut buf)? == 0 {
            break;
        }
    }
    Ok(())
}

/// Kills the parent of this program's parent, half a second from now.
fn kill_grandparent() -> Result<(), u16> {
    sys::sleep(500);
    let mut buf = [ProcInfo::new(0, 0, 0, 0, 0, 0, b""); PROC_MAX];
    let all = processes(&mut buf)?;
    let parent = entry(all, sys::getpid()).map_or(0, |p| p.ppid);
    let shell = entry(all, parent).map_or(0, |p| p.ppid);
    let _ = writeln!(Fd(1), "killing pid {shell}");
    sys::kill(i64::from(shell))
}
````

- [ ] **Step 3: Run the `screen_console`, `respawn` scenarios**

Run: `cargo xtask test --e2e-only --scenario screen_console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario respawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests userland
git commit -F - <<'EOF'
test(e2e,userland): show the error screen taking the console back

The error screen takes the console back from whoever had it, raw and
for process 1, which nothing showed: in milestone 2 nothing a person ran
could end the shell while its command had the console. Now kill can.
The scenario screen_console ends the shell for the third time within
10 s while t-proc reads the console in line mode, its child killing the
shell; then a key restarts the machine, which it does only in raw mode
(in line mode the key waits in the line discipline, where the screen
never sees it).
EOF
````


### Task 8: Each process pushes 4 tees, 64 in all

The prototype's review (important, I1) and decision 5: the tee stack held 4 tees for the whole machine, a bound made for nested scripts, and each background script keeps its transcript's tee while it runs, so four `sh a.sh &` kept every further script, foreground ones too, from starting (`sh: cannot write the transcript b.log: Device or resource busy`). The user chose: a process pushes at most 4 (`TEES_EACH`), and the stack holds 64 (`TEES`), one for each process the table can hold; nested scripts, one tee each, are no longer stopped at four deep. `t-tee owners` starts five children that each push a tee while the others hold theirs, in `tees`; `t-tee basic`'s `pushed 4, then EBUSY` (one process) stays. The red run is the kernel's tests, which push a fifth owner's tee. Mutation checks: the stack back at 4 in all fails `tees` (the fifth child's push is `EBUSY`); no limit for a process and no limit in all each fail a test.

**Files:**
- Modify: `kernel/src/tee.rs`
- Modify: `tests/e2e/tees.txt`
- Modify: `userland/tests/src/bin/t-tee.rs`

**Interfaces:**
- Consumes: plan 3b's tee stack.
- Produces: `tee::{TEES, TEES_EACH}`; `t-tee owners`.

- [ ] **Step 1: Add the failing tests to `kernel/src/tee.rs`**

In `kernel/src/tee.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn at_most_4_tees_and_each_gets_a_copy() {
        let mut s = TeeStack::new();
````

with:

````rust
    #[test]
    fn at_most_4_tees_a_process_and_each_gets_a_copy() {
        let mut s = TeeStack::new();
````

Replace:

````rust
            assert_eq!(files.of(f), b"hello\n");
        }
    }

````

with:

````rust
            assert_eq!(files.of(f), b"hello\n");
        }
    }

    #[test]
    fn other_processes_push_theirs_until_64_are_pushed() {
        let mut s = TeeStack::new();
        for f in 1..=4 {
            s.push(10, f).unwrap();
        }
        // Background scripts, one tee each (the review found the fifth
        // script refused at a stack of 4 for the whole machine).
        for owner in 11..71 {
            s.push(owner, owner).unwrap();
        }
        assert_eq!(s.push(71, 71), Err(Errno::EBUSY), "64 in all");
        let mut files = Files::default();
        s.pop(70, &mut files.writer()).unwrap();
        s.push(71, 71).unwrap();
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY), "still 4 for 10");
    }

````

- [ ] **Step 2: Expect the new lines in `tests/e2e/tees.txt`**

In `tests/e2e/tees.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# programs and the shell write to the console, written every 4 KiB and at
# every sync, until it is popped; at most 4; a process's tees end with it;
# a tee whose file is removed is gone.
timeout 60
````

with:

````text
# programs and the shell write to the console, written every 4 KiB and at
# every sync, until it is popped; at most 4 a process (64 in all,
# milestone 3); a process's tees end with it; a tee whose file is removed is
# gone.
timeout 60
````

Replace:

````text
send rm t-tee.log t-tee.end
````

with:

````text
send rm t-tee.log t-tee.end
# Five processes with a tee each at once, as background scripts have.
send t-tee owners
expect \n(a tee of its own: ok\n){5}root@relay:~# $
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-tee.rs`**

In `userland/tests/src/bin/t-tee.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   and its pop says `ENOSPC`.
#![no_std]
````

with:

````rust
//!   and its pop says `ENOSPC`.
//! - `t-tee owners`: five children push a tee each, all pushed at once (as
//!   background scripts' transcripts are): each gets its own; a process
//!   may push four (milestone 3).
#![no_std]
````

Replace:

````rust
        Some(b"typed") => typed(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|typed|full\n");
            return 2;
````

with:

````rust
        Some(b"typed") => typed(),
        Some(b"owners") => owners(),
        Some(b"owner") => owner(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|typed|full|owners\n");
            return 2;
````

Replace:

````rust
    sys::unlink(b"t-tee.typed")
}
````

with:

````rust
    sys::unlink(b"t-tee.typed")
}

fn owners() -> Result<(), u16> {
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let mut pids = [0u32; 5];
    for pid in &mut pids {
        *pid = sys::spawn(b"/bin/t-tee", b"t-tee\0owner\0", b"", &fds, 0, 0)?;
    }
    for pid in pids {
        sys::wait(i64::from(pid), false)?;
    }
    Ok(())
}

/// One of `owners`' children: a tee of its own while its siblings have
/// theirs.
fn owner() -> Result<(), u16> {
    let fd = create(b"t-tee.owner")?;
    let pushed = sys::console_tee_push(fd);
    sys::close(fd)?;
    sys::sleep(500);
    show_ok("a tee of its own", pushed);
    if pushed.is_ok() {
        sys::console_tee_pop()?;
    }
    Ok(())
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `tee::tests::other_processes_push_theirs_until_64_are_pushed`.

- [ ] **Step 5: Change `kernel/src/tee.rs`**

In `kernel/src/tee.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! The console's tees (user-space gate §6.5): files that get a copy of
//! everything written to the console. At most 4 are pushed at a time; a
//! process pops the newest one it pushed. What they get is buffered and
//! written once 4 KiB wait, and all of it at every `sync` and when the tee
````

with:

````rust
//! The console's tees (user-space gate §6.5): files that get a copy of
//! everything written to the console. A process pushes at most 4 at a
//! time, and the stack holds at most 64, one for each process the table
//! can hold, so a background script's transcript never keeps another
//! script from starting (milestone 3); a process pops the newest one it
//! pushed. What they get is buffered and
//! written once 4 KiB wait, and all of it at every `sync` and when the tee
````

Replace:

````rust

/// Tees on the stack at most.
pub const TEES: usize = 4;
/// A tee's copies are written once this much waits.
````

with:

````rust

/// Tees on the stack at most: one for each process the table can hold.
pub const TEES: usize = 64;
/// Tees one process may have pushed at a time.
pub const TEES_EACH: usize = 4;
/// A tee's copies are written once this much waits.
````

Replace:

````rust

    /// Pushes `file` for process `owner`. `EBUSY` if 4 tees are pushed.
    pub fn push(&mut self, owner: u32, file: F) -> Result<(), Errno> {
        if self.tees.len() >= TEES {
            return Err(Errno::EBUSY);
````

with:

````rust

    /// Pushes `file` for process `owner`. `EBUSY` if `owner` has 4 tees
    /// pushed, or the stack holds 64.
    pub fn push(&mut self, owner: u32, file: F) -> Result<(), Errno> {
        let mine = self.tees.iter().filter(|t| t.owner == owner).count();
        if mine >= TEES_EACH || self.tees.len() >= TEES {
            return Err(Errno::EBUSY);
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 391 tests.

- [ ] **Step 7: Run the `tees`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel tests userland
git commit -F - <<'EOF'
fix(kernel): let each process push 4 tees, 64 in all

The tee stack held 4 tees for the whole machine, a bound made for
nested scripts; each background script keeps its transcript's tee while
it runs, so four `sh a.sh &` kept every further script, foreground ones
too, from starting (`sh: cannot write the transcript b.log: Device or
resource busy`). A process may now push 4, and the stack holds 64, one
for each process the table can hold. Nested scripts, one tee each, are
no longer stopped at four deep. t-tee owners has five processes push a
tee each at once in the tees scenario.

Refs: review I1
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 45 scenario(s) passed`.

````bash
git push -u origin m3p2/kernel
gh pr create --base main --head m3p2/kernel --title "feat(kernel,relay-rt): add proc_list, the console's holders and wait's Ctrl-C" --body-file - <<'EOF'
## What

Milestone 3, plan 2, tasks 1–8: `proc_list` fills a buffer with a `ProcInfo` per process (pid, parent, group, what it waits for, its frames, its CPU ticks, its name), as many as fit, and returns the count of all; the process table keeps the chain of groups that handed the console on, and `console_mode`, `console_foreground` and `spawn`'s `FOREGROUND` are `EPERM` outside it, so a background job can neither take the console nor change its mode; `wait` gains `WAIT_CTRL_C`, which a raw Ctrl-C ends with `EINTR` (and wakes no other wait); `pipe()` puts copies of its ends in the fd table, both or neither; the error screen is shown taking the console back from a command that held it in line mode; a process pushes 4 tees, 64 in all. Scenarios `proc_calls` and `screen_console` (new), `console`, `ctrlc` and `tees`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types the Ctrl-C a `wait` needs); plan 4's NUC check 5 runs a background job and `kill` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p2/kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Background jobs, `jobs`, `wait` and `kill` in the shell (Tasks 9–23)

The shell's side: the parser's `&`, the table of jobs in bash's words, background jobs under `/bin/sh` with their Done lines, and the built-ins `jobs`, `wait` and `kill`; with the review's fixes: `>&`, a fault's words, collecting before each line, `sh &`, `jobs`' order, `wait`'s notices, `kill` of an ended job and bash's job specs. The scenario `jobs` runs them in QEMU.

Branch `m3p2/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p2/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p2/kernel` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p2/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-shell m3p2/kernel`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p2/kernel>` and re-run `cargo xtask ci` before pushing.

### Task 9: The parser reads a line that ends with `&`

Spec §9.2 and decision 6: an unquoted `&` at the end of a line (a comment may follow) runs it in the background. `parser::parse_line` returns a `Line`: the pipeline, and what was typed before the `&` without the blanks around it, the job's text for `jobs` and the Done lines; `parse` still gives the pipeline alone. Elsewhere `&` gets bash's syntax error (``syntax error near unexpected token `&'`` for `&` alone, `a | &`, `echo > &`, `a & &`) or is refused as unsupported (`a & b`, `&&` and `> f &`, which bash runs). The shell still refuses a background line (`unsupported syntax: &`) until Task 13 starts one. The red run is the shell's tests, which cannot find `parse_line`. Mutation checks: `a & &` taken, `a & b` taken, the text not trimmed, a pending redirection before `&` taken and `&&` read as two `&`s each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: plan 1's `parser::parse`.
- Produces: `parser::Line { pipeline: Pipeline, background: Option<String> }`, `parser::parse_line(line: &str) -> Result<Line, ParseError>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            ("a; b", ';'),
            ("a && b", '&'),
            ("echo $HOME", '$'),
````

with:

````rust
            ("a; b", ';'),
            ("echo $HOME", '$'),
````

Replace:

````rust
    #[test]
    fn unterminated_quotes_are_errors() {
````

with:

````rust
    #[test]
    fn a_line_ending_with_an_ampersand_runs_in_the_background() {
        let l = parse_line("sleep 5 &").unwrap();
        assert_eq!(l.pipeline[0].words, ["sleep", "5"]);
        assert_eq!(l.background.as_deref(), Some("sleep 5"));
        // The text is what was typed before the `&`, without the blanks
        // around it; a comment may follow.
        for (line, text) in [
            ("  cat f |  wc -l>out& ", "cat f |  wc -l>out"),
            ("echo 'a  b' \\& &\t# later", "echo 'a  b' \\&"),
            ("t-spin&", "t-spin"),
            ("grep x f | head -n 1 & # one", "grep x f | head -n 1"),
        ] {
            let l = parse_line(line).unwrap();
            assert_eq!(l.background.as_deref(), Some(text), "{line}");
        }
        let l = parse_line("cat f | wc -l > out &").unwrap();
        assert_eq!(l.pipeline.len(), 2);
        assert_eq!(l.pipeline[1].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
        for line in ["echo '&' \"&\" \\&", "echo a # &", "echo a"] {
            assert_eq!(parse_line(line).unwrap().background, None, "{line}");
        }
        assert_eq!(words("echo '&' \\& # &"), ["echo", "&", "&"]);
    }

    #[test]
    fn an_ampersand_anywhere_else_is_bash_s_error_or_unsupported() {
        // bash's messages (`bash -c '&'`, `bash -c 'a | &'`, …).
        for line in ["&", " & ", "a | &", "echo > &", "a & &", "a &&&"] {
            let e = parse_line(line).unwrap_err();
            if line.contains("&&") {
                assert_eq!(e, ParseError::Unsupported("&&".into()), "{line}");
            } else {
                assert_eq!(
                    e.to_string(),
                    "syntax error near unexpected token `&'",
                    "{line}"
                );
            }
        }
        // bash runs `a & b`, `a && b` and `> f &`; they are not supported.
        for (line, what) in [
            ("a & b", "&"),
            ("a &b", "&"),
            ("a & | b", "&"),
            ("a && b", "&&"),
            ("a &&", "&&"),
            ("> f &", "> &"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn unterminated_quotes_are_errors() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `parse_line` in this scope ``.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! only the last may redirect its output, and bash's syntax errors name a
//! `|` with no command before it or none after. Every other shell feature is refused:
//! an unquoted `;`, `&`, `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an
//! error naming the character, instead of being passed on as if it were
//! plain text; so are `||` and `2>` (another stream).

````

with:

````rust
//! only the last may redirect its output, and bash's syntax errors name a
//! `|` with no command before it or none after. An unquoted `&` at the end
//! of the line (a comment may follow) runs it in the background (§9.2).
//! Every other shell feature is refused: an unquoted `;`, `&` before more,
//! `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an error naming the
//! character, instead of being passed on as if it were plain text; so are
//! `||`, `&&` and `2>` (another stream).

````

Replace:

````rust
pub type Pipeline = Vec<Command>;

````

with:

````rust
pub type Pipeline = Vec<Command>;

/// A command line: its pipeline, and, if it ends with `&`, what was typed
/// before the `&` (a background job's text, spec §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pipeline: Pipeline,
    pub background: Option<String>,
}

````

Replace:

````rust

const UNSUPPORTED: &[char] = &[';', '&', '$', '*', '?', '<', '`', '(', ')'];

````

with:

````rust

const UNSUPPORTED: &[char] = &[';', '$', '*', '?', '<', '`', '(', ')'];

````

Replace:

````rust

pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    let mut pipeline = Vec::new();
````

with:

````rust

/// The commands of `line`, whether or not it ends with `&`.
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| l.pipeline)
}

/// `line`'s commands, and whether it runs in the background.
pub fn parse_line(line: &str) -> Result<Line, ParseError> {
    let mut background = None;
    let mut pipeline = Vec::new();
````

Replace:

````rust
                pipeline.push(parts.take_before_pipe()?);
            }
````

with:

````rust
                pipeline.push(parts.take_before_pipe()?);
            }
            '&' => {
                if chars.next_if_eq(&'&').is_some() {
                    return Err(ParseError::Unsupported("&&".into()));
                }
                parts.end_word(&mut word)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
                    return Err(ParseError::MissingTarget("&"));
                }
                if parts.words.is_empty() {
                    // `> f &`: a background job is a program.
                    return Err(ParseError::Unsupported("> &".into()));
                }
                let rest: String = chars.clone().collect();
                let after = rest.trim_start_matches([' ', '\t']);
                if after.starts_with('&') {
                    return Err(ParseError::MissingTarget("&"));
                }
                if !after.is_empty() && !after.starts_with('#') {
                    // `a & b` runs both in bash.
                    return Err(ParseError::Unsupported("&".into()));
                }
                let typed = &line[..line.len() - rest.len() - 1];
                background = Some(String::from(typed.trim_matches([' ', '\t'])));
                break;
            }
````

Replace:

````rust
    });
    Ok(pipeline)
}
````

with:

````rust
    });
    Ok(Line {
        pipeline,
        background,
    })
}
````

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse(line) {
            Ok(pipeline) => pipeline,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
````

with:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse_line(line) {
            Ok(parser::Line {
                background: Some(_),
                ..
            }) => {
                return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
            }
            Ok(line) => line.pipeline,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 192 tests.

- [ ] **Step 6: Run the `shell` scenario**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): parse a line that ends with & as a background job

An unquoted & at the end of a line (a comment may follow) makes it a
background job (spec §9.2): parser::parse_line gives the pipeline and
what was typed before the &, the job's text for `jobs` and the Done
lines. Elsewhere & gets bash's syntax error (`&` alone, `a | &`,
`echo > &`, `a & &`) or is refused as unsupported (`a & b`, `&&`,
`> f &`, which bash runs). The shell still refuses a background job
until its runner starts one.
EOF
````


### Task 10: `>&` is unsupported, not bash's syntax error

The prototype's review (minor, M5) and decision 6: with `&` read at the end of a line, `echo hi >&2` and `echo hi >& f` became ``syntax error near unexpected token `&'``, which bash says only of `>>&` and `> &`; bash runs the others, which send output to another fd or a file, and milestone 1's shell called them unsupported. They are `unsupported syntax: >&` now; `>>&2` and `> &2` keep bash's error. The red run is the shell's tests. Mutation check: `>&` read as bash's error again fails the test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 9's parser.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        // bash's messages (`bash -c '&'`, `bash -c 'a | &'`, …).
        for line in ["&", " & ", "a | &", "echo > &", "a & &", "a &&&"] {
            let e = parse_line(line).unwrap_err();
````

with:

````rust
        // bash's messages (`bash -c '&'`, `bash -c 'a | &'`, …).
        for line in [
            "&",
            " & ",
            "a | &",
            "echo > &",
            "echo >>&2",
            "a & &",
            "a &&&",
        ] {
            let e = parse_line(line).unwrap_err();
````

Replace:

````rust
            ("> f &", "> &"),
        ] {
````

with:

````rust
            ("> f &", "> &"),
            // bash runs these (the review found them called its syntax
            // error).
            ("echo hi >&2", ">&"),
            ("echo hi >& f", ">&"),
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::an_ampersand_anywhere_else_is_bash_s_error_or_unsupported`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
                }
                parts.pending = Some(chars.next_if_eq(&'>').is_some());
            }
````

with:

````rust
                }
                let append = chars.next_if_eq(&'>').is_some();
                // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                // `> &` are its syntax errors.
                if !append && chars.peek() == Some(&'&') {
                    return Err(ParseError::Unsupported(">&".into()));
                }
                parts.pending = Some(append);
            }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 192 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse >& as unsupported, not as bash's syntax error

With & read at the end of a line, `echo hi >&2` and `echo hi >& f`
became ``syntax error near unexpected token `&'``, which bash says only
of `>>&` and `> &`; bash runs the others, which send output to another
fd or a file, and milestone 1's shell called them unsupported. They are
`unsupported syntax: >&` now; `>>&2` and `> &2` keep bash's error.

Refs: review M5
EOF
````


### Task 11: The table of background jobs

Spec §9.2 and decision 7: `shell::jobs::Jobs` keeps what background lines started and says it as bash does. A new job is numbered one past the highest in the table, so numbers start again only once it is empty (bash 5.2, in a terminal, numbered 4 and 5 after jobs 1 and 2 ended, not 1 and 2); the newest is `+`, the one before `-`; a job has ended when every process it started has, and its status is its last one's (`status_of`: bash's for a signal when it was killed). Its lines are bash's, the state padded to 24 columns: `[1]+  Done                    sleep 5`, `Exit 1`, `Killed`, and `Running` with ` &` after the text, rather than spec §9.2's shorter form (corrected in its body); `list` gives `jobs`' lines and drops the jobs that have ended, `report` those of the jobs that have ended. The tests compare with what bash 5.2 printed for the same jobs. The red run is the shell's tests, which cannot find the module. Mutation checks: numbering by count, no `-`, ` &` on a Done line, the first process's status, `jobs` keeping finished jobs and a pid recorded twice each fail a test (padding to 23 and a space is the same line for every state, all shorter than 23).

**Files:**
- Create: `crates/shell/src/jobs.rs`
- Modify: `crates/shell/src/lib.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `shell::jobs::Jobs` (`new`, `is_empty`, `add(&mut self, pgid, pids: &[u32], text) -> u32`, `ended(&mut self, pid, status) -> bool`, `has`, `of_pid`, `pgid`, `running`, `all_running`, `status(number) -> Option<i32>`, `list(&mut self) -> Vec<String>`, `report`, `take(number) -> Option<String>`, `forget_finished`), `jobs::status_of(&WaitStatus) -> i32`.

- [ ] **Step 1: Write the failing tests for `crates/shell/src/jobs.rs`**

Create `crates/shell/src/jobs.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_KILL};

    fn killed() -> WaitStatus {
        WaitStatus::killed(KILLED_KILL)
    }

    #[test]
    fn jobs_are_numbered_one_past_the_highest_as_bash_numbers_them() {
        let mut j = Jobs::new();
        assert_eq!(j.add(10, &[10], "sleep 30"), 1);
        assert_eq!(j.add(11, &[11], "sleep 30"), 2);
        assert_eq!(j.add(12, &[12], "sleep 30"), 3);
        // bash 5.2: with 1, 2 and 3 started, 1 and 2 killed and reported,
        // the next two are 4 and 5, not 1 and 2.
        for pid in [10, 11] {
            assert!(j.ended(pid, killed()));
        }
        assert_eq!(j.report().len(), 2);
        assert_eq!(j.add(13, &[13], "sleep 30"), 4);
        assert_eq!(j.add(14, &[14], "sleep 30"), 5);
        for pid in [12, 13, 14] {
            j.ended(pid, WaitStatus::exited(0));
        }
        j.report();
        assert!(j.is_empty());
        assert_eq!(j.add(15, &[15], "true"), 1, "an empty table starts again");
    }

    #[test]
    fn the_lines_are_bash_s() {
        // What bash 5.2 printed for the same jobs, in a terminal.
        let mut j = Jobs::new();
        j.add(20, &[20], "sleep 0.5");
        j.add(21, &[21, 22], "sleep 30 | cat");
        j.add(23, &[23], "true");
        j.ended(23, WaitStatus::exited(0));
        assert_eq!(
            j.list(),
            [
                "[1]   Running                 sleep 0.5 &\n",
                "[2]-  Running                 sleep 30 | cat &\n",
                "[3]+  Done                    true\n",
            ]
        );
        assert_eq!(j.list().len(), 2, "a finished job is listed once");
        j.ended(20, WaitStatus::exited(0));
        assert_eq!(j.report(), ["[1]-  Done                    sleep 0.5\n"]);
        j.add(24, &[24], "false");
        j.ended(24, WaitStatus::exited(1));
        assert_eq!(j.report(), ["[3]+  Exit 1                  false\n"]);
        j.ended(21, killed());
        assert_eq!(j.report(), [] as [String; 0], "cat still runs");
        j.ended(22, killed());
        assert_eq!(
            j.report(),
            ["[2]+  Killed                  sleep 30 | cat\n"]
        );
        assert!(j.report().is_empty());
    }

    #[test]
    fn a_pipeline_s_status_is_its_last_process_s() {
        let mut j = Jobs::new();
        let n = j.add(30, &[30, 31], "false | true");
        j.ended(30, WaitStatus::exited(1));
        assert_eq!(j.status(n), None, "not ended yet");
        assert_eq!(j.running(n), [31]);
        j.ended(31, WaitStatus::exited(0));
        assert_eq!(j.status(n), Some(0));
        assert_eq!(
            j.take(n).unwrap(),
            "[1]+  Done                    false | true\n"
        );
        assert!(!j.has(n));
        let n = j.add(32, &[32, 33], "true | false");
        j.ended(33, WaitStatus::exited(1));
        j.ended(32, WaitStatus::exited(0));
        assert_eq!(j.status(n), Some(1));
        assert_eq!(j.report(), ["[1]+  Exit 1                  true | false\n"]);
        // A fault's or a kill's status is bash's for its signal.
        let n = j.add(34, &[34], "t-fault null-read");
        j.ended(34, WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1000));
        assert_eq!(j.status(n), Some(139));
        assert_eq!(
            j.take(n).unwrap(),
            "[1]+  Killed                  t-fault null-read\n"
        );
        let n = j.add(35, &[35], "t-spin");
        j.ended(35, killed());
        assert_eq!(j.status(n), Some(137));
    }

    #[test]
    fn a_job_is_found_by_its_processes() {
        let mut j = Jobs::new();
        let a = j.add(40, &[40, 41], "a | b");
        let b = j.add(42, &[42], "c");
        assert_eq!(
            (j.of_pid(41), j.of_pid(42), j.of_pid(7)),
            (Some(a), Some(b), None)
        );
        assert_eq!(
            (j.pgid(a), j.pgid(b), j.pgid(9)),
            (Some(40), Some(42), None)
        );
        assert_eq!(j.all_running(), [40, 41, 42]);
        assert!(!j.ended(7, WaitStatus::exited(0)), "no job's");
        assert!(j.ended(41, WaitStatus::exited(0)));
        assert!(!j.ended(41, WaitStatus::exited(0)), "once");
        assert_eq!(j.all_running(), [40, 42]);
        assert_eq!(j.take(a), None, "40 still runs");
        j.ended(40, WaitStatus::exited(0));
        j.forget_finished();
        assert!(!j.has(a) && j.has(b));
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod io;
pub mod killed;
````

with:

````rust
mod io;
pub mod jobs;
pub mod killed;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `WaitStatus` in this scope ``; `` cannot find type `Jobs` in this scope ``.

- [ ] **Step 4: Implement `crates/shell/src/jobs.rs`**

Insert this at the top of `crates/shell/src/jobs.rs`, above `#[cfg(test)]`:

````rust
//! The shell's background jobs (user-space gate §9.2, §16 item 9): what
//! `cmd &` started, numbered and reported as bash does.
//!
//! - A new job is numbered one past the highest in the table, so numbers
//!   start again from 1 only once the table is empty.
//! - The newest job is the current one, marked `+`, the one before it `-`.
//! - A job has ended when every process it started has; its status is its
//!   last one's: `Done`, `Exit 3` or `Killed`.
//! - A line is bash's: `[1]+  Done                    sleep 5`, the state
//!   padded to 24 columns, a running job's text followed by ` &`.

use crate::killed;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use relay_abi::wait::EXITED;

/// One job: the processes a background line started.
struct Job {
    number: u32,
    /// Its process group, which `kill %n` kills.
    pgid: u32,
    /// The line as typed, without the `&`.
    text: String,
    /// Its processes, in the order they started, and how each ended.
    procs: Vec<(u32, Option<WaitStatus>)>,
}

impl Job {
    fn finished(&self) -> bool {
        self.procs.iter().all(|(_, w)| w.is_some())
    }

    /// How its last process ended, once it has.
    fn last(&self) -> Option<WaitStatus> {
        self.procs.last().and_then(|(_, w)| *w)
    }
}

/// The background jobs, by number.
#[derive(Default)]
pub struct Jobs {
    jobs: Vec<Job>,
}

impl Jobs {
    pub fn new() -> Jobs {
        Jobs::default()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Adds the job of process group `pgid`, whose processes are `pids`
    /// (at least one; the last is the one `$!` and the status come from);
    /// its number.
    pub fn add(&mut self, pgid: u32, pids: &[u32], text: &str) -> u32 {
        let number = self.jobs.last().map_or(1, |j| j.number + 1);
        self.jobs.push(Job {
            number,
            pgid,
            text: String::from(text),
            procs: pids.iter().map(|&p| (p, None)).collect(),
        });
        number
    }

    /// Records how `pid` ended; whether it was a job's.
    pub fn ended(&mut self, pid: u32, status: WaitStatus) -> bool {
        for job in &mut self.jobs {
            if let Some(p) = job.procs.iter_mut().find(|(p, w)| *p == pid && w.is_none()) {
                p.1 = Some(status);
                return true;
            }
        }
        false
    }

    /// Whether job `number` is in the table.
    pub fn has(&self, number: u32) -> bool {
        self.find(number).is_some()
    }

    /// The job that has `pid` among its processes.
    pub fn of_pid(&self, pid: u32) -> Option<u32> {
        self.jobs
            .iter()
            .find(|j| j.procs.iter().any(|&(p, _)| p == pid))
            .map(|j| j.number)
    }

    /// Job `number`'s process group.
    pub fn pgid(&self, number: u32) -> Option<u32> {
        self.find(number).map(|j| j.pgid)
    }

    /// Job `number`'s processes that have not ended, in order.
    pub fn running(&self, number: u32) -> Vec<u32> {
        self.find(number).map_or(Vec::new(), |j| {
            j.procs
                .iter()
                .filter(|(_, w)| w.is_none())
                .map(|&(p, _)| p)
                .collect()
        })
    }

    /// Every job's processes that have not ended.
    pub fn all_running(&self) -> Vec<u32> {
        self.jobs
            .iter()
            .flat_map(|j| self.running(j.number))
            .collect()
    }

    /// The exit status job `number` ended with, as bash gives it (its last
    /// process's), once it has ended.
    pub fn status(&self, number: u32) -> Option<i32> {
        let job = self.find(number).filter(|j| j.finished())?;
        job.last().map(|w| status_of(&w))
    }

    /// The lines `jobs` prints, by number; the jobs that have ended are
    /// reported there, and leave the table.
    pub fn list(&mut self) -> Vec<String> {
        let lines = (0..self.jobs.len()).map(|i| self.line(i)).collect();
        self.jobs.retain(|j| !j.finished());
        lines
    }

    /// The lines for the jobs that have ended, which leave the table (what
    /// the shell says before a prompt).
    pub fn report(&mut self) -> Vec<String> {
        let lines = (0..self.jobs.len())
            .filter(|&i| self.jobs[i].finished())
            .map(|i| self.line(i))
            .collect();
        self.jobs.retain(|j| !j.finished());
        lines
    }

    /// Job `number`'s line if it has ended, as `report` gives it; it
    /// leaves the table (`wait %n` reports it at once).
    pub fn take(&mut self, number: u32) -> Option<String> {
        let i = self
            .jobs
            .iter()
            .position(|j| j.number == number && j.finished())?;
        let line = self.line(i);
        self.jobs.remove(i);
        Some(line)
    }

    /// The jobs that have ended leave the table without a word (`wait`
    /// without operands, as bash's).
    pub fn forget_finished(&mut self) {
        self.jobs.retain(|j| !j.finished());
    }

    fn find(&self, number: u32) -> Option<&Job> {
        self.jobs.iter().find(|j| j.number == number)
    }

    /// The line of the job at `i`: `+` for the newest, `-` for the one
    /// before.
    fn line(&self, i: usize) -> String {
        let job = &self.jobs[i];
        let mark = match self.jobs.len() - i {
            1 => '+',
            2 => '-',
            _ => ' ',
        };
        let (state, amp) = match job.last() {
            _ if !job.finished() => (String::from("Running"), " &"),
            Some(w) if w.how == EXITED && w.code == 0 => (String::from("Done"), ""),
            Some(w) if w.how == EXITED => (format!("Exit {}", w.code), ""),
            _ => (String::from("Killed"), ""),
        };
        format!("[{}]{mark}  {state:<24}{}{amp}\n", job.number, job.text)
    }
}

/// The status bash gives for a process that ended so.
pub fn status_of(w: &WaitStatus) -> i32 {
    if w.how == EXITED {
        w.code as i32
    } else {
        killed::killed(w).1
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 196 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add the table of background jobs

shell::jobs keeps what background lines started (spec §9.2) and says
it as bash does: a new job is numbered one past the highest in the
table, so numbers start again only once it is empty (bash 5.2 numbers
4 and 5 after jobs 1 and 2 ended, not 1 and 2); the newest is `+`, the
one before `-`; a job has ended when all its processes have, and its
status is its last one's. Its lines are bash's, the state padded to 24
columns, `[1]+  Done                    sleep 5`, rather than spec
§9.2's shorter form; `jobs` lists the finished ones once.
EOF
````


### Task 12: A job a fault ended says bash's words for it

The prototype's review (declined to judge, left to the user) and decision 7: a background job that a fault ended said `Killed`, as a kill does. The user chose bash's words: what bash says of the signal Linux would have sent, which the statuses already follow (`killed::killed`): `Segmentation fault` (139: a page fault, a protection fault, a stack overflow), `Illegal instruction` (132), `Floating point exception` (136), `Interrupt` (130), and `Killed` for `kill` (137); the kernel log keeps the fault's detail. bash pads the state to 24 columns and no further, so its `Floating point exception` meets the job's text (bash 5.2 printed `Floating point exception(core dumped)`); a blank stays between here (ruled). The red run is the shell's tests. Mutation checks: SIGFPE's and SIGINT's words missing, and bash's own padding each fail a test.

**Files:**
- Modify: `crates/shell/src/jobs.rs`

**Interfaces:**
- Consumes: Task 11's `Jobs`.
- Produces: nothing new (`jobs::signal_words`, private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            j.take(n).unwrap(),
            "[1]+  Killed                  t-fault null-read\n"
        );
````

with:

````rust
            j.take(n).unwrap(),
            "[1]+  Segmentation fault      t-fault null-read\n"
        );
````

Replace:

````rust
        assert!(!j.has(a) && j.has(b));
    }
}
````

with:

````rust
        assert!(!j.has(a) && j.has(b));
    }

    #[test]
    fn a_job_a_fault_ended_says_bash_s_words_for_its_signal() {
        use relay_abi::wait::{
            FAULT_DIVIDE, FAULT_FPU, FAULT_GENERAL_PROTECTION, FAULT_INVALID_OPCODE,
            FAULT_STACK_OVERFLOW,
        };
        // What bash 5.2 printed for jobs SIGSEGV, SIGILL and SIGFPE ended
        // (without its `(core dumped)`, which nothing here does).
        for (fault, words) in [
            (FAULT_PAGE, "Segmentation fault      "),
            (FAULT_GENERAL_PROTECTION, "Segmentation fault      "),
            (FAULT_STACK_OVERFLOW, "Segmentation fault      "),
            (FAULT_INVALID_OPCODE, "Illegal instruction     "),
            (FAULT_DIVIDE, "Floating point exception "),
            (FAULT_FPU, "Floating point exception "),
        ] {
            let mut j = Jobs::new();
            let n = j.add(50, &[50], "t-fault x");
            j.ended(50, WaitStatus::fault(fault, ACCESS_READ, 0, 0x40_1000));
            assert_eq!(
                j.take(n).unwrap(),
                alloc::format!("[1]+  {words}t-fault x\n"),
                "{fault}"
            );
        }
        let mut j = Jobs::new();
        let n = j.add(51, &[51], "t-spin");
        j.ended(51, WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C));
        assert_eq!(j.take(n).unwrap(), "[1]+  Interrupt               t-spin\n");
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `jobs::tests::a_job_a_fault_ended_says_bash_s_words_for_its_signal`, `jobs::tests::a_pipeline_s_status_is_its_last_process_s`.

- [ ] **Step 3: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! - A job has ended when every process it started has; its status is its
//!   last one's: `Done`, `Exit 3` or `Killed`.
//! - A line is bash's: `[1]+  Done                    sleep 5`, the state
````

with:

````rust
//! - A job has ended when every process it started has; its status is its
//!   last one's: `Done`, `Exit 3`, or the words bash has for the signal
//!   Linux would have sent (`Killed`, `Segmentation fault`, …).
//! - A line is bash's: `[1]+  Done                    sleep 5`, the state
````

Replace:

````rust
            Some(w) if w.how == EXITED => (format!("Exit {}", w.code), ""),
            _ => (String::from("Killed"), ""),
        };
        format!("[{}]{mark}  {state:<24}{}{amp}\n", job.number, job.text)
    }
````

with:

````rust
            Some(w) if w.how == EXITED => (format!("Exit {}", w.code), ""),
            Some(w) => (String::from(signal_words(status_of(&w))), ""),
            None => (String::from("Killed"), ""),
        };
        // bash pads to 24 and no further, so its `Floating point
        // exception` meets the text; here a blank stays between.
        format!("[{}]{mark}  {state:<23} {}{amp}\n", job.number, job.text)
    }
}

/// bash's words for a job its signal ended, by the status the signal
/// gives (`killed::killed`'s): SIGSEGV for a page fault or a protection
/// fault, SIGILL, SIGFPE, SIGINT, and SIGKILL for `kill` or anything else.
fn signal_words(status: i32) -> &'static str {
    match status {
        139 => "Segmentation fault",
        132 => "Illegal instruction",
        136 => "Floating point exception",
        130 => "Interrupt",
        _ => "Killed",
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 197 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): say how a job a fault ended in bash's words

A background job that a fault ended said Killed, as a kill does. It now
says what bash says of the signal Linux would have sent, which the
statuses already follow: `Segmentation fault` for a page fault, a
protection fault or a stack overflow (139), `Illegal instruction` (132),
`Floating point exception` (136), `Interrupt` (130), and `Killed` for
kill (137); the kernel log keeps the fault's detail. bash pads the state
to 24 columns and no further, so its `Floating point exception` meets
the job's text; a blank stays between here.
EOF
````


### Task 13: Background jobs start, and say how they ended

Spec §9.2 and decision 6: a line that ends with `&` starts a job. The spawning runner's `background` starts its commands as a pipeline's do (the starting is shared with `pipeline`, `Spawning::start`), the first that starts in a new group without the console (`Group::Background`, `NEW_GROUP` alone in `relay-rt`), at the prompt and in a script alike, the others joining it, and waits for none. At the prompt the shell says `[n] <pid of the last command>`; before each prompt it collects what ended (`Programs::collect`, `wait(-1, NOHANG)` in `relay-rt`, at most 64 at a time) and prints the lines of the jobs that have; a script and `X | sh` collect before each line and say neither, as bash's non-interactive shells do. A job none of whose commands started is no job; once one started the status is 0. `cd`, `exit` and `help` cannot run in the background (`relay-sh: cd: cannot be used in the background`); the in-process runner keeps refusing `&`. `relay-rt`'s `SysPrograms` maps a group to `spawn`'s flags in `spawn_group`, joining a pipeline's later stages to the last child it started in a group of its own (so a shell that leads no group keeps a foreground pipeline in its own, and a background job's stages join its first). `FakePrograms` gains children that run on through rounds of `collect` (a round ends when one finds nothing: a prompt), `children` and groups. The red runs are the shell's tests, which cannot compile against the new `Group` and fake, and `relay-rt`'s, which cannot find `spawn_group`. Mutation checks: `[n] pid` in a script, no collecting before a script's lines or `X | sh`'s (the second survived at first: its test had no later line), no report before a prompt, the first command given a group with the console, the status 0 dropped (it survived at first: its test's failing command was the first, which sets no status), built-ins taken in the background, a background job given `FOREGROUND` and a join of a group no child of the shell's leads each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 9's `parse_line`; Task 11's `Jobs`; plan 1's `Runner::pipeline`, `Group::Join`.
- Produces: `Group::Background`; `Programs::collect(&mut self) -> Option<(u32, WaitStatus)>`; `Runner::background(&mut self, parts, stages) -> Started { pgid, pids, ran }`; `relay_rt::sysio::spawn_group(group, own_group, leader) -> (u32, u32)`; the fake's `FakePrograms::{lives, children()}`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
    #[test]
    fn the_words_are_the_arguments_after_the_name() {
````

with:

````rust
    #[test]
    fn a_command_s_group_is_the_shell_s_a_new_one_or_its_first_stage_s() {
        // At the prompt of a shell leading its group: a command gets the
        // console; a pipeline's later stages join its first.
        assert_eq!(
            spawn_group(Group::New, true, None),
            (NEW_GROUP | FOREGROUND, 0)
        );
        assert_eq!(spawn_group(Group::Join(7), true, Some(7)), (0, 7));
        // A background job never gets the console, from any shell.
        for own in [true, false] {
            assert_eq!(spawn_group(Group::Background, own, None), (NEW_GROUP, 0));
            assert_eq!(
                spawn_group(Group::Join(9), own, Some(9)),
                (0, 9),
                "its later stages"
            );
        }
        // A shell in a script's group keeps its commands there, a
        // pipeline's later stages too (their first did not lead a group).
        assert_eq!(spawn_group(Group::New, false, None), (0, 0));
        assert_eq!(spawn_group(Group::Join(12), false, Some(9)), (0, 0));
        assert_eq!(spawn_group(Group::Shell, true, Some(12)), (0, 0));
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    }

    #[test]
    fn bin_sh_runs_a_script_s_commands_in_its_own_group_with_its_transcript_a_tee() {
````

with:

````rust
    }

    /// What an interactive `/bin/sh` prints for the lines typed, each
    /// ending with Enter, until the input ends.
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs).run();
        h.console.take()
    }

    /// `sleep` and `t-spin` run on through `collect`'s first round (the
    /// prompt after they start).
    fn with_jobs() -> Harness {
        let mut h = spawning();
        for p in ["/bin/sleep", "/bin/t-spin", "/bin/cat"] {
            h.programs.known.push((p, WaitStatus::exited(0)));
            h.programs.lives.push((p, 1));
        }
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        h
    }

    #[test]
    fn a_background_job_says_its_number_and_how_it_ended_before_a_prompt() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 &", "", "false &", ""]);
        assert_eq!(
            out,
            "root@relay:/# sleep 5 &\n[1] 101\n\
             root@relay:/# \n\
             [1]+  Done                    sleep 5\n\
             root@relay:/# false &\n[1] 102\n\
             [1]+  Exit 1                  false\n\
             root@relay:/# \nroot@relay:/# "
        );
        assert_eq!(
            h.programs.spawned[0].group,
            crate::Group::Background,
            "without the console"
        );
        assert!(h.programs.children().is_empty(), "every one collected");
    }

    #[test]
    fn a_background_pipeline_is_one_job_in_one_group() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["cat f | t-args x > out &", ""]);
        assert!(
            out.starts_with("root@relay:/# cat f | t-args x > out &\n[1] 102\n"),
            "{out}"
        );
        assert!(
            out.contains("[1]+  Exit 3                  cat f | t-args x > out\n"),
            "{out}"
        );
        let groups: Vec<crate::Group> = h.programs.spawned.iter().map(|s| s.group).collect();
        assert_eq!(groups, [crate::Group::Background, crate::Group::Join(101)]);
        assert_eq!(h.programs.spawned[1].stdout, Some(4), "the redirection");
        let (r, w) = h.programs.pipes[0];
        assert!(
            [r, w, 4].iter().all(|fd| h.programs.closed.contains(fd)),
            "the shell's copies"
        );
    }

    #[test]
    fn a_background_job_that_cannot_start_is_no_job() {
        let mut h = with_jobs();
        assert_eq!(
            h.spawning("nosuch &"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        // One stage of two that starts is a job, the status 0, as bash's.
        let out = typed(&mut h, &["nosuch | sleep 1 &", ""]);
        assert!(
            out.starts_with(
                "root@relay:/# nosuch | sleep 1 &\nrelay-sh: nosuch: command not found\n[1] 101\n"
            ),
            "{out}"
        );
        assert_eq!(
            h.spawning("sleep 1 | nosuch &"),
            (0, "relay-sh: nosuch: command not found\n".into()),
            "even the last"
        );
        for line in ["cd / &", "exit &", "help | cat &"] {
            let name = line.split([' ', '&']).next().unwrap();
            assert_eq!(
                h.spawning(line),
                (
                    1,
                    alloc::format!("relay-sh: {name}: cannot be used in the background\n")
                ),
                "{line}"
            );
        }
        assert_eq!(h.programs.spawned.len(), 2, "nothing else started");
    }

    #[test]
    fn a_script_and_x_into_sh_say_nothing_of_their_jobs() {
        let mut h = with_jobs();
        h.put("/tmp/s.sh", b"sleep 5 &\nt-args\nt-args\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (3, "+ sleep 5 &\n+ t-args\n+ t-args\n".into())
        );
        assert_eq!(
            h.programs.spawned[0].group,
            crate::Group::Background,
            "not the script's group"
        );
        assert!(
            h.programs.children().is_empty(),
            "collected before the later lines"
        );
        let mut h = with_jobs();
        let mut input = crate::Bytes::new(b"sleep 5 &\nt-args\nt-args\n".to_vec());
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(h.console.take(), "");
        assert!(
            h.programs.children().is_empty(),
            "collected before the later lines"
        );
    }

    #[test]
    fn the_in_process_runner_runs_no_background_job() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo hi &"),
            (2, "relay-sh: unsupported syntax: &\n".into())
        );
    }

    #[test]
    fn bin_sh_runs_a_script_s_commands_in_its_own_group_with_its_transcript_a_tee() {
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    pub spawned: Vec<Spawned>,
    /// The children started and not yet waited for, by pid.
    children: Vec<(u32, WaitStatus)>,
    /// The tees pushed and not popped, by path.
````

with:

````rust
    pub spawned: Vec<Spawned>,
    /// The programs that run on through this many rounds of `collect` (a
    /// round ends when it finds nothing), by path; the others end at once.
    pub lives: Vec<(&'static str, u32)>,
    /// The children started and not yet collected.
    children: Vec<FakeChild>,
    /// The rounds of `collect` so far.
    round: u32,
    /// The tees pushed and not popped, by path.
````

Replace:

````rust
            spawned: Vec::new(),
            children: Vec::new(),
            tees: Vec::new(),
````

with:

````rust
            spawned: Vec::new(),
            lives: Vec::new(),
            children: Vec::new(),
            round: 0,
            tees: Vec::new(),
````

Replace:

````rust
            next_pid: 100,
        }
    }
}
````

with:

````rust
            next_pid: 100,
        }
    }
}

/// A child of `FakePrograms`: how it ends, its group, and the round of
/// `collect` from which it has ended.
struct FakeChild {
    pid: u32,
    status: WaitStatus,
    group: u32,
    ends_at: u32,
}

impl FakePrograms {
    /// The children not yet collected, by pid, with their groups.
    pub fn children(&self) -> Vec<(u32, u32)> {
        self.children.iter().map(|c| (c.pid, c.group)).collect()
    }
}
````

Replace:

````rust
        let path = String::from_utf8_lossy(path).into_owned();
        let Some(&(_, status)) = self.known.iter().find(|(p, _)| *p == path) else {
````

with:

````rust
        let path = String::from_utf8_lossy(path).into_owned();
        let life = self
            .lives
            .iter()
            .find(|(p, _)| *p == path)
            .map_or(0, |l| l.1);
        let Some(&(_, status)) = self.known.iter().find(|(p, _)| *p == path) else {
````

Replace:

````rust
        self.next_pid += 1;
        self.children.push((self.next_pid, status));
        Ok(self.next_pid)
    }
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let i = self
            .children
            .iter()
            .position(|c| c.0 == pid)
            .ok_or(Errno::ECHILD)?;
        Ok(self.children.remove(i).1)
    }
````

with:

````rust
        self.next_pid += 1;
        let group = match group {
            Group::Shell => 0,
            Group::New | Group::Background => self.next_pid,
            Group::Join(g) => g,
        };
        self.children.push(FakeChild {
            pid: self.next_pid,
            status,
            group,
            ends_at: self.round + life,
        });
        Ok(self.next_pid)
    }
    /// Waits as long as the child runs on.
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let i = self
            .children
            .iter()
            .position(|c| c.pid == pid)
            .ok_or(Errno::ECHILD)?;
        Ok(self.children.remove(i).status)
    }
    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
        match self.children.iter().position(|c| c.ends_at <= self.round) {
            Some(i) => {
                let c = self.children.remove(i);
                Some((c.pid, c.status))
            }
            None => {
                self.round += 1;
                None
            }
        }
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `collect` is not a member of trait `Programs` ``; `` no variant, associated function, or constant named `Background` found for enum `Group` in the current scope ``.

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find function `spawn_group` in this scope ``; `` no variant, associated function, or constant named `Background` found for enum `shell::Group` in the current scope ``.

- [ ] **Step 5: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    own_group: bool,
}

impl SysPrograms {
    pub fn new(own_group: bool) -> SysPrograms {
        SysPrograms { own_group }
    }
````

with:

````rust
    own_group: bool,
    /// The last child started in a group of its own, which a pipeline's
    /// later commands join.
    leader: Option<u32>,
}

impl SysPrograms {
    pub fn new(own_group: bool) -> SysPrograms {
        SysPrograms {
            own_group,
            leader: None,
        }
    }
````

Replace:

````rust
    OPEN_WRITE | OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE }
}
````

with:

````rust
    OPEN_WRITE | OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE }
}

/// `spawn`'s flags and group for a command in `group`, from a shell that
/// leads a group of its own (`own_group`) or not, whose last child in a
/// new group was `leader`: a new group with the console only from a
/// leading shell; a background job's without it, from any shell; a
/// pipeline's later stages in the group its first one started, or else
/// the shell's.
pub fn spawn_group(group: Group, own_group: bool, leader: Option<u32>) -> (u32, u32) {
    match group {
        Group::New if own_group => (NEW_GROUP | FOREGROUND, 0),
        // Without the console, which stays where it is (spec §9.2).
        Group::Background => (NEW_GROUP, 0),
        Group::Join(pgid) if leader == Some(pgid) => (0, pgid),
        _ => (0, 0),
    }
}
````

Replace:

````rust
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

with:

````rust
    ) -> Result<u32, Errno> {
        let (flags, pgid) = spawn_group(group, self.own_group, self.leader);
        if flags & NEW_GROUP == 0 && pgid == 0 {
            // A command in the shell's own group reads the console in line
            // mode too, which is also where Ctrl-C ends it (and the group):
            // an interactive shell that leads no group left it raw at its
            // prompt.
            let _ = sys::console_mode(MODE_LINE);
        }
        let fds = command_fds(stdin, stdout);
        let pid = sys::spawn(path, &arg_bytes(args), b"", &fds, flags, pgid)
            .map_err(Errno::from_number)?;
        if flags & NEW_GROUP != 0 {
            self.leader = Some(pid);
        }
        Ok(pid)
    }
````

Replace:

````rust
            Err(e) => Err(Errno::from_number(e)),
        }
    }

````

with:

````rust
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
        sys::wait(relay_abi::spawn::WAIT_ANY, true).ok().flatten()
    }

````

- [ ] **Step 6: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// The group of the pipeline's first command (its pid), which has
    /// the console already.
    Join(u32),
}
````

with:

````rust
    /// The group of the pipeline's first command (its pid), which has
    /// the console already, or, in a background job, does not.
    Join(u32),
    /// A new one, without the console: a background job's first command
    /// (§9.2), at the prompt and in a script alike.
    Background,
}
````

Replace:

````rust
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// Copies the console into the file at `path` from now on (a script's
````

with:

````rust
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// A child that has ended, collected, if one has: a background job's
    /// process (`wait(-1, NOHANG)`).
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Copies the console into the file at `path` from now on (a script's
````

- [ ] **Step 7: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::parser::{Command, Redirect};
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND};
use crate::transcript::Transcript;
````

with:

````rust
use crate::parser::{Command, Redirect};
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
use crate::transcript::Transcript;
````

Replace:

````rust
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran;
}
````

with:

````rust
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran;

    /// Starts a pipeline of one or more commands, none of them a built-in,
    /// in the background (spec §9.2), and does not wait for it.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started;
}

/// A background job's start.
pub(crate) struct Started {
    /// Its process group, if any of its commands started.
    pub pgid: Option<u32>,
    /// The processes that started, in order.
    pub pids: Vec<u32>,
    /// What could not start, and the status if nothing did.
    pub ran: Ran,
}
````

Replace:

````rust
        )
    }
}

````

with:

````rust
        )
    }

    fn background(&mut self, _: Parts<'_>, _: &[Command]) -> Started {
        InProcess::refuse_background()
    }
}

impl InProcess {
    /// A background job needs programs: the in-process runner keeps
    /// refusing `&`, as milestone 1's shell did.
    fn refuse_background() -> Started {
        Started {
            pgid: None,
            pids: Vec::new(),
            ran: Ran::said(SYNTAX, format!("{NAME}: unsupported syntax: &\n")),
        }
    }
}

````

Replace:

````rust
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
````

with:

````rust
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        let in_script = parts.in_script;
        let (started, mut ran) = self.start(parts, stages, in_script, false);
        let mut cancelled = false;
        for (name, pid) in &started.pids {
            let ended = match self.programs.wait(*pid) {
                Ok(w) => ended(name, &w),
                Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
            };
            if Some(*pid) == started.last {
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

    /// The stages start as a foreground pipeline's do, the first that
    /// starts in a new group without the console (at the prompt and in a
    /// script alike, so that Ctrl-C of the script does not reach it), the
    /// others joining it; nothing waits for them.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started {
        let (started, mut ran) = self.start(parts, stages, false, true);
        let pgid = started.pids.first().map(|&(_, pid)| pid);
        if pgid.is_some() {
            ran.status = 0;
        }
        Started {
            pgid,
            pids: started.pids.iter().map(|&(_, pid)| pid).collect(),
            ran,
        }
    }
}

/// The stages a pipeline started: their names and pids, and the last
/// stage's pid if it started.
struct Stages<'c> {
    pids: Vec<(&'c String, u32)>,
    last: Option<u32>,
}

impl Spawning<'_> {
    /// Starts `stages` left to right with pipes between them: in the
    /// shell's group (`in_script`), or the first that starts in a new one
    /// (with the console unless `background`) and the others joining it.
    /// What started, and what did not with its message (the status that
    /// of a last stage that could not start, or of a pipe that could not
    /// be made).
    fn start<'c>(
        &mut self,
        parts: Parts<'_>,
        stages: &'c [Command],
        in_script: bool,
        background: bool,
    ) -> (Stages<'c>, Ran) {
        let (last, _) = stages.split_last().expect("a pipeline has stages");
        let mut started = Stages {
            pids: Vec::new(),
            last: None,
        };
        let last_out = match &last.redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return (started, Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
            },
            None => None,
        };
        let (mut stdin, mut first) = (None, None);
        let mut ran = Ran::said(0, String::new());
````

Replace:

````rust
            argv.extend(stage.words[1..].iter().map(|w| w.as_bytes()));
            let group = match (parts.in_script, first) {
                (true, _) => Group::Shell,
                (false, None) => Group::New,
                (false, Some(pgid)) => Group::Join(pgid),
            };
````

with:

````rust
            argv.extend(stage.words[1..].iter().map(|w| w.as_bytes()));
            let group = match (in_script, background, first) {
                (_, true, None) => Group::Background,
                (true, false, _) => Group::Shell,
                (_, _, None) => Group::New,
                (_, _, Some(pgid)) => Group::Join(pgid),
            };
````

Replace:

````rust
                    first.get_or_insert(pid);
                    started.push((name, pid));
                    if is_last {
                        last_pid = Some(pid);
                    }
````

with:

````rust
                    first.get_or_insert(pid);
                    started.pids.push((name, pid));
                    if is_last {
                        started.last = Some(pid);
                    }
````

Replace:

````rust
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

with:

````rust
        }
        (started, ran)
    }
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::parser::{self, HOME};
````

with:

````rust
use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::jobs::Jobs;
use crate::parser::{self, HOME};
````

Replace:

````rust
    input: Option<&'a mut dyn Stdin>,
}
````

with:

````rust
    input: Option<&'a mut dyn Stdin>,
    /// The background jobs (spec §9.2).
    jobs: Jobs,
    /// It reads commands at its prompt (`run`): it says a job's number
    /// when it starts one, and how jobs ended before each prompt. A script
    /// and `X | sh` say neither, as bash's do.
    prompting: bool,
}
````

Replace:

````rust
            input: None,
        }
````

with:

````rust
            input: None,
            jobs: Jobs::new(),
            prompting: false,
        }
````

Replace:

````rust
        self.stopped = false;
        while !self.stopped {
            let mut out = Vec::new();
````

with:

````rust
        self.stopped = false;
        self.prompting = true;
        while !self.stopped {
            self.collect_jobs();
            for line in self.jobs.report() {
                self.say(line.as_bytes());
            }
            let mut out = Vec::new();
````

Replace:

````rust
            Ok(parser::Line {
                background: Some(_),
                ..
            }) => {
                return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
            }
            Ok(line) => line.pipeline,
````

with:

````rust
            Ok(parser::Line {
                pipeline,
                background: Some(text),
            }) => return self.background(&pipeline, &text),
            Ok(line) => line.pipeline,
````

Replace:

````rust
        self.finish(ran.status, ran.message)
    }
````

with:

````rust
        self.finish(ran.status, ran.message)
    }

    /// Starts `stages` as a background job (user-space gate §9.2), whose
    /// text is `text`: at the prompt the shell says `[<number>] <pid of its
    /// last process>`. Its status is 0 once anything of it started. The
    /// shell's own commands cannot be in one.
    fn background(&mut self, stages: &[parser::Command], text: &str) -> i32 {
        let builtin = stages
            .iter()
            .find(|c| commands::builtin(&c.words[0]).is_some());
        if let Some(c) = builtin {
            let name = &c.words[0];
            let message = format!("{NAME}: {name}: cannot be used in the background\n");
            return self.finish(1, message);
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
            input: None,
        };
        let started = self.runner.get().background(parts, stages);
        if let Some(pgid) = started.pgid {
            let number = self.jobs.add(pgid, &started.pids, text);
            if self.prompting && !self.in_script {
                let last = started.pids.last().copied().unwrap_or(pgid);
                self.say(format!("[{number}] {last}\n").as_bytes());
            }
        }
        self.finish(started.ran.status, started.ran.message)
    }

    /// Collects the background jobs' processes that have ended, at most
    /// as many as can exist (`relay_abi::proc::PROC_MAX`).
    fn collect_jobs(&mut self) {
        if self.jobs.is_empty() {
            return;
        }
        let Some(programs) = self.runner.programs() else {
            return;
        };
        for _ in 0..relay_abi::proc::PROC_MAX {
            let Some((pid, status)) = programs.collect() else {
                break;
            };
            self.jobs.ended(pid, status);
        }
    }
````

Replace:

````rust
    fn input_line(&mut self, line: &[u8], too_long: bool) {
        match core::str::from_utf8(line) {
````

with:

````rust
    fn input_line(&mut self, line: &[u8], too_long: bool) {
        self.collect_jobs();
        match core::str::from_utf8(line) {
````

Replace:

````rust
            }
            // The line runs as written; only its trace is trimmed.
````

with:

````rust
            }
            self.collect_jobs();
            // The line runs as written; only its trace is trimmed.
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 202 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 10: Run the `sh`, `pipes`, `shell` scenarios**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): start background jobs and say how they ended

A line that ends with & starts a job (spec §9.2): its commands start
as a pipeline's do, the first in a new group without the console
(Group::Background, NEW_GROUP alone in relay-rt), the others joining
it, and nothing waits for them. At the prompt the shell says
`[n] <pid of the last command>`, and before each prompt it collects
what ended (Programs::collect, wait(-1, NOHANG)) and prints the Done,
Exit and Killed lines of the jobs that have. A script and `X | sh`
collect before each line and say neither, as bash's do; a job's
commands are in a group of their own there too, so Ctrl-C of the
script does not reach them. A job none of whose commands started is
no job; cd, exit and help cannot run in the background; the in-process
runner keeps refusing &.
EOF
````


### Task 14: Jobs that ended are collected before a typed line runs

The prototype's review (minor, M8) and decision 6: the interactive shell collected its jobs only before a prompt, so a job that ended while the next line was typed kept its slot in the process table while that line ran: with the table full, killing two jobs left `ps | wc -l` with no room to start (`Resource temporarily unavailable`). It now collects before running each line too, as scripts and `X | sh` do; how the jobs ended is still said at the next prompt, as bash says it. `FakePrograms` records how many children are alive at each spawn. The red run is the shell's tests. Mutation check: no collecting before a typed line fails the test (`t-args` started with `sleep`'s zombie still in the table).

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 13's `collect_jobs`.
- Produces: the fake's `FakePrograms::alive_at_spawn`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_background_pipeline_is_one_job_in_one_group() {
````

with:

````rust
    #[test]
    fn a_job_that_ended_while_a_line_was_typed_is_collected_before_it_runs() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["sleep 5 &", "t-args"]);
        assert_eq!(
            h.programs.alive_at_spawn,
            [0, 0],
            "sleep's slot was free when t-args started"
        );
        // Reported at the next prompt, as bash's.
        assert!(
            out.ends_with("# t-args\n[1]+  Done                    sleep 5\nroot@relay:/# "),
            "{out}"
        );
    }

    #[test]
    fn a_background_pipeline_is_one_job_in_one_group() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    round: u32,
    /// The tees pushed and not popped, by path.
````

with:

````rust
    round: u32,
    /// How many children were not yet collected at each `spawn`.
    pub alive_at_spawn: Vec<usize>,
    /// The tees pushed and not popped, by path.
````

Replace:

````rust
            round: 0,
            tees: Vec::new(),
````

with:

````rust
            round: 0,
            alive_at_spawn: Vec::new(),
            tees: Vec::new(),
````

Replace:

````rust
        let path = String::from_utf8_lossy(path).into_owned();
        let life = self
````

with:

````rust
        let path = String::from_utf8_lossy(path).into_owned();
        self.alive_at_spawn.push(self.children.len());
        let life = self
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_job_that_ended_while_a_line_was_typed_is_collected_before_it_runs`.

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
                    Feed::Line(line) => {
                        self.execute(&line);
````

with:

````rust
                    Feed::Line(line) => {
                        // What ended while it was typed frees its slot in
                        // the process table before the line runs; it is
                        // reported at the next prompt.
                        self.collect_jobs();
                        self.execute(&line);
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 203 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): collect ended jobs before a typed line runs

The interactive shell collected its jobs only before a prompt, so a job
that ended while the next line was typed kept its slot in the process
table while that line ran: with the table full, killing two jobs left
`ps | wc -l` with no room to start (Resource temporarily unavailable).
It now collects before running each line too, as scripts and `X | sh`
do; how the jobs ended is still said at the next prompt, as bash says
it.

Refs: review M8
EOF
````


### Task 15: An interactive shell in the background ends before its first prompt

The prototype's review (minor, M6) and decision 6: `sh &` started an interactive shell in a group that was never given the console: it printed a prompt among the screen's (`root@relay:~# root@relay:~# `), then read end of input and ended. Its leader test, `console_foreground` of its own group, says `EPERM` exactly then (Task 3), and `/bin/sh` now ends at once with 0, before any prompt; its job is reported Done. bash stops such a shell instead, which needs job control this gate leaves out. `/bin/sh` gains `relay-abi` for the error's number. `sh` runs `sh &` and expects no second prompt. The red run is `sh`, which sees the stray prompt. Mutation check: the `EPERM` case dropped fails `sh`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `tests/e2e/sh.txt`
- Modify: `userland/sh/Cargo.toml`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: Task 3's refusal.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, replace:

````text
alive 1
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log
````

with:

````text
alive 1
# An interactive shell in the background has no console to read: it ends
# before its first prompt, and its job is done (milestone 3).
send sh &
expect \n\[1\] \d+\nroot@relay:\S+# $
alive 1
send echo next
expect \Aecho next\nnext\n\[1\]\+  Done                    sh\n
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log
````

- [ ] **Step 2: Change `userland/sh/Cargo.toml`**

In `userland/sh/Cargo.toml`, replace:

````toml
[dependencies]
relay-rt.workspace = true
````

with:

````toml
[dependencies]
relay-abi.workspace = true
relay-rt.workspace = true
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: FAIL: scenario `sh` stops at line 110, timed out waiting for `\n\[1\] \d+\nroot@relay:\S+# $`.

- [ ] **Step 4: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, replace:

````rust
        // was started at a prompt (the group is numbered after it); one a
        // script started is in the script's group.
        let leader = sys::console_foreground(sys::getpid()).is_ok();
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
````

with:

````rust
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

- [ ] **Step 5: Run the `sh` scenario**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add Cargo.lock tests userland
git commit -F - <<'EOF'
fix(sh): end a background interactive shell before its first prompt

`sh &` started an interactive shell in a group that was never given the
console: it printed a prompt among the screen's, then read end of input
and ended. Its leader test, console_foreground of its own group, now
says EPERM exactly then, and /bin/sh ends at once with 0, before any
prompt; its job is reported Done. bash stops such a shell instead,
which needs job control this gate leaves out (spec §15).

Refs: review M6
EOF
````


### Task 16: The `jobs` built-in

Spec §8.3, §9.2 and decision 8: `jobs` lists the background jobs, a job that has ended saying how there once and leaving the table; `jobs %n` or `jobs n` names jobs (`relay-sh: jobs: %3: no such job`, status 1; a job not named that has ended stays to be reported at the prompt), and an option is refused (status 2), as bash's. A built-in's `Ctx` gains the shell's job control (`JobControl`: its jobs and, under `/bin/sh`, its programs), which only the shell's own commands get, and `Jobs::collect` collects through `Programs`; `jobs` joins `cd`, `exit` and `help` as a built-in, and `help` lists it. The red run is the shell's tests, which cannot find the command. Mutation checks: no collecting first, the names ignored and status 0 for an unknown job each fail a test; `jobs %1` dropping a job it did not name, unreported, survived at first and now fails the test that types a prompt after it (a check that no job is 0 survived too: no job is, and it is gone).

**Files:**
- Create: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/jobs.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 13's jobs and `Programs::collect`.
- Produces: `commands::jobs`; `Ctx::control: Option<JobControl>`, `JobControl { jobs, programs, .. }` with `collect`; `Jobs::collect(&mut self, programs: &mut dyn Programs)`, `Jobs::list(&mut self, named: Option<&[u32]>)`; `jobs::number(&str) -> Option<u32>`; `runner::run_function(…, control: Option<JobControl>)`.

- [ ] **Step 1: Write the failing tests for `crates/shell/src/commands/control.rs`**

Create `crates/shell/src/commands/control.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::Harness;
    use alloc::string::String;
    use relay_abi::WaitStatus;

    /// What an interactive `/bin/sh` prints for the lines typed.
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs).run();
        h.console.take()
    }

    /// `t-spin` runs on through 99 rounds of collecting (a round each
    /// prompt, typed line and `jobs`), `sleep` through 1.
    fn with_jobs() -> Harness {
        let mut h = Harness::new();
        for (p, life) in [("/bin/t-spin", 99), ("/bin/sleep", 1)] {
            h.programs.known.push((p, WaitStatus::exited(0)));
            h.programs.lives.push((p, life));
        }
        h
    }

    #[test]
    fn jobs_lists_them_and_says_once_how_the_ended_ones_ended() {
        let mut h = with_jobs();
        // sleep ends after the prompt before `jobs`, which reports it.
        let out = typed(
            &mut h,
            &["t-spin &", "sleep 5 &", "jobs", "jobs", "jobs > /tmp/j"],
        );
        assert_eq!(
            out,
            "root@relay:/# t-spin &\n[1] 101\n\
             root@relay:/# sleep 5 &\n[2] 102\n\
             root@relay:/# jobs\n\
             [1]-  Running                 t-spin &\n\
             [2]+  Done                    sleep 5\n\
             root@relay:/# jobs\n\
             [1]+  Running                 t-spin &\n\
             root@relay:/# jobs > /tmp/j\n\
             root@relay:/# "
        );
        assert_eq!(h.get("/tmp/j"), b"[1]+  Running                 t-spin &\n");
    }

    #[test]
    fn a_job_jobs_does_not_name_is_reported_at_the_prompt() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["t-spin &", "sleep 5 &", "jobs %1", ""]);
        assert!(
            out.ends_with(
                "# jobs %1\n[1]-  Running                 t-spin &\n\
                 [2]+  Done                    sleep 5\nroot@relay:/# \nroot@relay:/# "
            ),
            "{out}"
        );
    }

    #[test]
    fn jobs_names_jobs_as_bash_s_does() {
        let mut h = with_jobs();
        let out = typed(
            &mut h,
            &[
                "t-spin &",
                "t-spin 2 &",
                "jobs %1 2",
                "jobs 2",
                "jobs %3 x %0",
                "jobs -l",
            ],
        );
        assert!(
            out.contains(
                "# jobs %1 2\n[1]-  Running                 t-spin &\n[2]+  Running                 t-spin 2 &\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("# jobs 2\n[2]+  Running                 t-spin 2 &\nroot"),
            "{out}"
        );
        assert!(
            out.contains(
                "# jobs %3 x %0\nrelay-sh: jobs: %3: no such job\nrelay-sh: jobs: x: no such job\nrelay-sh: jobs: %0: no such job\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("# jobs -l\nrelay-sh: jobs: -l: invalid option\n"),
            "{out}"
        );
        assert_eq!(
            h.spawning("jobs %3"),
            (1, "relay-sh: jobs: %3: no such job\n".into())
        );
        assert_eq!(
            h.spawning("jobs -l"),
            (2, "relay-sh: jobs: -l: invalid option\n".into())
        );
        // A shell without programs has no jobs.
        assert_eq!(h.run("jobs"), (0, String::new()));
    }
}
````

- [ ] **Step 2: Add the failing tests and the module declaration to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod change;
mod grep;
````

with:

````rust
mod change;
mod control;
mod grep;
````

Replace:

````rust
    #[test]
    fn the_shell_s_own_commands_are_cd_exit_and_help() {
        let own: alloc::vec::Vec<_> = COMMANDS
````

with:

````rust
    #[test]
    fn the_shell_s_own_commands_are_cd_exit_help_and_the_job_commands() {
        let own: alloc::vec::Vec<_> = COMMANDS
````

Replace:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

with:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(
            j.list(),
            [
````

with:

````rust
        assert_eq!(
            j.list(None),
            [
````

Replace:

````rust
        );
        assert_eq!(j.list().len(), 2, "a finished job is listed once");
        j.ended(20, WaitStatus::exited(0));
````

with:

````rust
        );
        assert_eq!(j.list(None).len(), 2, "a finished job is listed once");
        j.ended(20, WaitStatus::exited(0));
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `this method takes 0 arguments but 1 argument was supplied`.

- [ ] **Step 5: Implement `crates/shell/src/commands/control.rs`**

Insert this at the top of `crates/shell/src/commands/control.rs`, above `#[cfg(test)]`:

````rust
//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, and
//! later `wait` and `kill`. They work on the shell's jobs, which only the
//! shell has (`Ctx::control`).

use crate::ctx::Ctx;
use crate::jobs::number;
use crate::shell::NAME;
use alloc::string::String;
use alloc::vec::Vec;

/// `jobs [%n | n]...`: the background jobs, by number, or those named; the
/// ones that have ended say how, once, and leave the table.
pub fn jobs(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Some(option) = args.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        ctx.fail(NAME, format_args!("jobs: {option}: invalid option"));
        return 2;
    }
    let Some(control) = ctx.control.as_mut() else {
        return 0;
    };
    control.collect();
    let (named, unknown): (Vec<_>, Vec<_>) = args
        .iter()
        .map(|a| (a, number(a.strip_prefix('%').unwrap_or(a))))
        .partition(|(_, n)| n.is_some_and(|n| control.jobs.has(n)));
    let named: Vec<u32> = named.into_iter().filter_map(|(_, n)| n).collect();
    let lines = if args.is_empty() {
        control.jobs.list(None)
    } else {
        control.jobs.list(Some(&named))
    };
    let status = i32::from(!unknown.is_empty());
    for (a, _) in unknown {
        ctx.fail(NAME, format_args!("jobs: {a}: no such job"));
    }
    for line in lines {
        ctx.out(line.as_bytes());
    }
    status
}

````

- [ ] **Step 6: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use grep::grep;
````

with:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use control::jobs;
pub use grep::grep;
````

Replace:

````rust
        run: basic::help,
    },
````

with:

````rust
        run: basic::help,
    },
    Builtin {
        name: "jobs",
        help: "list the background jobs",
        run: control::jobs,
    },
````

Replace:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help"];

````

with:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs"];

````

- [ ] **Step 7: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

use crate::io::{Console, Stdin, Stdout, System};
use crate::transcript::Transcript;
````

with:

````rust

use crate::io::{Console, Programs, Stdin, Stdout, System};
use crate::jobs::Jobs;
use crate::transcript::Transcript;
````

Replace:

````rust
    pub(crate) exited: bool,
}
````

with:

````rust
    pub(crate) exited: bool,
    /// The shell's jobs, for its own commands `jobs`, `wait` and `kill`.
    pub(crate) control: Option<JobControl<'a>>,
}

/// What the shell's job commands work with: its jobs, and its programs
/// (none in the in-process runner, which starts no job).
pub(crate) struct JobControl<'a> {
    pub jobs: &'a mut Jobs,
    pub programs: Option<&'a mut dyn Programs>,
}

impl JobControl<'_> {
    /// Collects what has ended of the jobs.
    pub fn collect(&mut self) {
        if let Some(programs) = self.programs.as_deref_mut() {
            self.jobs.collect(programs);
        }
    }
}
````

Replace:

````rust
            exited: false,
        }
````

with:

````rust
            exited: false,
            control: None,
        }
````

- [ ] **Step 8: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use crate::killed;
````

with:

````rust

use crate::io::Programs;
use crate::killed;
````

Replace:

````rust

    /// Records how `pid` ended; whether it was a job's.
````

with:

````rust

    /// Collects what has ended of the jobs' processes from `programs`, at
    /// most as many as can exist (`relay_abi::proc::PROC_MAX`).
    pub fn collect(&mut self, programs: &mut dyn Programs) {
        if self.jobs.is_empty() {
            return;
        }
        for _ in 0..relay_abi::proc::PROC_MAX {
            let Some((pid, status)) = programs.collect() else {
                break;
            };
            self.ended(pid, status);
        }
    }

    /// Records how `pid` ended; whether it was a job's.
````

Replace:

````rust

    /// The lines `jobs` prints, by number; the jobs that have ended are
    /// reported there, and leave the table.
    pub fn list(&mut self) -> Vec<String> {
        let lines = (0..self.jobs.len()).map(|i| self.line(i)).collect();
        self.jobs.retain(|j| !j.finished());
        lines
````

with:

````rust

    /// The lines `jobs` prints, by number, of every job or of those named;
    /// the jobs listed that have ended are reported there, and leave the
    /// table.
    pub fn list(&mut self, named: Option<&[u32]>) -> Vec<String> {
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        let lines = (0..self.jobs.len())
            .filter(|&i| listed(&self.jobs[i]))
            .map(|i| self.line(i))
            .collect();
        self.jobs.retain(|j| !(listed(j) && j.finished()));
        lines
````

Replace:

````rust
        _ => "Killed",
    }
}

````

with:

````rust
        _ => "Killed",
    }
}

/// The job number `n` names (`1`, `27`), as `%n` does: decimal digits only
/// (no job is 0).
pub fn number(n: &str) -> Option<u32> {
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

````

- [ ] **Step 9: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::commands::{self, Builtin, Script};
use crate::ctx::{Ctx, JobControl};
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
````

Replace:

````rust
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, file),
            None => not_found(name),
````

with:

````rust
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, file, None),
            None => not_found(name),
````

Replace:

````rust
/// any.
pub(crate) fn run_function(
    parts: Parts<'_>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    if let Some(input) = parts.input {
````

with:

````rust
/// any.
pub(crate) fn run_function<'s>(
    parts: Parts<'s>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
    control: Option<JobControl<'s>>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.control = control;
    if let Some(input) = parts.input {
````

- [ ] **Step 10: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::commands::{self, SCRIPT_MAX, Script};
use crate::ctx::{Ctx, quote_if_needed};
use crate::editor::{Feed, LineEditor};
````

with:

````rust
use crate::commands::{self, SCRIPT_MAX, Script};
use crate::ctx::{Ctx, JobControl, quote_if_needed};
use crate::editor::{Feed, LineEditor};
````

Replace:

````rust
                Some(builtin) => {
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file),
                        Err(ran) => ran,
````

with:

````rust
                Some(builtin) => {
                    let control = JobControl {
                        jobs: &mut self.jobs,
                        programs: self.runner.programs(),
                    };
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file, Some(control)),
                        Err(ran) => ran,
````

Replace:

````rust

    /// Collects the background jobs' processes that have ended, at most
    /// as many as can exist (`relay_abi::proc::PROC_MAX`).
    fn collect_jobs(&mut self) {
        if self.jobs.is_empty() {
            return;
        }
        let Some(programs) = self.runner.programs() else {
            return;
        };
        for _ in 0..relay_abi::proc::PROC_MAX {
            let Some((pid, status)) = programs.collect() else {
                break;
            };
            self.jobs.ended(pid, status);
        }
````

with:

````rust

    /// Collects the background jobs' processes that have ended.
    fn collect_jobs(&mut self) {
        if let Some(programs) = self.runner.programs() {
            self.jobs.collect(programs);
        }
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 206 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add the jobs built-in

`jobs` (spec §9.2) lists the background jobs as bash's does, each
running one as `[1]+  Running                 sleep 5 &`; a job that has
ended says how, once, and leaves the table. `jobs %n` or `jobs n` names
jobs (`relay-sh: jobs: %3: no such job`, status 1); an option is
refused, status 2. A built-in's Ctx gains the shell's job control (its
jobs and, under /bin/sh, its programs), which only the shell's own
commands get; the in-process runner has no jobs. help lists it.
EOF
````


### Task 17: `jobs` lists the jobs it names in the order named

The prototype's review (minor, M7) and decision 8: `jobs 2 %1` listed job 1 before job 2, and `jobs %1 %1` listed it once: the jobs named came in the table's order. bash 5.2 lists them in the order named, one named twice twice, and so does `jobs` now; without operands it still lists every job by number. Task 16's tests give `t-spin` a longer life in `FakePrograms`, since Task 14's collecting before each line adds rounds. The red run is the shell's tests. Mutation check: the named jobs not found fails the test.

**Files:**
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/jobs.rs`

**Interfaces:**
- Consumes: Task 16's `Jobs::list`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, replace:

````rust
            "{out}"
        );
        assert!(
            out.contains(
````

with:

````rust
            "{out}"
        );
        // In the order named, a job named twice listed twice (bash 5.2).
        let ordered = typed(
            &mut h,
            &["t-spin &", "t-spin 2 &", "jobs 2 %1", "jobs %1 %1"],
        );
        assert!(
            ordered.contains(
                "# jobs 2 %1\n[2]+  Running                 t-spin 2 &\n[1]-  Running                 t-spin &\n"
            ),
            "{ordered}"
        );
        assert!(
            ordered.contains(
                "# jobs %1 %1\n[1]-  Running                 t-spin &\n[1]-  Running                 t-spin &\n"
            ),
            "{ordered}"
        );
        assert!(
            out.contains(
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::control::tests::jobs_names_jobs_as_bash_s_does`.

- [ ] **Step 3: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, replace:

````rust

    /// The lines `jobs` prints, by number, of every job or of those named;
    /// the jobs listed that have ended are reported there, and leave the
    /// table.
    pub fn list(&mut self, named: Option<&[u32]>) -> Vec<String> {
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        let lines = (0..self.jobs.len())
            .filter(|&i| listed(&self.jobs[i]))
            .map(|i| self.line(i))
            .collect();
        self.jobs.retain(|j| !(listed(j) && j.finished()));
````

with:

````rust

    /// The lines `jobs` prints: of every job by number, or of those named
    /// in the order named, a job named twice listed twice, as bash's;
    /// the jobs listed that have ended are reported there, and leave the
    /// table.
    pub fn list(&mut self, named: Option<&[u32]>) -> Vec<String> {
        let order: Vec<usize> = match named {
            None => (0..self.jobs.len()).collect(),
            Some(n) => n
                .iter()
                .filter_map(|&k| self.jobs.iter().position(|j| j.number == k))
                .collect(),
        };
        let lines = order.iter().map(|&i| self.line(i)).collect();
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        self.jobs.retain(|j| !(listed(j) && j.finished()));
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 206 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): list the jobs jobs names in the order it names them

`jobs 2 %1` listed job 1 before job 2, and `jobs %1 %1` listed it once:
the jobs named came in the table's order. bash 5.2 lists them in the
order named, one named twice twice, and so does jobs now; without
operands it still lists every job by number.

Refs: review M7
EOF
````


### Task 18: The `wait` built-in

Spec §9.2 and decision 8: `wait` waits for every background job, which then leave the table without a word, as bash's do; `wait %n` or `wait PID` waits for one, its status that job's last process's or that process's own (kept in the table if it ended before), and at the prompt a job that ends there says how at once (not in a script). What is no job of the shell's gets bash's message and 127 (`wait: %3: no such job`, `wait: pid 9 is not a child of this shell`), what is no pid ``wait: `abc': not a pid or valid job spec`` (1). A Ctrl-C at the prompt ends the wait (`^C`, 130), the jobs running on: `Programs::wait_or_ctrl_c` waits with Task 4's `WAIT_CTRL_C`. Until plan 3's `$?` the status is the shell's last status. The red run is the shell's tests, which cannot find the command. Mutation checks: the Ctrl-C not ending the wait, finished jobs left after `wait`, a pid's status replaced by its job's (a process that had ended at the prompt gave 0 before: found while writing the test), 1 instead of 127 and the `^C` not said each fail a test; the Done line in a script survived at first and now fails a script's test (collecting before waiting survived too: a wait for a process that has ended collects it, so it is gone).

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/jobs.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 16's `JobControl`; Task 4's `sys::wait_with`.
- Produces: `commands::wait`; `Programs::wait_or_ctrl_c(&mut self, pid) -> Result<WaitStatus, Errno>`; `JobControl::report`; `Jobs::status_of_pid(&self, pid) -> Option<i32>`; the fake's `FakePrograms::{ctrl_c_after, waited}`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs).run();
        h.console.take()
    }
````

with:

````rust
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        typed_status(h, lines).1
    }

    /// The last line's status, and what the shell printed.
    fn typed_status(h: &mut Harness, lines: &[&str]) -> (i32, String) {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.run();
        let status = shell.status();
        (status, h.console.take())
    }
````

Replace:

````rust
    }
}
````

with:

````rust
    }

    #[test]
    fn wait_waits_for_every_job_and_says_nothing_of_them() {
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["t-spin &", "t-spin 2 &", "wait", "jobs"]);
        assert!(
            out.ends_with("root@relay:/# wait\nroot@relay:/# jobs\nroot@relay:/# "),
            "{out}"
        );
        assert_eq!(status, 0);
        assert_eq!(h.programs.waited, [101, 102]);
        assert!(h.programs.children().is_empty());
    }

    #[test]
    fn wait_for_a_job_or_a_pid_says_how_it_ended_and_takes_its_status() {
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        h.programs.lives.push(("/bin/false", 9));
        let (status, out) = typed_status(
            &mut h,
            &[
                "false | t-spin &",
                "false &",
                "wait %2",
                "wait 101",
                "wait 102",
                "jobs",
            ],
        );
        assert!(
            out.contains("# wait %2\n[2]+  Exit 1                  false\nroot@relay:/# "),
            "{out}"
        );
        // A pid of a job: its own status; the job says how it ended once
        // its last process has.
        assert!(
            out.contains(
                "# wait 101\nroot@relay:/# wait 102\n[1]+  Done                    false | t-spin\n"
            ),
            "{out}"
        );
        assert!(out.ends_with("# jobs\nroot@relay:/# "), "{out}");
        assert_eq!(status, 0, "jobs's");
        assert_eq!(
            typed_status(&mut h, &["false &", "wait %1"]).0,
            1,
            "the job's"
        );
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        assert_eq!(
            typed_status(&mut h, &["false | t-spin &", "wait 101"]).0,
            1,
            "the pid's"
        );
    }

    #[test]
    fn a_script_s_wait_says_nothing_of_the_job() {
        let mut h = with_jobs();
        h.put("/tmp/s.sh", b"t-spin &\nwait %1\njobs\n");
        let mut out = crate::testing::FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (0, "+ t-spin &\n+ wait %1\n+ jobs\n".into())
        );
        assert_eq!(h.programs.waited, [101]);
    }

    #[test]
    fn ctrl_c_ends_a_wait_and_the_jobs_run_on() {
        let mut h = with_jobs();
        h.programs.ctrl_c_after = Some(1);
        let (status, out) = typed_status(&mut h, &["t-spin &", "t-spin 2 &", "wait", "jobs"]);
        assert!(
            out.contains(
                "# wait\n^C\nroot@relay:/# jobs\n[2]+  Running                 t-spin 2 &\n"
            ),
            "{out}"
        );
        assert_eq!(h.programs.waited, [101], "the first, then the Ctrl-C");
        assert_eq!(status, 0, "jobs's");
        h.programs.ctrl_c_after = Some(0);
        assert_eq!(typed_status(&mut h, &["t-spin &", "wait %1"]).0, 130);
        h.programs.ctrl_c_after = Some(0);
        assert_eq!(typed_status(&mut h, &["t-spin &", "wait"]).0, 130);
    }

    #[test]
    fn wait_refuses_what_is_no_job_of_its_as_bash_s_does() {
        let mut h = with_jobs();
        for (line, status, said) in [
            ("wait %3", 127, "wait: %3: no such job"),
            (
                "wait 999",
                127,
                "wait: pid 999 is not a child of this shell",
            ),
            ("wait abc", 1, "wait: `abc': not a pid or valid job spec"),
            ("wait %x", 127, "wait: %x: no such job"),
            ("wait -n", 2, "wait: -n: invalid option"),
        ] {
            assert_eq!(
                h.spawning(line),
                (status, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert_eq!(
            h.run("wait"),
            (0, String::new()),
            "no jobs without programs"
        );
        assert_eq!(h.run("wait %1").0, 127);
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

with:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs", "wait"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub alive_at_spawn: Vec<usize>,
    /// The tees pushed and not popped, by path.
````

with:

````rust
    pub alive_at_spawn: Vec<usize>,
    /// A Ctrl-C ends the `wait_or_ctrl_c` after this many more.
    pub ctrl_c_after: Option<usize>,
    /// Every pid `wait_or_ctrl_c` waited for.
    pub waited: Vec<u32>,
    /// The tees pushed and not popped, by path.
````

Replace:

````rust
            alive_at_spawn: Vec::new(),
            tees: Vec::new(),
````

with:

````rust
            alive_at_spawn: Vec::new(),
            ctrl_c_after: None,
            waited: Vec::new(),
            tees: Vec::new(),
````

Replace:

````rust
        Ok(self.children.remove(i).status)
    }
````

with:

````rust
        Ok(self.children.remove(i).status)
    }
    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        if let Some(n) = &mut self.ctrl_c_after {
            if *n == 0 {
                self.ctrl_c_after = None;
                return Err(Errno::EINTR);
            }
            *n -= 1;
        }
        self.waited.push(pid);
        self.wait(pid)
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `wait_or_ctrl_c` is not a member of trait `Programs` ``.

- [ ] **Step 5: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust

    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
````

with:

````rust

    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        match sys::wait_with(i64::from(pid), relay_abi::spawn::WAIT_CTRL_C) {
            Ok(Some((_, status))) => Ok(status),
            Ok(None) => Err(Errno::ECHILD),
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
````

- [ ] **Step 6: Change `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, and
//! later `wait` and `kill`. They work on the shell's jobs, which only the
//! shell has (`Ctx::control`).

use crate::ctx::Ctx;
use crate::jobs::number;
````

with:

````rust
//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, `wait`,
//! and later `kill`. They work on the shell's jobs, which only the shell
//! has (`Ctx::control`).

use crate::ctx::{Ctx, JobControl};
use crate::jobs::number;
````

Replace:

````rust
        ctx.out(line.as_bytes());
    }
````

with:

````rust
        ctx.out(line.as_bytes());
    }
    status
}

/// `wait` waited for a job: it ended, or a Ctrl-C ended the wait.
enum Waited {
    Ended,
    Interrupted,
}

/// Waits for each of `pids`, a job's processes that have not ended,
/// recording how they end.
fn wait_for(control: &mut JobControl<'_>, pids: &[u32]) -> Waited {
    let Some(programs) = control.programs.as_deref_mut() else {
        return Waited::Ended;
    };
    for &pid in pids {
        match programs.wait_or_ctrl_c(pid) {
            Ok(w) => {
                control.jobs.ended(pid, w);
            }
            Err(vfs::Errno::EINTR) => return Waited::Interrupted,
            // Not the shell's child after all: nothing to wait for.
            Err(_) => {
                let gone = relay_abi::WaitStatus::exited(127);
                control.jobs.ended(pid, gone);
            }
        }
    }
    Waited::Ended
}

/// `wait [%n | PID]...`: waits for every job, or for the jobs and
/// processes named, as bash's does (spec §9.2). With operands the status
/// is the last one's: a job's last process's, or 127 for one that is no
/// job (`%3: no such job`, `pid 9 is not a child of this shell`); without,
/// 0, and the jobs that ended leave the table without a word. At the
/// prompt a job named that ends says how at once. Ctrl-C ends the wait
/// (`^C`, 130); the jobs run on.
pub fn wait(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Some(option) = args.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        ctx.fail(NAME, format_args!("wait: {option}: invalid option"));
        return 2;
    }
    // A process that has ended is collected by the wait for it.
    let Some(control) = ctx.control.as_mut() else {
        return 0;
    };
    if args.is_empty() {
        let pids = control.jobs.all_running();
        let waited = wait_for(control, &pids);
        control.jobs.forget_finished();
        if let Waited::Interrupted = waited {
            ctx.cancelled = true;
        }
        return 0;
    }
    let mut status = 0;
    for a in args {
        let Some(control) = ctx.control.as_mut() else {
            break;
        };
        let job = match (a.strip_prefix('%'), number(a)) {
            (Some(n), _) => match number(n).filter(|&n| control.jobs.has(n)) {
                Some(n) => Some((n, control.jobs.running(n))),
                None => {
                    ctx.fail(NAME, format_args!("wait: {a}: no such job"));
                    status = 127;
                    continue;
                }
            },
            (None, Some(pid)) => match control.jobs.of_pid(pid) {
                Some(n) => {
                    let running = control.jobs.running(n).into_iter().filter(|&p| p == pid);
                    Some((n, running.collect()))
                }
                None => {
                    ctx.fail(
                        NAME,
                        format_args!("wait: pid {pid} is not a child of this shell"),
                    );
                    status = 127;
                    continue;
                }
            },
            (None, None) => None,
        };
        let Some((n, pids)) = job else {
            ctx.fail(
                NAME,
                format_args!("wait: `{a}': not a pid or valid job spec"),
            );
            status = 1;
            continue;
        };
        if let Waited::Interrupted = wait_for(control, &pids) {
            ctx.cancelled = true;
            return status;
        }
        // A pid's own status, or the job's (its last process's).
        status = match number(a) {
            Some(pid) => control.jobs.status_of_pid(pid),
            None => control.jobs.status(n),
        }
        .unwrap_or(0);
        let report = control.report;
        if let Some(line) = control.jobs.take(n)
            && report
        {
            ctx.out(line.as_bytes());
        }
    }
````

- [ ] **Step 7: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use control::jobs;
pub use grep::grep;
````

with:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use control::{jobs, wait};
pub use grep::grep;
````

Replace:

````rust
    Builtin {
        name: "wc",
````

with:

````rust
    Builtin {
        name: "wait",
        help: "wait for background jobs",
        run: control::wait,
    },
    Builtin {
        name: "wc",
````

Replace:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs"];

````

with:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs", "wait"];

````

- [ ] **Step 8: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust
    pub programs: Option<&'a mut dyn Programs>,
}
````

with:

````rust
    pub programs: Option<&'a mut dyn Programs>,
    /// The shell reads commands at its prompt, so `wait %n` says how the
    /// job ended, as bash's interactive shell does.
    pub report: bool,
}
````

- [ ] **Step 9: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Copies the console into the file at `path` from now on (a script's
````

with:

````rust
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Waits for the child `pid` to end, as `wait` does, unless a Ctrl-C
    /// is typed at the shell's prompt meanwhile (`EINTR`, `WAIT_CTRL_C`).
    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// Copies the console into the file at `path` from now on (a script's
````

- [ ] **Step 10: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, replace:

````rust
            .map(|j| j.number)
    }
````

with:

````rust
            .map(|j| j.number)
    }

    /// How `pid`, a job's process, ended, once it has.
    pub fn status_of_pid(&self, pid: u32) -> Option<i32> {
        self.jobs
            .iter()
            .flat_map(|j| &j.procs)
            .find(|&&(p, _)| p == pid)
            .and_then(|(_, w)| w.as_ref().map(status_of))
    }
````

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
                        programs: self.runner.programs(),
                    };
````

with:

````rust
                        programs: self.runner.programs(),
                        report: self.prompting && !self.in_script,
                    };
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 211 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): add the wait built-in

`wait` (spec §9.2) waits for every background job, and they leave the
table without a word, as bash's do; `wait %n` or `wait PID` for one, the
status that job's last process's or that process's own, and at the
prompt a job that ends there says how at once. What is no job of the
shell's gets bash's message and 127 (`%3: no such job`, `pid 9 is not a
child of this shell`), what is no pid 1. A Ctrl-C at the prompt ends
the wait (`^C`, 130) and the jobs run on: Programs::wait_or_ctrl_c
waits with WAIT_CTRL_C. Until plan 3's $? the status is the shell's
last status.
EOF
````


### Task 19: `wait` says what bash's says, where bash says it

The prototype's review (minor, M1, M2 and M4) and decision 8: three ways `wait` differed from bash 5.2's. A bare `wait` let every job that ended leave the table without a word; bash's is silent only of jobs that exited, and tells of one a signal ended (`[1]+  Killed                  sleep 30`), as it now does (`Jobs::report_killed`). The line `wait %n` prints went to standard output, so `wait %1 > f` put it into `f`; like bash's notices, both now go to the screen. And `wait PID` of a job's process the shell had already reported said `pid N is not a child of this shell` (127), where bash, which keeps the statuses of its background pids, gives that process's status once: the table keeps those of the jobs that left it, the table's 64 at most (`Jobs::take_gone`). The red run is the shell's tests, which cannot find `report_killed`. Mutation checks: every finished job told of by a bare `wait`, either line into the redirection, no answer for a reported job's pid, the kept statuses unbounded and a kept status answered twice each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/jobs.rs`

**Interfaces:**
- Consumes: Task 18's `wait`.
- Produces: `Jobs::{report_killed(&mut self) -> Vec<String>, take_gone(&mut self, pid) -> Option<i32>}` (`forget_finished` goes).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, replace:

````rust
    }
}
````

with:

````rust
    }

    #[test]
    fn a_bare_wait_tells_of_a_killed_job_on_the_screen() {
        // bash 5.2: `kill -9 %1; wait` says the job was killed; a job that
        // exited it does not mention.
        let mut h = with_jobs();
        h.programs.known.push((
            "/bin/t-fault",
            WaitStatus::killed(relay_abi::wait::KILLED_KILL),
        ));
        h.programs.lives.push(("/bin/t-fault", 99));
        let out = typed(&mut h, &["t-fault x &", "t-spin &", "wait > /tmp/w"]);
        assert!(
            out.ends_with(
                "# wait > /tmp/w\n[1]-  Killed                  t-fault x\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(h.get("/tmp/w"), b"", "never into the redirection");
    }

    #[test]
    fn wait_for_a_job_says_how_it_ended_on_the_screen() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["t-spin &", "wait %1 > /tmp/w"]);
        assert!(
            out.ends_with(
                "# wait %1 > /tmp/w\n[1]+  Done                    t-spin\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(h.get("/tmp/w"), b"");
    }

    #[test]
    fn wait_for_a_reported_job_s_pid_gives_its_status_once() {
        // bash 5.2: `false &`, Enter (`Exit 1`), then `wait $!` is 1.
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        let (status, out) = typed_status(&mut h, &["false &", "", "wait 101"]);
        assert!(out.ends_with("# wait 101\nroot@relay:/# "), "{out}");
        assert_eq!(status, 1);
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        let (status, out) = typed_status(&mut h, &["false &", "", "wait 101", "wait 101"]);
        assert!(
            out.ends_with("relay-sh: wait: pid 101 is not a child of this shell\nroot@relay:/# "),
            "{out}"
        );
        assert_eq!(status, 127, "once");
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, replace:

````rust
        j.ended(40, WaitStatus::exited(0));
        j.forget_finished();
        assert!(!j.has(a) && j.has(b));
    }
````

with:

````rust
        j.ended(40, WaitStatus::exited(0));
        assert!(j.report_killed().is_empty(), "it exited");
        assert!(!j.has(a) && j.has(b));
    }

    #[test]
    fn a_job_s_processes_answer_once_after_it_left_the_table() {
        let mut j = Jobs::new();
        let a = j.add(60, &[60, 61], "false | true");
        j.ended(60, WaitStatus::exited(1));
        j.ended(61, WaitStatus::exited(0));
        assert_eq!(j.report().len(), 1);
        assert!(!j.has(a));
        assert_eq!((j.take_gone(60), j.take_gone(61)), (Some(1), Some(0)));
        assert_eq!(j.take_gone(60), None, "once");
        // At most the table's 64 are kept, the oldest forgotten.
        for pid in 100..170 {
            let n = j.add(pid, &[pid], "true");
            j.ended(pid, WaitStatus::exited(0));
            assert!(j.take(n).is_some());
        }
        assert_eq!((j.take_gone(105), j.take_gone(106)), (None, Some(0)));
        // A killed job is told of by `wait`, an exited one not.
        j.add(200, &[200], "t-spin");
        j.add(201, &[201], "sleep 5");
        j.ended(200, WaitStatus::killed(relay_abi::wait::KILLED_KILL));
        j.ended(201, WaitStatus::exited(0));
        assert_eq!(
            j.report_killed(),
            ["[1]-  Killed                  t-spin\n"]
        );
        assert!(j.is_empty());
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `report_killed` found for struct `jobs::Jobs` in the current scope ``; `` no method named `take_gone` found for struct `jobs::Jobs` in the current scope ``.

- [ ] **Step 4: Change `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
/// is the last one's: a job's last process's, or 127 for one that is no
/// job (`%3: no such job`, `pid 9 is not a child of this shell`); without,
/// 0, and the jobs that ended leave the table without a word. At the
/// prompt a job named that ends says how at once. Ctrl-C ends the wait
/// (`^C`, 130); the jobs run on.
pub fn wait(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

with:

````rust
/// is the last one's: a job's last process's, or 127 for one that is no
/// job (`%3: no such job`, `pid 9 is not a child of this shell`; a job's
/// process already reported answers once more); without, 0, and the jobs
/// that ended leave the table, those a signal ended saying so. At the
/// prompt a job named that ends says how at once, on the screen. Ctrl-C
/// ends the wait (`^C`, 130); the jobs run on.
pub fn wait(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
````

Replace:

````rust
        let waited = wait_for(control, &pids);
        control.jobs.forget_finished();
        if let Waited::Interrupted = waited {
````

with:

````rust
        let waited = wait_for(control, &pids);
        let report = control.report;
        let killed = control.jobs.report_killed();
        // Notices, as bash's: on the screen, never into a redirection.
        for line in killed.iter().filter(|_| report) {
            ctx.err(line.as_bytes());
        }
        if let Waited::Interrupted = waited {
````

Replace:

````rust
                    Some((n, running.collect()))
                }
````

with:

````rust
                    Some((n, running.collect()))
                }
                // A job's process the table let go of: once more.
                None if let Some(gone) = control.jobs.take_gone(pid) => {
                    status = gone;
                    continue;
                }
````

Replace:

````rust
        {
            ctx.out(line.as_bytes());
        }
````

with:

````rust
        {
            ctx.err(line.as_bytes());
        }
````

- [ ] **Step 5: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    jobs: Vec<Job>,
}

````

with:

````rust
    jobs: Vec<Job>,
    /// How the processes of the jobs that left the table ended, the newest
    /// last, at most `GONE` of them: `wait PID` answers for each once, as
    /// bash's does.
    gone: Vec<(u32, WaitStatus)>,
}

/// The processes of jobs gone from the table whose statuses are kept.
const GONE: usize = relay_abi::proc::PROC_MAX;

````

Replace:

````rust
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        self.jobs.retain(|j| !(listed(j) && j.finished()));
        lines
````

with:

````rust
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        self.drop_jobs(|j| listed(j) && j.finished());
        lines
````

Replace:

````rust
            .collect();
        self.jobs.retain(|j| !j.finished());
        lines
````

with:

````rust
            .collect();
        self.drop_jobs(Job::finished);
        lines
````

Replace:

````rust
        let line = self.line(i);
        self.jobs.remove(i);
        Some(line)
    }

    /// The jobs that have ended leave the table without a word (`wait`
    /// without operands, as bash's).
    pub fn forget_finished(&mut self) {
        self.jobs.retain(|j| !j.finished());
    }
````

with:

````rust
        let line = self.line(i);
        self.drop_jobs(|j| j.number == number);
        Some(line)
    }

    /// The jobs that have ended leave the table, and the lines of those a
    /// signal ended are given: `wait` without operands says nothing of a
    /// job that exited, as bash's does, but tells of one that was killed.
    pub fn report_killed(&mut self) -> Vec<String> {
        let killed = |j: &Job| j.finished() && j.last().is_some_and(|w| w.how != EXITED);
        let lines = (0..self.jobs.len())
            .filter(|&i| killed(&self.jobs[i]))
            .map(|i| self.line(i))
            .collect();
        self.drop_jobs(Job::finished);
        lines
    }

    /// The status `pid`, a process of a job gone from the table, ended
    /// with; asked once (bash's `wait $!` after the job was reported).
    pub fn take_gone(&mut self, pid: u32) -> Option<i32> {
        let i = self.gone.iter().position(|&(p, _)| p == pid)?;
        Some(status_of(&self.gone.remove(i).1))
    }

    /// The jobs `which` names leave the table; their processes' statuses
    /// are kept, the oldest forgotten beyond `GONE`.
    fn drop_jobs(&mut self, which: impl Fn(&Job) -> bool) {
        for job in self.jobs.iter().filter(|j| which(j)) {
            for &(pid, w) in &job.procs {
                if let Some(w) = w {
                    self.gone.push((pid, w));
                }
            }
        }
        let over = self.gone.len().saturating_sub(GONE);
        self.gone.drain(..over);
        self.jobs.retain(|j| !which(j));
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 215 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): make wait say what bash's says, where bash says it

Three ways wait differed from bash 5.2's, found by the review. A bare
`wait` let every job that ended leave the table without a word; bash's
is silent only of jobs that exited, and tells of one a signal ended
(`[1]+  Killed  sleep 30`), as it now does. The line `wait %n` prints
went to standard output, so `wait %1 > f` put it into f; like bash's
notices, both now go to the screen. And `wait PID` of a job's process
the shell had already reported said `pid N is not a child of this
shell` (127), where bash, which keeps the statuses of its background
pids, gives that process's status once: the table keeps those of the
jobs that left it, the table's 64 at most.

Refs: review M1, M2, M4
EOF
````


### Task 20: The `kill` built-in

Spec §9.3 and decision 8: `kill PID...` and `kill %n...` kill processes, or a job's whole process group (`kill(-pgid)`), with bash's messages: `(1) - Operation not permitted`, `(999) - No such process`, `%3: no such job`, `abc: arguments must be process or job IDs`; the status is 1 if any was not killed, and without a target it is the usage line, status 2. The job is reported `Killed` before the next prompt. Killing is the only signal (spec §15): `-9`, `-KILL`, `-SIGKILL` and `-s KILL` (any case) are taken, another signal Linux has is `not supported`, a name it has not is an `invalid signal specification`, and `-s` alone `option requires an argument`; `kill 0`, which in bash kills the shell's own group, is the kernel's `ESRCH`. `Programs` gains `kill`; the fake ends what it kills at once. The red run is the shell's tests, which cannot find the command. Mutation checks: `%n` killing the group's number as a pid, `-9` not killing, names case-sensitive, the `SIG` prefix kept, real signals called invalid and no usage line each fail a test (refusing pid 0 itself survived and is gone).

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 16's `JobControl`.
- Produces: `commands::kill`; `Programs::kill(&mut self, target: i64) -> Result<(), Errno>`; the fake's `FakePrograms::kills`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, replace:

````rust
    }
}
````

with:

````rust
    }

    #[test]
    fn kill_ends_a_job_s_whole_group_or_a_process() {
        let mut h = with_jobs();
        let out = typed(
            &mut h,
            &[
                "t-spin | t-spin &",
                "t-spin 3 &",
                "kill %1",
                "",
                "kill -9 103",
                "",
            ],
        );
        assert!(
            out.contains("# kill %1\n[1]-  Killed                  t-spin | t-spin\n"),
            "{out}"
        );
        assert!(
            out.contains("# kill -9 103\n[2]+  Killed                  t-spin 3\n"),
            "{out}"
        );
        assert_eq!(h.programs.kills, [-101, 103], "the group, then the process");
        assert!(h.programs.children().is_empty());
    }

    #[test]
    fn kill_says_what_it_could_not_kill_as_bash_s_does() {
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["t-spin &", "kill 1 999 %2 %x abc 0x 101"]);
        assert!(
            out.contains(
                "relay-sh: kill: (1) - Operation not permitted\n\
                 relay-sh: kill: (999) - No such process\n\
                 relay-sh: kill: %2: no such job\n\
                 relay-sh: kill: %x: no such job\n\
                 relay-sh: kill: abc: arguments must be process or job IDs\n\
                 relay-sh: kill: 0x: arguments must be process or job IDs\n"
            ),
            "{out}"
        );
        assert_eq!(status, 1, "though 101 was killed");
        assert_eq!(h.programs.kills, [1, 999, 101]);
        assert_eq!(
            h.spawning("kill"),
            (
                2,
                "relay-sh: kill: usage: kill [-s KILL | -KILL] pid | %job ...\n".into()
            )
        );
        assert_eq!(h.spawning("kill -9").0, 2);
        // bash kills its own group with 0; the kernel's 0 names nothing.
        assert_eq!(
            h.spawning("kill 0"),
            (1, "relay-sh: kill: (0) - No such process\n".into())
        );
        // A shell without programs kills nothing.
        assert_eq!(
            h.run("kill 5"),
            (1, "relay-sh: kill: (5) - No such process\n".into())
        );
    }

    #[test]
    fn killing_is_the_only_signal() {
        let mut h = with_jobs();
        for line in [
            "kill -9 5",
            "kill -KILL 5",
            "kill -kill 5",
            "kill -SIGKILL 5",
            "kill -s KILL 5",
            "kill -s 9 -- 5",
        ] {
            h.programs.kills.clear();
            assert_eq!(
                h.spawning(line),
                (1, "relay-sh: kill: (5) - No such process\n".into()),
                "{line}"
            );
            assert_eq!(h.programs.kills, [5], "{line}");
        }
        h.programs.kills.clear();
        for (line, said) in [
            ("kill -TERM 5", "kill: TERM: not supported"),
            ("kill -15 5", "kill: 15: not supported"),
            ("kill -s HUP 5", "kill: HUP: not supported"),
            ("kill -FOO 5", "kill: FOO: invalid signal specification"),
            ("kill -99 5", "kill: 99: invalid signal specification"),
            ("kill -s", "kill: -s: option requires an argument"),
        ] {
            assert_eq!(
                h.spawning(line),
                (1, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert!(h.programs.kills.is_empty(), "nothing killed");
        assert_eq!(
            h.spawning("kill -- -5"),
            (
                1,
                "relay-sh: kill: -5: arguments must be process or job IDs\n".into()
            )
        );
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs", "wait"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

with:

````rust
            .collect();
        assert_eq!(own, ["cd", "exit", "help", "jobs", "kill", "wait"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub waited: Vec<u32>,
    /// The tees pushed and not popped, by path.
````

with:

````rust
    pub waited: Vec<u32>,
    /// Every `kill`'s target.
    pub kills: Vec<i64>,
    /// The tees pushed and not popped, by path.
````

Replace:

````rust
            waited: Vec::new(),
            tees: Vec::new(),
````

with:

````rust
            waited: Vec::new(),
            kills: Vec::new(),
            tees: Vec::new(),
````

Replace:

````rust
        Ok(self.children.remove(i).status)
    }
````

with:

````rust
        Ok(self.children.remove(i).status)
    }
    /// Its children end at once, killed; process 1 is refused, and any
    /// other pid or group is none.
    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        self.kills.push(target);
        if target == 1 || target == -1 {
            return Err(Errno::EPERM);
        }
        let hit = |c: &FakeChild| match target {
            t if t < 0 => i64::from(c.group) == -t,
            t => i64::from(c.pid) == t,
        };
        let mut found = false;
        for c in self.children.iter_mut().filter(|c| hit(c)) {
            c.status = WaitStatus::killed(relay_abi::wait::KILLED_KILL);
            c.ends_at = self.round;
            found = true;
        }
        if found { Ok(()) } else { Err(Errno::ESRCH) }
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `kill` is not a member of trait `Programs` ``.

- [ ] **Step 5: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust

    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
````

with:

````rust

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        sys::kill(target).map_err(Errno::from_number)
    }

    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
````

- [ ] **Step 6: Change `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, `wait`,
//! and later `kill`. They work on the shell's jobs, which only the shell
//! has (`Ctx::control`).

````

with:

````rust
//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, `wait`
//! and `kill`. They work on the shell's jobs, which only the shell has
//! (`Ctx::control`).

````

Replace:

````rust
        {
            ctx.err(line.as_bytes());
        }
    }
````

with:

````rust
        {
            ctx.err(line.as_bytes());
        }
    }
    status
}

/// Linux's signal names, which `kill` knows but for `KILL` refuses: only
/// killing exists here (spec §15).
const SIGNALS: &[&str] = &[
    "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1", "SEGV", "USR2",
    "PIPE", "ALRM", "TERM", "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU", "URG",
    "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO", "PWR", "SYS",
];

/// What `kill` makes of a signal it was given: `Ok` for killing, else
/// the message.
fn signal(spec: &str) -> Result<(), &'static str> {
    let upper = spec.to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    match (name, number(name)) {
        ("KILL", _) | (_, Some(9)) => Ok(()),
        (_, Some(1..=64)) => Err("not supported"),
        (n, None) if SIGNALS.contains(&n) => Err("not supported"),
        _ => Err("invalid signal specification"),
    }
}

/// `kill [-9 | -KILL | -s KILL] PID | %n...` (spec §9.3): kills each
/// process, or each job's whole process group, as bash's does with
/// SIGKILL, its messages bash's (`(1) - Operation not permitted`, `(9) -
/// No such process`, `%3: no such job`); the status is 1 if any was not
/// killed. Killing is the only signal here: another is not supported.
pub fn kill(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let mut targets = args;
    loop {
        match targets {
            [s, spec, rest @ ..] if s == "-s" => {
                if let Err(why) = signal(spec) {
                    return ctx.fail(NAME, format_args!("kill: {spec}: {why}"));
                }
                targets = rest;
            }
            [s] if s == "-s" => {
                return ctx.fail(NAME, format_args!("kill: -s: option requires an argument"));
            }
            [dashes, rest @ ..] if dashes == "--" => {
                targets = rest;
                break;
            }
            [spec, rest @ ..] if spec.len() > 1 && spec.starts_with('-') => {
                if let Err(why) = signal(&spec[1..]) {
                    return ctx.fail(NAME, format_args!("kill: {}: {why}", &spec[1..]));
                }
                targets = rest;
            }
            _ => break,
        }
    }
    if targets.is_empty() {
        ctx.fail(
            NAME,
            format_args!("kill: usage: kill [-s KILL | -KILL] pid | %job ..."),
        );
        return 2;
    }
    let mut status = 0;
    for t in targets {
        let target = match t.strip_prefix('%') {
            Some(n) => {
                let pgid = ctx
                    .control
                    .as_ref()
                    .and_then(|c| number(n).and_then(|n| c.jobs.pgid(n)));
                match pgid {
                    Some(pgid) => -i64::from(pgid),
                    None => {
                        status = ctx.fail(NAME, format_args!("kill: {t}: no such job"));
                        continue;
                    }
                }
            }
            None => match number(t) {
                Some(pid) => i64::from(pid),
                None => {
                    status = ctx.fail(
                        NAME,
                        format_args!("kill: {t}: arguments must be process or job IDs"),
                    );
                    continue;
                }
            },
        };
        let programs = ctx.control.as_mut().and_then(|c| c.programs.as_deref_mut());
        let killed = programs.map_or(Err(vfs::Errno::ESRCH), |p| p.kill(target));
        if let Err(e) = killed {
            status = ctx.fail(
                NAME,
                format_args!("kill: ({}) - {e}", target.unsigned_abs()),
            );
        }
    }
````

- [ ] **Step 7: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use control::{jobs, wait};
pub use grep::grep;
````

with:

````rust
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use control::{jobs, kill, wait};
pub use grep::grep;
````

Replace:

````rust
        run: control::jobs,
    },
````

with:

````rust
        run: control::jobs,
    },
    Builtin {
        name: "kill",
        help: "end processes, or background jobs",
        run: control::kill,
    },
````

Replace:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs", "wait"];

````

with:

````rust
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help", "jobs", "kill", "wait"];

````

- [ ] **Step 8: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Waits for the child `pid` to end, as `wait` does, unless a Ctrl-C
````

with:

````rust
    fn collect(&mut self) -> Option<(u32, WaitStatus)>;
    /// Kills the process `target`, or the process group `-target`
    /// (`kill`): `ESRCH` if there is none, `EPERM` for process 1.
    fn kill(&mut self, target: i64) -> Result<(), Errno>;
    /// Waits for the child `pid` to end, as `wait` does, unless a Ctrl-C
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 218 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): add the kill built-in

`kill PID...` and `kill %n...` (spec §9.3) kill processes, or a job's
whole process group (kill(-pgid)), with bash's messages: `(1) -
Operation not permitted`, `(999) - No such process`, `%3: no such job`,
`abc: arguments must be process or job IDs`; the status is 1 if any was
not killed, and without a target it is the usage line, status 2. A
job's text is reported Killed before the next prompt. Killing is the
only signal (spec §15): -9, -KILL, -SIGKILL and -s KILL are taken, any
other signal Linux has is not supported, and a name it has not is an
invalid signal specification. Programs gains kill.
EOF
````


### Task 21: `kill` of a job that has ended says what bash says

The prototype's review (minor, M3) and decision 8: `kill %1` of a job that had ended but was not yet collected was silent, with status 0, where bash 5.2 says `(pid) - No such process` (1) and then the Done line: the kernel's `kill` counts a zombie. Task 14's collecting before a typed line settles it: the job is collected first, and the kill finds nothing. `FakePrograms` hid it, rewriting an ended child's status as killed, which the kernel never does to a zombie; it now keeps how the child ended, and a test of `sleep 1 &` then `kill %1` pins bash's answer. The task changes the fake and a test, so it has no failing run. Mutation check: no collecting before the typed line fails the test (`kill %1` silent, status 0).

**Files:**
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 20's `kill`; Task 14's collecting.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, replace:

````rust
        );
    }
}
````

with:

````rust
        );
    }

    #[test]
    fn kill_of_a_job_that_has_ended_says_it_is_no_process_as_bash_s_does() {
        // bash 5.2: `true &`, a moment, `kill %1`: `(pid) - No such
        // process`, status 1, then the Done line. The job ended while the
        // line was typed, and is collected before it runs.
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["sleep 1 &", "kill %1"]);
        assert!(
            out.ends_with(
                "# kill %1\nrelay-sh: kill: (101) - No such process\n\
                 [1]+  Done                    sleep 1\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(status, 1);
    }
}
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    }
    /// Its children end at once, killed; process 1 is refused, and any
    /// other pid or group is none.
````

with:

````rust
    }
    /// Its children end at once, killed, as the kernel's `kill` ends them;
    /// one that has ended already and is not yet collected counts, but
    /// keeps how it ended, as a zombie does. Process 1 is refused, and any
    /// other pid or group is none.
````

Replace:

````rust
        let mut found = false;
        for c in self.children.iter_mut().filter(|c| hit(c)) {
            c.status = WaitStatus::killed(relay_abi::wait::KILLED_KILL);
            c.ends_at = self.round;
            found = true;
````

with:

````rust
        let mut found = false;
        let round = self.round;
        for c in self.children.iter_mut().filter(|c| hit(c)) {
            if c.ends_at > round {
                c.status = WaitStatus::killed(relay_abi::wait::KILLED_KILL);
                c.ends_at = round;
            }
            found = true;
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 219 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): show kill of a job that has ended as bash says it

The review found `kill %1` of a job that had ended but was not yet
collected silent, with status 0, where bash 5.2 says `(pid) - No such
process` (1) and then the Done line: the kernel's kill counts a zombie.
Collecting before a typed line runs, an earlier fix, settles it: the
job is collected first, and the kill finds nothing. FakePrograms hid
it, rewriting an ended child's status as killed, which the kernel never
does to a zombie; it now keeps how the child ended, and a test of
`sleep 1 &` then `kill %1` pins bash's answer.

Refs: review M3
EOF
````


### Task 22: The current and previous jobs are named as bash names them

The prototype's review (minor, M10) and decisions 7 and 8: `jobs`, `wait` and `kill` took only `%n`, and said `no such job` of the job specs bash has for the current job (`%%`, `%+`, `%`) and the previous one (`%-`, the current one when it is alone, as bash 5.2 showed). `Jobs::spec` names them all. `kill`'s `-0`, which tests that a process exists, and `-l`, which lists the signals, are `not supported` here, rather than an invalid signal and an invalid option: killing is the only signal. The red run is the shell's tests, which cannot find `spec`. Mutation checks: `%-` alone not the current job, `%+` not the current job, `jobs` not reading specs and signal 0 called invalid each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/jobs.rs`

**Interfaces:**
- Consumes: Tasks 16, 18 and 20.
- Produces: `Jobs::spec(&self, spec: &str) -> Option<u32>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, replace:

````rust
        assert_eq!(status, 1);
    }
}
````

with:

````rust
        assert_eq!(status, 1);
    }

    #[test]
    fn the_current_and_previous_jobs_are_named_as_bash_names_them() {
        let mut h = with_jobs();
        let out = typed(
            &mut h,
            &[
                "t-spin &",
                "t-spin 2 &",
                "jobs %%",
                "jobs %-",
                "jobs %+ %",
                "kill %-",
                "",
                "wait %%",
            ],
        );
        let line2 = "[2]+  Running                 t-spin 2 &\n";
        assert!(out.contains(&alloc::format!("# jobs %%\n{line2}")), "{out}");
        assert!(
            out.contains("# jobs %-\n[1]-  Running                 t-spin &\n"),
            "{out}"
        );
        assert!(
            out.contains(&alloc::format!("# jobs %+ %\n{line2}{line2}")),
            "{out}"
        );
        assert!(out.contains("# kill %-\n"), "{out}");
        assert_eq!(h.programs.kills, [-101], "the previous job's group");
        assert!(
            out.contains("# wait %%\n[2]+  Done                    t-spin 2\n"),
            "{out}"
        );
    }

    #[test]
    fn kill_s_signal_0_and_list_are_not_supported() {
        let mut h = with_jobs();
        for (line, said) in [
            ("kill -0 5", "kill: 0: not supported"),
            ("kill -l", "kill: -l: not supported"),
            ("kill -L", "kill: -L: not supported"),
        ] {
            assert_eq!(
                h.spawning(line),
                (1, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert!(h.programs.kills.is_empty());
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, replace:

````rust
    #[test]
    fn a_job_s_processes_answer_once_after_it_left_the_table() {
````

with:

````rust
    #[test]
    fn a_job_spec_names_a_number_or_the_current_or_previous_job() {
        let mut j = Jobs::new();
        assert_eq!(j.spec("%"), None, "no job at all");
        j.add(70, &[70], "a");
        assert_eq!((j.spec(""), j.spec("-")), (Some(1), Some(1)), "alone, both");
        j.add(71, &[71], "b");
        j.add(72, &[72], "c");
        for (spec, number) in [("", 3), ("%", 3), ("+", 3), ("-", 2), ("1", 1), ("3", 3)] {
            assert_eq!(j.spec(spec), Some(number), "{spec:?}");
        }
        for spec in ["4", "0", "x", "-1", "++"] {
            assert_eq!(j.spec(spec), None, "{spec:?}");
        }
    }

    #[test]
    fn a_job_s_processes_answer_once_after_it_left_the_table() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `spec` found for struct `jobs::Jobs` in the current scope ``.

- [ ] **Step 4: Change `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        .iter()
        .map(|a| (a, number(a.strip_prefix('%').unwrap_or(a))))
        .partition(|(_, n)| n.is_some_and(|n| control.jobs.has(n)));
    let named: Vec<u32> = named.into_iter().filter_map(|(_, n)| n).collect();
````

with:

````rust
        .iter()
        .map(|a| match a.strip_prefix('%') {
            Some(spec) => (a, control.jobs.spec(spec)),
            None => (a, number(a).filter(|&n| control.jobs.has(n))),
        })
        .partition(|(_, n)| n.is_some());
    let named: Vec<u32> = named.into_iter().filter_map(|(_, n)| n).collect();
````

Replace:

````rust
        let job = match (a.strip_prefix('%'), number(a)) {
            (Some(n), _) => match number(n).filter(|&n| control.jobs.has(n)) {
                Some(n) => Some((n, control.jobs.running(n))),
````

with:

````rust
        let job = match (a.strip_prefix('%'), number(a)) {
            (Some(spec), _) => match control.jobs.spec(spec) {
                Some(n) => Some((n, control.jobs.running(n))),
````

Replace:

````rust
        ("KILL", _) | (_, Some(9)) => Ok(()),
        (_, Some(1..=64)) => Err("not supported"),
        (n, None) if SIGNALS.contains(&n) => Err("not supported"),
````

with:

````rust
        ("KILL", _) | (_, Some(9)) => Ok(()),
        // Signal 0 tests that a process exists, in bash's kill.
        (_, Some(0..=64)) => Err("not supported"),
        (n, None) if SIGNALS.contains(&n) => Err("not supported"),
````

Replace:

````rust
            }
            [spec, rest @ ..] if spec.len() > 1 && spec.starts_with('-') => {
````

with:

````rust
            }
            // bash lists the signals; there is one here.
            [l, ..] if l == "-l" || l == "-L" => {
                return ctx.fail(NAME, format_args!("kill: {l}: not supported"));
            }
            [spec, rest @ ..] if spec.len() > 1 && spec.starts_with('-') => {
````

Replace:

````rust
                    .as_ref()
                    .and_then(|c| number(n).and_then(|n| c.jobs.pgid(n)));
                match pgid {
````

with:

````rust
                    .as_ref()
                    .and_then(|c| c.jobs.spec(n).and_then(|n| c.jobs.pgid(n)));
                match pgid {
````

- [ ] **Step 5: Change `crates/shell/src/jobs.rs`**

In `crates/shell/src/jobs.rs`, replace:

````rust
        false
    }
````

with:

````rust
        false
    }

    /// The job a job spec names, its `%` taken off (bash's): a number;
    /// nothing, `%` or `+` for the current job, the newest; `-` for the
    /// previous one, the one before it, or the newest when it is alone.
    pub fn spec(&self, spec: &str) -> Option<u32> {
        let newest = |k: usize| self.jobs.len().checked_sub(k).map(|i| self.jobs[i].number);
        match spec {
            "" | "%" | "+" => newest(1),
            "-" => newest(2).or_else(|| newest(1)),
            n => number(n).filter(|&n| self.has(n)),
        }
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 222 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): name the current and previous jobs as bash does

jobs, wait and kill took only `%n`, and said `no such job` of the job
specs bash has for the current job (`%%`, `%+`, `%`) and the previous
one (`%-`, the newest when it is alone). Jobs::spec names them all, as
bash 5.2 does. kill's `-0`, which tests that a process exists, and `-l`,
which lists the signals, are not supported here, rather than an invalid
signal and an invalid option: killing is the only signal (spec §15).

Refs: review M10
EOF
````


### Task 23: Background jobs under `/bin/sh`

Spec §12.3 and decisions 6 and 8: the new scenario `jobs` starts `sleep 4 &` and `t-spin &`, lists them with `jobs` (a Ctrl-C at the prompt reaches neither), refuses `kill 1`, kills `%2` and waits for each job, the shell saying how they ended in bash's words; a Ctrl-C ends a `wait` while its job runs on until `kill %1`; a job reading the console gets end of input at once, a pipeline's output reaches the screen, `sh s.sh &` leaves the prompt to the person (its transcript, a tee, gets what the screen showed meanwhile); and `exit` leaves a running `t-spin 1 &` to process 1, which collects it. Where output races the prompt, the expectations take either order. The task tests what Tasks 13–20 built, so it has no failing run. Mutation check: `wait_or_ctrl_c` without `WAIT_CTRL_C` fails the scenario (the Ctrl-C never ends the wait).

**Files:**
- Create: `tests/e2e/jobs.txt`

**Interfaces:**
- Consumes: Tasks 13–20.
- Produces: nothing new.

- [ ] **Step 1: Add the scenario `tests/e2e/jobs.txt`**

Create `tests/e2e/jobs.txt`:

````text
# Background jobs (user-space gate §9.2, §9.3; milestone 3, plan 2): a
# line that ends with & starts a job in a process group of its own,
# without the console; jobs, wait and kill work on them, and the shell
# says how each ended, in bash's words.
timeout 60
expect root@relay:~# $
send sleep 4 &
expect \n\[1\] \d+\n
send t-spin &
expect \n\[2\] \d+\n
send jobs
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# Ctrl-C at the prompt only cancels the line typed: the jobs run on.
key echo nope{ctrl-c}
expect \^C\n
send jobs
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# Process 1 cannot be killed; a job's group can, and wait says so.
send kill 1
expect \nrelay-sh: kill: \(1\) - Operation not permitted\n
send kill %2
send wait %2
expect \n\[2\]\+  Killed                  t-spin\n
send wait %1
expect \n\[1\]\+  Done                    sleep 4\n
send jobs
expect root@relay:~# jobs\nroot@relay:~# $
# Ctrl-C ends a wait at the prompt; the job runs on until it is killed.
send t-spin &
expect \n\[1\] \d+\n
send wait
alive 1
key {ctrl-c}
expect \n\^C\n
send jobs
expect \n\[1\]\+  Running                 t-spin &\n
send kill %1
# A bare wait tells of a job a signal ended, as bash's does.
send wait
expect root@relay:~# wait\n(pid \d+ \(/bin/t-spin\): killed: kill\n)?\[1\]\+  Killed                  t-spin\nroot@relay:~# $
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: kill\npid \d+ \(/bin/t-spin\): killed: kill\n
# A job reading the console gets end of input at once; a pipeline's output
# goes to the screen; a script in the background never takes the console.
send cat &
expect \n\[1\] \d+\n
send wait %1
expect \n\[1\]\+  Done                    cat\n
send seq 3 | grep 2 &
expect \n\[1\] \d+\n
send wait %1
# Printed at the prompt or after the wait's echo, whichever came first.
expect (# |\n)2\n
expect \[1\]\+  Done                    seq 3 \| grep 2\n
send echo 'sleep 1' > s.sh
send echo 'echo the script ran' >> s.sh
send sh s.sh &
expect \n\[1\] \d+\n
send echo the prompt is mine
expect \nthe prompt is mine\n
send wait %1
expect the script ran\n
expect \[1\]\+  Done                    sh s.sh\n
# Its transcript, a console tee, also got what the screen showed meanwhile
# (here what was typed at the prompt).
send cat s.log
expect \n\+ sleep 1\n
expect \+ echo the script ran\nthe script ran\n
# Four scripts in the background, a transcript's tee each, leave room for
# a fifth in the foreground: a process pushes 4 tees, 64 in all.
send echo 'sleep 2' > a.sh
send sh a.sh &
expect \n\[1\] \d+\n
send sh a.sh &
expect \n\[2\] \d+\n
send sh a.sh &
expect \n\[3\] \d+\n
send sh a.sh &
expect \n\[4\] \d+\n
send echo 'echo hello' > b.sh
send sh b.sh
expect \+ echo hello\nhello\n
send wait
expect root@relay:~# wait\nroot@relay:~# $
# exit leaves a running job to process 1, which collects it when it ends.
send t-spin 1 &
expect \n\[1\] \d+\n
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
expect \d+ iterations\n
send jobs
expect jobs\nroot@relay:~# $
````

- [ ] **Step 2: Run the `jobs` scenario**

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): show background jobs under /bin/sh

The scenario jobs (spec §12.3) starts sleep and t-spin in the
background, lists them with jobs (Ctrl-C at the prompt reaches neither),
kills one with kill %2 and waits for each, the shell saying how they
ended in bash's words; a Ctrl-C ends a wait while its job runs on, and
a bare wait tells that it was killed; a job reading the console gets
end of input at once, a pipeline's output reaches the screen, a script
in the background leaves the prompt alone (its transcript, a tee, gets
what the screen showed meanwhile), and four of them leave room for a
fifth in the foreground; exit leaves a running job to process 1.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 46 scenario(s) passed`.

````bash
git push -u origin m3p2/shell
gh pr create --base main --head m3p2/shell --title "feat(shell,relay-rt): run background jobs, with jobs, wait and kill" --body-file - <<'EOF'
## What

Milestone 3, plan 2, tasks 9–23: a line that ends with `&` starts a job in a process group of its own without the console; at the prompt the shell says `[n] <pid>`, and before each prompt how each finished job ended, numbered, marked and worded as bash 5.2 does (a script and `X | sh` say neither); `jobs` lists them; `wait` waits for every job, or for one (`%n`, `%%`, a pid), and a Ctrl-C ends it; `kill` kills processes or a job's whole group, killing being the only signal; `sh &` ends before its first prompt; bash's messages, statuses and job specs throughout. Scenario `jobs` (new) and `sh`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types the Ctrl-C a `wait` needs); plan 4's NUC check 5 runs a background job and `kill` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p2/shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `ps` (Tasks 24–25)

`ps` over `proc_list`, a command function and a program of `/bin` like the others.

Branch `m3p2/utils`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-utils`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p2/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-utils origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-utils
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p2/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p2/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-utils m3p2/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p2/shell>` and re-run `cargo xtask ci` before pushing.

### Task 24: The `ps` command

Spec §9.3 and decision 9: `ps` lists every process by pid in procps's columns, numbers right-aligned: `  PID  PPID STATE      MEM  TIME CMD`, STATE `run`, `ready`, `wait`, `read`, `sleep`, `pipe` or `zombie`, MEM the KiB of the frames its address space holds, TIME its CPU time as `m:ss`, CMD the path it was started from; it takes no operand. The shell's `System` gains `processes`: `relay-rt`'s `SysSystem` asks `proc_list` with room for all 64; the host has none, so `host-shell`'s `ps` says `ps: processes are not available`, as its `free` says of memory figures. The red run is the shell's tests, which cannot find `ps` or `TestSystem::processes`. Mutation checks: MEM in frames, seconds not padded, ticks taken as centiseconds, `read` said as `wait` and the pid as the parent each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/relay-rt/src/sysvfs.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/system.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: Task 2's `sys::proc_list`.
- Produces: `commands::ps`; `System::processes(&self) -> Option<Vec<ProcInfo>>`; `TestSystem::processes`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysvfs.rs`**

In `crates/relay-rt/src/sysvfs.rs`, replace:

````rust
        }
        fn sleep(&mut self, _: u64) {}
````

with:

````rust
        }
        fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
            None
        }
        fn sleep(&mut self, _: u64) {}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, replace:

````rust
        assert_eq!((status, h.system.poweroffs), (0, 1));
    }
}
````

with:

````rust
        assert_eq!((status, h.system.poweroffs), (0, 1));
    }

    #[test]
    fn ps_lists_every_process_with_what_it_does() {
        use relay_abi::ProcInfo;
        use relay_abi::proc::*;
        let mut h = Harness::new();
        h.system.processes = Some(alloc::vec![
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 12, b"init"),
            ProcInfo::new(2, 1, 2, STATE_WAIT, 309, 1_234, b"/bin/sh"),
            ProcInfo::new(5, 2, 5, STATE_SLEEP, 41, 3, b"/bin/sleep"),
            ProcInfo::new(6, 2, 6, STATE_READY, 35, 754_000, b"/bin/t-spin"),
            ProcInfo::new(8, 2, 8, STATE_PIPE, 38, 0, b"/bin/cat"),
            ProcInfo::new(9, 2, 9, STATE_READ, 37, 0, b"t-read"),
            ProcInfo::new(10, 2, 8, STATE_ZOMBIE, 0, 61_000, b"/bin/seq"),
            ProcInfo::new(11, 2, 11, STATE_RUN, 43, 0, b"/bin/ps"),
            ProcInfo::new(65_536, 1, 3, 99, 2_500_000, 6_000_000, &[b'x'; 64]),
        ]);
        assert_eq!(
            h.run("ps"),
            (
                0,
                "  PID  PPID STATE      MEM  TIME CMD\n\
                 \x20   1     0 wait         0  0:00 init\n\
                 \x20   2     1 wait      1236  0:01 /bin/sh\n\
                 \x20   5     2 sleep      164  0:00 /bin/sleep\n\
                 \x20   6     2 ready      140 12:34 /bin/t-spin\n\
                 \x20   8     2 pipe       152  0:00 /bin/cat\n\
                 \x20   9     2 read       148  0:00 t-read\n\
                 \x20  10     2 zombie       0  1:01 /bin/seq\n\
                 \x20  11     2 run        172  0:00 /bin/ps\n\
                 65536     1 ?      10000000 100:00 xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n"
                    .into()
            )
        );
    }

    #[test]
    fn ps_takes_no_operand_and_needs_processes() {
        let mut h = Harness::new();
        assert_eq!(h.run("ps"), (1, "ps: processes are not available\n".into()));
        h.system.processes = Some(alloc::vec::Vec::new());
        assert_eq!(h.run("ps x"), (1, "ps: extra operand 'x'\n".into()));
        assert_eq!(h.run("ps -e").0, 1);
        assert_eq!(
            h.run("ps"),
            (0, "  PID  PPID STATE      MEM  TIME CMD\n".into())
        );
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub slept: Vec<u64>,
}
````

with:

````rust
    pub slept: Vec<u64>,
    /// What `processes` says.
    pub processes: Option<Vec<relay_abi::ProcInfo>>,
}
````

Replace:

````rust
            slept: Vec::new(),
        }
````

with:

````rust
            slept: Vec::new(),
            processes: None,
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
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        self.processes.clone()
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `processes` is not a member of trait `System` ``.

- [ ] **Step 5: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        buf
    }
````

with:

````rust
        buf
    }

    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        let mut buf = [relay_abi::ProcInfo::new(0, 0, 0, 0, 0, 0, b""); relay_abi::proc::PROC_MAX];
        let n = sys::proc_list(&mut buf).ok()?;
        Some(buf[..n.min(buf.len())].to_vec())
    }
````

- [ ] **Step 6: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, reboot, sleep, sync};
pub use text::{cat, head, tail, wc};
````

with:

````rust
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, ps, reboot, sleep, sync};
pub use text::{cat, head, tail, wc};
````

Replace:

````rust
        run: system::poweroff,
    },
````

with:

````rust
        run: system::poweroff,
    },
    Builtin {
        name: "ps",
        help: "list the processes",
        run: system::ps,
    },
````

- [ ] **Step 7: Change `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! `date`, `df`, `free`, `dmesg`, `sync`, `reboot` and `poweroff`
//! (spec §7.3, §7.4).

````

with:

````rust
//! `date`, `df`, `free`, `dmesg`, `ps`, `sync`, `reboot` and `poweroff`
//! (spec §7.3, §7.4, §9.3).

````

Replace:

````rust
        );
    }
    0
}

/// `dmesg`: the kernel log.
````

with:

````rust
        );
    }
    0
}

/// `ps`: every process, by pid (spec §9.3, §16 item 9): its parent, what
/// it does (`run`, `ready`, or what it waits for: `wait` for a child,
/// `read` the console, `sleep`, `pipe`; `zombie` once it has ended), the
/// memory its address space holds in KiB, its CPU time as `m:ss`, and the
/// path it was started from. Process 0, the idle task, is no process.
pub fn ps(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "ps", args, "") {
        return status;
    }
    let Some(list) = ctx.system.processes() else {
        return ctx.fail("ps", format_args!("processes are not available"));
    };
    outln!(
        ctx,
        "{:>5} {:>5} {:<6} {:>7} {:>5} CMD",
        "PID",
        "PPID",
        "STATE",
        "MEM",
        "TIME"
    );
    for p in list {
        let secs = p.ticks / 1000;
        let time = format!("{}:{:02}", secs / 60, secs % 60);
        let kib = p.frames.saturating_mul(4);
        let cmd = String::from_utf8_lossy(p.name());
        outln!(
            ctx,
            "{:>5} {:>5} {:<6} {kib:>7} {time:>5} {cmd}",
            p.pid,
            p.ppid,
            state(p.state)
        );
    }
    0
}

/// A process's state as `ps` says it.
fn state(s: u32) -> &'static str {
    use relay_abi::proc::*;
    match s {
        STATE_RUN => "run",
        STATE_READY => "ready",
        STATE_WAIT => "wait",
        STATE_READ => "read",
        STATE_SLEEP => "sleep",
        STATE_PIPE => "pipe",
        STATE_ZOMBIE => "zombie",
        _ => "?",
    }
}

/// `dmesg`: the kernel log.
````

- [ ] **Step 8: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Waits `ms` milliseconds (`sleep`).
````

with:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Every process, by pid, for `ps`; `None` where there are none to
    /// show (on the host).
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>>;
    /// Waits `ms` milliseconds (`sleep`).
````

- [ ] **Step 9: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, replace:

````rust
        self.0.contents()
    }
````

with:

````rust
        self.0.contents()
    }
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        None
    }
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 224 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 11: Run xtask's host-shell tests**

Run: `cargo test -p xtask host_shell`

Expected: PASS: 3 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates xtask
git commit -F - <<'EOF'
feat(shell,relay-rt): add the ps command

ps (spec §9.3) lists every process by pid: its parent, what it does in
a word (run, ready, or what it waits for: wait for a child, read the
console, sleep, pipe; zombie once it has ended), the KiB of memory its
address space holds, its CPU time as m:ss, and the path it was started
from, in columns like procps's. The shell's System gains processes,
from proc_list in relay-rt; the host has none, so host-shell's ps says
processes are not available, as its free says of memory figures.
EOF
````


### Task 25: `ps` in `/bin`

Spec §8.4, §9.3 and decision 9: `/bin/ps` runs the shell's `ps` as a program, so `/bin` holds 42 programs. `jobs` sees `ps` list init, the shell and both background jobs (`t-spin` ready or running, as the CPU falls), and `system`'s `ls /bin` and the hardware checklist's startup line follow. The red runs are `system`, whose `ls /bin` lacks `ps`, and `jobs`, where `ps` is not found.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`
- Modify: `tests/e2e/jobs.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/utils/src/bin/ps.rs`

**Interfaces:**
- Consumes: Task 24's `commands::ps`.
- Produces: the program `/bin/ps`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/jobs.txt`**

In `tests/e2e/jobs.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# line that ends with & starts a job in a process group of its own,
# without the console; jobs, wait and kill work on them, and the shell
# says how each ended, in bash's words.
timeout 60
````

with:

````text
# line that ends with & starts a job in a process group of its own,
# without the console; jobs, ps, wait and kill show and end them, and the
# shell says how each ended, in bash's words.
timeout 60
````

Replace:

````text
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# Ctrl-C at the prompt only cancels the line typed: the jobs run on.
````

with:

````text
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# ps shows them both, with init and the shell; t-spin shares the CPU.
send ps
expect \n  PID  PPID STATE      MEM  TIME CMD\n    1     0 wait         0  0:00 init\n    2     1 wait +\d+  0:00 /bin/sh\n
expect \A +\d+     2 sleep +\d+  0:00 /bin/sleep\n +\d+     2 (ready|run) +\d+  0:0\d /bin/t-spin\n +\d+     2 run +\d+  0:00 /bin/ps\n
# Ctrl-C at the prompt only cancels the line typed: the jobs run on.
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-proc   t-spin  tail   uname\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-read   t-sys   touch  wc\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spawn  t-tee   true\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     ps      rm     sh     sync    t-fault  t-pipe  t-spawn  t-tee  true\nclear  df     false  head  mv        pwd     rmdir  sleep  t-abi   t-files  t-proc  t-spin   tail   uname\ncp     dmesg  free   ls    poweroff  reboot  seq    stat   t-args  t-mem    t-read  t-sys    touch  wc\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     pwd     rmdir  sleep  t-abi    t-files  t-proc   t-spin  tail   uname\nclear  df     false  head  mv        reboot  seq    stat   t-args   t-mem    t-read   t-sys   touch  wc\ncp     dmesg  free   ls    poweroff  rm      sh     sync   t-fault  t-pipe   t-spawn  t-tee   true\n
````

with:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     ps      rm     sh     sync    t-fault  t-pipe  t-spawn  t-tee  true\nclear  df     false  head  mv        pwd     rmdir  sleep  t-abi   t-files  t-proc  t-spin   tail   uname\ncp     dmesg  free   ls    poweroff  reboot  seq    stat   t-args  t-mem    t-read  t-sys    touch  wc\n
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: FAIL: scenario `system` stops at line 8, timed out waiting for `\ncat    date   echo   grep  mkdir     ps      rm     sh     sync    t-fault  t-pipe  t-spawn  t-tee  true\nclear  df     false  head  mv        pwd     rmdir  sleep  t-abi   t-files  t-proc  t-spin   tail   uname\ncp     dmesg  free   ls    poweroff  reboot  seq    stat   t-args  t-mem    t-read  t-sys    touch  wc\n`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: FAIL: scenario `jobs` stops at line 15, timed out waiting for `\n  PID  PPID STATE      MEM  TIME CMD\n    1     0 wait         0  0:00 init\n    2     1 wait +\d+  0:00 /bin/sh\n`.

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 41 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 42 programs, ABI 3` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 5: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 41 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 42 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 6: Create `userland/utils/src/bin/ps.rs`**

Create `userland/utils/src/bin/ps.rs`:

````rust
//! `/bin/ps`: list the processes (user-space gate §8.4, §9.3), the shell's
//! command function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("ps", shell::commands::ps, &args)
}
````

- [ ] **Step 7: Run the `system`, `jobs`, `utils` scenarios**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add docs kernel tests userland
git commit -F - <<'EOF'
feat(utils): add ps to /bin

/bin/ps runs the shell's ps as a program (spec §8.4, §9.3), so /bin
holds 42 programs. The jobs scenario sees ps list both background jobs
with init and the shell; the system scenario's ls /bin and the hardware
checklist's startup line follow.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 46 scenario(s) passed`.

````bash
git push -u origin m3p2/utils
gh pr create --base main --head m3p2/utils --title "feat(shell,utils): add ps" --body-file - <<'EOF'
## What

Milestone 3, plan 2, tasks 24–25: `ps` lists every process by pid in procps's columns: its parent, what it does or waits for, the KiB its address space holds, its CPU time and its path; `/bin` holds 42 programs, and the `jobs` scenario sees both background jobs in `ps`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (QEMU's USB keyboard types the Ctrl-C a `wait` needs); plan 4's NUC check 5 runs a background job and `kill` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p2/utils --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p2-utils
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
