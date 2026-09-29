# Milestone 2 · Plan 1: Toolchain and archive — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** User programs can be built and shipped: `relay-abi` and `relay-rt` exist, the first program `t-args` is built for Relay OS and checked against the kernel's rules, xtask packs the programs into `system.img` on the ESP, the loader hands it to the kernel, and the kernel mounts it read-only at `/bin`, with the startup line `[ ok ] system: 1 program, ABI 1`. Nothing runs in ring 3 yet (plan 2). It ends with `cargo xtask ci` green and NUC check 3, by script, listing `/bin` on a stick written by `cargo xtask flash --full`.

**Architecture:** Pure logic in host-tested crates, as in milestone 1: the ABI (`crates/relay-abi`), the archive format and its read-only filesystem (`crates/sysimg`), the heap allocator and CRC-32 (moved out of the kernel into `crates/heap` and `crates/crc32`), and the mount table's mounts on a missing name (`crates/vfs`). The runtime `crates/relay-rt` keeps its architecture-specific parts (`_start`, `syscall`, the panic handler, the note's static) to `target_os = "none"`. xtask builds the user packages with the `user` profile, checks each binary with binutils' `readelf` and writes `system.img` next to `kernel.elf`; the loader copies it into `LOADER_DATA` pages and passes it in `BootInfo` version 2; the kernel checks and mounts it before the shell starts.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; binutils' `readelf` and Linux's headers (`linux-libc-dev`) for tests; QEMU 8.2, mtools (`mdel`), e2fsprogs 1.47.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1–§4, §5.2, §7.1–§7.4, §8.1, §8.4, §8.5, §11.2, §12, §13 step 1, §14, §16 item 1)
**Roadmap:** `docs/superpowers/plans/2026-09-29-milestone-2-roadmap.md` — this is plan 1 of 5.

## In brief

- **Size.** 15 tasks in four code pull requests, plus this plan as PR 1. The kernel changes are the `system` startup step and a changed `run_shell` signature; the loader reads one more file. Everything else is host-tested code, xtask and QEMU scenarios.
- **The toolchain question is settled.** `t-args` builds on stable Rust for `x86_64-unknown-none`, linked at `0x40_0000` by `crates/relay-rt/user.ld`, about 20 KiB with LTO, with its ABI note in a `PT_NOTE`. It uses the target's default code model, which the prebuilt `core` requires for LTO and which reaches everything below 2 GiB (decision 1); a program past 2 GiB would fail to link, never silently.
- **The prototype's review.** A fresh reviewer read the whole prototype and found 2 important and 5 minor real defects; one important (the `mount_fail` scenario, broken once `/bin` mounted) was fixed in its task before this plan was generated, the other important and two minors are tasks of their own (9, 10, 14), and the text findings are fixed in Task 15:

| Finding (review) | Decision |
|---|---|
| Important: `check_program` let through programs plan 2's kernel will refuse; a `user.ld` without its `PT_NOTE` passed, because the note was read from sections | Fixed, Task 9 |
| Important: `mount_fail` failed once `/bin` was mounted on the fallback root | Fixed in Task 13 itself |
| Minor: an empty `system.img` is reported as missing | Fixed, Task 14 |
| Minor: a missing `readelf` is blamed on the program | Fixed, Task 10 |
| Minor: `flash --kernel`'s comment and message, and the NUC transcript test's comment, are out of date | Fixed, Task 15 |
| Minor: spec §4.4 says a 0.2.0 root has no `/bin`, but the images have had one since 0.2.0 | The spec's §4.4 says where a mount on a name happens (PR 1); the mount table's comment, Task 15 |
| Minor: the progress squares' table does not know the `system.img` read | Fixed, Task 15 |
| Declined: `readelf`'s labels in another language | `readelf` runs with `LC_ALL=C` (Task 9) |
| Declined: `ls -l` prints a year for a file time in the future | Ruled out: the images are built on the machine whose clock the NUC's RTC and QEMU read, before they boot |
| Declined: `cp` out of `/bin` gives `-rw-r--r--`, where GNU keeps `0755` less the umask | Ruled out: milestone 1's `cp` (spec M1 §7.3), unchanged here |

## Where this plan fits

Plan 1 implements spec §13 step 1 of the user-space gate. It builds on milestone 1 (0.2.0): the loader's file reading and `BootInfo`, the frame allocator that never hands out `Bootloader` memory, the linear map, the mount table, the in-kernel shell, the e2e runner's `esp-write` and the check scripts with their recorded transcripts. It leaves plan 2 a runtime whose `_start` has never run, a kernel that can read any program in `/bin` through its `MountTable`, and xtask checks that refuse what plan 2's kernel checks must refuse (roadmap, "What plan 1 leaves for plan 2").

## Working conventions

- Plan 1 lands as **five pull requests** (table below). This plan, with the gate's spec and the milestone's roadmap, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-29-m2-plan-1-toolchain-and-archive.md`, to every task, and all tasks share one workspace directory.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.

  Two more come as commands: "Delete `p`" (`git rm`, for a file that moved) and a `python3` command that inserts lines into a recorded transcript (they hold escape bytes, which a markdown block cannot carry).
- Each task first adds its failing tests (unit tests in each file's test module; e2e scenarios under `tests/e2e/`; check-script expectations under `rootfs/root/checks/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Task 1 has no red run: it moves code under its existing tests. The mutation checks the prototype ran are named in each task's intro where a guard could pass vacuously.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `68529f7` with PR 1's spec and roadmap applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m2/spec` | — | The gate's spec (with §16 item 1), the milestone 2 roadmap, this plan | `lint`, `unit`, `e2e` |
| 2 | `m2p1/crates` | 1–2 | `crates/heap` (moved), `crates/relay-abi` | `lint`, `unit`, `e2e` |
| 3 | `m2p1/sysimg` | 3–5 | `crates/crc32` (moved), `crates/sysimg` with `SysImgFs`, mounts on a missing name | `lint`, `unit`, `e2e` |
| 4 | `m2p1/userland` | 6–10 | `crates/relay-rt`, `t-args` and the `user` profile, xtask building, checking and packing `system.img` | `lint`, `unit`, `e2e` |
| 5 | `m2p1/system` | 11–15 | `BootInfo` version 2 and the loader, the e2e steps, the kernel's `/bin`, NUC check 3 with `ls -l /bin`; NUC check 3 | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 1 adds no third-party crate. New host crates are `#![cfg_attr(not(test), no_std)]` and go in both `members` and `default-members`; the user packages (`userland/*`) go in `members` only and in `USER_PACKAGES`.
- `relay-abi` holds no architecture detail (gate §3.1): fixed-width integers only; register assignments appear only in `relay-rt`'s `arch` module.
- Everything in `system.img` is untrusted (M1 §10): nothing in it may make the kernel panic, index out of bounds, overflow (the kernel builds with overflow checks) or read outside the archive; a bad archive is a `[FAIL] system: <reason>` line and the boot continues.
- The loader opens firmware protocols only with `GET_PROTOCOL` and never closes them (M1 §15 item 7).
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 33 rows and no serial port.
- Shell messages follow GNU coreutils.
- Missing tools fail tests, never skip them (`readelf`, Linux's error-number headers, `mdel`).
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 1 in PR 1:

1. **Code model** (§8.1, §8.4). User programs use the target's default code model (`kernel`): the prebuilt `core` for `x86_64-unknown-none` is compiled for it, LTO needs one model throughout, and its sign-extended 32-bit addresses reach everything a program linked at `0x40_0000` holds below 2 GiB. A program past 2 GiB fails to link ("relocation truncated"), never silently. They need no `RUSTFLAGS` of their own; xtask builds them in `target/user`, so a build from inside `cargo test` never waits for the outer build's lock.
2. **The `user` profile** (§8.4) is `relay` (overflow checks and debug assertions on) plus LTO, stripped of debug info: `t-args` is about 20 KiB. The user packages are listed in `xtask`'s `USER_PACKAGES` and are not default members; `cargo xtask lint` clippies each for `x86_64-unknown-none`.
3. **Checks at build time** (§5.2, §10). xtask checks every program with binutils' `readelf` (`LC_ALL=C`), independently of our own code, before it packs `system.img`: an x86_64 `EXEC` with only `PT_LOAD`, `PT_NOTE` and `PT_GNU_STACK` program headers; loadable segments inside `0x40_0000`–`0x1000_0000_0000`, not overlapping (by page), with offsets and addresses congruent modulo 4 KiB, no more file than memory, never writable and executable; the entry point in an executable segment; and a `PT_NOTE` segment (not merely a section) holding the `Relay` note of type 1 with this ABI's version. A missing `readelf` fails the build naming binutils. The kernel's own checks come with plan 2.
4. **Moved crates** (§3). The heap allocator is `crates/heap` (the kernel keeps `KernelHeap`, the locked `#[global_allocator]`), and CRC-32 is `crates/crc32`, shared by the kernel's GPT code and `sysimg`.
5. **The first program** is `t-args` (§8.5), which plan 2 runs first.
6. **Plan 1 keeps the in-kernel shell** (§11.2). A missing, empty, damaged or wrong-ABI archive gives `[FAIL] system: <reason>` and the shell runs on with an empty `/bin`; the error screen comes with plan 4, when the shell leaves the kernel. `mount_fail`'s empty root now lists `bin`.
7. **The e2e runner** (§12.3) gains the before-boot steps `esp-delete <path>` and `system-abi <n>` (the same programs under ABI `n`).
8. **Check scripts** (§12.4). `check3-a.sh` expects the `system` line and runs `ls -l /bin`, which must list `t-args`; the recorded transcripts of it in `xtask/fixtures/checks/` get those lines by hand until plan 1's NUC check records real ones.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A `system.img` on the stick that is missing, stale or damaged:** `flash --kernel` from another worktree (another ABI), a file cut short or corrupted on the FAT, a zero-length file, an old stick whose ESP never had one. Expected: `[FAIL] system: <the reason>` naming what is wrong, the shell starting anyway with an empty `/bin`, no panic or hang in the loader or the kernel, and the loader's pages never handed out. Tests: `a_bad_archive_is_not_mounted`, `the_other_messages`, `the_archive_must_lie_in_loader_memory` (Task 13), `the_system_image_is_there_when_it_has_an_address` (Task 14), the `system_missing` and `system_abi` scenarios (Task 13), the `system_empty` scenario (Task 14).
2. **An archive whose fields lie:** a header length, entry count, name length, mode, offset or data length that is wrong, overflows or points outside the file, names out of order or repeated, a CRC that does not match. Expected: `Archive::parse` refuses it with the field's error and never panics or reads outside the bytes; whatever it accepts reads without failing. Tests: `a_damaged_header_is_refused`, `a_damaged_entry_is_refused`, `a_name_length_past_the_end_is_refused`, `random_damage_never_panics` (Task 3).
3. **Using `/bin` like any other directory:** `ls -l /bin`, `cd /bin` and `..` from it, `cp` out of it, `touch`, `rm`, `mv`, `rmdir`, `mkdir` or a redirection into it, a root without a `/bin` or a read-only root. Expected: `bin` listed once in `/`, GNU messages (`Read-only file system`, `Device or resource busy`, `File exists`), the root filesystem never getting the name. Tests: `it_answers_as_a_read_only_memfs_does`, `every_change_is_refused` (Task 4), `a_filesystem_mounts_on_a_name_its_directory_does_not_have`, `a_mount_on_a_name_is_a_mount_point_like_any_other`, `only_the_last_name_of_a_mount_point_may_be_missing` (Task 5), the `system` scenario (Task 13).
4. **A program the build got wrong:** a change to the linker script or the runtime that puts a segment below `0x40_0000`, makes one writable and executable, drops or changes the ABI note, or links dynamically. Expected: `cargo xtask build` fails, naming the program and the rule it breaks, before an image is made. Tests: `check_program_refuses_what_the_kernel_would`, `every_program_is_built_for_ring_3` (Task 8), `the_kernel_s_segment_rules_are_the_build_s` (Task 9), `the_note_is_laid_out_as_elf_says` (Task 6).
5. **Building and testing on another machine:** CI's runner, or a checkout without `readelf` or Linux's headers, or `cargo test` of xtask building the programs while an outer build runs. Expected: a test that fails with a clear message, never one that is skipped, and no build waiting on another's lock. Tests: `every_number_is_linux_s` (Task 2), `every_program_is_built_for_ring_3` (Task 8), `a_missing_tool_is_named` (Task 10).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/heap/` | The heap allocator (`Heap`, `HeapStats`), moved from the kernel |
| `crates/crc32/` | CRC-32, moved from `kernel/src/block/crc32.rs` |
| `crates/relay-abi/src/{lib,call,result,errno}.rs` | `VERSION`, the note constants, `Call`, `encode`/`decode`, error numbers |
| `crates/sysimg/src/{lib,format,fs}.rs` | The archive's writer and checking reader; `SysImgFs` |
| `crates/vfs/src/mount.rs` | Mounts on a name its directory does not have |
| `crates/relay-rt/{build.rs,user.ld}`, `src/{lib,args,arch,note,start,sys}.rs` | The runtime of user programs and their linker script |
| `userland/tests/` | Package `relay-tests`: `t-args` |
| `Cargo.toml` | The new members and `[profile.user]` |
| `xtask/src/{userland,build,image,e2e,ci,config,flash}.rs` | Building, checking and packing the programs; `system.img` on the ESP; the steps `esp-delete` and `system-abi`; linting the user packages |
| `crates/boot-info/src/lib.rs`, `boot/src/main.rs` | `BootInfo` version 2; the loader loading `system.img` |
| `kernel/src/{system,lib,session}.rs`, `kernel/src/mm/heap.rs`, `kernel/src/block/{mod,gpt}.rs` | The `system` startup step; `KernelHeap` over `heap::Heap`; GPT over `crc32` |
| `tests/e2e/{system,system_missing,system_abi,system_empty,boot,mount_fail}.txt` | The new scenarios; the new startup line; `bin` on the fallback root |
| `rootfs/root/checks/check3-a.sh`, `xtask/fixtures/checks/check3-a.{qemu,nuc}.log` | NUC check 3 expects the `system` line and lists `/bin` |
| `README.md`, `docs/hardware-test.md` | The new crates and tools; check 3 and its failures |

---

## PR 1: The spec, the roadmap and this plan (already committed)

The gate's spec (with its §16 item 1), the milestone 2 roadmap and this plan are committed on branch `m2/spec` (worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2-spec`). They change no code, but CI runs on every pull request. Run the commands below from that worktree.

- [ ] **Push and open the pull request**

````bash
git push -u origin m2/spec
gh pr create --base main --head m2/spec --title "Milestone 2: the user-space gate's design, the roadmap and plan 1" --body-file - <<'EOF2'
## What

The design of the user-space gate (milestones 2 and 3: the shell and every command as ring-3 programs, pipes, jobs, script variables), the roadmap of milestone 2 (plans 1–5, with milestone 1's deferred findings placed in them), and the full implementation plan of plan 1 ("toolchain and archive"), whose decisions are the spec's §16 item 1.

## How it was tested

- [x] Every task of plan 1 was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m2/spec --watch`). Ask the user to review and merge. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2-spec` and continue with PR 2.

---

## PR 2: The heap crate and the ABI (Tasks 1–2)

Two crates everything later builds on: the heap allocator, now shared, and `relay-abi`.

Branch `m2p1/crates`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-crates`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p1/crates /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-crates origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-crates
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2/spec` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p1/crates /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-crates m2/spec`), and after its merge rebase with `git rebase --onto origin/main <old tip of m2/spec>` and re-run `cargo xtask ci` before pushing.

### Task 1: The heap allocator in its own crate

User programs need a heap too (spec §8.1: `relay-rt`'s global allocator comes from `crates/heap`), so the kernel's allocator moves out of `kernel/src/mm/heap.rs` into `crates/heap`, unchanged: `Heap` with its size classes and first-fit list, `HeapStats`, `SIZE_CLASSES`, and five of its tests, among them `random_mix_never_overlaps` (20,000 seeded operations). The kernel keeps what is the kernel's: `KernelHeap`, the `Heap` behind a spin lock registered as `#[global_allocator]`, whose failure panics with the heap's figures, and its two tests. `kernel::mm::heap::HeapStats` stays reachable (re-exported), so no caller changes. This is a move under the existing tests and has no red run; the heap's randomized test runs in the new crate.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/heap/Cargo.toml`
- Create: `crates/heap/src/lib.rs`
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/mm/heap.rs`

**Interfaces:**
- Consumes: milestone 1's heap (`kernel/src/mm/heap.rs`).
- Produces: crate `heap` with `Heap::{empty, new, alloc, dealloc, stats}`, `HeapStats`, `SIZE_CLASSES`; `relay_kernel::mm::heap::{KernelHeap, HeapStats}` as before.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "xtask"]

````

Replace:

````toml
usb = { path = "crates/usb" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
usb = { path = "crates/usb" }
heap = { path = "crates/heap" }
uefi = { version = "0.41", default-features = false }
````

- [ ] **Step 2: Create `crates/heap/Cargo.toml`**

Create `crates/heap/Cargo.toml`:

````toml
[package]
name = "heap"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
````

- [ ] **Step 3: Create `crates/heap/src/lib.rs`**

Create `crates/heap/src/lib.rs`:

````rust
//! A heap allocator over one region of memory (spec §5.2 of milestone 1).
//! The kernel's `#[global_allocator]` is one of these behind a lock
//! (`kernel/src/mm/heap.rs`); user programs will get one too.
//!
//! - **Small blocks** (size and alignment at most 2 KiB) come from size
//!   classes 16 B to 2 KiB. Each class has a free list of blocks carved out
//!   of 4 KiB slabs; slabs are taken from the large-block list and never
//!   returned.
//! - **Larger blocks** come from an address-ordered first-fit free list.
//!   Freed blocks are merged with free neighbours.
//!
//! No block header is needed: `dealloc` receives the same `Layout` as
//! `alloc`, which gives the class or the rounded size again.
#![cfg_attr(not(test), no_std)]

use core::alloc::Layout;
use core::ptr::NonNull;

pub const SIZE_CLASSES: [usize; 8] = [16, 32, 64, 128, 256, 512, 1024, 2048];
const SLAB: usize = 4096;
const MIN_BLOCK: usize = 16;

/// A free large block, stored in the free memory itself.
struct FreeBlock {
    size: usize,
    next: Link<FreeBlock>,
}

/// A free small block: only the link to the next one of its class.
struct SmallBlock {
    next: Link<SmallBlock>,
}

/// A free-list link; `None` ends the list. Every block on a list is free
/// heap memory, so a `Some` may be read and written.
type Link<T> = Option<NonNull<T>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapStats {
    pub total: usize,
    /// Bytes handed out, counted at their class or rounded size.
    pub used: usize,
    /// Bytes on the large-block free list.
    pub free_large: usize,
    pub largest_free: usize,
}

pub struct Heap {
    start: usize,
    end: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
    large: Link<FreeBlock>,
    used: usize,
}

// SAFETY: the links point into the heap region, which the Heap owns.
unsafe impl Send for Heap {}

fn class_of(layout: Layout) -> Option<usize> {
    let size = layout.size().max(layout.align());
    SIZE_CLASSES.iter().position(|&c| c >= size)
}

fn large_size(layout: Layout) -> usize {
    layout.size().max(MIN_BLOCK).next_multiple_of(MIN_BLOCK)
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap {
            start: 0,
            end: 0,
            classes: [None; SIZE_CLASSES.len()],
            large: None,
            used: 0,
        }
    }

    /// # Safety
    /// [start, start + size) must be writable memory owned by this heap for
    /// its whole life. `start` must be 16-byte aligned.
    pub unsafe fn new(start: usize, size: usize) -> Heap {
        assert!(start.is_multiple_of(MIN_BLOCK));
        let size = size & !(MIN_BLOCK - 1);
        let mut h = Heap::empty();
        h.start = start;
        h.end = start + size;
        unsafe { h.insert_free(start, size) };
        h
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let addr = match class_of(layout) {
            Some(c) => self.alloc_small(c)?,
            None => {
                let size = large_size(layout);
                let a = self.alloc_large(size, layout.align().max(MIN_BLOCK))?;
                self.used += size;
                a
            }
        };
        NonNull::new(addr as *mut u8)
    }

    /// # Safety
    /// `ptr` must come from `alloc` on this heap with the same `layout`.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let addr = ptr.as_ptr() as usize;
        debug_assert!(self.start <= addr && addr < self.end, "foreign pointer");
        match class_of(layout) {
            Some(c) => {
                let block = ptr.cast::<SmallBlock>();
                unsafe {
                    block.write(SmallBlock {
                        next: self.classes[c],
                    })
                };
                self.classes[c] = Some(block);
                self.used -= SIZE_CLASSES[c];
            }
            None => {
                let size = large_size(layout);
                unsafe { self.insert_free(addr, size) };
                self.used -= size;
            }
        }
    }

    pub fn stats(&self) -> HeapStats {
        let (mut free_large, mut largest_free) = (0, 0);
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size, next } = unsafe { b.read() };
            free_large += size;
            largest_free = largest_free.max(size);
            link = next;
        }
        HeapStats {
            total: self.end - self.start,
            used: self.used,
            free_large,
            largest_free,
        }
    }

    fn alloc_small(&mut self, c: usize) -> Option<usize> {
        let block = match self.classes[c] {
            Some(block) => block,
            None => self.refill(c)?,
        };
        self.classes[c] = unsafe { block.read() }.next;
        self.used += SIZE_CLASSES[c];
        Some(block.as_ptr() as usize)
    }

    /// Carves a new slab into blocks of class `c` and returns the first.
    fn refill(&mut self, c: usize) -> Option<NonNull<SmallBlock>> {
        let slab = self.alloc_large(SLAB, SLAB)?;
        let size = SIZE_CLASSES[c];
        // Push the blocks in reverse so they are handed out in address
        // order.
        for addr in (slab..slab + SLAB).step_by(size).rev() {
            let block = block_at::<SmallBlock>(addr);
            unsafe {
                block.write(SmallBlock {
                    next: self.classes[c],
                })
            };
            self.classes[c] = Some(block);
        }
        self.classes[c]
    }

    /// First fit: the lowest free block that holds `size` bytes at an
    /// `align`-aligned address. What is left before and after goes back on
    /// the free list.
    fn alloc_large(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: Link<FreeBlock> = None;
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size: bsize, next } = unsafe { b.read() };
            let start = b.as_ptr() as usize;
            let aligned = start.next_multiple_of(align);
            if aligned + size <= start + bsize {
                // Unlink, then return the unused head and tail.
                match prev {
                    None => self.large = next,
                    Some(mut p) => unsafe { p.as_mut().next = next },
                }
                unsafe {
                    if aligned > start {
                        self.insert_free(start, aligned - start);
                    }
                    let tail = start + bsize - (aligned + size);
                    if tail > 0 {
                        self.insert_free(aligned + size, tail);
                    }
                }
                return Some(aligned);
            }
            prev = link;
            link = next;
        }
        None
    }

    /// Inserts [addr, addr + size) in address order, merging it with the
    /// blocks directly before and after it.
    ///
    /// # Safety
    /// The range must be free heap memory, 16-byte aligned, at least 16
    /// bytes long.
    unsafe fn insert_free(&mut self, addr: usize, size: usize) {
        let mut prev: Link<FreeBlock> = None;
        let mut next = self.large;
        while let Some(n) = next.filter(|n| (n.as_ptr() as usize) < addr) {
            prev = next;
            next = unsafe { n.read() }.next;
        }
        let mut block = block_at::<FreeBlock>(addr);
        unsafe { block.write(FreeBlock { size, next }) };
        let b = unsafe { block.as_mut() };
        if let Some(n) = next.filter(|n| addr + size == n.as_ptr() as usize) {
            let n = unsafe { n.read() };
            b.size += n.size;
            b.next = n.next;
        }
        match prev {
            None => self.large = Some(block),
            Some(mut p) => {
                let p = unsafe { p.as_mut() };
                if p as *mut FreeBlock as usize + p.size == addr {
                    p.size += b.size;
                    p.next = b.next;
                } else {
                    p.next = Some(block);
                }
            }
        }
    }
}

/// The block at `addr`, a heap address (never 0: no heap starts at page 0,
/// which stays unmapped in the kernel and in user programs).
fn block_at<T>(addr: usize) -> NonNull<T> {
    NonNull::new(addr as *mut T).expect("heap block at address 0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heap over a fresh 4 KiB-aligned host allocation.
    fn heap(size: usize) -> Heap {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        unsafe { Heap::new(mem, size) }
    }

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn small_blocks_are_aligned_distinct_and_reused() {
        let mut h = heap(64 * 1024);
        let a = h.alloc(l(24, 8)).unwrap();
        let b = h.alloc(l(24, 8)).unwrap();
        assert_eq!(b.as_ptr() as usize - a.as_ptr() as usize, 32, "class 32");
        let c = h.alloc(l(1, 1)).unwrap();
        let d = h.alloc(l(100, 128)).unwrap();
        assert_eq!(d.as_ptr() as usize % 128, 0);
        assert_eq!(h.stats().used, 32 + 32 + 16 + 128);
        unsafe { h.dealloc(a, l(24, 8)) };
        assert_eq!(h.alloc(l(20, 4)), Some(a), "freed block is reused");
        unsafe {
            h.dealloc(c, l(1, 1));
            h.dealloc(d, l(100, 128));
        }
    }

    #[test]
    fn large_blocks_merge_when_freed() {
        let mut h = heap(1 << 20);
        let total = h.stats().free_large;
        let a = h.alloc(l(10_000, 16)).unwrap();
        let b = h.alloc(l(20_000, 16)).unwrap();
        let c = h.alloc(l(30_000, 16)).unwrap();
        unsafe {
            h.dealloc(b, l(20_000, 16));
            h.dealloc(a, l(10_000, 16));
        }
        // a and b merged: 30_000 bytes fit where they were.
        assert_eq!(h.alloc(l(30_000, 16)), Some(a));
        unsafe {
            h.dealloc(a, l(30_000, 16));
            h.dealloc(c, l(30_000, 16));
        }
        let s = h.stats();
        assert_eq!(s.used, 0);
        assert_eq!(s.free_large, total);
        assert_eq!(s.largest_free, total, "everything merged into one block");
    }

    #[test]
    fn large_alignment_is_honoured() {
        let mut h = heap(1 << 20);
        let _small = h.alloc(l(5000, 16)).unwrap();
        let p = h.alloc(l(64 * 1024, 4096)).unwrap();
        assert_eq!(p.as_ptr() as usize % 4096, 0);
        let q = h.alloc(l(16, 8192)).unwrap(); // tiny but over-aligned
        assert_eq!(q.as_ptr() as usize % 8192, 0);
        unsafe { h.dealloc(q, l(16, 8192)) };
    }

    #[test]
    fn running_out_returns_none() {
        let mut h = heap(64 * 1024);
        assert!(h.alloc(l(65 * 1024, 16)).is_none());
        let a = h.alloc(l(60 * 1024, 16)).unwrap();
        assert!(h.alloc(l(8 * 1024, 16)).is_none());
        unsafe { h.dealloc(a, l(60 * 1024, 16)) };
        assert!(h.alloc(l(8 * 1024, 16)).is_some());
    }

    /// Seeded random alloc/free mix. Every block is filled with a tag byte
    /// and checked before it is freed, so any overlap between live blocks
    /// shows up.
    #[test]
    fn random_mix_never_overlaps() {
        let mut h = heap(4 << 20);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut live: Vec<(NonNull<u8>, Layout, u8)> = Vec::new();
        for i in 0..20_000u32 {
            if live.len() > 300 || (!live.is_empty() && rnd() % 3 == 0) {
                let (p, layout, tag) = live.swap_remove(rnd() as usize % live.len());
                let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), layout.size()) };
                assert!(bytes.iter().all(|&b| b == tag), "block overwritten");
                unsafe { h.dealloc(p, layout) };
            } else {
                let size = match rnd() % 4 {
                    0 => 1 + rnd() as usize % 64,
                    1 => 1 + rnd() as usize % 2048,
                    _ => 1 + rnd() as usize % 20_000,
                };
                let align = 1 << (rnd() % 13);
                let layout = l(size, align);
                let p = h.alloc(layout).expect("4 MiB is plenty for 300 blocks");
                assert_eq!(p.as_ptr() as usize % align, 0);
                let tag = i as u8;
                unsafe { std::ptr::write_bytes(p.as_ptr(), tag, size) };
                live.push((p, layout, tag));
            }
        }
        for (p, layout, _) in live.drain(..) {
            unsafe { h.dealloc(p, layout) };
        }
        assert_eq!(h.stats().used, 0);
    }
}
````

