# Milestone 4 · Plan 3: `test` and `[` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/bin/test` and `/bin/[` answer conditions as GNU coreutils 9.4's do, word for word and status for status, so that scripts and the prompt can write `if [ "$1" = go ]`, `while test ! -f stop` and `[ -w /bin/ls ]`; `grep -q` says only whether a line matches; and every QEMU scenario waits for the prompt before it types, so that a line typed ahead can no longer land inside the output an `expect` waits for. The spec's §15 item 3 lands with this plan. It ends with `cargo xtask ci` green, the new scenario `test_cmd`, 49 scenarios in all, and `system: 44 programs, ABI 3`.

**Architecture:** In the shell crate, with small changes to `vfs` (test-only setters), `relay-rt` (`-t` from `fstat`), `relay-utils` (one program) and xtask (the program packed under two names; the scenario parser). `crates/shell/src/commands/test.rs` holds one evaluator under two names (`test`, `bracket`), GNU's `test.c` rule for rule but with an explicit stack of open parentheses instead of recursion; files are answered from `stat` as for root, `-t` through `System::is_terminal`. Cargo refuses `[` as a binary's name, so xtask packs the `test` ELF as `test` and `[`, and the program picks its mode by its argument 0 (`commands::named`). The unit tests compare every case with the host's `/usr/bin/test` and `/usr/bin/[`, the file operators through `unshare -r` so that GNU answers as root. The scenario parser holds every input to an `expect` of the prompt, unless it is marked `-ahead`.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; GNU coreutils 9.4 and GNU grep 3.11 on the host and on CI's ubuntu-24.04 (`/usr/bin/test`, `/usr/bin/[`, `/usr/bin/grep`); `unshare` (util-linux) with unprivileged user namespaces; bash 5.2.21 for the corpus; QEMU 8.2 under KVM with `-cpu max`.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§6, §10, §11.1–§11.3, §11.5, §12, §15 items 1–3)
**Roadmap:** `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md` — this is plan 3 of 4 of milestone 4.

## In brief

- **Size.** 18 tasks in four code pull requests, plus this plan as PR 1: PR 2 `test` and `[` in the shell (`/bin` unchanged); PR 3 the two programs in `/bin`, everything that lists `/bin`, the scenario `test_cmd` and the corpus; PR 4 `grep -q` and two of plan 2's deferred minors; PR 5 the pacing of every scenario. No NUC check: plan 4's `check6.sh` runs `test` there, and `check4.nuc.log` gets its two new `ls /bin` lines by hand until then (decision 10).
- **GNU's grammar in full** (decisions 2–6), probed against `/usr/bin/test` and `/usr/bin/[` for every case (`tmp/m4p3/progress.md`, `tmp/m4p3/probes/`) and read from coreutils 9.4's `src/test.c`: the rules by argument count, `-a`/`-o`/`!`/`(`, GNU's messages and quoting, integers of any length, `-l`; no `<` or `>`, which GNU's program lacks (the user's choice); `[ --help` refused; no recursion (GNU nests 30,000 levels). A fuzz of about 18,000 random argument lists, and the review's of 94,000, found no difference.
- **Files as for root** (decisions 7, 8, 9): Relay OS runs every program as root, so the tests run GNU through `unshare -r` (CI's `unit` job allows unprivileged user namespaces, the user's choice); symbolic links are not followed and `-w` knows only `/bin`'s filesystem to be read-only, decided differences (§10; milestone 5's ABI 4 makes `-w` exact).
- **Every scenario's input waits for its prompt** (decision 12, the user's choice): a line typed while a program runs is echoed twice, by design, as on Linux; 354 `expect`s were added and 45 inputs marked `-ahead`, and the scenario parser refuses an input that no prompt paces.
- **One finding outside the plan:** `/bin/sh` prints an extra empty prompt after a `wait` that a job's end or a Ctrl-C ended (since before this plan); it was offered to the user as a task of its own, and `jobs.txt` expects it tolerantly.
- **The prototype's review.** A fresh reviewer read the whole prototype (then fifteen tasks), ran the shell's and xtask's tests and every scenario, a differential fuzz of 94,000 random argument lists under both names against `/usr/bin/test` and `/usr/bin/[`, throwaway QEMU scenarios under KVM (every path form of `[`, `-t` in pipes, scripts and `X | sh`, a root mounted read-only, `grep -q` in pipelines), and GNU on a read-only bind mount through `unshare -rm`. It found 0 critical, 2 important and 8 minor real defects. Each important one is settled in a task of its own after the task it concerns, with a test that fails first (I1) or by the user's decision (I2); the minors are fixed in the commits they concern, but for two that are commits of their own (M8) and one the user ruled (M3). It found correct: the grammar and every message against GNU (no difference in the fuzz, no panic), the explicit stack, `quote()`, `[`'s rules, the two names in QEMU, `ls /bin`, `help`, `-t` and the files on the real disk, `on_bin` through `SysVfs`, `grep -q` against GNU grep 3.11, the pacing rule and its replacements, the ripples and the rules (no ABI change, no new dependency, every tool on CI). It declined to judge invalid UTF-8 arguments (unreachable: the shell drops lines that are not text), the hand-edited NUC transcript (decided, 10), `-t` in `host-shell`, `-N` and atime on ext2, and the extra empty prompt after `wait` (out of scope).

| Finding (review) | Decision |
|---|---|
| Important (I1): the harness's sockets were bound at their own paths, past a Unix socket's 108 bytes in a checkout 57 or more characters deep, so `cargo test -p shell` failed in worktrees such as `m4p3-pacing` | Fixed, Task 5 |
| Important (I2): `-w` is true on a root the kernel mounted read-only (ext2 after an `EIO`, or the empty filesystem when no root mounts), where GNU says false; §15 item 3's premise ("the only read-only filesystem") was wrong | Ruled by the user: a decided difference now (§10, decision 7), exact with milestone 5's ABI 4 (`statfs`'s read-only flag; the roadmap's note) |
| Minor (M1): the room a binary operator needs (`left >= 3`) was untested; without it `test x -a y =` panics | Fixed in Task 3's tables |
| Minor (M2): five of `jobs.txt`'s inputs were marked `-ahead` though they are typed at the prompt, and `sh b.sh` was still echoed twice | Fixed in Task 16 (they wait for the prompt) and Task 17 (an `-ahead` mark after an expect of the prompt is refused, which found a sixth) |
| Minor (M3): PR 2 alone has `help` name `[` and `test`, which `/bin` gets only in PR 3 | Ruled by the user: kept, PR 2's body says so |
| Minor (M4): `-s` on an empty directory depends on the host's filesystem (0 on btrfs) | Fixed in Task 4 (each directory holds a file) |
| Minor (M5): `[ ! = ]`, named in spec §11.1, was not in the tables | Fixed in Task 3 |
| Minor (M6): the README did not say the unit tests need unprivileged user namespaces | Fixed in Task 4 |
| Minor (M7): `test.rs`'s module comment left out the two decided differences | Fixed in Task 4 |
| Minor (M8): one commit changed more than two modules (the documents' counts), another `AGENTS.md` beside xtask | Split: Tasks 9 and 18 |

## Where this plan fits

Plan 3 of milestone 4 implements the gate's third step (spec §12). It builds on plans 1 and 2: lists and the compound commands, whose conditions it gives a command to test with. It leaves plan 4 the NUC check of it all (`check6.sh`), the transcripts recorded again on the NUC, version 0.5.0, and the deferred findings of plans 2 and 3 (the roadmap's notes).

## Working conventions

