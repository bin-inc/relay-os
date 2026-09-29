# Milestone 2 · Plan 2: Ring 3 and one program — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs from `/bin` run in ring 3, each in its own address space: the in-kernel shell runs a name it does not know as a program and waits for it, `t-args a 'b c' ''` prints its three arguments (the first time `relay-rt`'s `_start` runs), and a fault in ring 3 ends the program, not the kernel. It ends with `cargo xtask ci` green and NUC check 3, by script, running `t-args` and `t-fault` on a stick written by `cargo xtask flash --full`.

**Architecture:** As in milestone 1 and plan 1, the logic is architecture-neutral and host-tested, and the hardware glue is thin: page tables, address spaces, kernel stacks and `UserSlice` (`kernel/src/mm/`) against a fake `PhysMem` that counts every frame; the ELF checks as `crates/elf`, shared by the kernel and the build; loading (`kernel/src/exec.rs`) and the system-call dispatcher (`kernel/src/syscall.rs`) over the same fake; the shell's way to programs (`shell::System::spawn`/`wait`) against fake programs. The x86_64 parts are in `kernel/src/arch/`: the GDT's user segments and TSS `rsp0`, `syscall`/`sysret` with `swapgs`, entering ring 3 and coming back (`arch/user.rs`), and classifying exceptions (`arch/fault.rs`). `kernel/src/proc.rs` joins them for one child at a time, run to completion inside `wait`; plan 3's process table and scheduler replace it.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; binutils' `readelf` and the host C library's error messages for tests; QEMU 8.2, e2fsprogs 1.47.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§3.1, §5.1–§5.4, §6.1, §6.2, §7.1–§7.3, §8.1, §8.2, §8.5, §11.1, §12, §13 step 2, §14, §16 item 2)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 2 of 5.

## In brief

- **Size.** 20 tasks in four code pull requests, plus this plan as PR 1. PRs 2 and 3 are host-tested code (the kernel only changes in `mm::init`); PR 4 runs the first program in ring 3; PR 5 adds faults and the NUC check.
- **Three corrections to the spec** (PR 1): a process's address space shares the kernel's whole upper half, entries 256–511, since the linear map and the heap are in 256–384 (decision 1, and spec §5.1's body); `syscall` clears NT too, and the way back to the kernel loads the kernel's flags (decision 2, and §6.2's body); and a system call that would return to a non-canonical address kills the program instead of using `iretq`, which faults in ring 0 as well (decision 3).
- **One child at a time, run to completion.** `spawn` and `wait` are the kernel's, for the in-kernel shell; from ring 3 only `exit` and `write` exist yet (decision 6). There is no scheduler, so a program that never ends hangs the shell until plan 3.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran throwaway tests and scenarios under KVM on the same CPU model as the NUC (an i7-1260P), and found 1 critical and 4 minor real defects; each is fixed in a task of its own, after the task it concerns, with a test that fails first:

| Finding (review) | Decision |
|---|---|
| Critical: a program that sets RFLAGS.NT (`popfq`) and calls `exit` makes the next program's `iretq` fault in ring 0 (the panic screen); a program's AC reaches ring 0 through a fault, which would switch SMAP off in plan 3 | Fixed, Task 18 |
| Minor: `..`, `.` and `''` say `Is a directory` (126) instead of milestone 1's and bash's `command not found` (127) | Fixed, Task 10 |
| Minor: `write` drops the good bytes before a page the program does not have (pieces counted from the buffer's start) | Fixed, Task 13 |
| Minor: a program over 16 MiB is refused with no reason in the kernel log | Fixed, Task 16 |
| Minor: a killed program whose redirected output hit a write error loses its report and status | Fixed, Task 19 |
| Declined to judge: the `sysenter` MSRs are never written | Zeroed, Task 18 |
| Declined to judge: `wait` holds a `&mut` to the running child while its system calls make another | One raw pointer throughout, Task 16 |
| Declined to judge: every other exception gives 139 (bash would give some another signal's status) | Ruled: the kinds are the ABI's, and the shell reads no architecture numbers (decision 9) |
| Declined to judge: `userfault` checks no `$?` (spec §12.3) | Ruled: no shell of milestone 2 prints `$?`; the statuses are host-tested (decision 12) |
| Declined to judge: the hand-edited transcripts' `ls -l /bin` lists only `t-args` | Ruled: the script's `...` lines allow it; the real transcripts replace them (decision 13) |

## Where this plan fits

Plan 2 implements spec §13 step 2 of the user-space gate. It builds on milestone 1 (the frame allocator, the kernel's page tables and linear map, the GDT and IDT, the in-kernel shell, the e2e runner, the check scripts) and on plan 1: `relay-rt` with its `_start`, `t-args`, `relay-abi`'s call and error numbers, `system.img` at `/bin`, and xtask's `readelf` check. It leaves plan 3 a kernel that runs one program to completion: the context it saves and restores (`enter`/`leave`) becomes plan 3's context switch, the kernel-stack area has a slot per process-table entry to come, and `UserSlice` never touches a program's addresses, so SMEP and SMAP can be enabled as they are (roadmap, "What plan 2 leaves for plan 3").

## Working conventions

- Plan 2 lands as **five pull requests** (table below). This plan, with the spec's §16 item 2 (and its §5.1 and §6.2 corrected) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-29-m2-plan-2-ring-3-and-one-program.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.

  One more comes as a command: a `python3` command that inserts lines into a recorded transcript (they hold escape bytes, which a markdown block cannot carry).
- Each task first adds its failing tests (unit tests in each file's test module; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. The mutation checks the prototype ran are named in each task's introduction where a guard could pass vacuously.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `b0395db` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p2/plan` | — | The spec's §16 item 2 and corrected §5.1 and §6.2, the roadmap's plan 2 and "What plan 2 leaves for plan 3", this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p2/memory` | 1–5 | New error numbers; user pages, `unmap`, teardown; address spaces over a fixed upper half; kernel stacks; `UserSlice` | `lint`, `unit`, `e2e` |
| 3 | `m2p2/elf` | 6–8 | `crates/elf`, shared with the build; ELF64, little-endian and 16 MiB on both sides; loading a program | `lint`, `unit`, `e2e` |
| 4 | `m2p2/ring3` | 9–16 | The shell's `spawn`/`wait`; user segments and TSS `rsp0`; the dispatcher; `expect-same`; `syscall`/`sysret` and ring 3; the `programs` scenario; three review fixes | `lint`, `unit`, `e2e` |
| 5 | `m2p2/faults` | 17–20 | Faults in ring 3, `t-fault`, the `userfault` scenario; a program's flags stay its own; NUC check 3 runs programs; NUC check 3 | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 2 adds no third-party crate. The kernel has no dev-dependencies. New host crates are `#![cfg_attr(not(test), no_std)]` and go in both `members` and `default-members`; user packages go in `members` only and in `USER_PACKAGES`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. A bad pointer is `EFAULT`, a bad program `ENOEXEC`, a fault in ring 3 kills only the program. Panics are for kernel bugs only.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils and bash.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 2 in PR 1:

1. **The kernel's whole upper half is shared** (§5.1, corrected in its body). PML4 entries 256–384 hold the linear map (`PHYS_OFFSET`, up to 64 TiB) and the heap, which system calls need as much as the kernel image; every address space copies entries 256–511. `mm::init` gives every upper-half entry a table (about 1 MiB), so the kernel half never gains a PML4 entry later and a copy made once sees every later kernel mapping (`map_mmio`, kernel stacks).
2. **A program's flags stay its own** (§6.2, corrected in its body; found by the prototype's review). A program sets any flag `popfq` allows, and neither `syscall` nor an exception clears all of them. `SFMASK` also clears NT, which would otherwise make the next `iretq` into a program fault in ring 0 once an `exit` carried it back to the kernel, as Linux found in 2014; the way back to the waiting kernel (`leave`) loads the kernel's own flags, so a program's AC never reaches ring 0 (it would switch SMAP off in plan 3); and a debug assertion checks that no TF, DF, AC or NT came back. The `sysenter` MSRs are zeroed, so `sysenter` is a general protection fault whatever the firmware left in them.
3. **A return to a non-canonical address** (§6.2). A system call whose return address is not canonical kills the program as a general protection fault at that address, instead of returning with `iretq`: on Intel CPUs `iretq` to a non-canonical `rip` faults in ring 0 as well, only on the kernel stack. The top user page is never mapped (§5.1), so no program reaches this; `is_canonical` is unit-tested.
4. **`UserSlice` copies through the linear map** (§5.1, §11.1). After the walk of the program's tables it copies from the frames they name, so the kernel never touches a program's addresses and SMAP (plan 3) needs no `stac`/`clac`. Plan 2 only copies in (`write`); copying out and `UserStr` come with plan 3's first calls that need them. `write` copies a page at a time, so a buffer that runs into a page the program does not have writes every byte before it (as Linux does).
5. **The ELF checks are `crates/elf`** (§3.1, §5.2), shared by the kernel's `spawn` and xtask's build as `sysimg` is, so one test runs every patched `t-args` through both and the build refuses exactly what the kernel refuses; `readelf` stays the independent check. Both also check ELF64, little-endian and at most 16 MiB (plan 1's deferred finding). OS/ABI and the ident version are not rules.
6. **Scheduler scope: none in plan 2** (§5.4, §6.1). One child at a time: `spawn` loads it (`EAGAIN` while one exists), `wait` runs it to completion on its own kernel stack, saving the waiting kernel's callee-saved registers and stack pointer (`arch::user::enter`, and `leave` back); the timer only counts ticks meanwhile. There is no preemption, blocking, idle task or Ctrl-C yet, so a program that never ends hangs the shell until plan 3. `spawn` and `wait` are kernel functions the in-kernel shell uses; from ring 3 every call but `exit` and `write` is `ENOSYS` until plan 3 brings the process table, when a parent can block. Plan 3 turns `enter`/`leave` into the context switch.
7. **The in-kernel shell's hook** (§8.2). `shell::System` gains `spawn` (reads the program through the `Vfs`) and `wait` (runs it, its fd 1 going to the command's standard output and fd 2 to the screen, a running script's transcript getting both); both default to "no programs", as on the host. A name that is no built-in runs `/bin/<name>`, a name with a `/` runs as given, argument 0 is the path as typed. Not found is `relay-sh: <name>: command not found` (127), a missing path `No such file or directory` (127), any other refusal `relay-sh: <name>: <message>` (126), as bash. `..`, `.` and an empty name are directories in `/bin`, and a search for a command skips directories: they are `command not found`, as in milestone 1. A program's write error is reported as a built-in's; a program that was killed keeps its report and status next to it. Plan 4's `Runner` replaces the hook, and the kernel drops it with the `shell` crate.
8. **Kernel stacks** (§5.4) are in PML4 entry 509: 64 slots, each an unmapped guard page and 64 KiB. Their page tables stay once made, so frame comparisons in scenarios start after a first program.
9. **Faults** (§11.1). Exceptions 2, 8 and 18 (NMI, double fault, machine check) are the machine's, not the program's, and reach the panic screen; NMIs and machine checks now use the double fault's IST stack, so one taken with a program's stack pointer in ring 0 (between `syscall` and the switch, or before `sysret`) still does. Every other exception in ring 3 kills the program. A page fault in the stack's guard page is a stack overflow (`stack overflow at <address>`); a stack-segment fault counts as a general protection fault; any other exception is `CPU exception <n>`, status 139. The words are `relay_abi::WaitStatus`'s `Display`, so the kernel log (`pid <n> (<path>): killed: …`), the in-kernel shell and plan 4's `/bin/sh` say the same; the shell adds `killed (…)` and bash's status. Plan 2's in-kernel shell names itself `relay-sh`, as in milestone 1. Every other exception is status 139, although bash would give some of them another signal's (a debug exception SIGTRAP): the kinds are the ABI's, and the shell does not read architecture numbers. A refusal of a program (`ENOEXEC`) always has its reason in the kernel log, a file over 16 MiB too, which is refused before it is read.
10. **Limits** (§11.1). The 8 MiB reserve applies to every frame of a program's address space, its page tables included, not to kernel stacks. An argument holding a NUL is `EINVAL`.
11. **Error numbers** (§7.2). `vfs::Errno` gains the numbers plan 2 returns (`E2BIG`, `ENOEXEC`, `EBADF`, `ECHILD`, `EAGAIN`, `ENOMEM`, `EFAULT`, `ENOSYS`) and `number()`, checked against the host C library's messages; the others come with the plans that return them.
12. **Tests** (§8.5, §12.3). `t-fault` has every kind but `sse`, which needs plan 3's CR0.TS, and two more: `flags-exit` and `flags-ud` set NT, AC and DF, then exit or fault. The e2e runner gains `expect-same NAME REGEX` (the regex's group must capture what it did the first time), used to compare `free` before and after. Scenarios `programs` (arguments, paths, redirection, refusals) and `userfault` (every `t-fault` kind, and a program after each); plan 3 adds `sse` to `userfault`. The scenarios check the messages, not `$?` (§12.3): no shell of milestone 2 prints it, since shell variables are milestone 3's (§9.4); the statuses are host-tested.
13. **Check scripts** (§12.4). `check3-a.sh` runs `t-args a 'b c' ''` and `t-fault null-read`; the recorded transcripts get those lines by hand until plan 2's NUC check records real ones (their `ls -l /bin` still lists only `t-args`, which the script's `...` lines allow).

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A program that misuses its system calls:** a null or kernel pointer, a buffer past the lower half's end or wrapping around, a buffer that runs into an unmapped page, a length of 0 or of 2^64 − 1, `write` to fd 0 or 3, a call number no call has or a call plan 2 does not serve. Expected: `EFAULT`, `EBADF` or `ENOSYS` (or a short write that keeps what came before the bad page), never a kernel fault, a panic or a hang. Tests: `a_range_is_read_across_pages_of_any_permission`, `a_page_the_program_does_not_have_is_efault`, `nothing_outside_the_lower_half_is_a_program_s` (Task 5), `write_copies_the_buffer_to_fd_1_or_2`, `other_fds_are_ebadf`, `a_bad_buffer_is_efault_or_a_short_write`, `every_other_call_is_enosys` (Task 12, and 13 for the bytes before a bad page).
2. **A file that is not a program the kernel can run:** a text file or directory run by path, a PIE or a Linux binary, a truncated or corrupted ELF, one built for another machine or ABI, one over 16 MiB, a name that is not in `/bin`, a path that does not exist. Expected: `relay-sh: <name>: Exec format error` (status 126) with the reason in the kernel log, `command not found` or `No such file or directory` (127), never a panic; the build refuses exactly what the kernel refuses. Tests: the `elf` crate's tests and `random_damage_never_panics_and_what_passes_follows_the_rules` (Task 6), `only_little_endian_elf64_up_to_16_mib` and every patched `t-args` through `both` (Task 7), `a_program_that_does_not_fit_its_file_is_enoexec` (Task 8), `what_cannot_run_says_why` (Task 9), `names_of_directories_in_bin_are_not_commands` (Task 10), the `programs` scenario (Task 15), `a_file_that_is_no_program_says_why`, `a_file_that_cannot_be_read_is_its_error` (Task 16).
3. **A program that crashes or leaves a mess, and the ones after it:** each fault kind, a stack overflow into the guard page, executing data, writing code, reading kernel memory, dividing by zero, flags set with `popfq` (NT, AC, DF) before an exit or a fault; then more programs, a fault after an exit and an exit after a fault. Expected: the program killed with spec §11.1's message and bash's status, the kernel log naming it, the shell and every later program unaffected (CR3, TSS `rsp0`, both `gs` bases and the flags set afresh), and no frame lost. Tests: `page_faults_say_which_access_at_which_address`, `each_exception_has_its_kind`, `nmis_double_faults_and_machine_checks_are_not_the_program_s`, `only_the_guard_page_is_an_overflow`, `each_fault_reads_as_the_spec_says`, `each_fault_gets_bash_s_status_for_its_signal` (Task 17), the `userfault` scenario (Task 17, and 18 for `flags-exit` and `flags-ud`), `syscall_clears_the_flags_the_kernel_needs_clear` (Task 18), the `programs` scenario's repeated runs and `expect-same mem` (Task 15).
4. **Memory running out while a program is loaded:** a program with a large `.bss`, 64 KiB of arguments, a machine low on frames. Expected: `ENOMEM` once fewer than 8 MiB of frames would be left, `E2BIG` past 64 KiB of arguments, and every frame of the half-built address space and its kernel stack given back. Tests: `running_out_of_frames_midway_leaks_nothing`, `user_memory_leaves_8_mib_free` (Task 3), `running_out_of_frames_leaks_nothing` (Task 4), `arguments_up_to_64_kib`, `running_out_of_memory_is_enomem_and_leaks_nothing` (Task 8).
5. **A program's output where the shell sends it:** `t-args > file`, `>> file`, a program run from a script whose transcript is being written, a full disk under a redirection. Expected: fd 1 follows the redirection, fd 2 goes to the screen, a running script's transcript gets both, and a write error is reported as a built-in's is. Tests: `a_program_s_output_follows_the_redirection_and_its_errors_the_screen`, `what_cannot_run_says_why`, `a_killed_program_is_reported` (Task 9, and 19 for a kill with a write error), the `programs` scenario (Task 15), `check3-a.sh` in the `checks` scenario (Task 20).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/vfs/src/errno.rs` | The error numbers plan 2 returns and `Errno::number` |
| `crates/relay-abi/src/wait.rs` | `WaitStatus`, the fault kinds and their words |
| `crates/elf/` | The rules a program's ELF file follows (`elf::check`), for the kernel and the build |
| `crates/shell/src/{io,ctx,shell,killed,testing}.rs` | `System::spawn`/`wait`; `Ctx`'s output as `Streams`; running a program; the `killed (…)` message and status; fake programs |
| `kernel/src/mm/{paging,space,kstack,user,testing}.rs`, `kernel/src/mm/mod.rs` | User pages, `unmap`, teardown; address spaces; kernel stacks; `UserSlice`; the fake `PhysMem`; the fixed upper half and `UserMem` |
| `kernel/src/exec.rs` | A program, its stack and arguments in its address space |
| `kernel/src/syscall.rs` | The architecture-neutral dispatcher |
| `kernel/src/proc.rs` | `spawn`, `wait`, the running child's calls and faults |
| `kernel/src/arch/{gdt,idt,user,fault}.rs`, `kernel/src/arch/mod.rs` | User segments, TSS `rsp0`, IST for NMIs; ring-3 exceptions; `syscall`/`sysret`, `enter`/`leave`; classifying faults |
| `kernel/src/{lib,session}.rs` | `arch::user::init`; `KernelSystem`'s `spawn`/`wait` |
| `userland/tests/src/bin/t-fault.rs` | `t-fault KIND` |
| `xtask/src/{userland,e2e}.rs` | The build's second check and its new rules; the step `expect-same` |
| `tests/e2e/{programs,userfault,system}.txt` | The new scenarios; `/bin` with two programs |
| `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/check3-a.{qemu,nuc}.log` | NUC check 3 runs `t-args` and `t-fault` |
| `README.md`, `docs/hardware-test.md` | Programs in ring 3; check 3's new lines and failures |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 2 (with its §5.1 and §6.2 corrected), the roadmap's plan 2 row and "What plan 2 leaves for plan 3", and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 2, and the roadmap's plan 2`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p2/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p2/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-29-m2-plan-2-ring-3-and-one-program.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-29-m2-plan-2-ring-3-and-one-program.md
git commit -m "docs: plan 2 of milestone 2, ring 3 and one program"
cargo xtask lint
git push -u origin m2p2/plan
gh pr create --base main --head m2p2/plan --title "Milestone 2, plan 2: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 2 ("ring 3 and one program"), the spec's §16 item 2 with its decisions (among them three corrections: every address space shares the kernel's whole upper half, spec §5.1; `syscall` clears NT as well and a program's flags never reach the kernel, §6.2; a return to a non-canonical address kills the program, §6.2), and the roadmap's plan 2 row with what plan 2 leaves for plan 3.

## How it was tested

- [x] Every task of plan 2 was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p2/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p2/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-plan` and continue with PR 2.

---

## PR 2: Memory for programs (Tasks 1–5)

What a program's memory is made of, all host-tested against a fake `PhysMem` that counts every frame: the new error numbers, user pages with their permissions, address spaces that share a fixed upper half, kernel stacks with guard pages, and `UserSlice`. The kernel only changes in `mm::init`, which now fills the upper half.

Branch `m2p2/memory`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-memory`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p2/memory /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-memory origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-memory
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p2/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p2/memory /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-memory m2p2/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p2/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: The error numbers programs need

Plan 2's calls and `spawn` return errors that `vfs::Errno` does not have yet (spec §7.2): `E2BIG` (arguments over 64 KiB), `ENOEXEC` (a file that is not a program this kernel runs), `EBADF` (a `write` to an fd that is not open), `ECHILD` (`wait` for no such child), `EAGAIN` (a second child while one exists), `ENOMEM` (not enough frames for a program), `EFAULT` (a pointer that is not the program's) and `ENOSYS` (a call plan 2 does not serve). Each gets Linux's message, and every variant gets `number()`, the value a system call returns (`relay_abi::errno`, Linux's numbers since plan 1). The test compares each variant's number and message with what the host's C library says for that number (`std::io::Error::from_raw_os_error`), so the table is checked against the real thing, not against itself. The other numbers of spec §7.2 (`ESRCH`, `EMFILE`, `EPERM`, `EINTR`, `EPIPE`) come with the plans that return them (decision 11). Mutation checks: a wrong number or a wrong message fails the test.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/vfs/Cargo.toml`
- Modify: `crates/vfs/src/errno.rs`

**Interfaces:**
- Consumes: plan 1's `relay_abi::errno`.
- Produces: `vfs::Errno::{E2BIG, ENOEXEC, EBADF, ECHILD, EAGAIN, ENOMEM, EFAULT, ENOSYS}` and `Errno::number(self) -> u16`; `vfs` depends on `relay-abi`.

- [ ] **Step 1: Change `crates/vfs/Cargo.toml`**

In `crates/vfs/Cargo.toml`, replace:

````toml
[dependencies]
````

with:

````toml
[dependencies]
relay-abi.workspace = true
````

- [ ] **Step 2: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, replace:

````rust
    }

    #[test]
    fn device_errors_become_eio() {
````

with:

````rust
    }

    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 21] = [
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
    ];

    /// The name, number and message the host's C library gives: the real
    /// thing, independent of `relay_abi`'s table.
    #[test]
    fn each_number_and_message_is_the_host_s() {
        for e in ALL {
            let n = e.number();
            let host = std::io::Error::from_raw_os_error(i32::from(n)).to_string();
            assert_eq!(
                host,
                format!("{} (os error {n})", e.message()),
                "{e:?} is {n}"
            );
        }
        let mut numbers: Vec<u16> = ALL.iter().map(|e| e.number()).collect();
        numbers.sort();
        numbers.dedup();
        assert_eq!(numbers.len(), ALL.len(), "numbers are unique");
    }

    #[test]
    fn device_errors_become_eio() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `E2BIG` found for enum `errno::Errno` in the current scope ``; `` no variant, associated function, or constant named `ENOEXEC` found for enum `errno::Errno` in the current scope ``.

- [ ] **Step 4: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! Error numbers with Linux names and messages (spec §8.1, §10).

````

with:

````rust
//! Error numbers with Linux names, messages and values (spec §8.1, §10 of
//! milestone 1; §7.2 of the user-space gate).

````

Replace:

````rust
    EFBIG,
}
````

with:

````rust
    EFBIG,
    /// The arguments of a new program are too long.
    E2BIG,
    /// A file that is not a program this kernel can run.
    ENOEXEC,
    /// A file descriptor that is not open.
    EBADF,
    /// No such child to wait for.
    ECHILD,
    /// A limit on processes was reached; trying later may work.
    EAGAIN,
    /// Not enough memory for a program.
    ENOMEM,
    /// A pointer a program passed does not point at its memory.
    EFAULT,
    /// A system call this kernel does not have.
    ENOSYS,
}
````

Replace:

````rust
            Errno::EFBIG => "File too large",
        }
````

with:

````rust
            Errno::EFBIG => "File too large",
            Errno::E2BIG => "Argument list too long",
            Errno::ENOEXEC => "Exec format error",
            Errno::EBADF => "Bad file descriptor",
            Errno::ECHILD => "No child processes",
            Errno::EAGAIN => "Resource temporarily unavailable",
            Errno::ENOMEM => "Cannot allocate memory",
            Errno::EFAULT => "Bad address",
            Errno::ENOSYS => "Function not implemented",
        }
    }

    /// The number a system call returns for it (`relay_abi::errno`, Linux's
    /// values).
    pub const fn number(self) -> u16 {
        use relay_abi::errno as n;
        match self {
            Errno::ENOENT => n::ENOENT,
            Errno::EEXIST => n::EEXIST,
            Errno::ENOTDIR => n::ENOTDIR,
            Errno::EISDIR => n::EISDIR,
            Errno::ENOTEMPTY => n::ENOTEMPTY,
            Errno::ENOSPC => n::ENOSPC,
            Errno::EIO => n::EIO,
            Errno::EROFS => n::EROFS,
            Errno::EINVAL => n::EINVAL,
            Errno::ENAMETOOLONG => n::ENAMETOOLONG,
            Errno::EXDEV => n::EXDEV,
            Errno::EBUSY => n::EBUSY,
            Errno::EFBIG => n::EFBIG,
            Errno::E2BIG => n::E2BIG,
            Errno::ENOEXEC => n::ENOEXEC,
            Errno::EBADF => n::EBADF,
            Errno::ECHILD => n::ECHILD,
            Errno::EAGAIN => n::EAGAIN,
            Errno::ENOMEM => n::ENOMEM,
            Errno::EFAULT => n::EFAULT,
            Errno::ENOSYS => n::ENOSYS,
        }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 46 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add Cargo.lock crates
git commit -m "vfs: the error numbers programs need, with their system-call values"
````


### Task 2: Page tables for programs' pages

Spec §5.1: a program's pages are 4 KiB pages in the lower half with the user bit and the permissions of their ELF segment: code read and execute, read-only data not executable, data and the stack writable and not executable (`Perm`). `PageTables::map_user` maps one such page; the tables on the way carry the user and writable bits, so the leaf entry alone decides what ring 3 may do, and the kernel's tables never get the user bit. `user_page` is the walk `UserSlice` will use: a page counts as the program's only in the lower half and with the user bit at every level, whatever the leaf says. `unmap` removes a 4 KiB page and returns its frame (kernel stacks use it too), and `free_lower_half` gives back every page of the lower half with every table below its PML4 entries. `PhysMem` gains `free_frame` and a `bytes` view of a frame; the fake `PhysMem` of the tests counts frames and panics on a double free, so the tests see every frame come back. Mutation checks: tables without the user bit, a walk that ignores the user bit, a lower-half table without it reused for a program's page, a teardown that keeps its tables, and `map_user` accepting an upper-half address all fail a test.

**Files:**
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/mm/paging.rs`

**Interfaces:**
- Consumes: milestone 1's `mm::paging` (`PageTables`, `PhysMem`, `MapError`, `PAGE`).
- Produces: `mm::paging::{Perm::{ReadExec, Read, ReadWrite}, LOWER_HALF_END, UPPER_HALF, MapError::NotUser { virt }}`; `PageTables::{map_user(&mut self, &mut impl PhysMem, virt, phys, Perm) -> Result<(), MapError>, unmap(&mut self, &mut impl PhysMem, virt) -> Option<u64>, user_page(&self, &mut impl PhysMem, virt) -> Option<(u64, Perm)>, free_lower_half(&mut self, &mut impl PhysMem)}`; `PhysMem::{free_frame(&mut self, u64), bytes(&mut self, u64) -> &mut [u8; 4096]}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            Some(p)
        }
    }

````

with:

````rust
            Some(p)
        }
        fn free_frame(&mut self, phys: u64) {
            assert!(self.tables.remove(&phys).is_some(), "{phys:#x} freed twice");
        }
    }

````

Replace:

````rust
    }

    #[test]
    fn linear_ranges_merge_touching_ram_and_skip_holes() {
````

with:

````rust
    }

    /// A lower-half address of a program's (spec §5.1).
    const U: u64 = 0x40_0000;

    /// The raw leaf entry for `v`.
    fn leaf_entry(m: &mut FakeMem, t: &PageTables, v: u64) -> u64 {
        let (pt, i) = t.leaf(m, v).expect("tables");
        m.table(pt)[i]
    }

    #[test]
    fn user_pages_carry_the_user_bit_and_their_permission() {
        let (mut m, mut t) = setup();
        let perms = [Perm::ReadExec, Perm::Read, Perm::ReadWrite];
        for (n, perm) in perms.into_iter().enumerate() {
            let frame = m.alloc_table().unwrap();
            let v = U + n as u64 * PAGE;
            t.map_user(&mut m, v, frame, perm).unwrap();
            assert_eq!(t.user_page(&mut m, v + 0x123), Some((frame, perm)));
        }
        let code = leaf_entry(&mut m, &t, U);
        assert_eq!(code & (USER | WRITABLE | NO_EXECUTE), USER, "R-X");
        let rodata = leaf_entry(&mut m, &t, U + PAGE);
        assert_eq!(rodata & (USER | WRITABLE | NO_EXECUTE), USER | NO_EXECUTE);
        let data = leaf_entry(&mut m, &t, U + 2 * PAGE);
        assert_eq!(
            data & (USER | WRITABLE | NO_EXECUTE),
            USER | WRITABLE | NO_EXECUTE
        );
        // The tables on the way let the leaf decide.
        let mut table = t.pml4;
        for level in (1..4).rev() {
            let e = m.table(table)[index(U, level)];
            assert_eq!(
                e & (PRESENT | WRITABLE | USER | NO_EXECUTE),
                PRESENT | WRITABLE | USER
            );
            table = e & ADDR;
        }
        assert_eq!(t.user_page(&mut m, U + 3 * PAGE), None, "not mapped");
    }

    #[test]
    fn kernel_pages_are_not_user_pages() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.user_page(&mut m, V + 0x1000), None);
        let pdpt = m.table(t.pml4)[index(V, 3)];
        assert_eq!(pdpt & USER, 0, "kernel tables have no user bit");
        // A kernel-only mapping in the lower half (none exists, but the
        // walk must not trust the leaf alone).
        let frame = m.alloc_table().unwrap();
        t.map_user(&mut m, U, frame, Perm::ReadWrite).unwrap();
        let (pt, i) = t.leaf(&mut m, U).unwrap();
        m.table(pt)[i] &= !USER;
        assert_eq!(t.user_page(&mut m, U), None);
        assert_eq!(t.user_page(&mut m, LOWER_HALF_END), None);
        assert_eq!(t.user_page(&mut m, u64::MAX), None);
        // An upper-half page is never a program's, whatever its bits say.
        let mut table = t.pml4;
        for level in (0..4).rev() {
            let e = &mut m.table(table)[index(V + 0x1000, level)];
            *e |= USER;
            table = *e & ADDR;
        }
        assert_eq!(t.user_page(&mut m, V + 0x1000), None);
    }

    #[test]
    fn user_pages_are_lower_half_4k_pages_mapped_once() {
        let (mut m, mut t) = setup();
        let frame = m.alloc_table().unwrap();
        assert_eq!(
            t.map_user(&mut m, LOWER_HALF_END, frame, Perm::Read),
            Err(MapError::NotUser {
                virt: LOWER_HALF_END
            })
        );
        assert_eq!(
            t.map_user(&mut m, U + 8, frame, Perm::Read),
            Err(MapError::Unaligned)
        );
        t.map_user(&mut m, U, frame, Perm::Read).unwrap();
        assert_eq!(
            t.map_user(&mut m, U, frame, Perm::Read),
            Err(MapError::Conflict { virt: U })
        );
        // A lower-half table without the user bit is not used for one.
        let kernel_table = m.alloc_table().unwrap();
        t.set_pml4_entry(&mut m, 1, kernel_table | PRESENT | WRITABLE);
        assert_eq!(
            t.map_user(&mut m, 1 << 39, frame, Perm::Read),
            Err(MapError::Conflict { virt: 1 << 39 })
        );
        // The last page below the upper half is fine.
        t.map_user(&mut m, LOWER_HALF_END - PAGE, frame, Perm::Read)
            .unwrap();
    }

    #[test]
    fn unmap_returns_the_frame_and_leaves_nothing_behind() {
        let (mut m, mut t) = setup();
        let frame = m.alloc_table().unwrap();
        t.map_user(&mut m, U, frame, Perm::ReadWrite).unwrap();
        assert_eq!(t.unmap(&mut m, U), Some(frame));
        assert_eq!(t.user_page(&mut m, U), None);
        assert_eq!(t.unmap(&mut m, U), None, "already gone");
        assert_eq!(t.unmap(&mut m, U + HUGE), None, "never mapped");
        // Kernel pages too (the kernel-stack area).
        t.map(&mut m, V + 0x5000, 0x5000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.unmap(&mut m, V + 0x5000), Some(0x5000));
        assert_eq!(t.translate(&mut m, V + 0x5000), None);
        // Not a 4 KiB page: left alone.
        t.map(&mut m, V + HUGE, HUGE, HUGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(t.unmap(&mut m, V + HUGE), None);
        assert!(t.translate(&mut m, V + HUGE).is_some());
    }

    #[test]
    fn freeing_the_lower_half_gives_back_every_frame() {
        let (mut m, mut t) = setup();
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        let kernel = m.tables.len();
        // Pages spread over several tables of each level: two PML4 entries,
        // two PDPT entries, two PDs, several PTs.
        let spots = [
            U,
            U + PAGE,
            U + HUGE,
            U + (1 << 30),
            0x7FFF_FFFF_E000,
            0x7FFF_FFF0_0000,
        ];
        for v in spots {
            let frame = m.alloc_table().unwrap();
            t.map_user(&mut m, v, frame, Perm::ReadWrite).unwrap();
        }
        assert!(m.tables.len() > kernel + spots.len() + 4);
        t.free_lower_half(&mut m);
        assert_eq!(m.tables.len(), kernel, "only the kernel's tables are left");
        assert!(m.table(t.pml4)[..256].iter().all(|&e| e == 0));
        assert_eq!(
            t.translate(&mut m, V + 0x1000).map(|(p, ..)| p),
            Some(0x1000),
            "the kernel half is untouched"
        );
        for v in spots {
            assert_eq!(t.user_page(&mut m, v), None);
        }
        // An empty lower half frees nothing.
        t.free_lower_half(&mut m);
        assert_eq!(m.tables.len(), kernel);
    }

    #[test]
    fn a_frame_s_bytes_are_its_table_s() {
        let (mut m, _) = setup();
        let f = m.alloc_table().unwrap();
        m.bytes(f)[8..16].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(m.table(f)[1], 0x1122_3344_5566_7788);
        assert_eq!(m.bytes(f).len(), 4096);
    }

    #[test]
    fn linear_ranges_merge_touching_ram_and_skip_holes() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib paging`

Expected: FAIL: compile errors such as `` cannot find value `USER` in this scope ``; `` cannot find value `LOWER_HALF_END` in this scope ``.

- [ ] **Step 3: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
        Some(p)
    }
````

with:

````rust
        Some(p)
    }

    fn free_frame(&mut self, phys: u64) {
        self.0.free(phys, 1);
    }
