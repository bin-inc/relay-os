# AGENTS.md

Context and working rules for AI coding agents in this repository. People
can read it too. `README.md` is the project's introduction and
`CONTRIBUTING.md` holds the commit and pull-request rules in full; this
file sums up only the rules an agent most often needs.

## The project

Relay OS is an operating system written from scratch in Rust for x86_64
UEFI machines. Its test machine is an Intel NUC 12 Pro booting from a USB
stick; QEMU with OVMF runs everything else. A later aim is an ARM
handheld, so the system-call ABI must stay free of x86 detail.

Status: milestones 1 to 3 are done, at version 0.4.0 (tag `v0.4.0`). The
kernel boots, drives xHCI (keyboard and USB storage), mounts an ext2 root
and runs programs in ring 3 from a read-only `/bin` (`system.img`).
Process 1 is the kernel's init, which runs `/bin/sh`; the shell has
pipes, background jobs, scripts with arguments and variables, and six
built-ins (`cd`, `exit`, `help`, `jobs`, `kill`, `wait`). Every other
command is a program. No later milestone has been planned yet; the next
expected direction is an aarch64 port.

## Where to read

| To learn | Read |
|---|---|
| Layout, host tools, every `cargo xtask` command | `README.md` |
| Commit and pull-request format | `CONTRIBUTING.md` |
| Milestone 1's design (boot, kernel core, USB, ext2, shell) | `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`, cited as "M1 §n" |
| Milestones 2 and 3's design (programs, ABI, pipes, jobs, scripts) | `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`; its §16 records every decision taken while planning |
| What each plan delivered | `docs/superpowers/plans/*-roadmap.md` and the plans beside them |
| The hardware check and its history | `docs/hardware-test.md` |
| The e2e scenario language | the module comment of `xtask/src/e2e.rs` |

Specs, roadmaps and plans are records of what was decided and done. Do
not rewrite them to match later code; a later decision goes into the
current spec's decision log, and a finished plan stays as it was
executed.

## Commands

```sh
cargo xtask ci                      # what every pull request must pass: lint, unit, every scenario
cargo xtask lint                    # cargo fmt --check, clippy with warnings as errors
cargo xtask unit                    # host unit tests, including the kernel's and loader's libraries
cargo xtask test --scenario pipes   # unit tests, then one QEMU scenario (tests/e2e/pipes.txt)
cargo xtask test --e2e-only --jobs 1  # every scenario, one at a time
cargo test -p shell                 # one host crate's tests
cargo test -p relay-kernel --lib    # the kernel's logic, on the host
cargo xtask qemu                    # boot the image in a window
```

A scenario's serial log (and any screenshot) stays in
`target/relay/e2e/<scenario>/`. `RELAY_QEMU_ACCEL=tcg` forces software
emulation. The NUC commands (`flash`, `verify-usb`, `setup-udev`) need the
test stick and the maintainer; do not run them unless asked.

## Rules the code keeps

- **Toolchain:** stable Rust pinned in `rust-toolchain.toml`; no nightly
  features. All assembly is inline (`asm!`, `naked_asm!`); no `.S` files.
- **Dependencies:** `kernel/` and `crates/*` may use only `x86_64`,
  `bitflags` and `spin` besides workspace crates (`boot/` also uses
  `uefi`); `crates/usb` has none at all. `xtask/` and dev-dependencies may
  use anything. The kernel has no dev-dependencies, and it does not
  depend on `shell` (an xtask test walks the resolved graph).
- **Architecture:** new architecture-specific code goes in
  `kernel/src/arch/` or `relay-rt`'s `arch` module. Some kernel modules
  (`power`, `timer`, `rtc`, `serial`, `mm`, `panic_screen`) still use
  `x86_64` directly; they are expected to move with the aarch64 port, so
  do not add more such code outside `arch/`. `crates/relay-abi` holds no
  architecture detail and keeps the same values (call numbers, error
  numbers, struct layouts) on every architecture.
- **Crates:** every host crate starts with
  `#![cfg_attr(not(test), no_std)]` (and `extern crate alloc;` where it
  allocates). A new host crate goes into both `members` and
  `default-members` in `Cargo.toml`; a user package goes into `members`
  only and into `USER_PACKAGES` in `xtask/src/config.rs`. Kernel logic
  goes in the kernel's library target so the host can test it.
- **Untrusted input:** devices, disks, firmware, program data and what a
  person types are untrusted. Nothing that follows from them may panic,
  index unchecked, overflow (the target profile has overflow checks),
  allocate without bound or loop forever. Errors are values; panics are
  for bugs. Nothing waits forever except the error screen and a program
  waiting for its own input. The kernel heap panics when it runs out, so
  bound every allocation sized from outside data.
