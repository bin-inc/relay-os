# Milestone 2 · Plan 4a: The shell and its commands as programs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The shell and every command become programs of their own in `/bin`: a program per command, `cat` to `wc`, each printing exactly what the shell's command prints, and `/bin/sh`, which runs every command but `cd`, `exit` and `help` as a program, redirects into files it opens for them, gives each the console at its prompt, and runs scripts whose transcripts are console tees and which may run scripts themselves. `relay-rt` implements the shell's `Vfs`, `Console` and `System` over system calls, so the command functions do not change. The in-kernel shell stays process 1 and runs them all by path; plan 4b makes `/bin/sh` the shell the machine starts. It ends with `cargo xtask ci` green; it has no NUC check.

**Architecture:** The shell crate gains a `Runner` (`crates/shell/src/runner.rs`): the in-process one is milestone 1's path, the spawning one starts programs through the `Programs` trait, and `run_command` (`program.rs`) runs one command as its program does, with its output on the `Stdout` trait; all of it host-tested against fakes (`testing.rs`). `relay-rt` implements the traits over system calls: `SysVfs` (`sysvfs.rs`) goes by path and names each file by the filesystem and inode `stat` gives (a new `Stat::dev`, which makes the ABI version 2), tested over the file calls of a `MountTable` (`testing.rs`); `SysConsole`, `SysSystem`, `SysStdout` and `SysPrograms` (`sysio.rs`) are thin. The programs are `userland/utils` and `userland/sh`. The kernel changes only in `stat`'s `dev` and `spawn`'s new `FOREGROUND`, which hands the console to a command before it can run.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model), e2fsprogs 1.47, binutils' `readelf` for tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.4, §3.1, §6.4–§6.6, §7.3, §7.4, §8, §11.1, §12, §13 step 4, §16 item 5)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 4a of 7.

## In brief

- **Size.** 16 tasks in three code pull requests, plus this plan as PR 1: PR 2 the shell crate's runners, host-tested; PR 3 `Stat::dev`, `SysVfs` and the 23 programs; PR 4 `FOREGROUND` and `/bin/sh`. Plan 4 is two plans (decision 1): 4b, written after this one merges, makes process 1 the kernel's init, which starts `/bin/sh` and starts it again (decision 2), brings the error screen, drops `shell` from the kernel and ends with NUC check 4.
- **The command functions do not change** (spec §3.1). A program runs its command function through `run_command`, over `SysVfs`, `SysConsole`, `SysSystem` and `SysStdout`; the `SysVfs` tests run every command through the calls and over the mount table and compare what they print (decisions 5 and 6).
- **Two changes to the ABI** (decisions 7 and 9): `Stat` gains `dev`, since a file of `/bin` and one of the root can have the same inode number and the commands tell files apart by it (`cp`, `mv`, `rm -r`, `cat f >> f`), so the ABI is version 2; and `spawn` gains `FOREGROUND`, so that a command has the console before it can read it.
- **`/bin/sh` says `relay-sh`** (decision 4), as the in-kernel shell does: every milestone 1 scenario and check script stays as it is when plan 4b makes it the shell.
- **Scripts are child shells with tees** (decision 10): their transcripts get their programs' output and a nested script's lines; one script may run another.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran throwaway probes and scenarios under KVM on the NUC's CPU model (an i7-1260P), and found no critical, 1 important and 8 minor real defects; seven are fixed in a task of their own after the task they concern, three of them only a stricter test (with the mutation check that shows what it guards), one is a decision and one a note in the spec. It found correct the redirections, names and statuses of `/bin/sh` against the in-kernel shell's (every fd closed over 72 redirections), five-deep scripts and `exit` in a nested one, `FOREGROUND` checked before any work and applied before the child runs with no lock held, `SysVfs`'s identity, `rm -r`'s checks, reads and writes and `Errno`'s round trip, `reboot`/`poweroff` through `power`, and ABI 2 everywhere:

| Finding (review) | Decision |
|---|---|
| Important: every Ctrl-C of a script under `/bin/sh` leaves its command's zombie to the in-kernel shell, which collects nothing while it waits; after about 62, `/bin/sh` cannot start anything | Fixed, Task 14 |
| Minor: an interactive `/bin/sh` that a script starts (not a group leader) reads end of input after its first program and ends without a word | Fixed, Task 15 |
| Minor: the `sh` scenario does not notice `/bin/sh` reading its prompt in line mode (every line echoed twice) | Fixed (a stricter test), Task 16 |
| Minor: no test reads the kernel's `Stat::dev`; a constant 1 passes everything | Fixed (a test), Task 9 |
| Minor: under the in-process runner `exit` in a script stops the interactive shell too | Fixed, Task 7 |
| Minor: a bare `sh` at `/bin/sh` starts an interactive shell (milestone 1 said `sh: missing operand`), and the test of `missing operand` took a path the program never takes | Ruled: as bash does, decision 10; the test is gone |
| Minor: the transcript `/bin/sh` cannot push is not quoted as `sh` quotes one | Fixed, Task 6 |
| Minor: `exit`'s code differs from bash at the edges (blanks, past 64 bits, `--`) | Fixed, Task 2 |
| Minor: the spec's §7.3 does not describe `Stat::dev` | Fixed in the spec (PR 1) |
| Declined to judge: a program's standard output under the in-kernel shell is its output hook, which `fstat` calls a character device (`/bin/ls > f` lays out columns) | Ruled: plan 4b's `/bin/sh` as the shell ends it (roadmap, "What plan 4a leaves for plan 4b") |
| Declined to judge: `SysVfs`'s node table grows by one entry per file named | Ruled: in `/bin/sh` only built-ins and bare redirections name files through it; a program's ends with it |

## Where this plan fits

Plan 4a implements the first half of spec §13 step 4. It builds on plan 3b: every call of spec §7.3 but `proc_list` and `pipe` from ring 3, `relay-rt`'s wrappers and heap, the console's line discipline and calls, and tees (roadmap, "What plan 3b leaves for plan 4"). It leaves plan 4b `/bin/sh` and the programs, ready to be started by the kernel's init, and the kernel's findings carried forward (roadmap, "What plan 4a leaves for plan 4b").

## Working conventions

- Plan 4a lands as **four pull requests** (table below). This plan, with the spec's §16 item 5 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. No PR of this plan needs a NUC check. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-4a-the-shell-and-its-commands-as-programs.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.

  One more comes as a command: a `python3` command that edits a recorded NUC transcript (they hold escape bytes, which a markdown block cannot carry).
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 11 and 13 add programs a scenario runs: their red runs are the scenario's, which cannot find the program, and the packages' manifests come with the programs, since a workspace member without its programs does not build; Tasks 14 and 15 fix what only the machine shows, and their red runs are scenarios too. Two tasks have no failing run, and their introductions say why: Tasks 9 and 16 add stricter tests of what the code already does, with the mutation checks that show what they guard. The mutation checks the prototype ran are named in each task's introduction where a guard could pass vacuously.
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; it happened twice while plan 3b's prototype was made, under a mutant. Mutation checks run under an address-space limit and a timeout (`tmp/m2p4/mutate.py`).
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `282a848` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p4a/plan` | — | The spec's §16 item 5 and the corrections to its body, the roadmap's notes for plans 4a and 4b, this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p4a/shell` | 1–5 | The shell's own commands and its runners; `exit`; a command run as a program; `/bin/sh FILE`'s scripts with tees | `lint`, `unit`, `e2e` |
| 3 | `m2p4a/utils` | 8–11 | `Stat::dev` and ABI 2; `SysVfs`; `SysConsole`, `SysSystem`, `SysStdout`; one program per command | `lint`, `unit`, `e2e` |
| 4 | `m2p4a/sh` | 12–13 | `spawn`'s `FOREGROUND`; `SysPrograms`; `/bin/sh` | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 4a adds no third-party crate (`relay-rt` gains the workspace's `shell` and `vfs`). The kernel has no dev-dependencies. New host crates are `#![cfg_attr(not(test), no_std)]` and go in both `members` and `default-members`; user packages (`relay-utils`, `relay-sh`) go in `members` only and in `USER_PACKAGES`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- The command functions in `crates/shell` do not change to become programs (gate §3.1); every command prints exactly what it prints in milestone 1.
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. Nothing waits for ever. Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils and bash; the in-kernel shell and `/bin/sh` name themselves `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 5 in PR 1:

1. **Plan 4 is two plans** (§13). 4a brings the shell and its commands as programs: the `Runner` split, `relay-rt`'s traits, `/bin/sh` and a program per command, which the in-kernel shell, still process 1, runs by path. 4b brings process 1, the error screen, the kernel without `shell`, `check4.sh` and NUC check 4; it is written after 4a merges. 4a has no NUC check: of what the NUC runs it changes only the count of `/bin` and the ABI version in check 3's startup lines, which the check scripts' patterns allow; the recorded transcripts get `ABI 2` by hand until 4b's NUC check records real ones.
2. **Process 1** (§5.4, §6.6, corrected in its body; built by 4b). Process 1 stays a process of the kernel's own, "init", as the in-kernel shell is now: it prints `/etc/motd`, starts `/bin/sh` in `/root` (or `/` without one) as its child, and when the shell ends logs how and starts another; three ends within 10 s, or a shell that cannot be started, reach the error screen. It collects every orphan as it ends (plan 3a's deferred finding), and `kill` still refuses it. So `/bin/sh` is not process 1 and collects no orphans.
3. **The shell's own commands and the runners** (§8.2, §8.3). The shell runs `cd`, `exit` and `help` itself, over its `Vfs`; every other name goes to its runner. The in-process runner runs the command table's functions, and any other name through `System::spawn` (the tests, `host-shell`, and the in-kernel shell until 4b). The spawning runner (`/bin/sh`) starts `/bin/<name>`, or the path as given, through the `shell::Programs` trait: it opens a redirection in the shell (created, emptied or appended to) and passes it as fd 1, closing its own copy once the child has it, and reports what cannot start, a killed program and a Ctrl-C as the in-kernel shell does. `exit [code]` stops the shell with the code modulo 256, or the last status, as bash does: the code may have blanks around it and must fit in 64 bits (otherwise it is status 2 and the shell stops anyway), `--` before it is dropped, and more than one argument stops nothing. In a script `exit` ends only the script, under either runner. `Shell::greet` (the motd, `/root`) is apart from `Shell::run`.
4. **Messages** (§8.2, §11.1, corrected in their bodies). `/bin/sh` names itself `relay-sh`, as the in-kernel shell does, so every milestone 1 scenario and check script stays as it is (§1.4); `sh`'s own messages keep `sh:`, as in milestone 1.
5. **A command as a program** (§8.1, §8.4). `shell::run_command` runs a command function with its standard output on the program's fd 1 (`shell::Stdout`), written at once to the console, so that it keeps its place among the errors on fd 2, and in pieces of 4 KiB to a file; a write error is `<name>: write error: <message>`, status 1, as the shell said it in milestone 1. Each program names its own command function, so it holds no other command's code. With the `user` profile (debug assertions and overflow checks on) the programs are 71 to 133 KiB, `/bin/sh` 268 KiB, `system.img` about 2.7 MiB; the 24 build in about 10 s. `opt-level = "s"` would save a quarter and is not used.
6. **`SysVfs` goes by path** (§8.1). A `Node` is the file's filesystem and inode, as `stat` gives them; `SysVfs` remembers the path it found each node by and opens that path for each operation, so a command holds no fd between calls and always reaches the file the path names now. `read_dir`'s records give the entries' kinds. `SysVfs::shutdown` does nothing: the kernel's `power` shuts the filesystems down.
7. **`Stat::dev` and ABI 2** (§7.3, §7.4, corrected in its body). `relay_abi::Stat` gains `dev` (offset 72, 80 bytes): the mount's number from 1, and 0 for the console. With `ino` it names a file, so that `cp`, `mv`, `rm -r` and `cat` tell a file of `/bin` from one of the root, whose inode numbers overlap, as the in-kernel shell does. The changed layout makes `relay_abi::VERSION` 2: `system: 33 programs, ABI 2`, `ABI 99, kernel wants 2`.
8. **`SysConsole`, `SysSystem`, `SysStdout`, `SysPrograms`** (§8.1). `SysConsole` reads fd 0 and writes fd 2 (a program's errors, and a shell's prompt and messages, as bash writes them); an interactive shell's takes the console back, raw and for its own group, before every read, whatever a program left it in (§16 item 4). An interactive shell that does not lead a group (one a script starts, in the script's group) takes back only raw mode and runs its commands in that group, without `FOREGROUND`, so the console never leaves it (the prototype's review found such a shell reading end of input after its first command). `Console::interrupted` is false, since Ctrl-C kills. `SysSystem` has `time`, `sys_info`'s memory figures and log, and `power`: `System::reboot` and `System::poweroff` take `-f` (`POWER_FORCE`) and return the error that kept the machine up, from which `reboot` and `poweroff` print milestone 1's two lines. `SysStdout` is fd 1, the console when `fstat` says a character device. `SysPrograms` is `/bin/sh`'s `Programs` over `open`, `spawn`, `wait` and the tee calls.
9. **The console at the prompt** (§6.4, §7.3). `spawn` gains the flag `FOREGROUND` (with `NEW_GROUP`, `EINVAL` otherwise): the child's new group gets the console, in line mode, before the child runs. Without it the scheduler could run a child that reads the console before its parent handed it over, and the child would get end of input. A script's commands run in the script shell's group, without it.
10. **Scripts** (§6.5, §8.3). `sh FILE` in `/bin/sh` is a child `/bin/sh`, which checks and reads the script as `sh` does in milestone 1 (its operands, 64 KiB, UTF-8, a byte-order mark and CRLF, output not redirected, the transcript emptied or created), pushes the transcript as a console tee, traces, runs and syncs each line, and pops the tee. So the transcript gets the output of the script's programs and of any script it runs, whose own transcript is pushed on top, four deep at most (a fifth is `sh: cannot write the transcript …: Device or resource busy`). A write that failed is reported when the script ends: `sh: <log>: <error>; the transcript ends here`. `sh` without an operand starts an interactive shell, as bash's does (milestone 1's built-in said `sh: missing operand`). A script's `cd` stays in it; Ctrl-C ends the script with its command, which share its group, so its transcript has no `^C`. The in-process runner keeps milestone 1's scripts, which cannot run scripts.
11. **The in-kernel shell until 4b** (§16 item 3). It greets once, and reads on at a new prompt after `exit`. While it waits for a command it collects every orphan that ends (plan 3a's deferred finding, for the in-kernel shell): the prototype's review found that under `/bin/sh` each script stopped by Ctrl-C left its command's zombie to process 1, and that 62 of them filled the process table.
12. **Milestone 1's deferred findings** (plan 4's). The re-exports of `commands/mod.rs` come after its module list. A command's big output is no longer held whole under `/bin/sh`, whose transcripts are tees written every 4 KiB; the in-process runner keeps its transcript, and after 4b only the host uses it. A script that redirects into its own transcript garbles it, as it would under bash: ruled, and `sh`'s documentation says so.
13. **Left to 4b.** The kernel's findings carried forward (polling the console on the way back to ring 3, `console_foreground` of a group of zombies, `open(READ|CREATE)` of a directory, `POWER_FORCE` and `POWER_REBOOT` end to end, a host test of `push_tee`'s checks) come with 4b's kernel work. The console calls refusing a caller outside the foreground group wait for milestone 3's background jobs: no process of milestone 2 runs in the background, and a rule that lets every shell, nested ones too, take the console back needs job control.
14. **Error numbers** (§7.2). `vfs::Errno::ALL` lists them, and `Errno::from_number` turns a system call's number back into one.
15. **Tests** (§12). Scenarios `utils` (every program by path under the in-kernel shell, whose output hook is never a file) and `sh` (`/bin/sh` under the in-kernel shell: redirections into files, the console at once, Ctrl-C, nested scripts and their transcripts, `exit`); `system` lists the new `/bin`. Host tests: the runners against `FakePrograms`, `run_command` against `FakeStdout`, and `SysVfs` against the file calls over a `MountTable`, where every command prints what it prints over the table itself.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **What a person types at `/bin/sh`:** a redirection into a directory, a read-only file of `/bin`, a missing directory, or alone (`> file`); a bare name that is a directory in `/bin` (`..`), one too long for a file name, a path to a missing file or to a file that is no program; a program that faults or is stopped by Ctrl-C; `exit` with no code, a code past 255, a negative one, one with blanks around it, one past 64 bits, one that is not a number, `--`, two codes, or in a script. Expected: the in-kernel shell's messages and statuses (`relay-sh: …`), nothing started when the redirection cannot be opened, the redirection's fd closed in the shell either way, and `exit`'s code and stop as bash has them (in a script, only the script stops). Tests: `every_command_but_cd_exit_and_help_is_a_program`, `a_path_runs_as_given`, `a_redirection_is_opened_in_the_shell_and_passed_as_fd_1`, `a_redirection_that_cannot_open_starts_nothing`, `what_cannot_start_says_why`, `a_killed_program_is_reported_and_ctrl_c_is_only_so` (Task 3), `exit_stops_the_shell_with_its_code`, `exit_without_a_code_keeps_the_last_status`, `exit_errors` (Tasks 1 and 2), `exit_ends_the_script_not_the_shell` (Task 7), `nosuch`, `t-fault null-read` and `exit 3` in the `sh` scenario (Task 13).
2. **Files named across filesystems and paths:** `cp` from `/bin` onto a file of the root whose inode number is the same, one file by two paths, `rm -r /`, `rm` of a file of `/bin`, `cat f >> f` with a real file as standard output, a directory too big for one `read_dir`, a kernel whose `read_dir` never ends, a `Node` `SysVfs` never gave. Expected: what the command prints over the mount table in the in-kernel shell, line for line; `EIO` rather than a listing that grows for ever; `ENOENT` for a stranger node; no fd left open after a command. Tests: `commands_do_through_the_calls_what_they_do_over_the_mount_table`, `a_file_is_one_node_whatever_its_path`, `files_of_two_filesystems_are_never_one_node`, `a_command_holds_no_fd_between_calls`, `a_big_directory_is_listed_over_many_calls`, `a_directory_that_never_ends_is_eio`, `errors_are_the_calls_own` (Task 10), `the_layouts_are_fixed` and `a_struct_s_bytes_are_its_memory` (Task 8), `stat_and_fstat_name_the_filesystem` (Task 9), `a_file_is_not_its_own_output` (Task 4), `cat /root/list >> /root/list` in the `sh` scenario (Task 13), `/bin/rm /bin/t-args` and `/bin/rm -r /` in the `utils` scenario (Task 11).
3. **The console between `/bin/sh` and its commands:** a command that reads the console the moment it starts, Ctrl-C of a program and of a script, Ctrl-C at the prompt, typing at the prompt after a command left the console in line mode, an interactive shell a script starts (in the script's group), `FOREGROUND` without `NEW_GROUP` or with an unknown flag, `exit` back to the in-kernel shell. Expected: the command gets what is typed (never end of input for want of the console), `^C` and a fresh prompt, a prompt that always reads, `EINVAL` for a bad flag, and the in-kernel shell's prompt again after `exit`. Tests: `spawn_copies_in_everything_its_struct_names`, `spawn_refuses_what_is_not_a_valid_request` (Task 12), `t-read`, `t-spin` with Ctrl-C, `sh /root/spin.sh` with Ctrl-C and `exit 3` in the `sh` scenario (Task 13), `sh /root/i.sh` in it (Task 15), `echo once` and Ctrl-C at the prompt in it (Task 16).
4. **Scripts under `/bin/sh`:** a script that runs another, five deep, a transcript that cannot be pushed, a tee write that fails, a script whose output is redirected, a missing operand, Ctrl-C in a script (and in 60 of them), a script's `cd`, `exit` in a script. Expected: every line traced, run in the script shell's group and synced; the nested script's lines in both transcripts; `sh: cannot write the transcript …` with nothing run; `sh: <log>: <error>; the transcript ends here` when the script ends; milestone 1's `sh` messages otherwise; Ctrl-C ending the script with its command. Tests: `bin_sh_runs_a_script_s_commands_in_its_own_group_with_its_transcript_a_tee`, `a_script_run_by_bin_sh_may_run_another`, `a_transcript_that_failed_is_reported_when_the_script_ends`, `a_transcript_that_cannot_be_pushed_runs_nothing`, `bin_sh_keeps_sh_s_rules` (Task 5), the quoted name in `a_transcript_that_cannot_be_pushed_runs_nothing` (Task 6), `sh /root/s.sh` and `sh /root/spin.sh` in the `sh` scenario (Task 13), `t-spawn fill` twice under `/bin/sh` in it (Task 14).
5. **Output a program cannot write, and a machine that stays up:** a full disk under a program's redirection, a write error on the console, big output into a file, a program's errors among its output on the screen, `reboot` or `poweroff` whose filesystems cannot be shut down, with and without `-f`. Expected: `<name>: write error: <message>` and status 1, output to a file in pieces of 4 KiB and never held whole, errors in their place among the output, milestone 1's two lines and status 1 when the machine stays up, and `-f` going ahead. Tests: `output_goes_to_stdout_and_errors_to_the_console`, `output_to_a_file_is_written_in_pieces_of_4_kib`, `ls_lays_out_columns_only_on_the_console`, `a_write_error_is_reported_as_the_shell_did`, `reboot_and_poweroff_say_why_the_machine_stayed_up` (Task 4), `/bin/cat /root/a/f /root/nope /root/a/f` and `poweroff /bin/poweroff` in the `utils` scenario (Task 11).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/shell/src/{runner,program,shell,ctx,io,lib}.rs` | The runners; a command run as a program; the shell's own commands and scripts; a program's output; `Programs` and `Stdout` |
| `crates/shell/src/commands/{mod,basic,system,script}.rs`, `crates/shell/src/transcript.rs` | `exit`, the shell's own commands, the command functions re-exported; `reboot`/`poweroff` with `power`'s error; `sh`'s documentation; the transcript's message |
| `crates/shell/src/testing.rs` | `FakePrograms`, `FakeStdout`, the harness's spawning shell, program and `/bin/sh` |
| `crates/relay-abi/src/{file,spawn,lib}.rs`, `crates/vfs/src/errno.rs` | `Stat::dev`; `FOREGROUND`; ABI 2; `Errno::ALL`, `from_number` |
| `crates/relay-rt/src/{sysvfs,sysio,testing,lib}.rs` | `SysVfs`; `SysConsole`, `SysSystem`, `SysStdout`, `SysPrograms`; the file calls over a `MountTable` for tests |
| `kernel/src/{file,system,syscall,proc,session}.rs`, `kernel/src/syscall/files.rs` | `stat`'s `dev`; ABI 2; `FOREGROUND`; the in-kernel shell greets once and runs again after `exit` |
| `userland/utils/`, `userland/sh/`, `Cargo.toml`, `xtask/src/{config,host_shell}.rs` | The programs of `/bin`; the user packages; `host-shell` greets and ends at `exit` |
| `tests/e2e/{utils,sh,system,boot,system_abi}.txt`, `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/*.log` | The new scenarios; the new `/bin`; ABI 2 |
| `README.md`, `docs/hardware-test.md` | What milestone 2 does now; `system: 33 programs, ABI 2` |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 5 (with the parts of its body it corrects: §4.3's startup line, §6.6's process 1, §7.3's `spawn` flags, §7.4's version, §8.2's and §11.1's messages, §12.3's scenarios, §13's step 4), the roadmap's rows and notes for plans 4a and 4b ("What plan 4a leaves for plan 4b"), and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 4a, and the roadmap's notes for plan 4`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4a/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p4/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-4a-the-shell-and-its-commands-as-programs.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-30-m2-plan-4a-the-shell-and-its-commands-as-programs.md
git commit -m "docs: plan 4a of milestone 2, the shell and its commands as programs"
cargo xtask lint
git push -u origin m2p4a/plan
gh pr create --base main --head m2p4a/plan --title "Milestone 2, plan 4a: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 4a ("the shell and its commands as programs"), the spec's §16 item 5 with its decisions (among them: plan 4 is two plans, 4a and 4b; process 1 becomes the kernel's init, which starts `/bin/sh` and starts it again; `/bin/sh` names itself `relay-sh`; `Stat` gains `dev` and the ABI is version 2; `spawn` gains `FOREGROUND`; scripts are child shells whose transcripts are console tees), the corrections to the spec's body they bring, and the roadmap's rows and notes for plans 4a and 4b.

## How it was tested

- [x] Every task of plan 4a was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p4a/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p4a/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-plan` and continue with PR 2.

---

## PR 2: The shell's runners (Tasks 1–7)

The shell crate, host-tested: `cd`, `exit` and `help` are the shell's own, and a `Runner` runs the rest, in-process as before (the tests, `host-shell`, the in-kernel shell) or as programs (`/bin/sh`, through `Programs`); a command runs as a program with its output on `Stdout`; `/bin/sh FILE` runs scripts with their transcripts as tees. Nothing runs these new paths on the machine yet.

