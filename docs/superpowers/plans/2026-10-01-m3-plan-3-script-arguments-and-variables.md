# Milestone 3 · Plan 3: Script arguments and variables — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scripts take arguments and the shell has variables. Outside single quotes `/bin/sh` and the in-process runner expand `$NAME`, `${NAME}`, `$0`…`$9`, `${N}`, `$#`, `$@` and `$?`, never splitting a value into words; a line of `NAME=value` sets variables, at the prompt and in scripts; `sh FILE a b` gives the script its arguments, and a script, a nested shell and `X | sh` each have variables of their own; `X | sh` reads a byte at a time. The spec gets §16 item 10. It ends with `cargo xtask ci` green, the new scenario `script_vars` among the 47.

**Architecture:** All in the shell crate and `/bin/sh`; the kernel and the ABI do not change. `parser::parse_line` gives each word as typed (`parser::Word`: pieces of text marked quoted or not, and parameters, `Piece::Param`, with the word's text as typed for messages); the commands, redirections and lines take the word type as a parameter. A new module `expand` holds a shell's `Vars` (its variables and arguments) and turns a parsed line into the words its commands get (`expand::expand`), a value never split, an unquoted empty word removed, `$@` a word per argument, every piece's room taken from a 64 KiB budget before it is made. `Shell::execute` parses, handles a line of assignments itself (`Word::assignment`, `Shell::assign`), expands, and hands the words to the runners as before; a script's `Script` carries its file and arguments, which `run_script` (in-process, the shell's own variables kept aside) and `run_file` (`/bin/sh`'s child shell) make the script's `Vars`. `Shell::named` gives `/bin/sh` its argument 0 as `$0`, and `run_input` reads a byte at a time.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model); bash 5.2 (`bash FILE a b`, `bash -c '…' x a b`, and interactive in a pty) for the expectations.

**Spec:** `docs/superpowers/specs/2026-09-29-user-space-gate-design.md` (§1.4, §8.2, §8.3, §9.4, §11.1, §12.1, §12.3, §13 step 8, §15, §16 item 10)
**Roadmap:** `docs/superpowers/plans/2026-10-01-milestone-3-roadmap.md` — this is plan 3 of 4 of milestone 3 (spec §13 step 8).

## In brief

- **Size.** 14 tasks in two code pull requests, plus this plan as PR 1 (decision 1): PR 2 the parser's parameters and the shell's variables and `$?`; PR 3 script arguments, `$0`, `X | sh` a byte at a time and the scenario `script_vars`. No NUC check: no check script holds a `$`, so their transcripts stay as they are, and plan 4's `check5.sh` runs variables and script arguments there.
- **A spike first** (decision 1). `$` was refused everywhere; with every parameter expanded to nothing, every scenario and check script passed and only the two host tests that pinned the refusal failed. So nothing typed today depends on it.
- **bash's words, without splitting** (decisions 2, 3, 4), checked against bash 5.2: `$10` is `$1` and a `0`, `${10}` argument 10; `"$@"` and `$@` give each argument as a word, an empty one only quoted, text joined to the first and the last; an unquoted empty expansion is no word, a quoted one an empty word; a `$` before no parameter is a `$`; `bad substitution`, `ambiguous redirect` and the syntax error at a missing `}` are bash's. A value is never split into words, where bash splits an unquoted one: each test that differs says so. bash's other parameters, operators and quotes (`$*`, `$$`, `$!`, `$-`, `$_`, `${A:-x}`, `${#A}`, `$'…'`) are unsupported syntax. A command that expands to nothing runs nothing, in a pipeline too (Task 5: the runners took a command's name for granted). No variable is set when a shell starts.
- **Assignments** (decision 5): a line of `NAME=value` sets each in turn, as bash's; `~` in a value as bash's; `A=1 cmd` and bash's `A+=x` are unsupported (programs get no environment; the user chose the second's refusal), and an assignment in a pipeline or the background is refused as a built-in is.
- **`$?` and scope** (decisions 6, 7, 8): `$?` is bash's after every kind of line (a pipeline, a job's start, `wait`, Ctrl-C, a syntax error, an assignment); a script, a nested `sh` and `X | sh` start with no variables and `$?` 0, nothing passes in or out, and the shell gets the script's status; `$0` is a script's file, else the shell's argument 0 (`/bin/sh`, `sh`); `sh FILE ARG...` takes its options before the file only, as bash's.
- **Limits** (decision 9): variables and a line's expansion hold at most 64 KiB each, counting each word too, so nothing typed can run `/bin/sh` out of heap.
- **Deferred minors** (decisions 10, 11): `X | sh` reads a byte at a time (plan 1's); `|&` is unsupported syntax again (plan 2's); the others go to plan 4.
- **The prototype's review.** A fresh reviewer (Opus) read the whole prototype, ran every crate's tests, throwaway host probes and QEMU scenarios under KVM on the NUC's CPU model, and compared with bash 5.2 (`bash FILE`, `bash -c`, a terminal), and found 1 critical, no important and 4 minor real defects. Each is fixed in a task of its own after the task it concerns, with a test that fails first where there is behaviour to test, and the mutation checks that show what it guards; one design call it raised went to the user (`NAME+=value`). It found correct the quoting and expansion of every form it tried against bash (values unsplit, as decided), the refusals and their statuses, redirection targets, assignments and their `~`, `$?` after every kind of line in the host tests and in QEMU (jobs, `wait`, scripts, a nested `sh`, `X | sh`, Ctrl-C), scripts' scope and `$0`, typing on QEMU's USB keyboard, commands that expand to nothing under both runners, the limits (the room taken before each piece, the measure equal to `spawn`'s argument limit), `X | sh`'s byte reads, and the scenario's strictness:

| Finding (review) | Decision |
|---|---|
| Critical (C1): a `${` followed by a character of several bytes (`${é}`, `"${…}"`) cut it in two and panicked the shell, from a script or `X \| sh` (101, the rest of the script unrun) | Fixed, Task 3: the character taken whole, bash's `bad substitution` |
| Minor (M1): the public `parser::parse` panicked on a line that does not expand | Fixed, Task 4: `ParseError::Expansion` |
| Minor (M2): `A+=2` (bash appends) was `command not found` | Fixed, Task 7 (the user chose): `unsupported syntax: A+=2` |
| Minor (M3): milestone 1's `~"/x"`, `~\/x` and `~''` no longer `/root` (now bash's), not recorded | Recorded in spec §16 item 10 (decision 4) |
| Minor (M4): the parser's module comment stale (`$'`, `${`, assignments, `\|&`, `>&`) | Fixed, Task 10 |
| Declined to judge: C1's rank (not reachable from the keyboard); the decided departures from bash (no splitting, `$E &` no job, `A=1 $E` refused); the byte reads' speed on the NUC; the e2e `key` step unable to type `{` and `}` | Kept critical by the no-panic rule; the user's decisions; nothing here is particular to the NUC; xtask's syntax, the keymap's unit tests cover the keys |

## Where this plan fits

Plan 3 of milestone 3 implements spec §13 step 8. It builds on plans 1 and 2: pipelines, `X | sh`, background jobs, and `wait`'s and `kill`'s statuses, which `$?` now reads. It leaves plan 4 (hardening and 0.4.0) a shell whose scripts take arguments and variables, which `check5.sh` will run on the NUC.

## Working conventions

- Plan 3 lands as **three pull requests** (table below). This plan, with the spec's §16 item 10 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-3-script-arguments-and-variables.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 10 and 14 have no failing run: one changes a comment, the other tests what exists, and its introduction says what the mutation checks showed. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m3p3/mutations.md`).
- **Bound every loop in a test.** A test that loops until a call returns 0, and collects what it gets, takes the whole machine down when the code under test never returns 0; a console read or a `wait` can wait for ever. Every loop here has a bound, and the shell's expansion stops at 64 KiB. Mutation checks run under an address-space limit and a timeout (`tmp/m3p3/mutate.py`).
- **The expectations are bash 5.2's.** `bash FILE a b`, `bash -c '…' x a b` and, for the prompt, an interactive bash in a pty (`tmp/m3p3/pty_bash.py`); what they printed is in `tmp/m3p3/progress.md` and `tmp/m3p3/bash/`. Where this shell departs (a value is never split into words), the test says so.
- **The NUC check scripts must not change**: this plan has no NUC check, and none of them holds a `$`.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests and scenarios that come with the change belong to its commit; a task that only adds tests names the module whose files it changes. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and the USB ones poll. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `37d9360` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m3p3/plan` | — | The spec's §16 item 10 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m3p3/shell` | 1–10 | Words as typed and their parameters; expansion without splitting; commands that expand to nothing; `NAME=value`; 64 KiB limits; `\|&`; the review's fixes (a substitution of any character, `parse`'s errors, `NAME+=value`, the parser's comment) | `lint`, `unit`, `e2e` |
| 3 | `m3p3/scripts` | 11–14 | Script arguments and scope under both runners; `$0`; `X \| sh` a byte at a time; the scenario `script_vars` | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, gate §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 3 adds no crate. The kernel has no dev-dependencies and does not depend on `shell`; `crates/usb` has no dependencies.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (gate §3.1); `relay-abi` holds no architecture detail, and keeps the same values on every architecture. Plan 3 changes neither the kernel nor the ABI: `relay_abi::VERSION` stays 3.
- Every command prints exactly what it prints in milestone 1 for files, and every milestone 1 scenario passes unchanged but for the startup line (gate §1.4). The NUC check scripts and their recorded transcripts do not change.
- What a person types, and everything programs pass to the kernel, is untrusted, as disk, device and firmware data are (M1 §10): nothing they do may make the shell or the kernel panic, index out of bounds, overflow, allocate without bound or loop forever. `/bin/sh`'s heap ends the program with 134 when it runs out. Nothing waits for ever but the error screen and a program waiting for its own input. Panics are for bugs only.
- Status lines are exactly `[ ok ] <step>` / `[FAIL] <step>: <reason>`; detail goes to the kernel log. The NUC's terminal has 120×33 cells and no serial port.
- Shell messages follow bash 5.2 and GNU coreutils; `/bin/sh` names itself `relay-sh`, a script's own messages say `sh:`.
- Missing tools fail tests, never skip them.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. Every action stays pinned to a full commit SHA with its release in a comment. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green on its own: output changes land with the scenarios and check scripts that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §16 item 10 in PR 1:

1. **Plan 3 is one plan** (§13) in three pull requests: this plan; the parser's parameters and the shell's variables and `$?`; script arguments and the scenario `script_vars`. It has no NUC check: no check script holds a `$`, so their transcripts do not change, and plan 4's `check5.sh` runs variables and script arguments there. A spike first expanded every parameter to nothing and ran every scenario and check script: all passed, and only the two host tests that pinned `$`'s refusal failed.
2. **What expands** (§9.4). Outside single quotes, `$NAME` and `${NAME}` (a name is `[A-Za-z_][A-Za-z0-9_]*`, the longest that follows), `$0` to `$9`, `${N}` for any argument (`$10` is `$1` and a `0`, as in bash), `$#`, `$@` and `$?`. A `$` alone, or before a character no parameter starts with, is a `$`. bash's other parameters, `$*`, `$$`, `$!`, `$-` and `$_`, are unsupported (`unsupported syntax: $*`), as are `$(`, `$((` and `` ` ``, and bash's other quotes `$'…'` and `$"…"`; so are `${NAME:-x}` and bash's other operators, `${#NAME}` and `${!NAME}` (`unsupported syntax: ${NAME:-x}`); a `${…}` that holds no name, number or special parameter is bash's `relay-sh: ${1A}: bad substitution` (status 1), and one without its `}` a syntax error (``syntax error: unexpected EOF while looking for matching `}'``, status 2). These end only their line: bash ends a non-interactive shell at them, but a failing line does not stop a script here.
3. **No word splitting** (§9.4). An expansion is never split: a value with spaces stays one word, where bash splits an unquoted one. An unquoted expansion that leaves a word empty removes it; a quoted one leaves an empty word. `$@` and `"$@"` give each argument as one word, none with no arguments; an empty argument is kept only quoted; text joined to them goes with the first and the last argument (`"a$@b"`), as bash's `"$@"` does. No variable is set when a shell starts: bash sets `HOME`, `PWD`, `PATH` and others, here they read as empty (`~` is `/root`). A command whose words expand to nothing runs nothing, with status 0, and a redirection after it still makes its file, as bash's do; in a pipeline it reads nothing and gives the next command an end, and in the background it starts no job.
4. **Where it expands** (§9.4). In command names, arguments and redirection targets, so `$C` may name a built-in; an unquoted target that expands to nothing is bash's `relay-sh: $f: ambiguous redirect` (status 1). Not in comments, nor in a background job's text, which stays as typed (as bash's `jobs` shows it), nor in a script's trace, `+ <line>`, which shows the line as written (bash's `set -x` shows the expanded words): the transcripts keep milestone 1's form. Every command of a pipeline is expanded before any starts. A `~` is `/root` only alone or before a `/` in the same unquoted piece, as bash's is, so `~"/x"`, `~\/x` and `~''` keep it, where milestone 1 made them `/root`, and so does `~$A`; a `~` that a variable holds never expands.
5. **Assignments** (§9.4). A line of only `NAME=value` words sets each in turn (`A=1 B=$A`), with status 0. The value expands as a word does but always makes one, empty or not (`A=`, `A="a b"`, `A=$B`); an unquoted `~` at its start or after a `:` is `/root`, as in bash. A word is an assignment only if its name and `=` are unquoted; `1A=x`, `A-B=x` and `"A"=x` are command names, as in bash. A redirection after the assignments makes its file, and they are made even when it cannot be (status 1), as in bash. `A=1 cmd`, which gives bash's command an environment, is unsupported (programs get none: `unsupported syntax: A=1 before a command`), and so is bash's `NAME+=value`, which appends (`unsupported syntax: A+=2`; the review found it run as a command, and the user chose the refusal); in a pipeline or with `&`, where bash's assignment changes nothing in the shell, it is refused as a built-in is (`relay-sh: A=1: cannot be used in a pipeline`, `… in the background`, status 1).
6. **`$?`** (§9.4). The status of the last line that ran: a pipeline's last command's, a background job's start (0, or 127 when nothing of it started), `wait`'s, `kill`'s and `jobs`', 130 after Ctrl-C, 2 after a syntax error, 126 and 127; a blank or comment line keeps it. A script starts with 0, and the shell that ran it gets the script's status.
7. **Whose variables** (§8.3, §9.4). Variables and arguments belong to one shell: a script, `X | sh` and a nested `sh` start with none, and nothing passes in or out (bash's, without `export`); the in-process runner keeps its own and a script's apart, as `/bin/sh` does. `$0` is a script's path as `sh` was given it, and outside a script the shell's argument 0, as bash's (`/bin/sh` for the one init starts, `sh` for one typed), and `relay-sh` in the in-process runner.
8. **Script arguments** (§8.3). `sh FILE [ARG]...` gives the script its arguments under both runners; `/bin/sh` passes them to the child shell, which reads them as `$1` on, where milestone 1's `sh` said `extra operand`. Options come only before the file, as bash's do, so what follows it is the script's (`sh f -x`), and `--` lets a file's name start with `-`.
9. **Limits.** A shell's variables, names and values together, hold at most 64 KiB, and a line expands to at most 64 KiB, counting one for each word as well as their bytes (`spawn` takes at most 64 KiB of arguments, §11.1). Beyond either the line stops there, with status 1: `relay-sh: B: the variables would hold more than 64 KiB`, `relay-sh: the line would expand to more than 64 KiB`. `/bin/sh`'s heap ends the program when it runs out, so nothing a person types may grow it without bound (`"$@"` many times over many arguments could).
10. **`X | sh` reads a byte at a time** (plan 1's deferred minor). It reads a pipe one byte at a time, as bash does, so a command it runs reads the rest of the input after its line, not what the shell took ahead of it.
11. **Plan 2's deferred minors.** `|&` (bash: both outputs into the pipe) is `unsupported syntax: |&` again, rather than bash's syntax error at `&`. The stray prompt of an interactive `sh` in a background script and bash's `(wd: …)` note go to plan 4, with plan 1's and plan 5's.
12. **Tests** (§12). The parser's parameters, refusals and assignments; the expansion against bash 5.2's words for each case, the departures named in each test; the shell's variables, `$?` and scripts' scope under both runners; the scenario `script_vars` (a script with arguments, the prompt, a nested `sh` and `X | sh`).

## Review Focus

The inputs and failure modes a person using this software is most likely to hit that the spec does not spell out, most likely first. Each has a test in the owning task:

1. **Quoting and expansion as typed:** a value with blanks unquoted and in double quotes, `'$A'` and `\$A`, an unset or empty variable alone, in an argument and in quotes, `"$@"` and `$@` with no arguments and with an empty one, text joined to `"$@"`, `$10` against `${10}`, a `$` before punctuation or at the end, `~` next to a variable or a quote. Expected: bash 5.2's words, but that a value is never split (each test that differs says so); an unquoted empty expansion is no word, a quoted one an empty word. Tests: `parameters_expand_to_their_values`, `a_value_with_blanks_stays_one_word`, `an_unquoted_empty_expansion_is_no_word_a_quoted_one_is` and `all_arguments_are_a_word_each` (Task 2), `a_dollar_before_no_parameter_is_a_dollar` and `tilde_at_the_start_of_a_word_is_home` (Task 2), `script_vars` (Task 14).
2. **What `$?` says:** after a command that is not found, a syntax error, a Ctrl-C at the prompt, a pipeline whose first command fails, a background job's start, `wait %n` of a killed job, an assignment, a blank line, a script's `exit` and a script's first line. Expected: bash's status in each case. Tests: `dollar_question_is_the_last_line_s_status` (Task 2), `a_command_that_expands_to_nothing_runs_nothing` (Task 5), `an_assignment_sets_a_variable_for_the_lines_after_it` (Task 6), `a_script_has_its_own_variables_and_arguments` (Task 11), `script_vars` (Task 14).
3. **Assignments a person tries:** `A=1 B=$A`, `A="a b"`, `A=`, `A=~/x`, `A=1 cmd`, `A+=2`, `A=1 | cat`, `A=1 &`, `A=1 > f` (and one that cannot be made), `1A=x`, `"A"=x`, a variable naming a built-in. Expected: bash's variables and statuses where bash sets them; `A=1 cmd` and `A+=2` unsupported; refusals in a pipeline and the background; command names as bash's. Tests: `a_word_with_an_unquoted_name_and_equals_sign_is_an_assignment`, `a_tilde_in_a_value_is_home_at_its_start_or_after_a_colon`, `an_assignment_before_a_command_is_unsupported`, `appending_to_a_variable_is_unsupported` (Task 7), `an_assignment_sets_a_variable_for_the_lines_after_it`, `a_variable_may_name_a_built_in`, `an_assignment_with_a_redirection_makes_its_file` and `an_assignment_cannot_be_in_a_pipeline_or_the_background` (Task 6).
4. **Lines that do not expand, and refusals:** `${A`, `${1A}`, `${é}` and `"${…}"` in a script, `${A:-x}`, `${#A}`, `$*`, `$$`, `$!`, `$(`, `$'…'`, `> $E`, `> "$E"`, `> $@`, a command that expands to nothing alone, with a redirection, in a pipeline and in the background. Expected: bash's messages (`bad substitution`, `ambiguous redirect`, its syntax error at a missing `}`) or `unsupported syntax: …`; nothing of the line runs; no panic anywhere. Tests: `bash_s_other_parameters_and_operators_are_unsupported` and `a_brace_without_its_end_is_a_syntax_error` (Task 2), `a_substitution_starting_with_any_character_is_read_whole` and `a_substitution_of_any_character_fails_only_its_line` (Task 3), `parse_says_why_a_line_does_not_expand` (Task 4), `a_substitution_that_names_no_parameter_is_bad`, `a_redirection_target_must_expand_to_one_word` and `a_line_that_does_not_expand_runs_nothing` (Task 2), `a_command_that_expands_to_nothing_runs_nothing` and `a_program_s_neighbour_that_expands_to_nothing_is_an_end` (Task 5), `a_bar_needs_a_command_on_each_side`'s `|&` (Task 9).
5. **Scripts' arguments and scope:** `sh f a 'b c' ''`, `sh f -x`, `sh -x f`, `sh -- -f.sh`, a script that runs a script with `"$@"`, a variable set before a script and in it, a nested `sh` at the prompt, `X | sh`, `$0` in each, a script whose command reads the rest of a piped script. Expected: bash's arguments and `$0`, but that nothing is split; nothing passes into or out of a script or a nested shell; a piped script's command reads the lines after its own. Tests: `a_script_gets_its_arguments`, `a_script_has_its_own_variables_and_arguments` and `bin_sh_gives_a_script_its_arguments` (Task 11), `a_shell_s_name_is_its_argument_0` (Task 12), `a_shell_reading_a_pipe_takes_no_more_than_each_line` (Task 13), `script_vars` (Task 14).
6. **Limits:** a huge value, many variables, a value replaced by a smaller one, `"$@"` many times over many arguments, a typed line of many empty words. Expected: refused with a message and status 1 once 64 KiB would be passed, nothing of the line run further, `/bin/sh` never out of heap. Tests: `the_variables_hold_at_most_64_kib` and `a_line_expands_to_at_most_64_kib` (Task 8), `a_line_beyond_the_limits_runs_nothing` (Task 8).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/shell/src/parser.rs` | `Word`, `Piece`, `Param`, `Word::typed`, `Word::assignment`; `$` read in and out of double quotes; refusals; `Line::is_blank`; `\|&`; `A=1 cmd` unsupported |
| `crates/shell/src/expand.rs` (new) | `Vars` (variables, arguments, `set`, `script`), `expand`, `value`, `redirect`, `plain`; the 64 KiB limits; `bad substitution`, `ambiguous redirect` |
| `crates/shell/src/shell.rs`, `crates/shell/src/lib.rs` | Expansion in `execute`; `assign`; assignments refused in a pipeline and the background; `named`; a script's variables in `run_script` and `run_file`; `run_input` a byte at a time |
| `crates/shell/src/runner.rs` | Commands that expand to nothing in a pipeline, under both runners; `Ran::script` boxed |
| `crates/shell/src/commands/script.rs` | `sh FILE [ARG]...`, options before the file only; `Script::{name, args}` |
| `crates/shell/src/testing.rs` | `Harness::lines` |
| `userland/sh/src/main.rs` | `$0` from argument 0 |
| `tests/e2e/script_vars.txt` (new) | The scenario `script_vars` |

---

## PR 1: The spec's revisions, the roadmap's notes and this plan

The spec's §16 item 10 with the parts of its body it corrects (the status line, §9.4's arguments and refusals); the roadmap's notes on plan 2's deferred minors and plan 3's NUC check; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record plan 3's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p3/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m3p3/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-01-m3-plan-3-script-arguments-and-variables.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-01-m3-plan-3-script-arguments-and-variables.md
git commit -m "docs(plan): add milestone 3 plan 3, script arguments and variables"
cargo xtask lint
git push -u origin m3p3/plan
gh pr create --base main --head m3p3/plan --title "docs(spec,plan): add milestone 3's plan 3, script arguments and variables" --body-file - <<'EOF2'
## What

The implementation plan of milestone 3's plan 3 ("script arguments and variables"), and the spec's §16 item 10 with its decisions: which parameters expand and how, against bash 5.2 (a value never split into words); assignments; what `$?` says; whose variables a script, a nested shell and `X | sh` see; `sh FILE ARG...` and `$0`; the 64 KiB limits; `X | sh` reading a byte at a time; `|&`; the corrections to §9.4 they bring. The roadmap places plan 2's deferred minors and says plan 3 has no NUC check.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m3p3/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m3p3/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-plan` and continue with PR 2.

---

## PR 2: Parameters, variables and `$?` in the shell (Tasks 1–10)

The shell's side of variables: the parser keeps which parts of a word were quoted and reads parameters; expansion replaces them from the shell's variables, arguments and last status, without splitting words; a command that expands to nothing runs nothing; `NAME=value` sets a variable; variables and expansion hold at most 64 KiB each; and `|&` is unsupported again (plan 2's deferred minor); with the review's fixes: a substitution starting with a character of several bytes, `parse`'s errors, `NAME+=value` and the parser's comment. Both runners expand the same way; scripts get no arguments yet.

Branch `m3p3/shell`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-shell`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p3/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-shell origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-shell
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p3/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p3/shell /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-shell m3p3/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p3/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: The parser keeps which parts of a word were quoted

Spec §9.4 and decision 2: expansion must know which parts of a word were quoted, so `$A` can expand outside single quotes and an unquoted empty word can go while a quoted one stays. The parser now gives each word as typed, `parser::Word`, its pieces of text each marked quoted (or escaped) or not (`Piece::Text`); `Command`, `Redirect`, `Line` and `Pipeline` take the word type as a parameter, `String` by default. An empty `''` or `""` still leaves a quoted piece. The new module `expand` turns a parsed line into the words its commands get, for now by joining each word's pieces; `Shell::execute` expands every line it parses, a script's blank-line check looks at the line as typed, and `parser::parse` keeps giving expanded words for the tests that run no shell. Nothing a command receives changes, so every existing test passes as it is. The red run is the shell's tests, which cannot find `Piece` or the `expand` module. The task is a refactor: its mutation checks are Task 2's.

**Files:**
- Create: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: plan 2's `parser::parse_line`, `Line`, `Command`, `Redirect`.
- Produces: `parser::Word { pieces: Vec<Piece> }`, `parser::Piece::Text(String, bool)`; `parser::{Line, Command, Redirect}<W = String>`, `parser::Pipeline<W = String>`; `parser::parse_line(line) -> Result<Line<Word>, ParseError>`; `expand::expand(&Line<Word>) -> Line` (crate-private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

    fn words(line: &str) -> Vec<String> {
````

with:

````rust

    /// `line` as `Shell::execute` runs it: parsed, then expanded.
    fn expanded(line: &str) -> Result<Line, ParseError> {
        parse_line(line).map(|l| crate::expand::expand(&l))
    }

    fn words(line: &str) -> Vec<String> {
````

Replace:

````rust
        assert_eq!(words("echo a'b c'd"), ["echo", "ab cd"]);
    }
````

with:

````rust
        assert_eq!(words("echo a'b c'd"), ["echo", "ab cd"]);
    }

    #[test]
    fn a_word_keeps_which_of_its_pieces_were_quoted() {
        let text = |t: &str, quoted| Piece::Text(t.into(), quoted);
        let l = parse_line(r#"a'b c'\d"e" '' "" ~/x"#).unwrap();
        let pieces: Vec<&[Piece]> = l.pipeline[0].words.iter().map(|w| &w.pieces[..]).collect();
        assert_eq!(
            pieces,
            [
                &[text("a", false), text("b cde", true)][..],
                &[text("", true)],
                &[text("", true)],
                &[text("/root/x", false)],
            ]
        );
        let c = &parse_line("echo >'o'ut").unwrap().pipeline[0];
        assert_eq!(
            c.redirect.as_ref().unwrap().path.pieces,
            [text("o", true), text("ut", false)]
        );
    }
````

Replace:

````rust
    fn a_line_ending_with_an_ampersand_runs_in_the_background() {
        let l = parse_line("sleep 5 &").unwrap();
        assert_eq!(l.pipeline[0].words, ["sleep", "5"]);
````

with:

````rust
    fn a_line_ending_with_an_ampersand_runs_in_the_background() {
        let l = expanded("sleep 5 &").unwrap();
        assert_eq!(l.pipeline[0].words, ["sleep", "5"]);
````

Replace:

````rust
        }
        let l = parse_line("cat f | wc -l > out &").unwrap();
        assert_eq!(l.pipeline.len(), 2);
````

with:

````rust
        }
        let l = expanded("cat f | wc -l > out &").unwrap();
        assert_eq!(l.pipeline.len(), 2);
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find `expand` in `crate` ``; `` cannot find type `Piece` in this scope ``.

- [ ] **Step 3: Create `crates/shell/src/expand.rs`**

Create `crates/shell/src/expand.rs`:

````rust
//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4).

use crate::parser::{Command, Line, Redirect, Word};

/// `line`'s commands with the words they get.
pub(crate) fn expand(line: &Line<Word>) -> Line {
    Line {
        pipeline: line.pipeline.iter().map(command).collect(),
        background: line.background.clone(),
    }
}

fn command(c: &Command<Word>) -> Command {
    Command {
        words: c.words.iter().map(Word::text).collect(),
        redirect: c.redirect.as_ref().map(|r| Redirect {
            path: r.path.text(),
            append: r.append,
        }),
    }
}
````

- [ ] **Step 4: Change `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
pub mod editor;
mod io;
````

with:

````rust
pub mod editor;
mod expand;
mod io;
````

- [ ] **Step 5: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
/// next one's input. A blank line is one command without words.
pub type Pipeline = Vec<Command>;

/// A command line: its pipeline, and, if it ends with `&`, what was typed
/// before the `&` (a background job's text, spec §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub pipeline: Pipeline,
    pub background: Option<String>,
}

/// One command: its words and where its output goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<String>,
    pub redirect: Option<Redirect>,
}

/// `> path` (truncate) or `>> path` (append).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    pub path: String,
    pub append: bool,
}
````

with:

````rust
/// next one's input. A blank line is one command without words.
pub type Pipeline<W = String> = Vec<Command<W>>;

/// A command line: its pipeline, and, if it ends with `&`, what was typed
/// before the `&` (a background job's text, spec §9.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<W = String> {
    pub pipeline: Pipeline<W>,
    pub background: Option<String>,
}

/// One command: its words and where its output goes. The parser gives
/// them as typed ([`Word`]), and expansion as the strings a command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<W>,
    pub redirect: Option<Redirect<W>>,
}

/// `> path` (truncate) or `>> path` (append).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect<W = String> {
    pub path: W,
    pub append: bool,
}

/// A word as typed: its pieces of text, each quoted (or escaped) or not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub pieces: Vec<Piece>,
}

/// A piece of a word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text, and whether it was quoted or escaped.
    Text(String, bool),
}

impl Word {
    /// Adds `c`, quoted or not, to the word's last piece, or a new one.
    fn push(&mut self, c: char, quoted: bool) {
        match self.pieces.last_mut() {
            Some(Piece::Text(text, q)) if *q == quoted => text.push(c),
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

    /// The word is one unquoted piece of text, all ASCII digits (`2` in
    /// `2>`).
    fn is_digits(&self) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t.bytes().all(|b| b.is_ascii_digit()))
    }

    /// The word's text, its pieces joined.
    pub fn text(&self) -> String {
        self.pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t, _) => t.as_str(),
            })
            .collect()
    }
}
````

Replace:

````rust
#[derive(Default)]
struct Word {
    text: String,
    /// Something (even `''`) was seen, so the word exists even if empty.
    started: bool,
    /// The word began with an unquoted `~`.
    tilde: bool,
    /// Part of the word was quoted or escaped.
    quoted: bool,
}

impl Word {
    fn finish(self) -> Option<String> {
        if !self.started {
            return None;
        }
        let t = self.text;
        if self.tilde && (t == "~" || t.starts_with("~/")) {
            Some(alloc::format!("{HOME}{}", &t[1..]))
        } else {
            Some(t)
        }
    }
````

with:

````rust
#[derive(Default)]
struct Building {
    word: Word,
    /// Something (even `''`) was seen, so the word exists even if empty.
    started: bool,
    /// The word began with an unquoted `~`.
    tilde: bool,
}

impl Building {
    /// A quote opens: the word exists, and has a quoted piece even if
    /// nothing is quoted (`''`).
    fn open_quote(&mut self) {
        self.started = true;
        if !matches!(self.word.pieces.last(), Some(Piece::Text(_, true))) {
            self.word.pieces.push(Piece::Text(String::new(), true));
        }
    }

    /// Adds a quoted or escaped character.
    fn quoted(&mut self, c: char) {
        self.started = true;
        self.word.push(c, true);
    }