- **Locks:** never hold one across a context switch (`no_lock_held` in
  `kernel/src/proc.rs` lists the locks it checks). Order: `PROCS` before
  `MEMORY`; the input queue before the USB hosts; the tee stack before the
  mount table. No console output (it may write the tees) and no pipe-end
  drop while `PROCS` is held.
- **Startup lines** are exactly `[ ok ] <step>` or `[FAIL] <step>:
  <reason>`; detail goes to the kernel log. The NUC's screen is 120×33
  cells and has no serial port, so a failure must be readable there.
- **Messages:** the shell and commands follow bash 5.2 (interactive where
  it differs) and GNU coreutils, word for word; `/bin/sh` calls itself
  `relay-sh`, `sh`'s own messages start `sh:`, init's `init:`. A refused
  option prints the first line of GNU's message without the `Try … --help`
  line. Unsupported shell syntax is `relay-sh: unsupported syntax: <what>`
  with status 2, never passed on as text. Expansion never splits words,
  a decided difference from bash.
- **Command functions** are written once in `crates/shell/src/commands/`
  and run unchanged in the shell's tests, in `host-shell` and as programs
  of `/bin`.
- **Breaking changes:** a change to the ABI (`crates/relay-abi`), the
  on-disk or `system.img` format or the loader → kernel hand-off
  (`crates/boot-info`) bumps what it must (`relay_abi::VERSION`, …) and is
  marked `!` with a `BREAKING CHANGE:` footer (`CONTRIBUTING.md`).

## Testing

- **Test first.** Write the test, watch it fail for the right reason, then
  write the code. A bug fix starts with a test that shows the bug.
- **Bound every loop in a test,** host or guest. Pipes, console reads and
  `wait` block: a test that uses them needs a bound or a timeout, so a
  broken build fails instead of hanging the machine.
- **Missing tools fail tests;** they never skip them. A test that needs a
  host tool needs the CI job that runs it to install the tool
  (`.github/workflows/ci.yml`), and CI has fewer tools than a workstation.
- **Compare with the real thing exactly:** bash 5.2 (in a pty for anything
  interactive), the host's GNU tools (`crates/shell/src/testing.rs` has
  helpers that run them), `readelf`, the host C library's `strerror`,
  Linux's error numbers. Test with non-ASCII text as well as what the
  keyboard can type.
- **The check scripts are tests.** `rootfs/root/checks/*.sh` run on the
  NUC and in the `checks` scenario, against the transcripts recorded in
  `xtask/fixtures/checks/` (`*.qemu.log`, `*.nuc.log`). A change to what
  they print changes those transcripts; the NUC transcripts come only off
  the stick, after a NUC run.
- **Output that changes ripples.** One more program in `/bin` changes
  `ls /bin` in the `system` scenario and the count on
  `[ ok ] system: N programs` in `kernel/src/system.rs`'s comment and
  `docs/hardware-test.md`; a new command also changes `help`, and a name
  that sorts first or last changes `check4.sh`'s `ls /bin` lines. The
  recorded transcripts change only when a script's expectations stop
  matching them. Run the full `cargo xtask ci` after any change to
  output.
- **e2e expectations:** an `expect` consumes its match, including a
  trailing newline or the prompt it ended at; the command echo follows the
  prompt (match `root@relay:~# <cmd>`); output printed while the shell
  waits at its prompt has no newline before it; kernel log lines
  (`pid N (…): killed: …`) can arrive between other lines. `key` and
  `type` cannot type `{`, which opens a key name (`{ctrl-c}`), so send it
  over serial (`send`).
- **Mutation checks:** for a guard, break it and see a test fail. A
  mutant that does not compile is no kill; a surviving mutant usually
  names a case no test reaches.
- **`pub` code with only test callers gets no dead-code warning,** while
  a private item nothing uses fails lint.

## How work is done here

Each milestone has a spec, a roadmap that splits it into plans, and one
plan per step, written just before it is executed so it builds on the
code that exists. Plans live in `docs/superpowers/plans/` and follow the
format of the latest one.

A plan is made in this order:

1. **Kickoff.** The maintainer states the plan's scope, the decisions
   still open and the deferred findings it must settle. Start by saying
   back how you understand the scope and the pull-request split, before
   writing anything.
2. **Spike** first when the risk is "does everything still pass after X"
   (a version bump, a new check script).