Branch `m2p4a/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4a/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4a/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4a/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-shell m2p4a/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4a/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: The shell's own commands are `cd`, `exit` and `help`; a `Runner` runs the rest

Spec §8.2 and decision 3: the shell crate gains `runner.rs`, with the `Runner` trait and its first implementation, `InProcess`, which is milestone 1's path unchanged: the command table's functions run against the shell's `Vfs`, `Console` and `System` (their redirection opened as before), and any other name through `System::spawn`, as the in-kernel shell reaches programs. `Shell::execute` runs only `cd`, `exit` and `help` itself (`commands::builtin`, over `commands::BUILTINS`) and hands every other name to its runner, which says how the command went (`Ran`: its status, the shell's message after it, whether the shell stops, a script `sh` read); the redirection code, the message of what cannot start (`cannot_start`) and of how a program ended (`ended`) move there, to be shared with the spawning runner of Task 3. The shell holds its runner in an enum (`Runners`), not a box, since a box's destructor would keep what the shell borrows until it is dropped. `exit [code]` is new (decision 3): the code modulo 256, or the last status (`Ctx::status`); not a number is status 2 and stops the shell anyway, two arguments stop nothing, as bash does. `Shell::greet` (the motd, `/root`) is apart from `Shell::run`, so that `/bin/sh` (Task 13) does not print the motd; the in-kernel shell greets once and runs again after an `exit`, and `host-shell` greets, runs and ends at `exit`. The re-exports of `commands/mod.rs` move after its module list (milestone 1's deferred finding). Mutation checks: `builtin` finding the whole table, `exit` not stopping the shell and a negative code not taken modulo 256 each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/system.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/lib.rs`
- Create: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `kernel/src/session.rs`
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: milestone 1's `Shell`, `Ctx`, `commands::COMMANDS`; plan 2's `System::spawn`/`wait`.
- Produces: `commands::{BUILTINS, builtin(&str) -> Option<&'static Builtin>}`; the `exit` command; `Ctx::status` (crate-private); `Shell::greet(&mut self)`; `runner::{Parts {vfs, console, system, transcript, in_script, status}, Ran {status, message, stop, script}, Ran::said(i32, String), Runner::run(&mut self, Parts<'_>, name: &str, args: &[String], redirect: Option<&Redirect>) -> Ran, InProcess, Runners, redirect_to(&mut dyn Vfs, Option<&Redirect>) -> Result<Option<(Node, u64)>, Ran>, run_function(Parts<'_>, &Builtin, &[String], Option<(Node, u64)>) -> Ran, cannot_start(&str, Errno) -> Ran, not_found(&str) -> Ran, program_path(&str) -> String, ended(&str, &WaitStatus) -> Ran}` (all crate-private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    #[test]
    fn pwd_ignores_arguments() {
````

with:

````rust
    #[test]
    fn exit_stops_the_shell_with_its_code() {
        let mut h = Harness::new();
        h.console.type_in(b"exit 3\recho never\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 3);
        assert_eq!(h.console.take(), "root@relay:/# exit 3\n");
        // Modulo 256, as in bash, and a sign may come first.
        for (code, status) in [
            ("256", 0),
            ("257", 1),
            ("-1", 255),
            ("+7", 7),
            ("1000", 232),
        ] {
            let line = alloc::format!("exit {code}");
            assert_eq!(h.run(&line), (status, "".into()), "{code}");
        }
    }

    #[test]
    fn exit_without_a_code_keeps_the_last_status() {
        let mut h = Harness::new();
        h.console.type_in(b"nope\rexit\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 127);
    }

    #[test]
    fn exit_errors() {
        let mut h = Harness::new();
        // Not a number: status 2, and the shell stops anyway.
        h.console.type_in(b"exit abc\recho never\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 2);
        assert_eq!(
            h.console.take(),
            "root@relay:/# exit abc\nrelay-sh: exit: abc: numeric argument required\n"
        );
        // Too many: nothing stops.
        h.console.type_in(b"exit 1 2\recho still\r");
        crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert!(
            h.console
                .take()
                .contains("relay-sh: exit: too many arguments\nroot@relay:/# echo still\nstill\n")
        );
    }

    #[test]
    fn pwd_ignores_arguments() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
    }
}
````

with:

````rust
    }

    #[test]
    fn the_shell_s_own_commands_are_cd_exit_and_help() {
        let own: alloc::vec::Vec<_> = COMMANDS
            .iter()
            .filter(|b| builtin(b.name).is_some())
            .map(|b| b.name)
            .collect();
        assert_eq!(own, ["cd", "exit", "help"]);
        assert!(builtin("cat").is_none() && builtin("sh").is_none());
    }
}
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, replace:

````rust
        assert_eq!(h.system.reboots, 1);
        assert_eq!(
            h.console.take(),
            "Welcome to Relay OS.\nroot@relay:~# reboot\n"
        );
        assert_eq!(h.run("poweroff"), (0, "".into()));
````

with:

````rust
        assert_eq!(h.system.reboots, 1);
        assert_eq!(h.console.take(), "root@relay:/# reboot\n");
        assert_eq!(h.run("poweroff"), (0, "".into()));
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn run_greets_goes_home_and_reads_lines_until_input_ends() {
        let mut h = Harness::new();
````

with:

````rust
    #[test]
    fn greet_shows_the_motd_and_goes_home() {
        let mut h = Harness::new();
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).greet();
        assert_eq!(h.console.take(), "Welcome to Relay OS.\n");
        assert_eq!(h.run("pwd").1, "/root\n");
    }

    #[test]
    fn run_reads_lines_until_input_ends() {
        let mut h = Harness::new();
````

Replace:

````rust
            h.console.text(),
            "Welcome to Relay OS.\nroot@relay:~# echo hi\nhi\nroot@relay:~# pwd\n/root\nroot@relay:~# "
        );
````

with:

````rust
            h.console.text(),
            "root@relay:/# echo hi\nhi\nroot@relay:/# pwd\n/\nroot@relay:/# "
        );
````

Replace:

````rust
                .text()
                .contains("echo no^C\nroot@relay:~# pwd\n/root\n")
        );
````

with:

````rust
                .text()
                .contains("echo no^C\nroot@relay:/# pwd\n/\n")
        );
````

Replace:

````rust
        h.console.type_in(b"pwd\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(h.console.text(), "root@relay:/# pwd\n/\nroot@relay:/# ");
````

with:

````rust
        h.console.type_in(b"pwd\r");
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.greet();
        shell.run();
        assert_eq!(h.console.text(), "root@relay:/# pwd\n/\nroot@relay:/# ");
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `builtin` in this scope ``.

- [ ] **Step 6: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! `cd`, `pwd`, `echo`, `clear`, `help` and `uname`.

````

with:

````rust
//! `cd`, `exit`, `pwd`, `echo`, `clear`, `help` and `uname`.

````

Replace:

````rust
        Err(e) => ctx.fail(NAME, format_args!("cd: {dir}: {e}")),
    }
}

````

with:

````rust
        Err(e) => ctx.fail(NAME, format_args!("cd: {dir}: {e}")),
    }
}

/// `exit [code]`: the shell stops with `code` (modulo 256), or the last
/// command's status. As in bash, a code that is not a number is status 2
/// and the shell still stops; more than one argument stops nothing.
pub fn exit(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let status = match args {
        [] => ctx.status,
        [code] => match parse_code(code) {
            Some(n) => n,
            None => {
                ctx.fail(
                    NAME,
                    format_args!("exit: {code}: numeric argument required"),
                );
                2
            }
        },
        _ => return ctx.fail(NAME, format_args!("exit: too many arguments")),
    };
    ctx.exit = true;
    status
}

/// A decimal number with an optional sign, modulo 256.
fn parse_code(code: &str) -> Option<i32> {
    let (negative, digits) = match code.as_bytes().first() {
        Some(b'-') => (true, &code[1..]),
        Some(b'+') => (false, &code[1..]),
        _ => (false, code),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Reduced as it goes, so any number of digits fits.
    let n = digits
        .bytes()
        .fold(0u32, |n, d| (n * 10 + u32::from(d - b'0')) % 256);
    Some(if negative {
        ((256 - n) % 256) as i32
    } else {
        n as i32
    })
}

````

- [ ] **Step 7: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod stat;

pub(crate) use script::Script;
pub use script::transcript_name;
mod system;
mod text;

````

with:

````rust
mod stat;
mod system;
mod text;

pub(crate) use script::Script;
pub use script::transcript_name;

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
        name: "exit",
        help: "leave the shell",
        run: basic::exit,
    },
````

Replace:

````rust

pub fn find(name: &str) -> Option<&'static Builtin> {
    COMMANDS.iter().find(|b| b.name == name)
}
````

with:

````rust

/// The shell's own commands (user-space gate §8.3); every other one is a
/// program of its own in `/bin`.
pub const BUILTINS: &[&str] = &["cd", "exit", "help"];

pub fn find(name: &str) -> Option<&'static Builtin> {
    COMMANDS.iter().find(|b| b.name == name)
}

/// One of the shell's own commands.
pub fn builtin(name: &str) -> Option<&'static Builtin> {
    find(name).filter(|b| BUILTINS.contains(&b.name))
}
````

- [ ] **Step 8: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    out: Output,
    /// Set by `reboot` and `poweroff` when the machine did not go away.
    pub(crate) exit: bool,
````

with:

````rust
    out: Output,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
    /// not go away: the shell stops.
    pub(crate) exit: bool,
````

Replace:

````rust
    pub(crate) transcript: Option<Transcript>,
}
````

with:

````rust
    pub(crate) transcript: Option<Transcript>,
    /// The last command's exit status, for `exit`.
    pub(crate) status: i32,
}
````

Replace:

````rust
            transcript: None,
        }
````

with:

````rust
            transcript: None,
            status: 0,
        }
````

- [ ] **Step 9: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub mod parser;
mod shell;
````

with:

````rust
pub mod parser;
mod runner;
mod shell;
````

- [ ] **Step 10: Create `crates/shell/src/runner.rs`**

Create `crates/shell/src/runner.rs`:

````rust
//! How the shell runs a command that is not one of its own built-ins
//! (user-space gate §8.2): the `Runner`. The in-process runner runs the
//! command functions against the shell's `Vfs`, `Console` and `System`, as
//! milestone 1 does (the unit tests, `cargo xtask host-shell` and the
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, System};
use crate::killed;
use crate::parser::Redirect;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND};
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs};

/// What a runner works with: the shell's surroundings and its script.
pub(crate) struct Parts<'s> {
    pub vfs: &'s mut dyn Vfs,
    pub console: &'s mut dyn Console,
    pub system: &'s mut dyn System,
    /// A running script's transcript, which gets what the screen gets.
    pub transcript: &'s mut Option<Transcript>,
    /// The command is a line of a script.
    pub in_script: bool,
    /// The last command's exit status, for `exit`.
    pub status: i32,
}

/// How a command went.
pub(crate) struct Ran {
    pub status: i32,
    /// What the shell says after it: a write error, `^C`, how a program
    /// was killed.
    pub message: String,
    /// `exit`, or `reboot`/`poweroff` returning: the shell stops.
    pub stop: bool,
    /// Set by `sh`: the script the shell runs next.
    pub script: Option<Script>,
}

impl Ran {
    pub fn said(status: i32, message: String) -> Ran {
        Ran {
            status,
            message,
            stop: false,
            script: None,
        }
    }
}

/// Runs the commands that are not the shell's own (`cd`, `exit`, `help`).
pub(crate) trait Runner {
    /// Runs `name` with `args`, its standard output going to `redirect`
    /// if there is one.
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran;
}

/// The runners a shell may have. (A shell holds its runner in this rather
/// than in a box, so that it holds no destructor that would keep what it
/// borrows until it is dropped.)
pub(crate) enum Runners {
    InProcess(InProcess),
}

impl Runners {
    pub fn get(&mut self) -> &mut dyn Runner {
        match self {
            Runners::InProcess(r) => r,
        }
    }
}

/// The command functions of `commands::COMMANDS`, run in the shell's
/// process; any other name is a program, through `System::spawn`.
pub(crate) struct InProcess;

impl Runner for InProcess {
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let file = match redirect_to(&mut *parts.vfs, redirect) {
            Ok(file) => file,
            Err(ran) => return ran,
        };
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, file),
            None => run_program(parts, name, args, file),
        }
    }
}

/// The redirection target, opened; a target that cannot be opened stops
/// the command.
pub(crate) fn redirect_to(
    vfs: &mut dyn Vfs,
    redirect: Option<&Redirect>,
) -> Result<Option<(Node, u64)>, Ran> {
    match redirect {
        Some(r) => match open_redirect(vfs, r) {
            Ok(file) => Ok(Some(file)),
            Err(e) => Err(Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
        },
        None => Ok(None),
    }
}

/// Opens a redirection target: created if missing, emptied for `>`,
/// written at its end for `>>`.
fn open_redirect(vfs: &mut dyn Vfs, r: &Redirect) -> Result<(Node, u64), Errno> {
    let path = r.path.as_bytes();
    let node = match vfs.lookup(path) {
        Ok(node) => {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            if !r.append {
                vfs.truncate(node, 0)?;
            }
            node
        }
        Err(Errno::ENOENT) => vfs.create(path)?,
        Err(e) => return Err(e),
    };
    let offset = if r.append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
}

/// Runs a command function with its standard output going to `file`, if
/// any.
pub(crate) fn run_function(
    parts: Parts<'_>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.in_script = parts.in_script;
    ctx.status = parts.status;
    ctx.transcript = parts.transcript.take();
    let mut status = (command.run)(&mut ctx, args);
    let mut message = String::new();
    if let Err(e) = ctx.finish() {
        message = format!("{}: write error: {e}\n", command.name);
        status = 1;
    }
    if ctx.cancelled {
        message = String::from("^C\n");
        status = CANCELLED;
    }
    *parts.transcript = ctx.transcript.take();
    Ran {
        status,
        message,
        stop: ctx.exit,
        script: ctx.script.take(),
    }
}

/// The status and message of a command that could not be started (user-
/// space gate §8.2): a bare name that names no program in `/bin` is not
/// found, as in bash; a path says why.
pub(crate) fn cannot_start(name: &str, e: Errno) -> Ran {
    // `..`, `.` and `''` name directories in /bin, which a search for a
    // command skips, as bash's does; a name too long for a file name is
    // no file in /bin either.
    if !name.contains('/') && matches!(e, Errno::ENOENT | Errno::EISDIR | Errno::ENAMETOOLONG) {
        return not_found(name);
    }
    let status = if e == Errno::ENOENT {
        NOT_FOUND
    } else {
        CANNOT_RUN
    };
    Ran::said(status, format!("{NAME}: {name}: {e}\n"))
}

pub(crate) fn not_found(name: &str) -> Ran {
    Ran::said(NOT_FOUND, format!("{NAME}: {name}: command not found\n"))
}

/// The program's path: `/bin/<name>`, or `name` itself when it holds a
/// `/`.
pub(crate) fn program_path(name: &str) -> String {
    if name.contains('/') {
        String::from(name)
    } else {
        format!("/bin/{name}")
    }
}

/// The status of a program that ended, and what the shell says if it did
/// not exit by itself: Ctrl-C says only `^C`, as for a built-in, and stops
/// a script (spec §6.4).
pub(crate) fn ended(name: &str, w: &relay_abi::WaitStatus) -> Ran {
    if w.how == relay_abi::wait::EXITED {
        return Ran::said(w.code as i32, String::new());
    }
    let (what, status) = killed::killed(w);
    let message = if status == CANCELLED {
        String::from("^C\n")
    } else {
        format!("{NAME}: {name}: {what}\n")
    };
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

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
use crate::commands::{self, Script};
use crate::ctx::Ctx;
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, System};
use crate::killed;
use crate::parser::{self, HOME, Redirect};
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs, path};

````

with:

````rust
use crate::commands::{self, Script};
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, System};
use crate::parser::{self, HOME};
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::Transcript;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Vfs, path};

````

Replace:

````rust
    system: &'a mut dyn System,
    editor: LineEditor,
````

with:

````rust
    system: &'a mut dyn System,
    runner: Runners,
    editor: LineEditor,
````

Replace:

````rust
impl<'a> Shell<'a> {
    pub fn new(
````

with:

````rust
impl<'a> Shell<'a> {
    /// A shell that runs every command in its own process (the in-process
    /// runner, user-space gate §8.2).
    pub fn new(
````

Replace:

````rust
    ) -> Shell<'a> {
        Shell {
            vfs,
            console,
            system,
            editor: LineEditor::new(),
````

with:

````rust
    ) -> Shell<'a> {
        Shell::with_runner(vfs, console, system, Runners::InProcess(runner::InProcess))
    }

    fn with_runner(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        runner: Runners,
    ) -> Shell<'a> {
        Shell {
            vfs,
            console,
            system,
            runner,
            editor: LineEditor::new(),
````

Replace:

````rust

    /// Shows `/etc/motd`, goes to `/root`, then reads and runs commands
    /// until the input ends or `reboot`/`poweroff` return.
    pub fn run(&mut self) {
        self.greet();
        while !self.stopped {
````

with:

````rust

    /// Reads and runs commands until the input ends, `exit` or
    /// `reboot`/`poweroff` return.
    pub fn run(&mut self) {
        self.stopped = false;
        while !self.stopped {
````

Replace:

````rust

    fn greet(&mut self) {
        if let Ok(node) = self.vfs.lookup(b"/etc/motd") {
````

with:

````rust

    /// Shows `/etc/motd` and goes to `/root`, as the shell does when the
    /// machine starts.
    pub fn greet(&mut self) {
        if let Ok(node) = self.vfs.lookup(b"/etc/motd") {
````

Replace:

````rust
        }
        let file = match &cmd.redirect {
            Some(r) => match self.open_redirect(r) {
                Ok(file) => Some(file),
                Err(e) => return self.finish(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
            None => None,
        };
        let Some(name) = cmd.words.first() else {
            // A bare `> file` just creates or empties the file.
            return self.finish(0, String::new());
        };
        let Some(builtin) = commands::find(name) else {
            return self.run_program(name, &cmd.words[1..], file);
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.in_script = self.in_script;
        ctx.transcript = self.transcript.take();
        let mut status = (builtin.run)(&mut ctx, &cmd.words[1..]);
        let mut message = String::new();
        if let Err(e) = ctx.finish() {
            message = format!("{name}: write error: {e}\n");
            status = 1;
        }
        if ctx.cancelled {
            message = String::from("^C\n");
            status = CANCELLED;
        }
        self.transcript = ctx.transcript.take();
        self.stopped = ctx.exit;
        if let Some(script) = ctx.script.take() {
            status = self.run_script(script);
        }
        self.finish(status, message)
    }

    /// Runs a program (user-space gate §8.2): `/bin/<name>`, or `name`
    /// itself when it holds a `/`, with `name` as argument 0 and the words
    /// after it as the others, as bash does. Its fd 1 is standard output
    /// (the redirection file, if any), its fd 2 the screen; a running
    /// script's transcript gets both.
    fn run_program(&mut self, name: &str, words: &[String], file: Option<(Node, u64)>) -> i32 {
        let path = if name.contains('/') {
            String::from(name)
        } else {
            format!("/bin/{name}")
        };
        let mut args: Vec<&[u8]> = alloc::vec![name.as_bytes()];
        args.extend(words.iter().map(|w| w.as_bytes()));
        let started = self.system.spawn(&mut *self.vfs, path.as_bytes(), &args);
        let pid = match started {
            Some(Ok(pid)) => pid,
            // No programs here (the host), or none by that name in /bin:
            // `..`, `.` and `''` name directories there, which a search
            // for a command skips, as bash's does.
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            // A name too long for a file name is no file in /bin either.
            Some(Err(Errno::ENOENT | Errno::EISDIR | Errno::ENAMETOOLONG))
                if !name.contains('/') =>
            {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
            }
            Some(Err(e)) => {
                let status = if e == Errno::ENOENT {
                    NOT_FOUND
                } else {
                    CANNOT_RUN
                };
                return self.finish(status, format!("{NAME}: {name}: {e}\n"));
            }
        };
        let mut ctx = Ctx::new(&mut *self.vfs, &mut *self.system, &mut *self.console, file);
        ctx.transcript = self.transcript.take();
        let ended = ctx.wait_program(pid);
        let mut message = String::new();
        let mut status = match ended {
            Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
            Ok(w) => {
                let (what, status) = killed::killed(&w);
                // Ctrl-C says only `^C`, as for a built-in, and stops a
                // script (spec §6.4).
                message = if status == CANCELLED {
                    String::from("^C\n")
                } else {
                    format!("{NAME}: {name}: {what}\n")
                };
                status
            }
            Err(e) => {
                message = format!("{NAME}: {name}: {e}\n");
                CANNOT_RUN
            }
        };
        if let Err(e) = ctx.finish() {
            // A program that did not exit keeps its report and status.
            if message.is_empty() {
                status = 1;
            }
            message.insert_str(0, &format!("{name}: write error: {e}\n"));
        }
        self.transcript = ctx.transcript.take();
        self.finish(status, message)
    }
````

with:

````rust
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            system: &mut *self.system,
            transcript: &mut self.transcript,
            in_script: self.in_script,
            status: self.status,
        };
        let ran = match cmd.words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
                Some(builtin) => {
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file),
                        Err(ran) => ran,
                    }
                }
                None => self
                    .runner
                    .get()
                    .run(parts, name, args, cmd.redirect.as_ref()),
            },
            // A bare `> file` just creates or empties the file.
            None => match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            },
        };
        self.stopped = ran.stop;
        let mut status = ran.status;
        if let Some(script) = ran.script {
            status = self.run_script(script);
        }
        self.finish(status, ran.message)
    }
````

Replace:

````rust

    fn end_transcript(&mut self, e: Errno) {
        if let Some(t) = self.transcript.take() {
````

with:

````rust

    fn end_transcript(&mut self, e: vfs::Errno) {
        if let Some(t) = self.transcript.take() {
````

Replace:

````rust
    /// `+ <line>`, then runs and is synced as if typed. Blank and comment
    /// lines are skipped. Ctrl-C, or `reboot`/`poweroff` returning, ends
    /// the script; failing commands do not. Returns the last status.
    fn run_script(&mut self, script: Script) -> i32 {
````

with:

````rust
    /// `+ <line>`, then runs and is synced as if typed. Blank and comment
    /// lines are skipped. Ctrl-C, `exit`, or `reboot`/`poweroff`
    /// returning, ends the script; failing commands do not. Returns the
    /// last status.
    fn run_script(&mut self, script: Script) -> i32 {
````

Replace:

````rust
        status
    }

    /// Opens a redirection target: created if missing, emptied for `>`,
    /// written at its end for `>>`.
    fn open_redirect(&mut self, r: &Redirect) -> Result<(Node, u64), Errno> {
        let path = r.path.as_bytes();
        let node = match self.vfs.lookup(path) {
            Ok(node) => {
                if self.vfs.stat(node)?.kind == FileType::Directory {
                    return Err(Errno::EISDIR);
                }
                if !r.append {
                    self.vfs.truncate(node, 0)?;
                }
                node
            }
            Err(Errno::ENOENT) => self.vfs.create(path)?,
            Err(e) => return Err(e),
        };
        let offset = if r.append {
            self.vfs.stat(node)?.size
        } else {
            0
        };
        Ok((node, offset))
    }
````

with:

````rust
        status
    }
````

- [ ] **Step 12: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::syscall::Spawn;
use crate::{arch, console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
````

with:

````rust
use crate::syscall::Spawn;
use crate::{console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
````

Replace:

````rust
    };
    Shell::new(&mut vfs, &mut console, &mut system).run();
    // `run` returns only if `reboot` or `poweroff` do, which they do not.
    arch::halt_forever()
}
````

with:

````rust
    };
    let mut shell = Shell::new(&mut vfs, &mut console, &mut system);
    shell.greet();
    // `run` returns after `exit` (or if `reboot` or `poweroff` did, which
    // they do not): the in-kernel shell reads on at a new prompt.
    loop {
        shell.run();
    }
}
````

- [ ] **Step 13: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    let mut system = HostSystem(log);
    Shell::new(&mut vfs, console, &mut system).run();
    vfs.shutdown()
````

with:

````rust
    let mut system = HostSystem(log);
    let mut shell = Shell::new(&mut vfs, console, &mut system);
    shell.greet();
    shell.run();
    vfs.shutdown()
````

Replace:

````rust
    eprintln!(
        "host-shell: {} (ext2 root partition). Type `poweroff` to leave.",
        img.display()
````

with:

````rust
    eprintln!(
        "host-shell: {} (ext2 root partition). Type `exit` to leave.",
        img.display()
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 134 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 340 tests.

- [ ] **Step 15: Run the `shell` scenario**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add crates kernel xtask
git commit -m "shell: a Runner runs what is not the shell's own cd, exit and help; exit [code]; greet apart from run"
````


### Task 2: `exit` takes its code as bash does

The prototype's review (minor): `exit` differed from bash at the edges (decision 3). bash takes a code with blanks around it (`exit ' 5'` is 5), refuses one that does not fit in 64 bits (`exit 9223372036854775808` and `exit 99999999999999999999` are `numeric argument required` and status 2, where the prototype wrapped them to 0 and 255), and drops `--` before it (`exit --` keeps the last status). `parse_code` now trims blanks and parses an `i64`, and `exit` drops a leading `--`. Mutation checks: no trimming, no `--`, and a 128-bit number each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`

**Interfaces:**
- Consumes: Task 1's `exit`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            ("1000", 232),
        ] {
````

with:

````rust
            ("1000", 232),
            ("' 5 '", 5),
            ("010", 10),
            ("9223372036854775807", 255),
            ("-9223372036854775808", 0),
            ("-- 3", 3),
        ] {
````

Replace:

````rust
        );
        // Too many: nothing stops.
````

with:

````rust
        );
        // Past what bash's 64-bit number holds, or not a number at all.
        for code in [
            "9223372036854775808",
            "99999999999999999999",
            "''",
            "-",
            "0x10",
        ] {
            let line = alloc::format!("exit {code}");
            assert_eq!(h.run(&line).0, 2, "{code}");
        }
        // `--` ends the options, as for any command.
        h.console.type_in(b"nope\rexit --\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 127);
        h.console.take();
        // Too many: nothing stops.
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `commands::basic::tests::exit_errors`, `commands::basic::tests::exit_stops_the_shell_with_its_code`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// `exit [code]`: the shell stops with `code` (modulo 256), or the last
/// command's status. As in bash, a code that is not a number is status 2
/// and the shell still stops; more than one argument stops nothing.
pub fn exit(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let status = match args {
````

with:

````rust
/// `exit [code]`: the shell stops with `code` (modulo 256), or the last
/// command's status. As in bash, a code that is not a 64-bit number is
/// status 2 and the shell still stops; more than one argument stops
/// nothing; `--` before the code is dropped.
pub fn exit(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let args = match args {
        [dashes, rest @ ..] if dashes == "--" => rest,
        _ => args,
    };
    let status = match args {
````

Replace:

````rust

/// A decimal number with an optional sign, modulo 256.
fn parse_code(code: &str) -> Option<i32> {
    let (negative, digits) = match code.as_bytes().first() {
        Some(b'-') => (true, &code[1..]),
        Some(b'+') => (false, &code[1..]),
        _ => (false, code),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Reduced as it goes, so any number of digits fits.
    let n = digits
        .bytes()
        .fold(0u32, |n, d| (n * 10 + u32::from(d - b'0')) % 256);
    Some(if negative {
        ((256 - n) % 256) as i32
    } else {
        n as i32
    })
}
````

with:

````rust

/// A decimal number with an optional sign and blanks around it, as bash
/// takes it: one that fits in 64 bits, modulo 256.
fn parse_code(code: &str) -> Option<i32> {
    let code = code.trim_matches([' ', '\t']);
    let digits = code.strip_prefix(['-', '+']).unwrap_or(code);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = code.parse().ok()?;
    Some(n.rem_euclid(256) as i32)
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 134 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: exit takes its code as bash does: blanks around it, 64 bits at most, -- before it"
````


### Task 3: The spawning runner starts every other command as a program

Spec §8.2 and decision 3: `Shell::spawning` makes a shell whose runner starts `/bin/<name>`, or the path as given, for every name but `cd`, `exit` and `help`, through the new `shell::Programs` trait (`relay-rt`'s calls in `/bin/sh`, Task 13; `FakePrograms` in the tests). It opens a redirection in the shell (`open_output`: created, emptied or appended to) and passes it as the program's fd 1, and closes its own copy once the program has it, or when it cannot start; a redirection that cannot be opened starts nothing. At the prompt a program runs in a process group of its own that gets the console (`foreground`, spec §6.4, and Task 12); in a script, in the shell's group (Task 5). What cannot start, a killed program and a Ctrl-C say what the in-kernel shell says (`cannot_start`, `ended`). A bare redirection is still the shell's own. Mutation checks: the redirection's fd not closed fails a test; every program in the foreground survives here and fails Task 5's test.

**Files:**
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 1's `Runner`, `Runners`, `Ran`, `cannot_start`, `ended`, `program_path`.
- Produces: `shell::Programs::{open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>, close(&mut self, fd: u32), spawn(&mut self, path: &[u8], args: &[&[u8]], stdout: Option<u32>, foreground: bool) -> Result<u32, Errno>, wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>}`; `Shell::spawning(vfs, console, system, programs: &mut dyn Programs) -> Shell`; `runner::Spawning`; `testing::{FakePrograms, Spawned}`, `Harness::spawning(&mut self, line: &str) -> (i32, String)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
    ran
}
````

with:

````rust
    ran
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, Spawned};
    use alloc::string::String;
    use alloc::vec::Vec;
    use relay_abi::WaitStatus;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_CTRL_C};
    use vfs::Errno;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| String::from(*s)).collect()
    }

    /// `/bin/t-args` exits with 3; `/bin/cat` with 0.
    fn with_programs() -> Harness {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(3)));
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        h
    }

    #[test]
    fn every_command_but_cd_exit_and_help_is_a_program() {
        let mut h = with_programs();
        assert_eq!(h.spawning("t-args a 'b c' ''"), (3, String::new()));
        assert_eq!(
            h.programs.spawned,
            [Spawned {
                path: "/bin/t-args".into(),
                args: words(&["t-args", "a", "b c", ""]),
                stdout: None,
                foreground: true,
            }],
            "argument 0 is the name as typed; at the prompt it gets the console"
        );
        // `cat` is a program too, not the command function.
        h.put("/tmp/f", b"in the file\n");
        assert_eq!(h.spawning("cat /tmp/f"), (0, String::new()));
        assert_eq!(h.programs.spawned[1].path, "/bin/cat");
        // The shell's own commands run in it.
        assert_eq!(h.spawning("cd /tmp"), (0, String::new()));
        assert_eq!(h.spawning("help").0, 0);
        assert_eq!(h.programs.spawned.len(), 2);
        assert_eq!(h.run("pwd").1, "/tmp\n");
    }

    #[test]
    fn a_path_runs_as_given() {
        let mut h = with_programs();
        h.programs.known.push(("./t-args", WaitStatus::exited(5)));
        assert_eq!(h.spawning("./t-args x").0, 5);
        assert_eq!(h.programs.spawned[0].args, words(&["./t-args", "x"]));
        assert_eq!(h.spawning("/bin/t-args").0, 3);
    }

    #[test]
    fn a_redirection_is_opened_in_the_shell_and_passed_as_fd_1() {
        let mut h = with_programs();
        assert_eq!(h.spawning("t-args > /tmp/out").0, 3);
        h.spawning("t-args >> /tmp/out");
        assert_eq!(
            h.programs.opened,
            [("/tmp/out".into(), false, 4), ("/tmp/out".into(), true, 5)]
        );
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.stdout).collect();
        assert_eq!(fds, [Some(4), Some(5)]);
        assert_eq!(h.programs.closed, [4, 5], "the shell keeps no copy");
        // Opened before the program starts, and closed when it cannot.
        assert_eq!(h.spawning("nosuch > /tmp/o").0, 127);
        assert_eq!(h.programs.closed, [4, 5, 6]);
    }

    #[test]
    fn a_redirection_that_cannot_open_starts_nothing() {
        let mut h = with_programs();
        h.programs.open_error = Some(Errno::EISDIR);
        assert_eq!(
            h.spawning("t-args > /tmp"),
            (1, "relay-sh: /tmp: Is a directory\n".into())
        );
        assert!(h.programs.spawned.is_empty());
        // A bare redirection is the shell's alone.
        h.programs.open_error = None;
        assert_eq!(h.spawning("> /tmp/new"), (0, String::new()));
        assert!(h.exists("/tmp/new") && h.programs.opened.is_empty());
    }

    #[test]
    fn what_cannot_start_says_why() {
        let mut h = with_programs();
        h.programs.refusals.push(("/root/text", Errno::ENOEXEC));
        h.programs.refusals.push(("/root", Errno::EISDIR));
        h.programs.refusals.push(("/bin/..", Errno::EISDIR));
        for (line, status, said) in [
            ("nosuch", 127, "relay-sh: nosuch: command not found\n"),
            ("..", 127, "relay-sh: ..: command not found\n"),
            (
                "/root/nosuch",
                127,
                "relay-sh: /root/nosuch: No such file or directory\n",
            ),
            (
                "/root/text",
                126,
                "relay-sh: /root/text: Exec format error\n",
            ),
            ("/root", 126, "relay-sh: /root: Is a directory\n"),
        ] {
            assert_eq!(h.spawning(line), (status, said.into()), "{line}");
        }
    }

    #[test]
    fn a_killed_program_is_reported_and_ctrl_c_is_only_so() {
        let mut h = Harness::new();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1a2c);
        h.programs.known.push(("/bin/t-fault", fault));
        h.programs
            .known
            .push(("/bin/t-spin", WaitStatus::killed(KILLED_CTRL_C)));
        assert_eq!(
            h.spawning("t-fault null-read > /tmp/o"),
            (
                139,
                "relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x401a2c)\n".into()
            )
        );
        assert_eq!(h.spawning("t-spin"), (130, "^C\n".into()));
    }

    #[test]
    fn every_program_is_followed_by_a_sync() {
        let mut h = with_programs();
        h.spawning("t-args");
        h.spawning("nosuch");
        assert_eq!(h.spy.syncs.get(), 2);
    }
}
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, Programs, System};
use alloc::boxed::Box;
````

Replace:

````rust

struct Clock;
````

with:

````rust

/// What a spawning shell asked `FakePrograms` to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawned {
    pub path: String,
    pub args: Vec<String>,
    /// The redirection's fd, as fd 1.
    pub stdout: Option<u32>,
    pub foreground: bool,
}

/// `/bin/sh`'s system calls, for the spawning runner: redirection files
/// are opened by path (and recorded, not created), and a program is known
/// by its path.
pub struct FakePrograms {
    /// The programs `spawn` starts, by path, and how each ends.
    pub known: Vec<(&'static str, WaitStatus)>,
    /// What `spawn` says of a path it does not know (default `ENOENT`).
    pub refusals: Vec<(&'static str, Errno)>,
    /// Every redirection opened: its path, whether it appends, its fd.
    pub opened: Vec<(String, bool, u32)>,
    /// What `open_output` fails with, if anything.
    pub open_error: Option<Errno>,
    pub closed: Vec<u32>,
    pub spawned: Vec<Spawned>,
    /// The children started and not yet waited for, by pid.
    children: Vec<(u32, WaitStatus)>,
    next_fd: u32,
    next_pid: u32,
}

impl FakePrograms {
    pub fn new() -> FakePrograms {
        FakePrograms {
            known: Vec::new(),
            refusals: Vec::new(),
            opened: Vec::new(),
            open_error: None,
            closed: Vec::new(),
            spawned: Vec::new(),
            children: Vec::new(),
            next_fd: 3,
            next_pid: 100,
        }
    }
}

impl Programs for FakePrograms {
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno> {
        if let Some(e) = self.open_error {
            return Err(e);
        }
        self.next_fd += 1;
        let path = String::from_utf8_lossy(path).into_owned();
        self.opened.push((path, append, self.next_fd));
        Ok(self.next_fd)
    }
    fn close(&mut self, fd: u32) {
        self.closed.push(fd);
    }
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
        let path = String::from_utf8_lossy(path).into_owned();
        let Some(&(_, status)) = self.known.iter().find(|(p, _)| *p == path) else {
            let refusal = self.refusals.iter().find(|(p, _)| *p == path);
            return Err(refusal.map_or(Errno::ENOENT, |r| r.1));
        };
        self.spawned.push(Spawned {
            path,
            args: args
                .iter()
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect(),
            stdout,
            foreground,
        });
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
}

struct Clock;
````

Replace:

````rust
    pub system: TestSystem,
    pub spy: Rc<SpyState>,
````

with:

````rust
    pub system: TestSystem,
    pub programs: FakePrograms,
    pub spy: Rc<SpyState>,
````

Replace:

````rust
            system: TestSystem::new(),
            spy: state,
````

with:

````rust
            system: TestSystem::new(),
            programs: FakePrograms::new(),
            spy: state,
````

Replace:

````rust
            system: TestSystem::new(),
            spy,
````

with:

````rust
            system: TestSystem::new(),
            programs: FakePrograms::new(),
            spy,
````

Replace:

````rust
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system).execute(line);
        (status, self.console.take())
````

with:

````rust
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system).execute(line);
        (status, self.console.take())
    }

    /// Runs one command line in a spawning shell (`/bin/sh`'s); its exit
    /// status and what the shell printed.
    pub fn spawning(&mut self, line: &str) -> (i32, String) {
        let status = Shell::spawning(
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            &mut self.programs,
        )
        .execute(line);
        (status, self.console.take())
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` unresolved import `crate::io::Programs` ``.

- [ ] **Step 4: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements both traits over its console, clock, memory
//! manager, log, ACPI and programs; `xtask host-shell` over the host
//! terminal; the tests over buffers.

````

with:

````rust
//! What the shell needs from its surroundings besides files (spec §7.3).
//! The kernel implements `Console` and `System` over its console, clock,
//! memory manager, log, ACPI and programs; `xtask host-shell` over the
//! host terminal; `relay-rt` over system calls; the tests over buffers.
//! `Programs` is `/bin/sh`'s way to its commands.

````

Replace:

````rust
        Err(Errno::ECHILD)
    }
}
````

with:

````rust
        Err(Errno::ECHILD)
    }
}

/// Programs, for a shell that runs its commands as programs (`/bin/sh`,
/// user-space gate §8.2): `relay-rt`'s system calls there, a fake in the
/// tests.
pub trait Programs {
    /// Opens a redirection target for writing: created if missing, emptied
    /// or, with `append`, written at its end. Its fd.
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
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
    /// Waits for the child `pid` to end.
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
}
````

- [ ] **Step 5: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, System};
pub use shell::Shell;
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, Programs, System};
pub use shell::Shell;
````

- [ ] **Step 6: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, System};
use crate::killed;
````

with:

````rust
//! in-kernel shell, whose other names are programs it reaches through
//! `System::spawn`); the spawning runner starts `/bin/<name>` for every
//! one (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::Ctx;
use crate::io::{Console, Programs, System};
use crate::killed;
````

Replace:

````rust
/// borrows until it is dropped.)
pub(crate) enum Runners {
    InProcess(InProcess),
}

impl Runners {
    pub fn get(&mut self) -> &mut dyn Runner {
        match self {
            Runners::InProcess(r) => r,
        }
````

with:

````rust
/// borrows until it is dropped.)
pub(crate) enum Runners<'a> {
    InProcess(InProcess),
    Spawning(Spawning<'a>),
}

impl Runners<'_> {
    pub fn get(&mut self) -> &mut dyn Runner {
        match self {
            Runners::InProcess(r) => r,
            Runners::Spawning(r) => r,
        }
````

Replace:

````rust
            None => run_program(parts, name, args, file),
        }
````

with:

````rust
            None => run_program(parts, name, args, file),
        }
    }
}

/// Every command a program: `/bin/<name>`, or the path as given.
pub(crate) struct Spawning<'a> {
    pub programs: &'a mut dyn Programs,
}

impl Runner for Spawning<'_> {
    /// Redirections are opened in the shell and passed to the program as
    /// its fd 1, which the shell closes once the program has it. At the
    /// prompt a program gets a process group of its own and the console;
    /// in a script it runs in the shell's group, so that Ctrl-C ends the
    /// script with it (spec §6.4).
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let stdout = match redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
            None => None,
        };
        let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
        argv.extend(args.iter().map(|w| w.as_bytes()));
        let path = program_path(name);
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, stdout, !parts.in_script);
        if let Some(fd) = stdout {
            self.programs.close(fd);
        }
        match started {
            Ok(pid) => match self.programs.wait(pid) {
                Ok(w) => ended(name, &w),
                Err(e) => Ran::said(CANNOT_RUN, format!("{NAME}: {name}: {e}\n")),
            },
            Err(e) => cannot_start(name, e),
        }
````

- [ ] **Step 7: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, System};
use crate::parser::{self, HOME};
````

with:

````rust
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, System};
use crate::parser::{self, HOME};
````

Replace:

````rust
    system: &'a mut dyn System,
    runner: Runners,
    editor: LineEditor,
````

with:

````rust
    system: &'a mut dyn System,
    runner: Runners<'a>,
    editor: LineEditor,
````

Replace:

````rust

    fn with_runner(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        runner: Runners,
    ) -> Shell<'a> {
````

with:

````rust

    /// A shell whose commands are programs: `/bin/sh` (the spawning
    /// runner, user-space gate §8.2). Only `cd`, `exit` and `help` run in
    /// it.
    pub fn spawning(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        programs: &'a mut dyn Programs,
    ) -> Shell<'a> {
        let runner = Runners::Spawning(runner::Spawning { programs });
        Shell::with_runner(vfs, console, system, runner)
    }

    fn with_runner(
        vfs: &'a mut dyn Vfs,
        console: &'a mut dyn Console,
        system: &'a mut dyn System,
        runner: Runners<'a>,
    ) -> Shell<'a> {
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 141 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -m "shell: the spawning runner starts every command but cd, exit and help as a program, its redirection opened in the shell as fd 1"
````


### Task 4: A command runs as a program: output on fd 1, errors on the console

Spec §8.1, §8.4 and decisions 5 and 8: `shell::run_command(name, run, args, vfs, console, system, stdout)` runs a command function as its program will, with standard output on the program's fd 1, the new `shell::Stdout` trait (`relay-rt`'s `SysStdout`, Task 11; `FakeStdout` in the tests): `Ctx::program` makes a context whose output goes there, written at once when it is the console, so that it keeps its place among the errors on fd 2, and in pieces of 4 KiB when it is a file; `ls` lays out columns only on the console; `cat f >> f` knows its output file by `Stdout::node`. A write error is `<name>: write error: <message>` with status 1, as the shell said it in milestone 1. Each program names its own function (`commands::Run`; the command functions are re-exported), so it holds no other command's code. `System::reboot` and `System::poweroff` take `-f` and return the error that kept the machine up, since a program's `power` call shuts the filesystems down itself and says why it could not: `reboot` and `poweroff` print milestone 1's two lines from that error too (the kernel's, the host's and the tests' systems never return one). Mutation checks: output to the console buffered, no write-error line, and `power`'s error ignored each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Modify: `crates/shell/src/commands/system.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Create: `crates/shell/src/program.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `kernel/src/session.rs`
- Modify: `xtask/src/host_shell.rs`

**Interfaces:**
- Consumes: Task 1's shell; milestone 1's `Ctx`.
- Produces: `shell::Stdout::{write(&mut self, &[u8]) -> Result<(), Errno>, is_tty(&self) -> bool, node(&self) -> Option<Node>}`; `shell::run_command(name: &str, run: commands::Run, args: &[String], vfs: &mut dyn Vfs, console: &mut dyn Console, system: &mut dyn System, stdout: &mut dyn Stdout) -> i32`; `commands::Run` (`fn(&mut Ctx<'_>, &[String]) -> i32`) and the re-exported command functions `commands::{cat, clear, cp, date, df, dmesg, echo, free, head, ls, mkdir, mv, poweroff, pwd, reboot, rm, rmdir, stat, sync, tail, touch, uname, wc}`; `System::{reboot(&mut self, force: bool) -> Result<(), Errno>, poweroff(&mut self, force: bool) -> Result<(), Errno>}`; `testing::FakeStdout`, `Harness::program`, `TestSystem::{power_error, forced}`.

- [ ] **Step 1: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub mod parser;
mod runner;
````

with:

````rust
pub mod parser;
mod program;
mod runner;
````

- [ ] **Step 2: Write the failing tests for `crates/shell/src/program.rs`**

Create `crates/shell/src/program.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
    use vfs::{Errno, Vfs};

    #[test]
    fn output_goes_to_stdout_and_errors_to_the_console() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"in a\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.program("cat /tmp/a /tmp/nope /tmp/a", &mut out),
            (1, "cat: /tmp/nope: No such file or directory\n".into())
        );
        // To the console it is written at once, so that it keeps its place
        // among the errors.
        assert_eq!(out.writes, [b"in a\n".to_vec(), b"in a\n".to_vec()]);
    }

    #[test]
    fn output_to_a_file_is_written_in_pieces_of_4_kib() {
        let mut h = Harness::new();
        let big = alloc::vec![b'x'; 10_000];
        h.put("/tmp/big", &big);
        let mut out = FakeStdout::file(None);
        assert_eq!(h.program("cat /tmp/big", &mut out), (0, String::new()));
        assert_eq!(out.text().len(), 10_000);
        assert!(out.writes.len() <= 3, "{} writes", out.writes.len());
        // And a line at a time is not a write each.
        let mut out = FakeStdout::file(None);
        h.program("ls /etc", &mut out);
        assert_eq!(out.writes, [b"hostname\nmotd\n".to_vec()]);
    }

    #[test]
    fn ls_lays_out_columns_only_on_the_console() {
        let mut h = Harness::new();
        let mut out = FakeStdout::console();
        h.program("ls /etc", &mut out);
        assert_eq!(out.text(), "hostname  motd\n");
    }

    #[test]
    fn a_write_error_is_reported_as_the_shell_did() {
        let mut h = Harness::new();
        h.put("/tmp/big", &alloc::vec![b'x'; 10_000]);
        for mut out in [FakeStdout::file(None), FakeStdout::console()] {
            out.fail_after = Some((5000, Errno::ENOSPC));
            assert_eq!(
                h.program("cat /tmp/big", &mut out),
                (1, "cat: write error: No space left on device\n".into())
            );
        }
    }

    #[test]
    fn a_file_is_not_its_own_output() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"f\n");
        let node = h.vfs.lookup(b"/tmp/f").unwrap();
        let mut out = FakeStdout::file(Some(node));
        assert_eq!(
            h.program("cat /tmp/f", &mut out),
            (1, "cat: /tmp/f: input file is output file\n".into())
        );
    }

    #[test]
    fn reboot_and_poweroff_say_why_the_machine_stayed_up() {
        let mut h = Harness::new();
        h.system.power_error = Some(Errno::EIO);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.program("reboot", &mut out),
            (
                1,
                "reboot: cannot shut the filesystems down cleanly: Input/output error\n\
                 reboot: use 'reboot -f' to go ahead anyway\n"
                    .into()
            )
        );
        assert_eq!(h.system.reboots, 0);
        // -f goes ahead: the kernel's `power` shuts down what it can.
        assert_eq!(h.program("poweroff -f", &mut out), (0, String::new()));
        assert_eq!(
            (h.system.poweroffs, &h.system.forced[..]),
            (1, &[false, true][..])
        );
        assert_eq!(
            h.spy.shutdowns.get(),
            2,
            "the Vfs's own shutdown comes first"
        );
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, Programs, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, Programs, Stdout, System};
use alloc::boxed::Box;
````

Replace:

````rust
use relay_abi::WaitStatus;
use vfs::{DirEntry, Env, Errno, FileSystem, FileType, Ino, MemFs, MountTable, Stat, StatFs, Vfs};

````

with:

````rust
use relay_abi::WaitStatus;
use vfs::{
    DirEntry, Env, Errno, FileSystem, FileType, Ino, MemFs, MountTable, Node, Stat, StatFs, Vfs,
};

````

Replace:

````rust
    child: Option<usize>,
}
````

with:

````rust
    child: Option<usize>,
    /// What `reboot` and `poweroff` say, unforced, as a program's `power`
    /// call does when the filesystems cannot be shut down.
    pub power_error: Option<Errno>,
    /// Whether each `reboot` and `poweroff` was forced.
    pub forced: Vec<bool>,
}

impl TestSystem {
    fn power(&mut self, force: bool) -> Result<(), Errno> {
        self.forced.push(force);
        match self.power_error {
            Some(e) if !force => Err(e),
            _ => Ok(()),
        }
    }
}
````

Replace:

````rust
            child: None,
        }
````

with:

````rust
            child: None,
            power_error: None,
            forced: Vec::new(),
        }
````

Replace:

````rust
    }
    fn reboot(&mut self) {
        self.reboots += 1;
    }
    fn poweroff(&mut self) {
        self.poweroffs += 1;
    }
````

with:

````rust
    }
    fn reboot(&mut self, force: bool) -> Result<(), Errno> {
        self.power(force)?;
        self.reboots += 1;
        Ok(())
    }
    fn poweroff(&mut self, force: bool) -> Result<(), Errno> {
        self.power(force)?;
        self.poweroffs += 1;
        Ok(())
    }
````

Replace:

````rust
        Ok(self.children.remove(i).1)
    }
````

with:

````rust
        Ok(self.children.remove(i).1)
    }
}

/// A program's fd 1: the console or a file, keeping every write whole.
pub struct FakeStdout {
    pub tty: bool,
    pub node: Option<Node>,
    pub writes: Vec<Vec<u8>>,
    /// Writes fail with this once this many bytes were written.
    pub fail_after: Option<(usize, Errno)>,
}

impl FakeStdout {
    pub fn console() -> FakeStdout {
        FakeStdout {
            tty: true,
            node: None,
            writes: Vec::new(),
            fail_after: None,
        }
    }

    pub fn file(node: Option<Node>) -> FakeStdout {
        FakeStdout {
            tty: false,
            ..FakeStdout::console()
        }
        .with_node(node)
    }

    fn with_node(mut self, node: Option<Node>) -> FakeStdout {
        self.node = node;
        self
    }

    /// Everything written.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.writes.concat()).into_owned()
    }
}

impl Stdout for FakeStdout {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        if let Some((limit, e)) = self.fail_after
            && self.writes.iter().map(Vec::len).sum::<usize>() + bytes.len() > limit
        {
            return Err(e);
        }
        self.writes.push(bytes.to_vec());
        Ok(())
    }
    fn is_tty(&self) -> bool {
        self.tty
    }
    fn node(&self) -> Option<Node> {
        self.node
    }
````

Replace:

````rust

    /// Creates (or replaces) a file.
````

with:

````rust

    /// Runs `line` as the program named by its first word does, standard
    /// output going to `stdout`; its status and what it said on the
    /// console.
    pub fn program(&mut self, line: &str, stdout: &mut FakeStdout) -> (i32, String) {
        let words = crate::parser::parse(line).unwrap().words;
        let status = crate::run_command(
            &words[0],
            crate::commands::find(&words[0]).unwrap().run,
            &words[1..],
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            stdout,
        );
        (status, self.console.take())
    }

    /// Creates (or replaces) a file.
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `run_command` in the crate root ``; `` unresolved import `crate::io::Stdout` ``.

- [ ] **Step 5: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

pub(crate) use script::Script;
pub use script::transcript_name;

````

with:

````rust

pub use basic::{clear, echo, pwd, uname};
pub use change::{cp, mkdir, mv, rm, rmdir, touch};
pub use ls::ls;
pub(crate) use script::Script;
pub use script::transcript_name;
pub use stat::stat;
pub use system::{date, df, dmesg, free, poweroff, reboot, sync};
pub use text::{cat, head, tail, wc};

/// A command function: runs the command with its arguments (without the
/// name); returns the exit status.
pub type Run = fn(&mut Ctx<'_>, &[String]) -> i32;

````

Replace:

````rust
    pub help: &'static str,
    /// Runs the command with its arguments (without the name); returns the
    /// exit status.
    pub run: fn(&mut Ctx<'_>, &[String]) -> i32,
}
````

with:

````rust
    pub help: &'static str,
    pub run: Run,
}
````

- [ ] **Step 6: Change `crates/shell/src/commands/system.rs`**

In `crates/shell/src/commands/system.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// (spec §7.4). If that fails the machine stays up, so nothing is lost
/// silently; `-f` goes ahead anyway.
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
````

