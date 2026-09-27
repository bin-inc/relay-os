# Milestone 1 · Plan 4: USB Keyboard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Type at the Relay shell on a USB keyboard: the `usb` crate (the `Hal` trait, an xHCI host-controller driver with the BIOS handoff, reset, scratchpad, 32/64-byte context, USB 2/USB 3 port and enumeration sequences of spec §6.2, and the HID boot keyboard of §6.3 with the US keymap, Caps Lock and auto-repeat), and the kernel side: DMA memory, every xHCI controller started at boot with hot-plug on its root ports, key presses as terminal input next to COM1, and plan 3's shell running at the end of boot on the empty read-only `/`. It ends with the QEMU `keyboard` scenario over QMP `send-key` and NUC check 2, typing on the K120.

**Architecture:** The `usb` crate is `no_std + alloc` and reaches hardware only through `Hal`, which also carries register reads and writes, so its tests run the driver against a fake xHCI controller that sees every access, is as strict as hardware, and plays fake devices modelled on the K120, the Unifying receiver, QEMU's keyboard and the Kingston stick. Class drivers talk to the controller through a small `Bus` trait; `Host` ties one controller to its keyboards. The kernel implements `Hal` over `mm::map_mmio`, the frame allocator, the TSC and the kernel log, keeps one `Host` per controller, and polls them from the shell's `Console`: key presses whenever it looks for input, hot-plug and LEDs only while idle. Pure kernel logic (the input queue and key translation, PCI power-up, DMA frame math, status lines) lives in the kernel's library target and is host-tested; QEMU checks the whole path end to end.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates (`usb` has no dependencies at all); QEMU 8.2 (`qemu-xhci`, `usb-kbd`, `usb-storage`) with QMP `send-key` for the e2e step `key`.

**Spec:** `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md` (§2, §4.4, §5.3, §5.5, §6.1–6.3, §7.2, §7.3, §9.3, §9.4, §10, §12, §13, §15)
**Roadmap:** `docs/superpowers/plans/2026-09-26-milestone-1-roadmap.md` — this is plan 4 of 6.

## Where this plan fits

Plan 4 implements spec §11 step 5. It builds on plan 2's kernel core (`map_mmio`, the frame allocator, the timer, PCI) and plan 3's `shell` and `vfs` crates. Plan 5 adds the USB mass-storage driver to the same `Host` and `Bus`, mounts ext2 at `/` and implements `reboot` and `poweroff`; until then the boot says `[FAIL] mount /: no storage driver yet`, and the Kingston stick is set up but not claimed, which already exercises the SuperSpeed path on the NUC.

## Working conventions

- Plan 4 lands as **seven pull requests** (table below). This plan itself is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-09-27-m1-plan-4-usb-keyboard.md`, to every task, and all tasks share one workspace directory.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module; the USB stack's test support under `crates/usb/src/testing/`, which is compiled only for tests; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match.
- Every task ends with `cargo xtask lint` and a commit. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats (it throttled at 99 °C during plan 3) before blaming code.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `511e987` and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `plan4/plan` | — | This plan, spec §15 item 10, roadmap notes | `lint`, `unit`, `e2e` |
| 2 | `plan4/kernel-shell` | 1–2 | Input queue and COM1 receive; the shell at the end of boot on the empty read-only `/`; the `shell` scenario | `lint`, `unit`, `e2e` |
| 3 | `plan4/usb-core` | 3–6 | The `usb` crate: `Hal`, `DmaBuf`, errors, requests, descriptors, `Bus`; TRBs and rings; contexts; registers and capabilities; the fake controller | `lint`, `unit`, `e2e` |
| 4 | `plan4/xhci-start` | 7–9 | BIOS handoff, halt and reset; scratchpads, DCBAA, rings, run; commands, aborts, events, `poll` | `lint`, `unit`, `e2e` |
| 5 | `plan4/xhci-devices` | 10–12 | Port reset for USB 2 and USB 3, port changes; enumeration and control transfers; endpoint configuration, IN transfers, detach | `lint`, `unit`, `e2e` |
| 6 | `plan4/hid-keyboard` | 13–15 | The US keymap, the HID boot keyboard, `Host` with hot-plug | `lint`, `unit`, `e2e` |
| 7 | `plan4/usb-kernel` | 16–19 | DMA memory, PCI power-up and the kernel `Hal`; key presses as terminal input; the e2e `key` step; USB at startup, the `keyboard` scenario, NUC check 2 | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; any assembly is inline (plan 4 adds none).
- Crate policy (spec §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin`; `xtask/` and dev-dependencies may use anything. `crates/usb` uses none of them: the xHCI and HID drivers are written here.
- `crates/usb` is `#![cfg_attr(not(test), no_std)]` with `extern crate alloc;`, and is listed in both `members` and `default-members` of the root `Cargo.toml`, so `cargo xtask lint` and `unit` cover it. Its test support (`crates/usb/src/testing/`) is `#[cfg(test)]`.
- The kernel stack is 64 KiB (spec §4.2): no big arrays on the stack; buffers are `Vec`/`Box` or DMA memory. Rings, contexts and transfer buffers are DMA memory from the frame allocator, never the stack or statics.
- Errors are values (spec §10): device failures are `UsbError`, never panics; nothing waits forever (registers 1 s, commands 500 ms, control transfers 1 s, port reset 500 ms); `poll` never waits. A device-level failure prints `[FAIL] <step>: <reason>` and the boot continues. Panics are for kernel bugs only. Nothing a device, a descriptor or a controller sends may make an `unwrap`, an index or an arithmetic overflow fail (the kernel builds with overflow checks).
- The NUC has no serial port: every xHCI step logs to `dmesg` (`xhci <pci>: …`) in enough detail to debug from a photo of the screen; the screen shows one line per controller and per device plus the status lines, because the terminal has 33 rows.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action in the workflow stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: MMIO and DMA code may raise `rust/access-invalid-pointer` alerts; triage each, and ask the user before dismissing any.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 10 in PR 1:

1. **The `Hal` also carries register access and the log** (refines §6.1). `map_mmio` returns the registers' virtual address and `read32`/`write32` read and write them, so host tests run the driver against a fake controller that sees every access with its real semantics (write-one-to-clear bits, doorbells). `map_mmio` and `alloc_dma` return `None` on failure; `log` adds a line to `dmesg`. `DmaBuf` is reached only through bounds-checked volatile accessors, and `alloc_dma` returns page-aligned memory.
2. **Class drivers see a `Bus`, not the xHCI driver:** control requests, IN transfers that complete later, halt recovery. `poll` never waits; what needs a control transfer (the Caps Lock LED, recovering a failed endpoint, setting up a new device) runs in a separate `service` step from the console's idle loop, where blocking is allowed.
3. **Every xHCI controller is started** (§5.5), not just the first: the NUC's Thunderbolt xHCI `00:0d.0` enumerates before the PCH's `00:14.0`, which has the keyboard and the stick. Each PCI function is put into power state D0 first (the firmware may leave an unused controller in D3hot), with its BARs written back if leaving D3hot reset it, then memory decoding and bus mastering are enabled. `[ ok ] usb` needs one working controller; one that fails gets a line of its own and is skipped. Controllers without 64-bit DMA, or with a page size other than 4 KiB, are refused.
4. **Hot-plug on root ports.** Devices are set up when they appear, at boot and later (after a 100 ms debounce), and dropped when they are unplugged, including an unplug and replug between two looks. Boot-time and hot-plugged devices take the same path.
5. **Waits and resets the spec leaves open.** After start the driver waits until 100 ms after the ports were reset or powered (USB 2.0's TSIGATT, the time a device may take to signal attach) and up to 1 s while a USB 3 link is still training, so devices present at boot are in the first scan. A port reset waits 500 ms. A USB 3 port that trains gets a hot reset before enumeration, as Linux does (the stick may keep its address from the firmware otherwise); one that does not train, or sits in SS.Inactive, gets one warm reset instead. Devices get 10 ms after `SET_ADDRESS` before the next request, as in Linux. A command that times out aborts the command ring (all 64 bits of CRCR are written, which some controllers need), the driver waits for Command Ring Stopped, and the timed-out TRB becomes a No Op; a controller whose abort fails, or that reports a host system error, is halted and not used again. USB time comes from the TSC, so timeouts expire even if the LAPIC timer did not start; without a TSC frequency USB is not started.
6. **Memory the controller may still use is never freed.** A slot's contexts, rings and buffers are freed only after Disable Slot succeeded; if it fails the memory is kept (a small leak, logged), and nothing of a dead controller is freed.
7. **Keyboard details.** `SET_PROTOCOL(boot)` must succeed (a keyboard that refuses it shows `keyboard not started: <reason>` on its boot line); a stalled `SET_IDLE` is only logged. A report with ErrorRollOver, POSTFail or ErrorUndefined in any key slot changes nothing, and a usage listed twice is one key. Caps Lock and its LED are per keyboard, and the LED is sent once per change. Keypad keys always give digits (Num Lock is not tracked); lock keys do not repeat; the newest key repeats, and after a long pause it repeats once, not in a burst. When a keyboard's endpoint fails every held key is released, and recovery is tried once a second, three times.
8. **Console input** (§7.2): keyboard and COM1 bytes join one queue with 4 KiB of type-ahead. A Ctrl-C typed while a command runs drops what was typed before it, as a terminal does. Keys send what a Linux terminal sends: Ctrl with a letter its control code, Enter CR, Backspace DEL, Insert/Delete/Page Up/Page Down `ESC [ 2~/3~/5~/6~`. Between polls the console sleeps until the next timer tick.
9. **The shell's output goes to the screen and serial, not into the kernel log**, so `dmesg` shows what the kernel reported.
10. **Until plan 5** the boot shows `[FAIL] mount /: no storage driver yet`, the shell runs on the empty read-only `/` of §10, and `reboot` and `poweroff` print the safe-to-power-off message and halt. The `relay: early boot complete` line is gone: the prompt is the last line of the boot.
11. **New cmdline word `debug=usb`** also shows the USB log on screen, for a NUC whose keyboard does not work (no serial port, so no `dmesg`).
12. **The e2e `key <text>` step** types over QMP `send-key` and then presses Enter, like `send`; `{up}`, `{backspace}`, `{caps_lock}`, `{ctrl-c}` and similar name keys without a character. Each press is held 30 ms, and the step returns once QEMU has played every press, so a following `send` cannot interleave.
13. **What QEMU 8.2 really does** (corrects §9.3): `qemu-xhci` numbers its USB 3 ports 1–4 and its USB 2 ports 5–8, and `usb-storage` connects at SuperSpeed, so a SuperSpeed port is exercised in QEMU too; `usb-kbd` is high-speed. 64-byte contexts, scratchpad buffers, the BIOS handoff, port power control, low- and full-speed devices and a USB 3 port that needs a warm reset remain covered only by the fake controller and the NUC.
14. **Deferred review findings** from plans 1–3 concern the loader, parts of the kernel plan 4 does not use, and the shell and ext2 over a real disk; they stay with plan 6. Plan 3's final-review minors are added to the roadmap, which did not list them yet.

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **A keyboard that comes and goes.** The Unifying receiver's keyboard sleeps and wakes, cables get pulled, the K120 moves to another port, or a key is held while it is unplugged. Expected: the keyboard is set up again when it reappears, its memory is freed when it goes, nothing keeps repeating, and other devices are untouched. Tests: `a_keyboard_plugged_in_later_is_picked_up_and_dropped_when_unplugged`, `a_quick_replug_replaces_the_device`, `a_held_key_stops_repeating_when_its_keyboard_is_unplugged` (Task 15); `a_failed_endpoint_releases_held_keys_and_recovers_in_service`, `a_disconnected_keyboard_stops_at_once` (Task 14); the unplug tests of Tasks 10–12.
2. **A second controller that is powered down, slow or broken** (the NUC's Thunderbolt xHCI enumerates first). Expected: it costs at most its timeouts, gets its own line, never hides the working controller or hangs the boot, and a dead controller never writes into memory that was given back. Tests: `a_function_in_d3_is_woken_to_d0`, `a_function_reset_by_its_wake_gets_its_bars_back` (Task 16); `an_abort_that_never_finishes_kills_and_halts_the_controller` (Task 9); `a_disable_slot_that_times_out_keeps_the_memory_the_controller_uses`, `detaching_on_a_dead_controller_frees_nothing` (Task 12); the all-ones, halt, reset and CNR timeout tests (Task 7); the freeing-on-failure tests of `Xhci::new` (Task 8); the aborted and dead-controller command tests (Task 9).
3. **A device that misbehaves during setup:** stalls a request, never answers, sends a malformed or oversized descriptor, has an EP0 packet size the default does not match, or is unplugged mid-enumeration. Expected: that device's attach fails with a reason on screen, everything it allocated is freed, and the rest of USB and the boot continue. Tests: the descriptor tests (Task 3); the STALL, timeout, short and oversized configuration, Evaluate Context and mid-enumeration unplug tests (Task 11); `a_device_that_fails_setup_is_reported_once` (Task 15).
4. **Typing faster than the machine reads,** or while a command runs. Expected: type-ahead is kept in order (up to 4 KiB), a Ctrl-C drops what was typed before it, a rollover report never invents keys, a long pause never makes a repeat burst, and unread key events are bounded. Tests: `typing_ahead_is_bounded`, `an_interrupt_drops_what_was_typed_before_it` (Task 1); `rollover_error_reports_change_nothing`, `a_long_pause_gives_one_repeat_not_a_burst` (Task 14); `unread_key_events_are_bounded` (Task 15).
5. **Hardware behaviour QEMU never shows:** the BIOS keeping the controller, 64-byte contexts, scratchpad buffers, port power control, devices that take their time to show up after reset, a low-speed keyboard, a SuperSpeed port that needs a warm reset, a slow `SET_ADDRESS`, PORTSC's write-one-to-clear bits, a command abort that moves past the aborted command. Expected: each sequence does what the xHCI specification says, checked against a fake that panics on anything the specification forbids, devices present at boot are on the boot screen, and everything a NUC photo needs is in the log. Tests: the handoff tests (Task 7); `intel`-preset start tests and `an_idle_controller_waits_only_the_attach_time` (Task 8); the abort tests (Task 9); the context tests at both strides (Task 5); `devices_present_at_boot_are_in_the_first_port_changes`, the neutral-PORTSC, hot-reset and warm-reset tests (Task 10); the K120 and Unifying receiver enumeration tests (Task 11); `a_keyboard_plugged_in_before_boot_is_found_by_the_first_service` (Task 15).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `Cargo.toml` | + `crates/usb` as a member and default member |
| `crates/usb/src/{lib,hal,error,bus}.rs` | `Hal`, `DmaBuf`, `UsbError`, `Speed`, `Setup`, `Bus` |
| `crates/usb/src/descriptor.rs` | Device and configuration descriptors |
| `crates/usb/src/xhci/{trb,ring}.rs` | TRBs; producer and event rings |
| `crates/usb/src/xhci/context.rs` | Device and input contexts at both strides, intervals |
| `crates/usb/src/xhci/{regs,caps}.rs` | Registers, `Params`, the extended capability list, the port map |
| `crates/usb/src/xhci/{init,start}.rs` | BIOS handoff, halt, reset; scratchpads, rings, run, `Xhci::new` |
| `crates/usb/src/xhci/command.rs` | Commands, aborts, `poll` |
| `crates/usb/src/xhci/port.rs` | Port reset, PORTSC writes, port changes |
| `crates/usb/src/xhci/{device,transfer,configure}.rs` | Enumeration, control and IN transfers, recovery, Configure Endpoint, detach, `Bus` |
| `crates/usb/src/xhci/mod.rs` | `Xhci`, `ControllerInfo`, `Device`, `PortChange`, slot and endpoint state |
| `crates/usb/src/hid/{keymap,keyboard}.rs` | The US keymap; the HID boot keyboard |
| `crates/usb/src/host.rs` | One controller with its keyboards; hot-plug |
| `crates/usb/src/testing/**` | Fake `Hal`, fake xHCI controller, fake devices (tests only) |
| `kernel/src/input.rs` | Input queue, Ctrl-C, key presses as terminal bytes |
| `kernel/src/session.rs` | `shell::Console`, `shell::System`, `vfs::Env`; running the shell |
| `kernel/src/usb.rs` | `KernelHal`, startup step 8, polling and hot-plug for the console |
| `kernel/src/{serial,console,klog,arch/mod,timer,pci,cmdline,lib}.rs`, `kernel/src/mm/mod.rs` | COM1 receive; output without the log; the whole log; waiting for an interrupt; TSC time; PCI power-up; `debug=usb`; the startup order; DMA memory |
| `xtask/src/{keys,e2e}.rs` | The e2e `key` step |
| `tests/e2e/{boot,boot_bigmode,shell,keyboard}.txt` | Boot to the prompt; the shell over COM1; typing on the USB keyboard |
| `docs/hardware-test.md`, `README.md` | NUC check 2; the new crate |

---

## PR 1: The plan (already committed)

This plan, the spec's §15 item 10 and the roadmap notes are committed on branch `plan4/plan` (worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-plan`). They change no code, but CI runs on every pull request. Run the commands below from that worktree.

- [ ] **Push and open the pull request**

````bash
git push -u origin plan4/plan
gh pr create --base main --head plan4/plan --title "Plan 4: USB keyboard (plan document)" --body-file - <<'EOF'
## What

Milestone 1, plan 4 (USB keyboard): the full implementation plan, the spec's §15 item 10 (decisions made while planning) and roadmap notes.

## How it was tested

- [x] Every task was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks plan4/plan --watch`). Ask the user to review and merge. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-plan` and continue with PR 2.

---

## PR 2: The shell in the kernel, over COM1 (Tasks 1–2)

Plan 3's shell at the end of boot, on the empty read-only `/`, typed at over COM1. It needs nothing from the USB stack, so it lands first and the later PRs only add the keyboard as a second input.

Branch `plan4/kernel-shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-kernel-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/kernel-shell /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-kernel-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-kernel-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/kernel-shell /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-kernel-shell plan4/plan`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: Console input: the input queue and COM1 receive

Spec §7.2: the shell reads one stream of bytes, whatever typed them. `InputQueue` holds bytes from every source until the shell reads them. Typing ahead while a command runs is kept (4 KiB at most, so a stuck key cannot fill the heap), and `take_interrupt` answers `Console::interrupted`: a Ctrl-C drops itself and everything typed before it, as a Linux terminal flushes its input on an interrupt, and keeps what came after. COM1 learns to receive (Line Status bit 0, Data Ready), polled and without interrupts; on the NUC, which has no UART, it always returns nothing. The USB keyboard joins the same queue in Tasks 17 and 19.

**Files:**
- Create: `kernel/src/input.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/serial.rs`

**Interfaces:**
- Consumes: `serial::SERIAL` (plan 1).
- Produces: `relay_kernel::input::{INTERRUPT = 0x03, QUEUE_MAX = 4096, InputQueue}` with `InputQueue::{new(), push(&mut self, &[u8]), pop(&mut self) -> Option<u8>, is_empty(&self) -> bool, take_interrupt(&mut self) -> bool}`; `relay_kernel::serial::read_byte() -> Option<u8>` (never waits).

- [ ] **Step 1: Write the failing tests for `kernel/src/input.rs`**

Create `kernel/src/input.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    fn drain(q: &mut InputQueue) -> Vec<u8> {
        core::iter::from_fn(|| q.pop()).collect()
    }

    #[test]
    fn bytes_come_out_in_order() {
        let mut q = InputQueue::new();
        assert!(q.is_empty());
        q.push(b"ls");
        q.push(b" -l\r");
        assert_eq!(drain(&mut q), b"ls -l\r");
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn typing_ahead_is_bounded() {
        let mut q = InputQueue::new();
        q.push(&[b'a'; QUEUE_MAX - 1]);
        q.push(b"bcd");
        let got = drain(&mut q);
        assert_eq!(got.len(), QUEUE_MAX);
        assert_eq!(got[QUEUE_MAX - 1], b'b');
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
        let mut q = InputQueue::new();
        q.push(b"rm x\x03 ls\x03pwd\r");
        assert!(q.take_interrupt());
        assert_eq!(drain(&mut q), b"pwd\r");
        q.push(b"echo");
        assert!(!q.take_interrupt());
        assert_eq!(drain(&mut q), b"echo");
    }
}
````

- [ ] **Step 2: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod console;
pub mod klog;
````

with:

````rust
pub mod console;
pub mod input;
pub mod klog;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `InputQueue` in this scope ``; `` cannot find value `QUEUE_MAX` in this scope ``.

- [ ] **Step 4: Implement `kernel/src/input.rs`**

Insert this at the top of `kernel/src/input.rs`, above `#[cfg(test)]`:

````rust
//! Console input (spec §7.2): bytes from every source (the USB keyboard,
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it.

use alloc::collections::VecDeque;

/// Ctrl-C.
pub const INTERRUPT: u8 = 0x03;
/// Bytes typed ahead beyond this are dropped (a stuck key during a long
/// command must not fill the heap).
pub const QUEUE_MAX: usize = 4096;

pub struct InputQueue {
    bytes: VecDeque<u8>,
}

impl InputQueue {
    pub const fn new() -> InputQueue {
        InputQueue {
            bytes: VecDeque::new(),
        }
    }

    /// Adds input; what does not fit is dropped.
    pub fn push(&mut self, bytes: &[u8]) {
        let room = QUEUE_MAX - self.bytes.len();
        self.bytes.extend(&bytes[..bytes.len().min(room)]);
    }

    /// The oldest byte.
    pub fn pop(&mut self) -> Option<u8> {
        self.bytes.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Whether a Ctrl-C is waiting. If one is, it and everything typed
    /// before it are dropped, as a terminal flushes its input on an
    /// interrupt; what was typed after it stays.
    pub fn take_interrupt(&mut self) -> bool {
        match self.bytes.iter().rposition(|&b| b == INTERRUPT) {
            Some(i) => {
                self.bytes.drain(..=i);
                true
            }
            None => false,
        }
    }
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

````

- [ ] **Step 5: Change `kernel/src/serial.rs`**

In `kernel/src/serial.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! COM1 (16550 UART at 0x3F8). Present in QEMU, absent on the NUC; the probe
//! makes every call a no-op when there is no UART.

````

with:

````rust
//! COM1 (16550 UART at 0x3F8). Present in QEMU, absent on the NUC; the probe
//! makes every call a no-op when there is no UART. Output mirrors the
//! console; input joins the keyboard's (spec §7.2), polled, without
//! interrupts.

````

Replace:

````rust

    /// Writes bytes, translating `\n` to `\r\n`.
````

with:

````rust

    /// A received byte, if one is waiting (Line Status bit 0, Data Ready).
    fn read_byte(&mut self) -> Option<u8> {
        unsafe {
            if Port::<u8>::new(self.base + 5).read() & 0x01 == 0 {
                return None;
            }
            Some(Port::<u8>::new(self.base).read())
        }
    }

    /// Writes bytes, translating `\n` to `\r\n`.
````

Replace:

````rust

pub fn write(bytes: &[u8]) {
````

with:

````rust

/// The next byte received on COM1, if any. Never waits; `None` on machines
/// without a UART (the NUC).
pub fn read_byte() -> Option<u8> {
    SERIAL.lock().as_mut().and_then(|s| s.read_byte())
}

pub fn write(bytes: &[u8]) {
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 93 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -m "kernel: console input queue and COM1 receive"
````


### Task 2: The shell at the end of boot

Plan 3's shell runs in the kernel (spec §4.4 step 10). `session.rs` implements its three traits over the kernel: `KernelConsole` (output to the screen and serial; input from the queue of Task 1, fed from COM1; `columns` from the terminal; `interrupted` polls the input without waiting, so a Ctrl-C stops a long command), `KernelSystem` (`rtc::now_unix`, `mm::stats` as `MemInfo` in bytes, the kernel log for `dmesg`) and `KernelEnv` (`vfs::Env` over the RTC and `klogln!`). There is no storage driver until plan 5, so boot says `[FAIL] mount /: no storage driver yet` and the shell starts on the empty read-only `MemFs` of spec §10; `reboot` and `poweroff` print the safe-to-power-off message and halt until plan 5 can shut a disk down cleanly. The shell's output goes to the screen and serial but not into the kernel log (`console::write_output`), so `dmesg` shows what the kernel reported and not what commands printed. While it waits for input the console sleeps until the next timer tick (`arch::wait_for_interrupt`) instead of spinning; without the timer interrupt it only pauses. The `relay: early boot complete` line is gone: the prompt is the last line of the boot. The new `shell` scenario types over COM1.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/arch/mod.rs`
- Modify: `kernel/src/console.rs`
- Modify: `kernel/src/klog.rs`
- Modify: `kernel/src/lib.rs`
- Create: `kernel/src/session.rs`
- Modify: `tests/e2e/boot.txt`
- Modify: `tests/e2e/boot_bigmode.txt`
- Create: `tests/e2e/shell.txt`

**Interfaces:**
- Consumes: `input::InputQueue`, `serial::read_byte` (Task 1); `shell::{Shell, Console, System, MemInfo}`, `vfs::{Env, MemFs, MountTable}` (plan 3); `mm::stats`, `rtc::now_unix`, `klog::KLOG`, `console::size` (plans 1–2).
- Produces: `relay_kernel::session::{KernelConsole, KernelSystem, KernelEnv, mem_info(MemStats) -> MemInfo, run_shell() -> !}`; `console::write_output(&[u8])` (screen and serial, not the log); `klog::Ring::to_vec(&self) -> Vec<u8>`; `arch::wait_for_interrupt()`; boot ends with `[FAIL] mount /: no storage driver yet` and the prompt `root@relay:/# `; scenario `tests/e2e/shell.txt`.

- [ ] **Step 1: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
term.workspace = true
x86_64.workspace = true
````

with:

````toml
term.workspace = true
shell.workspace = true
vfs.workspace = true
x86_64.workspace = true
````

- [ ] **Step 2: Add the failing tests to `kernel/src/klog.rs`**

In `kernel/src/klog.rs`, replace:

````rust
    #[test]
    fn empty_ring() {
````

with:

````rust
    #[test]
    fn the_whole_log_comes_out_oldest_first() {
        let mut r: Ring<8> = Ring::new();
        r.write(b"abc");
        assert_eq!(r.to_vec(), b"abc");
        r.write(b"defghij");
        assert_eq!(r.to_vec(), b"cdefghij");
    }

    #[test]
    fn empty_ring() {
````

- [ ] **Step 3: Declare the new module in `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod serial;
pub mod timer;
````

with:

````rust
pub mod serial;
pub mod session;
pub mod timer;
````

- [ ] **Step 4: Write the failing tests for `kernel/src/session.rs`**

Create `kernel/src/session.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::heap::HeapStats;

    #[test]
    fn memory_figures_are_in_bytes() {
        let s = MemStats {
            free_frames: 3,
            total_frames: 5,
            heap: HeapStats {
                total: 32 << 20,
                used: 1024,
                free_large: 0,
                largest_free: 0,
            },
        };
        assert_eq!(
            mem_info(s),
            MemInfo {
                ram_total: 5 * 4096,
                ram_free: 3 * 4096,
                heap_total: 32 << 20,
                heap_used: 1024,
            }
        );
    }
}
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# The loader hands off, the kernel draws on the framebuffer and every
# startup step reports ok.
timeout 30
````

with:

````text
# The loader hands off, the kernel draws on the framebuffer, every startup
# step reports ok, and the shell starts on an empty read-only / (there is
# no storage driver yet).
timeout 30
````

Replace:

````text
expect \[ ok \] pci: \d+ devices on bus 00, xHCI at 00:02\.0
expect relay: early boot complete
screenshot-nonblank
````

with:

````text
expect \[ ok \] pci: \d+ devices on bus 00, xHCI at 00:02\.0
expect \[FAIL\] mount /: no storage driver yet
expect root@relay:/# $
screenshot-nonblank
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/boot_bigmode.txt`**

In `tests/e2e/boot_bigmode.txt`, replace:

````text
expect \[ ok \] console 2560x1600 \(120x33 cells\)
expect relay: early boot complete
screenshot-nonblank
````

with:

````text
expect \[ ok \] console 2560x1600 \(120x33 cells\)
expect root@relay:/# $
screenshot-nonblank
````

- [ ] **Step 7: Add the scenario `tests/e2e/shell.txt`**

The patterns start with `\n`: the shell echoes what is typed, so `hello` alone would match the typed command instead of its output.

Create `tests/e2e/shell.txt`:

````text
# The shell at the end of boot, typed at over COM1: the empty read-only /
# of spec §10 and the commands that report on the machine.
timeout 30
expect root@relay:/# $
send echo hello, world
expect \nhello, world\n
send uname -a
expect \nRelay relay 0\.1\.0 x86_64\n
send ls -a /
expect \n\.\s+\.\.\n
send touch /f
expect \ntouch: cannot touch '/f': Read-only file system\n
send date
expect \n(Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d\n
send free
expect \nMem:\s+\d+\s+\d+\s+\d+\nHeap:\s+32768\s+\d+\s+\d+\n
send dmesg
expect \n\[ ok \] pci: .*\n
send nosuch
expect \nrelay-sh: nosuch: command not found\n
expect root@relay:/# $
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `MemStats` in this scope ``; `` cannot find struct, variant or union type `MemInfo` in this scope ``.

- [ ] **Step 9: Change `kernel/src/arch/mod.rs`**

In `kernel/src/arch/mod.rs`, replace:

````rust

/// Waits for interrupts forever: the CPU sleeps between timer ticks.
````

with:

````rust

/// Sleeps until the next interrupt (the 1 kHz tick) when interrupts are on;
/// without them (the timer failed to start) it only pauses briefly, so
/// callers that poll keep running.
pub fn wait_for_interrupt() {
    if x86_64::instructions::interrupts::are_enabled() {
        x86_64::instructions::hlt();
    } else {
        core::hint::spin_loop();
    }
}

/// Waits for interrupts forever: the CPU sleeps between timer ticks.
````

- [ ] **Step 10: Change `kernel/src/console.rs`**

In `kernel/src/console.rs`, replace:

````rust
pub fn write_bytes(bytes: &[u8]) {
    klog::KLOG.lock().write(bytes);
    serial::write(bytes);
    if let Some(c) = CONSOLE.lock().as_mut() {
````

with:

````rust
pub fn write_bytes(bytes: &[u8]) {
    klog::KLOG.lock().write(bytes);
    write_output(bytes);
}

/// Writes to the screen and serial but not the kernel log: the shell's
/// output (`dmesg` shows what the kernel reported, not what commands
/// printed).
pub fn write_output(bytes: &[u8]) {
    serial::write(bytes);
    if let Some(c) = CONSOLE.lock().as_mut() {
````

- [ ] **Step 11: Change `kernel/src/klog.rs`**

In `kernel/src/klog.rs`, replace:

````rust
        self.buf[(self.start + i) % N]
    }
````

with:

````rust
        self.buf[(self.start + i) % N]
    }

    /// The whole log, oldest byte first (for `dmesg`).
    pub fn to_vec(&self) -> alloc::vec::Vec<u8> {
        (0..self.len).map(|i| self.byte(i)).collect()
    }
````

- [ ] **Step 12: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    }
    kprintln!("relay: early boot complete");
    arch::idle_forever()
}
````

with:

````rust
    }
    // There is no storage driver yet, so the shell starts on an empty,
    // read-only `/` (spec §10).
    console::fail("mount /", format_args!("no storage driver yet"));
    session::run_shell()
}
````

- [ ] **Step 13: Implement `kernel/src/session.rs`**

Insert this at the top of `kernel/src/session.rs`, above `#[cfg(test)]`:

````rust
//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console`, the clock, memory figures and kernel log as
//! `shell::System`, and `vfs::Env` for filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, rtc, serial};
use alloc::boxed::Box;
use alloc::vec::Vec;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, MemFs, MountTable};

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

/// The screen and serial for output; COM1 for input.
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
        for _ in 0..SERIAL_BURST {
            match serial::read_byte() {
                Some(b) => self.input.push(&[b]),
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
            }
            arch::wait_for_interrupt();
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }

    fn columns(&self) -> usize {
        console::size().map_or(80, |(cols, _)| cols)
    }

    fn interrupted(&mut self) -> bool {
        self.poll();
        self.input.take_interrupt()
    }
}

/// The frame allocator's and the heap's figures, in bytes, for `free`.
pub fn mem_info(s: MemStats) -> MemInfo {
    MemInfo {
        ram_total: s.total_frames * FRAME_SIZE,
        ram_free: s.free_frames * FRAME_SIZE,
        heap_total: s.heap.total as u64,
        heap_used: s.heap.used as u64,
    }
}

pub struct KernelSystem;

impl System for KernelSystem {
    fn now(&self) -> u64 {
        rtc::now_unix().unwrap_or(0)
    }

    fn memory(&self) -> Option<MemInfo> {
        Some(mem_info(mm::stats()))
    }

    fn kernel_log(&self) -> Vec<u8> {
        klog::KLOG.lock().to_vec()
    }

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

/// The RTC and the kernel log, for filesystems.
pub struct KernelEnv;

impl Env for KernelEnv {
    fn now(&self) -> u64 {
        rtc::now_unix().unwrap_or(0)
    }

    fn log(&self, line: &str) {
        klogln!("{line}");
    }
}

/// Runs the shell on an empty read-only `/` (spec §10). Never returns.
pub fn run_shell() -> ! {
    let root = MemFs::new(Box::new(KernelEnv)).read_only();
    let mut vfs = MountTable::new(Box::new(root));
    let mut console = KernelConsole::new();
    let mut system = KernelSystem;
    Shell::new(&mut vfs, &mut console, &mut system).run();
    // `run` returns only if `reboot` or `poweroff` do, which they do not.
    arch::halt_forever()
}

````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 95 tests.

- [ ] **Step 15: Run the boot and shell scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario boot_bigmode`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add Cargo.lock kernel tests
git commit -m "kernel: the shell at the end of boot, over COM1"
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 10 scenario(s) passed`.

````bash
git push -u origin plan4/kernel-shell
gh pr create --base main --head plan4/kernel-shell --title "Plan 4: The shell in the kernel, over COM1" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 1–2: the console input queue (type-ahead, Ctrl-C while a command runs) and COM1 receive; the shell at the end of boot on the empty read-only `/` with `[FAIL] mount /: no storage driver yet`; shell output kept out of `dmesg`; the `shell` scenario.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests and QEMU scenarios

## Hardware

- [x] Not needed yet: on the NUC the shell has no input until the USB keyboard (PR 7); the boot still ends at the prompt
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/kernel-shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-kernel-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: The usb crate's foundations (Tasks 3–6)

The crate, its `Hal`, errors, requests and descriptors, and the xHCI data structures (rings, contexts, registers, capabilities), each tested on the host; the fake controller starts here.

Branch `plan4/usb-core`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-core`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/usb-core /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-core origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-core
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/kernel-shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/usb-core /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-core plan4/kernel-shell`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/kernel-shell>` and re-run `cargo xtask ci` before pushing.

### Task 3: The usb crate: Hal, errors, requests and descriptors

The new `usb` crate (spec §6) reaches hardware only through `Hal` (spec §6.1, revised by decision 1): `map_mmio` returns the registers' virtual address and `read32`/`write32` access them, so the tests run the driver against a fake controller that sees every access with its real semantics; `map_mmio` and `alloc_dma` return `None` instead of failing hard; `log` adds a line to `dmesg`. `DmaBuf` is memory the device reads and writes, reached only through bounds-checked volatile accessors, and freed by giving it back. `UsbError` is how every device failure is reported (spec §10). `Bus` is what class drivers see of a controller (decision 2): control requests, IN transfers that complete later, halt recovery. `Setup` builds the 8-byte setup packets; `descriptor` parses device and configuration descriptors without trusting them: a zero length, a descriptor running past the end or a truncated header is an error, never a loop or a panic, and alternate settings, repeated interface numbers and endpoint 0 entries are skipped.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `Cargo.toml`
- Create: `crates/usb/Cargo.toml`
- Create: `crates/usb/src/bus.rs`
- Create: `crates/usb/src/descriptor.rs`
- Create: `crates/usb/src/error.rs`
- Create: `crates/usb/src/hal.rs`
- Create: `crates/usb/src/lib.rs`

**Interfaces:**
- Consumes: nothing (a new crate without dependencies).
- Produces: `usb::{Hal { map_mmio(&self, phys: u64, len: usize) -> Option<usize>, unsafe read32(&self, addr: usize) -> u32, unsafe write32(&self, addr: usize, value: u32), alloc_dma(&self, size, align) -> Option<DmaBuf>, free_dma(&self, DmaBuf), now(&self) -> Duration, sleep(&self, Duration), log(&self, fmt::Arguments) }, DmaBuf { unsafe new(virt, phys, size), phys, phys_at, size, virt, read32, write32, read64, write64, read_bytes, write_bytes, zero }, UsbError { Timeout, Stall, Transfer(u8), Command(u8), NoMemory, NotResponding, Unsupported(&str), BadDescriptor(&str), Disconnected, ControllerDead }, Speed { Low, Full, High, Super, SuperPlus } (default_max_packet0, is_superspeed, Display), Setup { request_type, request, value, index, length } (is_in, to_bytes, get_descriptor, set_configuration, clear_halt), Bus { control, queue_in, take_in, clear_halt, now, log }}`; `usb::bus::{CLEAR_FEATURE, GET_DESCRIPTOR, SET_CONFIGURATION, DIR_IN, TYPE_CLASS, RECIPIENT_INTERFACE, RECIPIENT_ENDPOINT, ENDPOINT_HALT}`; `usb::descriptor::{DeviceDescriptor::parse, max_packet0(prefix, Speed), configuration_length, parse_configuration, Configuration { value, interfaces }, Interface { number, class, subclass, protocol, endpoints }, Endpoint { address, kind, max_packet, interval, max_burst } (is_in, number, packet_size, extra_transactions), EndpointKind}`.

- [ ] **Step 1: Change `Cargo.toml`**

In `Cargo.toml`, make these 2 replacements, top to bottom:

Replace:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "xtask"]

````

with:

````toml
resolver = "3"
members = ["boot", "kernel", "crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "xtask"]
default-members = ["crates/boot-info", "crates/term", "crates/vfs", "crates/shell", "crates/ext2", "crates/usb", "xtask"]

````

Replace:

````toml
ext2 = { path = "crates/ext2" }
uefi = { version = "0.41", default-features = false }
````

with:

````toml
ext2 = { path = "crates/ext2" }
usb = { path = "crates/usb" }
uefi = { version = "0.41", default-features = false }
````

- [ ] **Step 2: Create `crates/usb/Cargo.toml`**

Create `crates/usb/Cargo.toml`:

````toml
[package]
name = "usb"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
````

- [ ] **Step 3: Write the failing tests for `crates/usb/src/bus.rs`**

Create `crates/usb/src/bus.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn setup_packets_are_little_endian() {
        let s = Setup::get_descriptor(2, 0, 0x0109);
        assert!(s.is_in());
        assert_eq!(s.to_bytes(), [0x80, 6, 0, 2, 0, 0, 0x09, 0x01]);
        assert_eq!(
            Setup::set_configuration(1).to_bytes(),
            [0, 9, 1, 0, 0, 0, 0, 0]
        );
        let c = Setup::clear_halt(0x81);
        assert!(!c.is_in());
        assert_eq!(c.to_bytes(), [2, 1, 0, 0, 0x81, 0, 0, 0]);
    }

    #[test]
    fn default_packet_sizes_follow_the_speed() {
        assert_eq!(Speed::Low.default_max_packet0(), 8);
        assert_eq!(Speed::Full.default_max_packet0(), 8);
        assert_eq!(Speed::High.default_max_packet0(), 64);
        assert_eq!(Speed::Super.default_max_packet0(), 512);
        assert!(Speed::SuperPlus.is_superspeed() && !Speed::High.is_superspeed());
        assert_eq!(Speed::Low.to_string(), "low-speed");
        assert_eq!(Speed::Super.to_string(), "SuperSpeed");
    }
}
````

- [ ] **Step 4: Write the failing tests for `crates/usb/src/descriptor.rs`**

Create `crates/usb/src/descriptor.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// The Logitech K120's configuration: a boot keyboard and a second HID
    /// interface for its extra keys, each with a HID descriptor.
    pub const K120: [u8; 59] = [
        9, 2, 59, 0, 2, 1, 0, 0xA0, 50, // configuration 1, 2 interfaces
        9, 4, 0, 0, 1, 3, 1, 1, 0, // interface 0: HID, boot, keyboard
        9, 0x21, 0x10, 1, 0, 1, 0x22, 65, 0, // HID descriptor
        7, 5, 0x81, 3, 8, 0, 10, // endpoint 1 IN, interrupt, 8 bytes, 10 ms
        9, 4, 1, 0, 1, 3, 0, 0, 0, // interface 1: HID, no boot protocol
        9, 0x21, 0x10, 1, 0, 1, 0x22, 51, 0, // HID descriptor
        7, 5, 0x82, 3, 4, 0, 0xFF, // endpoint 2 IN, interrupt, 4 bytes
    ];

    fn ep(address: u8, kind: EndpointKind, max_packet: u16, interval: u8) -> Endpoint {
        Endpoint {
            address,
            kind,
            max_packet,
            interval,
            max_burst: 0,
        }
    }

    #[test]
    fn a_device_descriptor_parses() {
        let d = [
            18, 1, 0x10, 1, 0, 0, 0, 8, 0x6D, 0x04, 0x1C, 0xC3, 0x10, 0x01, 1, 2, 0, 1,
        ];
        let dev = DeviceDescriptor::parse(&d).unwrap();
        assert_eq!((dev.vendor, dev.product), (0x046D, 0xC31C));
        assert_eq!(dev.usb_version, 0x0110);
        assert_eq!(dev.max_packet0, 8);
        assert_eq!(dev.configurations, 1);
        assert!(DeviceDescriptor::parse(&d[..17]).is_err());
        let mut wrong = d;
        wrong[1] = 2;
        assert!(DeviceDescriptor::parse(&wrong).is_err());
        let mut short = d;
        short[0] = 8;
        assert!(DeviceDescriptor::parse(&short).is_err());
    }

    #[test]
    fn endpoint_zero_packet_sizes_are_checked_against_the_speed() {
        let prefix = |mps| [18, 1, 0, 2, 0, 0, 0, mps];
        assert_eq!(max_packet0(&prefix(8), Speed::Low), Ok(8));
        assert_eq!(max_packet0(&prefix(64), Speed::Full), Ok(64));
        assert_eq!(max_packet0(&prefix(64), Speed::High), Ok(64));
        assert_eq!(max_packet0(&prefix(9), Speed::Super), Ok(512));
        assert!(max_packet0(&prefix(64), Speed::Low).is_err());
        assert!(max_packet0(&prefix(0), Speed::Full).is_err());
        assert!(max_packet0(&prefix(12), Speed::Full).is_err());
        assert!(max_packet0(&prefix(200), Speed::Super).is_err());
        assert!(max_packet0(&prefix(8)[..7], Speed::Full).is_err());
    }

    #[test]
    fn the_k120_configuration_parses() {
        assert_eq!(configuration_length(&K120[..9]), Ok(59));
        let c = parse_configuration(&K120).unwrap();
        assert_eq!(c.value, 1);
        assert_eq!(
            c.interfaces,
            vec![
                Interface {
                    number: 0,
                    class: 3,
                    subclass: 1,
                    protocol: 1,
                    endpoints: vec![ep(0x81, EndpointKind::Interrupt, 8, 10)],
                },
                Interface {
                    number: 1,
                    class: 3,
                    subclass: 0,
                    protocol: 0,
                    endpoints: vec![ep(0x82, EndpointKind::Interrupt, 4, 0xFF)],
                },
            ]
        );
        let e = c.interfaces[0].endpoints[0];
        assert!(e.is_in());
        assert_eq!(
            (e.number(), e.packet_size(), e.extra_transactions()),
            (1, 8, 0)
        );
    }

    #[test]
    fn alternate_settings_and_repeated_interfaces_are_skipped() {
        let d = [
            9, 2, 50, 0, 1, 1, 0, 0x80, 50, //
            9, 4, 0, 0, 1, 8, 6, 0x50, 0, // interface 0 alt 0: mass storage
            7, 5, 0x81, 2, 0, 2, 0, // bulk IN 512
            9, 4, 0, 1, 1, 8, 6, 0x62, 0, // interface 0 alt 1 (UAS)
            7, 5, 0x83, 2, 0, 2, 0, // its endpoint
            9, 4, 0, 0, 0, 3, 1, 1, 0, // interface 0 again: ignored
        ];
        let c = parse_configuration(&d).unwrap();
        assert_eq!(c.interfaces.len(), 1);
        assert_eq!(c.interfaces[0].protocol, 0x50);
        assert_eq!(
            c.interfaces[0].endpoints,
            vec![ep(0x81, EndpointKind::Bulk, 512, 0)]
        );
    }

    #[test]
    fn superspeed_companions_give_the_burst() {
        let d = [
            9, 2, 44, 0, 1, 1, 0, 0x80, 50, //
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 4, 0, // bulk IN 1024
            6, 0x30, 15, 0, 0, 0, // companion: burst of 16
            7, 5, 0x02, 2, 0, 4, 0, // bulk OUT 1024
            6, 0x30, 3, 0, 0, 0, //
        ];
        let c = parse_configuration(&d).unwrap();
        let eps = &c.interfaces[0].endpoints;
        assert_eq!((eps[0].address, eps[0].max_burst), (0x81, 15));
        assert_eq!((eps[1].address, eps[1].max_burst), (0x02, 3));
        assert!(!eps[1].is_in());
    }

    #[test]
    fn malformed_configurations_are_errors() {
        // A zero-length descriptor would loop forever if it were believed.
        let mut zero = K120;
        zero[18] = 0;
        assert!(parse_configuration(&zero).is_err());
        // An endpoint descriptor that runs past wTotalLength.
        let mut past = K120;
        past[52] = 9;
        assert!(parse_configuration(&past).is_err());
        let mut short = K120;
        short[9] = 5;
        assert!(parse_configuration(&short).is_err());
        assert!(configuration_length(&[9, 2, 5, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(configuration_length(&K120[..8]).is_err());
        assert!(parse_configuration(&[9, 1, 9, 0, 0, 1, 0, 0, 0]).is_err());
    }

    #[test]
    fn data_beyond_the_total_length_is_ignored_and_less_is_parsed() {
        let mut long = K120.to_vec();
        long.extend_from_slice(&[0, 0, 0]);
        assert_eq!(parse_configuration(&long).unwrap().interfaces.len(), 2);
        // A device that sends fewer bytes than it announced: what arrived
        // is parsed, as long as no descriptor is cut in half.
        assert_eq!(
            parse_configuration(&K120[..34]).unwrap().interfaces.len(),
            1
        );
        assert!(parse_configuration(&K120[..30]).is_err());
    }

    #[test]
    fn endpoint_zero_and_endpoints_outside_interfaces_are_ignored() {
        let d = [
            9, 2, 32, 0, 1, 1, 0, 0x80, 50, //
            7, 5, 0x81, 3, 8, 0, 10, // before any interface
            9, 4, 0, 0, 1, 3, 1, 1, 0, //
            7, 5, 0x80, 3, 8, 0, 10, // endpoint 0: invalid here
        ];
        let c = parse_configuration(&d).unwrap();
        assert!(c.interfaces[0].endpoints.is_empty());
    }
}
````

- [ ] **Step 5: Write the failing tests for `crates/usb/src/error.rs`**

Create `crates/usb/src/error.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn messages_read_well_in_a_status_line() {
        assert_eq!(UsbError::Timeout.to_string(), "timed out");
        assert_eq!(
            UsbError::Command(5).to_string(),
            "command failed (completion code 5)"
        );
        assert_eq!(
            UsbError::Unsupported("32-bit addressing").to_string(),
            "unsupported: 32-bit addressing"
        );
        assert_eq!(
            UsbError::BadDescriptor("zero length").to_string(),
            "bad descriptor: zero length"
        );
    }
}
````

- [ ] **Step 6: Write the failing tests for `crates/usb/src/hal.rs`**

Create `crates/usb/src/hal.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::alloc::{Layout, alloc_zeroed, dealloc};

    fn buf(size: usize) -> (DmaBuf, Layout) {
        let layout = Layout::from_size_align(size, 64).unwrap();
        let p = NonNull::new(unsafe { alloc_zeroed(layout) }).unwrap();
        (unsafe { DmaBuf::new(p, 0x1000, size) }, layout)
    }

    #[test]
    fn reads_and_writes_little_endian_words_and_bytes() {
        let (b, layout) = buf(64);
        b.write32(4, 0x1122_3344);
        b.write64(8, 0x5566_7788_99AA_BBCC);
        let mut out = [0u8; 4];
        b.read_bytes(4, &mut out);
        assert_eq!(out, [0x44, 0x33, 0x22, 0x11]);
        assert_eq!(b.read32(8), 0x99AA_BBCC);
        assert_eq!(b.read64(8), 0x5566_7788_99AA_BBCC);
        b.write_bytes(62, &[1, 2]);
        assert_eq!(b.read32(60), 0x0201_0000);
        b.zero(60, 4);
        assert_eq!(b.read32(60), 0);
        assert_eq!(b.phys_at(16), 0x1010);
        unsafe { dealloc(b.virt().as_ptr(), layout) };
    }

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn accesses_past_the_end_panic() {
        let (b, _layout) = buf(64);
        b.read32(62);
    }

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn byte_copies_past_the_end_panic() {
        let (b, _layout) = buf(64);
        b.write_bytes(60, &[0; 5]);
    }
}
````

- [ ] **Step 7: Create `crates/usb/src/lib.rs`**

Create `crates/usb/src/lib.rs`:

````rust
//! The Relay USB stack (spec §6): an xHCI host controller driver and the
//! HID boot-keyboard class driver.
//!
//! The crate reaches hardware only through [`Hal`], which the kernel
//! implements over its page tables, frame allocator and timer, and the tests
//! over a fake register file and heap memory. So every sequence, including
//! the ones QEMU never exercises (BIOS handoff, scratchpad buffers, 64-byte
//! contexts, SuperSpeed ports), is tested on the host.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod bus;
pub mod descriptor;
mod error;
mod hal;

pub use bus::{Bus, Setup, Speed};
pub use error::UsbError;
pub use hal::{DmaBuf, Hal};
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `EndpointKind` in this scope ``; `` cannot find type `Endpoint` in this scope ``.

- [ ] **Step 9: Implement `crates/usb/src/bus.rs`**

Insert this at the top of `crates/usb/src/bus.rs`, above `#[cfg(test)]`:

````rust
//! What class drivers see of a host controller: control requests, IN
//! transfers that complete later, and halt recovery. The xHCI driver
//! implements [`Bus`]; the class drivers' tests use a fake.

use crate::UsbError;
use core::fmt;
use core::time::Duration;

/// A device's connection speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
    SuperPlus,
}

impl Speed {
    /// Endpoint 0's packet size until the device descriptor says otherwise
    /// (USB 2.0 §5.5.3, USB 3.2 §9.6.1).
    pub fn default_max_packet0(self) -> u16 {
        match self {
            Speed::Low | Speed::Full => 8,
            Speed::High => 64,
            Speed::Super | Speed::SuperPlus => 512,
        }
    }

    /// SuperSpeed and faster, which use USB 3 ports and descriptors.
    pub fn is_superspeed(self) -> bool {
        matches!(self, Speed::Super | Speed::SuperPlus)
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Speed::Low => "low-speed",
            Speed::Full => "full-speed",
            Speed::High => "high-speed",
            Speed::Super => "SuperSpeed",
            Speed::SuperPlus => "SuperSpeedPlus",
        })
    }
}

/// Standard request codes (USB 2.0 table 9-4).
pub const CLEAR_FEATURE: u8 = 1;
pub const GET_DESCRIPTOR: u8 = 6;
pub const SET_CONFIGURATION: u8 = 9;

/// `bmRequestType` bits.
pub const DIR_IN: u8 = 0x80;
pub const TYPE_CLASS: u8 = 0x20;
pub const RECIPIENT_INTERFACE: u8 = 0x01;
pub const RECIPIENT_ENDPOINT: u8 = 0x02;

/// The feature selector of `CLEAR_FEATURE(ENDPOINT_HALT)`.
pub const ENDPOINT_HALT: u16 = 0;

/// The 8-byte setup packet of a control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setup {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl Setup {
    /// Whether the data stage goes from the device to the host.
    pub fn is_in(&self) -> bool {
        self.request_type & DIR_IN != 0
    }

    /// The packet as it goes on the wire (little-endian fields).
    pub fn to_bytes(&self) -> [u8; 8] {
        let [v0, v1] = self.value.to_le_bytes();
        let [i0, i1] = self.index.to_le_bytes();
        let [l0, l1] = self.length.to_le_bytes();
        [self.request_type, self.request, v0, v1, i0, i1, l0, l1]
    }

    /// `GET_DESCRIPTOR` of descriptor type `kind`, number `index`.
    pub fn get_descriptor(kind: u8, index: u8, length: u16) -> Setup {
        Setup {
            request_type: DIR_IN,
            request: GET_DESCRIPTOR,
            value: (kind as u16) << 8 | index as u16,
            index: 0,
            length,
        }
    }

    pub fn set_configuration(value: u8) -> Setup {
        Setup {
            request_type: 0,
            request: SET_CONFIGURATION,
            value: value as u16,
            index: 0,
            length: 0,
        }
    }

    /// `CLEAR_FEATURE(ENDPOINT_HALT)` for endpoint address `endpoint`.
    pub fn clear_halt(endpoint: u8) -> Setup {
        Setup {
            request_type: RECIPIENT_ENDPOINT,
            request: CLEAR_FEATURE,
            value: ENDPOINT_HALT,
            index: endpoint as u16,
            length: 0,
        }
    }
}

/// A host controller as class drivers use it. Devices are named by their
/// slot number on that controller; endpoints by their address (bit 7 set
/// for IN).
pub trait Bus {
    /// A control transfer on endpoint 0: `setup`, then `data` (received for
    /// an IN request, sent for an OUT one; `setup.length` bytes at most),
    /// then the status stage. Waits for it (1 s at most) and returns the
    /// number of data bytes moved. A STALL is `UsbError::Stall`, and the
    /// endpoint is usable again afterwards.
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError>;
    /// Starts an IN transfer of up to `len` bytes on `endpoint` and returns
    /// at once. At most one is outstanding per endpoint.
    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError>;
    /// The outcome of the IN transfer on `endpoint` once it has finished:
    /// the bytes received are copied into `buf`. `None` while it is still
    /// running (or none was queued). Never waits.
    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>>;
    /// Makes a halted `endpoint` usable again: resets it on the controller
    /// and sends `CLEAR_FEATURE(ENDPOINT_HALT)` to the device.
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError>;
    /// Monotonic time since boot.
    fn now(&self) -> Duration;
    /// Adds one line to the kernel log.
    fn log(&self, args: fmt::Arguments);
}

````

- [ ] **Step 10: Implement `crates/usb/src/descriptor.rs`**

Insert this at the top of `crates/usb/src/descriptor.rs`, above `#[cfg(test)]`:

````rust
//! Standard descriptors (USB 2.0 chapter 9, USB 3.2 §9.6): the device
//! descriptor and the configuration descriptor with its interfaces and
//! endpoints. Devices are not trusted: a malformed descriptor is an error,
//! never a panic or a loop.

use crate::{Speed, UsbError};
use alloc::vec::Vec;

pub const DEVICE: u8 = 1;
pub const CONFIGURATION: u8 = 2;
pub const INTERFACE: u8 = 4;
pub const ENDPOINT: u8 = 5;
pub const SS_ENDPOINT_COMPANION: u8 = 0x30;

/// Length of the device descriptor.
pub const DEVICE_LEN: usize = 18;
/// Length of the configuration descriptor's own header.
pub const CONFIGURATION_LEN: usize = 9;

fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    /// `bcdUSB`, for example 0x0200.
    pub usb_version: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    /// `bMaxPacketSize0` as sent: bytes, or an exponent for SuperSpeed.
    pub max_packet0: u8,
    pub vendor: u16,
    pub product: u16,
    pub device_version: u16,
    pub configurations: u8,
}

impl DeviceDescriptor {
    pub fn parse(b: &[u8]) -> Result<DeviceDescriptor, UsbError> {
        if b.len() < DEVICE_LEN || (b[0] as usize) < DEVICE_LEN {
            return Err(UsbError::BadDescriptor("device descriptor too short"));
        }
        if b[1] != DEVICE {
            return Err(UsbError::BadDescriptor("not a device descriptor"));
        }
        Ok(DeviceDescriptor {
            usb_version: le16(b, 2),
            class: b[4],
            subclass: b[5],
            protocol: b[6],
            max_packet0: b[7],
            vendor: le16(b, 8),
            product: le16(b, 10),
            device_version: le16(b, 12),
            configurations: b[17],
        })
    }
}

/// Endpoint 0's packet size from the first 8 bytes of the device
/// descriptor (all a full-speed device is asked for before its packet size
/// is known). SuperSpeed devices send an exponent (9 means 512).
pub fn max_packet0(prefix: &[u8], speed: Speed) -> Result<u16, UsbError> {
    if prefix.len() < 8 || prefix[1] != DEVICE {
        return Err(UsbError::BadDescriptor("not a device descriptor"));
    }
    let raw = prefix[7];
    let size = if speed.is_superspeed() {
        if raw > 15 {
            return Err(UsbError::BadDescriptor("endpoint 0 packet size"));
        }
        1u16 << raw
    } else {
        raw as u16
    };
    match (speed, size) {
        (Speed::Low, 8) | (Speed::Full, 8 | 16 | 32 | 64) | (Speed::High, 64) => Ok(size),
        (Speed::Super | Speed::SuperPlus, 512) => Ok(size),
        _ => Err(UsbError::BadDescriptor("endpoint 0 packet size")),
    }
}

/// `wTotalLength` from the configuration descriptor's 9-byte header.
pub fn configuration_length(header: &[u8]) -> Result<u16, UsbError> {
    if header.len() < CONFIGURATION_LEN || header[1] != CONFIGURATION {
        return Err(UsbError::BadDescriptor("not a configuration descriptor"));
    }
    let total = le16(header, 2);
    if (total as usize) < CONFIGURATION_LEN {
        return Err(UsbError::BadDescriptor("configuration too short"));
    }
    Ok(total)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointKind {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// `bEndpointAddress`: the number in bits 0-3, bit 7 set for IN.
    pub address: u8,
    pub kind: EndpointKind,
    /// `wMaxPacketSize` as sent: the size in bits 0-10, extra transactions
    /// per microframe in bits 11-12 (high-speed periodic endpoints).
    pub max_packet: u16,
    /// `bInterval` as sent (its unit depends on speed and kind).
    pub interval: u8,
    /// `bMaxBurst` from a SuperSpeed endpoint companion, else 0.
    pub max_burst: u8,
}

impl Endpoint {
    pub fn is_in(&self) -> bool {
        self.address & 0x80 != 0
    }

    pub fn number(&self) -> u8 {
        self.address & 0x0F
    }

    /// Bytes per packet.
    pub fn packet_size(&self) -> u16 {
        self.max_packet & 0x7FF
    }

    /// Additional transactions per microframe (high-speed periodic).
    pub fn extra_transactions(&self) -> u8 {
        (self.max_packet >> 11) as u8 & 3
    }
}

/// An interface in its default setting (alternate setting 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub number: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// `bConfigurationValue`, for `SET_CONFIGURATION`.
    pub value: u8,
    pub interfaces: Vec<Interface>,
}

/// Parses a whole configuration descriptor (`wTotalLength` bytes).
/// Alternate settings other than 0 are skipped with their endpoints, as are
/// class-specific descriptors (HID's among them) and endpoint 0 entries.
pub fn parse_configuration(b: &[u8]) -> Result<Configuration, UsbError> {
    let total = configuration_length(b)? as usize;
    if (b[0] as usize) < CONFIGURATION_LEN {
        return Err(UsbError::BadDescriptor("configuration too short"));
    }
    let b = &b[..total.min(b.len())];
    let mut config = Configuration {
        value: b[5],
        interfaces: Vec::new(),
    };
    // Where endpoints go: the interface being parsed, or none while an
    // alternate setting or a repeated interface number is skipped.
    let mut current: Option<usize> = None;
    let mut pos = b[0] as usize;
    while pos < b.len() {
        let rest = &b[pos..];
        if rest.len() < 2 || rest[0] < 2 {
            return Err(UsbError::BadDescriptor("zero-length descriptor"));
        }
        let len = rest[0] as usize;
        if len > rest.len() {
            return Err(UsbError::BadDescriptor("descriptor runs past the end"));
        }
        let d = &rest[..len];
        match d[1] {
            INTERFACE => {
                if len < 9 {
                    return Err(UsbError::BadDescriptor("interface descriptor too short"));
                }
                let seen = config.interfaces.iter().any(|i| i.number == d[2]);
                current = if d[3] == 0 && !seen {
                    config.interfaces.push(Interface {
                        number: d[2],
                        class: d[5],
                        subclass: d[6],
                        protocol: d[7],
                        endpoints: Vec::new(),
                    });
                    Some(config.interfaces.len() - 1)
                } else {
                    None
                };
            }
            ENDPOINT => {
                if len < 7 {
                    return Err(UsbError::BadDescriptor("endpoint descriptor too short"));
                }
                let kind = match d[3] & 3 {
                    0 => EndpointKind::Control,
                    1 => EndpointKind::Isochronous,
                    2 => EndpointKind::Bulk,
                    _ => EndpointKind::Interrupt,
                };
                if let Some(i) = current
                    && d[2] & 0x0F != 0
                {
                    config.interfaces[i].endpoints.push(Endpoint {
                        address: d[2] & 0x8F,
                        kind,
                        max_packet: le16(d, 4),
                        interval: d[6],
                        max_burst: 0,
                    });
                }
            }
            SS_ENDPOINT_COMPANION if len >= 6 => {
                if let Some(ep) = current.and_then(|i| config.interfaces[i].endpoints.last_mut()) {
                    ep.max_burst = d[2];
                }
            }
            _ => {}
        }
        pos += len;
    }
    Ok(config)
}

````

- [ ] **Step 11: Implement `crates/usb/src/error.rs`**

Insert this at the top of `crates/usb/src/error.rs`, above `#[cfg(test)]`:

````rust
//! USB errors. Device errors are values, not panics (spec §10).

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// A wait ran out of time: a register, a command or a transfer.
    Timeout,
    /// The device answered with a STALL handshake.
    Stall,
    /// A transfer ended with this xHCI completion code.
    Transfer(u8),
    /// A command ended with this xHCI completion code.
    Command(u8),
    /// No DMA memory left, or the registers could not be mapped.
    NoMemory,
    /// The controller's registers read as all ones: it is powered down or
    /// not there.
    NotResponding,
    /// The controller or device needs something this driver does not do.
    Unsupported(&'static str),
    /// A descriptor the device sent is malformed.
    BadDescriptor(&'static str),
    /// The device is no longer connected.
    Disconnected,
    /// The controller stopped working (a command timed out or it reported a
    /// host system error); nothing more is sent to it.
    ControllerDead,
}

impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UsbError::Timeout => write!(f, "timed out"),
            UsbError::Stall => write!(f, "stalled"),
            UsbError::Transfer(code) => write!(f, "transfer failed (completion code {code})"),
            UsbError::Command(code) => write!(f, "command failed (completion code {code})"),
            UsbError::NoMemory => write!(f, "out of memory"),
            UsbError::NotResponding => write!(f, "controller not responding"),
            UsbError::Unsupported(what) => write!(f, "unsupported: {what}"),
            UsbError::BadDescriptor(what) => write!(f, "bad descriptor: {what}"),
            UsbError::Disconnected => write!(f, "device disconnected"),
            UsbError::ControllerDead => write!(f, "controller stopped working"),
        }
    }
}

````

- [ ] **Step 12: Implement `crates/usb/src/hal.rs`**

Insert this at the top of `crates/usb/src/hal.rs`, above `#[cfg(test)]`:

````rust
//! The hardware abstraction (spec §6.1): the only way this crate reaches
//! hardware.

use core::fmt;
use core::ptr::NonNull;
use core::time::Duration;

/// Memory a device can read and write: a virtual address for the CPU, a
/// physical address for the device, zeroed when allocated.
///
/// Every access goes through volatile reads and writes, because the device
/// changes the memory behind the compiler's back. A `DmaBuf` is not `Clone`:
/// [`Hal::free_dma`] consumes it, so no access can outlive the memory.
#[derive(Debug)]
pub struct DmaBuf {
    virt: NonNull<u8>,
    phys: u64,
    size: usize,
}

impl DmaBuf {
    /// # Safety
    /// `virt` must point to `size` bytes of memory that stay valid, and that
    /// nothing else writes to except the device at `phys`, until the buffer
    /// is given back to [`Hal::free_dma`].
    pub unsafe fn new(virt: NonNull<u8>, phys: u64, size: usize) -> DmaBuf {
        DmaBuf { virt, phys, size }
    }

    /// The physical address of byte 0, for the device.
    pub fn phys(&self) -> u64 {
        self.phys
    }

    /// The physical address of byte `offset`.
    pub fn phys_at(&self, offset: usize) -> u64 {
        assert!(
            offset <= self.size,
            "DMA offset {offset} beyond {}",
            self.size
        );
        self.phys + offset as u64
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// The CPU's address of byte 0 (for the `Hal` that frees it).
    pub fn virt(&self) -> NonNull<u8> {
        self.virt
    }

    fn check(&self, offset: usize, len: usize) {
        assert!(
            offset.checked_add(len).is_some_and(|end| end <= self.size),
            "DMA access {offset}+{len} beyond {}",
            self.size
        );
    }

    pub fn read32(&self, offset: usize) -> u32 {
        self.check(offset, 4);
        assert!(offset.is_multiple_of(4), "unaligned DMA read at {offset}");
        // SAFETY: inside the buffer (checked), aligned, valid until freed.
        unsafe { self.virt.add(offset).cast::<u32>().read_volatile() }
    }

    pub fn write32(&self, offset: usize, value: u32) {
        self.check(offset, 4);
        assert!(offset.is_multiple_of(4), "unaligned DMA write at {offset}");
        // SAFETY: as in `read32`.
        unsafe { self.virt.add(offset).cast::<u32>().write_volatile(value) }
    }

    pub fn read64(&self, offset: usize) -> u64 {
        self.read32(offset) as u64 | (self.read32(offset + 4) as u64) << 32
    }

    pub fn write64(&self, offset: usize, value: u64) {
        self.write32(offset, value as u32);
        self.write32(offset + 4, (value >> 32) as u32);
    }

    /// Copies `out.len()` bytes starting at `offset` out of the buffer.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) {
        self.check(offset, out.len());
        for (i, b) in out.iter_mut().enumerate() {
            // SAFETY: inside the buffer (checked).
            *b = unsafe { self.virt.add(offset + i).read_volatile() };
        }
    }

    /// Copies `data` into the buffer starting at `offset`.
    pub fn write_bytes(&self, offset: usize, data: &[u8]) {
        self.check(offset, data.len());
        for (i, &b) in data.iter().enumerate() {
            // SAFETY: inside the buffer (checked).
            unsafe { self.virt.add(offset + i).write_volatile(b) };
        }
    }

    /// Sets `len` bytes from `offset` to zero.
    pub fn zero(&self, offset: usize, len: usize) {
        self.check(offset, len);
        for i in 0..len {
            // SAFETY: inside the buffer (checked).
            unsafe { self.virt.add(offset + i).write_volatile(0) };
        }
    }
}

/// What the USB stack needs from the machine. The kernel implements it over
/// `mm::map_mmio`, the frame allocator, the timer and the kernel log; tests
/// implement it over a fake controller.
pub trait Hal {
    /// Maps `len` bytes of device registers at physical `phys`, uncached, and
    /// returns their virtual address. `None` if they cannot be mapped.
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize>;
    /// Reads the 32-bit device register at virtual address `addr`.
    ///
    /// # Safety
    /// `addr` is 4-byte aligned and inside a range `map_mmio` returned.
    unsafe fn read32(&self, addr: usize) -> u32;
    /// Writes the 32-bit device register at virtual address `addr`.
    ///
    /// # Safety
    /// As for [`Hal::read32`].
    unsafe fn write32(&self, addr: usize, value: u32);
    /// `size` bytes of zeroed, physically contiguous memory starting on a
    /// 4 KiB page boundary, or on `align` bytes (a power of two) if that is
    /// larger. So a buffer of at most 4 KiB never crosses a page. `None`
    /// when memory has run out.
    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf>;
    /// Gives memory from `alloc_dma` back.
    fn free_dma(&self, buf: DmaBuf);
    /// Monotonic time since boot.
    fn now(&self) -> Duration;
    /// Waits at least `d`, busy (there are no device interrupts).
    fn sleep(&self, d: Duration);
    /// Adds one line to the kernel log (`dmesg`).
    fn log(&self, args: fmt::Arguments);
}

````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 14 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add Cargo.lock Cargo.toml crates
git commit -m "usb: the crate, its Hal, errors, requests and descriptors"
````


### Task 4: TRBs and rings

Spec §6.2's data structures begin with the rings. A TRB is 16 bytes; `trb.rs` has a constructor for every TRB this driver sends and accessors for the events it reads. A `ProducerRing` (the command ring and one transfer ring per endpoint) is one 4 KiB segment of 256 TRBs ending in a Link TRB with Toggle Cycle: `push` writes the first three dwords and the dword with the cycle bit last, gives the link the current cycle bit (and the chain bit when a TD spans it) and toggles at the wrap. The `EventRing` has one segment and a one-entry ERST, and consumes an event only while its cycle bit matches. The fake `Hal` starts here: virtual time (a 1 s timeout costs no real time), DMA memory from the host heap whose physical address is its host address, checked on every access by the fake controller, and captured log lines.

**Files:**
- Modify: `crates/usb/src/lib.rs`
- Create: `crates/usb/src/testing/hal.rs`
- Create: `crates/usb/src/testing/mod.rs`
- Create: `crates/usb/src/xhci/mod.rs`
- Create: `crates/usb/src/xhci/ring.rs`
- Create: `crates/usb/src/xhci/trb.rs`

**Interfaces:**
- Consumes: `Hal`, `DmaBuf`, `UsbError` (Task 3).
- Produces: `usb::xhci::trb::{Trb([u32; 4]), TRB type and completion code constants, completion_name}` with the constructors (`normal`, `setup_stage`, `data_stage`, `status_stage`, `link`, `no_op_command`, `enable_slot`, `disable_slot`, `address_device`, `configure_endpoint`, `evaluate_context`, `reset_endpoint`, `stop_endpoint`, `set_tr_dequeue`) and event accessors; `xhci::ring::{TRBS = 256, ProducerRing { new, phys, push -> u64, replace, enqueue_pointer, free }, EventRing { new, erst_phys, phys, next -> Option<Trb>, dequeue_pointer, free }}`; test support `testing::{FakeHal { new, clock, outstanding_dma, log_text, … }}`; the crate-internal `xlog!(hal, name, …)` macro (every driver line starts `xhci <name>: `).

- [ ] **Step 1: Declare the new module in `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust
mod hal;

````

with:

````rust
mod hal;
#[cfg(test)]
mod testing;
pub mod xhci;

````

- [ ] **Step 2: Create `crates/usb/src/testing/hal.rs`**

Test support shared by every xHCI test (compiled only for tests).

Create `crates/usb/src/testing/hal.rs`:

````rust
//! A fake [`Hal`]: virtual time, DMA memory from the host heap (its
//! physical address is its host address), and captured log lines.

use crate::{DmaBuf, Hal};
use core::fmt;
use core::ptr::NonNull;
use core::time::Duration;
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// A shared handle: the driver owns one clone, the test another.
#[derive(Clone)]
pub struct FakeHal(Rc<Inner>);

struct Inner {
    clock: Cell<Duration>,
    dma: RefCell<Dma>,
    log: RefCell<Vec<String>>,
}

/// Every DMA buffer handed out and not yet freed, by physical address.
#[derive(Default)]
pub struct Dma {
    buffers: BTreeMap<u64, (NonNull<u8>, Layout)>,
    /// Allocations that still succeed; `None` is no limit.
    allocs_left: Option<usize>,
}

impl Dma {
    /// The host pointer to `len` bytes at `phys`, which must lie inside one
    /// allocated buffer: the controller never reaches other memory.
    fn find(&self, phys: u64, len: usize) -> *mut u8 {
        let hit = self.buffers.range(..=phys).next_back();
        match hit {
            Some((&start, &(ptr, layout))) if phys + len as u64 <= start + layout.size() as u64 => {
                // SAFETY: inside the allocation (checked just above).
                unsafe { ptr.as_ptr().add((phys - start) as usize) }
            }
            _ => panic!("fake xhci: access to unallocated DMA memory at {phys:#x}"),
        }
    }

    /// Whether `len` bytes at `phys` lie inside one allocated buffer.
    pub fn contains(&self, phys: u64, len: usize) -> bool {
        self.buffers
            .range(..=phys)
            .next_back()
            .is_some_and(|(&start, &(_, layout))| phys + len as u64 <= start + layout.size() as u64)
    }

    pub fn read32(&self, phys: u64) -> u32 {
        assert!(phys.is_multiple_of(4), "fake xhci: unaligned DMA read");
        // SAFETY: `find` checked the range; the address is aligned.
        unsafe { self.find(phys, 4).cast::<u32>().read_volatile() }
    }

    pub fn write32(&self, phys: u64, value: u32) {
        assert!(phys.is_multiple_of(4), "fake xhci: unaligned DMA write");
        // SAFETY: as in `read32`.
        unsafe { self.find(phys, 4).cast::<u32>().write_volatile(value) }
    }

    pub fn read64(&self, phys: u64) -> u64 {
        self.read32(phys) as u64 | (self.read32(phys + 4) as u64) << 32
    }

    pub fn write64(&self, phys: u64, value: u64) {
        self.write32(phys, value as u32);
        self.write32(phys + 4, (value >> 32) as u32);
    }

    pub fn read_bytes(&self, phys: u64, len: usize) -> Vec<u8> {
        let p = self.find(phys, len);
        // SAFETY: `find` checked the range.
        (0..len)
            .map(|i| unsafe { p.add(i).read_volatile() })
            .collect()
    }

    pub fn write_bytes(&self, phys: u64, data: &[u8]) {
        let p = self.find(phys, data.len());
        for (i, &b) in data.iter().enumerate() {
            // SAFETY: `find` checked the range.
            unsafe { p.add(i).write_volatile(b) };
        }
    }
}

impl Drop for Dma {
    fn drop(&mut self) {
        for (ptr, layout) in self.buffers.values() {
            // SAFETY: allocated with this layout and not freed yet.
            unsafe { dealloc(ptr.as_ptr(), *layout) };
        }
    }
}

impl Default for FakeHal {
    fn default() -> FakeHal {
        FakeHal::new()
    }
}

impl FakeHal {
    pub fn new() -> FakeHal {
        FakeHal(Rc::new(Inner {
            clock: Cell::new(Duration::ZERO),
            dma: RefCell::new(Dma::default()),
            log: RefCell::new(Vec::new()),
        }))
    }

    /// The virtual time, without advancing it (`now` advances it).
    pub fn clock(&self) -> Duration {
        self.0.clock.get()
    }

    /// DMA buffers allocated and not freed.
    pub fn outstanding_dma(&self) -> usize {
        self.0.dma.borrow().buffers.len()
    }

    /// Lets `n` more allocations succeed, then fails every one.
    pub fn fail_alloc_after(&self, n: usize) {
        self.0.dma.borrow_mut().allocs_left = Some(n);
    }

    /// Every log line so far, one per line.
    pub fn log_text(&self) -> String {
        self.0.log.borrow().join("\n")
    }

    fn advance(&self, d: Duration) {
        self.0.clock.set(self.0.clock.get() + d);
    }
}

impl Hal for FakeHal {
    fn map_mmio(&self, _phys: u64, _len: usize) -> Option<usize> {
        None
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        panic!("fake hal: no controller for the read at {addr:#x}")
    }

    unsafe fn write32(&self, addr: usize, _value: u32) {
        panic!("fake hal: no controller for the write at {addr:#x}")
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        assert!(
            size > 0 && align.is_power_of_two(),
            "fake hal: bad DMA request"
        );
        let mut dma = self.0.dma.borrow_mut();
        if let Some(left) = dma.allocs_left.as_mut() {
            if *left == 0 {
                return None;
            }
            *left -= 1;
        }
        let layout = Layout::from_size_align(size, align.max(4096)).expect("DMA layout");
        // SAFETY: the layout has a non-zero size.
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).expect("host memory");
        let phys = ptr.as_ptr() as u64;
        dma.buffers.insert(phys, (ptr, layout));
        // SAFETY: `size` zeroed bytes, freed only through `free_dma`.
        Some(unsafe { DmaBuf::new(ptr, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        let mut dma = self.0.dma.borrow_mut();
        let Some((ptr, layout)) = dma.buffers.remove(&buf.phys()) else {
            panic!(
                "fake hal: freeing DMA memory at {:#x} that is not allocated",
                buf.phys()
            );
        };
        assert_eq!(
            buf.size(),
            layout.size(),
            "fake hal: freed with another size"
        );
        // SAFETY: allocated with this layout; removed from the map, so
        // nothing reaches it any more.
        unsafe { dealloc(ptr.as_ptr(), layout) };
    }

    fn now(&self) -> Duration {
        // Time moves on every look, so a wait that forgets to sleep ends.
        self.advance(Duration::from_micros(1));
        self.clock()
    }

    fn sleep(&self, d: Duration) {
        self.advance(d);
    }

    fn log(&self, args: fmt::Arguments) {
        self.0.log.borrow_mut().push(args.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_moves_only_when_asked() {
        let hal = FakeHal::new();
        assert_eq!(hal.clock(), Duration::ZERO);
        assert_eq!(hal.now(), Duration::from_micros(1));
        hal.sleep(Duration::from_secs(1));
        assert_eq!(hal.clock(), Duration::from_micros(1_000_001));
    }

    #[test]
    fn dma_is_zeroed_aligned_and_identity_mapped() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        let b = hal.alloc_dma(8192, 8192).unwrap();
        assert_eq!(a.phys() % 4096, 0);
        assert_eq!(b.phys() % 8192, 0);
        assert_eq!(a.phys(), a.virt().as_ptr() as u64);
        assert_eq!(a.read64(56), 0);
        a.write32(8, 0xDEAD_BEEF);
        assert_eq!(hal.0.dma.borrow().read32(a.phys() + 8), 0xDEAD_BEEF);
        assert_eq!(hal.outstanding_dma(), 2);
        hal.free_dma(a);
        hal.free_dma(b);
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    #[should_panic(expected = "access to unallocated DMA memory")]
    fn the_controller_cannot_reach_past_a_buffer() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        hal.0.dma.borrow().read32(a.phys() + 64);
    }

    #[test]
    #[should_panic(expected = "not allocated")]
    fn a_double_free_panics() {
        let hal = FakeHal::new();
        let a = hal.alloc_dma(64, 64).unwrap();
        // SAFETY: a second handle to the same memory, to free it twice.
        let twin = unsafe { DmaBuf::new(a.virt(), a.phys(), a.size()) };
        hal.free_dma(a);
        hal.free_dma(twin);
    }

    #[test]
    fn allocations_can_be_made_to_fail() {
        let hal = FakeHal::new();
        hal.fail_alloc_after(1);
        let a = hal.alloc_dma(64, 64).unwrap();
        assert!(hal.alloc_dma(64, 64).is_none());
        hal.free_dma(a);
    }

    #[test]
    fn log_lines_are_captured() {
        let hal = FakeHal::new();
        hal.log(format_args!("one {}", 1));
        hal.log(format_args!("two"));
        assert_eq!(hal.log_text(), "one 1\ntwo");
    }
}
````

- [ ] **Step 3: Create `crates/usb/src/testing/mod.rs`**

Create `crates/usb/src/testing/mod.rs`:

````rust
//! Test support: a fake `Hal` with virtual time and checked DMA memory.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod hal;

pub use hal::FakeHal;
````

- [ ] **Step 4: Create `crates/usb/src/xhci/mod.rs`**

Create `crates/usb/src/xhci/mod.rs`:

````rust
//! The xHCI host controller driver (spec §6.2).
// The driver is built bottom-up: parts land before their users.
#![allow(dead_code)]

mod ring;
mod trb;
````

- [ ] **Step 5: Write the failing tests for `crates/usb/src/xhci/ring.rs`**

Create `crates/usb/src/xhci/ring.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::trb::{LINK, TOGGLE_CYCLE};
    use super::*;
    use crate::testing::FakeHal;

    fn trb_at(ring: &ProducerRing, index: usize) -> Trb {
        ring.read(index)
    }

    #[test]
    fn a_new_ring_ends_in_a_link_that_is_not_yet_valid() {
        let hal = FakeHal::new();
        let ring = ProducerRing::new(&hal).unwrap();
        let link = trb_at(&ring, 255);
        assert_eq!(link.trb_type(), LINK);
        assert_eq!(link.pointer(), ring.phys());
        assert_eq!(link.0[3] & TOGGLE_CYCLE, TOGGLE_CYCLE);
        assert!(!link.cycle());
        assert_eq!(ring.enqueue_pointer(), ring.phys() | 1);
        ring.free(&hal);
    }

    #[test]
    fn a_lap_fills_255_trbs_then_validates_the_link_and_toggles() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for i in 0..255 {
            let addr = ring.push(Trb::no_op_command());
            assert_eq!(addr, ring.phys() + 16 * i as u64);
        }
        for i in 0..255 {
            assert!(trb_at(&ring, i).cycle(), "TRB {i} of the first lap");
        }
        assert!(trb_at(&ring, 255).cycle(), "the link is valid for lap 1");
        // Lap 2 starts at index 0 with cycle 0.
        assert_eq!(ring.enqueue_pointer(), ring.phys());
        assert_eq!(ring.push(Trb::no_op_command()), ring.phys());
        let first = trb_at(&ring, 0);
        assert!(!first.cycle());
        assert_eq!(first.trb_type(), Trb::no_op_command().trb_type());
        assert!(trb_at(&ring, 1).cycle(), "still lap 1's TRB");
        ring.free(&hal);
    }

    #[test]
    fn after_two_wraps_the_cycle_is_one_again() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        assert!(trb_at(&ring, 255).cycle());
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        assert!(!trb_at(&ring, 255).cycle(), "the link for lap 2");
        assert!(!trb_at(&ring, 254).cycle());
        assert_eq!(ring.enqueue_pointer() & 1, 1);
        ring.push(Trb::no_op_command());
        assert!(trb_at(&ring, 0).cycle());
        ring.free(&hal);
    }

    #[test]
    fn the_link_carries_the_chain_bit_of_the_trb_before_it() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..254 {
            ring.push(Trb::normal(0x1000, 8));
        }
        let mut chained = Trb::normal(0x1000, 8);
        chained.0[3] |= CHAIN;
        ring.push(chained);
        assert!(trb_at(&ring, 255).chain());
        for _ in 0..255 {
            ring.push(Trb::normal(0x1000, 8));
        }
        assert!(!trb_at(&ring, 255).chain(), "an unchained TD clears it");
        ring.free(&hal);
    }

    #[test]
    fn the_enqueue_pointer_carries_the_cycle_state() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        ring.push(Trb::no_op_command());
        assert_eq!(ring.enqueue_pointer(), (ring.phys() + 16) | 1);
        for _ in 1..255 {
            ring.push(Trb::no_op_command());
        }
        assert_eq!(ring.enqueue_pointer(), ring.phys());
        ring.free(&hal);
    }

    #[test]
    fn a_replaced_trb_keeps_its_cycle_bit() {
        let hal = FakeHal::new();
        let mut ring = ProducerRing::new(&hal).unwrap();
        for _ in 0..255 {
            ring.push(Trb::no_op_command());
        }
        let old = ring.push(Trb::enable_slot());
        ring.replace(old, Trb::no_op_command());
        assert_eq!(trb_at(&ring, 0), Trb::no_op_command().with_cycle(false));
        ring.free(&hal);
    }

    fn produce(ring: &EventRing, index: usize, cycle: bool, tag: u32) {
        let o = index * TRB_SIZE;
        ring.segment.write32(o, tag);
        ring.segment.write32(o + 12, 33 << 10 | cycle as u32);
    }

    #[test]
    fn the_event_ring_describes_its_segment_in_the_erst() {
        let hal = FakeHal::new();
        let ring = EventRing::new(&hal).unwrap();
        assert_eq!(ring.erst.read64(0), ring.phys());
        assert_eq!(ring.erst.read32(8), 256);
        assert_eq!(ring.dequeue_pointer(), ring.phys());
        ring.free(&hal);
    }

    #[test]
    fn events_are_consumed_only_while_their_cycle_bit_matches() {
        let hal = FakeHal::new();
        let mut ring = EventRing::new(&hal).unwrap();
        assert_eq!(ring.next(), None, "a zeroed ring is empty");
        produce(&ring, 0, true, 7);
        produce(&ring, 1, true, 8);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(7));
        assert_eq!(ring.dequeue_pointer(), ring.phys() + 16);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(8));
        assert_eq!(ring.next(), None);
        assert_eq!(ring.dequeue_pointer(), ring.phys() + 32);
        ring.free(&hal);
    }

    #[test]
    fn the_event_ring_wraps_at_256_and_toggles_its_cycle() {
        let hal = FakeHal::new();
        let mut ring = EventRing::new(&hal).unwrap();
        for i in 0..256 {
            produce(&ring, i, true, i as u32);
        }
        for i in 0..256 {
            assert_eq!(ring.next().map(|t| t.0[0]), Some(i));
        }
        assert_eq!(ring.dequeue_pointer(), ring.phys());
        assert_eq!(ring.next(), None, "lap 1's TRBs are stale in lap 2");
        produce(&ring, 0, false, 1000);
        assert_eq!(ring.next().map(|t| t.0[0]), Some(1000));
        ring.free(&hal);
    }

    #[test]
    fn freeing_gives_all_dma_back() {
        let hal = FakeHal::new();
        let ring = ProducerRing::new(&hal).unwrap();
        let events = EventRing::new(&hal).unwrap();
        assert_eq!(hal.outstanding_dma(), 3);
        ring.free(&hal);
        events.free(&hal);
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn running_out_of_memory_is_an_error_that_leaks_nothing() {
        let hal = FakeHal::new();
        hal.fail_alloc_after(1);
        assert_eq!(EventRing::new(&hal).err(), Some(UsbError::NoMemory));
        assert_eq!(hal.outstanding_dma(), 0);
        assert_eq!(ProducerRing::new(&hal).err(), Some(UsbError::NoMemory));
    }
}
````

- [ ] **Step 6: Write the failing tests for `crates/usb/src/xhci/trb.rs`**

Create `crates/usb/src/xhci/trb.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_trb_interrupts_on_completion_and_short_packets() {
        let t = Trb::normal(0x1_2345_6780, 8);
        // xHCI 6.4.1.1: type 1 in bits 15:10, ISP bit 2, IOC bit 5.
        assert_eq!(t.0, [0x2345_6780, 0x1, 8, 0x0000_0424]);
        assert_eq!(t.trb_type(), NORMAL);
        assert_eq!(t.pointer(), 0x1_2345_6780);
    }

    #[test]
    fn a_setup_stage_carries_the_packet_inline_with_its_transfer_type() {
        let get = Trb::setup_stage(&Setup::get_descriptor(1, 0, 18));
        // bmRequestType 0x80, bRequest 6, wValue 0x0100; wIndex 0, wLength 18.
        assert_eq!(get.0, [0x0100_0680, 0x0012_0000, 8, 0x0003_0840]);
        let set = Trb::setup_stage(&Setup::set_configuration(1));
        assert_eq!(set.0[3], 0x0000_0840, "no data stage: TRT 0");
        let out = Setup {
            request_type: 0x21,
            request: 9,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(Trb::setup_stage(&out).0[3] >> 16, 2, "OUT data: TRT 2");
    }

    #[test]
    fn data_and_status_stages_carry_the_direction() {
        let d = Trb::data_stage(0x8000, 18, true);
        assert_eq!(d.0, [0x8000, 0, 18, 0x0001_0C04]);
        assert_eq!(Trb::data_stage(0x8000, 1, false).0[3], 0x0000_0C04);
        assert_eq!(Trb::status_stage(false).0, [0, 0, 0, 0x0000_1020]);
        assert_eq!(Trb::status_stage(true).0[3], 0x0001_1020);
    }

    #[test]
    fn a_link_toggles_the_cycle() {
        let l = Trb::link(0xABC0_0000_1000);
        assert_eq!(l.0, [0x0000_1000, 0xABC0, 0, 0x0000_1802]);
        assert_eq!(l.trb_type(), LINK);
    }

    #[test]
    fn commands_name_their_slot_and_endpoint() {
        assert_eq!(Trb::no_op_command().0[3], 23 << 10);
        assert_eq!(Trb::enable_slot().0[3], 9 << 10);
        assert_eq!(Trb::disable_slot(3).0[3], 0x0300_2800);
        let a = Trb::address_device(0x7000, 5, false);
        assert_eq!(a.0, [0x7000, 0, 0, 0x0500_2C00]);
        assert_eq!(Trb::address_device(0x7000, 5, true).0[3], 0x0500_2E00);
        assert_eq!(Trb::configure_endpoint(0x7000, 1, false).0[3], 0x0100_3000);
        assert_eq!(Trb::configure_endpoint(0x7000, 1, true).0[3], 0x0100_3200);
        assert_eq!(Trb::evaluate_context(0x7000, 2).0[3], 0x0200_3400);
        // xHCI 6.4.3.6-6.4.3.9: the endpoint ID is in bits 20:16.
        assert_eq!(Trb::reset_endpoint(1, 3).0[3], 0x0103_3800);
        assert_eq!(Trb::stop_endpoint(2, 1).0[3], 0x0201_3C00);
        let s = Trb::set_tr_dequeue(1, 3, 0x9000_1231);
        assert_eq!(s.0, [0x9000_1231 & !0xE, 0, 0, 0x0103_4000]);
    }

    #[test]
    fn the_cycle_bit_is_set_and_cleared_without_touching_the_rest() {
        let t = Trb::status_stage(true).with_cycle(true);
        assert!(t.cycle());
        assert_eq!(t.0[3], 0x0001_1021);
        assert_eq!(t.with_cycle(false), Trb::status_stage(true));
        assert!(!t.chain());
        assert!(Trb([0, 0, 0, CHAIN]).chain());
    }

    #[test]
    fn event_fields_are_decoded() {
        // A transfer event: TRB 0x1_0000_2040, residual 5, Short Packet,
        // slot 2, DCI 3.
        let e = Trb([0x2040, 1, 13 << 24 | 5, 0x0203_8001]);
        assert_eq!(e.trb_type(), TRANSFER_EVENT);
        assert_eq!(e.pointer(), 0x1_0000_2040);
        assert_eq!(e.completion_code(), SHORT_PACKET);
        assert_eq!(e.transfer_length(), 5);
        assert_eq!((e.slot_id(), e.endpoint_id()), (2, 3));
        let p = Trb([7 << 24, 0, 1 << 24, 34 << 10]);
        assert_eq!((p.trb_type(), p.port_id()), (PORT_STATUS_CHANGE, 7));
    }

    #[test]
    fn completion_codes_and_commands_have_names() {
        assert_eq!(completion_name(STALL), "stall");
        assert_eq!(
            completion_name(USB_TRANSACTION_ERROR),
            "USB transaction error"
        );
        assert_eq!(completion_name(200), "other error");
        assert_eq!(command_name(ADDRESS_DEVICE), "Address Device");
    }
}
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `ProducerRing` in this scope ``; `` cannot find type `Trb` in this scope ``.

- [ ] **Step 8: Implement `crates/usb/src/xhci/ring.rs`**

Insert this at the top of `crates/usb/src/xhci/ring.rs`, above `#[cfg(test)]`:

````rust
//! Rings (xHCI 4.9): the producer rings this driver fills (the command
//! ring and one transfer ring per endpoint) and the event ring the
//! controller fills. Each is one 4 KiB segment of 256 TRBs; ownership of a
//! TRB passes with its cycle bit.

use super::trb::{CHAIN, Trb};
use crate::{DmaBuf, Hal, UsbError};
use core::sync::atomic::{Ordering, fence};

/// TRBs per segment: one 4 KiB page.
pub const TRBS: usize = 256;
const TRB_SIZE: usize = 16;
const SEGMENT_SIZE: usize = TRBS * TRB_SIZE;
/// Producer rings end in a Link TRB back to their start.
const LINK_INDEX: usize = TRBS - 1;

/// A ring this driver produces and the controller consumes: 255 usable
/// TRBs and a Link TRB with Toggle Cycle (xHCI 4.9.2).
#[derive(Debug)]
pub struct ProducerRing {
    buf: DmaBuf,
    /// Where the next TRB goes.
    index: usize,
    /// The Producer Cycle State: the cycle bit valid TRBs carry this lap.
    cycle: bool,
}

impl ProducerRing {
    pub fn new<H: Hal>(hal: &H) -> Result<ProducerRing, UsbError> {
        let buf = hal.alloc_dma(SEGMENT_SIZE, 64).ok_or(UsbError::NoMemory)?;
        let ring = ProducerRing {
            buf,
            index: 0,
            cycle: true,
        };
        // Cycle 0: the controller stops before the link until it is valid.
        ring.write(LINK_INDEX, Trb::link(ring.buf.phys()));
        Ok(ring)
    }

    /// The physical address of the segment (for CRCR and contexts).
    pub fn phys(&self) -> u64 {
        self.buf.phys()
    }

    /// Adds `trb` with the current cycle bit and returns its physical
    /// address, which the controller's events name.
    pub fn push(&mut self, trb: Trb) -> u64 {
        let addr = self.buf.phys_at(self.index * TRB_SIZE);
        self.write(self.index, trb.with_cycle(self.cycle));
        self.index += 1;
        if self.index == LINK_INDEX {
            // xHCI 4.11.5.1: a TD that spans the link carries its chain
            // bit through it.
            let mut link = Trb::link(self.buf.phys());
            if trb.chain() {
                link.0[3] |= CHAIN;
            }
            self.write(LINK_INDEX, link.with_cycle(self.cycle));
            self.cycle = !self.cycle;
            self.index = 0;
        }
        addr
    }

    /// Overwrites the TRB at `addr` (returned by `push`) with `trb`, keeping
    /// the cycle bit it has, so the controller treats it as it would have
    /// treated the old one.
    pub fn replace(&self, addr: u64, trb: Trb) {
        let offset = addr
            .checked_sub(self.buf.phys())
            .map(|o| o as usize)
            .filter(|&o| o < LINK_INDEX * TRB_SIZE && o % TRB_SIZE == 0)
            .expect("replace: not a TRB of this ring");
        let old = self.read(offset / TRB_SIZE);
        self.write(offset / TRB_SIZE, trb.with_cycle(old.cycle()));
    }

    /// The next TRB's address with the cycle state in bit 0: what Set TR
    /// Dequeue Pointer and CRCR take.
    pub fn enqueue_pointer(&self) -> u64 {
        self.buf.phys_at(self.index * TRB_SIZE) | self.cycle as u64
    }

    pub fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.buf);
    }

    fn read(&self, index: usize) -> Trb {
        let offset = index * TRB_SIZE;
        Trb(core::array::from_fn(|i| self.buf.read32(offset + 4 * i)))
    }

    fn write(&self, index: usize, trb: Trb) {
        let offset = index * TRB_SIZE;
        for (i, &dword) in trb.0[..3].iter().enumerate() {
            self.buf.write32(offset + 4 * i, dword);
        }
        // The dword with the cycle bit hands the TRB over, so it goes last.
        fence(Ordering::Release);
        self.buf.write32(offset + 12, trb.0[3]);
    }
}

/// The primary interrupter's event ring: one segment and a one-entry Event
/// Ring Segment Table (xHCI 4.9.4, 6.5).
#[derive(Debug)]
pub struct EventRing {
    segment: DmaBuf,
    erst: DmaBuf,
    /// The next TRB to look at.
    index: usize,
    /// The Consumer Cycle State.
    cycle: bool,
}

impl EventRing {
    pub fn new<H: Hal>(hal: &H) -> Result<EventRing, UsbError> {
        let segment = hal.alloc_dma(SEGMENT_SIZE, 64).ok_or(UsbError::NoMemory)?;
        let Some(erst) = hal.alloc_dma(16, 64) else {
            hal.free_dma(segment);
            return Err(UsbError::NoMemory);
        };
        erst.write64(0, segment.phys());
        erst.write32(8, TRBS as u32);
        Ok(EventRing {
            segment,
            erst,
            index: 0,
            cycle: true,
        })
    }

    /// The ERST's physical address (for ERSTBA).
    pub fn erst_phys(&self) -> u64 {
        self.erst.phys()
    }

    /// The segment's physical address.
    pub fn phys(&self) -> u64 {
        self.segment.phys()
    }

    /// The next event, if the controller has written one.
    pub fn next(&mut self) -> Option<Trb> {
        let offset = self.index * TRB_SIZE;
        let control = self.segment.read32(offset + 12);
        if (control & 1 != 0) != self.cycle {
            return None;
        }
        // The rest of the TRB is read after its cycle bit said it is there.
        fence(Ordering::Acquire);
        let trb = Trb([
            self.segment.read32(offset),
            self.segment.read32(offset + 4),
            self.segment.read32(offset + 8),
            control,
        ]);
        self.index += 1;
        if self.index == TRBS {
            self.index = 0;
            self.cycle = !self.cycle;
        }
        Some(trb)
    }

    /// Where the next event goes: what ERDP must be told.
    pub fn dequeue_pointer(&self) -> u64 {
        self.segment.phys_at(self.index * TRB_SIZE)
    }

    pub fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.segment);
        hal.free_dma(self.erst);
    }
}

````

- [ ] **Step 9: Implement `crates/usb/src/xhci/trb.rs`**

Insert this at the top of `crates/usb/src/xhci/trb.rs`, above `#[cfg(test)]`:

````rust
//! Transfer Request Blocks (xHCI 6.4): the 16-byte records of every ring.
//! Constructors build the TRBs this driver sends; accessors read the
//! events the controller sends back. The cycle bit is left to the ring.

use crate::Setup;

/// A TRB as four little-endian dwords.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trb(pub [u32; 4]);

// TRB types (xHCI table 6-91).
pub const NORMAL: u8 = 1;
pub const SETUP_STAGE: u8 = 2;
pub const DATA_STAGE: u8 = 3;
pub const STATUS_STAGE: u8 = 4;
pub const LINK: u8 = 6;
pub const ENABLE_SLOT: u8 = 9;
pub const DISABLE_SLOT: u8 = 10;
pub const ADDRESS_DEVICE: u8 = 11;
pub const CONFIGURE_ENDPOINT: u8 = 12;
pub const EVALUATE_CONTEXT: u8 = 13;
pub const RESET_ENDPOINT: u8 = 14;
pub const STOP_ENDPOINT: u8 = 15;
pub const SET_TR_DEQUEUE: u8 = 16;
pub const NO_OP_COMMAND: u8 = 23;
pub const TRANSFER_EVENT: u8 = 32;
pub const COMMAND_COMPLETION: u8 = 33;
pub const PORT_STATUS_CHANGE: u8 = 34;
pub const HOST_CONTROLLER_EVENT: u8 = 37;

// Completion codes (xHCI table 6-90).
pub const SUCCESS: u8 = 1;
pub const DATA_BUFFER_ERROR: u8 = 2;
pub const BABBLE: u8 = 3;
pub const USB_TRANSACTION_ERROR: u8 = 4;
pub const TRB_ERROR: u8 = 5;
pub const STALL: u8 = 6;
pub const RESOURCE_ERROR: u8 = 7;
pub const BANDWIDTH_ERROR: u8 = 8;
pub const NO_SLOTS: u8 = 9;
pub const SLOT_NOT_ENABLED: u8 = 11;
pub const ENDPOINT_NOT_ENABLED: u8 = 12;
pub const SHORT_PACKET: u8 = 13;
pub const PARAMETER_ERROR: u8 = 17;
pub const CONTEXT_STATE_ERROR: u8 = 19;
pub const EVENT_RING_FULL: u8 = 21;
pub const COMMAND_RING_STOPPED: u8 = 24;
pub const COMMAND_ABORTED: u8 = 25;
pub const STOPPED: u8 = 26;
pub const STOPPED_LENGTH_INVALID: u8 = 27;

// Dword 3 flags. Several bits mean different things per TRB type.
pub const CYCLE: u32 = 1 << 0;
/// Link TRB: toggle the consumer's cycle state.
pub const TOGGLE_CYCLE: u32 = 1 << 1;
/// Interrupt on Short Packet: an event with the residual when a transfer
/// ends short.
pub const ISP: u32 = 1 << 2;
pub const CHAIN: u32 = 1 << 4;
pub const IOC: u32 = 1 << 5;
/// Immediate data: the setup packet is in the TRB itself.
pub const IDT: u32 = 1 << 6;
/// Address Device: Block Set Address Request. Configure Endpoint:
/// Deconfigure.
pub const BSR: u32 = 1 << 9;
pub const DECONFIGURE: u32 = 1 << 9;
/// Data and Status Stage: the direction is IN.
pub const DIR_IN: u32 = 1 << 16;

// Setup Stage Transfer Type (xHCI 6.4.1.2.1).
const TRT_NO_DATA: u32 = 0;
const TRT_OUT: u32 = 2;
const TRT_IN: u32 = 3;

fn control(kind: u8) -> u32 {
    (kind as u32) << 10
}

fn slot_field(slot: u8) -> u32 {
    (slot as u32) << 24
}

fn endpoint_field(dci: usize) -> u32 {
    ((dci as u32) & 0x1F) << 16
}

impl Trb {
    fn with_pointer(pointer: u64, status: u32, control: u32) -> Trb {
        Trb([pointer as u32, (pointer >> 32) as u32, status, control])
    }

    /// A Normal TRB for `len` bytes at `buffer`, with an event when it
    /// completes or ends short (xHCI 6.4.1.1).
    pub fn normal(buffer: u64, len: u32) -> Trb {
        Trb::with_pointer(buffer, len & 0x1_FFFF, control(NORMAL) | IOC | ISP)
    }

    /// A Setup Stage TRB carrying the 8 setup bytes inline (xHCI 6.4.1.2.1).
    pub fn setup_stage(setup: &Setup) -> Trb {
        let b = setup.to_bytes();
        let trt = match (setup.length, setup.is_in()) {
            (0, _) => TRT_NO_DATA,
            (_, true) => TRT_IN,
            (_, false) => TRT_OUT,
        };
        Trb([
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            8,
            control(SETUP_STAGE) | IDT | trt << 16,
        ])
    }

    /// A Data Stage TRB for `len` bytes at `buffer` (xHCI 6.4.1.2.2). A
    /// short IN transfer produces an event with the residual.
    pub fn data_stage(buffer: u64, len: u32, dir_in: bool) -> Trb {
        let dir = if dir_in { DIR_IN } else { 0 };
        Trb::with_pointer(buffer, len & 0x1_FFFF, control(DATA_STAGE) | ISP | dir)
    }

    /// A Status Stage TRB with an event on completion (xHCI 6.4.1.2.3).
    pub fn status_stage(dir_in: bool) -> Trb {
        let dir = if dir_in { DIR_IN } else { 0 };
        Trb([0, 0, 0, control(STATUS_STAGE) | IOC | dir])
    }

    /// A Link TRB to `segment` that toggles the consumer's cycle state
    /// (xHCI 6.4.4.1).
    pub fn link(segment: u64) -> Trb {
        Trb::with_pointer(segment, 0, control(LINK) | TOGGLE_CYCLE)
    }

    pub fn no_op_command() -> Trb {
        Trb([0, 0, 0, control(NO_OP_COMMAND)])
    }

    /// Enable Slot for a USB device (slot type 0, xHCI 6.4.3.2).
    pub fn enable_slot() -> Trb {
        Trb([0, 0, 0, control(ENABLE_SLOT)])
    }

    pub fn disable_slot(slot: u8) -> Trb {
        Trb([0, 0, 0, control(DISABLE_SLOT) | slot_field(slot)])
    }

    /// Address Device with the input context at `input` (xHCI 6.4.3.4).
    /// With `bsr` the controller does not send SET_ADDRESS.
    pub fn address_device(input: u64, slot: u8, bsr: bool) -> Trb {
        let bsr = if bsr { BSR } else { 0 };
        Trb::with_pointer(input, 0, control(ADDRESS_DEVICE) | bsr | slot_field(slot))
    }

    /// Configure Endpoint (xHCI 6.4.3.5); `deconfigure` drops every
    /// endpoint but EP0.
    pub fn configure_endpoint(input: u64, slot: u8, deconfigure: bool) -> Trb {
        let dc = if deconfigure { DECONFIGURE } else { 0 };
        Trb::with_pointer(
            input,
            0,
            control(CONFIGURE_ENDPOINT) | dc | slot_field(slot),
        )
    }

    pub fn evaluate_context(input: u64, slot: u8) -> Trb {
        Trb::with_pointer(input, 0, control(EVALUATE_CONTEXT) | slot_field(slot))
    }

    /// Reset Endpoint: a Halted endpoint becomes Stopped (xHCI 4.6.8).
    pub fn reset_endpoint(slot: u8, dci: usize) -> Trb {
        Trb([
            0,
            0,
            0,
            control(RESET_ENDPOINT) | endpoint_field(dci) | slot_field(slot),
        ])
    }

    /// Stop Endpoint: a Running endpoint becomes Stopped (xHCI 4.6.9).
    pub fn stop_endpoint(slot: u8, dci: usize) -> Trb {
        Trb([
            0,
            0,
            0,
            control(STOP_ENDPOINT) | endpoint_field(dci) | slot_field(slot),
        ])
    }

    /// Set TR Dequeue Pointer to `dequeue`, whose bit 0 is the Dequeue
    /// Cycle State (xHCI 6.4.3.9).
    pub fn set_tr_dequeue(slot: u8, dci: usize, dequeue: u64) -> Trb {
        Trb::with_pointer(
            dequeue & !0xE,
            0,
            control(SET_TR_DEQUEUE) | endpoint_field(dci) | slot_field(slot),
        )
    }

    pub fn trb_type(&self) -> u8 {
        (self.0[3] >> 10) as u8 & 0x3F
    }

    pub fn cycle(&self) -> bool {
        self.0[3] & CYCLE != 0
    }

    pub fn chain(&self) -> bool {
        self.0[3] & CHAIN != 0
    }

    /// This TRB with its cycle bit set to `cycle`.
    pub fn with_cycle(self, cycle: bool) -> Trb {
        let mut t = self;
        t.0[3] = t.0[3] & !CYCLE | cycle as u32;
        t
    }

    /// Dwords 0-1: a buffer, a context or, in events, the TRB concerned.
    pub fn pointer(&self) -> u64 {
        self.0[0] as u64 | (self.0[1] as u64) << 32
    }

    pub fn completion_code(&self) -> u8 {
        (self.0[2] >> 24) as u8
    }

    /// Transfer events: the bytes not transferred (the residual).
    pub fn transfer_length(&self) -> u32 {
        self.0[2] & 0xFF_FFFF
    }

    pub fn slot_id(&self) -> u8 {
        (self.0[3] >> 24) as u8
    }

    /// Transfer events: the endpoint's DCI.
    pub fn endpoint_id(&self) -> usize {
        (self.0[3] >> 16) as usize & 0x1F
    }

    /// Port Status Change events: the root port number.
    pub fn port_id(&self) -> u8 {
        (self.0[0] >> 24) as u8
    }
}

/// A completion code's name for the log.
pub fn completion_name(code: u8) -> &'static str {
    match code {
        0 => "invalid",
        SUCCESS => "success",
        DATA_BUFFER_ERROR => "data buffer error",
        BABBLE => "babble",
        USB_TRANSACTION_ERROR => "USB transaction error",
        TRB_ERROR => "TRB error",
        STALL => "stall",
        RESOURCE_ERROR => "resource error",
        BANDWIDTH_ERROR => "bandwidth error",
        NO_SLOTS => "no slots available",
        SLOT_NOT_ENABLED => "slot not enabled",
        ENDPOINT_NOT_ENABLED => "endpoint not enabled",
        SHORT_PACKET => "short packet",
        PARAMETER_ERROR => "parameter error",
        CONTEXT_STATE_ERROR => "context state error",
        EVENT_RING_FULL => "event ring full",
        COMMAND_RING_STOPPED => "command ring stopped",
        COMMAND_ABORTED => "command aborted",
        STOPPED => "stopped",
        STOPPED_LENGTH_INVALID => "stopped, length invalid",
        _ => "other error",
    }
}

/// A command TRB type's name for the log.
pub fn command_name(kind: u8) -> &'static str {
    match kind {
        ENABLE_SLOT => "Enable Slot",
        DISABLE_SLOT => "Disable Slot",
        ADDRESS_DEVICE => "Address Device",
        CONFIGURE_ENDPOINT => "Configure Endpoint",
        EVALUATE_CONTEXT => "Evaluate Context",
        RESET_ENDPOINT => "Reset Endpoint",
        STOP_ENDPOINT => "Stop Endpoint",
        SET_TR_DEQUEUE => "Set TR Dequeue Pointer",
        NO_OP_COMMAND => "No Op",
        _ => "unknown",
    }
}

````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 39 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -m "usb: TRB rings with link TRBs and cycle bits"
````


### Task 5: Device and input contexts at 32 and 64 bytes

Spec §6.2's context-size quirk: `HCCPARAMS1.CSZ` makes every context 32 bytes (QEMU) or 64 bytes (Intel), and the stride is a parameter of every accessor, never assumed. An input context holds the input control context (drop and add flags), the slot context and 31 endpoint contexts at their DCI; a device context the slot context and the endpoints one entry earlier. `dci` maps endpoint addresses to device context indexes (EP0 is 1, OUT n is 2n, IN n is 2n + 1); `interval_exponent` turns `bInterval` into the xHCI interval (xHCI 6.2.3.6: a low-speed keyboard's 10 ms becomes 2^6 × 125 µs).

**Files:**
- Create: `crates/usb/src/xhci/context.rs`
- Modify: `crates/usb/src/xhci/mod.rs`

**Interfaces:**
- Consumes: `DmaBuf`, `Speed`, `descriptor::EndpointKind` (Task 3).
- Produces: `usb::xhci::context::{EP_* states, endpoint type constants, dci(u8) -> usize, endpoint_type(EndpointKind, bool) -> u8, interval_exponent(Speed, EndpointKind, u8) -> u8, speed_id, speed_from_id, input_size(stride), device_size(stride), SlotContext, EndpointContext, Input { new(&DmaBuf, stride), clear, set_flags, slot/endpoint writers }, device context readers}`.

- [ ] **Step 1: Write the failing tests for `crates/usb/src/xhci/context.rs`**

Create `crates/usb/src/xhci/context.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hal;
    use crate::testing::FakeHal;

    fn sample_slot() -> SlotContext {
        SlotContext {
            route_string: 0x1_2345,
            speed: 4,
            context_entries: 31,
            root_port: 13,
            interrupter: 0x2AB,
            address: 7,
            state: 3,
        }
    }

    fn sample_endpoint() -> EndpointContext {
        EndpointContext {
            state: EP_HALTED,
            mult: 2,
            max_primary_streams: 0,
            interval: 6,
            cerr: 3,
            ep_type: INTERRUPT_IN,
            max_burst: 15,
            max_packet: 1024,
            dequeue: 0x12_3456_7890 | 1,
            average_trb_length: 3072,
            max_esit_payload: 0x12_3456,
        }
    }

    #[test]
    fn slot_context_fields_round_trip_at_the_slot_offset() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let input = Input::new(&buf, stride);
            input.set_slot(&sample_slot());
            assert_eq!(SlotContext::read(&buf, stride), sample_slot());
            // xHCI 6.2.2: the slot context is entry 1 of an input context.
            assert_eq!(buf.read32(stride), 0x1_2345 | 4 << 20 | 31 << 27);
            assert_eq!(buf.read32(stride + 4), 13 << 16);
            assert_eq!(buf.read32(stride + 8), 0x2AB << 22);
            assert_eq!(buf.read32(stride + 12), 7 | 3 << 27);
            assert_eq!(buf.read32(0), 0, "the control context is untouched");
            hal.free_dma(buf);
        }
    }

    #[test]
    fn endpoint_context_fields_round_trip_at_dci_plus_one() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let input = Input::new(&buf, stride);
            for dci in [1, 3, 31] {
                input.set_endpoint(dci, &sample_endpoint());
                let at = (dci + 1) * stride;
                assert_eq!(EndpointContext::read(&buf, at), sample_endpoint());
                assert_eq!(buf.read32(at), 2 | 2 << 8 | 6 << 16 | 0x12 << 24);
                assert_eq!(buf.read32(at + 4), 3 << 1 | 7 << 3 | 15 << 8 | 1024 << 16);
                assert_eq!(buf.read64(at + 8), 0x12_3456_7891);
                assert_eq!(buf.read32(at + 16), 3072 | 0x3456 << 16);
            }
            hal.free_dma(buf);
        }
    }

    #[test]
    fn output_contexts_start_with_the_slot() {
        for stride in [32, 64] {
            let hal = FakeHal::new();
            let buf = hal.alloc_dma(4096, 64).unwrap();
            let out = Output::new(&buf, stride);
            // As the controller writes them: the slot at 0, DCI n at n.
            sample_slot().write(&buf, 0);
            sample_endpoint().write(&buf, 3 * stride);
            assert_eq!(
                (out.slot(), out.endpoint(3)),
                (sample_slot(), sample_endpoint())
            );
            hal.free_dma(buf);
        }
    }

    #[test]
    fn the_input_control_context_holds_drop_and_add_flags_and_clears() {
        let hal = FakeHal::new();
        let buf = hal.alloc_dma(4096, 64).unwrap();
        let input = Input::new(&buf, 64);
        input.set_flags(1 << 4, 0b1011);
        assert_eq!((buf.read32(0), buf.read32(4)), (1 << 4, 0b1011));

        input.set_endpoint(31, &sample_endpoint());
        input.clear();
        assert_eq!((buf.read32(0), buf.read32(4)), (0, 0));
        assert_eq!(
            EndpointContext::read(&buf, 32 * 64),
            EndpointContext::default()
        );
        assert_eq!(input_size(64), 2112);
        assert_eq!(device_size(32), 1024);
        hal.free_dma(buf);
    }

    #[test]
    fn the_dequeue_pointer_keeps_its_cycle_state_but_not_stream_bits() {
        let hal = FakeHal::new();
        let buf = hal.alloc_dma(4096, 64).unwrap();
        let input = Input::new(&buf, 32);
        let ep = EndpointContext {
            dequeue: 0x5000 | 0xF,
            ..EndpointContext::default()
        };
        input.set_endpoint(1, &ep);
        assert_eq!(EndpointContext::read(&buf, 2 * 32).dequeue, 0x5001);
        hal.free_dma(buf);
    }

    #[test]
    fn device_context_indices() {
        assert_eq!(dci(0x00), 1);
        assert_eq!(dci(0x80), 1);
        assert_eq!(dci(0x01), 2);
        assert_eq!(dci(0x81), 3);
        assert_eq!(dci(0x02), 4);
        assert_eq!(dci(0x82), 5);
        assert_eq!(dci(0x8F), 31);
    }

    #[test]
    fn endpoint_types_follow_kind_and_direction() {
        assert_eq!(endpoint_type(EndpointKind::Interrupt, true), 7);
        assert_eq!(endpoint_type(EndpointKind::Interrupt, false), 3);
        assert_eq!(endpoint_type(EndpointKind::Bulk, true), 6);
        assert_eq!(endpoint_type(EndpointKind::Bulk, false), 2);
        assert_eq!(endpoint_type(EndpointKind::Control, true), 4);
    }

    #[test]
    fn intervals_follow_xhci_6_2_3_6() {
        use EndpointKind::*;
        use Speed::*;
        assert_eq!(interval_exponent(Low, Interrupt, 10), 6, "the K120");
        assert_eq!(interval_exponent(Full, Interrupt, 1), 3);
        assert_eq!(interval_exponent(Full, Interrupt, 255), 10);
        assert_eq!(interval_exponent(Full, Interrupt, 0), 3);
        assert_eq!(interval_exponent(High, Interrupt, 1), 0);
        assert_eq!(interval_exponent(High, Interrupt, 4), 3);
        assert_eq!(interval_exponent(High, Interrupt, 0), 0);
        assert_eq!(interval_exponent(Super, Interrupt, 16), 15);
        assert_eq!(interval_exponent(Super, Interrupt, 200), 15);
        assert_eq!(interval_exponent(Full, Isochronous, 1), 3);
        assert_eq!(interval_exponent(High, Isochronous, 4), 3);
        assert_eq!(interval_exponent(High, Bulk, 255), 0);
        assert_eq!(interval_exponent(Super, Bulk, 0), 0);
        assert_eq!(interval_exponent(Full, Control, 10), 0);
    }

    #[test]
    fn speed_ids_follow_the_default_table() {
        for s in [
            Speed::Low,
            Speed::Full,
            Speed::High,
            Speed::Super,
            Speed::SuperPlus,
        ] {
            assert_eq!(speed_from_id(speed_id(s)), Some(s));
        }
        assert_eq!(speed_id(Speed::Low), 2);
        assert_eq!(speed_from_id(0), None);
        assert_eq!(speed_from_id(6), None);
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust

mod ring;
````

with:

````rust

mod context;
mod ring;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` unresolved import `EndpointKind` ``; `` unresolved import `Speed` ``.

- [ ] **Step 4: Implement `crates/usb/src/xhci/context.rs`**

Insert this at the top of `crates/usb/src/xhci/context.rs`, above `#[cfg(test)]`:

````rust
//! Device and input contexts (xHCI 6.2). A context is 32 or 64 bytes
//! (HCCPARAMS1.CSZ); the stride is part of every view, never assumed. An
//! input context holds the input control context, the slot context and 31
//! endpoint contexts at their DCI; a device (output) context the slot
//! context and the endpoints, one entry earlier.

use crate::DmaBuf;
use crate::Speed;
use crate::descriptor::EndpointKind;

/// Entries in a device context: the slot and 31 endpoints.
const DEVICE_ENTRIES: usize = 32;

// Endpoint states (xHCI 6.2.3, dword 0).
pub const EP_DISABLED: u8 = 0;
pub const EP_RUNNING: u8 = 1;
pub const EP_HALTED: u8 = 2;
pub const EP_STOPPED: u8 = 3;
pub const EP_ERROR: u8 = 4;

// Endpoint types (xHCI table 6-9).
pub const ISOCH_OUT: u8 = 1;
pub const BULK_OUT: u8 = 2;
pub const INTERRUPT_OUT: u8 = 3;
pub const CONTROL: u8 = 4;
pub const ISOCH_IN: u8 = 5;
pub const BULK_IN: u8 = 6;
pub const INTERRUPT_IN: u8 = 7;

/// The Device Context Index of an endpoint address: EP0 is 1, OUT n is
/// 2n, IN n is 2n + 1 (xHCI 4.5.1).
pub fn dci(endpoint_address: u8) -> usize {
    let number = (endpoint_address & 0x0F) as usize;
    if number == 0 {
        1
    } else {
        2 * number + (endpoint_address >> 7) as usize
    }
}

/// The context's endpoint type for an endpoint of `kind` and direction.
pub fn endpoint_type(kind: EndpointKind, is_in: bool) -> u8 {
    match (kind, is_in) {
        (EndpointKind::Control, _) => CONTROL,
        (EndpointKind::Isochronous, false) => ISOCH_OUT,
        (EndpointKind::Isochronous, true) => ISOCH_IN,
        (EndpointKind::Bulk, false) => BULK_OUT,
        (EndpointKind::Bulk, true) => BULK_IN,
        (EndpointKind::Interrupt, false) => INTERRUPT_OUT,
        (EndpointKind::Interrupt, true) => INTERRUPT_IN,
    }
}

/// The endpoint context's Interval: the service interval is 2^n × 125 µs
/// (xHCI 6.2.3.6). `b_interval` counts frames for low- and full-speed
/// interrupt endpoints and is an exponent for everything else periodic.
pub fn interval_exponent(speed: Speed, kind: EndpointKind, b_interval: u8) -> u8 {
    let exponent_minus_one = || b_interval.saturating_sub(1).min(15);
    match (kind, speed) {
        (EndpointKind::Control | EndpointKind::Bulk, _) => 0,
        (EndpointKind::Interrupt, Speed::Low | Speed::Full) => {
            // Frames to microframes, rounded down to a power of two.
            (b_interval.max(1) as u32 * 8).ilog2().clamp(3, 10) as u8
        }
        (EndpointKind::Isochronous, Speed::Full) => b_interval.clamp(1, 13) + 2,
        _ => exponent_minus_one(),
    }
}

/// The Protocol Speed ID of the default speed table (xHCI 7.2.2.1.1), as
/// PORTSC and the slot context carry it.
pub fn speed_id(speed: Speed) -> u8 {
    match speed {
        Speed::Full => 1,
        Speed::Low => 2,
        Speed::High => 3,
        Speed::Super => 4,
        Speed::SuperPlus => 5,
    }
}

pub fn speed_from_id(id: u8) -> Option<Speed> {
    match id {
        1 => Some(Speed::Full),
        2 => Some(Speed::Low),
        3 => Some(Speed::High),
        4 => Some(Speed::Super),
        5 => Some(Speed::SuperPlus),
        _ => None,
    }
}

/// Bytes for an input context: the control context and 32 more.
pub fn input_size(stride: usize) -> usize {
    (DEVICE_ENTRIES + 1) * stride
}

/// Bytes for a device (output) context.
pub fn device_size(stride: usize) -> usize {
    DEVICE_ENTRIES * stride
}

/// A slot context's fields (xHCI 6.2.2). Hubs and TTs are not used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotContext {
    pub route_string: u32,
    /// Protocol Speed ID.
    pub speed: u8,
    /// The highest DCI in use.
    pub context_entries: u8,
    pub root_port: u8,
    pub interrupter: u16,
    /// Output only: the USB address the controller assigned.
    pub address: u8,
    /// Output only.
    pub state: u8,
}

impl SlotContext {
    fn write(&self, buf: &DmaBuf, at: usize) {
        buf.write32(
            at,
            self.route_string & 0xF_FFFF
                | (self.speed as u32 & 0xF) << 20
                | (self.context_entries as u32 & 0x1F) << 27,
        );
        buf.write32(at + 4, (self.root_port as u32) << 16);
        buf.write32(at + 8, (self.interrupter as u32 & 0x3FF) << 22);
        buf.write32(
            at + 12,
            self.address as u32 | (self.state as u32 & 0x1F) << 27,
        );
    }

    fn read(buf: &DmaBuf, at: usize) -> SlotContext {
        let d = [
            buf.read32(at),
            buf.read32(at + 4),
            buf.read32(at + 8),
            buf.read32(at + 12),
        ];
        SlotContext {
            route_string: d[0] & 0xF_FFFF,
            speed: (d[0] >> 20) as u8 & 0xF,
            context_entries: (d[0] >> 27) as u8,
            root_port: (d[1] >> 16) as u8,
            interrupter: (d[2] >> 22) as u16,
            address: d[3] as u8,
            state: (d[3] >> 27) as u8,
        }
    }
}

/// An endpoint context's fields (xHCI 6.2.3). Streams are not used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndpointContext {
    /// Output only.
    pub state: u8,
    pub mult: u8,
    pub max_primary_streams: u8,
    pub interval: u8,
    /// Error count: retries before the endpoint halts (3 = the most).
    pub cerr: u8,
    pub ep_type: u8,
    pub max_burst: u8,
    pub max_packet: u16,
    /// The TR Dequeue Pointer with the Dequeue Cycle State in bit 0.
    pub dequeue: u64,
    pub average_trb_length: u16,
    /// Max ESIT Payload: bytes per service interval (24 bits).
    pub max_esit_payload: u32,
}

impl EndpointContext {
    fn write(&self, buf: &DmaBuf, at: usize) {
        buf.write32(
            at,
            self.state as u32 & 7
                | (self.mult as u32 & 3) << 8
                | (self.max_primary_streams as u32 & 0x1F) << 10
                | (self.interval as u32) << 16
                | (self.max_esit_payload >> 16 & 0xFF) << 24,
        );
        buf.write32(
            at + 4,
            (self.cerr as u32 & 3) << 1
                | (self.ep_type as u32 & 7) << 3
                | (self.max_burst as u32) << 8
                | (self.max_packet as u32) << 16,
        );
        buf.write64(at + 8, self.dequeue & !0xE);
        buf.write32(
            at + 16,
            self.average_trb_length as u32 | (self.max_esit_payload & 0xFFFF) << 16,
        );
    }

    fn read(buf: &DmaBuf, at: usize) -> EndpointContext {
        let d0 = buf.read32(at);
        let d1 = buf.read32(at + 4);
        let d4 = buf.read32(at + 16);
        EndpointContext {
            state: d0 as u8 & 7,
            mult: (d0 >> 8) as u8 & 3,
            max_primary_streams: (d0 >> 10) as u8 & 0x1F,
            interval: (d0 >> 16) as u8,
            cerr: (d1 >> 1) as u8 & 3,
            ep_type: (d1 >> 3) as u8 & 7,
            max_burst: (d1 >> 8) as u8,
            max_packet: (d1 >> 16) as u16,
            dequeue: buf.read64(at + 8) & !0xE,
            average_trb_length: d4 as u16,
            max_esit_payload: (d0 >> 24) << 16 | d4 >> 16,
        }
    }
}

/// An input context in `buf`, for Address Device, Evaluate Context and
/// Configure Endpoint (xHCI 6.2.5).
pub struct Input<'a> {
    buf: &'a DmaBuf,
    stride: usize,
}

impl<'a> Input<'a> {
    pub fn new(buf: &'a DmaBuf, stride: usize) -> Input<'a> {
        debug_assert!(buf.size() >= input_size(stride));
        Input { buf, stride }
    }

    /// Zeroes the whole context, so no flag or field of an earlier command
    /// is left over.
    pub fn clear(&self) {
        self.buf.zero(0, input_size(self.stride));
    }

    /// The Input Control Context's drop and add flags: bit n for DCI n,
    /// bit 0 (add only) for the slot context.
    pub fn set_flags(&self, drop: u32, add: u32) {
        self.buf.write32(0, drop);
        self.buf.write32(4, add);
    }

    pub fn set_slot(&self, slot: &SlotContext) {
        slot.write(self.buf, self.stride);
    }

    pub fn set_endpoint(&self, dci: usize, ep: &EndpointContext) {
        ep.write(self.buf, self.endpoint_offset(dci));
    }

    fn endpoint_offset(&self, dci: usize) -> usize {
        assert!((1..DEVICE_ENTRIES).contains(&dci), "DCI {dci}");
        (dci + 1) * self.stride
    }
}

/// A device (output) context in `buf`, which the controller writes.
pub struct Output<'a> {
    buf: &'a DmaBuf,
    stride: usize,
}

impl<'a> Output<'a> {
    pub fn new(buf: &'a DmaBuf, stride: usize) -> Output<'a> {
        debug_assert!(buf.size() >= device_size(stride));
        Output { buf, stride }
    }

    pub fn slot(&self) -> SlotContext {
        SlotContext::read(self.buf, 0)
    }

    pub fn endpoint(&self, dci: usize) -> EndpointContext {
        assert!((1..DEVICE_ENTRIES).contains(&dci), "DCI {dci}");
        EndpointContext::read(self.buf, dci * self.stride)
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 48 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "usb: xHCI device and input contexts at 32 and 64 bytes"
````


### Task 6: Registers and the extended capability list

Where the registers are and what the controller says about itself. `Regs` computes the operational, runtime, port and doorbell blocks from CAPLENGTH, RTSOFF and DBOFF and keeps every access inside the BAR; `Params` reads the capability registers (slots, ports, interrupters, the scratchpad count split over HCSPARAMS2's high and low fields, AC64, CSZ, PPC, xECP). The extended capability list is walked with bounds (at most 64 entries, every offset inside the BAR), so a list that never ends or points outside cannot hang or fault the kernel; it yields USB Legacy Support (for the BIOS handoff) and the Supported Protocol capabilities, which say which root ports are USB 2 and which USB 3 (spec §6.2). `ControllerInfo` is the line the NUC photo shows: version, ports per protocol, context size, scratchpads. The fake controller's register file starts here, with three presets: `basic` (as simple as QEMU's), `qemu` (QEMU 8.2's port layout: USB 3 on ports 1–4) and `intel` (the NUC's PCH xHCI: 64-byte contexts, 2 scratchpads, BIOS-owned legacy support, port power control, other capabilities between the protocol ones).

**Files:**
- Modify: `crates/usb/src/testing/hal.rs`
- Modify: `crates/usb/src/testing/mod.rs`
- Create: `crates/usb/src/testing/xhci/mod.rs`
- Create: `crates/usb/src/testing/xhci/regs.rs`
- Create: `crates/usb/src/xhci/caps.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Create: `crates/usb/src/xhci/regs.rs`

**Interfaces:**
- Consumes: Tasks 3–5.
- Produces: `usb::xhci::{ControllerInfo { version, max_slots, ports, usb2_ports, usb3_ports, context_size, scratchpads } (Display: "xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 64-byte contexts, 2 scratchpads")}`; `xhci::regs::{register offsets and bits, Regs, Params, portsc_neutral}`; `xhci::caps::{LEGACY_SUPPORT, SUPPORTED_PROTOCOL, CapList { walk, find }, Protocol, PortProtocol { Usb2, Usb3 }, protocols, port_map}`; test support `testing::{FakeConfig { basic, qemu, intel }, ExtCap, FakeXhci}` and `FakeHal::with_controller`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/hal.rs`**

In `crates/usb/src/testing/hal.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! A fake [`Hal`]: virtual time, DMA memory from the host heap (its
//! physical address is its host address), and captured log lines.

use crate::{DmaBuf, Hal};
````

with:

````rust
//! A fake [`Hal`]: virtual time, DMA memory from the host heap (its
//! physical address is its host address), captured log lines, and the
//! registers of a [`FakeXhci`], which sees every access and every tick.

use super::xhci::{FakeConfig, FakeXhci};
use crate::{DmaBuf, Hal};
````

Replace:

````rust
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

````

with:

````rust
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::{Cell, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The fake controller's BAR: the only MMIO `map_mmio` accepts.
pub const FAKE_BAR: u64 = 0xFE00_0000;
pub const FAKE_BAR_LEN: usize = 0x1_0000;
/// Where the BAR appears to be mapped.
const FAKE_MMIO: usize = 0xFFFF_9000_FE00_0000;

````

Replace:

````rust
    log: RefCell<Vec<String>>,
}
````

with:

````rust
    log: RefCell<Vec<String>>,
    xhci: RefCell<Option<FakeXhci>>,
}
````

Replace:

````rust
impl FakeHal {
    pub fn new() -> FakeHal {
````

with:

````rust
impl FakeHal {
    /// A machine without a controller (for rings and contexts).
    pub fn new() -> FakeHal {
````

Replace:

````rust
            log: RefCell::new(Vec::new()),
        }))
    }
````

with:

````rust
            log: RefCell::new(Vec::new()),
            xhci: RefCell::new(None),
        }))
    }

    /// A machine with a fake controller at [`FAKE_BAR`].
    pub fn with_controller(config: FakeConfig) -> FakeHal {
        let hal = FakeHal::new();
        *hal.0.xhci.borrow_mut() = Some(FakeXhci::new(config));
        hal
    }

    /// The fake controller, to plug devices, turn knobs and look inside.
    /// Drop the guard before calling the driver again.
    pub fn fake(&self) -> RefMut<'_, FakeXhci> {
        RefMut::map(self.0.xhci.borrow_mut(), |x| {
            x.as_mut().expect("fake hal: no controller")
        })
    }
````

Replace:

````rust
    fn advance(&self, d: Duration) {
        self.0.clock.set(self.0.clock.get() + d);
    }
}

impl Hal for FakeHal {
    fn map_mmio(&self, _phys: u64, _len: usize) -> Option<usize> {
        None
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        panic!("fake hal: no controller for the read at {addr:#x}")
    }

    unsafe fn write32(&self, addr: usize, _value: u32) {
        panic!("fake hal: no controller for the write at {addr:#x}")
    }
````

with:

````rust
    fn advance(&self, d: Duration) {
        let now = self.0.clock.get() + d;
        self.0.clock.set(now);
        if let Some(x) = self.0.xhci.borrow_mut().as_mut() {
            x.advance_to(now, &self.0.dma.borrow());
        }
    }

    /// The BAR offset of a mapped register address.
    fn offset(&self, addr: usize) -> usize {
        match addr.checked_sub(FAKE_MMIO) {
            Some(o) if o < FAKE_BAR_LEN && o.is_multiple_of(4) => o,
            _ => panic!("fake hal: register access at {addr:#x} outside the BAR"),
        }
    }
}

impl Hal for FakeHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        let present = self.0.xhci.borrow().is_some();
        (present && phys == FAKE_BAR && len <= FAKE_BAR_LEN).then_some(FAKE_MMIO)
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        let offset = self.offset(addr);
        self.fake().read(offset, &self.0.dma.borrow())
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        let offset = self.offset(addr);
        self.fake().write(offset, value, &self.0.dma.borrow());
    }
````

Replace:

````rust
    #[test]
    fn log_lines_are_captured() {
````

with:

````rust
    #[test]
    fn only_the_fake_bar_can_be_mapped() {
        assert_eq!(FakeHal::new().map_mmio(FAKE_BAR, 4096), None);
        let hal = FakeHal::with_controller(FakeConfig::basic());
        assert_eq!(hal.map_mmio(FAKE_BAR + 0x1000, 4096), None);
        assert_eq!(hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN + 1), None);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        // SAFETY: the fake checks every register address.
        assert_eq!(unsafe { hal.read32(base) } >> 16, 0x0100);
    }

    #[test]
    #[should_panic(expected = "outside the BAR")]
    fn registers_past_the_bar_panic() {
        let hal = FakeHal::with_controller(FakeConfig::basic());
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        // SAFETY: the fake checks every register address.
        unsafe { hal.read32(base + FAKE_BAR_LEN) };
    }

    #[test]
    fn log_lines_are_captured() {
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/mod.rs`**

Replace the whole of `crates/usb/src/testing/mod.rs` with:

````rust
//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! and the fake xHCI controller behind its registers.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod hal;
mod xhci;

pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use xhci::{ExtCap, FakeConfig};
````

- [ ] **Step 3: Create `crates/usb/src/testing/xhci/mod.rs`**

The fake controller. It is as strict as hardware: anything the xHCI specification forbids panics with a message starting `fake xhci:`, so a driver bug fails a test instead of passing by luck.

Create `crates/usb/src/testing/xhci/mod.rs`:

````rust
//! A fake xHCI controller. It is as strict as hardware: whatever the xHCI
//! specification forbids panics with a message starting "fake xhci: ", so
//! a driver bug fails its test instead of passing by luck. It decodes
//! registers, TRBs and contexts with its own constants, not the driver's.

mod regs;

use super::hal::Dma;
use core::time::Duration;

/// An extended capability of the fake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtCap {
    /// USB Legacy Support (ID 1): USBLEGSUP and USBLEGCTLSTS.
    Legacy,
    /// Supported Protocol (ID 2): USB `major.minor` on `count` ports from
    /// `first`.
    Protocol {
        major: u8,
        minor: u8,
        first: u8,
        count: u8,
    },
    /// A capability the driver has no use for.
    Other(u8),
}

/// A capability at `offset` bytes into the BAR; `next` is the distance to
/// the next one in dwords (0 ends the list).
#[derive(Clone, Debug)]
pub struct FakeCap {
    pub offset: usize,
    pub next: u8,
    pub cap: ExtCap,
}

/// What the fake is: the capability values and the fault knobs.
#[derive(Clone, Debug)]
pub struct FakeConfig {
    pub version: u16,
    pub max_slots: u8,
    pub interrupters: u16,
    pub ports: u8,
    pub context_64: bool,
    pub scratchpads: u16,
    pub ac64: bool,
    /// Port Power Control: ports start unpowered and need PORTSC.PP.
    pub ppc: bool,
    /// The PAGESIZE register: bit n means pages of 2^(n+12) bytes.
    pub page_size: u32,
    pub cap_length: u8,
    pub rtsoff: u32,
    pub dboff: u32,
    /// Where the extended capability list starts, in bytes (0: none).
    pub xecp: usize,
    pub caps: Vec<FakeCap>,
    /// Dword 1 of every Supported Protocol capability.
    pub protocol_name: u32,
    /// Knob: every dword from `xecp` on reads as a capability whose next
    /// one is the following dword.
    pub endless_caps: bool,
    /// Knob: every register reads 0xFFFF_FFFF (powered down or gone).
    pub all_ones: bool,
}

/// Capabilities at `offsets`, each pointing to the next.
fn chain(caps: Vec<(usize, ExtCap)>) -> Vec<FakeCap> {
    let mut out: Vec<FakeCap> = Vec::new();
    for (i, (offset, cap)) in caps.iter().enumerate() {
        let next = caps.get(i + 1).map_or(0, |(o, _)| ((o - offset) / 4) as u8);
        out.push(FakeCap {
            offset: *offset,
            next,
            cap: cap.clone(),
        });
    }
    out
}

impl FakeConfig {
    /// A controller as simple as QEMU's `qemu-xhci` (xHCI 1.00, 32-byte
    /// contexts, no scratchpads, no legacy support), with USB 2 ports 1-4
    /// and USB 3 ports 5-8.
    pub fn basic() -> FakeConfig {
        FakeConfig {
            version: 0x0100,
            max_slots: 64,
            interrupters: 16,
            ports: 8,
            context_64: false,
            scratchpads: 0,
            ac64: true,
            ppc: false,
            page_size: 1,
            cap_length: 0x40,
            rtsoff: 0x1000,
            dboff: 0x2000,
            xecp: 0x20,
            caps: chain(vec![
                (
                    0x20,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 1,
                        count: 4,
                    },
                ),
                (
                    0x30,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0,
                        first: 5,
                        count: 4,
                    },
                ),
            ]),
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
        }
    }

    /// QEMU 8.2's `qemu-xhci` as the e2e scenarios meet it: `basic`, but
    /// with USB 3 on ports 1-4 and USB 2 on ports 5-8 (QEMU lists the USB 2
    /// protocol capability first).
    pub fn qemu() -> FakeConfig {
        FakeConfig {
            caps: chain(vec![
                (
                    0x20,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 5,
                        count: 4,
                    },
                ),
                (
                    0x30,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0,
                        first: 1,
                        count: 4,
                    },
                ),
            ]),
            ..FakeConfig::basic()
        }
    }

    /// The NUC's Alder Lake PCH xHCI (`8086:51ed`): xHCI 1.20, 64-byte
    /// contexts, 2 scratchpads, BIOS-owned legacy support, port power
    /// control, USB 2 ports 1-12 and USB 3 ports 13-16, with unrelated
    /// capabilities between the protocol ones.
    pub fn intel() -> FakeConfig {
        FakeConfig {
            version: 0x0120,
            max_slots: 64,
            interrupters: 8,
            ports: 16,
            context_64: true,
            scratchpads: 2,
            ac64: true,
            ppc: true,
            page_size: 1,
            cap_length: 0x80,
            rtsoff: 0x2000,
            dboff: 0x3000,
            xecp: 0x8000,
            caps: chain(vec![
                (0x8000, ExtCap::Legacy),
                (
                    0x8020,
                    ExtCap::Protocol {
                        major: 2,
                        minor: 0,
                        first: 1,
                        count: 12,
                    },
                ),
                (0x8040, ExtCap::Other(0xC0)),
                (0x8070, ExtCap::Other(0xC1)),
                (
                    0x8080,
                    ExtCap::Protocol {
                        major: 3,
                        minor: 0x10,
                        first: 13,
                        count: 4,
                    },
                ),
                (0x80A0, ExtCap::Other(10)),
            ]),
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
        }
    }
}

/// The fake controller: registers, and (as tasks add them) rings, slots
/// and ports. `FakeHal` routes every register access and every tick of
/// virtual time here.
pub struct FakeXhci {
    config: FakeConfig,
    now: Duration,
    usbcmd: u32,
    usbsts: u32,
    dnctrl: u32,
    crcr: u64,
    dcbaap: u64,
    config_reg: u32,
    portsc: Vec<u32>,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: u64,
    erdp: u64,
    /// USBLEGSUP and USBLEGCTLSTS.
    legacy: [u32; 2],
    cap_reads: usize,
    highest_read: usize,
}

impl FakeXhci {
    pub fn new(config: FakeConfig) -> FakeXhci {
        let ports = config.ports as usize;
        let mut x = FakeXhci {
            config,
            now: Duration::ZERO,
            usbcmd: 0,
            usbsts: regs::HCH,
            dnctrl: 0,
            crcr: 0,
            dcbaap: 0,
            config_reg: 0,
            portsc: vec![0; ports],
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            erdp: 0,
            legacy: [0; 2],
            cap_reads: 0,
            highest_read: 0,
        };
        x.power_on_ports();
        x
    }

    /// Lets the controller act on everything due by `now`.
    pub fn advance_to(&mut self, now: Duration, _dma: &Dma) {
        self.now = now;
    }

    /// Reads of the extended capability area so far.
    pub fn cap_reads(&self) -> usize {
        self.cap_reads
    }

    /// The highest register offset read so far.
    pub fn highest_read(&self) -> usize {
        self.highest_read
    }

    pub fn config(&self) -> &FakeConfig {
        &self.config
    }

    /// Without port power control, ports are always powered (xHCI 5.4.8).
    fn power_on_ports(&mut self) {
        if !self.config.ppc {
            for p in &mut self.portsc {
                *p |= regs::PP;
            }
        }
    }
}
````

- [ ] **Step 4: Create `crates/usb/src/testing/xhci/regs.rs`**

Create `crates/usb/src/testing/xhci/regs.rs`:

````rust
//! The fake's register file: capability registers, the extended
//! capability list, and the operational and runtime registers.

use super::{ExtCap, FakeXhci};
use crate::testing::hal::Dma;

// Operational register offsets and bits, as the fake knows them.
const USBCMD: usize = 0x00;
const USBSTS: usize = 0x04;
const PAGESIZE: usize = 0x08;
const DNCTRL: usize = 0x14;
const CRCR: usize = 0x18;
const DCBAAP: usize = 0x30;
const CONFIG: usize = 0x38;
const PORTS: usize = 0x400;

pub const HCH: u32 = 1 << 0;
/// USBSTS bits software clears by writing 1: HSE, EINT, PCD, SRE.
const USBSTS_RW1C: u32 = 1 << 2 | 1 << 3 | 1 << 4 | 1 << 10;
pub const CRR: u64 = 1 << 3;

pub const PP: u32 = 1 << 9;

// Interrupter 0, from the runtime base.
const IMAN: usize = 0x20;
const IMOD: usize = 0x24;
const ERSTSZ: usize = 0x28;
const ERSTBA: usize = 0x30;
const ERDP: usize = 0x38;

/// Sets the low or high half of a 64-bit register.
fn set_half(reg: &mut u64, high: bool, value: u32) {
    *reg = if high {
        *reg & 0xFFFF_FFFF | (value as u64) << 32
    } else {
        *reg & !0xFFFF_FFFF | value as u64
    };
}

fn half(reg: u64, high: bool) -> u32 {
    if high { (reg >> 32) as u32 } else { reg as u32 }
}

/// Which register block an offset falls in.
enum Block {
    Capability(usize),
    Operational(usize),
    Runtime(usize),
    Doorbell(usize),
    Extended(usize),
}

impl FakeXhci {
    fn block(&self, offset: usize) -> Block {
        let c = &self.config;
        let op = c.cap_length as usize;
        let rt = c.rtsoff as usize;
        let db = c.dboff as usize;
        if offset < 0x20 {
            Block::Capability(offset)
        } else if (op..op + PORTS + 0x10 * c.ports as usize).contains(&offset) {
            Block::Operational(offset - op)
        } else if (rt..rt + 0x20 * (c.interrupters as usize + 1)).contains(&offset) {
            Block::Runtime(offset - rt)
        } else if (db..db + 4 * (c.max_slots as usize + 1)).contains(&offset) {
            Block::Doorbell((offset - db) / 4)
        } else {
            Block::Extended(offset)
        }
    }

    pub fn read(&mut self, offset: usize, _dma: &Dma) -> u32 {
        self.highest_read = self.highest_read.max(offset);
        if self.config.all_ones {
            return 0xFFFF_FFFF;
        }
        match self.block(offset) {
            Block::Capability(o) => self.capability(o),
            Block::Operational(o) => self.op_read(o),
            Block::Runtime(o) => self.runtime_read(o),
            Block::Doorbell(_) => 0,
            Block::Extended(o) => {
                self.cap_reads += 1;
                self.extended_read(o)
            }
        }
    }

    pub fn write(&mut self, offset: usize, value: u32, _dma: &Dma) {
        match self.block(offset) {
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
            Block::Operational(o) => self.op_write(o, value),
            Block::Runtime(o) => self.runtime_write(o, value),
            Block::Doorbell(_) => {}
            Block::Extended(o) => self.extended_write(o, value),
        }
    }

    fn capability(&self, offset: usize) -> u32 {
        let c = &self.config;
        match offset {
            0x00 => c.cap_length as u32 | (c.version as u32) << 16,
            0x04 => c.max_slots as u32 | (c.interrupters as u32) << 8 | (c.ports as u32) << 24,
            // xHCI 5.3.4: Max Scratchpad Buffers high bits 25:21, low 31:27.
            0x08 => {
                let n = c.scratchpads as u32;
                (n >> 5 & 0x1F) << 21 | (n & 0x1F) << 27 | 0xF << 4
            }
            0x10 => {
                c.ac64 as u32
                    | (c.context_64 as u32) << 2
                    | (c.ppc as u32) << 3
                    | ((c.xecp / 4) as u32) << 16
            }
            0x14 => c.dboff,
            0x18 => c.rtsoff,
            _ => 0,
        }
    }

    fn extended_read(&self, offset: usize) -> u32 {
        let c = &self.config;
        if c.endless_caps && c.xecp != 0 && offset >= c.xecp {
            return 0xC0 | 1 << 8;
        }
        let Some(cap) = c
            .caps
            .iter()
            .find(|cap| (cap.offset..cap.offset + 16).contains(&offset))
        else {
            return 0;
        };
        let header = |id: u32| id | (cap.next as u32) << 8;
        match (&cap.cap, (offset - cap.offset) / 4) {
            (ExtCap::Legacy, 0) => header(1) | self.legacy[0],
            (ExtCap::Legacy, 1) => self.legacy[1],
            (ExtCap::Protocol { major, minor, .. }, 0) => {
                header(2) | (*minor as u32) << 16 | (*major as u32) << 24
            }
            (ExtCap::Protocol { .. }, 1) => c.protocol_name,
            (ExtCap::Protocol { first, count, .. }, 2) => *first as u32 | (*count as u32) << 8,
            (ExtCap::Other(id), 0) => header(*id as u32),
            _ => 0,
        }
    }

    fn extended_write(&mut self, offset: usize, value: u32) {
        let legacy = self
            .config
            .caps
            .iter()
            .find(|cap| cap.cap == ExtCap::Legacy)
            .map(|cap| cap.offset);
        match legacy {
            Some(at) if offset == at => self.legacy[0] = value & !0xFFFF,
            Some(at) if offset == at + 4 => self.legacy[1] = value,
            _ => panic!("fake xhci: write to read-only capability space at {offset:#x}"),
        }
    }

    fn op_read(&self, offset: usize) -> u32 {
        match offset {
            USBCMD => self.usbcmd,
            USBSTS => self.usbsts,
            PAGESIZE => self.config.page_size,
            DNCTRL => self.dnctrl,
            // xHCI 5.4.5: only CRR reads back; the pointer reads as 0.
            CRCR => (self.crcr & CRR) as u32,
            0x1C => 0,
            0x30 | 0x34 => half(self.dcbaap, offset == 0x34),
            CONFIG => self.config_reg,
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 { self.portsc[port] } else { 0 }
            }
            _ => 0,
        }
    }

    fn op_write(&mut self, offset: usize, value: u32) {
        match offset {
            USBCMD => self.usbcmd = value,
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
            DNCTRL => self.dnctrl = value,
            0x18 | 0x1C => set_half(&mut self.crcr, offset == 0x1C, value),
            0x30 | 0x34 => set_half(&mut self.dcbaap, offset == 0x34, value),
            CONFIG => self.config_reg = value,
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 {
                    self.portsc[port] = value;
                }
            }
            _ => panic!("fake xhci: write to read-only operational register {offset:#x}"),
        }
    }

    fn runtime_read(&self, offset: usize) -> u32 {
        match offset {
            IMAN => self.iman,
            IMOD => self.imod,
            ERSTSZ => self.erstsz,
            0x30 | 0x34 => half(self.erstba, offset == 0x34),
            0x38 | 0x3C => half(self.erdp, offset == 0x3C),
            _ => 0,
        }
    }

    fn runtime_write(&mut self, offset: usize, value: u32) {
        match offset {
            IMAN => self.iman = value,
            IMOD => self.imod = value,
            ERSTSZ => self.erstsz = value & 0xFFFF,
            0x30 | 0x34 => set_half(&mut self.erstba, offset == 0x34, value),
            0x38 | 0x3C => set_half(&mut self.erdp, offset == 0x3C, value),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;

    #[test]
    fn the_scratchpad_count_is_split_over_two_fields() {
        let mut config = FakeConfig::intel();
        config.scratchpads = 33;
        let x = FakeXhci::new(config);
        assert_eq!(x.capability(0x08) >> 21 & 0x1F, 1);
        assert_eq!(x.capability(0x08) >> 27, 1);
    }

    #[test]
    fn the_legacy_capability_reads_its_id_and_next_pointer() {
        let x = FakeXhci::new(FakeConfig::intel());
        assert_eq!(x.extended_read(0x8000) & 0xFFFF, 0x0801);
        assert_eq!(x.extended_read(0x8080) >> 16, 0x0310);
    }

    #[test]
    #[should_panic(expected = "fake xhci: write to capability register")]
    fn capability_registers_are_read_only() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x04, 0, &Dma::default());
    }
}
````

- [ ] **Step 5: Write the failing tests for `crates/usb/src/xhci/caps.rs`**

Create `crates/usb/src/xhci/caps.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ExtCap, FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn walk(config: FakeConfig) -> (FakeHal, Regs, CapList) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let xecp = super::super::regs::Params::read(&hal, &regs).xecp;
        let list = CapList::walk(&hal, &regs, xecp);
        (hal, regs, list)
    }

    fn map(config: FakeConfig) -> Vec<Option<PortProtocol>> {
        let (hal, regs, list) = walk(config);
        let ports = super::super::regs::Params::read(&hal, &regs).ports;
        port_map(ports, &protocols(&hal, &regs, &list).usable)
    }

    use PortProtocol::{Usb2, Usb3};

    #[test]
    fn basic_has_four_usb2_ports_then_four_usb3_ports() {
        let (hal, regs, list) = walk(FakeConfig::basic());
        assert_eq!(list.problem, None);
        assert_eq!(list.caps.len(), 2);
        let p = protocols(&hal, &regs, &list).usable;
        assert_eq!(
            p,
            [
                Protocol {
                    major: 2,
                    minor: 0,
                    first: 1,
                    count: 4
                },
                Protocol {
                    major: 3,
                    minor: 0,
                    first: 5,
                    count: 4
                },
            ]
        );
        assert_eq!(
            map(FakeConfig::basic()),
            [&[Some(Usb2); 4][..], &[Some(Usb3); 4]].concat()
        );
    }

    #[test]
    fn qemu_lists_usb2_first_but_numbers_usb3_first() {
        assert_eq!(
            map(FakeConfig::qemu()),
            [&[Some(Usb3); 4][..], &[Some(Usb2); 4]].concat()
        );
    }

    #[test]
    fn intel_has_the_legacy_capability_first_and_others_between() {
        let (hal, regs, list) = walk(FakeConfig::intel());
        assert_eq!(list.caps[0].id, LEGACY_SUPPORT);
        assert_eq!(list.find(LEGACY_SUPPORT), Some(list.caps[0].offset));
        assert!(
            list.caps.len() > 4,
            "unrelated capabilities are walked past"
        );
        let p = protocols(&hal, &regs, &list).usable;
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].major, p[1].minor), (3, 0x10));
        assert_eq!(
            map(FakeConfig::intel()),
            [&[Some(Usb2); 12][..], &[Some(Usb3); 4]].concat()
        );
    }

    #[test]
    fn a_port_no_capability_covers_has_no_protocol() {
        let mut config = FakeConfig::basic();
        config
            .caps
            .retain(|c| !matches!(c.cap, ExtCap::Protocol { major: 3, .. }));
        config.caps[0].next = 0;
        let m = map(config);
        assert_eq!(m[3], Some(Usb2));
        assert_eq!(m[4], None);
    }

    #[test]
    fn ports_beyond_the_controller_and_other_protocols_are_ignored() {
        let p = [
            Protocol {
                major: 2,
                minor: 0,
                first: 0,
                count: 2,
            },
            Protocol {
                major: 3,
                minor: 0,
                first: 3,
                count: 200,
            },
            Protocol {
                major: 2,
                minor: 0,
                first: 4,
                count: 1,
            },
            Protocol {
                major: 9,
                minor: 0,
                first: 1,
                count: 1,
            },
        ];
        assert_eq!(port_map(4, &p), [Some(Usb2), None, Some(Usb3), Some(Usb3)]);
    }

    #[test]
    fn a_protocol_capability_naming_no_ports_is_set_aside() {
        for (first, count) in [(0, 0), (5, 0), (0, 4)] {
            let mut config = FakeConfig::basic();
            if let ExtCap::Protocol {
                first: f, count: c, ..
            } = &mut config.caps[1].cap
            {
                (*f, *c) = (first, count);
            }
            let (hal, regs, list) = walk(config);
            let p = protocols(&hal, &regs, &list);
            assert_eq!(p.usable.len(), 1, "first {first} count {count}");
            assert_eq!(
                p.empty,
                [Protocol {
                    major: 3,
                    minor: 0,
                    first,
                    count
                }]
            );
        }
    }

    #[test]
    fn a_protocol_capability_not_named_usb_is_skipped() {
        let mut config = FakeConfig::basic();
        config.protocol_name = u32::from_le_bytes(*b"ABCD");
        let (hal, regs, list) = walk(config);
        assert_eq!(protocols(&hal, &regs, &list), Protocols::default());
    }

    #[test]
    fn a_list_without_end_stops_after_64_entries() {
        let mut config = FakeConfig::intel();
        config.endless_caps = true;
        let (hal, _regs, list) = walk(config);
        assert_eq!(list.caps.len(), 64);
        assert_eq!(list.problem, Some("more than 64 capabilities"));
        assert_eq!(hal.fake().cap_reads(), 64, "one read per entry, no more");
    }

    #[test]
    fn a_list_pointing_outside_the_bar_stops_at_the_edge() {
        let mut config = FakeConfig::intel();
        let last = config.caps.len() - 1;
        let last_offset = config.caps[last].offset;
        config.caps[last].next = 0xFF;
        // The BAR ends 0x100 bytes after the last capability; its next
        // pointer reaches 0x3FC bytes on.
        let end = last_offset + 0x100;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, end).unwrap();
        let regs = Regs::new(&hal, base, end).unwrap();
        let xecp = super::super::regs::Params::read(&hal, &regs).xecp;
        let list = CapList::walk(&hal, &regs, xecp);
        assert_eq!(list.caps.last().map(|c| c.offset), Some(last_offset));
        assert_eq!(list.problem, Some("capability outside the registers"));
        assert!(hal.fake().highest_read() < end);
    }
}
````

- [ ] **Step 6: Add the failing tests and the module declaration to `crates/usb/src/xhci/mod.rs`**

Replace the whole of `crates/usb/src/xhci/mod.rs` with:

````rust
//! The xHCI host controller driver (spec §6.2).
// The driver is built bottom-up: parts land before their users.
#![allow(dead_code)]

mod caps;
mod context;
mod regs;
mod ring;
mod trb;
#[cfg(test)]
mod tests {
    use super::caps::{CapList, PortProtocol, port_map, protocols};
    use super::regs::{Params, Regs};
    use super::*;
    use crate::Hal;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};
    use alloc::string::ToString;

    fn info(config: FakeConfig) -> ControllerInfo {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let params = Params::read(&hal, &regs);
        let list = CapList::walk(&hal, &regs, params.xecp);
        let map = port_map(params.ports, &protocols(&hal, &regs, &list).usable);
        let count = |kind| map.iter().filter(|&&p| p == Some(kind)).count() as u8;
        params.info(count(PortProtocol::Usb2), count(PortProtocol::Usb3))
    }

    #[test]
    fn the_qemu_controller_describes_itself() {
        let i = info(FakeConfig::basic());
        assert_eq!(
            i,
            ControllerInfo {
                version: 0x0100,
                max_slots: 64,
                ports: 8,
                usb2_ports: 4,
                usb3_ports: 4,
                context_size: 32,
                scratchpads: 0,
            }
        );
        assert_eq!(
            i.to_string(),
            "xHCI 1.00, 8 ports (4 USB 2, 4 USB 3), 32-byte contexts, 0 scratchpads"
        );
    }

    #[test]
    fn the_intel_controller_describes_itself() {
        let i = info(FakeConfig::intel());
        assert_eq!((i.usb2_ports, i.usb3_ports, i.scratchpads), (12, 4, 2));
        assert_eq!(
            i.to_string(),
            "xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 64-byte contexts, 2 scratchpads"
        );
    }

    #[test]
    fn versions_print_as_two_bcd_digits_and_one_scratchpad_is_singular() {
        let mut i = info(FakeConfig::basic());
        i.version = 0x0110;
        i.scratchpads = 1;
        assert!(i.to_string().starts_with("xHCI 1.10, "));
        assert!(i.to_string().ends_with(", 1 scratchpad"));
    }
}
````

- [ ] **Step 7: Write the failing tests for `crates/usb/src/xhci/regs.rs`**

Create `crates/usb/src/xhci/regs.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn regs(config: FakeConfig) -> (FakeHal, Regs) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        (hal, regs)
    }

    #[test]
    fn the_qemu_parameters_are_read() {
        let (hal, regs) = regs(FakeConfig::basic());
        let p = Params::read(&hal, &regs);
        assert_eq!((p.version, p.max_slots, p.ports), (0x0100, 64, 8));
        assert_eq!((p.context_size, p.scratchpads), (32, 0));
        assert!(p.ac64 && !p.ppc);
        assert_eq!(p.xecp, 0x20 / 4);
    }

    #[test]
    fn the_intel_parameters_are_read() {
        let (hal, regs) = regs(FakeConfig::intel());
        let p = Params::read(&hal, &regs);
        assert_eq!((p.version, p.max_slots, p.ports), (0x0120, 64, 16));
        assert_eq!((p.context_size, p.scratchpads, p.interrupters), (64, 2, 8));
        assert!(p.ac64 && p.ppc);
    }

    #[test]
    fn scratchpad_counts_use_both_halves_of_the_field() {
        assert_eq!(scratchpads(2 << 27), 2);
        assert_eq!(scratchpads(1 << 21 | 1 << 27), 33);
        assert_eq!(scratchpads(0x1F << 21 | 0x1F << 27), 1023);
        let mut config = FakeConfig::intel();
        config.scratchpads = 33;
        let (hal, regs) = regs(config);
        assert_eq!(Params::read(&hal, &regs).scratchpads, 33);
    }

    #[test]
    fn the_register_blocks_are_found_from_the_capabilities() {
        let (hal, regs) = regs(FakeConfig::intel());
        assert_eq!(regs.op, 0x80);
        assert_eq!(regs.runtime, 0x2000);
        assert_eq!(regs.doorbells, 0x3000);
        // PAGESIZE is operational register 2: bit 0 means 4 KiB.
        assert_eq!(regs.op_read(&hal, PAGESIZE), 1);
    }

    #[test]
    fn registers_that_read_all_ones_mean_the_controller_is_not_there() {
        let mut config = FakeConfig::intel();
        config.all_ones = true;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        assert_eq!(
            Regs::new(&hal, base, FAKE_BAR_LEN).err(),
            Some(UsbError::NotResponding)
        );
    }

    #[test]
    fn register_blocks_outside_the_bar_are_refused() {
        let mut config = FakeConfig::intel();
        config.dboff = 0xFFF0;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        assert_eq!(
            Regs::new(&hal, base, FAKE_BAR_LEN).err(),
            Some(UsbError::Unsupported("registers outside the BAR"))
        );
        let base = hal.map_mmio(FAKE_BAR, 0x1000).unwrap();
        assert!(Regs::new(&hal, base, 0x1000).is_err(), "a BAR cut short");
    }

    #[test]
    fn offsets_from_the_hardware_are_checked_against_the_bar() {
        let (hal, regs) = regs(FakeConfig::basic());
        assert_eq!(regs.try_read(&hal, FAKE_BAR_LEN), None);
        assert_eq!(regs.try_read(&hal, FAKE_BAR_LEN - 2), None);
        assert_eq!(regs.try_read(&hal, 0x21), None, "unaligned");
        assert!(regs.try_read(&hal, FAKE_BAR_LEN - 4).is_some());
    }
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find `Params` in `regs` ``; `` cannot find type `Regs` in this scope ``.

- [ ] **Step 9: Implement `crates/usb/src/xhci/caps.rs`**

Insert this at the top of `crates/usb/src/xhci/caps.rs`, above `#[cfg(test)]`:

````rust
//! The extended capability list (xHCI 7): USB Legacy Support for the BIOS
//! handoff and Supported Protocol, which says which root ports are USB 2
//! and which USB 3. The list comes from the hardware, so the walk is
//! bounded: it cannot loop or read outside the BAR.

use super::regs::Regs;
use crate::Hal;
use alloc::vec;
use alloc::vec::Vec;

pub const LEGACY_SUPPORT: u8 = 1;
pub const SUPPORTED_PROTOCOL: u8 = 2;
/// More entries than any controller has; a longer list is broken.
const MAX_CAPS: usize = 64;
/// "USB " in Supported Protocol dword 1 (xHCI 7.2.2.1.2).
const USB_NAME: u32 = u32::from_le_bytes(*b"USB ");

/// One entry: its offset from the start of the BAR and its ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap {
    pub offset: usize,
    pub id: u8,
}

/// The walked list, and why it ended early if it did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapList {
    pub caps: Vec<Cap>,
    pub problem: Option<&'static str>,
}

impl CapList {
    /// Walks the list starting `xecp` dwords into the BAR (HCCPARAMS1
    /// bits 31:16); each entry's bits 15:8 give the next one's distance in
    /// dwords, 0 ending the list.
    pub fn walk<H: Hal>(hal: &H, regs: &Regs, xecp: u16) -> CapList {
        let mut list = CapList {
            caps: Vec::new(),
            problem: None,
        };
        if xecp == 0 {
            return list;
        }
        let mut offset = xecp as usize * 4;
        loop {
            if list.caps.len() == MAX_CAPS {
                list.problem = Some("more than 64 capabilities");
                break;
            }
            let Some(header) = regs.try_read(hal, offset) else {
                list.problem = Some("capability outside the registers");
                break;
            };
            list.caps.push(Cap {
                offset,
                id: header as u8,
            });
            let next = (header >> 8) as u8 as usize;
            if next == 0 {
                break;
            }
            offset += next * 4;
        }
        list
    }

    /// The first capability with `id`.
    pub fn find(&self, id: u8) -> Option<usize> {
        self.caps.iter().find(|c| c.id == id).map(|c| c.offset)
    }
}

/// A Supported Protocol capability: USB `major.minor` (BCD) on `count`
/// ports from `first`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protocol {
    pub major: u8,
    pub minor: u8,
    pub first: u8,
    pub count: u8,
}

/// The Supported Protocol capabilities named "USB " in the list: those
/// that name ports, and those that name none (count 0 or first port 0),
/// which the caller logs and ignores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Protocols {
    pub usable: Vec<Protocol>,
    pub empty: Vec<Protocol>,
}

pub fn protocols<H: Hal>(hal: &H, regs: &Regs, list: &CapList) -> Protocols {
    let mut found = Protocols::default();
    for cap in list.caps.iter().filter(|c| c.id == SUPPORTED_PROTOCOL) {
        let read = |i: usize| regs.try_read(hal, cap.offset + 4 * i);
        let (Some(d0), Some(name), Some(d2)) = (read(0), read(1), read(2)) else {
            continue;
        };
        if name != USB_NAME {
            continue;
        }
        let p = Protocol {
            major: (d0 >> 24) as u8,
            minor: (d0 >> 16) as u8,
            first: d2 as u8,
            count: (d2 >> 8) as u8,
        };
        if p.first == 0 || p.count == 0 {
            found.empty.push(p);
        } else {
            found.usable.push(p);
        }
    }
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortProtocol {
    Usb2,
    Usb3,
}

/// Which protocol each of `ports` root ports speaks (index 0 is port 1).
/// A port no capability covers stays `None`; where two overlap, the first
/// wins.
pub fn port_map(ports: u8, protocols: &[Protocol]) -> Vec<Option<PortProtocol>> {
    let mut map = vec![None; ports as usize];
    for p in protocols {
        let kind = match p.major {
            2 => PortProtocol::Usb2,
            3 => PortProtocol::Usb3,
            _ => continue,
        };
        let first = p.first as usize;
        for port in first..first + p.count as usize {
            if let Some(slot @ None) = port.checked_sub(1).and_then(|i| map.get_mut(i)) {
                *slot = Some(kind);
            }
        }
    }
    map
}

````

- [ ] **Step 10: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod trb;
#[cfg(test)]
````

with:

````rust
mod trb;

use core::fmt;

/// What the controller reported about itself (for the status line and
/// dmesg).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControllerInfo {
    /// HCIVERSION, BCD: 0x0120 is 1.20.
    pub version: u16,
    pub max_slots: u8,
    pub ports: u8,
    /// Ports the Supported Protocol capabilities name USB 2 and USB 3.
    pub usb2_ports: u8,
    pub usb3_ports: u8,
    /// 32 or 64 bytes (HCCPARAMS1.CSZ).
    pub context_size: usize,
    pub scratchpads: u16,
}

impl fmt::Display for ControllerInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "xHCI {:x}.{:02x}, {} ports ({} USB 2, {} USB 3), {}-byte contexts, {} scratchpad{}",
            self.version >> 8,
            self.version & 0xFF,
            self.ports,
            self.usb2_ports,
            self.usb3_ports,
            self.context_size,
            self.scratchpads,
            if self.scratchpads == 1 { "" } else { "s" },
        )
    }
}

#[cfg(test)]
````

- [ ] **Step 11: Implement `crates/usb/src/xhci/regs.rs`**

Insert this at the top of `crates/usb/src/xhci/regs.rs`, above `#[cfg(test)]`:

````rust
//! Register offsets and bits (xHCI chapter 5) and [`Regs`], which knows
//! where the register blocks are and keeps every access inside the BAR.

use super::ControllerInfo;
use crate::{Hal, UsbError};

/// What a register of a powered-down or absent device reads as.
pub const ALL_ONES: u32 = 0xFFFF_FFFF;

// Capability registers (xHCI 5.3), from the BAR.
/// CAPLENGTH in bits 7:0, HCIVERSION in bits 31:16.
pub const CAPLENGTH: usize = 0x00;
pub const HCSPARAMS1: usize = 0x04;
pub const HCSPARAMS2: usize = 0x08;
pub const HCCPARAMS1: usize = 0x10;
pub const DBOFF: usize = 0x14;
pub const RTSOFF: usize = 0x18;

// HCCPARAMS1 bits.
pub const AC64: u32 = 1 << 0;
pub const CSZ: u32 = 1 << 2;
pub const PPC: u32 = 1 << 3;

// Operational registers (xHCI 5.4), from CAPLENGTH.
pub const USBCMD: usize = 0x00;
pub const USBSTS: usize = 0x04;
pub const PAGESIZE: usize = 0x08;
pub const CRCR: usize = 0x18;
pub const DCBAAP: usize = 0x30;
pub const CONFIG: usize = 0x38;
const PORT_BASE: usize = 0x400;
const PORT_STRIDE: usize = 0x10;

// USBCMD bits.
pub const RUN: u32 = 1 << 0;
pub const HCRST: u32 = 1 << 1;
pub const INTE: u32 = 1 << 2;

// USBSTS bits.
pub const HCH: u32 = 1 << 0;
pub const HSE: u32 = 1 << 2;
pub const CNR: u32 = 1 << 11;
pub const HCE: u32 = 1 << 12;

// CRCR bits.
pub const RCS: u32 = 1 << 0;
pub const CA: u32 = 1 << 2;
pub const CRR: u32 = 1 << 3;

// Interrupter registers (xHCI 5.5.2), from the runtime base; interrupter 0
// only.
const INTERRUPTER_0: usize = 0x20;
pub const IMAN: usize = 0x00;
pub const ERSTSZ: usize = 0x08;
pub const ERSTBA: usize = 0x10;
pub const ERDP: usize = 0x18;
/// IMAN: Interrupt Enable.
pub const IE: u32 = 1 << 1;
/// ERDP: Event Handler Busy (RW1C).
pub const EHB: u64 = 1 << 3;

/// Where the register blocks of one controller are.
#[derive(Debug)]
pub struct Regs {
    base: usize,
    len: usize,
    op: usize,
    runtime: usize,
    doorbells: usize,
}

impl Regs {
    /// Finds the register blocks of the controller mapped at `base` (`len`
    /// bytes) and checks that all of them lie inside the mapping.
    pub fn new<H: Hal>(hal: &H, base: usize, len: usize) -> Result<Regs, UsbError> {
        if len < 0x20 {
            return Err(UsbError::Unsupported("register space too small"));
        }
        let mut regs = Regs {
            base,
            len,
            op: 0,
            runtime: 0,
            doorbells: 0,
        };
        let caplength = regs.read(hal, CAPLENGTH);
        if caplength == ALL_ONES {
            return Err(UsbError::NotResponding);
        }
        let params = regs.read(hal, HCSPARAMS1);
        let slots = params as u8 as usize;
        let ports = (params >> 24) as usize;
        regs.op = (caplength & 0xFF) as usize;
        regs.runtime = (regs.read(hal, RTSOFF) & !0x1F) as usize;
        regs.doorbells = (regs.read(hal, DBOFF) & !0x3) as usize;
        let inside = |start: usize, size: usize| start.checked_add(size).is_some_and(|e| e <= len);
        if regs.op < 0x20
            || !inside(regs.op, PORT_BASE + ports * PORT_STRIDE)
            || !inside(regs.runtime, INTERRUPTER_0 + 0x20)
            || !inside(regs.doorbells, 4 * (slots + 1))
        {
            return Err(UsbError::Unsupported("registers outside the BAR"));
        }
        Ok(regs)
    }

    /// Reads the register at `offset` from the start of the BAR. Offsets
    /// come from this module's constants inside blocks `new` checked.
    pub fn read<H: Hal>(&self, hal: &H, offset: usize) -> u32 {
        assert!(self.inside(offset), "register {offset:#x} outside the BAR");
        // SAFETY: `base` is `len` bytes of registers from `map_mmio`, and
        // the offset is aligned and inside them (asserted).
        unsafe { hal.read32(self.base + offset) }
    }

    pub fn write<H: Hal>(&self, hal: &H, offset: usize, value: u32) {
        assert!(self.inside(offset), "register {offset:#x} outside the BAR");
        // SAFETY: as in `read`.
        unsafe { hal.write32(self.base + offset, value) }
    }

    /// Reads a register at an offset the hardware gave (the capability
    /// list): `None` if it is not inside the BAR.
    pub fn try_read<H: Hal>(&self, hal: &H, offset: usize) -> Option<u32> {
        self.inside(offset).then(|| self.read(hal, offset))
    }

    fn inside(&self, offset: usize) -> bool {
        offset.is_multiple_of(4) && offset.checked_add(4).is_some_and(|e| e <= self.len)
    }

    pub fn op_read<H: Hal>(&self, hal: &H, reg: usize) -> u32 {
        self.read(hal, self.op + reg)
    }

    pub fn op_write<H: Hal>(&self, hal: &H, reg: usize, value: u32) {
        self.write(hal, self.op + reg, value);
    }

    /// A 64-bit register as two dwords, low first (xHCI 5.1).
    pub fn op_write64<H: Hal>(&self, hal: &H, reg: usize, value: u64) {
        self.op_write(hal, reg, value as u32);
        self.op_write(hal, reg + 4, (value >> 32) as u32);
    }

    /// PORTSC of root port `port` (1-based).
    pub fn portsc<H: Hal>(&self, hal: &H, port: u8) -> u32 {
        self.op_read(hal, self.port_reg(port))
    }

    pub fn set_portsc<H: Hal>(&self, hal: &H, port: u8, value: u32) {
        self.op_write(hal, self.port_reg(port), value);
    }

    fn port_reg(&self, port: u8) -> usize {
        assert!(port >= 1, "port 0");
        PORT_BASE + (port as usize - 1) * PORT_STRIDE
    }

    /// A register of the primary interrupter.
    pub fn intr_read<H: Hal>(&self, hal: &H, reg: usize) -> u32 {
        self.read(hal, self.runtime + INTERRUPTER_0 + reg)
    }

    pub fn intr_write<H: Hal>(&self, hal: &H, reg: usize, value: u32) {
        self.write(hal, self.runtime + INTERRUPTER_0 + reg, value);
    }

    pub fn intr_write64<H: Hal>(&self, hal: &H, reg: usize, value: u64) {
        self.intr_write(hal, reg, value as u32);
        self.intr_write(hal, reg + 4, (value >> 32) as u32);
    }

    /// Rings doorbell `slot` (0 is the command ring) for `target` (xHCI
    /// 5.6).
    pub fn ring_doorbell<H: Hal>(&self, hal: &H, slot: u8, target: u32) {
        self.write(hal, self.doorbells + 4 * slot as usize, target);
    }
}

/// What the capability registers say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Params {
    pub version: u16,
    pub max_slots: u8,
    pub interrupters: u16,
    pub ports: u8,
    pub scratchpads: u16,
    pub context_size: usize,
    pub ac64: bool,
    pub ppc: bool,
    /// The extended capability list's offset in dwords (0: none).
    pub xecp: u16,
}

impl Params {
    pub fn read<H: Hal>(hal: &H, regs: &Regs) -> Params {
        let hcs1 = regs.read(hal, HCSPARAMS1);
        let hcc1 = regs.read(hal, HCCPARAMS1);
        Params {
            version: (regs.read(hal, CAPLENGTH) >> 16) as u16,
            max_slots: hcs1 as u8,
            interrupters: (hcs1 >> 8) as u16 & 0x7FF,
            ports: (hcs1 >> 24) as u8,
            scratchpads: scratchpads(regs.read(hal, HCSPARAMS2)),
            context_size: if hcc1 & CSZ != 0 { 64 } else { 32 },
            ac64: hcc1 & AC64 != 0,
            ppc: hcc1 & PPC != 0,
            xecp: (hcc1 >> 16) as u16,
        }
    }

    /// The summary, given how many ports each protocol has.
    pub fn info(&self, usb2_ports: u8, usb3_ports: u8) -> ControllerInfo {
        ControllerInfo {
            version: self.version,
            max_slots: self.max_slots,
            ports: self.ports,
            usb2_ports,
            usb3_ports,
            context_size: self.context_size,
            scratchpads: self.scratchpads,
        }
    }
}

/// Max Scratchpad Buffers from HCSPARAMS2: the high five bits are in bits
/// 25:21, the low five in bits 31:27 (xHCI 5.3.4).
pub fn scratchpads(hcsparams2: u32) -> u16 {
    let high = (hcsparams2 >> 21) & 0x1F;
    let low = (hcsparams2 >> 27) & 0x1F;
    (high << 5 | low) as u16
}

````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 72 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -m "usb: xHCI registers and the extended capability list"
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 10 scenario(s) passed`.

````bash
git push -u origin plan4/usb-core
gh pr create --base main --head plan4/usb-core --title "Plan 4: The usb crate's foundations" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 3–6: the `usb` crate: `Hal` with register access, `DmaBuf`, `UsbError`, `Setup`, `Bus` and descriptor parsing; TRBs and rings with link TRBs and cycle bits; device and input contexts at 32 and 64 bytes; registers and the bounded extended capability walk; the fake HAL and the fake controller's register file.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/usb-core --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-core
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: Starting the xHCI controller (Tasks 7–9)

BIOS handoff, halt and reset, scratchpads and rings, running the controller, commands with timeouts and aborts, and `poll`: everything up to the root ports, tested against a fake controller as strict as hardware.

Branch `plan4/xhci-start`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-start`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/xhci-start /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-start origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-start
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/usb-core` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/xhci-start /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-start plan4/usb-core`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/usb-core>` and re-run `cargo xtask ci` before pushing.

### Task 7: BIOS handoff, halt and reset

Spec §6.2's first two quirks, each its own function. **BIOS handoff:** if the BIOS owns the controller (USB Legacy Support), set the OS-owned semaphore and wait up to 1 s for the BIOS to let go; if it does not, take over anyway as Linux does; then turn off every SMI enable and clear the SMI status, because the NUC firmware's SMM keyboard emulation otherwise keeps interfering. **Halt and reset:** clear Run/Stop and wait for HCHalted, set HCRST, wait 1 ms before touching any register (some Intel hosts hang otherwise; Linux waits too), then wait for both HCRST and CNR to clear; each wait times out after 1 s and logs the register. Registers that read all ones mean the controller is powered down or gone (`NotResponding`): the NUC's Thunderbolt controller may be in that state. The fake controller gets timers, USBCMD/USBSTS behaviour and legacy support, and panics on any access within 1 ms of HCRST or while CNR is set.

**Files:**
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Create: `crates/usb/src/xhci/init.rs`
- Modify: `crates/usb/src/xhci/mod.rs`

**Interfaces:**
- Consumes: `Regs`, `CapList` (Task 6).
- Produces: `usb::xhci::init::{REGISTER_TIMEOUT = 1 s, wait_for(hal, timeout, cond) -> Option<Duration>, bios_handoff(hal, regs, caps, name), halt(hal, regs, name) -> Result<(), UsbError>, reset(hal, regs, name) -> Result<(), UsbError>}`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
    pub all_ones: bool,
}
````

with:

````rust
    pub all_ones: bool,
    /// The legacy support capability says the BIOS owns the controller.
    pub bios_owned: bool,
    /// How long the BIOS takes to let go once asked; `None`: it never does.
    pub bios_release: Option<Duration>,
    /// USBLEGCTLSTS as the BIOS left it.
    pub legacy_ctlsts: u32,
    /// The controller is running when the driver finds it.
    pub running: bool,
    /// How long HCH takes to follow R/S = 0; `None`: it never halts.
    pub halt_time: Option<Duration>,
    /// How long HCRST takes to clear; `None`: it never does.
    pub reset_time: Option<Duration>,
    /// How long CNR stays set after HCRST cleared; `None`: forever.
    pub cnr_time: Option<Duration>,
}
````

Replace:

````rust
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
        }
    }

````

with:

````rust
            protocol_name: u32::from_le_bytes(*b"USB "),
            endless_caps: false,
            all_ones: false,
            bios_owned: false,
            bios_release: None,
            legacy_ctlsts: 0,
            running: false,
            halt_time: Some(Duration::ZERO),
            reset_time: Some(Duration::ZERO),
            cnr_time: Some(Duration::ZERO),
        }
    }

````

Replace:

````rust
            all_ones: false,
        }
    }
}

````

with:

````rust
            all_ones: false,
            bios_owned: true,
            bios_release: Some(Duration::from_millis(5)),
            // SMIs on (bits 0, 4, 13-15) and pending (29-31); bit 8 is
            // reserved and must survive.
            legacy_ctlsts: 0xE000_E111,
            running: true,
            halt_time: Some(Duration::from_millis(1)),
            reset_time: Some(Duration::from_millis(2)),
            cnr_time: Some(Duration::from_millis(10)),
        }
    }
}

/// Something the fake does at a set time.
type Action = Box<dyn FnOnce(&mut FakeXhci, &Dma)>;

````

Replace:

````rust
    highest_read: usize,
}
````

with:

````rust
    highest_read: usize,
    /// Due actions, in the order they were scheduled.
    timers: Vec<(Duration, Action)>,
    /// When HCRST was last written.
    hcrst_at: Option<Duration>,
    /// Writes to the extended capability space.
    cap_writes: usize,
}
````

Replace:

````rust
            highest_read: 0,
        };
        x.power_on_ports();
````

with:

````rust
            highest_read: 0,
            timers: Vec::new(),
            hcrst_at: None,
            cap_writes: 0,
        };
        x.legacy = [
            if x.config.bios_owned {
                regs::BIOS_OWNED
            } else {
                0
            },
            x.config.legacy_ctlsts,
        ];
        if x.config.running {
            x.usbcmd = regs::RUN;
            x.usbsts = 0;
        }
        x.power_on_ports();
````

Replace:

````rust
    /// Lets the controller act on everything due by `now`.
    pub fn advance_to(&mut self, now: Duration, _dma: &Dma) {
        self.now = now;
    }
````

with:

````rust
    /// Lets the controller act on everything due by `now`.
    pub fn advance_to(&mut self, now: Duration, dma: &Dma) {
        while let Some(i) = self.next_due(now) {
            let (when, action) = self.timers.remove(i);
            self.now = self.now.max(when);
            action(self, dma);
        }
        self.now = now;
    }

    /// The earliest action due by `now` (the first scheduled among equals).
    fn next_due(&self, now: Duration) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, (when, _)) in self.timers.iter().enumerate() {
            if *when <= now && best.is_none_or(|b| *when < self.timers[b].0) {
                best = Some(i);
            }
        }
        best
    }

    /// Runs `action` once the clock reaches `when`.
    pub fn at(&mut self, when: Duration, action: impl FnOnce(&mut FakeXhci, &Dma) + 'static) {
        self.timers.push((when, Box::new(action)));
    }

    /// Runs `action` `delay` from now: how tests unplug a device halfway
    /// through a driver call.
    pub fn after(&mut self, delay: Duration, action: impl FnOnce(&mut FakeXhci, &Dma) + 'static) {
        self.at(self.now + delay, action);
    }

    /// The knobs, to turn one while the driver runs (the controller
    /// falling off the bus, say).
    pub fn config_mut(&mut self) -> &mut FakeConfig {
        &mut self.config
    }

    pub fn usbcmd(&self) -> u32 {
        self.usbcmd
    }

    pub fn usbsts(&self) -> u32 {
        self.usbsts
    }

    /// USBLEGSUP and USBLEGCTLSTS.
    pub fn legacy(&self) -> [u32; 2] {
        self.legacy
    }

    pub fn cap_writes(&self) -> usize {
        self.cap_writes
    }
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
use crate::testing::hal::Dma;

````

with:

````rust
use crate::testing::hal::Dma;
use core::time::Duration;

````

Replace:

````rust

pub const HCH: u32 = 1 << 0;
/// USBSTS bits software clears by writing 1: HSE, EINT, PCD, SRE.
````

with:

````rust

pub const RUN: u32 = 1 << 0;
pub const HCRST: u32 = 1 << 1;
pub const HCH: u32 = 1 << 0;
pub const CNR: u32 = 1 << 11;
/// USBSTS bits software clears by writing 1: HSE, EINT, PCD, SRE.
````

Replace:

````rust
pub const PP: u32 = 1 << 9;

````

with:

````rust
pub const PP: u32 = 1 << 9;

// USB Legacy Support (xHCI 7.1).
pub const BIOS_OWNED: u32 = 1 << 16;
const OS_OWNED: u32 = 1 << 24;
/// USBLEGCTLSTS: SMI enables (RW), SMI status (RW1C), and the RsvdP bits
/// software must write back as it read them.
const SMI_ENABLES: u32 = 1 | 1 << 4 | 0x7 << 13;
const SMI_STATUS: u32 = 0x7 << 29;
const LEGCTL_RESERVED: u32 = 0xE | 0xFF << 5;

````

Replace:

````rust

    pub fn read(&mut self, offset: usize, _dma: &Dma) -> u32 {
        self.highest_read = self.highest_read.max(offset);
````

with:

````rust

    /// Linux (xhci_reset): some Intel hosts hang if touched within 1 ms of
    /// HCRST, so the fake refuses it.
    fn check_access(&self, offset: usize) {
        if let Some(t) = self.hcrst_at
            && self.now < t + Duration::from_millis(1)
        {
            panic!(
                "fake xhci: register {offset:#x} accessed {:?} after HCRST",
                self.now - t
            );
        }
    }

    pub fn read(&mut self, offset: usize, _dma: &Dma) -> u32 {
        self.check_access(offset);
        self.highest_read = self.highest_read.max(offset);
````

Replace:

````rust
    pub fn write(&mut self, offset: usize, value: u32, _dma: &Dma) {
        match self.block(offset) {
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
````

with:

````rust
    pub fn write(&mut self, offset: usize, value: u32, _dma: &Dma) {
        self.check_access(offset);
        let block = self.block(offset);
        // xHCI 5.4.2: no operational or runtime register may be written
        // until the controller is ready.
        if self.usbsts & CNR != 0 && !matches!(block, Block::Extended(_)) {
            panic!("fake xhci: register {offset:#x} written while USBSTS.CNR is 1");
        }
        match block {
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
````

Replace:

````rust
    fn extended_write(&mut self, offset: usize, value: u32) {
        let legacy = self
````

with:

````rust
    fn extended_write(&mut self, offset: usize, value: u32) {
        self.cap_writes += 1;
        let legacy = self
````

Replace:

````rust
        match legacy {
            Some(at) if offset == at => self.legacy[0] = value & !0xFFFF,
            Some(at) if offset == at + 4 => self.legacy[1] = value,
            _ => panic!("fake xhci: write to read-only capability space at {offset:#x}"),
        }
````

with:

````rust
        match legacy {
            Some(at) if offset == at => self.write_usblegsup(value),
            Some(at) if offset == at + 4 => self.write_usblegctlsts(value),
            _ => panic!("fake xhci: write to read-only capability space at {offset:#x}"),
        }
    }

    /// The OS asking for the controller starts the BIOS's release.
    fn write_usblegsup(&mut self, value: u32) {
        let asked = value & OS_OWNED != 0 && self.legacy[0] & OS_OWNED == 0;
        self.legacy[0] = value & (BIOS_OWNED | OS_OWNED);
        if asked
            && value & BIOS_OWNED != 0
            && let Some(delay) = self.config.bios_release
        {
            self.after(delay, |x, _| x.legacy[0] &= !BIOS_OWNED);
        }
    }

    fn write_usblegctlsts(&mut self, value: u32) {
        let old = self.legacy[1];
        if (old ^ value) & LEGCTL_RESERVED != 0 {
            panic!("fake xhci: USBLEGCTLSTS reserved bits changed ({old:#010x} -> {value:#010x})");
        }
        let status = old & SMI_STATUS & !value;
        self.legacy[1] = old & !(SMI_ENABLES | SMI_STATUS) | value & SMI_ENABLES | status;
    }

    fn write_usbcmd(&mut self, value: u32) {
        let was = self.usbcmd;
        if value & HCRST != 0 {
            if self.usbsts & HCH == 0 {
                panic!("fake xhci: HCRST set while the controller runs");
            }
            self.reset();
            return;
        }
        self.usbcmd = value;
        if was & RUN != 0 && value & RUN == 0 {
            if let Some(delay) = self.config.halt_time {
                self.after(delay, |x, _| x.usbsts |= HCH);
            }
        } else if was & RUN == 0 && value & RUN != 0 {
            self.usbsts &= !HCH;
        }
    }

    /// HCRST (xHCI 4.22.1): registers to their defaults, CNR set, HCRST
    /// and then CNR clearing after their delays.
    fn reset(&mut self) {
        self.usbcmd = HCRST;
        self.usbsts = HCH | CNR;
        self.dnctrl = 0;
        self.crcr = 0;
        self.dcbaap = 0;
        self.config_reg = 0;
        self.portsc.iter_mut().for_each(|p| *p = 0);
        self.power_on_ports();
        (self.iman, self.imod, self.erstsz, self.erstba, self.erdp) = (0, 0, 0, 0, 0);
        self.hcrst_at = Some(self.now);
        if let Some(delay) = self.config.reset_time {
            self.after(delay, |x, _| {
                x.usbcmd &= !HCRST;
                if let Some(cnr) = x.config.cnr_time {
                    x.after(cnr, |x, _| x.usbsts &= !CNR);
                }
            });
        }
````

Replace:

````rust
        match offset {
            USBCMD => self.usbcmd = value,
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
````

with:

````rust
        match offset {
            USBCMD => self.write_usbcmd(value),
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
````

Replace:

````rust
    }

    #[test]
    #[should_panic(expected = "fake xhci: write to capability register")]
````

with:

````rust
    }

    fn intel() -> FakeXhci {
        FakeXhci::new(FakeConfig::intel())
    }

    #[test]
    fn hcrst_takes_its_time_and_cnr_follows() {
        let dma = Dma::default();
        let mut x = intel();
        x.write(0x80, 0, &dma);
        x.advance_to(Duration::from_millis(1), &dma);
        assert_eq!(x.usbsts & HCH, HCH);
        x.write(0x80, HCRST, &dma);
        x.advance_to(Duration::from_millis(4), &dma);
        assert_eq!((x.usbcmd & HCRST, x.usbsts & CNR), (0, CNR));
        x.advance_to(Duration::from_millis(13), &dma);
        assert_eq!(x.usbsts & CNR, 0);
    }

    #[test]
    #[should_panic(expected = "after HCRST")]
    fn a_register_touched_right_after_hcrst_panics() {
        let dma = Dma::default();
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x40, HCRST, &dma);
        x.advance_to(Duration::from_micros(900), &dma);
        x.read(0x44, &dma);
    }

    #[test]
    #[should_panic(expected = "while USBSTS.CNR is 1")]
    fn writes_before_the_controller_is_ready_panic() {
        let dma = Dma::default();
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.config.reset_time = None;
        x.write(0x40, HCRST, &dma);
        x.advance_to(Duration::from_millis(2), &dma);
        x.write(0x40 + 0x38, 1, &dma);
    }

    #[test]
    #[should_panic(expected = "HCRST set while the controller runs")]
    fn a_reset_without_a_halt_panics() {
        intel().write(0x80, HCRST, &Dma::default());
    }

    #[test]
    fn the_bios_lets_go_after_its_delay() {
        let dma = Dma::default();
        let mut x = intel();
        x.write(0x8000, BIOS_OWNED | OS_OWNED, &dma);
        x.advance_to(Duration::from_millis(4), &dma);
        assert_eq!(x.legacy[0] & BIOS_OWNED, BIOS_OWNED);
        x.advance_to(Duration::from_millis(5), &dma);
        assert_eq!(x.legacy[0], OS_OWNED);
    }

    #[test]
    #[should_panic(expected = "reserved bits changed")]
    fn legacy_control_reserved_bits_must_be_kept() {
        intel().write(0x8004, 0, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "fake xhci: write to capability register")]
````

- [ ] **Step 3: Write the failing tests for `crates/usb/src/xhci/init.rs`**

Create `crates/usb/src/xhci/init.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::regs::{CONFIG, Params};
    use super::*;
    use crate::testing::{FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn setup(config: FakeConfig) -> (FakeHal, Regs, CapList) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let caps = CapList::walk(&hal, &regs, Params::read(&hal, &regs).xecp);
        (hal, regs, caps)
    }

    #[test]
    fn the_bios_hands_over_after_5_ms_and_its_smis_are_turned_off() {
        let (hal, regs, caps) = setup(FakeConfig::intel());
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        let [sup, ctl] = hal.fake().legacy();
        assert_eq!(sup & (BIOS_OWNED | OS_OWNED), OS_OWNED);
        assert_eq!(ctl & SMI_ENABLES, 0, "SMIs off");
        assert_eq!(ctl & SMI_STATUS, 0, "SMI status cleared");
        assert_eq!(ctl, 0x0000_0100, "the reserved bit is kept");
        let log = hal.log_text();
        assert_eq!(
            log,
            "xhci 00:14.0: legacy support at 0x8000: BIOS owned, handed over after 5 ms; \
             SMI enables 0x0000e011 cleared"
        );
    }

    #[test]
    fn a_bios_that_never_lets_go_is_taken_over_after_a_second() {
        let mut config = FakeConfig::intel();
        config.bios_release = None;
        let (hal, regs, caps) = setup(config);
        let start = hal.clock();
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        assert!(hal.clock() - start >= REGISTER_TIMEOUT);
        assert!(hal.clock() - start < REGISTER_TIMEOUT + Duration::from_millis(5));
        let [sup, ctl] = hal.fake().legacy();
        assert_eq!(sup & (BIOS_OWNED | OS_OWNED), OS_OWNED);
        assert_eq!(ctl & (SMI_ENABLES | SMI_STATUS), 0);
        assert!(
            hal.log_text()
                .contains("did not let go in 1000 ms, taken over; SMI")
        );
    }

    #[test]
    fn a_controller_the_bios_does_not_own_is_claimed_at_once() {
        let mut config = FakeConfig::intel();
        config.bios_owned = false;
        let (hal, regs, caps) = setup(config);
        bios_handoff(&hal, &regs, &caps, "00:14.0");
        assert_eq!(hal.fake().legacy()[0], OS_OWNED);
        assert!(hal.clock() < Duration::from_millis(1));
        assert!(
            hal.log_text()
                .contains("legacy support at 0x8000: not BIOS owned")
        );
    }

    #[test]
    fn without_legacy_support_nothing_is_written() {
        let (hal, regs, caps) = setup(FakeConfig::basic());
        bios_handoff(&hal, &regs, &caps, "00:04.0");
        assert_eq!(hal.fake().cap_writes(), 0);
        assert_eq!(hal.log_text(), "xhci 00:04.0: no legacy support capability");
    }

    #[test]
    fn a_running_controller_is_halted_first() {
        let (hal, regs, _) = setup(FakeConfig::intel());
        assert_eq!(hal.fake().usbsts() & HCH, 0);
        halt(&hal, &regs, "00:14.0").unwrap();
        assert_eq!(hal.fake().usbcmd() & RUN, 0);
        assert_eq!(hal.fake().usbsts() & HCH, HCH);
        assert!(hal.log_text().contains("xhci 00:14.0: halted after 1 ms"));
    }

    #[test]
    fn a_halted_controller_is_left_alone() {
        let (hal, regs, _) = setup(FakeConfig::basic());
        halt(&hal, &regs, "q").unwrap();
        assert!(hal.log_text().contains("already halted"));
    }

    #[test]
    fn reset_waits_for_hcrst_and_then_for_cnr() {
        let mut config = FakeConfig::intel();
        config.running = false;
        config.reset_time = Some(Duration::from_millis(3));
        config.cnr_time = Some(Duration::from_millis(5));
        let (hal, regs, _) = setup(config);
        reset(&hal, &regs, "00:14.0").unwrap();
        assert_eq!(hal.fake().usbsts() & CNR, 0);
        assert_eq!(hal.fake().usbcmd() & HCRST, 0);
        // The fake panics on a write while CNR is 1.
        regs.op_write(&hal, CONFIG, 1);
        assert!(hal.clock() >= Duration::from_millis(8));
        assert!(hal.log_text().contains("xhci 00:14.0: reset after 8 ms"));
    }

    #[test]
    fn nothing_is_touched_in_the_millisecond_after_hcrst() {
        // The fake panics on any access within 1 ms of HCRST; QEMU's reset
        // is instant, so only that rule makes the driver wait.
        let (hal, regs, _) = setup(FakeConfig::basic());
        reset(&hal, &regs, "q").unwrap();
        assert!(hal.clock() >= Duration::from_millis(1));
    }

    #[test]
    fn a_halt_that_never_comes_times_out_naming_hch() {
        let mut config = FakeConfig::intel();
        config.halt_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(halt(&hal, &regs, "00:14.0"), Err(UsbError::Timeout));
        assert!(hal.clock() >= REGISTER_TIMEOUT);
        assert!(
            hal.log_text()
                .contains("USBSTS.HCH still 0 1000 ms after R/S = 0")
        );
    }

    #[test]
    fn a_reset_that_never_ends_times_out_naming_hcrst() {
        let mut config = FakeConfig::basic();
        config.reset_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(reset(&hal, &regs, "q"), Err(UsbError::Timeout));
        assert!(
            hal.log_text()
                .contains("USBCMD.HCRST still 1 1000 ms after reset")
        );
    }

    #[test]
    fn a_controller_never_ready_times_out_naming_cnr() {
        let mut config = FakeConfig::basic();
        config.cnr_time = None;
        let (hal, regs, _) = setup(config);
        assert_eq!(reset(&hal, &regs, "q"), Err(UsbError::Timeout));
        assert!(hal.clock() >= REGISTER_TIMEOUT);
        assert!(
            hal.log_text()
                .contains("USBSTS.CNR still 1 1000 ms after reset")
        );
    }

    #[test]
    fn a_controller_reading_all_ones_is_not_responding() {
        let (hal, regs, _) = setup(FakeConfig::intel());
        hal.fake().config_mut().all_ones = true;
        assert_eq!(halt(&hal, &regs, "00:0d.0"), Err(UsbError::NotResponding));
        assert!(
            hal.log_text()
                .contains("xhci 00:0d.0: USBSTS reads 0xffffffff")
        );
    }
}
````

- [ ] **Step 4: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod context;
mod regs;
````

with:

````rust
mod context;
mod init;
mod regs;
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Regs` in this scope ``; `` cannot find type `CapList` in this scope ``.

- [ ] **Step 6: Implement `crates/usb/src/xhci/init.rs`**

Insert this at the top of `crates/usb/src/xhci/init.rs`, above `#[cfg(test)]`:

````rust
//! Bringing the controller up (spec §6.2): taking it over from the BIOS,
//! halting and resetting it. Each hardware quirk is its own function.

use super::caps::{CapList, LEGACY_SUPPORT};
use super::regs::{ALL_ONES, CNR, HCH, HCRST, RUN, Regs, USBCMD, USBSTS};
use crate::{Hal, UsbError};
use core::fmt;
use core::time::Duration;

/// Every register wait (spec §6.2).
pub const REGISTER_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a register wait looks again.
const POLL_INTERVAL: Duration = Duration::from_micros(100);
/// Linux (xhci_reset) waits this long after HCRST before touching any
/// register: some Intel hosts hang otherwise.
const HCRST_SETTLE: Duration = Duration::from_millis(1);

// USBLEGSUP (xHCI 7.1.1).
const BIOS_OWNED: u32 = 1 << 16;
const OS_OWNED: u32 = 1 << 24;
// USBLEGCTLSTS (xHCI 7.1.2): the SMI enables, and the SMI status bits,
// which are cleared by writing 1.
const LEGCTLSTS: usize = 4;
const SMI_ENABLES: u32 = 1 | 1 << 4 | 1 << 13 | 1 << 14 | 1 << 15;
const SMI_STATUS: u32 = 0x7 << 29;

/// Polls `done` until it holds, sleeping in between, for up to `timeout`.
/// Returns how long it took, or `None` on a timeout.
pub fn wait_for<H: Hal>(
    hal: &H,
    timeout: Duration,
    mut done: impl FnMut() -> bool,
) -> Option<Duration> {
    let start = hal.now();
    loop {
        if done() {
            return Some(hal.now() - start);
        }
        if hal.now() - start >= timeout {
            return None;
        }
        hal.sleep(POLL_INTERVAL);
    }
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

/// How the BIOS handoff went.
enum Handoff {
    NotBiosOwned,
    HandedOver(Duration),
    TakenOver,
}

impl fmt::Display for Handoff {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Handoff::NotBiosOwned => write!(f, "not BIOS owned"),
            Handoff::HandedOver(t) => write!(f, "BIOS owned, handed over after {} ms", ms(*t)),
            Handoff::TakenOver => write!(
                f,
                "BIOS owned, did not let go in {} ms, taken over",
                ms(REGISTER_TIMEOUT)
            ),
        }
    }
}

/// Takes the controller from the BIOS (xHCI 4.22.1): asks for it through
/// USB Legacy Support, waits up to 1 s, takes it anyway if the BIOS does
/// not let go (as Linux does), then turns the BIOS's SMIs off. Without
/// this the NUC firmware's SMM keyboard emulation keeps interfering.
pub fn bios_handoff<H: Hal>(hal: &H, regs: &Regs, caps: &CapList, name: &str) {
    let Some(at) = caps.find(LEGACY_SUPPORT) else {
        xlog!(hal, name, "no legacy support capability");
        return;
    };
    let sup = regs.read(hal, at);
    regs.write(hal, at, sup | OS_OWNED);
    let outcome = if sup & BIOS_OWNED == 0 {
        Handoff::NotBiosOwned
    } else if let Some(t) = wait_for(hal, REGISTER_TIMEOUT, || {
        regs.read(hal, at) & BIOS_OWNED == 0
    }) {
        Handoff::HandedOver(t)
    } else {
        let sup = regs.read(hal, at);
        regs.write(hal, at, sup & !BIOS_OWNED | OS_OWNED);
        Handoff::TakenOver
    };
    let ctl = regs.read(hal, at + LEGCTLSTS);
    // Reserved bits are written back as read; status bits are RW1C.
    regs.write(hal, at + LEGCTLSTS, ctl & !SMI_ENABLES | SMI_STATUS);
    xlog!(
        hal,
        name,
        "legacy support at {at:#x}: {outcome}; SMI enables {:#010x} cleared",
        ctl & SMI_ENABLES
    );
}

/// Stops a running controller: clears R/S and waits up to 1 s for HCH
/// (xHCI 4.22.1: HCRST is only allowed on a halted controller).
pub fn halt<H: Hal>(hal: &H, regs: &Regs, name: &str) -> Result<(), UsbError> {
    let sts = regs.op_read(hal, USBSTS);
    if sts == ALL_ONES {
        xlog!(
            hal,
            name,
            "USBSTS reads {ALL_ONES:#x}: controller powered down or gone"
        );
        return Err(UsbError::NotResponding);
    }
    if sts & HCH != 0 {
        xlog!(hal, name, "already halted");
        return Ok(());
    }
    let cmd = regs.op_read(hal, USBCMD);
    regs.op_write(hal, USBCMD, cmd & !RUN);
    match wait_for(hal, REGISTER_TIMEOUT, || {
        regs.op_read(hal, USBSTS) & HCH != 0
    }) {
        Some(t) => {
            xlog!(hal, name, "halted after {} ms", ms(t));
            Ok(())
        }
        None => {
            xlog!(
                hal,
                name,
                "USBSTS.HCH still 0 {} ms after R/S = 0",
                ms(REGISTER_TIMEOUT)
            );
            Err(UsbError::Timeout)
        }
    }
}

/// Resets a halted controller: HCRST, 1 ms without touching it, then up to
/// 1 s for HCRST and USBSTS.CNR to read 0 (no register may be written
/// while CNR is 1, xHCI 5.4.2).
pub fn reset<H: Hal>(hal: &H, regs: &Regs, name: &str) -> Result<(), UsbError> {
    let start = hal.now();
    regs.op_write(hal, USBCMD, HCRST);
    hal.sleep(HCRST_SETTLE);
    let ready = || regs.op_read(hal, USBCMD) & HCRST == 0 && regs.op_read(hal, USBSTS) & CNR == 0;
    if wait_for(hal, REGISTER_TIMEOUT, ready).is_some() {
        xlog!(hal, name, "reset after {} ms", ms(hal.now() - start));
        return Ok(());
    }
    let stuck = if regs.op_read(hal, USBCMD) & HCRST != 0 {
        "USBCMD.HCRST"
    } else {
        "USBSTS.CNR"
    };
    xlog!(
        hal,
        name,
        "{stuck} still 1 {} ms after reset",
        ms(REGISTER_TIMEOUT)
    );
    Err(UsbError::Timeout)
}

````

- [ ] **Step 7: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
#![allow(dead_code)]

````

with:

````rust
#![allow(dead_code)]

/// Logs one line starting "xhci <name>: ", as every line of this driver
/// does (spec §13: the NUC is debugged from a photo of `dmesg`).
macro_rules! xlog {
    ($hal:expr, $name:expr, $($arg:tt)*) => {
        $crate::Hal::log($hal, format_args!("xhci {}: {}", $name, format_args!($($arg)*)))
    };
}

````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 90 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -m "usb: xHCI BIOS handoff, halt and reset"
````


### Task 8: Starting the controller

`Xhci::new` (spec §6.2): map the BAR, refuse what this driver cannot drive (no 64-bit DMA, since the frame allocator hands out memory above 4 GiB; a page size other than 4 KiB), hand over from the BIOS, halt, reset, then allocate what the controller reads: the scratchpad buffers (Intel needs them, QEMU none) with their array in DCBAA[0], the DCBAA, the command ring and the event ring. Programming follows xHCI 4.2: MaxSlotsEn, DCBAAP, CRCR with the ring cycle state, ERSTSZ, ERDP, then ERSTBA last, interrupts off (milestone 1 polls), Run/Stop, and power for every port when the controller has port power control. Then it waits until 100 ms after the ports were reset or powered, the time a USB 2 device may take to signal attach (TSIGATT), so that devices present at boot are seen by the first scan. Any failure frees everything allocated so far. The log gets the facts a NUC photo depends on: the context size, the scratchpad count and the handoff outcome.

**Files:**
- Modify: `crates/usb/src/testing/hal.rs`
- Modify: `crates/usb/src/testing/mod.rs`
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Create: `crates/usb/src/testing/xhci/rings.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/regs.rs`
- Create: `crates/usb/src/xhci/start.rs`

**Interfaces:**
- Consumes: Tasks 4–7.
- Produces: `usb::xhci::Xhci<H: Hal>` with `Xhci::{new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError>, info(&self) -> &ControllerInfo, name(&self) -> &str, hal(&self) -> &H}`; `xhci::start::Scratchpads`; test support `testing::start(FakeConfig) -> (FakeHal, Xhci<FakeHal>)` (the driver started on the fake as "00:14.0"), `FakeHal::{fail_alloc_after, fail_one_alloc}`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/hal.rs`**

In `crates/usb/src/testing/hal.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    allocs_left: Option<usize>,
}
````

with:

````rust
    allocs_left: Option<usize>,
    /// The one allocation (counting from 0) that fails, if any.
    failing: Option<usize>,
}
````

Replace:

````rust

    /// Every log line so far, one per line.
````

with:

````rust

    /// Fails only the allocation `n` from now (0 is the next one).
    pub fn fail_one_alloc(&self, n: usize) {
        self.0.dma.borrow_mut().failing = Some(n);
    }

    /// Every log line so far, one per line.
````

Replace:

````rust
        let mut dma = self.0.dma.borrow_mut();
        if let Some(left) = dma.allocs_left.as_mut() {
````

with:

````rust
        let mut dma = self.0.dma.borrow_mut();
        if let Some(n) = dma.failing.as_mut() {
            if *n == 0 {
                dma.failing = None;
                return None;
            }
            *n -= 1;
        }
        if let Some(left) = dma.allocs_left.as_mut() {
````

Replace:

````rust
        assert!(hal.alloc_dma(64, 64).is_none());
        hal.free_dma(a);
    }
````

with:

````rust
        assert!(hal.alloc_dma(64, 64).is_none());
        assert!(hal.alloc_dma(64, 64).is_none());
        hal.free_dma(a);
        let hal = FakeHal::new();
        hal.fail_one_alloc(1);
        let a = hal.alloc_dma(64, 64).unwrap();
        assert!(hal.alloc_dma(64, 64).is_none());
        let b = hal.alloc_dma(64, 64).unwrap();
        hal.free_dma(a);
        hal.free_dma(b);
    }
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/mod.rs`**

Replace the whole of `crates/usb/src/testing/mod.rs` with:

````rust
//! Test support: a fake `Hal` with virtual time and checked DMA memory,
//! and the fake xHCI controller behind its registers.
// Helpers serve tests across the crate; not every build uses all of them.
#![allow(dead_code)]

mod hal;
mod xhci;

pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
pub use xhci::{ExtCap, FakeConfig};

use crate::xhci::Xhci;

/// A fake machine with a controller made from `config`, and the driver
/// brought up on it as "00:14.0".
pub fn start(config: FakeConfig) -> (FakeHal, Xhci<FakeHal>) {
    let hal = FakeHal::with_controller(config);
    let xhci = Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0")
        .unwrap_or_else(|e| panic!("the controller did not start: {e}\n{}", hal.log_text()));
    (hal, xhci)
}
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
mod regs;

````

with:

````rust
mod regs;
mod rings;

````

Replace:

````rust
    pub cnr_time: Option<Duration>,
}
````

with:

````rust
    pub cnr_time: Option<Duration>,
    /// How long HCH takes to clear after R/S = 1; `None`: it never runs.
    pub run_time: Option<Duration>,
}
````

Replace:

````rust
            cnr_time: Some(Duration::ZERO),
        }
````

with:

````rust
            cnr_time: Some(Duration::ZERO),
            run_time: Some(Duration::ZERO),
        }
````

Replace:

````rust
            cnr_time: Some(Duration::from_millis(10)),
        }
````

with:

````rust
            cnr_time: Some(Duration::from_millis(10)),
            run_time: Some(Duration::from_micros(500)),
        }
````

Replace:

````rust
type Action = Box<dyn FnOnce(&mut FakeXhci, &Dma)>;

````

with:

````rust
type Action = Box<dyn FnOnce(&mut FakeXhci, &Dma)>;

/// A ring the fake consumes: the command ring or a transfer ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Consumer {
    pub dequeue: u64,
    /// The Consumer Cycle State.
    pub cycle: bool,
}

/// The event ring the fake produces into (one segment).
#[derive(Clone, Copy, Debug)]
pub struct EventRing {
    pub base: u64,
    pub size: usize,
    pub enqueue: usize,
    pub cycle: bool,
}

````

Replace:

````rust
    dnctrl: u32,
    crcr: u64,
    dcbaap: u64,
    config_reg: u32,
    portsc: Vec<u32>,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: u64,
    erdp: u64,
    /// USBLEGSUP and USBLEGCTLSTS.
````

with:

````rust
    dnctrl: u32,
    /// CRCR as written; the ring it names once the high half is written.
    crcr: u64,
    command_ring: Option<Consumer>,
    /// Command Ring Running.
    crr: bool,
    dcbaap: u64,
    config_reg: u32,
    portsc: Vec<u32>,
    port_writes: usize,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: u64,
    event_ring: Option<EventRing>,
    /// ERDP without its flag bits, and Event Handler Busy.
    erdp: u64,
    ehb: bool,
    /// The scratchpad pages DCBAA[0] named when the controller started.
    scratchpad_pages: Vec<u64>,
    /// When a port was last powered on.
    powered_at: Option<Duration>,
    /// USBLEGSUP and USBLEGCTLSTS.
````

Replace:

````rust
            crcr: 0,
            dcbaap: 0,
            config_reg: 0,
            portsc: vec![0; ports],
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            erdp: 0,
            legacy: [0; 2],
````

with:

````rust
            crcr: 0,
            command_ring: None,
            crr: false,
            dcbaap: 0,
            config_reg: 0,
            portsc: vec![0; ports],
            port_writes: 0,
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            event_ring: None,
            erdp: 0,
            ehb: false,
            scratchpad_pages: Vec::new(),
            powered_at: None,
            legacy: [0; 2],
````

Replace:

````rust

    /// Reads of the extended capability area so far.
````

with:

````rust

    /// Whether R/S is set and the controller has left the halted state.
    pub fn running(&self) -> bool {
        self.usbsts & regs::HCH == 0
    }

    pub fn dcbaap(&self) -> u64 {
        self.dcbaap
    }

    pub fn iman(&self) -> u32 {
        self.iman
    }

    pub fn event_ring(&self) -> Option<EventRing> {
        self.event_ring
    }

    pub fn command_ring(&self) -> Option<Consumer> {
        self.command_ring
    }

    pub fn scratchpad_pages(&self) -> &[u64] {
        &self.scratchpad_pages
    }

    pub fn portsc(&self, port: u8) -> u32 {
        self.portsc[port as usize - 1]
    }

    /// PORTSC writes so far.
    pub fn port_writes(&self) -> usize {
        self.port_writes
    }

    /// When software last powered a port on.
    pub fn powered_at(&self) -> Option<Duration> {
        self.powered_at
    }

    /// Reads of the extended capability area so far.
````

- [ ] **Step 4: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
const USBSTS_RW1C: u32 = 1 << 2 | 1 << 3 | 1 << 4 | 1 << 10;
pub const CRR: u64 = 1 << 3;

````

with:

````rust
const USBSTS_RW1C: u32 = 1 << 2 | 1 << 3 | 1 << 4 | 1 << 10;
pub const CRR: u32 = 1 << 3;

````

Replace:

````rust
const ERDP: usize = 0x38;

/// Sets the low or high half of a 64-bit register.
fn set_half(reg: &mut u64, high: bool, value: u32) {
    *reg = if high {
````

with:

````rust
const ERDP: usize = 0x38;
pub const EHB: u32 = 1 << 3;

/// Sets the low or high half of a 64-bit register.
pub fn set_half(reg: &mut u64, high: bool, value: u32) {
    *reg = if high {
````

Replace:

````rust

    pub fn write(&mut self, offset: usize, value: u32, _dma: &Dma) {
        self.check_access(offset);
````

with:

````rust

    pub fn write(&mut self, offset: usize, value: u32, dma: &Dma) {
        self.check_access(offset);
````

Replace:

````rust
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
            Block::Operational(o) => self.op_write(o, value),
            Block::Runtime(o) => self.runtime_write(o, value),
            Block::Doorbell(_) => {}
````

with:

````rust
            Block::Capability(o) => panic!("fake xhci: write to capability register {o:#x}"),
            Block::Operational(o) => self.op_write(o, value, dma),
            Block::Runtime(o) => self.runtime_write(o, value, dma),
            Block::Doorbell(_) => {}
````

Replace:

````rust

    fn write_usbcmd(&mut self, value: u32) {
        let was = self.usbcmd;
````

with:

````rust

    fn write_usbcmd(&mut self, value: u32, dma: &Dma) {
        let was = self.usbcmd;
````

Replace:

````rust
        } else if was & RUN == 0 && value & RUN != 0 {
            self.usbsts &= !HCH;
        }
````

with:

````rust
        } else if was & RUN == 0 && value & RUN != 0 {
            self.start(dma);
        }
````

Replace:

````rust
        self.dnctrl = 0;
        self.crcr = 0;
        self.dcbaap = 0;
        self.config_reg = 0;
        self.portsc.iter_mut().for_each(|p| *p = 0);
        self.power_on_ports();
        (self.iman, self.imod, self.erstsz, self.erstba, self.erdp) = (0, 0, 0, 0, 0);
        self.hcrst_at = Some(self.now);
````

with:

````rust
        self.dnctrl = 0;
        (self.crcr, self.command_ring, self.crr) = (0, None, false);
        self.dcbaap = 0;
        self.config_reg = 0;
        self.portsc.iter_mut().for_each(|p| *p = 0);
        self.power_on_ports();
        (self.iman, self.imod, self.erstsz, self.erstba) = (0, 0, 0, 0);
        (self.event_ring, self.erdp, self.ehb) = (None, 0, false);
        self.hcrst_at = Some(self.now);
````

Replace:

````rust
            // xHCI 5.4.5: only CRR reads back; the pointer reads as 0.
            CRCR => (self.crcr & CRR) as u32,
            0x1C => 0,
````

with:

````rust
            // xHCI 5.4.5: only CRR reads back; the pointer reads as 0.
            CRCR => {
                if self.crr {
                    CRR
                } else {
                    0
                }
            }
            0x1C => 0,
````

Replace:

````rust

    fn op_write(&mut self, offset: usize, value: u32) {
        match offset {
            USBCMD => self.write_usbcmd(value),
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
            DNCTRL => self.dnctrl = value,
            0x18 | 0x1C => set_half(&mut self.crcr, offset == 0x1C, value),
            0x30 | 0x34 => set_half(&mut self.dcbaap, offset == 0x34, value),
            CONFIG => self.config_reg = value,
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 {
                    self.portsc[port] = value;
                }
````

with:

````rust

    fn op_write(&mut self, offset: usize, value: u32, dma: &Dma) {
        match offset {
            USBCMD => self.write_usbcmd(value, dma),
            USBSTS => self.usbsts &= !(value & USBSTS_RW1C),
            DNCTRL => self.dnctrl = value,
            0x18 | 0x1C => self.write_crcr(offset == 0x1C, value, dma),
            0x30 | 0x34 => {
                self.forbid_while_running("DCBAAP");
                set_half(&mut self.dcbaap, offset == 0x34, value);
            }
            CONFIG => {
                self.forbid_while_running("CONFIG");
                if value & 0xFF > self.config.max_slots as u32 {
                    panic!("fake xhci: MaxSlotsEn {} above MaxSlots", value & 0xFF);
                }
                self.config_reg = value;
            }
            o if o >= PORTS => {
                let (port, reg) = ((o - PORTS) / 0x10, (o - PORTS) % 0x10);
                if reg == 0 {
                    self.write_portsc(port, value);
                }
````

Replace:

````rust
            0x30 | 0x34 => half(self.erstba, offset == 0x34),
            0x38 | 0x3C => half(self.erdp, offset == 0x3C),
            _ => 0,
        }
    }

    fn runtime_write(&mut self, offset: usize, value: u32) {
        match offset {
            IMAN => self.iman = value,
            IMOD => self.imod = value,
            ERSTSZ => self.erstsz = value & 0xFFFF,
            0x30 | 0x34 => set_half(&mut self.erstba, offset == 0x34, value),
            0x38 | 0x3C => set_half(&mut self.erdp, offset == 0x3C, value),
            _ => {}
        }
````

with:

````rust
            0x30 | 0x34 => half(self.erstba, offset == 0x34),
            ERDP => self.erdp as u32 | if self.ehb { EHB } else { 0 },
            0x3C => (self.erdp >> 32) as u32,
            _ => 0,
        }
    }

    fn runtime_write(&mut self, offset: usize, value: u32, dma: &Dma) {
        match offset {
            // IP (bit 0) is RW1C.
            IMAN => self.iman = value & 2 | self.iman & 1 & !value,
            IMOD => self.imod = value,
            ERSTSZ => self.erstsz = value & 0xFFFF,
            0x30 | 0x34 => self.write_erstba(offset == 0x34, value, dma),
            0x38 | 0x3C => self.write_erdp(offset == 0x3C, value),
            _ => {}
        }
    }

    /// Port power; the rest of PORTSC comes with the port model.
    fn write_portsc(&mut self, index: usize, value: u32) {
        self.port_writes += 1;
        if self.config.ppc && value & PP != 0 && self.portsc[index] & PP == 0 {
            self.portsc[index] |= PP;
            self.powered_at = Some(self.now);
        }
````

- [ ] **Step 5: Create `crates/usb/src/testing/xhci/rings.rs`**

Create `crates/usb/src/testing/xhci/rings.rs`:

````rust
//! The rings as software sets them up: CRCR, the event ring segment table
//! and ERDP, with the checks the controller makes when it starts.

use super::regs::{HCH, set_half};
use super::{Consumer, EventRing, FakeXhci};
use crate::testing::hal::Dma;

impl FakeXhci {
    pub(super) fn forbid_while_running(&self, what: &str) {
        if self.running() {
            panic!("fake xhci: {what} written while running");
        }
    }

    /// CRCR (xHCI 5.4.5): the ring pointer takes effect with the high half.
    pub(super) fn write_crcr(&mut self, high: bool, value: u32, dma: &Dma) {
        if self.crr {
            panic!("fake xhci: CRCR written while the command ring runs");
        }
        set_half(&mut self.crcr, high, value);
        if high {
            let dequeue = self.crcr & !0x3F;
            if !dma.contains(dequeue, 16) {
                panic!("fake xhci: CRCR names unallocated memory at {dequeue:#x}");
            }
            self.command_ring = Some(Consumer {
                dequeue,
                cycle: self.crcr & 1 != 0,
            });
        }
    }

    /// ERSTBA (xHCI 4.9.4): writing it (the high half last) makes the
    /// controller read the segment table, so ERSTSZ must already be set.
    pub(super) fn write_erstba(&mut self, high: bool, value: u32, dma: &Dma) {
        self.forbid_while_running("ERSTBA");
        if self.erstsz == 0 {
            panic!("fake xhci: ERSTBA written before ERSTSZ");
        }
        set_half(&mut self.erstba, high, value);
        if !high {
            return;
        }
        if self.erstba & 0x3F != 0 {
            panic!("fake xhci: ERSTBA {:#x} not 64-byte aligned", self.erstba);
        }
        let base = dma.read64(self.erstba) & !0x3F;
        let size = (dma.read32(self.erstba + 8) & 0xFFFF) as usize;
        if !(16..=4096).contains(&size) || !dma.contains(base, size * 16) {
            panic!("fake xhci: bad event ring segment {base:#x} of {size} TRBs");
        }
        self.event_ring = Some(EventRing {
            base,
            size,
            enqueue: 0,
            cycle: true,
        });
        self.check_erdp();
    }

    /// ERDP: EHB (bit 3) is RW1C; the pointer must stay in the segment.
    pub(super) fn write_erdp(&mut self, high: bool, value: u32) {
        if high {
            set_half(&mut self.erdp, true, value);
            self.check_erdp();
        } else {
            if value & super::regs::EHB != 0 {
                self.ehb = false;
            }
            set_half(&mut self.erdp, false, value & !0xF);
        }
    }

    fn check_erdp(&self) {
        if let Some(r) = self.event_ring
            && !(r.base..r.base + 16 * r.size as u64).contains(&self.erdp)
        {
            panic!(
                "fake xhci: ERDP {:#x} outside the event ring segment",
                self.erdp
            );
        }
    }

    /// R/S = 1: what must be programmed first (xHCI 4.2), then running.
    pub(super) fn start(&mut self, dma: &Dma) {
        let slots = (self.config_reg & 0xFF) as usize;
        if slots == 0 {
            panic!("fake xhci: R/S set with CONFIG.MaxSlotsEn 0");
        }
        if self.dcbaap == 0
            || self.dcbaap & 0x3F != 0
            || !dma.contains(self.dcbaap, 8 * (slots + 1))
        {
            panic!("fake xhci: R/S set without a DCBAA ({:#x})", self.dcbaap);
        }
        if self.command_ring.is_none() {
            panic!("fake xhci: R/S set without CRCR");
        }
        if self.event_ring.is_none() {
            panic!("fake xhci: R/S set without an event ring");
        }
        self.scratchpad_pages = self.read_scratchpads(dma);
        if let Some(delay) = self.config.run_time {
            self.after(delay, |x, _| x.usbsts &= !HCH);
        }
    }

    /// DCBAA[0] names the scratchpad array when there are buffers (xHCI
    /// 4.20); each entry a page of its own.
    fn read_scratchpads(&self, dma: &Dma) -> Vec<u64> {
        let n = self.config.scratchpads as usize;
        let array = dma.read64(self.dcbaap);
        if n == 0 {
            return Vec::new();
        }
        if array == 0 || array & 0x3F != 0 || !dma.contains(array, 8 * n) {
            panic!("fake xhci: no scratchpad array for {n} buffers in DCBAA[0] ({array:#x})");
        }
        let pages: Vec<u64> = (0..n).map(|i| dma.read64(array + 8 * i as u64)).collect();
        for (i, &page) in pages.iter().enumerate() {
            if page & 0xFFF != 0 || !dma.contains(page, 4096) || pages[..i].contains(&page) {
                panic!("fake xhci: scratchpad buffer {i} at {page:#x} is not a page of its own");
            }
        }
        pages
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;

    #[test]
    #[should_panic(expected = "ERSTBA written before ERSTSZ")]
    fn the_segment_table_size_comes_first() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x1000 + 0x30, 0, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "R/S set with CONFIG.MaxSlotsEn 0")]
    fn running_needs_the_programming_done() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.write(0x40, 1, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "CONFIG written while running")]
    fn config_cannot_change_while_running() {
        let mut x = FakeXhci::new(FakeConfig::intel());
        x.write(0x80 + 0x38, 1, &Dma::default());
    }
}
````

- [ ] **Step 6: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod ring;
mod trb;
````

with:

````rust
mod ring;
mod start;
mod trb;
````

- [ ] **Step 7: Add the failing tests to `crates/usb/src/xhci/regs.rs`**

In `crates/usb/src/xhci/regs.rs`, replace:

````rust
    #[test]
    fn offsets_from_the_hardware_are_checked_against_the_bar() {
````

with:

````rust
    #[test]
    fn a_neutral_portsc_write_keeps_only_power_indicator_and_wake_bits() {
        let all = 0xFFFF_FFFF;
        assert_eq!(portsc_neutral(all), PP | 3 << 14 | 7 << 25);
        assert_eq!(portsc_neutral(PED | CHANGE_BITS | PR | WPR | CCS), 0);
    }

    #[test]
    fn offsets_from_the_hardware_are_checked_against_the_bar() {
````

- [ ] **Step 8: Write the failing tests for `crates/usb/src/xhci/start.rs`**

Create `crates/usb/src/xhci/start.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ExtCap, FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal, start};

    fn new(hal: &FakeHal) -> Result<Xhci<FakeHal>, UsbError> {
        Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0")
    }

    #[test]
    fn qemu_and_intel_controllers_start() {
        for config in [FakeConfig::basic(), FakeConfig::intel()] {
            let (hal, xhci) = start(config);
            let fake = hal.fake();
            assert!(fake.running());
            assert_eq!(fake.dcbaap(), xhci.dcbaa.phys());
            let ring = fake.command_ring().unwrap();
            assert_eq!((ring.dequeue, ring.cycle), (xhci.commands.phys(), true));
            assert_eq!(fake.event_ring().unwrap().base, xhci.events.phys());
            assert_eq!(xhci.name(), "00:14.0");
        }
        let (_, xhci) = start(FakeConfig::intel());
        assert_eq!(
            xhci.info().to_string(),
            "xHCI 1.20, 16 ports (12 USB 2, 4 USB 3), 64-byte contexts, 2 scratchpads"
        );
    }

    #[test]
    fn interrupts_stay_off() {
        let (hal, _xhci) = start(FakeConfig::intel());
        assert_eq!(hal.fake().iman() & IE, 0);
        assert_eq!(hal.fake().usbcmd() & (INTE | RUN), RUN);
    }

    #[test]
    fn scratchpad_pages_are_where_dcbaa_0_says() {
        for n in [2, 33] {
            let mut config = FakeConfig::intel();
            config.scratchpads = n;
            let (hal, xhci) = start(config);
            let s = xhci.scratchpads.as_ref().unwrap();
            assert_eq!(xhci.dcbaa.read64(0), s.array.phys());
            let pages: Vec<u64> = s.pages.iter().map(|p| p.phys()).collect();
            assert_eq!(hal.fake().scratchpad_pages(), &pages[..]);
            assert_eq!(pages.len(), n as usize);
            assert!(
                hal.log_text()
                    .contains(&alloc::format!("{n} scratchpad buffers, array 0x"))
            );
        }
    }

    #[test]
    fn without_scratchpads_nothing_is_allocated_for_them() {
        let (hal, xhci) = start(FakeConfig::basic());
        assert!(xhci.scratchpads.is_none());
        assert_eq!(xhci.dcbaa.read64(0), 0);
        // The DCBAA, the command ring, the event ring and its ERST.
        assert_eq!(hal.outstanding_dma(), 4);
        assert!(hal.log_text().contains("no scratchpad buffers"));
    }

    #[test]
    fn page_sizes_other_than_4_kib_are_refused() {
        let mut config = FakeConfig::intel();
        config.page_size = 2;
        let hal = FakeHal::with_controller(config);
        assert_eq!(
            new(&hal).err(),
            Some(UsbError::Unsupported("page size other than 4 KiB"))
        );
        assert_eq!(hal.outstanding_dma(), 0);
        assert!(hal.log_text().contains("page size 8 KiB"));
        assert!(
            hal.log_text()
                .contains("PAGESIZE 0x2: only 4 KiB pages are supported")
        );
    }

    #[test]
    fn controllers_without_64_bit_addressing_are_refused() {
        let mut config = FakeConfig::basic();
        config.ac64 = false;
        let hal = FakeHal::with_controller(config);
        assert_eq!(
            new(&hal).err(),
            Some(UsbError::Unsupported("32-bit DMA only"))
        );
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn a_controller_reading_all_ones_does_not_start() {
        let mut config = FakeConfig::intel();
        config.all_ones = true;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::NotResponding));
        assert!(hal.log_text().contains("controller not responding"));
    }

    #[test]
    fn a_start_that_fails_at_any_allocation_frees_every_dma_buffer() {
        // Intel's start allocates 7 buffers: 2 scratchpad pages and their
        // array, the DCBAA, the command ring, the event ring and its ERST.
        for n in 0..7 {
            let hal = FakeHal::with_controller(FakeConfig::intel());
            hal.fail_alloc_after(n);
            assert_eq!(new(&hal).err(), Some(UsbError::NoMemory), "allocation {n}");
            assert_eq!(hal.outstanding_dma(), 0, "allocation {n}");
            assert!(!hal.fake().running());
        }
        let hal = FakeHal::with_controller(FakeConfig::intel());
        hal.fail_alloc_after(7);
        assert!(new(&hal).is_ok());
        // One allocation failing while the later ones succeed.
        for n in 0..7 {
            let hal = FakeHal::with_controller(FakeConfig::intel());
            hal.fail_one_alloc(n);
            assert_eq!(new(&hal).err(), Some(UsbError::NoMemory), "allocation {n}");
            assert_eq!(hal.outstanding_dma(), 0, "allocation {n}");
        }
    }

    #[test]
    fn a_controller_that_never_runs_is_stopped_and_freed() {
        let mut config = FakeConfig::intel();
        config.run_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
        assert_eq!(hal.fake().usbcmd() & RUN, 0);
        assert!(
            hal.log_text()
                .contains("USBSTS.HCH still 1 1000 ms after R/S = 1")
        );
    }

    #[test]
    fn a_failed_halt_or_reset_allocates_nothing() {
        let mut config = FakeConfig::intel();
        config.halt_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
        let mut config = FakeConfig::intel();
        config.cnr_time = None;
        let hal = FakeHal::with_controller(config);
        assert_eq!(new(&hal).err(), Some(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), 0);
    }

    #[test]
    fn the_log_has_what_a_photo_of_the_nuc_screen_needs() {
        let (hal, _xhci) = start(FakeConfig::intel());
        let log = hal.log_text();
        for line in [
            "xhci 00:14.0: xHCI 1.20, 64 slots, 16 ports, 8 interrupters, 64-byte contexts, page size 4 KiB",
            "xhci 00:14.0: legacy support at 0x8000: BIOS owned, handed over after 5 ms; SMI enables 0x0000e011 cleared",
            "xhci 00:14.0: halted after 1 ms",
            "xhci 00:14.0: reset after 12 ms",
            "xhci 00:14.0: USB 2.0 ports 1-12, USB 3.1 ports 13-16",
            "xhci 00:14.0: 2 scratchpad buffers, array 0x",
            "xhci 00:14.0: DCBAA 0x",
            "xhci 00:14.0: running, ports powered",
        ] {
            assert!(log.contains(line), "missing {line:?} in\n{log}");
        }
    }

    #[test]
    fn ports_get_power_when_the_controller_controls_it() {
        let (hal, _xhci) = start(FakeConfig::intel());
        for port in 1..=16 {
            assert_eq!(hal.fake().portsc(port) & PP, PP, "port {port}");
        }
        let powered = hal.fake().powered_at().unwrap();
        assert!(
            hal.clock() - powered >= ATTACH_TIME,
            "devices get time to signal their attach"
        );
        let (hal, _xhci) = start(FakeConfig::basic());
        assert_eq!(
            hal.fake().port_writes(),
            0,
            "QEMU's ports are always powered"
        );
        assert!(hal.log_text().contains("xhci 00:14.0: running\n"));
    }

    #[test]
    fn a_port_no_protocol_covers_is_logged_and_counted_nowhere() {
        let mut config = FakeConfig::basic();
        config
            .caps
            .retain(|c| !matches!(c.cap, ExtCap::Protocol { major: 3, .. }));
        config.caps[0].next = 0;
        let (hal, xhci) = start(config);
        assert_eq!((xhci.info().usb2_ports, xhci.info().usb3_ports), (4, 0));
        assert_eq!(xhci.ports[4], None);
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 5: no supported protocol, not used")
        );
    }

    #[test]
    fn an_idle_controller_waits_only_the_attach_time() {
        let (hal, _xhci) = start(FakeConfig::intel());
        let waited = hal.clock() - hal.fake().powered_at().unwrap();
        assert!(waited >= ATTACH_TIME && waited < ATTACH_TIME + Duration::from_millis(1));
        assert!(
            hal.log_text()
                .ends_with("xhci 00:14.0: ports settled after 100 ms")
        );
        // Without port power control the reset is what the ports settle
        // from.
        let (hal, _xhci) = start(FakeConfig::basic());
        assert!(hal.clock() >= ATTACH_TIME && hal.clock() < ATTACH_TIME + Duration::from_millis(5));
    }

    #[test]
    fn an_empty_protocol_capability_does_not_panic() {
        let mut config = FakeConfig::qemu();
        if let ExtCap::Protocol { first, count, .. } = &mut config.caps[1].cap {
            (*first, *count) = (0, 0);
        }
        let (hal, xhci) = start(config);
        assert_eq!((xhci.info().usb2_ports, xhci.info().usb3_ports), (4, 0));
        assert!(hal.log_text().contains(
            "xhci 00:14.0: USB 3.0 protocol capability names no ports (first 0, count 0); ignored"
        ));
    }
}
````

- [ ] **Step 9: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find value `PP` in this scope ``; `` cannot find value `PED` in this scope ``.

- [ ] **Step 10: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use core::fmt;

````

with:

````rust

use crate::{DmaBuf, Hal};
use alloc::string::String;
use alloc::vec::Vec;
use caps::PortProtocol;
use core::fmt;
use regs::Regs;
use ring::{EventRing, ProducerRing};
use start::Scratchpads;

````

Replace:

````rust
        )
    }
````

with:

````rust
        )
    }
}

/// One xHCI controller, brought up by [`Xhci::new`].
pub struct Xhci<H: Hal> {
    hal: H,
    name: String,
    regs: Regs,
    info: ControllerInfo,
    /// Each root port's protocol; index 0 is port 1.
    ports: Vec<Option<PortProtocol>>,
    dcbaa: DmaBuf,
    scratchpads: Option<Scratchpads>,
    commands: ProducerRing,
    events: EventRing,
}

impl<H: Hal> Xhci<H> {
    pub fn info(&self) -> &ControllerInfo {
        &self.info
    }

    /// The PCI address every log line starts with.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hal(&self) -> &H {
        &self.hal
    }
````

- [ ] **Step 11: Change `crates/usb/src/xhci/regs.rs`**

In `crates/usb/src/xhci/regs.rs`, replace:

````rust
const PORT_STRIDE: usize = 0x10;

````

with:

````rust
const PORT_STRIDE: usize = 0x10;

// PORTSC bits (xHCI 5.4.8). PED and the change bits are RW1C: writing 1
// to PED disables the port.
pub const CCS: u32 = 1 << 0;
pub const PED: u32 = 1 << 1;
pub const PR: u32 = 1 << 4;
pub const PLS_SHIFT: u32 = 5;
pub const PP: u32 = 1 << 9;
pub const SPEED_SHIFT: u32 = 10;
const PIC: u32 = 3 << 14;
pub const CSC: u32 = 1 << 17;
pub const PEC: u32 = 1 << 18;
pub const WRC: u32 = 1 << 19;
pub const OCC: u32 = 1 << 20;
pub const PRC: u32 = 1 << 21;
pub const PLC: u32 = 1 << 22;
pub const CEC: u32 = 1 << 23;
const WAKE: u32 = 7 << 25;
pub const WPR: u32 = 1 << 31;
pub const CHANGE_BITS: u32 = CSC | PEC | WRC | OCC | PRC | PLC | CEC;

/// The start of every PORTSC write: of what was read, only PP, PIC and
/// the wake bits, which keep their value when written back. Writing back
/// anything else would clear change bits by accident or disable the port.
pub fn portsc_neutral(portsc: u32) -> u32 {
    portsc & (PP | PIC | WAKE)
}

````

- [ ] **Step 12: Implement `crates/usb/src/xhci/start.rs`**

Insert this at the top of `crates/usb/src/xhci/start.rs`, above `#[cfg(test)]`:

````rust
//! Starting the controller (spec §6.2): the memory it reads from the
//! start (scratchpad buffers, DCBAA, command and event rings), programming
//! and running it, port power, and `Xhci::new`, which puts it all together
//! after the BIOS handoff, halt and reset of `init`.

use super::caps::{CapList, PortProtocol, Protocol, port_map, protocols};
use super::init::{REGISTER_TIMEOUT, bios_handoff, halt, reset, wait_for};
use super::regs::{
    CONFIG, CRCR, DCBAAP, ERDP, ERSTBA, ERSTSZ, HCH, IE, IMAN, INTE, PAGESIZE, PP, Params, RCS,
    RUN, Regs, USBCMD, USBSTS, portsc_neutral,
};
use super::ring::{EventRing, ProducerRing};
use super::{ControllerInfo, Xhci};
use crate::{DmaBuf, Hal, UsbError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use core::time::Duration;

/// How long a device may take to signal its attach after its port was
/// powered or reset (USB 2.0 7.1.7.3, TSIGATT), so devices present at boot
/// are connected when the first `port_changes` looks.
const ATTACH_TIME: Duration = Duration::from_millis(100);
/// The only page size this driver sets up scratchpad buffers for.
const PAGE: usize = 4096;

/// The scratchpad buffers (xHCI 4.20): pages the controller keeps its own
/// state in, named by an array DCBAA[0] points to.
#[derive(Debug)]
pub struct Scratchpads {
    array: DmaBuf,
    pages: Vec<DmaBuf>,
}

impl Scratchpads {
    /// `count` pages and the array naming them; `None` for 0.
    pub fn new<H: Hal>(hal: &H, count: u16) -> Result<Option<Scratchpads>, UsbError> {
        if count == 0 {
            return Ok(None);
        }
        let array = hal
            .alloc_dma(8 * count as usize, 64)
            .ok_or(UsbError::NoMemory)?;
        let mut s = Scratchpads {
            array,
            pages: Vec::new(),
        };
        for i in 0..count as usize {
            let Some(page) = hal.alloc_dma(PAGE, PAGE) else {
                s.free(hal);
                return Err(UsbError::NoMemory);
            };
            s.array.write64(8 * i, page.phys());
            s.pages.push(page);
        }
        Ok(Some(s))
    }

    pub fn free<H: Hal>(self, hal: &H) {
        for page in self.pages {
            hal.free_dma(page);
        }
        hal.free_dma(self.array);
    }
}

/// What the controller reads from memory from the start.
struct Memory {
    dcbaa: DmaBuf,
    scratchpads: Option<Scratchpads>,
    commands: ProducerRing,
    events: EventRing,
}

impl Memory {
    /// Allocates all of it, or nothing.
    fn new<H: Hal>(hal: &H, params: &Params) -> Result<Memory, UsbError> {
        let scratchpads = Scratchpads::new(hal, params.scratchpads)?;
        let dcbaa = hal.alloc_dma(8 * (params.max_slots as usize + 1), 64);
        let commands = ProducerRing::new(hal).ok();
        let events = EventRing::new(hal).ok();
        match (dcbaa, commands, events) {
            (Some(dcbaa), Some(commands), Some(events)) => {
                if let Some(s) = &scratchpads {
                    dcbaa.write64(0, s.array.phys());
                }
                Ok(Memory {
                    dcbaa,
                    scratchpads,
                    commands,
                    events,
                })
            }
            (dcbaa, commands, events) => {
                if let Some(b) = dcbaa {
                    hal.free_dma(b);
                }
                if let Some(r) = commands {
                    r.free(hal);
                }
                if let Some(r) = events {
                    r.free(hal);
                }
                if let Some(s) = scratchpads {
                    s.free(hal);
                }
                Err(UsbError::NoMemory)
            }
        }
    }

    fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.dcbaa);
        self.commands.free(hal);
        self.events.free(hal);
        if let Some(s) = self.scratchpads {
            s.free(hal);
        }
    }
}

/// Programs the rings and starts the controller (xHCI 4.2): slots, DCBAA,
/// command ring, the primary interrupter with ERSTBA last (xHCI 4.9.4),
/// interrupts off (milestone 1 polls), then R/S and up to 1 s for HCH = 0.
fn run<H: Hal>(
    hal: &H,
    regs: &Regs,
    params: &Params,
    mem: &Memory,
    name: &str,
) -> Result<(), UsbError> {
    let config = regs.op_read(hal, CONFIG);
    regs.op_write(hal, CONFIG, config & !0xFF | params.max_slots as u32);
    regs.op_write64(hal, DCBAAP, mem.dcbaa.phys());
    regs.op_write64(hal, CRCR, mem.commands.phys() | RCS as u64);
    regs.intr_write(hal, ERSTSZ, 1);
    regs.intr_write64(hal, ERDP, mem.events.dequeue_pointer());
    regs.intr_write64(hal, ERSTBA, mem.events.erst_phys());
    let iman = regs.intr_read(hal, IMAN);
    regs.intr_write(hal, IMAN, iman & !IE);
    let cmd = regs.op_read(hal, USBCMD) & !INTE;
    regs.op_write(hal, USBCMD, cmd | RUN);
    if wait_for(hal, REGISTER_TIMEOUT, || {
        regs.op_read(hal, USBSTS) & HCH == 0
    })
    .is_none()
    {
        xlog!(
            hal,
            name,
            "USBSTS.HCH still 1 {} ms after R/S = 1",
            REGISTER_TIMEOUT.as_millis()
        );
        regs.op_write(hal, USBCMD, cmd);
        return Err(UsbError::Timeout);
    }
    Ok(())
}

/// With Port Power Control, ports start unpowered: powers each one that
/// is off. Returns whether any was.
fn power_ports<H: Hal>(hal: &H, regs: &Regs, params: &Params) -> bool {
    if !params.ppc {
        return false;
    }
    let mut powered = false;
    for port in 1..=params.ports {
        let sc = regs.portsc(hal, port);
        if sc & PP == 0 {
            regs.set_portsc(hal, port, portsc_neutral(sc) | PP);
            powered = true;
        }
    }
    powered
}

/// "USB 2.0 ports 1-12, USB 3.1 ports 13-16".
fn describe(protocols: &[Protocol]) -> String {
    let mut line = String::new();
    for (i, p) in protocols.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        let last = (p.first as u16 + p.count as u16).saturating_sub(1);
        let _ = write!(
            line,
            "{sep}USB {}.{:x} ports {}-{last}",
            p.major,
            p.minor >> 4,
            p.first
        );
    }
    if line.is_empty() {
        line.push_str("no supported protocol capabilities");
    }
    line
}

impl<H: Hal> Xhci<H> {
    /// Brings the controller up (spec §6.2): maps `mmio_len` bytes of
    /// registers at `mmio_phys`, BIOS handoff, halt, reset, scratchpad
    /// buffers, DCBAA, command and event rings, run, port power. `name`
    /// (the PCI address, "00:14.0") starts every log line. On an error
    /// everything allocated is freed again.
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
        let Some(base) = hal.map_mmio(mmio_phys, mmio_len) else {
            xlog!(
                &hal,
                name,
                "cannot map {mmio_len:#x} bytes of registers at {mmio_phys:#x}"
            );
            return Err(UsbError::NoMemory);
        };
        let regs = Regs::new(&hal, base, mmio_len)
            .inspect_err(|e| xlog!(&hal, name, "registers at {mmio_phys:#x}: {e}"))?;
        let params = Params::read(&hal, &regs);
        let page = regs.op_read(&hal, PAGESIZE);
        xlog!(
            &hal,
            name,
            "xHCI {:x}.{:02x}, {} slots, {} ports, {} interrupters, {}-byte contexts, page size {} KiB",
            params.version >> 8,
            params.version & 0xFF,
            params.max_slots,
            params.ports,
            params.interrupters,
            params.context_size,
            4u64 << page.trailing_zeros().min(20),
        );
        if !params.ac64 {
            xlog!(&hal, name, "no 64-bit addressing (HCCPARAMS1.AC64 = 0)");
            return Err(UsbError::Unsupported("32-bit DMA only"));
        }
        if page & 1 == 0 {
            xlog!(
                &hal,
                name,
                "PAGESIZE {page:#x}: only 4 KiB pages are supported"
            );
            return Err(UsbError::Unsupported("page size other than 4 KiB"));
        }
        let caps = CapList::walk(&hal, &regs, params.xecp);
        if let Some(problem) = caps.problem {
            xlog!(&hal, name, "extended capabilities: {problem}");
        }
        bios_handoff(&hal, &regs, &caps, name);
        halt(&hal, &regs, name)?;
        reset(&hal, &regs, name)?;
        // HCRST reset the root ports too.
        let mut settle_from = hal.now();
        let found = protocols(&hal, &regs, &caps);
        xlog!(&hal, name, "{}", describe(&found.usable));
        for p in &found.empty {
            xlog!(
                &hal,
                name,
                "USB {}.{:x} protocol capability names no ports (first {}, count {}); ignored",
                p.major,
                p.minor >> 4,
                p.first,
                p.count
            );
        }
        let ports = port_map(params.ports, &found.usable);
        for (i, _) in ports.iter().enumerate().filter(|(_, p)| p.is_none()) {
            xlog!(
                &hal,
                name,
                "port {}: no supported protocol, not used",
                i + 1
            );
        }
        let mem =
            Memory::new(&hal, &params).inspect_err(|_| xlog!(&hal, name, "out of DMA memory"))?;
        match &mem.scratchpads {
            Some(s) => xlog!(
                &hal,
                name,
                "{} scratchpad buffers, array {:#x}",
                s.pages.len(),
                s.array.phys()
            ),
            None => xlog!(&hal, name, "no scratchpad buffers"),
        }
        xlog!(
            &hal,
            name,
            "DCBAA {:#x}, command ring {:#x}, event ring {:#x}",
            mem.dcbaa.phys(),
            mem.commands.phys(),
            mem.events.phys()
        );
        if let Err(e) = run(&hal, &regs, &params, &mem, name) {
            mem.free(&hal);
            return Err(e);
        }
        let powered = power_ports(&hal, &regs, &params);
        if powered {
            settle_from = hal.now();
        }
        xlog!(
            &hal,
            name,
            "running{}",
            if powered { ", ports powered" } else { "" }
        );
        let count = |kind| ports.iter().filter(|&&p| p == Some(kind)).count() as u8;
        let info: ControllerInfo =
            params.info(count(PortProtocol::Usb2), count(PortProtocol::Usb3));
        let mut xhci = Xhci {
            hal,
            name: name.to_string(),
            regs,
            info,
            ports,
            dcbaa: mem.dcbaa,
            scratchpads: mem.scratchpads,
            commands: mem.commands,
            events: mem.events,
        };
        xhci.settle_ports(settle_from);
        Ok(xhci)
    }

    /// Waits until 100 ms after the ports were last powered or reset, so
    /// that devices present at boot have signalled their attach.
    fn settle_ports(&mut self, since: Duration) {
        let start = self.hal.now();
        self.hal.sleep((since + ATTACH_TIME).saturating_sub(start));
        let waited = (self.hal.now() - start).as_millis();
        xlog!(&self.hal, &self.name, "ports settled after {waited} ms");
    }
}

````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 109 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add crates
git commit -m "usb: start the xHCI controller: scratchpads, DCBAA, rings, run, port power"
````


### Task 9: Commands, events and poll

One command at a time on the command ring: `command` pushes it, rings doorbell 0 and polls for its Command Completion Event, matched by the command TRB's address, for up to 500 ms (spec §6.2). On a timeout it aborts the command ring, writing all 64 bits of CRCR (some controllers act only on the high half), and waits up to 1 s for the Command Ring Stopped event that follows the Command Aborted one; neither event ever completes a command. It then overwrites the timed-out TRB with a No Op, so that whether the controller resumes on it or after it nothing runs twice. If the abort never finishes, the controller is dead: it is halted (R/S cleared) so it stops using memory, and every later operation fails at once with `ControllerDead`. `poll` never waits: it drains the event ring while the cycle bits match, hands each event to whoever waits for it (commands, transfers, port changes), ignores stale completions, and writes ERDP once with EHB. A host system error marks the controller dead too; the controller clears R/S itself then (xHCI 5.4.2), so `poll` still does not wait.

**Files:**
- Modify: `crates/usb/src/testing/hal.rs`
- Create: `crates/usb/src/testing/xhci/commands.rs`
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Modify: `crates/usb/src/testing/xhci/rings.rs`
- Create: `crates/usb/src/xhci/command.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/start.rs`

**Interfaces:**
- Consumes: Task 8.
- Produces: `usb::xhci::command::{COMMAND_TIMEOUT = 500 ms, Completion { code, slot, … }}`, `Xhci::poll(&mut self)` (never waits) and the crate-internal `Xhci::command(&mut self, Trb) -> Result<Completion, UsbError>`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/hal.rs`**

In `crates/usb/src/testing/hal.rs`, replace:

````rust
        hal
    }
````

with:

````rust
        hal
    }

    /// Runs `f` on the fake controller with the DMA memory it reaches (to
    /// post events or read what the driver wrote).
    pub fn act<R>(&self, f: impl FnOnce(&mut FakeXhci, &Dma) -> R) -> R {
        let mut x = self.fake();
        f(&mut x, &self.0.dma.borrow())
    }
````

- [ ] **Step 2: Create `crates/usb/src/testing/xhci/commands.rs`**

Create `crates/usb/src/testing/xhci/commands.rs`:

````rust
//! The command ring (xHCI 4.6): the fake consumes TRBs while their cycle
//! bit matches its cycle state, follows Link TRBs, executes each command
//! and posts its completion; CRCR.CA aborts. Commands are decoded with the
//! fake's own field layouts.

use super::{Consumer, FakeXhci};
use crate::testing::hal::Dma;

// TRB types (xHCI table 6-91).
pub const LINK: u32 = 6;
pub const ENABLE_SLOT: u32 = 9;
pub const DISABLE_SLOT: u32 = 10;
pub const NO_OP_COMMAND: u32 = 23;
pub const TRANSFER_EVENT: u32 = 32;
pub const COMMAND_COMPLETION: u32 = 33;
pub const PORT_STATUS_CHANGE: u32 = 34;

// Completion codes.
pub const SUCCESS: u32 = 1;
pub const NO_SLOTS: u32 = 9;
pub const SLOT_NOT_ENABLED: u32 = 11;
pub const COMMAND_RING_STOPPED: u32 = 24;
pub const COMMAND_ABORTED: u32 = 25;

pub fn trb_type(trb: &[u32; 4]) -> u32 {
    trb[3] >> 10 & 0x3F
}

pub fn slot_of(trb: &[u32; 4]) -> usize {
    (trb[3] >> 24) as usize
}

/// A command the fake executed: its TRB's address and type, and the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Executed {
    pub addr: u64,
    pub kind: u32,
    pub slot: usize,
}

/// A device slot as the controller sees it.
#[derive(Debug, Default)]
pub struct FakeSlot {}

impl Consumer {
    /// The TRB at the dequeue pointer if software has handed it over,
    /// following Link TRBs (toggling the cycle state where they say so).
    pub fn peek(&mut self, dma: &Dma) -> Option<(u64, [u32; 4])> {
        for _ in 0..2 {
            let at = self.dequeue;
            let trb = [
                dma.read32(at),
                dma.read32(at + 4),
                dma.read32(at + 8),
                dma.read32(at + 12),
            ];
            if (trb[3] & 1 != 0) != self.cycle {
                return None;
            }
            if trb_type(&trb) != LINK {
                return Some((at, trb));
            }
            self.dequeue = (trb[0] as u64 | (trb[1] as u64) << 32) & !0xF;
            if trb[3] & 2 != 0 {
                self.cycle = !self.cycle;
            }
        }
        panic!("fake xhci: a Link TRB leads to a Link TRB");
    }

    pub fn advance(&mut self) {
        self.dequeue += 16;
    }
}

impl FakeXhci {
    /// Doorbell 0 (xHCI 5.6): target 0 runs the command ring.
    pub(super) fn command_doorbell(&mut self, target: u32) {
        if target != 0 {
            panic!("fake xhci: doorbell 0 rung with target {target}");
        }
        if !self.running() {
            panic!("fake xhci: doorbell rung while halted");
        }
        self.crr = true;
    }

    /// Executes every command handed over, unless one hangs.
    pub(super) fn process_commands(&mut self, dma: &Dma) {
        if !self.crr || !self.running() {
            return;
        }
        loop {
            let Some(ring) = self.command_ring.as_mut() else {
                return;
            };
            let Some((addr, trb)) = ring.peek(dma) else {
                return;
            };
            if self.config.hang_command == Some(trb_type(&trb)) {
                self.hung = Some(addr);
                return;
            }
            ring.advance();
            self.execute(addr, trb, dma);
        }
    }

    fn execute(&mut self, addr: u64, trb: [u32; 4], dma: &Dma) {
        let kind = trb_type(&trb);
        let (code, slot) = match kind {
            NO_OP_COMMAND => (SUCCESS, 0),
            ENABLE_SLOT => self.enable_slot(),
            DISABLE_SLOT => (self.disable_slot(slot_of(&trb)), slot_of(&trb)),
            _ => panic!("fake xhci: TRB type {kind} on the command ring"),
        };
        self.executed.push(Executed { addr, kind, slot });
        self.post_completion(addr, code, slot, dma);
    }

    pub fn post_completion(&mut self, addr: u64, code: u32, slot: usize, dma: &Dma) {
        self.post(
            [
                addr as u32,
                (addr >> 32) as u32,
                code << 24,
                COMMAND_COMPLETION << 10 | (slot as u32) << 24,
            ],
            dma,
        );
    }

    /// The lowest free slot up to CONFIG.MaxSlotsEn (xHCI 4.6.3).
    fn enable_slot(&mut self) -> (u32, usize) {
        let enabled = (self.config_reg & 0xFF) as usize;
        match (1..=enabled).find(|&s| self.slots[s].is_none()) {
            Some(s) => {
                self.slots[s] = Some(FakeSlot::default());
                (SUCCESS, s)
            }
            None => (NO_SLOTS, 0),
        }
    }

    fn disable_slot(&mut self, slot: usize) -> u32 {
        match self.slots.get_mut(slot).and_then(Option::take) {
            Some(_) => SUCCESS,
            None => SLOT_NOT_ENABLED,
        }
    }

    /// CRCR.CA (xHCI 4.6.1.2): the command in progress gets a Command
    /// Aborted completion and is consumed, then the ring stops with a
    /// Command Ring Stopped event naming the next TRB. With
    /// `abort_keeps_dequeue` the ring stops on the aborted command instead,
    /// so whatever software leaves there runs when the ring restarts.
    pub(super) fn abort_commands(&mut self, dma: &Dma) {
        if self.config.abort_never_completes || !self.crr {
            return;
        }
        self.aborts += 1;
        self.crr = false;
        if let Some(addr) = self.hung.take()
            && !self.config.abort_keeps_dequeue
        {
            self.post_completion(addr, COMMAND_ABORTED, 0, dma);
            if let Some(ring) = self.command_ring.as_mut() {
                ring.advance();
                // Past a Link TRB, if the next TRB is one.
                let _ = ring.peek(dma);
            }
        }
        let dequeue = self.command_ring.map_or(0, |r| r.dequeue);
        self.post_completion(dequeue, COMMAND_RING_STOPPED, 0, dma);
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;
    use crate::testing::FakeHal;
    use crate::{DmaBuf, Hal};

    fn put(buf: &DmaBuf, index: usize, trb: [u32; 4]) {
        for (i, d) in trb.iter().enumerate() {
            buf.write32(16 * index + 4 * i, *d);
        }
    }

    #[test]
    fn the_consumer_follows_links_and_toggles_its_cycle() {
        let hal = FakeHal::with_controller(FakeConfig::basic());
        let buf = hal.alloc_dma(4096, 64).unwrap();
        put(&buf, 0, [0, 0, 0, NO_OP_COMMAND << 10 | 1]);
        put(
            &buf,
            1,
            [
                buf.phys() as u32,
                (buf.phys() >> 32) as u32,
                0,
                LINK << 10 | 2 | 1,
            ],
        );
        put(&buf, 2, [0, 0, 0, NO_OP_COMMAND << 10 | 1]);
        let mut c = Consumer {
            dequeue: buf.phys(),
            cycle: true,
        };
        hal.act(|_, dma| {
            assert_eq!(c.peek(dma).map(|(a, _)| a), Some(buf.phys()));
            c.advance();
            // The link toggles to cycle 0, and TRB 0 still has cycle 1.
            assert_eq!(c.peek(dma), None);
            assert_eq!((c.dequeue, c.cycle), (buf.phys(), false));
        });
        hal.free_dma(buf);
    }

    #[test]
    #[should_panic(expected = "doorbell rung while halted")]
    fn a_halted_controller_takes_no_doorbell() {
        let mut x = FakeXhci::new(FakeConfig::basic());
        x.command_doorbell(0);
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 9 replacements, top to bottom:

Replace:

````rust

mod regs;
mod rings;

````

with:

````rust

mod commands;
mod regs;
mod rings;

pub use commands::Executed;
use commands::FakeSlot;

````

Replace:

````rust
    pub run_time: Option<Duration>,
}
````

with:

````rust
    pub run_time: Option<Duration>,
    /// Knob: commands of this TRB type never complete.
    pub hang_command: Option<u32>,
    /// Knob: CRCR.CA never stops the command ring (CRR stays 1).
    pub abort_never_completes: bool,
    /// Knob: an abort leaves the dequeue pointer on the aborted command, so
    /// the ring resumes on it (some controllers; xHCI 4.6.1.2 allows both).
    pub abort_keeps_dequeue: bool,
}
````

Replace:

````rust
            run_time: Some(Duration::ZERO),
        }
````

with:

````rust
            run_time: Some(Duration::ZERO),
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
        }
````

Replace:

````rust
            run_time: Some(Duration::from_micros(500)),
        }
````

with:

````rust
            run_time: Some(Duration::from_micros(500)),
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
        }
````

Replace:

````rust
    cap_writes: usize,
}
````

with:

````rust
    cap_writes: usize,
    /// Events waiting for the controller to run.
    pending_events: Vec<[u32; 4]>,
    events_posted: usize,
    erdp_writes: usize,
    /// Slots 1..=MaxSlots (index 0 unused).
    slots: Vec<Option<FakeSlot>>,
    /// Every command executed, in order.
    executed: Vec<Executed>,
    /// The command TRB the ring is stuck on.
    hung: Option<u64>,
    aborts: usize,
    /// CRCR's low half was written with CA; the abort happens once the
    /// high half follows.
    abort_requested: bool,
}
````

Replace:

````rust
            cap_writes: 0,
        };
````

with:

````rust
            cap_writes: 0,
            pending_events: Vec::new(),
            events_posted: 0,
            erdp_writes: 0,
            slots: Vec::new(),
            executed: Vec::new(),
            hung: None,
            aborts: 0,
            abort_requested: false,
        };
````

Replace:

````rust
        }
        x.power_on_ports();
````

with:

````rust
        }
        x.slots = (0..=x.config.max_slots).map(|_| None).collect();
        x.power_on_ports();
````

Replace:

````rust
        self.now = now;
    }
````

with:

````rust
        self.now = now;
        self.process_commands(dma);
        self.flush_events(dma);
    }
````

Replace:

````rust

    /// Reads of the extended capability area so far.
````

with:

````rust

    /// Event Handler Busy: set with the first event after software last
    /// cleared it through ERDP.
    pub fn ehb(&self) -> bool {
        self.ehb
    }

    /// ERDP as software last wrote it (without flags).
    pub fn erdp(&self) -> u64 {
        self.erdp
    }

    pub fn events_posted(&self) -> usize {
        self.events_posted
    }

    /// Writes of ERDP's high half: one per update.
    pub fn erdp_writes(&self) -> usize {
        self.erdp_writes
    }

    pub fn executed(&self) -> &[Executed] {
        &self.executed
    }

    /// The command TRB the ring hangs on.
    pub fn hung(&self) -> Option<u64> {
        self.hung
    }

    /// Command ring aborts (CRCR.CA) that took effect.
    pub fn aborts(&self) -> usize {
        self.aborts
    }

    pub fn slot_enabled(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(Option::is_some)
    }

    /// USBSTS.HSE (xHCI 4.10.2.6): the controller halts at once.
    pub fn host_system_error(&mut self) {
        self.usbsts |= regs::HSE | regs::HCH;
        self.usbcmd &= !regs::RUN;
        self.crr = false;
    }

    /// Reads of the extended capability area so far.
````

- [ ] **Step 4: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub const HCH: u32 = 1 << 0;
pub const CNR: u32 = 1 << 11;
````

with:

````rust
pub const HCH: u32 = 1 << 0;
pub const HSE: u32 = 1 << 2;
pub const CNR: u32 = 1 << 11;
````

Replace:

````rust
            Block::Runtime(o) => self.runtime_write(o, value, dma),
            Block::Doorbell(_) => {}
````

with:

````rust
            Block::Runtime(o) => self.runtime_write(o, value, dma),
            Block::Doorbell(0) => self.command_doorbell(value),
            Block::Doorbell(_) => {}
````

Replace:

````rust
        (self.event_ring, self.erdp, self.ehb) = (None, 0, false);
        self.hcrst_at = Some(self.now);
````

with:

````rust
        (self.event_ring, self.erdp, self.ehb) = (None, 0, false);
        self.pending_events.clear();
        self.slots.iter_mut().for_each(|s| *s = None);
        self.hcrst_at = Some(self.now);
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/xhci/rings.rs`**

In `crates/usb/src/testing/xhci/rings.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use super::regs::{HCH, set_half};
use super::{Consumer, EventRing, FakeXhci};
````

with:

````rust
use super::regs::{HCH, set_half};

/// CRCR: Command Abort.
const CA: u32 = 1 << 2;
use super::{Consumer, EventRing, FakeXhci};
````

Replace:

````rust
    /// CRCR (xHCI 5.4.5): the ring pointer takes effect with the high half.
    pub(super) fn write_crcr(&mut self, high: bool, value: u32, dma: &Dma) {
        if self.crr {
            panic!("fake xhci: CRCR written while the command ring runs");
````

with:

````rust
    /// CRCR (xHCI 5.4.5): the ring pointer takes effect with the high half.
    /// While the ring runs only an abort may be written: CA in the low half,
    /// acted on once the high half follows (as some controllers need).
    pub(super) fn write_crcr(&mut self, high: bool, value: u32, dma: &Dma) {
        if self.crr {
            if !high && value & CA != 0 {
                self.abort_requested = true;
                return;
            }
            if high && self.abort_requested {
                self.abort_requested = false;
                self.abort_commands(dma);
                return;
            }
            panic!("fake xhci: CRCR written while the command ring runs");
````

Replace:

````rust
            set_half(&mut self.erdp, true, value);
            self.check_erdp();
````

with:

````rust
            set_half(&mut self.erdp, true, value);
            self.erdp_writes += 1;
            self.check_erdp();
````

Replace:

````rust
            set_half(&mut self.erdp, false, value & !0xF);
        }
    }

````

with:

````rust
            set_half(&mut self.erdp, false, value & !0xF);
        }
    }

    /// Writes an event (xHCI 4.9.4): dwords 0-2, then the one with the
    /// cycle bit. A full ring is a driver that stopped consuming.
    pub fn post(&mut self, trb: [u32; 4], dma: &Dma) {
        if !self.running() || self.event_ring.is_none() {
            self.pending_events.push(trb);
            return;
        }
        let erdp = self.erdp;
        let Some(ring) = self.event_ring.as_mut() else {
            return;
        };
        let dequeue = ((erdp - ring.base) / 16) as usize;
        let next = (ring.enqueue + 1) % ring.size;
        if next == dequeue {
            panic!("fake xhci: event ring full");
        }
        let at = ring.base + 16 * ring.enqueue as u64;
        for (i, &d) in trb[..3].iter().enumerate() {
            dma.write32(at + 4 * i as u64, d);
        }
        dma.write32(at + 12, trb[3] & !1 | ring.cycle as u32);
        ring.enqueue = next;
        if next == 0 {
            ring.cycle = !ring.cycle;
        }
        self.events_posted += 1;
        if !self.ehb {
            self.ehb = true;
            self.iman |= 1;
        }
    }

    /// Events that came while the controller was halted.
    pub(super) fn flush_events(&mut self, dma: &Dma) {
        if self.running() && self.event_ring.is_some() && !self.pending_events.is_empty() {
            for trb in core::mem::take(&mut self.pending_events) {
                self.post(trb, dma);
            }
        }
    }

    /// Events the controller has written that software has not consumed.
    pub fn unconsumed_events(&self) -> usize {
        self.event_ring.map_or(0, |r| {
            let dequeue = ((self.erdp - r.base) / 16) as usize;
            (r.enqueue + r.size - dequeue) % r.size
        })
    }

````

- [ ] **Step 6: Write the failing tests for `crates/usb/src/xhci/command.rs`**

Create `crates/usb/src/xhci/command.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::trb::{ENABLE_SLOT, NO_OP_COMMAND};
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, start};

    fn no_op(xhci: &mut Xhci<FakeHal>) -> Result<Completion, UsbError> {
        xhci.command(Trb::no_op_command())
    }

    /// An event software does not wait for (MFINDEX Wrap, type 39).
    fn inject(hal: &FakeHal, n: usize) {
        hal.act(|x, dma| {
            for _ in 0..n {
                x.post([0, 0, 1 << 24, 39 << 10], dma);
            }
        });
    }

    #[test]
    fn a_no_op_command_completes() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
        assert_eq!(hal.fake().executed().len(), 1);
        assert_eq!(hal.fake().executed()[0].kind, NO_OP_COMMAND as u32);
    }

    #[test]
    fn six_hundred_commands_in_a_row_all_complete() {
        // The command ring (255 TRBs a lap) and the event ring (256) wrap
        // more than twice.
        let (hal, mut xhci) = start(FakeConfig::intel());
        for i in 0..600 {
            assert_eq!(
                no_op(&mut xhci),
                Ok(Completion {
                    code: SUCCESS,
                    slot: 0
                }),
                "command {i}"
            );
        }
        assert_eq!(hal.fake().executed().len(), 600);
        assert_eq!(hal.fake().unconsumed_events(), 0);
    }

    #[test]
    fn enable_slot_names_the_slot_and_failures_carry_their_code() {
        let (_hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.command(Trb::enable_slot()).map(|c| c.slot), Ok(1));
        assert_eq!(xhci.command(Trb::enable_slot()).map(|c| c.slot), Ok(2));
        assert_eq!(
            xhci.command(Trb::disable_slot(9)),
            Err(UsbError::Command(super::super::trb::SLOT_NOT_ENABLED))
        );
        assert!(
            xhci.hal()
                .log_text()
                .contains("command Disable Slot failed: slot not enabled")
        );
    }

    #[test]
    fn more_than_256_events_are_all_seen_and_erdp_is_written_with_ehb() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        let base = hal.fake().event_ring().unwrap().base;
        inject(&hal, 3);
        assert!(hal.fake().ehb());
        let writes = hal.fake().erdp_writes();
        xhci.poll();
        assert_eq!(hal.fake().erdp(), base + 3 * 16);
        assert!(!hal.fake().ehb(), "EHB cleared");
        assert_eq!(
            hal.fake().erdp_writes(),
            writes + 1,
            "one ERDP write per poll"
        );
        for _ in 0..3 {
            inject(&hal, 200);
            xhci.poll();
            assert_eq!(hal.fake().unconsumed_events(), 0);
            assert!(!hal.fake().ehb());
        }
        assert_eq!(hal.fake().erdp(), base + (603 % 256) * 16);
    }

    #[test]
    fn a_command_that_never_completes_is_aborted_and_the_next_gets_its_own_completion() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        let (hal, mut xhci) = start(config);
        let before = hal.clock();
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        let waited = hal.clock() - before;
        assert!(waited >= COMMAND_TIMEOUT && waited < COMMAND_TIMEOUT * 2);
        assert_eq!(hal.fake().aborts(), 1);
        assert_eq!(hal.fake().hung(), None, "the abort released the ring");
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: command Enable Slot timed out; aborted")
        );
        // Command Ring Stopped named the TRB after the aborted one, where
        // this command goes: its completion is its own, not that event.
        assert_eq!(
            no_op(&mut xhci),
            Ok(Completion {
                code: SUCCESS,
                slot: 0
            })
        );
        let executed = hal.fake().executed().to_vec();
        assert_eq!(executed.len(), 1);
        assert_eq!(executed[0].kind, NO_OP_COMMAND as u32);
        assert!(!hal.fake().slot_enabled(1), "Enable Slot never ran");
        assert!(!hal.log_text().contains("matches no command"));
    }

    #[test]
    fn a_controller_that_resumes_on_the_aborted_command_finds_a_no_op() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        config.abort_keeps_dequeue = true;
        let (hal, mut xhci) = start(config);
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        // The ring restarts on the old TRB, now a No Op, then runs the next.
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
        let executed = hal.fake().executed().to_vec();
        assert_eq!(executed.len(), 2);
        assert!(executed.iter().all(|e| e.kind == NO_OP_COMMAND as u32));
        assert!(!hal.fake().slot_enabled(1), "Enable Slot never ran");
    }

    #[test]
    fn an_abort_that_never_finishes_kills_and_halts_the_controller() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        config.abort_never_completes = true;
        let (hal, mut xhci) = start(config);
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        let log = hal.log_text();
        assert!(log.contains("command Enable Slot timed out and the command ring did not stop"));
        assert!(
            log.contains("xhci 00:14.0: controller dead; halted after 0 ms"),
            "{log}"
        );
        assert_eq!(hal.fake().usbcmd() & RUN, 0, "R/S cleared");
        assert!(!hal.fake().running());
        let before = hal.clock();
        assert_eq!(no_op(&mut xhci), Err(UsbError::ControllerDead));
        assert_eq!(hal.clock(), before, "failing at once");
    }

    #[test]
    fn a_completion_for_no_command_is_ignored() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.act(|x, dma| x.post_completion(0x1234_5670, 1, 0, dma));
        xhci.poll();
        assert!(
            hal.log_text()
                .contains("completion for command 0x12345670 matches no command (success)")
        );
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
    }

    #[test]
    fn a_host_system_error_marks_the_controller_dead() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().host_system_error();
        let before = hal.clock();
        xhci.poll();
        assert!(
            hal.clock() - before <= Duration::from_micros(5),
            "poll does not wait for the halt"
        );
        xhci.poll();
        assert_eq!(hal.fake().usbcmd() & RUN, 0, "R/S cleared");
        let log = hal.log_text();
        assert_eq!(log.matches("host system error").count(), 1, "logged once");
        assert_eq!(no_op(&mut xhci), Err(UsbError::ControllerDead));
    }

    #[test]
    fn poll_with_nothing_pending_returns_without_sleeping() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        let before = hal.clock();
        let writes = hal.fake().erdp_writes();
        xhci.poll();
        assert!(hal.clock() - before <= Duration::from_micros(5));
        assert_eq!(
            hal.fake().erdp_writes(),
            writes,
            "no ERDP write without events"
        );
    }
}
````

- [ ] **Step 7: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod caps;
mod context;
````

with:

````rust
mod caps;
mod command;
mod context;
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Xhci` in this scope ``; `` cannot find type `Completion` in this scope ``.

- [ ] **Step 9: Implement `crates/usb/src/xhci/command.rs`**

Insert this at the top of `crates/usb/src/xhci/command.rs`, above `#[cfg(test)]`:

````rust
//! Commands (xHCI 4.6) and events (xHCI 4.9.4): one command at a time on
//! the command ring, and `poll`, which drains the event ring and hands
//! each event to whoever waits for it.

use super::Xhci;
use super::init::{REGISTER_TIMEOUT, wait_for};
use super::regs::{ALL_ONES, CA, CRCR, EHB, ERDP, HCE, HCH, HSE, RUN, USBCMD, USBSTS};
use super::ring::TRBS;
use super::trb::{
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT, SUCCESS, Trb,
    command_name, completion_name,
};
use crate::{Hal, UsbError};
use core::sync::atomic::{Ordering, fence};
use core::time::Duration;

/// How long a command may take (spec §6.2).
pub const COMMAND_TIMEOUT: Duration = Duration::from_millis(500);
/// How often a command wait polls.
const COMMAND_POLL: Duration = Duration::from_micros(10);

/// What a Command Completion Event said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub code: u8,
    /// The slot concerned (Enable Slot's new slot).
    pub slot: u8,
}

/// The command in flight: its TRB's address, and its completion once the
/// event has come.
#[derive(Debug)]
pub struct Pending {
    trb: u64,
    done: Option<Completion>,
}

impl<H: Hal> Xhci<H> {
    /// Runs one command and waits up to 500 ms for its completion. On a
    /// timeout the command ring is aborted and the command turned into a
    /// No Op; if even the abort fails, the controller is dead.
    pub(super) fn command(&mut self, trb: Trb) -> Result<Completion, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let kind = trb.trb_type();
        let addr = self.commands.push(trb);
        self.pending = Some(Pending {
            trb: addr,
            done: None,
        });
        // The controller reads the TRB after it sees the doorbell.
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, 0, 0);
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                self.pending = None;
                return Err(UsbError::ControllerDead);
            }
            if let Some(done) = self.pending.as_ref().and_then(|p| p.done) {
                self.pending = None;
                if done.code == SUCCESS {
                    return Ok(done);
                }
                xlog!(
                    &self.hal,
                    &self.name,
                    "command {} failed: {}",
                    command_name(kind),
                    completion_name(done.code)
                );
                return Err(UsbError::Command(done.code));
            }
            if self.hal.now() - start >= COMMAND_TIMEOUT {
                break;
            }
            self.hal.sleep(COMMAND_POLL);
        }
        self.pending = None;
        self.abort_command(addr, kind);
        Err(UsbError::Timeout)
    }

    /// Aborts the command ring (xHCI 4.6.1.2) and polls up to 1 s for its
    /// Command Ring Stopped event. CRCR is written whole, CA with the
    /// timed-out TRB's address, as Linux does: some controllers act only
    /// once the high half is written. The controller may resume on the
    /// timed-out TRB or after it; as a No Op with the same cycle bit it does
    /// nothing either way.
    fn abort_command(&mut self, addr: u64, kind: u8) {
        self.ring_stopped = false;
        self.regs
            .op_write64(&self.hal, CRCR, addr & !0x3F | CA as u64);
        let start = self.hal.now();
        while !self.ring_stopped && !self.dead && self.hal.now() - start < REGISTER_TIMEOUT {
            self.poll();
            self.hal.sleep(COMMAND_POLL);
        }
        self.commands.replace(addr, Trb::no_op_command());
        if !self.ring_stopped {
            xlog!(
                &self.hal,
                &self.name,
                "command {} timed out and the command ring did not stop",
                command_name(kind)
            );
            self.controller_dead(true);
            return;
        }
        xlog!(
            &self.hal,
            &self.name,
            "command {} timed out; aborted",
            command_name(kind)
        );
    }

    /// Processes every pending event (command completions, transfer
    /// events, port status changes) and never waits.
    pub fn poll(&mut self) {
        if self.dead {
            return;
        }
        let sts = self.regs.op_read(&self.hal, USBSTS);
        if sts == ALL_ONES || sts & (HSE | HCE) != 0 {
            xlog!(
                &self.hal,
                &self.name,
                "USBSTS {sts:#010x}: host system error, controller error or gone"
            );
            // `poll` never waits: on a host system error the controller
            // clears R/S itself (xHCI 5.4.2), so it is only cleared here.
            self.controller_dead(false);
            return;
        }
        // At most one segment's worth, so a controller that keeps writing
        // cannot hold `poll` forever.
        let mut seen = 0;
        while seen < TRBS {
            let Some(event) = self.events.next() else {
                break;
            };
            seen += 1;
            self.handle_event(event);
        }
        if seen > 0 {
            // EHB is RW1C: writing 1 clears it (xHCI 5.5.2.3.3).
            let erdp = self.events.dequeue_pointer() | EHB;
            self.regs.intr_write64(&self.hal, ERDP, erdp);
        }
    }

    /// The controller stopped working: nothing more is sent to it, and it
    /// is halted (R/S = 0; with `wait`, up to 1 s for HCH) so it stops using
    /// memory. Its DMA memory is never freed after this.
    fn controller_dead(&mut self, wait: bool) {
        self.dead = true;
        let cmd = self.regs.op_read(&self.hal, USBCMD);
        if cmd == ALL_ONES {
            xlog!(&self.hal, &self.name, "controller dead and gone");
            return;
        }
        self.regs.op_write(&self.hal, USBCMD, cmd & !RUN);
        if !wait {
            xlog!(&self.hal, &self.name, "controller dead; R/S cleared");
            return;
        }
        let (hal, regs) = (&self.hal, &self.regs);
        match wait_for(hal, REGISTER_TIMEOUT, || {
            regs.op_read(hal, USBSTS) & HCH != 0
        }) {
            Some(t) => xlog!(
                &self.hal,
                &self.name,
                "controller dead; halted after {} ms",
                t.as_millis()
            ),
            None => xlog!(
                &self.hal,
                &self.name,
                "controller dead; USBSTS.HCH still 0 {} ms after R/S = 0",
                REGISTER_TIMEOUT.as_millis()
            ),
        }
    }

    fn handle_event(&mut self, event: Trb) {
        match event.trb_type() {
            COMMAND_COMPLETION => self.command_completed(event),
            HOST_CONTROLLER_EVENT => xlog!(
                &self.hal,
                &self.name,
                "host controller event: {}",
                completion_name(event.completion_code())
            ),
            _ => {}
        }
    }

    fn command_completed(&mut self, event: Trb) {
        let done = Completion {
            code: event.completion_code(),
            slot: event.slot_id(),
        };
        // An abort's own events never complete a command, whatever they
        // point at: Command Ring Stopped names the TRB after the aborted
        // one, where the next command goes (xHCI 4.6.1.2).
        match done.code {
            COMMAND_RING_STOPPED => {
                self.ring_stopped = true;
                return;
            }
            COMMAND_ABORTED => return,
            _ => {}
        }
        match &mut self.pending {
            Some(p) if p.trb == event.pointer() && p.done.is_none() => p.done = Some(done),
            _ => xlog!(
                &self.hal,
                &self.name,
                "completion for command {:#x} matches no command ({})",
                event.pointer(),
                completion_name(done.code)
            ),
        }
    }
}

````

- [ ] **Step 10: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use caps::PortProtocol;
use core::fmt;
````

with:

````rust
use caps::PortProtocol;
use command::Pending;
use core::fmt;
````

Replace:

````rust
    events: EventRing,
}
````

with:

````rust
    events: EventRing,
    /// The command in flight (one at a time).
    pending: Option<Pending>,
    /// A Command Ring Stopped event came since the last abort.
    ring_stopped: bool,
    /// Set when the controller stopped working: nothing is sent to it any
    /// more (`UsbError::ControllerDead`).
    dead: bool,
}
````

- [ ] **Step 11: Change `crates/usb/src/xhci/start.rs`**

In `crates/usb/src/xhci/start.rs`, replace:

````rust
            events: mem.events,
        };
````

with:

````rust
            events: mem.events,
            pending: None,
            ring_stopped: false,
            dead: false,
        };
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 121 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -m "usb: xHCI commands with timeouts and aborts, event ring polling"
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 10 scenario(s) passed`.

````bash
git push -u origin plan4/xhci-start
gh pr create --base main --head plan4/xhci-start --title "Plan 4: Starting the xHCI controller" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 7–9: BIOS handoff (with the takeover after 1 s), halt and reset (1 ms after HCRST, HCRST and CNR), scratchpad buffers, DCBAA, command and event rings, run and port power in `Xhci::new`, freeing everything on failure; commands with 500 ms timeouts, command ring abort and No Op replacement; `poll`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/xhci-start --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-start
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: xHCI devices (Tasks 10–12)

Root ports, enumeration, control transfers, endpoint configuration and IN transfers: the controller side of setting up a device and talking to it, and the `Bus` class drivers use.

Branch `plan4/xhci-devices`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-devices`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/xhci-devices /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-devices origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-devices
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/xhci-start` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/xhci-devices /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-devices plan4/xhci-start`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/xhci-start>` and re-run `cargo xtask ci` before pushing.

### Task 10: Root ports: reset for USB 2 and USB 3, port changes

Spec §6.2's port quirk: the Supported Protocol map decides the reset. USB 2 ports get a PORTSC reset, 500 ms at most for Port Reset Change, then Port Enabled and 10 ms of reset recovery; USB 3 ports train on their own (500 ms for Port Enabled with the link in U0) and then get a hot reset, as Linux does, because a device the firmware used (the stick the NUC booted from) may keep its old address otherwise; a link that does not train, or sits in SS.Inactive, gets one warm reset instead. A USB 2 port never gets a warm reset. At the end of `Xhci::new` the driver also waits, up to 1 s in all, while a USB 3 link is still training (its port shows no connection until then). PORTSC mixes write-one-to-clear bits (writing 1 to Port Enabled *disables* the port) with ones to keep, so every write starts from a neutral value and adds only what it means to change. `port_changes` lists the ports whose connection changed since the last call, with their change bits cleared; right after start every connected port counts as changed, so boot-time and hot-plugged devices take the same path (decision 4). The fake devices start here: `FakeUsbDevice` with presets modelled on the K120, the Unifying receiver, QEMU's keyboard and the two sticks. The fake makes devices present at a reset or power-on appear only after a while (30 ms for USB 2, the link training time for USB 3), as hardware does.

**Files:**
- Create: `crates/usb/src/testing/device.rs`
- Modify: `crates/usb/src/testing/mod.rs`
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Create: `crates/usb/src/testing/xhci/ports.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Modify: `crates/usb/src/xhci/command.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Create: `crates/usb/src/xhci/port.rs`
- Modify: `crates/usb/src/xhci/start.rs`

**Interfaces:**
- Consumes: Tasks 6–9.
- Produces: `usb::xhci::{PortChange { port, connected, reconnected }, Xhci::port_changes(&mut self) -> Vec<PortChange>}`, crate-internal `Xhci::reset_port(&mut self, port: u8) -> Result<Speed, UsbError>`, `xhci::port::PORT_RESET_TIMEOUT = 500 ms`; test support `testing::FakeUsbDevice::{k120, unifying_receiver, qemu_keyboard, kingston_stick, usb2_stick, with_speed, push_in, requests, stall_request, ignore_requests, …}` and `FakeXhci::{plug(port, device), unplug(port)}`.

- [ ] **Step 1: Create `crates/usb/src/testing/device.rs`**

The fake devices plugged into the fake controller's ports; tests share them with the controller as `Rc<RefCell<FakeUsbDevice>>`.

Create `crates/usb/src/testing/device.rs`:

````rust
//! Fake USB devices: what the fake controller talks to on its ports.

use crate::Speed;
use std::cell::RefCell;
use std::rc::Rc;

/// A device as the fake controller sees it on a port.
pub trait FakeDevice {
    fn speed(&self) -> Speed;
}

/// A configurable device with its descriptors, modelled on real ones.
pub struct FakeUsbDevice {
    speed: Speed,
    /// The device descriptor (18 bytes).
    device: Vec<u8>,
    /// The whole configuration descriptor.
    configuration: Vec<u8>,
}

/// The Logitech K120's configuration (as in `descriptor.rs`'s tests): a
/// boot keyboard and a second HID interface for its extra keys.
pub const K120_CONFIGURATION: [u8; 59] = [
    9, 2, 59, 0, 2, 1, 0, 0xA0, 50, // configuration 1, 2 interfaces
    9, 4, 0, 0, 1, 3, 1, 1, 0, // interface 0: HID, boot, keyboard
    9, 0x21, 0x10, 1, 0, 1, 0x22, 65, 0, // HID descriptor
    7, 5, 0x81, 3, 8, 0, 10, // endpoint 1 IN, interrupt, 8 bytes, 10 ms
    9, 4, 1, 0, 1, 3, 0, 0, 0, // interface 1: HID, no boot protocol
    9, 0x21, 0x10, 1, 0, 1, 0x22, 51, 0, // HID descriptor
    7, 5, 0x82, 3, 4, 0, 0xFF, // endpoint 2 IN, interrupt, 4 bytes
];

/// A device descriptor: `bcdUSB`, EP0's size, vendor and product.
fn device_descriptor(usb: u16, max_packet0: u8, vendor: u16, product: u16) -> Vec<u8> {
    let [u0, u1] = usb.to_le_bytes();
    let [v0, v1] = vendor.to_le_bytes();
    let [p0, p1] = product.to_le_bytes();
    vec![
        18,
        1,
        u0,
        u1,
        0,
        0,
        0,
        max_packet0,
        v0,
        v1,
        p0,
        p1,
        0,
        1,
        1,
        2,
        3,
        1,
    ]
}

/// A configuration descriptor around `body` (interfaces and endpoints).
fn configuration(interfaces: u8, body: &[u8]) -> Vec<u8> {
    let total = (9 + body.len()) as u16;
    let [t0, t1] = total.to_le_bytes();
    let mut c = vec![9, 2, t0, t1, interfaces, 1, 0, 0x80, 50];
    c.extend_from_slice(body);
    c
}

impl FakeUsbDevice {
    pub fn new(
        speed: Speed,
        device: Vec<u8>,
        configuration: Vec<u8>,
    ) -> Rc<RefCell<FakeUsbDevice>> {
        Rc::new(RefCell::new(FakeUsbDevice {
            speed,
            device,
            configuration,
        }))
    }

    /// The NUC's keyboard: Logitech K120, low-speed, `046d:c31c`.
    pub fn k120() -> Rc<RefCell<FakeUsbDevice>> {
        FakeUsbDevice::new(
            Speed::Low,
            device_descriptor(0x0110, 8, 0x046D, 0xC31C),
            K120_CONFIGURATION.to_vec(),
        )
    }

    /// Logitech Unifying receiver, full-speed, `046d:c534`, EP0 64 bytes,
    /// three HID interfaces: keyboard, mouse and a vendor one.
    pub fn unifying_receiver() -> Rc<RefCell<FakeUsbDevice>> {
        let hid = |n: u8, sub: u8, proto: u8, ep: u8, size: u8| {
            let mut d = vec![9, 4, n, 0, 1, 3, sub, proto, 0];
            d.extend_from_slice(&[9, 0x21, 0x11, 1, 0, 1, 0x22, 59, 0]);
            d.extend_from_slice(&[7, 5, ep, 3, size, 0, 8]);
            d
        };
        let body = [
            hid(0, 1, 1, 0x81, 8),
            hid(1, 1, 2, 0x82, 8),
            hid(2, 0, 0, 0x83, 32),
        ]
        .concat();
        FakeUsbDevice::new(
            Speed::Full,
            device_descriptor(0x0200, 64, 0x046D, 0xC534),
            configuration(3, &body),
        )
    }

    /// QEMU 8.2's usb-kbd on an xHCI: high-speed, `0627:0001`, USB 2.00,
    /// EP0 64 bytes, one boot keyboard with interrupt IN 0x81 of 8 bytes
    /// and bInterval 7 (2^6 microframes: 8 ms).
    pub fn qemu_keyboard() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 1, 3, 1, 1, 0, //
            9, 0x21, 0x11, 1, 0, 1, 0x22, 63, 0, //
            7, 5, 0x81, 3, 8, 0, 7,
        ];
        FakeUsbDevice::new(
            Speed::High,
            device_descriptor(0x0200, 64, 0x0627, 0x0001),
            configuration(1, &body),
        )
    }

    /// The NUC's stick: Kingston DataTraveler 3.0, SuperSpeed,
    /// `0951:1666`, mass storage (8/6/0x50), bulk IN and OUT of 1024 bytes
    /// with a burst of 4 (companion bMaxBurst 3).
    pub fn kingston_stick() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 4, 0, //
            6, 0x30, 3, 0, 0, 0, //
            7, 5, 0x02, 2, 0, 4, 0, //
            6, 0x30, 3, 0, 0, 0,
        ];
        FakeUsbDevice::new(
            Speed::Super,
            device_descriptor(0x0320, 9, 0x0951, 0x1666),
            configuration(1, &body),
        )
    }

    /// A USB 2 stick: high-speed mass storage with bulk endpoints of 512.
    pub fn usb2_stick() -> Rc<RefCell<FakeUsbDevice>> {
        let body = [
            9, 4, 0, 0, 2, 8, 6, 0x50, 0, //
            7, 5, 0x81, 2, 0, 2, 0, //
            7, 5, 0x02, 2, 0, 2, 0,
        ];
        FakeUsbDevice::new(
            Speed::High,
            device_descriptor(0x0200, 64, 0x0951, 0x1665),
            configuration(1, &body),
        )
    }

    /// A device of `speed` with the K120's descriptors otherwise.
    pub fn with_speed(speed: Speed) -> Rc<RefCell<FakeUsbDevice>> {
        let dev = FakeUsbDevice::k120();
        dev.borrow_mut().speed = speed;
        let mps = speed.default_max_packet0();
        dev.borrow_mut().device[7] = if speed.is_superspeed() { 9 } else { mps as u8 };
        dev
    }

    pub fn device_descriptor(&self) -> &[u8] {
        &self.device
    }

    pub fn configuration_descriptor(&self) -> &[u8] {
        &self.configuration
    }
}

impl FakeDevice for FakeUsbDevice {
    fn speed(&self) -> Speed {
        self.speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{DeviceDescriptor, parse_configuration};

    #[test]
    fn the_presets_have_valid_descriptors() {
        for (dev, vendor, product, interfaces) in [
            (FakeUsbDevice::k120(), 0x046D, 0xC31C, 2),
            (FakeUsbDevice::unifying_receiver(), 0x046D, 0xC534, 3),
            (FakeUsbDevice::qemu_keyboard(), 0x0627, 0x0001, 1),
            (FakeUsbDevice::kingston_stick(), 0x0951, 0x1666, 1),
            (FakeUsbDevice::usb2_stick(), 0x0951, 0x1665, 1),
        ] {
            let d = dev.borrow();
            let desc = DeviceDescriptor::parse(d.device_descriptor()).unwrap();
            assert_eq!((desc.vendor, desc.product), (vendor, product));
            let c = parse_configuration(d.configuration_descriptor()).unwrap();
            assert_eq!(c.interfaces.len(), interfaces);
        }
        let stick = FakeUsbDevice::kingston_stick();
        let c = parse_configuration(stick.borrow().configuration_descriptor()).unwrap();
        assert_eq!(c.interfaces[0].endpoints[0].max_burst, 3);
    }
}
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/mod.rs`**

In `crates/usb/src/testing/mod.rs`, replace:

````rust

mod hal;
mod xhci;

pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
````

with:

````rust

mod device;
mod hal;
mod xhci;

pub use device::FakeUsbDevice;
pub use hal::{FAKE_BAR, FAKE_BAR_LEN, FakeHal};
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
mod commands;
mod regs;
````

with:

````rust
mod commands;
mod ports;
mod regs;
````

Replace:

````rust
    pub abort_keeps_dequeue: bool,
}
````

with:

````rust
    pub abort_keeps_dequeue: bool,
    /// How long a port reset takes; `None`: it never completes.
    pub port_reset_time: Option<Duration>,
    /// How long a USB 3 link trains (in Polling, CCS 0) after a connect.
    pub usb3_training: Duration,
    /// Knob: USB 3 links fail to train and end in SS.Inactive (CCS 1),
    /// where only a warm reset helps.
    pub usb3_link_fails: bool,
    /// How long a USB 2 device present when its port is reset or powered
    /// takes to signal its attach (at most 100 ms, USB 2.0 7.1.7.3).
    pub usb2_attach_delay: Duration,
}
````

Replace:

````rust
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
        }
    }

````

with:

````rust
            hang_command: None,
            abort_never_completes: false,
            abort_keeps_dequeue: false,
            port_reset_time: Some(ports::RESET_TIME),
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
        }
    }

````

Replace:

````rust
            abort_keeps_dequeue: false,
        }
````

with:

````rust
            abort_keeps_dequeue: false,
            port_reset_time: Some(ports::RESET_TIME),
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
        }
````

Replace:

````rust
    port_writes: usize,
    iman: u32,
````

with:

````rust
    port_writes: usize,
    portsc_writes: Vec<(u8, u32)>,
    /// Which ports are USB 3 (from the Supported Protocol capabilities).
    usb3: Vec<bool>,
    devices: Vec<Option<ports::Device>>,
    /// Bumped by every connect, disconnect and reset, so a timer set for
    /// an earlier state does nothing.
    port_generation: Vec<u64>,
    reset_done: Vec<Option<Duration>>,
    iman: u32,
````

Replace:

````rust
            port_writes: 0,
            iman: 0,
````

with:

````rust
            port_writes: 0,
            portsc_writes: Vec::new(),
            usb3: vec![false; ports],
            devices: vec![None; ports],
            port_generation: vec![0; ports],
            reset_done: vec![None; ports],
            iman: 0,
````

Replace:

````rust
        x.slots = (0..=x.config.max_slots).map(|_| None).collect();
        x.power_on_ports();
````

with:

````rust
        x.slots = (0..=x.config.max_slots).map(|_| None).collect();
        for cap in &x.config.caps {
            if let ExtCap::Protocol {
                major: 3,
                first,
                count,
                ..
            } = cap.cap
            {
                for p in first..first.saturating_add(count) {
                    if let Some(u) = (p as usize).checked_sub(1).and_then(|i| x.usb3.get_mut(i)) {
                        *u = true;
                    }
                }
            }
        }
        x.power_on_ports();
````

- [ ] **Step 4: Create `crates/usb/src/testing/xhci/ports.rs`**

Create `crates/usb/src/testing/xhci/ports.rs`:

````rust
//! Root ports (xHCI 4.19, 5.4.8): PORTSC with its RW1C, RWS and RO bits,
//! USB 2 port reset, USB 3 link training, hot and warm reset, and devices
//! being plugged and unplugged. Every change bit rising from an all-clear
//! state posts a Port Status Change Event. A device present when its port
//! is reset or powered takes a while to show: a USB 2 device signals its
//! attach after `usb2_attach_delay`, a USB 3 link trains in Polling with
//! CCS 0 for `usb3_training`.

use super::FakeXhci;
use super::commands::{PORT_STATUS_CHANGE, SUCCESS};
use crate::testing::device::FakeDevice;
use crate::testing::hal::Dma;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

pub const CCS: u32 = 1 << 0;
pub const PED: u32 = 1 << 1;
pub const PR: u32 = 1 << 4;
const PLS: u32 = 0xF << 5;
pub const PP: u32 = 1 << 9;
const SPEED: u32 = 0xF << 10;
const PIC: u32 = 3 << 14;
const LWS: u32 = 1 << 16;
pub const CSC: u32 = 1 << 17;
pub const WRC: u32 = 1 << 19;
pub const PRC: u32 = 1 << 21;
/// CSC, PEC, WRC, OCC, PRC, PLC, CEC: RW1C.
pub const CHANGES: u32 = 0x7F << 17;
const WAKE: u32 = 7 << 25;
pub const WPR: u32 = 1 << 31;

// Port link states.
const U0: u32 = 0;
const RX_DETECT: u32 = 5;
const SS_INACTIVE: u32 = 6;
const POLLING: u32 = 7;

/// How long a port reset or warm reset takes.
pub const RESET_TIME: Duration = Duration::from_millis(10);

pub type Device = Rc<RefCell<dyn FakeDevice>>;

fn with_pls(sc: u32, pls: u32) -> u32 {
    sc & !PLS | pls << 5
}

/// The Protocol Speed ID of the default table (xHCI 7.2.2.1.1).
fn speed_id(speed: crate::Speed) -> u32 {
    use crate::Speed::*;
    match speed {
        Full => 1,
        Low => 2,
        High => 3,
        Super => 4,
        SuperPlus => 5,
    }
}

impl FakeXhci {
    /// Connects `device` to `port` (1-based): CCS and CSC, and an event.
    pub fn plug(&mut self, port: u8, device: Device) {
        let i = port as usize - 1;
        let speed = device.borrow().speed();
        if speed.is_superspeed() != self.usb3[i] {
            panic!("fake xhci: a {speed} device cannot be on port {port}");
        }
        assert!(self.devices[i].is_none(), "fake: port {port} is taken");
        self.devices[i] = Some(device);
        if self.portsc[i] & PP != 0 {
            self.connect(i, false);
        }
    }

    /// Disconnects `port`: CCS and PED clear, CSC set, an event; the
    /// device's transfers in progress fail.
    pub fn unplug(&mut self, port: u8) {
        let i = port as usize - 1;
        if self.devices[i].take().is_none() {
            return;
        }
        self.port_generation[i] += 1;
        let was = self.portsc[i];
        let sc = was & !(CCS | PED | PR | SPEED);
        self.portsc[i] = with_pls(sc, RX_DETECT);
        if was & CCS != 0 {
            self.set_changes(i, CSC);
        }
    }

    pub fn device(&self, port: u8) -> Option<Device> {
        self.devices[port as usize - 1].clone()
    }

    /// When the last reset of `port` completed.
    pub fn reset_done_at(&self, port: u8) -> Option<Duration> {
        self.reset_done[port as usize - 1]
    }

    /// Every PORTSC write so far: port and value.
    pub fn portsc_writes(&self) -> &[(u8, u32)] {
        &self.portsc_writes
    }

    /// A device on a powered port. `boot`: the port was just reset or
    /// powered, so a USB 2 device takes `usb2_attach_delay` to signal its
    /// attach (USB 2.0 7.1.7.3). A USB 3 link trains first, in Polling with
    /// CCS 0 (xHCI 4.19.1.2), then is enabled in U0, or ends in SS.Inactive
    /// if it fails to train.
    fn connect(&mut self, i: usize, boot: bool) {
        self.port_generation[i] += 1;
        let generation = self.port_generation[i];
        if self.usb3[i] {
            self.portsc[i] = with_pls(self.portsc[i] & !CCS, POLLING);
            self.after(self.config.usb3_training, move |x, _| {
                x.trained(i, generation)
            });
        } else if boot {
            self.after(self.config.usb2_attach_delay, move |x, _| {
                if x.port_generation[i] == generation && x.devices[i].is_some() {
                    x.attached(i);
                }
            });
        } else {
            self.attached(i);
        }
    }

    /// A USB 2 device signalled its attach: connected, not yet enabled.
    fn attached(&mut self, i: usize) {
        self.portsc[i] = with_pls(self.portsc[i] | CCS, POLLING);
        self.set_changes(i, CSC);
    }

    /// The end of USB 3 link training.
    fn trained(&mut self, i: usize, generation: u64) {
        if self.port_generation[i] != generation || self.devices[i].is_none() {
            return;
        }
        if self.config.usb3_link_fails {
            self.portsc[i] = with_pls(self.portsc[i] | CCS, SS_INACTIVE);
        } else {
            self.enable(i, generation);
        }
        self.set_changes(i, CSC);
    }

    /// The link is up: connected, enabled, in U0, at the device's speed.
    fn enable(&mut self, i: usize, generation: u64) {
        if self.port_generation[i] != generation {
            return;
        }
        let Some(dev) = &self.devices[i] else {
            return;
        };
        let speed = speed_id(dev.borrow().speed());
        let sc = self.portsc[i] & !SPEED | CCS | PED | speed << 10;
        self.portsc[i] = with_pls(sc, U0);
    }

    /// Sets change bits; an event when none was set before (xHCI 4.19.2).
    fn set_changes(&mut self, i: usize, bits: u32) {
        let before = self.portsc[i] & CHANGES;
        self.portsc[i] |= bits;
        if before == 0 {
            self.port_event(i);
        }
    }

    fn port_event(&mut self, i: usize) {
        let trb = [
            ((i + 1) as u32) << 24,
            0,
            SUCCESS << 24,
            PORT_STATUS_CHANGE << 10,
        ];
        self.pending_events.push(trb);
    }

    /// Reconnects the devices on powered ports (after power-on or HCRST).
    pub(super) fn reconnect_ports(&mut self) {
        for i in 0..self.portsc.len() {
            if self.portsc[i] & PP != 0 && self.devices[i].is_some() {
                self.connect(i, true);
            }
        }
    }

    pub(super) fn write_portsc(&mut self, i: usize, value: u32, _dma: &Dma) {
        self.port_writes += 1;
        self.portsc_writes.push(((i + 1) as u8, value));
        if value & LWS != 0 {
            panic!("fake xhci: port {} link state written", i + 1);
        }
        let old = self.portsc[i];
        // Change bits and PED are RW1C: a 1 clears them, and a 1 in PED
        // disables the port (xHCI 4.19.1.1).
        let mut sc = old & !(value & (CHANGES | PED));
        sc = sc & !(PIC | WAKE) | value & (PIC | WAKE);
        self.portsc[i] = sc;
        if self.config.ppc && value & PP == 0 && old & PP != 0 {
            panic!("fake xhci: port {} powered off by a PORTSC write", i + 1);
        }
        if self.config.ppc && value & PP != 0 && old & PP == 0 {
            self.portsc[i] |= PP;
            self.powered_at = Some(self.now);
            if self.devices[i].is_some() {
                self.connect(i, true);
            }
        }
        if value & WPR != 0 {
            if !self.usb3[i] {
                panic!("fake xhci: warm reset on USB 2 port {}", i + 1);
            }
            self.start_reset(i, true);
        } else if value & PR != 0 {
            self.start_reset(i, false);
        }
    }

    /// PR or WPR: the port is disabled while the reset runs; afterwards
    /// PRC (and WRC) is set and, with a device, the port is enabled (on a
    /// USB 3 port: in U0).
    fn start_reset(&mut self, i: usize, warm: bool) {
        if self.portsc[i] & PP == 0 {
            return;
        }
        self.portsc[i] = self.portsc[i] & !PED | PR;
        let Some(delay) = self.config.port_reset_time else {
            return;
        };
        self.port_generation[i] += 1;
        let generation = self.port_generation[i];
        self.after(delay, move |x, _| {
            x.portsc[i] &= !PR;
            x.reset_done[i] = Some(x.now);
            if x.devices[i].is_some() && x.port_generation[i] == generation {
                x.enable(i, generation);
            }
            x.set_changes(i, if warm { PRC | WRC } else { PRC });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::FakeConfig;
    use super::*;
    use crate::testing::device::FakeUsbDevice;

    fn basic() -> FakeXhci {
        FakeXhci::new(FakeConfig::basic())
    }

    #[test]
    fn a_plugged_device_is_connected_but_not_enabled() {
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        assert_eq!(x.portsc(1) & (CCS | PED | CSC), CCS | CSC);
        assert_eq!(x.pending_events.len(), 1);
        x.plug(2, FakeUsbDevice::k120());
        x.unplug(2);
        assert_eq!(x.pending_events.len(), 2, "CSC was already set: one event");
    }

    #[test]
    fn a_usb2_reset_enables_the_port_after_10_ms() {
        let dma = Dma::default();
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        x.write_portsc(0, PP | PR, &dma);
        assert_eq!(x.portsc(1) & (PR | PED), PR);
        x.advance_to(RESET_TIME, &dma);
        assert_eq!(x.portsc(1) & (PR | PED | PRC), PED | PRC);
        assert_eq!(x.portsc(1) >> 10 & 0xF, 2, "low-speed");
    }

    #[test]
    fn writing_ped_disables_the_port_and_change_bits_clear_on_1() {
        let dma = Dma::default();
        let mut x = basic();
        x.plug(1, FakeUsbDevice::k120());
        x.write_portsc(0, PP | PR, &dma);
        x.advance_to(RESET_TIME, &dma);
        x.write_portsc(0, PP | CSC, &dma);
        assert_eq!(x.portsc(1) & (PED | CSC | PRC), PED | PRC);
        x.write_portsc(0, x.portsc(1), &dma);
        assert_eq!(
            x.portsc(1) & (PED | CHANGES),
            0,
            "writing back what was read"
        );
    }

    #[test]
    #[should_panic(expected = "warm reset on USB 2 port 2")]
    fn usb2_ports_have_no_warm_reset() {
        basic().write_portsc(1, PP | WPR, &Dma::default());
    }

    #[test]
    #[should_panic(expected = "cannot be on port 5")]
    fn a_usb2_device_does_not_fit_a_usb3_port() {
        basic().plug(5, FakeUsbDevice::k120());
    }
}
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

    pub fn read(&mut self, offset: usize, _dma: &Dma) -> u32 {
        self.check_access(offset);
        self.highest_read = self.highest_read.max(offset);
````

with:

````rust

    pub fn read(&mut self, offset: usize, dma: &Dma) -> u32 {
        self.check_access(offset);
        // Whatever happened since the last tick is visible now.
        self.flush_events(dma);
        self.highest_read = self.highest_read.max(offset);
````

Replace:

````rust
        self.power_on_ports();
        (self.iman, self.imod, self.erstsz, self.erstba) = (0, 0, 0, 0);
````

with:

````rust
        self.power_on_ports();
        self.reconnect_ports();
        (self.iman, self.imod, self.erstsz, self.erstba) = (0, 0, 0, 0);
````

Replace:

````rust
                if reg == 0 {
                    self.write_portsc(port, value);
                }
````

with:

````rust
                if reg == 0 {
                    self.write_portsc(port, value, dma);
                }
````

Replace:

````rust
            _ => {}
        }
    }

    /// Port power; the rest of PORTSC comes with the port model.
    fn write_portsc(&mut self, index: usize, value: u32) {
        self.port_writes += 1;
        if self.config.ppc && value & PP != 0 && self.portsc[index] & PP == 0 {
            self.portsc[index] |= PP;
            self.powered_at = Some(self.now);
        }
````

with:

````rust
            _ => {}
        }
````

- [ ] **Step 6: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod init;
mod regs;
````

with:

````rust
mod init;
mod port;
mod regs;
````

- [ ] **Step 7: Write the failing tests for `crates/usb/src/xhci/port.rs`**

Create `crates/usb/src/xhci/port.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::regs::PP;
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec;

    fn change(port: u8, connected: bool, reconnected: bool) -> PortChange {
        PortChange {
            port,
            connected,
            reconnected,
        }
    }

    fn wpr_writes(hal: &FakeHal) -> usize {
        hal.fake()
            .portsc_writes()
            .iter()
            .filter(|(_, v)| v & WPR != 0)
            .count()
    }

    #[test]
    fn a_usb2_reset_gives_each_device_its_speed_and_leaves_the_port_enabled() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        for (port, speed) in [(1, Speed::Low), (2, Speed::Full), (3, Speed::High)] {
            hal.fake().plug(port, FakeUsbDevice::with_speed(speed));
            assert_eq!(xhci.reset_port(port), Ok(speed));
            let sc = hal.fake().portsc(port);
            assert_eq!(
                sc & (PED | PR | CHANGE_BITS),
                PED | CSC,
                "port {port}: only CSC left"
            );
            let done = hal.fake().reset_done_at(port).unwrap();
            assert!(
                hal.clock() - done >= RESET_RECOVERY,
                "port {port}: reset recovery"
            );
        }
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 1: reset done, low-speed")
        );
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: reset done, high-speed")
        );
        assert_eq!(wpr_writes(&hal), 0, "a USB 2 port never gets WPR");
    }

    #[test]
    fn clearing_change_bits_leaves_the_port_enabled() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        xhci.reset_port(3).unwrap();
        assert_eq!(xhci.port_changes(), vec![change(3, true, true)]);
        let sc = hal.fake().portsc(3);
        assert_eq!(sc & (PED | PP | CHANGE_BITS), PED | PP);
    }

    fn pr_writes(hal: &FakeHal, port: u8) -> usize {
        let writes = hal.fake().portsc_writes().to_vec();
        writes
            .iter()
            .filter(|&&(p, v)| p == port && v & PR != 0)
            .count()
    }

    /// Lets a USB 3 link plugged just now train (the fake takes 50 ms).
    fn train(hal: &FakeHal) {
        hal.sleep(Duration::from_millis(60));
    }

    #[test]
    fn a_usb3_link_trains_by_itself_and_then_gets_a_hot_reset() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(13, FakeUsbDevice::kingston_stick());
        assert_eq!(
            hal.fake().portsc(13) & CCS,
            0,
            "no connection while training"
        );
        train(&hal);
        assert_eq!(xhci.reset_port(13), Ok(Speed::Super));
        assert_eq!(pr_writes(&hal, 13), 1, "one hot reset");
        assert_eq!(wpr_writes(&hal), 0);
        let sc = hal.fake().portsc(13);
        assert_eq!(sc & (PED | CHANGE_BITS), PED | CSC, "enabled, PRC cleared");
        assert!(hal.log_text().contains("port 13: reset done, SuperSpeed"));
    }

    #[test]
    fn a_usb3_link_that_fails_to_train_gets_one_warm_reset() {
        let mut config = FakeConfig::intel();
        config.usb3_link_fails = true;
        let (hal, mut xhci) = start(config);
        hal.fake().plug(14, FakeUsbDevice::kingston_stick());
        train(&hal);
        assert_eq!(
            xhci.port_changes(),
            vec![change(14, true, true)],
            "SS.Inactive, CCS 1"
        );
        let before = hal.clock();
        assert_eq!(xhci.reset_port(14), Ok(Speed::Super));
        assert!(
            hal.clock() - before < Duration::from_millis(50),
            "no wait in SS.Inactive"
        );
        assert_eq!(wpr_writes(&hal), 1);
        assert_eq!(pr_writes(&hal, 14), 0, "the warm reset reset the device");
        assert_eq!(
            hal.fake().portsc(14) & CHANGE_BITS,
            0,
            "PRC and WRC cleared"
        );
        assert!(
            hal.log_text()
                .contains("port 14: link not trained (PLS 6), warm reset")
        );
    }

    #[test]
    fn devices_present_at_boot_are_in_the_first_port_changes() {
        for training in [50, 300] {
            let mut config = FakeConfig::intel();
            config.usb3_training = Duration::from_millis(training);
            let hal = FakeHal::with_controller(config);
            hal.fake().plug(3, FakeUsbDevice::k120());
            hal.fake().plug(13, FakeUsbDevice::kingston_stick());
            let mut xhci = Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
            assert_eq!(
                xhci.port_changes(),
                vec![change(3, true, true), change(13, true, true)],
                "USB 3 training for {training} ms"
            );
            let waited = hal.clock() - hal.fake().powered_at().unwrap();
            let want = Duration::from_millis(training.max(100));
            assert!(waited >= want && waited < want + Duration::from_millis(5));
        }
        // A link that never trains is given up after 1 s.
        let mut config = FakeConfig::intel();
        config.usb3_training = Duration::from_secs(5);
        let hal = FakeHal::with_controller(config);
        hal.fake().plug(13, FakeUsbDevice::kingston_stick());
        Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
        assert!(hal.clock() < Duration::from_millis(1050));
        assert!(
            hal.log_text()
                .contains("port 13: USB 3 link still training after")
        );
    }

    #[test]
    fn a_reset_that_never_completes_times_out_after_500_ms() {
        let mut config = FakeConfig::basic();
        config.port_reset_time = None;
        let (hal, mut xhci) = start(config);
        hal.fake().plug(2, FakeUsbDevice::qemu_keyboard());
        let before = hal.clock();
        assert_eq!(xhci.reset_port(2), Err(UsbError::Timeout));
        let waited = hal.clock() - before;
        assert!(waited >= PORT_RESET_TIMEOUT && waited < PORT_RESET_TIMEOUT * 2);
        assert!(hal.log_text().contains("port 2: reset failed, PORTSC 0x"));
        assert_eq!(wpr_writes(&hal), 0, "a USB 2 port never gets WPR");
    }

    #[test]
    fn a_device_unplugged_during_reset_is_disconnected() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.fake().plug(1, FakeUsbDevice::k120());
        hal.fake()
            .after(Duration::from_millis(3), |x, _| x.unplug(1));
        assert_eq!(xhci.reset_port(1), Err(UsbError::Disconnected));
        assert!(hal.log_text().contains("port 1: disconnected during reset"));
        hal.fake().plug(5, FakeUsbDevice::kingston_stick());
        train(&hal);
        hal.fake()
            .after(Duration::from_millis(3), |x, _| x.unplug(5));
        assert_eq!(xhci.reset_port(5), Err(UsbError::Disconnected));
    }

    #[test]
    fn right_after_start_every_connected_port_is_listed_once() {
        for config in [FakeConfig::basic(), FakeConfig::intel()] {
            let hal = FakeHal::with_controller(config.clone());
            let usb3 = if config.ppc { 13 } else { 5 };
            hal.fake().plug(1, FakeUsbDevice::k120());
            hal.fake().plug(3, FakeUsbDevice::usb2_stick());
            hal.fake().plug(usb3, FakeUsbDevice::kingston_stick());
            let mut xhci = Xhci::new(hal.clone(), crate::testing::FAKE_BAR, 0x1_0000, "x").unwrap();
            assert_eq!(
                xhci.port_changes(),
                vec![
                    change(1, true, true),
                    change(3, true, true),
                    change(usb3, true, true)
                ]
            );
            assert_eq!(xhci.port_changes(), vec![]);
            for port in [1, 3, usb3] {
                assert_eq!(hal.fake().portsc(port) & CHANGE_BITS, 0, "port {port}");
            }
        }
    }

    #[test]
    fn plugs_and_unplugs_are_listed_once_each() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.port_changes(), vec![]);
        hal.fake().plug(2, FakeUsbDevice::k120());
        assert_eq!(xhci.port_changes(), vec![change(2, true, true)]);
        assert_eq!(xhci.port_changes(), vec![]);
        hal.fake().unplug(2);
        assert_eq!(xhci.port_changes(), vec![change(2, false, true)]);
        // Out and in again between two looks: the old device is gone.
        hal.fake().plug(2, FakeUsbDevice::k120());
        xhci.port_changes();
        hal.fake().unplug(2);
        hal.fake().plug(2, FakeUsbDevice::k120());
        assert_eq!(xhci.port_changes(), vec![change(2, true, true)]);
        assert_eq!(hal.fake().portsc(2) & CHANGE_BITS, 0);
    }

    #[test]
    fn a_reset_on_its_own_is_not_a_connection_change() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.fake().plug(4, FakeUsbDevice::k120());
        xhci.port_changes();
        xhci.reset_port(4).unwrap();
        assert_eq!(xhci.port_changes(), vec![]);
    }

    #[test]
    fn events_for_ports_that_do_not_exist_are_ignored() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        xhci.port_changes();
        hal.act(|x, dma| {
            x.post([0, 0, 1 << 24, 34 << 10], dma);
            x.post([200 << 24, 0, 1 << 24, 34 << 10], dma);
        });
        assert_eq!(xhci.port_changes(), vec![]);
    }
}
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `PortChange` in this scope ``; `` cannot find struct, variant or union type `PortChange` in this scope ``.

- [ ] **Step 9: Change `crates/usb/src/xhci/command.rs`**

In `crates/usb/src/xhci/command.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use super::trb::{
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT, SUCCESS, Trb,
    command_name, completion_name,
};
````

with:

````rust
use super::trb::{
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT,
    PORT_STATUS_CHANGE, SUCCESS, Trb, command_name, completion_name,
};
````

Replace:

````rust
            COMMAND_COMPLETION => self.command_completed(event),
            HOST_CONTROLLER_EVENT => xlog!(
````

with:

````rust
            COMMAND_COMPLETION => self.command_completed(event),
            PORT_STATUS_CHANGE => self.port_event(event.port_id()),
            HOST_CONTROLLER_EVENT => xlog!(
````

- [ ] **Step 10: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

/// One xHCI controller, brought up by [`Xhci::new`].
````

with:

````rust

/// A root port whose state changed since the last `port_changes()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortChange {
    pub port: u8,
    pub connected: bool,
    /// A connect or disconnect happened (CSC was set): a device that is
    /// attached on this port is gone, even if something is connected now.
    pub reconnected: bool,
}

/// One xHCI controller, brought up by [`Xhci::new`].
````

Replace:

````rust
    ring_stopped: bool,
    /// Set when the controller stopped working: nothing is sent to it any
````

with:

````rust
    ring_stopped: bool,
    /// Ports a Port Status Change Event named since `port_changes` looked.
    port_flags: Vec<bool>,
    /// `port_changes` has not run yet: every port counts.
    first_scan: bool,
    /// Set when the controller stopped working: nothing is sent to it any
````

- [ ] **Step 11: Implement `crates/usb/src/xhci/port.rs`**

Insert this at the top of `crates/usb/src/xhci/port.rs`, above `#[cfg(test)]`:

````rust
//! Root ports (xHCI 4.19): PORTSC writes that change only what they mean
//! to, port reset (USB 2 ports are reset, USB 3 ports train by
//! themselves), and the list of ports whose connection changed.

use super::caps::PortProtocol;
use super::context::speed_from_id;
use super::init::wait_for;
use super::regs::{
    ALL_ONES, CCS, CHANGE_BITS, CSC, PED, PLS_SHIFT, PR, PRC, SPEED_SHIFT, WPR, WRC, portsc_neutral,
};
use super::{PortChange, Xhci};
use crate::{Hal, Speed, UsbError};
use alloc::vec::Vec;
use core::time::Duration;

/// How long a port reset or USB 3 link training may take (spec §6.2).
pub const PORT_RESET_TIMEOUT: Duration = Duration::from_millis(500);
/// Reset recovery before the device must answer (USB 2.0 7.1.7.5).
const RESET_RECOVERY: Duration = Duration::from_millis(10);
// PORTSC.PLS: link states (xHCI table 5-27).
const U0: u32 = 0;
const SS_INACTIVE: u32 = 6;
const COMPLIANCE: u32 = 10;
const POLLING: u32 = 7;

fn link_state(portsc: u32) -> u32 {
    portsc >> PLS_SHIFT & 0xF
}

/// Enabled with the link in U0: a trained USB 3 port.
fn trained(portsc: u32) -> bool {
    portsc & PED != 0 && link_state(portsc) == U0
}

impl<H: Hal> Xhci<H> {
    fn portsc(&self, port: u8) -> u32 {
        self.regs.portsc(&self.hal, port)
    }

    /// Clears the change bits in `bits` (RW1C) and nothing else.
    fn clear_changes(&self, port: u8, portsc: u32, bits: u32) {
        if portsc & bits != 0 {
            let value = portsc_neutral(portsc) | portsc & bits;
            self.regs.set_portsc(&self.hal, port, value);
        }
    }

    /// A USB 3 port whose link is still training (Polling, CCS 0), if any.
    pub(super) fn usb3_link_training(&self) -> Option<u8> {
        (1..=self.info.ports).find(|&port| {
            self.ports[port as usize - 1] == Some(PortProtocol::Usb3)
                && link_state(self.portsc(port)) == POLLING
        })
    }

    /// A Port Status Change Event: the port is looked at by the next
    /// `port_changes`, which also clears its change bits.
    pub(super) fn port_event(&mut self, port: u8) {
        if let Some(flag) = port
            .checked_sub(1)
            .and_then(|i| self.port_flags.get_mut(i as usize))
        {
            *flag = true;
        }
    }

    /// Ports whose status changed since the last call, in port order, with
    /// their change bits cleared. Right after `new` every port that is
    /// connected counts as changed, so the caller attaches boot-time devices
    /// and later hot-plugged ones the same way. Calls `poll` first.
    pub fn port_changes(&mut self) -> Vec<PortChange> {
        self.poll();
        let mut changes = Vec::new();
        if self.dead {
            return changes;
        }
        let first = core::mem::replace(&mut self.first_scan, false);
        for port in 1..=self.info.ports {
            let i = port as usize - 1;
            if !(first || self.port_flags[i]) {
                continue;
            }
            self.port_flags[i] = false;
            let sc = self.portsc(port);
            if sc == ALL_ONES {
                continue;
            }
            self.clear_changes(port, sc, CHANGE_BITS);
            let connected = sc & CCS != 0;
            let reconnected = sc & CSC != 0;
            if self.ports[i].is_none() || !(reconnected || first && connected) {
                continue;
            }
            let what = if connected {
                "connected"
            } else {
                "disconnected"
            };
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: {what}, PORTSC {sc:#010x}"
            );
            changes.push(PortChange {
                port,
                connected,
                reconnected,
            });
        }
        changes
    }

    /// Resets `port` as its protocol needs and returns the speed the device
    /// connected at.
    pub(super) fn reset_port(&mut self, port: u8) -> Result<Speed, UsbError> {
        match self.ports.get(port as usize - 1).copied().flatten() {
            Some(PortProtocol::Usb2) => self.reset_usb2(port)?,
            Some(PortProtocol::Usb3) => self.reset_usb3(port)?,
            None => return Err(UsbError::Unsupported("port without a supported protocol")),
        }
        let sc = self.portsc(port);
        let Some(speed) = speed_from_id((sc >> SPEED_SHIFT & 0xF) as u8) else {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: unknown speed, PORTSC {sc:#010x}"
            );
            return Err(UsbError::Unsupported("unknown port speed"));
        };
        xlog!(&self.hal, &self.name, "port {port}: reset done, {speed}");
        Ok(speed)
    }

    /// Waits up to 500 ms for `done` or a disconnect; the last PORTSC read.
    fn wait_port(&self, port: u8, done: impl Fn(u32) -> bool) -> (bool, u32) {
        let ok = wait_for(&self.hal, PORT_RESET_TIMEOUT, || {
            let sc = self.portsc(port);
            done(sc) || sc & CCS == 0
        });
        let sc = self.portsc(port);
        (ok.is_some() && sc & CCS != 0, sc)
    }

    /// The outcome when a wait on `port` did not end well.
    fn port_failed(&self, port: u8, sc: u32, what: &str) -> UsbError {
        if sc & CCS == 0 {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: disconnected during {what}"
            );
            UsbError::Disconnected
        } else {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: {what} failed, PORTSC {sc:#010x}"
            );
            UsbError::Timeout
        }
    }

    /// USB 2 (xHCI 4.3.1): PR, up to 500 ms for PRC, clear it, PED must be
    /// set, then 10 ms of reset recovery.
    fn reset_usb2(&mut self, port: u8) -> Result<(), UsbError> {
        let sc = self.portsc(port);
        self.regs
            .set_portsc(&self.hal, port, portsc_neutral(sc) | PR);
        let (ok, sc) = self.wait_port(port, |sc| sc & PRC != 0);
        self.clear_changes(port, sc, PRC);
        if !ok || sc & PED == 0 {
            return Err(self.port_failed(port, sc, "reset"));
        }
        self.hal.sleep(RESET_RECOVERY);
        Ok(())
    }

    /// USB 3 (xHCI 4.3.1): the link trains by itself. Once it is enabled in
    /// U0, a hot reset (PR) resets the device, as Linux does, so one that
    /// kept its address across HCRST (the stick the machine booted from)
    /// starts afresh. A link not in U0 within 500 ms, or in SS.Inactive or
    /// Compliance, gets one warm reset instead.
    fn reset_usb3(&mut self, port: u8) -> Result<(), UsbError> {
        let (ok, sc) = self.wait_port(port, |sc| {
            trained(sc) || matches!(link_state(sc), SS_INACTIVE | COMPLIANCE)
        });
        if ok && trained(sc) {
            self.regs
                .set_portsc(&self.hal, port, portsc_neutral(sc) | PR);
            let (ok, sc) = self.wait_port(port, |sc| sc & PRC != 0);
            self.clear_changes(port, sc, PRC);
            if !ok || !trained(sc) {
                return Err(self.port_failed(port, sc, "hot reset"));
            }
            return Ok(());
        }
        if sc & CCS == 0 {
            return Err(self.port_failed(port, sc, "link training"));
        }
        xlog!(
            &self.hal,
            &self.name,
            "port {port}: link not trained (PLS {}), warm reset",
            link_state(sc)
        );
        self.regs
            .set_portsc(&self.hal, port, portsc_neutral(sc) | WPR);
        let (ok, sc) = self.wait_port(port, |sc| sc & (PRC | WRC) != 0);
        self.clear_changes(port, sc, PRC | WRC);
        if !ok || !trained(sc) {
            return Err(self.port_failed(port, sc, "warm reset"));
        }
        Ok(())
    }
}

````

- [ ] **Step 12: Change `crates/usb/src/xhci/start.rs`**

In `crates/usb/src/xhci/start.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
const ATTACH_TIME: Duration = Duration::from_millis(100);
/// The only page size this driver sets up scratchpad buffers for.
````

with:

````rust
const ATTACH_TIME: Duration = Duration::from_millis(100);
/// How long `Xhci::new` waits at most, in all, for USB 3 links present at
/// boot to finish training.
const SETTLE_LIMIT: Duration = Duration::from_secs(1);
/// The only page size this driver sets up scratchpad buffers for.
````

Replace:

````rust
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
        let Some(base) = hal.map_mmio(mmio_phys, mmio_len) else {
````

with:

````rust
    pub fn new(hal: H, mmio_phys: u64, mmio_len: usize, name: &str) -> Result<Xhci<H>, UsbError> {
        let started = hal.now();
        let Some(base) = hal.map_mmio(mmio_phys, mmio_len) else {
````

Replace:

````rust
            dead: false,
        };
        xhci.settle_ports(settle_from);
        Ok(xhci)
    }

    /// Waits until 100 ms after the ports were last powered or reset, so
    /// that devices present at boot have signalled their attach.
    fn settle_ports(&mut self, since: Duration) {
        let start = self.hal.now();
        self.hal.sleep((since + ATTACH_TIME).saturating_sub(start));
        let waited = (self.hal.now() - start).as_millis();
        xlog!(&self.hal, &self.name, "ports settled after {waited} ms");
    }
````

with:

````rust
            dead: false,
            port_flags: alloc::vec![false; params.ports as usize],
            first_scan: true,
        };
        xhci.settle_ports(started, settle_from);
        Ok(xhci)
    }

    /// Waits until 100 ms after the ports were last powered or reset, so
    /// that devices present at boot have signalled their attach, then,
    /// polling, up to 1 s after `started` in all while a USB 3 link still
    /// trains (in Polling it shows no connection yet).
    fn settle_ports(&mut self, started: Duration, since: Duration) {
        let start = self.hal.now();
        self.hal.sleep((since + ATTACH_TIME).saturating_sub(start));
        while self.usb3_link_training().is_some() && self.hal.now() - started < SETTLE_LIMIT {
            self.poll();
            self.hal.sleep(Duration::from_millis(1));
        }
        let waited = (self.hal.now() - start).as_millis();
        match self.usb3_link_training() {
            Some(port) => xlog!(
                &self.hal,
                &self.name,
                "port {port}: USB 3 link still training after {waited} ms"
            ),
            None => xlog!(&self.hal, &self.name, "ports settled after {waited} ms"),
        }
    }
````

- [ ] **Step 13: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 138 tests.

- [ ] **Step 14: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 15: Commit**

````bash
git add crates
git commit -m "usb: xHCI root ports: reset for USB 2 and USB 3, port changes"
````


### Task 11: Enumeration and control transfers

Spec §6.2's device setup, steps 1–5. `attach` debounces the port (100 ms), resets it, runs Enable Slot, gives the slot its output context (in the DCBAA), an input context and a ring for EP0, and runs Address Device with EP0's packet size for the speed (8, 64 or 512), then gives the device 10 ms to settle after `SET_ADDRESS` (USB 2.0 allows it 2 ms; Linux waits 10). It then reads the first 8 bytes of the device descriptor and, if EP0's real packet size differs (the Unifying receiver's is 64), fixes it with Evaluate Context before reading the rest; then the configuration descriptor, header first and then all of it (4 KiB at most). A control transfer is a Setup Stage with the packet inline, a Data Stage into the slot's DMA buffer and a Status Stage, each its own TD: a short IN data stage reports its residue in a Short Packet event (some controllers say Success with a residue; that counts the same) and the status stage still completes. A STALL resets EP0 (Reset Endpoint, Set TR Dequeue Pointer) and a timeout (1 s) stops it, so the next request works either way. Every failure disables the slot and frees everything it allocated, with a log line naming the step; if Disable Slot itself fails, the memory is kept rather than freed while the controller may still use it (decision 6).

**Files:**
- Modify: `crates/usb/src/testing/device.rs`
- Modify: `crates/usb/src/testing/xhci/commands.rs`
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/ports.rs`
- Modify: `crates/usb/src/testing/xhci/regs.rs`
- Create: `crates/usb/src/testing/xhci/slots.rs`
- Create: `crates/usb/src/testing/xhci/transfers.rs`
- Modify: `crates/usb/src/xhci/command.rs`
- Create: `crates/usb/src/xhci/device.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/start.rs`
- Create: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: Tasks 3–10.
- Produces: `usb::xhci::{Device { slot, port, speed, descriptor, configuration }, Xhci::{attach(&mut self, port: u8) -> Result<Device, UsbError>, slot_of_port(&self, port: u8) -> Option<u8>}}`, `xhci::device::DEBOUNCE = 100 ms`, `xhci::transfer::{CONTROL_TIMEOUT = 1 s, DATA_BUFFER_SIZE = 4096}`, crate-internal `control_transfer`, `transfer_event`, `reposition`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/device.rs`**

In `crates/usb/src/testing/device.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

use crate::Speed;
use std::cell::RefCell;
use std::rc::Rc;

/// A device as the fake controller sees it on a port.
pub trait FakeDevice {
    fn speed(&self) -> Speed;
}

/// A configurable device with its descriptors, modelled on real ones.
pub struct FakeUsbDevice {
````

with:

````rust

use crate::bus::{GET_DESCRIPTOR, SET_CONFIGURATION};
use crate::descriptor::{CONFIGURATION, DEVICE};
use crate::{Setup, Speed};
use std::cell::RefCell;
use std::rc::Rc;

const SET_ADDRESS: u8 = 5;

/// A STALL handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stall;

/// A device as the fake controller sees it on a port.
pub trait FakeDevice {
    fn speed(&self) -> Speed;
    /// Endpoint 0's packet size: how the device splits a data stage.
    fn max_packet0(&self) -> u16;
    /// A control request with its OUT data; the IN data, or `None` if the
    /// device never answers it (NAKs until the host gives up).
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>>;
}

/// A control request as the device got it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub setup: Setup,
    pub data: Vec<u8>,
}

/// A configurable device with its descriptors, modelled on real ones. It
/// answers the standard requests itself and records every request.
pub struct FakeUsbDevice {
````

Replace:

````rust
    configuration: Vec<u8>,
}
````

with:

````rust
    configuration: Vec<u8>,
    requests: Vec<Request>,
    /// Knob: requests (bRequest, wValue) that are stalled.
    stalls: Vec<(u8, u16)>,
    /// Knob: this many requests from now on are never answered.
    ignore: usize,
    /// Knob: at most this many bytes of the configuration are sent.
    configuration_limit: Option<usize>,
    address: u8,
    configuration_value: u8,
}
````

Replace:

````rust
            configuration,
        }))
````

with:

````rust
            configuration,
            requests: Vec::new(),
            stalls: Vec::new(),
            ignore: 0,
            configuration_limit: None,
            address: 0,
            configuration_value: 0,
        }))
````

Replace:

````rust
        &self.configuration
    }
}

````

with:

````rust
        &self.configuration
    }

    /// Replaces the descriptors (a device that sends a wrong one).
    pub fn set_descriptors(&mut self, device: Vec<u8>, configuration: Vec<u8>) {
        self.device = device;
        self.configuration = configuration;
    }

    /// Every request so far, SET_ADDRESS from the controller included.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.clone()
    }

    /// Stalls every `request` with `value` from now on.
    pub fn stall_request(&mut self, request: u8, value: u16) {
        self.stalls.push((request, value));
    }

    /// Never answers the next `n` requests of the driver (SET_ADDRESS,
    /// which the controller sends, is always answered).
    pub fn ignore_requests(&mut self, n: usize) {
        self.ignore = n;
    }

    /// Sends at most `len` bytes of the configuration descriptor.
    pub fn truncate_configuration(&mut self, len: usize) {
        self.configuration_limit = Some(len);
    }

    /// The address SET_ADDRESS gave it.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The value SET_CONFIGURATION gave it.
    pub fn configuration_value(&self) -> u8 {
        self.configuration_value
    }

    fn descriptor(&self, setup: &Setup) -> Result<Vec<u8>, Stall> {
        let mut d = match ((setup.value >> 8) as u8, setup.value as u8) {
            (DEVICE, 0) => self.device.clone(),
            (CONFIGURATION, 0) => {
                let limit = self.configuration_limit.unwrap_or(usize::MAX);
                self.configuration[..self.configuration.len().min(limit)].to_vec()
            }
            _ => return Err(Stall),
        };
        d.truncate(setup.length as usize);
        Ok(d)
    }
}

````

Replace:

````rust
        self.speed
    }
````

with:

````rust
        self.speed
    }

    fn max_packet0(&self) -> u16 {
        let raw = self.device[7];
        if self.speed.is_superspeed() {
            1 << raw
        } else {
            raw as u16
        }
    }

    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>> {
        self.requests.push(Request {
            setup,
            data: data_out.to_vec(),
        });
        if self.ignore > 0 && setup.request != SET_ADDRESS {
            self.ignore -= 1;
            return None;
        }
        if self.stalls.contains(&(setup.request, setup.value)) {
            return Some(Err(Stall));
        }
        Some(match (setup.request_type, setup.request) {
            (0x80, GET_DESCRIPTOR) => self.descriptor(&setup),
            (0x00, SET_ADDRESS) => {
                self.address = setup.value as u8;
                Ok(Vec::new())
            }
            (0x00, SET_CONFIGURATION) => {
                self.configuration_value = setup.value as u8;
                Ok(Vec::new())
            }
            _ if setup.is_in() => Ok(vec![0; setup.length as usize]),
            _ => Ok(Vec::new()),
        })
    }
````

Replace:

````rust
        assert_eq!(c.interfaces[0].endpoints[0].max_burst, 3);
    }
````

with:

````rust
        assert_eq!(c.interfaces[0].endpoints[0].max_burst, 3);
        assert_eq!(stick.borrow().max_packet0(), 512);
        assert_eq!(
            FakeUsbDevice::unifying_receiver().borrow().max_packet0(),
            64
        );
    }

    #[test]
    fn standard_requests_are_answered_and_recorded() {
        let dev = FakeUsbDevice::k120();
        let mut d = dev.borrow_mut();
        let got = d.control(Setup::get_descriptor(DEVICE, 0, 8), &[]);
        assert_eq!(got.map(|r| r.map(|v| v.len())), Some(Ok(8)));
        d.truncate_configuration(20);
        let got = d.control(Setup::get_descriptor(CONFIGURATION, 0, 255), &[]);
        assert_eq!(got.map(|r| r.map(|v| v.len())), Some(Ok(20)));
        d.stall_request(GET_DESCRIPTOR, 0x0300);
        assert_eq!(
            d.control(Setup::get_descriptor(3, 0, 4), &[]),
            Some(Err(Stall))
        );
        d.ignore_requests(1);
        let set_address = Setup {
            request: SET_ADDRESS,
            ..Setup::set_configuration(3)
        };
        assert_eq!(d.control(set_address, &[]), Some(Ok(vec![])));
        assert_eq!(d.control(Setup::set_configuration(1), &[]), None);
        assert_eq!(
            d.control(Setup::set_configuration(1), &[]),
            Some(Ok(vec![]))
        );
        assert_eq!(d.configuration_value(), 1);
        assert_eq!(d.requests().len(), 6);
        assert_eq!(d.address(), 3);
    }
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/commands.rs`**

In `crates/usb/src/testing/xhci/commands.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

use super::{Consumer, FakeXhci};
````

with:

````rust

use super::slots::FakeSlot;
use super::{Consumer, FakeXhci};
````

Replace:

````rust
pub const DISABLE_SLOT: u32 = 10;
pub const NO_OP_COMMAND: u32 = 23;
````

with:

````rust
pub const DISABLE_SLOT: u32 = 10;
pub const ADDRESS_DEVICE: u32 = 11;
pub const EVALUATE_CONTEXT: u32 = 13;
pub const RESET_ENDPOINT: u32 = 14;
pub const STOP_ENDPOINT: u32 = 15;
pub const SET_TR_DEQUEUE: u32 = 16;
pub const NO_OP_COMMAND: u32 = 23;
````

Replace:

````rust
}

/// A device slot as the controller sees it.
#[derive(Debug, Default)]
pub struct FakeSlot {}

````

with:

````rust
}

````

Replace:

````rust
            ENABLE_SLOT => self.enable_slot(),
            DISABLE_SLOT => (self.disable_slot(slot_of(&trb)), slot_of(&trb)),
            _ => panic!("fake xhci: TRB type {kind} on the command ring"),
````

with:

````rust
            ENABLE_SLOT => self.enable_slot(),
            DISABLE_SLOT => (self.disable_slot(slot_of(&trb), dma), slot_of(&trb)),
            ADDRESS_DEVICE => (self.address_device(&trb, dma), slot_of(&trb)),
            EVALUATE_CONTEXT => (self.evaluate_context(&trb, dma), slot_of(&trb)),
            RESET_ENDPOINT => (self.reset_endpoint(&trb, dma), slot_of(&trb)),
            STOP_ENDPOINT => (self.stop_endpoint(&trb, dma), slot_of(&trb)),
            SET_TR_DEQUEUE => (self.set_tr_dequeue(&trb, dma), slot_of(&trb)),
            _ => panic!("fake xhci: TRB type {kind} on the command ring"),
````

Replace:

````rust
            None => (NO_SLOTS, 0),
        }
    }

    fn disable_slot(&mut self, slot: usize) -> u32 {
        match self.slots.get_mut(slot).and_then(Option::take) {
            Some(_) => SUCCESS,
            None => SLOT_NOT_ENABLED,
        }
````

with:

````rust
            None => (NO_SLOTS, 0),
        }
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
mod rings;

pub use commands::Executed;
use commands::FakeSlot;

````

with:

````rust
mod rings;
mod slots;
mod transfers;

pub use commands::Executed;
use slots::FakeSlot;

````

Replace:

````rust
    pub usb3_link_fails: bool,
    /// How long a USB 2 device present when its port is reset or powered
````

with:

````rust
    pub usb3_link_fails: bool,
    /// Knob: a short control data stage is reported as Success with the
    /// residual, not as Short Packet (some controllers do).
    pub short_as_success: bool,
    /// Knob: an unplug fails the device's transfers in progress with a USB
    /// Transaction Error, as Intel controllers do.
    pub fail_transfers_on_unplug: bool,
    /// How long a USB 2 device present when its port is reset or powered
````

Replace:

````rust
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
        }
    }

````

with:

````rust
            usb3_training: Duration::from_millis(50),
            usb3_link_fails: false,
            usb2_attach_delay: Duration::from_millis(30),
            short_as_success: false,
            fail_transfers_on_unplug: true,
        }
    }

````

Replace:

````rust
            usb2_attach_delay: Duration::from_millis(30),
        }
````

with:

````rust
            usb2_attach_delay: Duration::from_millis(30),
            short_as_success: false,
            fail_transfers_on_unplug: true,
        }
````

Replace:

````rust
    abort_requested: bool,
}
````

with:

````rust
    abort_requested: bool,
    /// Endpoints (slot, DCI) with work: rung, or waiting for the device.
    active: std::collections::BTreeSet<(usize, usize)>,
}
````

Replace:

````rust
            abort_requested: false,
        };
````

with:

````rust
            abort_requested: false,
            active: Default::default(),
        };
````

Replace:

````rust
        self.process_commands(dma);
        self.flush_events(dma);
````

with:

````rust
        self.process_commands(dma);
        self.process_transfers(dma);
        self.flush_events(dma);
````

- [ ] **Step 4: Extend the test support in `crates/usb/src/testing/xhci/ports.rs`**

In `crates/usb/src/testing/xhci/ports.rs`, replace:

````rust
            self.set_changes(i, CSC);
        }
````

with:

````rust
            self.set_changes(i, CSC);
        }
        if self.config.fail_transfers_on_unplug {
            self.after(Duration::ZERO, move |x, dma| x.device_gone(port, dma));
        }
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/xhci/regs.rs`**

In `crates/usb/src/testing/xhci/regs.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            Block::Doorbell(0) => self.command_doorbell(value),
            Block::Doorbell(_) => {}
            Block::Extended(o) => self.extended_write(o, value),
````

with:

````rust
            Block::Doorbell(0) => self.command_doorbell(value),
            Block::Doorbell(slot) => self.slot_doorbell(slot, value, dma),
            Block::Extended(o) => self.extended_write(o, value),
````

Replace:

````rust
        self.slots.iter_mut().for_each(|s| *s = None);
        self.hcrst_at = Some(self.now);
````

with:

````rust
        self.slots.iter_mut().for_each(|s| *s = None);
        self.active.clear();
        self.hcrst_at = Some(self.now);
````

- [ ] **Step 6: Create `crates/usb/src/testing/xhci/slots.rs`**

Create `crates/usb/src/testing/xhci/slots.rs`:

````rust
//! Device slots and endpoints as the controller keeps them: the commands
//! that change them (xHCI 4.6.5-4.6.10), checking input contexts like the
//! hardware does, and the output (device) contexts the driver reads.

use super::commands::{SLOT_NOT_ENABLED, SUCCESS};
use super::{Consumer, FakeXhci};
use crate::Setup;
use crate::Speed;
use crate::testing::device::Stall;
use crate::testing::hal::Dma;
use core::time::Duration;
use std::collections::BTreeMap;

pub const USB_TRANSACTION_ERROR: u32 = 4;
pub const ENDPOINT_NOT_ENABLED: u32 = 12;
pub const CONTEXT_STATE_ERROR: u32 = 19;
pub const STOPPED: u32 = 26;

// Slot states.
const ENABLED: u32 = 0;
const DEFAULT: u32 = 1;
const ADDRESSED: u32 = 2;
pub const CONFIGURED: u32 = 3;

// Endpoint states.
pub const RUNNING: u32 = 1;
pub const HALTED: u32 = 2;
pub const STOPPED_STATE: u32 = 3;
const EP_ERROR: u32 = 4;

/// Endpoint type 4: control.
const CONTROL: u32 = 4;

/// A slot: its state, its port, where its device context is.
#[derive(Clone, Debug, Default)]
pub struct FakeSlot {
    pub state: u32,
    pub port: u8,
    pub output: u64,
    pub context_entries: u32,
    pub endpoints: BTreeMap<usize, FakeEndpoint>,
    /// When the device got its SET_ADDRESS.
    pub addressed_at: Duration,
}

/// An endpoint as its context describes it, and its transfer ring.
#[derive(Clone, Debug)]
pub struct FakeEndpoint {
    pub state: u32,
    pub ring: Consumer,
    pub ep_type: u32,
    pub max_packet: u32,
    pub cerr: u32,
    pub interval: u32,
    pub mult: u32,
    pub max_burst: u32,
    pub average_trb_length: u32,
    pub max_esit_payload: u32,
    /// A TD is in progress, waiting for the device.
    pub busy: bool,
}

/// The default EP0 packet size of a speed (USB 2.0 5.5.3, USB 3.2 9.6.1).
fn default_max_packet0(speed: Speed) -> u32 {
    speed.default_max_packet0() as u32
}

fn speed_id(speed: Speed) -> u32 {
    match speed {
        Speed::Full => 1,
        Speed::Low => 2,
        Speed::High => 3,
        Speed::Super => 4,
        Speed::SuperPlus => 5,
    }
}

impl FakeXhci {
    pub(super) fn stride(&self) -> u64 {
        if self.config.context_64 { 64 } else { 32 }
    }

    pub fn slot(&self, slot: usize) -> Option<FakeSlot> {
        self.slots.get(slot).cloned().flatten()
    }

    pub fn endpoint(&self, slot: usize, dci: usize) -> Option<FakeEndpoint> {
        self.slot(slot)?.endpoints.get(&dci).cloned()
    }

    /// The input context at `trb`'s pointer, checked like the controller
    /// would before it reads it.
    fn input_context(&self, trb: &[u32; 4], dma: &Dma) -> u64 {
        let input = trb[0] as u64 | (trb[1] as u64) << 32;
        if input & 0xF != 0 || !dma.contains(input, 33 * self.stride() as usize) {
            panic!("fake xhci: input context at {input:#x} is not 33 contexts of allocated memory");
        }
        input
    }

    /// Parses the endpoint context at `at` and checks its ring.
    fn read_endpoint(&self, at: u64, dma: &Dma, what: &str) -> FakeEndpoint {
        let d = [dma.read32(at), dma.read32(at + 4), dma.read32(at + 16)];
        let dequeue = dma.read64(at + 8);
        let ring = Consumer {
            dequeue: dequeue & !0xF,
            cycle: dequeue & 1 != 0,
        };
        self.check_dequeue(dequeue, dma, what);
        FakeEndpoint {
            state: RUNNING,
            ring,
            ep_type: d[1] >> 3 & 7,
            max_packet: d[1] >> 16,
            cerr: d[1] >> 1 & 3,
            interval: d[0] >> 16 & 0xFF,
            mult: d[0] >> 8 & 3,
            max_burst: d[1] >> 8 & 0xFF,
            average_trb_length: d[2] & 0xFFFF,
            max_esit_payload: (d[0] >> 24) << 16 | d[2] >> 16,
            busy: false,
        }
    }

    /// A TR Dequeue Pointer with its DCS: 16-byte aligned, in allocated
    /// memory, and not pointing at a TRB its DCS already says is queued
    /// (this driver always hands over an empty ring position).
    pub(super) fn check_dequeue(&self, dequeue: u64, dma: &Dma, what: &str) {
        let ptr = dequeue & !0xF;
        if dequeue & 0xE != 0 || !dma.contains(ptr, 16) {
            panic!("fake xhci: {what}: bad TR dequeue pointer {dequeue:#x}");
        }
        let dcs = dequeue & 1 != 0;
        if (dma.read32(ptr + 12) & 1 != 0) == dcs {
            panic!(
                "fake xhci: {what}: DCS {} at {ptr:#x}, whose TRB it makes look queued",
                dcs as u8
            );
        }
    }

    /// Writes an endpoint's state into the fake and the device context.
    pub(super) fn set_ep_state(&mut self, slot: usize, dci: usize, state: u32, dma: &Dma) {
        let stride = self.stride();
        let Some(s) = self.slots[slot].as_mut() else {
            return;
        };
        if let Some(ep) = s.endpoints.get_mut(&dci) {
            ep.state = state;
            let at = s.output + dci as u64 * stride;
            dma.write32(at, dma.read32(at) & !7 | state);
        }
    }

    fn slot_state(&self, slot: usize) -> Option<u32> {
        self.slots
            .get(slot)
            .and_then(|s| s.as_ref())
            .map(|s| s.state)
    }

    /// Address Device (xHCI 4.6.5): the slot and EP0 contexts are checked
    /// and copied to the device context, and SET_ADDRESS goes to the
    /// device on the slot context's root port.
    pub(super) fn address_device(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        let Some(state) = self.slot_state(slot) else {
            return SLOT_NOT_ENABLED;
        };
        if !matches!(state, ENABLED | DEFAULT) {
            return CONTEXT_STATE_ERROR;
        }
        let stride = self.stride();
        let output = dma.read64(self.dcbaap + 8 * slot as u64);
        if output & 0x3F != 0 || !dma.contains(output, 32 * stride as usize) {
            panic!("fake xhci: DCBAA[{slot}] = {output:#x} is not a device context");
        }
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop != 0 || add != 0b11 {
            panic!("fake xhci: Address Device with drop {drop:#x} add {add:#x}, not A0 and A1");
        }
        let slot_ctx = input + stride;
        let d0 = dma.read32(slot_ctx);
        let port = (dma.read32(slot_ctx + 4) >> 16 & 0xFF) as u8;
        if d0 >> 27 == 0 {
            panic!("fake xhci: Address Device with context entries 0");
        }
        if port == 0 || port > self.config.ports {
            panic!("fake xhci: Address Device for root port {port}");
        }
        let Some(dev) = self.devices[port as usize - 1].clone() else {
            return USB_TRANSACTION_ERROR;
        };
        let speed = dev.borrow().speed();
        if self.portsc(port) & 2 == 0 {
            return USB_TRANSACTION_ERROR;
        }
        if d0 >> 20 & 0xF != speed_id(speed) {
            panic!(
                "fake xhci: slot context speed {} for a {speed} device",
                d0 >> 20 & 0xF
            );
        }
        let mut ep0 = self.read_endpoint(input + 2 * stride, dma, "Address Device EP0");
        if ep0.ep_type != CONTROL || ep0.max_packet != default_max_packet0(speed) {
            panic!(
                "fake xhci: EP0 context type {} max packet {} for a {speed} device",
                ep0.ep_type, ep0.max_packet
            );
        }
        if trb[3] & 1 << 9 == 0 {
            let set_address = Setup {
                request_type: 0,
                request: 5,
                value: slot as u16,
                index: 0,
                length: 0,
            };
            if dev.borrow_mut().control(set_address, &[]) != Some(Ok::<_, Stall>(Vec::new())) {
                return USB_TRANSACTION_ERROR;
            }
        }
        let new_state = if trb[3] & 1 << 9 != 0 {
            DEFAULT
        } else {
            ADDRESSED
        };
        for i in 0..4 {
            dma.write32(output + 4 * i, dma.read32(slot_ctx + 4 * i));
        }
        dma.write32(output + 12, slot as u32 | new_state << 27);
        for i in 0..5 {
            dma.write32(
                output + stride + 4 * i,
                dma.read32(input + 2 * stride + 4 * i),
            );
        }
        ep0.state = RUNNING;
        let s = self.slots[slot].as_mut().expect("slot checked above");
        (s.state, s.port, s.output, s.context_entries) = (new_state, port, output, d0 >> 27);
        s.addressed_at = self.now;
        s.endpoints.insert(1, ep0);
        self.set_ep_state(slot, 1, RUNNING, dma);
        SUCCESS
    }

    /// Evaluate Context (xHCI 4.6.7): for EP0 only its max packet size.
    pub(super) fn evaluate_context(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        match self.slot_state(slot) {
            None => return SLOT_NOT_ENABLED,
            Some(DEFAULT | ADDRESSED | CONFIGURED) => {}
            Some(_) => return CONTEXT_STATE_ERROR,
        }
        let stride = self.stride();
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop != 0 || add & !0b11 != 0 {
            panic!("fake xhci: Evaluate Context with drop {drop:#x} add {add:#x}");
        }
        if add & 0b10 != 0 {
            let mps = dma.read32(input + 2 * stride + 4) >> 16;
            if mps == 0 {
                panic!("fake xhci: Evaluate Context with EP0 max packet size 0");
            }
            let s = self.slots[slot].as_mut().expect("slot checked above");
            if let Some(ep0) = s.endpoints.get_mut(&1) {
                ep0.max_packet = mps;
            }
            let at = s.output + stride + 4;
            dma.write32(at, dma.read32(at) & 0xFFFF | mps << 16);
        }
        SUCCESS
    }

    /// The endpoint a Reset/Stop/Set TR Dequeue command names, or the
    /// completion code for a slot or endpoint that is not there.
    fn command_endpoint(&self, trb: &[u32; 4]) -> Result<(usize, usize, u32), u32> {
        let slot = (trb[3] >> 24) as usize;
        let dci = (trb[3] >> 16 & 0x1F) as usize;
        let s = self
            .slots
            .get(slot)
            .and_then(|s| s.as_ref())
            .ok_or(SLOT_NOT_ENABLED)?;
        let ep = s.endpoints.get(&dci).ok_or(ENDPOINT_NOT_ENABLED)?;
        Ok((slot, dci, ep.state))
    }

    /// Reset Endpoint (xHCI 4.6.8): Halted to Stopped.
    pub(super) fn reset_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        match self.command_endpoint(trb) {
            Ok((slot, dci, HALTED)) => {
                self.set_ep_state(slot, dci, STOPPED_STATE, dma);
                SUCCESS
            }
            Ok(_) => CONTEXT_STATE_ERROR,
            Err(code) => code,
        }
    }

    /// Stop Endpoint (xHCI 4.6.9): a TD in progress ends with a Stopped
    /// event, then the endpoint is Stopped.
    pub(super) fn stop_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let (slot, dci) = match self.command_endpoint(trb) {
            Ok((slot, dci, RUNNING)) => (slot, dci),
            Ok(_) => return CONTEXT_STATE_ERROR,
            Err(code) => return code,
        };
        let ep = &self.slots[slot]
            .as_ref()
            .expect("slot checked above")
            .endpoints[&dci];
        let (busy, dequeue) = (ep.busy, ep.ring.dequeue);
        if busy {
            self.post_transfer(slot, dci, dequeue, STOPPED, 0, dma);
        }
        self.set_ep_state(slot, dci, STOPPED_STATE, dma);
        if let Some(ep) = self.slots[slot]
            .as_mut()
            .and_then(|s| s.endpoints.get_mut(&dci))
        {
            ep.busy = false;
        }
        SUCCESS
    }

    /// Set TR Dequeue Pointer (xHCI 4.6.10): only on a Stopped endpoint.
    pub(super) fn set_tr_dequeue(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let (slot, dci) = match self.command_endpoint(trb) {
            Ok((slot, dci, STOPPED_STATE | EP_ERROR)) => (slot, dci),
            Ok(_) => return CONTEXT_STATE_ERROR,
            Err(code) => return code,
        };
        let dequeue = trb[0] as u64 | (trb[1] as u64) << 32;
        self.check_dequeue(dequeue, dma, "Set TR Dequeue Pointer");
        let stride = self.stride();
        let s = self.slots[slot].as_mut().expect("slot checked above");
        let ep = s.endpoints.get_mut(&dci).expect("endpoint checked above");
        ep.ring = Consumer {
            dequeue: dequeue & !0xF,
            cycle: dequeue & 1 != 0,
        };
        dma.write64(s.output + dci as u64 * stride + 8, dequeue);
        SUCCESS
    }

    /// Disable Slot (xHCI 4.6.4): the slot and its endpoints are gone.
    pub(super) fn disable_slot(&mut self, slot: usize, dma: &Dma) -> u32 {
        match self.slots.get_mut(slot).and_then(Option::take) {
            Some(s) => {
                if s.output != 0 {
                    dma.write32(s.output + 12, 0);
                }
                SUCCESS
            }
            None => SLOT_NOT_ENABLED,
        }
    }
}
````

- [ ] **Step 7: Create `crates/usb/src/testing/xhci/transfers.rs`**

Create `crates/usb/src/testing/xhci/transfers.rs`:

````rust
//! Transfer rings (xHCI 4.10, 4.11): slot doorbells, control TDs played
//! against the device on the slot's port, NAKs, STALLs, short packets,
//! babble, and what a disconnect does to transfers in progress.

use super::commands::{SUCCESS, TRANSFER_EVENT, trb_type};
use super::slots::{HALTED, RUNNING, STOPPED_STATE, USB_TRANSACTION_ERROR};
use super::{Consumer, FakeXhci};
use crate::Setup;
use crate::testing::device::Stall;
use crate::testing::hal::Dma;
use core::time::Duration;

const SETUP_STAGE: u32 = 2;
const DATA_STAGE: u32 = 3;
const STATUS_STAGE: u32 = 4;
const BABBLE: u32 = 3;
const STALL: u32 = 6;
const SHORT_PACKET: u32 = 13;
const ISP: u32 = 1 << 2;
const IOC: u32 = 1 << 5;
const IDT: u32 = 1 << 6;
const DIR_IN: u32 = 1 << 16;

/// How far a TD got.
enum Td {
    /// Finished (well or not); the next TD may follow.
    Done,
    /// Waiting for the device.
    Waiting,
    /// Software has not handed over all of its TRBs yet.
    Incomplete,
}

fn pointer(trb: &[u32; 4]) -> u64 {
    trb[0] as u64 | (trb[1] as u64) << 32
}

impl FakeXhci {
    /// Doorbell `slot` (xHCI 5.6): its target is the DCI to run.
    pub(super) fn slot_doorbell(&mut self, slot: usize, value: u32, dma: &Dma) {
        if !self.running() {
            panic!("fake xhci: doorbell rung while halted");
        }
        let dci = (value & 0xFF) as usize;
        if value >> 16 != 0 {
            panic!("fake xhci: doorbell with a stream ID");
        }
        let Some(s) = self.slots.get(slot).and_then(|s| s.as_ref()) else {
            panic!("fake xhci: doorbell for slot {slot}, which is not enabled");
        };
        let Some(ep) = s.endpoints.get(&dci) else {
            panic!("fake xhci: doorbell for DCI {dci} of slot {slot}, which is not configured");
        };
        match ep.state {
            // xHCI 4.8.3: a halted endpoint ignores its doorbell.
            HALTED => return,
            STOPPED_STATE => self.set_ep_state(slot, dci, RUNNING, dma),
            _ => {}
        }
        self.active.insert((slot, dci));
    }

    /// Runs every endpoint with work.
    pub(super) fn process_transfers(&mut self, dma: &Dma) {
        if !self.running() {
            return;
        }
        for (slot, dci) in self.active.clone() {
            if !self.process_endpoint(slot, dci, dma) {
                self.active.remove(&(slot, dci));
            }
        }
    }

    fn ring(&mut self, slot: usize, dci: usize) -> Option<&mut super::slots::FakeEndpoint> {
        self.slots.get_mut(slot)?.as_mut()?.endpoints.get_mut(&dci)
    }

    /// Runs TDs until the ring is empty or one waits; whether it waits.
    fn process_endpoint(&mut self, slot: usize, dci: usize, dma: &Dma) -> bool {
        loop {
            let Some(ep) = self.ring(slot, dci) else {
                return false;
            };
            if ep.state != RUNNING {
                return false;
            }
            if ep.busy && dci == 1 {
                // A control request the device never answers: it waits
                // for Stop Endpoint.
                return true;
            }
            let Some((_, trb)) = ep.ring.peek(dma) else {
                return false;
            };
            let td = match trb_type(&trb) {
                SETUP_STAGE if dci == 1 => self.control_td(slot, dma),
                t => panic!("fake xhci: TRB type {t} on the ring of slot {slot} DCI {dci}"),
            };
            match td {
                Td::Done => {}
                Td::Waiting => return true,
                Td::Incomplete => return false,
            }
        }
    }

    pub(super) fn post_transfer(
        &mut self,
        slot: usize,
        dci: usize,
        trb: u64,
        code: u32,
        residual: u32,
        dma: &Dma,
    ) {
        let event = [
            trb as u32,
            (trb >> 32) as u32,
            code << 24 | residual & 0xFF_FFFF,
            TRANSFER_EVENT << 10 | (dci as u32) << 16 | (slot as u32) << 24,
        ];
        self.post(event, dma);
    }

    /// An error on the TRB at `at` (xHCI 4.10.2): an event, and the
    /// endpoint halts with its dequeue pointer on that TRB.
    fn fail(&mut self, slot: usize, dci: usize, at: Consumer, code: u32, dma: &Dma) {
        self.post_transfer(slot, dci, at.dequeue, code, 0, dma);
        if let Some(ep) = self.ring(slot, dci) {
            ep.ring = at;
            ep.busy = false;
        }
        self.set_ep_state(slot, dci, HALTED, dma);
    }

    /// A control transfer (xHCI 4.11.2.2): Setup, an optional Data and a
    /// Status stage, each its own TD. The fake checks each stage's fields.
    fn control_td(&mut self, slot: usize, dma: &Dma) -> Td {
        let ep = self
            .ring(slot, 1)
            .expect("EP0 checked by the caller")
            .clone();
        let mut c = ep.ring;
        let Some((_, s)) = c.peek(dma) else {
            return Td::Incomplete;
        };
        if s[3] & IDT == 0 || s[2] & 0x1_FFFF != 8 {
            panic!("fake xhci: Setup Stage without IDT or not 8 bytes");
        }
        let setup = Setup {
            request_type: s[0] as u8,
            request: (s[0] >> 8) as u8,
            value: (s[0] >> 16) as u16,
            index: s[1] as u16,
            length: (s[1] >> 16) as u16,
        };
        let trt = s[3] >> 16 & 3;
        let want = match (setup.length, setup.is_in()) {
            (0, _) => 0,
            (_, true) => 3,
            (_, false) => 2,
        };
        if trt != want {
            panic!("fake xhci: Setup Stage TRT {trt} for {setup:?}");
        }
        c.advance();
        let after_setup = c;
        let data = if trt == 0 {
            None
        } else {
            let Some((at, d)) = c.peek(dma) else {
                return Td::Incomplete;
            };
            let len = d[2] & 0x1_FFFF;
            if trb_type(&d) != DATA_STAGE
                || (d[3] & DIR_IN != 0) != setup.is_in()
                || len != setup.length as u32
            {
                panic!("fake xhci: Data Stage {d:x?} does not match {setup:?}");
            }
            c.advance();
            Some((at, d))
        };
        let Some((status_at, st)) = c.peek(dma) else {
            return Td::Incomplete;
        };
        let status_in = data.is_none() || !setup.is_in();
        if trb_type(&st) != STATUS_STAGE || (st[3] & DIR_IN != 0) != status_in {
            panic!("fake xhci: Status Stage {st:x?} does not match {setup:?}");
        }
        c.advance();
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.devices.get(port as usize - 1).cloned().flatten() else {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
            return Td::Done;
        };
        let out = match data {
            Some((_, d)) if !setup.is_in() => dma.read_bytes(pointer(&d), setup.length as usize),
            _ => Vec::new(),
        };
        // USB 2.0 9.2.6.3: a device needs 2 ms after SET_ADDRESS before it
        // takes the next request.
        let recovered = self.slots[slot]
            .as_ref()
            .is_some_and(|s| self.now >= s.addressed_at + Duration::from_millis(2));
        if !recovered {
            self.fail(slot, 1, ep.ring, USB_TRANSACTION_ERROR, dma);
            return Td::Done;
        }
        let answer = dev.borrow_mut().control(setup, &out);
        let mut stage = after_setup;
        match answer {
            None => {
                let ep = self.ring(slot, 1).expect("EP0");
                ep.ring = after_setup;
                ep.busy = true;
                return Td::Waiting;
            }
            Some(Err(Stall)) => {
                self.fail(slot, 1, stage, STALL, dma);
                return Td::Done;
            }
            Some(Ok(bytes)) => {
                if let Some((at, d)) = data.filter(|_| setup.is_in()) {
                    let len = setup.length as usize;
                    let dev_mps = dev.borrow().max_packet0().max(1) as usize;
                    let ctx_mps = ep.max_packet as usize;
                    let mut sent = 0;
                    let mut babble = bytes.len() > len;
                    for chunk in bytes.chunks(dev_mps) {
                        // A packet bigger than the context's max packet
                        // size is babble; a smaller one ends the stage.
                        if chunk.len() > ctx_mps {
                            babble = true;
                            break;
                        }
                        dma.write_bytes(pointer(&d) + sent as u64, chunk);
                        sent += chunk.len();
                        if chunk.len() < ctx_mps {
                            break;
                        }
                    }
                    if babble {
                        self.fail(slot, 1, stage, BABBLE, dma);
                        return Td::Done;
                    }
                    if sent < len && d[3] & ISP != 0 {
                        let code = if self.config.short_as_success {
                            SUCCESS
                        } else {
                            SHORT_PACKET
                        };
                        self.post_transfer(slot, 1, at, code, (len - sent) as u32, dma);
                    }
                    stage.advance();
                }
            }
        }
        if st[3] & IOC != 0 {
            self.post_transfer(slot, 1, status_at, SUCCESS, 0, dma);
        }
        if let Some(ep) = self.ring(slot, 1) {
            ep.ring = c;
        }
        Td::Done
    }

    /// The device on `port` went away: every TD in progress of its slot
    /// fails with a USB Transaction Error, as Intel controllers do.
    pub(super) fn device_gone(&mut self, port: u8, dma: &Dma) {
        let slots: Vec<usize> = (1..self.slots.len())
            .filter(|&s| self.slots[s].as_ref().is_some_and(|s| s.port == port))
            .collect();
        for slot in slots {
            let busy: Vec<(usize, Consumer)> = self.slots[slot]
                .as_ref()
                .map(|s| {
                    s.endpoints
                        .iter()
                        .filter(|(_, ep)| ep.busy)
                        .map(|(&dci, ep)| (dci, ep.ring))
                        .collect()
                })
                .unwrap_or_default();
            for (dci, at) in busy {
                self.fail(slot, dci, at, USB_TRANSACTION_ERROR, dma);
            }
        }
    }
}
````

- [ ] **Step 8: Write the failing tests for `crates/usb/src/xhci/device.rs`**

Create `crates/usb/src/xhci/device.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::regs::PR;
    use super::*;
    use crate::bus::GET_DESCRIPTOR;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn plugged(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(Duration::from_millis(60));
        xhci.port_changes();
        (hal, xhci)
    }

    /// The commands the fake executed, by TRB type.
    fn commands(hal: &FakeHal) -> Vec<u32> {
        hal.fake().executed().iter().map(|e| e.kind).collect()
    }

    fn requests(dev: &Dev) -> Vec<(u8, u16, u16)> {
        let r = dev.borrow().requests();
        r.iter()
            .map(|r| (r.setup.request, r.setup.value, r.setup.length))
            .collect()
    }

    #[test]
    fn the_k120_is_addressed_with_ep0_8_and_its_descriptors_parse() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let dev = xhci.attach(1).unwrap();
        assert_eq!((dev.slot, dev.port, dev.speed), (1, 1, Speed::Low));
        assert_eq!(
            (dev.descriptor.vendor, dev.descriptor.product),
            (0x046D, 0xC31C)
        );
        assert_eq!(dev.configuration.value, 1);
        assert_eq!(dev.configuration.interfaces.len(), 2);
        assert_eq!(xhci.slot_of_port(1), Some(1));
        assert_eq!(
            k120.borrow().address(),
            1,
            "SET_ADDRESS from Address Device"
        );
        let ep0 = hal.fake().endpoint(1, 1).unwrap();
        assert_eq!((ep0.ep_type, ep0.max_packet, ep0.cerr), (4, 8, 3));
        assert_eq!(ep0.average_trb_length, 8);
        assert_eq!(hal.fake().slot(1).unwrap().port, 1);
        let log = hal.log_text();
        assert!(log.contains(
            "xhci 00:14.0: port 1: slot 1 at address 1, 046d:c31c USB 1.10, ep0 8 bytes, 1 configuration"
        ));
        assert!(log.contains(
            "xhci 00:14.0: slot 1: interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 10"
        ));
        assert!(
            !commands(&hal).contains(&13),
            "no Evaluate Context for 8 bytes"
        );
    }

    #[test]
    fn the_superspeed_stick_gets_512_at_64_byte_contexts() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 13, &stick);
        let dev = xhci.attach(13).unwrap();
        assert_eq!(dev.speed, Speed::Super);
        assert_eq!(dev.descriptor.product, 0x1666);
        assert_eq!(
            hal.fake()
                .endpoint(dev.slot as usize, 1)
                .unwrap()
                .max_packet,
            512
        );
        assert!(!commands(&hal).contains(&13));
        assert!(hal.log_text().contains("0951:1666 USB 3.20, ep0 512 bytes"));
    }

    #[test]
    fn the_unifying_receiver_is_asked_for_8_bytes_before_ep0_becomes_64() {
        let receiver = FakeUsbDevice::unifying_receiver();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 2, &receiver);
        let dev = xhci.attach(2).unwrap();
        assert_eq!(dev.configuration.interfaces.len(), 3);
        assert_eq!(
            requests(&receiver),
            [
                (5, 1, 0),
                (GET_DESCRIPTOR, 0x0100, 8),
                (GET_DESCRIPTOR, 0x0100, 18),
                (GET_DESCRIPTOR, 0x0200, 9),
                (GET_DESCRIPTOR, 0x0200, 84),
            ]
        );
        assert!(commands(&hal).contains(&13), "Evaluate Context");
        assert_eq!(hal.fake().endpoint(1, 1).unwrap().max_packet, 64);
    }

    #[test]
    fn a_stall_fails_the_attach_and_frees_everything() {
        for (config, port, dev) in [
            (FakeConfig::basic(), 1, FakeUsbDevice::k120()),
            (FakeConfig::intel(), 13, FakeUsbDevice::kingston_stick()),
        ] {
            dev.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0100);
            let (hal, mut xhci) = plugged(config, port, &dev);
            let before = hal.outstanding_dma();
            assert_eq!(xhci.attach(port).err(), Some(UsbError::Stall));
            assert_eq!(hal.outstanding_dma(), before);
            assert!(!hal.fake().slot_enabled(1), "the slot is disabled");
            assert_eq!(xhci.dcbaa.read64(8), 0);
            assert_eq!(xhci.slot_of_port(port), None);
            let log = hal.log_text();
            let reason = alloc::format!("port {port}: device descriptor (8 bytes): stalled");
            assert!(log.contains(&reason), "{log}");
            assert!(log.contains("attach failed (stalled), slot 1 released"));
        }
    }

    #[test]
    fn a_device_that_never_answers_fails_its_attach_after_a_second() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().ignore_requests(1);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 3, &k120);
        let before = (hal.outstanding_dma(), hal.clock());
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Timeout));
        assert!(hal.clock() - before.1 >= Duration::from_secs(1));
        assert_eq!(hal.outstanding_dma(), before.0);
        // Stop Endpoint and Set TR Dequeue Pointer before Disable Slot.
        assert!(commands(&hal).ends_with(&[15, 16, 10]));
    }

    #[test]
    fn running_out_of_memory_for_a_slot_frees_what_it_got() {
        for n in 0..4 {
            let k120 = FakeUsbDevice::k120();
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
            let before = hal.outstanding_dma();
            hal.fail_one_alloc(n);
            assert_eq!(
                xhci.attach(1).err(),
                Some(UsbError::NoMemory),
                "allocation {n}"
            );
            assert_eq!(hal.outstanding_dma(), before, "allocation {n}");
            assert!(!hal.fake().slot_enabled(1));
            assert!(
                hal.log_text()
                    .contains("port 1: slot memory: out of memory")
            );
        }
    }

    #[test]
    fn a_short_configuration_still_parses_what_arrived() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().truncate_configuration(34);
        let (_hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let dev = xhci.attach(1).unwrap();
        assert_eq!(dev.configuration.interfaces.len(), 1);
    }

    #[test]
    fn a_configuration_over_4096_bytes_is_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut config = k120.borrow().configuration_descriptor().to_vec();
        config[2..4].copy_from_slice(&5000u16.to_le_bytes());
        let device = k120.borrow().device_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let before = hal.outstanding_dma();
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::BadDescriptor("configuration too large"))
        );
        assert_eq!(hal.outstanding_dma(), before);
        assert!(
            hal.log_text()
                .contains("configuration descriptor: bad descriptor: configuration too large")
        );
    }

    #[test]
    fn a_device_without_configurations_is_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut device = k120.borrow().device_descriptor().to_vec();
        device[17] = 0;
        let config = k120.borrow().configuration_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config);
        let (_hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::Unsupported("no configuration"))
        );
    }

    #[test]
    fn a_device_unplugged_mid_enumeration_fails_cleanly() {
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().ignore_requests(1);
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 3, &k120);
        let before = hal.outstanding_dma();
        hal.fake()
            .after(Duration::from_millis(300), |x, _| x.unplug(3));
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Disconnected));
        assert!(hal.clock() < Duration::from_secs(1), "no timeout needed");
        assert_eq!(hal.outstanding_dma(), before);
        assert!(!hal.fake().slot_enabled(1));
        assert!(hal.log_text().contains("USB transaction error"));
        assert!(
            hal.log_text()
                .contains("port 3: attach failed (device disconnected)")
        );
    }

    #[test]
    fn a_device_unplugged_within_the_debounce_is_not_reset() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &k120);
        let start = hal.clock();
        hal.fake()
            .after(Duration::from_millis(50), |x, _| x.unplug(1));
        assert_eq!(xhci.attach(1).err(), Some(UsbError::Disconnected));
        assert!(hal.clock() - start >= DEBOUNCE);
        let resets = hal
            .fake()
            .portsc_writes()
            .iter()
            .filter(|(p, v)| *p == 1 && v & PR != 0)
            .count();
        assert_eq!(resets, 0);
    }

    #[test]
    fn ports_without_a_device_or_a_protocol_are_refused() {
        let (_hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.attach(2).err(), Some(UsbError::Disconnected));
        assert_eq!(
            xhci.attach(9).err(),
            Some(UsbError::Unsupported("no such port"))
        );
        assert_eq!(
            xhci.attach(0).err(),
            Some(UsbError::Unsupported("no such port"))
        );
    }

    #[test]
    fn a_port_is_attached_once_and_a_new_device_after_a_failure_works() {
        let bad = FakeUsbDevice::k120();
        bad.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0200);
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &bad);
        assert_eq!(xhci.attach(1).err(), Some(UsbError::Stall));
        hal.fake().unplug(1);
        hal.fake().plug(1, FakeUsbDevice::k120());
        xhci.port_changes();
        assert_eq!(xhci.attach(1).map(|d| d.slot), Ok(1));
        assert_eq!(
            xhci.attach(1).err(),
            Some(UsbError::Unsupported("port already attached"))
        );
    }

    #[test]
    fn a_slot_whose_disable_fails_keeps_its_memory() {
        let mut config = FakeConfig::intel();
        config.hang_command = Some(10); // Disable Slot
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0100);
        let (hal, mut xhci) = plugged(config, 3, &k120);
        let before = hal.outstanding_dma();
        assert_eq!(xhci.attach(3).err(), Some(UsbError::Stall));
        assert!(
            hal.fake().slot_enabled(1),
            "the controller still has the slot"
        );
        assert_eq!(
            hal.outstanding_dma(),
            before + 4,
            "contexts, EP0 ring and buffer kept"
        );
        assert_eq!(xhci.slot_of_port(3), None, "forgotten all the same");
        assert!(
            hal.log_text().contains(
                "xhci 00:14.0: slot 1: Disable Slot failed (timed out); its memory is kept"
            )
        );
    }
}
````

- [ ] **Step 9: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
mod context;
mod init;
````

with:

````rust
mod context;
mod device;
mod init;
````

Replace:

````rust
mod start;
mod trb;
````

with:

````rust
mod start;
mod transfer;
mod trb;
````

- [ ] **Step 10: Write the failing tests for `crates/usb/src/xhci/transfer.rs`**

Create `crates/usb/src/xhci/transfer.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::super::Device;
    use super::*;
    use crate::bus::GET_DESCRIPTOR;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::vec::Vec;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn attached(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>, Device) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(Duration::from_millis(60));
        xhci.port_changes();
        let d = xhci.attach(port).unwrap();
        (hal, xhci, d)
    }

    fn both() -> [(FakeConfig, u8, Dev); 2] {
        [
            (FakeConfig::basic(), 1, FakeUsbDevice::k120()),
            (FakeConfig::intel(), 13, FakeUsbDevice::kingston_stick()),
        ]
    }

    fn commands_since(hal: &FakeHal, n: usize) -> Vec<u32> {
        hal.fake().executed()[n..].iter().map(|e| e.kind).collect()
    }

    fn get_device(xhci: &mut Xhci<FakeHal>, slot: u8, len: u16) -> Result<usize, UsbError> {
        let mut buf = [0; 64];
        xhci.control_transfer(slot, Setup::get_descriptor(1, 0, len), &mut buf)
    }

    #[test]
    fn a_short_in_data_stage_returns_the_bytes_that_came() {
        for (config, port, dev) in both() {
            let (_hal, mut xhci, d) = attached(config, port, &dev);
            let mut buf = [0; 64];
            let n = xhci.control_transfer(d.slot, Setup::get_descriptor(1, 0, 64), &mut buf);
            assert_eq!(n, Ok(18));
            assert_eq!(&buf[..18], dev.borrow().device_descriptor());
        }
    }

    #[test]
    fn requests_without_data_and_out_requests_work() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::set_configuration(1), &mut []),
            Ok(0)
        );
        let k120 = FakeUsbDevice::k120();
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 2, &k120);
        let set_report = Setup {
            request_type: 0x21,
            request: 9,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(
            xhci.control_transfer(d.slot, set_report, &mut [0x02]),
            Ok(1)
        );
        let last = k120.borrow().requests().pop().unwrap();
        assert_eq!((last.setup, last.data), (set_report, vec![0x02]));
    }

    #[test]
    fn a_stall_resets_ep0_and_the_next_request_works() {
        for (config, port, dev) in both() {
            let (hal, mut xhci, d) = attached(config, port, &dev);
            dev.borrow_mut().stall_request(GET_DESCRIPTOR, 0x0300);
            let n = hal.fake().executed().len();
            let mut buf = [0; 4];
            let r = xhci.control_transfer(d.slot, Setup::get_descriptor(3, 0, 4), &mut buf);
            assert_eq!(r, Err(UsbError::Stall));
            // Reset Endpoint, then Set TR Dequeue Pointer.
            assert_eq!(commands_since(&hal, n), [14, 16]);
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18));
            assert!(
                hal.log_text()
                    .contains("control request 0x80/6 failed: stall")
            );
        }
    }

    #[test]
    fn a_request_never_answered_times_out_and_ep0_is_repositioned() {
        for (config, port, dev) in both() {
            let (hal, mut xhci, d) = attached(config, port, &dev);
            dev.borrow_mut().ignore_requests(1);
            let (n, before) = (hal.fake().executed().len(), hal.clock());
            assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
            let waited = hal.clock() - before;
            assert!(waited >= CONTROL_TIMEOUT && waited < CONTROL_TIMEOUT * 2);
            // Stop Endpoint, then Set TR Dequeue Pointer.
            assert_eq!(commands_since(&hal, n), [15, 16]);
            assert_eq!(
                get_device(&mut xhci, d.slot, 18),
                Ok(18),
                "the device answers now"
            );
            assert_eq!(
                hal.fake().endpoint(d.slot as usize, 1).unwrap().state,
                1,
                "running"
            );
        }
    }

    #[test]
    fn many_requests_wrap_the_ep0_ring() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        for i in 0..200 {
            assert_eq!(get_device(&mut xhci, d.slot, 18), Ok(18), "request {i}");
        }
    }

    #[test]
    fn requests_the_driver_cannot_do_are_refused() {
        let (_hal, mut xhci, d) = attached(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
        let mut small = [0; 8];
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::get_descriptor(1, 0, 18), &mut small),
            Err(UsbError::Unsupported("buffer smaller than request"))
        );
        let mut big = vec![0; 5000];
        assert_eq!(
            xhci.control_transfer(d.slot, Setup::get_descriptor(2, 0, 5000), &mut big),
            Err(UsbError::Unsupported("request over 4096 bytes"))
        );
        assert_eq!(get_device(&mut xhci, 7, 18), Err(UsbError::Disconnected));
    }

    #[test]
    fn a_short_data_stage_reported_as_success_gives_its_length() {
        let mut config = FakeConfig::basic();
        config.short_as_success = true;
        let (_hal, mut xhci, d) = attached(config, 1, &FakeUsbDevice::k120());
        assert_eq!(get_device(&mut xhci, d.slot, 64), Ok(18));
    }

    #[test]
    fn without_unplug_failures_a_request_to_a_gone_device_times_out() {
        let mut config = FakeConfig::basic();
        config.fail_transfers_on_unplug = false;
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(config, 1, &k120);
        k120.borrow_mut().ignore_requests(1);
        hal.fake()
            .after(Duration::from_millis(5), |x, _| x.unplug(1));
        assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
    }
}
````

- [ ] **Step 11: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Xhci` in this scope ``; `` cannot find type `Duration` in this scope ``.

- [ ] **Step 12: Change `crates/usb/src/xhci/command.rs`**

In `crates/usb/src/xhci/command.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT,
    PORT_STATUS_CHANGE, SUCCESS, Trb, command_name, completion_name,
};
````

with:

````rust
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT,
    PORT_STATUS_CHANGE, SUCCESS, TRANSFER_EVENT, Trb, command_name, completion_name,
};
````

Replace:

````rust
            PORT_STATUS_CHANGE => self.port_event(event.port_id()),
            HOST_CONTROLLER_EVENT => xlog!(
````

with:

````rust
            PORT_STATUS_CHANGE => self.port_event(event.port_id()),
            TRANSFER_EVENT => self.transfer_event(event),
            HOST_CONTROLLER_EVENT => xlog!(
````

- [ ] **Step 13: Implement `crates/usb/src/xhci/device.rs`**

Insert this at the top of `crates/usb/src/xhci/device.rs`, above `#[cfg(test)]`:

````rust
//! Devices (spec §6.2 steps 1-5): enumeration on a root port. Each step
//! logs why it failed; a failed attach disables its slot and frees
//! everything it allocated, so a misbehaving device costs nothing else.

use super::context::{CONTROL, EndpointContext, Input, Output, SlotContext, speed_id};
use super::regs::CCS;
use super::transfer::DATA_BUFFER_SIZE;
use super::trb::Trb;
use super::{Device, Slot, Xhci};
use crate::descriptor::{
    self, CONFIGURATION, DEVICE, DEVICE_LEN, DeviceDescriptor, EndpointKind, Interface,
};
use crate::{Hal, Setup, Speed, UsbError};
use alloc::string::String;
use alloc::vec;
use core::fmt::Write;
use core::time::Duration;

/// A connection must last this long before the port is reset (USB 2.0
/// 7.1.7.3: 100 ms of debounce).
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long a device gets after SET_ADDRESS before its next request.
const SET_ADDRESS_RECOVERY: Duration = Duration::from_millis(10);
/// Input control context flags: A0 is the slot context, A1 EP0.
const A0: u32 = 1 << 0;
const A1: u32 = 1 << 1;

/// EP0's context: control, 3 retries, `max_packet`, its ring, and an
/// average TRB length of 8 (xHCI 4.14.1.1: setup packets).
fn ep0_context(max_packet: u16, dequeue: u64) -> EndpointContext {
    EndpointContext {
        ep_type: CONTROL,
        cerr: 3,
        max_packet,
        dequeue,
        average_trb_length: 8,
        ..EndpointContext::default()
    }
}

fn kind_name(kind: EndpointKind) -> &'static str {
    match kind {
        EndpointKind::Control => "control",
        EndpointKind::Isochronous => "isochronous",
        EndpointKind::Bulk => "bulk",
        EndpointKind::Interrupt => "interrupt",
    }
}

/// "interface 0 class 3/1/1, endpoints 0x81 interrupt 8 bytes interval 10"
fn describe(interface: &Interface) -> String {
    let mut line = String::new();
    let i = interface;
    let _ = write!(
        line,
        "interface {} class {}/{}/{}",
        i.number, i.class, i.subclass, i.protocol
    );
    for (n, e) in i.endpoints.iter().enumerate() {
        let sep = if n == 0 { ", endpoints " } else { ", " };
        let _ = write!(
            line,
            "{sep}{:#04x} {} {} bytes interval {}",
            e.address,
            kind_name(e.kind),
            e.packet_size(),
            e.interval
        );
    }
    line
}

impl<H: Hal> Xhci<H> {
    /// The slot of the device attached on `port`, if any.
    pub fn slot_of_port(&self, port: u8) -> Option<u8> {
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.port == port))
            .map(|slot| slot as u8)
    }

    fn connected(&self, port: u8) -> bool {
        self.regs.portsc(&self.hal, port) & CCS != 0
    }

    /// Sets up the device on `port` (spec §6.2 steps 1-5): debounce (the
    /// port must still be connected 100 ms after this call starts), port
    /// reset, Enable Slot, Address Device, device descriptor (fixing EP0's
    /// packet size), configuration descriptor. On an error the slot is
    /// disabled and everything allocated for it freed.
    pub fn attach(&mut self, port: u8) -> Result<Device, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        if port == 0 || port > self.info.ports {
            return Err(UsbError::Unsupported("no such port"));
        }
        if self.ports[port as usize - 1].is_none() {
            return Err(UsbError::Unsupported("port without a supported protocol"));
        }
        if let Some(slot) = self.slot_of_port(port) {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: already attached as slot {slot}"
            );
            return Err(UsbError::Unsupported("port already attached"));
        }
        let start = self.hal.now();
        if self.connected(port) {
            let waited = self.hal.now() - start;
            self.hal.sleep(DEBOUNCE.saturating_sub(waited));
        }
        if !self.connected(port) {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: not connected after debounce"
            );
            return Err(UsbError::Disconnected);
        }
        let speed = self.reset_port(port)?;
        let slot = self.enable_slot(port)?;
        match self.enumerate(slot, port, speed) {
            Ok(device) => Ok(device),
            Err(e) => {
                let e = if self.connected(port) {
                    e
                } else {
                    UsbError::Disconnected
                };
                xlog!(
                    &self.hal,
                    &self.name,
                    "port {port}: attach failed ({e}), slot {slot} released"
                );
                self.release_slot(slot);
                Err(e)
            }
        }
    }

    fn enable_slot(&mut self, port: u8) -> Result<u8, UsbError> {
        let done = self
            .command(Trb::enable_slot())
            .inspect_err(|e| xlog!(&self.hal, &self.name, "port {port}: Enable Slot: {e}"))?;
        let slot = done.slot;
        if slot == 0 || slot > self.info.max_slots || self.slots[slot as usize].is_some() {
            xlog!(
                &self.hal,
                &self.name,
                "port {port}: Enable Slot gave slot {slot}"
            );
            return Err(UsbError::Unsupported("bad slot ID from Enable Slot"));
        }
        Ok(slot)
    }

    /// Logs a failed step of the attach on `port`.
    fn step<T>(&self, port: u8, what: &str, r: Result<T, UsbError>) -> Result<T, UsbError> {
        r.inspect_err(|e| xlog!(&self.hal, &self.name, "port {port}: {what}: {e}"))
    }

    /// GET_DESCRIPTOR into `buf`; the number of bytes that came.
    fn get_descriptor(
        &mut self,
        port: u8,
        slot: u8,
        (kind, what): (u8, &str),
        buf: &mut [u8],
    ) -> Result<usize, UsbError> {
        let setup = Setup::get_descriptor(kind, 0, buf.len() as u16);
        let r = self.control_transfer(slot, setup, buf);
        self.step(port, what, r)
    }

    fn enumerate(&mut self, slot: u8, port: u8, speed: Speed) -> Result<Device, UsbError> {
        let stride = self.info.context_size;
        let s = Slot::new(&self.hal, port, speed, stride);
        let s = self.step(port, "slot memory", s)?;
        self.dcbaa.write64(8 * slot as usize, s.output.phys());
        self.slots[slot as usize] = Some(s);
        let r = self.address_device(slot);
        self.step(port, "Address Device", r)?;
        let mut prefix = [0; 8];
        let n = self.get_descriptor(
            port,
            slot,
            (DEVICE, "device descriptor (8 bytes)"),
            &mut prefix,
        )?;
        let max_packet0 = self.step(
            port,
            "EP0 packet size",
            descriptor::max_packet0(&prefix[..n], speed),
        )?;
        if max_packet0 != speed.default_max_packet0() {
            let r = self.set_max_packet0(slot, max_packet0);
            self.step(port, "Evaluate Context", r)?;
        }
        let mut raw = [0; DEVICE_LEN];
        let n = self.get_descriptor(port, slot, (DEVICE, "device descriptor"), &mut raw)?;
        let desc = self.step(
            port,
            "device descriptor",
            DeviceDescriptor::parse(&raw[..n]),
        )?;
        if desc.configurations == 0 {
            return self.step(
                port,
                "device descriptor",
                Err(UsbError::Unsupported("no configuration")),
            );
        }
        let mut header = [0; descriptor::CONFIGURATION_LEN];
        let what = (CONFIGURATION, "configuration descriptor");
        let n = self.get_descriptor(port, slot, what, &mut header)?;
        let total = self.step(port, what.1, descriptor::configuration_length(&header[..n]))?;
        if total as usize > DATA_BUFFER_SIZE {
            let e = Err(UsbError::BadDescriptor("configuration too large"));
            return self.step(port, what.1, e);
        }
        let mut raw = vec![0; total as usize];
        let n = self.get_descriptor(port, slot, what, &mut raw)?;
        let configuration = self.step(port, what.1, descriptor::parse_configuration(&raw[..n]))?;
        let plural = if desc.configurations == 1 { "" } else { "s" };
        // The USB address the controller gave the device (xHCI 6.2.2).
        let address = self.slots[slot as usize]
            .as_ref()
            .map_or(0, |s| Output::new(&s.output, stride).slot().address);
        xlog!(
            &self.hal,
            &self.name,
            "port {port}: slot {slot} at address {address}, {:04x}:{:04x} USB {:x}.{:02x}, \
             ep0 {max_packet0} bytes, {} configuration{plural}",
            desc.vendor,
            desc.product,
            desc.usb_version >> 8,
            desc.usb_version & 0xFF,
            desc.configurations
        );
        for interface in &configuration.interfaces {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: {}",
                describe(interface)
            );
        }
        Ok(Device {
            slot,
            port,
            speed,
            descriptor: desc,
            configuration,
        })
    }

    /// Address Device with BSR = 0 (xHCI 4.3.3): the slot context (speed,
    /// one context entry, root port) and EP0 with the speed's default
    /// packet size; the controller sends SET_ADDRESS.
    fn address_device(&mut self, slot: u8) -> Result<(), UsbError> {
        let s = self.slots[slot as usize]
            .as_ref()
            .ok_or(UsbError::Disconnected)?;
        let input = Input::new(&s.input, self.info.context_size);
        input.clear();
        input.set_flags(0, A0 | A1);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            context_entries: 1,
            root_port: s.port,
            ..SlotContext::default()
        });
        input.set_endpoint(1, &ep0_context(s.max_packet0, s.ep0.enqueue_pointer()));
        let trb = Trb::address_device(s.input.phys(), slot, false);
        self.command(trb)?;
        // SET_ADDRESS recovery: USB 2.0 9.2.6.3 gives the device 2 ms; Linux
        // waits 10.
        self.hal.sleep(SET_ADDRESS_RECOVERY);
        Ok(())
    }

    /// Evaluate Context with EP0's real packet size (xHCI 4.6.7).
    fn set_max_packet0(&mut self, slot: u8, max_packet: u16) -> Result<(), UsbError> {
        let s = self.slots[slot as usize]
            .as_mut()
            .ok_or(UsbError::Disconnected)?;
        let input = Input::new(&s.input, self.info.context_size);
        input.clear();
        input.set_flags(0, A1);
        input.set_endpoint(1, &ep0_context(max_packet, s.ep0.enqueue_pointer()));
        let trb = Trb::evaluate_context(s.input.phys(), slot);
        self.command(trb)?;
        if let Some(s) = self.slots[slot as usize].as_mut() {
            s.max_packet0 = max_packet;
        }
        Ok(())
    }

    /// Disable Slot (a failure is logged; the slot is forgotten anyway),
    /// DCBAA[slot] = 0, and every ring, context and buffer freed.
    ///
    /// The memory is freed only once Disable Slot succeeded: until then the
    /// controller may still write to it (a transfer in progress, the device
    /// context). If it failed, timed out or the controller is dead, the
    /// memory is kept (leaked) and the slot forgotten.
    pub(super) fn release_slot(&mut self, slot: u8) {
        let s = self.slots[slot as usize].take();
        let outcome = if self.dead {
            Err(UsbError::ControllerDead)
        } else {
            self.command(Trb::disable_slot(slot)).map(|_| ())
        };
        match (outcome, s) {
            (Ok(()), s) => {
                self.dcbaa.write64(8 * slot as usize, 0);
                if let Some(s) = s {
                    s.free(&self.hal);
                }
            }
            (Err(e), _) => xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: Disable Slot failed ({e}); its memory is kept"
            ),
        }
    }
}

````

- [ ] **Step 14: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use crate::{DmaBuf, Hal};
use alloc::string::String;
````

with:

````rust

use crate::descriptor::{Configuration, DeviceDescriptor};
use crate::{DmaBuf, Hal, Speed, UsbError};
use alloc::string::String;
````

Replace:

````rust

/// A root port whose state changed since the last `port_changes()`.
````

with:

````rust

/// An addressed device: what enumeration found (spec §6.2 steps 1-5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub slot: u8,
    pub port: u8,
    pub speed: Speed,
    pub descriptor: DeviceDescriptor,
    /// The first configuration, parsed.
    pub configuration: Configuration,
}

/// A root port whose state changed since the last `port_changes()`.
````

Replace:

````rust
    pub reconnected: bool,
}
````

with:

````rust
    pub reconnected: bool,
}

/// A control request in flight on EP0: its TRBs, and what their events
/// said so far.
#[derive(Debug)]
struct Control {
    setup: u64,
    data: Option<u64>,
    status: u64,
    /// Bytes of the data stage not transferred (a short packet).
    residual: u32,
    /// `Err` holds the completion code that ended it.
    result: Option<Result<(), u8>>,
}

/// A device slot: its contexts, EP0 and the buffer control transfers use.
#[derive(Debug)]
struct Slot {
    port: u8,
    speed: Speed,
    /// The device (output) context the controller writes; DCBAA[slot].
    output: DmaBuf,
    /// The input context of this slot's commands.
    input: DmaBuf,
    ep0: ProducerRing,
    /// Control data stages go through this 4 KiB buffer.
    data: DmaBuf,
    /// EP0's max packet size as its context has it.
    max_packet0: u16,
    control: Option<Control>,
}

impl Slot {
    /// Allocates everything a slot needs, or nothing.
    fn new<H: Hal>(hal: &H, port: u8, speed: Speed, stride: usize) -> Result<Slot, UsbError> {
        let output = hal.alloc_dma(context::device_size(stride), 64);
        let input = hal.alloc_dma(context::input_size(stride), 64);
        let ep0 = ProducerRing::new(hal).ok();
        let data = hal.alloc_dma(transfer::DATA_BUFFER_SIZE, 64);
        match (output, input, ep0, data) {
            (Some(output), Some(input), Some(ep0), Some(data)) => Ok(Slot {
                port,
                speed,
                output,
                input,
                ep0,
                data,
                max_packet0: speed.default_max_packet0(),
                control: None,
            }),
            (output, input, ep0, data) => {
                for buf in [output, input, data].into_iter().flatten() {
                    hal.free_dma(buf);
                }
                if let Some(ring) = ep0 {
                    ring.free(hal);
                }
                Err(UsbError::NoMemory)
            }
        }
    }

    fn free<H: Hal>(self, hal: &H) {
        hal.free_dma(self.output);
        hal.free_dma(self.input);
        hal.free_dma(self.data);
        self.ep0.free(hal);
    }
}
````

Replace:

````rust
    first_scan: bool,
    /// Set when the controller stopped working: nothing is sent to it any
````

with:

````rust
    first_scan: bool,
    /// Index = slot ID (0 unused).
    slots: Vec<Option<Slot>>,
    /// Set when the controller stopped working: nothing is sent to it any
````

- [ ] **Step 15: Change `crates/usb/src/xhci/start.rs`**

In `crates/usb/src/xhci/start.rs`, replace:

````rust
            first_scan: true,
        };
````

with:

````rust
            first_scan: true,
            slots: (0..=params.max_slots).map(|_| None).collect(),
        };
````

- [ ] **Step 16: Implement `crates/usb/src/xhci/transfer.rs`**

Insert this at the top of `crates/usb/src/xhci/transfer.rs`, above `#[cfg(test)]`:

````rust
//! Transfers (xHCI 4.11): control transfers on EP0, matching transfer
//! events to what is in flight, and putting an endpoint back in order
//! after a STALL or a timeout.

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output};
use super::trb::{SHORT_PACKET, STALL, SUCCESS, Trb, completion_name};
use super::{Control, Xhci};
use crate::{Hal, Setup, UsbError};
use core::sync::atomic::{Ordering, fence};
use core::time::Duration;

/// How long a control transfer may take (spec §6.2).
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
/// The largest transfer: one page of DMA buffer.
pub const DATA_BUFFER_SIZE: usize = 4096;
/// How often a transfer wait polls.
const TRANSFER_POLL: Duration = Duration::from_micros(10);
/// EP0's Device Context Index.
const EP0: usize = 1;

impl Control {
    /// Takes in a transfer event for one of this request's TRBs. Each stage
    /// is its own TD (xHCI 4.11.2.2): a short IN data stage reports the
    /// residual and the status stage still follows.
    fn on_event(&mut self, event: &Trb) {
        let (trb, code) = (event.pointer(), event.completion_code());
        if self.result.is_some() {
            return;
        }
        // Some controllers report a short data stage as Success with the
        // residual (Linux: XHCI_TRUST_TX_LENGTH); both mean the same.
        if Some(trb) == self.data && matches!(code, SHORT_PACKET | SUCCESS) {
            self.residual = event.transfer_length();
        } else if trb == self.status && code == SUCCESS {
            self.result = Some(Ok(()));
        } else if (trb == self.setup || Some(trb) == self.data || trb == self.status)
            && code != SUCCESS
        {
            self.result = Some(Err(code));
        }
    }
}

impl<H: Hal> Xhci<H> {
    /// A control transfer on EP0 of `slot` (the `Bus::control` contract):
    /// at most 4096 bytes, 1 s at most. After a STALL, an error or a
    /// timeout EP0 is reset or stopped and repositioned, so the next
    /// request works.
    pub(super) fn control_transfer(
        &mut self,
        slot: u8,
        setup: Setup,
        data: &mut [u8],
    ) -> Result<usize, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let len = setup.length as usize;
        if data.len() < len {
            return Err(UsbError::Unsupported("buffer smaller than request"));
        }
        if len > DATA_BUFFER_SIZE {
            return Err(UsbError::Unsupported("request over 4096 bytes"));
        }
        let Some(s) = self.slots.get_mut(slot as usize).and_then(Option::as_mut) else {
            return Err(UsbError::Disconnected);
        };
        if s.control.is_some() {
            return Err(UsbError::Unsupported("control request in flight"));
        }
        if !setup.is_in() {
            s.data.write_bytes(0, &data[..len]);
        }
        let setup_trb = s.ep0.push(Trb::setup_stage(&setup));
        let data_trb = (len > 0).then(|| {
            s.ep0
                .push(Trb::data_stage(s.data.phys(), len as u32, setup.is_in()))
        });
        // The status stage goes the other way; IN when there is no data.
        let status = s.ep0.push(Trb::status_stage(len == 0 || !setup.is_in()));
        s.control = Some(Control {
            setup: setup_trb,
            data: data_trb,
            status,
            residual: 0,
            result: None,
        });
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, EP0 as u32);
        let result = self.wait_control(slot as usize);
        let s = self.slots[slot as usize]
            .as_mut()
            .expect("the slot outlives its request");
        let residual = s.control.take().map_or(0, |c| c.residual);
        let failure = match result {
            Some(Ok(())) => {
                let n = len.saturating_sub(residual as usize);
                if setup.is_in() {
                    s.data.read_bytes(0, &mut data[..n]);
                }
                return Ok(n);
            }
            Some(Err(STALL)) => UsbError::Stall,
            Some(Err(code)) => UsbError::Transfer(code),
            None if self.dead => return Err(UsbError::ControllerDead),
            None => UsbError::Timeout,
        };
        let what = match failure {
            UsbError::Transfer(code) => completion_name(code),
            UsbError::Stall => "stall",
            _ => "timed out",
        };
        xlog!(
            &self.hal,
            &self.name,
            "slot {slot}: control request {:#04x}/{} failed: {what}",
            setup.request_type,
            setup.request
        );
        if let Err(e) = self.reposition(slot as usize, EP0) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: EP0 recovery failed: {e}"
            );
        }
        Err(failure)
    }

    /// Polls until the request on `slot`'s EP0 ends; `None` after 1 s or
    /// when the controller dies.
    fn wait_control(&mut self, slot: usize) -> Option<Result<(), u8>> {
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                return None;
            }
            let control = self.slots[slot].as_ref().and_then(|s| s.control.as_ref());
            if let Some(result) = control.and_then(|c| c.result) {
                return Some(result);
            }
            if self.hal.now() - start >= CONTROL_TIMEOUT {
                return None;
            }
            self.hal.sleep(TRANSFER_POLL);
        }
    }

    /// A transfer event: it goes to the request it names. Events for a
    /// slot or TRB nothing waits for (a detached device, a request given
    /// up) are dropped.
    pub(super) fn transfer_event(&mut self, event: Trb) {
        let Some(s) = self
            .slots
            .get_mut(event.slot_id() as usize)
            .and_then(Option::as_mut)
        else {
            return;
        };
        if event.endpoint_id() == EP0
            && let Some(control) = s.control.as_mut()
        {
            control.on_event(&event);
        }
    }

    /// Makes an endpoint usable after a halt or a timeout: Reset Endpoint
    /// if it is Halted, Stop Endpoint if it still runs, then Set TR Dequeue
    /// Pointer to where the next TRB will go, dropping whatever was queued.
    pub(super) fn reposition(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        match self.endpoint_state(slot, dci)? {
            EP_HALTED => {
                self.command(Trb::reset_endpoint(slot as u8, dci))?;
            }
            EP_RUNNING => {
                self.command(Trb::stop_endpoint(slot as u8, dci))?;
            }
            EP_STOPPED | EP_ERROR => {}
            EP_DISABLED => return Err(UsbError::Disconnected),
            _ => return Err(UsbError::Unsupported("reserved endpoint state")),
        }
        self.set_dequeue(slot, dci)
    }

    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        let dequeue = if dci == EP0 {
            s.ep0.enqueue_pointer()
        } else {
            return Err(UsbError::Disconnected);
        };
        self.command(Trb::set_tr_dequeue(slot as u8, dci, dequeue))?;
        Ok(())
    }

    /// The endpoint's state as the controller last wrote it.
    fn endpoint_state(&self, slot: usize, dci: usize) -> Result<u8, UsbError> {
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        Ok(Output::new(&s.output, self.info.context_size)
            .endpoint(dci)
            .state)
    }
}

````

- [ ] **Step 17: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 161 tests.

- [ ] **Step 18: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 19: Commit**

````bash
git add crates
git commit -m "usb: xHCI enumeration and control transfers"
````


### Task 12: Configuring endpoints, IN transfers and detaching

Spec §6.2 step 7 and what class drivers need. `configure` fills an endpoint context for each endpoint of the claimed interfaces (type, packet size, burst from the SuperSpeed companion or the high-speed extra transactions, the interval exponent, CErr 3, average TRB length and max ESIT payload), gives each a ring, runs Configure Endpoint and then `SET_CONFIGURATION`; other interfaces stay unconfigured, isochronous endpoints are skipped with a log line and a packet size of 0 is refused. The `Bus` implementation: `queue_in` puts one Normal TRB into the endpoint's buffer and returns; `take_in` polls, then returns a finished transfer once (the length is what was asked minus the residue); `clear_halt` resets or stops the endpoint as its state requires, repositions its ring and sends `CLEAR_FEATURE(ENDPOINT_HALT)`. `detach` disables the slot and frees everything once Disable Slot succeeded (otherwise the memory is kept and logged; on a dead controller nothing is freed); later events for it are ignored.

**Files:**
- Modify: `crates/usb/src/testing/device.rs`
- Modify: `crates/usb/src/testing/xhci/commands.rs`
- Modify: `crates/usb/src/testing/xhci/mod.rs`
- Modify: `crates/usb/src/testing/xhci/slots.rs`
- Modify: `crates/usb/src/testing/xhci/transfers.rs`
- Create: `crates/usb/src/xhci/configure.rs`
- Modify: `crates/usb/src/xhci/device.rs`
- Modify: `crates/usb/src/xhci/mod.rs`
- Modify: `crates/usb/src/xhci/regs.rs`
- Modify: `crates/usb/src/xhci/transfer.rs`

**Interfaces:**
- Consumes: Tasks 3–11.
- Produces: `Xhci::{configure(&mut self, &Device, interfaces: &[u8]) -> Result<(), UsbError>, detach(&mut self, slot: u8)}` and `impl<H: Hal> Bus for Xhci<H>`.

- [ ] **Step 1: Extend the test support in `crates/usb/src/testing/device.rs`**

In `crates/usb/src/testing/device.rs`, make these 7 replacements, top to bottom:

Replace:

````rust

use crate::bus::{GET_DESCRIPTOR, SET_CONFIGURATION};
use crate::descriptor::{CONFIGURATION, DEVICE};
use crate::{Setup, Speed};
use std::cell::RefCell;
use std::rc::Rc;
````

with:

````rust

use crate::bus::{
    CLEAR_FEATURE, ENDPOINT_HALT, GET_DESCRIPTOR, RECIPIENT_ENDPOINT, SET_CONFIGURATION,
};
use crate::descriptor::{CONFIGURATION, DEVICE};
use crate::{Setup, Speed};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
````

Replace:

````rust
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>>;
}
````

with:

````rust
    fn control(&mut self, setup: Setup, data_out: &[u8]) -> Option<Result<Vec<u8>, Stall>>;
    /// Up to `max_len` bytes from IN `endpoint`; `None` is a NAK.
    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>>;
    /// `data` for OUT `endpoint`; `None` is a NAK.
    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>>;
}
````

Replace:

````rust
    configuration_value: u8,
}
````

with:

````rust
    configuration_value: u8,
    /// IN data tests pushed, per endpoint address.
    data_in: BTreeMap<u8, VecDeque<Vec<u8>>>,
    /// What OUT transfers brought, per endpoint address.
    data_out: Vec<(u8, Vec<u8>)>,
    /// Endpoints that answer with STALL until CLEAR_FEATURE(ENDPOINT_HALT).
    halted: BTreeSet<u8>,
}
````

Replace:

````rust
            configuration_value: 0,
        }))
````

with:

````rust
            configuration_value: 0,
            data_in: BTreeMap::new(),
            data_out: Vec::new(),
            halted: BTreeSet::new(),
        }))
````

Replace:

````rust

    /// The address SET_ADDRESS gave it.
````

with:

````rust

    /// Queues `bytes` as the next IN transfer of `endpoint` (0x81, say).
    pub fn push_in(&mut self, endpoint: u8, bytes: &[u8]) {
        self.data_in
            .entry(endpoint)
            .or_default()
            .push_back(bytes.to_vec());
    }

    /// Makes `endpoint` answer with STALL until the host clears the halt.
    pub fn stall_endpoint(&mut self, endpoint: u8) {
        self.halted.insert(endpoint);
    }

    /// What OUT transfers brought: endpoint and data.
    pub fn data_out_received(&self) -> Vec<(u8, Vec<u8>)> {
        self.data_out.clone()
    }

    /// The address SET_ADDRESS gave it.
````

Replace:

````rust
            }
            _ if setup.is_in() => Ok(vec![0; setup.length as usize]),
            _ => Ok(Vec::new()),
        })
    }
````

with:

````rust
            }
            (RECIPIENT_ENDPOINT, CLEAR_FEATURE) if setup.value == ENDPOINT_HALT => {
                self.halted.remove(&(setup.index as u8));
                Ok(Vec::new())
            }
            _ if setup.is_in() => Ok(vec![0; setup.length as usize]),
            _ => Ok(Vec::new()),
        })
    }

    fn data_in(&mut self, endpoint: u8, max_len: usize) -> Option<Result<Vec<u8>, Stall>> {
        if self.halted.contains(&endpoint) {
            return Some(Err(Stall));
        }
        let mut data = self.data_in.get_mut(&endpoint)?.pop_front()?;
        data.truncate(max_len);
        Some(Ok(data))
    }

    fn data_out(&mut self, endpoint: u8, data: &[u8]) -> Option<Result<(), Stall>> {
        if self.halted.contains(&endpoint) {
            return Some(Err(Stall));
        }
        self.data_out.push((endpoint, data.to_vec()));
        Some(Ok(()))
    }
````

Replace:

````rust
        assert_eq!(d.address(), 3);
    }
}
````

with:

````rust
        assert_eq!(d.address(), 3);
    }

    #[test]
    fn in_data_is_handed_out_in_order_and_a_halt_lasts_until_cleared() {
        let dev = FakeUsbDevice::k120();
        let mut d = dev.borrow_mut();
        assert_eq!(d.data_in(0x81, 8), None, "a NAK");
        d.push_in(0x81, &[1, 2, 3]);
        d.push_in(0x81, &[4; 20]);
        assert_eq!(d.data_in(0x81, 8), Some(Ok(vec![1, 2, 3])));
        assert_eq!(d.data_in(0x81, 8), Some(Ok(vec![4; 8])));
        d.stall_endpoint(0x81);
        assert_eq!(d.data_in(0x81, 8), Some(Err(Stall)));
        d.control(Setup::clear_halt(0x81), &[]);
        assert_eq!(d.data_in(0x81, 8), None);
    }
}
````

- [ ] **Step 2: Extend the test support in `crates/usb/src/testing/xhci/commands.rs`**

In `crates/usb/src/testing/xhci/commands.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
pub const ADDRESS_DEVICE: u32 = 11;
pub const EVALUATE_CONTEXT: u32 = 13;
````

with:

````rust
pub const ADDRESS_DEVICE: u32 = 11;
pub const CONFIGURE_ENDPOINT: u32 = 12;
pub const EVALUATE_CONTEXT: u32 = 13;
````

Replace:

````rust
            ADDRESS_DEVICE => (self.address_device(&trb, dma), slot_of(&trb)),
            EVALUATE_CONTEXT => (self.evaluate_context(&trb, dma), slot_of(&trb)),
````

with:

````rust
            ADDRESS_DEVICE => (self.address_device(&trb, dma), slot_of(&trb)),
            CONFIGURE_ENDPOINT => (self.configure_endpoint(&trb, dma), slot_of(&trb)),
            EVALUATE_CONTEXT => (self.evaluate_context(&trb, dma), slot_of(&trb)),
````

- [ ] **Step 3: Extend the test support in `crates/usb/src/testing/xhci/mod.rs`**

In `crates/usb/src/testing/xhci/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    active: std::collections::BTreeSet<(usize, usize)>,
}
````

with:

````rust
    active: std::collections::BTreeSet<(usize, usize)>,
    /// Every control request relayed: slot, request, the slot's state.
    requests: Vec<(usize, crate::Setup, u32)>,
}
````

Replace:

````rust
            active: Default::default(),
        };
````

with:

````rust
            active: Default::default(),
            requests: Vec::new(),
        };
````

Replace:

````rust

    pub fn slot_enabled(&self, slot: usize) -> bool {
````

with:

````rust

    /// Every control request relayed to a device: slot, request, and the
    /// slot's state (3 is Configured) when it went out.
    pub fn requests(&self) -> &[(usize, crate::Setup, u32)] {
        &self.requests
    }

    pub fn slot_enabled(&self, slot: usize) -> bool {
````

- [ ] **Step 4: Extend the test support in `crates/usb/src/testing/xhci/slots.rs`**

In `crates/usb/src/testing/xhci/slots.rs`, replace:

````rust

    /// Disable Slot (xHCI 4.6.4): the slot and its endpoints are gone.
````

with:

````rust

    /// Configure Endpoint (xHCI 4.6.6): the added endpoints' contexts are
    /// checked and copied, the dropped ones disabled.
    pub(super) fn configure_endpoint(&mut self, trb: &[u32; 4], dma: &Dma) -> u32 {
        let slot = (trb[3] >> 24) as usize;
        match self.slot_state(slot) {
            None => return SLOT_NOT_ENABLED,
            Some(ADDRESSED | CONFIGURED) => {}
            Some(_) => return CONTEXT_STATE_ERROR,
        }
        if trb[3] & 1 << 9 != 0 {
            panic!("fake xhci: Configure Endpoint with Deconfigure is not modelled");
        }
        let stride = self.stride();
        let input = self.input_context(trb, dma);
        let (drop, add) = (dma.read32(input), dma.read32(input + 4));
        if drop & 0b11 != 0 || add & 0b11 != 0b01 {
            panic!(
                "fake xhci: Configure Endpoint with drop {drop:#x} add {add:#x}: A0 only, no EP0"
            );
        }
        let entries = dma.read32(input + stride) >> 27;
        let highest = 31 - add.leading_zeros();
        if entries < highest {
            panic!("fake xhci: context entries {entries} below DCI {highest}");
        }
        let mut added = Vec::new();
        for dci in 2..32usize {
            if add & 1 << dci == 0 {
                continue;
            }
            let at = input + (dci as u64 + 1) * stride;
            let ep = self.read_endpoint(at, dma, "Configure Endpoint");
            let types: &[u32] = if dci % 2 == 1 { &[5, 6, 7] } else { &[1, 2, 3] };
            if !types.contains(&ep.ep_type) || ep.max_packet == 0 || ep.interval > 15 {
                panic!("fake xhci: bad context for DCI {dci}: {ep:?}");
            }
            if ep.average_trb_length == 0 || (ep.ep_type % 4 == 3 && ep.max_esit_payload == 0) {
                panic!("fake xhci: DCI {dci} lacks its average TRB length or ESIT payload");
            }
            added.push((dci, at, ep));
        }
        let s = self.slots[slot].as_mut().expect("slot checked above");
        for dci in 2..32usize {
            if drop & 1 << dci != 0 {
                s.endpoints.remove(&dci);
            }
        }
        for (dci, at, ep) in added {
            let out = s.output + dci as u64 * stride;
            for i in 0..5 {
                dma.write32(out + 4 * i, dma.read32(at + 4 * i));
            }
            dma.write32(out, dma.read32(out) & !7 | RUNNING);
            s.endpoints.insert(dci, ep);
        }
        s.context_entries = entries;
        s.state = if s.endpoints.len() > 1 {
            CONFIGURED
        } else {
            ADDRESSED
        };
        let d0 = dma.read32(s.output);
        dma.write32(s.output, d0 & 0x07FF_FFFF | entries << 27);
        dma.write32(
            s.output + 12,
            dma.read32(s.output + 12) & 0xFF | s.state << 27,
        );
        SUCCESS
    }

    /// Disable Slot (xHCI 4.6.4): the slot and its endpoints are gone.
````

- [ ] **Step 5: Extend the test support in `crates/usb/src/testing/xhci/transfers.rs`**

In `crates/usb/src/testing/xhci/transfers.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

const SETUP_STAGE: u32 = 2;
````

with:

````rust

const NORMAL: u32 = 1;
const SETUP_STAGE: u32 = 2;
````

Replace:

````rust
                SETUP_STAGE if dci == 1 => self.control_td(slot, dma),
                t => panic!("fake xhci: TRB type {t} on the ring of slot {slot} DCI {dci}"),
````

with:

````rust
                SETUP_STAGE if dci == 1 => self.control_td(slot, dma),
                NORMAL if dci > 1 => self.normal_td(slot, dci, dma),
                t => panic!("fake xhci: TRB type {t} on the ring of slot {slot} DCI {dci}"),
````

Replace:

````rust

    pub(super) fn post_transfer(
        &mut self,
````

with:

````rust

    pub fn post_transfer(
        &mut self,
````

Replace:

````rust
        }
        let answer = dev.borrow_mut().control(setup, &out);
````

with:

````rust
        }
        let state = self.slots[slot].as_ref().map_or(0, |s| s.state);
        self.requests.push((slot, setup, state));
        let answer = dev.borrow_mut().control(setup, &out);
````

Replace:

````rust

    /// The device on `port` went away: every TD in progress of its slot
````

with:

````rust

    /// A Normal TRB (xHCI 4.10.1): one TD, played against the device's
    /// endpoint. A NAK leaves it waiting (tried again every tick).
    fn normal_td(&mut self, slot: usize, dci: usize, dma: &Dma) -> Td {
        let ep = self
            .ring(slot, dci)
            .expect("endpoint checked by the caller")
            .clone();
        let mut c = ep.ring;
        let Some((at, trb)) = c.peek(dma) else {
            return Td::Incomplete;
        };
        c.advance();
        let (buffer, len) = (pointer(&trb), (trb[2] & 0x1_FFFF) as usize);
        let address = (dci / 2) as u8 | if dci % 2 == 1 { 0x80 } else { 0 };
        let port = self.slots[slot].as_ref().map_or(0, |s| s.port);
        let Some(dev) = self.devices.get(port as usize - 1).cloned().flatten() else {
            self.fail(slot, dci, ep.ring, USB_TRANSACTION_ERROR, dma);
            return Td::Done;
        };
        let sent = if address & 0x80 != 0 {
            match dev.borrow_mut().data_in(address, len) {
                None => None,
                Some(Err(Stall)) => Some(Err(STALL)),
                Some(Ok(bytes)) if bytes.len() > len => Some(Err(BABBLE)),
                Some(Ok(bytes)) => {
                    dma.write_bytes(buffer, &bytes);
                    Some(Ok(bytes.len()))
                }
            }
        } else {
            let data = dma.read_bytes(buffer, len);
            dev.borrow_mut()
                .data_out(address, &data)
                .map(|r| r.map(|()| len).map_err(|Stall| STALL))
        };
        match sent {
            None => {
                if let Some(ep) = self.ring(slot, dci) {
                    ep.busy = true;
                }
                Td::Waiting
            }
            Some(Err(code)) => {
                self.fail(slot, dci, ep.ring, code, dma);
                Td::Done
            }
            Some(Ok(n)) => {
                let residual = (len - n) as u32;
                if residual > 0 && trb[3] & ISP != 0 {
                    self.post_transfer(slot, dci, at, SHORT_PACKET, residual, dma);
                } else if trb[3] & IOC != 0 {
                    self.post_transfer(slot, dci, at, SUCCESS, residual, dma);
                }
                if let Some(ep) = self.ring(slot, dci) {
                    ep.ring = c;
                    ep.busy = false;
                }
                Td::Done
            }
        }
    }

    /// The device on `port` went away: every TD in progress of its slot
````

- [ ] **Step 6: Write the failing tests for `crates/usb/src/xhci/configure.rs`**

Create `crates/usb/src/xhci/configure.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Dev = Rc<RefCell<FakeUsbDevice>>;

    fn plugged(config: FakeConfig, port: u8, dev: &Dev) -> (FakeHal, Xhci<FakeHal>) {
        let (hal, mut xhci) = start(config);
        hal.fake().plug(port, dev.clone());
        // A USB 3 link trains for 50 ms before the port shows a connection.
        hal.sleep(core::time::Duration::from_millis(60));
        xhci.port_changes();
        (hal, xhci)
    }

    fn configured(
        config: FakeConfig,
        port: u8,
        dev: &Dev,
        interfaces: &[u8],
    ) -> (FakeHal, Xhci<FakeHal>, Device) {
        let (hal, mut xhci) = plugged(config, port, dev);
        let device = xhci.attach(port).unwrap();
        xhci.configure(&device, interfaces).unwrap();
        (hal, xhci, device)
    }

    #[test]
    fn configuring_the_k120s_boot_interface_sets_up_its_interrupt_endpoint() {
        let k120 = FakeUsbDevice::k120();
        let (hal, _xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0]);
        let ep = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        assert_eq!(
            (ep.ep_type, ep.max_packet, ep.interval, ep.cerr),
            (7, 8, 6, 3)
        );
        assert_eq!((ep.average_trb_length, ep.max_esit_payload), (8, 8));
        assert_eq!(hal.fake().slot(d.slot as usize).unwrap().context_entries, 3);
        // SET_CONFIGURATION(1) reached the device once the slot was
        // configured.
        let set = hal
            .fake()
            .requests()
            .iter()
            .find(|(_, s, _)| s.request == 9)
            .copied();
        assert_eq!(set.map(|(_, s, state)| (s.value, state)), Some((1, 3)));
        assert_eq!(k120.borrow().configuration_value(), 1);
        assert!(
            hal.log_text()
                .contains("slot 1: endpoint 0x81 configured (DCI 3, interval exponent 6)")
        );
        // Interface 1 was not asked for.
        assert!(hal.fake().endpoint(d.slot as usize, 5).is_none());
    }

    #[test]
    fn the_sticks_bulk_endpoints_get_their_burst_and_packet_size() {
        let (hal, _xhci, d) = configured(
            FakeConfig::intel(),
            13,
            &FakeUsbDevice::kingston_stick(),
            &[0],
        );
        let bulk_in = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        let bulk_out = hal.fake().endpoint(d.slot as usize, 4).unwrap();
        assert_eq!(
            (bulk_in.ep_type, bulk_in.max_packet, bulk_in.max_burst),
            (6, 1024, 3)
        );
        assert_eq!(
            (bulk_out.ep_type, bulk_out.max_packet, bulk_out.max_burst),
            (2, 1024, 3)
        );
        assert_eq!((bulk_in.interval, bulk_in.average_trb_length), (0, 3072));
        let (hal, _xhci, d) =
            configured(FakeConfig::basic(), 3, &FakeUsbDevice::usb2_stick(), &[0]);
        let bulk_in = hal.fake().endpoint(d.slot as usize, 3).unwrap();
        assert_eq!((bulk_in.max_packet, bulk_in.max_burst), (512, 0));
    }

    #[test]
    fn isochronous_endpoints_are_skipped_and_empty_ones_refused() {
        let k120 = FakeUsbDevice::k120();
        let mut config = k120.borrow().configuration_descriptor().to_vec();
        config[30] = 1; // endpoint 0x81 becomes isochronous
        let device = k120.borrow().device_descriptor().to_vec();
        k120.borrow_mut().set_descriptors(device, config.clone());
        let (hal, _xhci, d) = configured(FakeConfig::basic(), 1, &k120, &[0, 1]);
        assert!(hal.fake().endpoint(d.slot as usize, 3).is_none());
        assert!(hal.fake().endpoint(d.slot as usize, 5).is_some());
        assert!(
            hal.log_text()
                .contains("endpoint 0x81 (isochronous) not configured")
        );
        let (hal, mut xhci) = plugged(FakeConfig::basic(), 2, &FakeUsbDevice::k120());
        let mut device = xhci.attach(2).unwrap();
        device.configuration.interfaces[0].endpoints[0].max_packet = 0;
        let before = hal.outstanding_dma();
        assert_eq!(
            xhci.configure(&device, &[1, 0]),
            Err(UsbError::BadDescriptor("endpoint packet size 0"))
        );
        assert_eq!(hal.outstanding_dma(), before);
    }

    #[test]
    fn a_failed_configure_endpoint_frees_the_new_rings() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(12);
        let (hal, mut xhci) = plugged(config, 1, &FakeUsbDevice::k120());
        let device = xhci.attach(1).unwrap();
        let before = hal.outstanding_dma();
        assert_eq!(xhci.configure(&device, &[0, 1]), Err(UsbError::Timeout));
        assert_eq!(hal.outstanding_dma(), before);
        assert!(
            hal.log_text()
                .contains("slot 1: Configure Endpoint: timed out")
        );
    }

    #[test]
    fn a_configure_endpoint_that_kills_the_controller_keeps_the_new_rings() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(12);
        config.abort_never_completes = true;
        let (hal, mut xhci) = plugged(config, 1, &FakeUsbDevice::k120());
        let device = xhci.attach(1).unwrap();
        let before = hal.outstanding_dma();
        assert_eq!(xhci.configure(&device, &[0, 1]), Err(UsbError::Timeout));
        assert_eq!(
            hal.outstanding_dma(),
            before + 4,
            "two rings and two buffers kept"
        );
    }

    #[test]
    fn running_out_of_memory_while_configuring_frees_what_it_got() {
        for n in 0..3 {
            let (hal, mut xhci) = plugged(FakeConfig::basic(), 1, &FakeUsbDevice::k120());
            let device = xhci.attach(1).unwrap();
            let before = hal.outstanding_dma();
            hal.fail_one_alloc(n);
            assert_eq!(
                xhci.configure(&device, &[0, 1]),
                Err(UsbError::NoMemory),
                "allocation {n}"
            );
            assert_eq!(hal.outstanding_dma(), before, "allocation {n}");
        }
    }
}
````

- [ ] **Step 7: Add the failing tests to `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, replace:

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
    fn detach_frees_everything_and_the_port_can_be_attached_again() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = plugged(FakeConfig::intel(), 3, &k120);
        let before = hal.outstanding_dma();
        let d = xhci.attach(3).unwrap();
        xhci.configure(&d, &[0, 1]).unwrap();
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert!(!hal.fake().slot_enabled(d.slot as usize));
        assert_eq!(xhci.dcbaa.read64(8 * d.slot as usize), 0);
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(hal.log_text().contains("port 3: slot 1 released"));
        xhci.detach(d.slot);
        let again = xhci.attach(3).unwrap();
        assert_eq!(again.slot, 1, "the lowest free slot");
        xhci.configure(&again, &[0]).unwrap();
    }
}
````

- [ ] **Step 8: Declare the new module in `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, replace:

````rust
mod command;
mod context;
````

with:

````rust
mod command;
mod configure;
mod context;
````

- [ ] **Step 9: Add the failing tests to `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, replace:

````rust
        assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
    }
}
````

with:

````rust
        assert_eq!(get_device(&mut xhci, d.slot, 18), Err(UsbError::Timeout));
    }

    fn keyboard(config: FakeConfig, port: u8) -> (FakeHal, Xhci<FakeHal>, Device, Dev) {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(config, port, &k120);
        xhci.configure(&d, &[0]).unwrap();
        (hal, xhci, d, k120)
    }

    /// `take_in` after letting the fake run for `ms` milliseconds.
    fn take_after(
        hal: &FakeHal,
        xhci: &mut Xhci<FakeHal>,
        slot: u8,
        ms: u64,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        hal.sleep(Duration::from_millis(ms));
        xhci.take_in(slot, 0x81, buf)
    }

    #[test]
    fn a_nakked_report_stays_pending_then_arrives_exactly_once() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        let mut buf = [0; 8];
        assert_eq!(xhci.take_in(d.slot, 0x81, &mut buf), None, "nothing queued");
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        for _ in 0..100 {
            assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        }
        k120.borrow_mut().push_in(0x81, &[0, 0, 4, 5, 6, 0, 0, 0]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
        assert_eq!(buf, [0, 0, 4, 5, 6, 0, 0, 0]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            None,
            "returned once"
        );
    }

    #[test]
    fn a_short_report_gives_its_length() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::intel(), 2);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[1, 2, 3]);
        let mut buf = [0; 8];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(3))
        );
        assert_eq!(&buf[..3], [1, 2, 3]);
        // A buffer smaller than what came gets what fits.
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[9; 8]);
        let mut small = [0; 2];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut small),
            Some(Ok(2))
        );
    }

    #[test]
    fn a_stalled_endpoint_is_recovered_by_clear_halt() {
        for (config, port) in [(FakeConfig::basic(), 1), (FakeConfig::intel(), 5)] {
            let (hal, mut xhci, d, k120) = keyboard(config, port);
            k120.borrow_mut().stall_endpoint(0x81);
            xhci.queue_in(d.slot, 0x81, 8).unwrap();
            let mut buf = [0; 8];
            assert_eq!(
                take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
                Some(Err(UsbError::Stall))
            );
            assert!(
                hal.log_text()
                    .contains("slot 1 endpoint 0x81: transfer failed: stall")
            );
            assert_eq!(
                hal.fake().endpoint(d.slot as usize, 3).unwrap().state,
                2,
                "halted"
            );
            let n = hal.fake().executed().len();
            xhci.clear_halt(d.slot, 0x81).unwrap();
            // Reset Endpoint, Set TR Dequeue Pointer, then the request.
            assert_eq!(commands_since(&hal, n), [14, 16]);
            let last = k120.borrow().requests().pop().unwrap();
            assert_eq!(last.setup, Setup::clear_halt(0x81));
            xhci.queue_in(d.slot, 0x81, 8).unwrap();
            k120.borrow_mut().push_in(0x81, &[7; 8]);
            assert_eq!(
                take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
                Some(Ok(8))
            );
        }
    }

    #[test]
    fn clear_halt_on_a_running_endpoint_drops_what_was_queued() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.sleep(Duration::from_millis(1));
        let n = hal.fake().executed().len();
        xhci.clear_halt(d.slot, 0x81).unwrap();
        // Not halted: Stop Endpoint instead of Reset Endpoint.
        assert_eq!(commands_since(&hal, n), [15, 16]);
        let mut buf = [0; 8];
        assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        k120.borrow_mut().push_in(0x81, &[5; 8]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
    }

    #[test]
    fn unplugging_fails_the_queued_transfer_and_detach_frees_everything() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().plug(3, k120.clone());
        xhci.port_changes();
        let before = hal.outstanding_dma();
        let d = xhci.attach(3).unwrap();
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.fake().unplug(3);
        let mut buf = [0; 8];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Err(UsbError::Transfer(4)))
        );
        let changes = xhci.port_changes();
        assert!(changes[0].reconnected && !changes[0].connected);
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert_eq!(
            xhci.take_in(d.slot, 0x81, &mut buf),
            Some(Err(UsbError::Disconnected))
        );
    }

    #[test]
    fn a_bulk_in_transfer_gets_what_the_device_sent() {
        let stick = FakeUsbDevice::kingston_stick();
        let (hal, mut xhci, d) = attached(FakeConfig::intel(), 13, &stick);
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 512).unwrap();
        // A mass storage status wrapper: 13 bytes.
        stick.borrow_mut().push_in(0x81, &[0x55; 13]);
        let mut buf = [0; 512];
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(13))
        );
        assert_eq!(buf[..13], [0x55; 13]);
    }

    #[test]
    fn an_event_for_another_trb_does_not_finish_a_transfer() {
        let (hal, mut xhci, d, k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.act(|x, dma| x.post_transfer(d.slot as usize, 3, 0x1000, 1, 0, dma));
        let mut buf = [0; 8];
        assert_eq!(take_after(&hal, &mut xhci, d.slot, 1, &mut buf), None);
        k120.borrow_mut().push_in(0x81, &[3; 8]);
        assert_eq!(
            take_after(&hal, &mut xhci, d.slot, 1, &mut buf),
            Some(Ok(8))
        );
    }

    #[test]
    fn events_for_a_detached_slot_are_ignored() {
        let (hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        xhci.detach(d.slot);
        hal.act(|x, dma| {
            x.post_transfer(1, 3, 0x1000, 1, 0, dma);
            x.post_transfer(1, 1, 0x2000, 6, 0, dma);
        });
        xhci.poll();
        assert_eq!(
            xhci.control(d.slot, Setup::set_configuration(1), &mut []),
            Err(UsbError::Disconnected)
        );
    }

    #[test]
    fn in_transfers_the_driver_cannot_do_are_refused() {
        let (_hal, mut xhci, d, _k120) = keyboard(FakeConfig::basic(), 1);
        assert_eq!(
            xhci.queue_in(d.slot, 0x82, 4),
            Err(UsbError::Disconnected),
            "interface 1"
        );
        assert_eq!(xhci.queue_in(9, 0x81, 8), Err(UsbError::Disconnected));
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 5000),
            Err(UsbError::Unsupported("transfer over 4096 bytes"))
        );
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        assert_eq!(
            xhci.queue_in(d.slot, 0x81, 8),
            Err(UsbError::Unsupported("transfer already queued"))
        );
        let mut buf = [0; 8];
        assert_eq!(
            xhci.take_in(d.slot, 0x82, &mut buf),
            Some(Err(UsbError::Disconnected))
        );
        assert_eq!(xhci.clear_halt(d.slot, 0x83), Err(UsbError::Disconnected));
    }

    #[test]
    fn the_bus_forwards_time_and_log_lines() {
        let (hal, xhci, _d, _k120) = keyboard(FakeConfig::basic(), 1);
        let before = hal.clock();
        assert!(Bus::now(&xhci) > before);
        Bus::log(&xhci, format_args!("hid: hello"));
        assert!(hal.log_text().ends_with("hid: hello"));
    }

    #[test]
    fn a_disable_slot_that_times_out_keeps_the_memory_the_controller_uses() {
        let k120 = FakeUsbDevice::k120();
        let (hal, mut xhci, d) = attached(FakeConfig::intel(), 3, &k120);
        xhci.configure(&d, &[0]).unwrap();
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.sleep(Duration::from_millis(20));
        let before = hal.outstanding_dma();
        hal.fake().config_mut().hang_command = Some(10); // Disable Slot
        xhci.detach(d.slot);
        assert!(
            hal.fake().slot_enabled(d.slot as usize),
            "the controller still has the slot"
        );
        assert_eq!(hal.outstanding_dma(), before, "nothing freed");
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(
            hal.log_text()
                .contains("slot 1: Disable Slot failed (timed out); its memory is kept")
        );
        // The keyboard sends a report: the controller writes it into the
        // endpoint buffer, which is still allocated (the fake panics on
        // freed memory).
        k120.borrow_mut().push_in(0x81, &[0, 0, 4, 0, 0, 0, 0, 0]);
        hal.sleep(Duration::from_millis(20));
    }

    #[test]
    fn detaching_on_a_dead_controller_frees_nothing() {
        let (hal, mut xhci, d, _k120) = keyboard(FakeConfig::intel(), 3);
        xhci.queue_in(d.slot, 0x81, 8).unwrap();
        hal.fake().host_system_error();
        xhci.poll();
        let before = hal.outstanding_dma();
        let commands = hal.fake().executed().len();
        xhci.detach(d.slot);
        assert_eq!(hal.outstanding_dma(), before);
        assert_eq!(hal.fake().executed().len(), commands, "no command sent");
        assert_eq!(xhci.slot_of_port(3), None);
        assert!(hal.log_text().contains(
            "slot 1: Disable Slot failed (controller stopped working); its memory is kept"
        ));
    }
}
````

- [ ] **Step 10: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Xhci` in this scope ``; `` cannot find type `Device` in this scope ``.

- [ ] **Step 11: Implement `crates/usb/src/xhci/configure.rs`**

Insert this at the top of `crates/usb/src/xhci/configure.rs`, above `#[cfg(test)]`:

````rust
//! Configuring a device (spec §6.2 step 7): endpoint contexts for the
//! interfaces a class driver claimed, Configure Endpoint, then
//! SET_CONFIGURATION.

use super::context::{
    EndpointContext, Input, SlotContext, dci, endpoint_type, interval_exponent, speed_id,
};
use super::device::{A0, kind_name};
use super::ring::ProducerRing;
use super::transfer::DATA_BUFFER_SIZE;
use super::trb::Trb;
use super::{Device, Endpoint, Transfer, Xhci};
use crate::descriptor::{self, EndpointKind};
use crate::{Hal, Setup, Speed, UsbError};
use alloc::vec::Vec;

/// The endpoint context for `e` of a device at `speed` (xHCI 6.2.3,
/// 4.14.1.1): bursts from the SuperSpeed companion or, for high-speed
/// periodic endpoints, the extra transactions; interrupt endpoints move
/// one packet per interval, bulk ones large TRBs.
fn endpoint_context(e: &descriptor::Endpoint, speed: Speed, dequeue: u64) -> EndpointContext {
    let interrupt = e.kind == EndpointKind::Interrupt;
    let max_burst = if speed.is_superspeed() {
        e.max_burst
    } else if speed == Speed::High && interrupt {
        e.extra_transactions()
    } else {
        0
    };
    let packet = e.packet_size();
    EndpointContext {
        ep_type: endpoint_type(e.kind, e.is_in()),
        cerr: 3,
        max_packet: packet,
        max_burst,
        interval: interval_exponent(speed, e.kind, e.interval),
        dequeue,
        average_trb_length: if interrupt { packet } else { 3072 },
        max_esit_payload: if interrupt {
            packet as u32 * (max_burst as u32 + 1)
        } else {
            0
        },
        ..EndpointContext::default()
    }
}

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
    Ok(Endpoint {
        address: e.address,
        dci: dci(e.address),
        ring,
        buffer,
        transfer: Transfer::Idle,
    })
}

impl<H: Hal> Xhci<H> {
    /// Configures the endpoints of the given interfaces of `device` (spec
    /// §6.2 step 7: Configure Endpoint, then SET_CONFIGURATION). Endpoints of
    /// other interfaces are left unconfigured.
    pub fn configure(&mut self, device: &Device, interfaces: &[u8]) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let slot = device.slot;
        let stride = self.info.context_size;
        let s = match self.slots.get(slot as usize).and_then(Option::as_ref) {
            Some(s) if s.port == device.port => s,
            _ => return Err(UsbError::Disconnected),
        };
        if !s.endpoints.is_empty() {
            return Err(UsbError::Unsupported("already configured"));
        }
        let input = Input::new(&s.input, stride);
        input.clear();
        let mut endpoints: Vec<Endpoint> = Vec::new();
        let mut add = A0;
        let claimed = device
            .configuration
            .interfaces
            .iter()
            .filter(|i| interfaces.contains(&i.number));
        for e in claimed.flat_map(|i| &i.endpoints) {
            let dci = dci(e.address);
            if !matches!(e.kind, EndpointKind::Interrupt | EndpointKind::Bulk)
                || endpoints.iter().any(|ep| ep.dci == dci)
            {
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot}: endpoint {:#04x} ({}) not configured",
                    e.address,
                    kind_name(e.kind)
                );
                continue;
            }
            let r = if e.packet_size() == 0 {
                Err(UsbError::BadDescriptor("endpoint packet size 0"))
            } else {
                new_endpoint(&self.hal, e)
            };
            let ep = match r {
                Ok(ep) => ep,
                Err(err) => {
                    endpoints.into_iter().for_each(|ep| ep.free(&self.hal));
                    xlog!(
                        &self.hal,
                        &self.name,
                        "slot {slot}: endpoint {:#04x}: {err}",
                        e.address
                    );
                    return Err(err);
                }
            };
            input.set_endpoint(
                dci,
                &endpoint_context(e, s.speed, ep.ring.enqueue_pointer()),
            );
            add |= 1 << dci;
            endpoints.push(ep);
        }
        input.set_flags(0, add);
        input.set_slot(&SlotContext {
            speed: speed_id(s.speed),
            // The highest DCI in use (xHCI 6.2.2).
            context_entries: 31 - add.leading_zeros() as u8,
            root_port: s.port,
            ..SlotContext::default()
        });
        let trb = Trb::configure_endpoint(s.input.phys(), slot, false);
        if let Err(e) = self.command(trb) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: Configure Endpoint: {e}"
            );
            // A dead controller may still have the new rings.
            if !self.dead {
                endpoints.into_iter().for_each(|ep| ep.free(&self.hal));
            }
            return Err(e);
        }
        for ep in &endpoints {
            let e = device
                .configuration
                .interfaces
                .iter()
                .flat_map(|i| &i.endpoints);
            let interval = e
                .clone()
                .find(|e| e.address == ep.address)
                .map_or(0, |e| interval_exponent(device.speed, e.kind, e.interval));
            xlog!(
                &self.hal,
                &self.name,
                "slot {slot}: endpoint {:#04x} configured (DCI {}, interval exponent {interval})",
                ep.address,
                ep.dci
            );
        }
        if let Some(s) = self.slots[slot as usize].as_mut() {
            s.endpoints = endpoints;
        }
        let value = device.configuration.value;
        self.control_transfer(slot, Setup::set_configuration(value), &mut [])
            .inspect_err(|e| {
                xlog!(
                    &self.hal,
                    &self.name,
                    "slot {slot}: SET_CONFIGURATION({value}): {e}"
                )
            })?;
        Ok(())
    }
}

````

- [ ] **Step 12: Change `crates/usb/src/xhci/device.rs`**

In `crates/usb/src/xhci/device.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! Devices (spec §6.2 steps 1-5): enumeration on a root port. Each step
//! logs why it failed; a failed attach disables its slot and frees
//! everything it allocated, so a misbehaving device costs nothing else.

````

with:

````rust
//! Devices (spec §6.2 steps 1-5): enumeration on a root port, and
//! detaching. Each step logs why it failed; a failed attach disables its
//! slot and frees everything it allocated, so a misbehaving device costs
//! nothing else.

````

Replace:

````rust
/// Input control context flags: A0 is the slot context, A1 EP0.
const A0: u32 = 1 << 0;
const A1: u32 = 1 << 1;
````

with:

````rust
/// Input control context flags: A0 is the slot context, A1 EP0.
pub(super) const A0: u32 = 1 << 0;
const A1: u32 = 1 << 1;
````

Replace:

````rust

fn kind_name(kind: EndpointKind) -> &'static str {
    match kind {
````

with:

````rust

pub(super) fn kind_name(kind: EndpointKind) -> &'static str {
    match kind {
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

    /// Forgets the device in `slot`: Disable Slot, free its rings, contexts
    /// and buffers. Later events for it are ignored. If Disable Slot fails,
    /// or the controller is dead, the memory is kept, as the controller may
    /// still use it.
    pub fn detach(&mut self, slot: u8) {
        let Some(port) = self
            .slots
            .get(slot as usize)
            .and_then(Option::as_ref)
            .map(|s| s.port)
        else {
            return;
        };
        self.release_slot(slot);
        xlog!(&self.hal, &self.name, "port {port}: slot {slot} released");
    }

````

- [ ] **Step 13: Change `crates/usb/src/xhci/mod.rs`**

In `crates/usb/src/xhci/mod.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! The xHCI host controller driver (spec §6.2).
// The driver is built bottom-up: parts land before their users.
#![allow(dead_code)]

/// Logs one line starting "xhci <name>: ", as every line of this driver
````

with:

````rust
//! The xHCI host controller driver (spec §6.2).
/// Logs one line starting "xhci <name>: ", as every line of this driver
````

Replace:

````rust

/// A device slot: its contexts, EP0 and the buffer control transfers use.
````

with:

````rust

/// Where an endpoint's one transfer is.
#[derive(Debug)]
enum Transfer {
    Idle,
    /// A Normal TRB at `trb` for `len` bytes is on the ring.
    Queued {
        trb: u64,
        len: usize,
    },
    /// Finished; `take_in` returns it once.
    Done(Result<usize, UsbError>),
}

/// A configured endpoint other than EP0.
#[derive(Debug)]
struct Endpoint {
    address: u8,
    dci: usize,
    ring: ProducerRing,
    /// IN endpoints: the 4 KiB buffer their transfers land in.
    buffer: Option<DmaBuf>,
    transfer: Transfer,
}

impl Endpoint {
    fn free<H: Hal>(self, hal: &H) {
        self.ring.free(hal);
        if let Some(buf) = self.buffer {
            hal.free_dma(buf);
        }
    }
}

/// A device slot: its contexts, EP0 and the buffer control transfers use.
````

Replace:

````rust
    control: Option<Control>,
}
````

with:

````rust
    control: Option<Control>,
    /// What `configure` set up.
    endpoints: Vec<Endpoint>,
}
````

Replace:

````rust
                control: None,
            }),
````

with:

````rust
                control: None,
                endpoints: Vec::new(),
            }),
````

Replace:

````rust
        self.ep0.free(hal);
    }
````

with:

````rust
        self.ep0.free(hal);
        for ep in self.endpoints {
            ep.free(hal);
        }
    }

    fn endpoint(&mut self, dci: usize) -> Option<&mut Endpoint> {
        self.endpoints.iter_mut().find(|e| e.dci == dci)
    }
````

Replace:

````rust
    dcbaa: DmaBuf,
    scratchpads: Option<Scratchpads>,
````

with:

````rust
    dcbaa: DmaBuf,
    // The controller uses these pages; the driver only owns them.
    #[cfg_attr(not(test), expect(dead_code))]
    scratchpads: Option<Scratchpads>,
````

- [ ] **Step 14: Change `crates/usb/src/xhci/regs.rs`**

In `crates/usb/src/xhci/regs.rs`, replace:

````rust
pub const CA: u32 = 1 << 2;
pub const CRR: u32 = 1 << 3;

````

with:

````rust
pub const CA: u32 = 1 << 2;

````

- [ ] **Step 15: Change `crates/usb/src/xhci/transfer.rs`**

In `crates/usb/src/xhci/transfer.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output};
use super::trb::{SHORT_PACKET, STALL, SUCCESS, Trb, completion_name};
use super::{Control, Xhci};
use crate::{Hal, Setup, UsbError};
use core::sync::atomic::{Ordering, fence};
````

with:

````rust

use super::context::{EP_DISABLED, EP_ERROR, EP_HALTED, EP_RUNNING, EP_STOPPED, Output, dci};
use super::trb::{SHORT_PACKET, STALL, SUCCESS, Trb, completion_name};
use super::{Control, Transfer, Xhci};
use crate::{Bus, Hal, Setup, UsbError};
use core::fmt;
use core::sync::atomic::{Ordering, fence};
````

Replace:

````rust
        };
        if event.endpoint_id() == EP0
            && let Some(control) = s.control.as_mut()
        {
            control.on_event(&event);
        }
````

with:

````rust
        };
        let dci = event.endpoint_id();
        if dci == EP0 {
            if let Some(control) = s.control.as_mut() {
                control.on_event(&event);
            }
            return;
        }
        let Some(ep) = s.endpoint(dci) else {
            return;
        };
        let Transfer::Queued { trb, len } = ep.transfer else {
            return;
        };
        if trb != event.pointer() {
            return;
        }
        let code = event.completion_code();
        let residual = event.transfer_length() as usize;
        ep.transfer = Transfer::Done(match code {
            SUCCESS | SHORT_PACKET => Ok(len.saturating_sub(residual)),
            STALL => Err(UsbError::Stall),
            code => Err(UsbError::Transfer(code)),
        });
        if !matches!(code, SUCCESS | SHORT_PACKET) {
            xlog!(
                &self.hal,
                &self.name,
                "slot {} endpoint {:#04x}: transfer failed: {}",
                event.slot_id(),
                ep.address,
                completion_name(code)
            );
        }
````

Replace:

````rust
    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        let s = self.slots[slot].as_ref().ok_or(UsbError::Disconnected)?;
        let dequeue = if dci == EP0 {
            s.ep0.enqueue_pointer()
        } else {
            return Err(UsbError::Disconnected);
        };
        self.command(Trb::set_tr_dequeue(slot as u8, dci, dequeue))?;
        Ok(())
    }
````

with:

````rust
    fn set_dequeue(&mut self, slot: usize, dci: usize) -> Result<(), UsbError> {
        let s = self.slots[slot].as_mut().ok_or(UsbError::Disconnected)?;
        let dequeue = if dci == EP0 {
            s.ep0.enqueue_pointer()
        } else {
            s.endpoint(dci)
                .ok_or(UsbError::Disconnected)?
                .ring
                .enqueue_pointer()
        };
        self.command(Trb::set_tr_dequeue(slot as u8, dci, dequeue))?;
        Ok(())
    }

    /// The configured endpoint `address` of `slot`.
    fn endpoint_mut(&mut self, slot: u8, address: u8) -> Option<&mut super::Endpoint> {
        let s = self.slots.get_mut(slot as usize)?.as_mut()?;
        s.endpoints.iter_mut().find(|e| e.address == address)
    }
````

Replace:

````rust
            .state)
    }
````

with:

````rust
            .state)
    }
}

impl<H: Hal> Bus for Xhci<H> {
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
        self.control_transfer(slot, setup, data)
    }

    /// One Normal TRB (IOC + ISP) into the endpoint's own buffer.
    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
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
            return Err(UsbError::Unsupported("transfer already queued"));
        }
        let trb = ep.ring.push(Trb::normal(buffer.phys(), len as u32));
        ep.transfer = Transfer::Queued { trb, len };
        let dci = ep.dci as u32;
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, slot, dci);
        Ok(())
    }

    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        self.poll();
        let Some(ep) = self.endpoint_mut(slot, endpoint) else {
            return Some(Err(UsbError::Disconnected));
        };
        if !matches!(ep.transfer, Transfer::Done(_)) {
            return None;
        }
        let Transfer::Done(result) = core::mem::replace(&mut ep.transfer, Transfer::Idle) else {
            return None;
        };
        Some(result.map(|n| {
            let n = n.min(buf.len());
            if let Some(buffer) = &ep.buffer {
                buffer.read_bytes(0, &mut buf[..n]);
            }
            n
        }))
    }

    /// Reset Endpoint if the context says Halted (Stop Endpoint if it still
    /// runs), Set TR Dequeue Pointer to the enqueue position, dropping
    /// anything outstanding, then CLEAR_FEATURE(ENDPOINT_HALT).
    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let dci = dci(endpoint);
        let ep = self
            .endpoint_mut(slot, endpoint)
            .ok_or(UsbError::Disconnected)?;
        ep.transfer = Transfer::Idle;
        self.reposition(slot as usize, dci)?;
        self.control_transfer(slot, Setup::clear_halt(endpoint), &mut [])?;
        Ok(())
    }

    fn now(&self) -> Duration {
        self.hal.now()
    }

    fn log(&self, args: fmt::Arguments) {
        self.hal.log(args);
    }
````

- [ ] **Step 16: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 181 tests.

- [ ] **Step 17: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 18: Commit**

````bash
git add crates
git commit -m "usb: xHCI endpoint configuration, IN transfers, halt recovery and detach"
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 10 scenario(s) passed`.

````bash
git push -u origin plan4/xhci-devices
gh pr create --base main --head plan4/xhci-devices --title "Plan 4: xHCI devices" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 10–12: port reset for USB 2 and USB 3 (with one warm reset), neutral PORTSC writes, port changes; enumeration (debounce, Enable Slot, Address Device, EP0 packet size fixed with Evaluate Context, descriptors) and control transfers with STALL and timeout recovery; Configure Endpoint and `SET_CONFIGURATION`, IN transfers, halt recovery and detach; the `Bus` implementation.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/xhci-devices --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-xhci-devices
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 6: The HID keyboard and the host (Tasks 13–15)

The US keymap, the boot-keyboard driver with Caps Lock and auto-repeat, and `Host`, which sets up devices as they come and go and hands keyboards to the driver.

Branch `plan4/hid-keyboard`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-hid-keyboard`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/hid-keyboard /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-hid-keyboard origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-hid-keyboard
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/xhci-devices` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/hid-keyboard /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-hid-keyboard plan4/xhci-devices`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/xhci-devices>` and re-run `cargo xtask ci` before pushing.

### Task 13: The US keymap

Spec §6.3's keymap: HID usage codes (HID Usage Tables, page 7) to keys on the US layout, which the host's `/etc/default/keyboard` also uses. Letters follow Shift and Caps Lock (either makes a capital, both cancel); the digit row and punctuation follow Shift only; Enter, Escape, Backspace, Tab, the arrows, Home/End, Insert/Delete, Page Up/Down, F1–F12 and the lock keys are keys of their own. Keypad keys always give digits and operators (decision 7: Num Lock is not tracked). `Modifiers` merges the left and right modifier keys of a boot report; Alt is recognised but has no function yet.

**Files:**
- Create: `crates/usb/src/hid/keymap.rs`
- Create: `crates/usb/src/hid/mod.rs`
- Modify: `crates/usb/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `usb::hid::{Key { Char(u8), Enter, Escape, Backspace, Tab, CapsLock, F(u8), PrintScreen, ScrollLock, Pause, Insert, Home, PageUp, Delete, End, PageDown, Right, Left, Down, Up, NumLock, Menu }, Modifiers { shift, ctrl, alt, caps_lock } (from_report(byte, caps_lock))}`; `usb::hid::keymap::key(usage: u8, Modifiers) -> Option<Key>`.

- [ ] **Step 1: Write the failing tests for `crates/usb/src/hid/keymap.rs`**

Create `crates/usb/src/hid/keymap.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        caps_lock: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..PLAIN
    };
    const CAPS: Modifiers = Modifiers {
        caps_lock: true,
        ..PLAIN
    };
    const SHIFT_CAPS: Modifiers = Modifiers {
        shift: true,
        caps_lock: true,
        ..PLAIN
    };

    fn text(usages: &[u8], m: Modifiers) -> String {
        usages
            .iter()
            .map(|&u| match key(u, m) {
                Some(Key::Char(c)) => c as char,
                other => panic!("usage {u:#x}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn letters_follow_shift_and_caps_lock() {
        assert_eq!(text(&[0x04, 0x05, 0x1D], PLAIN), "abz");
        assert_eq!(text(&[0x04, 0x05, 0x1D], SHIFT), "ABZ");
        assert_eq!(text(&[0x04, 0x05, 0x1D], CAPS), "ABZ");
        assert_eq!(text(&[0x04, 0x05, 0x1D], SHIFT_CAPS), "abz");
    }

    #[test]
    fn the_digit_row_and_punctuation_follow_shift_only() {
        let row: Vec<u8> = (0x1E..=0x27).chain(0x2C..=0x38).collect();
        assert_eq!(text(&row, PLAIN), "1234567890 -=[]\\\\;'`,./");
        assert_eq!(text(&row, SHIFT), "!@#$%^&*() _+{}||:\"~<>?");
        assert_eq!(text(&row, CAPS), "1234567890 -=[]\\\\;'`,./");
    }

    #[test]
    fn control_keys_and_the_navigation_block() {
        assert_eq!(key(0x28, PLAIN), Some(Key::Enter));
        assert_eq!(key(0x29, PLAIN), Some(Key::Escape));
        assert_eq!(key(0x2A, SHIFT), Some(Key::Backspace));
        assert_eq!(key(0x2B, PLAIN), Some(Key::Tab));
        assert_eq!(key(0x39, PLAIN), Some(Key::CapsLock));
        assert_eq!(key(0x3A, PLAIN), Some(Key::F(1)));
        assert_eq!(key(0x45, PLAIN), Some(Key::F(12)));
        assert_eq!(key(0x4A, PLAIN), Some(Key::Home));
        assert_eq!(key(0x4C, PLAIN), Some(Key::Delete));
        assert_eq!(key(0x4D, PLAIN), Some(Key::End));
        assert_eq!(
            [0x4F, 0x50, 0x51, 0x52].map(|u| key(u, PLAIN).unwrap()),
            [Key::Right, Key::Left, Key::Down, Key::Up]
        );
    }

    #[test]
    fn the_keypad_gives_digits_and_operators() {
        let pad: Vec<u8> = (0x54..=0x57).chain(0x59..=0x63).collect();
        assert_eq!(text(&pad, PLAIN), "/*-+1234567890.");
        assert_eq!(text(&pad, SHIFT), "/*-+1234567890.");
        assert_eq!(key(0x58, PLAIN), Some(Key::Enter));
    }

    #[test]
    fn usages_the_layout_lacks_are_none() {
        for u in [0x00, 0x01, 0x02, 0x03, 0x66, 0x68, 0xE0, 0xFF] {
            assert_eq!(key(u, PLAIN), None, "usage {u:#x}");
        }
    }

    #[test]
    fn modifier_bytes_merge_left_and_right() {
        assert_eq!(Modifiers::from_report(0x00, false), PLAIN);
        assert_eq!(Modifiers::from_report(0x02, false), SHIFT);
        assert_eq!(Modifiers::from_report(0x20, true), SHIFT_CAPS);
        let m = Modifiers::from_report(0x11 | 0x40, false);
        assert!(m.ctrl && m.alt && !m.shift);
    }
}
````

- [ ] **Step 2: Create `crates/usb/src/hid/mod.rs`**

Create `crates/usb/src/hid/mod.rs`:

````rust
//! The HID boot keyboard (spec §6.3): the driver and the US layout.

pub mod keymap;

pub use keymap::{Key, Modifiers};
````

- [ ] **Step 3: Declare the new module in `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust
mod hal;
#[cfg(test)]
````

with:

````rust
mod hal;
pub mod hid;
#[cfg(test)]
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Modifiers` in this scope ``; `` cannot find struct, variant or union type `Modifiers` in this scope ``.

- [ ] **Step 5: Implement `crates/usb/src/hid/keymap.rs`**

Insert this at the top of `crates/usb/src/hid/keymap.rs`, above `#[cfg(test)]`:

````rust
//! The US keyboard layout (spec §6.3): HID usage codes (HID Usage Tables,
//! page 7) to keys, with Shift and Caps Lock applied. Keypad keys always
//! give digits and operators (Num Lock is not tracked).

/// A key as the console sees it. Characters come with Shift and Caps Lock
/// already applied; Ctrl and Alt are left to the console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable ASCII character, or the space.
    Char(u8),
    Enter,
    Escape,
    Backspace,
    Tab,
    CapsLock,
    /// F1 to F12.
    F(u8),
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    Right,
    Left,
    Down,
    Up,
    NumLock,
    Menu,
}

/// The modifier state a key is read with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    /// Recognised, but has no function yet (spec §6.3).
    pub alt: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    /// From a boot report's modifier byte (left Ctrl, Shift, Alt, GUI in
    /// bits 0-3, the right ones in bits 4-7) and the Caps Lock state.
    pub fn from_report(byte: u8, caps_lock: bool) -> Modifiers {
        Modifiers {
            ctrl: byte & 0x11 != 0,
            shift: byte & 0x22 != 0,
            alt: byte & 0x44 != 0,
            caps_lock,
        }
    }
}

/// Unshifted and shifted characters of usages 0x1E-0x38: the digit row and
/// the punctuation keys.
const ROW: &[u8; 27] = b"1234567890\n\x1b\x08\t -=[]\\\\;'`,./";
const SHIFTED: &[u8; 27] = b"!@#$%^&*()\n\x1b\x08\t _+{}||:\"~<>?";

/// The key for `usage`, or `None` for usages this layout does not have.
pub fn key(usage: u8, m: Modifiers) -> Option<Key> {
    Some(match usage {
        0x04..=0x1D => {
            let c = b'a' + (usage - 0x04);
            Key::Char(if m.shift != m.caps_lock {
                c.to_ascii_uppercase()
            } else {
                c
            })
        }
        0x28 | 0x58 => Key::Enter,
        0x29 => Key::Escape,
        0x2A => Key::Backspace,
        0x2B => Key::Tab,
        // 0x32 is the non-US `#` key, which Linux's US map treats as `\`.
        0x1E..=0x27 | 0x2C..=0x38 => {
            let i = (usage - 0x1E) as usize;
            Key::Char(if m.shift { SHIFTED[i] } else { ROW[i] })
        }
        0x39 => Key::CapsLock,
        0x3A..=0x45 => Key::F(usage - 0x3A + 1),
        0x46 => Key::PrintScreen,
        0x47 => Key::ScrollLock,
        0x48 => Key::Pause,
        0x49 => Key::Insert,
        0x4A => Key::Home,
        0x4B => Key::PageUp,
        0x4C => Key::Delete,
        0x4D => Key::End,
        0x4E => Key::PageDown,
        0x4F => Key::Right,
        0x50 => Key::Left,
        0x51 => Key::Down,
        0x52 => Key::Up,
        0x53 => Key::NumLock,
        0x54 => Key::Char(b'/'),
        0x55 => Key::Char(b'*'),
        0x56 => Key::Char(b'-'),
        0x57 => Key::Char(b'+'),
        0x59..=0x61 => Key::Char(b'1' + (usage - 0x59)),
        0x62 => Key::Char(b'0'),
        0x63 => Key::Char(b'.'),
        // The ISO key between left Shift and Z, as Linux's US console map.
        0x64 => Key::Char(if m.shift { b'>' } else { b'<' }),
        0x65 => Key::Menu,
        0x67 => Key::Char(b'='),
        _ => return None,
    })
}

````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 187 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -m "usb: the US keymap"
````


### Task 14: The HID boot keyboard

Spec §6.3's driver, over the `Bus` of Task 3, so it is tested against a small fake bus. It claims interfaces 3/1/1 with an interrupt-IN endpoint (the K120 and the Unifying receiver's keyboard), sends `SET_PROTOCOL(boot)` (which must succeed) and `SET_IDLE(0)` (a stall is only logged), and keeps one IN transfer queued. Each report is compared with the last to produce presses and releases; a report with ErrorRollOver (too many keys), POSTFail or ErrorUndefined changes nothing, and a usage listed twice is one key. Caps Lock toggles on its press and is shown on the LED with `SET_REPORT(Output)`, sent once per change. A held key repeats after 500 ms, 30 times a second, driven by the clock; the newest key repeats, lock keys do not, and after a long pause (a busy command) it repeats once instead of in a burst. `poll` never waits; the LED and recovery wait for `service` (decision 2). When the endpoint fails every held key is released (nothing may keep repeating), and recovery (`clear_halt`, then a new transfer) is tried once a second, three times; a disconnected keyboard stops at once.

**Files:**
- Create: `crates/usb/src/hid/keyboard.rs`
- Modify: `crates/usb/src/hid/mod.rs`

**Interfaces:**
- Consumes: `Bus`, `Setup`, `descriptor::Interface` (Task 3), the keymap (Task 13).
- Produces: `usb::hid::{KeyEvent { key: Key, modifiers: Modifiers, pressed: bool }, BootKeyboard, is_boot_keyboard(&Interface) -> bool}` with `BootKeyboard::{start(&mut dyn Bus, slot: u8, &Interface) -> Result<BootKeyboard, UsbError>, poll(&mut self, &mut dyn Bus, &mut VecDeque<KeyEvent>), service(&mut self, &mut dyn Bus), slot(&self) -> u8, interface(&self) -> u8, is_stopped(&self) -> bool}`; `hid::keyboard::{REPEAT_DELAY = 500 ms, REPEAT_INTERVAL = 33.333 ms, RETRY_INTERVAL = 1 s, MAX_RECOVERIES = 3}`.

- [ ] **Step 1: Write the failing tests for `crates/usb/src/hid/keyboard.rs`**

Create `crates/usb/src/hid/keyboard.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};
    use core::fmt::{self, Write};

    const SLOT: u8 = 1;

    /// A bus with one device: records requests, completes the queued IN
    /// transfer with the next scripted report, and has a clock of its own.
    #[derive(Default)]
    struct FakeBus {
        now: Cell<Duration>,
        requests: Vec<(Setup, Vec<u8>)>,
        /// Request codes the device stalls.
        stall: Vec<u8>,
        queued: Option<(u8, usize)>,
        reports: VecDeque<Result<Vec<u8>, UsbError>>,
        clear_halts: u32,
        /// `clear_halt` fails this many more times.
        failing_clear_halts: u32,
        log: RefCell<String>,
    }

    impl Bus for FakeBus {
        fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
            assert_eq!(slot, SLOT);
            self.requests.push((setup, data.to_vec()));
            if self.stall.contains(&setup.request) {
                return Err(UsbError::Stall);
            }
            Ok(data.len())
        }

        fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
            assert_eq!(slot, SLOT);
            if self.queued.is_some() {
                return Err(UsbError::Unsupported("transfer already queued"));
            }
            self.queued = Some((endpoint, len));
            Ok(())
        }

        fn take_in(
            &mut self,
            _slot: u8,
            endpoint: u8,
            buf: &mut [u8],
        ) -> Option<Result<usize, UsbError>> {
            let (ep, len) = self.queued?;
            assert_eq!(ep, endpoint);
            let r = self.reports.pop_front()?;
            self.queued = None;
            Some(r.map(|data| {
                let n = data.len().min(len).min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                n
            }))
        }

        fn clear_halt(&mut self, _slot: u8, _endpoint: u8) -> Result<(), UsbError> {
            self.clear_halts += 1;
            if self.failing_clear_halts > 0 {
                self.failing_clear_halts -= 1;
                return Err(UsbError::Timeout);
            }
            self.queued = None;
            Ok(())
        }

        fn now(&self) -> Duration {
            self.now.get()
        }

        fn log(&self, args: fmt::Arguments) {
            let mut log = self.log.borrow_mut();
            log.write_fmt(args).unwrap();
            log.push('\n');
        }
    }

    impl FakeBus {
        fn advance(&self, ms: u64) {
            self.now.set(self.now.get() + Duration::from_millis(ms));
        }
    }

    /// The K120's first interface: a boot keyboard with an 8-byte
    /// interrupt-IN endpoint polled every 10 ms.
    fn k120_keyboard() -> Interface {
        Interface {
            number: 0,
            class: 3,
            subclass: 1,
            protocol: 1,
            endpoints: vec![Endpoint {
                address: 0x81,
                kind: EndpointKind::Interrupt,
                max_packet: 8,
                interval: 10,
                max_burst: 0,
            }],
        }
    }

    fn started() -> (FakeBus, BootKeyboard) {
        let mut bus = FakeBus::default();
        let kbd = BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).unwrap();
        bus.requests.clear();
        (bus, kbd)
    }

    fn report(modifiers: u8, keys: &[u8]) -> Result<Vec<u8>, UsbError> {
        let mut r = vec![modifiers, 0, 0, 0, 0, 0, 0, 0];
        r[2..2 + keys.len()].copy_from_slice(keys);
        Ok(r)
    }

    /// Delivers reports one poll at a time and returns the events.
    fn feed(
        bus: &mut FakeBus,
        kbd: &mut BootKeyboard,
        reports: &[Result<Vec<u8>, UsbError>],
    ) -> Vec<KeyEvent> {
        let mut out = VecDeque::new();
        for r in reports {
            bus.reports.push_back(r.clone());
            kbd.poll(bus, &mut out);
        }
        out.into_iter().collect()
    }

    /// The characters of the key-down events.
    fn typed(events: &[KeyEvent]) -> String {
        events
            .iter()
            .filter(|e| e.pressed)
            .map(|e| match e.key {
                Key::Char(c) => c as char,
                Key::Enter => '\n',
                _ => '?',
            })
            .collect()
    }

    fn down(c: u8) -> KeyEvent {
        KeyEvent {
            key: Key::Char(c),
            modifiers: Modifiers::default(),
            pressed: true,
        }
    }

    fn up(c: u8) -> KeyEvent {
        KeyEvent {
            pressed: false,
            ..down(c)
        }
    }

    #[test]
    fn boot_keyboards_are_recognised() {
        assert!(is_boot_keyboard(&k120_keyboard()));
        // The K120's second interface: HID, but no boot protocol.
        let mut extra_keys = k120_keyboard();
        (extra_keys.subclass, extra_keys.protocol) = (0, 0);
        assert!(!is_boot_keyboard(&extra_keys));
        let mut mouse = k120_keyboard();
        mouse.protocol = 2;
        assert!(!is_boot_keyboard(&mouse));
        let mut out_only = k120_keyboard();
        out_only.endpoints[0].address = 0x01;
        assert!(!is_boot_keyboard(&out_only));
        let mut no_endpoint = k120_keyboard();
        no_endpoint.endpoints.clear();
        assert!(!is_boot_keyboard(&no_endpoint));
    }

    #[test]
    fn starting_selects_the_boot_protocol_and_queues_a_report() {
        let mut bus = FakeBus::default();
        let kbd = BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).unwrap();
        let setups: Vec<Setup> = bus.requests.iter().map(|r| r.0).collect();
        assert_eq!(
            setups,
            [
                Setup {
                    request_type: 0x21,
                    request: 0x0B,
                    value: 0,
                    index: 0,
                    length: 0
                },
                Setup {
                    request_type: 0x21,
                    request: 0x0A,
                    value: 0,
                    index: 0,
                    length: 0
                },
            ]
        );
        assert_eq!(bus.queued, Some((0x81, 8)));
        assert_eq!((kbd.slot(), kbd.interface()), (SLOT, 0));
    }

    #[test]
    fn a_keyboard_that_refuses_the_boot_protocol_is_not_used() {
        let mut bus = FakeBus {
            stall: vec![0x0B],
            ..FakeBus::default()
        };
        assert_eq!(
            BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).err(),
            Some(UsbError::Stall)
        );
        assert_eq!(bus.queued, None);
    }

    #[test]
    fn a_stalled_set_idle_is_only_logged() {
        let mut bus = FakeBus {
            stall: vec![0x0A],
            ..FakeBus::default()
        };
        assert!(BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).is_ok());
        assert!(bus.log.borrow().contains("SET_IDLE stalled, continuing"));
        assert!(bus.queued.is_some());
    }

    #[test]
    fn presses_and_releases_come_from_comparing_reports() {
        let (mut bus, mut kbd) = started();
        // h, h+i (rollover), i, nothing.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x0B]),
                report(0, &[0x0B, 0x0C]),
                report(0, &[0x0C]),
                report(0, &[]),
            ],
        );
        assert_eq!(events, [down(b'h'), down(b'i'), up(b'h'), up(b'i')]);
        // Every report was followed by a new transfer.
        assert!(bus.queued.is_some());
        // A key that moves to another slot of the report is still held.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x04, 0x05]),
                report(0, &[0x05, 0x04]),
                report(0, &[]),
            ],
        );
        assert_eq!(typed(&events), "ab");
        assert_eq!(events.len(), 4);
        // A usage twice in one report is one key.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[report(0, &[0x04, 0x04]), report(0, &[])],
        );
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn nothing_happens_until_a_report_arrives() {
        let (mut bus, mut kbd) = started();
        let mut out = VecDeque::new();
        for _ in 0..100 {
            kbd.poll(&mut bus, &mut out);
        }
        assert!(out.is_empty());
        assert_eq!(bus.queued, Some((0x81, 8)));
    }

    #[test]
    fn shift_and_ctrl_come_with_the_event() {
        let (mut bus, mut kbd) = started();
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0x02, &[]),
                report(0x02, &[0x04]),
                report(0x01, &[0x06]),
            ],
        );
        assert_eq!(typed(&events), "Ac");
        assert!(events[0].modifiers.shift);
        assert!(events.last().unwrap().modifiers.ctrl);
    }

    #[test]
    fn rollover_error_reports_change_nothing() {
        let (mut bus, mut kbd) = started();
        let phantom = Ok(vec![0, 0, 1, 1, 1, 1, 1, 1]);
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x04]),
                phantom,
                report(0x02, &[0x04, 1]),
                report(0, &[]),
            ],
        );
        // One press, no phantom presses or releases, then the release.
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn short_reports_are_padded_and_tiny_ones_ignored() {
        let (mut bus, mut kbd) = started();
        let events = feed(
            &mut bus,
            &mut kbd,
            &[Ok(vec![0, 0, 0x04]), Ok(vec![0, 0]), Ok(vec![0, 0, 0])],
        );
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn caps_lock_toggles_letters_and_the_led() {
        let (mut bus, mut kbd) = started();
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        assert_eq!(events[0].key, Key::CapsLock);
        assert!(events[0].modifiers.caps_lock);
        kbd.service(&mut bus);
        let led = Setup {
            request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(bus.requests, [(led, vec![0x02])]);
        // The LED is only sent when it changes.
        kbd.service(&mut bus);
        assert_eq!(bus.requests.len(), 1);
        let events = feed(
            &mut bus,
            &mut kbd,
            &[report(0, &[0x04]), report(0x02, &[0x05]), report(0, &[])],
        );
        assert_eq!(typed(&events), "Ab");
        feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        kbd.service(&mut bus);
        assert_eq!(bus.requests[1], (led, vec![0x00]));
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        assert_eq!(typed(&events), "a");
    }

    #[test]
    fn a_keyboard_without_leds_is_asked_once() {
        let (mut bus, mut kbd) = started();
        bus.stall = vec![0x09];
        feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        kbd.service(&mut bus);
        kbd.service(&mut bus);
        assert_eq!(bus.requests.len(), 1);
        assert!(bus.log.borrow().contains("LED report stalled"));
    }

    fn repeats_between(
        bus: &mut FakeBus,
        kbd: &mut BootKeyboard,
        from_ms: u64,
        to_ms: u64,
    ) -> usize {
        let mut out = VecDeque::new();
        for _ in from_ms..to_ms {
            bus.advance(1);
            kbd.poll(bus, &mut out);
        }
        out.iter().filter(|e| e.pressed).count()
    }

    #[test]
    fn a_held_key_repeats_after_500_ms_at_30_per_second() {
        let (mut bus, mut kbd) = started();
        assert_eq!(feed(&mut bus, &mut kbd, &[report(0, &[0x04])]).len(), 1);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 499), 0);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 499, 500), 1);
        // 500 ms more: 15 repeats (one every 33.3 ms).
        assert_eq!(repeats_between(&mut bus, &mut kbd, 500, 1000), 15);
        let events = feed(&mut bus, &mut kbd, &[report(0, &[])]);
        assert_eq!(events, [up(b'a')]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 1000, 2000), 0);
    }

    #[test]
    fn a_long_pause_gives_one_repeat_not_a_burst() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        bus.advance(5000);
        let mut out = VecDeque::new();
        kbd.poll(&mut bus, &mut out);
        kbd.poll(&mut bus, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 34), 1);
    }

    #[test]
    fn the_newest_key_repeats_and_lock_keys_do_not() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        bus.advance(300);
        feed(&mut bus, &mut kbd, &[report(0, &[0x04, 0x05])]);
        let mut out = VecDeque::new();
        for _ in 0..500 {
            bus.advance(1);
            kbd.poll(&mut bus, &mut out);
        }
        assert_eq!(typed(out.make_contiguous()), "b");
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x39])]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 2000), 0);
    }

    #[test]
    fn releasing_another_key_keeps_the_repeat() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        feed(&mut bus, &mut kbd, &[report(0, &[0x04, 0x05])]);
        feed(&mut bus, &mut kbd, &[report(0, &[0x05])]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 500), 1);
    }

    #[test]
    fn a_failed_endpoint_releases_held_keys_and_recovers_in_service() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        let events = feed(&mut bus, &mut kbd, &[Err(UsbError::Stall)]);
        assert_eq!(events, [up(b'a')]);
        // Nothing repeats while the endpoint is down, and nothing is queued.
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 1000), 0);
        assert_eq!(bus.queued, None);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 1);
        assert_eq!(bus.queued, Some((0x81, 8)));
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x05])]);
        assert_eq!(typed(&events), "b");
        assert!(bus.log.borrow().contains("keyboard recovered"));
    }

    #[test]
    fn recovery_is_tried_once_a_second_then_given_up() {
        let (mut bus, mut kbd) = started();
        bus.failing_clear_halts = 10;
        feed(&mut bus, &mut kbd, &[Err(UsbError::Transfer(4))]);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 1);
        for _ in 0..999 {
            bus.advance(1);
            kbd.service(&mut bus);
        }
        assert_eq!(bus.clear_halts, 1);
        bus.advance(1);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 2);
        assert!(!kbd.is_stopped());
        bus.advance(1000);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, MAX_RECOVERIES);
        assert!(kbd.is_stopped());
        bus.advance(5000);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, MAX_RECOVERIES);
        assert!(bus.log.borrow().contains("keyboard given up"));
    }

    #[test]
    fn a_disconnected_keyboard_stops_at_once() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        let events = feed(&mut bus, &mut kbd, &[Err(UsbError::Disconnected)]);
        assert_eq!(events, [up(b'a')]);
        assert!(kbd.is_stopped());
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 0);
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/usb/src/hid/mod.rs`**

In `crates/usb/src/hid/mod.rs`, replace:

````rust

pub mod keymap;
````

with:

````rust

pub mod keyboard;
pub mod keymap;
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Duration` in this scope ``; `` cannot find type `Setup` in this scope ``.

- [ ] **Step 4: Implement `crates/usb/src/hid/keyboard.rs`**

Insert this at the top of `crates/usb/src/hid/keyboard.rs`, above `#[cfg(test)]`:

````rust
//! The HID boot-protocol keyboard (spec §6.3). It keeps one interrupt-IN
//! transfer queued, compares each 8-byte report with the last one to find
//! presses and releases, tracks Caps Lock (shown on the keyboard's LED) and
//! repeats a held key in software: 500 ms, then 30 times a second.
//!
//! `poll` never waits; everything that needs a control transfer (the LED,
//! recovering a halted endpoint) waits for `service`, which runs where
//! blocking is allowed (the console's idle loop).

use super::keymap::{self, Key, Modifiers};
use crate::UsbError;
use crate::bus::{Bus, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use crate::descriptor::{Endpoint, EndpointKind, Interface};
use alloc::collections::VecDeque;
use core::time::Duration;

pub const REPEAT_DELAY: Duration = Duration::from_millis(500);
/// 30 repeats a second.
pub const REPEAT_INTERVAL: Duration = Duration::from_micros(33_333);
/// Time between attempts to recover a failed interrupt endpoint.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(1);
/// Failed recoveries in a row after which the keyboard is given up.
pub const MAX_RECOVERIES: u32 = 3;

/// HID class requests (HID 1.11 §7.2).
const SET_REPORT: u8 = 0x09;
const SET_IDLE: u8 = 0x0A;
const SET_PROTOCOL: u8 = 0x0B;
const BOOT_PROTOCOL: u16 = 0;
/// `SET_REPORT`'s value: report type Output (2), report ID 0.
const OUTPUT_REPORT: u16 = 0x0200;
/// Bit 1 of the LED output report.
const LED_CAPS_LOCK: u8 = 0x02;
const USAGE_CAPS_LOCK: u8 = 0x39;
/// The largest report read; a boot report is 8 bytes.
const MAX_REPORT: usize = 64;

/// A key going down (also when it repeats) or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    /// The modifiers when the event happened; `key` already has Shift and
    /// Caps Lock applied.
    pub modifiers: Modifiers,
    pub pressed: bool,
}

fn interrupt_in(iface: &Interface) -> Option<&Endpoint> {
    iface
        .endpoints
        .iter()
        .find(|e| e.is_in() && e.kind == EndpointKind::Interrupt && e.packet_size() > 0)
}

/// Class 3 (HID), subclass 1 (boot), protocol 1 (keyboard), with an
/// interrupt-IN endpoint: the K120 and the Unifying receiver's keyboard.
pub fn is_boot_keyboard(iface: &Interface) -> bool {
    (iface.class, iface.subclass, iface.protocol) == (3, 1, 1) && interrupt_in(iface).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Running,
    /// The interrupt endpoint failed; recovery is tried at `retry_at`.
    Failed {
        retry_at: Duration,
        attempts: u32,
    },
    /// Given up, or the device is gone.
    Stopped,
}

pub struct BootKeyboard {
    slot: u8,
    interface: u8,
    endpoint: u8,
    len: usize,
    /// Usages held in the last report accepted (0 is an empty slot).
    keys: [u8; 6],
    modifier_byte: u8,
    caps_lock: bool,
    /// The LED state the keyboard shows.
    leds: u8,
    /// The key being repeated and when it repeats next.
    repeat: Option<(u8, Duration)>,
    state: State,
}

impl BootKeyboard {
    /// Claims `iface` of the configured device in `slot`: `SET_PROTOCOL`
    /// (boot), `SET_IDLE(0)` (reports only on changes; a stall is fine),
    /// then the first interrupt-IN transfer.
    pub fn start(bus: &mut dyn Bus, slot: u8, iface: &Interface) -> Result<BootKeyboard, UsbError> {
        let ep = *interrupt_in(iface).ok_or(UsbError::Unsupported("no interrupt IN endpoint"))?;
        class_request(
            bus,
            slot,
            iface.number,
            SET_PROTOCOL,
            BOOT_PROTOCOL,
            &mut [],
        )?;
        if let Err(e) = class_request(bus, slot, iface.number, SET_IDLE, 0, &mut []) {
            bus.log(format_args!(
                "hid: slot {slot} interface {}: SET_IDLE {e}, continuing",
                iface.number
            ));
        }
        let len = (ep.packet_size() as usize).min(MAX_REPORT);
        bus.queue_in(slot, ep.address, len)?;
        Ok(BootKeyboard {
            slot,
            interface: iface.number,
            endpoint: ep.address,
            len,
            keys: [0; 6],
            modifier_byte: 0,
            caps_lock: false,
            leds: 0,
            repeat: None,
            state: State::Running,
        })
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    pub fn interface(&self) -> u8 {
        self.interface
    }

    /// Whether the keyboard was given up (its device is gone or its
    /// endpoint cannot be recovered).
    pub fn is_stopped(&self) -> bool {
        self.state == State::Stopped
    }

    /// Takes a finished report, queues the next transfer and produces the
    /// events, including repeats that are due. Never waits.
    pub fn poll(&mut self, bus: &mut dyn Bus, out: &mut VecDeque<KeyEvent>) {
        if self.state == State::Running {
            let mut buf = [0u8; MAX_REPORT];
            match bus.take_in(self.slot, self.endpoint, &mut buf[..self.len]) {
                None => {}
                Some(Ok(n)) => {
                    self.report(&buf[..n.min(self.len)], bus.now(), out);
                    if let Err(e) = bus.queue_in(self.slot, self.endpoint, self.len) {
                        self.fail(bus, e, out);
                    }
                }
                Some(Err(e)) => self.fail(bus, e, out),
            }
        }
        self.repeat_due(bus.now(), out);
    }

    /// The work that needs control transfers: the Caps Lock LED and
    /// recovering a failed endpoint (at most once a second).
    pub fn service(&mut self, bus: &mut dyn Bus) {
        if let State::Failed { retry_at, attempts } = self.state {
            if bus.now() < retry_at {
                return;
            }
            let recovered = bus
                .clear_halt(self.slot, self.endpoint)
                .and_then(|()| bus.queue_in(self.slot, self.endpoint, self.len));
            self.state = match recovered {
                Ok(()) => {
                    bus.log(format_args!("hid: slot {}: keyboard recovered", self.slot));
                    State::Running
                }
                Err(e) if attempts + 1 >= MAX_RECOVERIES => {
                    bus.log(format_args!(
                        "hid: slot {}: recovery failed ({e}); keyboard given up",
                        self.slot
                    ));
                    State::Stopped
                }
                Err(e) => {
                    bus.log(format_args!(
                        "hid: slot {}: recovery failed ({e})",
                        self.slot
                    ));
                    State::Failed {
                        retry_at: bus.now() + RETRY_INTERVAL,
                        attempts: attempts + 1,
                    }
                }
            };
        }
        let leds = if self.caps_lock { LED_CAPS_LOCK } else { 0 };
        if self.state == State::Running && leds != self.leds {
            // Tried once: a keyboard without LEDs must not be asked again
            // on every idle loop.
            self.leds = leds;
            let mut data = [leds];
            if let Err(e) = class_request(
                bus,
                self.slot,
                self.interface,
                SET_REPORT,
                OUTPUT_REPORT,
                &mut data,
            ) {
                bus.log(format_args!("hid: slot {}: LED report {e}", self.slot));
            }
        }
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::from_report(self.modifier_byte, self.caps_lock)
    }

    fn emit(&self, usage: u8, pressed: bool, out: &mut VecDeque<KeyEvent>) {
        if let Some(key) = keymap::key(usage, self.modifiers()) {
            out.push_back(KeyEvent {
                key,
                modifiers: self.modifiers(),
                pressed,
            });
        }
    }

    /// One boot report: modifiers, a reserved byte, up to six usages.
    fn report(&mut self, r: &[u8], now: Duration, out: &mut VecDeque<KeyEvent>) {
        if r.len() < 3 {
            return;
        }
        let mut keys = [0u8; 6];
        for (k, &u) in keys.iter_mut().zip(&r[2..]) {
            *k = u;
        }
        // ErrorRollOver (too many keys), POSTFail and ErrorUndefined: the
        // keyboard does not know which keys are down, so nothing changes.
        if keys.iter().any(|u| (1..=3).contains(u)) {
            return;
        }
        // A usage listed twice is one key.
        for i in 1..keys.len() {
            if keys[..i].contains(&keys[i]) {
                keys[i] = 0;
            }
        }
        self.modifier_byte = r[0];
        for u in self.keys {
            if u != 0 && !keys.contains(&u) {
                self.emit(u, false, out);
                if self.repeat.is_some_and(|(r, _)| r == u) {
                    self.repeat = None;
                }
            }
        }
        for u in keys {
            if u == 0 || self.keys.contains(&u) {
                continue;
            }
            if u == USAGE_CAPS_LOCK {
                self.caps_lock = !self.caps_lock;
            }
            self.emit(u, true, out);
            if repeats(u) {
                self.repeat = Some((u, now + REPEAT_DELAY));
            }
        }
        self.keys = keys;
    }

    /// At most one repeat per call: after a long gap (a busy command) the
    /// key repeats once, not in a burst.
    fn repeat_due(&mut self, now: Duration, out: &mut VecDeque<KeyEvent>) {
        let Some((usage, due)) = self.repeat else {
            return;
        };
        if now < due {
            return;
        }
        self.emit(usage, true, out);
        let next = due + REPEAT_INTERVAL;
        self.repeat = Some((
            usage,
            if next <= now {
                now + REPEAT_INTERVAL
            } else {
                next
            },
        ));
    }

    /// The endpoint failed: every held key is released (nothing may keep
    /// repeating) and recovery waits for `service`.
    fn fail(&mut self, bus: &mut dyn Bus, e: UsbError, out: &mut VecDeque<KeyEvent>) {
        bus.log(format_args!(
            "hid: slot {} endpoint {:#04x}: {e}",
            self.slot, self.endpoint
        ));
        for u in core::mem::take(&mut self.keys) {
            if u != 0 {
                self.emit(u, false, out);
            }
        }
        self.repeat = None;
        self.state = if e == UsbError::Disconnected {
            State::Stopped
        } else {
            State::Failed {
                retry_at: bus.now(),
                attempts: 0,
            }
        };
    }
}

/// Lock keys toggle; they do not repeat.
fn repeats(usage: u8) -> bool {
    !matches!(usage, 0x39 | 0x47 | 0x53)
}

fn class_request(
    bus: &mut dyn Bus,
    slot: u8,
    interface: u8,
    request: u8,
    value: u16,
    data: &mut [u8],
) -> Result<(), UsbError> {
    let setup = Setup {
        request_type: TYPE_CLASS | RECIPIENT_INTERFACE,
        request,
        value,
        index: interface as u16,
        length: data.len() as u16,
    };
    bus.control(slot, setup, data).map(|_| ())
}

````

- [ ] **Step 5: Change `crates/usb/src/hid/mod.rs`**

In `crates/usb/src/hid/mod.rs`, replace:

````rust

pub use keymap::{Key, Modifiers};
````

with:

````rust

pub use keyboard::{BootKeyboard, KeyEvent, is_boot_keyboard};
pub use keymap::{Key, Modifiers};
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 205 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -m "usb: the HID boot keyboard"
````


### Task 15: The host: devices, keyboards and hot-plug on one controller

`Host` puts one controller and its class drivers together. `service` handles every port change (decision 4): a new connection is set up and its boot-keyboard interfaces are configured and handed to keyboard drivers (a device nothing claims stays addressed but unused, like the stick until plan 5); a disconnection, or a quick unplug and replug between two looks, drops the device's keyboards (a held key stops repeating with them) and frees the slot. It then lets the keyboards do their blocking work. It returns a line per device set up, for the startup screen (`port 3: 046d:c31c low-speed, keyboard`, `port 5: setup failed: timed out`, and `keyboard not started: <reason>` when a keyboard refuses the boot protocol, since without a keyboard nobody can read `dmesg` on the NUC). `poll` never waits and collects key events, at most 256 unread. Everything is tested on the fake controller with the fake devices, including unplugging a keyboard while a key is held.

**Files:**
- Create: `crates/usb/src/host.rs`
- Modify: `crates/usb/src/lib.rs`

**Interfaces:**
- Consumes: `Xhci` (Tasks 8–12), `BootKeyboard` (Task 14).
- Produces: `usb::host::{Host<H: Hal>, Attached { port, outcome: Result<Found, UsbError> } (Display), Found { vendor, product, speed, keyboards, not_started: Option<UsbError> }, MAX_EVENTS = 256}` with `Host::{new(Xhci<H>), xhci(&self) -> &Xhci<H>, keyboards(&self) -> usize, service(&mut self) -> Vec<Attached>, poll(&mut self), next_key(&mut self) -> Option<KeyEvent>}`.

- [ ] **Step 1: Write the failing tests for `crates/usb/src/host.rs`**

Create `crates/usb/src/host.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Key;
    use crate::testing::{FakeConfig, FakeHal, FakeUsbDevice, start};
    use alloc::string::{String, ToString};
    use core::time::Duration;

    fn host(config: FakeConfig) -> (FakeHal, Host<FakeHal>) {
        let (hal, xhci) = start(config);
        (hal, Host::new(xhci))
    }

    /// Polls for `ms` milliseconds of fake time and returns the characters
    /// of the key presses that came out.
    fn typed(hal: &FakeHal, host: &mut Host<FakeHal>, ms: u64) -> String {
        let mut s = String::new();
        for _ in 0..ms {
            host.poll();
            while let Some(e) = host.next_key() {
                if let (true, Key::Char(c)) = (e.pressed, e.key) {
                    s.push(c as char);
                }
            }
            hal.sleep(Duration::from_millis(1));
        }
        s
    }

    fn key_report(usage: u8) -> [u8; 8] {
        [0, 0, usage, 0, 0, 0, 0, 0]
    }

    #[test]
    fn devices_present_at_start_are_set_up_and_reported() {
        // Laid out as QEMU's e2e machine: USB 3 ports first.
        let (hal, mut host) = host(FakeConfig::qemu());
        hal.fake().plug(2, FakeUsbDevice::kingston_stick());
        hal.fake().plug(5, FakeUsbDevice::qemu_keyboard());
        // The stick's USB 3 link trains before its port shows a connection.
        hal.sleep(Duration::from_millis(60));
        let lines: Vec<String> = host.service().iter().map(|a| a.to_string()).collect();
        assert_eq!(
            lines,
            [
                "port 2: 0951:1666 SuperSpeed, not claimed",
                "port 5: 0627:0001 high-speed, keyboard",
            ]
        );
        assert_eq!(host.keyboards(), 1);
        // Nothing changed since: nothing more to do.
        assert!(host.service().is_empty());
    }

    #[test]
    fn typing_on_a_keyboard_gives_key_events() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for usage in [0x0B, 0x00, 0x0C, 0x00] {
            k120.borrow_mut().push_in(0x81, &key_report(usage));
        }
        assert_eq!(typed(&hal, &mut host, 200), "hi");
    }

    #[test]
    fn only_the_receivers_keyboard_interface_is_claimed() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(1, FakeUsbDevice::unifying_receiver());
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 1: 046d:c534 full-speed, keyboard"
        );
        let slot = host.xhci().slot_of_port(1).unwrap() as usize;
        // Interface 0's endpoint 0x81 (DCI 3) only; the mouse's 0x82 and
        // the vendor interface's 0x83 stay unconfigured.
        assert!(hal.fake().endpoint(slot, 3).is_some());
        assert!(hal.fake().endpoint(slot, 5).is_none());
        assert!(hal.fake().endpoint(slot, 7).is_none());
    }

    #[test]
    fn a_keyboard_plugged_in_later_is_picked_up_and_dropped_when_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        assert!(host.service().is_empty());
        let before = hal.outstanding_dma();
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1);
        hal.fake().unplug(3);
        assert!(host.service().is_empty());
        assert_eq!(host.keyboards(), 0);
        assert_eq!(host.xhci().slot_of_port(3), None);
        assert_eq!(
            hal.outstanding_dma(),
            before,
            "the device's memory is freed"
        );
        // Plugged in again, it works again.
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        assert_eq!(host.service().len(), 1);
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        k120.borrow_mut().push_in(0x81, &key_report(0x00));
        assert_eq!(typed(&hal, &mut host, 100), "a");
    }

    #[test]
    fn a_quick_replug_replaces_the_device() {
        let (hal, mut host) = host(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        host.service();
        // Unplugged and another keyboard plugged in before the console
        // looked again: the port is connected both times.
        hal.fake().unplug(3);
        let other = FakeUsbDevice::qemu_keyboard();
        hal.fake().plug(3, other.clone());
        let attached = host.service();
        assert_eq!(attached.len(), 1);
        assert_eq!(
            attached[0].to_string(),
            "port 3: 0627:0001 high-speed, keyboard"
        );
        assert_eq!(host.keyboards(), 1);
        other.borrow_mut().push_in(0x81, &key_report(0x05));
        assert_eq!(typed(&hal, &mut host, 100), "b");
    }

    #[test]
    fn a_held_key_stops_repeating_when_its_keyboard_is_unplugged() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(typed(&hal, &mut host, 2000), "");
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_keyboard_whose_transfers_just_stop_is_dropped_too() {
        // A controller that does not fail the queued report on unplug: the
        // keyboard never sees an error, so only the port change stops it.
        let mut config = FakeConfig::intel();
        config.fail_transfers_on_unplug = false;
        let (hal, mut host) = host(config);
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        k120.borrow_mut().push_in(0x81, &key_report(0x04));
        assert_eq!(typed(&hal, &mut host, 100), "a");
        hal.fake().unplug(3);
        host.service();
        assert_eq!(host.keyboards(), 0);
        assert_eq!(typed(&hal, &mut host, 2000), "");
    }

    #[test]
    fn a_keyboard_that_cannot_be_started_says_so_on_screen() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x0B, 0); // SET_PROTOCOL(boot)
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(
            attached[0].to_string(),
            "port 3: 046d:c31c low-speed, keyboard not started: stalled"
        );
        assert_eq!(host.keyboards(), 0);
    }

    #[test]
    fn a_device_that_fails_setup_is_reported_once() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        k120.borrow_mut().stall_request(0x06, 0x0100);
        hal.fake().plug(3, k120);
        let attached = host.service();
        assert_eq!(attached[0].to_string(), "port 3: setup failed: stalled");
        assert!(host.service().is_empty());
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: port 3: setup failed: stalled")
        );
    }

    #[test]
    fn unread_key_events_are_bounded() {
        let (hal, mut host) = host(FakeConfig::intel());
        let k120 = FakeUsbDevice::k120();
        hal.fake().plug(3, k120.clone());
        host.service();
        for _ in 0..MAX_EVENTS {
            k120.borrow_mut().push_in(0x81, &key_report(0x04));
            k120.borrow_mut().push_in(0x81, &key_report(0x00));
        }
        for _ in 0..4 * MAX_EVENTS {
            host.poll();
            hal.sleep(Duration::from_millis(1));
        }
        assert_eq!(host.events.len(), MAX_EVENTS);
    }

    #[test]
    fn a_keyboard_plugged_in_before_boot_is_found_by_the_first_service() {
        use crate::testing::{FAKE_BAR, FAKE_BAR_LEN};
        let hal = FakeHal::with_controller(FakeConfig::intel());
        hal.fake().plug(3, FakeUsbDevice::k120());
        let xhci = Xhci::new(hal.clone(), FAKE_BAR, FAKE_BAR_LEN, "00:14.0").unwrap();
        let mut host = Host::new(xhci);
        assert_eq!(host.service().len(), 1);
        assert_eq!(host.keyboards(), 1, "[ ok ] keyboard at boot");
    }
}
````

- [ ] **Step 2: Declare the new module in `crates/usb/src/lib.rs`**

In `crates/usb/src/lib.rs`, replace:

````rust
pub mod hid;
#[cfg(test)]
````

with:

````rust
pub mod hid;
pub mod host;
#[cfg(test)]
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` cannot find type `Host` in this scope ``; `` cannot find value `MAX_EVENTS` in this scope ``.

- [ ] **Step 4: Implement `crates/usb/src/host.rs`**

Insert this at the top of `crates/usb/src/host.rs`, above `#[cfg(test)]`:

````rust
//! One host controller with its class drivers (spec §6): devices are set up
//! when they appear on a root port (at start and when plugged in later),
//! boot-keyboard interfaces go to the keyboard driver, and a device's
//! drivers are dropped when it goes away. The kernel keeps one `Host` per
//! xHCI controller and polls it from the console.

use crate::hid::{BootKeyboard, KeyEvent, is_boot_keyboard};
use crate::xhci::{Device, Xhci};
use crate::{Hal, Speed, UsbError};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::fmt;

/// Key events kept for the console at most; beyond this new ones are
/// dropped (nobody is reading).
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

pub struct Host<H: Hal> {
    xhci: Xhci<H>,
    keyboards: Vec<BootKeyboard>,
    events: VecDeque<KeyEvent>,
}

impl<H: Hal> Host<H> {
    pub fn new(xhci: Xhci<H>) -> Host<H> {
        Host {
            xhci,
            keyboards: Vec::new(),
            events: VecDeque::new(),
        }
    }

    pub fn xhci(&self) -> &Xhci<H> {
        &self.xhci
    }

    /// Boot keyboards in use.
    pub fn keyboards(&self) -> usize {
        self.keyboards.len()
    }

    /// Handles ports that changed: sets up new devices, drops those that
    /// went away, and lets the keyboards do their blocking work (LEDs,
    /// recovery). Waits while a device is set up (a few hundred ms), so it
    /// runs only where the caller may block. Returns one line per device
    /// set up.
    pub fn service(&mut self) -> Vec<Attached> {
        let mut attached = Vec::new();
        for change in self.xhci.port_changes() {
            if let Some(slot) = self.xhci.slot_of_port(change.port)
                && (!change.connected || change.reconnected)
            {
                self.drop_device(slot);
            }
            if change.connected && self.xhci.slot_of_port(change.port).is_none() {
                attached.push(self.attach(change.port));
            }
        }
        for k in &mut self.keyboards {
            k.service(&mut self.xhci);
        }
        attached
    }

    /// Processes events and keyboard reports. Never waits.
    pub fn poll(&mut self) {
        self.xhci.poll();
        for k in &mut self.keyboards {
            k.poll(&mut self.xhci, &mut self.events);
        }
        self.events.truncate(MAX_EVENTS);
    }

    /// The oldest key event not yet taken.
    pub fn next_key(&mut self) -> Option<KeyEvent> {
        self.events.pop_front()
    }

    fn attach(&mut self, port: u8) -> Attached {
        let outcome = self.xhci.attach(port).and_then(|d| self.claim(d));
        if let Err(e) = &outcome {
            self.log(format_args!("port {port}: setup failed: {e}"));
        }
        Attached { port, outcome }
    }

    /// Configures the device for the interfaces a driver wants and starts
    /// the drivers. A device nothing claims stays addressed but unused.
    fn claim(&mut self, d: Device) -> Result<Found, UsbError> {
        let interfaces: Vec<u8> = d
            .configuration
            .interfaces
            .iter()
            .filter(|i| is_boot_keyboard(i))
            .map(|i| i.number)
            .collect();
        let mut found = Found {
            vendor: d.descriptor.vendor,
            product: d.descriptor.product,
            speed: d.speed,
            keyboards: 0,
            not_started: None,
        };
        if interfaces.is_empty() {
            self.log(format_args!("slot {}: no driver for this device", d.slot));
            return Ok(found);
        }
        if let Err(e) = self.xhci.configure(&d, &interfaces) {
            self.xhci.detach(d.slot);
            return Err(e);
        }
        for iface in d
            .configuration
            .interfaces
            .iter()
            .filter(|i| interfaces.contains(&i.number))
        {
            match BootKeyboard::start(&mut self.xhci, d.slot, iface) {
                Ok(k) => {
                    self.keyboards.push(k);
                    found.keyboards += 1;
                }
                Err(e) => {
                    self.log(format_args!(
                        "slot {} interface {}: keyboard not started: {e}",
                        d.slot, iface.number
                    ));
                    found.not_started = Some(e);
                }
            }
        }
        Ok(found)
    }

    fn log(&self, args: fmt::Arguments) {
        self.xhci
            .hal()
            .log(format_args!("xhci {}: {args}", self.xhci.name()));
    }

    /// The device in `slot` is gone: its keyboards stop (a held key stops
    /// repeating with them) and the controller forgets it.
    fn drop_device(&mut self, slot: u8) {
        self.keyboards.retain(|k| k.slot() != slot);
        self.xhci.detach(slot);
    }
}

````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p usb`

Expected: PASS: 216 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -m "usb: the host: devices, keyboards and hot-plug on one controller"
````


### Finish PR 6

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 10 scenario(s) passed`.

````bash
git push -u origin plan4/hid-keyboard
gh pr create --base main --head plan4/hid-keyboard --title "Plan 4: The HID keyboard and the host" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 13–15: the US keymap; the HID boot keyboard (boot protocol, report diffing with rollover rejection, Caps Lock with its LED, repeat after 500 ms at 30/s, recovery of a failed endpoint); `Host` with boot-time and hot-plugged devices.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller

## Hardware

- [x] Not needed: host-tested code only, nothing new runs on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/hid-keyboard --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-hid-keyboard
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 7: The USB keyboard in the kernel (Tasks 16–19)

The kernel's `Hal`, key presses as terminal input, the e2e `key` step, and USB at startup: the shell can be typed at on the USB keyboard, in QEMU and on the NUC (check 2).

Branch `plan4/usb-kernel`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-kernel`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b plan4/usb-kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-kernel origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-kernel
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. If `plan4/hid-keyboard` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b plan4/usb-kernel /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-kernel plan4/hid-keyboard`), and after its merge rebase with `git rebase --onto origin/main <old tip of plan4/hid-keyboard>` and re-run `cargo xtask ci` before pushing.

### Task 16: DMA memory, PCI power-up and the kernel's Hal

What the USB stack needs from the kernel. `mm::alloc_dma` hands out zeroed, physically contiguous whole frames (spec §5.1), at least frame-aligned; they are RAM in the linear map, write-back, which is right because x86 DMA is cache-coherent. `pci::wake_to_d0` puts a function into power state D0 through its Power Management capability (the firmware may leave the NUC's Thunderbolt xHCI in D3hot, where its registers read all ones), walking the capability list with a bound. Leaving D3hot without PMCSR's No_Soft_Reset resets the function, clearing its BARs (PCI PM 1.2 §5.4.1), so `restore_bars` writes back the addresses enumeration found. `pci::enable_device` does all of that (waiting 10 ms after a change), then enables memory decoding and bus mastering (spec §5.5), through the ECAM windows `pci::init` now remembers. `timer::tsc_time` is time from the TSC: unlike `uptime` it does not need the timer interrupt, so USB timeouts expire even if the LAPIC timer failed. `KernelHal` implements `usb::Hal` over all of it, with uncached `map_mmio` and the kernel log.

**Files:**
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `kernel/Cargo.toml`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/mm/mod.rs`
- Modify: `kernel/src/pci.rs`
- Modify: `kernel/src/timer.rs`
- Create: `kernel/src/usb.rs`

**Interfaces:**
- Consumes: `usb::{Hal, DmaBuf}` (Task 3); the frame allocator, `mm::map_mmio`, `timer`, PCI (plan 2).
- Produces: `mm::{dma_frames(size, align) -> (usize, usize), alloc_dma(size, align) -> Option<(NonNull<u8>, u64)>, free_dma(phys, size)}`; `pci::{Wake { from: u8, reset: bool }, wake_to_d0(&mut impl ConfigSpace, PciAddress) -> Option<Wake>, restore_bars(&mut impl ConfigSpace, PciAddress, &[Bar]), enable_device(&PciDevice) -> Option<Wake>}`; `timer::{cycles_to_duration(cycles, tsc_hz) -> Duration, tsc_time() -> Option<Duration>}`; `relay_kernel::usb::KernelHal` implementing `usb::Hal`.

- [ ] **Step 1: Change `kernel/Cargo.toml`**

In `kernel/Cargo.toml`, replace:

````toml
vfs.workspace = true
x86_64.workspace = true
````

with:

````toml
vfs.workspace = true
usb.workspace = true
x86_64.workspace = true
````

- [ ] **Step 2: Add the failing tests to `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust
    Ok((PHYS_OFFSET + phys) as *mut u8)
}
````

with:

````rust
    Ok((PHYS_OFFSET + phys) as *mut u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dma_buffers_take_whole_aligned_frames() {
        assert_eq!(dma_frames(1, 64), (1, 1));
        assert_eq!(dma_frames(4096, 4096), (1, 1));
        assert_eq!(dma_frames(4097, 16), (2, 1));
        assert_eq!(dma_frames(0, 0), (1, 1));
        assert_eq!(dma_frames(8192, 65536), (2, 16));
    }
}
````

- [ ] **Step 3: Add the failing tests to `kernel/src/pci.rs`**

In `kernel/src/pci.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            } else {
                f.regs[i] = value;
````

with:

````rust
            } else {
                // Power management control at 0x84 (see
                // `with_power_management`): leaving D3hot without
                // No_Soft_Reset resets the function, clearing its BARs'
                // address bits and its command register.
                if offset == 0x84
                    && f.regs[i] & 0x3 == 3
                    && value & 0x3 == 0
                    && f.regs[i] & NO_SOFT_RESET == 0
                {
                    for b in 0..6 {
                        let mask = f.bar_masks[b];
                        f.regs[4 + b] &= !mask;
                    }
                    f.regs[1] &= 0xFFFF_0000;
                }
                f.regs[i] = value;
````

Replace:

````rust
    }

    #[test]
    fn summary_line() {
````

with:

````rust
    }

    /// Gives the function at `a` a capability list: an MSI capability,
    /// then power management in `state`.
    fn with_power_management(c: &mut FakeConfig, a: PciAddress, state: u32) {
        let f = c.funcs.get_mut(&a).unwrap();
        f.regs[0x34 / 4] = 0x70;
        f.regs[0x70 / 4] = 0x0080_8005; // MSI, next at 0x80
        f.regs[0x80 / 4] = 0xC803_0001; // power management, last
        f.regs[0x84 / 4] = 0x0000_8100 | state; // PME status and enable set
    }

    #[test]
    fn a_function_in_d3_is_woken_to_d0() {
        let mut c = nuc();
        let a = addr(0, 0x0d, 0);
        with_power_management(&mut c, a, 3);
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 3,
                reset: true
            })
        );
        assert_eq!(
            c.funcs[&a].regs[0x84 / 4],
            0x0000_0100,
            "D0, PME enable kept, PME status left alone"
        );
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 0,
                reset: false
            })
        );
        assert_eq!(c.writes.iter().filter(|w| w.1 == 0x84).count(), 1);
    }

    #[test]
    fn a_function_reset_by_its_wake_gets_its_bars_back() {
        let mut c = nuc();
        let a = addr(0, 0x0d, 0);
        let bars = enumerate(&mut c, 0, 0)
            .into_iter()
            .find(|d| d.address == a)
            .unwrap()
            .bars;
        let before = (c.funcs[&a].regs[4], c.funcs[&a].regs[5]);
        with_power_management(&mut c, a, 3);
        assert!(wake_to_d0(&mut c, a).unwrap().reset);
        assert_eq!(c.funcs[&a].regs[4] & !0xF, 0, "the wake cleared BAR0");
        restore_bars(&mut c, a, &bars);
        assert_eq!((c.funcs[&a].regs[4], c.funcs[&a].regs[5]), before);
        // With No_Soft_Reset the function keeps its configuration.
        let mut c = nuc();
        with_power_management(&mut c, a, 3 | NO_SOFT_RESET);
        assert_eq!(
            wake_to_d0(&mut c, a),
            Some(Wake {
                from: 3,
                reset: false
            })
        );
        assert_eq!((c.funcs[&a].regs[4], c.funcs[&a].regs[5]), before);
    }

    #[test]
    fn functions_without_power_management_are_left_alone() {
        let mut c = nuc();
        let a = addr(0, 0x14, 0);
        assert_eq!(wake_to_d0(&mut c, a), None);
        // A capability list that loops ends too.
        let f = c.funcs.get_mut(&a).unwrap();
        f.regs[0x34 / 4] = 0x40;
        f.regs[0x40 / 4] = 0x0000_4005;
        assert_eq!(wake_to_d0(&mut c, a), None);
        // No capability list at all.
        c.funcs.get_mut(&a).unwrap().regs[1] = 0x0280_0007;
        with_power_management(&mut c, a, 3);
        c.funcs.get_mut(&a).unwrap().regs[1] = 0x0280_0007;
        assert_eq!(wake_to_d0(&mut c, a), None);
        assert!(c.writes.is_empty());
    }

    #[test]
    fn summary_line() {
````

- [ ] **Step 4: Add the failing tests to `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, replace:

````rust
    #[test]
    fn nuc_cpuid_gives_2496_mhz() {
````

with:

````rust
    #[test]
    fn tsc_cycles_become_time() {
        assert_eq!(
            cycles_to_duration(2_496_000_000, 2_496_000_000),
            Duration::from_secs(1)
        );
        assert_eq!(
            cycles_to_duration(2_496, 2_496_000_000),
            Duration::from_micros(1)
        );
        // Ten years of cycles at 5 GHz do not overflow.
        let ten_years = 10 * 365 * 86_400;
        assert_eq!(
            cycles_to_duration(ten_years * 5_000_000_000, 5_000_000_000),
            Duration::from_secs(ten_years)
        );
    }

    #[test]
    fn nuc_cpuid_gives_2496_mhz() {
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find value `NO_SOFT_RESET` in this scope ``; `` cannot find struct, variant or union type `Wake` in this scope ``.

- [ ] **Step 6: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
pub mod timer;

````

with:

````rust
pub mod timer;
pub mod usb;

````

- [ ] **Step 7: Change `kernel/src/mm/mod.rs`**

In `kernel/src/mm/mod.rs`, replace:

````rust

/// Maps device memory (or firmware tables) at `PHYS_OFFSET + phys` with the
````

with:

````rust

/// Frames and frame alignment for a DMA buffer of `size` bytes aligned to
/// `align` bytes: whole frames, and at least frame-aligned.
pub fn dma_frames(size: usize, align: usize) -> (usize, usize) {
    let frame = FRAME_SIZE as usize;
    (size.max(1).div_ceil(frame), align.div_ceil(frame).max(1))
}

/// Zeroed, physically contiguous memory for device DMA (the USB stack's
/// rings, contexts and buffers): whole frames, aligned to `align` bytes or
/// a frame. Frames are RAM, so the linear map covers them, write-back
/// (x86 DMA is cache-coherent). Returns the virtual and physical address.
pub fn alloc_dma(size: usize, align: usize) -> Option<(core::ptr::NonNull<u8>, u64)> {
    let (count, align) = dma_frames(size, align);
    if !align.is_power_of_two() {
        return None;
    }
    let phys = MEMORY.lock().as_mut()?.frames.alloc(count, align)?;
    let virt = (PHYS_OFFSET + phys) as *mut u8;
    // SAFETY: fresh frames, mapped through the linear map, owned by the
    // caller from now on.
    unsafe { core::ptr::write_bytes(virt, 0, count * FRAME_SIZE as usize) };
    Some((core::ptr::NonNull::new(virt)?, phys))
}

/// Gives back memory from `alloc_dma` of the same `size`.
pub fn free_dma(phys: u64, size: usize) {
    let (count, _) = dma_frames(size, 1);
    if let Some(m) = MEMORY.lock().as_mut() {
        m.frames.free(phys, count);
    }
}

/// Maps device memory (or firmware tables) at `PHYS_OFFSET + phys` with the
````

- [ ] **Step 8: Change `kernel/src/pci.rs`**

In `kernel/src/pci.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
const HEADER_MULTI_FUNCTION: u8 = 0x80;

````

with:

````rust
const HEADER_MULTI_FUNCTION: u8 = 0x80;
/// Status register bit 4 (the upper half of dword 1): a capability list.
const STATUS_CAPABILITIES: u32 = 1 << 20;
const CAPABILITIES_POINTER: u16 = 0x34;
const CAP_POWER_MANAGEMENT: u8 = 0x01;
/// PMCSR bit 3: the function keeps its configuration from D3hot to D0.
const NO_SOFT_RESET: u32 = 1 << 3;

````

Replace:

````rust
    cfg.write32(a, COMMAND, command | MEMORY_SPACE | BUS_MASTER);
}
````

with:

````rust
    cfg.write32(a, COMMAND, command | MEMORY_SPACE | BUS_MASTER);
}

/// What `wake_to_d0` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wake {
    /// The power state the function was in (0 is D0, 3 is D3hot).
    pub from: u8,
    /// Leaving D3hot reset the function (PMCSR's No_Soft_Reset was 0, PCI
    /// PM 1.2 §5.4.1): its BARs and command register are cleared.
    pub reset: bool,
}

/// Puts the function into power state D0 through its PCI Power Management
/// capability (firmware may leave an unused controller in D3hot, where its
/// registers read as all ones). `None` without the capability. After a
/// change from D3hot the device needs 10 ms before it is used (PCI PM 1.2
/// §5.6.1), and if it was reset, its BARs back (`restore_bars`).
pub fn wake_to_d0(cfg: &mut impl ConfigSpace, a: PciAddress) -> Option<Wake> {
    if cfg.read32(a, COMMAND) & STATUS_CAPABILITIES == 0 {
        return None;
    }
    let mut ptr = (cfg.read32(a, CAPABILITIES_POINTER) & 0xFC) as u16;
    // Capabilities live in 0x40..0x100; a list longer than fits there
    // loops, so the walk is bounded.
    for _ in 0..48 {
        if ptr < 0x40 {
            return None;
        }
        let header = cfg.read32(a, ptr);
        if header as u8 == CAP_POWER_MANAGEMENT {
            let pmcsr = cfg.read32(a, ptr + 4);
            let from = (pmcsr & 0x3) as u8;
            if from != 0 {
                // Power state 0; bit 15 (PME status) is write-one-to-clear
                // and the upper half is read-only, so both are written as 0.
                cfg.write32(a, ptr + 4, pmcsr & 0x7FFC);
            }
            return Some(Wake {
                from,
                reset: from == 3 && pmcsr & NO_SOFT_RESET == 0,
            });
        }
        ptr = ((header >> 8) & 0xFC) as u16;
    }
    None
}

/// Writes the BAR addresses enumeration found back, after a wake that
/// reset the function. Only the address bits are writable, so the type
/// bits come back by themselves.
pub fn restore_bars(cfg: &mut impl ConfigSpace, a: PciAddress, bars: &[Bar]) {
    for b in bars {
        let off = BAR0 + 4 * b.index as u16;
        cfg.write32(a, off, b.address as u32);
        if b.kind == BarKind::Memory64 {
            cfg.write32(a, off + 4, (b.address >> 32) as u32);
        }
    }
}
````

Replace:

````rust
static DEVICES: spin::Once<Vec<PciDevice>> = spin::Once::new();

````

with:

````rust
static DEVICES: spin::Once<Vec<PciDevice>> = spin::Once::new();
/// The ECAM windows `init` mapped: (physical base of bus 0, first bus, last bus).
static WINDOWS: spin::Once<Vec<(u64, u8, u8)>> = spin::Once::new();

````

Replace:

````rust
    let mut found = Vec::new();
    // A window whose bus range is backwards is firmware garbage.
````

with:

````rust
    let mut found = Vec::new();
    let mut mapped = Vec::new();
    // A window whose bus range is backwards is firmware garbage.
````

Replace:

````rust
        found.extend(enumerate(&mut cfg, r.start_bus, r.end_bus));
    }
    if found.is_empty() {
        return Err(PciError::NoDevices);
    }
    Ok(DEVICES.call_once(|| found))
}
````

with:

````rust
        found.extend(enumerate(&mut cfg, r.start_bus, r.end_bus));
        mapped.push((r.base, r.start_bus, r.end_bus));
    }
    WINDOWS.call_once(|| mapped);
    if found.is_empty() {
        return Err(PciError::NoDevices);
    }
    Ok(DEVICES.call_once(|| found))
}

/// Readies `d` for its driver: power state D0 (waiting 10 ms if it had to
/// change, and restoring its BARs if that reset it), then memory decoding
/// and bus mastering. Returns what the wake did, if the function has power
/// management.
pub fn enable_device(d: &PciDevice) -> Option<Wake> {
    let a = d.address;
    let (base, _, _) = *WINDOWS
        .get()?
        .iter()
        .find(|&&(_, start, end)| (start..=end).contains(&a.bus))?;
    let mut cfg = Ecam {
        base: boot_info::PHYS_OFFSET + base,
    };
    let wake = wake_to_d0(&mut cfg, a);
    if let Some(w) = wake
        && w.from != 0
    {
        crate::timer::sleep(core::time::Duration::from_millis(10));
        if w.reset {
            restore_bars(&mut cfg, a, &d.bars);
        }
    }
    enable_memory_and_bus_master(&mut cfg, a);
    wake
}
````

- [ ] **Step 9: Change `kernel/src/timer.rs`**

In `kernel/src/timer.rs`, replace:

````rust

/// TSC cycles in `d`, rounded up.
````

with:

````rust

/// The time `cycles` TSC cycles take at `tsc_hz`.
pub fn cycles_to_duration(cycles: u64, tsc_hz: u64) -> Duration {
    Duration::from_nanos((cycles as u128 * 1_000_000_000 / tsc_hz as u128) as u64)
}

/// Time since the CPU started, from the TSC, with sub-microsecond
/// resolution; `None` until `init` has found the TSC's frequency. Unlike
/// `uptime` it does not depend on the timer interrupt, so timeouts still
/// expire if the LAPIC timer could not be started.
pub fn tsc_time() -> Option<Duration> {
    let hz = tsc_hz();
    (hz != 0).then(|| cycles_to_duration(rdtsc(), hz))
}

/// TSC cycles in `d`, rounded up.
````

- [ ] **Step 10: Create `kernel/src/usb.rs`**

Create `kernel/src/usb.rs`:

````rust
//! The USB stack in the kernel (spec §6): the `Hal` it runs on.

use crate::mm::{self, paging::Cache};
use crate::{klogln, timer};
use core::fmt;
use core::time::Duration;
use usb::{DmaBuf, Hal};

/// Device registers through `mm::map_mmio` (uncached), DMA memory from the
/// frame allocator, the TSC for time and the kernel log.
#[derive(Clone, Copy, Debug, Default)]
pub struct KernelHal;

impl Hal for KernelHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        match mm::map_mmio(phys, len as u64, Cache::Uncached) {
            Ok(p) => Some(p as usize),
            Err(e) => {
                klogln!("usb: cannot map {len:#x} bytes at {phys:#x}: {e}");
                None
            }
        }
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        // SAFETY: the caller keeps `addr` inside a range `map_mmio` mapped.
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        // SAFETY: as in `read32`.
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        let (virt, phys) = mm::alloc_dma(size, align)?;
        // SAFETY: fresh frames of at least `size` bytes, owned by this
        // buffer until `free_dma`.
        Some(unsafe { DmaBuf::new(virt, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        mm::free_dma(buf.phys(), buf.size());
    }

    fn now(&self) -> Duration {
        timer::tsc_time().unwrap_or_else(timer::uptime)
    }

    fn sleep(&self, d: Duration) {
        timer::sleep(d);
    }

    fn log(&self, args: fmt::Arguments) {
        klogln!("{args}");
    }
}
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 100 tests.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add Cargo.lock kernel
git commit -m "kernel: DMA memory, PCI power-up and the USB Hal"
````


### Task 17: Key presses become terminal input

Spec §7.2: key events become the bytes a terminal sends, so the shell's line editor cannot tell the keyboard from a serial line. Characters are ASCII; Ctrl with a letter (or `[ \ ] ^ _ ?`) is its control code, so Ctrl-C is 0x03; Enter is CR, Backspace DEL, Tab, Escape; the arrows, Home and End are `ESC [ A`…`D`, `ESC [ H`, `ESC [ F`, and Insert, Delete, Page Up and Page Down `ESC [ 2~`, `3~`, `5~`, `6~`, as a Linux terminal sends them. Releases, lock keys and function keys send nothing; Ctrl with a digit is the digit, as on a Linux console.

**Files:**
- Modify: `kernel/src/input.rs`

**Interfaces:**
- Consumes: `usb::hid::{Key, KeyEvent}` (Tasks 13–14), `InputQueue` (Task 1).
- Produces: `InputQueue::push_key(&mut self, &KeyEvent)`.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::*;

````

with:

````rust
    use super::*;
    use usb::hid::Modifiers;

````

Replace:

````rust
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

with:

````rust
    }

    fn press(key: Key, ctrl: bool) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers {
                ctrl,
                ..Modifiers::default()
            },
            pressed: true,
        }
    }

    fn sent(key: Key, ctrl: bool) -> Vec<u8> {
        let mut q = InputQueue::new();
        q.push_key(&press(key, ctrl));
        drain(&mut q)
    }

    #[test]
    fn characters_and_control_codes() {
        assert_eq!(sent(Key::Char(b'a'), false), b"a");
        assert_eq!(sent(Key::Char(b'~'), false), b"~");
        assert_eq!(sent(Key::Char(b' '), false), b" ");
        assert_eq!(sent(Key::Char(b'c'), true), b"\x03");
        assert_eq!(sent(Key::Char(b'C'), true), b"\x03");
        assert_eq!(sent(Key::Char(b'a'), true), b"\x01");
        assert_eq!(sent(Key::Char(b'l'), true), b"\x0c");
        assert_eq!(sent(Key::Char(b'['), true), b"\x1b");
        assert_eq!(sent(Key::Char(b'?'), true), b"\x7f");
        // Ctrl with a digit is the digit, as on a Linux console.
        assert_eq!(sent(Key::Char(b'1'), true), b"1");
        // Ctrl-@ and Ctrl-space would be NUL, which the shell has no use for.
        assert_eq!(sent(Key::Char(b'@'), true), b"@");
    }

    #[test]
    fn editing_keys_send_terminal_sequences() {
        assert_eq!(sent(Key::Enter, false), b"\r");
        assert_eq!(sent(Key::Backspace, false), b"\x7f");
        assert_eq!(sent(Key::Tab, false), b"\t");
        assert_eq!(sent(Key::Escape, false), b"\x1b");
        assert_eq!(sent(Key::Up, false), b"\x1b[A");
        assert_eq!(sent(Key::Down, false), b"\x1b[B");
        assert_eq!(sent(Key::Right, false), b"\x1b[C");
        assert_eq!(sent(Key::Left, false), b"\x1b[D");
        assert_eq!(sent(Key::Home, false), b"\x1b[H");
        assert_eq!(sent(Key::End, false), b"\x1b[F");
        assert_eq!(sent(Key::Delete, false), b"\x1b[3~");
        assert_eq!(sent(Key::PageDown, true), b"\x1b[6~");
    }

    #[test]
    fn releases_and_keys_without_a_meaning_send_nothing() {
        let mut q = InputQueue::new();
        let mut release = press(Key::Char(b'a'), false);
        release.pressed = false;
        q.push_key(&release);
        for key in [Key::CapsLock, Key::F(1), Key::NumLock, Key::Menu] {
            q.push_key(&press(key, false));
        }
        assert!(q.is_empty());
    }

    #[test]
    fn an_interrupt_drops_what_was_typed_before_it() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find type `Key` in this scope ``; `` cannot find type `KeyEvent` in this scope ``.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it.

use alloc::collections::VecDeque;

````

with:

````rust
//! COM1 in QEMU) wait here until the shell reads them. Typing ahead while a
//! command runs is kept, as on a Linux terminal; a Ctrl-C drops it. Key
//! events become the bytes a terminal sends, so the shell cannot tell the
//! keyboard from a serial line.

use alloc::collections::VecDeque;
use usb::hid::{Key, KeyEvent};

````

Replace:

````rust
            None => false,
        }
````

with:

````rust
            None => false,
        }
    }
}

/// The escape sequence an editing key sends, as a Linux terminal does.
fn sequence(key: Key) -> Option<&'static [u8]> {
    Some(match key {
        Key::Up => b"\x1b[A",
        Key::Down => b"\x1b[B",
        Key::Right => b"\x1b[C",
        Key::Left => b"\x1b[D",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::Insert => b"\x1b[2~",
        Key::Delete => b"\x1b[3~",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        _ => return None,
    })
}

/// The single byte a key press sends, if it sends one: characters as
/// ASCII, Ctrl with a letter (or `[ \\ ] ^ _ ?`) as its control code, Enter
/// as CR, Backspace as DEL.
fn byte(e: &KeyEvent) -> Option<u8> {
    match e.key {
        Key::Char(c) if e.modifiers.ctrl => Some(match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'[' | b'\\' | b']' | b'^' | b'_' => c & 0x1F,
            b'?' => 0x7F,
            // Ctrl with a digit is the digit, as on a Linux console.
            _ => c,
        }),
        Key::Char(c) => Some(c),
        Key::Enter => Some(b'\r'),
        Key::Tab => Some(b'\t'),
        Key::Backspace => Some(0x7F),
        Key::Escape => Some(0x1B),
        _ => None,
    }
}

impl InputQueue {
    /// Adds what a key press sends (spec §7.2). Releases, lock keys and
    /// function keys send nothing; Alt has no function yet (spec §6.3).
    pub fn push_key(&mut self, e: &KeyEvent) {
        if !e.pressed {
            return;
        }
        if let Some(seq) = sequence(e.key) {
            self.push(seq);
        } else if let Some(b) = byte(e) {
            self.push(&[b]);
        }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 103 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add kernel
git commit -m "kernel: key presses become terminal input"
````


### Task 18: The e2e key step

Spec §9.3's `key` step: text typed on QEMU's USB keyboard through QMP `send-key`, which goes through `qemu-xhci` and the HID driver, unlike `send`, which types over serial. `keys.rs` turns the text into presses on the US layout (Shift for capitals and symbols); `{name}` is a key without a character (`{up}`, `{backspace}`, `{caps_lock}`, `{ctrl-c}` …), and Enter follows, as with `send`. Each press is held for 30 ms: the guest polls the keyboard every 8 ms, and the repeat delay is 500 ms. QEMU queues the presses and plays them in order, so the step waits until it has played them all (`typing_time`) before the next step runs; otherwise its Enter could land in the middle of a following `send`. Unknown keys and characters no US key types are errors when the scenario is parsed, not when it runs.

**Files:**
- Modify: `xtask/src/e2e.rs`
- Create: `xtask/src/keys.rs`
- Modify: `xtask/src/main.rs`

**Interfaces:**
- Consumes: `qmp::Qmp::execute` (plan 1).
- Produces: `xtask::keys::presses(&str) -> Result<Vec<Vec<&'static str>>>`; `e2e::typing_time(presses) -> Duration`; the scenario step `key <text>` (`Step::Key`).

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_alive_step() {
````

with:

````rust
    #[test]
    fn parses_key_steps() {
        let s = parse_scenario("x", "key echo {up}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Key("echo {up}".into()))]);
        assert!(parse_scenario("x", "key {bogus}").is_err());
    }

    #[test]
    fn a_key_step_waits_until_qemu_has_played_it() {
        // "ls" and Enter: three presses of 30 ms, then the settling time.
        assert_eq!(typing_time(3), Duration::from_millis(190));
    }

    #[test]
    fn parses_alive_step() {
````

- [ ] **Step 2: Write the failing tests for `xtask/src/keys.rs`**

Create `xtask/src/keys.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_become_keys_with_shift_where_needed() {
        assert_eq!(
            presses("echo Hi!").unwrap(),
            [
                vec!["e"],
                vec!["c"],
                vec!["h"],
                vec!["o"],
                vec!["spc"],
                vec!["shift", "h"],
                vec!["i"],
                vec!["shift", "1"],
                vec!["ret"],
            ]
        );
        assert_eq!(presses("").unwrap(), [vec!["ret"]]);
        let all = "abcxyzABCXYZ0123456789 -_=+[]\\|;:'\"`~,<.>/?!@#$%^&*()";
        assert_eq!(presses(all).unwrap().len(), all.len() + 1);
        assert_eq!(presses(")").unwrap()[0], ["shift", "0"]);
        assert_eq!(presses("|").unwrap()[0], ["shift", "backslash"]);
    }

    #[test]
    fn braces_name_keys_without_characters() {
        assert_eq!(
            presses("ls{backspace}{up}{caps_lock}{ctrl-c}").unwrap(),
            [
                vec!["l"],
                vec!["s"],
                vec!["backspace"],
                vec!["up"],
                vec!["caps_lock"],
                vec!["ctrl", "c"],
                vec!["ret"],
            ]
        );
    }

    #[test]
    fn unknown_keys_and_characters_are_errors() {
        assert!(presses("{nope}").is_err());
        assert!(presses("{ctrl-}").is_err());
        assert!(presses("{ctrl-ab}").is_err());
        assert!(presses("{up").is_err());
        assert!(presses("é").is_err());
        assert!(presses("\t").is_err());
    }
}
````

- [ ] **Step 3: Declare the new module in `xtask/src/main.rs`**

In `xtask/src/main.rs`, replace:

````rust
mod image;
mod qemu;
````

with:

````rust
mod image;
mod keys;
mod qemu;
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask`

Expected: FAIL: compile errors such as `` cannot find function `typing_time` in this scope ``; `` cannot find function `presses` in this scope ``.

- [ ] **Step 5: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
//! send <text>                      (types <text> + Enter over serial)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

with:

````rust
//! send <text>                      (types <text> + Enter over serial)
//! key <text>                       (types <text> + Enter on the USB keyboard,
//!                                   QMP send-key; {up}, {ctrl-c}: see keys.rs)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

Replace:

````rust
use crate::image::{self, Layout, esp_write, set_cmdline};
use crate::qemu::{self, Qemu};
````

with:

````rust
use crate::image::{self, Layout, esp_write, set_cmdline};
use crate::keys;
use crate::qemu::{self, Qemu};
````

Replace:

````rust
pub const DEFAULT_CMDLINE: &str = "test=1";

````

with:

````rust
pub const DEFAULT_CMDLINE: &str = "test=1";
/// How long each `key` press is held, in milliseconds: long enough for
/// the guest to poll the keyboard (every 8 ms), far below the 500 ms
/// repeat delay.
const KEY_HOLD_MS: u64 = 30;
/// Time for the guest to see the last release after QEMU has played it.
const KEY_SETTLE_MS: u64 = 100;

/// How long QEMU takes to play `presses` presses: it queues them and holds
/// each for `KEY_HOLD_MS`. The `key` step waits that long, so its Enter
/// cannot land in the middle of what a following `send` types.
pub fn typing_time(presses: usize) -> Duration {
    Duration::from_millis(KEY_HOLD_MS * presses as u64 + KEY_SETTLE_MS)
}

````

Replace:

````rust
    Send(String),
    ScreenshotNonblank,
````

with:

````rust
    Send(String),
    /// Text typed on the emulated USB keyboard.
    Key(String),
    ScreenshotNonblank,
````

Replace:

````rust
            "send" => Step::Send(rest.to_string()),
            "screenshot-nonblank" => Step::ScreenshotNonblank,
````

with:

````rust
            "send" => Step::Send(rest.to_string()),
            "key" => {
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
            }
            "screenshot-nonblank" => Step::ScreenshotNonblank,
````

Replace:

````rust
            r.stdin.flush()?;
        }
````

with:

````rust
            r.stdin.flush()?;
        }
        Step::Key(text) => {
            let presses = keys::presses(text)?;
            for press in &presses {
                let keys: Vec<_> = press
                    .iter()
                    .map(|k| serde_json::json!({ "type": "qcode", "data": k }))
                    .collect();
                r.qmp.execute(
                    "send-key",
                    serde_json::json!({ "keys": keys, "hold-time": KEY_HOLD_MS }),
                )?;
            }
            std::thread::sleep(typing_time(presses.len()));
        }
````

- [ ] **Step 6: Implement `xtask/src/keys.rs`**

Insert this at the top of `xtask/src/keys.rs`, above `#[cfg(test)]`:

````rust
//! Text for the e2e `key` step, as QMP `send-key` presses: each press is the
//! QEMU key codes (qcodes) held down together, Shift plus a letter for a
//! capital. `{name}` is a key without a character: `{up}`, `{down}`,
//! `{left}`, `{right}`, `{home}`, `{end}`, `{delete}`, `{backspace}`,
//! `{tab}`, `{esc}`, `{ret}`, `{caps_lock}` and `{ctrl-<letter>}`. Enter
//! follows the text, as with `send`.

use anyhow::{Result, bail};

const LETTERS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
/// Characters on the US layout that are not letters or digits: the
/// character, its key, and whether Shift is needed.
const SYMBOLS: [(char, &str, bool); 32] = [
    (' ', "spc", false),
    ('-', "minus", false),
    ('_', "minus", true),
    ('=', "equal", false),
    ('+', "equal", true),
    ('[', "bracket_left", false),
    ('{', "bracket_left", true),
    (']', "bracket_right", false),
    ('}', "bracket_right", true),
    ('\\', "backslash", false),
    ('|', "backslash", true),
    (';', "semicolon", false),
    (':', "semicolon", true),
    ('\'', "apostrophe", false),
    ('"', "apostrophe", true),
    ('`', "grave_accent", false),
    ('~', "grave_accent", true),
    (',', "comma", false),
    ('<', "comma", true),
    ('.', "dot", false),
    ('>', "dot", true),
    ('/', "slash", false),
    ('?', "slash", true),
    ('!', "1", true),
    ('@', "2", true),
    ('#', "3", true),
    ('$', "4", true),
    ('%', "5", true),
    ('^', "6", true),
    ('&', "7", true),
    ('*', "8", true),
    ('(', "9", true),
];

/// The key for `c` and whether it needs Shift.
fn char_key(c: char) -> Option<(&'static str, bool)> {
    match c {
        'a'..='z' => Some((LETTERS[c as usize - 'a' as usize], false)),
        'A'..='Z' => Some((LETTERS[c as usize - 'A' as usize], true)),
        '0'..='9' => Some((DIGITS[c as usize - '0' as usize], false)),
        ')' => Some(("0", true)),
        _ => SYMBOLS
            .iter()
            .find(|s| s.0 == c)
            .map(|&(_, key, shift)| (key, shift)),
    }
}

/// Keys named in braces; the names are QEMU's qcodes.
const NAMED: [&str; 12] = [
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "delete",
    "backspace",
    "tab",
    "esc",
    "ret",
    "caps_lock",
];

fn special(name: &str) -> Option<Vec<&'static str>> {
    if let Some(&key) = NAMED.iter().find(|&&k| k == name) {
        return Some(vec![key]);
    }
    let mut letter = name.strip_prefix("ctrl-")?.chars();
    match (letter.next(), letter.next()) {
        (Some(c @ 'a'..='z'), None) => Some(vec!["ctrl", LETTERS[c as usize - 'a' as usize]]),
        _ => None,
    }
}

/// The presses that type `text` and then Enter.
pub fn presses(text: &str) -> Result<Vec<Vec<&'static str>>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            let Some(end) = rest.find('}') else {
                bail!("unterminated {{ in key text {text:?}");
            };
            let name = &rest[1..end];
            match special(name) {
                Some(keys) => out.push(keys),
                None => bail!("unknown key {{{name}}} in {text:?}"),
            }
            rest = &rest[end + 1..];
            continue;
        }
        match char_key(c) {
            Some((key, true)) => out.push(vec!["shift", key]),
            Some((key, false)) => out.push(vec![key]),
            None => bail!("no key types {c:?} on a US keyboard"),
        }
        rest = &rest[c.len_utf8()..];
    }
    out.push(vec!["ret"]);
    Ok(out)
}

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p xtask`

Expected: PASS: 39 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add xtask
git commit -m "xtask: the e2e key step types on the USB keyboard"
````


### Task 19: USB at startup and typing at the shell

Startup step 8 (spec §4.4) and the keyboard at the shell. `usb::init` starts every xHCI controller PCI found (decision 3: the NUC has two, and the Thunderbolt one enumerates first): it readies the PCI function, starts the controller and sets up the devices on its ports, printing one line per controller and per device, because a photo of the screen is how the NUC is debugged. `[ ok ] usb` needs one working controller; one that fails gets its own line and is skipped. `[ ok ] keyboard` counts the boot keyboards; without one it is `[FAIL] keyboard: no USB keyboard found` and the shell still reads COM1 (spec §10). Without a TSC frequency there is no clock for timeouts, and USB is not started. The console polls the controllers for key presses whenever it looks for input, and runs their blocking work (hot-plug, LEDs) only while idle, before sleeping until the next tick. The new cmdline word `debug=usb` sends the USB log to the screen as well, for a NUC whose keyboard does not work (no serial port, no `dmesg`). `DmaBuf` becomes `Send`, because the controllers live in a static. The `keyboard` scenario types with `key`, and `docs/hardware-test.md` gains NUC check 2.

**Files:**
- Modify: `README.md`
- Modify: `crates/usb/src/hal.rs`
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/cmdline.rs`
- Modify: `kernel/src/lib.rs`
- Modify: `kernel/src/session.rs`
- Modify: `kernel/src/usb.rs`
- Modify: `tests/e2e/boot.txt`
- Create: `tests/e2e/keyboard.txt`

**Interfaces:**
- Consumes: Tasks 1, 2 and 15–18.
- Produces: `relay_kernel::usb::{init(verbose: bool), poll(&mut InputQueue), service(), usb_summary(controllers, devices) -> String}`; `Cmdline::debug_usb` (`debug=usb`); `unsafe impl Send for usb::DmaBuf`; startup lines `usb: <pci> xHCI …`, `usb: <pci> port N: …`, `[ ok ] usb: …`, `[ ok ] keyboard: …`; scenario `tests/e2e/keyboard.txt`; NUC check 2.

- [ ] **Step 1: Add the failing tests to `crates/usb/src/hal.rs`**

In `crates/usb/src/hal.rs`, replace:

````rust

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn accesses_past_the_end_panic() {
````

with:

````rust

    #[test]
    fn buffers_can_move_to_a_static() {
        fn send<T: Send>() {}
        send::<DmaBuf>();
    }

    #[test]
    #[should_panic(expected = "beyond 64")]
    fn accesses_past_the_end_panic() {
````

- [ ] **Step 2: Add the failing tests to `kernel/src/cmdline.rs`**

In `kernel/src/cmdline.rs`, replace:

````rust
    #[test]
    fn unknown_values_are_ignored() {
        let c = Cmdline::parse("test=0 panic=bogus foo tsc=bogus");
        assert_eq!(c, Cmdline::default());
````

with:

````rust
    #[test]
    fn usb_debugging() {
        assert!(Cmdline::parse("video=1920x1080 debug=usb").debug_usb);
        assert!(!Cmdline::parse("debug=all").debug_usb);
    }

    #[test]
    fn unknown_values_are_ignored() {
        let c = Cmdline::parse("test=0 panic=bogus foo tsc=bogus debug=bogus");
        assert_eq!(c, Cmdline::default());
````

- [ ] **Step 3: Add the failing tests to `kernel/src/usb.rs`**

In `kernel/src/usb.rs`, replace:

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

    #[test]
    fn the_status_line_counts_controllers_and_devices() {
        assert_eq!(usb_summary(1, 2), "usb: 1 controller, 2 devices");
        assert_eq!(usb_summary(2, 1), "usb: 2 controllers, 1 device");
        assert_eq!(usb_summary(1, 0), "usb: 1 controller, 0 devices");
    }
}
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/boot.txt`**

In `tests/e2e/boot.txt`, replace:

````text
expect \[ ok \] pci: \d+ devices on bus 00, xHCI at 00:02\.0
expect \[FAIL\] mount /: no storage driver yet
````

with:

````text
expect \[ ok \] pci: \d+ devices on bus 00, xHCI at 00:02\.0
expect usb: 00:02\.0 xHCI 1\.00, 8 ports \(4 USB 2, 4 USB 3\), 32-byte contexts, 0 scratchpads
expect usb: 00:02\.0 port 2: 46f4:0001 SuperSpeed, not claimed
expect usb: 00:02\.0 port 5: 0627:0001 high-speed, keyboard
expect \[ ok \] usb: 1 controller, 2 devices
expect \[ ok \] keyboard: 1 keyboard
expect \[FAIL\] mount /: no storage driver yet
````

- [ ] **Step 5: Add the scenario `tests/e2e/keyboard.txt`**

In QEMU 8.2 the keyboard and the stick sit on ports 5 and 2: `qemu-xhci` numbers its USB 3 ports first, and `usb-storage` connects at SuperSpeed.

Create `tests/e2e/keyboard.txt`:

````text
# Typing on the USB keyboard (spec §9.3 #2): QMP send-key goes through
# qemu-xhci and the HID boot keyboard driver to the shell.
timeout 30
expect \[ ok \] keyboard: 1 keyboard
expect root@relay:/# $
key echo hello
expect \nhello\n
# Shift for capitals and symbols.
key echo Hello, World!
expect \nHello, World!\n
# Caps Lock: letters change, digits do not, Shift inverts it.
key echo {caps_lock}abc1{caps_lock}def
expect \nABC1def\n
key echo {caps_lock}aBc{caps_lock}
expect \nAbC\n
# Backspace and the cursor keys.
key echo abcx{backspace}d
expect \nabcd\n
key echo ac{left}b{end}d
expect \nabcd\n
# History: the up arrow brings back earlier lines (a repeated line is kept
# once).
key {up}
expect \nabcd\n
key {up}{up}{up}
expect \nABC1def\n
# Ctrl-C drops the line being typed.
key echo nope{ctrl-c}
expect \^C\n
# The keyboard and the serial line type into the same shell.
send echo serial
expect \nserial\n
key echo keyboard
expect \nkeyboard\n
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` cannot find function `usb_summary` in this scope ``; `` no field `debug_usb` on type `cmdline::Cmdline` ``.

Run: `cargo test -p usb`

Expected: FAIL: compile errors such as `` `std::ptr::NonNull<u8>` cannot be sent between threads safely ``.

- [ ] **Step 7: Change `README.md`**

In `README.md`, replace:

````markdown
| `crates/shell` | Line editor, parser and built-in commands |
| `xtask/` | Build, image, QEMU, test and flash tool |
````

with:

````markdown
| `crates/shell` | Line editor, parser and built-in commands |
| `crates/usb` | xHCI host controller driver and HID boot keyboard, over a `Hal` trait |
| `xtask/` | Build, image, QEMU, test and flash tool |
````

- [ ] **Step 8: Change `crates/usb/src/hal.rs`**

In `crates/usb/src/hal.rs`, replace:

````rust
    size: usize,
}

impl DmaBuf {
````

with:

````rust
    size: usize,
}

// SAFETY: a `DmaBuf` owns its memory exclusively, as a `Box` does; it
// can move to another context (the kernel keeps its controllers in a
// static) without anything else keeping a pointer into it.
unsafe impl Send for DmaBuf {}

impl DmaBuf {
````

- [ ] **Step 9: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
     roughly 15000.
   - `relay: early boot complete`, followed by a solid block cursor.
4. Panic screen: in Mint run `cargo xtask flash --kernel --cmdline panic=pagefault`,
````

with:

````markdown
     roughly 15000.
   - Since plan 4 more lines follow (checks 1b and 2), and the last one is
     the shell prompt `root@relay:/# ` with a solid block cursor.
4. Panic screen: in Mint run `cargo xtask flash --kernel --cmdline panic=pagefault`,
````

Replace:

````markdown
     `lspci | wc -l` in Mint also says 24.
   - `relay: early boot complete`
3. Photograph the screen, then restore: `cargo xtask flash --kernel`.

A `[FAIL]` line names the step and the reason; boot carries on after it
(except for memory, which stops the machine).

````

with:

````markdown
     `lspci | wc -l` in Mint also says 24.
3. Photograph the screen, then restore: `cargo xtask flash --kernel`.

A `[FAIL]` line names the step and the reason; boot carries on after it
(except for memory, which stops the machine).

## Check 2 — typing on the K120 (plan 4)

The K120 must be on the port it has in Mint (`lsusb -t`: bus 3 port 3), the
Unifying receiver on port 1 and the stick on a USB 3 port.

1. In Mint: `cargo xtask flash --kernel`.
2. Boot the stick. After the check 1b lines the screen shows:
   - one line for the Thunderbolt controller, `usb: 00:0d.0 xHCI …` if it
     starts or `usb: 00:0d.0: controller not responding` if the firmware
     left it powered off. Either is fine: nothing is plugged into it.
   - `usb: 00:14.0 xHCI 1.20, N ports (N USB 2, N USB 3), 64-byte contexts,
     N scratchpads`. Note N. 64-byte contexts and a nonzero scratchpad
     count are the paths QEMU cannot test.
   - one line per device, in port order:
     - `usb: 00:14.0 port 1: 046d:c534 full-speed, keyboard` (the receiver)
     - `usb: 00:14.0 port 3: 046d:c31c low-speed, keyboard` (the K120)
     - `usb: 00:14.0 port N: 0951:1666 SuperSpeed, not claimed` (the stick,
       on one of the USB 3 ports after the USB 2 ones; its driver comes
       with plan 5)

     The boot waits for them: at least 100 ms, and up to 1 s while a USB 3
     link is still training (`dmesg`: `ports settled after N ms`).
   - `[ ok ] usb: 2 controllers, 3 devices` (or 1 controller),
     `[ ok ] keyboard: 2 keyboards`, `[FAIL] mount /: no storage driver
     yet`, and the prompt `root@relay:/# `.
3. On the K120, type and check each result:
   - `echo hello`, Enter → `hello`.
   - `echo Hello, World!` with Shift → `Hello, World!`.
   - Caps Lock: the K120's Caps Lock light goes on; `echo abc` shows
     `ABC`; Caps Lock again turns the light off.
   - Backspace, ←/→, Home/End while editing a line; ↑/↓ for history.
   - Hold a letter: after half a second it repeats, about 30 times a
     second, and stops when released.
   - `echo nope` then Ctrl-C → `^C` and a fresh prompt.
   - `uname -a`, `date` (the current UTC time), `free`, `dmesg` (the
     `xhci 00:14.0:` lines of every step).
4. Unplug the K120, plug it back into the same port, and type `echo back`:
   it works again (hot-plug; `dmesg` shows `port 3: disconnected, PORTSC …`,
   then `port 3: connected` and the device set up again, possibly in the
   same slot number).
5. Photograph the screen after step 2 and after `dmesg`.

### If it fails

The NUC has no serial port, so a keyboard that does not work cannot run
`dmesg`. Boot with the USB log on screen instead: in Mint
`cargo xtask flash --kernel --cmdline debug=usb`, boot, and photograph the
`xhci 00:14.0:` lines (they scroll; take several photos). Restore with
`cargo xtask flash --kernel`.

| What you see | Likely cause | Next step |
|---|---|---|
| `usb: 00:14.0: timed out` | Handoff or reset did not finish | `debug=usb`: the handoff and reset lines name the register |
| `port 3: setup failed: …` | The K120's enumeration failed | `debug=usb`: the last `port 3` / `slot` line before the failure |
| `port 3: … keyboard not started: …` | The K120 refused the boot protocol | Note the reason; `debug=usb` shows the `hid:` lines |
| `[FAIL] keyboard: no USB keyboard found` and no `port 3` line | The K120 connected after the boot's wait, or not at all | Type anyway: a late keyboard is set up when it appears. If nothing works, `debug=usb`: a `port 3: connected` line means it appeared late, none means the port never saw it; the `ports settled` line shows the wait |
| Keys show up twice or not at all | Firmware still emulating a keyboard (handoff) | `debug=usb`: the `legacy support` line |

````

- [ ] **Step 10: Change `kernel/src/cmdline.rs`**

In `kernel/src/cmdline.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub check_timer: bool,
}
````

with:

````rust
    pub check_timer: bool,
    /// `debug=usb`: the USB stack's log also goes to the screen, for a
    /// machine whose keyboard does not work (no serial port, no `dmesg`).
    pub debug_usb: bool,
}
````

Replace:

````rust
                Some(("check", "timer")) => c.check_timer = true,
                Some(("panic", v)) => {
````

with:

````rust
                Some(("check", "timer")) => c.check_timer = true,
                Some(("debug", "usb")) => c.debug_usb = true,
                Some(("panic", v)) => {
````

- [ ] **Step 11: Change `kernel/src/lib.rs`**

In `kernel/src/lib.rs`, replace:

````rust
    }
    // There is no storage driver yet, so the shell starts on an empty,
````

with:

````rust
    }
    usb::init(cmdline.debug_usb);
    // There is no storage driver yet, so the shell starts on an empty,
````

- [ ] **Step 12: Change `kernel/src/session.rs`**

In `kernel/src/session.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console`, the clock, memory figures and kernel log as
//! `shell::System`, and `vfs::Env` for filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, rtc, serial};
use alloc::boxed::Box;
````

with:

````rust
//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console` (input from the USB keyboards and COM1), the clock,
//! memory figures and kernel log as `shell::System`, and `vfs::Env` for
//! filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, rtc, serial, usb};
use alloc::boxed::Box;
````

Replace:

````rust

/// The screen and serial for output; COM1 for input.
pub struct KernelConsole {
````

with:

````rust

/// The screen and serial for output; the USB keyboards and COM1 for input.
pub struct KernelConsole {
````

Replace:

````rust
    fn poll(&mut self) {
        for _ in 0..SERIAL_BURST {
````

with:

````rust
    fn poll(&mut self) {
        usb::poll(&mut self.input);
        for _ in 0..SERIAL_BURST {
````

Replace:

````rust
            }
            arch::wait_for_interrupt();
````

with:

````rust
            }
            // Nothing typed: time for the work that may wait (a keyboard
            // plugged in, the Caps Lock LED), then sleep until the next
            // tick.
            usb::service();
            arch::wait_for_interrupt();
````

- [ ] **Step 13: Change `kernel/src/usb.rs`**

Replace the whole of `kernel/src/usb.rs` with:

````rust
//! The USB stack in the kernel (spec §6): the `Hal` it runs on, startup
//! step 8 (every xHCI controller and the devices on its ports), and the
//! polling the console does for key presses and hot-plug.

use crate::input::InputQueue;
use crate::mm::{self, paging::Cache};
use crate::pci::{self, BarKind, PciDevice};
use crate::{console, klogln, kprintln, timer};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use spin::Mutex;
use usb::host::Host;
use usb::xhci::Xhci;
use usb::{DmaBuf, Hal, UsbError};

/// Every running controller with its drivers. The console polls them; the
/// storage driver will share them.
static HOSTS: Mutex<Vec<Host<KernelHal>>> = Mutex::new(Vec::new());

/// `debug=usb`: the USB log goes to the screen as well.
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Device registers through `mm::map_mmio` (uncached), DMA memory from the
/// frame allocator, the TSC for time and the kernel log.
#[derive(Clone, Copy, Debug, Default)]
pub struct KernelHal;

impl Hal for KernelHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        match mm::map_mmio(phys, len as u64, Cache::Uncached) {
            Ok(p) => Some(p as usize),
            Err(e) => {
                klogln!("usb: cannot map {len:#x} bytes at {phys:#x}: {e}");
                None
            }
        }
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        // SAFETY: the caller keeps `addr` inside a range `map_mmio` mapped.
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        // SAFETY: as in `read32`.
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        let (virt, phys) = mm::alloc_dma(size, align)?;
        // SAFETY: fresh frames of at least `size` bytes, owned by this
        // buffer until `free_dma`.
        Some(unsafe { DmaBuf::new(virt, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        mm::free_dma(buf.phys(), buf.size());
    }

    fn now(&self) -> Duration {
        timer::tsc_time().unwrap_or_else(timer::uptime)
    }

    fn sleep(&self, d: Duration) {
        timer::sleep(d);
    }

    fn log(&self, args: fmt::Arguments) {
        if VERBOSE.load(Ordering::Relaxed) {
            kprintln!("{args}");
        } else {
            klogln!("{args}");
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    alloc::format!("{n} {}", if n == 1 { one } else { many })
}

/// The `[ ok ] usb` step: controllers running and devices set up.
pub fn usb_summary(controllers: usize, devices: usize) -> String {
    alloc::format!(
        "usb: {}, {}",
        plural(controllers, "controller", "controllers"),
        plural(devices, "device", "devices")
    )
}

/// Startup step 8 (spec §4.4): starts every xHCI controller PCI found and
/// sets up the devices on its ports. One line per controller and device
/// goes to the screen (a photo of it is how the NUC is debugged), then the
/// `usb` and `keyboard` status lines. A controller that fails is skipped.
pub fn init(verbose: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
    if timer::tsc_hz() == 0 {
        console::fail("usb", format_args!("no timer"));
        console::fail("keyboard", format_args!("no USB"));
        return;
    }
    let mut hosts = Vec::new();
    let mut devices = 0;
    let mut error = None;
    for d in pci::devices().iter().filter(|d| d.is_xhci()) {
        let name = d.address.to_string();
        match start(d, &name) {
            Ok((host, found)) => {
                hosts.push(host);
                devices += found;
            }
            Err(e) => {
                kprintln!("usb: {name}: {e}");
                error = Some(e);
            }
        }
    }
    match (hosts.len(), error) {
        (0, None) => console::fail("usb", format_args!("no xHCI controller")),
        (0, Some(e)) => console::fail("usb", format_args!("{e}")),
        (n, _) => console::ok(format_args!("{}", usb_summary(n, devices))),
    }
    let keyboards: usize = hosts.iter().map(|h| h.keyboards()).sum();
    if keyboards == 0 {
        console::fail("keyboard", format_args!("no USB keyboard found"));
    } else {
        console::ok(format_args!(
            "keyboard: {}",
            plural(keyboards, "keyboard", "keyboards")
        ));
    }
    *HOSTS.lock() = hosts;
}

/// Starts one controller and the devices on its ports; returns it and how
/// many devices were set up.
fn start(d: &PciDevice, name: &str) -> Result<(Host<KernelHal>, usize), UsbError> {
    let bar = d
        .bars
        .iter()
        .find(|b| b.index == 0 && b.kind != BarKind::Io)
        .ok_or(UsbError::Unsupported("no memory BAR"))?;
    if let Some(w) = pci::enable_device(d)
        && w.from != 0
    {
        let reset = if w.reset { ", BARs restored" } else { "" };
        klogln!("usb: {name}: woken from D{}{reset}", w.from);
    }
    let xhci = Xhci::new(KernelHal, bar.address, bar.size as usize, name)?;
    kprintln!("usb: {name} {}", xhci.info());
    let mut host = Host::new(xhci);
    let attached = host.service();
    for a in &attached {
        kprintln!("usb: {name} {a}");
    }
    Ok((host, attached.iter().filter(|a| a.outcome.is_ok()).count()))
}

/// Moves key presses from every keyboard into `input`. Never waits.
pub fn poll(input: &mut InputQueue) {
    for host in HOSTS.lock().iter_mut() {
        host.poll();
        while let Some(e) = host.next_key() {
            input.push_key(&e);
        }
    }
}

/// Hot-plug and the keyboards' LED and recovery work. May wait while a new
/// device is set up, so only the console's idle loop calls it.
pub fn service() {
    for host in HOSTS.lock().iter_mut() {
        for a in host.service() {
            klogln!("usb: {} {a}", host.xhci().name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_counts_controllers_and_devices() {
        assert_eq!(usb_summary(1, 2), "usb: 1 controller, 2 devices");
        assert_eq!(usb_summary(2, 1), "usb: 2 controllers, 1 device");
        assert_eq!(usb_summary(1, 0), "usb: 1 controller, 0 devices");
    }
}
````

- [ ] **Step 14: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 105 tests.

Run: `cargo test -p usb`

Expected: PASS: 217 tests.

- [ ] **Step 15: Run the boot and keyboard scenarios**

Run: `cargo xtask test --e2e-only --scenario boot`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario keyboard`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add README.md crates docs kernel tests
git commit -m "kernel: USB at startup and typing at the shell on the USB keyboard"
````


### Finish PR 7

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 11 scenario(s) passed`.

````bash
git push -u origin plan4/usb-kernel
gh pr create --base main --head plan4/usb-kernel --title "Plan 4: The USB keyboard in the kernel" --body-file - <<'EOF'
## What

Milestone 1, plan 4, tasks 16–19: DMA memory, PCI power-up to D0 and the kernel `Hal`; key presses as terminal bytes; the e2e `key` step over QMP `send-key`; startup step 8 with every xHCI controller, the `usb` and `keyboard` status lines, polling and hot-plug from the console, `debug=usb`; the `keyboard` scenario; NUC check 2 in `docs/hardware-test.md`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by unit tests, the fake xHCI controller and QEMU scenarios

## Hardware

- [x] Needed: NUC check 2 (typing on the K120), run by the user with `cargo xtask flash --kernel` from this worktree; the result goes into `docs/hardware-test.md`'s results log
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks plan4/usb-kernel --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/plan4-usb-kernel
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