3. **Prototype.** Every task is done for real in a scratch clone of
   `origin/main` outside this repository, one commit per task in
   `CONTRIBUTING.md`'s format, on a base tagged `p0` with each task tagged
   `t1`…`tN`. Each guard gets a mutation check. Run the full
   `cargo xtask ci` at the end of each task that changes output.
4. **Independent review** of the prototype by a fresh, read-only reviewer
   on the most capable model, in its own clone. Every real finding
   becomes a task of its own, placed after the task it concerns, with a
   test that fails first.
5. **Generate the plan** from the prototype (each task's code, tests and
   commands taken from its commits), then **replay** it: a script applies
   the plan's own text to a fresh clone, runs every command and compares
   each task's tree with the prototype's. The plan's first pull request
   carries the plan and the spec's new decision-log item.
6. **Approval.** The maintainer reviews the plan and chooses how it is
   executed. So far: natively, task by task, with one whole-branch review
   on the most capable model at the end.
7. **Execute** in pull requests that are each green on their own: one
   branch and worktree per pull request, created from `origin/main`.
   Keep a ledger of each task's state, rulings and deferred findings
   outside the repository; deferred findings go into the roadmap's notes
   for the next plan.

Scratch work (prototype clones, review reports, ledgers, logs) never goes
into the repository or into a worktree's tracked files; the maintainer
says where it goes.

## Git, pull requests and releases

- Commits follow `CONTRIBUTING.md` (Conventional Commits,
  `type(scope): description`, imperative, at most 72 characters, at most
  two scopes). No attribution lines (`Co-authored-by:`, "Generated with
  …") in commits or pull-request bodies.
- Chain a commit after its check with `&&`, so a failed lint or test
  stops it. Write long commit messages to a file instead of nesting
  heredocs.
- `cargo xtask ci` passes before every push. The maintainer reviews and
  merges every pull request; never merge one yourself, and never poll CI
  in a loop.
- Later pull requests of a plan are prepared locally, stacked, and pushed
  only after their predecessor merges. The maintainer merges by squash or
  merge commit, so after a merge rebase only the next branch with
  `git rebase --onto origin/main <its predecessor's old tip>` (record the
  old tips first), and run `cargo xtask ci` again unless the tree is
  unchanged. Compare trees before deleting a branch.
- A pull request that needs a NUC check is opened as a draft and stays one
  until the maintainer reports the result. Its transcripts (copied off the
  stick, checked by `verify-usb`) and the results-log row in
  `docs/hardware-test.md` go into a commit of that same pull request.
- CodeQL scans every pull request: triage each new alert and ask the
  maintainer before dismissing one.
- Ask before creating any tag or GitHub release, and propose its text.
  A release bumps the workspace version (`chore(release): X.Y.Z`), which
  reaches `uname -a` in the `shell` scenario, in `check3-a.sh` and in the
  `uname` unit test (`crates/shell/src/commands/basic.rs`), the recorded
  transcripts' version lines, and `docs/hardware-test.md`'s checks 1
  and 1b.

## Pitfalls

- `sed` over `docs/hardware-test.md` can rewrite a row of its results log,
  which is history. Check `git diff` after every docs edit, and reflow a
  paragraph you change.
- The git stash is shared by every worktree. Never use a bare
  `git stash`; use a work-in-progress commit or a patch file.
- `pkill -f <pattern>` can match the shell that runs it; stop background
  processes by PID. A test binary left behind by a timeout can spin a core
  for hours: after a mutant times out, check nothing it started survives
  (`pgrep -fa relay_kernel-`).
- Long builds heat the host. When a scenario fails oddly, check the
  temperature and run scenarios one at a time (`--jobs 1`), not beside
  another build.
- Rebasing a prototype: `cargo fmt` reflows code, so later edits must
  match the formatted text, and a fix inserted early in the history can
  change a later task's test; rerun the tip's tests after inserting one.

## Hardware

The NUC 12 Pro (`NUC12WSHi7`, Core i7-1260P: SMEP and SMAP on,
CR4.FSGSBASE clear; 16 GB) shows the console at 1920×1200 in 120×33 cells.
Its PCH xHCI is `00:14.0`, with the K120 keyboard on port 3 and the
Kingston test stick on port 15. `flash --full` makes the stick's ext2 root
exactly 2 GiB; `flash --kernel` replaces the loader, kernel, `system.img`
and command line and keeps the files, after which `verify-usb` fails any
transcript older than the new `system.img`. Only the maintainer runs the
NUC; `docs/hardware-test.md` is the checklist.