with:

````rust
/// (spec §7.4). If that fails the machine stays up, so nothing is lost
/// silently; `-f` goes ahead anyway. (A program's filesystems are shut down
/// by the kernel's `power`, which returns the error instead.)
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
````

Replace:

````rust
    };
    if let Err(e) = ctx.vfs.shutdown() {
        ctx.fail(
            name,
            format_args!("cannot shut the filesystems down cleanly: {e}"),
        );
        if !opts.has('f') {
            return ctx.fail(name, format_args!("use '{name} -f' to go ahead anyway"));
        }
    }
    if name == "reboot" {
        ctx.system.reboot();
    } else {
        ctx.system.poweroff();
    }
````

with:

````rust
    };
    let force = opts.has('f');
    let unclean = |ctx: &mut Ctx<'_>, e| {
        ctx.fail(
            name,
            format_args!("cannot shut the filesystems down cleanly: {e}"),
        );
        if force {
            1
        } else {
            ctx.fail(name, format_args!("use '{name} -f' to go ahead anyway"))
        }
    };
    if let Err(e) = ctx.vfs.shutdown() {
        let status = unclean(ctx, e);
        if !force {
            return status;
        }
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

- [ ] **Step 7: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 12 replacements, top to bottom:

Replace:

````rust
//! What a command gets to work with: the filesystem, the system, standard
//! output (the screen or a redirection file) and the screen for errors;
//! plus the helpers every command shares for options and GNU-style
//! messages.

use crate::io::{Console, System};
use crate::transcript::Transcript;
````

with:

````rust
//! What a command gets to work with: the filesystem, the system, standard
//! output (the screen, a redirection file, or a program's fd 1) and the
//! screen for errors; plus the helpers every command shares for options
//! and GNU-style messages.

use crate::io::{Console, Stdout, System};
use crate::transcript::Transcript;
````

Replace:

````rust
    console: &'a mut dyn Console,
    out: Output,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

with:

````rust
    console: &'a mut dyn Console,
    out: Output<'a>,
    /// Set by `exit`, and by `reboot` and `poweroff` when the machine did
````

Replace:

````rust

enum Output {
    Console,
````

with:

````rust

enum Output<'a> {
    Console,
````

Replace:

````rust
        /// The first write error; later output is dropped.
        error: Option<Errno>,
````

with:

````rust
        /// The first write error; later output is dropped.
        error: Option<Errno>,
    },
    /// A program's standard output (user-space gate §8.1): written at once
    /// when it is the console, so that it keeps its place among the
    /// errors, and in pieces of 4 KiB when it is a file.
    Program {
        stdout: &'a mut dyn Stdout,
        tty: bool,
        buf: Vec<u8>,
        error: Option<Errno>,
````

Replace:

````rust
        };
        Ctx {
````

with:

````rust
        };
        Ctx::with_output(vfs, system, console, out)
    }

    /// A command run as a program: standard output is `stdout`, errors go
    /// to `console`.
    pub(crate) fn program(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        stdout: &'a mut dyn Stdout,
    ) -> Ctx<'a> {
        let tty = stdout.is_tty();
        let out = Output::Program {
            stdout,
            tty,
            buf: Vec::new(),
            error: None,
        };
        Ctx::with_output(vfs, system, console, out)
    }

    fn with_output(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        out: Output<'a>,
    ) -> Ctx<'a> {
        Ctx {
````

Replace:

````rust
    /// Where output goes, borrowed apart from the system.
    fn streams(&mut self) -> Streams<'_> {
        Streams {
````

with:

````rust
    /// Where output goes, borrowed apart from the system.
    fn streams(&mut self) -> Streams<'_, 'a> {
        Streams {
````

Replace:

````rust
    pub fn is_tty(&self) -> bool {
        matches!(self.out, Output::Console)
    }
````

with:

````rust
    pub fn is_tty(&self) -> bool {
        match self.out {
            Output::Console => true,
            Output::File { .. } => false,
            Output::Program { tty, .. } => tty,
        }
    }
````

Replace:

````rust
    pub fn out_failed(&self) -> bool {
        matches!(self.out, Output::File { error: Some(_), .. })
    }

    /// The file standard output goes to, if any.
    pub fn output_node(&self) -> Option<Node> {
        match self.out {
            Output::File { node, .. } => Some(node),
            Output::Console => None,
````

with:

````rust
    pub fn out_failed(&self) -> bool {
        matches!(
            self.out,
            Output::File { error: Some(_), .. } | Output::Program { error: Some(_), .. }
        )
    }

    /// The file standard output goes to, if any.
    pub fn output_node(&self) -> Option<Node> {
        match &self.out {
            Output::File { node, .. } => Some(*node),
            Output::Program { stdout, .. } => stdout.node(),
            Output::Console => None,
````

Replace:

````rust
        match self.out {
            Output::File { error: Some(e), .. } => Err(e),
            _ => Ok(()),
````

with:

````rust
        match self.out {
            Output::File { error: Some(e), .. } | Output::Program { error: Some(e), .. } => Err(e),
            _ => Ok(()),
````

Replace:

````rust
/// A command's standard output, the screen and a script's transcript.
struct Streams<'s> {
    vfs: &'s mut dyn Vfs,
    console: &'s mut dyn Console,
    out: &'s mut Output,
    transcript: &'s mut Option<Transcript>,
}

impl Streams<'_> {
    /// Standard output; the file's first write error, once there is one.
    fn out(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, error, .. } => {
                if let Some(e) = error {
````

with:

````rust
/// A command's standard output, the screen and a script's transcript.
struct Streams<'s, 'a> {
    vfs: &'s mut dyn Vfs,
    console: &'s mut dyn Console,
    out: &'s mut Output<'a>,
    transcript: &'s mut Option<Transcript>,
}

impl Streams<'_, '_> {
    /// Standard output; the file's first write error, once there is one.
    fn out(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::Program {
                stdout,
                tty: true,
                error,
                ..
            } => {
                if error.is_none()
                    && let Err(e) = stdout.write(bytes)
                {
                    *error = Some(e);
                }
            }
            Output::File { buf, error, .. } | Output::Program { buf, error, .. } => {
                if let Some(e) = error {
````

Replace:

````rust
        match &*self.out {
            Output::File { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
````

with:

````rust
        match &*self.out {
            Output::File { error: Some(e), .. } | Output::Program { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
````

Replace:

````rust
    fn flush(&mut self) {
        let Output::File {
            node,
            offset,
            buf,
            error,
        } = &mut *self.out
        else {
            return;
        };
        let mut done = 0;
        while error.is_none() && done < buf.len() {
            match self.vfs.write_at(*node, *offset, &buf[done..]) {
                // Nothing written would loop forever; the contract says
                // that is ENOSPC.
                Ok(0) => *error = Some(Errno::ENOSPC),
                Ok(n) => {
                    done += n;
                    *offset += n as u64;
                }
                Err(e) => *error = Some(e),
            }
        }
        buf.clear();
    }
````

with:

````rust
    fn flush(&mut self) {
        match &mut *self.out {
            Output::File {
                node,
                offset,
                buf,
                error,
            } => {
                let mut done = 0;
                while error.is_none() && done < buf.len() {
                    match self.vfs.write_at(*node, *offset, &buf[done..]) {
                        // Nothing written would loop forever; the contract
                        // says that is ENOSPC.
                        Ok(0) => *error = Some(Errno::ENOSPC),
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                buf.clear();
            }
            Output::Program {
                stdout, buf, error, ..
            } => {
                if error.is_none()
                    && !buf.is_empty()
                    && let Err(e) = stdout.write(buf)
                {
                    *error = Some(e);
                }
                buf.clear();
            }
            Output::Console => {}
        }
    }
````

- [ ] **Step 8: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use relay_abi::WaitStatus;
use vfs::{Errno, Vfs};

````

with:

````rust
use relay_abi::WaitStatus;
use vfs::{Errno, Node, Vfs};

````

Replace:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Restarts the machine. Returns only where it cannot (on the host, in
    /// tests); the shell then stops.
    fn reboot(&mut self);
    /// Turns the machine off. Returns only where it cannot; the shell then
    /// stops.
    fn poweroff(&mut self);
    /// Starts the program at `path`, read through `vfs`, with `args`
````

with:

````rust
    fn kernel_log(&self) -> Vec<u8>;
    /// Restarts the machine, going ahead with `force` when the filesystems
    /// cannot be shut down cleanly. Returns only where it cannot (on the
    /// host, in tests), and the shell then stops; or with the error that
    /// kept the machine up (a program's `power` call, which shuts the
    /// filesystems down itself).
    fn reboot(&mut self, force: bool) -> Result<(), Errno>;
    /// Turns the machine off, as `reboot` restarts it.
    fn poweroff(&mut self, force: bool) -> Result<(), Errno>;
    /// Starts the program at `path`, read through `vfs`, with `args`
````

Replace:

````rust
        Err(Errno::ECHILD)
    }
}

````

with:

````rust
        Err(Errno::ECHILD)
    }
}

/// A program's standard output, fd 1 (user-space gate §8.1): the console
/// or a file the shell opened for it.
pub trait Stdout {
    /// Writes all of `bytes`; the error that stopped it (`ENOSPC` for a
    /// write that took nothing).
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno>;
    /// Whether it is the console (`ls` lays out columns then).
    fn is_tty(&self) -> bool;
    /// The file it is, as the `Vfs` names files (`cat f >> f` must not
    /// read its own output).
    fn node(&self) -> Option<Node>;
}

````

- [ ] **Step 9: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, Programs, System};
pub use shell::Shell;
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, Programs, Stdout, System};
pub use program::run_command;
pub use shell::Shell;
````

- [ ] **Step 10: Implement `crates/shell/src/program.rs`**

Insert this at the top of `crates/shell/src/program.rs`, above `#[cfg(test)]`:

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

/// Runs the command `name`, whose function is `run` (one of
/// `commands`'), with `args` (without the name), as its program does;
/// returns the exit status. A write error on standard output is reported
/// as `<name>: write error: <message>` with status 1. Each program names
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
    if let Err(e) = ctx.finish() {
        ctx.err(format!("{name}: write error: {e}\n").as_bytes());
        status = 1;
    }
    status
}

````

- [ ] **Step 11: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust

    // The shell has shut the filesystems down before these.
    fn reboot(&mut self) {
        power::reboot()
    }

    fn poweroff(&mut self) {
        power::poweroff(self.test_mode)
````

with:

````rust

    // The shell has shut the filesystems down before these (or `-f` goes
    // ahead without).
    fn reboot(&mut self, _force: bool) -> Result<(), Errno> {
        power::reboot()
    }

    fn poweroff(&mut self, _force: bool) -> Result<(), Errno> {
        power::poweroff(self.test_mode)
````

- [ ] **Step 12: Change `xtask/src/host_shell.rs`**

In `xtask/src/host_shell.rs`, replace:

````rust
    }
    fn reboot(&mut self) {}
    fn poweroff(&mut self) {}
}
````

with:

````rust
    }
    fn reboot(&mut self, _force: bool) -> Result<(), vfs::Errno> {
        Ok(())
    }
    fn poweroff(&mut self, _force: bool) -> Result<(), vfs::Errno> {
        Ok(())
    }
}
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 147 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 340 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add crates kernel xtask
git commit -m "shell: a command runs as a program, its output on fd 1 and its errors on the console; reboot and poweroff report the error that kept the machine up"
````


### Task 5: `/bin/sh FILE`: a script's commands in its own group, its transcript a tee

Spec §6.4, §6.5, §8.3 and decision 10: `Shell::run_file(args, stdout)` is `/bin/sh FILE` in a spawning shell. It runs `sh`'s checks through a program's context (so `sh s.sh > f` is `sh: a script's output cannot be redirected`, and the other messages are milestone 1's), then pushes the transcript as a console tee (`Programs::tee_push`), traces, runs and syncs each line (`Shell::run_lines`, shared with the in-process runner's scripts), and pops the tee: a failed write is reported then, `sh: <log>: <error>; the transcript ends here` (`transcript::ended`); a transcript that cannot be pushed runs nothing. A script's commands run in the script shell's group (no `foreground`), so Ctrl-C ends the script with them; `sh` in a script starts another `/bin/sh`, so one script may run another. `sh`'s documentation says that a script which redirects into its own transcript garbles it, as under bash (milestone 1's deferred finding, decision 12). Mutation checks: a script's commands in the foreground (Task 3's mutant) and the pop's error ignored each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `crates/shell/src/transcript.rs`

**Interfaces:**
- Consumes: Tasks 3 (`Programs`, `Spawning`), 4 (`Ctx::program`, `Stdout`).
- Produces: `Programs::{tee_push(&mut self, path: &[u8]) -> Result<(), Errno>, tee_pop(&mut self) -> Result<(), Errno>}`; `Shell::run_file(&mut self, args: &[String], stdout: &mut dyn Stdout) -> i32`; `transcript::ended(name: &str, e: Errno) -> String`; `Runners::programs(&mut self) -> Option<&mut dyn Programs>`; `FakePrograms::{tees, pushed, push_error, pop_error}`, `Harness::sh`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use crate::Shell;
    use crate::testing::{FakeProgram, Harness};
    use alloc::string::String;
````

with:

````rust
    use crate::Shell;
    use crate::testing::{FakeProgram, FakeStdout, Harness};
    use alloc::string::String;
````

Replace:

````rust
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

with:

````rust
    }

    /// `/bin/t-args` exits with 3 and `/bin/sh` with 0, for a spawning
    /// shell.
    fn spawning() -> Harness {
        let mut h = Harness::new();
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(3)));
        h.programs.known.push(("/bin/sh", WaitStatus::exited(0)));
        h
    }

    #[test]
    fn bin_sh_runs_a_script_s_commands_in_its_own_group_with_its_transcript_a_tee() {
        let mut h = spawning();
        h.put("/tmp/s.log", b"an old transcript");
        h.put(
            "/tmp/s.sh",
            b"t-args a\n# a comment\ncd /etc\nnosuch\nt-args b\n",
        );
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                3,
                "+ t-args a\n+ cd /etc\n+ nosuch\nrelay-sh: nosuch: command not found\n+ t-args b\n"
                    .into()
            )
        );
        assert!(
            h.programs.spawned.iter().all(|s| !s.foreground),
            "a script's commands run in its group, so Ctrl-C ends it with them"
        );
        assert_eq!(h.programs.pushed, ["/tmp/s.log"]);
        assert!(h.programs.tees.is_empty(), "popped at the end");
        assert_eq!(h.get("/tmp/s.log"), b"", "emptied; the tee writes it");
        assert_eq!(h.run("pwd").1, "/etc\n", "the script's own directory");
    }

    #[test]
    fn a_script_run_by_bin_sh_may_run_another() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"sh /tmp/t.sh\n");
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (0, "+ sh /tmp/t.sh\n".into())
        );
        assert_eq!(h.programs.spawned[0].path, "/bin/sh");
        assert_eq!(h.programs.spawned[0].args, ["sh", "/tmp/t.sh"]);
    }

    #[test]
    fn a_transcript_that_failed_is_reported_when_the_script_ends() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\nt-args\n");
        h.programs.pop_error = Some(Errno::ENOSPC);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                3,
                "+ t-args\n+ t-args\nsh: /tmp/s.log: No space left on device; the transcript ends here\n"
                    .into()
            )
        );
    }

    #[test]
    fn a_transcript_that_cannot_be_pushed_runs_nothing() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\n");
        h.programs.push_error = Some(Errno::EBUSY);
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (
                1,
                "sh: cannot write the transcript /tmp/s.log: Device or resource busy\n".into()
            )
        );
        assert!(h.programs.spawned.is_empty());
    }

    #[test]
    fn bin_sh_keeps_sh_s_rules() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args\n");
        let mut out = FakeStdout::file(None);
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (1, "sh: a script's output cannot be redirected\n".into())
        );
        let mut out = FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/nope.sh"], &mut out),
            (1, "sh: /tmp/nope.sh: No such file or directory\n".into())
        );
        assert!(h.programs.spawned.is_empty() && h.programs.pushed.is_empty());
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    children: Vec<(u32, WaitStatus)>,
    next_fd: u32,
````

with:

````rust
    children: Vec<(u32, WaitStatus)>,
    /// The tees pushed and not popped, by path.
    pub tees: Vec<String>,
    /// Every tee pushed.
    pub pushed: Vec<String>,
    /// What `tee_push` fails with, if anything.
    pub push_error: Option<Errno>,
    /// What the next `tee_pop` returns, as a failed write would.
    pub pop_error: Option<Errno>,
    next_fd: u32,
````

Replace:

````rust
            children: Vec::new(),
            next_fd: 3,
````

with:

````rust
            children: Vec::new(),
            tees: Vec::new(),
            pushed: Vec::new(),
            push_error: None,
            pop_error: None,
            next_fd: 3,
````

Replace:

````rust
        Ok(self.children.remove(i).1)
    }
````

with:

````rust
        Ok(self.children.remove(i).1)
    }
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno> {
        if let Some(e) = self.push_error {
            return Err(e);
        }
        let path = String::from_utf8_lossy(path).into_owned();
        self.pushed.push(path.clone());
        self.tees.push(path);
        Ok(())
    }
    fn tee_pop(&mut self) -> Result<(), Errno> {
        self.tees.pop().ok_or(Errno::EINVAL)?;
        self.pop_error.take().map_or(Ok(()), Err)
    }
````

Replace:

````rust

    /// Creates (or replaces) a file.
````

with:

````rust

    /// Runs `/bin/sh` with `args` (after argument 0), as a spawning shell
    /// does, its fd 1 `stdout`; its status and what it printed.
    pub fn sh(&mut self, args: &[&str], stdout: &mut FakeStdout) -> (i32, String) {
        let args: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
        let status = Shell::spawning(
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            &mut self.programs,
        )
        .run_file(&args, stdout);
        (status, self.console.take())
    }

    /// Creates (or replaces) a file.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `tee_push` is not a member of trait `Programs` ``; `` method `tee_pop` is not a member of trait `Programs` ``.

- [ ] **Step 4: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
//! `x.log`), written and synced as each line starts and ends, so it can be
//! checked afterwards (`cargo xtask verify-usb`).

````

with:

````rust
//! `x.log`), written and synced as each line starts and ends, so it can be
//! checked afterwards (`cargo xtask verify-usb`). Under `/bin/sh` the
//! transcript is a console tee, and a script may run another. A command of
//! the script that redirects into the script's own transcript garbles it,
//! as it would under bash: the redirection writes from the file's start,
//! the transcript goes on where it was.

````

- [ ] **Step 5: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
}
````

with:

````rust
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno>;
    /// Copies the console into the file at `path` from now on (a script's
    /// transcript, spec §6.5), written at its end.
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno>;
    /// Stops the copy `tee_push` started; the error of a write that failed
    /// meanwhile, which ended the copying there.
    fn tee_pop(&mut self) -> Result<(), Errno>;
}
````

- [ ] **Step 6: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
            Runners::Spawning(r) => r,
        }
````

with:

````rust
            Runners::Spawning(r) => r,
        }
    }

    /// The spawning runner's programs, which push and pop tees.
    pub fn programs(&mut self) -> Option<&mut dyn Programs> {
        match self {
            Runners::InProcess(_) => None,
            Runners::Spawning(r) => Some(&mut *r.programs),
        }
````

- [ ] **Step 7: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::commands::{self, Script};
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, System};
use crate::parser::{self, HOME};
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::Transcript;
use alloc::format;
````

with:

````rust
use crate::commands::{self, Script};
use crate::ctx::Ctx;
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, Stdout, System};
use crate::parser::{self, HOME};
use crate::runner::{self, Parts, Ran, Runners};
use crate::transcript::{self, Transcript};
use alloc::format;
````

Replace:

````rust

    /// Runs a script's lines (spec §15 item 12): each command is shown as
````

with:

````rust

    /// Runs a script `sh` read, in this shell (the in-process runner): its
    /// transcript is written by the shell. A script cannot run another.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let status = self.run_lines(&script.text);
        self.write_transcript();
        self.transcript = None;
        status
    }

    /// `/bin/sh FILE`: runs the script `args` names (user-space gate §8.3)
    /// as `sh` does, in a spawning shell (`Shell::spawning`), whose
    /// commands then run in its process group. The transcript is a console
    /// tee, so it gets the output of the script's programs and of any
    /// script it runs (whose own transcript is pushed on top); a write
    /// that failed is reported when the script ends. `stdout` is the
    /// shell's fd 1: a script's output cannot be redirected.
    pub fn run_file(&mut self, args: &[String], stdout: &mut dyn Stdout) -> i32 {
        let Some(sh) = commands::find("sh") else {
            unreachable!("sh is in the command table")
        };
        let mut ctx = Ctx::program(
            &mut *self.vfs,
            &mut *self.system,
            &mut *self.console,
            stdout,
        );
        let status = (sh.run)(&mut ctx, args);
        let Some(script) = ctx.script.take() else {
            return status;
        };
        let Some(programs) = self.runner.programs() else {
            unreachable!("run_file needs a spawning shell")
        };
        let log = script.transcript_name;
        if let Err(e) = programs.tee_push(log.as_bytes()) {
            let shown = path::display(log.as_bytes());
            let message = format!("sh: cannot write the transcript {shown}: {e}\n");
            self.console.write(message.as_bytes());
            return 1;
        }
        let status = self.run_lines(&script.text);
        if let Some(Err(e)) = self.runner.programs().map(|p| p.tee_pop()) {
            self.console.write(transcript::ended(&log, e).as_bytes());
        }
        status
    }

    /// Runs a script's lines (spec §15 item 12): each command is shown as
````

Replace:

````rust
    /// last status.
    fn run_script(&mut self, script: Script) -> i32 {
        self.in_script = true;
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let mut status = 0;
        for line in script.text.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
````

with:

````rust
    /// last status.
    fn run_lines(&mut self, text: &str) -> i32 {
        self.in_script = true;
        let mut status = 0;
        for line in text.lines() {
            if matches!(parser::parse(line), Ok(c) if c.words.is_empty() && c.redirect.is_none()) {
````

Replace:

````rust
        }
        self.write_transcript();
        self.transcript = None;
        self.in_script = false;
````

with:

````rust
        }
        self.in_script = false;
````

- [ ] **Step 8: Change `crates/shell/src/transcript.rs`**

In `crates/shell/src/transcript.rs`, replace:

````rust
    pub fn ended(&self, e: Errno) -> String {
        let name = path::display(self.name.as_bytes());
        format!("sh: {name}: {e}; the transcript ends here\n")
    }
}
````

with:

````rust
    pub fn ended(&self, e: Errno) -> String {
        ended(&self.name, e)
    }
}

/// What the screen says when a write of the transcript `name` failed.
pub fn ended(name: &str, e: Errno) -> String {
    let name = path::display(name.as_bytes());
    format!("sh: {name}: {e}; the transcript ends here\n")
}
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 152 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -m "shell: /bin/sh FILE runs a script as sh does, its commands in its own group and its transcript a console tee; one script may run another"
````


### Task 6: `/bin/sh` quotes a transcript it cannot push as `sh` quotes one it cannot write

The prototype's review (minor): when `/bin/sh FILE` cannot push the transcript, it named it bare (`sh: cannot write the transcript /tmp/my s.log: …`), where `sh` quotes a name it cannot empty as GNU tools do (`'/tmp/my s.log'`). It now uses `quote_if_needed` too.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 5's `run_file`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert!(h.programs.spawned.is_empty());
    }
````

with:

````rust
        assert!(h.programs.spawned.is_empty());
        // Quoted as `sh` quotes it when it cannot empty the transcript.
        h.put("/tmp/my s.sh", b"t-args\n");
        assert_eq!(
            h.sh(&["/tmp/my s.sh"], &mut out),
            (
                1,
                "sh: cannot write the transcript '/tmp/my s.log': Device or resource busy\n".into()
            )
        );
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_transcript_that_cannot_be_pushed_runs_nothing`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::commands::{self, Script};
use crate::ctx::Ctx;
use crate::editor::{Feed, LineEditor};
````

with:

````rust
use crate::commands::{self, Script};
use crate::ctx::{Ctx, quote_if_needed};
use crate::editor::{Feed, LineEditor};
````

Replace:

````rust
        if let Err(e) = programs.tee_push(log.as_bytes()) {
            let shown = path::display(log.as_bytes());
            let message = format!("sh: cannot write the transcript {shown}: {e}\n");
````

with:

````rust
        if let Err(e) = programs.tee_push(log.as_bytes()) {
            let shown = quote_if_needed(&path::display(log.as_bytes()));
            let message = format!("sh: cannot write the transcript {shown}: {e}\n");
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 152 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: /bin/sh quotes a transcript it cannot push as sh quotes one it cannot write"
````


### Task 7: `exit` in a script the shell runs itself ends the script, not the shell

The prototype's review (minor): under the in-process runner (the tests, `host-shell`, the in-kernel shell), `exit` in a script stopped the interactive shell as well, since `exit` and a `reboot` that returned both set `Ran::stop`; under `/bin/sh` the same script is a child shell, and `exit` ends only it (decision 3). `exit` now also sets `Ctx::exited` (`Ran::exited`), and a script the shell runs itself clears the stop that came from it; a `reboot` or `poweroff` that returns still stops the shell.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Tasks 1 (`exit`, `Ran`), 5 (`run_lines`).
- Produces: `Ctx::exited`, `Ran::exited` (crate-private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
    #[test]
    fn reboot_ends_the_script() {
````

with:

````rust
    #[test]
    fn exit_ends_the_script_not_the_shell() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"exit 4\necho after\n");
        assert_eq!(h.run("sh /tmp/s.sh"), (4, "+ exit 4\n".into()));
        // The shell reads on, as when /bin/sh ran the script as its child.
        h.console.type_in(b"sh /tmp/s.sh\recho still\r");
        let mut shell = crate::Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
        shell.run();
        assert_eq!(shell.status(), 0);
        assert!(
            h.console
                .text()
                .ends_with("+ exit 4\nroot@relay:/# echo still\nstill\nroot@relay:/# ")
        );
    }

    #[test]
    fn reboot_ends_the_script() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::script::tests::exit_ends_the_script_not_the_shell`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
    ctx.exit = true;
    status
````

with:

````rust
    ctx.exit = true;
    ctx.exited = true;
    status
````

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub(crate) status: i32,
}
````

with:

````rust
    pub(crate) status: i32,
    /// Set by `exit`: in a script the shell runs itself, only the script
    /// stops.
    pub(crate) exited: bool,
}
````

Replace:

````rust
            status: 0,
        }
````

with:

````rust
            status: 0,
            exited: false,
        }
````

- [ ] **Step 5: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub stop: bool,
    /// Set by `sh`: the script the shell runs next.
````

with:

````rust
    pub stop: bool,
    /// It was `exit`, which stops only a script the shell runs itself.
    pub exited: bool,
    /// Set by `sh`: the script the shell runs next.
````

Replace:

````rust
            stop: false,
            script: None,
````

with:

````rust
            stop: false,
            exited: false,
            script: None,
````

Replace:

````rust
        stop: ctx.exit,
        script: ctx.script.take(),
````

with:

````rust
        stop: ctx.exit,
        exited: ctx.exited,
        script: ctx.script.take(),
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    stopped: bool,
    /// A script's lines are running (`sh`).
````

with:

````rust
    stopped: bool,
    /// The last command was `exit`.
    exited: bool,
    /// A script's lines are running (`sh`).
````

Replace:

````rust
            stopped: false,
            in_script: false,
````

with:

````rust
            stopped: false,
            exited: false,
            in_script: false,
````

Replace:

````rust
        self.stopped = ran.stop;
        let mut status = ran.status;
````

with:

````rust
        self.stopped = ran.stop;
        self.exited = ran.exited;
        let mut status = ran.status;
````

Replace:

````rust
    /// transcript is written by the shell. A script cannot run another.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let status = self.run_lines(&script.text);
        self.write_transcript();
````

with:

````rust
    /// transcript is written by the shell. A script cannot run another.
    /// Its `exit` ends only the script, as it does under `/bin/sh`, where a
    /// script is a shell of its own.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let status = self.run_lines(&script.text);
        if self.exited {
            self.stopped = false;
        }
        self.write_transcript();
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 153 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -m "shell: exit in a script the shell runs itself ends the script, not the shell, as under /bin/sh"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 35 scenario(s) passed`.

````bash
git push -u origin m2p4a/shell
gh pr create --base main --head m2p4a/shell --title "Milestone 2, plan 4a: The shell's runners" --body-file - <<'EOF'
## What

Milestone 2, plan 4a, tasks 1–7: the shell runs only `cd`, `exit` (new, as bash's, ending only a script in one) and `help` itself and hands every other command to a runner: in-process (milestone 1's path: the command table, and programs through `System::spawn`) or spawning (every command a program, its redirection opened in the shell as fd 1, through the new `Programs` trait); `run_command` runs a command as its program will, output on fd 1 (`Stdout`), errors on the console, `<name>: write error: …` as before; `reboot`/`poweroff` report the error that kept the machine up; `/bin/sh FILE` runs a script with its commands in its own group and its transcript a console tee, and one script may run another; `Shell::greet` apart from `run`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4a/shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: One program per command (Tasks 8–11)

`stat` names a file's filesystem (ABI 2), `relay-rt`'s `SysVfs`, `SysConsole`, `SysSystem` and `SysStdout`, and the 23 programs of `/bin`, each printing what the shell's command prints, run by path from the in-kernel shell.

Branch `m2p4a/utils`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-utils`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4a/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-utils origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-utils
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4a/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4a/utils /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-utils m2p4a/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4a/shell>` and re-run `cargo xtask ci` before pushing.

### Task 8: `stat` says which filesystem a file is on; ABI 2

Decision 7: a program names a file by what `stat` gives, and inode numbers alone collide between `/bin` and the root (a fresh filesystem numbers its first files as the root's first ones were), so `relay_abi::Stat` gains `dev` at offset 72 (80 bytes, no padding): the mount's number from 1, the same for every file of one filesystem, and 0 for the console. The kernel's `stat` and `fstat` fill it (`file::stat_of` takes the node). A changed layout changes `relay_abi::VERSION` (spec §7.4): it is 2, so the startup line says `ABI 2`, `system_abi` says `kernel wants 2`, `check3-a.sh` expects `ABI 2`, and its recorded transcripts get it by commands (they hold escape bytes) until plan 4b's NUC check records real ones.

**Files:**
- Modify: `crates/relay-abi/src/file.rs`
- Modify: `crates/relay-abi/src/lib.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/file.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/system.rs`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `tests/e2e/system_abi.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/fixtures/checks/check3-b.nuc.log`

**Interfaces:**
- Consumes: plan 3b's `relay_abi::Stat`, `file::stat_of`.
- Produces: `relay_abi::Stat::dev: u64`; `relay_abi::VERSION == 2`; `file::stat_of(node: Node, s: &vfs::Stat) -> relay_abi::Stat`.

- [ ] **Step 1: Add the failing tests to `crates/relay-abi/src/file.rs`**

In `crates/relay-abi/src/file.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    fn the_layouts_are_fixed() {
        assert_eq!(Stat::SIZE, 72);
        assert_eq!(offset_of!(Stat, ino), 0);
````

with:

````rust
    fn the_layouts_are_fixed() {
        assert_eq!(Stat::SIZE, 80);
        assert_eq!(offset_of!(Stat, ino), 0);
````

Replace:

````rust
        assert_eq!(offset_of!(Stat, block_size), 68);
        assert_eq!(StatFs::SIZE, 48);
````

with:

````rust
        assert_eq!(offset_of!(Stat, block_size), 68);
        assert_eq!(offset_of!(Stat, dev), 72);
        assert_eq!(StatFs::SIZE, 48);
````

Replace:

````rust
            block_size: 12,
        };
````

with:

````rust
            block_size: 12,
            dev: 13,
        };
````

- [ ] **Step 2: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
                block_size: s.block_size,
            }
````

with:

````rust
                block_size: s.block_size,
                dev: 1,
            }
````

- [ ] **Step 3: Add the failing tests to `kernel/src/system.rs`**

In `kernel/src/system.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 1");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
        assert_eq!(vfs.stat(node).unwrap().perm, 0o755);
        let mut one = table(true);
        assert_eq!(
            mount_into(&mut one, archive(1, &["t-args"])).unwrap(),
            "1 program, ABI 1"
        );
````

with:

````rust
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 2");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
        assert_eq!(vfs.stat(node).unwrap().perm, 0o755);
        let mut one = table(true);
        assert_eq!(
            mount_into(&mut one, archive(relay_abi::VERSION, &["t-args"])).unwrap(),
            "1 program, ABI 2"
        );
````

Replace:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 1");
        assert_eq!(
````

with:

````rust
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 2");
        assert_eq!(
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 1
expect \nWelcome to Relay OS\.\n
````

with:

````text
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 2
expect \nWelcome to Relay OS\.\n
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, replace:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 1
expect root@relay:~# $
````

with:

````text
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 2
expect root@relay:~# $
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/system_abi.txt`**

In `tests/e2e/system_abi.txt`, replace:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 1
expect root@relay:~# $
````

with:

````text
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 2
expect root@relay:~# $
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p relay-abi -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no field `dev` on type `file::Stat` ``; `` struct `file::Stat` has no field named `dev` ``.

- [ ] **Step 8: Change `crates/relay-abi/src/file.rs`**

In `crates/relay-abi/src/file.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub block_size: u32,
}
````

with:

````rust
    pub block_size: u32,
    /// The filesystem it is on: the mount's number from 1, the same for
    /// every file of one filesystem; 0 for the console. With `ino` it names
    /// the file (user-space gate §16 item 5).
    pub dev: u64,
}
````

Replace:

````rust
            b[48 + 4 * i..52 + 4 * i].copy_from_slice(&v.to_ne_bytes());
        }
        b
    }
````

with:

````rust
            b[48 + 4 * i..52 + 4 * i].copy_from_slice(&v.to_ne_bytes());
        }
        b[72..80].copy_from_slice(&self.dev.to_ne_bytes());
        b
    }
````

- [ ] **Step 9: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
/// header and into every program's ELF note, and checked by the kernel.
pub const VERSION: u32 = 1;

````

with:

````rust
/// header and into every program's ELF note, and checked by the kernel.
pub const VERSION: u32 = 2;

````

- [ ] **Step 10: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 9 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 9 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 11: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

/// `stat`'s answer for a program.
pub fn stat_of(s: &vfs::Stat) -> relay_abi::Stat {
    relay_abi::Stat {
````

with:

````rust

/// `stat`'s answer for a program about the file `node`: its filesystem is
/// its mount's number from 1 (0 is the console).
pub fn stat_of(node: Node, s: &vfs::Stat) -> relay_abi::Stat {
    relay_abi::Stat {
````

Replace:

````rust
        block_size: s.block_size,
    }
````

with:

````rust
        block_size: s.block_size,
        dev: node.mount as u64 + 1,
    }
````

Replace:

````rust
        self.offset()?;
        Ok(stat_of(&vfs.stat(self.node)?))
    }
````

with:

````rust
        self.offset()?;
        Ok(stat_of(self.node, &vfs.stat(self.node)?))
    }
````

- [ ] **Step 12: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    let slice = UserSlice::new(out, Stat::SIZE as u64)?;
    let st = caller.with_vfs(|v| v.lookup(&path).and_then(|n| v.stat(n)))?;
    caller.write(&slice, 0, &open_file::stat_of(&st).to_bytes())?;
    Ok(0)
````

with:

````rust
    let slice = UserSlice::new(out, Stat::SIZE as u64)?;
    let (node, st) = caller.with_vfs(|v| v.lookup(&path).and_then(|n| Ok((n, v.stat(n)?))))?;
    caller.write(&slice, 0, &open_file::stat_of(node, &st).to_bytes())?;
    Ok(0)
````

- [ ] **Step 13: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 29 programs, ABI 1` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 33 programs, ABI 2` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.
````

- [ ] **Step 14: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 1
#> ...
````

with:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 2\.0 GiB
#> \[ ok \] system: \d+ programs?, ABI 2
#> ...
````

- [ ] **Step 15: Put ABI 2 into the recorded NUC transcript of `check3-a.sh`**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
text = open(path, newline="").read()
assert text.count(", ABI 1\n") == 1, path
open(path, "w", newline="").write(text.replace(", ABI 1\n", ", ABI 2\n"))
EOF
````

- [ ] **Step 16: Change `xtask/fixtures/checks/check3-a.qemu.log`**

In `xtask/fixtures/checks/check3-a.qemu.log`, replace:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] system: 1 program, ABI 1
+ ls -l /bin
````

with:

````text
[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB
[ ok ] system: 1 program, ABI 2
+ ls -l /bin
````

- [ ] **Step 17: Put ABI 2 into the recorded NUC transcript of `check3-b.sh`**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-b.nuc.log'
text = open(path, newline="").read()
assert text.count(", ABI 1\n") == 1, path
open(path, "w", newline="").write(text.replace(", ABI 1\n", ", ABI 2\n"))
EOF
````

- [ ] **Step 18: Run the tests to see them pass**

Run: `cargo test -p relay-abi -p relay-kernel --lib`

Expected: PASS: 363 tests.

Run: `cargo test -p xtask checks`

Expected: PASS: 11 tests.

- [ ] **Step 19: Run the `boot`, `system`, `system_abi`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 20: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 21: Commit**

````bash
git add crates docs kernel rootfs tests xtask
git commit -m "relay-abi, kernel: stat and fstat say which filesystem a file is on (Stat::dev); ABI 2"
````


### Task 9: `stat` and `fstat` name the filesystem, tested

The prototype's review (minor): no test read `Stat::dev` from the kernel (the dispatcher's tests read the first 72 bytes, and the open files' test expected the root's 1, which a constant gives), so a kernel that gave every file `dev` 1 passed everything, and `SysVfs` would then take a program of `/bin` and a file of the root with one inode number for the same file. The dispatcher's tests now read `dev` for the root's file (1), the filesystem mounted at `/full` (2) by path and by fd, and the console (0). This task adds only a test of what Task 8 does, so it has no failing run; the mutation checks show what it guards: `dev` always 1, and a console with a `dev`, fail it.

**Files:**
- Modify: `kernel/src/syscall/files.rs`

**Interfaces:**
- Consumes: Task 8's `Stat::dev`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    #[test]
    fn stat_names_a_file_by_its_path_and_never_follows_a_link() {
````

with:

````rust
    #[test]
    fn stat_and_fstat_name_the_filesystem() {
        let mut f = fake();
        let dev = |f: &mut Fake| u64_at(&get(f, W, relay_abi::Stat::SIZE), 72);
        assert_eq!(on(&mut f, Call::Stat, b"/root/f", [0, W]), Ok(0));
        assert_eq!(dev(&mut f), 1, "the root's");
        assert_eq!(on(&mut f, Call::Stat, b"/full", [0, W]), Ok(0));
        assert_eq!(dev(&mut f), 2, "the filesystem mounted at /full");
        let fd = open(&mut f, b"/full", OPEN_READ).unwrap();
        assert_eq!(call(&mut f, Call::Fstat, [fd, W, 0]), Ok(0));
        assert_eq!(dev(&mut f), 2);
        assert_eq!(call(&mut f, Call::Fstat, [0, W, 0]), Ok(0));
        assert_eq!(dev(&mut f), 0, "the console's");
    }

    #[test]
    fn stat_names_a_file_by_its_path_and_never_follows_a_link() {
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 341 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add kernel
git commit -m "Tests: stat and fstat name the filesystem, the console's 0"
````


### Task 10: `SysVfs`: the shell's `Vfs` over the file calls

Spec §8.1 and decision 6: `relay-rt` gains `sysvfs.rs`, the shell's `vfs::Vfs` over the file calls, so a command function runs unchanged in a program. It goes by path: a `Node` is the file's filesystem and inode from `stat` (`node_of`, Task 8), and `SysVfs` remembers the path it found each node by and opens that path for each operation (`read_at` and `write_at` open, seek, read or write, and close), so a command holds no fd between calls and always reaches the file the path names now. `read_dir` reads the records until the kernel says it is done, and remembers their kinds for `entry_kind`; a call that gives bytes but no record, or more than 2^18 entries, is `EIO` rather than a listing that grows for ever. `shutdown` does nothing: the kernel's `power` shuts the filesystems down. The calls are a trait (`Calls`, with `Sys` the program's), so the tests run `SysVfs` over the calls of a `MountTable` (`testing.rs`'s `FakeCalls`), where every command prints what it prints over the table itself. `vfs::Errno` gains `ALL` and `from_number`, for the calls' numbers. `relay-rt` now depends on `shell` and `vfs`. Mutation checks: a node without its filesystem, an fd left open, `read_dir` without its bound (the test binary then runs out of its 10 GiB), `read_at` without its seek, and `create` without `EXCLUSIVE` each fail a test.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/relay-rt/Cargo.toml`
- Modify: `crates/relay-rt/src/lib.rs`
- Create: `crates/relay-rt/src/sysvfs.rs`
- Create: `crates/relay-rt/src/testing.rs`
- Modify: `crates/vfs/src/errno.rs`

**Interfaces:**
- Consumes: Task 8's `Stat::dev`; Task 4's `run_command`, `commands::find`; plan 3b's `relay_rt::sys` wrappers and `relay_abi::file::dir_entries`.
- Produces: `vfs::Errno::{ALL, from_number(u16) -> Errno}`; `relay_rt::sysvfs::{Calls (open, close, read, write, seek, fstat, stat, read_dir, mkdir, rmdir, unlink, touch, truncate, readlink, rename, statfs, sync, chdir, getcwd, all by `&self`), Sys, SysVfs<C: Calls = Sys>::{new(), with(C), calls(&self) -> &C}, node_of(&relay_abi::Stat) -> Node}`, re-exported as `relay_rt::SysVfs`; `relay_rt`'s test support `testing::{FakeCalls, memfs, NOW}`.

- [ ] **Step 1: Change `crates/relay-rt/Cargo.toml`**

In `crates/relay-rt/Cargo.toml`, replace:

````toml
spin.workspace = true
````

with:

````toml
spin.workspace = true
shell.workspace = true
vfs.workspace = true
````

- [ ] **Step 2: Change `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

mod allocator;
````

with:

````rust

extern crate alloc;

mod allocator;
````

Replace:

````rust
pub mod sys;

pub use args::Args;
pub use start::name;

````

with:

````rust
pub mod sys;
pub mod sysvfs;
mod testing;

pub use args::Args;
pub use start::name;
pub use sysvfs::SysVfs;

````

- [ ] **Step 3: Write the failing tests for `crates/relay-rt/src/sysvfs.rs`**

Create `crates/relay-rt/src/sysvfs.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeCalls, memfs};
    use alloc::boxed::Box;
    use alloc::string::String;
    use shell::{Console, MemInfo, Stdout, System};
    use vfs::{FileSystem, MountTable};

    /// `/root/a`, `/root/sub/`, `/tmp/`, and at `/bin` a filesystem of its
    /// own whose `x` has the same inode number as `/root/a`.
    fn tree() -> MountTable {
        let mut t = MountTable::new(Box::new(memfs()));
        for d in ["/root", "/root/sub", "/tmp", "/bin"] {
            t.mkdir(d.as_bytes()).unwrap();
        }
        let a = t.create(b"/root/a").unwrap();
        t.write_at(a, 0, b"one\ntwo\nthree\n").unwrap();
        // A fresh MemFs numbers its files as the root's first ones were.
        let mut bin = memfs();
        let root = bin.root();
        for d in ["r", "s", "t", "b"] {
            bin.mkdir(root, d.as_bytes()).unwrap();
        }
        let x = bin.create(root, b"x").unwrap();
        bin.write_at(x, 0, b"program\n").unwrap();
        assert_eq!(x, a.ino, "the same inode number on both");
        t.mount(b"/bin", Box::new(bin)).unwrap();
        t
    }

    fn sysvfs() -> SysVfs<FakeCalls> {
        SysVfs::with(FakeCalls::new(tree()))
    }

    #[derive(Default)]
    struct Screen(Vec<u8>);

    impl Console for Screen {
        fn read_byte(&mut self) -> Option<u8> {
            None
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        fn columns(&self) -> usize {
            80
        }
    }

    impl Stdout for Screen {
        fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
        fn is_tty(&self) -> bool {
            true
        }
        fn node(&self) -> Option<Node> {
            None
        }
    }

    struct Clock;

    impl System for Clock {
        fn now(&self) -> u64 {
            crate::testing::NOW
        }
        fn memory(&self) -> Option<MemInfo> {
            None
        }
        fn kernel_log(&self) -> Vec<u8> {
            Vec::new()
        }
        fn reboot(&mut self, _: bool) -> Result<(), Errno> {
            Ok(())
        }
        fn poweroff(&mut self, _: bool) -> Result<(), Errno> {
            Ok(())
        }
    }

    /// Runs `line` as its program does over `vfs`: the status, the errors
    /// and the output.
    fn run(vfs: &mut dyn Vfs, line: &str) -> (i32, String, String) {
        let words = shell::parser::parse(line).unwrap().words;
        let (mut errors, mut out) = (Screen::default(), Screen::default());
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
        (status, text(errors), text(out))
    }

    #[test]
    fn commands_do_through_the_calls_what_they_do_over_the_mount_table() {
        let mut direct = tree();
        let mut sys = sysvfs();
        for line in [
            "ls -la /root",
            "ls -l /",
            "ls /bin",
            "cat /root/a /bin/x /nope",
            "wc /root/a",
            "head -n 1 /root/a",
            "tail -n 2 /root/a",
            "stat /root/a /bin",
            "df",
            "mkdir /root/d /root/d",
            "touch /root/d/x /root/a",
            "cp /root/a /root/b",
            "cp /root/a /root/a",
            "cp /bin/x /root/a",
            "cat /root/a",
            "mv /root/b /root/d/",
            "ls -l /root/d",
            "rm /root/sub",
            "rm -r /root/d /nope",
            "rm -r /",
            "rmdir /tmp /root/sub",
            "rm /bin/x",
            "cat /root/sub",
            "ls /root",
        ] {
            assert_eq!(run(&mut sys, line), run(&mut direct, line), "{line}");
        }
    }

    #[test]
    fn a_file_is_one_node_whatever_its_path() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        assert_eq!(v.lookup(b"/root/../root/sub/../a").unwrap(), a);
        v.chdir(b"/root").unwrap();
        assert_eq!(v.lookup(b"a").unwrap(), a);
        assert_eq!(v.cwd(), b"/root");
    }

    #[test]
    fn files_of_two_filesystems_are_never_one_node() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        let x = v.lookup(b"/bin/x").unwrap();
        assert_eq!(a.ino, x.ino);
        assert_ne!(a, x);
        assert_eq!(run(&mut v, "cp /bin/x /root/a").0, 0, "not the same file");
        assert_eq!(run(&mut v, "cat /root/a").2, "program\n");
    }

    #[test]
    fn a_command_holds_no_fd_between_calls() {
        let mut v = sysvfs();
        for line in [
            "cat /root/a",
            "cp /root/a /tmp/c",
            "ls -l /root",
            "rm -r /tmp",
        ] {
            run(&mut v, line);
            assert_eq!(v.calls().open_now.get(), 0, "{line}");
        }
        assert_eq!(v.calls().open_most.get(), 1);
    }

    #[test]
    fn a_big_directory_is_listed_over_many_calls() {
        let mut v = sysvfs();
        for i in 0..300 {
            v.create(alloc::format!("/tmp/f{i:03}").as_bytes()).unwrap();
        }
        v.mkdir(b"/tmp/zdir").unwrap();
        v.calls().dir_chunk.set(200);
        let tmp = v.lookup(b"/tmp").unwrap();
        let entries = v.read_dir(tmp).unwrap();
        assert_eq!(entries.len(), 303, ". and .. too");
        let reads = v
            .calls()
            .calls
            .borrow()
            .iter()
            .filter(|c| **c == "read_dir")
            .count();
        assert!(reads > 30, "{reads} calls");
        let zdir = entries.iter().find(|e| e.name == b"zdir").unwrap();
        assert_eq!(v.entry_kind(tmp, zdir).unwrap(), FileType::Directory);
        let f = entries.iter().find(|e| e.name == b"f123").unwrap();
        assert_eq!(v.entry_kind(tmp, f).unwrap(), FileType::Regular);
        // A mount point is a directory in its parent.
        let root = v.lookup(b"/").unwrap();
        let bin = v
            .read_dir(root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == b"bin")
            .unwrap();
        assert_eq!(v.entry_kind(root, &bin).unwrap(), FileType::Directory);
    }

    #[test]
    fn a_directory_that_never_ends_is_eio() {
        let mut v = sysvfs();
        v.calls().dir_repeats.set(true);
        let root = v.lookup(b"/root").unwrap();
        assert_eq!(v.read_dir(root), Err(Errno::EIO));
        assert_eq!(v.calls().open_now.get(), 0);
    }

    #[test]
    fn reads_and_writes_go_by_offset() {
        let mut v = sysvfs();
        let a = v.lookup(b"/root/a").unwrap();
        let mut buf = [0; 3];
        assert_eq!(v.read_at(a, 4, &mut buf), Ok(3));
        assert_eq!(&buf, b"two");
        assert_eq!(v.write_at(a, 4, b"TWO"), Ok(3));
        assert_eq!(v.read_at(a, 0, &mut [0; 64]), Ok(14));
        assert_eq!(run(&mut v, "cat /root/a").2, "one\nTWO\nthree\n");
    }

    #[test]
    fn errors_are_the_calls_own() {
        let mut v = sysvfs();
        assert_eq!(v.lookup(b"/nope"), Err(Errno::ENOENT));
        assert_eq!(v.lookup(b"/root/a/b"), Err(Errno::ENOTDIR));
        assert_eq!(v.create(b"/root/a"), Err(Errno::EEXIST));
        let a = v.lookup(b"/root/a").unwrap();
        assert_eq!(v.read_dir(a), Err(Errno::ENOTDIR));
        let sub = v.lookup(b"/root/sub").unwrap();
        assert_eq!(v.write_at(sub, 0, b"x"), Err(Errno::EISDIR));
        // A node this SysVfs never gave names no file.
        let stranger = Node { mount: 9, ino: 9 };
        assert_eq!(v.stat(stranger), Err(Errno::ENOENT));
    }

    #[test]
    fn the_kernel_shuts_the_filesystems_down() {
        let mut v = sysvfs();
        assert_eq!(v.shutdown(), Ok(()));
        assert!(v.calls().calls.borrow().is_empty(), "no call");
    }
}
````