- Plan 3 lands as **five pull requests** (table below). This plan, with the spec's §15 item 3 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-02-m4-plan-3-test-and-bracket.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; the harness in `crates/shell/src/testing.rs`; corpus scripts under `crates/shell/tests/corpus/`; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 2, 9, 10, 11, 13, 14, 16 and 18 have no failing run: they set up CI, change documents, or test what exists, and their introductions say what the mutation checks showed. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m4p3/mutations.md`).
- **The expectations are GNU's.** The unit tests run the host's own `/usr/bin/test`, `/usr/bin/[` and `/usr/bin/grep` (coreutils 9.4 and grep 3.11 here and on CI) and compare word for word and status for status; the file operators run GNU through `unshare -r`, which needs unprivileged user namespaces (Ubuntu 24.04 restricts them unless `kernel.apparmor_restrict_unprivileged_userns` is 0; CI's `unit` job sets it from Task 2 on). What GNU printed for each case is in `tmp/m4p3/progress.md` with its id (g1, q3, tt, qt5, …). In this session's shell `grep` may be a wrapper of another tool: the tests run `grep` without a shell, and any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop in a test.** The evaluator's own loops are bounded by the argument count; the endless input of `grep -q`'s test ends after 100 reads; the shell's test console fails a test that asks for a Ctrl-C more than 100,000 times. Mutation checks run under an address-space limit and a timeout (`tmp/m4p3/mutate.py`).
- **The NUC check scripts change once:** `check4.sh` gains the `[` line of `ls /bin`; its QEMU transcript is recorded again and its NUC transcript edited by hand (Task 8, decision 10).
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests, transcripts and scenarios that come with the change belong to its commit, and documents that are not tests take a commit of their own (Tasks 9 and 18, the review's M8). Every commit body line is within 72 columns. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. From Task 17 on, the scenario parser refuses an input that no `expect` of the prompt paces (`send-ahead` and its kin mark input sent while something runs on purpose). If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `c99f814` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m4p3/plan` | — | The spec's §15 item 3 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m4p3/test` | 1–7 | `MemFs`'s test setters; CI's user namespaces; `test` and `[` with GNU's grammar, file operators as root, `-t`; the harness's short socket path | `lint`, `unit`, `e2e` |
| 3 | `m4p3/bin` | 8–11 | `/bin/test` and `/bin/[` (44 programs) and what lists `/bin`; the documents' counts; the scenario `test_cmd`; the corpus's `test.sh` | `lint`, `unit`, `e2e` |
| 4 | `m4p3/grep-q` | 12–14 | `grep -q`; the comments of `RESERVED` and `depth`; `control`'s `kill` and `wait` | `lint`, `unit`, `e2e` |
| 5 | `m4p3/pacing` | 15–18 | The `-ahead` steps; every scenario paced; the parser's rule; `AGENTS.md` | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 3 adds no crate. `crates/shell` uses only `vfs` and `relay-abi` and is `no_std` outside its tests. The kernel has no dev-dependencies and does not depend on `shell`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (UG §3.1); `relay-abi` holds no architecture detail and keeps the same values on every architecture. Plan 3 changes no ABI value and no kernel code (one comment): `relay_abi::VERSION` stays 3.
- Every milestone 1–3 scenario, plans 1 and 2's, and every check script pass, paced as Task 16 paces them; `check4.sh` and its transcripts change only by the `[` line of `ls /bin`.
- What a person types is untrusted (AGENTS.md): nothing that follows from it may make `test` or `grep` panic, index out of bounds, overflow, allocate without bound or loop forever. A program's stack is 255 pages, so nothing recurses as deep as its arguments; integers are compared as digit strings.
- Commands follow GNU coreutils and GNU grep word for word in the C locale; the only decided differences are spec §10's and §15 item 3's: no `<` or `>`, `[ --help` refused, symbolic links not followed, `-w` false only on `/bin`'s filesystem.
- Missing tools fail tests, never skip them: GNU's `test`, `[`, `grep`, `touch`, `mkfifo`, `unshare` and a block device in `/dev` are on CI's runner, and a user namespace refused fails the test with the sysctl named.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 3 in PR 1:

1. **The two names** (§6.1). Cargo refuses `[` as a binary's name (`invalid character '[' in crate name`), so `relay-utils` builds one program, `test`, and xtask packs its ELF into `system.img` under both names, the bytes twice (the format gains no shared entries): 44 programs. The program is `[` when the last part of its argument 0's path is `[`, as the shells give a command's name as typed. `COMMANDS` lists `[`, first in the C locale's order, and `test`: two functions over one evaluator. Plan 3 is five pull requests: its plan; `test` and `[` in the shell with their unit tests, `/bin` unchanged; both names in `/bin` with every ripple, the scenario `test_cmd` and corpus scripts using `test`; `grep -q` with two of plan 2's deferred minors; the pacing of every scenario's input. It has no NUC check; plan 4's check 6 runs `test` there.
2. **The grammar** (§6.2) is GNU coreutils 9.4's (`src/test.c`), rule for rule: the rules by argument count for 1 to 4 arguments, else `-o` over `-a` over terms; a term is a run of `!`, then `(` (an expression of up to 4 arguments before its `)`, or the rest), a binary operator if the argument after next is one, a unary operator of the form `-X`, or a string. Its messages are GNU's, each quoting its argument as GNU's `quote()` does in the C locale (`'\t1\n'`, `'\331\241'`): `missing argument after 'X'`, `'X': unary operator expected`, `'X': binary operator expected`, `extra argument 'X'`, `')' expected` (in `[`, `')' expected, found ']'` when the `]` is where the `)` should be), `invalid integer 'X'`, `-nt does not accept -l` (and `-ot`, `-ef`), `missing ']'`.
3. **No `<` or `>`** (§6.2). GNU's program has neither (`test a '<' b` is `'<': binary operator expected`, status 2); bash's built-in has them. Plan 3 follows GNU, so §6.2's `\<` and `\>` are dropped (the maintainer, 2026-10-02).
4. **`-l STRING`**, GNU's length of STRING, stands for an integer on either side of `-eq`, `-ne`, `-lt`, `-le`, `-gt` and `-ge`, as "GNU's grammar in full" asks.
5. **No recursion.** GNU nests `( x -a ( x -a … ) )` 30,000 levels deep on its 8 MB stack, and an argument list of 64 KiB can ask for more than that; a program here has a stack of 255 pages, so the evaluator keeps its open `(` on a stack of its own and nests as deep as GNU.
6. **`[ --help` and `[ --version`** (§6.2). GNU 9.4 prints its help and version when one of them is `[`'s only argument; here they are refused as §6.2 says, `[: unrecognized option '--help'` (or `'--version'`), status 2, and only then: `[ --help ]` is one string, true.
7. **`-w`** (§6.2). No call says that a filesystem is read-only, and `open` for writing succeeds on `/bin` (`EROFS` comes with the write), so `-w` is false for a file on `/bin`'s filesystem (the `dev` of its `stat` is `/bin`'s), `system.img`. The kernel also mounts the root read-only (ext2 after an `EIO`, or an empty filesystem when no root mounts), and there `-w` is true where GNU says false (the prototype's review): a decided difference (§10). Nothing in the kernel or the ABI changes; milestone 5's ABI 4 gives `statfs` a read-only flag, and `test -w` uses it then (the maintainer, 2026-10-02).
8. **`-t FD`** is true when FD, read as GNU reads an integer, is open on the console (`fstat`: a character device on filesystem 0); a number past `int` is false, as in GNU.
9. **Testing** (§11.1). The unit tests run GNU's `test` and `[` through `unshare -r`, so that it answers as root, as Relay OS runs every program; CI's ubuntu-24.04 restricts user namespaces, so its `unit` job sets `kernel.apparmor_restrict_unprivileged_userns=0` first, and a namespace refused fails the test (the maintainer, 2026-10-02). The cases a symbolic link answers differently (§10) are checked against GNU's `-h` and named as the difference.
10. **The NUC transcript** (§11.5). `check4.nuc.log` gets the `[` line of `ls /bin` by hand, as milestone 2's plans edited NUC transcripts, until plan 4's NUC run replaces it; `check4.qemu.log` is recorded again.
11. **`grep -q`** (the roadmap's note): GNU grep 3.11's quiet mode. It prints nothing and ends, status 0, at the first selected line, opening no later file; an error before it is still told, and status 2 needs an error and no line selected. `-q` outweighs `-c` and `-n`, says nothing of a binary file, and skips the check of an input that is the output file, as GNU does.
12. **Input paced by its prompt** (§11.3). A line sent while a program runs is echoed twice, by the line discipline as it is typed and by the shell's editor once the console is handed back, as on Linux; sent before a scenario's last command has ended, it can land inside the output an `expect` waits for (#103's failed CI run). So every `send`, `key` and `type` follows an `expect` that ends at a prompt, the scenario parser refuses one that does not, and a step marked as typed ahead (`send-ahead`, and the same for `key` and `type`) is sent while something runs on purpose: input for a program, Ctrl-C, a line beside a background job's output (the maintainer, 2026-10-02). The parser refuses that mark right after an expect of the prompt, where the input waits for nothing (the prototype's review).
13. **Plan 2's deferred minors.** Plan 3 settles two: the stale comments of `RESERVED` (`crates/shell/src/parser.rs`) and of the scan's `depth` (`crates/shell/src/scan.rs`), and `control.txt`'s `kill %1` / `wait %1` pair, which nothing paced. The scan reading `(( x = 1 << 2 ))` as a here-document (on the safe side) and a test of the kernel's `tty::ctrl_c` wiring go to plan 4 (the maintainer, 2026-10-02).

## Review Focus

1. **Conditions people write in scripts**, compared with GNU's `test` and `[`: operators used as operands (`[ -n = -n ]`, `[ ! = ]`, `[ "$x" = ]` with `$x` empty or `-n`), `!`, `-a`, `-o` and `\(` `\)` in 4–8 arguments and deeper, `-l`, integers with blanks, signs, leading zeros and any length, non-ASCII strings, every message under each name, `[`'s `]`, `[ --help`, too few arguments for an operator. Expected: GNU's status and message byte for byte, never a panic. Tests: `the_rules_by_argument_count_are_gnu_s`, `not_and_or_and_parentheses_are_gnu_s`, `strings_compare_bytes_and_have_no_less_or_greater`, `integers_are_read_as_gnu_reads_them_and_have_any_length`, `dash_l_stands_for_a_string_s_length`, `errors_quote_their_argument_as_gnu_s_quote_does`, `parentheses_nest_as_deep_as_gnu_s_without_recursion`, `bracket_needs_its_bracket_and_refuses_help_and_version` (Task 3), the corpus's `test.sh` (Task 11), `test_cmd` (Task 10).
2. **Files on the real disk and at the console:** `[ -w /bin/ls ]`, `[ -w /root ]`, `-x` on directories and on files without execute bits, `-e`/`-f`/`-d` on a symbolic link, `-nt`/`-ot` of files made seconds apart and within one second, `-ef` across mounts and between `/bin`'s two names, `-N`, `-s` on directories, `-O`/`-G`; `-t 0`, `-t 1`, `-t 2` at the console, in a pipe, redirected, in a script and under `X | sh`. Expected: GNU's answers as root, except the decided differences (links not followed, `-w` true on a read-only root). Tests: `file_operators_answer_as_gnu_s_do_for_root`, `devices_answer_as_gnu_s_do`, `a_symbolic_link_is_never_followed`, `nothing_on_bin_s_filesystem_is_writable`, `owned_by_another_is_not_owned_by_root` (Task 4), `a_socket_is_made_however_deep_its_directory_is` (Task 5), `dash_t_reads_its_fd_as_gnu_reads_an_integer` (Task 6), `the_console_is_the_character_device_of_filesystem_0` (Task 7), `test_cmd` (Task 10).
3. **The two names in `/bin`:** `/bin/[` and `[` started by every path form, `ls /bin` and `ls -l /bin`, `help`, check 4 and its transcripts. Expected: `[` needs its `]` however it is started; `ls /bin` as GNU's `ls -C`; `system: 44 programs, ABI 3`. Tests: `the_program_is_bracket_when_started_as_bracket` (Task 3), `the_system_image_holds_every_program`, `the_check_scripts_pass_on_both_machines`, the scenarios `system` and `checks` (Task 8), `test_cmd` (Task 10), `help_lists_every_command_with_its_description`.
4. **`grep -q` in conditions and pipelines:** a match before or after an error, `-q` with `-c`, `-n`, `-v`, `-i`, binary input, the output file as an input, standard input, `seq 100000 | grep -q 1`. Expected: GNU grep 3.11's status and silence; grep ends at the first match and the writer ends quietly. Tests: `grep_q_says_only_whether_a_line_is_selected`, `grep_q_stops_reading_at_its_first_selected_line`, `grep_q_writes_nothing_so_its_output_may_be_an_input` (Task 12).
5. **A scenario that types ahead by mistake:** a `send`, `key` or `type` with no `expect` of the prompt since the input before, after a reboot or a reset, an `-ahead` mark where the input waits for the prompt, a prompt matched with or without `$` or in another directory, a background job's output after the prompt. Expected: the parser refuses the scenario, naming the line; every scenario in `tests/e2e/` passes the rule and runs green. Tests: `input_waits_for_a_prompt_unless_typed_ahead`, `every_scenario_waits_for_its_prompts` (Task 17), `input_typed_ahead_is_sent_as_other_input_is` (Task 15), every scenario (Task 16).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/shell/src/commands/test.rs` (new) | `test`, `bracket`, `named`; the evaluator `Eval` with its stack of `Frame`s and `Step`s; `Number` (digit strings, `-l`, `-t`'s fd); `quote` (GNU's `quote()`); `on_bin` |
| `crates/shell/src/commands/mod.rs` | `[` and `test` in `COMMANDS`; `grep`'s `help` line |
| `crates/shell/src/commands/grep.rs` | `-q` (`Options.quiet`) |
| `crates/shell/src/io.rs` | `System::is_terminal` |
| `crates/shell/src/testing.rs` | `TestFile`, `Made`, `host_files` (GNU through `unshare -r`, sockets through a short path), `like_host_files`, `Harness::program_args`; `TestSystem.terminals` |
| `crates/shell/tests/corpus/test.sh` (new) | The corpus's conditions |
| `crates/vfs/src/memfs.rs` | `special`, `set_mode`, `set_times`, `set_owner` (tests only) |
| `crates/relay-rt/src/sysio.rs` | `is_console`; `SysSystem::is_terminal` |
| `userland/utils/src/bin/test.rs` (new) | `/bin/test` and `/bin/[` |
| `xtask/src/userland.rs` | The `test` program packed as `[` too |
| `xtask/src/e2e.rs` | The `-ahead` steps; `ends_at_prompt` and the parser's rule |
| `xtask/src/checks.rs`, `xtask/fixtures/checks/check4.{qemu,nuc}.log`, `rootfs/root/checks/check4.sh` | Check 4's `ls /bin` with `[` |
| `tests/e2e/*.txt` | Every input paced; `test_cmd.txt` (new); `system.txt`'s `ls /bin` |
| `.github/workflows/ci.yml` | The `unit` job allows unprivileged user namespaces |
| `README.md`, `AGENTS.md`, `kernel/src/system.rs`, `docs/hardware-test.md` | The tests' namespaces; the pacing rule; the count of 44 programs |

---

## PR 1: The spec's §15 item 3, the roadmap's notes and this plan

The spec's §15 item 3, the decisions of this plan, with the parts of its body it corrects (the status line, §6.1, §6.2, §10, §11.1, §11.5, §12); the roadmap's notes on plan 2's deferred minors and rulings, plan 3's NUC transcript and CI, the pacing, and milestone 5's ABI 4 for `test -w`; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 4 plan 3's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p3/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m4p3/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-02-m4-plan-3-test-and-bracket.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-02-m4-plan-3-test-and-bracket.md
git commit -m "docs(plan): add milestone 4 plan 3, test and ["
cargo xtask lint
git push -u origin m4p3/plan
gh pr create --base main --head m4p3/plan --title "docs(spec,plan): add milestone 4 plan 3, test and [" --body-file - <<'EOF2'
## What

The implementation plan of milestone 4's plan 3 ("`test` and `[`"), with the spec's §15 item 3: one program under two names, packed twice since Cargo refuses `[`; GNU coreutils 9.4's grammar rule for rule, without `<` and `>`, with `-l`, without recursion; `[ --help` refused; `-w` false only on `/bin`'s filesystem (a decided difference until milestone 5's ABI 4); `-t` from the console; the tests against GNU as root through `unshare -r`, which CI's `unit` job allows; check 4's NUC transcript edited by hand; `grep -q`; every scenario's input paced by its prompt; two of plan 2's deferred minors; and the corrections they bring to §6.1, §6.2, §10, §11.1, §11.5 and §12. The roadmap's notes: plan 2's deferred minors and rulings, plan 3's NUC transcript and CI, the pacing, and `statfs`'s read-only flag for milestone 5.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m4p3/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m4p3/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-plan` and continue with PR 2.

---

## PR 2: `test` and `[` in the shell (Tasks 1–7)

One evaluator under two names with GNU coreutils 9.4's grammar, compared with the host's `/usr/bin/test` and `/usr/bin/[` (the file operators as root, through `unshare -r`, which CI's `unit` job now allows); `-t` through the system's console, from `fstat` in programs. `/bin` is unchanged: `help` names `[` and `test` before PR 3 puts them there (the review's M3, which the user ruled; PR 2's body says so). With the review's fix: the harness's sockets bound through a short path.

Branch `m4p3/test`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-test`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p3/test /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-test origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-test
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p3/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p3/test /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-test m4p3/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p3/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: A test can shape a `MemFs` node and make FIFOs, sockets and devices

Spec §11.1 and decision 9: `test`'s file operators are compared with GNU on files of every kind and mode, which nothing in the shell can make. `MemFs` gains test-only `set_mode` (the permission bits with set-user-ID, set-group-ID and sticky, `EINVAL` past `0o7777`), `set_times` (access and modification), `set_owner`, and `special`, which makes a FIFO, a socket, a character or a block device (`Body::Special`, mode `0644`): it holds no data, so reading, writing or truncating it is `EINVAL`, as for a symbolic link, and a read-only `MemFs` refuses to make one. Every node keeps its owner (root's until set). The red run is the vfs tests, which cannot find the new methods. Mutation checks: the mode's bound, the kinds `special` takes, the owner `stat` gives, and the data calls on a special node, each broken, fail a test.

**Files:**
- Modify: `crates/vfs/src/memfs.rs`

**Interfaces:**
- Consumes: `vfs::MemFs`, `vfs::FileType`.
- Produces: `MemFs::special(&mut self, Ino, &[u8], FileType) -> Result<Ino, Errno>`, `MemFs::set_mode(&mut self, Ino, u16)`, `MemFs::set_times(&mut self, Ino, atime: u64, mtime: u64)`, `MemFs::set_owner(&mut self, Ino, uid: u32, gid: u32)`, each `-> Result<(), Errno>` (tests only).

- [ ] **Step 1: Add the failing tests to `crates/vfs/src/memfs.rs`**

In `crates/vfs/src/memfs.rs`, replace:

````rust

    fn names(fs: &mut MemFs, dir: Ino) -> Vec<Vec<u8>> {
````

with:

````rust

    #[test]
    fn a_test_sets_a_node_s_mode_times_and_owner() {
        let mut fs = fs();
        let f = fs.create(ROOT, b"f").unwrap();
        fs.set_mode(f, 0o4711).unwrap();
        fs.set_times(f, 5, 7).unwrap();
        fs.set_owner(f, 1000, 100).unwrap();
        let st = fs.stat(f).unwrap();
        assert_eq!((st.perm, st.atime, st.mtime), (0o4711, 5, 7));
        assert_eq!((st.uid, st.gid, st.ctime), (1000, 100, 1_000));
        // Only the permission bits and set-user-ID, set-group-ID, sticky.
        assert_eq!(fs.set_mode(f, 0o10_000), Err(Errno::EINVAL));
        assert_eq!(fs.set_times(99, 0, 0), Err(Errno::ENOENT));
        assert_eq!(fs.set_owner(99, 0, 0), Err(Errno::ENOENT));
    }

    #[test]
    fn a_test_makes_fifos_sockets_and_devices() {
        let mut fs = fs();
        for (name, kind) in [
            (&b"p"[..], FileType::Fifo),
            (b"s", FileType::Socket),
            (b"c", FileType::CharDev),
            (b"b", FileType::BlockDev),
        ] {
            let ino = fs.special(ROOT, name, kind).unwrap();
            let st = fs.stat(ino).unwrap();
            assert_eq!((st.kind, st.perm, st.size, st.nlink), (kind, 0o644, 0, 1));
            // Their data is not the filesystem's.
            let mut buf = [0; 4];
            assert_eq!(fs.read_at(ino, 0, &mut buf), Err(Errno::EINVAL));
            assert_eq!(fs.write_at(ino, 0, b"x"), Err(Errno::EINVAL));
            assert_eq!(fs.truncate(ino, 0), Err(Errno::EINVAL));
            assert_eq!(fs.read_link(ino), Err(Errno::EINVAL));
            fs.unlink(ROOT, name).unwrap();
        }
        for kind in [FileType::Regular, FileType::Directory, FileType::Symlink] {
            assert_eq!(fs.special(ROOT, b"x", kind), Err(Errno::EINVAL));
        }
        assert_eq!(fs.special(ROOT, b"", FileType::Fifo), Err(Errno::ENOENT));
        let mut ro = MemFs::new(Box::new(Clock(Cell::new(1)))).read_only();
        assert_eq!(ro.special(ROOT, b"p", FileType::Fifo), Err(Errno::EROFS));
    }

    fn names(fs: &mut MemFs, dir: Ino) -> Vec<Vec<u8>> {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vfs`

Expected: FAIL: compile errors such as `` no method named `set_mode` found for struct `memfs::MemFs` in the current scope ``; `` no method named `set_times` found for struct `memfs::MemFs` in the current scope ``.

- [ ] **Step 3: Change `crates/vfs/src/memfs.rs`**

In `crates/vfs/src/memfs.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
    Symlink(Vec<u8>),
}
````

with:

````rust
    Symlink(Vec<u8>),
    /// A FIFO, a socket or a device (tests only), which holds no data.
    Special(FileType),
}
````

Replace:

````rust
    nlink: u32,
    atime: u64,
````

with:

````rust
    nlink: u32,
    uid: u32,
    gid: u32,
    atime: u64,
````

Replace:

````rust
        self.new_node(dir, name, 0o777, Body::Symlink(target.to_vec()))
    }
````

with:

````rust
        self.new_node(dir, name, 0o777, Body::Symlink(target.to_vec()))
    }

    /// Creates a FIFO, a socket or a device, mode `0644` (tests only:
    /// nothing else makes one).
    pub fn special(&mut self, dir: Ino, name: &[u8], kind: FileType) -> Result<Ino, Errno> {
        match kind {
            FileType::Fifo | FileType::Socket | FileType::CharDev | FileType::BlockDev => {
                self.new_node(dir, name, 0o644, Body::Special(kind))
            }
            _ => Err(Errno::EINVAL),
        }
    }

    /// Sets a node's permission bits, set-user-ID, set-group-ID and sticky
    /// included (tests only).
    pub fn set_mode(&mut self, ino: Ino, perm: u16) -> Result<(), Errno> {
        if perm > 0o7777 {
            return Err(Errno::EINVAL);
        }
        self.node_mut(ino)?.perm = perm;
        Ok(())
    }

    /// Sets a node's access and modification times (tests only).
    pub fn set_times(&mut self, ino: Ino, atime: u64, mtime: u64) -> Result<(), Errno> {
        let node = self.node_mut(ino)?;
        (node.atime, node.mtime) = (atime, mtime);
        Ok(())
    }

    /// Sets a node's owner and group (tests only; every node is root's).
    pub fn set_owner(&mut self, ino: Ino, uid: u32, gid: u32) -> Result<(), Errno> {
        let node = self.node_mut(ino)?;
        (node.uid, node.gid) = (uid, gid);
        Ok(())
    }
````

Replace:

````rust
            Body::Dir { .. } => Err(Errno::EISDIR),
            Body::Symlink(_) => Err(Errno::EINVAL),
        }
````

with:

````rust
            Body::Dir { .. } => Err(Errno::EISDIR),
            Body::Symlink(_) | Body::Special(_) => Err(Errno::EINVAL),
        }
````

Replace:

````rust
            nlink,
            atime: now,
````

with:

````rust
            nlink,
            uid: 0,
            gid: 0,
            atime: now,
````

Replace:

````rust
            Body::Symlink(t) => (FileType::Symlink, t.len() as u64, 0),
        };
````

with:

````rust
            Body::Symlink(t) => (FileType::Symlink, t.len() as u64, 0),
            Body::Special(kind) => (*kind, 0, 0),
        };
````

Replace:

````rust
            nlink: n.nlink,
            uid: 0,
            gid: 0,
            size,
````

with:

````rust
            nlink: n.nlink,
            uid: n.uid,
            gid: n.gid,
            size,
````

Replace:

````rust
            Body::Dir { .. } => return Err(Errno::EISDIR),
            Body::Symlink(_) => return Err(Errno::EINVAL),
        };
````

with:

````rust
            Body::Dir { .. } => return Err(Errno::EISDIR),
            Body::Symlink(_) | Body::Special(_) => return Err(Errno::EINVAL),
        };
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p vfs`

Expected: PASS: 56 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(vfs): let a test shape a MemFs node and make special files

The tests of `test`'s file operators need files with modes, times and
owners that nothing in the shell can set, and FIFOs, sockets and
devices, which nothing makes. MemFs gains set_mode, set_times and
set_owner, and special, which makes a node of one of those kinds: it
holds no data, so reading or writing it is EINVAL, as for a symbolic
link.
EOF
````


### Task 2: CI's unit job may make user namespaces

Decision 9: the unit tests of `test` run GNU's `test` through `unshare -r`, so that it answers as root, as Relay OS runs every program. Ubuntu 24.04 lets AppArmor refuse an unprivileged user namespace (`kernel.apparmor_restrict_unprivileged_userns`, set on GitHub's runners, actions/runner-images#10443), so the `unit` job clears it first. The workflow still only installs and sets up tools. No test needs it yet; Task 4's do.

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `.github/workflows/ci.yml`'s `unit` job.
- Produces: the step "Allow unprivileged user namespaces".

- [ ] **Step 1: Change `.github/workflows/ci.yml`**

In `.github/workflows/ci.yml`, replace:

````text
          sudo apt-get install -y --no-install-recommends mtools
      - name: Install the pinned toolchain
````

with:

````text
          sudo apt-get install -y --no-install-recommends mtools
      # The tests of `test` run GNU's test as root through `unshare -r`;
      # Ubuntu 24.04 lets AppArmor refuse an unprivileged user namespace.
      - name: Allow unprivileged user namespaces
        run: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
      - name: Install the pinned toolchain
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add .github
git commit -F - <<'EOF'
ci: let the unit job make user namespaces

The unit tests of `test` (spec §15 item 3) run GNU's test through
`unshare -r`, so that it answers as root, as Relay OS runs every
program. Ubuntu 24.04 lets AppArmor refuse an unprivileged user
namespace (kernel.apparmor_restrict_unprivileged_userns), which the
job's runner sets; the job clears it first.
EOF
````


### Task 3: `test` and `[` with GNU's grammar, strings and integers

Spec §6 and decisions 1, 2, 3, 4, 5 and 6: `crates/shell/src/commands/test.rs` holds one evaluator under two names, `test` and `bracket`, both in `COMMANDS` (`[` sorts first), so both run in the shell's tests and in `host-shell`; `/bin` gets them in PR 3. The grammar is GNU coreutils 9.4's (`src/test.c`) rule for rule: the rules by argument count for 1 to 4 arguments, then `-o` over `-a` over terms; a term is a run of `!`, then `(` (an expression of up to 4 arguments before its `)`, or all the rest), a binary operator if the argument after next is one (with `-l STRING` on either side of an integer operator), a unary operator of the form `-X`, or a string. Strings compare as bytes, with no `<` or `>`; integers are read as GNU's `find_int` reads them (blanks, a sign, digits) and compared as digit strings, so any length works; `-nt`, `-ot` and `-ef` answer from `stat`. GNU's messages are word for word, each argument quoted as its `quote()` does in the C locale (`'\303\251'`). The evaluator keeps each open `(` on a stack of its own (`Frame`, `Step`), so it nests as deep as GNU's (30,000 levels in a thread with 256 KiB of stack). `[` needs its `]` (`[: missing ']'`), sees the `]` past the expression's end as GNU's does (`')' expected, found ']'`), and refuses `--help` and `--version` alone. `named` picks the program's name and function from its argument 0, for Task 8. The harness's `like_host` passes words to the command without parsing them (`Harness::program_args`), and `TestFile`, `host_files` and `like_host_files` make files with times on both sides. The cases are inputs only: each runs under both names and is compared with the host's `/usr/bin/test` and `/usr/bin/[` (GNU's, not bash's built-in; `tmp/m4p3/probes/grammar.txt` holds what they printed). `commands/mod.rs`'s own tests import by name, since a module named `test` makes a glob import's `#[test]` ambiguous. The red run is the shell's tests, which cannot find `bracket`, `test` or `named`. Mutation checks (24): `[`'s `]` and its `--help`, the name picking, the extra-argument check, the 4-argument `(` rule, a string's truth, the scan of `(` (one equivalent mutant: counting 5 before taking the rest reads the same), the `)` past the end, `-l`'s and a binary operator's room (each panics without it: `x -a -l a =`, `x -a y =`, the review's M1), `-nt` with a missing file, the unary form, each of `find_int`'s rules, the digit-string order, and the quoting, each broken, fail a test. A throwaway fuzz of about 18,000 random argument lists found no difference from GNU (`progress.md` f1).

**Files:**
- Modify: `crates/shell/src/commands/mod.rs`
- Create: `crates/shell/src/commands/test.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: `commands::{Builtin, Run, COMMANDS}`, `Ctx` (`err`, `vfs`), `vfs::{Vfs, Node, Stat}`, `testing::{Harness, host_tool, memfs}`; Task 1's `MemFs::set_times`.
- Produces: `commands::bracket`, `commands::named(&[u8]) -> (&'static str, Run)`, `test::test` (crate-private); `testing::TestFile { name, made, times }` with `file`, `dir`, `hard_link`, `times`; `testing::Made::{File, Dir, HardLink}`; `testing::like_host_files(&[&str], &[TestFile]) -> (i32, String, String)`, `testing::host_files(&[&str], &[TestFile], root: bool) -> (i32, String, String)`; `Harness::program_args(&mut self, &[&str], &mut FakeStdout) -> (i32, String)`.

- [ ] **Step 1: Add the failing tests and the module declaration to `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
mod system;
mod text;
````

with:

````rust
mod system;
mod test;
mod text;
````

Replace:

````rust
mod tests {
    use super::*;

````

with:

````rust
mod tests {
    // Not a glob: the module `test` would make `#[test]` ambiguous.
    use super::{COMMANDS, builtin, find};

````

- [ ] **Step 2: Write the failing tests for `crates/shell/src/commands/test.rs`**

Create `crates/shell/src/commands/test.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::named;
    use crate::testing::{Harness, TestFile, host_files, host_tool, like_host_files};
    use alloc::string::String;
    use alloc::vec::Vec;

    /// `args` (without the name) under `name`, as GNU's program and as
    /// ours; for `[` the `]` is added.
    fn both(name: &str, args: &[&str]) -> ((i32, String, String), (i32, String, String)) {
        let mut line: Vec<&str> = [name].into_iter().chain(args.iter().copied()).collect();
        if name == "[" {
            line.push("]");
        }
        (
            Harness::new().like_host(&line, &[], b""),
            host_tool(&line, &[], b""),
        )
    }

    fn like_gnu(cases: &[&[&str]]) {
        for args in cases {
            for name in ["test", "["] {
                let (ours, gnu) = both(name, args);
                assert_eq!(ours, gnu, "{name} {args:?}");
            }
        }
    }

    #[test]
    fn the_rules_by_argument_count_are_gnu_s() {
        // probes/grammar.txt a0–a4h: 0 to 4 arguments.
        like_gnu(&[
            &[],
            &[""],
            &["x"],
            &["-n"],
            &["!"],
            &["("],
            &[")"],
            &["-a"],
            &["--help"],
            &["--version"],
            &["!", ""],
            &["!", "x"],
            &["-n", ""],
            &["-z", ""],
            &["-z", "x"],
            &["-q", "x"],
            &["x", "y"],
            &["-", "x"],
            &["--", "x"],
            &["(", "x"],
            &["-n", "-n"],
            &["!", "!"],
            &["!", "="],
            &["(", ")"],
            &["a", "=", "a"],
            &["a", "==", "b"],
            &["a", "!=", "b"],
            &["a", "!=", "a"],
            &["!", "-n", ""],
            &["(", "x", ")"],
            &["(", "", ")"],
            &["x", "-a", ""],
            &["", "-o", "x"],
            &["a", "b", "c"],
            &["!", "!", "x"],
            &["-n", "=", "-n"],
            &["!", "=", "!"],
            &["(", "=", ")"],
            &["a", "<", "b"],
            &["b", ">", "a"],
            &["(", "!", ")"],
            &["-a", "-a", "-a"],
            &["-o", "-o", "-o"],
            &["!", "a", "b"],
            &["!", "a", "=", "a"],
            &["(", "a", "=", ")"],
            &["(", "-n", "x", ")"],
            &["!", "!", "!", "x"],
            &["a", "=", "a", "b"],
            &["!", "(", "x", ")"],
            &["(", "a", "b", ")"],
            &["-n", "x", "-a", "y"],
            &["(", "!", "x", ")"],
            &["(", "(", "x", ")"],
        ]);
    }

    #[test]
    fn not_and_or_and_parentheses_are_gnu_s() {
        // probes/grammar.txt a5a–a5s: 5 arguments and more, GNU's
        // precedence (`-a` before `-o`) and its errors.
        like_gnu(&[
            &["", "-o", "x", "-a", ""],
            &["x", "-o", "", "-a", ""],
            &["!", "", "-a", "!", ""],
            &["(", "", "-o", "x", ")", "-a", "x"],
            &["!", "(", "x", ")", "-o", "x"],
            &["(", "x", "-a", "y"],
            &["x", "-a"],
            &["x", "-a", "y", "-o"],
            &["a", "b", "c", "d", "e"],
            &["x", ")", "y", "z", "w"],
            &["!", "!", "!", "!", "x"],
            &["-n", "x", "-a", "-z", "x"],
            &["(", "(", "x", ")", ")"],
            &["a", "=", "a", "-a", "-n"],
            &["-z", "-a", "-z", "-a", "-z"],
            &["x", "-a", "(", "y"],
            &["=", "=", "=", "-a", "x"],
            &["(", "x", ")", "-a", "(", "", ")"],
            &["(", "x", "-a", "", ")", "-o", "y"],
            &["(", "(", "(", "x", ")", ")", ")"],
            &["(", "!", "(", "x", ")", ")"],
            &["(", "a", "=", "b", "-o", "c", ")"],
            &["(", "a", "b", "c", "d", ")"],
            &["(", "x", ")", "y"],
            &["x", "-o", "(", "y", ")", "-a", "!", "z"],
            &["!", "x", "-o", "!", "", "-a", "x"],
            &["a", "-a", "b", "-a", "c", "-a", "d"],
            &["", "-o", "", "-o", "", "-o", "x"],
            &["(", "-a", ")"],
            &["(", "-o", "x", ")"],
            // Found by a fuzz against GNU: the 4-argument rule inside `(`.
            &["x", "-a", "(", "!", "=", "x", "1", ")"],
            &["(", "!", "(", "-l", "-o", ")", ""],
            // Too few arguments left for a binary operator (the review).
            &["x", "-a", "y", "="],
            &["x", "-o", "="],
        ]);
    }

    #[test]
    fn strings_compare_bytes_and_have_no_less_or_greater() {
        // probes/grammar.txt s1–s7: GNU's program has no `<` or `>`
        // (spec §15 item 3).
        like_gnu(&[
            &["é", "=", "é"],
            &["é", "=", "e"],
            &["é", "!=", "e"],
            &["-n", "é"],
            &["-z", "é"],
            &["日本"],
            &["a", "<", "b"],
            &["é", ">", "z"],
            &["a", "=", "b", "=", "c"],
            &["a b", "=", "a b"],
            &["=", "=", "="],
            &["!=", "!=", "!="],
            &["-", "=", "-"],
        ]);
    }

    #[test]
    fn integers_are_read_as_gnu_reads_them_and_have_any_length() {
        // probes/grammar.txt i1–i23: blanks around, a sign, any length.
        like_gnu(&[
            &["1", "-eq", "x"],
            &[" +12 ", "-eq", "12"],
            &["-0", "-eq", "+0"],
            &["99999999999999999999999", "-gt", "99999999999999999999998"],
            &["-99999999999999999999999", "-lt", "1"],
            &[
                "-99999999999999999999999",
                "-lt",
                "-99999999999999999999998",
            ],
            &[
                "123456789012345678901234567890",
                "-eq",
                "0123456789012345678901234567890",
            ],
            &["", "-eq", "0"],
            &[" ", "-eq", "0"],
            &["1 2", "-eq", "1"],
            &["\t1\n", "-eq", "1"],
            &["\t1\t", "-eq", "1"],
            &["0x1", "-eq", "1"],
            &["1", "-eq", "1.0"],
            &["007", "-eq", "7"],
            &["+", "-eq", "0"],
            &["-", "-eq", "0"],
            &["--1", "-eq", "1"],
            &["+-1", "-eq", "1"],
            &["١", "-eq", "1"],
            &["1", "-ne", "2"],
            &["2", "-le", "2"],
            &["3", "-le", "2"],
            &["3", "-ge", "4"],
            &["4", "-ge", "4"],
            &["-5", "-gt", "-6"],
            &["-5", "-lt", "-6"],
            &["\u{b}1", "-eq", "1"],
            &["1\r", "-eq", "1"],
            &["- 1", "-eq", "-1"],
            &["1", "-eq", "1", "-a", "2", "-lt", "1"],
            &["!", "1", "-eq", "1"],
            &["1", "-EQ", "1"],
            &["1", "-eqq", "1"],
        ]);
    }

    #[test]
    fn dash_l_stands_for_a_string_s_length() {
        like_gnu(&[
            &["-l", "abc", "-eq", "3"],
            &["-l", "", "-eq", "0"],
            &["-l", "é", "-eq", "2"],
            &["3", "-eq", "-l", "abc"],
            &["-l", "ab", "-lt", "-l", "abc"],
            &["1", "-lt", "-l"],
            &["-l", "abc", "-eq", "x"],
            &["-l", "abc", "=", "abc"],
            &["a", "=", "-l", "a"],
            &["=", "=", "-l", "b"],
            &["-l", "a", "-nt", "b"],
            &["a", "-ot", "-l", "b"],
            &["-l", "a", "-ef", "a"],
            &["-l"],
            &["-l", "x"],
            &["-l", "x", "y"],
            &["!", "-l", "abc", "-eq", "3"],
            &["-l", "abc", "-eq", "3", "-a", "x"],
            // Too few arguments left for `-l` and an operator.
            &["x", "-a", "-l", "a", "="],
        ]);
    }

    #[test]
    fn errors_quote_their_argument_as_gnu_s_quote_does() {
        // probes/quote.txt qt0–qt13: C escapes in plain quotes, other
        // bytes in octal.
        like_gnu(&[
            &["a'b", "-eq", "1"],
            &["a\\b", "-eq", "1"],
            &["a\"b", "-eq", "1"],
            &["é", "-eq", "1"],
            &["\u{7f}", "-eq", "1"],
            &["\u{1}", "-eq", "1"],
            &["\u{1b}", "-eq", "1"],
            &["\u{7}\u{8}\u{c}\r\u{b}", "-eq", "1"],
            &["a?b??=c", "-eq", "1"],
            &["€", "-eq", "1"],
            &["-é"],
            &["-é", "x"],
            &["x", "é", "y"],
            &["a", "b", "c", "d", "é"],
        ]);
    }

    #[test]
    fn parentheses_nest_as_deep_as_gnu_s_without_recursion() {
        // probes/deep.txt d30000e: GNU nests 30000 levels; this evaluator
        // keeps its open parentheses on a stack of its own, so a small
        // stack is enough.
        let mut line = alloc::vec![String::from("test")];
        for _ in 0..30_000 {
            line.extend(["(", "x", "-a"].map(String::from));
        }
        line.push(String::from("x"));
        line.extend((0..30_000).map(|_| String::from(")")));
        let args: Vec<&str> = line.iter().map(String::as_str).collect();
        let gnu = host_tool(&args, &[], b"");
        let ours = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let args: Vec<&str> = line.iter().map(String::as_str).collect();
                Harness::new().like_host(&args, &[], b"")
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!((ours, gnu.0), ((0, String::new(), String::new()), 0));
    }

    #[test]
    fn bracket_needs_its_bracket_and_refuses_help_and_version() {
        let mut h = Harness::new();
        for (line, said) in [
            ("[", "[: missing ']'\n"),
            ("[ x", "[: missing ']'\n"),
            ("[ ] x", "[: missing ']'\n"),
            ("[ x ]]", "[: missing ']'\n"),
            // GNU 9.4 prints its help or version here (probes g1); these
            // are refused as other commands refuse an option (§15 item 3).
            ("[ --help", "[: unrecognized option '--help'\n"),
            ("[ --version", "[: unrecognized option '--version'\n"),
        ] {
            assert_eq!(h.run(line), (2, String::from(said)), "{line}");
        }
        // Not alone, they are strings, as in GNU.
        like_gnu(&[&["--help"], &["--version"], &["--help", "=", "--help"]]);
        assert_eq!(h.run("test --help"), (0, String::new()));
        // GNU's `[` sees the `]` past the expression's end (probes g6).
        assert_eq!(
            h.run("[ '(' x -a y ]"),
            (2, String::from("[: ')' expected, found ']'\n"))
        );
        assert_eq!(
            h.run("test '(' x -a y"),
            (2, String::from("test: ')' expected\n"))
        );
    }

    #[test]
    fn newer_older_and_the_same_file_are_gnu_s() {
        let files = [
            TestFile::file("old", b"o").times(100, 1_000),
            TestFile::file("new", b"n").times(100, 2_000),
            TestFile::file("same", b"s").times(100, 2_000),
            TestFile::hard_link("link", "old"),
            TestFile::dir("d").times(100, 1_500),
        ];
        for args in [
            ["new", "-nt", "old"],
            ["old", "-nt", "new"],
            ["new", "-nt", "same"],
            ["new", "-nt", "nope"],
            ["nope", "-nt", "new"],
            ["nope", "-nt", "nope"],
            ["old", "-ot", "new"],
            ["new", "-ot", "old"],
            ["old", "-ot", "nope"],
            ["nope", "-ot", "old"],
            ["d", "-nt", "old"],
            ["old", "-ef", "link"],
            ["link", "-ef", "old"],
            ["old", "-ef", "old"],
            ["old", "-ef", "new"],
            ["old", "-ef", "nope"],
            ["d", "-ef", "d/."],
            ["d", "-ef", "."],
            [".", "-ef", "d/.."],
        ] {
            for name in ["test", "["] {
                let mut line = alloc::vec![name];
                line.extend(args);
                if name == "[" {
                    line.push("]");
                }
                assert_eq!(
                    like_host_files(&line, &files),
                    host_files(&line, &files, false),
                    "{line:?}"
                );
            }
        }
    }

    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
        let name = |arg0: &[u8]| named(arg0).0;
        assert_eq!(name(b"["), "[");
        assert_eq!(name(b"/bin/["), "[");
        assert_eq!(name(b"../bin/["), "[");
        assert_eq!(name(b"test"), "test");
        assert_eq!(name(b"/bin/test"), "test");
        assert_eq!(name(b"[/test"), "test");
        assert_eq!(name(b"[["), "test");
        assert_eq!(name(b""), "test");
    }
}
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
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
````

with:

````rust
        let mut out = FakeStdout::file(None);
        let (status, errors) = self.program_args(args, &mut out);
        self.vfs.chdir(b"/").unwrap();
        (status, out.text(), errors)
    }

    /// As [`Harness::program`], with the words given (the command's name
    /// first), so that any byte can be in one.
    pub fn program_args(&mut self, args: &[&str], stdout: &mut FakeStdout) -> (i32, String) {
        let words: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
        let status = crate::run_command(
            &words[0],
            crate::commands::find(&words[0]).unwrap().run,
            &words[1..],
            crate::CommandIo {
                vfs: &mut self.vfs,
                console: &mut self.console,
                system: &mut self.system,
                stdin: &mut Bytes::new(core::mem::take(&mut self.stdin)),
                stdout,
            },
        );
        (status, self.console.take())
    }
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

/// A file a test makes on both sides, in a fresh directory: on the host
/// for its own tool ([`host_files`]) and in a `MemFs` for ours
/// ([`like_host_files`]).
#[derive(Clone, Copy, Debug)]
pub struct TestFile<'a> {
    pub name: &'a str,
    pub made: Made<'a>,
    /// Access and modification times, in seconds since 1970.
    pub times: Option<(u64, u64)>,
}

/// What a [`TestFile`] is.
#[derive(Clone, Copy, Debug)]
pub enum Made<'a> {
    File(&'a [u8]),
    Dir,
    /// Another name for the file of that name, made before it.
    HardLink(&'a str),
}

impl<'a> TestFile<'a> {
    pub fn file(name: &'a str, data: &'a [u8]) -> TestFile<'a> {
        TestFile::made(name, Made::File(data))
    }

    pub fn dir(name: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::Dir)
    }

    pub fn hard_link(name: &'a str, to: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::HardLink(to))
    }

    fn made(name: &'a str, made: Made<'a>) -> TestFile<'a> {
        TestFile {
            name,
            made,
            times: None,
        }
    }

    pub fn times(self, atime: u64, mtime: u64) -> TestFile<'a> {
        TestFile {
            times: Some((atime, mtime)),
            ..self
        }
    }
}

/// Runs `args` (the command's name first) as its program, in `/w` of a
/// fresh standard tree holding `files`: its status, standard output and
/// standard error, to compare with [`host_files`]'.
pub fn like_host_files(args: &[&str], files: &[TestFile<'_>]) -> (i32, String, String) {
    let mut fs = memfs();
    let w = fs.mkdir(fs.root(), b"w").unwrap();
    let mut made: Vec<(&str, Ino)> = Vec::new();
    for f in files {
        let (dir, name) = match f.name.rsplit_once('/') {
            Some((d, n)) => (lookup_in(&mut fs, w, d), n),
            None => (w, f.name),
        };
        let ino = match f.made {
            Made::File(data) => {
                let ino = fs.create(dir, name.as_bytes()).unwrap();
                fs.write_at(ino, 0, data).unwrap();
                ino
            }
            Made::Dir => fs.mkdir(dir, name.as_bytes()).unwrap(),
            Made::HardLink(to) => {
                let ino = made.iter().find(|m| m.0 == to).unwrap().1;
                fs.link(dir, name.as_bytes(), ino).unwrap();
                ino
            }
        };
        made.push((f.name, ino));
    }
    // Last, as on the host: making a file changes its directory's times.
    for (f, &(_, ino)) in files.iter().zip(&made) {
        if let Some((atime, mtime)) = f.times {
            fs.set_times(ino, atime, mtime).unwrap();
        }
    }
    let mut h = Harness::on(fs);
    h.vfs.chdir(b"/w").unwrap();
    let mut out = FakeStdout::file(None);
    let (status, errors) = h.program_args(args, &mut out);
    (status, out.text(), errors)
}

/// The directory `path` under `dir`.
fn lookup_in(fs: &mut MemFs, dir: Ino, path: &str) -> Ino {
    path.split('/')
        .fold(dir, |d, name| fs.lookup(d, name.as_bytes()).unwrap())
}

/// What the host's own tool prints for `args` (its name first), as
/// [`host_tool`] runs it, in a fresh directory holding `files`; with
/// `root`, through `unshare -r`, so that it answers as root (Relay OS runs
/// every program as root). A missing tool, or a namespace refused, fails
/// the test.
pub fn host_files(args: &[&str], files: &[TestFile<'_>], root: bool) -> (i32, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/like-host")
        .join(std::format!("{}-{}", std::process::id(), next_dir()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        let path = dir.join(f.name);
        match f.made {
            Made::File(data) => std::fs::write(&path, data).unwrap(),
            Made::Dir => std::fs::create_dir(&path).unwrap(),
            Made::HardLink(to) => std::fs::hard_link(dir.join(to), &path).unwrap(),
        }
    }
    for f in files {
        if let Some((atime, mtime)) = f.times {
            for (flag, t) in [("-a", atime), ("-m", mtime)] {
                let ok = std::process::Command::new("touch")
                    .args(["-h", flag, "-d", &std::format!("@{t}")])
                    .arg(dir.join(f.name))
                    .status()
                    .expect("the host's touch is needed")
                    .success();
                assert!(ok, "touch {flag} {}", f.name);
            }
        }
    }
    let mut cmd = if root {
        let mut c = std::process::Command::new("unshare");
        c.arg("-r").args(args);
        c
    } else {
        let mut c = std::process::Command::new(args[0]);
        c.args(&args[1..]);
        c
    };
    let out = cmd
        .current_dir(&dir)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    let _ = std::fs::remove_dir_all(&dir);
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    let stderr = text(out.stderr);
    if root {
        assert!(
            !stderr.starts_with("unshare:"),
            "unshare -r is needed (on Ubuntu 24.04, sysctl \
             kernel.apparmor_restrict_unprivileged_userns=0): {stderr}"
        );
    }
    (out.status.code().unwrap_or(-1), text(out.stdout), stderr)
}

````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` unresolved import `super::named` ``.

- [ ] **Step 5: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub use system::{date, df, dmesg, free, poweroff, ps, reboot, sleep, sync};
pub use text::{cat, head, tail, wc};
````

with:

````rust
pub use system::{date, df, dmesg, free, poweroff, ps, reboot, sleep, sync};
pub use test::{bracket, named};
pub use text::{cat, head, tail, wc};
````

Replace:

````rust
pub const COMMANDS: &[Builtin] = &[
    Builtin {
````

with:

````rust
pub const COMMANDS: &[Builtin] = &[
    Builtin {
        name: "[",
        help: "evaluate an expression, as test does, up to a ]",
        run: test::bracket,
    },
    Builtin {
````

Replace:

````rust
    Builtin {
        name: "touch",
````

with:

````rust
    Builtin {
        name: "test",
        help: "evaluate an expression: 0 if true, 1 if false",
        run: test::test,
    },
    Builtin {
        name: "touch",
````

- [ ] **Step 6: Implement `crates/shell/src/commands/test.rs`**

Insert this at the top of `crates/shell/src/commands/test.rs`, above `#[cfg(test)]`:

````rust
//! `test EXPRESSION` and `[ EXPRESSION ]` (spec §6, §15 item 3): GNU
//! coreutils 9.4's grammar rule for rule, its messages word for word in
//! the C locale, status 0 if the expression is true, 1 if it is false and
//! 2 after an error. One evaluator under two names: called as `[`, the last
//! argument must be `]`, and `[ --help` and `[ --version` are refused
//! where GNU prints its help.
//!
//! The rules by argument count come first, for 1 to 4 arguments, then
//! `-o` over `-a` over terms; a term is a run of `!`, then `(` and an
//! expression of up to 4 arguments before its `)` (or of all the rest), a
//! binary operator if the argument after next is one, a unary operator of
//! the form `-X`, or a string. Strings are compared as bytes; GNU's program
//! has no `<` or `>`, which bash's built-in has. Integers are read as GNU
//! reads them (blanks around, a sign, any number of digits) and compared as
//! digit strings, so nothing overflows; `-l STRING` stands for STRING's
//! length. Files are answered from `stat`, as for root. The evaluator never
//! recurses: each open `(` waits on a stack of its own, so it nests as deep
//! as the arguments go, as GNU does on its larger stack.

use crate::commands::Run;
use crate::ctx::Ctx;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;

pub fn test(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    evaluate(ctx, "test", args, None)
}

pub fn bracket(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let [only] = args
        && (only == "--help" || only == "--version")
    {
        ctx.err(format!("[: unrecognized option '{only}'\n").as_bytes());
        return 2;
    }
    match args.split_last() {
        Some((last, rest)) if last == "]" => evaluate(ctx, "[", rest, Some("]")),
        _ => {
            ctx.err(b"[: missing ']'\n");
            2
        }
    }
}

/// `/bin/test`'s name and command function: `[` when the last part of the
/// path it was started by is `[`, as the shells give a command's name as
/// typed (one program under two names, spec §15 item 3).
pub fn named(arg0: &[u8]) -> (&'static str, Run) {
    match arg0.rsplit(|&b| b == b'/').next() {
        Some(b"[") => ("[", bracket),
        _ => ("test", test),
    }
}

/// Runs the expression `args`; `past` is what GNU's `[` still sees past
/// its end (the `]`).
fn evaluate(ctx: &mut Ctx<'_>, name: &str, args: &[String], past: Option<&str>) -> i32 {
    if args.is_empty() {
        return 1;
    }
    let mut e = Eval {
        ctx,
        args,
        past,
        pos: 0,
    };
    let answer = e
        .expression(args.len())
        .and_then(|v| match args.get(e.pos) {
            Some(extra) => Err(format!("extra argument {}", quote(extra))),
            None => Ok(v),
        });
    match answer {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            e.ctx.err(format!("{name}: {message}\n").as_bytes());
            2
        }
    }
}

/// A truth value, or GNU's message for an expression it cannot read.
type Answer = Result<bool, String>;

/// What waits for a value: an `-o` or `-a` list (its value so far), a `(`
/// (whether a `!` came before it), or the `!` of the 4-argument rule.
enum Frame {
    Or(bool),
    And(bool),
    Paren(bool),
    Not,
}

/// What to read next: an expression of so many arguments (GNU's
/// `posixtest`), the 3-argument rule, an `-o` list, or a term.
enum Step {
    Count(usize),
    Three,
    Expr,
    Term,
}

struct Eval<'e, 'c> {
    ctx: &'e mut Ctx<'c>,
    args: &'e [String],
    past: Option<&'e str>,
    /// The next argument.
    pos: usize,
}

impl<'e> Eval<'e, '_> {
    /// GNU's `posixtest(n)`, its recursion turned into `stack`.
    fn expression(&mut self, n: usize) -> Answer {
        let mut stack = Vec::new();
        let mut step = Step::Count(n);
        loop {
            let mut value = match step {
                Step::Count(1) => self.one(),
                Step::Count(2) => self.two()?,
                Step::Count(3) => {
                    step = Step::Three;
                    continue;
                }
                Step::Count(4) if self.is(self.pos, "!") => {
                    self.advance(true)?;
                    stack.push(Frame::Not);
                    step = Step::Three;
                    continue;
                }
                Step::Count(4) if self.is(self.pos, "(") && self.is(self.pos + 3, ")") => {
                    self.pos += 1;
                    let v = self.two()?;
                    self.pos += 1;
                    v
                }
                Step::Count(_) => {
                    step = Step::Expr;
                    continue;
                }
                Step::Three => match self.three()? {
                    Some(v) => v,
                    None => {
                        step = Step::Expr;
                        continue;
                    }
                },
                Step::Expr => {
                    if self.pos >= self.args.len() {
                        return Err(self.beyond());
                    }
                    stack.push(Frame::Or(false));
                    stack.push(Frame::And(true));
                    step = Step::Term;
                    continue;
                }
                Step::Term => {
                    let mut negated = false;
                    while self.is(self.pos, "!") {
                        self.advance(true)?;
                        negated = !negated;
                    }
                    if self.pos >= self.args.len() {
                        return Err(self.beyond());
                    }
                    if self.is(self.pos, "(") {
                        self.advance(true)?;
                        stack.push(Frame::Paren(negated));
                        step = Step::Count(self.in_parens());
                        continue;
                    }
                    negated ^ self.operand()?
                }
            };
            // Hand the value back until a list takes another term.
            loop {
                match stack.pop() {
                    None => return Ok(value),
                    Some(Frame::Not) => value = !value,
                    Some(Frame::Paren(negated)) => {
                        self.close()?;
                        value ^= negated;
                    }
                    Some(Frame::And(so_far)) => {
                        let so_far = so_far & value;
                        if self.is(self.pos, "-a") {
                            self.pos += 1;
                            stack.push(Frame::And(so_far));
                            step = Step::Term;
                            break;
                        }
                        value = so_far;
                    }
                    Some(Frame::Or(so_far)) => {
                        let so_far = so_far | value;
                        if self.is(self.pos, "-o") {
                            self.pos += 1;
                            stack.push(Frame::Or(so_far));
                            stack.push(Frame::And(true));
                            step = Step::Term;
                            break;
                        }
                        value = so_far;
                    }
                }
            }
        }
    }

    fn arg(&self, i: usize) -> Option<&'e str> {
        self.args.get(i).map(String::as_str)
    }

    fn is(&self, i: usize, word: &str) -> bool {
        self.arg(i) == Some(word)
    }

    /// Moves to the next argument; with `needed`, one must be there.
    fn advance(&mut self, needed: bool) -> Result<(), String> {
        self.pos += 1;
        if needed && self.pos >= self.args.len() {
            return Err(self.beyond());
        }
        Ok(())
    }

    fn beyond(&self) -> String {
        let last = self.args.last().map_or("", String::as_str);
        format!("missing argument after {}", quote(last))
    }

    fn one(&mut self) -> bool {
        let v = self.arg(self.pos).is_some_and(|a| !a.is_empty());
        self.pos += 1;
        v
    }

    fn two(&mut self) -> Answer {
        if self.is(self.pos, "!") {
            self.pos += 1;
            return Ok(!self.one());
        }
        if unary_form(self.arg(self.pos)) {
            return self.unary();
        }
        Err(self.beyond())
    }

    /// GNU's 3-argument rule; `None` where it reads an `-a` or `-o` list.
    fn three(&mut self) -> Result<Option<bool>, String> {
        let p = self.pos;
        if binop(self.arg(p + 1)) {
            return self.binary(false).map(Some);
        }
        if self.is(p, "!") {
            self.advance(true)?;
            return self.two().map(|v| Some(!v));
        }
        if self.is(p, "(") && self.is(p + 2, ")") {
            self.pos += 1;
            let v = self.one();
            self.pos += 1;
            return Ok(Some(v));
        }
        if self.is(p + 1, "-a") || self.is(p + 1, "-o") {
            return Ok(None);
        }
        let op = self.arg(p + 1).unwrap_or("");
        Err(format!("{}: binary operator expected", quote(op)))
    }

    /// How many arguments the expression after a `(` has: up to its `)`
    /// if one comes within 4, else all the rest.
    fn in_parens(&self) -> usize {
        let mut n = 1;
        while self.pos + n < self.args.len() && !self.is(self.pos + n, ")") {
            if n == 4 {
                return self.args.len() - self.pos;
            }
            n += 1;
        }
        n
    }

    /// The `)` after an expression in parentheses.
    fn close(&mut self) -> Result<(), String> {
        let at = match self.arg(self.pos) {
            None if self.pos == self.args.len() => self.past,
            found => found,
        };
        match at {
            None => Err(format!("{} expected", quote(")"))),
            Some(")") => {
                self.pos += 1;
                Ok(())
            }
            Some(other) => Err(format!("{} expected, found {}", quote(")"), quote(other))),
        }
    }

    /// A term after its `!`s and `(`: a binary or unary operator's, or a
    /// string.
    fn operand(&mut self) -> Answer {
        let (p, left) = (self.pos, self.args.len() - self.pos);
        if left >= 4 && self.is(p, "-l") && binop(self.arg(p + 2)) {
            return self.binary(true);
        }
        if left >= 3 && binop(self.arg(p + 1)) {
            return self.binary(false);
        }
        if unary_form(self.arg(p)) {
            return self.unary();
        }
        Ok(self.one())
    }

    /// The operand after a unary operator, which must be there.
    fn unary_operand(&mut self) -> Result<&'e str, String> {
        self.advance(true)?;
        self.pos += 1;
        Ok(&self.args[self.pos - 1])
    }

    fn unary(&mut self) -> Answer {
        let op = &self.args[self.pos];
        match op.as_bytes()[1] {
            b'n' => Ok(!self.unary_operand()?.is_empty()),
            b'z' => Ok(self.unary_operand()?.is_empty()),
            _ => Err(format!("{}: unary operator expected", quote(op))),
        }
    }

    /// GNU's `binary_operator`, at the left operand (or at the `-l`
    /// before it, `left_is_l`).
    fn binary(&mut self, left_is_l: bool) -> Answer {
        if left_is_l {
            self.pos += 1;
        }
        let op = self.pos + 1;
        let right_is_l = op + 2 < self.args.len() && self.is(op + 1, "-l");
        if right_is_l {
            self.pos += 1;
        }
        let which = self.args[op].as_str();
        if let Some(compare) = integer_operator(which) {
            let left = match left_is_l {
                true => Number::length(&self.args[op - 1]),
                false => Number::parse(&self.args[op - 1])?,
            };
            let right = match right_is_l {
                true => Number::length(&self.args[op + 2]),
                false => Number::parse(&self.args[op + 1])?,
            };
            self.pos += 3;
            return Ok(compare(left.cmp(&right)));
        }
        if which.starts_with('-') {
            self.pos += 3;
            if left_is_l || right_is_l {
                return Err(format!("{which} does not accept -l"));
            }
            let (a, b) = (&self.args[op - 1], &self.args[op + 1]);
            return Ok(match which {
                "-nt" => self.newer(a, b),
                "-ot" => self.newer(b, a),
                _ => self.same_file(a, b),
            });
        }
        // GNU compares the arguments at pos and pos + 2, which a `-l` on
        // the right has moved.
        let same = self.args[self.pos] == self.args[self.pos + 2];
        self.pos += 3;
        Ok(if which == "!=" { !same } else { same })
    }

    /// Whether `a` is newer than `b`: `a` exists and `b` does not, or is
    /// older (GNU's `-nt`; `-ot` is it turned round).
    fn newer(&mut self, a: &str, b: &str) -> bool {
        let mtime = |e: &mut Self, path| e.stat(path).map(|(_, s)| s.mtime);
        match (mtime(self, a), mtime(self, b)) {
            (Some(a), Some(b)) => a > b,
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Whether `a` and `b` are one file: one filesystem, one inode.
    fn same_file(&mut self, a: &str, b: &str) -> bool {
        match (self.stat(a), self.stat(b)) {
            (Some((a, _)), Some((b, _))) => a == b,
            _ => false,
        }
    }

    /// `path`'s node and `stat`, if it exists; a symbolic link is not
    /// followed (§10).
    fn stat(&mut self, path: &str) -> Option<(vfs::Node, vfs::Stat)> {
        let node = self.ctx.vfs.lookup(path.as_bytes()).ok()?;
        let stat = self.ctx.vfs.stat(node).ok()?;
        Some((node, stat))
    }
}

/// Whether `arg` has a unary operator's form: `-` and one byte.
fn unary_form(arg: Option<&str>) -> bool {
    arg.is_some_and(|a| a.len() == 2 && a.starts_with('-'))
}

/// GNU's binary operators.
fn binop(arg: Option<&str>) -> bool {
    matches!(
        arg,
        Some(
            "=" | "!="
                | "=="
                | "-nt"
                | "-ot"
                | "-ef"
                | "-eq"
                | "-ne"
                | "-lt"
                | "-le"
                | "-gt"
                | "-ge"
        )
    )
}

/// What an integer operator asks of the comparison.
fn integer_operator(op: &str) -> Option<fn(Ordering) -> bool> {
    Some(match op {
        "-eq" => Ordering::is_eq,
        "-ne" => Ordering::is_ne,
        "-lt" => Ordering::is_lt,
        "-le" => Ordering::is_le,
        "-gt" => Ordering::is_gt,
        "-ge" => Ordering::is_ge,
        _ => return None,
    })
}

/// An integer as GNU's `find_int` reads it: its sign and its digits
/// without leading zeros (none for zero, which has no sign).
#[derive(PartialEq, Eq)]
struct Number {
    negative: bool,
    digits: String,
}

impl Number {
    /// Blanks (spaces and tabs), then `+` or `-`, at least one digit, then
    /// blanks; GNU's `invalid integer 'X'` for anything else.
    fn parse(s: &str) -> Result<Number, String> {
        let blank = |c: char| c == ' ' || c == '\t';
        let rest = s.trim_start_matches(blank);
        let (negative, rest) = match rest.as_bytes().first() {
            Some(b'+') => (false, &rest[1..]),
            Some(b'-') => (true, &rest[1..]),
            _ => (false, rest),
        };
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let (digits, after) = rest.split_at(end);
        if digits.is_empty() || !after.trim_start_matches(blank).is_empty() {
            return Err(format!("invalid integer {}", quote(s)));
        }
        let digits = digits.trim_start_matches('0');
        Ok(Number {
            negative: negative && !digits.is_empty(),
            digits: String::from(digits),
        })
    }

    /// `-l`'s length of `s`, in bytes.
    fn length(s: &str) -> Number {
        Number::parse(&s.len().to_string()).unwrap_or(Number {
            negative: false,
            digits: String::new(),
        })
    }
}

impl Ord for Number {
    fn cmp(&self, other: &Number) -> Ordering {
        let size = |n: &Number| (n.digits.len(), n.digits.clone());
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => size(self).cmp(&size(other)),
            (true, true) => size(other).cmp(&size(self)),
        }
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Number) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `arg` as GNU's `quote()` shows it in the C locale: in single quotes, a
/// quote or backslash escaped, control bytes by their C names, and every
/// other byte outside printable ASCII in octal (`'\303\251'` for `é`).
fn quote(arg: &str) -> String {
    let mut q = String::from("'");
    for &b in arg.as_bytes() {
        match b {
            b'\'' => q.push_str("\\'"),
            b'\\' => q.push_str("\\\\"),
            0x07 => q.push_str("\\a"),
            0x08 => q.push_str("\\b"),
            0x0c => q.push_str("\\f"),
            b'\n' => q.push_str("\\n"),
            b'\r' => q.push_str("\\r"),
            b'\t' => q.push_str("\\t"),
            0x0b => q.push_str("\\v"),
            0x20..=0x7e => q.push(char::from(b)),
            _ => q.push_str(&format!("\\{b:03o}")),
        }
    }
    q.push('\'');
    q
}

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 369 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add test and [ with GNU's grammar

`test` and `[` are one evaluator under two names (spec §6, §15 item 3),
both in COMMANDS, so both run in the shell's tests and in host-shell;
/bin gets them later. The grammar is GNU coreutils 9.4's, rule for rule:
the rules by argument count, then -o over -a over terms, with GNU's
messages, quoted as its quote() does in the C locale. Strings compare as
bytes, without < or >, which GNU's program lacks; integers are read as
GNU reads them and compared as digit strings, of any length; -l STRING
is its length; -nt, -ot and -ef answer from stat. Open parentheses wait
on a stack of the evaluator's own, so they nest as deep as GNU's do.
`[` needs its `]` and refuses --help and --version alone. The tests
compare every case with the host's /usr/bin/test and /usr/bin/[; the
harness's like_host passes words without parsing them, and files with
times are made on both sides.
EOF
````


### Task 4: `test`'s file operators answer as GNU's do for root

Spec §6.2 and decisions 7 and 9: every unary file operator but `-t`, from `stat` as GNU answers for root: `-r` (and `-e`) true for a file that exists, `-w` too unless the file is on `/bin`'s filesystem (`on_bin`, by the node's mount: no call says a filesystem is read-only, so a root mounted read-only answers true, a decided difference, §10), `-x` for any execute bit or a directory, `-O` and `-G` for user and group 0, `-N` for a modification after the last access, and the kinds, sizes and mode bits. A symbolic link is never followed (§10): `-e`, `-f` and `-d` look at the link itself, and the test names GNU's answer beside ours. `TestFile` gains a mode, links, FIFOs (the host's `mkfifo`) and sockets, set on both sides with every mode last (a directory's entries before it) and every file's times given, so that neither the umask nor the clock decides a case; each directory holds a file, so that `-s` does not depend on the size the host's filesystem gives an empty one (the review's M4). The module comment names both decided differences (the review's M7), and `README.md` says the tests need unprivileged user namespaces (M6). The red run is the shell's tests: each operator is `'-e': unary operator expected`, and the `/bin` test's harness cannot be built. Mutation checks (19): the `/bin` rule and its mount comparison, `-r`, `-x` on directories and on each bit, `-f`, `-h`, `-p`, `-S`, `-b`, `-c`, `-s`, `-u`, `-g`, `-k`, `-O`, `-G`, `-N`, and a missing file, each broken, fail a test.

**Files:**
- Modify: `README.md`
- Modify: `crates/shell/src/commands/test.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 3's evaluator and harness; Task 1's `MemFs` setters; Task 2's namespaces on CI.
- Produces: `Eval::on_bin` (private); `testing::TestFile::{link, fifo, socket, mode}`, `TestFile.mode: Option<u16>`, `testing::Made::{Link, Fifo, Socket}`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use super::named;
    use crate::testing::{Harness, TestFile, host_files, host_tool, like_host_files};
    use alloc::string::String;
    use alloc::vec::Vec;

````

with:

````rust
    use super::named;
    use crate::testing::{Harness, TestFile, host_files, host_tool, like_host_files, memfs};
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec::Vec;
    use vfs::{FileSystem, FileType, Vfs};

    /// Every unary operator on files, `-t` aside.
    const FILE_OPERATORS: [&str; 19] = [
        "-e", "-f", "-d", "-s", "-r", "-w", "-x", "-O", "-G", "-N", "-u", "-g", "-k", "-p", "-S",
        "-b", "-c", "-h", "-L",
    ];

    /// `op path` under both names, as GNU answers it (as root, with
    /// `files` made on the host) and as ours does.
    fn unary_like_gnu(files: &[TestFile<'_>], op: &str, path: &str) {
        for name in ["test", "["] {
            let mut line = alloc::vec![name, op, path];
            if name == "[" {
                line.push("]");
            }
            assert_eq!(
                like_host_files(&line, files),
                host_files(&line, files, true),
                "{line:?}"
            );
        }
    }

````

Replace:

````rust
    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
````

with:

````rust
    #[test]
    fn file_operators_answer_as_gnu_s_do_for_root() {
        // GNU's answers as root, through `unshare -r` (spec §15 item 3):
        // probe u1, `-r` and `-w` true without the bits, `-x` true for a
        // directory. Every file has its times, so that `-N` does not
        // depend on the host's clock.
        let files = [
            TestFile::file("f", b"data"),
            TestFile::file("empty", b""),
            TestFile::file("x", b"#").mode(0o100),
            TestFile::file("g", b"#").mode(0o010),
            TestFile::file("none", b"x").mode(0o000),
            TestFile::file("su", b"x").mode(0o4755),
            TestFile::file("sg", b"x").mode(0o2644),
            TestFile::file("read", b"x").times(1_000, 2_000),
            TestFile::file("unread", b"x").times(2_000, 1_000),
            TestFile::file("same", b"x").times(2_000, 2_000),
            // Each directory holds a file, so that `-s` does not depend on
            // the size the host's filesystem gives an empty one.
            TestFile::dir("d"),
            TestFile::file("d/in", b""),
            TestFile::dir("closed").mode(0o600),
            TestFile::file("closed/in", b""),
            TestFile::dir("search").mode(0o100),
            TestFile::file("search/in", b""),
            TestFile::dir("sticky").mode(0o1777),
            TestFile::file("sticky/in", b""),
            TestFile::fifo("p"),
            TestFile::socket("s"),
        ];
        let names = [
            "f", "empty", "x", "g", "none", "su", "sg", "read", "unread", "same", "d", "closed",
            "search", "sticky", "p", "s", "nope", "", "d/", "f/",
        ];
        let files = files.map(|f| match f.times {
            Some(_) => f,
            None => f.times(5_000, 5_000),
        });
        for op in FILE_OPERATORS {
            for path in names {
                unary_like_gnu(&files, op, path);
            }
        }
        // Missing their file: GNU's message.
        like_gnu(&[
            &["-e"],
            &["!", "-f"],
            &["-d", "-a", "x"],
            &["-x", "-o", "-f"],
        ]);
    }

    #[test]
    fn devices_answer_as_gnu_s_do() {
        // The host's /dev/null and a block device of its own stand for
        // the harness's.
        let block = std::fs::read_dir("/dev")
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| {
                use std::os::unix::fs::FileTypeExt;
                e.file_type().is_ok_and(|t| t.is_block_device())
            })
            .expect("a block device in the host's /dev")
            .path();
        let block = block.to_str().unwrap();
        for (host, kind) in [
            ("/dev/null", FileType::CharDev),
            (block, FileType::BlockDev),
        ] {
            for op in ["-e", "-f", "-d", "-p", "-S", "-b", "-c", "-h", "-L"] {
                let mut fs = memfs();
                let root = fs.root();
                fs.special(root, b"dev", kind).unwrap();
                let mut h = Harness::on(fs);
                let line = alloc::format!("test {op} /dev");
                let gnu = host_tool(&["test", op, host], &[], b"");
                assert_eq!(h.run(&line), (gnu.0, gnu.2), "{op} {host}");
            }
        }
    }

    #[test]
    fn a_symbolic_link_is_never_followed() {
        // A decided difference (spec §6.2, §10): GNU's operators but -h
        // and -L look at what the link names; these look at the link.
        let files = [
            TestFile::file("f", b"data"),
            TestFile::dir("d"),
            TestFile::link("lf", "f"),
            TestFile::link("ld", "d"),
            TestFile::link("dangling", "nope"),
        ];
        for path in ["lf", "ld", "dangling"] {
            unary_like_gnu(&files, "-h", path);
            unary_like_gnu(&files, "-L", path);
        }
        for (op, path, ours, gnu) in [
            ("-e", "dangling", 0, 1),
            ("-f", "lf", 1, 0),
            ("-d", "ld", 1, 0),
            ("-r", "dangling", 0, 1),
            ("-x", "lf", 0, 1),
            ("-s", "dangling", 0, 1),
        ] {
            let line = ["test", op, path];
            assert_eq!(host_files(&line, &files, true).0, gnu, "{line:?}");
            assert_eq!(like_host_files(&line, &files).0, ours, "{line:?}");
        }
        let line = ["test", "lf", "-ef", "f"];
        assert_eq!(host_files(&line, &files, false).0, 0);
        assert_eq!(like_host_files(&line, &files).0, 1);
    }

    #[test]
    fn nothing_on_bin_s_filesystem_is_writable() {
        // `/bin` is read-only (system.img), which no call tells: a file
        // on its filesystem is not writable (spec §15 item 3).
        let mut programs = memfs();
        let root = programs.root();
        let ls = programs.create(root, b"ls").unwrap();
        programs.set_mode(ls, 0o755).unwrap();
        let mut h = Harness::new();
        h.vfs.mkdir(b"/bin").unwrap();
        h.vfs
            .mount(b"/bin", Box::new(programs.read_only()))
            .unwrap();
        for (line, status) in [
            ("test -w /bin/ls", 1),
            ("[ -w /bin ]", 1),
            ("test -w /bin/.", 1),
            ("test -w /bin/..", 0),
            ("test -w /root", 0),
            ("test -w /etc/motd", 0),
            ("test -r /bin/ls", 0),
            ("test -x /bin/ls", 0),
            ("test -f /bin/ls", 0),
            // One inode number, two filesystems.
            ("test /bin/ls -ef /etc", 1),
            ("test /bin/ls -ef /bin/ls", 0),
            ("test /bin -ef /bin/.", 0),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
        let etc = h.vfs.lookup(b"/etc").unwrap();
        assert_eq!(etc.ino, h.vfs.lookup(b"/bin/ls").unwrap().ino);
    }

    #[test]
    fn owned_by_another_is_not_owned_by_root() {
        // Relay OS runs every program as root; a file another made (on a
        // disk written elsewhere) is not its.
        let mut fs = memfs();
        let root = fs.root();
        for (name, uid, gid) in [
            (&b"theirs"[..], 1000, 0),
            (b"group", 0, 100),
            (b"mine", 0, 0),
        ] {
            let ino = fs.create(root, name).unwrap();
            fs.set_owner(ino, uid, gid).unwrap();
        }
        let mut h = Harness::on(fs);
        for (line, status) in [
            ("test -O /theirs", 1),
            ("test -G /theirs", 0),
            ("test -O /group", 0),
            ("test -G /group", 1),
            ("test -O /mine", 0),
            ("test -G /mine", 0),
            ("test -O /nope", 1),
            ("test -G /nope", 1),
        ] {
            assert_eq!(h.run(line), (status, String::new()), "{line}");
        }
    }

    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
use relay_abi::WaitStatus;
use vfs::{DirEntry, Env, Errno, FileSystem, Ino, MemFs, MountTable, Node, Stat, StatFs, Vfs};

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
    pub times: Option<(u64, u64)>,
}
````

with:

````rust
    pub times: Option<(u64, u64)>,
    /// The mode's permission bits (`0o644` for a file, a FIFO or a socket
    /// and `0o755` for a directory unless given; a link's are its own).
    pub mode: Option<u16>,
}
````

Replace:

````rust
    HardLink(&'a str),
}
````

with:

````rust
    HardLink(&'a str),
    /// A symbolic link to that path.
    Link(&'a str),
    Fifo,
    Socket,
}

impl Made<'_> {
    /// The mode a test file has unless one is given.
    fn mode(&self) -> Option<u16> {
        match self {
            Made::Dir => Some(0o755),
            Made::HardLink(_) | Made::Link(_) => None,
            _ => Some(0o644),
        }
    }
}
````

Replace:

````rust

    fn made(name: &'a str, made: Made<'a>) -> TestFile<'a> {
````

with:

````rust

    pub fn link(name: &'a str, to: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::Link(to))
    }

    pub fn fifo(name: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::Fifo)
    }

    pub fn socket(name: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::Socket)
    }

    fn made(name: &'a str, made: Made<'a>) -> TestFile<'a> {
````

Replace:

````rust
            times: None,
        }
    }
````

with:

````rust
            times: None,
            mode: None,
        }
    }

    pub fn mode(self, mode: u16) -> TestFile<'a> {
        TestFile {
            mode: Some(mode),
            ..self
        }
    }

    /// Its mode, if it is one to set.
    fn chmod(&self) -> Option<u16> {
        self.mode.or(self.made.mode())
    }
````

Replace:

````rust
            }
        };
````

with:

````rust
            }
            Made::Link(to) => fs.symlink(dir, name.as_bytes(), to.as_bytes()).unwrap(),
            Made::Fifo => fs.special(dir, name.as_bytes(), FileType::Fifo).unwrap(),
            Made::Socket => fs.special(dir, name.as_bytes(), FileType::Socket).unwrap(),
        };
````

Replace:

````rust
            fs.set_times(ino, atime, mtime).unwrap();
        }
````

with:

````rust
            fs.set_times(ino, atime, mtime).unwrap();
        }
        if let Some(mode) = f.chmod() {
            fs.set_mode(ino, mode).unwrap();
        }
````

Replace:

````rust
            Made::HardLink(to) => std::fs::hard_link(dir.join(to), &path).unwrap(),
        }
````

with:

````rust
            Made::HardLink(to) => std::fs::hard_link(dir.join(to), &path).unwrap(),
            Made::Link(to) => std::os::unix::fs::symlink(to, &path).unwrap(),
            Made::Fifo => {
                let ok = std::process::Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .expect("the host's mkfifo is needed")
                    .success();
                assert!(ok, "mkfifo {}", f.name);
            }
            // The socket's file stays when the listener goes.
            Made::Socket => drop(std::os::unix::net::UnixListener::bind(&path).unwrap()),
        }
````

Replace:

````rust
                assert!(ok, "touch {flag} {}", f.name);
            }
        }
    }
````

with:

````rust
                assert!(ok, "touch {flag} {}", f.name);
            }
        }
    }
    // Last, and a directory's entries before it, so that a directory
    // without write or search bits is made full.
    for f in files.iter().rev() {
        if let Some(mode) = f.chmod() {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::Permissions::from_mode(u32::from(mode));
            std::fs::set_permissions(dir.join(f.name), mode).unwrap();
        }
    }
````

Replace:

````rust
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    let _ = std::fs::remove_dir_all(&dir);
````

with:

````rust
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    // A directory without write or search bits could not be emptied.
    for f in files {
        if matches!(f.made, Made::Dir) {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::Permissions::from_mode(0o755);
            let _ = std::fs::set_permissions(dir.join(f.name), mode);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 5 tests fail, among them `commands::test::tests::devices_answer_as_gnu_s_do`, `commands::test::tests::nothing_on_bin_s_filesystem_is_writable`.

- [ ] **Step 4: Change `README.md`**

In `README.md`, replace:

````markdown
(`readelf` checks every user program) and `linux-libc-dev` (the tests
check the error numbers against Linux's headers). On Ubuntu or Linux Mint:

````

with:

````markdown
(`readelf` checks every user program) and `linux-libc-dev` (the tests
check the error numbers against Linux's headers). The tests of `test` run
GNU's as root through `unshare -r`, which needs unprivileged user
namespaces: Ubuntu 24.04 restricts them unless
`kernel.apparmor_restrict_unprivileged_userns` is 0 (Linux Mint allows
them). On Ubuntu or Linux Mint:

````

- [ ] **Step 5: Change `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! digit strings, so nothing overflows; `-l STRING` stands for STRING's
//! length. Files are answered from `stat`, as for root. The evaluator never
//! recurses: each open `(` waits on a stack of its own, so it nests as deep
````

with:

````rust
//! digit strings, so nothing overflows; `-l STRING` stands for STRING's
//! length. Files are answered from `stat`, as for root, with two decided
//! differences (spec §10): a symbolic link is never followed, so `-e`, `-f`
//! and `-d` look at the link itself, and `-w` is false only on `/bin`'s
//! filesystem, the read-only one a program can tell. The evaluator never
//! recurses: each open `(` waits on a stack of its own, so it nests as deep
````

Replace:

````rust
use core::cmp::Ordering;

````

with:

````rust
use core::cmp::Ordering;
use vfs::FileType;

````

Replace:

````rust
        let op = &self.args[self.pos];
        match op.as_bytes()[1] {
            b'n' => Ok(!self.unary_operand()?.is_empty()),
            b'z' => Ok(self.unary_operand()?.is_empty()),
            _ => Err(format!("{}: unary operator expected", quote(op))),
        }
    }
````

with:

````rust
        let op = &self.args[self.pos];
        let file: fn(&vfs::Stat) -> bool = match op.as_bytes()[1] {
            b'n' => return Ok(!self.unary_operand()?.is_empty()),
            b'z' => return Ok(self.unary_operand()?.is_empty()),
            b'w' => {
                let path = self.unary_operand()?;
                return Ok(self.stat(path).is_some_and(|(node, _)| !self.on_bin(node)));
            }
            // As for root, which reads every file.
            b'e' | b'r' => |_| true,
            // As for root: any execute bit, or a directory to search.
            b'x' => |s| s.perm & 0o111 != 0 || s.kind == FileType::Directory,
            b'f' => |s| s.kind == FileType::Regular,
            b'd' => |s| s.kind == FileType::Directory,
            b'h' | b'L' => |s| s.kind == FileType::Symlink,
            b'p' => |s| s.kind == FileType::Fifo,
            b'S' => |s| s.kind == FileType::Socket,
            b'b' => |s| s.kind == FileType::BlockDev,
            b'c' => |s| s.kind == FileType::CharDev,
            b's' => |s| s.size > 0,
            b'u' => |s| s.perm & 0o4000 != 0,
            b'g' => |s| s.perm & 0o2000 != 0,
            b'k' => |s| s.perm & 0o1000 != 0,
            // Every program runs as root, user and group 0.
            b'O' => |s| s.uid == 0,
            b'G' => |s| s.gid == 0,
            b'N' => |s| s.mtime > s.atime,
            _ => return Err(format!("{}: unary operator expected", quote(op))),
        };
        let path = self.unary_operand()?;
        Ok(self.stat(path).is_some_and(|(_, s)| file(&s)))
    }

    /// Whether `node` is on `/bin`'s filesystem, `system.img`, which is
    /// read-only: no call says which filesystems are, and `open` for writing
    /// succeeds on them, so a root the kernel mounted read-only is not told
    /// (spec §10, §15 item 3).
    fn on_bin(&mut self, node: vfs::Node) -> bool {
        self.ctx
            .vfs
            .lookup(b"/bin")
            .is_ok_and(|bin| bin.mount == node.mount)
    }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 374 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add README.md crates
git commit -F - <<'EOF'
feat(shell): answer test's file operators as GNU does for root

Every unary file operator but -t (spec §6.2, §15 item 3), answered from
stat as GNU answers for root, since Relay OS runs every program as root:
-r is true for a file that exists, -w too unless the file is on /bin's
filesystem (system.img, read-only, which no call tells), -x for any
execute bit or a directory; -O and -G for user and group 0. Symbolic
links are not followed, a decided difference (§10). The tests compare
each operator on files of every kind and mode with the host's GNU test
run through `unshare -r`, so that it answers as root, and devices with
the host's; the harness makes links, FIFOs, sockets and modes on both
sides. The README says these tests need unprivileged user namespaces.
EOF
````


### Task 5: The harness binds its sockets through a short path

The prototype's review (important I1): a Unix socket's address holds 108 bytes, and Task 4's sockets were bound at their own paths under the checkout's `target/`, so `cargo test -p shell` failed in a checkout 57 or more characters deep (`/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing` among them) with `path must be shorter than SUN_LEN`. `host_files` binds each socket through the `/proc/self/fd` entry of its open directory, as xtask's `qmp::reachable` does since #102. The red run is the shell's tests: a socket made in a directory whose name alone is 60 characters long fails whatever the checkout. The whole shell suite passed from a 78-character checkout after the change.

**Files:**
- Modify: `crates/shell/src/commands/test.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 4's `host_files`.
- Produces: nothing new outside the harness.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, replace:

````rust
    #[test]
    fn devices_answer_as_gnu_s_do() {
````

with:

````rust
    #[test]
    fn a_socket_is_made_however_deep_its_directory_is() {
        // A Unix socket's address holds 108 bytes: the review found the
        // harness's sockets failing in a checkout 57 characters deep.
        let deep = "d".repeat(60);
        let socket = alloc::format!("{deep}/s");
        let files = [TestFile::dir(&deep), TestFile::socket(&socket)];
        for line in [
            ["test", "-S", socket.as_str()],
            ["test", "-e", deep.as_str()],
        ] {
            assert_eq!(
                like_host_files(&line, &files),
                (0, String::new(), String::new())
            );
            assert_eq!(
                host_files(&line, &files, true),
                (0, String::new(), String::new())
            );
        }
    }

    #[test]
    fn devices_answer_as_gnu_s_do() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::test::tests::a_socket_is_made_however_deep_its_directory_is`.

- [ ] **Step 3: Change `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust
            }
            // The socket's file stays when the listener goes.
            Made::Socket => drop(std::os::unix::net::UnixListener::bind(&path).unwrap()),
        }
````

with:

````rust
            }
            // The socket's file stays when the listener goes. It is bound
            // through the `/proc/self/fd` entry of its open directory, so
            // that the address fits a Unix socket's 108 bytes however deep
            // the checkout is (as xtask's `qmp::reachable`).
            Made::Socket => {
                use std::os::fd::AsRawFd;
                let parent = std::fs::File::open(path.parent().unwrap()).unwrap();
                let name = path.file_name().unwrap().to_str().unwrap();
                let short = std::format!("/proc/self/fd/{}/{name}", parent.as_raw_fd());
                drop(std::os::unix::net::UnixListener::bind(short).unwrap());
            }
        }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 375 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): bind the harness's sockets through a short path

The prototype's review (important I1): a Unix socket's address holds
108 bytes, and the file operators' test made its sockets under the
checkout's target/, so `cargo test -p shell` failed in a checkout 57 or
more characters deep, a worktree as m4p3-pacing among them. The harness
binds each socket through the /proc/self/fd entry of its open
directory, as xtask's qmp::reachable does, and a test makes one in a
directory whose name alone is 60 characters long.
EOF
````


### Task 6: `test -t` asks the system whether an fd is the console

Spec §6.2 and decision 8: `test -t FD` reads FD as GNU reads an integer, so `-0`, `00` and `' 1 '` name fds 0 and 1, and a negative one, or one past `int`'s largest, is false (`probes/tty.txt`, GNU on a pty). The `System` trait gains `is_terminal(fd)`, false by default where there is no console (on the host: `host-shell` and the sysvfs tests keep it); the test system says which fds are terminals (none unless a test names them, as the host's tool has pipes). The red run is the shell's tests, which cannot find `is_terminal` on `System`. Mutation checks: a negative fd, fd 0 from `-0`, the bound at `int`'s largest (both ways), and the system's answer, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/commands/test.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 3's `Number`; `io::System`.
- Produces: `System::is_terminal(&self, fd: u32) -> bool` (default `false`); `TestSystem.terminals: Vec<u32>`; `Number::fd(&self) -> Option<u32>` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, replace:

````rust
    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
````

with:

````rust
    #[test]
    fn dash_t_reads_its_fd_as_gnu_reads_an_integer() {
        // The host's tool has pipes, and the harness no console: false,
        // or GNU's message.
        like_gnu(&[
            &["-t", "0"],
            &["-t", "1"],
            &["-t", "2"],
            &["-t", "-1"],
            &["-t", "x"],
            &["-t", ""],
            &["-t", "0x0"],
            &["-t", "99999999999999999999"],
            &["-t", "é"],
            &["-t"],
            &["-t", "0", "-a", "x"],
            &["!", "-t", "1"],
        ]);
        // On a console: probes/tty.txt (fds 0 and 1 on a pty).
        let mut h = Harness::new();
        h.system.terminals = alloc::vec![0, 1];
        for (fd, status) in [
            ("0", 0),
            ("1", 0),
            ("2", 1),
            ("3", 1),
            ("-0", 0),
            ("00", 0),
            ("+0", 0),
            ("' 1 '", 0),
            ("-1", 1),
            ("2147483647", 1),
            ("2147483648", 1),
            ("99999999999999999999", 1),
        ] {
            let line = alloc::format!("test -t {fd}");
            assert_eq!(h.run(&line), (status, String::new()), "{line}");
        }
        // GNU takes fds up to `int`'s largest, as an fd is.
        h.system.terminals = alloc::vec![2_147_483_647, 2_147_483_648];
        assert_eq!(h.run("[ -t 2147483647 ]"), (0, String::new()));
        assert_eq!(h.run("[ -t 2147483648 ]"), (1, String::new()));
    }

    #[test]
    fn the_program_is_bracket_when_started_as_bracket() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub processes: Option<Vec<relay_abi::ProcInfo>>,
}
````

with:

````rust
    pub processes: Option<Vec<relay_abi::ProcInfo>>,
    /// The fds open on the console (none, as on the host where a test's
    /// tool has pipes).
    pub terminals: Vec<u32>,
}
````

Replace:

````rust
            processes: None,
        }
````

with:

````rust
            processes: None,
            terminals: Vec::new(),
        }
````

Replace:

````rust
        self.processes.clone()
    }
````

with:

````rust
        self.processes.clone()
    }
    fn is_terminal(&self, fd: u32) -> bool {
        self.terminals.contains(&fd)
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `is_terminal` is not a member of trait `System` ``.

- [ ] **Step 4: Change `crates/shell/src/commands/test.rs`**

In `crates/shell/src/commands/test.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            b'z' => return Ok(self.unary_operand()?.is_empty()),
            b'w' => {
````

with:

````rust
            b'z' => return Ok(self.unary_operand()?.is_empty()),
            b't' => {
                let fd = Number::parse(self.unary_operand()?)?;
                return Ok(fd.fd().is_some_and(|fd| self.ctx.system.is_terminal(fd)));
            }
            b'w' => {
````

Replace:

````rust
            digits: String::from(digits),
        })
    }

````

with:

````rust
            digits: String::from(digits),
        })
    }

    /// The fd this names, if any can be: from 0 to `int`'s largest, as
    /// GNU's `-t` takes them.
    fn fd(&self) -> Option<u32> {
        match (self.negative, self.digits.as_str()) {
            (true, _) => None,
            (false, "") => Some(0),
            (false, digits) => digits.parse().ok().filter(|&fd| fd <= i32::MAX as u32),
        }
    }

````

- [ ] **Step 5: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>>;
    /// Waits `ms` milliseconds (`sleep`).
````

with:

````rust
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>>;
    /// Whether the command's fd `fd` is open on the console (`test -t`);
    /// never where there is no console (on the host).
    fn is_terminal(&self, _fd: u32) -> bool {
        false
    }
    /// Waits `ms` milliseconds (`sleep`).
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 376 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): answer test -t through the system's console

`test -t FD` is true when FD is open on the console (spec §6.2, §15
item 3). FD is read as GNU reads an integer, so `-0`, `00` and `' 1 '`
name fds 0 and 1, and a negative one or one past int's largest is
false. The System trait gains is_terminal, false where there is no
console (on the host); the test system says which fds are terminals,
and programs answer from fstat in the next change.
EOF
````


### Task 7: Programs answer `test -t` from `fstat`

Decision 8: a program's `System` says an fd is a terminal when `fstat` says it is the console, the character device on filesystem 0 (`sysio::is_console`), which the checks of fds 0 and 1 for the console use too. The kernel cannot be called on the host, so the rule is a function of a `Stat` tested alone, and the scenario `test_cmd` (Task 10) checks the call in QEMU. The red run is `relay-rt`'s tests, which cannot find `is_console` or `Stat`. Mutation checks: the kind and the filesystem, each left out, fail the test; in QEMU, `is_terminal` always false fails `test_cmd`.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`

**Interfaces:**
- Consumes: Task 6's `System::is_terminal`; `sys::fstat`.
- Produces: `relay_rt::sysio::is_console(&Stat) -> bool`; `SysSystem::is_terminal`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
    #[test]
    fn dash_f_is_power_force() {
````

with:

````rust
    #[test]
    fn the_console_is_the_character_device_of_filesystem_0() {
        let console = Stat {
            kind: u32::from(KIND_CHAR_DEVICE),
            dev: 0,
            ..Stat::default()
        };
        assert!(is_console(&console));
        // A device on a disk, and anything else on filesystem 0.
        assert!(!is_console(&Stat { dev: 1, ..console }));
        let fifo = u32::from(relay_abi::file::KIND_FIFO);
        assert!(!is_console(&Stat {
            kind: fifo,
            ..console
        }));
        assert!(!is_console(&Stat::default()));
    }

    #[test]
    fn dash_f_is_power_force() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-rt`

Expected: FAIL: compile errors such as `` cannot find struct, variant or union type `Stat` in this scope ``; `` cannot find type `Stat` in this scope ``.

- [ ] **Step 3: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, Group, MemInfo, Programs, Stdin, Stdout, System};
````

with:

````rust
use relay_abi::spawn::{FOREGROUND, NEW_GROUP};
use relay_abi::{FdMap, Stat, WaitStatus};
use shell::{Console, Group, MemInfo, Programs, Stdin, Stdout, System};
````

Replace:

````rust

    fn kernel_log(&self) -> Vec<u8> {
````

with:

````rust

    fn is_terminal(&self, fd: u32) -> bool {
        sys::fstat(fd).is_ok_and(|st| is_console(&st))
    }

    fn kernel_log(&self) -> Vec<u8> {
````

Replace:

````rust

/// Standard input: fd 0, the console (in line mode, as the shell gives it
````

with:

````rust

/// Whether `fstat` says this is the console: the character device on
/// filesystem 0, the only one there is.
pub fn is_console(st: &Stat) -> bool {
    st.kind == u32::from(KIND_CHAR_DEVICE) && st.dev == 0
}

/// Standard input: fd 0, the console (in line mode, as the shell gives it
````

Replace:

````rust
    pub fn is_console() -> bool {
        sys::fstat(0).is_ok_and(|st| st.kind == u32::from(KIND_CHAR_DEVICE))
    }
````

with:

````rust
    pub fn is_console() -> bool {
        sys::fstat(0).is_ok_and(|st| is_console(&st))
    }
````

Replace:

````rust
    pub fn new() -> SysStdout {
        let tty = sys::fstat(1).is_ok_and(|st| st.kind == u32::from(KIND_CHAR_DEVICE));
        SysStdout { tty }
````

with:

````rust
    pub fn new() -> SysStdout {
        let tty = sys::fstat(1).is_ok_and(|st| is_console(&st));
        SysStdout { tty }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(relay-rt): answer test -t from fstat in programs

A program's System says an fd is a terminal when fstat says it is the
console: the character device on filesystem 0 (spec §15 item 3). The
rule is one function, which the checks of fds 0 and 1 for the console
use too.
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 48 scenario(s) passed`.

````bash
git push -u origin m4p3/test
gh pr create --base main --head m4p3/test --title "feat(shell): add test and [ with GNU's grammar" --body-file - <<'EOF'
## What

Milestone 4, plan 3, Tasks 1–7: `test` and `[`, one evaluator under two names, GNU coreutils 9.4's grammar rule for rule and its messages word for word (no `<` or `>`; `-l`; `[ --help` refused), integers of any length, every file operator answered as GNU answers for root (symbolic links not followed, `-w` false on `/bin`'s filesystem), `-t` from the console; compared with the host's `/usr/bin/test` and `/usr/bin/[`, the file operators through `unshare -r`, which CI's `unit` job now allows. `/bin` gets the two programs in the next pull request; until then `help` names them.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's NUC check 6 runs `test` and `[` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p3/test --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-test
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: `/bin/test` and `/bin/[`, and what lists `/bin` (Tasks 8–11)

xtask packs the `test` program under both names (44 programs); the `system` scenario, check 4 and its transcripts (the NUC's by hand), and the documents' counts follow; the scenario `test_cmd` runs both on the real disk; the corpus uses `test` and `[`.

Branch `m4p3/bin`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-bin`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p3/bin /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-bin origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-bin
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p3/test` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p3/bin /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-bin m4p3/test`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p3/test>` and re-run `cargo xtask ci` before pushing.

### Task 8: `/bin/test` and `/bin/[`

Spec §6.1, §11.5 and decisions 1 and 10: `userland/utils/src/bin/test.rs` runs the command named by its argument 0 (Task 3's `named`), and xtask packs the `test` program under both names, since Cargo refuses `[` as a binary's name (`invalid character '[' in crate name`): 44 programs, `system.img` holding the bytes twice. What lists `/bin` changes with it: the `system` scenario's two `ls /bin`, and check 4's, where `[` sorts first in the C locale (`check4.sh` gains `#> \[`). Check 4's QEMU transcript is recorded again from the `checks` scenario (the old one lacked programs added since, which the script's `...` let pass); its NUC transcript gets `[` and `test` by hand until plan 4's NUC run replaces it, as milestone 3's plans edited NUC transcripts, and `checks.rs`'s test says so. The red run is xtask's image test, which finds no `test` entry. Mutation checks: packing `[` from no program, and the order the archive wants, each broken, fail the image test.

**Files:**
- Modify: `rootfs/root/checks/check4.sh`
- Modify: `tests/e2e/system.txt`
- Create: `userland/utils/src/bin/test.rs`
- Modify: `xtask/fixtures/checks/check4.nuc.log`
- Modify: `xtask/fixtures/checks/check4.qemu.log`
- Modify: `xtask/src/checks.rs`
- Modify: `xtask/src/userland.rs`

**Interfaces:**
- Consumes: `shell::commands::named`; `xtask::userland::build`, `Program`.
- Produces: the program `test`, packed as `test` and `[`.

- [ ] **Step 1: Expect the new lines in `tests/e2e/system.txt`**

In `tests/e2e/system.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     ps      rm     sh     sync    t-fault  t-pipe  t-spawn  t-tee  true\nclear  df     false  head  mv        pwd     rmdir  sleep  t-abi   t-files  t-proc  t-spin   tail   uname\ncp     dmesg  free   ls    poweroff  reboot  seq    stat   t-args  t-mem    t-read  t-sys    touch  wc\n
send ls -l /bin/t-args
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
send ls -l /bin/t-args
````

Replace:

````text
send ls /bin
expect \ncat    date   echo   grep  mkdir     ps      rm     sh     sync    t-fault  t-pipe  t-spawn  t-tee  true\nclear  df     false  head  mv        pwd     rmdir  sleep  t-abi   t-files  t-proc  t-spin   tail   uname\ncp     dmesg  free   ls    poweroff  reboot  seq    stat   t-args  t-mem    t-read  t-sys    touch  wc\n
````

with:

````text
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
````

- [ ] **Step 2: Add the failing tests to `xtask/src/checks.rs`**

In `xtask/src/checks.rs`, replace:

````rust
    /// last NUC run of checks 3, 4 and 5 (`docs/hardware-test.md`'s results
    /// log), copied off the stick unchanged. A `#nuc>` line must
    /// not need a line of its own next to the `#>` line for the same
````

with:

````rust
    /// last NUC run of checks 3, 4 and 5 (`docs/hardware-test.md`'s results
    /// log), copied off the stick unchanged but for check 4's `ls /bin`,
    /// which has `[` and `test` by hand until the next NUC run (spec §15
    /// item 3). A `#nuc>` line must
    /// not need a line of its own next to the `#>` line for the same
````

- [ ] **Step 3: Add the failing tests to `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
        assert_eq!(t.data, fs::read(t_args().path).unwrap());
        let now = SystemTime::now()
````

with:

````rust
        assert_eq!(t.data, fs::read(t_args().path).unwrap());
        // One program under two names (spec §15 item 3): Cargo refuses `[`
        // as a binary's name.
        let test = archive.entry(archive.find(b"test").unwrap()).unwrap();
        let bracket = archive.entry(archive.find(b"[").unwrap()).unwrap();
        assert_eq!((bracket.mode, bracket.data), (0o755, test.data));
        assert_eq!(programs[0].name, "[", "first, as the archive sorts");
        let now = SystemTime::now()
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `userland::tests::the_system_image_holds_every_program`.

- [ ] **Step 5: Change `rootfs/root/checks/check4.sh`**

In `rootfs/root/checks/check4.sh`, replace:

````bash
cat /root/check4.list
#> cat
````

with:

````bash
cat /root/check4.list
#> \[
#> cat
````

- [ ] **Step 6: Create `userland/utils/src/bin/test.rs`**

Create `userland/utils/src/bin/test.rs`:

````rust
//! `/bin/test` and `/bin/[` (spec §6.1, §15 item 3): the shell's command
//! function run as a program. xtask packs it under both names, since Cargo
//! refuses `[` as a binary's, and it is `[` when started by that name.
#![no_std]
#![no_main]

relay_rt::main!(main);

fn main(args: relay_rt::Args) -> u8 {
    let (name, run) = shell::commands::named(args.get(0).unwrap_or(b""));
    relay_rt::sysio::run_command(name, run, &args)
}
````

- [ ] **Step 7: Change `xtask/fixtures/checks/check4.nuc.log`**

In `xtask/fixtures/checks/check4.nuc.log`, make these 2 replacements, top to bottom:

Replace:

````text
+ cat /root/check4.list
cat
````

with:

````text
+ cat /root/check4.list
[
cat
````

Replace:

````text
tail
touch
````

with:

````text
tail
test
touch
````

- [ ] **Step 8: Change `xtask/fixtures/checks/check4.qemu.log`**

In `xtask/fixtures/checks/check4.qemu.log`, make these 8 replacements, top to bottom:

Replace:

````text
+ t-spin 1
4493148160 iterations
+ t-spawn 1000
free frames before: 247463
free frames after: 247463
frames lost: 0
+ free
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
````

with:

````text
+ t-spin 1
4418699264 iterations
+ t-spawn 1000
free frames before: 246961
free frames after: 246961
frames lost: 0
+ free
              total        used        free
Mem:        1029016       41200      987816
Heap:         32768        8524       24244
````

Replace:

````text
+ t-spin 1
2300575744 iterations
2312110080 iterations
+ t-args a 'b c' ''
````

with:

````text
+ t-spin 1
2319450112 iterations
2321547264 iterations
+ t-args a 'b c' ''
````

Replace:

````text
+ cat /root/check4.list
cat
````

with:

````text
+ cat /root/check4.list
[
cat
````

Replace:

````text
echo
free
head
````

with:

````text
echo
false
free
grep
head
````

Replace:

````text
poweroff
pwd
reboot
rm
rmdir
sh
stat
````

with:

````text
poweroff
ps
pwd
reboot
rm
rmdir
seq
sh
sleep
stat
````

Replace:

````text
t-mem
t-read
````

with:

````text
t-mem
t-pipe
t-proc
t-read
````

Replace:

````text
tail
touch
uname
````

with:

````text
tail
test
touch
true
uname
````

Replace:

````text
              total        used        free
Mem:        1030532       40720      989812
Heap:         32768        8524       24244
````

with:

````text
              total        used        free
Mem:        1029016       41200      987816
Heap:         32768        8524       24244
````

- [ ] **Step 9: Change `xtask/src/userland.rs`**

In `xtask/src/userland.rs`, replace:

````rust
            }
        }
    }
    programs.sort_by(|a, b| a.name.cmp(&b.name));
````

with:

````rust
            }
        }
    }
    // Cargo refuses `[` as a binary's name, so the `test` program is packed
    // under both names; it takes its name from its argument 0 (spec §15
    // item 3).
    if let Some(test) = programs.iter().find(|p| p.name == "test") {
        let path = test.path.clone();
        programs.push(Program {
            name: "[".into(),
            path,
        });
    }
    programs.sort_by(|a, b| a.name.cmp(&b.name));
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 90 tests.

- [ ] **Step 11: Run the `system`, `checks` scenarios**

Run: `cargo xtask test --e2e-only --scenario system`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario checks`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 12: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 13: Commit**

````bash
git add rootfs tests userland xtask
git commit -F - <<'EOF'
feat(utils,xtask): put test and [ in /bin

/bin/test is the shell's test and [ run as a program (spec §6.1, §15
item 3). Cargo refuses `[` as a binary's name, so xtask packs the test
program under both names, and the program is `[` when started by that
name: 44 programs.

What lists /bin changes with it: the system scenario's `ls /bin`, and
check 4's, where `[` sorts first in the C locale. Check 4's QEMU
transcript is recorded again from the checks scenario; its NUC
transcript gets `[` and `test` by hand until plan 4's NUC run replaces
it, as milestone 3's plans edited NUC transcripts.
EOF
````


### Task 9: The documents count 44 programs

Spec §11.5 and the review's M8 (a commit names at most two modules): the startup line's count in `kernel/src/system.rs`'s comment and in `docs/hardware-test.md`'s checklist says 44 programs. The results log is history and keeps its 42. Nothing checks either text.

**Files:**
- Modify: `docs/hardware-test.md`
- Modify: `kernel/src/system.rs`

**Interfaces:**
- Consumes: Task 8's 44 programs.
- Produces: nothing new.

- [ ] **Step 1: Change `docs/hardware-test.md`**

In `docs/hardware-test.md`, make these 2 replacements, top to bottom:

Replace:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 42 programs, ABI 3`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

with:

````markdown
   - the lines of checks 1b, 2 and 3, ending with
     `[ ok ] system: 44 programs, ABI 3`, then the motd
     (`Welcome to Relay OS.`) and the prompt `root@relay:~# ` with a solid
````

Replace:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 42 programs, ABI 3` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

with:

````markdown
     scripts refuse: run `flash --full` again)
   - `[ ok ] system: 44 programs, ABI 3` (the programs of `/bin`, read
     from `\EFI\RELAY\system.img`)
````

- [ ] **Step 2: Change `kernel/src/system.rs`**

In `kernel/src/system.rs`, replace:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 42 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

with:

````rust
//! at `/bin`. Startup step 10 reports how that went, as `[ ok ] system:
//! 44 programs, ABI 3` or `[FAIL] system: <reason>`; without an archive
//! there is no shell to run, and init shows the error screen (§11.2).
````

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add docs kernel
git commit -F - <<'EOF'
docs(kernel,checks): count 44 programs in /bin

/bin holds test and [ now (spec §11.5): the startup line's count in
kernel/src/system.rs's comment and in docs/hardware-test.md's checklist
follows. The results log is history and keeps its 42.
EOF
````


### Task 10: The scenario `test_cmd`

Spec §11.3: `/bin/test` and `/bin/[` on the real disk, every line waiting for the prompt: `-w` false on the read-only `/bin` and true on the root, `-x`, `-f`, `-d`; `-t` true for the console as fd 0 and 1 and false for a pipe or a file; `-nt` and `-ot` of two files made two seconds apart by the RTC's clock; `-ef` across filesystems and between `/bin`'s two names (two entries); GNU's messages under each name, `[ --help` refused; 20-digit integers; and a script whose `if` and `while` take `[` as their condition, with its trace. 49 scenarios. It passes as written: the mutation checks ran in QEMU, and the `/bin` rule removed, `is_terminal` always false, or the program never `[`, each fail it.

**Files:**
- Create: `tests/e2e/test_cmd.txt`

**Interfaces:**
- Consumes: Tasks 8's programs, 7's `is_terminal`.
- Produces: `tests/e2e/test_cmd.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/test_cmd.txt`**

Create `tests/e2e/test_cmd.txt`:

````text
# test and [ on the real disk (programmable shell gate §6, §11.3, §15
# item 3; milestone 4, plan 3): one program under two names in the
# read-only /bin, files on the ext2 root, the console as fd 0 and 1.
timeout 30
expect root@relay:~# $
# /bin is read-only: nothing on its filesystem is writable.
send [ -w /bin/ls ]; echo "w $?"
expect \nw 1\nroot@relay:~# $
send test -w /bin; echo "w $?"
expect \nw 1\nroot@relay:~# $
send test -w /root; echo "w $?"
expect \nw 0\nroot@relay:~# $
send [ -x /bin/[ -a -f /bin/test -a -d /bin ]; echo "x $?"
expect \nx 0\nroot@relay:~# $
# The console is fd 0 and fd 1, a pipe or a file is not.
send [ -t 0 ]; echo "t $?"
expect \nt 0\nroot@relay:~# $
send test -t 1; echo "t $?"
expect \nt 0\nroot@relay:~# $
send echo | test -t 0; echo "t $?"
expect \nt 1\nroot@relay:~# $
send test -t 1 > /root/t-out; echo "t $?"
expect \nt 1\nroot@relay:~# $
# Two files made seconds apart, by the RTC's clock.
send touch /root/older
expect \nroot@relay:~# $
send sleep 2
expect \nroot@relay:~# $
send touch /root/newer
expect \nroot@relay:~# $
send [ /root/newer -nt /root/older ]; echo "nt $?"
expect \nnt 0\nroot@relay:~# $
send [ /root/older -nt /root/newer ]; echo "nt $?"
expect \nnt 1\nroot@relay:~# $
send test /root/older -ot /root/newer; echo "ot $?"
expect \not 0\nroot@relay:~# $
# Two filesystems, and two entries of /bin with one program's bytes.
send [ /bin/ls -ef /root ]; echo "ef $?"
expect \nef 1\nroot@relay:~# $
send [ /bin/[ -ef /bin/test ]; echo "ef $?"
expect \nef 1\nroot@relay:~# $
send [ /bin/ls -ef /bin/./ls ]; echo "ef $?"
expect \nef 0\nroot@relay:~# $
# GNU's messages, by the name each program was started by.
send [ 1
expect \n\[: missing '\]'\nroot@relay:~# $
send echo $?
expect \n2\nroot@relay:~# $
send /bin/[ 12 -eq x ]
expect \n\[: invalid integer 'x'\nroot@relay:~# $
send test 1 -eq 1 -a
expect \ntest: missing argument after '-a'\nroot@relay:~# $
send [ --help
expect \n\[: unrecognized option '--help'\nroot@relay:~# $
send test --help; echo "help $?"
expect \nhelp 0\nroot@relay:~# $
send [ 99999999999999999999 -gt 99999999999999999998 ] && echo bigger
expect \nbigger\nroot@relay:~# $
# A script's conditions.
send echo 'if [ "$1" = go ]; then echo going; fi' > /root/t.sh
expect \nroot@relay:~# $
send echo 'while [ ! -f /root/stop ]; do touch /root/stop; echo once; done' >> /root/t.sh
expect \nroot@relay:~# $
send sh /root/t.sh go
expect \n\+ if \[ "\$1" = go \]; then echo going; fi\ngoing\n\+ while \[ ! -f /root/stop \]; do touch /root/stop; echo once; done\nonce\nroot@relay:~# $
poweroff
````

- [ ] **Step 2: Run the `test_cmd` scenario**

Run: `cargo xtask test --e2e-only --scenario test_cmd`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): add the scenario test_cmd

/bin/test and /bin/[ on the real disk (spec §11.3, §15 item 3): -w false
on the read-only /bin and true on the root, -t true for the console as
fd 0 and 1 and false for a pipe or a file, -nt and -ot of two files made
seconds apart, -ef across filesystems and between /bin's two names,
GNU's messages under each name, and a script whose if and while take [
as their condition. Every line waits for the prompt before the next.
EOF
````


### Task 11: The corpus runs `test` and `[`

Spec §11.2: `crates/shell/tests/corpus/test.sh` uses `test` and `[` in `if`, `while`, `until` and `for`, with strings, integers, `-a`, `-o`, `!` and parentheses, and the directories both sides have (`/`, `/etc`). bash runs its built-in `test` there, whose messages differ from GNU's, so the script prints none; bash's cannot read integers past 64 bits (`integer expression expected`), so those stay in the unit tests and `test_cmd`. It passes as written (it tests what Task 3 made).

**Files:**
- Create: `crates/shell/tests/corpus/test.sh`

**Interfaces:**
- Consumes: Task 3's `test` and `[`; the corpus harness (`crates/shell/src/corpus.rs`).
- Produces: `crates/shell/tests/corpus/test.sh`.

- [ ] **Step 1: Create `crates/shell/tests/corpus/test.sh`**

Create `crates/shell/tests/corpus/test.sh`:

````bash
# test and [ as conditions (plan 3): bash runs its built-in test, which
# answers these as GNU's program does, and neither prints a message
# (bash's cannot read integers past 64 bits, which GNU's can).
X=abc
if [ "$X" = abc ]; then echo equal; fi
if test -n "$X" && [ -z "" ]; then echo both; fi
[ "$X" != abd ] && echo differ
[ 1 -lt 2 ] && echo less
[ 10 -gt 9 ] || echo not-run
[ -0 -eq 0 ] && [ +7 -eq 007 ] && echo signs
[ ! -e /nonexistent ] && echo absent
[ -d / ] && test -d /etc && echo dirs
[ / -ef / ] && echo same
[ a = b -o c = c ] && echo or
[ a = a -a ! b = c ] && echo and
[ \( a = a \) -a \( '' -o x \) ] && echo parens
test && echo no-arguments || echo false
test '' || echo empty
test x && echo string
[ ! ] && echo bang
[ -n ] && echo dash-n
[ é = é ] && echo accent
# In loops: a counter kept as a string of x's, ended by test.
N=
while [ "$N" != xxx ]; do N=${N}x; echo "pass $N"; done
until test "$N" = ""; do N=; echo cleared; done
for n in 1 2 3; do
  if [ "$n" -eq 2 ]; then
    echo two
  elif [ "$n" -lt 2 ]; then
    echo small
  else
    echo big
  fi
done
# The statuses: 0 for true, 1 for false.
[ 1 -eq 2 ]; echo $?
[ x ]; echo $?
! [ x ]; echo $?
````

- [ ] **Step 2: Run the corpus**

Run: `cargo test -p shell corpus`

Expected: PASS: 2 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): run test and [ in the bash corpus

The corpus may use test and [ now (spec §11.2): a script of conditions
in if, while, until and for, with strings, integers, -a, -o, ! and
parentheses, and the directories both sides have. bash runs its
built-in test, whose messages differ from GNU's, so the script prints
none; bash's cannot read integers past 64 bits, so those stay in the
unit tests and test_cmd.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p3/bin
gh pr create --base main --head m4p3/bin --title "feat(utils,xtask): put test and [ in /bin" --body-file - <<'EOF'
## What

Milestone 4, plan 3, Tasks 8–11: the `test` program packed as `/bin/test` and `/bin/[` (Cargo refuses `[` as a binary's name), picking its mode from its argument 0: 44 programs; the `system` scenario's `ls /bin` and check 4's (`[` sorts first), check 4's QEMU transcript recorded again and its NUC transcript edited by hand until plan 4's NUC run; the counts in `kernel/src/system.rs`'s comment and `docs/hardware-test.md`'s checklist; the scenario `test_cmd`; the corpus's `test.sh`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `test_cmd`

## Hardware

- [x] Not needed now: the stick keeps 0.4.0; `check4.nuc.log` has the `[` and `test` lines of `ls /bin` by hand until plan 4's NUC run records it again
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p3/bin --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-bin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `grep -q` and two of plan 2's minors (Tasks 12–14)

GNU grep's quiet mode; the comments of `RESERVED` and the scan's `depth`; `control`'s `kill` and `wait` on one line.

Branch `m4p3/grep-q`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-grep-q`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p3/grep-q /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-grep-q origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-grep-q
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p3/bin` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p3/grep-q /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-grep-q m4p3/bin`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p3/bin>` and re-run `cargo xtask ci` before pushing.

### Task 12: `grep -q`

Decision 11 (the roadmap's note): GNU grep 3.11's quiet mode (`tmp/m4p3/probes/grepq.txt`). `grep -q` prints nothing and ends at the first selected line, reading no further and opening no later file; its status is 0 then, even after an error, and 2 only for an error with no line selected. It outweighs `-c` and `-n`, says nothing of a binary file, and skips the check of an input that is the output file, since it writes nothing. `help`'s line for `grep` names it, within 72 columns. The cases are compared with the host's GNU grep (run directly, never through a shell function), and an endless standard input that counts its reads is read once. The red run is the shell's tests: `grep: invalid option -- 'q'`. Mutation checks: the stop after a match among files, the status with an earlier error, the output check, `-c`'s count, and the stop at the first line (both ways), each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/commands/mod.rs`

**Interfaces:**
- Consumes: `commands::grep`'s `Options`, `Lines::take`, `search`.
- Produces: `Options.quiet`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
    #[test]
    fn grep_never_reads_its_own_output() {
````

with:

````rust
    #[test]
    fn grep_q_says_only_whether_a_line_is_selected() {
        // probes/grepq.txt q1–q15 (GNU grep 3.11).
        let files: &[(&str, &[u8])] = &[
            ("f", b"a\nb\n"),
            ("g", b"zz\n"),
            ("bin", b"a\0b\n"),
            ("d/", b""),
        ];
        let cases: &[(&[&str], &[u8])] = &[
            (&["grep", "-q", "a", "f"], b""),
            (&["grep", "-q", "x", "f"], b""),
            // It stops at the first selected line: `nope` is never opened.
            (&["grep", "-q", "a", "f", "nope"], b""),
            (&["grep", "-q", "a", "f", "d"], b""),
            // An error before it is told; the status is still 0.
            (&["grep", "-q", "a", "nope", "f"], b""),
            (&["grep", "-q", "x", "nope", "f"], b""),
            (&["grep", "-q", "a", "d", "g", "f"], b""),
            (&["grep", "-qc", "a", "f"], b""),
            (&["grep", "-qc", "x", "f"], b""),
            (&["grep", "-qn", "a", "f"], b""),
            (&["grep", "-qv", "a", "f"], b""),
            (&["grep", "-qv", ".", "f"], b""),
            (&["grep", "-qi", "A", "f"], b""),
            (&["grep", "-q", "", "g"], b""),
            (&["grep", "-q", "a", "bin"], b""),
            (&["grep", "-q", "a", "f", "f"], b""),
            (&["grep", "-q", "a", "-", "f"], b"z\n"),
            (&["grep", "-q", "a"], b"x\na\n"),
            (&["grep", "-q", "a"], b""),
            (&["grep", "-q", "é"], "été\n".as_bytes()),
            (&["grep", "-q"], b""),
            (&["grep", "-q", "[a", "f"], b""),
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

    /// Standard input that never ends by itself: `yes` lines, counted.
    struct Endless {
        reads: usize,
    }

    impl crate::Stdin for Endless {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
            self.reads += 1;
            if self.reads > 100 {
                return Ok(0);
            }
            let n = buf.len().min(4096) / 4 * 4;
            for line in buf[..n].chunks_mut(4) {
                line.copy_from_slice(b"yes\n");
            }
            Ok(n)
        }
    }

    #[test]
    fn grep_q_stops_reading_at_its_first_selected_line() {
        // As GNU's: `yes | grep -q y` ends at once (probe q17).
        for (args, status, reads) in [(&["-q", "y"][..], 0, 1), (&["-c", "y"], 0, 101)] {
            let mut h = Harness::new();
            let mut input = Endless { reads: 0 };
            let mut out = crate::testing::FakeStdout::file(None);
            let args: alloc::vec::Vec<String> = args.iter().map(|a| String::from(*a)).collect();
            let got = crate::run_command(
                "grep",
                super::grep,
                &args,
                crate::CommandIo {
                    vfs: &mut h.vfs,
                    console: &mut h.console,
                    system: &mut h.system,
                    stdin: &mut input,
                    stdout: &mut out,
                },
            );
            assert_eq!((got, input.reads), (status, reads), "{args:?}");
        }
    }

    #[test]
    fn grep_q_writes_nothing_so_its_output_may_be_an_input() {
        // GNU skips the check of an input that is the output file under -q,
        // which writes nothing (probes q16, q16c).
        let mut h = Harness::new();
        h.put("/tmp/f", b"a\nb\n");
        assert_eq!(h.run("grep -q a /tmp/f >> /tmp/f"), (0, "".into()));
        assert_eq!(h.run("grep -q x /tmp/f >> /tmp/f"), (1, "".into()));
        assert_eq!(h.get("/tmp/f"), b"a\nb\n");
    }

    #[test]
    fn grep_never_reads_its_own_output() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `commands::grep::tests::grep_q_writes_nothing_so_its_output_may_be_an_input`, `commands::grep::tests::grep_q_stops_reading_at_its_first_selected_line`.

- [ ] **Step 3: Change `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
//! `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` (user-space gate §9.1):
//! the lines of each file, or of standard input, that hold a match of
//! `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`
//! (a file `-` is standard input too).
````

with:

````rust
//! `grep [-i] [-v] [-n] [-c] [-q] PATTERN [FILE...]` (user-space gate
//! §9.1): the lines of each file, or of standard input, that hold a match
//! of `PATTERN` (`crate::pattern`), as GNU grep prints them with `LC_ALL=C`
//! (a file `-` is standard input too).
````

Replace:

````rust
//! error, a write error included.

````

with:

````rust
//! error, a write error included.
//!
//! `-q` (spec §15 item 3), GNU's quiet mode, prints nothing and ends at the
//! first selected line, reading no further and opening no later file, with
//! status 0 even after an error; it writes nothing, so its output may be an
//! input.

````

Replace:

````rust

/// What `-n`, `-v`, `-c` and the number of inputs ask for.
struct Options {
    invert: bool,
    number: bool,
    count: bool,
    /// Each line starts with its input's name.
````

with:

````rust

/// What `-n`, `-v`, `-c`, `-q` and the number of inputs ask for.
struct Options {
    invert: bool,
    number: bool,
    count: bool,
    quiet: bool,
    /// Each line starts with its input's name.
````

Replace:

````rust
    ctx.set_write_error_status(2);
    let opts = match getopt(args, "cinv", "") {
        Ok(o) => o,
````

with:

````rust
    ctx.set_write_error_status(2);
    let opts = match getopt(args, "cinqv", "") {
        Ok(o) => o,
````

Replace:

````rust
        count: opts.has('c'),
        names: files.len() > 1,
````

with:

````rust
        count: opts.has('c'),
        quiet: opts.has('q'),
        names: files.len() > 1,
````

Replace:

````rust
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
````

with:

````rust
    for file in files {
        let found = || outcomes.iter().any(|o| matches!(o, Outcome::Read(true)));
        if ctx.out_failed() || ctx.interrupted() || (options.quiet && found()) {
            break;
        }
        outcomes.push(search(ctx, &pattern, &options, Some(file)));
    }
    let found = outcomes.iter().any(|o| matches!(o, Outcome::Read(true)));
    if options.quiet && found {
        0
    } else if outcomes.iter().any(|o| matches!(o, Outcome::Failed)) {
        2
    } else if found {
        0
````

Replace:

````rust
        Some(path) => match open(ctx, path) {
            // It would read what it wrote, for ever (`grep x f >> f`).
            Ok(node) if ctx.output_node() == Some(node) => {
                ctx.fail(
````

with:

````rust
        Some(path) => match open(ctx, path) {
            // It would read what it wrote, for ever (`grep x f >> f`);
            // `-q` writes nothing.
            Ok(node) if ctx.output_node() == Some(node) && !options.quiet => {
                ctx.fail(
````

Replace:

````rust
    }
    if options.count {
        let prefix = if options.names {
````

with:

````rust
    }
    if options.count && !options.quiet {
        let prefix = if options.names {
````

Replace:

````rust
    /// False once a binary input has had a line selected, which is said
    /// instead, and the rest of the input is not read.
    fn take(&mut self, ctx: &mut Ctx<'_>, line: &[u8]) -> bool {
````

with:

````rust
    /// False once a binary input has had a line selected, which is said
    /// instead, and the rest of the input is not read; and under `-q` once
    /// a line is selected at all.
    fn take(&mut self, ctx: &mut Ctx<'_>, line: &[u8]) -> bool {
````

Replace:

````rust
        self.selected += 1;
        if self.options.count {
````

with:

````rust
        self.selected += 1;
        if self.options.quiet {
            return false;
        }
        if self.options.count {
````

- [ ] **Step 4: Change `crates/shell/src/commands/mod.rs`**

In `crates/shell/src/commands/mod.rs`, replace:

````rust
        name: "grep",
        help: "print the lines that match a pattern",
        run: grep::grep,
````

with:

````rust
        name: "grep",
        help: "print lines that match a pattern, or with -q say if one does",
        run: grep::grep,
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 379 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): add grep -q

GNU grep's quiet mode, a condition's usual command (spec §15 item 3):
grep -q prints nothing and ends at the first selected line, reading no
further and opening no later file; its status is 0 then, even after an
error, and 2 only for an error with no line selected. It outweighs -c
and -n, says nothing of a binary file, and skips the check of an input
that is the output file, as it writes nothing. The tests compare it with
the host's GNU grep and show that an endless input is read once.
EOF
````


### Task 13: The comments of `RESERVED` and the scan's `depth`

Decision 13 (plan 2's deferred minor): `RESERVED`'s comment in `crates/shell/src/parser.rs` said the loops' words were refused until the loops came, which they have; the scan's `depth` in `crates/shell/src/scan.rs` counts `select`, `case` and `esac` too, which its comment left out. Comments only.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/scan.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing new.

- [ ] **Step 1: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust

/// bash's other reserved words, and its loop built-ins, refused where a
/// command name could stand (programmable shell gate §4.2); the loops'
/// until they are implemented.
const RESERVED: &[&str] = &[
````

with:

````rust

/// bash's other reserved words, and its loop built-ins `break` and
/// `continue`, refused where a command name could stand (programmable
/// shell gate §4.2, §14).
const RESERVED: &[&str] = &[
````

- [ ] **Step 2: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until` and `for` where a
    /// command name would stand counts one, and each `fi` and `done` there
    /// one less.
    depth: usize,
````

with:

````rust
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until`, `for`, `select`
    /// and `case` where a command name would stand counts one, and each
    /// `fi`, `done` and `esac` there one less.
    depth: usize,
````

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
docs(shell): correct the comments of RESERVED and the scan's depth

Plan 2's deferred minor: RESERVED's comment said the loops' words were
refused until the loops came, which they have; the scan's depth counts
select, case and esac too, which its comment left out.
EOF
````


### Task 14: `control`'s `kill` and `wait` on one line

Decision 13 (plan 2's deferred minor): `send kill %1` and `send wait %1` came with no `expect` between them, so the kernel's line for the killed `sleep` could land inside the echo of the second. On one line, `wait` returns only after the `sleep` has gone, so the kernel's line comes first, then `wait`'s, and the scenario expects both. It passed three runs in a row.

**Files:**
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: `tests/e2e/control.txt`.
- Produces: nothing new.

- [ ] **Step 1: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, replace:

````text
expect \^C\nroot@relay:~# $
send kill %1
send wait %1
expect \n\[1\]\+  Killed                  sleep 30\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
````

with:

````text
expect \^C\nroot@relay:~# $
# On one line, so that the kernel's line for the killed sleep comes
# before wait's and never inside a command's echo.
send kill %1; wait %1
expect \npid \d+ \(/bin/sleep\): killed: kill\n\[1\]\+  Killed                  sleep 30\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
````

- [ ] **Step 2: Run the `control` scenario**

Run: `cargo xtask test --e2e-only --scenario control`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): pace control's kill and wait

Plan 2's deferred minor: `send kill %1` and `send wait %1` came with no
expect between them, so the kernel's line for the killed sleep could
land inside the echo of the second. On one line, wait returns only
after the sleep has gone, so the kernel's line comes first, then
wait's, and the scenario expects both.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p3/grep-q
gh pr create --base main --head m4p3/grep-q --title "feat(shell): add grep -q" --body-file - <<'EOF'
## What

Milestone 4, plan 3, Tasks 12–14: `grep -q`, GNU grep 3.11's quiet mode (nothing printed, the end at the first selected line, 0 even after an error, no binary message, no check of its output file); two of plan 2's deferred minors: the stale comments of `RESERVED` and the scan's `depth`, and `control`'s `kill %1` / `wait %1`, now one line.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's NUC check 6 runs `test` and `[` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p3/grep-q --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-grep-q
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 5: Every scenario's input waits for its prompt (Tasks 15–18)

The `-ahead` steps; every scenario paced; the scenario parser refusing input that no prompt paces; `AGENTS.md`'s e2e paragraph.

Branch `m4p3/pacing`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p3/pacing /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p3/grep-q` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p3/pacing /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing m4p3/grep-q`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p3/grep-q>` and re-run `cargo xtask ci` before pushing.

### Task 15: Scenario steps for input sent while something runs

Decision 12: `send-ahead`, `send-crlf-ahead`, `key-ahead` and `type-ahead` send as `send`, `send-crlf`, `key` and `type` do (a key name is checked the same), and mark input a scenario sends while something runs on purpose, so that Task 17 can hold every other input to wait for the prompt. The module comment of `xtask/src/e2e.rs` lists them. The red run is the scenario parser's tests: `unknown step 'send-ahead'`.

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: `e2e::parse_scenario`, `Step::{Send, SendCrLf, Key, Type}`.
- Produces: the steps `send-ahead`, `send-crlf-ahead`, `key-ahead`, `type-ahead`.

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, replace:

````rust
    #[test]
    fn parses_esp_writes() {
````

with:

````rust
    #[test]
    fn input_typed_ahead_is_sent_as_other_input_is() {
        let s = parse_scenario(
            "x",
            "send-ahead a b\nkey-ahead {ctrl-c}\ntype-ahead q\nsend-crlf-ahead c\n",
        )
        .unwrap();
        assert_eq!(
            s.steps,
            vec![
                (1, Step::Send("a b".into())),
                (2, Step::Key("{ctrl-c}".into())),
                (3, Step::Type("q".into())),
                (4, Step::SendCrLf("c".into())),
            ]
        );
        // A key name is checked as for `key` and `type`.
        assert!(parse_scenario("x", "key-ahead {nope}").is_err());
        assert!(parse_scenario("x", "type-ahead {nope}").is_err());
    }

    #[test]
    fn parses_esp_writes() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `e2e::tests::input_typed_ahead_is_sent_as_other_input_is`.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//! type <text>                      (as key, without the Enter)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

with:

````rust
//! type <text>                      (as key, without the Enter)
//! send-ahead <text>                (as send, while something runs on purpose:
//!                                   input for a program, a line typed ahead;
//!                                   send-crlf-ahead, key-ahead and type-ahead
//!                                   likewise)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

Replace:

````rust
            }
            "send" => Step::Send(rest.to_string()),
            "send-crlf" => Step::SendCrLf(rest.to_string()),
            "key" => {
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
            }
            "type" => {
                keys::typed(rest).with_context(|| format!("{name}:{line_no}"))?;
````

with:

````rust
            }
            "send" | "send-ahead" => Step::Send(rest.to_string()),
            "send-crlf" | "send-crlf-ahead" => Step::SendCrLf(rest.to_string()),
            "key" | "key-ahead" => {
                keys::presses(rest).with_context(|| format!("{name}:{line_no}"))?;
                Step::Key(rest.to_string())
            }
            "type" | "type-ahead" => {
                keys::typed(rest).with_context(|| format!("{name}:{line_no}"))?;
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 91 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
feat(xtask): add scenario steps for input typed ahead

A line sent while a program runs is echoed twice, by the line
discipline as it is typed and by the shell's editor once the console is
handed back, as on Linux (spec §15 item 3), and can land inside the
output an expect waits for. send-ahead, send-crlf-ahead, key-ahead and
type-ahead send as send, send-crlf, key and type do, and mark the input
that a scenario sends while something runs on purpose, so that every
other input can be held to wait for the prompt.
EOF
````


### Task 16: Every scenario's input waits for its prompt

Decision 12: a line sent while a program runs is echoed twice, by the line discipline as it is typed and by the shell's editor once the console is handed back (`kernel/src/input.rs`'s `set_line_mode`), as on Linux, and a line sent before the command before it has ended can land inside the output an `expect` waits for (#103's first CI run). Every `send`, `key` and `type` now follows an `expect` of the prompt, mostly one added just before it (an `expect` matches what is unconsumed and its `$` is the end of what has arrived, so one added just before the input also consumes a prompt an earlier expect left); 354 were added by `tmp/m4p3/pacing/pace.py` from the scenarios before this task, with `pacing/overrides.py`'s reasons. Input for a running program, a Ctrl-C, and a line beside a background job whose output may follow the prompt are marked `-ahead` instead (45); a line beside background scripts whose traces may follow the prompt waits for the prompt wherever it ends (`jobs`, the review's M2, so that `sh b.sh` is echoed once). A `kill` and the `wait` after it are one line (`jobs`, `script_vars`), as in Task 14; expectations that matched the prompt before an echo (`spawn`), or the kernel's lines for the stick after a prompt (`unplug`), or a child's line after it (`console`), wait for that prompt first; `jobs` does not pin the extra empty prompt after a `wait` (`progress.md` e1, offered as a task of its own). Every scenario passed twice after the change, in about the same time.

**Files:**
- Modify: `tests/e2e/bigfile.txt`
- Modify: `tests/e2e/console.txt`
- Modify: `tests/e2e/control.txt`
- Modify: `tests/e2e/ctrlc.txt`
- Modify: `tests/e2e/diskfull.txt`
- Modify: `tests/e2e/fileops.txt`
- Modify: `tests/e2e/files.txt`
- Modify: `tests/e2e/jobs.txt`
- Modify: `tests/e2e/keyboard.txt`
- Modify: `tests/e2e/memory.txt`
- Modify: `tests/e2e/mount_fail.txt`
- Modify: `tests/e2e/noroot.txt`
- Modify: `tests/e2e/persist.txt`
- Modify: `tests/e2e/pipe_calls.txt`
- Modify: `tests/e2e/pipes.txt`
- Modify: `tests/e2e/proc_calls.txt`
- Modify: `tests/e2e/programs.txt`
- Modify: `tests/e2e/respawn.txt`
- Modify: `tests/e2e/script_vars.txt`
- Modify: `tests/e2e/sh.txt`
- Modify: `tests/e2e/shell.txt`
- Modify: `tests/e2e/spawn.txt`
- Modify: `tests/e2e/stale_program.txt`
- Modify: `tests/e2e/sysinfo.txt`
- Modify: `tests/e2e/system.txt`
- Modify: `tests/e2e/tees.txt`
- Modify: `tests/e2e/unplug.txt`
- Modify: `tests/e2e/userfault.txt`
- Modify: `tests/e2e/utils.txt`

**Interfaces:**
- Consumes: Task 15's steps.
- Produces: every `tests/e2e/*.txt` but `test_cmd`'s, paced.

- [ ] **Step 1: Expect the new lines in `tests/e2e/bigfile.txt`**

Replace the whole of `tests/e2e/bigfile.txt` with:

````text
# Building an 8 MiB file on the ext2 root (spec §9.3 #5): a 4 KiB file of
# 64-byte lines, doubled with >> until it is 8 MiB (double-indirect
# blocks), then copied. The host reads both from the disk with debugfs.
timeout 120
expect root@relay:~# $
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
expect root@relay:~# $
send cat l l l l l l l l l l l l l l l l > m
expect root@relay:~# $
send cat m m m m > l
expect root@relay:~# $
send wc l
expect \n\s*64\s+64\s+4096 l\n
expect root@relay:~# $
send cp l big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send wc big
expect \n\s*131072\s+131072\s+8388608 big\n
expect root@relay:~# $
send cp big copy
expect root@relay:~# $
send rm t m
expect root@relay:~# $
send ls -l big copy
expect \n-rw-r--r-- 1 root root 8388608 .* big\n-rw-r--r-- 1 root root 8388608 .* copy\n
poweroff
file-lines /root/big 8388608 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
file-lines /root/copy 8388608 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/console.txt`**

In `tests/e2e/console.txt`, make these 5 replacements, top to bottom:

Replace:

````text
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
````

with:

````text
# Over COM1 and on the USB keyboard.
expect root@relay:~# $
send t-read
alive 1
send-ahead hello
expect \nhello\n\[6\] hello\\n\n
key-ahead wrold{backspace}{backspace}{backspace}{backspace}orld
expect wrold(\x08 \x08){4}orld\n\[6\] world\\n\n
# Ctrl-D hands over a line without its newline; on an empty line it ends
# the input.
type-ahead abc{ctrl-d}
expect abc\[3\] abc\n
type-ahead {ctrl-d}
expect end of input\nroot@relay:~# $
````

Replace:

````text
alive 1
key first line
expect \nfirst line\nfirst line\n
key second
expect second\nsecond\n
type {ctrl-d}
expect root@relay:~# $
````

with:

````text
alive 1
key-ahead first line
expect \nfirst line\nfirst line\n
key-ahead second
expect second\nsecond\n
type-ahead {ctrl-d}
expect root@relay:~# $
````

Replace:

````text
alive 1
send echo typed
expect echo typed\n\d+ iterations\nroot@relay:~# echo typed\ntyped\nroot@relay:~# $
# Raw mode: no echo, bytes as they come, until q.
send t-read raw
expect was line mode: true\n
send xyq
expect \[\d+\] xy
````

with:

````text
alive 1
send-ahead echo typed
expect echo typed\n\d+ iterations\nroot@relay:~# echo typed\ntyped\nroot@relay:~# $
# Raw mode: no echo, bytes as they come, until q.
send t-read raw
expect was line mode: true\n
send-ahead xyq
expect \[\d+\] xy
````

Replace:

````text
alive 1
type {ctrl-d}
expect t-read\nend of input\nroot@relay:~# $
# A program outside the console's groups cannot put it in line mode
# while the shell waits at its prompt (milestone 3).
send t-read leave
expect \nroot@relay:~# line mode from outside: EPERM\n
send echo ok
````

with:

````text
alive 1
type-ahead {ctrl-d}
expect t-read\nend of input\nroot@relay:~# $
# A program outside the console's groups cannot put it in line mode
# while the shell waits at its prompt (milestone 3).
send t-read leave
expect \nroot@relay:~# 
expect line mode from outside: EPERM\n
send echo ok
````

Replace:

````text
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

with:

````text
send echo t-spin 2 > s.sh
expect root@relay:~# $
send echo t-args one >> s.sh
expect root@relay:~# $
send echo t-args two >> s.sh
expect root@relay:~# $
send sh s.sh
alive 1
type-ahead abc
expect abc\d+ iterations\n\+ t-args one\n\[1\] one\n\+ t-args two\n\[1\] two\nroot@relay:~# abc$
type-ahead {ctrl-c}
expect root@relay:~# $
````

- [ ] **Step 3: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, make these 6 replacements, top to bottom:

Replace:

````text
send t-spin; echo not-run
alive 1
type {ctrl-c}
expect \n\^C\nroot@relay:~# $
# & mid-line starts a job, and the line goes on.
````

with:

````text
send t-spin; echo not-run
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
# & mid-line starts a job, and the line goes on.
````

Replace:

````text
send echo 'echo s1 &&' > s.sh
send echo '  echo s2' >> s.sh
send sh s.sh
````

with:

````text
send echo 'echo s1 &&' > s.sh
expect root@relay:~# $
send echo '  echo s2' >> s.sh
expect root@relay:~# $
send sh s.sh
````

Replace:

````text
send while cd; do cd; done
alive 1
type {ctrl-c}
expect \n\^C\nroot@relay:~# $
send echo $?
````

with:

````text
send while cd; do cd; done
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
send echo $?
````

Replace:

````text
alive 2
type {ctrl-c}
expect \^C\nroot@relay:~# $
````

with:

````text
alive 2
type-ahead {ctrl-c}
expect \^C\nroot@relay:~# $
````

Replace:

````text
alive 1
type {ctrl-c}
expect \n\^C\nroot@relay:~# $
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
````

Replace:

````text
alive 1
type {ctrl-c}
expect \^C\nroot@relay:~# $
````

with:

````text
alive 1
type-ahead {ctrl-c}
expect \^C\nroot@relay:~# $
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/ctrlc.txt`**

In `tests/e2e/ctrlc.txt`, make these 7 replacements, top to bottom:

Replace:

````text
alive 1
key echo typed-while-it-spins
expect \n\d+ iterations\n
````

with:

````text
alive 1
key-ahead echo typed-while-it-spins
expect \n\d+ iterations\n
````

Replace:

````text
# a built-in, and the kernel log names the process.
send t-spin
alive 1
key {ctrl-c}
expect \n\^C\nroot@relay:~# 
````

with:

````text
# a built-in, and the kernel log names the process.
expect root@relay:~# $
send t-spin
alive 1
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
````

Replace:

````text
# stops it too, although a tick almost never finds it in ring 3.
send t-mem churn
alive 1
key {ctrl-c}
timeout 3
````

with:

````text
# stops it too, although a tick almost never finds it in ring 3.
expect root@relay:~# $
send t-mem churn
alive 1
key-ahead {ctrl-c}
timeout 3
````

Replace:

````text
send t-spin
key {ctrl-c}
expect \n\^C\nroot@relay:~# 
````

with:

````text
send t-spin
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
````

Replace:

````text
alive 1
key {ctrl-c}
expect t-spawn sleepers\n(pid \d+ \(/bin/t-spawn\): killed: Ctrl-C\n){4}\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
key echo nope{ctrl-c}
expect \^C\n
send echo still here
````

with:

````text
alive 1
key-ahead {ctrl-c}
expect t-spawn sleepers\n(pid \d+ \(/bin/t-spawn\): killed: Ctrl-C\n){4}\^C\nroot@relay:~# 
# At the prompt, Ctrl-C only cancels the line being typed.
key echo nope{ctrl-c}
expect \^C\n
expect root@relay:~# $
send echo still here
````

Replace:

````text
# until the program kills it.
send t-spawn ctrl-c
expect \nwaiting for t-spin\n
alive 1
type {ctrl-c}x
expect \Await: EINTR\nthen read: "x"\n
````

with:

````text
# until the program kills it.
expect root@relay:~# $
send t-spawn ctrl-c
expect \nwaiting for t-spin\n
alive 1
type-ahead {ctrl-c}x
expect \Await: EINTR\nthen read: "x"\n
````

Replace:

````text
alive 1
type {ctrl-c}x
expect \Athe child's wait: a child\nthe child's wait: a child\nthen read: "\\u\{3\}x"\n
````

with:

````text
alive 1
type-ahead {ctrl-c}x
expect \Athe child's wait: a child\nthe child's wait: a child\nthen read: "\\u\{3\}x"\n
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/diskfull.txt`**

Replace the whole of `tests/e2e/diskfull.txt` with:

````text
# A full disk (spec §9.3 #7): the 32 MiB root is filled until writes fail
# with ENOSPC; the shell says so, space comes back after rm, and e2fsck
# finds the disk clean.
disk small
timeout 120
expect root@relay:~# $
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
expect root@relay:~# $
send cat l l l l l l l l l l l l l l l l > m
expect root@relay:~# $
send cat m m m m > l
expect root@relay:~# $
send cp l big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect root@relay:~# $
send cat big > t
expect root@relay:~# $
send cat t >> big
expect \ncat: write error: No space left on device\n
expect root@relay:~# $
send df
expect \n/dev/root\s+\d+\s+\d+\s+0 100% /\n
expect root@relay:~# $
send echo more > more
expect \necho: write error: No space left on device\n
# A program sees the write error too, through the kernel: on its
# standard output, which the shell redirected, and on a file it opened.
expect root@relay:~# $
send t-files full > more
expect \nwriting standard output: ENOSPC\n
expect writing a file of its own: ENOSPC\nroot@relay:~# $
# A console tee that cannot be written: its pop says why.
send t-tee full
expect \npop: ENOSPC\nroot@relay:~# $
send rm t
expect root@relay:~# $
send echo more > more
expect root@relay:~# $
send cat more
expect \nmore\n
poweroff
````

- [ ] **Step 6: Expect the new lines in `tests/e2e/fileops.txt`**

Replace the whole of `tests/e2e/fileops.txt` with:

````text
# File operations on the ext2 root (spec §9.3 #3), typed over serial. The
# shell syncs after every command, and e2fsck checks the disk afterwards.
timeout 30
expect root@relay:~# $
send pwd
expect \n/root\n
expect root@relay:~# $
send mkdir -p /root/a/b/c
expect root@relay:~# $
send echo hello > /root/a/f
expect root@relay:~# $
send echo world >> /root/a/f
expect root@relay:~# $
send cat /root/a/f
expect \nhello\nworld\n
expect root@relay:~# $
send ls -l /root/a
expect \ntotal 8\ndrwxr-xr-x 3 root root 4096 \w{3} [ \d]\d \d\d:\d\d b\n-rw-r--r-- 1 root root   12 \w{3} [ \d]\d \d\d:\d\d f\n
expect root@relay:~# $
send cp /root/a/f /root/a/g
expect root@relay:~# $
send mv /root/a/g /root/a/b/h
expect root@relay:~# $
send ls /root/a /root/a/b
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
expect root@relay:~# $
send cat /root/a/b/h
expect \nhello\nworld\n
expect root@relay:~# $
send rmdir /root/a/b
expect \nrmdir: failed to remove '/root/a/b': Directory not empty\n
expect root@relay:~# $
send rm -r /root/a/b
expect root@relay:~# $
send ls /root/a
expect \nf\n
expect root@relay:~# $
send touch /root/t
expect root@relay:~# $
send stat /root/t
expect \n  File: /root/t\n  Size: 0 .*regular empty file\n
expect root@relay:~# $
send head -n 1 /root/a/f
expect \nhello\n
expect root@relay:~# $
send tail -n 1 /root/a/f
expect \nworld\n
expect root@relay:~# $
send wc /root/a/f
expect \n 2  2 12 /root/a/f\n
expect root@relay:~# $
send rm /root/a/f /root/t
expect root@relay:~# $
send rmdir /root/a
expect root@relay:~# $
send ls
expect \nREADME  checks\n
# Error messages follow GNU coreutils.
expect root@relay:~# $
send cat /root/nope
expect \ncat: /root/nope: No such file or directory\n
expect root@relay:~# $
send ls /nope
expect \nls: cannot access '/nope': No such file or directory\n
expect root@relay:~# $
send cp /root/nope /root/x
expect \ncp: cannot stat '/root/nope': No such file or directory\n
expect root@relay:~# $
send mkdir /root
expect \nmkdir: cannot create directory '/root': File exists\n
expect root@relay:~# $
send rm -r /
expect \nrm: it is dangerous to operate recursively on '/'\n
expect root@relay:~# $
send rm /
expect \nrm: cannot remove '/': Is a directory\n
expect root@relay:~# $
send ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
poweroff
````

- [ ] **Step 7: Expect the new lines in `tests/e2e/files.txt`**

In `tests/e2e/files.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect chdir to a file: ENOTDIR\nchdir to nothing: ENOENT\nmy working directory: /root/t-files.c\nmy working directory: /root\ngetcwd: /root\n
send pwd
````

with:

````text
expect chdir to a file: ENOTDIR\nchdir to nothing: ENOENT\nmy working directory: /root/t-files.c\nmy working directory: /root\ngetcwd: /root\n
expect root@relay:~# $
send pwd
````

Replace:

````text
# which may get their inodes, keep what they hold.
send t-files gone
````

with:

````text
# which may get their inodes, keep what they hold.
expect root@relay:~# $
send t-files gone
````

- [ ] **Step 8: Expect the new lines in `tests/e2e/jobs.txt`**

Replace the whole of `tests/e2e/jobs.txt` with:

````text
# Background jobs (user-space gate §9.2, §9.3; milestone 3, plan 2): a
# line that ends with & starts a job in a process group of its own,
# without the console; jobs, ps, wait and kill show and end them, and the
# shell says how each ended, in bash's words.
timeout 60
expect root@relay:~# $
send sleep 4 &
expect \n\[1\] \d+\n
expect root@relay:~# $
send t-spin &
expect \n\[2\] \d+\n
expect root@relay:~# $
send jobs
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# ps shows them both, with init and the shell; t-spin shares the CPU.
expect root@relay:~# $
send ps
expect \n  PID  PPID STATE      MEM  TIME CMD\n    1     0 wait         0  0:00 init\n    2     1 wait +\d+  0:00 /bin/sh\n
expect \A +\d+     2 sleep +\d+  0:00 /bin/sleep\n +\d+     2 (ready|run) +\d+  0:0\d /bin/t-spin\n +\d+     2 run +\d+  0:00 /bin/ps\n
# Ctrl-C at the prompt only cancels the line typed: the jobs run on.
expect root@relay:~# $
key echo nope{ctrl-c}
expect \^C\n
expect root@relay:~# $
send jobs
expect \n\[1\]-  Running                 sleep 4 &\n\[2\]\+  Running                 t-spin &\n
# Process 1 cannot be killed; a job's group can, and wait says so.
expect root@relay:~# $
send kill 1
expect \nrelay-sh: kill: \(1\) - Operation not permitted\n
expect root@relay:~# $
send kill %2; wait %2
expect \n(pid \d+ \(/bin/t-spin\): killed: kill\n)?\[2\]\+  Killed                  t-spin\n
expect root@relay:~# $
send wait %1
expect \n\[1\]\+  Done                    sleep 4\n
expect root@relay:~# $
send jobs
expect jobs\nroot@relay:~# $
# Ctrl-C ends a wait at the prompt; the job runs on until it is killed.
send t-spin &
expect \n\[1\] \d+\n
expect root@relay:~# $
send wait
alive 1
key-ahead {ctrl-c}
expect \n\^C\n
expect root@relay:~# $
send jobs
expect \n\[1\]\+  Running                 t-spin &\n
expect root@relay:~# $
send kill %1; wait
# A bare wait tells of a job a signal ended, as bash's does.
expect \n(pid \d+ \(/bin/t-spin\): killed: kill\n)?\[1\]\+  Killed                  t-spin\n
expect root@relay:~# $
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: kill\npid \d+ \(/bin/t-spin\): killed: kill\n
# A job reading the console gets end of input at once; a pipeline's output
# goes to the screen; a script in the background never takes the console.
expect root@relay:~# $
send cat &
expect \n\[1\] \d+\n
expect root@relay:~# $
send wait %1
expect \n\[1\]\+  Done                    cat\n
expect root@relay:~# $
send seq 3 | grep 2 &
expect \n\[1\] \d+\n
send-ahead wait %1
# Printed at the prompt or after the wait's echo, whichever came first.
expect (# |\n)2\n
expect \[1\]\+  Done                    seq 3 \| grep 2\n
expect root@relay:~# $
send echo 'sleep 1' > s.sh
expect root@relay:~# $
send echo 'echo the script ran' >> s.sh
expect root@relay:~# $
send sh s.sh &
expect \n\[1\] \d+\n
send-ahead echo the prompt is mine
expect \nthe prompt is mine\n
send-ahead wait %1
expect the script ran\n
expect \[1\]\+  Done                    sh s.sh\n
# Its transcript, a console tee, also got what the screen showed meanwhile
# (here what was typed at the prompt).
expect root@relay:~# $
send cat s.log
expect \n\+ sleep 1\n
expect \+ echo the script ran\nthe script ran\n
# Four scripts in the background, a transcript's tee each, leave room for
# a fifth in the foreground: a process pushes 4 tees, 64 in all.
expect root@relay:~# $
send echo 'sleep 2' > a.sh
expect root@relay:~# $
send sh a.sh &
expect \n\[1\] \d+\n
expect root@relay:~# 
send sh a.sh &
expect \n\[2\] \d+\n
expect root@relay:~# 
send sh a.sh &
expect \n\[3\] \d+\n
expect root@relay:~# 
send sh a.sh &
expect \n\[4\] \d+\n
expect root@relay:~# 
send echo 'echo hello' > b.sh
expect root@relay:~# 
send sh b.sh
# Typed only at the prompt: until then the console is still the ended
# script's, in line mode, which echoed a line typed early once more.
expect \+ echo hello\nhello\nroot@relay:~# $
# A bare wait says how the jobs it waited for ended (those that ended
# before it were told of at the prompt).
send wait
expect wait\n(\[\d\][-+ ]  Done                    sh a\.sh\n){0,4}root@relay:~# $
# exit leaves a running job to process 1, which collects it when it ends.
send t-spin 1 &
expect \n\[1\] \d+\n
expect root@relay:~# $
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
expect \d+ iterations\n
send jobs
expect jobs\nroot@relay:~# $
````

- [ ] **Step 9: Expect the new lines in `tests/e2e/keyboard.txt`**

Replace the whole of `tests/e2e/keyboard.txt` with:

````text
# Typing on the USB keyboard (spec §9.3 #2): QMP send-key goes through
# qemu-xhci and the HID boot keyboard driver to the shell.
timeout 30
expect \[ ok \] keyboard: 1 keyboard
expect root@relay:~# $
key echo hello
expect \nhello\n
# Shift for capitals and symbols.
expect root@relay:~# $
key echo Hello, World!
expect \nHello, World!\n
# Caps Lock: letters change, digits do not, Shift inverts it.
expect root@relay:~# $
key echo {caps_lock}abc1{caps_lock}def
expect \nABC1def\n
expect root@relay:~# $
key echo {caps_lock}aBc{caps_lock}
expect \nAbC\n
# Backspace and the cursor keys.
expect root@relay:~# $
key echo abcx{backspace}d
expect \nabcd\n
expect root@relay:~# $
key echo ac{left}b{end}d
expect \nabcd\n
# History: the up arrow brings back earlier lines (a repeated line is kept
# once).
expect root@relay:~# $
key {up}
expect \nabcd\n
expect root@relay:~# $
key {up}{up}{up}
expect \nABC1def\n
# Ctrl-C drops the line being typed.
expect root@relay:~# $
key echo nope{ctrl-c}
expect \^C\n
# The keyboard and the serial line type into the same shell.
expect root@relay:~# $
send echo serial
expect \nserial\n
expect root@relay:~# $
key echo keyboard
expect \nkeyboard\n
````

- [ ] **Step 10: Expect the new lines in `tests/e2e/memory.txt`**

In `tests/e2e/memory.txt`, make these 2 replacements, top to bottom:

Replace:

````text
expect \nrelay-sh: t-mem: killed \(page fault at 0x100000000000, read, ip 0x[0-9a-f]+\)\n
send t-mem unmapped-many
expect \nrelay-sh: t-mem: killed \(page fault at 0x1000000ff000, read, ip 0x[0-9a-f]+\)\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-mem map
expect frames lost: 0\n
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# The heap of relay-rt grows by mem_map (spec §8.1); when it cannot, the
# program says so and exits with 134; either way its memory comes back.
send t-mem grow 64
````

with:

````text
expect \nrelay-sh: t-mem: killed \(page fault at 0x100000000000, read, ip 0x[0-9a-f]+\)\n
expect root@relay:~# $
send t-mem unmapped-many
expect \nrelay-sh: t-mem: killed \(page fault at 0x1000000ff000, read, ip 0x[0-9a-f]+\)\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send t-mem map
expect frames lost: 0\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
# The heap of relay-rt grows by mem_map (spec §8.1); when it cannot, the
# program says so and exits with 134; either way its memory comes back.
expect root@relay:~# $
send t-mem grow 64
````

Replace:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
send t-mem grow 1
````

with:

````text
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send t-mem grow 1
````

- [ ] **Step 11: Expect the new lines in `tests/e2e/mount_fail.txt`**

In `tests/e2e/mount_fail.txt`, replace:

````text
expect \n\.\s+\.\.\s+bin\n
send touch /f
expect \ntouch: cannot touch '/f': Read-only file system\n
send echo still here
````

with:

````text
expect \n\.\s+\.\.\s+bin\n
expect root@relay:/# $
send touch /f
expect \ntouch: cannot touch '/f': Read-only file system\n
expect root@relay:/# $
send echo still here
````

- [ ] **Step 12: Expect the new lines in `tests/e2e/noroot.txt`**

In `tests/e2e/noroot.txt`, replace:

````text
send cd /
send rm -r /root
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:/# $
send pwd
expect \n/\n
send ls /
````

with:

````text
send cd /
expect root@relay:/# $
send rm -r /root
expect root@relay:/# $
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:/# $
send pwd
expect \n/\n
expect root@relay:/# $
send ls /
````

- [ ] **Step 13: Expect the new lines in `tests/e2e/persist.txt`**

In `tests/e2e/persist.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send mkdir /root/notes
send echo remember me > /root/notes/a
send echo and me >> /root/notes/a
send touch /root/notes/b
send ls /root/notes
````

with:

````text
send mkdir /root/notes
expect root@relay:~# $
send echo remember me > /root/notes/a
expect root@relay:~# $
send echo and me >> /root/notes/a
expect root@relay:~# $
send touch /root/notes/b
expect root@relay:~# $
send ls /root/notes
````

Replace:

````text
expect \nremember me\nand me\n
send ls /root/notes
````

with:

````text
expect \nremember me\nand me\n
expect root@relay:~# $
send ls /root/notes
````

- [ ] **Step 14: Expect the new lines in `tests/e2e/pipe_calls.txt`**

Replace the whole of `tests/e2e/pipe_calls.txt` with:

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
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send t-pipe basic
expect \npipe: 3 4\nwrite: 5\nread: hello\nfstat: fifo, dev 0\nseek: EINVAL\nread the write end: EBADF\nwrite the read end: EBADF\ntee: EINVAL\nafter the writer closed: 0\n
expect root@relay:~# $
send t-pipe room
expect \nwrite 20000: 16384\nread back: 16384, all as written: true\n
# 1 MiB through 16 KiB: the writer and the reader block in turn.
expect root@relay:~# $
send t-pipe child
expect-same sum \ndrained 1048576 bytes, checksum (\d+)\n
expect-same sum sent 1048576 bytes, checksum (\d+)\n
expect root@relay:~# $
send t-pipe eof
expect \nend of data once the child ended: 0\n
expect root@relay:~# $
send t-pipe epipe
expect \nwrite with no reader: EPIPE\nt-args with nobody reading: exited with 141\n
expect root@relay:~# $
send t-pipe killed
expect \na reader blocked on an empty pipe: kill\n
expect \na writer blocked on a full pipe: kill\n
expect root@relay:~# $
send dmesg
expect \npid \d+ \(/bin/t-pipe\): killed: kill\npid \d+ \(/bin/t-pipe\): killed: kill\n
expect root@relay:~# $
send t-pipe many
expect \npipes: 14, then EMFILE\nthe last fd: 31\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 15: Expect the new lines in `tests/e2e/pipes.txt`**

In `tests/e2e/pipes.txt`, make these 6 replacements, top to bottom:

Replace:

````text
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
````

with:

````text
alive 1
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde > l
expect root@relay:~# $
send cat l l l l l l l l l l l l l l l l > m
expect root@relay:~# $
send cat m m m m > l
expect root@relay:~# $
send cat l l l l l l l l l l l l l l l l > m
expect root@relay:~# $
send cat m m m m m m m m m m m m m m m m > l
expect root@relay:~# $
send cat l l l l l l l l > big
expect root@relay:~# $
send cat big | wc -c
expect \n8388608\n
expect root@relay:~# $
send cat big | cat | cat | wc -l
expect \n131072\n
# head has its line and ends; cat, writing to nobody, ends quietly.
expect root@relay:~# $
send cat big | head -n 1
````

Replace:

````text
expect \n5\n
send seq 1000 | grep 7 | wc -l
expect \n271\n
send seq 3 | grep -v 2
````

with:

````text
expect \n5\n
expect root@relay:~# $
send seq 1000 | grep 7 | wc -l
expect \n271\n
expect root@relay:~# $
send seq 3 | grep -v 2
````

Replace:

````text
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
# And each line reaches the next stage when Enter ends it.
send cat | cat
alive 1
key first line
expect \nfirst line\nfirst line\n
key second
expect second\nsecond\n
type {ctrl-d}
expect root@relay:~# $
````

with:

````text
send ls /etc | cat > out
expect root@relay:~# $
send cat out
expect \nhostname\nmotd\nroot@relay:~# $
send cat out nosuch | wc -l
expect \ncat: nosuch: No such file or directory\n2\n
expect root@relay:~# $
send nosuch | wc -l
expect \nrelay-sh: nosuch: command not found\n0\n
expect root@relay:~# $
send cd / | cat
expect \nrelay-sh: cd: cannot be used in a pipeline\n
# The first stage reads the console, a line at a time, to Ctrl-D.
expect root@relay:~# $
send cat | wc -l
alive 1
key-ahead one
key-ahead two
type-ahead {ctrl-d}
expect \ntwo\n2\nroot@relay:~# $
# And each line reaches the next stage when Enter ends it.
send cat | cat
alive 1
key-ahead first line
expect \nfirst line\nfirst line\n
key-ahead second
expect second\nsecond\n
type-ahead {ctrl-d}
expect root@relay:~# $
````

Replace:

````text
alive 1
key {ctrl-c}
expect \^C\nroot@relay:~# 
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
send t-spin | cat | cat
alive 1
key {ctrl-c}
expect \^C\nroot@relay:~# 
````

with:

````text
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
# Ctrl-C ends every stage of the pipeline, and the shell says so once.
send t-spin | cat | cat
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
````

Replace:

````text
# the screen gets.
send echo 'echo piped | cat' > p.sh
send sh p.sh
````

with:

````text
# the screen gets.
expect root@relay:~# $
send echo 'echo piped | cat' > p.sh
expect root@relay:~# $
send sh p.sh
````

Replace:

````text
send rm big l m out p.sh p.log
send free
````

with:

````text
send rm big l m out p.sh p.log
expect root@relay:~# $
send free
````

- [ ] **Step 16: Expect the new lines in `tests/e2e/proc_calls.txt`**

In `tests/e2e/proc_calls.txt`, replace:

````text
expect \none entry: pid 1, of more than one: true\nno entry: the same count: true\n
send t-proc long
````

with:

````text
expect \none entry: pid 1, of more than one: true\nno entry: the same count: true\n
expect root@relay:~# $
send t-proc long
````

- [ ] **Step 17: Expect the new lines in `tests/e2e/programs.txt`**

Replace the whole of `tests/e2e/programs.txt` with:

````text
# Programs run in ring 3 (user-space gate §5, §6.2; milestone 2, plan 2):
# the shell runs a name it does not know from /bin, or a path as
# given, and waits for it. What the program writes on fd 1 follows the
# redirection; memory in use is the same before and after (once the first
# program has made the kernel-stack area's page tables, which stay).
timeout 30
expect root@relay:~# $
send t-args a 'b c' ''
expect \n\[1\] a\n\[2\] b c\n\[3\] \nroot@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send /bin/t-args by-path
expect \n\[1\] by-path\nroot@relay:~# $
send cd /bin
expect root@relay:/bin# $
send ./t-args relative
expect \n\[1\] relative\nroot@relay:/bin# $
send cd
expect root@relay:~# $
send t-args one two > /root/args.txt
expect root@relay:~# $
send cat /root/args.txt
expect \n\[1\] one\n\[2\] two\n
expect root@relay:~# $
send t-args
expect t-args\nroot@relay:~# $
send echo not a program > /root/text
expect root@relay:~# $
send /root/text
expect \nrelay-sh: /root/text: Exec format error\n
expect root@relay:~# $
send /root/missing
expect \nrelay-sh: /root/missing: No such file or directory\n
expect root@relay:~# $
send t-missing
expect \nrelay-sh: t-missing: command not found\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 18: Expect the new lines in `tests/e2e/respawn.txt`**

In `tests/e2e/respawn.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send cd /
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
send echo alive
expect \nalive\n
send exit 3
````

with:

````text
send cd /
expect root@relay:/# $
send exit
expect \ninit: /bin/sh \(pid 2\) exited with 0; starting it again\nroot@relay:~# $
send echo alive
expect \nalive\n
expect root@relay:~# $
send exit 3
````

Replace:

````text
send t-spawn orphan
send-crlf exit
````

with:

````text
send t-spawn orphan
expect root@relay:~# $
send-crlf exit
````

- [ ] **Step 19: Expect the new lines in `tests/e2e/script_vars.txt`**

Replace the whole of `tests/e2e/script_vars.txt` with:

````text
# Script arguments and variables (user-space gate §9.4; milestone 3, plan
# 3): sh FILE ARG... gives the script $0, $1 on, $# and $@; NAME=value sets
# a variable, which $NAME and ${NAME} read, at the prompt too; $? is the
# last status. A value is never split into words, where bash splits an
# unquoted one (said where they differ), and a script's variables are its
# own.
timeout 30
expect root@relay:~# $
send echo 'echo "0=$0 1=$1 #=$#"' > /root/v.sh
expect root@relay:~# $
send echo 't-args "$@"' >> /root/v.sh
expect root@relay:~# $
send echo 't-args $@' >> /root/v.sh
expect root@relay:~# $
send echo 'false' >> /root/v.sh
expect root@relay:~# $
send echo 'echo "?=$?"' >> /root/v.sh
expect root@relay:~# $
send echo 'A="a  b"' >> /root/v.sh
expect root@relay:~# $
send echo 't-args $A "$A" "$UNSET" $UNSET' >> /root/v.sh
expect root@relay:~# $
send echo 'exit 3' >> /root/v.sh
expect root@relay:~# $
send sh /root/v.sh one 'two three' ''
expect \n\+ echo "0=\$0 1=\$1 #=\$#"\n0=/root/v.sh 1=one #=3\n
expect \A\+ t-args "\$@"\n\[1\] one\n\[2\] two three\n\[3\] \n
# bash would split `two three` into two words here.
expect \A\+ t-args \$@\n\[1\] one\n\[2\] two three\n
expect \A\+ false\n\+ echo "\?=\$\?"\n\?=1\n
# And `a  b` into `a` and `b` here.
expect \A\+ A="a  b"\n\+ t-args \$A "\$A" "\$UNSET" \$UNSET\n\[1\] a  b\n\[2\] a  b\n\[3\] \n
expect \A\+ exit 3\n
# The shell gets the script's status, and none of its variables.
expect root@relay:~# $
send echo $? "[$A]"
expect \n3 \[\]\n
# The transcript holds the lines as written.
expect root@relay:~# $
send cat /root/v.log
expect \n\+ echo "0=\$0 1=\$1 #=\$#"\n0=/root/v.sh 1=one #=3\n\+ t-args "\$@"\n
# At the prompt: variables, in a pipeline and in a redirection.
expect root@relay:~# $
send B='hi  there'
expect root@relay:~# $
send t-args $B "$B" ${B}!
expect \n\[1\] hi  there\n\[2\] hi  there\n\[3\] hi  there!\n
expect root@relay:~# $
send echo $B | wc -c
expect \n10\n
expect root@relay:~# $
send echo x > $NONE
expect \nrelay-sh: \$NONE: ambiguous redirect\n
expect root@relay:~# $
send echo $0 $# [$1]
expect \n/bin/sh 0 \[\]\n
# $? after a program that is not found, a Ctrl-C, a job and a wait.
expect root@relay:~# $
send nosuch
expect root@relay:~# $
send echo $?
expect \n127\n
expect root@relay:~# $
key echo no{ctrl-c}
expect \^C\n
expect root@relay:~# $
send echo $?
expect \n130\n
expect root@relay:~# $
send t-spin &
expect \n\[1\] \d+\n
expect root@relay:~# $
send echo $?
expect \n0\n
expect root@relay:~# $
send kill %1; wait %1
expect \n(pid \d+ \(/bin/t-spin\): killed: kill\n)?\[1\]\+  Killed                  t-spin\n
expect root@relay:~# $
send echo $?
expect \n137\n
# A shell started at the prompt has its own variables and name.
expect root@relay:~# $
send sh
expect \nroot@relay:~# $
send echo "$0 [$B]"
expect \nsh \[\]\n
expect root@relay:~# $
send exit
expect \nroot@relay:~# $
# A shell reading a pipe takes no more than each line, as bash's: cat
# gets the line after it.
send echo cat > /root/in.txt
expect root@relay:~# $
send echo 'echo "[$B]" no' >> /root/in.txt
expect root@relay:~# $
send cat /root/in.txt | sh
expect \necho "\[\$B\]" no\n
expect root@relay:~# $
send echo 'echo $0 $#' | sh
expect \nsh 0\n
expect root@relay:~# $
send echo $B
expect \nhi  there\n
````

- [ ] **Step 20: Expect the new lines in `tests/e2e/sh.txt`**

In `tests/e2e/sh.txt`, make these 8 replacements, top to bottom:

Replace:

````text
alive 1
send t-args collected
expect \n\[1\] collected\n
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
````

with:

````text
alive 1
expect root@relay:~# $
send t-args collected
expect \n\[1\] collected\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send /bin/sh
expect /bin/sh\nroot@relay:~# $
send mkdir /root/a
expect root@relay:~# $
send touch /root/a/x /root/a/y
expect root@relay:~# $
send ls /root/a
expect \nx  y\n
# Into a file a program writes lines, not columns, and never reads its
# own output back.
expect root@relay:~# $
send ls /root/a > /root/list
expect root@relay:~# $
send cat /root/list
expect \nx\ny\n
expect root@relay:~# $
send cat /root/list >> /root/list
expect \ncat: /root/list: input file is output file\n
expect root@relay:~# $
send nosuch
expect \nrelay-sh: nosuch: command not found\n
expect root@relay:~# $
send t-fault null-read
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)\n
# The command reads the console at once: it has it before it runs.
expect root@relay:~# $
send t-read
alive 1
send-ahead hello
expect \nhello\n\[6\] hello\\n\n
type-ahead {ctrl-d}
expect end of input\nroot@relay:~# $
````

Replace:

````text
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
````

with:

````text
alive 1
key-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# 
# Scripts: the transcript gets what the screen gets, a script's programs'
# output and a nested script's lines too.
send echo 'echo one' > /root/s.sh
expect root@relay:~# $
send echo 'sh /root/t.sh' >> /root/s.sh
expect root@relay:~# $
send echo 't-args x' >> /root/s.sh
expect root@relay:~# $
send echo 'echo two' > /root/t.sh
expect root@relay:~# $
send sh /root/s.sh
````

Replace:

````text
send echo sh > /root/i.sh
send echo 'echo after' >> /root/i.sh
send sh /root/i.sh
````

with:

````text
send echo sh > /root/i.sh
expect root@relay:~# $
send echo 'echo after' >> /root/i.sh
expect root@relay:~# $
send sh /root/i.sh
````

Replace:

````text
send echo sh > /root/j.sh
send echo 'echo never' >> /root/j.sh
send sh /root/j.sh
expect \+ sh\nroot@relay:~# $
send t-spin
alive 1
key {ctrl-c}
expect \^C\nroot@relay:~# 
send echo back
expect \nback\n
# Ctrl-C ends a script with its command.
send echo t-spin > /root/spin.sh
send sh /root/spin.sh
alive 1
key {ctrl-c}
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
````

with:

````text
send echo sh > /root/j.sh
expect root@relay:~# $
send echo 'echo never' >> /root/j.sh
expect root@relay:~# $
send sh /root/j.sh
expect \+ sh\nroot@relay:~# $
send t-spin
alive 1
key-ahead {ctrl-c}
expect \^C\nroot@relay:~# 
send echo back
expect \nback\n
# Ctrl-C ends a script with its command.
expect root@relay:~# $
send echo t-spin > /root/spin.sh
expect root@relay:~# $
send sh /root/spin.sh
alive 1
key-ahead {ctrl-c}
expect \+ t-spin\n(pid \d+ \(/bin/(t-spin|sh)\): killed: Ctrl-C\n){2}\^C\nroot@relay:~# 
````

Replace:

````text
alive 1
send t-spawn fill
````

with:

````text
alive 1
expect root@relay:~# $
send t-spawn fill
````

Replace:

````text
# shows one prompt and one Done line, and `jobs` that it was reported.
send sh &
````

with:

````text
# shows one prompt and one Done line, and `jobs` that it was reported.
expect root@relay:~# $
send sh &
````

Replace:

````text
expect \A(root@relay:~# \+ sh\n|\+ sh\n(\[1\]\+  Done {20}sh /root/k\.sh\n)?root@relay:~# )$
send echo next
expect \Aecho next\nnext\n(\[1\]\+  Done {20}sh /root/k\.sh\n)?root@relay:\S+# $
send jobs
expect \Ajobs\nroot@relay:\S+# $
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log /root/k.sh /root/k.log
send cd /tmp
````

with:

````text
expect \A(root@relay:~# \+ sh\n|\+ sh\n(\[1\]\+  Done {20}sh /root/k\.sh\n)?root@relay:~# )$
send-ahead echo next
expect \Aecho next\nnext\n(\[1\]\+  Done {20}sh /root/k\.sh\n)?root@relay:\S+# $
send jobs
expect \Ajobs\nroot@relay:\S+# $
send rm -r /root/a /root/list /root/s.sh /root/s.log /root/t.sh /root/t.log /root/spin.sh /root/spin.log /root/i.sh /root/i.log /root/j.sh /root/j.log /root/k.sh /root/k.log
expect root@relay:\S+# $
send cd /tmp
````

Replace:

````text
expect \n/root\n
send free
````

with:

````text
expect \n/root\n
expect root@relay:~# $
send free
````

- [ ] **Step 21: Expect the new lines in `tests/e2e/shell.txt`**

Replace the whole of `tests/e2e/shell.txt` with:

````text
# The shell at the end of boot, typed at over COM1: the commands that
# report on the machine, and the files of the ext2 root.
timeout 30
expect root@relay:~# $
send echo hello, world
expect \nhello, world\n
expect root@relay:~# $
send uname -a
expect \nRelay relay 0\.4\.0 x86_64\n
expect root@relay:~# $
send cat /etc/hostname
expect \nrelay\n
expect root@relay:~# $
send ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
expect root@relay:~# $
send date
expect \n(Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d\n
expect root@relay:~# $
send free
expect \nMem:\s+\d+\s+\d+\s+\d+\nHeap:\s+32768\s+\d+\s+\d+\n
expect root@relay:~# $
send df
expect \nFilesystem\s+1K-blocks\s+Used\s+Available\s+Use% Mounted on\n/dev/root\s+\d+\s+\d+\s+\d+\s+\d+% /\n
expect root@relay:~# $
send dmesg
expect \n\[ ok \] mount /: .*\n
expect root@relay:~# $
send nosuch
expect \nrelay-sh: nosuch: command not found\n
expect root@relay:~# $
````

- [ ] **Step 22: Expect the new lines in `tests/e2e/spawn.txt`**

In `tests/e2e/spawn.txt`, make these 4 replacements, top to bottom:

Replace:

````text
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
send t-spawn join
````

with:

````text
expect \n\[1\] warm-up\n
expect root@relay:~# $
send t-spawn 1000
expect-same frames \nfree frames before: (\d+)\n
expect-same frames free frames after: (\d+)\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send t-args a 'b c' ''
expect \n\[1\] a\n\[2\] b c\n\[3\] \nroot@relay:~# $
send t-spawn kill
expect \nt-spin: kill\nkill 1: EPERM\nkill 999999: ESRCH\npid above 1: true\n
expect root@relay:~# $
send dmesg
expect \npid \d+ \(/bin/t-spin\): killed: kill\n
expect root@relay:~# $
send t-spawn join
````

Replace:

````text
# line comes right after the command, with no output of t-args before it.
send t-spawn kill-new
````

with:

````text
# line comes right after the command, with no output of t-args before it.
expect root@relay:~# $
send t-spawn kill-new
````

Replace:

````text
# t-spin 2 runs, redirected.
send t-spawn orphan
send t-spin 2 > /root/o.txt
expect > /root/o.txt\n\d+ iterations\n
send cat /root/o.txt
expect # cat /root/o.txt\n\d+ iterations\nroot@relay:~# $
send free
````

with:

````text
# t-spin 2 runs, redirected.
expect root@relay:~# $
send t-spawn orphan
expect root@relay:~# $
send t-spin 2 > /root/o.txt
expect > /root/o.txt\n\d+ iterations\n
expect root@relay:~# $
send cat /root/o.txt
expect cat /root/o.txt\n\d+ iterations\nroot@relay:~# $
send free
````

Replace:

````text
# and t-spawn leave room for 61.)
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
send t-spawn 10
````

with:

````text
# and t-spawn leave room for 61.)
expect root@relay:~# $
send t-spawn fill
expect \nfilled the table with 61 children\n
alive 1
expect root@relay:~# $
send t-spawn 10
````

- [ ] **Step 23: Expect the new lines in `tests/e2e/stale_program.txt`**

In `tests/e2e/stale_program.txt`, replace:

````text
send cp /bin/t-abi /root/t-abi
send /root/t-abi
expect \nrelay-sh: /root/t-abi: Exec format error\n
send t-abi
expect \nrelay-sh: t-abi: Exec format error\n
send dmesg
expect \nspawn /root/t-abi: built for ABI 2\n
send t-args still
````

with:

````text
send cp /bin/t-abi /root/t-abi
expect root@relay:~# $
send /root/t-abi
expect \nrelay-sh: /root/t-abi: Exec format error\n
expect root@relay:~# $
send t-abi
expect \nrelay-sh: t-abi: Exec format error\n
expect root@relay:~# $
send dmesg
expect \nspawn /root/t-abi: built for ABI 2\n
expect root@relay:~# $
send t-args still
````

- [ ] **Step 24: Expect the new lines in `tests/e2e/sysinfo.txt`**

In `tests/e2e/sysinfo.txt`, replace:

````text
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
send uname -a
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
send t-sys log
````

with:

````text
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
expect root@relay:~# $
send uname -a
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
expect root@relay:~# $
send t-sys log
````

- [ ] **Step 25: Expect the new lines in `tests/e2e/system.txt`**

Replace the whole of `tests/e2e/system.txt` with:

````text
# /bin is the system archive the loader read from the ESP (user-space gate
# spec §4.3, §4.4): its programs are listed and read like other files, and
# nothing in it can be changed or removed.
timeout 30
expect \[ ok \] system: \d+ programs?, ABI 3
expect root@relay:~# $
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
expect root@relay:~# $
send ls -l /bin/t-args
expect \n-rwxr-xr-x 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d /bin/t-args\n
expect root@relay:~# $
send cp /bin/t-args /root/t-args
expect root@relay:~# $
send ls -l /root/t-args
expect \n-rw-r--r-- 1 root root +\d+ \w{3} [ \d]\d \d\d:\d\d /root/t-args\n
expect root@relay:~# $
send touch /bin/t-args
expect \ntouch: cannot touch '/bin/t-args': Read-only file system\n
expect root@relay:~# $
send rm /bin/t-args
expect \nrm: cannot remove '/bin/t-args': Read-only file system\n
expect root@relay:~# $
send echo x > /bin/new
expect \nrelay-sh: /bin/new: Read-only file system\n
expect root@relay:~# $
send rmdir /bin
expect \nrmdir: failed to remove '/bin': Device or resource busy\n
expect root@relay:~# $
send mv /bin /root/bin
expect \nmv: cannot move '/bin' to '/root/bin': Device or resource busy\n
expect root@relay:~# $
send cd /bin
expect root@relay:/bin# $
send pwd
expect \n/bin\n
expect root@relay:/bin# $
send cd ..
expect root@relay:/# $
send ls /bin
expect \n\[      cp    dmesg  free  ls     poweroff  reboot  seq    stat   t-args   t-mem   t-read   t-sys  test   uname\ncat    date  echo   grep  mkdir  ps        rm      sh     sync   t-fault  t-pipe  t-spawn  t-tee  touch  wc\nclear  df    false  head  mv     pwd       rmdir   sleep  t-abi  t-files  t-proc  t-spin   tail   true\n
````

- [ ] **Step 26: Expect the new lines in `tests/e2e/tees.txt`**

Replace the whole of `tests/e2e/tees.txt` with:

````text
# Console tees (user-space gate §6.5; milestone 2, plan 3b): a tee gets what
# programs and the shell write to the console, written every 4 KiB and at
# every sync, until it is popped; at most 4 a process (64 in all,
# milestone 3); a process's tees end with it; a tee whose file is removed is
# gone.
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
expect root@relay:~# $
send t-tee typed
alive 1
send-ahead aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
send-ahead bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
send-ahead end
expect \nwritten as it was typed: true\nroot@relay:~# $
send rm t-tee.log t-tee.end
# Five processes with a tee each at once, as background scripts have.
expect root@relay:~# $
send t-tee owners
expect \n(a tee of its own: ok\n){5}root@relay:~# $
````

- [ ] **Step 27: Expect the new lines in `tests/e2e/unplug.txt`**

In `tests/e2e/unplug.txt`, make these 2 replacements, top to bottom:

Replace:

````text
send echo before > /root/f
send cat /root/f
expect \nbefore\n
unplug
````

with:

````text
send echo before > /root/f
expect root@relay:~# $
send cat /root/f
expect \nbefore\n
expect root@relay:~# $
unplug
````

Replace:

````text
expect \nrelay-sh: sync failed: Input/output error\n
send echo still here
expect \nstill here\n
# A program's power call keeps the machine up too, and says why.
send t-sys poweroff
expect \npower: EIO\n
send poweroff
````

with:

````text
expect \nrelay-sh: sync failed: Input/output error\n
expect root@relay:~# $
send echo still here
expect \nstill here\n
# A program's power call keeps the machine up too, and says why.
expect root@relay:~# $
send t-sys poweroff
expect \npower: EIO\n
expect root@relay:~# $
send poweroff
````

- [ ] **Step 28: Expect the new lines in `tests/e2e/userfault.txt`**

Replace the whole of `tests/e2e/userfault.txt` with:

````text
# A fault in ring 3 ends the program, not the kernel (user-space gate
# §11.1; milestone 2, plan 2): each kind of t-fault is killed with its
# message, the kernel log names it, the shell carries on, and memory in
# use is the same before and after. A program's flags stay its own, after
# an exit as after a fault, and through the timer ticks of a spin.
timeout 30
expect root@relay:~# $
send t-args warm-up
expect \n\[1\] warm-up\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send t-fault null-read
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, read, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault null-write
expect \nrelay-sh: t-fault: killed \(page fault at 0x0, write, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault write-code
expect \nrelay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, write, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault exec-data
expect \nrelay-sh: t-fault: killed \(page fault at 0x4[0-9a-f]+, execute, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault ud
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault div0
expect \nrelay-sh: t-fault: killed \(divide error, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault sse
expect \nrelay-sh: t-fault: killed \(FPU/SSE instruction, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault gsbase
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault stack
expect \nrelay-sh: t-fault: killed \(stack overflow at 0x7fffffeff[0-9a-f]{3}, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault kernel-read
expect \nrelay-sh: t-fault: killed \(page fault at 0xffff800000000000, read, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-fault flags-exit
expect root@relay:~# $
send t-args after-flags
expect \n\[1\] after-flags\n
expect root@relay:~# $
send t-fault flags-ud
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-args after-a-fault
expect \n\[1\] after-a-fault\n
expect root@relay:~# $
send t-fault flags-tf
expect \nrelay-sh: t-fault: killed \(CPU exception 1, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-args after-the-trap-flag
expect \n\[1\] after-the-trap-flag\n
expect root@relay:~# $
send t-fault flags-ac
expect \nrelay-sh: t-fault: killed \(invalid opcode, ip 0x4[0-9a-f]+\)\n
expect root@relay:~# $
send t-args after-ticks-with-ac
expect \n\[1\] after-ticks-with-ac\n
expect root@relay:~# $
send t-fault
expect \nusage: t-fault null-read\|
expect root@relay:~# $
send dmesg
expect \npid \d+ \(/bin/t-fault\): killed: page fault at 0x0, read, ip 0x4[0-9a-f]+\n
expect root@relay:~# $
send t-args still here
expect \n\[1\] still\n\[2\] here\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
````

- [ ] **Step 29: Expect the new lines in `tests/e2e/utils.txt`**

Replace the whole of `tests/e2e/utils.txt` with:

````text
# One program per command (user-space gate §8.4; milestone 2, plans 4a
# and 4b): each /bin program prints exactly what milestone 1's command of
# the same name printed, run here by path, as a bare name runs it too.
# Output follows a redirection, into which a program writes lines and
# which it never reads back; errors stay on the screen, and memory in use
# is the same before and after.
timeout 30
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
expect root@relay:~# $
send /bin/pwd
expect \n/root\n
expect root@relay:~# $
send /bin/mkdir -p /root/a/b/c
expect root@relay:~# $
send /bin/echo hello > /root/a/f
expect root@relay:~# $
send /bin/echo world >> /root/a/f
expect root@relay:~# $
send /bin/cat /root/a/f
expect \nhello\nworld\n
expect root@relay:~# $
send /bin/ls -l /root/a
expect \ntotal 8\ndrwxr-xr-x 3 root root 4096 \w{3} [ \d]\d \d\d:\d\d b\n-rw-r--r-- 1 root root   12 \w{3} [ \d]\d \d\d:\d\d f\n
expect root@relay:~# $
send /bin/cp /root/a/f /root/a/g
expect root@relay:~# $
send /bin/mv /root/a/g /root/a/b/h
expect root@relay:~# $
send /bin/ls /root/a /root/a/b
expect \n/root/a:\nb  f\n\n/root/a/b:\nc  h\n
expect root@relay:~# $
send /bin/ls /root/a > /root/list
expect root@relay:~# $
send /bin/cat /root/list
expect \nb\nf\n
expect root@relay:~# $
send /bin/cat /root/list >> /root/list
expect \ncat: /root/list: input file is output file\n
expect root@relay:~# $
send /bin/rmdir /root/a/b
expect \nrmdir: failed to remove '/root/a/b': Directory not empty\n
expect root@relay:~# $
send /bin/rm -r /root/a/b
expect root@relay:~# $
send /bin/touch /root/t
expect root@relay:~# $
send /bin/stat /root/t
expect \n  File: /root/t\n  Size: 0 .*regular empty file\n
expect root@relay:~# $
send /bin/head -n 1 /root/a/f
expect \nhello\n
expect root@relay:~# $
send /bin/tail -n 1 /root/a/f
expect \nworld\n
expect root@relay:~# $
send /bin/wc /root/a/f
expect \n 2  2 12 /root/a/f\n
# Errors keep their place among the output, and never enter a file.
expect root@relay:~# $
send /bin/cat /root/a/f /root/nope /root/a/f
expect \nhello\nworld\ncat: /root/nope: No such file or directory\nhello\nworld\n
expect root@relay:~# $
send /bin/cat /root/nope > /root/out
expect \ncat: /root/nope: No such file or directory\n
expect root@relay:~# $
send /bin/stat /root/out
expect \n  File: /root/out\n  Size: 0 .*regular empty file\n
# A program of /bin copies onto the root (SysVfs's tests show that files
# of two filesystems with one inode number are two files).
expect root@relay:~# $
send /bin/cp /bin/t-args /root/t
expect /bin/cp /bin/t-args /root/t\nroot@relay:~# $
send /bin/rm /root/a/f /root/t /root/out /root/list
expect root@relay:~# $
send /bin/rmdir /root/a
expect root@relay:~# $
send /bin/ls
expect \nREADME  checks\n
expect root@relay:~# $
send /bin/rm /bin/t-args
expect \nrm: cannot remove '/bin/t-args': Read-only file system\n
expect root@relay:~# $
send /bin/rm -r /
expect \nrm: it is dangerous to operate recursively on '/'\n
expect root@relay:~# $
send /bin/ls /
expect \nbin  dev  etc  home  lost\+found  root  tmp  usr  var\n
expect root@relay:~# $
send /bin/uname -a
expect \nRelay relay \d+\.\d+\.\d+ x86_64\n
expect root@relay:~# $
send /bin/date
expect \n(Mon|Tue|Wed|Thu|Fri|Sat|Sun) \w{3} [ \d]\d \d\d:\d\d:\d\d UTC 20\d\d\n
expect root@relay:~# $
send /bin/df
expect \nFilesystem     1K-blocks +Used Available Use% Mounted on\n/dev/root +\d+ +\d+ +\d+ +\d+% /\n
expect root@relay:~# $
send /bin/dmesg
expect \n\[ ok \] system: \d+ programs, ABI 3\n
expect root@relay:~# $
send /bin/sync
expect root@relay:~# $
send /bin/free
expect \n +total +used +free\nMem: +\d+ +\d+ +\d+\nHeap: +\d+ +\d+ +\d+\n
expect root@relay:~# $
send free
expect-same mem \nMem:\s+\d+\s+(\d+)\s
poweroff /bin/poweroff
````

- [ ] **Step 30: Run every scenario**

Run: `cargo xtask test --e2e-only`

Expected: PASS: ends with `all 49 scenario(s) passed`.

- [ ] **Step 31: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 32: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): pace every scenario's input by its prompt

A line sent before the command before it has ended is echoed twice and
can land inside the output an expect waits for (spec §15 item 3;
#103's first CI run). Every send, key and type now follows an expect of
the prompt, mostly one added just before it, which also consumes a
prompt an earlier expect left. Input meant for a running program, a
Ctrl-C, and a line sent beside a background job whose output may follow
the prompt are marked -ahead instead; a line sent beside background
scripts whose traces may follow the prompt waits for the prompt
wherever it ends. A kill and the wait after it are
one line, so that the kernel's line for the killed job comes before
wait's; expectations that matched the prompt before an echo, or the
kernel's lines for the stick after a prompt, wait for that prompt
first. Every scenario passed twice after the change.
EOF
````


### Task 17: The scenario parser refuses input that no prompt paces

Decision 12: `parse_scenario` refuses a `send`, `send-crlf`, `key` or `type` unless an `expect` (or `expect-same`) that ends at a prompt has come since the input before it, the start, a reboot or a reset (`ends_at_prompt`: `root@relay:…#` with or without its blank, which the line's trim removes, and ` $`, or `> $`), and refuses an `-ahead` mark after such an expect, where the input waits for nothing (the review's M2). A test holds every scenario in `tests/e2e/` to the rule, and the parser's older tests send through `-ahead` forms or after a prompt. The red run is the parser's tests. Mutation checks: the refusal, what clears it (an `-ahead` input, a reboot or reset, a plain input), the prompt's pattern (four ways; a fifth, `> $` or `>` alone, is equivalent since the trim takes the blank), `expect-same` counting, and the `-ahead` guard (two ways), each broken, fail a test.

**Files:**
- Modify: `xtask/src/e2e.rs`

**Interfaces:**
- Consumes: Tasks 15's steps and 16's scenarios; `e2e::load_scenarios`.
- Produces: `e2e::ends_at_prompt(&str) -> bool` (private).

- [ ] **Step 1: Add the failing tests to `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
            "x",
            "# hi\ncmdline test=1 panic=ud\n\ntimeout 5\nexpect \\[ ok \\] cpu\nsend ls -l\nscreenshot-nonblank\n",
        )
````

with:

````rust
            "x",
            "# hi\ncmdline test=1 panic=ud\n\ntimeout 5\nexpect \\[ ok \\] cpu\nsend-ahead ls -l\nscreenshot-nonblank\n",
        )
````

Replace:

````rust
            ]
        );
    }

````

with:

````rust
            ]
        );
    }

    #[test]
    fn input_waits_for_a_prompt_unless_typed_ahead() {
        let at_prompt = |text: &str| {
            parse_scenario("x", text)
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        let prompt = "expect root@relay:~# $\n";
        for ok in [
            "expect root@relay:~# $\nsend a\n",
            "expect \\na\\nroot@relay:~# $\nsend a\n",
            "expect root@relay:/tmp# $\nkey a\n",
            "expect \\^C\\nroot@relay:~# \ntype a\n",
            "expect root@relay:\\S+# $\nsend-crlf a\n",
            "expect \\n> $\nsend a\n",
            "expect-same m (\\d+)\\nroot@relay:~# $\nsend a\n",
            // An expect of the prompt, then others, before the input.
            "expect root@relay:~# $\nexpect x\nalive 1\nsend a\n",
            "send-ahead a\nkey-ahead b\ntype-ahead c\nsend-crlf-ahead d\n",
        ] {
            assert_eq!(at_prompt(ok), Ok(()), "{ok:?}");
        }
        for (text, line) in [
            ("send a\n", 1),
            ("expect x\nsend a\n", 2),
            ("expect root@relay:~# $\nsend a\nsend b\n", 3),
            ("expect root@relay:~# $\nsend a\nexpect \\na\\n\nkey b\n", 4),
            ("expect root@relay:~# $\nsend a\nsend-ahead b\ntype c\n", 4),
            ("expect root@relay:~# $\nreboot\nsend a\n", 3),
            ("expect root@relay:~# $\nreset\nsend a\n", 3),
            ("expect root@relay:~# $\nreset-key\nsend a\n", 3),
            // A prompt with something after it is not the end of the text.
            ("expect root@relay:~# x\nsend a\n", 2),
            ("expect > x\nsend a\n", 2),
            ("expect a> \nsend a\n", 2),
        ] {
            let e = at_prompt(text).unwrap_err();
            assert!(e.starts_with(&format!("x:{line}: ")), "{text:?}: {e}");
            assert!(e.contains("wait for the prompt"), "{e}");
        }
        assert!(at_prompt(&format!("{prompt}send a")).is_ok());
        // Marked typed ahead right after the prompt, an input waits for
        // nothing (the review).
        for (text, word) in [
            ("expect root@relay:~# $\nsend-ahead a\n", "send-ahead"),
            ("expect x\nexpect \\n> $\nkey-ahead a\n", "key-ahead"),
        ] {
            let e = at_prompt(text).unwrap_err();
            assert!(
                e.contains(&format!("{word} after an expect of the prompt")),
                "{e}"
            );
        }
    }

    #[test]
    fn every_scenario_waits_for_its_prompts() {
        assert!(load_scenarios(None).unwrap().len() >= 49);
    }

````

Replace:

````rust
    fn parses_the_unplug_step() {
        let s = parse_scenario("x", "unplug\nsend ls").unwrap();
        assert_eq!(s.steps[0], (1, Step::Unplug));
````

with:

````rust
    fn parses_the_unplug_step() {
        let s = parse_scenario("x", "unplug\nsend-ahead ls").unwrap();
        assert_eq!(s.steps[0], (1, Step::Unplug));
````

Replace:

````rust
    fn parses_key_steps() {
        let s = parse_scenario("x", "key echo {up}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Key("echo {up}".into()))]);
        assert!(parse_scenario("x", "key {bogus}").is_err());
        let s = parse_scenario("x", "send-crlf ls").unwrap();
        assert_eq!(s.steps, vec![(1, Step::SendCrLf("ls".into()))]);
        let s = parse_scenario("x", "type {ctrl-d}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Type("{ctrl-d}".into()))]);
        assert!(parse_scenario("x", "type {bogus}").is_err());
    }
````

with:

````rust
    fn parses_key_steps() {
        let s = parse_scenario("x", "key-ahead echo {up}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Key("echo {up}".into()))]);
        let bad =
            |t: &str| parse_scenario("x", &format!("expect root@relay:~# $\n{t}")).unwrap_err();
        assert!(format!("{:#}", bad("key {bogus}")).contains("bogus"));
        let s = parse_scenario("x", "send-crlf-ahead ls").unwrap();
        assert_eq!(s.steps, vec![(1, Step::SendCrLf("ls".into()))]);
        let s = parse_scenario("x", "type-ahead {ctrl-d}").unwrap();
        assert_eq!(s.steps, vec![(1, Step::Type("{ctrl-d}".into()))]);
        assert!(format!("{:#}", bad("type {bogus}")).contains("bogus"));
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p xtask --bin xtask`

Expected: FAIL: 1 test fails: `e2e::tests::input_waits_for_a_prompt_unless_typed_ahead`.

- [ ] **Step 3: Change `xtask/src/e2e.rs`**

In `xtask/src/e2e.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//!                                   likewise)
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

with:

````rust
//!                                   likewise)
//! ```
//!
//! Every `send`, `send-crlf`, `key` and `type` waits for the prompt: since
//! the input before it (or the start, a reboot or a reset), an expect must
//! have ended at one (`root@relay:~# `, `root@relay:~# $` or `> $`), or the
//! scenario is refused. A line sent while a command runs is echoed twice,
//! by the line discipline and by the shell's editor, and lands inside the
//! output an expect waits for (programmable shell gate §15 item 3); input
//! sent while something runs on purpose is marked `-ahead`.
//!
//! ```text
//! screenshot-nonblank              (QMP screendump; top rows not one colour)
````

Replace:

````rust
    let mut steps = Vec::new();
    for (i, raw) in text.lines().enumerate() {
````

with:

````rust
    let mut steps = Vec::new();
    // An expect of the prompt has come since the last input.
    let mut paced = false;
    for (i, raw) in text.lines().enumerate() {
````

Replace:

````rust
        let rest = rest.trim();
        let step = match word {
````

with:

````rust
        let rest = rest.trim();
        let input = matches!(word, "send" | "send-crlf" | "key" | "type");
        if input && !paced {
            bail!(
                "{name}:{line_no}: {word} does not wait for the prompt: an expect that \
                 ends at it must come first, or it is {word}-ahead (sent while \
                 something runs)"
            );
        }
        if word.ends_with("-ahead") && paced {
            bail!(
                "{name}:{line_no}: {word} after an expect of the prompt: it waits for \
                 nothing, so it is {}",
                word.trim_end_matches("-ahead")
            );
        }
        if input || word.ends_with("-ahead") || matches!(word, "reboot" | "reset" | "reset-key") {
            paced = false;
        }
        if matches!(word, "expect" | "expect-same") && ends_at_prompt(rest) {
            paced = true;
        }
        let step = match word {
````

Replace:

````rust
        steps,
    })
}

````

with:

````rust
        steps,
    })
}

/// Whether an expect's pattern ends at a prompt: the shell's
/// (`root@relay:~# `, its blank trimmed with the line's, or `# $`) or the
/// `> ` of a command that goes on (`> $`).
fn ends_at_prompt(pattern: &str) -> bool {
    static PROMPT: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"root@relay:\S*#( \$)?$|> \$$").unwrap());
    PROMPT.is_match(pattern)
}

````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p xtask --bin xtask`

Expected: PASS: 93 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add xtask
git commit -F - <<'EOF'
feat(xtask): refuse a scenario's input that no prompt paces

The scenario parser refuses a send, send-crlf, key or type unless an
expect that ends at a prompt (`root@relay:…# `, perhaps then `$`, or
`> $`) has come since the input before it, the start, a reboot or a
reset (spec §15 item 3): a scenario can no longer type ahead by
mistake. Input sent while something runs on purpose is -ahead, and the
parser refuses that mark after an expect of the prompt, where the input
waits for nothing (the prototype's review). A test holds every scenario
to the rule.
EOF
````


### Task 18: AGENTS.md says that input waits for its prompt

Decision 12 and the review's M8: `AGENTS.md`'s e2e expectations gain Task 17's rule and the `-ahead` steps, and no longer say to match the prompt before a command's echo, which the expect before the input now consumes. Documentation only.

**Files:**
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: Task 17's rule.
- Produces: nothing new.

- [ ] **Step 1: Change `AGENTS.md`**

In `AGENTS.md`, replace:

````markdown
  trailing newline or the prompt it ended at; the command echo follows the
  prompt (match `root@relay:~# <cmd>`); output printed while the shell
  waits at its prompt has no newline before it; kernel log lines
  (`pid N (…): killed: …`) can arrive between other lines. `key` and
  `type` cannot type `{`, which opens a key name (`{ctrl-c}`), so send it
  over serial (`send`).
- **Mutation checks:** for a guard, break it and see a test fail. A
````

with:

````markdown
  trailing newline or the prompt it ended at; the command echo follows the
  prompt, which the `expect` before the input has consumed (match
  `<cmd>\n…`, or `\n<output>`); output printed while the shell waits at
  its prompt has no newline before it; kernel log lines (`pid N (…):
  killed: …`) can arrive between other lines. `key` and `type` cannot type
  `{`, which opens a key name (`{ctrl-c}`), so send it over serial
  (`send`). Every `send`, `key` and `type` waits for the prompt: an
  `expect` that ends at it must come since the input before, or the
  scenario is refused; input for a running program, a Ctrl-C, or a line
  beside a background job's output is `send-ahead`, `key-ahead` or
  `type-ahead`.
- **Mutation checks:** for a guard, break it and see a test fail. A
````

- [ ] **Step 2: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 3: Commit**

````bash
git add AGENTS.md
git commit -F - <<'EOF'
docs: say that every scenario input waits for its prompt

AGENTS.md's e2e expectations gain the scenario parser's rule (spec §15
item 3) and the -ahead steps, and no longer say to match the prompt
before a command's echo, which the expect before the input now
consumes.
EOF
````


### Finish PR 5

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m4p3/pacing
gh pr create --base main --head m4p3/pacing --title "test(e2e,xtask): pace every scenario's input by its prompt" --body-file - <<'EOF'
## What

Milestone 4, plan 3, Tasks 15–18: `send-ahead`, `send-crlf-ahead`, `key-ahead` and `type-ahead` for input sent while something runs on purpose; every scenario's other input waits for an `expect` of the prompt (354 added, 45 inputs marked), and the scenario parser refuses one that does not, or an `-ahead` mark right after a prompt; `AGENTS.md` says so. A line typed ahead is echoed twice and can land inside the output an expect waits for (#103's first CI run).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; plan 4's NUC check 6 runs `test` and `[` there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p3/pacing --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p3-pacing
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