````

- [ ] **Step 4: Change `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
//!
//! Every mapping made here is writable and not executable (the kernel image
//! keeps the loader's mappings). Each physical page appears at most once in
//! the linear map, at `PHYS_OFFSET + phys`, so there are never two mappings
//! of one page with different cache types.

````

with:

````rust
//!
//! The kernel's own mappings (`map`) are writable and not executable (the
//! kernel image keeps the loader's mappings). Each physical page appears at
//! most once in the linear map, at `PHYS_OFFSET + phys`, so there are never
//! two mappings of one page with different cache types.
//!
//! A program's pages (`map_user`, user-space gate §5.1) are 4 KiB pages in
//! the lower half with the user bit and the permissions of their segment;
//! `free_lower_half` gives back every one of them with its tables.

````

Replace:

````rust
const WRITABLE: u64 = 1 << 1;
const PWT: u64 = 1 << 3;
````

with:

````rust
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const PWT: u64 = 1 << 3;
````

Replace:

````rust
const HUGE_ADDR: u64 = 0x000F_FFFF_FFE0_0000;

````

with:

````rust
const HUGE_ADDR: u64 = 0x000F_FFFF_FFE0_0000;
/// The first address of the upper half; PML4 entries 256-511.
pub const UPPER_HALF: u64 = 0xFFFF_8000_0000_0000;
/// The end of the lower half (PML4 entries 0-255), which programs own.
pub const LOWER_HALF_END: u64 = 0x0000_8000_0000_0000;

/// What a program may do with one of its pages (user-space gate §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perm {
    /// Code: read and execute.
    ReadExec,
    /// Read-only data: not executable.
    Read,
    /// Data, the stack: writable, not executable.
    ReadWrite,
}

impl Perm {
    fn bits(self) -> u64 {
        match self {
            Perm::ReadExec => 0,
            Perm::Read => NO_EXECUTE,
            Perm::ReadWrite => WRITABLE | NO_EXECUTE,
        }
    }

    fn of_entry(e: u64) -> Perm {
        if e & WRITABLE != 0 {
            Perm::ReadWrite
        } else if e & NO_EXECUTE != 0 {
            Perm::Read
        } else {
            Perm::ReadExec
        }
    }
}

````

Replace:

````rust
    fn table(&mut self, phys: u64) -> &mut [u64; 512];
    /// A zeroed frame for a new table, or `None` when memory is exhausted.
    fn alloc_table(&mut self) -> Option<u64>;
}
````

with:

````rust
    fn table(&mut self, phys: u64) -> &mut [u64; 512];
    /// A zeroed frame for a new table (or a program's page), or `None`
    /// when memory is exhausted.
    fn alloc_table(&mut self) -> Option<u64>;
    /// Gives back a frame from `alloc_table`.
    fn free_frame(&mut self, phys: u64);
    /// The bytes of the frame at `phys`.
    fn bytes(&mut self, phys: u64) -> &mut [u8; PAGE as usize] {
        let table: *mut [u64; 512] = self.table(phys);
        // SAFETY: a frame of 512 `u64`s is 4096 bytes, and bytes have no
        // alignment of their own.
        unsafe { &mut *table.cast::<[u8; PAGE as usize]>() }
    }
}
````

Replace:

````rust
    OutOfRange,
}
````

with:

````rust
    OutOfRange,
    /// A program's page must lie in the lower half.
    NotUser {
        virt: u64,
    },
}
````

Replace:

````rust
            MapError::OutOfRange => write!(f, "physical address beyond the linear map"),
        }
````

with:

````rust
            MapError::OutOfRange => write!(f, "physical address beyond the linear map"),
            MapError::NotUser { virt } => write!(f, "{virt:#x} is not in the lower half"),
        }
````

Replace:

````rust
    /// The next-level table under entry `i` of `table`, created if missing.
    fn child(
````

with:

````rust
    /// The next-level table under entry `i` of `table`, created if missing.
    /// Tables of the lower half carry the user bit (the leaf entry decides
    /// what ring 3 may do); the kernel's never do.
    fn child(
````

Replace:

````rust
    ) -> Result<u64, MapError> {
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            mem.table(table)[i] = t | PRESENT | WRITABLE;
            Ok(t)
        } else if e & HUGE_PAGE != 0 {
            Err(MapError::Conflict { virt })
````

with:

````rust
    ) -> Result<u64, MapError> {
        let flags = if virt < LOWER_HALF_END {
            PRESENT | WRITABLE | USER
        } else {
            PRESENT | WRITABLE
        };
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            mem.table(table)[i] = t | flags;
            Ok(t)
        } else if e & HUGE_PAGE != 0 || e & flags != flags {
            Err(MapError::Conflict { virt })
````

Replace:

````rust
        Ok((pd, index(virt, 1)))
    }
````

with:

````rust
        Ok((pd, index(virt, 1)))
    }

    /// Maps the 4 KiB page at `virt` in the lower half to the frame `phys`
    /// for ring 3, with `perm`. A page already mapped is a conflict: a
    /// program's pages are mapped once, when they are allocated.
    pub fn map_user(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        phys: u64,
        perm: Perm,
    ) -> Result<(), MapError> {
        if !(virt | phys).is_multiple_of(PAGE) {
            return Err(MapError::Unaligned);
        }
        if virt >= LOWER_HALF_END {
            return Err(MapError::NotUser { virt });
        }
        let (pd, i) = self.pd_slot(mem, virt)?;
        let pt = self.child(mem, pd, i, virt)?;
        let slot = &mut mem.table(pt)[index(virt, 0)];
        if *slot & PRESENT != 0 {
            return Err(MapError::Conflict { virt });
        }
        *slot = phys | PRESENT | USER | perm.bits();
        Ok(())
    }

    /// The 4 KiB leaf entry for `virt`, if every table on the way is there.
    fn leaf(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, usize)> {
        let mut table = self.pml4;
        for level in (1..4).rev() {
            let e = mem.table(table)[index(virt, level)];
            if e & PRESENT == 0 || e & HUGE_PAGE != 0 {
                return None;
            }
            table = e & ADDR;
        }
        Some((table, index(virt, 0)))
    }

    /// Removes the 4 KiB page at `virt` and returns its frame; `None` if
    /// no 4 KiB page is mapped there. The tables stay; the caller flushes
    /// the TLB.
    pub fn unmap(&mut self, mem: &mut impl PhysMem, virt: u64) -> Option<u64> {
        let (pt, i) = self.leaf(mem, virt)?;
        let e = mem.table(pt)[i];
        if e & PRESENT == 0 {
            return None;
        }
        mem.table(pt)[i] = 0;
        Some(e & ADDR)
    }

    /// The frame and permission of the page holding `virt`, if ring 3 may
    /// use it: in the lower half, and the user bit at every level.
    pub fn user_page(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Perm)> {
        if virt >= LOWER_HALF_END {
            return None;
        }
        let mut table = self.pml4;
        for level in (0..4).rev() {
            let e = mem.table(table)[index(virt, level)];
            if e & (PRESENT | USER) != PRESENT | USER || e & HUGE_PAGE != 0 {
                return None;
            }
            if level == 0 {
                return Some((e & ADDR, Perm::of_entry(e)));
            }
            table = e & ADDR;
        }
        None
    }

    /// Gives back every page of the lower half and every table below its
    /// PML4 entries, which are left empty (user-space gate §5.1). Only
    /// `map_user` puts pages there, so all of them are 4 KiB pages.
    pub fn free_lower_half(&mut self, mem: &mut impl PhysMem) {
        for i in 0..256 {
            let pdpt = mem.table(self.pml4)[i];
            if pdpt & PRESENT == 0 {
                continue;
            }
            mem.table(self.pml4)[i] = 0;
            free_table(mem, pdpt & ADDR, 2);
        }
    }
````

Replace:

````rust
        None
    }
}

````

with:

````rust
        None
    }
}

/// Frees the table at `table` of `level` (2 a PDPT, 1 a PD, 0 a PT), the
/// tables below it and, from a PT, the pages it maps.
fn free_table(mem: &mut impl PhysMem, table: u64, level: u32) {
    for i in 0..512 {
        let e = mem.table(table)[i];
        if e & PRESENT == 0 {
            continue;
        }
        if level == 0 {
            mem.free_frame(e & ADDR);
        } else {
            free_table(mem, e & ADDR, level - 1);
        }
    }
    mem.free_frame(table);
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib paging`

Expected: PASS: 16 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: page tables hold programs' pages with their permissions, and give them back"
````


### Task 3: Address spaces over a fixed upper half

Every program gets its own PML4 (spec §5.1): its lower half holds the program's pages, its upper half is the kernel's. The kernel's upper half is entries 256–511, not only 385 and up as the spec's body said before PR 1: the linear map and the heap are in 256–384, and system calls use them (decision 1). `mm::init` now gives every upper-half PML4 entry an empty table (`fill_upper_half`, about 1 MiB), so the kernel half never gains a PML4 entry later, and an `AddressSpace` that copies entries 256–511 once still sees every later kernel mapping (`map_mmio`, kernel stacks); a test maps into the kernel's tables after the space exists and finds the mapping through the space. `AddressSpace` maps fresh zeroed pages (`map_zeroed`), fills them before the program runs whatever their permissions (`fill`), and `destroy` gives back every page, table and the PML4. Programs' frames come through `UserMem`, which refuses a frame once fewer than 8 MiB (2048 frames) would be left (spec §11.1, decision 10), so DMA buffers, page tables and kernel stacks never run out. The fake `PhysMem` moves to `mm/testing.rs` for every test of `mm` and after. Mutation checks: a failed mapping keeping its frame, `destroy` keeping the PML4, sharing from entry 257, a range not bounded by the lower half, the reserve off by one and `fill_upper_half` skipping entries all fail a test.

**Files:**
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/mm/paging.rs`
- Create: `kernel/src/mm/space.rs`
- Create: `kernel/src/mm/testing.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces: `mm::space::AddressSpace::{new(&mut impl PhysMem, kernel: &PageTables) -> Result<AddressSpace, MapError>, pml4(&self) -> u64, map_zeroed(&mut self, &mut impl PhysMem, virt, pages, Perm) -> Result<(), MapError>, user_page(&self, &mut impl PhysMem, virt) -> Option<(u64, Perm)>, fill(&mut self, &mut impl PhysMem, virt, &[u8]) -> Result<(), MapError>, destroy(self, &mut impl PhysMem)}`; `PageTables::fill_upper_half`; `mm::{USER_RESERVE_FRAMES, user_may_take, UserMem, with_user_memory(f: impl FnOnce(&mut UserMem, &PageTables) -> R) -> R, kernel_pml4() -> u64}`; `mm::testing::FakeMem` (tests only).

- [ ] **Step 1: Add the failing tests and the module declaration to `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub mod paging;

````

with:

````rust
pub mod paging;
pub mod space;
#[cfg(test)]
pub mod testing;

````

Replace:

````rust
    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
````

with:

````rust
    #[test]
    fn user_memory_leaves_8_mib_free() {
        assert_eq!(USER_RESERVE_FRAMES, 2048);
        assert!(user_may_take(2049), "the frame taken leaves 2048");
        assert!(!user_may_take(2048));
        assert!(!user_may_take(0));
    }

    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
````

- [ ] **Step 2: Add the failing tests to `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::*;
    use std::collections::HashMap;

    /// Page tables in host memory, with made-up physical addresses.
    struct FakeMem {
        tables: HashMap<u64, Box<[u64; 512]>>,
        next: u64,
        limit: usize,
    }

    impl FakeMem {
        fn new() -> FakeMem {
            FakeMem {
                tables: HashMap::new(),
                next: 0x1000_0000,
                limit: usize::MAX,
            }
        }
    }

    impl PhysMem for FakeMem {
        fn table(&mut self, phys: u64) -> &mut [u64; 512] {
            self.tables.get_mut(&phys).expect("not a table frame")
        }
        fn alloc_table(&mut self) -> Option<u64> {
            if self.tables.len() >= self.limit {
                return None;
            }
            let p = self.next;
            self.next += PAGE;
            self.tables.insert(p, Box::new([0; 512]));
            Some(p)
        }
        fn free_frame(&mut self, phys: u64) {
            assert!(self.tables.remove(&phys).is_some(), "{phys:#x} freed twice");
        }
    }

````

with:

````rust
    use super::*;
    use crate::mm::testing::FakeMem;

````

Replace:

````rust
            "lower half empty"
        );
    }

````

with:

````rust
            "lower half empty"
        );
    }

    #[test]
    fn the_upper_half_gets_every_pml4_entry_up_front() {
        let (mut m, mut t) = setup();
        t.set_pml4_entry(&mut m, 511, 0x1234_5000 | PRESENT | WRITABLE);
        t.fill_upper_half(&mut m).unwrap();
        let pml4 = *m.table(t.pml4);
        assert_eq!(pml4[511], 0x1234_5003, "the loader's entry stays");
        for (i, e) in pml4.iter().enumerate() {
            if i < 256 {
                assert_eq!(*e, 0, "entry {i}: the lower half stays empty");
            } else {
                assert_eq!(
                    e & (PRESENT | WRITABLE | USER),
                    PRESENT | WRITABLE,
                    "entry {i}"
                );
            }
        }
        // 255 new tables, each empty; filling again adds none.
        assert_eq!(m.tables.len(), 1 + 255);
        t.fill_upper_half(&mut m).unwrap();
        assert_eq!(m.tables.len(), 1 + 255);
        // Kernel mappings now go under those tables.
        t.map(&mut m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        assert_eq!(m.table(t.pml4)[256], pml4[256]);
    }

    #[test]
    fn filling_the_upper_half_can_run_out_of_memory() {
        let (mut m, mut t) = setup();
        m.limit = 10;
        assert_eq!(t.fill_upper_half(&mut m), Err(MapError::OutOfMemory));
    }

````

- [ ] **Step 3: Write the failing tests for `kernel/src/mm/space.rs`**

Create `kernel/src/mm/space.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::Cache;
    use crate::mm::testing::FakeMem;

    const V: u64 = 0xFFFF_8000_0000_0000;
    const U: u64 = 0x40_0000;

    fn kernel(m: &mut FakeMem) -> PageTables {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        k.map(m, V + 0x1000, 0x1000, PAGE, Cache::WriteBack)
            .unwrap();
        k
    }

    /// The bytes of the program's memory at `virt`.
    fn read(m: &mut FakeMem, s: &AddressSpace, virt: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| {
                let (frame, _) = s.user_page(m, virt + i).expect("mapped");
                m.bytes(frame)[((virt + i) % PAGE) as usize]
            })
            .collect()
    }

    #[test]
    fn a_new_space_shares_the_kernel_half_and_sees_later_kernel_mappings() {
        let mut m = FakeMem::new();
        let mut k = kernel(&mut m);
        let s = AddressSpace::new(&mut m, &k).unwrap();
        let (kp, sp) = (*m.table(k.pml4), *m.table(s.pml4()));
        assert_eq!(kp[256..], sp[256..], "the upper half is the kernel's");
        assert!(sp[..256].iter().all(|&e| e == 0), "the lower half is empty");
        // A kernel mapping made afterwards (a kernel stack, `map_mmio`).
        k.map(&mut m, V + 0x7000_0000, 0x7000_0000, PAGE, Cache::Uncached)
            .unwrap();
        let space_tables = PageTables { pml4: s.pml4() };
        assert_eq!(
            space_tables.translate(&mut m, V + 0x7000_0000),
            Some((0x7000_0000, Cache::Uncached, PAGE))
        );
        s.destroy(&mut m);
    }

    #[test]
    fn mapped_pages_are_zeroed_filled_and_all_given_back() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        s.map_zeroed(&mut m, U, 3, Perm::ReadExec).unwrap();
        s.map_zeroed(&mut m, 0x7FFF_FFF0_0000, 255, Perm::ReadWrite)
            .unwrap();
        assert_eq!(
            read(&mut m, &s, U, 3 * PAGE as usize),
            vec![0; 3 * PAGE as usize]
        );
        // Across a page boundary, into a read-only page.
        s.fill(&mut m, U + PAGE - 2, b"code").unwrap();
        assert_eq!(read(&mut m, &s, U + PAGE - 3, 6), b"\0code\0");
        assert_eq!(s.user_page(&mut m, U).unwrap().1, Perm::ReadExec);
        assert_eq!(
            s.fill(&mut m, U + 3 * PAGE - 1, b"xy"),
            Err(MapError::NotUser { virt: U + 3 * PAGE }),
            "past the mapped pages"
        );
        assert!(m.frames() > before + 258);
        s.destroy(&mut m);
        assert_eq!(m.frames(), before, "every frame came back");
    }

    #[test]
    fn running_out_of_frames_midway_leaks_nothing() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        for extra in 1..12 {
            m.limit = before + extra;
            let Ok(mut s) = AddressSpace::new(&mut m, &k) else {
                assert_eq!(m.frames(), before);
                continue;
            };
            assert_eq!(
                s.map_zeroed(&mut m, U, 16, Perm::ReadWrite),
                Err(MapError::OutOfMemory)
            );
            s.destroy(&mut m);
            assert_eq!(m.frames(), before, "limit {extra}");
        }
    }

    #[test]
    fn a_range_must_stay_in_the_lower_half() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        let last = LOWER_HALF_END - PAGE;
        assert_eq!(
            s.map_zeroed(&mut m, last, 2, Perm::Read),
            Err(MapError::NotUser { virt: last })
        );
        assert_eq!(
            s.map_zeroed(&mut m, U, u64::MAX / 2, Perm::Read),
            Err(MapError::NotUser { virt: U })
        );
        s.map_zeroed(&mut m, last, 1, Perm::Read).unwrap();
        // Mapping a page twice fails without keeping the second frame.
        let frames = m.frames();
        assert_eq!(
            s.map_zeroed(&mut m, last, 1, Perm::Read),
            Err(MapError::Conflict { virt: last })
        );
        assert_eq!(m.frames(), frames);
        s.destroy(&mut m);
        assert_eq!(m.frames(), before);
    }
}
````

- [ ] **Step 4: Create `kernel/src/mm/testing.rs`**

Create `kernel/src/mm/testing.rs`:

````rust
//! A fake `PhysMem` for the host tests: frames in host memory at made-up
//! physical addresses, counted, so a test sees every frame come back.

use super::paging::{PAGE, PhysMem};
use std::collections::HashMap;

pub struct FakeMem {
    /// Every allocated frame, tables and pages alike.
    pub tables: HashMap<u64, Box<[u64; 512]>>,
    next: u64,
    /// No more frames than this may be allocated at once.
    pub limit: usize,
}

impl FakeMem {
    pub fn new() -> FakeMem {
        FakeMem {
            tables: HashMap::new(),
            next: 0x1000_0000,
            limit: usize::MAX,
        }
    }

    /// Frames allocated and not yet given back.
    pub fn frames(&self) -> usize {
        self.tables.len()
    }
}

impl Default for FakeMem {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysMem for FakeMem {
    fn table(&mut self, phys: u64) -> &mut [u64; 512] {
        self.tables.get_mut(&phys).expect("not a table frame")
    }
    fn alloc_table(&mut self) -> Option<u64> {
        if self.tables.len() >= self.limit {
            return None;
        }
        let p = self.next;
        self.next += PAGE;
        self.tables.insert(p, Box::new([0; 512]));
        Some(p)
    }
    fn free_frame(&mut self, phys: u64) {
        assert!(self.tables.remove(&phys).is_some(), "{phys:#x} freed twice");
    }
}
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib mm::`

Expected: FAIL: compile errors such as `` cannot find type `PageTables` in this scope ``; `` cannot find value `PAGE` in this scope ``.

- [ ] **Step 6: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! half, including the loader's identity-mapped trampoline. Device memory is
//! mapped later, on demand, with `map_mmio`.

````

with:

````rust
//! half, including the loader's identity-mapped trampoline. Device memory is
//! mapped later, on demand, with `map_mmio`. Every upper-half PML4 entry
//! exists from `init` on, so programs' address spaces (`space`) share the
//! kernel's half by copying those entries once.

````

Replace:

````rust

/// Page tables are reached through the linear map; new ones come from the
/// frame allocator.
struct LinearMem<'a>(&'a mut FrameAllocator<'static>);

````

with:

````rust

/// Frames user memory must leave free (user-space gate §11.1): 8 MiB, so
/// DMA buffers, page tables and kernel stacks never run out.
pub const USER_RESERVE_FRAMES: u64 = (8 << 20) / FRAME_SIZE;

/// Whether one more frame may go to a program's memory while `free` frames
/// are free.
pub fn user_may_take(free: u64) -> bool {
    free > USER_RESERVE_FRAMES
}

/// Page tables are reached through the linear map; new ones come from the
/// frame allocator.
struct LinearMem<'a>(&'a mut FrameAllocator<'static>);

/// The same for a program's memory, which leaves `USER_RESERVE_FRAMES`
/// free.
pub struct UserMem<'a>(LinearMem<'a>);

impl PhysMem for UserMem<'_> {
    fn table(&mut self, phys: u64) -> &mut [u64; 512] {
        self.0.table(phys)
    }

    fn alloc_table(&mut self) -> Option<u64> {
        if !user_may_take(self.0.0.free_frames()) {
            return None;
        }
        self.0.alloc_table()
    }

    fn free_frame(&mut self, phys: u64) {
        self.0.free_frame(phys)
    }
}

/// Runs `f` with the frames for a program's memory and the kernel's page
/// tables (which a new `AddressSpace` shares).
pub fn with_user_memory<R>(f: impl FnOnce(&mut UserMem<'_>, &PageTables) -> R) -> R {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    f(&mut UserMem(LinearMem(&mut m.frames)), &m.tables)
}

/// The kernel's own page tables, for CR3 when no program runs.
pub fn kernel_pml4() -> u64 {
    MEMORY
        .lock()
        .as_ref()
        .expect("mm::init has not run")
        .tables
        .pml4
}

````

Replace:

````rust
    }
    for (start, end) in paging::linear_ranges(map) {
````

with:

````rust
    }
    tables.fill_upper_half(&mut mem)?;
    for (start, end) in paging::linear_ranges(map) {
````

- [ ] **Step 7: Change `kernel/src/mm/paging.rs`**

In `kernel/src/mm/paging.rs`, replace:

````rust
        Ok(PageTables { pml4 })
    }
````

with:

````rust
        Ok(PageTables { pml4 })
    }

    /// Gives every empty upper-half PML4 entry (256-511) an empty table,
    /// so the kernel's half never gains a PML4 entry again: every address
    /// space copies these entries once, when it is made, and still sees
    /// every kernel mapping made later (user-space gate §5.1).
    pub fn fill_upper_half(&mut self, mem: &mut impl PhysMem) -> Result<(), MapError> {
        for i in 256..512 {
            if mem.table(self.pml4)[i] & PRESENT == 0 {
                let t = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
                mem.table(self.pml4)[i] = t | PRESENT | WRITABLE;
            }
        }
        Ok(())
    }
````

- [ ] **Step 8: Implement `kernel/src/mm/space.rs`**

Insert this at the top of `kernel/src/mm/space.rs`, above `#[cfg(test)]`:

````rust
//! A program's address space (user-space gate §5.1): a PML4 of its own,
//! whose lower half holds the program's pages and whose upper half is the
//! kernel's, shared. The kernel's upper-half PML4 entries never change
//! after `mm::init` (`PageTables::fill_upper_half`), so copying them once
//! is enough for the space to see every kernel mapping, later ones too.

use super::paging::{LOWER_HALF_END, MapError, PAGE, PageTables, Perm, PhysMem};

pub struct AddressSpace {
    tables: PageTables,
}

impl AddressSpace {
    /// An empty lower half under the kernel's upper half (PML4 entries
    /// 256-511 of `kernel`).
    pub fn new(mem: &mut impl PhysMem, kernel: &PageTables) -> Result<AddressSpace, MapError> {
        let mut tables = PageTables::new(mem)?;
        for i in 256..512 {
            let e = mem.table(kernel.pml4)[i];
            tables.set_pml4_entry(mem, i, e);
        }
        Ok(AddressSpace { tables })
    }

    /// The physical address of its PML4, for CR3.
    pub fn pml4(&self) -> u64 {
        self.tables.pml4
    }

    /// Maps `pages` fresh zeroed pages from `virt` for the program, with
    /// `perm` (spec §5.1: memory is allocated and zeroed when it is mapped).
    /// On failure the pages mapped so far stay mapped; `destroy` gives them
    /// back with the rest.
    pub fn map_zeroed(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        pages: u64,
        perm: Perm,
    ) -> Result<(), MapError> {
        let end = pages
            .checked_mul(PAGE)
            .and_then(|len| virt.checked_add(len))
            .filter(|&end| end <= LOWER_HALF_END)
            .ok_or(MapError::NotUser { virt })?;
        let mut v = virt;
        while v < end {
            let frame = mem.alloc_table().ok_or(MapError::OutOfMemory)?;
            if let Err(e) = self.tables.map_user(mem, v, frame, perm) {
                mem.free_frame(frame);
                return Err(e);
            }
            v += PAGE;
        }
        Ok(())
    }

    /// The frame and permission of the program's page holding `virt`.
    pub fn user_page(&self, mem: &mut impl PhysMem, virt: u64) -> Option<(u64, Perm)> {
        self.tables.user_page(mem, virt)
    }

    /// Copies `bytes` to `virt`, whatever the pages' permissions: the
    /// kernel filling a program's pages before it runs. Every page must be
    /// mapped already.
    pub fn fill(
        &mut self,
        mem: &mut impl PhysMem,
        virt: u64,
        bytes: &[u8],
    ) -> Result<(), MapError> {
        let mut done = 0;
        while done < bytes.len() {
            let v = virt
                .checked_add(done as u64)
                .ok_or(MapError::NotUser { virt })?;
            let (frame, _) = self
                .user_page(mem, v)
                .ok_or(MapError::NotUser { virt: v })?;
            let at = (v % PAGE) as usize;
            let n = (PAGE as usize - at).min(bytes.len() - done);
            mem.bytes(frame)[at..at + n].copy_from_slice(&bytes[done..done + n]);
            done += n;
        }
        Ok(())
    }