- [ ] **Step 4: Create `crates/relay-rt/src/testing.rs`**

Create `crates/relay-rt/src/testing.rs`:

````rust
//! The file calls over a `vfs::MountTable` of `MemFs`s, for testing
//! `SysVfs` on the host: fds with offsets, `read_dir`'s records in name
//! order, and each call's errors where a test needs them.
#![cfg(test)]

use crate::sysvfs::Calls;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_TRUNCATE, OPEN_WRITE,
    put_dir_entry,
};
use vfs::{Env, Errno, FileType, MemFs, MountTable, Node, Vfs};

/// The tests' time: Sat Sep 26 12:00:00 UTC 2026.
pub const NOW: u64 = 1_790_424_000;

struct Clock;

impl Env for Clock {
    fn now(&self) -> u64 {
        NOW
    }
    fn log(&self, _: &str) {}
}

pub fn memfs() -> MemFs {
    MemFs::new(Box::new(Clock))
}

struct Open {
    node: Node,
    offset: u64,
    /// The last name `read_dir` gave.
    after: Option<Vec<u8>>,
}

pub struct FakeCalls {
    pub table: RefCell<MountTable>,
    fds: RefCell<Vec<Option<Open>>>,
    /// How many fds are open now, and the most there ever were.
    pub open_now: Cell<usize>,
    pub open_most: Cell<usize>,
    /// Every call made, by name.
    pub calls: RefCell<Vec<&'static str>>,
    /// `read_dir` gives at most this many bytes a call.
    pub dir_chunk: Cell<usize>,
    /// `read_dir` never goes on: it gives the first records every time.
    pub dir_repeats: Cell<bool>,
}

fn kind(t: FileType) -> u8 {
    match t {
        FileType::Regular => KIND_REGULAR,
        FileType::Directory => KIND_DIRECTORY,
        FileType::Symlink => KIND_SYMLINK,
        FileType::CharDev => KIND_CHAR_DEVICE,
        FileType::BlockDev => KIND_BLOCK_DEVICE,
        FileType::Fifo => KIND_FIFO,
        FileType::Socket => KIND_SOCKET,
    }
}

impl FakeCalls {
    pub fn new(table: MountTable) -> FakeCalls {
        FakeCalls {
            table: RefCell::new(table),
            fds: RefCell::new(Vec::new()),
            open_now: Cell::new(0),
            open_most: Cell::new(0),
            calls: RefCell::new(Vec::new()),
            dir_chunk: Cell::new(usize::MAX),
            dir_repeats: Cell::new(false),
        }
    }

    fn called(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }

    fn stat_of(&self, node: Node) -> Result<relay_abi::Stat, Errno> {
        let s = self.table.borrow_mut().stat(node)?;
        Ok(relay_abi::Stat {
            ino: s.ino,
            size: s.size,
            blocks: s.blocks,
            atime: s.atime,
            mtime: s.mtime,
            ctime: s.ctime,
            kind: u32::from(kind(s.kind)),
            perm: u32::from(s.perm),
            nlink: s.nlink,
            uid: s.uid,
            gid: s.gid,
            block_size: s.block_size,
            dev: node.mount as u64 + 1,
        })
    }

    fn with_open<R>(&self, fd: u32, f: impl FnOnce(&mut Open) -> R) -> Result<R, Errno> {
        let mut fds = self.fds.borrow_mut();
        let open = fds
            .get_mut(fd as usize)
            .and_then(Option::as_mut)
            .ok_or(Errno::EBADF)?;
        Ok(f(open))
    }
}

impl Calls for FakeCalls {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno> {
        self.called("open");
        let mut t = self.table.borrow_mut();
        let node = if flags & OPEN_CREATE != 0 {
            match t.lookup(path) {
                Ok(_) if flags & OPEN_EXCLUSIVE != 0 => return Err(Errno::EEXIST),
                Ok(node) => node,
                Err(Errno::ENOENT) => t.create(path)?,
                Err(e) => return Err(e),
            }
        } else {
            t.lookup(path)?
        };
        let st = t.stat(node)?;
        let dir = st.kind == FileType::Directory;
        if flags & OPEN_DIRECTORY != 0 && !dir {
            return Err(Errno::ENOTDIR);
        }
        if flags & OPEN_WRITE != 0 && dir {
            return Err(Errno::EISDIR);
        }
        if flags & OPEN_TRUNCATE != 0 {
            t.truncate(node, 0)?;
        }
        let mut fds = self.fds.borrow_mut();
        let open = Open {
            node,
            offset: 0,
            after: None,
        };
        let fd = match fds.iter().position(Option::is_none) {
            Some(i) => {
                fds[i] = Some(open);
                i
            }
            None => {
                fds.push(Some(open));
                fds.len() - 1
            }
        };
        self.open_now.set(self.open_now.get() + 1);
        self.open_most
            .set(self.open_most.get().max(self.open_now.get()));
        Ok(fd as u32)
    }

    fn close(&self, fd: u32) {
        self.called("close");
        if let Some(slot) = self.fds.borrow_mut().get_mut(fd as usize)
            && slot.take().is_some()
        {
            self.open_now.set(self.open_now.get() - 1);
        }
    }

    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("read");
        let (node, offset) = self.with_open(fd, |o| (o.node, o.offset))?;
        let n = self.table.borrow_mut().read_at(node, offset, buf)?;
        self.with_open(fd, |o| o.offset += n as u64)?;
        Ok(n)
    }

    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno> {
        self.called("write");
        let (node, offset) = self.with_open(fd, |o| (o.node, o.offset))?;
        let n = self.table.borrow_mut().write_at(node, offset, bytes)?;
        self.with_open(fd, |o| o.offset += n as u64)?;
        Ok(n)
    }

    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno> {
        self.called("seek");
        self.with_open(fd, |o| o.offset = offset)
    }

    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno> {
        self.called("fstat");
        let node = self.with_open(fd, |o| o.node)?;
        self.stat_of(node)
    }

    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno> {
        self.called("stat");
        let node = self.table.borrow_mut().lookup(path)?;
        self.stat_of(node)
    }

    /// Records in name order, from the one after the last name given, as
    /// many as fit in `buf` and `dir_chunk`.
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("read_dir");
        let (node, after) = self.with_open(fd, |o| (o.node, o.after.clone()))?;
        let mut t = self.table.borrow_mut();
        let mut entries = t.read_dir(node)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let limit = buf.len().min(self.dir_chunk.get());
        let mut done = 0;
        let mut last = None;
        for e in entries
            .iter()
            .filter(|e| after.as_ref().is_none_or(|a| e.name > *a))
        {
            let k = kind(t.entry_kind(node, e)?);
            match put_dir_entry(&mut buf[done..limit], e.ino, k, &e.name) {
                Some(n) => done += n,
                None => break,
            }
            last = Some(e.name.clone());
        }
        if last.is_some() && !self.dir_repeats.get() {
            self.with_open(fd, |o| o.after = last)?;
        }
        Ok(done)
    }

    fn mkdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("mkdir");
        self.table.borrow_mut().mkdir(path)
    }

    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("rmdir");
        self.table.borrow_mut().rmdir(path)
    }

    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("unlink");
        self.table.borrow_mut().unlink(path)
    }

    fn touch(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("touch");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        t.touch(node)
    }

    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno> {
        self.called("truncate");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        t.truncate(node, size)
    }

    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("readlink");
        let mut t = self.table.borrow_mut();
        let node = t.lookup(path)?;
        let target = t.read_link(node)?;
        let n = target.len().min(buf.len());
        buf[..n].copy_from_slice(&target[..n]);
        Ok(n)
    }

    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.called("rename");
        self.table.borrow_mut().rename(from, to)
    }

    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno> {
        self.called("statfs");
        let s = self.table.borrow_mut().statfs(path)?;
        Ok(relay_abi::StatFs {
            block_size: s.block_size,
            blocks: s.blocks,
            free_blocks: s.free_blocks,
            avail_blocks: s.avail_blocks,
            files: s.files,
            free_files: s.free_files,
        })
    }

    fn sync(&self) -> Result<(), Errno> {
        self.called("sync");
        self.table.borrow_mut().sync()
    }

    fn chdir(&self, path: &[u8]) -> Result<(), Errno> {
        self.called("chdir");
        self.table.borrow_mut().chdir(path)
    }

    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        self.called("getcwd");
        let cwd = self.table.borrow().cwd();
        let dst = buf.get_mut(..cwd.len()).ok_or(Errno::ERANGE)?;
        dst.copy_from_slice(&cwd);
        Ok(cwd.len())
    }
}
````

- [ ] **Step 5: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 26] = [
        Errno::ENOENT,
        Errno::EEXIST,
        Errno::ENOTDIR,
        Errno::EISDIR,
        Errno::ENOTEMPTY,
        Errno::ENOSPC,
        Errno::EIO,
        Errno::EROFS,
        Errno::EINVAL,
        Errno::ENAMETOOLONG,
        Errno::EXDEV,
        Errno::EBUSY,
        Errno::EFBIG,
        Errno::E2BIG,
        Errno::ENOEXEC,
        Errno::EBADF,
        Errno::ECHILD,
        Errno::EAGAIN,
        Errno::ENOMEM,
        Errno::EFAULT,
        Errno::ENOSYS,
        Errno::ESRCH,
        Errno::EPERM,
        Errno::EINTR,
        Errno::EMFILE,
        Errno::ERANGE,
    ];

    /// The name, number and message the host's C library gives: the real
    /// thing, independent of `relay_abi`'s table.
    #[test]
    fn each_number_and_message_is_the_host_s() {
        for e in ALL {
            let n = e.number();
````

with:

````rust

    /// The name, number and message the host's C library gives: the real
    /// thing, independent of `relay_abi`'s table.
    #[test]
    fn each_number_and_message_is_the_host_s() {
        for e in Errno::ALL {
            let n = e.number();
````

Replace:

````rust
        }
        let mut numbers: Vec<u16> = ALL.iter().map(|e| e.number()).collect();
        numbers.sort();
        numbers.dedup();
        assert_eq!(numbers.len(), ALL.len(), "numbers are unique");
    }
````

with:

````rust
        }
        let mut numbers: Vec<u16> = Errno::ALL.iter().map(|e| e.number()).collect();
        numbers.sort();
        numbers.dedup();
        assert_eq!(numbers.len(), Errno::ALL.len(), "numbers are unique");
    }

    #[test]
    fn a_number_names_its_error() {
        for e in Errno::ALL {
            assert_eq!(Errno::from_number(e.number()), e);
        }
        assert_eq!(Errno::from_number(0), Errno::EIO);
        assert_eq!(Errno::from_number(4095), Errno::EIO);
    }
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p vfs -p relay-rt`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `ALL` found for enum `errno::Errno` in the current scope ``; `` no variant, associated function, or constant named `from_number` found for enum `errno::Errno` in the current scope ``.

- [ ] **Step 7: Implement `crates/relay-rt/src/sysvfs.rs`**

Insert this at the top of `crates/relay-rt/src/sysvfs.rs`, above `#[cfg(test)]`:

````rust
//! `SysVfs`: the shell's `vfs::Vfs` over the file calls (user-space gate
//! §8.1), so a command function runs unchanged in a program.
//!
//! The `Vfs` names files by `Node`, the program's calls by path or fd.
//! `SysVfs` goes by path: a `Node` is the file's filesystem and inode, as
//! `stat` gives them (`Stat::dev`, `Stat::ino`), so the same file is always
//! the same node (`cp a a`, `rm -r /`) and files of two filesystems never
//! are; it remembers the path each node was found by, and every operation
//! on a node opens that path, works and closes it. A command then holds
//! no fd between calls, and always reaches the file the path names now.

use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_START,
    STAT_NOFOLLOW, dir_entries,
};
use vfs::{DirEntry, Errno, FileType, Node, Stat, StatFs, Vfs};

/// The longest path `getcwd` gives (the kernel's limit, spec §16 item 3).
const PATH_MAX: usize = 4096;
/// `read_dir` is asked for this much at a time.
const DIR_BUFFER: usize = 16 * 1024;
/// A directory with more entries than this is refused (`EIO`), so a
/// kernel that never says it is done cannot make a listing grow without
/// end.
const DIR_ENTRIES_MAX: usize = 1 << 18;

/// The calls `SysVfs` makes: the program's (`Sys`), or a fake in tests.
pub trait Calls {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno>;
    fn close(&self, fd: u32);
    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno>;
    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno>;
    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno>;
    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno>;
    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno>;
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno>;
    fn mkdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn rmdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn unlink(&self, path: &[u8]) -> Result<(), Errno>;
    fn touch(&self, path: &[u8]) -> Result<(), Errno>;
    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno>;
    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno>;
    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno>;
    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno>;
    fn sync(&self) -> Result<(), Errno>;
    fn chdir(&self, path: &[u8]) -> Result<(), Errno>;
    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno>;
}

/// The program's own calls.
pub struct Sys;

fn errno(e: u16) -> Errno {
    Errno::from_number(e)
}

impl Calls for Sys {
    fn open(&self, path: &[u8], flags: u32) -> Result<u32, Errno> {
        crate::sys::open(path, flags).map_err(errno)
    }
    fn close(&self, fd: u32) {
        let _ = crate::sys::close(fd);
    }
    fn read(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::read(fd, buf).map_err(errno)
    }
    fn write(&self, fd: u32, bytes: &[u8]) -> Result<usize, Errno> {
        crate::sys::write(fd, bytes).map_err(errno)
    }
    fn seek(&self, fd: u32, offset: u64) -> Result<(), Errno> {
        let offset = i64::try_from(offset).map_err(|_| Errno::EINVAL)?;
        crate::sys::seek(fd, offset, SEEK_START)
            .map(|_| ())
            .map_err(errno)
    }
    fn fstat(&self, fd: u32) -> Result<relay_abi::Stat, Errno> {
        crate::sys::fstat(fd).map_err(errno)
    }
    fn stat(&self, path: &[u8]) -> Result<relay_abi::Stat, Errno> {
        crate::sys::stat(path, STAT_NOFOLLOW).map_err(errno)
    }
    fn read_dir(&self, fd: u32, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::read_dir(fd, buf).map_err(errno)
    }
    fn mkdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::mkdir(path).map_err(errno)
    }
    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::rmdir(path).map_err(errno)
    }
    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::unlink(path).map_err(errno)
    }
    fn touch(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::touch(path).map_err(errno)
    }
    fn truncate(&self, path: &[u8], size: u64) -> Result<(), Errno> {
        crate::sys::truncate(path, size).map_err(errno)
    }
    fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::readlink(path, buf).map_err(errno)
    }
    fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        crate::sys::rename(from, to).map_err(errno)
    }
    fn statfs(&self, path: &[u8]) -> Result<relay_abi::StatFs, Errno> {
        crate::sys::statfs(path).map_err(errno)
    }
    fn sync(&self) -> Result<(), Errno> {
        crate::sys::sync().map_err(errno)
    }
    fn chdir(&self, path: &[u8]) -> Result<(), Errno> {
        crate::sys::chdir(path).map_err(errno)
    }
    fn getcwd(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        crate::sys::getcwd(buf).map_err(errno)
    }
}

/// The node `stat` describes: its filesystem and inode.
pub fn node_of(st: &relay_abi::Stat) -> Node {
    Node {
        mount: st.dev as usize,
        ino: st.ino,
    }
}

fn file_type(kind: u32) -> FileType {
    match u8::try_from(kind) {
        Ok(KIND_REGULAR) => FileType::Regular,
        Ok(KIND_DIRECTORY) => FileType::Directory,
        Ok(KIND_SYMLINK) => FileType::Symlink,
        Ok(KIND_CHAR_DEVICE) => FileType::CharDev,
        Ok(KIND_BLOCK_DEVICE) => FileType::BlockDev,
        Ok(KIND_FIFO) => FileType::Fifo,
        Ok(KIND_SOCKET) => FileType::Socket,
        // The kernel names only the kinds above.
        _ => FileType::Regular,
    }
}

/// A program's `Stat` as the `Vfs` gives it.
fn stat_from(st: &relay_abi::Stat) -> Stat {
    Stat {
        ino: st.ino,
        kind: file_type(st.kind),
        perm: (st.perm & 0o7777) as u16,
        nlink: st.nlink,
        uid: st.uid,
        gid: st.gid,
        size: st.size,
        blocks: st.blocks,
        block_size: st.block_size,
        atime: st.atime,
        mtime: st.mtime,
        ctime: st.ctime,
    }
}

/// The shell's `Vfs` over `C`'s calls.
pub struct SysVfs<C: Calls = Sys> {
    calls: C,
    /// Every node found so far and the path it was found by.
    paths: RefCell<Vec<(Node, Vec<u8>)>>,
    /// The kinds of the entries of the directory listed last, from its
    /// records (`entry_kind`).
    kinds: Vec<(Node, Vec<u8>, FileType)>,
}

impl SysVfs<Sys> {
    pub fn new() -> SysVfs<Sys> {
        SysVfs::with(Sys)
    }
}

impl Default for SysVfs<Sys> {
    fn default() -> SysVfs<Sys> {
        SysVfs::new()
    }
}

impl<C: Calls> SysVfs<C> {
    pub fn with(calls: C) -> SysVfs<C> {
        SysVfs {
            calls,
            paths: RefCell::new(Vec::new()),
            kinds: Vec::new(),
        }
    }

    pub fn calls(&self) -> &C {
        &self.calls
    }

    /// Remembers that `node` is at `path`, the latest path it was found by.
    fn found(&self, node: Node, path: &[u8]) {
        let mut paths = self.paths.borrow_mut();
        match paths.iter_mut().find(|(n, _)| *n == node) {
            Some((_, p)) => {
                p.clear();
                p.extend_from_slice(path);
            }
            None => paths.push((node, path.to_vec())),
        }
    }

    /// The path `node` was found by; `ENOENT` for a node this `SysVfs`
    /// never gave.
    fn path(&self, node: Node) -> Result<Vec<u8>, Errno> {
        let paths = self.paths.borrow();
        paths
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, p)| p.clone())
            .ok_or(Errno::ENOENT)
    }

    /// Runs `f` on the file at `node`'s path, opened with `flags`.
    fn with_fd<R>(
        &self,
        node: Node,
        flags: u32,
        f: impl FnOnce(&C, u32) -> Result<R, Errno>,
    ) -> Result<R, Errno> {
        let fd = self.calls.open(&self.path(node)?, flags)?;
        let result = f(&self.calls, fd);
        self.calls.close(fd);
        result
    }
}