- [ ] **Step 4: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
usb.workspace = true
x86_64.workspace = true
````

with:

````toml
usb.workspace = true
heap.workspace = true
x86_64.workspace = true
````

- [ ] **Step 5: Add the failing tests to `kernel/src/mm/heap.rs`**

In `kernel/src/mm/heap.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// A heap over a fresh 4 KiB-aligned host allocation.
    fn heap(size: usize) -> Heap {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let mem = unsafe { std::alloc::alloc(layout) } as usize;
        assert_ne!(mem, 0);
        unsafe { Heap::new(mem, size) }
    }

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn small_blocks_are_aligned_distinct_and_reused() {
        let mut h = heap(64 * 1024);
        let a = h.alloc(l(24, 8)).unwrap();
        let b = h.alloc(l(24, 8)).unwrap();
        assert_eq!(b.as_ptr() as usize - a.as_ptr() as usize, 32, "class 32");
        let c = h.alloc(l(1, 1)).unwrap();
        let d = h.alloc(l(100, 128)).unwrap();
        assert_eq!(d.as_ptr() as usize % 128, 0);
        assert_eq!(h.stats().used, 32 + 32 + 16 + 128);
        unsafe { h.dealloc(a, l(24, 8)) };
        assert_eq!(h.alloc(l(20, 4)), Some(a), "freed block is reused");
        unsafe {
            h.dealloc(c, l(1, 1));
            h.dealloc(d, l(100, 128));
        }
    }

    #[test]
    fn large_blocks_merge_when_freed() {
        let mut h = heap(1 << 20);
        let total = h.stats().free_large;
        let a = h.alloc(l(10_000, 16)).unwrap();
        let b = h.alloc(l(20_000, 16)).unwrap();
        let c = h.alloc(l(30_000, 16)).unwrap();
        unsafe {
            h.dealloc(b, l(20_000, 16));
            h.dealloc(a, l(10_000, 16));
        }
        // a and b merged: 30_000 bytes fit where they were.
        assert_eq!(h.alloc(l(30_000, 16)), Some(a));
        unsafe {
            h.dealloc(a, l(30_000, 16));
            h.dealloc(c, l(30_000, 16));
        }
        let s = h.stats();
        assert_eq!(s.used, 0);
        assert_eq!(s.free_large, total);
        assert_eq!(s.largest_free, total, "everything merged into one block");
    }

    #[test]
    fn large_alignment_is_honoured() {
        let mut h = heap(1 << 20);
        let _small = h.alloc(l(5000, 16)).unwrap();
        let p = h.alloc(l(64 * 1024, 4096)).unwrap();
        assert_eq!(p.as_ptr() as usize % 4096, 0);
        let q = h.alloc(l(16, 8192)).unwrap(); // tiny but over-aligned
        assert_eq!(q.as_ptr() as usize % 8192, 0);
        unsafe { h.dealloc(q, l(16, 8192)) };
    }

    #[test]
    fn running_out_returns_none() {
        let mut h = heap(64 * 1024);
        assert!(h.alloc(l(65 * 1024, 16)).is_none());
        let a = h.alloc(l(60 * 1024, 16)).unwrap();
        assert!(h.alloc(l(8 * 1024, 16)).is_none());
        unsafe { h.dealloc(a, l(60 * 1024, 16)) };
        assert!(h.alloc(l(8 * 1024, 16)).is_some());
    }
````

with:

````rust

    fn l(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }
````

Replace:

````rust
    }

    /// Seeded random alloc/free mix. Every block is filled with a tag byte
    /// and checked before it is freed, so any overlap between live blocks
    /// shows up.
    #[test]
    fn random_mix_never_overlaps() {
        let mut h = heap(4 << 20);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut live: Vec<(NonNull<u8>, Layout, u8)> = Vec::new();
        for i in 0..20_000u32 {
            if live.len() > 300 || (!live.is_empty() && rnd() % 3 == 0) {
                let (p, layout, tag) = live.swap_remove(rnd() as usize % live.len());
                let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), layout.size()) };
                assert!(bytes.iter().all(|&b| b == tag), "block overwritten");
                unsafe { h.dealloc(p, layout) };
            } else {
                let size = match rnd() % 4 {
                    0 => 1 + rnd() as usize % 64,
                    1 => 1 + rnd() as usize % 2048,
                    _ => 1 + rnd() as usize % 20_000,
                };
                let align = 1 << (rnd() % 13);
                let layout = l(size, align);
                let p = h.alloc(layout).expect("4 MiB is plenty for 300 blocks");
                assert_eq!(p.as_ptr() as usize % align, 0);
                let tag = i as u8;
                unsafe { std::ptr::write_bytes(p.as_ptr(), tag, size) };
                live.push((p, layout, tag));
            }
        }
        for (p, layout, _) in live.drain(..) {
            unsafe { h.dealloc(p, layout) };
        }
        assert_eq!(h.stats().used, 0);
    }
}
````

with:

````rust
    }
}
````

- [ ] **Step 6: Change `kernel/src/mm/heap.rs`**

In `kernel/src/mm/heap.rs`, replace:

````rust
//! The kernel heap allocator (spec §5.2), registered as `#[global_allocator]`.
//!
//! - **Small blocks** (size and alignment at most 2 KiB) come from size
//!   classes 16 B to 2 KiB. Each class has a free list of blocks carved out
//!   of 4 KiB slabs; slabs are taken from the large-block list and never
//!   returned.
//! - **Larger blocks** come from an address-ordered first-fit free list.
//!   Freed blocks are merged with free neighbours.
//!
//! No block header is needed: `dealloc` receives the same `Layout` as
//! `alloc`, which gives the class or the rounded size again. Interrupt
//! handlers must never allocate (the heap lock is not interrupt-safe).

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use spin::Mutex;

pub const SIZE_CLASSES: [usize; 8] = [16, 32, 64, 128, 256, 512, 1024, 2048];
const SLAB: usize = 4096;
const MIN_BLOCK: usize = 16;

/// A free large block, stored in the free memory itself.
struct FreeBlock {
    size: usize,
    next: Link<FreeBlock>,
}

/// A free small block: only the link to the next one of its class.
struct SmallBlock {
    next: Link<SmallBlock>,
}

/// A free-list link; `None` ends the list. Every block on a list is free
/// heap memory, so a `Some` may be read and written.
type Link<T> = Option<NonNull<T>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapStats {
    pub total: usize,
    /// Bytes handed out, counted at their class or rounded size.
    pub used: usize,
    /// Bytes on the large-block free list.
    pub free_large: usize,
    pub largest_free: usize,
}

pub struct Heap {
    start: usize,
    end: usize,
    classes: [Link<SmallBlock>; SIZE_CLASSES.len()],
    large: Link<FreeBlock>,
    used: usize,
}

// SAFETY: the links point into the heap region, which the Heap owns.
unsafe impl Send for Heap {}

fn class_of(layout: Layout) -> Option<usize> {
    let size = layout.size().max(layout.align());
    SIZE_CLASSES.iter().position(|&c| c >= size)
}

fn large_size(layout: Layout) -> usize {
    layout.size().max(MIN_BLOCK).next_multiple_of(MIN_BLOCK)
}

impl Heap {
    pub const fn empty() -> Heap {
        Heap {
            start: 0,
            end: 0,
            classes: [None; SIZE_CLASSES.len()],
            large: None,
            used: 0,
        }
    }

    /// # Safety
    /// [start, start + size) must be writable memory owned by this heap for
    /// its whole life. `start` must be 16-byte aligned.
    pub unsafe fn new(start: usize, size: usize) -> Heap {
        assert!(start.is_multiple_of(MIN_BLOCK));
        let size = size & !(MIN_BLOCK - 1);
        let mut h = Heap::empty();
        h.start = start;
        h.end = start + size;
        unsafe { h.insert_free(start, size) };
        h
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let addr = match class_of(layout) {
            Some(c) => self.alloc_small(c)?,
            None => {
                let size = large_size(layout);
                let a = self.alloc_large(size, layout.align().max(MIN_BLOCK))?;
                self.used += size;
                a
            }
        };
        NonNull::new(addr as *mut u8)
    }

    /// # Safety
    /// `ptr` must come from `alloc` on this heap with the same `layout`.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let addr = ptr.as_ptr() as usize;
        debug_assert!(self.start <= addr && addr < self.end, "foreign pointer");
        match class_of(layout) {
            Some(c) => {
                let block = ptr.cast::<SmallBlock>();
                unsafe {
                    block.write(SmallBlock {
                        next: self.classes[c],
                    })
                };
                self.classes[c] = Some(block);
                self.used -= SIZE_CLASSES[c];
            }
            None => {
                let size = large_size(layout);
                unsafe { self.insert_free(addr, size) };
                self.used -= size;
            }
        }
    }

    pub fn stats(&self) -> HeapStats {
        let (mut free_large, mut largest_free) = (0, 0);
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size, next } = unsafe { b.read() };
            free_large += size;
            largest_free = largest_free.max(size);
            link = next;
        }
        HeapStats {
            total: self.end - self.start,
            used: self.used,
            free_large,
            largest_free,
        }
    }

    fn alloc_small(&mut self, c: usize) -> Option<usize> {
        let block = match self.classes[c] {
            Some(block) => block,
            None => self.refill(c)?,
        };
        self.classes[c] = unsafe { block.read() }.next;
        self.used += SIZE_CLASSES[c];
        Some(block.as_ptr() as usize)
    }

    /// Carves a new slab into blocks of class `c` and returns the first.
    fn refill(&mut self, c: usize) -> Option<NonNull<SmallBlock>> {
        let slab = self.alloc_large(SLAB, SLAB)?;
        let size = SIZE_CLASSES[c];
        // Push the blocks in reverse so they are handed out in address
        // order.
        for addr in (slab..slab + SLAB).step_by(size).rev() {
            let block = block_at::<SmallBlock>(addr);
            unsafe {
                block.write(SmallBlock {
                    next: self.classes[c],
                })
            };
            self.classes[c] = Some(block);
        }
        self.classes[c]
    }

    /// First fit: the lowest free block that holds `size` bytes at an
    /// `align`-aligned address. What is left before and after goes back on
    /// the free list.
    fn alloc_large(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: Link<FreeBlock> = None;
        let mut link = self.large;
        while let Some(b) = link {
            let FreeBlock { size: bsize, next } = unsafe { b.read() };
            let start = b.as_ptr() as usize;
            let aligned = start.next_multiple_of(align);
            if aligned + size <= start + bsize {
                // Unlink, then return the unused head and tail.
                match prev {
                    None => self.large = next,
                    Some(mut p) => unsafe { p.as_mut().next = next },
                }
                unsafe {
                    if aligned > start {
                        self.insert_free(start, aligned - start);
                    }
                    let tail = start + bsize - (aligned + size);
                    if tail > 0 {
                        self.insert_free(aligned + size, tail);
                    }
                }
                return Some(aligned);
            }
            prev = link;
            link = next;
        }
        None
    }

    /// Inserts [addr, addr + size) in address order, merging it with the
    /// blocks directly before and after it.
    ///
    /// # Safety
    /// The range must be free heap memory, 16-byte aligned, at least 16
    /// bytes long.
    unsafe fn insert_free(&mut self, addr: usize, size: usize) {
        let mut prev: Link<FreeBlock> = None;
        let mut next = self.large;
        while let Some(n) = next.filter(|n| (n.as_ptr() as usize) < addr) {
            prev = next;
            next = unsafe { n.read() }.next;
        }
        let mut block = block_at::<FreeBlock>(addr);
        unsafe { block.write(FreeBlock { size, next }) };
        let b = unsafe { block.as_mut() };
        if let Some(n) = next.filter(|n| addr + size == n.as_ptr() as usize) {
            let n = unsafe { n.read() };
            b.size += n.size;
            b.next = n.next;
        }
        match prev {
            None => self.large = Some(block),
            Some(mut p) => {
                let p = unsafe { p.as_mut() };
                if p as *mut FreeBlock as usize + p.size == addr {
                    p.size += b.size;
                    p.next = b.next;
                } else {
                    p.next = Some(block);
                }
            }
        }
    }
}

/// The block at `addr`, a heap address (never 0: the heap is in the
/// kernel's upper half).
fn block_at<T>(addr: usize) -> NonNull<T> {
    NonNull::new(addr as *mut T).expect("heap block at address 0")
}

````

with:

````rust
//! The kernel heap (spec §5.2): a `heap::Heap` behind a spin lock,
//! registered as `#[global_allocator]` in `mm`. Interrupt handlers must
//! never allocate (the lock is not interrupt-safe).

use ::heap::Heap;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use spin::Mutex;

pub use ::heap::HeapStats;

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p heap`

Expected: PASS: 5 tests.

Run: `cargo test -p relay-kernel --lib heap`

Expected: PASS: 2 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add Cargo.lock Cargo.toml crates kernel
git commit -m "heap: the allocator is its own crate, so user programs can share it"
````


### Task 2: `relay-abi`: version, call numbers, error numbers and results

The crate both sides of the system-call boundary build on (spec §7), with no dependencies and no architecture detail: `VERSION` (1), the ELF note's owner `Relay` and type 1 (spec §5.2), the calls of spec §7.3 numbered from 1 in the spec's order (`Call`, 37 of them; `from_number` gives `None` for a number no call has, which the kernel will answer with `ENOSYS`), the error numbers of spec §7.2 with Linux's values, and how a result register carries a value (0 to 2^63 − 1) or a negated error number (−4095 to −1). The error numbers are checked against Linux's own headers (`/usr/include/asm-generic/errno-base.h` and `errno.h`, from `linux-libc-dev`, on every Ubuntu machine with a C compiler and on the CI runner): a missing header fails the test, it is never skipped. The structs of the calls (`SpawnArgs`, `WaitStatus`, `Stat`, …) come with the plans that implement their calls.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/relay-abi/Cargo.toml`
- Create: `crates/relay-abi/src/call.rs`
- Create: `crates/relay-abi/src/errno.rs`
- Create: `crates/relay-abi/src/lib.rs`
- Create: `crates/relay-abi/src/result.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: crate `relay-abi` (`relay_abi`): `VERSION: u32`, `NOTE_NAME: &[u8]` (`b"Relay"`), `NOTE_TYPE: u32`, `Call` (`#[repr(u64)]`, `Call::ALL`, `number()`, `from_number(u64) -> Option<Call>`), `MAX_ERRNO`, `encode(Result<u64, u16>) -> u64`, `decode(u64) -> Result<u64, u16>`, `errno::{EPERM, ENOENT, …}` as `u16`.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "xtask"]

````

Replace:

````toml
heap = { path = "crates/heap" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
heap = { path = "crates/heap" }
relay-abi = { path = "crates/relay-abi" }
uefi = { version = "0.41", default-features = false }
````

- [ ] **Step 2: Create `crates/relay-abi/Cargo.toml`**

Create `crates/relay-abi/Cargo.toml`:

````toml
[package]
name = "relay-abi"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
````

- [ ] **Step 3: Write the failing tests for `crates/relay-abi/src/call.rs`**

Create `crates/relay-abi/src/call.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_numbered_from_1_in_spec_order() {
        for (i, c) in Call::ALL.iter().enumerate() {
            assert_eq!(c.number(), i as u64 + 1, "{c:?}");
        }
        assert_eq!(Call::Exit.number(), 1);
        assert_eq!(Call::Write.number(), 12);
        assert_eq!(Call::Power.number(), 36);
        assert_eq!(Call::Pipe.number(), 37);
    }

    #[test]
    fn numbers_map_back_to_their_calls() {
        for c in Call::ALL {
            assert_eq!(Call::from_number(c.number()), Some(c));
        }
        for n in [0, 38, 1000, u64::MAX] {
            assert_eq!(Call::from_number(n), None, "{n}");
        }
    }
}
````

- [ ] **Step 4: Write the failing tests for `crates/relay-abi/src/errno.rs`**

Create `crates/relay-abi/src/errno.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// `#define ENAME value` lines of Linux's own headers (linux-libc-dev,
    /// on every Ubuntu machine with a C compiler, the CI runner included).
    fn linux_errnos() -> HashMap<String, u16> {
        let mut map = HashMap::new();
        for file in ["errno-base.h", "errno.h"] {
            let path = format!("/usr/include/asm-generic/{file}");
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            for line in text.lines() {
                let mut words = line.split_whitespace();
                if let (Some("#define"), Some(name), Some(value)) =
                    (words.next(), words.next(), words.next())
                    && let Ok(n) = value.parse()
                {
                    map.insert(name.to_string(), n);
                }
            }
        }
        map
    }

    #[test]
    fn every_number_is_linux_s() {
        let linux = linux_errnos();
        let ours = [
            ("EPERM", EPERM),
            ("ENOENT", ENOENT),
            ("ESRCH", ESRCH),
            ("EINTR", EINTR),
            ("EIO", EIO),
            ("E2BIG", E2BIG),
            ("ENOEXEC", ENOEXEC),
            ("EBADF", EBADF),
            ("ECHILD", ECHILD),
            ("EAGAIN", EAGAIN),
            ("ENOMEM", ENOMEM),
            ("EFAULT", EFAULT),
            ("EBUSY", EBUSY),
            ("EEXIST", EEXIST),
            ("EXDEV", EXDEV),
            ("ENOTDIR", ENOTDIR),
            ("EISDIR", EISDIR),
            ("EINVAL", EINVAL),
            ("EMFILE", EMFILE),
            ("EFBIG", EFBIG),
            ("ENOSPC", ENOSPC),
            ("EROFS", EROFS),
            ("EPIPE", EPIPE),
            ("ENAMETOOLONG", ENAMETOOLONG),
            ("ENOSYS", ENOSYS),
            ("ENOTEMPTY", ENOTEMPTY),
        ];
        for (name, n) in ours {
            assert_eq!(linux.get(name), Some(&n), "{name}");
        }
    }
}
````

- [ ] **Step 5: Create `crates/relay-abi/src/lib.rs`**

Create `crates/relay-abi/src/lib.rs`:

````rust
//! The Relay system-call ABI (spec §7 of the user-space gate): the version,
//! the ELF note that marks a program built for it, the call numbers, the
//! error numbers and how a call's result carries a value or an error.
//!
//! The kernel and the programs' runtime both build on this crate, so they
//! cannot disagree. It holds no architecture detail: which registers carry
//! the number, the arguments and the result is written down in spec §7.1
//! and implemented only in the `arch` modules.
#![cfg_attr(not(test), no_std)]

mod call;
pub mod errno;
mod result;

pub use call::Call;
pub use result::{MAX_ERRNO, decode, encode};

/// Changes whenever a call's meaning or a struct's layout changes; adding
/// a call does not change it (spec §7.4). Written into `system.img`'s
/// header and into every program's ELF note, and checked by the kernel.
pub const VERSION: u32 = 1;

/// The ELF note that names the ABI a program was built for (spec §5.2):
/// owner `Relay`, type [`NOTE_TYPE`], a 4-byte descriptor holding
/// [`VERSION`].
pub const NOTE_NAME: &[u8] = b"Relay";
pub const NOTE_TYPE: u32 = 1;
````

- [ ] **Step 6: Write the failing tests for `crates/relay-abi/src/result.rs`**

Create `crates/relay-abi/src/result.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::errno;

    #[test]
    fn results_carry_values_or_negated_error_numbers() {
        assert_eq!(encode(Ok(0)), 0);
        assert_eq!(encode(Ok(4096)), 4096);
        assert_eq!(encode(Err(errno::EPERM)), u64::MAX, "-1");
        assert_eq!(encode(Err(errno::ENOENT)), u64::MAX - 1, "-2");
        assert_eq!(encode(Err(4095)), u64::MAX - 4094, "-4095");
        for r in [Ok(0), Ok(1), Ok((1 << 63) - 1), Err(1), Err(38), Err(4095)] {
            assert_eq!(decode(encode(r)), r, "{r:?}");
        }
    }

    #[test]
    fn only_the_top_4095_values_are_errors() {
        assert_eq!(decode(u64::MAX - 4095), Ok(u64::MAX - 4095));
        assert_eq!(decode(u64::MAX - 4094), Err(4095));
        assert_eq!(decode(1 << 63), Ok(1 << 63));
    }
}
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p relay-abi`

Expected: FAIL: compile errors such as `` cannot find value `EPERM` in this scope ``; `` cannot find value `ENOENT` in this scope ``.

- [ ] **Step 8: Implement `crates/relay-abi/src/call.rs`**

Insert this at the top of `crates/relay-abi/src/call.rs`, above `#[cfg(test)]`:

````rust
//! The call numbers (spec §7.3).

/// The system calls, numbered from 1 in the order of spec §7.3. A number is
/// never reused.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Exit = 1,
    Spawn,
    Wait,
    Kill,
    Getpid,
    ProcList,
    MemMap,
    MemUnmap,
    Open,
    Close,
    Read,
    Write,
    Seek,
    Fstat,
    Stat,
    ReadDir,
    Mkdir,
    Rmdir,
    Unlink,
    Truncate,
    Touch,
    Readlink,
    Rename,
    Statfs,
    Sync,
    Chdir,
    Getcwd,
    ConsoleMode,
    ConsoleSize,
    ConsoleForeground,
    ConsoleTeePush,
    ConsoleTeePop,
    Time,
    Sleep,
    SysInfo,
    Power,
    Pipe,
}

impl Call {
    /// Every call, in number order.
    pub const ALL: [Call; 37] = [
        Call::Exit,
        Call::Spawn,
        Call::Wait,
        Call::Kill,
        Call::Getpid,
        Call::ProcList,
        Call::MemMap,
        Call::MemUnmap,
        Call::Open,
        Call::Close,
        Call::Read,
        Call::Write,
        Call::Seek,
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
        Call::ConsoleMode,
        Call::ConsoleSize,
        Call::ConsoleForeground,
        Call::ConsoleTeePush,
        Call::ConsoleTeePop,
        Call::Time,
        Call::Sleep,
        Call::SysInfo,
        Call::Power,
        Call::Pipe,
    ];

    pub const fn number(self) -> u64 {
        self as u64
    }

    /// The call with number `n`; `None` for a number no call has (the
    /// kernel answers those with `ENOSYS`).
    pub fn from_number(n: u64) -> Option<Call> {
        let i = usize::try_from(n.checked_sub(1)?).ok()?;
        Call::ALL.get(i).copied()
    }
}

````

- [ ] **Step 9: Implement `crates/relay-abi/src/errno.rs`**

Insert this at the top of `crates/relay-abi/src/errno.rs`, above `#[cfg(test)]`:

````rust
//! Error numbers (spec §7.2). The values are Linux's, so a program's
//! messages match what GNU tools print for the same error.

pub const EPERM: u16 = 1;
pub const ENOENT: u16 = 2;
pub const ESRCH: u16 = 3;
pub const EINTR: u16 = 4;
pub const EIO: u16 = 5;
pub const E2BIG: u16 = 7;
pub const ENOEXEC: u16 = 8;
pub const EBADF: u16 = 9;
pub const ECHILD: u16 = 10;
pub const EAGAIN: u16 = 11;
pub const ENOMEM: u16 = 12;
pub const EFAULT: u16 = 14;
pub const EBUSY: u16 = 16;
pub const EEXIST: u16 = 17;
pub const EXDEV: u16 = 18;
pub const ENOTDIR: u16 = 20;
pub const EISDIR: u16 = 21;
pub const EINVAL: u16 = 22;
pub const EMFILE: u16 = 24;
pub const EFBIG: u16 = 27;
pub const ENOSPC: u16 = 28;
pub const EROFS: u16 = 30;
pub const EPIPE: u16 = 32;
pub const ENAMETOOLONG: u16 = 36;
pub const ENOSYS: u16 = 38;
pub const ENOTEMPTY: u16 = 39;

````

- [ ] **Step 10: Implement `crates/relay-abi/src/result.rs`**

Insert this at the top of `crates/relay-abi/src/result.rs`, above `#[cfg(test)]`:

````rust
//! How a result register carries a value or an error (spec §7.1).

/// Results from `MAX_ERRNO` below 2^64 up are negated error numbers.
pub const MAX_ERRNO: u64 = 4095;

/// A call's result as the kernel returns it: a value from 0 to 2^63 − 1, or
/// a negated error number from −4095 to −1.
pub fn encode(result: Result<u64, u16>) -> u64 {
    match result {
        Ok(v) => {
            debug_assert!(v < 1 << 63, "a result value must be below 2^63");
            v
        }
        Err(e) => {
            debug_assert!(e != 0 && u64::from(e) <= MAX_ERRNO);
            u64::from(e).wrapping_neg()
        }
    }
}

