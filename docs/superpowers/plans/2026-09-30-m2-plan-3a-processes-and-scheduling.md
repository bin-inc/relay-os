# Milestone 2 · Plan 3a: Processes and scheduling — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs become processes: they share the CPU (a round-robin scheduler with 10-tick slices preempts the ones that never make a system call, and the keyboard keeps working meanwhile), start and wait for programs of their own, and are killed by Ctrl-C or `kill`; the in-kernel shell becomes process 1, and SMEP, SMAP and CR0.TS keep the kernel and the programs apart. It ends with `cargo xtask ci` green and NUC check 3, by script and with two steps by hand, running `t-spin`, `t-spawn` and the new `t-fault` kinds on a stick written by `cargo xtask flash --full`.

**Architecture:** As in plans 1 and 2, the logic is architecture-neutral and host-tested and the hardware glue is thin. The process table and its run queue (`kernel/src/proc/table.rs`) are generic over what a process owns and are tested without any; the fd table (`kernel/src/fd.rs`), `UserSlice`'s copying out and `UserStr` (`kernel/src/mm/user.rs`) and the system-call dispatcher (`kernel/src/syscall.rs`, over its `Caller` trait) are tested against fakes; `vfs::Cwd` gives each process its current directory in the one mount table. The x86_64 parts are in `kernel/src/arch/`: the context switch and a new stack's first frame (`context.rs`), `gs` and the flags on every way in from ring 3 (`irq.rs`, `idt.rs`, `user.rs`), SMEP, SMAP and CR0.TS (`cpu.rs`). `kernel/src/proc.rs` joins them: processes on the table, blocking, the idle task, ticks, Ctrl-C and the calls' kernel side; the in-kernel shell runs as process 1 until plan 4 replaces it with `/bin/sh`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model), e2fsprogs 1.47, binutils' `readelf` and the host C library's error messages for tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§3.1, §5.1–§5.5, §6.1–§6.4, §7.1–§7.3, §8.1, §8.5, §11.1, §12, §13 step 3a, §14, §16 item 3)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 3a of 6 (plan 3 became 3a and 3b).

## In brief

- **Size.** 27 tasks in four code pull requests, plus this plan as PR 1. PR 2 is host-tested code the kernel does not use yet; PR 3 brings the processes, the switch and preemption; PR 4 Ctrl-C and the process calls from ring 3; PR 5 what the CPU enforces, and the NUC check.
- **Plan 3 is two plans** (decision 1). 3a is the processes; 3b, written after 3a merges, brings the file and memory calls, `relay-rt`'s allocator, the console's line discipline and calls, and tees.
- **The in-kernel shell is process 1** (decision 2), a process without a program on a kernel stack of its own; the boot context is the idle task. It blocks in `wait` while its command runs, and gives the command's group the console in line mode, so Ctrl-C kills the command (decision 7). Its output hook stays until plan 4, now answering each write (decision 8).
- **The kernel's own `gs` and flags on every way in from ring 3** (decision 4): the interrupt and exception stubs `swapgs` by CPL and load the kernel's flags (`push 2; popfq`) on every CPU, which does what spec §16 item 2 asked of a `clac`; the first context switch's debug assertion found a program's AC coming in through a fault before the stubs did so.
- **Two corrections to the spec's body** (PR 1): argument 0 is what the caller gives, and the shells give the name as typed (decision 12, §5.3); due sleepers are also woken where a process's ticks are counted, not only by the idle task (decision 6, §6.1).
- **The prototype's review.** A fresh reviewer read the whole prototype, ran throwaway programs and scenarios under KVM on the NUC's CPU model (an i7-1260P), and found no critical, 2 important and 4 minor real defects; each is fixed in a task of its own after the task it concerns (for the gap in the tests, a test), with a test that fails first and a mutation check. It found correct every misuse of the new calls from ring 3 it tried, 64 processes, the `gs` and flags on every path through the assembly, the context switch, no lost wake-up, no lock held across a switch, and every failure path of `spawn`:

| Finding (review) | Decision |
|---|---|
| Important: once the zombies of orphans that ended at the prompt fill the table, every program is `EAGAIN` until a reboot | Fixed, Task 17 |
| Important: an orphan's output goes into the redirection (or a script's transcript) of the command the shell waits for next | Fixed, Task 18 |
| Minor: a process killed before its first run still runs until its first system call | Fixed, Task 16 |
| Minor: no test notices `before_user` without its kill check or `collect` without its `EINTR`, nor a slice that never ends after a system call | Tested, Task 19; the last is ruled: only a program in the kernel at every tick shows it, and any test would rest on timing |
| Minor: the pid counter overflows and panics the kernel after 2^32 spawns | Fixed, Task 4 |
| Minor: `sleep(ms)` can return up to a tick early | Fixed, Task 11 |
| Declined to judge: CR4.FSGSBASE is left as the firmware set it | Cleared, Task 23, with `t-fault gsbase` on the NUC |
| Declined to judge: the keyboard is not polled while a program spends its ticks in the kernel, so a real HID keyboard can lose a key | Ruled: spec §6.3; plan 4, whose commands are such programs, polls on the way back to ring 3 (roadmap, "What plan 3a leaves for plan 4") |
| Declined to judge: keyboard endpoint recovery runs only in the idle task | Ruled: spec §6.1 keeps `usb::service` in the idle task |
| Declined to judge: a removal marks as gone only the current directory in the mount table | Ruled: harmless while programs only look paths up; plan 3b, which brings `chdir` and file creation, handles the others (roadmap) |

## Where this plan fits

Plan 3a implements the first half of spec §13 step 3. It builds on plan 2: address spaces and kernel stacks (`mm/`), `syscall`/`sysret` and the dispatcher, `UserSlice`, the ELF checks and `exec::load`, faults in ring 3, and one child at a time run to completion inside the in-kernel shell's `wait` (`enter`/`leave`), which the context switch replaces. It leaves plan 3b the dispatcher with its `Caller`, an fd table with the console and the shell's outputs, the console's mode and foreground group, and a current directory per process, for the file, memory and console calls (roadmap, "What plan 3a leaves for plan 3b"); plan 4 then moves the shell to `/bin/sh` as process 1.

## Working conventions

- Plan 3a lands as **five pull requests** (table below). This plan, with the spec's §16 item 3 (and its §5.3, §6.1, §6.2, §13 and §14 brought up to date) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. The last PR goes up as a **draft** until the user has run NUC check 3 and reported its results. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-3a-processes-and-scheduling.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.

  One more comes as a command: a `python3` command that inserts lines into a recorded transcript (they hold escape bytes, which a markdown block cannot carry).
- Each task first adds its failing tests (unit tests in each file's test module; test programs under `userland/tests/`; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Four tasks have no failing run, and their introductions say why: Task 8 moves state and changes nothing a test sees, Task 9 replaces plan 2's runner with processes that the existing scenarios and new debug assertions check, and Tasks 19 and 24 add tests of what the code already does, with the mutation checks that show what they guard. The mutation checks the prototype ran are named in each task's introduction where a guard could pass vacuously.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `23f6d61` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p3a/plan` | — | The spec's §16 item 3 and its body brought up to date (§5.3, §6.1, §6.2, §13, §14), the roadmap's plans 3a and 3b and "What plan 3a leaves for plan 3b", this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p3a/table` | 1–6 | The ABI's new structs and kill reasons; `UserSlice` copying out and `UserStr`; the process table and run queue; the fd table; a current directory per process | `lint`, `unit`, `e2e` |
| 3 | `m2p3a/sched` | 7–13 | The kernel's own `gs` and flags; the kernel's mount table and input; processes, the switch, the idle task, the shell as process 1; `time`, `sleep`; ticks that poll and preempt, `t-spin`; milestone 1's interrupt findings | `lint`, `unit`, `e2e` |
| 4 | `m2p3a/calls` | 14–20 | Ctrl-C; `spawn`, `wait`, `kill`, `getpid`, `sys_info` from ring 3, `t-spawn`; a redirection's write error | `lint`, `unit`, `e2e` |
| 5 | `m2p3a/cpu` | 21–27 | SMEP, SMAP, CR0.TS; the flag kinds of `t-fault`; plan 2's minors; NUC check 3 runs processes; NUC check 3 (a draft until its results) | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 3a adds no third-party crate. The kernel has no dev-dependencies. New host crates are `#![cfg_attr(not(test), no_std)]` and go in both `members` and `default-members`; user packages go in `members` only and in `USER_PACKAGES`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. A bad pointer is `EFAULT`, a fault in ring 3 kills only the program, and a program that never gives up the CPU is preempted. Nothing waits for ever. Panics are for kernel bugs only.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils and bash; the in-kernel shell names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 3 in PR 1:

1. **Plan 3 is two plans** (§13). 3a brings the processes: the table, the scheduler, blocking, the idle task, polling on ticks, Ctrl-C and `kill`, the process calls from ring 3, SMEP/SMAP and CR0.TS, and a NUC check. 3b brings the file, memory and remaining calls, `relay-rt`'s allocator, the console's line discipline and calls, and tees; it is written after 3a merges. Plans 4 and 5 keep their numbers.
2. **The in-kernel shell is process 1** (§6.6), a process without a program: the kernel's page tables and a kernel stack of its own. It blocks in `wait` while its command runs and on the console while it waits for a line; being kernel code, it is never preempted. The context the kernel booted in becomes the idle task, process 0, with no entry in the table. Plan 4 starts `/bin/sh` as process 1 instead.
3. **The context switch** (§5.4) saves the callee-saved registers and the flags on the process's own kernel stack and keeps only the stack pointer, by kernel-stack slot. Every switch happens in the kernel with interrupts off and no lock held, and points the CPU at the next process's page tables, TSS `rsp0` and `syscall` stack; debug assertions check the flags (none of TF, DF, AC, NT), that `rsp0` equals the `syscall` stack, and that no kernel lock is held. A new process's kernel stack starts with a frame whose return calls its function; a program's first run goes to ring 3 through `arch::user::enter`, which puts `exec::Entry` in the registers §5.3 names, so that detail stays in `arch`.
4. **Every way in from ring 3 gives the kernel its own `gs` and flags** (§6.2, §16 item 2). The kernel always runs with the per-CPU block as its `gs` base; `syscall` and the interrupt and exception stubs swap it in when they come from ring 3 (the stubs test the interrupted CPL) and back out on the way back, so every kernel context has the same `gs` and a switch never mixes them. The stubs also load the kernel's flags (`push 2; popfq`) when they interrupted ring 3, on every CPU, instead of a `clac` that exists only with SMAP: AC, which an interrupt or trap gate leaves as the program set it, never reaches a handler (the switch's assertion caught it doing so). Debug assertions in both dispatchers check the `gs` and the flags.
5. **Ending and killing** (§5.4, §11.1). A process that ends gives back its memory and fds at once and stays a zombie, with its kernel stack, until its parent collects it. `kill` and Ctrl-C mark a process and wake it if it is blocked; it ends before its program runs another instruction: on its way back to ring 3 from a system call or a tick, when its blocking point returns, or before its first instruction if it had not run yet. The kernel log says `pid <n> (<path>): killed: Ctrl-C` or `kill`. `kill` of a zombie succeeds and changes nothing; a pid or group that holds process 1 is `EPERM`, and nothing is killed; 0 names no process (`ESRCH`).
6. **Ticks** (§6.1, §6.3). A tick that interrupts ring 3 polls the console (the USB keyboards and COM1), acts on a Ctrl-C, wakes the sleepers whose time has come and ends the slice if another process is ready. A tick that interrupts the kernel is only counted, and the count is settled when the process returns to ring 3 or gives up the CPU, where the sleepers are woken too: the idle task alone would never run while a program spins. A sleep ends on the tick after the one its time reaches, so it is never shorter than asked. When the timer could not start, nothing is preempted and `sleep` returns at once (nothing waits for ever).
7. **The console's mode and foreground group** (§6.4, §6.5) exist in 3a as far as Ctrl-C needs them. The in-kernel shell gives the console to its command's group in line mode while it waits, and takes it back (raw, its own group) after. In line mode a Ctrl-C kills the foreground group and drops what was typed before it; one typed before the command has started waits in the queue and is the command's. The shell then says `^C`, as when Ctrl-C stops a built-in (status 130), and a script stops there. Reading in line mode and the `console_*` calls come with 3b, milestone 1's two serial findings with them.
8. **The in-kernel shell's output hook stays** (§16 item 2, until plan 4's tees). Each command gets fresh outputs of the shell's as its fds 1 and 2 (its descendants inherit them), whose writes reach the waiting shell (the redirection, the screen, a script's transcript), and the shell's answer is the write's result: a program sees its redirection's write error from the write that flushes the file's 4 KiB buffer on. What a program writes to another command's outputs (an orphan of an earlier command) goes to the screen, never into this command's redirection.
9. **The mount table is the kernel's** (§7.3), locked for one operation at a time, and each process has its own current directory, put in while the table works for it. A removal marks as gone only the current directory in the table at the time.
10. **The process calls** (§7.3). Paths and working directories are at most 4096 bytes (`ENAMETOOLONG`); `spawn`'s arguments are at least argument 0, each ending in its NUL (`EINVAL` otherwise); at most 8 fd pairs, a child fd below 32 and given once, a parent fd that is open; `NEW_GROUP` is the only flag. `wait`'s status memory is checked before a child is collected, so a bad pointer loses no child; `wait` takes a pid or −1 (0 and below −1 are `EINVAL`). Once pid 2^32 − 1 has been used, no process starts (`EAGAIN`) instead of the counter wrapping around. `time`'s uptime comes from the TSC. `sys_info` has the memory figures in 3a (`MemInfo`, a shorter buffer or another kind is `EINVAL` until 3b). `UserSlice` copies out whole or not at all; `UserStr` copies a byte string in and refuses a length over its limit before it reads anything.
11. **Orphans** (§5.4) pass to process 1. The in-kernel shell collects the ones that have ended before every command it starts and after every one it waited for, so their zombies never fill the table; plan 4's `/bin/sh` does so before every prompt.
12. **Argument 0** (§5.3, corrected in its body) is what the caller gives; the in-kernel shell gives the command's name as typed, as bash does, and a program still says its last path name in messages. A bare name too long for a file name is `command not found` (127), as in bash.
13. **SMEP, SMAP and CR0.TS** (§5.1, §5.5). SMEP and SMAP are on when CPUID reports them, and the kernel log says `cpu: SMEP on, SMAP on`, which the NUC's check script expects. The panic tests `panic=user-read` and `panic=user-exec` show that each is on (the scenarios `panic_smap` and `panic_smep`); QEMU's TCG with `-cpu max` has both too. CR0.TS (with MP) is set once at boot, since the kernel uses no floating point, so x87 and SSE state never passes from one program to the next. CR4.FSGSBASE is cleared whatever the firmware left, so a program cannot set its own `gs` base, which the next program would inherit (a switch saves no segment bases).
14. **Interrupts** (milestone 1's deferred findings). The LAPIC gets an EOI for any vector it has in service, 32–47 included; an unexpected vector is a storm at 1000 within one second, not over the whole uptime; the timer masks the PIC before anything can fail.
15. **Plan 2's deferred minors.** A kernel stack that cannot get frames is `ENOMEM` (a full process table stays `EAGAIN`); NMIs, double faults and machine checks have an IST stack each, so one arriving while another's handler runs keeps that one's frame.
16. **Error numbers** (§7.2). `vfs::Errno` gains `ESRCH`, `EPERM` and `EINTR` (the last never reaches a program: it ends a blocking call of one that was killed).
17. **Tests** (§8.5, §12.3). `t-spin [SECONDS]`; `t-spawn N` also prints `frames lost: <n>` for the NUC's script, `t-spawn kill` kills a spinning child while it sleeps, `kill-new` one before it has run, `orphan` leaves one behind, `fill` fills the table with napping orphans, `sleepers` blocks a group in `wait` and `sleep` for Ctrl-C; `t-fault` gains `sse`, `gsbase`, `flags-ac` (a spin through ticks with AC, NT and DF set) and `flags-tf` (the trap flag before a call that returns), and `flags-exit` sets the trap flag too. New scenarios: `ctrlc`, `spawn`, `panic_smap`, `panic_smep`.
18. **Check scripts** (§12.4). `check3-a.sh` runs `t-spin 1`, `t-spawn 100`, `t-spawn kill`, `t-fault sse`, `flags-ac` and `gsbase`, and expects the `cpu:` line; two steps follow by hand (typing during `t-spin 5`, and Ctrl-C of `t-spin`). The recorded transcripts get those lines by hand until plan 3a's NUC check records real ones.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A program that misuses the new calls:** a `SpawnArgs` at a bad address or running off its page; a path or working directory over 4096 bytes, arguments over 64 KiB, arguments without their final NUL or none at all; 9 fd pairs, a child fd twice or at 32 and above, a parent fd that is not open, unknown flags; `wait` with a bad status pointer, pid 0, −2 or above 2^32, unknown flags; `kill` of 0, −1 or a pid nobody has; `sys_info` with a short buffer or another kind; `time` into read-only memory. Expected: `EFAULT`, `ENAMETOOLONG`, `E2BIG`, `EINVAL`, `EBADF`, `ECHILD`, `ESRCH` or `EPERM`, with nothing started or collected, never a kernel fault, a panic or a hang. Tests: `a_range_is_written_across_writable_pages`, `a_write_that_touches_a_page_it_may_not_write_writes_nothing`, `a_string_is_copied_whole_and_no_longer_than_its_limit` (Task 2), `kill_refuses_process_1_and_what_does_not_exist` (Task 3), `a_bad_mapping_is_refused` (Task 5), `time_fills_in_the_clock_s_answer` (Task 10), `spawn_copies_in_everything_its_struct_names`, `spawn_refuses_what_is_not_a_valid_request`, `a_bad_status_pointer_loses_no_child`, `wait_refuses_what_it_cannot_mean`, `kill_getpid_and_the_memory_figures`, `an_fd_that_is_not_open_is_ebadf` (Task 15).
2. **A program that never gives up the CPU:** `t-spin` for ever or for a while, typing on the USB keyboard while it runs, Ctrl-C to stop it (also pressed before it has started), a parent that sleeps while its child spins. Expected: every key typed arrives (no stuck key), Ctrl-C ends it at once with `^C` and status 130, the sleeper wakes on time and the other processes get the CPU. Tests: `a_slice_is_10_ticks_and_ends_only_for_another_process`, `sleepers_wake_when_their_tick_comes` (Task 3), the `ctrlc` scenario (Tasks 12 and 14), `a_sleep_is_never_shorter_than_asked` (Task 11), `t-spawn kill` in the `spawn` scenario (Task 15), `t-spawn sleepers` in the `ctrlc` scenario (Task 19), `t-spin 1` in `check3-a.sh` (Task 27).
3. **Many programs and what they leave behind:** a thousand children in a row, 64 processes at once, children that outlive their parent and end while the shell sits at its prompt, zombies nobody waits for, a child killed before it has run, a whole group killed by Ctrl-C, no frames left for a kernel stack, a spawning loop that runs for days. Expected: `EAGAIN` beyond 64 processes (and once every pid has been used), `ENOMEM` when the frames run out, orphans pass to process 1, which collects them before and after each command, a child killed before it ran runs none of its code, and no frame is lost (`free` and `t-spawn`'s counts). Tests: `at_most_64_processes_zombies_included`, `an_ended_child_is_a_zombie_until_its_parent_waits`, `orphans_pass_to_process_1_which_is_woken_to_collect_them`, `process_1_is_woken_for_a_grandchild_s_zombie`, `kill_marks_a_process_or_a_group_and_wakes_the_blocked` (Task 3), `when_every_pid_has_been_used_no_process_starts` (Task 4), the `spawn` scenario (Task 15, and Tasks 16 and 17 for `t-spawn kill-new` and `fill`), `running_out_of_frames_leaks_nothing` (Task 25), `frames lost: 0` in `check3-a.sh` (Task 27).
4. **A program that sets flags or does what the kernel does not allow:** `popfq` with AC, NT, DF or TF before a call, before a fault, or through a spin of many ticks; an SSE instruction; `wrgsbase`; and the kernel itself touching a program's page. Expected: nothing of the program's reaches kernel code (every handler and every switch has the kernel's `gs` and flags, which debug assertions check), SSE is `killed (FPU/SSE instruction, ip …)` with status 136, `wrgsbase` is an invalid opcode whatever the firmware left in CR4, the trap flag traps in ring 3 after `sysret`, and SMEP and SMAP turn a kernel access into the panic screen. Tests: `an_interrupt_of_ring_3_gets_the_kernel_s_gs_and_flags`, `an_exception_in_ring_3_gets_the_kernel_s_gs_and_flags`, `a_program_starts_with_its_own_gs` (Task 7), `a_first_frame_is_what_switch_pops` (Task 9), the `userfault` scenario with `flags-exit`, `flags-ud`, `flags-ac`, `flags-tf`, `sse` and `gsbase` (Tasks 7, 21, 22, 23 and 24), `cr4_gets_bits_20_and_21_and_keeps_the_rest` (Tasks 21 and 23), `panic_smap`, `panic_smep` (Task 21).
5. **Ctrl-C and a program's output at the in-kernel shell:** Ctrl-C at the prompt, during a built-in, during a program inside a script, and before a command has started; a program's output redirected to a full disk; an orphan of an earlier command writing while the next one runs, redirected. Expected: at the prompt it cancels the line, a built-in stops as in milestone 1, a program is killed with `^C` and status 130 and a script stops there; a program's writes to a full redirection get `ENOSPC` from the write that flushes the file's 4 KiB buffer on, and the shell reports the write error; the orphan's output goes to the screen, never into the other command's file. Tests: `ctrl_c_and_kill_get_bash_s_statuses` (Task 1), the `ctrlc` scenario and `a_program_stopped_by_ctrl_c_says_only_so_and_ends_a_script` (Task 14), `a_program_sees_its_redirection_s_write_error` (Task 20), `a_file_put_in_replaces_the_old_one_for_later_children_only` and the `spawn` scenario's `t-spin 2 > /root/o.txt` (Task 18).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/relay-abi/src/{spawn,info,wait}.rs` | `SpawnArgs`, `FdMap`, `wait`'s flags; `Time`, `MemInfo`; the kill reasons |
| `crates/vfs/src/{errno,mount}.rs` | `ESRCH`, `EPERM`, `EINTR`; `Cwd`, a current directory per process |
| `crates/shell/src/{io,ctx,shell,killed,testing}.rs` | The output hook's answer; `^C` for a program; argument 0 as typed; the kill reasons' statuses |
| `crates/relay-rt/src/{sys,args}.rs` | `spawn`, `wait`, `kill`, `getpid`, `memory`, `time`, `sleep` |
| `kernel/src/proc.rs`, `kernel/src/proc/table.rs` | Processes: the table and run queue; switching, blocking, the idle task, ticks, Ctrl-C, the calls' kernel side |
| `kernel/src/{fd,mounts,tty,syscall}.rs` | The fd table; the kernel's mount table; the console's input, mode and foreground group; the dispatcher |
| `kernel/src/arch/{context,cpu}.rs` | The context switch and first frames; SMEP, SMAP, CR0.TS |
| `kernel/src/arch/{irq,idt,user,gdt,lapic}.rs` | `gs` and flags from ring 3; `enter(&Entry)`; the EOI rule and storms; an IST stack each |
| `kernel/src/mm/{user,kstack,mod}.rs`, `kernel/src/{timer,cmdline,lib,session,exec}.rs` | Copying out, `UserStr`; `StackError`; the PIC masked first; the SMAP and SMEP panic tests; process 1 |
| `userland/tests/src/bin/{t-spin,t-spawn,t-fault}.rs` | `t-spin`, `t-spawn`, the new `t-fault` kinds |
| `tests/e2e/{ctrlc,spawn,panic_smap,panic_smep,userfault,system}.txt` | The new scenarios; `/bin` with four programs |
| `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/check3-a.{qemu,nuc}.log` | NUC check 3 runs processes |
| `README.md`, `docs/hardware-test.md` | What milestone 2 does now; check 3's new lines, steps by hand and failures |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 3 (with its §5.3, §6.1, §6.2, §13 and §14 brought up to date), the roadmap's plans 3a and 3b and "What plan 3a leaves for plan 3b", and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 3a, and the roadmap's plans 3a and 3b`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3a/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p3/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-3a-processes-and-scheduling.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-30-m2-plan-3a-processes-and-scheduling.md
git commit -m "docs: plan 3a of milestone 2, processes and scheduling"
cargo xtask lint
git push -u origin m2p3a/plan
gh pr create --base main --head m2p3a/plan --title "Milestone 2, plan 3a: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 3a ("processes and scheduling"), the spec's §16 item 3 with its decisions (among them: plan 3 becomes plans 3a and 3b, spec §13; the in-kernel shell becomes process 1; every way in from ring 3 gives the kernel its own `gs` and flags, which does what §16 item 2 asked of a `clac`; argument 0 is what the caller gives, §5.3; due sleepers are woken wherever a process's ticks are counted, §6.1), and the roadmap's plans 3a and 3b with what plan 3a leaves for plan 3b.

## How it was tested

- [x] Every task of plan 3a was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p3a/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p3a/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-plan` and continue with PR 2.

---

## PR 2: What processes are made of (Tasks 1–6)

All host-tested, and nothing the kernel runs uses it yet: the ABI's structs for the new calls and the kill reasons, `UserSlice` copying out and `UserStr`, the process table with its run queue (and a review fix: no pid counter overflow), the fd table, and a current directory per process.

Branch `m2p3a/table`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-table`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3a/table /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-table origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-table
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3a/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3a/table /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-table m2p3a/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3a/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: What spawn, wait, time and sys_info pass, and why a program was killed

The calls plan 3a brings pass structs the ABI does not have yet (spec §7.3): `SpawnArgs` (the path, the argument bytes, the working directory, up to 8 `FdMap` pairs and the `NEW_GROUP` flag), `Time` (Unix seconds and the uptime in nanoseconds) and `MemInfo` (`free`'s figures, `sys_info`'s memory kind), with `wait`'s `WAIT_ANY` and `WAIT_NOHANG`. Like `WaitStatus`, each is `#[repr(C)]` with fixed-width fields and no padding, and a test checks its size and every field offset (§7.1); the kernel reads a program's `SpawnArgs` from its bytes (`from_bytes`), which a test compares with the struct itself. `WaitStatus` gains the two kill reasons of §11.1, `KILLED_CTRL_C` and `KILLED_KILL` (`killed(reason)`), whose words are `Ctrl-C` and `kill` for the kernel log; the shell's `killed` gives them bash's statuses, 130 and 137, and only `^C` as the words for Ctrl-C, as when Ctrl-C stops a built-in. `vfs::Errno` gains `ESRCH` and `EPERM` for `kill` (decision 16), checked against the host C library as plan 2's are. Mutation checks: a wrong offset, number or message fails a test.

**Files:**
- Create: `crates/relay-abi/src/info.rs`
- Modify: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-abi/src/spawn.rs`
- Modify: `crates/relay-abi/src/wait.rs`
- Modify: `crates/shell/src/killed.rs`
- Modify: `crates/vfs/src/errno.rs`

**Interfaces:**
- Consumes: plan 2's `relay_abi::WaitStatus`, `vfs::Errno::number`, `shell::killed::killed`.
- Produces: `relay_abi::{SpawnArgs, FdMap, Time, MemInfo}`, `relay_abi::spawn::{SPAWN_FDS, NEW_GROUP, WAIT_ANY, WAIT_NOHANG}`, `SpawnArgs::{SIZE, from_bytes(&[u8; SIZE]) -> SpawnArgs}`, `relay_abi::info::INFO_MEMORY`, `relay_abi::wait::{KILLED_CTRL_C, KILLED_KILL}`, `WaitStatus::killed(reason) -> WaitStatus`; `vfs::Errno::{ESRCH, EPERM}`; `shell::killed::INTERRUPTED` (130).

- [ ] **Step 1: Write the failing tests for `crates/relay-abi/src/info.rs`**

Create `crates/relay-abi/src/info.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layouts_are_fixed() {
        assert_eq!(size_of::<Time>(), 16);
        assert_eq!(offset_of!(Time, unix_seconds), 0);
        assert_eq!(offset_of!(Time, uptime_ns), 8);
        assert_eq!(size_of::<MemInfo>(), 32);
        assert_eq!(offset_of!(MemInfo, ram_total), 0);
        assert_eq!(offset_of!(MemInfo, ram_free), 8);
        assert_eq!(offset_of!(MemInfo, heap_total), 16);
        assert_eq!(offset_of!(MemInfo, heap_used), 24);
        assert_eq!(INFO_MEMORY, 1);
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub mod errno;
mod result;
pub mod wait;
````

with:

````rust
pub mod errno;
pub mod info;
mod result;
pub mod spawn;
pub mod wait;
````

- [ ] **Step 3: Write the failing tests for `crates/relay-abi/src/spawn.rs`**

Create `crates/relay-abi/src/spawn.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<FdMap>(), 8);
        assert_eq!(offset_of!(FdMap, child), 0);
        assert_eq!(offset_of!(FdMap, parent), 4);
        assert_eq!(SpawnArgs::SIZE, 120);
        assert_eq!(offset_of!(SpawnArgs, path), 0);
        assert_eq!(offset_of!(SpawnArgs, path_len), 8);
        assert_eq!(offset_of!(SpawnArgs, args), 16);
        assert_eq!(offset_of!(SpawnArgs, args_len), 24);
        assert_eq!(offset_of!(SpawnArgs, cwd), 32);
        assert_eq!(offset_of!(SpawnArgs, cwd_len), 40);
        assert_eq!(offset_of!(SpawnArgs, fds), 48);
        assert_eq!(offset_of!(SpawnArgs, fd_count), 112);
        assert_eq!(offset_of!(SpawnArgs, flags), 116);
        assert_eq!((NEW_GROUP, WAIT_NOHANG, WAIT_ANY), (1, 1, -1));
    }

    #[test]
    fn a_program_s_bytes_read_back_as_its_struct() {
        let mut fds = [FdMap::default(); SPAWN_FDS];
        for (k, fd) in fds.iter_mut().enumerate() {
            *fd = FdMap {
                child: k as u32,
                parent: 100 + k as u32,
            };
        }
        let a = SpawnArgs {
            path: 0x40_1000,
            path_len: 11,
            args: 0x40_2000,
            args_len: 22,
            cwd: 0x40_3000,
            cwd_len: 33,
            fds,
            fd_count: 3,
            flags: NEW_GROUP,
        };
        // SAFETY: `SpawnArgs` is `repr(C)` of integers with no padding.
        let bytes: [u8; SpawnArgs::SIZE] = unsafe { core::mem::transmute(a) };
        assert_eq!(SpawnArgs::from_bytes(&bytes), a);
    }
}
````

- [ ] **Step 4: Add the failing tests to `crates/relay-abi/src/wait.rs`**

In `crates/relay-abi/src/wait.rs`, replace:

````rust
        assert!(!other_reason.is_known_fault());
        assert!(WaitStatus::fault(FAULT_OTHER, 3, 0, 0).is_known_fault());
````

with:

````rust
        assert!(!other_reason.is_known_fault());
    }

    #[test]
    fn ctrl_c_and_kill_are_kills_with_their_reason() {
        let c = WaitStatus::killed(KILLED_CTRL_C);
        assert_eq!((c.how, c.code), (KILLED, 2));
        assert_eq!(c.to_string(), "Ctrl-C");
        assert!(!c.is_known_fault());
        let k = WaitStatus::killed(KILLED_KILL);
        assert_eq!((k.how, k.code), (KILLED, 3));
        assert_eq!(k.to_string(), "kill");
        assert_eq!(
            k,
            WaitStatus {
                how: KILLED,
                code: 3,
                ..WaitStatus::default()
            }
        );
        assert_eq!([KILLED_FAULT, KILLED_CTRL_C, KILLED_KILL], [1, 2, 3]);
        assert!(WaitStatus::fault(FAULT_OTHER, 3, 0, 0).is_known_fault());
````

- [ ] **Step 5: Add the failing tests to `crates/shell/src/killed.rs`**

In `crates/shell/src/killed.rs`, replace:

````rust
    }
}
````

with:

````rust
    }

    #[test]
    fn ctrl_c_and_kill_get_bash_s_statuses() {
        assert_eq!(
            killed(&WaitStatus::killed(KILLED_CTRL_C)),
            ("^C".into(), 130)
        );
        assert_eq!(
            killed(&WaitStatus::killed(relay_abi::wait::KILLED_KILL)),
            ("killed".into(), 137)
        );
    }
}
````

- [ ] **Step 6: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 21] = [
        Errno::ENOENT,
````

with:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 23] = [
        Errno::ENOENT,
````

Replace:

````rust
        Errno::ENOSYS,
    ];
````

with:

````rust
        Errno::ENOSYS,
        Errno::ESRCH,
        Errno::EPERM,
    ];
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p relay-abi -p vfs -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Time` in this scope ``; `` cannot find type `MemInfo` in this scope ``.

- [ ] **Step 8: Implement `crates/relay-abi/src/info.rs`**

Insert this at the top of `crates/relay-abi/src/info.rs`, above `#[cfg(test)]`:

````rust
//! What `time` and `sys_info` fill in (spec §7.3).

/// `time`'s answer, `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// Wall-clock seconds since 1970, UTC (0 without a clock).
    pub unix_seconds: u64,
    /// Nanoseconds since the kernel's timer started.
    pub uptime_ns: u64,
}

/// `sys_info`'s kind for memory figures ([`MemInfo`]).
pub const INFO_MEMORY: u32 = 1;

/// The memory figures of M1's `free`, in bytes, `#[repr(C)]` with no
/// padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub ram_total: u64,
    pub ram_free: u64,
    pub heap_total: u64,
    pub heap_used: u64,
}

````

- [ ] **Step 9: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub use call::Call;
pub use result::{MAX_ERRNO, decode, encode};
pub use wait::WaitStatus;
````

with:

````rust
pub use call::Call;
pub use info::{MemInfo, Time};
pub use result::{MAX_ERRNO, decode, encode};
pub use spawn::{FdMap, SpawnArgs};
pub use wait::WaitStatus;
````

- [ ] **Step 10: Implement `crates/relay-abi/src/spawn.rs`**

Insert this at the top of `crates/relay-abi/src/spawn.rs`, above `#[cfg(test)]`:

````rust
//! What `spawn` and `wait` take (spec §7.3): the new program's path,
//! arguments, working directory and file descriptors, and `wait`'s flags.

/// How many fds `spawn` can hand a child.
pub const SPAWN_FDS: usize = 8;

/// `SpawnArgs::flags`: the child starts a process group of its own, with
/// its pid as the group's number, instead of joining its parent's (spec
/// §6.4).
pub const NEW_GROUP: u32 = 1;

/// `wait`'s pid for any child.
pub const WAIT_ANY: i64 = -1;
/// `wait`'s flags: return 0 at once when no child has ended.
pub const WAIT_NOHANG: u32 = 1;

/// One of the child's fds: `child` gets what the caller has open as
/// `parent`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FdMap {
    pub child: u32,
    pub parent: u32,
}

/// `spawn`'s argument, `#[repr(C)]` with no padding. Every pointer and
/// length names bytes of the caller's memory.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnArgs {
    /// The program's path, relative to the caller's working directory.
    pub path: u64,
    pub path_len: u64,
    /// The arguments, argument 0 first, each followed by a NUL.
    pub args: u64,
    pub args_len: u64,
    /// The child's working directory, relative to the caller's; empty for
    /// the caller's own.
    pub cwd: u64,
    pub cwd_len: u64,
    /// The first `fd_count` entries are the child's fds; every other fd of
    /// the child is closed.
    pub fds: [FdMap; SPAWN_FDS],
    pub fd_count: u32,
    /// [`NEW_GROUP`] or 0.
    pub flags: u32,
}

impl SpawnArgs {
    /// Its size in bytes.
    pub const SIZE: usize = core::mem::size_of::<SpawnArgs>();

    /// The struct whose bytes (as the calling program laid them out) are
    /// `b`.
    pub fn from_bytes(b: &[u8; SpawnArgs::SIZE]) -> SpawnArgs {
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let mut fds = [FdMap::default(); SPAWN_FDS];
        for (k, fd) in fds.iter_mut().enumerate() {
            *fd = FdMap {
                child: u32_at(48 + 8 * k),
                parent: u32_at(52 + 8 * k),
            };
        }
        SpawnArgs {
            path: u64_at(0),
            path_len: u64_at(8),
            args: u64_at(16),
            args_len: u64_at(24),
            cwd: u64_at(32),
            cwd_len: u64_at(40),
            fds,
            fd_count: u32_at(112),
            flags: u32_at(116),
        }
    }
}

````

- [ ] **Step 11: Change `crates/relay-abi/src/wait.rs`**

In `crates/relay-abi/src/wait.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! How a child ended, as `wait` reports it (spec §7.3): it exited with a
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction. Its `Display` says so in the
//! words of spec §11.1 (`page fault at 0x0, read, ip 0x401a2c`), for the
//! kernel log and the shells.

````

with:

````rust
//! How a child ended, as `wait` reports it (spec §7.3): it exited with a
//! code, or it was killed, with the reason (a fault, Ctrl-C, `kill`) and,
//! for a fault, what the CPU refused, where and at which instruction. Its
//! `Display` says so in the words of spec §11.1 (`page fault at 0x0, read,
//! ip 0x401a2c`), for the kernel log and the shells.

````

Replace:

````rust
pub const KILLED_FAULT: u32 = 1;

````

with:

````rust
pub const KILLED_FAULT: u32 = 1;
/// `WaitStatus::code` of a killed child: Ctrl-C, typed while its process
/// group had the console (spec §6.4).
pub const KILLED_CTRL_C: u32 = 2;
/// `WaitStatus::code` of a killed child: the `kill` call.
pub const KILLED_KILL: u32 = 3;

````

Replace:

````rust

    /// A child killed for `fault` at instruction `ip`.
````

with:

````rust

    /// A child killed for `reason` ([`KILLED_CTRL_C`] or [`KILLED_KILL`]).
    pub const fn killed(reason: u32) -> WaitStatus {
        WaitStatus {
            how: KILLED,
            code: reason,
            fault: 0,
            detail: 0,
            address: 0,
            ip: 0,
        }
    }

    /// A child killed for `fault` at instruction `ip`.
````

Replace:

````rust
    /// `exited with 3`; for a fault what it was, where and at which
    /// instruction; `killed` for any other end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.how == EXITED {
            return write!(f, "exited with {}", self.code);
        }
````

with:

````rust
    /// `exited with 3`; for a fault what it was, where and at which
    /// instruction; `Ctrl-C` or `kill` for those kills; `killed` for any
    /// other end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.how == EXITED {
            return write!(f, "exited with {}", self.code);
        }
        if self.how == KILLED && self.code == KILLED_CTRL_C {
            return f.write_str("Ctrl-C");
        }
        if self.how == KILLED && self.code == KILLED_KILL {
            return f.write_str("kill");
        }
````

- [ ] **Step 12: Change `crates/shell/src/killed.rs`**

In `crates/shell/src/killed.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! 0x401a2c)`, and the status bash would give for the signal Linux would
//! send (128 + 11 for SIGSEGV, + 4 for SIGILL, + 8 for SIGFPE).

````

with:

````rust
//! 0x401a2c)`, and the status bash would give for the signal Linux would
//! send (128 + 11 for SIGSEGV, + 4 for SIGILL, + 8 for SIGFPE, + 2 for
//! SIGINT on a Ctrl-C, + 9 for SIGKILL on a `kill`).

````

Replace:

````rust

/// A program killed with no reason the shell knows (bash's SIGKILL).
pub const KILLED: i32 = 128 + 9;

/// The words for a program that did not exit, and its exit status.
pub fn killed(w: &WaitStatus) -> (String, i32) {
    if !w.is_known_fault() {
````

with:

````rust

/// A program killed with `kill`, or with no reason the shell knows
/// (bash's SIGKILL).
pub const KILLED: i32 = 128 + 9;
/// A program killed with Ctrl-C (bash's SIGINT), as a built-in Ctrl-C
/// stops.
pub const INTERRUPTED: i32 = 128 + 2;

/// The words for a program that did not exit, and its exit status. A
/// Ctrl-C is only `^C`, as the shell says when Ctrl-C stops a built-in.
pub fn killed(w: &WaitStatus) -> (String, i32) {
    if w.how == relay_abi::wait::KILLED && w.code == KILLED_CTRL_C {
        return (String::from("^C"), INTERRUPTED);
    }
    if !w.is_known_fault() {
````

- [ ] **Step 13: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    ENOSYS,
}
````

with:

````rust
    ENOSYS,
    /// No process or process group by that number.
    ESRCH,
    /// Process 1 cannot be killed.
    EPERM,
}
````

Replace:

````rust
            Errno::ENOSYS => "Function not implemented",
        }
````

with:

````rust
            Errno::ENOSYS => "Function not implemented",
            Errno::ESRCH => "No such process",
            Errno::EPERM => "Operation not permitted",
        }
````

Replace:

````rust
            Errno::ENOSYS => n::ENOSYS,
        }
````

with:

````rust
            Errno::ENOSYS => n::ENOSYS,
            Errno::ESRCH => n::ESRCH,
            Errno::EPERM => n::EPERM,
        }
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-abi -p vfs -p shell`

Expected: PASS: 186 tests.

- [ ] **Step 15: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 16: Commit**

````bash
git add crates
git commit -m "relay-abi: what spawn, wait, time and sys_info pass, and the kill reasons Ctrl-C and kill"
````


### Task 2: `UserSlice` copies out, and `UserStr` copies a byte string in

Plan 2's `UserSlice` only copies in (decision 4 of spec §16 item 2); `wait`, `time` and `sys_info` must write into a program's memory, and `spawn` reads a path and arguments of the program's choosing. `UserSlice::write` checks every page the bytes cover (the program's, and writable: code and read-only data are `EFAULT` as a hole is) before it writes any of them, so a `WaitStatus` or a `Time` arrives whole or not at all (decision 10). `UserStr` is a byte string with a limit: a length over it is refused with the caller's error (`ENAMETOOLONG` for a path, `E2BIG` for arguments) before anything is read or allocated, and a range outside the lower half is `EFAULT`; `read` copies it into the kernel. Both copy through the linear map after the walk of the program's tables, as `read` does, so SMAP needs no `stac`/`clac`. Mutation checks: a write accepting any mapped page, a pre-check that stops at a bad page instead of refusing, a write without the range-end check and a limit off by one all fail a test.

**Files:**
- Modify: `kernel/src/mm/user.rs`

**Interfaces:**
- Consumes: plan 2's `mm::user::UserSlice`, `AddressSpace::user_page`, `Perm`.
- Produces: `UserSlice::write(&self, &AddressSpace, &mut impl PhysMem, offset: u64, bytes: &[u8]) -> Result<(), Errno>`; `mm::user::UserStr::{new(addr, len, max: usize, too_long: Errno) -> Result<UserStr, Errno>, read(&self, &AddressSpace, &mut impl PhysMem) -> Result<Vec<u8>, Errno>}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/mm/user.rs`**

In `kernel/src/mm/user.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    /// Two readable pages at `U` (code, then read-only data), a hole, then
    /// a writable page; and the last page of the lower half.
    fn program(m: &mut FakeMem) -> AddressSpace {
````

with:

````rust
    /// Two readable pages at `U` (code, then read-only data), a hole, then
    /// two writable pages; and the last page of the lower half.
    fn program(m: &mut FakeMem) -> AddressSpace {
````

Replace:

````rust
        s.map_zeroed(m, U + PAGE, 1, Perm::Read).unwrap();
        s.map_zeroed(m, U + 3 * PAGE, 1, Perm::ReadWrite).unwrap();
        s.map_zeroed(m, LOWER_HALF_END - PAGE, 1, Perm::ReadWrite)
````

with:

````rust
        s.map_zeroed(m, U + PAGE, 1, Perm::Read).unwrap();
        s.map_zeroed(m, U + 3 * PAGE, 2, Perm::ReadWrite).unwrap();
        s.map_zeroed(m, LOWER_HALF_END - PAGE, 1, Perm::ReadWrite)
````

Replace:

````rust
        assert_eq!(UserSlice::new(U, 7).unwrap().len(), 7);
    }
}
````

with:

````rust
        assert_eq!(UserSlice::new(U, 7).unwrap().len(), 7);
    }

    fn write(m: &mut FakeMem, s: &AddressSpace, addr: u64, bytes: &[u8]) -> Result<(), Errno> {
        UserSlice::new(addr, bytes.len() as u64)?.write(s, m, 0, bytes)
    }

    #[test]
    fn a_range_is_written_across_writable_pages() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let at = U + 4 * PAGE - 3;
        assert_eq!(write(&mut m, &s, at, b"abcdefgh"), Ok(()));
        assert_eq!(read(&mut m, &s, at, 8), Ok(b"abcdefgh".to_vec()));
        // At an offset in a longer range.
        let slice = UserSlice::new(U + 3 * PAGE, 100).unwrap();
        slice.write(&s, &mut m, 90, b"xyz").unwrap();
        assert_eq!(read(&mut m, &s, U + 3 * PAGE + 90, 3), Ok(b"xyz".to_vec()));
        assert_eq!(
            slice.write(&s, &mut m, 98, b"xyz"),
            Err(Errno::EFAULT),
            "past its end"
        );
        assert_eq!(slice.write(&s, &mut m, u64::MAX, b"x"), Err(Errno::EFAULT));
        assert_eq!(write(&mut m, &s, LOWER_HALF_END - 2, b"ok"), Ok(()));
    }

    #[test]
    fn a_write_that_touches_a_page_it_may_not_write_writes_nothing() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        for (addr, why) in [
            (U, "code"),
            (U + PAGE, "read-only data"),
            (U + 2 * PAGE, "the hole"),
            (U + 5 * PAGE - 2, "past the writable pages"),
            (0, "null"),
        ] {
            assert_eq!(
                write(&mut m, &s, addr, b"abcd"),
                Err(Errno::EFAULT),
                "{why}"
            );
        }
        // The last write began on a writable page: its bytes there are not
        // written either.
        assert_eq!(read(&mut m, &s, U + 5 * PAGE - 2, 2), Ok(vec![0; 2]));
        assert_eq!(
            write(&mut m, &s, 0xFFFF_8000_0000_0000, b"k"),
            Err(Errno::EFAULT),
            "the kernel"
        );
        let code = read(&mut m, &s, U, 4).unwrap();
        assert_eq!(code, [0, 1, 2, 3], "the code is as it was");
    }

    #[test]
    fn a_string_is_copied_whole_and_no_longer_than_its_limit() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let str = UserStr::new(U + PAGE - 2, 4, 4, Errno::ENAMETOOLONG).unwrap();
        assert_eq!(str.read(&s, &mut m), Ok(vec![254, 255, 0, 1]));
        assert_eq!(
            UserStr::new(U, 5, 4, Errno::ENAMETOOLONG),
            Err(Errno::ENAMETOOLONG),
            "one byte too long"
        );
        assert_eq!(
            UserStr::new(U, u64::MAX, 4, Errno::E2BIG),
            Err(Errno::E2BIG),
            "the length decides before the address"
        );
        assert_eq!(
            UserStr::new(u64::MAX, 2, 4, Errno::E2BIG),
            Err(Errno::EFAULT)
        );
        let hole = UserStr::new(U + 2 * PAGE - 1, 2, 4, Errno::E2BIG).unwrap();
        assert_eq!(hole.read(&s, &mut m), Err(Errno::EFAULT));
        let empty = UserStr::new(0, 0, 4, Errno::E2BIG).unwrap();
        assert_eq!(empty.read(&s, &mut m), Ok(vec![]));
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib mm::user`

Expected: FAIL: compile errors such as `` cannot find type `UserStr` in this scope ``; `` no method named `write` found for struct `mm::user::UserSlice` in the current scope ``.

- [ ] **Step 3: Change `kernel/src/mm/user.rs`**

In `kernel/src/mm/user.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! range of its own address space, checked page by page against its page
//! tables before a byte is copied. A pointer into the upper half, past the
//! lower half's end, onto a page the program does not have, or one that
````

with:

````rust
//! range of its own address space, checked page by page against its page
//! tables before a byte is copied in or out. A pointer into the upper half, past the
//! lower half's end, onto a page the program does not have, or one that
````

Replace:

````rust

use super::paging::{LOWER_HALF_END, PAGE, PhysMem};
use super::space::AddressSpace;
use vfs::Errno;
````

with:

````rust

use super::paging::{LOWER_HALF_END, PAGE, Perm, PhysMem};
use super::space::AddressSpace;
use alloc::vec::Vec;
use vfs::Errno;
````

Replace:

````rust
        Ok(())
    }
````

with:

````rust
        Ok(())
    }

    /// Copies `bytes` into the range from `offset`. Every page they cover
    /// must be one of the program's and writable, or nothing is written
    /// and the answer is `EFAULT`: a `WaitStatus` or a `Time` arrives
    /// whole or not at all.
    pub fn write(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), Errno> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|&end| end <= self.len)
            .ok_or(Errno::EFAULT)?;
        let (start, end) = (self.addr + offset, self.addr + end);
        let mut page = start - start % PAGE;
        while page < end {
            match space.user_page(mem, page) {
                Some((_, Perm::ReadWrite)) => page += PAGE,
                _ => return Err(Errno::EFAULT),
            }
        }
        let mut virt = start;
        let mut done = 0;
        while virt < end {
            let (frame, _) = space.user_page(mem, virt).ok_or(Errno::EFAULT)?;
            let at = (virt % PAGE) as usize;
            let n = (PAGE as usize - at).min(bytes.len() - done);
            mem.bytes(frame)[at..at + n].copy_from_slice(&bytes[done..done + n]);
            done += n;
            virt += n as u64;
        }
        Ok(())
    }
}

/// A byte string a program passes (a path, its children's arguments):
/// `len` bytes at `addr`, at most `max` of them. Paths and arguments are
/// bytes, not necessarily UTF-8, and carry their length (spec §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserStr {
    slice: UserSlice,
}

impl UserStr {
    /// `too_long` (`ENAMETOOLONG` for a path, `E2BIG` for arguments) if
    /// `len` is over `max`, before anything is read or allocated; `EFAULT`
    /// if the range is not in the lower half.
    pub fn new(addr: u64, len: u64, max: usize, too_long: Errno) -> Result<UserStr, Errno> {
        if len > max as u64 {
            return Err(too_long);
        }
        Ok(UserStr {
            slice: UserSlice::new(addr, len)?,
        })
    }

    /// The bytes, copied into the kernel; `EFAULT` if any of them is not
    /// the program's.
    pub fn read(&self, space: &AddressSpace, mem: &mut impl PhysMem) -> Result<Vec<u8>, Errno> {
        let mut buf = alloc::vec![0; self.slice.len() as usize];
        self.slice.read(space, mem, 0, &mut buf)?;
        Ok(buf)
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib mm::user`

Expected: PASS: 7 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: UserSlice copies out whole or not at all, and UserStr copies a program's byte string in"
````


### Task 3: The process table and the run queue

Spec §5.4 and §6.1, architecture-neutral and host-tested: `proc::table::Table<R>` holds at most 64 processes (`EAGAIN` beyond, zombies included), with pids from 1 that are never used again, each with its parent, its process group (its parent's, or a new one numbered with its own pid), its state (ready, running, blocked on a child, the console or a tick count, or a zombie with its `WaitStatus`), the reason it was killed, its CPU ticks and a payload `R` for what the kernel adds (its memory, kernel stack, fds; the tests use none). The run queue is round-robin: `schedule` gives up the CPU for the running process and picks the next ready one, or 0, the idle task, which has no entry; `block`, `wake`, `wake_all` and `wake_sleepers` move processes between blocked and ready; `tick` counts a tick and says when a slice of 10 is used up while another process is ready (a slice nobody else wants starts again). `end` makes a zombie, passes its children to process 1 and wakes a parent blocked in `wait`, and process 1 too when an orphan that has ended passes to it; `reap` takes a zombie child out (`ECHILD` without such a child); `kill` marks a process or a group and wakes the blocked ones (`ESRCH` for none, `EPERM` for process 1's, the first reason stays, a zombie counts as existing and keeps how it ended; decision 5). Mutation checks: orphans kept by their dead parent, process 1 not refused, a later reason overwriting, a slice ending without another ready process, a yielded process not queued again, a zombie marked, process 1 not woken for an orphan's zombie, room for 65 and a group kill by pid all fail a test.

**Files:**
- Modify: `kernel/src/proc.rs`
- Create: `kernel/src/proc/table.rs`

**Interfaces:**
- Consumes: Task 1 (`WaitStatus`, `KILLED_CTRL_C`, `KILLED_KILL`, `ESRCH`, `EPERM`).
- Produces: `proc::table::{MAX, INIT, SLICE, Blocked::{Wait, Console, Sleep(u64)}, State::{Ready, Running, Blocked, Zombie(WaitStatus)}, Process<R> {pid, ppid, pgid, name, state, killed, ticks, res}, Want::{Any, Pid(u32)}, Table<R>}` with `Table::{new, len, is_empty, current, get, get_mut, iter, insert(ppid, new_group, name, res) -> Result<u32, Errno>, schedule() -> u32, block(Blocked), wake(pid) -> bool, wake_all(Blocked), wake_sleepers(now), others_ready() -> bool, tick() -> bool, end(pid, WaitStatus), reap(parent, Want) -> Result<Option<Process<R>>, Errno>, kill(target: i64, reason) -> Result<(), Errno>}`.

- [ ] **Step 1: Declare the new module in `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

````

with:

````rust
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

pub mod table;

````

- [ ] **Step 2: Write the failing tests for `kernel/src/proc/table.rs`**

Create `kernel/src/proc/table.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{KILLED_CTRL_C, KILLED_KILL};

    fn table() -> Table<()> {
        Table::new()
    }

    fn add(t: &mut Table<()>, ppid: u32, new_group: bool) -> u32 {
        t.insert(ppid, new_group, String::from("p"), ()).unwrap()
    }

    fn state(t: &Table<()>, pid: u32) -> State {
        t.get(pid).unwrap().state
    }

    /// The order the processes get the CPU in when each gives it up at
    /// once.
    fn turns(t: &mut Table<()>, n: usize) -> Vec<u32> {
        (0..n).map(|_| t.schedule()).collect()
    }

    #[test]
    fn pids_count_from_1_and_are_not_used_again() {
        let mut t = table();
        assert_eq!(add(&mut t, 0, true), 1);
        assert_eq!(add(&mut t, 1, true), 2);
        t.schedule();
        t.schedule();
        t.end(2, WaitStatus::exited(0));
        assert!(t.reap(1, Want::Pid(2)).unwrap().is_some());
        assert_eq!(add(&mut t, 1, true), 3, "2 is not used again");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn at_most_64_processes_zombies_included() {
        let mut t = table();
        for _ in 0..MAX {
            add(&mut t, 0, true);
        }
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN)
        );
        t.end(64, WaitStatus::exited(0));
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN),
            "a zombie keeps its entry"
        );
        assert!(t.reap(0, Want::Pid(64)).unwrap().is_some());
        assert_eq!(add(&mut t, 1, false), 65);
    }

    #[test]
    fn a_child_joins_its_parent_s_group_or_starts_its_own() {
        let mut t = table();
        let shell = add(&mut t, 0, false);
        assert_eq!(t.get(shell).unwrap().pgid, shell, "no parent: its own");
        let cmd = add(&mut t, shell, true);
        assert_eq!(t.get(cmd).unwrap().pgid, cmd);
        let child = add(&mut t, cmd, false);
        assert_eq!(t.get(child).unwrap().pgid, cmd);
        assert_eq!(t.get(child).unwrap().ppid, cmd);
    }

    #[test]
    fn ready_processes_take_turns_in_order() {
        let mut t = table();
        for _ in 0..3 {
            add(&mut t, 0, true);
        }
        assert_eq!(t.current(), 0, "the idle task runs first");
        assert_eq!(turns(&mut t, 7), [1, 2, 3, 1, 2, 3, 1]);
        assert_eq!(state(&t, 1), State::Running);
        assert_eq!(state(&t, 2), State::Ready);
        // A new process joins the end of the queue.
        add(&mut t, 1, false);
        assert_eq!(turns(&mut t, 4), [2, 3, 4, 1]);
    }

    #[test]
    fn a_blocked_process_waits_until_it_is_woken() {
        let mut t = table();
        add(&mut t, 0, true);
        add(&mut t, 0, true);
        assert_eq!(t.schedule(), 1);
        t.block(Blocked::Console);
        assert_eq!(t.schedule(), 2);
        t.block(Blocked::Wait);
        assert_eq!(t.schedule(), 0, "nothing ready: the idle task");
        assert_eq!(t.schedule(), 0);
        assert!(!t.wake(0), "the idle task is not in the table");
        t.wake_all(Blocked::Console);
        assert_eq!(state(&t, 1), State::Ready);
        assert_eq!(state(&t, 2), State::Blocked(Blocked::Wait));
        assert!(!t.wake(1), "already ready");
        assert_eq!(turns(&mut t, 2), [1, 1], "queued once");
    }

    #[test]
    fn sleepers_wake_when_their_tick_comes() {
        let mut t = table();
        add(&mut t, 0, true);
        add(&mut t, 0, true);
        t.schedule();
        t.block(Blocked::Sleep(100));
        t.schedule();
        t.block(Blocked::Sleep(50));
        t.schedule();
        t.wake_sleepers(49);
        assert_eq!(t.schedule(), 0);
        t.wake_sleepers(100);
        assert_eq!(turns(&mut t, 3), [1, 2, 1]);
    }

    #[test]
    fn a_slice_is_10_ticks_and_ends_only_for_another_process() {
        let mut t = table();
        add(&mut t, 0, true);
        t.schedule();
        for _ in 0..25 {
            assert!(!t.tick(), "nobody else is ready");
        }
        assert_eq!(t.get(1).unwrap().ticks, 25);
        add(&mut t, 0, true);
        // 5 ticks of the slice are gone.
        for _ in 0..4 {
            assert!(!t.tick());
        }
        assert!(t.tick(), "used up");
        assert_eq!(t.schedule(), 2);
        for _ in 0..9 {
            assert!(!t.tick());
        }
        assert!(t.tick());
        assert_eq!(t.get(2).unwrap().ticks, 10);
        // A process that gives up the CPU early leaves the next a whole
        // slice.
        assert_eq!(t.schedule(), 1);
        t.tick();
        assert_eq!(t.schedule(), 2);
        for _ in 0..9 {
            assert!(!t.tick());
        }
        assert!(t.tick());
    }

    #[test]
    fn an_ended_child_is_a_zombie_until_its_parent_waits() {
        let mut t = table();
        let parent = add(&mut t, 0, true);
        let child = add(&mut t, parent, true);
        assert_eq!(t.reap(parent, Want::Any).unwrap().map(|p| p.pid), None);
        assert_eq!(
            t.reap(parent, Want::Pid(child)).unwrap().map(|p| p.pid),
            None
        );
        t.schedule();
        t.block(Blocked::Wait);
        t.schedule();
        t.end(child, WaitStatus::exited(3));
        assert_eq!(state(&t, parent), State::Ready, "woken");
        assert_eq!(state(&t, child), State::Zombie(WaitStatus::exited(3)));
        let reaped = t.reap(parent, Want::Pid(child)).unwrap().unwrap();
        assert_eq!(reaped.state, State::Zombie(WaitStatus::exited(3)));
        assert!(t.get(child).is_none());
        assert_eq!(t.reap(parent, Want::Any).err(), Some(Errno::ECHILD));
    }

    #[test]
    fn wait_is_only_for_one_s_own_children() {
        let mut t = table();
        let a = add(&mut t, 0, true);
        let b = add(&mut t, 0, true);
        let child = add(&mut t, a, true);
        t.end(child, WaitStatus::exited(0));
        assert_eq!(t.reap(b, Want::Pid(child)).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(b, Want::Any).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(a, Want::Pid(99)).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(a, Want::Pid(a)).err(), Some(Errno::ECHILD), "itself");
        assert!(t.reap(a, Want::Any).unwrap().is_some());
    }

    #[test]
    fn a_parent_is_woken_only_while_it_waits() {
        let mut t = table();
        let parent = add(&mut t, 0, true);
        let child = add(&mut t, parent, true);
        t.schedule();
        t.block(Blocked::Console);
        t.schedule();
        t.end(child, WaitStatus::exited(0));
        assert_eq!(state(&t, parent), State::Blocked(Blocked::Console));
    }

    #[test]
    fn orphans_pass_to_process_1_which_is_woken_to_collect_them() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let cmd = add(&mut t, init, true);
        let a = add(&mut t, cmd, false);
        let b = add(&mut t, cmd, false);
        t.end(a, WaitStatus::exited(1));
        t.schedule();
        t.block(Blocked::Wait);
        t.end(cmd, WaitStatus::exited(0));
        assert_eq!(t.get(a).unwrap().ppid, INIT);
        assert_eq!(t.get(b).unwrap().ppid, INIT);
        assert_eq!(state(&t, init), State::Ready);
        let mut got: Vec<u32> = core::iter::from_fn(|| t.reap(INIT, Want::Any).unwrap())
            .map(|p| p.pid)
            .collect();
        got.sort();
        assert_eq!(got, [cmd, a], "b still runs");
        // An orphan ending later wakes process 1 too.
        t.schedule();
        t.block(Blocked::Wait);
        t.end(b, WaitStatus::exited(0));
        assert_eq!(state(&t, init), State::Ready);
        assert!(t.reap(INIT, Want::Pid(b)).unwrap().is_some());
    }

    #[test]
    fn process_1_is_woken_for_a_grandchild_s_zombie() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let x = add(&mut t, init, true);
        let y = add(&mut t, x, false);
        let z = add(&mut t, y, false);
        t.end(z, WaitStatus::exited(0));
        assert_eq!(t.schedule(), init);
        t.block(Blocked::Wait);
        t.end(y, WaitStatus::exited(0));
        assert_eq!(state(&t, init), State::Ready, "z is its orphan now");
        assert_eq!(t.reap(INIT, Want::Any).unwrap().map(|p| p.pid), Some(z));
        assert_eq!(t.reap(x, Want::Any).unwrap().map(|p| p.pid), Some(y));
    }

    #[test]
    fn kill_marks_a_process_or_a_group_and_wakes_the_blocked() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let cmd = add(&mut t, init, true);
        let child = add(&mut t, cmd, false);
        let other = add(&mut t, init, true);
        assert_eq!(turns(&mut t, 2), [init, cmd]);
        t.block(Blocked::Wait);
        t.schedule();
        assert_eq!(t.kill(-i64::from(cmd), KILLED_CTRL_C), Ok(()));
        assert_eq!(t.get(cmd).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.get(child).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.get(other).unwrap().killed, None);
        assert_eq!(state(&t, cmd), State::Ready, "woken to end");
        // The first reason stays.
        assert_eq!(t.kill(i64::from(child), KILLED_KILL), Ok(()));
        assert_eq!(t.get(child).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.kill(i64::from(other), KILLED_KILL), Ok(()));
        assert_eq!(t.get(other).unwrap().killed, Some(KILLED_KILL));
    }

    #[test]
    fn kill_refuses_process_1_and_what_does_not_exist() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let child = add(&mut t, init, false);
        assert_eq!(t.kill(1, KILLED_KILL), Err(Errno::EPERM));
        assert_eq!(
            t.kill(-1, KILLED_KILL),
            Err(Errno::EPERM),
            "the group of process 1"
        );
        assert_eq!(t.get(child).unwrap().killed, None, "nothing marked");
        for target in [0, 99, -99, i64::MIN, i64::MAX] {
            assert_eq!(t.kill(target, KILLED_KILL), Err(Errno::ESRCH), "{target}");
        }
        let zombie = add(&mut t, init, true);
        t.end(zombie, WaitStatus::exited(0));
        assert_eq!(t.kill(i64::from(zombie), KILLED_KILL), Ok(()), "exists");
        assert_eq!(
            state(&t, zombie),
            State::Zombie(WaitStatus::exited(0)),
            "and keeps how it ended"
        );
        assert_eq!(t.get(zombie).unwrap().killed, None);
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib proc::table`

Expected: FAIL: compile errors such as `` cannot find type `Table` in this scope ``; `` cannot find type `State` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/proc/table.rs`**

Insert this at the top of `kernel/src/proc/table.rs`, above `#[cfg(test)]`:

````rust
//! The process table and the run queue (user-space gate §5.4, §6.1),
//! architecture-neutral: who exists, who is whose child and in which group,
//! who is ready, running, blocked or a zombie, and whose turn is next.
//! What a process owns besides that (its memory, kernel stack, fds) is the
//! payload `R`, so the rules are tested here with none.
//!
//! - Pids count from 1 and are not used again while the kernel runs; at
//!   most [`MAX`] processes exist, zombies included (`EAGAIN` beyond).
//! - Process 0 is the idle task: it has no entry, and runs when nothing is
//!   ready.
//! - A process that ends stays as a zombie until its parent `wait`s for it;
//!   its children pass to process 1.
//! - Round-robin: a process made ready joins the end of the queue, and one
//!   that used up its slice of [`SLICE`] ticks goes to the end when another
//!   is ready.
//! - `kill` and Ctrl-C only mark a process and wake it if it is blocked; it
//!   ends itself the next time it runs, before any more of its code runs.

use alloc::string::String;
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::Errno;

/// The most processes that exist at once (spec §5.4).
pub const MAX: usize = 64;
/// Process 1: the parent of orphans, and the one `kill` refuses.
pub const INIT: u32 = 1;
/// Timer ticks in a time slice (spec §6.1).
pub const SLICE: u32 = 10;

/// What a blocked process waits for (spec §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// A child to end.
    Wait,
    /// Console input.
    Console,
    /// The tick count to reach this value.
    Sleep(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Blocked(Blocked),
    Zombie(WaitStatus),
}

pub struct Process<R> {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    /// Its path, for the kernel log (and `ps`, milestone 3).
    pub name: String,
    pub state: State,
    /// Why it must end when it runs next: `KILLED_CTRL_C` or `KILLED_KILL`.
    pub killed: Option<u32>,
    /// Timer ticks it has run for.
    pub ticks: u64,
    pub res: R,
}

/// Which child `wait` waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Any,
    Pid(u32),
}

/// The ready processes in order, at most [`MAX`] of them.
struct Queue {
    pids: [u32; MAX],
    head: usize,
    len: usize,
}

impl Queue {
    const fn new() -> Queue {
        Queue {
            pids: [0; MAX],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, pid: u32) {
        // Every entry is a different process, so it always fits.
        assert!(self.len < MAX, "the run queue is full");
        self.pids[(self.head + self.len) % MAX] = pid;
        self.len += 1;
    }

    /// Takes `pid` out of the queue, keeping the others' order.
    fn remove(&mut self, pid: u32) {
        let mut kept = Queue::new();
        while let Some(p) = self.pop() {
            if p != pid {
                kept.push(p);
            }
        }
        *self = kept;
    }

    fn pop(&mut self) -> Option<u32> {
        if self.len == 0 {
            return None;
        }
        let pid = self.pids[self.head];
        self.head = (self.head + 1) % MAX;
        self.len -= 1;
        Some(pid)
    }
}

pub struct Table<R> {
    procs: Vec<Process<R>>,
    next_pid: u32,
    ready: Queue,
    /// The running process; 0 while the idle task runs.
    current: u32,
    /// Ticks left of the running process's slice.
    slice_left: u32,
}

impl<R> Default for Table<R> {
    fn default() -> Self {
        Table::new()
    }
}

impl<R> Table<R> {
    pub const fn new() -> Table<R> {
        Table {
            procs: Vec::new(),
            next_pid: 1,
            ready: Queue::new(),
            current: 0,
            slice_left: SLICE,
        }
    }

    pub fn len(&self) -> usize {
        self.procs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.procs.is_empty()
    }

    /// The running process's pid; 0 for the idle task.
    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn get(&self, pid: u32) -> Option<&Process<R>> {
        self.procs.iter().find(|p| p.pid == pid)
    }

    pub fn get_mut(&mut self, pid: u32) -> Option<&mut Process<R>> {
        self.procs.iter_mut().find(|p| p.pid == pid)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Process<R>> {
        self.procs.iter()
    }

    /// Adds a ready process, a child of `ppid`, in its parent's group or,
    /// with `new_group` (or no parent), in a new group numbered with its
    /// own pid; its pid. `EAGAIN` when [`MAX`] processes exist.
    pub fn insert(
        &mut self,
        ppid: u32,
        new_group: bool,
        name: String,
        res: R,
    ) -> Result<u32, Errno> {
        if self.procs.len() >= MAX {
            return Err(Errno::EAGAIN);
        }
        let pid = self.next_pid;
        self.next_pid += 1;
        let pgid = match self.get(ppid) {
            Some(parent) if !new_group => parent.pgid,
            _ => pid,
        };
        self.procs.push(Process {
            pid,
            ppid,
            pgid,
            name,
            state: State::Ready,
            killed: None,
            ticks: 0,
            res,
        });
        self.ready.push(pid);
        Ok(pid)
    }

    /// Gives up the CPU for the running process (which stays ready unless
    /// it has blocked or ended) and picks the next ready one, which is then
    /// running; 0 when none is and the idle task runs.
    pub fn schedule(&mut self) -> u32 {
        let me = self.current;
        if let Some(p) = self.get_mut(me)
            && p.state == State::Running
        {
            p.state = State::Ready;
            self.ready.push(me);
        }
        let next = self.ready.pop().unwrap_or(0);
        if let Some(p) = self.get_mut(next) {
            p.state = State::Running;
        }
        self.current = next;
        self.slice_left = SLICE;
        next
    }

    /// The running process blocks on `why`; the caller then calls
    /// `schedule`.
    pub fn block(&mut self, why: Blocked) {
        let me = self.current;
        if let Some(p) = self.get_mut(me) {
            p.state = State::Blocked(why);
        }
    }

    /// Makes `pid` ready if it is blocked; whether it was.
    pub fn wake(&mut self, pid: u32) -> bool {
        match self.get_mut(pid) {
            Some(p) if matches!(p.state, State::Blocked(_)) => {
                p.state = State::Ready;
                self.ready.push(pid);
                true
            }
            _ => false,
        }
    }

    /// Wakes every process blocked on `why`.
    pub fn wake_all(&mut self, why: Blocked) {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| p.state == State::Blocked(why))
            .map(|p| p.pid)
            .collect();
        for pid in pids {
            self.wake(pid);
        }
    }

    /// Wakes the sleepers whose time has come at tick `now`.
    pub fn wake_sleepers(&mut self, now: u64) {
        let mut due = [0u32; MAX];
        let mut n = 0;
        for p in &self.procs {
            if let State::Blocked(Blocked::Sleep(until)) = p.state
                && until <= now
            {
                due[n] = p.pid;
                n += 1;
            }
        }
        for &pid in &due[..n] {
            self.wake(pid);
        }
    }

    /// Whether anything but the running process is ready.
    pub fn others_ready(&self) -> bool {
        self.ready.len > 0
    }

    /// A timer tick while the running process ran: counts it, and whether
    /// its slice is used up while another process is ready (it should
    /// then give up the CPU). A slice nobody else wants starts again.
    pub fn tick(&mut self) -> bool {
        let me = self.current;
        if let Some(p) = self.get_mut(me) {
            p.ticks += 1;
        }
        self.slice_left = self.slice_left.saturating_sub(1);
        if self.slice_left > 0 {
            return false;
        }
        self.slice_left = SLICE;
        self.others_ready()
    }

    /// `pid` has ended with `status`: it is a zombie until its parent
    /// waits for it, its children pass to process 1, and a parent blocked
    /// in `wait` is woken.
    pub fn end(&mut self, pid: u32, status: WaitStatus) {
        let Some(p) = self.get_mut(pid) else {
            return;
        };
        let was_ready = p.state == State::Ready;
        p.state = State::Zombie(status);
        let ppid = p.ppid;
        if was_ready {
            self.ready.remove(pid);
        }
        let mut orphaned_zombie = false;
        for child in self.procs.iter_mut().filter(|c| c.ppid == pid) {
            child.ppid = INIT;
            orphaned_zombie |= matches!(child.state, State::Zombie(_));
        }
        for waiter in [Some(ppid), orphaned_zombie.then_some(INIT)]
            .into_iter()
            .flatten()
        {
            if self.get(waiter).map(|p| p.state) == Some(State::Blocked(Blocked::Wait)) {
                self.wake(waiter);
            }
        }
    }

    /// Takes a zombie child of `parent` out of the table: `Ok(None)` if
    /// there are such children but none has ended, `ECHILD` if there are
    /// none.
    pub fn reap(&mut self, parent: u32, want: Want) -> Result<Option<Process<R>>, Errno> {
        let mine =
            |p: &Process<R>| p.ppid == parent && (want == Want::Any || want == Want::Pid(p.pid));
        if !self.procs.iter().any(mine) {
            return Err(Errno::ECHILD);
        }
        match self
            .procs
            .iter()
            .position(|p| mine(p) && matches!(p.state, State::Zombie(_)))
        {
            Some(i) => Ok(Some(self.procs.remove(i))),
            None => Ok(None),
        }
    }

    /// Marks the process `target` (a pid, or a negated group number) to end
    /// for `reason`, waking whichever of them is blocked. `ESRCH` if none
    /// exists; `EPERM` (and nothing marked) if process 1 is among them. A
    /// zombie counts as existing, and a process marked earlier keeps its
    /// first reason.
    pub fn kill(&mut self, target: i64, reason: u32) -> Result<(), Errno> {
        let hit = |p: &Process<R>| match target {
            t if t > 0 => i64::from(p.pid) == t,
            t if t < 0 => t.checked_neg() == Some(i64::from(p.pgid)),
            _ => false,
        };
        if !self.procs.iter().any(hit) {
            return Err(Errno::ESRCH);
        }
        if self.procs.iter().any(|p| hit(p) && p.pid == INIT) {
            return Err(Errno::EPERM);
        }
        let mut woken = [0u32; MAX];
        let mut n = 0;
        for p in self.procs.iter_mut().filter(|p| hit(p)) {
            if matches!(p.state, State::Zombie(_)) {
                continue;
            }
            p.killed.get_or_insert(reason);
            if matches!(p.state, State::Blocked(_)) {
                woken[n] = p.pid;
                n += 1;
            }
        }
        for &pid in &woken[..n] {
            self.wake(pid);
        }
        Ok(())
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib proc::table`

Expected: PASS: 14 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the process table and the run queue"
````


### Task 4: Once every pid has been used, no process starts

Found by the prototype's review: `Table::insert` counted pids with `next_pid += 1` on a `u32`, and the kernel builds with overflow checks, so after 2^32 spawns (about 32 hours of a spawning loop at the 37,000 a second QEMU manages) the kernel panicked. Pids are never used again while the kernel runs (spec §5.4), so once pid 2^32 − 1 has been handed out, `insert` is `EAGAIN` for good (decision 10). The red run is the overflow's panic in the new test. Mutation check: without the check for used-up pids the table hands out pid 0, which the test sees.

**Files:**
- Modify: `kernel/src/proc/table.rs`

**Interfaces:**
- Consumes: Task 3.
- Produces: `Table::insert`'s `EAGAIN` once every pid has been used.

- [ ] **Step 1: Add the failing tests to `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
    #[test]
    fn at_most_64_processes_zombies_included() {
````

with:

````rust
    #[test]
    fn when_every_pid_has_been_used_no_process_starts() {
        let mut t = table();
        t.next_pid = u32::MAX - 1;
        assert_eq!(add(&mut t, 0, true), u32::MAX - 1);
        assert_eq!(add(&mut t, 0, true), u32::MAX);
        assert_eq!(
            t.insert(0, true, String::from("x"), ()),
            Err(Errno::EAGAIN),
            "no pid is used twice, and none overflows"
        );
        t.end(u32::MAX, WaitStatus::exited(0));
        assert!(t.reap(0, Want::Pid(u32::MAX)).unwrap().is_some());
        assert_eq!(t.insert(0, true, String::from("x"), ()), Err(Errno::EAGAIN));
    }

    #[test]
    fn at_most_64_processes_zombies_included() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib proc::table`

Expected: FAIL: 1 test fails: `proc::table::tests::when_every_pid_has_been_used_no_process_starts`.

- [ ] **Step 3: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! - Pids count from 1 and are not used again while the kernel runs; at
//!   most [`MAX`] processes exist, zombies included (`EAGAIN` beyond).
//! - Process 0 is the idle task: it has no entry, and runs when nothing is
````

with:

````rust
//! - Pids count from 1 and are not used again while the kernel runs; at
//!   most [`MAX`] processes exist, zombies included (`EAGAIN` beyond), and
//!   once pid 2^32 − 1 has been used, no process starts any more (`EAGAIN`:
//!   at the fastest rate QEMU manages, after more than a day).
//! - Process 0 is the idle task: it has no entry, and runs when nothing is
````

Replace:

````rust
    procs: Vec<Process<R>>,
    next_pid: u32,
````

with:

````rust
    procs: Vec<Process<R>>,
    /// The pid the next process gets; 0 once every pid has been used.
    next_pid: u32,
````

Replace:

````rust
    /// with `new_group` (or no parent), in a new group numbered with its
    /// own pid; its pid. `EAGAIN` when [`MAX`] processes exist.
    pub fn insert(
````

with:

````rust
    /// with `new_group` (or no parent), in a new group numbered with its
    /// own pid; its pid. `EAGAIN` when [`MAX`] processes exist, or every
    /// pid has been used.
    pub fn insert(
````

Replace:

````rust
    ) -> Result<u32, Errno> {
        if self.procs.len() >= MAX {
            return Err(Errno::EAGAIN);
        }
        let pid = self.next_pid;
        self.next_pid += 1;
        let pgid = match self.get(ppid) {
````

with:

````rust
    ) -> Result<u32, Errno> {
        if self.procs.len() >= MAX || self.next_pid == 0 {
            return Err(Errno::EAGAIN);
        }
        let pid = self.next_pid;
        self.next_pid = pid.checked_add(1).unwrap_or(0);
        let pgid = match self.get(ppid) {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib proc::table`

Expected: PASS: 15 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: once every pid has been used, no process starts, instead of the pid counter overflowing"
````


### Task 5: The fd table

Each process has 32 fd slots, each a shared reference (`Arc`) to an open file (spec §5.4), so a child `spawn` gives an fd uses the same file as its parent. Plan 3a's files are the console and the in-kernel shell's standard output and error (`File::ShellOutput`, whose writes go to the waiting shell: decision 8); plan 3b adds files of the VFS, milestone 3 pipe ends. `FdTable::shell` is the in-kernel shell's own table (the console, and its outputs as fds 1 and 2); `for_child` builds a child's table from `spawn`'s pairs (§7.3): each child fd gets the parent's file, every other fd is closed, `EBADF` for a parent fd that is not open or a child fd not below 32, `EINVAL` for more than 8 pairs or a child fd named twice. Mutation checks: a child fd named twice accepted, 9 pairs accepted and a child given a new file instead of its parent's all fail a test.

**Files:**
- Create: `kernel/src/fd.rs`
- Modify: `kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 (`FdMap`, `SPAWN_FDS`).
- Produces: `fd::{FDS, File::{Console, ShellOutput(u32)}, FdTable::{new, shell, get(fd: u64) -> Result<&Arc<File>, Errno>, for_child(&[FdMap]) -> Result<FdTable, Errno>, open}}` (`FdTable: Default`).

- [ ] **Step 1: Write the failing tests for `kernel/src/fd.rs`**

Create `kernel/src/fd.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn map(child: u32, parent: u32) -> FdMap {
        FdMap { child, parent }
    }

    #[test]
    fn the_shell_has_the_console_and_its_outputs() {
        let t = FdTable::shell();
        assert_eq!(**t.get(0).unwrap(), File::Console);
        assert_eq!(**t.get(1).unwrap(), File::ShellOutput(1));
        assert_eq!(**t.get(2).unwrap(), File::ShellOutput(2));
        assert_eq!(t.open(), 3);
        for fd in [3, 31, 32, 1 << 32, u64::MAX] {
            assert_eq!(t.get(fd).err(), Some(Errno::EBADF), "{fd}");
        }
    }

    #[test]
    fn a_child_shares_the_files_it_is_given_and_nothing_else() {
        let parent = FdTable::shell();
        let child = parent
            .for_child(&[map(0, 0), map(1, 2), map(31, 1)])
            .unwrap();
        assert!(Arc::ptr_eq(child.get(0).unwrap(), parent.get(0).unwrap()));
        assert!(Arc::ptr_eq(child.get(1).unwrap(), parent.get(2).unwrap()));
        assert_eq!(**child.get(31).unwrap(), File::ShellOutput(1));
        assert_eq!(child.get(2).err(), Some(Errno::EBADF), "not given");
        assert_eq!(child.open(), 3);
        // The files stay while either has them.
        assert_eq!(Arc::strong_count(parent.get(2).unwrap()), 2);
        drop(child);
        assert_eq!(Arc::strong_count(parent.get(2).unwrap()), 1);
        assert_eq!(parent.for_child(&[]).unwrap().open(), 0);
    }

    #[test]
    fn a_bad_mapping_is_refused() {
        let parent = FdTable::shell();
        assert_eq!(parent.for_child(&[map(0, 3)]).err(), Some(Errno::EBADF));
        assert_eq!(parent.for_child(&[map(32, 0)]).err(), Some(Errno::EBADF));
        assert_eq!(
            parent.for_child(&[map(u32::MAX, 0)]).err(),
            Some(Errno::EBADF)
        );
        assert_eq!(
            parent.for_child(&[map(1, 1), map(1, 2)]).err(),
            Some(Errno::EINVAL),
            "fd 1 twice"
        );
        let nine: Vec<FdMap> = (0..9).map(|i| map(i, 0)).collect();
        assert_eq!(parent.for_child(&nine).err(), Some(Errno::EINVAL));
        assert_eq!(parent.for_child(&nine[..8]).unwrap().open(), 8);
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod exec;
pub mod input;
````

with:

````rust
pub mod exec;
pub mod fd;
pub mod input;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib fd::`

Expected: FAIL: compile errors such as `` cannot find type `FdMap` in this scope ``; `` cannot find struct, variant or union type `FdMap` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/fd.rs`**

Insert this at the top of `kernel/src/fd.rs`, above `#[cfg(test)]`:

````rust
//! A process's file descriptors (user-space gate §5.4): 32 slots, each a
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent. Plan 3a's files are the console and
//! the in-kernel shell's output; plan 3b adds files of the VFS, milestone 3
//! pipe ends.

use alloc::sync::Arc;
use relay_abi::FdMap;
use relay_abi::spawn::SPAWN_FDS;
use vfs::Errno;

/// Slots per process (spec §5.4).
pub const FDS: usize = 32;

/// An open file.
#[derive(Debug, PartialEq, Eq)]
pub enum File {
    /// The screen and keyboard.
    Console,
    /// Standard output (1) or standard error (2) of the in-kernel shell:
    /// what is written goes to the shell, which sends it where the command
    /// line says (a redirection, the screen, a script's transcript). Plan 4
    /// replaces the in-kernel shell and this with it.
    ShellOutput(u32),
}

pub struct FdTable {
    slots: [Option<Arc<File>>; FDS],
}

impl Default for FdTable {
    fn default() -> Self {
        FdTable::new()
    }
}

impl FdTable {
    /// No fd open.
    pub fn new() -> FdTable {
        FdTable {
            slots: [const { None }; FDS],
        }
    }

    /// The in-kernel shell's fds: the console to read, and its standard
    /// output and error.
    pub fn shell() -> FdTable {
        let mut t = FdTable::new();
        t.slots[0] = Some(Arc::new(File::Console));
        t.slots[1] = Some(Arc::new(File::ShellOutput(1)));
        t.slots[2] = Some(Arc::new(File::ShellOutput(2)));
        t
    }

    /// The file open as `fd`; `EBADF` if none is.
    pub fn get(&self, fd: u64) -> Result<&Arc<File>, Errno> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots.get(i))
            .and_then(Option::as_ref)
            .ok_or(Errno::EBADF)
    }

    /// A child's fds from `maps` (spec §7.3): each child fd gets the file
    /// this table has as the parent fd, every other one is closed. `EBADF`
    /// if a parent fd is not open or a child fd is not below 32, `EINVAL`
    /// for more than 8 pairs or a child fd named twice.
    pub fn for_child(&self, maps: &[FdMap]) -> Result<FdTable, Errno> {
        if maps.len() > SPAWN_FDS {
            return Err(Errno::EINVAL);
        }
        let mut child = FdTable::new();
        for m in maps {
            let file = self.get(u64::from(m.parent))?;
            let slot = child.slots.get_mut(m.child as usize).ok_or(Errno::EBADF)?;
            if slot.is_some() {
                return Err(Errno::EINVAL);
            }
            *slot = Some(Arc::clone(file));
        }
        Ok(child)
    }

    /// How many fds are open.
    pub fn open(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib fd::`

Expected: PASS: 3 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the fd table, shared files and a child's fds from spawn's pairs"
````


### Task 6: A current directory per process

There is one mount table, shared by every process, with each process's current directory switched in (spec §5.4, §7.3). `vfs::Cwd` is a current directory apart from the table (the names walked to it, and whether it was removed); `MountTable::swap_cwd` puts one in and gives back the one it replaces, and `root_cwd` is `/`. A removal still marks the directory as gone only for the current directory in the table at the time (decision 9). Mutation check: a swap that does not carry the removed state fails the test.

**Files:**
- Modify: `crates/vfs/src/lib.rs`
- Modify: `crates/vfs/src/mount.rs`

**Interfaces:**
- Consumes: milestone 1's `vfs::MountTable`.
- Produces: `vfs::Cwd` (`Clone`, `PartialEq`), `MountTable::{root_cwd(&self) -> Cwd, swap_cwd(&mut self, Cwd) -> Cwd}`.

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, replace:

````rust
    #[test]
    fn dot_dot_at_the_root_stays_at_the_root() {
````

with:

````rust
    #[test]
    fn a_current_directory_can_be_put_aside_and_back() {
        let mut t = table();
        t.chdir(b"/root").unwrap();
        let shell = t.swap_cwd(t.root_cwd());
        assert_eq!(t.cwd(), b"/");
        assert_eq!(read(&mut t, b"etc/motd").unwrap(), b"welcome\n");
        t.mkdir(b"tmp/x").unwrap();
        t.chdir(b"tmp/x").unwrap();
        t.rmdir(b"/tmp/x").unwrap();
        assert_eq!(read(&mut t, b"../../etc/motd"), Err(Errno::ENOENT), "gone");
        let gone = t.swap_cwd(shell);
        assert_eq!(t.cwd(), b"/root");
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
        let shell = t.swap_cwd(gone);
        assert_eq!(t.cwd(), b"/tmp/x", "the prompt's path");
        assert_eq!(
            read(&mut t, b"../../etc/motd"),
            Err(Errno::ENOENT),
            "still gone"
        );
        t.swap_cwd(shell);
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
    }

    #[test]
    fn dot_dot_at_the_root_stays_at_the_root() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` no method named `swap_cwd` found for struct `mount::MountTable` in the current scope ``; `` no method named `root_cwd` found for struct `mount::MountTable` in the current scope ``.

- [ ] **Step 3: Change `crates/vfs/src/lib.rs`**

In `crates/vfs/src/lib.rs`, replace:

````rust
pub use memfs::{MAX_FILE_SIZE, MemFs};
pub use mount::{MountTable, Node, Vfs};
````

with:

````rust
pub use memfs::{MAX_FILE_SIZE, MemFs};
pub use mount::{Cwd, MountTable, Node, Vfs};
````

- [ ] **Step 4: Change `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// The mount table plus the current directory.
````

with:

````rust

/// A current directory apart from the table: each process has its own
/// (user-space gate §5.4), and the kernel puts it in with
/// `MountTable::swap_cwd` while it works for that process. A removal marks
/// only the current directory that is in the table at the time as gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cwd {
    trail: Trail,
    gone: bool,
}

/// The mount table plus the current directory.
````

Replace:

````rust
            cwd_gone: false,
        }
    }

````

with:

````rust
            cwd_gone: false,
        }
    }

    /// The root, as a current directory to put in with `swap_cwd`.
    pub fn root_cwd(&self) -> Cwd {
        Cwd {
            trail: self.cwd[..1].to_vec(),
            gone: false,
        }
    }

    /// Makes `cwd` the current directory and returns the one it replaces.
    pub fn swap_cwd(&mut self, cwd: Cwd) -> Cwd {
        let old = Cwd {
            trail: core::mem::replace(&mut self.cwd, cwd.trail),
            gone: self.cwd_gone,
        };
        self.cwd_gone = cwd.gone;
        old
    }

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 47 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "vfs: a current directory can be put aside and back, one per process"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 26 scenario(s) passed`.

````bash
git push -u origin m2p3a/table
gh pr create --base main --head m2p3a/table --title "Milestone 2, plan 3a: What processes are made of" --body-file - <<'EOF'
## What

Milestone 2, plan 3a, tasks 1–6: `relay-abi` gains `SpawnArgs`, `FdMap`, `Time`, `MemInfo` and the kill reasons Ctrl-C and `kill`, `vfs::Errno` gains `ESRCH` and `EPERM`, and the shell gives the kill reasons bash's statuses; `UserSlice` copies out whole or not at all and `UserStr` copies a program's byte string in; the process table and the run queue (pids, groups, zombies, orphans passing to process 1, round-robin with 10-tick slices, blocking and waking, group kills), which refuses a process once every pid has been used instead of overflowing; the fd table; `vfs::Cwd`, a current directory per process.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3a/table --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-table
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Processes share the CPU (Tasks 7–13)

The kernel's own `gs` and flags on every way in from ring 3, the mount table and the console's input as the kernel's, the process table in the kernel with the context switch, the idle task and the in-kernel shell as process 1, the `time` and `sleep` calls (with a review fix: a sleep never shorter than asked), ticks that poll the keyboard and end a program's slice, and milestone 1's interrupt findings.

Branch `m2p3a/sched`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-sched`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3a/sched /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-sched origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-sched
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3a/table` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3a/sched /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-sched m2p3a/table`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3a/table>` and re-run `cargo xtask ci` before pushing.

### Task 7: Every way in from ring 3 gives the kernel its own `gs` and flags

Once a tick from ring 3 can switch to a process that is blocked in the middle of a system call, the kernel's contexts must agree on `gs`: plan 2's interrupt and exception stubs never `swapgs`, and `run` set both `gs` bases before each program. Now the kernel always has its own `gs` (the per-CPU block in `GsBase`, the program's, always 0, in `KernelGsBase`), set at `arch::user::init`: `syscall_entry` swaps as before, `enter` swaps before its `iretq`, and `irq_common` and `exception_common` test the interrupted CPL and swap when it was ring 3, and back before `iretq` (decision 4). The same stubs load the kernel's flags there (`push 2; popfq`): an interrupt or trap gate clears only IF, TF, NT and RF, and AC, which a program sets with `popfq`, would switch SMAP off in the handler (spec §16 item 2). That works on every CPU, where `clac` exists only with SMAP. Debug assertions at the top of both dispatchers check the `gs` and the flags, and the kernel's `relay` profile keeps debug assertions, so a forgotten `swapgs` or flag is a panic in a scenario. The unit tests read the stubs' first and last instructions from the kernel's own code, as plan 2's stub test does. Mutation checks: `exception_common` without its `swapgs` or without `push 2; popfq`, and `enter` without its `swapgs`, fail the `userfault` and `programs` scenarios (the assertions, and `syscall_entry` swapping to the program's `gs`); `irq_common`'s `swapgs` is killed by the code-byte test here and by the assertion once `t-spin` takes ticks (Task 12).

**Files:**
- Modify: `kernel/src/arch/idt.rs`
- Modify: `kernel/src/arch/irq.rs`
- Modify: `kernel/src/arch/user.rs`

**Interfaces:**
- Consumes: plan 2's `arch::{irq, idt, user}`.
- Produces: `arch::user::{gs_is_kernel() -> bool, flags_are_kernel() -> bool}`; the stubs' ring-3 prologue and `irq_common`'s epilogue.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, replace:

````rust
        assert_eq!(exception_name(99), "unknown");
    }
}
````

with:

````rust
        assert_eq!(exception_name(99), "unknown");
    }

    #[test]
    fn an_exception_in_ring_3_gets_the_kernel_s_gs_and_flags() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(exception_common as *const [u8; 17]) };
        // `test qword ptr [rsp + 24], 3; jz +6; swapgs; push 2; popfq`:
        // from ring 3, to the kernel's `gs` and flags.
        let swap = [
            0x48, 0xF7, 0x44, 0x24, 0x18, 0x03, 0x00, 0x00, 0x00, 0x74, 0x06, 0x0F, 0x01, 0xF8,
            0x6A, 0x02, 0x9D,
        ];
        assert_eq!(code, swap, "first thing");
    }
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/arch/irq.rs`**

In `kernel/src/arch/irq.rs`, replace:

````rust
        assert!(!note_unexpected(&counts, 101), "counted per vector");
    }
}
````

with:

````rust
        assert!(!note_unexpected(&counts, 101), "counted per vector");
    }

    /// `test qword ptr [rsp + 24], 3; jz +6; swapgs; push 2; popfq`: from
    /// ring 3, to the kernel's `gs` and flags.
    const FROM_RING_3: [u8; 17] = [
        0x48, 0xF7, 0x44, 0x24, 0x18, 0x03, 0x00, 0x00, 0x00, 0x74, 0x06, 0x0F, 0x01, 0xF8, 0x6A,
        0x02, 0x9D,
    ];

    #[test]
    fn an_interrupt_of_ring_3_gets_the_kernel_s_gs_and_flags() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(irq_common as *const [u8; 128]) };
        assert_eq!(code[..17], FROM_RING_3, "first thing");
        // Before `iretq`: `test qword ptr [rsp + 8], 3; jz +3; swapgs`.
        let back = [
            0x48, 0xF7, 0x44, 0x24, 0x08, 0x03, 0x00, 0x00, 0x00, 0x74, 0x03, 0x0F, 0x01, 0xF8,
            0x48, 0xCF,
        ];
        assert!(code.windows(back.len()).any(|w| w == back), "last thing");
    }
}
````

- [ ] **Step 3: Add the failing tests to `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, replace:

````rust
        assert_eq!(KERNEL_RFLAGS, 2, "leave: nothing of the program's");
    }
}
````

with:

````rust
        assert_eq!(KERNEL_RFLAGS, 2, "leave: nothing of the program's");
    }

    #[test]
    fn a_program_starts_with_its_own_gs() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(enter as *const [u8; 128]) };
        // `swapgs; iretq`.
        let last = [0x0F, 0x01, 0xF8, 0x48, 0xCF];
        assert!(code.windows(5).any(|w| w == last));
    }
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::`

Expected: FAIL: 3 tests fail, among them `arch::irq::tests::an_interrupt_of_ring_3_gets_the_kernel_s_gs_and_flags`, `arch::idt::tests::an_exception_in_ring_3_gets_the_kernel_s_gs_and_flags`.

- [ ] **Step 5: Change `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// Saves the general registers (building an `ExceptionFrame` on the stack)
/// and calls the Rust dispatcher. Never returns.
#[unsafe(naked)]
unsafe extern "C" fn exception_common() {
    naked_asm!(
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
````

with:

````rust
/// Saves the general registers (building an `ExceptionFrame` on the stack)
/// and calls the Rust dispatcher. Never returns. An exception in ring 3
/// swaps to the kernel's `gs` and loads the kernel's flags first (see
/// `user`).
#[unsafe(naked)]
unsafe extern "C" fn exception_common() {
    naked_asm!(
        // The interrupted CS, above the vector, the error code and RIP.
        "test qword ptr [rsp + 24], 3",
        "jz 2f",
        "swapgs",
        // The kernel's flags (bit 1 only): nothing the program set, AC
        // above all (it would switch SMAP off), reaches the handler.
        "push 2",
        "popfq",
        "2:",
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
````

Replace:

````rust
    if frame.cs & 3 == 3 {
        let cr2 = x86_64::registers::control::Cr2::read_raw();
````

with:

````rust
    if frame.cs & 3 == 3 {
        debug_assert!(
            super::user::gs_is_kernel(),
            "an exception with the program's gs"
        );
        debug_assert!(
            super::user::flags_are_kernel(),
            "an exception with the program's flags"
        );
        let cr2 = x86_64::registers::control::Cr2::read_raw();
````

- [ ] **Step 6: Change `kernel/src/arch/irq.rs`**

In `kernel/src/arch/irq.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
extern "C" fn dispatch(frame: &ExceptionFrame) {
    match classify(frame.vector as u8) {
````

with:

````rust
extern "C" fn dispatch(frame: &ExceptionFrame) {
    debug_assert!(
        super::user::gs_is_kernel(),
        "an interrupt with the program's gs"
    );
    debug_assert!(
        super::user::flags_are_kernel(),
        "an interrupt with the program's flags"
    );
    match classify(frame.vector as u8) {
````

Replace:

````rust
/// error code and the vector), calls `dispatch` on a 16-byte aligned stack,
/// restores everything and returns from the interrupt.
#[unsafe(naked)]
unsafe extern "C" fn irq_common() {
    naked_asm!(
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
````

with:

````rust
/// error code and the vector), calls `dispatch` on a 16-byte aligned stack,
/// restores everything and returns from the interrupt. An interrupt of
/// ring 3 swaps to the kernel's `gs` and loads the kernel's flags first,
/// and swaps back last (see `user`); `iretq` gives the program its flags
/// back.
#[unsafe(naked)]
unsafe extern "C" fn irq_common() {
    naked_asm!(
        // The interrupted CS, above the vector, the error code and RIP.
        "test qword ptr [rsp + 24], 3",
        "jz 2f",
        "swapgs",
        // The kernel's flags (bit 1 only): nothing the program set, AC
        // above all (it would switch SMAP off), reaches the handler.
        "push 2",
        "popfq",
        "2:",
        "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi", "push rbp",
````

Replace:

````rust
        "add rsp, 16",
        "iretq",
````

with:

````rust
        "add rsp, 16",
        "test qword ptr [rsp + 8], 3",
        "jz 3f",
        "swapgs",
        "3:",
        "iretq",
````

- [ ] **Step 7: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//!
//! The kernel uses `gs` only in `syscall_entry`. Because `exit` leaves
//! from inside a call, before the stub's second `swapgs`, `run` sets both
//! `gs` bases afresh before every program.

````

with:

````rust
//!
//! **`gs` and the flags.** The kernel always runs with its own `gs` base
//! (the per-CPU block) and the program's (always 0: nothing lets a program
//! set one) in `KernelGsBase`: every way into the kernel from ring 3
//! (`syscall`, and the interrupt and exception stubs when they interrupted
//! ring 3) starts with `swapgs`, and every way back ends with one. So every
//! kernel context has the same `gs`, whichever way it came in, and
//! switching from one to another never mixes them. The same ways in leave
//! the program's flags behind: `SFMASK` for `syscall`, and the stubs load
//! the kernel's own (`push 2; popfq`), since an interrupt or trap gate
//! clears only IF, TF, NT and RF, and AC would switch SMAP off. That works
//! on every CPU, where `clac` exists only with SMAP.

````

Replace:

````rust

/// Turns on `syscall` and points it at `syscall_entry`.
pub fn init() {
````

with:

````rust

/// Whether `gs` is the kernel's (the per-CPU block), as it always must be
/// while the kernel runs.
pub fn gs_is_kernel() -> bool {
    GsBase::read().as_u64() == &raw const PER_CPU as u64
}

/// Whether none of the flags a program may set and the kernel must never
/// run with (TF, DF, AC, NT) is set.
pub fn flags_are_kernel() -> bool {
    !x86_64::registers::rflags::read().intersects(PROGRAM_FLAGS)
}

/// Turns on `syscall`, points it at `syscall_entry`, and gives the kernel
/// its `gs` (the program's is 0).
pub fn init() {
````

Replace:

````rust
    SFMask::write(SFMASK);
    // `sysenter` is legal in 64-bit mode on Intel CPUs: with a code segment
````

with:

````rust
    SFMask::write(SFMASK);
    GsBase::write(VirtAddr::new(&raw const PER_CPU as u64));
    KernelGsBase::write(VirtAddr::new(0));
    // `sysenter` is legal in 64-bit mode on Intel CPUs: with a code segment
````

Replace:

````rust
        PER_CPU.kernel_rsp = kernel_stack;
        GsBase::write(VirtAddr::new(0));
        KernelGsBase::write(VirtAddr::new(&raw const PER_CPU as u64));
        Cr3::write(
````

with:

````rust
        PER_CPU.kernel_rsp = kernel_stack;
        Cr3::write(
````

Replace:

````rust
/// Saves the callee-saved registers and the stack pointer in `*waiter`,
/// then goes to ring 3 at `entry`: its stack, `rdi`, `rsi`, `rdx`,
/// interrupts on, every other register zero. Returns when `leave(waiter)`
/// runs.
#[unsafe(naked)]
````

with:

````rust
/// Saves the callee-saved registers and the stack pointer in `*waiter`,
/// then goes to ring 3 at `entry` with the program's `gs`: its stack,
/// `rdi`, `rsi`, `rdx`, interrupts on, every other register zero. Returns
/// when `leave(waiter)` runs.
#[unsafe(naked)]
````

Replace:

````rust
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
````

with:

````rust
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "swapgs",
        "iretq",
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::`

Expected: PASS: 26 tests.

- [ ] **Step 9: Run the `userfault`, `programs` scenarios**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel
git commit -m "kernel: every way in from ring 3 gives the kernel its own gs and flags"
````


### Task 8: The mount table and the console's input are the kernel's

Until now the in-kernel shell owned the mount table and the console's input queue; programs' calls (Task 15) and ticks that interrupt a program (Task 12) need them too. `mounts.rs` holds the one mount table behind a lock that is held for one operation at a time (`mounts::with(cwd, f)` swaps a current directory in and out, Task 6); `KernelVfs`, the shell's `Vfs`, forwards every operation to it with the shell's current directory. `tty.rs` holds the input queue and its poll (the USB keyboards and COM1, as `KernelConsole::poll` did), and `KernelConsole` uses it. The filesystems are not `Send`; the table's wrapper says why one CPU and the lock make it safe. Nothing changes that a scenario sees: this is a move, tested by the scenarios that use files and the keyboard.

**Files:**
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/mounts.rs`
- Modify: `kernel/src/session.rs`
- Create: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: Task 6 (`Cwd`, `swap_cwd`, `root_cwd`), milestone 1's `input::InputQueue`, `usb::poll`, `serial::read_byte`.
- Produces: `mounts::{init(MountTable) -> Cwd, with(&mut Cwd, impl FnOnce(&mut MountTable) -> R) -> R, is_locked() -> bool, KernelVfs}`; `tty::{poll(), pop() -> Option<u8>, take_interrupt() -> bool, is_locked() -> bool}`; `session::run_shell(Cwd, bool)`.

- [ ] **Step 1: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub mod mm;
pub mod panic_screen;
````

with:

````rust
pub mod mm;
pub mod mounts;
pub mod panic_screen;
````

Replace:

````rust
pub mod timer;
pub mod usb;
````

with:

````rust
pub mod timer;
pub mod tty;
pub mod usb;
````

Replace:

````rust
    system::mount(info, &mut vfs);
    session::run_shell(vfs, cmdline.test_mode)
}
````

with:

````rust
    system::mount(info, &mut vfs);
    let root = mounts::init(vfs);
    session::run_shell(root, cmdline.test_mode)
}
````

- [ ] **Step 2: Create `kernel/src/mounts.rs`**

Create `kernel/src/mounts.rs`:

````rust
//! The mount table (user-space gate §7.3): one for the whole kernel,
//! shared by the in-kernel shell and every process's calls, with each
//! caller's current directory put in while the kernel works for it
//! (§5.4). The lock is held only for one operation, never while anything
//! waits, so a process that blocks never holds it.

use alloc::vec::Vec;
use spin::Mutex;
use vfs::{Cwd, DirEntry, Errno, MountTable, Node, Stat, StatFs, Vfs};

/// The table behind its lock.
struct Mounts(Option<MountTable>);

// SAFETY: one CPU, and the filesystems are reached only through `MOUNTS`'s
// lock; nothing of theirs is used from an interrupt.
unsafe impl Send for Mounts {}

static MOUNTS: Mutex<Mounts> = Mutex::new(Mounts(None));

/// Makes `table` the kernel's mount table; the root, as a current
/// directory.
pub fn init(table: MountTable) -> Cwd {
    let root = table.root_cwd();
    MOUNTS.lock().0 = Some(table);
    root
}

/// Runs `f` on the mount table with `cwd` as its current directory, and
/// keeps what `f` makes of it (a `chdir`) in `cwd`.
pub fn with<R>(cwd: &mut Cwd, f: impl FnOnce(&mut MountTable) -> R) -> R {
    let mut guard = MOUNTS.lock();
    let table = guard.0.as_mut().expect("mounts::init has not run");
    let theirs = table.swap_cwd(cwd.clone());
    let r = f(table);
    *cwd = table.swap_cwd(theirs);
    r
}

/// Whether the mount table is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    MOUNTS.is_locked()
}

/// The mount table as the in-kernel shell's `Vfs`, with the shell's own
/// current directory.
pub struct KernelVfs {
    cwd: Cwd,
}

impl KernelVfs {
    pub fn new(cwd: Cwd) -> KernelVfs {
        KernelVfs { cwd }
    }

    /// Its current directory, for a program it starts.
    pub fn current(&self) -> &Cwd {
        &self.cwd
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut MountTable) -> R) -> R {
        with(&mut self.cwd, f)
    }
}

impl Vfs for KernelVfs {
    fn cwd(&self) -> Vec<u8> {
        with(&mut self.cwd.clone(), |t| t.cwd())
    }
    fn chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.chdir(path))
    }
    fn lookup(&mut self, path: &[u8]) -> Result<Node, Errno> {
        self.with(|t| t.lookup(path))
    }
    fn stat(&mut self, node: Node) -> Result<Stat, Errno> {
        self.with(|t| t.stat(node))
    }
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        self.with(|t| t.read_dir(node))
    }
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
        self.with(|t| t.read_link(node))
    }
    fn read_at(&mut self, node: Node, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.with(|t| t.read_at(node, offset, buf))
    }
    fn write_at(&mut self, node: Node, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        self.with(|t| t.write_at(node, offset, buf))
    }
    fn truncate(&mut self, node: Node, size: u64) -> Result<(), Errno> {
        self.with(|t| t.truncate(node, size))
    }
    fn touch(&mut self, node: Node) -> Result<(), Errno> {
        self.with(|t| t.touch(node))
    }
    fn create(&mut self, path: &[u8]) -> Result<Node, Errno> {
        self.with(|t| t.create(path))
    }
    fn mkdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.mkdir(path))
    }
    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.unlink(path))
    }
    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.rmdir(path))
    }
    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.with(|t| t.rename(from, to))
    }
    fn statfs(&mut self, path: &[u8]) -> Result<StatFs, Errno> {
        self.with(|t| t.statfs(path))
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.with(|t| t.sync())
    }
    fn shutdown(&mut self) -> Result<(), Errno> {
        self.with(|t| t.shutdown())
    }
}
````

- [ ] **Step 3: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console` (input from the USB keyboards and COM1), the clock,
//! memory figures, kernel log and programs as `shell::System`, and
//! `vfs::Env` for filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, power, proc, rtc, serial, usb};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, Errno, MountTable, Vfs};

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

/// The screen and serial for output; the USB keyboards and COM1 for input.
pub struct KernelConsole {
    input: InputQueue,
}

impl KernelConsole {
    pub fn new() -> KernelConsole {
        KernelConsole {
            input: InputQueue::new(),
        }
    }

    /// Moves whatever the input devices have into the queue. Never waits.
    fn poll(&mut self) {
        usb::poll(&mut self.input);
        for _ in 0..SERIAL_BURST {
            match serial::read_byte() {
                Some(b) => self.input.push_serial(b),
                None => break,
            }
        }
    }
}

impl Default for KernelConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for KernelConsole {
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            self.poll();
            if let Some(b) = self.input.pop() {
                return Some(b);
````

with:

````rust
//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console` (input from the USB keyboards and COM1, `tty`), the
//! clock, memory figures, kernel log and programs as `shell::System`, the
//! kernel's mount table as its `Vfs` (`mounts::KernelVfs`), and `vfs::Env`
//! for filesystems.

use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::mounts::KernelVfs;
use crate::{arch, console, klog, klogln, power, proc, rtc, tty, usb};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Cwd, Env, Errno, Vfs};

/// The screen and serial for output; the USB keyboards and COM1 for input.
#[derive(Default)]
pub struct KernelConsole;

impl Console for KernelConsole {
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            tty::poll();
            if let Some(b) = tty::pop() {
                return Some(b);
````

Replace:

````rust
    fn interrupted(&mut self) -> bool {
        self.poll();
        self.input.take_interrupt()
    }
````

with:

````rust
    fn interrupted(&mut self) -> bool {
        tty::poll();
        tty::take_interrupt()
    }
````

Replace:

````rust

/// Runs the shell over `vfs`, the root at `/` and the programs at `/bin`
/// (spec §4.4 step 11). Never returns.
pub fn run_shell(mut vfs: MountTable, test_mode: bool) -> ! {
    let mut console = KernelConsole::new();
    let mut system = KernelSystem { test_mode };
````

with:

````rust

/// Runs the shell over the kernel's mount table (the root at `/`, the
/// programs at `/bin`), starting in `cwd` (spec §4.4 step 11). Never
/// returns.
pub fn run_shell(cwd: Cwd, test_mode: bool) -> ! {
    let mut vfs = KernelVfs::new(cwd);
    let mut console = KernelConsole;
    let mut system = KernelSystem { test_mode };
````

- [ ] **Step 4: Create `kernel/src/tty.rs`**

Create `kernel/src/tty.rs`:

````rust
//! The console's input (user-space gate §6.3): one queue for every source,
//! the USB keyboards and COM1, filled by `poll`. `poll` never waits, so it
//! can run wherever the kernel holds nothing: in the console's idle loop
//! and whenever a command asks whether Ctrl-C was pressed.

use crate::input::InputQueue;
use crate::{serial, usb};
use spin::Mutex;

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Moves whatever the input devices have into the queue. Never waits.
pub fn poll() {
    let mut input = INPUT.lock();
    usb::poll(&mut input);
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b),
            None => break,
        }
    }
}

/// The oldest byte typed.
pub fn pop() -> Option<u8> {
    INPUT.lock().pop()
}

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
/// dropped (`InputQueue::take_interrupt`).
pub fn take_interrupt() -> bool {
    INPUT.lock().take_interrupt()
}

/// Whether the input queue is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    INPUT.is_locked()
}
````

- [ ] **Step 5: Run the `shell`, `fileops`, `programs`, `keyboard` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the mount table and the console's input queue are the kernel's, not the shell's"
````


### Task 9: Processes, the context switch, the idle task, and the in-kernel shell as process 1

Plan 2 ran one child to completion inside `wait` (`arch::user::run`, `enter`, `leave`). Now every process is an entry of the process table (Task 3) with a payload of its kernel stack, its address space, its entry point until it first runs, its fds (Task 5) and its current directory, and the CPU goes from one to another with a context switch (decisions 2 and 3). `arch/context.rs` has `switch` (push the callee-saved registers and the flags, save the stack pointer, load another, pop, `ret`), `first_frame` (a new stack's top 64 bytes, whose `ret` goes to `start`, which enables interrupts and calls the stack's function with its argument) and `switch_to`, which points the CPU at the next process's page tables, TSS `rsp0` and `syscall` stack first; the saved stack pointers live in `SAVED`, by kernel-stack slot, since table entries move. Debug assertions at every switch: interrupts off, none of TF, DF, AC, NT set, TSS `rsp0` equal to the `syscall` stack (`kernel_stacks_agree`), and no kernel lock held. `proc.rs` is rewritten on the table: `reschedule`, `block`, the idle task (the boot context, which polls the console, services the USB hosts and `hlt`s), `start` (process 1, the in-kernel shell, then the idle task), `spawn` (as plan 2's, into a new process group), `wait` (blocks until the child is a zombie, then frees its kernel stack), `end` (memory and fds at once, CR3 to the kernel's tables first) and `with_cwd`. The shell's `KernelVfs` works with the running process's current directory; its console read blocks until the idle task sees input. A program's first run is `first_run`, which calls `arch::user::enter(&exec::Entry)`: the entry registers stay in `arch` (plan 2's deferred minor). The first switch's assertion found that a program's AC came into the kernel through a fault (`flags-ud`), which Task 7's stubs now prevent. Mutation check: a TSS `rsp0` that differs from the `syscall` stack fails the `programs` scenario (the assertion).

**Files:**
- Create: `kernel/src/arch/context.rs`
- Modify: `kernel/src/arch/gdt.rs`
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/arch/user.rs`
- Modify: `kernel/src/exec.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/mm/kstack.rs`
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/mounts.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: Tasks 3, 5, 6, 7, 8; plan 2's `exec::Entry`, `mm::alloc_kernel_stack`, `AddressSpace`.
- Produces: `arch::context::{FRAME_WORDS, first_frame(extern "C" fn(u64) -> !, u64) -> [u64; 8], Next {rsp, pml4, stack_top}, switch_to(*mut u64, &Next), use_kernel_tables(u64)}`; `arch::user::{enter(*const Entry) -> !, set_kernel_stack(u64), kernel_stacks_agree() -> bool}`, `arch::gdt::kernel_stack() -> u64`; `proc::{start(extern "C" fn(u64) -> !, u64, Cwd) -> !, spawn, wait, with_cwd, wait_for_input, system_call, fault, non_canonical_return}`; `session::shell(u64) -> !`; `KernelStack::slot()`, `mm::is_locked()`, `tty::has_input()`; `exec::Entry` is `repr(C)`.

- [ ] **Step 1: Write the failing tests for `kernel/src/arch/context.rs`**

Create `kernel/src/arch/context.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn f(_: u64) -> ! {
        unreachable!()
    }

    #[test]
    fn a_first_frame_is_what_switch_pops() {
        let w = first_frame(f, 42);
        assert_eq!(w.len() * 8, 64, "the stack's top 64 bytes");
        assert_eq!(w[0], 0x2, "popfq: interrupts off, bit 1");
        // pop r15, r14, r13, r12, rbx, rbp.
        assert_eq!(w[3], 42, "r13: the argument");
        assert_eq!(w[4], f as *const () as u64, "r12: the function");
        assert_eq!([w[1], w[2], w[5], w[6]], [0; 4]);
        assert_eq!(w[7], start as *const () as u64, "ret");
    }

    #[test]
    fn a_switch_pushes_what_a_first_frame_holds() {
        // SAFETY: reads the kernel's own code.
        let code = unsafe { *(switch as *const [u8; 11]) };
        // push rbp, rbx, r12, r13, r14, r15; pushfq: the first frame's
        // words from the top down.
        assert_eq!(
            code,
            [
                0x55, 0x53, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x9C
            ]
        );
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust

pub mod fault;
````

with:

````rust

pub mod context;
pub mod fault;
````

- [ ] **Step 3: Add the failing tests to `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn enter_reads_the_entry_at_these_offsets() {
        assert_eq!(offset_of!(UserEntry, ip), 0);
        assert_eq!(offset_of!(UserEntry, sp), 8);
        assert_eq!(offset_of!(UserEntry, rdi), 16);
        assert_eq!(offset_of!(UserEntry, rsi), 24);
        assert_eq!(offset_of!(UserEntry, rdx), 32);
        assert_eq!(offset_of!(PerCpu, kernel_rsp), 0);
````

with:

````rust
    #[test]
    fn the_per_cpu_block_is_where_syscall_entry_looks() {
        assert_eq!(offset_of!(PerCpu, kernel_rsp), 0);
````

Replace:

````rust
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
        assert_eq!(KERNEL_RFLAGS, 2, "leave: nothing of the program's");
    }
````

with:

````rust
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::`

Expected: FAIL: compile errors such as `` cannot find value `start` in this scope ``; `` cannot find value `switch` in this scope ``.

- [ ] **Step 5: Implement `kernel/src/arch/context.rs`**

Insert this at the top of `kernel/src/arch/context.rs`, above `#[cfg(test)]`:

````rust
//! The context switch on x86_64 (user-space gate §5.4, §6.1). A process
//! that is not running is a kernel stack whose top holds everything needed
//! to go on: the kernel code that gave up the CPU pushed its callee-saved
//! registers and flags there, and the process table keeps only the stack
//! pointer. `switch` saves the current context that way and continues
//! another; a new stack gets a first frame that looks as if it had given
//! up the CPU just before calling its function.
//!
//! Every switch happens in the kernel, with interrupts off and no lock
//! held; `switch_to` first points the CPU at the next process's page
//! tables and kernel stack (TSS `rsp0`, and the per-CPU block's stack for
//! `syscall`).

use core::arch::naked_asm;
use x86_64::PhysAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::PhysFrame;

/// Words in a first frame (and in what `switch` pushes, with the return
/// address).
pub const FRAME_WORDS: usize = 8;

/// The words a new kernel stack starts with, lowest address first, so the
/// first `switch` to it (with the stack pointer at the first word) runs
/// `f(arg)` with interrupts on: the flags (interrupts off until `start`),
/// `r15` to `rbp` as `switch` pops them (`r12` holding `f`, `r13` `arg`),
/// and `start` as the return address. The frame fills the top 64 bytes of
/// a 16-byte aligned stack, so `f` is called with the stack aligned as the
/// ABI wants.
pub fn first_frame(f: extern "C" fn(u64) -> !, arg: u64) -> [u64; FRAME_WORDS] {
    [
        0x2,
        0,
        0,
        arg,
        f as *const () as u64,
        0,
        0,
        start as *const () as u64,
    ]
}

/// Where a new stack's first `switch` returns to.
#[unsafe(naked)]
unsafe extern "C" fn start() -> ! {
    naked_asm!("mov rdi, r13", "sti", "call r12", "ud2")
}

/// Saves the callee-saved registers and the flags on the current stack and
/// the stack pointer in `*save`, then continues the context saved at
/// `next`. Returns when something switches back to `*save`.
///
/// # Safety
/// Interrupts off; `next` is a stack pointer a `switch` saved or a first
/// frame's, on a mapped stack.
#[unsafe(naked)]
unsafe extern "C" fn switch(save: *mut u64, next: u64) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "pushfq",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "popfq",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

/// What the next process runs on: its page tables and the top of its
/// kernel stack.
pub struct Next {
    /// The stack pointer `switch` saved, or its first frame's.
    pub rsp: u64,
    /// Its PML4 (the kernel's own for a process without a program).
    pub pml4: u64,
    pub stack_top: u64,
}

/// Saves the running context in `*save` and continues `next`, with the CPU
/// pointed at its page tables and kernel stack. Returns when something
/// switches back.
///
/// # Safety
/// As `switch`; `next.pml4` maps the kernel as the current tables do.
pub unsafe fn switch_to(save: *mut u64, next: &Next) {
    debug_assert!(
        !x86_64::instructions::interrupts::are_enabled(),
        "a switch with interrupts on"
    );
    debug_assert!(
        super::user::flags_are_kernel(),
        "a program's flags reached a switch"
    );
    super::user::set_kernel_stack(next.stack_top);
    debug_assert!(
        super::user::kernel_stacks_agree(),
        "TSS rsp0 and the syscall stack differ"
    );
    let (current, _) = Cr3::read_raw();
    if current.start_address().as_u64() != next.pml4 {
        // SAFETY: the tables map the kernel as the current ones do.
        unsafe {
            Cr3::write(
                PhysFrame::containing_address(PhysAddr::new(next.pml4)),
                Cr3Flags::empty(),
            )
        };
    }
    // SAFETY: the caller's.
    unsafe { switch(save, next.rsp) };
    debug_assert!(
        super::user::flags_are_kernel(),
        "a program's flags came back with a switch"
    );
}

/// Points CR3 at the kernel's own tables (before a program's are freed).
pub fn use_kernel_tables(pml4: u64) {
    // SAFETY: the kernel's tables map everything the kernel uses.
    unsafe {
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(pml4)),
            Cr3Flags::empty(),
        )
    };
}

````

- [ ] **Step 6: Change `kernel/src/arch/gdt.rs`**

In `kernel/src/arch/gdt.rs`, replace:

````rust

#[cfg(test)]
````

with:

````rust

/// TSS `rsp0`, as `set_kernel_stack` left it.
pub fn kernel_stack() -> u64 {
    // SAFETY: a plain read on one CPU.
    unsafe { TSS.privilege_stack_table[0].as_u64() }
}

#[cfg(test)]
````

- [ ] **Step 7: Change `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust
//! x86_64-specific setup: segmentation, interrupt table, CPU control, and
//! ring 3.

````

with:

````rust
//! x86_64-specific setup: segmentation, interrupt table, CPU control, ring
//! 3 and the context switch.

````

- [ ] **Step 8: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//! Ring 3 on x86_64 (user-space gate §5.3, §6.2): the `syscall` entry, going
//! into a program and coming back out of it.
//!
````

with:

````rust
//! Ring 3 on x86_64 (user-space gate §5.3, §6.2): the `syscall` entry, and
//! going into a program the first time.
//!
````

Replace:

````rust
//!   stack pointer, so such a program is killed instead.
//! - **`enter`** saves the kernel's callee-saved registers and stack pointer
//!   (the waiting kernel code), then goes to ring 3 with `iretq`; **`leave`**
//!   goes back to that kernel code from the program's kernel stack, which
//!   is abandoned, with the kernel's flags. `exit` and a program's fault
//!   end that way.
//!
//! A program sets the flags it likes (`popfq`), and neither `syscall` nor
//! an exception clears all of them: `SFMASK` clears the ones the kernel
//! must not run with (NT would make the next `iretq` fault in ring 0; AC
//! would switch SMAP off), and `leave` loads the kernel's own flags, so
//! nothing of the program's reaches the code that waited for it.
//!
````

with:

````rust
//!   stack pointer, so such a program is killed instead.
//! - **`enter`** goes to ring 3 at a program's entry point with `iretq`,
//!   the first time the program runs (a new process's first frame calls it,
//!   see `context`). After that a program leaves the kernel only the way
//!   it came in: returning from a system call or an interrupt.
//!
//! A program sets the flags it likes (`popfq`), and neither `syscall` nor
//! an exception clears all of them: `SFMASK` clears the ones the kernel
//! must not run with (NT would make the next `iretq` fault in ring 0; AC
//! would switch SMAP off), and every switch between processes checks that
//! none of them reached the kernel (`context`).
//!
````

Replace:

````rust
use super::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA};
use core::arch::naked_asm;
use x86_64::instructions::interrupts;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::registers::model_specific::{
````

with:

````rust
use super::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA};
use crate::exec::Entry;
use core::arch::naked_asm;
use x86_64::VirtAddr;
use x86_64::registers::model_specific::{
````

Replace:

````rust
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::paging::PhysFrame;
use x86_64::{PhysAddr, VirtAddr};

````

with:

````rust
use x86_64::structures::gdt::SegmentSelector;

````

Replace:

````rust
    .union(RFlags::NESTED_TASK);

/// The kernel's flags when `leave` goes back: only bit 1, always set.
const KERNEL_RFLAGS: u64 = 0x2;

````

with:

````rust
    .union(RFlags::NESTED_TASK);

````

Replace:

````rust

/// Where a program starts (spec §5.3), in the order `enter` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserEntry {
    pub ip: u64,
    pub sp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
}

/// Whether `addr` is canonical: bits 63-47 all equal.
````

with:

````rust

/// Whether `addr` is canonical: bits 63-47 all equal.
````

Replace:

````rust

/// Runs a program: `entry` in the address space whose PML4 is at `pml4`,
/// with `kernel_stack` (its top) for its system calls, interrupts and
/// faults. Returns once something calls `leave(waiter)`, with the kernel's
/// own tables (`kernel_pml4`) back in CR3 and interrupts as they were.
/// `waiter` is where `enter` saves the stack pointer to go back to.
///
/// # Safety
/// `waiter` must stay valid until `run` returns, and only this program's
/// `leave` may use it; `pml4` must map the kernel as the current tables do.
pub unsafe fn run(
    entry: &UserEntry,
    kernel_stack: u64,
    pml4: u64,
    kernel_pml4: u64,
    waiter: *mut u64,
) {
    let enabled = interrupts::are_enabled();
    interrupts::disable();
    super::gdt::set_kernel_stack(kernel_stack);
    // SAFETY: interrupts are off and no system call is running, so nothing
    // reads the per-CPU block now. The page tables map the kernel as the
    // current ones do (the upper half is shared).
    unsafe {
        PER_CPU.kernel_rsp = kernel_stack;
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(pml4)),
            Cr3Flags::empty(),
        );
        enter(waiter, entry);
        Cr3::write(
            PhysFrame::containing_address(PhysAddr::new(kernel_pml4)),
            Cr3Flags::empty(),
        );
    }
    // A kernel bug if the program's flags came back with it.
    debug_assert!(
        !x86_64::registers::rflags::read().intersects(PROGRAM_FLAGS),
        "a program's flags reached the kernel"
    );
    if enabled {
        interrupts::enable();
    }
}

/// Saves the callee-saved registers and the stack pointer in `*waiter`,
/// then goes to ring 3 at `entry` with the program's `gs`: its stack,
/// `rdi`, `rsi`, `rdx`, interrupts on, every other register zero. Returns
/// when `leave(waiter)` runs.
#[unsafe(naked)]
unsafe extern "C" fn enter(waiter: *mut u64, entry: *const UserEntry) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "push {ss}",
        "push qword ptr [rsi + 8]",
        "push {rflags}",
        "push {cs}",
        "push qword ptr [rsi]",
        "mov rdi, [rsi + 16]",
        "mov rdx, [rsi + 32]",
        "mov rsi, [rsi + 24]",
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor ebp, ebp",
````

with:

````rust

/// Points the CPU at `top` as the kernel stack for the next process's
/// system calls (the per-CPU block) and for its interrupts and exceptions
/// (TSS `rsp0`).
pub fn set_kernel_stack(top: u64) {
    // SAFETY: interrupts are off during a switch and no system call is
    // running, so nothing reads the per-CPU block now.
    unsafe { PER_CPU.kernel_rsp = top };
    super::gdt::set_kernel_stack(top);
}

/// Whether `syscall` and interrupts from ring 3 would land on the same
/// kernel stack.
pub fn kernel_stacks_agree() -> bool {
    // SAFETY: a plain read, as above.
    unsafe { PER_CPU.kernel_rsp == super::gdt::kernel_stack() }
}

/// Goes to ring 3 at `entry` (spec §5.3) with the program's `gs`: its
/// stack, `rdi` = the arguments' address, `rsi` their length, `rdx` their
/// count, interrupts on, every other register zero. The kernel stack this
/// is called on is where the program's system calls, interrupts and
/// faults arrive from now on.
///
/// # Safety
/// The program's address space is in CR3 and its kernel stack is set
/// (`set_kernel_stack`).
#[unsafe(naked)]
pub unsafe extern "C" fn enter(entry: *const Entry) -> ! {
    naked_asm!(
        "cli",
        "push {ss}",
        "push qword ptr [rdi + {sp}]",
        "push {rflags}",
        "push {cs}",
        "push qword ptr [rdi + {ip}]",
        "mov rsi, [rdi + {len}]",
        "mov rdx, [rdi + {argc}]",
        "mov rdi, [rdi + {args}]",
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor ebp, ebp",
````

Replace:

````rust
        rflags = const USER_RFLAGS,
    )
}

/// Goes back to the kernel code waiting in `enter(waiter, _)`, on its
/// stack, with interrupts off and the kernel's flags. The stack this runs
/// on is abandoned.
///
/// # Safety
/// `waiter` must be what `enter` saved, and that `enter` must not have
/// returned yet.
#[unsafe(naked)]
pub unsafe extern "C" fn leave(waiter: *mut u64) -> ! {
    naked_asm!(
        "cli",
        "mov rsp, [rdi]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "push {rflags}",
        "popfq",
        "ret",
        rflags = const KERNEL_RFLAGS,
    )
````

with:

````rust
        rflags = const USER_RFLAGS,
        ip = const core::mem::offset_of!(Entry, ip),
        sp = const core::mem::offset_of!(Entry, sp),
        args = const core::mem::offset_of!(Entry, args),
        len = const core::mem::offset_of!(Entry, args_len),
        argc = const core::mem::offset_of!(Entry, argc),
    )
````

- [ ] **Step 9: Change `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, replace:

````rust
/// aligned, below the arguments), and its arguments' address, length and
/// count (spec §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

with:

````rust
/// aligned, below the arguments), and its arguments' address, length and
/// count (spec §5.3). `arch::user::enter` reads it (`repr(C)`) and puts
/// each where its architecture says.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

- [ ] **Step 10: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    let root = mounts::init(vfs);
    session::run_shell(root, cmdline.test_mode)
}
````

with:

````rust
    let root = mounts::init(vfs);
    proc::start(session::shell, u64::from(cmdline.test_mode), root)
}
````

- [ ] **Step 11: Change `kernel/src/mm/kstack.rs`**

In `kernel/src/mm/kstack.rs`, replace:

````rust
impl KernelStack {
    /// The lowest address of the stack; the page below is the guard.
````

with:

````rust
impl KernelStack {
    /// Its slot, 0 to `SLOTS - 1`: one per process at most.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// The lowest address of the stack; the page below is the guard.
````

- [ ] **Step 12: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
    HEAP.stats().largest_free.saturating_sub(HEAP_MARGIN)
}
````

with:

````rust
    HEAP.stats().largest_free.saturating_sub(HEAP_MARGIN)
}

/// Whether the memory manager is locked now (for the kernel's checks that
/// no lock is held across a switch).
pub fn is_locked() -> bool {
    MEMORY.is_locked()
}
````

- [ ] **Step 13: Change `kernel/src/mounts.rs`**

In `kernel/src/mounts.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// The mount table as the in-kernel shell's `Vfs`, with the shell's own
/// current directory.
pub struct KernelVfs {
    cwd: Cwd,
}

impl KernelVfs {
    pub fn new(cwd: Cwd) -> KernelVfs {
        KernelVfs { cwd }
    }

    /// Its current directory, for a program it starts.
    pub fn current(&self) -> &Cwd {
        &self.cwd
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut MountTable) -> R) -> R {
        with(&mut self.cwd, f)
    }
````

with:

````rust

/// The mount table as the in-kernel shell's `Vfs`, with the current
/// directory of the process it works for.
pub struct KernelVfs;

impl KernelVfs {
    fn with<R>(&mut self, f: impl FnOnce(&mut MountTable) -> R) -> R {
        crate::proc::with_cwd(|cwd| with(cwd, f))
    }
````

Replace:

````rust
    fn cwd(&self) -> Vec<u8> {
        with(&mut self.cwd.clone(), |t| t.cwd())
    }
````

with:

````rust
    fn cwd(&self) -> Vec<u8> {
        crate::proc::with_cwd(|cwd| with(cwd, |t| t.cwd()))
    }
````

- [ ] **Step 14: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! The programs the kernel runs (user-space gate §5), as plan 2 of
//! milestone 2 has them: one child at a time, started by the in-kernel
//! shell. `spawn` loads the child; `wait` runs it on its own kernel stack
//! until it exits, and gives everything it had back. There is no
//! scheduler: while the child runs, the shell waits inside `wait`, and the
//! timer only counts ticks. Plan 3 replaces this with the process table and
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

pub mod table;

use crate::exec::{self, Entry};
use crate::mm::kstack::KernelStack;
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::UserSlice;
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, klogln, mm};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use relay_abi::WaitStatus;
use spin::Mutex;
use vfs::{Errno, FileType, Vfs};

/// A program `spawn` loaded and nobody has waited for yet.
struct Child {
    pid: u32,
    /// Its path, for the kernel log.
    name: String,
    space: AddressSpace,
    stack: KernelStack,
    entry: Entry,
}

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// Pids count from 1 and are not used again while the kernel runs.
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

/// The child while it runs: what its system calls and faults reach. Lives
/// on `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
    pid: u32,
    name: &'a str,
    space: &'a AddressSpace,
    out: &'a mut dyn FnMut(u32, &[u8]),
    /// The stack pointer `arch::user::enter` saved; `leave` goes back to it.
    waiter: u64,
    status: Option<WaitStatus>,
}

static RUNNING: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

impl Caller for Running<'_> {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        // The lock is held only for the copy: the output may reach a disk,
        // whose driver allocates memory too.
        mm::with_user_memory(|mem, _| slice.read(self.space, mem, offset, buf))
    }

    fn output(&mut self, fd: u32, bytes: &[u8]) {
        (self.out)(fd, bytes);
    }
}
````

with:

````rust
//! Processes (user-space gate §5.4, §6.1): the process table, and the
//! kernel's side of running them. Every process has its own kernel stack;
//! one that is not running is switched out on it (`arch::context`). The
//! idle task is the context the kernel booted in, process 0: it runs when
//! nothing is ready, polls the console and the USB hosts, and sleeps until
//! the next tick. Process 1 is the in-kernel shell, a process without a
//! program, which blocks in `wait` while its commands run and on the
//! console while it waits for a line (plan 4 replaces it with `/bin/sh`).
//!
//! The kernel is not preemptible, and a switch happens only in the kernel
//! with interrupts off and no lock held, so a blocked process never holds
//! a lock another one needs.

pub mod table;

use crate::arch::context::{self, Next};
use crate::exec::{self, Entry};
use crate::fd::FdTable;
use crate::mm::kstack::{self, KernelStack};
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::UserSlice;
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, console, klogln, mm, mounts, tty, usb};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::WaitStatus;
use spin::Mutex;
use table::{Blocked, Table, Want};
use vfs::{Cwd, Errno, FileType, Vfs};
use x86_64::instructions::interrupts;

/// What a process owns besides its entry in the table.
struct Res {
    stack: KernelStack,
    /// Its program's memory; `None` for the in-kernel shell, and once the
    /// process has ended.
    space: Option<AddressSpace>,
    /// Where its program starts, until it first runs.
    entry: Option<Entry>,
    fds: FdTable,
    /// Its current directory; out of the table while the mount table works
    /// with it (`with_cwd`).
    cwd: Option<Cwd>,
}

static PROCS: Mutex<Table<Res>> = Mutex::new(Table::new());

/// The stack pointer each switched-out process left (`arch::context`), by
/// kernel-stack slot: fixed addresses, which the table's entries are not.
static SAVED: [AtomicU64; kstack::SLOTS] = [const { AtomicU64::new(0) }; kstack::SLOTS];
/// The idle task's.
static IDLE: AtomicU64 = AtomicU64::new(0);

/// The in-kernel shell's output while it waits for a command (plan 2's
/// hook, `System::wait`): what its children write to fds 1 and 2.
type Out<'a> = &'a mut dyn FnMut(u32, &[u8]);
static SHELL_OUT: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Where the running context is saved when it gives up the CPU.
fn save_slot(t: &Table<Res>) -> *mut u64 {
    match t.get(t.current()) {
        Some(p) => SAVED[p.res.stack.slot()].as_ptr(),
        None => IDLE.as_ptr(),
    }
}

/// What the running process runs on, for the switch to it.
fn next(t: &Table<Res>) -> Next {
    match t.get(t.current()) {
        Some(p) => Next {
            rsp: SAVED[p.res.stack.slot()].load(Ordering::Relaxed),
            pml4: p
                .res
                .space
                .as_ref()
                .map_or_else(mm::kernel_pml4, AddressSpace::pml4),
            stack_top: p.res.stack.top(),
        },
        None => Next {
            rsp: IDLE.load(Ordering::Relaxed),
            pml4: mm::kernel_pml4(),
            stack_top: 0,
        },
    }
}

/// A kernel bug if one of these is held when the CPU goes to another
/// process: that one could wait for it for ever.
fn no_lock_held() -> bool {
    !PROCS.is_locked() && !mounts::is_locked() && !tty::is_locked() && !mm::is_locked()
}

/// Gives the CPU to the next ready process, or to the idle task, and
/// returns when the running one gets it back (at once if nothing else is
/// ready and it still is). A process that has blocked returns once woken;
/// one that has ended never does.
fn reschedule() {
    let enabled = interrupts::are_enabled();
    interrupts::disable();
    let switch = {
        let mut t = PROCS.lock();
        let me = t.current();
        let save = save_slot(&t);
        (t.schedule() != me).then(|| (save, next(&t)))
    };
    if let Some((save, next)) = switch {
        debug_assert!(no_lock_held(), "a lock held across a switch");
        // SAFETY: interrupts are off; `next` is a context `switch` saved
        // or a first frame; every process's tables map the kernel.
        unsafe { context::switch_to(save, &next) };
    }
    if enabled {
        interrupts::enable();
    }
}

/// The running process blocks on `why` until something wakes it.
fn block(why: Blocked) {
    PROCS.lock().block(why);
    reschedule();
}

/// The idle task (spec §6.1): the context the kernel booted in, from the
/// moment process 1 exists. Never returns.
fn idle() -> ! {
    loop {
        tty::poll();
        if tty::has_input() {
            PROCS.lock().wake_all(Blocked::Console);
        }
        usb::service();
        if PROCS.lock().others_ready() {
            reschedule();
        } else {
            arch::wait_for_interrupt();
        }
    }
}

/// A new kernel stack whose first switch runs `f(arg)`.
fn prepare(stack: &KernelStack, f: extern "C" fn(u64) -> !, arg: u64) {
    let frame = context::first_frame(f, arg);
    let at = stack.top() - (frame.len() * 8) as u64;
    // SAFETY: the stack is mapped, nothing runs on it yet, and the frame
    // fits in its top page.
    unsafe { core::ptr::write(at as *mut [u64; context::FRAME_WORDS], frame) };
    SAVED[stack.slot()].store(at, Ordering::Relaxed);
}

/// The running process (the in-kernel shell) waits until something is
/// typed.
pub fn wait_for_input() {
    block(Blocked::Console);
}

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
    assert_eq!(pid, table::INIT);
    idle()
}

/// Runs `f` with the running process's current directory, and keeps what
/// `f` makes of it.
pub fn with_cwd<R>(f: impl FnOnce(&mut Cwd) -> R) -> R {
    let taken = {
        let mut t = PROCS.lock();
        let me = t.current();
        t.get_mut(me).and_then(|p| p.res.cwd.take())
    };
    let mut cwd = taken.expect("a process with its current directory");
    let r = f(&mut cwd);
    let mut t = PROCS.lock();
    let me = t.current();
    if let Some(p) = t.get_mut(me) {
        p.res.cwd = Some(cwd);
    }
    r
}

/// Where a new process's program starts: its first switch comes here.
extern "C" fn first_run(_: u64) -> ! {
    let entry = {
        let mut t = PROCS.lock();
        let me = t.current();
        t.get_mut(me).and_then(|p| p.res.entry.take())
    };
    let entry = entry.expect("a new process has its entry");
    // SAFETY: the switch put its address space in CR3 and its kernel stack
    // in the TSS and the per-CPU block; this stack is its kernel stack.
    unsafe { arch::user::enter(&entry) }
}
````

Replace:

````rust
/// Loads the program at `path` (read through `vfs`) with `args` (argument
/// 0 first) as the child; its pid. `EAGAIN` while another child exists.
pub fn spawn(vfs: &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Result<u32, Errno> {
    let mut child = CHILD.lock();
    if child.is_some() {
        return Err(Errno::EAGAIN);
````

with:

````rust
/// Loads the program at `path` (read through `vfs`) with `args` (argument
/// 0 first) as a child of the running process, in a new process group,
/// with the running process's fds 0-2 and current directory; its pid.
/// `EAGAIN` when the table is full.
pub fn spawn(vfs: &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Result<u32, Errno> {
    if PROCS.lock().len() >= table::MAX {
        return Err(Errno::EAGAIN);
````

Replace:

````rust
    };
    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
    *child = Some(Child {
        pid,
        name: name.into_owned(),
        space,
        stack,
        entry,
    });
    Ok(pid)
}

/// Runs the child `pid` until it ends, giving what it writes to fds 1 and 2
/// to `out`; then gives back its memory and kernel stack. `ECHILD` if
/// `pid` is not the child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let child = {
        let mut slot = CHILD.lock();
        match slot.take() {
            Some(c) if c.pid == pid => c,
            other => {
                *slot = other;
                return Err(Errno::ECHILD);
            }
        }
    };
    let mut running = Running {
        pid: child.pid,
        name: &child.name,
        space: &child.space,
        out,
        waiter: 0,
        status: None,
    };
    // From here until `run` returns, `running` is reached only through
    // this pointer: here, and in its system calls and faults (`RUNNING`).
    let r = &raw mut running;
    let e = child.entry;
    let entry = arch::user::UserEntry {
        ip: e.ip,
        sp: e.sp,
        rdi: e.args,
        rsi: e.args_len,
        rdx: e.argc,
    };
    RUNNING.store(r.cast(), Ordering::Release);
    // SAFETY: `running` outlives the run, and nothing else touches it
    // while this function waits in `run`; the space shares the kernel's
    // upper half.
    unsafe {
        arch::user::run(
            &entry,
            child.stack.top(),
            child.space.pml4(),
            mm::kernel_pml4(),
            &raw mut (*r).waiter,
        );
    }
    RUNNING.store(core::ptr::null_mut(), Ordering::Release);
    // SAFETY: the run is over; `r` is the only way to `running` again.
    let status = unsafe { (*r).status }.unwrap_or_default();
    mm::with_user_memory(|mem, _| child.space.destroy(mem));
    mm::free_kernel_stack(child.stack);
    Ok(status)
}

/// The running child and the stack pointer to leave to.
///
/// # Safety
/// Only while a child runs (from inside its system calls and faults).
unsafe fn running<'a>() -> &'a mut Running<'a> {
    let p = RUNNING.load(Ordering::Acquire);
    assert!(!p.is_null(), "a system call without a running program");
    // SAFETY: set by `wait` to its `Running`, which lives until `run`
    // returns.
    unsafe { &mut *p.cast::<Running<'a>>() }
}

/// Ends the running child with `status` and goes back to `wait`.
fn end(status: WaitStatus) -> ! {
    // SAFETY: called on the child's kernel stack while it runs.
    let r = unsafe { running() };
    r.status = Some(status);
    let waiter = &raw mut r.waiter;
    // SAFETY: `enter` saved it and has not returned.
    unsafe { arch::user::leave(waiter) }
}

/// A system call of the running child: its result register, or, for
/// `exit`, back to `wait`.
pub fn system_call(number: u64, args: [u64; 6]) -> u64 {
    // SAFETY: called from the entry stub, on the child's kernel stack.
    let r = unsafe { running() };
    match syscall::dispatch(r, number, args) {
        Outcome::Return(result) => result,
````

with:

````rust
    };
    prepare(&stack, first_run, 0);
    let mut t = PROCS.lock();
    let me = t.current();
    let parent = t.get(me);
    let fds = parent.map_or_else(FdTable::new, |p| {
        p.res
            .fds
            .for_child(&[
                relay_abi::FdMap {
                    child: 0,
                    parent: 0,
                },
                relay_abi::FdMap {
                    child: 1,
                    parent: 1,
                },
                relay_abi::FdMap {
                    child: 2,
                    parent: 2,
                },
            ])
            .unwrap_or_default()
    });
    let cwd = parent.and_then(|p| p.res.cwd.clone());
    let res = Res {
        stack,
        space: Some(space),
        entry: Some(entry),
        fds,
        cwd,
    };
    t.insert(me, true, name.into_owned(), res)
        .map_err(|e| unreachable!("room was checked, and nothing else runs: {e}"))
}

/// Waits until the child `pid` of the running process has ended, giving
/// what the in-kernel shell's children write to fds 1 and 2 to `out`
/// meanwhile; then gives back what was left of it. `ECHILD` if `pid` is
/// not its child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = loop {
        let reaped = {
            let mut t = PROCS.lock();
            let me = t.current();
            t.reap(me, Want::Pid(pid))
        };
        match reaped {
            Ok(Some(p)) => break Ok(p),
            Ok(None) => block(Blocked::Wait),
            Err(e) => break Err(e),
        }
    };
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    let p = ended?;
    mm::free_kernel_stack(p.res.stack);
    match p.state {
        table::State::Zombie(status) => Ok(status),
        _ => unreachable!("reap takes only zombies"),
    }
}

/// The running process ends with `status` (spec §5.4): its memory and fds
/// are given back at once, it stays a zombie until its parent waits for
/// it, and the CPU goes to the next process for good.
fn end(status: WaitStatus) -> ! {
    let (space, fds) = {
        let mut t = PROCS.lock();
        let me = t.current();
        let p = t.get_mut(me).expect("a running process ends");
        let taken = (p.res.space.take(), core::mem::take(&mut p.res.fds));
        t.end(me, status);
        taken
    };
    drop(fds);
    if let Some(space) = space {
        // Off its page tables before they go; the kernel stack is in the
        // kernel's half, which every table maps.
        context::use_kernel_tables(mm::kernel_pml4());
        mm::with_user_memory(|mem, _| space.destroy(mem));
    }
    reschedule();
    unreachable!("a zombie ran again")
}

/// The running process's side of the dispatcher.
struct Current;

impl Caller for Current {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        let t = PROCS.lock();
        let space = t
            .get(t.current())
            .and_then(|p| p.res.space.as_ref())
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| slice.read(space, mem, offset, buf))
    }

    fn output(&mut self, fd: u32, bytes: &[u8]) {
        let out = SHELL_OUT.load(Ordering::Acquire);
        if out.is_null() {
            console::write_output(bytes);
        } else {
            // SAFETY: set by the in-kernel shell's `wait`, which is blocked
            // until this child has ended, and cleared before it returns;
            // nothing else calls it meanwhile.
            unsafe { (*out.cast::<Out<'_>>())(fd, bytes) }
        }
    }
}

/// A system call of the running process: its result register, or, for
/// `exit`, the end of it.
pub fn system_call(number: u64, args: [u64; 6]) -> u64 {
    match syscall::dispatch(&mut Current, number, args) {
        Outcome::Return(result) => result,
````

Replace:

````rust

/// The running child caused an exception (spec §11.1): it is killed, and
/// the kernel log says how. A page fault in the stack's guard page is a
````

with:

````rust

/// The running process caused an exception (spec §11.1): it is killed, and
/// the kernel log says how. A page fault in the stack's guard page is a
````

Replace:

````rust
    let status = WaitStatus::fault(kind, detail, address, ip);
    // SAFETY: called on the child's kernel stack while it runs.
    let r = unsafe { running() };
    klogln!("pid {} ({}): killed: {status}", r.pid, r.name);
    end(status)
}

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the child is killed as if the return
/// itself had faulted.
pub fn non_canonical_return(ip: u64) -> ! {
````

with:

````rust
    let status = WaitStatus::fault(kind, detail, address, ip);
    {
        let t = PROCS.lock();
        let me = t.current();
        let name = t.get(me).map_or("?", |p| p.name.as_str());
        klogln!("pid {me} ({name}): killed: {status}");
    }
    end(status)
}

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the process is killed as if the
/// return itself had faulted.
pub fn non_canonical_return(ip: u64) -> ! {
````

- [ ] **Step 15: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::mounts::KernelVfs;
use crate::{arch, console, klog, klogln, power, proc, rtc, tty, usb};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Cwd, Env, Errno, Vfs};

````

with:

````rust
use crate::mounts::KernelVfs;
use crate::{arch, console, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, Errno, Vfs};

````

Replace:

````rust
            }
            // Nothing typed: time for the work that may wait (a keyboard
            // plugged in, the Caps Lock LED), then sleep until the next
            // tick.
            usb::service();
            arch::wait_for_interrupt();
        }
````

with:

````rust
            }
            // Nothing typed: the idle task polls until something is.
            proc::wait_for_input();
        }
````

Replace:

````rust

/// Runs the shell over the kernel's mount table (the root at `/`, the
/// programs at `/bin`), starting in `cwd` (spec §4.4 step 11). Never
/// returns.
pub fn run_shell(cwd: Cwd, test_mode: bool) -> ! {
    let mut vfs = KernelVfs::new(cwd);
    let mut console = KernelConsole;
    let mut system = KernelSystem { test_mode };
    Shell::new(&mut vfs, &mut console, &mut system).run();
````

with:

````rust

/// Process 1 (spec §4.4 step 11): the shell over the kernel's mount table
/// (the root at `/`, the programs at `/bin`); `test_mode` is 1 for
/// `test=1`. Never returns.
pub extern "C" fn shell(test_mode: u64) -> ! {
    let mut vfs = KernelVfs;
    let mut console = KernelConsole;
    let mut system = KernelSystem {
        test_mode: test_mode != 0,
    };
    Shell::new(&mut vfs, &mut console, &mut system).run();
````

- [ ] **Step 16: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! the USB keyboards and COM1, filled by `poll`. `poll` never waits, so it
//! can run wherever the kernel holds nothing: in the console's idle loop
//! and whenever a command asks whether Ctrl-C was pressed.

````

with:

````rust
//! the USB keyboards and COM1, filled by `poll`. `poll` never waits, so it
//! can run wherever the kernel holds nothing: in the idle task, and
//! whenever the in-kernel shell reads or asks whether Ctrl-C was pressed.

````

Replace:

````rust

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
````

with:

````rust

/// Whether anything typed waits to be read.
pub fn has_input() -> bool {
    !INPUT.lock().is_empty()
}

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
````

- [ ] **Step 17: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 253 tests.

- [ ] **Step 18: Run the `programs`, `userfault`, `shell`, `keyboard` scenarios**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 19: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 20: Commit**

````bash
git add kernel
git commit -m "kernel: processes on the process table, the context switch, the idle task, and the in-kernel shell as process 1"
````


### Task 10: The time and sleep calls

Spec §7.3's `time(&mut Time)` and `sleep(ms)`. The dispatcher's `Caller` gains `write` (copy out, Task 2), `time` and `sleep`; `time` fills in the wall clock and the uptime (from the TSC, `timer::tsc_time`, in nanoseconds) and writes them whole or not at all (`EFAULT`), `sleep` blocks the program until the tick count reaches its end (`Blocked::Sleep`), which the idle task wakes. Without a ticking timer (`timer::init` failed) nothing would wake a sleeper, so `sleep` returns at once (decision 6). `relay-rt` gains `sys::time` and `sys::sleep`. Mutation checks: `sleep` passing another argument and `time` without its uptime fail a test.

**Files:**
- Modify: `crates/relay-abi/src/info.rs`
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/timer.rs`

**Interfaces:**
- Consumes: Tasks 1 (`Time`), 2 (`UserSlice::write`), 9 (`block`, `idle`).
- Produces: `syscall::Caller::{write, time, sleep}`; `timer::is_ticking() -> bool`; `relay_rt::sys::{time() -> Result<Time, u16>, sleep(ms)}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

    /// A program with three readable pages at `U` holding a pattern, and
    /// nothing after them; what it wrote.
    struct Fake {
        mem: FakeMem,
        space: AddressSpace,
        written: Vec<(u32, Vec<u8>)>,
    }

````

with:

````rust

    /// A program with three readable pages at `U` holding a pattern, a
    /// writable page after them and nothing after that; what it wrote, and
    /// how long it slept.
    struct Fake {
        mem: FakeMem,
        space: AddressSpace,
        written: Vec<(u32, Vec<u8>)>,
        slept: Vec<u64>,
    }

    /// What the fake clock says.
    const NOW: Time = Time {
        unix_seconds: 1_790_000_000,
        uptime_ns: 12_345_678_901,
    };

````

Replace:

````rust
        }
        fn output(&mut self, fd: u32, bytes: &[u8]) {
            self.written.push((fd, bytes.to_vec()));
        }
    }

````

with:

````rust
        }
        fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
            slice.write(&self.space, &mut self.mem, offset, bytes)
        }
        fn output(&mut self, fd: u32, bytes: &[u8]) {
            self.written.push((fd, bytes.to_vec()));
        }
        fn time(&self) -> Time {
            NOW
        }
        fn sleep(&mut self, ms: u64) {
            self.slept.push(ms);
        }
    }

    /// The writable page.
    const W: u64 = U + 3 * PAGE;

````

Replace:

````rust
        space.fill(&mut mem, U, &pattern).unwrap();
        Fake {
            mem,
            space,
            written: Vec::new(),
        }
````

with:

````rust
        space.fill(&mut mem, U, &pattern).unwrap();
        space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
        Fake {
            mem,
            space,
            written: Vec::new(),
            slept: Vec::new(),
        }
````

Replace:

````rust
        let mut f = fake();
        let end = U + 3 * PAGE;
        assert_eq!(
````

with:

````rust
        let mut f = fake();
        let end = U + 4 * PAGE;
        assert_eq!(
````

Replace:

````rust
        assert_eq!(call(&mut f, Call::Write, [1, end - 5000, 9000]), Ok(5000));
        let want: Vec<u8> = (3 * PAGE - 5000..3 * PAGE)
            .map(|i| (i % 251) as u8)
            .collect();
````

with:

````rust
        assert_eq!(call(&mut f, Call::Write, [1, end - 5000, 9000]), Ok(5000));
        let want: Vec<u8> = (4 * PAGE - 5000..3 * PAGE)
            .map(|i| (i % 251) as u8)
            .chain([0; PAGE as usize])
            .collect();
````

Replace:

````rust
    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        for c in Call::ALL {
            if c != Call::Exit && c != Call::Write {
                assert_eq!(call(&mut f, c, [1, U, 1]), Err(errno::ENOSYS), "{c:?}");
````

with:

````rust
    #[test]
    fn time_fills_in_the_clock_s_answer() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Time, [W + 8, 0, 0]), Ok(0));
        let mut buf = [0u8; 16];
        UserSlice::new(W + 8, 16)
            .unwrap()
            .read(&f.space, &mut f.mem, 0, &mut buf)
            .unwrap();
        assert_eq!(buf[..8], NOW.unix_seconds.to_ne_bytes());
        assert_eq!(buf[8..], NOW.uptime_ns.to_ne_bytes());
        for bad in [0, U, W + PAGE - 8, u64::MAX - 4] {
            assert_eq!(
                call(&mut f, Call::Time, [bad, 0, 0]),
                Err(errno::EFAULT),
                "{bad:#x}"
            );
        }
    }

    #[test]
    fn sleep_blocks_for_what_it_is_asked() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Sleep, [250, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Sleep, [0, 0, 0]), Ok(0));
        assert_eq!(f.slept, [250, 0]);
    }

    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        for c in Call::ALL {
            if ![Call::Exit, Call::Write, Call::Time, Call::Sleep].contains(&c) {
                assert_eq!(call(&mut f, c, [1, U, 1]), Err(errno::ENOSYS), "{c:?}");
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` cannot find type `Time` in this scope ``; `` cannot find struct, variant or union type `Time` in this scope ``.

- [ ] **Step 3: Change `crates/relay-abi/src/info.rs`**

In `crates/relay-abi/src/info.rs`, replace:

````rust
    pub unix_seconds: u64,
    /// Nanoseconds since the kernel's timer started.
    pub uptime_ns: u64,
````

with:

````rust
    pub unix_seconds: u64,
    /// Nanoseconds since the machine started.
    pub uptime_ns: u64,
````

- [ ] **Step 4: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust

/// Ends the program with status `code`.
````

with:

````rust

/// The wall clock and the time since the machine started.
pub fn time() -> Result<relay_abi::Time, u16> {
    let mut t = relay_abi::Time::default();
    let args = [&raw mut t as u64, 0, 0, 0, 0, 0];
    decode(unsafe { syscall(Call::Time, args) })?;
    Ok(t)
}

/// Blocks the program for `ms` milliseconds.
pub fn sleep(ms: u64) {
    unsafe { syscall(Call::Sleep, [ms, 0, 0, 0, 0, 0]) };
}

/// Ends the program with status `code`.
````

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, console, klogln, mm, mounts, tty, usb};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::WaitStatus;
use spin::Mutex;
````

with:

````rust
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, console, klogln, mm, mounts, rtc, timer, tty, usb};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::{Time, WaitStatus};
use spin::Mutex;
````

Replace:

````rust
/// The idle task (spec §6.1): the context the kernel booted in, from the
/// moment process 1 exists. Never returns.
fn idle() -> ! {
````

with:

````rust
/// The idle task (spec §6.1): the context the kernel booted in, from the
/// moment process 1 exists. It polls the console, services the USB hosts,
/// wakes the sleepers whose time has come, and sleeps until the next tick
/// when nothing is ready. Never returns.
fn idle() -> ! {
````

Replace:

````rust
        usb::service();
        if PROCS.lock().others_ready() {
````

with:

````rust
        usb::service();
        PROCS.lock().wake_sleepers(timer::ticks());
        if PROCS.lock().others_ready() {
````

Replace:

````rust

    fn output(&mut self, fd: u32, bytes: &[u8]) {
````

with:

````rust

    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
        let t = PROCS.lock();
        let space = t
            .get(t.current())
            .and_then(|p| p.res.space.as_ref())
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| slice.write(space, mem, offset, bytes))
    }

    fn time(&self) -> Time {
        let uptime = timer::tsc_time().unwrap_or_else(timer::uptime);
        Time {
            unix_seconds: rtc::now_unix().unwrap_or(0),
            uptime_ns: u64::try_from(uptime.as_nanos()).unwrap_or(u64::MAX),
        }
    }

    /// Without a ticking timer nothing would wake it, so it returns at once
    /// (nothing waits for ever).
    fn sleep(&mut self, ms: u64) {
        if ms == 0 || !timer::is_ticking() {
            return;
        }
        let until = timer::ticks().saturating_add(ms.saturating_mul(timer::TICK_HZ) / 1000);
        block(Blocked::Sleep(until));
    }

    fn output(&mut self, fd: u32, bytes: &[u8]) {
````

- [ ] **Step 6: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! and it answers with the result register's value or with the program's
//! exit. Plan 2 of milestone 2 serves `exit` and `write` to fds 1 and 2;
//! every other call is `ENOSYS` until the plan that brings it.

use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use relay_abi::{Call, encode};
use vfs::Errno;

/// What the dispatcher needs of the program that called: its memory and
/// where its output goes.
pub trait Caller {
    /// Copies `buf.len()` bytes from `offset` into `slice`; `EFAULT` if
    /// they are not all the program's.
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno>;
    /// What the program wrote to fd 1 or 2.
    fn output(&mut self, fd: u32, bytes: &[u8]);
}
````

with:

````rust
//! and it answers with the result register's value or with the program's
//! exit. It serves `exit`, `write` to fds 1 and 2, `time` and `sleep`;
//! every other call is `ENOSYS` until the plan that brings it.

use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use relay_abi::{Call, Time, encode};
use vfs::Errno;

/// What the dispatcher needs of the program that called: its memory,
/// where its output goes, and the kernel's services.
pub trait Caller {
    /// Copies `buf.len()` bytes from `offset` into `slice`; `EFAULT` if
    /// they are not all the program's.
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno>;
    /// Copies `bytes` into `slice` from `offset`; `EFAULT`, and nothing
    /// written, if they are not all the program's and writable.
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// What the program wrote to fd 1 or 2.
    fn output(&mut self, fd: u32, bytes: &[u8]);
    /// The wall clock and the uptime.
    fn time(&self) -> Time;
    /// Blocks the program for `ms` milliseconds.
    fn sleep(&mut self, ms: u64);
}
````

Replace:

````rust
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        _ => Err(Errno::ENOSYS),
````

with:

````rust
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Time) => time(caller, args[0]),
        Some(Call::Sleep) => {
            caller.sleep(args[0]);
            Ok(0)
        }
        _ => Err(Errno::ENOSYS),
````

Replace:

````rust
    Ok(done)
}
````

with:

````rust
    Ok(done)
}

/// `time(&mut Time)` (spec §7.3).
fn time(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let t = caller.time();
    let mut bytes = [0u8; size_of::<Time>()];
    bytes[..8].copy_from_slice(&t.unix_seconds.to_ne_bytes());
    bytes[8..].copy_from_slice(&t.uptime_ns.to_ne_bytes());
    caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
    Ok(0)
}
````

- [ ] **Step 7: Change `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
````

with:

````rust
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::time::Duration;
````

Replace:

````rust
static TSC_HZ: AtomicU64 = AtomicU64::new(0);

````

with:

````rust
static TSC_HZ: AtomicU64 = AtomicU64::new(0);
/// Whether the 1 kHz tick runs.
static TICKING: AtomicBool = AtomicBool::new(false);

````

Replace:

````rust
    x86_64::instructions::interrupts::enable();
    Ok(TimerInfo {
````

with:

````rust
    x86_64::instructions::interrupts::enable();
    TICKING.store(true, Ordering::Relaxed);
    Ok(TimerInfo {
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

/// Whether the 1 kHz tick runs (`init` succeeded).
pub fn is_ticking() -> bool {
    TICKING.load(Ordering::Relaxed)
}
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: PASS: 9 tests.

- [ ] **Step 9: Run the `programs` scenario**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates kernel
git commit -m "kernel: the time and sleep calls"
````


### Task 11: A sleep is never shorter than asked

Found by the prototype's review: `sleep(ms)` ended at `ticks() + ms`, and the next tick may come right after the call, so a sleep of 1 ms lasted 252 µs at the shortest in QEMU. `timer::sleep_until(now, ms)` is the tick after the one `ms` reaches (the ticks rounded up, and one more, saturating), a pure function with a host test (decision 6). Mutation check: without the extra tick the test fails.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/timer.rs`

**Interfaces:**
- Consumes: Task 10.
- Produces: `timer::sleep_until(now: u64, ms: u64) -> u64`.

- [ ] **Step 1: Add the failing tests to `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, replace:

````rust
    #[test]
    fn tsc_cycles_become_time() {
````

with:

````rust
    #[test]
    fn a_sleep_is_never_shorter_than_asked() {
        // At tick 100 the next tick may come at once: 1 ms needs 2 more.
        assert_eq!(sleep_until(100, 1), 102);
        assert_eq!(sleep_until(100, 20), 121);
        assert_eq!(sleep_until(0, 0), 1);
        assert_eq!(sleep_until(u64::MAX - 5, 10), u64::MAX, "no overflow");
        assert_eq!(sleep_until(7, u64::MAX), u64::MAX);
    }

    #[test]
    fn tsc_cycles_become_time() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib timer::`

Expected: FAIL: compile errors such as `` cannot find function `sleep_until` in this scope ``.

- [ ] **Step 3: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        }
        let until = timer::ticks().saturating_add(ms.saturating_mul(timer::TICK_HZ) / 1000);
        block(Blocked::Sleep(until));
    }
````

with:

````rust
        }
        block(Blocked::Sleep(timer::sleep_until(timer::ticks(), ms)));
    }
````

- [ ] **Step 4: Change `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, replace:

````rust

/// Timer ticks since the timer started (1 per millisecond).
````

with:

````rust

/// The tick a sleep of `ms` milliseconds that starts at tick `now` ends on:
/// the ticks `ms` takes, rounded up, and one more, since the next tick may
/// come right after `now`. So a sleep is never shorter than asked.
pub fn sleep_until(now: u64, ms: u64) -> u64 {
    let ticks = ms
        .checked_mul(TICK_HZ)
        .map_or(u64::MAX, |t| t.div_ceil(1000));
    now.saturating_add(ticks).saturating_add(1)
}

/// Timer ticks since the timer started (1 per millisecond).
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib timer::`

Expected: PASS: 10 tests.

- [ ] **Step 6: Run the `programs` scenario**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -m "kernel: a sleep is never shorter than asked"
````


### Task 12: Ticks that interrupt a program poll the keyboard and end its slice; `t-spin`

Spec §6.1 and §6.3: a program that never makes a system call must not keep the CPU, nor freeze the keyboard. `t-spin [SECONDS]` (§8.5) is such a program: it spins, reads the clock every 2^20 iterations, and prints how many it made. The new `ctrlc` scenario types a line on the USB keyboard while `t-spin 3` runs; before this task no one polls the keyboard then, the keyboard's own queue overflows, its releases are lost, and the key repeat fills the line with `eeee…` (the red run shows it). Now the tick handler, after its EOI, calls `proc::user_tick` when it interrupted ring 3: it polls the console (the kernel holds nothing then), wakes due sleepers and ends the slice when another process is ready; a tick that interrupts the kernel only counts (`kernel_tick`), and those ticks are settled when the program returns to ring 3 (`before_user`, from `syscall_dispatch`) or gives up the CPU (decision 6). The `system` scenario lists the third program. Mutation checks: `user_tick` without its poll fails `ctrlc`; `irq_common` without either of its `swapgs` fails `ctrlc` (Task 7's assertion, now that a program takes ticks).

**Files:**
- Modify: `kernel/src/arch/irq.rs`
- Modify: `kernel/src/arch/user.rs`
- Modify: `kernel/src/proc.rs`
- Create: `tests/e2e/ctrlc.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-spin.rs`

**Interfaces:**
- Consumes: Tasks 3 (`tick`, `wake_sleepers`), 8 (`tty::poll`), 9, 10 (`sys::time`).
- Produces: `proc::{user_tick(), kernel_tick(), before_user()}`; `userland/tests/src/bin/t-spin.rs`; the `ctrlc` scenario.

- [ ] **Step 1: Add the scenario `tests/e2e/ctrlc.txt`**

Create `tests/e2e/ctrlc.txt`:

````text
# Programs share the CPU (user-space gate §6.1, §6.3; milestone 2, plan 3a):
# t-spin never makes a system call, so only the timer takes the CPU from
# it, and every tick that interrupts it also polls the keyboard: what is
# typed on the USB keyboard while it spins is all there when the shell
# reads again, although the keyboard holds only a few keys itself.
timeout 30
expect root@relay:~# $
send t-spin 3
alive 1
key echo typed-while-it-spins
expect \n\d+ iterations\n
expect \ntyped-while-it-spins\n
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-spin\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-spin\n
````

- [ ] **Step 3: Add the test program `userland/tests/src/bin/t-spin.rs`**

Create `userland/tests/src/bin/t-spin.rs`:

````rust
//! `t-spin [SECONDS]`: spins without system calls (spec §8.5), for ever or
//! for `SECONDS` seconds, reading the clock only every 2^20 iterations,
//! then prints how many iterations it made. Nothing but the timer takes
//! the CPU away from it, and the keyboard must keep working meanwhile.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Iterations between two looks at the clock.
const CHECK_EVERY: u64 = 1 << 20;

fn main(args: Args) -> u8 {
    let limit = match args.get(1) {
        None => None,
        Some(s) => match parse(s) {
            Some(secs) => Some(secs.saturating_mul(1_000_000_000)),
            None => {
                let _ = sys::write_all(2, b"usage: t-spin [SECONDS]\n");
                return 2;
            }
        },
    };
    let Ok(start) = sys::time() else {
        let _ = sys::write_all(2, b"t-spin: no clock\n");
        return 1;
    };
    let mut n: u64 = 0;
    loop {
        n = core::hint::black_box(n + 1);
        if n.is_multiple_of(CHECK_EVERY)
            && let Some(limit) = limit
            && sys::time().is_ok_and(|t| t.uptime_ns - start.uptime_ns >= limit)
        {
            break;
        }
    }
    if writeln!(Fd(1), "{n} iterations").is_err() {
        return 1;
    }
    0
}

/// A decimal number of seconds.
fn parse(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0u64, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(u64::from(d))
    })
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: FAIL: scenario `ctrlc` stops at line 12, timed out waiting for `\ntyped-while-it-spins\n`.

- [ ] **Step 5: Change `kernel/src/arch/irq.rs`**

In `kernel/src/arch/irq.rs`, replace:

````rust
            TICKS.fetch_add(1, Ordering::Relaxed);
            super::lapic::eoi();
        }
        Action::Legacy { eoi_master, .. } => {
````

with:

````rust
            TICKS.fetch_add(1, Ordering::Relaxed);
            super::lapic::eoi();
            // After the EOI: the tick may switch to another process.
            if frame.cs & 3 == 3 {
                crate::proc::user_tick();
            } else {
                crate::proc::kernel_tick();
            }
        }
        Action::Legacy { eoi_master, .. } => {
````

- [ ] **Step 6: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, replace:

````rust
    frame.rax = crate::proc::system_call(frame.rax, args);
    if !is_canonical(frame.rip) {
````

with:

````rust
    frame.rax = crate::proc::system_call(frame.rax, args);
    crate::proc::before_user();
    if !is_canonical(frame.rip) {
````

- [ ] **Step 7: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let me = t.current();
        let save = save_slot(&t);
````

with:

````rust
        let me = t.current();
        if me != 0 {
            settle_ticks(&mut t, 0);
        } else {
            KERNEL_TICKS.store(0, Ordering::Relaxed);
        }
        let save = save_slot(&t);
````

Replace:

````rust
        interrupts::enable();
    }
````

with:

````rust
        interrupts::enable();
    }
}

/// Ticks that interrupted the kernel since they were last counted: the
/// kernel is not preemptible, so such a tick only counts here (spec §6.1's
/// `need_resched`), and the count is settled when the running process
/// returns to ring 3 or gives up the CPU.
static KERNEL_TICKS: AtomicU64 = AtomicU64::new(0);

/// Charges the ticks that interrupted the kernel (and `more`) to the
/// running process; whether its slice is used up while another process is
/// ready. Due sleepers are woken first, so a sleeper does not wait for the
/// idle task behind a process that never blocks.
fn settle_ticks(t: &mut Table<Res>, more: u64) -> bool {
    t.wake_sleepers(timer::ticks());
    let n = KERNEL_TICKS.swap(0, Ordering::Relaxed) + more;
    let mut used_up = false;
    for _ in 0..n {
        used_up |= t.tick();
    }
    used_up
}

/// A tick interrupted the kernel (spec §6.1): counted for later, nothing
/// else. The idle task's ticks are nobody's.
pub fn kernel_tick() {
    KERNEL_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// A tick interrupted a program (spec §6.1, §6.3): the kernel holds nothing
/// now, so the tick polls the console, and the program gives up the CPU if
/// its slice is used up.
pub fn user_tick() {
    tty::poll();
    let used_up = {
        let mut t = PROCS.lock();
        if tty::has_input() {
            t.wake_all(Blocked::Console);
        }
        settle_ticks(&mut t, 1)
    };
    if used_up {
        reschedule();
    }
}

/// On the way back to ring 3 from a system call: the ticks the call took
/// are counted, and the program gives up the CPU if its slice is used up.
pub fn before_user() {
    let used_up = settle_ticks(&mut PROCS.lock(), 0);
    if used_up {
        reschedule();
    }
````

- [ ] **Step 8: Run the `ctrlc`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: ticks that interrupt a program poll the keyboard and end its time slice; t-spin"
````


### Task 13: Milestone 1's interrupt findings: the LAPIC's EOI, storms per second, the PIC masked first

Three of milestone 1's deferred findings (roadmap), which matter now that the timer preempts programs and ring 3 raises exceptions (decision 14). A vector from 32 to 47 that the LAPIC delivered (the firmware may route a source there) got no LAPIC EOI, so every vector of its priority class and below stayed blocked: now `eoi_for` gives the LAPIC an EOI for any vector it has in service (`lapic::in_service`, its ISR bit), and the PIC's master its EOI for a spurious IRQ 15 as before. The interrupt-storm count never reset, so a stray interrupt now and then over days of uptime would end in a panic: now it counts per second (`Storm`, 1000 within `STORM_WINDOW` ticks). And `timer::init` returned before masking the PIC when it failed to find the TSC's frequency; since the first system call enables interrupts, an unmasked PIC would then deliver its timer on the firmware's vectors (0x08 is the double fault's): now `init_on` masks it first, which a host test sees through fake ports on the `NoSource` path. Mutation checks: a storm window that never resets, no LAPIC EOI for a delivered legacy vector and the PIC masked after the TSC search each fail a test.

**Files:**
- Modify: `kernel/src/arch/irq.rs`
- Modify: `kernel/src/arch/lapic.rs`
- Modify: `kernel/src/timer.rs`

**Interfaces:**
- Consumes: milestone 1's `arch::{irq, lapic, pic}`, `timer`.
- Produces: `arch::irq::{Eoi {lapic, pic_master}, eoi_for(Action, bool) -> Eoi, Storm, STORM_WINDOW, note_unexpected(&[Storm; 256], vector, now) -> bool}`; `arch::lapic::{in_service(&mut impl Regs, u8) -> bool, vector_in_service(u8) -> bool}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/irq.rs`**

In `kernel/src/arch/irq.rs`, replace:

````rust
    fn an_unexpected_vector_is_a_storm_at_the_limit() {
        let counts = [const { AtomicU32::new(0) }; 256];
        for _ in 1..STORM_LIMIT {
            assert!(!note_unexpected(&counts, 100));
        }
        assert!(note_unexpected(&counts, 100));
        assert!(!note_unexpected(&counts, 101), "counted per vector");
    }
````

with:

````rust
    fn an_unexpected_vector_is_a_storm_at_the_limit() {
        let storms = [const { Storm::new() }; 256];
        for _ in 1..STORM_LIMIT {
            assert!(!note_unexpected(&storms, 100, 5));
        }
        assert!(note_unexpected(&storms, 100, 999));
        assert!(!note_unexpected(&storms, 101, 999), "counted per vector");
    }

    #[test]
    fn a_stray_interrupt_now_and_then_is_no_storm() {
        let storms = [const { Storm::new() }; 256];
        // Fewer than the limit in each second, for a long time.
        for second in 0..10 {
            for _ in 1..STORM_LIMIT {
                assert!(!note_unexpected(&storms, 60, second * STORM_WINDOW + 7));
            }
        }
        // The limit within one second is a storm, whenever it comes.
        for _ in 1..STORM_LIMIT {
            note_unexpected(&storms, 61, 50_000);
        }
        assert!(note_unexpected(&storms, 61, 50_000 + STORM_WINDOW - 1));
    }

    #[test]
    fn the_lapic_gets_an_eoi_for_whatever_it_delivered() {
        let lapic = |a| eoi_for(a, true);
        let legacy = |irq| Action::Legacy {
            irq,
            eoi_master: irq == 15,
        };
        // A LAPIC-delivered source on the PIC's vectors.
        assert!(lapic(legacy(3)).lapic);
        assert!(!eoi_for(legacy(3), false).lapic, "a spurious PIC IRQ");
        assert_eq!(
            eoi_for(legacy(15), false),
            Eoi {
                lapic: false,
                pic_master: true
            }
        );
        assert!(lapic(Action::Unexpected).lapic);
        assert!(!eoi_for(Action::Unexpected, false).lapic);
        assert!(eoi_for(Action::Timer, false).lapic);
        assert!(!lapic(Action::Spurious).lapic);
        assert!(!lapic(Action::Timer).pic_master);
    }
````

- [ ] **Step 2: Add the failing tests to `kernel/src/arch/lapic.rs`**

In `kernel/src/arch/lapic.rs`, replace:

````rust
    #[test]
    fn base_msr_selects_the_mode() {
````

with:

````rust
    #[test]
    fn a_vector_is_in_service_by_its_isr_bit() {
        let mut r = Fake::default();
        r.isr[1] = 1 << 3; // vector 35
        r.isr[7] = 1 << 31; // vector 255
        assert!(in_service(&mut r, 35));
        assert!(in_service(&mut r, 255));
        for v in [0, 3, 34, 36, 67, 254] {
            assert!(!in_service(&mut r, v), "vector {v}");
        }
    }

    #[test]
    fn base_msr_selects_the_mode() {
````

- [ ] **Step 3: Add the failing tests to `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, replace:

````rust
    use super::*;

    #[test]
    fn a_sleep_is_never_shorter_than_asked() {
````

with:

````rust
    use super::*;

    #[derive(Default)]
    struct Ports(Vec<(u16, u8)>);

    impl pic::PortOut for Ports {
        fn outb(&mut self, port: u16, value: u8) {
            self.0.push((port, value));
        }
    }

    #[test]
    fn the_pic_is_masked_even_when_the_timer_cannot_start() {
        // No HPET to measure against, as on a machine whose ACPI tables
        // lack one: `init` fails before it touches the LAPIC.
        let mut io = Ports::default();
        assert!(matches!(
            init_on(&mut io, None, true),
            Err(TimerError::NoSource)
        ));
        assert!(io.0.contains(&(pic::MASTER_DATA, 0xFF)), "master masked");
        assert!(io.0.contains(&(pic::SLAVE_DATA, 0xFF)), "slave masked");
    }

    #[test]
    fn a_sleep_is_never_shorter_than_asked() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `STORM_WINDOW` in this scope ``; `` cannot find struct, variant or union type `Eoi` in this scope ``.

- [ ] **Step 5: Change `kernel/src/arch/irq.rs`**

In `kernel/src/arch/irq.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! interrupt the firmware left pending costs a count and an EOI instead of a
//! fault, and one that keeps firing (a source nobody masked) panics as an
//! interrupt storm, naming its vector.

````

with:

````rust
//! interrupt the firmware left pending costs a count and an EOI instead of a
//! fault, and one that keeps firing (a source nobody masked, [`STORM_LIMIT`]
//! times within a second) panics as an interrupt storm, naming its vector.
//! Whatever the vector, the LAPIC gets an EOI when it delivered it (its
//! in-service bit is set), so a source the firmware routed through the
//! LAPIC onto 32-47 does not block every vector below it.

````

Replace:

````rust
pub const SPURIOUS_VECTOR: u8 = 255;
/// An unexpected vector that fires this often has a live source.
pub const STORM_LIMIT: u32 = 1000;
/// Bytes per entry stub; each starts on a 16-byte boundary.
````

with:

````rust
pub const SPURIOUS_VECTOR: u8 = 255;
/// An unexpected vector that fires this often within [`STORM_WINDOW`] ticks
/// has a live source.
pub const STORM_LIMIT: u32 = 1000;
/// Timer ticks (a second) in which `STORM_LIMIT` interrupts are a storm.
pub const STORM_WINDOW: u64 = 1000;
/// Bytes per entry stub; each starts on a 16-byte boundary.
````

Replace:

````rust
    Unexpected,
}
````

with:

````rust
    Unexpected,
}

/// Which end-of-interrupt signals an interrupt needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eoi {
    pub lapic: bool,
    pub pic_master: bool,
}

/// The EOIs for `action`, given whether the LAPIC has its vector in
/// service. The timer is always the LAPIC's; the spurious vector never
/// gets one.
pub fn eoi_for(action: Action, in_service: bool) -> Eoi {
    match action {
        Action::Timer => Eoi {
            lapic: true,
            pic_master: false,
        },
        Action::Spurious => Eoi {
            lapic: false,
            pic_master: false,
        },
        Action::Legacy { eoi_master, .. } => Eoi {
            lapic: in_service,
            pic_master: eoi_master,
        },
        Action::Unexpected => Eoi {
            lapic: in_service,
            pic_master: false,
        },
    }
}
````

Replace:

````rust
pub static SPURIOUS: AtomicU64 = AtomicU64::new(0);
/// Interrupts per vector that nothing handles.
static UNEXPECTED: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

/// Counts an interrupt on a vector nothing handles; true once that vector
/// has fired `STORM_LIMIT` times.
pub fn note_unexpected(counts: &[AtomicU32; 256], vector: u8) -> bool {
    counts[vector as usize].fetch_add(1, Ordering::Relaxed) + 1 >= STORM_LIMIT
}
````

with:

````rust
pub static SPURIOUS: AtomicU64 = AtomicU64::new(0);
/// Interrupts on a vector nothing handles, in the window that started at
/// tick `since`.
pub struct Storm {
    since: AtomicU64,
    count: AtomicU32,
}

impl Storm {
    pub const fn new() -> Storm {
        Storm {
            since: AtomicU64::new(0),
            count: AtomicU32::new(0),
        }
    }
}

impl Default for Storm {
    fn default() -> Self {
        Storm::new()
    }
}

static UNEXPECTED: [Storm; 256] = [const { Storm::new() }; 256];

/// Counts an interrupt on a vector nothing handles at tick `now`; true once
/// that vector has fired `STORM_LIMIT` times within `STORM_WINDOW` ticks.
/// An older window starts again: a stray interrupt now and then over days
/// is no storm.
pub fn note_unexpected(storms: &[Storm; 256], vector: u8, now: u64) -> bool {
    let s = &storms[vector as usize];
    if now.saturating_sub(s.since.load(Ordering::Relaxed)) >= STORM_WINDOW {
        s.since.store(now, Ordering::Relaxed);
        s.count.store(0, Ordering::Relaxed);
    }
    s.count.fetch_add(1, Ordering::Relaxed) + 1 >= STORM_LIMIT
}
````

Replace:

````rust
    );
    match classify(frame.vector as u8) {
        Action::Timer => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            super::lapic::eoi();
            // After the EOI: the tick may switch to another process.
            if frame.cs & 3 == 3 {
                crate::proc::user_tick();
            } else {
                crate::proc::kernel_tick();
            }
        }
        Action::Legacy { eoi_master, .. } => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
            if eoi_master {
                super::pic::eoi_master(&mut super::pic::RealPorts);
            }
        }
        Action::Spurious => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
        }
        Action::Unexpected => {
            let vector = frame.vector as u8;
            if note_unexpected(&UNEXPECTED, vector) {
                panic!(
                    "interrupt storm: vector {vector} fired {STORM_LIMIT} times and nothing handles it"
                );
            }
            // Only LAPIC-delivered sources can raise these vectors.
            super::lapic::eoi();
        }
````

with:

````rust
    );
    let vector = frame.vector as u8;
    let action = classify(vector);
    match action {
        Action::Timer => {
            TICKS.fetch_add(1, Ordering::Relaxed);
        }
        Action::Legacy { .. } | Action::Spurious => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
        }
        Action::Unexpected => {
            if note_unexpected(&UNEXPECTED, vector, TICKS.load(Ordering::Relaxed)) {
                panic!(
                    "interrupt storm: vector {vector} fired {STORM_LIMIT} times in a second and nothing handles it"
                );
            }
        }
    }
    let in_service = action != Action::Timer
        && action != Action::Spurious
        && super::lapic::vector_in_service(vector);
    let eoi = eoi_for(action, in_service);
    if eoi.pic_master {
        super::pic::eoi_master(&mut super::pic::RealPorts);
    }
    if eoi.lapic {
        super::lapic::eoi();
    }
    // After the EOI: a tick may switch to another process.
    if action == Action::Timer {
        if frame.cs & 3 == 3 {
            crate::proc::user_tick();
        } else {
            crate::proc::kernel_tick();
        }
````

- [ ] **Step 6: Change `kernel/src/arch/lapic.rs`**

In `kernel/src/arch/lapic.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Starts the timer in periodic mode (divide by 16), interrupting on
````

with:

````rust

/// Whether `vector` is in service: the LAPIC delivered it and waits for its
/// EOI.
pub fn in_service(regs: &mut impl Regs, vector: u8) -> bool {
    let reg = ISR0 + 0x10 * u32::from(vector / 32);
    regs.read(reg) & (1 << (vector % 32)) != 0
}

/// Starts the timer in periodic mode (divide by 16), interrupting on
````

Replace:

````rust
pub static LAPIC: Once<Lapic> = Once::new();

````

with:

````rust
pub static LAPIC: Once<Lapic> = Once::new();

/// Whether the LAPIC has `vector` in service (false before the timer
/// driver has set it up).
pub fn vector_in_service(vector: u8) -> bool {
    LAPIC
        .get()
        .copied()
        .is_some_and(|mut l| in_service(&mut l, vector))
}

````

- [ ] **Step 7: Change `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Finds the TSC frequency, masks the legacy PIC, starts the LAPIC timer at
/// `TICK_HZ` and enables interrupts. `hpet` is the HPET base from ACPI;
/// `force_hpet` (cmdline `tsc=hpet`) measures against it even when CPUID
/// has the frequency.
pub fn init(hpet: Option<u64>, force_hpet: bool) -> Result<TimerInfo, TimerError> {
    let (tsc_hz, source) = match cpuid_tsc() {
````

with:

````rust

/// Masks the legacy PIC, finds the TSC frequency, starts the LAPIC timer at
/// `TICK_HZ` and enables interrupts. `hpet` is the HPET base from ACPI;
/// `force_hpet` (cmdline `tsc=hpet`) measures against it even when CPUID
/// has the frequency.
pub fn init(hpet: Option<u64>, force_hpet: bool) -> Result<TimerInfo, TimerError> {
    init_on(&mut pic::RealPorts, hpet, force_hpet)
}

/// `init`, with the PIC's ports `io`. The PIC is masked before anything can
/// fail: an unmasked PIC delivers its IRQs on the firmware's vectors (0x08
/// is the double fault's) as soon as anything enables interrupts, and the
/// first system call does.
fn init_on(
    io: &mut impl pic::PortOut,
    hpet: Option<u64>,
    force_hpet: bool,
) -> Result<TimerInfo, TimerError> {
    pic::remap_and_mask(io, irq::PIC_BASE);
    let (tsc_hz, source) = match cpuid_tsc() {
````

Replace:

````rust

    pic::remap_and_mask(&mut pic::RealPorts, irq::PIC_BASE);
    let mode = Lapic::detect();
````

with:

````rust

    let mode = Lapic::detect();
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 260 tests.

- [ ] **Step 9: Run the `boot`, `timer`, `ctrlc` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario timer`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel
git commit -m "kernel: the LAPIC's EOI for whatever it delivered, storms counted per second, and the PIC masked before the timer can fail"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 27 scenario(s) passed`.

````bash
git push -u origin m2p3a/sched
gh pr create --base main --head m2p3a/sched --title "Milestone 2, plan 3a: Processes share the CPU" --body-file - <<'EOF'
## What

Milestone 2, plan 3a, tasks 7–13: every way in from ring 3 gives the kernel its own `gs` and flags (the stubs `swapgs` by CPL and load the kernel's flags, so AC never reaches a handler); the mount table and the console's input are the kernel's; processes live on the process table with a context switch between their kernel stacks, the boot context is the idle task and the in-kernel shell process 1, and debug assertions check the flags, TSS `rsp0` and the locks at every switch; `time` and `sleep`, which is never shorter than asked; a tick that interrupts a program polls the keyboard and ends its slice (`t-spin`, the `ctrlc` scenario's first part); the LAPIC's EOI for whatever it delivered, storms counted per second, the PIC masked before the timer can fail.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed now: QEMU runs it under KVM on the NUC's CPU model; NUC check 3 runs it with PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3a/sched --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-sched
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Ctrl-C, and programs that start programs (Tasks 14–20)

Ctrl-C kills the command's process group; `spawn`, `wait`, `kill`, `getpid` and the memory figures from ring 3, with `write` through the fd table and `t-spawn`; four of the prototype review's findings, each after that task (a process killed before its first run, orphans filling the table, an orphan's output in the next command's, a test of Ctrl-C of a group blocked in the kernel); and a program sees its redirection's write error.

Branch `m2p3a/calls`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-calls`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3a/calls /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-calls origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-calls
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3a/sched` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3a/calls /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-calls m2p3a/sched`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3a/sched>` and re-run `cargo xtask ci` before pushing.

### Task 14: Ctrl-C kills the command's process group

Spec §6.4: in line mode, a Ctrl-C kills every process of the console's foreground group and drops what was typed before it; in raw mode it is input like any other byte. `tty.rs` gains the mode and the foreground group (decision 7); the in-kernel shell's `System::wait` gives the console to its command's group in line mode (`proc::give_console`) and takes it back after (`take_console`: raw mode, process 1's group), so at the prompt Ctrl-C still cancels the line and during a built-in it still stops the built-in, as in milestone 1. Wherever the console is polled (the idle task, a tick that interrupts a program), `console_input` kills the foreground group with `KILLED_CTRL_C` (Task 3's `kill`: marked, and woken if blocked); a killed program ends before its next instruction, on the tick's way back to ring 3 or a system call's (`end_if_killed`), and the kernel log says `pid <n> (<path>): killed: Ctrl-C`. A Ctrl-C typed before the command has started waits in the queue until the next poll in line mode, so it is the command's. The shell says only `^C` for a program killed by Ctrl-C, as for a built-in, with status 130, and a script stops there. The `ctrlc` scenario presses Ctrl-C during `t-spin` (which would otherwise never end), right after starting it, and at the prompt. Mutation checks: a tick that does not end a killed program and the console left in raw mode while the shell waits fail `ctrlc`; the shell's `killed (…)` words for a Ctrl-C fail a shell test.

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/tty.rs`
- Modify: `tests/e2e/ctrlc.txt`

**Interfaces:**
- Consumes: Tasks 1 (`KILLED_CTRL_C`, `INTERRUPTED`), 3 (`kill`), 8, 12.
- Produces: `tty::{set_line_mode(bool) -> bool, set_foreground(u32), ctrl_c() -> Option<u32>}`; `proc::{give_console(pid), take_console()}`; the shell's `^C` for a program.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_killed_program_is_reported() {
````

with:

````rust
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
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/ctrlc.txt`**

Replace the whole of `tests/e2e/ctrlc.txt` with:

````text
# Programs share the CPU and Ctrl-C stops them (user-space gate §6.1,
# §6.3, §6.4; milestone 2, plan 3a):
# t-spin never makes a system call, so only the timer takes the CPU from
# it, and every tick that interrupts it also polls the keyboard: what is
# typed on the USB keyboard while it spins is all there when the shell
# reads again, although the keyboard holds only a few keys itself.
timeout 30
expect root@relay:~# $
send t-spin 3
alive 1
key echo typed-while-it-spins
expect \n\d+ iterations\n
expect \ntyped-while-it-spins\n
# Ctrl-C on the keyboard kills the command's process group (spec §6.4):
# t-spin alone would never end. The shell says `^C`, as when Ctrl-C stops
# a built-in, and the kernel log names the process.
send t-spin
alive 1
key {ctrl-c}
expect \n\^C\nroot@relay:~# 
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: Ctrl-C\n
# A Ctrl-C typed before the command has even started is the command's too:
# it ends at once, whichever comes first.
send t-spin
key {ctrl-c}
expect \n\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
key echo nope{ctrl-c}
expect \^C\n
send echo still here
expect \nstill here\n
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_program_stopped_by_ctrl_c_says_only_so_and_ends_a_script`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: FAIL: scenario `ctrlc` stops at line 20, timed out waiting for `\n\^C\nroot@relay:~#`.

- [ ] **Step 4: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
                let (what, status) = killed::killed(&w);
                message = format!("{NAME}: {name}: {what}\n");
                status
````

with:

````rust
                let (what, status) = killed::killed(&w);
                // Ctrl-C says only `^C`, as for a built-in, and stops a
                // script (spec §6.4).
                message = if status == CANCELLED {
                    String::from("^C\n")
                } else {
                    format!("{NAME}: {name}: {what}\n")
                };
                status
````

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// A tick interrupted a program (spec §6.1, §6.3): the kernel holds nothing
/// now, so the tick polls the console, and the program gives up the CPU if
/// its slice is used up.
pub fn user_tick() {
    tty::poll();
    let used_up = {
        let mut t = PROCS.lock();
        if tty::has_input() {
            t.wake_all(Blocked::Console);
        }
        settle_ticks(&mut t, 1)
````

with:

````rust

/// What the console's input asks of the processes: a Ctrl-C in line mode
/// kills the foreground group (spec §6.4), and anything typed wakes whoever
/// waits for input.
fn console_input(t: &mut Table<Res>) {
    if let Some(pgid) = tty::ctrl_c() {
        // Refused only for process 1's group, which never has the console
        // in line mode.
        let _ = t.kill(-i64::from(pgid), relay_abi::wait::KILLED_CTRL_C);
    }
    if tty::has_input() {
        t.wake_all(Blocked::Console);
    }
}

/// A tick interrupted a program (spec §6.1, §6.3): the kernel holds nothing
/// now, so the tick polls the console, and the program gives up the CPU if
/// its slice is used up, or ends if it was killed.
pub fn user_tick() {
    tty::poll();
    let used_up = {
        let mut t = PROCS.lock();
        console_input(&mut t);
        settle_ticks(&mut t, 1)
````

Replace:

````rust
    }
}

/// On the way back to ring 3 from a system call: the ticks the call took
/// are counted, and the program gives up the CPU if its slice is used up.
pub fn before_user() {
````

with:

````rust
    }
    end_if_killed();
}

/// On the way back to ring 3 from a system call: the ticks the call took
/// are counted, the program gives up the CPU if its slice is used up, and
/// it ends if it was killed meanwhile.
pub fn before_user() {
````

Replace:

````rust
        reschedule();
    }
}

````

with:

````rust
        reschedule();
    }
    end_if_killed();
}

/// Ends the running process if Ctrl-C or `kill` marked it (spec §11.1):
/// before it runs another instruction of its program.
fn end_if_killed() {
    let killed = {
        let t = PROCS.lock();
        t.get(t.current()).and_then(|p| p.killed)
    };
    if let Some(reason) = killed {
        let status = WaitStatus::killed(reason);
        {
            let t = PROCS.lock();
            let me = t.current();
            let name = t.get(me).map_or("?", |p| p.name.as_str());
            klogln!("pid {me} ({name}): killed: {status}");
        }
        end(status);
    }
}

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

/// Gives the console back to the in-kernel shell: its own group, raw mode.
pub fn take_console() {
    tty::set_line_mode(false);
    tty::set_foreground(table::INIT);
}

````

Replace:

````rust
        tty::poll();
        if tty::has_input() {
            PROCS.lock().wake_all(Blocked::Console);
        }
        usb::service();
````

with:

````rust
        tty::poll();
        console_input(&mut PROCS.lock());
        usb::service();
````

- [ ] **Step 6: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust

    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        proc::wait(pid, out)
    }
````

with:

````rust

    /// The command has the console, in line mode, while the shell waits
    /// for it (spec §6.4): a Ctrl-C kills it.
    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        proc::give_console(pid);
        let ended = proc::wait(pid, out);
        proc::take_console();
        ended
    }
````

- [ ] **Step 7: Change `kernel/src/tty.rs`**

Replace the whole of `kernel/src/tty.rs` with:

````rust
//! The console's input (user-space gate §6.3–§6.5): one queue for every
//! source, the USB keyboards and COM1, filled by `poll`. `poll` never waits,
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program, and whenever the in-kernel shell
//! reads or asks whether Ctrl-C was pressed.
//!
//! The console has a foreground process group and a mode (spec §6.4). In
//! raw mode a Ctrl-C is input like any other byte (the shell's line editor
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`). Plan 3b adds reading in line mode.

use crate::input::InputQueue;
use crate::{serial, usb};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

/// Line mode (true) or raw mode.
static LINE_MODE: AtomicBool = AtomicBool::new(false);
/// The foreground process group; process 1's at first.
static FOREGROUND: AtomicU32 = AtomicU32::new(1);

/// The console's mode: raw (`false`) or line (`true`); the previous one.
pub fn set_line_mode(line: bool) -> bool {
    LINE_MODE.swap(line, Ordering::Relaxed)
}

/// Makes `pgid` the console's foreground group.
pub fn set_foreground(pgid: u32) {
    FOREGROUND.store(pgid, Ordering::Relaxed);
}

/// The foreground group a Ctrl-C typed in line mode is for, if one is
/// waiting: it and what was typed before it are dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    if !LINE_MODE.load(Ordering::Relaxed) {
        return None;
    }
    take_interrupt().then(|| FOREGROUND.load(Ordering::Relaxed))
}

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Moves whatever the input devices have into the queue. Never waits.
pub fn poll() {
    let mut input = INPUT.lock();
    usb::poll(&mut input);
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b),
            None => break,
        }
    }
}

/// The oldest byte typed.
pub fn pop() -> Option<u8> {
    INPUT.lock().pop()
}

/// Whether anything typed waits to be read.
pub fn has_input() -> bool {
    !INPUT.lock().is_empty()
}

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
/// dropped (`InputQueue::take_interrupt`).
pub fn take_interrupt() -> bool {
    INPUT.lock().take_interrupt()
}

/// Whether the input queue is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    INPUT.is_locked()
}
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 127 tests.

- [ ] **Step 9: Run the `ctrlc` scenario**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates kernel tests
git commit -m "kernel: Ctrl-C kills the command's process group, which has the console in line mode while the shell waits"
````


### Task 15: Programs start programs: spawn, wait, kill, getpid and the memory figures from ring 3; `t-spawn`

Spec §7.3's process calls from ring 3 (decision 10). The dispatcher copies in and checks what the program passes, and leaves the rest to its `Caller`: `spawn(&SpawnArgs)` reads the struct, then the path (at most 4096 bytes, `ENAMETOOLONG`), the arguments (at most 64 KiB, `E2BIG`; at least argument 0 and every one ending in its NUL, `EINVAL`) and the working directory, and refuses more than 8 fd pairs or an unknown flag (`EINVAL`); `wait(pid or −1, NOHANG, &mut WaitStatus or 0)` checks that the status's memory is writable before a child is collected, so a bad pointer loses no child, and refuses pid 0, a pid below −1 (`EINVAL`) and one above 2^32 (`ECHILD`); `kill`, `getpid`, and `sys_info`'s memory kind (a shorter buffer or another kind is `EINVAL` until plan 3b). `write` now goes through the fd table: `EBADF` for an fd that is not open, even for 0 bytes, and a file's error reaches the program. In the kernel one `proc::spawn(&Spawn)` serves the in-kernel shell and ring 3: fds and working directory checked first, the program read from the caller's current directory; `collect` waits for a child (`EINTR` when the waiter is killed, so it ends on its way back); the in-kernel shell collects process 1's orphans after each program (decision 11). `relay-rt` gains `sys::{spawn, wait, kill, getpid, memory}`, and `t-spawn` (§8.5) uses them: `t-spawn N` starts N children and prints the free frames before and after; `t-spawn kill` kills a spinning `t-spin` while it sleeps (Task 10's sleepers, woken by the spinner's ticks), then shows `kill 1` refused and a pid nobody has; `t-spawn orphan` leaves a `t-spin 1` behind. The `spawn` scenario runs all three (in the red run `t-spawn` gets `ENOSYS`), and `system` lists four programs. `EINTR` joins `vfs::Errno`. Mutation checks: `wait` without its status check, arguments without their final NUL accepted, `write` without its fd check, a pid above 2^32 truncated (a test that first survived with 2^32 and was made to use 2^32 + 7) and an unknown flag accepted each fail a test; the shell not collecting orphans and a switch only away from the idle task fail `spawn`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `crates/vfs/src/errno.rs`
- Modify: `kernel/src/mm/user.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/syscall.rs`
- Create: `tests/e2e/spawn.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `userland/tests/Cargo.toml`
- Create: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: Tasks 1, 2 (`UserStr`), 3, 5, 9, 10, 14.
- Produces: `syscall::{PATH_MAX, Spawn {path, args, argc, cwd, fds, new_group}, Child::{Any, Pid(u32)}}`, `Caller::{writable, read_str, writable_fd, output(fd: u64, &[u8]) -> Result<(), Errno>, spawn, wait, kill, pid, memory}`; `proc::spawn(&Spawn) -> Result<u32, Errno>`; `UserSlice::check_writable`; `vfs::Errno::EINTR`; `relay_rt::sys::{spawn, wait, kill, getpid, memory}`; `userland/tests/src/bin/t-spawn.rs`; the `spawn` scenario.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use relay_abi::{Call, decode};

````

with:

````rust
use core::fmt;
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, WaitStatus, decode};

````

Replace:

````rust

/// The wall clock and the time since the machine started.
````

with:

````rust

/// Starts the program at `path` with `args` (each followed by a NUL,
/// argument 0 first) in `cwd` (empty: this program's), giving it the fds
/// `fds` names (child, parent) and closing its others; with `NEW_GROUP` in
/// `flags` it starts a process group of its own. Its pid.
pub fn spawn(path: &[u8], args: &[u8], cwd: &[u8], fds: &[FdMap], flags: u32) -> Result<u32, u16> {
    if fds.len() > SPAWN_FDS {
        return Err(relay_abi::errno::EINVAL);
    }
    let mut a = SpawnArgs {
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        args: args.as_ptr() as u64,
        args_len: args.len() as u64,
        cwd: cwd.as_ptr() as u64,
        cwd_len: cwd.len() as u64,
        fd_count: fds.len() as u32,
        flags,
        ..SpawnArgs::default()
    };
    a.fds[..fds.len()].copy_from_slice(fds);
    let r = unsafe { syscall(Call::Spawn, [&raw const a as u64, 0, 0, 0, 0, 0]) };
    decode(r).map(|pid| pid as u32)
}

/// Waits for the child `pid` (or any, `relay_abi::spawn::WAIT_ANY`) to
/// end: its pid and how it ended. With `nohang`, `None` at once if none
/// has.
pub fn wait(pid: i64, nohang: bool) -> Result<Option<(u32, WaitStatus)>, u16> {
    let mut w = WaitStatus::default();
    let flags = if nohang { WAIT_NOHANG } else { 0 };
    let args = [pid as u64, u64::from(flags), &raw mut w as u64, 0, 0, 0];
    match decode(unsafe { syscall(Call::Wait, args) })? {
        0 => Ok(None),
        pid => Ok(Some((pid as u32, w))),
    }
}

/// Kills the process `target`, or the process group `-target`.
pub fn kill(target: i64) -> Result<(), u16> {
    decode(unsafe { syscall(Call::Kill, [target as u64, 0, 0, 0, 0, 0]) }).map(|_| ())
}

/// This program's pid.
pub fn getpid() -> u32 {
    unsafe { syscall(Call::Getpid, [0; 6]) as u32 }
}

/// The memory figures of `free`.
pub fn memory() -> Result<MemInfo, u16> {
    let mut m = MemInfo::default();
    let len = core::mem::size_of::<MemInfo>() as u64;
    let kind = u64::from(relay_abi::info::INFO_MEMORY);
    let args = [kind, &raw mut m as u64, len, 0, 0, 0];
    decode(unsafe { syscall(Call::SysInfo, args) })?;
    Ok(m)
}

/// The wall clock and the time since the machine started.
````

- [ ] **Step 2: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 23] = [
        Errno::ENOENT,
````

with:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 24] = [
        Errno::ENOENT,
````

Replace:

````rust
        Errno::EPERM,
    ];
````

with:

````rust
        Errno::EPERM,
        Errno::EINTR,
    ];
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall.rs`**

Replace the whole of `kernel/src/syscall.rs` with:

````rust
//! The system-call dispatcher (user-space gate §7), architecture-neutral:
//! the `arch` entry stub hands it the call number and the six arguments,
//! and it answers with the result register's value or with the program's
//! exit. It serves `exit`, `write` to fds 1 and 2, `time` and `sleep`;
//! every other call is `ENOSYS` until the plan that brings it.

use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use relay_abi::{Call, Time, encode};
use vfs::Errno;

/// What the dispatcher needs of the program that called: its memory,
/// where its output goes, and the kernel's services.
pub trait Caller {
    /// Copies `buf.len()` bytes from `offset` into `slice`; `EFAULT` if
    /// they are not all the program's.
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno>;
    /// Copies `bytes` into `slice` from `offset`; `EFAULT`, and nothing
    /// written, if they are not all the program's and writable.
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// What the program wrote to fd 1 or 2.
    fn output(&mut self, fd: u32, bytes: &[u8]);
    /// The wall clock and the uptime.
    fn time(&self) -> Time;
    /// Blocks the program for `ms` milliseconds.
    fn sleep(&mut self, ms: u64);
}

/// How a call ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Back to the program with this result register.
    Return(u64),
    /// The program called `exit` with this code.
    Exit(u8),
}

/// Serves call `number` with `args` for `caller`.
pub fn dispatch(caller: &mut impl Caller, number: u64, args: [u64; 6]) -> Outcome {
    let result = match Call::from_number(number) {
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Time) => time(caller, args[0]),
        Some(Call::Sleep) => {
            caller.sleep(args[0]);
            Ok(0)
        }
        _ => Err(Errno::ENOSYS),
    };
    Outcome::Return(encode(result.map_err(Errno::number)))
}

/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out a page at a time; a page that is not the program's ends the
/// call with the bytes written before it, or with `EFAULT` if there were
/// none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let fd = match fd {
        1 | 2 => fd as u32,
        _ => return Err(Errno::EBADF),
    };
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        // To the end of the page, so a bad page costs only its own bytes.
        // `UserSlice::new` checked that `addr + len` does not overflow.
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        caller.output(fd, &buf[..n]);
        done += n as u64;
    }
    Ok(done)
}

/// `time(&mut Time)` (spec §7.3).
fn time(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let t = caller.time();
    let mut bytes = [0u8; size_of::<Time>()];
    bytes[..8].copy_from_slice(&t.unix_seconds.to_ne_bytes());
    bytes[8..].copy_from_slice(&t.uptime_ns.to_ne_bytes());
    caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::{PAGE, PageTables, Perm};
    use crate::mm::space::AddressSpace;
    use crate::mm::testing::FakeMem;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE};
    use relay_abi::{decode, errno};

    const U: u64 = 0x40_0000;
    /// The writable page.
    const W: u64 = U + 3 * PAGE;
    /// An fd whose file fails every write (a full disk).
    const FULL: u64 = 7;

    /// What the fake clock says.
    const NOW: Time = Time {
        unix_seconds: 1_790_000_000,
        uptime_ns: 12_345_678_901,
    };

    const MEM: MemInfo = MemInfo {
        ram_total: 16 << 30,
        ram_free: 15 << 30,
        heap_total: 32 << 20,
        heap_used: 1 << 20,
    };

    /// A program with three readable pages at `U` holding a pattern, a
    /// writable page after them and nothing after that; fds 0-2 and a full
    /// disk as `FULL`; what it wrote, started, killed, and how long it
    /// slept.
    struct Fake {
        mem: FakeMem,
        space: AddressSpace,
        written: Vec<(u64, Vec<u8>)>,
        slept: Vec<u64>,
        spawned: Vec<Spawn>,
        /// Children that have ended, and whether any still runs.
        ended: Vec<(u32, WaitStatus)>,
        running: bool,
        killed: Vec<i64>,
    }

    impl Caller for Fake {
        fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
            slice.read(&self.space, &mut self.mem, offset, buf)
        }
        fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
            slice.write(&self.space, &mut self.mem, offset, bytes)
        }
        fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno> {
            // Writing what is there already writes nothing new.
            let mut old = alloc::vec![0; slice.len() as usize];
            slice.read(&self.space, &mut self.mem, 0, &mut old)?;
            slice.write(&self.space, &mut self.mem, 0, &old)
        }
        fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno> {
            s.read(&self.space, &mut self.mem)
        }
        fn writable_fd(&mut self, fd: u64) -> Result<(), Errno> {
            match fd {
                0..=2 | FULL => Ok(()),
                _ => Err(Errno::EBADF),
            }
        }
        fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
            if fd == FULL {
                return Err(Errno::ENOSPC);
            }
            self.written.push((fd, bytes.to_vec()));
            Ok(())
        }
        fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno> {
            if s.path == b"missing" {
                return Err(Errno::ENOENT);
            }
            self.spawned.push(s.clone());
            Ok(100 + self.spawned.len() as u32)
        }
        fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
            let at = self
                .ended
                .iter()
                .position(|&(pid, _)| child == Child::Any || child == Child::Pid(pid));
            match at {
                Some(i) => Ok(Some(self.ended.remove(i))),
                None if self.running && nohang => Ok(None),
                None => Err(Errno::ECHILD),
            }
        }
        fn kill(&mut self, target: i64) -> Result<(), Errno> {
            self.killed.push(target);
            if target == 1 {
                Err(Errno::EPERM)
            } else {
                Ok(())
            }
        }
        fn pid(&self) -> u32 {
            42
        }
        fn memory(&self) -> MemInfo {
            MEM
        }
        fn time(&self) -> Time {
            NOW
        }
        fn sleep(&mut self, ms: u64) {
            self.slept.push(ms);
        }
    }

    fn fake() -> Fake {
        let mut mem = FakeMem::new();
        let mut k = PageTables::new(&mut mem).unwrap();
        k.fill_upper_half(&mut mem).unwrap();
        let mut space = AddressSpace::new(&mut mem, &k).unwrap();
        space.map_zeroed(&mut mem, U, 3, Perm::Read).unwrap();
        let pattern: Vec<u8> = (0..3 * PAGE).map(|i| (i % 251) as u8).collect();
        space.fill(&mut mem, U, &pattern).unwrap();
        space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
        Fake {
            mem,
            space,
            written: Vec::new(),
            slept: Vec::new(),
            spawned: Vec::new(),
            ended: Vec::new(),
            running: false,
            killed: Vec::new(),
        }
    }

    fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
        match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
            Outcome::Return(r) => decode(r),
            Outcome::Exit(code) => panic!("exited with {code}"),
        }
    }

    /// Everything written, joined, per fd.
    fn text(f: &Fake, fd: u64) -> Vec<u8> {
        f.written
            .iter()
            .filter(|(d, _)| *d == fd)
            .flat_map(|(_, b)| b.clone())
            .collect()
    }

    /// Puts `bytes` into the writable page at `at`, as the program would.
    fn put(f: &mut Fake, at: u64, bytes: &[u8]) {
        UserSlice::new(at, bytes.len() as u64)
            .unwrap()
            .write(&f.space, &mut f.mem, 0, bytes)
            .unwrap();
    }

    fn get(f: &mut Fake, at: u64, len: usize) -> Vec<u8> {
        let mut buf = alloc::vec![0; len];
        UserSlice::new(at, len as u64)
            .unwrap()
            .read(&f.space, &mut f.mem, 0, &mut buf)
            .unwrap();
        buf
    }

    /// A `SpawnArgs` at `W` naming a path, arguments and a working
    /// directory stored after it; `edit` changes it first.
    fn spawn_args(f: &mut Fake, args: &[u8], edit: impl FnOnce(&mut SpawnArgs)) -> u64 {
        let (path, cwd) = (b"/bin/t-args", b"sub");
        put(f, W + 200, path);
        put(f, W + 300, cwd);
        put(f, W + 400, args);
        let mut fds = [FdMap::default(); SPAWN_FDS];
        fds[0] = FdMap {
            child: 1,
            parent: 2,
        };
        fds[1] = FdMap {
            child: 2,
            parent: 2,
        };
        let mut a = SpawnArgs {
            path: W + 200,
            path_len: path.len() as u64,
            args: W + 400,
            args_len: args.len() as u64,
            cwd: W + 300,
            cwd_len: cwd.len() as u64,
            fds,
            fd_count: 2,
            flags: NEW_GROUP,
        };
        edit(&mut a);
        // SAFETY: `SpawnArgs` is `repr(C)` of integers with no padding.
        let bytes: [u8; SpawnArgs::SIZE] = unsafe { core::mem::transmute(a) };
        put(f, W, &bytes);
        W
    }

    #[test]
    fn exit_ends_the_program_with_its_code() {
        let mut f = fake();
        assert_eq!(dispatch(&mut f, 1, [7, 0, 0, 0, 0, 0]), Outcome::Exit(7));
        // The code is a byte.
        assert_eq!(
            dispatch(&mut f, 1, [0x1_02, 0, 0, 0, 0, 0]),
            Outcome::Exit(2)
        );
    }

    #[test]
    fn write_copies_the_buffer_to_the_fd() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [1, U + 10, 5]), Ok(5));
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
            call(&mut f, Call::Write, [1, 0, 0]),
            Ok(0),
            "nothing, anywhere"
        );
    }

    #[test]
    fn an_fd_that_is_not_open_is_ebadf() {
        let mut f = fake();
        for fd in [3, 31, 1 << 32 | 1, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 1]),
                Err(errno::EBADF),
                "{fd}"
            );
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 0]),
                Err(errno::EBADF),
                "{fd}, nothing to write"
            );
        }
        assert!(f.written.is_empty());
    }

    #[test]
    fn a_file_s_error_reaches_the_program() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [FULL, U, 10]), Err(errno::ENOSPC));
    }

    #[test]
    fn a_bad_buffer_is_efault_or_a_short_write() {
        let mut f = fake();
        let end = U + 4 * PAGE;
        assert_eq!(
            call(&mut f, Call::Write, [1, 0, 1]),
            Err(errno::EFAULT),
            "null"
        );
        assert_eq!(call(&mut f, Call::Write, [1, end, 1]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Write, [1, 0xFFFF_FFFF_8000_0000, 8]),
            Err(errno::EFAULT),
            "the kernel"
        );
        assert_eq!(
            call(&mut f, Call::Write, [1, U, u64::MAX]),
            Err(errno::EFAULT)
        );
        assert!(f.written.is_empty());
        // Every byte before the hole is written, as on Linux.
        assert_eq!(call(&mut f, Call::Write, [1, end - 5000, 9000]), Ok(5000));
        let want: Vec<u8> = (4 * PAGE - 5000..3 * PAGE)
            .map(|i| (i % 251) as u8)
            .chain([0; PAGE as usize])
            .collect();
        assert_eq!(text(&f, 1), want);
        f.written.clear();
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 904, 2000]),
            Ok(904),
            "less than a page before the hole"
        );
        assert_eq!(text(&f, 1).len(), 904);
    }

    #[test]
    fn spawn_copies_in_everything_its_struct_names() {
        let mut f = fake();
        let a = spawn_args(&mut f, b"t-args\0a\0\0", |_| {});
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(101));
        assert_eq!(
            f.spawned,
            [Spawn {
                path: b"/bin/t-args".to_vec(),
                args: b"t-args\0a\0\0".to_vec(),
                argc: 3,
                cwd: b"sub".to_vec(),
                fds: vec![
                    FdMap {
                        child: 1,
                        parent: 2
                    },
                    FdMap {
                        child: 2,
                        parent: 2
                    }
                ],
                new_group: true,
            }]
        );
        let a = spawn_args(&mut f, b"x\0", |a| {
            a.flags = 0;
            a.cwd_len = 0;
            a.fd_count = 0;
        });
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Ok(102));
        let s = &f.spawned[1];
        assert!(!s.new_group && s.cwd.is_empty() && s.fds.is_empty());
        assert_eq!(s.argc, 1);
        // The caller's refusal.
        let a = spawn_args(&mut f, b"x\0", |a| a.path_len = 7);
        put(&mut f, W + 200, b"missing");
        assert_eq!(call(&mut f, Call::Spawn, [a, 0, 0]), Err(errno::ENOENT));
    }

    #[test]
    fn spawn_refuses_what_is_not_a_valid_request() {
        let mut f = fake();
        let refused = |f: &mut Fake, args: &[u8], edit: fn(&mut SpawnArgs)| {
            let a = spawn_args(f, args, edit);
            call(f, Call::Spawn, [a, 0, 0])
        };
        assert_eq!(refused(&mut f, b"x", |_| {}), Err(errno::EINVAL), "no NUL");
        assert_eq!(
            refused(&mut f, b"", |_| {}),
            Err(errno::EINVAL),
            "no argument 0"
        );
        assert_eq!(refused(&mut f, b"x\0", |a| a.flags = 2), Err(errno::EINVAL));
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.fd_count = 9),
            Err(errno::EINVAL)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.path_len = PATH_MAX as u64 + 1),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.cwd_len = PATH_MAX as u64 + 1),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.args_len = ARGS_MAX as u64 + 1),
            Err(errno::E2BIG)
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.args = 0),
            Err(errno::EFAULT),
            "arguments at null"
        );
        assert_eq!(
            refused(&mut f, b"x\0", |a| a.path = u64::MAX),
            Err(errno::EFAULT)
        );
        assert_eq!(call(&mut f, Call::Spawn, [0, 0, 0]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Spawn, [W + PAGE - 8, 0, 0]),
            Err(errno::EFAULT),
            "the struct runs off the page"
        );
        assert!(f.spawned.is_empty());
    }

    #[test]
    fn wait_copies_out_how_the_child_ended() {
        let mut f = fake();
        let fault = WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0x10, 0x40_1a2c);
        f.ended = vec![(7, WaitStatus::exited(3)), (8, fault)];
        assert_eq!(call(&mut f, Call::Wait, [8, 0, W + 64]), Ok(8));
        let b = get(&mut f, W + 64, 32);
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        assert_eq!(
            (
                u32_at(0),
                u32_at(4),
                u32_at(8),
                u32_at(12),
                u64_at(16),
                u64_at(24)
            ),
            (
                fault.how,
                fault.code,
                fault.fault,
                fault.detail,
                0x10,
                0x40_1a2c
            )
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-1i64 as u64, 0, 0]),
            Ok(7),
            "any, no status"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-1i64 as u64, 0, 0]),
            Err(errno::ECHILD)
        );
        f.running = true;
        assert_eq!(
            call(
                &mut f,
                Call::Wait,
                [-1i64 as u64, u64::from(WAIT_NOHANG), W]
            ),
            Ok(0),
            "nothing has ended"
        );
    }

    #[test]
    fn a_bad_status_pointer_loses_no_child() {
        let mut f = fake();
        f.ended = vec![(7, WaitStatus::exited(3))];
        for bad in [U, W + PAGE - 16, 0xFFFF_8000_0000_0000, u64::MAX - 8] {
            assert_eq!(
                call(&mut f, Call::Wait, [7, 0, bad]),
                Err(errno::EFAULT),
                "{bad:#x}"
            );
        }
        assert_eq!(f.ended.len(), 1, "still there");
        assert_eq!(call(&mut f, Call::Wait, [7, 0, W]), Ok(7));
    }

    #[test]
    fn wait_refuses_what_it_cannot_mean() {
        let mut f = fake();
        f.ended = vec![(7, WaitStatus::exited(3))];
        assert_eq!(
            call(&mut f, Call::Wait, [0, 0, 0]),
            Err(errno::EINVAL),
            "a group"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [-2i64 as u64, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::Wait, [7, 2, 0]),
            Err(errno::EINVAL),
            "flags"
        );
        assert_eq!(
            call(&mut f, Call::Wait, [(1 << 32) + 7, 0, 0]),
            Err(errno::ECHILD),
            "no such pid, and not 7"
        );
        assert_eq!(f.ended.len(), 1);
    }

    #[test]
    fn kill_getpid_and_the_memory_figures() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Kill, [5, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Kill, [-5i64 as u64, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Kill, [1, 0, 0]), Err(errno::EPERM));
        assert_eq!(f.killed, [5, -5, 1]);
        assert_eq!(call(&mut f, Call::Getpid, [0, 0, 0]), Ok(42));
        let info = u64::from(INFO_MEMORY);
        assert_eq!(call(&mut f, Call::SysInfo, [info, W, 32]), Ok(32));
        let b = get(&mut f, W, 32);
        let at = |i: usize| u64::from_ne_bytes(b[8 * i..8 * i + 8].try_into().unwrap());
        assert_eq!(
            [at(0), at(1), at(2), at(3)],
            [MEM.ram_total, MEM.ram_free, MEM.heap_total, MEM.heap_used]
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [info, W, 31]),
            Err(errno::EINVAL)
        );
        assert_eq!(call(&mut f, Call::SysInfo, [2, W, 32]), Err(errno::EINVAL));
        assert_eq!(
            call(&mut f, Call::SysInfo, [info, U, 32]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn time_fills_in_the_clock_s_answer() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Time, [W + 8, 0, 0]), Ok(0));
        let buf = get(&mut f, W + 8, 16);
        assert_eq!(buf[..8], NOW.unix_seconds.to_ne_bytes());
        assert_eq!(buf[8..], NOW.uptime_ns.to_ne_bytes());
        for bad in [0, U, W + PAGE - 8, u64::MAX - 4] {
            assert_eq!(
                call(&mut f, Call::Time, [bad, 0, 0]),
                Err(errno::EFAULT),
                "{bad:#x}"
            );
        }
    }

    #[test]
    fn sleep_blocks_for_what_it_is_asked() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Sleep, [250, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Sleep, [0, 0, 0]), Ok(0));
        assert_eq!(f.slept, [250, 0]);
    }

    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        let served = [
            Call::Exit,
            Call::Spawn,
            Call::Wait,
            Call::Kill,
            Call::Getpid,
            Call::Write,
            Call::Time,
            Call::Sleep,
            Call::SysInfo,
        ];
        for c in Call::ALL {
            if !served.contains(&c) {
                assert_eq!(call(&mut f, c, [1, U, 1]), Err(errno::ENOSYS), "{c:?}");
            }
        }
        for n in [0, 38, 1000, u64::MAX] {
            assert_eq!(
                dispatch(&mut f, n, [0; 6]),
                Outcome::Return(encode(Err(errno::ENOSYS))),
                "{n}"
            );
        }
        assert!(f.written.is_empty() && f.spawned.is_empty() && f.killed.is_empty());
    }
}
````

- [ ] **Step 4: Add the scenario `tests/e2e/spawn.txt`**

Create `tests/e2e/spawn.txt`:

````text
# Programs start programs (user-space gate §5.4, §7.3, §12.2; milestone 2,
# plan 3a): a thousand children that exit at once leave the free frames as
# they were, `kill` ends a program that never makes a system call while
# its parent sleeps, process 1 cannot be killed, and an orphan passes to
# process 1, which collects it.
timeout 60
expect root@relay:~# $
send t-args warm-up
expect \n\[1\] warm-up\n
send t-spawn 1000
expect-same frames \nfree frames before: (\d+)\n
expect-same frames free frames after: (\d+)\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-args a 'b c' ''
expect \n\[1\] a\n\[2\] b c\n\[3\] \nroot@relay:~# $
send t-spawn kill
expect \nt-spin: kill\nkill 1: EPERM\nkill 999999: ESRCH\npid above 1: true\n
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
# The orphan prints when its second is up, over the prompt.
send t-spawn orphan
expect # \d+ iterations\n
send t-args after-the-orphan
expect \n\[1\] after-the-orphan\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-spin\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-spawn  t-spin\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-spin\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-spawn  t-spin\n
````

- [ ] **Step 6: Change the test program `userland/tests/Cargo.toml`**

In `userland/tests/Cargo.toml`, replace:

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

- [ ] **Step 7: Add the test program `userland/tests/src/bin/t-spawn.rs`**

Create `userland/tests/src/bin/t-spawn.rs`:

````rust
//! `t-spawn N`: starts N children that exit at once, waiting for each, and
//! prints the free frames before and after (spec §8.5), so a scenario sees
//! that starting and ending a program leaks nothing. More kinds:
//!
//! - `t-spawn kill` starts `t-spin`, kills it after a moment and prints how
//!   it ended; then shows that process 1 cannot be killed and a pid nobody
//!   has does not exist;
//! - `t-spawn orphan` starts `t-spin 1` and ends without waiting for it,
//!   so it passes to process 1;
//! - `t-spawn child` exits at once (the children of `t-spawn N`).
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Standard output and error, as this program has them.
const STD: [FdMap; 2] = [
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
        Some(b"child") => return 0,
        Some(b"kill") => kill(),
        Some(b"orphan") => sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0).map(|_| ()),
        Some(n) => match parse(n) {
            Some(n) => many(n),
            None => return usage(),
        },
        None => return usage(),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-spawn: error {e}");
            1
        }
    }
}

fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|orphan\n");
    2
}

/// Free frames now.
fn free() -> Result<u64, u16> {
    Ok(sys::memory()?.ram_free / 4096)
}

/// Starts `t-spawn child` and waits for it; it must exit with 0.
fn child() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0child\0", b"", &[], 0)?;
    match sys::wait(i64::from(pid), false)? {
        Some((p, w)) if p == pid && w == relay_abi::WaitStatus::exited(0) => Ok(()),
        _ => Err(relay_abi::errno::ECHILD),
    }
}

fn many(n: u64) -> Result<(), u16> {
    // The first child may make the page tables of a kernel-stack slot,
    // which stay: count after it.
    child()?;
    let before = free()?;
    for _ in 0..n {
        child()?;
    }
    let after = free()?;
    let _ = writeln!(Fd(1), "free frames before: {before}");
    let _ = writeln!(Fd(1), "free frames after: {after}");
    Ok(())
}

fn kill() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-spin", b"t-spin\0", b"", &STD, 0)?;
    // It spins, and this one sleeps: the tick wakes it all the same.
    sys::sleep(200);
    sys::kill(i64::from(pid))?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "t-spin: {w}");
    }
    let name = |e: u16| match e {
        relay_abi::errno::EPERM => "EPERM",
        relay_abi::errno::ESRCH => "ESRCH",
        _ => "?",
    };
    for target in [1, 999_999] {
        let said = sys::kill(target).map_or_else(name, |()| "killed");
        let _ = writeln!(Fd(1), "kill {target}: {said}");
    }
    let me = sys::getpid();
    let _ = writeln!(Fd(1), "pid above 1: {}", me > 1);
    Ok(())
}

/// A decimal number.
fn parse(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0u64, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(u64::from(d))
    })
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` cannot find type `MemInfo` in this scope ``; `` cannot find struct, variant or union type `MemInfo` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: FAIL: scenario `spawn` stops at line 11, timed out waiting for `\nfree frames before: (\d+)\n`.

- [ ] **Step 9: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    EPERM,
}
````

with:

````rust
    EPERM,
    /// A call that blocked was cut short (the process was killed).
    EINTR,
}
````

Replace:

````rust
            Errno::EPERM => "Operation not permitted",
        }
````

with:

````rust
            Errno::EPERM => "Operation not permitted",
            Errno::EINTR => "Interrupted system call",
        }
````

Replace:

````rust
            Errno::EPERM => n::EPERM,
        }
````

with:

````rust
            Errno::EPERM => n::EPERM,
            Errno::EINTR => n::EINTR,
        }
````

- [ ] **Step 10: Change `kernel/src/mm/user.rs`**

In `kernel/src/mm/user.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let (start, end) = (self.addr + offset, self.addr + end);
        let mut page = start - start % PAGE;
        while page < end {
            match space.user_page(mem, page) {
                Some((_, Perm::ReadWrite)) => page += PAGE,
                _ => return Err(Errno::EFAULT),
            }
        }
        let mut virt = start;
````

with:

````rust
        let (start, end) = (self.addr + offset, self.addr + end);
        writable(space, mem, start, end)?;
        let mut virt = start;
````

Replace:

````rust
        Ok(())
    }
}

````

with:

````rust
        Ok(())
    }

    /// Whether every page of the range is one of the program's and
    /// writable; `EFAULT` otherwise. Nothing is written.
    pub fn check_writable(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
    ) -> Result<(), Errno> {
        writable(space, mem, self.addr, self.addr + self.len)
    }
}

/// `EFAULT` unless every page from `start` up to `end` is the program's
/// and writable.
fn writable(
    space: &AddressSpace,
    mem: &mut impl PhysMem,
    start: u64,
    end: u64,
) -> Result<(), Errno> {
    let mut page = start - start % PAGE;
    while page < end {
        match space.user_page(mem, page) {
            Some((_, Perm::ReadWrite)) => page += PAGE,
            _ => return Err(Errno::EFAULT),
        }
    }
    Ok(())
}

````

- [ ] **Step 11: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::exec::{self, Entry};
use crate::fd::FdTable;
use crate::mm::kstack::{self, KernelStack};
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::UserSlice;
use crate::syscall::{self, Caller, Outcome};
use crate::{arch, console, klogln, mm, mounts, rtc, timer, tty, usb};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::{Time, WaitStatus};
use spin::Mutex;
````

with:

````rust
use crate::exec::{self, Entry};
use crate::fd::{FdTable, File};
use crate::mm::kstack::{self, KernelStack};
use crate::mm::paging::MapError;
use crate::mm::space::AddressSpace;
use crate::mm::user::{UserSlice, UserStr};
use crate::mounts::KernelVfs;
use crate::syscall::{self, Caller, Child, Outcome, Spawn};
use crate::{arch, console, klogln, mm, mounts, rtc, timer, tty, usb};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use relay_abi::{MemInfo, Time, WaitStatus};
use spin::Mutex;
````

Replace:

````rust

/// Loads the program at `path` (read through `vfs`) with `args` (argument
/// 0 first) as a child of the running process, in a new process group,
/// with the running process's fds 0-2 and current directory; its pid.
/// `EAGAIN` when the table is full.
pub fn spawn(vfs: &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Result<u32, Errno> {
    if PROCS.lock().len() >= table::MAX {
        return Err(Errno::EAGAIN);
    }
    let name = String::from_utf8_lossy(path);
    let (file, program) = read_program(vfs, path, mm::heap_room()).map_err(|r| match r {
        Refusal::Unreadable(e) => e,
        Refusal::NotAProgram(e) => {
            klogln!("spawn {name}: {e}");
            Errno::ENOEXEC
        }
    })?;
    let arg_bytes = exec::arg_bytes(args)?;
    let stack = mm::alloc_kernel_stack().ok_or(Errno::EAGAIN)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(
            &mut space,
            mem,
            &file,
            &program,
            &arg_bytes,
            args.len() as u64,
        ) {
            Ok(entry) => Ok((space, entry)),
````

with:

````rust

/// Starts the child `s` of the running process (spec §5.2-§5.4, §7.3): the
/// program at its path, read through the mount table from the running
/// process's current directory, with its arguments, fds, working directory
/// and group. `EAGAIN` when the table is full; the fds and the working
/// directory are checked before the program is read.
pub fn spawn(s: &Spawn) -> Result<u32, Errno> {
    let (fds, mut cwd) = {
        let t = PROCS.lock();
        if t.len() >= table::MAX {
            return Err(Errno::EAGAIN);
        }
        let parent = t.get(t.current()).expect("a process spawns");
        (parent.res.fds.for_child(&s.fds)?, parent.res.cwd.clone())
    };
    let mut cwd = cwd.take().expect("the parent has its current directory");
    if !s.cwd.is_empty() {
        mounts::with(&mut cwd, |t| t.chdir(&s.cwd))?;
    }
    let name = String::from_utf8_lossy(&s.path);
    let (file, program) =
        read_program(&mut KernelVfs, &s.path, mm::heap_room()).map_err(|r| match r {
            Refusal::Unreadable(e) => e,
            Refusal::NotAProgram(e) => {
                klogln!("spawn {name}: {e}");
                Errno::ENOEXEC
            }
        })?;
    let stack = mm::alloc_kernel_stack().ok_or(Errno::EAGAIN)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
        let mut space = AddressSpace::new(mem, kernel).map_err(memory_error)?;
        match exec::load(&mut space, mem, &file, &program, &s.args, s.argc) {
            Ok(entry) => Ok((space, entry)),
````

Replace:

````rust
    prepare(&stack, first_run, 0);
    let mut t = PROCS.lock();
    let me = t.current();
    let parent = t.get(me);
    let fds = parent.map_or_else(FdTable::new, |p| {
        p.res
            .fds
            .for_child(&[
                relay_abi::FdMap {
                    child: 0,
                    parent: 0,
                },
                relay_abi::FdMap {
                    child: 1,
                    parent: 1,
                },
                relay_abi::FdMap {
                    child: 2,
                    parent: 2,
                },
            ])
            .unwrap_or_default()
    });
    let cwd = parent.and_then(|p| p.res.cwd.clone());
    let res = Res {
````

with:

````rust
    prepare(&stack, first_run, 0);
    let res = Res {
````

Replace:

````rust
        fds,
        cwd,
    };
    t.insert(me, true, name.into_owned(), res)
        .map_err(|e| unreachable!("room was checked, and nothing else runs: {e}"))
}

/// Waits until the child `pid` of the running process has ended, giving
/// what the in-kernel shell's children write to fds 1 and 2 to `out`
/// meanwhile; then gives back what was left of it. `ECHILD` if `pid` is
/// not its child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = loop {
        let reaped = {
            let mut t = PROCS.lock();
            let me = t.current();
            t.reap(me, Want::Pid(pid))
        };
        match reaped {
            Ok(Some(p)) => break Ok(p),
            Ok(None) => block(Blocked::Wait),
            Err(e) => break Err(e),
        }
    };
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    let p = ended?;
    mm::free_kernel_stack(p.res.stack);
    match p.state {
        table::State::Zombie(status) => Ok(status),
        _ => unreachable!("reap takes only zombies"),
    }
````

with:

````rust
        fds,
        cwd: Some(cwd),
    };
    let mut t = PROCS.lock();
    let me = t.current();
    t.insert(me, s.new_group, name.into_owned(), res)
        .map_err(|e| unreachable!("room was checked, and nothing else runs: {e}"))
}

/// A child of the running process that has ended, taken out of the table
/// with its kernel stack given back; `None` with `nohang` when none has.
/// `ECHILD` if there is no such child; `EINTR` if the running process was
/// killed while it waited (it ends on its way back to ring 3).
fn collect(child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
    let want = match child {
        Child::Any => Want::Any,
        Child::Pid(pid) => Want::Pid(pid),
    };
    loop {
        let reaped = {
            let mut t = PROCS.lock();
            let me = t.current();
            if t.get(me).is_some_and(|p| p.killed.is_some()) {
                return Err(Errno::EINTR);
            }
            t.reap(me, want)?
        };
        match reaped {
            Some(p) => {
                mm::free_kernel_stack(p.res.stack);
                let table::State::Zombie(status) = p.state else {
                    unreachable!("reap takes only zombies")
                };
                return Ok(Some((p.pid, status)));
            }
            None if nohang => return Ok(None),
            None => block(Blocked::Wait),
        }
    }
}

/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook); then it
/// collects the orphans that have ended, which pass to it as process 1
/// (plan 4's `/bin/sh` does that before every prompt). `ECHILD` if `pid`
/// is not its child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
    SHELL_OUT.store((&raw mut out).cast(), Ordering::Release);
    let ended = collect(Child::Pid(pid), false);
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    while let Ok(Some(_)) = collect(Child::Any, true) {}
    match ended? {
        Some((_, status)) => Ok(status),
        None => unreachable!("wait without nohang collects a child"),
    }
````

Replace:

````rust

impl Caller for Current {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        let t = PROCS.lock();
````

with:

````rust

impl Current {
    /// Runs `f` with the running process's address space.
    fn space<R>(
        &self,
        f: impl FnOnce(&AddressSpace, &mut mm::UserMem<'_>) -> Result<R, Errno>,
    ) -> Result<R, Errno> {
        let t = PROCS.lock();
````

Replace:

````rust
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| slice.read(space, mem, offset, buf))
    }

    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
        let t = PROCS.lock();
        let space = t
            .get(t.current())
            .and_then(|p| p.res.space.as_ref())
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| slice.write(space, mem, offset, bytes))
    }
````

with:

````rust
            .ok_or(Errno::EFAULT)?;
        mm::with_user_memory(|mem, _| f(space, mem))
    }

    /// The running process's file open as `fd`.
    fn file(&self, fd: u64) -> Result<Arc<File>, Errno> {
        let t = PROCS.lock();
        let p = t.get(t.current()).ok_or(Errno::EBADF)?;
        p.res.fds.get(fd).cloned()
    }
}

impl Caller for Current {
    fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
        self.space(|space, mem| slice.read(space, mem, offset, buf))
    }

    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno> {
        self.space(|space, mem| slice.write(space, mem, offset, bytes))
    }

    fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno> {
        self.space(|space, mem| slice.check_writable(space, mem))
    }

    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno> {
        self.space(|space, mem| s.read(space, mem))
    }

    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno> {
        self.file(fd).map(|_| ())
    }

    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
        match *self.file(fd)? {
            File::Console => console::write_output(bytes),
            File::ShellOutput(n) => {
                let out = SHELL_OUT.load(Ordering::Acquire);
                if out.is_null() {
                    // The shell waits for nobody: its prompt is on the
                    // screen, and so is this.
                    console::write_output(bytes);
                } else {
                    // SAFETY: set by the in-kernel shell's `wait`, which is
                    // blocked until its child has ended and clears it
                    // before it returns; nothing else calls it meanwhile.
                    unsafe { (*out.cast::<Out<'_>>())(n, bytes) }
                }
            }
        }
        Ok(())
    }

    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno> {
        spawn(s)
    }

    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno> {
        collect(child, nohang)
    }

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        PROCS.lock().kill(target, relay_abi::wait::KILLED_KILL)
    }

    fn pid(&self) -> u32 {
        PROCS.lock().current()
    }

    fn memory(&self) -> MemInfo {
        let s = mm::stats();
        MemInfo {
            ram_total: s.total_frames * mm::frame::FRAME_SIZE,
            ram_free: s.free_frames * mm::frame::FRAME_SIZE,
            heap_total: s.heap.total as u64,
            heap_used: s.heap.used as u64,
        }
    }
````

Replace:

````rust
        block(Blocked::Sleep(timer::sleep_until(timer::ticks(), ms)));
    }

    fn output(&mut self, fd: u32, bytes: &[u8]) {
        let out = SHELL_OUT.load(Ordering::Acquire);
        if out.is_null() {
            console::write_output(bytes);
        } else {
            // SAFETY: set by the in-kernel shell's `wait`, which is blocked
            // until this child has ended, and cleared before it returns;
            // nothing else calls it meanwhile.
            unsafe { (*out.cast::<Out<'_>>())(fd, bytes) }
        }
    }
````

with:

````rust
        block(Blocked::Sleep(timer::sleep_until(timer::ticks(), ms)));
    }
````

- [ ] **Step 12: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::mounts::KernelVfs;
use crate::{arch, console, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
````

with:

````rust
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
use crate::{arch, console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Shell, System};
````

Replace:

````rust

    fn spawn(
        &mut self,
        vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        Some(proc::spawn(vfs, path, args))
    }
````

with:

````rust

    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2.
    fn spawn(
        &mut self,
        _vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        let std = [0, 1, 2].map(|fd| FdMap {
            child: fd,
            parent: fd,
        });
        Some(exec::arg_bytes(args).and_then(|bytes| {
            proc::spawn(&Spawn {
                path: path.to_vec(),
                args: bytes,
                argc: args.len() as u64,
                cwd: Vec::new(),
                fds: std.to_vec(),
                new_group: true,
            })
        }))
    }
````

- [ ] **Step 13: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! and it answers with the result register's value or with the program's
//! exit. It serves `exit`, `write` to fds 1 and 2, `time` and `sleep`;
//! every other call is `ENOSYS` until the plan that brings it.

use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
use relay_abi::{Call, Time, encode};
use vfs::Errno;

/// What the dispatcher needs of the program that called: its memory,
/// where its output goes, and the kernel's services.
pub trait Caller {
````

with:

````rust
//! and it answers with the result register's value or with the program's
//! exit. It checks and copies what the program passes (`UserSlice`,
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. Plan 3a serves `exit`, `spawn`, `wait`, `kill`, `getpid`,
//! `write`, `time`, `sleep` and `sys_info`'s memory figures; every other
//! call is `ENOSYS` until the plan that brings it.

use crate::exec::ARGS_MAX;
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::vec::Vec;
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::Errno;

/// The longest path a program may pass (Linux's `PATH_MAX`).
pub const PATH_MAX: usize = 4096;

/// A child to start, as the dispatcher checked it (spec §7.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub path: Vec<u8>,
    /// The arguments, each followed by a NUL, and how many there are (at
    /// least argument 0).
    pub args: Vec<u8>,
    pub argc: u64,
    /// Relative to the caller's current directory; empty for that one.
    pub cwd: Vec<u8>,
    pub fds: Vec<FdMap>,
    pub new_group: bool,
}

/// Which child `wait` waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Any,
    Pid(u32),
}

/// What the dispatcher needs of the program that called: its memory, its
/// files, and the kernel's services.
pub trait Caller {
````

Replace:

````rust
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// What the program wrote to fd 1 or 2.
    fn output(&mut self, fd: u32, bytes: &[u8]);
    /// The wall clock and the uptime.
````

with:

````rust
    fn write(&mut self, slice: &UserSlice, offset: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// Whether `slice` is all the program's and writable (`EFAULT`
    /// otherwise), without writing it.
    fn writable(&mut self, slice: &UserSlice) -> Result<(), Errno>;
    /// A byte string of the program's, copied in.
    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno>;
    /// `EBADF` unless `fd` is open for writing.
    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno>;
    /// Writes `bytes` to `fd` (checked with `writable_fd`); the file's
    /// error, if any.
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// Starts a child; its pid.
    fn spawn(&mut self, s: &Spawn) -> Result<u32, Errno>;
    /// A child that has ended, with how; `None` if `nohang` and none has.
    /// `ECHILD` if there is no such child.
    fn wait(&mut self, child: Child, nohang: bool) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
    fn kill(&mut self, target: i64) -> Result<(), Errno>;
    fn pid(&self) -> u32;
    /// The memory figures of `free`.
    fn memory(&self) -> MemInfo;
    /// The wall clock and the uptime.
````

Replace:

````rust
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
````

with:

````rust
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Spawn) => spawn(caller, args[0]),
        Some(Call::Wait) => wait(caller, args[0] as i64, args[1], args[2]),
        Some(Call::Kill) => caller.kill(args[0] as i64).map(|()| 0),
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
````

Replace:

````rust
        }
        _ => Err(Errno::ENOSYS),
````

with:

````rust
        }
        Some(Call::SysInfo) => sys_info(caller, args[0], args[1], args[2]),
        _ => Err(Errno::ENOSYS),
````

Replace:

````rust

/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out a page at a time; a page that is not the program's ends the
/// call with the bytes written before it, or with `EFAULT` if there were
/// none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let fd = match fd {
        1 | 2 => fd as u32,
        _ => return Err(Errno::EBADF),
    };
    let slice = UserSlice::new(addr, len)?;
````

with:

````rust

/// `spawn(&SpawnArgs)` (spec §7.3): the struct, then the path, the
/// arguments and the working directory it points at, each checked and
/// copied in before anything starts.
fn spawn(caller: &mut impl Caller, addr: u64) -> Result<u64, Errno> {
    let mut raw = [0u8; SpawnArgs::SIZE];
    caller.read(&UserSlice::new(addr, raw.len() as u64)?, 0, &mut raw)?;
    let a = SpawnArgs::from_bytes(&raw);
    if a.flags & !NEW_GROUP != 0 || a.fd_count as usize > SPAWN_FDS {
        return Err(Errno::EINVAL);
    }
    let path = caller.read_str(&UserStr::new(
        a.path,
        a.path_len,
        PATH_MAX,
        Errno::ENAMETOOLONG,
    )?)?;
    let args = caller.read_str(&UserStr::new(a.args, a.args_len, ARGS_MAX, Errno::E2BIG)?)?;
    // Argument 0 at least, and every argument ends with its NUL.
    if args.last() != Some(&0) {
        return Err(Errno::EINVAL);
    }
    let argc = args.iter().filter(|&&b| b == 0).count() as u64;
    let cwd = caller.read_str(&UserStr::new(
        a.cwd,
        a.cwd_len,
        PATH_MAX,
        Errno::ENAMETOOLONG,
    )?)?;
    let s = Spawn {
        path,
        args,
        argc,
        cwd,
        fds: a.fds[..a.fd_count as usize].to_vec(),
        new_group: a.flags & NEW_GROUP != 0,
    };
    caller.spawn(&s).map(u64::from)
}

/// `wait(pid or -1, flags, &mut WaitStatus or 0)` (spec §7.3). The status's
/// memory is checked before a child is collected, so a bad pointer never
/// loses a child's status.
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
    if flags & !u64::from(WAIT_NOHANG) != 0 {
        return Err(Errno::EINVAL);
    }
    let child = match pid {
        WAIT_ANY => Child::Any,
        p if p > 0 => Child::Pid(u32::try_from(p).map_err(|_| Errno::ECHILD)?),
        _ => return Err(Errno::EINVAL),
    };
    let size = core::mem::size_of::<WaitStatus>() as u64;
    let status = match addr {
        0 => None,
        a => Some(UserSlice::new(a, size)?),
    };
    if let Some(slice) = &status {
        caller.writable(slice)?;
    }
    let Some((pid, w)) = caller.wait(child, flags & u64::from(WAIT_NOHANG) != 0)? else {
        return Ok(0);
    };
    if let Some(slice) = &status {
        let mut bytes = [0u8; 32];
        for (i, v) in [w.how, w.code, w.fault, w.detail].iter().enumerate() {
            bytes[4 * i..4 * i + 4].copy_from_slice(&v.to_ne_bytes());
        }
        bytes[16..24].copy_from_slice(&w.address.to_ne_bytes());
        bytes[24..].copy_from_slice(&w.ip.to_ne_bytes());
        caller.write(slice, 0, &bytes)?;
    }
    Ok(u64::from(pid))
}

/// `write(fd, buffer, length)`. Copies the buffer out a page at a time; a
/// page that is not the program's, or the file's error, ends the call with
/// the bytes written before it, or with the error if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    caller.writable_fd(fd)?;
    let slice = UserSlice::new(addr, len)?;
````

Replace:

````rust
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        caller.output(fd, &buf[..n]);
        done += n as u64;
````

with:

````rust
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        let written = caller
            .read(&slice, done, &mut buf[..n])
            .and_then(|()| caller.output(fd, &buf[..n]));
        if let Err(e) = written {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        done += n as u64;
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

/// `sys_info(kind, buffer, length)` (spec §7.3): the bytes written. Plan
/// 3a has the memory figures (`MemInfo`); the `uname` fields and the kernel
/// log come with plan 3b.
fn sys_info(caller: &mut impl Caller, kind: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    if kind != u64::from(INFO_MEMORY) {
        return Err(Errno::EINVAL);
    }
    let m = caller.memory();
    let mut bytes = [0u8; size_of::<MemInfo>()];
    for (i, v) in [m.ram_total, m.ram_free, m.heap_total, m.heap_used]
        .iter()
        .enumerate()
    {
        bytes[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
    }
    if len < bytes.len() as u64 {
        return Err(Errno::EINVAL);
    }
    caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
    Ok(bytes.len() as u64)
}
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 267 tests.

Run: `cargo test -p vfs`

Expected: PASS: 47 tests.

- [ ] **Step 15: Run the `spawn`, `system`, `programs`, `userfault`, `ctrlc` scenarios**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add Cargo.lock crates kernel tests userland
git commit -m "kernel: programs start programs: spawn, wait, kill, getpid and the memory figures from ring 3, write through the fd table; t-spawn"
````


### Task 16: A process killed before its first run runs none of its program

Found by the prototype's review: `first_run` went to ring 3 without looking whether the process had been killed, so a child killed right after `spawn` (or a group killed while one of its members waited for its first run) still ran until its first system call, which the kernel served before `before_user` ended it; in plan 3b that call could be an `unlink`. `first_run` now calls `end_if_killed` first (decision 5). `t-spawn kill-new` starts `t-args` and kills it at once: the `spawn` scenario expects the kernel log's line right after the command, with no output of `t-args` before it (the red run shows its `[`). Mutation check: `first_run` without `end_if_killed` fails `spawn`.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `tests/e2e/spawn.txt`
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: Tasks 14 (`end_if_killed`), 15 (`t-spawn`, `sys::kill`).
- Produces: `t-spawn kill-new`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, replace:

````text
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
# The orphan prints when its second is up, over the prompt.
````

with:

````text
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
# A program killed before it has run runs none of its code: the kernel log
# line comes right after the command, with no output of t-args before it.
send t-spawn kill-new
expect t-spawn kill-new\npid \d+ \(/bin/t-args\): killed: kill\nt-args: kill\n
# The orphan prints when its second is up, over the prompt.
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!   has does not exist;
//! - `t-spawn orphan` starts `t-spin 1` and ends without waiting for it,
````

with:

````rust
//!   has does not exist;
//! - `t-spawn kill-new` starts `t-args` and kills it before it has run:
//!   none of its code may run;
//! - `t-spawn orphan` starts `t-spin 1` and ends without waiting for it,
````

Replace:

````rust
        Some(b"kill") => kill(),
        Some(b"orphan") => sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0).map(|_| ()),
````

with:

````rust
        Some(b"kill") => kill(),
        Some(b"kill-new") => kill_new(),
        Some(b"orphan") => sys::spawn(b"/bin/t-spin", b"t-spin\x001\0", b"", &STD, 0).map(|_| ()),
````

Replace:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|orphan\n");
    2
````

with:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan\n");
    2
````

Replace:

````rust

/// A decimal number.
````

with:

````rust

/// Kills a child before it has run, and says how it ended.
fn kill_new() -> Result<(), u16> {
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0SHOULD-NOT-PRINT\0", b"", &STD, 0)?;
    sys::kill(i64::from(pid))?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "t-args: {w}");
    }
    Ok(())
}

/// A decimal number.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: FAIL: scenario `spawn` stops at line 24, timed out waiting for `t-spawn kill-new\npid \d+ \(/bin/t-args\): killed: kill\nt-args: kill\n`.

- [ ] **Step 4: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

/// Where a new process's program starts: its first switch comes here.
extern "C" fn first_run(_: u64) -> ! {
    let entry = {
````

with:

````rust

/// Where a new process's program starts: its first switch comes here, with
/// no lock held.
extern "C" fn first_run(_: u64) -> ! {
    // Killed before it ever ran: none of its program runs either.
    end_if_killed();
    let entry = {
````

- [ ] **Step 5: Run the `spawn` scenario**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: a process killed before its first run runs none of its program"
````


### Task 17: The in-kernel shell collects ended orphans before each command

Found by the prototype's review (important): process 1 collected its orphans only after a program it waited for, so the zombies of descendants that ended while the shell sat at its prompt stayed in the table; once they filled it, every program was `EAGAIN`, and since a refused `spawn` never reaches `wait`, nothing collected them again until a reboot. `KernelSystem::spawn` now collects them first (`proc::collect_orphans`, decision 11). `t-spawn fill` starts children that nap for 300 ms until the table is full (62 of them) and ends; after they have ended, `t-spawn 10` must still start its children (the red run: `t-spawn: error 11`). Mutation check: `KernelSystem::spawn` without `collect_orphans` fails `spawn`.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `tests/e2e/spawn.txt`
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: Task 15 (`collect`, `t-spawn`).
- Produces: `proc::collect_orphans()`; `t-spawn fill` and `nap`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, replace:

````text
expect \n\[1\] after-the-orphan\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

with:

````text
expect \n\[1\] after-the-orphan\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# Orphans that end while the shell waits for nobody are collected before
# its next command starts: a table full of their zombies would refuse
# every program until a reboot.
send t-spawn fill
expect \nfilled the table with 62 children\n
alive 1
send t-spawn 10
expect \nfree frames after: \d+\n
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!   so it passes to process 1;
//! - `t-spawn child` exits at once (the children of `t-spawn N`).
#![no_std]
````

with:

````rust
//!   so it passes to process 1;
//! - `t-spawn fill` starts children that nap for 300 ms until the process
//!   table is full, and ends without waiting for them: their zombies pass
//!   to process 1;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
//!   nap` after 300 ms.
#![no_std]
````

Replace:

````rust
        Some(b"child") => return 0,
        Some(b"kill") => kill(),
````

with:

````rust
        Some(b"child") => return 0,
        Some(b"nap") => {
            sys::sleep(300);
            return 0;
        }
        Some(b"fill") => fill(),
        Some(b"kill") => kill(),
````

Replace:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan\n");
    2
````

with:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan|fill\n");
    2
````

Replace:

````rust

/// Kills a child before it has run, and says how it ended.
````

with:

````rust

/// Starts napping children until the table is full, and leaves them.
fn fill() -> Result<(), u16> {
    let mut n = 0;
    loop {
        match sys::spawn(b"/bin/t-spawn", b"t-spawn\0nap\0", b"", &[], 0) {
            Ok(_) => n += 1,
            Err(relay_abi::errno::EAGAIN) => break,
            Err(e) => return Err(e),
        }
    }
    let _ = writeln!(Fd(1), "filled the table with {n} children");
    Ok(())
}

/// Kills a child before it has run, and says how it ended.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: FAIL: scenario `spawn` stops at line 39, timed out waiting for `\nfree frames after: \d+\n`.

- [ ] **Step 4: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook); then it
/// collects the orphans that have ended, which pass to it as process 1
/// (plan 4's `/bin/sh` does that before every prompt). `ECHILD` if `pid`
/// is not its child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
````

with:

````rust

/// Collects the running process's children that have ended: for the
/// in-kernel shell, process 1, the orphans that passed to it. It does so
/// before each command it starts and after each it waited for, so their
/// zombies never fill the table (plan 4's `/bin/sh` does it before every
/// prompt).
pub fn collect_orphans() {
    while let Ok(Some(_)) = collect(Child::Any, true) {}
}

/// The in-kernel shell waits for its child `pid`, giving what its children
/// write to fds 1 and 2 to `out` meanwhile (plan 2's hook); then it
/// collects the orphans that have ended. `ECHILD` if `pid` is not its
/// child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
````

Replace:

````rust
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    while let Ok(Some(_)) = collect(Child::Any, true) {}
    match ended? {
````

with:

````rust
    SHELL_OUT.store(core::ptr::null_mut(), Ordering::Release);
    collect_orphans();
    match ended? {
````

- [ ] **Step 5: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2.
    fn spawn(
````

with:

````rust
    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2;
    /// the orphans that have ended are collected first.
    fn spawn(
````

Replace:

````rust
        });
        Some(exec::arg_bytes(args).and_then(|bytes| {
````

with:

````rust
        });
        // Orphans that ended while the shell waited for nobody.
        proc::collect_orphans();
        Some(exec::arg_bytes(args).and_then(|bytes| {
````

- [ ] **Step 6: Run the `spawn` scenario**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: the in-kernel shell collects ended orphans before each command, so their zombies never fill the table"
````


### Task 18: An orphan's output goes to the screen, not into the next command's

Found by the prototype's review (important): every write to a `File::ShellOutput` reached the waiting shell's hook, whoever wrote it, so an orphan of an earlier command wrote into the next command's redirection file (or a script's transcript), under that command's write-error rules. Now the shell gets fresh outputs for every command it starts (`proc::renew_outputs`, from `KernelSystem::spawn`), which the command and its descendants inherit, and the hook answers only writes to the shell's outputs of the moment; anything else goes to the screen (decision 8). `FdTable::set` puts a file in. The `spawn` scenario runs `t-spawn orphan`, then `t-spin 2 > /root/o.txt`: the orphan's line must be on the screen, and the file must hold one line (the red run finds the line in the file). Mutation checks: the hook answering any `ShellOutput`, and no fresh outputs per command, each fail `spawn`.

**Files:**
- Modify: `kernel/src/fd.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `tests/e2e/spawn.txt`

**Interfaces:**
- Consumes: Tasks 5, 15, 17.
- Produces: `FdTable::set(usize, Arc<File>)`; `proc::renew_outputs()`.

- [ ] **Step 1: Add the failing tests to `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
    #[test]
    fn a_bad_mapping_is_refused() {
````

with:

````rust
    #[test]
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
    fn a_bad_mapping_is_refused() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, replace:

````text
expect t-spawn kill-new\npid \d+ \(/bin/t-args\): killed: kill\nt-args: kill\n
# The orphan prints when its second is up, over the prompt.
send t-spawn orphan
expect # \d+ iterations\n
send t-args after-the-orphan
expect \n\[1\] after-the-orphan\n
send free
````

with:

````text
expect t-spawn kill-new\npid \d+ \(/bin/t-args\): killed: kill\nt-args: kill\n
# An orphan's output goes to the screen, never into the output of the
# command the shell waits for next: the orphaned t-spin 1 prints while
# t-spin 2 runs, redirected.
send t-spawn orphan
send t-spin 2 > /root/o.txt
expect > /root/o.txt\n\d+ iterations\n
send cat /root/o.txt
expect # cat /root/o.txt\n\d+ iterations\nroot@relay:~# $
send free
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib fd::`

Expected: FAIL: compile errors such as `` no method named `set` found for struct `fd::FdTable` in the current scope ``.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: FAIL: scenario `spawn` stops at line 30, timed out waiting for `> /root/o.txt\n\d+ iterations\n`.

- [ ] **Step 4: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
        t
    }
````

with:

````rust
        t
    }

    /// Opens `file` as `fd` (below 32), closing what was there.
    pub fn set(&mut self, fd: usize, file: Arc<File>) {
        self.slots[fd] = Some(file);
    }
````

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Collects the running process's children that have ended: for the
````

with:

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
````

Replace:

````rust
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
        match *self.file(fd)? {
            File::Console => console::write_output(bytes),
            File::ShellOutput(n) => {
                let out = SHELL_OUT.load(Ordering::Acquire);
                if out.is_null() {
                    // The shell waits for nobody: its prompt is on the
                    // screen, and so is this.
                    console::write_output(bytes);
````

with:

````rust
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno> {
        let file = self.file(fd)?;
        match *file {
            File::Console => console::write_output(bytes),
            File::ShellOutput(n) => {
                let out = SHELL_OUT.load(Ordering::Acquire);
                if out.is_null() || !shell_output_now(&file, n) {
                    // The shell waits for nobody, or for another command:
                    // this goes to the screen.
                    console::write_output(bytes);
````

- [ ] **Step 6: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2;
    /// the orphans that have ended are collected first.
    fn spawn(
````

with:

````rust
    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2,
    /// its outputs fresh for this command; the orphans that have ended are
    /// collected first.
    fn spawn(
````

Replace:

````rust
        });
        // Orphans that ended while the shell waited for nobody.
        proc::collect_orphans();
        Some(exec::arg_bytes(args).and_then(|bytes| {
````

with:

````rust
        });
        // Orphans that ended while the shell waited for nobody; and
        // outputs of the command's own.
        proc::collect_orphans();
        proc::renew_outputs();
        Some(exec::arg_bytes(args).and_then(|bytes| {
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib fd::`

Expected: PASS: 4 tests.

- [ ] **Step 8: Run the `spawn`, `programs` scenarios**

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add kernel tests
git commit -m "kernel: an orphan's output goes to the screen, not into the next command's"
````


### Task 19: Ctrl-C of a group blocked in the kernel: `t-spawn sleepers`

Found by the prototype's review: every kill in the scenarios hit a spinning `t-spin` through a tick, so no test noticed `before_user` without its `end_if_killed`, or `collect` without its `EINTR` (decision 5). `t-spawn sleepers` starts three children that sleep for a minute in its group and a fourth in a group of its own, and waits for that fourth; Ctrl-C kills the group, whose members are all blocked in the kernel: `wait` can end only by its `EINTR` (the fourth sleeps on, and the other three ending does not satisfy it), and each ends on its way out of the kernel; the `ctrlc` scenario expects four `killed: Ctrl-C` lines and `^C` at once. This task only adds the test: the code already does the right thing, which the mutation checks show (`before_user` without `end_if_killed`: `t-spawn: error 4`; `collect` without `EINTR`: no `^C` for a minute). A first version with every child in the group let the second mutant survive, since the children's ends woke the parent to reap one. The review's third mutant, `before_user` never ending a slice, is ruled out: only a program in the kernel at every tick shows it, and any test of it would rest on timing.

**Files:**
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `userland/tests/src/bin/t-spawn.rs`

**Interfaces:**
- Consumes: Tasks 14, 15.
- Produces: `t-spawn sleepers` and `doze`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, replace:

````text
expect \n\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
````

with:

````text
expect \n\^C\nroot@relay:~# 
# Ctrl-C ends a group that waits in the kernel: t-spawn blocked in wait
# for a child in a group of its own, and three children in sleep. Each ends
# on its way out of the kernel, the parent's wait cut short by the kill
# alone, and the shell says `^C` at once; the child apart sleeps on.
send t-spawn sleepers
alive 1
key {ctrl-c}
expect t-spawn sleepers\n(pid \d+ \(/bin/t-spawn\): killed: Ctrl-C\n){4}\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!   to process 1;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
//!   nap` after 300 ms.
#![no_std]
````

with:

````rust
//!   to process 1;
//! - `t-spawn sleepers` starts three children that sleep for a minute in
//!   its group and one in a group of its own, and waits for that one: a
//!   group blocked in the kernel, for Ctrl-C, whose `wait` nothing but the
//!   kill can end;
//! - `t-spawn child` exits at once (the children of `t-spawn N`), `t-spawn
//!   nap` after 300 ms, `t-spawn doze` after a minute.
#![no_std]
````

Replace:

````rust
        Some(b"fill") => fill(),
        Some(b"kill") => kill(),
````

with:

````rust
        Some(b"fill") => fill(),
        Some(b"doze") => {
            sys::sleep(60_000);
            return 0;
        }
        Some(b"sleepers") => sleepers(),
        Some(b"kill") => kill(),
````

Replace:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan|fill\n");
    2
````

with:

````rust
fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-spawn N|kill|kill-new|orphan|fill|sleepers\n");
    2
````

Replace:

````rust

/// Starts napping children until the table is full, and leaves them.
````

with:

````rust

/// Children asleep, and this one waiting for one outside its group.
fn sleepers() -> Result<(), u16> {
    for _ in 0..3 {
        sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], 0)?;
    }
    let apart = relay_abi::spawn::NEW_GROUP;
    let pid = sys::spawn(b"/bin/t-spawn", b"t-spawn\0doze\0", b"", &[], apart)?;
    sys::wait(i64::from(pid), false)?;
    let _ = sys::write_all(1, b"t-spawn: the sleeper woke\n");
    Ok(())
}

/// Starts napping children until the table is full, and leaves them.
````

- [ ] **Step 3: Run the `ctrlc` scenario**

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests userland
git commit -m "Ctrl-C of a group blocked in the kernel: t-spawn sleepers"
````


### Task 20: A program sees its redirection's write error

The final review of plan 2 found that a program writing to a redirection file that hits a write error (a full disk) sees success: the shell reports the error when the command ends, but the program cannot stop early (decision 8). `System::wait`'s hook now answers each write (`shell::Output`, a `FnMut(u32, &[u8]) -> Result<(), Errno>`): `Streams::out` returns the file's first error once there is one, from the write that flushes the 4 KiB buffer on, and the kernel passes the answer to the program's `write`. Output to the screen is always fine. Mutation check: `Streams::out` answering `Ok` after the file's error fails the shell test.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`

**Interfaces:**
- Consumes: Task 15 (`Caller::output` returning a result), plan 2's `System::wait`, `Ctx::wait_program`.
- Produces: `shell::Output<'a>`; `System::wait(&mut self, pid, &mut Output<'_>)`; `TestSystem::answers`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_program_stopped_by_ctrl_c_says_only_so_and_ends_a_script() {
````

with:

````rust
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
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, System};
use alloc::boxed::Box;
````

with:

````rust
use crate::Shell;
use crate::io::{Console, MemInfo, Output, System};
use alloc::boxed::Box;
````

Replace:

````rust
    pub spawned: Vec<Vec<Vec<u8>>>,
    /// The program started and not yet waited for.
````

with:

````rust
    pub spawned: Vec<Vec<Vec<u8>>>,
    /// What each of the programs' writes was answered.
    pub answers: Vec<Result<(), Errno>>,
    /// The program started and not yet waited for.
````

Replace:

````rust
            spawned: Vec::new(),
            child: None,
````

with:

````rust
            spawned: Vec::new(),
            answers: Vec::new(),
            child: None,
````

Replace:

````rust
    }
    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        let i = self
````

with:

````rust
    }
    fn wait(&mut self, pid: u32, out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        let i = self
````

Replace:

````rust
        for (fd, bytes) in &p.writes {
            out(*fd, bytes);
        }
````

with:

````rust
        for (fd, bytes) in &p.writes {
            self.answers.push(out(*fd, bytes));
        }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` unresolved import `crate::io::Output` ``.

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

    /// Standard output.
    pub fn out(&mut self, bytes: &[u8]) {
        self.streams().out(bytes);
    }
````

with:

````rust

    /// Standard output. A write error is kept for `finish` (and
    /// `out_failed`); later output is dropped.
    pub fn out(&mut self, bytes: &[u8]) {
        let _ = self.streams().out(bytes);
    }
````

Replace:

````rust
    /// Waits for the program `pid` (`System::spawn`): what it writes to fd 1
    /// is standard output, to fd 2 the screen.
    pub(crate) fn wait_program(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
````

with:

````rust
    /// Waits for the program `pid` (`System::spawn`): what it writes to fd 1
    /// is standard output, to fd 2 the screen. A redirection file's write
    /// error is the program's too, from the write that met it on (output
    /// to a file is written in pieces of 4 KiB, so a short one meets it
    /// only when the command ends, and the shell reports it then).
    pub(crate) fn wait_program(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
````

Replace:

````rust
            if fd == 1 {
                streams.out(bytes);
            } else {
                streams.screen(bytes);
            }
````

with:

````rust
            if fd == 1 {
                streams.out(bytes)
            } else {
                streams.screen(bytes);
                Ok(())
            }
````

Replace:

````rust
impl Streams<'_> {
    fn out(&mut self, bytes: &[u8]) {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, .. } => {
                buf.extend_from_slice(bytes);
````

with:

````rust
impl Streams<'_> {
    /// Standard output; the file's first write error, once there is one.
    fn out(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, error, .. } => {
                if let Some(e) = error {
                    return Err(*e);
                }
                buf.extend_from_slice(bytes);
````

Replace:

````rust
                }
            }
        }
    }
````

with:

````rust
                }
            }
        }
        match &*self.out {
            Output::File { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
        }
    }
````

- [ ] **Step 5: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use vfs::{Errno, Vfs};

````

with:

````rust
use vfs::{Errno, Vfs};

/// Where a program's output goes (`System::wait`): what it writes, on fd
/// 1 or 2, and the write's error, if any.
pub type Output<'a> = dyn FnMut(u32, &[u8]) -> Result<(), Errno> + 'a;

````

Replace:

````rust
    /// Runs the program `spawn` started until it ends, giving what it
    /// writes to fds 1 and 2 to `out`, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
````

with:

````rust
    /// Runs the program `spawn` started until it ends, giving what it
    /// writes to fds 1 and 2 to `out`, whose answer (a redirection file's
    /// write error) is the program's, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
````

- [ ] **Step 6: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, System};
pub use shell::Shell;
````

with:

````rust
pub use ctx::Ctx;
pub use io::{Console, MemInfo, Output, System};
pub use shell::Shell;
````

- [ ] **Step 7: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
/// The in-kernel shell's output while it waits for a command (plan 2's
/// hook, `System::wait`): what its children write to fds 1 and 2.
type Out<'a> = &'a mut dyn FnMut(u32, &[u8]);
static SHELL_OUT: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
````

with:

````rust
/// The in-kernel shell's output while it waits for a command (plan 2's
/// hook, `System::wait`): what its children write to fds 1 and 2, and the
/// answer to each write (a redirection file's error).
type Out<'a> = &'a mut shell::Output<'a>;
static SHELL_OUT: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
````

Replace:

````rust
/// child.
pub fn wait(pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
````

with:

````rust
/// child.
pub fn wait(pid: u32, out: &mut shell::Output<'_>) -> Result<WaitStatus, Errno> {
    let mut out: Out<'_> = out;
````

Replace:

````rust
                    // before it returns; nothing else calls it meanwhile.
                    unsafe { (*out.cast::<Out<'_>>())(n, bytes) }
                }
````

with:

````rust
                    // before it returns; nothing else calls it meanwhile.
                    return unsafe { (*out.cast::<Out<'_>>())(n, bytes) };
                }
````

- [ ] **Step 8: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, Errno, Vfs};
````

with:

````rust
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Output, Shell, System};
use vfs::{Env, Errno, Vfs};
````

Replace:

````rust
    /// for it (spec §6.4): a Ctrl-C kills it.
    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        proc::give_console(pid);
````

with:

````rust
    /// for it (spec §6.4): a Ctrl-C kills it.
    fn wait(&mut self, pid: u32, out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        proc::give_console(pid);
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 128 tests.

- [ ] **Step 10: Run the `programs`, `diskfull` scenarios**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates kernel
git commit -m "kernel, shell: a program sees the write error of the file its output is redirected to"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 28 scenario(s) passed`.

````bash
git push -u origin m2p3a/calls
gh pr create --base main --head m2p3a/calls --title "Milestone 2, plan 3a: Ctrl-C, and programs that start programs" --body-file - <<'EOF'
## What

Milestone 2, plan 3a, tasks 14–20: the in-kernel shell gives the console to its command's group in line mode while it waits, and a Ctrl-C kills that group (`^C`, status 130, a script stops); `spawn`, `wait`, `kill`, `getpid` and `sys_info`'s memory figures from ring 3, each checking what the program passes, with `write` through the fd table and process 1 collecting orphans before and after each command; a process killed before its first run runs none of its program; an orphan's output goes to the screen, not into the next command's; `relay-rt`'s wrappers and `t-spawn` (the `spawn` scenario: a thousand children lose no frame; `ctrlc`: a group blocked in the kernel); a program sees its redirection's write error.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed now: NUC check 3 runs it with PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3a/calls --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-calls
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: What the CPU enforces, and processes on the NUC (Tasks 21–27)

SMEP and SMAP, CR0.TS, CR4.FSGSBASE cleared (a hardening the prototype review raised), the trap flag's test, plan 2's deferred minors, and NUC check 3 with processes. It ends with NUC check 3 on a stick written by `cargo xtask flash --full` from this worktree; the pull request stays a draft until the user reports its results.

Branch `m2p3a/cpu`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-cpu`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3a/cpu /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-cpu origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-cpu
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3a/calls` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3a/cpu /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-cpu m2p3a/calls`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3a/calls>` and re-run `cargo xtask ci` before pushing.

### Task 21: SMEP and SMAP; `t-fault flags-ac`

Spec §5.1: SMEP and SMAP are on when CPUID reports them (leaf 7's EBX bits 7 and 20; the i7-1260P has both, and so does QEMU with `-cpu max`, under KVM and TCG alike), and the kernel log says `cpu: SMEP on, SMAP on` (decision 13). The kernel copies a program's memory through the linear map and runs none of its code, so nothing else could show that they are on: two panic tests do, `panic=user-read` (the kernel reads a program's page) and `panic=user-exec` (it calls the `ret` on one), each a page fault in ring 0 with SMEP or SMAP (scenarios `panic_smap`, `panic_smep`, which a kernel without them fails by going on). `t-fault flags-ac` sets AC, NT and DF and spins through a few hundred ticks before it faults: every tick and the fault must find the kernel's flags (Task 7), which is what SMAP needs. Mutation checks: `irq_common` without `push 2; popfq` fails `userfault` (the dispatcher's assertion during `flags-ac`); CR4 not written fails `panic_smap` (`the kernel read a program's page: no SMAP`). (A first run of both mutants with a build error was discarded: a mutant that does not compile is not a kill.)

**Files:**
- Modify: `kernel/src/arch/context.rs`
- Create: `kernel/src/arch/cpu.rs`
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/cmdline.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/proc.rs`
- Create: `tests/e2e/panic_smap.txt`
- Create: `tests/e2e/panic_smep.txt`
- Modify: `tests/e2e/userfault.txt`
- Modify: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Tasks 7, 9 (`arch::context::use_tables`); plan 2's `AddressSpace`, `elf::PROGRAM_BASE`.
- Produces: `arch::cpu::{Protection {smep, smap}, features(max_leaf, leaf7_ebx) -> Protection, cr4_with(u64, Protection) -> u64, init() -> Protection}`; `cmdline::PanicTest::{UserRead, UserExec}`; `arch::context::use_tables(u64)` (renamed from `use_kernel_tables`); `t-fault flags-ac`; the `panic_smap` and `panic_smep` scenarios.

- [ ] **Step 1: Write the failing tests for `kernel/src/arch/cpu.rs`**

Create `kernel/src/arch/cpu.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smep_and_smap_are_leaf_7_s_ebx_bits_7_and_20() {
        let both = features(0x20, SMEP | SMAP | 1);
        assert_eq!(
            both,
            Protection {
                smep: true,
                smap: true
            }
        );
        assert_eq!(
            features(0x20, SMEP),
            Protection {
                smep: true,
                smap: false
            }
        );
        assert_eq!(
            features(0x20, SMAP),
            Protection {
                smep: false,
                smap: true
            }
        );
        assert_eq!(
            features(6, SMEP | SMAP),
            Protection {
                smep: false,
                smap: false
            },
            "no leaf 7"
        );
        assert_eq!(both.to_string(), "SMEP on, SMAP on");
        assert_eq!(
            features(0, 0).to_string(),
            "SMEP not available, SMAP not available"
        );
    }

    #[test]
    fn cr4_gets_bits_20_and_21_and_keeps_the_rest() {
        let p = Protection {
            smep: true,
            smap: true,
        };
        assert_eq!(cr4_with(0x6F0, p), 0x6F0 | 3 << 20);
        let none = Protection {
            smep: false,
            smap: false,
        };
        assert_eq!(cr4_with(0x6F0 | 3 << 20, none), 0x6F0);
        let smep = Protection {
            smep: true,
            smap: false,
        };
        assert_eq!(cr4_with(0, smep), 1 << 20);
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust
pub mod context;
pub mod fault;
````

with:

````rust
pub mod context;
pub mod cpu;
pub mod fault;
````

- [ ] **Step 3: Add the failing tests to `kernel/src/cmdline.rs`**

In `kernel/src/cmdline.rs`, replace:

````rust
    #[test]
    fn test_mode_and_panic_kinds() {
````

with:

````rust
    #[test]
    fn a_program_s_page_from_the_kernel() {
        assert_eq!(
            Cmdline::parse("panic=user-read").panic_test,
            Some(PanicTest::UserRead)
        );
        assert_eq!(
            Cmdline::parse("panic=user-exec").panic_test,
            Some(PanicTest::UserExec)
        );
    }

    #[test]
    fn test_mode_and_panic_kinds() {
````

- [ ] **Step 4: Add the scenario `tests/e2e/panic_smap.txt`**

Create `tests/e2e/panic_smap.txt`:

````text
# SMAP is on (user-space gate §5.1; milestone 2, plan 3a): the kernel reading
# a program's page is a page fault in ring 0, the panic screen, not a
# silent read. The CPU says why: a present page, read, from ring 0.
cmdline test=1 panic=user-read
timeout 30
expect triggering UserRead
expect KERNEL PANIC
expect CPU exception 14: page fault \(error code 0x1\)
expect CR2=0x0000000000400000
expect System halted
````

- [ ] **Step 5: Add the scenario `tests/e2e/panic_smep.txt`**

Create `tests/e2e/panic_smep.txt`:

````text
# SMEP is on (user-space gate §5.1; milestone 2, plan 3a): the kernel running
# a program's code is a page fault in ring 0 at the fetch, the panic screen.
cmdline test=1 panic=user-exec
timeout 30
expect triggering UserExec
expect KERNEL PANIC
expect CPU exception 14: page fault \(error code 0x11\)
expect CR2=0x0000000000400000
expect System halted
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/userfault.txt`**

In `tests/e2e/userfault.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# use is the same before and after. A program's flags stay its own, after
# an exit as after a fault. `sse` comes with plan 3's CR0.TS.
timeout 30
````

with:

````text
# use is the same before and after. A program's flags stay its own, after
# an exit as after a fault, and through the timer ticks of a spin.
timeout 30
````

Replace:

````text
expect \n\[1\] after-a-fault\n
send t-fault
````

with:

````text
expect \n\[1\] after-a-fault\n
send t-fault flags-ac
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
send t-args after-ticks-with-ac
expect \n\[1\] after-ticks-with-ac\n
send t-fault
````

- [ ] **Step 7: Change the test program `userland/tests/src/bin/t-fault.rs`**

In `userland/tests/src/bin/t-fault.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program. Plan 3 adds
//! `sse`, once CR0.TS makes SSE fault.
#![no_std]
````

with:

````rust
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program. `flags-ac` sets
//! them and spins through a few hundred timer ticks before it faults: the
//! kernel's interrupt handlers must not run with them either (AC would
//! switch SMAP off).
#![no_std]
````

Replace:

````rust
            ),
            b"flags-ud" => asm!(
````

with:

````rust
            ),
            b"flags-ac" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "2:",
                "dec rcx",
                "jnz 2b",
                "ud2",
                flags = in(reg) FLAGS,
                in("rcx") 1u64 << 30,
                options(noreturn),
            ),
            b"flags-ud" => asm!(
````

Replace:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud\n",
                );
````

with:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac\n",
                );
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `SMEP` in this scope ``; `` cannot find value `SMAP` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario panic_smap`

Expected: FAIL: scenario `panic_smap` stops at line 6, timed out waiting for `triggering UserRead`.

- [ ] **Step 9: Change `kernel/src/arch/context.rs`**

In `kernel/src/arch/context.rs`, replace:

````rust

/// Points CR3 at the kernel's own tables (before a program's are freed).
pub fn use_kernel_tables(pml4: u64) {
    // SAFETY: the kernel's tables map everything the kernel uses.
    unsafe {
````

with:

````rust

/// Points CR3 at the tables whose PML4 is at `pml4`: the kernel's own
/// (before a program's are freed), or any that share the kernel's half.
pub fn use_tables(pml4: u64) {
    // SAFETY: every table the kernel makes maps its upper half.
    unsafe {
````

- [ ] **Step 10: Implement `kernel/src/arch/cpu.rs`**

Insert this at the top of `kernel/src/arch/cpu.rs`, above `#[cfg(test)]`:

````rust
//! What the CPU enforces for the kernel (user-space gate §5.1): SMEP and
//! SMAP, where CPUID reports them. The kernel copies a program's memory
//! through the linear map (`UserSlice`), never at the program's own
//! addresses, and runs none of a program's code, so with them on any
//! access to a program's page from ring 0 is a kernel bug that faults (the
//! panic screen) instead of passing silently.

use core::fmt;
use x86_64::registers::control::{Cr4, Cr4Flags};

/// CPUID leaf 7's EBX bits.
const SMEP: u32 = 1 << 7;
const SMAP: u32 = 1 << 20;

/// What `init` turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protection {
    pub smep: bool,
    pub smap: bool,
}

impl fmt::Display for Protection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let on = |b| if b { "on" } else { "not available" };
        write!(f, "SMEP {}, SMAP {}", on(self.smep), on(self.smap))
    }
}

/// SMEP and SMAP as CPUID reports them: `max_leaf` is leaf 0's EAX, and
/// `leaf7_ebx` leaf 7's (subleaf 0) EBX.
pub fn features(max_leaf: u32, leaf7_ebx: u32) -> Protection {
    let ebx = if max_leaf >= 7 { leaf7_ebx } else { 0 };
    Protection {
        smep: ebx & SMEP != 0,
        smap: ebx & SMAP != 0,
    }
}

/// CR4 with SMEP (bit 20) and SMAP (bit 21) set as `p` says.
pub fn cr4_with(cr4: u64, p: Protection) -> u64 {
    let mut cr4 = cr4 & !(1 << 20 | 1 << 21);
    if p.smep {
        cr4 |= 1 << 20;
    }
    if p.smap {
        cr4 |= 1 << 21;
    }
    cr4
}

/// Turns SMEP and SMAP on where the CPU has them.
pub fn init() -> Protection {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    let max = __cpuid(0).eax;
    let ebx = if max >= 7 { __cpuid_count(7, 0).ebx } else { 0 };
    let p = features(max, ebx);
    let cr4 = cr4_with(Cr4::read_raw(), p);
    // SAFETY: only bits CPUID reports are set; the kernel touches no
    // program's page.
    unsafe { Cr4::write(Cr4Flags::from_bits_retain(cr4)) };
    p
}

````

- [ ] **Step 11: Change `kernel/src/cmdline.rs`**

In `kernel/src/cmdline.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    StackOverflow,
}
````

with:

````rust
    StackOverflow,
    /// The kernel reads a program's page (SMAP must stop it).
    UserRead,
    /// The kernel runs a program's code (SMEP must stop it).
    UserExec,
}
````

Replace:

````rust
    pub test_mode: bool,
    /// `panic=early|pagefault|ud|panic|stack`: deliberately crash (exercises
    /// the panic screen in tests). `early` faults before the console starts,
    /// the others after boot.
    pub panic_test: Option<PanicTest>,
````

with:

````rust
    pub test_mode: bool,
    /// `panic=early|pagefault|ud|panic|stack|user-read|user-exec`:
    /// deliberately crash (exercises the panic screen in tests). `early`
    /// faults before the console starts, the others after boot.
    pub panic_test: Option<PanicTest>,
````

Replace:

````rust
                        "stack" => Some(PanicTest::StackOverflow),
                        _ => None,
````

with:

````rust
                        "stack" => Some(PanicTest::StackOverflow),
                        "user-read" => Some(PanicTest::UserRead),
                        "user-exec" => Some(PanicTest::UserExec),
                        _ => None,
````

- [ ] **Step 12: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    arch::user::init();
    let cmdline = Cmdline::parse(info.cmdline());
````

with:

````rust
    arch::user::init();
    let protection = arch::cpu::init();
    let cmdline = Cmdline::parse(info.cmdline());
````

Replace:

````rust
    console::ok(format_args!("cpu tables"));

````

with:

````rust
    console::ok(format_args!("cpu tables"));
    klogln!("cpu: {protection}");

````

Replace:

````rust
            recurse(0);
        }
    }
}
````

with:

````rust
            recurse(0);
        }
        PanicTest::UserRead | PanicTest::UserExec => user_page(t),
    }
}

/// `panic=user-read|user-exec`: the kernel reads a program's page, or runs
/// the `ret` on it; SMAP or SMEP turns either into a page fault in ring 0,
/// the panic screen. Without them it returns, and says so.
fn user_page(t: PanicTest) {
    use mm::paging::Perm;
    const AT: u64 = elf::PROGRAM_BASE;
    let space = mm::with_user_memory(|mem, kernel| {
        let mut s = mm::space::AddressSpace::new(mem, kernel).ok()?;
        s.map_zeroed(mem, AT, 1, Perm::ReadExec).ok()?;
        s.fill(mem, AT, &[0xC3]).ok()?;
        Some(s)
    })
    .expect("a page for the test");
    arch::context::use_tables(space.pml4());
    if t == PanicTest::UserRead {
        // SAFETY: none; the point is that SMAP faults.
        unsafe { core::ptr::read_volatile(AT as *const u8) };
        kprintln!("relay: the kernel read a program's page: no SMAP");
    } else {
        // SAFETY: none; the point is that SMEP faults (it is a `ret`).
        let f: extern "C" fn() = unsafe { core::mem::transmute(AT as usize) };
        f();
        kprintln!("relay: the kernel ran a program's code: no SMEP");
    }
}
````

- [ ] **Step 13: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        // kernel's half, which every table maps.
        context::use_kernel_tables(mm::kernel_pml4());
        mm::with_user_memory(|mem, _| space.destroy(mem));
````

with:

````rust
        // kernel's half, which every table maps.
        context::use_tables(mm::kernel_pml4());
        mm::with_user_memory(|mem, _| space.destroy(mem));
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 271 tests.

- [ ] **Step 15: Run the `panic_smap`, `panic_smep`, `userfault` scenarios**

Run: `cargo xtask test --e2e-only --scenario panic_smap`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario panic_smep`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: SMEP and SMAP where the CPU has them; t-fault flags-ac spins with AC through ticks"
````


### Task 22: CR0.TS: a program's FPU and SSE instructions fault; `t-fault sse`

Spec §5.5: the kernel saves no FPU state, so CR0.TS stays set (with MP, and EM clear): any x87, MMX or SSE instruction raises #NM, and the program is killed with `killed (FPU/SSE instruction, ip …)` and status 136. The kernel is built without floating point, so TS is set once at boot (decision 13), and the x87/SSE state no longer passes from one program to the next (plan 2's deferred minor). `t-fault sse` runs `xorps`; the `userfault` scenario expects its kill (the red run prints `t-fault: no fault`). Mutation check: CR0 not written fails `userfault`.

**Files:**
- Modify: `kernel/src/arch/cpu.rs`
- Modify: `tests/e2e/userfault.txt`
- Modify: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Task 21 (`arch::cpu::init`); plan 2's `FAULT_FPU`.
- Produces: `arch::cpu::cr0_with(u64) -> u64`; `t-fault sse`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/cpu.rs`**

In `kernel/src/arch/cpu.rs`, replace:

````rust
    #[test]
    fn cr4_gets_bits_20_and_21_and_keeps_the_rest() {
````

with:

````rust
    #[test]
    fn cr0_makes_fpu_and_sse_instructions_fault_with_nm() {
        // PE, ET, NE, WP, PG as a UEFI loader leaves them, with EM set.
        let firmware = 0x8005_0031 | 1 << 2;
        assert_eq!(cr0_with(firmware), 0x8005_0031 | 1 << 3 | 1 << 1);
        assert_eq!(cr0_with(0), 0b1010);
    }

    #[test]
    fn cr4_gets_bits_20_and_21_and_keeps_the_rest() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/userfault.txt`**

In `tests/e2e/userfault.txt`, replace:

````text
expect \nrelay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)\n
send t-fault stack
````

with:

````text
expect \nrelay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)\n
send t-fault sse
expect \nrelay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)\n
send t-fault stack
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-fault.rs`**

In `userland/tests/src/bin/t-fault.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! kernel's interrupt handlers must not run with them either (AC would
//! switch SMAP off).
#![no_std]
````

with:

````rust
//! kernel's interrupt handlers must not run with them either (AC would
//! switch SMAP off). `sse` runs an SSE instruction, which CR0.TS makes a
//! fault (spec §5.5).
#![no_std]
````

Replace:

````rust
            b"ud" => asm!("ud2"),
            b"div0" => asm!(
````

with:

````rust
            b"ud" => asm!("ud2"),
            b"sse" => asm!("xorps xmm0, xmm0"),
            b"div0" => asm!(
````

Replace:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac\n",
                );
````

with:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|sse\n",
                );
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::cpu`

Expected: FAIL: compile errors such as `` cannot find function `cr0_with` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: FAIL: scenario `userfault` stops at line 25, timed out waiting for `\nrelay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)\n`.

- [ ] **Step 5: Change `kernel/src/arch/cpu.rs`**

In `kernel/src/arch/cpu.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! What the CPU enforces for the kernel (user-space gate §5.1): SMEP and
//! SMAP, where CPUID reports them. The kernel copies a program's memory
//! through the linear map (`UserSlice`), never at the program's own
//! addresses, and runs none of a program's code, so with them on any
//! access to a program's page from ring 0 is a kernel bug that faults (the
//! panic screen) instead of passing silently.

use core::fmt;
use x86_64::registers::control::{Cr4, Cr4Flags};

````

with:

````rust
//! What the CPU enforces (user-space gate §5.1, §5.5): SMEP and SMAP, where
//! CPUID reports them, and CR0.TS.
//!
//! - The kernel copies a program's memory through the linear map
//!   (`UserSlice`), never at the program's own addresses, and runs none of
//!   a program's code, so with SMEP and SMAP on any access to a program's
//!   page from ring 0 is a kernel bug that faults (the panic screen)
//!   instead of passing silently.
//! - The kernel is built without floating point and saves no FPU state,
//!   so CR0.TS stays set: any x87, MMX or SSE instruction (and `fwait`,
//!   with MP) raises #NM, and a program that uses one is killed instead of
//!   reading or corrupting another's registers.

use core::fmt;
use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

````

Replace:

````rust

/// Turns SMEP and SMAP on where the CPU has them.
pub fn init() -> Protection {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
````

with:

````rust

/// CR0 with TS (bit 3) and MP (bit 1) set and EM (bit 2) clear: FPU and
/// SSE instructions raise #NM, not #UD.
pub fn cr0_with(cr0: u64) -> u64 {
    (cr0 | 1 << 3 | 1 << 1) & !(1 << 2)
}

/// Turns SMEP and SMAP on where the CPU has them, and sets CR0.TS.
pub fn init() -> Protection {
    // SAFETY: the kernel runs no FPU or SSE instruction.
    unsafe { Cr0::write(Cr0Flags::from_bits_retain(cr0_with(Cr0::read_raw()))) };
    use core::arch::x86_64::{__cpuid, __cpuid_count};
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::cpu`

Expected: PASS: 3 tests.

- [ ] **Step 7: Run the `userfault` scenario**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: CR0.TS makes a program's FPU and SSE instructions fault; t-fault sse"
````


### Task 23: CR4.FSGSBASE stays clear: a program cannot set its own `gs` base; `t-fault gsbase`

The prototype's review declined to judge it, and it is real: CR4.FSGSBASE was left as the firmware set it. With it set, a program could set its own `gs` base (`wrgsbase`), which `swapgs` keeps in `KernelGsBase` while the kernel runs, and a switch saves no segment bases, so the next program would get it; the comment in `arch/user.rs` that the program's `gs` base is always 0 relies on it being clear. `cr4_with` now clears bit 16 too (decision 13). `t-fault gsbase` runs `wrgsbase`, an invalid opcode while FSGSBASE is clear; QEMU's firmware leaves it clear, so the `userfault` scenario passes on the old code, and the unit test is the red run. Mutation checks: FSGSBASE forced on fails `userfault` (`t-fault: no fault`), and not cleared fails the unit test; the NUC's check script runs `t-fault gsbase` too (Task 27), since only the NUC shows what its firmware leaves.

**Files:**
- Modify: `kernel/src/arch/cpu.rs`
- Modify: `tests/e2e/userfault.txt`
- Modify: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Tasks 21, 22 (`arch::cpu`).
- Produces: `cr4_with`'s bit 16 cleared; `t-fault gsbase`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/cpu.rs`**

In `kernel/src/arch/cpu.rs`, replace:

````rust
        assert_eq!(cr4_with(0, smep), 1 << 20);
    }
````

with:

````rust
        assert_eq!(cr4_with(0, smep), 1 << 20);
        // FSGSBASE, as a firmware may leave it.
        assert_eq!(cr4_with(0x6F0 | 1 << 16, p), 0x6F0 | 3 << 20);
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/userfault.txt`**

In `tests/e2e/userfault.txt`, replace:

````text
expect \nrelay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)\n
send t-fault stack
````

with:

````text
expect \nrelay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)\n
send t-fault gsbase
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
send t-fault stack
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-fault.rs`**

In `userland/tests/src/bin/t-fault.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! switch SMAP off). `sse` runs an SSE instruction, which CR0.TS makes a
//! fault (spec §5.5).
#![no_std]
````

with:

````rust
//! switch SMAP off). `sse` runs an SSE instruction, which CR0.TS makes a
//! fault (spec §5.5). `gsbase` tries to set its own `gs` base, an invalid
//! opcode while CR4.FSGSBASE is clear.
#![no_std]
````

Replace:

````rust
            b"sse" => asm!("xorps xmm0, xmm0"),
            b"div0" => asm!(
````

with:

````rust
            b"sse" => asm!("xorps xmm0, xmm0"),
            b"gsbase" => asm!("wrgsbase {0}", in(reg) 0x1000u64),
            b"div0" => asm!(
````

Replace:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|sse\n",
                );
````

with:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|sse|gsbase\n",
                );
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::cpu`

Expected: FAIL: 1 test fails: `arch::cpu::tests::cr4_gets_bits_20_and_21_and_keeps_the_rest`.

- [ ] **Step 5: Change `kernel/src/arch/cpu.rs`**

In `kernel/src/arch/cpu.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! What the CPU enforces (user-space gate §5.1, §5.5): SMEP and SMAP, where
//! CPUID reports them, and CR0.TS.
//!
````

with:

````rust
//! What the CPU enforces (user-space gate §5.1, §5.5): SMEP and SMAP, where
//! CPUID reports them, CR0.TS, and no `fs`/`gs` base of a program's own.
//!
````

Replace:

````rust
//!   reading or corrupting another's registers.

````

with:

````rust
//!   reading or corrupting another's registers.
//! - CR4.FSGSBASE stays clear, whatever the firmware left: with it a program
//!   could set its own `gs` base (`wrgsbase`), which `swapgs` would carry
//!   into the next program, since a switch saves no segment bases.

````

Replace:

````rust

/// CR4 with SMEP (bit 20) and SMAP (bit 21) set as `p` says.
pub fn cr4_with(cr4: u64, p: Protection) -> u64 {
    let mut cr4 = cr4 & !(1 << 20 | 1 << 21);
    if p.smep {
````

with:

````rust

/// CR4 with SMEP (bit 20) and SMAP (bit 21) set as `p` says, and FSGSBASE
/// (bit 16) clear.
pub fn cr4_with(cr4: u64, p: Protection) -> u64 {
    let mut cr4 = cr4 & !(1 << 20 | 1 << 21 | 1 << 16);
    if p.smep {
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::cpu`

Expected: PASS: 3 tests.

- [ ] **Step 7: Run the `userfault` scenario**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: CR4.FSGSBASE stays clear, so a program cannot set its own gs base; t-fault gsbase"
````


### Task 24: `t-fault`: the trap flag before an exit and before a call that returns

Plan 2's final review recommended a test with the trap flag. `flags-exit` now sets TF too before its `exit`: `SFMASK` clears TF, and no single-step trap may come in ring 0 after `syscall`. `flags-tf` sets TF and makes a call that returns (`getpid`): the step traps after `sysret`, in ring 3, and kills the program as `CPU exception 1` (status 139). KVM and TCG agree on both. This task only adds the test: the code already does the right thing, which the mutation check shows (`SFMASK` without TF: a double fault in ring 0 during `flags-exit`, the panic screen).

**Files:**
- Modify: `tests/e2e/userfault.txt`
- Modify: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Tasks 7, 21 (`t-fault`'s kinds).
- Produces: `t-fault flags-tf`; `flags-exit` with TF.

- [ ] **Step 1: Expect the new lines in `tests/e2e/userfault.txt`**

In `tests/e2e/userfault.txt`, replace:

````text
expect \n\[1\] after-a-fault\n
send t-fault flags-ac
````

with:

````text
expect \n\[1\] after-a-fault\n
send t-fault flags-tf
expect \nrelay-sh: t-fault: killed \(CPU exception 1, ip 0x4[0-9a-f]+\)\n
send t-args after-the-trap-flag
expect \n\[1\] after-the-trap-flag\n
send t-fault flags-ac
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-fault.rs`**

In `userland/tests/src/bin/t-fault.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program. `flags-ac` sets
//! them and spins through a few hundred timer ticks before it faults: the
````

with:

````rust
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program (`flags-exit`
//! sets the trap flag too). `flags-tf` sets the trap flag and makes a
//! call that returns: the single step traps after the return, in ring 3,
//! which kills the program. `flags-ac` sets
//! them and spins through a few hundred timer ticks before it faults: the
````

Replace:

````rust
const FLAGS: u64 = (1 << 10) | (1 << 14) | (1 << 18);

````

with:

````rust
const FLAGS: u64 = (1 << 10) | (1 << 14) | (1 << 18);
/// RFLAGS' trap flag.
const TF: u64 = 1 << 8;

````

Replace:

````rust
            }
            // `exit(0)` at once: no Rust code runs with these flags.
            b"flags-exit" => asm!(
````

with:

````rust
            }
            // `exit(0)` at once: no Rust code runs with these flags, and
            // the trap flag's step after `syscall` is not the kernel's.
            b"flags-exit" => asm!(
````

Replace:

````rust
                "syscall",
                flags = in(reg) FLAGS,
                in("rax") 1u64,
````

with:

````rust
                "syscall",
                flags = in(reg) FLAGS | TF,
                in("rax") 1u64,
````

Replace:

````rust
            ),
            b"flags-ud" => asm!(
````

with:

````rust
            ),
            // The trap flag, then a call that returns: the single-step
            // trap comes after `sysret`, in ring 3.
            b"flags-tf" => asm!(
                "pushfq",
                "or qword ptr [rsp], {tf}",
                "popfq",
                "syscall",
                "ud2",
                tf = in(reg) TF,
                in("rax") 5u64,
                options(noreturn),
            ),
            b"flags-ud" => asm!(
````

Replace:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|sse|gsbase\n",
                );
````

with:

````rust
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud|flags-ac|flags-tf|sse|gsbase\n",
                );
````

- [ ] **Step 3: Run the `userfault` scenario**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests userland
git commit -m "t-fault: the trap flag before exit and before a call that returns"
````


### Task 25: A kernel stack without frames is `ENOMEM`, and NMIs, double faults and machine checks have an IST stack each

Two of plan 2's deferred minors (decision 15). `KernelStacks::alloc` said `None` both when every slot was in use and when there were no frames, and `spawn` made both `EAGAIN`; now it says which (`StackError::NoSlot`, `NoMemory`), and `errno` makes them `EAGAIN` (a full table: the slots are one per entry) and `ENOMEM`. NMIs, double faults and machine checks shared IST1, so one arriving while another's handler ran overwrote its frame on the panic screen; now each has its own 16 KiB stack (IST1 for #DF, IST2 for NMI, IST3 for #MC). Mutation checks: a stack without frames as `EAGAIN` and NMIs back on the double fault's stack each fail a test.

**Files:**
- Modify: `kernel/src/arch/gdt.rs`
- Modify: `kernel/src/arch/idt.rs`
- Modify: `kernel/src/mm/kstack.rs`
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/proc.rs`

**Interfaces:**
- Consumes: plan 2's `mm::kstack`, `arch::{gdt, idt}`.
- Produces: `mm::kstack::StackError::{NoSlot, NoMemory}` with `errno()`, `KernelStacks::alloc -> Result<KernelStack, StackError>`, `mm::alloc_kernel_stack -> Result<KernelStack, StackError>`; `arch::gdt::{NMI_IST, MACHINE_CHECK_IST}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/gdt.rs`**

In `kernel/src/arch/gdt.rs`, replace:

````rust
    #[test]
    fn the_segments_are_in_the_order_sysret_needs() {
````

with:

````rust
    #[test]
    fn each_ist_stack_is_its_own() {
        let tops = [DOUBLE_FAULT_IST, NMI_IST, MACHINE_CHECK_IST].map(ist_top);
        assert_eq!(
            tops[0],
            &raw const IST_STACKS as u64 + IST_STACK_SIZE as u64
        );
        assert_eq!(tops[1] - tops[0], IST_STACK_SIZE as u64);
        assert_eq!(tops[2] - tops[1], IST_STACK_SIZE as u64);
        assert!(tops.iter().all(|t| t % 16 == 0));
    }

    #[test]
    fn the_segments_are_in_the_order_sysret_needs() {
````

- [ ] **Step 2: Add the failing tests to `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, replace:

````rust
        assert_eq!(g[8].ist, 1, "double fault on IST1");
        assert_eq!(g[2].ist, 1, "NMI on IST1");
        assert_eq!(g[18].ist, 1, "machine check on IST1");
        let others = (0..256).filter(|v| ![2, 8, 18].contains(v));
````

with:

````rust
        assert_eq!(g[8].ist, 1, "double fault on IST1");
        assert_eq!(g[2].ist, 2, "NMI on IST2");
        assert_eq!(g[18].ist, 3, "machine check on IST3");
        let others = (0..256).filter(|v| ![2, 8, 18].contains(v));
````

- [ ] **Step 3: Add the failing tests to `kernel/src/mm/kstack.rs`**

In `kernel/src/mm/kstack.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        assert_eq!(all.last().unwrap().slot, SLOTS - 1);
        assert_eq!(stacks.alloc(&mut t, &mut m), None);
        for s in all {
            stacks.free(s, &mut t, &mut m);
        }
        assert!(stacks.alloc(&mut t, &mut m).is_some());
    }
````

with:

````rust
        assert_eq!(all.last().unwrap().slot, SLOTS - 1);
        assert_eq!(stacks.alloc(&mut t, &mut m), Err(StackError::NoSlot));
        assert_eq!(
            StackError::NoSlot.errno(),
            vfs::Errno::EAGAIN,
            "a full table"
        );
        for s in all {
            stacks.free(s, &mut t, &mut m);
        }
        assert!(stacks.alloc(&mut t, &mut m).is_ok());
    }
````

Replace:

````rust
        m.limit = cold + 1;
        assert_eq!(stacks.alloc(&mut t, &mut m), None);
        assert_eq!(m.frames(), cold);
````

with:

````rust
        m.limit = cold + 1;
        assert_eq!(stacks.alloc(&mut t, &mut m), Err(StackError::NoMemory));
        assert_eq!(StackError::NoMemory.errno(), vfs::Errno::ENOMEM);
        assert_eq!(m.frames(), cold);
````

Replace:

````rust
            m.limit = before + extra;
            assert_eq!(stacks.alloc(&mut t, &mut m), None, "{extra} frames");
            assert_eq!(m.frames(), before);
        }
        m.limit = usize::MAX;
        assert_eq!(stacks.alloc(&mut t, &mut m), Some(KernelStack { slot: 0 }));
    }
````

with:

````rust
            m.limit = before + extra;
            assert_eq!(
                stacks.alloc(&mut t, &mut m),
                Err(StackError::NoMemory),
                "{extra} frames"
            );
            assert_eq!(m.frames(), before);
        }
        m.limit = usize::MAX;
        assert_eq!(stacks.alloc(&mut t, &mut m), Ok(KernelStack { slot: 0 }));
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `NMI_IST` in this scope ``; `` cannot find value `MACHINE_CHECK_IST` in this scope ``.

- [ ] **Step 5: Change `kernel/src/arch/gdt.rs`**

In `kernel/src/arch/gdt.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! the stack the CPU switches to when an interrupt or exception arrives in
//! ring 3, and IST1 the stack for double faults, NMIs and machine checks
//! (so a kernel stack overflow, or an NMI taken with a program's stack
//! pointer, still reaches the panic screen).

````

with:

````rust
//! the stack the CPU switches to when an interrupt or exception arrives in
//! ring 3, and IST1-3 the stacks for double faults, NMIs and machine checks,
//! one each (so a kernel stack overflow, or an NMI taken with a program's
//! stack pointer, still reaches the panic screen, and one arriving while
//! another's handler runs does not overwrite its frame).

````

Replace:

````rust

/// IST index (1-based in the gate, 0-based in the TSS table).
pub const DOUBLE_FAULT_IST: u8 = 1;
const IST_STACK_SIZE: usize = 16 * 1024;
````

with:

````rust

/// IST indexes (1-based in the gate, 0-based in the TSS table).
pub const DOUBLE_FAULT_IST: u8 = 1;
pub const NMI_IST: u8 = 2;
pub const MACHINE_CHECK_IST: u8 = 3;
const IST_STACK_SIZE: usize = 16 * 1024;
````

Replace:

````rust
struct Stack([u8; IST_STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: Stack = Stack([0; IST_STACK_SIZE]);

````

with:

````rust
struct Stack([u8; IST_STACK_SIZE]);
/// The IST stacks, in IST order.
static mut IST_STACKS: [Stack; 3] = [const { Stack([0; IST_STACK_SIZE]) }; 3];

/// The top of IST stack `ist` (1-3).
fn ist_top(ist: u8) -> u64 {
    let stacks = &raw const IST_STACKS;
    stacks as u64 + u64::from(ist) * IST_STACK_SIZE as u64
}

````

Replace:

````rust
    unsafe {
        let top = &raw const DOUBLE_FAULT_STACK as u64 + IST_STACK_SIZE as u64;
        (*tss).interrupt_stack_table[(DOUBLE_FAULT_IST - 1) as usize] = VirtAddr::new(top);
    }
````

with:

````rust
    unsafe {
        for ist in [DOUBLE_FAULT_IST, NMI_IST, MACHINE_CHECK_IST] {
            (*tss).interrupt_stack_table[(ist - 1) as usize] = VirtAddr::new(ist_top(ist));
        }
    }
````

- [ ] **Step 6: Change `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, replace:

````rust

/// Vectors that run on the IST stack whatever the stack pointer was: a
/// double fault (the kernel stack may have overflowed), an NMI and a
/// machine check (they may arrive between `syscall` and the switch to the
/// kernel stack, or just before `sysret`, with a program's stack pointer).
const IST_VECTORS: [usize; 3] = [2, 8, 18];

/// Gates for the 32 exceptions (three on the IST stack) and for every
/// hardware interrupt vector, 32-255.
fn gates(selector: u16) -> [Gate; 256] {
    let mut gates = [Gate::MISSING; 256];
    for (v, stub) in STUBS.iter().enumerate() {
        let ist = if IST_VECTORS.contains(&v) {
            super::gdt::DOUBLE_FAULT_IST
        } else {
            0
        };
        gates[v] = Gate::new(*stub as *const () as u64, selector, ist);
    }
````

with:

````rust

/// The IST stack of a vector that runs on one whatever the stack pointer
/// was, 0 for the others: a double fault (the kernel stack may have
/// overflowed), an NMI and a machine check (they may arrive between
/// `syscall` and the switch to the kernel stack, or just before `sysret`,
/// with a program's stack pointer), each on its own, so one arriving while
/// another's handler runs keeps that one's frame.
fn ist(vector: usize) -> u8 {
    match vector {
        8 => super::gdt::DOUBLE_FAULT_IST,
        2 => super::gdt::NMI_IST,
        18 => super::gdt::MACHINE_CHECK_IST,
        _ => 0,
    }
}

/// Gates for the 32 exceptions (three on IST stacks) and for every hardware
/// interrupt vector, 32-255.
fn gates(selector: u16) -> [Gate; 256] {
    let mut gates = [Gate::MISSING; 256];
    for (v, stub) in STUBS.iter().enumerate() {
        gates[v] = Gate::new(*stub as *const () as u64, selector, ist(v));
    }
````

- [ ] **Step 7: Change `kernel/src/mm/kstack.rs`**

In `kernel/src/mm/kstack.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

/// Which slots are in use.
````

with:

````rust

/// Why there is no kernel stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackError {
    /// Every slot is in use: one per process-table entry, so the table is
    /// full.
    NoSlot,
    /// No frames for it.
    NoMemory,
}

impl StackError {
    /// What `spawn` says: `EAGAIN` for a full table, `ENOMEM` when the
    /// memory ran out.
    pub fn errno(self) -> vfs::Errno {
        match self {
            StackError::NoSlot => vfs::Errno::EAGAIN,
            StackError::NoMemory => vfs::Errno::ENOMEM,
        }
    }
}

/// Which slots are in use.
````

Replace:

````rust

    /// Maps a stack in a free slot; `None` when every slot is in use or
    /// there are no frames (what was mapped is given back).
    pub fn alloc(
        &mut self,
        tables: &mut PageTables,
        mem: &mut impl PhysMem,
    ) -> Option<KernelStack> {
        let slot = (0..SLOTS).find(|&s| self.used & (1 << s) == 0)?;
        let stack = KernelStack { slot };
````

with:

````rust

    /// Maps a stack in a free slot; `NoSlot` when every slot is in use,
    /// `NoMemory` when there are no frames (what was mapped is given back).
    pub fn alloc(
        &mut self,
        tables: &mut PageTables,
        mem: &mut impl PhysMem,
    ) -> Result<KernelStack, StackError> {
        let slot = (0..SLOTS)
            .find(|&s| self.used & (1 << s) == 0)
            .ok_or(StackError::NoSlot)?;
        let stack = KernelStack { slot };
````

Replace:

````rust
                release(&stack, n, tables, mem);
                return None;
            }
        }
        self.used |= 1 << slot;
        Some(stack)
    }
````

with:

````rust
                release(&stack, n, tables, mem);
                return Err(StackError::NoMemory);
            }
        }
        self.used |= 1 << slot;
        Ok(stack)
    }
````

- [ ] **Step 8: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust

/// A kernel stack for a program (user-space gate §5.4), from the frames
/// the kernel keeps for itself; `None` when every slot is in use.
pub fn alloc_kernel_stack() -> Option<KernelStack> {
    let mut guard = MEMORY.lock();
````

with:

````rust

/// A kernel stack for a process (user-space gate §5.4), from the frames
/// the kernel keeps for itself.
pub fn alloc_kernel_stack() -> Result<KernelStack, kstack::StackError> {
    let mut guard = MEMORY.lock();
````

- [ ] **Step 9: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
        })?;
    let stack = mm::alloc_kernel_stack().ok_or(Errno::EAGAIN)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
````

with:

````rust
        })?;
    let stack = mm::alloc_kernel_stack().map_err(kstack::StackError::errno)?;
    let loaded = mm::with_user_memory(|mem, kernel| {
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 273 tests.

- [ ] **Step 11: Run the `panic_stack`, `programs` scenarios**

Run: `cargo xtask test --e2e-only --scenario panic_stack`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add kernel
git commit -m "kernel: a kernel stack without frames is ENOMEM, and NMIs, double faults and machine checks have an IST stack each"
````


### Task 26: Argument 0 is the name as typed, and a name too long for a file is not found

Two of plan 2's deferred minors in the shell (decision 12). Argument 0 was `/bin/<name>` for a bare name; `SpawnArgs` carries the path and the arguments apart, so argument 0 is now what the caller gives, and the in-kernel shell gives the word as typed, as bash does (spec §5.3 corrected in its body); a program's messages still use its last path name (`Args::name`). A bare name whose lookup in `/bin` fails with `ENAMETOOLONG` said `File name too long` (126); bash says `command not found` (127), and so does the shell now; given as a path, it keeps the path's error. Mutation checks: argument 0 back to the path and `ENAMETOOLONG` reported as the error each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/args.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `userland/tests/src/bin/t-args.rs`

**Interfaces:**
- Consumes: plan 2's `Shell::run_program`.
- Produces: the shell's argument 0 and `ENAMETOOLONG` rule.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            [vec![
                b"/bin/t-args".to_vec(),
                b"a".to_vec(),
                b"b c".to_vec(),
                b"".to_vec()
            ]]
        );
        // A path runs as given; argument 0 is the path as typed.
        assert_eq!(h.run("/bin/t-args").0, 3);
````

with:

````rust
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
````

Replace:

````rust
        assert_eq!(h.run("./"), (126, "relay-sh: ./: Is a directory\n".into()));
    }
````

with:

````rust
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
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-args.rs`**

In `userland/tests/src/bin/t-args.rs`, replace:

````rust
//! `t-args [ARG]...`: prints each argument after the program's path as
//! `[n] <arg>`, one per line (spec §8.5), so a scenario sees exactly what
````

with:

````rust
//! `t-args [ARG]...`: prints each argument after argument 0 as
//! `[n] <arg>`, one per line (spec §8.5), so a scenario sees exactly what
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `shell::tests::a_name_that_is_no_built_in_runs_from_bin`, `shell::tests::a_name_too_long_for_a_file_is_not_found`.

- [ ] **Step 4: Change `crates/relay-rt/src/args.rs`**

In `crates/relay-rt/src/args.rs`, replace:

````rust
//! The arguments a program was started with (spec §5.3): `count` byte
//! strings, each followed by a NUL; argument 0 is the program's path as
//! given to `spawn`.

````

with:

````rust
//! The arguments a program was started with (spec §5.3): `count` byte
//! strings, each followed by a NUL; argument 0 is what the program that
//! started it gave (the shells give the command's name as typed, as bash
//! does).

````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    /// Runs a program (user-space gate §8.2): `/bin/<name>`, or `name`
    /// itself when it holds a `/`, with the words after it as arguments.
    /// Its fd 1 is standard output (the redirection file, if any), its fd 2
    /// the screen; a running script's transcript gets both.
    fn run_program(&mut self, name: &str, words: &[String], file: Option<(Node, u64)>) -> i32 {
````

with:

````rust
    /// Runs a program (user-space gate §8.2): `/bin/<name>`, or `name`
    /// itself when it holds a `/`, with `name` as argument 0 and the words
    /// after it as the others, as bash does. Its fd 1 is standard output
    /// (the redirection file, if any), its fd 2 the screen; a running
    /// script's transcript gets both.
    fn run_program(&mut self, name: &str, words: &[String], file: Option<(Node, u64)>) -> i32 {
````

Replace:

````rust
        };
        let mut args: Vec<&[u8]> = alloc::vec![path.as_bytes()];
        args.extend(words.iter().map(|w| w.as_bytes()));
````

with:

````rust
        };
        let mut args: Vec<&[u8]> = alloc::vec![name.as_bytes()];
        args.extend(words.iter().map(|w| w.as_bytes()));
````

Replace:

````rust
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            Some(Err(Errno::ENOENT | Errno::EISDIR)) if !name.contains('/') => {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
````

with:

````rust
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            // A name too long for a file name is no file in /bin either.
            Some(Err(Errno::ENOENT | Errno::EISDIR | Errno::ENAMETOOLONG))
                if !name.contains('/') =>
            {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 129 tests.

- [ ] **Step 7: Run the `programs` scenario**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates userland
git commit -m "shell: argument 0 is the name as typed, and a name too long for a file is not found"
````


### Task 27: NUC check 3 runs processes

Spec §12.4 and decision 18: `check3-a.sh` expects `dmesg`'s `cpu: SMEP on, SMAP on` and runs `t-spin 1` (a program that never makes a system call ends after its second), `t-spawn 100` (with a new `frames lost:` line, since a check script compares no two lines), `t-spawn kill`, `t-fault sse`, `t-fault flags-ac` and `t-fault gsbase` (whatever the NUC's firmware left in CR4). `docs/hardware-test.md` describes them and adds two steps by hand, since a script cannot type: typing during `t-spin 5`, and Ctrl-C of `t-spin`; its table of failures gets the ones these can show. The recorded transcripts in `xtask/fixtures/checks/` get the new lines by commands (they hold escape bytes), with representative values, until plan 3a's NUC check records real ones, as plans 1 and 2 did. The README says what milestone 2 does now.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `userland/tests/src/bin/t-spawn.rs`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`

**Interfaces:**
- Consumes: Tasks 12, 15, 21, 22, 23.
- Produces: `check3-a.sh` with the processes' lines; `t-spawn`'s `frames lost:`; the README and `docs/hardware-test.md` up to date.

- [ ] **Step 1: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, make these 2 replacements, top to bottom:

Replace:

````bash
#> \[ ok \] cpu tables
#qemu> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
````

with:

````bash
#> \[ ok \] cpu tables
#> cpu: SMEP on, SMAP on
#qemu> \[ ok \] boot info: \d+ MiB usable in \d+ regions, cmdline '.*'
````

Replace:

````bash
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)

````

with:

````bash
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)

# Processes and scheduling (milestone 2, plan 3a): t-spin never makes a
# system call and still ends after its second, the timer taking the CPU
# from it; a hundred children lose no frame; a child killed while its
# parent sleeps; an SSE instruction, a spin with AC set and a program
# setting its own gs base (whatever the firmware left in CR4), killed
# without harm to the kernel.
t-spin 1
#> \d+ iterations
t-spawn 100
#> free frames before: \d+
#> free frames after: \d+
#> frames lost: 0
t-spawn kill
#> t-spin: kill
#> kill 1: EPERM
#> kill 999999: ESRCH
#> pid above 1: true
t-fault sse
#> relay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)
t-fault flags-ac
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)
t-fault gsbase
#> relay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)

````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-spawn.rs`**

In `userland/tests/src/bin/t-spawn.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! `t-spawn N`: starts N children that exit at once, waiting for each, and
//! prints the free frames before and after (spec §8.5), so a scenario sees
//! that starting and ending a program leaks nothing. More kinds:
//!
````

with:

````rust
//! `t-spawn N`: starts N children that exit at once, waiting for each, and
//! prints the free frames before and after (spec §8.5), and how many were
//! lost, so a scenario sees that starting and ending a program leaks
//! nothing. More kinds:
//!
````

Replace:

````rust
    let _ = writeln!(Fd(1), "free frames after: {after}");
    Ok(())
````

with:

````rust
    let _ = writeln!(Fd(1), "free frames after: {after}");
    // For the NUC's check script, which compares no two lines.
    let lost = i128::from(before) - i128::from(after);
    let _ = writeln!(Fd(1), "frames lost: {lost}");
    Ok(())
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 4: Change `README.md`**

In `README.md`, replace:

````markdown
from `/bin` in ring 3, each in its own address space: the shell runs a
name it does not know (`t-args a b`) as a program.

````

with:

````markdown
from `/bin` in ring 3, each in its own address space: the shell runs a
name it does not know (`t-args a b`) as a program, programs start
programs, the timer shares the CPU among them, and Ctrl-C stops the
command that runs.

````

- [ ] **Step 5: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 4 replacements, top to bottom:

Replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 2 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 4 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

Replace:

````markdown
   arguments printed, then `relay-sh: t-fault: killed (page fault at 0x0,
   read, ip …)` and the script goes on), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

with:

````markdown
   arguments printed, then `relay-sh: t-fault: killed (page fault at 0x0,
   read, ip …)` and the script goes on), `t-spin 1` (a program that never
   makes a system call ends after its second: the timer takes the CPU from
   it), `t-spawn 100` (`frames lost: 0`), `t-spawn kill` (a child killed
   while its parent sleeps; process 1 cannot be killed), `t-fault sse`,
   `t-fault flags-ac` and `t-fault gsbase` (an SSE instruction, a spin with
   the alignment check flag set and a program setting its own `gs` base,
   all killed), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

Replace:

````markdown
   comes back after `+ df` and its two lines.
5. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10 and type `sh checks/check3-b.sh`: the files written before the
   restart are read back.
6. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
7. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 66 of 66 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

with:

````markdown
   comes back after `+ df` and its two lines.
   `dmesg`'s lines include `cpu: SMEP on, SMAP on`: the kernel would fault
   if it touched a program's page.
5. Two steps by hand (milestone 2, plan 3a; a script cannot type):
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
     K120. After five seconds its `… iterations` line comes, then `typed`:
     the keyboard is polled on every tick that interrupts a program.
   - `t-spin`, then Ctrl-C: `^C` and the prompt come back at once, and
     `dmesg` ends with `pid <n> (/bin/t-spin): killed: Ctrl-C`.

   Photograph the screen after both.
6. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10 and type `sh checks/check3-b.sh`: the files written before the
   restart are read back.
7. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
8. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, the tree
   lists `/root/notes/a`, `/root/notes/big` (8388608 bytes) and
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 72 of 72 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

Replace:

````markdown
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
| `relay-sh: t-args: Exec format error` | The kernel refused the program; `dmesg` shows `spawn /bin/t-args: <reason>` | `cargo xtask flash --kernel` from the same worktree as the kernel |
````

with:

````markdown
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
| `t-spin 1` never ends, or the keyboard stops during `t-spin 5` | The LAPIC timer's ticks do not reach ring 3 on this machine, or the tick's poll of the xHCI keyboard fails there | Photograph the screen; after a reboot, `dmesg` shows the `timer:` and `usb:` lines |
| The panic screen during `t-spawn` or `t-fault flags-ac` | A context switch, or an interrupt taken in ring 3, goes wrong on this CPU: the panic's message names the check that failed (`a program's flags reached a switch`, `TSS rsp0 and the syscall stack differ`, `an interrupt with the program's gs`) | Photograph the panic screen |
| `t-fault gsbase` prints `t-fault: no fault` | CR4.FSGSBASE is set: the firmware left it, and the kernel did not clear it | Note it; a program could set a `gs` base that would pass to the next program |
| `cpu: SMEP not available, SMAP not available` in `dmesg` | CPUID reports neither (the i7-1260P has both) | Note the `dmesg` line; the check scripts expect both on the NUC |
| `relay-sh: t-args: Exec format error` | The kernel refused the program; `dmesg` shows `spawn /bin/t-args: <reason>` | `cargo xtask flash --kernel` from the same worktree as the kernel |
````

- [ ] **Step 6: Put the new lines into the recorded NUC transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
lines = open(path, newline="").read().split("\n")
for marker, new in [('cpu tables', ['cpu: SMEP on, SMAP on']), ('relay-sh: t-fault: killed (page fault at 0x0, read', ['+ t-spin 1', '2493644800 iterations', '+ t-spawn 100', 'free frames before: 3998421', 'free frames after: 3998421', 'frames lost: 0', '+ t-spawn kill', 't-spin: kill', 'kill 1: EPERM', 'kill 999999: ESRCH', 'pid above 1: true', '+ t-fault sse', 'relay-sh: t-fault: killed (FPU/SSE instruction, ip 0x40011b)', '+ t-fault flags-ac', 'relay-sh: t-fault: killed (invalid opcode, ip 0x400349)', '+ t-fault gsbase', 'relay-sh: t-fault: killed (invalid opcode, ip 0x400131)'])]:
    at = [i for i, l in enumerate(lines) if marker in l]
    assert len(at) == 1, (marker, at)
    lines[at[0] + 1:at[0] + 1] = new
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 7: Put the new lines into the recorded QEMU transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.qemu.log'
lines = open(path, newline="").read().split("\n")
for marker, new in [('cpu tables', ['cpu: SMEP on, SMAP on']), ('relay-sh: t-fault: killed (page fault at 0x0, read', ['+ t-spin 1', '4517789696 iterations', '+ t-spawn 100', 'free frames before: 249241', 'free frames after: 249241', 'frames lost: 0', '+ t-spawn kill', 't-spin: kill', 'kill 1: EPERM', 'kill 999999: ESRCH', 'pid above 1: true', '+ t-fault sse', 'relay-sh: t-fault: killed (FPU/SSE instruction, ip 0x40011b)', '+ t-fault flags-ac', 'relay-sh: t-fault: killed (invalid opcode, ip 0x400349)', '+ t-fault gsbase', 'relay-sh: t-fault: killed (invalid opcode, ip 0x400131)'])]:
    at = [i for i, l in enumerate(lines) if marker in l]
    assert len(at) == 1, (marker, at)
    lines[at[0] + 1:at[0] + 1] = new
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 11 tests.

- [ ] **Step 9: Run the `checks`, `spawn` scenarios**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add README.md docs rootfs userland xtask
git commit -m "Check scripts: NUC check 3 runs t-spin, t-spawn and the new t-fault kinds, and expects SMEP and SMAP; two steps by hand"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 30 scenario(s) passed`.

````bash
git push -u origin m2p3a/cpu
gh pr create --draft --base main --head m2p3a/cpu --title "Milestone 2, plan 3a: What the CPU enforces, and processes on the NUC" --body-file - <<'EOF'
## What

Milestone 2, plan 3a, tasks 21–27: SMEP and SMAP where the CPU has them, with panic tests that show each is on; `t-fault flags-ac` spins with AC through ticks; CR0.TS makes a program's FPU and SSE instructions fault (`t-fault sse`); CR4.FSGSBASE stays clear, so a program cannot set its own `gs` base (`t-fault gsbase`); the trap flag before an exit and before a call that returns; a kernel stack without frames is `ENOMEM`, and NMIs, double faults and machine checks have an IST stack each; argument 0 is the name as typed, and a bare name too long for a file is `command not found`; `check3-a.sh` runs processes, with two steps by hand.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 3 of `docs/hardware-test.md`, run by the user with `cargo xtask flash --full` from this worktree; the result goes into the results log, and the recorded transcripts are replaced by the real ones in a follow-up
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3a/cpu --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to run NUC check 3 (`docs/hardware-test.md`) with `cargo xtask flash --full` from this worktree, and to report its results; the pull request stays a draft until they do. After a pass, mark it ready (`gh pr ready m2p3a/cpu`) and ask the user to review and merge. Do not merge it yourself. The real transcripts and the results-log row follow in a pull request of their own, as plans 1 and 2 did (`xtask/fixtures/checks/` from the stick, `docs/hardware-test.md`'s row). After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3a-cpu
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