impl<C: Calls> Vfs for SysVfs<C> {
    fn cwd(&self) -> Vec<u8> {
        let mut buf = vec![0; PATH_MAX];
        match self.calls.getcwd(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                buf
            }
            Err(_) => b"/".to_vec(),
        }
    }

    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.chdir(path)
    }

    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno> {
        let node = node_of(&self.calls.stat(path)?);
        self.found(node, path);
        Ok(node)
    }

    fn stat(&mut self, node: Node) -> Result<Stat, Errno> {
        Ok(stat_from(&self.calls.stat(&self.path(node)?)?))
    }

    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        let mut entries = Vec::new();
        let mut kinds = Vec::new();
        self.with_fd(node, OPEN_READ | OPEN_DIRECTORY, |calls, fd| {
            let mut buf = vec![0; DIR_BUFFER];
            loop {
                let n = calls.read_dir(fd, &mut buf)?;
                if n == 0 {
                    return Ok(());
                }
                let before = entries.len();
                for r in dir_entries(&buf[..n]) {
                    entries.push(DirEntry {
                        name: r.name.to_vec(),
                        ino: r.ino,
                    });
                    kinds.push((node, r.name.to_vec(), file_type(u32::from(r.kind))));
                }
                // A call that gave bytes but no record, or a directory that
                // never ends, is the kernel's fault; stop rather than spin.
                if entries.len() == before || entries.len() > DIR_ENTRIES_MAX {
                    return Err(Errno::EIO);
                }
            }
        })?;
        self.kinds = kinds;
        Ok(entries)
    }

    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno> {
        if let Some((_, _, kind)) = self
            .kinds
            .iter()
            .find(|(d, name, _)| *d == dir && *name == entry.name)
        {
            return Ok(*kind);
        }
        let mut path = self.path(dir)?;
        path.push(b'/');
        path.extend_from_slice(&entry.name);
        Ok(file_type(self.calls.stat(&path)?.kind))
    }

    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
        let mut buf = vec![0; PATH_MAX];
        let n = self.calls.readlink(&self.path(node)?, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.with_fd(node, OPEN_READ, |calls, fd| {
            calls.seek(fd, offset)?;
            calls.read(fd, buf)
        })
    }

    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.with_fd(node, OPEN_WRITE, |calls, fd| {
            calls.seek(fd, offset)?;
            calls.write(fd, buf)
        })
    }

    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno> {
        self.calls.truncate(&self.path(node)?, size)
    }

    fn touch(&mut self, node: Node) -> Result<(), Errno> {
        self.calls.touch(&self.path(node)?)
    }

    fn create(&mut self, path: &[u8]) -> Result<Node, Errno> {
        let flags = OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE;
        let fd = self.calls.open(path, flags)?;
        let st = self.calls.fstat(fd);
        self.calls.close(fd);
        let node = node_of(&st?);
        self.found(node, path);
        Ok(node)
    }

    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.mkdir(path)
    }

    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.unlink(path)
    }

    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.calls.rmdir(path)
    }

    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.calls.rename(from, to)
    }

    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno> {
        let s = self.calls.statfs(path)?;
        Ok(StatFs {
            block_size: s.block_size,
            blocks: s.blocks,
            free_blocks: s.free_blocks,
            avail_blocks: s.avail_blocks,
            files: s.files,
            free_files: s.free_files,
        })
    }

    fn sync(&mut self) -> Result<(), Errno> {
        self.calls.sync()
    }

    /// The kernel's `power` shuts the filesystems down (spec §7.3); a
    /// program has nothing to do first.
    fn shutdown(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

````

- [ ] **Step 8: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, replace:

````rust
impl Errno {
    /// Linux's `strerror` text.
````

with:

````rust
impl Errno {
    /// Every error number.
    pub const ALL: [Errno; 26] = [
        Errno::ENOENT,
        Errno::EEXIST,
        Errno::ENOTDIR,
        Errno::EISDIR,
        Errno::ENOTEMPTY,
        Errno::ENOSPC,
        Errno::EIO,
        Errno::EROFS,
        Errno::EINVAL,
        Errno::ENAMETOOLONG,
        Errno::EXDEV,
        Errno::EBUSY,
        Errno::EFBIG,
        Errno::E2BIG,
        Errno::ENOEXEC,
        Errno::EBADF,
        Errno::ECHILD,
        Errno::EAGAIN,
        Errno::ENOMEM,
        Errno::EFAULT,
        Errno::ENOSYS,
        Errno::ESRCH,
        Errno::EPERM,
        Errno::EINTR,
        Errno::EMFILE,
        Errno::ERANGE,
    ];

    /// The error with Linux's number `n` (a system call's error, in a
    /// program); `EIO` for a number this list does not have.
    pub fn from_number(n: u16) -> Errno {
        Errno::ALL
            .into_iter()
            .find(|e| e.number() == n)
            .unwrap_or(Errno::EIO)
    }

    /// Linux's `strerror` text.
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p vfs -p relay-rt`

Expected: PASS: 74 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add Cargo.lock crates
git commit -m "relay-rt: SysVfs, the shell's Vfs over the file calls, by path, each file named by its filesystem and inode"
````


### Task 11: One program per command in `/bin`

Spec §8.1, §8.4 and decisions 5 and 8: `relay-rt` gains `sysio.rs`, the rest of what a command needs over system calls. `SysConsole` reads fd 0 and writes fd 2, where a program's errors go (Task 13 gives an interactive shell's one the console back before every read); `SysSystem` has `time`, `sys_info`'s memory figures and kernel log, and `power` (`-f` is `POWER_FORCE`); `SysStdout` is fd 1, the console when `fstat` says a character device, a file otherwise, which it names by `stat` for `cat f >> f`; `sysio::run_command` is a program's `main`. `userland/utils` (`relay-utils`, a user package in `members` only and in `USER_PACKAGES`) holds one program per command, `cat` to `wc`, each naming its own command function. They are 71 to 133 KiB with the `user` profile, about 2.2 MiB together, and build in about 10 s. The `utils` scenario runs each by path, since the in-kernel shell's own commands come first, and expects what the in-kernel shell prints; under the in-kernel shell a program's standard output is the shell's output hook, which `fstat` calls a character device, so what depends on writing into a file waits for `/bin/sh`'s scenario (Task 13). `/bin` lists them (`system`), and `docs/hardware-test.md` and the README say so. The red run is the scenario's: without the programs, `/bin/pwd` is not found.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `crates/relay-rt/src/lib.rs`
- Create: `crates/relay-rt/src/sysio.rs`
- Modify: `docs/hardware-test.md`
- Modify: `tests/e2e/system.txt`
- Create: `tests/e2e/utils.txt`
- Create: `userland/utils/Cargo.toml`
- Create: `userland/utils/build.rs`
- Create: `userland/utils/src/bin/cat.rs`
- Create: `userland/utils/src/bin/clear.rs`
- Create: `userland/utils/src/bin/cp.rs`
- Create: `userland/utils/src/bin/date.rs`
- Create: `userland/utils/src/bin/df.rs`
- Create: `userland/utils/src/bin/dmesg.rs`
- Create: `userland/utils/src/bin/echo.rs`
- Create: `userland/utils/src/bin/free.rs`
- Create: `userland/utils/src/bin/head.rs`
- Create: `userland/utils/src/bin/ls.rs`
- Create: `userland/utils/src/bin/mkdir.rs`
- Create: `userland/utils/src/bin/mv.rs`
- Create: `userland/utils/src/bin/poweroff.rs`
- Create: `userland/utils/src/bin/pwd.rs`
- Create: `userland/utils/src/bin/reboot.rs`
- Create: `userland/utils/src/bin/rm.rs`
- Create: `userland/utils/src/bin/rmdir.rs`
- Create: `userland/utils/src/bin/stat.rs`
- Create: `userland/utils/src/bin/sync.rs`
- Create: `userland/utils/src/bin/tail.rs`
- Create: `userland/utils/src/bin/touch.rs`
- Create: `userland/utils/src/bin/uname.rs`
- Create: `userland/utils/src/bin/wc.rs`
- Modify: `xtask/src/config.rs`

**Interfaces:**
- Consumes: Tasks 4 (`run_command`, `commands::Run`, `Stdout`), 10 (`SysVfs`, `node_of`); plan 3b's `relay_rt::sys`.
- Produces: `relay_rt::sysio::{SysConsole::{new(), owned_by(pgid: u32)}, SysSystem, SysStdout::new(), mem_info(relay_abi::MemInfo) -> shell::MemInfo, power_flags(bool) -> u32, words(&Args) -> Vec<String>, run_command(name: &str, run: commands::Run, args: &Args) -> u8}`, re-exported as `relay_rt::{SysConsole, SysStdout, SysSystem}`; the package `userland/utils` (`relay-utils`); the `utils` scenario.

- [ ] **Step 1: Declare the new module in `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust
pub mod sys;
pub mod sysvfs;
````

with:

````rust
pub mod sys;
pub mod sysio;
pub mod sysvfs;
````

- [ ] **Step 2: Write the failing tests for `crates/relay-rt/src/sysio.rs`**

Create `crates/relay-rt/src/sysio.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_figures_pass_through() {
        let m = relay_abi::MemInfo {
            ram_total: 1,
            ram_free: 2,
            heap_total: 3,
            heap_used: 4,
        };
        assert_eq!(
            mem_info(m),
            MemInfo {
                ram_total: 1,
                ram_free: 2,
                heap_total: 3,
                heap_used: 4,
            }
        );
    }

    #[test]
    fn dash_f_is_power_force() {
        assert_eq!(power_flags(true), POWER_FORCE);
        assert_eq!(power_flags(false), 0);
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
        let args = Args::new(b"cat\0a b\0\xff\0\0", 4);
        assert_eq!(words(&args), ["a b", "\u{fffd}", ""]);
    }
}
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-sys  t-tee\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    cp    df     echo  head  mkdir  poweroff  reboot  rmdir  sync    t-fault  t-mem   t-spawn  t-sys  tail   uname\nclear  date  dmesg  free  ls    mv     pwd       rm      stat   t-args  t-files  t-read  t-spin   t-tee  touch  wc\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-sys  t-tee\n
````

with:

````text
send ls /bin
expect \ncat    cp    df     echo  head  mkdir  poweroff  reboot  rmdir  sync    t-fault  t-mem   t-spawn  t-sys  tail   uname\nclear  date  dmesg  free  ls    mv     pwd       rm      stat   t-args  t-files  t-read  t-spin   t-tee  touch  wc\n
````

- [ ] **Step 4: Add the scenario `tests/e2e/utils.txt`**

Create `tests/e2e/utils.txt`:

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
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send /bin/pwd
expect \n/root\n
send /bin/mkdir -p /root/a/b/c
send /bin/echo hello > /root/a/f
send /bin/echo world >> /root/a/f
send /bin/cat /root/a/f
expect \nhello\nworld\n
send /bin/ls -l /root/a
expect \ntotal 8\ndrwxr-xr-x 3 root root 4096 \w{3} [ \d]\d \d\d:\d\d b\n-rw-r--r-- 1 root root   12 \w{3} [ \d]\d \d\d:\d\d f\n
send /bin/cp /root/a/f /root/a/g
send /bin/mv /root/a/g /root/a/b/h
send /bin/ls /root/a /root/a/b
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
send /bin/rmdir /root/a/b
expect \nrmdir: failed to remove '/root/a/b': Directory not empty\n
send /bin/rm -r /root/a/b
send /bin/touch /root/t
send /bin/stat /root/t
expect \n  File: /root/t\n  Size: 0 .*regular empty file\n
send /bin/head -n 1 /root/a/f
expect \nhello\n
send /bin/tail -n 1 /root/a/f
expect \nworld\n
send /bin/wc /root/a/f
expect \n 2  2 12 /root/a/f\n
# Errors keep their place among the output, and never enter a file.
send /bin/cat /root/a/f /root/nope /root/a/f
expect \nhello\nworld\ncat: /root/nope: No such file or directory\nhello\nworld\n
send /bin/cat /root/nope > /root/out
expect \ncat: /root/nope: No such file or directory\n
send /bin/stat /root/out
expect \n  File: /root/out\n  Size: 0 .*regular empty file\n
# A program of /bin copies onto the root (SysVfs's tests show that files
# of two filesystems with one inode number are two files).
send /bin/cp /bin/t-args /root/t
expect /bin/cp /bin/t-args /root/t\nroot@relay:~# $
send /bin/rm /root/a/f /root/t /root/out
send /bin/rmdir /root/a
send /bin/ls
expect \nREADME  checks\n
send /bin/rm /bin/t-args
expect \nrm: cannot remove '/bin/t-args': Read-only file system\n
send /bin/rm -r /
expect \nrm: it is dangerous to operate recursively on '/'\n
send /bin/ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
send /bin/uname -a
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
send /bin/date
expect \n(Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d\n
send /bin/df
expect \nFilesystem     1K-blocks +Used Available Use% Mounted on\n/dev/root +\d+ +\d+ +\d+ +\d+% /\n
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 2\n
send /bin/sync
send /bin/free
expect \n +total +used +free\nMem: +\d+ +\d+ +\d+\nHeap: +\d+ +\d+ +\d+\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
poweroff /bin/poweroff
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `MemInfo` in this scope ``; `` cannot find value `POWER_FORCE` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: FAIL: scenario `utils` stops at line 14, timed out waiting for `\n/root\n`.

- [ ] **Step 6: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "userland/utils", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
````

- [ ] **Step 7: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
runtime gives them a heap), read the console a line at a time or as it is
typed, and copy what the console shows into files (tees).

````

with:

````markdown
runtime gives them a heap), read the console a line at a time or as it is
typed, and copy what the console shows into files (tees). Every command
is also a program of its own in `/bin` (`/bin/ls`), which prints what the
shell's command prints.

````

Replace:

````markdown
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, heap, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `tests/` holds the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
````

with:

````markdown
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, heap, panic handler, ABI note, linker script; the shell's `Vfs`, `Console` and `System` over system calls |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `utils/` one per command (`cat`, `ls`, …), `tests/` the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
````

- [ ] **Step 8: Change `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! gate): the entry point, the arguments, system-call wrappers, the heap
//! (`alloc` works in every program), the panic handler and the ELF note
//! that names the ABI.
//!
````

with:

````rust
//! gate): the entry point, the arguments, system-call wrappers, the heap
//! (`alloc` works in every program), the panic handler, the ELF note that
//! names the ABI, and the shell's `Vfs`, `Console` and `System` over system
//! calls, so a command function runs unchanged in a program.
//!
````

Replace:

````rust
pub use start::name;
pub use sysvfs::SysVfs;
````

with:

````rust
pub use start::name;
pub use sysio::{SysConsole, SysStdout, SysSystem};
pub use sysvfs::SysVfs;
````

- [ ] **Step 9: Implement `crates/relay-rt/src/sysio.rs`**

Insert this at the top of `crates/relay-rt/src/sysio.rs`, above `#[cfg(test)]`:

````rust
//! The shell's `Console` and `System`, and a program's standard output,
//! over system calls (user-space gate §8.1), and the `main` of `/bin`'s
//! command programs.

use crate::Args;
use crate::sys;
use crate::sysvfs::{SysVfs, node_of};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use relay_abi::console::MODE_RAW;
use relay_abi::file::KIND_CHAR_DEVICE;
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use shell::{Console, MemInfo, Stdout, System};
use vfs::{Errno, Node};

/// The console: what is typed on fd 0, and the screen on fd 2 (a
/// program's errors, and the shell's prompt and messages, as bash writes
/// them).
pub struct SysConsole {
    /// The process group that takes the console before every read (an
    /// interactive shell's own, spec §6.4).
    owner: Option<u32>,
}

impl SysConsole {
    /// The console of a program that does not read it.
    pub fn new() -> SysConsole {
        SysConsole { owner: None }
    }

    /// The console of an interactive shell, whose process group `pgid`
    /// takes it back, in raw mode, before every read, whatever a program
    /// left it in (spec §6.4, §16 item 4).
    pub fn owned_by(pgid: u32) -> SysConsole {
        SysConsole { owner: Some(pgid) }
    }
}

impl Default for SysConsole {
    fn default() -> SysConsole {
        SysConsole::new()
    }
}

impl Console for SysConsole {
    fn read_byte(&mut self) -> Option<u8> {
        if let Some(pgid) = self.owner {
            let _ = sys::console_mode(MODE_RAW);
            let _ = sys::console_foreground(pgid);
        }
        let mut byte = [0];
        match sys::read(0, &mut byte) {
            Ok(1) => Some(byte[0]),
            _ => None,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        let _ = sys::write_all(2, bytes);
    }

    fn columns(&self) -> usize {
        sys::console_size().0 as usize
    }
}

/// The clock, memory figures, kernel log and `power`.
pub struct SysSystem;

/// The kernel's memory figures as `free` shows them.
pub fn mem_info(m: relay_abi::MemInfo) -> MemInfo {
    MemInfo {
        ram_total: m.ram_total,
        ram_free: m.ram_free,
        heap_total: m.heap_total,
        heap_used: m.heap_used,
    }
}

/// `power`'s flags for `reboot -f` and `poweroff -f`.
pub fn power_flags(force: bool) -> u32 {
    if force { POWER_FORCE } else { 0 }
}

impl System for SysSystem {
    fn now(&self) -> u64 {
        sys::time().map_or(0, |t| t.unix_seconds)
    }

    fn memory(&self) -> Option<MemInfo> {
        sys::memory().ok().map(mem_info)
    }

    fn kernel_log(&self) -> Vec<u8> {
        let mut buf = vec![0; LOG_MAX];
        let n = sys::kernel_log(&mut buf).unwrap_or(0);
        buf.truncate(n);
        buf
    }

    /// `power` returns only when the machine stays up.
    fn reboot(&mut self, force: bool) -> Result<(), Errno> {
        Err(Errno::from_number(sys::power(
            POWER_REBOOT,
            power_flags(force),
        )))
    }

    fn poweroff(&mut self, force: bool) -> Result<(), Errno> {
        Err(Errno::from_number(sys::power(
            POWER_POWEROFF,
            power_flags(force),
        )))
    }
}

/// Standard output: fd 1, the console or the file the shell opened.
pub struct SysStdout {
    tty: bool,
}

impl SysStdout {
    pub fn new() -> SysStdout {
        let tty = sys::fstat(1).is_ok_and(|st| st.kind == u32::from(KIND_CHAR_DEVICE));
        SysStdout { tty }
    }
}

impl Default for SysStdout {
    fn default() -> SysStdout {
        SysStdout::new()
    }
}

impl Stdout for SysStdout {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        sys::write_all(1, bytes).map_err(Errno::from_number)
    }

    fn is_tty(&self) -> bool {
        self.tty
    }

    /// A file's `stat` names its filesystem (the console's is 0).
    fn node(&self) -> Option<Node> {
        sys::fstat(1)
            .ok()
            .filter(|st| st.dev != 0)
            .map(|st| node_of(&st))
    }
}

/// A program's arguments after argument 0, as the command functions take
/// them (bytes that are not UTF-8 are replaced, as they could not be typed
/// at the shell's prompt).
pub fn words(args: &Args) -> Vec<String> {
    args.iter()
        .skip(1)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect()
}

/// The `main` of one of `/bin`'s commands (user-space gate §8.4): runs the
/// command `name`, whose function is `run`, with the program's arguments,
/// printing exactly what it prints in the shell; its exit status.
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
}

````

- [ ] **Step 10: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 9 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 32 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 11: Create `userland/utils/Cargo.toml`**

Create `userland/utils/Cargo.toml`:

````toml
[package]
name = "relay-utils"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "One program per command of /bin (spec §8.4)"
build = "build.rs"

[dependencies]
relay-rt.workspace = true
shell.workspace = true
````

- [ ] **Step 12: Create `userland/utils/build.rs`**

Create `userland/utils/build.rs`:

````rust
fn main() {
    // The user linker script, from relay-rt's build script.
    let script = std::env::var("DEP_RELAY_RT_SCRIPT").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{script}");
}
````

- [ ] **Step 13: Create `userland/utils/src/bin/cat.rs`**

Create `userland/utils/src/bin/cat.rs`:

````rust
//! `/bin/cat`: print files (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("cat", shell::commands::cat, &args)
}
````

- [ ] **Step 14: Create `userland/utils/src/bin/clear.rs`**

Create `userland/utils/src/bin/clear.rs`:

````rust
//! `/bin/clear`: clear the screen (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("clear", shell::commands::clear, &args)
}
````

- [ ] **Step 15: Create `userland/utils/src/bin/cp.rs`**

Create `userland/utils/src/bin/cp.rs`:

````rust
//! `/bin/cp`: copy files (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("cp", shell::commands::cp, &args)
}
````

- [ ] **Step 16: Create `userland/utils/src/bin/date.rs`**

Create `userland/utils/src/bin/date.rs`:

````rust
//! `/bin/date`: print the date and time (UTC) (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("date", shell::commands::date, &args)
}
````

- [ ] **Step 17: Create `userland/utils/src/bin/df.rs`**

Create `userland/utils/src/bin/df.rs`:

````rust
//! `/bin/df`: show the free space on / (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("df", shell::commands::df, &args)
}
````

- [ ] **Step 18: Create `userland/utils/src/bin/dmesg.rs`**

Create `userland/utils/src/bin/dmesg.rs`:

````rust
//! `/bin/dmesg`: print the kernel log (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("dmesg", shell::commands::dmesg, &args)
}
````

- [ ] **Step 19: Create `userland/utils/src/bin/echo.rs`**

Create `userland/utils/src/bin/echo.rs`:

````rust
//! `/bin/echo`: print the arguments (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("echo", shell::commands::echo, &args)
}
````

- [ ] **Step 20: Create `userland/utils/src/bin/free.rs`**

Create `userland/utils/src/bin/free.rs`:

````rust
//! `/bin/free`: show memory use (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("free", shell::commands::free, &args)
}
````

- [ ] **Step 21: Create `userland/utils/src/bin/head.rs`**

Create `userland/utils/src/bin/head.rs`:

````rust
//! `/bin/head`: print the first lines of a file (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("head", shell::commands::head, &args)
}
````

- [ ] **Step 22: Create `userland/utils/src/bin/ls.rs`**

Create `userland/utils/src/bin/ls.rs`:

````rust
//! `/bin/ls`: list directory contents (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("ls", shell::commands::ls, &args)
}
````

- [ ] **Step 23: Create `userland/utils/src/bin/mkdir.rs`**

Create `userland/utils/src/bin/mkdir.rs`:

````rust
//! `/bin/mkdir`: make directories (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("mkdir", shell::commands::mkdir, &args)
}
````

- [ ] **Step 24: Create `userland/utils/src/bin/mv.rs`**

Create `userland/utils/src/bin/mv.rs`:

````rust
//! `/bin/mv`: move or rename files (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("mv", shell::commands::mv, &args)
}
````

- [ ] **Step 25: Create `userland/utils/src/bin/poweroff.rs`**

Create `userland/utils/src/bin/poweroff.rs`:

````rust
//! `/bin/poweroff`: sync and turn the machine off (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("poweroff", shell::commands::poweroff, &args)
}
````

- [ ] **Step 26: Create `userland/utils/src/bin/pwd.rs`**

Create `userland/utils/src/bin/pwd.rs`:

````rust
//! `/bin/pwd`: print the current directory (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("pwd", shell::commands::pwd, &args)
}
````

- [ ] **Step 27: Create `userland/utils/src/bin/reboot.rs`**

Create `userland/utils/src/bin/reboot.rs`:

````rust
//! `/bin/reboot`: sync and restart the machine (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("reboot", shell::commands::reboot, &args)
}
````

- [ ] **Step 28: Create `userland/utils/src/bin/rm.rs`**

Create `userland/utils/src/bin/rm.rs`:

````rust
//! `/bin/rm`: remove files or directories (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("rm", shell::commands::rm, &args)
}
````

- [ ] **Step 29: Create `userland/utils/src/bin/rmdir.rs`**

Create `userland/utils/src/bin/rmdir.rs`:

````rust
//! `/bin/rmdir`: remove empty directories (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("rmdir", shell::commands::rmdir, &args)
}
````

- [ ] **Step 30: Create `userland/utils/src/bin/stat.rs`**

Create `userland/utils/src/bin/stat.rs`:

````rust
//! `/bin/stat`: show everything about a file (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("stat", shell::commands::stat, &args)
}
````

- [ ] **Step 31: Create `userland/utils/src/bin/sync.rs`**

Create `userland/utils/src/bin/sync.rs`:

````rust
//! `/bin/sync`: write cached changes to the disk (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("sync", shell::commands::sync, &args)
}
````

- [ ] **Step 32: Create `userland/utils/src/bin/tail.rs`**

Create `userland/utils/src/bin/tail.rs`:

````rust
//! `/bin/tail`: print the last lines of a file (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("tail", shell::commands::tail, &args)
}
````

- [ ] **Step 33: Create `userland/utils/src/bin/touch.rs`**

Create `userland/utils/src/bin/touch.rs`:

````rust
//! `/bin/touch`: create files or update their times (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("touch", shell::commands::touch, &args)
}
````

- [ ] **Step 34: Create `userland/utils/src/bin/uname.rs`**

Create `userland/utils/src/bin/uname.rs`:

````rust
//! `/bin/uname`: print the system name (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("uname", shell::commands::uname, &args)
}
````

- [ ] **Step 35: Create `userland/utils/src/bin/wc.rs`**

Create `userland/utils/src/bin/wc.rs`:

````rust
//! `/bin/wc`: count lines, words and bytes (user-space gate §8.4), the shell's command
//! function run as a program.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    relay_rt::sysio::run_command("wc", shell::commands::wc, &args)
}
````

- [ ] **Step 36: Change `xtask/src/config.rs`**

In `xtask/src/config.rs`, replace:

````rust
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-tests"];
````

with:

````rust
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-tests", "relay-utils"];
````

- [ ] **Step 37: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 23 tests.

- [ ] **Step 38: Run the `utils`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario utils`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 39: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 40: Commit**

````bash
git add Cargo.lock Cargo.toml README.md crates docs tests userland xtask
git commit -m "relay-rt, userland: SysConsole, SysSystem and SysStdout; one program per command in /bin, printing what the shell's command prints"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 36 scenario(s) passed`.

````bash
git push -u origin m2p4a/utils
gh pr create --base main --head m2p4a/utils --title "Milestone 2, plan 4a: One program per command" --body-file - <<'EOF'
## What

Milestone 2, plan 4a, tasks 8–11: `relay_abi::Stat` gains `dev`, the file's filesystem, and the ABI is version 2; `relay-rt`'s `SysVfs` is the shell's `Vfs` over the file calls, by path, each file named by its filesystem and inode (`vfs::Errno::from_number`); `SysConsole`, `SysSystem` and `SysStdout`; `userland/utils`, one program per command in `/bin` (`cat` to `wc`), each printing what the shell's command prints (the `utils` scenario).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: plan 4a runs no NUC check (decision 1); plan 4b's NUC check 4 runs these programs
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4a/utils --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-utils
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `/bin/sh` (Tasks 12–16)

`spawn`'s `FOREGROUND` and `/bin/sh`, the shell as a program, started here from the in-kernel shell.

Branch `m2p4a/sh`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-sh`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p4a/sh /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-sh origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-sh
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p4a/utils` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p4a/sh /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-sh m2p4a/utils`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p4a/utils>` and re-run `cargo xtask ci` before pushing.

### Task 12: `spawn`'s `FOREGROUND` gives the child's new group the console before it runs

Spec §6.4, §7.3 and decision 9: an interactive `/bin/sh` gives the console to each command's new process group. It could only call `console_foreground` after `spawn` made the group, and the scheduler may run the child first (the ticks the spawn took can end the shell's slice): a command that reads the console at once would get end of input. `relay_abi::spawn::FOREGROUND` (2, with `NEW_GROUP`; `EINVAL` alone) makes the kernel hand the console over, in line mode, and wake the console's readers, right after the child is in the table and before it can run: the kernel is not preemptible and has not switched to it yet. Mutation check: the flag accepted without `NEW_GROUP` fails a test.

**Files:**
- Modify: `crates/relay-abi/src/spawn.rs`
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/syscall.rs`

**Interfaces:**
- Consumes: plan 3b's `spawn`, `tty::set_foreground`, `tty::set_line_mode`.
- Produces: `relay_abi::spawn::FOREGROUND`; `syscall::Spawn::foreground: bool`.

- [ ] **Step 1: Add the failing tests to `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, replace:

````rust
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
    }
````

with:

````rust
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
        assert_eq!(FOREGROUND, 2);
    }
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
                new_group: true,
            }]
````

with:

````rust
                new_group: true,
                foreground: false,
            }]
````

Replace:

````rust
        assert_eq!(s.argc, 1);
        // The caller's refusal.
````

with:

````rust
        assert_eq!(s.argc, 1);
        // A group of its own that gets the console.
        let a = spawn_args(&mut f, b"x\0", |a| a.flags = NEW_GROUP | FOREGROUND);
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(103));
        assert!(f.spawned[2].new_group && f.spawned[2].foreground);
        // The caller's refusal.
````

Replace:

````rust
        );
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 2), Err(errno::EINVAL));
        assert_eq!(
````

with:

````rust
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.flags = FOREGROUND),
            Err(errno::EINVAL),
            "the console goes to a group of the child's own"
        );
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 4), Err(errno::EINVAL));
        assert_eq!(
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-abi -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `FOREGROUND` in this scope ``.

- [ ] **Step 4: Change `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const NEW_GROUP: u32 = 1;

````

with:

````rust
pub const NEW_GROUP: u32 = 1;
/// `SpawnArgs::flags`, with [`NEW_GROUP`]: the new group becomes the
/// console's foreground, in line mode, before the child runs, as an
/// interactive shell gives the console to its command (spec §6.4, §16
/// item 5). Without it the child might read the console before its parent
/// could hand it over, and get end of input.
pub const FOREGROUND: u32 = 2;

````

Replace:

````rust
    pub fd_count: u32,
    /// [`NEW_GROUP`] or 0.
    pub flags: u32,
````

with:

````rust
    pub fd_count: u32,
    /// [`NEW_GROUP`], with or without [`FOREGROUND`], or 0.
    pub flags: u32,
````

- [ ] **Step 5: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
/// `fds` names (child, parent) and closing its others; with `NEW_GROUP` in
/// `flags` it starts a process group of its own. Its pid.
pub fn spawn(path: &[u8], args: &[u8], cwd: &[u8], fds: &[FdMap], flags: u32) -> Result<u32, u16> {
````

with:

````rust
/// `fds` names (child, parent) and closing its others; with `NEW_GROUP` in
/// `flags` it starts a process group of its own, which `FOREGROUND` also
/// gives the console, in line mode. Its pid.
pub fn spawn(path: &[u8], args: &[u8], cwd: &[u8], fds: &[FdMap], flags: u32) -> Result<u32, u16> {
````

- [ ] **Step 6: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
    let me = t.current();
    t.insert(me, s.new_group, name.into_owned(), res)
        .map_err(|e| unreachable!("has_room was checked under this lock: {e}"))
}
````

with:

````rust
    let me = t.current();
    let pid = t
        .insert(me, s.new_group, name.into_owned(), res)
        .unwrap_or_else(|e| unreachable!("has_room was checked under this lock: {e}"));
    drop(t);
    // Before the child can run: the kernel is not preemptible, and it has
    // not been switched to yet.
    if s.foreground {
        tty::set_foreground(pid);
        tty::set_line_mode(true);
        PROCS.lock().wake_all(Blocked::Console);
    }
    Ok(pid)
}
````

- [ ] **Step 7: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust
                new_group: true,
            })
````

with:

````rust
                new_group: true,
                foreground: false,
            })
````

- [ ] **Step 8: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
````

with:

````rust
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{FOREGROUND, NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
````

Replace:

````rust
    pub new_group: bool,
}
````

with:

````rust
    pub new_group: bool,
    /// The new group gets the console (`FOREGROUND`).
    pub foreground: bool,
}
````

Replace:

````rust
    let a = SpawnArgs::from_bytes(&raw);
    if a.flags & !NEW_GROUP != 0 || a.fd_count as usize > SPAWN_FDS {
        return Err(Errno::EINVAL);
````

with:

````rust
    let a = SpawnArgs::from_bytes(&raw);
    let foreground = a.flags & FOREGROUND != 0;
    if a.flags & !(NEW_GROUP | FOREGROUND) != 0
        || (foreground && a.flags & NEW_GROUP == 0)
        || a.fd_count as usize > SPAWN_FDS
    {
        return Err(Errno::EINVAL);
````

Replace:

````rust
        new_group: a.flags & NEW_GROUP != 0,
    };
````

with:

````rust
        new_group: a.flags & NEW_GROUP != 0,
        foreground,
    };
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-abi -p relay-kernel --lib`

Expected: PASS: 364 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates kernel
git commit -m "relay-abi, kernel: spawn's FOREGROUND gives the child's new group the console, in line mode, before the child runs"
````


### Task 13: `/bin/sh`

Spec §8.3 and decisions 8, 9 and 10: `relay-rt`'s `SysPrograms` is `/bin/sh`'s `Programs` (a redirection opened with `CREATE` and `TRUNCATE` or `APPEND`, a command's fds the shell's 0 and 2 and the redirection or its 1, `NEW_GROUP | FOREGROUND` at the prompt, a tee pushed from the transcript's fd, which it then closes). `userland/sh` (`relay-sh`, the program `sh`) is the interactive shell without arguments, whose console is its own group's (`SysConsole::owned_by(getpid())`: whoever starts it at a prompt gives it a group of its own; Task 15 handles a shell a script starts), and `sh FILE` with one (`Shell::run_file`). It names itself `relay-sh`, as the in-kernel shell does (decision 4). The `sh` scenario starts it from the in-kernel shell: redirections into files (lines, not columns; `cat f >> f`), what cannot run, a fault, a command that reads the console at once, Ctrl-C of a program and of a script, a script that runs another with both transcripts, `cd`, and `exit 3` back to the in-kernel shell, with the same memory in use before and after. `/bin` lists `sh` (`system`: 33 programs). The red run is the scenario's: without `/bin/sh` it is not found.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `crates/relay-rt/src/lib.rs`
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `docs/hardware-test.md`
- Create: `tests/e2e/sh.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/sh/Cargo.toml`
- Create: `userland/sh/build.rs`
- Create: `userland/sh/src/main.rs`
- Modify: `xtask/src/config.rs`

**Interfaces:**
- Consumes: Tasks 5 (`Shell::spawning`, `run_file`, `Programs`), 11 (`SysConsole`, `SysSystem`, `SysStdout`, `words`), 10 (`SysVfs`), 12 (`FOREGROUND`).
- Produces: `relay_rt::sysio::{SysPrograms, output_flags(append: bool) -> u32, arg_bytes(&[&[u8]]) -> Vec<u8>, command_fds(Option<u32>) -> [FdMap; 3]}`, re-exported as `relay_rt::SysPrograms`; the package `userland/sh` (`relay-sh`, the program `/bin/sh`); the `sh` scenario.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
    #[test]
    fn the_words_are_the_arguments_after_the_name() {
````

with:

````rust
    #[test]
    fn a_redirection_is_created_and_emptied_or_appended_to() {
        assert_eq!(
            output_flags(false),
            OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE
        );
        assert_eq!(output_flags(true), OPEN_WRITE | OPEN_CREATE | OPEN_APPEND);
    }

    #[test]
    fn a_command_gets_the_shell_s_fds_but_its_redirection() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds(None)), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds(Some(5))), [(0, 0), (1, 5), (2, 2)]);
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
    }

    #[test]
    fn the_words_are_the_arguments_after_the_name() {
````

- [ ] **Step 2: Add the scenario `tests/e2e/sh.txt`**

Create `tests/e2e/sh.txt`:

````text
# /bin/sh, the shell as a program (user-space gate §8.2, §8.3; milestone
# 2, plan 4a), started here from the in-kernel shell. Every command but cd,
# exit and help is a program; a redirection is a file the program writes
# as its fd 1; at the prompt the shell takes the console and gives it to
# each command (spawn's FOREGROUND); a script's commands run in its own
# group, its transcript a console tee, and one script may run another.
timeout 30
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send /bin/sh
expect /bin/sh\nroot@relay:~# $
send mkdir /root/a
send touch /root/a/x /root/a/y
send ls /root/a
expect \nx  y\n
# Into a file a program writes lines, not columns, and never reads its
# own output back.
send ls /root/a > /root/list
send cat /root/list
expect \nx\ny\n
send cat /root/list >> /root/list
expect \ncat: /root/list: input file is output file\n
send nosuch
expect \nrelay-sh: nosuch: command not found\n
send t-fault null-read
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)\n
# The command reads the console at once: it has it before it runs.
send t-read
alive 1
send hello
expect \nhello\n\[6\] hello\\n\n
type {ctrl-d}
expect end of input\nroot@relay:~# $
send t-spin
alive 1
key {ctrl-c}
expect \n\^C\nroot@relay:~# 
# Scripts: the transcript gets what the screen gets, a script's programs'
# output and a nested script's lines too.
send echo 'echo one' > /root/s.sh
send echo 'sh /root/t.sh' >> /root/s.sh
send echo 't-args x' >> /root/s.sh
send echo 'echo two' > /root/t.sh
send sh /root/s.sh
expect \n\+ echo one\none\n\+ sh /root/t.sh\n\+ echo two\ntwo\n\+ t-args x\n\[1\] x\nroot@relay:~# $
send cat /root/s.log
expect \n\+ echo one\none\n\+ sh /root/t.sh\n\+ echo two\ntwo\n\+ t-args x\n\[1\] x\nroot@relay:~# $
send cat /root/t.log
expect \n\+ echo two\ntwo\nroot@relay:~# $
# Ctrl-C ends a script with its command.
send echo t-spin > /root/spin.sh
send sh /root/spin.sh
alive 1
key {ctrl-c}
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log
send cd /tmp
expect root@relay:/tmp# $
send exit 3
expect exit 3\nroot@relay:~# $
send pwd
expect \n/root\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    cp    df     echo  head  mkdir  poweroff  reboot  rmdir  sync    t-fault  t-mem   t-spawn  t-sys  tail   uname\nclear  date  dmesg  free  ls    mv     pwd       rm      stat   t-args  t-files  t-read  t-spin   t-tee  touch  wc\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat    t-fault  t-read   t-sys  touch\nclear  df     free  mkdir  pwd       rmdir  sync    t-files  t-spawn  t-tee  uname\ncp     dmesg  head  mv     reboot    sh     t-args  t-mem    t-spin   tail   wc\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    cp    df     echo  head  mkdir  poweroff  reboot  rmdir  sync    t-fault  t-mem   t-spawn  t-sys  tail   uname\nclear  date  dmesg  free  ls    mv     pwd       rm      stat   t-args  t-files  t-read  t-spin   t-tee  touch  wc\n
````

with:

````text
send ls /bin
expect \ncat    date   echo  ls     poweroff  rm     stat    t-fault  t-read   t-sys  touch\nclear  df     free  mkdir  pwd       rmdir  sync    t-files  t-spawn  t-tee  uname\ncp     dmesg  head  mv     reboot    sh     t-args  t-mem    t-spin   tail   wc\n
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find value `OPEN_WRITE` in this scope ``; `` cannot find value `OPEN_CREATE` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: FAIL: scenario `sh` stops at line 12, timed out waiting for `/bin/sh\nroot@relay:~# $`.

- [ ] **Step 5: Change `Cargo.toml`**

In `Cargo.toml`, replace:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "userland/utils", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "userland/utils", "userland/sh", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
````

- [ ] **Step 6: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
is also a program of its own in `/bin` (`/bin/ls`), which prints what the
shell's command prints.

````

with:

````markdown
is also a program of its own in `/bin` (`/bin/ls`), which prints what the
shell's command prints, and the shell itself is one too: `/bin/sh` runs
every command but `cd`, `exit` and `help` as a program, and its scripts
may run scripts.

````

Replace:

````markdown
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `utils/` one per command (`cat`, `ls`, …), `tests/` the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
````

with:

````markdown
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `sh/` the shell (`/bin/sh`), `utils/` one per command (`cat`, `ls`, …), `tests/` the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
````

- [ ] **Step 7: Change `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust
pub use start::name;
pub use sysio::{SysConsole, SysStdout, SysSystem};
pub use sysvfs::SysVfs;
````

with:

````rust
pub use start::name;
pub use sysio::{SysConsole, SysPrograms, SysStdout, SysSystem};
pub use sysvfs::SysVfs;
````

- [ ] **Step 8: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::console::MODE_RAW;
use relay_abi::file::KIND_CHAR_DEVICE;
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use shell::{Console, MemInfo, Stdout, System};
use vfs::{Errno, Node};
````

with:

````rust
use relay_abi::console::MODE_RAW;
use relay_abi::file::{KIND_CHAR_DEVICE, OPEN_APPEND, OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Programs, Stdout, System};
use vfs::{Errno, Node};
````

Replace:

````rust

/// A program's arguments after argument 0, as the command functions take
````

with:

````rust

/// `/bin/sh`'s way to its commands (user-space gate §8.2, §8.3): `spawn`,
/// `wait`, and the tees of a script's transcript.
pub struct SysPrograms;

/// `open`'s flags for a redirection: `>` empties the file, `>>` writes at
/// its end.
pub fn output_flags(append: bool) -> u32 {
    OPEN_WRITE | OPEN_CREATE | if append { OPEN_APPEND } else { OPEN_TRUNCATE }
}

/// `spawn`'s arguments: each followed by a NUL.
pub fn arg_bytes(args: &[&[u8]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for a in args {
        bytes.extend_from_slice(a);
        bytes.push(0);
    }
    bytes
}

/// A command's fds: the shell's 0 and 2, and `stdout` or the shell's 1.
pub fn command_fds(stdout: Option<u32>) -> [FdMap; 3] {
    [(0, 0), (1, stdout.unwrap_or(1)), (2, 2)].map(|(child, parent)| FdMap { child, parent })
}

impl Programs for SysPrograms {
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno> {
        sys::open(path, output_flags(append)).map_err(Errno::from_number)
    }

    fn close(&mut self, fd: u32) {
        let _ = sys::close(fd);
    }

    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
        let flags = if foreground {
            NEW_GROUP | FOREGROUND
        } else {
            0
        };
        sys::spawn(path, &arg_bytes(args), b"", &command_fds(stdout), flags)
            .map_err(Errno::from_number)
    }

    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        match sys::wait(i64::from(pid), false) {
            Ok(Some((_, status))) => Ok(status),
            Ok(None) => Err(Errno::ECHILD),
            Err(e) => Err(Errno::from_number(e)),
        }
    }

    /// The tee holds the open file (spec §16 item 4), so the fd goes.
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno> {
        let fd = sys::open(path, OPEN_WRITE).map_err(Errno::from_number)?;
        let pushed = sys::console_tee_push(fd).map_err(Errno::from_number);
        let _ = sys::close(fd);
        pushed
    }

    fn tee_pop(&mut self) -> Result<(), Errno> {
        sys::console_tee_pop().map_err(Errno::from_number)
    }
}

/// A program's arguments after argument 0, as the command functions take
````

- [ ] **Step 9: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 32 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 33 programs, ABI 2` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 10: Create `userland/sh/Cargo.toml`**

Create `userland/sh/Cargo.toml`:

````toml
[package]
name = "relay-sh"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "/bin/sh, the shell as a program (spec §8.3)"
build = "build.rs"

[[bin]]
name = "sh"
path = "src/main.rs"

[dependencies]
relay-rt.workspace = true
shell.workspace = true
````

- [ ] **Step 11: Create `userland/sh/build.rs`**

Create `userland/sh/build.rs`:

````rust
fn main() {
    // The user linker script, from relay-rt's build script.
    let script = std::env::var("DEP_RELAY_RT_SCRIPT").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{script}");
}
````

- [ ] **Step 12: Create `userland/sh/src/main.rs`**

Create `userland/sh/src/main.rs`:

````rust
//! `/bin/sh` (user-space gate §8.3): the shell as a program. Without
//! arguments it is the interactive shell, which takes the console at its
//! prompt and gives it to each command it starts; `sh FILE` runs a script,
//! its commands in the shell's own process group and its transcript a
//! console tee. Every command but `cd`, `exit` and `help` is a program.
#![no_std]
#![no_main]

use relay_rt::sysio::words;
use relay_rt::{Args, SysConsole, SysPrograms, SysStdout, SysSystem, SysVfs, sys};
use shell::Shell;

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let (mut vfs, mut system, mut programs) = (SysVfs::new(), SysSystem, SysPrograms);
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell is started in a process group of its own.
        let mut console = SysConsole::owned_by(sys::getpid());
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
        shell.status() as u8
    } else {
        let mut console = SysConsole::new();
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_file(&words, &mut SysStdout::new()) as u8
    }
}
````

- [ ] **Step 13: Change `xtask/src/config.rs`**

In `xtask/src/config.rs`, replace:

````rust
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-tests", "relay-utils"];
````

with:

````rust
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-sh", "relay-tests", "relay-utils"];
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 25 tests.

- [ ] **Step 15: Run the `sh`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add Cargo.lock Cargo.toml README.md crates docs tests userland xtask
git commit -m "relay-rt, userland: /bin/sh, the shell as a program; SysPrograms spawns its commands and pushes a script's transcript as a tee"
````


### Task 14: The in-kernel shell collects the orphans that end while it waits

The prototype's review (important): Ctrl-C of a script under `/bin/sh` kills the script shell and its command together, and the command's zombie passes to process 1, the in-kernel shell, which waited for `/bin/sh`'s pid and collected nothing else until it ended; after about 62 such scripts every command was `Resource temporarily unavailable` (decision 11). `proc::wait` now collects any child that ends while it waits, returning when its own does (`ECHILD` still comes first for a pid that is not its child). The `sh` scenario fills the process table with `t-spawn fill` twice under `/bin/sh` and expects the same count both times, and the in-kernel shell fills it first, before memory in use is measured, since the kernel-stack slots' page tables stay once made. The red run is the scenario's: the Ctrl-C'd script before it has already left a zombie, so the table holds one child fewer.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `tests/e2e/sh.txt`

**Interfaces:**
- Consumes: plan 3a's `proc::collect`; Task 13's scenario.
- Produces: `proc::wait` collecting orphans as they end.

- [ ] **Step 1: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect root@relay:~# $
send free
````

with:

````text
expect root@relay:~# $
# Every kernel-stack slot's page tables made first (they stay), so memory
# in use compares after them (and after the in-kernel shell collected the
# orphans, which it does before it starts a program).
send t-spawn fill
expect \nfilled the table with 62 children\n
alive 1
send t-args collected
expect \n\[1\] collected\n
send free
````

Replace:

````text
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log
````

with:

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
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: FAIL: scenario `sh` stops at line 69, timed out waiting for `\nfilled the table with 61 children\n`.

- [ ] **Step 3: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook); then it
/// collects the orphans that have ended. `ECHILD` if `pid` is not its
/// child.
pub fn wait(pid: u32, out: &mut shell::Output<'_>) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = collect(Child::Pid(pid), false);
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    collect_orphans();
    match ended? {
        Some((_, status)) => Ok(status),
        None => unreachable!("wait without nohang collects a child"),
    }
````

with:

````rust
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
/// meanwhile.
fn wait_collecting(pid: u32) -> Result<WaitStatus, Errno> {
    // `ECHILD` before anything else is collected.
    if let Some((_, status)) = collect(Child::Pid(pid), true)? {
        return Ok(status);
    }
    loop {
        match collect(Child::Any, false)? {
            Some((ended, status)) if ended == pid => return Ok(status),
            Some(_) => {}
            None => unreachable!("wait without nohang collects a child"),
        }
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 341 tests.

- [ ] **Step 5: Run the `sh`, `spawn`, `ctrlc` scenarios**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel tests
git commit -m "kernel: the in-kernel shell collects the orphans that end while it waits for a command, so a script Ctrl-C'd under /bin/sh leaks no process"
````


### Task 15: An interactive `/bin/sh` that a script starts keeps the console

The prototype's review (minor): a script line `sh` starts an interactive shell in the script's process group, and that shell took the console back for a group numbered after its own pid, which does not exist: after its first program `console_foreground` said `ESRCH`, its read got end of input, and it ended without a word, the script going on (decision 8). `/bin/sh` now asks whether it leads a group (`console_foreground` of its pid succeeds only then); if not, `SysConsole::interactive(None)` takes back only raw mode, and `SysPrograms::new(false)` runs its commands in its group, without `FOREGROUND`, so the console never leaves the group. The `sh` scenario runs such a script. Mutation check: a non-leader's commands given `FOREGROUND` fail the scenario; a shell that always takes the console for its pid is the red run.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `tests/e2e/sh.txt`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: Tasks 11 (`SysConsole`), 13 (`SysPrograms`, `main.rs`).
- Produces: `SysConsole::interactive(group: Option<u32>)` in place of `owned_by`; `SysPrograms::new(own_group: bool)`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \n\+ echo two\ntwo\nroot@relay:~# $
# Ctrl-C ends a script with its command.
````

with:

````text
expect \n\+ echo two\ntwo\nroot@relay:~# $
# A shell a script starts is in the script's group, not a group of its
# own: it keeps the console all the same, its commands in that group, and
# the script goes on after its `exit`.
send echo sh > /root/i.sh
send echo 'echo after' >> /root/i.sh
send sh /root/i.sh
expect \+ sh\nroot@relay:~# $
send echo inner
expect \ninner\nroot@relay:~# $
send echo again
expect \nagain\nroot@relay:~# $
send exit
expect \+ echo after\nafter\nroot@relay:~# $
# Ctrl-C ends a script with its command.
````

Replace:

````text
alive 1
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log
send cd /tmp
````

with:

````text
alive 1
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log
send cd /tmp
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: FAIL: scenario `sh` stops at line 67, timed out waiting for `\ninner\nroot@relay:~# $`.

- [ ] **Step 3: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
pub struct SysConsole {
    /// The process group that takes the console before every read (an
    /// interactive shell's own, spec §6.4).
    owner: Option<u32>,
}
````

with:

````rust
pub struct SysConsole {
    /// An interactive shell's: it takes the console back, in raw mode,
    /// before every read.
    interactive: bool,
    /// The process group that takes it (the shell's own, spec §6.4).
    group: Option<u32>,
}
````

Replace:

````rust
    pub fn new() -> SysConsole {
        SysConsole { owner: None }
    }

    /// The console of an interactive shell, whose process group `pgid`
    /// takes it back, in raw mode, before every read, whatever a program
    /// left it in (spec §6.4, §16 item 4).
    pub fn owned_by(pgid: u32) -> SysConsole {
        SysConsole { owner: Some(pgid) }
    }
````

with:

````rust
    pub fn new() -> SysConsole {
        SysConsole {
            interactive: false,
            group: None,
        }
    }

    /// The console of an interactive shell, which takes it back, in raw
    /// mode, before every read, whatever a program left it in (spec §6.4,
    /// §16 item 4): for its process group `group` when it leads one, which
    /// its commands' groups had; otherwise it shares a group, a script's,
    /// with its commands, which never take the console from it.
    pub fn interactive(group: Option<u32>) -> SysConsole {
        SysConsole {
            interactive: true,
            group,
        }
    }
````

Replace:

````rust
    fn read_byte(&mut self) -> Option<u8> {
        if let Some(pgid) = self.owner {
            let _ = sys::console_mode(MODE_RAW);
            let _ = sys::console_foreground(pgid);
````

with:

````rust
    fn read_byte(&mut self) -> Option<u8> {
        if self.interactive {
            let _ = sys::console_mode(MODE_RAW);
        }
        if let Some(pgid) = self.group {
            let _ = sys::console_foreground(pgid);
````

Replace:

````rust
/// `wait`, and the tees of a script's transcript.
pub struct SysPrograms;

````

with:

````rust
/// `wait`, and the tees of a script's transcript.
pub struct SysPrograms {
    /// The shell leads a process group of its own, so a command at its
    /// prompt may have one too, with the console; a shell in a script's
    /// group keeps its commands there (and the console with them).
    own_group: bool,
}

impl SysPrograms {
    pub fn new(own_group: bool) -> SysPrograms {
        SysPrograms { own_group }
    }
}

````

Replace:

````rust
    ) -> Result<u32, Errno> {
        let flags = if foreground {
            NEW_GROUP | FOREGROUND
````

with:

````rust
    ) -> Result<u32, Errno> {
        let flags = if foreground && self.own_group {
            NEW_GROUP | FOREGROUND
````

- [ ] **Step 4: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, replace:

````rust
fn main(args: Args) -> u8 {
    let (mut vfs, mut system, mut programs) = (SysVfs::new(), SysSystem, SysPrograms);
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell is started in a process group of its own.
        let mut console = SysConsole::owned_by(sys::getpid());
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
        shell.status() as u8
    } else {
        let mut console = SysConsole::new();
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
````

with:

````rust
fn main(args: Args) -> u8 {
    let (mut vfs, mut system) = (SysVfs::new(), SysSystem);
    let words = words(&args);
    if words.is_empty() {
        // An interactive shell leads a process group of its own when it
        // was started at a prompt (the group is numbered after it); one a
        // script started is in the script's group.
        let leader = sys::console_foreground(sys::getpid()).is_ok();
        let mut console = SysConsole::interactive(leader.then(sys::getpid));
        let mut programs = SysPrograms::new(leader);
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
        shell.status() as u8
    } else {
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
````

- [ ] **Step 5: Run the `sh` scenario**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates tests userland
git commit -m "relay-rt, userland: an interactive /bin/sh a script starts keeps the console, its commands in the script's group"
````


### Task 16: The `sh` scenario sees the prompt take the console back raw

The prototype's review (minor): nothing noticed `/bin/sh` reading its prompt in line mode: without `SysConsole`'s `console_mode(MODE_RAW)`, every line typed after a command that left line mode was echoed twice, and the `sh` scenario still passed. After `t-read` it now expects `echo once` echoed once, anchored where the last match ended (`\A`), and Ctrl-C on a line being typed to cancel only the line. This task adds only a test of what Task 13 does, so it has no failing run; the mutation check shows what it guards: without the raw mode, the scenario fails at `echo once`.

**Files:**
- Modify: `tests/e2e/sh.txt`

**Interfaces:**
- Consumes: Task 13's scenario.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, replace:

````text
expect end of input\nroot@relay:~# $
send t-spin
````

with:

````text
expect end of input\nroot@relay:~# $
# t-read had the console in line mode; the prompt takes it back raw:
# what is typed is echoed once, by the shell, and Ctrl-C only cancels the
# line.
send echo once
expect \Aecho once\nonce\nroot@relay:~# $
type echo no{ctrl-c}
expect \Aecho no\^C\nroot@relay:~# $
send t-spin
````

- [ ] **Step 2: Run the `sh` scenario**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -m "Tests: the sh scenario sees /bin/sh take the console back raw at its prompt after a command left line mode"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 37 scenario(s) passed`.

````bash
git push -u origin m2p4a/sh
gh pr create --base main --head m2p4a/sh --title "Milestone 2, plan 4a: /bin/sh" --body-file - <<'EOF'
## What

Milestone 2, plan 4a, tasks 12–16: `spawn`'s `FOREGROUND` gives the child's new group the console, in line mode, before it runs; `relay-rt`'s `SysPrograms`; `userland/sh`, `/bin/sh`: every command but `cd`, `exit` and `help` a program, redirections into files, the console at the prompt, scripts in their own group with their transcripts as tees, one script running another (the `sh` scenario); the in-kernel shell collects the orphans that end while it waits, and an interactive `/bin/sh` that a script starts keeps the console.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: plan 4a runs no NUC check (decision 1); plan 4b's NUC check 4 runs these programs
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p4a/sh --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p4a-sh
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