/// The value or error number in a result register.
pub fn decode(raw: u64) -> Result<u64, u16> {
    if raw > u64::MAX - MAX_ERRNO {
        Err(raw.wrapping_neg() as u16)
    } else {
        Ok(raw)
    }
}

````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p relay-abi`

Expected: PASS: 5 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add Cargo.lock Cargo.toml crates
git commit -m "relay-abi: the ABI version, call numbers, error numbers and result encoding"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 20 scenario(s) passed`.

````bash
git push -u origin m2p1/crates
gh pr create --base main --head m2p1/crates --title "Milestone 2, plan 1: The heap crate and the ABI" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 1–2: the kernel's heap allocator moves into `crates/heap`, unchanged, so user programs can share it; `crates/relay-abi` holds the ABI version, the ELF note's constants, the call numbers of spec §7.3, the error numbers (checked against Linux's headers) and the result encoding.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p1/crates --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-crates
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: The system archive and `/bin` (Tasks 3–5)

The archive's format, its read-only filesystem, and a mount table that can put it at `/bin` on any root.

Branch `m2p1/sysimg`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-sysimg`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p1/sysimg /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-sysimg origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-sysimg
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p1/crates` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p1/sysimg /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-sysimg m2p1/crates`), and after its merge rebase with `git rebase --onto origin/main <old tip of m2p1/crates>` and re-run `cargo xtask ci` before pushing.

### Task 3: The system archive's format

`system.img` (spec §4.2): a 64-byte header (magic `RELAYSYS`, format version 1, ABI version, entry count, total length, CRC-32 of everything after the header, and the build time at offset 40), a table of 88-byte entries sorted by name (name up to 64 bytes, its length, mode, data offset, data length), then each entry's data on a 4 KiB boundary. `sysimg::write` builds one (xtask); `Archive::parse` checks one (the kernel) in the order size, magic, format, length, CRC, table size, then every entry's name, order, mode and data range, all with checked arithmetic, so the accessors after it cannot fail. The file comes from a FAT partition on a stick, so everything in it is untrusted: a randomized test damages a valid archive 20,000 times (fixing the CRC half of the time, so the checks after it run too) and `parse` must never panic. The CRC-32 GPT already uses moves from `kernel/src/block/crc32.rs` into `crates/crc32`, so the kernel and xtask share it.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/crc32/Cargo.toml`
- Create: `crates/crc32/src/lib.rs`
- Create: `crates/sysimg/Cargo.toml`
- Create: `crates/sysimg/src/format.rs`
- Create: `crates/sysimg/src/lib.rs`
- Modify: `kernel/Cargo.toml`
- Delete: `kernel/src/block/crc32.rs`
- Modify: `kernel/src/block/gpt.rs`
- Modify: `kernel/src/block/mod.rs`

**Interfaces:**
- Consumes: milestone 1's `kernel::block::crc32::crc32`, which moves.
- Produces: crate `crc32` with `crc32(&[u8]) -> u32` (the kernel's GPT code uses it); crate `sysimg` with `write(abi: u32, build_time: u64, &[Entry]) -> Result<Vec<u8>, SysImgError>`, `Archive::{parse, abi, build_time, len, is_empty, total_len, entry, find, entries}`, `Entry { name, mode, data }`, `SysImgError` (with `Display`, used on the startup line), `valid_name`, and the layout constants `MAGIC`, `FORMAT_VERSION`, `HEADER_LEN`, `ENTRY_LEN`, `NAME_MAX`, `DATA_ALIGN`, `MODE_MASK`.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "xtask"]

````

Replace:

````toml
relay-abi = { path = "crates/relay-abi" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
relay-abi = { path = "crates/relay-abi" }
crc32 = { path = "crates/crc32" }
sysimg = { path = "crates/sysimg" }
uefi = { version = "0.41", default-features = false }
````

- [ ] **Step 2: Create `crates/crc32/Cargo.toml`**

Create `crates/crc32/Cargo.toml`:

````toml
[package]
name = "crc32"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
````

- [ ] **Step 3: Create `crates/crc32/src/lib.rs`**

Create `crates/crc32/src/lib.rs`:

````rust
//! CRC-32 (IEEE 802.3), as GPT (spec §6.5 of milestone 1) and the system
//! archive (spec §4.2 of the user-space gate) use it.
#![cfg_attr(not(test), no_std)]

/// The reflected IEEE 802.3 polynomial.
const POLY: u32 = 0xEDB8_8320;

/// The CRC of each byte value, built at compile time so it costs no stack.
static TABLE: [u32; 256] = table();

const fn table() -> [u32; 256] {
    let mut t = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
            bit += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

/// CRC-32 (IEEE 802.3, reflected, init and final xor 0xFFFFFFFF), as GPT and zlib use it.
pub fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &b| {
        TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn every_byte_value_counts() {
        // zlib's crc32 of the bytes 0..=255 in order, which reaches every
        // entry of the table.
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(crc32(&all), 0x2905_8C73);
    }
}
````

- [ ] **Step 4: Create `crates/sysimg/Cargo.toml`**

Create `crates/sysimg/Cargo.toml`:

````toml
[package]
name = "sysimg"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
crc32.workspace = true
````

- [ ] **Step 5: Write the failing tests for `crates/sysimg/src/format.rs`**

Create `crates/sysimg/src/format.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(name: &'a str, mode: u16, data: &'a [u8]) -> Entry<'a> {
        Entry {
            name: name.as_bytes(),
            mode,
            data,
        }
    }

    fn sample() -> Vec<u8> {
        let big = vec![0xAB; 5000];
        let big: &'static [u8] = Box::leak(big.into_boxed_slice());
        write(
            7,
            1_790_000_000,
            &[
                file("t-args", 0o755, big),
                file("empty", 0o644, b""),
                file("cat", 0o755, b"\x7fELF cat"),
            ],
        )
        .unwrap()
    }

    /// Recomputes the CRC after a test has changed the table or the data.
    fn fix_crc(b: &mut [u8]) {
        let crc = crc32::crc32(&b[HEADER_LEN..]);
        put_u32(b, H_CRC, crc);
    }

    fn entry_field(i: usize, field: usize) -> usize {
        HEADER_LEN + i * ENTRY_LEN + field
    }

    #[test]
    fn what_is_written_reads_back_sorted_by_name() {
        let bytes = sample();
        let a = Archive::parse(&bytes).unwrap();
        assert_eq!(a.abi(), 7);
        assert_eq!(a.build_time(), 1_790_000_000);
        assert_eq!(a.len(), 3);
        assert_eq!(a.total_len(), bytes.len() as u64);
        let names: Vec<&[u8]> = a.entries().map(|e| e.name).collect();
        assert_eq!(names, [&b"cat"[..], b"empty", b"t-args"]);
        assert_eq!(a.entry(0).unwrap().data, b"\x7fELF cat");
        assert_eq!(a.entry(0).unwrap().mode, 0o755);
        assert_eq!(a.entry(1).unwrap().data, b"");
        assert_eq!(a.entry(1).unwrap().mode, 0o644);
        assert_eq!(a.entry(2).unwrap().data, &[0xAB; 5000][..]);
        assert_eq!(a.entry(3), None);
        assert_eq!(a.find(b"t-args"), Some(2));
        assert_eq!(a.find(b"cat"), Some(0));
        assert_eq!(a.find(b"empty"), Some(1));
        assert_eq!(a.find(b"ls"), None);
        assert_eq!(a.find(b""), None);
    }

    #[test]
    fn the_layout_is_the_spec_s() {
        let b = sample();
        assert_eq!(&b[0..8], b"RELAYSYS");
        assert_eq!(u32_at(&b, 8), 1, "format version");
        assert_eq!(u32_at(&b, 12), 7, "ABI version");
        assert_eq!(u32_at(&b, 16), 3, "entry count");
        assert_eq!(u64_at(&b, 24), b.len() as u64, "total length");
        assert_eq!(
            u32_at(&b, 32),
            crc32::crc32(&b[64..]),
            "CRC of all after the header"
        );
        assert_eq!(u64_at(&b, 40), 1_790_000_000, "build time");
        assert!(
            b[20..24]
                .iter()
                .chain(&b[36..40])
                .chain(&b[48..64])
                .all(|&x| x == 0)
        );
        // Entry 0 ("cat") right after the header.
        assert_eq!(&b[64..67], b"cat");
        assert!(b[67..128].iter().all(|&x| x == 0), "name padded with zeros");
        assert_eq!(b[128], 3, "name length");
        assert_eq!(u16_at(&b, 130), 0o755, "mode");
        let offsets: Vec<u64> = (0..3)
            .map(|i| u64_at(&b, entry_field(i, E_OFFSET)))
            .collect();
        assert_eq!(
            offsets,
            [4096, 8192, 8192],
            "each on a 4 KiB boundary; empty takes none"
        );
        assert_eq!(u64_at(&b, entry_field(2, E_LEN)), 5000);
        assert_eq!(b.len(), 8192 + 5000, "no padding after the last data");
    }

    #[test]
    fn an_empty_archive_is_just_a_header() {
        let b = write(1, 0, &[]).unwrap();
        assert_eq!(b.len(), HEADER_LEN);
        let a = Archive::parse(&b).unwrap();
        assert!(a.is_empty());
        assert_eq!(a.entries().count(), 0);
    }

    #[test]
    fn the_writer_refuses_what_the_reader_would() {
        let long = "x".repeat(65);
        for name in ["", "a/b", "a\0b", ".", "..", long.as_str()] {
            assert_eq!(
                write(1, 0, &[file(name, 0o644, b"")]),
                Err(SysImgError::BadName(0)),
                "{name:?}"
            );
        }
        assert!(write(1, 0, &[file(&"x".repeat(64), 0o644, b"")]).is_ok());
        assert_eq!(
            write(1, 0, &[file("a", 0o644, b""), file("a", 0o755, b"x")]),
            Err(SysImgError::OutOfOrder(1))
        );
        assert_eq!(
            write(1, 0, &[file("a", 0o10644, b"")]),
            Err(SysImgError::BadMode(0))
        );
    }

    #[test]
    fn a_damaged_header_is_refused() {
        let good = sample();
        assert_eq!(
            Archive::parse(&good[..63]).err(),
            Some(SysImgError::TooShort(63))
        );
        assert_eq!(Archive::parse(&[]).err(), Some(SysImgError::TooShort(0)));
        let mut b = good.clone();
        b[0] = b'X';
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::BadMagic));
        let mut b = good.clone();
        put_u32(&mut b, H_FORMAT, 2);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::FormatVersion(2))
        );
        let n = good.len() as u64;
        assert_eq!(
            Archive::parse(&good[..good.len() - 1]).err(),
            Some(SysImgError::Length {
                header: n,
                actual: n - 1
            })
        );
        let mut b = good.clone();
        b.push(0);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::Length {
                header: n,
                actual: n + 1
            })
        );
        let mut b = good.clone();
        *b.last_mut().unwrap() ^= 1;
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::Crc));
        let mut b = good.clone();
        b[HEADER_LEN] ^= 1;
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::Crc),
            "the table is covered"
        );
        let mut b = good.clone();
        put_u32(&mut b, H_COUNT, 1000);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::TableOutside(1000))
        );
        let mut b = good.clone();
        put_u32(&mut b, H_COUNT, u32::MAX);
        assert_eq!(
            Archive::parse(&b).err(),
            Some(SysImgError::TableOutside(u32::MAX))
        );
    }

    #[test]
    fn a_damaged_entry_is_refused() {
        let good = sample();
        let damaged = |edit: &dyn Fn(&mut Vec<u8>)| {
            let mut b = good.clone();
            edit(&mut b);
            fix_crc(&mut b);
            Archive::parse(&b).err()
        };
        assert_eq!(
            damaged(&|b| b[entry_field(1, E_NAME_LEN)] = 0),
            Some(SysImgError::BadName(1))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(1, E_NAME_LEN)] = 65),
            Some(SysImgError::BadName(1))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME + 1)] = b'/'),
            Some(SysImgError::BadName(0))
        );
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME + 1)] = 0),
            Some(SysImgError::BadName(0))
        );
        // "cat" -> "fat" sorts after "empty".
        assert_eq!(
            damaged(&|b| b[entry_field(0, E_NAME)] = b'f'),
            Some(SysImgError::OutOfOrder(1))
        );
        // "empty" -> "cat\0\0" with length 3: a repeat.
        assert_eq!(
            damaged(&|b| {
                b[entry_field(1, E_NAME)..entry_field(1, E_NAME) + 3].copy_from_slice(b"cat");
                b[entry_field(1, E_NAME_LEN)] = 3;
            }),
            Some(SysImgError::OutOfOrder(1))
        );
        assert_eq!(
            damaged(&|b| put_u16(b, entry_field(2, E_MODE), 0o100755)),
            Some(SysImgError::BadMode(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(0, E_OFFSET), 4097)),
            Some(SysImgError::Misaligned(0))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(0, E_OFFSET), 0)),
            Some(SysImgError::DataOutside(0))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_LEN), 5001)),
            Some(SysImgError::DataOutside(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_LEN), u64::MAX)),
            Some(SysImgError::DataOutside(2))
        );
        assert_eq!(
            damaged(&|b| put_u64(b, entry_field(2, E_OFFSET), u64::MAX - 4095)),
            Some(SysImgError::DataOutside(2))
        );
    }

    /// With 232 empty files the table ends exactly at a 4 KiB boundary, so
    /// the archive ends with the table: a name length of 255 in the last
    /// entry points past the end.
    #[test]
    fn a_name_length_past_the_end_is_refused() {
        let names: Vec<String> = (0..232).map(|i| format!("f{i:03}")).collect();
        let files: Vec<Entry<'_>> = names.iter().map(|n| file(n, 0o644, b"")).collect();
        let mut b = write(1, 0, &files).unwrap();
        assert_eq!(
            b.len(),
            HEADER_LEN + 232 * ENTRY_LEN,
            "nothing after the table"
        );
        b[entry_field(231, E_NAME_LEN)] = 255;
        fix_crc(&mut b);
        assert_eq!(Archive::parse(&b).err(), Some(SysImgError::BadName(231)));
    }

    /// Seeded random damage to a valid archive, with the CRC fixed half of
    /// the time so the checks after it run too: `parse` never panics, and
    /// whatever it accepts reads without panicking.
    #[test]
    fn random_damage_never_panics() {
        let good = sample();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut accepted = 0;
        for _ in 0..20_000 {
            let mut b = good.clone();
            for _ in 0..1 + rnd() % 4 {
                // Mostly the header and the table, where the fields are.
                let at = if rnd() % 4 == 0 {
                    rnd() as usize % b.len()
                } else {
                    rnd() as usize % (HEADER_LEN + 3 * ENTRY_LEN)
                };
                b[at] = rnd() as u8;
            }
            if rnd() % 8 == 0 {
                b.truncate(rnd() as usize % b.len());
            }
            if b.len() >= HEADER_LEN && rnd() % 2 == 0 {
                let len = b.len() as u64;
                if rnd() % 2 == 0 {
                    put_u64(&mut b, H_LEN, len);
                }
                fix_crc(&mut b);
            }
            if let Ok(a) = Archive::parse(&b) {
                accepted += 1;
                for e in a.entries() {
                    assert!(valid_name(e.name));
                    assert_eq!(a.find(e.name).and_then(|i| a.entry(i)), Some(e));
                }
            }
        }
        assert!(
            accepted > 0,
            "some damage (a data byte with the CRC fixed) is harmless"
        );
    }

    #[test]
    fn errors_read_as_boot_messages() {
        assert_eq!(SysImgError::Crc.to_string(), "checksum mismatch");
        assert_eq!(SysImgError::BadMagic.to_string(), "not a system archive");
        assert_eq!(
            SysImgError::Length {
                header: 10,
                actual: 9
            }
            .to_string(),
            "header says 10 bytes, 9 were read"
        );
        assert_eq!(
            SysImgError::OutOfOrder(3).to_string(),
            "entry 3: name out of order or repeated"
        );
    }
}
````

- [ ] **Step 6: Create `crates/sysimg/src/lib.rs`**

Create `crates/sysimg/src/lib.rs`:

````rust
//! The system archive, `system.img` (spec §4 of the user-space gate): the
//! programs of `/bin` in one file on the ESP, which xtask writes and the
//! kernel reads in place after the loader has put it in memory.
//!
//! Everything here is `no_std + alloc` and has no hardware access, so it is
//! tested on the host and used unchanged by the kernel.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod format;

pub use format::{
    Archive, DATA_ALIGN, ENTRY_LEN, Entry, FORMAT_VERSION, HEADER_LEN, MAGIC, MODE_MASK, NAME_MAX,
    SysImgError, valid_name, write,
};
````

- [ ] **Step 7: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
heap.workspace = true
x86_64.workspace = true
````

with:

````toml
heap.workspace = true
crc32.workspace = true
x86_64.workspace = true
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p sysimg`

Expected: FAIL: compile errors such as `` cannot find type `Entry` in this scope ``; `` cannot find struct, variant or union type `Entry` in this scope ``.

- [ ] **Step 9: Implement `crates/sysimg/src/format.rs`**

Insert this at the top of `crates/sysimg/src/format.rs`, above `#[cfg(test)]`:

````rust
//! The archive format (spec §4.2): a 64-byte header, a table of 88-byte
//! entries sorted by name, then each entry's data on a 4 KiB boundary.
//! Little-endian throughout. The CRC-32 covers everything after the header.

use alloc::vec::Vec;
use core::fmt;

pub const MAGIC: [u8; 8] = *b"RELAYSYS";
pub const FORMAT_VERSION: u32 = 1;
pub const HEADER_LEN: usize = 64;
pub const ENTRY_LEN: usize = 88;
/// The longest entry name, in bytes.
pub const NAME_MAX: usize = 64;
/// Every entry's data starts at a multiple of this.
pub const DATA_ALIGN: usize = 4096;
/// Unix permission bits; a mode may have no others.
pub const MODE_MASK: u16 = 0o7777;

// Header fields (offsets). 20..24, 36..40 and 48..64 are zero.
const H_MAGIC: usize = 0;
const H_FORMAT: usize = 8;
const H_ABI: usize = 12;
const H_COUNT: usize = 16;
const H_LEN: usize = 24;
const H_CRC: usize = 32;
const H_TIME: usize = 40;

// Entry fields (offsets). 65 and 68..72 are zero.
const E_NAME: usize = 0;
const E_NAME_LEN: usize = 64;
const E_MODE: usize = 66;
const E_OFFSET: usize = 72;
const E_LEN: usize = 80;

/// One file of the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a [u8],
    /// Permission bits (`MODE_MASK`).
    pub mode: u16,
    pub data: &'a [u8],
}

/// Why an archive was refused. Entry numbers count from 0 in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysImgError {
    TooShort(usize),
    BadMagic,
    FormatVersion(u32),
    /// The header's length is not the length that was read.
    Length {
        header: u64,
        actual: u64,
    },
    Crc,
    /// The entry table does not fit in the archive.
    TableOutside(u32),
    BadName(usize),
    /// A name not greater than the one before it (so also a duplicate).
    OutOfOrder(usize),
    BadMode(usize),
    DataOutside(usize),
    Misaligned(usize),
}

impl fmt::Display for SysImgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysImgError::TooShort(n) => write!(f, "only {n} bytes"),
            SysImgError::BadMagic => write!(f, "not a system archive"),
            SysImgError::FormatVersion(v) => {
                write!(f, "format version {v}, expected {FORMAT_VERSION}")
            }
            SysImgError::Length { header, actual } => {
                write!(f, "header says {header} bytes, {actual} were read")
            }
            SysImgError::Crc => write!(f, "checksum mismatch"),
            SysImgError::TableOutside(n) => write!(f, "{n} entries do not fit"),
            SysImgError::BadName(i) => write!(f, "entry {i}: invalid name"),
            SysImgError::OutOfOrder(i) => write!(f, "entry {i}: name out of order or repeated"),
            SysImgError::BadMode(i) => write!(f, "entry {i}: invalid mode"),
            SysImgError::DataOutside(i) => write!(f, "entry {i}: data outside the archive"),
            SysImgError::Misaligned(i) => write!(f, "entry {i}: data not on a 4 KiB boundary"),
        }
    }
}

/// A name the archive can hold: 1 to `NAME_MAX` bytes, no `/`, no NUL, not
/// `.` or `..`.
pub fn valid_name(name: &[u8]) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && !name.contains(&b'/')
        && !name.contains(&0)
        && name != b"."
        && name != b".."
}

fn put_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    let mut v = [0; 4];
    v.copy_from_slice(&b[at..at + 4]);
    u32::from_le_bytes(v)
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    let mut v = [0; 8];
    v.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(v)
}