    /// Gives back every page, table and the PML4. The kernel must not be
    /// running on this space's tables (CR3) any more.
    pub fn destroy(mut self, mem: &mut impl PhysMem) {
        self.tables.free_lower_half(mem);
        mem.free_frame(self.tables.pml4);
    }
}

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib mm::`

Expected: PASS: 33 tests.

- [ ] **Step 10: Run the `boot` scenario**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add kernel
git commit -m "kernel: address spaces for programs, sharing an upper half that no longer changes"
````


### Task 4: Kernel stacks for programs

While the kernel serves a program (its system calls, interrupts and faults), it runs on a kernel stack of the program's own (spec §5.4): 64 KiB above an unmapped guard page, so an overflow faults (and reaches the panic screen through the double fault's IST stack) instead of running into the next stack. The kernel-stack area has PML4 entry 509 to itself, between the loader's kernel stack (508) and the kernel image (511), with 64 slots of 128 KiB, one per process-table entry to come (decision 8). Plan 2 uses one slot at a time. The stacks' frames come from the kernel's own frames, not through the 8 MiB reserve; the slot's page tables stay once made. Mutation checks: a failed allocation keeping the pages it mapped, or the frame whose mapping failed (a test with no tables made yet), a slot never freed, and a stack without its guard page all fail a test.

**Files:**
- Create: `kernel/src/mm/kstack.rs`
- Modify: `kernel/src/mm/mod.rs`

**Interfaces:**
- Consumes: Tasks 2 (`unmap`) and 3 (`fill_upper_half`, `FakeMem`).
- Produces: `mm::kstack::{AREA, SLOTS, SIZE, KernelStack::{bottom, top, pages}, KernelStacks::{new, alloc(&mut self, &mut PageTables, &mut impl PhysMem) -> Option<KernelStack>, free(&mut self, KernelStack, &mut PageTables, &mut impl PhysMem)}}`; `mm::{alloc_kernel_stack() -> Option<KernelStack>, free_kernel_stack(KernelStack)}`.

- [ ] **Step 1: Write the failing tests for `kernel/src/mm/kstack.rs`**

Create `kernel/src/mm/kstack.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::testing::FakeMem;
    use boot_info::{HEAP_BASE, KERNEL_BASE, KERNEL_STACK_BOTTOM, PHYS_OFFSET};

    fn pml4_entry(virt: u64) -> u64 {
        (virt >> 39) & 0x1FF
    }

    fn setup() -> (FakeMem, PageTables) {
        let mut m = FakeMem::new();
        let mut t = PageTables::new(&mut m).unwrap();
        t.fill_upper_half(&mut m).unwrap();
        (m, t)
    }

    #[test]
    fn the_area_has_a_pml4_entry_of_its_own() {
        assert_eq!(pml4_entry(AREA), 509);
        let last = KernelStack { slot: SLOTS - 1 };
        assert_eq!(pml4_entry(last.top() - 1), 509);
        for other in [PHYS_OFFSET, HEAP_BASE, KERNEL_STACK_BOTTOM, KERNEL_BASE] {
            assert_ne!(pml4_entry(other), 509, "{other:#x}");
        }
    }

    #[test]
    fn a_stack_is_64_kib_above_an_unmapped_guard_page() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let a = stacks.alloc(&mut t, &mut m).unwrap();
        let b = stacks.alloc(&mut t, &mut m).unwrap();
        assert_eq!(a.bottom(), AREA + PAGE);
        assert_eq!(a.top(), AREA + PAGE + SIZE);
        assert_eq!(b.bottom(), AREA + SLOT_SIZE + PAGE);
        assert_eq!(a.top() % 16, 0);
        for s in [&a, &b] {
            assert_eq!(t.translate(&mut m, s.bottom() - PAGE), None, "guard");
            assert_eq!(t.translate(&mut m, s.top()), None, "above");
            for v in s.pages() {
                let (_, cache, size) = t.translate(&mut m, v).expect("mapped");
                assert_eq!((cache, size), (Cache::WriteBack, PAGE));
                assert_eq!(t.user_page(&mut m, v), None, "not for ring 3");
            }
        }
        assert_eq!(a.pages().count(), 16);
    }

    #[test]
    fn freed_stacks_give_back_their_frames_and_slot() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let first = stacks.alloc(&mut t, &mut m).unwrap();
        stacks.free(first, &mut t, &mut m);
        // The page tables of the slot stay; its 16 frames do not.
        let before = m.frames();
        let s = stacks.alloc(&mut t, &mut m).unwrap();
        assert_eq!(s, KernelStack { slot: 0 }, "the slot is used again");
        assert_eq!(m.frames(), before + 16);
        let bottom = s.bottom();
        stacks.free(s, &mut t, &mut m);
        assert_eq!(m.frames(), before);
        assert_eq!(t.translate(&mut m, bottom), None);
    }

    #[test]
    fn every_slot_can_be_used_and_no_more() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        let all: Vec<KernelStack> = (0..SLOTS)
            .map(|_| stacks.alloc(&mut t, &mut m).unwrap())
            .collect();
        assert_eq!(all.last().unwrap().slot, SLOTS - 1);
        assert_eq!(stacks.alloc(&mut t, &mut m), None);
        for s in all {
            stacks.free(s, &mut t, &mut m);
        }
        assert!(stacks.alloc(&mut t, &mut m).is_some());
    }

    #[test]
    fn running_out_of_frames_leaks_nothing() {
        let (mut m, mut t) = setup();
        let mut stacks = KernelStacks::new();
        // A frame for the first page, but none for the tables it needs.
        let cold = m.frames();
        m.limit = cold + 1;
        assert_eq!(stacks.alloc(&mut t, &mut m), None);
        assert_eq!(m.frames(), cold);
        m.limit = usize::MAX;
        // Tables for the first slot, so only the stack's own frames vary.
        let warm = stacks.alloc(&mut t, &mut m).unwrap();
        stacks.free(warm, &mut t, &mut m);
        let before = m.frames();
        for extra in [0, 1, 7, 15] {
            m.limit = before + extra;
            assert_eq!(stacks.alloc(&mut t, &mut m), None, "{extra} frames");
            assert_eq!(m.frames(), before);
        }
        m.limit = usize::MAX;
        assert_eq!(stacks.alloc(&mut t, &mut m), Some(KernelStack { slot: 0 }));
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
pub mod heap;
pub mod paging;
````

with:

````rust
pub mod heap;
pub mod kstack;
pub mod paging;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib kstack`

Expected: FAIL: compile errors such as `` cannot find type `PageTables` in this scope ``; `` cannot find value `AREA` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/mm/kstack.rs`**

Insert this at the top of `kernel/src/mm/kstack.rs`, above `#[cfg(test)]`:

````rust
//! The kernel-stack area (user-space gate §5.4): the stacks the kernel runs
//! on while it serves a program (its system calls, interrupts and faults).
//! One slot per process-table entry, each a 64 KiB stack above an unmapped
//! guard page, so a kernel stack overflow faults instead of running into
//! the next stack. The area has PML4 entry 509 to itself, which every
//! address space shares.

use super::paging::{Cache, PAGE, PageTables, PhysMem};

/// The area's first address: PML4 entry 509, between the loader's kernel
/// stack (508) and the kernel image (511).
pub const AREA: u64 = 0xFFFF_FE80_0000_0000;
/// Slots, one per process-table entry (user-space gate §5.4).
pub const SLOTS: usize = 64;
/// Bytes of stack in each slot.
pub const SIZE: u64 = 64 * 1024;
/// Each slot: the guard page, then the stack; the rest stays unmapped.
const SLOT_SIZE: u64 = 128 * 1024;

/// A kernel stack in one slot; `top` is where it starts (it grows down).
#[derive(Debug, PartialEq, Eq)]
pub struct KernelStack {
    slot: usize,
}

impl KernelStack {
    /// The lowest address of the stack; the page below is the guard.
    pub fn bottom(&self) -> u64 {
        AREA + self.slot as u64 * SLOT_SIZE + PAGE
    }

    /// The first address above the stack, 16-byte aligned.
    pub fn top(&self) -> u64 {
        self.bottom() + SIZE
    }

    /// The address of each of its pages.
    pub fn pages(&self) -> impl Iterator<Item = u64> + use<> {
        let bottom = self.bottom();
        (0..SIZE / PAGE).map(move |i| bottom + i * PAGE)
    }
}

/// Which slots are in use.
#[derive(Default)]
pub struct KernelStacks {
    used: u64,
}

impl KernelStacks {
    pub const fn new() -> KernelStacks {
        KernelStacks { used: 0 }
    }

    /// Maps a stack in a free slot; `None` when every slot is in use or
    /// there are no frames (what was mapped is given back).
    pub fn alloc(
        &mut self,
        tables: &mut PageTables,
        mem: &mut impl PhysMem,
    ) -> Option<KernelStack> {
        let slot = (0..SLOTS).find(|&s| self.used & (1 << s) == 0)?;
        let stack = KernelStack { slot };
        for (n, virt) in stack.pages().enumerate() {
            let mapped = mem.alloc_table().map(|frame| {
                let r = tables.map(mem, virt, frame, PAGE, Cache::WriteBack);
                if r.is_err() {
                    mem.free_frame(frame);
                }
                r
            });
            if !matches!(mapped, Some(Ok(()))) {
                release(&stack, n, tables, mem);
                return None;
            }
        }
        self.used |= 1 << slot;
        Some(stack)
    }

    /// Gives back the stack's frames and its slot. The caller flushes the
    /// TLB for its pages.
    pub fn free(&mut self, stack: KernelStack, tables: &mut PageTables, mem: &mut impl PhysMem) {
        release(&stack, (SIZE / PAGE) as usize, tables, mem);
        self.used &= !(1 << stack.slot);
    }
}

/// Unmaps and frees the first `pages` pages of `stack`.
fn release(stack: &KernelStack, pages: usize, tables: &mut PageTables, mem: &mut impl PhysMem) {
    for virt in stack.pages().take(pages) {
        if let Some(frame) = tables.unmap(mem, virt) {
            mem.free_frame(frame);
        }
    }
}

````

- [ ] **Step 5: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use heap::{HeapStats, KernelHeap};
use paging::{Cache, MapError, PAGE, PAT_VALUE, PageTables, PhysMem};
````

with:

````rust
use heap::{HeapStats, KernelHeap};
use kstack::{KernelStack, KernelStacks};
use paging::{Cache, MapError, PAGE, PAT_VALUE, PageTables, PhysMem};
````

Replace:

````rust
    tables: PageTables,
}
````

with:

````rust
    tables: PageTables,
    stacks: KernelStacks,
}
````

Replace:

````rust
    f(&mut UserMem(LinearMem(&mut m.frames)), &m.tables)
}
````

with:

````rust
    f(&mut UserMem(LinearMem(&mut m.frames)), &m.tables)
}

/// A kernel stack for a program (user-space gate §5.4), from the frames
/// the kernel keeps for itself; `None` when every slot is in use.
pub fn alloc_kernel_stack() -> Option<KernelStack> {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    m.stacks.alloc(&mut m.tables, &mut LinearMem(&mut m.frames))
}

/// Gives back a kernel stack nothing runs on any more.
pub fn free_kernel_stack(stack: KernelStack) {
    let mut guard = MEMORY.lock();
    let m = guard.as_mut().expect("mm::init has not run");
    let pages: alloc::vec::Vec<u64> = stack.pages().collect();
    m.stacks
        .free(stack, &mut m.tables, &mut LinearMem(&mut m.frames));
    for virt in pages {
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
    }
}
````

Replace:

````rust

    *MEMORY.lock() = Some(Memory { frames, tables });
    heap_self_test();
````

with:

````rust

    *MEMORY.lock() = Some(Memory {
        frames,
        tables,
        stacks: KernelStacks::new(),
    });
    heap_self_test();
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib kstack`

Expected: PASS: 5 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -m "kernel: kernel stacks for programs, each above a guard page"
````


### Task 5: `UserSlice`: a program's memory, checked page by page

A pointer and a length a program passes to a system call name memory that must be the program's (spec §11.1): `UserSlice::new` refuses a range that is not inside the lower half (ends past it, wraps around, starts in the kernel) with `EFAULT`, and `read` walks the program's tables for every page it copies, so an unmapped page, a kernel page or a page without the user bit is `EFAULT`, never a kernel fault. The bytes are copied from the frames the tables name, through the linear map: the kernel never touches a program's addresses itself, so SMAP (plan 3) needs no `stac`/`clac`, and nothing can unmap the pages during the call (spec §6.1). Plan 2 has one call that reads a program's memory (`write`) and none that writes it, so `UserSlice` copies in only; copying out and `UserStr` come with plan 3's first calls that need them (decision 4). Mutation checks: dropping the lower-half bound, the range-end check or the page walk's last byte fails a test.

**Files:**
- Modify: `kernel/src/mm/mod.rs`
- Create: `kernel/src/mm/user.rs`

**Interfaces:**
- Consumes: Tasks 1 (`Errno::EFAULT`), 2 and 3 (`AddressSpace::user_page`).
- Produces: `mm::user::UserSlice::{new(addr, len) -> Result<UserSlice, Errno>, len, is_empty, read(&self, &AddressSpace, &mut impl PhysMem, offset: u64, buf: &mut [u8]) -> Result<(), Errno>}`.

- [ ] **Step 1: Declare the new module in `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
pub mod testing;

````

with:

````rust
pub mod testing;
pub mod user;

````

- [ ] **Step 2: Write the failing tests for `kernel/src/mm/user.rs`**

Create `kernel/src/mm/user.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::{PageTables, Perm};
    use crate::mm::testing::FakeMem;

    const U: u64 = 0x40_0000;

    /// Two readable pages at `U` (code, then read-only data), a hole, then
    /// a writable page; and the last page of the lower half.
    fn program(m: &mut FakeMem) -> AddressSpace {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        let mut s = AddressSpace::new(m, &k).unwrap();
        s.map_zeroed(m, U, 1, Perm::ReadExec).unwrap();
        s.map_zeroed(m, U + PAGE, 1, Perm::Read).unwrap();
        s.map_zeroed(m, U + 3 * PAGE, 1, Perm::ReadWrite).unwrap();
        s.map_zeroed(m, LOWER_HALF_END - PAGE, 1, Perm::ReadWrite)
            .unwrap();
        let text: Vec<u8> = (0..2 * PAGE).map(|i| i as u8).collect();
        s.fill(m, U, &text).unwrap();
        s
    }

    fn read(m: &mut FakeMem, s: &AddressSpace, addr: u64, len: u64) -> Result<Vec<u8>, Errno> {
        let slice = UserSlice::new(addr, len)?;
        let mut buf = vec![0; len as usize];
        slice.read(s, m, 0, &mut buf).map(|()| buf)
    }

    #[test]
    fn a_range_is_read_across_pages_of_any_permission() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let want: Vec<u8> = (PAGE - 3..PAGE + 5).map(|i| i as u8).collect();
        assert_eq!(read(&mut m, &s, U + PAGE - 3, 8), Ok(want));
        assert_eq!(read(&mut m, &s, U + 3 * PAGE, 4), Ok(vec![0; 4]));
        let whole = read(&mut m, &s, U, 2 * PAGE).unwrap();
        assert_eq!(whole[4097], 1);
        // In pieces, at an offset.
        let slice = UserSlice::new(U + 10, 100).unwrap();
        let mut buf = [0; 5];
        slice.read(&s, &mut m, 90, &mut buf).unwrap();
        assert_eq!(buf, [100, 101, 102, 103, 104]);
        assert_eq!(
            slice.read(&s, &mut m, 96, &mut buf),
            Err(Errno::EFAULT),
            "past its end"
        );
        assert_eq!(
            slice.read(&s, &mut m, u64::MAX, &mut buf),
            Err(Errno::EFAULT)
        );
    }

    #[test]
    fn a_page_the_program_does_not_have_is_efault() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        assert_eq!(
            read(&mut m, &s, U + 2 * PAGE, 1),
            Err(Errno::EFAULT),
            "the hole"
        );
        assert_eq!(
            read(&mut m, &s, U + 2 * PAGE - 1, 2),
            Err(Errno::EFAULT),
            "into the hole"
        );
        assert_eq!(read(&mut m, &s, 0, 1), Err(Errno::EFAULT), "null");
        assert_eq!(read(&mut m, &s, U - 1, 2), Err(Errno::EFAULT), "from below");
    }

    #[test]
    fn nothing_outside_the_lower_half_is_a_program_s() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        let last = LOWER_HALF_END - PAGE;
        assert_eq!(read(&mut m, &s, last, PAGE), Ok(vec![0; PAGE as usize]));
        assert_eq!(
            read(&mut m, &s, last, PAGE + 1),
            Err(Errno::EFAULT),
            "across the end"
        );
        assert_eq!(UserSlice::new(LOWER_HALF_END, 1), Err(Errno::EFAULT));
        assert_eq!(
            UserSlice::new(0xFFFF_8000_0000_0000, 8),
            Err(Errno::EFAULT),
            "the kernel"
        );
        assert_eq!(UserSlice::new(0xFFFF_FFFF_8000_0000, 8), Err(Errno::EFAULT));
        assert_eq!(
            UserSlice::new(u64::MAX, 2),
            Err(Errno::EFAULT),
            "wraps around"
        );
        assert_eq!(UserSlice::new(U, u64::MAX), Err(Errno::EFAULT));
    }

    #[test]
    fn an_empty_range_reads_nothing_anywhere_below_the_end() {
        let mut m = FakeMem::new();
        let s = program(&mut m);
        assert_eq!(read(&mut m, &s, 0, 0), Ok(vec![]));
        assert_eq!(read(&mut m, &s, LOWER_HALF_END, 0), Ok(vec![]));
        assert!(UserSlice::new(U, 0).unwrap().is_empty());
        assert_eq!(UserSlice::new(U, 7).unwrap().len(), 7);
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib mm::user`

Expected: FAIL: compile errors such as `` cannot find type `AddressSpace` in this scope ``; `` cannot find value `PAGE` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/mm/user.rs`**

Insert this at the top of `kernel/src/mm/user.rs`, above `#[cfg(test)]`:

````rust
//! Memory a program names in a system call (user-space gate §11.1): a
//! range of its own address space, checked page by page against its page
//! tables before a byte is copied. A pointer into the upper half, past the
//! lower half's end, onto a page the program does not have, or one that
//! wraps around is `EFAULT`, never a kernel fault. The bytes are copied
//! through the frames the tables name, so the kernel never touches a
//! program's addresses itself, and nothing can unmap them during the call
//! (§6.1).

use super::paging::{LOWER_HALF_END, PAGE, PhysMem};
use super::space::AddressSpace;
use vfs::Errno;

/// `len` bytes at `addr` in a program's memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserSlice {
    addr: u64,
    len: u64,
}

impl UserSlice {
    /// A range that lies in the lower half; `EFAULT` otherwise. Whether
    /// its pages are mapped is checked when it is copied.
    pub fn new(addr: u64, len: u64) -> Result<UserSlice, Errno> {
        match addr.checked_add(len) {
            Some(end) if end <= LOWER_HALF_END => Ok(UserSlice { addr, len }),
            _ => Err(Errno::EFAULT),
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Copies `buf.len()` bytes from `offset` into the range to `buf`.
    /// `EFAULT` if they run past the range or any of their pages is not
    /// one of the program's; `buf` may then hold part of them.
    pub fn read(
        &self,
        space: &AddressSpace,
        mem: &mut impl PhysMem,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), Errno> {
        let end = offset
            .checked_add(buf.len() as u64)
            .filter(|&end| end <= self.len)
            .ok_or(Errno::EFAULT)?;
        let mut virt = self.addr + offset;
        let mut done = 0;
        while virt < self.addr + end {
            let (frame, _) = space.user_page(mem, virt).ok_or(Errno::EFAULT)?;
            let at = (virt % PAGE) as usize;
            let n = (PAGE as usize - at).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&mem.bytes(frame)[at..at + n]);
            done += n;
            virt += n as u64;
        }
        Ok(())
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib mm::user`

Expected: PASS: 4 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: UserSlice reads a program's memory through its page tables"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 24 scenario(s) passed`.

````bash
git push -u origin m2p2/memory
gh pr create --base main --head m2p2/memory --title "Milestone 2, plan 2: Memory for programs" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 1–5: `vfs::Errno` gains the error numbers plan 2 returns, with their system-call values checked against the host's C library; `paging.rs` maps programs' 4 KiB pages with their permissions and the user bit, unmaps pages and frees a whole lower half; `AddressSpace` shares the kernel's upper half, which `mm::init` now fixes (spec §5.1 corrected), and keeps 8 MiB of frames free; kernel stacks with guard pages in their own PML4 entry; `UserSlice` checks a program's pointers against its page tables.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p2/memory --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-memory
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Checking and loading programs (Tasks 6–8)

The rules a program's file follows, as a crate the kernel and the build share, and loading a checked program into an address space.

Branch `m2p2/elf`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-elf`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p2/elf /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-elf origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-elf
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p2/memory` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p2/elf /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-elf m2p2/memory`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p2/memory>` and re-run `cargo xtask ci` before pushing.

### Task 6: `crates/elf`: the rules a program's file follows

The kernel checks every program it loads (spec §5.2), and the build must refuse exactly what the kernel refuses, so the checks are a crate both use, as `sysimg` is (decision 5): `elf::check(file, machine, abi)` accepts at most 16 MiB of a little-endian ELF64 `ET_EXEC` for `machine` whose program headers are 56 bytes and lie in the file; only `PT_LOAD`, `PT_NOTE` and `PT_GNU_STACK` headers; at least one `PT_LOAD`, each inside `0x40_0000`–`0x1000_0000_0000`, never writable and executable, with no more file than memory, its file bytes in the file, its offset and address congruent modulo 4 KiB, and on pages of its own; the entry point in an executable segment; and a `PT_NOTE` holding the `Relay` note of type 1 whose 4-byte descriptor is `abi`. It returns the entry point and the loadable segments in address order, with their access. The machine is the caller's (`EM_X86_64` from the kernel's `arch`), so nothing in the crate is x86-specific but that constant. OS/ABI and the ident version are not rules (plan 1's ruling). The file comes from a disk, so a randomized test damages a program shaped like `t-args` 20,000 times and `check` must never panic, and whatever it accepts must follow every rule. Mutation checks: each rule's removal fails a test; rounding the previous segment's end up to a page in the overlap check is an equivalent mutant (a page-aligned start is below an end exactly when it is below that end rounded up), so the check compares with the end itself.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/elf/Cargo.toml`
- Create: `crates/elf/src/check.rs`
- Create: `crates/elf/src/lib.rs`

**Interfaces:**
- Consumes: plan 1's `relay_abi::{NOTE_NAME, NOTE_TYPE, VERSION}`.
- Produces: crate `elf`: `check(file: &[u8], machine: u16, abi: u32) -> Result<Program, ElfError>`, `Program { entry, segments: Vec<Segment> }`, `Segment { vaddr, memsz, offset, filesz, access }` with `end()`, `Access::{ReadExec, Read, ReadWrite}`, `ElfError` (with `Display`), `MAX_SIZE`, `PROGRAM_BASE`, `PROGRAM_END`, `EM_X86_64`.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "crates/elf", "xtask"]

````

Replace:

````toml
relay-rt = { path = "crates/relay-rt" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
relay-rt = { path = "crates/relay-rt" }
elf = { path = "crates/elf" }
uefi = { version = "0.41", default-features = false }
````

- [ ] **Step 2: Create `crates/elf/Cargo.toml`**

Create `crates/elf/Cargo.toml`:

````toml
[package]
name = "elf"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
relay-abi.workspace = true
````

- [ ] **Step 3: Write the failing tests for `crates/elf/src/check.rs`**

Create `crates/elf/src/check.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::VERSION;

    /// A program header: type, flags, offset, vaddr, filesz, memsz.
    type Phdr = (u32, u32, u64, u64, u64, u64);

    const NOTE_AT: u64 = 0x2000;

    /// The `Relay` note with ABI `v`, as `relay-rt` lays it out.
    fn note(v: u32) -> Vec<u8> {
        let mut n = Vec::new();
        n.extend_from_slice(&6u32.to_le_bytes());
        n.extend_from_slice(&4u32.to_le_bytes());
        n.extend_from_slice(&1u32.to_le_bytes());
        n.extend_from_slice(b"Relay\0\0\0");
        n.extend_from_slice(&v.to_le_bytes());
        n
    }

    /// A program shaped like `t-args`: code at 0x401000, read-only data
    /// holding the note at 0x402000, data and bss at 0x403000.
    fn phdrs() -> Vec<Phdr> {
        vec![
            (PT_LOAD, 5, 0x1000, 0x40_1000, 0x800, 0x800),
            (PT_LOAD, 4, NOTE_AT, 0x40_2000, 0x40, 0x40),
            (PT_LOAD, 6, 0x3000, 0x40_3000, 0x10, 0x2000),
            (PT_NOTE, 4, NOTE_AT, 0x40_2000, 0x18, 0x18),
            (PT_GNU_STACK, 6, 0, 0, 0, 0),
        ]
    }

    fn elf(phdrs: &[Phdr], entry: u64) -> Vec<u8> {
        let mut f = vec![0u8; 0x3010];
        f[..4].copy_from_slice(b"\x7fELF");
        f[4] = 2;
        f[5] = 1;
        f[6] = 1;
        f[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        f[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        f[24..32].copy_from_slice(&entry.to_le_bytes());
        f[32..40].copy_from_slice(&64u64.to_le_bytes());
        f[54..56].copy_from_slice(&(PHDR_LEN as u16).to_le_bytes());
        f[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());
        for (i, &(kind, flags, offset, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
            let at = 64 + i * PHDR_LEN;
            f[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            f[at + 4..at + 8].copy_from_slice(&flags.to_le_bytes());
            f[at + 8..at + 16].copy_from_slice(&offset.to_le_bytes());
            f[at + 16..at + 24].copy_from_slice(&vaddr.to_le_bytes());
            f[at + 32..at + 40].copy_from_slice(&filesz.to_le_bytes());
            f[at + 40..at + 48].copy_from_slice(&memsz.to_le_bytes());
        }
        let n = note(VERSION);
        f[NOTE_AT as usize..NOTE_AT as usize + n.len()].copy_from_slice(&n);
        f
    }

    fn good() -> Vec<u8> {
        elf(&phdrs(), 0x40_1000)
    }

    fn check_x86(f: &[u8]) -> Result<Program, ElfError> {
        check(f, EM_X86_64, VERSION)
    }

    /// `good()` with program header `i` changed by `edit`.
    fn with(i: usize, edit: impl Fn(&mut Phdr)) -> Result<Program, ElfError> {
        let mut p = phdrs();
        edit(&mut p[i]);
        check_x86(&elf(&p, 0x40_1000))
    }

    #[test]
    fn a_program_like_t_args_is_accepted() {
        let p = check_x86(&good()).unwrap();
        assert_eq!(p.entry, 0x40_1000);
        let access: Vec<Access> = p.segments.iter().map(|s| s.access).collect();
        assert_eq!(access, [Access::ReadExec, Access::Read, Access::ReadWrite]);
        assert_eq!(
            p.segments[2],
            Segment {
                vaddr: 0x40_3000,
                memsz: 0x2000,
                offset: 0x3000,
                filesz: 0x10,
                access: Access::ReadWrite
            }
        );
        assert_eq!(p.segments[2].end(), 0x40_5000);
    }

    #[test]
    fn the_file_header_is_checked() {
        let edit = |at: usize, v: u8| {
            let mut f = good();
            f[at] = v;
            check_x86(&f)
        };
        assert_eq!(edit(0, 0x7e), Err(ElfError::NotElf));
        assert_eq!(edit(4, 1), Err(ElfError::Not64Bit));
        assert_eq!(edit(5, 2), Err(ElfError::NotLittleEndian));
        assert_eq!(edit(16, 3), Err(ElfError::NotExec(3)), "DYN");
        assert_eq!(edit(18, 0xB7), Err(ElfError::Machine(0xB7)), "aarch64");
        assert_eq!(check(&good(), 0xB7, VERSION), Err(ElfError::Machine(62)));
        assert_eq!(edit(54, 64), Err(ElfError::ProgramHeaders));
        assert_eq!(edit(57, 1), Err(ElfError::ProgramHeaders), "too many");
        assert_eq!(check_x86(&good()[..63]), Err(ElfError::NotElf));
        assert_eq!(check_x86(b""), Err(ElfError::NotElf));
        // OS/ABI and the ident version are not rules (plan 1's ruling).
        assert!(edit(7, 3).is_ok());
        assert!(edit(6, 0).is_ok());
    }

    #[test]
    fn at_most_16_mib() {
        let mut f = good();
        f.resize(MAX_SIZE, 0);
        assert!(check_x86(&f).is_ok());
        f.push(0);
        assert_eq!(check_x86(&f), Err(ElfError::TooBig(MAX_SIZE + 1)));
    }

    #[test]
    fn only_load_note_and_gnu_stack_segments() {
        for kind in [0, 2, 3, 6, 7, 0x6474_e550, 0x6474_e552] {
            assert_eq!(with(4, |p| p.0 = kind), Err(ElfError::SegmentKind(kind)));
        }
        assert_eq!(
            check_x86(&elf(&phdrs()[3..], 0x40_1000)),
            Err(ElfError::NoLoad)
        );
    }

    #[test]
    fn segments_stay_in_the_program_area() {
        assert_eq!(
            with(0, |p| p.3 = 0x3F_F000),
            Err(ElfError::Outside { vaddr: 0x3F_F000 })
        );
        assert_eq!(
            with(2, |p| p.3 = PROGRAM_END - 0x1000),
            Err(ElfError::Outside {
                vaddr: PROGRAM_END - 0x1000
            }),
            "its bss runs past the end"
        );
        assert_eq!(
            with(2, |p| p.5 = u64::MAX),
            Err(ElfError::Outside { vaddr: 0x40_3000 }),
            "overflows"
        );
        // Right at both ends is fine.
        let mut p = phdrs();
        p[0] = (PT_LOAD, 5, 0, PROGRAM_BASE, 0x800, 0x800);
        p[2] = (PT_LOAD, 6, 0x3000, PROGRAM_END - 0x2000, 0x10, 0x2000);
        assert!(check_x86(&elf(&p, PROGRAM_BASE)).is_ok());
    }

    #[test]
    fn each_segment_rule() {
        assert_eq!(
            with(0, |p| p.1 = 7),
            Err(ElfError::WritableAndExecutable { vaddr: 0x40_1000 })
        );
        assert_eq!(
            with(2, |p| p.4 = 0x2001),
            Err(ElfError::MoreFileThanMemory { vaddr: 0x40_3000 })
        );
        assert_eq!(
            with(0, |p| p.2 = 0x1800),
            Err(ElfError::NotCongruent { vaddr: 0x40_1000 })
        );
        assert_eq!(
            with(2, |p| (p.4, p.5) = (0x1000, 0x1000)),
            Err(ElfError::PastTheFile { vaddr: 0x40_3000 })
        );
        assert_eq!(
            with(2, |p| p.2 = u64::MAX - 0xFFF),
            Err(ElfError::PastTheFile { vaddr: 0x40_3000 }),
            "an offset that overflows"
        );
        // On the same page as the one before, even without sharing a byte.
        assert_eq!(
            with(1, |p| (p.2, p.3) = (0x1900, 0x40_1900)),
            Err(ElfError::Overlap { vaddr: 0x40_1900 })
        );
        assert_eq!(
            with(1, |p| p.3 = 0x40_1000),
            Err(ElfError::Overlap { vaddr: 0x40_1000 })
        );
        // Out of order in the file is fine: they are sorted.
        let mut p = phdrs();
        p.swap(0, 2);
        assert_eq!(
            check_x86(&elf(&p, 0x40_1000)).unwrap().segments[0].vaddr,
            0x40_1000
        );
        // Writable-only and execute-only flags.
        assert_eq!(
            with(2, |p| p.1 = 2).unwrap().segments[2].access,
            Access::ReadWrite
        );
        assert_eq!(
            with(0, |p| p.1 = 1).unwrap().segments[0].access,
            Access::ReadExec
        );
    }

    #[test]
    fn the_entry_point_is_in_an_executable_segment() {
        let p = phdrs();
        assert!(check_x86(&elf(&p, 0x40_17FF)).is_ok());
        for entry in [0x40_1800, 0x40_0FFF, 0x40_2000, 0x40_3000, 0] {
            assert_eq!(check_x86(&elf(&p, entry)), Err(ElfError::Entry(entry)));
        }
    }

    #[test]
    fn the_relay_note_names_this_abi() {
        let at = NOTE_AT as usize;
        let edit = |edit: &dyn Fn(&mut Vec<u8>)| {
            let mut f = good();
            edit(&mut f);
            check_x86(&f)
        };
        assert_eq!(
            edit(&|f| f[at + 20] = f[at + 20].wrapping_add(1)),
            Err(ElfError::Abi(VERSION + 1))
        );
        assert_eq!(
            check(&good(), EM_X86_64, VERSION + 1),
            Err(ElfError::Abi(VERSION))
        );
        assert_eq!(edit(&|f| f[at + 12] = b'X'), Err(ElfError::NoNote));
        assert_eq!(edit(&|f| f[at + 8] = 2), Err(ElfError::NoteType(2)));
        assert_eq!(edit(&|f| f[at + 4] = 8), Err(ElfError::NotePastItsSegment));
        assert_eq!(
            edit(&|f| f[at + 3] = 0x80),
            Err(ElfError::NotePastItsSegment)
        );
        // A name without its NUL is the same name.
        assert!(edit(&|f| f[at] = 5).is_ok());
        // Another owner's note first, then ours.
        let mut p = phdrs();
        p[3].2 = NOTE_AT - 0x14;
        p[3].4 = 0x14 + 0x18;
        let mut f = elf(&p, 0x40_1000);
        let other = [4u32.to_le_bytes(), 4u32.to_le_bytes(), 1u32.to_le_bytes()].concat();
        f[at - 0x14..at - 8].copy_from_slice(&other[..12]);
        f[at - 8..at - 4].copy_from_slice(b"GNU\0");
        assert!(check_x86(&f).is_ok());
        // Only a PT_NOTE segment counts, and it must lie in the file.
        assert_eq!(with(3, |p| p.0 = PT_GNU_STACK), Err(ElfError::NoNote));
        assert_eq!(with(3, |p| p.2 = 0x10_0000), Err(ElfError::NoteOutside));
        // A descriptor of another size.
        let mut p = phdrs();
        p[3].4 = 0x1C;
        let mut f = elf(&p, 0x40_1000);
        f[at + 4] = 8;
        assert_eq!(check_x86(&f), Err(ElfError::NoteSize(8)));
    }

    /// A small deterministic generator (xorshift64*).
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    /// Damages the header, program headers and note 20,000 times: `check`
    /// must never panic, and what it accepts must follow every rule.
    #[test]
    fn random_damage_never_panics_and_what_passes_follows_the_rules() {
        let base = good();
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        let mut accepted = 0;
        for _ in 0..20_000 {
            let mut f = base.clone();
            for _ in 0..1 + rng.next() % 4 {
                let at = match rng.next() % 3 {
                    0 => rng.next() % 64,
                    1 => 64 + rng.next() % (5 * PHDR_LEN as u64),
                    _ => NOTE_AT + rng.next() % 0x18,
                } as usize;
                f[at] = match rng.next() % 4 {
                    0 => 0,
                    1 => 0xFF,
                    _ => rng.next() as u8,
                };
            }
            if rng.next().is_multiple_of(8) {
                let len = rng.next() as usize % f.len();
                f.truncate(len);
            }
            let Ok(p) = check_x86(&f) else { continue };
            accepted += 1;
            let mut previous_end = 0;
            for s in &p.segments {
                assert!(s.vaddr >= PROGRAM_BASE && s.end() <= PROGRAM_END);
                assert!(s.filesz <= s.memsz);
                assert!(range(&f, s.offset, s.filesz).is_some());
                assert!(s.vaddr & !(PAGE - 1) >= previous_end);
                previous_end = s.end();
            }
            assert!(
                p.segments
                    .iter()
                    .any(|s| s.access == Access::ReadExec && (s.vaddr..s.end()).contains(&p.entry))
            );
        }
        assert!(
            accepted > 1000,
            "only {accepted} accepted: the damage is too coarse"
        );
    }

    #[test]
    fn the_messages_name_what_is_wrong() {
        assert_eq!(ElfError::NotExec(3).to_string(), "type 3, not EXEC");
        assert_eq!(
            ElfError::WritableAndExecutable { vaddr: 0x40_1000 }.to_string(),
            "segment at 0x401000 is writable and executable"
        );
        assert_eq!(
            ElfError::Outside { vaddr: 0x1000 }.to_string(),
            "segment at 0x1000 is outside 0x400000..0x100000000000"
        );
        assert_eq!(ElfError::Abi(2).to_string(), "built for ABI 2");
        assert_eq!(
            ElfError::TooBig(16777217).to_string(),
            "16777217 bytes, more than 16 MiB"
        );
    }
}
````

- [ ] **Step 4: Create `crates/elf/src/lib.rs`**

Create `crates/elf/src/lib.rs`:

````rust
//! The rules a program's ELF file must follow before the kernel runs it
//! (spec §5.2 of the user-space gate): a static ELF64 executable with its
//! segments in the program area and the ABI's note. The kernel checks every
//! program `spawn` loads with [`check`]; xtask checks every program it
//! builds with the same function, so the build refuses exactly what the
//! kernel would.
//!
//! Everything here is `no_std + alloc` over a byte slice, host-tested with
//! randomized input: the file comes from a disk, so nothing in it may make
//! [`check`] panic or read outside it.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod check;

pub use check::{
    Access, EM_X86_64, ElfError, MAX_SIZE, PROGRAM_BASE, PROGRAM_END, Program, Segment, check,
};
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p elf`

Expected: FAIL: compile errors such as `` cannot find value `PT_LOAD` in this scope ``; `` cannot find value `PT_NOTE` in this scope ``.

- [ ] **Step 6: Implement `crates/elf/src/check.rs`**

Insert this at the top of `crates/elf/src/check.rs`, above `#[cfg(test)]`:

````rust
//! [`check`]: the file header, the program headers, the loadable segments,
//! the entry point and the ABI note, in that order.

use alloc::vec::Vec;
use core::fmt;

/// The largest program the kernel loads (spec §5.2).
pub const MAX_SIZE: usize = 16 << 20;
/// Where a program's segments may lie (spec §5.1): above the unmapped
/// first 4 MiB, below the `mem_map` area.
pub const PROGRAM_BASE: u64 = 0x40_0000;
pub const PROGRAM_END: u64 = 0x1000_0000_0000;
/// `e_machine` of x86_64 programs.
pub const EM_X86_64: u16 = 62;

const HEADER_LEN: usize = 64;
const PHDR_LEN: usize = 56;
const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;
const PT_NOTE: u32 = 4;
const PT_GNU_STACK: u32 = 0x6474_e551;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PAGE: u64 = 4096;

/// What the program may do with a segment's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadExec,
    Read,
    ReadWrite,
}

/// A loadable segment: `memsz` bytes at `vaddr`, the first `filesz` of
/// them from the file at `offset`, the rest zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub vaddr: u64,
    pub memsz: u64,
    pub offset: u64,
    pub filesz: u64,
    pub access: Access,
}

impl Segment {
    /// The first address after it.
    pub fn end(&self) -> u64 {
        self.vaddr + self.memsz
    }
}

/// A program that passed every check: its entry point and its loadable
/// segments, in address order, on pages of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub entry: u64,
    pub segments: Vec<Segment>,
}