    fn finish(self) -> Option<Word> {
        if !self.started {
            return None;
        }
        let mut word = self.word;
        let t = word.text();
        if self.tilde
            && (t == "~" || t.starts_with("~/"))
            && let Some(Piece::Text(first, _)) = word.pieces.first_mut()
        {
            first.replace_range(..1, HOME);
        }
        Some(word)
    }
````

Replace:

````rust
struct Parts {
    words: Vec<String>,
    redirect: Option<Redirect>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
````

with:

````rust
struct Parts {
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
````

Replace:

````rust
    /// word.
    fn end_word(&mut self, word: &mut Word) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish() else {
````

with:

````rust
    /// word.
    fn end_word(&mut self, word: &mut Building) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish() else {
````

Replace:

````rust
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
````

with:

````rust
    /// bash runs, is refused like one on a command before the last.
    fn take_before_pipe(&mut self) -> Result<Command<Word>, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
````

Replace:

````rust

/// The commands of `line`, whether or not it ends with `&`.
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| l.pipeline)
}

/// `line`'s commands, and whether it runs in the background.
pub fn parse_line(line: &str) -> Result<Line, ParseError> {
    let mut background = None;
    let mut pipeline = Vec::new();
    let mut parts = Parts::default();
    let mut word = Word::default();
    let mut chars = line.chars().peekable();
````

with:

````rust

/// The commands of `line`, whether or not it ends with `&`, their words
/// expanded (for callers that run no shell: tests).
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| crate::expand::expand(&l).pipeline)
}

/// `line`'s commands, their words as typed, and whether it runs in the
/// background.
pub fn parse_line(line: &str) -> Result<Line<Word>, ParseError> {
    let mut background = None;
    let mut pipeline = Vec::new();
    let mut parts = Parts::default();
    let mut word = Building::default();
    let mut chars = line.chars().peekable();
````

Replace:

````rust
                // `2>` redirects another stream in a real shell.
                if word.started && !word.quoted && word.text.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ParseError::Unsupported(alloc::format!("{}>", word.text)));
                }
````

with:

````rust
                // `2>` redirects another stream in a real shell.
                if word.started && word.word.is_digits() {
                    return Err(ParseError::Unsupported(alloc::format!(
                        "{}>",
                        word.word.text()
                    )));
                }
````

Replace:

````rust
            '\'' => {
                word.started = true;
                word.quoted = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.text.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
````

with:

````rust
            '\'' => {
                word.open_quote();
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
````

Replace:

````rust
            '"' => {
                word.started = true;
                word.quoted = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.text.push(chars.next().expect("peeked"));
                        }
                        Some(c @ ('$' | '`')) => return Err(ParseError::Unsupported(c.into())),
                        Some(c) => word.text.push(c),
                        None => return Err(ParseError::UnterminatedQuote),
````

with:

````rust
            '"' => {
                word.open_quote();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.quoted(chars.next().expect("peeked"));
                        }
                        Some(c @ ('$' | '`')) => return Err(ParseError::Unsupported(c.into())),
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
````

Replace:

````rust
            '\\' => match chars.next() {
                Some(c) => {
                    word.started = true;
                    word.quoted = true;
                    word.text.push(c);
                }
                None => return Err(ParseError::TrailingBackslash),
````

with:

````rust
            '\\' => match chars.next() {
                Some(c) => word.quoted(c),
                None => return Err(ParseError::TrailingBackslash),
````

Replace:

````rust
                word.started = true;
                word.text.push(c);
            }
````

with:

````rust
                word.started = true;
                word.word.push(c, false);
            }
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::editor::{Feed, LineEditor};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::editor::{Feed, LineEditor};
use crate::expand;
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse_line(line) {
            Ok(parser::Line {
````

with:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse_line(line).map(|l| expand::expand(&l)) {
            Ok(parser::Line {
````

Replace:

````rust
        for line in text.lines() {
            if matches!(parser::parse(line), Ok(p) if p.len() == 1 && p[0].words.is_empty() && p[0].redirect.is_none())
            {
````

with:

````rust
        for line in text.lines() {
            if matches!(parser::parse_line(line), Ok(l) if l.pipeline.len() == 1 && l.pipeline[0].words.is_empty() && l.pipeline[0].redirect.is_none())
            {
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 226 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell): keep which parts of a word were quoted

The parser now gives each word as typed: its pieces of text, each
marked quoted (or escaped) or not, so that a later pass can expand
parameters outside single quotes and tell a quoted empty word from an
unquoted one (user-space gate §9.4). `Command`, `Redirect`, `Line` and
`Pipeline` take the word type as a parameter, `String` by default.

The new `expand` module turns a parsed line into the words its
commands get; for now it joins each word's pieces. `Shell::execute`
expands every line it parses, and a script's blank-line check looks at
the line as typed. `parser::parse` keeps giving expanded words, for the
tests that run no shell. Nothing a command receives changes.
EOF
````


### Task 2: Parameters expand in words

Spec §9.4 and decisions 2, 3 and 4: outside single quotes `$NAME`, `${NAME}`, `$0`…`$9`, `${N}`, `$#`, `$@` and `$?` are pieces of their words (`Piece::Param`, `Param`), which `expand::expand` replaces from the shell's `Vars` and last status. A value is never split into words (bash splits an unquoted one: each test that differs says so); a word holding nothing quoted that expands to nothing is no word, a quoted one an empty word; `$@` gives each argument as a word of its own, text joined to the first and the last, an empty argument kept only quoted, as bash's `"$@"` does. A `$` before no parameter is a `$`, as in bash; bash's other parameters, operators and quotes (`$*`, `$$`, `$!`, `$-`, `$_`, `${A:-x}`, `${#A}`, `$'…'`, `$"…"`) and `$(`, `$((` are unsupported syntax; a `${` without its `}` is bash's syntax error; a `${…}` naming no parameter is bash's `bad substitution` and a redirection target that is no single word its `ambiguous redirect`, both status 1 with nothing run. Each word keeps its text as typed for those messages (`Word::typed`), so the parser reads through a `Cursor` that knows where each character is. A `~` is `/root` only alone or before a `/` in the same unquoted piece, so `~"/x"` and `~$A` keep it, as bash's do. The shell holds its `Vars` (none set yet, `$0` `relay-sh`); `Harness::lines` runs several lines in one shell. The red run is the shell's tests, which cannot find `Param`, `Vars` or the new `expand` items. Mutation checks: unquoted empty words kept, a redirection target of no word taken, `$@`'s first argument not quoted, an empty quote leaving no piece, `~$E` expanded, `$'` refused inside double quotes, `$_` taken, the braced operators and names taken, and an expansion error's status 2 each fail a test.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 1's `Word`, `Piece`, `expand`.
- Produces: `parser::Piece::Param(Param, bool)`, `parser::Param::{Name(String), Arg(usize), Count, All, Status, Bad(String)}`, `Word::typed: String`, `parser::is_name(&str) -> bool`, `ParseError::UnclosedBrace`; `expand::Vars::{new(name), get(name)}`, `expand::Error::{BadSubstitution, AmbiguousRedirect}`, `expand::expand(&Line<Word>, &Vars, status: i32) -> Result<Line, Error>`, `expand::plain(&Line<Word>) -> Line` (crate-private); `Harness::lines(&mut self, &[&str]) -> (i32, String)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/expand.rs`**

Replace the whole of `crates/shell/src/expand.rs` with:

````rust
//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4).

use crate::parser::{Command, Line, Redirect, Word};

/// `line`'s commands with the words they get.
pub(crate) fn expand(line: &Line<Word>) -> Line {
    Line {
        pipeline: line.pipeline.iter().map(command).collect(),
        background: line.background.clone(),
    }
}

fn command(c: &Command<Word>) -> Command {
    Command {
        words: c.words.iter().map(Word::text).collect(),
        redirect: c.redirect.as_ref().map(|r| Redirect {
            path: r.path.text(),
            append: r.append,
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        let l = parse_line(line).unwrap();
        expand(&l, vars, 3).map(|mut l| l.pipeline.remove(0).words)
    }

    /// A script `s.sh` run as `sh s.sh one 'two three' '' four`, with `A`
    /// set to `a  b` and `E` to nothing.
    fn script() -> Vars {
        Vars::of(
            &[("A", "a  b"), ("E", "")],
            &["s.sh", "one", "two three", "", "four"],
        )
    }

    #[test]
    fn parameters_expand_to_their_values() {
        // What bash prints for each, but where a value has blanks.
        let v = script();
        assert_eq!(
            words(r#"echo $0 $1 "$2" $4 $# $? ${1}x $10 ${10} $9"#, &v).unwrap(),
            [
                "echo",
                "s.sh",
                "one",
                "two three",
                "four",
                "4",
                "3",
                "onex",
                "one0"
            ]
        );
        assert_eq!(
            words(r#"echo "$A" ${A}! "<$E>" '$A' \$A $UNSET"#, &v).unwrap(),
            ["echo", "a  b", "a  b!", "<>", "$A", "$A"]
        );
    }

    #[test]
    fn a_value_with_blanks_stays_one_word() {
        // bash splits an unquoted `$A` into `a` and `b`, and `$2` into
        // `two` and `three`: here a value is never split.
        let v = script();
        assert_eq!(
            words("echo $A $2", &v).unwrap(),
            ["echo", "a  b", "two three"]
        );
    }

    #[test]
    fn an_unquoted_empty_expansion_is_no_word_a_quoted_one_is() {
        let v = script();
        // As bash's.
        assert_eq!(
            words(r#"echo $E $3 $UNSET "$E" ''$E "$3" x$E"#, &v).unwrap(),
            ["echo", "", "", "", "x"]
        );
        assert_eq!(words("$E $UNSET", &v).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn all_arguments_are_a_word_each() {
        let v = script();
        // `"$@"` as bash's; unquoted, bash would also split `two three`.
        assert_eq!(
            words(r#"echo "$@""#, &v).unwrap(),
            ["echo", "one", "two three", "", "four"]
        );
        assert_eq!(
            words("echo $@", &v).unwrap(),
            ["echo", "one", "two three", "four"]
        );
        assert_eq!(
            words(r#"echo "a$@b" x$@"#, &v).unwrap(),
            [
                "echo",
                "aone",
                "two three",
                "",
                "fourb",
                "xone",
                "two three",
                "four"
            ]
        );
        // None, without arguments; text joined to it stays.
        let none = Vars::of(&[], &["s.sh"]);
        assert_eq!(
            words(r#"echo "$@" $@ "x$@" ''"$@" $#"#, &none).unwrap(),
            ["echo", "x", "", "0"]
        );
        // An empty argument alone, as bash's.
        let empty = Vars::of(&[], &["s.sh", ""]);
        assert_eq!(words(r#"echo $@ "$@""#, &empty).unwrap(), ["echo", ""]);
    }

    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
        let v = script();
        for (line, typed) in [
            ("echo ${1A}", "${1A}"),
            ("echo \"${}\"", "${}"),
            ("echo ${ A}", "${ A}"),
            ("echo ${A B}", "${A B}"),
        ] {
            let e = words(line, &v).unwrap_err();
            assert_eq!(e, Error::BadSubstitution(typed.into()), "{line}");
            assert_eq!(e.to_string(), alloc::format!("{typed}: bad substitution"));
        }
    }

    #[test]
    fn a_redirection_target_must_expand_to_one_word() {
        let v = script();
        let target = |line: &str| {
            let l = parse_line(line).unwrap();
            expand(&l, &v, 0).map(|mut l| l.pipeline.remove(0).redirect.unwrap().path)
        };
        assert_eq!(target("echo > $1.txt").unwrap(), "one.txt");
        assert_eq!(target("echo > $A").unwrap(), "a  b");
        assert_eq!(target(r#"echo > "$E""#).unwrap(), "");
        // bash's message names the target as typed.
        for (line, typed) in [
            ("echo hi > $E", "$E"),
            ("echo hi >> $UNSET$E", "$UNSET$E"),
            ("echo hi > $@", "$@"),
        ] {
            let e = target(line).unwrap_err();
            assert_eq!(e.to_string(), alloc::format!("{typed}: ambiguous redirect"));
        }
    }
}
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    fn expanded(line: &str) -> Result<Line, ParseError> {
        parse_line(line).map(|l| crate::expand::expand(&l))
    }
````

with:

````rust
    fn expanded(line: &str) -> Result<Line, ParseError> {
        parse_line(line).map(|l| crate::expand::plain(&l))
    }
````

Replace:

````rust
    #[test]
    fn dollar_and_backquote_inside_double_quotes_are_unsupported() {
        // Bash expands them there too; passing them on as text would
        // print something bash never prints.
        assert_eq!(
            one(r#"echo "$HOME""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
            one(r#"echo "a $ b""#),
            Err(ParseError::Unsupported("$".into()))
        );
        assert_eq!(
````

with:

````rust
    #[test]
    fn backquotes_are_unsupported_inside_double_quotes_too() {
        // Bash runs the command there too; passing it on as text would
        // print something bash never prints.
        assert_eq!(
````

Replace:

````rust
        assert_eq!(words(r"echo '$HOME `x`'"), ["echo", "$HOME `x`"]);
    }
````

with:

````rust
        assert_eq!(words(r"echo '$HOME `x`'"), ["echo", "$HOME `x`"]);
    }

    /// The pieces of `line`'s words.
    fn pieces(line: &str) -> Vec<Vec<Piece>> {
        parse_line(line).unwrap().pipeline[0]
            .words
            .iter()
            .map(|w| w.pieces.clone())
            .collect()
    }

    #[test]
    fn parameters_are_pieces_of_their_words() {
        let text = |t: &str, quoted| Piece::Text(t.into(), quoted);
        let name = |n: &str, quoted| Piece::Param(Param::Name(n.into()), quoted);
        assert_eq!(
            pieces(r#"$A ${B}x "$C1 $_D" $1$# "$@" $? $10 ${10} ${99999999999999999999}"#),
            [
                vec![name("A", false)],
                vec![name("B", false), text("x", false)],
                vec![name("C1", true), text(" ", true), name("_D", true)],
                vec![
                    Piece::Param(Param::Arg(1), false),
                    Piece::Param(Param::Count, false)
                ],
                vec![Piece::Param(Param::All, true)],
                vec![Piece::Param(Param::Status, false)],
                vec![Piece::Param(Param::Arg(1), false), text("0", false)],
                vec![Piece::Param(Param::Arg(10), false)],
                vec![Piece::Param(Param::Arg(usize::MAX), false)],
            ]
        );
        // `${#}`, `${@}` and `${?}` are `$#`, `$@` and `$?`.
        assert_eq!(
            pieces("${#} ${@} ${?}"),
            [
                [Piece::Param(Param::Count, false)],
                [Piece::Param(Param::All, false)],
                [Piece::Param(Param::Status, false)],
            ]
        );
    }

    #[test]
    fn a_dollar_before_no_parameter_is_a_dollar() {
        // As in bash.
        assert_eq!(
            words(r#"echo $ a$ $% $/x $. "$" "a $ b" "$'" $=1 $:"#),
            [
                "echo", "$", "a$", "$%", "$/x", "$.", "$", "a $ b", "$'", "$=1", "$:"
            ]
        );
        // Escaped or in single quotes it is one anyway.
        assert_eq!(words(r#"echo \$A '$A' "\$A""#), ["echo", "$A", "$A", "$A"]);
    }

    #[test]
    fn bash_s_other_parameters_and_operators_are_unsupported() {
        for (line, what) in [
            ("echo $*", "$*"),
            ("echo \"$$\"", "$$"),
            ("kill $!", "$!"),
            ("echo $-", "$-"),
            ("echo $_", "$_"),
            ("echo $(date)", "$("),
            ("echo \"$((1 + 2))\"", "$(("),
            ("echo $'a'", "$'"),
            ("echo $\"a\"", "$\""),
            ("echo ${A:-x}", "${A:-x}"),
            ("echo ${A-x}", "${A-x}"),
            ("echo ${1:+y}", "${1:+y}"),
            ("echo ${#A}", "${#A}"),
            ("echo ${!A}", "${!A}"),
            ("echo ${A[0]}", "${A[0]}"),
            ("echo ${A%.sh}", "${A%.sh}"),
            ("echo ${*}", "${*}"),
            ("echo ${_}", "${_}"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn a_brace_without_its_end_is_a_syntax_error() {
        for line in ["echo ${A", "echo \"${A\"", "echo ${"] {
            let e = one(line).unwrap_err();
            assert_eq!(e, ParseError::UnclosedBrace, "{line}");
            // bash's words (`bash -c 'echo ${A'`).
            assert_eq!(
                e.to_string(),
                "syntax error: unexpected EOF while looking for matching `}'"
            );
        }
    }

    #[test]
    fn a_word_is_kept_as_typed() {
        let l = parse_line(r#"echo  a"b c"$D  > '$f'x# 2"#).unwrap();
        let c = &l.pipeline[0];
        let typed: Vec<&str> = c.words.iter().map(|w| w.typed.as_str()).collect();
        assert_eq!(typed, ["echo", r#"a"b c"$D"#, "2"]);
        assert_eq!(c.redirect.as_ref().unwrap().path.typed, "'$f'x#");
    }
````

Replace:

````rust
            ("a; b", ';'),
            ("echo $HOME", '$'),
            ("ls *.txt", '*'),
````

with:

````rust
            ("a; b", ';'),
            ("ls *.txt", '*'),
````

Replace:

````rust
        assert_eq!(c.redirect.unwrap().path, "/root/out");
    }
````

with:

````rust
        assert_eq!(c.redirect.unwrap().path, "/root/out");
        // As bash's: a quoted `/` after it, or a parameter, keeps it.
        assert_eq!(
            words(r#"echo ~"/x" ~\/x ~$E ~"""#),
            ["echo", "~/x", "~/x", "~", "~"]
        );
    }
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

with:

````rust
    #[test]
    fn dollar_question_is_the_last_line_s_status() {
        let mut h = Harness::new();
        // What bash prints for each.
        assert_eq!(
            h.lines(&["nope", "echo $?", "echo \"$?\"", "echo 'open", "echo ${?}"]),
            (
                0,
                "relay-sh: nope: command not found\n127\n0\n\
                 relay-sh: syntax error: unterminated quote\n2\n"
                    .into()
            )
        );
        // A pipeline's is its last command's; a blank line keeps it.
        assert_eq!(
            h.lines(&["cat /nope | wc -l", "echo $?", "cat /nope", "", "echo $?"]),
            (
                0,
                "cat: /nope: No such file or directory\n0\n0\n\
                 cat: /nope: No such file or directory\n1\n"
                    .into()
            )
        );
    }

    #[test]
    fn the_shell_has_no_arguments_and_no_variables_set() {
        let mut h = Harness::new();
        // bash's `$0` is its own name; this shell's is `relay-sh` in the
        // in-process runner. bash sets `HOME` and others: no variable is
        // set here.
        assert_eq!(
            h.run(r#"echo $0 $# [$1] [$@] [$HOME] "[$UNSET]""#),
            (0, "relay-sh 0 [] [] [] []\n".into())
        );
        let mut h = spawning();
        h.spawning(r#"t-args $? "$E" $E ${E}x "~/$E" ~/$E"#);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "0", "", "x", "~/", "/root/"]
        );
    }

    #[test]
    fn a_line_that_does_not_expand_runs_nothing() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo hi > ${1A}"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        // bash's: an unquoted target that is no word.
        assert_eq!(
            h.run("echo hi > $NONE"),
            (1, "relay-sh: $NONE: ambiguous redirect\n".into())
        );
        assert_eq!(
            h.run(r#"echo hi > "$NONE""#),
            (1, "relay-sh: : No such file or directory\n".into())
        );
        let mut h = spawning();
        assert_eq!(
            h.spawning("t-args | t-args > $NONE"),
            (1, "relay-sh: $NONE: ambiguous redirect\n".into())
        );
        assert!(h.programs.spawned.is_empty(), "nothing of it started");
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

- [ ] **Step 4: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust

    /// Runs one command line in a spawning shell (`/bin/sh`'s); its exit
````

with:

````rust

    /// Runs `lines` one after another in one shell, as if typed (no
    /// prompt); the last one's status and everything they printed.
    pub fn lines(&mut self, lines: &[&str]) -> (i32, String) {
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let mut shell =
            Shell::new(&mut self.vfs, &mut self.console, &mut self.system).with_input(&mut input);
        let mut status = 0;
        for line in lines {
            status = shell.execute(line);
        }
        (status, self.console.take())
    }

    /// Runs one command line in a spawning shell (`/bin/sh`'s); its exit
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Vars` in this scope ``; `` cannot find type `Error` in this scope ``.

- [ ] **Step 6: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4).

use crate::parser::{Command, Line, Redirect, Word};

/// `line`'s commands with the words they get.
pub(crate) fn expand(line: &Line<Word>) -> Line {
    Line {
        pipeline: line.pipeline.iter().map(command).collect(),
        background: line.background.clone(),
    }
}

fn command(c: &Command<Word>) -> Command {
    Command {
        words: c.words.iter().map(Word::text).collect(),
        redirect: c.redirect.as_ref().map(|r| Redirect {
            path: r.path.text(),
            append: r.append,
        }),
    }
}
#[cfg(test)]
````

with:

````rust
//! Expansion: the words a command gets from the words as typed (user-space
//! gate §9.4). A parameter is replaced by its value, which is never split
//! into words (bash splits an unquoted one): a value with blanks stays one
//! word. A word that holds nothing quoted and expands to nothing is no
//! word; a quoted empty one is an empty word. `$@` gives each argument as
//! a word of its own, the text before it joined to the first and the text
//! after it to the last, as bash's `"$@"` does; an empty argument is kept
//! only in quotes.

use crate::parser::{Command, Line, Param, Piece, Redirect, Word};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// `$0`, then `$1` on.
    args: Vec<String>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            args: alloc::vec![String::from(name)],
        }
    }

    /// The variable `name`'s value; an unset one is empty.
    pub fn get(&self, name: &str) -> &str {
        self.names.get(name).map_or("", String::as_str)
    }
}

/// Why a line could not be expanded; the shell says so, status 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A `${…}` naming no parameter, as typed.
    BadSubstitution(String),
    /// A redirection whose target is not one word, as typed.
    AmbiguousRedirect(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSubstitution(t) => write!(f, "{t}: bad substitution"),
            Error::AmbiguousRedirect(t) => write!(f, "{t}: ambiguous redirect"),
        }
    }
}

/// `line`'s commands with the words they get, `status` being `$?`.
pub(crate) fn expand(line: &Line<Word>, vars: &Vars, status: i32) -> Result<Line, Error> {
    let x = Expander { vars, status };
    let mut pipeline = Vec::new();
    for c in &line.pipeline {
        pipeline.push(x.command(c)?);
    }
    Ok(Line {
        pipeline,
        background: line.background.clone(),
    })
}

struct Expander<'v> {
    vars: &'v Vars,
    status: i32,
}

/// A word being made, and whether anything quoted went into it.
struct Field {
    text: String,
    quoted: bool,
}

impl Expander<'_> {
    fn command(&self, c: &Command<Word>) -> Result<Command, Error> {
        let mut words = Vec::new();
        for w in &c.words {
            words.extend(self.word(w)?);
        }
        let redirect = match &c.redirect {
            Some(r) => {
                let mut fields = self.word(&r.path)?;
                if fields.len() != 1 {
                    return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
                }
                Some(Redirect {
                    path: fields.remove(0),
                    append: r.append,
                })
            }
            None => None,
        };
        Ok(Command { words, redirect })
    }

    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&self, word: &Word) -> Result<Vec<String>, Error> {
        let mut fields = alloc::vec![Field {
            text: String::new(),
            quoted: false,
        }];
        for piece in &word.pieces {
            let last = fields.last_mut().expect("a field");
            match piece {
                Piece::Text(t, quoted) => {
                    last.text.push_str(t);
                    last.quoted |= quoted;
                }
                Piece::Param(Param::All, quoted) => {
                    let Some((first, rest)) = self.vars.args[1..].split_first() else {
                        continue;
                    };
                    last.text.push_str(first);
                    last.quoted |= quoted;
                    fields.extend(rest.iter().map(|a| Field {
                        text: a.clone(),
                        quoted: *quoted,
                    }));
                }
                Piece::Param(p, quoted) => {
                    last.text.push_str(&self.value(p)?);
                    last.quoted |= quoted;
                }
            }
        }
        Ok(fields
            .into_iter()
            .filter(|f| f.quoted || !f.text.is_empty())
            .map(|f| f.text)
            .collect())
    }

    /// A parameter's value (not `$@`'s).
    fn value(&self, p: &Param) -> Result<String, Error> {
        let args = &self.vars.args;
        Ok(match p {
            Param::Name(n) => String::from(self.vars.get(n)),
            Param::Arg(i) => args.get(*i).cloned().unwrap_or_default(),
            Param::Count => (args.len() - 1).to_string(),
            Param::Status => self.status.to_string(),
            Param::All => args[1..].join(" "),
            Param::Bad(t) => return Err(Error::BadSubstitution(t.clone())),
        })
    }
}

/// The words of `line` with nothing set: for callers that run no shell
/// (tests), whose lines expand.
pub(crate) fn plain(line: &Line<Word>) -> Line {
    let vars = Vars::new(crate::shell::NAME);
    expand(line, &vars, 0).unwrap_or_else(|e| panic!("{e}"))
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        Vars {
            names: names
                .iter()
                .map(|(n, v)| (String::from(*n), String::from(*v)))
                .collect(),
            args: args.iter().map(|a| String::from(*a)).collect(),
        }
    }
}

#[cfg(test)]
````

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 17 replacements, top to bottom:

Replace:

````rust
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, and a bare `$` or `` ` `` in it is refused as outside quotes
//! (bash would expand it); outside quotes `\` makes the next character
//! literal. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
````

with:

````rust
//! except that `\"`, `\\`, `\$` and `` \` `` stand for the second
//! character, parameters expand in it, and a bare `` ` `` in it is refused
//! as outside quotes (bash would run it); outside quotes `\` makes the
//! next character literal. Outside single quotes a parameter, `$NAME`,
//! `${NAME}`, `$0`…`$9`, `${N}`, `$#`, `$@` or `$?`, is a piece of its word
//! that expansion replaces (user-space gate §9.4); bash's other
//! parameters and operators are refused, and a `$` before anything else is
//! a `$`. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
````

Replace:

````rust
//! Every other shell feature is refused: an unquoted `;`, `&` before more,
//! `$`, `*`, `?`, `<`, `` ` ``, `(` or `)` is an error naming the
//! character, instead of being passed on as if it were plain text; so are
//! `||`, `&&` and `2>` (another stream).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

````

with:

````rust
//! Every other shell feature is refused: an unquoted `;`, `&` before more,
//! `*`, `?`, `<`, `` ` ``, `(` or `)` is an error naming the
//! character, instead of being passed on as if it were plain text; so are
//! `||`, `&&` and `2>` (another stream).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::iter::Peekable;
use core::str::CharIndices;

````

Replace:

````rust

/// A word as typed: its pieces of text, each quoted (or escaped) or not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub pieces: Vec<Piece>,
}
````

with:

````rust

/// A word as typed: its pieces of text, each quoted (or escaped) or not,
/// and the parameters in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub pieces: Vec<Piece>,
    /// The word as it was typed (`$f`), for messages about it.
    pub typed: String,
}
````

Replace:

````rust
    Text(String, bool),
}
````

with:

````rust
    Text(String, bool),
    /// A parameter, and whether it was in double quotes.
    Param(Param, bool),
}

/// A parameter expansion replaces (user-space gate §9.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Param {
    /// `$NAME` or `${NAME}`: a variable.
    Name(String),
    /// `$N` or `${N}`: argument N, `$0` the script's (or shell's) name.
    Arg(usize),
    /// `$#`: how many arguments there are.
    Count,
    /// `$@`: every argument, each one word.
    All,
    /// `$?`: the last status.
    Status,
    /// A `${…}` that names no parameter (`${1A}`), as typed: bash's `bad
    /// substitution` when it expands.
    Bad(String),
}
````

Replace:

````rust

    /// The word is one unquoted piece of text, all ASCII digits (`2` in
    /// `2>`).
    fn is_digits(&self) -> bool {
        matches!(&self.pieces[..], [Piece::Text(t, false)] if t.bytes().all(|b| b.is_ascii_digit()))
    }

    /// The word's text, its pieces joined.
    pub fn text(&self) -> String {
        self.pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t, _) => t.as_str(),
            })
            .collect()
    }
````

with:

````rust

    /// The word if it is one unquoted piece of text, all ASCII digits (`2`
    /// in `2>`).
    fn digits(&self) -> Option<&str> {
        match &self.pieces[..] {
            [Piece::Text(t, false)] if t.bytes().all(|b| b.is_ascii_digit()) => Some(t),
            _ => None,
        }
    }
````

Replace:

````rust
    UnexpectedEnd,
}
````

with:

````rust
    UnexpectedEnd,
    /// A `${` without its `}`.
    UnclosedBrace,
}
````

Replace:

````rust
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
        }
    }
}

const UNSUPPORTED: &[char] = &[';', '$', '*', '?', '<', '`', '(', ')'];

````

with:

````rust
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
            ParseError::UnclosedBrace => {
                f.write_str("syntax error: unexpected EOF while looking for matching `}'")
            }
        }
    }
}

const UNSUPPORTED: &[char] = &[';', '*', '?', '<', '`', '(', ')'];

/// The characters a line is read from, and where each is.
struct Cursor<'l> {
    line: &'l str,
    chars: Peekable<CharIndices<'l>>,
}

impl<'l> Cursor<'l> {
    fn new(line: &'l str) -> Cursor<'l> {
        Cursor {
            line,
            chars: line.char_indices().peekable(),
        }
    }

    fn next(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| c)
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    /// Takes the next character if it is `c`.
    fn next_if_eq(&mut self, c: char) -> bool {
        self.chars.next_if(|&(_, n)| n == c).is_some()
    }

    /// Where the next character is (the line's length at its end).
    fn pos(&mut self) -> usize {
        self.chars.peek().map_or(self.line.len(), |&(i, _)| i)
    }

    /// What is left of the line.
    fn rest(&mut self) -> &'l str {
        let at = self.pos();
        &self.line[at..]
    }
}

/// A name starts with a letter or `_`.
fn starts_name(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn in_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `s` is a name (`[A-Za-z_][A-Za-z0-9_]*`).
pub fn is_name(s: &str) -> bool {
    s.chars().next().is_some_and(starts_name) && s.chars().all(in_name)
}

/// bash's parameters that are not supported (`$*`, `$$`, `$!`, `$-`).
const OTHER_SPECIALS: &[char] = &['*', '$', '!', '-'];

/// The parameter after a `$` (inside double quotes if `quoted`), or none
/// for a `$` that stands for itself.
fn parameter(cur: &mut Cursor<'_>, quoted: bool) -> Result<Option<Param>, ParseError> {
    let Some(c) = cur.peek() else {
        return Ok(None);
    };
    let param = match c {
        c if starts_name(c) => {
            let mut name = String::new();
            while let Some(c) = cur.peek().filter(|&c| in_name(c)) {
                cur.next();
                name.push(c);
            }
            if name == "_" {
                // bash's last argument of the command before.
                return Err(ParseError::Unsupported("$_".into()));
            }
            return Ok(Some(Param::Name(name)));
        }
        '0'..='9' => Param::Arg(usize::from(c as u8 - b'0')),
        '#' => Param::Count,
        '@' => Param::All,
        '?' => Param::Status,
        '{' => {
            cur.next();
            return braced(cur).map(Some);
        }
        c if OTHER_SPECIALS.contains(&c) => {
            return Err(ParseError::Unsupported(format!("${c}")));
        }
        '(' => {
            cur.next();
            let what = if cur.peek() == Some('(') { "$((" } else { "$(" };
            return Err(ParseError::Unsupported(what.into()));
        }
        // `$'…'` and `$"…"` are bash's quotes of other kinds.
        '\'' | '"' if !quoted => return Err(ParseError::Unsupported(format!("${c}"))),
        _ => return Ok(None),
    };
    cur.next();
    Ok(Some(param))
}

/// The parameter of a `${…}`, its `{` taken: a name, a number or one of
/// `#`, `@`, `?`. bash's operators (`${A:-x}`, `${#A}`) are unsupported,
/// and anything else is a bad substitution.
fn braced(cur: &mut Cursor<'_>) -> Result<Param, ParseError> {
    let mut inside = String::new();
    loop {
        match cur.next() {
            Some('}') => break,
            Some(c) => inside.push(c),
            None => return Err(ParseError::UnclosedBrace),
        }
    }
    let typed = format!("${{{inside}}}");
    let head = match inside.chars().next() {
        Some(c) if starts_name(c) => inside.find(|c| !in_name(c)),
        Some('0'..='9') => inside.find(|c: char| !c.is_ascii_digit()),
        Some(_) => Some(1),
        None => return Ok(Param::Bad(typed)),
    }
    .unwrap_or(inside.len());
    let (name, rest) = inside.split_at(head);
    if !rest.is_empty() {
        let operator =
            rest.starts_with([':', '-', '=', '+', '?', '%', '/', '^', ',', '#', '[', '@']);
        return if operator || name == "#" || name == "!" {
            Err(ParseError::Unsupported(typed))
        } else {
            Ok(Param::Bad(typed))
        };
    }
    Ok(match name {
        "#" => Param::Count,
        "@" => Param::All,
        "?" => Param::Status,
        "_" => return Err(ParseError::Unsupported(typed)),
        n if is_name(n) => Param::Name(n.into()),
        // A number too big for any argument names none.
        n if n.bytes().all(|b| b.is_ascii_digit()) => Param::Arg(n.parse().unwrap_or(usize::MAX)),
        n if n.chars().all(|c| OTHER_SPECIALS.contains(&c)) => {
            return Err(ParseError::Unsupported(typed));
        }
        _ => Param::Bad(typed),
    })
}

````

Replace:

````rust
    tilde: bool,
}

impl Building {
    /// A quote opens: the word exists, and has a quoted piece even if
    /// nothing is quoted (`''`).
    fn open_quote(&mut self) {
        self.started = true;
        if !matches!(self.word.pieces.last(), Some(Piece::Text(_, true))) {
            self.word.pieces.push(Piece::Text(String::new(), true));
````

with:

````rust
    tilde: bool,
    /// Where it starts and ends in the line.
    start: usize,
    end: usize,
    /// How many characters and parameters went into it.
    added: usize,
}

impl Building {
    /// Adds a parameter, in double quotes or not.
    fn param(&mut self, param: Param, quoted: bool) {
        self.started = true;
        self.added += 1;
        self.word.pieces.push(Piece::Param(param, quoted));
    }

    /// A quote opens: the word exists. Returns how much it holds.
    fn open_quote(&mut self) -> usize {
        self.started = true;
        self.added
    }

    /// The quote opened at `before` closes: with nothing in it (`''`), the
    /// word still has a quoted piece, so it is a word even when empty
    /// (`"$@"`, with no arguments, adds nothing and is none).
    fn close_quote(&mut self, before: usize) {
        if self.added == before && !matches!(self.word.pieces.last(), Some(Piece::Text(_, true))) {
            self.word.pieces.push(Piece::Text(String::new(), true));
````

Replace:

````rust
        self.started = true;
        self.word.push(c, true);
    }

    fn finish(self) -> Option<Word> {
        if !self.started {
            return None;
        }
        let mut word = self.word;
        let t = word.text();
        if self.tilde
            && (t == "~" || t.starts_with("~/"))
            && let Some(Piece::Text(first, _)) = word.pieces.first_mut()
        {
            first.replace_range(..1, HOME);
        }
        Some(word)
````

with:

````rust
        self.started = true;
        self.added += 1;
        self.word.push(c, true);
    }

    /// The word, its `~` (alone, or before a `/` in the same unquoted
    /// piece) made `/root`, as bash's is (`~"/x"` and `~$A` keep it).
    fn finish(self, line: &str) -> Option<Word> {
        if !self.started {
            return None;
        }
        let mut word = self.word;
        let alone = word.pieces.len() == 1;
        if self.tilde
            && let Some(Piece::Text(first, false)) = word.pieces.first_mut()
            && ((alone && first == "~") || first.starts_with("~/"))
        {
            first.replace_range(..1, HOME);
        }
        word.typed = String::from(&line[self.start..self.end]);
        Some(word)
````

Replace:

````rust
    /// word.
    fn end_word(&mut self, word: &mut Building) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish() else {
            return Ok(());
````

with:

````rust
    /// word.
    fn end_word(&mut self, word: &mut Building, line: &str) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish(line) else {
            return Ok(());
````

Replace:

````rust
/// The commands of `line`, whether or not it ends with `&`, their words
/// expanded (for callers that run no shell: tests).
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| crate::expand::expand(&l).pipeline)
}
````

with:

````rust
/// The commands of `line`, whether or not it ends with `&`, their words
/// expanded with no variables set (for callers that run no shell: tests,
/// whose lines expand).
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| crate::expand::plain(&l).pipeline)
}
````

Replace:

````rust
    let mut word = Building::default();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => parts.end_word(&mut word)?,
            '>' => {
                // `2>` redirects another stream in a real shell.
                if word.started && word.word.is_digits() {
                    return Err(ParseError::Unsupported(alloc::format!(
                        "{}>",
                        word.word.text()
                    )));
                }
                parts.end_word(&mut word)?;
                if parts.pending.is_some() {
                    return Err(ParseError::MissingTarget(">"));
                }
                let append = chars.next_if_eq(&'>').is_some();
                // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                // `> &` are its syntax errors.
                if !append && chars.peek() == Some(&'&') {
                    return Err(ParseError::Unsupported(">&".into()));
````

with:

````rust
    let mut word = Building::default();
    let mut cur = Cursor::new(line);
    loop {
        let at = cur.pos();
        let Some(c) = cur.next() else {
            break;
        };
        match c {
            ' ' | '\t' => parts.end_word(&mut word, line)?,
            '>' => {
                // `2>` redirects another stream in a real shell.
                if word.started
                    && let Some(digits) = word.word.digits()
                {
                    return Err(ParseError::Unsupported(format!("{digits}>")));
                }
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() {
                    return Err(ParseError::MissingTarget(">"));
                }
                let append = cur.next_if_eq('>');
                // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                // `> &` are its syntax errors.
                if !append && cur.peek() == Some('&') {
                    return Err(ParseError::Unsupported(">&".into()));
````

Replace:

````rust
            '|' => {
                if chars.next_if_eq(&'|').is_some() {
                    return Err(ParseError::Unsupported("||".into()));
                }
                parts.end_word(&mut word)?;
                pipeline.push(parts.take_before_pipe()?);
            }
            '&' => {
                if chars.next_if_eq(&'&').is_some() {
                    return Err(ParseError::Unsupported("&&".into()));
                }
                parts.end_word(&mut word)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
````

with:

````rust
            '|' => {
                if cur.next_if_eq('|') {
                    return Err(ParseError::Unsupported("||".into()));
                }
                parts.end_word(&mut word, line)?;
                pipeline.push(parts.take_before_pipe()?);
            }
            '&' => {
                if cur.next_if_eq('&') {
                    return Err(ParseError::Unsupported("&&".into()));
                }
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
````

Replace:

````rust
                }
                let rest: String = chars.clone().collect();
                let after = rest.trim_start_matches([' ', '\t']);
                if after.starts_with('&') {
````

with:

````rust
                }
                let after = cur.rest().trim_start_matches([' ', '\t']);
                if after.starts_with('&') {
````

Replace:

````rust
                }
                let typed = &line[..line.len() - rest.len() - 1];
                background = Some(String::from(typed.trim_matches([' ', '\t'])));
                break;
            }
            '\'' => {
                word.open_quote();
                loop {
                    match chars.next() {
                        Some('\'') => break,
````

with:

````rust
                }
                background = Some(String::from(line[..at].trim_matches([' ', '\t'])));
                break;
            }
            '\'' => {
                let before = word.open_quote();
                loop {
                    match cur.next() {
                        Some('\'') => break,
````

Replace:

````rust
                }
            }
            '"' => {
                word.open_quote();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.quoted(chars.next().expect("peeked"));
                        }
                        Some(c @ ('$' | '`')) => return Err(ParseError::Unsupported(c.into())),
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '\\' => match chars.next() {
                Some(c) => word.quoted(c),
                None => return Err(ParseError::TrailingBackslash),
            },
````

with:

````rust
                }
                word.close_quote(before);
            }
            '"' => {
                let before = word.open_quote();
                loop {
                    match cur.next() {
                        Some('"') => break,
                        Some('\\') if matches!(cur.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.quoted(cur.next().expect("peeked"));
                        }
                        Some('$') => match parameter(&mut cur, true)? {
                            Some(p) => word.param(p, true),
                            None => word.quoted('$'),
                        },
                        Some('`') => return Err(ParseError::Unsupported('`'.into())),
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
                word.close_quote(before);
            }
            '\\' => match cur.next() {
                Some(c) => word.quoted(c),
                None => return Err(ParseError::TrailingBackslash),
            },
            '$' => match parameter(&mut cur, false)? {
                Some(p) => word.param(p, false),
                None => {
                    word.started = true;
                    word.added += 1;
                    word.word.push('$', false);
                }
            },
````

Replace:

````rust
                word.started = true;
                word.word.push(c, false);
            }
        }
    }
    parts.end_word(&mut word)?;
    if parts.pending.is_some() {
````

with:

````rust
                word.started = true;
                word.added += 1;
                word.word.push(c, false);
            }
        }
        if word.started {
            if word.end == 0 {
                word.start = at;
            }
            word.end = cur.pos();
        }
    }
    parts.end_word(&mut word, line)?;
    if parts.pending.is_some() {
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
use crate::editor::{Feed, LineEditor};
use crate::expand;
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::editor::{Feed, LineEditor};
use crate::expand::{self, Vars};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
    prompting: bool,
}
````

with:

````rust
    prompting: bool,
    /// Its variables and arguments (spec §9.4).
    vars: Vars,
}
````

Replace:

````rust
            prompting: false,
        }
````

with:

````rust
            prompting: false,
            vars: Vars::new(NAME),
        }
````

Replace:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let mut pipeline = match parser::parse_line(line).map(|l| expand::expand(&l)) {
            Ok(parser::Line {
````

with:

````rust
    pub fn execute(&mut self, line: &str) -> i32 {
        let typed = match parser::parse_line(line) {
            Ok(typed) => typed,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
        let mut pipeline = match expand::expand(&typed, &self.vars, self.status) {
            Ok(parser::Line {
````

Replace:

````rust
            Ok(line) => line.pipeline,
            Err(e) => return self.finish(SYNTAX, format!("{NAME}: {e}\n")),
        };
````

with:

````rust
            Ok(line) => line.pipeline,
            Err(e) => return self.finish(1, format!("{NAME}: {e}\n")),
        };
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 240 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 10: Run the `shell`, `sh`, `pipes`, `jobs` scenarios**

Run: `cargo xtask test --e2e-only --scenario shell`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario jobs`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): expand parameters in words

Outside single quotes, `$NAME`, `${NAME}`, `$0`…`$9`, `${N}`, `$#`,
`$@` and `$?` are now pieces of their words, which expansion replaces
with the shell's variables, arguments and last status (user-space gate
§9.4). A value is never split into words, unlike bash; an unquoted
expansion that leaves a word empty removes it, a quoted one leaves an
empty word, and `$@` gives each argument as a word of its own, text
joined to the first and the last, as bash's `"$@"` does.

A `$` before no parameter stays a `$`, as in bash. bash's other
parameters and operators (`$*`, `$$`, `$!`, `$-`, `$_`, `${A:-x}`,
`${#A}`, …) and its other quotes (`$'…'`, `$"…"`) are unsupported
syntax, as `$(` and `$((` are; a `${` without its `}` is bash's syntax
error, and a `${…}` naming no parameter its `bad substitution`. A
redirection whose target is no single word is bash's `ambiguous
redirect`. Both expansion errors are status 1, and nothing of the line
runs.

Each word keeps its text as typed for those messages. A `~` is `/root`
only alone or before a `/` in the same unquoted piece, so `~"/x"` and
`~$A` keep it, as bash's do. No variable is set yet, and `$0` is
`relay-sh`.
EOF
````


### Task 3: A substitution may start with any character

The prototype's review (critical C1) and decision 2: a `${…}` whose first character was neither a name's nor a digit took its first byte as that character, so a character of several bytes (`${é}`, `"${…}"`, as the spec itself writes it) was cut in two and the shell panicked: a script or `X | sh` holding one ended with 101 before the line's trace, the rest of the script unrun. The character is now taken whole, and the line gets bash's `bad substitution`, status 1; the script goes on. The line editor takes only printable ASCII, so only a script or a pipe could reach it. The red run is the shell's tests, which panic at the cut. Mutation check: the first byte taken again fails all three tests.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 2's `braced`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
                 + ls; wc\nrelay-sh: unsupported syntax: ;\n+ echo after\nafter\n"
                    .into()
````

with:

````rust
                 + ls; wc\nrelay-sh: unsupported syntax: ;\n+ echo after\nafter\n"
                    .into()
            )
        );
    }

    #[test]
    fn a_substitution_of_any_character_fails_only_its_line() {
        // The review found it panicking the shell before the line's trace.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            "echo before\necho \"${…}\"\necho after\n".as_bytes(),
        );
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ echo before\nbefore\n+ echo \"${…}\"\nrelay-sh: ${…}: bad substitution\n\
                 + echo after\nafter\n"
                    .into()
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
            ("echo ${A B}", "${A B}"),
        ] {
````

with:

````rust
            ("echo ${A B}", "${A B}"),
            ("echo ${é}", "${é}"),
        ] {
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn a_dollar_before_no_parameter_is_a_dollar() {
````

with:

````rust
    #[test]
    fn a_substitution_starting_with_any_character_is_read_whole() {
        // The review found `${é}` panicking the shell (a multi-byte
        // character cut in two); bash's `bad substitution`.
        let bad = |t: &str| vec![Piece::Param(Param::Bad(t.into()), false)];
        assert_eq!(
            pieces("${é} ${€x} ${ é}"),
            [bad("${é}"), bad("${€x}"), bad("${ é}")]
        );
        assert_eq!(
            pieces("\"${…}\""),
            [vec![Piece::Param(Param::Bad("${…}".into()), true)]]
        );
    }

    #[test]
    fn a_dollar_before_no_parameter_is_a_dollar() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `commands::script::tests::a_substitution_of_any_character_fails_only_its_line`, `expand::tests::a_substitution_that_names_no_parameter_is_bad`.

- [ ] **Step 5: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
        Some('0'..='9') => inside.find(|c: char| !c.is_ascii_digit()),
        Some(_) => Some(1),
        None => return Ok(Param::Bad(typed)),
````

with:

````rust
        Some('0'..='9') => inside.find(|c: char| !c.is_ascii_digit()),
        Some(c) => Some(c.len_utf8()),
        None => return Ok(Param::Bad(typed)),
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 242 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read a substitution that starts with any character

A `${…}` whose first character was neither a name's nor a digit took
its first byte as that character, so one of several bytes (`${é}`,
`"${…}"`) cut the character in two and panicked the shell: a script or
`X | sh` holding one ended with 101 before the line's trace, and the
rest of the script never ran. The character is now taken whole, and
such a line gets bash's `bad substitution`, status 1, like any other
`${…}` that names no parameter; the script goes on.

The prototype's review found it (critical C1); the line editor takes
only printable ASCII, so only a script or a pipe could reach it.
EOF
````


### Task 4: `parser::parse` returns a line that does not expand as an error

The prototype's review (minor M1): `parser::parse`, public, gives a line's words expanded with no variables set, for callers that run no shell; since Task 2 it panicked on a line that parses but does not expand (`echo ${1A}`, `echo > $E`), where it had returned every error as a value. It now returns `ParseError::Expansion` with bash's words for why, and `expand::plain` returns the error. Its only callers are tests. The red run is the shell's tests, which cannot find the new variant. Mutation check: a panic again fails the test.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 2's `expand::plain`.
- Produces: `ParseError::Expansion(String)`; `expand::plain(&Line<Word>) -> Result<Line, Error>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn expanded(line: &str) -> Result<Line, ParseError> {
        parse_line(line).map(|l| crate::expand::plain(&l))
    }
````

with:

````rust
    fn expanded(line: &str) -> Result<Line, ParseError> {
        parse_line(line).and_then(|l| {
            crate::expand::plain(&l).map_err(|e| ParseError::Expansion(e.to_string()))
        })
    }
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
    fn parse_says_why_a_line_does_not_expand() {
        // The review found it panicking, though it is public.
        assert_eq!(
            parse("echo ${1A}"),
            Err(ParseError::Expansion("${1A}: bad substitution".into()))
        );
        assert_eq!(
            parse("echo > $E").unwrap_err().to_string(),
            "$E: ambiguous redirect"
        );
        assert_eq!(parse("echo $E x").unwrap()[0].words, ["echo", "x"]);
    }

````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `map_err` found for struct `parser::Line<W>` in the current scope ``; `` no variant, associated function, or constant named `Expansion` found for enum `parser::ParseError` in the current scope ``.

- [ ] **Step 3: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
/// The words of `line` with nothing set: for callers that run no shell
/// (tests), whose lines expand.
pub(crate) fn plain(line: &Line<Word>) -> Line {
    let vars = Vars::new(crate::shell::NAME);
    expand(line, &vars, 0).unwrap_or_else(|e| panic!("{e}"))
}
````

with:

````rust
/// The words of `line` with nothing set: for callers that run no shell
/// (tests).
pub(crate) fn plain(line: &Line<Word>) -> Result<Line, Error> {
    expand(line, &Vars::new(crate::shell::NAME), 0)
}
````

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
use alloc::string::String;
use alloc::vec::Vec;
````

with:

````rust
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
````

Replace:

````rust
    UnclosedBrace,
}
````

with:

````rust
    UnclosedBrace,
    /// From `parse` only: the line parses, but does not expand (why).
    Expansion(String),
}
````

Replace:

````rust
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
            ParseError::UnclosedBrace => {
````

with:

````rust
            ParseError::UnexpectedEnd => f.write_str("syntax error: unexpected end of file"),
            ParseError::Expansion(why) => f.write_str(why),
            ParseError::UnclosedBrace => {
````

Replace:

````rust
/// The commands of `line`, whether or not it ends with `&`, their words
/// expanded with no variables set (for callers that run no shell: tests,
/// whose lines expand).
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    parse_line(line).map(|l| crate::expand::plain(&l).pipeline)
}
````

with:

````rust
/// The commands of `line`, whether or not it ends with `&`, their words
/// expanded with no variables set (for callers that run no shell: tests);
/// a line that does not expand is `ParseError::Expansion`.
pub fn parse(line: &str) -> Result<Pipeline, ParseError> {
    let typed = parse_line(line)?;
    match crate::expand::plain(&typed) {
        Ok(l) => Ok(l.pipeline),
        Err(e) => Err(ParseError::Expansion(e.to_string())),
    }
}
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 243 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): return a line that does not expand as an error from parse

`parser::parse`, public, gives a line's words expanded with no
variables set, for callers that run no shell. Since expansion came it
panicked on a line that parses but does not expand (`echo ${1A}`,
`echo > $E`, a line of more words than expansion takes), where it had
returned every error as a value. It now returns
`ParseError::Expansion` with bash's words for why.

The prototype's review found it (minor M1); its only callers are tests.
EOF
````


### Task 5: A command that expands to no words runs nothing

Decision 3: once a word can expand to nothing, a whole command can (`$E`), and the runners took a command's name for granted: a pipeline or a background job holding one panicked at `words[0]`. As bash's, such a command runs nothing with status 0, and a redirection after it still makes its file; in a pipeline it reads nothing and gives the next command an end (the spawning runner makes the pipe and closes its ends, the in-process runner gives the next stage nothing); in the background it starts no job, or a job of the commands that did start. A line is blank only as typed (`Line::is_blank`): a script traces and runs a line whose words expand to nothing, and the status a blank line keeps is the last command's. The red run is the shell's tests: `$E` keeps the last status, and `$E | t-args` panics. Mutation checks: the old status kept, an empty stage's fds left open, the in-process runner's empty last stage given status 1 and its empty stage giving the next one the earlier output each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 2's expansion.
- Produces: `parser::Line::is_blank(&self) -> bool`; `shell::builtin_in(&[Command]) -> Option<&str>` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

with:

````rust
    #[test]
    fn a_command_that_expands_to_nothing_runs_nothing() {
        let mut h = Harness::new();
        // bash's: status 0, and a redirection alone still makes its file.
        assert_eq!(h.lines(&["nope", "$E"]).0, 0);
        assert_eq!(h.run("$E ${E} > /tmp/f"), (0, "".into()));
        assert!(h.exists("/tmp/f"));
        // In a pipeline it reads nothing and gives the next one nothing.
        assert_eq!(h.run("$E | wc -c"), (0, "0\n".into()));
        assert_eq!(h.run("echo hi | $E"), (0, "".into()));
        assert_eq!(h.run("echo hi | $E | $E > /tmp/g"), (0, "".into()));
        assert_eq!(h.get("/tmp/g"), b"");
        // A script traces it and runs it.
        h.put("/tmp/s.sh", b"nope\n$E\necho $?\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ nope\nrelay-sh: nope: command not found\n+ $E\n+ echo $?\n0\n".into()
            )
        );
    }

    #[test]
    fn a_program_s_neighbour_that_expands_to_nothing_is_an_end() {
        let mut h = spawning();
        assert_eq!(h.spawning("$E | t-args"), (3, "".into()));
        let s = &h.programs.spawned[0];
        assert_eq!((s.args.len(), s.group), (1, crate::Group::New));
        let (r, w) = h.programs.pipes[0];
        assert_eq!(s.stdin, Some(r), "the pipe it reads");
        assert!(h.programs.closed.contains(&w), "with no writer");
        assert_eq!(h.spawning("t-args | $E > /tmp/o"), (0, "".into()));
        let (path, append, fd) = h.programs.opened.last().unwrap();
        assert_eq!(
            (path.as_str(), *append),
            ("/tmp/o", false),
            "made, as bash's"
        );
        assert!(h.programs.closed.contains(fd));
        // In the background: no job, or one of what started.
        let mut h = with_jobs();
        let out = typed(&mut h, &["$E &", "$E | sleep 1 &", ""]);
        assert!(
            out.starts_with("root@relay:/# $E &\nroot@relay:/# $E | sleep 1 &\n[1] 101\n"),
            "{out}"
        );
        assert!(
            out.contains("[1]+  Done                    $E | sleep 1\n"),
            "{out}"
        );
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `shell::tests::a_command_that_expands_to_nothing_runs_nothing`, `shell::tests::a_program_s_neighbour_that_expands_to_nothing_is_an_end`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    pub background: Option<String>,
}
````

with:

````rust
    pub background: Option<String>,
}

impl<W> Line<W> {
    /// Nothing but blanks or a comment was typed.
    pub fn is_blank(&self) -> bool {
        matches!(&self.pipeline[..], [c] if c.words.is_empty() && c.redirect.is_none())
            && self.background.is_none()
    }
}
````

- [ ] **Step 4: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
    /// Each stage runs to its end before the next starts, its output kept
    /// as the next one's input; a stage that is not found says so and
    /// gives the next one nothing, as bash's does. Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
        if stages.iter().any(|c| c.words[0] == "sh") {
            return in_a_pipeline("sh");
````

with:

````rust
    /// Each stage runs to its end before the next starts, its output kept
    /// as the next one's input; a stage that is not found, or whose words
    /// expanded to nothing, gives the next one nothing, as bash's does (the
    /// first says so). Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
        if stages
            .iter()
            .any(|c| c.words.first().is_some_and(|w| w == "sh"))
        {
            return in_a_pipeline("sh");
````

Replace:

````rust
        for stage in before {
            let (name, args) = (&stage.words[0], &stage.words[1..]);
            let mut out = Collected(Vec::new());
            if let Some(command) = commands::find(name) {
````

with:

````rust
        for stage in before {
            let mut out = Collected(Vec::new());
            let Some((name, args)) = stage.words.split_first() else {
                piped = Some(Bytes::new(out.0));
                continue;
            };
            if let Some(command) = commands::find(name) {
````

Replace:

````rust
        let mut piped = piped.expect("a stage before the last");
        let parts = Parts {
````

with:

````rust
        let mut piped = piped.expect("a stage before the last");
        let Some((name, args)) = last.words.split_first() else {
            return match redirect_to(&mut *vfs, last.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            };
        };
        let parts = Parts {
````

Replace:

````rust
        };
        self.run(
            parts,
            &last.words[0],
            &last.words[1..],
            last.redirect.as_ref(),
        )
    }
````

with:

````rust
        };
        self.run(parts, name, args, last.redirect.as_ref())
    }
````

Replace:

````rust
            };
            let name = &stage.words[0];
            let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
            argv.extend(stage.words[1..].iter().map(|w| w.as_bytes()));
            let group = match (in_script, background, first) {
````

with:

````rust
            };
            let Some((name, args)) = stage.words.split_first() else {
                // Its words expanded to nothing: it runs nothing, and its
                // neighbours see an end.
                for fd in [stdin, stdout].into_iter().flatten() {
                    self.programs.close(fd);
                }
                stdin = next;
                continue;
            };
            let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
            argv.extend(args.iter().map(|w| w.as_bytes()));
            let group = match (in_script, background, first) {
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
        };
        let mut pipeline = match expand::expand(&typed, &self.vars, self.status) {
````

with:

````rust
        };
        if typed.is_blank() {
            return self.status;
        }
        let mut pipeline = match expand::expand(&typed, &self.vars, self.status) {
````

Replace:

````rust
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            return self.status;
        }
````

with:

````rust
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            // Its words expanded to nothing: bash's status 0.
            return self.finish(0, String::new());
        }
````

Replace:

````rust
    fn pipeline(&mut self, stages: &[parser::Command]) -> i32 {
        let builtin = stages
            .iter()
            .find(|c| commands::builtin(&c.words[0]).is_some());
        if let Some(c) = builtin {
            let ran = runner::in_a_pipeline(&c.words[0]);
            return self.finish(ran.status, ran.message);
````

with:

````rust
    fn pipeline(&mut self, stages: &[parser::Command]) -> i32 {
        if let Some(name) = builtin_in(stages) {
            let ran = runner::in_a_pipeline(name);
            return self.finish(ran.status, ran.message);
````

Replace:

````rust
    fn background(&mut self, stages: &[parser::Command], text: &str) -> i32 {
        let builtin = stages
            .iter()
            .find(|c| commands::builtin(&c.words[0]).is_some());
        if let Some(c) = builtin {
            let name = &c.words[0];
            let message = format!("{NAME}: {name}: cannot be used in the background\n");
````

with:

````rust
    fn background(&mut self, stages: &[parser::Command], text: &str) -> i32 {
        if let Some(name) = builtin_in(stages) {
            let message = format!("{NAME}: {name}: cannot be used in the background\n");
````

Replace:

````rust
        for line in text.lines() {
            if matches!(parser::parse_line(line), Ok(l) if l.pipeline.len() == 1 && l.pipeline[0].words.is_empty() && l.pipeline[0].redirect.is_none())
            {
                continue;
````

with:

````rust
        for line in text.lines() {
            // Blank as typed: one whose words expand to nothing is traced
            // and runs.
            if parser::parse_line(line).is_ok_and(|l| l.is_blank()) {
                continue;
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
}

/// The name of the first of `stages` that is one of the shell's own
/// commands (a command whose words expanded to nothing is none).
fn builtin_in(stages: &[parser::Command]) -> Option<&str> {
    stages
        .iter()
        .filter_map(|c| c.words.first())
        .find(|name| commands::builtin(name).is_some())
        .map(String::as_str)
}

````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 245 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): run nothing for a command that expands to no words

A command whose words all expand to nothing (`$E`) runs nothing, with
status 0, as bash's does, and a redirection after it still makes its
file. In a pipeline such a command reads nothing and gives the next
one an end, under both runners; in the background it starts no job, or
a job of the commands that did start; the runners no longer take a
command's name for granted.

A line is blank only as typed: a script traces and runs a line whose
words expand to nothing, and the status a blank line keeps is the last
command's.
EOF
````


### Task 6: `NAME=value` sets a variable

Spec §9.4 and decision 5: a line of only `NAME=value` words sets each variable in turn (`A=1 B=$A`), with status 0 (`Vars::set`, `Shell::assign`). The value expands as a word does but is always one string, `$@` joined by blanks as in bash (`expand::value`); an unquoted `~` at its start or after a `:` is `/root` (`value_tildes`). A word is an assignment only if its name and `=` are unquoted (`Word::assignment`), so `1A=x`, `A-B=x` and `"A"=x` stay command names. A redirection after the assignments makes its file, and the variables are set even when it cannot be made (status 1), as bash does (`expand::redirect`). `A=1 cmd`, which gives bash's command an environment, is unsupported syntax (`A=1 before a command`): programs get none. In a pipeline or the background, where bash's assignment changes nothing in the shell, it is refused as a built-in is (`relay-sh: A=1: cannot be used in a pipeline`, status 1). A variable may name a built-in (`$C`). The red run is the shell's tests, which cannot find `Word::assignment` or `expand::value`. Mutation checks: a `~` taken where no `:` comes before it, a `~` taken before more text, `A=1 cmd` taken, a name that is no name taken, the assignment arm before the background one (`A=1 &` assigns), the redirection never opened and the value joined without blanks each fail a test.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 2's `Vars`, `expand`; plan 1's `runner::redirect_to`.
- Produces: `parser::Word::assignment(&self) -> Option<(&str, Word)>`; `expand::Vars::set(&mut self, name, value: String)`, `expand::value(&Word, &Vars, status) -> Result<String, Error>`, `expand::redirect(&Redirect<Word>, &Vars, status) -> Result<Redirect, Error>`; `Shell::assign` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
````

with:

````rust
    #[test]
    fn an_assignment_s_value_is_one_string() {
        // As bash sets it: `$@` joined by blanks, nothing removed.
        let v = script();
        let value = |line: &str| {
            let l = parse_line(line).unwrap();
            let (_, value) = l.pipeline[0].words[0].assignment().unwrap();
            super::value(&value, &v, 0)
        };
        assert_eq!(value("A=$@").unwrap(), "one two three  four");
        assert_eq!(value(r#"A="<$@>""#).unwrap(), "<one two three  four>");
        assert_eq!(value("A=$E$UNSET").unwrap(), "");
        assert_eq!(value("A=$A.$#").unwrap(), "a  b.4");
        assert_eq!(
            value("A=${1A}").unwrap_err(),
            Error::BadSubstitution("${1A}".into())
        );
    }

    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    }

    #[test]
    fn a_word_is_kept_as_typed() {
````

with:

````rust
    }

    /// `word`'s assignment, the value's pieces joined.
    fn assignment(word: &str) -> Option<(String, String)> {
        let l = parse_line(word).unwrap();
        let (name, value) = l.pipeline[0].words[0].assignment()?;
        let text = value
            .pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t, _) => t.clone(),
                Piece::Param(p, _) => format!("<{p:?}>"),
            })
            .collect();
        Some((name.into(), text))
    }

    #[test]
    fn a_word_with_an_unquoted_name_and_equals_sign_is_an_assignment() {
        for (word, name, value) in [
            ("A=1", "A", "1"),
            ("A=", "A", ""),
            ("_x9=a=b", "_x9", "a=b"),
            (r#"A="a b""#, "A", "a b"),
            ("A=$B", "A", "<Name(\"B\")>"),
            ("A=x$1", "A", "x<Arg(1)>"),
            ("A=''", "A", ""),
        ] {
            assert_eq!(
                assignment(word),
                Some((name.into(), value.into())),
                "{word}"
            );
        }
        // Command names in bash (`1A=x: command not found`).
        for word in [
            "1A=x", "A-B=x", r#""A"=x"#, r"A\=x", "=x", r#"A"="x"#, "$A=x", "a",
        ] {
            assert_eq!(assignment(word), None, "{word}");
        }
    }

    #[test]
    fn a_tilde_in_a_value_is_home_at_its_start_or_after_a_colon() {
        // What bash sets for each.
        for (word, value) in [
            ("A=~/x:~/y:~:a~", "/root/x:/root/y:/root:a~"),
            ("A=~", "/root"),
            ("A=x:~", "x:/root"),
            ("A=~x", "~x"),
            ("A='~'/x", "~/x"),
            (r#"A=~"/x""#, "~/x"),
            ("A=~$B", "~<Name(\"B\")>"),
        ] {
            assert_eq!(assignment(word).unwrap().1, value, "{word}");
        }
    }

    #[test]
    fn an_assignment_before_a_command_is_unsupported() {
        // bash gives the command an environment, which programs have not.
        for (line, what) in [
            ("A=1 echo hi", "A=1 before a command"),
            ("A='a b' B=2 cat f", "A='a b' before a command"),
            ("ls | A=1 wc", "A=1 before a command"),
            ("A=1 echo &", "A=1 before a command"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        // An argument that looks like one is one; assignments alone parse.
        assert_eq!(words("echo A=1"), ["echo", "A=1"]);
        assert!(parse_line("A=1 B=2 > f").is_ok());
    }

    #[test]
    fn a_word_is_kept_as_typed() {
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

with:

````rust
    #[test]
    fn an_assignment_sets_a_variable_for_the_lines_after_it() {
        let mut h = Harness::new();
        // What bash prints for each.
        assert_eq!(
            h.lines(&[
                "A=1 B=$A C=",
                "echo $A $B [$C] \"[$C]\"",
                "nope",
                "D=$? E=\"a  b\" F=~/x",
                "echo $? $D \"$E\" $F",
                "A=${A}2",
                "echo $A",
            ]),
            (
                0,
                "1 1 [] []\nrelay-sh: nope: command not found\n0 127 a  b /root/x\n12\n".into()
            )
        );
        // A value with blanks stays one word (bash would split it).
        let mut h = spawning();
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.execute("A='a b' P=t-args");
        assert_eq!(shell.execute("$P $A"), 3);
        assert_eq!(h.programs.spawned[0].args, ["t-args", "a b"]);
    }

    #[test]
    fn a_variable_may_name_a_built_in() {
        let mut h = Harness::new();
        assert_eq!(
            h.lines(&["C=cd D=/etc", "$C $D", "pwd"]),
            (0, "/etc\n".into())
        );
    }

    #[test]
    fn an_assignment_with_a_redirection_makes_its_file() {
        let mut h = Harness::new();
        // bash's: the variable is set even when the file cannot be made.
        assert_eq!(h.lines(&["A=1 > /tmp/f", "echo $A"]), (0, "1\n".into()));
        assert!(h.exists("/tmp/f"));
        assert_eq!(
            h.lines(&["B=2 > /nope/f", "echo $? $B"]),
            (
                0,
                "relay-sh: /nope/f: No such file or directory\n1 2\n".into()
            )
        );
        assert_eq!(
            h.lines(&["B=3 > $NONE", "echo $? $B"]),
            (0, "relay-sh: $NONE: ambiguous redirect\n1 3\n".into())
        );
    }

    #[test]
    fn an_assignment_cannot_be_in_a_pipeline_or_the_background() {
        // bash runs it in a shell of its own there, so it sets nothing.
        let mut h = spawning();
        for (line, said) in [
            ("A=1 | t-args", "A=1: cannot be used in a pipeline"),
            ("t-args | A='x y'", "A='x y': cannot be used in a pipeline"),
            ("A=1 &", "A=1: cannot be used in the background"),
            (
                "A=1 B=2 | t-args &",
                "A=1: cannot be used in the background",
            ),
        ] {
            assert_eq!(
                h.spawning(line),
                (1, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert!(h.programs.spawned.is_empty(), "nothing of it ran");
        // A word that is no assignment is a command, as in bash.
        assert_eq!(
            h.run("1A=x"),
            (127, "relay-sh: 1A=x: command not found\n".into())
        );
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find function `value` in module `super` ``; `` no method named `assignment` found for struct `Word` in the current scope ``.

- [ ] **Step 5: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        self.names.get(name).map_or("", String::as_str)
    }
}

````

with:

````rust
        self.names.get(name).map_or("", String::as_str)
    }

    /// Sets the variable `name` to `value`.
    pub fn set(&mut self, name: &str, value: String) {
        self.names.insert(String::from(name), value);
    }
}

````

Replace:

````rust

struct Expander<'v> {
````

with:

````rust

/// An assignment's value: one string, however it expands (`$@` joined by
/// blanks, as bash joins it there).
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    let x = Expander { vars, status };
    Ok(x.fields(word)?
        .into_iter()
        .map(|f| f.text)
        .collect::<Vec<_>>()
        .join(" "))
}

/// A redirection's target, which must expand to one word.
pub(crate) fn redirect(r: &Redirect<Word>, vars: &Vars, status: i32) -> Result<Redirect, Error> {
    Expander { vars, status }.redirect(r)
}

struct Expander<'v> {
````

Replace:

````rust
        let redirect = match &c.redirect {
            Some(r) => {
                let mut fields = self.word(&r.path)?;
                if fields.len() != 1 {
                    return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
                }
                Some(Redirect {
                    path: fields.remove(0),
                    append: r.append,
                })
            }
            None => None,
````

with:

````rust
        let redirect = match &c.redirect {
            Some(r) => Some(self.redirect(r)?),
            None => None,
````

Replace:

````rust

    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&self, word: &Word) -> Result<Vec<String>, Error> {
        let mut fields = alloc::vec![Field {
````

with:

````rust

    fn redirect(&self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let mut fields = self.word(&r.path)?;
        if fields.len() != 1 {
            return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
        }
        Ok(Redirect {
            path: fields.remove(0),
            append: r.append,
        })
    }

    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&self, word: &Word) -> Result<Vec<String>, Error> {
        Ok(self
            .fields(word)?
            .into_iter()
            .filter(|f| f.quoted || !f.text.is_empty())
            .map(|f| f.text)
            .collect())
    }

    /// What `word` expands to, before an unquoted empty word is removed.
    fn fields(&self, word: &Word) -> Result<Vec<Field>, Error> {
        let mut fields = alloc::vec![Field {
````

Replace:

````rust
        }
        Ok(fields
            .into_iter()
            .filter(|f| f.quoted || !f.text.is_empty())
            .map(|f| f.text)
            .collect())
    }
````

with:

````rust
        }
        Ok(fields)
    }
````

- [ ] **Step 6: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

````

with:

````rust
            _ => self.pieces.push(Piece::Text(String::from(c), quoted)),
        }
    }

    /// The word as an assignment, `NAME=value`, if its name and `=` are
    /// unquoted (`"A"=x` is none, as in bash): the name, and the value as a
    /// word of its own, a `~` at its start or after a `:` made `/root`, as
    /// bash's is.
    pub fn assignment(&self) -> Option<(&str, Word)> {
        let Some(Piece::Text(first, false)) = self.pieces.first() else {
            return None;
        };
        let (name, rest) = first.split_once('=')?;
        if !is_name(name) {
            return None;
        }
        let mut pieces = Vec::new();
        if !rest.is_empty() {
            pieces.push(Piece::Text(rest.into(), false));
        }
        pieces.extend(self.pieces[1..].iter().cloned());
        let count = pieces.len();
        for (i, piece) in pieces.iter_mut().enumerate() {
            if let Piece::Text(text, false) = piece {
                *text = value_tildes(text, i == 0, i + 1 == count);
            }
        }
        let typed = String::from(self.typed.get(name.len() + 1..).unwrap_or(""));
        Some((name, Word { pieces, typed }))
    }

````

Replace:

````rust

/// A word being built.
````

with:

````rust

/// An unquoted piece of an assignment's value with each `~` made `/root`
/// that is at the value's start (`first`) or after a `:`, and before a `/`,
/// a `:` or the value's end (`last`).
fn value_tildes(text: &str, first: bool, last: bool) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut after_colon = first;
    while let Some(c) = chars.next() {
        let ends = match chars.peek() {
            Some(&n) => n == '/' || n == ':',
            None => last,
        };
        if c == '~' && after_colon && ends {
            out.push_str(HOME);
        } else {
            out.push(c);
        }
        after_colon = c == ':';
    }
    out
}

/// A command of `words` and `redirect`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4).
fn command(
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
    if let Some(first) = words.first()
        && first.assignment().is_some()
        && words.iter().any(|w| w.assignment().is_none())
    {
        return Err(ParseError::Unsupported(format!(
            "{} before a command",
            first.typed
        )));
    }
    Ok(Command { words, redirect })
}

/// A word being built.
````

Replace:

````rust
        let p = core::mem::take(self);
        Ok(Command {
            words: p.words,
            redirect: None,
        })
    }
````

with:

````rust
        let p = core::mem::take(self);
        command(p.words, None)
    }
````

Replace:

````rust
    }
    pipeline.push(Command {
        words: parts.words,
        redirect: parts.redirect,
    });
    Ok(Line {
````

with:

````rust
    }
    pipeline.push(command(parts.words, parts.redirect)?);
    Ok(Line {
````

- [ ] **Step 7: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        }
        let mut pipeline = match expand::expand(&typed, &self.vars, self.status) {
````

with:

````rust
        }
        let assigns = typed
            .pipeline
            .iter()
            .find_map(|c| c.words.first().filter(|w| w.assignment().is_some()));
        if let Some(first) = assigns {
            // Alone on its line; bash's changes nothing elsewhere.
            let place = match (&typed.background, typed.pipeline.len()) {
                (Some(_), _) => "the background",
                (None, 1) => return self.assign(&typed.pipeline[0]),
                (None, _) => "a pipeline",
            };
            let message = format!("{NAME}: {}: cannot be used in {place}\n", first.typed);
            return self.finish(1, message);
        }
        let mut pipeline = match expand::expand(&typed, &self.vars, self.status) {
````

Replace:

````rust
        self.finish(status, ran.message)
    }
````

with:

````rust
        self.finish(status, ran.message)
    }

    /// A line of assignments (spec §9.4): each sets its variable in turn,
    /// so a later one reads an earlier one, and the status is 0. A
    /// redirection after them makes its file, as bash's does.
    fn assign(&mut self, cmd: &parser::Command<parser::Word>) -> i32 {
        for (name, value) in cmd.words.iter().filter_map(parser::Word::assignment) {
            match expand::value(&value, &self.vars, self.status) {
                Ok(v) => self.vars.set(name, v),
                Err(e) => return self.finish(1, format!("{NAME}: {e}\n")),
            }
        }
        let redirect = match cmd.redirect.as_ref() {
            Some(r) => match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => Some(r),
                Err(e) => return self.finish(1, format!("{NAME}: {e}\n")),
            },
            None => None,
        };
        match runner::redirect_to(&mut *self.vfs, redirect.as_ref()) {
            Ok(_) => self.finish(0, String::new()),
            Err(ran) => self.finish(ran.status, ran.message),
        }
    }
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 253 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): set variables with NAME=value

A line of `NAME=value` words sets each variable in turn, so a later
value reads an earlier one, with status 0 (user-space gate §9.4). The
value expands as a word does but is always one string, `$@` joined by
blanks as in bash; an unquoted `~` at its start or after a `:` is
`/root`. A word is an assignment only if its name and `=` are
unquoted, so `1A=x`, `A-B=x` and `"A"=x` stay command names, as in
bash. A redirection after the assignments makes its file, and the
variables are set even when it cannot be made, as bash does.

An assignment before a command, which gives bash's command an
environment, is unsupported syntax: programs get none. In a pipeline
or the background, where bash's assignment changes nothing in the
shell, it is refused as a built-in is (`relay-sh: A=1: cannot be used
in a pipeline`, status 1). A variable may name a built-in (`$C`).
EOF
````


### Task 7: `NAME+=value` is unsupported syntax

The prototype's review (minor M2) and decision 5: bash's `A+=x` appends to a variable. Here the word was no assignment (`A+` is no name), so the shell looked for a command named `A+=2` (127), and a script's `PATH+=:/x` failed so. The user chose to refuse it as bash's other forms are: `unsupported syntax: A+=2` (status 2), alone, among assignments or before a command (`Word::appends`). The red run is the shell's tests. Mutation checks: any `X+=` taken as one, none of them refused and only appends counted among the leading words each fail the test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 6's `Word::assignment` and `command`.
- Produces: `Word::appends` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn a_word_is_kept_as_typed() {
````

with:

````rust
    #[test]
    fn appending_to_a_variable_is_unsupported() {
        // bash's `A+=x` appends; here it is refused rather than run as a
        // command (the review found `A+=2: command not found`).
        for (line, what) in [
            ("A+=2", "A+=2"),
            ("PATH+=:/x", "PATH+=:/x"),
            ("A=1 B+=\"x y\"", "B+=\"x y\""),
            ("A+=1 echo hi", "A+=1"),
            ("ls | A+=1", "A+=1"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
        // An argument, or a word that is no name's, is not one.
        assert_eq!(words("echo A+=1"), ["echo", "A+=1"]);
        assert!(parse_line("1+=x").is_ok() && parse_line("A\\+=x").is_ok());
    }

    #[test]
    fn a_word_is_kept_as_typed() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::appending_to_a_variable_is_unsupported`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        Some((name, Word { pieces, typed }))
    }
````

with:

````rust
        Some((name, Word { pieces, typed }))
    }

    /// The word appends to a variable, `NAME+=value`, as bash's does.
    fn appends(&self) -> bool {
        let Some(Piece::Text(first, false)) = self.pieces.first() else {
            return false;
        };
        first
            .split_once('=')
            .and_then(|(before, _)| before.strip_suffix('+'))
            .is_some_and(is_name)
    }
````

Replace:

````rust
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4).
fn command(
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
    if let Some(first) = words.first()
````

with:

````rust
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
    let mut leading = words
        .iter()
        .take_while(|w| w.assignment().is_some() || w.appends());
    if let Some(append) = leading.find(|w| w.appends()) {
        return Err(ParseError::Unsupported(append.typed.clone()));
    }
    if let Some(first) = words.first()
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 254 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse NAME+=value as unsupported syntax

bash's `A+=x` appends to a variable. Here the word was no assignment
(`A+` is no name), so the shell looked for a command named `A+=2` and
said it was not found (127), and a script's `PATH+=:/x` failed so. It
is now refused as bash's other forms are, `unsupported syntax: A+=2`
(status 2), alone, among assignments or before a command.

The prototype's review found it (minor M2); the user chose the
refusal.
EOF
````


### Task 8: Variables and expansion hold at most 64 KiB each

Decision 9: `/bin/sh`'s heap ends the program with 134 when it runs out, so nothing a person types may grow it without bound, and assignments, or `"$@"` repeated over many arguments, could. A shell's variables hold at most 64 KiB, names and values together (`VARS_MAX`; `Vars::set` returns `Error::Full`), and a line expands to at most 64 KiB, counting one for each word as well as their bytes (`EXPANSION_MAX`, as `spawn` takes at most 64 KiB of arguments; `Error::TooLong`). The room is taken before each piece is made. Beyond either the line stops there with status 1: `relay-sh: B: the variables would hold more than 64 KiB`, `relay-sh: the line would expand to more than 64 KiB`. The red run is the shell's tests, which cannot find the limits. Mutation checks: a word's own count removed (it survived at first: no test had more words than bytes; a line of 65 537 `''` words now kills it), `$@`'s count for each argument removed, the bytes not counted, `Vars::set` never full and a replaced value's room not given back each fail a test.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 6's `Vars::set`.
- Produces: `expand::{EXPANSION_MAX, VARS_MAX}` (64 KiB); `Vars::set(&mut self, name, value: String) -> Result<(), Error>`; `expand::Error::{TooLong, Full(String)}`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
````

with:

````rust
    #[test]
    fn the_variables_hold_at_most_64_kib() {
        let mut v = Vars::new("sh");
        let big = "x".repeat(VARS_MAX - 1);
        v.set("A", big.clone()).unwrap();
        assert_eq!(v.set("B", String::new()), Err(Error::Full("B".into())));
        assert_eq!(v.set("A", big.clone() + "y"), Err(Error::Full("A".into())));
        assert_eq!(v.get("A"), big, "unchanged");
        // A smaller value makes room again.
        v.set("A", String::from("1")).unwrap();
        v.set("B", "x".repeat(VARS_MAX - 3)).unwrap();
        assert_eq!(
            Error::Full("B".into()).to_string(),
            "B: the variables would hold more than 64 KiB"
        );
    }

    #[test]
    fn a_line_expands_to_at_most_64_kib() {
        let half = "x".repeat(EXPANSION_MAX / 2);
        let v = Vars::of(&[("A", &half)], &["s.sh"]);
        assert_eq!(words("echo $A", &v).unwrap()[1].len(), half.len());
        assert_eq!(words("echo $A $A", &v), Err(Error::TooLong));
        assert_eq!(words("echo \"$A$A\"", &v), Err(Error::TooLong));
        // Each word counts too: many empty arguments, many times.
        let mut args = alloc::vec!["s.sh"];
        args.extend(core::iter::repeat_n("", 20_000));
        let v = Vars::of(&[], &args);
        assert_eq!(words(r#"echo "$@""#, &v).unwrap().len(), 20_001);
        assert_eq!(
            words(r#"echo "$@" "$@" "$@" "$@""#, &v),
            Err(Error::TooLong)
        );
        // And so does each word of the line itself.
        let empties = "'' ".repeat(EXPANSION_MAX);
        assert_eq!(words(&empties, &v).unwrap().len(), EXPANSION_MAX);
        assert_eq!(words(&(empties + "''"), &v), Err(Error::TooLong));
        assert_eq!(
            Error::TooLong.to_string(),
            "the line would expand to more than 64 KiB"
        );
    }

    #[test]
    fn a_substitution_that_names_no_parameter_is_bad() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

with:

````rust
    #[test]
    fn a_line_beyond_the_limits_runs_nothing() {
        let mut h = spawning();
        let big = "x".repeat(40_000);
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        assert_eq!(shell.execute(&alloc::format!("A={big}")), 0);
        assert_eq!(shell.execute(&alloc::format!("B={big} C=1")), 1);
        assert_eq!(shell.execute("t-args $A $A"), 1);
        assert_eq!(
            h.console.take(),
            "relay-sh: B: the variables would hold more than 64 KiB\n\
             relay-sh: the line would expand to more than 64 KiB\n"
        );
        assert!(h.programs.spawned.is_empty(), "nothing started");
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.execute(&alloc::format!("A={big}"));
        shell.execute(&alloc::format!("B={big} C=1"));
        shell.execute(r#"t-args "[$B$C]""#);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "[]"],
            "neither B nor what came after it"
        );
    }

    #[test]
    fn a_blank_line_keeps_the_last_status() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `VARS_MAX` in this scope ``; `` cannot find value `EXPANSION_MAX` in this scope ``.

- [ ] **Step 4: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 16 replacements, top to bottom:

Replace:

````rust

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// `$0`, then `$1` on.
````

with:

````rust

/// The most a line expands to, its words' bytes and one for each word, as
/// `spawn` takes at most 64 KiB of arguments (spec §11.1): so nothing a
/// person types grows the shell's heap without bound.
pub const EXPANSION_MAX: usize = 64 * 1024;

/// The most a shell's variables hold, their names' and values' bytes.
pub const VARS_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, String>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
````

Replace:

````rust
            names: BTreeMap::new(),
            args: alloc::vec![String::from(name)],
````

with:

````rust
            names: BTreeMap::new(),
            size: 0,
            args: alloc::vec![String::from(name)],
````

Replace:

````rust

    /// Sets the variable `name` to `value`.
    pub fn set(&mut self, name: &str, value: String) {
        self.names.insert(String::from(name), value);
    }
````

with:

````rust

    /// Sets the variable `name` to `value`, unless the variables would
    /// then hold more than `VARS_MAX`.
    pub fn set(&mut self, name: &str, value: String) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| name.len() + v.len());
        let size = self.size - old + name.len() + value.len();
        if size > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = size;
        self.names.insert(String::from(name), value);
        Ok(())
    }
````

Replace:

````rust
    AmbiguousRedirect(String),
}
````

with:

````rust
    AmbiguousRedirect(String),
    /// The line would expand to more than `EXPANSION_MAX`.
    TooLong,
    /// The variable would make the variables hold more than `VARS_MAX`.
    Full(String),
}
````

Replace:

````rust
            Error::AmbiguousRedirect(t) => write!(f, "{t}: ambiguous redirect"),
        }
````

with:

````rust
            Error::AmbiguousRedirect(t) => write!(f, "{t}: ambiguous redirect"),
            Error::TooLong => f.write_str("the line would expand to more than 64 KiB"),
            Error::Full(n) => write!(f, "{n}: the variables would hold more than 64 KiB"),
        }
````

Replace:

````rust
pub(crate) fn expand(line: &Line<Word>, vars: &Vars, status: i32) -> Result<Line, Error> {
    let x = Expander { vars, status };
    let mut pipeline = Vec::new();
````

with:

````rust
pub(crate) fn expand(line: &Line<Word>, vars: &Vars, status: i32) -> Result<Line, Error> {
    let mut x = Expander::new(vars, status);
    let mut pipeline = Vec::new();
````

Replace:

````rust
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    let x = Expander { vars, status };
    Ok(x.fields(word)?
        .into_iter()
````

with:

````rust
pub(crate) fn value(word: &Word, vars: &Vars, status: i32) -> Result<String, Error> {
    Ok(Expander::new(vars, status)
        .fields(word)?
        .into_iter()
````

Replace:

````rust
pub(crate) fn redirect(r: &Redirect<Word>, vars: &Vars, status: i32) -> Result<Redirect, Error> {
    Expander { vars, status }.redirect(r)
}
````

with:

````rust
pub(crate) fn redirect(r: &Redirect<Word>, vars: &Vars, status: i32) -> Result<Redirect, Error> {
    Expander::new(vars, status).redirect(r)
}
````

Replace:

````rust
    status: i32,
}
````

with:

````rust
    status: i32,
    /// What may still be made: bytes, and one for each word.
    room: usize,
}
````

Replace:

````rust

impl Expander<'_> {
    fn command(&self, c: &Command<Word>) -> Result<Command, Error> {
        let mut words = Vec::new();
````

with:

````rust

impl<'v> Expander<'v> {
    fn new(vars: &'v Vars, status: i32) -> Expander<'v> {
        Expander {
            vars,
            status,
            room: EXPANSION_MAX,
        }
    }

    /// Takes `n` from the room left, before what it is for is made.
    fn take(&mut self, n: usize) -> Result<(), Error> {
        self.room = self.room.checked_sub(n).ok_or(Error::TooLong)?;
        Ok(())
    }

    fn command(&mut self, c: &Command<Word>) -> Result<Command, Error> {
        let mut words = Vec::new();
````

Replace:

````rust

    fn redirect(&self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let mut fields = self.word(&r.path)?;
````

with:

````rust

    fn redirect(&mut self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let mut fields = self.word(&r.path)?;
````

Replace:

````rust
    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&self, word: &Word) -> Result<Vec<String>, Error> {
        Ok(self
````

with:

````rust
    /// The words `word` gives: one, none, or one an argument for `$@`.
    fn word(&mut self, word: &Word) -> Result<Vec<String>, Error> {
        Ok(self
````

Replace:

````rust
    /// What `word` expands to, before an unquoted empty word is removed.
    fn fields(&self, word: &Word) -> Result<Vec<Field>, Error> {
        let mut fields = alloc::vec![Field {
````

with:

````rust
    /// What `word` expands to, before an unquoted empty word is removed.
    fn fields(&mut self, word: &Word) -> Result<Vec<Field>, Error> {
        self.take(1)?;
        let mut fields = alloc::vec![Field {
````

Replace:

````rust
        for piece in &word.pieces {
            let last = fields.last_mut().expect("a field");
            match piece {
                Piece::Text(t, quoted) => {
                    last.text.push_str(t);
                    last.quoted |= quoted;
                }
                Piece::Param(Param::All, quoted) => {
                    let Some((first, rest)) = self.vars.args[1..].split_first() else {
                        continue;
                    };
                    last.text.push_str(first);
                    last.quoted |= quoted;
                    fields.extend(rest.iter().map(|a| Field {
                        text: a.clone(),
                        quoted: *quoted,
                    }));
                }
                Piece::Param(p, quoted) => {
                    last.text.push_str(&self.value(p)?);
                    last.quoted |= quoted;
                }
````

with:

````rust
        for piece in &word.pieces {
            match piece {
                Piece::Param(Param::All, quoted) => {
                    let Some((first, rest)) = self.vars.args[1..].split_first() else {
                        continue;
                    };
                    self.push(&mut fields, first, *quoted)?;
                    for a in rest {
                        self.take(1)?;
                        fields.push(Field {
                            text: String::new(),
                            quoted: false,
                        });
                        self.push(&mut fields, a, *quoted)?;
                    }
                }
                Piece::Text(t, quoted) => self.push(&mut fields, t, *quoted)?,
                Piece::Param(p, quoted) => {
                    let value = self.value(p)?;
                    self.push(&mut fields, &value, *quoted)?;
                }
````

Replace:

````rust

    /// A parameter's value (not `$@`'s).
    fn value(&self, p: &Param) -> Result<String, Error> {
````

with:

````rust

    /// Adds `text` to the last of `fields`.
    fn push(&mut self, fields: &mut [Field], text: &str, quoted: bool) -> Result<(), Error> {
        self.take(text.len())?;
        let last = fields.last_mut().expect("a field");
        last.text.push_str(text);
        last.quoted |= quoted;
        Ok(())
    }

    /// A parameter's value (`$@`'s joined by blanks).
    fn value(&self, p: &Param) -> Result<String, Error> {
````

Replace:

````rust
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        Vars {
            names: names
                .iter()
                .map(|(n, v)| (String::from(*n), String::from(*v)))
                .collect(),
            args: args.iter().map(|a| String::from(*a)).collect(),
        }
    }
````

with:

````rust
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        let mut vars = Vars::new("");
        for (n, v) in names {
            vars.set(n, String::from(*v)).unwrap();
        }
        vars.args = args.iter().map(|a| String::from(*a)).collect();
        vars
    }
````

- [ ] **Step 5: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        for (name, value) in cmd.words.iter().filter_map(parser::Word::assignment) {
            match expand::value(&value, &self.vars, self.status) {
                Ok(v) => self.vars.set(name, v),
                Err(e) => return self.finish(1, format!("{NAME}: {e}\n")),
            }
````

with:

````rust
        for (name, value) in cmd.words.iter().filter_map(parser::Word::assignment) {
            let set =
                expand::value(&value, &self.vars, self.status).and_then(|v| self.vars.set(name, v));
            if let Err(e) = set {
                return self.finish(1, format!("{NAME}: {e}\n"));
            }
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 257 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): bound variables and expansion at 64 KiB each

A shell's variables hold at most 64 KiB, names and values together,
and a line expands to at most 64 KiB, counting one for each word as
well as their bytes, as `spawn` takes at most 64 KiB of arguments
(user-space gate §11.1). Beyond either the line is refused with status
1 and runs no further: `relay-sh: B: the variables would hold more
than 64 KiB`, `relay-sh: the line would expand to more than 64 KiB`.

`/bin/sh` ends with 134 when its heap runs out, so nothing a person
types may grow it without bound: before this, assignments and `"$@"`
repeated over many arguments could. The room is taken before each
piece is made.
EOF
````


### Task 9: `|&` is unsupported syntax

Plan 2's deferred minor and decision 11: bash's `|&` sends a command's errors into the pipe as well as its output. Since plan 2 the parser read its `&` as one ending a line and called it bash's syntax error at `&`; it is now `unsupported syntax: |&`, which names what is missing. `| &`, with a blank between, stays bash's syntax error. The red run is the shell's tests. Mutation check: the `|&` check removed fails the test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: plan 2's parser.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
        assert_eq!(parse("a || b"), Err(ParseError::Unsupported("||".into())));
        // Only the last command redirects (spec §9.1): bash would send the
````

with:

````rust
        assert_eq!(parse("a || b"), Err(ParseError::Unsupported("||".into())));
        // bash's `|&` sends the errors into the pipe too; `| &` is its
        // syntax error.
        assert_eq!(
            parse("a |& b").unwrap_err().to_string(),
            "unsupported syntax: |&"
        );
        assert_eq!(parse("a|&b"), Err(ParseError::Unsupported("|&".into())));
        // Only the last command redirects (spec §9.1): bash would send the
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::a_bar_needs_a_command_on_each_side`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
                    return Err(ParseError::Unsupported("||".into()));
                }
````

with:

````rust
                    return Err(ParseError::Unsupported("||".into()));
                }
                // bash's `|&` pipes the errors too.
                if cur.next_if_eq('&') {
                    return Err(ParseError::Unsupported("|&".into()));
                }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 257 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): refuse |& as unsupported syntax

bash's `|&` sends a command's errors into the pipe as well as its
output. Since background jobs came, the parser read its `&` as one
that ends a line and called it bash's syntax error at `&`; before
them it was `unsupported syntax: &`. It is now `unsupported syntax:
|&`, which names what is missing. `| &`, with a blank between, stays
bash's syntax error.
EOF
````


### Task 10: The parser's comment describes parameters and assignments

The prototype's review (minor M4): the parser's module comment said that a `$` before anything but a parameter is a `$`, while `$'` and `$"` are refused and a `${` without its `}` is a syntax error; it described neither assignments nor a value's `~`, and its list of refusals lacked `|&` and `>&`. It now says what the parser does with each. The task changes a comment only, so it has no failing run.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Tasks 1–9's parser.
- Produces: nothing new.

- [ ] **Step 1: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
//! as outside quotes (bash would run it); outside quotes `\` makes the
//! next character literal. Outside single quotes a parameter, `$NAME`,
//! `${NAME}`, `$0`…`$9`, `${N}`, `$#`, `$@` or `$?`, is a piece of its word
//! that expansion replaces (user-space gate §9.4); bash's other
//! parameters and operators are refused, and a `$` before anything else is
//! a `$`. `> file` and `>> file` redirect standard output (at most one
//! per command). An unquoted `~` alone or before `/` at the start of a word
//! means `/root`, as in Linux. An unquoted `#` at the start of a word
//! begins a comment, which runs to the end of the line. An unquoted `|`
//! joins commands into a pipeline (user-space gate §9.1); each has a name,
//! only the last may redirect its output, and bash's syntax errors name a
//! `|` with no command before it or none after. An unquoted `&` at the end
//! of the line (a comment may follow) runs it in the background (§9.2).
//! Every other shell feature is refused: an unquoted `;`, `&` before more,
//! `*`, `?`, `<`, `` ` ``, `(` or `)` is an error naming the
//! character, instead of being passed on as if it were plain text; so are
//! `||`, `&&` and `2>` (another stream).

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
````

with:

````rust
//! as outside quotes (bash would run it); outside quotes `\` makes the
//! next character literal. Each word is given as typed: its pieces, quoted
//! or not, for expansion (`crate::expand`), and its text for messages.
//!
//! Outside single quotes a parameter, `$NAME`, `${NAME}`, `$0`…`$9`,
//! `${N}`, `$#`, `$@` or `$?`, is a piece of its word that expansion
//! replaces (user-space gate §9.4). bash's other parameters (`$*`, `$$`,
//! `$!`, `$-`, `$_`), its operators (`${A:-x}`, `${#A}`) and, outside
//! double quotes, its quotes `$'…'` and `$"…"` are refused, as are `$(`
//! and `$((`; a `${` without its `}` is bash's syntax error, and a `${…}`
//! that names nothing expands to its `bad substitution`. A `$` before
//! anything else is a `$`.
//!
//! A word whose unquoted start is a name and `=` is an assignment
//! (`Word::assignment`), its value's `~` at its start or after a `:` made
//! `/root`, as bash's is; an assignment before a command, which would give
//! bash's command an environment, and bash's `NAME+=value` are refused.
//!
//! `> file` and `>> file` redirect standard output (at most one per
//! command). An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
//! `#` at the start of a word begins a comment, which runs to the end of
//! the line. An unquoted `|` joins commands into a pipeline (user-space
//! gate §9.1); each has a name, only the last may redirect its output, and
//! bash's syntax errors name a `|` with no command before it or none after.
//! An unquoted `&` at the end of the line (a comment may follow) runs it in
//! the background (§9.2). Every other shell feature is refused: an unquoted
//! `;`, `&` before more, `*`, `?`, `<`, `` ` ``, `(` or `)` is an error
//! naming the character, instead of being passed on as if it were plain
//! text; so are `||`, `&&`, `|&` (the errors into the pipe too), `>&` and
//! `2>` (another stream).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 257 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
docs(shell): describe the parser's parameters and assignments

The parser's module comment said that a `$` before anything but a
parameter is a `$`, while `$'` and `$"` are refused and a `${` without
its `}` is a syntax error; it described neither assignments nor the
`~` of a value, and its list of refusals lacked `|&` and `>&`. It now
says what the parser does with each.

The prototype's review found it (minor M4).
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 46 scenario(s) passed`.

````bash
git push -u origin m3p3/shell
gh pr create --base main --head m3p3/shell --title "feat(shell): expand parameters and set variables" --body-file - <<'EOF'
## What

Milestone 3, plan 3, tasks 1–10: outside single quotes `$NAME`, `${NAME}`, `$0`…`$9`, `${N}`, `$#`, `$@` and `$?` expand from the shell's variables, arguments and last status, never split into words (unlike bash), an unquoted empty expansion being no word and a quoted one an empty word; bash's other parameters, operators and quotes are unsupported syntax, and its `bad substitution`, `ambiguous redirect` and syntax error at a missing `}` are kept; a command that expands to nothing runs nothing, in a pipeline too; a line of `NAME=value` sets variables, `A=1 cmd` and `A+=x` are unsupported and an assignment in a pipeline or the background is refused; variables and a line's expansion hold at most 64 KiB each; `|&` is unsupported syntax again.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (the shell's parsing and expansion, run the same in QEMU); no check script holds a `$`, and plan 4's NUC check 5 runs script arguments and variables there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p3/shell --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-shell
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Script arguments, `$0` and the scenario `script_vars` (Tasks 11–14)

Scripts get their arguments under both runners, each script and shell its own variables; `$0` is the shell's argument 0; `X | sh` reads a byte at a time (plan 1's deferred minor); and the scenario `script_vars` runs it all in QEMU.

Branch `m3p3/scripts`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-scripts`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m3p3/scripts /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-scripts origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-scripts
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m3p3/shell` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m3p3/scripts /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-scripts m3p3/shell`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m3p3/shell>` and re-run `cargo xtask ci` before pushing.

### Task 11: A script gets its arguments

Spec §8.3, §9.4 and decisions 7, 8: `sh FILE [ARG]...` gives the script its arguments as `$1` on, `$#` and `$@`, and `$0` the file as `sh` was given it, under both runners: `/bin/sh` already passes a script's arguments to the child shell, which now takes them instead of saying `sh: extra operand` (`Script::{name, args}`, `Vars::script`). Options come only before the file, as bash's do, so what follows it is the script's (`sh f -x`), and `--` lets a file's name start with `-`. A script starts with no variables and `$?` 0, nothing it sets reaches the shell that ran it, and that shell's `$?` afterwards is the script's status; the in-process runner, which runs a script in its own shell, keeps the shell's variables aside meanwhile. `Ran` boxes the script it carries, which keeps it small as an error. The red run is the shell's tests: `sh` says `extra operand`. Mutation checks: the outer variables not restored, `$?` not started at 0, `/bin/sh`'s script given no variables, the `--` check in `sh`'s scan of its options removed (it survived at first: a file named after `--` worked by luck; `sh -- -n.sh` now kills it) and `--` not skipped each fail a test.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 2's `Vars`; plan 4a's `commands::Script`, `Shell::run_file`, `run_script`.
- Produces: `Script::{name: String, args: Vec<String>}`; `expand::Vars::script(name, args: &[String]) -> Vars`; `runner::Ran::script: Option<Box<Script>>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn a_script_cannot_run_a_script() {
````

with:

````rust
    #[test]
    fn a_script_gets_its_arguments() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo $0 $# \"[$1]\" $2\necho \"$@\"\n");
        // What bash prints, but that bash would split `b c` in `$2`.
        assert_eq!(
            h.run("sh /tmp/s.sh a 'b c' -x ''"),
            (
                0,
                "+ echo $0 $# \"[$1]\" $2\n/tmp/s.sh 4 [a] b c\n+ echo \"$@\"\na b c -x \n".into()
            )
        );
        // `--` ends `sh`'s options; one after the file is the script's.
        h.put("/tmp/t.sh", b"echo $# \"$@\"\n");
        assert_eq!(
            h.run("sh -- /tmp/t.sh -- -n"),
            (0, "+ echo $# \"$@\"\n2 -- -n\n".into())
        );
        // So may a file whose name starts with `-`.
        h.put("/-n.sh", b"echo n\n");
        assert_eq!(h.run("sh -- -n.sh").1, "+ echo n\nn\n");
        h.put("/tmp/u.sh", b"echo [$0] $#\n");
        assert_eq!(h.run("sh /tmp/u.sh").1, "+ echo [$0] $#\n[/tmp/u.sh] 0\n");
    }

    #[test]
    fn a_script_cannot_run_a_script() {
````

Replace:

````rust
        h.put("/tmp/s.sh", b"echo hi\n");
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh x"),
            (1, "sh: extra operand 'x'\n".into())
        );
````

with:

````rust
        h.put("/tmp/s.sh", b"echo hi\n");
        assert_eq!(run(&mut h, "sh --"), (1, "sh: missing operand\n".into()));
        assert_eq!(
            run(&mut h, "sh -x /tmp/s.sh"),
            (1, "sh: invalid option -- 'x'\n".into())
        );
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_transcript_that_failed_is_reported_when_the_script_ends() {
````

with:

````rust
    #[test]
    fn a_script_has_its_own_variables_and_arguments() {
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"echo $? \"[$A]\" $1\nB=in\nexit 4\n");
        // As bash's, without `export`: nothing passes in or out.
        assert_eq!(
            h.lines(&["A=out", "nope", "sh /tmp/s.sh x", "echo $? $A [$B] $1"]),
            (
                0,
                "relay-sh: nope: command not found\n+ echo $? \"[$A]\" $1\n0 [] x\n\
                 + B=in\n+ exit 4\n4 out []\n"
                    .into()
            )
        );
    }

    #[test]
    fn bin_sh_gives_a_script_its_arguments() {
        let mut h = spawning();
        h.put("/tmp/s.sh", b"t-args $0 $# \"$@\"\n");
        let mut out = FakeStdout::console();
        h.sh(&["/tmp/s.sh", "a", "b c", ""], &mut out);
        assert_eq!(
            h.programs.spawned[0].args,
            ["t-args", "/tmp/s.sh", "3", "a", "b c", ""]
        );
        // A script it runs gets its arguments as typed, expanded.
        h.put("/tmp/r.sh", b"A='x y'\nsh /tmp/s.sh \"$A\" $1\n");
        h.sh(&["/tmp/r.sh", "z"], &mut out);
        assert_eq!(h.programs.spawned[1].args, ["sh", "/tmp/s.sh", "x y", "z"]);
        assert_eq!(h.programs.spawned[1].path, "/bin/sh");
    }

    #[test]
    fn a_transcript_that_failed_is_reported_when_the_script_ends() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `commands::script::tests::a_script_gets_its_arguments`, `shell::tests::a_script_has_its_own_variables_and_arguments`.

- [ ] **Step 4: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
//! `sh FILE`: runs the commands in a file, one line at a time, as if each
//! had been typed (spec §15 item 12). There are no variables, loops or
//! conditions: a script is a list of commands. Each command is shown as
````

with:

````rust
//! `sh FILE [ARG]...`: runs the commands in a file, one line at a time, as
//! if each had been typed (spec §15 item 12), its arguments `$1` on and
//! `$0` the file as given (user-space gate §9.4). There are no loops or
//! conditions: a script is a list of commands. Each command is shown as
````

Replace:

````rust

use crate::ctx::{Ctx, getopt, quote, quote_if_needed};
use alloc::format;
use alloc::string::String;
use vfs::{Errno, FileType, Node, path};
````

with:

````rust

use crate::ctx::{Ctx, getopt, quote_if_needed};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, path};
````

Replace:

````rust
    pub text: String,
    /// The transcript file, emptied, and its name as `sh` was given it.
````

with:

````rust
    pub text: String,
    /// The file as `sh` was given it, the script's `$0`, and its
    /// arguments.
    pub name: String,
    pub args: Vec<String>,
    /// The transcript file, emptied, and its name as `sh` was given it.
````

Replace:

````rust

/// `sh FILE`: checks and reads the file and empties its transcript; the
/// shell then runs its lines (`Shell::execute`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("sh", format_args!("{e}")),
    };
    let file = match &opts.operands[..] {
        [] => return ctx.fail("sh", format_args!("missing operand")),
        [file] => file,
        [_, extra, ..] => return ctx.fail("sh", format_args!("extra operand {}", quote(extra))),
    };
````

with:

````rust

/// `sh FILE [ARG]...`: checks and reads the file and empties its
/// transcript; the shell then runs its lines (`Shell::execute`) with the
/// arguments. Options come only before the file, as bash's do: what
/// follows it is the script's (`sh f -x`).
pub fn sh(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let first = args
        .iter()
        .position(|a| a == "--" || a == "-" || !a.starts_with('-'))
        .unwrap_or(args.len());
    let rest = match args.get(first) {
        Some(a) if a == "--" => &args[first + 1..],
        _ => &args[first..],
    };
    if let Err(e) = getopt(&args[..first], "", "") {
        return ctx.fail("sh", format_args!("{e}"));
    }
    let Some((file, script_args)) = rest.split_first() else {
        return ctx.fail("sh", format_args!("missing operand"));
    };
````

Replace:

````rust
                text,
                transcript,
````

with:

````rust
                text,
                name: file.clone(),
                args: script_args.to_vec(),
                transcript,
````

- [ ] **Step 5: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
            args: alloc::vec![String::from(name)],
        }
    }

````

with:

````rust
            args: alloc::vec![String::from(name)],
        }
    }

    /// A script's: none set, `$0` its `name` and `args` after it.
    pub fn script(name: &str, args: &[String]) -> Vars {
        let mut vars = Vars::new(name);
        vars.args.extend_from_slice(args);
        vars
    }

````

- [ ] **Step 6: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use crate::transcript::Transcript;
use alloc::format;
````

with:

````rust
use crate::transcript::Transcript;
use alloc::boxed::Box;
use alloc::format;
````

Replace:

````rust
    pub exited: bool,
    /// Set by `sh`: the script the shell runs next.
    pub script: Option<Script>,
}
````

with:

````rust
    pub exited: bool,
    /// Set by `sh`: the script the shell runs next (boxed, so that a `Ran`
    /// stays small as an error).
    pub script: Option<Box<Script>>,
}
````

Replace:

````rust
        exited: ctx.exited,
        script: ctx.script.take(),
    }
````

with:

````rust
        exited: ctx.exited,
        script: ctx.script.take().map(Box::new),
    }
````

- [ ] **Step 7: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        if let Some(script) = ran.script {
            status = self.run_script(script);
        }
````

with:

````rust
        if let Some(script) = ran.script {
            status = self.run_script(*script);
        }
````

Replace:

````rust
    /// Its `exit` ends only the script, as it does under `/bin/sh`, where a
    /// script is a shell of its own.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let status = self.run_lines(&script.text);
        if self.exited {
````

with:

````rust
    /// Its `exit` ends only the script, as it does under `/bin/sh`, where a
    /// script is a shell of its own; and it has variables and arguments of
    /// its own, and starts with `$?` 0, as there.
    fn run_script(&mut self, script: Script) -> i32 {
        self.transcript = Some(Transcript::new(script.transcript, script.transcript_name));
        let outer = core::mem::replace(&mut self.vars, Vars::script(&script.name, &script.args));
        self.status = 0;
        let status = self.run_lines(&script.text);
        self.vars = outer;
        if self.exited {
````

Replace:

````rust
        };
        let log = script.transcript_name;
````

with:

````rust
        };
        self.vars = Vars::script(&script.name, &script.args);
        let log = script.transcript_name;
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 260 tests.

- [ ] **Step 9: Run the `sh` scenario**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): give a script its arguments

`sh FILE [ARG]...` gives the script its arguments as `$1` on, `$#` and
`$@`, and `$0` the file as `sh` was given it (user-space gate §8.3,
§9.4), under both runners: `/bin/sh` already passes a script's
arguments to the child shell, which now takes them instead of saying
`sh: extra operand`. Options come only before the file, as bash's do,
so what follows it is the script's (`sh f -x`), and `--` lets a file's
name start with `-`.

A script starts with no variables and `$?` 0, and nothing it sets
reaches the shell that ran it, as with bash without `export`; that
shell's `$?` afterwards is the script's status. The in-process runner,
which runs a script in its own shell, keeps the shell's variables
aside while the script runs.
EOF
````


### Task 12: The shell's `$0` is its argument 0

Decision 7: outside a script `$0` is the shell's argument 0, as bash's is: `/bin/sh` for the shell init starts, `sh` for one typed at a prompt or at the end of a pipeline (`X | sh`). `Shell::named` sets it, and `/bin/sh`'s `main` passes its argument 0; the in-process runner keeps `relay-sh`, and a script's `$0` stays its file. The red run is the shell's tests, which cannot find `Shell::named`. Mutation check: `named` setting nothing fails the test; `main`'s two calls are checked by Task 14's scenario.

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Modify: `userland/sh/src/main.rs`

**Interfaces:**
- Consumes: Task 2's `Vars`.
- Produces: `Shell::named(self, name: &str) -> Shell`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_line_that_does_not_expand_runs_nothing() {
````

with:

````rust
    #[test]
    fn a_shell_s_name_is_its_argument_0() {
        let mut h = spawning();
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .named("/bin/sh")
            .execute("t-args $0 $#");
        assert_eq!(h.programs.spawned[0].args, ["t-args", "/bin/sh", "0"]);
        // A script's is its file, whatever the shell's.
        h.put("/tmp/s.sh", b"t-args $0\n");
        let mut out = FakeStdout::console();
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .named("sh")
            .run_file(&["/tmp/s.sh".into()], &mut out);
        assert_eq!(h.programs.spawned[1].args, ["t-args", "/tmp/s.sh"]);
    }

    #[test]
    fn a_line_that_does_not_expand_runs_nothing() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no method named `named` found for struct `Shell<'a>` in the current scope ``.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            vars: Vars::new(NAME),
        }
    }

````

with:

````rust
            vars: Vars::new(NAME),
        }
    }

    /// The same shell, its `$0` `name`: its argument 0, as bash's is
    /// (`relay-sh` otherwise).
    pub fn named(mut self, name: &str) -> Shell<'a> {
        self.vars = Vars::new(name);
        self
    }

````

- [ ] **Step 4: Change `userland/sh/src/main.rs`**

In `userland/sh/src/main.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
//! is no console (`X | sh`): then it runs the lines it reads there and
//! leaves the console alone. `sh FILE` runs a script, its commands in the
//! shell's own process group and its transcript a console tee. Every
//! command but `cd`, `exit` and `help` is a program.
#![no_std]
#![no_main]

use relay_rt::sysio::words;
````

with:

````rust
//! is no console (`X | sh`): then it runs the lines it reads there and
//! leaves the console alone. `sh FILE [ARG]...` runs a script, its
//! commands in the shell's own process group and its transcript a console
//! tee. Every command but the shell's own is a program. Outside a script
//! `$0` is the shell's argument 0, as bash's is.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use relay_rt::sysio::words;
````

Replace:

````rust
    let words = words(&args);
    if words.is_empty() && !SysStdin::is_console() {
        // In a pipeline, in its group: commands read from the pipe, and the
        // console stays with the group, in line mode, so Ctrl-C ends them.
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run_input(&mut SysStdin) as u8
````

with:

````rust
    let words = words(&args);
    let name = args
        .iter()
        .next()
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .unwrap_or_default();
    if words.is_empty() && !SysStdin::is_console() {
        // In a pipeline, in its group: commands read from the pipe, and the
        // console stays with the group, in line mode, so Ctrl-C ends them.
        let (mut console, mut programs) = (SysConsole::new(), SysPrograms::new(false));
        let mut shell =
            Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs).named(&name);
        shell.run_input(&mut SysStdin) as u8
````

Replace:

````rust
        let mut programs = SysPrograms::new(leader);
        let mut shell = Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs);
        shell.run();
````

with:

````rust
        let mut programs = SysPrograms::new(leader);
        let mut shell =
            Shell::spawning(&mut vfs, &mut console, &mut system, &mut programs).named(&name);
        shell.run();
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 261 tests.

- [ ] **Step 6: Run the `sh` scenario**

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add crates userland
git commit -F - <<'EOF'
feat(sh,shell): name the shell's $0 after its argument 0

Outside a script, `$0` is the shell's argument 0, as bash's is:
`/bin/sh` for the shell init starts, `sh` for one typed at a prompt or
at the end of a pipeline (`X | sh`). `Shell::named` sets it; the
in-process runner keeps `relay-sh`. A script's `$0` stays its file as
`sh` was given it.
EOF
````


### Task 13: A piped script is read a byte at a time

Plan 1's deferred minor and decision 10: `X | sh` read its input 4 KiB at a time, so a command it ran found the rest of the piped input gone into the shell: `printf 'cat\nx\n' | sh` ran `x` as a command where bash gives it to `cat`. `Shell::run_input` now reads a byte at a time, as bash reads a pipe, and so takes no more than each line. The red run is the shell's tests: the new one sees reads of 4096 bytes. Mutation check: reading 4096 bytes again fails the test (and Task 14's scenario).

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: plan 1's `Shell::run_input`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(h.programs.spawned.last().unwrap().args, ["t-args", "c"]);
    }
````

with:

````rust
        assert_eq!(h.programs.spawned.last().unwrap().args, ["t-args", "c"]);
    }

    /// Input that records how much each read asked for.
    struct Asked {
        bytes: crate::Bytes,
        asked: Vec<usize>,
    }

    impl crate::Stdin for Asked {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
            self.asked.push(buf.len());
            self.bytes.read(buf)
        }
    }

    #[test]
    fn a_shell_reading_a_pipe_takes_no_more_than_each_line() {
        // bash reads a pipe a byte at a time, so `printf 'cat\nx\n' | bash`
        // gives `cat` the `x`; here a command reads the same fd 0.
        let mut h = spawning();
        let mut input = Asked {
            bytes: crate::Bytes::new(b"t-args a\nt-args b\n".to_vec()),
            asked: Vec::new(),
        };
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(h.programs.spawned.len(), 2);
        assert!(input.asked.iter().all(|&n| n == 1), "{:?}", input.asked);
        assert_eq!(input.asked.len(), 19, "18 bytes and the end");
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_shell_reading_a_pipe_takes_no_more_than_each_line`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    /// editor, so it never takes the console; it ends at the input's end or
    /// `exit`. A line over 64 KiB, or not UTF-8, is skipped with a message,
    /// as `sh` refuses such a script. Returns the last status.
    pub fn run_input(&mut self, input: &mut dyn Stdin) -> i32 {
        let mut buf = alloc::vec![0; 4096];
        let mut line: Vec<u8> = Vec::new();
        let mut too_long = false;
        self.stopped = false;
        loop {
            let n = match input.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    let message = format!("sh: standard input: {e}\n");
                    return self.finish(1, message);
                }
            };
            if n == 0 {
                if !line.is_empty() || too_long {
                    self.input_line(&line, too_long);
                }
                return self.status;
            }
            for piece in buf[..n].split_inclusive(|&b| b == b'\n') {
                let ended = piece.last() == Some(&b'\n');
                let piece = &piece[..piece.len() - usize::from(ended)];
                if !too_long {
                    line.extend_from_slice(piece);
                    if line.len() as u64 > SCRIPT_MAX {
                        too_long = true;
                        line.clear();
                    }
                }
                if ended {
                    self.input_line(&line, too_long);
                    line.clear();
                    too_long = false;
                    if self.stopped {
                        return self.status;
                    }
                }
````

with:

````rust
    /// editor, so it never takes the console; it ends at the input's end or
    /// `exit`. It reads a byte at a time, as bash reads a pipe, so that a
    /// command it runs reads what follows its line (`printf 'cat\nx\n' |
    /// sh` gives `cat` the `x`). A line over 64 KiB, or not UTF-8, is
    /// skipped with a message, as `sh` refuses such a script. Returns the
    /// last status.
    pub fn run_input(&mut self, input: &mut dyn Stdin) -> i32 {
        let mut line: Vec<u8> = Vec::new();
        let mut too_long = false;
        self.stopped = false;
        loop {
            let mut byte = [0];
            match input.read(&mut byte) {
                Ok(0) => {
                    if !line.is_empty() || too_long {
                        self.input_line(&line, too_long);
                    }
                    return self.status;
                }
                Ok(_) => {}
                Err(e) => {
                    let message = format!("sh: standard input: {e}\n");
                    return self.finish(1, message);
                }
            }
            if byte[0] == b'\n' {
                self.input_line(&line, too_long);
                line.clear();
                too_long = false;
                if self.stopped {
                    return self.status;
                }
            } else if !too_long {
                line.push(byte[0]);
                if line.len() as u64 > SCRIPT_MAX {
                    too_long = true;
                    line.clear();
                }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 262 tests.

- [ ] **Step 5: Run the `pipes`, `sh` scenarios**

Run: `cargo xtask test --e2e-only --scenario pipes`

Expected: PASS: ends with `all 1 scenario(s) passed`.

Run: `cargo xtask test --e2e-only --scenario sh`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read a piped script a byte at a time

`X | sh` read its input 4 KiB at a time, so a command it ran found the
rest of the piped input gone into the shell (plan 1's deferred minor):
`printf 'cat\nx\n' | sh` ran `x` as a command where bash gives it to
`cat`. The shell now reads its standard input a byte at a time, as
bash reads a pipe, and so takes no more than each line.
EOF
````


### Task 14: The scenario `script_vars`

Spec §12.3 and decision 12: the scenario runs a script as `sh /root/v.sh one 'two three' ''`, which prints `$0`, `$1`, `$#`, `"$@"` and `$@`, `$?` after `false`, and a variable with and without quotes, then exits with 3; the shell gets its status and none of its variables, and the transcript holds the lines as written. At the prompt: a variable in arguments, a pipeline and a redirection that is no word, `$0` (`/bin/sh`), `$?` after a missing program, a Ctrl-C, a job's start and a `wait`; a nested `sh` and `X | sh` with variables and a name of their own; and `X | sh` leaving the rest of its input to the command it runs. Where bash would split a value, the scenario says so. The task adds a test of what Tasks 11–13 built, so it has no failing run. Mutation checks: `/bin/sh` without `Shell::named` at the prompt and in `X | sh`, and `X | sh` reading 4096 bytes at a time, each fail the scenario.

**Files:**
- Create: `tests/e2e/script_vars.txt`

**Interfaces:**
- Consumes: Tasks 2–13.
- Produces: the scenario `script_vars`.

- [ ] **Step 1: Add the scenario `tests/e2e/script_vars.txt`**

Create `tests/e2e/script_vars.txt`:

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
send echo 't-args "$@"' >> /root/v.sh
send echo 't-args $@' >> /root/v.sh
send echo 'false' >> /root/v.sh
send echo 'echo "?=$?"' >> /root/v.sh
send echo 'A="a  b"' >> /root/v.sh
send echo 't-args $A "$A" "$UNSET" $UNSET' >> /root/v.sh
send echo 'exit 3' >> /root/v.sh
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
send echo $? "[$A]"
expect \n3 \[\]\n
# The transcript holds the lines as written.
send cat /root/v.log
expect \n\+ echo "0=\$0 1=\$1 #=\$#"\n0=/root/v.sh 1=one #=3\n\+ t-args "\$@"\n
# At the prompt: variables, in a pipeline and in a redirection.
send B='hi  there'
send t-args $B "$B" ${B}!
expect \n\[1\] hi  there\n\[2\] hi  there\n\[3\] hi  there!\n
send echo $B | wc -c
expect \n10\n
send echo x > $NONE
expect \nrelay-sh: \$NONE: ambiguous redirect\n
send echo $0 $# [$1]
expect \n/bin/sh 0 \[\]\n
# $? after a program that is not found, a Ctrl-C, a job and a wait.
send nosuch
send echo $?
expect \n127\n
key echo no{ctrl-c}
expect \^C\n
send echo $?
expect \n130\n
send t-spin &
expect \n\[1\] \d+\n
send echo $?
expect \n0\n
send kill %1
send wait %1
expect \n(pid \d+ \(/bin/t-spin\): killed: kill\n)?\[1\]\+  Killed                  t-spin\n
send echo $?
expect \n137\n
# A shell started at the prompt has its own variables and name.
send sh
expect \nroot@relay:~# $
send echo "$0 [$B]"
expect \nsh \[\]\n
send exit
expect \nroot@relay:~# $
# A shell reading a pipe takes no more than each line, as bash's: cat
# gets the line after it.
send echo cat > /root/in.txt
send echo 'echo "[$B]" no' >> /root/in.txt
send cat /root/in.txt | sh
expect \necho "\[\$B\]" no\n
send echo 'echo $0 $#' | sh
expect \nsh 0\n
send echo $B
expect \nhi  there\n
````

- [ ] **Step 2: Run the `script_vars` scenario**

Run: `cargo xtask test --e2e-only --scenario script_vars`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): add the script_vars scenario

A script run as `sh /root/v.sh one 'two three' ''` prints `$0`, `$1`,
`$#`, `"$@"` and `$@`, `$?` after `false`, and a variable with and
without quotes, then exits with 3 (user-space gate §12.3); the shell
gets its status and none of its variables, and its transcript holds
the lines as written. At the prompt: a variable in arguments, a
pipeline and a redirection that is no word (`ambiguous redirect`),
`$0` (`/bin/sh`), `$?` after a missing program, a Ctrl-C, a job's
start and a `wait`; a nested `sh` and `X | sh` with variables and a
name of their own; and `X | sh` leaving the rest of its input to the
command it runs.

Where bash would split an unquoted value into words, the scenario says
so.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 47 scenario(s) passed`.

````bash
git push -u origin m3p3/scripts
gh pr create --base main --head m3p3/scripts --title "feat(shell,sh): give scripts their arguments" --body-file - <<'EOF'
## What

Milestone 3, plan 3, tasks 11–14: `sh FILE [ARG]...` gives the script `$1` on, `$#`, `$@` and `$0` (its file as given) under both runners, its options before the file only; a script, a nested `sh` and `X | sh` start with no variables, nothing passes in or out, and the shell gets the script's status; outside a script `$0` is the shell's argument 0 (`/bin/sh`, `sh`); `X | sh` reads a byte at a time, so a command it runs reads the lines after its own. Scenario `script_vars` (new).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and QEMU scenarios

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC (the shell's parsing and expansion, run the same in QEMU); no check script holds a `$`, and plan 4's NUC check 5 runs script arguments and variables there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m3p3/scripts --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m3p3-scripts
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
