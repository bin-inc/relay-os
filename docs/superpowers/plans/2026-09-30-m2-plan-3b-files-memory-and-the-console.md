# Milestone 2 · Plan 3b: Files, memory and the console — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Programs get every call of spec §7.3 but `proc_list` and `pipe`: they open, read, write and list files on any filesystem the kernel mounts, with an fd table of VFS files whose offsets their children share; they map memory and have a heap from `relay-rt`; they read the console a line at a time through a line discipline that echoes as keys are typed, or raw; they copy the console into files with tees; they learn the system's names and kernel log and restart or switch the machine off. A removal reaches every process, so no program reaches an inode a new file got. It ends with `cargo xtask ci` green and NUC check 3, by script and with three steps by hand, running the new programs on a stick written by `cargo xtask flash --full`.

**Architecture:** As in plans 1–3a, the logic is architecture-neutral and host-tested, and the glue is thin. The parts come first, each tested alone: the ABI's structs (`crates/relay-abi/src/{file,console,power,info}.rs`), a heap that grows by regions (`crates/heap`), the line discipline (`kernel/src/line.rs`), the tee stack (`kernel/src/tee.rs`), the VFS's record of removals and moves (`vfs::Change`), open files (`kernel/src/file.rs`, over any `Vfs`), and an address space's `mem_map` regions (`kernel/src/mm/space.rs`). The dispatcher (`kernel/src/syscall.rs` and `syscall/files.rs`) checks what programs pass and reaches their fds, the files and the console through its `Caller`, tested against a fake program (`syscall/testing.rs`) with a real fd table over a `MemFs`; `kernel/src/proc.rs` is the kernel's side, `kernel/src/tty.rs` the console's (the input queue with the discipline, tees). Nothing is architecture-specific but a TLB flush and the machine's name.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model), e2fsprogs 1.47, binutils' `readelf` and the host C library's error messages for tests.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§3.1, §5.1, §5.4, §6.1, §6.3–§6.5, §7, §8.1, §8.5, §11.1, §12, §13 step 3b, §16 item 4)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 3b of 6.

## In brief

- **Size.** 25 tasks in four code pull requests, plus this plan as PR 1. PR 2 is the parts, host-tested and not used yet (milestone 1's serial findings excepted); PR 3 the file calls; PR 4 memory; PR 5 the console, tees, `sys_info`, `power` and the NUC check. Plan 3b is one plan (decision 1).
- **A removal reaches every process** (decision 6). ext2 gives a freed inode to the next file at once, so another process's working directory or open file would reach a new file; the mount table records what each removal and move did, and the kernel applies it to every process: removed means gone (`ENOENT`), moved means the new path. Without it, the prototype's `t-files gone` wrote into the new file that got its removed file's inode.
- **The console's line discipline runs as keys come** (decision 9): what is typed in line mode is echoed at once, even while no program reads, and what a program did not read goes back to the shell, as bash sees it. Only the foreground group reads; any other process gets end of input at once.
- **Two changes to the spec's calls** (decisions 11 and 13): a tee's pop after a failed write returns that write's error, not always `EIO`, and `power` gains `POWER_FORCE` for milestone 1's `reboot -f` and `poweroff -f`.
- **The in-kernel shell keeps its transcript and outputs until plan 4** (decision 14); its commands read the console as fd 0, in line mode.
- **The prototype's review.** A fresh reviewer read the whole prototype, ran throwaway probes and scenarios under KVM on the NUC's CPU model (an i7-1260P), and found no critical, 2 important and 5 minor real defects, all but one in the console and the tees; six are fixed in a task of their own after the task they concern, with a test that fails first and a mutation check, and one is ruled. It found correct every check before its effect in the file calls, no overflow or unbounded allocation from what a program passes, the removals reaching every process with the locks in order, no lost wake-up of a console reader, tees written only where files can be written, and the `mem_map` regions, flushes and reserve:

| Finding (review) | Decision |
|---|---|
| Important: what is typed during a script is echoed again at the start of every later command, on the screen and in every tee | Fixed, Task 18 |
| Important: over a terminal that sends CR LF, the LF after the shell's line is an empty line for every command (a regression from plan 3a, serial only) | Fixed, Task 19 |
| Minor: a tee of a program that reads the console and writes nothing grows without bound from echo | Fixed, Task 23 |
| Minor: `read_dir`'s heap check understates what a directory's entries take (7.7 times its size measured, not 4) | Fixed, Task 8 |
| Minor: an Escape typed ahead swallows the key after it at the switch to line mode | Fixed, Task 20 |
| Minor: the in-kernel shell's prompt hangs once a program has left the console in line mode | Fixed, Task 21 |
| Minor: Backspace over a Tab echoes one column back, so the screen differs from the line read | Ruled: the line a program reads is right; erasing a Tab on the screen needs the column of every character the console wrote, which the line discipline does not see (Linux's tty follows all output for it) |
| Declined to judge: the in-kernel shell's redirection files and transcripts are nodes, which a removal does not reach | Ruled: decision 14; plan 4's `/bin/sh` holds them as fds |
| Declined to judge: echo from a tick scrolls the framebuffer inside the timer interrupt on the NUC | Ruled: the tick already polls the keyboard there; a line's echo is a character or three, and a scroll a few milliseconds at most; the NUC check's typing step shows it |
| Declined to judge: a big `mem_map` or `read` holds the CPU while it zeroes or copies | Ruled: spec §6.1, decisions 5 and 7; plan 4 polls the keyboard on the way back to ring 3 (roadmap) |

## Where this plan fits

Plan 3b implements the second half of spec §13 step 3. It builds on plan 3a: the process table, the dispatcher and its `Caller`, the fd table with the console and the in-kernel shell's outputs, the console's mode and foreground group, the kernel's mount table with a current directory per process, `UserSlice` and `UserStr`. It leaves plan 4 every call `/bin/sh` and its commands need, `relay-rt`'s heap, and tees for its transcripts (roadmap, "What plan 3b leaves for plan 4").

## Working conventions

- Plan 3b lands as **five pull requests** (table below). This plan, with the spec's §16 item 4 (and its §5.1 and §8.5 brought up to date) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. The last PR goes up as a **draft** until the user has run NUC check 3 and reported its results; the real transcripts then go into a commit of that same PR. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-3b-files-memory-and-the-console.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.

  One more comes as a command: a `python3` command that inserts lines into a recorded transcript (they hold escape bytes, which a markdown block cannot carry).
- Each task first adds its failing tests (unit tests in each file's test module; test programs under `userland/tests/`; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Two tasks have no failing run, and their introductions say why: Task 10 moves code and changes nothing a test sees, and Task 14 adds a test of what the code already does, with the mutation check that shows what it guards. The mutation checks the prototype ran are named in each task's introduction where a guard could pass vacuously.
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; it happened twice while this plan's prototype was made, under a mutant. Mutation checks run under an address-space limit and a timeout (`tmp/m2p3b/mutate.py`).
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `6df78bc` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2p3b/plan` | — | The spec's §16 item 4 and its §5.1 and §8.5 brought up to date, the roadmap's notes for plan 3b, this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p3b/parts` | 1–9 | The ABI's structs and flags; heap regions; milestone 1's serial findings; the line discipline; the tee stack; the record of removals; open files; `mem_map` regions | `lint`, `unit`, `e2e` |
| 3 | `m2p3b/files` | 10–14 | The file calls from ring 3, `t-files`; a removal reaches every process; a redirection's write error | `lint`, `unit`, `e2e` |
| 4 | `m2p3b/memory` | 15–16 | `mem_map` and `mem_unmap` from ring 3; `relay-rt`'s heap; `t-mem` | `lint`, `unit`, `e2e` |
| 5 | `m2p3b/console` | 17–25 | Console reads and calls, `t-read`; tees, `t-tee`; `sys_info` and `power`, `t-sys`; NUC check 3 (a draft until its results) | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 3b adds no third-party crate (`relay-rt` gains the workspace's `heap` and `spin`). The kernel has no dev-dependencies. New host crates are `#![cfg_attr(not(test), no_std)]` and go in both `members` and `default-members`; user packages go in `members` only and in `USER_PACKAGES`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail.
- Programs, and everything they pass to the kernel, are untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks), allocate without bound or loop forever. The kernel's heap panics when it runs out, so no kernel allocation is sized by a program without a bound or `mm::heap_room()`. A bad pointer is `EFAULT`. Nothing waits for ever. Panics are for kernel bugs only.
- Never hold a lock across a switch; `PROCS` comes before `MEMORY`, the input queue before the USB hosts, the tee stack before the mount table.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils and bash; the in-kernel shell names itself `relay-sh`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 4 in PR 1:

1. **Plan 3b is one plan** (§13), in five pull requests: this plan; the parts, host-tested and not used yet; the file calls; memory; the console, tees, `sys_info` and `power`, with a NUC check.
2. **Open files** (§5.4, §7.3). A file of the VFS open in the fd table is its node, what it was opened for and an offset, shared by the fds that share the open file (the ones `spawn` hands a child); each `open` makes its own, as on Linux. `open` takes the lowest free fd, and is `EMFILE` when all 32 are in use, before anything is opened or created. Its flags' errors are Linux's where they apply: neither `READ` nor `WRITE`, `TRUNCATE` or `APPEND` without `WRITE`, `EXCLUSIVE` without `CREATE`, or `DIRECTORY` with `CREATE`, is `EINVAL`; `CREATE` with `EXCLUSIVE` of what exists is `EEXIST`; a directory opened for writing, or a name to create that ends in `/`, is `EISDIR`; `DIRECTORY` of anything else is `ENOTDIR`; a read-only filesystem says `EROFS` when something would change. Reading or writing what an fd was not opened for is `EBADF`, reading a directory `EISDIR`. Symbolic links are never followed (milestone 1): one can be opened, and reading it is the filesystem's `EINVAL`.
3. **`seek`** may go past the end (a write there leaves a hole); before the start, past 2^63 − 1 or with an unknown whence it is `EINVAL`, and so it is on the console and the in-kernel shell's outputs (§7.3). A directory only goes back to its start, where `read_dir` begins again.
4. **`read_dir`** (§7.3) writes records of a 16-byte header (inode `u64`, the record's length `u16`, the name's length `u16`, the kind `u8`, 3 bytes reserved) and the name, padded to 8 bytes, which `relay_abi::file` writes and reads for the kernel and the programs alike. Entries come sorted by name, and the continuation token is the last name given, kept in the open file: an entry that is there throughout comes exactly once however the directory changes between calls. `EINVAL` if not even the next record fits, 0 after the last; `ENOMEM` for a directory whose size, ten times over, is more than the kernel's heap has room for (its heap panics when it runs out; an ext2 entry of 12 bytes takes up to 112 in memory while the list grows); at most 64 KiB of the buffer is used per call. The kind comes from `Vfs::entry_kind`: a name something is mounted on, `.` and `..` are directories.
5. **The other calls on files** (§7.3). `stat` never follows a link, so `NOFOLLOW` changes nothing; `touch` sets the times of a file that exists (programs create files with `open`); `readlink` cuts the target at the buffer's length, as Linux does; `getcwd` is `ERANGE` for a buffer that is too short and gives a removed directory the path it had, as the in-kernel shell's `pwd` does. `read` and `write` copy a page at a time: a read checks each page writable before it reads into it, so the offset never moves past what the program got, and stops at the end of the file; a write stops where the filesystem is full, and the next piece's `ENOSPC` ends it. A call is not preempted (§6.1), so a big read holds the CPU while it runs.
6. **A removal reaches every process** (§5.4; the roadmap's note from plan 3a). An inode a removal frees may go to the next file at once. The mount table records what each removal and move did (`vfs::Change`: a directory, or a file's last name, removed; a directory moved), and after every operation the kernel applies it: every other process's working directory follows (a removed one resolves nothing, a moved one takes its new path), and every open file and tee of a freed inode is gone, so everything but closing it is `ENOENT`, what the filesystem says of an inode it has freed. The prototype showed the danger: without it, a program wrote into the new file that got its removed file's inode.
7. **`mem_map` and `mem_unmap`** (§5.1, §7.3). Fresh zeroed read-write NX pages, the length rounded up to pages, first fit from `0x1000_0000_0000` to `0x7000_0000_0000`; regions that meet are one, and a process has at most 1024 (`ENOMEM` beyond, for an unmap that would cut one in two as well), so its list cannot fill the kernel's heap. A length of 0 is `EINVAL`; more than the frames above the 8 MiB reserve is `ENOMEM` before anything is mapped, and running out midway leaves nothing mapped. `mem_unmap` takes an aligned address and whole pages of one region (`EINVAL` otherwise) and flushes their TLB entries, all of them for more than 64 pages.
8. **The heap of `relay-rt`** (§8.1). `crates/heap` gains regions (`Heap::add`; regions that meet are one block). The allocator grows by the block's size, alignment and a slab, rounded up to whole mebibytes; when `mem_map` refuses, the allocator itself prints `<name>: out of memory` and exits with 134 (not the panic handler, whose status is 101). Nothing is given back to the kernel before the program ends.
9. **Reading the console** (§6.4, §6.5). Only the foreground group reads; any other process gets end of input at once, and a reader killed while it waits gets `EINTR`. In line mode the line discipline runs as keys come in (on the ticks that poll, in the idle task, and in reads), so what is typed is echoed at once, to the screen and the tees, even while no program reads. It works on keys: an escape sequence (`ESC [` up to its final byte) is one key and does nothing; `ESC` and any other byte are the Escape key and that key (nothing here sends Alt). Backspace erases a whole UTF-8 character (`\b \b`); Enter is CR, LF or CR LF; Ctrl-D on an empty line is end of input and on another hands the line over without its `\n`, as Linux does; other control characters do nothing; the lines waiting and the one typed hold 4096 bytes together, and Enter always fits. A read gets at most one line. In raw mode a read gets what was typed. Going to line mode hands the discipline what was typed ahead, without echoing again what it echoed before; going back to raw mode hands back what was typed and not read, as bash sees it, and a Ctrl-C nobody took as a raw Ctrl-C, which the shell's line editor and a running script see (plan 3a's ruling that such a Ctrl-C was swallowed no longer holds). A LF right after the CR that ended the shell's line is part of that Enter, for a terminal that sends CR LF.
10. **The console calls** (§7.3). `console_mode` takes raw (0) or line (1), `EINVAL` otherwise, and returns the previous one; `console_size` returns the columns in the low and the rows in the high 32 bits (80×25 without a console); `console_foreground` takes a group some process is in (`ESRCH` otherwise, 0 too). Any process may call them (single user, §1.3); a change wakes the blocked readers.
11. **Tees** (§6.5). `console_tee_push` takes an fd of a VFS file open for writing (`EBADF` otherwise, `EINVAL` for the console and the in-kernel shell's outputs); the tee holds the open file, so closing the fd keeps it. At most 4 (`EBUSY`). A tee gets what programs and the in-kernel shell write to the console and the line discipline's echo, not the kernel's own messages. Its copies are written once 4 KiB wait, in a process's context (a write or a read of the console), at every `sync` (the call's and the in-kernel shell's after each command) and when it is popped; the echo a tick makes only waits, 16 KiB at most, beyond which it is dropped, as a terminal drops what nobody reads. A failed write removes the tee from the copying and is logged; the owner's next pop returns that write's error (`ENOSPC`, `EIO`, `ENOENT`), not always `EIO` as §6.5 says, so the shell reports the real reason. A pop with no tee is `EINVAL`. A process's tees stop at its end and are written at the next flush, since a process may end in a fault or a tick, with interrupts off, where no file can be written.
12. **`sys_info`** (§7.3). `INFO_UNAME` fills `relay_abi::Uname`, four fields of 64 bytes padded with NULs (`Relay`, `relay`, the kernel's version, `x86_64`), `EINVAL` for a shorter buffer; `INFO_LOG` gives the newest bytes of the kernel log that fit (it holds at most `LOG_MAX`, 64 KiB).
13. **`power`** (§7.3) takes reboot (1) or poweroff (2) and the flag `POWER_FORCE` (added: milestone 1's `reboot -f` and `poweroff -f` go ahead when the filesystems cannot be shut down cleanly). It writes the tees, shuts the filesystems down and restarts or switches off; if the shutdown fails it returns the error, unless forced, and the machine stays up, as milestone 1's `unplug` scenario requires. In test mode poweroff makes QEMU exit, as the shell's does.
14. **The in-kernel shell until plan 4** (§16 item 3). It keeps its own `Transcript` and its outputs (`File::ShellOutput`); its commands get the console as fd 0, in line mode while it waits for them. At its prompt it reads raw bytes from the input queue itself, taking the console back (raw mode, its own group) before each one, whatever mode a program left it in. Its redirection files and transcripts are nodes, not open files, so a removal does not reach them; plan 4's `/bin/sh` holds them as fds, which it does.
15. **Milestone 1's serial findings.** An `ESC` over COM1 with nothing after it for 50 ms (TSC time) goes in as the Escape key; any Ctrl-C, from the keyboard too, ends a half-arrived serial sequence.
16. **Error numbers** (§7.2). `vfs::Errno` gains `EMFILE` and `ERANGE` (34, for `getcwd`); `relay_abi::errno::name` names each number for test programs.
17. **Tests** (§8.5, §12). Test programs `t-files`, `t-mem`, `t-read`, `t-tee` and `t-sys`; scenarios `files`, `memory`, `console`, `tees` and `sysinfo`; `diskfull` and `unplug` gain a program's write errors, a tee on a full disk and `power` on an unplugged stick. The e2e runner gains `type TEXT` (keys without Enter, for Ctrl-D), `send-crlf TEXT` and `poweroff COMMAND`.
18. **Check scripts** (§12.4). `check3-a.sh` runs `t-files basic`, `dir`, `cwd` and `gone`, `t-mem map` and `grow 64`, `t-tee end`, `t-sys uname` and `t-read apart` (84 commands); a third step by hand types into `t-read` on the K120, with Backspace and Ctrl-D. The recorded transcripts get those lines by hand until plan 3b's NUC check records real ones.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A program that misuses the new calls:** a path over 4096 bytes or at a bad address; a buffer that runs off its page, into read-only memory or past the lower half, for `read`, `fstat`, `stat`, `read_dir`, `readlink`, `getcwd` and `sys_info`; an fd that is not open, or opened for the other direction; a 33rd `open`; contradictory `open` flags; `seek` before the start, past 2^63 − 1 or with an unknown whence; a `read_dir` buffer too small for one record, or a huge one; `mem_map` of 0 bytes or of more than there is, `mem_unmap` of pages it did not map or half a page in; an unknown console mode, a foreground group nobody is in, a tee on the console, a read-only fd or a closed one, a fifth tee; a `power` kind or flag that does not exist. Expected: `ENAMETOOLONG`, `EFAULT`, `EBADF`, `EMFILE`, `EINVAL`, `ENOMEM`, `ESRCH`, `EBUSY` or `ERANGE`, with nothing created, changed, read from the console or taken from a file's offset, and never a kernel panic, a hang or a heap run out. Tests: `open_s_flags_are_checked`, `what_a_file_was_not_opened_for_is_ebadf`, `seek_goes_anywhere_from_the_start_on`, `read_dir_needs_room_for_one_record_and_the_heap_for_the_directory` (Task 7), `the_heap_check_covers_a_directory_of_the_smallest_entries` (Task 8), `mem_map_refuses_what_it_cannot_give`, `mem_unmap_takes_whole_pages_of_earlier_maps_only`, `at_most_1024_regions` (Task 9), `open_gives_the_lowest_free_fd_and_close_frees_it`, `open_refuses_a_bad_path_or_flags`, `a_read_across_pages_copies_them_all_and_stops_at_a_bad_one`, `what_an_fd_was_not_opened_for_is_ebadf`, `seek_has_no_meaning_on_the_console`, `fstat_fills_in_the_file_s_status` (Task 11), `stat_names_a_file_by_its_path_and_never_follows_a_link`, `read_dir_copies_out_records_and_goes_on_where_it_stopped`, `the_calls_on_a_path_do_what_their_vfs_operation_does`, `readlink_gives_the_target_cut_at_the_buffer`, `chdir_and_getcwd` (Task 12), `mem_map_gives_whole_pages_and_mem_unmap_takes_them_back` and the `memory` scenario (Task 15), `a_console_read_copies_out_what_the_console_gives`, `the_console_s_mode_size_and_foreground` (Task 17), `tees_are_pushed_by_fd_and_popped`, `at_most_4_tees_and_each_gets_a_copy` (Tasks 5 and 22), `sys_info_names_the_system_and_gives_the_newest_of_the_log`, `power_returns_only_the_error_that_kept_the_machine_up` (Task 24).
2. **Files that another process removes or moves while a program uses them:** its working directory removed or moved (or a directory above it), an open file removed and a new file made that gets its inode, a tee's file removed, a directory changed between two `read_dir` calls. Expected: a removed working directory and a removed file are gone for everyone (`ENOENT`), a moved directory keeps working under its new path, the new file and directory keep what they hold, and every entry that is there throughout is listed exactly once. Tests: `a_removal_reaches_a_current_directory_put_aside`, `a_move_takes_a_current_directory_put_aside_along`, `the_last_name_of_a_file_going_frees_it`, `a_file_with_another_name_lives_on` (Task 6), `an_entry_there_throughout_comes_once_however_the_directory_changes`, `a_file_marked_gone_is_enoent_for_everything` (Task 7), `the_open_files_of_a_freed_inode_are_gone` and `t-files gone` in the `files` scenario (Task 13), `t-tee gone` in the `tees` scenario (Task 22).
3. **Typing at the console, whether a program reads or not:** Backspace over a character of two bytes and at the start of a line, Enter over COM1 as CR LF (also the one that ends the shell's line), Ctrl-D on an empty line and on a started one, arrows and Delete in line mode, a Ctrl-C with half an arrow key over COM1, more than 4096 bytes typed, Escape alone over COM1, typing while a program spins or a script runs and the prompt afterwards, an Escape typed ahead, a program that leaves the console in line mode, a program outside the console's group that reads, raw mode. Expected: echo once, as typed, the line a program reads is what the screen shows, end of input exactly where Ctrl-D says, nothing held for ever or without bound, the shell gets what the program did not read, and its prompt always reads. Tests: `backspace_removes_the_last_character_whole`, `a_terminal_s_cr_lf_is_one_enter_and_a_lone_lf_is_one_too`, `ctrl_d_ends_input_on_an_empty_line_and_hands_over_a_started_one`, `what_waits_is_bounded_and_enter_still_gets_in` (Task 4), `an_escape_alone_over_serial_goes_in_once_nothing_follows`, `a_ctrl_c_from_the_keyboard_ends_a_serial_sequence` (Task 3), `in_line_mode_keys_are_echoed_and_read_a_line_at_a_time`, `an_escape_sequence_is_one_key_in_line_mode`, `what_was_typed_ahead_goes_to_the_line_discipline_and_back`, `a_ctrl_c_nobody_took_is_raw_input_again` and the `console` scenario (Task 17), `what_was_typed_ahead_is_echoed_once` (Task 18), `the_lf_of_a_cr_lf_the_shell_read_is_not_a_line` and `send-crlf t-read` (Task 19), `an_escape_typed_ahead_keeps_the_key_after_it` (Task 20), `t-read leave` in the `console` scenario (Task 21), the step by hand in `check3-a`'s instructions (Task 25).
4. **Memory a program asks for:** a big map and many small ones, part of a map given back and the hole filled again, a page read after it was given back (one page and many), a heap that grows to 64 MiB, a program that takes memory until there is none, and what is left afterwards. Expected: first fit from the area's start, `ENOMEM` beyond the reserve or 1024 regions, a page given back faults at once, `<name>: out of memory` with status 134, and every frame back when the program ends. Tests: `mem_map_fills_the_area_from_its_start_and_merges_what_meets`, `at_most_1024_regions` (Task 9), `a_region_added_later_serves_what_the_first_cannot`, `regions_that_meet_are_one` (Task 2), the `memory` scenario with `t-mem unmapped` and `unmapped-many` (Task 15), `a_big_block_gets_a_big_enough_step`, `steps_that_meet_merge_into_one_region` and `t-mem grow` and `oom` in the `memory` scenario (Task 16), `frames lost: 0` in `check3-a.sh` (Task 25).
5. **A disk that fills up or goes away, and tees that are not written:** writes to a full disk through a program's own file, through its redirected standard output, and through a tee; a tee of a program that only reads the console; `sync` and `power` with the stick unplugged; a tee of a process that ended in a fault. Expected: a short write then `ENOSPC`, the shell's `write error` line for the redirection, the tee's pop says `ENOSPC`, `power` returns `EIO` and the machine stays up, a tee is written once more after its process ended and then gets nothing, and a reader's tee is written as it reads, its echo never more than 16 KiB. Tests: `a_full_filesystem_gives_a_short_write_then_enospc` (Task 7), `a_full_disk_is_a_short_write_then_enospc` (Task 11), `t-files full` in the `diskfull` scenario (Task 14), `a_failed_write_removes_the_tee_and_its_pop_says_why`, `a_process_s_tees_end_with_it_and_are_written_at_the_next_flush` (Task 5), `t-tee end` and `t-tee full` in the `tees` and `diskfull` scenarios (Task 22), `echo_alone_waits_no_more_than_its_limit` and `t-tee typed` (Task 23), `t-sys poweroff` in the `unplug` scenario (Task 24).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/relay-abi/src/{file,console,power,info,errno}.rs` | Open flags, `Stat`, `StatFs`, `read_dir`'s records; the console's modes; `power`'s kinds; `Uname` and the log; `ERANGE` and error names |
| `crates/heap/src/lib.rs` | A heap over regions |
| `crates/vfs/src/{errno,mount}.rs` | `EMFILE`, `ERANGE`; `Change`, what a removal or a move did; `Vfs::entry_kind` |
| `crates/relay-rt/src/{sys,allocator,lib}.rs` | A wrapper for every new call; the programs' heap |
| `kernel/src/{line,tee,file}.rs` | The line discipline; the tee stack; open files |
| `kernel/src/{fd,input,tty,mounts,proc,session,power}.rs`, `kernel/src/proc/table.rs` | VFS files in the fd table; the discipline in the input queue; reading the console and tees; changes reaching every process; the calls' kernel side |
| `kernel/src/syscall.rs`, `kernel/src/syscall/{files,testing}.rs` | The dispatcher; the file calls; the fake program |
| `kernel/src/mm/{space,mod}.rs`, `kernel/src/arch/mod.rs` | `mem_map` regions; the room above the reserve and the TLB flush; the machine's name |
| `userland/tests/src/bin/{t-files,t-mem,t-read,t-tee,t-sys}.rs` | The new test programs |
| `tests/e2e/{files,memory,console,tees,sysinfo,diskfull,unplug,system}.txt`, `xtask/src/{e2e,keys}.rs` | The new scenarios; `type` and `poweroff COMMAND` |
| `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/check3-a.{qemu,nuc}.log` | NUC check 3 runs plan 3b's programs |
| `README.md`, `docs/hardware-test.md` | What milestone 2 does now; check 3's new lines, steps by hand and failures |

---

## PR 1: The spec's revisions, the roadmap and this plan

The spec's §16 item 4 (with its §5.1 and §8.5 brought up to date), the roadmap's notes for plan 3b ("What plan 3b leaves for plan 4"), and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec and roadmap changes are the prototype's first commit, `docs: the spec's revisions from planning milestone 2's plan 3b, and the roadmap's notes for plan 3b`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3b/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m2p3b/proto p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-09-30-m2-plan-3b-files-memory-and-the-console.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-30-m2-plan-3b-files-memory-and-the-console.md
git commit -m "docs: plan 3b of milestone 2, files, memory and the console"
cargo xtask lint
git push -u origin m2p3b/plan
gh pr create --base main --head m2p3b/plan --title "Milestone 2, plan 3b: the spec's revisions, the roadmap and the plan" --body-file - <<'EOF2'
## What

The implementation plan of milestone 2's plan 3b ("files, memory and the console"), the spec's §16 item 4 with its decisions (among them: a removal reaches every process's working directory and open files, so none reaches an inode a new file got; the console's line discipline echoes as keys are typed and hands what a program did not read back to the shell; a tee's pop returns the failed write's own error; `power` gains `POWER_FORCE`), the spec's §5.1 and §8.5 brought up to date (plan 3a's final review), and the roadmap's notes for plan 4.

## How it was tested

- [x] Every task of plan 3b was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2p3b/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m2p3b/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-plan` and continue with PR 2.

---

## PR 2: The parts (Tasks 1–9)

All host-tested, and nothing the kernel runs uses it yet, but for milestone 1's two serial findings: the ABI's structs and flags for the new calls, a heap that grows by regions, the serial fixes, the line discipline, the tee stack, the record of removals and moves, open files in the fd table, and an address space's `mem_map` regions.

Branch `m2p3b/parts`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-parts`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3b/parts /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-parts origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-parts
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3b/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3b/parts /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-parts m2p3b/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3b/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: What the file, console, `sys_info` and `power` calls pass; `EMFILE` and `ERANGE`

The calls plan 3b brings pass what the ABI does not have yet (spec §7.3): `open`'s flags, `seek`'s whence and `stat`'s `NOFOLLOW` (`relay_abi::file`); `Stat` and `StatFs`, what `stat`, `fstat` and `statfs` fill in; the records `read_dir` writes (a 16-byte `DirEntry` header and the name, padded to 8 bytes, decision 4), which `put_dir_entry` writes for the kernel and `dir_entries` reads for the programs, so the two cannot disagree; the console's modes and `console_size`'s packed result (`relay_abi::console`); `sys_info`'s names (`Uname`) and kernel log (`INFO_UNAME`, `INFO_LOG`); `power`'s kinds and `POWER_FORCE` (`relay_abi::power`, decision 13). Each struct is `#[repr(C)]` with fixed-width fields and no padding, and a test checks its size and every field offset, and that `to_bytes` is the struct's memory (§7.1). `vfs::Errno` gains `EMFILE` and `ERANGE` (decision 16), checked against the host C library as the others are. Mutation checks: records padded to 4 bytes, and `dir_entries` trusting a length shorter than its name, fail a test.

**Files:**
- Create: `crates/relay-abi/src/console.rs`
- Modify: `crates/relay-abi/src/errno.rs`
- Create: `crates/relay-abi/src/file.rs`
- Modify: `crates/relay-abi/src/info.rs`
- Modify: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-abi/src/power.rs`
- Modify: `crates/vfs/src/errno.rs`

**Interfaces:**
- Consumes: plan 3a's `relay_abi` and `vfs::Errno`.
- Produces: `relay_abi::file::{OPEN_READ, OPEN_WRITE, OPEN_CREATE, OPEN_TRUNCATE, OPEN_APPEND, OPEN_EXCLUSIVE, OPEN_DIRECTORY, OPEN_FLAGS, SEEK_START, SEEK_CURRENT, SEEK_END, STAT_NOFOLLOW, KIND_*, Stat, StatFs, DirEntry {ino, len, name_len, kind, reserved}, DirEntry::record_len(usize) -> usize, put_dir_entry(&mut [u8], ino, kind, name) -> Option<usize>, DirRecord {ino, kind, name}, dir_entries(&[u8]) -> impl Iterator<Item = DirRecord>}` (`Stat::to_bytes`, `StatFs::to_bytes`); `relay_abi::console::{MODE_RAW, MODE_LINE, size_result(u32, u32) -> u64, size_of_result(u64) -> (u32, u32)}`; `relay_abi::info::{INFO_UNAME, INFO_LOG, LOG_MAX, UNAME_FIELD, Uname {sysname, nodename, release, machine}}` (`Uname::{new, name, to_bytes}`); `relay_abi::power::{POWER_REBOOT, POWER_POWEROFF, POWER_FORCE}`; `relay_abi::errno::ERANGE`; `vfs::Errno::{EMFILE, ERANGE}`.

- [ ] **Step 1: Write the failing tests for `crates/relay-abi/src/console.rs`**

Create `crates/relay-abi/src/console.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_sizes() {
        assert_eq!((MODE_RAW, MODE_LINE), (0, 1));
        assert_eq!(size_result(120, 33), 120 | 33 << 32);
        assert_eq!(size_of_result(size_result(120, 33)), (120, 33));
        assert_eq!(size_of_result(size_result(u32::MAX, 1)), (u32::MAX, 1));
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/relay-abi/src/errno.rs`**

In `crates/relay-abi/src/errno.rs`, replace:

````rust
            ("EPIPE", EPIPE),
            ("ENAMETOOLONG", ENAMETOOLONG),
````

with:

````rust
            ("EPIPE", EPIPE),
            ("ERANGE", ERANGE),
            ("ENAMETOOLONG", ENAMETOOLONG),
````

- [ ] **Step 3: Write the failing tests for `crates/relay-abi/src/file.rs`**

Create `crates/relay-abi/src/file.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layouts_are_fixed() {
        assert_eq!(Stat::SIZE, 72);
        assert_eq!(offset_of!(Stat, ino), 0);
        assert_eq!(offset_of!(Stat, size), 8);
        assert_eq!(offset_of!(Stat, blocks), 16);
        assert_eq!(offset_of!(Stat, atime), 24);
        assert_eq!(offset_of!(Stat, mtime), 32);
        assert_eq!(offset_of!(Stat, ctime), 40);
        assert_eq!(offset_of!(Stat, kind), 48);
        assert_eq!(offset_of!(Stat, perm), 52);
        assert_eq!(offset_of!(Stat, nlink), 56);
        assert_eq!(offset_of!(Stat, uid), 60);
        assert_eq!(offset_of!(Stat, gid), 64);
        assert_eq!(offset_of!(Stat, block_size), 68);
        assert_eq!(StatFs::SIZE, 48);
        assert_eq!(offset_of!(StatFs, block_size), 0);
        assert_eq!(offset_of!(StatFs, blocks), 8);
        assert_eq!(offset_of!(StatFs, free_blocks), 16);
        assert_eq!(offset_of!(StatFs, avail_blocks), 24);
        assert_eq!(offset_of!(StatFs, files), 32);
        assert_eq!(offset_of!(StatFs, free_files), 40);
        assert_eq!(size_of::<DirEntry>(), 16);
        assert_eq!(offset_of!(DirEntry, ino), 0);
        assert_eq!(offset_of!(DirEntry, len), 8);
        assert_eq!(offset_of!(DirEntry, name_len), 10);
        assert_eq!(offset_of!(DirEntry, kind), 12);
        assert_eq!(offset_of!(DirEntry, reserved), 13);
    }

    #[test]
    fn the_numbers_are_fixed() {
        let flags = [
            OPEN_READ,
            OPEN_WRITE,
            OPEN_CREATE,
            OPEN_TRUNCATE,
            OPEN_APPEND,
            OPEN_EXCLUSIVE,
            OPEN_DIRECTORY,
        ];
        assert_eq!(flags, [1, 2, 4, 8, 16, 32, 64]);
        assert_eq!(flags.iter().fold(0, |a, f| a | f), OPEN_FLAGS);
        assert_eq!((SEEK_START, SEEK_CURRENT, SEEK_END), (0, 1, 2));
        assert_eq!(STAT_NOFOLLOW, 1);
        assert_eq!(
            [
                KIND_UNKNOWN,
                KIND_REGULAR,
                KIND_DIRECTORY,
                KIND_SYMLINK,
                KIND_CHAR_DEVICE,
                KIND_BLOCK_DEVICE,
                KIND_FIFO,
                KIND_SOCKET
            ],
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn a_struct_s_bytes_are_its_memory() {
        let s = Stat {
            ino: 1,
            size: 2,
            blocks: 3,
            atime: 4,
            mtime: 5,
            ctime: 6,
            kind: 7,
            perm: 8,
            nlink: 9,
            uid: 10,
            gid: 11,
            block_size: 12,
        };
        // SAFETY: both are `repr(C)` of integers with no padding.
        let mem: [u8; Stat::SIZE] = unsafe { core::mem::transmute(s) };
        assert_eq!(s.to_bytes(), mem);
        let f = StatFs {
            block_size: 1,
            blocks: 2,
            free_blocks: 3,
            avail_blocks: 4,
            files: 5,
            free_files: 6,
        };
        let mem: [u8; StatFs::SIZE] = unsafe { core::mem::transmute(f) };
        assert_eq!(f.to_bytes(), mem);
    }

    #[test]
    fn records_are_padded_to_8_bytes_and_read_back() {
        let mut buf = [0xAAu8; 64];
        assert_eq!(put_dir_entry(&mut buf, 7, KIND_REGULAR, b"a"), Some(24));
        assert_eq!(buf[17..24], [0; 7], "padded with zeros");
        let n = put_dir_entry(&mut buf[24..], 1 << 40, KIND_DIRECTORY, b"12345678");
        assert_eq!(n, Some(24), "16 + 8 needs no padding");
        let got: Vec<_> = dir_entries(&buf[..48]).collect();
        assert_eq!(
            got,
            [
                DirRecord {
                    ino: 7,
                    kind: KIND_REGULAR,
                    name: b"a"
                },
                DirRecord {
                    ino: 1 << 40,
                    kind: KIND_DIRECTORY,
                    name: b"12345678"
                }
            ]
        );
        // The header as the struct: the kernel and the programs agree.
        let head = DirEntry {
            ino: 7,
            len: 24,
            name_len: 1,
            kind: KIND_REGULAR,
            reserved: [0; 3],
        };
        let mem: [u8; 16] = unsafe { core::mem::transmute(head) };
        assert_eq!(buf[..16], mem);
    }

    #[test]
    fn a_record_that_does_not_fit_is_not_written() {
        let mut buf = [0xAAu8; 23];
        assert_eq!(put_dir_entry(&mut buf, 7, KIND_REGULAR, b"a"), None);
        assert_eq!(buf, [0xAA; 23], "nothing written");
        assert_eq!(put_dir_entry(&mut [0; 16], 1, 0, b""), Some(16));
        let long = vec![b'x'; 70_000];
        assert_eq!(put_dir_entry(&mut vec![0; 80_000], 1, 0, &long), None);
    }

    #[test]
    fn records_that_do_not_hold_together_end_the_list() {
        let mut buf = [0u8; 48];
        put_dir_entry(&mut buf, 1, KIND_REGULAR, b"one").unwrap();
        put_dir_entry(&mut buf[24..], 2, KIND_REGULAR, b"two").unwrap();
        // The second record's length says more than there is.
        buf[32..34].copy_from_slice(&200u16.to_ne_bytes());
        assert_eq!(dir_entries(&buf).count(), 1);
        // A length shorter than its name.
        buf[8..10].copy_from_slice(&8u16.to_ne_bytes());
        assert_eq!(dir_entries(&buf).count(), 0);
        assert_eq!(dir_entries(&buf[..10]).count(), 0, "half a header");
        assert_eq!(dir_entries(&[]).count(), 0);
    }
}
````

- [ ] **Step 4: Add the failing tests to `crates/relay-abi/src/info.rs`**

In `crates/relay-abi/src/info.rs`, replace:

````rust
        assert_eq!(offset_of!(MemInfo, heap_used), 24);
        assert_eq!(INFO_MEMORY, 1);
    }
````

with:

````rust
        assert_eq!(offset_of!(MemInfo, heap_used), 24);
        assert_eq!((INFO_MEMORY, INFO_UNAME, INFO_LOG), (1, 2, 3));
        assert_eq!(Uname::SIZE, 256);
        assert_eq!(offset_of!(Uname, sysname), 0);
        assert_eq!(offset_of!(Uname, nodename), 64);
        assert_eq!(offset_of!(Uname, release), 128);
        assert_eq!(offset_of!(Uname, machine), 192);
    }

    #[test]
    fn uname_s_fields_are_padded_names() {
        let u = Uname::new(b"Relay", b"relay", b"0.2.0", &[b'x'; 70]);
        assert_eq!(Uname::name(&u.sysname), b"Relay");
        assert_eq!(Uname::name(&u.release), b"0.2.0");
        assert_eq!(Uname::name(&u.machine), &[b'x'; 64], "cut at 64 bytes");
        // SAFETY: `repr(C)` of byte arrays, no padding.
        let mem: [u8; Uname::SIZE] = unsafe { core::mem::transmute(u) };
        assert_eq!(u.to_bytes(), mem);
        assert_eq!(mem[64..70], *b"relay\0");
    }
````

- [ ] **Step 5: Declare the new module in `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
mod call;
pub mod errno;
pub mod info;
mod result;
````

with:

````rust
mod call;
pub mod console;
pub mod errno;
pub mod file;
pub mod info;
pub mod power;
mod result;
````

- [ ] **Step 6: Write the failing tests for `crates/relay-abi/src/power.rs`**

Create `crates/relay-abi/src/power.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_fixed() {
        assert_eq!((POWER_REBOOT, POWER_POWEROFF, POWER_FORCE), (1, 2, 1));
    }
}
````

- [ ] **Step 7: Add the failing tests to `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 24] = [
        Errno::ENOENT,
````

with:

````rust
    /// Every variant, so the tests below cover each one.
    const ALL: [Errno; 26] = [
        Errno::ENOENT,
````

Replace:

````rust
        Errno::EINTR,
    ];
````

with:

````rust
        Errno::EINTR,
        Errno::EMFILE,
        Errno::ERANGE,
    ];
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-abi -p vfs`

Expected: FAIL: compile errors such as `` cannot find value `MODE_RAW` in this scope ``; `` cannot find value `MODE_LINE` in this scope ``.

- [ ] **Step 9: Implement `crates/relay-abi/src/console.rs`**

Insert this at the top of `crates/relay-abi/src/console.rs`, above `#[cfg(test)]`:

````rust
//! What the console calls take (spec §6.4, §6.5, §7.3).

/// `console_mode`'s modes: bytes as they are typed, or a line at a time
/// with the line discipline of spec §6.5.
pub const MODE_RAW: u32 = 0;
pub const MODE_LINE: u32 = 1;

/// `console_size`'s result: the columns in the low 32 bits, the rows in the
/// high 32.
pub const fn size_result(columns: u32, rows: u32) -> u64 {
    columns as u64 | (rows as u64) << 32
}

/// The columns and rows in `console_size`'s result.
pub const fn size_of_result(r: u64) -> (u32, u32) {
    (r as u32, (r >> 32) as u32)
}

````

- [ ] **Step 10: Change `crates/relay-abi/src/errno.rs`**

In `crates/relay-abi/src/errno.rs`, replace:

````rust
pub const EPIPE: u16 = 32;
pub const ENAMETOOLONG: u16 = 36;
````

with:

````rust
pub const EPIPE: u16 = 32;
pub const ERANGE: u16 = 34;
pub const ENAMETOOLONG: u16 = 36;
````

- [ ] **Step 11: Implement `crates/relay-abi/src/file.rs`**

Insert this at the top of `crates/relay-abi/src/file.rs`, above `#[cfg(test)]`:

````rust
//! What the file calls pass (spec §7.3): `open`'s flags, `seek`'s whence,
//! `stat`'s flag, what `stat`, `fstat` and `statfs` fill in, and the
//! records `read_dir` writes.

/// `open`'s flags: read, write, or both.
pub const OPEN_READ: u32 = 1;
pub const OPEN_WRITE: u32 = 2;
/// Create the file if it does not exist.
pub const OPEN_CREATE: u32 = 4;
/// Empty the file (only with [`OPEN_WRITE`]).
pub const OPEN_TRUNCATE: u32 = 8;
/// Every write goes to the end (only with [`OPEN_WRITE`]).
pub const OPEN_APPEND: u32 = 16;
/// With [`OPEN_CREATE`]: `EEXIST` if the file exists.
pub const OPEN_EXCLUSIVE: u32 = 32;
/// `ENOTDIR` unless the path is a directory.
pub const OPEN_DIRECTORY: u32 = 64;
/// Every flag there is.
pub const OPEN_FLAGS: u32 = 127;

/// `seek`'s whence: from the start, from the current offset, from the end.
pub const SEEK_START: u32 = 0;
pub const SEEK_CURRENT: u32 = 1;
pub const SEEK_END: u32 = 2;

/// `stat`'s flag: a symbolic link's own status, not its target's.
pub const STAT_NOFOLLOW: u32 = 1;

/// What kind of file a [`Stat`] or a directory entry is.
pub const KIND_UNKNOWN: u8 = 0;
pub const KIND_REGULAR: u8 = 1;
pub const KIND_DIRECTORY: u8 = 2;
pub const KIND_SYMLINK: u8 = 3;
pub const KIND_CHAR_DEVICE: u8 = 4;
pub const KIND_BLOCK_DEVICE: u8 = 5;
pub const KIND_FIFO: u8 = 6;
pub const KIND_SOCKET: u8 = 7;

/// `stat`'s and `fstat`'s answer, `#[repr(C)]` with no padding. Times are
/// seconds since 1970, UTC.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub ino: u64,
    pub size: u64,
    /// Space used, in 512-byte units.
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    /// One of the `KIND_*` numbers.
    pub kind: u32,
    /// The permission bits, with set-user-ID, set-group-ID and sticky.
    pub perm: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub block_size: u32,
}

impl Stat {
    pub const SIZE: usize = core::mem::size_of::<Stat>();

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; Stat::SIZE] {
        let mut b = [0u8; Stat::SIZE];
        let longs = [
            self.ino,
            self.size,
            self.blocks,
            self.atime,
            self.mtime,
            self.ctime,
        ];
        for (i, v) in longs.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
        }
        let words = [
            self.kind,
            self.perm,
            self.nlink,
            self.uid,
            self.gid,
            self.block_size,
        ];
        for (i, v) in words.iter().enumerate() {
            b[48 + 4 * i..52 + 4 * i].copy_from_slice(&v.to_ne_bytes());
        }
        b
    }
}

/// `statfs`'s answer, `#[repr(C)]` with no padding. Counts are in
/// `block_size` units.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatFs {
    pub block_size: u64,
    pub blocks: u64,
    pub free_blocks: u64,
    /// Free blocks an unprivileged user could use.
    pub avail_blocks: u64,
    pub files: u64,
    pub free_files: u64,
}

impl StatFs {
    pub const SIZE: usize = core::mem::size_of::<StatFs>();

    pub fn to_bytes(&self) -> [u8; StatFs::SIZE] {
        let mut b = [0u8; StatFs::SIZE];
        let longs = [
            self.block_size,
            self.blocks,
            self.free_blocks,
            self.avail_blocks,
            self.files,
            self.free_files,
        ];
        for (i, v) in longs.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&v.to_ne_bytes());
        }
        b
    }
}

/// The header of each record `read_dir` writes, `#[repr(C)]` with no
/// padding. The name follows it, and the record is padded with zeros to
/// a multiple of 8 bytes, which `len` includes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirEntry {
    pub ino: u64,
    /// The whole record's length.
    pub len: u16,
    pub name_len: u16,
    /// One of the `KIND_*` numbers.
    pub kind: u8,
    pub reserved: [u8; 3],
}

impl DirEntry {
    pub const SIZE: usize = core::mem::size_of::<DirEntry>();

    /// The length of the record for a name of `name_len` bytes.
    pub const fn record_len(name_len: usize) -> usize {
        (DirEntry::SIZE + name_len).next_multiple_of(8)
    }
}

/// Writes the record for `name` at the start of `buf`: how many bytes it
/// took, or `None` (and nothing written) if it does not fit or the name is
/// too long for a record.
pub fn put_dir_entry(buf: &mut [u8], ino: u64, kind: u8, name: &[u8]) -> Option<usize> {
    let len = DirEntry::record_len(name.len());
    let (Ok(len16), Ok(name_len)) = (u16::try_from(len), u16::try_from(name.len())) else {
        return None;
    };
    let rec = buf.get_mut(..len)?;
    rec.fill(0);
    rec[..8].copy_from_slice(&ino.to_ne_bytes());
    rec[8..10].copy_from_slice(&len16.to_ne_bytes());
    rec[10..12].copy_from_slice(&name_len.to_ne_bytes());
    rec[12] = kind;
    rec[DirEntry::SIZE..DirEntry::SIZE + name.len()].copy_from_slice(name);
    Some(len)
}

/// One record `read_dir` wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirRecord<'a> {
    pub ino: u64,
    pub kind: u8,
    pub name: &'a [u8],
}

/// The records in the bytes `read_dir` returned. A record that does not
/// hold together ends them.
pub fn dir_entries(buf: &[u8]) -> impl Iterator<Item = DirRecord<'_>> {
    let mut rest = buf;
    core::iter::from_fn(move || {
        let head = rest.get(..DirEntry::SIZE)?;
        let ino = u64::from_ne_bytes(head[..8].try_into().ok()?);
        let len = usize::from(u16::from_ne_bytes([head[8], head[9]]));
        let name_len = usize::from(u16::from_ne_bytes([head[10], head[11]]));
        if len < DirEntry::record_len(name_len) || len > rest.len() {
            return None;
        }
        let rec = DirRecord {
            ino,
            kind: head[12],
            name: &rest[DirEntry::SIZE..DirEntry::SIZE + name_len],
        };
        rest = &rest[len..];
        Some(rec)
    })
}

````

- [ ] **Step 12: Change `crates/relay-abi/src/info.rs`**

In `crates/relay-abi/src/info.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const INFO_MEMORY: u32 = 1;

````

with:

````rust
pub const INFO_MEMORY: u32 = 1;
/// `sys_info`'s kind for the system's names ([`Uname`]).
pub const INFO_UNAME: u32 = 2;
/// `sys_info`'s kind for the kernel log (for `dmesg`): its newest bytes,
/// as many as the buffer holds. The log keeps at most [`LOG_MAX`].
pub const INFO_LOG: u32 = 3;
/// The most bytes the kernel log holds.
pub const LOG_MAX: usize = 64 * 1024;

````

Replace:

````rust
    pub heap_used: u64,
}
````

with:

````rust
    pub heap_used: u64,
}

/// The length of each of [`Uname`]'s fields.
pub const UNAME_FIELD: usize = 64;

/// What `uname` prints, `#[repr(C)]` with no padding: each field a name,
/// padded with NULs.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uname {
    /// `Relay`.
    pub sysname: [u8; UNAME_FIELD],
    /// `relay`.
    pub nodename: [u8; UNAME_FIELD],
    /// The kernel's version, `0.2.0`.
    pub release: [u8; UNAME_FIELD],
    /// The machine, `x86_64`.
    pub machine: [u8; UNAME_FIELD],
}

impl Uname {
    pub const SIZE: usize = core::mem::size_of::<Uname>();

    /// A `Uname` from its names, each cut at [`UNAME_FIELD`] bytes.
    pub fn new(sysname: &[u8], nodename: &[u8], release: &[u8], machine: &[u8]) -> Uname {
        fn field(name: &[u8]) -> [u8; UNAME_FIELD] {
            let mut f = [0; UNAME_FIELD];
            let n = name.len().min(UNAME_FIELD);
            f[..n].copy_from_slice(&name[..n]);
            f
        }
        Uname {
            sysname: field(sysname),
            nodename: field(nodename),
            release: field(release),
            machine: field(machine),
        }
    }

    /// A field's name, without its padding.
    pub fn name(field: &[u8; UNAME_FIELD]) -> &[u8] {
        let end = field.iter().position(|&b| b == 0).unwrap_or(UNAME_FIELD);
        &field[..end]
    }

    /// Its bytes, as a program's memory holds the struct.
    pub fn to_bytes(&self) -> [u8; Uname::SIZE] {
        let mut b = [0; Uname::SIZE];
        for (i, f) in [self.sysname, self.nodename, self.release, self.machine]
            .iter()
            .enumerate()
        {
            b[UNAME_FIELD * i..UNAME_FIELD * (i + 1)].copy_from_slice(f);
        }
        b
    }
}
````

- [ ] **Step 13: Change `crates/relay-abi/src/lib.rs`**

In `crates/relay-abi/src/lib.rs`, replace:

````rust
pub use call::Call;
pub use info::{MemInfo, Time};
pub use result::{MAX_ERRNO, decode, encode};
````

with:

````rust
pub use call::Call;
pub use file::{DirEntry, Stat, StatFs};
pub use info::{MemInfo, Time, Uname};
pub use result::{MAX_ERRNO, decode, encode};
````

- [ ] **Step 14: Implement `crates/relay-abi/src/power.rs`**

Insert this at the top of `crates/relay-abi/src/power.rs`, above `#[cfg(test)]`:

````rust
//! What `power` takes (spec §7.3).

/// `power`'s kinds.
pub const POWER_REBOOT: u32 = 1;
pub const POWER_POWEROFF: u32 = 2;
/// `power`'s flag: go ahead even if the filesystems cannot be shut down
/// cleanly (milestone 1's `reboot -f`).
pub const POWER_FORCE: u32 = 1;

````

- [ ] **Step 15: Change `crates/vfs/src/errno.rs`**

In `crates/vfs/src/errno.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    EINTR,
}
````

with:

````rust
    EINTR,
    /// Every fd of a process is in use.
    EMFILE,
    /// A buffer too short for the answer (`getcwd`).
    ERANGE,
}
````

Replace:

````rust
            Errno::EINTR => "Interrupted system call",
        }
````

with:

````rust
            Errno::EINTR => "Interrupted system call",
            Errno::EMFILE => "Too many open files",
            Errno::ERANGE => "Numerical result out of range",
        }
````

Replace:

````rust
            Errno::EINTR => n::EINTR,
        }
````

with:

````rust
            Errno::EINTR => n::EINTR,
            Errno::EMFILE => n::EMFILE,
            Errno::ERANGE => n::ERANGE,
        }
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p relay-abi -p vfs`

Expected: PASS: 70 tests.

- [ ] **Step 17: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 18: Commit**

````bash
git add crates
git commit -m "abi: what the file, console, sys_info and power calls pass; EMFILE and ERANGE"
````


### Task 2: The heap grows by regions

`crates/heap` served one region; a program's heap (Task 16) grows by what `mem_map` gives. `Heap::add(start, size)` adds a region, and since the free list is kept in address order and merged, regions that meet are one: a block may span them, and `mem_map`, which fills its area from the start, makes a heap that grows by adding to its end (decision 8). `Heap::new` is `empty` and `add`; the stats count every region. The random test now runs over four regions apart. Mutation checks: `add` without counting its bytes, or without widening the bounds the debug check of a freed block uses, fails a test.

**Files:**
- Modify: `crates/heap/src/lib.rs`

**Interfaces:**
- Consumes: plan 1's `crates/heap`.
- Produces: `heap::Heap::add(&mut self, start: usize, size: usize)` (unsafe).

- [ ] **Step 1: Add the failing tests to `crates/heap/src/lib.rs`**

In `crates/heap/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Seeded random alloc/free mix. Every block is filled with a tag byte
````

with:

````rust

    /// `size` bytes of fresh 4 KiB-aligned host memory.
    fn region(size: usize) -> usize {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        mem
    }

    #[test]
    fn an_empty_heap_has_nothing_until_a_region_is_added() {
        let mut h = Heap::empty();
        assert!(h.alloc(l(16, 8)).is_none());
        assert_eq!(h.stats().total, 0);
        unsafe { h.add(region(64 * 1024), 64 * 1024) };
        assert!(h.alloc(l(16, 8)).is_some());
        assert_eq!(h.stats().total, 64 * 1024);
    }

    #[test]
    fn a_region_added_later_serves_what_the_first_cannot() {
        let mut h = heap(64 * 1024);
        let a = h.alloc(l(60 * 1024, 16)).unwrap();
        assert!(h.alloc(l(8 * 1024, 16)).is_none());
        let second = region(64 * 1024);
        unsafe { h.add(second, 64 * 1024) };
        let b = h.alloc(l(8 * 1024, 16)).unwrap();
        assert!((second..second + 64 * 1024).contains(&(b.as_ptr() as usize)));
        assert_eq!(h.stats().total, 128 * 1024);
        unsafe {
            h.dealloc(a, l(60 * 1024, 16));
            h.dealloc(b, l(8 * 1024, 16));
        }
        assert_eq!(h.stats().used, 0);
        assert_eq!(h.stats().free_large, 128 * 1024);
    }

    #[test]
    fn regions_that_meet_are_one() {
        // Mapped one after the other, as a program's heap grows.
        let mem = region(3 * 64 * 1024);
        let mut h = unsafe { Heap::new(mem, 64 * 1024) };
        unsafe {
            h.add(mem + 2 * 64 * 1024, 64 * 1024);
            h.add(mem + 64 * 1024, 64 * 1024);
        }
        let s = h.stats();
        assert_eq!((s.total, s.largest_free), (3 * 64 * 1024, 3 * 64 * 1024));
        let big = h.alloc(l(150 * 1024, 16)).unwrap();
        assert_eq!(big.as_ptr() as usize, mem, "one block across all three");
        unsafe { h.dealloc(big, l(150 * 1024, 16)) };
        assert_eq!(h.stats().largest_free, 3 * 64 * 1024);
    }

    /// Seeded random alloc/free mix. Every block is filled with a tag byte
````

Replace:

````rust
    fn random_mix_never_overlaps() {
        let mut h = heap(4 << 20);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
````

with:

````rust
    fn random_mix_never_overlaps() {
        // Four regions apart, as a program's heap may have.
        let mut h = heap(1 << 20);
        for _ in 0..3 {
            unsafe { h.add(region(1 << 20), 1 << 20) };
        }
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p heap`

Expected: FAIL: compile errors such as `` no method named `add` found for struct `Heap` in the current scope ``.

- [ ] **Step 3: Change `crates/heap/src/lib.rs`**

In `crates/heap/src/lib.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! A heap allocator over one region of memory (spec §5.2 of milestone 1).
//! The kernel's `#[global_allocator]` is one of these behind a lock
//! (`kernel/src/mm/heap.rs`); user programs will get one too.
//!
````

with:

````rust
//! A heap allocator over regions of memory (spec §5.2 of milestone 1, §8.1
//! of the user-space gate). The kernel's `#[global_allocator]` is one of
//! these over one region behind a lock (`kernel/src/mm/heap.rs`); a
//! program's (`relay-rt`) starts empty and grows by regions it maps.
//! Regions that meet are one: a block may span them.
//!
````

Replace:

````rust
pub struct Heap {
    start: usize,
    end: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
````

with:

````rust
pub struct Heap {
    /// The lowest and highest address of any region, for the debug check
    /// that a freed block is the heap's.
    start: usize,
    end: usize,
    /// Bytes in all regions.
    total: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
````

Replace:

````rust
        Heap {
            start: 0,
            end: 0,
            classes: [None; SIZE_CLASSES.len()],
````

with:

````rust
        Heap {
            start: usize::MAX,
            end: 0,
            total: 0,
            classes: [None; SIZE_CLASSES.len()],
````

Replace:

````rust
    pub unsafe fn new(start: usize, size: usize) -> Heap {
        assert!(start.is_multiple_of(MIN_BLOCK));
        let size = size & !(MIN_BLOCK - 1);
        let mut h = Heap::empty();
        h.start = start;
        h.end = start + size;
        unsafe { h.insert_free(start, size) };
        h
    }
````

with:

````rust
    pub unsafe fn new(start: usize, size: usize) -> Heap {
        let mut h = Heap::empty();
        unsafe { h.add(start, size) };
        h
    }

    /// Adds [start, start + size) to the heap. A region that meets
    /// another one merges with it.
    ///
    /// # Safety
    /// As for `new`, and the region must not overlap one the heap has.
    pub unsafe fn add(&mut self, start: usize, size: usize) {
        assert!(start.is_multiple_of(MIN_BLOCK));
        let size = size & !(MIN_BLOCK - 1);
        if size == 0 {
            return;
        }
        self.start = self.start.min(start);
        self.end = self.end.max(start + size);
        self.total += size;
        unsafe { self.insert_free(start, size) };
    }
````

Replace:

````rust
        HeapStats {
            total: self.end - self.start,
            used: self.used,
````

with:

````rust
        HeapStats {
            total: self.total,
            used: self.used,
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p heap`

Expected: PASS: 8 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "heap: more regions, for a heap that grows"
````


### Task 3: Milestone 1's serial findings: an Escape alone over COM1, and a Ctrl-C from the keyboard

Two of milestone 1's deferred findings, which the roadmap gives to plan 3b (decision 15). An `ESC` over COM1 started an escape sequence that waited for its next byte, so the Escape key typed on a serial terminal did nothing until the next key, which it swallowed; now `push_serial` takes the time each byte came, and `expire`, which `tty::poll` calls, lets an `ESC` with nothing after it for `ESC_TIMEOUT_MS` (50 ms, counted from the `ESC`, by the TSC, which runs even without a timer) in as the Escape key, and drops the start of a longer sequence that never ends. A Ctrl-C typed on the keyboard left a half-arrived serial sequence, which the next serial byte finished into a key nobody typed; now any Ctrl-C clears it, in `push`. Mutation checks: `push` without clearing the sequence, `expire` without its timeout or letting any half sequence in fail a test; the timeout counted from each byte instead of the `ESC` survived until the test gained a slow `ESC [`.

**Files:**
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: milestone 1's `input::InputQueue`, `tty::poll`.
- Produces: `InputQueue::{push_serial(b: u8, now: u64), expire(now: u64)}`, `input::ESC_TIMEOUT_MS`.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        for &b in bytes {
            q.push_serial(b);
        }
````

with:

````rust
        for &b in bytes {
            q.push_serial(b, 1000);
        }
````

Replace:

````rust
    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    #[test]
    fn an_escape_alone_over_serial_goes_in_once_nothing_follows() {
        // Milestone 1's deferred finding: the Escape key over COM1 waited
        // for the next byte, which it then swallowed.
        let mut q = InputQueue::new();
        q.push_serial(0x1B, 1000);
        q.expire(1000 + ESC_TIMEOUT_MS - 1);
        assert!(q.is_empty(), "the rest of a sequence may still come");
        q.expire(1000 + ESC_TIMEOUT_MS);
        assert_eq!(drain(&mut q), b"\x1b");
        q.push_serial(b'x', 2000);
        assert_eq!(drain(&mut q), b"x", "what follows is plain input");
        // A sequence that arrives in time is not cut.
        serial(&mut q, b"\x1b[");
        q.expire(1000 + ESC_TIMEOUT_MS - 1);
        serial(&mut q, b"A");
        assert_eq!(drain(&mut q), b"\x1b[A");
        // The start of a longer one that never ends is dropped, counting
        // from its `ESC`.
        q.push_serial(0x1B, 3000);
        q.push_serial(b'[', 3000 + ESC_TIMEOUT_MS - 1);
        q.expire(3000 + ESC_TIMEOUT_MS);
        q.push_serial(b'y', 5000);
        assert_eq!(drain(&mut q), b"y");
        q.expire(0);
        assert!(q.is_empty(), "nothing waits");
    }

    #[test]
    fn a_ctrl_c_from_the_keyboard_ends_a_serial_sequence() {
        // Milestone 1's deferred finding: the keyboard's Ctrl-C left the
        // half sequence, and the next serial byte finished it.
        let mut q = InputQueue::new();
        serial(&mut q, b"\x1b[");
        q.push_key(&press(Key::Char(b'c'), true));
        serial(&mut q, b"A");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"A");
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib input::`

Expected: FAIL: compile errors such as `` cannot find value `ESC_TIMEOUT_MS` in this scope ``; `this method takes 1 argument but 2 arguments were supplied`.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

pub struct InputQueue {
    bytes: VecDeque<u8>,
    /// An escape sequence arriving over serial, until it is complete.
    sequence: Vec<u8>,
}
````

with:

````rust

/// How long the rest of an escape sequence may take to arrive over serial.
/// A terminal sends a sequence at once, so an `ESC` with nothing after it
/// for this long is the Escape key (milestone 1's deferred finding).
pub const ESC_TIMEOUT_MS: u64 = 50;

pub struct InputQueue {
    bytes: VecDeque<u8>,
    /// An escape sequence arriving over serial, until it is complete.
    sequence: Vec<u8>,
    /// When its `ESC` came, in milliseconds.
    sequence_since: u64,
}
````

Replace:

````rust
            sequence: Vec::new(),
        }
    }

    /// Adds one byte from COM1. An escape sequence (`ESC` and one byte, or
    /// `ESC [` up to its final byte) waits until it is complete and then
    /// goes in whole or not at all, like a key's; Ctrl-C ends it.
    pub fn push_serial(&mut self, b: u8) {
        if b == INTERRUPT {
            self.sequence.clear();
            self.push(&[b]);
        } else if self.sequence.is_empty() && b != 0x1B {
            self.push(&[b]);
        } else {
            self.sequence.push(b);
````

with:

````rust
            sequence: Vec::new(),
            sequence_since: 0,
        }
    }

    /// Adds one byte from COM1 that came at `now` (milliseconds). An escape
    /// sequence (`ESC` and one byte, or `ESC [` up to its final byte) waits
    /// until it is complete and then goes in whole or not at all, like a
    /// key's; Ctrl-C ends it, and so does `expire`.
    pub fn push_serial(&mut self, b: u8, now: u64) {
        if b == INTERRUPT || self.sequence.is_empty() && b != 0x1B {
            self.push(&[b]);
        } else {
            if self.sequence.is_empty() {
                self.sequence_since = now;
            }
            self.sequence.push(b);
````

Replace:

````rust

    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would.
    pub fn push(&mut self, bytes: &[u8]) {
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.clear();
                &bytes[i..]
````

with:

````rust

    /// Ends an escape sequence arriving over serial whose rest has not come
    /// `ESC_TIMEOUT_MS` after its `ESC`: an `ESC` alone is the Escape key
    /// and goes in; the start of a longer one is garbage and is dropped.
    pub fn expire(&mut self, now: u64) {
        if self.sequence.is_empty() || now.saturating_sub(self.sequence_since) < ESC_TIMEOUT_MS {
            return;
        }
        let seq = core::mem::take(&mut self.sequence);
        if seq == [0x1B] {
            self.push(&seq);
        }
    }

    /// Adds input; what does not fit is dropped. A Ctrl-C always fits:
    /// it drops what was typed before it, as `take_interrupt` would, and a
    /// half-arrived serial sequence, wherever the Ctrl-C came from.
    pub fn push(&mut self, bytes: &[u8]) {
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.clear();
                self.sequence.clear();
                &bytes[i..]
````

- [ ] **Step 4: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::input::InputQueue;
use crate::{serial, usb};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
````

with:

````rust
use crate::input::InputQueue;
use crate::{serial, timer, usb};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
````

Replace:

````rust
    usb::poll(&mut input);
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b),
            None => break,
        }
    }
}
````

with:

````rust
    usb::poll(&mut input);
    let now = now_ms();
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b, now),
            None => break,
        }
    }
    input.expire(now);
}

/// Milliseconds since the machine started, for serial escape sequences:
/// from the TSC, which runs even when the timer could not start.
fn now_ms() -> u64 {
    let t = timer::tsc_time().unwrap_or_else(timer::uptime);
    u64::try_from(t.as_millis()).unwrap_or(u64::MAX)
}
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib input::`

Expected: PASS: 12 tests.

- [ ] **Step 6: Run the `keyboard` scenario**

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -m "kernel: an Escape over COM1 goes in alone after 50 ms, and any Ctrl-C ends a half-arrived serial sequence"
````


### Task 4: The console's line discipline

Spec §6.5's line mode, architecture-neutral and host-tested; Task 17 puts it into the input queue. `LineDiscipline::input` takes one key's bytes (an escape sequence is one key and does nothing) and says what the screen should show: printable characters and UTF-8 are echoed and kept; Backspace (DEL or Ctrl-H) removes the last character whole, its continuation bytes too, and echoes `\b \b`; Enter (CR, LF, or CR LF as one) ends the line, which is read with its `\n`; Ctrl-D on an empty line is end of input and on a started one hands it over without its `\n`, as Linux does; Ctrl-C drops everything typed and not read and says so (the kernel kills the foreground group, §6.4); other control characters do nothing. What waits, the lines and the one being typed, holds 4096 bytes (`LINE_MAX`, Linux's), and a character is taken only with room for the `\n` after it, so Enter always gets in (decision 9). `read` gives at most one line, and what does not fit in the reader's buffer waits for the next read; `take_all` hands back what was typed and not read, for the console's way back to raw mode. Mutation checks: Backspace removing one byte of a character, CR LF as two Enters, a character taken without room for the newline, end of input unbounded, Ctrl-C keeping the lines, a read not counting what it took and a read of the newest line each fail a test.

**Files:**
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/line.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `line::{LINE_MAX, LineDiscipline::{new, input(&mut self, key: &[u8], echo: &mut Vec<u8>) -> bool, has_line(&self) -> bool, read(&mut self, buf: &mut [u8]) -> Option<usize>, take_all(&mut self) -> Vec<u8>}}` (`Default`).

- [ ] **Step 1: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod klog;
pub mod mm;
````

with:

````rust
pub mod klog;
pub mod line;
pub mod mm;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/line.rs`**

Create `kernel/src/line.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Types `bytes`, one key each; the echo.
    fn typed(d: &mut LineDiscipline, bytes: &[u8]) -> Vec<u8> {
        let mut echo = Vec::new();
        for &b in bytes {
            d.input(&[b], &mut echo);
        }
        echo
    }

    /// Reads with a buffer of `n` bytes.
    fn read(d: &mut LineDiscipline, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0; n];
        d.read(&mut buf).map(|k| buf[..k].to_vec())
    }

    #[test]
    fn a_line_is_echoed_and_read_with_its_newline_once_enter_ends_it() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"ls -l"), b"ls -l");
        assert!(!d.has_line());
        assert_eq!(read(&mut d, 100), None, "no line yet: the reader waits");
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        assert!(d.has_line());
        assert_eq!(read(&mut d, 100).unwrap(), b"ls -l\n");
        assert_eq!(read(&mut d, 100), None);
    }

    #[test]
    fn a_read_returns_at_most_one_line_and_keeps_the_rest() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\rtwo\r");
        assert_eq!(read(&mut d, 100).unwrap(), b"one\n");
        assert_eq!(read(&mut d, 2).unwrap(), b"tw");
        assert_eq!(read(&mut d, 2).unwrap(), b"o\n");
        assert_eq!(read(&mut d, 0).unwrap(), b"", "an empty read takes nothing");
        assert_eq!(read(&mut d, 1), None);
        // An empty line is a line.
        typed(&mut d, b"\r");
        assert_eq!(read(&mut d, 10).unwrap(), b"\n");
    }

    #[test]
    fn a_terminal_s_cr_lf_is_one_enter_and_a_lone_lf_is_one_too() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"a\r\nb\n\n"), b"a\nb\n\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"a\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"b\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"\n");
        assert_eq!(read(&mut d, 10), None);
    }

    #[test]
    fn backspace_removes_the_last_character_whole() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"cat\x7f\x7fp"), b"cat\x08 \x08\x08 \x08p");
        // A character of two bytes goes with one Backspace; Ctrl-H erases
        // too.
        assert_eq!(typed(&mut d, "é".as_bytes()), "é".as_bytes());
        assert_eq!(typed(&mut d, b"\x08"), b"\x08 \x08");
        assert_eq!(
            typed(&mut d, b"\x7f\x7f\x7f"),
            b"\x08 \x08\x08 \x08",
            "nothing left to erase"
        );
        typed(&mut d, b"x\r");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
    }

    #[test]
    fn ctrl_d_ends_input_on_an_empty_line_and_hands_over_a_started_one() {
        let mut d = LineDiscipline::new();
        assert_eq!(typed(&mut d, b"\x04"), b"", "not echoed");
        assert_eq!(read(&mut d, 10).unwrap(), b"", "end of input");
        assert_eq!(read(&mut d, 10), None, "taken by that read");
        typed(&mut d, b"abc\x04");
        assert_eq!(read(&mut d, 10).unwrap(), b"abc", "without a newline");
        assert_eq!(read(&mut d, 10), None);
        typed(&mut d, b"x\r\x04\x04");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"");
        assert_eq!(read(&mut d, 10).unwrap(), b"");
    }

    #[test]
    fn escape_sequences_and_other_control_keys_do_nothing() {
        let mut d = LineDiscipline::new();
        let mut echo = Vec::new();
        for key in [
            &b"\x1b[A"[..],
            b"\x1b[3~",
            b"\x1b",
            b"\x01",
            b"\x0c",
            b"\x00",
        ] {
            assert!(!d.input(key, &mut echo));
        }
        assert!(echo.is_empty());
        assert_eq!(typed(&mut d, b"a\tb\r"), b"a\tb\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"a\tb\n");
    }

    #[test]
    fn ctrl_c_drops_everything_typed_and_not_read() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\rtwo\x04thr");
        let mut echo = Vec::new();
        assert!(d.input(b"\x03", &mut echo));
        assert!(echo.is_empty(), "the shell says ^C");
        assert!(!d.has_line());
        assert_eq!(typed(&mut d, b"x\r"), b"x\n");
        assert_eq!(read(&mut d, 10).unwrap(), b"x\n");
    }

    #[test]
    fn what_waits_is_bounded_and_enter_still_gets_in() {
        let mut d = LineDiscipline::new();
        let long = vec![b'a'; LINE_MAX + 10];
        let echo = typed(&mut d, &long);
        assert_eq!(echo.len(), LINE_MAX - 1, "room is kept for the newline");
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        assert_eq!(read(&mut d, 2 * LINE_MAX).unwrap().len(), LINE_MAX);
        // Lines that wait take room from the next one.
        typed(&mut d, &[b'b'; 2000]);
        typed(&mut d, b"\r");
        assert_eq!(typed(&mut d, &[b'c'; 3000]).len(), LINE_MAX - 2001 - 1);
        assert_eq!(typed(&mut d, b"\r"), b"\n");
        // Full: an Enter no longer fits, nor an end of input.
        assert_eq!(typed(&mut d, b"\r\x04"), b"");
        assert_eq!(read(&mut d, LINE_MAX).unwrap().len(), 2001);
        assert_eq!(read(&mut d, LINE_MAX).unwrap().len(), LINE_MAX - 2001);
        assert_eq!(read(&mut d, 10), None);
        // Everything read: the whole room again.
        assert_eq!(typed(&mut d, &long).len(), LINE_MAX - 1);
    }

    #[test]
    fn going_back_to_raw_hands_over_what_was_typed() {
        let mut d = LineDiscipline::new();
        typed(&mut d, b"one\r\x04two\rthr");
        assert_eq!(d.take_all(), b"one\ntwo\nthr");
        assert!(!d.has_line());
        assert_eq!(d.take_all(), b"");
        // The room is back.
        assert_eq!(typed(&mut d, &[b'a'; LINE_MAX]).len(), LINE_MAX - 1);
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `LineDiscipline` in this scope ``; `` cannot find value `LINE_MAX` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/line.rs`**

Insert this at the top of `kernel/src/line.rs`, above `#[cfg(test)]`:

````rust
//! The console's line discipline (user-space gate §6.5): in line mode what
//! is typed is echoed and edited here, and a program reads it a line at a
//! time. Enter ends a line (the reader gets it with a `\n`), Backspace
//! removes the last character, Ctrl-D on an empty line is end of input and
//! on a line with something in it hands that over without a `\n`, as Linux
//! does. Keys that send escape sequences (arrows, Delete) and other control
//! characters do nothing. Ctrl-C drops everything typed and not read (the
//! kernel then kills the foreground group, §6.4).
//!
//! It works on keys, not bytes: the input queue hands over each key's
//! bytes together, so an escape sequence is one key.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// The bytes typed and not read that the discipline keeps, the lines that
/// wait and the one being typed together, each line's `\n` included (as
/// Linux's 4096). Beyond it, what is typed is dropped; Enter and Ctrl-C
/// always get in.
pub const LINE_MAX: usize = 4096;

/// Ctrl-C, Ctrl-D, Backspace (and Ctrl-H), Escape.
const INTERRUPT: u8 = 0x03;
const END: u8 = 0x04;
const ERASE: [u8; 2] = [0x7F, 0x08];
const ESC: u8 = 0x1B;

#[derive(Default)]
pub struct LineDiscipline {
    /// The line being typed.
    line: Vec<u8>,
    /// Lines ended with Enter or Ctrl-D, oldest first; `None` is end of
    /// input.
    ready: VecDeque<Option<Vec<u8>>>,
    /// The bytes in `ready`, an end of input counting as one.
    waiting: usize,
    /// The last key was a CR: a LF right after it is the same Enter (a
    /// terminal that sends CR LF).
    after_cr: bool,
}

impl LineDiscipline {
    pub const fn new() -> LineDiscipline {
        LineDiscipline {
            line: Vec::new(),
            ready: VecDeque::new(),
            waiting: 0,
            after_cr: false,
        }
    }

    /// One key's bytes. What the screen should show is added to `echo`.
    /// Whether it was Ctrl-C, which dropped everything typed and not read.
    pub fn input(&mut self, key: &[u8], echo: &mut Vec<u8>) -> bool {
        let after_cr = core::mem::take(&mut self.after_cr);
        let &[b] = key else {
            // An escape sequence.
            return false;
        };
        match b {
            INTERRUPT => {
                self.line.clear();
                self.ready.clear();
                self.waiting = 0;
                return true;
            }
            b'\n' if after_cr => {}
            b'\r' | b'\n' => {
                self.after_cr = b == b'\r';
                // Enter always fits: a character is taken only with room
                // for the `\n` after it.
                if self.waiting + self.line.len() < LINE_MAX {
                    self.line.push(b'\n');
                    self.hand_over();
                    echo.push(b'\n');
                }
            }
            END => {
                if self.line.is_empty() {
                    if self.waiting < LINE_MAX {
                        self.ready.push_back(None);
                        self.waiting += 1;
                    }
                } else {
                    self.hand_over();
                }
            }
            _ if ERASE.contains(&b) => {
                if self.line.is_empty() {
                    return false;
                }
                // A whole character: its continuation bytes, then its
                // first byte.
                while self.line.pop().is_some_and(|c| c & 0xC0 == 0x80) {}
                echo.extend_from_slice(b"\x08 \x08");
            }
            ESC => {}
            b'\t' | 0x20..=0x7E | 0x80.. if self.waiting + self.line.len() + 2 <= LINE_MAX => {
                self.line.push(b);
                echo.push(b);
            }
            // Other control characters, and what does not fit.
            _ => {}
        }
        false
    }

    /// Moves the line being typed to the ones that wait.
    fn hand_over(&mut self) {
        self.waiting += self.line.len();
        self.ready.push_back(Some(core::mem::take(&mut self.line)));
    }

    /// Whether a line, or end of input, waits to be read.
    pub fn has_line(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Reads from the oldest line that waits into `buf`: `None` if none
    /// does; `Some(0)` at end of input (which it takes); otherwise the
    /// bytes read, never more than one line. What does not fit in `buf`
    /// waits for the next read.
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if buf.is_empty() {
            return Some(0);
        }
        let first = self.ready.front_mut()?;
        let Some(line) = first else {
            self.ready.pop_front();
            self.waiting -= 1;
            return Some(0);
        };
        let n = buf.len().min(line.len());
        buf[..n].copy_from_slice(&line[..n]);
        if n == line.len() {
            self.ready.pop_front();
        } else {
            line.drain(..n);
        }
        self.waiting -= n;
        Some(n)
    }

    /// Everything typed and not read, as the bytes a raw reader would have
    /// had (the console going back to raw mode): the lines that wait, each
    /// with its `\n`, then the line being typed. Ends of input are dropped.
    pub fn take_all(&mut self) -> Vec<u8> {
        let mut out: Vec<u8> = self.ready.drain(..).flatten().flatten().collect();
        out.append(&mut self.line);
        self.waiting = 0;
        self.after_cr = false;
        out
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 285 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the console's line discipline"
````


### Task 5: The console's tee stack

Spec §6.5's tees, architecture-neutral and host-tested; Task 22 gives them the files and the console. `TeeStack<F>` holds at most 4 tees (`EBUSY` beyond), each with its owner, its file and the copies that wait: `add` copies what the console was given to every tee; `flush` writes a tee's copies once 4 KiB wait (`CHUNK`), or all of them for a `sync`, through the writer it is given, so it is tested without files; `pop` writes what waits for the owner's newest tee and takes it off, `EINVAL` if the owner has none. A write that fails takes the tee out of the copying (the flush returns it, for the log), but it keeps its place until its owner pops it, and that pop returns the write's error (decision 11). `end` stops the tees of a process that ended; they are written for the last time at the next flush, since a process may end where no file can be written. Mutation checks: 5 tees, every flush writing everything, a pop of the owner's oldest, a pop forgetting an earlier failure, a failed tee still written and an ended process's tees never written each fail a test; a failed tee still collecting copies (which then pile up) survived until the test looked at its buffer.

**Files:**
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/tee.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `tee::{TEES, CHUNK, Writer<'_, F>, TeeStack<F>::{new, push(owner: u32, F) -> Result<(), Errno>, is_copying(&self) -> bool, add(&mut self, &[u8]), flush(&mut self, all: bool, &mut Writer<'_, F>) -> Vec<(u32, Errno)>, pop(&mut self, owner: u32, &mut Writer<'_, F>) -> Result<(), Errno>, end(&mut self, owner: u32), files(&self) -> impl Iterator<Item = &F>}}`.

- [ ] **Step 1: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod system;
pub mod timer;
````

with:

````rust
pub mod system;
pub mod tee;
pub mod timer;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/tee.rs`**

Create `kernel/src/tee.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// Files by number: what was written to each, and the ones that fail.
    #[derive(Default)]
    struct Files {
        written: BTreeMap<u32, Vec<u8>>,
        writes: usize,
        full: Vec<u32>,
    }

    impl Files {
        fn writer(&mut self) -> impl FnMut(&u32, &[u8]) -> Result<(), Errno> + '_ {
            |f, bytes| {
                self.writes += 1;
                if self.full.contains(f) {
                    return Err(Errno::ENOSPC);
                }
                self.written.entry(*f).or_default().extend_from_slice(bytes);
                Ok(())
            }
        }

        fn of(&self, f: u32) -> &[u8] {
            self.written.get(&f).map_or(&[], |v| v)
        }
    }

    #[test]
    fn at_most_4_tees_and_each_gets_a_copy() {
        let mut s = TeeStack::new();
        assert!(!s.is_copying());
        for f in 1..=4 {
            s.push(10, f).unwrap();
        }
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY));
        assert!(s.is_copying());
        s.add(b"hello\n");
        let mut files = Files::default();
        assert!(s.flush(true, &mut files.writer()).is_empty());
        for f in 1..=4 {
            assert_eq!(files.of(f), b"hello\n");
        }
    }

    #[test]
    fn copies_are_written_every_4_kib_and_at_a_full_flush() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        let mut files = Files::default();
        s.add(&[b'a'; CHUNK - 1]);
        s.flush(false, &mut files.writer());
        assert_eq!(files.writes, 0, "less than 4 KiB waits");
        s.add(b"b");
        s.flush(false, &mut files.writer());
        assert_eq!((files.writes, files.of(1).len()), (1, CHUNK));
        s.add(b"tail");
        s.flush(true, &mut files.writer());
        assert_eq!(&files.of(1)[CHUNK..], b"tail");
        s.flush(true, &mut files.writer());
        assert_eq!(files.writes, 2, "nothing waits, nothing is written");
    }

    #[test]
    fn a_pop_writes_what_waits_and_takes_the_owner_s_newest() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(20, 2).unwrap();
        s.push(10, 3).unwrap();
        s.add(b"x");
        let mut files = Files::default();
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!((files.of(3), files.of(1)), (&b"x"[..], &b""[..]));
        s.add(b"y");
        s.flush(true, &mut files.writer());
        assert_eq!(files.of(3), b"x", "popped: no more copies");
        assert_eq!((files.of(1), files.of(2)), (&b"xy"[..], &b"xy"[..]));
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
        s.pop(20, &mut files.writer()).unwrap();
        assert!(!s.is_copying());
        // Room for 4 again.
        for f in 1..=4 {
            s.push(30, f).unwrap();
        }
    }

    #[test]
    fn a_failed_write_removes_the_tee_and_its_pop_says_why() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(10, 2).unwrap();
        let mut files = Files {
            full: vec![1],
            ..Files::default()
        };
        s.add(&[b'a'; CHUNK]);
        assert_eq!(
            s.flush(false, &mut files.writer()),
            [(10, Errno::ENOSPC)],
            "for the log"
        );
        let writes = files.writes;
        s.add(&[b'b'; CHUNK]);
        assert!(s.tees[0].pending.is_empty(), "nothing piles up for it");
        s.flush(true, &mut files.writer());
        assert_eq!(files.writes, writes + 1, "tee 1 gets nothing more");
        assert_eq!(s.push(10, 3), Ok(()));
        assert_eq!(s.push(10, 4), Ok(()));
        assert_eq!(s.push(10, 5), Err(Errno::EBUSY), "it still takes its place");
        s.pop(10, &mut files.writer()).unwrap();
        s.pop(10, &mut files.writer()).unwrap();
        s.pop(10, &mut files.writer()).unwrap();
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::ENOSPC));
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
        // The pop's own write can fail too.
        s.push(10, 1).unwrap();
        s.add(b"z");
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::ENOSPC));
    }

    #[test]
    fn a_process_s_tees_end_with_it_and_are_written_at_the_next_flush() {
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        s.push(20, 2).unwrap();
        s.push(10, 3).unwrap();
        s.add(b"last");
        s.end(10);
        s.add(b"more");
        assert_eq!(s.files().count(), 3, "the closing ones too");
        for f in [4, 5, 6] {
            s.push(30, f).unwrap();
        }
        let mut files = Files::default();
        s.flush(false, &mut files.writer());
        assert_eq!((files.of(1), files.of(3)), (&b"last"[..], &b"last"[..]));
        assert_eq!(files.of(2), b"", "not due yet");
        assert_eq!(s.files().count(), 4, "written for the last time");
        s.flush(true, &mut files.writer());
        assert_eq!(files.of(1), b"last");
        assert_eq!(files.of(2), b"lastmore");
        assert_eq!(s.pop(10, &mut files.writer()), Err(Errno::EINVAL));
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib tee::`

Expected: FAIL: compile errors such as `` cannot find type `Errno` in this scope ``; `` cannot find value `CHUNK` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/tee.rs`**

Insert this at the top of `kernel/src/tee.rs`, above `#[cfg(test)]`:

````rust
//! The console's tees (user-space gate §6.5): files that get a copy of
//! everything written to the console. At most 4 are pushed at a time; a
//! process pops the newest one it pushed. What they get is buffered and
//! written once 4 KiB wait, and all of it at every `sync` and when the tee
//! is popped, so a big `cat` is never held whole.
//!
//! A write that fails removes the tee from the copying (the kernel logs
//! it); the entry stays until its owner pops it, and that pop returns the
//! write's error, so a script's shell can report it. When a process ends,
//! its tees stop getting copies at once and are written for the last time
//! at the next flush (a process may end where the kernel cannot write
//! files: in a fault or a tick).
//!
//! The stack is generic over the file (`F`) and gets a writer at each
//! flush, so it is tested without files.

use alloc::vec::Vec;
use vfs::Errno;

/// Tees on the stack at most.
pub const TEES: usize = 4;
/// A tee's copies are written once this much waits.
pub const CHUNK: usize = 4096;

struct Tee<F> {
    owner: u32,
    /// `None` once a write failed.
    file: Option<F>,
    pending: Vec<u8>,
    error: Option<Errno>,
}

/// How a flush writes a tee's copies to its file: all of them, or the
/// error.
pub type Writer<'a, F> = dyn FnMut(&F, &[u8]) -> Result<(), Errno> + 'a;

pub struct TeeStack<F> {
    tees: Vec<Tee<F>>,
    /// Tees of processes that have ended, written at the next flush.
    closing: Vec<Tee<F>>,
}

impl<F> Default for TeeStack<F> {
    fn default() -> Self {
        TeeStack::new()
    }
}

impl<F> TeeStack<F> {
    pub const fn new() -> TeeStack<F> {
        TeeStack {
            tees: Vec::new(),
            closing: Vec::new(),
        }
    }

    /// Pushes `file` for process `owner`. `EBUSY` if 4 tees are pushed.
    pub fn push(&mut self, owner: u32, file: F) -> Result<(), Errno> {
        if self.tees.len() >= TEES {
            return Err(Errno::EBUSY);
        }
        self.tees.push(Tee {
            owner,
            file: Some(file),
            pending: Vec::new(),
            error: None,
        });
        Ok(())
    }

    /// Whether any tee gets copies (so a console write can skip `add`).
    pub fn is_copying(&self) -> bool {
        self.tees.iter().any(|t| t.file.is_some())
    }

    /// A copy of what the console was given, for every tee.
    pub fn add(&mut self, bytes: &[u8]) {
        for t in self.tees.iter_mut().filter(|t| t.file.is_some()) {
            t.pending.extend_from_slice(bytes);
        }
    }

    /// Writes the copies that wait: of the tees with 4 KiB waiting, or of
    /// all of them with `all`, and the last ones of the tees whose process
    /// ended. The tees whose write failed, as (owner, error), for the log.
    pub fn flush(&mut self, all: bool, write: &mut Writer<'_, F>) -> Vec<(u32, Errno)> {
        let mut failed = Vec::new();
        for t in &mut self.tees {
            if (all || t.pending.len() >= CHUNK)
                && let Err(e) = write_out(t, write)
            {
                failed.push((t.owner, e));
            }
        }
        for mut t in core::mem::take(&mut self.closing) {
            if let Err(e) = write_out(&mut t, write) {
                failed.push((t.owner, e));
            }
        }
        failed
    }

    /// Pops the newest tee `owner` pushed, after writing what waits for it.
    /// Its write's error if one failed, now or before; `EINVAL` if `owner`
    /// has none.
    pub fn pop(&mut self, owner: u32, write: &mut Writer<'_, F>) -> Result<(), Errno> {
        let i = self
            .tees
            .iter()
            .rposition(|t| t.owner == owner)
            .ok_or(Errno::EINVAL)?;
        let mut t = self.tees.remove(i);
        let flushed = write_out(&mut t, write);
        match t.error {
            Some(e) => Err(e),
            None => flushed,
        }
    }

    /// Process `owner` ended: its tees get no more copies, and are written
    /// for the last time at the next flush.
    pub fn end(&mut self, owner: u32) {
        let (theirs, others) = core::mem::take(&mut self.tees)
            .into_iter()
            .partition(|t| t.owner == owner);
        self.tees = others;
        self.closing
            .extend(theirs.into_iter().filter(|t| t.file.is_some()));
    }

    /// Every file on the stack (to mark the ones a removal took, spec
    /// §16 item 4).
    pub fn files(&self) -> impl Iterator<Item = &F> {
        self.tees
            .iter()
            .chain(&self.closing)
            .filter_map(|t| t.file.as_ref())
    }
}

/// Writes what waits for `t`; on an error the tee gets no more copies and
/// keeps the error.
fn write_out<F>(t: &mut Tee<F>, write: &mut Writer<'_, F>) -> Result<(), Errno> {
    let Some(file) = &t.file else {
        return Ok(());
    };
    if t.pending.is_empty() {
        return Ok(());
    }
    let r = write(file, &t.pending);
    t.pending.clear();
    if let Err(e) = r {
        t.file = None;
        t.error = Some(e);
    }
    r
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib tee::`

Expected: PASS: 5 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel
git commit -m "kernel: the console's tee stack"
````


### Task 6: A removal or a move is recorded for the directories and files outside the table

Plan 3a's roadmap note: a removal marked as gone only the current directory in the mount table at the time. Once programs `chdir`, open files and create new ones, an inode a removal frees can go to the next file at once (ext2 takes the lowest free one), and another process's working directory or open file would reach it (decision 6). The mount table now records what each `rmdir`, `unlink` and `rename` did as a `vfs::Change`: a directory, or a file's last name, removed (a file with another name lives on; one whose inode cannot be read counts as freed); a directory moved, with the trail to its new place. The table applies each change to its own current directory as before (`follow`), and `take_changes` hands them to the kernel for the rest (Task 13): `Cwd::follow` leaves a directory gone when a removed one is on its way, takes a moved one's new path along, and never follows a trail that is gone already, whose inodes may be other files' now; `Change::removed` names the freed inode, for open files. Mutation checks: every removal freeing (ignoring the link count), a directory not always freeing, a gone trail followed, a removal not marking a trail, and rmdir, a replacing rename, a move or an unlink not recorded each fail a test.

**Files:**
- Modify: `crates/vfs/src/lib.rs`
- Modify: `crates/vfs/src/mount.rs`

**Interfaces:**
- Consumes: plan 3a's `vfs::Cwd`, `MountTable::swap_cwd`.
- Produces: `vfs::Change::removed(&self) -> Option<Node>`, `Cwd::follow(&mut self, &Change)`, `MountTable::take_changes(&mut self) -> Vec<Change>`.

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, replace:

````rust
    }

    #[test]
    fn a_second_filesystem_mounts_on_a_directory() {
````

with:

````rust
    }

    /// A current directory put aside at `path`, with `/` in the table.
    fn aside(t: &mut MountTable, path: &[u8]) -> Cwd {
        t.chdir(path).unwrap();
        t.swap_cwd(t.root_cwd())
    }

    /// `cwd` put in to resolve `path`, and put aside again.
    fn from(t: &mut MountTable, cwd: &mut Cwd, path: &[u8]) -> Result<Node, Errno> {
        let theirs = t.swap_cwd(cwd.clone());
        let r = t.lookup(path);
        *cwd = t.swap_cwd(theirs);
        r
    }

    fn follow_all(t: &mut MountTable, cwd: &mut Cwd) {
        for c in t.take_changes() {
            cwd.follow(&c);
        }
    }

    #[test]
    fn a_removal_reaches_a_current_directory_put_aside() {
        let mut t = table();
        t.mkdir(b"/tmp/x").unwrap();
        let mut other = aside(&mut t, b"/tmp/x");
        let x = t.lookup(b"/tmp/x").unwrap();
        t.rmdir(b"/tmp/x").unwrap();
        let changes = t.take_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].removed(), Some(x));
        assert!(t.take_changes().is_empty(), "taken once");
        other.follow(&changes[0]);
        // Its inode is reused at once.
        t.mkdir(b"/root/y").unwrap();
        assert_eq!(t.lookup(b"/root/y").unwrap(), x);
        assert_eq!(from(&mut t, &mut other, b"."), Err(Errno::ENOENT));
        assert_eq!(t.swap_cwd(other.clone()), t.root_cwd());
        assert_eq!(t.cwd(), b"/tmp/x", "the path it had");
        t.swap_cwd(t.root_cwd());
        // Gone for good: a move of the directory that has its inode now
        // is not followed.
        t.rename(b"/root/y", b"/root/z").unwrap();
        follow_all(&mut t, &mut other);
        t.swap_cwd(other.clone());
        assert_eq!(t.cwd(), b"/tmp/x");
        t.swap_cwd(t.root_cwd());
        t.rename(b"/root/z", b"/root/y").unwrap();
        // A directory not on its way changes nothing.
        let mut home = aside(&mut t, b"/root");
        follow_all(&mut t, &mut home);
        t.rmdir(b"/root/y").unwrap();
        follow_all(&mut t, &mut home);
        assert!(from(&mut t, &mut home, b".").is_ok());
    }

    #[test]
    fn a_move_takes_a_current_directory_put_aside_along() {
        let mut t = table();
        t.mkdir(b"/tmp/a").unwrap();
        t.mkdir(b"/tmp/a/b").unwrap();
        t.mkdir(b"/tmp/a/b/c").unwrap();
        let mut other = aside(&mut t, b"/tmp/a/b/c");
        t.rename(b"/tmp/a/b", b"/root/b").unwrap();
        follow_all(&mut t, &mut other);
        let theirs = t.swap_cwd(other.clone());
        assert_eq!(t.cwd(), b"/root/b/c");
        t.swap_cwd(theirs);
        // Its old parent is empty now, and removing it leaves the moved
        // directory alone; `..` goes along the new path.
        t.rmdir(b"/tmp/a").unwrap();
        follow_all(&mut t, &mut other);
        assert!(from(&mut t, &mut other, b".").is_ok());
        assert_eq!(
            from(&mut t, &mut other, b"../..").unwrap(),
            t.lookup(b"/root").unwrap()
        );
        // Moving a file changes no current directory.
        t.create(b"/tmp/f").unwrap();
        t.rename(b"/tmp/f", b"/tmp/g").unwrap();
        assert!(t.take_changes().is_empty());
    }

    #[test]
    fn the_last_name_of_a_file_going_frees_it() {
        let mut t = table();
        let motd = t.lookup(b"/etc/motd").unwrap();
        t.unlink(b"/etc/motd").unwrap();
        let changes = t.take_changes();
        assert_eq!(
            changes.iter().map(Change::removed).collect::<Vec<_>>(),
            [Some(motd)]
        );
        // A rename over a file or an empty directory frees what it
        // replaces; the moved directory itself is not removed.
        let old = t.create(b"/tmp/old").unwrap();
        t.create(b"/tmp/new").unwrap();
        t.rename(b"/tmp/new", b"/tmp/old").unwrap();
        let removed: Vec<_> = t
            .take_changes()
            .iter()
            .filter_map(Change::removed)
            .collect();
        assert_eq!(removed, [old]);
        t.mkdir(b"/tmp/d").unwrap();
        let e = {
            t.mkdir(b"/tmp/e").unwrap();
            t.lookup(b"/tmp/e").unwrap()
        };
        t.rename(b"/tmp/d", b"/tmp/e").unwrap();
        let removed: Vec<_> = t
            .take_changes()
            .iter()
            .filter_map(Change::removed)
            .collect();
        assert_eq!(removed, [e]);
        // A rename onto itself and a failed removal change nothing.
        t.rename(b"/tmp/e", b"/tmp/e").unwrap();
        assert_eq!(t.unlink(b"/tmp/missing"), Err(Errno::ENOENT));
        assert_eq!(t.rmdir(b"/"), Err(Errno::EBUSY));
        assert!(
            t.take_changes().iter().all(|c| c.removed().is_none()),
            "at most a move"
        );
    }

    #[test]
    fn a_file_with_another_name_lives_on() {
        let mut fs = memfs();
        let root = fs.root();
        let two = fs.create(root, b"two").unwrap();
        fs.link(root, b"also", two).unwrap();
        let mut t = MountTable::new(Box::new(fs));
        t.unlink(b"/two").unwrap();
        assert!(t.take_changes().is_empty(), "/also still names it");
        t.unlink(b"/also").unwrap();
        let removed: Vec<_> = t
            .take_changes()
            .iter()
            .filter_map(Change::removed)
            .collect();
        assert_eq!(removed, [Node { mount: 0, ino: two }]);
    }

    #[test]
    fn a_second_filesystem_mounts_on_a_directory() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vfs -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Change` in this scope ``; `` no method named `take_changes` found for mutable reference `&mut mount::MountTable` in the current scope ``.

- [ ] **Step 3: Change `crates/vfs/src/lib.rs`**

In `crates/vfs/src/lib.rs`, replace:

````rust
pub use memfs::{MAX_FILE_SIZE, MemFs};
pub use mount::{Cwd, MountTable, Node, Vfs};
````

with:

````rust
pub use memfs::{MAX_FILE_SIZE, MemFs};
pub use mount::{Change, Cwd, MountTable, Node, Vfs};
````

- [ ] **Step 4: Change `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
/// (user-space gate §5.4), and the kernel puts it in with
/// `MountTable::swap_cwd` while it works for that process. A removal marks
/// only the current directory that is in the table at the time as gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cwd {
    trail: Trail,
    gone: bool,
}
````

with:

````rust
/// (user-space gate §5.4), and the kernel puts it in with
/// `MountTable::swap_cwd` while it works for that process. A removal or a
/// move changes the current directory in the table at the time; the others
/// learn of it from the table's [`Change`]s (`Cwd::follow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cwd {
    trail: Trail,
    gone: bool,
}

impl Cwd {
    /// Takes in a change made while another current directory was in the
    /// table: a removed directory on the way to this one leaves it gone,
    /// and a moved one takes its path along.
    pub fn follow(&mut self, change: &Change) {
        follow(&mut self.trail, &mut self.gone, change);
    }
}

/// What a removal or a move did that current directories and open files
/// elsewhere must learn of (user-space gate §16 item 4), since an inode a
/// removal frees can be reused at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change(What);

#[derive(Clone, Debug, PartialEq, Eq)]
enum What {
    /// A directory, or a file's last name, was removed.
    Removed(Node),
    /// A directory moved; `to` is the trail to its new place.
    Moved { node: Node, to: Trail },
}

impl Change {
    /// The inode a removal freed, if this is one: an open file of it is
    /// gone.
    pub fn removed(&self) -> Option<Node> {
        match self.0 {
            What::Removed(node) => Some(node),
            What::Moved { .. } => None,
        }
    }
}

/// Applies `change` to a current directory's trail. A gone one is not
/// followed any more: its inodes may be other files' now.
fn follow(trail: &mut Trail, gone: &mut bool, change: &Change) {
    if *gone {
        return;
    }
    match &change.0 {
        What::Removed(node) => {
            if trail.iter().any(|(n, _)| n == node) {
                *gone = true;
            }
        }
        What::Moved { node, to } => {
            if let Some(k) = trail.iter().position(|(n, _)| n == node) {
                let mut moved = to.clone();
                moved.extend_from_slice(&trail[k + 1..]);
                *trail = moved;
            }
        }
    }
}
````

Replace:

````rust
    cwd_gone: bool,
}
````

with:

````rust
    cwd_gone: bool,
    /// The removals and moves since `take_changes`.
    changes: Vec<Change>,
}
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
            changes: Vec::new(),
        }
    }

    /// The removals and moves made since the last call, oldest first, for
    /// the current directories and open files outside the table (the
    /// kernel asks after every operation).
    pub fn take_changes(&mut self) -> Vec<Change> {
        core::mem::take(&mut self.changes)
    }

    /// Records `change` and applies it to the current directory.
    fn changed(&mut self, change: What) {
        let change = Change(change);
        follow(&mut self.cwd, &mut self.cwd_gone, &change);
        self.changes.push(change);
    }

    /// Whether removing a name of `node` frees it: a directory, or a file's
    /// last link. One whose inode cannot be read counts as freed.
    fn frees(&mut self, node: Node) -> bool {
        self.stat(node)
            .map_or(true, |st| st.kind == FileType::Directory || st.nlink <= 1)
    }
````

Replace:

````rust
        }
    }

    fn in_cwd(&self, node: Node) -> bool {
        self.cwd.iter().any(|(n, _)| *n == node)
    }
````

with:

````rust
        }
    }
````

Replace:

````rust
        }
        if parent.trailing_slash
            && let Some(child) = self.child(&parent)?
            && !self.is_dir(child)?
        {
            return Err(Errno::ENOTDIR);
        }
        self.fs(parent.dir.mount)?.unlink(parent.dir.ino, name)
    }
````

with:

````rust
        }
        let child = self.child(&parent)?;
        if parent.trailing_slash
            && let Some(child) = child
            && !self.is_dir(child)?
        {
            return Err(Errno::ENOTDIR);
        }
        let frees = child.is_some_and(|c| self.frees(c));
        self.fs(parent.dir.mount)?.unlink(parent.dir.ino, name)?;
        if let Some(c) = child.filter(|_| frees) {
            self.changed(What::Removed(c));
        }
        Ok(())
    }
````

Replace:

````rust
        self.fs(parent.dir.mount)?.rmdir(parent.dir.ino, name)?;
        if child.is_some_and(|c| self.in_cwd(c)) {
            self.cwd_gone = true;
        }
````

with:

````rust
        self.fs(parent.dir.mount)?.rmdir(parent.dir.ino, name)?;
        if let Some(c) = child {
            self.changed(What::Removed(c));
        }
````

Replace:

````rust
        }
        let target = self.child(&dst)?;
        if target.is_some_and(|t| self.is_mount_point(t)) {
            return Err(Errno::EBUSY);
        }
        let (from_dir, to_dir) = (src.dir.ino, dst.dir.ino);
        self.fs(src.dir.mount)?
            .rename(from_dir, src_name, to_dir, dst_name)?;
        // A directory replaced by the rename is gone; one moved takes the
        // current directory's path along.
        if target.is_some_and(|t| t != moving && self.in_cwd(t)) {
            self.cwd_gone = true;
        } else if let Some(k) = self.cwd.iter().position(|(n, _)| *n == moving) {
            match self.walk(to) {
                Ok(mut trail) => {
                    trail.extend_from_slice(&self.cwd[k + 1..]);
                    self.cwd = trail;
                }
                Err(_) => self.cwd_gone = true,
            }
````

with:

````rust
        }
        let target = self.child(&dst)?.filter(|&t| t != moving);
        if target.is_some_and(|t| self.is_mount_point(t)) {
            return Err(Errno::EBUSY);
        }
        let replaced = target.filter(|&t| self.frees(t));
        let moves_dir = self.is_dir(moving)?;
        let (from_dir, to_dir) = (src.dir.ino, dst.dir.ino);
        self.fs(src.dir.mount)?
            .rename(from_dir, src_name, to_dir, dst_name)?;
        // What the rename replaced is gone; a directory moved takes the
        // current directories on its way along.
        if let Some(t) = replaced {
            self.changed(What::Removed(t));
        }
        if moves_dir {
            match self.walk(to) {
                Ok(to) => self.changed(What::Moved { node: moving, to }),
                Err(_) => self.changed(What::Removed(moving)),
            }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p vfs -p shell`

Expected: PASS: 180 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "vfs: a removal or a move is recorded for the current directories and open files outside the table"
````


### Task 7: Files of the VFS open in the fd table, with an offset their fds share

Spec §5.4's fd table held the console and the in-kernel shell's outputs; plan 3b adds files of the VFS (decision 2). `file::open` opens a path with `open`'s flags and their errors, Linux's where they apply; an `OpenFile` is the node, what it was opened for and a state behind a lock (the offset, the last name `read_dir` gave, whether a removal freed its inode), shared by every fd that shares the open file (an `Arc<File>`), while each `open` makes its own. `read` and `write` move the offset (`write` at the end when opened to append; fewer bytes when the filesystem fills up, then `ENOSPC`); `seek` goes anywhere from the start to 2^63 − 1, a directory only back to its start (decision 3); `stat` gives `relay_abi::Stat`; `read_dir` gives sorted records after the last name it gave, so an entry there throughout comes exactly once, and refuses a directory the kernel's heap has no room for (decision 4); a file marked gone is `ENOENT` for everything. The `Vfs` trait gains `entry_kind`, the kind of a directory's entry, where a name something is mounted on is a directory whatever its inode number means in the filesystem below (`/bin`'s root is 1, the bad-blocks inode of the ext2 root below it). `FdTable` gains `insert` (the lowest free fd, `EMFILE` at 32), `remove` and `files`. The tests run over a `MemFs`, with a read-only one at `/bin` and one that fills up; their loops are bounded, since a `read_dir` that never ended took the machine down under a mutant. Mutation checks: each of `open`'s flag rules, `EEXIST`, `EISDIR`, `ENOTDIR`, `TRUNCATE`, `APPEND`, a negative seek, a read that does not move the offset, `read_dir` repeating its last name, its heap check, its `EINVAL`, the gone, read and write checks, a directory's seek to 0 not starting over, no sort, and `insert` taking the highest fd each fail a test; `entry_kind` without the mount-point case survived until a test mounted a filesystem whose root is 7 on one whose 7 is a file; `write`'s own `EFBIG` check survived, and was dropped as redundant (the filesystem's answers).

**Files:**
- Modify: `crates/vfs/src/mount.rs`
- Modify: `kernel/src/fd.rs`
- Create: `kernel/src/file.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/mounts.rs`
- Modify: `kernel/src/proc.rs`

**Interfaces:**
- Consumes: Tasks 1 (`relay_abi::file`), 6; plan 3a's `fd::{File, FdTable}`.
- Produces: `file::{open(&mut dyn Vfs, &[u8], u32) -> Result<OpenFile, Errno>, kind(FileType) -> u8, stat_of(&vfs::Stat) -> relay_abi::Stat, OpenFile::{node, is_readable, is_writable, is_dir, mark_gone, read, write, write_all, seek(&self, &mut dyn Vfs, i64, u32) -> Result<u64, Errno>, stat, read_dir(&self, &mut dyn Vfs, &mut [u8], room: usize) -> Result<usize, Errno>}}`; `fd::File::Vfs(OpenFile)`, `FdTable::{insert(Arc<File>) -> Result<u32, Errno>, remove(u64) -> Result<Arc<File>, Errno>, files()}`; `vfs::Vfs::entry_kind(&mut self, Node, &DirEntry) -> Result<FileType, Errno>`.

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, replace:

````rust
    }

    #[test]
    fn a_filesystem_mounts_on_a_name_its_directory_does_not_have() {
````

with:

````rust
    }

    /// An empty read-only filesystem whose root is inode 7, as ext2's is 2:
    /// the number means something else in the filesystem it is mounted in.
    struct Seven;

    impl FileSystem for Seven {
        fn root(&self) -> Ino {
            7
        }
        fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
            if ino != 7 {
                return Err(Errno::ENOENT);
            }
            Ok(Stat {
                ino,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: 0,
                gid: 0,
                size: 0,
                blocks: 0,
                block_size: 4096,
                atime: 0,
                mtime: 0,
                ctime: 0,
            })
        }
        fn lookup(&mut self, _: Ino, name: &[u8]) -> Result<Ino, Errno> {
            match name {
                b"." | b".." => Ok(7),
                _ => Err(Errno::ENOENT),
            }
        }
        fn read_dir(&mut self, _: Ino) -> Result<Vec<DirEntry>, Errno> {
            Ok(alloc::vec![
                DirEntry {
                    name: b".".to_vec(),
                    ino: 7
                },
                DirEntry {
                    name: b"..".to_vec(),
                    ino: 7
                },
            ])
        }
        fn read_link(&mut self, _: Ino) -> Result<Vec<u8>, Errno> {
            Err(Errno::EINVAL)
        }
        fn read_at(&mut self, _: Ino, _: u64, _: &mut [u8]) -> Result<usize, Errno> {
            Err(Errno::EISDIR)
        }
        fn write_at(&mut self, _: Ino, _: u64, _: &[u8]) -> Result<usize, Errno> {
            Err(Errno::EROFS)
        }
        fn truncate(&mut self, _: Ino, _: u64) -> Result<(), Errno> {
            Err(Errno::EROFS)
        }
        fn touch(&mut self, _: Ino) -> Result<(), Errno> {
            Err(Errno::EROFS)
        }
        fn create(&mut self, _: Ino, _: &[u8]) -> Result<Ino, Errno> {
            Err(Errno::EROFS)
        }
        fn mkdir(&mut self, _: Ino, _: &[u8]) -> Result<Ino, Errno> {
            Err(Errno::EROFS)
        }
        fn unlink(&mut self, _: Ino, _: &[u8]) -> Result<(), Errno> {
            Err(Errno::EROFS)
        }
        fn rmdir(&mut self, _: Ino, _: &[u8]) -> Result<(), Errno> {
            Err(Errno::EROFS)
        }
        fn rename(&mut self, _: Ino, _: &[u8], _: Ino, _: &[u8]) -> Result<(), Errno> {
            Err(Errno::EROFS)
        }
        fn statfs(&mut self) -> Result<StatFs, Errno> {
            Err(Errno::EINVAL)
        }
        fn sync(&mut self) -> Result<(), Errno> {
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), Errno> {
            Ok(())
        }
    }

    #[test]
    fn a_mount_on_a_name_is_a_directory_whatever_its_number_means_below() {
        let mut t = table();
        // Inode 7 of the table's own MemFs is a file.
        let root = t.lookup(b"/").unwrap();
        let mut n = 0;
        while n < 10 && t.stat(Node { mount: 0, ino: 7 }).is_err() {
            t.create(format!("/f{n}").as_bytes()).unwrap();
            n += 1;
        }
        assert_eq!(
            t.stat(Node { mount: 0, ino: 7 }).unwrap().kind,
            FileType::Regular
        );
        t.mount(b"/seven", Box::new(Seven)).unwrap();
        let entry = t
            .read_dir(root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == b"seven")
            .unwrap();
        assert_eq!(entry.ino, 7);
        assert_eq!(t.entry_kind(root, &entry), Ok(FileType::Directory));
    }

    #[test]
    fn entry_kinds_come_from_the_entries_and_the_mounts() {
        let mut t = table();
        t.mount(b"/bin", Box::new(programs())).unwrap();
        let root = t.lookup(b"/").unwrap();
        let kinds: Vec<(Vec<u8>, FileType)> = t
            .read_dir(root)
            .unwrap()
            .iter()
            .map(|e| (e.name.clone(), t.entry_kind(root, e).unwrap()))
            .collect();
        for (name, kind) in [
            (&b"."[..], FileType::Directory),
            (b"..", FileType::Directory),
            (b"etc", FileType::Directory),
            (b"bin", FileType::Directory),
        ] {
            assert!(kinds.contains(&(name.to_vec(), kind)), "{name:?}");
        }
        let home = t.lookup(b"/root").unwrap();
        let link = DirEntry {
            name: b"link".to_vec(),
            ino: t.lookup(b"/root/link").unwrap().ino,
        };
        assert_eq!(t.entry_kind(home, &link), Ok(FileType::Symlink));
        let etc = t.lookup(b"/etc").unwrap();
        let motd = DirEntry {
            name: b"motd".to_vec(),
            ino: t.lookup(b"/etc/motd").unwrap().ino,
        };
        assert_eq!(t.entry_kind(etc, &motd), Ok(FileType::Regular));
        let bad = DirEntry {
            name: b"x".to_vec(),
            ino: 999,
        };
        assert_eq!(t.entry_kind(etc, &bad), Err(Errno::ENOENT));
    }

    #[test]
    fn a_filesystem_mounts_on_a_name_its_directory_does_not_have() {
````

- [ ] **Step 2: Add the failing tests to `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
    #[test]
    fn a_bad_mapping_is_refused() {
````

with:

````rust
    #[test]
    fn a_file_opened_takes_the_lowest_free_fd_up_to_32() {
        let mut t = FdTable::shell();
        let console = || Arc::new(File::Console);
        assert_eq!(t.insert(console()), Ok(3));
        assert_eq!(t.remove(1).map(|f| *f == File::ShellOutput(1)), Ok(true));
        assert_eq!(t.insert(console()), Ok(1), "the lowest free one");
        for fd in 4..32 {
            assert_eq!(t.insert(console()), Ok(fd));
        }
        assert_eq!(t.insert(console()), Err(Errno::EMFILE));
        assert_eq!(t.open(), 32);
        assert_eq!(t.files().count(), 32);
        assert!(t.remove(31).is_ok());
        assert_eq!(t.remove(31).err(), Some(Errno::EBADF), "closed already");
        for fd in [32, 1 << 32, u64::MAX] {
            assert_eq!(t.remove(fd).err(), Some(Errno::EBADF), "{fd}");
        }
        assert_eq!(t.insert(console()), Ok(31));
    }

    #[test]
    fn a_bad_mapping_is_refused() {
````

- [ ] **Step 3: Write the failing tests for `kernel/src/file.rs`**

Create `kernel/src/file.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use relay_abi::file::{DirEntry, dir_entries};
    use vfs::{Env, FileSystem, MemFs, MountTable};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            1_000
        }
        fn log(&self, _: &str) {}
    }

    const RW: u32 = OPEN_READ | OPEN_WRITE;

    /// `/root` with `/root/f` holding "hello", a symlink `/root/l`, and a
    /// read-only filesystem at `/bin` holding `prog`.
    fn table() -> MountTable {
        table_on(MemFs::new(Box::new(Clock)))
    }

    fn table_on(fs: MemFs) -> MountTable {
        let mut fs = fs;
        let root = fs.root();
        let home = fs.mkdir(root, b"root").unwrap();
        let f = fs.create(home, b"f").unwrap();
        fs.write_at(f, 0, b"hello").unwrap();
        fs.symlink(home, b"l", b"f").unwrap();
        let mut bin = MemFs::new(Box::new(Clock));
        let r = bin.root();
        bin.create(r, b"prog").unwrap();
        let mut t = MountTable::new(Box::new(fs));
        t.mount(b"/bin", Box::new(bin.read_only())).unwrap();
        t
    }

    fn read_all(t: &mut MountTable, f: &OpenFile) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0; 3];
        loop {
            assert!(out.len() < 1 << 20, "a read that never ends");
            match f.read(t, &mut buf).unwrap() {
                0 => return out,
                n => out.extend_from_slice(&buf[..n]),
            }
        }
    }

    #[test]
    fn open_s_flags_are_checked() {
        let mut t = table();
        for bad in [
            0,
            OPEN_CREATE,
            OPEN_READ | OPEN_TRUNCATE,
            OPEN_READ | OPEN_APPEND,
            OPEN_READ | OPEN_EXCLUSIVE,
            RW | OPEN_CREATE | OPEN_DIRECTORY,
            OPEN_READ | 128,
            u32::MAX,
        ] {
            assert_eq!(
                open(&mut t, b"/root/f", bad).err(),
                Some(Errno::EINVAL),
                "{bad:#x}"
            );
        }
        assert_eq!(
            open(&mut t, b"/root/nope", OPEN_READ).err(),
            Some(Errno::ENOENT)
        );
        assert_eq!(
            open(&mut t, b"/root/f", RW | OPEN_CREATE | OPEN_EXCLUSIVE).err(),
            Some(Errno::EEXIST)
        );
        assert_eq!(
            open(&mut t, b"/root", OPEN_WRITE).err(),
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f", OPEN_READ | OPEN_DIRECTORY).err(),
            Some(Errno::ENOTDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/f/", OPEN_READ).err(),
            Some(Errno::ENOTDIR)
        );
        assert_eq!(
            open(&mut t, b"/root/new/", OPEN_WRITE | OPEN_CREATE).err(),
            Some(Errno::EISDIR)
        );
        assert_eq!(
            open(&mut t, b"/bin/new", OPEN_WRITE | OPEN_CREATE).err(),
            Some(Errno::EROFS)
        );
        assert_eq!(
            open(&mut t, b"/bin/prog", OPEN_WRITE | OPEN_TRUNCATE).err(),
            Some(Errno::EROFS)
        );
        // What may be opened.
        let d = open(&mut t, b"/root", OPEN_READ | OPEN_DIRECTORY).unwrap();
        assert!(d.is_dir() && d.is_readable() && !d.is_writable());
        assert!(open(&mut t, b"/root", OPEN_READ).unwrap().is_dir());
        assert!(open(&mut t, b"/bin/prog", OPEN_READ).is_ok());
        let f = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert!(!f.is_readable() && f.is_writable() && !f.is_dir());
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(read_all(&mut t, &r), b"hello");
    }

    #[test]
    fn create_makes_a_file_and_truncate_empties_one() {
        let mut t = table();
        let f = open(
            &mut t,
            b"/root/new",
            OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
        )
        .unwrap();
        assert_eq!(f.write(&mut t, b"abc"), Ok(3));
        let node = t.lookup(b"/root/new").unwrap();
        assert_eq!(f.node(), node);
        let g = open(&mut t, b"/root/new", RW | OPEN_TRUNCATE).unwrap();
        assert_eq!(read_all(&mut t, &g), b"");
        assert_eq!(t.stat(node).unwrap().size, 0);
        // Without TRUNCATE it stays.
        f.write(&mut t, b"xyz").unwrap();
        let h = open(&mut t, b"/root/new", RW | OPEN_CREATE).unwrap();
        assert_eq!(read_all(&mut t, &h), b"\0\0\0xyz", "f's offset was 3");
    }

    #[test]
    fn reads_and_writes_move_the_offset_that_sharers_share() {
        let mut t = table();
        let f = alloc::sync::Arc::new(open(&mut t, b"/root/f", RW).unwrap());
        let shared = alloc::sync::Arc::clone(&f);
        let mut buf = [0; 2];
        assert_eq!(f.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"he");
        assert_eq!(shared.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"ll", "the same offset");
        // Another open has its own.
        let other = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(other.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf, b"he");
        assert_eq!(f.write(&mut t, b"O!"), Ok(2));
        assert_eq!(f.read(&mut t, &mut buf), Ok(0), "at the end");
        assert_eq!(read_all(&mut t, &other), b"llO!");
        assert_ne!(*f, other);
        assert_eq!(*f, *shared);
    }

    #[test]
    fn append_writes_at_the_end_whatever_the_offset() {
        let mut t = table();
        let a = open(&mut t, b"/root/f", OPEN_WRITE | OPEN_APPEND).unwrap();
        let w = open(&mut t, b"/root/f", OPEN_WRITE).unwrap();
        a.write(&mut t, b" world").unwrap();
        w.write(&mut t, b"J").unwrap();
        a.seek(&mut t, 0, SEEK_START).unwrap();
        a.write(&mut t, b"!").unwrap();
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(read_all(&mut t, &r), b"Jello world!");
    }

    #[test]
    fn seek_goes_anywhere_from_the_start_on() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", RW).unwrap();
        assert_eq!(f.seek(&mut t, 1, SEEK_START), Ok(1));
        assert_eq!(f.seek(&mut t, 2, SEEK_CURRENT), Ok(3));
        assert_eq!(f.seek(&mut t, -1, SEEK_CURRENT), Ok(2));
        assert_eq!(f.seek(&mut t, -2, SEEK_END), Ok(3));
        let mut buf = [0; 8];
        assert_eq!(f.read(&mut t, &mut buf), Ok(2));
        assert_eq!(&buf[..2], b"lo");
        // Past the end: reads nothing, and a write leaves a hole.
        assert_eq!(f.seek(&mut t, 3, SEEK_END), Ok(8));
        assert_eq!(f.read(&mut t, &mut buf), Ok(0));
        f.write(&mut t, b"!").unwrap();
        f.seek(&mut t, 0, SEEK_START).unwrap();
        assert_eq!(read_all(&mut t, &f), b"hello\0\0\0!");
        for (off, whence) in [(-1, SEEK_START), (-10, SEEK_END), (0, 3), (1, u32::MAX)] {
            assert_eq!(
                f.seek(&mut t, off, whence),
                Err(Errno::EINVAL),
                "{off} {whence}"
            );
        }
        f.seek(&mut t, i64::MAX, SEEK_START).unwrap();
        assert_eq!(
            f.seek(&mut t, 1, SEEK_CURRENT),
            Err(Errno::EINVAL),
            "past 2^63 - 1"
        );
        assert_eq!(
            f.seek(&mut t, 0, SEEK_CURRENT),
            Ok(i64::MAX as u64),
            "unchanged"
        );
        assert_eq!(f.write(&mut t, b"x"), Err(Errno::EFBIG));
    }

    #[test]
    fn what_a_file_was_not_opened_for_is_ebadf() {
        let mut t = table();
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(r.write(&mut t, b"x"), Err(Errno::EBADF));
        let w = open(&mut t, b"/root/f", OPEN_WRITE).unwrap();
        assert_eq!(w.read(&mut t, &mut [0; 4]), Err(Errno::EBADF));
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(d.read(&mut t, &mut [0; 4]), Err(Errno::EISDIR));
        assert_eq!(
            r.read_dir(&mut t, &mut [0; 64], usize::MAX),
            Err(Errno::ENOTDIR)
        );
        // A symbolic link is never followed: reading one is the
        // filesystem's EINVAL.
        let l = open(&mut t, b"/root/l", OPEN_READ).unwrap();
        assert_eq!(l.read(&mut t, &mut [0; 4]), Err(Errno::EINVAL));
    }

    #[test]
    fn a_full_filesystem_gives_a_short_write_then_enospc() {
        let mut t = table_on(MemFs::new(Box::new(Clock)).with_capacity(8192));
        let f = open(&mut t, b"/root/big", OPEN_WRITE | OPEN_CREATE).unwrap();
        let n = f.write(&mut t, &[b'x'; 10_000]).unwrap();
        assert!(n < 10_000, "{n}");
        assert_eq!(f.write(&mut t, &[b'x'; 10_000]), Err(Errno::ENOSPC));
        assert_eq!(f.write_all(&mut t, b"y"), Err(Errno::ENOSPC));
        assert_eq!(
            f.seek(&mut t, 0, SEEK_CURRENT),
            Ok(n as u64),
            "at what was written"
        );
    }

    #[test]
    fn stat_maps_every_field() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        let s = t.stat(f.node()).unwrap();
        let got = f.stat(&mut t).unwrap();
        assert_eq!(
            got,
            relay_abi::Stat {
                ino: s.ino,
                size: 5,
                blocks: s.blocks,
                atime: 1_000,
                mtime: 1_000,
                ctime: 1_000,
                kind: u32::from(KIND_REGULAR),
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                block_size: s.block_size,
            }
        );
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(d.stat(&mut t).unwrap().kind, u32::from(KIND_DIRECTORY));
        assert_eq!(kind(FileType::Symlink), KIND_SYMLINK);
        assert_eq!(kind(FileType::Fifo), KIND_FIFO);
    }

    /// The names and kinds `read_dir` gives with a buffer of `size` bytes,
    /// call by call. A `read_dir` that never gets to the end fails the test
    /// instead of running for ever.
    fn listing(t: &mut MountTable, d: &OpenFile, size: usize) -> Vec<Vec<(Vec<u8>, u8)>> {
        let mut calls = Vec::new();
        let mut buf = vec![0; size];
        loop {
            assert!(calls.len() < 100, "read_dir never ends: {:?}", &calls[..3]);
            match d.read_dir(t, &mut buf, usize::MAX).unwrap() {
                0 => return calls,
                n => calls.push(
                    dir_entries(&buf[..n])
                        .map(|r| (r.name.to_vec(), r.kind))
                        .collect(),
                ),
            }
        }
    }

    #[test]
    fn read_dir_gives_sorted_records_and_goes_on_where_it_stopped() {
        let mut t = table();
        let root = open(&mut t, b"/", OPEN_READ | OPEN_DIRECTORY).unwrap();
        let calls = listing(&mut t, &root, 4096);
        assert_eq!(
            calls,
            [vec![
                (b".".to_vec(), KIND_DIRECTORY),
                (b"..".to_vec(), KIND_DIRECTORY),
                (b"bin".to_vec(), KIND_DIRECTORY),
                (b"root".to_vec(), KIND_DIRECTORY),
            ]]
        );
        let home = open(&mut t, b"/root", OPEN_READ).unwrap();
        // Room for one record at a time.
        let one = DirEntry::record_len(2);
        let calls = listing(&mut t, &home, one);
        assert_eq!(
            calls,
            [
                vec![(b".".to_vec(), KIND_DIRECTORY)],
                vec![(b"..".to_vec(), KIND_DIRECTORY)],
                vec![(b"f".to_vec(), KIND_REGULAR)],
                vec![(b"l".to_vec(), KIND_SYMLINK)],
            ]
        );
        // At the end until it goes back to the start.
        assert_eq!(home.read_dir(&mut t, &mut [0; 64], usize::MAX), Ok(0));
        assert_eq!(home.seek(&mut t, 0, SEEK_START), Ok(0));
        assert_eq!(listing(&mut t, &home, 4096)[0].len(), 4);
        for (off, whence) in [(1, SEEK_START), (0, SEEK_END), (0, SEEK_CURRENT)] {
            assert_eq!(home.seek(&mut t, off, whence), Err(Errno::EINVAL));
        }
    }

    #[test]
    fn an_entry_there_throughout_comes_once_however_the_directory_changes() {
        let mut t = table();
        for name in ["a", "c", "e", "g"] {
            t.create(&[b"/root/", name.as_bytes()].concat()).unwrap();
        }
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        let mut buf = vec![0; 2 * DirEntry::record_len(1)];
        let mut names = Vec::new();
        let mut take = |t: &mut MountTable| {
            let n = d.read_dir(t, &mut buf, usize::MAX).unwrap();
            let got: Vec<Vec<u8>> = dir_entries(&buf[..n]).map(|r| r.name.to_vec()).collect();
            names.extend(got.clone());
            got
        };
        assert_eq!(take(&mut t), [b".".to_vec(), b"..".to_vec()]);
        assert_eq!(take(&mut t), [b"a".to_vec(), b"c".to_vec()]);
        // Removing what was given and adding before and after the place it
        // stopped.
        t.unlink(b"/root/a").unwrap();
        t.create(b"/root/b").unwrap();
        t.create(b"/root/d").unwrap();
        assert_eq!(take(&mut t), [b"d".to_vec(), b"e".to_vec()]);
        assert_eq!(take(&mut t), [b"f".to_vec(), b"g".to_vec()]);
        assert_eq!(take(&mut t), [b"l".to_vec()]);
        assert!(take(&mut t).is_empty());
        for n in ["c", "e", "f", "g", "l"] {
            assert_eq!(
                names.iter().filter(|x| *x == n.as_bytes()).count(),
                1,
                "{n}"
            );
        }
    }

    #[test]
    fn read_dir_needs_room_for_one_record_and_the_heap_for_the_directory() {
        let mut t = table();
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        assert_eq!(
            d.read_dir(&mut t, &mut [0; DirEntry::SIZE], usize::MAX),
            Err(Errno::EINVAL),
            "not even `.` fits"
        );
        let size = t.stat(d.node()).unwrap().size;
        assert!(size > 0);
        let need = (size * DIR_MEMORY_FACTOR) as usize;
        assert_eq!(
            d.read_dir(&mut t, &mut [0; 64], need - 1),
            Err(Errno::ENOMEM)
        );
        assert!(d.read_dir(&mut t, &mut [0; 64], need).unwrap() > 0);
    }

    #[test]
    fn a_file_marked_gone_is_enoent_for_everything() {
        let mut t = table();
        let f = open(&mut t, b"/root/f", RW).unwrap();
        let d = open(&mut t, b"/root", OPEN_READ).unwrap();
        f.mark_gone();
        d.mark_gone();
        assert_eq!(f.read(&mut t, &mut [0; 4]), Err(Errno::ENOENT));
        assert_eq!(f.write(&mut t, b"x"), Err(Errno::ENOENT));
        assert_eq!(f.seek(&mut t, 0, SEEK_START), Err(Errno::ENOENT));
        assert_eq!(f.stat(&mut t).err(), Some(Errno::ENOENT));
        assert_eq!(
            d.read_dir(&mut t, &mut [0; 64], usize::MAX),
            Err(Errno::ENOENT)
        );
        assert_eq!(d.seek(&mut t, 0, SEEK_START), Err(Errno::ENOENT));
        let r = open(&mut t, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(
            read_all(&mut t, &r),
            b"hello",
            "the file itself is untouched"
        );
    }
}
````

- [ ] **Step 4: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod fd;
pub mod input;
````

with:

````rust
pub mod fd;
pub mod file;
pub mod input;
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p vfs -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `entry_kind` found for struct `mount::MountTable` in the current scope ``.

- [ ] **Step 6: Change `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno>;
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno>;
````

with:

````rust
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno>;
    /// What kind of file `entry`, one of `dir`'s entries, is. A name
    /// something is mounted on, `.` and `..` are directories.
    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno>;
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno>;
````

Replace:

````rust
        Ok(entries)
    }
````

with:

````rust
        Ok(entries)
    }

    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno> {
        if entry.name == b"." || entry.name == b".." {
            return Ok(FileType::Directory);
        }
        if self.mounted_at_name(dir, &entry.name).is_some() {
            return Ok(FileType::Directory);
        }
        let node = Node {
            mount: dir.mount,
            ino: entry.ino,
        };
        Ok(self.stat(node)?.kind)
    }
````

- [ ] **Step 7: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent. Plan 3a's files are the console and
//! the in-kernel shell's output; plan 3b adds files of the VFS, milestone 3
//! pipe ends.

use alloc::sync::Arc;
````

with:

````rust
//! shared reference to an open file, so a child `spawn` hands an fd to
//! uses the same file as its parent, offset and all. The files are the
//! console, the in-kernel shell's output and files of the VFS; milestone 3
//! adds pipe ends.

use crate::file::OpenFile;
use alloc::sync::Arc;
````

Replace:

````rust
    ShellOutput(u32),
}
````

with:

````rust
    ShellOutput(u32),
    /// A file of the VFS.
    Vfs(OpenFile),
}
````

Replace:

````rust
        self.slots[fd] = Some(file);
    }
````

with:

````rust
        self.slots[fd] = Some(file);
    }

    /// Opens `file` as the lowest fd that is free; `EMFILE` if all 32 are
    /// in use (spec §11.1).
    pub fn insert(&mut self, file: Arc<File>) -> Result<u32, Errno> {
        let fd = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Errno::EMFILE)?;
        self.slots[fd] = Some(file);
        Ok(fd as u32)
    }

    /// Closes `fd`; the file, which lives on while anything else has it.
    /// `EBADF` if `fd` is not open.
    pub fn remove(&mut self, fd: u64) -> Result<Arc<File>, Errno> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
            .and_then(Option::take)
            .ok_or(Errno::EBADF)
    }

    /// Every open file.
    pub fn files(&self) -> impl Iterator<Item = &Arc<File>> {
        self.slots.iter().flatten()
    }
````

- [ ] **Step 8: Implement `kernel/src/file.rs`**

Insert this at the top of `kernel/src/file.rs`, above `#[cfg(test)]`:

````rust
//! Files of the VFS as processes have them open (user-space gate §7.3,
//! §16 item 4): a node, what the file was opened for, and an offset that
//! every fd sharing the open file shares (the fds `spawn` hands a child);
//! each `open` makes a new one with an offset of its own, as on Linux.
//!
//! `open`'s flags and their errors are Linux's where they apply. `seek`
//! may go past the end (a write there leaves a hole). `read_dir` gives a
//! directory's entries sorted by name and continues after the last name it
//! gave, so an entry that is there throughout comes exactly once however
//! the directory changes between calls.
//!
//! A removal elsewhere can free the file's inode, which the filesystem may
//! then give to a new file; the kernel marks every open file of it gone,
//! and from then on everything but closing it is `ENOENT`, what the
//! filesystem says for an inode it has freed.
//!
//! The functions take the `Vfs` to use, so they are tested over a `MemFs`.

use alloc::vec::Vec;
use core::fmt;
use relay_abi::file::{
    KIND_BLOCK_DEVICE, KIND_CHAR_DEVICE, KIND_DIRECTORY, KIND_FIFO, KIND_REGULAR, KIND_SOCKET,
    KIND_SYMLINK, KIND_UNKNOWN, OPEN_APPEND, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE,
    OPEN_FLAGS, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE, SEEK_CURRENT, SEEK_END, SEEK_START,
    put_dir_entry,
};
use spin::Mutex;
use vfs::{Errno, FileType, Node, Vfs};

/// A file of the VFS, open.
pub struct OpenFile {
    node: Node,
    read: bool,
    write: bool,
    append: bool,
    dir: bool,
    state: Mutex<State>,
}

struct State {
    offset: u64,
    /// The last name `read_dir` gave.
    after: Option<Vec<u8>>,
    /// A removal freed the inode.
    gone: bool,
}

impl fmt::Debug for OpenFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpenFile({:?})", self.node)
    }
}

/// Open files are the same only if they are one (fds that share it).
impl PartialEq for OpenFile {
    fn eq(&self, other: &OpenFile) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for OpenFile {}

/// Opens `path` with `open`'s `flags` (spec §7.3).
pub fn open(vfs: &mut dyn Vfs, path: &[u8], flags: u32) -> Result<OpenFile, Errno> {
    let has = |f: u32| flags & f != 0;
    let valid = flags & !OPEN_FLAGS == 0
        && (has(OPEN_READ) || has(OPEN_WRITE))
        && (has(OPEN_WRITE) || !has(OPEN_TRUNCATE | OPEN_APPEND))
        && (has(OPEN_CREATE) || !has(OPEN_EXCLUSIVE))
        && !(has(OPEN_CREATE) && has(OPEN_DIRECTORY));
    if !valid {
        return Err(Errno::EINVAL);
    }
    let (node, created) = match vfs.lookup(path) {
        Ok(_) if has(OPEN_CREATE) && has(OPEN_EXCLUSIVE) => return Err(Errno::EEXIST),
        Ok(node) => (node, false),
        Err(Errno::ENOENT) if has(OPEN_CREATE) => (vfs.create(path)?, true),
        Err(e) => return Err(e),
    };
    let dir = !created && vfs.stat(node)?.kind == FileType::Directory;
    if dir && has(OPEN_WRITE) {
        return Err(Errno::EISDIR);
    }
    if !dir && has(OPEN_DIRECTORY) {
        return Err(Errno::ENOTDIR);
    }
    if has(OPEN_TRUNCATE) && !created {
        vfs.truncate(node, 0)?;
    }
    Ok(OpenFile {
        node,
        read: has(OPEN_READ),
        write: has(OPEN_WRITE),
        append: has(OPEN_APPEND),
        dir,
        state: Mutex::new(State {
            offset: 0,
            after: None,
            gone: false,
        }),
    })
}

/// A `KIND_*` number.
pub fn kind(t: FileType) -> u8 {
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

/// `stat`'s answer for a program.
pub fn stat_of(s: &vfs::Stat) -> relay_abi::Stat {
    relay_abi::Stat {
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
    }
}

/// How much bigger than a directory's size its entries may be in memory
/// (an ext2 entry of 12 bytes becomes about 48): `read_dir` refuses a
/// directory the heap has no room for, since the kernel's heap panics when
/// it runs out.
const DIR_MEMORY_FACTOR: u64 = 4;

impl OpenFile {
    pub fn node(&self) -> Node {
        self.node
    }

    pub fn is_readable(&self) -> bool {
        self.read
    }

    pub fn is_writable(&self) -> bool {
        self.write
    }

    pub fn is_dir(&self) -> bool {
        self.dir
    }

    /// A removal freed the file's inode.
    pub fn mark_gone(&self) {
        self.state.lock().gone = true;
    }

    /// The offset now, after checking the file is still there.
    fn offset(&self) -> Result<u64, Errno> {
        let s = self.state.lock();
        if s.gone {
            return Err(Errno::ENOENT);
        }
        Ok(s.offset)
    }

    /// Reads from the offset, which moves past what was read; 0 at or past
    /// the end.
    pub fn read(&self, vfs: &mut dyn Vfs, buf: &mut [u8]) -> Result<usize, Errno> {
        let at = self.offset()?;
        if !self.read {
            return Err(Errno::EBADF);
        }
        if self.dir {
            return Err(Errno::EISDIR);
        }
        let n = vfs.read_at(self.node, at, buf)?;
        self.state.lock().offset = at + n as u64;
        Ok(n)
    }

    /// Writes at the offset (at the end if opened to append), which moves
    /// past what was written. Fewer bytes than asked when the filesystem
    /// fills up; `ENOSPC` when not one fits.
    pub fn write(&self, vfs: &mut dyn Vfs, bytes: &[u8]) -> Result<usize, Errno> {
        let mut at = self.offset()?;
        if !self.write {
            return Err(Errno::EBADF);
        }
        if self.append {
            at = vfs.stat(self.node)?.size;
        }
        let n = vfs.write_at(self.node, at, bytes)?;
        self.state.lock().offset = at + n as u64;
        Ok(n)
    }

    /// Writes all of `bytes`, as a tee does: `ENOSPC` if the filesystem
    /// fills up first.
    pub fn write_all(&self, vfs: &mut dyn Vfs, mut bytes: &[u8]) -> Result<(), Errno> {
        while !bytes.is_empty() {
            match self.write(vfs, bytes)? {
                0 => return Err(Errno::ENOSPC),
                n => bytes = &bytes[n..],
            }
        }
        Ok(())
    }

    /// Moves the offset (spec §7.3): from the start, the offset or the end.
    /// Past the end is allowed; before the start, past 2^63 − 1 and an
    /// unknown `whence` are `EINVAL`. A directory only goes back to its
    /// start, where `read_dir` begins again.
    pub fn seek(&self, vfs: &mut dyn Vfs, offset: i64, whence: u32) -> Result<u64, Errno> {
        let at = self.offset()?;
        if self.dir {
            if (offset, whence) != (0, SEEK_START) {
                return Err(Errno::EINVAL);
            }
            let mut s = self.state.lock();
            s.offset = 0;
            s.after = None;
            return Ok(0);
        }
        let base = match whence {
            SEEK_START => 0,
            SEEK_CURRENT => at,
            SEEK_END => vfs.stat(self.node)?.size,
            _ => return Err(Errno::EINVAL),
        };
        let to = i64::try_from(base)
            .ok()
            .and_then(|b| b.checked_add(offset))
            .filter(|&to| to >= 0)
            .ok_or(Errno::EINVAL)? as u64;
        self.state.lock().offset = to;
        Ok(to)
    }

    pub fn stat(&self, vfs: &mut dyn Vfs) -> Result<relay_abi::Stat, Errno> {
        self.offset()?;
        Ok(stat_of(&vfs.stat(self.node)?))
    }

    /// Fills `buf` with the directory's next entries (spec §7.3), sorted by
    /// name, as `relay_abi::file` records: the bytes written, 0 after the
    /// last. `EINVAL` if not even the next one fits; `ENOMEM` if the
    /// directory is too big for the `room` bytes of heap there are.
    pub fn read_dir(&self, vfs: &mut dyn Vfs, buf: &mut [u8], room: usize) -> Result<usize, Errno> {
        self.offset()?;
        if !self.dir {
            return Err(Errno::ENOTDIR);
        }
        let size = vfs.stat(self.node)?.size;
        if size.saturating_mul(DIR_MEMORY_FACTOR) > room as u64 {
            return Err(Errno::ENOMEM);
        }
        let mut entries = vfs.read_dir(self.node)?;
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let after = self.state.lock().after.clone();
        let start = match &after {
            Some(last) => entries.partition_point(|e| e.name <= *last),
            None => 0,
        };
        let mut done = 0;
        let mut last = None;
        for e in &entries[start..] {
            let k = vfs.entry_kind(self.node, e).map_or(KIND_UNKNOWN, kind);
            match put_dir_entry(&mut buf[done..], e.ino, k, &e.name) {
                Some(n) => {
                    done += n;
                    last = Some(&e.name);
                }
                None if done == 0 => return Err(Errno::EINVAL),
                None => break,
            }
        }
        if let Some(name) = last {
            self.state.lock().after = Some(name.clone());
        }
        Ok(done)
    }
}

````

- [ ] **Step 9: Change `kernel/src/mounts.rs`**

In `kernel/src/mounts.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use spin::Mutex;
use vfs::{Cwd, DirEntry, Errno, MountTable, Node, Stat, StatFs, Vfs};

````

with:

````rust
use spin::Mutex;
use vfs::{Cwd, DirEntry, Errno, FileType, MountTable, Node, Stat, StatFs, Vfs};

````

Replace:

````rust
    }
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
````

with:

````rust
    }
    fn entry_kind(&mut self, dir: Node, entry: &DirEntry) -> Result<FileType, Errno> {
        self.with(|t| t.entry_kind(dir, entry))
    }
    fn read_link(&mut self, node: Node) -> Result<Vec<u8>, Errno> {
````

- [ ] **Step 10: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
                }
            }
        }
        Ok(())
````

with:

````rust
                }
            }
            File::Vfs(ref open) => return open.write_all(&mut KernelVfs, bytes),
        }
        Ok(())
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p vfs -p relay-kernel --lib`

Expected: PASS: 356 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates kernel
git commit -m "kernel: files of the VFS open in the fd table, with an offset their fds share"
````


### Task 8: `read_dir`'s heap check covers what the smallest entries take

Found by the prototype's review: `read_dir` refused a directory whose size, four times over, was more than the kernel's heap had room for, on the belief that an ext2 entry of 12 bytes takes about 48 in memory. It takes more: a `vfs::DirEntry` is 32 bytes, in a `Vec` that doubles as it grows, so the old and the new buffer hold three slots an entry at the peak, and its name takes a heap block of at least 16 bytes; the reviewer measured 7.7 times the directory's size for 40,002 entries of short names, 8.7 with the heap's rounding. So a directory that passed the check could still run the heap out, which panics; only a crafted disk (hard links) has that many entries in a directory the 2 GiB root can hold, but disk data is untrusted. The factor is now 10, and a test holds it to that layout (decision 4). Mutation check: a factor of 9 fails the test.

**Files:**
- Modify: `kernel/src/file.rs`

**Interfaces:**
- Consumes: Task 7.
- Produces: `file::DIR_MEMORY_FACTOR` of 10.

- [ ] **Step 1: Add the failing tests to `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
    #[test]
    fn a_file_marked_gone_is_enoent_for_everything() {
````

with:

````rust
    #[test]
    fn the_heap_check_covers_a_directory_of_the_smallest_entries() {
        // Found by the prototype's review: ext2's smallest entry is 12
        // bytes on disk (a name of up to 4). In memory it is a
        // `vfs::DirEntry` in a `Vec` that doubles as it grows, so the old
        // and the new buffer hold three slots an entry at the peak, and its
        // name takes a heap block of at least 16 bytes (7.7 times the
        // directory's size was measured, 8.7 with the heap's rounding).
        let per_entry = 3 * core::mem::size_of::<vfs::DirEntry>() as u64 + 16;
        assert!(
            DIR_MEMORY_FACTOR * 12 >= per_entry,
            "{DIR_MEMORY_FACTOR} x 12 bytes < {per_entry}"
        );
    }

    #[test]
    fn a_file_marked_gone_is_enoent_for_everything() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `file::tests::the_heap_check_covers_a_directory_of_the_smallest_entries`.

- [ ] **Step 3: Change `kernel/src/file.rs`**

In `kernel/src/file.rs`, replace:

````rust
/// How much bigger than a directory's size its entries may be in memory
/// (an ext2 entry of 12 bytes becomes about 48): `read_dir` refuses a
/// directory the heap has no room for, since the kernel's heap panics when
/// it runs out.
const DIR_MEMORY_FACTOR: u64 = 4;

````

with:

````rust
/// How much bigger than a directory's size its entries may be in memory
/// (an ext2 entry of 12 bytes becomes up to three `vfs::DirEntry` slots of
/// 32 bytes while the list grows, and a name of at least 16): `read_dir`
/// refuses a directory the heap has no room for, since the kernel's heap
/// panics when it runs out.
const DIR_MEMORY_FACTOR: u64 = 10;

````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 304 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: read_dir's heap check covers the memory the smallest entries take"
````


### Task 9: An address space keeps its `mem_map` regions, first fit from the area's start

Spec §5.1 and §5.4: a process's address space holds the list of what `mem_map` gave it. `AddressSpace::map_area` maps fresh zeroed read-write pages at the lowest place in the area (`0x1000_0000_0000` to `0x7000_0000_0000`) where they fit, merging with the regions they meet; `unmap_area` gives back whole pages of one region, cutting it if need be, and frees their frames (the caller flushes the TLB, Task 15). A process has at most 1024 regions (`MAPS_MAX`), for an unmap that would cut one in two as well, so its list cannot fill the kernel's heap; asking for more frames than the `room` the caller gives (those above the 8 MiB reserve) is `ENOMEM` before anything is mapped, and running out midway leaves nothing mapped (decision 7). Host-tested over the fake `PhysMem`, frames counted. Mutation checks: no room check, a region counted apart when it meets the next, no rollback, an unmap past its region, an unaligned unmap, a split past 1024 and a first fit that needs more than the gap each fail a test.

**Files:**
- Modify: `kernel/src/mm/space.rs`

**Interfaces:**
- Consumes: plan 2's `AddressSpace`, `PageTables::unmap`.
- Produces: `mm::space::{MAP_START, MAP_END, MAPS_MAX}`, `AddressSpace::{map_area(&mut self, &mut impl PhysMem, pages: u64, room: u64) -> Result<u64, Errno>, unmap_area(&mut self, &mut impl PhysMem, addr: u64, pages: u64) -> Result<(), Errno>, maps(&self) -> &[(u64, u64)]}`.

- [ ] **Step 1: Add the failing tests to `kernel/src/mm/space.rs`**

In `kernel/src/mm/space.rs`, replace:

````rust
    }

    #[test]
    fn a_range_must_stay_in_the_lower_half() {
````

with:

````rust
    }

    fn space(m: &mut FakeMem) -> AddressSpace {
        let k = kernel(m);
        AddressSpace::new(m, &k).unwrap()
    }

    #[test]
    fn mem_map_fills_the_area_from_its_start_and_merges_what_meets() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let a = s.map_area(&mut m, 2, u64::MAX).unwrap();
        assert_eq!(a, MAP_START);
        let (frame, perm) = s.user_page(&mut m, a + PAGE).unwrap();
        assert_eq!(perm, Perm::ReadWrite);
        assert!(m.bytes(frame).iter().all(|&b| b == 0), "zeroed");
        let b = s.map_area(&mut m, 3, u64::MAX).unwrap();
        assert_eq!(b, a + 2 * PAGE);
        assert_eq!(s.maps(), [(MAP_START, 5)], "one region");
        // A hole is filled first fit, and closing it merges all three.
        s.unmap_area(&mut m, a + PAGE, 2).unwrap();
        assert_eq!(s.maps(), [(MAP_START, 1), (MAP_START + 3 * PAGE, 2)]);
        assert!(s.user_page(&mut m, a + PAGE).is_none(), "unmapped");
        assert_eq!(
            s.map_area(&mut m, 3, u64::MAX).unwrap(),
            MAP_START + 5 * PAGE,
            "too big for the hole"
        );
        assert_eq!(s.map_area(&mut m, 2, u64::MAX).unwrap(), a + PAGE);
        assert_eq!(s.maps(), [(MAP_START, 8)]);
        s.destroy(&mut m);
    }

    #[test]
    fn mem_unmap_takes_whole_pages_of_earlier_maps_only() {
        let mut m = FakeMem::new();
        let k = kernel(&mut m);
        let before = m.frames();
        let mut s = AddressSpace::new(&mut m, &k).unwrap();
        let a = s.map_area(&mut m, 4, u64::MAX).unwrap();
        for (addr, pages) in [
            (a + 1, 1),
            (a, 0),
            (a, 5),
            (a - PAGE, 1),
            (a + 4 * PAGE, 1),
            (U, 1),
            (u64::MAX - PAGE + 1, 2),
            (a, u64::MAX),
        ] {
            assert_eq!(
                s.unmap_area(&mut m, addr, pages),
                Err(Errno::EINVAL),
                "{addr:#x} {pages}"
            );
        }
        assert_eq!(s.maps(), [(a, 4)], "nothing changed");
        // The first and last pages, then the middle.
        s.unmap_area(&mut m, a, 1).unwrap();
        s.unmap_area(&mut m, a + 3 * PAGE, 1).unwrap();
        assert_eq!(s.maps(), [(a + PAGE, 2)]);
        assert_eq!(
            s.unmap_area(&mut m, a, 2),
            Err(Errno::EINVAL),
            "the first is gone"
        );
        s.unmap_area(&mut m, a + PAGE, 2).unwrap();
        assert!(s.maps().is_empty());
        s.destroy(&mut m);
        assert_eq!(m.frames(), before, "every frame came back");
    }

    #[test]
    fn mem_map_refuses_what_it_cannot_give() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        assert_eq!(s.map_area(&mut m, 0, u64::MAX), Err(Errno::EINVAL));
        assert_eq!(
            s.map_area(&mut m, 11, 10),
            Err(Errno::ENOMEM),
            "more than the room"
        );
        let area = (MAP_END - MAP_START) / PAGE;
        assert_eq!(s.map_area(&mut m, area + 1, u64::MAX), Err(Errno::ENOMEM));
        assert_eq!(s.map_area(&mut m, u64::MAX, u64::MAX), Err(Errno::ENOMEM));
        assert!(s.maps().is_empty());
        // Running out of frames midway leaves nothing mapped.
        let frames = m.frames();
        m.limit = frames + 6;
        assert_eq!(s.map_area(&mut m, 8, u64::MAX), Err(Errno::ENOMEM));
        assert!(s.maps().is_empty());
        assert!(s.user_page(&mut m, MAP_START).is_none());
        m.limit = usize::MAX;
        assert_eq!(
            s.map_area(&mut m, 8, u64::MAX),
            Ok(MAP_START),
            "the same place again"
        );
        s.destroy(&mut m);
    }

    #[test]
    fn at_most_1024_regions() {
        let mut m = FakeMem::new();
        let mut s = space(&mut m);
        let n = MAPS_MAX as u64;
        let page = |k: u64| MAP_START + k * PAGE;
        // Pages 0..=2n, then 0 and 1 and every odd page from 3 given back:
        // n regions of one page, 2, 4, ..., 2n, and a hole of two pages at
        // the start of the area.
        s.map_area(&mut m, 2 * n + 1, u64::MAX).unwrap();
        s.unmap_area(&mut m, page(0), 2).unwrap();
        for k in 1..n {
            s.unmap_area(&mut m, page(2 * k + 1), 1).unwrap();
        }
        assert_eq!(s.maps().len(), MAPS_MAX);
        // A region apart from the others is refused; one that meets
        // another is not.
        assert_eq!(s.map_area(&mut m, 1, u64::MAX), Err(Errno::ENOMEM));
        assert_eq!(s.map_area(&mut m, 2, u64::MAX), Ok(page(0)));
        assert_eq!(s.maps().len(), MAPS_MAX);
        assert_eq!(s.maps()[0], (page(0), 3));
        // So is cutting a region in two; taking its end is not.
        assert_eq!(s.unmap_area(&mut m, page(1), 1), Err(Errno::ENOMEM));
        assert!(s.user_page(&mut m, page(1)).is_some(), "still mapped");
        s.unmap_area(&mut m, page(0), 1).unwrap();
        assert_eq!(s.maps().len(), MAPS_MAX);
        s.destroy(&mut m);
    }

    #[test]
    fn a_range_must_stay_in_the_lower_half() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib mm::space`

Expected: FAIL: compile errors such as `` cannot find value `MAP_START` in this scope ``; `` cannot find value `MAP_END` in this scope ``.

- [ ] **Step 3: Change `kernel/src/mm/space.rs`**

In `kernel/src/mm/space.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! is enough for the space to see every kernel mapping, later ones too.

use super::paging::{LOWER_HALF_END, MapError, PAGE, PageTables, Perm, PhysMem};

pub struct AddressSpace {
    tables: PageTables,
}
````

with:

````rust
//! is enough for the space to see every kernel mapping, later ones too.
//!
//! The space also keeps the list of what `mem_map` gave the program
//! (§5.1, §5.4): regions of the `mem_map` area, first fit from its start,
//! merged where they meet, so `mem_unmap` can give back whole pages of
//! them and nothing else.

use super::paging::{LOWER_HALF_END, MapError, PAGE, PageTables, Perm, PhysMem};
use alloc::vec::Vec;
use vfs::Errno;

/// The `mem_map` area (spec §5.1).
pub const MAP_START: u64 = 0x0000_1000_0000_0000;
pub const MAP_END: u64 = 0x0000_7000_0000_0000;
/// The most regions of `mem_map` memory a program may have, so its list
/// cannot fill the kernel's heap (regions that meet count as one, as
/// Linux's `max_map_count` counts them).
pub const MAPS_MAX: usize = 1024;

pub struct AddressSpace {
    tables: PageTables,
    /// The `mem_map` regions, as (start, pages), sorted and apart.
    maps: Vec<(u64, u64)>,
}
````

Replace:

````rust
        }
        Ok(AddressSpace { tables })
    }
````

with:

````rust
        }
        Ok(AddressSpace {
            tables,
            maps: Vec::new(),
        })
    }
````

Replace:

````rust
            done += n;
        }
        Ok(())
    }

    /// Gives back every page, table and the PML4. The kernel must not be
````

with:

````rust
            done += n;
        }
        Ok(())
    }

    /// Maps `pages` fresh zeroed read-write pages at the lowest place in the
    /// `mem_map` area they fit (spec §7.3); their address. `ENOMEM` if
    /// they do not fit in the area, need more than the `room` frames the
    /// program may still take, would make more than `MAPS_MAX` regions, or
    /// the frames run out; then nothing stays mapped.
    pub fn map_area(
        &mut self,
        mem: &mut impl PhysMem,
        pages: u64,
        room: u64,
    ) -> Result<u64, Errno> {
        if pages == 0 {
            return Err(Errno::EINVAL);
        }
        if pages > room {
            return Err(Errno::ENOMEM);
        }
        let len = pages.checked_mul(PAGE).ok_or(Errno::ENOMEM)?;
        // First fit: the gaps between the regions, in address order.
        let mut at = MAP_START;
        let mut i = 0;
        loop {
            let next = self.maps.get(i).map_or(MAP_END, |&(s, _)| s);
            if next - at >= len {
                break;
            }
            match self.maps.get(i) {
                Some(&(s, n)) => at = s + n * PAGE,
                None => return Err(Errno::ENOMEM),
            }
            i += 1;
        }
        let meets_before = i > 0 && self.end_of(i - 1) == at;
        let meets_after = self.maps.get(i).is_some_and(|&(s, _)| s == at + len);
        if !meets_before && !meets_after && self.maps.len() >= MAPS_MAX {
            return Err(Errno::ENOMEM);
        }
        for k in 0..pages {
            if self
                .map_zeroed(mem, at + k * PAGE, 1, Perm::ReadWrite)
                .is_err()
            {
                self.unmap_pages(mem, at, k);
                return Err(Errno::ENOMEM);
            }
        }
        match (meets_before, meets_after) {
            (true, true) => {
                let (_, n) = self.maps.remove(i);
                self.maps[i - 1].1 += pages + n;
            }
            (true, false) => self.maps[i - 1].1 += pages,
            (false, true) => self.maps[i] = (at, pages + self.maps[i].1),
            (false, false) => self.maps.insert(i, (at, pages)),
        }
        Ok(at)
    }

    /// The first address after region `i`.
    fn end_of(&self, i: usize) -> u64 {
        let (s, n) = self.maps[i];
        s + n * PAGE
    }

    /// Unmaps `pages` pages from `addr` (spec §7.3) and gives their frames
    /// back: whole pages of earlier `map_area`s only, `EINVAL` otherwise;
    /// `ENOMEM` if cutting a region in two would make more than `MAPS_MAX`.
    /// The caller flushes the TLB for them.
    pub fn unmap_area(
        &mut self,
        mem: &mut impl PhysMem,
        addr: u64,
        pages: u64,
    ) -> Result<(), Errno> {
        let end = pages
            .checked_mul(PAGE)
            .and_then(|len| addr.checked_add(len))
            .ok_or(Errno::EINVAL)?;
        if pages == 0 || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let i = self
            .maps
            .iter()
            .position(|&(s, n)| s <= addr && addr < s + n * PAGE)
            .ok_or(Errno::EINVAL)?;
        let (s, _) = self.maps[i];
        let region_end = self.end_of(i);
        if end > region_end {
            return Err(Errno::EINVAL);
        }
        let (before, after) = (addr > s, end < region_end);
        if before && after && self.maps.len() >= MAPS_MAX {
            return Err(Errno::ENOMEM);
        }
        self.unmap_pages(mem, addr, pages);
        match (before, after) {
            (true, true) => {
                self.maps[i].1 = (addr - s) / PAGE;
                self.maps.insert(i + 1, (end, (region_end - end) / PAGE));
            }
            (true, false) => self.maps[i].1 = (addr - s) / PAGE,
            (false, true) => self.maps[i] = (end, (region_end - end) / PAGE),
            (false, false) => {
                self.maps.remove(i);
            }
        }
        Ok(())
    }

    /// Unmaps `pages` pages from `addr` and frees their frames.
    fn unmap_pages(&mut self, mem: &mut impl PhysMem, addr: u64, pages: u64) {
        for k in 0..pages {
            if let Some(frame) = self.tables.unmap(mem, addr + k * PAGE) {
                mem.free_frame(frame);
            }
        }
    }

    /// The `mem_map` regions, as (start, pages).
    pub fn maps(&self) -> &[(u64, u64)] {
        &self.maps
    }

    /// Gives back every page, table and the PML4. The kernel must not be
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib mm::space`

Expected: PASS: 8 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: an address space keeps its mem_map regions, first fit from the area's start"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 30 scenario(s) passed`.

````bash
git push -u origin m2p3b/parts
gh pr create --base main --head m2p3b/parts --title "Milestone 2, plan 3b: The parts" --body-file - <<'EOF'
## What

Milestone 2, plan 3b, tasks 1–9: `relay-abi` gains what the file, console, `sys_info` and `power` calls pass (`Stat`, `StatFs`, `read_dir`'s records, `Uname`, the flags), `vfs::Errno` gains `EMFILE` and `ERANGE`; `crates/heap` grows by regions; an Escape alone over COM1 goes in after 50 ms, and any Ctrl-C ends a half-arrived serial sequence (milestone 1's findings); the line discipline; the tee stack; the mount table records what a removal or a move did, for the directories and files outside it; files of the VFS open in the fd table, with an offset their fds share; an address space's `mem_map` regions, first fit, at most 1024.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3b/parts --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-parts
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Programs use files (Tasks 10–14)

The file calls from ring 3, over the open files of PR 2: `open` to `close`, then the calls on paths, `read_dir`, `chdir` and `getcwd`; a removal that reaches every process's working directory and open files; and a program seeing its redirection's write error.

Branch `m2p3b/files`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-files`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3b/files /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-files origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-files
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3b/parts` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3b/files /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-files m2p3b/parts`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3b/parts>` and re-run `cargo xtask ci` before pushing.

### Task 10: The dispatcher's fake program in a module of its own

The file calls go into a module of the dispatcher's own (Task 11), whose tests need the fake program the dispatcher's tests use. It moves from `syscall.rs`'s test module into `syscall/testing.rs` (test support), unchanged but public. Nothing changes that a test sees: this is a move, and the dispatcher's tests pass before and after.

**Files:**
- Modify: `kernel/src/syscall.rs`
- Create: `kernel/src/syscall/testing.rs`

**Interfaces:**
- Consumes: plan 3a's dispatcher tests.
- Produces: `syscall::testing::{U, W, FULL, NOW, MEM, Fake, fake(), call, text, put, get}`.

- [ ] **Step 1: Add the failing tests and the module declaration to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use vfs::Errno;

````

with:

````rust
use vfs::Errno;

#[cfg(test)]
mod testing;

````

Replace:

````rust
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

````

with:

````rust
mod tests {
    use super::testing::*;
    use super::*;
    use crate::mm::paging::PAGE;
    use relay_abi::errno;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE};

````

- [ ] **Step 2: Create `kernel/src/syscall/testing.rs`**

Create `kernel/src/syscall/testing.rs`:

````rust
//! The dispatcher's fake of a program (tests only): its memory, its
//! files and what it asked of the kernel.

use super::*;
use crate::mm::paging::{PAGE, PageTables, Perm};
use crate::mm::space::AddressSpace;
use crate::mm::testing::FakeMem;
use relay_abi::decode;

pub const U: u64 = 0x40_0000;
/// The writable page.
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (a full disk).
pub const FULL: u64 = 7;

/// What the fake clock says.
pub const NOW: Time = Time {
    unix_seconds: 1_790_000_000,
    uptime_ns: 12_345_678_901,
};

pub const MEM: MemInfo = MemInfo {
    ram_total: 16 << 30,
    ram_free: 15 << 30,
    heap_total: 32 << 20,
    heap_used: 1 << 20,
};

/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; fds 0-2 and a full
/// disk as `FULL`; what it wrote, started, killed, and how long it
/// slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
    pub written: Vec<(u64, Vec<u8>)>,
    pub slept: Vec<u64>,
    pub spawned: Vec<Spawn>,
    /// Children that have ended, and whether any still runs.
    pub ended: Vec<(u32, WaitStatus)>,
    pub running: bool,
    pub killed: Vec<i64>,
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

pub fn fake() -> Fake {
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

pub fn call(f: &mut Fake, c: Call, args: [u64; 3]) -> Result<u64, u16> {
    match dispatch(f, c.number(), [args[0], args[1], args[2], 0, 0, 0]) {
        Outcome::Return(r) => decode(r),
        Outcome::Exit(code) => panic!("exited with {code}"),
    }
}

/// Everything written, joined, per fd.
pub fn text(f: &Fake, fd: u64) -> Vec<u8> {
    f.written
        .iter()
        .filter(|(d, _)| *d == fd)
        .flat_map(|(_, b)| b.clone())
        .collect()
}

/// Puts `bytes` into the writable page at `at`, as the program would.
pub fn put(f: &mut Fake, at: u64, bytes: &[u8]) {
    UserSlice::new(at, bytes.len() as u64)
        .unwrap()
        .write(&f.space, &mut f.mem, 0, bytes)
        .unwrap();
}

pub fn get(f: &mut Fake, at: u64, len: usize) -> Vec<u8> {
    let mut buf = alloc::vec![0; len];
    UserSlice::new(at, len as u64)
        .unwrap()
        .read(&f.space, &mut f.mem, 0, &mut buf)
        .unwrap();
    buf
}
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: PASS: 16 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add kernel
git commit -m "kernel: the dispatcher's fake program in a module of its own"
````


### Task 11: Programs open, read, write, seek, `fstat` and close files; `t-files`

Spec §7.3's calls on fds from ring 3 (decisions 2 and 5). The dispatcher's `Caller` gives it the program's fds (`with_fds`), the files (`with_vfs`, from the program's current directory) and the console, instead of writing to an fd itself: `write` goes through the file's kind (the screen, the in-kernel shell's outputs, or the open file), `EBADF` for an fd opened for reading before the buffer is looked at, and a full disk takes what fits and then says `ENOSPC`. The new calls are in `syscall/files.rs`: `open` (the path at most 4096 bytes; `EMFILE` before anything is opened or created), `close`, `read` (a page at a time, each checked writable before it is read into, so the offset never moves past what the program got), `seek` (`EINVAL` on the console and the shell's outputs) and `fstat` (the console and the outputs are character devices). Reading the console comes with Task 17. `relay_abi::errno::name` names each error for the test programs; `relay-rt` gains `sys::{open, close, read, seek, fstat}`, and `t-files basic` runs them on the ext2 root: `open`'s flags and errors, a write over part of the file, append, an offset shared with a child that got the fd, 32 fds and then `EMFILE` (the `files` scenario; the red run's `t-files` gets `ENOSYS`); `system` lists five programs. Mutation checks: `write`'s `EBADF` before the buffer, `open`'s `EMFILE` before creating and `read`'s writable check each fail a test, and `read` not stopping at a short read hangs its test; `write` stopping after a short write survived and was dropped as redundant (the next piece's `ENOSPC` ends the call with the same count).

**Files:**
- Modify: `crates/relay-abi/src/errno.rs`
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Create: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Create: `tests/e2e/files.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-files.rs`

**Interfaces:**
- Consumes: Tasks 1, 7, 10.
- Produces: `Caller::{with_fds<R>(&mut self, impl FnOnce(&mut FdTable) -> R) -> R, with_vfs<R>(&mut self, impl FnOnce(&mut dyn Vfs) -> R) -> R, console_write(&mut self, &[u8]), shell_output(&mut self, &Arc<File>, n: u32, &[u8]) -> Result<(), Errno>}` (replacing `writable_fd` and `output`); `syscall::files::{path, open, close, read, seek, fstat}`; `relay_abi::errno::name(u16) -> Option<&str>`; `relay_rt::sys::{open, close, read, seek, fstat}`; `userland/tests/src/bin/t-files.rs`; the `files` scenario.

- [ ] **Step 1: Change `crates/relay-abi/src/errno.rs`**

In `crates/relay-abi/src/errno.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const ENOTEMPTY: u16 = 39;

````

with:

````rust
pub const ENOTEMPTY: u16 = 39;

/// The name of error number `n` (`ENOENT`), for test programs and logs.
pub const fn name(n: u16) -> Option<&'static str> {
    Some(match n {
        EPERM => "EPERM",
        ENOENT => "ENOENT",
        ESRCH => "ESRCH",
        EINTR => "EINTR",
        EIO => "EIO",
        E2BIG => "E2BIG",
        ENOEXEC => "ENOEXEC",
        EBADF => "EBADF",
        ECHILD => "ECHILD",
        EAGAIN => "EAGAIN",
        ENOMEM => "ENOMEM",
        EFAULT => "EFAULT",
        EBUSY => "EBUSY",
        EEXIST => "EEXIST",
        EXDEV => "EXDEV",
        ENOTDIR => "ENOTDIR",
        EISDIR => "EISDIR",
        EINVAL => "EINVAL",
        EMFILE => "EMFILE",
        EFBIG => "EFBIG",
        ENOSPC => "ENOSPC",
        EROFS => "EROFS",
        EPIPE => "EPIPE",
        ERANGE => "ERANGE",
        ENAMETOOLONG => "ENAMETOOLONG",
        ENOSYS => "ENOSYS",
        ENOTEMPTY => "ENOTEMPTY",
        _ => return None,
    })
}

````

Replace:

````rust
            assert_eq!(linux.get(name), Some(&n), "{name}");
        }
    }
````

with:

````rust
            assert_eq!(linux.get(name), Some(&n), "{name}");
            assert_eq!(super::name(n), Some(name));
        }
        assert_eq!(super::name(0), None);
        assert_eq!(super::name(6), None, "ENXIO is not ours");
    }
````

- [ ] **Step 2: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, WaitStatus, decode};

````

with:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Stat, WaitStatus, decode};

````

Replace:

````rust
    unimplemented!("system calls exist only on Relay OS")
}
````

with:

````rust
    unimplemented!("system calls exist only on Relay OS")
}

/// Makes call `c` with `args` (the rest zero): its value or error.
fn call(c: Call, args: &[u64]) -> Result<u64, u16> {
    let mut a = [0; 6];
    a[..args.len()].copy_from_slice(args);
    decode(unsafe { syscall(c, a) })
}

/// Opens `path` with `relay_abi::file`'s `OPEN_*` flags: the new fd.
pub fn open(path: &[u8], flags: u32) -> Result<u32, u16> {
    call(
        Call::Open,
        &[path.as_ptr() as u64, path.len() as u64, u64::from(flags)],
    )
    .map(|fd| fd as u32)
}

/// Closes `fd`.
pub fn close(fd: u32) -> Result<(), u16> {
    call(Call::Close, &[u64::from(fd)]).map(|_| ())
}

/// Reads some of `fd` into `buf`; returns how many bytes, 0 at the end.
pub fn read(fd: u32, buf: &mut [u8]) -> Result<usize, u16> {
    call(
        Call::Read,
        &[u64::from(fd), buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// Moves `fd`'s offset (`relay_abi::file`'s `SEEK_*`); the new offset.
pub fn seek(fd: u32, offset: i64, whence: u32) -> Result<u64, u16> {
    call(
        Call::Seek,
        &[u64::from(fd), offset as u64, u64::from(whence)],
    )
}

/// What `fd` is.
pub fn fstat(fd: u32) -> Result<Stat, u16> {
    let mut st = Stat::default();
    call(Call::Fstat, &[u64::from(fd), &raw mut st as u64])?;
    Ok(st)
}
````

- [ ] **Step 3: Add the failing tests and the module declaration to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use vfs::Errno;

#[cfg(test)]
mod testing;
````

with:

````rust
use vfs::Errno;

mod files;
#[cfg(test)]
mod testing;
````

Replace:

````rust
            Call::Getpid,
            Call::Write,
            Call::Time,
````

with:

````rust
            Call::Getpid,
            Call::Open,
            Call::Close,
            Call::Read,
            Call::Write,
            Call::Seek,
            Call::Fstat,
            Call::Time,
````

- [ ] **Step 4: Write the failing tests for `kernel/src/syscall/files.rs`**

Create `kernel/src/syscall/files.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use crate::mm::paging::PAGE;
    use relay_abi::Call;
    use relay_abi::errno;
    use relay_abi::file::{
        KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_WRITE, SEEK_END,
        SEEK_START,
    };

    /// Opens `path` (put at `W + 512`) with `flags`.
    fn open(f: &mut Fake, path: &[u8], flags: u32) -> Result<u64, u16> {
        put(f, W + 512, path);
        call(
            f,
            Call::Open,
            [W + 512, path.len() as u64, u64::from(flags)],
        )
    }

    #[test]
    fn open_gives_the_lowest_free_fd_and_close_frees_it() {
        let mut f = fake();
        assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(3));
        assert_eq!(
            open(&mut f, b"f", OPEN_READ),
            Ok(4),
            "from /root, the current directory"
        );
        assert_eq!(call(&mut f, Call::Close, [3, 0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Close, [3, 0, 0]), Err(errno::EBADF));
        assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(3));
        for fd in [5, 31, 32, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::Close, [fd, 0, 0]),
                Err(errno::EBADF),
                "{fd}"
            );
        }
        // Up to 32 (7 is `FULL`); the 33rd creates nothing.
        for fd in (5..32).filter(|&fd| fd != FULL) {
            assert_eq!(open(&mut f, b"/root/f", OPEN_READ), Ok(fd));
        }
        assert_eq!(
            open(&mut f, b"/root/new", OPEN_WRITE | OPEN_CREATE),
            Err(errno::EMFILE)
        );
        assert_eq!(open(&mut f, b"/root/new", OPEN_READ), Err(errno::EMFILE));
        assert_eq!(call(&mut f, Call::Close, [4, 0, 0]), Ok(0));
        assert_eq!(
            open(&mut f, b"/root/new", OPEN_READ),
            Err(errno::ENOENT),
            "was not created"
        );
    }

    #[test]
    fn open_refuses_a_bad_path_or_flags() {
        let mut f = fake();
        assert_eq!(
            call(&mut f, Call::Open, [0, 4, u64::from(OPEN_READ)]),
            Err(errno::EFAULT)
        );
        assert_eq!(
            call(&mut f, Call::Open, [W, 4097, u64::from(OPEN_READ)]),
            Err(errno::ENAMETOOLONG)
        );
        assert_eq!(open(&mut f, b"/root/f", 0), Err(errno::EINVAL));
        put(&mut f, W + 512, b"/root/f");
        assert_eq!(
            call(
                &mut f,
                Call::Open,
                [W + 512, 7, 1 << 32 | u64::from(OPEN_READ)]
            ),
            Err(errno::EINVAL),
            "flags above 32 bits"
        );
        assert_eq!(
            open(
                &mut f,
                b"/root/f",
                OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE
            ),
            Err(errno::EEXIST)
        );
        assert_eq!(open(&mut f, b"", OPEN_READ), Err(errno::ENOENT));
    }

    #[test]
    fn read_and_write_move_the_offset_of_the_open_file() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/f", OPEN_READ | OPEN_WRITE).unwrap();
        assert_eq!(call(&mut f, Call::Read, [fd, W, 2]), Ok(2));
        assert_eq!(get(&mut f, W, 2), b"he");
        // `U` holds the pattern 0, 1, 2, ...
        assert_eq!(call(&mut f, Call::Write, [fd, U + 10, 3]), Ok(3));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, u64::from(SEEK_START)]),
            Ok(0)
        );
        assert_eq!(
            call(&mut f, Call::Read, [fd, W, 100]),
            Ok(5),
            "fewer at the end"
        );
        assert_eq!(get(&mut f, W, 5), [b'h', b'e', 10, 11, 12]);
        assert_eq!(call(&mut f, Call::Read, [fd, W, 100]), Ok(0), "at the end");
        assert_eq!(call(&mut f, Call::Read, [fd, W, 0]), Ok(0));
        let ap = open(&mut f, b"/root/f", OPEN_WRITE | OPEN_APPEND).unwrap();
        assert_eq!(call(&mut f, Call::Write, [ap, U, 2]), Ok(2));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, -2i64 as u64, u64::from(SEEK_END)]),
            Ok(5)
        );
        assert_eq!(call(&mut f, Call::Read, [fd, W, 10]), Ok(2));
        assert_eq!(get(&mut f, W, 2), [0, 1]);
    }

    #[test]
    fn a_read_across_pages_copies_them_all_and_stops_at_a_bad_one() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/big", OPEN_READ | OPEN_WRITE | OPEN_CREATE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [fd, U, 3 * PAGE]), Ok(3 * PAGE));
        call(&mut f, Call::Seek, [fd, 0, 0]).unwrap();
        // Into the one writable page, 100 bytes before its end: 100 bytes,
        // and the offset stays after them.
        assert_eq!(
            call(&mut f, Call::Read, [fd, W + PAGE - 100, 1000]),
            Ok(100)
        );
        assert_eq!(
            get(&mut f, W + PAGE - 100, 100),
            (0..100).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        assert_eq!(call(&mut f, Call::Seek, [fd, 0, 1]), Ok(100));
        assert_eq!(
            call(&mut f, Call::Read, [fd, U, 10]),
            Err(errno::EFAULT),
            "read-only memory"
        );
        assert_eq!(call(&mut f, Call::Read, [fd, 0, 10]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, 1]),
            Ok(100),
            "nothing was read"
        );
        assert_eq!(call(&mut f, Call::Read, [fd, W, PAGE]), Ok(PAGE));
        assert_eq!(get(&mut f, W, 4), [100, 101, 102, 103]);
    }

    #[test]
    fn what_an_fd_was_not_opened_for_is_ebadf() {
        let mut f = fake();
        let r = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        let w = open(&mut f, b"/root/f", OPEN_WRITE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [r, U, 1]), Err(errno::EBADF));
        assert_eq!(
            call(&mut f, Call::Write, [r, 0, 1]),
            Err(errno::EBADF),
            "before the buffer"
        );
        assert_eq!(call(&mut f, Call::Read, [w, W, 1]), Err(errno::EBADF));
        assert_eq!(
            call(&mut f, Call::Read, [1, W, 1]),
            Err(errno::EBADF),
            "an output"
        );
        assert_eq!(
            call(&mut f, Call::Read, [9, W, 1]),
            Err(errno::EBADF),
            "not open"
        );
        let d = open(&mut f, b"/root", OPEN_READ).unwrap();
        assert_eq!(call(&mut f, Call::Read, [d, W, 1]), Err(errno::EISDIR));
        assert_eq!(call(&mut f, Call::Write, [d, U, 1]), Err(errno::EBADF));
    }

    #[test]
    fn a_full_disk_is_a_short_write_then_enospc() {
        let mut f = fake();
        let fd = open(&mut f, b"/full/x", OPEN_WRITE | OPEN_CREATE).unwrap();
        assert_eq!(call(&mut f, Call::Write, [fd, U, 3 * PAGE]), Ok(FULL_BYTES));
        assert_eq!(call(&mut f, Call::Write, [fd, U, 10]), Err(errno::ENOSPC));
    }

    #[test]
    fn seek_has_no_meaning_on_the_console() {
        let mut f = fake();
        for fd in [0, 1, 2] {
            assert_eq!(
                call(&mut f, Call::Seek, [fd, 0, 0]),
                Err(errno::EINVAL),
                "{fd}"
            );
        }
        assert_eq!(call(&mut f, Call::Seek, [9, 0, 0]), Err(errno::EBADF));
        let fd = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(
            call(&mut f, Call::Seek, [fd, 0, 1 << 32]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::Seek, [fd, -1i64 as u64, 0]),
            Err(errno::EINVAL)
        );
    }

    #[test]
    fn fstat_fills_in_the_file_s_status() {
        let mut f = fake();
        let fd = open(&mut f, b"/root/f", OPEN_READ).unwrap();
        assert_eq!(call(&mut f, Call::Fstat, [fd, W, 0]), Ok(0));
        let b = get(&mut f, W, 72);
        let u64_at = |i: usize| u64::from_ne_bytes(b[i..i + 8].try_into().unwrap());
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(
            (u64_at(8), u32_at(48), u32_at(52)),
            (5, u32::from(KIND_REGULAR), 0o644)
        );
        assert_eq!(call(&mut f, Call::Fstat, [0, W, 0]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_CHAR_DEVICE]);
        assert_eq!(call(&mut f, Call::Fstat, [fd, U, 0]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Fstat, [fd, W + PAGE - 71, 0]),
            Err(errno::EFAULT)
        );
        assert_eq!(call(&mut f, Call::Fstat, [9, W, 0]), Err(errno::EBADF));
    }

    #[test]
    fn reading_the_console_waits_for_its_line_discipline() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Read, [0, W, 1]), Err(errno::ENOSYS));
    }
}
````

- [ ] **Step 5: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use super::*;
use crate::mm::paging::{PAGE, PageTables, Perm};
use crate::mm::space::AddressSpace;
use crate::mm::testing::FakeMem;
use relay_abi::decode;

pub const U: u64 = 0x40_0000;
/// The writable page.
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (a full disk).
pub const FULL: u64 = 7;

````

with:

````rust
use super::*;
use crate::fd::{FdTable, File};
use crate::mm::paging::{PAGE, PageTables, Perm};
use crate::mm::space::AddressSpace;
use crate::mm::testing::FakeMem;
use alloc::boxed::Box;
use relay_abi::decode;
use vfs::{Env, FileSystem, MemFs, MountTable};

pub const U: u64 = 0x40_0000;
/// The writable page.
pub const W: u64 = U + 3 * PAGE;
/// An fd whose file fails every write (the shell's output redirected to a
/// full disk).
pub const FULL: u64 = 7;
/// How much file data `/full` holds.
pub const FULL_BYTES: u64 = 8192;

````

Replace:

````rust
/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; fds 0-2 and a full
/// disk as `FULL`; what it wrote, started, killed, and how long it
/// slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
/// A program with three readable pages at `U` holding a pattern, a
/// writable page after them and nothing after that; the console as fd 0,
/// the in-kernel shell's outputs as fds 1 and 2 and one to a full disk as
/// `FULL`; the files `/root/f` ("hello", `/root` its current directory)
/// and `/full` (8 KiB of room); what it wrote to the console (as fd 0) or
/// the outputs (as 1 and 2), started, killed, and how long it slept.
pub struct Fake {
    pub mem: FakeMem,
    pub space: AddressSpace,
    pub fds: FdTable,
    pub vfs: MountTable,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
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
````

with:

````rust
    }
    fn with_fds<R>(&mut self, f: impl FnOnce(&mut FdTable) -> R) -> R {
        f(&mut self.fds)
    }
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R {
        f(&mut self.vfs)
    }
    fn console_write(&mut self, bytes: &[u8]) {
        self.written.push((0, bytes.to_vec()));
    }
    fn shell_output(&mut self, _: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
        if u64::from(n) == FULL {
            return Err(Errno::ENOSPC);
        }
        self.written.push((u64::from(n), bytes.to_vec()));
        Ok(())
````

Replace:

````rust
    space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
    Fake {
        mem,
        space,
        written: Vec::new(),
````

with:

````rust
    space.map_zeroed(&mut mem, W, 1, Perm::ReadWrite).unwrap();
    let mut fds = FdTable::shell();
    fds.set(FULL as usize, Arc::new(File::ShellOutput(FULL as u32)));
    Fake {
        mem,
        space,
        fds,
        vfs: files(),
        written: Vec::new(),
````

Replace:

````rust
    buf
}
````

with:

````rust
    buf
}

struct Clock;

impl Env for Clock {
    fn now(&self) -> u64 {
        1_000
    }
    fn log(&self, _: &str) {}
}

/// `/root/f` holding "hello", with `/root` the current directory, and a
/// filesystem at `/full` with room for `FULL_BYTES`.
fn files() -> MountTable {
    let mut fs = MemFs::new(Box::new(Clock));
    let root = fs.root();
    let home = fs.mkdir(root, b"root").unwrap();
    let f = fs.create(home, b"f").unwrap();
    fs.write_at(f, 0, b"hello").unwrap();
    let mut t = MountTable::new(Box::new(fs));
    let full = MemFs::new(Box::new(Clock)).with_capacity(FULL_BYTES);
    t.mount(b"/full", Box::new(full)).unwrap();
    t.chdir(b"/root").unwrap();
    t
}
````

- [ ] **Step 6: Add the scenario `tests/e2e/files.txt`**

Create `tests/e2e/files.txt`:

````text
# Programs open, read, write and close files (user-space gate §7.3,
# milestone 2, plan 3b): open's flags and their errors, an offset shared
# with a child that got the fd, and 32 fds at most. On the ext2 root.
timeout 60
expect root@relay:~# $
send t-files basic
expect \ncreate: 3\nexclusive: EEXIST\nno flags: EINVAL\nmissing: ENOENT\n
expect write: 13\nread a write-only fd: EBADF\nseek: 7\noverwrite: 6\nclose: ok\nclose again: EBADF\n
expect read: "hello"\nfstat: 13 bytes, regular true\nseek to the end: 13\nread at the end: 0\n
expect append: 5\nread what was appended: "more\\n"\nseek before the start: EINVAL\nseek the screen: EINVAL\n
expect the child read: "hello, "\nread after the child: "files"\nfds up to 31, then EMFILE\nroot@relay:~# $
send cat t-files.tmp
expect \nhello, files\nmore\nroot@relay:~# $
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-spawn  t-spin\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-spawn  t-spin\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-spawn  t-spin\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-spawn  t-spin\n
````

- [ ] **Step 8: Add the test program `userland/tests/src/bin/t-files.rs`**

Create `userland/tests/src/bin/t-files.rs`:

````rust
//! `t-files KIND`: the file calls from ring 3 (spec §7.3), each answer
//! printed as `<what>: <value or error name>`, so a scenario and the NUC's
//! check script see what the kernel said. It works in the current
//! directory, on `t-files.tmp`.
//!
//! - `t-files basic`: `open` with its flags, `read`, `write`, `seek`,
//!   `fstat` and `close`; an offset shared with a child that got the fd;
//!   32 fds and then `EMFILE`.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
//!   of `basic`).
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::errno;
use relay_abi::file::{
    KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
    SEEK_END, SEEK_START,
};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

const TMP: &[u8] = b"t-files.tmp";

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"child") => child(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-files: {}", name(e));
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

/// `<what>: ok` or `<what>: <error>`.
fn show_ok(what: &str, r: Result<(), u16>) {
    show(what, r.map(|()| "ok"));
}

/// `<what>: "<text>"` for bytes read, a newline as `\n`.
fn show_text(what: &str, bytes: &[u8]) {
    let _ = write!(Fd(1), "{what}: \"");
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        let _ = match line.strip_suffix(b"\n") {
            Some(l) => sys::write_all(1, l).and_then(|()| sys::write_all(1, b"\\n")),
            None => sys::write_all(1, line),
        };
    }
    let _ = sys::write_all(1, b"\"\n");
}

fn basic() -> Result<(), u16> {
    let rw = OPEN_READ | OPEN_WRITE;
    let fd = sys::open(TMP, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    show("create", Ok(fd));
    show(
        "exclusive",
        sys::open(TMP, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE),
    );
    show("no flags", sys::open(TMP, 0));
    show("missing", sys::open(b"t-files.missing", OPEN_READ));
    show("write", sys::write(fd, b"hello, world\n"));
    show("read a write-only fd", sys::read(fd, &mut [0; 4]));
    show("seek", sys::seek(fd, 7, SEEK_START));
    show("overwrite", sys::write(fd, b"files\n"));
    show_ok("close", sys::close(fd));
    show_ok("close again", sys::close(fd));

    let r = sys::open(TMP, OPEN_READ)?;
    let mut buf = [0u8; 64];
    let n = sys::read(r, &mut buf[..5])?;
    show_text("read", &buf[..n]);
    let st = sys::fstat(r)?;
    let _ = writeln!(
        Fd(1),
        "fstat: {} bytes, regular {}",
        st.size,
        st.kind == u32::from(KIND_REGULAR)
    );
    show("seek to the end", sys::seek(r, 0, SEEK_END));
    show("read at the end", sys::read(r, &mut buf));
    let a = sys::open(TMP, OPEN_WRITE | OPEN_APPEND)?;
    show("append", sys::write(a, b"more\n"));
    let n = sys::read(r, &mut buf)?;
    show_text("read what was appended", &buf[..n]);
    show("seek before the start", sys::seek(r, -1, SEEK_START));
    show("seek the screen", sys::seek(1, 0, SEEK_START));
    sys::close(a)?;

    // A child that got the fd shares its offset.
    sys::seek(r, 0, SEEK_START)?;
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
        FdMap {
            child: 3,
            parent: r,
        },
    ];
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0child\0", b"", &fds, 0)?;
    sys::wait(i64::from(pid), false)?;
    let n = sys::read(r, &mut buf[..5])?;
    show_text("read after the child", &buf[..n]);
    sys::close(r)?;

    // 32 fds at most.
    let mut last = 0;
    let full = loop {
        match sys::open(TMP, rw) {
            Ok(fd) => last = fd,
            Err(e) => break e,
        }
    };
    let _ = writeln!(Fd(1), "fds up to {last}, then {}", name(full));
    for fd in 3..=last {
        sys::close(fd)?;
    }
    Ok(())
}

fn child() -> Result<(), u16> {
    let mut buf = [0u8; 7];
    let n = sys::read(3, &mut buf)?;
    show_text("the child read", &buf[..n]);
    Ok(())
}
````

- [ ] **Step 9: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find trait `Vfs` in this scope ``; `` cannot find type `Arc` in this scope ``.

Run: `cargo xtask test --e2e-only --scenario files`

Expected: FAIL: scenario `files` stops at line 7, timed out waiting for `\ncreate: 3\nexclusive: EEXIST\nno flags: EINVAL\nmissing: ENOENT\n`.

- [ ] **Step 10: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 4 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 5 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 11: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    }

    /// The running process's file open as `fd`.
    fn file(&self, fd: u64) -> Result<Arc<File>, Errno> {
        let t = PROCS.lock();
        let p = t.get(t.current()).ok_or(Errno::EBADF)?;
        p.res.fds.get(fd).cloned()
    }
}
````

with:

````rust
    }
}
````

Replace:

````rust

    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno> {
        self.file(fd).map(|_| ())
    }

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
                } else {
                    // SAFETY: set by the in-kernel shell's `wait`, which is
                    // blocked until its child has ended and clears it
                    // before it returns; nothing else calls it meanwhile.
                    return unsafe { (*out.cast::<Out<'_>>())(n, bytes) };
                }
            }
            File::Vfs(ref open) => return open.write_all(&mut KernelVfs, bytes),
        }
        Ok(())
    }
````

with:

````rust

    fn with_fds<R>(&mut self, f: impl FnOnce(&mut FdTable) -> R) -> R {
        let mut t = PROCS.lock();
        let me = t.current();
        let p = t.get_mut(me).expect("a process makes system calls");
        f(&mut p.res.fds)
    }

    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R {
        f(&mut KernelVfs)
    }

    fn console_write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }

    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
        let out = SHELL_OUT.load(Ordering::Acquire);
        if out.is_null() || !shell_output_now(file, n) {
            // The shell waits for nobody, or for another command: this
            // goes to the screen.
            console::write_output(bytes);
            return Ok(());
        }
        // SAFETY: set by the in-kernel shell's `wait`, which is blocked
        // until its child has ended and clears it before it returns;
        // nothing else calls it meanwhile.
        unsafe { (*out.cast::<Out<'_>>())(n, bytes) }
    }
````

- [ ] **Step 12: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
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

````

with:

````rust
//! `UserStr`) and leaves the rest to the `Caller`, the kernel's side of
//! the process. The file calls are in `files`; `proc_list` and `pipe` are
//! `ENOSYS` until milestone 3.

use crate::exec::ARGS_MAX;
use crate::fd::{FdTable, File};
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::sync::Arc;
use alloc::vec::Vec;
use relay_abi::info::INFO_MEMORY;
use relay_abi::spawn::{NEW_GROUP, SPAWN_FDS, WAIT_ANY, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Time, WaitStatus, encode};
use vfs::{Errno, Vfs};

````

Replace:

````rust
    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno>;
    /// `EBADF` unless `fd` is open for writing.
    fn writable_fd(&mut self, fd: u64) -> Result<(), Errno>;
    /// Writes `bytes` to `fd` (checked with `writable_fd`); the file's
    /// error, if any.
    fn output(&mut self, fd: u64, bytes: &[u8]) -> Result<(), Errno>;
    /// Starts a child; its pid.
````

with:

````rust
    fn read_str(&mut self, s: &UserStr) -> Result<Vec<u8>, Errno>;
    /// Runs `f` with the program's fds. `f` must not block.
    fn with_fds<R>(&mut self, f: impl FnOnce(&mut FdTable) -> R) -> R;
    /// Runs `f` with the files, from the program's current directory.
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R;
    /// Writes `bytes` to the screen.
    fn console_write(&mut self, bytes: &[u8]);
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
    /// 2), which sends them where the command line says; its error, if any.
    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno>;
    /// Starts a child; its pid.
````

Replace:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Time) => time(caller, args[0]),
````

with:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::Open) => files::open(caller, args[0], args[1], args[2]),
        Some(Call::Close) => files::close(caller, args[0]),
        Some(Call::Read) => files::read(caller, args[0], args[1], args[2]),
        Some(Call::Write) => write(caller, args[0], args[1], args[2]),
        Some(Call::Seek) => files::seek(caller, args[0], args[1] as i64, args[2]),
        Some(Call::Fstat) => files::fstat(caller, args[0], args[1]),
        Some(Call::Time) => time(caller, args[0]),
````

Replace:

````rust

/// `write(fd, buffer, length)`. Copies the buffer out a page at a time; a
/// page that is not the program's, or the file's error, ends the call with
/// the bytes written before it, or with the error if there were none.
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    caller.writable_fd(fd)?;
    let slice = UserSlice::new(addr, len)?;
````

with:

````rust

/// The file open as `fd`.
fn file(caller: &mut impl Caller, fd: u64) -> Result<Arc<File>, Errno> {
    caller.with_fds(|t| t.get(fd).cloned())
}

/// `write(fd, buffer, length)`. Copies the buffer in a page at a time; a
/// page that is not the program's, or the file's error, ends the call with
/// the bytes written before it, or with the error if there were none (a
/// full disk takes what fits, then says `ENOSPC`).
fn write(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    if let File::Vfs(open) = &*file
        && !open.is_writable()
    {
        return Err(Errno::EBADF);
    }
    let slice = UserSlice::new(addr, len)?;
````

Replace:

````rust
            .read(&slice, done, &mut buf[..n])
            .and_then(|()| caller.output(fd, &buf[..n]));
        if let Err(e) = written {
            return if done > 0 { Ok(done) } else { Err(e) };
        }
        done += n as u64;
    }
    Ok(done)
}
````

with:

````rust
            .read(&slice, done, &mut buf[..n])
            .and_then(|()| write_to(caller, &file, &buf[..n]));
        match written {
            Ok(k) => done += k as u64,
            Err(e) if done == 0 => return Err(e),
            Err(_) => break,
        }
    }
    Ok(done)
}

/// Writes `bytes` to `file`: how many it took.
fn write_to(caller: &mut impl Caller, file: &Arc<File>, bytes: &[u8]) -> Result<usize, Errno> {
    match &**file {
        File::Console => {
            caller.console_write(bytes);
            Ok(bytes.len())
        }
        File::ShellOutput(n) => caller.shell_output(file, *n, bytes).map(|()| bytes.len()),
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
}
````

- [ ] **Step 13: Implement `kernel/src/syscall/files.rs`**

Insert this at the top of `kernel/src/syscall/files.rs`, above `#[cfg(test)]`:

````rust
//! The file calls (user-space gate §7.3): what a program passes is checked
//! and copied here, the open files do the rest (`crate::file`).

use super::{Caller, PATH_MAX, file};
use crate::fd::{FDS, File};
use crate::file as open_file;
use crate::mm::paging::PAGE;
use crate::mm::user::{UserSlice, UserStr};
use alloc::sync::Arc;
use alloc::vec::Vec;
use relay_abi::file::{KIND_CHAR_DEVICE, Stat};
use vfs::Errno;

/// A path of the program's, copied in: at most 4096 bytes.
pub(super) fn path(caller: &mut impl Caller, addr: u64, len: u64) -> Result<Vec<u8>, Errno> {
    caller.read_str(&UserStr::new(addr, len, PATH_MAX, Errno::ENAMETOOLONG)?)
}

/// `open(path, length, flags)`: the lowest free fd. `EMFILE` when all 32
/// are in use, before anything is opened or created.
pub(super) fn open(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    flags: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let flags = u32::try_from(flags).map_err(|_| Errno::EINVAL)?;
    if caller.with_fds(|t| t.open()) >= FDS {
        return Err(Errno::EMFILE);
    }
    let open = caller.with_vfs(|v| open_file::open(v, &path, flags))?;
    let file = Arc::new(File::Vfs(open));
    caller.with_fds(|t| t.insert(file)).map(u64::from)
}

/// `close(fd)`. The open file lives on while another fd has it.
pub(super) fn close(caller: &mut impl Caller, fd: u64) -> Result<u64, Errno> {
    caller.with_fds(|t| t.remove(fd)).map(|_| 0)
}

/// `read(fd, buffer, length)`: from the file's offset, a page at a time,
/// each page checked writable before anything is read into it, so the
/// offset never moves past what the program got. A bad page ends the
/// call with the bytes read before it, or with `EFAULT` if there were
/// none; so does the end of the file.
pub(super) fn read(caller: &mut impl Caller, fd: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let open = match &*file {
        File::Vfs(open) if open.is_readable() => open,
        File::Vfs(_) | File::ShellOutput(_) => return Err(Errno::EBADF),
        // The console's reads come with its line discipline.
        File::Console => return Err(Errno::ENOSYS),
    };
    let slice = UserSlice::new(addr, len)?;
    let mut buf = [0u8; PAGE as usize];
    let mut done = 0;
    while done < len {
        let n = (len - done).min(PAGE - (addr + done) % PAGE);
        let page = UserSlice::new(addr + done, n)?;
        let got = caller
            .writable(&page)
            .and_then(|()| caller.with_vfs(|v| open.read(v, &mut buf[..n as usize])));
        match got {
            Ok(k) => {
                caller.write(&slice, done, &buf[..k])?;
                done += k as u64;
                if k < n as usize {
                    break;
                }
            }
            Err(e) if done == 0 => return Err(e),
            Err(_) => break,
        }
    }
    Ok(done)
}

/// `seek(fd, offset, whence)`: the new offset. The console and the shell's
/// outputs have none (`EINVAL`).
pub(super) fn seek(
    caller: &mut impl Caller,
    fd: u64,
    offset: i64,
    whence: u64,
) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let File::Vfs(open) = &*file else {
        return Err(Errno::EINVAL);
    };
    let whence = u32::try_from(whence).map_err(|_| Errno::EINVAL)?;
    caller.with_vfs(|v| open.seek(v, offset, whence))
}

/// `fstat(fd, &mut Stat)`. The console and the shell's outputs are
/// character devices, with nothing else to say.
pub(super) fn fstat(caller: &mut impl Caller, fd: u64, addr: u64) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let slice = UserSlice::new(addr, Stat::SIZE as u64)?;
    let stat = match &*file {
        File::Vfs(open) => caller.with_vfs(|v| open.stat(v))?,
        File::Console | File::ShellOutput(_) => Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
            ..Stat::default()
        },
    };
    caller.write(&slice, 0, &stat.to_bytes())?;
    Ok(0)
}

````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 317 tests.

- [ ] **Step 15: Run the `files`, `system`, `diskfull`, `spawn` scenarios**

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario spawn`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add crates docs kernel tests userland
git commit -m "kernel, relay-rt: programs open, read, write, seek, fstat and close files; t-files"
````


### Task 12: The calls on paths, `read_dir`, `chdir` and `getcwd` from ring 3

The rest of spec §7.3's file calls (decision 5): `stat` (links are never followed, so `NOFOLLOW` changes nothing), `read_dir` (the buffer checked writable before anything is read, so no entry is lost to a bad one, and used up to 64 KiB, so a program cannot size a kernel allocation; the kernel's heap room goes to Task 7's check through `Caller::heap_room`), `mkdir`, `rmdir`, `unlink`, `truncate`, `touch` (of a file that exists), `readlink` (cut at the buffer), `rename`, `statfs`, `sync`, `chdir` and `getcwd` (`ERANGE` for a buffer too short). `relay-rt` gains a wrapper for each, and `t-files dir` and `cwd` run them on the ext2 root: a listing a record at a time, in name order, the calls' errors, a working directory of the program's own and its children's (the `files` scenario). Mutation checks: `stat`'s flag check, `read_dir`'s writable check, `getcwd`'s `ERANGE` and `readlink`'s cut each fail a test; `read_dir`'s 64 KiB limit on its kernel buffer is not observable on the host (the tests allocate from the host's heap) and was checked by reading.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `tests/e2e/files.txt`
- Modify: `userland/tests/src/bin/t-files.rs`

**Interfaces:**
- Consumes: Tasks 7, 11.
- Produces: `Caller::heap_room(&self) -> usize`; `syscall::files::{stat, read_dir, on_path, touch, truncate, readlink, rename, statfs, getcwd}`; `relay_rt::sys::{stat, read_dir, mkdir, rmdir, unlink, touch, truncate, readlink, rename, statfs, sync, chdir, getcwd}`; `t-files dir`, `cwd`, `pwd`.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Stat, WaitStatus, decode};

````

with:

````rust
use relay_abi::spawn::{SPAWN_FDS, WAIT_NOHANG};
use relay_abi::{Call, FdMap, MemInfo, SpawnArgs, Stat, StatFs, WaitStatus, decode};

````

Replace:

````rust
    Ok(st)
}
````

with:

````rust
    Ok(st)
}

/// What `path` is (`relay_abi::file::STAT_NOFOLLOW` in `flags`: a link's
/// own status; links are never followed anyway).
pub fn stat(path: &[u8], flags: u32) -> Result<Stat, u16> {
    let mut st = Stat::default();
    let args = [
        path.as_ptr() as u64,
        path.len() as u64,
        u64::from(flags),
        &raw mut st as u64,
    ];
    call(Call::Stat, &args)?;
    Ok(st)
}

/// The directory `fd`'s next entries into `buf`, as `relay_abi::file`
/// records (`dir_entries` reads them); 0 after the last.
pub fn read_dir(fd: u32, buf: &mut [u8]) -> Result<usize, u16> {
    call(
        Call::ReadDir,
        &[u64::from(fd), buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// A call on one path.
fn on_path(c: Call, path: &[u8], rest: &[u64]) -> Result<u64, u16> {
    let mut a = [path.as_ptr() as u64, path.len() as u64, 0, 0];
    a[2..2 + rest.len()].copy_from_slice(rest);
    call(c, &a)
}

pub fn mkdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Mkdir, path, &[]).map(|_| ())
}

pub fn rmdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Rmdir, path, &[]).map(|_| ())
}

pub fn unlink(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Unlink, path, &[]).map(|_| ())
}

/// Sets the file's times to now; it must exist.
pub fn touch(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Touch, path, &[]).map(|_| ())
}

pub fn truncate(path: &[u8], size: u64) -> Result<(), u16> {
    on_path(Call::Truncate, path, &[size]).map(|_| ())
}

/// A symbolic link's target into `buf`, cut at its length; how many bytes.
pub fn readlink(path: &[u8], buf: &mut [u8]) -> Result<usize, u16> {
    on_path(
        Call::Readlink,
        path,
        &[buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

pub fn rename(from: &[u8], to: &[u8]) -> Result<(), u16> {
    let args = [
        from.as_ptr() as u64,
        from.len() as u64,
        to.as_ptr() as u64,
        to.len() as u64,
    ];
    call(Call::Rename, &args).map(|_| ())
}

/// The figures of the filesystem holding `path`.
pub fn statfs(path: &[u8]) -> Result<StatFs, u16> {
    let mut f = StatFs::default();
    on_path(Call::Statfs, path, &[&raw mut f as u64])?;
    Ok(f)
}

/// Makes every change to every filesystem durable.
pub fn sync() -> Result<(), u16> {
    call(Call::Sync, &[]).map(|_| ())
}

pub fn chdir(path: &[u8]) -> Result<(), u16> {
    on_path(Call::Chdir, path, &[]).map(|_| ())
}

/// The current directory's path into `buf`; how many bytes (`ERANGE` if
/// it does not fit).
pub fn getcwd(buf: &mut [u8]) -> Result<usize, u16> {
    call(Call::Getcwd, &[buf.as_mut_ptr() as u64, buf.len() as u64]).map(|n| n as usize)
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
            Call::Fstat,
            Call::Time,
````

with:

````rust
            Call::Fstat,
            Call::Stat,
            Call::ReadDir,
            Call::Mkdir,
            Call::Rmdir,
            Call::Unlink,
            Call::Truncate,
            Call::Touch,
            Call::Readlink,
            Call::Rename,
            Call::Statfs,
            Call::Sync,
            Call::Chdir,
            Call::Getcwd,
            Call::Time,
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    }

    #[test]
    fn reading_the_console_waits_for_its_line_discipline() {
````

with:

````rust
    }

    /// Calls `c` on `path` (put at `W + 512`) and the other arguments.
    fn on(f: &mut Fake, c: Call, path: &[u8], rest: [u64; 2]) -> Result<u64, u16> {
        put(f, W + 512, path);
        let args = [W + 512, path.len() as u64, rest[0], rest[1], 0, 0];
        match super::super::dispatch(f, c.number(), args) {
            super::super::Outcome::Return(r) => relay_abi::decode(r),
            super::super::Outcome::Exit(_) => unreachable!(),
        }
    }

    fn u64_at(b: &[u8], i: usize) -> u64 {
        u64::from_ne_bytes(b[i..i + 8].try_into().unwrap())
    }

    #[test]
    fn stat_names_a_file_by_its_path_and_never_follows_a_link() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Stat, b"f", [0, W]), Ok(0));
        assert_eq!(u64_at(&get(&mut f, W, 72), 8), 5, "its size");
        assert_eq!(on(&mut f, Call::Stat, b"link", [0, W]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_SYMLINK]);
        let nofollow = u64::from(relay_abi::file::STAT_NOFOLLOW);
        assert_eq!(on(&mut f, Call::Stat, b"link", [nofollow, W]), Ok(0));
        assert_eq!(get(&mut f, W + 48, 1), [relay_abi::file::KIND_SYMLINK]);
        assert_eq!(on(&mut f, Call::Stat, b"f", [2, W]), Err(errno::EINVAL));
        assert_eq!(on(&mut f, Call::Stat, b"nope", [0, W]), Err(errno::ENOENT));
        assert_eq!(on(&mut f, Call::Stat, b"f", [0, U]), Err(errno::EFAULT));
    }

    #[test]
    fn read_dir_copies_out_records_and_goes_on_where_it_stopped() {
        let mut f = fake();
        let d = open(&mut f, b"/root", OPEN_READ).unwrap();
        // Room for two records of short names.
        let two = 2 * relay_abi::file::DirEntry::record_len(4) as u64;
        let mut names = Vec::new();
        for _ in 0..4 {
            let n = call(&mut f, Call::ReadDir, [d, W, two]).unwrap();
            let bytes = get(&mut f, W, n as usize);
            names.extend(relay_abi::file::dir_entries(&bytes).map(|r| r.name.to_vec()));
        }
        assert_eq!(
            names,
            [
                b".".to_vec(),
                b"..".to_vec(),
                b"f".to_vec(),
                b"link".to_vec()
            ]
        );
        call(&mut f, Call::Seek, [d, 0, 0]).unwrap();
        // A bad buffer loses no entry.
        assert_eq!(call(&mut f, Call::ReadDir, [d, U, two]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::ReadDir, [d, W + PAGE - 8, two]),
            Err(errno::EFAULT)
        );
        let n = call(&mut f, Call::ReadDir, [d, W, two]).unwrap();
        assert_eq!(
            relay_abi::file::dir_entries(&get(&mut f, W, n as usize))
                .next()
                .unwrap()
                .name,
            b"."
        );
        assert_eq!(
            call(&mut f, Call::ReadDir, [d, W, 8]),
            Err(errno::EINVAL),
            "not one fits"
        );
        let file = open(&mut f, b"f", OPEN_READ).unwrap();
        assert_eq!(
            call(&mut f, Call::ReadDir, [file, W, two]),
            Err(errno::ENOTDIR)
        );
        assert_eq!(
            call(&mut f, Call::ReadDir, [0, W, two]),
            Err(errno::ENOTDIR),
            "the console"
        );
        f.heap_room = 0;
        assert_eq!(call(&mut f, Call::ReadDir, [d, W, two]), Err(errno::ENOMEM));
        // A huge buffer is fine: records are made in 64 KiB at most.
        f.heap_room = usize::MAX;
        call(&mut f, Call::Seek, [d, 0, 0]).unwrap();
        assert!(call(&mut f, Call::ReadDir, [d, W, PAGE]).unwrap() > 0);
    }

    #[test]
    fn the_calls_on_a_path_do_what_their_vfs_operation_does() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Mkdir, b"d", [0, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Mkdir, b"d", [0, 0]), Err(errno::EEXIST));
        assert_eq!(open(&mut f, b"d/x", OPEN_WRITE | OPEN_CREATE), Ok(3));
        assert_eq!(on(&mut f, Call::Rmdir, b"d", [0, 0]), Err(errno::ENOTEMPTY));
        assert_eq!(on(&mut f, Call::Truncate, b"d/x", [10, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Stat, b"d/x", [0, W]), Ok(0));
        assert_eq!(u64_at(&get(&mut f, W, 72), 8), 10);
        assert_eq!(on(&mut f, Call::Touch, b"d/x", [0, 0]), Ok(0));
        assert_eq!(
            on(&mut f, Call::Touch, b"d/nope", [0, 0]),
            Err(errno::ENOENT)
        );
        put(&mut f, W + 900, b"d/y");
        assert_eq!(on(&mut f, Call::Rename, b"d/x", [W + 900, 3]), Ok(0));
        assert_eq!(on(&mut f, Call::Unlink, b"d/x", [0, 0]), Err(errno::ENOENT));
        assert_eq!(on(&mut f, Call::Unlink, b"d/y", [0, 0]), Ok(0));
        assert_eq!(on(&mut f, Call::Unlink, b"d", [0, 0]), Err(errno::EISDIR));
        assert_eq!(on(&mut f, Call::Rmdir, b"d", [0, 0]), Ok(0));
        put(&mut f, W + 900, b"/full/f");
        assert_eq!(
            on(&mut f, Call::Rename, b"f", [W + 900, 7]),
            Err(errno::EXDEV)
        );
        assert_eq!(on(&mut f, Call::Rename, b"f", [0, 7]), Err(errno::EFAULT));
        assert_eq!(call(&mut f, Call::Sync, [0, 0, 0]), Ok(0));
        for c in [
            Call::Mkdir,
            Call::Rmdir,
            Call::Unlink,
            Call::Touch,
            Call::Chdir,
        ] {
            assert_eq!(
                call(&mut f, c, [W, 4097, 0]),
                Err(errno::ENAMETOOLONG),
                "{c:?}"
            );
            assert_eq!(call(&mut f, c, [0, 1, 0]), Err(errno::EFAULT), "{c:?}");
        }
    }

    #[test]
    fn readlink_gives_the_target_cut_at_the_buffer() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Readlink, b"link", [W, 100]), Ok(1));
        assert_eq!(get(&mut f, W, 1), b"f");
        assert_eq!(on(&mut f, Call::Readlink, b"link", [W, 0]), Ok(0));
        assert_eq!(
            on(&mut f, Call::Readlink, b"f", [W, 100]),
            Err(errno::EINVAL),
            "not a link"
        );
        assert_eq!(
            on(&mut f, Call::Readlink, b"link", [U, 100]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn statfs_fills_in_the_filesystem_s_figures() {
        let mut f = fake();
        assert_eq!(on(&mut f, Call::Statfs, b"/full", [W, 0]), Ok(0));
        let b = get(&mut f, W, 48);
        let want = vfs::Vfs::statfs(&mut f.vfs, b"/full").unwrap();
        assert_eq!(
            [
                u64_at(&b, 0),
                u64_at(&b, 8),
                u64_at(&b, 16),
                u64_at(&b, 24),
                u64_at(&b, 32),
                u64_at(&b, 40)
            ],
            [
                want.block_size,
                want.blocks,
                want.free_blocks,
                want.avail_blocks,
                want.files,
                want.free_files
            ]
        );
        assert_eq!(
            on(&mut f, Call::Statfs, b"/nope", [W, 0]),
            Err(errno::ENOENT)
        );
        assert_eq!(
            on(&mut f, Call::Statfs, b"/", [W + PAGE - 40, 0]),
            Err(errno::EFAULT)
        );
    }

    #[test]
    fn chdir_and_getcwd() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Getcwd, [W, 100, 0]), Ok(5));
        assert_eq!(get(&mut f, W, 5), b"/root");
        assert_eq!(call(&mut f, Call::Getcwd, [W, 4, 0]), Err(errno::ERANGE));
        assert_eq!(call(&mut f, Call::Getcwd, [U, 100, 0]), Err(errno::EFAULT));
        assert_eq!(on(&mut f, Call::Chdir, b"/full", [0, 0]), Ok(0));
        assert_eq!(call(&mut f, Call::Getcwd, [W, 100, 0]), Ok(5));
        assert_eq!(get(&mut f, W, 5), b"/full");
        assert_eq!(
            on(&mut f, Call::Chdir, b"/root/f", [0, 0]),
            Err(errno::ENOTDIR)
        );
        assert_eq!(
            on(&mut f, Call::Chdir, b"/nope", [0, 0]),
            Err(errno::ENOENT)
        );
        assert_eq!(
            open(&mut f, b"../root/f", OPEN_READ),
            Ok(3),
            "relative to /full"
        );
    }

    #[test]
    fn reading_the_console_waits_for_its_line_discipline() {
````

- [ ] **Step 4: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    pub vfs: MountTable,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
    pub vfs: MountTable,
    /// What `heap_room` says.
    pub heap_room: usize,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
        f(&mut self.vfs)
    }
````

with:

````rust
        f(&mut self.vfs)
    }
    fn heap_room(&self) -> usize {
        self.heap_room
    }
````

Replace:

````rust
        vfs: files(),
        written: Vec::new(),
````

with:

````rust
        vfs: files(),
        heap_room: usize::MAX,
        written: Vec::new(),
````

Replace:

````rust

/// `/root/f` holding "hello", with `/root` the current directory, and a
/// filesystem at `/full` with room for `FULL_BYTES`.
fn files() -> MountTable {
````

with:

````rust

/// `/root/f` holding "hello" and `/root/link` to it, with `/root` the
/// current directory, and a filesystem at `/full` with room for
/// `FULL_BYTES`.
fn files() -> MountTable {
````

Replace:

````rust
    fs.write_at(f, 0, b"hello").unwrap();
    let mut t = MountTable::new(Box::new(fs));
````

with:

````rust
    fs.write_at(f, 0, b"hello").unwrap();
    fs.symlink(home, b"link", b"f").unwrap();
    let mut t = MountTable::new(Box::new(fs));
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/files.txt`**

Replace the whole of `tests/e2e/files.txt` with:

````text
# Programs open, read, write and close files (user-space gate §7.3,
# milestone 2, plan 3b): open's flags and their errors, an offset shared
# with a child that got the fd, and 32 fds at most. On the ext2 root.
timeout 60
expect root@relay:~# $
send t-files basic
expect \ncreate: 3\nexclusive: EEXIST\nno flags: EINVAL\nmissing: ENOENT\n
expect write: 13\nread a write-only fd: EBADF\nseek: 7\noverwrite: 6\nclose: ok\nclose again: EBADF\n
expect read: "hello"\nfstat: 13 bytes, regular true\nseek to the end: 13\nread at the end: 0\n
expect append: 5\nread what was appended: "more\\n"\nseek before the start: EINVAL\nseek the screen: EINVAL\n
expect the child read: "hello, "\nread after the child: "files"\nfds up to 31, then EMFILE\nroot@relay:~# $
send cat t-files.tmp
expect \nhello, files\nmore\nroot@relay:~# $
# Directories and names: mkdir, read_dir a record at a time (in
# name order, `/` after a directory), stat, truncate, touch, rename,
# readlink, statfs, sync, unlink and rmdir.
send t-files dir
expect \nmkdir: ok\nmkdir again: EEXIST\nentries: \./ \.\./ a b sub/ \(5 calls\)\n
expect read_dir at the end: 0\nread_dir of a file: ENOTDIR\nstat size: 3\ntruncate: ok\nstat size: 10\n
expect touch: ok\ntouch a missing file: ENOENT\nrename: ok\nstat the old name: ENOENT\nreadlink of a file: EINVAL\n
expect rmdir a full directory: ENOTEMPTY\nunlink a directory: EISDIR\nstatfs: some blocks, free ones among them true\n
expect sync: ok\nrmdir: ok\nstat it: ENOENT\nroot@relay:~# $
# A working directory of its own: chdir and getcwd, and a child's.
send t-files cwd
expect \ngetcwd: /root\ngetcwd into 3 bytes: ERANGE\nchdir: ok\ngetcwd: /root/t-files.c\nstat it from above: 4\n
expect chdir to a file: ENOTDIR\nchdir to nothing: ENOENT\nmy working directory: /root/t-files.c\nmy working directory: /root\ngetcwd: /root\n
send pwd
expect \n/root\nroot@relay:~# $
send ls t-files.c
expect \nls: cannot access 't-files.c': No such file or directory\n
````

- [ ] **Step 6: Change the test program `userland/tests/src/bin/t-files.rs`**

In `userland/tests/src/bin/t-files.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!   32 fds and then `EMFILE`.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
//!   of `basic`).
#![no_std]
````

with:

````rust
//!   32 fds and then `EMFILE`.
//! - `t-files dir`: `mkdir`, `read_dir` a record or two at a time, `stat`,
//!   `truncate`, `touch`, `rename`, `readlink`, `statfs`, `sync`, `unlink`
//!   and `rmdir`, in `t-files.d`.
//! - `t-files cwd`: `chdir` and `getcwd`, and the working directory a child
//!   starts in.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
//!   of `basic`); `t-files pwd` prints its working directory (the children
//!   of `cwd`).
#![no_std]
````

Replace:

````rust
use relay_abi::file::{
    KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
    SEEK_END, SEEK_START,
};
````

with:

````rust
use relay_abi::file::{
    KIND_DIRECTORY, KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_DIRECTORY, OPEN_EXCLUSIVE,
    OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE, SEEK_END, SEEK_START, dir_entries,
};
````

Replace:

````rust
        Some(b"child") => child(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic\n");
            return 2;
````

with:

````rust
        Some(b"child") => child(),
        Some(b"dir") => dir(),
        Some(b"cwd") => cwd(),
        Some(b"pwd") => pwd("my working directory"),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd\n");
            return 2;
````

Replace:

````rust
    show_text("the child read", &buf[..n]);
    Ok(())
}
````

with:

````rust
    show_text("the child read", &buf[..n]);
    Ok(())
}

/// Creates `path` holding `bytes`.
fn put(path: &[u8], bytes: &[u8]) -> Result<(), u16> {
    let fd = sys::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    sys::write_all(fd, bytes)?;
    sys::close(fd)
}

fn dir() -> Result<(), u16> {
    show_ok("mkdir", sys::mkdir(b"t-files.d"));
    show_ok("mkdir again", sys::mkdir(b"t-files.d"));
    put(b"t-files.d/b", b"abc")?;
    put(b"t-files.d/a", b"")?;
    sys::mkdir(b"t-files.d/sub")?;
    // One record at a time: a short name's takes 24 bytes, so 40 hold
    // only one.
    let d = sys::open(b"t-files.d", OPEN_READ | OPEN_DIRECTORY)?;
    let mut buf = [0u8; 40];
    let mut calls = 0;
    let _ = write!(Fd(1), "entries:");
    loop {
        let n = sys::read_dir(d, &mut buf)?;
        if n == 0 {
            break;
        }
        calls += 1;
        for r in dir_entries(&buf[..n]) {
            let kind = match r.kind {
                KIND_DIRECTORY => "/",
                KIND_REGULAR => "",
                _ => "?",
            };
            let _ = write!(Fd(1), " ");
            let _ = sys::write_all(1, r.name);
            let _ = write!(Fd(1), "{kind}");
        }
    }
    let _ = writeln!(Fd(1), " ({calls} calls)");
    show("read_dir at the end", sys::read_dir(d, &mut buf));
    show("read_dir of a file", sys::read_dir(1, &mut buf));
    sys::close(d)?;
    show("stat size", sys::stat(b"t-files.d/b", 0).map(|s| s.size));
    show_ok("truncate", sys::truncate(b"t-files.d/b", 10));
    show("stat size", sys::stat(b"t-files.d/b", 0).map(|s| s.size));
    show_ok("touch", sys::touch(b"t-files.d/b"));
    show_ok("touch a missing file", sys::touch(b"t-files.d/nope"));
    show_ok("rename", sys::rename(b"t-files.d/a", b"t-files.d/c"));
    show(
        "stat the old name",
        sys::stat(b"t-files.d/a", 0).map(|s| s.size),
    );
    show(
        "readlink of a file",
        sys::readlink(b"t-files.d/b", &mut buf),
    );
    show_ok("rmdir a full directory", sys::rmdir(b"t-files.d"));
    show_ok("unlink a directory", sys::unlink(b"t-files.d/sub"));
    let f = sys::statfs(b".")?;
    let _ = writeln!(
        Fd(1),
        "statfs: {} blocks, free ones among them {}",
        if f.blocks > 0 { "some" } else { "no" },
        f.free_blocks <= f.blocks
    );
    show_ok("sync", sys::sync());
    sys::unlink(b"t-files.d/b")?;
    sys::unlink(b"t-files.d/c")?;
    sys::rmdir(b"t-files.d/sub")?;
    show_ok("rmdir", sys::rmdir(b"t-files.d"));
    show("stat it", sys::stat(b"t-files.d", 0).map(|s| s.size));
    Ok(())
}

/// Prints the working directory as `<what>: <path>`.
fn pwd(what: &str) -> Result<(), u16> {
    let mut buf = [0u8; 256];
    let n = sys::getcwd(&mut buf)?;
    let _ = write!(Fd(1), "{what}: ");
    sys::write_all(1, &buf[..n])?;
    sys::write_all(1, b"\n")
}

/// Starts `t-files pwd` in `cwd` and waits for it.
fn child_in(cwd: &[u8]) -> Result<(), u16> {
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
    let pid = sys::spawn(b"/bin/t-files", b"t-files\0pwd\0", cwd, &fds, 0)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}

fn cwd() -> Result<(), u16> {
    pwd("getcwd")?;
    show("getcwd into 3 bytes", sys::getcwd(&mut [0; 3]));
    sys::mkdir(b"t-files.c")?;
    show_ok("chdir", sys::chdir(b"t-files.c"));
    pwd("getcwd")?;
    put(b"x", b"here")?;
    show(
        "stat it from above",
        sys::stat(b"../t-files.c/x", 0).map(|s| s.size),
    );
    show_ok("chdir to a file", sys::chdir(b"x"));
    show_ok("chdir to nothing", sys::chdir(b"nope"));
    child_in(b"")?;
    child_in(b"..")?;
    sys::chdir(b"..")?;
    pwd("getcwd")?;
    sys::unlink(b"t-files.c/x")?;
    sys::rmdir(b"t-files.c")
}
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` method `heap_room` is not a member of trait `Caller` ``.

Run: `cargo xtask test --e2e-only --scenario files`

Expected: FAIL: scenario `files` stops at line 18, timed out waiting for `\nmkdir: ok\nmkdir again: EEXIST\nentries: \./ \.\./ a b sub/ \(5 calls\)\n`.

- [ ] **Step 8: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

    fn console_write(&mut self, bytes: &[u8]) {
````

with:

````rust

    fn heap_room(&self) -> usize {
        mm::heap_room()
    }

    fn console_write(&mut self, bytes: &[u8]) {
````

- [ ] **Step 9: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R;
    /// Writes `bytes` to the screen.
````

with:

````rust
    fn with_vfs<R>(&mut self, f: impl FnOnce(&mut dyn Vfs) -> R) -> R;
    /// The most bytes the kernel's heap may give one allocation now.
    fn heap_room(&self) -> usize;
    /// Writes `bytes` to the screen.
````

Replace:

````rust
        Some(Call::Fstat) => files::fstat(caller, args[0], args[1]),
        Some(Call::Time) => time(caller, args[0]),
````

with:

````rust
        Some(Call::Fstat) => files::fstat(caller, args[0], args[1]),
        Some(Call::Stat) => files::stat(caller, args[0], args[1], args[2], args[3]),
        Some(Call::ReadDir) => files::read_dir(caller, args[0], args[1], args[2]),
        Some(Call::Mkdir) => files::on_path(caller, args[0], args[1], |v, p| v.mkdir(p)),
        Some(Call::Rmdir) => files::on_path(caller, args[0], args[1], |v, p| v.rmdir(p)),
        Some(Call::Unlink) => files::on_path(caller, args[0], args[1], |v, p| v.unlink(p)),
        Some(Call::Truncate) => files::truncate(caller, args[0], args[1], args[2]),
        Some(Call::Touch) => files::on_path(caller, args[0], args[1], files::touch),
        Some(Call::Readlink) => files::readlink(caller, args[0], args[1], args[2], args[3]),
        Some(Call::Rename) => files::rename(caller, [args[0], args[1], args[2], args[3]]),
        Some(Call::Statfs) => files::statfs(caller, args[0], args[1], args[2]),
        Some(Call::Sync) => caller.with_vfs(|v| v.sync()).map(|()| 0),
        Some(Call::Chdir) => files::on_path(caller, args[0], args[1], |v, p| v.chdir(p)),
        Some(Call::Getcwd) => files::getcwd(caller, args[0], args[1]),
        Some(Call::Time) => time(caller, args[0]),
````

- [ ] **Step 10: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use alloc::vec::Vec;
use relay_abi::file::{KIND_CHAR_DEVICE, Stat};
use vfs::Errno;

````

with:

````rust
use alloc::vec::Vec;
use relay_abi::StatFs;
use relay_abi::file::{KIND_CHAR_DEVICE, STAT_NOFOLLOW, Stat};
use vfs::{Errno, Vfs};

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

/// `read_dir`'s records are made in a buffer of at most this, and copied
/// out whole.
const READ_DIR_MAX: u64 = 64 * 1024;

/// `stat(path, length, flags, &mut Stat)`. Symbolic links are never
/// followed (milestone 1), so a link's status is its own with
/// `STAT_NOFOLLOW` or without.
pub(super) fn stat(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    flags: u64,
    out: u64,
) -> Result<u64, Errno> {
    if flags & !u64::from(STAT_NOFOLLOW) != 0 {
        return Err(Errno::EINVAL);
    }
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, Stat::SIZE as u64)?;
    let st = caller.with_vfs(|v| v.lookup(&path).and_then(|n| v.stat(n)))?;
    caller.write(&slice, 0, &open_file::stat_of(&st).to_bytes())?;
    Ok(0)
}

/// `read_dir(fd, buffer, length)`: the directory's next entries as
/// `relay_abi::file` records, the bytes written; 0 after the last. The
/// buffer is checked writable before anything is read, so no entry is
/// lost to a bad one.
pub(super) fn read_dir(
    caller: &mut impl Caller,
    fd: u64,
    addr: u64,
    len: u64,
) -> Result<u64, Errno> {
    let file = file(caller, fd)?;
    let File::Vfs(open) = &*file else {
        return Err(Errno::ENOTDIR);
    };
    let len = len.min(READ_DIR_MAX);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let room = caller.heap_room();
    let mut buf = alloc::vec![0u8; len as usize];
    let n = caller.with_vfs(|v| open.read_dir(v, &mut buf, room))?;
    caller.write(&slice, 0, &buf[..n])?;
    Ok(n as u64)
}

/// A call on one path: `mkdir`, `rmdir`, `unlink`, `touch`, `chdir`.
pub(super) fn on_path(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    op: impl FnOnce(&mut dyn Vfs, &[u8]) -> Result<(), Errno>,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    caller.with_vfs(|v| op(v, &path)).map(|()| 0)
}

/// `touch(path, length)`: the file's times set to now; `ENOENT` if it does
/// not exist (a program creates files with `open`).
pub(super) fn touch(v: &mut dyn Vfs, path: &[u8]) -> Result<(), Errno> {
    let node = v.lookup(path)?;
    v.touch(node)
}

/// `truncate(path, length, size)`.
pub(super) fn truncate(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    size: u64,
) -> Result<u64, Errno> {
    on_path(caller, addr, len, |v, path| {
        let node = v.lookup(path)?;
        v.truncate(node, size)
    })
}

/// `readlink(path, length, buffer, buffer length)`: the link's target, cut
/// at the buffer's length as on Linux; the bytes written.
pub(super) fn readlink(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    out: u64,
    out_len: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, out_len)?;
    let target = caller.with_vfs(|v| v.lookup(&path).and_then(|n| v.read_link(n)))?;
    let n = target.len().min(out_len as usize);
    caller.write(&slice, 0, &target[..n])?;
    Ok(n as u64)
}

/// `rename(from, length, to, length)`.
pub(super) fn rename(caller: &mut impl Caller, a: [u64; 4]) -> Result<u64, Errno> {
    let from = path(caller, a[0], a[1])?;
    let to = path(caller, a[2], a[3])?;
    caller.with_vfs(|v| v.rename(&from, &to)).map(|()| 0)
}

/// `statfs(path, length, &mut StatFs)`: the filesystem holding the path.
pub(super) fn statfs(
    caller: &mut impl Caller,
    addr: u64,
    len: u64,
    out: u64,
) -> Result<u64, Errno> {
    let path = path(caller, addr, len)?;
    let slice = UserSlice::new(out, StatFs::SIZE as u64)?;
    let f = caller.with_vfs(|v| v.statfs(&path))?;
    let s = StatFs {
        block_size: f.block_size,
        blocks: f.blocks,
        free_blocks: f.free_blocks,
        avail_blocks: f.avail_blocks,
        files: f.files,
        free_files: f.free_files,
    };
    caller.write(&slice, 0, &s.to_bytes())?;
    Ok(0)
}

/// `getcwd(buffer, length)`: the current directory's path, the bytes
/// written; `ERANGE` if the buffer is too short. A directory that was
/// removed keeps the path it had, as the in-kernel shell's `pwd` shows it.
pub(super) fn getcwd(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let slice = UserSlice::new(addr, len)?;
    let cwd = caller.with_vfs(|v| v.cwd());
    if cwd.len() as u64 > len {
        return Err(Errno::ERANGE);
    }
    caller.write(&slice, 0, &cwd)?;
    Ok(cwd.len() as u64)
}
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 323 tests.

- [ ] **Step 12: Run the `files` scenario**

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates kernel tests userland
git commit -m "kernel, relay-rt: the calls on paths, read_dir, chdir and getcwd from ring 3"
````


### Task 13: A removal reaches every process

Decision 6, with Task 6's records: after every operation on the mount table, `mounts::with` hands what it removed or moved to `proc::follow_changes`, with the table's lock released: the working directory of every process but the running one (which the table changed itself) follows, and every process's open files of a freed inode are gone (`FdTable::follow`). `t-files gone` shows why: a child keeps `t-files.g` as its working directory and `t-files.g/f` open while its parent removes both and makes a new directory and file, which ext2 gives their inodes; the child finds both gone and the new ones keep what they hold. In the red run the child creates `x` in the new directory and writes `child` into the new file (`newchild`). Mutation checks: no propagation, only the directories and only the files each fail `files`; `FdTable::follow` never matching survived until its test made a file on the freed inode before reading the old one.

**Files:**
- Modify: `kernel/src/fd.rs`
- Modify: `kernel/src/mounts.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/proc/table.rs`
- Modify: `tests/e2e/files.txt`
- Modify: `userland/tests/src/bin/t-files.rs`

**Interfaces:**
- Consumes: Tasks 6, 7, 12.
- Produces: `proc::follow_changes(&[Change])`, `FdTable::follow(&self, &Change)`, `Table::iter_mut`; `t-files gone`.

- [ ] **Step 1: Add the failing tests to `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, replace:

````rust
    #[test]
    fn a_bad_mapping_is_refused() {
````

with:

````rust
    #[test]
    fn the_open_files_of_a_freed_inode_are_gone() {
        use crate::file;
        use alloc::boxed::Box;
        use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_WRITE};
        use vfs::{Env, MemFs, MountTable, Vfs};
        struct Clock;
        impl Env for Clock {
            fn now(&self) -> u64 {
                0
            }
            fn log(&self, _: &str) {}
        }
        let mut t = MountTable::new(Box::new(MemFs::new(Box::new(Clock))));
        let rw = OPEN_READ | OPEN_WRITE | OPEN_CREATE;
        let a = Arc::new(File::Vfs(file::open(&mut t, b"/a", rw).unwrap()));
        let b = Arc::new(File::Vfs(file::open(&mut t, b"/b", rw).unwrap()));
        let mut fds = FdTable::shell();
        fds.insert(Arc::clone(&a)).unwrap();
        fds.insert(Arc::clone(&a)).unwrap();
        fds.insert(Arc::clone(&b)).unwrap();
        let freed = t.lookup(b"/a").unwrap();
        t.unlink(b"/a").unwrap();
        for c in t.take_changes() {
            fds.follow(&c);
        }
        // The next file gets the freed inode: `a` must not reach it.
        let c = t.create(b"/c").unwrap();
        assert_eq!(c, freed);
        t.write_at(c, 0, b"new").unwrap();
        let read = |f: &File, t: &mut MountTable| match f {
            File::Vfs(o) => o.read(t, &mut [0; 4]),
            _ => unreachable!(),
        };
        assert_eq!(read(&a, &mut t), Err(Errno::ENOENT));
        assert_eq!(read(&b, &mut t), Ok(0), "another file");
        // A move frees nothing.
        t.mkdir(b"/d").unwrap();
        t.rename(b"/d", b"/e").unwrap();
        for c in t.take_changes() {
            fds.follow(&c);
        }
        assert_eq!(read(&b, &mut t), Ok(0));
    }

    #[test]
    fn a_bad_mapping_is_refused() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/files.txt`**

In `tests/e2e/files.txt`, replace:

````text
expect \nls: cannot access 't-files.c': No such file or directory\n
````

with:

````text
expect \nls: cannot access 't-files.c': No such file or directory\n
# Another process removes a program's working directory and an open file
# (spec §16 item 4): both are gone for it, and the new directory and file,
# which may get their inodes, keep what they hold.
send t-files gone
expect \nread the removed file: ENOENT\nthe child's working directory: /root/t-files.g\nthe child creates x: ENOENT\n
expect the child reads its file: ENOENT\nthe child writes its file: ENOENT\nthe new file holds: "new"\nthe new directory holds x: ENOENT\nroot@relay:~# $
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-files.rs`**

In `userland/tests/src/bin/t-files.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   starts in.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
````

with:

````rust
//!   starts in.
//! - `t-files gone`: a child keeps `t-files.g` as its working directory and
//!   `t-files.g/f` open while this one removes both and makes a new
//!   directory and file, which may get their inodes; the child finds its
//!   directory gone and its file too (`ENOENT`), and the new ones keep
//!   what they hold.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
````

Replace:

````rust
        Some(b"pwd") => pwd("my working directory"),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd\n");
            return 2;
````

with:

````rust
        Some(b"pwd") => pwd("my working directory"),
        Some(b"gone") => gone(),
        Some(b"gone-child") => gone_child(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd|gone\n");
            return 2;
````

Replace:

````rust
    sys::rmdir(b"t-files.c")
}
````

with:

````rust
    sys::rmdir(b"t-files.c")
}

fn gone() -> Result<(), u16> {
    sys::mkdir(b"t-files.g")?;
    put(b"t-files.g/f", b"old")?;
    let f = sys::open(b"t-files.g/f", OPEN_READ | OPEN_WRITE)?;
    let fds = [
        FdMap {
            child: 1,
            parent: 1,
        },
        FdMap {
            child: 2,
            parent: 2,
        },
        FdMap {
            child: 3,
            parent: f,
        },
    ];
    let pid = sys::spawn(
        b"/bin/t-files",
        b"t-files\0gone-child\0",
        b"t-files.g",
        &fds,
        0,
    )?;
    // While the child naps: remove its directory and file, and make new
    // ones, which may get their inodes.
    sys::unlink(b"t-files.g/f")?;
    sys::rmdir(b"t-files.g")?;
    sys::mkdir(b"t-files.n")?;
    put(b"t-files.n/new", b"new")?;
    show("read the removed file", sys::read(f, &mut [0; 4]));
    sys::wait(i64::from(pid), false)?;
    let r = sys::open(b"t-files.n/new", OPEN_READ)?;
    let mut buf = [0u8; 16];
    let n = sys::read(r, &mut buf)?;
    show_text("the new file holds", &buf[..n]);
    sys::close(r)?;
    show(
        "the new directory holds x",
        sys::stat(b"t-files.n/x", 0).map(|s| s.size),
    );
    sys::unlink(b"t-files.n/new")?;
    sys::rmdir(b"t-files.n")
}

fn gone_child() -> Result<(), u16> {
    sys::sleep(300);
    pwd("the child's working directory")?;
    show(
        "the child creates x",
        sys::open(b"x", OPEN_WRITE | OPEN_CREATE),
    );
    show("the child reads its file", sys::read(3, &mut [0; 4]));
    show("the child writes its file", sys::write(3, b"child"));
    Ok(())
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib fd::`

Expected: FAIL: compile errors such as `` no method named `follow` found for struct `fd::FdTable` in the current scope ``.

Run: `cargo xtask test --e2e-only --scenario files`

Expected: FAIL: scenario `files` stops at line 35, timed out waiting for `\nread the removed file: ENOENT\nthe child's working directory: /root/t-files.g\nthe child creates x: ENOENT\n`.

- [ ] **Step 5: Change `kernel/src/fd.rs`**

In `kernel/src/fd.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::spawn::SPAWN_FDS;
use vfs::Errno;

````

with:

````rust
use relay_abi::spawn::SPAWN_FDS;
use vfs::{Change, Errno};

````

Replace:

````rust
        self.slots.iter().flatten()
    }
````

with:

````rust
        self.slots.iter().flatten()
    }

    /// A removal elsewhere (spec §16 item 4): the open files of an inode it
    /// freed are gone.
    pub fn follow(&self, change: &Change) {
        let Some(node) = change.removed() else {
            return;
        };
        for file in self.files() {
            if let File::Vfs(open) = &**file
                && open.node() == node
            {
                open.mark_gone();
            }
        }
    }
````

- [ ] **Step 6: Change `kernel/src/mounts.rs`**

In `kernel/src/mounts.rs`, replace:

````rust
/// Runs `f` on the mount table with `cwd` as its current directory, and
/// keeps what `f` makes of it (a `chdir`) in `cwd`.
pub fn with<R>(cwd: &mut Cwd, f: impl FnOnce(&mut MountTable) -> R) -> R {
    let mut guard = MOUNTS.lock();
    let table = guard.0.as_mut().expect("mounts::init has not run");
    let theirs = table.swap_cwd(cwd.clone());
    let r = f(table);
    *cwd = table.swap_cwd(theirs);
    r
````

with:

````rust
/// Runs `f` on the mount table with `cwd` as its current directory, and
/// keeps what `f` makes of it (a `chdir`) in `cwd`. What `f` removed or
/// moved then reaches every process (spec §16 item 4): their current
/// directories follow it, and their open files of a freed inode are gone.
pub fn with<R>(cwd: &mut Cwd, f: impl FnOnce(&mut MountTable) -> R) -> R {
    let (r, changes) = {
        let mut guard = MOUNTS.lock();
        let table = guard.0.as_mut().expect("mounts::init has not run");
        let theirs = table.swap_cwd(cwd.clone());
        let r = f(table);
        *cwd = table.swap_cwd(theirs);
        (r, table.take_changes())
    };
    if !changes.is_empty() {
        crate::proc::follow_changes(&changes);
    }
    r
````

- [ ] **Step 7: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
    idle()
}
````

with:

````rust
    idle()
}

/// A removal or a move made through the mount table (spec §16 item 4): the
/// current directory of every process but the running one (which the
/// table changed itself) follows it, and every process's open files of an
/// inode it freed are gone.
pub fn follow_changes(changes: &[vfs::Change]) {
    let mut t = PROCS.lock();
    for p in t.iter_mut() {
        for c in changes {
            if let Some(cwd) = p.res.cwd.as_mut() {
                cwd.follow(c);
            }
            p.res.fds.follow(c);
        }
    }
}
````

- [ ] **Step 8: Change `kernel/src/proc/table.rs`**

In `kernel/src/proc/table.rs`, replace:

````rust
        self.procs.iter()
    }
````

with:

````rust
        self.procs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Process<R>> {
        self.procs.iter_mut()
    }
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 324 tests.

- [ ] **Step 10: Run the `files` scenario**

Run: `cargo xtask test --e2e-only --scenario files`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: a removal reaches every process: their working directories follow it, and open files of a freed inode are gone"
````


### Task 14: A program sees its redirection's write error, and its own file's `ENOSPC`

Plan 3a's final review left a gap: nothing tested a program seeing its redirection's write error through the kernel (`Current::shell_output` passing the in-kernel shell's answer on). `t-files full`, run in the `diskfull` scenario once the disk is full, writes to its standard output, which the shell redirected into a file, and to a file it opened itself, until each refuses, and says with what: `ENOSPC` both times, and the shell's own `write error` line after it. The code was right, so the task adds only the test and has no failing run; the mutation check shows what it guards: `shell_output` swallowing the shell's error makes `diskfull` stop at `writing standard output: ENOSPC`.

**Files:**
- Modify: `tests/e2e/diskfull.txt`
- Modify: `userland/tests/src/bin/t-files.rs`

**Interfaces:**
- Consumes: Tasks 11, plan 3a's output hook.
- Produces: `t-files full`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/diskfull.txt`**

In `tests/e2e/diskfull.txt`, replace:

````text
expect \necho: write error: No space left on device\n
send rm t
````

with:

````text
expect \necho: write error: No space left on device\n
# A program sees the write error too, through the kernel: on its
# standard output, which the shell redirected, and on a file it opened.
send t-files full > more
expect \nwriting standard output: ENOSPC\n
expect writing a file of its own: ENOSPC\nt-files: write error: No space left on device\n
send rm t
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-files.rs`**

In `userland/tests/src/bin/t-files.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   what they hold.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
````

with:

````rust
//!   what they hold.
//! - `t-files full`, on a full disk: writes to standard output (redirected
//!   by the shell into a file) and to a file it opens itself until each
//!   refuses, and says with what, on standard error.
//! - `t-files child`: reads 7 bytes from fd 3 and prints them (the child
````

Replace:

````rust
        Some(b"gone-child") => gone_child(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd|gone\n");
            return 2;
````

with:

````rust
        Some(b"gone-child") => gone_child(),
        Some(b"full") => full(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-files basic|dir|cwd|gone|full\n");
            return 2;
````

Replace:

````rust
    show("the child writes its file", sys::write(3, b"child"));
    Ok(())
}
````

with:

````rust
    show("the child writes its file", sys::write(3, b"child"));
    Ok(())
}

/// Writes 1 KiB at a time to `fd` until a write fails; its error, after
/// at most 64 MiB.
fn fill(fd: u32) -> Result<(), u16> {
    let chunk = [b'x'; 1024];
    for _ in 0..64 * 1024 {
        sys::write_all(fd, &chunk)?;
    }
    Ok(())
}

fn full() -> Result<(), u16> {
    let said = fill(1).err().map_or("nothing", name);
    let _ = writeln!(Fd(2), "writing standard output: {said}");
    let fd = sys::open(b"t-files.full", OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    let said = fill(fd).err().map_or("nothing", name);
    let _ = writeln!(Fd(2), "writing a file of its own: {said}");
    sys::close(fd)?;
    sys::unlink(b"t-files.full")
}
````

- [ ] **Step 3: Run the `diskfull` scenario**

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add tests userland
git commit -m "tests: a program sees its redirection's write error and its own file's ENOSPC through the kernel"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 31 scenario(s) passed`.

````bash
git push -u origin m2p3b/files
gh pr create --base main --head m2p3b/files --title "Milestone 2, plan 3b: Programs use files" --body-file - <<'EOF'
## What

Milestone 2, plan 3b, tasks 10–14: the dispatcher reaches the program's fds and the files through its `Caller`, and `write` goes through the file's kind; `open`, `close`, `read`, `seek`, `fstat`, `stat`, `read_dir`, `mkdir`, `rmdir`, `unlink`, `truncate`, `touch`, `readlink`, `rename`, `statfs`, `sync`, `chdir` and `getcwd` from ring 3, each checking what the program passes; a removal or a move reaches every process's working directory and open files, so none reaches an inode a new file got; `relay-rt`'s wrappers and `t-files` (the `files` scenario, and `diskfull`: a program sees its redirection's write error and its own file's `ENOSPC`).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed now: NUC check 3 runs it with PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3b/files --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-files
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Memory for programs (Tasks 15–16)

`mem_map` and `mem_unmap` from ring 3, and the heap every program gets from `relay-rt`.

Branch `m2p3b/memory`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-memory`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3b/memory /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-memory origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-memory
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3b/files` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3b/memory /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-memory m2p3b/files`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3b/files>` and re-run `cargo xtask ci` before pushing.

### Task 15: `mem_map` and `mem_unmap` from ring 3; `t-mem`

Spec §7.3's memory calls (decision 7): the length rounded up to pages (`EINVAL` for 0); the kernel side maps with Task 9's `map_area`, its room the frames above the 8 MiB reserve (`UserMem::room`), and after `unmap_area` flushes the program's TLB entries for the pages (`mm::flush_pages`, every entry for more than 64 pages), so a page given back faults at once. `relay-rt` gains `sys::{mem_map, mem_unmap}`, and `t-mem map` shows the area's first fit, zeroed pages, the refusals and no frame lost; `t-mem unmapped` and `unmapped-many` read a page after giving it back and are killed (the `memory` scenario); `system` lists six programs. Mutation checks: no TLB flush lets `t-mem unmapped` read its old value from a page it gave back, and `flush_all` skipped does the same for `unmapped-many`, both failing `memory`; rounding down fails a test; `UserMem::room` ignored only makes a huge map slow (it zeroes frames up to the reserve and rolls back to the same `ENOMEM`).

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Create: `tests/e2e/memory.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-mem.rs`

**Interfaces:**
- Consumes: Task 9, plan 2's `UserMem`.
- Produces: `Caller::{mem_map(&mut self, pages: u64) -> Result<u64, Errno>, mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno>}`; `mm::{UserMem::room(&self) -> u64, flush_pages(virt: u64, pages: u64)}`; `relay_rt::sys::{mem_map(usize) -> Result<usize, u16>, mem_unmap(usize, usize)}` (unsafe); `userland/tests/src/bin/t-mem.rs`; the `memory` scenario.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
    decode(unsafe { syscall(c, a) })
}
````

with:

````rust
    decode(unsafe { syscall(c, a) })
}

/// Maps `len` bytes (rounded up to pages) of fresh zeroed memory; its
/// address.
pub fn mem_map(len: usize) -> Result<usize, u16> {
    call(Call::MemMap, &[len as u64]).map(|a| a as usize)
}

/// Gives back `len` bytes (rounded up to pages) from `addr`, of memory
/// `mem_map` gave.
///
/// # Safety
/// Nothing may use that memory any more.
pub unsafe fn mem_unmap(addr: usize, len: usize) -> Result<(), u16> {
    call(Call::MemUnmap, &[addr as u64, len as u64]).map(|_| ())
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn every_other_call_is_enosys() {
````

with:

````rust
    #[test]
    fn mem_map_gives_whole_pages_and_mem_unmap_takes_them_back() {
        use crate::mm::space::MAP_START;
        let mut f = fake();
        assert_eq!(
            call(&mut f, Call::MemMap, [1, 0, 0]),
            Ok(MAP_START),
            "a page for a byte"
        );
        assert_eq!(
            call(&mut f, Call::MemMap, [PAGE + 1, 0, 0]),
            Ok(MAP_START + PAGE)
        );
        assert_eq!(f.space.maps(), [(MAP_START, 3)]);
        assert_eq!(call(&mut f, Call::MemMap, [0, 0, 0]), Err(errno::EINVAL));
        assert_eq!(
            call(&mut f, Call::MemMap, [u64::MAX, 0, 0]),
            Err(errno::ENOMEM)
        );
        f.room = 2;
        assert_eq!(
            call(&mut f, Call::MemMap, [3 * PAGE, 0, 0]),
            Err(errno::ENOMEM),
            "the reserve"
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START + PAGE, 1, 0]),
            Ok(0),
            "a byte is its page"
        );
        assert_eq!(f.space.maps(), [(MAP_START, 1), (MAP_START + 2 * PAGE, 1)]);
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START + 1, 1, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [U, PAGE, 0]),
            Err(errno::EINVAL),
            "not mem_map's"
        );
        assert_eq!(
            call(&mut f, Call::MemUnmap, [MAP_START, u64::MAX, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(f.unmapped, [(MAP_START + PAGE, 1)], "flushed once");
    }

    #[test]
    fn every_other_call_is_enosys() {
````

Replace:

````rust
            Call::Getpid,
            Call::Open,
````

with:

````rust
            Call::Getpid,
            Call::MemMap,
            Call::MemUnmap,
            Call::Open,
````

- [ ] **Step 3: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub heap_room: usize,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
    pub heap_room: usize,
    /// Frames `mem_map` may take.
    pub room: u64,
    /// What `mem_unmap` gave back (and the kernel would flush).
    pub unmapped: Vec<(u64, u64)>,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
        self.heap_room
    }
````

with:

````rust
        self.heap_room
    }
    fn mem_map(&mut self, pages: u64) -> Result<u64, Errno> {
        self.space.map_area(&mut self.mem, pages, self.room)
    }
    fn mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno> {
        self.space.unmap_area(&mut self.mem, addr, pages)?;
        self.unmapped.push((addr, pages));
        Ok(())
    }
````

Replace:

````rust
        heap_room: usize::MAX,
        written: Vec::new(),
````

with:

````rust
        heap_room: usize::MAX,
        room: u64::MAX,
        unmapped: Vec::new(),
        written: Vec::new(),
````

- [ ] **Step 4: Add the scenario `tests/e2e/memory.txt`**

Create `tests/e2e/memory.txt`:

````text
# Memory from mem_map (user-space gate §5.1, §7.3; milestone 2, plan 3b):
# the area's first fit, zeroed pages, mem_unmap of whole pages of earlier
# maps only, and no frame lost; a page given back is gone at once (its
# TLB entry flushed), so touching it kills the program.
timeout 60
expect root@relay:~# $
send t-mem map
expect \nat the area's start: true\nzeroed: true\nwritten: true\nmap nothing: EINVAL\nmap too much: ENOMEM\n
expect unmap the middle: ok\nunmap it again: EINVAL\nunmap half a page in: EINVAL\nunmap the program's code: EINVAL\n
expect the hole is filled first: true\nframes lost: 0\nroot@relay:~# $
send t-mem unmapped
expect \nrelay-sh: t-mem: killed \(page fault at 0x100000000000, read, ip 0x[0-9a-f]+\)\n
send t-mem unmapped-many
expect \nrelay-sh: t-mem: killed \(page fault at 0x1000000ff000, read, ip 0x[0-9a-f]+\)\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-mem map
expect frames lost: 0\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-spawn  t-spin\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-spawn  t-spin\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-spawn  t-spin\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-spawn  t-spin\n
````

- [ ] **Step 6: Add the test program `userland/tests/src/bin/t-mem.rs`**

Create `userland/tests/src/bin/t-mem.rs`:

````rust
//! `t-mem KIND`: memory from `mem_map` (spec §5.1, §7.3), each answer
//! printed as `<what>: <value or error name>`.
//!
//! - `t-mem map`: the first map at the area's start, zeroed and writable;
//!   `mem_unmap` of part of it, and of what it did not map; a hole filled
//!   first fit; too much refused; and the free frames the same after.
//! - `t-mem unmapped`: reads a page after giving it back, which must kill
//!   it (`killed (page fault at 0x100000000000, read, …)`); `t-mem
//!   unmapped-many` the last of 256 pages given back together.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::errno;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

/// Where the `mem_map` area starts (spec §5.1).
const AREA: usize = 0x1000_0000_0000;
const PAGE: usize = 4096;

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"map") => map(),
        Some(b"unmapped") => unmapped(1),
        Some(b"unmapped-many") => unmapped(256),
        _ => {
            let _ = sys::write_all(2, b"usage: t-mem map|unmapped|unmapped-many\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-mem: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn show<T: core::fmt::Display>(what: &str, r: Result<T, u16>) {
    let _ = match r {
        Ok(v) => writeln!(Fd(1), "{what}: {v}"),
        Err(e) => writeln!(Fd(1), "{what}: {}", errno::name(e).unwrap_or("?")),
    };
}

fn free() -> Result<u64, u16> {
    Ok(sys::memory()?.ram_free / 4096)
}

fn map() -> Result<(), u16> {
    // A first map makes the page tables of the area's first 4 MiB, which
    // stay: count after it.
    let warm = sys::mem_map(4 << 20)?;
    // SAFETY: nothing uses it.
    unsafe { sys::mem_unmap(warm, 4 << 20)? };
    let before = free()?;
    let a = sys::mem_map(3 * PAGE)?;
    let _ = writeln!(Fd(1), "at the area's start: {}", a == AREA);
    // SAFETY: three pages `mem_map` just gave.
    let mem = unsafe { core::slice::from_raw_parts_mut(a as *mut u8, 3 * PAGE) };
    let _ = writeln!(Fd(1), "zeroed: {}", mem.iter().all(|&b| b == 0));
    mem.fill(0xA5);
    let _ = writeln!(Fd(1), "written: {}", mem[3 * PAGE - 1] == 0xA5);
    show("map nothing", sys::mem_map(0));
    show("map too much", sys::mem_map(1 << 46));
    // SAFETY: the middle page is not used any more; the others are not
    // this program's, or not the start of a page.
    unsafe {
        show(
            "unmap the middle",
            sys::mem_unmap(a + PAGE, 1).map(|()| "ok"),
        );
        show(
            "unmap it again",
            sys::mem_unmap(a + PAGE, PAGE).map(|()| "ok"),
        );
        show(
            "unmap half a page in",
            sys::mem_unmap(a + PAGE / 2, PAGE).map(|()| "ok"),
        );
        show(
            "unmap the program's code",
            sys::mem_unmap(0x40_0000, PAGE).map(|()| "ok"),
        );
    }
    let hole = sys::mem_map(PAGE)?;
    let _ = writeln!(Fd(1), "the hole is filled first: {}", hole == a + PAGE);
    // SAFETY: nothing uses them any more.
    unsafe {
        sys::mem_unmap(a, 3 * PAGE)?;
    }
    // Many pages, mapped and given back.
    let big = sys::mem_map(4 << 20)?;
    unsafe { sys::mem_unmap(big, 4 << 20)? };
    let lost = i128::from(before) - i128::from(free()?);
    let _ = writeln!(Fd(1), "frames lost: {lost}");
    Ok(())
}

/// Maps `pages` pages, writes the last, gives them back and reads it.
fn unmapped(pages: usize) -> Result<(), u16> {
    let a = sys::mem_map(pages * PAGE)?;
    let last = a + (pages - 1) * PAGE;
    // SAFETY: a page `mem_map` gave.
    unsafe { (last as *mut u8).write_volatile(1) };
    // SAFETY: the read after it is the test: it must fault.
    unsafe {
        sys::mem_unmap(a, pages * PAGE)?;
        let b = (last as *const u8).read_volatile();
        let _ = writeln!(Fd(1), "read an unmapped page: {b}");
    }
    Ok(())
}
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` method `mem_map` is not a member of trait `Caller` ``; `` method `mem_unmap` is not a member of trait `Caller` ``.

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: FAIL: scenario `memory` stops at line 8, timed out waiting for `\nat the area's start: true\nzeroed: true\nwritten: true\nmap nothing: EINVAL\nmap too much: ENOMEM\n`.

- [ ] **Step 8: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 5 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 6 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 9: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

impl PhysMem for UserMem<'_> {
````

with:

````rust

impl UserMem<'_> {
    /// Frames a program may still take: those free beyond the reserve.
    pub fn room(&self) -> u64 {
        self.0.0.free_frames().saturating_sub(USER_RESERVE_FRAMES)
    }
}

impl PhysMem for UserMem<'_> {
````

Replace:

````rust
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
    }
````

with:

````rust
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
    }
}

/// Drops the running program's TLB entries for `pages` pages from `virt`,
/// which it no longer has: one by one for a few, all at once for many.
pub fn flush_pages(virt: u64, pages: u64) {
    if pages > 64 {
        x86_64::instructions::tlb::flush_all();
        return;
    }
    for k in 0..pages {
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt + k * paging::PAGE));
    }
````

- [ ] **Step 10: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

    fn console_write(&mut self, bytes: &[u8]) {
````

with:

````rust

    fn mem_map(&mut self, pages: u64) -> Result<u64, Errno> {
        let mut t = PROCS.lock();
        let me = t.current();
        let space = t
            .get_mut(me)
            .and_then(|p| p.res.space.as_mut())
            .ok_or(Errno::ENOMEM)?;
        mm::with_user_memory(|mem, _| {
            let room = mem.room();
            space.map_area(mem, pages, room)
        })
    }

    fn mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno> {
        let mut t = PROCS.lock();
        let me = t.current();
        let space = t
            .get_mut(me)
            .and_then(|p| p.res.space.as_mut())
            .ok_or(Errno::EINVAL)?;
        mm::with_user_memory(|mem, _| space.unmap_area(mem, addr, pages))?;
        // Its tables are the ones in CR3.
        mm::flush_pages(addr, pages);
        Ok(())
    }

    fn console_write(&mut self, bytes: &[u8]) {
````

- [ ] **Step 11: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    fn heap_room(&self) -> usize;
    /// Writes `bytes` to the screen.
````

with:

````rust
    fn heap_room(&self) -> usize;
    /// Maps `pages` fresh pages in the `mem_map` area; their address.
    fn mem_map(&mut self, pages: u64) -> Result<u64, Errno>;
    /// Gives back `pages` pages from `addr`, which `mem_map` gave.
    fn mem_unmap(&mut self, addr: u64, pages: u64) -> Result<(), Errno>;
    /// Writes `bytes` to the screen.
````

Replace:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::Open) => files::open(caller, args[0], args[1], args[2]),
````

with:

````rust
        Some(Call::Getpid) => Ok(u64::from(caller.pid())),
        Some(Call::MemMap) => mem_map(caller, args[0]),
        Some(Call::MemUnmap) => mem_unmap(caller, args[0], args[1]),
        Some(Call::Open) => files::open(caller, args[0], args[1], args[2]),
````

Replace:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
}

````

with:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
}

/// `mem_map(length)` (spec §7.3): fresh zeroed read-write pages, the
/// length rounded up to whole pages; their address. `EINVAL` for nothing,
/// `ENOMEM` when they do not fit or the frames would run too low.
fn mem_map(caller: &mut impl Caller, len: u64) -> Result<u64, Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    caller.mem_map(len.div_ceil(PAGE))
}

/// `mem_unmap(address, length)` (spec §7.3): whole pages of earlier
/// `mem_map`s, the length rounded up to pages; `EINVAL` otherwise.
fn mem_unmap(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    caller.mem_unmap(addr, len.div_ceil(PAGE)).map(|()| 0)
}

````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 325 tests.

- [ ] **Step 13: Run the `memory`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add crates docs kernel tests userland
git commit -m "kernel, relay-rt: mem_map and mem_unmap from ring 3; t-mem"
````


### Task 16: A heap that grows by `mem_map`; out of memory is a message and 134

Spec §8.1: every program's `#[global_allocator]` is a `heap::Heap` (Task 2) that grows by `mem_map`: when a block does not fit, `alloc_growing` maps the block's size, alignment and a slab, rounded up to whole mebibytes (`STEP`), adds it and tries again (decision 8). When `mem_map` refuses, the allocator itself prints `<name>: out of memory` and exits with 134, as a Rust program aborting on Linux does, rather than going through the panic handler (status 101). The growth is host-tested with host memory for `mem_map`; `t-mem grow 64` takes 64 MiB in blocks of every size and checks each, and `t-mem oom` starts a child that takes memory until there is none and reports how it ended (the `memory` scenario, with `free` the same before and after; `t-mem` cannot link without the allocator, so the failing run is the unit tests'). Mutation checks: a step without room for the alignment or a slab fails a test; exiting with 101 fails `memory`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/relay-rt/Cargo.toml`
- Create: `crates/relay-rt/src/allocator.rs`
- Modify: `crates/relay-rt/src/lib.rs`
- Modify: `tests/e2e/memory.txt`
- Modify: `userland/tests/src/bin/t-mem.rs`

**Interfaces:**
- Consumes: Tasks 2, 15.
- Produces: `relay_rt::allocator::{STEP, grow_size(Layout) -> Option<usize>, alloc_growing(&mut Heap, Layout, impl FnOnce(usize) -> Option<usize>) -> Option<NonNull<u8>>}` and the programs' `#[global_allocator]`; `t-mem grow N`, `oom`, `hog`.

- [ ] **Step 1: Change `crates/relay-rt/Cargo.toml`**

In `crates/relay-rt/Cargo.toml`, replace:

````toml
relay-abi.workspace = true
````

with:

````toml
relay-abi.workspace = true
heap.workspace = true
spin.workspace = true
````

- [ ] **Step 2: Write the failing tests for `crates/relay-rt/src/allocator.rs`**

Create `crates/relay-rt/src/allocator.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    /// `len` bytes of host memory, page-aligned, as `mem_map` would give.
    fn region(len: usize) -> usize {
        let layout = Layout::from_size_align(len, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        mem
    }

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn growth_comes_in_whole_steps_with_room_for_the_block() {
        assert_eq!(grow_size(l(1, 1)), Some(STEP));
        assert_eq!(grow_size(l(STEP, 16)), Some(2 * STEP));
        assert_eq!(grow_size(l(STEP - 4096 - 4096, 4096)), Some(STEP));
        assert_eq!(grow_size(l(STEP - 4096 - 4095, 4096)), Some(2 * STEP));
        // The biggest a layout can be still has a size to map (which
        // `mem_map` refuses).
        let most = l(isize::MAX as usize - 15, 16);
        assert_eq!(grow_size(most), Some((1 << 63) + STEP));
    }

    #[test]
    fn an_empty_heap_grows_by_a_step_and_serves_from_it() {
        let mut heap = Heap::empty();
        let mut asked = Vec::new();
        let a = alloc_growing(&mut heap, l(100, 8), |len| {
            asked.push(len);
            Some(region(len))
        })
        .unwrap();
        assert_eq!(asked, [STEP]);
        // The next ones come from the same step without mapping.
        for _ in 0..100 {
            alloc_growing(&mut heap, l(1000, 8), |_| panic!("no need to grow")).unwrap();
        }
        assert_eq!(heap.stats().total, STEP);
        unsafe { heap.dealloc(a, l(100, 8)) };
    }

    #[test]
    fn a_big_block_gets_a_big_enough_step() {
        let mut heap = Heap::empty();
        let size = 3 * STEP + 5;
        let p = alloc_growing(&mut heap, l(size, 4096), |len| {
            assert_eq!(len, 4 * STEP);
            Some(region(len))
        })
        .unwrap();
        assert_eq!(p.as_ptr() as usize % 4096, 0);
        unsafe { core::ptr::write_bytes(p.as_ptr(), 1, size) };
    }

    #[test]
    fn steps_that_meet_merge_into_one_region() {
        // `mem_map` gives the area in order: each step after the last.
        let area = region(4 * STEP);
        let mut next = area;
        let mut map = |len: usize| {
            let at = next;
            next += len;
            Some(at)
        };
        let mut heap = Heap::empty();
        let a = alloc_growing(&mut heap, l(STEP - 8192, 16), &mut map).unwrap();
        // More than is left of the first step: a second step, which meets
        // it, so the free end of the first is used too.
        let b = alloc_growing(&mut heap, l(STEP / 2 + 8192, 16), &mut map).unwrap();
        assert_eq!(b.as_ptr() as usize, a.as_ptr() as usize + STEP - 8192);
        assert_eq!(heap.stats().total, 2 * STEP);
    }

    #[test]
    fn nothing_to_map_is_nothing_allocated() {
        let mut heap = Heap::empty();
        assert_eq!(alloc_growing(&mut heap, l(16, 8), |_| None), None);
        assert_eq!(heap.stats().total, 0);
    }
}
````

- [ ] **Step 3: Declare the new module in `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
````

with:

````rust

mod allocator;
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/memory.txt`**

In `tests/e2e/memory.txt`, replace:

````text
expect frames lost: 0\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

with:

````text
expect frames lost: 0\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# The heap of relay-rt grows by mem_map (spec §8.1); when it cannot, the
# program says so and exits with 134; either way its memory comes back.
send t-mem grow 64
expect \n\d+ blocks, 64 MiB, all there: true\nroot@relay:~# $
send t-mem oom
expect \nt-mem: out of memory\nthe hog: exited with 134\nroot@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-mem grow 1
expect all there: true\n
````

- [ ] **Step 5: Change the test program `userland/tests/src/bin/t-mem.rs`**

In `userland/tests/src/bin/t-mem.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   unmapped-many` the last of 256 pages given back together.
#![no_std]
#![no_main]

use core::fmt::Write;
````

with:

````rust
//!   unmapped-many` the last of 256 pages given back together.
//! - `t-mem grow N`: `N` MiB on the heap (spec §8.1) in blocks of every
//!   size, each checked, which `relay-rt` maps as it grows.
//! - `t-mem oom`: starts `t-mem hog`, which takes heap until there is
//!   none (`t-mem: out of memory`), and says how it ended: status 134.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
````

Replace:

````rust
        Some(b"unmapped-many") => unmapped(256),
        _ => {
            let _ = sys::write_all(2, b"usage: t-mem map|unmapped|unmapped-many\n");
            return 2;
        }
    };
````

with:

````rust
        Some(b"unmapped-many") => unmapped(256),
        Some(b"grow") => match args.get(2).and_then(parse) {
            Some(n) => grow(n),
            None => return usage(),
        },
        Some(b"oom") => oom(),
        Some(b"hog") => hog(),
        _ => return usage(),
    };
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
    Ok(())
}

fn usage() -> u8 {
    let _ = sys::write_all(2, b"usage: t-mem map|unmapped|unmapped-many|grow N|oom\n");
    2
}

/// A decimal number.
fn parse(s: &[u8]) -> Option<usize> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0usize, |n, &c| {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        n.checked_mul(10)?.checked_add(usize::from(d))
    })
}

/// `mib` MiB in blocks of 16 bytes to 256 KiB, each filled with its own
/// byte and checked once all are there.
fn grow(mib: usize) -> Result<(), u16> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut total = 0;
    let mut size = 16;
    while total < mib << 20 {
        let tag = blocks.len() as u8;
        blocks.push(alloc::vec![tag; size]);
        total += size;
        size = if size >= 256 << 10 { 16 } else { size * 2 };
    }
    let all_there = blocks
        .iter()
        .enumerate()
        .all(|(i, b)| b.iter().all(|&x| x == i as u8));
    let _ = writeln!(
        Fd(1),
        "{} blocks, {} MiB, all there: {all_there}",
        blocks.len(),
        total >> 20
    );
    Ok(())
}

fn oom() -> Result<(), u16> {
    let fds = [
        relay_abi::FdMap {
            child: 1,
            parent: 1,
        },
        relay_abi::FdMap {
            child: 2,
            parent: 2,
        },
    ];
    let pid = sys::spawn(b"/bin/t-mem", b"t-mem\0hog\0", b"", &fds, 0)?;
    if let Some((_, w)) = sys::wait(i64::from(pid), false)? {
        let _ = writeln!(Fd(1), "the hog: {w}");
    }
    Ok(())
}

/// Takes the heap a mebibyte at a time until `relay-rt` says it is out.
fn hog() -> Result<(), u16> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    loop {
        blocks.push(alloc::vec![1; 1 << 20]);
    }
}
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find type `Layout` in this scope ``; `` cannot find value `STEP` in this scope ``.

- [ ] **Step 7: Implement `crates/relay-rt/src/allocator.rs`**

Insert this at the top of `crates/relay-rt/src/allocator.rs`, above `#[cfg(test)]`:

````rust
//! The program's heap (spec §8.1): `crates/heap` over memory from
//! `mem_map`, growing a mebibyte or more at a time. Memory that meets the
//! heap's last region merges with it, as `mem_map` fills its area from the
//! start. When the heap cannot grow, the program says `<name>: out of
//! memory` and exits with 134, as a Rust program aborting on Linux does.
//! Nothing is given back to the kernel before the program ends.
// Off Relay OS only the tests use it.
#![cfg_attr(not(target_os = "none"), allow(dead_code))]

use core::alloc::Layout;
use core::ptr::NonNull;
use heap::Heap;

/// The heap grows by multiples of this.
pub const STEP: usize = 1 << 20;

/// What to map so the heap can serve `layout`: its size with room for its
/// alignment and for a new slab of small blocks, in whole steps (`None`
/// cannot happen for a valid layout, whose size is below 2^63).
pub fn grow_size(layout: Layout) -> Option<usize> {
    layout
        .size()
        .checked_add(layout.align())?
        .checked_add(4096)?
        .checked_next_multiple_of(STEP)
}

/// A block of `layout` from `heap`, which grows by what `map` gives (a
/// length in, an address out) when it has no room. `None` if `map` has
/// nothing.
pub fn alloc_growing(
    heap: &mut Heap,
    layout: Layout,
    map: impl FnOnce(usize) -> Option<usize>,
) -> Option<NonNull<u8>> {
    if let Some(p) = heap.alloc(layout) {
        return Some(p);
    }
    let size = grow_size(layout)?;
    let at = map(size)?;
    // SAFETY: `map` gave `size` bytes of fresh memory that nothing else
    // uses, page-aligned.
    unsafe { heap.add(at, size) };
    heap.alloc(layout)
}

/// The `#[global_allocator]` of every program.
#[cfg(target_os = "none")]
mod on_relay {
    use super::alloc_growing;
    use crate::sys;
    use core::alloc::{GlobalAlloc, Layout};
    use core::ptr::NonNull;
    use heap::Heap;
    use spin::Mutex;

    struct Allocator(Mutex<Heap>);

    // SAFETY: blocks come from the heap, which hands each out once.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let got = alloc_growing(&mut self.0.lock(), layout, |len| sys::mem_map(len).ok());
            match got {
                Some(p) => p.as_ptr(),
                None => out_of_memory(),
            }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if let Some(p) = NonNull::new(ptr) {
                // SAFETY: `ptr` came from `alloc` with this layout.
                unsafe { self.0.lock().dealloc(p, layout) };
            }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator(Mutex::new(Heap::empty()));

    /// `<name>: out of memory`, and the end (spec §8.1).
    fn out_of_memory() -> ! {
        let _ = sys::write_all(2, crate::name());
        let _ = sys::write_all(2, b": out of memory\n");
        sys::exit(134)
    }
}

````

- [ ] **Step 8: Change `crates/relay-rt/src/lib.rs`**

In `crates/relay-rt/src/lib.rs`, replace:

````rust
//! The runtime every Relay OS program links (spec §8.1 of the user-space
//! gate): the entry point, the arguments, system-call wrappers, the panic
//! handler and the ELF note that names the ABI.
//!
````

with:

````rust
//! The runtime every Relay OS program links (spec §8.1 of the user-space
//! gate): the entry point, the arguments, system-call wrappers, the heap
//! (`alloc` works in every program), the panic handler and the ELF note
//! that names the ABI.
//!
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 11 tests.

- [ ] **Step 10: Run the `memory` scenario**

Run: `cargo xtask test --e2e-only --scenario memory`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add Cargo.lock crates tests userland
git commit -m "relay-rt: a heap that grows by mem_map; out of memory is a message and 134"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 32 scenario(s) passed`.

````bash
git push -u origin m2p3b/memory
gh pr create --base main --head m2p3b/memory --title "Milestone 2, plan 3b: Memory for programs" --body-file - <<'EOF'
## What

Milestone 2, plan 3b, tasks 15–16: `mem_map` and `mem_unmap` from ring 3, with the TLB flushed for what is given back and the 8 MiB reserve kept; `relay-rt`'s `#[global_allocator]` grows by `mem_map` in whole mebibytes, and a program that cannot get more says `<name>: out of memory` and exits with 134; `t-mem` (the `memory` scenario).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed now: NUC check 3 runs it with PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3b/memory --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-memory
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: The console, tees and the NUC (Tasks 17–25)

Reading the console through the line discipline, the `console_*` calls, tees, `sys_info`'s names and kernel log, `power`, and NUC check 3 with all of plan 3b. It ends with NUC check 3 on a stick written by `cargo xtask flash --full` from this worktree; the pull request stays a draft until the user reports its results, and the real transcripts go into a commit of this same pull request.

Branch `m2p3b/console`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-console`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p3b/console /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-console origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-console
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m2p3b/memory` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p3b/console /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-console m2p3b/memory`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m2p3b/memory>` and re-run `cargo xtask ci` before pushing.

### Task 17: Programs read the console; `console_mode`, `console_size`, `console_foreground`; `t-read`

Spec §6.4 and §6.5 (decision 9). The input queue holds Task 4's discipline: in line mode every key goes through it as it is typed, wherever the console is polled (a tick that interrupts a program, the idle task, a read), so what is typed is echoed at once, even while no program reads; the switch to line mode hands it what was typed ahead, and the way back to raw mode hands back what was typed and not read, as bash sees it, and a Ctrl-C nobody took as a raw one (plan 3a's ruling that such a Ctrl-C was swallowed no longer holds). `tty::read` gives a line in line mode and what was typed in raw mode. A console read (`Caller::console_read`) is for the foreground group only, any other process getting end of input at once; it blocks until input comes, `EINTR` if the reader is killed, and the dispatcher checks its buffer writable before it takes anything from the console, and asks for at most a line. `console_mode`, `console_size` (80×25 without a console) and `console_foreground` (a group some process is in; a change wakes the readers) come with it (decision 10), and `relay-rt` gains their wrappers. `t-read` prints what each read got: typed over COM1 and on the USB keyboard, with Backspace and Ctrl-D, raw mode, and a process in a group of its own (the `console` scenario, which needs the runner's new step, so the failing runs are the unit tests'). The e2e runner gains `type TEXT`, keys without the Enter `key` adds, for Ctrl-D on an empty line. `system` lists seven programs; the by-hand step of typing during `t-spin 5` now shows the echo. Mutation checks: type-ahead dropped at the switch, an untaken Ctrl-C not handed back, `ESC x` cut in two, a Ctrl-C keeping a half serial sequence and the console read's writable check each fail a test; reads outside the foreground group that wait, and the poll's echo not written, fail `console`; the wake-up at a change of foreground group or mode has no test.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/files.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/tty.rs`
- Create: `tests/e2e/console.txt`
- Modify: `tests/e2e/system.txt`
- Create: `userland/tests/src/bin/t-read.rs`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/keys.rs`

**Interfaces:**
- Consumes: Tasks 4, 3, 11.
- Produces: `InputQueue::{set_line_mode(bool) -> bool, is_line_mode, take_line_interrupt() -> bool, take_echo() -> Vec<u8>, read(&mut [u8]) -> Option<usize>}`; `tty::{set_line_mode, is_line_mode, foreground() -> u32, read(&mut [u8]) -> Option<usize>}`; `Caller::{console_read(&mut self, &mut [u8]) -> Result<usize, Errno>, console_mode(&mut self, bool) -> bool, console_size(&self) -> (u32, u32), console_foreground(&mut self, u32) -> Result<(), Errno>}`; `relay_rt::sys::{console_mode, console_size, console_foreground}`; `userland/tests/src/bin/t-read.rs`; the e2e step `type`; the `console` scenario.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
    decode(unsafe { syscall(c, a) })
}
````

with:

````rust
    decode(unsafe { syscall(c, a) })
}

/// Sets the console's mode (`relay_abi::console`'s `MODE_RAW` or
/// `MODE_LINE`); the previous one.
pub fn console_mode(mode: u32) -> Result<u32, u16> {
    call(Call::ConsoleMode, &[u64::from(mode)]).map(|m| m as u32)
}

/// The console's columns and rows.
pub fn console_size() -> (u32, u32) {
    relay_abi::console::size_of_result(call(Call::ConsoleSize, &[]).unwrap_or(0))
}

/// Gives the console to process group `pgid`.
pub fn console_foreground(pgid: u32) -> Result<(), u16> {
    call(Call::ConsoleForeground, &[u64::from(pgid)]).map(|_| ())
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    }

    /// A queue in line mode.
    fn line_mode() -> InputQueue {
        let mut q = InputQueue::new();
        q.set_line_mode(true);
        q
    }

    fn read(q: &mut InputQueue, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0; n];
        q.read(&mut buf).map(|k| buf[..k].to_vec())
    }

    #[test]
    fn in_line_mode_keys_are_echoed_and_read_a_line_at_a_time() {
        let mut q = line_mode();
        assert!(q.is_line_mode());
        q.push_key(&press(Key::Char(b'h'), false));
        serial(&mut q, b"i\x7f\x7fok");
        assert_eq!(read(&mut q, 10), None, "no line yet");
        assert!(q.is_empty());
        q.push_key(&press(Key::Enter, false));
        assert_eq!(q.take_echo(), b"hi\x08 \x08\x08 \x08ok\n");
        assert!(q.take_echo().is_empty(), "taken");
        assert!(!q.is_empty());
        assert_eq!(read(&mut q, 10).unwrap(), b"ok\n");
        assert_eq!(q.pop(), None, "nothing raw");
    }

    #[test]
    fn an_escape_sequence_is_one_key_in_line_mode() {
        let mut q = line_mode();
        q.push_key(&press(Key::Up, false));
        serial(&mut q, b"a\x1b[3~b\x1bxc\r");
        q.push(b"d\x1b[1;5Ce\r");
        assert_eq!(
            read(&mut q, 10).unwrap(),
            b"abc\n",
            "Delete and Alt-x do nothing"
        );
        assert_eq!(read(&mut q, 10).unwrap(), b"de\n");
        assert_eq!(q.take_echo(), b"abc\nde\n");
    }

    #[test]
    fn a_ctrl_c_in_line_mode_is_for_the_foreground_group() {
        let mut q = line_mode();
        q.push(b"one\rtw");
        assert!(!q.take_line_interrupt());
        serial(&mut q, b"\x1b[");
        q.push_key(&press(Key::Char(b'c'), true));
        assert!(q.take_line_interrupt());
        assert!(!q.take_line_interrupt(), "taken");
        assert!(q.is_empty(), "what was typed is dropped");
        serial(&mut q, b"A\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"A\n", "and the half sequence");
        assert!(!q.take_interrupt(), "not a raw Ctrl-C");
    }

    #[test]
    fn what_was_typed_ahead_goes_to_the_line_discipline_and_back() {
        let mut q = InputQueue::new();
        q.push(b"ls\r\x1b[Ap");
        assert!(!q.set_line_mode(true), "it was raw");
        assert_eq!(q.take_echo(), b"ls\np", "echoed when it goes in");
        assert_eq!(read(&mut q, 10).unwrap(), b"ls\n");
        q.push(b"wd\rec");
        assert_eq!(q.take_echo(), b"wd\nec");
        assert!(q.set_line_mode(false));
        assert_eq!(drain(&mut q), b"pwd\nec", "unread lines and the line typed");
        assert!(!q.set_line_mode(false), "already raw: nothing moves");
        // A Ctrl-C typed ahead kills the group the console goes to.
        q.push(b"x\x03y");
        q.set_line_mode(true);
        assert!(q.take_line_interrupt());
        assert_eq!(q.take_echo(), b"y");
    }

    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
        // Typed the moment a command ended: the shell's line editor and a
        // script see it, instead of nobody.
        let mut q = line_mode();
        q.push(b"abc\x03de");
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"de");
    }

    #[test]
    fn a_raw_read_takes_what_there_is() {
        let mut q = InputQueue::new();
        assert_eq!(read(&mut q, 10), None);
        q.push(b"abc\x1b[A");
        assert_eq!(read(&mut q, 2).unwrap(), b"ab");
        assert_eq!(read(&mut q, 0).unwrap(), b"");
        assert_eq!(read(&mut q, 10).unwrap(), b"c\x1b[A");
        assert!(q.is_empty());
    }

    #[test]
    fn keys_are_cut_where_a_terminal_sends_them() {
        let got: Vec<&[u8]> = keys(b"a\x1b[1;5Cb\x1bxc\x1b[").collect();
        assert_eq!(
            got,
            [&b"a"[..], b"\x1b[1;5C", b"b", b"\x1bx", b"c", b"\x1b["]
        );
        assert_eq!(keys(b"\x1b").collect::<Vec<_>>(), [&b"\x1b"[..]]);
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 3: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn every_other_call_is_enosys() {
````

with:

````rust
    #[test]
    fn the_console_s_mode_size_and_foreground() {
        use relay_abi::console::{MODE_LINE, MODE_RAW, size_of_result};
        let mut f = fake();
        let line = u64::from(MODE_LINE);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [line, 0, 0]),
            Ok(u64::from(MODE_RAW))
        );
        assert!(f.line_mode);
        assert_eq!(
            call(&mut f, Call::ConsoleMode, [0, 0, 0]),
            Ok(line),
            "the previous one"
        );
        assert!(!f.line_mode);
        for bad in [2, 1 << 32, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::ConsoleMode, [bad, 0, 0]),
                Err(errno::EINVAL)
            );
        }
        let size = call(&mut f, Call::ConsoleSize, [0, 0, 0]).unwrap();
        assert_eq!(size_of_result(size), (120, 33));
        assert_eq!(call(&mut f, Call::ConsoleForeground, [42, 0, 0]), Ok(0));
        assert_eq!(f.foreground, 42);
        for bad in [0, 7, 1 << 32 | 42, u64::MAX] {
            assert_eq!(
                call(&mut f, Call::ConsoleForeground, [bad, 0, 0]),
                Err(errno::ESRCH),
                "{bad}"
            );
        }
        assert_eq!(f.foreground, 42);
    }

    #[test]
    fn every_other_call_is_enosys() {
````

Replace:

````rust
            Call::Getcwd,
            Call::Time,
````

with:

````rust
            Call::Getcwd,
            Call::ConsoleMode,
            Call::ConsoleSize,
            Call::ConsoleForeground,
            Call::Time,
````

- [ ] **Step 4: Add the failing tests to `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, replace:

````rust
    #[test]
    fn reading_the_console_waits_for_its_line_discipline() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::Read, [0, W, 1]), Err(errno::ENOSYS));
    }
````

with:

````rust
    #[test]
    fn a_console_read_copies_out_what_the_console_gives() {
        let mut f = fake();
        f.typed.push_back(b"hello\n".to_vec());
        assert_eq!(call(&mut f, Call::Read, [0, W, 100]), Ok(6));
        assert_eq!(get(&mut f, W, 6), b"hello\n");
        assert_eq!(f.asked, [100], "the length it asked for");
        // A bad buffer takes nothing from the console.
        f.typed.push_back(b"kept\n".to_vec());
        assert_eq!(call(&mut f, Call::Read, [0, U, 10]), Err(errno::EFAULT));
        assert_eq!(
            call(&mut f, Call::Read, [0, W + PAGE - 2, 10]),
            Err(errno::EFAULT)
        );
        assert_eq!(f.typed.len(), 1, "still there");
        // At most a line's worth is asked for, and checked, whatever the
        // buffer's length.
        assert_eq!(call(&mut f, Call::Read, [0, W, 1 << 40]), Ok(5));
        assert_eq!(f.asked, [100, crate::line::LINE_MAX]);
        // Nothing more: end of input.
        assert_eq!(call(&mut f, Call::Read, [0, W, 10]), Ok(0));
        f.killed_while_reading = true;
        assert_eq!(call(&mut f, Call::Read, [0, W, 10]), Err(errno::EINTR));
    }
````

- [ ] **Step 5: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub unmapped: Vec<(u64, u64)>,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
    pub unmapped: Vec<(u64, u64)>,
    /// What each console read gets, then end of input; the lengths asked;
    /// whether a read ends in a kill.
    pub typed: alloc::collections::VecDeque<Vec<u8>>,
    pub asked: Vec<usize>,
    pub killed_while_reading: bool,
    /// The console's mode and foreground group (groups 1 and 42 exist).
    pub line_mode: bool,
    pub foreground: u32,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
        self.written.push((0, bytes.to_vec()));
    }
````

with:

````rust
        self.written.push((0, bytes.to_vec()));
    }
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        if self.killed_while_reading {
            return Err(Errno::EINTR);
        }
        self.asked.push(buf.len());
        let Some(line) = self.typed.pop_front() else {
            return Ok(0);
        };
        let n = line.len().min(buf.len());
        buf[..n].copy_from_slice(&line[..n]);
        Ok(n)
    }
    fn console_mode(&mut self, line: bool) -> bool {
        core::mem::replace(&mut self.line_mode, line)
    }
    fn console_size(&self) -> (u32, u32) {
        (120, 33)
    }
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno> {
        if ![1, 42].contains(&pgid) {
            return Err(Errno::ESRCH);
        }
        self.foreground = pgid;
        Ok(())
    }
````

Replace:

````rust
        unmapped: Vec::new(),
        written: Vec::new(),
````

with:

````rust
        unmapped: Vec::new(),
        typed: alloc::collections::VecDeque::new(),
        asked: Vec::new(),
        killed_while_reading: false,
        line_mode: false,
        foreground: 1,
        written: Vec::new(),
````

- [ ] **Step 6: Add the scenario `tests/e2e/console.txt`**

Create `tests/e2e/console.txt`:

````text
# Programs read the console (user-space gate §6.4, §6.5; milestone 2, plan
# 3b): in line mode a line at a time, echoed as it is typed, with
# Backspace, and Ctrl-D for end of input; in raw mode bytes as typed; a
# process outside the foreground group gets end of input at once.
timeout 60
expect root@relay:~# $
send t-read size
expect \nconsole: \d+x\d+\nmode 7: EINVAL\nforeground 999999: ESRCH\nforeground of my own group: ok\n
# Over COM1 and on the USB keyboard.
send t-read
alive 1
send hello
expect \nhello\n\[6\] hello\\n\n
key wrold{backspace}{backspace}{backspace}{backspace}orld
expect wrold(\x08 \x08){4}orld\n\[6\] world\\n\n
# Ctrl-D hands over a line without its newline; on an empty line it ends
# the input.
type abc{ctrl-d}
expect abc\[3\] abc\n
type {ctrl-d}
expect end of input\nroot@relay:~# $
# Typed while a program does not read: echoed at once, and the shell's
# afterwards, as in bash.
send t-spin 2
alive 1
send echo typed
expect echo typed\n\d+ iterations\nroot@relay:~# echo typed\ntyped\nroot@relay:~# $
# Raw mode: no echo, bytes as they come, until q.
send t-read raw
expect was line mode: true\n
send xyq
expect \[\d+\] xy
expect root@relay:~# $
send t-read apart
expect \nend of input\nroot@relay:~# $
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-spawn  t-spin\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-spawn  t-spin\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin\n
````

- [ ] **Step 8: Add the test program `userland/tests/src/bin/t-read.rs`**

Create `userland/tests/src/bin/t-read.rs`:

````rust
//! `t-read [KIND]`: reads the console (spec §6.4, §6.5) and prints each
//! read as `[<bytes>] <what it got>`, a newline as `\n` and other control
//! bytes as `^X`, until end of input.
//!
//! - `t-read`: in line mode, as the shell gives it the console: what was
//!   typed, a line at a time, echoed as it is typed.
//! - `t-read raw`: in raw mode, bytes as they are typed and not echoed,
//!   until `q`; then the mode it found again.
//! - `t-read apart`: starts `t-read` in a process group of its own, which
//!   is not the console's, so it gets end of input at once.
//! - `t-read size`: the console's columns and rows, and what
//!   `console_mode` and `console_foreground` refuse.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::console::{MODE_LINE, MODE_RAW};
use relay_abi::errno;
use relay_abi::spawn::NEW_GROUP;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        None => lines(),
        Some(b"raw") => raw(),
        Some(b"apart") => apart(),
        Some(b"size") => size(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|size]\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-read: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

/// `[<n>] <bytes>`.
fn show(bytes: &[u8]) {
    let _ = write!(Fd(1), "[{}] ", bytes.len());
    for &b in bytes {
        let _ = match b {
            b'\n' => sys::write_all(1, b"\\n"),
            0..=0x1F | 0x7F => sys::write_all(1, &[b'^', b ^ 0x40]),
            _ => sys::write_all(1, &[b]),
        };
    }
    let _ = sys::write_all(1, b"\n");
}

/// Reads until end of input, 100 reads at most.
fn lines() -> Result<(), u16> {
    let mut buf = [0u8; 64];
    for _ in 0..100 {
        match sys::read(0, &mut buf)? {
            0 => {
                let _ = sys::write_all(1, b"end of input\n");
                return Ok(());
            }
            n => show(&buf[..n]),
        }
    }
    Ok(())
}

fn raw() -> Result<(), u16> {
    let was = sys::console_mode(MODE_RAW)?;
    let _ = writeln!(Fd(1), "was line mode: {}", was == MODE_LINE);
    let mut buf = [0u8; 16];
    for _ in 0..100 {
        let n = sys::read(0, &mut buf)?;
        show(&buf[..n]);
        if buf[..n].contains(&b'q') {
            break;
        }
    }
    sys::console_mode(was)?;
    Ok(())
}

fn apart() -> Result<(), u16> {
    let fds = [
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
    let pid = sys::spawn(b"/bin/t-read", b"t-read\0", b"", &fds, NEW_GROUP)?;
    sys::wait(i64::from(pid), false).map(|_| ())
}

fn size() -> Result<(), u16> {
    let (columns, rows) = sys::console_size();
    let _ = writeln!(Fd(1), "console: {columns}x{rows}");
    let name = |r: Result<u32, u16>| r.map_or_else(|e| errno::name(e).unwrap_or("?"), |_| "ok");
    let _ = writeln!(Fd(1), "mode 7: {}", name(sys::console_mode(7)));
    let fg = sys::console_foreground(999_999).map(|()| 0);
    let _ = writeln!(Fd(1), "foreground 999999: {}", name(fg));
    let me = sys::getpid();
    let _ = writeln!(
        Fd(1),
        "foreground of my own group: {}",
        name(sys::console_foreground(me).map(|()| 0))
    );
    Ok(())
}
````

- [ ] **Step 9: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
        assert!(parse_scenario("x", "key {bogus}").is_err());
    }
````

with:

````rust
        assert!(parse_scenario("x", "key {bogus}").is_err());
        let s = parse_scenario("x", "type {ctrl-d}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Type("{ctrl-d}".into()))]);
        assert!(parse_scenario("x", "type {bogus}").is_err());
    }
````

- [ ] **Step 10: Add the failing tests to `xtask/src/keys.rs`**

In `xtask/src/keys.rs`, replace:

````rust
    #[test]
    fn characters_become_keys_with_shift_where_needed() {
````

with:

````rust
    #[test]
    fn typed_text_has_no_enter_after_it() {
        assert_eq!(typed("a{ctrl-d}").unwrap(), [vec!["a"], vec!["ctrl", "d"]]);
        assert_eq!(typed("").unwrap(), Vec::<Vec<&str>>::new());
        assert_eq!(presses("").unwrap(), [vec!["ret"]]);
        assert!(typed("{bogus}").is_err());
    }

    #[test]
    fn characters_become_keys_with_shift_where_needed() {
````

- [ ] **Step 11: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `keys` in this scope ``; `` method `console_read` is not a member of trait `Caller` ``.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: FAIL: compile errors such as `` cannot find function `typed` in this scope ``; `` no variant, associated function, or constant named `Type` found for enum `e2e::Step` in the current scope ``.

- [ ] **Step 12: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 6 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 7 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

Replace:

````markdown
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
     K120. After five seconds its `… iterations` line comes, then `typed`:
     the keyboard is polled on every tick that interrupts a program.
   - `t-spin`, then Ctrl-C: `^C` and the prompt come back at once, and
````

with:

````markdown
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
     K120: the line shows as it is typed (milestone 2, plan 3b: the line
     discipline echoes it). After five seconds its `… iterations` line
     comes, then the prompt with `echo typed` again, then `typed`: the
     keyboard is polled on every tick that interrupts a program, and what
     the program did not read is the shell's, as in bash.
   - `t-spin`, then Ctrl-C: `^C` and the prompt come back at once, and
````

- [ ] **Step 13: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! keyboard from a serial line.

use alloc::collections::VecDeque;
````

with:

````rust
//! keyboard from a serial line.
//!
//! In line mode (user-space gate §6.5) every key goes to the line
//! discipline as it is typed, which echoes it at once; a program reads
//! the lines. Back in raw mode what was typed and not read is raw input
//! again, as a Linux terminal's is.

use crate::line::LineDiscipline;
use alloc::collections::VecDeque;
````

Replace:

````rust
pub struct InputQueue {
    bytes: VecDeque<u8>,
````

with:

````rust
pub struct InputQueue {
    /// Raw input.
    bytes: VecDeque<u8>,
````

Replace:

````rust
    sequence_since: u64,
}
````

with:

````rust
    sequence_since: u64,
    /// Line mode: keys go to `line`.
    line_mode: bool,
    line: LineDiscipline,
    /// What the line discipline echoed, for the screen.
    echo: Vec<u8>,
    /// A Ctrl-C typed in line mode, for the foreground group.
    interrupted: bool,
}
````

Replace:

````rust
            sequence_since: 0,
        }
    }
````

with:

````rust
            sequence_since: 0,
            line_mode: false,
            line: LineDiscipline::new(),
            echo: Vec::new(),
            interrupted: false,
        }
    }

    /// Line mode (`true`) or raw mode; the previous one. Going to line mode
    /// hands what was typed ahead to the line discipline (a Ctrl-C in it is
    /// for the new foreground group); going back hands what was typed and
    /// not read back as raw input, and a Ctrl-C nobody took as a raw one.
    pub fn set_line_mode(&mut self, line: bool) -> bool {
        let was = core::mem::replace(&mut self.line_mode, line);
        if line && !was {
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            self.push(&ahead);
        } else if !line && was {
            let typed = self.line.take_all();
            if core::mem::take(&mut self.interrupted) {
                self.push(&[INTERRUPT]);
            }
            self.push(&typed);
        }
        was
    }

    pub fn is_line_mode(&self) -> bool {
        self.line_mode
    }

    /// Whether a Ctrl-C was typed in line mode since the last call.
    pub fn take_line_interrupt(&mut self) -> bool {
        core::mem::take(&mut self.interrupted)
    }

    /// What the line discipline echoed since the last call.
    pub fn take_echo(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.echo)
    }

    /// In line mode, the next line (`LineDiscipline::read`); in raw mode as
    /// many bytes as there are, up to `buf`'s length. `None` if nothing
    /// waits.
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.line_mode {
            return self.line.read(buf);
        }
        if buf.is_empty() {
            return Some(0);
        }
        if self.bytes.is_empty() {
            return None;
        }
        let n = buf.len().min(self.bytes.len());
        for (b, x) in buf.iter_mut().zip(self.bytes.drain(..n)) {
            *b = x;
        }
        Some(n)
    }
````

Replace:

````rust
    pub fn push(&mut self, bytes: &[u8]) {
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
````

with:

````rust
    pub fn push(&mut self, bytes: &[u8]) {
        if self.line_mode {
            if bytes.contains(&INTERRUPT) {
                self.sequence.clear();
            }
            for key in keys(bytes) {
                self.interrupted |= self.line.input(key, &mut self.echo);
            }
            return;
        }
        let bytes = match bytes.iter().rposition(|&b| b == INTERRUPT) {
````

Replace:

````rust

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
````

with:

````rust

    /// Whether nothing waits to be read: no raw input, and no line.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty() && !self.line.has_line()
    }
````

Replace:

````rust
            None => false,
        }
    }
}

/// The escape sequence an editing key sends, as a Linux terminal does.
````

with:

````rust
            None => false,
        }
    }
}

/// `bytes` cut into keys: an escape sequence (`ESC` and one byte, or
/// `ESC [` up to its final byte) is one, every other byte is one.
fn keys(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = bytes;
    core::iter::from_fn(move || {
        let n = match rest {
            [] => return None,
            [0x1B, b'[', tail @ ..] => {
                2 + tail
                    .iter()
                    .position(|b| (0x40..=0x7E).contains(b))
                    .map_or(tail.len(), |i| i + 1)
            }
            [0x1B, _, ..] => 2,
            _ => 1,
        };
        let (key, after) = rest.split_at(n);
        rest = after;
        Some(key)
    })
}

/// The escape sequence an editing key sends, as a Linux terminal does.
````

- [ ] **Step 14: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
````

with:

````rust

    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        loop {
            {
                let t = PROCS.lock();
                let p = t.get(t.current()).expect("a process reads");
                if p.killed.is_some() {
                    return Err(Errno::EINTR);
                }
                if p.pgid != tty::foreground() {
                    return Ok(0);
                }
            }
            tty::poll();
            if let Some(n) = tty::read(buf) {
                return Ok(n);
            }
            // Typing, a kill, or a change of foreground group or mode wakes
            // it.
            block(Blocked::Console);
        }
    }

    fn console_mode(&mut self, line: bool) -> bool {
        let was = tty::set_line_mode(line);
        PROCS.lock().wake_all(Blocked::Console);
        was
    }

    fn console_size(&self) -> (u32, u32) {
        let (columns, rows) = console::size().unwrap_or((80, 25));
        (columns as u32, rows as u32)
    }

    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno> {
        let mut t = PROCS.lock();
        if !t.iter().any(|p| p.pgid == pgid) {
            return Err(Errno::ESRCH);
        }
        tty::set_foreground(pgid);
        t.wake_all(Blocked::Console);
        Ok(())
    }

    fn shell_output(&mut self, file: &Arc<File>, n: u32, bytes: &[u8]) -> Result<(), Errno> {
````

- [ ] **Step 15: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    fn console_write(&mut self, bytes: &[u8]);
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

with:

````rust
    fn console_write(&mut self, bytes: &[u8]);
    /// Reads the console into `buf` (spec §6.4, §6.5), waiting for input:
    /// 0 at once for a process outside the foreground group, and at end of
    /// input; `EINTR` if the program was killed while it waited.
    fn console_read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
    /// Line mode (`true`) or raw mode; the previous one.
    fn console_mode(&mut self, line: bool) -> bool;
    /// The console's columns and rows.
    fn console_size(&self) -> (u32, u32);
    /// Makes `pgid` the foreground group; `ESRCH` if no process is in it.
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

Replace:

````rust
        Some(Call::Getcwd) => files::getcwd(caller, args[0], args[1]),
        Some(Call::Time) => time(caller, args[0]),
````

with:

````rust
        Some(Call::Getcwd) => files::getcwd(caller, args[0], args[1]),
        Some(Call::ConsoleMode) => console_mode(caller, args[0]),
        Some(Call::ConsoleSize) => {
            let (columns, rows) = caller.console_size();
            Ok(relay_abi::console::size_result(columns, rows))
        }
        Some(Call::ConsoleForeground) => {
            let pgid = u32::try_from(args[0]).ok().filter(|&g| g != 0);
            pgid.ok_or(Errno::ESRCH)
                .and_then(|g| caller.console_foreground(g))
                .map(|()| 0)
        }
        Some(Call::Time) => time(caller, args[0]),
````

Replace:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
}

````

with:

````rust
        File::Vfs(open) => caller.with_vfs(|v| open.write(v, bytes)),
    }
}

/// `console_mode(mode)` (spec §7.3): the previous mode.
fn console_mode(caller: &mut impl Caller, mode: u64) -> Result<u64, Errno> {
    use relay_abi::console::{MODE_LINE, MODE_RAW};
    let line = match u32::try_from(mode) {
        Ok(MODE_RAW) => false,
        Ok(MODE_LINE) => true,
        _ => return Err(Errno::EINVAL),
    };
    let was = caller.console_mode(line);
    Ok(u64::from(if was { MODE_LINE } else { MODE_RAW }))
}

````

- [ ] **Step 16: Change `kernel/src/syscall/files.rs`**

In `kernel/src/syscall/files.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::file as open_file;
use crate::mm::paging::PAGE;
````

with:

````rust
use crate::file as open_file;
use crate::line::LINE_MAX;
use crate::mm::paging::PAGE;
````

Replace:

````rust
        File::Vfs(_) | File::ShellOutput(_) => return Err(Errno::EBADF),
        // The console's reads come with its line discipline.
        File::Console => return Err(Errno::ENOSYS),
    };
````

with:

````rust
        File::Vfs(_) | File::ShellOutput(_) => return Err(Errno::EBADF),
        File::Console => return console_read(caller, addr, len),
    };
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

/// A console read: at most one line (or what was typed, in raw mode), into
/// a buffer checked writable before anything is taken from the console.
fn console_read(caller: &mut impl Caller, addr: u64, len: u64) -> Result<u64, Errno> {
    let len = len.min(LINE_MAX as u64);
    let slice = UserSlice::new(addr, len)?;
    caller.writable(&slice)?;
    let mut buf = [0u8; LINE_MAX];
    let n = caller.console_read(&mut buf[..len as usize])?;
    caller.write(&slice, 0, &buf[..n])?;
    Ok(n as u64)
}
````

- [ ] **Step 17: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`). Plan 3b adds reading in line mode.

use crate::input::InputQueue;
use crate::{serial, timer, usb};
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
````

with:

````rust
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`), and what is typed goes through the
//! line discipline, which echoes it as it comes (`input`).

use crate::input::InputQueue;
use crate::{console, serial, timer, usb};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// The foreground process group; process 1's at first.
static FOREGROUND: AtomicU32 = AtomicU32::new(1);

/// The console's mode: raw (`false`) or line (`true`); the previous one.
/// What the switch hands the line discipline is echoed.
pub fn set_line_mode(line: bool) -> bool {
    let (was, echo) = {
        let mut input = INPUT.lock();
        (input.set_line_mode(line), input.take_echo())
    };
    output(&echo);
    was
}

pub fn is_line_mode() -> bool {
    INPUT.lock().is_line_mode()
}
````

Replace:

````rust

/// The foreground group a Ctrl-C typed in line mode is for, if one is
/// waiting: it and what was typed before it are dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    if !LINE_MODE.load(Ordering::Relaxed) {
        return None;
    }
    take_interrupt().then(|| FOREGROUND.load(Ordering::Relaxed))
}
````

with:

````rust

/// The group that reads the console (spec §6.4).
pub fn foreground() -> u32 {
    FOREGROUND.load(Ordering::Relaxed)
}

/// The foreground group a Ctrl-C typed in line mode is for, if one was
/// typed: what was typed before it is dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    INPUT
        .lock()
        .take_line_interrupt()
        .then(|| FOREGROUND.load(Ordering::Relaxed))
}

/// What a program reads (spec §6.5): in line mode the next line, in raw
/// mode what was typed, up to `buf`'s length; `None` if nothing waits.
pub fn read(buf: &mut [u8]) -> Option<usize> {
    INPUT.lock().read(buf)
}

/// The line discipline's echo, to the screen.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
    }
}
````

Replace:

````rust
    input.expire(now);
}
````

with:

````rust
    input.expire(now);
    let echo = input.take_echo();
    drop(input);
    output(&echo);
}
````

Replace:

````rust

/// Whether anything typed waits to be read.
pub fn has_input() -> bool {
````

with:

````rust

/// Whether anything typed waits to be read: raw input, or a line.
pub fn has_input() -> bool {
````

- [ ] **Step 18: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    Send(String),
    /// Text typed on the emulated USB keyboard.
    Key(String),
    ScreenshotNonblank,
````

with:

````rust
    Send(String),
    /// Text typed on the emulated USB keyboard, then Enter.
    Key(String),
    /// Text typed on the emulated USB keyboard, and nothing after it.
    Type(String),
    ScreenshotNonblank,
````

Replace:

````rust
                Step::Key(rest.to_string())
            }
````

with:

````rust
                Step::Key(rest.to_string())
            }
            "type" => {
                keys::typed(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Type(rest.to_string())
            }
````

Replace:

````rust
        }
        Step::Key(text) => {
            let presses = keys::presses(text)?;
            for press in &presses {
````

with:

````rust
        }
        Step::Key(_) | Step::Type(_) => {
            let presses = match step {
                Step::Key(text) => keys::presses(text)?,
                Step::Type(text) => keys::typed(text)?,
                _ => unreachable!(),
            };
            for press in &presses {
````

- [ ] **Step 19: Change `xtask/src/keys.rs`**

In `xtask/src/keys.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! `{tab}`, `{esc}`, `{ret}`, `{caps_lock}` and `{ctrl-<letter>}`. Enter
//! follows the text, as with `send`.

````

with:

````rust
//! `{tab}`, `{esc}`, `{ret}`, `{caps_lock}` and `{ctrl-<letter>}`. Enter
//! follows the text, as with `send`, except for the `type` step
//! (`typed`), which presses only what it says.

````

Replace:

````rust
pub fn presses(text: &str) -> Result<Vec<Vec<&'static str>>> {
    let mut out = Vec::new();
````

with:

````rust
pub fn presses(text: &str) -> Result<Vec<Vec<&'static str>>> {
    let mut out = typed(text)?;
    out.push(vec!["ret"]);
    Ok(out)
}

/// The presses that type `text`, and nothing after it.
pub fn typed(text: &str) -> Result<Vec<Vec<&'static str>>> {
    let mut out = Vec::new();
````

Replace:

````rust
    }
    out.push(vec!["ret"]);
    Ok(out)
````

with:

````rust
    }
    Ok(out)
````

- [ ] **Step 20: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 333 tests.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: PASS: 31 tests.

- [ ] **Step 21: Run the `console`, `system`, `keyboard`, `ctrlc`, `shell` scenarios**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 22: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 23: Commit**

````bash
git add crates docs kernel tests userland xtask
git commit -m "kernel, relay-rt: programs read the console, in line mode through the line discipline; console_mode, console_size, console_foreground; t-read"
````


### Task 18: What is typed ahead during a script is echoed once

Found by the prototype's review: keys typed while a command ran were echoed by the line discipline, handed back to raw input when the command ended, and at the next command's switch to line mode went through the discipline again, which echoed them again; in a script they appeared once more at the start of every command until the shell read them, on the screen and in every tee. The input queue now counts how many of its raw bytes, from the first, the discipline echoed already (`echoed`); they go through it again without being echoed, and whatever takes raw bytes (the shell's reads, a program's raw read, a Ctrl-C) takes them off the count. The `console` scenario runs a script of three commands with `abc` typed during the first: `abc` shows as it is typed and at the prompt, not before each command. Mutation checks: the handed-back bytes echoed again fails a test and `console`; `pop`, a raw read, `take_interrupt` or a raw Ctrl-C not counting echoed bytes each fails a test (the read's and `take_interrupt`'s survived until the test's cases stopped masking each other).

**Files:**
- Modify: `kernel/src/input.rs`
- Modify: `tests/e2e/console.txt`

**Interfaces:**
- Consumes: Task 17.
- Produces: `InputQueue`'s count of echoed raw bytes.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

with:

````rust
    #[test]
    fn what_was_typed_ahead_is_echoed_once() {
        // Found by the prototype's review: text typed during a script was
        // echoed again at the start of every later command, until the
        // shell read it.
        let mut q = line_mode();
        q.push(b"abc");
        assert_eq!(q.take_echo(), b"abc");
        q.set_line_mode(false);
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"", "echoed once already");
        q.push(b"d\r");
        assert_eq!(q.take_echo(), b"d\n");
        assert_eq!(read(&mut q, 10).unwrap(), b"abcd\n");
        // What the shell took is gone; what came after is echoed.
        q.push(b"xyz");
        q.take_echo();
        q.set_line_mode(false);
        assert_eq!(q.pop(), Some(b'x'));
        q.push(b"w");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"w", "yz were echoed, w was not");
        q.push(b"\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"yzw\n");
        // A raw read takes echoed bytes too.
        q.push(b"12345");
        assert_eq!(q.take_echo(), b"\n12345");
        q.set_line_mode(false);
        assert_eq!(q.read(&mut [0; 2]), Some(2));
        q.push(b"6");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"6");
        // And so does a Ctrl-C nobody took, handed back before them.
        let mut q = line_mode();
        q.push(b"ab\x03cd");
        q.take_echo();
        q.set_line_mode(false);
        assert!(q.take_interrupt());
        q.push(b"e");
        q.set_line_mode(true);
        assert_eq!(q.take_echo(), b"e");
        q.push(b"\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"cde\n");
    }

    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, replace:

````text
expect \nend of input\nroot@relay:~# $
````

with:

````text
expect \nend of input\nroot@relay:~# $
# What is typed during a script is echoed once, as it is typed, not again
# at every later command; the shell's line editor shows it at the prompt.
send echo t-spin 2 > s.sh
send echo t-args one >> s.sh
send echo t-args two >> s.sh
send sh s.sh
alive 1
type abc
expect abc\d+ iterations\n\+ t-args one\n\[1\] one\n\+ t-args two\n\[1\] two\nroot@relay:~# abc$
type {ctrl-c}
expect root@relay:~# $
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib input::`

Expected: FAIL: 1 test fails: `input::tests::what_was_typed_ahead_is_echoed_once`.

Run: `cargo xtask test --e2e-only --scenario console`

Expected: FAIL: scenario `console` stops at line 44, timed out waiting for `abc\d+ iterations\n\+ t-args one\n\[1\] one\n\+ t-args two\n\[1\] two\nroot@relay:~# abc$`.

- [ ] **Step 4: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
    bytes: VecDeque<u8>,
    /// An escape sequence arriving over serial, until it is complete.
````

with:

````rust
    bytes: VecDeque<u8>,
    /// How many of `bytes`, from the first, the line discipline echoed
    /// already (typed in line mode and handed back): they go through it
    /// again without being echoed again.
    echoed: usize,
    /// An escape sequence arriving over serial, until it is complete.
````

Replace:

````rust
            bytes: VecDeque::new(),
            sequence: Vec::new(),
````

with:

````rust
            bytes: VecDeque::new(),
            echoed: 0,
            sequence: Vec::new(),
````

Replace:

````rust
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            self.push(&ahead);
        } else if !line && was {
````

with:

````rust
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            let quiet = core::mem::take(&mut self.echoed).min(ahead.len());
            let mut shown = Vec::new();
            for key in keys(&ahead[..quiet]) {
                self.interrupted |= self.line.input(key, &mut shown);
            }
            self.push(&ahead[quiet..]);
        } else if !line && was {
````

Replace:

````rust
            self.push(&typed);
        }
````

with:

````rust
            self.push(&typed);
            self.echoed = self.bytes.len();
        }
````

Replace:

````rust
        }
        Some(n)
````

with:

````rust
        }
        self.echoed = self.echoed.saturating_sub(n);
        Some(n)
````

Replace:

````rust
                self.bytes.clear();
                self.sequence.clear();
````

with:

````rust
                self.bytes.clear();
                self.echoed = 0;
                self.sequence.clear();
````

Replace:

````rust
    pub fn pop(&mut self) -> Option<u8> {
        self.bytes.pop_front()
    }
````

with:

````rust
    pub fn pop(&mut self) -> Option<u8> {
        let b = self.bytes.pop_front()?;
        self.echoed = self.echoed.saturating_sub(1);
        Some(b)
    }
````

Replace:

````rust
                self.bytes.drain(..=i);
                true
````

with:

````rust
                self.bytes.drain(..=i);
                self.echoed = self.echoed.saturating_sub(i + 1);
                true
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib input::`

Expected: PASS: 20 tests.

- [ ] **Step 6: Run the `console` scenario**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel tests
git commit -m "kernel: what is typed ahead during a script is echoed once, not again at every command"
````


### Task 19: The LF of a CR LF the shell read is not an empty line; the e2e step `send-crlf`

Found by the prototype's review, a regression from plan 3a on a serial terminal that sends CR LF for Enter: the in-kernel shell's editor read up to the CR and started the command, and the LF, still raw (or arriving just after), went to the command's line discipline as an Enter of its own, so the command read an empty line first (`t-read` printed `[1] \n`). The input queue now remembers whether the last raw byte taken was a CR, and the switch to line mode tells the discipline (`LineDiscipline::after_cr`), which takes a LF right after it as the same Enter, as its own CR LF rule does. The e2e runner gains `send-crlf TEXT`, which sends CR LF after the text, and the `console` scenario runs `t-read` with it: `end of input` at the first Ctrl-D, with no empty line before. Mutation checks: the switch not handing the CR over, `pop` or a raw read not keeping it each fail a test (the raw read's survived until a case read raw bytes ending in CR); the first fails `console` too.

**Files:**
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/line.rs`
- Modify: `tests/e2e/console.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Tasks 17, 18.
- Produces: `LineDiscipline::after_cr(&mut self, bool)`; the e2e step `send-crlf`.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

with:

````rust
    #[test]
    fn the_lf_of_a_cr_lf_the_shell_read_is_not_a_line() {
        // Found by the prototype's review: over a terminal that sends CR
        // LF, the shell's editor read up to the CR, and the LF became an
        // empty line for the command, whether it was there at the switch
        // to line mode or came after it.
        for lf_later in [false, true] {
            let mut q = InputQueue::new();
            serial(&mut q, b"t-read\r");
            if !lf_later {
                serial(&mut q, b"\n");
            }
            let mut line = Vec::new();
            while let Some(b) = q.pop() {
                line.push(b);
                if b == b'\r' {
                    break;
                }
            }
            assert_eq!(line, b"t-read\r");
            q.set_line_mode(true);
            if lf_later {
                serial(&mut q, b"\n");
            }
            assert!(q.is_empty(), "no line (LF {lf_later})");
            assert_eq!(q.take_echo(), b"");
            serial(&mut q, b"hello\r\n");
            assert_eq!(read(&mut q, 10).unwrap(), b"hello\n");
            assert_eq!(read(&mut q, 10), None);
        }
        // The same after a program's raw read that ended in a CR.
        let mut q = InputQueue::new();
        serial(&mut q, b"y\r");
        assert_eq!(read(&mut q, 10).unwrap(), b"y\r");
        q.set_line_mode(true);
        serial(&mut q, b"\n");
        assert!(q.is_empty());
        // A LF alone after the shell's line is still an Enter.
        let mut q = InputQueue::new();
        q.push(b"ls\n\n");
        assert_eq!(
            (q.pop(), q.pop(), q.pop()),
            (Some(b'l'), Some(b's'), Some(b'\n'))
        );
        q.set_line_mode(true);
        assert_eq!(read(&mut q, 10).unwrap(), b"\n");
    }

    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, replace:

````text
expect \nend of input\nroot@relay:~# $
# What is typed during a script is echoed once, as it is typed, not again
````

with:

````text
expect \nend of input\nroot@relay:~# $
# A terminal that sends CR LF for Enter: the LF after the shell's line
# is not an empty line for the command.
send-crlf t-read
alive 1
type {ctrl-d}
expect t-read\nend of input\nroot@relay:~# $
# What is typed during a script is echoed once, as it is typed, not again
````

- [ ] **Step 3: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
        assert!(parse_scenario("x", "key {bogus}").is_err());
        let s = parse_scenario("x", "type {ctrl-d}").unwrap();
````

with:

````rust
        assert!(parse_scenario("x", "key {bogus}").is_err());
        let s = parse_scenario("x", "send-crlf ls").unwrap();
        assert_eq!(s.steps, vec![(1, Step::SendCrLf("ls".into()))]);
        let s = parse_scenario("x", "type {ctrl-d}").unwrap();
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib input::`

Expected: FAIL: 1 test fails: `input::tests::the_lf_of_a_cr_lf_the_shell_read_is_not_a_line`.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `SendCrLf` found for enum `e2e::Step` in the current scope ``.

- [ ] **Step 5: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    bytes: VecDeque<u8>,
    /// How many of `bytes`, from the first, the line discipline echoed
````

with:

````rust
    bytes: VecDeque<u8>,
    /// The last raw byte taken was a CR: a LF after it is the same Enter.
    taken_cr: bool,
    /// How many of `bytes`, from the first, the line discipline echoed
````

Replace:

````rust
            bytes: VecDeque::new(),
            echoed: 0,
````

with:

````rust
            bytes: VecDeque::new(),
            taken_cr: false,
            echoed: 0,
````

Replace:

````rust
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            let quiet = core::mem::take(&mut self.echoed).min(ahead.len());
````

with:

````rust
            let ahead: Vec<u8> = self.bytes.drain(..).collect();
            self.line.after_cr(core::mem::take(&mut self.taken_cr));
            let quiet = core::mem::take(&mut self.echoed).min(ahead.len());
````

Replace:

````rust
        }
        self.echoed = self.echoed.saturating_sub(n);
````

with:

````rust
        }
        self.taken_cr = buf[n - 1] == b'\r';
        self.echoed = self.echoed.saturating_sub(n);
````

Replace:

````rust
        let b = self.bytes.pop_front()?;
        self.echoed = self.echoed.saturating_sub(1);
````

with:

````rust
        let b = self.bytes.pop_front()?;
        self.taken_cr = b == b'\r';
        self.echoed = self.echoed.saturating_sub(1);
````

- [ ] **Step 6: Change `kernel/src/line.rs`**

In `kernel/src/line.rs`, replace:

````rust
        self.ready.push_back(Some(core::mem::take(&mut self.line)));
    }
````

with:

````rust
        self.ready.push_back(Some(core::mem::take(&mut self.line)));
    }

    /// The key before the next one was a CR (read raw by the shell, whose
    /// line it ended): a LF right after it is the same Enter.
    pub fn after_cr(&mut self, cr: bool) {
        self.after_cr = cr;
    }
````

- [ ] **Step 7: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    Send(String),
    /// Text typed on the emulated USB keyboard, then Enter.
````

with:

````rust
    Send(String),
    /// Text sent over the serial console with CR LF after it, as some
    /// terminals send Enter.
    SendCrLf(String),
    /// Text typed on the emulated USB keyboard, then Enter.
````

Replace:

````rust
            "send" => Step::Send(rest.to_string()),
            "key" => {
````

with:

````rust
            "send" => Step::Send(rest.to_string()),
            "send-crlf" => Step::SendCrLf(rest.to_string()),
            "key" => {
````

Replace:

````rust
            r.stdin.write_all(b"\r")?;
            r.stdin.flush()?;
````

with:

````rust
            r.stdin.write_all(b"\r")?;
            r.stdin.flush()?;
        }
        Step::SendCrLf(text) => {
            r.stdin.write_all(text.as_bytes())?;
            r.stdin.write_all(b"\r\n")?;
            r.stdin.flush()?;
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib input::`

Expected: PASS: 21 tests.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: PASS: 31 tests.

- [ ] **Step 9: Run the `console` scenario**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel tests xtask
git commit -m "kernel: the LF of a CR LF the shell read is not an empty line for the command; e2e step send-crlf"
````


### Task 20: An Escape typed ahead is a key of its own

Found by the prototype's review: at the switch to line mode, what was typed ahead is cut into keys, and `ESC` with the byte after it counted as one (a serial terminal's Alt-x), so the Escape key typed ahead swallowed the next key (`Esc l s Enter` read as `s`). Nothing here sends Alt: the keyboard sends Escape alone, and an Escape over COM1 goes in alone after its timeout, but in raw input it sits next to the key after it. Now only `ESC [` starts a sequence of several bytes; `ESC` and any other byte are the Escape key and that key, so a serial Alt-x is Escape, then x (decision 9). Mutation check: `ESC x` cut as one key fails three tests.

**Files:**
- Modify: `kernel/src/input.rs`

**Interfaces:**
- Consumes: Task 17.
- Produces: `input::keys`' rule for `ESC`.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            read(&mut q, 10).unwrap(),
            b"abc\n",
            "Delete and Alt-x do nothing"
        );
        assert_eq!(read(&mut q, 10).unwrap(), b"de\n");
        assert_eq!(q.take_echo(), b"abc\nde\n");
    }
````

with:

````rust
            read(&mut q, 10).unwrap(),
            b"abxc\n",
            "Delete does nothing; Alt-x is Escape, then x"
        );
        assert_eq!(read(&mut q, 10).unwrap(), b"de\n");
        assert_eq!(q.take_echo(), b"abxc\nde\n");
    }
````

Replace:

````rust
    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

with:

````rust
    #[test]
    fn an_escape_typed_ahead_keeps_the_key_after_it() {
        // Found by the prototype's review: the Escape key and the next key,
        // side by side in raw input, went into line mode as one key.
        let mut q = InputQueue::new();
        q.push_key(&press(Key::Escape, false));
        for c in *b"ls" {
            q.push_key(&press(Key::Char(c), false));
        }
        q.push_key(&press(Key::Escape, false));
        q.push_key(&press(Key::Enter, false));
        q.set_line_mode(true);
        assert_eq!(read(&mut q, 10).unwrap(), b"ls\n");
        assert_eq!(q.take_echo(), b"ls\n");
    }

    #[test]
    fn a_ctrl_c_nobody_took_is_raw_input_again() {
````

Replace:

````rust
            got,
            [&b"a"[..], b"\x1b[1;5C", b"b", b"\x1bx", b"c", b"\x1b["]
        );
````

with:

````rust
            got,
            [&b"a"[..], b"\x1b[1;5C", b"b", b"\x1b", b"x", b"c", b"\x1b["]
        );
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib input::`

Expected: FAIL: 3 tests fail, among them `input::tests::an_escape_typed_ahead_keeps_the_key_after_it`, `input::tests::an_escape_sequence_is_one_key_in_line_mode`.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// `bytes` cut into keys: an escape sequence (`ESC` and one byte, or
/// `ESC [` up to its final byte) is one, every other byte is one.
fn keys(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
````

with:

````rust

/// `bytes` cut into keys: an escape sequence (`ESC [` up to its final byte)
/// is one, every other byte is one. `ESC` and another byte are the Escape
/// key and that key: nothing here sends Alt (a serial terminal's `ESC x`
/// is Escape, then x), and raw input typed ahead keeps an Escape next to
/// the key after it.
fn keys(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
````

Replace:

````rust
            }
            [0x1B, _, ..] => 2,
            _ => 1,
````

with:

````rust
            }
            _ => 1,
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib input::`

Expected: PASS: 22 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: an Escape typed ahead is a key of its own in line mode, and keeps the key after it"
````


### Task 21: The in-kernel shell takes the console back at its prompt

Found by the prototype's review: at its prompt the in-kernel shell reads raw bytes itself, and any process may call `console_mode`; a program that outlived its command and put the console in line mode then kept every key in the line discipline, where the shell never looked, and nothing brought the prompt back. The shell now takes the console back (raw mode, its own group, what the discipline holds handed back as raw input) before each byte it reads at its prompt (decision 14). `t-read leave` leaves a child behind that puts the console in line mode while the shell waits; the `console` scenario then types `echo ok`, which the red run never runs. Mutation check: `read_byte` without taking the console back fails `console`.

**Files:**
- Modify: `kernel/src/session.rs`
- Modify: `tests/e2e/console.txt`
- Modify: `userland/tests/src/bin/t-read.rs`

**Interfaces:**
- Consumes: Tasks 17, 18.
- Produces: `KernelConsole::read_byte` taking the console back; `t-read leave`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, replace:

````text
expect t-read\nend of input\nroot@relay:~# $
# What is typed during a script is echoed once, as it is typed, not again
````

with:

````text
expect t-read\nend of input\nroot@relay:~# $
# A program that leaves the console in line mode, while the shell waits
# at its prompt, does not keep the shell from reading.
send t-read leave
alive 2
send echo ok
expect \nok\nroot@relay:~# $
# What is typed during a script is echoed once, as it is typed, not again
````

- [ ] **Step 2: Change the test program `userland/tests/src/bin/t-read.rs`**

In `userland/tests/src/bin/t-read.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//!   `console_mode` and `console_foreground` refuse.
#![no_std]
````

with:

````rust
//!   `console_mode` and `console_foreground` refuse.
//! - `t-read leave`: leaves a child behind that puts the console in line
//!   mode half a second later, while the shell waits at its prompt.
#![no_std]
````

Replace:

````rust
        Some(b"size") => size(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|size]\n");
            return 2;
````

with:

````rust
        Some(b"size") => size(),
        Some(b"leave") => sys::spawn(
            b"/bin/t-read",
            b"t-read\0leave-child\0",
            b"",
            &[],
            NEW_GROUP,
        )
        .map(|_| ()),
        Some(b"leave-child") => {
            sys::sleep(500);
            sys::console_mode(MODE_LINE).map(|_| ())
        }
        _ => {
            let _ = sys::write_all(2, b"usage: t-read [raw|apart|size|leave]\n");
            return 2;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: FAIL: scenario `console` stops at line 47, timed out waiting for `\nok\nroot@relay:~# $`.

- [ ] **Step 4: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust
impl Console for KernelConsole {
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            tty::poll();
````

with:

````rust
impl Console for KernelConsole {
    /// The shell reads at its prompt: the console is its own, in raw mode,
    /// whatever another process left it in (a program may call
    /// `console_mode` after its parent returned to the prompt).
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            proc::take_console();
            tty::poll();
````

- [ ] **Step 5: Run the `console`, `shell`, `keyboard`, `ctrlc` scenarios**

Run: `cargo xtask test --e2e-only --scenario console`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario ctrlc`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: the in-kernel shell takes the console back at its prompt, whatever mode a program left it in"
````


### Task 22: Console tees; `t-tee`

Spec §6.5 (decision 11), with Task 5's stack in `tty.rs`: `tty::write` is what programs and the in-kernel shell write to the console (the screen, and a copy for every tee, written once 4 KiB wait); the line discipline's echo is copied too but, coming from ticks, only waits; `sync` (the call's, and the in-kernel shell's after each command, through `KernelVfs::sync`) and a pop write what waits. `console_tee_push` takes an fd of a VFS file open for writing (`EBADF` otherwise, `EINVAL` for the console and the shell's outputs), `console_tee_pop` the owner's newest; a process's tees end with it (`proc::end`), written at the next flush; a failed write is logged and its pop says why; a tee of a freed inode is gone like an open file (`tty::follow_tees`). A tee's writes go through `mounts::with_nodes`, which needs no working directory; the tee stack's lock comes before the mount table's and is never held across a switch. `t-tee basic`, `end` and `gone` (the `tees` scenario) and `full` (`diskfull`): what a tee gets, its 4 KiB pieces, the refusals, a tee left by a process that ended, and a removed file. `system` lists eight programs. Mutation checks: console writes not copied fail `tees`; tees of a removed file not marked gone survived until `t-tee gone` closed its fd after the push (the fd's marking had marked the shared open file), then the tee wrote 18 bytes into the new file that got the inode; a process's tees not ended survived until `wc` counted the file twice.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/mounts.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Modify: `kernel/src/tty.rs`
- Modify: `tests/e2e/diskfull.txt`
- Modify: `tests/e2e/system.txt`
- Create: `tests/e2e/tees.txt`
- Create: `userland/tests/src/bin/t-tee.rs`

**Interfaces:**
- Consumes: Tasks 5, 7, 13, 11.
- Produces: `tty::{write(&[u8]), sync_tees(), push_tee(u32, Arc<File>) -> Result<(), Errno>, pop_tee(u32) -> Result<(), Errno>, end_tees(u32), follow_tees(&[Change]), tees_locked() -> bool}`; `mounts::with_nodes`; `Caller::{tee_push(&mut self, Arc<File>) -> Result<(), Errno>, tee_pop(&mut self) -> Result<(), Errno>, sync(&mut self) -> Result<(), Errno>}`; `relay_rt::sys::{console_tee_push, console_tee_pop}`; `userland/tests/src/bin/t-tee.rs`; the `tees` scenario.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust
    relay_abi::console::size_of_result(call(Call::ConsoleSize, &[]).unwrap_or(0))
}
````

with:

````rust
    relay_abi::console::size_of_result(call(Call::ConsoleSize, &[]).unwrap_or(0))
}

/// Pushes `fd`, a file open for writing, as a console tee (spec §6.5):
/// it gets a copy of everything written to the console.
pub fn console_tee_push(fd: u32) -> Result<(), u16> {
    call(Call::ConsoleTeePush, &[u64::from(fd)]).map(|_| ())
}

/// Pops the newest tee this program pushed; the error of a write to it
/// that failed.
pub fn console_tee_pop() -> Result<(), u16> {
    call(Call::ConsoleTeePop, &[]).map(|_| ())
}
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn every_other_call_is_enosys() {
````

with:

````rust
    #[test]
    fn tees_are_pushed_by_fd_and_popped() {
        let mut f = fake();
        assert_eq!(call(&mut f, Call::ConsoleTeePush, [1, 0, 0]), Ok(0));
        assert_eq!(f.tees.len(), 1);
        assert!(
            Arc::ptr_eq(&f.tees[0], f.fds.get(1).unwrap()),
            "the fd's file"
        );
        assert_eq!(
            call(&mut f, Call::ConsoleTeePush, [9, 0, 0]),
            Err(errno::EBADF)
        );
        assert_eq!(call(&mut f, Call::ConsoleTeePop, [0, 0, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::ConsoleTeePop, [0, 0, 0]),
            Err(errno::EINVAL)
        );
        assert_eq!(call(&mut f, Call::Sync, [0, 0, 0]), Ok(0));
        assert_eq!(f.syncs, 1);
    }

    #[test]
    fn every_other_call_is_enosys() {
````

Replace:

````rust
            Call::ConsoleForeground,
            Call::Time,
````

with:

````rust
            Call::ConsoleForeground,
            Call::ConsoleTeePush,
            Call::ConsoleTeePop,
            Call::Time,
````

- [ ] **Step 3: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub foreground: u32,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
    pub foreground: u32,
    /// The tees pushed, and the syncs.
    pub tees: Vec<Arc<File>>,
    pub syncs: u32,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
        (120, 33)
    }
````

with:

````rust
        (120, 33)
    }
    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno> {
        self.tees.push(file);
        Ok(())
    }
    fn tee_pop(&mut self) -> Result<(), Errno> {
        self.tees.pop().map(|_| ()).ok_or(Errno::EINVAL)
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.syncs += 1;
        self.vfs.sync()
    }
````

Replace:

````rust
        foreground: 1,
        written: Vec::new(),
````

with:

````rust
        foreground: 1,
        tees: Vec::new(),
        syncs: 0,
        written: Vec::new(),
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/diskfull.txt`**

In `tests/e2e/diskfull.txt`, replace:

````text
expect writing a file of its own: ENOSPC\nt-files: write error: No space left on device\n
send rm t
````

with:

````text
expect writing a file of its own: ENOSPC\nt-files: write error: No space left on device\n
# A console tee that cannot be written: its pop says why.
send t-tee full
expect \npop: ENOSPC\nroot@relay:~# $
send rm t
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-tee\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-tee\n
````

- [ ] **Step 6: Add the scenario `tests/e2e/tees.txt`**

Create `tests/e2e/tees.txt`:

````text
# Console tees (user-space gate §6.5; milestone 2, plan 3b): a tee gets what
# programs and the shell write to the console, written every 4 KiB and at
# every sync, until it is popped; at most 4; a process's tees end with it;
# a tee whose file is removed is gone.
timeout 60
expect root@relay:~# $
send t-tee basic
expect \npush: ok\nto the screen and the tee\n\[1\] from a child\nnothing written yet: true\nsync: ok\nwritten at the sync: true\n
expect written every 4 KiB: true\npop: ok\nnot in the tee\npop again: EINVAL\npush the screen: EINVAL\n
expect push a file open for reading: EBADF\npush an fd not open: EBADF\npushed 4, then EBUSY\n
expect \| to the screen and the tee\n\| \[1\] from a child\n\| nothing written yet: true\n\| sync: ok\n
expect \| written at the sync: true\n(\| \.{99}\n){42}\| written every 4 KiB: true\n
# The four tees pushed on the same file each got the line printed while
# they were pushed.
expect (\| pushed 4, then EBUSY\n){4}\d+ bytes\nroot@relay:~# $
send t-tee end
expect \nbefore the end\nroot@relay:~# $
send wc t-tee.end
expect \n\s*1\s+3\s+15 t-tee.end\nroot@relay:~# $
# Nothing more reaches it once its process has ended.
send wc t-tee.end
expect \n\s*1\s+3\s+15 t-tee.end\nroot@relay:~# $
# The sync's write to it fails, which the kernel logs (serial shows it).
send t-tee gone
expect \nafter the removal\npid \d+: a console tee failed: No such file or directory; it is removed\npop: ENOENT\nthe new file holds 0 bytes\nroot@relay:~# $
send dmesg
expect \npid \d+: a console tee failed: No such file or directory; it is removed\n
send rm t-tee.log t-tee.end
````

- [ ] **Step 7: Add the test program `userland/tests/src/bin/t-tee.rs`**

Create `userland/tests/src/bin/t-tee.rs`:

````rust
//! `t-tee KIND`: console tees (spec §6.5). Each answer is printed as
//! `<what>: <value or error name>`; what a tee got is shown afterwards,
//! line by line, as `| <line>`.
//!
//! - `t-tee basic`: a tee gets what this program and its child write to
//!   the console, in 4 KiB pieces and at every `sync`, until it is
//!   popped; the pushes and pops that are refused.
//! - `t-tee end`: pushes a tee on `t-tee.end` and ends without popping
//!   it: it gets what came before the end, written at the next sync.
//! - `t-tee gone`: a tee whose file is removed is gone: the sync's write
//!   to it fails, its pop is `ENOENT`, and a new file that may have got its
//!   inode gets nothing.
//! - `t-tee full`, on a full disk: a tee that cannot be written is removed,
//!   and its pop says `ENOSPC`.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_abi::FdMap;
use relay_abi::errno;
use relay_abi::file::{OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"basic") => basic(),
        Some(b"end") => end(),
        Some(b"gone") => gone(),
        Some(b"full") => full(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|full\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-tee: {}", name(e));
            1
        }
    }
}

fn name(e: u16) -> &'static str {
    errno::name(e).unwrap_or("?")
}

fn show_ok(what: &str, r: Result<(), u16>) {
    let _ = writeln!(Fd(1), "{what}: {}", r.map_or_else(name, |()| "ok"));
}

/// A new file for a tee, open for writing.
fn create(path: &[u8]) -> Result<u32, u16> {
    sys::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)
}

fn size(path: &[u8]) -> u64 {
    sys::stat(path, 0).map_or(0, |s| s.size)
}

/// Prints the lines of `path` as `| <line>`, and how many bytes it holds.
fn show_file(path: &[u8]) -> Result<(), u16> {
    let fd = sys::open(path, OPEN_READ)?;
    let mut buf = [0u8; 256];
    let mut line = [0u8; 256];
    let (mut len, mut total) = (0, 0);
    loop {
        let n = sys::read(fd, &mut buf)?;
        if n == 0 {
            break;
        }
        total += n;
        for &b in &buf[..n] {
            if b == b'\n' {
                let _ = sys::write_all(1, b"| ");
                let _ = sys::write_all(1, &line[..len]);
                let _ = sys::write_all(1, b"\n");
                len = 0;
            } else if len < line.len() {
                line[len] = b;
                len += 1;
            }
        }
    }
    sys::close(fd)?;
    let _ = writeln!(Fd(1), "{total} bytes");
    Ok(())
}

fn basic() -> Result<(), u16> {
    let log = create(b"t-tee.log")?;
    show_ok("push", sys::console_tee_push(log));
    let _ = writeln!(Fd(1), "to the screen and the tee");
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
    let pid = sys::spawn(b"/bin/t-args", b"t-args\0from a child\0", b"", &fds, 0)?;
    sys::wait(i64::from(pid), false)?;
    let _ = writeln!(Fd(1), "nothing written yet: {}", size(b"t-tee.log") == 0);
    show_ok("sync", sys::sync());
    let _ = writeln!(Fd(1), "written at the sync: {}", size(b"t-tee.log") > 0);
    // 4 KiB of dots is written as it comes.
    let before = size(b"t-tee.log");
    let dots = [b'.'; 99];
    for _ in 0..42 {
        let _ = sys::write_all(1, &dots);
        let _ = sys::write_all(1, b"\n");
    }
    let _ = writeln!(
        Fd(1),
        "written every 4 KiB: {}",
        size(b"t-tee.log") >= before + 4096
    );
    show_ok("pop", sys::console_tee_pop());
    let _ = writeln!(Fd(1), "not in the tee");
    show_ok("pop again", sys::console_tee_pop());
    show_ok("push the screen", sys::console_tee_push(1));
    let r = sys::open(b"t-tee.log", OPEN_READ)?;
    show_ok("push a file open for reading", sys::console_tee_push(r));
    show_ok("push an fd not open", sys::console_tee_push(30));
    let mut pushed = 0;
    let full = loop {
        match sys::console_tee_push(log) {
            Ok(()) => pushed += 1,
            Err(e) => break e,
        }
    };
    let _ = writeln!(Fd(1), "pushed {pushed}, then {}", name(full));
    for _ in 0..pushed {
        sys::console_tee_pop()?;
    }
    sys::close(log)?;
    sys::close(r)?;
    show_file(b"t-tee.log")
}

fn end() -> Result<(), u16> {
    let log = create(b"t-tee.end")?;
    sys::console_tee_push(log)?;
    let _ = writeln!(Fd(1), "before the end");
    Ok(())
}

fn gone() -> Result<(), u16> {
    let log = create(b"t-tee.gone")?;
    sys::console_tee_push(log)?;
    // Only the tee has the file now.
    sys::close(log)?;
    sys::unlink(b"t-tee.gone")?;
    // A new file may get its inode: the tee must not write into it.
    let new = create(b"t-tee.new")?;
    let _ = writeln!(Fd(1), "after the removal");
    // The sync's write fails (and is logged); the pop says so.
    sys::sync()?;
    show_ok("pop", sys::console_tee_pop());
    let _ = writeln!(Fd(1), "the new file holds {} bytes", size(b"t-tee.new"));
    sys::close(new)?;
    sys::unlink(b"t-tee.new")
}

fn full() -> Result<(), u16> {
    let log = create(b"t-tee.full")?;
    sys::console_tee_push(log)?;
    for _ in 0..100 {
        let _ = writeln!(Fd(1), "a line the disk has no room for");
    }
    show_ok("pop", sys::console_tee_pop());
    sys::close(log)?;
    sys::unlink(b"t-tee.full")
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` method `tee_push` is not a member of trait `Caller` ``; `` method `tee_pop` is not a member of trait `Caller` ``.

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: FAIL: scenario `tees` stops at line 8, timed out waiting for `\npush: ok\nto the screen and the tee\n\[1\] from a child\nnothing written yet: true\nsync: ok\nwritten at the sync: true\n`.

- [ ] **Step 9: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 7 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 8 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 10: Change `kernel/src/mounts.rs`**

In `kernel/src/mounts.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    r
}
````

with:

````rust
    r
}

/// Runs `f` on the mount table as it is, for work on nodes, which needs no
/// current directory and removes nothing (a tee's writes).
pub fn with_nodes<R>(f: impl FnOnce(&mut MountTable) -> R) -> R {
    let mut guard = MOUNTS.lock();
    f(guard.0.as_mut().expect("mounts::init has not run"))
}
````

Replace:

````rust
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.with(|t| t.sync())
````

with:

````rust
    }
    /// The tees, then every filesystem (spec §7.3).
    fn sync(&mut self) -> Result<(), Errno> {
        crate::tty::sync_tees();
        self.with(|t| t.sync())
````

- [ ] **Step 11: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
fn no_lock_held() -> bool {
    !PROCS.is_locked() && !mounts::is_locked() && !tty::is_locked() && !mm::is_locked()
}
````

with:

````rust
fn no_lock_held() -> bool {
    !PROCS.is_locked()
        && !mounts::is_locked()
        && !tty::is_locked()
        && !tty::tees_locked()
        && !mm::is_locked()
}
````

Replace:

````rust
pub fn follow_changes(changes: &[vfs::Change]) {
    let mut t = PROCS.lock();
    for p in t.iter_mut() {
        for c in changes {
            if let Some(cwd) = p.res.cwd.as_mut() {
                cwd.follow(c);
            }
            p.res.fds.follow(c);
        }
    }
}
````

with:

````rust
pub fn follow_changes(changes: &[vfs::Change]) {
    {
        let mut t = PROCS.lock();
        for p in t.iter_mut() {
            for c in changes {
                if let Some(cwd) = p.res.cwd.as_mut() {
                    cwd.follow(c);
                }
                p.res.fds.follow(c);
            }
        }
    }
    tty::follow_tees(changes);
}
````

Replace:

````rust

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
````

with:

````rust

/// The running process ends with `status` (spec §5.4): its memory, fds and
/// tees are given back at once, it stays a zombie until its parent waits
/// for it, and the CPU goes to the next process for good.
fn end(status: WaitStatus) -> ! {
    let (me, space, fds) = {
        let mut t = PROCS.lock();
        let me = t.current();
        let p = t.get_mut(me).expect("a running process ends");
        let (space, fds) = (p.res.space.take(), core::mem::take(&mut p.res.fds));
        t.end(me, status);
        (me, space, fds)
    };
    // Its tees get nothing more; they are written at the next sync.
    tty::end_tees(me);
    drop(fds);
````

Replace:

````rust
    fn console_write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }
````

with:

````rust
    fn console_write(&mut self, bytes: &[u8]) {
        tty::write(bytes);
    }
````

Replace:

````rust
            block(Blocked::Console);
        }
    }

````

with:

````rust
            block(Blocked::Console);
        }
    }

    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno> {
        tty::push_tee(self.pid(), file)
    }

    fn tee_pop(&mut self) -> Result<(), Errno> {
        tty::pop_tee(self.pid())
    }

    fn sync(&mut self) -> Result<(), Errno> {
        KernelVfs.sync()
    }

````

Replace:

````rust
            // goes to the screen.
            console::write_output(bytes);
            return Ok(());
````

with:

````rust
            // goes to the screen.
            tty::write(bytes);
            return Ok(());
````

- [ ] **Step 12: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust
    fn write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }
````

with:

````rust
    fn write(&mut self, bytes: &[u8]) {
        tty::write(bytes);
    }
````

- [ ] **Step 13: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

with:

````rust
    fn console_foreground(&mut self, pgid: u32) -> Result<(), Errno>;
    /// Pushes `file` as a console tee of the program (spec §6.5).
    fn tee_push(&mut self, file: Arc<File>) -> Result<(), Errno>;
    /// Pops the newest tee the program pushed.
    fn tee_pop(&mut self) -> Result<(), Errno>;
    /// Writes what waits for the tees, then syncs every filesystem.
    fn sync(&mut self) -> Result<(), Errno>;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

Replace:

````rust
        Some(Call::Statfs) => files::statfs(caller, args[0], args[1], args[2]),
        Some(Call::Sync) => caller.with_vfs(|v| v.sync()).map(|()| 0),
        Some(Call::Chdir) => files::on_path(caller, args[0], args[1], |v, p| v.chdir(p)),
````

with:

````rust
        Some(Call::Statfs) => files::statfs(caller, args[0], args[1], args[2]),
        Some(Call::Sync) => caller.sync().map(|()| 0),
        Some(Call::Chdir) => files::on_path(caller, args[0], args[1], |v, p| v.chdir(p)),
````

Replace:

````rust
        }
        Some(Call::ConsoleForeground) => {
````

with:

````rust
        }
        Some(Call::ConsoleTeePush) => file(caller, args[0])
            .and_then(|f| caller.tee_push(f))
            .map(|()| 0),
        Some(Call::ConsoleTeePop) => caller.tee_pop().map(|()| 0),
        Some(Call::ConsoleForeground) => {
````

- [ ] **Step 14: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use crate::input::InputQueue;
use crate::{console, serial, timer, usb};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

````

with:

````rust

use crate::fd::File;
use crate::input::InputQueue;
use crate::tee::TeeStack;
use crate::{console, klogln, mounts, serial, timer, usb};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;
use vfs::{Change, Errno};

````

Replace:

````rust

/// The line discipline's echo, to the screen.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
    }
}
````

with:

````rust

/// The line discipline's echo, to the screen and the tees. It may come
/// from a tick, where no file can be written: the tees keep it for the
/// next write.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
        TEES.lock().add(echo);
    }
}

/// The console's tees (spec §6.5).
static TEES: Mutex<TeeStack<Arc<File>>> = Mutex::new(TeeStack::new());

/// Writes what a process or the in-kernel shell gives the console: the
/// screen, and a copy for every tee, written once 4 KiB of it waits.
pub fn write(bytes: &[u8]) {
    console::write_output(bytes);
    let mut tees = TEES.lock();
    if tees.is_copying() {
        tees.add(bytes);
        flush(&mut tees, false);
    }
}

/// Writes what waits for the tees: all of it with `all` (a `sync`), or
/// what is due.
fn flush(tees: &mut TeeStack<Arc<File>>, all: bool) {
    for (owner, e) in tees.flush(all, &mut write_tee) {
        klogln!("pid {owner}: a console tee failed: {e}; it is removed");
    }
}

/// A tee's copies to its file.
fn write_tee(file: &Arc<File>, bytes: &[u8]) -> Result<(), Errno> {
    match &**file {
        File::Vfs(open) => mounts::with_nodes(|t| open.write_all(t, bytes)),
        File::Console | File::ShellOutput(_) => Err(Errno::EINVAL),
    }
}

/// Writes everything that waits for the tees (at every `sync`).
pub fn sync_tees() {
    flush(&mut TEES.lock(), true);
}

/// Pushes `file` as a tee of process `owner`: a file of the VFS it has
/// open for writing (`EBADF` otherwise, `EINVAL` for the console or the
/// shell's outputs); `EBUSY` if 4 are pushed.
pub fn push_tee(owner: u32, file: Arc<File>) -> Result<(), Errno> {
    match &*file {
        File::Vfs(open) if open.is_writable() => {}
        File::Vfs(_) => return Err(Errno::EBADF),
        File::Console | File::ShellOutput(_) => return Err(Errno::EINVAL),
    }
    TEES.lock().push(owner, file)
}

/// Pops `owner`'s newest tee after writing what waits for it; the error of
/// a write that failed for it, now or before.
pub fn pop_tee(owner: u32) -> Result<(), Errno> {
    TEES.lock().pop(owner, &mut write_tee)
}

/// Process `owner` ended: its tees end too (written at the next flush).
pub fn end_tees(owner: u32) {
    TEES.lock().end(owner);
}

/// A removal elsewhere freed an inode: a tee of it is gone.
pub fn follow_tees(changes: &[Change]) {
    let tees = TEES.lock();
    for file in tees.files() {
        for c in changes {
            if let File::Vfs(open) = &**file
                && c.removed() == Some(open.node())
            {
                open.mark_gone();
            }
        }
    }
}

/// Whether the tee stack is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn tees_locked() -> bool {
    TEES.is_locked()
}
````

- [ ] **Step 15: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 337 tests.

- [ ] **Step 16: Run the `tees`, `diskfull`, `system`, `shell` scenarios**

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 17: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 18: Commit**

````bash
git add crates docs kernel tests userland
git commit -m "kernel, relay-rt: console tees: copies of what is written to the console, written every 4 KiB and at every sync; t-tee"
````


### Task 23: A tee's echo waits no more than 16 KiB, and a console read writes the tees that are due

Found by the prototype's review: the line discipline's echo only waited in the tees (it comes from ticks, where no file can be written), and only a write, a sync or a pop wrote it, so the tee of a program that reads the console and writes nothing (plan 4's script running `cat > f`, a paste over COM1) grew until the kernel's heap ran out; typing and erasing on a full line echoes without end, too. Now a console read writes the tees that are due (`tty::flush_due_tees`), as a write does, and a tee keeps at most `ECHO_MAX` (16 KiB) of echo while it waits, dropping the rest, as a terminal drops what nobody reads; what programs write is never dropped (decision 11). `t-tee typed` reads 4 KiB of typed lines under a tee and writes nothing: the tee has them before the pop (the `tees` scenario). Mutation checks: a read that writes no tee fails `tees`; echo without its limit fails a test.

**Files:**
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/tee.rs`
- Modify: `kernel/src/tty.rs`
- Modify: `tests/e2e/tees.txt`
- Modify: `userland/tests/src/bin/t-tee.rs`

**Interfaces:**
- Consumes: Tasks 22, 5.
- Produces: `tee::ECHO_MAX`, `TeeStack::add_echo(&mut self, &[u8])`, `tty::flush_due_tees()`; `t-tee typed`.

- [ ] **Step 1: Add the failing tests to `kernel/src/tee.rs`**

In `kernel/src/tee.rs`, replace:

````rust
    #[test]
    fn a_process_s_tees_end_with_it_and_are_written_at_the_next_flush() {
````

with:

````rust
    #[test]
    fn echo_alone_waits_no_more_than_its_limit() {
        // Found by the prototype's review: the line discipline's echo only
        // waits (it comes from ticks, where no file can be written), so a
        // tee of a program that reads and never writes grew without bound.
        let mut s = TeeStack::new();
        s.push(10, 1).unwrap();
        for _ in 0..3 * ECHO_MAX {
            s.add_echo(b"x\x08 \x08");
        }
        assert_eq!(s.tees[0].pending.len(), ECHO_MAX);
        // Once written, it takes echo again.
        let mut files = Files::default();
        s.flush(false, &mut files.writer());
        assert_eq!(files.of(1).len(), ECHO_MAX);
        s.add_echo(b"y");
        assert_eq!(s.tees[0].pending, b"y");
        // What programs write is never dropped.
        s.add(&[b'z'; 2 * ECHO_MAX]);
        assert_eq!(s.tees[0].pending.len(), 2 * ECHO_MAX + 1);
    }

    #[test]
    fn a_process_s_tees_end_with_it_and_are_written_at_the_next_flush() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/tees.txt`**

Replace the whole of `tests/e2e/tees.txt` with:

````text
# Console tees (user-space gate §6.5; milestone 2, plan 3b): a tee gets what
# programs and the shell write to the console, written every 4 KiB and at
# every sync, until it is popped; at most 4; a process's tees end with it;
# a tee whose file is removed is gone.
timeout 60
expect root@relay:~# $
send t-tee basic
expect \npush: ok\nto the screen and the tee\n\[1\] from a child\nnothing written yet: true\nsync: ok\nwritten at the sync: true\n
expect written every 4 KiB: true\npop: ok\nnot in the tee\npop again: EINVAL\npush the screen: EINVAL\n
expect push a file open for reading: EBADF\npush an fd not open: EBADF\npushed 4, then EBUSY\n
expect \| to the screen and the tee\n\| \[1\] from a child\n\| nothing written yet: true\n\| sync: ok\n
expect \| written at the sync: true\n(\| \.{99}\n){42}\| written every 4 KiB: true\n
# The four tees pushed on the same file each got the line printed while
# they were pushed.
expect (\| pushed 4, then EBUSY\n){4}\d+ bytes\nroot@relay:~# $
send t-tee end
expect \nbefore the end\nroot@relay:~# $
send wc t-tee.end
expect \n\s*1\s+3\s+15 t-tee.end\nroot@relay:~# $
# Nothing more reaches it once its process has ended.
send wc t-tee.end
expect \n\s*1\s+3\s+15 t-tee.end\nroot@relay:~# $
# The sync's write to it fails, which the kernel logs (serial shows it).
send t-tee gone
expect \nafter the removal\npid \d+: a console tee failed: No such file or directory; it is removed\npop: ENOENT\nthe new file holds 0 bytes\nroot@relay:~# $
send dmesg
expect \npid \d+: a console tee failed: No such file or directory; it is removed\n
# A program that reads the console and writes nothing: its tee gets the
# echo as it goes, 4 KiB at a time.
send t-tee typed
alive 1
send aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
send bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
send end
expect \nwritten as it was typed: true\nroot@relay:~# $
send rm t-tee.log t-tee.end
````

- [ ] **Step 3: Change the test program `userland/tests/src/bin/t-tee.rs`**

In `userland/tests/src/bin/t-tee.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//!   inode gets nothing.
//! - `t-tee full`, on a full disk: a tee that cannot be written is removed,
````

with:

````rust
//!   inode gets nothing.
//! - `t-tee typed`: reads lines from the console until `end`, writing
//!   nothing: the tee gets their echo as they are read, 4 KiB at a time.
//! - `t-tee full`, on a full disk: a tee that cannot be written is removed,
````

Replace:

````rust
        Some(b"full") => full(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|full\n");
            return 2;
````

with:

````rust
        Some(b"full") => full(),
        Some(b"typed") => typed(),
        _ => {
            let _ = sys::write_all(2, b"usage: t-tee basic|end|gone|typed|full\n");
            return 2;
````

Replace:

````rust
    sys::unlink(b"t-tee.full")
}
````

with:

````rust
    sys::unlink(b"t-tee.full")
}

fn typed() -> Result<(), u16> {
    let log = create(b"t-tee.typed")?;
    sys::console_tee_push(log)?;
    let mut buf = [0u8; 4096];
    for _ in 0..20 {
        let n = sys::read(0, &mut buf)?;
        if n == 0 || &buf[..n] == b"end\n" {
            break;
        }
    }
    // Nothing written, no sync: only the reads can have written the echo.
    let written = size(b"t-tee.typed") >= 4096;
    sys::console_tee_pop()?;
    let _ = writeln!(Fd(1), "written as it was typed: {written}");
    sys::close(log)?;
    sys::unlink(b"t-tee.typed")
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib tee::`

Expected: FAIL: compile errors such as `` cannot find value `ECHO_MAX` in this scope ``; `` no method named `add_echo` found for struct `tee::TeeStack<F>` in the current scope ``.

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: FAIL: scenario `tees` stops at line 35, timed out waiting for `\nwritten as it was typed: true\nroot@relay:~# $`.

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
            if let Some(n) = tty::read(buf) {
                return Ok(n);
````

with:

````rust
            if let Some(n) = tty::read(buf) {
                tty::flush_due_tees();
                return Ok(n);
````

- [ ] **Step 6: Change `kernel/src/tee.rs`**

In `kernel/src/tee.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const CHUNK: usize = 4096;

````

with:

````rust
pub const CHUNK: usize = 4096;
/// The echo a tee keeps while it waits: what the line discipline echoes
/// comes from ticks, where no file can be written, so it waits for the
/// next write, read or sync of a process; beyond this it is dropped, as a
/// terminal drops what nobody reads.
pub const ECHO_MAX: usize = 4 * CHUNK;

````

Replace:

````rust
            t.pending.extend_from_slice(bytes);
        }
````

with:

````rust
            t.pending.extend_from_slice(bytes);
        }
    }

    /// A copy of the line discipline's echo, for every tee, as far as
    /// `ECHO_MAX` of waiting copies allows.
    pub fn add_echo(&mut self, bytes: &[u8]) {
        for t in self.tees.iter_mut().filter(|t| t.file.is_some()) {
            let room = ECHO_MAX.saturating_sub(t.pending.len());
            t.pending.extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
````

- [ ] **Step 7: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
/// from a tick, where no file can be written: the tees keep it for the
/// next write.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
        TEES.lock().add(echo);
    }
}
````

with:

````rust
/// from a tick, where no file can be written: the tees keep it for the
/// next write or read of a process (`flush_due_tees`), up to a limit.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
        TEES.lock().add_echo(echo);
    }
}

/// Writes the tees' copies that are due; for a process that reads the
/// console, whose echo would otherwise wait for its next write.
pub fn flush_due_tees() {
    flush(&mut TEES.lock(), false);
}
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib tee::`

Expected: PASS: 6 tests.

- [ ] **Step 9: Run the `tees` scenario**

Run: `cargo xtask test --e2e-only --scenario tees`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel tests userland
git commit -m "kernel: a tee's echo waits no more than 16 KiB, and a console read writes the tees that are due"
````


### Task 24: `sys_info`'s names and kernel log, and `power` from ring 3; `t-sys`

The last of spec §7.3 for milestone 2 (decisions 12 and 13): `sys_info`'s `INFO_UNAME` (the kernel's version from its crate, the machine from `arch::MACHINE`; `EINVAL` for a buffer shorter than `Uname`) and `INFO_LOG` (the newest bytes that fit); `power(kind, flags)`, which writes the tees, shuts the filesystems down and restarts or switches off, and returns the shutdown's error, unless `POWER_FORCE`, with the machine up. Test mode's poweroff makes QEMU exit, so it moves from the in-kernel shell's `KernelSystem` to `power::test_mode`. `relay-rt` gains `sys::{uname, kernel_log, power}`; `t-sys` prints them (the `sysinfo` scenario, which switches off through `t-sys poweroff`, the e2e runner's `poweroff` step gaining a command); in `unplug`, `t-sys poweroff` says `EIO` and the machine stays up. `system` lists nine programs. Mutation checks: `power` going ahead after a failed shutdown fails `unplug`; the log's oldest bytes fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/power.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`
- Create: `tests/e2e/sysinfo.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `tests/e2e/unplug.txt`
- Create: `userland/tests/src/bin/t-sys.rs`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Tasks 1, 22.
- Produces: `Caller::{kernel_log(&self) -> Vec<u8>, power(&mut self, reboot: bool, force: bool) -> Errno}`; `arch::MACHINE`; `power::{set_test_mode(bool), test_mode() -> bool}`; `relay_rt::sys::{uname() -> Result<Uname, u16>, kernel_log(&mut [u8]) -> Result<usize, u16>, power(u32, u32) -> u16}`; `userland/tests/src/bin/t-sys.rs`; the e2e step `poweroff COMMAND`; the `sysinfo` scenario.

- [ ] **Step 1: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust

/// The wall clock and the time since the machine started.
````

with:

````rust

/// The system's names, as `uname` prints them.
pub fn uname() -> Result<relay_abi::Uname, u16> {
    let mut u = relay_abi::Uname::new(b"", b"", b"", b"");
    let len = relay_abi::Uname::SIZE as u64;
    let kind = u64::from(relay_abi::info::INFO_UNAME);
    call(Call::SysInfo, &[kind, &raw mut u as u64, len])?;
    Ok(u)
}

/// The newest bytes of the kernel log that fit in `buf` (it holds at most
/// `relay_abi::info::LOG_MAX`); how many.
pub fn kernel_log(buf: &mut [u8]) -> Result<usize, u16> {
    let kind = u64::from(relay_abi::info::INFO_LOG);
    call(
        Call::SysInfo,
        &[kind, buf.as_mut_ptr() as u64, buf.len() as u64],
    )
    .map(|n| n as usize)
}

/// Restarts the machine or switches it off (`relay_abi::power`'s kinds and
/// `POWER_FORCE`) after shutting the filesystems down; returns only with
/// the error that kept it up.
pub fn power(kind: u32, flags: u32) -> u16 {
    match call(Call::Power, &[u64::from(kind), u64::from(flags)]) {
        Err(e) => e,
        Ok(_) => relay_abi::errno::EIO,
    }
}

/// The wall clock and the time since the machine started.
````

- [ ] **Step 2: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        );
        assert_eq!(call(&mut f, Call::SysInfo, [2, W, 32]), Err(errno::EINVAL));
        assert_eq!(
````

with:

````rust
        );
        assert_eq!(call(&mut f, Call::SysInfo, [4, W, 32]), Err(errno::EINVAL));
        assert_eq!(
````

Replace:

````rust
    #[test]
    fn every_other_call_is_enosys() {
````

with:

````rust
    #[test]
    fn sys_info_names_the_system_and_gives_the_newest_of_the_log() {
        use relay_abi::info::{INFO_LOG, INFO_UNAME};
        let mut f = fake();
        let uname = u64::from(INFO_UNAME);
        assert_eq!(call(&mut f, Call::SysInfo, [uname, W, 256]), Ok(256));
        let b = get(&mut f, W, 256);
        assert_eq!(&b[..6], b"Relay\0");
        assert_eq!(&b[64..70], b"relay\0");
        assert_eq!(
            &b[128..128 + 6],
            concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()
        );
        assert_eq!(&b[192..199], b"x86_64\0");
        assert_eq!(
            call(&mut f, Call::SysInfo, [uname, W, 255]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [uname, W + PAGE - 100, 256]),
            Err(errno::EFAULT)
        );
        let log = u64::from(INFO_LOG);
        assert_eq!(
            call(&mut f, Call::SysInfo, [log, W, 1000]),
            Ok(FAKE_LOG.len() as u64)
        );
        assert_eq!(get(&mut f, W, FAKE_LOG.len()), FAKE_LOG);
        assert_eq!(call(&mut f, Call::SysInfo, [log, W, 5]), Ok(5));
        assert_eq!(
            get(&mut f, W, 5),
            FAKE_LOG[FAKE_LOG.len() - 5..],
            "the newest"
        );
        assert_eq!(call(&mut f, Call::SysInfo, [log, W, 0]), Ok(0));
        assert_eq!(
            call(&mut f, Call::SysInfo, [log, U, 10]),
            Err(errno::EFAULT)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [4, W, 1000]),
            Err(errno::EINVAL)
        );
        assert_eq!(
            call(&mut f, Call::SysInfo, [1 << 32 | 1, W, 1000]),
            Err(errno::EINVAL)
        );
    }

    #[test]
    fn power_returns_only_the_error_that_kept_the_machine_up() {
        use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
        let mut f = fake();
        let (reboot, off, force) = (
            u64::from(POWER_REBOOT),
            u64::from(POWER_POWEROFF),
            u64::from(POWER_FORCE),
        );
        assert_eq!(call(&mut f, Call::Power, [reboot, 0, 0]), Err(errno::EIO));
        assert_eq!(call(&mut f, Call::Power, [off, force, 0]), Err(errno::EIO));
        assert_eq!(f.powered, [(true, false), (false, true)]);
        for (kind, flags) in [(0, 0), (3, 0), (reboot, 2), (1 << 32 | 1, 0)] {
            assert_eq!(
                call(&mut f, Call::Power, [kind, flags, 0]),
                Err(errno::EINVAL)
            );
        }
        assert_eq!(f.powered.len(), 2, "refused before anything was shut down");
    }

    #[test]
    fn every_other_call_is_enosys() {
````

Replace:

````rust
            Call::SysInfo,
        ];
````

with:

````rust
            Call::SysInfo,
            Call::Power,
        ];
````

- [ ] **Step 3: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
pub const FULL: u64 = 7;
/// How much file data `/full` holds.
````

with:

````rust
pub const FULL: u64 = 7;
/// What the fake kernel log holds.
pub const FAKE_LOG: &[u8] = b"Relay OS 0.2.0\n[ ok ] everything\n";
/// How much file data `/full` holds.
````

Replace:

````rust
    pub syncs: u32,
    pub written: Vec<(u64, Vec<u8>)>,
````

with:

````rust
    pub syncs: u32,
    /// The `power` calls: (reboot, force).
    pub powered: Vec<(bool, bool)>,
    pub written: Vec<(u64, Vec<u8>)>,
````

Replace:

````rust
        self.tees.pop().map(|_| ()).ok_or(Errno::EINVAL)
    }
````

with:

````rust
        self.tees.pop().map(|_| ()).ok_or(Errno::EINVAL)
    }
    fn kernel_log(&self) -> Vec<u8> {
        FAKE_LOG.to_vec()
    }
    /// A machine whose filesystems cannot be shut down (an unplugged
    /// stick): the call comes back.
    fn power(&mut self, reboot: bool, force: bool) -> Errno {
        self.powered.push((reboot, force));
        Errno::EIO
    }
````

Replace:

````rust
        syncs: 0,
        written: Vec::new(),
````

with:

````rust
        syncs: 0,
        powered: Vec::new(),
        written: Vec::new(),
````

- [ ] **Step 4: Add the scenario `tests/e2e/sysinfo.txt`**

Create `tests/e2e/sysinfo.txt`:

````text
# What sys_info tells a program (user-space gate §7.3; milestone 2, plan
# 3b): the system's names and the kernel log; and power from a program,
# which shuts the filesystems down cleanly first.
timeout 60
expect root@relay:~# $
send t-sys uname
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
send uname -a
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
send t-sys log
expect \n\d+ bytes, the first line: Relay OS \d+\.\d+\.\d+\nthe newest 16 bytes: 16 of them, the log's end: true\n
poweroff t-sys poweroff
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-tee\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-sys  t-tee\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-tee\n
````

with:

````text
send ls /bin
expect \nt-args  t-fault  t-files  t-mem  t-read  t-spawn  t-spin  t-sys  t-tee\n
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/unplug.txt`**

In `tests/e2e/unplug.txt`, replace:

````text
expect \nstill here\n
send poweroff
````

with:

````text
expect \nstill here\n
# A program's power call keeps the machine up too, and says why.
send t-sys poweroff
expect \npower: EIO\n
send poweroff
````

- [ ] **Step 7: Add the test program `userland/tests/src/bin/t-sys.rs`**

Create `userland/tests/src/bin/t-sys.rs`:

````rust
//! `t-sys KIND`: what `sys_info` and `power` answer (spec §7.3).
//!
//! - `t-sys uname`: the system's names, as `uname -a` prints them.
//! - `t-sys log`: the kernel log, whole (its first line) and its newest 16
//!   bytes.
//! - `t-sys poweroff [-f]`, `t-sys reboot`: shut the filesystems down and
//!   switch off or restart; if that fails, the error, and the machine
//!   stays up (`-f` goes ahead anyway).
#![no_std]
#![no_main]

extern crate alloc;

use core::fmt::Write;
use relay_abi::Uname;
use relay_abi::errno;
use relay_abi::info::LOG_MAX;
use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    let r = match args.get(1) {
        Some(b"uname") => uname(),
        Some(b"log") => log(),
        Some(b"poweroff") => power(POWER_POWEROFF, args.get(2)),
        Some(b"reboot") => power(POWER_REBOOT, args.get(2)),
        _ => {
            let _ = sys::write_all(2, b"usage: t-sys uname|log|poweroff [-f]|reboot [-f]\n");
            return 2;
        }
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(Fd(2), "t-sys: {}", errno::name(e).unwrap_or("?"));
            1
        }
    }
}

fn uname() -> Result<(), u16> {
    let u = sys::uname()?;
    for (i, f) in [u.sysname, u.nodename, u.release, u.machine]
        .iter()
        .enumerate()
    {
        if i > 0 {
            sys::write_all(1, b" ")?;
        }
        sys::write_all(1, Uname::name(f))?;
    }
    sys::write_all(1, b"\n")
}

fn log() -> Result<(), u16> {
    // Room for the whole log.
    let mut buf = alloc::vec![0u8; LOG_MAX];
    let n = sys::kernel_log(&mut buf)?;
    let first = buf[..n].split(|&b| b == b'\n').next().unwrap_or(&[]);
    let _ = write!(Fd(1), "{n} bytes, the first line: ");
    sys::write_all(1, first)?;
    let mut tail = [0u8; 16];
    let k = sys::kernel_log(&mut tail)?;
    let _ = writeln!(
        Fd(1),
        "\nthe newest 16 bytes: {} of them, the log's end: {}",
        k,
        tail[..k] == buf[n - k..n]
    );
    Ok(())
}

fn power(kind: u32, flag: Option<&[u8]>) -> Result<(), u16> {
    let flags = if flag == Some(b"-f") { POWER_FORCE } else { 0 };
    let e = sys::power(kind, flags);
    let _ = writeln!(Fd(1), "power: {}", errno::name(e).unwrap_or("?"));
    Ok(())
}
````

- [ ] **Step 8: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario("x", "reboot\nreboot relay: restarting\npoweroff").unwrap();
        assert_eq!(
````

with:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario(
            "x",
            "reboot\nreboot relay: restarting\npoweroff\npoweroff t-sys poweroff",
        )
        .unwrap();
        assert_eq!(
````

Replace:

````rust
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff)
            ]
````

with:

````rust
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff("poweroff".into())),
                (4, Step::Poweroff("t-sys poweroff".into()))
            ]
````

- [ ] **Step 9: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib syscall`

Expected: FAIL: compile errors such as `` method `kernel_log` is not a member of trait `Caller` ``; `` method `power` is not a member of trait `Caller` ``.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: FAIL: compile errors such as `` expected function, found `e2e::Step` ``.

- [ ] **Step 10: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 8 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

with:

````markdown
     run `flash --full` again)
   - `[ ok ] system: 9 programs, ABI 1` (milestone 2: the programs of
     `/bin` from `\EFI\RELAY\system.img`; the count grows as later plans
````

- [ ] **Step 11: Change `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust
pub mod user;

````

with:

````rust
pub mod user;

/// The machine, as `uname` names it.
pub const MACHINE: &str = "x86_64";

````

- [ ] **Step 12: Change `kernel/src/power.rs`**

In `kernel/src/power.rs`, replace:

````rust
    }
    arch::halt_forever()
}

````

with:

````rust
    }
    arch::halt_forever()
}

/// `test=1`: `poweroff` makes QEMU exit (spec §7.4), whoever asks for it.
static TEST_MODE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn set_test_mode(on: bool) {
    TEST_MODE.store(on, core::sync::atomic::Ordering::Relaxed);
}

pub fn test_mode() -> bool {
    TEST_MODE.load(core::sync::atomic::Ordering::Relaxed)
}

````

- [ ] **Step 13: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

    fn console_mode(&mut self, line: bool) -> bool {
````

with:

````rust

    fn kernel_log(&self) -> Vec<u8> {
        crate::klog::KLOG.lock().to_vec()
    }

    fn power(&mut self, reboot: bool, force: bool) -> Errno {
        tty::sync_tees();
        if let Err(e) = KernelVfs.shutdown()
            && !force
        {
            return e;
        }
        if reboot {
            crate::power::reboot()
        } else {
            crate::power::poweroff(crate::power::test_mode())
        }
    }

    fn console_mode(&mut self, line: bool) -> bool {
````

- [ ] **Step 14: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, replace:

````rust
pub extern "C" fn shell(test_mode: u64) -> ! {
    let mut vfs = KernelVfs;
````

with:

````rust
pub extern "C" fn shell(test_mode: u64) -> ! {
    power::set_test_mode(test_mode != 0);
    let mut vfs = KernelVfs;
````

- [ ] **Step 15: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    fn sync(&mut self) -> Result<(), Errno>;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

with:

````rust
    fn sync(&mut self) -> Result<(), Errno>;
    /// The kernel log.
    fn kernel_log(&self) -> Vec<u8>;
    /// Syncs and shuts the filesystems down, then restarts (`reboot`) or
    /// switches the machine off; returns only the shutdown's error, unless
    /// `force` goes ahead anyway (spec §7.3).
    fn power(&mut self, reboot: bool, force: bool) -> Errno;
    /// Writes `bytes` to `file`, output `n` of the in-kernel shell (1 or
````

Replace:

````rust
        Some(Call::SysInfo) => sys_info(caller, args[0], args[1], args[2]),
        _ => Err(Errno::ENOSYS),
````

with:

````rust
        Some(Call::SysInfo) => sys_info(caller, args[0], args[1], args[2]),
        Some(Call::Power) => power(caller, args[0], args[1]),
        _ => Err(Errno::ENOSYS),
````

Replace:

````rust

/// `sys_info(kind, buffer, length)` (spec §7.3): the bytes written. Plan
/// 3a has the memory figures (`MemInfo`); the `uname` fields and the kernel
/// log come with plan 3b.
fn sys_info(caller: &mut impl Caller, kind: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    if kind != u64::from(INFO_MEMORY) {
        return Err(Errno::EINVAL);
    }
````

with:

````rust

/// `power(kind, flags)` (spec §7.3): returns only with the error that kept
/// the machine up.
fn power(caller: &mut impl Caller, kind: u64, flags: u64) -> Result<u64, Errno> {
    use relay_abi::power::{POWER_FORCE, POWER_POWEROFF, POWER_REBOOT};
    let reboot = match u32::try_from(kind) {
        Ok(POWER_REBOOT) => true,
        Ok(POWER_POWEROFF) => false,
        _ => return Err(Errno::EINVAL),
    };
    if flags & !u64::from(POWER_FORCE) != 0 {
        return Err(Errno::EINVAL);
    }
    Err(caller.power(reboot, flags & u64::from(POWER_FORCE) != 0))
}

/// `sys_info(kind, buffer, length)` (spec §7.3): the bytes written. The
/// memory figures and the names need room for their whole struct; the
/// kernel log gives its newest bytes that fit.
fn sys_info(caller: &mut impl Caller, kind: u64, addr: u64, len: u64) -> Result<u64, Errno> {
    use relay_abi::info::{INFO_LOG, INFO_UNAME};
    match u32::try_from(kind) {
        Ok(INFO_MEMORY) => {}
        Ok(INFO_UNAME) => {
            let u = relay_abi::Uname::new(
                b"Relay",
                b"relay",
                env!("CARGO_PKG_VERSION").as_bytes(),
                crate::arch::MACHINE.as_bytes(),
            );
            let bytes = u.to_bytes();
            if len < bytes.len() as u64 {
                return Err(Errno::EINVAL);
            }
            caller.write(&UserSlice::new(addr, bytes.len() as u64)?, 0, &bytes)?;
            return Ok(bytes.len() as u64);
        }
        Ok(INFO_LOG) => {
            let slice = UserSlice::new(addr, len)?;
            let log = caller.kernel_log();
            let n = log.len().min(len as usize);
            caller.write(&slice, 0, &log[log.len() - n..])?;
            return Ok(n as u64);
        }
        _ => return Err(Errno::EINVAL),
    }
````

- [ ] **Step 16: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    Reboot(Option<String>),
    /// Switch the machine off; no later step talks to it.
    Poweroff,
    /// Pull the USB stick out (QMP `device_del`), waiting until QEMU has
````

with:

````rust
    Reboot(Option<String>),
    /// Switch the machine off with a command (`poweroff` if none is
    /// given); no later step talks to it.
    Poweroff(String),
    /// Pull the USB stick out (QMP `device_del`), waiting until QEMU has
````

Replace:

````rust
            }
            "poweroff" => Step::Poweroff,
            "unplug" if rest.is_empty() => Step::Unplug,
````

with:

````rust
            }
            "poweroff" if rest.is_empty() => Step::Poweroff("poweroff".to_string()),
            "poweroff" => Step::Poweroff(rest.to_string()),
            "unplug" if rest.is_empty() => Step::Unplug,
````

Replace:

````rust
        }
        Step::Poweroff => {
            exit_with(r, "poweroff", EXIT_POWEROFF, *timeout)?;
            r.off = true;
````

with:

````rust
        }
        Step::Poweroff(command) => {
            exit_with(r, command, EXIT_POWEROFF, *timeout)?;
            r.off = true;
````

- [ ] **Step 17: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 340 tests.

Run: `cargo test -p xtask -- keys:: e2e::`

Expected: PASS: 31 tests.

- [ ] **Step 18: Run the `sysinfo`, `unplug`, `power`, `system` scenarios**

Run: `cargo xtask test --e2e-only --scenario sysinfo`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario unplug`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 19: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 20: Commit**

````bash
git add crates docs kernel tests userland xtask
git commit -m "kernel, relay-rt: sys_info's names and kernel log, and power from ring 3; t-sys"
````


### Task 25: NUC check 3 runs the file calls, memory, a tee and the console

Spec §12.4 and decision 18: `check3-a.sh` runs `t-files basic`, `dir`, `cwd` and `gone` on the stick's ext2 root, `t-mem map` and `grow 64`, `t-tee end`, `t-sys uname` and `t-read apart` (84 commands). `docs/hardware-test.md` describes them, adds a third step by hand, since a script cannot type (typing into `t-read` on the K120, with Backspace and Ctrl-D), and its table of failures gets the ones these can show. The recorded transcripts in `xtask/fixtures/checks/` get the new lines by commands (they hold escape bytes), as QEMU printed them, until plan 3b's NUC check records real ones. The README says what milestone 2 does now.

**Files:**
- Modify: `README.md`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`

**Interfaces:**
- Consumes: Tasks 11, 12, 13, 15, 16, 17, 22, 24.
- Produces: `check3-a.sh` with plan 3b's section; the README and `docs/hardware-test.md` up to date.

- [ ] **Step 1: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash

# The file operations of the fileops scenario.
````

with:

````bash

# Files, memory and the console (milestone 2, plan 3b): programs open,
# read, write and list files on the stick's ext2 root; a removal reaches
# another process's working directory and open file, whose inodes new
# files may get; memory from mem_map and a heap that grows; a console
# tee; the system's names; a program outside the console's group reads
# nothing.
t-files basic
#> create: 3
#> exclusive: EEXIST
#> no flags: EINVAL
#> missing: ENOENT
#> write: 13
#> read a write-only fd: EBADF
#> seek: 7
#> overwrite: 6
#> close: ok
#> close again: EBADF
#> read: "hello"
#> fstat: 13 bytes, regular true
#> seek to the end: 13
#> read at the end: 0
#> append: 5
#> read what was appended: "more\\n"
#> seek before the start: EINVAL
#> seek the screen: EINVAL
#> the child read: "hello, "
#> read after the child: "files"
#> fds up to 31, then EMFILE
rm t-files.tmp
t-files dir
#> mkdir: ok
#> mkdir again: EEXIST
#> entries: \./ \.\./ a b sub/ \(5 calls\)
#> read_dir at the end: 0
#> read_dir of a file: ENOTDIR
#> stat size: 3
#> truncate: ok
#> stat size: 10
#> touch: ok
#> touch a missing file: ENOENT
#> rename: ok
#> stat the old name: ENOENT
#> readlink of a file: EINVAL
#> rmdir a full directory: ENOTEMPTY
#> unlink a directory: EISDIR
#> statfs: some blocks, free ones among them true
#> sync: ok
#> rmdir: ok
#> stat it: ENOENT
t-files cwd
#> getcwd: /root
#> getcwd into 3 bytes: ERANGE
#> chdir: ok
#> getcwd: /root/t-files.c
#> stat it from above: 4
#> chdir to a file: ENOTDIR
#> chdir to nothing: ENOENT
#> my working directory: /root/t-files.c
#> my working directory: /root
#> getcwd: /root
t-files gone
#> read the removed file: ENOENT
#> the child's working directory: /root/t-files.g
#> the child creates x: ENOENT
#> the child reads its file: ENOENT
#> the child writes its file: ENOENT
#> the new file holds: "new"
#> the new directory holds x: ENOENT
t-mem map
#> at the area's start: true
#> zeroed: true
#> written: true
#> map nothing: EINVAL
#> map too much: ENOMEM
#> unmap the middle: ok
#> unmap it again: EINVAL
#> unmap half a page in: EINVAL
#> unmap the program's code: EINVAL
#> the hole is filled first: true
#> frames lost: 0
t-mem grow 64
#> \d+ blocks, 64 MiB, all there: true
t-tee end
#> before the end
wc t-tee.end
#> \s*1\s+3\s+15 t-tee.end
rm t-tee.end
t-sys uname
#> Relay relay \d+\.\d+\.\d+ x86_64
t-read apart
#> end of input

# The file operations of the fileops scenario.
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 3: Change `README.md`**

In `README.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
programs, the timer shares the CPU among them, and Ctrl-C stops the
command that runs.

````

with:

````markdown
programs, the timer shares the CPU among them, and Ctrl-C stops the
command that runs. Programs open, read and write files, map memory (their
runtime gives them a heap), read the console a line at a time or as it is
typed, and copy what the console shows into files (tees).

````

Replace:

````markdown
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel (and, later, of user programs) |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
````

with:

````markdown
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel and of user programs |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding, `WaitStatus` |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, heap, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
````

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 5 replacements, top to bottom:

Replace:

````markdown
   the alignment check flag set and a program setting its own `gs` base,
   all killed), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

with:

````markdown
   the alignment check flag set and a program setting its own `gs` base,
   all killed), plan 3b's programs (`t-files basic`, `dir`, `cwd` and
   `gone`: the file calls on the stick's ext2 root, and another process's
   removal of a program's working directory and open file; `t-mem map`
   and `grow 64`: memory from `mem_map` and a heap that grows, `frames
   lost: 0`; `t-tee end`: a console tee; `t-sys uname`; `t-read apart`:
   a program outside the console's group reads nothing), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
````

Replace:

````markdown
   if it touched a program's page.
5. Two steps by hand (milestone 2, plan 3a; a script cannot type):
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
````

with:

````markdown
   if it touched a program's page.
5. Three steps by hand (milestone 2, plans 3a and 3b; a script cannot
   type):
   - `t-spin 5`, and while it spins type `echo typed` and Enter on the
````

Replace:

````markdown
     `dmesg` ends with `pid <n> (/bin/t-spin): killed: Ctrl-C`.

   Photograph the screen after both.
6. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
````

with:

````markdown
     `dmesg` ends with `pid <n> (/bin/t-spin): killed: Ctrl-C`.
   - `t-read`, then on the K120 `hello wrold`, four Backspaces, `orld`
     and Enter: the letters show as they are typed and the Backspaces
     erase them, and `t-read` prints `[12] hello world\n`. Then Ctrl-D:
     `end of input` and the prompt (the line discipline, milestone 2,
     plan 3b).

   Photograph the screen after each.
6. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
````

Replace:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 72 of 72 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

with:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 84 of 84 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

Replace:

````markdown
| The panic screen during `t-spawn` or `t-fault flags-ac` | A context switch, or an interrupt taken in ring 3, goes wrong on this CPU: the panic's message names the check that failed (`a program's flags reached a switch`, `TSS rsp0 and the syscall stack differ`, `an interrupt with the program's gs`) | Photograph the panic screen |
| `t-fault gsbase` prints `t-fault: no fault` | CR4.FSGSBASE is set: the firmware left it, and the kernel did not clear it | Note it; a program could set a `gs` base that would pass to the next program |
````

with:

````markdown
| The panic screen during `t-spawn` or `t-fault flags-ac` | A context switch, or an interrupt taken in ring 3, goes wrong on this CPU: the panic's message names the check that failed (`a program's flags reached a switch`, `TSS rsp0 and the syscall stack differ`, `an interrupt with the program's gs`) | Photograph the panic screen |
| `t-read` shows nothing as it is typed, a Backspace does not erase, or Ctrl-D does not end it | The line discipline gets the K120's keys otherwise than QEMU's keyboard sends them | Photograph the screen; `dmesg` shows the keyboard's `usb:` lines |
| A `t-files`, `t-mem` or `t-tee` line differs (`verify-usb` names it) | A file call, `mem_map` or a tee works otherwise on the stick's ext2 or the NUC's memory | The line after `FAILED` names the command, the expectation and what it printed; `dmesg` for `ext2:` and `usb:` lines |
| `t-fault gsbase` prints `t-fault: no fault` | CR4.FSGSBASE is set: the firmware left it, and the kernel did not clear it | Note it; a program could set a `gs` base that would pass to the next program |
````

- [ ] **Step 5: Put the new lines into the recorded NUC transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
lines = open(path, newline="").read().split("\n")
for marker, new in [('relay-sh: t-fault: killed (invalid opcode, ip 0x4002ed)', ['+ t-files basic', 'create: 3', 'exclusive: EEXIST', 'no flags: EINVAL', 'missing: ENOENT', 'write: 13', 'read a write-only fd: EBADF', 'seek: 7', 'overwrite: 6', 'close: ok', 'close again: EBADF', 'read: "hello"', 'fstat: 13 bytes, regular true', 'seek to the end: 13', 'read at the end: 0', 'append: 5', 'read what was appended: "more\\n"', 'seek before the start: EINVAL', 'seek the screen: EINVAL', 'the child read: "hello, "', 'read after the child: "files"', 'fds up to 31, then EMFILE', '+ rm t-files.tmp', '+ t-files dir', 'mkdir: ok', 'mkdir again: EEXIST', 'entries: ./ ../ a b sub/ (5 calls)', 'read_dir at the end: 0', 'read_dir of a file: ENOTDIR', 'stat size: 3', 'truncate: ok', 'stat size: 10', 'touch: ok', 'touch a missing file: ENOENT', 'rename: ok', 'stat the old name: ENOENT', 'readlink of a file: EINVAL', 'rmdir a full directory: ENOTEMPTY', 'unlink a directory: EISDIR', 'statfs: some blocks, free ones among them true', 'sync: ok', 'rmdir: ok', 'stat it: ENOENT', '+ t-files cwd', 'getcwd: /root', 'getcwd into 3 bytes: ERANGE', 'chdir: ok', 'getcwd: /root/t-files.c', 'stat it from above: 4', 'chdir to a file: ENOTDIR', 'chdir to nothing: ENOENT', 'my working directory: /root/t-files.c', 'my working directory: /root', 'getcwd: /root', '+ t-files gone', 'read the removed file: ENOENT', "the child's working directory: /root/t-files.g", 'the child creates x: ENOENT', 'the child reads its file: ENOENT', 'the child writes its file: ENOENT', 'the new file holds: "new"', 'the new directory holds x: ENOENT', '+ t-mem map', "at the area's start: true", 'zeroed: true', 'written: true', 'map nothing: EINVAL', 'map too much: ENOMEM', 'unmap the middle: ok', 'unmap it again: EINVAL', 'unmap half a page in: EINVAL', "unmap the program's code: EINVAL", 'the hole is filled first: true', 'frames lost: 0', '+ t-mem grow 64', '1928 blocks, 64 MiB, all there: true', '+ t-tee end', 'before the end', '+ wc t-tee.end', ' 1  3 15 t-tee.end', '+ rm t-tee.end', '+ t-sys uname', 'Relay relay 0.2.0 x86_64', '+ t-read apart', 'end of input'])]:
    at = [i for i, l in enumerate(lines) if marker in l]
    assert len(at) == 1, (marker, at)
    lines[at[0] + 1:at[0] + 1] = new
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 6: Put the new lines into the recorded QEMU transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.qemu.log'
lines = open(path, newline="").read().split("\n")
for marker, new in [('relay-sh: t-fault: killed (invalid opcode, ip 0x400131)', ['+ t-files basic', 'create: 3', 'exclusive: EEXIST', 'no flags: EINVAL', 'missing: ENOENT', 'write: 13', 'read a write-only fd: EBADF', 'seek: 7', 'overwrite: 6', 'close: ok', 'close again: EBADF', 'read: "hello"', 'fstat: 13 bytes, regular true', 'seek to the end: 13', 'read at the end: 0', 'append: 5', 'read what was appended: "more\\n"', 'seek before the start: EINVAL', 'seek the screen: EINVAL', 'the child read: "hello, "', 'read after the child: "files"', 'fds up to 31, then EMFILE', '+ rm t-files.tmp', '+ t-files dir', 'mkdir: ok', 'mkdir again: EEXIST', 'entries: ./ ../ a b sub/ (5 calls)', 'read_dir at the end: 0', 'read_dir of a file: ENOTDIR', 'stat size: 3', 'truncate: ok', 'stat size: 10', 'touch: ok', 'touch a missing file: ENOENT', 'rename: ok', 'stat the old name: ENOENT', 'readlink of a file: EINVAL', 'rmdir a full directory: ENOTEMPTY', 'unlink a directory: EISDIR', 'statfs: some blocks, free ones among them true', 'sync: ok', 'rmdir: ok', 'stat it: ENOENT', '+ t-files cwd', 'getcwd: /root', 'getcwd into 3 bytes: ERANGE', 'chdir: ok', 'getcwd: /root/t-files.c', 'stat it from above: 4', 'chdir to a file: ENOTDIR', 'chdir to nothing: ENOENT', 'my working directory: /root/t-files.c', 'my working directory: /root', 'getcwd: /root', '+ t-files gone', 'read the removed file: ENOENT', "the child's working directory: /root/t-files.g", 'the child creates x: ENOENT', 'the child reads its file: ENOENT', 'the child writes its file: ENOENT', 'the new file holds: "new"', 'the new directory holds x: ENOENT', '+ t-mem map', "at the area's start: true", 'zeroed: true', 'written: true', 'map nothing: EINVAL', 'map too much: ENOMEM', 'unmap the middle: ok', 'unmap it again: EINVAL', 'unmap half a page in: EINVAL', "unmap the program's code: EINVAL", 'the hole is filled first: true', 'frames lost: 0', '+ t-mem grow 64', '1928 blocks, 64 MiB, all there: true', '+ t-tee end', 'before the end', '+ wc t-tee.end', ' 1  3 15 t-tee.end', '+ rm t-tee.end', '+ t-sys uname', 'Relay relay 0.2.0 x86_64', '+ t-read apart', 'end of input'])]:
    at = [i for i, l in enumerate(lines) if marker in l]
    assert len(at) == 1, (marker, at)
    lines[at[0] + 1:at[0] + 1] = new
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
git commit -m "Check scripts: NUC check 3 runs the file calls, mem_map, the heap, a tee, sys_info and a read outside the console's group; a step by hand types into t-read"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 35 scenario(s) passed`.

````bash
git push -u origin m2p3b/console
gh pr create --draft --base main --head m2p3b/console --title "Milestone 2, plan 3b: The console, tees and the NUC" --body-file - <<'EOF'
## What

Milestone 2, plan 3b, tasks 17–25: programs read the console, the foreground group only, in line mode through the line discipline, which echoes as keys are typed; `console_mode`, `console_size` and `console_foreground`; tees get what programs and the shell write to the console, written every 4 KiB and at every `sync`; `sys_info`'s names and kernel log, and `power` (with `POWER_FORCE`), which keeps the machine up when the filesystems cannot be shut down; `t-read`, `t-tee`, `t-sys` (the `console`, `tees` and `sysinfo` scenarios); the e2e steps `type` and `poweroff COMMAND`; `check3-a.sh` runs plan 3b's programs, with a third step by hand.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 3 of `docs/hardware-test.md`, run by the user with `cargo xtask flash --full` from this worktree; the real transcripts and the results-log row go into a commit of this pull request
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p3b/console --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to run NUC check 3 (`docs/hardware-test.md`) with `cargo xtask flash --full` from this worktree, and to report its results; the pull request stays a draft until they do. After a pass, put the real transcripts copied off the stick (`xtask/fixtures/checks/check3-a.nuc.log` and `check3-b.nuc.log`, checked with `cargo xtask verify-usb`) and the results-log row of `docs/hardware-test.md` into a commit of this same pull request, re-run `cargo xtask ci`, push, and mark it ready (`gh pr ready m2p3b/console`); then ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p3b-console
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