/// Why a file is not a program the kernel runs (`ENOEXEC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    TooBig(usize),
    NotElf,
    Not64Bit,
    NotLittleEndian,
    NotExec(u16),
    Machine(u16),
    ProgramHeaders,
    SegmentKind(u32),
    NoLoad,
    Outside { vaddr: u64 },
    WritableAndExecutable { vaddr: u64 },
    MoreFileThanMemory { vaddr: u64 },
    NotCongruent { vaddr: u64 },
    Overlap { vaddr: u64 },
    PastTheFile { vaddr: u64 },
    Entry(u64),
    NoteOutside,
    NotePastItsSegment,
    NoteType(u32),
    NoteSize(u32),
    NoNote,
    Abi(u32),
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ElfError::TooBig(n) => write!(f, "{n} bytes, more than 16 MiB"),
            ElfError::NotElf => write!(f, "not an ELF file"),
            ElfError::Not64Bit => write!(f, "not ELF64"),
            ElfError::NotLittleEndian => write!(f, "not little-endian"),
            ElfError::NotExec(t) => write!(f, "type {t}, not EXEC"),
            ElfError::Machine(m) => write!(f, "machine {m}"),
            ElfError::ProgramHeaders => write!(f, "the program headers lie outside the file"),
            ElfError::SegmentKind(k) => write!(
                f,
                "has a segment of type {k:#x}; only LOAD, NOTE and GNU_STACK are allowed"
            ),
            ElfError::NoLoad => write!(f, "no loadable segment"),
            ElfError::Outside { vaddr } => write!(
                f,
                "segment at {vaddr:#x} is outside {PROGRAM_BASE:#x}..{PROGRAM_END:#x}"
            ),
            ElfError::WritableAndExecutable { vaddr } => {
                write!(f, "segment at {vaddr:#x} is writable and executable")
            }
            ElfError::MoreFileThanMemory { vaddr } => {
                write!(f, "segment at {vaddr:#x} has more file than memory")
            }
            ElfError::NotCongruent { vaddr } => write!(
                f,
                "segment at {vaddr:#x}: offset and address are not congruent modulo 4 KiB"
            ),
            ElfError::Overlap { vaddr } => {
                write!(f, "segment at {vaddr:#x} overlaps the one before it")
            }
            ElfError::PastTheFile { vaddr } => {
                write!(f, "segment at {vaddr:#x} runs past the end of the file")
            }
            ElfError::Entry(e) => write!(f, "entry point {e:#x} is not in an executable segment"),
            ElfError::NoteOutside => write!(f, "a PT_NOTE segment lies outside the file"),
            ElfError::NotePastItsSegment => write!(f, "a note runs past its segment"),
            ElfError::NoteType(t) => write!(f, "the Relay note has type {t}, not 1"),
            ElfError::NoteSize(n) => write!(f, "the Relay note holds {n} bytes, not 4"),
            ElfError::NoNote => write!(f, "no PT_NOTE segment holds the Relay note"),
            ElfError::Abi(v) => write!(f, "built for ABI {v}"),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from(u32_at(b, at)) | u64::from(u32_at(b, at + 4)) << 32
}

/// The bytes `offset..offset + len` of `file`, if they are all in it.
fn range(file: &[u8], offset: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    file.get(start..end)
}

/// Checks `file` against spec §5.2 for a machine of type `machine` and ABI
/// version `abi`, and returns what the kernel needs to load it.
pub fn check(file: &[u8], machine: u16, abi: u32) -> Result<Program, ElfError> {
    if file.len() > MAX_SIZE {
        return Err(ElfError::TooBig(file.len()));
    }
    if file.len() < HEADER_LEN || file[..4] != *b"\x7fELF" {
        return Err(ElfError::NotElf);
    }
    if file[4] != 2 {
        return Err(ElfError::Not64Bit);
    }
    if file[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }
    let kind = u16_at(file, 16);
    if kind != ET_EXEC {
        return Err(ElfError::NotExec(kind));
    }
    let m = u16_at(file, 18);
    if m != machine {
        return Err(ElfError::Machine(m));
    }
    let entry = u64_at(file, 24);
    let phoff = u64_at(file, 32);
    let phentsize = usize::from(u16_at(file, 54));
    let phnum = u64::from(u16_at(file, 56));
    if phentsize != PHDR_LEN {
        return Err(ElfError::ProgramHeaders);
    }
    let phdrs = range(file, phoff, phnum * PHDR_LEN as u64).ok_or(ElfError::ProgramHeaders)?;

    let mut segments = Vec::new();
    let mut notes = Vec::new();
    for ph in phdrs.as_chunks::<PHDR_LEN>().0 {
        let (kind, flags) = (u32_at(ph, 0), u32_at(ph, 4));
        let (offset, vaddr) = (u64_at(ph, 8), u64_at(ph, 16));
        let (filesz, memsz) = (u64_at(ph, 32), u64_at(ph, 40));
        match kind {
            PT_LOAD => {
                let access = if flags & PF_X != 0 {
                    if flags & PF_W != 0 {
                        return Err(ElfError::WritableAndExecutable { vaddr });
                    }
                    Access::ReadExec
                } else if flags & PF_W != 0 {
                    Access::ReadWrite
                } else {
                    Access::Read
                };
                segments.push(Segment {
                    vaddr,
                    memsz,
                    offset,
                    filesz,
                    access,
                });
            }
            PT_NOTE => notes.push(range(file, offset, filesz).ok_or(ElfError::NoteOutside)?),
            PT_GNU_STACK => {}
            other => return Err(ElfError::SegmentKind(other)),
        }
    }
    if segments.is_empty() {
        return Err(ElfError::NoLoad);
    }
    segments.sort_by_key(|s| s.vaddr);
    let mut previous_end = 0;
    for s in &segments {
        let vaddr = s.vaddr;
        match vaddr.checked_add(s.memsz) {
            Some(end) if vaddr >= PROGRAM_BASE && end <= PROGRAM_END => {}
            _ => return Err(ElfError::Outside { vaddr }),
        }
        if s.filesz > s.memsz {
            return Err(ElfError::MoreFileThanMemory { vaddr });
        }
        if s.offset % PAGE != vaddr % PAGE {
            return Err(ElfError::NotCongruent { vaddr });
        }
        if range(file, s.offset, s.filesz).is_none() {
            return Err(ElfError::PastTheFile { vaddr });
        }
        // Segments may not share a page: each page gets one permission.
        if vaddr & !(PAGE - 1) < previous_end {
            return Err(ElfError::Overlap { vaddr });
        }
        previous_end = s.end();
    }
    let executable = |s: &&Segment| s.access == Access::ReadExec;
    if !segments
        .iter()
        .filter(executable)
        .any(|s| (s.vaddr..s.end()).contains(&entry))
    {
        return Err(ElfError::Entry(entry));
    }
    let mut version = None;
    for n in notes {
        if let Some(v) = relay_note(n)? {
            version = Some(v);
        }
    }
    match version {
        None => Err(ElfError::NoNote),
        Some(v) if v != abi => Err(ElfError::Abi(v)),
        Some(_) => Ok(Program { entry, segments }),
    }
}

/// The ABI version in the `Relay` note among `notes` (a `PT_NOTE`
/// segment's bytes), if it has one: each note is `namesz`, `descsz`,
/// `type`, then the name and the descriptor, each padded to 4 bytes.
fn relay_note(notes: &[u8]) -> Result<Option<u32>, ElfError> {
    let mut at = 0usize;
    while at + 12 <= notes.len() {
        let namesz = u32_at(notes, at) as usize;
        let descsz = u32_at(notes, at + 4);
        let kind = u32_at(notes, at + 8);
        let name_at = at + 12;
        let desc_at = name_at
            .checked_add(namesz.next_multiple_of(4))
            .ok_or(ElfError::NotePastItsSegment)?;
        let next = desc_at
            .checked_add((descsz as usize).next_multiple_of(4))
            .filter(|&next| next <= notes.len())
            .ok_or(ElfError::NotePastItsSegment)?;
        let name = &notes[name_at..name_at + namesz];
        if name.strip_suffix(&[0]).unwrap_or(name) == relay_abi::NOTE_NAME {
            if kind != relay_abi::NOTE_TYPE {
                return Err(ElfError::NoteType(kind));
            }
            if descsz != 4 {
                return Err(ElfError::NoteSize(descsz));
            }
            return Ok(Some(u32_at(notes, desc_at)));
        }
        at = next;
    }
    Ok(None)
}

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p elf`

Expected: PASS: 10 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add Cargo.lock Cargo.toml crates
git commit -m "elf: the rules a program's ELF file follows, for the kernel and the build"
````


### Task 7: The build checks every program as the kernel will

`cargo xtask build` now checks each program twice: with binutils' `readelf` (plan 1's `check_program`, independent of our code) and with the kernel's own `elf::check`, so a program the kernel would refuse never reaches `system.img`. The patched-`t-args` tests of `xtask/src/userland.rs` run every patched file through both, and require them to agree on whether it passes: each rule is checked by two independent readers, and a rule one side lacks shows up as a disagreement. `check_program` gains what plan 1's final review deferred to plan 2 (decision 5): the file must be ELF64 (`Class:`), little-endian (`Data:`) and at most 16 MiB, as the kernel's check already requires; new patches make `t-args` ELF32, big-endian, exactly 16 MiB and one byte more. `PROGRAM_BASE` and `PROGRAM_END` come from the `elf` crate. Mutation checks: each new `readelf` rule fails a test; the kernel's little-endian and size rules fail the agreement; removing the kernel's check from `build()` survives by design (the two agree on every patched program, so no program tells them apart), and the kernel's `PT_PHDR` refusal is killed by the `elf` crate's own tests only (`t-args` has no `PT_GNU_STACK` header to patch, so a type patch also removes the note and both sides refuse).

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `xtask/Cargo.toml`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: Task 6.
- Produces: `xtask::userland::check_program` with the new rules, `kernel_check(&Path) -> Result<()>` (private), `PROGRAM_BASE`/`PROGRAM_END` re-exported from `elf`; xtask depends on `elf`.

- [ ] **Step 1: Change `xtask/Cargo.toml`**

In `xtask/Cargo.toml`, replace:

````toml
clap = { version = "4", features = ["derive"] }
ext2 = { workspace = true }
````

with:

````toml
clap = { version = "4", features = ["derive"] }
elf = { workspace = true }
ext2 = { workspace = true }
````

- [ ] **Step 2: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

    /// `t-args` with one ELF field changed, and what `check_program` says.
    fn patched(name: &str, edit: impl Fn(&mut Vec<u8>)) -> String {
````

with:

````rust

    /// Whether the build (`readelf`) and the kernel (the `elf` crate) agree
    /// on `path`; what the build says.
    fn both(path: &Path) -> String {
        let build = check_program(path);
        let kernel = kernel_check(path);
        assert_eq!(
            build.is_ok(),
            kernel.is_ok(),
            "{}: the build says {build:?}, the kernel {kernel:?}",
            path.display()
        );
        match build {
            Ok(()) => "accepted".into(),
            Err(e) => e.to_string(),
        }
    }

    /// `t-args` with one ELF field changed, and what `check_program` says.
    /// The kernel's check must refuse it as well, or accept it as well.
    fn patched(name: &str, edit: impl Fn(&mut Vec<u8>)) -> String {
````

Replace:

````rust
        fs::write(&path, &bytes).unwrap();
        match check_program(&path) {
            Ok(()) => "accepted".into(),
            Err(e) => e.to_string(),
        }
    }
````

with:

````rust
        fs::write(&path, &bytes).unwrap();
        both(&path)
    }
````

Replace:

````rust
        assert_eq!(patched("same", |_| {}), "accepted");
        let host = check_program(Path::new("/bin/true"))
            .unwrap_err()
            .to_string();
        assert!(host.contains("not EXEC"), "a Linux program: {host}");
````

with:

````rust
        assert_eq!(patched("same", |_| {}), "accepted");
        let host = both(Path::new("/bin/true"));
        assert!(host.contains("not EXEC"), "a Linux program: {host}");
````

Replace:

````rust
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
    }
````

with:

````rust
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
    }

    #[test]
    fn only_little_endian_elf64_up_to_16_mib() {
        let e = patched("elf32", |b| b[4] = 1);
        assert!(e.contains("class ELF32, not ELF64"), "{e}");
        let e = patched("big-endian", |b| b[5] = 2);
        assert!(e.contains("not little endian"), "{e}");
        let e = patched("16mib", |b| b.resize(elf::MAX_SIZE, 0));
        assert_eq!(e, "accepted", "16 MiB exactly");
        let e = patched("too-big", |b| b.resize(elf::MAX_SIZE + 1, 0));
        assert_eq!(e, "16777217 bytes, more than 16 MiB");
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask userland`

Expected: FAIL: compile errors such as `` cannot find function `kernel_check` in this scope ``.

- [ ] **Step 4: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! a program the kernel will load (spec §5.2), and packing them into
//! `system.img` (spec §4.2).

````

with:

````rust
//! a program the kernel will load (spec §5.2), and packing them into
//! `system.img` (spec §4.2). Each program is checked twice: by binutils'
//! `readelf`, independently of our code, and by the kernel's own check
//! (the `elf` crate), so the build refuses exactly what the kernel would.

````

Replace:

````rust

/// Lowest and end of the addresses a program's segments may use (spec
/// §5.1): above the unmapped first 4 MiB, below the `mem_map` area.
pub const PROGRAM_BASE: u64 = 0x40_0000;
pub const PROGRAM_END: u64 = 0x1000_0000_0000;

````

with:

````rust

pub use elf::{PROGRAM_BASE, PROGRAM_END};

````

Replace:

````rust
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}
````

with:

````rust
    for p in &programs {
        check_program(&p.path)
            .and_then(|()| kernel_check(&p.path))
            .with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}

/// The kernel's own check of `path` (spec §5.2), as `spawn` runs it.
fn kernel_check(path: &Path) -> Result<()> {
    elf::check(&fs::read(path)?, elf::EM_X86_64, relay_abi::VERSION)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("the kernel's check: {e}"))
}
````

Replace:

````rust
/// Checks the rules of spec §5.2 that the build decides, as the kernel will
/// read the program: a static x86_64 executable with only `PT_LOAD`,
/// `PT_NOTE` and `PT_GNU_STACK` program headers; loadable segments in the
````

with:

````rust
/// Checks the rules of spec §5.2 that the build decides, as the kernel will
/// read the program: at most 16 MiB, a little-endian ELF64 file, a static
/// x86_64 executable with only `PT_LOAD`,
/// `PT_NOTE` and `PT_GNU_STACK` program headers; loadable segments in the
````

Replace:

````rust
pub fn check_program(path: &Path) -> Result<()> {
    let text = readelf(path)?;
````

with:

````rust
pub fn check_program(path: &Path) -> Result<()> {
    let size = fs::metadata(path)?.len();
    ensure!(
        size <= elf::MAX_SIZE as u64,
        "{size} bytes, more than 16 MiB"
    );
    let text = readelf(path)?;
````

Replace:

````rust
    };
    ensure!(
````

with:

````rust
    };
    ensure!(
        field("Class:") == "ELF64",
        "class {}, not ELF64",
        field("Class:")
    );
    ensure!(
        field("Data:").ends_with("little endian"),
        "data {}, not little endian",
        field("Data:")
    );
    ensure!(
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 6 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add Cargo.lock xtask
git commit -m "xtask: the build checks every program as the kernel will, and ELF64, little-endian and 16 MiB"
````


### Task 8: A program in its address space

`exec::load` puts a program `elf::check` accepted into an address space (spec §5.1–§5.3): each segment on pages of its access with its file bytes at its address and zeroes around them (the bss included), a 1 MiB stack less one page from `0x7FFF_FFF0_0000` to `0x7FFF_FFFF_F000` above an unmapped guard page (the page above it is never mapped either), and the arguments at the top of the stack, each followed by a NUL, with the stack pointer 16-byte aligned below them. It returns the entry state (`Entry`: instruction pointer, stack pointer, and the arguments' address, length and count, which `arch` puts into `rdi`, `rsi` and `rdx`). `arg_bytes` builds the argument bytes: more than 64 KiB is `E2BIG`, an argument holding a NUL is `EINVAL` (decision 10). Running out of frames is `ENOMEM`; a program that does not fit the file it was checked against is `ENOEXEC`, never a panic. On an error the caller destroys the space, which gives back everything; a test runs out of frames at every seventh step of a whole load and counts them all back. Mutation checks: the stack pointer's alignment, the argument limit (in both places), the NUL, the rounding of a segment's end to a page and the `ENOMEM` mapping each fail a test.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `kernel/Cargo.toml`
- Create: `kernel/src/exec.rs`
- Modify: `kernel/src/lib.rs`

**Interfaces:**
- Consumes: Tasks 1, 3 (`AddressSpace`, `FakeMem`) and 6 (`Program`, `Segment`, `Access`).
- Produces: `exec::{STACK_TOP, STACK_PAGES, STACK_BOTTOM, ARGS_MAX, Entry { ip, sp, args, args_len, argc }, arg_bytes(&[&[u8]]) -> Result<Vec<u8>, Errno>, load(&mut AddressSpace, &mut impl PhysMem, file: &[u8], &Program, args: &[u8], argc: u64) -> Result<Entry, Errno>}`; the kernel depends on `elf`.

- [ ] **Step 1: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
sysimg.workspace = true
x86_64.workspace = true
````

with:

````toml
sysimg.workspace = true
elf.workspace = true
x86_64.workspace = true
````

- [ ] **Step 2: Write the failing tests for `kernel/src/exec.rs`**

Create `kernel/src/exec.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::PageTables;
    use crate::mm::testing::FakeMem;
    use elf::Segment;

    fn space(m: &mut FakeMem) -> AddressSpace {
        let mut k = PageTables::new(m).unwrap();
        k.fill_upper_half(m).unwrap();
        AddressSpace::new(m, &k).unwrap()
    }

    fn read(m: &mut FakeMem, s: &AddressSpace, virt: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| {
                let (frame, _) = s.user_page(m, virt + i).expect("mapped");
                m.bytes(frame)[((virt + i) % PAGE) as usize]
            })
            .collect()
    }

    /// A file of 0x4000 bytes, each its offset's low byte, and a program
    /// shaped like `t-args` over it: code, read-only data, data and bss.
    fn program() -> (Vec<u8>, Program) {
        let file: Vec<u8> = (0..0x4000u32).map(|i| i as u8).collect();
        let seg = |vaddr, memsz, offset, filesz, access| Segment {
            vaddr,
            memsz,
            offset,
            filesz,
            access,
        };
        let program = Program {
            entry: 0x40_1010,
            segments: vec![
                seg(0x40_1010, 0x7F0, 0x1010, 0x7F0, Access::ReadExec),
                seg(0x40_2000, 0x1800, 0x2000, 0x1800, Access::Read),
                seg(0x40_4800, 0x2000, 0x2800, 0x100, Access::ReadWrite),
            ],
        };
        (file, program)
    }

    #[test]
    fn segments_get_their_bytes_permissions_and_zeroes() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t"]).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
        // Code from 0x401010, zeroes before it on its page.
        assert_eq!(read(&mut m, &s, 0x40_1000, 0x10), vec![0; 0x10]);
        assert_eq!(read(&mut m, &s, 0x40_1010, 4), [0x10, 0x11, 0x12, 0x13]);
        assert_eq!(s.user_page(&mut m, 0x40_17FF).unwrap().1, Perm::ReadExec);
        // Read-only data over two pages.
        assert_eq!(read(&mut m, &s, 0x40_37FE, 2), [0xFE, 0xFF]);
        assert_eq!(s.user_page(&mut m, 0x40_3000).unwrap().1, Perm::Read);
        // Data: 0x100 bytes from the file, then bss to the end of its pages.
        assert_eq!(read(&mut m, &s, 0x40_4800, 2), [0x00, 0x01]);
        assert_eq!(read(&mut m, &s, 0x40_48FF, 2), [0xFF, 0]);
        assert_eq!(read(&mut m, &s, 0x40_6000, 0x800), vec![0; 0x800]);
        assert_eq!(s.user_page(&mut m, 0x40_67FF).unwrap().1, Perm::ReadWrite);
        // Nothing in between or around.
        for v in [0x40_0000, 0x40_7000, 0x3F_F000] {
            assert_eq!(s.user_page(&mut m, v), None, "{v:#x}");
        }
        s.destroy(&mut m);
    }

    #[test]
    fn the_stack_is_1_mib_less_a_page_above_a_guard_page() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        load(&mut s, &mut m, &file, &p, &arg_bytes(&[b"x"]).unwrap(), 1).unwrap();
        assert_eq!(STACK_BOTTOM, 0x7FFF_FFF0_0000);
        assert_eq!(
            s.user_page(&mut m, STACK_BOTTOM).unwrap().1,
            Perm::ReadWrite
        );
        assert_eq!(
            s.user_page(&mut m, STACK_TOP - 1).unwrap().1,
            Perm::ReadWrite
        );
        assert_eq!(s.user_page(&mut m, STACK_BOTTOM - 1), None, "guard page");
        assert_eq!(s.user_page(&mut m, STACK_TOP), None, "the top page");
        s.destroy(&mut m);
    }

    #[test]
    fn the_arguments_are_at_the_top_with_the_stack_below_them() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"/bin/t-args", b"a", b"b c", b""]).unwrap();
        assert_eq!(args, b"/bin/t-args\0a\0b c\0\0");
        let e = load(&mut s, &mut m, &file, &p, &args, 4).unwrap();
        assert_eq!(e.ip, 0x40_1010);
        assert_eq!((e.args_len, e.argc), (19, 4));
        assert_eq!(e.args + e.args_len, STACK_TOP);
        assert_eq!(read(&mut m, &s, e.args, 19), args);
        assert_eq!(e.sp % 16, 0);
        assert!(e.sp <= e.args && e.args - e.sp < 16);
        s.destroy(&mut m);
    }

    #[test]
    fn arguments_up_to_64_kib() {
        let long = vec![b'x'; ARGS_MAX - 3];
        let args = arg_bytes(&[b"p", &long]).unwrap();
        assert_eq!(args.len(), ARGS_MAX);
        assert_eq!(arg_bytes(&[b"pp", &long]), Err(Errno::E2BIG));
        assert_eq!(arg_bytes(&[b"p", b"a\0b"]), Err(Errno::EINVAL));
        // The most still fits, and lands at the top.
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let e = load(&mut s, &mut m, &file, &p, &args, 2).unwrap();
        assert_eq!(e.args, STACK_TOP - ARGS_MAX as u64);
        assert_eq!(read(&mut m, &s, STACK_TOP - 2, 2), b"x\0");
        let too_many = vec![1; ARGS_MAX + 1];
        let mut s2 = space(&mut m);
        assert_eq!(
            load(&mut s2, &mut m, &file, &p, &too_many, 1),
            Err(Errno::E2BIG)
        );
        s.destroy(&mut m);
        s2.destroy(&mut m);
    }

    #[test]
    fn a_program_that_does_not_fit_its_file_is_enoexec() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]).unwrap();
        assert_eq!(
            load(&mut s, &mut m, &file[..0x3000], &p, &args, 1),
            Err(Errno::ENOEXEC)
        );
        s.destroy(&mut m);
    }

    #[test]
    fn running_out_of_memory_is_enomem_and_leaks_nothing() {
        let mut m = FakeMem::new();
        let (file, p) = program();
        let args = arg_bytes(&[b"p"]).unwrap();
        let mut k = PageTables::new(&mut m).unwrap();
        k.fill_upper_half(&mut m).unwrap();
        let before = m.frames();
        // Every limit from nothing up to what the whole program needs.
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        load(&mut s, &mut m, &file, &p, &args, 1).unwrap();
        let needed = m.frames() - before;
        s.destroy(&mut m);
        for limit in (1..needed).step_by(7) {
            m.limit = before + limit;
            let mut s = AddressSpace::new(&mut m, &k).unwrap();
            assert_eq!(
                load(&mut s, &mut m, &file, &p, &args, 1),
                Err(Errno::ENOMEM)
            );
            s.destroy(&mut m);
            assert_eq!(m.frames(), before, "limit {limit}");
        }
    }
}
````

- [ ] **Step 3: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod console;
pub mod input;
````

with:

````rust
pub mod console;
pub mod exec;
pub mod input;
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib exec`

Expected: FAIL: compile errors such as `` cannot find type `AddressSpace` in this scope ``; `` cannot find value `PAGE` in this scope ``.

- [ ] **Step 5: Implement `kernel/src/exec.rs`**

Insert this at the top of `kernel/src/exec.rs`, above `#[cfg(test)]`:

````rust
//! A program in its address space (user-space gate §5.1–§5.3): its
//! segments on pages with their permissions, a 1 MiB stack below the top
//! of the lower half with an unmapped guard page beneath it, and its
//! arguments at the top of the stack. Architecture-neutral; the registers
//! the entry state goes into are the `arch` module's business.

use crate::mm::paging::{MapError, PAGE, Perm, PhysMem};
use crate::mm::space::AddressSpace;
use alloc::vec::Vec;
use elf::{Access, Program};
use vfs::Errno;

/// The first address above the stack; the page from here up is never
/// mapped (spec §5.1).
pub const STACK_TOP: u64 = 0x7FFF_FFFF_F000;
/// The stack's pages: 1 MiB less one page.
pub const STACK_PAGES: u64 = 255;
/// The lowest address of the stack; the page below it is the guard page.
pub const STACK_BOTTOM: u64 = STACK_TOP - STACK_PAGES * PAGE;
/// The most argument bytes a program gets (spec §5.3).
pub const ARGS_MAX: usize = 64 * 1024;

/// Where a program starts: its first instruction, its stack pointer (16-byte
/// aligned, below the arguments), and its arguments' address, length and
/// count (spec §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub ip: u64,
    pub sp: u64,
    pub args: u64,
    pub args_len: u64,
    pub argc: u64,
}

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
    match access {
        Access::ReadExec => Perm::ReadExec,
        Access::Read => Perm::Read,
        Access::ReadWrite => Perm::ReadWrite,
    }
}

/// `ENOMEM` when the frames ran out; anything else cannot happen to a
/// program `elf::check` accepted.
fn errno(e: MapError) -> Errno {
    match e {
        MapError::OutOfMemory => Errno::ENOMEM,
        _ => Errno::ENOEXEC,
    }
}

/// Maps `program` (read from `file`, checked by `elf::check`) and its stack
/// into `space` with `args` (from `arg_bytes`, `argc` of them) at the top
/// of the stack. On an error the caller destroys `space`, which gives
/// back whatever was mapped.
pub fn load(
    space: &mut AddressSpace,
    mem: &mut impl PhysMem,
    file: &[u8],
    program: &Program,
    args: &[u8],
    argc: u64,
) -> Result<Entry, Errno> {
    for s in &program.segments {
        let first = s.vaddr & !(PAGE - 1);
        let pages = (s.end().next_multiple_of(PAGE) - first) / PAGE;
        space
            .map_zeroed(mem, first, pages, perm(s.access))
            .map_err(errno)?;
        let bytes = usize::try_from(s.offset)
            .ok()
            .zip(usize::try_from(s.filesz).ok())
            .and_then(|(at, len)| file.get(at..at.checked_add(len)?))
            .ok_or(Errno::ENOEXEC)?;
        space.fill(mem, s.vaddr, bytes).map_err(errno)?;
    }
    space
        .map_zeroed(mem, STACK_BOTTOM, STACK_PAGES, Perm::ReadWrite)
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
}

````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib exec`

Expected: PASS: 7 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add Cargo.lock kernel
git commit -m "kernel: a checked program is loaded with its stack and arguments"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 24 scenario(s) passed`.

````bash
git push -u origin m2p2/elf
gh pr create --base main --head m2p2/elf --title "Milestone 2, plan 2: Checking and loading programs" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 6–8: `crates/elf` checks a program's file against spec §5.2 (and ELF64, little-endian, at most 16 MiB), randomized so damage never panics it; xtask's build runs it next to `readelf`, which gains the same three rules, and the patched-`t-args` tests require both to agree; `exec::load` puts a checked program, its stack and its arguments into an address space.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p2/elf --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-elf
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Ring 3 (Tasks 9–16)

The shell's way to programs, the CPU state for ring 3, the system-call dispatcher, a scenario step that compares memory before and after, and then the first program running in ring 3; with three of the prototype review's findings, each fixed after the task it concerns.

Branch `m2p2/ring3`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-ring3`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p2/ring3 /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-ring3 origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-ring3
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p2/elf` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p2/ring3 /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-ring3 m2p2/elf`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p2/elf>` and re-run `cargo xtask ci` before pushing.

### Task 9: The shell runs a name it does not know as a program

Until the shell itself is a program (plan 4), the in-kernel shell reaches programs through two new methods of `shell::System` (decision 7): `spawn(vfs, path, args)` starts the program at `path`, read through the `Vfs`, and `wait(pid, out)` runs it until it ends, handing what it writes to fd 1 or 2 to `out`, and returns a `relay_abi::WaitStatus` (spec §7.3: exited with a code, or killed; `#[repr(C)]`, 32 bytes, its layout pinned with `offset_of!`). Both default to "no programs here", so `xtask host-shell` and the tests behave as before. They are two calls, not one, because the output may go through the same `Vfs` the program was read through: a redirection file or a script's transcript. A name that is no built-in runs `/bin/<name>`, a name with a `/` runs as given, and argument 0 is the path as typed. Not found is `relay-sh: <name>: command not found` (127), a missing path `No such file or directory` (127), any other refusal `relay-sh: <name>: <message>` (126, bash's "cannot execute"). The program's fd 1 is the command's standard output (the redirection file, if any) and fd 2 the screen, and a running script's transcript gets both, so `Ctx`'s output is split into `Streams`, which `wait` borrows apart from the `System`. A program that was killed is `relay-sh: <name>: killed` (137) until Task 17 gives the reasons. The test system runs fake programs: the file must exist and be a program it knows. Mutation checks: fd 2 into the redirection file, `ENOENT` in `/bin` reported as a missing file, the transcript not handed back, 126 for a missing path and a write error dropped all fail a test (`fd != 2` for `fd == 1` is equivalent: the kernel passes only fds 1 and 2).

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-abi/src/wait.rs`
- Modify: `crates/shell/Cargo.toml`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces: `relay_abi::{WaitStatus { how, code, fault, detail, address, ip }, WaitStatus::exited(u8), wait::{EXITED, KILLED}}`; `shell::System::{spawn(&mut self, &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Option<Result<u32, Errno>>, wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno>}` with defaults; `shell::shell::CANNOT_RUN`; the shell depends on `relay-abi`; the test harness's `FakeProgram` and `TestSystem::{programs, no_programs, spawned}`.

- [ ] **Step 1: Declare the new module in `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
mod result;

````

with:

````rust
mod result;
pub mod wait;

````

- [ ] **Step 2: Write the failing tests for `crates/relay-abi/src/wait.rs`**

Create `crates/relay-abi/src/wait.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_fixed() {
        assert_eq!(size_of::<WaitStatus>(), 32);
        assert_eq!(offset_of!(WaitStatus, how), 0);
        assert_eq!(offset_of!(WaitStatus, code), 4);
        assert_eq!(offset_of!(WaitStatus, fault), 8);
        assert_eq!(offset_of!(WaitStatus, detail), 12);
        assert_eq!(offset_of!(WaitStatus, address), 16);
        assert_eq!(offset_of!(WaitStatus, ip), 24);
    }

    #[test]
    fn an_exit_carries_only_its_code() {
        let w = WaitStatus::exited(3);
        assert_eq!((w.how, w.code), (EXITED, 3));
        assert_eq!(
            WaitStatus {
                how: EXITED,
                code: 3,
                ..Default::default()
            },
            w
        );
        assert_ne!(EXITED, KILLED);
    }
}
````

- [ ] **Step 3: Change `crates/shell/Cargo.toml`**

In `crates/shell/Cargo.toml`, replace:

````toml
vfs = { workspace = true }
````

with:

````toml
vfs = { workspace = true }
relay-abi = { workspace = true }
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use crate::Shell;
    use crate::testing::Harness;
    use alloc::string::String;
    use vfs::Errno;
````

with:

````rust
    use crate::Shell;
    use crate::testing::{FakeProgram, Harness};
    use alloc::string::String;
    use relay_abi::WaitStatus;
    use vfs::Errno;
````

Replace:

````rust
            (2, "relay-sh: syntax error: unterminated quote\n".into())
        );
````

with:

````rust
            (2, "relay-sh: syntax error: unterminated quote\n".into())
        );
    }

    /// `t-args` in `/bin`, printing its arguments on fd 1 and a line on
    /// fd 2, and exiting with 3.
    fn with_programs() -> Harness {
        let mut h = Harness::new();
        h.dir("/bin");
        h.put("/bin/t-args", b"\x7fELF");
        h.put("/root/text", b"not a program");
        h.system.programs.push(FakeProgram {
            path: "/bin/t-args",
            writes: vec![(1, b"[1] a\n"), (2, b"t-args: note\n"), (1, b"[2] b c\n")],
            status: WaitStatus::exited(3),
        });
        h
    }

    #[test]
    fn a_name_that_is_no_built_in_runs_from_bin() {
        let mut h = with_programs();
        assert_eq!(
            h.run("t-args a 'b c' ''"),
            (3, "[1] a\nt-args: note\n[2] b c\n".into())
        );
        assert_eq!(
            h.system.spawned,
            [vec![
                b"/bin/t-args".to_vec(),
                b"a".to_vec(),
                b"b c".to_vec(),
                b"".to_vec()
            ]]
        );
        // A path runs as given; argument 0 is the path as typed.
        assert_eq!(h.run("/bin/t-args").0, 3);
        h.run("cd /bin");
        assert_eq!(h.run("./t-args x").0, 3);
        assert_eq!(h.system.spawned[2], [b"./t-args".to_vec(), b"x".to_vec()]);
        // Built-ins come first.
        h.put("/bin/echo", b"\x7fELF");
        assert_eq!(h.run("echo hi"), (0, "hi\n".into()));
        assert_eq!(h.system.spawned.len(), 3);
    }

    #[test]
    fn a_program_s_output_follows_the_redirection_and_its_errors_the_screen() {
        let mut h = with_programs();
        assert_eq!(h.run("t-args > /tmp/out"), (3, "t-args: note\n".into()));
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        h.run("t-args >> /tmp/out");
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n[1] a\n[2] b c\n");
        // A full disk is a write error, as for a built-in.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                1,
                "t-args: note\nt-args: write error: No space left on device\n".into()
            )
        );
    }

    #[test]
    fn what_cannot_run_says_why() {
        let mut h = with_programs();
        assert_eq!(
            h.run("nosuch x"),
            (127, "relay-sh: nosuch: command not found\n".into())
        );
        assert_eq!(
            h.run("/root/nosuch"),
            (
                127,
                "relay-sh: /root/nosuch: No such file or directory\n".into()
            )
        );
        assert_eq!(
            h.run("/root/text"),
            (126, "relay-sh: /root/text: Exec format error\n".into())
        );
        assert_eq!(
            h.run("/root"),
            (126, "relay-sh: /root: Is a directory\n".into())
        );
        // In a script too; the script goes on.
        h.put("/root/s.sh", b"nosuch\nt-args\n");
        let (status, out) = h.run("sh /root/s.sh");
        assert_eq!(status, 3);
        assert!(
            out.contains("+ nosuch\nrelay-sh: nosuch: command not found\n+ t-args\n[1] a\n"),
            "{out}"
        );
        let transcript = String::from_utf8(h.get("/root/s.log")).unwrap();
        assert!(
            transcript.contains("+ t-args\n[1] a\nt-args: note\n[2] b c\n"),
            "{transcript}"
        );
    }

    #[test]
    fn without_programs_every_unknown_name_is_not_found() {
        let mut h = with_programs();
        h.system.no_programs = true;
        assert_eq!(
            h.run("t-args"),
            (127, "relay-sh: t-args: command not found\n".into())
        );
        assert_eq!(
            h.run("/bin/t-args"),
            (127, "relay-sh: /bin/t-args: command not found\n".into())
        );
    }

    #[test]
    fn a_killed_program_is_reported() {
        let mut h = with_programs();
        h.system.programs[0].status = WaitStatus {
            how: relay_abi::wait::KILLED,
            ..Default::default()
        };
        assert_eq!(
            h.run("t-args"),
            (
                137,
                "[1] a\nt-args: note\n[2] b c\nrelay-sh: t-args: killed\n".into()
            )
        );
````

- [ ] **Step 5: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use core::cell::Cell;
use vfs::{DirEntry, Env, Errno, FileSystem, Ino, MemFs, MountTable, Stat, StatFs, Vfs};

````

with:

````rust
use core::cell::Cell;
use relay_abi::WaitStatus;
use vfs::{DirEntry, Env, Errno, FileSystem, FileType, Ino, MemFs, MountTable, Stat, StatFs, Vfs};

````

Replace:

````rust

pub struct TestSystem {
````

with:

````rust

/// A program the test system runs: what it writes, on which fd, and how
/// it ends.
pub struct FakeProgram {
    pub path: &'static str,
    pub writes: Vec<(u32, &'static [u8])>,
    pub status: WaitStatus,
}

pub struct TestSystem {
````

Replace:

````rust
    pub poweroffs: u32,
}
````

with:

````rust
    pub poweroffs: u32,
    /// The programs `spawn` knows, by path; any other file is not one.
    pub programs: Vec<FakeProgram>,
    /// Programs cannot run at all (as on the host).
    pub no_programs: bool,
    /// The arguments of every program started.
    pub spawned: Vec<Vec<Vec<u8>>>,
    /// The program started and not yet waited for.
    child: Option<usize>,
}
````

Replace:

````rust
            poweroffs: 0,
        }
````

with:

````rust
            poweroffs: 0,
            programs: Vec::new(),
            no_programs: false,
            spawned: Vec::new(),
            child: None,
        }
````

Replace:

````rust
        self.poweroffs += 1;
    }
````

with:

````rust
        self.poweroffs += 1;
    }
    /// As the kernel does: the file must exist and be a program.
    fn spawn(
        &mut self,
        vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        if self.no_programs {
            return None;
        }
        let found = vfs.lookup(path).and_then(|node| {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            self.programs
                .iter()
                .position(|p| vfs.lookup(p.path.as_bytes()) == Ok(node))
                .ok_or(Errno::ENOEXEC)
        });
        Some(found.map(|i| {
            self.spawned.push(args.iter().map(|a| a.to_vec()).collect());
            self.child = Some(i);
            i as u32 + 1
        }))
    }
    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        let i = self
            .child
            .take()
            .filter(|&i| i as u32 + 1 == pid)
            .ok_or(Errno::ECHILD)?;
        let p = &self.programs[i];
        for (fd, bytes) in &p.writes {
            out(*fd, bytes);
        }
        Ok(p.status)
    }
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `KILLED` in module `relay_abi::wait` ``; `` unresolved import `relay_abi::WaitStatus` ``.

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` cannot find type `WaitStatus` in this scope ``; `` cannot find value `EXITED` in this scope ``.

- [ ] **Step 7: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! the ELF note that marks a program built for it, the call numbers, the
//! error numbers and how a call's result carries a value or an error.
//!
````

with:

````rust
//! the ELF note that marks a program built for it, the call numbers, the
//! error numbers, how a call's result carries a value or an error, and the
//! structs the calls pass.
//!
````

Replace:

````rust
pub use result::{MAX_ERRNO, decode, encode};

````

with:

````rust
pub use result::{MAX_ERRNO, decode, encode};
pub use wait::WaitStatus;

````

- [ ] **Step 8: Implement `crates/relay-abi/src/wait.rs`**

Insert this at the top of `crates/relay-abi/src/wait.rs`, above `#[cfg(test)]`:

````rust
//! How a child ended, as `wait` reports it (spec §7.3): it exited with a
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction.

/// `WaitStatus::how`: the child called `exit`.
pub const EXITED: u32 = 1;
/// `WaitStatus::how`: the child was killed; `code` says why.
pub const KILLED: u32 = 2;

/// `WaitStatus` (spec §7.3), `#[repr(C)]` with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaitStatus {
    /// [`EXITED`] or [`KILLED`].
    pub how: u32,
    /// The exit code, or why the child was killed.
    pub code: u32,
    /// For a fault: what the CPU refused.
    pub fault: u32,
    /// For a fault: more about it (the access of a page fault).
    pub detail: u32,
    /// For a fault: the address it concerned.
    pub address: u64,
    /// For a fault: the instruction it happened at.
    pub ip: u64,
}

impl WaitStatus {
    /// A child that exited with `code`.
    pub const fn exited(code: u8) -> WaitStatus {
        WaitStatus {
            how: EXITED,
            code: code as u32,
            fault: 0,
            detail: 0,
            address: 0,
            ip: 0,
        }
    }
}

````

- [ ] **Step 9: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use core::fmt;
use vfs::{Errno, Node, Vfs};
````

with:

````rust
use core::fmt;
use relay_abi::WaitStatus;
use vfs::{Errno, Node, Vfs};
````

Replace:

````rust

    /// Standard output.
    pub fn out(&mut self, bytes: &[u8]) {
        match &mut self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, .. } => {
                buf.extend_from_slice(bytes);
                if buf.len() >= FILE_BUFFER {
                    self.flush();
                }
            }
        }
    }

    /// Errors always go to the screen, never into a redirection file.
    pub fn err(&mut self, bytes: &[u8]) {
        self.screen(bytes);
    }

    /// Writes to the screen and a running script's transcript.
    fn screen(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.console.write(t.ended(e).as_bytes());
            self.transcript = None;
        }
    }
````

with:

````rust

    /// Where output goes, borrowed apart from the system.
    fn streams(&mut self) -> Streams<'_> {
        Streams {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            out: &mut self.out,
            transcript: &mut self.transcript,
        }
    }

    /// Standard output.
    pub fn out(&mut self, bytes: &[u8]) {
        self.streams().out(bytes);
    }

    /// Errors always go to the screen, never into a redirection file.
    pub fn err(&mut self, bytes: &[u8]) {
        self.streams().screen(bytes);
    }

    /// Waits for the program `pid` (`System::spawn`): what it writes to fd 1
    /// is standard output, to fd 2 the screen.
    pub(crate) fn wait_program(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let mut streams = Streams {
            vfs: &mut *self.vfs,
            console: &mut *self.console,
            out: &mut self.out,
            transcript: &mut self.transcript,
        };
        self.system.wait(pid, &mut |fd, bytes| {
            if fd == 1 {
                streams.out(bytes);
            } else {
                streams.screen(bytes);
            }
        })
    }
````

Replace:

````rust

    fn flush(&mut self) {
````

with:

````rust

    /// Writes what is left of the output; the first write error, if any.
    pub(crate) fn finish(&mut self) -> Result<(), Errno> {
        self.streams().flush();
        match self.out {
            Output::File { error: Some(e), .. } => Err(e),
            _ => Ok(()),
        }
    }

    /// Prints `name: message` on the screen and returns exit status 1.
    pub fn fail(&mut self, name: &str, message: fmt::Arguments<'_>) -> i32 {
        self.err(format!("{name}: {message}\n").as_bytes());
        1
    }
}

/// A command's standard output, the screen and a script's transcript.
struct Streams<'s> {
    vfs: &'s mut dyn Vfs,
    console: &'s mut dyn Console,
    out: &'s mut Output,
    transcript: &'s mut Option<Transcript>,
}

impl Streams<'_> {
    fn out(&mut self, bytes: &[u8]) {
        match &mut *self.out {
            Output::Console => self.screen(bytes),
            Output::File { buf, .. } => {
                buf.extend_from_slice(bytes);
                if buf.len() >= FILE_BUFFER {
                    self.flush();
                }
            }
        }
    }

    /// Writes to the screen and a running script's transcript.
    fn screen(&mut self, bytes: &[u8]) {
        self.console.write(bytes);
        if let Some(t) = &mut *self.transcript
            && let Err(e) = t.add(&mut *self.vfs, bytes)
        {
            self.console.write(t.ended(e).as_bytes());
            *self.transcript = None;
        }
    }

    fn flush(&mut self) {
````

Replace:

````rust
            error,
        } = &mut self.out
        else {
````

with:

````rust
            error,
        } = &mut *self.out
        else {
````

Replace:

````rust
        buf.clear();
    }

    /// Writes what is left of the output; the first write error, if any.
    pub(crate) fn finish(&mut self) -> Result<(), Errno> {
        self.flush();
        match self.out {
            Output::File { error: Some(e), .. } => Err(e),
            _ => Ok(()),
        }
    }

    /// Prints `name: message` on the screen and returns exit status 1.
    pub fn fail(&mut self, name: &str, message: fmt::Arguments<'_>) -> i32 {
        self.err(format!("{name}: {message}\n").as_bytes());
        1
    }
````

with:

````rust
        buf.clear();
    }
````

- [ ] **Step 10: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! The kernel implements both traits over its console, clock, memory
//! manager, log and ACPI; `xtask host-shell` over the host terminal; the
//! tests over buffers.

use alloc::vec::Vec;

````

with:

````rust
//! The kernel implements both traits over its console, clock, memory
//! manager, log, ACPI and programs; `xtask host-shell` over the host
//! terminal; the tests over buffers.

use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::{Errno, Vfs};

````

Replace:

````rust
    fn poweroff(&mut self);
}
````

with:

````rust
    fn poweroff(&mut self);
    /// Starts the program at `path`, read through `vfs`, with `args`
    /// (argument 0 is the path); its pid. `None` where programs cannot run
    /// (on the host). The in-kernel shell's way to programs until the shell
    /// itself becomes one (user-space gate plan 4).
    fn spawn(
        &mut self,
        _vfs: &mut dyn Vfs,
        _path: &[u8],
        _args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        None
    }
    /// Runs the program `spawn` started until it ends, giving what it
    /// writes to fds 1 and 2 to `out`, and says how it ended.
    fn wait(&mut self, _pid: u32, _out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        Err(Errno::ECHILD)
    }
}
````

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command and syncing the filesystems after it (spec §7.3, §8.3).

````

with:

````rust
//! The shell itself: prompt, line editing, parsing, redirection, running a
//! built-in command or a program and syncing the filesystems after it
//! (spec §7.3, §8.3; user-space gate §8.2).

````

Replace:

````rust
pub const NOT_FOUND: i32 = 127;
/// Exit status of a line that does not parse.
pub const SYNTAX: i32 = 2;
/// Exit status after Ctrl-C.
pub const CANCELLED: i32 = 130;
/// The most of `/etc/motd` shown at start.
````

with:

````rust
pub const NOT_FOUND: i32 = 127;
/// Exit status of a program that could not be started (bash's).
pub const CANNOT_RUN: i32 = 126;
/// Exit status of a line that does not parse.
pub const SYNTAX: i32 = 2;
/// Exit status after Ctrl-C.
pub const CANCELLED: i32 = 130;
/// Exit status of a program that was killed (bash's for SIGKILL).
pub const KILLED: i32 = 137;
/// The most of `/etc/motd` shown at start.
````

Replace:

````rust
        let Some(builtin) = commands::find(name) else {
            return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
        };
````

with:

````rust
        let Some(builtin) = commands::find(name) else {
            return self.run_program(name, &cmd.words[1..], file);
        };
````

Replace:

````rust
        }
        self.finish(status, message)
````

with:

````rust
        }
        self.finish(status, message)
    }

    /// Runs a program (user-space gate §8.2): `/bin/<name>`, or `name`
    /// itself when it holds a `/`, with the words after it as arguments.
    /// Its fd 1 is standard output (the redirection file, if any), its fd 2
    /// the screen; a running script's transcript gets both.
    fn run_program(&mut self, name: &str, words: &[String], file: Option<(Node, u64)>) -> i32 {
        let path = if name.contains('/') {
            String::from(name)
        } else {
            format!("/bin/{name}")
        };
        let mut args: Vec<&[u8]> = alloc::vec![path.as_bytes()];
        args.extend(words.iter().map(|w| w.as_bytes()));
        let started = self.system.spawn(&mut *self.vfs, path.as_bytes(), &args);
        let pid = match started {
            Some(Ok(pid)) => pid,
            // No programs here (the host), or none by that name in /bin.
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            Some(Err(Errno::ENOENT)) if !name.contains('/') => {
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
            Ok(_) => {
                message = format!("{NAME}: {name}: killed\n");
                KILLED
            }
            Err(e) => {
                message = format!("{NAME}: {name}: {e}\n");
                CANNOT_RUN
            }
        };
        if let Err(e) = ctx.finish() {
            message = format!("{name}: write error: {e}\n");
            status = 1;
        }
        self.transcript = ctx.transcript.take();
        self.finish(status, message)
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 122 tests.

Run: `cargo test -p relay-abi`

Expected: PASS: 7 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add Cargo.lock crates
git commit -m "shell: a name that is no built-in runs as a program, through System's spawn and wait"
````


### Task 10: `..`, `.` and an empty name are not commands

Review finding (minor): a name without a `/` runs `/bin/<name>`, and `/bin/..`, `/bin/.` and `/bin/` are directories, so Task 9's shell said `relay-sh: ..: Is a directory` (126) where milestone 1 said `relay-sh: ..: command not found` (127), as bash does: a search for a command skips directories. For a name without a `/`, `EISDIR` from `/bin/<name>` now counts as not found, as `ENOENT` does; a path that is a directory (`./`) still says `Is a directory` (decision 7). Mutation check: taking only `ENOENT` as not found fails the test.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 9.
- Produces: no new interface.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn without_programs_every_unknown_name_is_not_found() {
````

with:

````rust
    #[test]
    fn names_of_directories_in_bin_are_not_commands() {
        let mut h = with_programs();
        for name in ["..", ".", "''"] {
            let shown = if name == "''" { "" } else { name };
            assert_eq!(
                h.run(name),
                (127, format!("relay-sh: {shown}: command not found\n")),
                "{name}"
            );
        }
        // Given as a path, a directory still says so.
        assert_eq!(h.run("./"), (126, "relay-sh: ./: Is a directory\n".into()));
    }

    #[test]
    fn without_programs_every_unknown_name_is_not_found() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::names_of_directories_in_bin_are_not_commands`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            Some(Ok(pid)) => pid,
            // No programs here (the host), or none by that name in /bin.
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            Some(Err(Errno::ENOENT)) if !name.contains('/') => {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
````

with:

````rust
            Some(Ok(pid)) => pid,
            // No programs here (the host), or none by that name in /bin:
            // `..`, `.` and `''` name directories there, which a search
            // for a command skips, as bash's does.
            None => return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n")),
            Some(Err(Errno::ENOENT | Errno::EISDIR)) if !name.contains('/') => {
                return self.finish(NOT_FOUND, format!("{NAME}: {name}: command not found\n"));
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 123 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: .., . and an empty name are not commands, as in milestone 1"
````


### Task 11: User segments, TSS `rsp0`, and NMIs on the IST stack

Spec §6.2: the GDT gets ring 3's segments in the order `syscall` and `sysret` need: kernel code (`0x08`), kernel data (`0x10`), user data (`0x1B`), user code (`0x23`), then the TSS (`0x28`); `syscall` loads CS from STAR[47:32] and SS eight above it, `sysret` SS eight and CS sixteen above STAR[63:48], each with RPL 3, and a test pins that arithmetic to the table's order. The TSS is a `static mut` now, since `set_kernel_stack` writes its `rsp0` (the stack the CPU switches to for an interrupt or exception in ring 3) before every program; the descriptor is made from its address. NMIs and machine checks move to the double fault's IST stack (decision 9): between `syscall` and the switch to the kernel stack, and just before `sysret`, the stack pointer is the program's while the CPU is in ring 0, and an NMI there must still reach the panic screen. Nothing runs in ring 3 yet; the `boot` and `panic_*` scenarios show nothing else changed. Mutation checks: user code before user data, and an NMI off the IST, fail a test.

**Files:**
- Modify: `kernel/src/arch/gdt.rs`
- Modify: `kernel/src/arch/idt.rs`

**Interfaces:**
- Consumes: milestone 1's `arch::{gdt, idt}`.
- Produces: `arch::gdt::{KERNEL_CODE, KERNEL_DATA, USER_DATA, USER_CODE, set_kernel_stack(top: u64)}`; IST1 for vectors 2, 8 and 18.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/gdt.rs`**

In `kernel/src/arch/gdt.rs`, replace:

````rust
    }
}
````

with:

````rust
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use x86_64::PrivilegeLevel;

    #[test]
    fn the_segments_are_in_the_order_sysret_needs() {
        static FAKE_TSS: TaskStateSegment = TaskStateSegment::new();
        let (_, sel) = gdt(Descriptor::tss_segment(&FAKE_TSS));
        assert_eq!(sel.code.0, KERNEL_CODE);
        assert_eq!(sel.data.0, KERNEL_DATA);
        assert_eq!(sel.user_data.0, USER_DATA);
        assert_eq!(sel.user_code.0, USER_CODE);
        assert_eq!(sel.user_code.rpl(), PrivilegeLevel::Ring3);
        assert_eq!(sel.user_data.rpl(), PrivilegeLevel::Ring3);
        // syscall: CS from STAR[47:32], SS 8 above it; sysret: SS 8 and CS
        // 16 above STAR[63:48], with RPL 3.
        assert_eq!(KERNEL_DATA, KERNEL_CODE + 8);
        let user_base = KERNEL_DATA;
        assert_eq!(USER_DATA, (user_base + 8) | 3);
        assert_eq!(USER_CODE, (user_base + 16) | 3);
        assert_eq!(sel.tss.0, 0x28);
    }
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, replace:

````rust
        assert_eq!(g[8].ist, 1, "double fault on IST1");
        assert_eq!(g[48].ist, 0);
        let addr = |v: usize| {
````

with:

````rust
        assert_eq!(g[8].ist, 1, "double fault on IST1");
        assert_eq!(g[2].ist, 1, "NMI on IST1");
        assert_eq!(g[18].ist, 1, "machine check on IST1");
        let others = (0..256).filter(|v| ![2, 8, 18].contains(v));
        assert!(others.into_iter().all(|v| g[v].ist == 0));
        let addr = |v: usize| {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::`

Expected: FAIL: compile errors such as `` cannot find value `KERNEL_CODE` in this scope ``; `` cannot find value `KERNEL_DATA` in this scope ``.

- [ ] **Step 4: Change `kernel/src/arch/gdt.rs`**

Replace the whole of `kernel/src/arch/gdt.rs` with:

````rust
//! GDT with the kernel's and ring 3's code and data segments, in the order
//! `syscall` and `sysret` need (user-space gate §6.2), and a TSS: `rsp0` is
//! the stack the CPU switches to when an interrupt or exception arrives in
//! ring 3, and IST1 the stack for double faults, NMIs and machine checks
//! (so a kernel stack overflow, or an NMI taken with a program's stack
//! pointer, still reaches the panic screen).

use spin::Once;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

/// IST index (1-based in the gate, 0-based in the TSS table).
pub const DOUBLE_FAULT_IST: u8 = 1;
const IST_STACK_SIZE: usize = 16 * 1024;

/// The selectors, as `gdt()` lays the table out: kernel code and data,
/// then user data before user code, as `sysret` loads them (STAR's user
/// base + 8 for SS, + 16 for CS), each with its privilege level.
pub const KERNEL_CODE: u16 = 0x08;
pub const KERNEL_DATA: u16 = 0x10;
pub const USER_DATA: u16 = 0x18 | 3;
pub const USER_CODE: u16 = 0x20 | 3;

#[repr(align(16))]
#[allow(dead_code)] // only its address is used
struct Stack([u8; IST_STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: Stack = Stack([0; IST_STACK_SIZE]);

/// Written by `set_kernel_stack` while the CPU may read it, so it is not
/// behind a reference.
static mut TSS: TaskStateSegment = TaskStateSegment::new();

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    user_data: SegmentSelector,
    user_code: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::new();

/// The table, with the TSS at `tss`.
fn gdt(tss: Descriptor) -> (GlobalDescriptorTable, Selectors) {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let user_data = gdt.append(Descriptor::user_data_segment());
    let user_code = gdt.append(Descriptor::user_code_segment());
    let tss = gdt.append(tss);
    (
        gdt,
        Selectors {
            code,
            data,
            user_data,
            user_code,
            tss,
        },
    )
}

pub fn init() {
    let tss = &raw mut TSS;
    // SAFETY: runs once, before interrupts and before anything reads the
    // TSS; the stack is 'static.
    unsafe {
        let top = &raw const DOUBLE_FAULT_STACK as u64 + IST_STACK_SIZE as u64;
        (*tss).interrupt_stack_table[(DOUBLE_FAULT_IST - 1) as usize] = VirtAddr::new(top);
    }
    // SAFETY: the TSS is 'static and stays where it is.
    let (gdt, sel) = GDT.call_once(|| gdt(unsafe { Descriptor::tss_segment_unchecked(tss) }));
    debug_assert_eq!((sel.user_data.0, sel.user_code.0), (USER_DATA, USER_CODE));
    gdt.load();
    // SAFETY: the selectors index the GDT that was just loaded.
    unsafe {
        CS::set_reg(sel.code);
        SS::set_reg(sel.data);
        DS::set_reg(sel.data);
        ES::set_reg(sel.data);
        load_tss(sel.tss);
    }
}

/// The stack the CPU switches to when an interrupt or exception arrives
/// in ring 3 (TSS `rsp0`): the running program's kernel stack.
pub fn set_kernel_stack(top: u64) {
    // SAFETY: one CPU, and the CPU reads rsp0 only when it enters ring 0
    // from ring 3, which cannot happen while the kernel runs here.
    unsafe { TSS.privilege_stack_table[0] = VirtAddr::new(top) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use x86_64::PrivilegeLevel;

    #[test]
    fn the_segments_are_in_the_order_sysret_needs() {
        static FAKE_TSS: TaskStateSegment = TaskStateSegment::new();
        let (_, sel) = gdt(Descriptor::tss_segment(&FAKE_TSS));
        assert_eq!(sel.code.0, KERNEL_CODE);
        assert_eq!(sel.data.0, KERNEL_DATA);
        assert_eq!(sel.user_data.0, USER_DATA);
        assert_eq!(sel.user_code.0, USER_CODE);
        assert_eq!(sel.user_code.rpl(), PrivilegeLevel::Ring3);
        assert_eq!(sel.user_data.rpl(), PrivilegeLevel::Ring3);
        // syscall: CS from STAR[47:32], SS 8 above it; sysret: SS 8 and CS
        // 16 above STAR[63:48], with RPL 3.
        assert_eq!(KERNEL_DATA, KERNEL_CODE + 8);
        let user_base = KERNEL_DATA;
        assert_eq!(USER_DATA, (user_base + 8) | 3);
        assert_eq!(USER_CODE, (user_base + 16) | 3);
        assert_eq!(sel.tss.0, 0x28);
    }
}
````

- [ ] **Step 5: Change `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, replace:

````rust

/// Gates for the 32 exceptions (double fault on its IST stack) and for
/// every hardware interrupt vector, 32-255.
fn gates(selector: u16) -> [Gate; 256] {
    let mut gates = [Gate::MISSING; 256];
    for (v, stub) in STUBS.iter().enumerate() {
        let ist = if v == 8 {
            super::gdt::DOUBLE_FAULT_IST
````

with:

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
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::`

Expected: PASS: 16 tests.

- [ ] **Step 7: Run the `boot`, `panic_stack`, `panic_pagefault` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario panic_stack`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario panic_pagefault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel
git commit -m "arch: user segments in the order sysret needs, TSS rsp0, and NMIs and machine checks on the IST stack"
````


### Task 12: The system-call dispatcher

The architecture-neutral half of a system call (spec §3.1, §7): `syscall::dispatch(caller, number, args)` gets the call number and six arguments from the `arch` entry stub and answers with the result register (`relay_abi::encode`) or the program's exit. Plan 2 serves `exit` (the code is a byte) and `write` to fds 1 and 2 (the console, or where the shell sends them); any other fd is `EBADF`, and every other call number, known or not, is `ENOSYS` (decision 6). `write` copies the buffer out in 4 KiB pieces through `UserSlice`, so a bad pointer is `EFAULT` and a buffer that runs into a page the program does not have is a short write that keeps the pieces before it (Task 13 makes that every byte). The dispatcher reaches the program through the `Caller` trait (read its memory, deliver its output), so the tests run it against an address space in the fake `PhysMem`; the kernel's `Caller` (Task 15) takes the memory lock only while it copies a piece, because the output may reach a disk whose driver allocates memory too. Mutation checks: a short write reported as `EFAULT` or the reverse, fd 3 accepted, no pieces, and an unknown call answered all fail a test.

**Files:**
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/syscall.rs`

**Interfaces:**
- Consumes: Tasks 1, 3 and 5.
- Produces: `syscall::{Caller { read(&mut self, &UserSlice, offset, &mut [u8]) -> Result<(), Errno>; output(&mut self, fd: u32, &[u8]) }, Outcome::{Return(u64), Exit(u8)}, dispatch(&mut impl Caller, number: u64, args: [u64; 6]) -> Outcome}`.

- [ ] **Step 1: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod storage;
pub mod system;
````

with:

````rust
pub mod storage;
pub mod syscall;
pub mod system;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/syscall.rs`**

Create `kernel/src/syscall.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::paging::{PAGE, PageTables, Perm};
    use crate::mm::space::AddressSpace;
    use crate::mm::testing::FakeMem;
    use relay_abi::{decode, errno};

    const U: u64 = 0x40_0000;

    /// A program with three readable pages at `U` holding a pattern, and
    /// nothing after them; what it wrote.
    struct Fake {
        mem: FakeMem,
        space: AddressSpace,
        written: Vec<(u32, Vec<u8>)>,
    }

    impl Caller for Fake {
        fn read(&mut self, slice: &UserSlice, offset: u64, buf: &mut [u8]) -> Result<(), Errno> {
            slice.read(&self.space, &mut self.mem, offset, buf)
        }
        fn output(&mut self, fd: u32, bytes: &[u8]) {
            self.written.push((fd, bytes.to_vec()));
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
        Fake {
            mem,
            space,
            written: Vec::new(),
        }
    }

    fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
        match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
            Outcome::Return(r) => decode(r),
            Outcome::Exit(code) => panic!("exited with {code}"),
        }
    }

    /// Everything written, joined, per fd.
    fn text(f: &Fake, fd: u32) -> Vec<u8> {
        f.written
            .iter()
            .filter(|(d, _)| *d == fd)
            .flat_map(|(_, b)| b.clone())
            .collect()
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
    fn write_copies_the_buffer_to_fd_1_or_2() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Write, [1, U + 10, 5]), Ok(5));
        assert_eq!(call(&mut f, Call::Write, [2, U, 3]), Ok(3));
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
    fn other_fds_are_ebadf() {
        let mut f = fake();
        for fd in [0, 3, 31, 1 << 32 | 1, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Write, [fd, U, 1]),
                Err(errno::EBADF),
                "{fd}"
            );
        }
        assert!(f.written.is_empty());
    }

    #[test]
    fn a_bad_buffer_is_efault_or_a_short_write() {
        let mut f = fake();
        let end = U + 3 * PAGE;
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
        // The pieces before the hole are written.
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 5000, 9000]),
            Ok(4096),
            "the first piece only"
        );
        assert_eq!(text(&f, 1).len(), 4096);
    }

    #[test]
    fn every_other_call_is_enosys() {
        let mut f = fake();
        for c in Call::ALL {
            if c != Call::Exit && c != Call::Write {
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
        assert!(f.written.is_empty());
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` cannot find trait `Caller` in this scope ``; `` cannot find type `UserSlice` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/syscall.rs`**

Insert this at the top of `kernel/src/syscall.rs`, above `#[cfg(test)]`:

````rust
//! The system-call dispatcher (user-space gate §7), architecture-neutral:
//! the `arch` entry stub hands it the call number and the six arguments,
//! and it answers with the result register's value or with the program's
//! exit. Plan 2 of milestone 2 serves `exit` and `write` to fds 1 and 2;
//! every other call is `ENOSYS` until the plan that brings it.

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

/// How a call ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Back to the program with this result register.
    Return(u64),
    /// The program called `exit` with this code.
    Exit(u8),
}

/// Bytes copied out of a program per step of a `write`.
const CHUNK: usize = 4096;

/// Serves call `number` with `args` for `caller`.
pub fn dispatch(caller: &mut impl Caller, number: u64, args: [u64; 6]) -> Outcome {
    let result = match Call::from_number(number) {
        Some(Call::Exit) => return Outcome::Exit(args[0] as u8),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        _ => Err(Errno::ENOSYS),
    };
    Outcome::Return(encode(result.map_err(Errno::number)))
}

/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out in pieces; a piece that is not the program's ends the call
/// with the bytes written before it, or with `EFAULT` if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let fd = match fd {
        1 | 2 => fd as u32,
        _ => return Err(Errno::EBADF),
    };
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; CHUNK];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(CHUNK as u64) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        caller.output(fd, &buf[..n]);
        done += n as u64;
    }
    Ok(done)
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: PASS: 5 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the system-call dispatcher serves exit and write"
````


### Task 13: `write` keeps every byte before a bad page

Review finding (minor): Task 12's `write` copied the buffer in 4 KiB pieces counted from its start, and threw away a piece that reached a page the program does not have, so `write(1, end - 904, 2000)` was `EFAULT` with 904 good bytes before the hole, and `write(1, end - 5000, 9000)` wrote 4096 bytes, not 5000; Linux writes 904 and 5000. Each piece now ends on a page boundary, so a bad page costs only its own bytes (decision 4); the test that pinned 4096 now expects 5000, and one more expects 904. Mutation check: pieces that do not end at a page boundary fail the test.

**Files:**
- Modify: `kernel/src/syscall.rs`

**Interfaces:**
- Consumes: Task 12.
- Produces: no new interface.

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
        assert!(f.written.is_empty());
        // The pieces before the hole are written.
        assert_eq!(
            call(&mut f, Call::Write, [1, end - 5000, 9000]),
            Ok(4096),
            "the first piece only"
        );
        assert_eq!(text(&f, 1).len(), 4096);
    }
````

with:

````rust
        assert!(f.written.is_empty());
        // Every byte before the hole is written, as on Linux.
        assert_eq!(call(&mut f, Call::Write, [1, end - 5000, 9000]), Ok(5000));
        let want: Vec<u8> = (3 * PAGE - 5000..3 * PAGE)
            .map(|i| (i % 251) as u8)
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
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: 1 test fails: `syscall::tests::a_bad_buffer_is_efault_or_a_short_write`.

- [ ] **Step 3: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use crate::mm::user::UserSlice;
````

with:

````rust

use crate::mm::paging::PAGE;
use crate::mm::user::UserSlice;
````

Replace:

````rust

/// Bytes copied out of a program per step of a `write`.
const CHUNK: usize = 4096;

/// Serves call `number` with `args` for `caller`.
````

with:

````rust

/// Serves call `number` with `args` for `caller`.
````

Replace:

````rust
/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out in pieces; a piece that is not the program's ends the call
/// with the bytes written before it, or with `EFAULT` if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
````

with:

````rust
/// `write(fd, buffer, length)`: fds 1 and 2 are the console. Copies the
/// buffer out a page at a time; a page that is not the program's ends the
/// call with the bytes written before it, or with `EFAULT` if there were
/// none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
````

Replace:

````rust
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; CHUNK];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(CHUNK as u64) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
````

with:

````rust
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        // To the end of the page, so a bad page costs only its own bytes.
        // `UserSlice::new` checked that `addr + len` does not overflow.
        let n = (len - done).min(PAGE - (addr + done) % PAGE) as usize;
        if let Err(e) = caller.read(&slice, done, &mut buf[..n]) {
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: PASS: 5 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: write keeps every byte before a page the program does not have"
````


### Task 14: The e2e step `expect-same`

A scenario must show that running programs loses no memory (spec §12.2): `free` before and after must say the same memory is in use. `expect-same NAME REGEX` waits for the regex as `expect` does, and its first group must capture what it captured the first time a step of that name matched (decision 12); the values survive a `reboot` step. `expect` and `expect-same` share the waiting code, so the refactor is checked by the `shell` scenario. Mutation checks: no comparison, and a regex without a group accepted, fail a test.

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: milestone 1's e2e runner.
- Produces: `e2e::Step::ExpectSame { name, pattern }`; scenarios may use `expect-same NAME REGEX`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_the_unplug_step() {
````

with:

````rust
    #[test]
    fn parses_the_expect_same_step() {
        let s = parse_scenario("x", r"expect-same mem Mem:\s+\d+\s+(\d+)").unwrap();
        assert_eq!(
            s.steps[0],
            (
                1,
                Step::ExpectSame {
                    name: "mem".into(),
                    pattern: r"Mem:\s+\d+\s+(\d+)".into()
                }
            )
        );
        assert!(parse_scenario("x", "expect-same mem").is_err(), "no regex");
        assert!(
            parse_scenario("x", r"expect-same mem Mem:\s+\d+").is_err(),
            "no group"
        );
        assert!(
            parse_scenario("x", "expect-same mem (").is_err(),
            "bad regex"
        );
    }

    #[test]
    fn expect_same_compares_with_the_first_value() {
        let mut seen = HashMap::new();
        same_as_before(&mut seen, "mem", "1024").unwrap();
        same_as_before(&mut seen, "mem", "1024").unwrap();
        same_as_before(&mut seen, "heap", "7").unwrap();
        let e = same_as_before(&mut seen, "mem", "1028").unwrap_err();
        assert_eq!(e.to_string(), "mem is 1028, but it was 1024 the first time");
        same_as_before(&mut seen, "mem", "1024").unwrap();
    }

    #[test]
    fn parses_the_unplug_step() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask e2e::`

Expected: FAIL: compile errors such as `` cannot find type `HashMap` in this scope ``; `` cannot find function `same_as_before` in this scope ``.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! expect <regex>                   (waits for serial output, ANSI stripped)
//! send <text>                      (types <text> + Enter over serial)
````

with:

````rust
//! expect <regex>                   (waits for serial output, ANSI stripped)
//! expect-same <name> <regex>       (as expect; the regex's first group must
//!                                   capture what it did the first time a
//!                                   step of that name matched)
//! send <text>                      (types <text> + Enter over serial)
````

Replace:

````rust
use regex::Regex;
use std::fs;
````

with:

````rust
use regex::Regex;
use std::collections::HashMap;
use std::fs;
````

Replace:

````rust
    Expect(String),
    Send(String),
````

with:

````rust
    Expect(String),
    /// As `Expect`; the first group must capture what it captured the first
    /// time a step with this name matched (free memory before and after).
    ExpectSame {
        name: String,
        pattern: String,
    },
    Send(String),
````

Replace:

````rust
                Step::Expect(rest.to_string())
            }
````

with:

````rust
                Step::Expect(rest.to_string())
            }
            "expect-same" => {
                parse_expect_same(rest).with_context(|| format!("{name}:{line_no}"))?
            }
````

Replace:

````rust

fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    let offline = matches!(
````

with:

````rust

/// `expect-same NAME REGEX`: a name without spaces, then a regex with at
/// least one group.
fn parse_expect_same(rest: &str) -> Result<Step> {
    let (name, pattern) = rest
        .split_once(' ')
        .context("expect-same needs a name and a regex")?;
    let re = Regex::new(pattern).context("bad regex")?;
    if re.captures_len() < 2 {
        bail!("expect-same's regex needs a group to compare");
    }
    Ok(Step::ExpectSame {
        name: name.to_string(),
        pattern: pattern.to_string(),
    })
}

/// Records `value` as what `name` captured, or checks it against what it
/// captured before.
fn same_as_before(seen: &mut HashMap<String, String>, name: &str, value: &str) -> Result<()> {
    match seen.get(name) {
        Some(first) if first != value => {
            bail!("{name} is {value}, but it was {first} the first time")
        }
        Some(_) => Ok(()),
        None => {
            seen.insert(name.to_string(), value.to_string());
            Ok(())
        }
    }
}

/// Waits until `re` matches the serial output after what earlier steps
/// consumed; its captures.
fn wait_for_match(r: &mut Running, re: &Regex, timeout: Duration) -> Result<Vec<String>> {
    let deadline = Instant::now() + timeout;
    loop {
        let text = r.text();
        let rest = &text[r.consumed.min(text.len())..];
        if let Some(c) = re.captures(rest) {
            let groups = c
                .iter()
                .map(|g| g.map_or(String::new(), |g| g.as_str().to_string()))
                .collect();
            r.consumed += c.get(0).unwrap().end();
            return Ok(groups);
        }
        if let Ok(Some(status)) = r.child.try_wait() {
            bail!("QEMU exited ({status}) while waiting for /{re}/");
        }
        if Instant::now() > deadline {
            bail!("timed out after {timeout:?} waiting for /{re}/");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_step(
    r: &mut Running,
    step: &Step,
    timeout: &mut Duration,
    run_dir: &Path,
    seen: &mut HashMap<String, String>,
) -> Result<()> {
    let offline = matches!(
````

Replace:

````rust
        Step::Expect(pattern) => {
            let re = Regex::new(pattern)?;
            let deadline = Instant::now() + *timeout;
            loop {
                let text = r.text();
                if let Some(m) = re.find(&text[r.consumed.min(text.len())..]) {
                    r.consumed += m.end();
                    return Ok(());
                }
                if let Ok(Some(status)) = r.child.try_wait() {
                    bail!("QEMU exited ({status}) while waiting for /{pattern}/");
                }
                if Instant::now() > deadline {
                    bail!("timed out after {timeout:?} waiting for /{pattern}/");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
````

with:

````rust
        Step::Expect(pattern) => {
            wait_for_match(r, &Regex::new(pattern)?, *timeout)?;
        }
        Step::ExpectSame { name, pattern } => {
            let groups = wait_for_match(r, &Regex::new(pattern)?, *timeout)?;
            same_as_before(seen, name, &groups[1])?;
        }
````

Replace:

````rust
    let mut timeout = Duration::from_secs(20);
    for (line, step) in &scenario.steps {
        if let Err(e) = run_step(&mut r, step, &mut timeout, &run_dir) {
            bail!(
````

with:

````rust
    let mut timeout = Duration::from_secs(20);
    let mut seen = HashMap::new();
    for (line, step) in &scenario.steps {
        if let Err(e) = run_step(&mut r, step, &mut timeout, &run_dir, &mut seen) {
            bail!(
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask e2e::`

Expected: PASS: 27 tests.

- [ ] **Step 5: Run the `shell` scenario**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add xtask
git commit -m "xtask: the e2e step expect-same compares a value with its first reading"
````


### Task 15: Programs run in ring 3

The x86_64 half (spec §5.3, §6.2), in `arch/user.rs`: `init` turns on `syscall` (EFER.SCE), sets STAR from the GDT's selectors, LSTAR to `syscall_entry` and SFMASK to clear IF, DF, TF and AC. `syscall_entry` reaches the per-CPU block with `swapgs`, keeps the program's stack pointer there, switches to the program's kernel stack, saves every register as a `SyscallFrame`, turns interrupts on and calls the dispatcher; it returns with `sysret`, but only to a canonical address: a return address that is not canonical kills the program as a general-protection fault (decision 3). `run` sets TSS `rsp0`, the per-CPU kernel stack and both `gs` bases (the kernel uses `gs` only in the stub, and `exit` leaves in the middle of a call, before the stub's second `swapgs`), loads the program's CR3 and calls `enter`, which saves the kernel's callee-saved registers and stack pointer and goes to ring 3 with `iretq` (interrupts on, `rdi`/`rsi`/`rdx` the arguments, every other register zero); `leave` goes back to that saved context from the program's kernel stack, which is then abandoned, and `run` restores the kernel's CR3. The architecture-neutral half, `proc.rs` (decision 6): `spawn` reads the file through the `Vfs` (a directory is `EISDIR`, over 16 MiB `ENOEXEC` before anything is read), checks it (a refusal is `ENOEXEC`, its reason in the kernel log), builds the argument bytes, a kernel stack and the address space, and keeps the child (`EAGAIN` while one exists); pids count from 1. `wait` runs the child to completion and gives back its memory and kernel stack; `system_call` dispatches for the running child and leaves on `exit`. `KernelSystem` implements the shell's `spawn` and `wait` with them. The `programs` scenario runs `t-args` with three arguments, by path and relative path, redirected, and without arguments, then a text file (`Exec format error`), a missing path and a missing name, and compares `free` after the first program (whose kernel stack makes the stack area's tables, which stay) with `free` at the end. The frame layout, the entry offsets, the canonical check and SFMASK are host-tested; the rest only runs in QEMU.

**Files:**
- Modify: `kernel/src/arch/mod.rs`
- Create: `kernel/src/arch/user.rs`
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Create: `tests/e2e/programs.txt`

**Interfaces:**
- Consumes: Tasks 3, 4, 6, 8, 9, 11, 12 and 14; plan 1's `relay-rt` (`_start`) and `t-args`.
- Produces: `arch::user::{SFMASK, SyscallFrame, UserEntry { ip, sp, rdi, rsi, rdx }, is_canonical(u64) -> bool, init(), run(&UserEntry, kernel_stack: u64, pml4: u64, kernel_pml4: u64, waiter: &mut u64), leave(*const u64) -> !}`; `arch::ELF_MACHINE`; `proc::{spawn(&mut dyn Vfs, &[u8], &[&[u8]]) -> Result<u32, Errno>, wait(u32, &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno>, system_call(u64, [u64; 6]) -> u64, non_canonical_return(u64) -> !}`; the `programs` scenario.

- [ ] **Step 1: Declare the new module in `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust
pub mod pic;

````

with:

````rust
pub mod pic;
pub mod user;

````

- [ ] **Step 2: Write the failing tests for `kernel/src/arch/user.rs`**

Create `kernel/src/arch/user.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_frame_is_what_the_stub_pushes() {
        // 16 pushes: the program's rsp first (highest), r15 last.
        assert_eq!(size_of::<SyscallFrame>(), 16 * 8);
        assert_eq!(offset_of!(SyscallFrame, r15), 0);
        assert_eq!(offset_of!(SyscallFrame, rbx), 5 * 8);
        assert_eq!(offset_of!(SyscallFrame, r9), 6 * 8);
        assert_eq!(offset_of!(SyscallFrame, r10), 8 * 8);
        assert_eq!(offset_of!(SyscallFrame, rdi), 11 * 8);
        assert_eq!(offset_of!(SyscallFrame, rax), 12 * 8);
        assert_eq!(offset_of!(SyscallFrame, rip), 13 * 8);
        assert_eq!(offset_of!(SyscallFrame, rflags), 14 * 8);
        assert_eq!(offset_of!(SyscallFrame, rsp), 15 * 8);
        // 16 pushes from a 16-byte aligned top keep the call aligned.
        assert_eq!(size_of::<SyscallFrame>() % 16, 0);
    }

    #[test]
    fn enter_reads_the_entry_at_these_offsets() {
        assert_eq!(offset_of!(UserEntry, ip), 0);
        assert_eq!(offset_of!(UserEntry, sp), 8);
        assert_eq!(offset_of!(UserEntry, rdi), 16);
        assert_eq!(offset_of!(UserEntry, rsi), 24);
        assert_eq!(offset_of!(UserEntry, rdx), 32);
        assert_eq!(offset_of!(PerCpu, kernel_rsp), 0);
        assert_eq!(offset_of!(PerCpu, user_rsp), 8);
    }

    #[test]
    fn only_canonical_addresses_are_returned_to() {
        for ok in [
            0,
            0x40_1000,
            0x7FFF_FFFF_FFFF,
            0xFFFF_8000_0000_0000,
            u64::MAX,
        ] {
            assert!(is_canonical(ok), "{ok:#x}");
        }
        for bad in [
            0x8000_0000_0000,
            0x7FFF_FFFF_FFFF + 1,
            0xFFFF_7FFF_FFFF_FFFF,
            1 << 63,
        ] {
            assert!(!is_canonical(bad), "{bad:#x}");
        }
    }

    #[test]
    fn syscall_clears_the_flags_the_kernel_needs_clear() {
        assert_eq!(SFMASK.bits(), (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18));
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
    }
}
````

- [ ] **Step 3: Add the scenario `tests/e2e/programs.txt`**

Create `tests/e2e/programs.txt`:

````text
# Programs run in ring 3 (user-space gate §5, §6.2; milestone 2, plan 2):
# the in-kernel shell runs a name it does not know from /bin, or a path as
# given, and waits for it. What the program writes on fd 1 follows the
# redirection; memory in use is the same before and after (once the first
# program has made the kernel-stack area's page tables, which stay).
timeout 30
expect root@relay:~# $
send t-args a 'b c' ''
expect \n\[1\] a\n\[2\] b c\n\[3\] \nroot@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send /bin/t-args by-path
expect \n\[1\] by-path\nroot@relay:~# $
send cd /bin
send ./t-args relative
expect \n\[1\] relative\nroot@relay:/bin# $
send cd
send t-args one two > /root/args.txt
send cat /root/args.txt
expect \n\[1\] one\n\[2\] two\n
send t-args
expect t-args\nroot@relay:~# $
send echo not a program > /root/text
send /root/text
expect \nrelay-sh: /root/text: Exec format error\n
send /root/missing
expect \nrelay-sh: /root/missing: No such file or directory\n
send t-missing
expect \nrelay-sh: t-missing: command not found\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::user`

Expected: FAIL: compile errors such as `` cannot find type `SyscallFrame` in this scope ``; `` cannot find type `UserEntry` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: FAIL: scenario `programs` stops at line 9, timed out waiting for `\n\[1\] a\n\[2\] b c\n\[3\] \nroot@relay:~# $`.

- [ ] **Step 5: Change `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! x86_64-specific setup: segmentation, interrupt table, CPU control.

````

with:

````rust
//! x86_64-specific setup: segmentation, interrupt table, CPU control, and
//! ring 3.

````

Replace:

````rust
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}
````

with:

````rust
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

/// `e_machine` of the programs this kernel runs.
pub const ELF_MACHINE: u16 = elf::EM_X86_64;
````

- [ ] **Step 6: Implement `kernel/src/arch/user.rs`**

Insert this at the top of `kernel/src/arch/user.rs`, above `#[cfg(test)]`:

````rust
//! Ring 3 on x86_64 (user-space gate §5.3, §6.2): the `syscall` entry, going
//! into a program and coming back out of it.
//!
//! - **`syscall`** jumps to `syscall_entry` with interrupts, direction,
//!   trap and alignment-check flags cleared (`SFMASK`). The stub reaches
//!   the per-CPU block with `swapgs`, keeps the program's stack pointer
//!   there, switches to the program's kernel stack, saves every register
//!   as a `SyscallFrame`, turns interrupts back on and calls the
//!   architecture-neutral dispatcher (`crate::proc::system_call`). It
//!   returns with `sysret`, but only to a canonical address: on Intel CPUs
//!   `sysret` to a non-canonical `rcx` faults in ring 0 with the program's
//!   stack pointer, so such a program is killed instead.
//! - **`enter`** saves the kernel's callee-saved registers and stack pointer
//!   (the waiting kernel code), then goes to ring 3 with `iretq`; **`leave`**
//!   goes back to that kernel code from the program's kernel stack, which
//!   is abandoned. `exit` and (with plan 2's faults) a program's fault end
//!   that way.
//!
//! The kernel uses `gs` only in `syscall_entry`. Because `exit` leaves
//! from inside a call, before the stub's second `swapgs`, `run` sets both
//! `gs` bases afresh before every program.

use super::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA};
use core::arch::naked_asm;
use x86_64::instructions::interrupts;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::registers::model_specific::{
    Efer, EferFlags, GsBase, KernelGsBase, LStar, SFMask, Star,
};
use x86_64::registers::rflags::RFlags;
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::paging::PhysFrame;
use x86_64::{PhysAddr, VirtAddr};

/// The flags `syscall` clears (spec §6.2): no interrupts until the stub is
/// on the kernel stack, and the direction, trap and alignment-check flags
/// in their kernel state.
pub const SFMASK: RFlags = RFlags::INTERRUPT_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::TRAP_FLAG)
    .union(RFlags::ALIGNMENT_CHECK);

/// A program starts with only the interrupt flag (and bit 1, always set).
const USER_RFLAGS: u64 = 0x202;

/// Reached through `gs` in `syscall_entry`: the kernel stack for the next
/// system call, and the program's stack pointer during one.
#[repr(C)]
struct PerCpu {
    kernel_rsp: u64,
    user_rsp: u64,
}

static mut PER_CPU: PerCpu = PerCpu {
    kernel_rsp: 0,
    user_rsp: 0,
};

/// The registers of a program in a system call, as `syscall_entry` pushes
/// them, lowest address first.
#[repr(C)]
#[derive(Debug)]
pub struct SyscallFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
    /// `rcx`: where `syscall` came from.
    pub rip: u64,
    /// `r11`: the flags `syscall` saw.
    pub rflags: u64,
    pub rsp: u64,
}

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
pub fn is_canonical(addr: u64) -> bool {
    let top = addr >> 47;
    top == 0 || top == 0x1_FFFF
}