/// Builds an archive of `files` (in any order; the table is sorted by
/// name) for ABI `abi`, built at `build_time` (seconds since 1970).
pub fn write(abi: u32, build_time: u64, files: &[Entry<'_>]) -> Result<Vec<u8>, SysImgError> {
    let mut sorted: Vec<Entry<'_>> = files.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(b.name));
    for (i, e) in sorted.iter().enumerate() {
        if !valid_name(e.name) {
            return Err(SysImgError::BadName(i));
        }
        if i > 0 && sorted[i - 1].name == e.name {
            return Err(SysImgError::OutOfOrder(i));
        }
        if e.mode & !MODE_MASK != 0 {
            return Err(SysImgError::BadMode(i));
        }
    }
    let table_end = HEADER_LEN + sorted.len() * ENTRY_LEN;
    let mut out = alloc::vec![0u8; table_end];
    for (i, e) in sorted.iter().enumerate() {
        let offset = out.len().next_multiple_of(DATA_ALIGN);
        out.resize(offset, 0);
        out.extend_from_slice(e.data);
        let at = HEADER_LEN + i * ENTRY_LEN;
        out[at + E_NAME..at + E_NAME + e.name.len()].copy_from_slice(e.name);
        out[at + E_NAME_LEN] = e.name.len() as u8;
        put_u16(&mut out, at + E_MODE, e.mode);
        put_u64(&mut out, at + E_OFFSET, offset as u64);
        put_u64(&mut out, at + E_LEN, e.data.len() as u64);
    }
    out[H_MAGIC..H_MAGIC + 8].copy_from_slice(&MAGIC);
    put_u32(&mut out, H_FORMAT, FORMAT_VERSION);
    put_u32(&mut out, H_ABI, abi);
    put_u32(&mut out, H_COUNT, sorted.len() as u32);
    let len = out.len() as u64;
    put_u64(&mut out, H_LEN, len);
    put_u64(&mut out, H_TIME, build_time);
    let crc = crc32::crc32(&out[HEADER_LEN..]);
    put_u32(&mut out, H_CRC, crc);
    Ok(out)
}

/// A checked archive: every entry's name, mode and data range were
/// checked by `parse`, so reading it cannot fail.
#[derive(Clone, Copy, Debug)]
pub struct Archive<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Archive<'a> {
    /// Checks `bytes`, all of what was read, in this order: size, magic,
    /// format version, length, CRC, the table's size, then each entry.
    pub fn parse(bytes: &'a [u8]) -> Result<Archive<'a>, SysImgError> {
        if bytes.len() < HEADER_LEN {
            return Err(SysImgError::TooShort(bytes.len()));
        }
        if bytes[H_MAGIC..H_MAGIC + 8] != MAGIC {
            return Err(SysImgError::BadMagic);
        }
        let format = u32_at(bytes, H_FORMAT);
        if format != FORMAT_VERSION {
            return Err(SysImgError::FormatVersion(format));
        }
        let header = u64_at(bytes, H_LEN);
        let actual = bytes.len() as u64;
        if header != actual {
            return Err(SysImgError::Length { header, actual });
        }
        if crc32::crc32(&bytes[HEADER_LEN..]) != u32_at(bytes, H_CRC) {
            return Err(SysImgError::Crc);
        }
        let count = u32_at(bytes, H_COUNT);
        let table_end = (count as usize)
            .checked_mul(ENTRY_LEN)
            .and_then(|t| t.checked_add(HEADER_LEN))
            .filter(|&end| end <= bytes.len())
            .ok_or(SysImgError::TableOutside(count))?;
        let archive = Archive {
            bytes,
            count: count as usize,
        };
        let mut previous: &[u8] = &[];
        for i in 0..archive.count {
            let at = HEADER_LEN + i * ENTRY_LEN;
            let name_len = bytes[at + E_NAME_LEN] as usize;
            if name_len > NAME_MAX || !valid_name(&bytes[at..at + name_len]) {
                return Err(SysImgError::BadName(i));
            }
            let name = &bytes[at..at + name_len];
            if i > 0 && name <= previous {
                return Err(SysImgError::OutOfOrder(i));
            }
            previous = name;
            if u16_at(bytes, at + E_MODE) & !MODE_MASK != 0 {
                return Err(SysImgError::BadMode(i));
            }
            let offset = u64_at(bytes, at + E_OFFSET);
            let len = u64_at(bytes, at + E_LEN);
            if !offset.is_multiple_of(DATA_ALIGN as u64) {
                return Err(SysImgError::Misaligned(i));
            }
            match offset.checked_add(len) {
                Some(end) if offset >= table_end as u64 && end <= actual => {}
                _ => return Err(SysImgError::DataOutside(i)),
            }
        }
        Ok(archive)
    }

    pub fn abi(&self) -> u32 {
        u32_at(self.bytes, H_ABI)
    }

    /// Seconds since 1970, UTC, when xtask built the archive.
    pub fn build_time(&self) -> u64 {
        u64_at(self.bytes, H_TIME)
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The whole archive's size in bytes.
    pub fn total_len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Entry `i` in name order.
    pub fn entry(&self, i: usize) -> Option<Entry<'a>> {
        if i >= self.count {
            return None;
        }
        let b = self.bytes;
        let at = HEADER_LEN + i * ENTRY_LEN;
        // `parse` checked the name and the data range.
        let name = &b[at..at + b[at + E_NAME_LEN] as usize];
        let offset = u64_at(b, at + E_OFFSET) as usize;
        let len = u64_at(b, at + E_LEN) as usize;
        Some(Entry {
            name,
            mode: u16_at(b, at + E_MODE),
            data: &b[offset..offset + len],
        })
    }

    /// The index of the entry called `name`.
    pub fn find(&self, name: &[u8]) -> Option<usize> {
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let e = self.entry(mid)?;
            match e.name.cmp(name) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        (0..self.count).filter_map(|i| self.entry(i))
    }
}

````

- [ ] **Step 10: Delete `kernel/src/block/crc32.rs`**

Delete `kernel/src/block/crc32.rs` (it moved):

````bash
git rm -q kernel/src/block/crc32.rs
````

- [ ] **Step 11: Change `kernel/src/block/gpt.rs`**

In `kernel/src/block/gpt.rs`, replace:

````rust

use super::crc32::crc32;
use alloc::vec::Vec;
````

with:

````rust

use ::crc32::crc32;
use alloc::vec::Vec;
````

- [ ] **Step 12: Change `kernel/src/block/mod.rs`**

In `kernel/src/block/mod.rs`, replace:

````rust

pub mod crc32;
pub mod gpt;
````

with:

````rust

pub mod gpt;
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p sysimg`

Expected: PASS: 9 tests.

Run: `cargo test -p crc32`

Expected: PASS: 3 tests.

Run: `cargo test -p relay-kernel --lib gpt`

Expected: PASS: 37 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add Cargo.lock Cargo.toml crates kernel
git commit -m "sysimg: the system archive's format, with a writer and a reader that checks everything"
````


### Task 4: `SysImgFs`: the archive as a read-only filesystem

The kernel mounts the archive at `/bin` without copying it (spec §4.2): `SysImgFs` implements `vfs::FileSystem` directly over the loaded bytes. The root (inode 1) lists the entries (entry `i` is inode `i + 2`) and `.`/`..`; `stat` shows the entry's mode, its length and the archive's build time; every change is `EROFS`, after `ENOENT` for an inode not in use, as the `FileSystem` contract orders them. Besides its own tests it is held to a read-only `MemFs` holding the same files, 5,000 seeded operations with every result compared, errors included, the way milestone 1 holds ext2 to `MemFs`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `crates/sysimg/Cargo.toml`
- Create: `crates/sysimg/src/fs.rs`
- Modify: `crates/sysimg/src/lib.rs`

**Interfaces:**
- Consumes: Task 3 (`Archive`, `Entry`, `write`); `vfs::{FileSystem, MemFs}`.
- Produces: `sysimg::{SysImgFs, ROOT}`: `SysImgFs::new(&'static [u8]) -> Result<SysImgFs, SysImgError>`, `SysImgFs::archive() -> &Archive<'static>`.

- [ ] **Step 1: Change `crates/sysimg/Cargo.toml`**

In `crates/sysimg/Cargo.toml`, replace:

````toml
crc32.workspace = true
````

with:

````toml
crc32.workspace = true
vfs.workspace = true
````

- [ ] **Step 2: Write the failing tests for `crates/sysimg/src/fs.rs`**

Create `crates/sysimg/src/fs.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::write;
    use vfs::{Env, MemFs};

    const BUILT: u64 = 1_790_000_000;

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            BUILT
        }
        fn log(&self, _line: &str) {}
    }

    const FILES: [(&str, u16, &[u8]); 3] = [
        ("t-args", 0o755, b"\x7fELF arguments"),
        ("empty", 0o644, b""),
        ("cat", 0o700, b"\x7fELF cat"),
    ];

    fn fs() -> SysImgFs {
        let files: Vec<Entry<'_>> = FILES
            .iter()
            .map(|&(name, mode, data)| Entry {
                name: name.as_bytes(),
                mode,
                data,
            })
            .collect();
        let bytes = write(1, BUILT, &files).unwrap();
        SysImgFs::new(Box::leak(bytes.into_boxed_slice())).unwrap()
    }

    #[test]
    fn a_bad_archive_is_not_served() {
        let bytes: &'static [u8] = b"not an archive, too short";
        assert_eq!(SysImgFs::new(bytes).err(), Some(SysImgError::TooShort(25)));
    }

    #[test]
    fn the_root_lists_every_program() {
        let mut fs = fs();
        let mut names: Vec<Vec<u8>> = fs
            .read_dir(ROOT)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        assert_eq!(names, [&b"."[..], b"..", b"cat", b"empty", b"t-args"]);
        for e in fs.read_dir(ROOT).unwrap() {
            assert_eq!(fs.lookup(ROOT, &e.name), Ok(e.ino), "{:?}", e.name);
        }
        assert_eq!(fs.lookup(ROOT, b"ls"), Err(Errno::ENOENT));
        let cat = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.lookup(cat, b"x"), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(cat), Err(Errno::ENOTDIR));
        assert_eq!(fs.read_dir(99), Err(Errno::ENOENT));
    }

    #[test]
    fn stat_shows_modes_sizes_and_the_build_time() {
        let mut fs = fs();
        let root = fs.stat(ROOT).unwrap();
        assert_eq!(
            (root.kind, root.perm, root.nlink),
            (FileType::Directory, 0o755, 2)
        );
        let args = fs.lookup(ROOT, b"t-args").unwrap();
        let s = fs.stat(args).unwrap();
        assert_eq!(s.ino, args);
        assert_eq!((s.kind, s.perm, s.nlink), (FileType::Regular, 0o755, 1));
        assert_eq!((s.uid, s.gid), (0, 0));
        assert_eq!(s.size, 14);
        assert_eq!((s.blocks, s.block_size), (1, 4096));
        assert_eq!((s.atime, s.mtime, s.ctime), (BUILT, BUILT, BUILT));
        let cat = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.stat(cat).unwrap().perm, 0o700);
        for bad in [0, FIRST_FILE + 3, u64::MAX] {
            assert_eq!(fs.stat(bad), Err(Errno::ENOENT), "{bad}");
        }
    }

    #[test]
    fn files_read_like_any_other() {
        let mut fs = fs();
        let args = fs.lookup(ROOT, b"t-args").unwrap();
        let mut buf = [0u8; 64];
        assert_eq!(fs.read_at(args, 0, &mut buf), Ok(14));
        assert_eq!(&buf[..14], b"\x7fELF arguments");
        assert_eq!(fs.read_at(args, 5, &mut buf[..4]), Ok(4));
        assert_eq!(&buf[..4], b"argu");
        assert_eq!(fs.read_at(args, 14, &mut buf), Ok(0));
        assert_eq!(fs.read_at(args, 1 << 40, &mut buf), Ok(0));
        assert_eq!(fs.read_at(args, u64::MAX, &mut buf), Ok(0));
        assert_eq!(fs.read_at(ROOT, 0, &mut buf), Err(Errno::EISDIR));
        assert_eq!(fs.read_link(args), Err(Errno::EINVAL));
        assert_eq!(fs.read_link(77), Err(Errno::ENOENT));
    }

    #[test]
    fn every_change_is_refused() {
        let mut fs = fs();
        let f = fs.lookup(ROOT, b"cat").unwrap();
        assert_eq!(fs.write_at(f, 0, b"x"), Err(Errno::EROFS));
        assert_eq!(fs.truncate(f, 0), Err(Errno::EROFS));
        assert_eq!(fs.touch(f), Err(Errno::EROFS));
        assert_eq!(fs.create(ROOT, b"new"), Err(Errno::EROFS));
        assert_eq!(fs.mkdir(ROOT, b"d"), Err(Errno::EROFS));
        assert_eq!(fs.unlink(ROOT, b"cat"), Err(Errno::EROFS));
        assert_eq!(fs.rmdir(ROOT, b"cat"), Err(Errno::EROFS));
        assert_eq!(fs.rename(ROOT, b"cat", ROOT, b"dog"), Err(Errno::EROFS));
        assert_eq!(fs.touch(42), Err(Errno::ENOENT), "an unknown inode first");
        assert_eq!(fs.rename(ROOT, b"cat", 42, b"dog"), Err(Errno::ENOENT));
        assert_eq!(fs.sync(), Ok(()));
        assert_eq!(fs.shutdown(), Ok(()));
        let mut buf = [0u8; 8];
        assert_eq!(fs.read_at(f, 0, &mut buf), Ok(8), "still readable");
    }

    #[test]
    fn statfs_counts_the_archive_as_full() {
        let mut fs = fs();
        let blocks = fs.archive().total_len().div_ceil(4096);
        assert_eq!(
            fs.statfs(),
            Ok(StatFs {
                block_size: 4096,
                blocks,
                free_blocks: 0,
                avail_blocks: 0,
                files: 4,
                free_files: 0,
            })
        );
    }

    /// The same files in a read-only `MemFs`, operation by operation with
    /// seeded random inodes, names and offsets: every result must agree,
    /// errors included (inodes compared by what they name).
    #[test]
    fn it_answers_as_a_read_only_memfs_does() {
        let mut ours = fs();
        let mut mem = MemFs::new(Box::new(Clock));
        for (name, _, data) in FILES {
            let ino = mem.create(mem.root(), name.as_bytes()).unwrap();
            mem.write_at(ino, 0, data).unwrap();
        }
        let mut mem = mem.read_only();
        let names: [&[u8]; 7] = [b"t-args", b"empty", b"cat", b"ls", b".", b"..", b"x/y"];
        let ino_of = |fs: &mut dyn FileSystem, k: usize| -> Ino {
            match k {
                0..=2 => fs.lookup(fs.root(), names[k]).unwrap(),
                3 => fs.root(),
                _ => 1000 + k as Ino,
            }
        };
        let ours_inos: Vec<Ino> = (0..5).map(|k| ino_of(&mut ours, k)).collect();
        let mem_inos: Vec<Ino> = (0..5).map(|k| ino_of(&mut mem, k)).collect();
        // An inode result as the index of what it names.
        let index =
            |inos: &[Ino], r: Result<Ino, Errno>| r.map(|i| inos.iter().position(|&x| x == i));
        let mut seed = 0x0123_4567_89AB_CDEFu64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..5_000 {
            let k = rnd() as usize % 5;
            let k2 = rnd() as usize % 5;
            let name = names[rnd() as usize % names.len()];
            let offset = [0, 3, 14, 15, 16, 4096, u64::MAX][rnd() as usize % 7];
            let len = rnd() as usize % 20;
            let (a, b) = (ours_inos[k], mem_inos[k]);
            let (a2, b2) = (ours_inos[k2], mem_inos[k2]);
            match rnd() % 12 {
                0 => assert_eq!(
                    index(&ours_inos, ours.lookup(a, name)),
                    index(&mem_inos, mem.lookup(b, name)),
                    "lookup {k} {name:?}"
                ),
                1 => {
                    let mut x = vec![0; len];
                    let mut y = vec![0; len];
                    assert_eq!(
                        ours.read_at(a, offset, &mut x),
                        mem.read_at(b, offset, &mut y),
                        "read {k}"
                    );
                    assert_eq!(x, y);
                }
                2 => {
                    let names_of = |fs: &mut dyn FileSystem, ino| {
                        fs.read_dir(ino).map(|v| {
                            let mut n: Vec<Vec<u8>> = v.into_iter().map(|e| e.name).collect();
                            n.sort();
                            n
                        })
                    };
                    assert_eq!(
                        names_of(&mut ours, a),
                        names_of(&mut mem, b),
                        "read_dir {k}"
                    );
                }
                3 => assert_eq!(
                    ours.stat(a).map(|s| (s.kind, s.size)),
                    mem.stat(b).map(|s| (s.kind, s.size)),
                    "stat {k}"
                ),
                4 => assert_eq!(ours.read_link(a), mem.read_link(b), "read_link {k}"),
                5 => assert_eq!(
                    ours.write_at(a, offset, b"zz"),
                    mem.write_at(b, offset, b"zz"),
                    "write {k}"
                ),
                6 => assert_eq!(
                    ours.truncate(a, offset),
                    mem.truncate(b, offset),
                    "truncate {k}"
                ),
                7 => assert_eq!(ours.touch(a), mem.touch(b), "touch {k}"),
                8 => assert_eq!(
                    index(&ours_inos, ours.create(a, name)),
                    index(&mem_inos, mem.create(b, name)),
                    "create {k} {name:?}"
                ),
                9 => assert_eq!(
                    ours.unlink(a, name),
                    mem.unlink(b, name),
                    "unlink {k} {name:?}"
                ),
                10 => assert_eq!(
                    ours.rmdir(a, name),
                    mem.rmdir(b, name),
                    "rmdir {k} {name:?}"
                ),
                _ => assert_eq!(
                    ours.rename(a, name, a2, b"n"),
                    mem.rename(b, name, b2, b"n"),
                    "rename {k} {name:?} {k2}"
                ),
            }
        }
    }
}
````

- [ ] **Step 3: Declare the new module in `crates/sysimg/src/lib.rs`**

In `crates/sysimg/src/lib.rs`, replace:

````rust
mod format;

````

with:

````rust
mod format;
mod fs;

````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p sysimg`

Expected: FAIL: compile errors such as `` cannot find type `SysImgFs` in this scope ``; `` cannot find type `Entry` in this scope ``.

- [ ] **Step 5: Implement `crates/sysimg/src/fs.rs`**

Insert this at the top of `crates/sysimg/src/fs.rs`, above `#[cfg(test)]`:

````rust
//! `SysImgFs`: the archive as a read-only filesystem (spec §4.2), read in
//! place without copying. The root directory lists the entries; every
//! change is `EROFS`; the file times are the archive's build time.

use crate::format::{Archive, Entry, SysImgError};
use alloc::vec::Vec;
use vfs::{DirEntry, Errno, FileSystem, FileType, Ino, Stat, StatFs};

/// The root directory's inode; entry `i` is inode `FIRST_FILE + i`.
pub const ROOT: Ino = 1;
const FIRST_FILE: Ino = 2;
/// The block size `stat` and `statfs` report: the data alignment.
const BLOCK: u32 = 4096;

pub struct SysImgFs {
    archive: Archive<'static>,
}

enum Node {
    Root,
    File(Entry<'static>),
}

impl SysImgFs {
    /// Checks `bytes` (see `Archive::parse`) and serves them.
    pub fn new(bytes: &'static [u8]) -> Result<SysImgFs, SysImgError> {
        Ok(SysImgFs {
            archive: Archive::parse(bytes)?,
        })
    }

    pub fn archive(&self) -> &Archive<'static> {
        &self.archive
    }

    fn node(&self, ino: Ino) -> Result<Node, Errno> {
        if ino == ROOT {
            return Ok(Node::Root);
        }
        let i = ino
            .checked_sub(FIRST_FILE)
            .and_then(|i| usize::try_from(i).ok())
            .ok_or(Errno::ENOENT)?;
        let e = self.archive.entry(i).ok_or(Errno::ENOENT)?;
        Ok(Node::File(e))
    }

    fn file_ino(i: usize) -> Ino {
        FIRST_FILE + i as Ino
    }

    /// Every change: `ENOENT` for an inode not in use, `EROFS` otherwise
    /// (the order of the `FileSystem` contract).
    fn refuse(&self, inos: &[Ino]) -> Errno {
        match inos.iter().find(|&&i| self.node(i).is_err()) {
            Some(_) => Errno::ENOENT,
            None => Errno::EROFS,
        }
    }
}

impl FileSystem for SysImgFs {
    fn root(&self) -> Ino {
        ROOT
    }

    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        let t = self.archive.build_time();
        let (kind, perm, nlink, size) = match self.node(ino)? {
            Node::Root => (FileType::Directory, 0o755, 2, u64::from(BLOCK)),
            Node::File(e) => (FileType::Regular, e.mode, 1, e.data.len() as u64),
        };
        Ok(Stat {
            ino,
            kind,
            perm,
            nlink,
            uid: 0,
            gid: 0,
            size,
            blocks: size.div_ceil(512),
            block_size: BLOCK,
            atime: t,
            mtime: t,
            ctime: t,
        })
    }

    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        if let Node::File(_) = self.node(dir)? {
            return Err(Errno::ENOTDIR);
        }
        if name == b"." || name == b".." {
            return Ok(ROOT);
        }
        self.archive
            .find(name)
            .map(Self::file_ino)
            .ok_or(Errno::ENOENT)
    }

    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        if let Node::File(_) = self.node(dir)? {
            return Err(Errno::ENOTDIR);
        }
        let mut out = alloc::vec![
            DirEntry {
                name: b".".to_vec(),
                ino: ROOT,
            },
            DirEntry {
                name: b"..".to_vec(),
                ino: ROOT,
            },
        ];
        out.extend(self.archive.entries().enumerate().map(|(i, e)| DirEntry {
            name: e.name.to_vec(),
            ino: Self::file_ino(i),
        }));
        Ok(out)
    }

    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.node(ino)?;
        Err(Errno::EINVAL)
    }

    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        let data = match self.node(ino)? {
            Node::Root => return Err(Errno::EISDIR),
            Node::File(e) => e.data,
        };
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        let rest = data.get(start..).unwrap_or(&[]);
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }

    fn write_at(&mut self, ino: Ino, _offset: u64, _buf: &[u8]) -> Result<usize, Errno> {
        Err(self.refuse(&[ino]))
    }

    fn truncate(&mut self, ino: Ino, _size: u64) -> Result<(), Errno> {
        Err(self.refuse(&[ino]))
    }

    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        Err(self.refuse(&[ino]))
    }

    fn create(&mut self, dir: Ino, _name: &[u8]) -> Result<Ino, Errno> {
        Err(self.refuse(&[dir]))
    }

    fn mkdir(&mut self, dir: Ino, _name: &[u8]) -> Result<Ino, Errno> {
        Err(self.refuse(&[dir]))
    }

    fn unlink(&mut self, dir: Ino, _name: &[u8]) -> Result<(), Errno> {
        Err(self.refuse(&[dir]))
    }

    fn rmdir(&mut self, dir: Ino, _name: &[u8]) -> Result<(), Errno> {
        Err(self.refuse(&[dir]))
    }

    fn rename(
        &mut self,
        from_dir: Ino,
        _from: &[u8],
        to_dir: Ino,
        _to: &[u8],
    ) -> Result<(), Errno> {
        Err(self.refuse(&[from_dir, to_dir]))
    }

    fn statfs(&mut self) -> Result<StatFs, Errno> {
        let files = self.archive.len() as u64 + 1;
        Ok(StatFs {
            block_size: u64::from(BLOCK),
            blocks: self.archive.total_len().div_ceil(u64::from(BLOCK)),
            free_blocks: 0,
            avail_blocks: 0,
            files,
            free_files: 0,
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

- [ ] **Step 6: Change `crates/sysimg/src/lib.rs`**

In `crates/sysimg/src/lib.rs`, replace:

````rust
};
````

with:

````rust
};
pub use fs::{ROOT, SysImgFs};
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p sysimg`

Expected: PASS: 16 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add Cargo.lock crates
git commit -m "sysimg: SysImgFs serves the archive as a read-only filesystem"
````


### Task 5: Mounting on a name its directory does not have

Spec §4.4: `/bin` must mount even where the root has none: the read-only `MemFs` the kernel falls back to without a usable root (M1 §10), or a root made elsewhere. `MountTable::mount` now accepts a path whose last name is missing from an existing directory. The mount on a name shows up in that directory's `read_dir`, resolves like any name (`..` from it goes back to the directory), and behaves as a mount point does on Linux (spec §4.4): `create` and `mkdir` of the name are `EEXIST`, `unlink` is `EISDIR`, `rmdir` and `rename` from or to it are `EBUSY`, so the filesystem below never gets the name through the mount table, the only way the shell reaches files. A missing name further up is still `ENOENT`.

**Files:**
- Modify: `crates/vfs/src/mount.rs`

**Interfaces:**
- Consumes: milestone 1's `vfs::MountTable`.
- Produces: no new interface; `MountTable::mount` also mounts on a missing last name.

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, replace:

````rust
    }

    #[test]
    fn shutdown_reaches_every_filesystem() {
````

with:

````rust
    }

    fn programs() -> MemFs {
        let mut fs = memfs();
        let r = fs.root();
        let f = fs.create(r, b"prog").unwrap();
        fs.write_at(f, 0, b"program").unwrap();
        fs.read_only()
    }

    fn names_in(t: &mut MountTable, path: &[u8]) -> Vec<Vec<u8>> {
        let node = t.lookup(path).unwrap();
        let mut names: Vec<Vec<u8>> = t
            .read_dir(node)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_filesystem_mounts_on_a_name_its_directory_does_not_have() {
        let mut t = table();
        t.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(read(&mut t, b"/bin/prog").unwrap(), b"program");
        assert_eq!(t.lookup(b"/bin/prog").unwrap().mount, 1);
        let bin = t.lookup(b"/bin/").unwrap();
        assert_eq!(t.stat(bin).unwrap().kind, FileType::Directory);
        assert_eq!(
            names_in(&mut t, b"/"),
            [&b"."[..], b"..", b"bin", b"etc", b"root", b"tmp"],
            "listed once in its directory"
        );
        assert_eq!(
            names_in(&mut t, b"/etc"),
            [&b"."[..], b"..", b"motd"],
            "and nowhere else"
        );
        t.chdir(b"/bin").unwrap();
        assert_eq!(t.cwd(), b"/bin");
        assert_eq!(t.lookup(b".").unwrap().mount, 1);
        assert_eq!(read(&mut t, b"prog").unwrap(), b"program");
        assert_eq!(read(&mut t, b"../etc/motd").unwrap(), b"welcome\n");
        assert_eq!(read(&mut t, b"/bin/../bin/./prog").unwrap(), b"program");
        assert!(t.statfs(b"/bin").is_ok());
    }

    #[test]
    fn a_mount_on_a_name_is_a_mount_point_like_any_other() {
        let mut t = table();
        t.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(t.rmdir(b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rmdir(b"/bin/"), Err(Errno::EBUSY));
        assert_eq!(t.unlink(b"/bin"), Err(Errno::EISDIR));
        assert_eq!(t.mkdir(b"/bin"), Err(Errno::EEXIST));
        assert_eq!(t.create(b"/bin"), Err(Errno::EEXIST));
        assert_eq!(t.rename(b"/bin", b"/bin2"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/etc/motd", b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/tmp", b"/bin"), Err(Errno::EBUSY));
        assert_eq!(t.rename(b"/etc/motd", b"/bin/motd"), Err(Errno::EXDEV));
        assert_eq!(t.create(b"/bin/new"), Err(Errno::EROFS));
        assert_eq!(t.mount(b"/bin", Box::new(memfs())), Err(Errno::EBUSY));
        // The filesystem below never got the name.
        assert_eq!(names_in(&mut t, b"/root"), [&b"."[..], b"..", b"link"]);
        t.mkdir(b"/tmp/bin").unwrap();
        assert_eq!(
            names_in(&mut t, b"/tmp"),
            [&b"."[..], b"..", b"bin"],
            "other directories keep the name free"
        );
    }

    #[test]
    fn only_the_last_name_of_a_mount_point_may_be_missing() {
        let mut t = table();
        assert_eq!(
            t.mount(b"/missing/bin", Box::new(memfs())),
            Err(Errno::ENOENT)
        );
        assert_eq!(
            t.mount(b"/etc/motd/bin", Box::new(memfs())),
            Err(Errno::ENOTDIR)
        );
        assert_eq!(
            t.mount(b"/root/link/bin", Box::new(memfs())),
            Err(Errno::ENOTDIR)
        );
        assert_eq!(
            t.mount(b"/tmp/..", Box::new(memfs())),
            Err(Errno::EBUSY),
            "that is /"
        );
        // A read-only root (the fallbacks of M1 §10) gets one too.
        let mut ro = MountTable::new(Box::new(memfs().read_only()));
        ro.mount(b"/bin", Box::new(programs())).unwrap();
        assert_eq!(read(&mut ro, b"/bin/prog").unwrap(), b"program");
    }

    #[test]
    fn shutdown_reaches_every_filesystem() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: 3 tests fail, among them `mount::tests::a_mount_on_a_name_is_a_mount_point_like_any_other`, `mount::tests::a_filesystem_mounts_on_a_name_its_directory_does_not_have`.

- [ ] **Step 3: Change `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
    fs: Box<dyn FileSystem>,
    /// The directory it is mounted on; `None` for `/`.
    on: Option<Node>,
}
````

with:

````rust
    fs: Box<dyn FileSystem>,
    /// Where it is mounted; `None` for `/`.
    on: Option<MountPoint>,
}

/// A directory of the filesystem below, or a name that directory does not
/// have (spec §4.4 of the user-space gate: `/bin` on a root without one).
/// A mount on a name is shown in its directory like any other, and the
/// name behaves as a mount point does: it cannot be created, removed or
/// renamed through the mount table, the only way the shell reaches files.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MountPoint {
    Dir(Node),
    Name(Node, Vec<u8>),
}
````

Replace:

````rust

    /// Mounts `fs` on the directory at `path`. `EBUSY` if something is
    /// mounted there already.
    pub fn mount(&mut self, path: &[u8], fs: Box<dyn FileSystem>) -> Result<(), Errno> {
        let trail = self.walk(path)?;
        let on = trail.last().expect("never empty").0;
        if self.stat(on)?.kind != FileType::Directory {
            return Err(Errno::ENOTDIR);
        }
        if on.ino == self.fs(on.mount)?.root() || self.mounted_on(on).is_some() {
            return Err(Errno::EBUSY);
        }
        self.mounts.push(Mount { fs, on: Some(on) });
````

with:

````rust

    /// Mounts `fs` on the directory at `path`, or on its last name if the
    /// directory holding it has no such name. `EBUSY` if something is
    /// mounted there already.
    pub fn mount(&mut self, path: &[u8], fs: Box<dyn FileSystem>) -> Result<(), Errno> {
        let on = match self.walk(path) {
            Ok(trail) => {
                let on = trail.last().expect("never empty").0;
                if self.stat(on)?.kind != FileType::Directory {
                    return Err(Errno::ENOTDIR);
                }
                if on.ino == self.fs(on.mount)?.root() || self.mounted_on(on).is_some() {
                    return Err(Errno::EBUSY);
                }
                MountPoint::Dir(on)
            }
            Err(Errno::ENOENT) => {
                // Only the last name may be missing.
                let parent = self.parent(path)?;
                let name = parent.name.ok_or(Errno::ENOENT)?;
                MountPoint::Name(parent.dir, name)
            }
            Err(e) => return Err(e),
        };
        self.mounts.push(Mount { fs, on: Some(on) });
````

Replace:

````rust
    fn mounted_on(&self, node: Node) -> Option<usize> {
        self.mounts.iter().position(|m| m.on == Some(node))
    }
````

with:

````rust
    fn mounted_on(&self, node: Node) -> Option<usize> {
        self.mounts
            .iter()
            .position(|m| m.on == Some(MountPoint::Dir(node)))
    }

    /// The mount on the name `name` in `dir`, which `dir` does not have.
    fn mounted_at_name(&self, dir: Node, name: &[u8]) -> Option<usize> {
        self.mounts.iter().position(|m| match &m.on {
            Some(MountPoint::Name(d, n)) => *d == dir && n == name,
            _ => false,
        })
    }

    /// The root of mount `m`.
    fn mount_root(&self, m: usize) -> Node {
        Node {
            mount: m,
            ino: self.mounts[m].fs.root(),
        }
    }

    /// Something is mounted on `node`, or it is a mounted filesystem's root
    /// (what `child` gives for a mount on a name).
    fn is_mount_point(&self, node: Node) -> bool {
        self.mounted_on(node).is_some() || (node.mount != 0 && node == self.mount_root(node.mount))
    }
````

Replace:

````rust
            Component::Name(name) => {
                let ino = self.fs(here.mount)?.lookup(here.ino, name)?;
````

with:

````rust
            Component::Name(name) => {
                if let Some(m) = self.mounted_at_name(here, name) {
                    trail.push((self.mount_root(m), name.to_vec()));
                    return Ok(());
                }
                let ino = self.fs(here.mount)?.lookup(here.ino, name)?;
````

Replace:

````rust

    /// The node `parent`'s name refers to, if any.
    fn child(&mut self, parent: &Parent) -> Result<Option<Node>, Errno> {
        let Some(name) = &parent.name else {
            return Ok(None);
        };
        match self.fs(parent.dir.mount)?.lookup(parent.dir.ino, name) {
````

with:

````rust

    /// The node `parent`'s name refers to, if any: for a mount on a name,
    /// the mounted root.
    fn child(&mut self, parent: &Parent) -> Result<Option<Node>, Errno> {
        let Some(name) = &parent.name else {
            return Ok(None);
        };
        if let Some(m) = self.mounted_at_name(parent.dir, name) {
            return Ok(Some(self.mount_root(m)));
        }
        match self.fs(parent.dir.mount)?.lookup(parent.dir.ino, name) {
````

Replace:

````rust
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        self.fs(node.mount)?.read_dir(node.ino)
    }
````

with:

````rust
    fn read_dir(&mut self, node: Node) -> Result<Vec<DirEntry>, Errno> {
        let mut entries = self.fs(node.mount)?.read_dir(node.ino)?;
        for (m, mount) in self.mounts.iter().enumerate() {
            if let Some(MountPoint::Name(dir, name)) = &mount.on
                && *dir == node
            {
                entries.push(DirEntry {
                    name: name.clone(),
                    ino: self.mounts[m].fs.root(),
                });
            }
        }
        Ok(entries)
    }
````

Replace:

````rust
        }
        let ino = self.fs(parent.dir.mount)?.create(parent.dir.ino, name)?;
````

with:

````rust
        }
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EEXIST);
        }
        let ino = self.fs(parent.dir.mount)?.create(parent.dir.ino, name)?;
````

Replace:

````rust
        };
        self.fs(parent.dir.mount)?.mkdir(parent.dir.ino, name)?;
````

with:

````rust
        };
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EEXIST);
        }
        self.fs(parent.dir.mount)?.mkdir(parent.dir.ino, name)?;
````

Replace:

````rust
        };
        if parent.trailing_slash
````

with:

````rust
        };
        if self.mounted_at_name(parent.dir, name).is_some() {
            return Err(Errno::EISDIR);
        }
        if parent.trailing_slash
````

Replace:

````rust
        let child = self.child(&parent)?;
        if child.is_some_and(|c| self.mounted_on(c).is_some()) {
            return Err(Errno::EBUSY);
````

with:

````rust
        let child = self.child(&parent)?;
        if child.is_some_and(|c| self.is_mount_point(c)) {
            return Err(Errno::EBUSY);
````

Replace:

````rust
        }
        if self.mounted_on(moving).is_some() {
            return Err(Errno::EBUSY);
        }
        let target = self.child(&dst)?;
        if target.is_some_and(|t| self.mounted_on(t).is_some()) {
            return Err(Errno::EBUSY);
````

with:

````rust
        }
        if self.is_mount_point(moving) {
            return Err(Errno::EBUSY);
        }
        let target = self.child(&dst)?;
        if target.is_some_and(|t| self.is_mount_point(t)) {
            return Err(Errno::EBUSY);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 45 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -m "vfs: a filesystem mounts on a name its directory does not have"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 20 scenario(s) passed`.

````bash
git push -u origin m2p1/sysimg
gh pr create --base main --head m2p1/sysimg --title "Milestone 2, plan 1: The system archive and /bin" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 3–5: `crates/sysimg` writes and checks `system.img` (spec §4.2), never panicking on a damaged one, and serves it as the read-only `SysImgFs`, held to a read-only `MemFs`; CRC-32 moves into `crates/crc32`; `MountTable` mounts on a name its directory does not have (spec §4.4).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p1/sysimg --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-sysimg
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: User programs and their build (Tasks 6–10)

The runtime, the first program, and xtask building, checking and packing programs into `system.img` on the ESP.

Branch `m2p1/userland`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-userland`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p1/userland /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-userland origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-userland
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p1/sysimg` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p1/userland /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-userland m2p1/sysimg`), and after its merge rebase with `git rebase --onto origin/main <old tip of m2p1/sysimg>` and re-run `cargo xtask ci` before pushing.

### Task 6: `relay-rt`: the runtime of user programs

Every program links `relay-rt` (spec §8.1). Plan 1 gives it what a program needs to be built and, from plan 2 on, to run: `_start`, which aligns the stack and calls `start` with the arguments' address, length and count as the kernel passes them in `rdi`, `rsi`, `rdx` (spec §5.3); `Args`, the NUL-separated arguments (argument 0 is the path, `name()` its last part); `main!`, which names the program's `fn main(Args) -> u8`; the `syscall` stub (`rax`, `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`; spec §7.1) and the wrappers `write`, `write_all`, `exit` and the `Fd` writer; a panic handler that prints `<name>: panicked at <file>:<line>:<column>: <message>` and exits with 101; the ELF note (owner `Relay`, type 1, `VERSION`) in section `.note.relay`; and `user.ld`, the linker script that puts programs at `0x40_0000` with one `PT_LOAD` per permission set and the note in a `PT_NOTE`, which programs get through the `links` key (`DEP_RELAY_RT_SCRIPT`). What only exists on Relay OS (`_start`, `start`, the panic handler, the note's static, the `syscall` instruction) is built only for `target_os = "none"`; the arguments, the note's layout and the panic message are host-tested.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/relay-rt/Cargo.toml`
- Create: `crates/relay-rt/build.rs`
- Create: `crates/relay-rt/src/arch.rs`
- Create: `crates/relay-rt/src/args.rs`
- Create: `crates/relay-rt/src/lib.rs`
- Create: `crates/relay-rt/src/note.rs`
- Create: `crates/relay-rt/src/start.rs`
- Create: `crates/relay-rt/src/sys.rs`
- Create: `crates/relay-rt/user.ld`

**Interfaces:**
- Consumes: Task 2 (`Call`, `decode`, `VERSION`, `NOTE_NAME`, `NOTE_TYPE`, `errno::EIO`).
- Produces: crate `relay-rt` (`relay_rt`): `Args::{new, iter, get, len, is_empty, name}`, `main!`, `name() -> &'static [u8]`, `sys::{write(u32, &[u8]) -> Result<usize, u16>, write_all, exit(u8) -> !, Fd}`; `links = "relay_rt"` with the metadata `script` (the path of `crates/relay-rt/user.ld`).

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 3 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]

````

Replace:

````toml
sysimg = { path = "crates/sysimg" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
sysimg = { path = "crates/sysimg" }
relay-rt = { path = "crates/relay-rt" }
uefi = { version = "0.41", default-features = false }
````

Replace:

````toml
overflow-checks = true
````

with:

````toml
overflow-checks = true

````

- [ ] **Step 2: Create `crates/relay-rt/Cargo.toml`**

Create `crates/relay-rt/Cargo.toml`:

````toml
[package]
name = "relay-rt"
version.workspace = true
edition.workspace = true
license.workspace = true
# Hands the user linker script to the programs' build scripts
# (DEP_RELAY_RT_SCRIPT).
links = "relay_rt"
build = "build.rs"

[dependencies]
relay-abi.workspace = true
````

- [ ] **Step 3: Create `crates/relay-rt/build.rs`**

Create `crates/relay-rt/build.rs`:

````rust
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Every program's build script passes this to the linker (see
    // `userland/tests/build.rs`).
    println!("cargo:script={dir}/user.ld");
    println!("cargo:rerun-if-changed=user.ld");
}
````

- [ ] **Step 4: Write the failing tests for `crates/relay-rt/src/args.rs`**

Create `crates/relay-rt/src/args.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn all(a: Args) -> Vec<&'static [u8]> {
        a.iter().collect()
    }

    #[test]
    fn each_argument_ends_with_a_nul() {
        let a = Args::new(b"/bin/t-args\0a\0b c\0\0", 4);
        assert_eq!(all(a), [&b"/bin/t-args"[..], b"a", b"b c", b""]);
        assert_eq!(a.len(), 4);
        assert_eq!(a.get(2), Some(&b"b c"[..]));
        assert_eq!(a.get(4), None);
    }

    #[test]
    fn the_count_and_the_last_nul_bound_the_list() {
        let bytes = b"one\0two\0three\0";
        assert_eq!(all(Args::new(bytes, 2)), [&b"one"[..], b"two"]);
        assert_eq!(all(Args::new(bytes, 9)).len(), 3, "no more than there are");
        assert_eq!(
            all(Args::new(b"one\0partial", 2)),
            [&b"one"[..]],
            "no NUL, no argument"
        );
        assert!(Args::new(b"", 0).is_empty());
        assert!(Args::new(b"x\0", 0).is_empty());
    }

    #[test]
    fn the_name_is_the_last_part_of_the_path() {
        assert_eq!(Args::new(b"/bin/t-args\0", 1).name(), b"t-args");
        assert_eq!(Args::new(b"t-args\0x\0", 2).name(), b"t-args");
        assert_eq!(Args::new(b"./a/b/\0", 1).name(), b"");
        assert_eq!(Args::new(b"", 0).name(), b"");
    }
}
````

- [ ] **Step 5: Create `crates/relay-rt/src/lib.rs`**

Create `crates/relay-rt/src/lib.rs`:

````rust
//! The runtime every Relay OS program links (spec §8.1 of the user-space
//! gate): the entry point, the arguments, system-call wrappers, the panic
//! handler and the ELF note that names the ABI.
//!
//! A program is a `#![no_std]`, `#![no_main]` binary that names its main
//! function with [`main!`]:
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! relay_rt::main!(main);
//! fn main(args: relay_rt::Args) -> u8 { 0 }
//! ```
//!
//! The architecture-specific parts (the system-call instruction and
//! `_start`) are in `arch`, built only for Relay OS; the rest is host-tested.
#![cfg_attr(not(test), no_std)]

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
mod arch;
mod args;
// Off Relay OS only the tests use these.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod note;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod start;
pub mod sys;

pub use args::Args;
pub use start::name;

/// Names the program's `fn main(args: Args) -> u8`; its result is the exit
/// status.
#[macro_export]
macro_rules! main {
    ($main:path) => {
        #[unsafe(no_mangle)]
        extern "Rust" fn __relay_main(args: $crate::Args) -> u8 {
            $main(args)
        }
    };
}
````

- [ ] **Step 6: Write the failing tests for `crates/relay-rt/src/note.rs`**

Create `crates/relay-rt/src/note.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_is_laid_out_as_elf_says() {
        let n = abi_note();
        assert_eq!(core::mem::size_of::<Note>(), 24);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&n.namesz.to_le_bytes());
        bytes.extend_from_slice(&n.descsz.to_le_bytes());
        bytes.extend_from_slice(&n.kind.to_le_bytes());
        bytes.extend_from_slice(&n.name);
        bytes.extend_from_slice(&n.desc.to_le_bytes());
        let mut want = vec![6, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0];
        want.extend_from_slice(b"Relay\0\0\0");
        want.extend_from_slice(&VERSION.to_le_bytes());
        assert_eq!(bytes, want);
    }
}
````

- [ ] **Step 7: Write the failing tests for `crates/relay-rt/src/start.rs`**

Create `crates/relay-rt/src/start.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_reads_as_one_line_naming_the_program() {
        let mut s = String::new();
        panic_message(&mut s, b"t-args", Some(("src/main.rs", 7, 5)), "boom").unwrap();
        assert_eq!(s, "t-args: panicked at src/main.rs:7:5: boom\n");
        let mut s = String::new();
        panic_message(&mut s, b"a\xffb", None, 42).unwrap();
        assert_eq!(s, "a\u{fffd}b: panicked: 42\n");
    }

    #[test]
    fn the_name_is_empty_until_set() {
        assert_eq!(name(), b"");
        set_name(b"prog");
        assert_eq!(name(), b"prog");
    }
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find type `Args` in this scope ``; `` cannot find type `Note` in this scope ``.

- [ ] **Step 9: Create `crates/relay-rt/src/arch.rs`**

Create `crates/relay-rt/src/arch.rs`:

````rust
//! x86_64 (spec §7.1): the call number in `rax`, arguments in `rdi`, `rsi`,
//! `rdx`, `r10`, `r8`, `r9`, the result in `rax`; `syscall` itself
//! clobbers `rcx` and `r11`. And `_start`, which the kernel jumps to with
//! the arguments' address, length and count in `rdi`, `rsi`, `rdx`.

use core::arch::{asm, naked_asm};
use relay_abi::Call;

/// # Safety
/// The arguments must be what `call` expects (pointers to memory the
/// program owns, of the stated lengths).
pub unsafe fn syscall(call: Call, a: [u64; 6]) -> u64 {
    let result: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") call.number() => result,
            in("rdi") a[0],
            in("rsi") a[1],
            in("rdx") a[2],
            in("r10") a[3],
            in("r8") a[4],
            in("r9") a[5],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

/// The program's first instruction. The stack is 16-byte aligned at entry
/// (spec §5.3); a call leaves it as the SysV ABI expects on function
/// entry. `rdi`, `rsi` and `rdx` pass through to `start` untouched.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "sysv64" fn _start() -> ! {
    naked_asm!(
        "xor ebp, ebp",
        "and rsp, -16",
        "call {start}",
        "ud2",
        start = sym crate::start::start,
    )
}
````

- [ ] **Step 10: Implement `crates/relay-rt/src/args.rs`**

Insert this at the top of `crates/relay-rt/src/args.rs`, above `#[cfg(test)]`:

````rust
//! The arguments a program was started with (spec §5.3): `count` byte
//! strings, each followed by a NUL; argument 0 is the program's path as
//! given to `spawn`.

#[derive(Clone, Copy, Debug)]
pub struct Args {
    bytes: &'static [u8],
    count: usize,
}

impl Args {
    pub fn new(bytes: &'static [u8], count: usize) -> Args {
        Args { bytes, count }
    }

    /// Every argument, argument 0 first. Bytes after the last NUL and
    /// arguments beyond `count` do not count.
    pub fn iter(&self) -> impl Iterator<Item = &'static [u8]> + use<> {
        let bytes: &'static [u8] = self.bytes;
        bytes
            .split_inclusive(|&b| b == 0)
            .filter_map(|a| a.strip_suffix(&[0]))
            .take(self.count)
    }

    pub fn get(&self, i: usize) -> Option<&'static [u8]> {
        self.iter().nth(i)
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The last part of argument 0's path, as messages name the program
    /// (`ls: cannot access …`).
    pub fn name(&self) -> &'static [u8] {
        let path = self.get(0).unwrap_or(b"");
        match path.iter().rposition(|&b| b == b'/') {
            Some(i) => &path[i + 1..],
            None => path,
        }
    }
}

````

- [ ] **Step 11: Implement `crates/relay-rt/src/note.rs`**

Insert this at the top of `crates/relay-rt/src/note.rs`, above `#[cfg(test)]`:

````rust
//! The ELF note that names the ABI a program was built for (spec §5.2):
//! owner `Relay`, type 1, a 4-byte descriptor holding
//! `relay_abi::VERSION`. The user linker script keeps it in a `PT_NOTE`.

use relay_abi::{NOTE_NAME, NOTE_TYPE, VERSION};

/// An ELF note with a name of up to 7 bytes (8 with its NUL, a multiple
/// of 4) and a 4-byte descriptor.
#[repr(C, align(4))]
pub struct Note {
    namesz: u32,
    descsz: u32,
    kind: u32,
    name: [u8; 8],
    desc: u32,
}

pub const fn abi_note() -> Note {
    let mut name = [0; 8];
    let mut i = 0;
    while i < NOTE_NAME.len() {
        name[i] = NOTE_NAME[i];
        i += 1;
    }
    Note {
        namesz: NOTE_NAME.len() as u32 + 1,
        descsz: 4,
        kind: NOTE_TYPE,
        name,
        desc: VERSION,
    }
}

#[cfg(target_os = "none")]
#[used]
#[unsafe(link_section = ".note.relay")]
static ABI_NOTE: Note = abi_note();

````

- [ ] **Step 12: Implement `crates/relay-rt/src/start.rs`**

Insert this at the top of `crates/relay-rt/src/start.rs`, above `#[cfg(test)]`:

````rust
//! From `_start` to the program's `main`, and the panic handler.

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// The program's name for messages, set before `main` runs.
static NAME_PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static NAME_LEN: AtomicUsize = AtomicUsize::new(0);

fn set_name(name: &'static [u8]) {
    NAME_PTR.store(name.as_ptr().cast_mut(), Ordering::Relaxed);
    NAME_LEN.store(name.len(), Ordering::Relaxed);
}

/// The program's name, as `Args::name` gave it; empty before `start`.
pub fn name() -> &'static [u8] {
    let p = NAME_PTR.load(Ordering::Relaxed);
    if p.is_null() {
        return b"";
    }
    // SAFETY: set from a `&'static [u8]` in `set_name`.
    unsafe { core::slice::from_raw_parts(p, NAME_LEN.load(Ordering::Relaxed)) }
}

/// `<name>: panicked at <file>:<line>:<column>: <message>`, as one line.
pub fn panic_message(
    w: &mut impl Write,
    name: &[u8],
    location: Option<(&str, u32, u32)>,
    message: impl fmt::Display,
) -> fmt::Result {
    for chunk in name.utf8_chunks() {
        w.write_str(chunk.valid())?;
        if !chunk.invalid().is_empty() {
            w.write_char(char::REPLACEMENT_CHARACTER)?;
        }
    }
    w.write_str(": panicked")?;
    if let Some((file, line, column)) = location {
        write!(w, " at {file}:{line}:{column}")?;
    }
    writeln!(w, ": {message}")
}

/// What only exists in a program on Relay OS: the start and the panic
/// handler.
#[cfg(target_os = "none")]
mod on_relay {
    use super::{name, panic_message, set_name};
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
            /// Defined by `relay_rt::main!`.
            fn __relay_main(args: Args) -> u8;
        }
        let bytes: &'static [u8] = if ptr.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(ptr, len) }
        };
        let args = Args::new(bytes, count);
        set_name(args.name());
        let code = unsafe { __relay_main(args) };
        sys::exit(code)
    }

    /// A panic ends the program with status 101, as Rust programs do.
    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let location = info.location().map(|l| (l.file(), l.line(), l.column()));
        let _ = panic_message(&mut sys::Fd(2), name(), location, info.message());
        sys::exit(101)
    }
}

#[cfg(target_os = "none")]
pub(crate) use on_relay::start;

````

- [ ] **Step 13: Create `crates/relay-rt/src/sys.rs`**

Create `crates/relay-rt/src/sys.rs`:

````rust
//! System calls (spec §7.3). Each wrapper passes its arguments as the ABI
//! says and decodes the result into a value or an error number.

use core::fmt;
use relay_abi::{Call, decode};

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
use crate::arch::syscall;

/// Off Relay OS (the host tests) there is no kernel to call.
#[cfg(not(all(target_arch = "x86_64", target_os = "none")))]
unsafe fn syscall(_call: Call, _args: [u64; 6]) -> u64 {
    unimplemented!("system calls exist only on Relay OS")
}

/// Writes some of `bytes` to `fd`; returns how many.
pub fn write(fd: u32, bytes: &[u8]) -> Result<usize, u16> {
    let args = [
        u64::from(fd),
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
        0,
        0,
    ];
    decode(unsafe { syscall(Call::Write, args) }).map(|n| n as usize)
}

/// Writes all of `bytes` to `fd`, however many calls that takes.
pub fn write_all(fd: u32, mut bytes: &[u8]) -> Result<(), u16> {
    while !bytes.is_empty() {
        match write(fd, bytes)? {
            0 => return Err(relay_abi::errno::EIO),
            n => bytes = &bytes[n.min(bytes.len())..],
        }
    }
    Ok(())
}

/// Ends the program with status `code`.
pub fn exit(code: u8) -> ! {
    unsafe { syscall(Call::Exit, [u64::from(code), 0, 0, 0, 0, 0]) };
    // `exit` does not return.
    loop {
        core::hint::spin_loop();
    }
}

/// A file descriptor to `write!` to: `Fd(1)` is standard output, `Fd(2)`
/// standard error.
pub struct Fd(pub u32);

impl fmt::Write for Fd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_all(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}
````

- [ ] **Step 14: Create `crates/relay-rt/user.ld`**

Create `crates/relay-rt/user.ld`:

````text
/* Relay OS user programs (spec §5.1, §5.2): static, linked at 0x400000,
 * one PT_LOAD per permission set, and the ABI note in a PT_NOTE. */
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);   /* R-X */
    rodata PT_LOAD FLAGS(4);   /* R-- */
    data   PT_LOAD FLAGS(6);   /* RW- */
    note   PT_NOTE FLAGS(4);
}

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) { *(.text .text.*) } :text

    . = ALIGN(4K);
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) } :rodata
    .note.relay : ALIGN(4) { KEEP(*(.note.relay)) } :rodata :note

    . = ALIGN(4K);
    .data : ALIGN(4K) { *(.data .data.*) *(.got .got.*) } :data
    .bss : ALIGN(16) { *(.bss .bss.*) *(COMMON) } :data

    /DISCARD/ : { *(.comment) *(.note.gnu.* .note.GNU-stack) *(.eh_frame .eh_frame_hdr) }
}
````

- [ ] **Step 15: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 6 tests.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add Cargo.lock Cargo.toml crates
git commit -m "relay-rt: the runtime of user programs"
````


### Task 7: `t-args`, the first program

The first program is `t-args` (spec §8.5, decision 5): it prints each argument after its path as `[n] <arg>`, one per line, so from plan 2 on a scenario sees exactly what `spawn` passed. It lives in the package `relay-tests` (`userland/tests`, the `t-*` test programs), whose `build.rs` passes `relay-rt`'s linker script. Programs are built with the new profile `user`: the `relay` profile (overflow checks and debug assertions stay on) plus LTO, without debug info, since `system.img` holds every program in memory (spec §8.4): `t-args` is about 20 KiB. They use the target's default code model (decision 1): the prebuilt `core` for `x86_64-unknown-none` is compiled for the `kernel` model, LTO needs one model throughout, and that model's sign-extended 32-bit addresses reach everything a program linked at `0x40_0000` holds below 2 GiB; a program past 2 GiB fails to link ("relocation truncated"), never silently. The user packages are not default members (they only build for Relay OS); `cargo xtask lint` clippies them for `x86_64-unknown-none` with the `user` profile, one command per package in `USER_PACKAGES`.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `userland/tests/Cargo.toml`
- Create: `userland/tests/build.rs`
- Create: `userland/tests/src/bin/t-args.rs`
- Modify: `xtask/src/ci.rs`
- Modify: `xtask/src/config.rs`

**Interfaces:**
- Consumes: Task 6.
- Produces: package `relay-tests` with the binary `t-args`; `[profile.user]`; `xtask::config::{USER_PROFILE = "user", USER_PACKAGES = ["relay-tests"]}`; `ci::Packages::userland`.

- [ ] **Step 1: Add the test support `userland/tests/build.rs`**

Create `userland/tests/build.rs`:

````rust
fn main() {
    // The user linker script, from relay-rt's build script.
    let script = std::env::var("DEP_RELAY_RT_SCRIPT").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{script}");
}
````

- [ ] **Step 2: Add the test support `userland/tests/src/bin/t-args.rs`**

Create `userland/tests/src/bin/t-args.rs`:

````rust
//! `t-args [ARG]...`: prints each argument after the program's path as
//! `[n] <arg>`, one per line (spec §8.5), so a scenario sees exactly what
//! `spawn` passed.
#![no_std]
#![no_main]

use core::fmt::Write;
use relay_rt::Args;
use relay_rt::sys::{self, Fd};

relay_rt::main!(main);

fn main(args: Args) -> u8 {
    for (n, arg) in args.iter().enumerate().skip(1) {
        let printed = write!(Fd(1), "[{n}] ").is_ok()
            && sys::write_all(1, arg).is_ok()
            && sys::write_all(1, b"\n").is_ok();
        if !printed {
            return 1;
        }
    }
    0
}
````

- [ ] **Step 3: Add the failing tests to `xtask/src/ci.rs`**

In `xtask/src/ci.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        kernel: false,
    };
    const ALL: Packages = Packages {
        boot: true,
        kernel: true,
    };
````

with:

````rust
        kernel: false,
        userland: false,
    };
    const ALL: Packages = Packages {
        boot: true,
        kernel: true,
        userland: true,
    };
````

Replace:

````rust
        let all = lint_commands(ALL);
        assert_eq!(all.len(), 6);
        assert!(
````

with:

````rust
        let all = lint_commands(ALL);
        assert_eq!(all.len(), 6 + USER_PACKAGES.len());
        assert!(
````

Replace:

````rust
                .any(|c| c.contains(&"x86_64-unknown-none".to_string()))
        );
````

with:

````rust
                .any(|c| c.contains(&"x86_64-unknown-none".to_string()))
        );
        assert!(
            all.contains(&cmd(&[
                "clippy",
                "--profile",
                "user",
                "--package",
                "relay-tests",
                "--target",
                "x86_64-unknown-none",
                "--",
                "-D",
                "warnings"
            ])),
            "the user programs, for Relay OS"
        );
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask ci::`

Expected: FAIL: compile errors such as `` cannot find value `USER_PACKAGES` in this scope ``; `` struct `ci::Packages` has no field named `userland` ``.

- [ ] **Step 5: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "userland/tests", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "crates/heap", "crates/relay-abi", "crates/crc32", "crates/sysimg", "crates/relay-rt", "xtask"]
````

Replace:

````toml
overflow-checks = true

````

with:

````toml
overflow-checks = true

