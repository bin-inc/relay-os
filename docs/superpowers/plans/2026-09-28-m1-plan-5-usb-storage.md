# Milestone 1 · Plan 5: USB Storage and `/` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Files that live on the stick: bulk transfers on the xHCI driver, the USB mass-storage driver (Bulk-Only Transport and SCSI with the setup, I/O and error recovery of spec §6.4), the kernel's `block` module (GPT with CRC32 and the backup header, partitions, the root partition chosen by the boot partition's GUID, spec §6.5), ext2 mounted at `/` at startup with the read-only retry of spec §10, and `reboot` and `poweroff` through ACPI with test mode's exit (spec §7.4). It ends with every QEMU scenario of spec §9.3 green, each followed by `e2fsck`, and NUC check 3, the full hardware checklist of spec §9.4.

**Architecture:** The storage driver lives in the `usb` crate next to the keyboard: it talks to the controller only through `Bus`, which gains bulk transfers that are waited for, and `Host` claims mass-storage interfaces and offers each disk under an id that is never reused. Its tests run it over the real xHCI driver against the fake controller and a new fake storage device, both as strict as hardware and modelled on the Kingston stick and QEMU's `usb-storage`. The kernel wraps each USB disk as a `vfs::BlockDevice` that locks the controllers per request; GPT parsing, partitions, the root choice, the mount fallback and the ACPI register values are pure kernel logic, host-tested against disks partitioned by the real `sfdisk`, `fdisk` and `mke2fs`. The port writes of `reboot` and `poweroff` are thin glue. QEMU checks the whole path end to end, with a runner that can reboot, switch off and read files back from the disk.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates (the kernel gains the workspace crate `ext2`); QEMU 8.2 (`qemu-xhci`, `usb-storage`, `isa-debug-exit`); e2fsprogs 1.47 (`e2fsck`, `debugfs`, `mke2fs`) and util-linux (`sfdisk`, `fdisk`) for tests.

**Spec:** `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md` (§2, §3, §4.1, §4.4, §5.4, §6.2, §6.4, §6.5, §7.4, §8.3, §9.1–9.4, §10, §12, §13, §15)
**Roadmap:** `docs/superpowers/plans/2026-09-26-milestone-1-roadmap.md` — this is plan 5 of 6.

## Where this plan fits

Plan 5 implements spec §11 step 6. It builds on plan 4's `usb` crate (`Hal`, `Bus`, the xHCI driver, `Host`, the fake controller and its Kingston stick) and kernel glue (`usb::HOSTS`, the console's polling), plan 3's `ext2`, `vfs` and `shell` crates (the per-command sync and the clean shutdown before `reboot`/`poweroff`), and plan 2's ACPI tables (the FADT reset register, PM1 control, `\_S5`). Plan 6 is hardening: fixes from NUC check 3, the deferred review findings, the README quick start and a final spec/code consistency pass.

## Working conventions

- Plan 5 lands as **six pull requests** (table below). This plan itself is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-28-m1-plan-5-usb-storage.md`, to every task, and all tasks share one workspace directory.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module; test support under `crates/usb/src/testing/` and `kernel/src/block/testing.rs`, which are compiled only for tests; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats (it throttled at 99 °C during plan 4) before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `ee0f0bb` and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `plan5/plan` | — | This plan, spec §15 item 11, roadmap notes | `lint`, `unit`, `e2e` |
| 2 | `plan5/bulk` | 1–3 | Bulk IN and OUT transfers of up to 64 KiB with the 5 s timeout and a safe abort; halts cleared with the data toggle reset; EP0 kept out of use after a failed abort; the stricter fake controller | `lint`, `unit`, `e2e` |
| 3 | `plan5/mass-storage` | 4–9 | BOT and SCSI: wrappers, setup, I/O, recovery; `Host` with disks and setup retries; requests never reaching a replugged device; a hung disk given up; the fake storage device | `lint`, `unit`, `e2e` |
| 4 | `plan5/block` | 10–11 | CRC32, GPT with the backup header, partitions, the root choice | `lint`, `unit`, `e2e` |
| 5 | `plan5/power` | 12–17 | `reboot` and `poweroff` through ACPI, test mode's exit, aligned registers, PM1 write order; the e2e `reboot` and `poweroff` steps with the reset method and the clean flag checked, `e2fsck` after every scenario | `lint`, `unit`, `e2e` |
| 6 | `plan5/root` | 18–22 | `/` on the USB stick, chosen by the boot partition's GUID; the `fileops`, `persist`, `display`, `bigfile`, `diskfull` and `mount_fail` scenarios; NUC check 3 | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; any assembly is inline (plan 5 adds one `int3` for the triple fault).
- Crate policy (spec §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates: the kernel gains `ext2`); `xtask/` and dev-dependencies may use anything. `crates/usb` has no dependencies at all; the kernel has no dev-dependencies (CRC32 is written here).
- The kernel stack is 64 KiB (spec §4.2): no big arrays on the stack; buffers are `Vec`/`Box` or DMA memory. A bulk transfer's 64 KiB lives in its endpoint's DMA buffer.
- Errors are values (spec §10): device failures are `UsbError`, `IoError` or `Errno`, never panics; nothing waits forever (control transfers 1 s, bulk transfers 5 s, TEST UNIT READY 5 s, three tries per command); `poll` never waits. Nothing a device, a disk's contents, a controller or the firmware sends may make an `unwrap`, an index or an arithmetic overflow fail (the kernel builds with overflow checks), or allocate without bound. A device-level failure prints `[FAIL] <step>: <reason>` and the boot continues. Panics are for kernel bugs only.
- Memory the controller may still write is never freed or reused (plan 4, decision 6): after a transfer times out its endpoint is stopped and repositioned before the buffer is used again, and an endpoint whose abort failed is not used until `clear_halt` succeeded.
- The NUC has no serial port: every storage step logs to `dmesg` (`storage: slot N: …`), every failure with its SCSI sense; the screen shows one line per device (`usb: <pci> port N: …, disk <vendor> <product>, <size>`) and the `mount /` status line.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action in the workflow stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: MMIO, DMA and port I/O code may raise alerts; alerts 1–10 (`kernel/src/mm/heap.rs`) are known false positives left open for plan 6; triage each new one, and ask the user before dismissing any.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 11 in PR 1:

1. **Bulk transfers are waited for** (refines §6.2 and plan 4's `Bus`). `Bus::bulk_in` and `bulk_out` block like `control`, for 5 s at most; a storage request blocks by nature. Every bulk endpoint has a 64 KiB buffer aligned to 64 KiB, so one Normal TRB covers any transfer without crossing a 64 KiB boundary (xHCI 6.4.1.1). A transfer on a halted endpoint fails with `Stall` at once, since the controller ignores its doorbell. A transfer that times out is aborted (Stop Endpoint, Set TR Dequeue Pointer) before the call returns; if the abort fails, the endpoint takes no transfer until `clear_halt` has repositioned its ring, and its buffer is freed only with its slot. A bulk transfer on a port that no longer shows a connection is `Disconnected`. `clear_halt` on an endpoint that is not halted drops and re-adds it with a Configure Endpoint, so the controller resets its data toggle or sequence number as the device does on `CLEAR_FEATURE(ENDPOINT_HALT)` (xHCI 4.6.8; Reset Endpoint only applies to a halted endpoint). EP0, like a bulk endpoint, is not used again after its abort failed until repositioning it succeeds. `Bus` also gains `sleep`.
2. **Mass storage details** (§6.4). `GET_MAX_LUN` that stalls or fails means one LUN; only LUN 0 is used. INQUIRY must report a direct-access device. TEST UNIT READY is tried every 100 ms for up to 5 s, with REQUEST SENSE after each failure; no medium fails at once. Block sizes of 512 to 4096 bytes are accepted; a disk of more than 2^32 blocks is refused, because READ(10) and WRITE(10) cannot reach further (READ CAPACITY(16) is used to learn the size). A command is tried three times at most; ILLEGAL REQUEST is not retried, and a device or controller that is gone ends the request at once. For READ and WRITE a short data phase or a residue is an error. A stick that refuses SYNCHRONIZE CACHE with ILLEGAL REQUEST has no cache to flush: `flush` succeeds and the command is not sent again. A disk whose command gets no answer in its three tries is given up: every later request fails at once until the stick is plugged in again, so a hung stick cannot stall every shell command for minutes (the block cache writes each dirty block again on every sync). A disk that fails to start does not make its device's setup fail; the boot line says `disk not started: <reason>`.
3. **Disks come and go with their device.** Each disk has an id that is never reused, so a request for a stick that was unplugged fails with `EIO` instead of reaching another device, even one plugged into the same port. `/` is not mounted again in milestone 1: after the stick is unplugged, `/` answers `EIO` until the next boot. The boot line names the disk: `port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB` (sizes in whole MiB under 1 GiB, in GiB with one decimal above, rounded down), or `disk not started: <reason>`.
4. **A failed device setup is tried again** (plan 4's deferred finding M4): up to three times in all, each try resetting the port afresh, unless the device is gone or the controller stopped working. Each failed try is logged; the boot line shows the outcome.
5. **GPT details** (§6.5, UEFI 2.10 §5.3). A header is accepted only with its signature, a size of 92 bytes up to a block, its CRC32, its own LBA, the usable range and entry array inside the disk, entries of at least 128 bytes in multiples of 8, an entry array of at most 1 MiB and the array's CRC32. When the primary header is bad the backup is read from the disk's last block; when only the primary's entry array is bad, from the primary's alternate LBA. Unused entries and entries outside the usable range are skipped. "Exactly one ESP and one Linux partition" in the fallback means one partition of each of those types; other types do not count. A boot disk without a Linux partition is an error, not a reason to fall back; a zero boot GUID counts as none; of two disks with the boot partition the first wins.
6. **The `mount /` line and the read-only retry** (§4.4 step 9, §10). `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`. If the read-write mount fails with `EIO` the root is mounted read-only and the line is `[FAIL] mount /: Input/output error; mounted read-only, ext2 on …`: the files can be read, and the photo shows something is wrong. Only if that fails too, or there is no root, is it `[FAIL] mount /: <reason>` with the empty read-only `/`. A damaged primary GPT and the root fallback each get a warning line on the screen; otherwise the log says `storage: root on <disk>, the disk with the boot partition <GUID>`.
7. **Reboot and power-off details** (§7.4). The FADT reset register is used only when it is in I/O or memory space (a memory register aligned to its width), whole bytes from bit 0; each way of restarting or switching off gets half a second before the next, and `reboot` prints `relay: restarting through <method>` before each. PM1 control is read and its sleep type and enable bits replaced: the sleep types go into PM1a and PM1b first, then the same values with `SLP_EN`, as Linux does. Test mode's exit writes 0x10 to `isa-debug-exit`, so QEMU exits with status 33. `poweroff` prints `relay: powering off` first; when switching off fails the reason is printed above the safe-to-power-off message.
8. **A long command needs no extra polling for Ctrl-C.** The shell already calls `Console::interrupted` between the 64 KiB pieces of `cp`, `cat` and the like, and that polls the keyboards; while a disk request runs, the xHCI driver keeps recording key reports.
9. **The e2e runner** (§9.3). `reboot [<regex>]` waits for QEMU to exit as after a reset (`-no-reboot`, status 0), after printing `<regex>` if given, and starts it again on the same disk, continuing the serial log; `poweroff` waits for test mode's exit (status 33). After both, `dumpe2fs -h` must say the root is `clean`. `e2fsck -fn` runs on the disk every scenario leaves, after QEMU is stopped; a filesystem only marked not clean passes, as e2fsck 1.47 treats it. New: the directives `disk small` (an image whose ext2 root is 32 MiB, for `diskfull`) and `break-root` (the root's ext2 magic zeroed while the scenario runs, then restored for `e2fsck`), the step `file-lines <path> <bytes> <line>` (read with `debugfs dump` once the machine is off), and the scenarios `power` and `mount_fail` (spec §10's empty read-only `/`). Scenarios build big files by doubling, because COM1 input passes through the 4 KiB input queue.
10. **What QEMU 8.2's `usb-storage` really answers** (checked by booting it): INQUIRY `QEMU` / `QEMU HARDDISK` / `2.5+`, 524,288 blocks of 512 bytes for the 256 MiB image, bulk endpoints of 1024 bytes with bursts of 16, and no unit attention at start. The Kingston DataTraveler 3.0 answers INQUIRY `Kingston` / `DataTraveler 3.0` / `PMAP` (removable, SPC-4) and has 30,277,632 blocks of 512 bytes, as Linux reports it; its bulk endpoints are 1024 bytes with bursts of 4.
11. **Deferred review findings.** Plan 4's final-review minors M1, M5, M6 and M7 (a full input queue drops a Ctrl-C; a controller whose run times out is freed without confirming it halted; the first port scan logs empty ports as disconnected; the fixed 100 ms debounce) go to the roadmap for plan 6; M4 is decision 4. Plan 3's shell and ext2 minors stay with plan 6: the new scenarios do not depend on them.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A stick that misbehaves or is slow:** never ready, stalls in any phase, phase errors, invalid status wrappers, timeouts, write protect, medium errors, a residue, or firmware that hangs while it stays connected. Expected: bounded time, the recovery of BOT 5.3.4 and 6.7 with at most three tries, host and device agreeing on the data toggle afterwards, `EIO` to the filesystem, a usable device where it answers, a hung one given up at once instead of stalling every command, and no DMA buffer reused while the controller may still write it. Tests: the stall, phase-error, invalid-CSW and three-tries tests of `storage::transport` (Tasks 5–6), `a_disk_that_stops_answering_is_given_up`, `a_disk_that_answers_with_an_error_is_not_given_up` (Task 9), `clearing_the_halt_of_an_endpoint_that_is_not_halted_resets_the_toggle_too` and the fake's data toggle check (Task 2), `a_bulk_transfer_that_times_out_is_aborted_and_the_endpoint_works_again`, `an_endpoint_whose_abort_failed_is_not_used_until_its_halt_is_cleared` (Task 1), the EP0 abort tests (Task 3).
2. **The stick unplugged or replugged** while `/` is mounted, during a transfer or between two looks. Expected: requests fail at once with `EIO` and never reach another device, the shell keeps running, and the memory is freed once the device is detached. Tests: `unplugging_during_a_bulk_transfer_fails_it_at_once` (Task 1), `unplugging_during_a_request_fails_it_at_once` (Task 6), `an_unplugged_disk_is_gone_for_good_and_a_replug_gets_a_new_id` (Task 7), `a_request_for_an_unplugged_disk_never_reaches_the_stick_plugged_in_after_it` (Task 8).
3. **Disks and partition tables that are not ours:** a damaged primary header or entry array, hostile sizes and ranges, several disks, no or a zero boot GUID, a root that is not ext2. Expected: the right partition, or a clear `[FAIL] mount /: …` with the empty read-only `/`, never a panic, a huge allocation or a read far past the disk. Tests: the corrupt, hostile and backup tests of `block::gpt` (Task 10), `fallback_disks`, `a_zero_boot_guid_matches_nothing` (Task 11), `a_disk_that_refuses_writes_is_mounted_read_only` (Task 18), the `boot` scenario's boot-GUID line (Task 19), the `mount_fail` scenario (Task 22).
4. **Shutting down and starting again:** `reboot` and `poweroff` after writes, firmware tables with odd registers. Expected: files persist, the filesystem is marked clean, `e2fsck -fn` is clean, the machine restarts through the register the firmware names or switches off, and no ACPI value can make the kernel fault. Tests: `a_memory_register_off_its_alignment_is_refused` (Task 13), `both_sleep_types_are_written_before_either_enable` (Task 14), the `power` scenario's reset-register line (Task 16), `the_clean_flag_is_read_from_the_superblock` and the clean check after every `reboot` and `poweroff` (Task 17), the `persist` scenario (Task 20).
5. **Big files and a full disk:** 64 KiB command boundaries, an 8 MiB file, `ENOSPC`. Expected: every byte right, `No space left on device`, and a clean filesystem. Tests: `bulk_transfers_move_up_to_64_kib_each_way`, `bulk_buffers_are_64_kib_aligned_so_no_transfer_crosses_a_boundary` (Task 1), `what_is_written_is_read_back`, `the_first_and_the_last_block_can_be_used` (Task 6), the `bigfile` and `diskfull` scenarios (Task 21).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/usb/src/{bus,error,lib}.rs` | `Bus::{bulk_in, bulk_out, sleep}`, `MAX_BULK`; `UsbError::{Protocol, Sense}` |
| `crates/usb/src/xhci/{mod,configure,transfer,device}.rs` | 64 KiB bulk buffers, bulk transfers with timeout and abort, lost endpoints |
| `crates/usb/src/storage/{mod,bot,scsi}.rs` | The storage module; CBW/CSW; SCSI command blocks and answers |
| `crates/usb/src/storage/{transport,disk,io}.rs` | One BOT command with recovery and three tries; setup; reads, writes, flush |
| `crates/usb/src/host.rs`, `crates/usb/src/host/boot_line.rs` | Disks in `Host`, `DiskId`, setup retries; the boot line |
| `crates/usb/src/testing/{bus,device}.rs`, `crates/usb/src/testing/storage/**`, `crates/usb/src/testing/xhci/transfers.rs` | Test support: `TamperBus`, `qemu_stick`, the fake storage device, the fake controller's TRB checks |
| `kernel/src/block/{mod,crc32,gpt,partition,root}.rs` | CRC32, GPT, partitions, the root choice |
| `kernel/src/block/testing.rs` | Test support: in-memory disks, `sfdisk`/`fdisk`/`mke2fs` images |
| `kernel/src/power.rs` | `reboot`, `poweroff`, test mode's exit |
| `kernel/src/storage.rs` | Startup step 9: GPT, root choice, mount with the read-only retry |
| `kernel/src/{usb,session,lib}.rs`, `kernel/Cargo.toml` | `UsbDisk`; the shell on the mounted root; the startup order |
| `xtask/src/{e2e,image,config}.rs` | The steps `reboot`, `poweroff`, `file-lines`, the directive `disk small`, `e2fsck` after every scenario |
| `tests/e2e/{boot,boot_bigmode,keyboard,shell,power}.txt` | Boot to `/root` on the real disk; reboot and power-off |
| `tests/e2e/{fileops,persist,display,bigfile,diskfull}.txt` | Spec §9.3 scenarios 3–7 |
| `docs/hardware-test.md`, `README.md` | NUC check 3; the crate list |

---

## PR 1: The plan (already committed)

This plan, the spec's §15 item 11 and the roadmap notes are committed on branch `plan5/plan` (worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-plan`). They change no code, but CI runs on every pull request. Run the commands below from that worktree.

- [ ] **Push and open the pull request**

````bash
git push -u origin plan5/plan
gh pr create --base main --head plan5/plan --title "Plan 5: USB storage and / (plan document)" --body-file - <<'EOF'
## What

Milestone 1, plan 5 (USB storage and `/`): the full implementation plan, the spec's §15 item 11 (decisions made while planning) and roadmap notes.

## How it was tested

- [x] Every task was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks plan5/plan --watch`). Ask the user to review and merge. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-plan` and continue with PR 2.

---

## PR 2: Bulk transfers (Tasks 1–3)

The xHCI side of mass storage: bulk IN and OUT transfers of up to 64 KiB with the 5 s timeout and a safe abort, and a fake controller that refuses the TRBs hardware would refuse.

Branch `plan5/bulk`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-bulk`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan5/bulk /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-bulk origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-bulk
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan5/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan5/bulk /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-bulk plan5/plan`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan5/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: Bulk transfers of up to 64 KiB

Spec §6.2 and §6.4: the storage driver moves its commands, data and status over one bulk IN and one bulk OUT endpoint, in transfers of up to 64 KiB, and a bulk transfer may take 5 s. `Bus` gains `bulk_in` and `bulk_out`, which wait for their transfer as `control` does (a storage request blocks by nature), and `sleep`, for a disk that asks for time. Every bulk endpoint gets a 64 KiB buffer aligned to 64 KiB, so one Normal TRB covers any transfer without crossing a 64 KiB boundary (xHCI 6.4.1.1); interrupt IN endpoints keep their page. A halted endpoint ignores its doorbell (xHCI 4.8.3), so a transfer on one fails with `Stall` at once instead of timing out. A transfer that times out is aborted (Stop Endpoint, then Set TR Dequeue Pointer past it) before the call returns; the Stopped event that follows is the driver's own doing and is not logged as a failure. If the abort fails, the controller may still own the TRB and write into the buffer, so the endpoint is marked lost: it takes no transfer until `clear_halt` has repositioned its ring, and its buffer is freed only with the slot (decision 1). The fake controller now panics on a Normal TRB over 64 KiB, one that crosses a 64 KiB boundary, one whose buffer is not allocated, and chained or immediate-data Normal TRBs, which this driver never sends.

**Files:**
- Modify: `crates/usb/src/bus.rs`
- Modify: `crates/usb/src/hid/keyboard.rs`
- Modify: `crates/usb/src/lib.rs`
- Modify: `crates/usb/src/testing/xhci/transfers.rs`
- Modify: `crates/usb/src/xhci/configure.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: plan 4's `Bus`, `Xhci` transfer code (`control_transfer`, `reposition`, `Transfer`, `Endpoint`) and the fake xHCI.
- Produces: `usb::MAX_BULK = 65536`; `Bus::{bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError>, bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError>, sleep(&self, Duration)}`; `xhci::transfer::{BULK_TIMEOUT = 5 s, BULK_BUFFER_SIZE = MAX_BULK}`; `Endpoint::lost`.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/hid/keyboard.rs`**

In `crates/usb/src/hid/keyboard.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

        fn clear_halt(&mut self, _slot: u8, _endpoint: u8) -> Result<(), UsbError> {
````

with:

````rust

        fn bulk_in(&mut self, _: u8, _: u8, _: &mut [u8]) -> Result<usize, UsbError> {
            panic!("keyboards have no bulk endpoints");
        }

        fn bulk_out(&mut self, _: u8, _: u8, _: &[u8]) -> Result<usize, UsbError> {
            panic!("keyboards have no bulk endpoints");
        }

        fn clear_halt(&mut self, _slot: u8, _endpoint: u8) -> Result<(), UsbError> {
````

Replace:

````rust
            self.now.get()
        }
````

with:

````rust
            self.now.get()
        }

        fn sleep(&self, d: Duration) {
            self.now.set(self.now.get() + d);
        }
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/transfers.rs`**

In `crates/usb/src/testing/xhci/transfers.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
const ISP: u32 = 1 << 2;
const IOC: u32 = 1 << 5;
````

with:

````rust
const ISP: u32 = 1 << 2;
const CHAIN: u32 = 1 << 4;
const IOC: u32 = 1 << 5;
````

Replace:

````rust
        let (buffer, len) = (pointer(&trb), (trb[2] & 0x1_FFFF) as usize);
        let address = (dci / 2) as u8 | if dci % 2 == 1 { 0x80 } else { 0 };
````

with:

````rust
        let (buffer, len) = (pointer(&trb), (trb[2] & 0x1_FFFF) as usize);
        // xHCI 6.4.1.1: at most 64 KiB, and the buffer must not cross a
        // 64 KiB boundary; this fake plays one-TRB TDs only.
        if len > 0x1_0000 {
            panic!("fake xhci: Normal TRB of {len} bytes (at most 64 KiB)");
        }
        if (buffer & 0xFFFF) + len as u64 > 0x1_0000 {
            panic!("fake xhci: Normal TRB buffer {buffer:#x}+{len} crosses a 64 KiB boundary");
        }
        if trb[3] & (CHAIN | IDT) != 0 {
            panic!("fake xhci: chained or immediate-data Normal TRBs are not modelled");
        }
        if !dma.contains(buffer, len) {
            panic!("fake xhci: Normal TRB buffer {buffer:#x}+{len} is not allocated");
        }
        let address = (dci / 2) as u8 | if dci % 2 == 1 { 0x80 } else { 0 };
````

- [ ] **Step 3: Add the failing tests to `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            xhci.queue_in(d.slot, 0x81, 5000),
            Err(UsbError::Unsupported("transfer over 4096 bytes"))
        );
````

with:

````rust
            xhci.queue_in(d.slot, 0x81, 5000),
            Err(UsbError::Unsupported("transfer larger than the buffer"))
        );
````

Replace:

````rust
        assert!(Bus::now(&xhci) > before);
        Bus::log(&xhci, format_args!("hid: hello"));
        assert!(hal.log_text().ends_with("hid: hello"));
    }
````

with:

````rust
        assert!(Bus::now(&xhci) > before);
        Bus::sleep(&xhci, Duration::from_millis(3));
        assert!(hal.clock() >= before + Duration::from_millis(3));
        Bus::log(&xhci, format_args!("hid: hello"));
        assert!(hal.log_text().ends_with("hid: hello"));
    }

    fn stick(config: FakeConfig, port: u8) -> (FakeHal, Xhci<FakeHal>, Device, Dev) {
        let stick = if config.ports > 8 {
            FakeUsbDevice::kingston_stick()
        } else {
            FakeUsbDevice::usb2_stick()
        };
        let (hal, mut xhci, d) = attached(config, port, &stick);
        xhci.configure(&d, &[0]).unwrap();
        (hal, xhci, d, stick)
    }

    fn sticks() -> [(FakeHal, Xhci<FakeHal>, Device, Dev); 2] {
        [
            stick(FakeConfig::intel(), 13),
            stick(FakeConfig::basic(), 3),
        ]
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 256) as u8).collect()
    }

    #[test]
    fn bulk_transfers_move_up_to_64_kib_each_way() {
        for (_hal, mut xhci, d, dev) in sticks() {
            let data = pattern(MAX_BULK);
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &data), Ok(MAX_BULK));
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &data[..31]), Ok(31));
            let got = dev.borrow().data_out_received();
            assert_eq!(got, [(0x02, data.clone()), (0x02, data[..31].to_vec())]);
            dev.borrow_mut().push_in(0x81, &data);
            let mut buf = vec![0; MAX_BULK];
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(MAX_BULK));
            assert_eq!(buf, data);
        }
    }

    #[test]
    fn a_short_bulk_in_returns_what_came() {
        for (_hal, mut xhci, d, dev) in sticks() {
            dev.borrow_mut().push_in(0x81, &[0x55; 13]);
            let mut buf = vec![0; 512];
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
            assert_eq!(buf[..13], [0x55; 13]);
            assert!(buf[13..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn bulk_buffers_are_64_kib_aligned_so_no_transfer_crosses_a_boundary() {
        for (_hal, xhci, d, _dev) in sticks() {
            let s = xhci.slots[d.slot as usize].as_ref().unwrap();
            for ep in &s.endpoints {
                let buf = ep.buffer.as_ref().expect("bulk endpoints have a buffer");
                assert_eq!(buf.size(), MAX_BULK);
                assert_eq!(
                    buf.phys() % MAX_BULK as u64,
                    0,
                    "endpoint {:#x}",
                    ep.address
                );
            }
        }
        // Interrupt endpoints keep their page.
        let (_hal, xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        let s = xhci.slots[d.slot as usize].as_ref().unwrap();
        assert_eq!(s.endpoints[0].buffer.as_ref().unwrap().size(), 4096);
    }

    #[test]
    fn a_stalled_bulk_endpoint_stays_halted_until_its_halt_is_cleared() {
        for (hal, mut xhci, d, dev) in sticks() {
            dev.borrow_mut().stall_endpoint(0x02);
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Err(UsbError::Stall));
            // Halted: refused at once instead of waiting for a doorbell the
            // controller ignores.
            let before = hal.clock();
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Err(UsbError::Stall));
            assert!(hal.clock() - before < Duration::from_millis(1));
            xhci.clear_halt(d.slot, 0x02).unwrap();
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[2; 31]), Ok(31));
            assert_eq!(dev.borrow().data_out_received(), [(0x02, vec![2; 31])]);
        }
    }

    #[test]
    fn a_bulk_transfer_that_times_out_is_aborted_and_the_endpoint_works_again() {
        for (hal, mut xhci, d, dev) in sticks() {
            let (n, before) = (hal.fake().executed().len(), hal.clock());
            let mut buf = vec![0; 512];
            // Nothing to send: the device NAKs.
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Timeout));
            let waited = hal.clock() - before;
            assert!(waited >= BULK_TIMEOUT && waited < BULK_TIMEOUT + Duration::from_secs(1));
            // Stop Endpoint, then Set TR Dequeue Pointer past the TRB.
            assert_eq!(commands_since(&hal, n), [15, 16]);
            assert!(
                hal.log_text()
                    .contains("slot 1 endpoint 0x81: bulk transfer of 512 bytes timed out")
            );
            assert!(
                !hal.log_text().contains("transfer failed"),
                "a stop is no failure"
            );
            dev.borrow_mut().push_in(0x81, &[9; 13]);
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
        }
    }

    #[test]
    fn an_endpoint_whose_abort_failed_is_not_used_until_its_halt_is_cleared() {
        let (hal, mut xhci, d, dev) = stick(FakeConfig::intel(), 13);
        hal.fake().config_mut().hang_command = Some(15); // Stop Endpoint
        let mut buf = vec![0; 512];
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Timeout));
        assert!(hal.log_text().contains("abort failed"));
        let (n, before) = (hal.fake().executed().len(), hal.outstanding_dma());
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Unsupported("endpoint lost after a failed abort"))
        );
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 13),
            Err(UsbError::Unsupported("endpoint lost after a failed abort"))
        );
        assert_eq!(hal.fake().executed().len(), n, "nothing sent");
        // The controller still owns the TRB: the device's data lands in
        // the buffer, which is still allocated (the fake panics otherwise).
        dev.borrow_mut().push_in(0x81, &[7; 13]);
        hal.sleep(Duration::from_millis(5));
        assert_eq!(hal.outstanding_dma(), before);
        hal.fake().config_mut().hang_command = None;
        xhci.clear_halt(d.slot, 0x81).unwrap();
        dev.borrow_mut().push_in(0x81, &[8; 13]);
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
        assert_eq!(buf[..13], [8; 13]);
    }

    #[test]
    fn unplugging_during_a_bulk_transfer_fails_it_at_once() {
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        hal.fake()
            .after(Duration::from_millis(5), |x, _| x.unplug(13));
        let before = hal.clock();
        let mut buf = vec![0; 512];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Transfer(4))
        );
        assert!(hal.clock() - before < Duration::from_millis(10));
    }

    #[test]
    fn bulk_transfers_the_driver_cannot_do_are_refused() {
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        let mut big = vec![0; MAX_BULK + 1];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut big),
            Err(UsbError::Unsupported("bulk transfer over 64 KiB"))
        );
        assert_eq!(
            xhci.bulk_out(d.slot, 0x81, &[0; 4]),
            Err(UsbError::Unsupported("bulk OUT on an IN endpoint"))
        );
        assert_eq!(
            xhci.bulk_in(d.slot, 0x02, &mut [0; 4]),
            Err(UsbError::Unsupported("bulk IN on an OUT endpoint"))
        );
        assert_eq!(
            xhci.bulk_in(9, 0x81, &mut [0; 4]),
            Err(UsbError::Disconnected)
        );
        hal.fake().host_system_error();
        xhci.poll();
        assert_eq!(
            xhci.bulk_out(d.slot, 0x02, &[0; 4]),
            Err(UsbError::ControllerDead)
        );
        let (_hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut [0; 8]),
            Err(UsbError::Unsupported("not a bulk endpoint"))
        );
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find value `MAX_BULK` in this scope ``; `` cannot find value `BULK_TIMEOUT` in this scope ``.

- [ ] **Step 5: Change `crates/usb/src/bus.rs`**

In `crates/usb/src/bus.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! What class drivers see of a host controller: control requests, IN
//! transfers that complete later, and halt recovery. The xHCI driver
//! implements [`Bus`]; the class drivers' tests use a fake.

use crate::UsbError;
use core::fmt;
use core::time::Duration;

````

with:

````rust
//! What class drivers see of a host controller: control requests, IN
//! transfers that complete later, bulk transfers that are waited for, and
//! halt recovery. The xHCI driver implements [`Bus`]; the class drivers'
//! tests use a fake.

use crate::UsbError;
use core::fmt;
use core::time::Duration;

/// The largest bulk transfer (spec §6.4: storage commands move at most
/// 64 KiB).
pub const MAX_BULK: usize = 64 * 1024;

````

Replace:

````rust
    ) -> Option<Result<usize, UsbError>>;
    /// Makes a halted `endpoint` usable again: resets it on the controller
````

with:

````rust
    ) -> Option<Result<usize, UsbError>>;
    /// A bulk IN transfer on `endpoint` of up to `buf.len()` bytes
    /// ([`MAX_BULK`] at most): waits for it (5 s at most) and returns the
    /// number of bytes received into `buf`. A STALL is `UsbError::Stall`,
    /// and the endpoint stays halted until [`Bus::clear_halt`]. A transfer
    /// that times out is aborted before this returns; if the abort fails,
    /// the endpoint takes no transfer until `clear_halt` succeeds.
    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError>;
    /// A bulk OUT transfer of `data` on `endpoint`, as [`Bus::bulk_in`]:
    /// returns the number of bytes sent.
    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError>;
    /// Makes a halted `endpoint` usable again: resets it on the controller
````

Replace:

````rust
    fn now(&self) -> Duration;
    /// Adds one line to the kernel log.
````

with:

````rust
    fn now(&self) -> Duration;
    /// Waits at least `d` (a device that asks for time, such as a disk
    /// spinning up).
    fn sleep(&self, d: Duration);
    /// Adds one line to the kernel log.
````

- [ ] **Step 6: Change `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust

pub use bus::{Bus, Setup, Speed};
pub use error::UsbError;
````

with:

````rust

pub use bus::{Bus, MAX_BULK, Setup, Speed};
pub use error::UsbError;
````

- [ ] **Step 7: Change `crates/usb/src/xhci/configure.rs`**

In `crates/usb/src/xhci/configure.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use super::ring::ProducerRing;
use super::transfer::DATA_BUFFER_SIZE;
use super::trb::Trb;
````

with:

````rust
use super::ring::ProducerRing;
use super::transfer::{BULK_BUFFER_SIZE, DATA_BUFFER_SIZE};
use super::trb::Trb;
````

Replace:

````rust

/// A transfer ring, and for IN endpoints a buffer, for endpoint `e`.
fn new_endpoint<H: Hal>(hal: &H, e: &descriptor::Endpoint) -> Result<Endpoint, UsbError> {
    let ring = ProducerRing::new(hal)?;
    let buffer = if e.is_in() {
        let Some(buf) = hal.alloc_dma(DATA_BUFFER_SIZE, 64) else {
            ring.free(hal);
            return Err(UsbError::NoMemory);
        };
        Some(buf)
    } else {
        None
    };
````

with:

````rust

/// A transfer ring, and a buffer, for endpoint `e`: bulk endpoints get
/// 64 KiB aligned to 64 KiB, so one Normal TRB covers any transfer without
/// crossing a 64 KiB boundary (xHCI 6.4.1.1); interrupt IN endpoints get
/// 4 KiB; interrupt OUT endpoints none.
fn new_endpoint<H: Hal>(hal: &H, e: &descriptor::Endpoint) -> Result<Endpoint, UsbError> {
    let ring = ProducerRing::new(hal)?;
    let size = match e.kind {
        EndpointKind::Bulk => Some(BULK_BUFFER_SIZE),
        _ if e.is_in() => Some(DATA_BUFFER_SIZE),
        _ => None,
    };
    let buffer = match size {
        Some(size) => {
            let Some(buf) = hal.alloc_dma(size, size) else {
                ring.free(hal);
                return Err(UsbError::NoMemory);
            };
            Some(buf)
        }
        None => None,
    };
````

Replace:

````rust
        transfer: Transfer::Idle,
    })
````

with:

````rust
        transfer: Transfer::Idle,
        lost: false,
    })
````

- [ ] **Step 8: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
    ring: ProducerRing,
    /// IN endpoints: the 4 KiB buffer their transfers land in.
    buffer: Option<DmaBuf>,
    transfer: Transfer,
}
````

with:

````rust
    ring: ProducerRing,
    /// Where transfers land or come from: 4 KiB for interrupt IN
    /// endpoints, 64 KiB (aligned to 64 KiB) for bulk endpoints.
    buffer: Option<DmaBuf>,
    transfer: Transfer,
    /// A transfer timed out and aborting it failed: the controller may
    /// still own it and write into the buffer, so no transfer uses the
    /// endpoint until `clear_halt` has repositioned its ring.
    lost: bool,
}
````

- [ ] **Step 9: Change `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//! Transfers (xHCI 4.11): control transfers on EP0, matching transfer
//! events to what is in flight, and putting an endpoint back in order
//! after a STALL or a timeout.

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output, dci};
use super::trb::{SHORT_PACKET, STALL, SUCCESS, Trb, completion_name};
use super::{Control, Transfer, Xhci};
use crate::{Bus, Hal, Setup, UsbError};
use core::fmt;
````

with:

````rust
//! Transfers (xHCI 4.11): control transfers on EP0, IN transfers that
//! complete later, bulk transfers that are waited for, matching transfer
//! events to what is in flight, and putting an endpoint back in order
//! after a STALL or a timeout.

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output, dci};
use super::trb::{
    SHORT_PACKET, STALL, STOPPED, STOPPED_LENGTH_INVALID, SUCCESS, Trb, completion_name,
};
use super::{Control, Transfer, Xhci};
use crate::{Bus, Hal, MAX_BULK, Setup, UsbError};
use core::fmt;
````

Replace:

````rust
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
/// The largest transfer: one page of DMA buffer.
pub const DATA_BUFFER_SIZE: usize = 4096;
/// How often a transfer wait polls.
````

with:

````rust
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a bulk transfer may take (spec §6.2).
pub const BULK_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest control or interrupt transfer: one page of DMA buffer.
pub const DATA_BUFFER_SIZE: usize = 4096;
/// A bulk endpoint's buffer: the largest bulk transfer.
pub const BULK_BUFFER_SIZE: usize = MAX_BULK;
/// How often a transfer wait polls.
````

Replace:

````rust
        });
        if !matches!(code, SUCCESS | SHORT_PACKET) {
            xlog!(
````

with:

````rust
        });
        // Stopped is the driver's own doing (an abort), not a failure.
        if !matches!(
            code,
            SUCCESS | SHORT_PACKET | STOPPED | STOPPED_LENGTH_INVALID
        ) {
            xlog!(
````

Replace:

````rust
                completion_name(code)
            );
        }
    }

    /// Makes an endpoint usable after a halt or a timeout: Reset Endpoint
````

with:

````rust
                completion_name(code)
            );
        }
    }

    /// A bulk transfer of `len` bytes on `endpoint` (the `Bus::bulk_in`
    /// and `bulk_out` contract): `out` is the data to send for an OUT
    /// endpoint. One Normal TRB into the endpoint's 64 KiB buffer; after
    /// 5 s the endpoint is stopped and its ring moved past the TRB.
    fn bulk_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        len: usize,
        out: Option<&[u8]>,
    ) -> Result<usize, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        if ep.lost {
            return Err(UsbError::Unsupported("endpoint lost after a failed abort"));
        }
        let Some(buffer) = ep.buffer.as_ref().filter(|b| b.size() == BULK_BUFFER_SIZE) else {
            return Err(UsbError::Unsupported("not a bulk endpoint"));
        };
        if len > buffer.size() {
            return Err(UsbError::Unsupported("bulk transfer over 64 KiB"));
        }
        if !matches!(ep.transfer, Transfer::Idle) {
            return Err(UsbError::Unsupported("transfer already queued"));
        }
        let dci = ep.dci;
        // A halted endpoint ignores its doorbell (xHCI 4.8.3): the
        // transfer would only time out.
        if self.endpoint_state(slot as usize, dci)? == EP_HALTED {
            return Err(UsbError::Stall);
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        let buffer = ep.buffer.as_ref().ok_or(UsbError::Disconnected)?;
        if let Some(data) = out {
            buffer.write_bytes(0, data);
        }
        let trb = ep.ring.push(Trb::normal(buffer.phys(), len as u32));
        ep.transfer = Transfer::Queued { trb, len };
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, dci as u32);
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                return Err(UsbError::ControllerDead);
            }
            let ep = self
                .endpoint_mut(slot, endpoint)
                .ok_or(UsbError::Disconnected)?;
            match core::mem::replace(&mut ep.transfer, Transfer::Idle) {
                Transfer::Done(result) => return result,
                other => ep.transfer = other,
            }
            if self.hal.now() - start >= BULK_TIMEOUT {
                break;
            }
            self.hal.sleep(TRANSFER_POLL);
        }
        xlog!(
            &self.hal,
            &self.name,
            "slot {slot} endpoint {endpoint:#04x}: bulk transfer of {len} bytes timed out"
        );
        let aborted = self.reposition(slot as usize, dci);
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
            ep.transfer = Transfer::Idle;
            if let Err(e) = aborted {
                // The controller may still own the TRB and its buffer.
                ep.lost = true;
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot} endpoint {endpoint:#04x}: abort failed: {e}; endpoint not used until reset"
                );
            }
        }
        Err(UsbError::Timeout)
    }

    /// Makes an endpoint usable after a halt or a timeout: Reset Endpoint
````

Replace:

````rust
        }
        if len > DATA_BUFFER_SIZE {
            return Err(UsbError::Unsupported("transfer over 4096 bytes"));
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        let Some(buffer) = &ep.buffer else {
            return Err(UsbError::Unsupported("not an IN endpoint"));
        };
        if !matches!(ep.transfer, Transfer::Idle) {
````

with:

````rust
        }
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        if ep.lost {
            return Err(UsbError::Unsupported("endpoint lost after a failed abort"));
        }
        let Some(buffer) = ep.buffer.as_ref().filter(|_| endpoint & 0x80 != 0) else {
            return Err(UsbError::Unsupported("not an IN endpoint"));
        };
        if len > buffer.size() {
            return Err(UsbError::Unsupported("transfer larger than the buffer"));
        }
        if !matches!(ep.transfer, Transfer::Idle) {
````

Replace:

````rust

    /// Reset Endpoint if the context says Halted (Stop Endpoint if it still
    /// runs), Set TR Dequeue Pointer to the enqueue position, dropping
    /// anything outstanding, then CLEAR_FEATURE(ENDPOINT_HALT).
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
````

with:

````rust

    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError> {
        if endpoint & 0x80 == 0 {
            return Err(UsbError::Unsupported("bulk IN on an OUT endpoint"));
        }
        let n = self.bulk_transfer(slot, endpoint, buf.len(), None)?;
        let n = n.min(buf.len());
        if let Some(buffer) = self
            .endpoint_mut(slot, endpoint)
            .and_then(|e| e.buffer.as_ref())
        {
            buffer.read_bytes(0, &mut buf[..n]);
        }
        Ok(n)
    }

    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError> {
        if endpoint & 0x80 != 0 {
            return Err(UsbError::Unsupported("bulk OUT on an IN endpoint"));
        }
        self.bulk_transfer(slot, endpoint, data.len(), Some(data))
    }

    /// Reset Endpoint if the context says Halted (Stop Endpoint if it still
    /// runs), Set TR Dequeue Pointer to the enqueue position, dropping
    /// anything outstanding, then CLEAR_FEATURE(ENDPOINT_HALT). An endpoint
    /// lost after a failed abort is usable again once this succeeds.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
````

Replace:

````rust
        self.reposition(slot as usize, dci)?;
        self.control_transfer(slot, Setup::clear_halt(endpoint), &mut [])?;
````

with:

````rust
        self.reposition(slot as usize, dci)?;
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
            ep.lost = false;
        }
        self.control_transfer(slot, Setup::clear_halt(endpoint), &mut [])?;
````

Replace:

````rust
        self.hal.now()
    }
````

with:

````rust
        self.hal.now()
    }

    fn sleep(&self, d: Duration) {
        self.hal.sleep(d);
    }
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 227 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -m "usb: xHCI bulk transfers of up to 64 KiB with timeouts and aborts"
````


### Task 2: Clearing a halt resets the host's data toggle too

Review finding (important): BOT reset recovery clears the halt of both bulk endpoints, and the device resets its data toggle (USB 2) or sequence number (USB 3) on every CLEAR_FEATURE(ENDPOINT_HALT) (USB 2.0 §9.4.5). The controller resets its own only on Reset Endpoint, which applies to a halted endpoint; for one that is merely stopped, Stop Endpoint and Set TR Dequeue Pointer leave it as it was. The two sides then disagree, and the next transfer is lost: a USB 2 stick acknowledges and drops the next command, and the status read times out. As xHCI 4.6.8's note and Linux's `xhci_endpoint_reset` do, `clear_halt` on an endpoint that is not halted stops it and then drops and re-adds it with a Configure Endpoint, its context rebuilt with the ring's current position, which resets the host side; a halted endpoint keeps Reset Endpoint and Set TR Dequeue Pointer. The fake controller gets a data toggle model on both sides, reset where the specifications reset it and advanced by every packet, and panics when they disagree (decision 1).

**Files:**
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/slots.rs`
- Modify: `crates/usb/src/testing/xhci/transfers.rs`
- Modify: `crates/usb/src/xhci/configure.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces: `Bus::clear_halt` resets the controller's toggle or sequence number of an endpoint that was not halted (Configure Endpoint drop and add); the fake xHCI's data toggle check (`fake xhci: data toggle mismatch …`).

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    requests: Vec<(usize, crate::Setup, u32)>,
}
````

with:

````rust
    requests: Vec<(usize, crate::Setup, u32)>,
    /// The device side's data toggle (USB 2) or sequence number (USB 3)
    /// of each endpoint (slot, DCI) but EP0; the host side's is in its
    /// `FakeEndpoint`.
    device_toggles: std::collections::BTreeMap<(usize, usize), u32>,
}
````

Replace:

````rust
            requests: Vec::new(),
        };
````

with:

````rust
            requests: Vec::new(),
            device_toggles: Default::default(),
        };
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/slots.rs`**

In `crates/usb/src/testing/xhci/slots.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    pub busy: bool,
}
````

with:

````rust
    pub busy: bool,
    /// The host side's data toggle (USB 2) or sequence number (USB 3):
    /// 0 when the endpoint is added and after Reset Endpoint (xHCI 4.6.8).
    pub toggle: u32,
}
````

Replace:

````rust
            busy: false,
        }
````

with:

````rust
            busy: false,
            toggle: 0,
        }
````

Replace:

````rust
        s.endpoints.insert(1, ep0);
        self.set_ep_state(slot, 1, RUNNING, dma);
````

with:

````rust
        s.endpoints.insert(1, ep0);
        // A new device: its endpoints start over.
        self.device_toggles.retain(|&(sl, _), _| sl != slot);
        self.set_ep_state(slot, 1, RUNNING, dma);
````

Replace:

````rust
                self.set_ep_state(slot, dci, STOPPED_STATE, dma);
                SUCCESS
````

with:

````rust
                self.set_ep_state(slot, dci, STOPPED_STATE, dma);
                if let Some(ep) = self.slots[slot]
                    .as_mut()
                    .and_then(|s| s.endpoints.get_mut(&dci))
                {
                    ep.toggle = 0;
                }
                SUCCESS
````

Replace:

````rust
        }
        let mut added = Vec::new();
````

with:

````rust
        }
        let configured = &self.slots[slot]
            .as_ref()
            .expect("slot checked above")
            .endpoints;
        for dci in 2..32usize {
            let (dropped, added) = (drop & 1 << dci != 0, add & 1 << dci != 0);
            match configured.get(&dci) {
                // xHCI 4.6.6: a Drop flag names an enabled endpoint, which
                // the driver stops first (a TD in flight would be lost).
                None if dropped => {
                    panic!("fake xhci: Configure Endpoint drops DCI {dci}, which is not configured")
                }
                Some(ep) if dropped && (ep.state == RUNNING || ep.busy) => {
                    panic!("fake xhci: Configure Endpoint drops DCI {dci} while it runs")
                }
                // Adding an enabled endpoint again needs its Drop flag.
                Some(_) if added && !dropped => {
                    panic!("fake xhci: Configure Endpoint adds DCI {dci} again without dropping it")
                }
                _ => {}
            }
        }
        let mut added = Vec::new();
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/transfers.rs`**

In `crates/usb/src/testing/xhci/transfers.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let answer = dev.borrow_mut().control(setup, &out);
        let mut stage = after_setup;
````

with:

````rust
        let answer = dev.borrow_mut().control(setup, &out);
        if matches!(answer, Some(Ok(_))) {
            self.device_request_done(slot, &setup);
        }
        let mut stage = after_setup;
````

Replace:

````rust
            Some(Ok(n)) => {
                let residual = (len - n) as u32;
````

with:

````rust
            Some(Ok(n)) => {
                let speed = dev.borrow().speed();
                self.toggle(slot, dci, n, speed);
                let residual = (len - n) as u32;
````

Replace:

````rust
                Td::Done
            }
        }
    }

    /// The device on `port` went away: every TD in progress of its slot
````

with:

````rust
                Td::Done
            }
        }
    }

    /// A standard request the device took resets its toggles: CLEAR_FEATURE
    /// (ENDPOINT_HALT) that endpoint's, SET_CONFIGURATION all of them (USB
    /// 2.0 9.4.5, 9.1.1.5).
    fn device_request_done(&mut self, slot: usize, setup: &Setup) {
        match (setup.request_type, setup.request, setup.value) {
            (0x02, 1, 0) => {
                let address = setup.index as u8;
                let dci = (address & 0x0F) as usize * 2 + (address >> 7) as usize;
                self.device_toggles.insert((slot, dci), 0);
            }
            (0x00, 9, _) => self.device_toggles.retain(|&(s, _), _| s != slot),
            _ => {}
        }
    }

    /// A TD that moved `bytes` on (slot, DCI): host and device must agree
    /// on the data toggle (USB 2.0 8.6) or sequence number (USB 3.2 8.12.1)
    /// before it, or the device drops the data; both then advance by the
    /// packets moved (a zero-length transfer is one packet).
    fn toggle(&mut self, slot: usize, dci: usize, bytes: usize, speed: crate::Speed) {
        let modulus = if speed.is_superspeed() { 32 } else { 2 };
        let device = self.device_toggles.get(&(slot, dci)).copied().unwrap_or(0);
        let Some(ep) = self.ring(slot, dci) else {
            return;
        };
        let host = ep.toggle;
        if host != device {
            panic!(
                "fake xhci: data toggle mismatch on slot {slot} DCI {dci}: host {host}, device {device}"
            );
        }
        let packets = bytes.div_ceil(ep.max_packet.max(1) as usize).max(1) as u32;
        ep.toggle = (host + packets) % modulus;
        self.device_toggles
            .insert((slot, dci), (device + packets) % modulus);
    }

    /// The device on `port` went away: every TD in progress of its slot
````

- [ ] **Step 4: Add the failing tests to `crates/usb/src/xhci/configure.rs`**

In `crates/usb/src/xhci/configure.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
````

with:

````rust
    use super::*;
    use crate::Bus;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
````

Replace:

````rust
        }
    }
}
````

with:

````rust
        }
    }

    #[test]
    fn adding_an_endpoint_again_keeps_its_context_with_the_ring_going_on() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        let before = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[0; 31]), Ok(31));
        xhci.command(Trb::stop_endpoint(d.slot, 4)).unwrap();
        xhci.add_again(d.slot, 4).unwrap();
        let after = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(
            (
                after.ep_type,
                after.max_packet,
                after.max_burst,
                after.toggle
            ),
            (before.ep_type, before.max_packet, before.max_burst, 0)
        );
        // The ring goes on after the TRB used: its dequeue moved one TRB.
        assert_eq!(after.ring.dequeue, before.ring.dequeue + 16);
        // With the device's toggle reset too, the next transfer works.
        let clear = Setup::clear_halt(0x02);
        xhci.control_transfer(d.slot, clear, &mut []).unwrap();
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[0; 31]), Ok(31));
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint drops DCI 4 while it runs")]
    fn the_fake_refuses_to_drop_a_running_endpoint() {
        let stick = FakeUsbDevice::kingston_stick();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        let _ = xhci.add_again(d.slot, 4);
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint drops DCI 5, which is not configured")]
    fn the_fake_refuses_to_drop_an_endpoint_that_is_not_there() {
        let stick = FakeUsbDevice::kingston_stick();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick, &[0]);
        // As if the driver had lost track of which endpoint it set up.
        let s = xhci.slots[d.slot as usize].as_mut().unwrap();
        s.endpoints[1].dci = 5;
        let _ = xhci.add_again(d.slot, 5);
    }

    #[test]
    #[should_panic(expected = "fake xhci: Configure Endpoint adds DCI 3 again without dropping it")]
    fn the_fake_refuses_to_add_an_endpoint_twice() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0]);
        // As if the driver had forgotten it configured the device.
        let s = xhci.slots[d.slot as usize].as_mut().unwrap();
        for ep in core::mem::take(&mut s.endpoints) {
            ep.free(&hal);
        }
        let _ = xhci.configure(&d, &[0]);
    }
}
````

- [ ] **Step 5: Add the failing tests to `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        xhci.clear_halt(d.slot, 0x81).unwrap();
        // Not halted: Stop Endpoint instead of Reset Endpoint.
        assert_eq!(commands_since(&hal, n), [15, 16]);
        let mut buf = [0; 8];
````

with:

````rust
        xhci.clear_halt(d.slot, 0x81).unwrap();
        // Not halted: Stop Endpoint, then Configure Endpoint dropping and
        // adding it (the data toggle starts over, as on the device).
        assert_eq!(commands_since(&hal, n), [15, 12]);
        let mut buf = [0; 8];
````

Replace:

````rust
    #[test]
    fn a_stalled_bulk_endpoint_stays_halted_until_its_halt_is_cleared() {
````

with:

````rust
    #[test]
    fn clearing_the_halt_of_an_endpoint_that_is_not_halted_resets_the_toggle_too() {
        for (hal, mut xhci, d, dev) in sticks() {
            // One packet each way: both sides' toggles are 1 now.
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Ok(31));
            dev.borrow_mut().push_in(0x81, &[2; 13]);
            let mut buf = vec![0; 512];
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
            // A running endpoint: Stop Endpoint, then Configure Endpoint
            // dropping and adding it, which resets the host's toggle as
            // CLEAR_FEATURE resets the device's (xHCI 4.6.8).
            let n = hal.fake().executed().len();
            xhci.clear_halt(d.slot, 0x02).unwrap();
            assert_eq!(commands_since(&hal, n), [15, 12]);
            assert_eq!(xhci.bulk_out(d.slot, 0x02, &[3; 31]), Ok(31));
            // A stopped one (after a timeout): Configure Endpoint only.
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Timeout));
            let n = hal.fake().executed().len();
            xhci.clear_halt(d.slot, 0x81).unwrap();
            assert_eq!(commands_since(&hal, n), [12]);
            dev.borrow_mut().push_in(0x81, &[4; 13]);
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
            // A halted one: Reset Endpoint and Set TR Dequeue Pointer.
            dev.borrow_mut().stall_endpoint(0x81);
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Err(UsbError::Stall));
            let n = hal.fake().executed().len();
            xhci.clear_halt(d.slot, 0x81).unwrap();
            assert_eq!(commands_since(&hal, n), [14, 16]);
            dev.borrow_mut().push_in(0x81, &[5; 13]);
            assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(13));
        }
    }

    #[test]
    #[should_panic(expected = "fake xhci: data toggle mismatch on slot 1 DCI 4: host 1, device 0")]
    fn the_fake_drops_a_transfer_whose_data_toggle_the_device_does_not_expect() {
        let (_hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Ok(31));
        // CLEAR_FEATURE alone: the device's toggle starts over, the host's
        // does not.
        xhci.control(d.slot, Setup::clear_halt(0x02), &mut [])
            .unwrap();
        let _ = xhci.bulk_out(d.slot, 0x02, &[1; 31]);
    }

    #[test]
    fn toggles_count_packets_and_a_zero_length_transfer_is_one() {
        // SuperSpeed: sequence numbers modulo 32, packets of 1024 bytes.
        let (hal, mut xhci, d, dev) = stick(FakeConfig::intel(), 13);
        let s = d.slot as usize;
        dev.borrow_mut().push_in(0x81, &[7; 1025]);
        dev.borrow_mut().push_in(0x81, &[]);
        let mut buf = vec![0; 2048];
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(1025));
        assert_eq!(hal.fake().endpoint(s, 3).unwrap().toggle, 2);
        assert_eq!(xhci.bulk_in(d.slot, 0x81, &mut buf), Ok(0));
        assert_eq!(hal.fake().endpoint(s, 3).unwrap().toggle, 3);
        // High-speed: a data toggle, packets of 512 bytes.
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::basic(), 3);
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 1024]), Ok(1024));
        assert_eq!(hal.fake().endpoint(d.slot as usize, 4).unwrap().toggle, 0);
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 1025]), Ok(1025));
        assert_eq!(hal.fake().endpoint(d.slot as usize, 4).unwrap().toggle, 1);
    }

    #[test]
    #[should_panic(expected = "fake xhci: data toggle mismatch on slot 1 DCI 4: host 1, device 0")]
    fn set_configuration_starts_the_device_toggles_over() {
        let (_hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        assert_eq!(xhci.bulk_out(d.slot, 0x02, &[1; 31]), Ok(31));
        xhci.control(d.slot, Setup::set_configuration(1), &mut [])
            .unwrap();
        let _ = xhci.bulk_out(d.slot, 0x02, &[1; 31]);
    }

    #[test]
    fn a_stalled_bulk_endpoint_stays_halted_until_its_halt_is_cleared() {
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` no method named `add_again` found for struct `xhci::Xhci<H>` in the current scope ``.

- [ ] **Step 7: Change `crates/usb/src/xhci/configure.rs`**

In `crates/usb/src/xhci/configure.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        lost: false,
    })
````

with:

````rust
        lost: false,
        context: EndpointContext::default(),
    })
````

Replace:

````rust
            };
            input.set_endpoint(
                dci,
                &endpoint_context(e, s.speed, ep.ring.enqueue_pointer()),
            );
            add |= 1 << dci;
````

with:

````rust
            };
            let mut ep = ep;
            ep.context = endpoint_context(e, s.speed, ep.ring.enqueue_pointer());
            input.set_endpoint(dci, &ep.context);
            add |= 1 << dci;
````

Replace:

````rust
            })?;
        Ok(())
````

with:

````rust
            })?;
        Ok(())
    }
}

impl<H: Hal> Xhci<H> {
    /// Drops endpoint `dci` of `slot` and adds it again with one Configure
    /// Endpoint: its context as `configure` made it, the ring going on from
    /// where the next TRB will go (whatever was queued is dropped). The
    /// controller starts it over, with its data toggle or sequence number
    /// 0 (xHCI 4.6.6, and 4.6.8's note on endpoints that are not Halted).
    /// The endpoint must be stopped.
    pub(super) fn add_again(&mut self, slot: u8, dci: usize) -> Result<(), UsbError> {
        let stride = self.info.context_size;
        let s = self
            .slots
            .get(slot as usize)
            .and_then(Option::as_ref)
            .ok_or(UsbError::Disconnected)?;
        let ep = s
            .endpoints
            .iter()
            .find(|e| e.dci == dci)
            .ok_or(UsbError::Disconnected)?;
        let highest = s.endpoints.iter().map(|e| e.dci).max().unwrap_or(dci);
        let input = Input::new(&s.input, stride);
        input.clear();
        input.set_flags(1 << dci, A0 | 1 << dci);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            context_entries: highest as u8,
            root_port: s.port,
            ..SlotContext::default()
        });
        let context = EndpointContext {
            dequeue: ep.ring.enqueue_pointer(),
            ..ep.context
        };
        input.set_endpoint(dci, &context);
        let trb = Trb::configure_endpoint(s.input.phys(), slot, false);
        self.command(trb)?;
        Ok(())
````

- [ ] **Step 8: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
    lost: bool,
}
````

with:

````rust
    lost: bool,
    /// The context `configure` gave it, to add it again (`clear_halt`).
    context: context::EndpointContext,
}
````

- [ ] **Step 9: Change `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
````

with:

````rust

    /// Puts an endpoint back to its start, as CLEAR_FEATURE(ENDPOINT_HALT)
    /// puts the device's (USB 2.0 9.4.5): the ring after what was queued,
    /// and the data toggle (sequence number) 0. Reset Endpoint does that for
    /// a Halted endpoint; one that is not halted is stopped if it runs and
    /// then dropped and added again (xHCI 4.6.8's note; Linux does the same
    /// in `xhci_endpoint_reset`). Otherwise the host would go on with its
    /// old toggle and the device drop the next packet as a repeat.
    fn restart(&mut self, slot: u8, dci: usize) -> Result<(), UsbError> {
        match self.endpoint_state(slot as usize, dci)? {
            EP_HALTED => {
                self.command(Trb::reset_endpoint(slot, dci))?;
                self.set_dequeue(slot as usize, dci)
            }
            EP_RUNNING => {
                self.command(Trb::stop_endpoint(slot, dci))?;
                self.add_again(slot, dci)
            }
            EP_STOPPED | EP_ERROR => self.add_again(slot, dci),
            EP_DISABLED => Err(UsbError::Disconnected),
            _ => Err(UsbError::Unsupported("reserved endpoint state")),
        }
    }

    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
````

Replace:

````rust

    /// Reset Endpoint if the context says Halted (Stop Endpoint if it still
    /// runs), Set TR Dequeue Pointer to the enqueue position, dropping
    /// anything outstanding, then CLEAR_FEATURE(ENDPOINT_HALT). An endpoint
    /// lost after a failed abort is usable again once this succeeds.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
````

with:

````rust

    /// Restarts the endpoint on the controller (Reset Endpoint and Set TR
    /// Dequeue Pointer if it is Halted, else Stop Endpoint if it runs and a
    /// Configure Endpoint that drops and adds it), dropping anything
    /// outstanding and resetting the data toggle, then
    /// CLEAR_FEATURE(ENDPOINT_HALT). An endpoint lost after a failed abort
    /// is usable again once this succeeds.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
````

Replace:

````rust
        ep.transfer = Transfer::Idle;
        self.reposition(slot as usize, dci)?;
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
````

with:

````rust
        ep.transfer = Transfer::Idle;
        self.restart(slot, dci)?;
        if let Some(ep) = self.endpoint_mut(slot, endpoint) {
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 235 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -m "usb: clearing a halt resets the host's data toggle too"
````


### Task 3: EP0 is not used again until its abort succeeded

Review finding (minor): when a control request times out or fails and repositioning EP0 fails too, the next control request reused the slot's data buffer while the controller might still own the old TD. Bulk endpoints are kept out of use in that case since Task 1; EP0 now is too: the next control request first tries the reposition again, and if that fails it returns an error without queuing anything. Plan 5's reset recovery sends three control requests in a row, so it would reach this (decision 1).

**Files:**
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2; plan 4's `control_transfer`.
- Produces: `Slot` remembers a failed EP0 reposition; `Bus::control` refuses to queue on such an EP0 until a reposition succeeds.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, replace:

````rust
    #[test]
    fn many_requests_wrap_the_ep0_ring() {
````

with:

````rust
    #[test]
    fn ep0_is_not_used_again_until_its_abort_succeeded() {
        for (config, port, dev) in both() {
            let (hal, mut xhci, d) = attached(config, port, &dev);
            // The request is never answered and its abort hangs: the
            // controller may still own the TD and the data buffer.
            dev.borrow_mut().ignore_requests(1);
            hal.fake().config_mut().hang_command = Some(15); // Stop Endpoint
            assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
            assert!(hal.log_text().contains("slot 1: EP0 recovery failed"));
            // The next request tries the abort again, which fails again:
            // it is refused, and nothing is queued for the device.
            let seen = dev.borrow().requests().len();
            let rung = hal.fake().requests().len();
            assert_eq!(
                get_device(&mut xhci, d.slot, 18),
                Err(UsbError::Unsupported("EP0 lost after a failed abort"))
            );
            assert_eq!(dev.borrow().requests().len(), seen);
            assert_eq!(hal.fake().requests().len(), rung);
            // Once the controller stops the endpoint, EP0 works again.
            hal.fake().config_mut().hang_command = None;
            let n = hal.fake().executed().len();
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18));
            assert_eq!(commands_since(&hal, n), [15, 16], "the abort done again");
            assert!(hal.log_text().contains("slot 1: EP0 recovered"));
            let n = hal.fake().executed().len();
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18));
            assert!(commands_since(&hal, n).is_empty(), "and not again");
        }
    }

    #[test]
    fn a_controller_that_dies_while_ep0_is_recovered_is_reported_dead() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &k120);
        k120.borrow_mut().ignore_requests(1);
        hal.fake().config_mut().hang_command = Some(15); // Stop Endpoint
        assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
        // The abort done again hangs, and so does aborting that command:
        // the controller is given up.
        hal.fake().config_mut().abort_never_completes = true;
        assert_eq!(
            get_device(&mut xhci, d.slot, 18),
            Err(UsbError::ControllerDead)
        );
    }

    #[test]
    fn many_requests_wrap_the_ep0_ring() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 2 tests fail: `xhci::transfer::tests::ep0_is_not_used_again_until_its_abort_succeeded`, `xhci::transfer::tests::a_controller_that_dies_while_ep0_is_recovered_is_reported_dead`.

- [ ] **Step 3: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    control: Option<Control>,
    /// What `configure` set up.
````

with:

````rust
    control: Option<Control>,
    /// A request failed and aborting it failed too: the controller may
    /// still own its TD and the data buffer, so EP0 takes no request until
    /// the abort has been done again.
    ep0_lost: bool,
    /// What `configure` set up.
````

Replace:

````rust
                control: None,
                endpoints: Vec::new(),
````

with:

````rust
                control: None,
                ep0_lost: false,
                endpoints: Vec::new(),
````

- [ ] **Step 4: Change `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    /// timeout EP0 is reset or stopped and repositioned, so the next
    /// request works.
    pub(super) fn control_transfer(
````

with:

````rust
    /// timeout EP0 is reset or stopped and repositioned, so the next
    /// request works. If that fails, the controller may still own the old
    /// TD: the next request does it again first, and is refused (nothing
    /// queued) if it fails again.
    pub(super) fn control_transfer(
````

Replace:

````rust
        }
        let Some(s) = self.slots.get_mut(slot as usize).and_then(Option::as_mut) else {
            return Err(UsbError::Disconnected);
        };
        if s.control.is_some() {
````

with:

````rust
        }
        if self
            .slots
            .get(slot as usize)
            .and_then(Option::as_ref)
            .is_some_and(|s| s.ep0_lost)
        {
            if let Err(e) = self.reposition(slot as usize, EP0) {
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot}: EP0 recovery failed again: {e}"
                );
                // An abort that never ends gives the controller up.
                return Err(if self.dead {
                    UsbError::ControllerDead
                } else {
                    UsbError::Unsupported("EP0 lost after a failed abort")
                });
            }
            xlog!(&self.hal, &self.name, "slot {slot}: EP0 recovered");
        }
        let Some(s) = self.slots.get_mut(slot as usize).and_then(Option::as_mut) else {
            return Err(UsbError::Disconnected);
        };
        s.ep0_lost = false;
        if s.control.is_some() {
````

Replace:

````rust
                &self.name,
                "slot {slot}: EP0 recovery failed: {e}"
            );
        }
````

with:

````rust
                &self.name,
                "slot {slot}: EP0 recovery failed: {e}; EP0 not used until it succeeds"
            );
            if let Some(s) = self.slots.get_mut(slot as usize).and_then(Option::as_mut) {
                s.ep0_lost = true;
            }
        }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 237 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "usb: EP0 is not used again until its abort succeeded"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 11 scenario(s) passed`.

````bash
git push -u origin plan5/bulk
gh pr create --base main --head plan5/bulk --title "Plan 5: Bulk transfers" --body-file - <<'EOF'
## What

Milestone 1, plan 5, tasks 1–3: `Bus::bulk_in`/`bulk_out` of up to 64 KiB through one Normal TRB into a 64 KiB-aligned buffer, the 5 s timeout with Stop Endpoint and Set TR Dequeue Pointer, a halted endpoint refused at once, an endpoint whose abort failed kept out of use until `clear_halt`, `Bus::sleep`; `clear_halt` resetting the controller's data toggle with a Configure Endpoint drop and add; EP0 kept out of use after a failed abort; the fake controller checks TRB lengths, 64 KiB boundaries and data toggles.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan5/bulk --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-bulk
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: The mass storage driver (Tasks 4–9)

Bulk-Only Transport and SCSI (spec §6.4), from the wire formats to the disk the host offers the kernel, tested against a fake storage device modelled on the Kingston stick and QEMU's `usb-storage`, as strict as a real one.

Branch `plan5/mass-storage`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-mass-storage`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan5/mass-storage /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-mass-storage origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-mass-storage
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan5/bulk` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan5/mass-storage /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-mass-storage plan5/bulk`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan5/bulk>` and re-run `cargo xtask ci` before pushing.

### Task 4: Mass storage wrappers and SCSI commands

The two layers of spec §6.4, as bytes. `storage::bot` builds the 31-byte Command Block Wrapper (BOT 5.1: signature `USBC`, a tag, the data length and direction, LUN 0, the command block) and checks the 13-byte Command Status Wrapper the device answers with before believing it (BOT 6.3: exactly 13 bytes, signature `USBS`, the tag of its CBW, a status of 0–2, a residue no larger than the data length); each check that fails is named in a `UsbError::Protocol`. `storage::scsi` builds the command blocks the driver sends (INQUIRY, TEST UNIT READY, REQUEST SENSE, READ CAPACITY(10) and (16), READ(10), WRITE(10), SYNCHRONIZE CACHE(10)) and parses the answers: INQUIRY's peripheral type, removable bit and names (non-printable bytes as `?`), the capacity, and sense data in fixed and descriptor format. A failed command's sense becomes `UsbError::Sense`, shown as `NOT READY (asc 0x3a, ascq 0x00)`, so a `[FAIL]` line or `dmesg` says why. The tests use the real INQUIRY answer of the Kingston stick, as Linux read it.

**Files:**
- Modify: `crates/usb/src/error.rs`
- Modify: `crates/usb/src/lib.rs`
- Create: `crates/usb/src/storage/bot.rs`
- Create: `crates/usb/src/storage/mod.rs`
- Create: `crates/usb/src/storage/scsi.rs`

**Interfaces:**
- Consumes: `UsbError` (plan 4).
- Produces: `usb::storage::{bot::{CBW_LEN = 31, CSW_LEN = 13, Cbw { tag, data_length, dir_in, cdb } (to_bytes), Csw { tag, residue, status } (parse(bytes, expected_tag, data_length)), CswStatus { Passed, Failed, PhaseError }}, scsi::{opcode and sense-key constants, inquiry, test_unit_ready, request_sense, read_capacity_10, read_capacity_16, read_10, write_10, synchronize_cache_10, Inquiry::parse, Capacity { last_lba, block_size } (parse_10, parse_16), Sense { key, asc, ascq } (parse, Display)}}`; `UsbError::{Protocol(&'static str), Sense(Sense)}`.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/error.rs`**

In `crates/usb/src/error.rs`, replace:

````rust
        );
    }
````

with:

````rust
        );
        assert_eq!(
            UsbError::Protocol("bad CSW signature").to_string(),
            "protocol error: bad CSW signature"
        );
        let not_ready = Sense {
            key: 2,
            asc: 0x3A,
            ascq: 0,
        };
        assert_eq!(
            UsbError::Sense(not_ready).to_string(),
            "NOT READY (asc 0x3a, ascq 0x00)"
        );
    }
````

- [ ] **Step 2: Declare the new module in `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust
pub mod host;
#[cfg(test)]
````

with:

````rust
pub mod host;
pub mod storage;
#[cfg(test)]
````

- [ ] **Step 3: Write the failing tests for `crates/usb/src/storage/bot.rs`**

Create `crates/usb/src/storage/bot.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn csw(signature: u32, tag: u32, residue: u32, status: u8) -> [u8; CSW_LEN] {
        let mut b = [0; CSW_LEN];
        b[0..4].copy_from_slice(&signature.to_le_bytes());
        b[4..8].copy_from_slice(&tag.to_le_bytes());
        b[8..12].copy_from_slice(&residue.to_le_bytes());
        b[12] = status;
        b
    }

    #[test]
    fn a_read_cbw_is_byte_exact() {
        // READ(10) of 128 blocks at LBA 0x12345678: 64 KiB in.
        let cdb = [0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0];
        let cbw = Cbw {
            tag: 0x0102_0304,
            data_length: 0x1_0000,
            dir_in: true,
            cdb: &cdb,
        };
        assert_eq!(
            cbw.to_bytes(),
            [
                0x55, 0x53, 0x42, 0x43, // "USBC"
                0x04, 0x03, 0x02, 0x01, // tag
                0x00, 0x00, 0x01, 0x00, // 65536 bytes
                0x80, // data in
                0x00, // LUN 0
                10,   // CB length
                0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0, // the CDB
                0, 0, 0, 0, 0, 0, // padded to 16
            ]
        );
    }

    #[test]
    fn an_out_cbw_and_one_without_data_have_flags_0() {
        let cdb = [0x2A, 0, 0, 0, 0, 1, 0, 0, 1, 0];
        let b = Cbw {
            tag: 7,
            data_length: 512,
            dir_in: false,
            cdb: &cdb,
        }
        .to_bytes();
        assert_eq!((&b[8..12], b[12], b[14]), (&[0, 2, 0, 0][..], 0, 10));
        let b = Cbw {
            tag: 8,
            data_length: 0,
            dir_in: false,
            cdb: &[0; 6],
        }
        .to_bytes();
        assert_eq!((b[4], b[12], b[13], b[14]), (8, 0, 0, 6));
        assert!(b[15..].iter().all(|&x| x == 0));
    }

    #[test]
    fn a_16_byte_cdb_fills_the_block_and_a_longer_one_is_cut() {
        let cdb: [u8; 16] = core::array::from_fn(|i| i as u8 + 1);
        let b = Cbw {
            tag: 1,
            data_length: 32,
            dir_in: true,
            cdb: &cdb,
        }
        .to_bytes();
        assert_eq!((b[14], &b[15..]), (16, &cdb[..]));
        let long = [0xAA; 20];
        let b = Cbw {
            tag: 1,
            data_length: 0,
            dir_in: false,
            cdb: &long,
        }
        .to_bytes();
        assert_eq!((b[14], &b[15..]), (16, &long[..16]));
    }

    #[test]
    fn a_good_csw_parses() {
        let b = csw(CSW_SIGNATURE, 42, 0, 0);
        assert_eq!(
            Csw::parse(&b, 42, 512),
            Ok(Csw {
                tag: 42,
                residue: 0,
                status: CswStatus::Passed
            })
        );
        let b = csw(CSW_SIGNATURE, 43, 512, 1);
        assert_eq!(
            Csw::parse(&b, 43, 512).map(|c| (c.residue, c.status)),
            Ok((512, CswStatus::Failed))
        );
        // A phase error's residue means nothing (BOT 6.3.2), so it is not
        // checked.
        let b = csw(CSW_SIGNATURE, 44, u32::MAX, 2);
        assert_eq!(
            Csw::parse(&b, 44, 0).map(|c| c.status),
            Ok(CswStatus::PhaseError)
        );
    }

    #[test]
    fn each_csw_check_fails_on_its_own() {
        let good = csw(CSW_SIGNATURE, 5, 0, 0);
        assert!(Csw::parse(&good, 5, 0).is_ok());
        let err = |b: &[u8], tag, len| match Csw::parse(b, tag, len) {
            Err(UsbError::Protocol(what)) => what,
            other => panic!("expected a protocol error, got {other:?}"),
        };
        assert_eq!(err(&good[..12], 5, 0), "CSW not 13 bytes");
        let mut long = good.to_vec();
        long.push(0);
        assert_eq!(err(&long, 5, 0), "CSW not 13 bytes");
        assert_eq!(err(&[], 5, 0), "CSW not 13 bytes");
        assert_eq!(err(&csw(CBW_SIGNATURE, 5, 0, 0), 5, 0), "bad CSW signature");
        assert_eq!(err(&good, 6, 0), "CSW tag does not match its CBW");
        assert_eq!(err(&csw(CSW_SIGNATURE, 5, 0, 3), 5, 0), "bad CSW status");
        assert_eq!(err(&csw(CSW_SIGNATURE, 5, 0, 0xFF), 5, 0), "bad CSW status");
        assert_eq!(
            err(&csw(CSW_SIGNATURE, 5, 513, 0), 5, 512),
            "CSW residue over the data length"
        );
        assert_eq!(
            err(&csw(CSW_SIGNATURE, 5, 1, 1), 5, 0),
            "CSW residue over the data length"
        );
        // The residue may be the whole length.
        assert!(Csw::parse(&csw(CSW_SIGNATURE, 5, 512, 0), 5, 512).is_ok());
    }
}
````

- [ ] **Step 4: Create `crates/usb/src/storage/mod.rs`**

Create `crates/usb/src/storage/mod.rs`:

````rust
//! The USB mass-storage class driver (spec §6.4): Bulk-Only Transport
//! (USB Mass Storage Class Bulk-Only Transport 1.0) carrying SCSI commands
//! (SPC-4, SBC-3) to LUN 0 of a USB stick.

pub mod bot;
pub mod scsi;
````

- [ ] **Step 5: Write the failing tests for `crates/usb/src/storage/scsi.rs`**

Create `crates/usb/src/storage/scsi.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The Kingston DataTraveler 3.0's INQUIRY data as Linux read it
    /// (`/sys/block/sda/device/inquiry`).
    const KINGSTON: [u8; 62] = [
        0x00, 0x80, 0x06, 0x02, 0x39, 0x00, 0x00, 0x00, //
        b'K', b'i', b'n', b'g', b's', b't', b'o', b'n', //
        b'D', b'a', b't', b'a', b'T', b'r', b'a', b'v', //
        b'e', b'l', b'e', b'r', b' ', b'3', b'.', b'0', //
        b'P', b'M', b'A', b'P', //
        0x50, 0x4d, 0x41, 0x50, 0x31, 0x32, 0x33, 0x34, 0x87, 0x5b, 0x82, 0xc9, 0x22, 0xb4, 0x96,
        0x9e, 0xa5, 0x84, 0xe9, 0x8f, 0xbc, 0xe1, 0x04, 0x60, 0x04, 0xc0,
    ];

    #[test]
    fn command_blocks_are_byte_exact() {
        assert_eq!(inquiry(36), [0x12, 0, 0, 0, 36, 0]);
        assert_eq!(inquiry(0x0102), [0x12, 0, 0, 0x01, 0x02, 0]);
        assert_eq!(test_unit_ready(), [0; 6]);
        assert_eq!(request_sense(18), [0x03, 0, 0, 0, 18, 0]);
        assert_eq!(read_capacity_10(), [0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            read_capacity_16(32),
            [0x9E, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0]
        );
        assert_eq!(
            read_capacity_16(0x0102_0304)[10..14],
            [0x01, 0x02, 0x03, 0x04]
        );
        // SBC-3 table 97: operation code, flags, LBA (big-endian), group,
        // transfer length (big-endian), control.
        assert_eq!(
            read_10(0x1234_5678, 128),
            [0x28, 0, 0x12, 0x34, 0x56, 0x78, 0, 0, 0x80, 0]
        );
        assert_eq!(
            write_10(0xFFFF_FFFF, 0xFFFF),
            [0x2A, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0xFF, 0xFF, 0]
        );
        assert_eq!(write_10(1, 1), [0x2A, 0, 0, 0, 0, 1, 0, 0, 1, 0]);
        assert_eq!(synchronize_cache_10(), [0x35, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn commands_have_names_for_the_log() {
        assert_eq!(command_name(READ_10), "READ(10)");
        assert_eq!(command_name(WRITE_10), "WRITE(10)");
        assert_eq!(command_name(TEST_UNIT_READY), "TEST UNIT READY");
        assert_eq!(command_name(SERVICE_ACTION_IN_16), "READ CAPACITY(16)");
        assert_eq!(command_name(SYNCHRONIZE_CACHE_10), "SYNCHRONIZE CACHE(10)");
        assert_eq!(command_name(0xA0), "command");
    }

    #[test]
    fn the_kingston_inquiry_data_parses() {
        let i = Inquiry::parse(&KINGSTON).unwrap();
        assert_eq!(
            i,
            Inquiry {
                peripheral_type: 0,
                removable: true,
                vendor: "Kingston".into(),
                product: "DataTraveler 3.0".into(),
                revision: "PMAP".into(),
            }
        );
        // Only the 36 bytes asked for.
        assert_eq!(Inquiry::parse(&KINGSTON[..36]), Ok(i));
    }

    #[test]
    fn inquiry_names_are_trimmed_and_cleaned() {
        let mut b = [0u8; 36];
        b[0] = 0x05; // a CD-ROM
        b[4] = 31;
        b[8..16].copy_from_slice(b"QEMU    ");
        b[16..32].copy_from_slice(b"QEMU HARDDISK   ");
        b[32..36].copy_from_slice(b"2.5+");
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.peripheral_type, i.removable), (5, false));
        assert_eq!(
            (i.vendor.as_str(), i.product.as_str()),
            ("QEMU", "QEMU HARDDISK")
        );
        assert_eq!(i.revision, "2.5+");
        // Leading blanks, NUL padding and bytes outside printable ASCII.
        b[8..16].copy_from_slice(b"  Ven\x01\0\0");
        b[16..32].copy_from_slice(b"\xFFroduct\0\0\0\0\0\0\0\0\0");
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("Ven?", "?roduct"));
        // The peripheral qualifier is not part of the type, and only bit 7
        // of byte 1 is RMB (bit 6 is LU_CONG).
        b[0] = 0x20;
        b[1] = 0x40;
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.peripheral_type, i.removable), (0, false));
    }

    #[test]
    fn short_inquiry_data_gives_what_came() {
        let i = Inquiry::parse(&KINGSTON[..5]).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("", ""));
        assert!(i.removable);
        let i = Inquiry::parse(&KINGSTON[..20]).unwrap();
        assert_eq!(
            (i.vendor.as_str(), i.product.as_str()),
            ("Kingston", "Data")
        );
        // The additional length says less than came: the rest is not used.
        let mut b = KINGSTON;
        b[4] = 11;
        let i = Inquiry::parse(&b).unwrap();
        assert_eq!((i.vendor.as_str(), i.product.as_str()), ("Kingston", ""));
        for len in 0..5 {
            assert_eq!(
                Inquiry::parse(&KINGSTON[..len]),
                Err(UsbError::Protocol("INQUIRY data too short"))
            );
        }
    }

    #[test]
    fn capacities_parse_including_the_largest() {
        // The Kingston stick: 30,277,632 blocks of 512 bytes.
        let c = Capacity::parse_10(&[0x01, 0xCD, 0xFF, 0xFF, 0, 0, 2, 0]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (30_277_631, 512));
        let c = Capacity::parse_10(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (0xFFFF_FFFF, 4096));
        assert!(Capacity::parse_10(&[0; 7]).is_err());
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&0x1_2345_6789u64.to_be_bytes());
        b[8..12].copy_from_slice(&512u32.to_be_bytes());
        let c = Capacity::parse_16(&b).unwrap();
        assert_eq!((c.last_lba, c.block_size), (0x1_2345_6789, 512));
        assert_eq!(Capacity::parse_16(&b[..12]), Ok(c));
        let c = Capacity::parse_16(&[0xFF; 32]).unwrap();
        assert_eq!((c.last_lba, c.block_size), (u64::MAX, u32::MAX));
        assert_eq!(
            Capacity::parse_16(&b[..11]),
            Err(UsbError::Protocol("READ CAPACITY data too short"))
        );
    }

    #[test]
    fn fixed_and_descriptor_sense_data_parse() {
        // Fixed format: NOT READY, becoming ready.
        let mut fixed = [0u8; 18];
        fixed[0] = 0x70;
        fixed[2] = 0x02;
        fixed[7] = 10;
        fixed[12] = 0x04;
        fixed[13] = 0x01;
        let s = Sense::parse(&fixed).unwrap();
        assert_eq!(
            s,
            Sense {
                key: 2,
                asc: 4,
                ascq: 1
            }
        );
        // Deferred errors and the VALID bit; the FILEMARK/EOM/ILI bits
        // are not part of the key.
        fixed[0] = 0xF1;
        fixed[2] = 0xE3;
        assert_eq!(Sense::parse(&fixed).unwrap().key, 3);
        // Descriptor format: UNIT ATTENTION, power on or reset.
        let desc = [0x72, 0x06, 0x29, 0x00, 0, 0, 0, 0];
        assert_eq!(
            Sense::parse(&desc),
            Ok(Sense {
                key: 6,
                asc: 0x29,
                ascq: 0
            })
        );
        assert_eq!(Sense::parse(&[0x73, 0x05, 0x20, 0x00]).unwrap().asc, 0x20);
    }

    #[test]
    fn short_and_garbage_sense_data_never_panics() {
        // Fixed format with only the key: no ASC and ASCQ.
        assert_eq!(
            Sense::parse(&[0x70, 0, 0x05]),
            Ok(Sense {
                key: 5,
                asc: 0,
                ascq: 0
            })
        );
        // The additional length does not reach the ASC: not believed.
        let mut b = [0u8; 18];
        b[0] = 0x70;
        b[2] = 0x03;
        b[7] = 4;
        b[12] = 0x11;
        assert_eq!(Sense::parse(&b).unwrap().asc, 0);
        assert!(Sense::parse(&[]).is_err());
        assert!(Sense::parse(&[0x70, 0]).is_err());
        assert!(Sense::parse(&[0x72, 0x06, 0x29]).is_err());
        assert_eq!(
            Sense::parse(&[0x00; 18]),
            Err(UsbError::Protocol("unknown sense data format"))
        );
        for len in 0..=18 {
            for fill in [0x00, 0x70, 0x72, 0xFF] {
                let _ = Sense::parse(&[fill; 18][..len]);
            }
        }
    }

    #[test]
    fn sense_reads_well_in_a_log_line() {
        let s = |key, asc, ascq| Sense { key, asc, ascq }.to_string();
        assert_eq!(s(2, 4, 1), "NOT READY (asc 0x04, ascq 0x01)");
        assert_eq!(s(3, 0x11, 0), "MEDIUM ERROR (asc 0x11, ascq 0x00)");
        assert_eq!(s(6, 0x29, 0), "UNIT ATTENTION (asc 0x29, ascq 0x00)");
        assert_eq!(s(7, 0x27, 0), "DATA PROTECT (asc 0x27, ascq 0x00)");
        assert_eq!(s(5, 0x20, 0), "ILLEGAL REQUEST (asc 0x20, ascq 0x00)");
        assert_eq!(s(0, 0, 0), "NO SENSE (asc 0x00, ascq 0x00)");
        assert_eq!(s(0xB, 0, 0), "ABORTED COMMAND (asc 0x00, ascq 0x00)");
        assert_eq!(s(0xE, 0x1D, 0), "MISCOMPARE (asc 0x1d, ascq 0x00)");
        assert_eq!(s(0xC, 0, 0), "sense key 0x0c (asc 0x00, ascq 0x00)");
    }
}
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `Sense` in this scope ``; `` cannot find value `CSW_LEN` in this scope ``.

- [ ] **Step 7: Change `crates/usb/src/error.rs`**

In `crates/usb/src/error.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

use core::fmt;
````

with:

````rust

use crate::storage::scsi::Sense;
use core::fmt;
````

Replace:

````rust
    ControllerDead,
}
````

with:

````rust
    ControllerDead,
    /// The device broke the rules of its transport (a bad CSW, a phase
    /// error, data that is too short); this says which.
    Protocol(&'static str),
    /// A SCSI command failed; the sense data says why.
    Sense(Sense),
}
````

Replace:

````rust
            UsbError::ControllerDead => write!(f, "controller stopped working"),
        }
````

with:

````rust
            UsbError::ControllerDead => write!(f, "controller stopped working"),
            UsbError::Protocol(what) => write!(f, "protocol error: {what}"),
            UsbError::Sense(sense) => write!(f, "{sense}"),
        }
````

- [ ] **Step 8: Change `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust
//! The Relay USB stack (spec §6): an xHCI host controller driver and the
//! HID boot-keyboard class driver.
//!
````

with:

````rust
//! The Relay USB stack (spec §6): an xHCI host controller driver, the HID
//! boot-keyboard class driver and the mass-storage class driver.
//!
````

- [ ] **Step 9: Implement `crates/usb/src/storage/bot.rs`**

Insert this at the top of `crates/usb/src/storage/bot.rs`, above `#[cfg(test)]`:

````rust
//! Bulk-Only Transport wrappers (USB Mass Storage Class Bulk-Only
//! Transport 1.0, §5): the 31-byte Command Block Wrapper the host sends on
//! bulk OUT before each command, and the 13-byte Command Status Wrapper the
//! device answers with on bulk IN after the data phase. The CSW comes from
//! the device, so every field is checked before it is believed (BOT 6.3).

use crate::UsbError;

/// A CBW is always 31 bytes (BOT 5.1).
pub const CBW_LEN: usize = 31;
/// A CSW is always 13 bytes (BOT 5.2).
pub const CSW_LEN: usize = 13;
/// `dCBWSignature`: "USBC" in little-endian order.
pub const CBW_SIGNATURE: u32 = 0x4342_5355;
/// `dCSWSignature`: "USBS" in little-endian order.
pub const CSW_SIGNATURE: u32 = 0x5342_5355;
/// The longest command block a CBW carries.
pub const MAX_CDB_LEN: usize = 16;

/// A Command Block Wrapper for LUN 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cbw<'a> {
    /// `dCBWTag`: the device returns it in the CSW.
    pub tag: u32,
    /// `dCBWDataTransferLength`: the bytes the host expects to move.
    pub data_length: u32,
    /// The data phase goes from the device to the host.
    pub dir_in: bool,
    /// The SCSI command block, 1 to 16 bytes (a longer one is cut off).
    pub cdb: &'a [u8],
}

impl Cbw<'_> {
    /// The CBW as it goes on the wire (BOT 5.1, table 5.1).
    pub fn to_bytes(&self) -> [u8; CBW_LEN] {
        let cdb = &self.cdb[..self.cdb.len().min(MAX_CDB_LEN)];
        let mut b = [0; CBW_LEN];
        b[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        b[4..8].copy_from_slice(&self.tag.to_le_bytes());
        b[8..12].copy_from_slice(&self.data_length.to_le_bytes());
        // bmCBWFlags: bit 7 is the direction, the rest is reserved.
        b[12] = if self.dir_in { 0x80 } else { 0 };
        // bCBWLUN stays 0.
        b[14] = cdb.len() as u8;
        b[15..15 + cdb.len()].copy_from_slice(cdb);
        b
    }
}

/// `bCSWStatus` (BOT 5.2, table 5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CswStatus {
    /// Command Passed.
    Passed,
    /// Command Failed: REQUEST SENSE says why.
    Failed,
    /// Phase Error: only a reset recovery helps (BOT 5.3.4).
    PhaseError,
}

/// A Command Status Wrapper that passed every check of BOT 6.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Csw {
    pub tag: u32,
    /// `dCSWDataResidue`: the bytes of the data phase not processed.
    pub residue: u32,
    pub status: CswStatus,
}

impl Csw {
    /// Checks what the device sent for the CBW with `expected_tag` and
    /// `data_length` (BOT 6.3): a CSW is valid if it is 13 bytes with the
    /// signature and that tag, and meaningful if its status is 0 or 1 with a
    /// residue of at most the data length, or 2 (a phase error, whose residue
    /// means nothing). Anything else is a `Protocol` error naming the check.
    pub fn parse(bytes: &[u8], expected_tag: u32, data_length: u32) -> Result<Csw, UsbError> {
        let Ok(b) = <&[u8; CSW_LEN]>::try_from(bytes) else {
            return Err(UsbError::Protocol("CSW not 13 bytes"));
        };
        let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if le32(0) != CSW_SIGNATURE {
            return Err(UsbError::Protocol("bad CSW signature"));
        }
        let tag = le32(4);
        if tag != expected_tag {
            return Err(UsbError::Protocol("CSW tag does not match its CBW"));
        }
        let residue = le32(8);
        let status = match b[12] {
            0 => CswStatus::Passed,
            1 => CswStatus::Failed,
            2 => CswStatus::PhaseError,
            _ => return Err(UsbError::Protocol("bad CSW status")),
        };
        if status != CswStatus::PhaseError && residue > data_length {
            return Err(UsbError::Protocol("CSW residue over the data length"));
        }
        Ok(Csw {
            tag,
            residue,
            status,
        })
    }
}

````

- [ ] **Step 10: Implement `crates/usb/src/storage/scsi.rs`**

Insert this at the top of `crates/usb/src/storage/scsi.rs`, above `#[cfg(test)]`:

````rust
//! The SCSI commands a USB stick needs (SPC-4, SBC-3): command blocks for
//! INQUIRY, TEST UNIT READY, REQUEST SENSE, READ CAPACITY(10) and (16),
//! READ(10), WRITE(10) and SYNCHRONIZE CACHE(10), and parsers for what the
//! device answers. The answers come from the device, so short or garbled
//! data is an error or a `?`, never a panic.

use crate::UsbError;
use alloc::string::String;
use core::fmt;

/// Operation codes (SPC-4 §6, SBC-3 §5).
pub const TEST_UNIT_READY: u8 = 0x00;
pub const REQUEST_SENSE: u8 = 0x03;
pub const INQUIRY: u8 = 0x12;
pub const READ_CAPACITY_10: u8 = 0x25;
pub const READ_10: u8 = 0x28;
pub const WRITE_10: u8 = 0x2A;
pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
pub const SERVICE_ACTION_IN_16: u8 = 0x9E;
/// SERVICE ACTION IN(16)'s service action for READ CAPACITY(16).
pub const READ_CAPACITY_16: u8 = 0x10;

/// The standard INQUIRY data up to the revision (SPC-4 §6.4.2).
pub const INQUIRY_LEN: usize = 36;
/// Fixed-format sense data without additional bytes (SPC-4 §4.5.3).
pub const SENSE_LEN: usize = 18;
/// READ CAPACITY(10) data (SBC-3 §5.15.2).
pub const CAPACITY_10_LEN: usize = 8;
/// READ CAPACITY(16) data (SBC-3 §5.16.2).
pub const CAPACITY_16_LEN: usize = 32;

/// Sense keys (SPC-4 table 49) the driver acts on.
pub const NOT_READY: u8 = 0x02;
pub const MEDIUM_ERROR: u8 = 0x03;
pub const ILLEGAL_REQUEST: u8 = 0x05;
pub const UNIT_ATTENTION: u8 = 0x06;
pub const DATA_PROTECT: u8 = 0x07;

/// INQUIRY with an allocation length of `len` bytes (SPC-4 §6.4.1).
pub fn inquiry(len: u16) -> [u8; 6] {
    let [l0, l1] = len.to_be_bytes();
    [INQUIRY, 0, 0, l0, l1, 0]
}

pub fn test_unit_ready() -> [u8; 6] {
    [TEST_UNIT_READY, 0, 0, 0, 0, 0]
}

/// REQUEST SENSE, fixed format, `len` bytes (SPC-4 §6.29).
pub fn request_sense(len: u8) -> [u8; 6] {
    [REQUEST_SENSE, 0, 0, 0, len, 0]
}

/// READ CAPACITY(10) (SBC-3 §5.15).
pub fn read_capacity_10() -> [u8; 10] {
    [READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// READ CAPACITY(16) with an allocation length of `len` bytes (SBC-3
/// §5.16).
pub fn read_capacity_16(len: u32) -> [u8; 16] {
    let mut c = [0; 16];
    c[0] = SERVICE_ACTION_IN_16;
    c[1] = READ_CAPACITY_16;
    c[10..14].copy_from_slice(&len.to_be_bytes());
    c
}

/// READ(10) or WRITE(10): the LBA and the transfer length big-endian.
fn rw_10(opcode: u8, lba: u32, blocks: u16) -> [u8; 10] {
    let [a0, a1, a2, a3] = lba.to_be_bytes();
    let [n0, n1] = blocks.to_be_bytes();
    [opcode, 0, a0, a1, a2, a3, 0, n0, n1, 0]
}

/// READ(10) of `blocks` blocks from `lba` (SBC-3 §5.8).
pub fn read_10(lba: u32, blocks: u16) -> [u8; 10] {
    rw_10(READ_10, lba, blocks)
}

/// WRITE(10) of `blocks` blocks to `lba` (SBC-3 §5.32).
pub fn write_10(lba: u32, blocks: u16) -> [u8; 10] {
    rw_10(WRITE_10, lba, blocks)
}

/// SYNCHRONIZE CACHE(10) of the whole medium: LBA 0 and 0 blocks, which
/// means "to the end" (SBC-3 §5.22).
pub fn synchronize_cache_10() -> [u8; 10] {
    rw_10(SYNCHRONIZE_CACHE_10, 0, 0)
}

/// The name of a command for log lines.
pub fn command_name(opcode: u8) -> &'static str {
    match opcode {
        TEST_UNIT_READY => "TEST UNIT READY",
        REQUEST_SENSE => "REQUEST SENSE",
        INQUIRY => "INQUIRY",
        READ_CAPACITY_10 => "READ CAPACITY(10)",
        READ_10 => "READ(10)",
        WRITE_10 => "WRITE(10)",
        SYNCHRONIZE_CACHE_10 => "SYNCHRONIZE CACHE(10)",
        SERVICE_ACTION_IN_16 => "READ CAPACITY(16)",
        _ => "command",
    }
}

/// What INQUIRY says about the device (SPC-4 §6.4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inquiry {
    /// 0 is a disk (direct-access block device).
    pub peripheral_type: u8,
    /// The RMB bit: the medium can be removed (a USB stick says so).
    pub removable: bool,
    /// T10 vendor identification, product identification and product
    /// revision level, without padding; bytes outside printable ASCII are
    /// `?`. Empty where the device sent less.
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

/// An ASCII field of INQUIRY data: bytes `range` of `b` as far as they
/// came, without the blanks or NULs around them.
fn ascii_field(b: &[u8], range: core::ops::Range<usize>) -> String {
    let end = range.end.min(b.len());
    let field = b.get(range.start..end).unwrap_or(&[]);
    let blank = |c: &u8| *c == b' ' || *c == 0;
    let first = field.iter().position(|c| !blank(c)).unwrap_or(field.len());
    let last = field
        .iter()
        .rposition(|c| !blank(c))
        .map_or(first, |i| i + 1);
    field[first..last]
        .iter()
        .map(|&c| {
            if (0x20..0x7F).contains(&c) {
                c as char
            } else {
                '?'
            }
        })
        .collect()
}

impl Inquiry {
    /// Parses INQUIRY data; 5 bytes (the header) are enough, the names are
    /// taken from what came and what the additional length covers.
    pub fn parse(b: &[u8]) -> Result<Inquiry, UsbError> {
        if b.len() < 5 {
            return Err(UsbError::Protocol("INQUIRY data too short"));
        }
        // Byte 4 is the number of bytes after it.
        let b = &b[..b.len().min(5 + b[4] as usize)];
        Ok(Inquiry {
            peripheral_type: b[0] & 0x1F,
            removable: b[1] & 0x80 != 0,
            vendor: ascii_field(b, 8..16),
            product: ascii_field(b, 16..32),
            revision: ascii_field(b, 32..36),
        })
    }
}

/// The medium's size: its last LBA and the bytes in a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub last_lba: u64,
    pub block_size: u32,
}

fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

impl Capacity {
    /// READ CAPACITY(10) data: 0xFFFFFFFF as the last LBA means READ
    /// CAPACITY(16) must be asked.
    pub fn parse_10(b: &[u8]) -> Result<Capacity, UsbError> {
        if b.len() < CAPACITY_10_LEN {
            return Err(UsbError::Protocol("READ CAPACITY data too short"));
        }
        Ok(Capacity {
            last_lba: be32(b, 0) as u64,
            block_size: be32(b, 4),
        })
    }

    /// READ CAPACITY(16) data (the first 12 bytes are used).
    pub fn parse_16(b: &[u8]) -> Result<Capacity, UsbError> {
        if b.len() < 12 {
            return Err(UsbError::Protocol("READ CAPACITY data too short"));
        }
        Ok(Capacity {
            last_lba: (be32(b, 0) as u64) << 32 | be32(b, 4) as u64,
            block_size: be32(b, 8),
        })
    }
}

/// Why a command failed: the sense key and additional sense code and
/// qualifier (SPC-4 §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sense {
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
}

impl Sense {
    /// Parses fixed-format (response code 0x70/0x71) or descriptor-format
    /// (0x72/0x73) sense data; only the key, ASC and ASCQ are kept. In
    /// fixed format the ASC and ASCQ count only if the additional sense
    /// length covers them (0 otherwise).
    pub fn parse(b: &[u8]) -> Result<Sense, UsbError> {
        let short = Err(UsbError::Protocol("sense data too short"));
        // Bit 7 of byte 0 is VALID (fixed format), not the response code.
        match b.first().map(|c| c & 0x7F) {
            Some(0x70 | 0x71) => {
                if b.len() < 3 {
                    return short;
                }
                // Byte 7 counts the bytes after it; ASC and ASCQ are 12-13.
                let valid = b.len().min(8 + b.get(7).copied().unwrap_or(0) as usize);
                let byte = |i: usize| if i < valid { b[i] } else { 0 };
                Ok(Sense {
                    key: b[2] & 0x0F,
                    asc: byte(12),
                    ascq: byte(13),
                })
            }
            Some(0x72 | 0x73) => match b {
                [_, key, asc, ascq, ..] => Ok(Sense {
                    key: key & 0x0F,
                    asc: *asc,
                    ascq: *ascq,
                }),
                _ => short,
            },
            Some(_) => Err(UsbError::Protocol("unknown sense data format")),
            None => short,
        }
    }

    /// The sense key's name (SPC-4 table 49).
    fn key_name(&self) -> Option<&'static str> {
        Some(match self.key {
            0x0 => "NO SENSE",
            0x1 => "RECOVERED ERROR",
            0x2 => "NOT READY",
            0x3 => "MEDIUM ERROR",
            0x4 => "HARDWARE ERROR",
            0x5 => "ILLEGAL REQUEST",
            0x6 => "UNIT ATTENTION",
            0x7 => "DATA PROTECT",
            0x8 => "BLANK CHECK",
            0x9 => "VENDOR SPECIFIC",
            0xA => "COPY ABORTED",
            0xB => "ABORTED COMMAND",
            0xD => "VOLUME OVERFLOW",
            0xE => "MISCOMPARE",
            _ => return None,
        })
    }
}

impl fmt::Display for Sense {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.key_name() {
            Some(name) => f.write_str(name)?,
            None => write!(f, "sense key {:#04x}", self.key)?,
        }
        write!(f, " (asc {:#04x}, ascq {:#04x})", self.asc, self.ascq)
    }
}

````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 251 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add crates
git commit -m "usb: mass storage wrappers and SCSI commands"
````


### Task 5: The mass storage driver finds its disk

Spec §6.4's setup, over the `Bus`. `is_mass_storage` picks interfaces of class 8, subclass 6, protocol 0x50 with a bulk IN and a bulk OUT endpoint. `MassStorage::start` reads `GET_MAX_LUN` (a STALL, which BOT 3.2 allows, or any other failure means one LUN; only LUN 0 is used), sends INQUIRY (logged as vendor, product and revision; a peripheral type other than 0 is `not a disk`), TEST UNIT READY every 100 ms for up to 5 s with REQUEST SENSE after each failure (which clears a unit attention; no medium fails at once), then READ CAPACITY(10), and (16) if the answer is 0xFFFFFFFF. Block sizes of 512 to 4096 bytes are accepted; a disk of more than 2^32 blocks is refused, because READ(10) cannot reach further (decision 2). `storage::transport` runs one BOT command: CBW, data phase, CSW, with the recovery setup itself can meet. The new fake storage device plays BOT and SCSI over an in-memory disk with presets for the Kingston DataTraveler 3.0 (its real INQUIRY bytes and 30,277,632 blocks) and QEMU 8.2's `usb-storage` (`QEMU HARDDISK`, burst 15, no unit attention at start, as a real QEMU run showed). It is as strict as a device: a CBW that is not 31 bytes, a wrong signature, LUN, direction or length, a repeated tag, data it does not expect, or a command past the end of the disk panics with `fake storage: …`.

**Files:**
- Create: `crates/usb/src/storage/disk.rs`
- Modify: `crates/usb/src/storage/mod.rs`
- Create: `crates/usb/src/storage/transport.rs`
- Create: `crates/usb/src/testing/bus.rs`
- Modify: `crates/usb/src/testing/device.rs`
- Modify: `crates/usb/src/testing/mod.rs`
- Create: `crates/usb/src/testing/storage/bot.rs`
- Create: `crates/usb/src/testing/storage/mod.rs`
- Create: `crates/usb/src/testing/storage/scsi.rs`

**Interfaces:**
- Consumes: `Bus::{control, bulk_in, bulk_out, clear_halt, sleep}` (Task 1); `storage::{bot, scsi}` (Task 4).
- Produces: `usb::storage::{is_mass_storage(&Interface) -> bool, MassStorage { start(bus: &mut dyn Bus, slot: u8, iface: &Interface) -> Result<MassStorage, UsbError>, slot, block_size, block_count, vendor, product }, Sense}`; `storage::disk::{READY_TIMEOUT = 5 s, READY_POLL = 100 ms}`; `storage::transport::{Transport, Data, MAX_TRIES = 3, is_fatal}`; test support `testing::storage::FakeStorage` (`kingston`, `qemu(blocks)`, `usb2(blocks)`, knobs), `FakeUsbDevice::qemu_stick()`, `testing::bus::TamperBus`.

- [ ] **Step 1: Write the failing tests for `crates/usb/src/storage/disk.rs`**

Create `crates/usb/src/storage/disk.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::Endpoint;
    use crate::storage::Sense;
    use crate::testing::{Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op};
    use crate::xhci::Xhci;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn start(
        config: FakeConfig,
        port: u8,
        stick: &Stick,
    ) -> (FakeHal, Xhci<FakeHal>, Result<MassStorage, UsbError>) {
        let (hal, mut xhci, d) = configured(config, port, stick);
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        (hal, xhci, r)
    }

    fn kingston_with(
        knobs: impl FnOnce(&mut FakeStorage),
    ) -> (FakeHal, Stick, Result<MassStorage, UsbError>) {
        let stick = FakeStorage::kingston();
        knobs(&mut stick.borrow_mut());
        let (hal, _xhci, r) = start(FakeConfig::intel(), 13, &stick);
        (hal, stick, r)
    }

    fn sense(key: u8, asc: u8, ascq: u8) -> UsbError {
        UsbError::Sense(Sense { key, asc, ascq })
    }

    #[test]
    fn the_kingston_stick_starts_on_the_nuc_controller() {
        let stick = FakeStorage::kingston();
        let (hal, _xhci, r) = start(FakeConfig::intel(), 13, &stick);
        let disk = r.unwrap();
        assert_eq!(disk.slot(), 1);
        assert_eq!(
            (disk.vendor(), disk.product()),
            ("Kingston", "DataTraveler 3.0")
        );
        assert_eq!((disk.block_size(), disk.block_count()), (512, 30_277_632));
        assert_eq!(
            stick.borrow().opcodes(),
            [op::INQUIRY, op::TEST_UNIT_READY, op::READ_CAPACITY_10]
        );
        let log = hal.log_text();
        assert!(log.contains(
            "storage: slot 1: vendor \"Kingston\", product \"DataTraveler 3.0\", revision \"PMAP\", removable"
        ));
        assert!(log.contains("storage: slot 1: 30277632 blocks of 512 bytes"));
        assert!(!log.contains("ready after"));
    }

    /// The device's events as GET_MAX_LUN, opcodes, resets and cleared
    /// halts, for comparing sequences.
    fn trace(stick: &Stick) -> Vec<String> {
        let name = |e: &Event| match e {
            Event::GetMaxLun => "GET_MAX_LUN".into(),
            Event::Command(c) => format!("{:02x}", c.opcode),
            Event::Reset => "reset".into(),
            Event::ClearHalt(ep) => format!("clear {ep:02x}"),
        };
        stick.borrow().events().iter().map(name).collect()
    }

    #[test]
    fn the_qemu_stick_starts_with_its_first_test_unit_ready() {
        let stick = FakeStorage::qemu(524_288);
        let (hal, _xhci, r) = start(FakeConfig::qemu(), 2, &stick);
        let disk = r.unwrap();
        assert_eq!((disk.vendor(), disk.product()), ("QEMU", "QEMU HARDDISK"));
        assert_eq!((disk.block_size(), disk.block_count()), (512, 524_288));
        // GET_MAX_LUN, then INQUIRY, TEST UNIT READY (which passes: no
        // REQUEST SENSE) and READ CAPACITY(10), as real QEMU 8.2 answers.
        assert_eq!(trace(&stick), ["GET_MAX_LUN", "12", "00", "25"]);
        // What the boot log of the real run shows, and nothing else.
        let log = hal.log_text();
        let lines: Vec<&str> = log.lines().filter(|l| l.starts_with("storage: ")).collect();
        assert_eq!(
            lines,
            [
                "storage: slot 1: vendor \"QEMU\", product \"QEMU HARDDISK\", revision \"2.5+\"",
                "storage: slot 1: 524288 blocks of 512 bytes",
            ]
        );
    }

    #[test]
    fn a_power_on_unit_attention_is_cleared_by_request_sense() {
        let (hal, stick, r) = kingston_with(|s| s.unit_attention());
        assert!(r.is_ok());
        // The first TEST UNIT READY meets the unit attention; REQUEST
        // SENSE clears it and the next one passes.
        assert_eq!(trace(&stick), ["GET_MAX_LUN", "12", "00", "03", "00", "25"]);
        let log = hal.log_text();
        assert!(
            log.contains("storage: slot 1: TEST UNIT READY: UNIT ATTENTION (asc 0x29, ascq 0x00)")
        );
        assert!(log.contains("storage: slot 1: ready after 2 tries"));
    }

    #[test]
    fn a_stick_that_is_not_ready_for_a_few_tries_becomes_ready() {
        let (hal, stick, r) = kingston_with(|s| s.not_ready_for(3));
        assert!(r.is_ok());
        let turs = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::TEST_UNIT_READY)
            .count();
        assert_eq!(turs, 4);
        let log = hal.log_text();
        // The same sense three times is logged once.
        assert_eq!(
            log.matches("TEST UNIT READY: NOT READY (asc 0x04, ascq 0x01)")
                .count(),
            1
        );
        assert!(log.contains("storage: slot 1: ready after 4 tries"));
    }

    #[test]
    fn a_stick_that_never_becomes_ready_fails_after_5_s_with_its_sense() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().never_ready();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let before = hal.clock();
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        let took = hal.clock() - before;
        assert_eq!(r.err(), Some(sense(2, 4, 1)));
        assert!(
            took >= READY_TIMEOUT && took < READY_TIMEOUT + Duration::from_secs(1),
            "{took:?}"
        );
        assert!(
            hal.log_text()
                .contains("storage: slot 1: not ready after 5 s: NOT READY (asc 0x04, ascq 0x01)")
        );
        // No capacity was asked of a disk that is not ready, and the tries
        // were 100 ms apart.
        let opcodes = stick.borrow().opcodes();
        assert!(!opcodes.contains(&op::READ_CAPACITY_10));
        let turs = opcodes
            .iter()
            .filter(|&&o| o == op::TEST_UNIT_READY)
            .count();
        assert!((50..=52).contains(&turs), "{turs} tries");
    }

    #[test]
    fn a_stick_that_goes_while_it_is_waited_for_fails_at_once() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().never_ready();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let mut bus = TamperBus::new(&mut xhci);
        // INQUIRY and a TEST UNIT READY with its REQUEST SENSE (3 bulk
        // transfers each), then the device is gone.
        bus.fail_bulk = Some((9, UsbError::Disconnected));
        let before = hal.clock();
        let r = MassStorage::start(&mut bus, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(UsbError::Disconnected));
        assert!(hal.clock() - before < Duration::from_secs(1));
        assert_eq!(bus.bulk, 10);
    }

    #[test]
    fn a_stick_gone_before_setup_fails_at_once() {
        let stick = FakeStorage::kingston();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        hal.fake().unplug(13);
        xhci.detach(d.slot);
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(UsbError::Disconnected));
        assert!(!hal.log_text().contains("GET_MAX_LUN"));
    }

    #[test]
    fn a_reader_without_a_medium_fails_at_once() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().no_medium();
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let before = hal.clock();
        let r = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]);
        assert_eq!(r.err(), Some(sense(2, 0x3A, 0)));
        assert!(hal.clock() - before < Duration::from_secs(1));
        assert!(hal.log_text().contains("storage: slot 1: no medium"));
    }

    #[test]
    fn get_max_lun_stalled_means_one_lun_and_more_luns_still_use_lun_0() {
        let (hal, _, r) = kingston_with(|s| s.set_max_lun(None));
        assert!(r.is_ok());
        assert!(
            hal.log_text()
                .contains("storage: slot 1: GET_MAX_LUN stalled: one LUN")
        );
        // The fake panics on a CBW for another LUN.
        let (hal, _, r) = kingston_with(|s| s.set_max_lun(Some(3)));
        assert!(r.is_ok());
        assert!(
            hal.log_text()
                .contains("storage: slot 1: 4 LUNs, only LUN 0 is used")
        );
        let (hal, stick, r) = kingston_with(|_| {});
        assert!(r.is_ok());
        assert!(!hal.log_text().contains("LUN"));
        let get_max_lun = Setup {
            request_type: 0xA1,
            request: 0xFE,
            value: 0,
            index: 0,
            length: 1,
        };
        assert!(
            stick
                .borrow_mut()
                .usb()
                .requests()
                .iter()
                .any(|r| r.setup == get_max_lun)
        );
    }

    #[test]
    fn a_device_that_is_not_a_disk_is_refused_after_inquiry() {
        let (hal, stick, r) = kingston_with(|s| s.set_peripheral_type(5));
        assert_eq!(r.err(), Some(UsbError::Unsupported("not a disk")));
        assert_eq!(stick.borrow().opcodes(), [op::INQUIRY]);
        assert!(
            hal.log_text()
                .contains("storage: slot 1: peripheral type 5 is not a disk")
        );
    }

    #[test]
    fn a_capacity_of_0xffffffff_blocks_asks_read_capacity_16() {
        // Exactly 2^32 blocks: READ(10) still reaches the last one.
        let (_, stick, r) = kingston_with(|s| s.set_capacity(1 << 32, 512));
        assert_eq!(r.unwrap().block_count(), 1 << 32);
        assert_eq!(
            stick.borrow().opcodes()[2..],
            [op::READ_CAPACITY_10, op::SERVICE_ACTION_IN_16]
        );
        let (_, stick, r) = kingston_with(|s| s.set_capacity(0xFFFF_FFFF, 512));
        assert_eq!(r.unwrap().block_count(), 0xFFFF_FFFF);
        assert!(!stick.borrow().opcodes().contains(&op::SERVICE_ACTION_IN_16));
    }

    #[test]
    fn more_than_2_pow_32_blocks_are_refused() {
        let (hal, _, r) = kingston_with(|s| s.set_capacity((1 << 32) + 1, 512));
        assert_eq!(r.err(), Some(UsbError::Unsupported("over 2^32 blocks")));
        assert!(
            hal.log_text()
                .contains("storage: slot 1: last LBA 0x100000000: over 2^32 blocks")
        );
    }

    #[test]
    fn a_disk_of_4096_byte_blocks_starts_and_odd_block_sizes_are_refused() {
        let (_, _, r) = kingston_with(|s| s.set_capacity(1000, 4096));
        let disk = r.unwrap();
        assert_eq!((disk.block_size(), disk.block_count()), (4096, 1000));
        for size in [2048, 1024] {
            assert!(kingston_with(|s| s.set_capacity(1000, size)).2.is_ok());
        }
        for size in [520, 256, 8192, 0] {
            let (hal, _, r) = kingston_with(|s| s.set_capacity(1000, size));
            assert_eq!(r.err(), Some(UsbError::Unsupported("block size")));
            assert!(
                hal.log_text()
                    .contains(&format!("block size {size} not supported"))
            );
        }
    }

    #[test]
    fn a_transport_failure_during_setup_is_followed_by_a_reset_recovery() {
        let (hal, stick, r) = kingston_with(|s| s.phase_error_next());
        assert_eq!(r.err(), Some(UsbError::Protocol("phase error")));
        assert!(
            hal.log_text()
                .contains("storage: slot 1: INQUIRY: protocol error: phase error; reset recovery")
        );
        assert_eq!(
            trace(&stick),
            ["GET_MAX_LUN", "12", "reset", "clear 81", "clear 02"]
        );
    }

    fn storage_interface(endpoints: Vec<Endpoint>) -> Interface {
        Interface {
            number: 0,
            class: 8,
            subclass: 6,
            protocol: 0x50,
            endpoints,
        }
    }

    fn bulk(address: u8) -> Endpoint {
        Endpoint {
            address,
            kind: EndpointKind::Bulk,
            max_packet: 512,
            interval: 0,
            max_burst: 0,
        }
    }

    #[test]
    fn mass_storage_interfaces_need_bot_scsi_and_two_bulk_endpoints() {
        assert!(is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            bulk(0x02)
        ])));
        assert!(is_mass_storage(&storage_interface(vec![
            bulk(0x02),
            bulk(0x83)
        ])));
        assert!(!is_mass_storage(&storage_interface(vec![bulk(0x81)])));
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            bulk(0x82)
        ])));
        let mut interrupt = bulk(0x02);
        interrupt.kind = EndpointKind::Interrupt;
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            interrupt
        ])));
        let mut empty = bulk(0x02);
        empty.max_packet = 0;
        assert!(!is_mass_storage(&storage_interface(vec![
            bulk(0x81),
            empty
        ])));
        for (class, subclass, protocol) in [(8, 6, 0x62), (8, 2, 0x50), (3, 6, 0x50), (8, 6, 0)] {
            let mut i = storage_interface(vec![bulk(0x81), bulk(0x02)]);
            (i.class, i.subclass, i.protocol) = (class, subclass, protocol);
            assert!(!is_mass_storage(&i), "{class}/{subclass}/{protocol}");
        }
    }

    #[test]
    fn a_stick_without_bulk_endpoints_is_refused_by_start() {
        let stick = FakeStorage::kingston();
        let (_hal, mut xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        let mut iface = d.configuration.interfaces[0].clone();
        iface.endpoints.truncate(1);
        assert_eq!(
            MassStorage::start(&mut xhci, d.slot, &iface).err(),
            Some(UsbError::Unsupported("no bulk IN and OUT endpoints"))
        );
        assert!(stick.borrow().events().is_empty());
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/usb/src/storage/mod.rs`**

In `crates/usb/src/storage/mod.rs`, replace:

````rust
pub mod bot;
pub mod scsi;
````

with:

````rust
pub mod bot;
mod disk;
pub mod scsi;
````

- [ ] **Step 3: Write the failing tests for `crates/usb/src/storage/transport.rs`**

Create `crates/usb/src/storage/transport.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured};
    use crate::xhci::Xhci;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn kingston() -> (FakeHal, Xhci<FakeHal>, Transport, Stick) {
        let stick = FakeStorage::kingston();
        let (hal, xhci, d) = configured(FakeConfig::intel(), 13, &stick);
        (hal, xhci, Transport::new(d.slot, 0, 0x81, 0x02), stick)
    }

    const TUR: [u8; 6] = [0; 6];

    fn tags(stick: &Stick) -> Vec<u32> {
        stick.borrow().commands().iter().map(|c| c.tag).collect()
    }

    #[test]
    fn a_command_goes_cbw_data_csw_with_a_new_tag_each_time() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut buf = [0u8; 36];
        let n = t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf));
        assert_eq!(n, Ok(36));
        assert_eq!(&buf[8..16], b"Kingston");
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x12, 0x00]);
        assert_eq!(tags(&stick), [1, 2]);
    }

    #[test]
    fn tags_wrap() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        t.next_tag = u32::MAX;
        for _ in 0..2 {
            assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        }
        assert_eq!(tags(&stick), [u32::MAX, 0]);
    }

    #[test]
    fn a_failed_command_is_followed_by_request_sense_and_fails_with_its_sense() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().not_ready_for(1);
        let not_ready = Sense {
            key: 2,
            asc: 4,
            ascq: 1,
        };
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Sense(not_ready))
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x00, 0x03, 0x00]);
        assert_eq!(stick.borrow().resets(), 0);
    }

    #[test]
    fn a_phase_error_is_followed_by_a_reset_recovery_and_the_device_works_again() {
        let (hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().phase_error_next();
        let mut buf = [0u8; 36];
        assert_eq!(
            t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf)),
            Err(UsbError::Protocol("phase error"))
        );
        let events = stick.borrow().events();
        assert_eq!(
            events[1..],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
        assert!(
            hal.log_text()
                .contains("storage: slot 1: INQUIRY: protocol error: phase error; reset recovery")
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
    }

    #[test]
    fn a_data_phase_over_64_kib_is_refused_before_anything_is_sent() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut big = vec![0u8; MAX_BULK + 512];
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_10(0, 129), Data::In(&mut big)),
            Err(UsbError::Unsupported("data phase over 64 KiB"))
        );
        assert!(stick.borrow().events().is_empty());
        // 64 KiB is fine.
        let mut max = vec![0u8; MAX_BULK];
        let read = scsi::read_10(0, 128);
        assert_eq!(
            t.execute(&mut xhci, &read, Data::In(&mut max)),
            Ok(MAX_BULK)
        );
    }

    #[test]
    fn a_stalled_data_phase_is_followed_by_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        // A failed command: the Kingston stalls its data phase.
        stick.borrow_mut().never_ready();
        let mut buf = [0u8; 8];
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_capacity_10(), Data::In(&mut buf)),
            Err(UsbError::Stall)
        );
        assert_eq!(
            stick.borrow().events()[1..],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
        let mut buf = [0u8; 36];
        let inquiry = scsi::inquiry(36);
        assert_eq!(t.execute(&mut xhci, &inquiry, Data::In(&mut buf)), Ok(36));
    }

    #[test]
    fn a_cbw_the_device_did_not_take_whole_is_a_protocol_error() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        let mut bus = TamperBus::new(&mut xhci);
        bus.short_out = true;
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Protocol("CBW not taken whole"))
        );
        assert_eq!(stick.borrow().resets(), 1);
    }

    #[test]
    fn a_device_that_goes_during_recovery_is_left_alone() {
        let (hal, mut xhci, mut t, stick) = kingston();
        stick.borrow_mut().phase_error_next();
        let mut bus = TamperBus::new(&mut xhci);
        bus.fail_control = Some(UsbError::Disconnected);
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Disconnected)
        );
        // Neither halt was cleared after the reset failed that way.
        assert_eq!(stick.borrow().events().len(), 1);
        assert!(!hal.log_text().contains("failed: device disconnected"));
    }

    #[test]
    fn a_gone_device_is_not_recovered() {
        let (hal, mut xhci, mut t, stick) = kingston();
        hal.fake().unplug(13);
        xhci.detach(t.slot());
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Disconnected)
        );
        assert!(!hal.log_text().contains("reset recovery"));
        assert!(stick.borrow().events().is_empty());
    }
}
````

- [ ] **Step 4: Create `crates/usb/src/testing/bus.rs`**

Create `crates/usb/src/testing/bus.rs`:

````rust
//! A [`Bus`] that changes what the one behind it returns, for faults the
//! fake controller cannot produce (a device that disappears at an exact
//! point, a short write).

use crate::{Bus, Setup, UsbError};
use core::time::Duration;

/// A [`Bus`] in front of another that changes what it returns: faults the
/// fake controller cannot produce.
pub struct TamperBus<'a> {
    pub inner: &'a mut dyn Bus,
    /// Every bulk transfer from this one on (counting from 0) fails with
    /// this error without being sent.
    pub fail_bulk: Option<(usize, UsbError)>,
    /// Every control request fails with this error without being sent.
    pub fail_control: Option<UsbError>,
    /// `bulk_out` reports one byte less than it sent.
    pub short_out: bool,
    /// Bulk transfers so far.
    pub bulk: usize,
}

impl<'a> TamperBus<'a> {
    pub fn new(inner: &'a mut dyn Bus) -> TamperBus<'a> {
        TamperBus {
            inner,
            fail_bulk: None,
            fail_control: None,
            short_out: false,
            bulk: 0,
        }
    }

    fn bulk_fault(&mut self) -> Option<UsbError> {
        self.bulk += 1;
        self.fail_bulk
            .and_then(|(from, e)| (self.bulk > from).then_some(e))
    }
}

impl Bus for TamperBus<'_> {
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
        match self.fail_control {
            Some(e) => Err(e),
            None => self.inner.control(slot, setup, data),
        }
    }

    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
        self.inner.queue_in(slot, endpoint, len)
    }

    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        self.inner.take_in(slot, endpoint, buf)
    }

    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError> {
        match self.bulk_fault() {
            Some(e) => Err(e),
            None => self.inner.bulk_in(slot, endpoint, buf),
        }
    }

    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError> {
        if let Some(e) = self.bulk_fault() {
            return Err(e);
        }
        let n = self.inner.bulk_out(slot, endpoint, data)?;
        Ok(if self.short_out {
            n.saturating_sub(1)
        } else {
            n
        })
    }

    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
        self.inner.clear_halt(slot, endpoint)
    }

    fn now(&self) -> Duration {
        self.inner.now()
    }

    fn sleep(&self, d: Duration) {
        self.inner.sleep(d)
    }

    fn log(&self, args: core::fmt::Arguments) {
        self.inner.log(args)
    }
}
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/device.rs`**

In `crates/usb/src/testing/device.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

    /// A USB 2 stick: high-speed mass storage with bulk endpoints of 512.
````

with:

````rust

    /// QEMU 8.2's usb-storage on a USB 3 port: SuperSpeed, `46f4:0001`, USB
    /// 3.00, EP0 512 bytes (bMaxPacketSize0 9), mass storage (8/6/0x50),
    /// bulk IN 0x81 and OUT 0x02 of 1024 bytes with a burst of 16
    /// (companion bMaxBurst 15).
    pub fn qemu_stick() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 4, 0, //
            6, 0x30, 15, 0, 0, 0, //
            7, 5, 0x02, 2, 0, 4, 0, //
            6, 0x30, 15, 0, 0, 0,
        ];
        FakeUsbDevice::new(
            Speed::Super,
            device_descriptor(0x0300, 9, 0x46F4, 0x0001),
            configuration(1, &body),
        )
    }

    /// A USB 2 stick: high-speed mass storage with bulk endpoints of 512.
````

Replace:

````rust
        self.halted.insert(endpoint);
    }
````

with:

````rust
        self.halted.insert(endpoint);
    }

    /// Whether `endpoint` answers with STALL (until its halt is cleared).
    pub fn is_halted(&self, endpoint: u8) -> bool {
        self.halted.contains(&endpoint)
    }
````

Replace:

````rust
            (FakeUsbDevice::usb2_stick(), 0x0951, 0x1665, 1),
        ] {
````

with:

````rust
            (FakeUsbDevice::usb2_stick(), 0x0951, 0x1665, 1),
            (FakeUsbDevice::qemu_stick(), 0x46F4, 0x0001, 1),
        ] {
````

Replace:

````rust
        assert_eq!(stick.borrow().max_packet0(), 512);
        assert_eq!(
````

with:

````rust
        assert_eq!(stick.borrow().max_packet0(), 512);
        let qemu = FakeUsbDevice::qemu_stick();
        let q = qemu.borrow();
        let desc = DeviceDescriptor::parse(q.device_descriptor()).unwrap();
        assert_eq!((desc.usb_version, q.max_packet0()), (0x0300, 512));
        let c = parse_configuration(q.configuration_descriptor()).unwrap();
        let i = &c.interfaces[0];
        assert_eq!((i.class, i.subclass, i.protocol), (8, 6, 0x50));
        let eps: Vec<_> = i
            .endpoints
            .iter()
            .map(|e| (e.address, e.packet_size(), e.max_burst))
            .collect();
        assert_eq!(eps, [(0x81, 1024, 15), (0x02, 1024, 15)]);
        assert_eq!(
````

Replace:

````rust
        d.stall_endpoint(0x81);
        assert_eq!(d.data_in(0x81, 8), Some(Err(Stall)));
        d.control(Setup::clear_halt(0x81), &[]);
        assert_eq!(d.data_in(0x81, 8), None);
````

with:

````rust
        d.stall_endpoint(0x81);
        assert!(d.is_halted(0x81) && !d.is_halted(0x02));
        assert_eq!(d.data_in(0x81, 8), Some(Err(Stall)));
        d.control(Setup::clear_halt(0x81), &[]);
        assert!(!d.is_halted(0x81));
        assert_eq!(d.data_in(0x81, 8), None);
````

- [ ] **Step 6: Extend the test support in `crates/usb/src/testing/mod.rs`**

In `crates/usb/src/testing/mod.rs`, replace:

````rust
//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! and the fake xHCI controller behind its registers.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod device;
mod hal;
mod xhci;

pub use device::FakeUsbDevice;
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use xhci::{ExtCap, FakeCap, FakeConfig};
````

with:

````rust
//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! the fake xHCI controller behind its registers, and the fake devices on
//! its ports.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod bus;
mod device;
mod hal;
mod storage;
mod xhci;

pub use bus::TamperBus;
pub use device::FakeUsbDevice;
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use storage::{Event, FakeStorage, configured, op};
pub use xhci::{ExtCap, FakeCap, FakeConfig};
````

- [ ] **Step 7: Create `crates/usb/src/testing/storage/bot.rs`**

Create `crates/usb/src/testing/storage/bot.rs`:

````rust
//! The fake stick's Bulk-Only Transport (BOT 5, 6): waiting for a CBW, the
//! data phase, the CSW; the class requests GET_MAX_LUN and Bulk-Only Mass
//! Storage Reset (BOT 3.1, 3.2); and the halts a reset recovery must clear
//! (BOT 5.3.4).

use super::scsi::Answer;
use super::*;
use crate::Speed;
use crate::bus::{CLEAR_FEATURE, ENDPOINT_HALT, RECIPIENT_ENDPOINT};
use crate::testing::device::Stall;

pub(super) enum Phase {
    /// Waiting for a CBW on bulk OUT.
    Cbw,
    /// `data` goes to the host; `left` of the announced bytes have not.
    DataIn {
        tag: u32,
        data: Vec<u8>,
        sent: usize,
        left: u32,
        status: u8,
        stall: bool,
    },
    /// `left` announced bytes still to come.
    DataOut {
        tag: u32,
        left: u32,
        received: Vec<u8>,
        write: Option<u64>,
        status: u8,
        stall: bool,
    },
    /// The CSW waits to be read on bulk IN.
    Csw { tag: u32, residue: u32, status: u8 },
}

impl FakeStorage {
    /// A CBW arrived on bulk OUT: checks it and starts the command.
    fn cbw(&mut self, b: &[u8]) {
        if b.len() != CBW_LEN {
            panic!("fake storage: CBW of {} bytes", b.len());
        }
        let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if le32(0) != CBW_SIGNATURE {
            panic!("fake storage: CBW signature {:#010x}", le32(0));
        }
        if !self.uncleared.is_empty() {
            panic!(
                "fake storage: CBW after a reset before the halts of {:x?} were cleared (BOT 5.3.4)",
                self.uncleared
            );
        }
        let (tag, length, flags, lun, cb_len) = (le32(4), le32(8), b[12], b[13], b[14]);
        if self.last_tag == Some(tag) {
            panic!("fake storage: CBW tag {tag:#x} again");
        }
        if flags & 0x7F != 0 {
            panic!("fake storage: reserved CBW flags {flags:#04x}");
        }
        if lun != 0 {
            panic!("fake storage: CBW for LUN {lun}");
        }
        if cb_len == 0 || cb_len > 16 {
            panic!("fake storage: bCBWCBLength {cb_len}");
        }
        self.last_tag = Some(tag);
        let cdb = &b[15..15 + cb_len as usize];
        self.command(tag, length, flags & 0x80 != 0, cdb);
    }

    /// Enters the data phase of a command that ends with `status`. A
    /// command that did not pass stalls its data phase or pads it
    /// (zeros in, data out discarded), as `failed_data` says.
    pub(super) fn begin(
        &mut self,
        tag: u32,
        length: u32,
        dir_in: bool,
        answer: Answer,
        status: u8,
    ) {
        let stall = status == FAILED && self.failed_data == FailedData::Stall;
        self.phase = if length == 0 {
            Phase::Csw {
                tag,
                residue: 0,
                status,
            }
        } else if dir_in {
            let mut data = match answer {
                Answer::In(data) => data,
                _ if stall => Vec::new(),
                _ => vec![0; length as usize],
            };
            data.truncate(length as usize);
            Phase::DataIn {
                tag,
                data,
                sent: 0,
                left: length,
                status,
                stall,
            }
        } else {
            let write = match answer {
                Answer::Out(lba) if status == PASSED => Some(lba),
                _ => None,
            };
            Phase::DataOut {
                tag,
                left: length,
                received: Vec::new(),
                write,
                status,
                stall,
            }
        };
    }
}

impl FakeDevice for FakeStorage {
    fn speed(&self) -> Speed {
        self.usb.speed()
    }

    fn max_packet0(&self) -> u16 {
        self.usb.max_packet0()
    }

    /// The standard requests go to the `FakeUsbDevice` (with its knobs);
    /// the class requests are GET_MAX_LUN and the Bulk-Only Mass Storage
    /// Reset (BOT 3.1, 3.2), both for interface 0 only.
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>> {
        let answer = self.usb.control(setup, data_out)?;
        if answer.is_err() {
            return Some(answer);
        }
        if setup.request_type & 0x60 == 0x20 {
            if setup.request_type & 0x1F != 1 || setup.index != INTERFACE || setup.value != 0 {
                panic!("fake storage: class request {setup:?} not for interface 0");
            }
            return Some(match (setup.request_type, setup.request, setup.length) {
                (0xA1, GET_MAX_LUN, 1) => {
                    self.events.push(Event::GetMaxLun);
                    self.max_lun.map(|n| vec![n]).ok_or(Stall)
                }
                (0x21, RESET, 0) => {
                    self.phase = Phase::Cbw;
                    self.uncleared = BTreeSet::from([BULK_IN, BULK_OUT]);
                    self.events.push(Event::Reset);
                    Ok(Vec::new())
                }
                _ => panic!("fake storage: class request {setup:?}"),
            });
        }
        if (setup.request_type, setup.request, setup.value)
            == (RECIPIENT_ENDPOINT, CLEAR_FEATURE, ENDPOINT_HALT)
        {
            let endpoint = setup.index as u8;
            self.uncleared.remove(&endpoint);
            self.events.push(Event::ClearHalt(endpoint));
        }
        Some(answer)
    }

    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
        if endpoint != BULK_IN {
            panic!("fake storage: IN transfer on endpoint {endpoint:#04x}");
        }
        if self.usb.is_halted(BULK_IN) {
            return Some(Err(Stall));
        }
        match &mut self.phase {
            Phase::Cbw => panic!("fake storage: IN transfer while waiting for a CBW"),
            Phase::DataOut { .. } => panic!("fake storage: IN transfer during a data-OUT phase"),
            Phase::DataIn {
                tag,
                data,
                sent,
                left,
                status,
                stall,
            } => {
                // More would take the CSW into the data (BOT 6.7.2).
                if max_len > *left as usize {
                    panic!(
                        "fake storage: IN transfer of {max_len} bytes, {left} left of the data phase"
                    );
                }
                if *stall {
                    let (tag, residue, status) = (*tag, *left, *status);
                    self.usb.stall_endpoint(BULK_IN);
                    self.phase = Phase::Csw {
                        tag,
                        residue,
                        status,
                    };
                    return Some(Err(Stall));
                }
                let n = max_len.min(data.len() - *sent);
                let chunk = data[*sent..*sent + n].to_vec();
                *sent += n;
                *left -= n as u32;
                if *sent == data.len() {
                    self.phase = Phase::Csw {
                        tag: *tag,
                        residue: *left,
                        status: *status,
                    };
                }
                Some(Ok(chunk))
            }
            Phase::Csw {
                tag,
                residue,
                status,
            } => {
                if max_len < CSW_LEN {
                    panic!("fake storage: CSW read of {max_len} bytes");
                }
                let mut b = CSW_SIGNATURE.to_le_bytes().to_vec();
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&residue.to_le_bytes());
                b.push(*status);
                self.phase = Phase::Cbw;
                Some(Ok(b))
            }
        }
    }

    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>> {
        if endpoint != BULK_OUT {
            panic!("fake storage: OUT transfer on endpoint {endpoint:#04x}");
        }
        if self.usb.is_halted(BULK_OUT) {
            return Some(Err(Stall));
        }
        match &mut self.phase {
            Phase::Cbw => self.cbw(data),
            Phase::DataIn { .. } => panic!("fake storage: OUT transfer during a data-IN phase"),
            Phase::Csw { .. } => panic!("fake storage: OUT transfer while the CSW is pending"),
            Phase::DataOut {
                tag,
                left,
                received,
                write,
                status,
                stall,
            } => {
                if *stall {
                    let (tag, residue, status) = (*tag, *left, *status);
                    self.usb.stall_endpoint(BULK_OUT);
                    self.phase = Phase::Csw {
                        tag,
                        residue,
                        status,
                    };
                    return Some(Err(Stall));
                }
                if data.len() > *left as usize {
                    panic!(
                        "fake storage: OUT transfer of {} bytes, {left} announced",
                        data.len()
                    );
                }
                received.extend_from_slice(data);
                *left -= data.len() as u32;
                if *left == 0 {
                    let (tag, status, write) = (*tag, *status, *write);
                    let received = std::mem::take(received);
                    if let Some(lba) = write {
                        self.write_blocks(lba, &received);
                    }
                    self.phase = Phase::Csw {
                        tag,
                        residue: 0,
                        status,
                    };
                }
            }
        }
        Some(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_requests_answer_and_a_reset_waits_for_both_halts() {
        let mut d = kingston_device();
        assert!(d.events().is_empty());
        assert_eq!(
            d.control(class_request(0xA1, GET_MAX_LUN, 0, 1), &[]),
            Some(Ok(vec![0]))
        );
        d.set_max_lun(None);
        assert_eq!(
            d.control(class_request(0xA1, GET_MAX_LUN, 0, 1), &[]),
            Some(Err(Stall))
        );
        // Both were recorded, the stalled one too.
        assert_eq!(d.events(), [Event::GetMaxLun, Event::GetMaxLun]);
        // A CBW whose CSW is never read, then a reset.
        send(
            &mut d,
            &cbw_bytes(1, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(
            d.control(class_request(0x21, RESET, 0, 0), &[]),
            Some(Ok(vec![]))
        );
        d.control(Setup::clear_halt(BULK_IN), &[]);
        d.control(Setup::clear_halt(BULK_OUT), &[]);
        send(
            &mut d,
            &cbw_bytes(2, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (2, 0, PASSED));
        assert_eq!(d.resets(), 1);
        assert_eq!(
            d.events()[3..6],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
    }

    #[test]
    fn a_failed_read_stalls_on_the_kingston_and_pads_on_qemu() {
        let mut d = kingston_device();
        d.never_ready();
        send(&mut d, &cbw_bytes(1, 512, true, &read_10_cdb(0, 1)));
        assert_eq!(d.data_in(BULK_IN, 512), Some(Err(Stall)));
        // Halted until the host clears it; then the CSW.
        assert_eq!(d.data_in(BULK_IN, 13), Some(Err(Stall)));
        d.control(Setup::clear_halt(BULK_IN), &[]);
        assert_eq!(read_csw(&mut d), (1, 512, FAILED));
        let q = FakeStorage::qemu(16);
        let mut d = q.borrow_mut();
        d.never_ready();
        send(&mut d, &cbw_bytes(1, 1024, true, &read_10_cdb(2, 2)));
        assert_eq!(receive(&mut d, 1024), [0; 1024]);
        assert_eq!(read_csw(&mut d), (1, 0, FAILED));
    }

    #[test]
    fn what_a_correct_host_never_does_panics() {
        let tur = [TEST_UNIT_READY, 0, 0, 0, 0, 0];
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, false, &tur)[..30]));
        assert!(msg.contains("CBW of 30 bytes"));
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[0] = 0;
            send(d, &b);
        });
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(7, 0, false, &tur));
            read_csw(d);
            send(d, &cbw_bytes(7, 0, false, &tur));
        });
        assert!(msg.contains("tag 0x7 again"));
        let msg = panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[13] = 1;
            send(d, &b);
        });
        assert!(msg.contains("LUN 1"));
        panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[])));
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[14] = 17;
            send(d, &b);
        });
        panics_with(|d| {
            let mut b = cbw_bytes(1, 0, false, &tur);
            b[12] = 0x40;
            send(d, &b);
        });
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[0xA0; 12])));
        assert!(msg.contains("unknown opcode 0xa0"));
        panics_with(|d| send(d, &cbw_bytes(1, 0, false, &[TEST_UNIT_READY; 10])));
    }

    #[test]
    fn transfers_out_of_turn_panic() {
        let tur = [TEST_UNIT_READY, 0, 0, 0, 0, 0];
        let inq = [INQUIRY, 0, 0, 0, 36, 0];
        let msg = panics_with(|d| {
            d.data_in(BULK_IN, 13);
        });
        assert!(msg.contains("while waiting for a CBW"));
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(1, 0, false, &tur));
            send(d, &cbw_bytes(2, 0, false, &tur));
        });
        assert!(msg.contains("while the CSW is pending"));
        panics_with(|d| {
            send(d, &cbw_bytes(1, 36, true, &inq));
            send(d, &cbw_bytes(2, 0, false, &tur));
        });
        let msg = panics_with(|d| {
            send(d, &cbw_bytes(1, 36, true, &inq));
            d.data_in(BULK_IN, 512);
        });
        assert!(msg.contains("IN transfer of 512 bytes, 36 left"));
        panics_with(|d| {
            let mut w = read_10_cdb(0, 1);
            w[0] = WRITE_10;
            send(d, &cbw_bytes(1, 512, false, &w));
            send(d, &[0; 1024]);
        });
        panics_with(|d| {
            send(d, &cbw_bytes(1, 0, false, &tur));
            d.data_in(BULK_IN, 12);
        });
        let msg = panics_with(|d| {
            d.control(class_request(0x21, RESET, 0, 0), &[]);
            d.control(Setup::clear_halt(BULK_IN), &[]);
            send(d, &cbw_bytes(1, 0, false, &tur));
        });
        assert!(msg.contains("before the halts of {2} were cleared"));
        panics_with(|d| {
            d.control(class_request(0xA1, GET_MAX_LUN, 1, 1), &[]);
        });
        panics_with(|d| {
            d.data_in(0x83, 8);
        });
    }
}
````

- [ ] **Step 8: Create `crates/usb/src/testing/storage/mod.rs`**

The fake storage device. Like the fake controller it is as strict as hardware: whatever BOT or SCSI forbids panics with a message starting `fake storage:`.

Create `crates/usb/src/testing/storage/mod.rs`:

````rust
//! A fake USB stick: Bulk-Only Transport (USB Mass Storage Class Bulk-Only
//! Transport 1.0) and the SCSI commands the driver sends (SPC-4, SBC-3),
//! over an in-memory disk. Its descriptors and standard requests are those
//! of a [`FakeUsbDevice`]; `bot` has the transport's state machine, `scsi`
//! the commands.
//!
//! It is as strict as a real device, and where a correct host has no
//! choice, stricter: whatever BOT or SCSI does not let a host do panics
//! with a message starting "fake storage: ". It decodes CBWs and command
//! blocks with its own constants, not the driver's. Its answers copy two
//! real devices: the NUC's Kingston DataTraveler 3.0 and QEMU 8.2's
//! `usb-storage` (with `scsi-disk` behind it).

mod bot;
mod scsi;

pub use scsi::{KINGSTON_BLOCKS, KINGSTON_INQUIRY, op};

use super::device::{FakeDevice, FakeUsbDevice};
use super::{FakeConfig, FakeHal, start};
use crate::xhci::{Device, Xhci};
use crate::{Hal, Setup};
use bot::Phase;
use core::time::Duration;
use scsi::op::*;
use scsi::{NO_SENSE, Sense};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const BULK_IN: u8 = 0x81;
const BULK_OUT: u8 = 0x02;
const INTERFACE: u16 = 0;

/// Class requests (BOT 3.1, 3.2).
const RESET: u8 = 0xFF;
const GET_MAX_LUN: u8 = 0xFE;

const CBW_SIGNATURE: u32 = 0x4342_5355;
const CSW_SIGNATURE: u32 = 0x5342_5355;
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;

/// bCSWStatus.
const PASSED: u8 = 0;
const FAILED: u8 = 1;
const PHASE_ERROR: u8 = 2;

/// A CBW as the device took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub opcode: u8,
    pub tag: u32,
    pub data_length: u32,
    /// READ(10) and WRITE(10): the first block and the number of blocks.
    pub lba: u64,
    pub blocks: u32,
}

/// What the device saw, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// GET_MAX_LUN (answered or stalled).
    GetMaxLun,
    Command(Command),
    /// A Bulk-Only Mass Storage Reset.
    Reset,
    /// CLEAR_FEATURE(ENDPOINT_HALT) of this endpoint.
    ClearHalt(u8),
}

/// What a failed command does with the data phase the host announced
/// (BOT 6.7.2 and 6.7.3 allow both).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedData {
    /// Stall the data endpoint (most sticks).
    Stall,
    /// Move the announced bytes (zeros in, discarded out), as QEMU does.
    Pad,
}

/// A mass-storage device with one LUN.
pub struct FakeStorage {
    usb: FakeUsbDevice,
    inquiry: Vec<u8>,
    blocks: u64,
    block_size: u32,
    /// Blocks written, by LBA; the others read as zeros.
    disk: BTreeMap<u64, Vec<u8>>,
    phase: Phase,
    last_tag: Option<u32>,
    sense: Sense,
    failed_data: FailedData,
    /// Halts the host must clear after a reset before the next CBW
    /// (BOT 5.3.4).
    uncleared: BTreeSet<u8>,
    events: Vec<Event>,
    /// Knob: GET_MAX_LUN's answer; `None` stalls it.
    max_lun: Option<u8>,
    /// Knob: a UNIT ATTENTION is pending (power on, reset).
    unit_attention: bool,
    /// Knob: this many more TEST UNIT READYs fail with NOT READY.
    not_ready: usize,
    /// Knob: NOT READY (becoming ready) for ever.
    never_ready: bool,
    /// Knob: NOT READY, medium not present.
    no_medium: bool,
    /// Knob: the next command ends with a phase error.
    phase_error: bool,
}

impl FakeStorage {
    fn new(
        usb: Rc<RefCell<FakeUsbDevice>>,
        inquiry: Vec<u8>,
        blocks: u64,
        failed_data: FailedData,
    ) -> Rc<RefCell<FakeStorage>> {
        let usb = Rc::try_unwrap(usb)
            .ok()
            .expect("a new device has one owner")
            .into_inner();
        Rc::new(RefCell::new(FakeStorage {
            usb,
            inquiry,
            blocks,
            block_size: 512,
            disk: BTreeMap::new(),
            phase: Phase::Cbw,
            last_tag: None,
            sense: NO_SENSE,
            failed_data,
            uncleared: BTreeSet::new(),
            events: Vec::new(),
            max_lun: Some(0),
            unit_attention: false,
            not_ready: 0,
            never_ready: false,
            no_medium: false,
            phase_error: false,
        }))
    }

    /// The NUC's Kingston DataTraveler 3.0: its descriptors, its INQUIRY
    /// data and its size (30,277,632 blocks of 512 bytes, about 14.4 GiB,
    /// stored sparsely). A failed command stalls its data phase.
    pub fn kingston() -> Rc<RefCell<FakeStorage>> {
        FakeStorage::new(
            FakeUsbDevice::kingston_stick(),
            KINGSTON_INQUIRY.to_vec(),
            KINGSTON_BLOCKS,
            FailedData::Stall,
        )
    }

    /// QEMU 8.2's usb-storage with a disk of `blocks` blocks of 512 bytes
    /// (the e2e image: 524,288). It is ready at once: in the real e2e run
    /// the first TEST UNIT READY passes (no unit attention is pending, as
    /// the driver sends no Bulk-Only reset during setup). A failed command
    /// pads its data phase.
    pub fn qemu(blocks: u64) -> Rc<RefCell<FakeStorage>> {
        FakeStorage::new(
            FakeUsbDevice::qemu_stick(),
            scsi::qemu_inquiry(),
            blocks,
            FailedData::Pad,
        )
    }

    /// The descriptors and standard requests, with their knobs.
    pub fn usb(&mut self) -> &mut FakeUsbDevice {
        &mut self.usb
    }

    /// Knob: the disk has `blocks` blocks of `block_size` bytes (what was
    /// written is forgotten).
    pub fn set_capacity(&mut self, blocks: u64, block_size: u32) {
        (self.blocks, self.block_size) = (blocks, block_size);
        self.disk.clear();
    }

    /// Knob: INQUIRY's peripheral device type (0 is a disk).
    pub fn set_peripheral_type(&mut self, kind: u8) {
        self.inquiry[0] = kind;
    }

    /// Knob: GET_MAX_LUN answers `answer`, or stalls for `None`.
    pub fn set_max_lun(&mut self, answer: Option<u8>) {
        self.max_lun = answer;
    }

    /// Knob: the next `n` TEST UNIT READYs fail with NOT READY, becoming
    /// ready (0x04/0x01), as a stick that is still powering up.
    pub fn not_ready_for(&mut self, n: usize) {
        self.not_ready = n;
    }

    /// Knob: NOT READY (becoming ready) for ever.
    pub fn never_ready(&mut self) {
        self.never_ready = true;
    }

    /// Knob: NOT READY, medium not present (0x3A/0x00): a card reader
    /// without a card.
    pub fn no_medium(&mut self) {
        self.no_medium = true;
    }

    /// Knob: a UNIT ATTENTION, power on or reset (0x29/0x00), is pending:
    /// the next command but INQUIRY and REQUEST SENSE fails with it (a
    /// device that reports its power-on, or one after a reset).
    pub fn unit_attention(&mut self) {
        self.unit_attention = true;
    }

    /// Knob: the next command ends with a phase error (CSW status 2) after
    /// its data phase.
    pub fn phase_error_next(&mut self) {
        self.phase_error = true;
    }

    /// Everything the device saw.
    pub fn events(&self) -> Vec<Event> {
        self.events.clone()
    }

    /// The CBWs the device took.
    pub fn commands(&self) -> Vec<Command> {
        let command = |e: &Event| match e {
            Event::Command(c) => Some(*c),
            _ => None,
        };
        self.events.iter().filter_map(command).collect()
    }

    /// The operation codes of the CBWs the device took.
    pub fn opcodes(&self) -> Vec<u8> {
        self.commands().iter().map(|c| c.opcode).collect()
    }

    /// Bulk-Only Mass Storage Resets so far.
    pub fn resets(&self) -> usize {
        self.events.iter().filter(|e| **e == Event::Reset).count()
    }

    /// `count` blocks of the disk from `lba`.
    pub fn read_blocks(&self, lba: u64, count: u64) -> Vec<u8> {
        let size = self.block_size as usize;
        let mut out = Vec::new();
        for b in lba..lba + count {
            match self.disk.get(&b) {
                Some(block) => out.extend_from_slice(block),
                None => out.resize(out.len() + size, 0),
            }
        }
        out
    }

    /// Stores whole blocks of `data` from `lba`.
    pub fn write_blocks(&mut self, lba: u64, data: &[u8]) {
        let size = self.block_size as usize;
        assert!(
            data.len().is_multiple_of(size),
            "fake storage: part of a block"
        );
        for (i, block) in data.chunks(size).enumerate() {
            self.disk.insert(lba + i as u64, block.to_vec());
        }
    }
}

/// A controller made from `config` with `stick` plugged into `port`,
/// addressed and configured for its interface 0: what `MassStorage::start`
/// gets.
pub fn configured(
    config: FakeConfig,
    port: u8,
    stick: &Rc<RefCell<FakeStorage>>,
) -> (FakeHal, Xhci<FakeHal>, Device) {
    let (hal, mut xhci) = start(config);
    hal.fake().plug(port, stick.clone());
    // A USB 3 link trains for 50 ms before the port shows a connection.
    hal.sleep(Duration::from_millis(60));
    xhci.port_changes();
    let d = xhci
        .attach(port)
        .unwrap_or_else(|e| panic!("the stick was not attached: {e}\n{}", hal.log_text()));
    xhci.configure(&d, &[0]).unwrap();
    (hal, xhci, d)
}

// Helpers for the fake's own tests.

/// A CBW laid out by hand (BOT 5.1).
fn cbw_bytes(tag: u32, length: u32, dir_in: bool, cdb: &[u8]) -> Vec<u8> {
    let mut b = CBW_SIGNATURE.to_le_bytes().to_vec();
    b.extend_from_slice(&tag.to_le_bytes());
    b.extend_from_slice(&length.to_le_bytes());
    b.extend_from_slice(&[if dir_in { 0x80 } else { 0 }, 0, cdb.len() as u8]);
    b.extend_from_slice(cdb);
    b.resize(CBW_LEN, 0);
    b
}

/// Hands `bytes` to bulk OUT, which must take them.
fn send(dev: &mut FakeStorage, bytes: &[u8]) {
    assert_eq!(dev.data_out(BULK_OUT, bytes), Some(Ok(())));
}

/// Takes up to `len` bytes from bulk IN, which must have them.
fn receive(dev: &mut FakeStorage, len: usize) -> Vec<u8> {
    dev.data_in(BULK_IN, len).unwrap().unwrap()
}

/// The CSW's tag, residue and status.
fn read_csw(dev: &mut FakeStorage) -> (u32, u32, u8) {
    let b = receive(dev, CSW_LEN);
    assert_eq!(b.len(), CSW_LEN);
    assert_eq!(b[..4], CSW_SIGNATURE.to_le_bytes());
    let le32 = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    (le32(4), le32(8), b[12])
}

/// READ(10) of `blocks` blocks from `lba`, laid out by hand (SBC-3 §5.8).
fn read_10_cdb(lba: u32, blocks: u16) -> [u8; 10] {
    let [a, b, c, d] = lba.to_be_bytes();
    let [n, m] = blocks.to_be_bytes();
    [READ_10, 0, a, b, c, d, 0, n, m, 0]
}

/// A Kingston stick outside its `Rc`, to drive by hand.
fn kingston_device() -> FakeStorage {
    Rc::try_unwrap(FakeStorage::kingston())
        .ok()
        .unwrap()
        .into_inner()
}

/// A class request with `wValue` 0.
fn class_request(request_type: u8, request: u8, index: u16, length: u16) -> Setup {
    Setup {
        request_type,
        request,
        value: 0,
        index,
        length,
    }
}

/// Runs `f` on a fresh Kingston stick and returns its panic message, which
/// must be the fake's own.
fn panics_with(f: impl FnOnce(&mut FakeStorage)) -> String {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut kingston_device())));
    let e = r.expect_err("the fake did not panic");
    let msg = e
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap();
    assert!(msg.starts_with("fake storage: "), "{msg}");
    msg
}
````

- [ ] **Step 9: Create `crates/usb/src/testing/storage/scsi.rs`**

Create `crates/usb/src/testing/storage/scsi.rs`:

````rust
//! The fake stick's SCSI side (SPC-4, SBC-3): a command block is checked
//! against its CBW, then answered from the in-memory disk and the knobs:
//! INQUIRY, TEST UNIT READY, REQUEST SENSE (fixed format), READ CAPACITY(10)
//! and (16), READ(10), WRITE(10) and SYNCHRONIZE CACHE(10).

use super::*;

/// The operation codes the fake knows (SPC-4, SBC-3), for tests to name.
pub mod op {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const INQUIRY: u8 = 0x12;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const WRITE_10: u8 = 0x2A;
    pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
    pub const SERVICE_ACTION_IN_16: u8 = 0x9E;
}
use op::*;

/// Sense key, ASC and ASCQ.
pub(super) type Sense = (u8, u8, u8);
pub(super) const NO_SENSE: Sense = (0, 0, 0);
const BECOMING_READY: Sense = (0x02, 0x04, 0x01);
const MEDIUM_NOT_PRESENT: Sense = (0x02, 0x3A, 0x00);
const POWER_ON_RESET: Sense = (0x06, 0x29, 0x00);

/// The Kingston DataTraveler 3.0's INQUIRY data, as Linux read it from the
/// NUC's stick (`/sys/block/sda/device/inquiry`).
pub const KINGSTON_INQUIRY: [u8; 62] = [
    0x00, 0x80, 0x06, 0x02, 0x39, 0x00, 0x00, 0x00, //
    b'K', b'i', b'n', b'g', b's', b't', b'o', b'n', //
    b'D', b'a', b't', b'a', b'T', b'r', b'a', b'v', //
    b'e', b'l', b'e', b'r', b' ', b'3', b'.', b'0', //
    b'P', b'M', b'A', b'P', //
    0x50, 0x4d, 0x41, 0x50, 0x31, 0x32, 0x33, 0x34, 0x87, 0x5b, 0x82, 0xc9, 0x22, 0xb4, 0x96, 0x9e,
    0xa5, 0x84, 0xe9, 0x8f, 0xbc, 0xe1, 0x04, 0x60, 0x04, 0xc0,
];

/// The Kingston stick's size: 30,277,632 blocks of 512 bytes.
pub const KINGSTON_BLOCKS: u64 = 30_277_632;

/// QEMU 8.2's scsi-disk INQUIRY data: a disk, not removable, SPC-3, with
/// QEMU's names and version.
pub(super) fn qemu_inquiry() -> Vec<u8> {
    let mut b = vec![0x00, 0x00, 0x05, 0x12, 31, 0x00, 0x00, 0x10];
    b.extend_from_slice(b"QEMU    QEMU HARDDISK   2.5+");
    b
}

/// What a command answers: its data phase.
pub(super) enum Answer {
    None,
    In(Vec<u8>),
    /// WRITE(10) data to store at this LBA.
    Out(u64),
}

impl FakeStorage {
    /// Checks the command block against the CBW and runs it.
    pub(super) fn command(&mut self, tag: u32, length: u32, dir_in: bool, cdb: &[u8]) {
        let opcode = cdb[0];
        let size = match opcode {
            TEST_UNIT_READY | REQUEST_SENSE | INQUIRY => 6,
            READ_CAPACITY_10 | READ_10 | WRITE_10 | SYNCHRONIZE_CACHE_10 => 10,
            SERVICE_ACTION_IN_16 => 16,
            _ => panic!("fake storage: unknown opcode {opcode:#04x}"),
        };
        if cdb.len() != size {
            panic!(
                "fake storage: {opcode:#04x} with a {}-byte command block",
                cdb.len()
            );
        }
        let be16 = |i: usize| u16::from_be_bytes([cdb[i], cdb[i + 1]]) as u32;
        let be32 = |i: usize| u32::from_be_bytes([cdb[i], cdb[i + 1], cdb[i + 2], cdb[i + 3]]);
        let (lba, blocks) = match opcode {
            READ_10 | WRITE_10 | SYNCHRONIZE_CACHE_10 => (be32(2) as u64, be16(7)),
            _ => (0, 0),
        };
        // The data phase the command needs: bytes and direction.
        let (want, want_in) = match opcode {
            REQUEST_SENSE => (cdb[4] as u64, true),
            INQUIRY => {
                if cdb[1] & 1 != 0 || cdb[2] != 0 {
                    panic!("fake storage: INQUIRY of a VPD page is not modelled");
                }
                (be16(3) as u64, true)
            }
            READ_CAPACITY_10 => (8, true),
            SERVICE_ACTION_IN_16 => {
                if cdb[1] & 0x1F != 0x10 {
                    panic!("fake storage: service action {:#04x}", cdb[1] & 0x1F);
                }
                (be32(10) as u64, true)
            }
            READ_10 | WRITE_10 => {
                if blocks == 0 {
                    panic!("fake storage: {opcode:#04x} of 0 blocks");
                }
                (blocks as u64 * self.block_size as u64, opcode == READ_10)
            }
            _ => (0, false),
        };
        if lba + blocks as u64 > self.blocks
            || (opcode == SYNCHRONIZE_CACHE_10 && lba >= self.blocks)
        {
            panic!(
                "fake storage: {opcode:#04x} of {blocks} blocks at LBA {lba} past the end ({} blocks)",
                self.blocks
            );
        }
        if length as u64 != want || (want > 0 && dir_in != want_in) {
            panic!(
                "fake storage: CBW for {opcode:#04x} announces {length} bytes {}, the command moves {want} {}",
                if dir_in { "in" } else { "out" },
                if want_in { "in" } else { "out" }
            );
        }
        self.events.push(Event::Command(Command {
            opcode,
            tag,
            data_length: length,
            lba,
            blocks,
        }));
        if std::mem::take(&mut self.phase_error) {
            return self.begin(tag, length, dir_in, Answer::None, PHASE_ERROR);
        }
        match self.answer(opcode, lba, blocks) {
            Ok(answer) => {
                if opcode != REQUEST_SENSE {
                    self.sense = NO_SENSE;
                }
                self.begin(tag, length, dir_in, answer, PASSED);
            }
            Err(sense) => {
                self.sense = sense;
                self.begin(tag, length, dir_in, Answer::None, FAILED);
            }
        }
    }

    /// What the command answers, or the sense it fails with.
    fn answer(&mut self, opcode: u8, lba: u64, blocks: u32) -> Result<Answer, Sense> {
        match opcode {
            // Neither reports a unit attention (SPC-4 §5.14); REQUEST
            // SENSE hands it out and clears it.
            REQUEST_SENSE => {
                let (key, asc, ascq) = if std::mem::take(&mut self.unit_attention) {
                    POWER_ON_RESET
                } else {
                    std::mem::replace(&mut self.sense, NO_SENSE)
                };
                let mut b = vec![0u8; 18];
                (b[0], b[2], b[7], b[12], b[13]) = (0x70, key, 10, asc, ascq);
                return Ok(Answer::In(b));
            }
            INQUIRY => return Ok(Answer::In(self.inquiry.clone())),
            _ => {}
        }
        if std::mem::take(&mut self.unit_attention) {
            return Err(POWER_ON_RESET);
        }
        if self.no_medium {
            return Err(MEDIUM_NOT_PRESENT);
        }
        if self.never_ready || self.not_ready > 0 {
            if opcode == TEST_UNIT_READY {
                self.not_ready = self.not_ready.saturating_sub(1);
            }
            return Err(BECOMING_READY);
        }
        let last = self.blocks - 1;
        Ok(match opcode {
            READ_CAPACITY_10 => {
                let mut b = (last.min(0xFFFF_FFFF) as u32).to_be_bytes().to_vec();
                b.extend_from_slice(&self.block_size.to_be_bytes());
                Answer::In(b)
            }
            SERVICE_ACTION_IN_16 => {
                let mut b = last.to_be_bytes().to_vec();
                b.extend_from_slice(&self.block_size.to_be_bytes());
                b.resize(32, 0);
                Answer::In(b)
            }
            READ_10 => Answer::In(self.read_blocks(lba, blocks as u64)),
            WRITE_10 => Answer::Out(lba),
            _ => Answer::None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inquiry_goes_cbw_data_csw_and_is_truncated_to_the_allocation() {
        let mut d = kingston_device();
        send(&mut d, &cbw_bytes(1, 36, true, &[INQUIRY, 0, 0, 0, 36, 0]));
        assert_eq!(receive(&mut d, 36), KINGSTON_INQUIRY[..36]);
        assert_eq!(read_csw(&mut d), (1, 0, PASSED));
        // More asked than the device has: a short data phase, the rest is
        // the residue.
        send(
            &mut d,
            &cbw_bytes(2, 100, true, &[INQUIRY, 0, 0, 0, 100, 0]),
        );
        assert_eq!(receive(&mut d, 100).len(), 62);
        assert_eq!(read_csw(&mut d), (2, 38, PASSED));
        assert_eq!(d.opcodes(), [INQUIRY, INQUIRY]);
    }

    #[test]
    fn capacity_is_capped_at_0xffffffff_for_read_capacity_10() {
        let mut d = kingston_device();
        send(
            &mut d,
            &cbw_bytes(1, 8, true, &[READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
        assert_eq!(receive(&mut d, 8), [0x01, 0xCD, 0xFF, 0xFF, 0, 0, 2, 0]);
        read_csw(&mut d);
        d.set_capacity(1 << 33, 4096);
        send(
            &mut d,
            &cbw_bytes(2, 8, true, &[READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
        assert_eq!(receive(&mut d, 8), [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]);
        read_csw(&mut d);
        let mut rc16 = [0u8; 16];
        (rc16[0], rc16[1], rc16[13]) = (SERVICE_ACTION_IN_16, 0x10, 32);
        send(&mut d, &cbw_bytes(3, 32, true, &rc16));
        let b = receive(&mut d, 32);
        assert_eq!(b[..12], [0, 0, 0, 1, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x10, 0]);
        assert_eq!(read_csw(&mut d), (3, 0, PASSED));
    }

    #[test]
    fn a_unit_attention_fails_the_next_command_and_request_sense_clears_it() {
        // QEMU's TEST UNIT READY passes at once...
        let q = FakeStorage::qemu(2048);
        let mut d = q.borrow_mut();
        send(
            &mut d,
            &cbw_bytes(9, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (9, 0, PASSED));
        // ...unless a unit attention is pending. INQUIRY does not report it.
        d.unit_attention();
        send(&mut d, &cbw_bytes(1, 36, true, &[INQUIRY, 0, 0, 0, 36, 0]));
        assert_eq!(&receive(&mut d, 36)[8..], b"QEMU    QEMU HARDDISK   2.5+");
        read_csw(&mut d);
        send(
            &mut d,
            &cbw_bytes(2, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (2, 0, FAILED));
        send(
            &mut d,
            &cbw_bytes(3, 18, true, &[REQUEST_SENSE, 0, 0, 0, 18, 0]),
        );
        let sense = receive(&mut d, 18);
        assert_eq!(
            (sense[0], sense[2], sense[12], sense[13]),
            (0x70, 6, 0x29, 0)
        );
        read_csw(&mut d);
        send(
            &mut d,
            &cbw_bytes(4, 0, false, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
        );
        assert_eq!(read_csw(&mut d), (4, 0, PASSED));
    }

    #[test]
    fn writes_are_stored_and_read_back() {
        let mut d = kingston_device();
        d.set_capacity(64, 512);
        let data: Vec<u8> = (0..1024).map(|i| i as u8).collect();
        let mut w = read_10_cdb(62, 2);
        w[0] = WRITE_10;
        send(&mut d, &cbw_bytes(1, 1024, false, &w));
        // In two transfers.
        send(&mut d, &data[..512]);
        send(&mut d, &data[512..]);
        assert_eq!(read_csw(&mut d), (1, 0, PASSED));
        send(&mut d, &cbw_bytes(2, 1024, true, &read_10_cdb(62, 2)));
        assert_eq!(receive(&mut d, 1024), data);
        read_csw(&mut d);
        assert_eq!(d.read_blocks(62, 2), data);
        let c = d.commands();
        assert_eq!((c[0].opcode, c[0].lba, c[0].blocks), (WRITE_10, 62, 2));
    }

    #[test]
    fn data_phases_that_do_not_match_the_command_panic() {
        // Lengths and directions: as allocated, or blocks × block size.
        panics_with(|d| send(d, &cbw_bytes(1, 35, true, &[INQUIRY, 0, 0, 0, 36, 0])));
        panics_with(|d| send(d, &cbw_bytes(1, 36, false, &[INQUIRY, 0, 0, 0, 36, 0])));
        panics_with(|d| send(d, &cbw_bytes(1, 512, false, &read_10_cdb(0, 1))));
        panics_with(|d| send(d, &cbw_bytes(1, 1024, true, &read_10_cdb(0, 1))));
        panics_with(|d| {
            send(
                d,
                &cbw_bytes(1, 512, true, &[TEST_UNIT_READY, 0, 0, 0, 0, 0]),
            )
        });
        let msg = panics_with(|d| send(d, &cbw_bytes(1, 0, true, &read_10_cdb(0, 0))));
        assert!(msg.contains("of 0 blocks"));
        let msg = panics_with(|d| {
            d.set_capacity(100, 512);
            send(d, &cbw_bytes(1, 1024, true, &read_10_cdb(99, 2)));
        });
        assert!(msg.contains("past the end"));
        panics_with(|d| {
            d.set_capacity(100, 512);
            send(
                d,
                &cbw_bytes(
                    1,
                    0,
                    false,
                    &[SYNCHRONIZE_CACHE_10, 0, 0, 0, 0, 100, 0, 0, 0, 0],
                ),
            );
        });
    }
}
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `MassStorage` in this scope ``; `` cannot find type `UsbError` in this scope ``.

- [ ] **Step 11: Implement `crates/usb/src/storage/disk.rs`**

Insert this at the top of `crates/usb/src/storage/disk.rs`, above `#[cfg(test)]`:

````rust
//! One disk behind a mass-storage interface, LUN 0 (spec §6.4): claiming
//! the interface and setting the disk up. Setup is GET_MAX_LUN (BOT 3.2),
//! INQUIRY (SPC-4 §6.4), TEST UNIT READY until the stick is ready (SPC-4
//! §6.37), and READ CAPACITY(10), or (16) for a big disk (SBC-3 §5.15,
//! §5.16).

use super::scsi::{self, Capacity, Inquiry, NOT_READY};
use super::transport::{Data, Transport, is_fatal};
use crate::UsbError;
use crate::bus::{Bus, DIR_IN, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use crate::descriptor::{EndpointKind, Interface};
use alloc::string::String;
use core::time::Duration;

/// How long setup waits for TEST UNIT READY to pass (spec §6.4).
pub const READY_TIMEOUT: Duration = Duration::from_secs(5);
/// The pause between two TEST UNIT READYs.
pub const READY_POLL: Duration = Duration::from_millis(100);
/// GET_MAX_LUN (BOT 3.2).
const GET_MAX_LUN: u8 = 0xFE;
/// ASC "medium not present" (SPC-4 annex D).
const MEDIUM_NOT_PRESENT: u8 = 0x3A;

/// The first bulk IN and bulk OUT endpoints of `iface`.
fn bulk_endpoints(iface: &Interface) -> Option<(u8, u8)> {
    let bulk = |is_in: bool| {
        iface
            .endpoints
            .iter()
            .find(|e| e.kind == EndpointKind::Bulk && e.is_in() == is_in && e.packet_size() > 0)
            .map(|e| e.address)
    };
    Some((bulk(true)?, bulk(false)?))
}

/// Class 8 (mass storage), subclass 6 (SCSI transparent), protocol 0x50
/// (Bulk-Only), with a bulk IN and a bulk OUT endpoint.
pub fn is_mass_storage(iface: &Interface) -> bool {
    (iface.class, iface.subclass, iface.protocol) == (8, 6, 0x50) && bulk_endpoints(iface).is_some()
}

/// One disk behind a mass-storage interface (LUN 0).
pub struct MassStorage {
    transport: Transport,
    block_size: usize,
    block_count: u64,
    vendor: String,
    product: String,
}

impl MassStorage {
    /// Setup (spec §6.4): GET_MAX_LUN (a STALL means 0; only LUN 0 is
    /// used), INQUIRY (logged as vendor, product, revision; a peripheral
    /// type other than 0 is `Unsupported("not a disk")`), TEST UNIT READY
    /// retried for up to 5 s with REQUEST SENSE after each failure and 100
    /// ms between tries, READ CAPACITY(10), then (16) if the answer is
    /// 0xFFFFFFFF. A block size other than 512, 1024, 2048 or 4096 is
    /// `Unsupported("block size")`; more than 2^32 blocks is
    /// `Unsupported("over 2^32 blocks")` (READ(10) cannot reach further).
    pub fn start(bus: &mut dyn Bus, slot: u8, iface: &Interface) -> Result<MassStorage, UsbError> {
        let Some((bulk_in, bulk_out)) = bulk_endpoints(iface) else {
            return Err(UsbError::Unsupported("no bulk IN and OUT endpoints"));
        };
        let mut t = Transport::new(slot, iface.number, bulk_in, bulk_out);
        max_lun(bus, slot, iface.number)?;
        let inquiry = inquiry(bus, &mut t)?;
        wait_until_ready(bus, &mut t)?;
        let (block_size, block_count) = capacity(bus, &mut t)?;
        slog!(bus, slot, "{block_count} blocks of {block_size} bytes");
        Ok(MassStorage {
            transport: t,
            block_size,
            block_count,
            vendor: inquiry.vendor,
            product: inquiry.product,
        })
    }

    pub fn slot(&self) -> u8 {
        self.transport.slot()
    }

    /// Bytes in a block: 512, 1024, 2048 or 4096.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn block_count(&self) -> u64 {
        self.block_count
    }

    /// INQUIRY's vendor identification, without padding.
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// INQUIRY's product identification, without padding.
    pub fn product(&self) -> &str {
        &self.product
    }
}

/// GET_MAX_LUN (BOT 3.2): only LUN 0 is used, so the answer is only
/// logged. Many single-LUN devices stall it; any failure but a gone device
/// means one LUN, as in Linux.
fn max_lun(bus: &mut dyn Bus, slot: u8, interface: u8) -> Result<(), UsbError> {
    let setup = Setup {
        request_type: DIR_IN | TYPE_CLASS | RECIPIENT_INTERFACE,
        request: GET_MAX_LUN,
        value: 0,
        index: interface as u16,
        length: 1,
    };
    let mut lun = [0u8];
    match bus.control(slot, setup, &mut lun) {
        Ok(1) if lun[0] > 0 => slog!(bus, slot, "{} LUNs, only LUN 0 is used", lun[0] as u32 + 1),
        Ok(_) => {}
        Err(UsbError::Stall) => slog!(bus, slot, "GET_MAX_LUN stalled: one LUN"),
        Err(e) if is_fatal(e) => return Err(e),
        Err(e) => slog!(bus, slot, "GET_MAX_LUN failed ({e}): one LUN"),
    }
    Ok(())
}

/// Runs a command that reads into `buf` and returns what came.
fn read_data<'a>(
    bus: &mut dyn Bus,
    t: &mut Transport,
    cdb: &[u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], UsbError> {
    let n = t.execute(bus, cdb, Data::In(buf))?;
    Ok(&buf[..n.min(buf.len())])
}

/// INQUIRY: the names, and whether it is a disk at all.
fn inquiry(bus: &mut dyn Bus, t: &mut Transport) -> Result<Inquiry, UsbError> {
    let slot = t.slot();
    let mut buf = [0u8; scsi::INQUIRY_LEN];
    let cdb = scsi::inquiry(scsi::INQUIRY_LEN as u16);
    let parsed = read_data(bus, t, &cdb, &mut buf).and_then(Inquiry::parse);
    let i = parsed.inspect_err(|e| slog!(bus, slot, "INQUIRY: {e}"))?;
    slog!(
        bus,
        slot,
        "vendor \"{}\", product \"{}\", revision \"{}\"{}",
        i.vendor,
        i.product,
        i.revision,
        if i.removable { ", removable" } else { "" }
    );
    if i.peripheral_type != 0 {
        slog!(
            bus,
            slot,
            "peripheral type {} is not a disk",
            i.peripheral_type
        );
        return Err(UsbError::Unsupported("not a disk"));
    }
    Ok(i)
}

/// TEST UNIT READY until it passes, for up to [`READY_TIMEOUT`]: a stick
/// may take a while after power-on, and the first command may meet a unit
/// attention (power on, reset), which REQUEST SENSE (in `execute`) clears.
/// A sense seen again is not logged again. No medium does not wait.
fn wait_until_ready(bus: &mut dyn Bus, t: &mut Transport) -> Result<(), UsbError> {
    let slot = t.slot();
    let start = bus.now();
    let mut last = None;
    let mut tries = 0u32;
    loop {
        tries = tries.saturating_add(1);
        let e = match t.execute(bus, &scsi::test_unit_ready(), Data::None) {
            Ok(_) => {
                if tries > 1 {
                    slog!(bus, slot, "ready after {tries} tries");
                }
                return Ok(());
            }
            Err(e) if is_fatal(e) => return Err(e),
            Err(e) => e,
        };
        if last != Some(e) {
            slog!(bus, slot, "TEST UNIT READY: {e}");
        }
        if let UsbError::Sense(s) = e
            && (s.key, s.asc) == (NOT_READY, MEDIUM_NOT_PRESENT)
        {
            slog!(bus, slot, "no medium");
            return Err(e);
        }
        if bus.now().saturating_sub(start) >= READY_TIMEOUT {
            slog!(bus, slot, "not ready after 5 s: {e}");
            return Err(e);
        }
        last = Some(e);
        bus.sleep(READY_POLL);
    }
}

/// READ CAPACITY(10), then (16) if the disk is too big for it: the block
/// size and the number of blocks.
fn capacity(bus: &mut dyn Bus, t: &mut Transport) -> Result<(usize, u64), UsbError> {
    let slot = t.slot();
    let mut buf = [0u8; scsi::CAPACITY_16_LEN];
    let cdb = scsi::read_capacity_10();
    let r = read_data(bus, t, &cdb, &mut buf[..scsi::CAPACITY_10_LEN]).and_then(Capacity::parse_10);
    let mut cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(10): {e}"))?;
    if cap.last_lba == 0xFFFF_FFFF {
        let cdb = scsi::read_capacity_16(scsi::CAPACITY_16_LEN as u32);
        let r = read_data(bus, t, &cdb, &mut buf).and_then(Capacity::parse_16);
        cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(16): {e}"))?;
    }
    if !matches!(cap.block_size, 512 | 1024 | 2048 | 4096) {
        slog!(bus, slot, "block size {} not supported", cap.block_size);
        return Err(UsbError::Unsupported("block size"));
    }
    // READ(10) addresses blocks with 32 bits: the last LBA must fit.
    if cap.last_lba > 0xFFFF_FFFF {
        slog!(bus, slot, "last LBA {:#x}: over 2^32 blocks", cap.last_lba);
        return Err(UsbError::Unsupported("over 2^32 blocks"));
    }
    Ok((cap.block_size as usize, cap.last_lba + 1))
}

````

- [ ] **Step 12: Change `crates/usb/src/storage/mod.rs`**

Replace the whole of `crates/usb/src/storage/mod.rs` with:

````rust
//! The USB mass-storage class driver (spec §6.4): Bulk-Only Transport
//! (USB Mass Storage Class Bulk-Only Transport 1.0) carrying SCSI commands
//! (SPC-4, SBC-3) to LUN 0 of a USB stick.
//!
//! Every log line starts "storage: slot N: " (spec §13: the NUC is debugged
//! from a photo of `dmesg`): each setup step and each failure, with its
//! sense, but not a read or write that went well.

/// Logs one line starting "storage: slot N: ".
macro_rules! slog {
    ($bus:expr, $slot:expr, $($arg:tt)*) => {
        $bus.log(format_args!("storage: slot {}: {}", $slot, format_args!($($arg)*)))
    };
}

pub mod bot;
mod disk;
pub mod scsi;
mod transport;

pub use disk::{MassStorage, is_mass_storage};
pub use scsi::Sense;
````

- [ ] **Step 13: Implement `crates/usb/src/storage/transport.rs`**

Insert this at the top of `crates/usb/src/storage/transport.rs`, above `#[cfg(test)]`:

````rust
//! One SCSI command over Bulk-Only Transport (BOT 5 and 6): the CBW on bulk
//! OUT, the data phase, then the CSW on bulk IN. A command that failed
//! (CSW status 1) is followed by REQUEST SENSE, and its sense becomes the
//! error. A phase error, an invalid CSW or a transfer that fails is followed
//! by a reset recovery (BOT 5.3.4), after which the device waits for a CBW
//! again.

use super::bot::{CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};
use super::scsi::{self, SENSE_LEN, Sense};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};

/// Bulk-Only Mass Storage Reset (BOT 3.1).
const MASS_STORAGE_RESET: u8 = 0xFF;

/// A command's data phase.
pub enum Data<'a> {
    None,
    /// Into this buffer; the command expects its length.
    In(&'a mut [u8]),
}

impl Data<'_> {
    fn len(&self) -> usize {
        match self {
            Data::None => 0,
            Data::In(b) => b.len(),
        }
    }
}

/// The device or the controller is gone: nothing more is tried.
pub fn is_fatal(e: UsbError) -> bool {
    matches!(e, UsbError::Disconnected | UsbError::ControllerDead)
}

/// The bulk pipes of one mass-storage interface and the next CBW tag.
pub struct Transport {
    slot: u8,
    interface: u8,
    bulk_in: u8,
    bulk_out: u8,
    next_tag: u32,
}

impl Transport {
    pub fn new(slot: u8, interface: u8, bulk_in: u8, bulk_out: u8) -> Transport {
        Transport {
            slot,
            interface,
            bulk_in,
            bulk_out,
            next_tag: 1,
        }
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// Runs `cdb` once with its data phase and returns the bytes the data
    /// phase moved. A failed command's error is its sense; any other
    /// failure is followed by a reset recovery. A data phase over
    /// [`MAX_BULK`] is `Unsupported`, and nothing is sent.
    pub fn execute(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
    ) -> Result<usize, UsbError> {
        if data.len() > MAX_BULK {
            return Err(UsbError::Unsupported("data phase over 64 KiB"));
        }
        let name = scsi::command_name(cdb.first().copied().unwrap_or(0xFF));
        match self.exchange(bus, cdb, data) {
            Ok((moved, CswStatus::Passed)) => Ok(moved),
            Ok((_, CswStatus::Failed)) => Err(self.request_sense(bus)),
            Ok((_, CswStatus::PhaseError)) => {
                Err(self.recover(bus, name, UsbError::Protocol("phase error")))
            }
            Err(e) => Err(self.recover(bus, name, e)),
        }
    }

    /// CBW, data phase, CSW: the bytes moved and the CSW's status.
    fn exchange(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
    ) -> Result<(usize, CswStatus), UsbError> {
        let tag = self.next_tag;
        self.next_tag = tag.wrapping_add(1);
        // At most MAX_BULK (checked by `execute`).
        let length = data.len() as u32;
        let cbw = Cbw {
            tag,
            data_length: length,
            dir_in: matches!(data, Data::In(_)),
            cdb,
        };
        if bus.bulk_out(self.slot, self.bulk_out, &cbw.to_bytes())? != CBW_LEN {
            return Err(UsbError::Protocol("CBW not taken whole"));
        }
        let moved = match data {
            Data::None => 0,
            Data::In(buf) => bus.bulk_in(self.slot, self.bulk_in, buf)?,
        };
        let mut raw = [0u8; CSW_LEN];
        let n = bus.bulk_in(self.slot, self.bulk_in, &mut raw)?;
        let csw = Csw::parse(&raw[..n.min(CSW_LEN)], tag, length)?;
        Ok((moved, csw.status))
    }

    /// REQUEST SENSE after a failed command (SPC-4 §5.11): the error the
    /// command fails with.
    fn request_sense(&mut self, bus: &mut dyn Bus) -> UsbError {
        let name = scsi::command_name(scsi::REQUEST_SENSE);
        let mut buf = [0u8; SENSE_LEN];
        let cdb = scsi::request_sense(SENSE_LEN as u8);
        match self.exchange(bus, &cdb, Data::In(&mut buf)) {
            Ok((n, CswStatus::Passed)) => match Sense::parse(&buf[..n.min(SENSE_LEN)]) {
                Ok(sense) => UsbError::Sense(sense),
                Err(e) => e,
            },
            Ok((_, CswStatus::Failed)) => UsbError::Protocol("REQUEST SENSE failed"),
            Ok((_, CswStatus::PhaseError)) => {
                self.recover(bus, name, UsbError::Protocol("phase error"))
            }
            Err(e) => self.recover(bus, name, e),
        }
    }

    /// Reset recovery after `cause` in command `name` (BOT 5.3.4):
    /// Bulk-Only Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT) of
    /// bulk IN and bulk OUT. Returns `cause`, or the error that showed the
    /// device or controller gone (then nothing more is sent).
    fn recover(&mut self, bus: &mut dyn Bus, name: &str, cause: UsbError) -> UsbError {
        if is_fatal(cause) {
            return cause;
        }
        slog!(bus, self.slot, "{name}: {cause}; reset recovery");
        let reset = Setup {
            request_type: TYPE_CLASS | RECIPIENT_INTERFACE,
            request: MASS_STORAGE_RESET,
            value: 0,
            index: self.interface as u16,
            length: 0,
        };
        // The reset, then each halt (the endpoint to clear).
        let steps = [
            ("mass storage reset", None),
            ("clearing the bulk IN halt", Some(self.bulk_in)),
            ("clearing the bulk OUT halt", Some(self.bulk_out)),
        ];
        for (what, endpoint) in steps {
            let done = match endpoint {
                None => bus.control(self.slot, reset, &mut []).map(|_| ()),
                Some(ep) => bus.clear_halt(self.slot, ep),
            };
            match done {
                Ok(()) => {}
                Err(e) if is_fatal(e) => return e,
                Err(e) => slog!(bus, self.slot, "{what} failed: {e}"),
            }
        }
        cause
    }
}

````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 285 tests.

- [ ] **Step 15: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 16: Commit**

````bash
git add crates
git commit -m "usb: the mass storage driver finds its disk"
````


### Task 6: Reads, writes and error recovery

`MassStorage::read` and `write` send READ(10) and WRITE(10) of at most 64 KiB each; `flush` sends SYNCHRONIZE CACHE(10), and a stick that does not support it (ILLEGAL REQUEST, as many cheap sticks answer) has no cache to flush, which counts as success and is not sent again. A request must be whole blocks inside the disk, or nothing is sent. The transport gets the whole of spec §6.4's recovery (BOT 5.3.4 and 6.7): a STALL in the data phase clears that endpoint's halt and reads the CSW; a stalled CSW is read once more after clearing the halt; CSW status 1 means REQUEST SENSE, with the sense logged and returned; a phase error, an invalid CSW, or a transfer that times out or fails otherwise means reset recovery (Bulk-Only Mass Storage Reset, then clearing both halts). For READ and WRITE a short data phase or a residue is an error, not short data. Each command is tried three times at most; ILLEGAL REQUEST is not retried, and a device or controller that is gone (`Disconnected`, `ControllerDead`) ends the request at once. The xHCI side reports a bulk transfer on a port that no longer shows a connection as `Disconnected`, so an unplug mid-request is not retried.

**Files:**
- Modify: `crates/usb/src/storage/disk.rs`
- Create: `crates/usb/src/storage/io.rs`
- Modify: `crates/usb/src/storage/mod.rs`
- Modify: `crates/usb/src/storage/transport.rs`
- Modify: `crates/usb/src/testing/mod.rs`
- Modify: `crates/usb/src/testing/storage/bot.rs`
- Modify: `crates/usb/src/testing/storage/mod.rs`
- Modify: `crates/usb/src/testing/storage/scsi.rs`
- Modify: `crates/usb/src/xhci/device.rs`
- Modify: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces: `MassStorage::{read(&mut self, bus: &mut dyn Bus, lba: u64, buf: &mut [u8]) -> Result<(), UsbError>, write(&mut self, bus, lba, buf: &[u8]) -> Result<(), UsbError>, flush(&mut self, bus) -> Result<(), UsbError>}`; bulk transfers on a disconnected port fail with `UsbError::Disconnected`.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/storage/disk.rs`**

In `crates/usb/src/storage/disk.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn a_transport_failure_during_setup_is_followed_by_a_reset_recovery() {
        let (hal, stick, r) = kingston_with(|s| s.phase_error_next());
        assert_eq!(r.err(), Some(UsbError::Protocol("phase error")));
        assert!(
````

with:

````rust
    #[test]
    fn a_setup_command_that_fails_is_tried_again() {
        let (hal, stick, r) = kingston_with(|s| s.phase_error_next());
        assert!(r.is_ok());
        assert!(
````

Replace:

````rust
            trace(&stick),
            ["GET_MAX_LUN", "12", "reset", "clear 81", "clear 02"]
        );
    }
````

with:

````rust
            trace(&stick),
            [
                "GET_MAX_LUN",
                "12",
                "reset",
                "clear 81",
                "clear 02",
                "12",
                "00",
                "25"
            ]
        );
        // READ CAPACITY too.
        let (_hal, stick, r) = kingston_with(|s| {
            s.stall_csw_reads(2);
            s.set_capacity(1000, 512);
        });
        assert_eq!(r.map(|d| d.block_count()), Ok(1000));
        assert_eq!(stick.borrow().resets(), 1);
    }
````

- [ ] **Step 2: Write the failing tests for `crates/usb/src/storage/io.rs`**

Create `crates/usb/src/storage/io.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Sense;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, configured, op};
    use crate::xhci::Xhci;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Stick = Rc<RefCell<FakeStorage>>;

    fn started(stick: &Stick) -> (FakeHal, Xhci<FakeHal>, MassStorage) {
        let (hal, mut xhci, d) = configured(FakeConfig::intel(), 13, stick);
        let disk = MassStorage::start(&mut xhci, d.slot, &d.configuration.interfaces[0]).unwrap();
        (hal, xhci, disk)
    }

    fn kingston() -> (FakeHal, Xhci<FakeHal>, MassStorage, Stick) {
        let stick = FakeStorage::kingston();
        let (hal, xhci, disk) = started(&stick);
        (hal, xhci, disk, stick)
    }

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 512) as u8 ^ seed).collect()
    }

    /// The READ(10) or WRITE(10) commands sent after setup: LBA, blocks
    /// and bytes.
    fn io(stick: &Stick, opcode: u8) -> Vec<(u64, u32, u32)> {
        let c = stick.borrow().commands();
        c.iter()
            .filter(|c| c.opcode == opcode)
            .map(|c| (c.lba, c.blocks, c.data_length))
            .collect()
    }

    #[test]
    fn what_is_written_is_read_back() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let data = pattern(3 * 512, 1);
        disk.write(&mut xhci, 10, &data).unwrap();
        assert_eq!(stick.borrow().read_blocks(10, 3), data);
        let mut buf = vec![0u8; 3 * 512];
        disk.read(&mut xhci, 10, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(io(&stick, op::WRITE_10), [(10, 3, 1536)]);
        assert_eq!(io(&stick, op::READ_10), [(10, 3, 1536)]);
    }

    #[test]
    fn a_200_kib_request_is_split_into_64_kib_commands() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let data = pattern(200 * 1024, 2);
        disk.write(&mut xhci, 1000, &data).unwrap();
        let chunks = [
            (1000, 128, 65536),
            (1128, 128, 65536),
            (1256, 128, 65536),
            (1384, 16, 8192),
        ];
        assert_eq!(io(&stick, op::WRITE_10), chunks);
        let mut buf = vec![0u8; 200 * 1024];
        disk.read(&mut xhci, 1000, &mut buf).unwrap();
        assert_eq!(io(&stick, op::READ_10), chunks);
        assert!(buf == data);
    }

    #[test]
    fn blocks_of_4096_bytes_go_16_to_a_command() {
        let stick = FakeStorage::kingston();
        stick.borrow_mut().set_capacity(1000, 4096);
        let (_hal, mut xhci, mut disk) = started(&stick);
        let data = pattern(20 * 4096, 3);
        disk.write(&mut xhci, 980, &data).unwrap();
        assert_eq!(
            io(&stick, op::WRITE_10),
            [(980, 16, 65536), (996, 4, 16384)]
        );
        let mut buf = vec![0u8; 20 * 4096];
        disk.read(&mut xhci, 980, &mut buf).unwrap();
        assert!(buf == data);
    }

    #[test]
    fn the_first_and_the_last_block_can_be_used() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let last = disk.block_count() - 1;
        assert_eq!(last, 30_277_631);
        for lba in [0, last] {
            let data = pattern(512, lba as u8);
            disk.write(&mut xhci, lba, &data).unwrap();
            let mut buf = vec![0u8; 512];
            disk.read(&mut xhci, lba, &mut buf).unwrap();
            assert_eq!(buf, data);
        }
        assert_eq!(io(&stick, op::READ_10), [(0, 1, 512), (last, 1, 512)]);
    }

    #[test]
    fn requests_that_are_not_whole_blocks_inside_the_disk_send_nothing() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        let count = disk.block_count();
        let sent = stick.borrow().events().len();
        let not_whole = Err(UsbError::Unsupported("not whole blocks"));
        let past = Err(UsbError::Unsupported("past the end of the disk"));
        let mut buf = vec![0u8; 1024];
        assert_eq!(disk.read(&mut xhci, 0, &mut buf[..511]), not_whole);
        assert_eq!(disk.write(&mut xhci, 0, &buf[..513]), not_whole);
        assert_eq!(disk.read(&mut xhci, count, &mut buf[..512]), past);
        assert_eq!(disk.read(&mut xhci, count - 1, &mut buf), past);
        assert_eq!(disk.write(&mut xhci, count - 1, &buf), past);
        assert_eq!(disk.read(&mut xhci, u64::MAX, &mut buf[..512]), past);
        assert_eq!(disk.write(&mut xhci, u64::MAX - 1, &buf), past);
        // Nothing to do is nothing sent.
        assert_eq!(disk.read(&mut xhci, count, &mut []), Ok(()));
        assert_eq!(disk.write(&mut xhci, 5, &[]), Ok(()));
        assert_eq!(stick.borrow().events().len(), sent);
    }

    #[test]
    fn a_failed_command_fails_the_request_after_the_ones_before_it() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().medium_error_at(250);
        let mut buf = vec![0u8; 200 * 1024];
        let medium_error = Sense {
            key: 3,
            asc: 0x11,
            ascq: 0,
        };
        assert_eq!(
            disk.read(&mut xhci, 100, &mut buf),
            Err(UsbError::Sense(medium_error))
        );
        // The first command read LBA 100-227; the second has the bad
        // block and is tried three times; nothing after it is sent.
        assert_eq!(
            io(&stick, op::READ_10),
            [
                (100, 128, 65536),
                (228, 128, 65536),
                (228, 128, 65536),
                (228, 128, 65536)
            ]
        );
        assert!(hal.log_text().contains(
            "storage: slot 1: READ(10) at LBA 228, 128 blocks: MEDIUM ERROR (asc 0x11, ascq 0x00) (try 3 of 3)"
        ));
    }

    #[test]
    fn a_write_protected_stick_fails_writes_with_its_sense() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().write_protect();
        let protected = Sense {
            key: 7,
            asc: 0x27,
            ascq: 0,
        };
        assert_eq!(
            disk.write(&mut xhci, 8, &[0xAA; 512]),
            Err(UsbError::Sense(protected))
        );
        assert_eq!(io(&stick, op::WRITE_10).len(), 3);
        assert_eq!(stick.borrow().read_blocks(8, 1), [0; 512]);
        // Reads still work.
        let mut buf = [0u8; 512];
        assert_eq!(disk.read(&mut xhci, 8, &mut buf), Ok(()));
    }

    #[test]
    fn flush_synchronizes_the_cache() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        assert_eq!(disk.flush(&mut xhci), Ok(()));
        let sync = stick.borrow().commands();
        let sync = sync.last().unwrap();
        assert_eq!(
            (sync.opcode, sync.lba, sync.blocks),
            (op::SYNCHRONIZE_CACHE_10, 0, 0)
        );
    }

    #[test]
    fn a_stick_without_synchronize_cache_has_nothing_to_flush() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().no_synchronize_cache();
        for _ in 0..3 {
            assert_eq!(disk.flush(&mut xhci), Ok(()));
        }
        // Asked once (ILLEGAL REQUEST is not tried again), then not at all.
        let syncs = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::SYNCHRONIZE_CACHE_10)
            .count();
        assert_eq!(syncs, 1);
        let log = hal.log_text();
        assert_eq!(
            log.matches("SYNCHRONIZE CACHE not supported: no cache to flush")
                .count(),
            1
        );
    }

    #[test]
    fn a_flush_that_fails_otherwise_is_an_error() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().never_ready();
        let not_ready = Sense {
            key: 2,
            asc: 4,
            ascq: 1,
        };
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Sense(not_ready)));
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Sense(not_ready)));
    }

    #[test]
    fn unplugging_during_a_request_fails_it_at_once() {
        let (hal, mut xhci, mut disk, _stick) = kingston();
        // How long one command takes, to unplug the stick during the
        // second command of the next request.
        let mut buf = vec![0u8; 200 * 1024];
        let before = hal.clock();
        disk.read(&mut xhci, 0, &mut buf[..MAX_BULK]).unwrap();
        let one = hal.clock() - before;
        hal.fake().after(one + one / 2, |x, _| x.unplug(13));
        let before = hal.clock();
        assert_eq!(
            disk.read(&mut xhci, 0, &mut buf),
            Err(UsbError::Disconnected)
        );
        let took = hal.clock() - before;
        assert!(
            took > one && took < 3 * one,
            "{took:?}, one command {one:?}"
        );
        // Neither tried again nor recovered.
        let log = hal.log_text();
        assert!(!log.contains("(try") && !log.contains("reset recovery"));
        assert_eq!(
            disk.write(&mut xhci, 0, &buf[..512]),
            Err(UsbError::Disconnected)
        );
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Disconnected));
    }
}
````

- [ ] **Step 3: Declare the new module in `crates/usb/src/storage/mod.rs`**

In `crates/usb/src/storage/mod.rs`, replace:

````rust
mod disk;
pub mod scsi;
````

with:

````rust
mod disk;
mod io;
pub mod scsi;
````

- [ ] **Step 4: Add the failing tests to `crates/usb/src/storage/transport.rs`**

In `crates/usb/src/storage/transport.rs`, make these 12 replacements, top to bottom:

Replace:

````rust
    use super::*;
    use crate::testing::{Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured};
    use crate::xhci::Xhci;
    use std::cell::RefCell;
````

with:

````rust
    use super::*;
    use crate::testing::{
        BadCsw, Event, FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op,
    };
    use crate::xhci::Xhci;
    use core::time::Duration;
    use std::cell::RefCell;
````

Replace:

````rust
        let mut buf = [0u8; 36];
        let n = t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf));
        assert_eq!(n, Ok(36));
        assert_eq!(&buf[8..16], b"Kingston");
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x12, 0x00]);
````

with:

````rust
        let mut buf = [0u8; 36];
        let n = t.execute(
            &mut xhci,
            &scsi::inquiry(36),
            Data::In(&mut buf),
            Need::UpTo,
        );
        assert_eq!(n, Ok(36));
        assert_eq!(&buf[8..16], b"Kingston");
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x12, 0x00]);
````

Replace:

````rust
        for _ in 0..2 {
            assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        }
````

with:

````rust
        for _ in 0..2 {
            assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
        }
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Sense(not_ready))
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x00, 0x03, 0x00]);
````

with:

````rust
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Sense(not_ready))
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
        assert_eq!(stick.borrow().opcodes(), [0x00, 0x03, 0x00]);
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut xhci, &scsi::inquiry(36), Data::In(&mut buf)),
            Err(UsbError::Protocol("phase error"))
````

with:

````rust
        assert_eq!(
            t.execute(
                &mut xhci,
                &scsi::inquiry(36),
                Data::In(&mut buf),
                Need::UpTo
            ),
            Err(UsbError::Protocol("phase error"))
````

Replace:

````rust
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None), Ok(0));
    }
````

with:

````rust
        );
        assert_eq!(t.execute(&mut xhci, &TUR, Data::None, Need::UpTo), Ok(0));
    }
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_10(0, 129), Data::In(&mut big)),
            Err(UsbError::Unsupported("data phase over 64 KiB"))
````

with:

````rust
        assert_eq!(
            t.execute(
                &mut xhci,
                &scsi::read_10(0, 129),
                Data::In(&mut big),
                Need::UpTo
            ),
            Err(UsbError::Unsupported("data phase over 64 KiB"))
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut xhci, &read, Data::In(&mut max)),
            Ok(MAX_BULK)
        );
    }

    #[test]
    fn a_stalled_data_phase_is_followed_by_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = kingston();
        // A failed command: the Kingston stalls its data phase.
        stick.borrow_mut().never_ready();
        let mut buf = [0u8; 8];
        assert_eq!(
            t.execute(&mut xhci, &scsi::read_capacity_10(), Data::In(&mut buf)),
            Err(UsbError::Stall)
        );
        assert_eq!(
            stick.borrow().events()[1..],
            [Event::Reset, Event::ClearHalt(0x81), Event::ClearHalt(0x02)]
        );
        let mut buf = [0u8; 36];
        let inquiry = scsi::inquiry(36);
        assert_eq!(t.execute(&mut xhci, &inquiry, Data::In(&mut buf)), Ok(36));
    }
````

with:

````rust
        assert_eq!(
            t.execute(&mut xhci, &read, Data::In(&mut max), Need::All),
            Ok(MAX_BULK)
        );
    }
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Protocol("CBW not taken whole"))
````

with:

````rust
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Protocol("CBW not taken whole"))
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None),
            Err(UsbError::Disconnected)
````

with:

````rust
        assert_eq!(
            t.execute(&mut bus, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Disconnected)
````

Replace:

````rust
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None),
            Err(UsbError::Disconnected)
````

with:

````rust
        assert_eq!(
            t.execute(&mut xhci, &TUR, Data::None, Need::UpTo),
            Err(UsbError::Disconnected)
````

Replace:

````rust
        assert!(stick.borrow().events().is_empty());
    }
}
````

with:

````rust
        assert!(stick.borrow().events().is_empty());
    }

    fn stick_with(
        knobs: impl FnOnce(&mut FakeStorage),
    ) -> (FakeHal, Xhci<FakeHal>, Transport, Stick) {
        let (hal, xhci, t, stick) = kingston();
        let block: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
        stick.borrow_mut().write_blocks(5, &block);
        knobs(&mut stick.borrow_mut());
        (hal, xhci, t, stick)
    }

    fn read_block(t: &mut Transport, bus: &mut dyn Bus, lba: u32) -> Result<Vec<u8>, UsbError> {
        let mut buf = vec![0u8; 512];
        let n = t.command(bus, &scsi::read_10(lba, 1), Data::In(&mut buf), Need::All)?;
        assert_eq!(n, 512);
        Ok(buf)
    }

    fn write_block(
        t: &mut Transport,
        bus: &mut dyn Bus,
        lba: u32,
        data: &[u8],
    ) -> Result<usize, UsbError> {
        t.command(bus, &scsi::write_10(lba, 1), Data::Out(data), Need::All)
    }

    /// The events as opcodes, resets and cleared halts, for comparing.
    fn trace(stick: &Stick) -> Vec<String> {
        stick
            .borrow()
            .events()
            .iter()
            .map(|e| match e {
                Event::GetMaxLun => "GET_MAX_LUN".into(),
                Event::Command(c) => format!("{:02x}", c.opcode),
                Event::Reset => "reset".into(),
                Event::ClearHalt(ep) => format!("clear {ep:02x}"),
            })
            .collect()
    }

    #[test]
    fn a_stalled_data_in_phase_is_cleared_the_csw_read_and_the_command_tried_again() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_next_data_in());
        let block = stick.borrow().read_blocks(5, 1);
        assert_eq!(read_block(&mut t, &mut xhci, 5), Ok(block));
        assert_eq!(trace(&stick), ["28", "clear 81", "03", "28"]);
        let log = hal.log_text();
        assert!(log.contains("storage: slot 1: READ(10) at LBA 5, 1 block: data phase stalled"));
        // The halt was cleared before the CSW was read, which did not stall.
        assert!(!log.contains("CSW stalled"));
        assert!(log.contains(
            "storage: slot 1: READ(10) at LBA 5, 1 block: ABORTED COMMAND (asc 0x00, ascq 0x00) (try 1 of 3)"
        ));
        let c = stick.borrow().commands();
        assert_eq!((c[0].opcode, c[0].lba, c[0].blocks), (op::READ_10, 5, 1));
    }

    #[test]
    fn a_stalled_data_out_phase_is_cleared_the_csw_read_and_the_command_tried_again() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_next_data_out());
        let data = [0x5A; 512];
        assert_eq!(write_block(&mut t, &mut xhci, 7, &data), Ok(512));
        assert_eq!(trace(&stick), ["2a", "clear 02", "03", "2a"]);
        assert_eq!(stick.borrow().read_blocks(7, 1), data);
    }

    #[test]
    fn a_stalled_csw_is_read_again_after_clearing_the_halt() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_csw_reads(1));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "clear 81"]);
        assert!(
            hal.log_text()
                .contains("storage: slot 1: READ(10) at LBA 5, 1 block: CSW stalled")
        );
    }

    #[test]
    fn a_csw_that_stalls_twice_is_followed_by_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.stall_csw_reads(2));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(
            trace(&stick),
            ["28", "clear 81", "reset", "clear 81", "clear 02", "28"]
        );
    }

    #[test]
    fn a_unit_attention_is_retried_like_any_failure() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.unit_attention());
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "clear 81", "03", "28"]);
        assert!(hal.log_text().contains(
            "READ(10) at LBA 5, 1 block: UNIT ATTENTION (asc 0x29, ascq 0x00) (try 1 of 3)"
        ));
    }

    #[test]
    fn a_phase_error_is_retried_after_a_reset_recovery() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.phase_error_next());
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "reset", "clear 81", "clear 02", "28"]);
    }

    #[test]
    fn invalid_csws_are_retried_after_a_reset_recovery() {
        for (bad, why) in [
            (BadCsw::Signature, "bad CSW signature"),
            (BadCsw::Tag, "CSW tag does not match its CBW"),
            (BadCsw::Short, "CSW not 13 bytes"),
        ] {
            let (hal, mut xhci, mut t, stick) = stick_with(|s| s.bad_csw_next(bad));
            assert!(read_block(&mut t, &mut xhci, 5).is_ok());
            assert_eq!(trace(&stick), ["28", "reset", "clear 81", "clear 02", "28"]);
            assert!(hal.log_text().contains(&format!(
                "READ(10) at LBA 5, 1 block: protocol error: {why}; reset recovery"
            )));
        }
    }

    #[test]
    fn short_data_or_a_residue_fails_a_read_or_write_and_it_is_tried_again() {
        // Short, with the residue that says so, or without it.
        for residue in [None, Some(0)] {
            let (hal, mut xhci, mut t, stick) = stick_with(|s| {
                s.short_next_data_in(100);
                if let Some(r) = residue {
                    s.residue_next(r);
                }
            });
            assert!(read_block(&mut t, &mut xhci, 5).is_ok());
            assert_eq!(trace(&stick), ["28", "28"]);
            assert!(
                hal.log_text()
                    .contains("protocol error: short data phase (try 1 of 3)")
            );
        }
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.residue_next(512));
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
        assert_eq!(trace(&stick), ["28", "28"]);
        assert!(
            hal.log_text()
                .contains("protocol error: data residue (try 1 of 3)")
        );
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.residue_next(1));
        assert_eq!(write_block(&mut t, &mut xhci, 5, &[1; 512]), Ok(512));
        assert_eq!(trace(&stick), ["2a", "2a"]);
        // Less than asked is fine for INQUIRY.
        let (_hal, mut xhci, mut t, _stick) = stick_with(|s| s.short_next_data_in(10));
        let mut buf = [0u8; 36];
        let inquiry = scsi::inquiry(36);
        let n = t.command(&mut xhci, &inquiry, Data::In(&mut buf), Need::UpTo);
        assert_eq!(n, Ok(26));
    }

    #[test]
    fn a_command_is_tried_three_times_then_fails_with_its_last_error() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.medium_error_at(5));
        let medium_error = Sense {
            key: 3,
            asc: 0x11,
            ascq: 0,
        };
        assert_eq!(
            read_block(&mut t, &mut xhci, 5),
            Err(UsbError::Sense(medium_error))
        );
        let one = ["28", "clear 81", "03"];
        assert_eq!(trace(&stick), [one, one, one].concat());
        assert!(hal.log_text().contains(
            "storage: slot 1: READ(10) at LBA 5, 1 block: MEDIUM ERROR (asc 0x11, ascq 0x00) (try 3 of 3)"
        ));
        // The rest of the disk still reads.
        assert!(read_block(&mut t, &mut xhci, 6).is_ok());
    }

    #[test]
    fn a_device_that_does_not_answer_costs_5_s_a_try_and_ends_with_a_timeout() {
        let (hal, mut xhci, mut t, stick) = stick_with(|s| s.nak(true));
        let before = hal.clock();
        assert_eq!(read_block(&mut t, &mut xhci, 5), Err(UsbError::Timeout));
        let took = hal.clock() - before;
        assert!(
            took >= Duration::from_secs(15) && took < Duration::from_secs(17),
            "{took:?}"
        );
        // Each CBW timed out (the device never took one) and was followed
        // by a reset recovery.
        assert!(stick.borrow().commands().is_empty());
        assert_eq!(stick.borrow().resets(), 3);
        stick.borrow_mut().nak(false);
        assert!(read_block(&mut t, &mut xhci, 5).is_ok());
    }

    #[test]
    fn a_gone_device_or_controller_is_not_tried_again() {
        for gone in [UsbError::Disconnected, UsbError::ControllerDead] {
            let (_hal, mut xhci, mut t, stick) = stick_with(|_| {});
            let mut bus = TamperBus::new(&mut xhci);
            bus.fail_bulk = Some((0, gone));
            assert_eq!(read_block(&mut t, &mut bus, 5), Err(gone));
            assert_eq!(bus.bulk, 1);
            assert!(stick.borrow().events().is_empty());
        }
    }

    #[test]
    fn illegal_request_is_not_tried_again() {
        let (_hal, mut xhci, mut t, stick) = stick_with(|s| s.no_synchronize_cache());
        let sync = scsi::synchronize_cache_10();
        let r = t.command(&mut xhci, &sync, Data::None, Need::UpTo);
        let invalid = Sense {
            key: 5,
            asc: 0x20,
            ascq: 0,
        };
        assert_eq!(r, Err(UsbError::Sense(invalid)));
        assert_eq!(trace(&stick), ["35", "03"]);
    }
}
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/mod.rs`**

In `crates/usb/src/testing/mod.rs`, replace:

````rust
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use storage::{Event, FakeStorage, configured, op};
pub use xhci::{ExtCap, FakeCap, FakeConfig};
````

with:

````rust
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use storage::{BadCsw, Event, FakeStorage, configured, op};
pub use xhci::{ExtCap, FakeCap, FakeConfig};
````

- [ ] **Step 6: Extend the test support in `crates/usb/src/testing/storage/bot.rs`**

In `crates/usb/src/testing/storage/bot.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

    /// Enters the data phase of a command that ends with `status`. A
    /// command that did not pass stalls its data phase or pads it
    /// (zeros in, data out discarded), as `failed_data` says.
    pub(super) fn begin(
````

with:

````rust

    /// Enters the data phase of a command that ends with `status`: a
    /// command that did not pass stalls its data phase if `stall`, or pads
    /// it (zeros in, data out discarded).
    pub(super) fn begin(
````

Replace:

````rust
        status: u8,
    ) {
        let stall = status == FAILED && self.failed_data == FailedData::Stall;
        self.phase = if length == 0 {
````

with:

````rust
        status: u8,
        stall: bool,
    ) {
        self.phase = if length == 0 {
````

Replace:

````rust
            data.truncate(length as usize);
            Phase::DataIn {
````

with:

````rust
            data.truncate(length as usize);
            if status == PASSED {
                let short = std::mem::take(&mut self.short_data_in) as usize;
                data.truncate(data.len().saturating_sub(short));
            }
            Phase::DataIn {
````

Replace:

````rust
        }
        if self.usb.is_halted(BULK_IN) {
````

with:

````rust
        }
        if self.nak {
            return None;
        }
        if self.usb.is_halted(BULK_IN) {
````

Replace:

````rust
                }
                let mut b = CSW_SIGNATURE.to_le_bytes().to_vec();
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&residue.to_le_bytes());
                b.push(*status);
                self.phase = Phase::Cbw;
````

with:

````rust
                }
                if self.stall_csw > 0 {
                    self.stall_csw -= 1;
                    self.usb.stall_endpoint(BULK_IN);
                    return Some(Err(Stall));
                }
                let residue = self.residue.take().unwrap_or(*residue);
                let (mut tag, mut signature) = (*tag, CSW_SIGNATURE);
                let bad = self.bad_csw.take();
                match bad {
                    Some(BadCsw::Signature) => signature = CBW_SIGNATURE,
                    Some(BadCsw::Tag) => tag = tag.wrapping_add(1),
                    _ => {}
                }
                let mut b = signature.to_le_bytes().to_vec();
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&residue.to_le_bytes());
                b.push(*status);
                if bad == Some(BadCsw::Short) {
                    b.truncate(CSW_LEN - 1);
                }
                self.phase = Phase::Cbw;
````

Replace:

````rust
            panic!("fake storage: OUT transfer on endpoint {endpoint:#04x}");
        }
````

with:

````rust
            panic!("fake storage: OUT transfer on endpoint {endpoint:#04x}");
        }
        if self.nak {
            return None;
        }
````

- [ ] **Step 7: Extend the test support in `crates/usb/src/testing/storage/mod.rs`**

In `crates/usb/src/testing/storage/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    phase_error: bool,
}
````

with:

````rust
    phase_error: bool,
    /// Knob: the next data-IN or data-OUT phase stalls (the command then
    /// fails with ABORTED COMMAND).
    stall_data_in: bool,
    stall_data_out: bool,
    /// Knob: this many CSW reads stall.
    stall_csw: usize,
    /// Knob: what is wrong with the next CSW.
    bad_csw: Option<BadCsw>,
    /// Knob: the next data-IN phase keeps back this many bytes (its CSW
    /// reports them as the residue).
    short_data_in: u32,
    /// Knob: the residue the next CSW reports, whatever moved.
    residue: Option<u32>,
    /// Knob: every bulk transfer is NAKed: the device does not answer.
    nak: bool,
    /// Knob: reading this block fails with MEDIUM ERROR.
    medium_error: Option<u64>,
    /// Knob: writes fail with DATA PROTECT.
    write_protected: bool,
    /// Knob: SYNCHRONIZE CACHE fails with ILLEGAL REQUEST.
    no_cache_sync: bool,
}

/// A CSW that breaks BOT 6.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadCsw {
    Signature,
    /// The tag of another CBW.
    Tag,
    /// 12 bytes.
    Short,
}
````

Replace:

````rust
            phase_error: false,
        }))
````

with:

````rust
            phase_error: false,
            stall_data_in: false,
            stall_data_out: false,
            stall_csw: 0,
            bad_csw: None,
            short_data_in: 0,
            residue: None,
            nak: false,
            medium_error: None,
            write_protected: false,
            no_cache_sync: false,
        }))
````

Replace:

````rust
        self.phase_error = true;
    }
````

with:

````rust
        self.phase_error = true;
    }

    /// Knob: the next data-IN phase stalls; once the host has cleared the
    /// halt, the CSW says the command failed (ABORTED COMMAND).
    pub fn stall_next_data_in(&mut self) {
        self.stall_data_in = true;
    }

    /// Knob: the next data-OUT phase stalls, as `stall_next_data_in`.
    pub fn stall_next_data_out(&mut self) {
        self.stall_data_out = true;
    }

    /// Knob: the next `n` CSW reads stall (the CSW stays pending).
    pub fn stall_csw_reads(&mut self, n: usize) {
        self.stall_csw = n;
    }

    /// Knob: the next CSW is broken this way.
    pub fn bad_csw_next(&mut self, bad: BadCsw) {
        self.bad_csw = Some(bad);
    }

    /// Knob: the next data-IN phase is `bytes` shorter than asked, and its
    /// CSW reports them as the residue.
    pub fn short_next_data_in(&mut self, bytes: u32) {
        self.short_data_in = bytes;
    }

    /// Knob: the next CSW reports a residue of `bytes`, whatever moved
    /// (Linux knows devices whose residue is wrong: US_FL_IGNORE_RESIDUE).
    pub fn residue_next(&mut self, bytes: u32) {
        self.residue = Some(bytes);
    }

    /// Knob: every bulk transfer is NAKed while `on`: transfers time out.
    pub fn nak(&mut self, on: bool) {
        self.nak = on;
    }

    /// Knob: reading block `lba` fails with MEDIUM ERROR, unrecovered read
    /// error (0x03/0x11/0x00).
    pub fn medium_error_at(&mut self, lba: u64) {
        self.medium_error = Some(lba);
    }

    /// Knob: writes fail with DATA PROTECT, write protected
    /// (0x07/0x27/0x00).
    pub fn write_protect(&mut self) {
        self.write_protected = true;
    }

    /// Knob: SYNCHRONIZE CACHE fails with ILLEGAL REQUEST, invalid command
    /// operation code (0x05/0x20/0x00), as many cheap sticks answer.
    pub fn no_synchronize_cache(&mut self) {
        self.no_cache_sync = true;
    }
````

- [ ] **Step 8: Extend the test support in `crates/usb/src/testing/storage/scsi.rs`**

In `crates/usb/src/testing/storage/scsi.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
const POWER_ON_RESET: Sense = (0x06, 0x29, 0x00);

````

with:

````rust
const POWER_ON_RESET: Sense = (0x06, 0x29, 0x00);
const ABORTED: Sense = (0x0B, 0x00, 0x00);
const UNRECOVERED_READ_ERROR: Sense = (0x03, 0x11, 0x00);
const WRITE_PROTECTED: Sense = (0x07, 0x27, 0x00);
const INVALID_OPCODE: Sense = (0x05, 0x20, 0x00);

````

Replace:

````rust
        if std::mem::take(&mut self.phase_error) {
            return self.begin(tag, length, dir_in, Answer::None, PHASE_ERROR);
        }
````

with:

````rust
        if std::mem::take(&mut self.phase_error) {
            return self.begin(tag, length, dir_in, Answer::None, PHASE_ERROR, false);
        }
        let stall_knob = match (length, dir_in) {
            (0, _) => false,
            (_, true) => std::mem::take(&mut self.stall_data_in),
            (_, false) => std::mem::take(&mut self.stall_data_out),
        };
        if stall_knob {
            self.sense = ABORTED;
            return self.begin(tag, length, dir_in, Answer::None, FAILED, true);
        }
````

Replace:

````rust
                }
                self.begin(tag, length, dir_in, answer, PASSED);
            }
            Err(sense) => {
                self.sense = sense;
                self.begin(tag, length, dir_in, Answer::None, FAILED);
            }
````

with:

````rust
                }
                self.begin(tag, length, dir_in, answer, PASSED, false);
            }
            Err(sense) => {
                self.sense = sense;
                let stall = self.failed_data == FailedData::Stall;
                self.begin(tag, length, dir_in, Answer::None, FAILED, stall);
            }
````

Replace:

````rust
            }
            READ_10 => Answer::In(self.read_blocks(lba, blocks as u64)),
            WRITE_10 => Answer::Out(lba),
            _ => Answer::None,
````

with:

````rust
            }
            READ_10 => {
                if self
                    .medium_error
                    .is_some_and(|bad| (lba..lba + blocks as u64).contains(&bad))
                {
                    return Err(UNRECOVERED_READ_ERROR);
                }
                Answer::In(self.read_blocks(lba, blocks as u64))
            }
            WRITE_10 if self.write_protected => return Err(WRITE_PROTECTED),
            WRITE_10 => Answer::Out(lba),
            SYNCHRONIZE_CACHE_10 if self.no_cache_sync => return Err(INVALID_OPCODE),
            _ => Answer::None,
````

- [ ] **Step 9: Add the failing tests to `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, replace:

````rust
        let mut buf = vec![0; 512];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Transfer(4))
        );
        assert!(hal.clock() - before < Duration::from_millis(10));
    }
````

with:

````rust
        let mut buf = vec![0; 512];
        // A USB Transaction Error on a port without a connection: the
        // device is gone, which the class driver must know (it does not
        // retry then).
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Disconnected)
        );
        assert!(hal.clock() - before < Duration::from_millis(10));
        assert_eq!(
            xhci.bulk_out(d.slot, 0x02, &[0; 31]),
            Err(UsbError::Disconnected)
        );
    }

    #[test]
    fn a_dead_controller_is_reported_as_dead_even_after_an_unplug() {
        let (hal, mut xhci, d, _dev) = stick(FakeConfig::intel(), 13);
        hal.fake().unplug(13);
        hal.fake().host_system_error();
        xhci.poll();
        let mut buf = vec![0; 512];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::ControllerDead)
        );
    }

    #[test]
    fn a_bulk_transfer_that_times_out_after_an_unplug_is_disconnected_too() {
        // A controller that does not fail transfers on unplug.
        let mut config = FakeConfig::intel();
        config.fail_transfers_on_unplug = false;
        let (hal, mut xhci, d, _dev) = stick(config, 13);
        hal.fake()
            .after(Duration::from_millis(5), |x, _| x.unplug(13));
        let mut buf = vec![0; 512];
        assert_eq!(
            xhci.bulk_in(d.slot, 0x81, &mut buf),
            Err(UsbError::Disconnected)
        );
    }
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `MassStorage` in this scope ``; `` cannot find type `UsbError` in this scope ``.

- [ ] **Step 11: Change `crates/usb/src/storage/disk.rs`**

In `crates/usb/src/storage/disk.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
use super::scsi::{self, Capacity, Inquiry, NOT_READY};
use super::transport::{Data, Transport, is_fatal};
use crate::UsbError;
````

with:

````rust
use super::scsi::{self, Capacity, Inquiry, NOT_READY};
use super::transport::{Data, Need, Transport, is_fatal};
use crate::UsbError;
````

Replace:

````rust
pub struct MassStorage {
    transport: Transport,
    block_size: usize,
    block_count: u64,
    vendor: String,
    product: String,
}
````

with:

````rust
pub struct MassStorage {
    pub(super) transport: Transport,
    pub(super) block_size: usize,
    pub(super) block_count: u64,
    vendor: String,
    product: String,
    /// SYNCHRONIZE CACHE was refused: there is no cache to flush.
    pub(super) no_cache: bool,
}
````

Replace:

````rust
            product: inquiry.product,
        })
````

with:

````rust
            product: inquiry.product,
            no_cache: false,
        })
````

Replace:

````rust

/// Runs a command that reads into `buf` and returns what came.
fn read_data<'a>(
````

with:

````rust

/// Runs a command that reads into `buf` (tried three times; each failure is
/// logged) and returns what came.
fn read_data<'a>(
````

Replace:

````rust
) -> Result<&'a [u8], UsbError> {
    let n = t.execute(bus, cdb, Data::In(buf))?;
    Ok(&buf[..n.min(buf.len())])
````

with:

````rust
) -> Result<&'a [u8], UsbError> {
    let n = t.command(bus, cdb, Data::In(buf), Need::UpTo)?;
    Ok(&buf[..n.min(buf.len())])
````

Replace:

````rust
    let cdb = scsi::inquiry(scsi::INQUIRY_LEN as u16);
    let parsed = read_data(bus, t, &cdb, &mut buf).and_then(Inquiry::parse);
    let i = parsed.inspect_err(|e| slog!(bus, slot, "INQUIRY: {e}"))?;
    slog!(
````

with:

````rust
    let cdb = scsi::inquiry(scsi::INQUIRY_LEN as u16);
    let data = read_data(bus, t, &cdb, &mut buf)?;
    let i = Inquiry::parse(data).inspect_err(|e| slog!(bus, slot, "INQUIRY: {e}"))?;
    slog!(
````

Replace:

````rust
        tries = tries.saturating_add(1);
        let e = match t.execute(bus, &scsi::test_unit_ready(), Data::None) {
            Ok(_) => {
````

with:

````rust
        tries = tries.saturating_add(1);
        let e = match t.execute(bus, &scsi::test_unit_ready(), Data::None, Need::UpTo) {
            Ok(_) => {
````

Replace:

````rust
    let cdb = scsi::read_capacity_10();
    let r = read_data(bus, t, &cdb, &mut buf[..scsi::CAPACITY_10_LEN]).and_then(Capacity::parse_10);
    let mut cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(10): {e}"))?;
    if cap.last_lba == 0xFFFF_FFFF {
        let cdb = scsi::read_capacity_16(scsi::CAPACITY_16_LEN as u32);
        let r = read_data(bus, t, &cdb, &mut buf).and_then(Capacity::parse_16);
        cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(16): {e}"))?;
````

with:

````rust
    let cdb = scsi::read_capacity_10();
    let data = read_data(bus, t, &cdb, &mut buf[..scsi::CAPACITY_10_LEN])?;
    let r = Capacity::parse_10(data);
    let mut cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(10): {e}"))?;
    if cap.last_lba == 0xFFFF_FFFF {
        let cdb = scsi::read_capacity_16(scsi::CAPACITY_16_LEN as u32);
        let r = Capacity::parse_16(read_data(bus, t, &cdb, &mut buf)?);
        cap = r.inspect_err(|e| slog!(bus, slot, "READ CAPACITY(16): {e}"))?;
````

- [ ] **Step 12: Implement `crates/usb/src/storage/io.rs`**

Insert this at the top of `crates/usb/src/storage/io.rs`, above `#[cfg(test)]`:

````rust
//! Reading and writing a disk (spec §6.4, SBC-3 §5.8, §5.32, §5.22):
//! READ(10) and WRITE(10) of at most [`MAX_BULK`] bytes each, and
//! SYNCHRONIZE CACHE(10) for `flush`. A request must be whole blocks inside
//! the disk, or nothing is sent.

use super::disk::MassStorage;
use super::scsi::{self, ILLEGAL_REQUEST};
use super::transport::{Data, Need};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK};

/// How a request moves: READ(10) into the buffer or WRITE(10) from it.
enum Request<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl MassStorage {
    /// Reads `buf.len() / block_size` blocks from `lba`: READ(10) commands
    /// of at most MAX_BULK bytes each. `buf` must be whole blocks inside
    /// the disk (`Unsupported` otherwise, nothing sent).
    pub fn read(&mut self, bus: &mut dyn Bus, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
        self.request(bus, lba, Request::Read(buf))
    }

    /// Writes `buf` to the blocks from `lba`, as `read`. A failure in one
    /// command fails the request; the commands before it were written.
    pub fn write(&mut self, bus: &mut dyn Bus, lba: u64, buf: &[u8]) -> Result<(), UsbError> {
        self.request(bus, lba, Request::Write(buf))
    }

    /// SYNCHRONIZE CACHE(10) over the whole disk. A device that does not
    /// support it (ILLEGAL REQUEST) has no cache to flush: that is
    /// success, logged once.
    pub fn flush(&mut self, bus: &mut dyn Bus) -> Result<(), UsbError> {
        if self.no_cache {
            return Ok(());
        }
        let cdb = scsi::synchronize_cache_10();
        match self.transport.command(bus, &cdb, Data::None, Need::UpTo) {
            Err(UsbError::Sense(s)) if s.key == ILLEGAL_REQUEST => {
                slog!(
                    bus,
                    self.slot(),
                    "SYNCHRONIZE CACHE not supported: no cache to flush"
                );
                self.no_cache = true;
                Ok(())
            }
            r => r.map(|_| ()),
        }
    }

    /// Checks the request, then sends one command per MAX_BULK bytes (a
    /// multiple of every block size the disk may have).
    fn request(&mut self, bus: &mut dyn Bus, lba: u64, req: Request) -> Result<(), UsbError> {
        let len = match &req {
            Request::Read(b) => b.len(),
            Request::Write(b) => b.len(),
        };
        let size = self.block_size;
        if !len.is_multiple_of(size) {
            return Err(UsbError::Unsupported("not whole blocks"));
        }
        match lba.checked_add((len / size) as u64) {
            Some(end) if end <= self.block_count => {}
            _ => return Err(UsbError::Unsupported("past the end of the disk")),
        }
        let per_command = (MAX_BULK / size) as u64;
        // Command `i` of `bytes`: its LBA is inside the disk, whose at most
        // 2^32 blocks READ(10) reaches (`start` checked), so it fits 32
        // bits; and at most MAX_BULK / 512 blocks fit 16.
        let command = |i: usize, bytes: usize| {
            let at = lba + i as u64 * per_command;
            (at as u32, (bytes / size) as u16)
        };
        match req {
            Request::Read(buf) => {
                for (i, part) in buf.chunks_mut(MAX_BULK).enumerate() {
                    let (at, blocks) = command(i, part.len());
                    let cdb = scsi::read_10(at, blocks);
                    self.transport
                        .command(bus, &cdb, Data::In(part), Need::All)?;
                }
            }
            Request::Write(buf) => {
                for (i, part) in buf.chunks(MAX_BULK).enumerate() {
                    let (at, blocks) = command(i, part.len());
                    let cdb = scsi::write_10(at, blocks);
                    self.transport
                        .command(bus, &cdb, Data::Out(part), Need::All)?;
                }
            }
        }
        Ok(())
    }
}

````

- [ ] **Step 13: Change `crates/usb/src/storage/transport.rs`**

In `crates/usb/src/storage/transport.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! One SCSI command over Bulk-Only Transport (BOT 5 and 6): the CBW on bulk
//! OUT, the data phase, then the CSW on bulk IN. A command that failed
//! (CSW status 1) is followed by REQUEST SENSE, and its sense becomes the
//! error. A phase error, an invalid CSW or a transfer that fails is followed
//! by a reset recovery (BOT 5.3.4), after which the device waits for a CBW
//! again.

use super::bot::{CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};
use super::scsi::{self, SENSE_LEN, Sense};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};

````

with:

````rust
//! One SCSI command over Bulk-Only Transport (BOT 5 and 6): the CBW on bulk
//! OUT, the data phase, then the CSW on bulk IN, with the recovery of spec
//! §6.4 and BOT 5.3 and 6.7:
//!
//! - a STALL in the data phase: the halt is cleared and the CSW read;
//! - a STALL on the CSW: the halt is cleared and the CSW read once more;
//! - CSW status 1: REQUEST SENSE, whose sense becomes the error;
//! - a phase error, an invalid CSW, a transfer that fails or times out:
//!   reset recovery (BOT 5.3.4), after which the device waits for a CBW.
//!
//! A command is tried three times at most; a gone device or controller is
//! not tried again.

use super::bot::{CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};
use super::scsi::{self, ILLEGAL_REQUEST, SENSE_LEN, Sense};
use crate::UsbError;
use crate::bus::{Bus, MAX_BULK, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use core::fmt;

````

Replace:

````rust
    In(&'a mut [u8]),
}

impl Data<'_> {
    fn len(&self) -> usize {
        match self {
            Data::None => 0,
            Data::In(b) => b.len(),
        }
    }
}

/// The device or the controller is gone: nothing more is tried.
pub fn is_fatal(e: UsbError) -> bool {
    matches!(e, UsbError::Disconnected | UsbError::ControllerDead)
}
````

with:

````rust
    In(&'a mut [u8]),
    Out(&'a [u8]),
}

impl Data<'_> {
    /// The same buffer again, for another try.
    fn reborrow(&mut self) -> Data<'_> {
        match self {
            Data::None => Data::None,
            Data::In(b) => Data::In(b),
            Data::Out(b) => Data::Out(b),
        }
    }

    fn len(&self) -> usize {
        match self {
            Data::None => 0,
            Data::In(b) => b.len(),
            Data::Out(b) => b.len(),
        }
    }
}

/// How much of its data phase a command needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// What the device has, up to the buffer (INQUIRY, REQUEST SENSE and
    /// READ CAPACITY may answer less than was asked).
    UpTo,
    /// Every byte (READ, WRITE): a data phase that moved less, or a CSW
    /// with a residue, is an error.
    All,
}

/// Tries of one command: the first and two retries (spec §6.4).
pub const MAX_TRIES: u32 = 3;

/// The device or the controller is gone: nothing more is tried.
pub fn is_fatal(e: UsbError) -> bool {
    matches!(e, UsbError::Disconnected | UsbError::ControllerDead)
}

/// A command for log lines: its name, and for READ(10) and WRITE(10) where
/// and how much.
struct Named<'a>(&'a [u8]);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self.0 {
            [
                op @ (scsi::READ_10 | scsi::WRITE_10),
                _,
                a,
                b,
                c,
                d,
                _,
                n0,
                n1,
                ..,
            ] => {
                let lba = u32::from_be_bytes([a, b, c, d]);
                let blocks = u16::from_be_bytes([n0, n1]);
                let plural = if blocks == 1 { "" } else { "s" };
                let name = scsi::command_name(op);
                write!(f, "{name} at LBA {lba}, {blocks} block{plural}")
            }
            [op, ..] => f.write_str(scsi::command_name(op)),
            [] => f.write_str("command"),
        }
    }
}
````

Replace:

````rust
    /// Runs `cdb` once with its data phase and returns the bytes the data
    /// phase moved. A failed command's error is its sense; any other
    /// failure is followed by a reset recovery. A data phase over
    /// [`MAX_BULK`] is `Unsupported`, and nothing is sent.
    pub fn execute(
````

with:

````rust
    /// Runs `cdb` once with its data phase and returns the bytes the data
    /// phase moved. A failed command's error is its sense; a transfer that
    /// fails or times out, an invalid CSW or a phase error is followed by a
    /// reset recovery. With `Need::All`, a short data phase or a residue
    /// is an error too. A data phase over [`MAX_BULK`] is `Unsupported`,
    /// and nothing is sent.
    pub fn execute(
````

Replace:

````rust
        data: Data,
    ) -> Result<usize, UsbError> {
        if data.len() > MAX_BULK {
            return Err(UsbError::Unsupported("data phase over 64 KiB"));
        }
        let name = scsi::command_name(cdb.first().copied().unwrap_or(0xFF));
        match self.exchange(bus, cdb, data) {
            Ok((moved, CswStatus::Passed)) => Ok(moved),
            Ok((_, CswStatus::Failed)) => Err(self.request_sense(bus)),
            Ok((_, CswStatus::PhaseError)) => {
                Err(self.recover(bus, name, UsbError::Protocol("phase error")))
            }
            Err(e) => Err(self.recover(bus, name, e)),
        }
    }

    /// CBW, data phase, CSW: the bytes moved and the CSW's status.
    fn exchange(
````

with:

````rust
        data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let length = data.len();
        if length > MAX_BULK {
            return Err(UsbError::Unsupported("data phase over 64 KiB"));
        }
        match self.exchange(bus, cdb, data) {
            Ok((moved, csw)) => match csw.status {
                CswStatus::Passed if need == Need::All && moved != length => {
                    Err(UsbError::Protocol("short data phase"))
                }
                CswStatus::Passed if need == Need::All && csw.residue != 0 => {
                    Err(UsbError::Protocol("data residue"))
                }
                CswStatus::Passed => Ok(moved),
                CswStatus::Failed => Err(self.request_sense(bus)),
                CswStatus::PhaseError => {
                    Err(self.recover(bus, cdb, UsbError::Protocol("phase error")))
                }
            },
            Err(e) => Err(self.recover(bus, cdb, e)),
        }
    }

    /// Runs `cdb` until it succeeds, [`MAX_TRIES`] times at most, and
    /// returns the bytes its data phase moved. A gone device or controller
    /// ends it at once, and so does ILLEGAL REQUEST (the command itself is
    /// wrong). Each failed try is logged with the command.
    pub fn command(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        mut data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let mut tries = 1;
        loop {
            let e = match self.execute(bus, cdb, data.reborrow(), need) {
                Ok(n) => return Ok(n),
                Err(e) if is_fatal(e) => return Err(e),
                Err(e) => e,
            };
            slog!(
                bus,
                self.slot,
                "{}: {e} (try {tries} of {MAX_TRIES})",
                Named(cdb)
            );
            let illegal = matches!(e, UsbError::Sense(s) if s.key == ILLEGAL_REQUEST);
            if illegal || tries >= MAX_TRIES {
                return Err(e);
            }
            tries += 1;
        }
    }

    /// CBW, data phase, CSW (BOT 5): the bytes moved and the CSW. A STALL
    /// ends the data phase early (BOT 6.7.2, 6.7.3): the halt is cleared
    /// and the CSW read as usual.
    fn exchange(
````

Replace:

````rust
        data: Data,
    ) -> Result<(usize, CswStatus), UsbError> {
        let tag = self.next_tag;
````

with:

````rust
        data: Data,
    ) -> Result<(usize, Csw), UsbError> {
        let tag = self.next_tag;
````

Replace:

````rust
        }
        let moved = match data {
            Data::None => 0,
            Data::In(buf) => bus.bulk_in(self.slot, self.bulk_in, buf)?,
        };
        let mut raw = [0u8; CSW_LEN];
        let n = bus.bulk_in(self.slot, self.bulk_in, &mut raw)?;
        let csw = Csw::parse(&raw[..n.min(CSW_LEN)], tag, length)?;
        Ok((moved, csw.status))
    }
````

with:

````rust
        }
        let (moved, endpoint) = match data {
            Data::None => (Ok(0), self.bulk_in),
            Data::In(buf) => (bus.bulk_in(self.slot, self.bulk_in, buf), self.bulk_in),
            Data::Out(buf) => (bus.bulk_out(self.slot, self.bulk_out, buf), self.bulk_out),
        };
        let moved = match moved {
            Err(UsbError::Stall) => {
                slog!(bus, self.slot, "{}: data phase stalled", Named(cdb));
                bus.clear_halt(self.slot, endpoint)?;
                0
            }
            moved => moved?,
        };
        let csw = self.read_csw(bus, cdb, tag, length)?;
        Ok((moved, csw))
    }

    /// Reads and checks the CSW. A STALL is cleared and the CSW read once
    /// more (BOT 5.3.3, figure 2).
    fn read_csw(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        tag: u32,
        length: u32,
    ) -> Result<Csw, UsbError> {
        let mut raw = [0u8; CSW_LEN];
        let n = match bus.bulk_in(self.slot, self.bulk_in, &mut raw) {
            Err(UsbError::Stall) => {
                slog!(bus, self.slot, "{}: CSW stalled", Named(cdb));
                bus.clear_halt(self.slot, self.bulk_in)?;
                bus.bulk_in(self.slot, self.bulk_in, &mut raw)?
            }
            n => n?,
        };
        Csw::parse(&raw[..n.min(CSW_LEN)], tag, length)
    }
````

Replace:

````rust
    fn request_sense(&mut self, bus: &mut dyn Bus) -> UsbError {
        let name = scsi::command_name(scsi::REQUEST_SENSE);
        let mut buf = [0u8; SENSE_LEN];
        let cdb = scsi::request_sense(SENSE_LEN as u8);
        match self.exchange(bus, &cdb, Data::In(&mut buf)) {
            Ok((n, CswStatus::Passed)) => match Sense::parse(&buf[..n.min(SENSE_LEN)]) {
                Ok(sense) => UsbError::Sense(sense),
                Err(e) => e,
            },
            Ok((_, CswStatus::Failed)) => UsbError::Protocol("REQUEST SENSE failed"),
            Ok((_, CswStatus::PhaseError)) => {
                self.recover(bus, name, UsbError::Protocol("phase error"))
            }
            Err(e) => self.recover(bus, name, e),
        }
    }

    /// Reset recovery after `cause` in command `name` (BOT 5.3.4):
    /// Bulk-Only Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT) of
    /// bulk IN and bulk OUT. Returns `cause`, or the error that showed the
    /// device or controller gone (then nothing more is sent).
    fn recover(&mut self, bus: &mut dyn Bus, name: &str, cause: UsbError) -> UsbError {
        if is_fatal(cause) {
            return cause;
        }
        slog!(bus, self.slot, "{name}: {cause}; reset recovery");
        let reset = Setup {
````

with:

````rust
    fn request_sense(&mut self, bus: &mut dyn Bus) -> UsbError {
        let mut buf = [0u8; SENSE_LEN];
        let cdb = scsi::request_sense(SENSE_LEN as u8);
        match self.exchange(bus, &cdb, Data::In(&mut buf)) {
            Ok((n, csw)) => match csw.status {
                CswStatus::Passed => match Sense::parse(&buf[..n.min(SENSE_LEN)]) {
                    Ok(sense) => UsbError::Sense(sense),
                    Err(e) => e,
                },
                CswStatus::Failed => UsbError::Protocol("REQUEST SENSE failed"),
                CswStatus::PhaseError => self.recover(bus, &cdb, UsbError::Protocol("phase error")),
            },
            Err(e) => self.recover(bus, &cdb, e),
        }
    }

    /// Reset recovery after `cause` in command `cdb` (BOT 5.3.4):
    /// Bulk-Only Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT) of
    /// bulk IN and bulk OUT. Returns `cause`, or the error that showed the
    /// device or controller gone (then nothing more is sent).
    fn recover(&mut self, bus: &mut dyn Bus, cdb: &[u8], cause: UsbError) -> UsbError {
        if is_fatal(cause) {
            return cause;
        }
        slog!(bus, self.slot, "{}: {cause}; reset recovery", Named(cdb));
        let reset = Setup {
````

- [ ] **Step 14: Change `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, replace:

````rust

    fn connected(&self, port: u8) -> bool {
        self.regs.portsc(&self.hal, port) & CCS != 0
````

with:

````rust

    pub(super) fn connected(&self, port: u8) -> bool {
        self.regs.portsc(&self.hal, port) & CCS != 0
````

- [ ] **Step 15: Change `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    /// The endpoint's state as the controller last wrote it.
````

with:

````rust

    /// `e` for a transfer to `slot`, or `Disconnected` if the slot's port
    /// no longer shows a connection: a transfer to a device that was
    /// unplugged fails (USB Transaction Error) or times out, and the class
    /// driver must not take that for a fault to recover from.
    fn gone_or(&self, slot: u8, e: UsbError) -> UsbError {
        match self.slots.get(slot as usize).and_then(Option::as_ref) {
            Some(s) if e != UsbError::ControllerDead && !self.connected(s.port) => {
                UsbError::Disconnected
            }
            _ => e,
        }
    }

    /// The endpoint's state as the controller last wrote it.
````

Replace:

````rust
        }
        let n = self.bulk_transfer(slot, endpoint, buf.len(), None)?;
        let n = n.min(buf.len());
````

with:

````rust
        }
        let n = self
            .bulk_transfer(slot, endpoint, buf.len(), None)
            .map_err(|e| self.gone_or(slot, e))?;
        let n = n.min(buf.len());
````

Replace:

````rust
        self.bulk_transfer(slot, endpoint, data.len(), Some(data))
    }
````

with:

````rust
        self.bulk_transfer(slot, endpoint, data.len(), Some(data))
            .map_err(|e| self.gone_or(slot, e))
    }
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 309 tests.

- [ ] **Step 17: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 18: Commit**

````bash
git add crates
git commit -m "usb: mass storage reads, writes and error recovery"
````


### Task 7: The host sets up disks and tries a failed setup again

The storage driver joins `Host` next to the keyboards (roadmap: "What plan 4 leaves for plan 5"). `Host::claim` claims mass-storage and boot-keyboard interfaces together and configures them in one Configure Endpoint. Each disk gets a `DiskId` that is never reused, so a request for a stick that was unplugged, or unplugged and plugged in again, fails with `Disconnected` instead of reaching another device (decision 3); `disks`, `read`, `write` and `flush` are what the kernel's block device uses. The boot line names what was found, because a photo of it is how the NUC is debugged: `port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB`, or `disk not started: <reason>` for a disk whose setup failed; sizes are whole MiB under 1 GiB and GiB with one decimal above, rounded down. Plan 4's deferred finding M4: a device whose setup fails is tried again, up to three times in all, each try resetting the port afresh, unless it is gone or the controller died; each failed try is logged, and the boot line shows only the outcome (decision 4). The boot-line types move to `host/boot_line.rs`.

**Files:**
- Modify: `crates/usb/src/host.rs`
- Create: `crates/usb/src/host/boot_line.rs`
- Modify: `crates/usb/src/testing/storage/bot.rs`
- Modify: `crates/usb/src/testing/storage/mod.rs`
- Modify: `tests/e2e/boot.txt`

**Interfaces:**
- Consumes: Tasks 1–6; plan 4's `Host`, `Attached`, `Found`.
- Produces: `usb::host::{DiskId(pub u32), DiskInfo { id, port, vendor, product, block_size, block_count }, ATTACH_TRIES = 3}`; `Host::{disks(&self) -> Vec<DiskInfo>, read(&mut self, DiskId, lba: u64, buf: &mut [u8]) -> Result<(), UsbError>, write(&mut self, DiskId, lba: u64, buf: &[u8]) -> Result<(), UsbError>, flush(&mut self, DiskId) -> Result<(), UsbError>}`; `Found::{disks, disk_info, disk_not_started}` (no longer `Copy`).

- [ ] **Step 1: Add the failing tests to `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    use crate::hid::Key;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::string::{String, ToString};
    use core::time::Duration;
````

with:

````rust
    use crate::hid::Key;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, FakeUsbDevice, op, start};
    use alloc::string::{String, ToString};
    use alloc::vec;
    use core::time::Duration;
````

Replace:

````rust

    #[test]
    fn devices_present_at_start_are_set_up_and_reported() {
        // Laid out as QEMU's e2e machine: USB 3 ports first.
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.fake().plug(2, FakeUsbDevice::kingston_stick());
        hal.fake().plug(5, FakeUsbDevice::qemu_keyboard());
````

with:

````rust

    /// QEMU's e2e disk: 256 MiB.
    const QEMU_BLOCKS: u64 = 524_288;

    #[test]
    fn devices_present_at_start_are_set_up_and_reported() {
        // Laid out as QEMU's e2e machine: USB 3 ports first.
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.fake().plug(5, FakeUsbDevice::qemu_keyboard());
````

Replace:

````rust
            [
                "port 2: 0951:1666 SuperSpeed, not claimed",
                "port 5: 0627:0001 high-speed, keyboard",
            ]
        );
        assert_eq!(host.keyboards(), 1);
        // Nothing changed since: nothing more to do.
        assert!(host.service().is_empty());
    }
````

with:

````rust
            [
                "port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB",
                "port 5: 0627:0001 high-speed, keyboard",
            ]
        );
        assert_eq!(host.keyboards(), 1);
        assert_eq!(
            host.disks(),
            [DiskInfo {
                id: DiskId(0),
                port: 2,
                vendor: "QEMU".into(),
                product: "QEMU HARDDISK".into(),
                block_size: 512,
                block_count: QEMU_BLOCKS,
            }]
        );
        // Nothing changed since: nothing more to do.
        assert!(host.service().is_empty());
    }

    #[test]
    fn the_nuc_stick_is_a_disk_of_14_4_gib() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(15, FakeStorage::kingston());
        hal.sleep(Duration::from_millis(60));
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 15: 0951:1666 SuperSpeed, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!(attached[0].outcome.as_ref().map(|f| f.disks), Ok(1));
    }

    #[test]
    fn reads_and_writes_reach_the_disk() {
        let (hal, mut host) = host(FakeConfig::qemu());
        let stick = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, stick.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let id = host.disks()[0].id;
        let data: Vec<u8> = (0..4096).map(|i| (i % 253) as u8).collect();
        host.write(id, 2048, &data).unwrap();
        assert_eq!(stick.borrow().read_blocks(2048, 8), data);
        let mut buf = vec![0u8; 4096];
        host.read(id, 2048, &mut buf).unwrap();
        assert_eq!(buf, data);
        host.flush(id).unwrap();
        assert_eq!(
            stick.borrow().opcodes().last(),
            Some(&op::SYNCHRONIZE_CACHE_10)
        );
        assert_eq!(
            host.read(DiskId(7), 0, &mut buf),
            Err(UsbError::Disconnected)
        );
    }

    #[test]
    fn an_unplugged_disk_is_gone_for_good_and_a_replug_gets_a_new_id() {
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.sleep(Duration::from_millis(60));
        assert!(host.service().is_empty());
        let before = hal.outstanding_dma();
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.sleep(Duration::from_millis(60));
        host.service();
        let old = host.disks()[0].id;
        hal.fake().unplug(2);
        assert!(host.service().is_empty());
        assert!(host.disks().is_empty());
        assert_eq!(hal.outstanding_dma(), before, "the stick's memory is freed");
        let mut buf = vec![0u8; 512];
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Disconnected));
        assert_eq!(host.write(old, 0, &buf), Err(UsbError::Disconnected));
        assert_eq!(host.flush(old), Err(UsbError::Disconnected));
        // Plugged in again: another id, which works; the old one does not.
        hal.fake().plug(2, FakeStorage::qemu(QEMU_BLOCKS));
        hal.sleep(Duration::from_millis(60));
        host.service();
        let new = host.disks()[0].id;
        assert!(new > old);
        assert_eq!(host.read(new, 0, &mut buf), Ok(()));
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        let stick = FakeStorage::usb2(1 << 20);
        stick.borrow_mut().no_medium();
        hal.fake().plug(3, stick.clone());
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 3: 0951:1665 high-speed, disk not started: NOT READY (asc 0x3a, ascq 0x00)"
        );
        assert!(host.disks().is_empty());
        let inquiries = stick
            .borrow()
            .opcodes()
            .iter()
            .filter(|&&o| o == op::INQUIRY)
            .count();
        assert_eq!(inquiries, 1);
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: slot 1 interface 0: disk not started: NOT READY")
        );
        assert!(!hal.log_text().contains("trying again"));
    }

    #[test]
    fn a_device_with_a_keyboard_and_a_disk_has_both() {
        let (hal, mut host) = host(FakeConfig::intel());
        let stick = FakeStorage::kingston();
        stick.borrow_mut().add_boot_keyboard();
        hal.fake().plug(13, stick.clone());
        hal.sleep(Duration::from_millis(60));
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 13: 0951:1666 SuperSpeed, keyboard, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!((host.keyboards(), host.disks().len()), (1, 1));
        // One Configure Endpoint for both interfaces.
        let configures = hal
            .fake()
            .executed()
            .iter()
            .filter(|e| e.kind == 12)
            .count();
        assert_eq!(configures, 1);
        let slot = host.xhci().slot_of_port(13).unwrap() as usize;
        for dci in [3, 4, 7] {
            assert!(hal.fake().endpoint(slot, dci).is_some(), "DCI {dci}");
        }
        stick.borrow_mut().usb().push_in(0x83, &key_report(0x0B));
        stick.borrow_mut().usb().push_in(0x83, &key_report(0));
        assert_eq!(typed(&hal, &mut host, 100), "h");
    }

    #[test]
    fn a_device_whose_first_setup_fails_is_set_up_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        // The first GET_DESCRIPTOR is never answered.
        k120.borrow_mut().ignore_requests(1);
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(
            attached[0].to_string(),
            "port 3: 046d:c31c low-speed, keyboard"
        );
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: setup failed: timed out, trying again")
        );
        assert_eq!(host.keyboards(), 1);
    }

    #[test]
    fn a_device_that_always_fails_is_tried_three_times_and_reported_once() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x06, 0x0100);
        hal.fake().plug(3, k120.clone());
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].to_string(), "port 3: setup failed: stalled");
        let log = hal.log_text();
        assert_eq!(
            log.matches("port 3: setup failed: stalled, trying again")
                .count(),
            2
        );
        let device_descriptors = k120
            .borrow()
            .requests()
            .iter()
            .filter(|r| r.setup.request == 0x06 && r.setup.value == 0x0100)
            .count();
        assert_eq!(device_descriptors, 3);
    }

    #[test]
    fn a_setup_that_kills_the_controller_is_not_tried_again() {
        let mut config = FakeConfig::intel();
        // Configure Endpoint hangs and so does its abort: the controller is
        // given up.
        config.hang_command = Some(12);
        config.abort_never_completes = true;
        let (hal, mut host) = host(config);
        hal.fake().plug(3, FakeUsbDevice::k120());
        let attached = host.service();
        assert_eq!(attached[0].outcome, Err(UsbError::ControllerDead));
        let log = hal.log_text();
        assert_eq!(log.matches("trying again").count(), 1, "{log}");
        assert!(log.contains("port 3: setup failed: timed out, trying again"));
    }

    #[test]
    fn a_device_that_is_unplugged_during_setup_is_not_tried_again() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        // After the debounce, during the port reset.
        hal.fake()
            .after(Duration::from_millis(105), |x, _| x.unplug(3));
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].outcome, Err(UsbError::Disconnected));
        assert!(!hal.log_text().contains("trying again"));
    }
````

- [ ] **Step 2: Write the failing tests for `crates/usb/src/host/boot_line.rs`**

Create `crates/usb/src/host/boot_line.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn found() -> Found {
        Found {
            vendor: 0x0951,
            product: 0x1666,
            speed: Speed::Super,
            keyboards: 0,
            not_started: None,
            disks: 0,
            disk_info: Vec::new(),
            disk_not_started: None,
        }
    }

    fn disk(vendor: &str, product: &str, block_size: usize, block_count: u64) -> DiskInfo {
        DiskInfo {
            id: DiskId(0),
            port: 1,
            vendor: vendor.into(),
            product: product.into(),
            block_size,
            block_count,
        }
    }

    fn line(found: Found) -> String {
        Attached {
            port: 1,
            outcome: Ok(found),
        }
        .to_string()
    }

    #[test]
    fn boot_lines_name_what_was_found() {
        assert_eq!(line(found()), "port 1: 0951:1666 SuperSpeed, not claimed");
        let one = |f: Found| {
            line(f)
                .trim_start_matches("port 1: 0951:1666 SuperSpeed, ")
                .to_string()
        };
        let kingston = disk("Kingston", "DataTraveler 3.0", 512, 30_277_632);
        let with_disk = Found {
            disks: 1,
            disk_info: vec![kingston.clone()],
            ..found()
        };
        assert_eq!(
            one(with_disk.clone()),
            "disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        let gone = Some(UsbError::Timeout);
        assert_eq!(
            one(Found {
                disk_not_started: gone,
                ..found()
            }),
            "disk not started: timed out"
        );
        assert_eq!(
            one(Found {
                disk_not_started: gone,
                ..with_disk.clone()
            }),
            "disk Kingston DataTraveler 3.0, 14.4 GiB, another disk not started: timed out"
        );
        assert_eq!(
            one(Found {
                keyboards: 2,
                not_started: gone,
                ..with_disk.clone()
            }),
            "2 keyboards, another not started: timed out, disk Kingston DataTraveler 3.0, 14.4 GiB"
        );
        assert_eq!(
            one(Found {
                not_started: gone,
                disk_not_started: Some(UsbError::Stall),
                ..found()
            }),
            "keyboard not started: timed out, disk not started: stalled"
        );
        // A disk without names.
        assert_eq!(
            one(Found {
                disks: 1,
                disk_info: vec![disk("", "", 512, 2048)],
                ..found()
            }),
            "disk, 1 MiB"
        );
        assert_eq!(
            one(Found {
                disks: 1,
                disk_info: vec![disk("", "Stick", 512, 2048)],
                ..found()
            }),
            "disk Stick, 1 MiB"
        );
    }

    #[test]
    fn disk_sizes_are_whole_mib_then_gib_with_one_decimal_rounded_down() {
        let size = |block_size, blocks| {
            let f = Found {
                disks: 1,
                disk_info: vec![disk("V", "P", block_size, blocks)],
                ..found()
            };
            line(f).rsplit(", ").next().unwrap().to_string()
        };
        assert_eq!(size(512, 524_288), "256 MiB");
        assert_eq!(size(512, 2047), "0 MiB");
        assert_eq!(size(512, 2 * 1024 * 1024 - 1), "1023 MiB");
        assert_eq!(size(512, 2 * 1024 * 1024), "1.0 GiB");
        assert_eq!(size(4096, 262_144 + 26_214), "1.0 GiB");
        assert_eq!(size(4096, 262_144 + 26_215), "1.1 GiB");
        assert_eq!(size(512, 30_277_632), "14.4 GiB");
        // 512 bytes under 200 GiB (integer arithmetic, no rounding up).
        assert_eq!(size(512, 419_430_399), "199.9 GiB");
        // The largest disk: 2^32 blocks of 4096 bytes, 16 TiB.
        assert_eq!(size(4096, 1 << 32), "16384.0 GiB");
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/storage/bot.rs`**

In `crates/usb/src/testing/storage/bot.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::testing::device::Stall;

````

with:

````rust
use crate::testing::device::Stall;

/// `add_boot_keyboard`'s interface and endpoint.
const KEYBOARD_INTERFACE: u16 = 1;
const KEYBOARD_IN: u8 = 0x83;

````

Replace:

````rust
        if setup.request_type & 0x60 == 0x20 {
            if setup.request_type & 0x1F != 1 || setup.index != INTERFACE || setup.value != 0 {
````

with:

````rust
        if setup.request_type & 0x60 == 0x20 {
            if self.keyboard && setup.index == KEYBOARD_INTERFACE {
                return Some(answer);
            }
            if setup.request_type & 0x1F != 1 || setup.index != INTERFACE || setup.value != 0 {
````

Replace:

````rust
    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
        if endpoint != BULK_IN {
````

with:

````rust
    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
        if self.keyboard && endpoint == KEYBOARD_IN {
            return self.usb.data_in(endpoint, max_len);
        }
        if endpoint != BULK_IN {
````

- [ ] **Step 4: Extend the test support in `crates/usb/src/testing/storage/mod.rs`**

In `crates/usb/src/testing/storage/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    no_cache_sync: bool,
}
````

with:

````rust
    no_cache_sync: bool,
    /// Knob: interface 1 is a boot keyboard, whose requests and reports
    /// the `FakeUsbDevice` handles.
    keyboard: bool,
}
````

Replace:

````rust
            no_cache_sync: false,
        }))
````

with:

````rust
            no_cache_sync: false,
            keyboard: false,
        }))
````

Replace:

````rust
            FailedData::Pad,
        )
    }

````

with:

````rust
            FailedData::Pad,
        )
    }

    /// A USB 2 stick (`FakeUsbDevice::usb2_stick`: high-speed, `0951:1665`)
    /// with the Kingston's answers, named DataTraveler 2.0, and `blocks`
    /// blocks of 512 bytes. A failed command stalls its data phase.
    pub fn usb2(blocks: u64) -> Rc<RefCell<FakeStorage>> {
        let mut inquiry = KINGSTON_INQUIRY.to_vec();
        inquiry[16..32].copy_from_slice(b"DataTraveler 2.0");
        FakeStorage::new(
            FakeUsbDevice::usb2_stick(),
            inquiry,
            blocks,
            FailedData::Stall,
        )
    }

    /// Knob: the device also has a boot keyboard, interface 1 with an
    /// interrupt IN endpoint 0x83 of 8 bytes (a composite device). Reports
    /// for it are pushed with `usb().push_in(0x83, …)`.
    pub fn add_boot_keyboard(&mut self) {
        let mut config = self.usb.configuration_descriptor().to_vec();
        config.extend_from_slice(&[
            9, 4, 1, 0, 1, 3, 1, 1, 0, // interface 1: HID boot keyboard
            9, 0x21, 0x11, 1, 0, 1, 0x22, 63, 0, // HID descriptor
            7, 5, 0x83, 3, 8, 0, 7, // interrupt IN 0x83, 8 bytes
        ]);
        let total = (config.len() as u16).to_le_bytes();
        (config[2], config[3], config[4]) = (total[0], total[1], 2);
        let device = self.usb.device_descriptor().to_vec();
        self.usb.set_descriptors(device, config);
        self.keyboard = true;
    }

````

- [ ] **Step 5: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
expect usb: 00:02\.0 xHCI 1\.00, 8 ports \(4 USB 2, 4 USB 3\), 32-byte contexts, 0 scratchpads
expect usb: 00:02\.0 port 2: 46f4:0001 SuperSpeed, not claimed
expect usb: 00:02\.0 port 5: 0627:0001 high-speed, keyboard
````

with:

````text
expect usb: 00:02\.0 xHCI 1\.00, 8 ports \(4 USB 2, 4 USB 3\), 32-byte contexts, 0 scratchpads
expect usb: 00:02\.0 port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB
expect usb: 00:02\.0 port 5: 0627:0001 high-speed, keyboard
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `DiskInfo` in this scope ``; `` cannot find function, tuple struct or tuple variant `DiskId` in this scope ``.

- [ ] **Step 7: Change `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
//! when they appear on a root port (at start and when plugged in later),
//! boot-keyboard interfaces go to the keyboard driver, and a device's
//! drivers are dropped when it goes away. The kernel keeps one `Host` per
//! xHCI controller and polls it from the console.

use crate::hid::{BootKeyboard, KeyEvent, is_boot_keyboard};
use crate::xhci::{Device, Xhci};
use crate::{Hal, Speed, UsbError};
use alloc::collections::VecDeque;
````

with:

````rust
//! when they appear on a root port (at start and when plugged in later),
//! boot-keyboard interfaces go to the keyboard driver and mass-storage
//! interfaces to the storage driver, and a device's drivers are dropped when
//! it goes away. A setup that fails is tried again, three times in all. The
//! kernel keeps one `Host` per xHCI controller and polls it from the
//! console; disks are named by a [`DiskId`] that is never reused.

use crate::hid::{BootKeyboard, KeyEvent, is_boot_keyboard};
use crate::storage::{MassStorage, is_mass_storage};
use crate::xhci::{Device, Xhci};
use crate::{Hal, UsbError};
use alloc::collections::VecDeque;
````

Replace:

````rust
pub const MAX_EVENTS: usize = 256;

/// What happened when a device was set up, for the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    pub port: u8,
    pub outcome: Result<Found, UsbError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub vendor: u16,
    pub product: u16,
    pub speed: Speed,
    /// Boot-keyboard interfaces now in use.
    pub keyboards: usize,
    /// Why a boot-keyboard interface could not be started, if one could
    /// not: the screen must say so, because `dmesg` needs a keyboard.
    pub not_started: Option<UsbError>,
}

impl fmt::Display for Attached {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "port {}: ", self.port)?;
        match &self.outcome {
            Err(e) => write!(f, "setup failed: {e}"),
            Ok(d) => {
                write!(f, "{:04x}:{:04x} {}, ", d.vendor, d.product, d.speed)?;
                match (d.keyboards, d.not_started) {
                    (0, None) => write!(f, "not claimed"),
                    (0, Some(e)) => write!(f, "keyboard not started: {e}"),
                    (1, None) => write!(f, "keyboard"),
                    (n, None) => write!(f, "{n} keyboards"),
                    (n, Some(e)) => write!(f, "{n} keyboards, another not started: {e}"),
                }
            }
        }
    }
}
````

with:

````rust
pub const MAX_EVENTS: usize = 256;
/// Setups of one device: the first and two more (plan 4's finding M4).
pub const ATTACH_TRIES: u32 = 3;

mod boot_line;

pub use boot_line::{Attached, DiskId, DiskInfo, Found};

/// A disk in use: its id, the port of its device and the driver.
struct Disk {
    id: DiskId,
    port: u8,
    storage: MassStorage,
}
````

Replace:

````rust
    events: VecDeque<KeyEvent>,
}
````

with:

````rust
    events: VecDeque<KeyEvent>,
    disks: Vec<Disk>,
    /// The id the next disk gets.
    next_disk: u32,
}
````

Replace:

````rust
            events: VecDeque::new(),
        }
````

with:

````rust
            events: VecDeque::new(),
            disks: Vec::new(),
            next_disk: 0,
        }
````

Replace:

````rust

    /// The oldest key event not yet taken.
````

with:

````rust

    /// The disks in use, in the order they were set up.
    pub fn disks(&self) -> Vec<DiskInfo> {
        self.disks.iter().map(Disk::info).collect()
    }

    /// Reads whole blocks of disk `disk` from `lba` (as
    /// `MassStorage::read`). `Err(UsbError::Disconnected)` for a disk that
    /// is gone.
    pub fn read(&mut self, disk: DiskId, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.read(&mut self.xhci, lba, buf)
    }

    /// Writes whole blocks to disk `disk` from `lba`, as `read`.
    pub fn write(&mut self, disk: DiskId, lba: u64, buf: &[u8]) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.write(&mut self.xhci, lba, buf)
    }

    /// Flushes disk `disk`'s cache, as `MassStorage::flush`.
    pub fn flush(&mut self, disk: DiskId) -> Result<(), UsbError> {
        storage(&mut self.disks, disk)?.flush(&mut self.xhci)
    }

    /// The oldest key event not yet taken.
````

Replace:

````rust

    fn attach(&mut self, port: u8) -> Attached {
        let outcome = self.xhci.attach(port).and_then(|d| self.claim(d));
        if let Err(e) = &outcome {
````

with:

````rust

    /// Sets up the device on `port`, [`ATTACH_TRIES`] times at most (each
    /// try resets the port afresh), unless it is gone or the controller
    /// died. Only the final outcome is reported.
    fn attach(&mut self, port: u8) -> Attached {
        let mut tries = 1;
        let outcome = loop {
            match self.xhci.attach(port).and_then(|d| self.claim(d)) {
                Err(e)
                    if tries < ATTACH_TRIES
                        && !matches!(e, UsbError::Disconnected | UsbError::ControllerDead) =>
                {
                    self.log(format_args!("port {port}: setup failed: {e}, trying again"));
                    tries += 1;
                }
                outcome => break outcome,
            }
        };
        if let Err(e) = &outcome {
````

Replace:

````rust

    /// Configures the device for the interfaces a driver wants and starts
    /// the drivers. A device nothing claims stays addressed but unused.
    fn claim(&mut self, d: Device) -> Result<Found, UsbError> {
````

with:

````rust

    /// Configures the device for the interfaces a driver wants (all in one
    /// Configure Endpoint) and starts the drivers. A device nothing claims
    /// stays addressed but unused. A driver that does not start is shown on
    /// the boot line; it does not fail the device's setup.
    fn claim(&mut self, d: Device) -> Result<Found, UsbError> {
````

Replace:

````rust
            .iter()
            .filter(|i| is_boot_keyboard(i))
            .map(|i| i.number)
````

with:

````rust
            .iter()
            .filter(|i| is_boot_keyboard(i) || is_mass_storage(i))
            .map(|i| i.number)
````

Replace:

````rust
            not_started: None,
        };
````

with:

````rust
            not_started: None,
            disks: 0,
            disk_info: Vec::new(),
            disk_not_started: None,
        };
````

Replace:

````rust
        {
            match BootKeyboard::start(&mut self.xhci, d.slot, iface) {
````

with:

````rust
        {
            if is_mass_storage(iface) {
                match MassStorage::start(&mut self.xhci, d.slot, iface) {
                    Ok(storage) => {
                        let disk = Disk {
                            id: DiskId(self.next_disk),
                            port: d.port,
                            storage,
                        };
                        self.next_disk = self.next_disk.wrapping_add(1);
                        found.disk_info.push(disk.info());
                        found.disks += 1;
                        self.disks.push(disk);
                    }
                    Err(e) => {
                        self.log(format_args!(
                            "slot {} interface {}: disk not started: {e}",
                            d.slot, iface.number
                        ));
                        found.disk_not_started = Some(e);
                    }
                }
                continue;
            }
            match BootKeyboard::start(&mut self.xhci, d.slot, iface) {
````

Replace:

````rust
    /// The device in `slot` is gone: its keyboards stop (a held key stops
    /// repeating with them) and the controller forgets it.
    fn drop_device(&mut self, slot: u8) {
        self.keyboards.retain(|k| k.slot() != slot);
        self.xhci.detach(slot);
    }
````

with:

````rust
    /// The device in `slot` is gone: its keyboards stop (a held key stops
    /// repeating with them), its disks go (their ids fail from now on) and
    /// the controller forgets it.
    fn drop_device(&mut self, slot: u8) {
        self.keyboards.retain(|k| k.slot() != slot);
        self.disks.retain(|d| d.storage.slot() != slot);
        self.xhci.detach(slot);
    }
}

/// The driver of disk `id`, if its device is still there.
fn storage(disks: &mut [Disk], id: DiskId) -> Result<&mut MassStorage, UsbError> {
    let disk = disks.iter_mut().find(|d| d.id == id);
    disk.map(|d| &mut d.storage).ok_or(UsbError::Disconnected)
}

impl Disk {
    fn info(&self) -> DiskInfo {
        DiskInfo {
            id: self.id,
            port: self.port,
            vendor: self.storage.vendor().into(),
            product: self.storage.product().into(),
            block_size: self.storage.block_size(),
            block_count: self.storage.block_count(),
        }
    }
````

- [ ] **Step 8: Implement `crates/usb/src/host/boot_line.rs`**

Insert this at the top of `crates/usb/src/host/boot_line.rs`, above `#[cfg(test)]`:

````rust
//! The line the startup screen shows for each device set up (spec §10:
//! the boot keeps going when a device fails, and says so): what was found
//! on it, keyboards and disks, or why a driver did not start. Also the
//! names the block layer gets for disks.

use crate::{Speed, UsbError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// What happened when a device was set up, for the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attached {
    pub port: u8,
    pub outcome: Result<Found, UsbError>,
}

/// Names one disk on one host; never reused, so a request for a stick that
/// was unplugged (and maybe plugged in again) fails instead of reaching
/// another device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskId(pub u32);

/// A disk in use, for the block layer and the startup screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskInfo {
    pub id: DiskId,
    pub port: u8,
    pub vendor: String,
    pub product: String,
    pub block_size: usize,
    pub block_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub vendor: u16,
    pub product: u16,
    pub speed: Speed,
    /// Boot-keyboard interfaces now in use.
    pub keyboards: usize,
    /// Why a boot-keyboard interface could not be started, if one could
    /// not: the screen must say so, because `dmesg` needs a keyboard.
    pub not_started: Option<UsbError>,
    /// Mass-storage interfaces now in use: `disk_info` has one entry for
    /// each.
    pub disks: usize,
    pub disk_info: Vec<DiskInfo>,
    /// Why a mass-storage interface could not be started, if one could not.
    pub disk_not_started: Option<UsbError>,
}

/// The boot line's parts after the speed, joined with ", ".
struct Parts<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    any: bool,
}

impl Parts<'_, '_> {
    fn add(&mut self, args: fmt::Arguments) -> fmt::Result {
        let sep = if self.any { ", " } else { "" };
        self.any = true;
        write!(self.f, "{sep}{args}")
    }
}

/// A disk's size: whole MiB under 1 GiB, else GiB with one decimal, both
/// rounded down.
struct Size(u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const MIB: u64 = 1 << 20;
        const GIB: u64 = 1 << 30;
        if self.0 < GIB {
            write!(f, "{} MiB", self.0 / MIB)
        } else {
            let tenths = self.0.saturating_mul(10) / GIB;
            write!(f, "{}.{} GiB", tenths / 10, tenths % 10)
        }
    }
}

impl fmt::Display for DiskInfo {
    /// "disk Kingston DataTraveler 3.0, 14.4 GiB".
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("disk")?;
        for name in [&self.vendor, &self.product] {
            if !name.is_empty() {
                write!(f, " {name}")?;
            }
        }
        let bytes = self.block_count.saturating_mul(self.block_size as u64);
        write!(f, ", {}", Size(bytes))
    }
}

impl fmt::Display for Attached {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "port {}: ", self.port)?;
        let d = match &self.outcome {
            Err(e) => return write!(f, "setup failed: {e}"),
            Ok(d) => d,
        };
        write!(f, "{:04x}:{:04x} {}, ", d.vendor, d.product, d.speed)?;
        let mut parts = Parts { f, any: false };
        match (d.keyboards, d.not_started) {
            (0, None) => {}
            (0, Some(e)) => parts.add(format_args!("keyboard not started: {e}"))?,
            (1, None) => parts.add(format_args!("keyboard"))?,
            (n, None) => parts.add(format_args!("{n} keyboards"))?,
            (n, Some(e)) => parts.add(format_args!("{n} keyboards, another not started: {e}"))?,
        }
        for disk in &d.disk_info {
            parts.add(format_args!("{disk}"))?;
        }
        match (d.disks, d.disk_not_started) {
            (_, None) => {}
            (0, Some(e)) => parts.add(format_args!("disk not started: {e}"))?,
            (_, Some(e)) => parts.add(format_args!("another disk not started: {e}"))?,
        }
        if !parts.any {
            parts.add(format_args!("not claimed"))?;
        }
        Ok(())
    }
}

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 320 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates tests
git commit -m "usb: the host sets up disks and tries a failed setup again"
````


### Task 8: A transfer never reaches a device plugged in after the one it was for

Review finding (minor): the fake controller sent a slot's transfers to whatever device was on its port, so after an unplug and a replug between two looks a request through the old disk reached the new stick and succeeded. A real controller addresses the device, and a newly plugged one answers only address 0 until Address Device, so the request fails. The fake now keeps each device's USB address and fails the transfers of a slot whose address the device on the port does not have with a USB Transaction Error; a host test pins that a request through the old `DiskId` fails, the new stick sees nothing, and after `service` the new stick is a new disk that works (decision 3). The test comes first and does not compile until the fake can say which address a device has; the stricter fake is this task's implementation (without its address check, the test fails because the old stick's request reaches the new one).

**Files:**
- Modify: `crates/usb/src/host.rs`
- Modify: `crates/usb/src/testing/device.rs`
- Modify: `crates/usb/src/testing/storage/bot.rs`
- Modify: `crates/usb/src/testing/xhci/ports.rs`
- Modify: `crates/usb/src/testing/xhci/slots.rs`
- Modify: `crates/usb/src/testing/xhci/transfers.rs`

**Interfaces:**
- Consumes: Tasks 1–7.
- Produces: the fake xHCI's per-device USB address; no production code changes unless the test finds one.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, replace:

````rust
    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
````

with:

````rust
    #[test]
    fn a_request_for_an_unplugged_disk_never_reaches_the_stick_plugged_in_after_it() {
        // Another stick, or the same one again: either is a new device.
        for same in [false, true] {
            let (hal, mut host) = host(FakeConfig::qemu());
            let a = FakeStorage::qemu(QEMU_BLOCKS);
            hal.fake().plug(2, a.clone());
            hal.sleep(Duration::from_millis(60));
            host.service();
            let old = host.disks()[0].id;
            // Stick A out and B in before the console looks again.
            hal.fake().unplug(2);
            let b = if same {
                a.clone()
            } else {
                FakeStorage::qemu(QEMU_BLOCKS)
            };
            let seen = b.borrow().events().len();
            hal.fake().plug(2, b.clone());
            hal.sleep(Duration::from_millis(60));
            let data = vec![0xAB; 512];
            // B has no address yet: A's slot reaches nothing.
            assert!(host.write(old, 100, &data).is_err());
            assert!(host.flush(old).is_err());
            assert_eq!(b.borrow().events().len(), seen, "B saw A's requests");
            assert_eq!(b.borrow().read_blocks(100, 1), [0; 512]);
            // The next look replaces A with B, under a new id.
            assert_eq!(host.service().len(), 1);
            let disks = host.disks();
            assert_eq!(disks.len(), 1);
            let new = disks[0].id;
            assert_ne!(new, old);
            assert_eq!(host.write(new, 100, &data), Ok(()));
            assert_eq!(b.borrow().read_blocks(100, 1), data);
            assert_eq!(host.write(old, 100, &data), Err(UsbError::Disconnected));
        }
    }

    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
````

- [ ] **Step 2: Add the failing tests to `crates/usb/src/testing/xhci/ports.rs`**

In `crates/usb/src/testing/xhci/ports.rs`, replace:

````rust
    #[test]
    fn a_usb2_reset_enables_the_port_after_10_ms() {
````

with:

````rust
    #[test]
    fn plugging_in_and_resetting_put_a_device_back_at_address_0() {
        let dma = Dma::default();
        let mut x = basic();
        let k120 = FakeUsbDevice::k120();
        let set_address = crate::Setup {
            request: 5,
            ..crate::Setup::set_configuration(7)
        };
        k120.borrow_mut().control(set_address, &[]);
        x.plug(1, k120.clone());
        assert_eq!(k120.borrow().usb_address(), 0, "plugged in");
        k120.borrow_mut().control(set_address, &[]);
        x.write_portsc(0, PP | PR, &dma);
        assert_eq!(k120.borrow().usb_address(), 0, "reset");
    }

    #[test]
    fn a_usb2_reset_enables_the_port_after_10_ms() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` no method named `usb_address` found for struct `Ref<'_, testing::device::FakeUsbDevice>` in the current scope ``.

- [ ] **Step 4: Change `crates/usb/src/testing/device.rs`**

In `crates/usb/src/testing/device.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>>;
}
````

with:

````rust
    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>>;
    /// The USB address it answers to: 0 until SET_ADDRESS.
    fn usb_address(&self) -> u8;
    /// A bus reset (plugged in, or its port reset): back to the Default
    /// state at address 0, unconfigured (USB 2.0 9.1.1.3).
    fn bus_reset(&mut self);
}
````

Replace:

````rust

    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
````

with:

````rust

    fn usb_address(&self) -> u8 {
        self.address
    }

    fn bus_reset(&mut self) {
        self.address = 0;
        self.configuration_value = 0;
    }

    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
````

- [ ] **Step 5: Change `crates/usb/src/testing/storage/bot.rs`**

In `crates/usb/src/testing/storage/bot.rs`, replace:

````rust
        self.usb.max_packet0()
    }
````

with:

````rust
        self.usb.max_packet0()
    }

    fn usb_address(&self) -> u8 {
        self.usb.usb_address()
    }

    /// A bus reset also ends whatever command was going on.
    fn bus_reset(&mut self) {
        self.usb.bus_reset();
        self.phase = Phase::Cbw;
        self.uncleared.clear();
    }
````

- [ ] **Step 6: Change `crates/usb/src/testing/xhci/ports.rs`**

In `crates/usb/src/testing/xhci/ports.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert!(self.devices[i].is_none(), "fake: port {port} is taken");
        self.devices[i] = Some(device);
````

with:

````rust
        assert!(self.devices[i].is_none(), "fake: port {port} is taken");
        device.borrow_mut().bus_reset();
        self.devices[i] = Some(device);
````

Replace:

````rust
        self.portsc[i] = self.portsc[i] & !PED | PR;
        let Some(delay) = self.config.port_reset_time else {
````

with:

````rust
        self.portsc[i] = self.portsc[i] & !PED | PR;
        if let Some(dev) = &self.devices[i] {
            dev.borrow_mut().bus_reset();
        }
        let Some(delay) = self.config.port_reset_time else {
````

- [ ] **Step 7: Change `crates/usb/src/testing/xhci/slots.rs`**

In `crates/usb/src/testing/xhci/slots.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub addressed_at: Duration,
}
````

with:

````rust
    pub addressed_at: Duration,
    /// The USB address its device was given (0 with BSR set).
    pub address: u8,
}
````

Replace:

````rust
        s.addressed_at = self.now;
        s.endpoints.insert(1, ep0);
````

with:

````rust
        s.addressed_at = self.now;
        s.address = if new_state == ADDRESSED {
            slot as u8
        } else {
            0
        };
        s.endpoints.insert(1, ep0);
````

- [ ] **Step 8: Change `crates/usb/src/testing/xhci/transfers.rs`**

In `crates/usb/src/testing/xhci/transfers.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.devices.get(port as usize - 1).cloned().flatten() else {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
````

with:

````rust
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.device_of(slot, port) else {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
````

Replace:

````rust
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.devices.get(port as usize - 1).cloned().flatten() else {
            self.fail(slot, dci, ep.ring, USB_TRANSACTION_ERROR, dma);
````

with:

````rust
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.device_of(slot, port) else {
            self.fail(slot, dci, ep.ring, USB_TRANSACTION_ERROR, dma);
````

Replace:

````rust
                Td::Done
            }
        }
    }

    /// A standard request the device took resets its toggles: CLEAR_FEATURE
````

with:

````rust
                Td::Done
            }
        }
    }

    /// The device a TD of `slot` reaches: the one on its port, if that one
    /// has the slot's address. A device plugged in after the slot's one is
    /// at address 0 until it is addressed itself, so it never answers the
    /// old slot's packets (the controller sees no handshake).
    fn device_of(&self, slot: usize, port: u8) -> Option<super::ports::Device> {
        let dev = self.devices.get((port as usize).checked_sub(1)?)?.clone()?;
        let address = self.slots.get(slot)?.as_ref()?.address;
        let answers = dev.borrow().usb_address() == address;
        answers.then_some(dev)
    }

    /// A standard request the device took resets its toggles: CLEAR_FEATURE
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 322 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -m "usb: a transfer never reaches a device plugged in after the one it was for"
````


### Task 9: A disk that stops answering is given up

Review finding (important): a stick whose firmware hangs but stays connected costs three tries of 5 s and a reset recovery per command, and plan 3's block cache writes every dirty block again on every later sync, block by block: `echo x > f` took about 90 s, writing 1 MiB about 70 minutes per shell command, and even `reboot -f` waited, because it shuts the filesystem down first. When a command's last try ends in `Timeout` (the device does not answer at all), the disk is given up: every later read, write and flush fails at once with nothing sent, logged once as `storage: slot N: not answering; given up until it is plugged in again`. A stick that answers with an error (a medium error, write protect) is not given up, and a stick plugged in again is a new disk (decision 2).

**Files:**
- Modify: `crates/usb/src/host.rs`
- Modify: `crates/usb/src/storage/disk.rs`
- Modify: `crates/usb/src/storage/io.rs`

**Interfaces:**
- Consumes: Tasks 6 and 7.
- Produces: `MassStorage` gives up after a command times out on its last try: later `read`, `write` and `flush` return `UsbError::Timeout` at once.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/host.rs`**

In `crates/usb/src/host.rs`, replace:

````rust
    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
````

with:

````rust
    #[test]
    fn a_disk_given_up_as_not_answering_works_again_when_plugged_in_again() {
        let (hal, mut host) = host(FakeConfig::qemu());
        let stick = FakeStorage::qemu(QEMU_BLOCKS);
        hal.fake().plug(2, stick.clone());
        hal.sleep(Duration::from_millis(60));
        host.service();
        let old = host.disks()[0].id;
        stick.borrow_mut().nak(true);
        let mut buf = vec![0u8; 512];
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Timeout));
        let before = hal.clock();
        assert_eq!(host.read(old, 0, &mut buf), Err(UsbError::Timeout));
        assert!(hal.clock() - before < Duration::from_millis(1));
        // The firmware came back after a replug: a new disk.
        hal.fake().unplug(2);
        host.service();
        stick.borrow_mut().nak(false);
        hal.fake().plug(2, stick);
        hal.sleep(Duration::from_millis(60));
        host.service();
        let new = host.disks()[0].id;
        assert_ne!(new, old);
        assert_eq!(host.read(new, 0, &mut buf), Ok(()));
    }

    #[test]
    fn a_disk_that_cannot_be_started_says_why_and_is_not_set_up_again() {
````

- [ ] **Step 2: Add the failing tests to `crates/usb/src/storage/io.rs`**

In `crates/usb/src/storage/io.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use crate::storage::Sense;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, configured, op};
    use crate::xhci::Xhci;
    use alloc::vec;
    use alloc::vec::Vec;
    use std::cell::RefCell;
````

with:

````rust
    use crate::storage::Sense;
    use crate::testing::{FakeConfig, FakeHal, FakeStorage, TamperBus, configured, op};
    use crate::xhci::Xhci;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::time::Duration;
    use std::cell::RefCell;
````

Replace:

````rust
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Disconnected));
    }
}
````

with:

````rust
        assert_eq!(disk.flush(&mut xhci), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_disk_that_stops_answering_is_given_up() {
        let (hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().nak(true);
        let mut buf = vec![0u8; 512];
        let before = hal.clock();
        assert_eq!(disk.read(&mut xhci, 5, &mut buf), Err(UsbError::Timeout));
        let took = hal.clock() - before;
        assert!(
            took >= Duration::from_secs(15) && took < Duration::from_secs(17),
            "{took:?}"
        );
        let log = hal.log_text();
        assert!(
            log.contains("storage: slot 1: not answering; given up until it is plugged in again")
        );
        // From now on nothing is sent, even once the device would answer.
        stick.borrow_mut().nak(false);
        let requests = hal.fake().requests().len();
        let seen = stick.borrow().events().len();
        let mut bus = TamperBus::new(&mut xhci);
        for _ in 0..2 {
            let before = hal.clock();
            assert_eq!(disk.read(&mut bus, 5, &mut buf), Err(UsbError::Timeout));
            assert_eq!(disk.write(&mut bus, 5, &buf), Err(UsbError::Timeout));
            assert_eq!(disk.flush(&mut bus), Err(UsbError::Timeout));
            assert!(hal.clock() - before < Duration::from_millis(1));
        }
        assert_eq!(bus.bulk, 0, "no bulk transfer");
        assert_eq!(hal.fake().requests().len(), requests, "no control request");
        assert_eq!(
            stick.borrow().events().len(),
            seen,
            "the device saw nothing"
        );
        assert_eq!(hal.log_text().matches("given up").count(), 1, "logged once");
    }

    #[test]
    fn a_disk_that_answers_with_an_error_is_not_given_up() {
        let (_hal, mut xhci, mut disk, stick) = kingston();
        stick.borrow_mut().medium_error_at(5);
        stick.borrow_mut().write_protect();
        let mut buf = vec![0u8; 512];
        assert!(matches!(
            disk.read(&mut xhci, 5, &mut buf),
            Err(UsbError::Sense(_))
        ));
        assert!(matches!(
            disk.write(&mut xhci, 6, &buf),
            Err(UsbError::Sense(_))
        ));
        assert_eq!(disk.read(&mut xhci, 6, &mut buf), Ok(()));
        assert_eq!(disk.flush(&mut xhci), Ok(()));
    }

    #[test]
    fn a_gone_device_is_not_given_up_as_not_answering() {
        let (hal, mut xhci, mut disk, _stick) = kingston();
        let mut bus = TamperBus::new(&mut xhci);
        bus.fail_bulk = Some((0, UsbError::Disconnected));
        let mut buf = vec![0u8; 512];
        assert_eq!(
            disk.read(&mut bus, 5, &mut buf),
            Err(UsbError::Disconnected)
        );
        assert!(!hal.log_text().contains("given up"));
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: 2 tests fail: `storage::io::tests::a_disk_that_stops_answering_is_given_up`, `host::tests::a_disk_given_up_as_not_answering_works_again_when_plugged_in_again`.

- [ ] **Step 4: Change `crates/usb/src/storage/disk.rs`**

In `crates/usb/src/storage/disk.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub(super) no_cache: bool,
}
````

with:

````rust
    pub(super) no_cache: bool,
    /// A command timed out on its last try: the device does not answer
    /// at all, and nothing more is sent to it.
    pub(super) not_answering: bool,
}
````

Replace:

````rust
            no_cache: false,
        })
````

with:

````rust
            no_cache: false,
            not_answering: false,
        })
````

- [ ] **Step 5: Change `crates/usb/src/storage/io.rs`**

In `crates/usb/src/storage/io.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! the disk, or nothing is sent.

````

with:

````rust
//! the disk, or nothing is sent.
//!
//! A disk whose command times out on every try (its firmware hangs: it
//! stays connected and NAKs everything) is given up: each later request
//! would cost three 5 s timeouts, and the block cache writes every dirty
//! block again on each sync, so one shell command would take minutes. From
//! then on every request fails at once with `Timeout`, until the stick is
//! plugged in again (a new `MassStorage`).

````

Replace:

````rust
    /// of at most MAX_BULK bytes each. `buf` must be whole blocks inside
    /// the disk (`Unsupported` otherwise, nothing sent).
    pub fn read(&mut self, bus: &mut dyn Bus, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
````

with:

````rust
    /// of at most MAX_BULK bytes each. `buf` must be whole blocks inside
    /// the disk (`Unsupported` otherwise, nothing sent). A disk given up as
    /// not answering fails at once with `Timeout`.
    pub fn read(&mut self, bus: &mut dyn Bus, lba: u64, buf: &mut [u8]) -> Result<(), UsbError> {
````

Replace:

````rust
    /// support it (ILLEGAL REQUEST) has no cache to flush: that is
    /// success, logged once.
    pub fn flush(&mut self, bus: &mut dyn Bus) -> Result<(), UsbError> {
        if self.no_cache {
            return Ok(());
        }
        let cdb = scsi::synchronize_cache_10();
        match self.transport.command(bus, &cdb, Data::None, Need::UpTo) {
            Err(UsbError::Sense(s)) if s.key == ILLEGAL_REQUEST => {
````

with:

````rust
    /// support it (ILLEGAL REQUEST) has no cache to flush: that is
    /// success, logged once. A disk given up as not answering fails at
    /// once with `Timeout`.
    pub fn flush(&mut self, bus: &mut dyn Bus) -> Result<(), UsbError> {
        if self.not_answering {
            return Err(UsbError::Timeout);
        }
        if self.no_cache {
            return Ok(());
        }
        let cdb = scsi::synchronize_cache_10();
        match self.command(bus, &cdb, Data::None, Need::UpTo) {
            Err(UsbError::Sense(s)) if s.key == ILLEGAL_REQUEST => {
````

Replace:

````rust

    /// Checks the request, then sends one command per MAX_BULK bytes (a
````

with:

````rust

    /// One command with its tries; a device that did not answer any of
    /// them is given up (logged once).
    fn command(
        &mut self,
        bus: &mut dyn Bus,
        cdb: &[u8],
        data: Data,
        need: Need,
    ) -> Result<usize, UsbError> {
        let r = self.transport.command(bus, cdb, data, need);
        if r == Err(UsbError::Timeout) {
            slog!(
                bus,
                self.slot(),
                "not answering; given up until it is plugged in again"
            );
            self.not_answering = true;
        }
        r
    }

    /// Checks the request, then sends one command per MAX_BULK bytes (a
````

Replace:

````rust
        };
        let size = self.block_size;
````

with:

````rust
        };
        if self.not_answering {
            return Err(UsbError::Timeout);
        }
        let size = self.block_size;
````

Replace:

````rust
                    let cdb = scsi::read_10(at, blocks);
                    self.transport
                        .command(bus, &cdb, Data::In(part), Need::All)?;
                }
````

with:

````rust
                    let cdb = scsi::read_10(at, blocks);
                    self.command(bus, &cdb, Data::In(part), Need::All)?;
                }
````

Replace:

````rust
                    let cdb = scsi::write_10(at, blocks);
                    self.transport
                        .command(bus, &cdb, Data::Out(part), Need::All)?;
                }
````

with:

````rust
                    let cdb = scsi::write_10(at, blocks);
                    self.command(bus, &cdb, Data::Out(part), Need::All)?;
                }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 326 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -m "usb: a disk that stops answering is given up"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 11 scenario(s) passed`.

````bash
git push -u origin plan5/mass-storage
gh pr create --base main --head plan5/mass-storage --title "Plan 5: The mass storage driver" --body-file - <<'EOF'
## What

Milestone 1, plan 5, tasks 4–9: CBW/CSW and the SCSI command blocks and answers; setup (GET_MAX_LUN, INQUIRY, TEST UNIT READY for up to 5 s with REQUEST SENSE, READ CAPACITY(10)/(16)); READ(10)/WRITE(10) in 64 KiB pieces and SYNCHRONIZE CACHE(10); stall, sense and reset recovery with at most three tries; `Host` claiming disks with never-reused `DiskId`s and the boot line naming them; a failed device setup tried three times (plan 4's M4); the fake controller addresses devices, so a request never reaches a stick plugged in after the one it was for; a disk that stops answering is given up.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests and the fake xHCI controller and storage device

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan5/mass-storage --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-mass-storage
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: GPT and the root partition (Tasks 10–11)

The kernel's `block` module (spec §6.5): the GPT with its CRC32 checks and backup header, partitions as bounded block devices, and the choice of the root partition by the boot partition's GUID, tested on the host against disks partitioned by the real `sfdisk` and `fdisk`.

Branch `plan5/block`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-block`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan5/block /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-block origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-block
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan5/mass-storage` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan5/block /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-block plan5/mass-storage`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan5/mass-storage>` and re-run `cargo xtask ci` before pushing.

### Task 10: GPT with CRC32 checks and the backup header

Spec §6.5's GPT parsing, in the kernel's new `block` module and tested on the host against disks that the real `sfdisk` and `fdisk` partitioned. `crc32` is IEEE 802.3's, as GPT uses it. `read_gpt` reads the primary header at LBA 1 and checks it as UEFI 2.10 §5.3 says: the signature, a header size of 92 bytes up to a block, its CRC32 (with the CRC field taken as zero), `my_lba`, the usable range and the entry array inside the disk, entries of at least 128 bytes in multiples of 8, an entry array of at most 1 MiB (so a corrupt count cannot allocate gigabytes), and the array's CRC32. If the primary header or its array is bad, the backup is used: at the disk's last block when the primary header is bad, at the primary's `alternate_lba` when only its array is (decision 5). Unused entries are skipped, and so are entries outside the usable range, without failing the table. Any block size works: the tests use 512 and 4096. `Guid` keeps GPT's on-disk byte order, which is also how `BootInfo` carries the boot partition.

**Files:**
- Create: `kernel/src/block/crc32.rs`
- Create: `kernel/src/block/gpt.rs`
- Create: `kernel/src/block/mod.rs`
- Create: `kernel/src/block/testing.rs`
- Modify: `kernel/src/lib.rs`

**Interfaces:**
- Consumes: `vfs::{BlockDevice, IoError}`.
- Produces: `relay_kernel::block::{crc32::crc32(&[u8]) -> u32, gpt::{Guid([u8; 16]) (from_fields, is_zero, Display), ESP_TYPE, LINUX_FS_TYPE, GptPartition { number, type_guid, unique_guid, first_lba, last_lba }, Gpt { disk_guid, partitions, used_backup }, GptError { Io(IoError), NoGpt }, read_gpt(&mut dyn BlockDevice) -> Result<Gpt, GptError>}}`; test support `block::testing::{MemDisk, sfdisk_image, fdisk_image, expected_gpt, guid}`.

- [ ] **Step 1: Write the failing tests for `kernel/src/block/crc32.rs`**

Create `kernel/src/block/crc32.rs` containing only this test module (the implementation goes above it in a later step):

````rust
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
        let all: alloc::vec::Vec<u8> = (0..=255).collect();
        assert_eq!(crc32(&all), 0x2905_8C73);
    }
}
````

- [ ] **Step 2: Write the failing tests for `kernel/src/block/gpt.rs`**

Create `kernel/src/block/gpt.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::{MemDisk, expected_gpt, fdisk_image, guid, sfdisk_image};
    use alloc::string::ToString;

    /// `xtask/src/image.rs`'s `sfdisk_script(true)` with the fixed GUIDs of
    /// `xtask/src/config.rs`: the QEMU image's layout.
    const OUR_LAYOUT: &str = "label: gpt\n\
        label-id: 52454C41-5900-4000-8000-000000000001\n\
        first-lba: 2048\n\
        start=2048, size=131072, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, \
        uuid=52454C41-5900-4000-8000-000000000002, name=\"RELAYESP\"\n\
        start=133120, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, \
        uuid=52454C41-5900-4000-8000-000000000003, name=\"relayroot\"\n";
    const IMAGE_BYTES: u64 = 256 << 20;

    /// Two partitions on 8 MiB, for the corruption tests.
    const SMALL: &str = "label: gpt\n\
        start=2048, size=4096, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
        start=6144, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";
    const SMALL_BYTES: u64 = 8 << 20;
    const SMALL_BLOCKS: u64 = SMALL_BYTES / 512;

    fn get32(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }
    fn get64(b: &[u8], o: usize) -> u64 {
        u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
    }
    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put64(b: &mut [u8], o: usize, v: u64) {
        b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }

    fn block(img: &mut [u8], bs: usize, lba: u64) -> &mut [u8] {
        let at = lba as usize * bs;
        &mut img[at..at + bs]
    }

    /// Recomputes the header CRC at `lba`, as a writer of a hostile header
    /// would, over `header_size` bytes (at most the block).
    fn reseal(img: &mut [u8], bs: usize, lba: u64) {
        let h = block(img, bs, lba);
        let size = (get32(h, H_SIZE) as usize).min(bs);
        put32(h, H_CRC, 0);
        let crc = crc32(&h[..size]);
        put32(h, H_CRC, crc);
    }

    /// Recomputes the entry array CRC of the header at `lba` from its
    /// fields, then the header CRC.
    fn reseal_array(img: &mut [u8], bs: usize, lba: u64) {
        let h = block(img, bs, lba);
        let at = get64(h, H_ENTRIES_LBA) as usize * bs;
        let len = get32(h, H_ENTRY_COUNT) as usize * get32(h, H_ENTRY_SIZE) as usize;
        let crc = crc32(&img[at..at + len]);
        put32(block(img, bs, lba), H_ENTRIES_CRC, crc);
        reseal(img, bs, lba);
    }

    /// Byte offset of entry `index` (0-based) of the header at `lba`.
    fn entry(img: &mut [u8], bs: usize, lba: u64, index: usize) -> usize {
        let h = block(img, bs, lba);
        get64(h, H_ENTRIES_LBA) as usize * bs + index * get32(h, H_ENTRY_SIZE) as usize
    }

    fn read(img: &[u8], bs: usize) -> Result<Gpt, GptError> {
        read_gpt(&mut MemDisk::new(img.to_vec(), bs))
    }

    /// A fresh small image and what reading it gives.
    fn small() -> (Vec<u8>, Gpt) {
        let img = sfdisk_image(SMALL, SMALL_BYTES);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        assert_eq!(gpt.partitions.len(), 2);
        (img, gpt)
    }

    fn from_backup(gpt: &Gpt) -> Result<Gpt, GptError> {
        Ok(Gpt {
            used_backup: true,
            ..gpt.clone()
        })
    }

    /// Applies `change` to the primary header, reseals it and expects the
    /// backup to be used.
    fn refused(change: impl FnOnce(&mut [u8])) {
        let (mut img, good) = small();
        change(block(&mut img, 512, 1));
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn guid_from_fields_matches_the_bytes_sfdisk_writes() {
        let img = sfdisk_image(OUR_LAYOUT, IMAGE_BYTES);
        let esp = &img[1024..1024 + 128];
        let root = &img[1024 + 128..1024 + 256];
        assert_eq!(esp[..16], ESP_TYPE.0);
        assert_eq!(root[..16], LINUX_FS_TYPE.0);
        let id = |n| Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n]);
        assert_eq!(img[512 + H_DISK_GUID..512 + H_DISK_GUID + 16], id(1).0);
        assert_eq!(esp[16..32], id(2).0);
        assert_eq!(root[16..32], id(3).0);
        assert_eq!(
            LINUX_FS_TYPE.0,
            [
                0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47,
                0x7D, 0xE4
            ]
        );
    }

    #[test]
    fn guid_display_is_canonical_upper_case() {
        assert_eq!(
            LINUX_FS_TYPE.to_string(),
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
        );
        assert_eq!(ESP_TYPE.to_string(), "C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
        assert_eq!(
            Guid([0; 16]).to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(guid("52454C41-5900-4000-8000-00000000000A").0[15], 0x0A);
    }

    #[test]
    fn only_the_all_zero_guid_is_zero() {
        assert!(Guid([0; 16]).is_zero());
        assert!(!LINUX_FS_TYPE.is_zero());
        let mut last = [0; 16];
        last[15] = 1;
        assert!(!Guid(last).is_zero());
    }

    #[test]
    fn errors_are_short() {
        assert_eq!(GptError::Io(IoError::Device).to_string(), "read error");
        assert_eq!(GptError::NoGpt.to_string(), "no valid GPT");
    }

    #[test]
    fn our_layout_as_sfdisk_writes_it() {
        let img = sfdisk_image(OUR_LAYOUT, IMAGE_BYTES);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        let id = |n| Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n]);
        // sfdisk ends the last partition on a 1 MiB boundary.
        let last = (IMAGE_BYTES >> 9) - 2048 - 1;
        assert_eq!(
            gpt,
            Gpt {
                disk_guid: id(1),
                partitions: alloc::vec![
                    GptPartition {
                        number: 1,
                        type_guid: ESP_TYPE,
                        unique_guid: id(2),
                        first_lba: 2048,
                        last_lba: 133_119,
                    },
                    GptPartition {
                        number: 2,
                        type_guid: LINUX_FS_TYPE,
                        unique_guid: id(3),
                        first_lba: 133_120,
                        last_lba: last,
                    },
                ],
                used_backup: false,
            }
        );
    }

    #[test]
    fn more_partitions_with_gaps_in_the_numbering() {
        // Entries 4, 6-8 and 10-127 stay unused; entry 128 is the array's last.
        let script = "label: gpt\n\
            p1 : start=2048, size=2048, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
            p2 : start=4096, size=1, type=0657FD6D-A4AB-43C4-84E5-0933C84B4F4F\n\
            p3 : start=6144, size=2048, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n\
            p5 : start=8192, size=2048, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n\
            p9 : start=10240, size=2048, type=21686148-6449-6E6F-744E-656564454649\n\
            p128 : start=12288, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";
        let img = sfdisk_image(script, 16 << 20);
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 512));
        let numbers: Vec<u32> = gpt.partitions.iter().map(|p| p.number).collect();
        assert_eq!(numbers, [1, 2, 3, 5, 9, 128]);
        assert_eq!(gpt.partitions[1].first_lba, gpt.partitions[1].last_lba);
        assert!(!gpt.used_backup);
    }

    const FOUR_K: &str = "label: gpt\n\
        sector-size: 4096\n\
        start=256, size=1024, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n\
        start=1280, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n";

    #[test]
    fn sectors_of_4096_bytes() {
        let img = fdisk_image(FOUR_K, 16 << 20, 4096);
        let gpt = read(&img, 4096).unwrap();
        assert_eq!(gpt, expected_gpt(&img, 4096));
        let lbas: Vec<(u64, u64)> = gpt
            .partitions
            .iter()
            .map(|p| (p.first_lba, p.last_lba))
            .collect();
        // sfdisk ends the last partition on a 1 MiB boundary.
        assert_eq!(lbas, [(256, 1279), (1280, 4096 - 256 - 1)]);
        assert_eq!(gpt.partitions[1].type_guid, LINUX_FS_TYPE);
    }

    #[test]
    fn the_block_size_is_the_devices() {
        let four_k = fdisk_image(FOUR_K, 16 << 20, 4096);
        assert_eq!(read(&four_k, 512), Err(GptError::NoGpt));
        let (small, _) = small();
        assert_eq!(read(&small, 4096), Err(GptError::NoGpt));
    }

    #[test]
    fn corrupt_primary_signature_uses_the_backup() {
        let (mut img, good) = small();
        img[512] ^= 1;
        assert_eq!(read(&img, 512), from_backup(&good));
        // With the CRC recomputed, only the signature check catches it.
        refused(|h| h[7] ^= 1);
    }

    #[test]
    fn corrupt_byte_under_the_primary_crc_uses_the_backup() {
        let (mut img, good) = small();
        img[512 + H_DISK_GUID] ^= 1;
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn primary_my_lba_must_be_where_it_was_read() {
        refused(|h| put64(h, H_MY_LBA, 2));
    }

    #[test]
    fn a_resealed_primary_is_accepted() {
        // The other tests change a field and reseal; this shows resealing
        // alone changes nothing.
        let (mut img, good) = small();
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    /// The small image with `extra` zero blocks after it, so the backup
    /// header is no longer at the last LBA.
    fn grown(extra: usize) -> (Vec<u8>, Gpt) {
        let (mut img, good) = small();
        img.resize(img.len() + extra * 512, 0);
        (img, good)
    }

    #[test]
    fn corrupt_primary_array_uses_the_alternate_lba() {
        let (mut img, good) = grown(64);
        let at = entry(&mut img, 512, 1, 0);
        img[at + 56] ^= 1; // a byte of the name
        assert_eq!(read(&img, 512), from_backup(&good));
    }

    #[test]
    fn a_bad_primary_header_looks_at_the_last_lba_only() {
        let (mut img, _) = grown(64);
        img[512] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn the_alternate_lba_must_be_inside_the_disk() {
        for alternate in [SMALL_BLOCKS, u64::MAX] {
            let (mut img, _) = small();
            let at = entry(&mut img, 512, 1, 0);
            img[at + 56] ^= 1;
            put64(block(&mut img, 512, 1), H_ALTERNATE, alternate);
            reseal(&mut img, 512, 1);
            assert_eq!(read(&img, 512), Err(GptError::NoGpt), "{alternate}");
        }
    }

    #[test]
    fn both_headers_corrupt_is_no_gpt() {
        let (mut img, _) = small();
        img[512] ^= 1;
        let last = img.len() - 512;
        img[last] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn both_arrays_corrupt_is_no_gpt() {
        let (mut img, _) = small();
        let primary = entry(&mut img, 512, 1, 0);
        let backup = entry(&mut img, 512, SMALL_BLOCKS - 1, 0);
        img[primary + 56] ^= 1;
        img[backup + 56] ^= 1;
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn a_disk_of_zeros_is_no_gpt() {
        assert_eq!(read(&[0; 64 * 512], 512), Err(GptError::NoGpt));
        assert_eq!(read(&[0; 8 * 4096], 4096), Err(GptError::NoGpt));
        assert_eq!(read(&[], 512), Err(GptError::NoGpt));
        assert_eq!(read(&[0; 512], 512), Err(GptError::NoGpt));
    }

    #[test]
    fn blocks_too_small_for_a_header_are_no_gpt() {
        assert_eq!(read(&[0; 64], 4), Err(GptError::NoGpt));
        let mut img = alloc::vec![0; 64 * 64];
        img[64..72].copy_from_slice(SIGNATURE);
        assert_eq!(read(&img, 64), Err(GptError::NoGpt));
    }

    fn with_bad(img: &[u8], bad: core::ops::Range<u64>) -> Result<Gpt, GptError> {
        let mut disk = MemDisk::new(img.to_vec(), 512);
        disk.bad = bad;
        read_gpt(&mut disk)
    }

    #[test]
    fn failing_reads_are_io_errors() {
        let (img, _) = small();
        assert_eq!(
            with_bad(&img, 0..u64::MAX),
            Err(GptError::Io(IoError::Device))
        );
    }

    #[test]
    fn an_unreadable_primary_uses_the_backup() {
        let (img, good) = small();
        assert_eq!(with_bad(&img, 1..2), from_backup(&good));
        // The primary array unreadable: the header is fine, so the backup
        // comes from its alternate LBA.
        let (img, good) = grown(64);
        assert_eq!(with_bad(&img, 2..3), from_backup(&good));
    }

    #[test]
    fn a_read_error_counts_only_if_the_backup_is_unreadable_too() {
        let (mut img, _) = small();
        let last = img.len() - 512;
        img[last] ^= 1;
        // Primary unreadable, backup readable but corrupt.
        assert_eq!(with_bad(&img, 1..2), Err(GptError::NoGpt));
        // Primary corrupt, backup unreadable.
        let (mut img, _) = small();
        img[512] ^= 1;
        assert_eq!(
            with_bad(&img, SMALL_BLOCKS - 1..SMALL_BLOCKS),
            Err(GptError::Io(IoError::Device))
        );
    }

    #[test]
    fn hostile_header_sizes_are_refused() {
        for size in [0, 91, 513, u32::MAX] {
            refused(|h| put32(h, H_SIZE, size));
        }
    }

    #[test]
    fn a_header_as_large_as_the_block_is_accepted() {
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_SIZE, 512);
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn hostile_usable_ranges_are_refused() {
        refused(|h| put64(h, H_LAST_USABLE, SMALL_BLOCKS));
        refused(|h| put64(h, H_LAST_USABLE, u64::MAX));
        refused(|h| {
            let last = get64(h, H_LAST_USABLE);
            put64(h, H_FIRST_USABLE, last + 1);
        });
    }

    #[test]
    fn a_usable_range_up_to_the_last_block_is_accepted() {
        let (mut img, good) = small();
        put64(block(&mut img, 512, 1), H_LAST_USABLE, SMALL_BLOCKS - 1);
        reseal(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn an_entry_array_past_the_end_is_refused() {
        refused(|h| put64(h, H_ENTRIES_LBA, SMALL_BLOCKS - 1));
        refused(|h| put64(h, H_ENTRIES_LBA, u64::MAX));
        // In the backup it is refused before anything is read, so it is
        // not a read error.
        let (mut img, _) = small();
        img[512] ^= 1;
        put64(
            block(&mut img, 512, SMALL_BLOCKS - 1),
            H_ENTRIES_LBA,
            SMALL_BLOCKS - 1,
        );
        reseal(&mut img, 512, SMALL_BLOCKS - 1);
        assert_eq!(read(&img, 512), Err(GptError::NoGpt));
    }

    #[test]
    fn an_entry_array_up_to_the_last_block_is_accepted() {
        // Moved over the backup header, so only the primary can be read.
        // 128 entries take 32 blocks of 512 bytes, 4 of 4096.
        let four_k = fdisk_image(FOUR_K, 16 << 20, 4096);
        for (mut img, bs) in [(small().0, 512), (four_k, 4096)] {
            let good = read(&img, bs).unwrap();
            let array = 128 * 128;
            let to = img.len() - array;
            img.copy_within(2 * bs..2 * bs + array, to);
            put64(block(&mut img, bs, 1), H_ENTRIES_LBA, (to / bs) as u64);
            reseal(&mut img, bs, 1);
            assert_eq!(read(&img, bs), Ok(good), "{bs}");
        }
    }

    #[test]
    fn an_entry_array_ending_inside_a_block_is_accepted() {
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_ENTRY_COUNT, 127);
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn hostile_entry_sizes_are_refused() {
        for size in [0, 8, 100, 120, 132] {
            let (mut img, good) = small();
            put32(block(&mut img, 512, 1), H_ENTRY_SIZE, size);
            reseal_array(&mut img, 512, 1);
            assert_eq!(read(&img, 512), from_backup(&good), "{size}");
        }
    }

    #[test]
    fn an_entry_array_over_1_mib_is_refused() {
        // 8193 entries of 128 bytes end inside the 8 MiB disk, so only the
        // size bound refuses them.
        for count in [8193, u32::MAX] {
            let (mut img, good) = small();
            put32(block(&mut img, 512, 1), H_ENTRY_COUNT, count);
            if count == 8193 {
                reseal_array(&mut img, 512, 1);
            } else {
                reseal(&mut img, 512, 1);
            }
            assert_eq!(read(&img, 512), from_backup(&good), "{count}");
        }
    }

    #[test]
    fn an_entry_array_of_1_mib_is_accepted() {
        // The entries past sfdisk's 128 are the zeros before partition 1.
        let (mut img, good) = small();
        put32(block(&mut img, 512, 1), H_ENTRY_COUNT, 8192);
        reseal_array(&mut img, 512, 1);
        assert_eq!(read(&img, 512), Ok(good));
    }

    /// Changes entry `index` of the primary array and reseals it.
    fn change_entry(img: &mut [u8], index: usize, change: impl FnOnce(&mut [u8])) {
        let at = entry(img, 512, 1, index);
        change(&mut img[at..at + 128]);
        reseal_array(img, 512, 1);
    }

    #[test]
    fn entries_outside_the_usable_range_are_skipped() {
        let (mut img, good) = small();
        let last_usable = get64(block(&mut img, 512, 1), H_LAST_USABLE);
        change_entry(&mut img, 1, |e| put64(e, E_LAST, last_usable + 1));
        let gpt = read(&img, 512).unwrap();
        assert_eq!(gpt.partitions, good.partitions[..1]);
        assert!(!gpt.used_backup);

        let (mut img, good) = small();
        let first_usable = get64(block(&mut img, 512, 1), H_FIRST_USABLE);
        change_entry(&mut img, 0, |e| put64(e, E_FIRST, first_usable - 1));
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }

    #[test]
    fn entries_may_fill_the_usable_range() {
        // sfdisk starts partition 1 at the first usable LBA but ends the
        // last one on a 1 MiB boundary; move its end to the last usable one.
        let (mut img, mut good) = small();
        let h = block(&mut img, 512, 1);
        let (first_usable, last_usable) = (get64(h, H_FIRST_USABLE), get64(h, H_LAST_USABLE));
        assert_eq!(good.partitions[0].first_lba, first_usable);
        change_entry(&mut img, 1, |e| put64(e, E_LAST, last_usable));
        good.partitions[1].last_lba = last_usable;
        assert_eq!(read(&img, 512), Ok(good));
    }

    #[test]
    fn reversed_entries_are_skipped() {
        let (mut img, good) = small();
        change_entry(&mut img, 0, |e| {
            let first = get64(e, E_FIRST);
            put64(e, E_LAST, first - 1);
        });
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }

    #[test]
    fn entries_with_a_zero_type_are_unused() {
        let (mut img, good) = small();
        change_entry(&mut img, 0, |e| e[..16].fill(0));
        assert_eq!(read(&img, 512).unwrap().partitions, good.partitions[1..]);
    }
}
````

- [ ] **Step 3: Create `kernel/src/block/mod.rs`**

Create `kernel/src/block/mod.rs`:

````rust
//! Block devices below the filesystem (spec §6.5): the GPT of a disk, read
//! with CRC32 checks and the backup header as a fallback.

pub mod crc32;
pub mod gpt;

#[cfg(test)]
mod testing;
````

- [ ] **Step 4: Create `kernel/src/block/testing.rs`**

Test support for the block module: an in-memory disk, and images partitioned by util-linux. Missing tools fail the tests, never skip them (the CI runner has `sfdisk` and `fdisk`).

Create `kernel/src/block/testing.rs`:

````rust
//! Test support for the block module: an in-memory disk and partitioned
//! images written by util-linux, so the parser is checked against what Linux
//! writes. `sfdisk` 2.39 cannot write 4096-byte sectors to a file (it
//! recalculates the script for the file's 512), so those images come from
//! `fdisk -b 4096`, which loads the same script format with its `I` command.

use super::gpt::{Gpt, GptPartition, Guid};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use vfs::{BlockDevice, IoError, check_request};

/// A disk in memory.
pub struct MemDisk {
    pub data: Vec<u8>,
    block_size: usize,
    /// Reads that touch one of these blocks fail with `IoError::Device`.
    pub bad: Range<u64>,
}

impl MemDisk {
    pub fn new(data: Vec<u8>, block_size: usize) -> MemDisk {
        assert!(data.len().is_multiple_of(block_size));
        MemDisk {
            data,
            block_size,
            bad: 0..0,
        }
    }

    /// Byte range of a checked request.
    fn span(&self, lba: u64, len: usize) -> Result<Range<usize>, IoError> {
        check_request(self.block_size, self.block_count(), lba, len)?;
        let start = lba as usize * self.block_size;
        Ok(start..start + len)
    }
}

impl BlockDevice for MemDisk {
    fn block_size(&self) -> usize {
        self.block_size
    }
    fn block_count(&self) -> u64 {
        (self.data.len() / self.block_size) as u64
    }
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let span = self.span(lba, buf.len())?;
        let end = lba + (buf.len() / self.block_size) as u64;
        if lba < self.bad.end && self.bad.start < end {
            return Err(IoError::Device);
        }
        buf.copy_from_slice(&self.data[span]);
        Ok(())
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        let span = self.span(lba, buf.len())?;
        self.data[span].copy_from_slice(buf);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// A fresh path `<workspace>/target/tmp/block/<name>-<n>`, unique within the
/// test binary. Kernel unit tests get no `CARGO_TARGET_TMPDIR`.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/tmp/block"));
    fs::create_dir_all(dir).unwrap();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{name}-{n}"));
    let _ = fs::remove_file(&path);
    path
}

/// A util-linux command. Missing tools fail the test: checking against
/// them is the point, so they are never skipped.
fn tool(name: &str) -> Command {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain(["/usr/sbin".into(), "/sbin".into()]);
    for dir in dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let mut cmd = Command::new(candidate);
            cmd.env("LC_ALL", "C");
            return cmd;
        }
    }
    panic!("install util-linux: {name} not found");
}

/// Runs `cmd` with `input` on stdin and returns its stdout.
fn run(mut cmd: Command, input: &str) -> String {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("starting {cmd:?}: {e}"));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        panic!(
            "{cmd:?} failed ({}):\n{stdout}{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    stdout
}

fn image_file(name: &str, bytes: u64) -> PathBuf {
    let path = scratch(name);
    fs::File::create(&path).unwrap().set_len(bytes).unwrap();
    path
}

fn take(path: &Path) -> Vec<u8> {
    let data = fs::read(path).unwrap();
    fs::remove_file(path).unwrap();
    data
}

/// A disk of `bytes` partitioned by `sfdisk` from `script` (512-byte sectors).
pub fn sfdisk_image(script: &str, bytes: u64) -> Vec<u8> {
    let path = image_file("sfdisk.img", bytes);
    let mut cmd = tool("sfdisk");
    cmd.args([
        "--quiet",
        "--no-reread",
        "--no-tell-kernel",
        "--wipe",
        "always",
    ])
    .arg(&path);
    run(cmd, script);
    take(&path)
}

/// A disk of `bytes` with `sector_size`-byte sectors, partitioned by
/// `fdisk -b <sector_size>` loading the sfdisk-format `script`.
pub fn fdisk_image(script: &str, bytes: u64, sector_size: usize) -> Vec<u8> {
    let path = image_file("fdisk.img", bytes);
    let script_path = scratch("fdisk.script");
    fs::write(&script_path, script).unwrap();
    let mut cmd = tool("fdisk");
    cmd.args(["-b", &sector_size.to_string(), "--wipe", "always"])
        .arg(&path);
    let out = run(cmd, &format!("I\n{}\nw\n", script_path.display()));
    fs::remove_file(&script_path).unwrap();
    // fdisk carries on (and writes an empty DOS label) if the script fails.
    assert!(out.contains("Script successfully applied"), "{out}");
    take(&path)
}

/// The GPT of `image` as util-linux reads it: `sfdisk --dump` for 512-byte
/// sectors, the same dump from fdisk's `O` command for other sizes.
pub fn expected_gpt(image: &[u8], sector_size: usize) -> Gpt {
    let path = scratch("dump.img");
    fs::write(&path, image).unwrap();
    let dump = if sector_size == 512 {
        let mut cmd = tool("sfdisk");
        cmd.arg("--dump").arg(&path);
        run(cmd, "")
    } else {
        let dump_path = scratch("fdisk.dump");
        let mut cmd = tool("fdisk");
        cmd.args(["-b", &sector_size.to_string()]).arg(&path);
        run(cmd, &format!("O\n{}\nq\n", dump_path.display()));
        let dump = fs::read_to_string(&dump_path).unwrap();
        fs::remove_file(&dump_path).unwrap();
        dump
    };
    fs::remove_file(&path).unwrap();
    parse_dump(&dump)
}

/// Parses the sfdisk dump format:
/// `label-id: <guid>` and `<node><n> : start=…, size=…, type=…, uuid=…`.
fn parse_dump(dump: &str) -> Gpt {
    let mut disk_guid = None;
    let mut partitions = Vec::new();
    for line in dump.lines() {
        if let Some(id) = line.strip_prefix("label-id: ") {
            disk_guid = Some(guid(id));
        }
        let Some((node, fields)) = line.split_once(" : ") else {
            continue;
        };
        let digits = node.len() - node.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let number = node[node.len() - digits..].parse().unwrap();
        let field = |key: &str| {
            fields
                .split(", ")
                .find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
                .unwrap_or_else(|| panic!("no {key} in {line}"))
                .trim()
        };
        let start: u64 = field("start").parse().unwrap();
        let size: u64 = field("size").parse().unwrap();
        partitions.push(GptPartition {
            number,
            type_guid: guid(field("type")),
            unique_guid: guid(field("uuid")),
            first_lba: start,
            last_lba: start + size - 1,
        });
    }
    Gpt {
        disk_guid: disk_guid.expect("no label-id in the dump"),
        partitions,
        used_backup: false,
    }
}

/// A GUID from its canonical text form.
pub fn guid(text: &str) -> Guid {
    let parts: Vec<&str> = text.split('-').collect();
    assert_eq!(parts.len(), 5, "{text}");
    let hex = |s: &str| u64::from_str_radix(s, 16).unwrap();
    let tail = hex(parts[3]) << 48 | hex(parts[4]);
    Guid::from_fields(
        hex(parts[0]) as u32,
        hex(parts[1]) as u16,
        hex(parts[2]) as u16,
        tail.to_be_bytes(),
    )
}
````

- [ ] **Step 5: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod arch;
pub mod cmdline;
````

with:

````rust
pub mod arch;
pub mod block;
pub mod cmdline;
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `H_SIZE` in this scope ``; `` cannot find value `H_CRC` in this scope ``.

- [ ] **Step 7: Implement `kernel/src/block/crc32.rs`**

Insert this at the top of `kernel/src/block/crc32.rs`, above `#[cfg(test)]`:

````rust
//! CRC-32 as GPT needs it (spec §6.5): the header and the partition entry
//! array each carry one.

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

````

- [ ] **Step 8: Implement `kernel/src/block/gpt.rs`**

Insert this at the top of `kernel/src/block/gpt.rs`, above `#[cfg(test)]`:

````rust
//! GPT parsing (spec §6.5, UEFI 2.10 §5.3): the primary header at LBA 1 and
//! its partition entry array, both CRC32-checked, and the backup header when
//! either is bad. Everything read from the disk is untrusted (spec §10): a
//! corrupt or hostile table is refused or skipped, never a panic.

use super::crc32::crc32;
use alloc::vec::Vec;
use core::fmt;
use vfs::{BlockDevice, IoError};

/// A GUID in on-disk byte order (the first three fields little-endian), as GPT and BootInfo store it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// From the canonical text form's fields: 0FC63DAF-8483-4772-8E79-3D69D8477DE4 is
    /// from_fields(0x0FC63DAF, 0x8483, 0x4772, [0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4]).
    pub const fn from_fields(a: u32, b: u16, c: u16, d: [u8; 8]) -> Guid {
        let a = a.to_le_bytes();
        let b = b.to_le_bytes();
        let c = c.to_le_bytes();
        Guid([
            a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5],
            d[6], d[7],
        ])
    }

    /// The zero GUID marks an unused entry.
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 16]
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let g = &self.0;
        let a = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
        let b = u16::from_le_bytes([g[4], g[5]]);
        let c = u16::from_le_bytes([g[6], g[7]]);
        write!(f, "{a:08X}-{b:04X}-{c:04X}-{:02X}{:02X}-", g[8], g[9])?;
        for byte in &g[10..] {
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

/// The EFI System Partition: C12A7328-F81F-11D2-BA4B-00A0C93EC93B.
pub const ESP_TYPE: Guid = Guid::from_fields(
    0xC12A_7328,
    0xF81F,
    0x11D2,
    [0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B],
);

/// A Linux filesystem: 0FC63DAF-8483-4772-8E79-3D69D8477DE4.
pub const LINUX_FS_TYPE: Guid = Guid::from_fields(
    0x0FC6_3DAF,
    0x8483,
    0x4772,
    [0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4],
);

/// A used entry of the partition array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GptPartition {
    /// 1-based index in the entry array (Linux's partition number).
    pub number: u32,
    pub type_guid: Guid,
    pub unique_guid: Guid,
    pub first_lba: u64,
    /// Inclusive, as GPT stores it.
    pub last_lba: u64,
}

/// A disk's partition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gpt {
    pub disk_guid: Guid,
    /// Used entries (type GUID not zero) in entry order.
    pub partitions: Vec<GptPartition>,
    /// The primary header or its entry array was bad and the backup was used.
    pub used_backup: bool,
}

/// Why `read_gpt` found no table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    /// Reading the disk failed.
    Io(IoError),
    /// Neither header is a valid GPT header with a valid entry array.
    NoGpt,
}

impl fmt::Display for GptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GptError::Io(_) => write!(f, "read error"),
            GptError::NoGpt => write!(f, "no valid GPT"),
        }
    }
}

const SIGNATURE: &[u8; 8] = b"EFI PART";
/// The header fields of UEFI 2.10 end here; `header_size` may be larger.
const MIN_HEADER_SIZE: usize = 92;
const MIN_ENTRY_SIZE: u32 = 128;
/// sfdisk writes 16 KiB; this leaves room for any real table, and a corrupt
/// entry count cannot make us allocate gigabytes.
const MAX_ARRAY_BYTES: u64 = 1 << 20;

/// Offsets in the header (UEFI 2.10 table 5.5).
const H_SIZE: usize = 12;
const H_CRC: usize = 16;
const H_MY_LBA: usize = 24;
const H_ALTERNATE: usize = 32;
const H_FIRST_USABLE: usize = 40;
const H_LAST_USABLE: usize = 48;
const H_DISK_GUID: usize = 56;
const H_ENTRIES_LBA: usize = 72;
const H_ENTRY_COUNT: usize = 80;
const H_ENTRY_SIZE: usize = 84;
const H_ENTRIES_CRC: usize = 88;

/// Offsets in a partition entry (UEFI 2.10 table 5.6).
const E_TYPE: usize = 0;
const E_UNIQUE: usize = 16;
const E_FIRST: usize = 32;
const E_LAST: usize = 40;

/// Callers check that `b` is long enough first.
fn u32_at(b: &[u8], o: usize) -> u32 {
    let mut v = [0; 4];
    v.copy_from_slice(&b[o..o + 4]);
    u32::from_le_bytes(v)
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut v = [0; 8];
    v.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(v)
}

fn guid_at(b: &[u8], o: usize) -> Guid {
    let mut g = [0; 16];
    g.copy_from_slice(&b[o..o + 16]);
    Guid(g)
}

/// A header that passed every check of `parse_header`.
struct Header {
    alternate_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: Guid,
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc: u32,
}

impl Header {
    /// Bytes in the entry array; bounded by `MAX_ARRAY_BYTES` once parsed.
    fn array_bytes(&self) -> u64 {
        self.entry_count as u64 * self.entry_size as u64
    }
}

/// Checks the header in `block`, read from `lba` of a disk of `blocks`
/// blocks of `block.len()` bytes.
fn parse_header(block: &[u8], lba: u64, blocks: u64) -> Option<Header> {
    if block.len() < MIN_HEADER_SIZE || block[..8] != *SIGNATURE {
        return None;
    }
    let size = u32_at(block, H_SIZE) as usize;
    if !(MIN_HEADER_SIZE..=block.len()).contains(&size) {
        return None;
    }
    let mut copy = block[..size].to_vec();
    copy[H_CRC..H_CRC + 4].fill(0);
    if crc32(&copy) != u32_at(block, H_CRC) || u64_at(block, H_MY_LBA) != lba {
        return None;
    }
    let h = Header {
        alternate_lba: u64_at(block, H_ALTERNATE),
        first_usable: u64_at(block, H_FIRST_USABLE),
        last_usable: u64_at(block, H_LAST_USABLE),
        disk_guid: guid_at(block, H_DISK_GUID),
        entries_lba: u64_at(block, H_ENTRIES_LBA),
        entry_count: u32_at(block, H_ENTRY_COUNT),
        entry_size: u32_at(block, H_ENTRY_SIZE),
        entries_crc: u32_at(block, H_ENTRIES_CRC),
    };
    if h.first_usable > h.last_usable || h.last_usable >= blocks {
        return None;
    }
    if h.entry_size < MIN_ENTRY_SIZE || !h.entry_size.is_multiple_of(8) {
        return None;
    }
    if h.array_bytes() > MAX_ARRAY_BYTES {
        return None;
    }
    let array_blocks = h.array_bytes().div_ceil(block.len() as u64);
    match h.entries_lba.checked_add(array_blocks) {
        Some(end) if end <= blocks => Some(h),
        _ => None,
    }
}

/// Reads `count` whole blocks at `lba`; `count` is bounded by the caller.
fn read_blocks(dev: &mut dyn BlockDevice, lba: u64, count: u64) -> Result<Vec<u8>, IoError> {
    let len = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(dev.block_size()))
        .ok_or(IoError::OutOfRange)?;
    let mut buf = alloc::vec![0; len];
    dev.read(lba, &mut buf)?;
    Ok(buf)
}

/// The header at `lba`, `Ok(None)` if it is not a valid one.
fn read_header(dev: &mut dyn BlockDevice, lba: u64) -> Result<Option<Header>, IoError> {
    let block = read_blocks(dev, lba, 1)?;
    Ok(parse_header(&block, lba, dev.block_count()))
}

/// The used, mountable entries of `h`'s array, `Ok(None)` if its CRC is wrong.
fn read_entries(
    dev: &mut dyn BlockDevice,
    h: &Header,
) -> Result<Option<Vec<GptPartition>>, IoError> {
    // parse_header bounded the array to MAX_ARRAY_BYTES inside the disk.
    let bytes = h.array_bytes() as usize;
    let blocks = bytes.div_ceil(dev.block_size()) as u64;
    let buf = read_blocks(dev, h.entries_lba, blocks)?;
    let array = &buf[..bytes];
    if crc32(array) != h.entries_crc {
        return Ok(None);
    }
    let mut partitions = Vec::new();
    for (i, e) in array.chunks_exact(h.entry_size as usize).enumerate() {
        let type_guid = guid_at(e, E_TYPE);
        let (first_lba, last_lba) = (u64_at(e, E_FIRST), u64_at(e, E_LAST));
        // An entry we could not mount is skipped, not the whole table.
        if type_guid.is_zero()
            || first_lba > last_lba
            || first_lba < h.first_usable
            || last_lba > h.last_usable
        {
            continue;
        }
        partitions.push(GptPartition {
            // At most MAX_ARRAY_BYTES / MIN_ENTRY_SIZE entries, so no overflow.
            number: i as u32 + 1,
            type_guid,
            unique_guid: guid_at(e, E_UNIQUE),
            first_lba,
            last_lba,
        });
    }
    Ok(Some(partitions))
}

/// Reads the GPT of `dev` (spec §6.5): the primary header at LBA 1 and its entry array,
/// both CRC32-checked; if either is bad, the backup header at the disk's last LBA (or at
/// the primary's alternate LBA when the primary header itself was valid) and its array.
///
/// A read error on the primary side is not final, since the backup may still
/// be readable; the result is `Io` only if reading the backup fails too.
pub fn read_gpt(dev: &mut dyn BlockDevice) -> Result<Gpt, GptError> {
    let blocks = dev.block_count();
    let backup_lba = match read_header(dev, 1) {
        Ok(Some(h)) => match read_entries(dev, &h) {
            Ok(Some(partitions)) => return Ok(gpt(&h, partitions, false)),
            // Only a header that passed its checks can say where the
            // backup is.
            Ok(None) | Err(_) => Some(h.alternate_lba).filter(|&lba| lba < blocks),
        },
        Ok(None) | Err(_) => blocks.checked_sub(1),
    };
    let Some(backup_lba) = backup_lba else {
        return Err(GptError::NoGpt);
    };
    let h = read_header(dev, backup_lba)
        .map_err(GptError::Io)?
        .ok_or(GptError::NoGpt)?;
    match read_entries(dev, &h).map_err(GptError::Io)? {
        Some(partitions) => Ok(gpt(&h, partitions, true)),
        None => Err(GptError::NoGpt),
    }
}

fn gpt(h: &Header, partitions: Vec<GptPartition>, used_backup: bool) -> Gpt {
    Gpt {
        disk_guid: h.disk_guid,
        partitions,
        used_backup,
    }
}

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 144 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel
git commit -m "kernel: GPT with CRC32 checks and the backup header"
````


### Task 11: Partitions and the root partition

The rest of spec §6.5. A `Partition` is one GPT partition as a block device of its own: block numbers relative to its start, and every request checked against its bounds (`vfs::check_request`), so a filesystem can never reach outside it. `choose_root` picks the root: the disk whose GPT has the partition with `BootInfo`'s boot GUID, and on it the first Linux filesystem partition; if no disk has it (or the firmware did not say), the first disk with exactly one ESP and exactly one Linux filesystem partition, with `fallback` set so the caller warns (decision 5). A boot disk without a Linux partition is an error, not a reason to fall back to another disk; a zero boot GUID counts as none, and of two disks with the boot partition (a stick copied with `dd`) the first wins.

**Files:**
- Modify: `kernel/src/block/mod.rs`
- Create: `kernel/src/block/partition.rs`
- Create: `kernel/src/block/root.rs`
- Modify: `kernel/src/block/testing.rs`

**Interfaces:**
- Consumes: Task 10.
- Produces: `block::partition::Partition<D: BlockDevice> { new(dev, first_lba, last_lba) -> Result<Partition<D>, IoError>, start } (BlockDevice)`; `block::root::{RootChoice { disk, partition, fallback }, NoRoot { NoDisks, NoLinuxPartition, NoMatch } (Display), choose_root(&[Option<Gpt>], Option<Guid>) -> Result<RootChoice, NoRoot>}`.

- [ ] **Step 1: Declare the new module in `kernel/src/block/mod.rs`**

In `kernel/src/block/mod.rs`, replace:

````rust
pub mod gpt;

````

with:

````rust
pub mod gpt;
pub mod partition;
pub mod root;

````

- [ ] **Step 2: Write the failing tests for `kernel/src/block/partition.rs`**

Create `kernel/src/block/partition.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::MemDisk;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A 16-block disk whose block `n` is filled with `n`.
    fn disk(bs: usize) -> MemDisk {
        let data: Vec<u8> = (0..16u8).flat_map(|n| vec![n; bs]).collect();
        MemDisk::new(data, bs)
    }

    #[test]
    fn block_numbers_are_relative_to_the_start() {
        for bs in [512, 4096] {
            let mut disk = disk(bs);
            let mut p = Partition::new(&mut disk, 4, 11).unwrap();
            assert_eq!(p.start(), 4);
            assert_eq!(p.block_size(), bs);
            assert_eq!(p.block_count(), 8);
            let mut buf = vec![0; 2 * bs];
            p.read(1, &mut buf).unwrap();
            assert!(buf[..bs].iter().all(|&b| b == 5));
            assert!(buf[bs..].iter().all(|&b| b == 6));
            p.write(2, &vec![0xEE; bs]).unwrap();
            assert!(disk.data[6 * bs..7 * bs].iter().all(|&b| b == 0xEE));
            assert!(disk.data[5 * bs..6 * bs].iter().all(|&b| b == 5));
            assert!(disk.data[7 * bs..8 * bs].iter().all(|&b| b == 7));
        }
    }

    #[test]
    fn the_first_and_last_blocks_are_reachable() {
        let mut disk = disk(512);
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let mut buf = vec![0; 512];
        p.read(0, &mut buf).unwrap();
        assert_eq!(buf[0], 4);
        p.read(7, &mut buf).unwrap();
        assert_eq!(buf[0], 11);
        let mut all = vec![0; 8 * 512];
        p.read(0, &mut all).unwrap();
        assert_eq!((all[0], all[all.len() - 1]), (4, 11));
        p.write(7, &[0xEE; 512]).unwrap();
        assert_eq!((disk.data[11 * 512], disk.data[12 * 512]), (0xEE, 12));
    }

    #[test]
    fn requests_outside_the_partition_touch_nothing() {
        let mut disk = disk(512);
        let before = disk.data.clone();
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let cases: [(u64, usize); 6] = [
            (8, 512),        // at the end
            (9, 512),        // past the end
            (7, 1024),       // runs over the end
            (u64::MAX, 512), // start + lba overflows
            (u64::MAX - 3, 512),
            (0, 100), // not whole blocks
        ];
        for (lba, len) in cases {
            let mut buf = vec![0xAA; len];
            assert_eq!(p.read(lba, &mut buf), Err(IoError::OutOfRange), "{lba}");
            assert!(buf.iter().all(|&b| b == 0xAA));
            assert_eq!(p.write(lba, &buf), Err(IoError::OutOfRange), "{lba}");
        }
        assert_eq!(disk.requests, 0);
        assert!(disk.data == before);
    }

    #[test]
    fn new_refuses_ranges_outside_the_disk_or_reversed() {
        let mut disk = disk(512);
        for (first, last) in [(4, 16), (16, 16), (4, u64::MAX), (5, 4), (u64::MAX, 0)] {
            assert!(
                matches!(
                    Partition::new(&mut disk, first, last),
                    Err(IoError::OutOfRange)
                ),
                "{first}..={last}"
            );
        }
        // The whole disk and single blocks are fine.
        assert_eq!(Partition::new(&mut disk, 0, 15).unwrap().block_count(), 16);
        assert_eq!(Partition::new(&mut disk, 15, 15).unwrap().block_count(), 1);
        assert_eq!(Partition::new(&mut disk, 0, 0).unwrap().block_count(), 1);
    }

    #[test]
    fn flush_forwards() {
        let mut disk = disk(512);
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        p.flush().unwrap();
        p.flush().unwrap();
        assert_eq!(disk.flushes, 2);
    }

    #[test]
    fn device_errors_pass_through() {
        let mut disk = disk(512);
        disk.bad = 5..6;
        let mut p = Partition::new(&mut disk, 4, 11).unwrap();
        let mut buf = vec![0; 512];
        assert_eq!(p.read(1, &mut buf), Err(IoError::Device));
        assert_eq!(p.read(0, &mut buf), Ok(()));
    }
}
````

- [ ] **Step 3: Write the failing tests for `kernel/src/block/root.rs`**

Create `kernel/src/block/root.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    const OTHER_TYPE: Guid = Guid([0x42; 16]);

    /// A GUID numbered `n`, unique within a test.
    fn id(n: u8) -> Guid {
        Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n])
    }

    /// A partition of `type_guid` whose unique GUID is `id(n)`.
    fn part(n: u8, type_guid: Guid) -> GptPartition {
        let first = n as u64 * 1000;
        GptPartition {
            number: n as u32,
            type_guid,
            unique_guid: id(n),
            first_lba: first,
            last_lba: first + 999,
        }
    }

    fn disk(partitions: Vec<GptPartition>) -> Option<Gpt> {
        Some(Gpt {
            disk_guid: id(0),
            partitions,
            used_backup: false,
        })
    }

    /// The usual stick: ESP `id(esp)` and Linux `id(esp + 1)`.
    fn stick(esp: u8) -> Option<Gpt> {
        disk(vec![part(esp, ESP_TYPE), part(esp + 1, LINUX_FS_TYPE)])
    }

    fn chosen(disk: usize, partition: GptPartition, fallback: bool) -> Result<RootChoice, NoRoot> {
        Ok(RootChoice {
            disk,
            partition,
            fallback,
        })
    }

    #[test]
    fn the_boot_esp_picks_its_disks_linux_partition() {
        // BootInfo carries the GUID of the ESP we booted from.
        let disks = [stick(1), stick(3)];
        assert_eq!(
            choose_root(&disks, Some(id(3))),
            chosen(1, part(4, LINUX_FS_TYPE), false)
        );
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            chosen(0, part(2, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn the_boot_guid_may_be_any_partition_of_the_disk() {
        let disks = [
            stick(1),
            disk(vec![part(3, OTHER_TYPE), part(4, LINUX_FS_TYPE)]),
        ];
        assert_eq!(
            choose_root(&disks, Some(id(3))),
            chosen(1, part(4, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn the_first_linux_partition_of_the_boot_disk() {
        let disks = [disk(vec![
            part(1, OTHER_TYPE),
            part(2, ESP_TYPE),
            part(3, LINUX_FS_TYPE),
            part(4, LINUX_FS_TYPE),
        ])];
        assert_eq!(
            choose_root(&disks, Some(id(2))),
            chosen(0, part(3, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn a_boot_disk_without_linux_partition_is_an_error() {
        // Even though the second disk would do for the fallback.
        let disks = [disk(vec![part(1, ESP_TYPE), part(2, OTHER_TYPE)]), stick(3)];
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            Err(NoRoot::NoLinuxPartition)
        );
    }

    #[test]
    fn the_first_disk_with_the_boot_guid_wins() {
        // A stick copied with dd has the same GUIDs as the original.
        let disks = [stick(1), stick(1)];
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            chosen(0, part(2, LINUX_FS_TYPE), false)
        );
    }

    /// Disks that do not qualify for the fallback, then one that does.
    fn fallback_disks() -> Vec<Option<Gpt>> {
        vec![
            None,
            disk(vec![]),
            disk(vec![
                part(1, ESP_TYPE),
                part(2, LINUX_FS_TYPE),
                part(3, LINUX_FS_TYPE),
            ]),
            disk(vec![
                part(4, ESP_TYPE),
                part(5, ESP_TYPE),
                part(6, LINUX_FS_TYPE),
            ]),
            disk(vec![part(7, LINUX_FS_TYPE)]),
            disk(vec![part(8, ESP_TYPE)]),
            disk(vec![
                part(9, OTHER_TYPE),
                part(10, LINUX_FS_TYPE),
                part(11, ESP_TYPE),
            ]),
            stick(12),
        ]
    }

    #[test]
    fn without_a_boot_guid_the_first_disk_with_one_esp_and_one_linux_partition() {
        // Other partition types do not matter, nor their order.
        assert_eq!(
            choose_root(&fallback_disks(), None),
            chosen(6, part(10, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn an_unknown_boot_guid_falls_back() {
        assert_eq!(
            choose_root(&fallback_disks(), Some(id(99))),
            chosen(6, part(10, LINUX_FS_TYPE), true)
        );
        assert_eq!(
            choose_root(&[stick(1)], Some(id(99))),
            chosen(0, part(2, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn a_zero_boot_guid_matches_nothing() {
        // A corrupt entry may have a zero unique GUID; it is not the boot one.
        let mut odd = part(1, LINUX_FS_TYPE);
        odd.unique_guid = Guid([0; 16]);
        let disks = [disk(vec![odd]), stick(2)];
        assert_eq!(
            choose_root(&disks, Some(Guid([0; 16]))),
            chosen(1, part(3, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn nothing_qualifies() {
        let mut disks = fallback_disks();
        disks.truncate(6);
        assert_eq!(choose_root(&disks, None), Err(NoRoot::NoMatch));
        assert_eq!(choose_root(&disks, Some(id(99))), Err(NoRoot::NoMatch));
    }

    #[test]
    fn no_readable_gpt_is_no_disks() {
        assert_eq!(choose_root(&[], None), Err(NoRoot::NoDisks));
        assert_eq!(choose_root(&[], Some(id(1))), Err(NoRoot::NoDisks));
        assert_eq!(
            choose_root(&[None, None], Some(id(1))),
            Err(NoRoot::NoDisks)
        );
    }

    #[test]
    fn errors_are_short() {
        assert_eq!(NoRoot::NoDisks.to_string(), "no disk with a GPT");
        assert_eq!(
            NoRoot::NoLinuxPartition.to_string(),
            "no Linux partition on the boot disk"
        );
        assert_eq!(
            NoRoot::NoMatch.to_string(),
            "no disk with the boot partition"
        );
    }
}
````

- [ ] **Step 4: Extend the test support in `kernel/src/block/testing.rs`**

In `kernel/src/block/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    pub bad: Range<u64>,
}
````

with:

````rust
    pub bad: Range<u64>,
    /// Reads and writes that reached the disk.
    pub requests: u32,
    pub flushes: u32,
}
````

Replace:

````rust
            bad: 0..0,
        }
````

with:

````rust
            bad: 0..0,
            requests: 0,
            flushes: 0,
        }
````

Replace:

````rust
        }
        buf.copy_from_slice(&self.data[span]);
````

with:

````rust
        }
        self.requests += 1;
        buf.copy_from_slice(&self.data[span]);
````

Replace:

````rust
        let span = self.span(lba, buf.len())?;
        self.data[span].copy_from_slice(buf);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
````

with:

````rust
        let span = self.span(lba, buf.len())?;
        self.requests += 1;
        self.data[span].copy_from_slice(buf);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.flushes += 1;
        Ok(())
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Guid` in this scope ``; `` cannot find function, tuple struct or tuple variant `Guid` in this scope ``.

- [ ] **Step 6: Change `kernel/src/block/mod.rs`**

Replace the whole of `kernel/src/block/mod.rs` with:

````rust
//! Block devices below the filesystem (spec §6.5): the GPT of a disk, read
//! with CRC32 checks and the backup header as a fallback, its partitions as
//! block devices of their own, and the choice of the root partition.

pub mod crc32;
pub mod gpt;
pub mod partition;
pub mod root;

#[cfg(test)]
mod testing;
````

- [ ] **Step 7: Implement `kernel/src/block/partition.rs`**

Insert this at the top of `kernel/src/block/partition.rs`, above `#[cfg(test)]`:

````rust
//! A GPT partition as a block device (spec §6.5), so a filesystem sees only
//! its own blocks.

use vfs::{BlockDevice, IoError, check_request};

/// One GPT partition of a disk as a block device of its own (spec §6.5): block numbers
/// are relative to its start, and requests outside it are refused (vfs::check_request).
pub struct Partition<D: BlockDevice> {
    dev: D,
    start: u64,
    blocks: u64,
}

impl<D: BlockDevice> Partition<D> {
    /// `first_lba..=last_lba` of `dev`; `Err(IoError::OutOfRange)` if that is not inside the disk or is reversed.
    pub fn new(dev: D, first_lba: u64, last_lba: u64) -> Result<Partition<D>, IoError> {
        if first_lba > last_lba || last_lba >= dev.block_count() {
            return Err(IoError::OutOfRange);
        }
        Ok(Partition {
            dev,
            start: first_lba,
            // last_lba < block_count, so this cannot overflow.
            blocks: last_lba - first_lba + 1,
        })
    }

    /// The partition's first block on the disk.
    pub fn start(&self) -> u64 {
        self.start
    }
}

impl<D: BlockDevice> BlockDevice for Partition<D> {
    fn block_size(&self) -> usize {
        self.dev.block_size()
    }
    fn block_count(&self) -> u64 {
        self.blocks
    }
    // After check_request, start + lba lies inside the disk, so it cannot
    // overflow.
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        check_request(self.block_size(), self.blocks, lba, buf.len())?;
        self.dev.read(self.start + lba, buf)
    }
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        check_request(self.block_size(), self.blocks, lba, buf.len())?;
        self.dev.write(self.start + lba, buf)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.dev.flush()
    }
}

````

- [ ] **Step 8: Implement `kernel/src/block/root.rs`**

Insert this at the top of `kernel/src/block/root.rs`, above `#[cfg(test)]`:

````rust
//! Choosing the root partition (spec §6.5): the Linux filesystem partition on
//! the disk we booted from, found by the boot partition's GUID from
//! `BootInfo`, with a fallback for firmware that does not report it.

use super::gpt::{ESP_TYPE, Gpt, GptPartition, Guid, LINUX_FS_TYPE};
use core::fmt;

/// The partition to mount as `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootChoice {
    /// Index into the slice given to choose_root.
    pub disk: usize,
    pub partition: GptPartition,
    /// No disk had the boot partition; this is the fallback of spec §6.5 (the caller logs a warning).
    pub fallback: bool,
}

/// Why no root partition was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoRoot {
    /// No disk has a readable GPT.
    NoDisks,
    /// The boot disk was found but has no Linux filesystem partition.
    NoLinuxPartition,
    /// No disk has the boot partition, and none has exactly one ESP and one Linux partition.
    NoMatch,
}

impl fmt::Display for NoRoot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            NoRoot::NoDisks => "no disk with a GPT",
            NoRoot::NoLinuxPartition => "no Linux partition on the boot disk",
            NoRoot::NoMatch => "no disk with the boot partition",
        })
    }
}

/// The partitions of `gpt` with type `type_guid`, in entry order.
fn of_type(gpt: &Gpt, type_guid: Guid) -> impl Iterator<Item = &GptPartition> {
    gpt.partitions
        .iter()
        .filter(move |p| p.type_guid == type_guid)
}

/// Spec §6.5: the disk whose GPT has a partition with unique GUID `boot` (from BootInfo), and on
/// it the first Linux filesystem partition. Otherwise (no boot GUID, or no disk has it) the first disk
/// with exactly one ESP partition and exactly one Linux filesystem partition (other partition types
/// do not matter), with `fallback` set. Disks whose GPT could not be read are `None`.
///
/// A zero `boot` GUID is treated as none: it would only match corrupt entries.
/// If several disks have the boot partition (a stick copied with `dd`), the
/// first one wins.
pub fn choose_root(disks: &[Option<Gpt>], boot: Option<Guid>) -> Result<RootChoice, NoRoot> {
    let readable = || {
        disks
            .iter()
            .enumerate()
            .filter_map(|(i, d)| Some((i, d.as_ref()?)))
    };
    if readable().next().is_none() {
        return Err(NoRoot::NoDisks);
    }
    if let Some(boot) = boot.filter(|g| !g.is_zero()) {
        let boot_disk =
            readable().find(|(_, gpt)| gpt.partitions.iter().any(|p| p.unique_guid == boot));
        if let Some((disk, gpt)) = boot_disk {
            let linux = of_type(gpt, LINUX_FS_TYPE)
                .next()
                .ok_or(NoRoot::NoLinuxPartition)?;
            return Ok(RootChoice {
                disk,
                partition: linux.clone(),
                fallback: false,
            });
        }
    }
    readable()
        .find_map(|(disk, gpt)| {
            let mut linux = of_type(gpt, LINUX_FS_TYPE);
            match (linux.next(), linux.next(), of_type(gpt, ESP_TYPE).count()) {
                (Some(partition), None, 1) => Some(RootChoice {
                    disk,
                    partition: partition.clone(),
                    fallback: true,
                }),
                _ => None,
            }
        })
        .ok_or(NoRoot::NoMatch)
}

````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 161 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add kernel
git commit -m "kernel: partitions as block devices and choosing the root partition"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 11 scenario(s) passed`.

````bash
git push -u origin plan5/block
gh pr create --base main --head plan5/block --title "Plan 5: GPT and the root partition" --body-file - <<'EOF'
## What

Milestone 1, plan 5, tasks 10–11: CRC32; `read_gpt` with every header and entry-array check of UEFI 2.10 §5.3, the backup header when the primary or its array is bad, a 1 MiB bound on the array; `Partition` as a bounded block device; `choose_root` by the boot partition GUID with the fallback of spec §6.5.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests and images partitioned by `sfdisk` and `fdisk`

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan5/block --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-block
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: Reboot and poweroff (Tasks 12–17)

`reboot` and `poweroff` through the ACPI registers (spec §7.4) with test mode's exit, and the e2e runner's `reboot` and `poweroff` steps and `e2fsck` after every scenario (spec §9.3).

Branch `plan5/power`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-power`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan5/power /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-power origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-power
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan5/block` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan5/power /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-power plan5/block`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan5/block>` and re-run `cargo xtask ci` before pushing.

### Task 12: Reboot and poweroff

Spec §7.4, after the shell has shut the filesystems down. `reboot` writes the FADT's reset register when it names one this kernel can reach (I/O or memory space, whole bytes from bit 0), then 0x06 to port 0xCF9, then triple-faults with an empty interrupt table, giving each half a second. `poweroff` writes `SLP_TYPa | SLP_EN` into PM1a control, and PM1b's type into PM1b, from the `\_S5` values, keeping the register's other bits as Linux does; in test mode (`test=1`) it first writes 0x10 to QEMU's `isa-debug-exit` at 0xF4, so QEMU exits with status 33. If there is no FADT, no `\_S5` or no usable PM1a register, or the machine is still on after half a second, it says why and prints `System halted. It is now safe to power off.` (decision 7). Which registers to write, and what, is worked out in functions tested on the host; the port and memory writes are thin glue. Both print a line first (`relay: restarting`, `relay: powering off`), the last thing a photo shows.

**Files:**
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/power.rs`
- Modify: `kernel/src/session.rs`

**Interfaces:**
- Consumes: `acpi::{get, tables::{Fadt, GenericAddress, AddressSpace}, aml::SleepType}` (plan 2); `mm::map_mmio`, `timer::sleep`.
- Produces: `relay_kernel::power::{Reg { Io, Memory } (from_gas), ResetMethod { Register, ResetControl, TripleFault }, reset_methods(Option<&Fadt>) -> Vec<ResetMethod>, pm1_sleep_value(u16, u8) -> u16, SoftOff { pm1a, pm1b }, soft_off(Option<&Fadt>, Option<SleepType>) -> Result<SoftOff, &'static str>, reboot() -> !, poweroff(test_mode: bool) -> !, TEST_EXIT_CODE = 0x10}`; `session::KernelSystem { test_mode }`; `session::run_shell(test_mode: bool)`.

- [ ] **Step 1: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod pci;
pub mod rtc;
````

with:

````rust
pub mod pci;
pub mod power;
pub mod rtc;
````

- [ ] **Step 2: Write the failing tests for `kernel/src/power.rs`**

Create `kernel/src/power.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn gas(space: AddressSpace, bits: u8, access: u8, address: u64) -> GenericAddress {
        GenericAddress {
            space,
            bit_width: bits,
            bit_offset: 0,
            access_size: access,
            address,
        }
    }

    fn fadt(reset: Option<(GenericAddress, u8)>) -> Fadt {
        Fadt {
            dsdt: 0,
            pm1a_cnt: Some(gas(AddressSpace::Io, 16, 0, 0x604)),
            pm1b_cnt: None,
            reset,
            century: 0,
        }
    }

    #[test]
    fn registers_take_their_width_from_the_access_size_or_the_bit_width() {
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 8, 1, 0xCF9)),
            Some(Reg::Io {
                port: 0xCF9,
                bytes: 1
            })
        );
        // QEMU's PM1a control block: 16 bits, access size unset.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 16, 0, 0x604)),
            Some(Reg::Io {
                port: 0x604,
                bytes: 2
            })
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 8, 3, 0xFED0_0000)),
            Some(Reg::Memory {
                phys: 0xFED0_0000,
                bytes: 4
            })
        );
    }

    #[test]
    fn registers_this_kernel_cannot_reach_are_refused() {
        for g in [
            gas(AddressSpace::Other(2), 8, 1, 0xCF9), // PCI config space
            gas(AddressSpace::Io, 8, 1, 0x1_0000),    // beyond the I/O space
            gas(AddressSpace::Io, 8, 1, 0),           // not described
            gas(AddressSpace::Io, 12, 0, 0xCF9),      // not whole bytes
            gas(AddressSpace::Io, 8, 5, 0xCF9),       // reserved access size
            gas(AddressSpace::Io, 64, 0, 0xCF9),      // no 64-bit port access
            gas(AddressSpace::Memory, 0, 0, 0xFED0_0000), // no width
        ] {
            assert_eq!(Reg::from_gas(&g), None, "{g:?}");
        }
        let mut g = gas(AddressSpace::Io, 8, 1, 0xCF9);
        g.bit_offset = 1;
        assert_eq!(Reg::from_gas(&g), None);
    }

    #[test]
    fn reboot_tries_the_reset_register_then_port_cf9_then_a_triple_fault() {
        // QEMU's FADT: reset io 0xcf9 <- 0xf.
        let f = fadt(Some((gas(AddressSpace::Io, 8, 1, 0xCF9), 0x0F)));
        assert_eq!(
            reset_methods(Some(&f)),
            [
                ResetMethod::Register(
                    Reg::Io {
                        port: 0xCF9,
                        bytes: 1
                    },
                    0x0F
                ),
                ResetMethod::ResetControl,
                ResetMethod::TripleFault,
            ]
        );
        let unusable = fadt(Some((gas(AddressSpace::Other(2), 8, 1, 0xCF9), 0x06)));
        for f in [None, Some(&fadt(None)), Some(&unusable)] {
            assert_eq!(
                reset_methods(f),
                [ResetMethod::ResetControl, ResetMethod::TripleFault]
            );
        }
    }

    #[test]
    fn the_sleep_value_sets_the_type_and_enable_and_keeps_the_rest() {
        assert_eq!(pm1_sleep_value(0, 0), SLP_EN);
        // The NUC's S5 type is 7.
        assert_eq!(pm1_sleep_value(0, 7), 0x3C00);
        // SCI_EN (bit 0) stays; an old sleep type is replaced.
        assert_eq!(
            pm1_sleep_value(0x0001 | 5 << 10, 2),
            0x0001 | 2 << 10 | SLP_EN
        );
        // Only three bits of type exist.
        assert_eq!(pm1_sleep_value(0, 0xFF), 0x3C00);
    }

    #[test]
    fn poweroff_writes_the_s5_types_into_pm1a_and_pm1b() {
        let s5 = SleepType { a: 7, b: 3 };
        let mut f = fadt(None);
        assert_eq!(
            soft_off(Some(&f), Some(s5)),
            Ok(SoftOff {
                pm1a: (
                    Reg::Io {
                        port: 0x604,
                        bytes: 2
                    },
                    7
                ),
                pm1b: None,
            })
        );
        f.pm1b_cnt = Some(gas(AddressSpace::Io, 16, 0, 0x608));
        assert_eq!(
            soft_off(Some(&f), Some(s5)).unwrap().pm1b,
            Some((
                Reg::Io {
                    port: 0x608,
                    bytes: 2
                },
                3
            ))
        );
    }

    #[test]
    fn poweroff_says_why_it_cannot() {
        let s5 = Some(SleepType { a: 0, b: 0 });
        assert_eq!(soft_off(None, s5), Err("no FADT"));
        assert_eq!(
            soft_off(Some(&fadt(None)), None),
            Err("no \\_S5 in the DSDT")
        );
        let mut f = fadt(None);
        f.pm1a_cnt = None;
        assert_eq!(
            soft_off(Some(&f), s5),
            Err("no usable PM1a control register")
        );
    }

    #[test]
    fn the_test_exit_code_makes_qemu_exit_with_status_33() {
        assert_eq!((TEST_EXIT_CODE as u32) << 1 | 1, 33);
    }
}
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `AddressSpace` in this scope ``; `` cannot find type `GenericAddress` in this scope ``.

- [ ] **Step 4: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    console::fail("mount /", format_args!("no storage driver yet"));
    session::run_shell()
}
````

with:

````rust
    console::fail("mount /", format_args!("no storage driver yet"));
    session::run_shell(cmdline.test_mode)
}
````

- [ ] **Step 5: Implement `kernel/src/power.rs`**

Insert this at the top of `kernel/src/power.rs`, above `#[cfg(test)]`:

````rust
//! Restarting and switching off (spec §7.4), through the registers the
//! FADT describes and the `\_S5` values of the DSDT. The shell has already
//! shut the filesystems down when these run. Which registers to write, and
//! what, is worked out here and tested on the host; the writes themselves
//! are thin glue.

use crate::acpi::aml::SleepType;
use crate::acpi::tables::{AddressSpace, Fadt, GenericAddress};
use crate::mm::{self, paging::Cache};
use crate::{acpi, arch, console, kprintln, timer};
use alloc::vec::Vec;
use core::time::Duration;
use x86_64::instructions::port::Port;

/// PM1 control: the sleep type field and the sleep enable bit (ACPI 6.5
/// table 4.13).
pub const SLP_TYP_SHIFT: u16 = 10;
pub const SLP_TYP_MASK: u16 = 7 << SLP_TYP_SHIFT;
pub const SLP_EN: u16 = 1 << 13;
/// The reset control register of Intel chipsets (and QEMU's): 0x06 asks
/// for a full reset.
pub const RESET_CONTROL_PORT: u16 = 0xCF9;
pub const RESET_CONTROL_VALUE: u8 = 0x06;
/// QEMU's `isa-debug-exit` device (the e2e machine has it at 0xF4).
pub const DEBUG_EXIT_PORT: u16 = 0xF4;
/// What `poweroff` writes there in test mode: QEMU then exits with status
/// (0x10 << 1) | 1 = 33, which the e2e runner takes as a clean power-off.
pub const TEST_EXIT_CODE: u8 = 0x10;
/// How long each way of restarting or switching off gets before the next.
const ATTEMPT_TIME: Duration = Duration::from_millis(500);

/// A register the firmware describes, and the width of one access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reg {
    Io { port: u16, bytes: u8 },
    Memory { phys: u64, bytes: u8 },
}

impl Reg {
    /// The register a Generic Address Structure names, if this kernel can
    /// reach it: I/O or memory space, a whole number of bytes starting at
    /// bit 0, 1, 2, 4 or 8 bytes wide (the access size field wins over the
    /// bit width when it is set, ACPI 6.5 §5.2.3.2).
    pub fn from_gas(g: &GenericAddress) -> Option<Reg> {
        let bytes = match g.access_size {
            0 if g.bit_width.is_multiple_of(8) => g.bit_width / 8,
            0 => return None,
            n @ 1..=4 => 1 << (n - 1),
            _ => return None,
        };
        if g.bit_offset != 0 || g.address == 0 || !matches!(bytes, 1 | 2 | 4 | 8) {
            return None;
        }
        match g.space {
            AddressSpace::Io if bytes <= 4 => Some(Reg::Io {
                port: u16::try_from(g.address).ok()?,
                bytes,
            }),
            AddressSpace::Memory => Some(Reg::Memory {
                phys: g.address,
                bytes,
            }),
            _ => None,
        }
    }
}

/// One way to restart the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMethod {
    /// The FADT's reset register and the value to write to it.
    Register(Reg, u8),
    /// Port 0xCF9 (spec §7.4's fallback).
    ResetControl,
    /// An empty interrupt table and an exception: always resets.
    TripleFault,
}

/// The ways to restart, in the order spec §7.4 tries them.
pub fn reset_methods(fadt: Option<&Fadt>) -> Vec<ResetMethod> {
    let mut methods = Vec::new();
    if let Some((gas, value)) = fadt.and_then(|f| f.reset)
        && let Some(reg) = Reg::from_gas(&gas)
    {
        methods.push(ResetMethod::Register(reg, value));
    }
    methods.push(ResetMethod::ResetControl);
    methods.push(ResetMethod::TripleFault);
    methods
}

/// PM1 control after asking for sleep type `slp_typ` (the `\_S5` value):
/// the other bits are kept, as Linux does.
pub fn pm1_sleep_value(current: u16, slp_typ: u8) -> u16 {
    current & !(SLP_TYP_MASK | SLP_EN) | ((slp_typ as u16) << SLP_TYP_SHIFT) & SLP_TYP_MASK | SLP_EN
}

/// The PM1 control registers and the sleep types that switch the machine
/// off (S5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftOff {
    pub pm1a: (Reg, u8),
    pub pm1b: Option<(Reg, u8)>,
}

/// What `poweroff` writes, or why it cannot (the reason is logged, and the
/// machine halts with the safe-to-power-off message).
pub fn soft_off(fadt: Option<&Fadt>, s5: Option<SleepType>) -> Result<SoftOff, &'static str> {
    let fadt = fadt.ok_or("no FADT")?;
    let s5 = s5.ok_or("no \\_S5 in the DSDT")?;
    let pm1a = fadt
        .pm1a_cnt
        .as_ref()
        .and_then(Reg::from_gas)
        .ok_or("no usable PM1a control register")?;
    let pm1b = fadt.pm1b_cnt.as_ref().and_then(Reg::from_gas);
    Ok(SoftOff {
        pm1a: (pm1a, s5.a),
        pm1b: pm1b.map(|r| (r, s5.b)),
    })
}

fn read(reg: Reg) -> u64 {
    // SAFETY: the firmware describes these registers for this purpose.
    unsafe {
        match reg {
            Reg::Io { port, bytes: 1 } => Port::<u8>::new(port).read() as u64,
            Reg::Io { port, bytes: 2 } => Port::<u16>::new(port).read() as u64,
            Reg::Io { port, .. } => Port::<u32>::new(port).read() as u64,
            Reg::Memory { phys, bytes } => {
                match mm::map_mmio(phys, bytes as u64, Cache::Uncached) {
                    Ok(p) => match bytes {
                        1 => p.read_volatile() as u64,
                        2 => p.cast::<u16>().read_volatile() as u64,
                        4 => p.cast::<u32>().read_volatile() as u64,
                        _ => p.cast::<u64>().read_volatile(),
                    },
                    Err(_) => 0,
                }
            }
        }
    }
}

fn write(reg: Reg, value: u64) {
    // SAFETY: as in `read`.
    unsafe {
        match reg {
            Reg::Io { port, bytes: 1 } => Port::<u8>::new(port).write(value as u8),
            Reg::Io { port, bytes: 2 } => Port::<u16>::new(port).write(value as u16),
            Reg::Io { port, .. } => Port::<u32>::new(port).write(value as u32),
            Reg::Memory { phys, bytes } => {
                if let Ok(p) = mm::map_mmio(phys, bytes as u64, Cache::Uncached) {
                    match bytes {
                        1 => p.write_volatile(value as u8),
                        2 => p.cast::<u16>().write_volatile(value as u16),
                        4 => p.cast::<u32>().write_volatile(value as u32),
                        _ => p.cast::<u64>().write_volatile(value),
                    }
                }
            }
        }
    }
}

/// Restarts the machine (spec §7.4): the FADT reset register, then port
/// 0xCF9, then a triple fault, each given half a second.
pub fn reboot() -> ! {
    kprintln!("relay: restarting");
    x86_64::instructions::interrupts::disable();
    for method in reset_methods(acpi::get().and_then(|a| a.fadt.as_ref())) {
        match method {
            ResetMethod::Register(reg, value) => write(reg, value as u64),
            ResetMethod::ResetControl => {
                write(
                    Reg::Io {
                        port: RESET_CONTROL_PORT,
                        bytes: 1,
                    },
                    RESET_CONTROL_VALUE as u64,
                );
            }
            ResetMethod::TripleFault => {
                // SAFETY: nothing runs after this; an exception with an
                // empty interrupt table resets the CPU.
                unsafe {
                    x86_64::instructions::tables::lidt(
                        &x86_64::structures::DescriptorTablePointer {
                            limit: 0,
                            base: x86_64::VirtAddr::new(0),
                        },
                    );
                    core::arch::asm!("int3");
                }
            }
        }
        timer::sleep(ATTEMPT_TIME);
    }
    arch::halt_forever()
}

/// Switches the machine off (spec §7.4): in test mode QEMU's
/// `isa-debug-exit` first, then `SLP_TYPa | SLP_EN` into PM1a control (and
/// PM1b's value into PM1b). If that is impossible or does not work, the
/// safe-to-power-off message and a halt.
pub fn poweroff(test_mode: bool) -> ! {
    kprintln!("relay: powering off");
    x86_64::instructions::interrupts::disable();
    if test_mode {
        write(
            Reg::Io {
                port: DEBUG_EXIT_PORT,
                bytes: 1,
            },
            TEST_EXIT_CODE as u64,
        );
    }
    let acpi = acpi::get();
    match soft_off(acpi.and_then(|a| a.fadt.as_ref()), acpi.and_then(|a| a.s5)) {
        Ok(off) => {
            for (reg, typ) in [Some(off.pm1a), off.pm1b].into_iter().flatten() {
                let value = pm1_sleep_value(read(reg) as u16, typ);
                write(reg, value as u64);
            }
            timer::sleep(ATTEMPT_TIME);
            kprintln!("relay: the machine did not switch off");
        }
        Err(why) => kprintln!("relay: cannot switch off: {why}"),
    }
    console::write_output(b"System halted. It is now safe to power off.\n");
    arch::halt_forever()
}

````

- [ ] **Step 6: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, rtc, serial, usb};
use alloc::boxed::Box;
````

with:

````rust
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, power, rtc, serial, usb};
use alloc::boxed::Box;
````

Replace:

````rust

pub struct KernelSystem;

````

with:

````rust

pub struct KernelSystem {
    /// `test=1`: `poweroff` makes QEMU exit (spec §7.4).
    pub test_mode: bool,
}

````

Replace:

````rust

    // Restarting and switching off through ACPI come with the storage
    // driver, which the shell must shut down cleanly first; until then both
    // stop the machine.
    fn reboot(&mut self) {
        halt()
    }

    fn poweroff(&mut self) {
        halt()
    }
}

fn halt() -> ! {
    console::write_output(b"System halted. It is now safe to power off.\n");
    arch::halt_forever()
}
````

with:

````rust

    // The shell has shut the filesystems down before these.
    fn reboot(&mut self) {
        power::reboot()
    }

    fn poweroff(&mut self) {
        power::poweroff(self.test_mode)
    }
}
````

Replace:

````rust
/// Runs the shell on an empty read-only `/` (spec §10). Never returns.
pub fn run_shell() -> ! {
    let root = MemFs::new(Box::new(KernelEnv)).read_only();
    let mut vfs = MountTable::new(Box::new(root));
    let mut console = KernelConsole::new();
    let mut system = KernelSystem;
    Shell::new(&mut vfs, &mut console, &mut system).run();
````

with:

````rust
/// Runs the shell on an empty read-only `/` (spec §10). Never returns.
pub fn run_shell(test_mode: bool) -> ! {
    let root = MemFs::new(Box::new(KernelEnv)).read_only();
    let mut vfs = MountTable::new(Box::new(root));
    let mut console = KernelConsole::new();
    let mut system = KernelSystem { test_mode };
    Shell::new(&mut vfs, &mut console, &mut system).run();
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 168 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add kernel
git commit -m "kernel: reboot and poweroff through ACPI, and the test-mode exit"
````


### Task 13: A misaligned ACPI memory register is not used

Review finding (minor): a Generic Address Structure in memory space whose address is not a multiple of its access width would make the volatile read or write fault, and with the `relay` profile's debug assertions it fails Rust's alignment check: a `reboot` or `poweroff` through such a register would end on the panic screen, and no firmware value may do that (spec §10). `Reg::from_gas` refuses it; I/O ports have no alignment rule (decision 7).

**Files:**
- Modify: `kernel/src/power.rs`

**Interfaces:**
- Consumes: Task 12.
- Produces: `Reg::from_gas` returns `None` for a memory register not aligned to its width.

- [ ] **Step 1: Add the failing tests to `kernel/src/power.rs`**

In `kernel/src/power.rs`, replace:

````rust
    #[test]
    fn reboot_tries_the_reset_register_then_port_cf9_then_a_triple_fault() {
````

with:

````rust
    #[test]
    fn a_memory_register_off_its_alignment_is_refused() {
        // A volatile access to it would fault (and fails Rust's alignment
        // check): firmware values must never do that.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 32, 3, 0xFED0_0001)),
            None
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 16, 2, 0xFED0_0003)),
            None
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 16, 2, 0xFED0_0002)),
            Some(Reg::Memory {
                phys: 0xFED0_0002,
                bytes: 2
            })
        );
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Memory, 8, 1, 0xFED0_0003)),
            Some(Reg::Memory {
                phys: 0xFED0_0003,
                bytes: 1
            })
        );
        // I/O ports have no alignment rule.
        assert_eq!(
            Reg::from_gas(&gas(AddressSpace::Io, 16, 2, 0x605)),
            Some(Reg::Io {
                port: 0x605,
                bytes: 2
            })
        );
    }

    #[test]
    fn reboot_tries_the_reset_register_then_port_cf9_then_a_triple_fault() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: 1 test fails: `power::tests::a_memory_register_off_its_alignment_is_refused`.

- [ ] **Step 3: Change `kernel/src/power.rs`**

In `kernel/src/power.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    /// The register a Generic Address Structure names, if this kernel can
    /// reach it: I/O or memory space, a whole number of bytes starting at
    /// bit 0, 1, 2, 4 or 8 bytes wide (the access size field wins over the
    /// bit width when it is set, ACPI 6.5 §5.2.3.2).
    pub fn from_gas(g: &GenericAddress) -> Option<Reg> {
````

with:

````rust
    /// The register a Generic Address Structure names, if this kernel can
    /// reach it: I/O or memory space (a memory register aligned to its
    /// width), a whole number of bytes starting at bit 0, 1, 2, 4 or 8
    /// bytes wide (the access size field wins over the bit width when it is
    /// set, ACPI 6.5 §5.2.3.2).
    pub fn from_gas(g: &GenericAddress) -> Option<Reg> {
````

Replace:

````rust
            }),
            AddressSpace::Memory => Some(Reg::Memory {
                phys: g.address,
````

with:

````rust
            }),
            // A misaligned volatile access could fault.
            AddressSpace::Memory if g.address.is_multiple_of(bytes as u64) => Some(Reg::Memory {
                phys: g.address,
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 169 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: a misaligned ACPI memory register is not used"
````


### Task 14: Poweroff sets both sleep types before enabling sleep

Review finding (minor): `poweroff` wrote PM1a with `SLP_EN` before PM1b had its sleep type, so on a machine with a PM1b block S5 could start with PM1b unprogrammed. As Linux's `acpi_hw_legacy_sleep` does, `soft_off_writes` lists the writes in order: the sleep types into PM1a and PM1b, then the same values with `SLP_EN`. The NUC and QEMU have no PM1b (decision 7).

**Files:**
- Modify: `kernel/src/power.rs`

**Interfaces:**
- Consumes: Tasks 12 and 13.
- Produces: `power::soft_off_writes(&SoftOff, current_a: u16, current_b: u16) -> Vec<(Reg, u16)>`.

- [ ] **Step 1: Add the failing tests to `kernel/src/power.rs`**

In `kernel/src/power.rs`, replace:

````rust
    #[test]
    fn poweroff_says_why_it_cannot() {
````

with:

````rust
    #[test]
    fn both_sleep_types_are_written_before_either_enable() {
        let a = Reg::Io {
            port: 0x604,
            bytes: 2,
        };
        let b = Reg::Io {
            port: 0x608,
            bytes: 2,
        };
        let off = SoftOff {
            pm1a: (a, 7),
            pm1b: Some((b, 3)),
        };
        // As Linux's acpi_hw_legacy_sleep: SLP_TYP to both, then SLP_EN.
        assert_eq!(
            soft_off_writes(&off, 0x0001, 0x0001),
            [
                (a, 0x0001 | 7 << 10),
                (b, 0x0001 | 3 << 10),
                (a, 0x0001 | 7 << 10 | SLP_EN),
                (b, 0x0001 | 3 << 10 | SLP_EN),
            ]
        );
        let only_a = SoftOff {
            pm1a: (a, 0),
            pm1b: None,
        };
        assert_eq!(soft_off_writes(&only_a, 0, 0), [(a, 0), (a, SLP_EN)]);
    }

    #[test]
    fn poweroff_says_why_it_cannot() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `soft_off_writes` in this scope ``.

- [ ] **Step 3: Change `kernel/src/power.rs`**

In `kernel/src/power.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub pm1b: Option<(Reg, u8)>,
}
````

with:

````rust
    pub pm1b: Option<(Reg, u8)>,
}

/// The writes that switch the machine off, given what PM1a and PM1b
/// control hold: the sleep types into both, then the same with `SLP_EN`, so
/// PM1b is set up before PM1a starts the sleep (as Linux's
/// `acpi_hw_legacy_sleep` does).
pub fn soft_off_writes(off: &SoftOff, current_a: u16, current_b: u16) -> Vec<(Reg, u16)> {
    let regs = [
        Some((off.pm1a, current_a)),
        off.pm1b.map(|b| (b, current_b)),
    ];
    let mut writes = Vec::new();
    for enable in [0, SLP_EN] {
        for ((reg, typ), current) in regs.iter().flatten() {
            writes.push((*reg, pm1_sleep_value(*current, *typ) & !SLP_EN | enable));
        }
    }
    writes
}
````

Replace:

````rust
        Ok(off) => {
            for (reg, typ) in [Some(off.pm1a), off.pm1b].into_iter().flatten() {
                let value = pm1_sleep_value(read(reg) as u16, typ);
                write(reg, value as u64);
````

with:

````rust
        Ok(off) => {
            let current_a = read(off.pm1a.0) as u16;
            let current_b = off.pm1b.map_or(0, |(reg, _)| read(reg) as u16);
            for (reg, value) in soft_off_writes(&off, current_a, current_b) {
                write(reg, value as u64);
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 170 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: poweroff sets both sleep types before enabling sleep"
````


### Task 15: The e2e steps reboot and poweroff, and e2fsck after every scenario

Spec §9.3's runner additions. `reboot` types `reboot`, waits for QEMU to exit as it does after a guest reset under `-no-reboot` (status 0), and starts it again on the same disk and firmware variables, continuing the serial log. `poweroff` types `poweroff` and waits for the test-mode exit through `isa-debug-exit` (status 33); later steps that talk to the machine then fail. After every scenario QEMU is stopped and `e2fsck -fn` must pass on the ext2 partition of the disk it leaves; a filesystem that is only marked not clean (the machine was killed with `/` mounted) passes, as e2fsck 1.47 treats it. The new `power` scenario reboots and switches off; until Task 18 it runs on the empty read-only `/`.

**Files:**
- Create: `tests/e2e/power.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Task 12; `image::fsck`, `qemu::Qemu` (plans 1 and 3).
- Produces: e2e steps `reboot` and `poweroff`; `e2e::{EXIT_RESET = 0, EXIT_POWEROFF = 33}`; `e2fsck -fn` after every scenario; scenario `tests/e2e/power.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/power.txt`**

Create `tests/e2e/power.txt`:

````text
# reboot and poweroff (spec §7.4): the machine restarts through the FADT
# reset register and boots again on the same disk, then test mode's
# poweroff makes QEMU exit through isa-debug-exit.
timeout 30
expect root@relay:/# $
send echo before
expect \nbefore\n
reboot
expect Relay OS \d+\.\d+\.\d+
expect root@relay:/# $
send echo after
expect \nafter\n
poweroff
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_alive_step() {
````

with:

````rust
    #[test]
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario("x", "reboot\npoweroff").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Reboot), (2, Step::Poweroff)]);
    }

    #[test]
    fn exit_statuses_match_the_kernel() {
        // relay_kernel::power::TEST_EXIT_CODE is 0x10.
        assert_eq!(EXIT_POWEROFF, (0x10 << 1) | 1);
    }

    #[test]
    fn parses_alive_step() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find value `EXIT_POWEROFF` in this scope ``; `` no variant, associated function, or constant named `Reboot` found for enum `e2e::Step` in the current scope ``.

- [ ] **Step 4: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 13 replacements, top to bottom:

Replace:

````rust
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! ```
//!
//! QEMU is killed at the end of every scenario.

````

with:

````rust
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! reboot                           (types `reboot`; QEMU must exit as after a
//!                                   reset, then starts again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
//!                                   isa-debug-exit, test mode's power-off)
//! ```
//!
//! QEMU is killed at the end of every scenario, and `e2fsck -fn` must find
//! the ext2 root on the disk it leaves clean (spec §9.3).

````

Replace:

````rust
const KEY_SETTLE_MS: u64 = 100;

````

with:

````rust
const KEY_SETTLE_MS: u64 = 100;
/// QEMU's exit status after a guest reset under `-no-reboot`.
pub const EXIT_RESET: i32 = 0;
/// QEMU's exit status after test mode's `poweroff` wrote 0x10 to
/// `isa-debug-exit`: (0x10 << 1) | 1.
pub const EXIT_POWEROFF: i32 = 33;

````

Replace:

````rust
    },
}
````

with:

````rust
    },
    /// Restart the machine and boot again on the same disk.
    Reboot,
    /// Switch the machine off; no later step talks to it.
    Poweroff,
}
````

Replace:

````rust
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
````

with:

````rust
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "reboot" => Step::Reboot,
            "poweroff" => Step::Poweroff,
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
````

Replace:

````rust
struct Running {
    child: Child,
````

with:

````rust
struct Running {
    /// The machine, kept to start it again after a reboot.
    qemu: Qemu,
    run_dir: PathBuf,
    child: Child,
````

Replace:

````rust
    consumed: usize,
}
````

with:

````rust
    consumed: usize,
    /// The machine switched itself off.
    off: bool,
}
````

Replace:

````rust
    }
    let qmp_name = qemu::qmp_name(&scenario.name);
    q.headless = true;
    q.qmp_name = Some(qmp_name.clone());
    let mut child = q
````

with:

````rust
    }
    q.headless = true;
    q.qmp_name = Some(qemu::qmp_name(&scenario.name));
    launch(q, run_dir, fs::File::create(run_dir.join("serial.log"))?)
}

/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let qmp_name = q.qmp_name.clone().context("QMP socket name")?;
    let mut child = q
````

Replace:

````rust
    let sink = serial.clone();
    let mut log = fs::File::create(run_dir.join("serial.log"))?;
    std::thread::spawn(move || {
````

with:

````rust
    let sink = serial.clone();
    std::thread::spawn(move || {
````

Replace:

````rust
    Ok(Running {
        child,
````

with:

````rust
    Ok(Running {
        qemu: q,
        run_dir: run_dir.to_path_buf(),
        child,
````

Replace:

````rust
        consumed: 0,
    })
}
````

with:

````rust
        consumed: 0,
        off: false,
    })
}

/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<()> {
    r.stdin.write_all(command.as_bytes())?;
    r.stdin.write_all(b"\r")?;
    r.stdin.flush()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(s) = r.child.try_wait()? {
            if s.code() != Some(status) {
                bail!("QEMU exited with {s} after `{command}`, expected exit status {status}");
            }
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("QEMU still runs {timeout:?} after `{command}`");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `reboot`: the machine resets, and QEMU (`-no-reboot`) exits; then the
/// same disk boots again, with the serial log continued.
fn reboot(r: &mut Running, timeout: Duration) -> Result<()> {
    exit_with(r, "reboot", EXIT_RESET, timeout)?;
    let mut log = fs::OpenOptions::new()
        .append(true)
        .open(r.run_dir.join("serial.log"))?;
    log.write_all(b"\n--- e2e: reboot ---\n")?;
    let q = Qemu {
        disk: r.qemu.disk.clone(),
        vars: r.qemu.vars.clone(),
        headless: true,
        qmp_name: r.qemu.qmp_name.clone(),
    };
    let fresh = launch(q, &r.run_dir.clone(), log)?;
    *r = fresh;
    Ok(())
}
````

Replace:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    match step {
````

with:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    if r.off && !matches!(step, Step::Timeout(_)) {
        bail!("the machine was switched off by an earlier step");
    }
    match step {
````

Replace:

````rust
        }
        Step::ScreenshotNonblank => {
````

with:

````rust
        }
        Step::Reboot => reboot(r, *timeout)?,
        Step::Poweroff => {
            exit_with(r, "poweroff", EXIT_POWEROFF, *timeout)?;
            r.off = true;
        }
        Step::ScreenshotNonblank => {
````

Replace:

````rust
            );
        }
    }
    Ok(())
}

pub fn load_scenarios(only: Option<&str>) -> Result<Vec<Scenario>> {
````

with:

````rust
            );
        }
    }
    let disk = r.qemu.disk.clone();
    drop(r);
    image::fsck(&disk, layout.root).with_context(|| {
        format!(
            "scenario '{}': e2fsck -fn on the disk it left",
            scenario.name
        )
    })
}

pub fn load_scenarios(only: Option<&str>) -> Result<Vec<Scenario>> {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 41 tests.

- [ ] **Step 6: Run the power scenario and all the others**

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only`

Expected: PASS: ends with `all 12 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add tests xtask
git commit -m "xtask: the e2e steps reboot and poweroff, and e2fsck after every scenario"
````


### Task 16: Reboot names the way it restarts

Review finding (minor): the `power` scenario said the machine restarts through the FADT reset register, but QEMU exits the same way after port 0xCF9 or the triple fault half a second later, so a broken reset register would have gone unnoticed. `reboot` now prints `relay: restarting through <method>` before each way it tries (the last such line on a photo is the one that worked), and the e2e `reboot` step takes an optional pattern that must appear before the machine goes down: `reboot relay: restarting through the FADT reset register` (decisions 7 and 9).

**Files:**
- Modify: `kernel/src/power.rs`
- Modify: `tests/e2e/power.txt`
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Tasks 12 and 15.
- Produces: `impl Display for power::ResetMethod`; e2e step `reboot [<regex>]` (`Step::Reboot(Option<String>)`).

- [ ] **Step 1: Add the failing tests to `kernel/src/power.rs`**

In `kernel/src/power.rs`, replace:

````rust
    #[test]
    fn the_sleep_value_sets_the_type_and_enable_and_keeps_the_rest() {
````

with:

````rust
    #[test]
    fn each_way_of_restarting_is_named_on_the_screen() {
        let reg = Reg::Io {
            port: 0xCF9,
            bytes: 1,
        };
        assert_eq!(
            ResetMethod::Register(reg, 0x0F).to_string(),
            "the FADT reset register"
        );
        assert_eq!(ResetMethod::ResetControl.to_string(), "port 0xcf9");
        assert_eq!(ResetMethod::TripleFault.to_string(), "a triple fault");
    }

    #[test]
    fn the_sleep_value_sets_the_type_and_enable_and_keeps_the_rest() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/power.txt`**

Replace the whole of `tests/e2e/power.txt` with:

````text
# reboot and poweroff (spec §7.4): the machine restarts through the FADT
# reset register (QEMU's is port 0xcf9 <- 0xf) and boots again on the same
# disk, then test mode's poweroff makes QEMU exit through isa-debug-exit.
timeout 30
expect root@relay:/# $
send echo before
expect \nbefore\n
reboot relay: restarting through the FADT reset register
expect Relay OS \d+\.\d+\.\d+
expect root@relay:/# $
send echo after
expect \nafter\n
poweroff
````

- [ ] **Step 3: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario("x", "reboot\npoweroff").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Reboot), (2, Step::Poweroff)]);
    }
````

with:

````rust
    fn parses_reboot_and_poweroff_steps() {
        let s = parse_scenario("x", "reboot\nreboot relay: restarting\npoweroff").unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Reboot(None)),
                (2, Step::Reboot(Some("relay: restarting".into()))),
                (3, Step::Poweroff)
            ]
        );
        assert!(parse_scenario("x", "reboot (").is_err());
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` `power::ResetMethod` doesn't implement `std::fmt::Display` ``.

- [ ] **Step 5: Change `kernel/src/power.rs`**

In `kernel/src/power.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use alloc::vec::Vec;
use core::time::Duration;
````

with:

````rust
use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;
````

Replace:

````rust
    TripleFault,
}
````

with:

````rust
    TripleFault,
}

/// How the screen names each way: the last line before a reset is the one
/// that worked.
impl fmt::Display for ResetMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            ResetMethod::Register(..) => "the FADT reset register",
            ResetMethod::ResetControl => "port 0xcf9",
            ResetMethod::TripleFault => "a triple fault",
        })
    }
}
````

Replace:

````rust
/// Restarts the machine (spec §7.4): the FADT reset register, then port
/// 0xCF9, then a triple fault, each given half a second.
pub fn reboot() -> ! {
    kprintln!("relay: restarting");
    x86_64::instructions::interrupts::disable();
    for method in reset_methods(acpi::get().and_then(|a| a.fadt.as_ref())) {
        match method {
````

with:

````rust
/// Restarts the machine (spec §7.4): the FADT reset register, then port
/// 0xCF9, then a triple fault, each given half a second and named on the
/// screen first.
pub fn reboot() -> ! {
    x86_64::instructions::interrupts::disable();
    for method in reset_methods(acpi::get().and_then(|a| a.fadt.as_ref())) {
        kprintln!("relay: restarting through {method}");
        match method {
````

- [ ] **Step 6: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! reboot                           (types `reboot`; QEMU must exit as after a
//!                                   reset, then starts again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
````

with:

````rust
//! screenshot-pixel 2540 20 #000000 (QMP screendump; that pixel has that colour)
//! reboot [<regex>]                 (types `reboot`; QEMU must exit as after a
//!                                   reset, having printed <regex> first, then
//!                                   starts again on the same disk)
//! poweroff                         (types `poweroff`; QEMU must exit through
````

Replace:

````rust
    },
    /// Restart the machine and boot again on the same disk.
    Reboot,
    /// Switch the machine off; no later step talks to it.
````

with:

````rust
    },
    /// Restart the machine and boot again on the same disk; the pattern
    /// must appear in what the machine printed before it went down.
    Reboot(Option<String>),
    /// Switch the machine off; no later step talks to it.
````

Replace:

````rust
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "reboot" => Step::Reboot,
            "poweroff" => Step::Poweroff,
````

with:

````rust
            "alive" => Step::Alive(rest.parse().with_context(|| format!("{name}:{line_no}"))?),
            "reboot" if rest.is_empty() => Step::Reboot(None),
            "reboot" => {
                Regex::new(rest).with_context(|| format!("{name}:{line_no}: bad regex"))?;
                Step::Reboot(Some(rest.to_string()))
            }
            "poweroff" => Step::Poweroff,
````

Replace:

````rust

/// `reboot`: the machine resets, and QEMU (`-no-reboot`) exits; then the
/// same disk boots again, with the serial log continued.
fn reboot(r: &mut Running, timeout: Duration) -> Result<()> {
    exit_with(r, "reboot", EXIT_RESET, timeout)?;
    let mut log = fs::OpenOptions::new()
````

with:

````rust

/// `reboot`: the machine resets, and QEMU (`-no-reboot`) exits, after
/// printing `last` if given; then the same disk boots again, with the serial
/// log continued.
fn reboot(r: &mut Running, last: Option<&str>, timeout: Duration) -> Result<()> {
    exit_with(r, "reboot", EXIT_RESET, timeout)?;
    if let Some(pattern) = last {
        let text = r.text();
        if !Regex::new(pattern)?.is_match(&text[r.consumed.min(text.len())..]) {
            bail!("the machine restarted without printing /{pattern}/");
        }
    }
    let mut log = fs::OpenOptions::new()
````

Replace:

````rust
        }
        Step::Reboot => reboot(r, *timeout)?,
        Step::Poweroff => {
````

with:

````rust
        }
        Step::Reboot(last) => reboot(r, last.as_deref(), *timeout)?,
        Step::Poweroff => {
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 171 tests.

Run: `cargo test -p xtask`

Expected: PASS: 41 tests.

- [ ] **Step 8: Run the power scenario**

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add kernel tests xtask
git commit -m "kernel: reboot names the way it restarts, and the power scenario checks it"
````


### Task 17: Reboot and poweroff must leave the root filesystem clean

Review finding (minor): `e2fsck -fn` passes a consistent filesystem that is only marked not clean, so nothing checked that the shell's shutdown before `reboot` and `poweroff` set the clean flag again. After QEMU exits from either, the runner reads the root's state with `dumpe2fs -h` and requires `clean` (decision 9). Until Task 18 the disk is never mounted, so the check holds trivially; from then on `persist`, `fileops` and the others depend on it.

**Files:**
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: Task 15.
- Produces: `image::ext2_state(target, Partition) -> Result<String>`; the e2e `reboot` and `poweroff` steps require `Filesystem state: clean`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn exit_statuses_match_the_kernel() {
````

with:

````rust
    #[test]
    fn the_clean_flag_is_read_from_the_superblock() {
        let dir = out_dir().join("e2e-selftest").join("clean-flag");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2"])
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        assert_eq!(image::ext2_state(&img, whole).unwrap(), "clean");
        // As a machine killed with / mounted read-write leaves it.
        crate::util::run(
            std::process::Command::new("debugfs")
                .args(["-w", "-R", "ssv state 0"])
                .arg(&img),
        )
        .unwrap();
        assert_eq!(image::ext2_state(&img, whole).unwrap(), "not clean");
    }

    #[test]
    fn exit_statuses_match_the_kernel() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `Partition` in this scope ``; `` cannot find function `ext2_state` in module `image` ``.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
use crate::build;
use crate::image::{self, Layout, esp_write, set_cmdline};
use crate::keys;
````

with:

````rust
use crate::build;
use crate::image::{self, Layout, Partition, esp_write, set_cmdline};
use crate::keys;
````

Replace:

````rust
    qemu: Qemu,
    run_dir: PathBuf,
````

with:

````rust
    qemu: Qemu,
    /// Where the ext2 root is on the disk.
    root: Partition,
    run_dir: PathBuf,
````

Replace:

````rust
    q.qmp_name = Some(qemu::qmp_name(&scenario.name));
    launch(q, run_dir, fs::File::create(run_dir.join("serial.log"))?)
}

/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let qmp_name = q.qmp_name.clone().context("QMP socket name")?;
````

with:

````rust
    q.qmp_name = Some(qemu::qmp_name(&scenario.name));
    let log = fs::File::create(run_dir.join("serial.log"))?;
    launch(q, layout.root, run_dir, log)
}

/// Starts QEMU on the prepared disk; its serial output goes to `log` too.
fn launch(q: Qemu, root: Partition, run_dir: &Path, mut log: fs::File) -> Result<Running> {
    let qmp_name = q.qmp_name.clone().context("QMP socket name")?;
````

Replace:

````rust
        qemu: q,
        run_dir: run_dir.to_path_buf(),
````

with:

````rust
        qemu: q,
        root,
        run_dir: run_dir.to_path_buf(),
````

Replace:

````rust
/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<()> {
````

with:

````rust
/// Types `command` and waits (up to `timeout`) for QEMU to exit with
/// `status`; the shell shut the filesystem down first, so it must be marked
/// clean.
fn exit_with(r: &mut Running, command: &str, status: i32, timeout: Duration) -> Result<()> {
````

Replace:

````rust
                bail!("QEMU exited with {s} after `{command}`, expected exit status {status}");
            }
````

with:

````rust
                bail!("QEMU exited with {s} after `{command}`, expected exit status {status}");
            }
            let state = image::ext2_state(&r.qemu.disk, r.root)?;
            if state != "clean" {
                bail!("after `{command}` the root filesystem is `{state}`, not `clean`");
            }
````

Replace:

````rust
    };
    let fresh = launch(q, &r.run_dir.clone(), log)?;
    *r = fresh;
````

with:

````rust
    };
    let fresh = launch(q, r.root, &r.run_dir.clone(), log)?;
    *r = fresh;
````

- [ ] **Step 4: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust

/// `e2fsck -fn`: read-only full check. Returns the checker output on failure.
````

with:

````rust

/// The ext2 filesystem state in `part`'s superblock, as `dumpe2fs -h` names
/// it: `clean` after a clean shutdown, `not clean` while mounted
/// read-write (or after a machine stopped with it mounted).
pub fn ext2_state(target: &Path, part: Partition) -> Result<String> {
    let out = run_stdout(
        Command::new("dumpe2fs")
            .arg("-h")
            .arg(e2fs_target(target, part)),
    )?;
    out.lines()
        .find_map(|l| l.strip_prefix("Filesystem state:"))
        .map(|s| s.trim().to_string())
        .context("dumpe2fs printed no filesystem state")
}

/// `e2fsck -fn`: read-only full check. Returns the checker output on failure.
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 42 tests.

- [ ] **Step 6: Run the power scenario**

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add xtask
git commit -m "xtask: reboot and poweroff must leave the root filesystem clean"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 12 scenario(s) passed`.

````bash
git push -u origin plan5/power
gh pr create --base main --head plan5/power --title "Plan 5: Reboot and poweroff" --body-file - <<'EOF'
## What

Milestone 1, plan 5, tasks 12–17: `reboot` through the FADT reset register, port 0xCF9 and a triple fault; `poweroff` through PM1a/PM1b with the `\_S5` types, and `isa-debug-exit` first in test mode; memory registers aligned to their width; both sleep types written before `SLP_EN`; `reboot` naming its method; the e2e steps `reboot [<regex>]` (relaunch on the same disk) and `poweroff`, both requiring a clean filesystem, `e2fsck -fn` after every scenario; the `power` scenario.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests and the `power` scenario

## Hardware

- [x] Not needed yet: NUC check 3 (PR 6) runs `reboot` and `poweroff` on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan5/power --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-power
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 6: / on the USB stick (Tasks 18–22)

The kernel mounts ext2 from the stick at `/`, and the QEMU scenarios of spec §9.3 run on it; NUC check 3 is the full hardware checklist of spec §9.4.

Branch `plan5/root`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-root`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan5/root /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-root origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-root
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan5/power` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan5/root /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-root plan5/power`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan5/power>` and re-run `cargo xtask ci` before pushing.

### Task 18: / on the USB stick

Startup step 9 (spec §4.4, §6.5, §10). `usb::UsbDisk` is one USB disk as a `vfs::BlockDevice`: it names the disk by its controller and `DiskId` and locks `HOSTS` for each request, so the console's polling and the filesystem share the controllers; a disk that went away answers every request with `EIO` until the next boot (decision 3). `storage::mount_root` reads the GPT of every USB disk (a damaged primary gets a warning line), chooses the root partition by `BootInfo`'s boot GUID (the fallback gets a warning line too), and mounts ext2 on it. If the read-write mount fails with `EIO`, a worn-out stick that turned read-only, it mounts read-only on a fresh partition device, so the files can still be read; only if that fails too, or there is no root, does the shell get the empty read-only `/` of §10. The status line is `[ ok ] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB`, `[FAIL] mount /: Input/output error; mounted read-only, ext2 on …`, or `[FAIL] mount /: <reason>` (decision 6). The shell then shows the motd and starts in `/root`, so `boot`, `boot_bigmode`, `keyboard`, `shell` and `power` expect `root@relay:~# `, and `boot` the disk line, the mount line and the motd. A long `cp` needs no extra polling: the shell's `Console::interrupted` already looks at the keyboards between its 64 KiB pieces (decision 8).

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `README.md`
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/block/mod.rs`
- Modify: `kernel/src/block/testing.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/session.rs`
- Create: `kernel/src/storage.rs`
- Modify: `kernel/src/usb.rs`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/boot_bigmode.txt`
- Modify: `tests/e2e/keyboard.txt`
- Modify: `tests/e2e/power.txt`
- Modify: `tests/e2e/shell.txt`

**Interfaces:**
- Consumes: Tasks 7–17; `ext2::{Ext2, MountOptions}` (plan 3); `BootInfo::boot_partition_guid`.
- Produces: `relay_kernel::usb::{UsbDisk { name } (BlockDevice, Clone), disks() -> Vec<UsbDisk>}`; `relay_kernel::storage::{mount_ext2(open, env) -> Result<(Ext2<D>, Option<Errno>), Errno>, size_text(u64) -> String, root_text(&str, u32, u64) -> String, mount_root(Option<[u8; 16]>) -> Box<dyn FileSystem>}`; `session::run_shell(root: Box<dyn FileSystem>, test_mode: bool)`; test support `block::testing::{mke2fs_image, MemDisk::read_only}`; the kernel depends on `ext2`.

- [ ] **Step 1: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
vfs.workspace = true
usb.workspace = true
````

with:

````toml
vfs.workspace = true
ext2.workspace = true
usb.workspace = true
````

- [ ] **Step 2: Extend the test support in `kernel/src/block/testing.rs`**

In `kernel/src/block/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    pub bad: Range<u64>,
    /// Reads and writes that reached the disk.
````

with:

````rust
    pub bad: Range<u64>,
    /// Every write fails with `IoError::Device` (a stick worn out into
    /// read-only mode).
    pub read_only: bool,
    /// Reads and writes that reached the disk.
````

Replace:

````rust
            bad: 0..0,
            requests: 0,
````

with:

````rust
            bad: 0..0,
            read_only: false,
            requests: 0,
````

Replace:

````rust
        let span = self.span(lba, buf.len())?;
        self.requests += 1;
````

with:

````rust
        let span = self.span(lba, buf.len())?;
        if self.read_only {
            return Err(IoError::Device);
        }
        self.requests += 1;
````

Replace:

````rust
    data
}
````

with:

````rust
    data
}

/// An empty ext2 filesystem of `bytes` as `mke2fs` makes the root (4 KiB
/// blocks, the image's features).
pub fn mke2fs_image(bytes: u64) -> Vec<u8> {
    let path = image_file("ext2.img", bytes);
    let mut cmd = tool("mke2fs");
    cmd.args(["-q", "-F", "-t", "ext2", "-b", "4096", "-I", "256"])
        .args(["-O", "^dir_index,^resize_inode,^ext_attr"])
        .arg(&path);
    run(cmd, "");
    take(&path)
}
````

- [ ] **Step 3: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod session;
pub mod timer;
````

with:

````rust
pub mod session;
pub mod storage;
pub mod timer;
````

- [ ] **Step 4: Write the failing tests for `kernel/src/storage.rs`**

Create `kernel/src/storage.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::testing::{MemDisk, mke2fs_image};
    use alloc::rc::Rc;
    use core::cell::RefCell;
    use vfs::IoError;

    /// A disk the test keeps a handle to while the filesystem uses it.
    #[derive(Clone)]
    struct Shared(Rc<RefCell<MemDisk>>);

    impl BlockDevice for Shared {
        fn block_size(&self) -> usize {
            self.0.borrow().block_size()
        }
        fn block_count(&self) -> u64 {
            self.0.borrow().block_count()
        }
        fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
            self.0.borrow_mut().read(lba, buf)
        }
        fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
            self.0.borrow_mut().write(lba, buf)
        }
        fn flush(&mut self) -> Result<(), IoError> {
            self.0.borrow_mut().flush()
        }
    }

    struct Quiet;

    impl Env for Quiet {
        fn now(&self) -> u64 {
            1_750_000_000
        }
        fn log(&self, _: &str) {}
    }

    fn quiet() -> Box<dyn Env> {
        Box::new(Quiet)
    }

    fn disk(data: Vec<u8>) -> (Shared, Rc<core::cell::Cell<u32>>) {
        (
            Shared(Rc::new(RefCell::new(MemDisk::new(data, 512)))),
            Rc::new(core::cell::Cell::new(0)),
        )
    }

    fn opener(
        d: &Shared,
        opens: &Rc<core::cell::Cell<u32>>,
    ) -> impl FnMut() -> Result<Shared, Errno> {
        let (d, opens) = (d.clone(), opens.clone());
        move || {
            opens.set(opens.get() + 1);
            Ok(d.clone())
        }
    }

    #[test]
    fn a_good_filesystem_is_mounted_read_write() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        let (mut fs, ro) = mount_ext2(opener(&d, &opens), quiet).unwrap();
        assert_eq!(ro, None);
        assert_eq!(opens.get(), 1);
        let root = fs.root();
        fs.create(root, b"new").unwrap();
        fs.shutdown().unwrap();
    }

    #[test]
    fn a_disk_that_refuses_writes_is_mounted_read_only() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        d.0.borrow_mut().read_only = true;
        let (mut fs, ro) = mount_ext2(opener(&d, &opens), quiet).unwrap();
        assert_eq!(ro, Some(Errno::EIO));
        assert_eq!(opens.get(), 2, "a fresh device for the second try");
        let root = fs.root();
        assert_eq!(fs.create(root, b"new"), Err(Errno::EROFS));
        assert!(fs.lookup(root, b"lost+found").is_ok(), "files can be read");
    }

    #[test]
    fn what_is_not_ext2_is_not_tried_again() {
        let (d, opens) = disk(vec![0; 8 << 20]);
        assert_eq!(
            mount_ext2(opener(&d, &opens), quiet).err(),
            Some(Errno::EINVAL)
        );
        assert_eq!(opens.get(), 1);
    }

    #[test]
    fn a_disk_that_cannot_be_read_fails_both_tries() {
        let (d, opens) = disk(mke2fs_image(8 << 20));
        d.0.borrow_mut().bad = 0..u64::MAX;
        assert_eq!(
            mount_ext2(opener(&d, &opens), quiet).err(),
            Some(Errno::EIO)
        );
        assert_eq!(opens.get(), 2);
    }

    #[test]
    fn sizes_read_as_mib_then_gib() {
        assert_eq!(size_text(256 << 20), "256 MiB");
        assert_eq!(size_text((1 << 30) - 1), "1023 MiB");
        assert_eq!(size_text(1 << 30), "1.0 GiB");
        // The Kingston stick's root partition, and the whole stick.
        assert_eq!(size_text(15_501_000_000), "14.4 GiB");
        assert_eq!(size_text(30_277_632 * 512), "14.4 GiB");
        assert_eq!(size_text(u64::MAX), "17179869183.9 GiB");
    }

    #[test]
    fn the_status_line_names_the_disk_and_partition() {
        assert_eq!(
            root_text("00:02.0 port 2", 2, 190 << 20),
            "ext2 on 00:02.0 port 2 partition 2, 190 MiB"
        );
    }
}
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# The loader hands off, the kernel draws on the framebuffer, every startup
# step reports ok, and the shell starts on an empty read-only / (there is
# no storage driver yet).
timeout 30
````

with:

````text
# The loader hands off, the kernel draws on the framebuffer, every startup
# step reports ok, / is the stick's ext2 partition, and the shell shows the
# motd and starts in /root (spec §9.3 #1).
timeout 30
````

Replace:

````text
expect \[ ok \] keyboard: 1 keyboard
expect \[FAIL\] mount /: no storage driver yet
expect root@relay:/# $
screenshot-nonblank
````

with:

````text
expect \[ ok \] keyboard: 1 keyboard
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
expect \nWelcome to Relay OS\.\n
expect root@relay:~# $
screenshot-nonblank
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/boot_bigmode.txt`**

In `tests/e2e/boot_bigmode.txt`, replace:

````text
expect \[ ok \] console 2560x1600 \(120x33 cells\)
expect root@relay:/# $
screenshot-nonblank
````

with:

````text
expect \[ ok \] console 2560x1600 \(120x33 cells\)
expect root@relay:~# $
screenshot-nonblank
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/keyboard.txt`**

In `tests/e2e/keyboard.txt`, replace:

````text
expect \[ ok \] keyboard: 1 keyboard
expect root@relay:/# $
key echo hello
````

with:

````text
expect \[ ok \] keyboard: 1 keyboard
expect root@relay:~# $
key echo hello
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/power.txt`**

In `tests/e2e/power.txt`, replace:

````text
timeout 30
expect root@relay:/# $
send echo before
expect \nbefore\n
reboot relay: restarting through the FADT reset register
expect Relay OS \d+\.\d+\.\d+
expect root@relay:/# $
send echo after
````

with:

````text
timeout 30
expect root@relay:~# $
send echo before
expect \nbefore\n
reboot relay: restarting through the FADT reset register
expect Relay OS \d+\.\d+\.\d+
expect root@relay:~# $
send echo after
````

- [ ] **Step 9: Expect the new lines in `tests/e2e/shell.txt`**

Replace the whole of `tests/e2e/shell.txt` with:

````text
# The shell at the end of boot, typed at over COM1: the commands that
# report on the machine, and the files of the ext2 root.
timeout 30
expect root@relay:~# $
send echo hello, world
expect \nhello, world\n
send uname -a
expect \nRelay relay 0\.1\.0 x86_64\n
send cat /etc/hostname
expect \nrelay\n
send ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
send date
expect \n(Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d\n
send free
expect \nMem:\s+\d+\s+\d+\s+\d+\nHeap:\s+32768\s+\d+\s+\d+\n
send df
expect \nFilesystem\s+1K-blocks\s+Used\s+Available\s+Use% Mounted on\n/dev/root\s+\d+\s+\d+\s+\d+\s+\d+% /\n
send dmesg
expect \n\[ ok \] mount /: .*\n
send nosuch
expect \nrelay-sh: nosuch: command not found\n
expect root@relay:~# $
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find trait `BlockDevice` in this scope ``; `` cannot find trait `Env` in this scope ``.

- [ ] **Step 11: Change `README.md`**

In `README.md`, replace:

````markdown
| `crates/shell` | Line editor, parser and built-in commands |
| `crates/usb` | xHCI host controller driver and HID boot keyboard, over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
````

with:

````markdown
| `crates/shell` | Line editor, parser and built-in commands |
| `crates/usb` | xHCI host controller driver, HID boot keyboard and USB mass storage (BOT, SCSI), over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
````

- [ ] **Step 12: Change `kernel/src/block/mod.rs`**

In `kernel/src/block/mod.rs`, replace:

````rust
#[cfg(test)]
mod testing;
````

with:

````rust
#[cfg(test)]
pub(crate) mod testing;
````

- [ ] **Step 13: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    usb::init(cmdline.debug_usb);
    // There is no storage driver yet, so the shell starts on an empty,
    // read-only `/` (spec §10).
    console::fail("mount /", format_args!("no storage driver yet"));
    session::run_shell(cmdline.test_mode)
}
````

with:

````rust
    usb::init(cmdline.debug_usb);
    let root = storage::mount_root(info.boot_partition_guid());
    session::run_shell(root, cmdline.test_mode)
}
````

- [ ] **Step 14: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, MemFs, MountTable};

````

with:

````rust
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, FileSystem, MountTable};

````

Replace:

````rust

/// Runs the shell on an empty read-only `/` (spec §10). Never returns.
pub fn run_shell(test_mode: bool) -> ! {
    let root = MemFs::new(Box::new(KernelEnv)).read_only();
    let mut vfs = MountTable::new(Box::new(root));
    let mut console = KernelConsole::new();
````

with:

````rust

/// Runs the shell with `root` mounted at `/` (spec §4.4 step 10). Never
/// returns.
pub fn run_shell(root: Box<dyn FileSystem>, test_mode: bool) -> ! {
    let mut vfs = MountTable::new(root);
    let mut console = KernelConsole::new();
````

- [ ] **Step 15: Implement `kernel/src/storage.rs`**

Insert this at the top of `kernel/src/storage.rs`, above `#[cfg(test)]`:

````rust
//! Startup step 9 (spec §4.4, §6.5, §10): the root filesystem. The GPT of
//! every USB disk is read, the root partition chosen by the boot
//! partition's GUID, and ext2 mounted on it; if the read-write mount fails
//! with an I/O error (a worn-out stick often turns read-only) it is mounted
//! read-only, and only if that fails too does the shell get the empty
//! read-only `/`.

use crate::block::gpt::{self, Guid};
use crate::block::partition::Partition;
use crate::block::root::{self, RootChoice};
use crate::session::KernelEnv;
use crate::usb::{self, UsbDisk};
use crate::{console, klogln, kprintln};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use ext2::{Ext2, MountOptions};
use vfs::{BlockDevice, Env, Errno, FileSystem, MemFs};

/// Mounts ext2 on the device `open` returns: read-write, and if that fails
/// with `EIO`, read-only on a fresh device. Returns the filesystem and, if
/// it is read-only, the error of the read-write attempt.
pub fn mount_ext2<D: BlockDevice>(
    mut open: impl FnMut() -> Result<D, Errno>,
    env: impl Fn() -> Box<dyn Env>,
) -> Result<(Ext2<D>, Option<Errno>), Errno> {
    match Ext2::mount(open()?, env(), MountOptions::default()) {
        Ok(fs) => Ok((fs, None)),
        Err(Errno::EIO) => {
            let opts = MountOptions {
                read_only: true,
                ..MountOptions::default()
            };
            Ext2::mount(open()?, env(), opts).map(|fs| (fs, Some(Errno::EIO)))
        }
        Err(e) => Err(e),
    }
}

/// `256 MiB`, or `14.4 GiB` from 1 GiB on (rounded down).
pub fn size_text(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    if bytes < GIB {
        format!("{} MiB", bytes >> 20)
    } else {
        let tenths = (bytes as u128 * 10 / GIB as u128) as u64;
        format!("{}.{} GiB", tenths / 10, tenths % 10)
    }
}

/// What the `mount /` status line says about the root: `ext2 on 00:02.0
/// port 2 partition 2, 190 MiB`.
pub fn root_text(disk: &str, number: u32, bytes: u64) -> String {
    format!("ext2 on {disk} partition {number}, {}", size_text(bytes))
}

/// Startup step 9: the root filesystem, or the empty read-only `/` of spec
/// §10 if there is none. Prints the `mount /` status line.
pub fn mount_root(boot: Option<[u8; 16]>) -> Box<dyn FileSystem> {
    match try_mount_root(boot.map(Guid)) {
        Ok((fs, what, None)) => {
            console::ok(format_args!("mount /: {what}"));
            fs
        }
        Ok((fs, what, Some(e))) => {
            console::fail("mount /", format_args!("{e}; mounted read-only, {what}"));
            fs
        }
        Err(why) => {
            console::fail("mount /", format_args!("{why}"));
            Box::new(MemFs::new(Box::new(KernelEnv)).read_only())
        }
    }
}

type Mounted = (Box<dyn FileSystem>, String, Option<Errno>);

fn try_mount_root(boot: Option<Guid>) -> Result<Mounted, String> {
    let mut disks = usb::disks();
    if disks.is_empty() {
        return Err(String::from("no USB disk"));
    }
    let gpts: Vec<_> = disks
        .iter_mut()
        .map(|d| match gpt::read_gpt(d) {
            Ok(g) => {
                if g.used_backup {
                    kprintln!("mount /: {}: primary GPT damaged, using the backup", d.name);
                }
                klogln!(
                    "storage: {}: GPT with {} partitions",
                    d.name,
                    g.partitions.len()
                );
                Some(g)
            }
            Err(e) => {
                klogln!("storage: {}: {e}", d.name);
                None
            }
        })
        .collect();
    let RootChoice {
        disk,
        partition,
        fallback,
    } = root::choose_root(&gpts, boot).map_err(|e| format!("{e}"))?;
    let dev: UsbDisk = disks.swap_remove(disk);
    if fallback {
        kprintln!(
            "mount /: warning: no disk has the boot partition; using {}",
            dev.name
        );
    }
    let bytes =
        (partition.last_lba - partition.first_lba + 1).saturating_mul(dev.block_size() as u64);
    let what = root_text(&dev.name, partition.number, bytes);
    let open = || {
        Partition::new(dev.clone(), partition.first_lba, partition.last_lba).map_err(|_| Errno::EIO)
    };
    let env = || Box::new(KernelEnv) as Box<dyn Env>;
    match mount_ext2(open, env) {
        Ok((fs, ro)) => Ok((Box::new(fs), what, ro)),
        Err(e) => Err(format!("{what}: {e}")),
    }
}

````

- [ ] **Step 16: Change `kernel/src/usb.rs`**

In `kernel/src/usb.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use spin::Mutex;
use usb::host::Host;
use usb::xhci::Xhci;
use usb::{DmaBuf, Hal, UsbError};

/// Every running controller with its drivers. The console polls them; the
/// storage driver will share them.
static HOSTS: Mutex<Vec<Host<KernelHal>>> = Mutex::new(Vec::new());
````

with:

````rust
use spin::Mutex;
use usb::host::{DiskId, Host};
use usb::xhci::Xhci;
use usb::{DmaBuf, Hal, UsbError};
use vfs::{BlockDevice, IoError, check_request};

/// Every running controller with its drivers. The console polls them, and
/// every disk request locks them.
static HOSTS: Mutex<Vec<Host<KernelHal>>> = Mutex::new(Vec::new());
````

Replace:

````rust

/// Hot-plug and the keyboards' LED and recovery work. May wait while a new
````

with:

````rust

/// A USB disk as a block device (spec §6.5). It names the disk by its host
/// and `DiskId`, and locks the hosts for each request, so the console's
/// polling and the filesystem share them. A disk that went away fails every
/// request; a stick plugged in again is another disk.
#[derive(Clone, Debug)]
pub struct UsbDisk {
    host: usize,
    id: DiskId,
    /// `00:14.0 port 15`, for status lines and the log.
    pub name: String,
    block_size: usize,
    blocks: u64,
}

/// Every disk on every controller, in controller and port order.
pub fn disks() -> Vec<UsbDisk> {
    let hosts = HOSTS.lock();
    let mut out = Vec::new();
    for (i, host) in hosts.iter().enumerate() {
        for d in host.disks() {
            out.push(UsbDisk {
                host: i,
                id: d.id,
                name: alloc::format!("{} port {}", host.xhci().name(), d.port),
                block_size: d.block_size,
                blocks: d.block_count,
            });
        }
    }
    out
}

impl UsbDisk {
    fn request(
        &self,
        what: &str,
        lba: u64,
        f: impl FnOnce(&mut Host<KernelHal>) -> Result<(), UsbError>,
    ) -> Result<(), IoError> {
        let mut hosts = HOSTS.lock();
        let host = hosts.get_mut(self.host).ok_or(IoError::Device)?;
        f(host).map_err(|e| {
            klogln!("usb: {}: {what} at block {lba}: {e}", self.name);
            IoError::Device
        })
    }
}

impl BlockDevice for UsbDisk {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.blocks
    }

    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError> {
        check_request(self.block_size, self.blocks, lba, buf.len())?;
        self.request("read", lba, |h| h.read(self.id, lba, buf))
    }

    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError> {
        check_request(self.block_size, self.blocks, lba, buf.len())?;
        self.request("write", lba, |h| h.write(self.id, lba, buf))
    }

    fn flush(&mut self) -> Result<(), IoError> {
        self.request("flush", 0, |h| h.flush(self.id))
    }
}

/// Hot-plug and the keyboards' LED and recovery work. May wait while a new
````

- [ ] **Step 17: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 177 tests.

- [ ] **Step 18: Run the scenarios whose boot changed**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario boot_bigmode`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario power`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 19: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 20: Commit**

````bash
git add Cargo.lock README.md kernel tests
git commit -m "kernel: / on the USB stick: the root partition mounted at startup"
````


### Task 19: The log says how the root disk was chosen

Review finding (minor): the image has one ESP and one Linux partition, so the `boot` scenario passed whether the root was found by the boot partition's GUID or by the fallback; a byte-order mistake in the GUID the loader hands over would have gone unnoticed. The kernel now logs `storage: root on <disk>, the disk with the boot partition <GUID>`, or puts the fallback's warning on the screen, and `boot` expects the log line with the image's fixed ESP GUID (decision 6).

**Files:**
- Modify: `kernel/src/storage.rs`
- Modify: `tests/e2e/boot.txt`

**Interfaces:**
- Consumes: Task 18.
- Produces: `storage::choice_text(disk: &str, fallback: bool, boot: Option<Guid>) -> String`.

- [ ] **Step 1: Add the failing tests to `kernel/src/storage.rs`**

In `kernel/src/storage.rs`, replace:

````rust
    #[test]
    fn the_status_line_names_the_disk_and_partition() {
````

with:

````rust
    #[test]
    fn how_the_root_was_chosen_is_said() {
        let esp = crate::block::gpt::Guid::from_fields(
            0x5245_4C41,
            0x5900,
            0x4000,
            [0x80, 0, 0, 0, 0, 0, 0, 0x02],
        );
        assert_eq!(
            choice_text("00:02.0 port 2", false, Some(esp)),
            "storage: root on 00:02.0 port 2, the disk with the boot partition 52454C41-5900-4000-8000-000000000002"
        );
        assert_eq!(
            choice_text("00:14.0 port 15", true, Some(esp)),
            "mount /: warning: no disk has the boot partition 52454C41-5900-4000-8000-000000000002; using 00:14.0 port 15, the first disk with one ESP and one Linux partition"
        );
        assert_eq!(
            choice_text("00:14.0 port 15", true, None),
            "mount /: warning: the loader did not name the boot partition; using 00:14.0 port 15, the first disk with one ESP and one Linux partition"
        );
    }

    #[test]
    fn the_status_line_names_the_disk_and_partition() {
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
expect \[ ok \] keyboard: 1 keyboard
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
````

with:

````text
expect \[ ok \] keyboard: 1 keyboard
# The root was found by the ESP's partition GUID from the loader (the
# image's fixed GUID, xtask/src/config.rs), not by the fallback.
expect storage: root on 00:02\.0 port 2, the disk with the boot partition 52454C41-5900-4000-8000-000000000002
expect \[ ok \] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `choice_text` in this scope ``.

- [ ] **Step 4: Change `kernel/src/storage.rs`**

In `kernel/src/storage.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// Startup step 9: the root filesystem, or the empty read-only `/` of spec
````

with:

````rust

/// How the root disk was chosen (spec §6.5): a log line when it has the
/// boot partition, a warning for the screen when it is the fallback.
pub fn choice_text(disk: &str, fallback: bool, boot: Option<Guid>) -> String {
    const FALLBACK: &str = "the first disk with one ESP and one Linux partition";
    match (fallback, boot.filter(|g| !g.is_zero())) {
        (false, Some(g)) => {
            format!("storage: root on {disk}, the disk with the boot partition {g}")
        }
        (false, None) => format!("storage: root on {disk}"),
        (true, Some(g)) => format!(
            "mount /: warning: no disk has the boot partition {g}; using {disk}, {FALLBACK}"
        ),
        (true, None) => format!(
            "mount /: warning: the loader did not name the boot partition; using {disk}, {FALLBACK}"
        ),
    }
}

/// Startup step 9: the root filesystem, or the empty read-only `/` of spec
````

Replace:

````rust
    let dev: UsbDisk = disks.swap_remove(disk);
    if fallback {
        kprintln!(
            "mount /: warning: no disk has the boot partition; using {}",
            dev.name
        );
    }
````

with:

````rust
    let dev: UsbDisk = disks.swap_remove(disk);
    let how = choice_text(&dev.name, fallback, boot);
    if fallback {
        kprintln!("{how}");
    } else {
        klogln!("{how}");
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 178 tests.

- [ ] **Step 6: Run the boot scenario**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel tests
git commit -m "kernel: the log says how the root disk was chosen, and boot checks it was the boot partition's"
````


### Task 20: The fileops, persist and display scenarios, and NUC check 3

Spec §9.3 scenarios 3, 4 and 6 over the real disk, typed over serial. `fileops` runs the file commands of §7.3 with their GNU-style outputs and error messages, including `rm -r /`; `persist` writes files, reboots and reads them back; `display` clears the screen and checks the prompt is drawn in the framebuffer's top rows. Each is followed by `e2fsck -fn` (Task 15). The shell syncs after every command, so what a command wrote is on the disk when the prompt returns. `docs/hardware-test.md` gains NUC check 3, the full checklist of spec §9.4, with the lines the NUC prints and what to do when one is wrong. These scenarios pass as soon as they are added: they check Task 18's work end to end.

**Files:**
- Modify: `docs/hardware-test.md`
- Create: `tests/e2e/display.txt`
- Create: `tests/e2e/fileops.txt`
- Create: `tests/e2e/persist.txt`

**Interfaces:**
- Consumes: Tasks 15 and 18.
- Produces: scenarios `tests/e2e/{fileops,persist,display}.txt`; NUC check 3 in `docs/hardware-test.md`.

- [ ] **Step 1: Add the scenario `tests/e2e/display.txt`**

Create `tests/e2e/display.txt`:

````text
# The display (spec §9.3 #6): after `clear` the prompt is drawn in the top
# rows of the framebuffer, where the screenshot must not be one colour.
timeout 30
expect root@relay:~# $
send clear
expect root@relay:~# $
screenshot-nonblank
````

- [ ] **Step 2: Add the scenario `tests/e2e/fileops.txt`**

Every pattern after a `send` starts with `\n`, so it matches the command's output, not the echoed command line.

Create `tests/e2e/fileops.txt`:

````text
# File operations on the ext2 root (spec §9.3 #3), typed over serial. The
# shell syncs after every command, and e2fsck checks the disk afterwards.
timeout 30
expect root@relay:~# $
send pwd
expect \n/root\n
send mkdir -p /root/a/b/c
send echo hello > /root/a/f
send echo world >> /root/a/f
send cat /root/a/f
expect \nhello\nworld\n
send ls -l /root/a
expect \ntotal 8\ndrwxr-xr-x 3 root root 4096 \w{3} [ \d]\d \d\d:\d\d b\n-rw-r--r-- 1 root root   12 \w{3} [ \d]\d \d\d:\d\d f\n
send cp /root/a/f /root/a/g
send mv /root/a/g /root/a/b/h
send ls /root/a /root/a/b
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
send cat /root/a/b/h
expect \nhello\nworld\n
send rmdir /root/a/b
expect \nrmdir: failed to remove '/root/a/b': Directory not empty\n
send rm -r /root/a/b
send ls /root/a
expect \nf\n
send touch /root/t
send stat /root/t
expect \n  File: /root/t\n  Size: 0 .*regular empty file\n
send head -n 1 /root/a/f
expect \nhello\n
send tail -n 1 /root/a/f
expect \nworld\n
send wc /root/a/f
expect \n 2  2 12 /root/a/f\n
send rm /root/a/f /root/t
send rmdir /root/a
send ls
expect \nREADME\n
# Error messages follow GNU coreutils.
send cat /root/nope
expect \ncat: /root/nope: No such file or directory\n
send ls /nope
expect \nls: cannot access '/nope': No such file or directory\n
send cp /root/nope /root/x
expect \ncp: cannot stat '/root/nope': No such file or directory\n
send mkdir /root
expect \nmkdir: cannot create directory '/root': File exists\n
send rm -r /
expect \nrm: it is dangerous to operate recursively on '/'\n
send rm /
expect \nrm: cannot remove '/': Is a directory\n
send ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
poweroff
````

- [ ] **Step 3: Add the scenario `tests/e2e/persist.txt`**

Create `tests/e2e/persist.txt`:

````text
# Files survive a reboot (spec §9.3 #4): written, then read back after the
# machine restarted and mounted the same disk again.
timeout 30
expect root@relay:~# $
send mkdir /root/notes
send echo remember me > /root/notes/a
send echo and me >> /root/notes/a
send touch /root/notes/b
send ls /root/notes
expect \na  b\n
reboot
expect \[ ok \] mount /
expect root@relay:~# $
send cat /root/notes/a
expect \nremember me\nand me\n
send ls /root/notes
expect \na  b\n
poweroff
````

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
     `[ ok ] keyboard: 2 keyboards`, `[FAIL] mount /: no storage driver
     yet`, and the prompt `root@relay:/# `.
3. On the K120, type and check each result:
````

with:

````markdown
     `[ ok ] keyboard: 2 keyboards`, `[FAIL] mount /: no storage driver
     yet`, and the prompt `root@relay:/# `. (Since plan 5 the stick is a
     disk, `/` is mounted and the prompt is `root@relay:~# `; see check 3.)
3. On the K120, type and check each result:
````

Replace:

````markdown

## Results log
````

with:

````markdown

## Check 3 — files on the stick (plan 5)

The full checklist of spec §9.4. The K120 and the stick sit on the ports of
check 2 (the stick on bus 4 port 3 in Mint's `lsusb -t`).

1. In Mint: `cargo xtask flash --full` and type `ERASE` when asked (this
   erases the files of earlier runs).
2. Reboot, press F10 and choose the UEFI entry for the Kingston stick.
3. The screen shows every startup line `[ ok ]`, at the monitor's native
   resolution (`console 1920x1200` on the ASUS PA248QV). After check 2's
   `usb:` lines, which now end:
   - `usb: 00:14.0 port 15: 0951:1666 SuperSpeed, disk Kingston
     DataTraveler 3.0, 14.4 GiB`
   - `[ ok ] usb: 2 controllers, 4 devices`, `[ ok ] keyboard: 2 keyboards`
   - `[ ok ] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB`
   - the motd (`Welcome to Relay OS.`) and the prompt `root@relay:~# `.
4. On the K120, type these and check each result (the `fileops` scenario's
   operations):
   - `mkdir -p /root/notes/old`, `echo remember me > /root/notes/a`,
     `echo and me >> /root/notes/a`, `cat /root/notes/a` → the two lines.
   - `ls -l /root/notes` → `a` (19 bytes) and the directory `old`, owned
     by `root root`, with today's date.
   - `cp /root/notes/a /root/notes/b`, `mv /root/notes/b /root/notes/old/c`,
     `ls /root/notes /root/notes/old` → `a  old`, then `c`.
   - `rmdir /root/notes/old` → `rmdir: failed to remove '/root/notes/old':
     Directory not empty`; `rm -r /root/notes/old`; `ls /root/notes` → `a`.
   - `touch /root/notes/t`, `stat /root/notes/t` (size 0, mode 0644, the
     current UTC time), `head -n 1 /root/notes/a`, `tail -n 1
     /root/notes/a`, `wc /root/notes/a` → `2 4 19`.
   - `cat /root/nope` → `cat: /root/nope: No such file or directory`;
     `rm -r /` → `rm: it is dangerous to operate recursively on '/'`.
   - `df` shows `/dev/root` of about 15 million 1K-blocks; `dmesg` shows the
     `storage: slot N:` lines (`Kingston`, `DataTraveler 3.0`, `PMAP`,
     `30277632 blocks of 512 bytes`).
5. `reboot`: the NUC restarts (`relay: restarting`). Choose the stick again
   with F10; `cat /root/notes/a` shows both lines and `ls /root/notes`
   shows `a  t`.
6. `poweroff`: the NUC switches itself off (`relay: powering off`). If the
   screen says `System halted. It is now safe to power off.` instead, note
   the `relay:` line above it and hold the power button.
7. Boot Mint and run `cargo xtask verify-usb`: `e2fsck: clean`, and the
   tree lists `/root/notes/a` and `/root/notes/t`.
8. Photograph the screen after step 3 and after `dmesg`.

### If it fails

| What you see | Likely cause | Next step |
|---|---|---|
| `port 15: … disk not started: …` and `[FAIL] mount /: no USB disk` | The stick's setup failed (the reason says which command) | `debug=usb`: the `storage: slot N:` lines with the sense of each failure |
| `port 15: setup failed: …` | Enumeration failed three times | Replug the stick into the same port and reboot; `debug=usb` shows each try |
| `[FAIL] mount /: no disk with the boot partition` | The loader's boot GUID matches no partition | Note the `boot info` line; `dmesg` shows the `storage:` GPT line |
| `[FAIL] mount /: …; mounted read-only` | The stick refused a write (worn out or write-protected) | The files can be read; note the `usb: … write at block N:` line in `dmesg` |
| `[FAIL] mount /: … partition 2: Invalid argument` | The ext2 root is not what `flash --full` writes | `dmesg` shows the `ext2:` reason; re-run `flash --full` |
| A command prints `Input/output error` | A disk request failed after three tries | `dmesg`: the `storage:` and `usb:` lines name the command and block |
| `reboot` leaves the screen as it is | No reset method worked (unlikely: the last is a triple fault) | Photograph the screen; hold the power button |
| `verify-usb` reports errors | A write was lost or wrong | Do not flash again: keep the stick as it is and report the output |

## Results log
````

- [ ] **Step 5: Run the new scenarios**

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario persist`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario display`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add docs tests
git commit -m "tests: the fileops, persist and display scenarios, and NUC check 3"
````


### Task 21: The bigfile and diskfull scenarios

Spec §9.3 scenarios 5 and 7. `bigfile` builds a 4 KiB file of 64-byte lines, doubles it with `>>` to 8 MiB (past the single-indirect blocks) and copies it; after `poweroff` the new runner step `file-lines` reads both files from the disk with `debugfs dump` and checks they are exactly that line, 131,072 times. `diskfull` boots, with the new directive `disk small`, an image whose ext2 root is 32 MiB, fills it until `cat` reports `No space left on device`, checks `df` at 100%, and frees space again. The runner builds the small image only when a scenario asks for it. COM1 input goes through the 4 KiB input queue, so the scenarios build big files by doubling instead of typing them.

**Files:**
- Create: `tests/e2e/bigfile.txt`
- Create: `tests/e2e/diskfull.txt`
- Modify: `xtask/src/config.rs`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: Tasks 15–20.
- Produces: e2e directive `disk small`, step `file-lines <path> <bytes> <line>`; `e2e::lines_mismatch`; `image::{build_image_as, read_ext2_file}`; `config::SMALL_IMAGE_BYTES`; scenarios `tests/e2e/{bigfile,diskfull}.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/bigfile.txt`**

Create `tests/e2e/bigfile.txt`:

````text
# Building an 8 MiB file on the ext2 root (spec §9.3 #5): a 4 KiB file of
# 64-byte lines, doubled with >> until it is 8 MiB (double-indirect
# blocks), then copied. The host reads both from the disk with debugfs.
timeout 120
expect root@relay:~# $
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
send cat l l l l l l l l l l l l l l l l > m
send cat m m m m > l
send wc l
expect \n\s*64\s+64\s+4096 l\n
send cp l big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send wc big
expect \n\s*131072\s+131072\s+8388608 big\n
send cp big copy
send rm t m
send ls -l big copy
expect \n-rw-r--r-- 1 root root 8388608 .* big\n-rw-r--r-- 1 root root 8388608 .* copy\n
poweroff
file-lines /root/big 8388608 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
file-lines /root/copy 8388608 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
````

- [ ] **Step 2: Add the scenario `tests/e2e/diskfull.txt`**

Create `tests/e2e/diskfull.txt`:

````text
# A full disk (spec §9.3 #7): the 32 MiB root is filled until writes fail
# with ENOSPC; the shell says so, space comes back after rm, and e2fsck
# finds the disk clean.
disk small
timeout 120
expect root@relay:~# $
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
send cat l l l l l l l l l l l l l l l l > m
send cat m m m m > l
send cp l big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
send cat big > t
send cat t >> big
expect \ncat: write error: No space left on device\n
send df
expect \n/dev/root\s+\d+\s+\d+\s+0 100% /\n
send echo more > more
expect \necho: write error: No space left on device\n
send rm t
send echo more > more
send cat more
expect \nmore\n
poweroff
````

- [ ] **Step 3: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_alive_step() {
````

with:

````rust
    #[test]
    fn parses_the_small_disk_and_file_lines() {
        let s = parse_scenario("x", "disk small\nfile-lines /root/big 8 abc").unwrap();
        assert!(s.small_disk);
        assert_eq!(
            s.steps,
            vec![(
                2,
                Step::FileLines {
                    path: "/root/big".into(),
                    bytes: 8,
                    line: "abc".into()
                }
            )]
        );
        assert!(!parse_scenario("x", "expect a").unwrap().small_disk);
        assert!(parse_scenario("x", "disk large").is_err());
        assert!(parse_scenario("x", "expect a\ndisk small").is_err());
        assert!(
            parse_scenario("x", "file-lines /f 7 abc").is_err(),
            "not whole lines"
        );
        assert!(parse_scenario("x", "file-lines f 8 abc").is_err());
        assert!(parse_scenario("x", "file-lines /f 8").is_err());
    }

    #[test]
    fn file_lines_finds_the_first_wrong_line() {
        assert_eq!(lines_mismatch(b"ab\nab\n", 6, "ab"), None);
        assert_eq!(
            lines_mismatch(b"ab\nab\n", 9, "ab"),
            Some("is 6 bytes, expected 9".into())
        );
        assert_eq!(
            lines_mismatch(b"ab\nax\nab\n", 9, "ab"),
            Some("line 2 (byte 3) is \"ax\\n\"".into())
        );
    }

    #[test]
    fn file_lines_reads_the_file_from_an_ext2_image() {
        let dir = out_dir().join("e2e-selftest").join("file-lines");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("staging/root")).unwrap();
        fs::write(dir.join("staging/root/big"), "abc\n".repeat(1000)).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2", "-d"])
                .arg(dir.join("staging"))
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        let data = image::read_ext2_file(&img, whole, "/root/big", &dir).unwrap();
        assert_eq!(lines_mismatch(&data.unwrap(), 4000, "abc"), None);
        let missing = image::read_ext2_file(&img, whole, "/root/nope", &dir).unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn parses_alive_step() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find function `read_ext2_file` in module `image` ``; `` cannot find function `lines_mismatch` in this scope ``.

- [ ] **Step 5: Change `xtask/src/config.rs`**

In `xtask/src/config.rs`, replace:

````rust
pub const IMAGE_BYTES: u64 = 256 << 20;
pub const ESP_START_LBA: u64 = 2048;
````

with:

````rust
pub const IMAGE_BYTES: u64 = 256 << 20;
/// The `diskfull` scenario's image (spec §9.3 #7): the same ESP and a
/// 32 MiB ext2 root.
pub const SMALL_IMAGE_BYTES: u64 = (ROOT_START_LBA * SECTOR) + (32 << 20) + (1 << 20);
pub const ESP_START_LBA: u64 = 2048;
````

- [ ] **Step 6: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
//! cmdline test=1 panic=pagefault   (before any other step; default "test=1")
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
````

with:

````rust
//! cmdline test=1 panic=pagefault   (before any other step; default "test=1")
//! disk small                       (before any other step: the 32 MiB root)
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
````

Replace:

````rust
//!                                   isa-debug-exit, test mode's power-off)
//! ```
````

with:

````rust
//!                                   isa-debug-exit, test mode's power-off)
//! file-lines /root/big 8388608 abc (after poweroff: the file, read with
//!                                   debugfs, is "abc\n" repeated to 8388608
//!                                   bytes)
//! ```
````

Replace:

````rust
    Poweroff,
}
````

with:

````rust
    Poweroff,
    /// The file at `path` on the ext2 root is `line` and a newline, again
    /// and again, `bytes` in all. Checked on the disk once the machine is
    /// off.
    FileLines {
        path: String,
        bytes: usize,
        line: String,
    },
}
````

Replace:

````rust
    pub cmdline: String,
    /// ESP files to overwrite before booting: (path, contents).
````

with:

````rust
    pub cmdline: String,
    /// `disk small`: boot the image with the 32 MiB root.
    pub small_disk: bool,
    /// ESP files to overwrite before booting: (path, contents).
````

Replace:

````rust
    let mut cmdline = DEFAULT_CMDLINE.to_string();
    let mut esp_writes = Vec::new();
````

with:

````rust
    let mut cmdline = DEFAULT_CMDLINE.to_string();
    let mut small_disk = false;
    let mut esp_writes = Vec::new();
````

Replace:

````rust
                cmdline = rest.to_string();
                continue;
````

with:

````rust
                cmdline = rest.to_string();
                continue;
            }
            "disk" => {
                if !steps.is_empty() || rest != "small" {
                    bail!("{name}:{line_no}: expected `disk small` before other steps");
                }
                small_disk = true;
                continue;
````

Replace:

````rust
            "poweroff" => Step::Poweroff,
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
````

with:

````rust
            "poweroff" => Step::Poweroff,
            "file-lines" => parse_file_lines(rest).with_context(|| format!("{name}:{line_no}"))?,
            other => bail!("{name}:{line_no}: unknown step '{other}'"),
````

Replace:

````rust
        cmdline,
        esp_writes,
        steps,
    })
}
````

with:

````rust
        cmdline,
        small_disk,
        esp_writes,
        steps,
    })
}

fn parse_file_lines(rest: &str) -> Result<Step> {
    let mut parts = rest.splitn(3, ' ');
    let (Some(path), Some(bytes), Some(line)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("expected: file-lines <path> <bytes> <line>");
    };
    let bytes: usize = bytes.parse().context("bytes")?;
    if !path.starts_with('/') || line.is_empty() || !bytes.is_multiple_of(line.len() + 1) {
        bail!("file-lines needs an absolute path, a line, and a size that is whole lines");
    }
    Ok(Step::FileLines {
        path: path.to_string(),
        bytes,
        line: line.to_string(),
    })
}

/// Why `data` is not `line` and a newline repeated to `bytes` bytes.
pub fn lines_mismatch(data: &[u8], bytes: usize, line: &str) -> Option<String> {
    if data.len() != bytes {
        return Some(format!("is {} bytes, expected {bytes}", data.len()));
    }
    let unit = [line.as_bytes(), b"\n"].concat();
    let bad = data.chunks(unit.len()).position(|c| c != unit)?;
    Some(format!(
        "line {} (byte {}) is {:?}",
        bad + 1,
        bad * unit.len(),
        String::from_utf8_lossy(
            &data[bad * unit.len()..][..unit.len().min(data.len() - bad * unit.len())]
        )
    ))
}
````

Replace:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    if r.off && !matches!(step, Step::Timeout(_)) {
        bail!("the machine was switched off by an earlier step");
````

with:

````rust
fn run_step(r: &mut Running, step: &Step, timeout: &mut Duration, run_dir: &Path) -> Result<()> {
    let offline = matches!(step, Step::Timeout(_) | Step::FileLines { .. });
    if r.off && !offline {
        bail!("the machine was switched off by an earlier step");
````

Replace:

````rust
        Step::Reboot(last) => reboot(r, last.as_deref(), *timeout)?,
        Step::Poweroff => {
````

with:

````rust
        Step::Reboot(last) => reboot(r, last.as_deref(), *timeout)?,
        Step::FileLines { path, bytes, line } => {
            if !r.off {
                bail!("file-lines reads the disk: switch the machine off first (poweroff)");
            }
            let data = image::read_ext2_file(&r.qemu.disk, r.root, path, run_dir)?
                .with_context(|| format!("{path}: no such file on the disk"))?;
            if let Some(why) = lines_mismatch(&data, *bytes, line) {
                bail!("{path} {why}");
            }
        }
        Step::Poweroff => {
````

Replace:

````rust
    let layout = image::read_layout(&img)?;
    for s in &scenarios {
        println!("== e2e {}", s.name);
        run_scenario(&img, &layout, s)?;
    }
````

with:

````rust
    let layout = image::read_layout(&img)?;
    let small = if scenarios.iter().any(|s| s.small_disk) {
        let img = image::build_image_as(
            &art,
            DEFAULT_CMDLINE,
            "relay-os-small.img",
            crate::config::SMALL_IMAGE_BYTES,
        )?;
        let layout = image::read_layout(&img)?;
        Some((img, layout))
    } else {
        None
    };
    for s in &scenarios {
        println!("== e2e {}", s.name);
        match (&small, s.small_disk) {
            (Some((img, layout)), true) => run_scenario(img, layout, s)?,
            _ => run_scenario(&img, &layout, s)?,
        }
    }
````

- [ ] **Step 7: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// `e2fsck -fn`: read-only full check. Returns the checker output on failure.
````

with:

````rust

/// The contents of the file at `path` in the ext2 filesystem of `part`,
/// through `debugfs dump`; `None` if there is no such regular file.
pub fn read_ext2_file(
    target: &Path,
    part: Partition,
    path: &str,
    scratch: &Path,
) -> Result<Option<Vec<u8>>> {
    fs::create_dir_all(scratch)?;
    let out = scratch.join("dump.tmp");
    let _ = fs::remove_file(&out);
    // debugfs reports a missing file on stderr and still exits 0.
    run(Command::new("debugfs")
        .arg("-R")
        .arg(format!("dump \"{path}\" {}", out.display()))
        .arg(e2fs_target(target, part)))?;
    match fs::read(&out) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// `e2fsck -fn`: read-only full check. Returns the checker output on failure.
````

Replace:

````rust
pub fn build_image(art: &Artifacts, cmdline: &str) -> Result<PathBuf> {
    let img = out_dir().join("relay-os.img");
    fs::create_dir_all(out_dir())?;
    let _ = fs::remove_file(&img);
    fs::File::create(&img)?.set_len(IMAGE_BYTES)?;
    partition(&img, true)?;
````

with:

````rust
pub fn build_image(art: &Artifacts, cmdline: &str) -> Result<PathBuf> {
    build_image_as(art, cmdline, "relay-os.img", IMAGE_BYTES)
}

/// Builds `target/relay/<name>`, `bytes` long; the ext2 root takes what
/// the ESP leaves.
pub fn build_image_as(art: &Artifacts, cmdline: &str, name: &str, bytes: u64) -> Result<PathBuf> {
    let img = out_dir().join(name);
    fs::create_dir_all(out_dir())?;
    let _ = fs::remove_file(&img);
    fs::File::create(&img)?.set_len(bytes)?;
    partition(&img, true)?;
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 45 tests.

- [ ] **Step 9: Run the new scenarios**

Run: `cargo xtask test --e2e-only --scenario bigfile`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario diskfull`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add tests xtask
git commit -m "xtask: the bigfile and diskfull scenarios on a small disk, checked with debugfs"
````


### Task 22: The mount_fail scenario

Review findings (minor): no scenario covered spec §10's failure path any more, and NUC check 3's failure table did not match what the kernel prints. The new directive `break-root` zeroes the ext2 superblock's magic of the root before boot; `mount_fail` then expects `[FAIL] mount /: ext2 on 00:02.0 port 2 partition 2, 190 MiB: Invalid argument`, the empty read-only `/` and a shell that still works. Afterwards the runner puts the magic back, and `e2fsck` shows the kernel wrote nothing to the filesystem it could not mount. The table's rows now quote the real lines, add `no disk with a GPT` (a stick whose reads fail after its setup) and the fallback's warning, and say what `no disk with the boot partition` really means (decision 9).

**Files:**
- Modify: `docs/hardware-test.md`
- Create: `tests/e2e/mount_fail.txt`
- Modify: `xtask/src/e2e.rs`
- Modify: `xtask/src/image.rs`

**Interfaces:**
- Consumes: Tasks 18 and 21.
- Produces: e2e directive `break-root` (`Scenario::break_root`); `image::set_ext2_magic(target, Partition, present: bool)`; scenario `tests/e2e/mount_fail.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/mount_fail.txt`**

Create `tests/e2e/mount_fail.txt`:

````text
# No usable root (spec §10): the ext2 superblock's magic is zeroed before
# boot, so mounting / fails with its reason on the status line, and the
# shell starts on the empty read-only /. Afterwards the runner puts the
# magic back and e2fsck finds the filesystem untouched.
break-root
timeout 30
expect \[FAIL\] mount /: ext2 on 00:02\.0 port 2 partition 2, 190 MiB: Invalid argument
expect root@relay:/# $
send ls -a /
expect \n\.\s+\.\.\n
send touch /f
expect \ntouch: cannot touch '/f': Read-only file system\n
send echo still here
expect \nstill here\n
````

- [ ] **Step 2: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn exit_statuses_match_the_kernel() {
````

with:

````rust
    #[test]
    fn parses_break_root() {
        let s = parse_scenario("x", "break-root\nexpect a").unwrap();
        assert!(s.break_root);
        assert!(!parse_scenario("x", "expect a").unwrap().break_root);
        assert!(parse_scenario("x", "expect a\nbreak-root").is_err());
    }

    #[test]
    fn breaking_the_root_hides_it_until_it_is_restored() {
        let dir = out_dir().join("e2e-selftest").join("break-root");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        crate::util::run(
            std::process::Command::new("mke2fs")
                .args(["-q", "-F", "-t", "ext2"])
                .arg(&img)
                .arg("1M"),
        )
        .unwrap();
        let whole = Partition {
            start_lba: 0,
            sectors: 2048,
        };
        image::set_ext2_magic(&img, whole, false).unwrap();
        assert!(image::fsck(&img, whole).is_err(), "no filesystem found");
        image::set_ext2_magic(&img, whole, true).unwrap();
        image::fsck(&img, whole).unwrap();
    }

    #[test]
    fn exit_statuses_match_the_kernel() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find function `set_ext2_magic` in module `image` ``; `` no field `break_root` on type `Scenario` ``.

- [ ] **Step 4: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, replace:

````markdown
| `port 15: setup failed: …` | Enumeration failed three times | Replug the stick into the same port and reboot; `debug=usb` shows each try |
| `[FAIL] mount /: no disk with the boot partition` | The loader's boot GUID matches no partition | Note the `boot info` line; `dmesg` shows the `storage:` GPT line |
| `[FAIL] mount /: …; mounted read-only` | The stick refused a write (worn out or write-protected) | The files can be read; note the `usb: … write at block N:` line in `dmesg` |
| `[FAIL] mount /: … partition 2: Invalid argument` | The ext2 root is not what `flash --full` writes | `dmesg` shows the `ext2:` reason; re-run `flash --full` |
| A command prints `Input/output error` | A disk request failed after three tries | `dmesg`: the `storage:` and `usb:` lines name the command and block |
````

with:

````markdown
| `port 15: setup failed: …` | Enumeration failed three times | Replug the stick into the same port and reboot; `debug=usb` shows each try |
| `[FAIL] mount /: no disk with a GPT` | The stick was set up but its reads fail, or it has no GPT | `dmesg`: a `usb: 00:14.0 port 15: read at block N: …` line and the `storage: slot N:` lines with the sense mean the reads fail; `storage: 00:14.0 port 15: no valid GPT` means re-run `flash --full` |
| `mount /: warning: no disk has the boot partition …; using …` above `[ ok ] mount /` | The loader's boot GUID matches no partition, and the one disk with an ESP and a Linux partition was used | Note the GUID in the warning and the `boot info` line; the files are usable |
| `[FAIL] mount /: no disk with the boot partition` | No disk has the boot partition, and none has exactly one ESP and one Linux partition | `dmesg`: the `storage:` GPT lines list what each disk has |
| `[FAIL] mount /: …; mounted read-only` | The stick refused a write (worn out or write-protected) | The files can be read; note the `usb: … write at block N:` line in `dmesg` |
| `[FAIL] mount /: ext2 on 00:14.0 port 15 partition 2, 14.3 GiB: Invalid argument` | The ext2 root is not what `flash --full` writes | `dmesg` shows the `ext2:` reason; re-run `flash --full` |
| A command prints `Input/output error` | A disk request failed after three tries | `dmesg`: the `storage:` and `usb:` lines name the command and block |
````

- [ ] **Step 5: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! disk small                       (before any other step: the 32 MiB root)
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
````

with:

````rust
//! disk small                       (before any other step: the 32 MiB root)
//! break-root                       (before any other step: the root's ext2
//!                                   magic is zeroed until the scenario ends)
//! esp-write /EFI/RELAY/kernel.elf garbage   (before boot: replace an ESP file)
````

Replace:

````rust
    pub small_disk: bool,
    /// ESP files to overwrite before booting: (path, contents).
````

with:

````rust
    pub small_disk: bool,
    /// `break-root`: the root filesystem is unrecognisable while the
    /// scenario runs.
    pub break_root: bool,
    /// ESP files to overwrite before booting: (path, contents).
````

Replace:

````rust
    let mut small_disk = false;
    let mut esp_writes = Vec::new();
````

with:

````rust
    let mut small_disk = false;
    let mut break_root = false;
    let mut esp_writes = Vec::new();
````

Replace:

````rust
                small_disk = true;
                continue;
````

with:

````rust
                small_disk = true;
                continue;
            }
            "break-root" => {
                if !steps.is_empty() {
                    bail!("{name}:{line_no}: break-root must come before other steps");
                }
                break_root = true;
                continue;
````

Replace:

````rust
        small_disk,
        esp_writes,
````

with:

````rust
        small_disk,
        break_root,
        esp_writes,
````

Replace:

````rust
        esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?;
    }
````

with:

````rust
        esp_write(&q.disk, layout.esp, path, contents.as_bytes(), run_dir)?;
    }
    if scenario.break_root {
        image::set_ext2_magic(&q.disk, layout.root, false)?;
    }
````

Replace:

````rust
    drop(r);
    image::fsck(&disk, layout.root).with_context(|| {
````

with:

````rust
    drop(r);
    // Repaired, the broken root must be as the machine found it: nothing
    // was written to what it could not mount.
    if scenario.break_root {
        image::set_ext2_magic(&disk, layout.root, true)?;
    }
    image::fsck(&disk, layout.root).with_context(|| {
````

- [ ] **Step 6: Change `xtask/src/image.rs`**

In `xtask/src/image.rs`, replace:

````rust

/// The ext2 filesystem state in `part`'s superblock, as `dumpe2fs -h` names
````

with:

````rust

/// Where the ext2 superblock's magic number is: 56 bytes into the
/// superblock, which starts 1024 bytes into the partition.
const EXT2_MAGIC_OFFSET: u64 = 1024 + 56;

/// Writes the ext2 magic number (0xEF53) into `part`'s superblock, or
/// zeroes it, so no ext2 driver recognises the filesystem.
pub fn set_ext2_magic(target: &Path, part: Partition, present: bool) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut f = fs::OpenOptions::new().write(true).open(target)?;
    f.seek(SeekFrom::Start(part.offset() + EXT2_MAGIC_OFFSET))?;
    f.write_all(if present { &[0x53, 0xEF] } else { &[0, 0] })?;
    Ok(())
}

/// The ext2 filesystem state in `part`'s superblock, as `dumpe2fs -h` names
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 47 tests.

- [ ] **Step 8: Run the mount_fail scenario**

Run: `cargo xtask test --e2e-only --scenario mount_fail`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add docs tests xtask
git commit -m "xtask: the mount_fail scenario: a root that cannot be mounted leaves the empty read-only /"
````


### Finish PR 6

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 18 scenario(s) passed`.

````bash
git push -u origin plan5/root
gh pr create --base main --head plan5/root --title "Plan 5: / on the USB stick" --body-file - <<'EOF'
## What

Milestone 1, plan 5, tasks 18–22: USB disks as block devices over `HOSTS`; startup step 9 (GPT, root choice, ext2 read-write with the read-only retry and the empty `/` as the last resort); the log line naming how the root was chosen; the scenarios `fileops`, `persist`, `display`, `bigfile`, `diskfull` and `mount_fail` with the runner's `disk small`, `break-root` and `file-lines`; `boot` ends with `[ ok ] mount /` and the motd; NUC check 3 in `docs/hardware-test.md`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests and QEMU scenarios

## Hardware

- [x] Needed: NUC check 3 (the full checklist of spec §9.4), run by the user with `cargo xtask flash --full` from this worktree; the result goes into `docs/hardware-test.md`'s results log
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan5/root --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan5-root
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