/// Turns on `syscall` and points it at `syscall_entry`.
pub fn init() {
    // SAFETY: the GDT has the segments STAR names; the entry stub is
    // ready; the per-CPU block is 'static.
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };
    Star::write(
        SegmentSelector(USER_CODE),
        SegmentSelector(USER_DATA),
        SegmentSelector(KERNEL_CODE),
        SegmentSelector(KERNEL_DATA),
    )
    .expect("the GDT's segments are in the order sysret needs");
    LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
    SFMask::write(SFMASK);
}

/// Runs a program: `entry` in the address space whose PML4 is at `pml4`,
/// with `kernel_stack` (its top) for its system calls, interrupts and
/// faults. Returns once something calls `leave(waiter)`, with the kernel's
/// own tables (`kernel_pml4`) back in CR3 and interrupts as they were.
pub fn run(entry: &UserEntry, kernel_stack: u64, pml4: u64, kernel_pml4: u64, waiter: &mut u64) {
    let enabled = interrupts::are_enabled();
    interrupts::disable();
    super::gdt::set_kernel_stack(kernel_stack);
    // SAFETY: interrupts are off and no system call is running, so nothing
    // reads the per-CPU block now. The page tables map the kernel as the
    // current ones do (the upper half is shared).
    unsafe {
        PER_CPU.kernel_rsp = kernel_stack;
        GsBase::write(VirtAddr::new(0));
        KernelGsBase::write(VirtAddr::new(&raw const PER_CPU as u64));
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
    if enabled {
        interrupts::enable();
    }
}

/// Saves the callee-saved registers and the stack pointer in `*waiter`,
/// then goes to ring 3 at `entry`: its stack, `rdi`, `rsi`, `rdx`,
/// interrupts on, every other register zero. Returns when `leave(waiter)`
/// runs.
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
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
        ss = const USER_DATA as u64,
        cs = const USER_CODE as u64,
        rflags = const USER_RFLAGS,
    )
}

/// Goes back to the kernel code waiting in `enter(waiter, _)`, on its
/// stack, with interrupts off. The stack this runs on is abandoned.
///
/// # Safety
/// `waiter` must be what `enter` saved, and that `enter` must not have
/// returned yet.
#[unsafe(naked)]
pub unsafe extern "C" fn leave(waiter: *const u64) -> ! {
    naked_asm!(
        "cli",
        "mov rsp, [rdi]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
    )
}

/// The `syscall` instruction's target. See the module comment.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs",
        "mov gs:[8], rsp",
        "mov rsp, gs:[0]",
        "push qword ptr gs:[8]",
        "push r11", "push rcx",
        "push rax", "push rdi", "push rsi", "push rdx", "push r10", "push r8", "push r9",
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",
        "sti",
        "call {dispatch}",
        "cli",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "pop r9", "pop r8", "pop r10", "pop rdx", "pop rsi", "pop rdi", "pop rax",
        "pop rcx", "pop r11",
        "pop rsp",
        "swapgs",
        "sysretq",
        dispatch = sym syscall_dispatch,
    )
}

extern "C" fn syscall_dispatch(frame: &mut SyscallFrame) {
    let args = [
        frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9,
    ];
    frame.rax = crate::proc::system_call(frame.rax, args);
    if !is_canonical(frame.rip) {
        crate::proc::non_canonical_return(frame.rip);
    }
}

````

- [ ] **Step 7: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub mod power;
pub mod rtc;
````

with:

````rust
pub mod power;
pub mod proc;
pub mod rtc;
````

Replace:

````rust
    arch::idt::init();
    let cmdline = Cmdline::parse(info.cmdline());
````

with:

````rust
    arch::idt::init();
    arch::user::init();
    let cmdline = Cmdline::parse(info.cmdline());
````

- [ ] **Step 8: Create `kernel/src/proc.rs`**

Create `kernel/src/proc.rs`:

````rust
//! The programs the kernel runs (user-space gate §5), as plan 2 of
//! milestone 2 has them: one child at a time, started by the in-kernel
//! shell. `spawn` loads the child; `wait` runs it on its own kernel stack
//! until it exits, and gives everything it had back. There is no
//! scheduler: while the child runs, the shell waits inside `wait`, and the
//! timer only counts ticks. Plan 3 replaces this with the process table and
//! the scheduler; `arch::user::{enter, leave}` becomes its context switch.

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
    space: AddressSpace,
    stack: KernelStack,
    entry: Entry,
}

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// Pids count from 1 and are not used again while the kernel runs.
static NEXT_PID: AtomicU32 = AtomicU32::new(1);

/// The child while it runs: what its system calls reach. Lives on
/// `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
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

/// The whole of the file at `path`, if it may be a program: not a
/// directory, at most 16 MiB (spec §5.2).
fn read_program(vfs: &mut dyn Vfs, path: &[u8]) -> Result<Vec<u8>, Errno> {
    let node = vfs.lookup(path)?;
    let stat = vfs.stat(node)?;
    if stat.kind == FileType::Directory {
        return Err(Errno::EISDIR);
    }
    if stat.size > elf::MAX_SIZE as u64 {
        return Err(Errno::ENOEXEC);
    }
    let mut file = alloc::vec![0; stat.size as usize];
    let mut done = 0;
    while done < file.len() {
        match vfs.read_at(node, done as u64, &mut file[done..])? {
            0 => break,
            n => done += n,
        }
    }
    file.truncate(done);
    Ok(file)
}

fn memory_error(e: MapError) -> Errno {
    match e {
        MapError::OutOfMemory => Errno::ENOMEM,
        _ => Errno::ENOEXEC,
    }
}

/// Loads the program at `path` (read through `vfs`) with `args` (argument
/// 0 first) as the child; its pid. `EAGAIN` while another child exists.
pub fn spawn(vfs: &mut dyn Vfs, path: &[u8], args: &[&[u8]]) -> Result<u32, Errno> {
    let mut child = CHILD.lock();
    if child.is_some() {
        return Err(Errno::EAGAIN);
    }
    let name = String::from_utf8_lossy(path);
    let file = read_program(vfs, path)?;
    let program = elf::check(&file, arch::ELF_MACHINE, relay_abi::VERSION).map_err(|e| {
        klogln!("spawn {name}: {e}");
        Errno::ENOEXEC
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
            Err(e) => {
                space.destroy(mem);
                Err(e)
            }
        }
    });
    let (space, entry) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            mm::free_kernel_stack(stack);
            klogln!("spawn {name}: {e}");
            return Err(e);
        }
    };
    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
    *child = Some(Child {
        pid,
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
        space: &child.space,
        out,
        waiter: 0,
        status: None,
    };
    let e = child.entry;
    let entry = arch::user::UserEntry {
        ip: e.ip,
        sp: e.sp,
        rdi: e.args,
        rsi: e.args_len,
        rdx: e.argc,
    };
    RUNNING.store((&raw mut running).cast(), Ordering::Release);
    let waiter = &raw mut running.waiter;
    // SAFETY: `running` outlives the run; the system calls reach it only
    // through `RUNNING`, while this function waits in `run`.
    arch::user::run(
        &entry,
        child.stack.top(),
        child.space.pml4(),
        mm::kernel_pml4(),
        unsafe { &mut *waiter },
    );
    RUNNING.store(core::ptr::null_mut(), Ordering::Release);
    let status = running.status.unwrap_or_default();
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
    let waiter = &raw const r.waiter;
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
        Outcome::Exit(code) => end(WaitStatus::exited(code)),
    }
}

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the child is killed instead.
pub fn non_canonical_return(ip: u64) -> ! {
    klogln!("pid killed: return to non-canonical address {ip:#x}");
    end(WaitStatus {
        how: relay_abi::wait::KILLED,
        ip,
        ..WaitStatus::default()
    })
}
````

- [ ] **Step 9: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! `shell::Console` (input from the USB keyboards and COM1), the clock,
//! memory figures and kernel log as `shell::System`, and `vfs::Env` for
//! filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, power, rtc, serial, usb};
use alloc::vec::Vec;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, MountTable};

````

with:

````rust
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

````

Replace:

````rust
        power::poweroff(self.test_mode)
    }
}

````

with:

````rust
        power::poweroff(self.test_mode)
    }

    fn spawn(
        &mut self,
        vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        Some(proc::spawn(vfs, path, args))
    }

    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        proc::wait(pid, out)
    }
}

````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::user`

Expected: PASS: 4 tests.

- [ ] **Step 11: Run the `programs`, `boot` scenarios**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add kernel tests
git commit -m "kernel: programs from /bin run in ring 3, and the in-kernel shell waits for them"
````


### Task 16: Every refused program is named in the kernel log

Review finding (minor): Task 15's `spawn` refused a file over 16 MiB with `ENOEXEC` before `elf::check` ran, so the kernel log, which is how the NUC is debugged, had no `spawn <path>: <reason>` line for it, although every other refusal has one. `read_program` now reads and checks the file and says why it refuses it: `Unreadable(errno)` (missing, a directory, a disk error) or `NotAProgram(reason)`; `spawn` logs every `NotAProgram` reason in one place and returns `ENOEXEC` (decision 9). A file over 16 MiB is still refused before it is read, so a big file is never read into memory: the tests run on a filesystem whose reads fail the test when they ask for more than 16 MiB, since `elf::check` refuses by size too and would hide a missing early check. On the way, `wait` reaches the running child through one raw pointer, from before the run until after it, instead of holding a `&mut` to it while its system calls make another; `arch::user::run` takes that pointer and is `unsafe` (the reviewer's aliasing note). Mutation check: without the early size check the tests fail.

**Files:**
- Modify: `kernel/src/arch/user.rs`
- Modify: `kernel/src/proc.rs`

**Interfaces:**
- Consumes: Task 15.
- Produces: `proc`'s private `Refusal::{Unreadable(Errno), NotAProgram(elf::ElfError)}` and `read_program(&mut dyn Vfs, &[u8]) -> Result<(Vec<u8>, elf::Program), Refusal>`; `arch::user::run` is `unsafe fn run(&UserEntry, kernel_stack: u64, pml4: u64, kernel_pml4: u64, waiter: *mut u64)`, and `leave` takes `*mut u64`.

- [ ] **Step 1: Add the failing tests to `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
    })
}
````

with:

````rust
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use vfs::{DirEntry, Env, FileSystem, Ino, MemFs, MountTable, Stat, StatFs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    /// A `MemFs` whose files must not be read: a file refused by its size
    /// is never read into memory.
    struct NoReads(MemFs);

    impl FileSystem for NoReads {
        fn root(&self) -> Ino {
            self.0.root()
        }
        fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
            self.0.stat(ino)
        }
        fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.0.lookup(dir, name)
        }
        fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
            self.0.read_dir(dir)
        }
        fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
            self.0.read_link(ino)
        }
        fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
            assert!(buf.len() <= elf::MAX_SIZE, "a read of {} bytes", buf.len());
            self.0.read_at(ino, offset, buf)
        }
        fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
            self.0.write_at(ino, offset, buf)
        }
        fn truncate(&mut self, ino: Ino, size: u64) -> Result<(), Errno> {
            self.0.truncate(ino, size)
        }
        fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
            self.0.touch(ino)
        }
        fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.0.create(dir, name)
        }
        fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
            self.0.mkdir(dir, name)
        }
        fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
            self.0.unlink(dir, name)
        }
        fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
            self.0.rmdir(dir, name)
        }
        fn rename(
            &mut self,
            from_dir: Ino,
            from: &[u8],
            to_dir: Ino,
            to: &[u8],
        ) -> Result<(), Errno> {
            self.0.rename(from_dir, from, to_dir, to)
        }
        fn statfs(&mut self) -> Result<StatFs, Errno> {
            self.0.statfs()
        }
        fn sync(&mut self) -> Result<(), Errno> {
            self.0.sync()
        }
        fn shutdown(&mut self) -> Result<(), Errno> {
            self.0.shutdown()
        }
    }

    /// `/root` with a text file and a sparse file of 16 MiB and one byte,
    /// on a filesystem that fails a test if more than 16 MiB are read.
    fn root() -> MountTable {
        let fs = NoReads(MemFs::new(Box::new(Clock)));
        let mut vfs = MountTable::new(Box::new(fs));
        vfs.mkdir(b"/root").unwrap();
        let text = vfs.create(b"/root/text").unwrap();
        vfs.write_at(text, 0, b"not a program\n").unwrap();
        let big = vfs.create(b"/root/big").unwrap();
        vfs.truncate(big, elf::MAX_SIZE as u64 + 1).unwrap();
        vfs
    }

    #[test]
    fn a_file_that_is_no_program_says_why() {
        let mut vfs = root();
        assert_eq!(
            read_program(&mut vfs, b"/root/text").unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::NotElf)
        );
        assert_eq!(
            read_program(&mut vfs, b"/root/big").unwrap_err(),
            Refusal::NotAProgram(elf::ElfError::TooBig(16 * 1024 * 1024 + 1)),
            "refused by its size, with the reason for the log"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_its_error() {
        let mut vfs = root();
        assert_eq!(
            read_program(&mut vfs, b"/root/missing").unwrap_err(),
            Refusal::Unreadable(Errno::ENOENT)
        );
        assert_eq!(
            read_program(&mut vfs, b"/root").unwrap_err(),
            Refusal::Unreadable(Errno::EISDIR)
        );
    }
}
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib proc`

Expected: FAIL: compile errors such as `` cannot find type `Refusal` in this scope ``.

- [ ] **Step 3: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// own tables (`kernel_pml4`) back in CR3 and interrupts as they were.
pub fn run(entry: &UserEntry, kernel_stack: u64, pml4: u64, kernel_pml4: u64, waiter: &mut u64) {
    let enabled = interrupts::are_enabled();
````

with:

````rust
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
````

Replace:

````rust
#[unsafe(naked)]
pub unsafe extern "C" fn leave(waiter: *const u64) -> ! {
    naked_asm!(
````

with:

````rust
#[unsafe(naked)]
pub unsafe extern "C" fn leave(waiter: *mut u64) -> ! {
    naked_asm!(
````

- [ ] **Step 4: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

/// The whole of the file at `path`, if it may be a program: not a
/// directory, at most 16 MiB (spec §5.2).
fn read_program(vfs: &mut dyn Vfs, path: &[u8]) -> Result<Vec<u8>, Errno> {
    let node = vfs.lookup(path)?;
    let stat = vfs.stat(node)?;
    if stat.kind == FileType::Directory {
        return Err(Errno::EISDIR);
    }
    if stat.size > elf::MAX_SIZE as u64 {
        return Err(Errno::ENOEXEC);
    }
    let mut file = alloc::vec![0; stat.size as usize];
    let mut done = 0;
````

with:

````rust

/// Why `spawn` refuses a file.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    /// It cannot be read (missing, a directory, a disk error).
    Unreadable(Errno),
    /// It is not a program this kernel runs: `ENOEXEC`, and the reason
    /// goes to the kernel log.
    NotAProgram(elf::ElfError),
}

impl From<Errno> for Refusal {
    fn from(e: Errno) -> Refusal {
        Refusal::Unreadable(e)
    }
}

/// The file at `path` and the program in it (spec §5.2). A file over 16
/// MiB is refused before anything is read.
fn read_program(vfs: &mut dyn Vfs, path: &[u8]) -> Result<(Vec<u8>, elf::Program), Refusal> {
    let node = vfs.lookup(path)?;
    let stat = vfs.stat(node)?;
    if stat.kind == FileType::Directory {
        return Err(Errno::EISDIR.into());
    }
    let size = usize::try_from(stat.size).unwrap_or(usize::MAX);
    if size > elf::MAX_SIZE {
        return Err(Refusal::NotAProgram(elf::ElfError::TooBig(size)));
    }
    let mut file = alloc::vec![0; size];
    let mut done = 0;
````

Replace:

````rust
    file.truncate(done);
    Ok(file)
}
````

with:

````rust
    file.truncate(done);
    match elf::check(&file, arch::ELF_MACHINE, relay_abi::VERSION) {
        Ok(program) => Ok((file, program)),
        Err(e) => Err(Refusal::NotAProgram(e)),
    }
}
````

Replace:

````rust
    let name = String::from_utf8_lossy(path);
    let file = read_program(vfs, path)?;
    let program = elf::check(&file, arch::ELF_MACHINE, relay_abi::VERSION).map_err(|e| {
        klogln!("spawn {name}: {e}");
        Errno::ENOEXEC
    })?;
````

with:

````rust
    let name = String::from_utf8_lossy(path);
    let (file, program) = read_program(vfs, path).map_err(|r| match r {
        Refusal::Unreadable(e) => e,
        Refusal::NotAProgram(e) => {
            klogln!("spawn {name}: {e}");
            Errno::ENOEXEC
        }
    })?;
````

Replace:

````rust
    };
    let e = child.entry;
````

with:

````rust
    };
    // From here until `run` returns, `running` is reached only through
    // this pointer: here, and in its system calls (`RUNNING`).
    let r = &raw mut running;
    let e = child.entry;
````

Replace:

````rust
    };
    RUNNING.store((&raw mut running).cast(), Ordering::Release);
    let waiter = &raw mut running.waiter;
    // SAFETY: `running` outlives the run; the system calls reach it only
    // through `RUNNING`, while this function waits in `run`.
    arch::user::run(
        &entry,
        child.stack.top(),
        child.space.pml4(),
        mm::kernel_pml4(),
        unsafe { &mut *waiter },
    );
    RUNNING.store(core::ptr::null_mut(), Ordering::Release);
    let status = running.status.unwrap_or_default();
    mm::with_user_memory(|mem, _| child.space.destroy(mem));
````

with:

````rust
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
````

Replace:

````rust
    r.status = Some(status);
    let waiter = &raw const r.waiter;
    // SAFETY: `enter` saved it and has not returned.
````

with:

````rust
    r.status = Some(status);
    let waiter = &raw mut r.waiter;
    // SAFETY: `enter` saved it and has not returned.
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib proc`

Expected: PASS: 2 tests.

- [ ] **Step 6: Run the `programs` scenario**

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -m "kernel: a program refused for its size is named in the kernel log, and wait reaches the running child through one pointer"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 25 scenario(s) passed`.

````bash
git push -u origin m2p2/ring3
gh pr create --base main --head m2p2/ring3 --title "Milestone 2, plan 2: Ring 3" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 9–16: the in-kernel shell runs a name it does not know from `/bin` (or a path) through `System::spawn`/`wait`, and `..`, `.` and `''` stay `command not found`; the GDT gets ring 3's segments in `sysret`'s order and the TSS its `rsp0`, with NMIs and machine checks on the IST stack; the architecture-neutral dispatcher serves `exit` and `write` (a page at a time, so a bad page costs only its own bytes); `syscall`/`sysret` entry and `enter`/`leave` run one child to completion on its own kernel stack; every refused program is named in the kernel log; the e2e step `expect-same`; the `programs` scenario: `t-args a 'b c' ''` prints its three arguments.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed now: the programs run in QEMU; NUC check 3 runs them with PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p2/ring3 --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-ring3
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: Faults, and ring 3 on the NUC (Tasks 17–20)

A fault in ring 3 ends the program, with spec §11.1's message; `t-fault` makes each kind happen; a program's flags never reach the kernel (the prototype review's critical finding); a killed program's report survives a write error. NUC check 3 runs a program and a fault. It ends with NUC check 3 on a stick written by `cargo xtask flash --full` from this worktree.

Branch `m2p2/faults`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-faults`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p2/faults /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-faults origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-faults
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p2/ring3` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p2/faults /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-faults m2p2/ring3`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p2/ring3>` and re-run `cargo xtask ci` before pushing.

### Task 17: A fault in ring 3 ends the program

Spec §11.1: an exception in ring 3 kills only that program. `relay_abi::wait` gains the kill reason `KILLED_FAULT`, the fault kinds in words every architecture has (page fault, general protection, invalid opcode, divide error, FPU/SSE instruction, stack overflow, any other) and a page fault's access, and `WaitStatus::fault`; its `Display` says spec §11.1's words (`page fault at 0x0, read, ip 0x401a2c`), so the kernel log, the in-kernel shell and plan 4's `/bin/sh` say the same without the kernel needing the shell (decision 9). `arch/fault.rs` classifies an x86_64 exception (vector, error code, `cr2`): a page fault's access from its error code (an instruction fetch before a write), a stack-segment fault as a general protection fault, any other exception as `CPU exception <n>`; an NMI, a double fault or a machine check is the machine's trouble and still reaches the panic screen. `exception_dispatch` sends every other exception from ring 3 to `proc::fault`, which turns a page fault in the stack's guard page into a stack overflow, logs `pid <n> (<path>): killed: …` and leaves to `wait`; a non-canonical return is now a general-protection fault. The shell prints `relay-sh: <name>: killed (<words>)` with bash's status for the signal Linux would send (139 for SIGSEGV, 132 SIGILL, 136 SIGFPE; 137 for a reason it does not know). `t-fault KIND` (spec §8.5) does each wrong thing but `sse`, which needs plan 3's CR0.TS (decision 12). The `userfault` scenario runs every kind, reads the kernel log line, runs `t-args` after the last one and compares `free` before and after; `/bin` now lists two programs, so the `system` scenario's `ls /bin` lines change with it. Mutation checks: a write before a fetch, `#SS` not a general protection fault, an NMI or machine check killing the program, `cr2` not the address, the guard page's bounds, FPU's status and an unknown kind taken as known all fail a test.

**Files:**
- Modify: `crates/relay-abi/src/wait.rs`
- Create: `crates/shell/src/killed.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/shell.rs`
- Create: `kernel/src/arch/fault.rs`
- Modify: `kernel/src/arch/idt.rs`
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/exec.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `tests/e2e/system.txt`
- Create: `tests/e2e/userfault.txt`
- Create: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Tasks 8 (`STACK_BOTTOM`), 9 and 15.
- Produces: `relay_abi::wait::{KILLED_FAULT, FAULT_PAGE, FAULT_GENERAL_PROTECTION, FAULT_INVALID_OPCODE, FAULT_DIVIDE, FAULT_FPU, FAULT_STACK_OVERFLOW, FAULT_OTHER, ACCESS_READ, ACCESS_WRITE, ACCESS_EXECUTE}`, `WaitStatus::{fault(kind, detail, address, ip), is_known_fault}`, `impl Display for WaitStatus`; `shell::killed::{KILLED, killed(&WaitStatus) -> (String, i32)}`; `arch::fault::{Fault { kind, detail, address }, classify(vector, error_code, cr2) -> Option<Fault>}`; `exec::in_guard_page(u64) -> bool`; `proc::fault(kind, detail, address, ip) -> !`; the program `t-fault`; the `userfault` scenario.

- [ ] **Step 1: Add the failing tests to `crates/relay-abi/src/wait.rs`**

In `crates/relay-abi/src/wait.rs`, replace:

````rust
        assert_ne!(EXITED, KILLED);
    }
}
````

with:

````rust
        assert_ne!(EXITED, KILLED);
    }

    #[test]
    fn a_fault_carries_its_kind_address_and_instruction() {
        let w = WaitStatus::fault(FAULT_PAGE, ACCESS_WRITE, 0x10, 0x40_1000);
        assert_eq!(
            w,
            WaitStatus {
                how: KILLED,
                code: KILLED_FAULT,
                fault: FAULT_PAGE,
                detail: ACCESS_WRITE,
                address: 0x10,
                ip: 0x40_1000
            }
        );
        let kinds = [
            FAULT_PAGE,
            FAULT_GENERAL_PROTECTION,
            FAULT_INVALID_OPCODE,
            FAULT_DIVIDE,
            FAULT_FPU,
            FAULT_STACK_OVERFLOW,
            FAULT_OTHER,
        ];
        assert_eq!(kinds, [1, 2, 3, 4, 5, 6, 7], "numbers are the ABI's");
        assert_eq!([ACCESS_READ, ACCESS_WRITE, ACCESS_EXECUTE], [1, 2, 3]);
    }

    fn words(kind: u32, detail: u32, address: u64) -> String {
        WaitStatus::fault(kind, detail, address, 0x40_1a2c).to_string()
    }

    #[test]
    fn each_fault_reads_as_the_spec_says() {
        assert_eq!(
            words(FAULT_PAGE, ACCESS_READ, 0),
            "page fault at 0x0, read, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, ACCESS_WRITE, 0x40_1000),
            "page fault at 0x401000, write, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, ACCESS_EXECUTE, 0x40_3000),
            "page fault at 0x403000, execute, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_PAGE, 9, 8),
            "page fault at 0x8, access, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_STACK_OVERFLOW, ACCESS_WRITE, 0x7fff_ffef_fff8),
            "stack overflow at 0x7fffffeffff8, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_GENERAL_PROTECTION, 0, 0),
            "general protection fault, ip 0x401a2c"
        );
        assert_eq!(
            words(FAULT_INVALID_OPCODE, 0, 0),
            "invalid opcode, ip 0x401a2c"
        );
        assert_eq!(words(FAULT_DIVIDE, 0, 0), "divide error, ip 0x401a2c");
        assert_eq!(words(FAULT_FPU, 0, 0), "FPU/SSE instruction, ip 0x401a2c");
        assert_eq!(words(FAULT_OTHER, 17, 0), "CPU exception 17, ip 0x401a2c");
    }

    #[test]
    fn other_ends_read_as_exits_or_kills() {
        assert_eq!(WaitStatus::exited(3).to_string(), "exited with 3");
        assert_eq!(words(0, 0, 0), "killed");
        assert_eq!(words(8, 0, 0), "killed");
        let other_reason = WaitStatus {
            how: KILLED,
            code: 7,
            ..WaitStatus::default()
        };
        assert_eq!(other_reason.to_string(), "killed");
        assert!(!other_reason.is_known_fault());
        assert!(WaitStatus::fault(FAULT_OTHER, 3, 0, 0).is_known_fault());
    }
}
````

- [ ] **Step 2: Write the failing tests for `crates/shell/src/killed.rs`**

Create `crates/shell/src/killed.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn fault(kind: u32) -> (String, i32) {
        killed(&WaitStatus::fault(kind, ACCESS_READ, 0, 0x40_1a2c))
    }

    #[test]
    fn each_fault_gets_bash_s_status_for_its_signal() {
        assert_eq!(
            fault(FAULT_PAGE),
            ("killed (page fault at 0x0, read, ip 0x401a2c)".into(), 139)
        );
        assert_eq!(fault(FAULT_STACK_OVERFLOW).1, 139);
        assert_eq!(fault(FAULT_GENERAL_PROTECTION).1, 139);
        assert_eq!(fault(FAULT_OTHER).1, 139);
        assert_eq!(fault(FAULT_INVALID_OPCODE).1, 132);
        assert_eq!(fault(FAULT_DIVIDE).1, 136);
        assert_eq!(
            fault(FAULT_FPU),
            ("killed (FPU/SSE instruction, ip 0x401a2c)".into(), 136)
        );
    }

    #[test]
    fn what_the_shell_does_not_know_is_just_killed() {
        assert_eq!(fault(99), ("killed".into(), 137));
        let w = WaitStatus {
            how: relay_abi::wait::KILLED,
            code: 7,
            ..WaitStatus::default()
        };
        assert_eq!(killed(&w), ("killed".into(), 137));
    }
}
````

- [ ] **Step 3: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod io;
pub mod parser;
````

with:

````rust
mod io;
pub mod killed;
pub mod parser;
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
                "[1] a\nt-args: note\n[2] b c\nrelay-sh: t-args: killed\n".into()
            )
        );
    }

    #[test]
````

with:

````rust
                "[1] a\nt-args: note\n[2] b c\nrelay-sh: t-args: killed\n".into()
            )
        );
        // A fault, redirected: the message goes to the screen.
        h.system.programs[0].status = WaitStatus::fault(
            relay_abi::wait::FAULT_PAGE,
            relay_abi::wait::ACCESS_READ,
            0,
            0x40_1a2c,
        );
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nrelay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
            )
        );
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
    }

    #[test]
````

- [ ] **Step 5: Write the failing tests for `kernel/src/arch/fault.rs`**

Create `kernel/src/arch/fault.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_faults_say_which_access_at_which_address() {
        // Error codes as the CPU gives them: present, write, user, fetch.
        let pf = |code| classify(14, code, 0x1234).unwrap();
        assert_eq!(
            pf(0b0_0100),
            Fault {
                kind: FAULT_PAGE,
                detail: ACCESS_READ,
                address: 0x1234
            }
        );
        assert_eq!(pf(0b0_0110).detail, ACCESS_WRITE);
        assert_eq!(pf(0b0_0111).detail, ACCESS_WRITE, "to a read-only page");
        assert_eq!(
            pf(0b1_0101).detail,
            ACCESS_EXECUTE,
            "from a no-execute page"
        );
        assert_eq!(pf(0b0_0101).detail, ACCESS_READ, "a kernel page");
    }

    #[test]
    fn each_exception_has_its_kind() {
        let kind = |v| classify(v, 0, 0x99).map(|f| f.kind);
        assert_eq!(kind(0), Some(FAULT_DIVIDE));
        assert_eq!(kind(6), Some(FAULT_INVALID_OPCODE));
        assert_eq!(kind(7), Some(FAULT_FPU));
        assert_eq!(kind(13), Some(FAULT_GENERAL_PROTECTION));
        assert_eq!(kind(12), Some(FAULT_GENERAL_PROTECTION));
        assert_eq!(
            classify(13, 0x18, 0x99).unwrap().address,
            0,
            "cr2 is a page fault's"
        );
        assert_eq!(
            classify(17, 0, 0),
            Some(Fault {
                kind: FAULT_OTHER,
                detail: 17,
                address: 0
            }),
            "alignment check"
        );
        assert_eq!(classify(3, 0, 0).unwrap().detail, 3);
    }

    #[test]
    fn nmis_double_faults_and_machine_checks_are_not_the_program_s() {
        for v in [2, 8, 18] {
            assert_eq!(classify(v, 0, 0), None, "vector {v}");
        }
    }
}
````

- [ ] **Step 6: Declare the new module in `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust

pub mod gdt;
````

with:

````rust

pub mod fault;
pub mod gdt;
````

- [ ] **Step 7: Add the failing tests to `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, replace:

````rust
    #[test]
    fn the_arguments_are_at_the_top_with_the_stack_below_them() {
````

with:

````rust
    #[test]
    fn only_the_guard_page_is_an_overflow() {
        assert!(in_guard_page(STACK_BOTTOM - 8));
        assert!(in_guard_page(STACK_BOTTOM - PAGE));
        assert!(!in_guard_page(STACK_BOTTOM));
        assert!(!in_guard_page(STACK_BOTTOM - PAGE - 1));
        assert!(!in_guard_page(0));
    }

    #[test]
    fn the_arguments_are_at_the_top_with_the_stack_below_them() {
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault\n
````

- [ ] **Step 9: Add the scenario `tests/e2e/userfault.txt`**

Create `tests/e2e/userfault.txt`:

````text
# A fault in ring 3 ends the program, not the kernel (user-space gate
# §11.1; milestone 2, plan 2): each kind of t-fault is killed with its
# message, the kernel log names it, the shell carries on, and memory in
# use is the same before and after. `sse` comes with plan 3's CR0.TS.
timeout 30
expect root@relay:~# $
send t-args warm-up
expect \n\[1\] warm-up\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-fault null-read
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)\n
send t-fault null-write
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, write, ip 0x4[0-9a-f]+\)\n
send t-fault write-code
expect \nrelay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, write, ip 0x4[0-9a-f]+\)\n
send t-fault exec-data
expect \nrelay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, execute, ip 0x4[0-9a-f]+\)\n
send t-fault ud
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
send t-fault div0
expect \nrelay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)\n
send t-fault stack
expect \nrelay-sh: t-fault: killed \(stack overflow at 0x7fffffeff[0-9a-f]{3}, ip 0x4[0-9a-f]+\)\n
send t-fault kernel-read
expect \nrelay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)\n
send t-fault
expect \nusage: t-fault null-read\|
send dmesg
expect \npid \d+ \(/bin/t-fault\): killed: page fault at 0x0, read, ip 0x4[0-9a-f]+\n
send t-args still here
expect \n\[1\] still\n\[2\] here\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 10: Add the test program `userland/tests/src/bin/t-fault.rs`**

Create `userland/tests/src/bin/t-fault.rs`:

````rust
//! `t-fault KIND`: does something the CPU refuses, on purpose (spec §8.5),
//! so a scenario sees the kernel end the program, not itself:
//! `null-read`, `null-write`, `write-code`, `exec-data`, `ud`, `div0`,
//! `stack` (runs into the guard page below the stack) and `kernel-read`
//! (reads an upper-half address). Plan 3 adds `sse`, once CR0.TS makes
//! SSE fault.
#![no_std]
#![no_main]

use core::arch::asm;
use relay_rt::Args;
use relay_rt::sys;

relay_rt::main!(main);

/// Somewhere writable and not executable to jump to: `ret`, in data.
static mut DATA: [u8; 16] = [0xC3; 16];

fn main(args: Args) -> u8 {
    let kind = args.get(1).unwrap_or(b"");
    // SAFETY: none. Each of these faults, which is the point.
    unsafe {
        match kind {
            b"null-read" => asm!("mov {0}, qword ptr [{0}]", inout(reg) 0u64 => _),
            b"null-write" => asm!("mov qword ptr [{0}], 1", in(reg) 0u64),
            b"write-code" => {
                asm!("mov byte ptr [{0}], 0x90", in(reg) main as *const () as u64)
            }
            b"exec-data" => asm!("call {0}", in(reg) &raw const DATA as u64),
            b"ud" => asm!("ud2"),
            b"div0" => asm!(
                "div {0:e}",
                in(reg) 0u32,
                inout("eax") 1u32 => _,
                inout("edx") 0u32 => _,
            ),
            b"stack" => asm!("2:", "push rax", "jmp 2b", options(noreturn)),
            b"kernel-read" => {
                asm!("mov {0}, qword ptr [{0}]", inout(reg) 0xFFFF_8000_0000_0000u64 => _)
            }
            _ => {
                let _ = sys::write_all(
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read\n",
                );
                return 2;
            }
        }
    }
    let _ = sys::write_all(2, b"t-fault: no fault\n");
    1
}
````

- [ ] **Step 11: Run the tests to see them fail**

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` cannot find value `FAULT_PAGE` in this scope ``; `` cannot find value `ACCESS_WRITE` in this scope ``.

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `ACCESS_READ` in this scope ``; `` cannot find value `FAULT_PAGE` in this scope ``.

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `classify` in this scope ``; `` cannot find struct, variant or union type `Fault` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: FAIL: scenario `userfault` stops at line 12, timed out waiting for `\nrelay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)\n`.

- [ ] **Step 12: Change `crates/relay-abi/src/wait.rs`**

In `crates/relay-abi/src/wait.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction.

````

with:

````rust
//! code, or it was killed, with the reason and, for a fault, what the CPU
//! refused, where and at which instruction. Its `Display` says so in the
//! words of spec §11.1 (`page fault at 0x0, read, ip 0x401a2c`), for the
//! kernel log and the shells.

use core::fmt;

````

Replace:

````rust
pub const KILLED: u32 = 2;

````

with:

````rust
pub const KILLED: u32 = 2;

/// `WaitStatus::code` of a killed child: the CPU refused what it did.
pub const KILLED_FAULT: u32 = 1;

/// `WaitStatus::fault`: what the CPU refused (spec §11.1), in words any
/// architecture has.
pub const FAULT_PAGE: u32 = 1;
pub const FAULT_GENERAL_PROTECTION: u32 = 2;
pub const FAULT_INVALID_OPCODE: u32 = 3;
pub const FAULT_DIVIDE: u32 = 4;
/// A floating-point or SIMD instruction (spec §5.5).
pub const FAULT_FPU: u32 = 5;
/// A page fault in the stack's guard page.
pub const FAULT_STACK_OVERFLOW: u32 = 6;
/// Any other exception; `detail` is the architecture's number for it.
pub const FAULT_OTHER: u32 = 7;

/// `WaitStatus::detail` of a page fault or stack overflow: the access.
pub const ACCESS_READ: u32 = 1;
pub const ACCESS_WRITE: u32 = 2;
pub const ACCESS_EXECUTE: u32 = 3;

````

Replace:

````rust
    pub fault: u32,
    /// For a fault: more about it (the access of a page fault).
    pub detail: u32,
````

with:

````rust
    pub fault: u32,
    /// For a fault: more about it (the access of a page fault, the
    /// exception number of `FAULT_OTHER`).
    pub detail: u32,
````

Replace:

````rust
        }
    }
````

with:

````rust
        }
    }

    /// Whether the status is a fault whose kind this ABI knows.
    pub const fn is_known_fault(&self) -> bool {
        self.how == KILLED
            && self.code == KILLED_FAULT
            && self.fault >= FAULT_PAGE
            && self.fault <= FAULT_OTHER
    }

    /// A child killed for `fault` at instruction `ip`.
    pub const fn fault(fault: u32, detail: u32, address: u64, ip: u64) -> WaitStatus {
        WaitStatus {
            how: KILLED,
            code: KILLED_FAULT,
            fault,
            detail,
            address,
            ip,
        }
    }
}

impl fmt::Display for WaitStatus {
    /// `exited with 3`; for a fault what it was, where and at which
    /// instruction; `killed` for any other end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.how == EXITED {
            return write!(f, "exited with {}", self.code);
        }
        if !self.is_known_fault() {
            return f.write_str("killed");
        }
        let access = match self.detail {
            ACCESS_READ => "read",
            ACCESS_WRITE => "write",
            ACCESS_EXECUTE => "execute",
            _ => "access",
        };
        let address = self.address;
        match self.fault {
            FAULT_PAGE => write!(f, "page fault at {address:#x}, {access}")?,
            FAULT_STACK_OVERFLOW => write!(f, "stack overflow at {address:#x}")?,
            FAULT_GENERAL_PROTECTION => f.write_str("general protection fault")?,
            FAULT_INVALID_OPCODE => f.write_str("invalid opcode")?,
            FAULT_DIVIDE => f.write_str("divide error")?,
            FAULT_FPU => f.write_str("FPU/SSE instruction")?,
            _ => write!(f, "CPU exception {}", self.detail)?,
        }
        write!(f, ", ip {:#x}", self.ip)
    }
````

- [ ] **Step 13: Implement `crates/shell/src/killed.rs`**

Insert this at the top of `crates/shell/src/killed.rs`, above `#[cfg(test)]`:

````rust
//! What the shell says about a program that did not exit by itself (spec
//! §11.1 of the user-space gate): `killed (page fault at 0x0, read, ip
//! 0x401a2c)`, and the status bash would give for the signal Linux would
//! send (128 + 11 for SIGSEGV, + 4 for SIGILL, + 8 for SIGFPE).

use alloc::format;
use alloc::string::String;
use relay_abi::WaitStatus;
use relay_abi::wait::*;

/// A program killed with no reason the shell knows (bash's SIGKILL).
pub const KILLED: i32 = 128 + 9;

/// The words for a program that did not exit, and its exit status.
pub fn killed(w: &WaitStatus) -> (String, i32) {
    if !w.is_known_fault() {
        return (String::from("killed"), KILLED);
    }
    let signal = match w.fault {
        FAULT_INVALID_OPCODE => 4,
        FAULT_DIVIDE | FAULT_FPU => 8,
        _ => 11,
    };
    (format!("killed ({w})"), 128 + signal)
}

````

- [ ] **Step 14: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::io::{Console, System};
use crate::parser::{self, HOME, Redirect};
````

with:

````rust
use crate::io::{Console, System};
use crate::killed;
use crate::parser::{self, HOME, Redirect};
````

Replace:

````rust
pub const CANCELLED: i32 = 130;
/// Exit status of a program that was killed (bash's for SIGKILL).
pub const KILLED: i32 = 137;
/// The most of `/etc/motd` shown at start.
````

with:

````rust
pub const CANCELLED: i32 = 130;
/// The most of `/etc/motd` shown at start.
````

Replace:

````rust
            Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
            Ok(_) => {
                message = format!("{NAME}: {name}: killed\n");
                KILLED
            }
````

with:

````rust
            Ok(w) if w.how == relay_abi::wait::EXITED => w.code as i32,
            Ok(w) => {
                let (what, status) = killed::killed(&w);
                message = format!("{NAME}: {name}: {what}\n");
                status
            }
````

- [ ] **Step 15: Implement `kernel/src/arch/fault.rs`**

Insert this at the top of `kernel/src/arch/fault.rs`, above `#[cfg(test)]`:

````rust
//! What an x86_64 exception in ring 3 means for the program (user-space
//! gate §11.1): the fault's kind, its detail and address in the ABI's
//! architecture-neutral words. NMIs, double faults and machine checks are
//! the machine's trouble, not the program's, and still reach the panic
//! screen.

use relay_abi::wait::*;

/// A page fault's error code: the access was a write, an instruction
/// fetch.
const PF_WRITE: u64 = 1 << 1;
const PF_FETCH: u64 = 1 << 4;

/// A fault's kind, detail and address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    pub kind: u32,
    pub detail: u32,
    pub address: u64,
}

/// The fault exception `vector` (with its `error_code`, and `cr2` for a
/// page fault) is, if it is the program's.
pub fn classify(vector: u64, error_code: u64, cr2: u64) -> Option<Fault> {
    let fault = |kind, detail, address| {
        Some(Fault {
            kind,
            detail,
            address,
        })
    };
    match vector {
        0 => fault(FAULT_DIVIDE, 0, 0),
        6 => fault(FAULT_INVALID_OPCODE, 0, 0),
        7 => fault(FAULT_FPU, 0, 0),
        // A stack-segment fault in 64-bit mode is a non-canonical stack
        // address: as a general protection fault would be.
        12 | 13 => fault(FAULT_GENERAL_PROTECTION, 0, 0),
        14 => {
            let access = if error_code & PF_FETCH != 0 {
                ACCESS_EXECUTE
            } else if error_code & PF_WRITE != 0 {
                ACCESS_WRITE
            } else {
                ACCESS_READ
            };
            fault(FAULT_PAGE, access, cr2)
        }
        2 | 8 | 18 => None,
        v => fault(FAULT_OTHER, v as u32, 0),
    }
}

````

- [ ] **Step 16: Change `kernel/src/arch/idt.rs`**

In `kernel/src/arch/idt.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! Interrupt descriptor table. Every CPU exception is fatal in milestone 1:
//! the entry stubs save all registers and call `exception_dispatch`, which
//! draws the panic screen. Hardware interrupts have their own, returning
//! stubs in `irq`.
//!
````

with:

````rust
//! Interrupt descriptor table. The entry stubs save all registers and call
//! `exception_dispatch`. An exception in ring 3 ends the program that
//! caused it (user-space gate §11.1); any other draws the panic screen.
//! Hardware interrupts have their own, returning stubs in `irq`.
//!
````

Replace:

````rust
extern "C" fn exception_dispatch(frame: &ExceptionFrame) -> ! {
    crate::panic_screen::exception(frame)
````

with:

````rust
extern "C" fn exception_dispatch(frame: &ExceptionFrame) -> ! {
    if frame.cs & 3 == 3 {
        let cr2 = x86_64::registers::control::Cr2::read_raw();
        if let Some(f) = super::fault::classify(frame.vector, frame.error_code, cr2) {
            crate::proc::fault(f.kind, f.detail, f.address, frame.rip);
        }
    }
    crate::panic_screen::exception(frame)
````

- [ ] **Step 17: Change `kernel/src/exec.rs`**

In `kernel/src/exec.rs`, replace:

````rust
pub const STACK_BOTTOM: u64 = STACK_TOP - STACK_PAGES * PAGE;
/// The most argument bytes a program gets (spec §5.3).
````

with:

````rust
pub const STACK_BOTTOM: u64 = STACK_TOP - STACK_PAGES * PAGE;
/// Whether `address` is in the stack's guard page: a page fault there is
/// a stack overflow.
pub fn in_guard_page(address: u64) -> bool {
    (STACK_BOTTOM - PAGE..STACK_BOTTOM).contains(&address)
}

/// The most argument bytes a program gets (spec §5.3).
````

- [ ] **Step 18: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
    pid: u32,
    space: AddressSpace,
````

with:

````rust
    pid: u32,
    /// Its path, for the kernel log.
    name: String,
    space: AddressSpace,
````

Replace:

````rust

/// The child while it runs: what its system calls reach. Lives on
/// `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
    space: &'a AddressSpace,
````

with:

````rust

/// The child while it runs: what its system calls and faults reach. Lives
/// on `wait`'s stack; `RUNNING` points at it for that long.
struct Running<'a> {
    pid: u32,
    name: &'a str,
    space: &'a AddressSpace,
````

Replace:

````rust
        pid,
        space,
````

with:

````rust
        pid,
        name: name.into_owned(),
        space,
````

Replace:

````rust
    let mut running = Running {
        space: &child.space,
````

with:

````rust
    let mut running = Running {
        pid: child.pid,
        name: &child.name,
        space: &child.space,
````

Replace:

````rust
    // From here until `run` returns, `running` is reached only through
    // this pointer: here, and in its system calls (`RUNNING`).
    let r = &raw mut running;
````

with:

````rust
    // From here until `run` returns, `running` is reached only through
    // this pointer: here, and in its system calls and faults (`RUNNING`).
    let r = &raw mut running;
````

Replace:

````rust

/// A system call would return to a non-canonical address (spec §6.2):
/// `sysret` would fault in ring 0, so the child is killed instead.
pub fn non_canonical_return(ip: u64) -> ! {
    klogln!("pid killed: return to non-canonical address {ip:#x}");
    end(WaitStatus {
        how: relay_abi::wait::KILLED,
        ip,
        ..WaitStatus::default()
    })
}
````

with:

````rust

/// The running child caused an exception (spec §11.1): it is killed, and
/// the kernel log says how. A page fault in the stack's guard page is a
/// stack overflow.
pub fn fault(kind: u32, detail: u32, address: u64, ip: u64) -> ! {
    use relay_abi::wait::{FAULT_PAGE, FAULT_STACK_OVERFLOW};
    let kind = if kind == FAULT_PAGE && exec::in_guard_page(address) {
        FAULT_STACK_OVERFLOW
    } else {
        kind
    };
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
    fault(relay_abi::wait::FAULT_GENERAL_PROTECTION, 0, 0, ip)
}
````

- [ ] **Step 19: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 10 tests.

Run: `cargo test -p shell`

Expected: PASS: 125 tests.

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 225 tests.

- [ ] **Step 20: Run the `userfault`, `system`, `programs` scenarios**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 21: Run the whole gate: `/bin` lists another program**

Run: `cargo xtask ci`

Expected: Lint, unit tests and e2e all succeed; the last line is `all 26 scenario(s) passed`.

- [ ] **Step 22: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 23: Commit**

````bash
git add crates kernel tests userland
git commit -m "kernel: a fault in ring 3 ends the program, with its kind, address and instruction; t-fault"
````


### Task 18: A program's flags stay its own

Review finding (critical): a program can set any flag `popfq` allows, and `syscall` cleared only IF, DF, TF and AC (spec §6.2). A program that set the nested-task flag (NT) and called `exit` went back to the waiting shell through `leave` with NT still set, and the next program's `iretq` raised a general protection fault in ring 0: the panic screen, caused by a program (Linux fixed the same in 2014). An exception carries the program's AC into the kernel the same way, which would switch SMAP off once plan 3 enables it. `SFMASK` now also clears NT; `leave` loads the kernel's own flags (only bit 1) before it returns to the waiting code; and `run` asserts, as a debug assertion (the kernel's `relay` profile keeps them), that none of TF, DF, AC or NT came back, since otherwise a missing flag reset would show only once SMAP is on. The `sysenter` MSRs are zeroed as well, as Linux does: on Intel CPUs `sysenter` is legal in 64-bit mode, and with a code segment of 0 it is a general protection fault whatever the firmware left there (decision 2). `t-fault` gains `flags-exit` and `flags-ud`, which set NT, AC and DF and then exit or fault; `userfault` runs a program after each (decision 12). The new kinds go in with the tests, before the red run, so it shows the kernel failing, not a kind missing: without the fix `userfault` stops at the program after `flags-exit`, on the panic screen. Mutation checks: `SFMASK` without NT fails the unit test; `leave` without its flag load fails `userfault` through the assertion; zeroing the `sysenter` MSRs survives in QEMU, whose are zero already.

**Files:**
- Modify: `kernel/src/arch/user.rs`
- Modify: `tests/e2e/userfault.txt`
- Modify: `userland/tests/src/bin/t-fault.rs`

**Interfaces:**
- Consumes: Tasks 15 and 17.
- Produces: `arch::user::SFMASK` with NT; `leave` loads RFLAGS 0x2; `t-fault flags-exit` and `t-fault flags-ud`.

- [ ] **Step 1: Add the failing tests to `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, replace:

````rust
    fn syscall_clears_the_flags_the_kernel_needs_clear() {
        assert_eq!(SFMASK.bits(), (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18));
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
    }
````

with:

````rust
    fn syscall_clears_the_flags_the_kernel_needs_clear() {
        // IF, DF, TF, AC and NT.
        assert_eq!(
            SFMASK.bits(),
            (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18) | (1 << 14)
        );
        assert_eq!(USER_RFLAGS, (1 << 9) | 2, "interrupts on, nothing else");
        assert_eq!(KERNEL_RFLAGS, 2, "leave: nothing of the program's");
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/userfault.txt`**

In `tests/e2e/userfault.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# message, the kernel log names it, the shell carries on, and memory in
# use is the same before and after. `sse` comes with plan 3's CR0.TS.
timeout 30
````

with:

````text
# message, the kernel log names it, the shell carries on, and memory in
# use is the same before and after. A program's flags stay its own, after
# an exit as after a fault. `sse` comes with plan 3's CR0.TS.
timeout 30
````

Replace:

````text
expect \nrelay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)\n
send t-fault
````

with:

````text
expect \nrelay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)\n
send t-fault flags-exit
send t-args after-flags
expect \n\[1\] after-flags\n
send t-fault flags-ud
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
send t-args after-a-fault
expect \n\[1\] after-a-fault\n
send t-fault
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-fault.rs`**

In `userland/tests/src/bin/t-fault.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! `stack` (runs into the guard page below the stack) and `kernel-read`
//! (reads an upper-half address). Plan 3 adds `sse`, once CR0.TS makes
//! SSE fault.
#![no_std]
````

with:

````rust
//! `stack` (runs into the guard page below the stack) and `kernel-read`
//! (reads an upper-half address). `flags-exit` and `flags-ud` set the
//! nested-task, alignment-check and direction flags, then exit or fault:
//! none of them may reach the kernel or the next program. Plan 3 adds
//! `sse`, once CR0.TS makes SSE fault.
#![no_std]
````

Replace:

````rust
static mut DATA: [u8; 16] = [0xC3; 16];

````

with:

````rust
static mut DATA: [u8; 16] = [0xC3; 16];

/// RFLAGS bits: direction, nested task, alignment check.
const FLAGS: u64 = (1 << 10) | (1 << 14) | (1 << 18);

````

Replace:

````rust
            }
            _ => {
                let _ = sys::write_all(
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read\n",
                );
````

with:

````rust
            }
            // `exit(0)` at once: no Rust code runs with these flags.
            b"flags-exit" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "syscall",
                flags = in(reg) FLAGS,
                in("rax") 1u64,
                in("rdi") 0u64,
                options(noreturn),
            ),
            b"flags-ud" => asm!(
                "pushfq",
                "or qword ptr [rsp], {flags}",
                "popfq",
                "ud2",
                flags = in(reg) FLAGS,
                options(noreturn),
            ),
            _ => {
                let _ = sys::write_all(
                    2,
                    b"usage: t-fault null-read|null-write|write-code|exec-data|ud|div0|stack|kernel-read|flags-exit|flags-ud\n",
                );
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib arch::user`

Expected: FAIL: compile errors such as `` cannot find value `KERNEL_RFLAGS` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: FAIL: scenario `userfault` stops at line 30, timed out waiting for `\n\[1\] after-flags\n`.

- [ ] **Step 5: Change `kernel/src/arch/user.rs`**

In `kernel/src/arch/user.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//!   goes back to that kernel code from the program's kernel stack, which
//!   is abandoned. `exit` and (with plan 2's faults) a program's fault end
//!   that way.
//!
````

with:

````rust
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

Replace:

````rust
use x86_64::registers::model_specific::{
    Efer, EferFlags, GsBase, KernelGsBase, LStar, SFMask, Star,
};
````

with:

````rust
use x86_64::registers::model_specific::{
    Efer, EferFlags, GsBase, KernelGsBase, LStar, Msr, SFMask, Star,
};
````

Replace:

````rust
/// The flags `syscall` clears (spec §6.2): no interrupts until the stub is
/// on the kernel stack, and the direction, trap and alignment-check flags
/// in their kernel state.
pub const SFMASK: RFlags = RFlags::INTERRUPT_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::TRAP_FLAG)
    .union(RFlags::ALIGNMENT_CHECK);

````

with:

````rust
/// The flags `syscall` clears (spec §6.2): no interrupts until the stub is
/// on the kernel stack, and the direction, trap, alignment-check and
/// nested-task flags in their kernel state.
pub const SFMASK: RFlags = RFlags::INTERRUPT_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::TRAP_FLAG)
    .union(RFlags::ALIGNMENT_CHECK)
    .union(RFlags::NESTED_TASK);

/// The kernel's flags when `leave` goes back: only bit 1, always set.
const KERNEL_RFLAGS: u64 = 0x2;

/// Flags a program may set that the kernel must never run with.
const PROGRAM_FLAGS: RFlags = RFlags::TRAP_FLAG
    .union(RFlags::DIRECTION_FLAG)
    .union(RFlags::ALIGNMENT_CHECK)
    .union(RFlags::NESTED_TASK);

/// `sysenter`'s code segment, stack and entry MSRs.
const IA32_SYSENTER: [u32; 3] = [0x174, 0x175, 0x176];

````

Replace:

````rust
    SFMask::write(SFMASK);
}
````

with:

````rust
    SFMask::write(SFMASK);
    // `sysenter` is legal in 64-bit mode on Intel CPUs: with a code segment
    // of 0 it is a general protection fault, whatever the firmware left.
    for msr in IA32_SYSENTER {
        // SAFETY: these MSRs exist on every x86_64 CPU; zero disables
        // `sysenter`.
        unsafe { Msr::new(msr).write(0) };
    }
}
````

Replace:

````rust
    }
    if enabled {
````

with:

````rust
    }
    // A kernel bug if the program's flags came back with it.
    debug_assert!(
        !x86_64::registers::rflags::read().intersects(PROGRAM_FLAGS),
        "a program's flags reached the kernel"
    );
    if enabled {
````

Replace:

````rust
/// Goes back to the kernel code waiting in `enter(waiter, _)`, on its
/// stack, with interrupts off. The stack this runs on is abandoned.
///
````

with:

````rust
/// Goes back to the kernel code waiting in `enter(waiter, _)`, on its
/// stack, with interrupts off and the kernel's flags. The stack this runs
/// on is abandoned.
///
````

Replace:

````rust
        "pop rbp",
        "ret",
    )
````

with:

````rust
        "pop rbp",
        "push {rflags}",
        "popfq",
        "ret",
        rflags = const KERNEL_RFLAGS,
    )
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib arch::user`

Expected: PASS: 4 tests.

- [ ] **Step 7: Run the `userfault`, `programs` scenarios**

Run: `cargo xtask test --e2e-only --scenario userfault`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario programs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: a program's flags stay its own: syscall clears NT, and leave restores the kernel's"
````


### Task 19: A killed program's report survives a write error

Review finding (minor): when a killed program's redirected output also hit a write error, the shell printed only the write error with status 1, and the `killed (…)` line and its status (139 for a page fault) were lost; the kernel log still had the kill. The write error now comes first, then the kill's report, and a killed program keeps its status; a program that exited still gets 1, as a built-in does (decision 7). Mutation checks: status 1 for a killed program, and the kill's report replaced by the write error, each fail the test.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Tasks 9 and 17.
- Produces: no new interface.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
    }
````

with:

````rust
        assert_eq!(h.get("/tmp/out"), b"[1] a\n[2] b c\n");
        // A write error too: both are reported, and the kill's status stays.
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("t-args > /tmp/out"),
            (
                139,
                "t-args: note\nt-args: write error: No space left on device\n\
                 relay-sh: t-args: killed (page fault at 0x0, read, ip 0x401a2c)\n"
                    .into()
            )
        );
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_killed_program_is_reported`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        };
        if let Err(e) = ctx.finish() {
            message = format!("{name}: write error: {e}\n");
            status = 1;
        }
        self.transcript = ctx.transcript.take();
````

with:

````rust
        };
        if let Err(e) = ctx.finish() {
            // A program that did not exit keeps its report and status.
            if message.is_empty() {
                status = 1;
            }
            message.insert_str(0, &format!("{name}: write error: {e}\n"));
        }
        self.transcript = ctx.transcript.take();
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 125 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "shell: a killed program's report and status survive a write error"
````


### Task 20: NUC check 3 runs programs

Plan 2 is the first ring-3 code on the NUC, where the CPU (an i7-1260P) and its firmware differ from QEMU's (decision 13). `check3-a.sh` runs `t-args a 'b c' ''` (three arguments, the last one empty: the `\x20` keeps the trailing space of `[3] ` through the script parser, which trims its lines) and `t-fault null-read` (`relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x4…)`, and the script goes on). The recorded transcripts in `xtask/fixtures/checks/` get those lines by hand, by a command, since they hold escape bytes; the real ones replace them after the NUC check, as plan 1's did. `docs/hardware-test.md` names the new lines, the new command count (66), the `system` line with two programs and two new failures (a panic screen right after `+ t-args` or `+ t-fault`; `Exec format error` for `t-args`); the README says what milestone 2 does so far and lists `crates/elf`. Mutation checks: `\x20` dropped from the `[3]` line fails the test.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`

**Interfaces:**
- Consumes: Tasks 15 and 17.
- Produces: `check3-a.sh` with the programs' lines; the README and `docs/hardware-test.md` up to date.

- [ ] **Step 1: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#> ...

````

with:

````bash
#> ...

# Programs in ring 3 (milestone 2, plan 2): t-args prints its arguments,
# and a fault ends t-fault, not the kernel.
t-args a 'b c' ''
#> \[1\] a
#> \[2\] b c
#> \[3\]\x20
t-fault null-read
#> relay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)

````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 3: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
from a USB stick on an Intel NUC 12 Pro, shows a terminal over HDMI and
stores files on the stick's ext2 root filesystem.

````

with:

````markdown
from a USB stick on an Intel NUC 12 Pro, shows a terminal over HDMI and
stores files on the stick's ext2 root filesystem. Milestone 2 runs programs
from `/bin` in ring 3, each in its own address space: the shell runs a
name it does not know (`t-args a b`) as a program.

````

Replace:

````markdown
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `userland/` | User programs: `tests/` holds the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and packs `system.img` |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
````

with:

````markdown
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `crates/elf` | The rules a program's ELF file must follow; the kernel's `spawn` and xtask's build both check them |
| `userland/` | User programs: `tests/` holds the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and the kernel's rules, and packs `system.img` |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
````

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 4 replacements, top to bottom:

Replace:

````markdown
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`
   - `[ ok ] system: 1 program, ABI 1` (milestone 2: the programs of `/bin`
     from `\EFI\RELAY\system.img`; the count grows as later plans add
     programs)
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
````

with:

````markdown
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`
   - `[ ok ] system: 2 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
     add programs)
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
````

Replace:

````markdown
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), `ls -l /bin` (the programs of the system archive), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

with:

````markdown
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), `ls -l /bin` (the programs of the system archive),
   `t-args a 'b c' ''` and `t-fault null-read` (programs in ring 3: three
   arguments printed, then `relay-sh: t-fault: killed (page fault at 0x0,
   read, ip …)` and the script goes on), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

Replace:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 64 of 64 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

with:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 66 of 66 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

Replace:

````markdown
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …` | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
````

with:

````markdown
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …` | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| The panic screen just after `+ t-args` or `+ t-fault` | Entering ring 3, a system call or a fault in ring 3 goes wrong on this CPU, where QEMU's works | Photograph the panic screen: its vector, `rip`, `cr2` and registers say which |
| `relay-sh: t-args: Exec format error` | The kernel refused the program; `dmesg` shows `spawn /bin/t-args: <reason>` | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
````

- [ ] **Step 5: Put the programs' lines into the recorded NUC transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if 'root 19688' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ["+ t-args a 'b c' ''", '[1] a', '[2] b c', '[3] ', '+ t-fault null-read', 'relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x400205)']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 6: Put the programs' lines into the recorded QEMU transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.qemu.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if 'root 19688' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ["+ t-args a 'b c' ''", '[1] a', '[2] b c', '[3] ', '+ t-fault null-read', 'relay-sh: t-fault: killed (page fault at 0x0, read, ip 0x400205)']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 11 tests.

- [ ] **Step 8: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add README.md docs rootfs xtask
git commit -m "docs, checks: NUC check 3 runs t-args and t-fault in ring 3"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 26 scenario(s) passed`.

````bash
git push -u origin m2p2/faults
gh pr create --base main --head m2p2/faults --title "Milestone 2, plan 2: Faults, and ring 3 on the NUC" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 17–20: an exception in ring 3 kills the program (NMIs, double faults and machine checks still reach the panic screen), with its kind, address and instruction in the kernel log and the shell's `killed (…)` message and bash's status; `WaitStatus` says so in spec §11.1's words; `syscall` clears NT as well and the way back to the kernel loads the kernel's flags, so a program that sets NT or AC can no longer crash the kernel or switch SMAP off; `t-fault` and the `userfault` scenario; `check3-a.sh` runs `t-args` and `t-fault`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 3 of `docs/hardware-test.md`, run by the user with `cargo xtask flash --full` from this worktree; the result goes into the results log, and the recorded transcripts are replaced by the real ones
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p2/faults --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p2-faults
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