# User programs (spec §8.4): the target profile, plus LTO, without debug
# info, since `system.img` holds every program in memory.
[profile.user]
inherits = "relay"
lto = true
strip = "debuginfo"
````

- [ ] **Step 6: Create `userland/tests/Cargo.toml`**

Create `userland/tests/Cargo.toml`:

````toml
[package]
name = "relay-tests"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "The t-* test programs of /bin (spec §8.5)"
build = "build.rs"

[dependencies]
relay-rt.workspace = true
````

- [ ] **Step 7: Change `xtask/src/ci.rs`**

In `xtask/src/ci.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET};
use crate::util::{cargo, root};
````

with:

````rust

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET, USER_PACKAGES, USER_PROFILE};
use crate::util::{cargo, root};
````

Replace:

````rust
    pub kernel: bool,
}
````

with:

````rust
    pub kernel: bool,
    /// The user programs (`USER_PACKAGES`).
    pub userland: bool,
}
````

Replace:

````rust
            kernel: has("kernel"),
        }
````

with:

````rust
            kernel: has("kernel"),
            userland: root().join("userland").is_dir(),
        }
````

Replace:

````rust
                target,
                "--",
````

with:

````rust
                target,
                "--",
                "-D",
                "warnings",
            ]));
        }
    }
    if p.userland {
        for package in USER_PACKAGES {
            v.push(cmd(&[
                "clippy",
                "--profile",
                USER_PROFILE,
                "--package",
                package,
                "--target",
                KERNEL_TARGET,
                "--",
````

- [ ] **Step 8: Change `xtask/src/config.rs`**

In `xtask/src/config.rs`, replace:

````rust
pub const PROFILE: &str = "relay";
````

with:

````rust
pub const PROFILE: &str = "relay";
/// User programs (spec §8.4): `relay` plus LTO, without debug info.
pub const USER_PROFILE: &str = "user";
/// The packages under `userland/`, whose binaries make up `system.img`.
pub const USER_PACKAGES: &[&str] = &["relay-tests"];
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p xtask ci::`

Expected: PASS: 4 tests.

- [ ] **Step 10: Build `t-args` for Relay OS**

Run: `cargo build --profile user --package relay-tests --target x86_64-unknown-none`

Expected: it succeeds.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add Cargo.lock Cargo.toml userland xtask
git commit -m "userland: t-args, the first program, built for Relay OS and linted with the user profile"
````


### Task 8: xtask builds, checks and packs the programs

`cargo xtask build` now also builds every package of `USER_PACKAGES` (in their own target directory, `target/user`, so a build from inside `cargo test` never waits for the outer build's lock), takes the binaries from cargo's JSON messages, checks each one, and packs them into `target/relay/system.img` (ABI `VERSION`, mode 0755, the build's time); `write_esp` copies it to `\EFI\RELAY\system.img`, so `image`, `flash --full` and `flash --kernel` all write it with the kernel. The check (decision 3) reads the program with binutils' `readelf`, independently of our own code, and refuses what the kernel will refuse in plan 2 (spec §5.2): not an `EXEC`, not x86_64, a `DYNAMIC`, `INTERP` or `TLS` segment, a `PT_LOAD` outside `0x40_0000`–`0x1000_0000_0000` or both writable and executable, an entry point outside an executable segment, and a missing or other-ABI `Relay` note. The tests patch one field of the real `t-args` at a time and expect the rule that field breaks.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `xtask/Cargo.toml`
- Modify: `xtask/src/build.rs`
- Modify: `xtask/src/image.rs`
- Modify: `xtask/src/main.rs`
- Create: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: Tasks 3 (`sysimg::write`) and 7 (`USER_PACKAGES`, `USER_PROFILE`).
- Produces: `xtask::userland::{build() -> Result<Vec<Program>>, check_program(&Path) -> Result<()>, system_image(&[Program]) -> Result<Vec<u8>>, build_system_image() -> Result<PathBuf>, Program { name, path }, PROGRAM_BASE, PROGRAM_END}`; `build::Artifacts::system_img`; `\EFI\RELAY\system.img` on every ESP xtask writes.

- [ ] **Step 1: Change `xtask/Cargo.toml`**

In `xtask/Cargo.toml`, replace:

````toml
regex = "1"
serde_json = "1"
shell = { workspace = true }
vfs = { workspace = true }
````

with:

````toml
regex = "1"
relay-abi = { workspace = true }
serde_json = "1"
shell = { workspace = true }
sysimg = { workspace = true }
vfs = { workspace = true }
````

- [ ] **Step 2: Declare the new module in `xtask/src/main.rs`**

In `xtask/src/main.rs`, replace:

````rust
mod qmp;
mod util;
````

with:

````rust
mod qmp;
mod userland;
mod util;
````

- [ ] **Step 3: Write the failing tests for `xtask/src/userland.rs`**

Create `xtask/src/userland.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn t_args() -> Program {
        build()
            .unwrap()
            .into_iter()
            .find(|p| p.name == "t-args")
            .expect("t-args is built")
    }

    #[test]
    fn every_program_is_built_for_ring_3() {
        let programs = build().unwrap();
        assert!(programs.iter().any(|p| p.name == "t-args"));
        for p in &programs {
            check_program(&p.path).unwrap();
            let text = readelf(&p.path).unwrap();
            assert!(
                text.contains("Entry point address:               0x4"),
                "{}",
                p.name
            );
        }
    }

    /// `t-args` with one ELF field changed, and what `check_program` says.
    fn patched(name: &str, edit: impl Fn(&mut Vec<u8>)) -> String {
        let mut bytes = fs::read(t_args().path).unwrap();
        edit(&mut bytes);
        let dir = out_dir().join("xtask-tests");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, &bytes).unwrap();
        match check_program(&path) {
            Ok(()) => "accepted".into(),
            Err(e) => e.to_string(),
        }
    }

    fn u64_at(b: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
    }

    /// The file offset of the first `PT_LOAD` program header (ELF64:
    /// `e_phoff` at 0x20, 56-byte entries, `p_type` 1).
    fn first_load(b: &[u8]) -> usize {
        let phoff = u64_at(b, 0x20) as usize;
        (0..16)
            .map(|i| phoff + i * 56)
            .find(|&at| b[at..at + 4] == 1u32.to_le_bytes())
            .unwrap()
    }

    #[test]
    fn check_program_refuses_what_the_kernel_would() {
        assert_eq!(patched("same", |_| {}), "accepted");
        let host = check_program(Path::new("/bin/true"))
            .unwrap_err()
            .to_string();
        assert!(host.contains("not EXEC"), "a Linux program: {host}");
        let e = patched("dyn", |b| b[0x10] = 3);
        assert!(e.contains("type DYN"), "{e}");
        let e = patched("rwx", |b| {
            let at = first_load(b);
            b[at + 4] |= 2;
        });
        assert!(e.contains("writable and executable"), "{e}");
        let e = patched("low", |b| {
            let at = first_load(b) + 16;
            b[at..at + 8].copy_from_slice(&0x1000u64.to_le_bytes());
        });
        assert!(e.contains("is outside 0x400000..0x100000000000"), "{e}");
        let e = patched("entry", |b| {
            b[0x18..0x20].copy_from_slice(&0x7000_0000u64.to_le_bytes())
        });
        assert!(e.contains("is not in an executable segment"), "{e}");
        let e = patched("dynamic", |b| {
            let at = first_load(b);
            b[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
        });
        assert!(e.contains("has a DYNAMIC segment"), "{e}");
        let e = patched("abi", |b| {
            let mut note = b"Relay\0\0\0".to_vec();
            note.extend_from_slice(&relay_abi::VERSION.to_le_bytes());
            let at = b
                .windows(note.len())
                .position(|w| w == note)
                .expect("the note");
            b[at + 8] = b[at + 8].wrapping_add(1);
        });
        assert!(
            e.contains(&format!("built for ABI {}", relay_abi::VERSION + 1)),
            "{e}"
        );
        let e = patched("no-note", |b| {
            let at = b.windows(8).position(|w| w == b"Relay\0\0\0").unwrap();
            b[at] = b'X';
        });
        assert!(e.contains("no Relay note"), "{e}");
    }

    #[test]
    fn the_system_image_holds_every_program() {
        let programs = build().unwrap();
        let image = system_image(&programs).unwrap();
        let archive = sysimg::Archive::parse(&image).unwrap();
        assert_eq!(archive.abi(), relay_abi::VERSION);
        assert_eq!(archive.len(), programs.len());
        let t = archive.entry(archive.find(b"t-args").unwrap()).unwrap();
        assert_eq!(t.mode, 0o755);
        assert_eq!(t.data, fs::read(t_args().path).unwrap());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(archive.build_time().abs_diff(now) < 600);
    }
}
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask userland`

Expected: FAIL: compile errors such as `` cannot find type `Program` in this scope ``; `` cannot find function `build` in this scope ``.

- [ ] **Step 5: Change `xtask/src/build.rs`**

In `xtask/src/build.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! `cargo xtask build`: compiles the loader and the kernel.

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET};
use crate::util::{cargo, root};
````

with:

````rust
//! `cargo xtask build`: compiles the loader, the kernel and the user
//! programs, which it packs into `system.img`.

use crate::config::{KERNEL_TARGET, PROFILE, UEFI_TARGET};
use crate::userland;
use crate::util::{cargo, root};
````

Replace:

````rust
    pub kernel: PathBuf,
}
````

with:

````rust
    pub kernel: PathBuf,
    pub system_img: PathBuf,
}
````

Replace:

````rust
    }
    let t = root().join("target");
    Ok(Artifacts {
        bootx64: t.join(UEFI_TARGET).join(PROFILE).join("relay-boot.efi"),
        kernel: t.join(KERNEL_TARGET).join(PROFILE).join("relay-kernel"),
    })
````

with:

````rust
    }
    let system_img = userland::build_system_image()?;
    let t = root().join("target");
    Ok(Artifacts {
        bootx64: t.join(UEFI_TARGET).join(PROFILE).join("relay-boot.efi"),
        kernel: t.join(KERNEL_TARGET).join(PROFILE).join("relay-kernel"),
        system_img,
    })
````

- [ ] **Step 6: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Writes the loader, kernel and cmdline into the ESP. With `format` the
/// partition is first formatted as FAT32.
pub fn write_esp(
````

with:

````rust

/// Writes the loader, kernel, system archive and cmdline into the ESP.
/// With `format` the partition is first formatted as FAT32.
pub fn write_esp(
````

Replace:

````rust
        (art.kernel.as_path(), "::/EFI/RELAY/kernel.elf"),
        (cmdline_file.as_path(), "::/EFI/RELAY/cmdline"),
````

with:

````rust
        (art.kernel.as_path(), "::/EFI/RELAY/kernel.elf"),
        (art.system_img.as_path(), "::/EFI/RELAY/system.img"),
        (cmdline_file.as_path(), "::/EFI/RELAY/cmdline"),
````

- [ ] **Step 7: Implement `xtask/src/userland.rs`**

Insert this at the top of `xtask/src/userland.rs`, above `#[cfg(test)]`:

````rust
//! The user programs (spec §8 of the user-space gate): building the
//! packages under `userland/` for Relay OS, checking that every binary is
//! a program the kernel will load (spec §5.2), and packing them into
//! `system.img` (spec §4.2).

use crate::config::{KERNEL_TARGET, USER_PACKAGES, USER_PROFILE};
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lowest and end of the addresses a program's segments may use (spec
/// §5.1): above the unmapped first 4 MiB, below the `mem_map` area.
pub const PROGRAM_BASE: u64 = 0x40_0000;
pub const PROGRAM_END: u64 = 0x1000_0000_0000;

/// One built program: its name in `/bin` and the binary.
#[derive(Clone, Debug)]
pub struct Program {
    pub name: String,
    pub path: PathBuf,
}

/// Builds every package of `USER_PACKAGES` and returns its binaries,
/// sorted by name. They have their own target directory, so a build from
/// inside `cargo test` does not wait for the outer build's lock.
pub fn build() -> Result<Vec<Program>> {
    let mut programs = Vec::new();
    for package in USER_PACKAGES {
        let out = cargo()
            .args([
                "build",
                "--profile",
                USER_PROFILE,
                "--package",
                package,
                "--target",
                KERNEL_TARGET,
                "--target-dir",
            ])
            .arg(root().join("target").join("user"))
            .args(["--message-format", "json-render-diagnostics"])
            .stderr(Stdio::inherit())
            .output()?;
        ensure!(out.status.success(), "building {package} failed");
        for line in String::from_utf8(out.stdout)?.lines() {
            let msg: serde_json::Value = serde_json::from_str(line)?;
            if msg["reason"] == "compiler-artifact"
                && let Some(exe) = msg["executable"].as_str()
            {
                let path = PathBuf::from(exe);
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                programs.push(Program { name, path });
            }
        }
    }
    programs.sort_by(|a, b| a.name.cmp(&b.name));
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}

/// What `readelf` says about a program. `readelf` (binutils) reads the ELF
/// independently of our own code.
fn readelf(path: &Path) -> Result<String> {
    run_stdout(Command::new("readelf").args(["-hlnW"]).arg(path))
}

/// Checks the rules of spec §5.2 that the build decides: a static x86_64
/// executable whose loadable segments lie in the program area and are never
/// both writable and executable, whose entry point is in an executable
/// segment, and whose `Relay` note holds this ABI's version.
pub fn check_program(path: &Path) -> Result<()> {
    let text = readelf(path)?;
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    ensure!(
        field("Type:").starts_with("EXEC"),
        "type {}, not EXEC",
        field("Type:")
    );
    ensure!(
        field("Machine:").contains("X86-64"),
        "machine {}",
        field("Machine:")
    );
    let entry = hex(&field("Entry point address:"))?;
    let mut loads = 0;
    let mut entry_ok = false;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first() {
            Some(&"LOAD") => {}
            Some(&"DYNAMIC") | Some(&"INTERP") | Some(&"TLS") => {
                bail!("has a {} segment", words[0])
            }
            _ => continue,
        }
        // LOAD offset vaddr paddr filesz memsz flags... align
        ensure!(words.len() >= 8, "unexpected readelf line {line:?}");
        let vaddr = hex(words[2])?;
        let memsz = hex(words[5])?;
        let flags: String = words[6..words.len() - 1].concat();
        let end = vaddr.checked_add(memsz).context("segment end overflows")?;
        ensure!(
            vaddr >= PROGRAM_BASE && end <= PROGRAM_END,
            "segment {vaddr:#x}..{end:#x} is outside {PROGRAM_BASE:#x}..{PROGRAM_END:#x}"
        );
        ensure!(
            !(flags.contains('W') && flags.contains('E')),
            "segment at {vaddr:#x} is writable and executable"
        );
        if flags.contains('E') && (vaddr..end).contains(&entry) {
            entry_ok = true;
        }
        loads += 1;
    }
    ensure!(loads > 0, "no loadable segment");
    ensure!(
        entry_ok,
        "entry point {entry:#x} is not in an executable segment"
    );
    let note = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Relay ") && l.contains("description data:"))
        .context("no Relay note")?;
    let data: Vec<u8> = note
        .split("description data:")
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<_, _>>()?;
    ensure!(
        data.len() == 4,
        "the Relay note holds {} bytes, not 4",
        data.len()
    );
    let abi = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ensure!(
        abi == relay_abi::VERSION,
        "built for ABI {abi}, this is ABI {}",
        relay_abi::VERSION
    );
    Ok(())
}

fn hex(s: &str) -> Result<u64> {
    let digits = s
        .strip_prefix("0x")
        .with_context(|| format!("{s:?} is not hex"))?;
    Ok(u64::from_str_radix(digits, 16)?)
}

/// `system.img` for these programs: every one executable (0755), with the
/// time of the build as their file times.
pub fn system_image(programs: &[Program]) -> Result<Vec<u8>> {
    let data: Vec<Vec<u8>> = programs
        .iter()
        .map(|p| fs::read(&p.path))
        .collect::<Result<_, _>>()?;
    let entries: Vec<sysimg::Entry<'_>> = programs
        .iter()
        .zip(&data)
        .map(|(p, d)| sysimg::Entry {
            name: p.name.as_bytes(),
            mode: 0o755,
            data: d,
        })
        .collect();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    sysimg::write(relay_abi::VERSION, now, &entries).map_err(|e| anyhow::anyhow!("system.img: {e}"))
}

/// Builds the programs and writes `target/relay/system.img`.
pub fn build_system_image() -> Result<PathBuf> {
    let image = system_image(&build()?)?;
    fs::create_dir_all(out_dir())?;
    let path = out_dir().join("system.img");
    fs::write(&path, image)?;
    Ok(path)
}

````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 3 tests.

- [ ] **Step 9: Run the `boot` scenario**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add Cargo.lock xtask
git commit -m "xtask: the user programs are built, checked with readelf and packed into system.img on the ESP"
````


### Task 9: The build checks the kernel's segment rules and the note's segment

Review finding (important): Task 8's `check_program` refused a few segment types by name and read the ABI note from the section headers, which is where `readelf -n` looks in a file that has them. But the kernel will read program headers only (spec §5.2), and accept only `PT_LOAD`, `PT_NOTE` and `PT_GNU_STACK`. A change to `user.ld` that dropped the note's `PT_NOTE` (the `:note` or the `note PT_NOTE` line) left the section in place, so the build passed and every program would fail with `ENOEXEC` in plan 2. Patches the check let through: a `PT_NOTE` turned into `PT_NULL`, `PT_GNU_RELRO` or `PT_PHDR`; the note's type 1 changed to 2; two overlapping `PT_LOAD`s; a `p_offset` not congruent with `p_vaddr` modulo 4 KiB; more file than memory. `check_program` now allow-lists the three types, checks every loadable segment for overlap (by page), congruence and `p_filesz ≤ p_memsz`, and reads the note from the bytes of each `PT_NOTE` segment, with its type. `readelf` runs with `LC_ALL=C`, since the checks read its English labels.

**Files:**
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: Task 8.
- Produces: no interface change; `check_program` refuses everything the kernel's checks of plan 2 will.

- [ ] **Step 1: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
        });
        assert!(e.contains("no Relay note"), "{e}");
    }
````

with:

````rust
        });
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
    }

    /// The file offset of the `n`th program header of type `kind`.
    fn phdr(b: &[u8], kind: u32, n: usize) -> usize {
        let phoff = u64_at(b, 0x20) as usize;
        (0..16)
            .map(|i| phoff + i * 56)
            .filter(|&at| b[at..at + 4] == kind.to_le_bytes())
            .nth(n)
            .unwrap()
    }

    const PT_NOTE: u32 = 4;

    #[test]
    fn the_kernel_s_segment_rules_are_the_build_s() {
        let set_type = |kind: u32| {
            move |b: &mut Vec<u8>| {
                let at = phdr(b, PT_NOTE, 0);
                b[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            }
        };
        let e = patched("null", set_type(0));
        assert!(
            e.contains("has a NULL segment; only LOAD, NOTE and GNU_STACK"),
            "{e}"
        );
        let e = patched("relro", set_type(0x6474_e552));
        assert!(e.contains("has a GNU_RELRO segment"), "{e}");
        let e = patched("phdr", set_type(6));
        assert!(e.contains("has a PHDR segment"), "{e}");
        // The note's section is still there, but no PT_NOTE holds it: the
        // kernel reads segments, not sections.
        let e = patched("no-note-segment", set_type(0x6474_e551));
        assert!(e.contains("no PT_NOTE segment holds the Relay note"), "{e}");
        let e = patched("note-type", |b| {
            let at = b.windows(8).position(|w| w == b"Relay\0\0\0").unwrap();
            b[at - 4] = 2;
        });
        assert!(e.contains("the Relay note has type 2, not 1"), "{e}");
        let e = patched("overlap", |b| {
            let (first, second) = (phdr(b, 1, 0), phdr(b, 1, 1));
            let vaddr = u64_at(b, first + 16);
            b[second + 16..second + 24].copy_from_slice(&vaddr.to_le_bytes());
        });
        assert!(e.contains("overlaps the one before it"), "{e}");
        let e = patched("congruence", |b| {
            let at = phdr(b, 1, 0) + 8;
            b[at..at + 8].copy_from_slice(&0x1800u64.to_le_bytes());
        });
        assert!(e.contains("are not congruent modulo 4 KiB"), "{e}");
        let e = patched("filesz", |b| {
            let at = phdr(b, 1, 0);
            let memsz = u64_at(b, at + 40);
            b[at + 32..at + 40].copy_from_slice(&(memsz + 1).to_le_bytes());
        });
        assert!(e.contains("has more file"), "{e}");
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask userland`

Expected: FAIL: 2 tests fail: `userland::tests::the_kernel_s_segment_rules_are_the_build_s`, `userland::tests::check_program_refuses_what_the_kernel_would`.

- [ ] **Step 3: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
````

with:

````rust
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, ensure};
use std::fs;
````

Replace:

````rust

/// What `readelf` says about a program. `readelf` (binutils) reads the ELF
/// independently of our own code.
fn readelf(path: &Path) -> Result<String> {
    run_stdout(Command::new("readelf").args(["-hlnW"]).arg(path))
}

/// Checks the rules of spec §5.2 that the build decides: a static x86_64
/// executable whose loadable segments lie in the program area and are never
/// both writable and executable, whose entry point is in an executable
/// segment, and whose `Relay` note holds this ABI's version.
pub fn check_program(path: &Path) -> Result<()> {
````

with:

````rust

/// What `readelf` says about a program's ELF header and program headers.
/// `readelf` (binutils) reads the ELF independently of our own code;
/// `LC_ALL=C` keeps its labels in English.
fn readelf(path: &Path) -> Result<String> {
    run_stdout(
        Command::new("readelf")
            .env("LC_ALL", "C")
            .args(["-hlW"])
            .arg(path),
    )
}

/// One row of `readelf -l`'s program headers.
struct Segment {
    kind: String,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    flags: String,
}

/// The rows between `Program Headers:` and the blank line after them:
/// `Type Offset VirtAddr PhysAddr FileSiz MemSiz Flg Align`, where the
/// flags are one to three words (`R E`).
fn segments(text: &str) -> Result<Vec<Segment>> {
    let mut rows = text
        .lines()
        .skip_while(|l| !l.starts_with("Program Headers:"))
        .skip(2)
        .take_while(|l| !l.trim().is_empty());
    let mut out = Vec::new();
    for line in rows.by_ref() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first().is_some_and(|w| w.starts_with('[')) {
            continue; // "[Requesting program interpreter: …]"
        }
        ensure!(words.len() >= 8, "unexpected readelf line {line:?}");
        out.push(Segment {
            kind: words[0].to_string(),
            offset: hex(words[1])?,
            vaddr: hex(words[2])?,
            filesz: hex(words[4])?,
            memsz: hex(words[5])?,
            flags: words[6..words.len() - 1].concat(),
        });
    }
    Ok(out)
}

/// The ABI version in the `Relay` note of `notes` (the bytes of a
/// `PT_NOTE` segment), if it has one: each note is `namesz`, `descsz`,
/// `type`, then the name and the descriptor, each padded to 4 bytes.
fn relay_note(notes: &[u8]) -> Result<Option<u32>> {
    let u32_at = |at: usize| -> Option<u32> {
        notes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let mut at = 0;
    while at + 12 <= notes.len() {
        let (namesz, descsz, kind) = (
            u32_at(at).unwrap() as usize,
            u32_at(at + 4).unwrap() as usize,
            u32_at(at + 8).unwrap(),
        );
        let name_at = at + 12;
        let desc_at = name_at + namesz.next_multiple_of(4);
        let next = desc_at + descsz.next_multiple_of(4);
        ensure!(next <= notes.len(), "a note runs past its segment");
        let mut name = &notes[name_at..name_at + namesz];
        name = name.strip_suffix(&[0]).unwrap_or(name);
        if name == relay_abi::NOTE_NAME {
            ensure!(
                kind == relay_abi::NOTE_TYPE,
                "the Relay note has type {kind}, not {}",
                relay_abi::NOTE_TYPE
            );
            ensure!(descsz == 4, "the Relay note holds {descsz} bytes, not 4");
            return Ok(u32_at(desc_at));
        }
        at = next;
    }
    Ok(None)
}

/// Checks the rules of spec §5.2 that the build decides, as the kernel will
/// read the program: a static x86_64 executable with only `PT_LOAD`,
/// `PT_NOTE` and `PT_GNU_STACK` program headers; loadable segments in the
/// program area, not overlapping, with offsets and addresses congruent
/// modulo 4 KiB, no more file than memory, never writable and executable;
/// the entry point in an executable segment; and a `PT_NOTE` segment
/// holding the `Relay` note with this ABI's version.
pub fn check_program(path: &Path) -> Result<()> {
````

Replace:

````rust
    let entry = hex(&field("Entry point address:"))?;
    let mut loads = 0;
    let mut entry_ok = false;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first() {
            Some(&"LOAD") => {}
            Some(&"DYNAMIC") | Some(&"INTERP") | Some(&"TLS") => {
                bail!("has a {} segment", words[0])
            }
            _ => continue,
        }
        // LOAD offset vaddr paddr filesz memsz flags... align
        ensure!(words.len() >= 8, "unexpected readelf line {line:?}");
        let vaddr = hex(words[2])?;
        let memsz = hex(words[5])?;
        let flags: String = words[6..words.len() - 1].concat();
        let end = vaddr.checked_add(memsz).context("segment end overflows")?;
        ensure!(
````

with:

````rust
    let entry = hex(&field("Entry point address:"))?;
    let segments = segments(&text)?;
    for s in &segments {
        ensure!(
            matches!(s.kind.as_str(), "LOAD" | "NOTE" | "GNU_STACK"),
            "has a {} segment; only LOAD, NOTE and GNU_STACK are allowed",
            s.kind
        );
    }
    let mut loads: Vec<&Segment> = segments.iter().filter(|s| s.kind == "LOAD").collect();
    ensure!(!loads.is_empty(), "no loadable segment");
    loads.sort_by_key(|s| s.vaddr);
    let mut entry_ok = false;
    let mut previous_end = 0;
    for s in &loads {
        let (vaddr, flags) = (s.vaddr, &s.flags);
        let end = vaddr
            .checked_add(s.memsz)
            .context("segment end overflows")?;
        ensure!(
````

Replace:

````rust
        );
        if flags.contains('E') && (vaddr..end).contains(&entry) {
            entry_ok = true;
        }
        loads += 1;
    }
    ensure!(loads > 0, "no loadable segment");
    ensure!(
        entry_ok,
        "entry point {entry:#x} is not in an executable segment"
    );
    let note = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Relay ") && l.contains("description data:"))
        .context("no Relay note")?;
    let data: Vec<u8> = note
        .split("description data:")
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<_, _>>()?;
    ensure!(
        data.len() == 4,
        "the Relay note holds {} bytes, not 4",
        data.len()
    );
    let abi = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ensure!(
````

with:

````rust
        );
        ensure!(
            s.filesz <= s.memsz,
            "segment at {vaddr:#x} has more file ({:#x}) than memory ({:#x})",
            s.filesz,
            s.memsz
        );
        ensure!(
            s.offset % 4096 == vaddr % 4096,
            "segment at {vaddr:#x}: offset {:#x} and address are not congruent modulo 4 KiB",
            s.offset
        );
        ensure!(
            vaddr & !0xFFF >= previous_end,
            "segment at {vaddr:#x} overlaps the one before it (pages up to {previous_end:#x})"
        );
        previous_end = end.next_multiple_of(4096);
        if flags.contains('E') && (vaddr..end).contains(&entry) {
            entry_ok = true;
        }
    }
    ensure!(
        entry_ok,
        "entry point {entry:#x} is not in an executable segment"
    );
    let file = fs::read(path)?;
    let mut abi = None;
    for s in segments.iter().filter(|s| s.kind == "NOTE") {
        let notes = usize::try_from(s.offset)
            .ok()
            .zip(usize::try_from(s.filesz).ok())
            .and_then(|(o, n)| file.get(o..o.checked_add(n)?))
            .context("a PT_NOTE segment lies outside the file")?;
        if let Some(v) = relay_note(notes)? {
            abi = Some(v);
        }
    }
    let abi = abi.context("no PT_NOTE segment holds the Relay note")?;
    ensure!(
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 4 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "xtask: a program passes the build only if the kernel's segment rules pass, its note in a PT_NOTE"
````


### Task 10: A missing `readelf` is named, not blamed on the program

Review finding (minor): on a machine without binutils, `cargo xtask build` failed with `t-args is not a Relay OS program`, the tool's absence showing only in the cause below. `userland::build` now makes sure `readelf` starts before it checks any program, and fails with `cannot run readelf (…): install binutils` (M1's rule: a missing tool fails, never skips).

**Files:**
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: Task 9.
- Produces: no interface change.

- [ ] **Step 1: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
    #[test]
    fn the_system_image_holds_every_program() {
````

with:

````rust
    #[test]
    fn a_missing_tool_is_named() {
        require_tool("readelf", "binutils").unwrap();
        let e = require_tool("readelf-that-is-not-installed", "binutils")
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("cannot run readelf-that-is-not-installed ("),
            "{e}"
        );
        assert!(e.ends_with("): install binutils"), "{e}");
    }

    #[test]
    fn the_system_image_holds_every_program() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask userland`

Expected: FAIL: compile errors such as `` cannot find function `require_tool` in this scope ``.

- [ ] **Step 3: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, ensure};
use std::fs;
````

with:

````rust
use crate::util::{cargo, out_dir, root, run_stdout};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
````

Replace:

````rust
    programs.sort_by(|a, b| a.name.cmp(&b.name));
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}
````

with:

````rust
    programs.sort_by(|a, b| a.name.cmp(&b.name));
    require_tool("readelf", "binutils")?;
    for p in &programs {
        check_program(&p.path).with_context(|| format!("{} is not a Relay OS program", p.name))?;
    }
    Ok(programs)
}

/// Fails, naming the tool and the package it comes in, if `program`
/// cannot be started, so a missing tool is not blamed on what it checks.
fn require_tool(program: &str, package: &str) -> Result<()> {
    match Command::new(program).arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) => bail!("cannot run {program} ({e}): install {package}"),
    }
}
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask userland`

Expected: PASS: 5 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -m "xtask: a missing readelf is named, not blamed on the program it checks"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 20 scenario(s) passed`.

````bash
git push -u origin m2p1/userland
gh pr create --base main --head m2p1/userland --title "Milestone 2, plan 1: User programs and their build" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 6–10: `crates/relay-rt`, the runtime of user programs (entry, arguments, `write`/`exit`, panic handler, the ABI note, the linker script); `t-args`, the first program, built with the `user` profile and linted for Relay OS; xtask builds the programs, checks each with `readelf` against the kernel's segment rules (the ABI note in a `PT_NOTE`) and writes `system.img` to the ESP with the kernel; a missing `readelf` is named.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed now: `system.img` is on the ESP but nothing reads it until PR 5
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p1/userland --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-userland
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: `/bin` on the NUC (Tasks 11–15)

The loader hands `system.img` to the kernel, which checks it and mounts it at `/bin`; new scenarios cover a good, a missing and a wrong-ABI archive; NUC check 3 lists `/bin`. It ends with NUC check 3 on a stick written by `cargo xtask flash --full` from this worktree.

Branch `m2p1/system`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-system`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m2p1/system /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-system origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-system
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `m2p1/userland` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m2p1/system /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-system m2p1/userland`), and after its merge rebase with `git rebase --onto origin/main <old tip of m2p1/userland>` and re-run `cargo xtask ci` before pushing.

### Task 11: The loader reads `system.img`

Spec §4.3: `relay-boot` reads `\EFI\RELAY\system.img` if it is there, copies it into pages it allocates as `LOADER_DATA`, and passes their physical address and the file's length in two new `BootInfo` fields (`BootInfo` version 2; a missing or empty file gives length 0). The loader does not check the contents; the kernel does, in Task 13. `LOADER_DATA` is the kernel's `Bootloader` memory, which its frame allocator never hands out (M1 §5.1) and its linear map covers, so the kernel can read the archive in place for as long as it runs. The file is read through the same `SimpleFileSystem` path as `kernel.elf` (protocols opened with `GET_PROTOCOL` only, M1 §15 item 7), right after the kernel is loaded and before the memory map is taken for the page tables.

**Files:**
- Modify: `boot/src/main.rs`
- Modify: `crates/boot-info/src/lib.rs`

**Interfaces:**
- Consumes: Task 8 (`system.img` on the ESP).
- Produces: `boot_info::{BOOT_INFO_VERSION = 2, BootInfo::{system_image_phys, system_image_len}, BootInfo::system_image() -> Option<(u64, u64)>}`.

- [ ] **Step 1: Add the failing tests to `crates/boot-info/src/lib.rs`**

In `crates/boot-info/src/lib.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            has_boot_partition_guid: 0,
        }
````

with:

````rust
            has_boot_partition_guid: 0,
            system_image_phys: 0,
            system_image_len: 0,
        }
````

Replace:

````rust
        assert!(bi.is_valid());
        bi.version = 2;
        assert!(!bi.is_valid());
        bi.version = BOOT_INFO_VERSION;
````

with:

````rust
        assert!(bi.is_valid());
        assert_eq!(BOOT_INFO_VERSION, 2);
        for other in [1, 3] {
            bi.version = other;
            assert!(!bi.is_valid(), "version {other}");
        }
        bi.version = BOOT_INFO_VERSION;
````

Replace:

````rust
        assert_eq!(bi.cmdline(), "");
    }
````

with:

````rust
        assert_eq!(bi.cmdline(), "");
    }

    #[test]
    fn the_system_image_is_there_when_it_has_a_length() {
        let mut bi = blank();
        assert_eq!(bi.system_image(), None);
        bi.system_image_phys = 0x1234_5000;
        assert_eq!(bi.system_image(), None, "no length, no file");
        bi.system_image_len = 23_784;
        assert_eq!(bi.system_image(), Some((0x1234_5000, 23_784)));
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p boot-info`

Expected: FAIL: compile errors such as `` struct `BootInfo` has no field named `system_image_phys` ``; `` struct `BootInfo` has no field named `system_image_len` ``.

- [ ] **Step 3: Change `boot/src/main.rs`**

In `boot/src/main.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//!
//! Loads \EFI\RELAY\kernel.elf, sets up the display, builds page tables,
//! exits boot services and jumps to the kernel with a `BootInfo`.
#![no_std]
````

with:

````rust
//!
//! Loads \EFI\RELAY\kernel.elf and \EFI\RELAY\system.img, sets up the
//! display, builds page tables, exits boot services and jumps to the kernel
//! with a `BootInfo`.
#![no_std]
````

Replace:

````rust
const CMDLINE_PATH: &CStr16 = cstr16!("\\EFI\\RELAY\\cmdline");
/// Spare memory-map slots for descriptors created by our own allocations.
````

with:

````rust
const CMDLINE_PATH: &CStr16 = cstr16!("\\EFI\\RELAY\\cmdline");
const SYSTEM_PATH: &CStr16 = cstr16!("\\EFI\\RELAY\\system.img");
/// Spare memory-map slots for descriptors created by our own allocations.
````

Replace:

````rust

/// Switches to the new page tables and stack, then calls the kernel.
````

with:

````rust

/// Copies `\EFI\RELAY\system.img` into `LOADER_DATA` pages, which the
/// kernel keeps (spec §4.3 of the user-space gate), and returns their
/// physical address and the file's length; `(0, 0)` without the file. The
/// kernel checks the contents.
fn load_system_image() -> (u64, u64) {
    let Some(file) = read_file(SYSTEM_PATH).filter(|f| !f.is_empty()) else {
        return (0, 0);
    };
    let base = alloc_pages(MemoryType::LOADER_DATA, file.len().div_ceil(4096));
    unsafe { core::ptr::copy_nonoverlapping(file.as_ptr(), base, file.len()) };
    (base as u64, file.len() as u64)
}

/// Switches to the new page tables and stack, then calls the kernel.
````

Replace:

````rust
    drop(kernel_file);

````

with:

````rust
    drop(kernel_file);
    let (system_phys, system_len) = load_system_image();

````

Replace:

````rust
        has_boot_partition_guid: guid.is_some() as u32,
    };
````

with:

````rust
        has_boot_partition_guid: guid.is_some() as u32,
        system_image_phys: system_phys,
        system_image_len: system_len,
    };
````

- [ ] **Step 4: Change `crates/boot-info/src/lib.rs`**

In `crates/boot-info/src/lib.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub const BOOT_INFO_MAGIC: u64 = u64::from_le_bytes(*b"RELAYBOO");
pub const BOOT_INFO_VERSION: u32 = 1;

````

with:

````rust
pub const BOOT_INFO_MAGIC: u64 = u64::from_le_bytes(*b"RELAYBOO");
/// 2: `system_image_*` (the user-space gate, spec §4.3).
pub const BOOT_INFO_VERSION: u32 = 2;

````

Replace:

````rust
    pub has_boot_partition_guid: u32,
}
````

with:

````rust
    pub has_boot_partition_guid: u32,
    /// Physical address and length of `\EFI\RELAY\system.img`, which the
    /// loader read into `LOADER_DATA` pages (`Bootloader` memory, which the
    /// kernel never hands out). Length 0: there was no such file.
    pub system_image_phys: u64,
    pub system_image_len: u64,
}
````

Replace:

````rust
        (self.has_boot_partition_guid == 1).then_some(self.boot_partition_guid)
    }
````

with:

````rust
        (self.has_boot_partition_guid == 1).then_some(self.boot_partition_guid)
    }

    /// `(physical address, length)` of the system archive, if the loader
    /// found one.
    pub fn system_image(&self) -> Option<(u64, u64)> {
        (self.system_image_len != 0).then_some((self.system_image_phys, self.system_image_len))
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p boot-info`

Expected: PASS: 8 tests.

- [ ] **Step 6: Run the `boot` scenario**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add boot crates
git commit -m "boot: the loader reads system.img and hands it over in BootInfo version 2"
````


### Task 12: The e2e steps `esp-delete` and `system-abi`

Two scenarios of spec §12.3 need an ESP the image does not have: `system_missing` (no `system.img`) and `system_abi` (an archive for another ABI). The runner gains two steps that, like `esp-write`, change the ESP of the scenario's copy of the image before booting: `esp-delete <path>` removes a file (mtools `mdel`), and `system-abi <n>` rewrites `system.img` with the same programs and build time under ABI `n` (`userland::with_abi`). The ESP edits become one ordered list, `Scenario::esp_edits`.

**Files:**
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/image.rs`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: Task 8 (`target/relay/system.img`).
- Produces: `e2e::EspEdit::{Write, Delete, SystemAbi}`, `Scenario::esp_edits`; `image::esp_delete(target, esp, path)`; `userland::with_abi(&[u8], u32) -> Result<Vec<u8>>`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
        assert_eq!(
            s.esp_writes,
            vec![("/EFI/RELAY/kernel.elf".into(), "not an elf".into())]
        );
        assert!(parse_scenario("x", "esp-write relative x").is_err());
        assert!(parse_scenario("x", "expect a\nesp-write /x y").is_err());
    }
````

with:

````rust
        assert_eq!(
            s.esp_edits,
            vec![EspEdit::Write(
                "/EFI/RELAY/kernel.elf".into(),
                "not an elf".into()
            )]
        );
        assert!(parse_scenario("x", "esp-write relative x").is_err());
        assert!(parse_scenario("x", "expect a\nesp-write /x y").is_err());
    }

    #[test]
    fn parses_esp_deletes_and_other_abis() {
        let s = parse_scenario(
            "x",
            "esp-delete /EFI/RELAY/system.img\nsystem-abi 99\nexpect x",
        )
        .unwrap();
        assert_eq!(
            s.esp_edits,
            vec![
                EspEdit::Delete("/EFI/RELAY/system.img".into()),
                EspEdit::SystemAbi(99)
            ]
        );
        assert!(parse_scenario("x", "esp-delete relative").is_err());
        assert!(parse_scenario("x", "system-abi many").is_err());
        assert!(parse_scenario("x", "expect a\nsystem-abi 2").is_err());
        assert!(parse_scenario("x", "expect a\nesp-delete /x").is_err());
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find type `EspEdit` in this scope ``; `` no field `esp_edits` on type `Scenario` ``.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
//! timeout 20                       (seconds, for the following expects)
````

with:

````rust
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
//! esp-delete /EFI/RELAY/system.img  (before boot: remove an ESP file)
//! system-abi 99                    (before boot: system.img, rewritten with
//!                                   another ABI version)
//! timeout 20                       (seconds, for the following expects)
````

Replace:

````rust
use crate::checks;
use crate::image::{self, Layout, Partition, esp_write, set_cmdline};
use crate::keys;
use crate::qemu::{self, Qemu};
use crate::qmp::Qmp;
use crate::util::{out_dir, root};
````

with:

````rust
use crate::checks;
use crate::image::{self, Layout, Partition, esp_delete, esp_write, set_cmdline};
use crate::keys;
use crate::qemu::{self, Qemu};
use crate::qmp::Qmp;
use crate::userland;
use crate::util::{out_dir, root};
````

Replace:

````rust

#[derive(Debug, PartialEq)]
````

with:

````rust

/// A change to the ESP before booting.
#[derive(Debug, PartialEq)]
pub enum EspEdit {
    /// `esp-write`: the file at this path gets these contents.
    Write(String, String),
    /// `esp-delete`: the file at this path is removed.
    Delete(String),
    /// `system-abi`: `system.img` holds the same programs under another
    /// ABI version.
    SystemAbi(u32),
}

#[derive(Debug, PartialEq)]
````

Replace:

````rust
    pub break_root: bool,
    /// ESP files to overwrite before booting: (path, contents).
    pub esp_writes: Vec<(String, String)>,
    /// (line number, step)
````

with:

````rust
    pub break_root: bool,
    /// Changes to the ESP before booting, in order.
    pub esp_edits: Vec<EspEdit>,
    /// (line number, step)
````

Replace:

````rust
    let mut break_root = false;
    let mut esp_writes = Vec::new();
    let mut steps = Vec::new();
````

with:

````rust
    let mut break_root = false;
    let mut esp_edits = Vec::new();
    let mut steps = Vec::new();
````

Replace:

````rust
            }
            "esp-write" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: esp-write must come before other steps");
                }
                let (path, contents) = rest.split_once(' ').unwrap_or((rest, ""));
                if !path.starts_with('/') {
                    bail!("{name}:{line_no}: esp-write path must be absolute");
                }
                esp_writes.push((path.to_string(), contents.to_string()));
                continue;
````

with:

````rust
            }
            "esp-write" | "esp-delete" | "system-abi" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: {word} must come before other steps");
                }
                let edit =
                    match word {
                        "system-abi" => EspEdit::SystemAbi(rest.parse().with_context(|| {
                            format!("{name}:{line_no}: system-abi needs a number")
                        })?),
                        _ => {
                            let (path, contents) = rest.split_once(' ').unwrap_or((rest, ""));
                            if !path.starts_with('/') {
                                bail!("{name}:{line_no}: {word} path must be absolute");
                            }
                            if word == "esp-write" {
                                EspEdit::Write(path.to_string(), contents.to_string())
                            } else {
                                EspEdit::Delete(path.to_string())
                            }
                        }
                    };
                esp_edits.push(edit);
                continue;
````

Replace:

````rust
        break_root,
        esp_writes,
        steps,
````

with:

````rust
        break_root,
        esp_edits,
        steps,
````

Replace:

````rust
    set_cmdline(&q.disk, layout.esp, &scenario.cmdline, run_dir)?;
    for (path, contents) in &scenario.esp_writes {
        esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?;
    }
````

with:

````rust
    set_cmdline(&q.disk, layout.esp, &scenario.cmdline, run_dir)?;
    for edit in &scenario.esp_edits {
        match edit {
            EspEdit::Write(path, contents) => {
                esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?
            }
            EspEdit::Delete(path) => esp_delete(&q.disk, layout.esp, path)?,
            EspEdit::SystemAbi(abi) => {
                let image = fs::read(out_dir().join("system.img"))?;
                let other = userland::with_abi(&image, *abi)?;
                esp_write(
                    &q.disk,
                    layout.esp,
                    "/EFI/RELAY/system.img",
                    &other,
                    run_dir,
                )?;
            }
        }
    }
````

- [ ] **Step 4: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust
        .arg(&file)
        .arg(format!("::{path}")))
````

with:

````rust
        .arg(&file)
        .arg(format!("::{path}")))
}

/// Removes one file (absolute ESP path, `/` separators) from an existing
/// ESP.
pub fn esp_delete(target: &Path, esp: Partition, path: &str) -> Result<()> {
    run(mtools("mdel")
        .args(["-i", &mtools_target(target, esp)])
        .arg(format!("::{path}")))
````

- [ ] **Step 5: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust

/// Builds the programs and writes `target/relay/system.img`.
````

with:

````rust

/// `image` with its programs under ABI version `abi` instead (the e2e step
/// `system-abi`).
pub fn with_abi(image: &[u8], abi: u32) -> Result<Vec<u8>> {
    let archive = sysimg::Archive::parse(image).map_err(|e| anyhow::anyhow!("system.img: {e}"))?;
    let entries: Vec<sysimg::Entry<'_>> = archive.entries().collect();
    sysimg::write(abi, archive.build_time(), &entries)
        .map_err(|e| anyhow::anyhow!("system.img: {e}"))
}

/// Builds the programs and writes `target/relay/system.img`.
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 70 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add xtask
git commit -m "xtask: the e2e steps esp-delete and system-abi change the ESP before booting"
````


### Task 13: The kernel mounts the archive at `/bin`

Startup step 10 (spec §4.3): after `mount /`, the kernel takes the archive the loader left, checks that its range is loader memory (in `Bootloader` regions only, which may split it), checks it (Task 3) and its ABI version, and mounts it at `/bin`, printing `[ ok ] system: 1 program, ABI 1` or `[FAIL] system: <reason>`: `no system.img`, `system.img: checksum mismatch` (or another `SysImgError`), `ABI 99, kernel wants 1`, `cannot mount /bin: Not a directory`. In plan 1 the kernel still runs the in-kernel shell (decision 6): on a failure it runs on with an empty `/bin`; spec §11.2's error screen comes when the shell leaves the kernel (plan 4). `kernel_main` now builds the `MountTable` itself and hands it to `session::run_shell`. The new startup line lands with everything that expects it: the `boot` scenario, the new `system`, `system_missing` and `system_abi` scenarios, `mount_fail` (its empty read-only root now lists `bin`, Task 5), `check3-a.sh`'s `dmesg` expectations and the two recorded transcripts of it, which get the line inserted (decision 8).

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/session.rs`
- Create: `kernel/src/system.rs`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/mount_fail.txt`
- Create: `tests/e2e/system.txt`
- Create: `tests/e2e/system_abi.txt`
- Create: `tests/e2e/system_missing.txt`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`

**Interfaces:**
- Consumes: Tasks 4 (`SysImgFs`), 5, 11 (`BootInfo::system_image`), 12 (the steps).
- Produces: `relay_kernel::system::{mount(&BootInfo, &mut MountTable), mount_into(&mut MountTable, &'static [u8]) -> Result<String, SystemError>, open, in_loader_memory, SystemError}`; `session::run_shell(MountTable, bool)`; the `[ ok ] system:` startup line.

- [ ] **Step 1: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
crc32.workspace = true
x86_64.workspace = true
````

with:

````toml
crc32.workspace = true
relay-abi.workspace = true
sysimg.workspace = true
x86_64.workspace = true
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod storage;
pub mod timer;
````

with:

````rust
pub mod storage;
pub mod system;
pub mod timer;
````

- [ ] **Step 3: Write the failing tests for `kernel/src/system.rs`**

Create `kernel/src/system.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use vfs::{Env, FileSystem, MemFs, Vfs};

    struct Clock;

    impl Env for Clock {
        fn now(&self) -> u64 {
            0
        }
        fn log(&self, _: &str) {}
    }

    fn archive(abi: u32, names: &[&str]) -> &'static [u8] {
        let entries: Vec<sysimg::Entry<'_>> = names
            .iter()
            .map(|n| sysimg::Entry {
                name: n.as_bytes(),
                mode: 0o755,
                data: b"\x7fELF",
            })
            .collect();
        Vec::leak(sysimg::write(abi, 0, &entries).unwrap())
    }

    fn table(with_bin: bool) -> MountTable {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        if with_bin {
            fs.mkdir(root, b"bin").unwrap();
        }
        MountTable::new(Box::new(fs))
    }

    fn region(start: u64, end: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion {
            start,
            len: end - start,
            kind,
        }
    }

    #[test]
    fn the_archive_is_mounted_at_bin() {
        let mut vfs = table(true);
        let text = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args", "cat"])).unwrap();
        assert_eq!(text, "2 programs, ABI 1");
        let node = vfs.lookup(b"/bin/t-args").unwrap();
        assert_eq!(vfs.stat(node).unwrap().perm, 0o755);
        let mut one = table(true);
        assert_eq!(
            mount_into(&mut one, archive(1, &["t-args"])).unwrap(),
            "1 program, ABI 1"
        );
    }

    #[test]
    fn a_root_without_bin_gets_one() {
        let mut vfs = table(false);
        mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args"])).unwrap();
        assert!(vfs.lookup(b"/bin/t-args").is_ok());
    }

    #[test]
    fn a_bad_archive_is_not_mounted() {
        let mut vfs = table(true);
        let junk: &'static [u8] =
            Vec::leak(b"this is not an archive, but long enough to have a header....".repeat(2));
        let e = mount_into(&mut vfs, junk).unwrap_err();
        assert_eq!(e.to_string(), "system.img: not a system archive");
        let e = mount_into(&mut vfs, archive(99, &["t-args"])).unwrap_err();
        assert_eq!(e, SystemError::Abi(99));
        assert_eq!(e.to_string(), "ABI 99, kernel wants 1");
        assert_eq!(
            vfs.lookup(b"/bin/t-args"),
            Err(Errno::ENOENT),
            "nothing mounted"
        );
    }

    #[test]
    fn a_bin_that_is_a_file_is_reported() {
        let mut fs = MemFs::new(Box::new(Clock));
        let root = fs.root();
        fs.create(root, b"bin").unwrap();
        let mut vfs = MountTable::new(Box::new(fs));
        let e = mount_into(&mut vfs, archive(relay_abi::VERSION, &["t-args"])).unwrap_err();
        assert_eq!(e.to_string(), "cannot mount /bin: Not a directory");
    }

    #[test]
    fn the_other_messages() {
        assert_eq!(SystemError::Missing.to_string(), "no system.img");
        assert_eq!(
            SystemError::NotLoaderMemory {
                phys: 0x10_0000,
                len: 5
            }
            .to_string(),
            "system.img at 0x100000 (5 bytes) is not loader memory"
        );
    }

    #[test]
    fn the_archive_must_lie_in_loader_memory() {
        use MemoryKind::*;
        let map = [
            region(0x1000, 0x8000, Usable),
            region(0x8000, 0x10000, Bootloader),
            region(0x10000, 0x20000, Bootloader),
            region(0x20000, 0x30000, Usable),
            region(0x40000, 0x50000, Bootloader),
        ];
        assert!(in_loader_memory(&map, 0x8000, 0x8000));
        assert!(
            in_loader_memory(&map, 0x9000, 0x10000),
            "across two loader regions"
        );
        assert!(in_loader_memory(&map, 0x40000, 0x10000));
        assert!(
            !in_loader_memory(&map, 0x7000, 0x2000),
            "starts in usable memory"
        );
        assert!(
            !in_loader_memory(&map, 0x1f000, 0x2000),
            "ends in usable memory"
        );
        assert!(!in_loader_memory(&map, 0x1f000, 0x30000), "spans a gap");
        assert!(!in_loader_memory(&map, 0x60000, 1), "outside the map");
        assert!(!in_loader_memory(&map, u64::MAX - 1, 0x10), "wraps around");
    }
}
````

- [ ] **Step 4: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
#> ...
````

with:

````bash
#nuc> \[ ok \] mount /: ext2 on 00:14\.0 port 15 partition 2, 14\.3 GiB
#> \[ ok \] system: \d+ programs?, ABI 1
#> ...
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# step reports ok, / is the stick's ext2 partition, and the shell shows the
# motd and starts in /root (spec §9.3 #1).
timeout 30
````

with:

````text
# step reports ok, / is the stick's ext2 partition, and the shell shows the
# motd and starts in /root (spec §9.3 #1), with the system archive at /bin.
timeout 30
````

Replace:

````text
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
expect \nWelcome to Relay OS\.\n
````

with:

````text
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
# The programs of /bin, from \EFI\RELAY\system.img (user-space gate §4.3).
expect \[ ok \] system: \d+ programs?, ABI 1
expect \nWelcome to Relay OS\.\n
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/mount_fail.txt`**

In `tests/e2e/mount_fail.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# boot, so mounting / fails with its reason on the status line, and the
# shell starts on the empty read-only /. Afterwards the runner puts the
# magic back and e2fsck finds the filesystem untouched.
````

with:

````text
# boot, so mounting / fails with its reason on the status line, and the
# shell starts on the empty read-only /, where only the programs of /bin
# are (user-space gate spec §4.4). Afterwards the runner puts the
# magic back and e2fsck finds the filesystem untouched.
````

Replace:

````text
send ls -a /
expect \n\.\s+\.\.\n
send touch /f
````

with:

````text
send ls -a /
expect \n\.\s+\.\.\s+bin\n
send touch /f
````

- [ ] **Step 7: Add the scenario `tests/e2e/system.txt`**

Create `tests/e2e/system.txt`:

````text
# /bin is the system archive the loader read from the ESP (user-space gate
# spec §4.3, §4.4): its programs are listed and read like other files, and
# nothing in it can be changed or removed.
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 1
expect root@relay:~# $
send ls /bin
expect \nt-args\n
send ls -l /bin/t-args
expect \n-rwxr-xr-x 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d /bin/t-args\n
send cp /bin/t-args /root/t-args
send ls -l /root/t-args
expect \n-rw-r--r-- 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d /root/t-args\n
send touch /bin/t-args
expect \ntouch: cannot touch '/bin/t-args': Read-only file system\n
send rm /bin/t-args
expect \nrm: cannot remove '/bin/t-args': Read-only file system\n
send echo x > /bin/new
expect \nrelay-sh: /bin/new: Read-only file system\n
send rmdir /bin
expect \nrmdir: failed to remove '/bin': Device or resource busy\n
send mv /bin /root/bin
expect \nmv: cannot move '/bin' to '/root/bin': Device or resource busy\n
send cd /bin
send pwd
expect \n/bin\n
send cd ..
send ls /bin
expect \nt-args\n
````

- [ ] **Step 8: Add the scenario `tests/e2e/system_abi.txt`**

Create `tests/e2e/system_abi.txt`:

````text
# An archive built for another ABI is refused before anything runs from it
# (user-space gate spec §4.3, §7.4), and the line names both versions.
system-abi 99
timeout 30
expect \[FAIL\] system: ABI 99, kernel wants 1
expect root@relay:~# $
send ls /bin
send echo done
expect \ndone\n
````

- [ ] **Step 9: Add the scenario `tests/e2e/system_missing.txt`**

Create `tests/e2e/system_missing.txt`:

````text
# Without \EFI\RELAY\system.img the startup line says so and the shell
# runs on, with nothing in /bin (user-space gate spec §4.3).
esp-delete /EFI/RELAY/system.img
timeout 30
expect \[ ok \] mount /: ext2 on .*
expect \[FAIL\] system: no system\.img
expect root@relay:~# $
send ls /bin
send echo done
expect \ndone\n
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib system`

Expected: FAIL: compile errors such as `` unresolved import `MemoryKind` ``.

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 11: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    let root = storage::mount_root(info.boot_partition_guid());
    session::run_shell(root, cmdline.test_mode)
}
````

with:

````rust
    let root = storage::mount_root(info.boot_partition_guid());
    let mut vfs = vfs::MountTable::new(root);
    system::mount(info, &mut vfs);
    session::run_shell(vfs, cmdline.test_mode)
}
````

- [ ] **Step 12: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::{arch, console, klog, klogln, power, rtc, serial, usb};
use alloc::boxed::Box;
use alloc::vec::Vec;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, FileSystem, MountTable};

````

with:

````rust
use crate::{arch, console, klog, klogln, power, rtc, serial, usb};
use alloc::vec::Vec;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, MountTable};

````

Replace:

````rust

/// Runs the shell with `root` mounted at `/` (spec §4.4 step 10). Never
/// returns.
pub fn run_shell(root: Box<dyn FileSystem>, test_mode: bool) -> ! {
    let mut vfs = MountTable::new(root);
    let mut console = KernelConsole::new();
````

with:

````rust

/// Runs the shell over `vfs`, the root at `/` and the programs at `/bin`
/// (spec §4.4 step 11). Never returns.
pub fn run_shell(mut vfs: MountTable, test_mode: bool) -> ! {
    let mut console = KernelConsole::new();
````

- [ ] **Step 13: Implement `kernel/src/system.rs`**

Insert this at the top of `kernel/src/system.rs`, above `#[cfg(test)]`:

````rust
//! The system archive (spec §4.3 of the user-space gate): the programs the
//! loader read from `\EFI\RELAY\system.img`, checked and mounted read-only
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 29 programs, ABI 1` or `[FAIL] system: <reason>`; without an archive the
//! shell runs on, with nothing in `/bin`.

use crate::console;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use boot_info::{BootInfo, MemoryKind, MemoryRegion, PHYS_OFFSET};
use core::fmt;
use sysimg::{SysImgError, SysImgFs};
use vfs::{Errno, MountTable};

#[derive(Debug, PartialEq, Eq)]
pub enum SystemError {
    /// The loader found no `system.img`.
    Missing,
    /// The loader's range is not memory it allocated.
    NotLoaderMemory {
        phys: u64,
        len: u64,
    },
    Bad(SysImgError),
    /// Built for another ABI than this kernel's.
    Abi(u32),
    Mount(Errno),
}

impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SystemError::Missing => write!(f, "no system.img"),
            SystemError::NotLoaderMemory { phys, len } => {
                write!(
                    f,
                    "system.img at {phys:#x} ({len} bytes) is not loader memory"
                )
            }
            SystemError::Bad(e) => write!(f, "system.img: {e}"),
            SystemError::Abi(abi) => {
                write!(f, "ABI {abi}, kernel wants {}", relay_abi::VERSION)
            }
            SystemError::Mount(e) => write!(f, "cannot mount /bin: {}", e.message()),
        }
    }
}

/// `phys..phys + len` lies in `Bootloader` regions only (the map is sorted
/// by start; neighbouring regions may split the range).
pub fn in_loader_memory(map: &[MemoryRegion], phys: u64, len: u64) -> bool {
    let Some(end) = phys.checked_add(len) else {
        return false;
    };
    let mut covered = phys;
    for r in map {
        if covered >= end {
            break;
        }
        if r.kind == MemoryKind::Bootloader && r.start <= covered && covered < r.end() {
            covered = r.end();
        }
    }
    covered >= end
}

/// Checks the archive and its ABI version.
pub fn open(bytes: &'static [u8]) -> Result<SysImgFs, SystemError> {
    let fs = SysImgFs::new(bytes).map_err(SystemError::Bad)?;
    match fs.archive().abi() {
        relay_abi::VERSION => Ok(fs),
        other => Err(SystemError::Abi(other)),
    }
}

/// Opens the archive in `bytes` and mounts it at `/bin`; returns the text
/// of the startup line.
pub fn mount_into(vfs: &mut MountTable, bytes: &'static [u8]) -> Result<String, SystemError> {
    let fs = open(bytes)?;
    let n = fs.archive().len();
    let text = format!(
        "{n} program{}, ABI {}",
        if n == 1 { "" } else { "s" },
        relay_abi::VERSION
    );
    vfs.mount(b"/bin", Box::new(fs))
        .map_err(SystemError::Mount)?;
    Ok(text)
}

/// Startup step 10: mounts the loader's archive at `/bin` and prints the
/// `system` line.
pub fn mount(info: &BootInfo, vfs: &mut MountTable) {
    let result = (|| {
        let (phys, len) = info.system_image().ok_or(SystemError::Missing)?;
        // SAFETY: built by relay-boot, lives forever.
        let map = unsafe { info.memory_map() };
        if !in_loader_memory(map, phys, len) {
            return Err(SystemError::NotLoaderMemory { phys, len });
        }
        // SAFETY: loader memory, in the linear map and never handed out
        // (`mm::frame`), so it stays as the loader left it.
        let bytes =
            unsafe { core::slice::from_raw_parts((PHYS_OFFSET + phys) as *const u8, len as usize) };
        mount_into(vfs, bytes)
    })();
    match result {
        Ok(text) => console::ok(format_args!("system: {text}")),
        Err(e) => console::fail("system", format_args!("{e}")),
    }
}

````

- [ ] **Step 14: Put the new line into the recorded NUC transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if '] mount /: ext2 on' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ['[\x1b[32m ok \x1b[0m] system: 1 program, ABI 1']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 15: Put the new line into the recorded QEMU transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.qemu.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if '] mount /: ext2 on' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ['[ ok ] system: 1 program, ABI 1']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib system`

Expected: PASS: 7 tests.

Run: `cargo test -p xtask checks`

Expected: PASS: 11 tests.

- [ ] **Step 17: Run the `boot`, `system`, `system_missing`, `system_abi`, `mount_fail`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_abi`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario mount_fail`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 18: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 19: Commit**

````bash
git add Cargo.lock kernel rootfs tests xtask
git commit -m "kernel: the system archive is checked and mounted at /bin, with its startup line"
````


### Task 14: An empty `system.img` is reported as empty

Review finding (minor): a copy cut short on the FAT can leave `\EFI\RELAY\system.img` with no bytes, and the loader treated that like no file at all, so the screen said `[FAIL] system: no system.img` although the file is there; a 7-byte one says `system.img: only 7 bytes`. The loader now hands over any file it could read, an empty one in a page of its own, and `BootInfo::system_image` tells a file (an address) from none (address 0), so the line says `[FAIL] system: system.img: only 0 bytes`. The scenario `system_empty` writes an empty archive to the ESP.

**Files:**
- Modify: `boot/src/main.rs`
- Modify: `crates/boot-info/src/lib.rs`
- Modify: `kernel/src/system.rs`
- Create: `tests/e2e/system_empty.txt`

**Interfaces:**
- Consumes: Tasks 11 and 13.
- Produces: `BootInfo::system_image()` is `Some` whenever `system_image_phys` is not 0, even with length 0.

- [ ] **Step 1: Add the failing tests to `crates/boot-info/src/lib.rs`**

In `crates/boot-info/src/lib.rs`, replace:

````rust
    #[test]
    fn the_system_image_is_there_when_it_has_a_length() {
        let mut bi = blank();
        assert_eq!(bi.system_image(), None);
        bi.system_image_phys = 0x1234_5000;
        assert_eq!(bi.system_image(), None, "no length, no file");
        bi.system_image_len = 23_784;
        assert_eq!(bi.system_image(), Some((0x1234_5000, 23_784)));
    }
````

with:

````rust
    #[test]
    fn the_system_image_is_there_when_it_has_an_address() {
        let mut bi = blank();
        assert_eq!(bi.system_image(), None);
        bi.system_image_len = 23_784;
        assert_eq!(bi.system_image(), None, "no address, no file");
        bi.system_image_phys = 0x1234_5000;
        assert_eq!(bi.system_image(), Some((0x1234_5000, 23_784)));
        bi.system_image_len = 0;
        assert_eq!(bi.system_image(), Some((0x1234_5000, 0)), "an empty file");
    }
````

- [ ] **Step 2: Add the failing tests to `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
        let mut vfs = table(true);
        let junk: &'static [u8] =
````

with:

````rust
        let mut vfs = table(true);
        let e = mount_into(&mut vfs, &[]).unwrap_err();
        assert_eq!(e.to_string(), "system.img: only 0 bytes", "an empty file");
        let junk: &'static [u8] =
````

- [ ] **Step 3: Add the scenario `tests/e2e/system_empty.txt`**

Create `tests/e2e/system_empty.txt`:

````text
# An empty \EFI\RELAY\system.img (a copy cut short) is named as such, not
# reported missing (user-space gate spec §4.3).
esp-write /EFI/RELAY/system.img
timeout 30
expect \[FAIL\] system: system\.img: only 0 bytes
expect root@relay:~# $
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p boot-info`

Expected: FAIL: 1 test fails: `tests::the_system_image_is_there_when_it_has_an_address`.

Run: `cargo xtask test --e2e-only --scenario system_empty`

Expected: FAIL: scenario `system_empty` stops at line 5, timed out waiting for `\[FAIL\] system: system\.img: only 0 bytes`.

- [ ] **Step 5: Change `boot/src/main.rs`**

In `boot/src/main.rs`, replace:

````rust
/// kernel keeps (spec §4.3 of the user-space gate), and returns their
/// physical address and the file's length; `(0, 0)` without the file. The
/// kernel checks the contents.
fn load_system_image() -> (u64, u64) {
    let Some(file) = read_file(SYSTEM_PATH).filter(|f| !f.is_empty()) else {
        return (0, 0);
    };
    let base = alloc_pages(MemoryType::LOADER_DATA, file.len().div_ceil(4096));
    unsafe { core::ptr::copy_nonoverlapping(file.as_ptr(), base, file.len()) };
````

with:

````rust
/// kernel keeps (spec §4.3 of the user-space gate), and returns their
/// physical address and the file's length; `(0, 0)` if the file cannot be
/// read. An empty file gets a page, so the kernel can tell it from none.
/// The kernel checks the contents.
fn load_system_image() -> (u64, u64) {
    let Some(file) = read_file(SYSTEM_PATH) else {
        return (0, 0);
    };
    let base = alloc_pages(MemoryType::LOADER_DATA, file.len().div_ceil(4096).max(1));
    unsafe { core::ptr::copy_nonoverlapping(file.as_ptr(), base, file.len()) };
````

- [ ] **Step 6: Change `crates/boot-info/src/lib.rs`**

In `crates/boot-info/src/lib.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// loader read into `LOADER_DATA` pages (`Bootloader` memory, which the
    /// kernel never hands out). Length 0: there was no such file.
    pub system_image_phys: u64,
````

with:

````rust
    /// loader read into `LOADER_DATA` pages (`Bootloader` memory, which the
    /// kernel never hands out). Address 0: the loader could not read such a
    /// file; an empty file has an address and length 0.
    pub system_image_phys: u64,
````

Replace:

````rust
    /// `(physical address, length)` of the system archive, if the loader
    /// found one.
    pub fn system_image(&self) -> Option<(u64, u64)> {
        (self.system_image_len != 0).then_some((self.system_image_phys, self.system_image_len))
    }
````

with:

````rust
    /// `(physical address, length)` of the system archive, if the loader
    /// read one (perhaps an empty one).
    pub fn system_image(&self) -> Option<(u64, u64)> {
        (self.system_image_phys != 0).then_some((self.system_image_phys, self.system_image_len))
    }
````

- [ ] **Step 7: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
pub enum SystemError {
    /// The loader found no `system.img`.
    Missing,
````

with:

````rust
pub enum SystemError {
    /// The loader could not read a `system.img`.
    Missing,
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p boot-info`

Expected: PASS: 8 tests.

Run: `cargo test -p relay-kernel --lib system`

Expected: PASS: 7 tests.

- [ ] **Step 9: Run the `system_empty`, `system_missing` scenarios**

Run: `cargo xtask test --e2e-only --scenario system_empty`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario system_missing`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add boot crates kernel tests
git commit -m "boot, kernel: an empty system.img is reported as empty, not missing"
````


### Task 15: NUC check 3 lists `/bin`; the README and the checklist

The NUC check of this plan runs from the check scripts, as in milestone 1: `check3-a.sh` gains `ls -l /bin`, which must list `t-args` as `-rwxr-xr-x 1 root root <size> <date> t-args` (with `#> ...` around it, so the programs later plans add do not break it), and the recorded transcripts get the lines a correct run prints (decision 8), taken from the `checks` scenario's transcript. `docs/hardware-test.md` check 3 names the new startup line and the command, its count becomes 64 commands, and its table says what a failed `system` line means; the README describes the new crates, `userland/`, `system.img` and the two host tools it needs (`binutils`, `linux-libc-dev`). The review's text findings are fixed here too: `flash --kernel`'s comment and message name `system.img`; the NUC transcript test's comment says which lines were put in by hand; the mount table's comment says where a mount on a name happens (the images xtask writes have a `/bin`, so only the fallback root needs it); and the progress squares' table and the loader say that a stop at square 3 can be the `system.img` read.

**Files:**
- Modify: `README.md`
- Modify: `boot/src/main.rs`
- Modify: `crates/vfs/src/mount.rs`
- Modify: `docs/hardware-test.md`
- Modify: `rootfs/root/checks/check3-a.sh`
- Modify: `xtask/fixtures/checks/check3-a.nuc.log`
- Modify: `xtask/fixtures/checks/check3-a.qemu.log`
- Modify: `xtask/src/checks.rs`
- Modify: `xtask/src/flash.rs`

**Interfaces:**
- Consumes: Task 13.
- Produces: `check3-a.sh` with `ls -l /bin` (64 commands).

- [ ] **Step 1: Change `rootfs/root/checks/check3-a.sh`**

In `rootfs/root/checks/check3-a.sh`, replace:

````bash
#!> \[FAIL\].*

````

with:

````bash
#!> \[FAIL\].*

# The programs of /bin: the system archive the loader read from the ESP.
ls -l /bin
#> total \d+
#> ...
#> -rwxr-xr-x 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d t-args
#> ...

````

- [ ] **Step 2: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
    /// full hardware checklist of 2026-09-29 (`docs/hardware-test.md`),
    /// copied off the stick unchanged. A `#nuc>` line must not need a line
    /// of its own next to the `#>` line for the same output, which QEMU
````

with:

````rust
    /// full hardware checklist of 2026-09-29 (`docs/hardware-test.md`),
    /// copied off the stick, with the lines of `check3-a.sh`'s `system`
    /// startup line and `ls -l /bin` put in by hand until the next NUC check
    /// records them (milestone 2, plan 1). A `#nuc>` line must not need a line
    /// of its own next to the `#>` line for the same output, which QEMU
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask checks`

Expected: FAIL: 1 test fails: `checks::tests::the_check_scripts_pass_on_both_machines`.

- [ ] **Step 4: Change `README.md`**

In `README.md`, make these 5 replacements, top to bottom:

Replace:

````markdown
Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`

````

with:

````markdown
Design: `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`
(milestone 1) and `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`
(milestones 2 and 3: the shell and its commands as programs in ring 3).

````

Replace:

````markdown

After a code change, `cargo xtask flash --kernel` replaces only the loader
and the kernel and keeps the files on the stick.

## Requirements (host)

Linux with `rustup`, `qemu-system-x86_64`, OVMF (`/usr/share/OVMF`),
`mtools`, `e2fsprogs`, `util-linux` (`sfdisk`) and `udisks2`. The Rust
toolchain and targets are installed automatically from `rust-toolchain.toml`.

````

with:

````markdown

After a code change, `cargo xtask flash --kernel` replaces only the loader,
the kernel and the programs of `/bin` (`system.img`) and keeps the files on
the stick.

## Requirements (host)

Linux with `rustup`, `qemu-system-x86_64`, OVMF (`/usr/share/OVMF`),
`mtools`, `e2fsprogs`, `util-linux` (`sfdisk`), `udisks2`, `binutils`
(`readelf` checks every user program) and `linux-libc-dev` (the tests check
the error numbers against Linux's headers). The Rust toolchain and targets
are installed automatically from `rust-toolchain.toml`.

````

Replace:

````markdown
| `cargo xtask flash --full` | Erase and write the Kingston test stick |
| `cargo xtask flash --kernel` | Update loader and kernel on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick, list its files and check the transcripts of the check scripts |
````

with:

````markdown
| `cargo xtask flash --full` | Erase and write the Kingston test stick |
| `cargo xtask flash --kernel` | Update loader, kernel and `system.img` on the stick, keep files |
| `cargo xtask verify-usb` | `e2fsck` the stick, list its files and check the transcripts of the check scripts |
````

Replace:

````markdown
|---|---|
| `boot/` | `relay-boot`, the UEFI loader (`BOOTX64.EFI`) |
| `kernel/` | `relay-kernel`, the higher-half kernel |
````

with:

````markdown
|---|---|
| `boot/` | `relay-boot`, the UEFI loader (`BOOTX64.EFI`); it also loads `system.img` |
| `kernel/` | `relay-kernel`, the higher-half kernel |
````

Replace:

````markdown
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
````

with:

````markdown
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `crates/heap` | The heap allocator of the kernel (and, later, of user programs) |
| `crates/crc32` | CRC-32, for GPT and `system.img` |
| `crates/relay-abi` | The system-call ABI: version, call numbers, error numbers, result encoding |
| `crates/relay-rt` | The runtime of user programs: entry, arguments, system calls, panic handler, ABI note, linker script |
| `crates/sysimg` | The `system.img` archive: format, writer, reader, and `SysImgFs`, mounted at `/bin` |
| `userland/` | User programs: `tests/` holds the `t-*` test programs |
| `xtask/` | Build, image, QEMU, test and flash tool; it builds `userland/`, checks each program with `readelf` and packs `system.img` |
| `rootfs/` | Files copied into `/`, among them the NUC check scripts in `root/checks/` |
````

- [ ] **Step 5: Change `boot/src/main.rs`**

In `boot/src/main.rs`, replace:

````rust
    drop(kernel_file);
    let (system_phys, system_len) = load_system_image();
````

with:

````rust
    drop(kernel_file);
    // Between squares 3 and 4: a stop at square 3 can be this read.
    let (system_phys, system_len) = load_system_image();
````

- [ ] **Step 6: Change `crates/vfs/src/mount.rs`**

In `crates/vfs/src/mount.rs`, replace:

````rust
/// A directory of the filesystem below, or a name that directory does not
/// have (spec §4.4 of the user-space gate: `/bin` on a root without one).
/// A mount on a name is shown in its directory like any other, and the
````

with:

````rust
/// A directory of the filesystem below, or a name that directory does not
/// have (spec §4.4 of the user-space gate: `/bin` on a root without one,
/// such as the empty read-only root the kernel falls back to; the images
/// xtask writes have a `/bin`).
/// A mount on a name is shown in its directory like any other, and the
````

- [ ] **Step 7: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 6 replacements, top to bottom:

Replace:

````markdown
| 2 | orange | boot partition and ACPI lookups done |
| 3 | yellow | kernel file loaded |
| 4 | green | page tables for RAM and screen built |
````

with:

````markdown
| 2 | orange | boot partition and ACPI lookups done |
| 3 | yellow | kernel file loaded; next is reading `system.img` |
| 4 | green | page tables for RAM and screen built |
````

Replace:

````markdown

## Check 3 — files on the stick (plans 5 and 6)

````

with:

````markdown

## Check 3 — files on the stick (plans 5 and 6; milestone 2)

````

Replace:

````markdown
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
````

with:

````markdown
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`
   - `[ ok ] system: 1 program, ABI 1` (milestone 2: the programs of `/bin`
     from `\EFI\RELAY\system.img`; the count grows as later plans add
     programs)
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
````

Replace:

````markdown
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), the `fileops` scenario's operations on `/root/notes` (`mkdir -p`,
   `echo >`/`>>`, `cat`, `ls -l`, `cp`, `mv`, `rmdir` of a full directory,
````

with:

````markdown
   `uname -a`, `date`, `dmesg` (every startup line of checks 1-3, checked
   later), `ls -l /bin` (the programs of the system archive), the `fileops`
   scenario's operations on `/root/notes` (`mkdir -p`,
   `echo >`/`>>`, `cat`, `ls -l`, `cp`, `mv`, `rmdir` of a full directory,
````

Replace:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 63 of 63 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

with:

````markdown
   `/root/notes/t`, and the last lines are
   `/root/checks/check3-a.sh: ok, 64 of 64 commands as expected (run <time>)`
   and `/root/checks/check3-b.sh: ok, 5 of 5 commands as expected (run
````

Replace:

````markdown
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
````

with:

````markdown
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] system: no system.img` | The loader could not read `\EFI\RELAY\system.img` (it is missing, or the FAT is damaged) | `cargo xtask flash --kernel` writes it with the loader and the kernel |
| `[FAIL] system: ABI N, kernel wants M` or `[FAIL] system: system.img: …` | The archive on the ESP is from another build, or damaged | `cargo xtask flash --kernel` from the same worktree as the kernel |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
````

- [ ] **Step 8: Put the command's lines into the recorded NUC transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.nuc.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if '] system: 1 program, ABI 1' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ['+ ls -l /bin', 'total 20', '-rwxr-xr-x 1 root root 19688 Sep 29 08:34 t-args']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 9: Put the command's lines into the recorded QEMU transcript**

Run:

````bash
python3 - <<'EOF'
path = 'xtask/fixtures/checks/check3-a.qemu.log'
lines = open(path, newline="").read().split("\n")
at = [i for i, l in enumerate(lines) if '] system: 1 program, ABI 1' in l]
assert len(at) == 1, at
lines[at[0] + 1:at[0] + 1] = ['+ ls -l /bin', 'total 20', '-rwxr-xr-x 1 root root 19688 Sep 29 08:34 t-args']
open(path, "w", newline="").write("\n".join(lines))
EOF
````

- [ ] **Step 10: Change `xtask/src/flash.rs`**

In `xtask/src/flash.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// `flash --kernel`: replace loader, kernel and cmdline on the ESP only.
pub fn flash_kernel(art: &Artifacts, cmdline: &str) -> Result<()> {
````

with:

````rust

/// `flash --kernel`: replace loader, kernel, `system.img` and cmdline on the
/// ESP only.
pub fn flash_kernel(art: &Artifacts, cmdline: &str) -> Result<()> {
````

Replace:

````rust
    println!(
        "updated loader and kernel on {} ({})",
        stick.dev.display(),
````

with:

````rust
    println!(
        "updated loader, kernel and system.img on {} ({})",
        stick.dev.display(),
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p xtask checks`

Expected: PASS: 11 tests.

- [ ] **Step 12: Run the `checks` scenario**

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add README.md boot crates docs rootfs xtask
git commit -m "docs, checks: NUC check 3 lists /bin, and the README describes the system archive"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 24 scenario(s) passed`.

````bash
git push -u origin m2p1/system
gh pr create --base main --head m2p1/system --title "Milestone 2, plan 1: /bin on the NUC" --body-file - <<'EOF'
## What

Milestone 2, plan 1, tasks 11–15: `BootInfo` version 2 carries `system.img`, which `relay-boot` loads; the kernel checks the archive (loader memory, format, ABI) and mounts it at `/bin`, with the startup line `[ ok ] system: …` or `[FAIL] system: …`; the e2e steps `esp-delete` and `system-abi` and the scenarios `system`, `system_missing`, `system_abi` and `system_empty`; `check3-a.sh` checks the new line and lists `/bin`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 3 of `docs/hardware-test.md`, run by the user with `cargo xtask flash --full` from this worktree; the result goes into the results log, and the recorded transcripts are replaced by the real ones
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m2p1/system --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m2p1-system
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
