# Milestone 4 · Plan 2: Compound commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/bin/sh` runs `if`/`elif`/`else`, `while`, `until` and `for`, as bash 5.2 does, with bash's statuses and syntax errors; their words are keywords only where a command name stands, and bash's other reserved words stay refused. A construct is read across lines, at the prompt with `> `, in scripts and in `X | sh`, and runs only once it is whole: one dropped before its end, by any error, is dropped to its end, so nothing of it runs in part. Compound commands nest at most 32 levels deep. Ctrl-C ends a loop, of built-ins too, at the prompt as well as in scripts. The spec's §15 item 2 lands with this plan. It ends with `cargo xtask ci` green, the scenario `control` extended, 48 scenarios in all.

**Architecture:** In the shell crate, with one change each to the kernel and `relay-rt` for Ctrl-C. A pipeline holds a `Run`: simple commands, or one compound command (`If`, `While`, `Until`, `For`), so a compound command cannot stand among others. The parser (`parser::Parser`) keeps a command's state from one line to the next and a stack of the constructs being read, so the `Reader` gives it each line once, in its context; a command dropped before its end is dropped by a scan (`crates/shell/src/scan.rs`) that counts openers and closers, reading refused syntax and here-documents as bash does. The walker gains `run_items`, `run_if`, `run_loop` and `run_for`, and asks the console for a Ctrl-C before each command; the interactive `/bin/sh` asks the kernel through `wait(0, WAIT_NOHANG | WAIT_CTRL_C)` (pid 0 was `EINVAL`; ABI 3 unchanged), takes the console back after each program, and the kernel keeps a Ctrl-C typed for a group that has ended for the next holder.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; QEMU 8.2 under KVM with `-cpu max` (the host has the NUC's CPU model); bash 5.2.21 (interactive in a pty, `tmp/m4p2/pty_bash.py`, for the prompt and every syntax error; `bash FILE` for the corpus, on CI's ubuntu-24.04 too).

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§1.4, §2, §4, §5, §10, §11.1–§11.3, §12, §15 items 1 and 2)
**Roadmap:** `docs/superpowers/plans/2026-10-02-milestone-4-roadmap.md` — this is plan 2 of 4 of milestone 4.

## In brief

- **Size.** 20 tasks in three code pull requests, plus this plan as PR 1 (decision 1): PR 2 the parser read a line at a time, the drop to a construct's end, `if` and the nesting bound; PR 3 the Ctrl-C check (with the kernel's `wait`), `while`, `until` and `for`; PR 4 `help` and the scenario `control`. A PR that ships `if` before the loops still refuses `while` and `for`. No NUC check: plan 4's `check6.sh` runs compound commands there.
- **Nothing runs in part** (decision 6): the parser reads each line once, in its context, so an error is told on its own line (plan 1's deferred minor); a command dropped before its end, by a syntax error, a refusal, 64 KiB, a line that is no text or over 64 KiB, the nesting bound, Ctrl-C at `> ` or the input's end, is dropped to the end of its constructs, and a script's `if` written across lines no longer runs its body while it is refused (plan 1's ruling).
- **bash's words** (decisions 2, 3, 4, 5), probed against an interactive bash 5.2.21 for every case (`tmp/m4p2/progress.md`): `syntax error near unexpected token` with bash's token, a word after `fi` or `done` named as typed, `for`'s name checked when it runs; refused, a compound command in a pipeline, before `&` or redirected (`| after fi`, `while after |`, `& after done`, `> after fi`).
- **Statuses and Ctrl-C** (decisions 8, 9, 10): bash's statuses; after Ctrl-C ends a command line `$?` is 130 but for a lone pipeline (plan 1's deferred minor); bash's lone `if` going on to its `else` after Ctrl-C in its condition is a decided difference. A loop a person writes may run for ever, and Ctrl-C ends it.
- **Two findings while prototyping**, each settled with the user: `/bin/sh` could not ask for a Ctrl-C between built-ins (decision 10, Task 10), and the 32-level bound leaves room (1024 levels ran under `/bin/sh` in QEMU).
- **The prototype's review.** A fresh reviewer read the whole prototype (then thirteen tasks), ran the shell's and the kernel's tests, a fuzz of 300,000 random texts comparing the scan with the parser, throwaway probes and QEMU scenarios under KVM, and compared about 40 interactive cases and an 18-line corpus script with bash 5.2.21. It found 1 critical, 2 important and 5 minor real defects (one minor there since milestone 3). Each is fixed in a task of its own after the task it concerns, with a test that fails first and the mutation checks that show what it guards, but two that a commit settles (M3, M4) and one the user had ruled (M2). It found correct: statuses and messages against bash, the keyword rules, the `for` header, Ctrl-C in QEMU in a loop of built-ins alone, in scripts, in `X | sh` and with a job running, the kernel's `wait(0, …)`, the parser's stack and bounds, the reader and the scan's constant memory, the shell's flags, PR 2 alone refusing the loops' words, and the test console's changed step counts. It declined to judge which fix to make for the critical finding (the user chose, decision 10), the drop to a construct's end after an error on its first line (decided, 6), `X | sh` keeping `\r` (unchanged since milestone 1) and the NUC's stack (1024 levels ran in QEMU).

| Finding (review) | Decision |
|---|---|
| Critical (C1): once a program had run on a command line, its group kept the console in line mode until the next prompt, and a later Ctrl-C was spent on it: `true; while cd; do cd; done` ended only with a reset, and plan 1's `sleep 20 & true; wait` likewise | Fixed, Tasks 13 (the kernel keeps a Ctrl-C for a group that has ended), 14 (the shell takes the console back after each program) and 15 (the scenario) |
| Important (I1): the drop scan miscounted refused syntax (`$(…)`, backquotes, `time`, `{`, `select`, `case`), so a loop's tail ran without its head | Fixed, Task 4 |
| Important (I2): a job in a `for`'s body had the header in its text, and `for é do ! a & done` panicked | Fixed, Task 17 |
| Minor (M1): the scan dropped lines bash runs, after `a && >` and after `fi if c` | Fixed, Task 5 |
| Minor (M2): `cargo xtask host-shell` cannot end a loop at Ctrl-C | Ruled by the user: a developer tool, outside §5.2's `/bin/sh` |
| Minor (M3): one commit of `control` lacked its `e2e` scope | Fixed in that commit (Task 8) |
| Minor (M4): a 620-byte `send` in `control` was not followed by an `expect`, which can garble serial | Fixed in Task 8's scenario |
| Minor (M5, since milestone 3): a refused here-document's body ran as commands | Fixed, Task 6 (the user chose to fix it here) |
| Declined to judge: the fix for C1; the drop after a construct's first-line error; `X \| sh` keeping `\r`; the NUC's stack | The user chose C1's fix; the others are decided (§15 item 2, milestone 1) or measured (1024 levels in QEMU) |

## Where this plan fits

Plan 2 of milestone 4 implements the gate's second step (spec §12). It builds on plan 1: the tree of lists, and-or lists and pipelines, the `Reader` with its `> ` prompt, the bash corpus and the scenario `control`. It leaves plan 3 (`test` and `[`, and `grep -q`) conditions to write, and plan 4 the NUC check of it all.

## Working conventions

- Plan 2 lands as **four pull requests** (table below). This plan, with the spec's §15 item 2 (and the parts of its body it corrects) and the roadmap's notes, is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-02-m4-plan-2-compound-commands.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; the shell's test console in `crates/shell/src/testing.rs`; corpus scripts under `crates/shell/tests/corpus/`; e2e scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Tasks 9, 15, 18 and 20 have no failing run: they test what exists, and their introductions say what the mutation checks showed. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m4p2/mutations.md`).
- **The expectations are bash 5.2's.** An interactive bash in a pty for the prompt and every syntax error (`tmp/m4p2/pty_bash.py`), `bash FILE` for scripts; what they printed is in `tmp/m4p2/progress.md` with its id (e1, f4, c6, …), and the tests name those ids. Where this shell departs (a value is never split into words, a construct dropped to its end where interactive bash runs its later lines, Ctrl-C ending a lone `if`, a pipeline expanded whole), the test or the spec's §10 says so.
- **Bound every loop in a test.** A loop a person writes may run for ever, so every loop a test runs ends by itself (on a file it removes or makes, `exit`, a variable `grep` checks) or at a Ctrl-C the test console presses; the test console fails a test that asks for a Ctrl-C more than 100,000 times (Task 12), so a loop that never ends fails instead of hanging. Mutation checks run under an address-space limit and a timeout (`tmp/m4p2/mutate.py`).
- **The NUC check scripts and their transcripts do not change**: this plan has no NUC check, and none of them uses a compound command.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two (so the review's Ctrl-C fix is three tasks: the kernel's, the shell's and `relay-rt`'s, the scenario's); the tests, corpus scripts and scenarios that come with the change belong to its commit; a task that only adds tests names the module whose files it changes. Every commit body line is within 72 columns. Chain a commit after a check with `&&`, never `;`. Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive, and serial input garbles when long lines or several `send`s come unpaced: each `send` of the scenario is followed by an `expect`. If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `489e69f` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m4p2/plan` | — | The spec's §15 item 2 and the corrections to its body, the roadmap's notes, this plan | `lint`, `unit`, `e2e` |
| 2 | `m4p2/if` | 1–9 | The `Run` tree; the parser read a line at a time; the drop to a construct's end (refused syntax, here-documents); `if`; the 32-level bound | `lint`, `unit`, `e2e` |
| 3 | `m4p2/loops` | 10–18 | `wait(0, …)` and the kept Ctrl-C (kernel); the Ctrl-C check and the take-back (shell, `relay-rt`); `while`, `until`, `for` | `lint`, `unit`, `e2e` |
| 4 | `m4p2/control` | 19–20 | `help`'s lines; the scenario `control` | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 2 adds no crate. `crates/shell` uses only `vfs` and `relay-abi` and is `no_std` outside its tests. The kernel has no dev-dependencies and does not depend on `shell`.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (UG §3.1); `relay-abi` holds no architecture detail and keeps the same values on every architecture. Plan 2 changes no ABI value: `relay_abi::VERSION` stays 3, and `wait`'s pid 0 with `WAIT_NOHANG | WAIT_CTRL_C`, which was `EINVAL`, gains a meaning.
- Every milestone 1–3 scenario, plan 1's, and every check script pass unchanged; the NUC check scripts and their recorded transcripts do not change.
- What a person types is untrusted (AGENTS.md): nothing that follows from it may make the shell panic, index out of bounds, overflow, allocate without bound or loop forever (a loop a person writes may run forever, and Ctrl-C ends it). `/bin/sh`'s heap ends the program when it runs out and its stack is fixed, so every allocation sized from input is bounded (a command at 64 KiB, the scan's memory constant) and recursion too (32 levels). Panics are for bugs only.
- Shell messages follow bash 5.2 (interactive where it differs), word for word, `relay-sh:` for `bash:`; unsupported syntax is `relay-sh: unsupported syntax: <what>`, status 2, never passed on as text nor run in part.
- Missing tools fail tests, never skip them; the corpus needs bash 5.2, which CI's runner has.
- Every PR must pass `cargo xtask ci`. The workflow only installs tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task, and nothing a PR accepts runs in part.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 2 in PR 1:

1. **The tree** (§4.1). `Command` stays the simple command (words and a redirection), which expansion and the runner take. A `Pipeline` holds its `!` and either simple commands joined by `|` or one compound command: `If` (each `if` or `elif` condition with its body, and the `else` body), `While` and `Until` (a condition and a body) and `For` (its name as typed, its words, or none for `"$@"`, and its body), each condition and body a `List`. A compound command among other commands, with `&` or with a redirection cannot stand in the tree: the parser refuses it. Plan 2 is four pull requests: its plan; `if`, with the tree, the parser read a line at a time, the dropping of a construct to its end, the nesting bound and the refusals, while `while`, `until` and `for` stay refused; the loops and the Ctrl-C check, which reaches the kernel and `relay-rt`; `help` and the scenario `control`. It has no NUC check; plan 4's check 6 runs compound commands there.
2. **Keywords** (§4.2). A compound command's word is a keyword only unquoted, whole, and where a command name would stand (after any `!`), and `in` and `do` also where a `for` takes them; after a command's name and in a `for`'s words it is a word (`echo if`, `for x in if then`). After `fi` or `done` the next word must be a keyword that the construct around takes there (`fi fi`, `fi done`, `fi then`, as bash takes them) or an operator; any other word, `!` and `if` included, is bash's syntax error naming it, and so is `in` where a command name would stand. bash's other reserved words, `break` and `continue` stay refused.
3. **`for`** (§4.1). Its name must be on its line (`for` alone is the syntax error near `newline`); any word is taken as the name, `in` too, and checked when the loop runs, as bash checks it: ``relay-sh: `1x': not a valid identifier``, status 1, naming the word as typed, and the line goes on. The words after `in` run to a `;` or a newline, keywords among them; then, past blank lines, `do`. `for NAME do`, `for NAME; do` and `for NAME` with `do` on a later line loop over `"$@"`.
4. **Syntax errors** (§4.4), each bash 5.2's ``syntax error near unexpected token `X'``, status 2, as interactive bash names it: a keyword where none fits (`then`, `elif`, `else`, `fi`, `do`, `done` or `in` where a command name stands and the construct around does not take it), an empty condition or body (the keyword after it, or the `;` that ends nothing, as in `do; echo`), a `for` without `in` or `do` (the word in their place), a word after `fi` or `done`; the input's end inside a construct is `syntax error: unexpected end of file`.
5. **Refusals** (§4.1), `relay-sh: unsupported syntax: <what>`, status 2: `| after fi` (or `done`) for a compound command before `|`, `if after |` (or `while`, `until`, `for`) for one after it, `& after fi`, and `> after fi` or `>> after fi` until milestone 5 redirects compound commands. A background job inside a compound command (`a &`) is allowed, as in bash; the in-process runner refuses a command holding one at any depth before any of it runs.
6. **Reading a construct** (§4.3, §4.5). The parser keeps its state from one line to the next, so the reader gives it each line once, in its context: reading 64 KiB stays linear, and an error is told on the line that has it, which fixes plan 1's judging of a line alone (`a |` then `! b |` was told a line late). A command dropped before its end (a syntax error, a refused word, past 64 KiB, a line that is no text, an `X | sh` line over 64 KiB) is dropped to its end by a scan that never fails: it reads quotes, escapes, comments, `${…}`, `$(…)` and backquotes as the parser and bash read them, counts each `if`, `while`, `until`, `for`, `select` and `case` and each `fi`, `done` and `esac` where a command name would stand (after `time`, `{` and `}` too, which bash allows, and never in a word before `)`, a `case` pattern), and notes a line ending after `|`, `&&` or `||`; the drop ends with the line after which the count is 0 and nothing is left open. It runs over the text read so far, then over each later line; a line too long or not text is fed to it a byte at a time as it is read, so the count stays exact (replacing plan 1's judging of such a line by its last bytes). So `if true` / `then` / `fi` drops nothing after `fi`, as bash does, and in `if true; then` / `echo a; then` / `echo b` / `fi` the line `echo b`, which interactive bash would run, is dropped (§10). At the prompt `> ` shows while a construct is read or dropped.
7. **The nesting bound** (§4.5). Each compound command is one level, and one that follows `&&` or `||` one more, as the walker's frame for the and-or list stands under it. The parser keeps the open constructs on a stack of its own and never recurses; the opener that would pass 32 levels is refused with §4.5's message, and its construct is dropped to its end. The `control` scenario runs a script at 32 levels under `/bin/sh` and one refused at 33.
8. **Statuses** (§5.1), as bash 5.2's: an `if` with no branch taken is 0, even after a failing condition; a loop whose body never runs is 0; a `while` or `until` condition is expanded again on each pass; `for` expands its words once, as arguments are, and sets its variable through the shell's variables before each pass. A value that does not fit in their 64 KiB fails the loop, status 1, and the line goes on, as an assignment that does not fit does; a bad substitution in its words abandons the top-level command, as `for` is a command the shell runs itself.
9. **Ctrl-C** (§5.2). The walker checks for a Ctrl-C before each simple and each compound command, printing `^C` and ending the command with 130. After Ctrl-C ends a top-level command `$?` is 130, as bash's is, unless that command is one pipeline, which keeps its own status (130, or 0 under `!`, §15 item 1); this fixes plan 1's `! sleep 5; echo x`, which left 0. bash 5.2 goes on after Ctrl-C in the condition of a lone compound command (`if sleep 5; then …; else echo b; fi` prints `b`, status 0); here §5.2 holds and it ends, status 130 (§10).
10. **The Ctrl-C between the shell's own commands** (§5.2). The interactive `/bin/sh` keeps the console in raw mode while it runs its own commands, where a Ctrl-C is input it would read only at the next prompt, so it asks the kernel: `wait(0, WAIT_NOHANG | WAIT_CTRL_C)`, which was `EINVAL`, says whether a raw Ctrl-C was typed for the caller's group (`EINTR`, taking it) and collects no child; the ABI's values and version (3) do not change. Once a foreground program it waited for has ended, the shell takes the console back (its group, raw mode), and a line-mode Ctrl-C that reaches a group with no process left is kept as a raw one for the group that holds the console next, so that a Ctrl-C typed after a program on the same command line still ends a loop of built-ins (the prototype's review found it lost until a reset). So plan 2 changes the kernel and `relay-rt`, not only the shell.
11. **Here-documents** (§4.2). `<<` and `<<-` stay refused; the lines after a line refused for one, up to its delimiter's line, are dropped with it, so a here-document's body never runs as commands (the prototype's review found it run since milestone 3).
12. **The script trace** (§5.4). Each line is traced as it is read, before the reader takes it, dropped lines too, so a construct's lines are all traced before any of it runs; blank and comment lines are not.
13. **A pipeline is expanded whole** (UG §16 item 10). `echo ${1A} | cat; echo $?` gives the bad substitution and 1, where bash's subshells fail only `echo` and `cat` gives 0. This is kept, a decided difference (§10), rather than expanding each command of a pipeline apart.

## Review Focus

1. **A construct run in part:** an `if`, `while`, `until` or `for` dropped before its end, at the prompt, in a script and in `X | sh`: by a syntax error in a construct inside it, refused syntax (`$(…)`, backquotes, `case`, `time`, `{`, a here-document), past 64 KiB, a line that is no text or over 64 KiB, past 32 levels, Ctrl-C at `> ` or between a script's lines, the input's end; a `fi` or `done` hidden in a quote, `${…}`, `$(…)`, a `for`'s words or a here-document's body. Expected: nothing of it runs, the line after its end runs, bash's message. Tests: `refused_syntax_is_read_as_bash_reads_it`, `a_here_document_s_body_is_read_to_its_delimiter` and the scan's other tests (Tasks 3–6), `a_refused_construct_runs_none_of_its_lines` (Tasks 3, 12), `a_dropped_construct_s_refused_syntax_hides_none_of_its_end` (Task 4), `a_line_bash_ends_in_error_drops_no_more` (Task 5), `a_here_document_s_body_does_not_run` (Task 6), `an_if_with_an_error_inside_is_dropped_to_its_fi` (Task 7), `a_construct_nested_too_deep_is_dropped_to_its_end` (Task 8), `an_if_dropped_any_way_runs_none_of_it`, `an_if_typed_at_the_prompt_runs_only_whole` (Task 9).
2. **Ctrl-C in loops at the NUC's prompt:** `while cd; do cd; done`, `while true; do sleep 1; done`, `until false; do A=x; done`, a Ctrl-C in a condition, in a nested loop, after a program earlier on the line (`true; while cd; do cd; done`), at a `wait` after one, with a background job running, in a script and in `X | sh`. Expected: the loop ends at once with one `^C`, `$?` 130 (a lone pipeline keeping its own), the next line running whole, no Ctrl-C lost. Tests: `wait_for_pid_0_asks_only_whether_ctrl_c_was_typed` (Task 10), `ctrl_c_ends_the_shell_s_own_commands_between_them`, `after_ctrl_c_ends_a_command_line_its_status_is_130` (Task 11), `ctrl_c_ends_a_loop_of_built_ins_or_of_programs` and the scenario `control` (Task 12), `a_line_ctrl_c_for_a_group_that_ended_waits_for_the_next_holder` (Task 13), `the_shell_takes_the_console_back_after_each_program` (Task 14), `control` (Task 15).
3. **Keywords in and out of place:** quoted or escaped keywords, keywords as arguments, after a redirection, after `!`, `fi done` and `done fi` with no `;`, `then`, `do` or `in` where a command name stands, an empty condition or body, a `for` header with operators, comments, or `in` on a later line, `for in in in`, nesting at 32 and 33 levels with `&&`, a compound command in a pipeline, before `&` or redirected. Expected: bash's `syntax error near unexpected token` with bash's token, or this shell's refusal, and the same tree on one line or many. Tests: `if_words_are_keywords_only_where_a_command_name_stands`, `a_keyword_out_of_place_is_bash_s_syntax_error`, `an_if_in_a_pipeline_with_ampersand_or_redirected_is_refused`, `an_if_not_finished_needs_more_lines` (Task 7), `compound_commands_nest_at_most_32_levels_deep` (Task 8), `while_and_until_hold_a_condition_and_a_body`, `a_loop_s_keyword_out_of_place_is_bash_s_syntax_error` (Task 12), `a_for_holds_its_name_its_words_and_its_body`, `a_for_s_header_takes_no_operator` (Task 16).
4. **`for` and loop state:** `for x in "$@"` and `for x; do` in a script with arguments (empty ones, ones with blanks), `$E` unset and `''` among the words, a value past the 64 KiB of variables, `for 1x`, `for "x"`, `$x` after the loop, `$?` in a body and after a loop that never ran, `exit` in a body, a bad substitution in the words, a background job in a body. Expected: bash's output and status (but never splitting words). Tests: `while_and_until_run_their_body_while_the_condition_allows` (Task 12), `a_for_runs_its_body_once_for_each_word`, `a_for_without_words_loops_over_the_script_s_arguments` (Task 16), `a_job_in_a_for_s_body_has_its_own_text` (Task 17), the corpus's `loops.sh` and `for.sh` (Task 18).
5. **Scripts and pipes with constructs:** each line traced once as it is read, all of a construct's lines before it runs, blank and comment lines not traced; the transcript the same as the screen; a script ending inside a construct; a construct read from a pipe whose command reads what follows it (`cat p.sh | sh`, its `cat` reading the line after `fi`); a nested `sh` running a script with loops. Expected: bash's behaviour or its message, the trace as §5.4 says. Tests: `a_script_s_if_is_traced_whole_before_it_runs_or_is_dropped` and `control`'s pipe (Task 9), the corpus's `if.sh`, `control`'s nested loops and transcript (Task 20).

## File map (end of this plan)

| Path | Responsibility |
|---|---|
| `crates/shell/src/parser.rs` | `Pipeline::run`, `Run`, `Compound`, `If`, `Loop`, `For`, `List::has_job`; `Parser` (a command's state between lines, the stack of `Open` constructs, `Keyword`, `Kind`, `Stage`, the `for` header); `NESTING_MAX`; `ParseError::Unexpected` |
| `crates/shell/src/scan.rs` (new) | `Scan`: what a dropped command's lines open and close, a byte at a time, refused syntax and here-documents read as bash reads them |
| `crates/shell/src/reader.rs` | The `Reader` over a `Parser`, dropping through a `Scan` (`drop_line`, `drop_bytes`, `drop_end`) |
| `crates/shell/src/shell.rs` | `run_list`, `run_items`, `run_compound`, `run_if`, `run_loop`, `run_for`; the Ctrl-C check before each command, `lone_pipeline` for `$?`; `take_back` after each program; `run_input` feeding long lines to the scan |
| `crates/shell/src/expand.rs` | `expand` over a pipeline's commands; `words` for `for`; `Vars::positional` |
| `crates/shell/src/io.rs` | `Console::take_back` |
| `crates/shell/src/testing.rs` | The test console takes a Ctrl-C when it reports one, counts take-backs, and bounds the times it is asked |
| `crates/shell/src/commands/basic.rs` | `help`'s syntax lines |
| `crates/shell/tests/corpus/{if,loops,for}.sh` (new) | The corpus's compound commands |
| `crates/relay-rt/src/{sys,sysio}.rs` | `sys::take_ctrl_c`; `SysConsole::interrupted`, `SysConsole::take_back` |
| `crates/relay-abi/src/spawn.rs` | `WAIT_CTRL_C`'s comment: pid 0 |
| `kernel/src/syscall.rs`, `kernel/src/syscall/testing.rs`, `kernel/src/proc.rs` | `wait(0, WAIT_NOHANG \| WAIT_CTRL_C)`, `Caller::take_ctrl_c`; `console_input` |
| `kernel/src/input.rs`, `kernel/src/tty.rs` | `take_line_interrupt_for`, `tty::ctrl_c(alive)` |
| `tests/e2e/control.txt` | The nesting bound, Ctrl-C in loops (after a program too), constructs at the `> ` prompt, a script of nested loops |

---

## PR 1: The spec's §15 item 2, the roadmap's notes and this plan

The spec's §15 item 2, the decisions of this plan, with the parts of its body it corrects (the status line, §4.4, §4.5, §5.2, §10); the roadmap's notes on plan 1's deferred minors and rulings, the kernel's change, plan 3's `grep -q` and the qmp fix; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The spec's and roadmap's changes are the prototype's first commit, `docs(spec,roadmap): record milestone 4 plan 2's decisions`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p2/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m4p2/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-02-m4-plan-2-compound-commands.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-02-m4-plan-2-compound-commands.md
git commit -m "docs(plan): add milestone 4 plan 2, compound commands"
cargo xtask lint
git push -u origin m4p2/plan
gh pr create --base main --head m4p2/plan --title "docs(spec,plan): add milestone 4 plan 2, compound commands" --body-file - <<'EOF2'
## What

The implementation plan of milestone 4's plan 2 ("compound commands"), with the spec's §15 item 2: the tree (a pipeline runs simple commands or one compound command); keywords only where a command name stands, the rest of bash's reserved words refused; `for`'s header and name; bash's syntax errors; the refusals in a pipeline, before `&` or redirected; reading a construct a line at a time and dropping one to its end, refused syntax and here-documents read as bash reads them; the 32-level bound; statuses; Ctrl-C between the shell's own commands, with `wait(0, WAIT_NOHANG | WAIT_CTRL_C)` and a Ctrl-C kept for the console's next holder (ABI 3 unchanged); the trace; and the corrections they bring to §4.4, §4.5, §5.2 and §10. The roadmap's notes: plan 1's deferred minors and rulings settled, the kernel's change, `grep -q` for plan 3.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Wait for CI and hand over for review**

Wait until `lint`, `unit` and `e2e` are green (`gh pr checks m4p2/plan --watch`). Ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m4p2/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-plan` and continue with PR 2.

---

## PR 2: The parser read a line at a time, the drop to a construct's end, and `if` (Tasks 1–9)

A pipeline holds a `Run`; the parser keeps a command's state from one line to the next, so each line is read once, in its context; a command dropped before its end is dropped to the end of its constructs by a scan that reads refused syntax and here-documents as bash does; `if`, `elif` and `else` run, refused in a pipeline, before `&` or redirected; compound commands nest at most 32 levels deep. `while`, `until` and `for` stay refused until PR 3. With the review's fixes: the scan reading `$(…)`, backquotes, `time`, `{`, `select` and `case` as bash does, dropping no more than bash after a line in error, and dropping a refused here-document's body.

Branch `m4p2/if`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-if`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p2/if /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-if origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-if
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p2/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p2/if /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-if m4p2/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p2/plan>` and re-run `cargo xtask ci` before pushing.

### Task 1: A pipeline runs what its `Run` holds

Spec §4.1 and decision 1: a `Pipeline` holds its `!` and a `Run`, which for now is only `Run::Commands`, the simple commands joined by `|`; Task 7 adds `Run::Compound`, so that a compound command can never stand among other commands of a pipeline. `expand::expand` and `expand::plain` take a pipeline's commands (`&[Command<Word>]`) instead of the pipeline, and `Shell::run_pipeline` matches on the `Run` and hands the commands to `run_commands`. `parser::parse`, for the tests that run no shell (`relay-rt`'s too), keeps giving the first pipeline's commands. Nothing the shell does changes: every test passes once moved onto the new field, through a test-only `Pipeline::commands()`. The red run is the shell's tests, which cannot find `Pipeline::run`. The task is a refactor: its mutation checks are the next tasks'.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: plan 1's `parser::Pipeline { negated, commands }`, `expand::expand(&Pipeline<Word>, …)`, `expand::plain`, `Shell::run_pipeline`, `Shell::run_commands`.
- Produces: `parser::Pipeline<W> { negated: bool, run: Run<W> }`, `parser::Run<W>::Commands(Vec<Command<W>>)`; `expand::expand(&[Command<Word>], &Vars, i32) -> Result<Vec<Command>, Error>`, `expand::plain(&[Command<Word>])` (crate-private); `Shell::run_commands(&mut self, &[Command<Word>], Option<&str>) -> i32` (private); `Pipeline::commands(&self) -> &[Command<W>]` (tests only).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    /// The first pipeline of `line`, as typed.
    fn typed(line: &str) -> Pipeline<Word> {
        parse_line(line).unwrap().items.remove(0).and_or.first
    }

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        expand(&typed(line), vars, 3).map(|mut p| p.commands.remove(0).words)
    }
````

with:

````rust

    /// The commands of the first pipeline of `line`, as typed.
    fn typed(line: &str) -> Vec<Command<Word>> {
        let first = parse_line(line).unwrap().items.remove(0).and_or.first;
        first.commands().to_vec()
    }

    /// The words of `line`'s one command with `vars`, `$?` 3.
    fn words(line: &str, vars: &Vars) -> Result<Vec<String>, Error> {
        expand(&typed(line), vars, 3).map(|mut c| c.remove(0).words)
    }
````

Replace:

````rust
            let p = typed(line);
            let (_, value) = p.commands[0].words[0].assignment().unwrap();
            super::value(&value, &v, 0)
````

with:

````rust
            let p = typed(line);
            let (_, value) = p[0].words[0].assignment().unwrap();
            super::value(&value, &v, 0)
````

Replace:

````rust
        let target = |line: &str| {
            expand(&typed(line), &v, 0).map(|mut p| p.commands.remove(0).redirect.unwrap().path)
        };
````

with:

````rust
        let target = |line: &str| {
            expand(&typed(line), &v, 0).map(|mut c| c.remove(0).redirect.unwrap().path)
        };
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 8 replacements, top to bottom:

Replace:

````rust

    /// The commands of `line`'s one pipeline, as typed.
````

with:

````rust

    impl<W> Pipeline<W> {
        /// The commands of a pipeline that runs commands.
        pub fn commands(&self) -> &[Command<W>] {
            match &self.run {
                Run::Commands(c) => c,
            }
        }
    }

    /// The commands of `line`'s one pipeline, as typed.
````

Replace:

````rust
            .first
            .commands
    }
````

with:

````rust
            .first
            .commands()
            .to_vec()
    }
````

Replace:

````rust
            .first
            .commands
            .iter()
````

with:

````rust
            .first
            .commands()
            .iter()
````

Replace:

````rust
            .iter()
            .map(|i| i.and_or.first.commands[0].words[0].typed.as_str())
            .collect();
````

with:

````rust
            .iter()
            .map(|i| i.and_or.first.commands()[0].words[0].typed.as_str())
            .collect();
````

Replace:

````rust
        let ao = parse_line(line).unwrap().items.remove(0).and_or;
        let name = |p: &Pipeline<Word>| p.commands[0].words[0].typed.clone();
        let mut names = alloc::vec![name(&ao.first)];
````

with:

````rust
        let ao = parse_line(line).unwrap().items.remove(0).and_or;
        let name = |p: &Pipeline<Word>| p.commands()[0].words[0].typed.clone();
        let mut names = alloc::vec![name(&ao.first)];
````

Replace:

````rust
        assert!(p.negated);
        assert_eq!(p.commands.len(), 2);
        // Each `!` turns it again, as bash's does.
````

with:

````rust
        assert!(p.negated);
        assert_eq!(p.commands().len(), 2);
        // Each `!` turns it again, as bash's does.
````

Replace:

````rust
            let p = &item.and_or.first;
            assert!(p.negated && p.commands[0].words.is_empty());
        }
````

with:

````rust
            let p = &item.and_or.first;
            assert!(p.negated && p.commands()[0].words.is_empty());
        }
````

Replace:

````rust
                    ps.iter()
                        .flat_map(|p| p.commands.iter().map(|c| c.words[0].typed.clone()))
                        .collect()
````

with:

````rust
                    ps.iter()
                        .flat_map(|p| p.commands().iter().map(|c| c.words[0].typed.clone()))
                        .collect()
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, replace:

````rust
        for item in &list.items {
            names.push(item.and_or.first.commands[0].words[0].typed.as_str());
            for (_, p) in &item.and_or.rest {
                names.push(p.commands[0].words[0].typed.as_str());
            }
````

with:

````rust
        for item in &list.items {
            names.push(item.and_or.first.commands()[0].words[0].typed.as_str());
            for (_, p) in &item.and_or.rest {
                names.push(p.commands()[0].words[0].typed.as_str());
            }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Run` in this scope ``; `mismatched types`.

- [ ] **Step 5: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

use crate::parser::{Command, Param, Piece, Pipeline, Redirect, Word};
use alloc::borrow::Cow;
````

with:

````rust

use crate::parser::{Command, Param, Piece, Redirect, Word};
use alloc::borrow::Cow;
````

Replace:

````rust
pub(crate) fn expand(
    pipeline: &Pipeline<Word>,
    vars: &Vars,
    status: i32,
) -> Result<Pipeline, Error> {
    let mut x = Expander::new(vars, status);
    let mut commands = Vec::new();
    for c in &pipeline.commands {
        commands.push(x.command(c)?);
    }
    Ok(Pipeline {
        negated: pipeline.negated,
        commands,
    })
}
````

with:

````rust
pub(crate) fn expand(
    commands: &[Command<Word>],
    vars: &Vars,
    status: i32,
) -> Result<Vec<Command>, Error> {
    let mut x = Expander::new(vars, status);
    commands.iter().map(|c| x.command(c)).collect()
}
````

Replace:

````rust

/// The words of `pipeline` with nothing set: for callers that run no
/// shell (tests).
pub(crate) fn plain(pipeline: &Pipeline<Word>) -> Result<Pipeline, Error> {
    expand(pipeline, &Vars::new(crate::shell::NAME), 0)
}
````

with:

````rust

/// The words of a pipeline's `commands` with nothing set: for callers
/// that run no shell (tests).
pub(crate) fn plain(commands: &[Command<Word>]) -> Result<Vec<Command>, Error> {
    expand(commands, &Vars::new(crate::shell::NAME), 0)
}
````

- [ ] **Step 6: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

/// One command, or several joined by `|`, each one's output the next
/// one's input; after a `!`, its status negated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline<W = String> {
    pub negated: bool,
    pub commands: Vec<Command<W>>,
}
````

with:

````rust

/// What a pipeline runs and, after a `!`, its status negated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline<W = String> {
    pub negated: bool,
    pub run: Run<W>,
}

/// What a pipeline runs (programmable shell gate §4.1): one command, or
/// several joined by `|`, each one's output the next one's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run<W = String> {
    Commands(Vec<Command<W>>),
}
````

Replace:

````rust
    };
    match crate::expand::plain(&item.and_or.first) {
        Ok(p) => Ok(p.commands),
        Err(e) => Err(ParseError::Expansion(e.to_string())),
    }
}
````

with:

````rust
    };
    let Run::Commands(commands) = &item.and_or.first.run;
    crate::expand::plain(commands).map_err(|e| ParseError::Expansion(e.to_string()))
}
````

Replace:

````rust
        negated: p.bangs % 2 == 1,
        commands: core::mem::take(pipeline),
    }))
````

with:

````rust
        negated: p.bangs % 2 == 1,
        run: Run::Commands(core::mem::take(pipeline)),
    }))
````

- [ ] **Step 7: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    ) -> i32 {
        let status = self.run_commands(typed, background);
        if !typed.negated || background.is_some() || self.stopped || self.abandoned {
````

with:

````rust
    ) -> i32 {
        let status = match &typed.run {
            parser::Run::Commands(commands) => self.run_commands(commands, background),
        };
        if !typed.negated || background.is_some() || self.stopped || self.abandoned {
````

Replace:

````rust
        &mut self,
        typed: &parser::Pipeline<parser::Word>,
        background: Option<&str>,
    ) -> i32 {
        let assigns = typed
            .commands
            .iter()
            .find_map(|c| c.words.first().filter(|w| w.assignment().is_some()));
        if let Some(first) = assigns {
            // Alone on its line; bash's changes nothing elsewhere.
            let place = match (background, typed.commands.len()) {
                (Some(_), _) => "the background",
                (None, 1) => return self.assign(&typed.commands[0]),
                (None, _) => "a pipeline",
````

with:

````rust
        &mut self,
        commands: &[parser::Command<parser::Word>],
        background: Option<&str>,
    ) -> i32 {
        let assigns = commands
            .iter()
            .find_map(|c| c.words.first().filter(|w| w.assignment().is_some()));
        if let Some(first) = assigns {
            // Alone on its line; bash's changes nothing elsewhere.
            let place = match (background, commands.len()) {
                (Some(_), _) => "the background",
                (None, 1) => return self.assign(&commands[0]),
                (None, _) => "a pipeline",
````

Replace:

````rust
        }
        let mut pipeline = match expand::expand(typed, &self.vars, self.status) {
            Ok(p) => match background {
                Some(text) => return self.background(&p.commands, text),
                None => p.commands,
            },
            Err(e) => {
                let alone = typed.commands.len() == 1 && background.is_none();
                return self.not_expanded(e, alone);
````

with:

````rust
        }
        let mut pipeline = match expand::expand(commands, &self.vars, self.status) {
            Ok(p) => match background {
                Some(text) => return self.background(&p, text),
                None => p,
            },
            Err(e) => {
                let alone = commands.len() == 1 && background.is_none();
                return self.not_expanded(e, alone);
````

- [ ] **Step 8: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 317 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell): let a pipeline run what its Run holds

A pipeline now holds its `!` and a `Run`, which is, for now, simple
commands joined by `|`; plan 2's compound commands join it as a
variant of their own (programmable shell gate §15 item 2). Expansion
takes a pipeline's commands instead of the pipeline.
EOF
````


### Task 2: The parser keeps a command's state from one line to the next

Spec §4.3, §4.5 and decision 6: everything `parse_line`'s loop kept moves into `parser::Parser` (the text read so far, the items, the pipeline, the parts and the comments), and `Parser::line` reads one line and its newline, from where it left off: it gives the list when the line finishes the command and starts afresh, or `None` while the command goes on. `parse_line` reads a whole text through one. The `Reader` gives each line to its parser once, so reading a command stays linear without plan 1's judging of a line alone, and an error is told on the line that has it: `a |` then `! b |` was told a line late, and `a &&` then `b & c &&` as the end of the file (plan 1's deferred minor). Dropping a command to its end still judges a line alone until Task 3. A test-only count of the bytes read (`Parser::read`) replaces plan 1's count of whole parses. The red run is the shell's tests: the deferred minor's lines are told late, and the counter is not there.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`

**Interfaces:**
- Consumes: Task 1's tree; plan 1's `parse_line`, `Reader::add`, `Cursor`.
- Produces: `parser::Parser` with `new()`, `len()`, `is_empty()`, `line(&mut self, &str) -> Result<Option<List<Word>>, ParseError>`, and (private) `read`, `read_text`, `end`; `Cursor::starting(&str, from: usize)` (private); `Parser::read: usize` (tests only); `Reader.parser` (crate-private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    #[test]
    fn the_text_is_parsed_whole_only_when_a_line_may_finish_it() {
        // The review found each line parsing all the text before it: 64 KiB
        // of `a |` lines took over a minute. A line that alone leaves the
        // command unfinished, or is blank or a comment, needs no such parse.
        let mut r = Reader::new();
````

with:

````rust
    #[test]
    fn each_line_is_read_once() {
        // Plan 1's review found each line parsing all the text before it:
        // 64 KiB of `a |` lines took over a minute. The parser keeps what
        // it has read, so each line is read once.
        let mut r = Reader::new();
````

Replace:

````rust
        }
        assert_eq!(names(&r.add("b").unwrap().unwrap()).len(), 1);
        assert_eq!(r.whole_parses, 2, "the first line's, and the last's");
        // A line that might finish it, or be wrong, is parsed with the rest.
        assert_eq!(r.add("a &&"), Ok(None));
        assert_eq!(r.add("|| b"), Err(ParseError::MissingTarget("||")));
    }
````

with:

````rust
        }
        assert_eq!(r.parser.read, 1000 * ("a |\n\n  # c\n".len()));
        assert_eq!(names(&r.add("b").unwrap().unwrap()).len(), 1);
    }

    #[test]
    fn an_error_is_told_on_the_line_that_has_it() {
        // Plan 1 judged a line alone, and told these a line late or as
        // the end of the file.
        let mut r = Reader::new();
        assert_eq!(r.add("a |"), Ok(None));
        assert_eq!(r.add("! b |"), Err(ParseError::MissingTarget("!")));
        let mut r = Reader::new();
        assert_eq!(r.add("a &&"), Ok(None));
        assert_eq!(
            r.add("b & c &&"),
            Err(ParseError::Unsupported("& after &&".into()))
        );
    }
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no field `parser` on type `reader::Reader` ``.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    line: &'l str,
    chars: Peekable<CharIndices<'l>>,
}

impl<'l> Cursor<'l> {
    fn new(line: &'l str) -> Cursor<'l> {
        Cursor {
            line,
            chars: line.char_indices().peekable(),
        }
````

with:

````rust
    line: &'l str,
    /// Where `chars` starts in `line`.
    from: usize,
    chars: Peekable<CharIndices<'l>>,
}

impl<'l> Cursor<'l> {
    /// From byte `from` of `line` on.
    fn starting(line: &'l str, from: usize) -> Cursor<'l> {
        Cursor {
            line,
            from,
            chars: line[from..].char_indices().peekable(),
        }
````

Replace:

````rust
    fn pos(&mut self) -> usize {
        self.chars.peek().map_or(self.line.len(), |&(i, _)| i)
    }
````

with:

````rust
    fn pos(&mut self) -> usize {
        self.chars
            .peek()
            .map_or(self.line.len(), |&(i, _)| self.from + i)
    }
````

Replace:

````rust

/// `line`'s list, its words as typed. `;` ends an item, and `&&` and `||`
/// join pipelines, as bash's do; one with nothing typed before it is
/// bash's syntax error naming it.
pub fn parse_line(line: &str) -> Result<List<Word>, ParseError> {
    let mut items = Items::default();
    // Where the item being read starts in the line.
    let mut item_start = 0;
    // Where each comment starts and ends in the line.
    let mut comments = Vec::new();
    let mut pipeline = Vec::new();
    let mut parts = Parts::default();
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
                }
                parts.pending = Some(append);
            }
            '|' if cur.next_if_eq('|') => {
                parts.end_word(&mut word, line)?;
                join(&mut items, &mut parts, &mut pipeline, Connector::Or)?;
            }
            '|' => {
                // bash's `|&` pipes the errors too.
                if cur.next_if_eq('&') {
                    return Err(ParseError::Unsupported("|&".into()));
                }
                parts.end_word(&mut word, line)?;
                pipeline.push(parts.take_before_pipe()?);
            }
            ';' => {
                // bash's `;;`, `;&` and `;;&` end a `case` branch.
                if cur.next_if_eq(';') {
                    let token = if cur.next_if_eq('&') { ";;&" } else { ";;" };
                    return Err(ParseError::MissingTarget(token));
                }
                if cur.next_if_eq('&') {
                    return Err(ParseError::MissingTarget(";&"));
                }
                parts.end_word(&mut word, line)?;
                match end_pipeline(&mut parts, &mut pipeline, ";")? {
                    Some(p) => items.pipeline(p),
                    None => return Err(ParseError::MissingTarget(";")),
                }
                items.end(None);
                item_start = cur.pos();
            }
            '&' if cur.next_if_eq('&') => {
                parts.end_word(&mut word, line)?;
                join(&mut items, &mut parts, &mut pipeline, Connector::And)?;
            }
            '&' => {
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none() {
                    return Err(ParseError::MissingTarget("&"));
                }
                if parts.words.is_empty() {
                    // `> f &`: a background job is a program.
                    return Err(ParseError::Unsupported("> &".into()));
                }
                // bash runs the whole and-or list in the background, in a
                // shell of its own.
                if let Some(c) = items.connector {
                    return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                }
                // Without its `!`, as bash's `jobs` shows it.
                let typed = job_text(line, item_start, at, &comments);
                let mut text = typed.as_str();
                for _ in 0..parts.bangs {
                    text = text[1..].trim_start_matches([' ', '\t']);
                }
                let text = String::from(text);
                if let Some(p) = end_pipeline(&mut parts, &mut pipeline, "&")? {
                    items.pipeline(p);
                }
                // The line goes on after it, as bash's does (`a & b`).
                items.end(Some(text));
                item_start = cur.pos();
            }
            '\'' => {
                let before = word.open_quote();
                loop {
                    match cur.next() {
                        Some('\'') => break,
                        Some(c) => word.quoted(c),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
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
            // bash joins a line ending in `\` to the next; here, as before
            // a command could go on to another line, it is an error.
            '\\' => match cur.next() {
                Some('\n') | None => return Err(ParseError::TrailingBackslash),
                Some(c) => word.quoted(c),
            },
            '$' => match parameter(&mut cur, false)? {
                Some(p) => word.param(p, false),
                None => {
                    word.started = true;
                    word.added += 1;
                    word.word.push('$', false);
                }
            },
            // A comment runs to the end of the line.
            '#' if !word.started => {
                while cur.peek().is_some_and(|c| c != '\n') {
                    cur.next();
                }
                comments.push((at, cur.pos()));
            }
            '\n' => {
                parts.end_word(&mut word, line)?;
                if parts.pending.is_some() {
                    return Err(ParseError::MissingTarget("newline"));
                }
                // After `|`, `&&` or `||` the command goes on, past blank
                // and comment lines, as bash's does.
                // (A `!` counts only before a pipeline's first command.)
                let nothing = parts.words.is_empty()
                    && parts.redirect.is_none()
                    && (parts.bangs == 0 || !pipeline.is_empty());
                if nothing && (!pipeline.is_empty() || items.connector.is_some()) {
                    continue;
                }
                if let Some(p) = end_pipeline(&mut parts, &mut pipeline, "newline")? {
                    items.pipeline(p);
                }
                items.end(None);
                item_start = cur.pos();
            }
            c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
            c => {
                if !word.started && c == '~' {
                    word.tilde = true;
                }
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
        return Err(ParseError::MissingTarget("newline"));
    }
    if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
        return Err(ParseError::Incomplete);
    }
    // Nothing after the last `;` (or at all) is no item.
    match end_pipeline(&mut parts, &mut pipeline, "newline")? {
        Some(p) => items.pipeline(p),
        None if items.connector.is_some() => return Err(ParseError::Incomplete),
        None => {}
    }
    items.end(None);
    Ok(List { items: items.items })
}
````

with:

````rust

/// A command's text being parsed (programmable shell gate §4.3): what has
/// been read of it, kept from one call to the next, so that a reader can
/// give it a command's lines one at a time and each one is read once, in
/// its context.
#[derive(Default)]
pub struct Parser {
    /// The text read so far, which words and a background job's text are
    /// cut from.
    text: String,
    items: Items,
    /// Where the item being read starts in the text.
    item_start: usize,
    /// Where each comment starts and ends in the text.
    comments: Vec<(usize, usize)>,
    pipeline: Vec<Command<Word>>,
    parts: Parts,
    word: Building,
    /// How many bytes it has read (the tests count them).
    #[cfg(test)]
    pub(crate) read: usize,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// How long the text read so far is.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Nothing has been read.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Reads `line` and its newline: the list the text makes if the line
    /// finishes it, none while it needs more lines (it ends after `|`,
    /// `&&` or `||`), or why it does not parse. Once it gives a list the
    /// parser starts afresh.
    pub fn line(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        let from = self.text.len();
        self.text.push_str(line);
        self.text.push('\n');
        self.read(from)?;
        if !self.pipeline.is_empty() || self.items.connector.is_some() {
            return Ok(None);
        }
        let items = core::mem::take(&mut self.items.items);
        *self = Parser::new();
        Ok(Some(List { items }))
    }

    /// Reads the text from `from` on.
    fn read(&mut self, from: usize) -> Result<(), ParseError> {
        let text = core::mem::take(&mut self.text);
        #[cfg(test)]
        {
            self.read += text.len() - from;
        }
        let read = self.read_text(&text, from);
        self.text = text;
        read
    }

    /// `;` ends an item, and `&&` and `||` join pipelines, as bash's do;
    /// one with nothing typed before it is bash's syntax error naming it.
    fn read_text(&mut self, line: &str, from: usize) -> Result<(), ParseError> {
        let Parser {
            items,
            item_start,
            comments,
            pipeline,
            parts,
            word,
            ..
        } = self;
        let mut cur = Cursor::starting(line, from);
        loop {
            let at = cur.pos();
            let Some(c) = cur.next() else {
                break;
            };
            match c {
                ' ' | '\t' => parts.end_word(word, line)?,
                '>' => {
                    // `2>` redirects another stream in a real shell.
                    if word.started
                        && let Some(digits) = word.word.digits()
                    {
                        return Err(ParseError::Unsupported(format!("{digits}>")));
                    }
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() {
                        return Err(ParseError::MissingTarget(">"));
                    }
                    let append = cur.next_if_eq('>');
                    // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                    // `> &` are its syntax errors.
                    if !append && cur.peek() == Some('&') {
                        return Err(ParseError::Unsupported(">&".into()));
                    }
                    parts.pending = Some(append);
                }
                '|' if cur.next_if_eq('|') => {
                    parts.end_word(word, line)?;
                    join(items, parts, pipeline, Connector::Or)?;
                }
                '|' => {
                    // bash's `|&` pipes the errors too.
                    if cur.next_if_eq('&') {
                        return Err(ParseError::Unsupported("|&".into()));
                    }
                    parts.end_word(word, line)?;
                    pipeline.push(parts.take_before_pipe()?);
                }
                ';' => {
                    // bash's `;;`, `;&` and `;;&` end a `case` branch.
                    if cur.next_if_eq(';') {
                        let token = if cur.next_if_eq('&') { ";;&" } else { ";;" };
                        return Err(ParseError::MissingTarget(token));
                    }
                    if cur.next_if_eq('&') {
                        return Err(ParseError::MissingTarget(";&"));
                    }
                    parts.end_word(word, line)?;
                    match end_pipeline(parts, pipeline, ";")? {
                        Some(p) => items.pipeline(p),
                        None => return Err(ParseError::MissingTarget(";")),
                    }
                    items.end(None);
                    *item_start = cur.pos();
                }
                '&' if cur.next_if_eq('&') => {
                    parts.end_word(word, line)?;
                    join(items, parts, pipeline, Connector::And)?;
                }
                '&' => {
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none()
                    {
                        return Err(ParseError::MissingTarget("&"));
                    }
                    if parts.words.is_empty() {
                        // `> f &`: a background job is a program.
                        return Err(ParseError::Unsupported("> &".into()));
                    }
                    // bash runs the whole and-or list in the background, in a
                    // shell of its own.
                    if let Some(c) = items.connector {
                        return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                    }
                    // Without its `!`, as bash's `jobs` shows it.
                    let typed = job_text(line, *item_start, at, comments);
                    let mut text = typed.as_str();
                    for _ in 0..parts.bangs {
                        text = text[1..].trim_start_matches([' ', '\t']);
                    }
                    let text = String::from(text);
                    if let Some(p) = end_pipeline(parts, pipeline, "&")? {
                        items.pipeline(p);
                    }
                    // The line goes on after it, as bash's does (`a & b`).
                    items.end(Some(text));
                    *item_start = cur.pos();
                }
                '\'' => {
                    let before = word.open_quote();
                    loop {
                        match cur.next() {
                            Some('\'') => break,
                            Some(c) => word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
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
                // bash joins a line ending in `\` to the next; here, as before
                // a command could go on to another line, it is an error.
                '\\' => match cur.next() {
                    Some('\n') | None => return Err(ParseError::TrailingBackslash),
                    Some(c) => word.quoted(c),
                },
                '$' => match parameter(&mut cur, false)? {
                    Some(p) => word.param(p, false),
                    None => {
                        word.started = true;
                        word.added += 1;
                        word.word.push('$', false);
                    }
                },
                // A comment runs to the end of the line.
                '#' if !word.started => {
                    while cur.peek().is_some_and(|c| c != '\n') {
                        cur.next();
                    }
                    comments.push((at, cur.pos()));
                }
                '\n' => {
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("newline"));
                    }
                    // After `|`, `&&` or `||` the command goes on, past blank
                    // and comment lines, as bash's does.
                    // (A `!` counts only before a pipeline's first command.)
                    let nothing = parts.words.is_empty()
                        && parts.redirect.is_none()
                        && (parts.bangs == 0 || !pipeline.is_empty());
                    if nothing && (!pipeline.is_empty() || items.connector.is_some()) {
                        continue;
                    }
                    if let Some(p) = end_pipeline(parts, pipeline, "newline")? {
                        items.pipeline(p);
                    }
                    items.end(None);
                    *item_start = cur.pos();
                }
                c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
                c => {
                    if !word.started && c == '~' {
                        word.tilde = true;
                    }
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
        Ok(())
    }

    /// The text ends: its list, or why it does not parse (a command that
    /// needs more lines is `ParseError::Incomplete`).
    fn end(mut self) -> Result<List<Word>, ParseError> {
        let Parser {
            text,
            items,
            pipeline,
            parts,
            word,
            ..
        } = &mut self;
        parts.end_word(word, text)?;
        if parts.pending.is_some() {
            return Err(ParseError::MissingTarget("newline"));
        }
        if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
            return Err(ParseError::Incomplete);
        }
        // Nothing after the last `;` (or at all) is no item.
        match end_pipeline(parts, pipeline, "newline")? {
            Some(p) => items.pipeline(p),
            None if items.connector.is_some() => return Err(ParseError::Incomplete),
            None => {}
        }
        items.end(None);
        Ok(List {
            items: core::mem::take(&mut items.items),
        })
    }
}

/// `line`'s list, its words as typed: the text read whole by a
/// [`Parser`].
pub fn parse_line(line: &str) -> Result<List<Word>, ParseError> {
    let mut parser = Parser {
        text: String::from(line),
        ..Parser::default()
    };
    parser.read(0)?;
    parser.end()
}
````

- [ ] **Step 4: Change `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, make these 6 replacements, top to bottom:

Replace:

````rust

use crate::parser::{self, COMMAND_MAX, List, ParseError, Word};
use alloc::string::String;

/// What has been read of the command so far.
#[derive(Default)]
pub(crate) struct Reader {
    text: String,
    /// A command was dropped before its end: its later lines are dropped
    /// too, up to one that finishes it, so that none of them runs without
    /// what came before (the end of an `&&` chain without its guard).
    dropping: bool,
    /// How many times all the text was parsed (the tests count them).
    #[cfg(test)]
    whole_parses: usize,
}
````

with:

````rust

use crate::parser::{self, COMMAND_MAX, List, ParseError, Parser, Word};

/// What has been read of the command so far.
#[derive(Default)]
pub(crate) struct Reader {
    /// What has been read of the command, kept between its lines.
    parser: Parser,
    /// A command was dropped before its end: its later lines are dropped
    /// too, up to one that finishes it, so that none of them runs without
    /// what came before (the end of an `&&` chain without its guard).
    dropping: bool,
}
````

Replace:

````rust
        }
        if self.text.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line);
            return Err(ParseError::TooLong);
        }
        // While a command goes on, a line that alone would leave it
        // unfinished (ending after `|`, `&&` or `||`), or holds nothing,
        // leaves it unfinished: so each line is parsed once alone, and all
        // the text only when a line may finish it, which keeps reading a
        // long command linear.
        let more = self.reading() && matches!(alone(line), Alone::GoesOn | Alone::Nothing);
        self.text.push_str(line);
        self.text.push('\n');
        if more {
            return Ok(None);
        }
        #[cfg(test)]
        {
            self.whole_parses += 1;
        }
        match parser::parse_line(&self.text) {
            Err(ParseError::Incomplete) => Ok(None),
            Err(e) => {
                // A line refused while it leaves the command open drops the
                // rest of the command with it, as `drop_line` does.
                self.text.clear();
                self.dropping = parser::ends_open(line);
                Err(e)
            }
            Ok(list) => {
                self.text.clear();
                Ok(Some(list))
            }
        }
    }
````

with:

````rust
        }
        if self.parser.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line);
            return Err(ParseError::TooLong);
        }
        // The parser keeps what it has read, so each line is read once, in
        // its context.
        self.parser.line(line).inspect_err(|_| {
            // A line refused while it leaves the command open drops the
            // rest of the command with it, as `drop_line` does.
            self.parser = Parser::new();
            self.dropping = parser::ends_open(line);
        })
    }
````

Replace:

````rust
        let reading = self.reading();
        self.text.clear();
        // What does not parse alone might go on (a line's last bytes may
````

with:

````rust
        let reading = self.reading();
        self.parser = Parser::new();
        // What does not parse alone might go on (a line's last bytes may
````

Replace:

````rust
    pub fn reading(&self) -> bool {
        !self.text.is_empty() || self.dropping
    }
````

with:

````rust
    pub fn reading(&self) -> bool {
        !self.parser.is_empty() || self.dropping
    }
````

Replace:

````rust
    pub fn end(&mut self) -> Option<ParseError> {
        let reading = !self.text.is_empty();
        self.clear();
````

with:

````rust
    pub fn end(&mut self) -> Option<ParseError> {
        let reading = !self.parser.is_empty();
        self.clear();
````

Replace:

````rust
    pub fn clear(&mut self) {
        self.text.clear();
        self.dropping = false;
````

with:

````rust
    pub fn clear(&mut self) {
        self.parser = Parser::new();
        self.dropping = false;
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 318 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read a command's lines once each, in their context

The parser keeps what it has read of a command between calls, so the
reader gives it each line once instead of judging a line alone and
parsing the whole text when the line might finish it. Reading stays
linear, and an error is told on the line that has it: `a |` then
`! b |` was told a line late (plan 1's deferred minor, programmable
shell gate §15 item 2).
EOF
````


### Task 3: A dropped command is dropped to the end of its constructs

Spec §4.4, §4.5 and decision 6: a command dropped before its end (a line that does not parse, past 64 KiB, a line `X | sh` cannot read) is dropped by a scan (`crates/shell/src/scan.rs`) that never fails: it reads quotes, escapes, comments and `${…}` as the parser does, counts each `if`, `while`, `until` and `for` and each `fi` and `done` where a command name would stand (not as an argument, quoted, after a redirection, or as a `for`'s name or words), and notes a line that ends after `|`, `&&` or `||`. The `Reader` runs it over the text read so far, the failing line included, then over each later line, and stops dropping after a line once the count is 0 and nothing is left open. `Reader::drop_bytes` and `drop_end` take a line a piece at a time, so `X | sh` counts a line too long to keep whole as it reads it, instead of plan 1's judging by its last kilobyte or two (`LINE_TAIL` goes, and with it that ruling's command lost). `parser::ends_open` and the line-alone judgment go too. So a script's `if` written across lines no longer runs its body while `if` is refused (plan 1's final ruling): its lines are dropped up to its `fi`. The red run is the shell's tests, which cannot find `scan::Scan`'s fields or the new `Reader` calls. Mutation checks: each opener, closer and keyword that keeps a command name's place left out, the command-name and `for` states, quotes, escapes, `${…}`, comments, `|`, `&&`, the newline's resets and `done()`'s two halves each broken, and each `Reader` call dropped, fail a test; four survivors of the first tests named redundant code, which went.

**Files:**
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Create: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 2's `Parser` (and its new `text()`); plan 1's `Reader` and `run_input`.
- Produces: `scan::Scan` with `new()`, `bytes(&[u8])`, `line(&[u8])`, `done(&self) -> bool` (crate-private); `Reader::drop_line(&mut self, &[u8])`, `Reader::drop_bytes(&mut self, &[u8])`, `Reader::drop_end(&mut self)` (crate-private); `Reader.dropping: Option<Scan>`; `Parser::text(&self) -> &str`.

- [ ] **Step 1: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod runner;
mod shell;
````

with:

````rust
mod runner;
mod scan;
mod shell;
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, replace:

````rust
        let mut r = Reader::new();
        // Its text decides, or what is left of it: one that plainly
        // finishes a command ends it; one that goes on, or might, does not.
        r.add("a &&").unwrap();
        r.drop_line("x");
        assert!(!r.reading());
        r.drop_line("x &&");
        assert!(r.reading());
        r.add("b").unwrap();
        assert!(!r.reading());
        r.drop_line("x' &&");
        assert!(r.reading(), "a quote cut in two: it might go on");
        r.clear();
        // A blank one ends nothing, and starts nothing.
        r.add("a ||").unwrap();
        r.drop_line("   ");
        assert!(r.reading());
        r.clear();
        r.drop_line("   ");
        assert!(!r.reading());
````

with:

````rust
        let mut r = Reader::new();
        // Its bytes decide, all of them: one that finishes a command ends
        // it; one that goes on does not.
        r.add("a &&").unwrap();
        r.drop_line(b"x");
        assert!(!r.reading());
        r.drop_line(b"x \xff &&");
        assert!(r.reading());
        r.add("b").unwrap();
        assert!(!r.reading());
        r.drop_line(b"x' &&");
        assert!(!r.reading(), "the quote is the line's, left open");
        // A blank one ends nothing, and starts nothing.
        r.add("a ||").unwrap();
        r.drop_line(b"   ");
        assert!(r.reading());
        r.clear();
        r.drop_line(b"   ");
        assert!(!r.reading());
        // One that opens a construct drops it to its end.
        r.drop_line(b"while \xff; do");
        for line in ["echo a", "if b; then", "fi"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
        }
        assert!(r.reading());
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
    }

    #[test]
    fn a_line_too_long_to_keep_is_counted_whole() {
        // Plan 1 judged such a line by its last bytes, and dropped the
        // next command to be safe when they did not parse alone.
        let mut r = Reader::new();
        r.drop_bytes(b"if a; then ");
        for _ in 0..100 {
            r.drop_bytes(&[b'x'; 1024]);
        }
        r.drop_bytes(b" '");
        assert!(r.reading());
        r.drop_end();
        assert!(r.reading(), "inside its `if`");
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
        r.drop_bytes(&[b'x'; 70 * 1024]);
        r.drop_bytes(b" '");
        r.drop_end();
        assert!(!r.reading(), "a quote left open ends with its line");
        assert_eq!(names(&r.add("b").unwrap().unwrap()), ["b"]);
    }

    #[test]
    fn a_refused_construct_is_dropped_to_its_end() {
        // Plan 1 dropped each refused line alone, so a script's `if`
        // written across lines ran its body (its final review's ruling):
        // its lines are dropped up to its `fi`, counting those inside.
        let mut r = Reader::new();
        assert_eq!(r.add("if true"), Err(ParseError::Unsupported("if".into())));
        assert!(r.reading());
        for line in ["then echo a", "while b", "do c", "done", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
        assert_eq!(names(&r.add("echo e").unwrap().unwrap()), ["echo"]);
        // Opened on a later line of the command, and refused there.
        assert_eq!(r.add("a &&"), Ok(None));
        assert!(r.add("for x in b; do").is_err());
        assert_eq!(r.add("echo $x"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
````

- [ ] **Step 3: Write the failing tests for `crates/shell/src/scan.rs`**

Create `crates/shell/src/scan.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;

    /// How many constructs `lines`, each with its newline, leave open, and
    /// whether the last ends after `|`, `&&` or `||`.
    fn after(lines: &[&str]) -> (usize, bool) {
        let mut s = Scan::new();
        for l in lines {
            s.line(l.as_bytes());
        }
        (s.depth, s.open)
    }

    #[test]
    fn openers_and_closers_count_where_a_command_name_stands() {
        for (lines, depth) in [
            (&["if a; then b; fi"][..], 0),
            (&["if a; then"], 1),
            (&["while a", "do if b", "then c"], 2),
            (&["until a; do b; done; for x in y; do"], 1),
            (&["a && while b; do"], 1),
            (&["a || until b"], 1),
            (&["a & if b"], 1),
            (&["! if a; then"], 1),
            (&["if a; then b; else if c; then"], 2),
            (&["if a; then b; elif c; then while d"], 2),
            (&["if a; then b; elif while c; do d; done; then"], 1),
            // After `fi` or `done` a keyword still stands there.
            (&["if a; then if b; then c; fi fi"], 0),
            (&["while a; do if b; then c; fi done"], 0),
            // A closer with nothing open counts nothing.
            (&["done", "fi", "if a; then"], 1),
            // Blanks and tabs around them.
            (&["\tif  a ;then"], 1),
            // A `#` inside a word, a redirection without its target, and a
            // comment, end nothing on the next line.
            (&["echo a#b; if c"], 1),
            (&["a >; if b"], 1),
            (&["a >", "if b"], 1),
            (&["# c", "if a"], 1),
            (&["echo a", "if b"], 1),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
    }

    #[test]
    fn a_keyword_elsewhere_or_quoted_counts_nothing() {
        for line in [
            "echo if while until for",
            "echo a; echo fi done",
            "'if' a",
            "\"while\" a",
            "\\if a",
            "i\\f a",
            "if'' a",
            "ifx a",
            "x=if",
            "> if a",
            "> f while a",
            "a > while",
            "a >> until",
            "echo a # if",
            "#if a",
            "echo ${if}",
            "echo \"$a\" if",
        ] {
            assert_eq!(after(&[line]), (0, false), "{line}");
        }
    }

    #[test]
    fn a_for_s_name_and_words_are_no_keywords() {
        for (lines, depth) in [
            (&["for x in if while until; do"][..], 1),
            (&["for in in in; do"], 1),
            (&["for done in a; do"], 1),
            (&["for if do"], 1),
            (&["for x do"], 1),
            (&["for x do if a"], 2),
            (&["for x", "do"], 1),
            (&["for x", "in a b", "do"], 1),
            (&["for x in a b do done", "done"], 0),
            (&["for x; do echo $x; done"], 0),
            (&["for x in a; do for y in b; do"], 2),
            // A `for` alone, which the parser refuses, still opens one:
            // dropped to its `done`, to be safe.
            (&["for"], 1),
            (&["for", "x in a; do b", "done"], 0),
            // A word after the name that is neither makes the rest words.
            (&["for x a if b"], 1),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
    }

    #[test]
    fn a_line_ending_after_a_pipe_or_connector_leaves_it_open() {
        for (lines, open) in [
            (&["a |"][..], true),
            (&["a &&"], true),
            (&["a ||"], true),
            (&["a | # c"], true),
            (&["a |", "", "  # c"], true),
            (&["if a; then b &&"], true),
            (&["a |", "b"], false),
            (&["a |& b"], false),
            (&["a |&"], false),
            (&["a ||b"], false),
            (&["a && b"], false),
            (&["a \\|"], false),
            (&["echo '&&'"], false),
            (&["echo \"a ||\""], false),
            (&["a # &&"], false),
            (&["a &"], false),
            (&["a ;"], false),
        ] {
            assert_eq!(after(lines).1, open, "{lines:?}");
        }
    }

    #[test]
    fn quotes_and_escapes_end_with_their_line() {
        // As the parser reads them: a quote left open is that line's error,
        // and the next line starts afresh.
        assert_eq!(after(&["echo 'a", "if b"]), (1, false));
        assert_eq!(after(&["echo \"a", "while b"]), (1, false));
        assert_eq!(after(&["a \\", "until b"]), (1, false));
        assert_eq!(after(&["echo ${a", "if b"]), (1, false));
        // Inside a quote, a `"`'s `\` takes the next character.
        assert_eq!(after(&["echo \"a\\\" if\" b; if c"]), (1, false));
        // `${…}` is read whole, its quotes, escapes and `${…}` too.
        assert_eq!(after(&["echo ${a:-'}'} ; if b"]), (1, false));
        assert_eq!(after(&["echo ${x;if}; if c"]), (1, false));
        assert_eq!(after(&["echo ${a:-${b}; if x}; if c"]), (1, false));
        assert_eq!(after(&["echo ${a:-\\}; if x}; if c"]), (1, false));
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
        let mut s = Scan::new();
        s.bytes(b"wh");
        s.bytes(b"ile a; do");
        s.bytes(b" \xff\xfe |\n");
        assert_eq!((s.depth, s.open), (1, true));
        assert!(!s.done());
        s.line(b"b; done");
        assert!(s.done());
    }
}
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(piped(&text), ["next"]);
    }
````

with:

````rust
        assert_eq!(piped(&text), ["next"]);
    }

    #[test]
    fn a_refused_construct_runs_none_of_its_lines() {
        // Plan 1's final review ruled that a script's `if` written across
        // lines ran its body, each refused line dropped alone: the lines
        // are dropped up to its `fi`, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo t-args d\ndone\nt-args e\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
        // Opened by a line too long, or no text.
        let mut text = b"while t-args a; do ".to_vec();
        text.extend(alloc::vec![b'x'; 70_000]);
        text.extend_from_slice(b"\nt-args b\ndone\nt-args next\n");
        assert_eq!(piped(&text), ["next"]);
        assert_eq!(
            piped(b"for x in \xff; do\nt-args b\ndone\nt-args next\n"),
            ["next"]
        );
        // In a script, each line traced as it is read.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"if true\nthen echo a\nwhile true\ndo echo b\ndone\nfi\necho next\n",
        );
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (
                0,
                "+ if true\nrelay-sh: unsupported syntax: if\n+ then echo a\n+ while true\n\
                 + do echo b\n+ done\n+ fi\n+ echo next\nnext\n"
                    .into()
            )
        );
        // At the prompt, with `> ` until its end.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &["if t-args a", "then t-args b", "fi", "t-args next"],
        );
        let args: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(args, ["next"]);
        assert_eq!(out.matches("\n> ").count(), 2, "{out}");
    }
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Scan` in this scope ``; `mismatched types`.

- [ ] **Step 6: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Reads `line` and its newline: the list the text makes if the line
````

with:

````rust

    /// The text read so far, with the newline of each line.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Reads `line` and its newline: the list the text makes if the line
````

Replace:

````rust
    parser.end()
}

/// Whether `line`, read as the parser reads quotes, escapes and comments,
/// ends after `|`, `&&` or `||`: the command it is in goes on after it, even
/// when the line does not parse (a reader drops a refused command to its
/// end). What is quoted counts as a word's characters, so a quote left
/// open, the line's own error, ends nothing open.
pub fn ends_open(line: &str) -> bool {
    let mut seen = String::new();
    let mut quote = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') => {
                chars.next();
                seen.push('x');
            }
            (None, '#') if seen.chars().last().is_none_or(|p| " \t;&|<>()".contains(p)) => break,
            (None, c) => seen.push(c),
        }
        if quote.is_some() {
            seen.push('x');
        }
    }
    let seen = seen.trim_end_matches([' ', '\t']);
    seen.ends_with('|') || seen.ends_with("&&")
}
````

with:

````rust
    parser.end()
}
````

- [ ] **Step 7: Change `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, make these 5 replacements, top to bottom:

Replace:

````rust

use crate::parser::{self, COMMAND_MAX, List, ParseError, Parser, Word};

````

with:

````rust

use crate::parser::{COMMAND_MAX, List, ParseError, Parser, Word};
use crate::scan::Scan;

````

Replace:

````rust
    parser: Parser,
    /// A command was dropped before its end: its later lines are dropped
    /// too, up to one that finishes it, so that none of them runs without
    /// what came before (the end of an `&&` chain without its guard).
    dropping: bool,
}
````

with:

````rust
    parser: Parser,
    /// A command was dropped before its end: what its lines open and
    /// close, while its later lines are dropped too, up to the one that
    /// closes what it opened (a construct's `fi` or `done`, or the line
    /// that finishes an `&&` chain), so that none of them runs without what
    /// came before.
    dropping: Option<Scan>,
}
````

Replace:

````rust
    pub fn add(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        if self.dropping {
            self.dropping = matches!(alone(line), Alone::GoesOn | Alone::Nothing);
            return Ok(None);
        }
        if self.parser.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line);
            return Err(ParseError::TooLong);
````

with:

````rust
    pub fn add(&mut self, line: &str) -> Result<Option<List<Word>>, ParseError> {
        if let Some(scan) = &mut self.dropping {
            scan.line(line.as_bytes());
            if scan.done() {
                self.dropping = None;
            }
            return Ok(None);
        }
        if self.parser.len() + line.len() + 1 > COMMAND_MAX {
            self.drop_line(line.as_bytes());
            return Err(ParseError::TooLong);
````

Replace:

````rust
        self.parser.line(line).inspect_err(|_| {
            // A line refused while it leaves the command open drops the
            // rest of the command with it, as `drop_line` does.
            self.parser = Parser::new();
            self.dropping = parser::ends_open(line);
        })
    }

    /// Drops the command a line is part of that cannot be added (too
    /// long, or no text: `line` is what can be read of it, or its last
    /// bytes): its later lines are dropped too unless this one plainly
    /// finishes it.
    pub fn drop_line(&mut self, line: &str) {
        let reading = self.reading();
        self.parser = Parser::new();
        // What does not parse alone might go on (a line's last bytes may
        // start inside a quote): it is dropped to be safe.
        self.dropping = match alone(line) {
            Alone::GoesOn | Alone::Wrong => true,
            Alone::Nothing => reading,
            Alone::Ends => false,
        };
    }

    /// Part of a command has been read, or is being dropped.
    pub fn reading(&self) -> bool {
        !self.parser.is_empty() || self.dropping
    }
````

with:

````rust
        self.parser.line(line).inspect_err(|_| {
            // A line that does not parse drops the rest of its command
            // with it: what has been read, the line too, says how far.
            let scan = self.drop_start();
            if !scan.done() {
                self.dropping = Some(scan);
            }
        })
    }

    /// Drops the command a line that cannot be added is part of (too
    /// long, or no text), and the lines after it up to its end.
    pub fn drop_line(&mut self, line: &[u8]) {
        self.drop_bytes(line);
        self.drop_end();
    }

    /// Drops the command the line being read is part of, `bytes` being
    /// more of the line: so a line too long to keep is counted whole.
    pub fn drop_bytes(&mut self, bytes: &[u8]) {
        let mut scan = self.drop_start();
        scan.bytes(bytes);
        self.dropping = Some(scan);
    }

    /// The line [`Reader::drop_bytes`] took ends.
    pub fn drop_end(&mut self) {
        if let Some(scan) = &mut self.dropping {
            scan.bytes(b"\n");
            if scan.done() {
                self.dropping = None;
            }
        }
    }

    /// What the command being dropped opens and closes: as far as it has
    /// been read, the lines before this one first.
    fn drop_start(&mut self) -> Scan {
        let scan = self.dropping.take().unwrap_or_else(|| {
            let mut scan = Scan::new();
            scan.bytes(self.parser.text().as_bytes());
            scan
        });
        self.parser = Parser::new();
        scan
    }

    /// Part of a command has been read, or is being dropped.
    pub fn reading(&self) -> bool {
        !self.parser.is_empty() || self.dropping.is_some()
    }
````

Replace:

````rust
        self.parser = Parser::new();
        self.dropping = false;
    }
}

/// What a line, read alone, does to an unfinished command before it.
enum Alone {
    /// It ends after `|`, `&&` or `||`: the command goes on after it.
    GoesOn,
    /// Blanks or a comment: the command goes on after it.
    Nothing,
    /// A command: it may finish the one before.
    Ends,
    /// It does not parse alone.
    Wrong,
}

fn alone(line: &str) -> Alone {
    match parser::parse_line(line) {
        Err(ParseError::Incomplete) => Alone::GoesOn,
        Ok(list) if list.items.is_empty() => Alone::Nothing,
        Ok(_) => Alone::Ends,
        Err(_) => Alone::Wrong,
    }
````

with:

````rust
        self.parser = Parser::new();
        self.dropping = None;
    }
````

- [ ] **Step 8: Implement `crates/shell/src/scan.rs`**

Insert this at the top of `crates/shell/src/scan.rs`, above `#[cfg(test)]`:

````rust
//! What a command being dropped opens and closes (programmable shell gate
//! §15 item 2). A reader that drops a command before its end goes on
//! dropping its lines until the constructs it opened are closed and no line
//! ends after `|`, `&&` or `||`, so that nothing of it runs without what
//! came before. The scan reads quotes, escapes and comments as the parser
//! does, but never fails, and takes a line a byte at a time, so that a line
//! too long to keep, or not text, still counts whole.

/// Where a `for` is, while its name and words are read.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum For {
    #[default]
    No,
    /// The next word is its name.
    Name,
    /// After its name: `in` or `do` may follow.
    AfterName,
}

/// The longest keyword, `until` and `while`.
const KEYWORD_MAX: usize = 5;

/// What the lines read so far open and close.
pub(crate) struct Scan {
    /// The constructs open: each `if`, `while`, `until` and `for` where a
    /// command name would stand counts one, and each `fi` and `done` there
    /// one less.
    depth: usize,
    /// What was read ends after `|`, `&&` or `||` (blank and comment lines
    /// after it leave it so).
    open: bool,
    /// The quote a word is in.
    quote: Option<u8>,
    /// After a `\`, which takes the next character.
    escaped: bool,
    comment: bool,
    /// How many `${` are open.
    braces: usize,
    /// The byte before, outside quotes.
    last: u8,
    /// The word being read: its first bytes, and how many it has. A quote
    /// or a `\` is one of them, so a word with one is no keyword.
    word: [u8; KEYWORD_MAX],
    len: usize,
    in_word: bool,
    /// The next word stands where a command name would.
    command: bool,
    /// The next word is a redirection's target.
    target: bool,
    for_: For,
}

impl Scan {
    pub fn new() -> Scan {
        Scan {
            depth: 0,
            open: false,
            quote: None,
            escaped: false,
            comment: false,
            braces: 0,
            last: b'\n',
            word: [0; KEYWORD_MAX],
            len: 0,
            in_word: false,
            command: true,
            target: false,
            for_: For::No,
        }
    }

    /// Reads some bytes of a line.
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    fn byte(&mut self, b: u8) {
        if b == b'\n' {
            // Nothing goes on past a line's end: a quote, an escape or a
            // `${` left open there is the line's error.
            self.end_word();
            self.newline();
            return;
        }
        if self.comment {
            return;
        }
        // What is escaped or quoted is part of a word that is no keyword.
        if self.escaped {
            self.escaped = false;
            return;
        }
        if let Some(q) = self.quote {
            match b {
                _ if b == q => self.quote = None,
                b'\\' if q == b'"' => self.escaped = true,
                _ => {}
            }
            return;
        }
        let last = core::mem::replace(&mut self.last, b);
        if self.braces > 0 {
            match b {
                b'}' => self.braces -= 1,
                b'{' if last == b'$' => self.braces += 1,
                b'\'' | b'"' => self.quote = Some(b),
                b'\\' => self.escaped = true,
                _ => {}
            }
            return;
        }
        match b {
            b' ' | b'\t' => self.end_word(),
            b'\'' | b'"' => {
                self.add(b);
                self.quote = Some(b);
            }
            b'\\' => {
                self.add(b);
                self.escaped = true;
            }
            b'{' if last == b'$' && self.in_word => {
                self.add(b);
                self.braces = 1;
            }
            b'#' if !self.in_word => self.comment = true,
            // `&&` and `||` go on to the next line, as `|` does.
            b'&' if last == b'&' => self.open = true,
            b'|' => {
                self.operator();
                self.open = true;
            }
            b';' | b'&' | b'(' | b')' => self.operator(),
            b'>' | b'<' => {
                self.end_word();
                self.target = true;
            }
            _ => self.add(b),
        }
    }

    /// Adds a byte to the word being read.
    fn add(&mut self, b: u8) {
        if !self.in_word {
            self.in_word = true;
            self.len = 0;
        }
        if let Some(slot) = self.word.get_mut(self.len) {
            *slot = b;
        }
        self.len = self.len.saturating_add(1);
    }

    /// `;`, `&`, `|`, `(` or `)`: the next word stands where a command
    /// name would, and a `for`'s words end.
    fn operator(&mut self) {
        self.end_word();
        self.open = false;
        self.command = true;
        self.target = false;
        self.for_ = For::No;
    }

    fn newline(&mut self) {
        self.quote = None;
        self.escaped = false;
        self.comment = false;
        self.braces = 0;
        self.last = b'\n';
        self.command = true;
        self.target = false;
        // A `for`'s words end with their line; an `in` on a later line is
        // a word that makes the rest of its line arguments, as the
        // parser's `in` would.
        self.for_ = For::No;
    }

    /// The word being read ends: a keyword where one may stand opens or
    /// closes a construct.
    fn end_word(&mut self) {
        if !core::mem::take(&mut self.in_word) {
            return;
        }
        self.open = false;
        // After a redirection, as in the parser, a word is a command's
        // name, never a keyword.
        if core::mem::take(&mut self.target) {
            self.command = false;
            return;
        }
        let word = self.word.get(..self.len).unwrap_or(b"");
        match self.for_ {
            For::Name => {
                self.for_ = For::AfterName;
                return;
            }
            // After `in` its words, up to a `;` or a newline, are no
            // command's name; after `do` one stands.
            For::AfterName => {
                self.for_ = For::No;
                self.command = word == b"do";
                return;
            }
            For::No => {}
        }
        if !self.command {
            return;
        }
        match word {
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
            b"for" => {
                self.depth = self.depth.saturating_add(1);
                self.for_ = For::Name;
            }
            b"fi" | b"done" => self.depth = self.depth.saturating_sub(1),
            b"then" | b"elif" | b"else" | b"do" | b"!" => {}
            _ => self.command = false,
        }
    }

    /// Reads a whole line and its newline.
    pub fn line(&mut self, line: &[u8]) {
        self.bytes(line);
        self.bytes(b"\n");
    }

    /// Everything read is closed: no construct is open and the last line
    /// does not end after `|`, `&&` or `||`.
    pub fn done(&self) -> bool {
        self.depth == 0 && !self.open
    }
}

````

- [ ] **Step 9: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
const CONTINUE: &str = "> ";
/// How much of a line over 64 KiB `X | sh` keeps: its last bytes, between
/// this and twice it.
const LINE_TAIL: usize = 1024;
/// The most of `/etc/motd` shown at start.
````

with:

````rust
const CONTINUE: &str = "> ";
/// The most of `/etc/motd` shown at start.
````

Replace:

````rust
                if line.len() as u64 > SCRIPT_MAX {
                    too_long = true;
                    line.clear();
                }
            } else {
                // Only its last bytes, which say whether the command it is
                // in goes on after it.
                line.push(byte[0]);
                if line.len() == 2 * LINE_TAIL {
                    line.drain(..LINE_TAIL);
                }
            }
````

with:

````rust
                if line.len() as u64 > SCRIPT_MAX {
                    // Not kept, but counted whole: it says how much of what
                    // follows is dropped with it.
                    too_long = true;
                    reader.drop_bytes(&line);
                    line.clear();
                }
            } else {
                reader.drop_bytes(&byte);
            }
````

Replace:

````rust
            _ if too_long => {
                reader.drop_line(&String::from_utf8_lossy(line));
                self.finish(1, String::from("sh: standard input: a line over 64 KiB\n"));
````

with:

````rust
            _ if too_long => {
                reader.drop_end();
                self.finish(1, String::from("sh: standard input: a line over 64 KiB\n"));
````

Replace:

````rust
            Err(_) => {
                reader.drop_line(&String::from_utf8_lossy(line));
                self.finish(1, String::from("sh: standard input: not a text line\n"));
````

with:

````rust
            Err(_) => {
                reader.drop_line(line);
                self.finish(1, String::from("sh: standard input: not a text line\n"));
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 327 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): drop a dropped command up to the end of its constructs

A command dropped before its end is now dropped by a scan that counts
each `if`, `while`, `until` and `for` and each `fi` and `done` where a
command name would stand, and notes a line ending after `|`, `&&` or
`||`. The scan reads quotes, escapes and comments as the parser does
but never fails, and takes a line a byte at a time, so `X | sh` counts
a line too long to keep whole instead of judging it by its last bytes.

So a script's `if` written across lines no longer runs its body while
its words are refused (plan 1's ruling): its lines are dropped up to
its `fi` (programmable shell gate §15 item 2).
EOF
````


### Task 4: The scan reads refused syntax as bash reads it

The prototype's review (important I1) and decision 6: the scan runs after a refusal, so it must count across what was refused as bash would, and it did not: in `while a; do` … `done`, a `done` inside `$(…)` or backquotes was counted, and an opener after `time` or `{`, or a `select`, was not, so the loop's tail ran without its head (six inputs, the most realistic `echo copy of $(hostname) done`). It now reads `$(…)`, `$((…))` and backquotes whole as part of a word, as `${…}` (one count for every open `${`, `$(` and `(` in them, so the scan stays constant in memory), keeps a command name's place after `time`, `{` and `}`, opens at `select` as at `for` and at `case`, closes at `esac`, and takes a word before `)` for a `case` pattern, no keyword. The red run is the shell's tests: the six inputs run their tail. Mutation checks: `$(` not opening, `(` and `)` not counted inside, a backquote not quoting (outside and inside `$(…)`), a `\` in one not escaping, `time`, `{`, `}`, `select`, `case` or `esac` left out, a `case` subject taken as a command name, the pattern rule dropped and the longest keyword cut to five bytes, each fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 3's `Scan`.
- Produces: nothing new outside the scan (`Scan.nested` replaces `braces`; `KEYWORD_MAX` is 6).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

with:

````rust
    #[test]
    fn refused_syntax_is_read_as_bash_reads_it() {
        // The prototype's review: the scan runs after a refusal, so it
        // must count across what was refused as bash would.
        for (lines, depth) in [
            // A command name stands after `time`, `{` and `}`.
            (&["time if a; then"][..], 1),
            (&["{ while a; do"], 1),
            (&["while a; do { b; } done"], 0),
            // `select` opens as `for` does, `case` as `esac` closes.
            (&["select x in if; do"], 1),
            (&["select x in a; do b; done"], 0),
            (&["case a in"], 1),
            (&["case done in"], 1),
            (&["case a in", "b) c;;", "esac"], 0),
            // A word before `)` is a `case` pattern.
            (&["case a in", "done) b;;", "fi) c;;"], 1),
            // `$(…)`, `$((…))` and backquotes are read whole.
            (&["echo $(if a; then b; fi) done"], 0),
            (&["echo $(a; fi) done; if b"], 1),
            (&["echo $((1 + (2))) fi; while a"], 1),
            (&["echo $(a (b) ; if c) x; while d"], 1),
            (&["echo $(a `)` b) fi; if c"], 1),
            (&["echo ` if a; then b; fi `; until c"], 1),
            (&["echo `a \\` fi` b; for x in y; do"], 1),
            (&["echo \"$(a) `b`\" done"], 0),
        ] {
            assert_eq!(after(lines), (depth, false), "{lines:?}");
        }
        // Left open at a line's end, they end with it as quotes do.
        assert_eq!(after(&["echo $(a", "if b"]), (1, false));
        assert_eq!(after(&["echo `a", "if b"]), (1, false));
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(out.matches("\n> ").count(), 2, "{out}");
    }
````

with:

````rust
        assert_eq!(out.matches("\n> ").count(), 2, "{out}");
    }

    #[test]
    fn a_dropped_construct_s_refused_syntax_hides_none_of_its_end() {
        // The prototype's review ran each tail without its loop: a `fi` or
        // `done` that the refused syntax around it hides from bash was
        // counted, or an opener it shows to bash was not.
        let head = b"while t-args a; do\n";
        for inner in [
            &b"time for f in b\ndo t-args c\ndone\n"[..],
            b"select x in a b; do\nt-args c\ndone\n",
            b"{ if t-args b\nthen t-args c\nfi\n}\n",
            b"t-args ` if t-args b; then t-args c; fi `\n",
            b"t-args copy of $(hostname) done\n",
            b"case b in\ndone) t-args c;;\nesac\n",
        ] {
            let mut text = head.to_vec();
            text.extend_from_slice(inner);
            text.extend_from_slice(b"t-args tail\ndone\nt-args next\n");
            assert_eq!(piped(&text), ["next"], "{}", String::from_utf8_lossy(inner));
        }
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::refused_syntax_is_read_as_bash_reads_it`, `shell::tests::a_dropped_construct_s_refused_syntax_hides_none_of_its_end`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 11 replacements, top to bottom:

Replace:

````rust
//! does, but never fails, and takes a line a byte at a time, so that a line
//! too long to keep, or not text, still counts whole.

````

with:

````rust
//! does, but never fails, and takes a line a byte at a time, so that a line
//! too long to keep, or not text, still counts whole. It runs after a
//! refusal, so it reads what the parser refuses as bash reads it: `$(…)`
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`.

````

Replace:

````rust

/// The longest keyword, `until` and `while`.
const KEYWORD_MAX: usize = 5;

````

with:

````rust

/// The longest keyword the scan looks for, `select`.
const KEYWORD_MAX: usize = 6;

````

Replace:

````rust
    comment: bool,
    /// How many `${` are open.
    braces: usize,
    /// The byte before, outside quotes.
````

with:

````rust
    comment: bool,
    /// How many `${`, `$(` and `(` inside them are open: what is in them
    /// is part of a word.
    nested: usize,
    /// The byte before, outside quotes.
````

Replace:

````rust
            comment: false,
            braces: 0,
            last: b'\n',
````

with:

````rust
            comment: false,
            nested: 0,
            last: b'\n',
````

Replace:

````rust
                _ if b == q => self.quote = None,
                b'\\' if q == b'"' => self.escaped = true,
                _ => {}
````

with:

````rust
                _ if b == q => self.quote = None,
                b'\\' if q != b'\'' => self.escaped = true,
                _ => {}
````

Replace:

````rust
        let last = core::mem::replace(&mut self.last, b);
        if self.braces > 0 {
            match b {
                b'}' => self.braces -= 1,
                b'{' if last == b'$' => self.braces += 1,
                b'\'' | b'"' => self.quote = Some(b),
                b'\\' => self.escaped = true,
````

with:

````rust
        let last = core::mem::replace(&mut self.last, b);
        if self.nested > 0 {
            match b {
                b'}' | b')' => self.nested -= 1,
                b'{' if last == b'$' => self.nested += 1,
                b'(' => self.nested += 1,
                b'\'' | b'"' | b'`' => self.quote = Some(b),
                b'\\' => self.escaped = true,
````

Replace:

````rust
            b' ' | b'\t' => self.end_word(),
            b'\'' | b'"' => {
                self.add(b);
````

with:

````rust
            b' ' | b'\t' => self.end_word(),
            b'\'' | b'"' | b'`' => {
                self.add(b);
````

Replace:

````rust
            }
            b'{' if last == b'$' && self.in_word => {
                self.add(b);
                self.braces = 1;
            }
````

with:

````rust
            }
            b'{' | b'(' if last == b'$' && self.in_word => {
                self.add(b);
                self.nested = 1;
            }
````

Replace:

````rust
            }
            b';' | b'&' | b'(' | b')' => self.operator(),
            b'>' | b'<' => {
````

with:

````rust
            }
            // The word before a `)` is a `case` pattern, no keyword.
            b')' => {
                self.command = false;
                self.operator();
            }
            b';' | b'&' | b'(' => self.operator(),
            b'>' | b'<' => {
````

Replace:

````rust
        self.comment = false;
        self.braces = 0;
        self.last = b'\n';
````

with:

````rust
        self.comment = false;
        self.nested = 0;
        self.last = b'\n';
````

Replace:

````rust
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
            b"for" => {
                self.depth = self.depth.saturating_add(1);
                self.for_ = For::Name;
            }
            b"fi" | b"done" => self.depth = self.depth.saturating_sub(1),
            b"then" | b"elif" | b"else" | b"do" | b"!" => {}
            _ => self.command = false,
````

with:

````rust
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
            b"for" | b"select" => {
                self.depth = self.depth.saturating_add(1);
                self.for_ = For::Name;
            }
            // Its word and `in` are no command's name; a pattern is
            // followed by `)`.
            b"case" => {
                self.depth = self.depth.saturating_add(1);
                self.command = false;
            }
            b"fi" | b"done" | b"esac" => self.depth = self.depth.saturating_sub(1),
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" | b"{" | b"}" => {}
            _ => self.command = false,
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 329 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read refused syntax as bash does when dropping a command

The prototype's review ran a loop's tail without its head: the scan
that drops a command to its end counted a `fi` or `done` that refused
syntax hides from bash, and missed an opener it shows. It now reads
`$(…)` and backquotes whole, as `${…}`, takes a command name's place
after `time`, `{` and `}`, opens at `select` and `case`, closes at
`esac`, and takes a word before `)` for a `case` pattern
(programmable shell gate §15 item 2).
EOF
````


### Task 5: The scan drops no more than bash does after a line in error

The prototype's review (minor M1) and decision 6: the scan dropped lines that bash runs. A redirection with no target at a line's end (`a && >`) left its command open, and an opener right after a closer (`fi if c`, probe fiif), bash's syntax error and no construct, was counted, dropping everything up to a later `fi`. A target left missing now ends the line's command, and an opener after `fi`, `done`, `esac` or `}` (probe brace_if) opens nothing; a closer there still closes (`fi fi`). The red run is the shell's tests: `t-args next` is dropped after each. Mutation checks: the target's reset, the opener rule, each closer's mark and its resets at an operator and a newline, and `for` left out of the openers, each fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 4's scan.
- Produces: `scan::OPENERS`; `Scan.closed` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

with:

````rust
    #[test]
    fn a_line_bash_ends_in_error_leaves_nothing_open() {
        // The prototype's review: bash runs the next line after these, so
        // the scan drops nothing more.
        // A redirection with no target ends the line's command.
        assert_eq!(after(&["a && >"]), (0, false));
        assert_eq!(after(&["a | >>"]), (0, false));
        // An opener right after a closer is an error, no construct (fiif,
        // brace_if); a closer there still closes.
        for line in [
            "if a; then b; fi if c; then",
            "while a; do b; done while c; do",
            "{ a; } if b; then",
            "case a in b) c;; esac for x in y; do",
        ] {
            assert_eq!(after(&[line]), (0, false), "{line}");
        }
        assert_eq!(after(&["if a; then if b; then c; fi fi"]), (0, false));
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            assert_eq!(piped(&text), ["next"], "{}", String::from_utf8_lossy(inner));
        }
````

with:

````rust
            assert_eq!(piped(&text), ["next"], "{}", String::from_utf8_lossy(inner));
        }
    }

    #[test]
    fn a_line_bash_ends_in_error_drops_no_more() {
        // The prototype's review: bash runs `next` after each.
        for text in [
            &b"t-args a && >\nt-args next\n"[..],
            b"if t-args a; then t-args b; fi if t-args c\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::a_line_bash_ends_in_error_leaves_nothing_open`, `shell::tests::a_line_bash_ends_in_error_drops_no_more`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
const KEYWORD_MAX: usize = 6;

````

with:

````rust
const KEYWORD_MAX: usize = 6;

/// The words that open a construct.
const OPENERS: &[&[u8]] = &[b"if", b"while", b"until", b"for", b"select", b"case"];

````

Replace:

````rust
    target: bool,
    for_: For,
````

with:

````rust
    target: bool,
    /// The word before was a closer (`fi`, `done`, `esac`, `}`), after
    /// which an opener is bash's error, no construct.
    closed: bool,
    for_: For,
````

Replace:

````rust
            target: false,
            for_: For::No,
````

with:

````rust
            target: false,
            closed: false,
            for_: For::No,
````

Replace:

````rust
        self.target = false;
        self.for_ = For::No;
````

with:

````rust
        self.target = false;
        self.closed = false;
        self.for_ = For::No;
````

Replace:

````rust
        self.command = true;
        self.target = false;
        // A `for`'s words end with their line; an `in` on a later line is
````

with:

````rust
        self.command = true;
        // A redirection with no target is the line's error: its command
        // goes on no further, as bash's does not.
        if core::mem::take(&mut self.target) {
            self.open = false;
        }
        self.closed = false;
        // A `for`'s words end with their line; an `in` on a later line is
````

Replace:

````rust
        }
        match word {
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
````

with:

````rust
        }
        let closed = core::mem::take(&mut self.closed);
        match word {
            _ if closed && OPENERS.contains(&word) => self.command = false,
            b"if" | b"while" | b"until" => self.depth = self.depth.saturating_add(1),
````

Replace:

````rust
            }
            b"fi" | b"done" | b"esac" => self.depth = self.depth.saturating_sub(1),
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" | b"{" | b"}" => {}
            _ => self.command = false,
````

with:

````rust
            }
            b"fi" | b"done" | b"esac" => {
                self.depth = self.depth.saturating_sub(1);
                self.closed = true;
            }
            b"}" => self.closed = true,
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" | b"{" => {}
            _ => self.command = false,
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 331 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): drop no more than bash does after a line in error

The prototype's review found the scan dropping lines that bash runs: a
redirection with no target at a line's end (`a && >`) left its command
open, and an opener right after a closer (`fi if c`), which is bash's
syntax error and no construct, was counted. A target left missing now
ends the line's command, and an opener after `fi`, `done`, `esac` or
`}` opens nothing (programmable shell gate §15 item 2).
EOF
````


### Task 6: A refused here-document's body is dropped with it

The prototype's review (minor M5, there since milestone 3) and decision 11: `<<` is refused, but the lines of its body then ran as commands: after `cat > x <<EOF`, a line `rm -r d` before `EOF` ran. The scan now reads a here-document's delimiter after `<<` or `<<-` (its quotes and `\` removed, as bash removes them; probes in `progress.md`), and takes the lines after its line as data up to the delimiter's (its leading tabs stripped after `<<-`), several in turn; `<<<` has no body. It keeps at most 8 delimiters a line, each by its first 64 bytes and its length, so its memory stays bounded. The red run is the shell's tests: the body runs. Mutation checks: `<<<` taken as `<<`, `-` taken as the word, the tabs not stripped or stripped after `<<`, `<` not counted, an escape, a quote or a blank before the word not read, a quote kept, an empty word kept, the next body not started, the length not compared, the body not counted in `done()` and the count of `<` kept across lines, each fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 5's scan.
- Produces: `scan::{HEREDOCS_MAX, DELIM_MAX}`, `Delim`, `Reading` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

with:

````rust
    }

    /// Whether the scan is done after each of `lines`.
    fn done_after(lines: &[&str]) -> Vec<bool> {
        let mut s = Scan::new();
        lines
            .iter()
            .map(|l| {
                s.line(l.as_bytes());
                s.done()
            })
            .collect()
    }

    #[test]
    fn a_here_document_s_body_is_read_to_its_delimiter() {
        // The prototype's review: `<<` is refused, and its body ran as
        // commands. The body is data up to its delimiter's line.
        assert_eq!(
            done_after(&["cat <<EOF", "if a", "fi", "done", "EOF", "b"]),
            [false, false, false, false, true, true]
        );
        // `<<-` strips leading tabs; the word's quotes are removed; a blank
        // before the word is allowed, before the delimiter's line not.
        assert_eq!(
            done_after(&["cat <<-EOF", "\tif a", "\tEOF"]),
            [false, false, true]
        );
        for (head, end) in [
            ("cat <<'E F'", "E F"),
            ("cat <<E\"O\"F", "EOF"),
            ("cat <<\\EOF", "EOF"),
            ("cat << EOF", "EOF"),
            ("a<<EOF", "EOF"),
        ] {
            assert_eq!(
                done_after(&[head, "x", end]),
                [false, false, true],
                "{head}"
            );
        }
        assert_eq!(
            done_after(&["cat <<EOF", " EOF", "EOF"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["cat <<EOF", "\tEOF", "EOF"]),
            [false, false, true]
        );
        assert_eq!(
            done_after(&["cat <<E\\ F", "E", "E F"]),
            [false, false, true]
        );
        // No word after it, or a `<` on the next line, is no here-document.
        assert_eq!(done_after(&["a <<", "b"]), [true, true]);
        assert_eq!(done_after(&["a << ;", "b"]), [true, true]);
        assert_eq!(done_after(&["a <", "<b", "c"]), [true, true, true]);
        // Several on one line take their bodies in turn.
        assert_eq!(
            done_after(&["cat <<A <<B", "B", "A", "A", "B"]),
            [false, false, false, false, true]
        );
        // `<<<` has no body, nor a quoted `<<`.
        assert_eq!(
            done_after(&["cat <<< if", "echo '<<EOF'", "a \\<<EOF"]),
            [true, true, true]
        );
        // Inside a construct, its `done` in the body closes nothing.
        assert_eq!(
            done_after(&["while a; do", "cat <<EOF", "done", "EOF", "done"]),
            [false, false, false, false, true]
        );
        // A delimiter longer than the scan keeps whole still ends it.
        let long = "x".repeat(100);
        let head = alloc::format!("cat <<{long}");
        let other = alloc::format!("{long}y");
        assert_eq!(done_after(&[&head, &other, &long]), [false, false, true]);
    }

    #[test]
    fn a_line_may_come_in_pieces_and_need_not_be_text() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

````

with:

````rust
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_here_document_s_body_does_not_run() {
        // The prototype's review: `cat > x <<EOF`, refused, then ran each
        // line of its body as a command.
        for text in [
            &b"t-args a <<EOF\nt-args body\nEOF\nt-args next\n"[..],
            b"while t-args a; do\nt-args <<EOF\ndone\nEOF\nt-args tail\ndone\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
        let mut h = Harness::new();
        h.run("mkdir /tmp/d");
        h.put(
            "/tmp/s.sh",
            b"cat > /tmp/x <<EOF\nrm -r /tmp/d\nEOF\necho next\n",
        );
        let (_, out) = h.run("sh /tmp/s.sh");
        assert!(out.ends_with("+ EOF\n+ echo next\nnext\n"), "{out}");
        assert!(h.exists("/tmp/d"));
    }

````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::a_here_document_s_body_is_read_to_its_delimiter`, `shell::tests::a_here_document_s_body_does_not_run`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`.

````

with:

````rust
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! and a here-document's body, data up to its delimiter's line.

````

Replace:

````rust
const KEYWORD_MAX: usize = 6;

````

with:

````rust
const KEYWORD_MAX: usize = 6;

/// The most here-documents the scan keeps that one line starts; the
/// bodies of any more are read as lines.
const HEREDOCS_MAX: usize = 8;

/// How much of a here-document's delimiter, or of a line of its body, the
/// scan keeps: a line as long, starting with those bytes, ends the body.
const DELIM_MAX: usize = 64;

/// A here-document's delimiter, or a line of its body: its first bytes,
/// its length, and (a delimiter's) whether the body's leading tabs are
/// stripped (`<<-`).
#[derive(Clone, Copy)]
struct Delim {
    bytes: [u8; DELIM_MAX],
    len: usize,
    tabs: bool,
}

impl Delim {
    const fn new(tabs: bool) -> Delim {
        Delim {
            bytes: [0; DELIM_MAX],
            len: 0,
            tabs,
        }
    }

    fn push(&mut self, b: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = b;
        }
        self.len = self.len.saturating_add(1);
    }

    fn same(&self, other: &Delim) -> bool {
        let n = self.len.min(DELIM_MAX);
        self.len == other.len && self.bytes[..n] == other.bytes[..n]
    }
}

/// A here-document's delimiter being read after `<<`: its quotes are
/// removed, as bash removes them.
#[derive(Clone, Copy)]
struct Reading {
    delim: Delim,
    quote: Option<u8>,
    escaped: bool,
    started: bool,
}

````

Replace:

````rust
    closed: bool,
    for_: For,
````

with:

````rust
    closed: bool,
    /// How many unquoted `<` came last.
    lt: usize,
    /// The delimiter being read after `<<`.
    reading: Option<Reading>,
    /// The here-documents the line started, and the one whose body is
    /// being read, if one is.
    heredocs: [Delim; HEREDOCS_MAX],
    pending: usize,
    body: Option<usize>,
    /// The body's line being read.
    body_line: Delim,
    for_: For,
````

Replace:

````rust
            closed: false,
            for_: For::No,
````

with:

````rust
            closed: false,
            lt: 0,
            reading: None,
            heredocs: [Delim::new(false); HEREDOCS_MAX],
            pending: 0,
            body: None,
            body_line: Delim::new(false),
            for_: For::No,
````

Replace:

````rust
    fn byte(&mut self, b: u8) {
        if b == b'\n' {
````

with:

````rust
    fn byte(&mut self, b: u8) {
        if self.body.is_some() {
            self.body_byte(b);
            return;
        }
        if self.reading.is_some() && self.delim_byte(b) {
            return;
        }
        if b == b'\n' {
````

Replace:

````rust
            }
            return;
        }
        let last = core::mem::replace(&mut self.last, b);
````

with:

````rust
            }
            return;
        }
        // `<<` and `<<-` start a here-document, whose delimiter follows;
        // `<<<` does not.
        if self.lt > 0 && b != b'<' && core::mem::take(&mut self.lt) == 2 {
            self.target = false;
            self.reading = Some(Reading {
                delim: Delim::new(b == b'-'),
                quote: None,
                escaped: false,
                started: false,
            });
            if b == b'-' || self.delim_byte(b) {
                return;
            }
        }
        let last = core::mem::replace(&mut self.last, b);
````

Replace:

````rust
            b';' | b'&' | b'(' => self.operator(),
            b'>' | b'<' => {
                self.end_word();
                self.target = true;
            }
            _ => self.add(b),
        }
    }
````

with:

````rust
            b';' | b'&' | b'(' => self.operator(),
            b'>' => {
                self.end_word();
                self.target = true;
            }
            b'<' => {
                self.end_word();
                self.target = true;
                self.lt += 1;
            }
            _ => self.add(b),
        }
    }

    /// A byte of a here-document's delimiter, after `<<`: whether it was
    /// one, or ended the delimiter and is the line's again.
    fn delim_byte(&mut self, b: u8) -> bool {
        let Some(r) = &mut self.reading else {
            return false;
        };
        if core::mem::take(&mut r.escaped) {
            r.delim.push(b);
            return true;
        }
        if let Some(q) = r.quote {
            match b {
                b'\n' => {}
                _ if b == q => r.quote = None,
                b'\\' if q == b'"' => r.escaped = true,
                _ => r.delim.push(b),
            }
            if b != b'\n' {
                return true;
            }
        }
        match b {
            b' ' | b'\t' if !r.started => return true,
            b'\'' | b'"' => r.quote = Some(b),
            b'\\' => r.escaped = true,
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => {
                let delim = r.delim;
                let started = r.started;
                self.reading = None;
                if started && let Some(slot) = self.heredocs.get_mut(self.pending) {
                    *slot = delim;
                    self.pending += 1;
                }
                return false;
            }
            _ => r.delim.push(b),
        }
        r.started = true;
        true
    }

    /// A byte of a here-document's body: a line equal to its delimiter
    /// (its leading tabs stripped after `<<-`) ends it, and the next one's
    /// starts.
    fn body_byte(&mut self, b: u8) {
        let Some(i) = self.body else {
            return;
        };
        let Some(delim) = self.heredocs.get(i).copied() else {
            self.body = None;
            return;
        };
        if b == b'\n' {
            if self.body_line.same(&delim) {
                self.body = Some(i + 1).filter(|&n| n < self.pending);
                if self.body.is_none() {
                    self.pending = 0;
                }
            }
            self.body_line = Delim::new(false);
            return;
        }
        if self.body_line.len == 0 && delim.tabs && b == b'\t' {
            return;
        }
        self.body_line.push(b);
    }
````

Replace:

````rust
        self.last = b'\n';
        self.command = true;
        // A redirection with no target is the line's error: its command
````

with:

````rust
        self.last = b'\n';
        self.lt = 0;
        self.command = true;
        // The here-documents the line started take the next lines.
        if self.pending > 0 {
            self.body = Some(0);
            self.body_line = Delim::new(false);
        }
        // A redirection with no target is the line's error: its command
````

Replace:

````rust
    pub fn done(&self) -> bool {
        self.depth == 0 && !self.open
    }
````

with:

````rust
    pub fn done(&self) -> bool {
        self.depth == 0 && !self.open && self.body.is_none()
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 333 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): drop a refused here-document's body with it

The prototype's review found `<<` refused but its body run: after `cat
> x <<EOF`, each line up to `EOF` ran as a command. The scan now reads
a here-document's delimiter after `<<` or `<<-`, its quotes removed as
bash removes them, and takes the lines after its line as data up to
the delimiter's (its leading tabs stripped after `<<-`), several in
turn, so the body is dropped with the line (programmable shell gate
§15 item 2). `<<<` has no body.
EOF
````


### Task 7: `if`, `elif` and `else` run

Spec §4.1, §4.2, §4.4, §5.1 and decisions 1, 2, 4, 5, 8: `if`, `then`, `elif`, `else` and `fi` become keywords where a command name would stand, unquoted and whole (`Parts::end_word` returns the `Keyword`), and stay words elsewhere (`echo if`, `"if"`, a redirection's target). The parser keeps a stack of the compound commands being read (`Open`: the list around it, its `!`s, its stage and its lists so far); `then`, `elif`, `else` and `fi` end the list before them, which must hold something and leave nothing open, and move the `if` on or close it, and `fi` makes it the pipeline's `Run::Compound(Compound::If)`. After `fi` only a keyword that closes or goes on with the construct around (`fi fi`, `fi then`) or an operator may come: any other word is bash's error naming it as typed (`ParseError::Unexpected`). Every error names bash 5.2's token (probes e1–e10, f1, g9–g17, h1–h12, k1–k12 in `progress.md`). An `if` in a pipeline, before `&` or redirected is refused (`| after fi`, `if after |`, `& after fi`, `> after fi`), and the in-process runner refuses a job at any depth (`List::has_job`). The walker splits `run_list` (a command line's) from `run_items` (any list's) and runs `run_if`: each condition in turn, the body of the first whose status is 0, else the `else` body; the status is the body's, 0 when none runs, and `exit`, Ctrl-C and a bad substitution end it and its line. `while`, `until` and `for` stay refused. The tests that pinned `if` as refused become tests of this (the scan's own, of a refused line inside a construct, use `while`). The red run is the shell's tests, which cannot find `Compound` or `If`. Mutation checks: the after-`fi` rule (both halves), each refusal, `if after |`, the `!` before an `if`, each stage's transition, an empty list or a `!` or a connector before a keyword taken, the `else` body lost, the job search's three parts, `Incomplete` in `line()` and `end()`, a quoted keyword taken, the job text's start at a keyword, the condition's stop, `status == 0`, the status of no branch and the status not set, each fail a test (two survivors of the first tests got the tests they named, and one dead line went).

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 1's `Run`; Task 2's `Parser`; Task 3's dropping.
- Produces: `parser::Run::Compound(Box<Compound<W>>)`, `parser::Compound<W>::If(If<W>)`, `parser::If<W> { branches: Vec<(List<W>, List<W>)>, otherwise: Option<List<W>> }`, `List::has_job(&self) -> bool`, `ParseError::Unexpected(String)`; `parser::{Keyword, Stage, Open}`, `Parser.open`, `Parts.compound`, `Parts::is_empty`, `Parser::{end_word, keyword, end_list}` (private); `Shell::{run_items, run_compound, run_if}` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
                Run::Commands(c) => c,
            }
        }
    }

    /// The commands of `line`'s one pipeline, as typed.
````

with:

````rust
                Run::Commands(c) => c,
                Run::Compound(_) => panic!("a compound command"),
            }
        }
    }

    /// The command name of each pipeline of each item of `list`, a compound
    /// command's as its first word (`if`).
    fn names_of(list: &List<Word>) -> Vec<String> {
        let mut names = Vec::new();
        for item in &list.items {
            for p in
                core::iter::once(&item.and_or.first).chain(item.and_or.rest.iter().map(|(_, p)| p))
            {
                names.push(match &p.run {
                    Run::Commands(c) => c[0].words[0].typed.clone(),
                    Run::Compound(c) => match **c {
                        Compound::If(_) => String::from("if"),
                    },
                });
            }
        }
        names
    }

    /// The `if` of `line`'s one item, and whether it is negated.
    fn if_of(line: &str) -> (bool, If<Word>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        match p.run {
            Run::Compound(c) => match *c {
                Compound::If(i) => (p.negated, i),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    /// Each of an `if`'s lists, its command names.
    fn branches(i: &If<Word>) -> Vec<Vec<String>> {
        let mut lists: Vec<Vec<String>> = Vec::new();
        for (c, b) in &i.branches {
            lists.push(names_of(c));
            lists.push(names_of(b));
        }
        lists.extend(i.otherwise.iter().map(names_of));
        lists
    }

    #[test]
    fn an_if_holds_its_conditions_and_bodies() {
        let (negated, i) = if_of("if a; then b; c; elif d; then e; else f; fi");
        assert!(!negated);
        assert_eq!(
            branches(&i),
            [["a"].as_slice(), &["b", "c"], &["d"], &["e"], &["f"]]
        );
        assert!(i.otherwise.is_some());
        let (_, i) = if_of("if a && b; then c | d; fi");
        assert_eq!(branches(&i), [["a", "b"].as_slice(), &["c"]]);
        assert!(i.otherwise.is_none());
        // f3: `!` negates it.
        assert!(if_of("! if a; then b; fi").0);
        // Across lines (p1, l1), blank and comment lines among them (p5).
        let (_, i) = if_of("if\na\n\n# c\nthen b\nelse\nc\nfi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["b"], &["c"]]);
        // Nested, and after `fi` a keyword with no `;` (k1, k2).
        let (_, i) = if_of("if a; then if b; then c; fi fi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["if"]]);
        let (_, i) = if_of("if if a; then b; fi then c; fi");
        assert_eq!(branches(&i), [["if"].as_slice(), &["c"]]);
        // In a list and an and-or list (f2, g11, k4).
        let list = parse_line("if a; then b; fi; c && if d; then e; fi || f").unwrap();
        assert_eq!(names_of(&list), ["if", "c", "if", "f"]);
        let list = parse_line("a && if b; then c; fi\nd").unwrap();
        assert_eq!(names_of(&list), ["a", "if", "d"]);
        // h11: a background job in a body.
        let (_, i) = if_of("if a; then b & fi");
        assert_eq!(i.branches[0].1.items[0].background.as_deref(), Some("b"));
        let (_, i) = if_of("if a & then b; fi");
        assert_eq!(i.branches[0].0.items[0].background.as_deref(), Some("a"));
    }

    #[test]
    fn if_words_are_keywords_only_where_a_command_name_stands() {
        // f8, h8: as an argument, quoted or a redirection's target they
        // are words.
        for (line, words) in [
            (
                "echo if then elif else fi",
                &["echo", "if", "then", "elif", "else", "fi"][..],
            ),
            ("\"if\" x", &["if", "x"]),
            ("'then' x", &["then", "x"]),
            ("\\fi x", &["fi", "x"]),
            ("> then fi", &["fi"]),
            ("iffy", &["iffy"]),
        ] {
            assert_eq!(parse(line).unwrap()[0].words, words, "{line}");
        }
        let (_, i) = if_of("if a > then; then b; fi");
        assert_eq!(
            i.branches[0].0.items[0].and_or.first.commands()[0]
                .redirect
                .as_ref()
                .unwrap()
                .path
                .typed,
            "then"
        );
    }

    #[test]
    fn an_if_not_finished_needs_more_lines() {
        for text in [
            "if",
            "if a",
            "if a;",
            "if a; then",
            "if a; then b",
            "if a; then b; elif c",
            "if a; then b; else",
            "if a; then b; else c",
            "if a; then if b; then c; fi",
            "if a; then b; fi &&",
            "a && if b; then c; fi |",
        ] {
            let e = parse_line(text);
            if text.ends_with('|') {
                assert_eq!(
                    e,
                    Err(ParseError::Unsupported("| after fi".into())),
                    "{text}"
                );
            } else {
                assert_eq!(e, Err(ParseError::Incomplete), "{text}");
            }
        }
        // A reader's lines: each line once, the `if` given at its `fi`.
        let mut p = Parser::new();
        assert_eq!(p.line("if a"), Ok(None));
        assert_eq!(p.line("then b"), Ok(None));
        assert_eq!(names_of(&p.line("fi; c").unwrap().unwrap()), ["if", "c"]);
        assert!(p.is_empty());
    }

    #[test]
    fn a_keyword_out_of_place_is_bash_s_syntax_error() {
        for (line, token) in [
            ("if then echo a; fi", "then"),
            ("if true; then fi", "fi"),
            ("if true; fi", "fi"),
            ("fi", "fi"),
            ("then", "then"),
            ("elif", "elif"),
            ("if true; then echo a; else fi", "fi"),
            ("if true; then echo a; elif then echo b; fi", "then"),
            ("if true; then echo a; fi; then", "then"),
            ("echo; else", "else"),
            ("if true; then echo a; fi if", "if"),
            ("if true; then echo a; fi ! true", "!"),
            ("if ! then echo a; fi", "then"),
            ("if true; then ! fi", "fi"),
            (
                "if true; then echo a; else echo b; elif true; then echo c; fi",
                "elif",
            ),
            ("if true; then echo a; else echo b; else echo c; fi", "else"),
            ("if true && then echo; fi", "then"),
            ("if true |\nthen echo; fi", "then"),
            ("if true; then echo a; fi; fi", "fi"),
            ("if true\nthen\nfi", "fi"),
            ("if a; then b; fi echo b", "echo"),
            ("if a; then b; fi 'x y'", "'x y'"),
            ("if a; then b; fi \"$x\"", "\"$x\""),
            ("if a; then b; fi ${x}y", "${x}y"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // Told on the line that has it.
        let mut p = Parser::new();
        assert_eq!(p.line("if true"), Ok(None));
        assert_eq!(p.line("then"), Ok(None));
        assert_eq!(p.line("fi"), Err(ParseError::MissingTarget("fi")));
    }

    #[test]
    fn an_if_in_a_pipeline_with_ampersand_or_redirected_is_refused() {
        // bash runs them in a subshell, or redirects all of it (f4–f7).
        for (line, what) in [
            ("if true; then echo a; fi | cat", "| after fi"),
            ("echo x | if true; then cat; fi", "if after |"),
            ("if true; then echo a; fi > f", "> after fi"),
            ("if true; then echo a; fi >> f", ">> after fi"),
            ("if true; then echo a; fi &", "& after fi"),
            ("if true; then echo a; fi & b", "& after fi"),
            ("if a; then if b; then c; fi | d; fi", "| after fi"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }
    /// The commands of `line`'s one pipeline, as typed.
````

Replace:

````rust
    fn a_reserved_word_where_a_command_name_stands_is_unsupported() {
        // Until compound commands come (programmable shell gate §4.2),
        // bash's reserved words are refused where they would be one, rather
        // than run as commands that are not found.
        // The words of the spec's §4.2, written out apart from `RESERVED`.
        for word in [
            "if", "then", "elif", "else", "fi", "while", "until", "for", "in", "do", "done",
            "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break",
            "continue",
        ] {
````

with:

````rust
    fn a_reserved_word_where_a_command_name_stands_is_unsupported() {
        // bash's other reserved words are refused where they would be one
        // (programmable shell gate §4.2), rather than run as commands that
        // are not found; the loops' until they come.
        // The words of the spec's §4.2, written out apart from `RESERVED`.
        for word in [
            "while", "until", "for", "in", "do", "done", "case", "esac", "select", "function",
            "time", "coproc", "{", "}", "[[", "]]", "break", "continue",
        ] {
````

Replace:

````rust
        for (line, words) in [
            ("echo if then fi", &["echo", "if", "then", "fi"][..]),
            ("'if' x", &["if", "x"]),
            ("\\while x", &["while", "x"]),
````

with:

````rust
        for (line, words) in [
            ("echo while do done", &["echo", "while", "do", "done"][..]),
            ("'for' x", &["for", "x"]),
            ("\\while x", &["while", "x"]),
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let mut r = Reader::new();
        assert_eq!(r.add("if true"), Err(ParseError::Unsupported("if".into())));
        assert!(r.reading());
        for line in ["then echo a", "while b", "do c", "done", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
````

with:

````rust
        let mut r = Reader::new();
        assert_eq!(
            r.add("while true"),
            Err(ParseError::Unsupported("while".into()))
        );
        assert!(r.reading());
        for line in ["do echo a", "if b", "then c", "fi", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
````

Replace:

````rust
        assert_eq!(r.add("echo $x"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
    }

    #[test]
````

with:

````rust
        assert_eq!(r.add("echo $x"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
        assert!(!r.reading());
    }

    #[test]
    fn an_if_with_an_error_inside_is_dropped_to_its_fi() {
        // Nothing of it runs, the lines after the error included; the line
        // after its `fi` starts afresh (programmable shell gate §15 item 2).
        let mut r = Reader::new();
        assert_eq!(r.add("if a; then"), Ok(None));
        assert_eq!(r.add("if b; then c"), Ok(None));
        assert_eq!(r.add("d; then"), Err(ParseError::MissingTarget("then")));
        for line in ["e", "fi", "f"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
            assert!(r.reading(), "{line}");
        }
        assert_eq!(r.add("fi"), Ok(None));
        assert!(!r.reading());
        // p2: the error at its `fi` drops nothing after it, as in bash.
        assert_eq!(r.add("if true"), Ok(None));
        assert_eq!(r.add("then"), Ok(None));
        assert_eq!(r.add("fi"), Err(ParseError::MissingTarget("fi")));
        assert!(!r.reading());
        assert_eq!(names(&r.add("echo b").unwrap().unwrap()), ["echo"]);
    }

    #[test]
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
        // lines ran its body, each refused line dropped alone: the lines
        // are dropped up to its `fi`, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo t-args d\ndone\nt-args e\nfi\nt-args next\n";
````

with:

````rust
        // lines ran its body, each refused line dropped alone: the lines
        // of a construct with a refused line in it are dropped up to its
        // end, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo t-args d\ndone\nt-args e\nfi\nt-args next\n";
````

Replace:

````rust
                0,
                "+ if true\nrelay-sh: unsupported syntax: if\n+ then echo a\n+ while true\n\
                 + do echo b\n+ done\n+ fi\n+ echo next\nnext\n"
````

with:

````rust
                0,
                "+ if true\n+ then echo a\n+ while true\nrelay-sh: unsupported syntax: while\n\
                 + do echo b\n+ done\n+ fi\n+ echo next\nnext\n"
````

Replace:

````rust
            &mut h,
            &["if t-args a", "then t-args b", "fi", "t-args next"],
        );
````

with:

````rust
            &mut h,
            &["while t-args a", "do t-args b", "done", "t-args next"],
        );
````

Replace:

````rust
    #[test]
    fn a_compound_command_runs_none_of_its_parts() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("echo a; if true; then echo b; fi"),
            (2, "relay-sh: unsupported syntax: if\n".into())
        );
    }
````

with:

````rust
    #[test]
    fn an_if_runs_the_body_of_the_first_condition_that_succeeds() {
        let mut h = Harness::new();
        for (line, ran) in [
            ("if true; then echo a; fi", (0, "a\n")),
            (
                "if false; then echo a; elif true; then echo b; else echo c; fi",
                (0, "b\n"),
            ),
            (
                "if false; then echo a; elif false; then echo b; else echo c; fi",
                (0, "c\n"),
            ),
            (
                "if true; then echo a; if false; then echo b; else echo c; fi fi",
                (0, "a\nc\n"),
            ),
            // Its status is the body's (g6), 0 when none runs (g4, g5,
            // g13), and a condition's status is `$?` in the body.
            ("if true; then false; fi", (1, "")),
            ("if false; then true; fi", (0, "")),
            ("false; if false; then true; fi; echo $?", (0, "0\n")),
            ("if true && false; then echo a; fi", (0, "")),
            ("if false; then true; else echo $?; fi", (0, "1\n")),
            ("if ! true; then echo a; else echo b; fi", (0, "b\n")),
            (
                "if nosuch; then echo a; else echo b; fi",
                (0, "relay-sh: nosuch: command not found\nb\n"),
            ),
            // f3: negated, and in an and-or list (g11).
            ("! if false; then true; fi", (1, "")),
            (
                "echo a && if true; then echo b; fi || echo c",
                (0, "a\nb\n"),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
    }

    #[test]
    fn exit_ctrl_c_and_a_bad_substitution_end_an_if_and_its_line() {
        let mut h = Harness::new();
        // r7: `exit` deep inside stops the shell at once.
        assert_eq!(
            h.run("if true; then if true; then exit 3; fi; echo no; fi; echo no"),
            (3, "".into())
        );
        // A bad substitution abandons the whole line (§5.1).
        assert_eq!(
            h.run("if true; then echo ${1A}; echo no; fi; echo no"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("if echo ${1A}; then echo no; else echo no; fi"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        // Ctrl-C ends it, in its condition or its body.
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        for (line, ran) in [
            (
                "if t-spin; then t-args no; else t-args no; fi; t-args no",
                &["t-spin"][..],
            ),
            (
                "if t-args a; then t-spin; t-args no; fi; t-args no",
                &["t-args a", "t-spin"],
            ),
        ] {
            let before = h.programs.spawned.len();
            assert_eq!(h.spawning(line).1, "^C\n", "{line}");
            let args: Vec<String> = h.programs.spawned[before..]
                .iter()
                .map(|s| s.args.join(" "))
                .collect();
            assert_eq!(args, ran, "{line}");
        }
        // The in-process runner refuses a job at any depth, before any of
        // the line runs.
        for line in [
            "echo a; if true; then echo b & fi",
            "echo a; if echo b & then echo c; fi",
            "echo a; if false; then echo b; else echo c & fi",
        ] {
            assert_eq!(
                h.run(line),
                (2, "relay-sh: unsupported syntax: &\n".into()),
                "{line}"
            );
        }
    }
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `If` in this scope ``; `` cannot find type `Compound` in this scope ``.

- [ ] **Step 5: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 24 replacements, top to bottom:

Replace:

````rust
//! `||` the command goes on to the next line; text that ends there is
//! [`ParseError::Incomplete`], and a reader asks for more. bash's reserved
//! words (`if`, `while`, `{`, …) are refused where a command name would
//! stand. Every other shell feature is refused: an unquoted `*`, `?`,
//! `<`, `` ` ``, `(` or `)` is an error naming the character, instead of
//! being passed on as if it were plain text; so are `|&` (the errors into
//! the pipe too), `>&` and `2>` (another stream).

use alloc::format;
````

with:

````rust
//! `||` the command goes on to the next line; text that ends there is
//! [`ParseError::Incomplete`], and a reader asks for more. `if … then …
//! [elif … then …] [else …] fi` is a compound command, its words keywords
//! only unquoted, whole and where a command name would stand (after `fi`,
//! only a keyword or an operator may follow); it cannot stand in a pipeline
//! of several, before `&` or with a redirection. bash's other reserved
//! words (`while`, `{`, …) are refused where a command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
//! `` ` ``, `(` or `)` is an error naming the character, instead of being
//! passed on as if it were plain text; so are `|&` (the errors into the
//! pipe too), `>&` and `2>` (another stream).

use alloc::boxed::Box;
use alloc::format;
````

Replace:

````rust
/// What a pipeline runs (programmable shell gate §4.1): one command, or
/// several joined by `|`, each one's output the next one's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run<W = String> {
    Commands(Vec<Command<W>>),
}
````

with:

````rust
/// What a pipeline runs (programmable shell gate §4.1): one command, or
/// several joined by `|`, each one's output the next one's input; or one
/// compound command, which cannot stand in a pipeline of several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run<W = String> {
    Commands(Vec<Command<W>>),
    Compound(Box<Compound<W>>),
}

/// A compound command (programmable shell gate §4.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compound<W = String> {
    If(If<W>),
}

/// `if`: each condition (the `if`'s, then each `elif`'s) with the body it
/// runs, and the `else` body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct If<W = String> {
    pub branches: Vec<(List<W>, List<W>)>,
    pub otherwise: Option<List<W>>,
}

impl<W> List<W> {
    /// Whether a background job stands anywhere in it, in a compound
    /// command too.
    pub fn has_job(&self) -> bool {
        self.items.iter().any(|item| {
            item.background.is_some()
                || core::iter::once(&item.and_or.first)
                    .chain(item.and_or.rest.iter().map(|(_, p)| p))
                    .any(|p| matches!(&p.run, Run::Compound(c) if c.has_job()))
        })
    }
}

impl<W> Compound<W> {
    fn has_job(&self) -> bool {
        match self {
            Compound::If(i) => {
                i.branches.iter().any(|(c, b)| c.has_job() || b.has_job())
                    || i.otherwise.as_ref().is_some_and(List::has_job)
            }
        }
    }

    /// The word that ends it, which messages about it name.
    fn end(&self) -> &'static str {
        match self {
            Compound::If(_) => "fi",
        }
    }
}
````

Replace:

````rust

    /// The word is one of bash's reserved words, unquoted.
````

with:

````rust

    /// The keyword the word is, unquoted and whole.
    fn keyword(&self) -> Option<Keyword> {
        let [Piece::Text(t, false)] = &self.pieces[..] else {
            return None;
        };
        KEYWORDS.iter().find(|(w, _)| w == t).map(|&(_, k)| k)
    }

    /// The word is one of bash's reserved words, unquoted.
````

Replace:

````rust
    MissingTarget(&'static str),
    /// The text is the start of a command that needs more lines: it ends
````

with:

````rust
    MissingTarget(&'static str),
    /// A word where bash's grammar allows none (after `fi`), as typed.
    Unexpected(String),
    /// The text is the start of a command that needs more lines: it ends
````

Replace:

````rust
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::Incomplete => f.write_str("syntax error: unexpected end of file"),
````

with:

````rust
            ParseError::MissingTarget(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::Unexpected(t) => write!(f, "syntax error near unexpected token `{t}'"),
            ParseError::Incomplete => f.write_str("syntax error: unexpected end of file"),
````

Replace:

````rust

/// bash's reserved words, and its loop built-ins, refused where a command
/// name could stand (programmable shell gate §4.2) until the compound
/// commands are implemented.
const RESERVED: &[&str] = &[
    "if", "then", "elif", "else", "fi", "while", "until", "for", "in", "do", "done", "case",
    "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break", "continue",
];

````

with:

````rust

/// bash's other reserved words, and its loop built-ins, refused where a
/// command name could stand (programmable shell gate §4.2); the loops'
/// until they are implemented.
const RESERVED: &[&str] = &[
    "while", "until", "for", "in", "do", "done", "case", "esac", "select", "function", "time",
    "coproc", "{", "}", "[[", "]]", "break", "continue",
];

/// A compound command's word, where a command name would stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    If,
    Then,
    Elif,
    Else,
    Fi,
}

const KEYWORDS: &[(&str, Keyword)] = &[
    ("if", Keyword::If),
    ("then", Keyword::Then),
    ("elif", Keyword::Elif),
    ("else", Keyword::Else),
    ("fi", Keyword::Fi),
];

impl Keyword {
    /// As typed.
    fn token(self) -> &'static str {
        KEYWORDS
            .iter()
            .find(|&&(_, k)| k == self)
            .map_or("", |&(w, _)| w)
    }
}

/// Where a compound command being read is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// An `if`'s or an `elif`'s condition, up to its `then`.
    Condition,
    /// The body after `then`.
    Then,
    /// The body after `else`.
    Else,
}

/// A compound command being read, and what was being read around it.
struct Open {
    /// The list it stands in, put back when it closes.
    outer: Items,
    /// The `!`s before it.
    bangs: usize,
    stage: Stage,
    /// Its lists so far: each condition and the body after it.
    lists: Vec<List<Word>>,
}

````

Replace:

````rust
    later: bool,
}

impl Parts {
    /// Ends a word: it becomes the pending redirection's target or the next
    /// word.
    fn end_word(&mut self, word: &mut Building, line: &str) -> Result<(), ParseError> {
        let Some(w) = core::mem::take(word).finish(line) else {
            return Ok(());
        };
        match self.pending.take() {
````

with:

````rust
    later: bool,
    /// The compound command just read, which only an operator or a
    /// keyword may follow.
    compound: Option<Compound<Word>>,
}

impl Parts {
    /// Nothing of a command has been read (a `!` may have been).
    fn is_empty(&self) -> bool {
        self.words.is_empty() && self.redirect.is_none() && self.compound.is_none()
    }

    /// Ends a word: it becomes the pending redirection's target or the next
    /// word, or it is the keyword it returns, standing where a command name
    /// would.
    fn end_word(&mut self, word: &mut Building, line: &str) -> Result<Option<Keyword>, ParseError> {
        let Some(w) = core::mem::take(word).finish(line) else {
            return Ok(None);
        };
        // After a compound command, as in bash, only a keyword that closes
        // or goes on with the one around it may come (`fi fi`, `fi then`).
        if self.compound.is_some() {
            return match w.keyword() {
                Some(k) if k != Keyword::If => Ok(Some(k)),
                _ => Err(ParseError::Unexpected(w.typed)),
            };
        }
        match self.pending.take() {
````

Replace:

````rust
            }
            None if self.words.is_empty() && self.redirect.is_none() && w.is_reserved() => {
                return Err(ParseError::Unsupported(w.typed));
            }
            None => self.words.push(w),
        }
        Ok(())
    }
````

with:

````rust
            }
            None if self.words.is_empty() && self.redirect.is_none() => {
                if let Some(k) = w.keyword() {
                    return Ok(Some(k));
                }
                if w.is_reserved() {
                    return Err(ParseError::Unsupported(w.typed));
                }
                self.words.push(w);
            }
            None => self.words.push(w),
        }
        Ok(None)
    }
````

Replace:

````rust
    fn take_before_pipe(&mut self) -> Result<Command<Word>, ParseError> {
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
````

with:

````rust
    fn take_before_pipe(&mut self) -> Result<Command<Word>, ParseError> {
        // bash runs it in a subshell.
        if let Some(c) = &self.compound {
            return Err(ParseError::Unsupported(format!("| after {}", c.end())));
        }
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
````

Replace:

````rust
/// run no shell: tests); a line that does not expand is
/// `ParseError::Expansion`. A blank line is one command without words.
pub fn parse(line: &str) -> Result<Vec<Command>, ParseError> {
````

with:

````rust
/// run no shell: tests); a line that does not expand is
/// `ParseError::Expansion`. A blank line is one command without words; a
/// compound command has none of its own.
pub fn parse(line: &str) -> Result<Vec<Command>, ParseError> {
````

Replace:

````rust
    };
    let Run::Commands(commands) = &item.and_or.first.run;
    crate::expand::plain(commands).map_err(|e| ParseError::Expansion(e.to_string()))
````

with:

````rust
    };
    let Run::Commands(commands) = &item.and_or.first.run else {
        return Ok(Vec::new());
    };
    crate::expand::plain(commands).map_err(|e| ParseError::Expansion(e.to_string()))
````

Replace:

````rust
) -> Result<Option<Pipeline<Word>>, ParseError> {
    if parts.pending.is_some() {
````

with:

````rust
) -> Result<Option<Pipeline<Word>>, ParseError> {
    if let Some(c) = parts.compound.take() {
        let p = core::mem::take(parts);
        return Ok(Some(Pipeline {
            negated: p.bangs % 2 == 1,
            run: Run::Compound(Box::new(c)),
        }));
    }
    if parts.pending.is_some() {
````

Replace:

````rust
    word: Building,
    /// How many bytes it has read (the tests count them).
````

with:

````rust
    word: Building,
    /// The compound commands being read, the innermost last.
    open: Vec<Open>,
    /// How many bytes it has read (the tests count them).
````

Replace:

````rust
        self.read(from)?;
        if !self.pipeline.is_empty() || self.items.connector.is_some() {
            return Ok(None);
````

with:

````rust
        self.read(from)?;
        if !self.pipeline.is_empty() || self.items.connector.is_some() || !self.open.is_empty() {
            return Ok(None);
````

Replace:

````rust
    fn read_text(&mut self, line: &str, from: usize) -> Result<(), ParseError> {
        let Parser {
            items,
            item_start,
            comments,
            pipeline,
            parts,
            word,
            ..
        } = self;
        let mut cur = Cursor::starting(line, from);
````

with:

````rust
    fn read_text(&mut self, line: &str, from: usize) -> Result<(), ParseError> {
        let mut cur = Cursor::starting(line, from);
````

Replace:

````rust
            match c {
                ' ' | '\t' => parts.end_word(word, line)?,
                '>' => {
                    // `2>` redirects another stream in a real shell.
                    if word.started
                        && let Some(digits) = word.word.digits()
                    {
                        return Err(ParseError::Unsupported(format!("{digits}>")));
                    }
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() {
                        return Err(ParseError::MissingTarget(">"));
                    }
                    let append = cur.next_if_eq('>');
                    // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
````

with:

````rust
            match c {
                ' ' | '\t' => self.end_word(line, at)?,
                '>' => {
                    // `2>` redirects another stream in a real shell.
                    if self.word.started
                        && let Some(digits) = self.word.word.digits()
                    {
                        return Err(ParseError::Unsupported(format!("{digits}>")));
                    }
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget(">"));
                    }
                    let append = cur.next_if_eq('>');
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        let op = if append { ">>" } else { ">" };
                        return Err(ParseError::Unsupported(format!("{op} after {}", c.end())));
                    }
                    // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
````

Replace:

````rust
                    }
                    parts.pending = Some(append);
                }
                '|' if cur.next_if_eq('|') => {
                    parts.end_word(word, line)?;
                    join(items, parts, pipeline, Connector::Or)?;
                }
````

with:

````rust
                    }
                    self.parts.pending = Some(append);
                }
                '|' if cur.next_if_eq('|') => {
                    self.end_word(line, at)?;
                    join(
                        &mut self.items,
                        &mut self.parts,
                        &mut self.pipeline,
                        Connector::Or,
                    )?;
                }
````

Replace:

````rust
                    }
                    parts.end_word(word, line)?;
                    pipeline.push(parts.take_before_pipe()?);
                }
````

with:

````rust
                    }
                    self.end_word(line, at)?;
                    self.pipeline.push(self.parts.take_before_pipe()?);
                }
````

Replace:

````rust
                    }
                    parts.end_word(word, line)?;
                    match end_pipeline(parts, pipeline, ";")? {
                        Some(p) => items.pipeline(p),
                        None => return Err(ParseError::MissingTarget(";")),
                    }
                    items.end(None);
                    *item_start = cur.pos();
                }
                '&' if cur.next_if_eq('&') => {
                    parts.end_word(word, line)?;
                    join(items, parts, pipeline, Connector::And)?;
                }
                '&' => {
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() || parts.words.is_empty() && parts.redirect.is_none()
                    {
                        return Err(ParseError::MissingTarget("&"));
                    }
                    if parts.words.is_empty() {
                        // `> f &`: a background job is a program.
````

with:

````rust
                    }
                    self.end_word(line, at)?;
                    match end_pipeline(&mut self.parts, &mut self.pipeline, ";")? {
                        Some(p) => self.items.pipeline(p),
                        None => return Err(ParseError::MissingTarget(";")),
                    }
                    self.items.end(None);
                    self.item_start = cur.pos();
                }
                '&' if cur.next_if_eq('&') => {
                    self.end_word(line, at)?;
                    join(
                        &mut self.items,
                        &mut self.parts,
                        &mut self.pipeline,
                        Connector::And,
                    )?;
                }
                '&' => {
                    self.end_word(line, at)?;
                    // bash runs it in a subshell of its own.
                    if let Some(c) = &self.parts.compound {
                        return Err(ParseError::Unsupported(format!("& after {}", c.end())));
                    }
                    if self.parts.pending.is_some()
                        || self.parts.words.is_empty() && self.parts.redirect.is_none()
                    {
                        return Err(ParseError::MissingTarget("&"));
                    }
                    if self.parts.words.is_empty() {
                        // `> f &`: a background job is a program.
````

Replace:

````rust
                    // shell of its own.
                    if let Some(c) = items.connector {
                        return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                    }
                    // Without its `!`, as bash's `jobs` shows it.
                    let typed = job_text(line, *item_start, at, comments);
                    let mut text = typed.as_str();
                    for _ in 0..parts.bangs {
                        text = text[1..].trim_start_matches([' ', '\t']);
                    }
                    let text = String::from(text);
                    if let Some(p) = end_pipeline(parts, pipeline, "&")? {
                        items.pipeline(p);
                    }
                    // The line goes on after it, as bash's does (`a & b`).
                    items.end(Some(text));
                    *item_start = cur.pos();
                }
                '\'' => {
                    let before = word.open_quote();
                    loop {
                        match cur.next() {
                            Some('\'') => break,
                            Some(c) => word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
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
````

with:

````rust
                    // shell of its own.
                    if let Some(c) = self.items.connector {
                        return Err(ParseError::Unsupported(format!("& after {}", c.token())));
                    }
                    // Without its `!`, as bash's `jobs` shows it.
                    let typed = job_text(line, self.item_start, at, &self.comments);
                    let mut text = typed.as_str();
                    for _ in 0..self.parts.bangs {
                        text = text[1..].trim_start_matches([' ', '\t']);
                    }
                    let text = String::from(text);
                    if let Some(p) = end_pipeline(&mut self.parts, &mut self.pipeline, "&")? {
                        self.items.pipeline(p);
                    }
                    // The line goes on after it, as bash's does (`a & b`).
                    self.items.end(Some(text));
                    self.item_start = cur.pos();
                }
                '\'' => {
                    let before = self.word.open_quote();
                    loop {
                        match cur.next() {
                            Some('\'') => break,
                            Some(c) => self.word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
                    }
                    self.word.close_quote(before);
                }
                '"' => {
                    let before = self.word.open_quote();
                    loop {
                        match cur.next() {
                            Some('"') => break,
                            Some('\\') if matches!(cur.peek(), Some('"' | '\\' | '$' | '`')) => {
                                self.word.quoted(cur.next().expect("peeked"));
                            }
                            Some('$') => match parameter(&mut cur, true)? {
                                Some(p) => self.word.param(p, true),
                                None => self.word.quoted('$'),
                            },
                            Some('`') => return Err(ParseError::Unsupported('`'.into())),
                            Some(c) => self.word.quoted(c),
                            None => return Err(ParseError::UnterminatedQuote),
                        }
                    }
                    self.word.close_quote(before);
                }
````

Replace:

````rust
                    Some('\n') | None => return Err(ParseError::TrailingBackslash),
                    Some(c) => word.quoted(c),
                },
                '$' => match parameter(&mut cur, false)? {
                    Some(p) => word.param(p, false),
                    None => {
                        word.started = true;
                        word.added += 1;
                        word.word.push('$', false);
                    }
                },
                // A comment runs to the end of the line.
                '#' if !word.started => {
                    while cur.peek().is_some_and(|c| c != '\n') {
                        cur.next();
                    }
                    comments.push((at, cur.pos()));
                }
                '\n' => {
                    parts.end_word(word, line)?;
                    if parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("newline"));
````

with:

````rust
                    Some('\n') | None => return Err(ParseError::TrailingBackslash),
                    Some(c) => self.word.quoted(c),
                },
                '$' => match parameter(&mut cur, false)? {
                    Some(p) => self.word.param(p, false),
                    None => {
                        self.word.started = true;
                        self.word.added += 1;
                        self.word.word.push('$', false);
                    }
                },
                // A comment runs to the end of the line.
                '#' if !self.word.started => {
                    while cur.peek().is_some_and(|c| c != '\n') {
                        cur.next();
                    }
                    self.comments.push((at, cur.pos()));
                }
                '\n' => {
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("newline"));
````

Replace:

````rust
                    // (A `!` counts only before a pipeline's first command.)
                    let nothing = parts.words.is_empty()
                        && parts.redirect.is_none()
                        && (parts.bangs == 0 || !pipeline.is_empty());
                    if nothing && (!pipeline.is_empty() || items.connector.is_some()) {
                        continue;
                    }
                    if let Some(p) = end_pipeline(parts, pipeline, "newline")? {
                        items.pipeline(p);
                    }
                    items.end(None);
                    *item_start = cur.pos();
                }
                c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
                c => {
                    if !word.started && c == '~' {
                        word.tilde = true;
                    }
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
````

with:

````rust
                    // (A `!` counts only before a pipeline's first command.)
                    let nothing = self.parts.is_empty()
                        && (self.parts.bangs == 0 || !self.pipeline.is_empty());
                    if nothing && (!self.pipeline.is_empty() || self.items.connector.is_some()) {
                        continue;
                    }
                    if let Some(p) = end_pipeline(&mut self.parts, &mut self.pipeline, "newline")? {
                        self.items.pipeline(p);
                    }
                    self.items.end(None);
                    self.item_start = cur.pos();
                }
                c if UNSUPPORTED.contains(&c) => return Err(ParseError::Unsupported(c.into())),
                c => {
                    if !self.word.started && c == '~' {
                        self.word.tilde = true;
                    }
                    self.word.started = true;
                    self.word.added += 1;
                    self.word.word.push(c, false);
                }
            }
            if self.word.started {
                if self.word.end == 0 {
                    self.word.start = at;
                }
                self.word.end = cur.pos();
            }
````

Replace:

````rust

    /// The text ends: its list, or why it does not parse (a command that
    /// needs more lines is `ParseError::Incomplete`).
    fn end(mut self) -> Result<List<Word>, ParseError> {
        let Parser {
            text,
            items,
            pipeline,
            parts,
            word,
            ..
        } = &mut self;
        parts.end_word(word, text)?;
        if parts.pending.is_some() {
````

with:

````rust

    /// Ends the word being read, which ends at `at`; a keyword where one
    /// may stand opens, goes on with or closes a compound command.
    fn end_word(&mut self, line: &str, at: usize) -> Result<(), ParseError> {
        match self.parts.end_word(&mut self.word, line)? {
            Some(k) => self.keyword(k, at),
            None => Ok(()),
        }
    }

    /// A keyword, ending at `at`, where a command name would stand.
    fn keyword(&mut self, k: Keyword, at: usize) -> Result<(), ParseError> {
        if k == Keyword::If {
            // bash runs it in a subshell.
            if !self.pipeline.is_empty() {
                return Err(ParseError::Unsupported(format!("{} after |", k.token())));
            }
            let bangs = core::mem::take(&mut self.parts).bangs;
            self.open.push(Open {
                outer: core::mem::take(&mut self.items),
                bangs,
                stage: Stage::Condition,
                lists: Vec::new(),
            });
            self.item_start = at;
            return Ok(());
        }
        let next = match (self.open.last().map(|o| o.stage), k) {
            (Some(Stage::Condition), Keyword::Then) => Stage::Then,
            (Some(Stage::Then), Keyword::Elif) => Stage::Condition,
            (Some(Stage::Then), Keyword::Else) => Stage::Else,
            (Some(Stage::Then | Stage::Else), Keyword::Fi) => Stage::Else,
            _ => return Err(ParseError::MissingTarget(k.token())),
        };
        let list = self.end_list(k.token())?;
        self.item_start = at;
        let Some(open) = self.open.last_mut() else {
            return Err(ParseError::MissingTarget(k.token()));
        };
        if k != Keyword::Fi {
            open.lists.push(list);
            open.stage = next;
            return Ok(());
        }
        let Some(open) = self.open.pop() else {
            return Err(ParseError::MissingTarget(k.token()));
        };
        let mut lists = open.lists;
        let otherwise = match open.stage {
            Stage::Else => Some(list),
            _ => {
                lists.push(list);
                None
            }
        };
        let mut branches = Vec::new();
        let mut lists = lists.into_iter();
        while let (Some(condition), Some(body)) = (lists.next(), lists.next()) {
            branches.push((condition, body));
        }
        self.items = open.outer;
        self.parts.bangs = open.bangs;
        self.parts.compound = Some(Compound::If(If {
            branches,
            otherwise,
        }));
        Ok(())
    }

    /// The list a keyword (`token`) ends: it must hold something, and
    /// leave nothing open (bash's error names the keyword).
    fn end_list(&mut self, token: &'static str) -> Result<List<Word>, ParseError> {
        if self.parts.bangs > 0 && self.parts.is_empty() {
            return Err(ParseError::MissingTarget(token));
        }
        match end_pipeline(&mut self.parts, &mut self.pipeline, token)? {
            Some(p) => self.items.pipeline(p),
            None if self.items.connector.is_some() => return Err(ParseError::MissingTarget(token)),
            None => {}
        }
        self.items.end(None);
        let items = core::mem::take(&mut self.items).items;
        if items.is_empty() {
            return Err(ParseError::MissingTarget(token));
        }
        Ok(List { items })
    }

    /// The text ends: its list, or why it does not parse (a command that
    /// needs more lines is `ParseError::Incomplete`).
    fn end(mut self) -> Result<List<Word>, ParseError> {
        let text = core::mem::take(&mut self.text);
        self.end_word(&text, text.len())?;
        if !self.open.is_empty() {
            return Err(ParseError::Incomplete);
        }
        let Parser {
            items,
            pipeline,
            parts,
            ..
        } = &mut self;
        if parts.pending.is_some() {
````

Replace:

````rust
    // Not even after a `!`, as bash's grammar has it.
    if parts.words.is_empty() && parts.redirect.is_none() && pipeline.is_empty() {
        return Err(ParseError::MissingTarget(connector.token()));
````

with:

````rust
    // Not even after a `!`, as bash's grammar has it.
    if parts.is_empty() && pipeline.is_empty() {
        return Err(ParseError::MissingTarget(connector.token()));
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

    /// Runs a list's items one after another (programmable shell gate
    /// §5.1); its status is the last one's. An empty list keeps the last
    /// status.
    /// `exit`, Ctrl-C (status 130, spec §6.4) and an expansion that
    /// abandons the line stop the rest.
    fn run_list(&mut self, list: &parser::List<parser::Word>) -> i32 {
        self.abandoned = false;
        self.cancelled = false;
        // A background job needs programs: the in-process runner refuses
        // a line that holds one, before any of it runs.
        if self.runner.programs().is_none() && list.items.iter().any(|i| i.background.is_some()) {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
        }
        let mut status = self.status;
````

with:

````rust

    /// Runs a command line's list (programmable shell gate §5.1).
    fn run_list(&mut self, list: &parser::List<parser::Word>) -> i32 {
        self.abandoned = false;
        self.cancelled = false;
        // A background job needs programs: the in-process runner refuses
        // a line that holds one, anywhere, before any of it runs.
        if self.runner.programs().is_none() && list.has_job() {
            return self.finish(SYNTAX, format!("{NAME}: unsupported syntax: &\n"));
        }
        self.run_items(list)
    }

    /// Runs a list's items one after another; its status is the last
    /// one's. An empty list keeps the last status.
    /// `exit`, Ctrl-C (status 130, spec §6.4) and an expansion that
    /// abandons the line stop the rest.
    fn run_items(&mut self, list: &parser::List<parser::Word>) -> i32 {
        let mut status = self.status;
````

Replace:

````rust
            parser::Run::Commands(commands) => self.run_commands(commands, background),
        };
````

with:

````rust
            parser::Run::Commands(commands) => self.run_commands(commands, background),
            parser::Run::Compound(c) => {
                self.status = self.run_compound(c);
                self.status
            }
        };
````

Replace:

````rust
        self.status = i32::from(status == 0);
        self.status
    }

````

with:

````rust
        self.status = i32::from(status == 0);
        self.status
    }

    /// Runs a compound command (programmable shell gate §5.1).
    fn run_compound(&mut self, c: &parser::Compound<parser::Word>) -> i32 {
        match c {
            parser::Compound::If(i) => self.run_if(i),
        }
    }

    /// Runs an `if`: each condition in turn, up to one whose status is 0,
    /// then its body; or, if none is, the `else` body. Its status is the
    /// body's, 0 when none runs.
    fn run_if(&mut self, i: &parser::If<parser::Word>) -> i32 {
        for (condition, body) in &i.branches {
            let status = self.run_items(condition);
            if self.ends_line() {
                return status;
            }
            if status == 0 {
                return self.run_items(body);
            }
        }
        match &i.otherwise {
            Some(body) => self.run_items(body),
            None => 0,
        }
    }

````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 340 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): run if, elif and else

`if`, `then`, `elif`, `else` and `fi` become keywords where a command
name would stand, unquoted and whole; the parser keeps a stack of the
`if`s being read, and a pipeline can run one as a compound command.
Its status is the body's, 0 when no branch runs; `exit`, Ctrl-C and a
bad substitution end it and its line.

Every syntax error names bash 5.2's token: a keyword out of place, an
empty condition or body, and any word after `fi` but a keyword. An
`if` in a pipeline, before `&` or redirected is refused, and the
in-process runner refuses a job at any depth (programmable shell gate
§15 item 2). `while`, `until` and `for` stay refused.
EOF
````


### Task 8: Compound commands nest at most 32 levels deep

Spec §4.5 and decision 7: the walker recurses (`run_items` → `run_pipeline` → `run_compound` → `run_items`) on `/bin/sh`'s fixed stack, so each compound command counts a level, and one that follows `&&` or `||` one more (`Open::levels`); the opener that would pass `parser::NESTING_MAX` (32) is refused, `unsupported syntax: more than 32 levels of nesting`, and its construct is dropped to its end. The parser itself never recurses. The scenario `control` writes a script holding an `if` 32 levels deep (about 620 bytes, followed by an `expect` of the prompt so that serial does not garble it) and runs it under `/bin/sh`, and refuses a 33rd level typed at the prompt; a throwaway probe ran 1024 levels under `/bin/sh` in QEMU (`progress.md`). The red run is the shell's tests. Mutation checks: the bound off by one, counted by frames instead of levels, gone or 33, and the `&&` level left out, each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: Task 7's `Open` and `keyword`.
- Produces: `parser::NESTING_MAX: usize` (32); `Open::levels` (private); `tests/e2e/control.txt`'s nesting lines.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    }

    #[test]
    fn an_if_in_a_pipeline_with_ampersand_or_redirected_is_refused() {
````

with:

````rust
    }

    /// `n` `if`s nested, each after `a &&` if `chained`.
    fn nested(n: usize, chained: bool) -> String {
        let open = if chained {
            "a && if b; then "
        } else {
            "if b; then "
        };
        alloc::format!("{}c{}", open.repeat(n), "; fi".repeat(n))
    }

    #[test]
    fn compound_commands_nest_at_most_32_levels_deep() {
        // The walker recurses on a fixed stack (§4.5): each one counts a
        // level, and one after `&&` or `||` one more.
        let too_deep = ParseError::Unsupported("more than 32 levels of nesting".into());
        let refused = Err(too_deep.clone());
        assert!(parse_line(&nested(32, false)).is_ok());
        assert_eq!(parse_line(&nested(33, false)), refused);
        assert!(parse_line(&nested(16, true)).is_ok());
        assert_eq!(parse_line(&nested(17, true)), refused);
        let mixed = alloc::format!("{}a || {}", "if b; then ".repeat(31), nested(1, false));
        assert_eq!(parse_line(&(mixed + &"; fi".repeat(31))), refused);
        // Levels close with their constructs: one after another is none.
        assert!(parse_line(&"if b; then c; fi; ".repeat(100)).is_ok());
        assert_eq!(
            too_deep.to_string(),
            "unsupported syntax: more than 32 levels of nesting"
        );
    }

    #[test]
    fn an_if_in_a_pipeline_with_ampersand_or_redirected_is_refused() {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, replace:

````rust
    #[test]
    fn an_if_with_an_error_inside_is_dropped_to_its_fi() {
````

with:

````rust
    #[test]
    fn a_construct_nested_too_deep_is_dropped_to_its_end() {
        let mut r = Reader::new();
        for _ in 0..32 {
            assert_eq!(r.add("if b; then"), Ok(None));
        }
        assert_eq!(
            r.add("if b; then"),
            Err(ParseError::Unsupported(
                "more than 32 levels of nesting".into()
            ))
        );
        for _ in 0..33 {
            assert!(r.reading());
            assert_eq!(r.add("echo c; fi"), Ok(None));
        }
        assert!(!r.reading());
    }

    #[test]
    fn an_if_with_an_error_inside_is_dropped_to_its_fi() {
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn exit_ctrl_c_and_a_bad_substitution_end_an_if_and_its_line() {
````

with:

````rust
    #[test]
    fn an_if_32_levels_deep_runs() {
        let mut h = Harness::new();
        let line = alloc::format!(
            "{}echo deep{}",
            "if true; then ".repeat(32),
            "; fi".repeat(32)
        );
        assert_eq!(h.run(&line), (0, "deep\n".into()));
    }

    #[test]
    fn exit_ctrl_c_and_a_bad_substitution_end_an_if_and_its_line() {
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# and a script whose command goes on across lines, with its transcript.
timeout 30
````

with:

````text
# and a script whose command goes on across lines, with its transcript.
# Compound commands (plan 2): the nesting bound under /bin/sh.
timeout 30
````

Replace:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
poweroff
````

with:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
# them on /bin/sh's fixed stack, and a 33rd level is refused.
send echo 'if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then echo deep; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi' > deep.sh
expect \nroot@relay:~# $
send sh deep.sh
expect \ndeep\nroot@relay:~# $
send if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then if true; then echo deep; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi; fi
expect \nrelay-sh: unsupported syntax: more than 32 levels of nesting\nroot@relay:~# $
poweroff
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::compound_commands_nest_at_most_32_levels_deep`, `reader::tests::a_construct_nested_too_deep_is_dropped_to_its_end`.

- [ ] **Step 6: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
pub const COMMAND_MAX: usize = 64 * 1024;

````

with:

````rust
pub const COMMAND_MAX: usize = 64 * 1024;

/// How deep compound commands may nest (programmable shell gate §4.5):
/// each counts a level, and one that follows `&&` or `||` one more, as the
/// walker, which recurses on `/bin/sh`'s fixed stack, has the and-or list's
/// frame under it.
pub const NESTING_MAX: usize = 32;

````

Replace:

````rust
    outer: Items,
    /// The `!`s before it.
````

with:

````rust
    outer: Items,
    /// The levels of nesting it counts ([`NESTING_MAX`]).
    levels: usize,
    /// The `!`s before it.
````

Replace:

````rust
            }
            let bangs = core::mem::take(&mut self.parts).bangs;
            self.open.push(Open {
                outer: core::mem::take(&mut self.items),
                bangs,
````

with:

````rust
            }
            let levels = 1 + usize::from(self.items.connector.is_some());
            if self.open.iter().map(|o| o.levels).sum::<usize>() + levels > NESTING_MAX {
                return Err(ParseError::Unsupported(format!(
                    "more than {NESTING_MAX} levels of nesting"
                )));
            }
            let bangs = core::mem::take(&mut self.parts).bangs;
            self.open.push(Open {
                outer: core::mem::take(&mut self.items),
                levels,
                bangs,
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 343 tests.

- [ ] **Step 8: Run the `control` scenario**

Run: `cargo xtask test --e2e-only --scenario control`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 9: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 10: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,e2e): nest compound commands at most 32 levels deep

The walker recurses on /bin/sh's fixed stack, so each compound
command counts a level, and one that follows `&&` or `||` one more;
the `if` that would pass 32 levels is refused, `unsupported syntax:
more than 32 levels of nesting`, and its construct dropped to its end
(programmable shell gate §4.5, §15 item 2). The scenario `control`
runs a script 32 levels deep under /bin/sh and refuses a 33rd level.
EOF
````


### Task 9: An `if` dropped any way runs none of it

Spec §5.4, §5.5 and decisions 6, 12; plan 1's lesson that its worst defects ran part of a command without its guard: an `if` dropped by a syntax error in an `if` inside it, past 64 KiB, by a line that is no text or over 64 KiB, by refused syntax (`$(…)`), past the nesting bound, by Ctrl-C at `> ` or between a script's lines, or by the input's end, runs none of its lines, at the prompt, in a script and in `X | sh`, and the line after its end runs; a script traces each of its lines once, before any of it runs, blank and comment lines not. The corpus script `if.sh` checks, against bash 5.2, what each `if` runs and its status, and the scenario `control` reads one from a pipe under `/bin/sh` (`cat p.sh | sh`), whose `cat` reads the line after its `fi`, as bash's does (`X | sh` reads a byte at a time). It tests what exists, so there is no failing run: the mutation checks of Tasks 3–8 break what these tests reach.

**Files:**
- Modify: `crates/shell/src/shell.rs`
- Create: `crates/shell/tests/corpus/if.sh`
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: Tasks 3–8.
- Produces: `crates/shell/tests/corpus/if.sh`; `tests/e2e/control.txt`'s lines for a construct read from a pipe.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert!(h.exists("/tmp/d"));
    }
````

with:

````rust
        assert!(h.exists("/tmp/d"));
    }

    #[test]
    fn an_if_dropped_any_way_runs_none_of_it() {
        // Plan 1's lesson: its worst defects ran part of a command without
        // its guard. However an `if` is dropped, none of it runs, the line
        // after its end does.
        let tail = b"fi\nt-args next\n";
        let with = |head: &[u8], body: &[u8]| -> Vec<u8> {
            let mut text = b"if t-args a; then\n".to_vec();
            text.extend_from_slice(head);
            text.extend_from_slice(body);
            text.extend_from_slice(tail);
            text
        };
        let mut long = alloc::vec![b'x'; 70_000];
        long.push(b'\n');
        let deep: Vec<u8> = b"if t-args a; then\n".repeat(33);
        for (text, said) in [
            // A syntax error in an `if` inside it.
            (
                with(
                    b"if t-args b; then t-args c\nt-args d; then\nt-args e\nfi\n",
                    b"",
                ),
                "relay-sh: syntax error near unexpected token `then'\n",
            ),
            // Too long, a line that is no text, a line over 64 KiB.
            (
                with(&b"t-args b\n".repeat(8000), b""),
                "relay-sh: the command would be longer than 64 KiB\n",
            ),
            (
                with(b"\xff\n", b"t-args b\n"),
                "sh: standard input: not a text line\n",
            ),
            (
                with(&long, b"t-args b\n"),
                "sh: standard input: a line over 64 KiB\n",
            ),
            // Refused syntax, and nesting past the bound.
            (
                with(b"t-args $(b)\n", b"t-args c\n"),
                "relay-sh: unsupported syntax: $(\n",
            ),
            (
                with(&deep, &b"fi\n".repeat(33)),
                "relay-sh: unsupported syntax: more than 32 levels of nesting\n",
            ),
        ] {
            let mut h = spawning();
            let mut input = crate::Bytes::new(text.clone());
            Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
                .run_input(&mut input);
            let ran: Vec<String> = h
                .programs
                .spawned
                .iter()
                .map(|s| s.args[1..].join(" "))
                .collect();
            assert_eq!(
                (ran, h.console.take()),
                (alloc::vec![String::from("next")], String::from(said)),
                "{said}"
            );
        }
        // The input's end inside it.
        let mut h = spawning();
        let mut input = crate::Bytes::new(b"if t-args a; then\nt-args b\n".to_vec());
        let status = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs)
            .run_input(&mut input);
        assert_eq!(
            (status, h.console.take(), h.programs.spawned.len()),
            (
                2,
                "relay-sh: syntax error: unexpected end of file\n".into(),
                0
            )
        );
    }

    #[test]
    fn a_script_s_if_is_traced_whole_before_it_runs_or_is_dropped() {
        // §5.4: each line once, as read; blank and comment lines not.
        let mut h = Harness::new();
        for (script, out, status) in [
            (
                &b"if true\nthen echo a\n\n  # c\nfi\necho b\n"[..],
                "+ if true\n+ then echo a\n+ fi\na\n+ echo b\nb\n",
                0,
            ),
            (
                b"if true; then\necho a; then\necho b\nfi\necho next\n",
                "+ if true; then\n+ echo a; then\n\
                 relay-sh: syntax error near unexpected token `then'\n\
                 + echo b\n+ fi\n+ echo next\nnext\n",
                0,
            ),
            (
                b"if true; then\necho a\n",
                "+ if true; then\n+ echo a\nrelay-sh: syntax error: unexpected end of file\n",
                2,
            ),
        ] {
            h.put("/tmp/s.sh", script);
            assert_eq!(h.run("sh /tmp/s.sh"), (status, String::from(out)));
        }
        // Ctrl-C between its lines ends the script and runs none of it.
        h.put("/tmp/s.sh", b"if true; then\necho a\nfi\necho b\n");
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ if true; then\n^C\n".into()));
    }

    #[test]
    fn an_if_typed_at_the_prompt_runs_only_whole() {
        // Ctrl-C at `> ` drops it; a `fi` after it is bash's syntax error.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &["if t-args a; then", "t-args b", "\x03", "fi", "t-args next"],
        );
        let ran: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(ran, ["next"]);
        assert!(
            out.contains("^C") && out.contains("syntax error near unexpected token `fi'"),
            "{out}"
        );
        // A syntax error inside it drops it to its `fi`, with `> ` until then.
        let mut h = spawning();
        let out = typed(
            &mut h,
            &[
                "if t-args a; then",
                "t-args b; then",
                "t-args c",
                "fi",
                "t-args next",
            ],
        );
        let ran: Vec<&str> = h
            .programs
            .spawned
            .iter()
            .map(|s| s.args[1].as_str())
            .collect();
        assert_eq!(ran, ["next"]);
        assert_eq!(out.matches("\n> ").count(), 3, "{out}");
        // The input's end at `> `.
        let mut h = spawning();
        let out = typed(&mut h, &["if t-args a; then", "t-args b"]);
        assert!(h.programs.spawned.is_empty());
        assert!(
            out.ends_with("relay-sh: syntax error: unexpected end of file\n"),
            "{out}"
        );
    }
````

- [ ] **Step 2: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, replace:

````text
expect \nrelay-sh: unsupported syntax: more than 32 levels of nesting\nroot@relay:~# $
poweroff
````

with:

````text
expect \nrelay-sh: unsupported syntax: more than 32 levels of nesting\nroot@relay:~# $
# A construct read from a pipe leaves what follows it to its commands,
# as bash's does: the `if`'s `cat` reads the line after its `fi`.
send echo 'if true; then' > p.sh
expect \nroot@relay:~# $
send echo cat >> p.sh
expect \nroot@relay:~# $
send echo fi >> p.sh
expect \nroot@relay:~# $
send echo hello >> p.sh
expect \nroot@relay:~# $
send cat p.sh | sh
expect \nhello\nroot@relay:~# $
poweroff
````

- [ ] **Step 3: Create `crates/shell/tests/corpus/if.sh`**

Create `crates/shell/tests/corpus/if.sh`:

````bash
# if, elif and else: the first condition whose status is 0 runs its
# body; the status is the body's, 0 when no branch runs.
if true; then echo then1; fi
if false; then echo no; fi; echo $?
if false; then echo no; else echo else1; fi
if false; then echo no; elif true; then echo elif1; else echo no; fi
if false; then echo no; elif false; then echo no; else echo else2; fi
if false; then echo no; elif false; then echo no; fi; echo $?
if true; then false; fi; echo $?
if false; then true; else seq 0; false; fi; echo $?
false; if false; then true; fi; echo $?
# A condition is a list; its status is $? in the body.
if false; true; then echo list1; fi
if true && false; then echo no; else echo $?; fi
if false || true; then echo or1; fi
if seq 3 | grep 2; then echo pipe1; fi
if ! false; then echo bang1; fi
! if false; then true; fi; echo $?
# In lists and and-or lists, and nested.
echo before; if true; then echo inside; fi; echo after
true && if true; then echo and1; fi || echo no
if false; then echo no; fi || echo or2
if true; then if false; then echo no; else echo nested1; fi; fi
if if true; then false; fi; then echo no; else echo nested2; fi
if true; then if true; then echo nested3; fi fi
# Across lines, blank and comment lines among them.
if true
then
  echo lines1

  # a comment
elif false
then
  echo no
else
  echo no
fi
if
  false
then echo no; else echo lines2; fi
# Its words are keywords only where a command name stands.
echo if then elif else fi
A=fi; echo $A
if true; then echo "then" 'fi'; fi
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 346 tests.

- [ ] **Step 5: Run the `control` scenario**

Run: `cargo xtask test --e2e-only --scenario control`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
test(shell,e2e): drop an if any way, run it whole as bash does

An `if` dropped by a syntax error in an `if` inside it, past 64 KiB,
by a line that is no text or over 64 KiB, by refused syntax, past the
nesting bound, by Ctrl-C at `> ` or between a script's lines, or by
the input's end, runs none of its lines, at the prompt, in a script
and in `X | sh`; a script traces each of its lines once, before it
runs (programmable shell gate §5.4, §15 item 2). The corpus script
`if.sh` checks what runs, and the statuses, against bash 5.2.
The scenario `control` reads one from a pipe under /bin/sh: its `cat`
reads the line after its `fi`, as bash's does.
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 48 scenario(s) passed`.

````bash
git push -u origin m4p2/if
gh pr create --base main --head m4p2/if --title "feat(shell): run if, read across lines in context" --body-file - <<'EOF'
## What

Milestone 4, plan 2, Tasks 1–9: a pipeline holds a `Run`; the parser keeps a command's state from one line to the next, each line read once and an error told on its own line; a command dropped before its end is dropped to the end of its constructs, the scan reading refused syntax and here-documents as bash does; `if`, `elif` and `else` run with bash's statuses and syntax errors, refused in a pipeline, before `&` or redirected; at most 32 levels of nesting; nothing of a dropped `if` runs, at the prompt, in a script or in `X | sh`; the corpus's `if.sh`; `while`, `until` and `for` stay refused.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `control`

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; no check script uses a compound command, and plan 4's NUC check 6 runs them there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p2/if --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-if
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: The Ctrl-C check, `while`, `until` and `for` (Tasks 10–18)

`wait(0, WAIT_NOHANG | WAIT_CTRL_C)` asks for a raw Ctrl-C; the walker asks before each command, so a loop of built-ins ends, and `$?` after Ctrl-C is bash's; `while`, `until` and `for` run. With the review's fixes: a Ctrl-C typed after a program on the same line kept for the shell, which takes the console back after each program, and a job in a `for`'s body given a text of its own.

Branch `m4p2/loops`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-loops`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p2/loops /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-loops origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-loops
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p2/if` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p2/loops /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-loops m4p2/if`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p2/if>` and re-run `cargo xtask ci` before pushing.

### Task 10: `wait` can ask whether a Ctrl-C was typed

Spec §5.2 and decision 10: the interactive `/bin/sh` keeps the console in raw mode while it runs its own commands, where a Ctrl-C is a byte it would read only at its next prompt, and the kernel told it of one only to a `wait` that blocks (`WAIT_CTRL_C`). `wait(0, WAIT_NOHANG | WAIT_CTRL_C)`, which was `EINVAL`, now only asks: `EINTR` if a raw Ctrl-C was typed for the caller's group (taken, through the new `Caller::take_ctrl_c`), 0 if not, and no child is collected; pid 0 with any other flags stays `EINVAL`. `relay_abi::spawn::WAIT_CTRL_C`'s comment says so; the ABI's values and its version (3) do not change. The red run is the kernel's tests: pid 0 is `EINVAL`. Mutation checks: the flags compared loosely (either alone taken), the Ctrl-C never reported, and the test double not taking it, each fail the test.

**Files:**
- Modify: `crates/relay-abi/src/spawn.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/syscall.rs`
- Modify: `kernel/src/syscall/testing.rs`

**Interfaces:**
- Consumes: the kernel's `syscall::wait`, `Caller`, `proc::holds_ctrl_c`.
- Produces: `Caller::take_ctrl_c(&mut self) -> bool` (the kernel's `Current` and the test `Fake`).

- [ ] **Step 1: Add the failing tests to `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, replace:

````rust
    #[test]
    fn wait_refuses_what_it_cannot_mean() {
````

with:

````rust
    #[test]
    fn wait_for_pid_0_asks_only_whether_ctrl_c_was_typed() {
        // A shell between its own commands (programmable shell gate §5.2):
        // the Ctrl-C is taken, and no child is collected.
        let mut f = fake();
        f.ended = vec![(7, WaitStatus::exited(3))];
        let ask = u64::from(WAIT_NOHANG | WAIT_CTRL_C);
        assert_eq!(call(&mut f, Call::Wait, [0, ask, 0]), Ok(0), "none typed");
        f.ctrl_c_typed = true;
        assert_eq!(call(&mut f, Call::Wait, [0, ask, W]), Err(errno::EINTR));
        assert_eq!(call(&mut f, Call::Wait, [0, ask, 0]), Ok(0), "taken");
        assert_eq!(f.ended.len(), 1, "no child collected");
        // Only with both flags.
        f.ctrl_c_typed = true;
        for flags in [0, WAIT_NOHANG, WAIT_CTRL_C] {
            assert_eq!(
                call(&mut f, Call::Wait, [0, u64::from(flags), 0]),
                Err(errno::EINVAL),
                "{flags}"
            );
        }
        assert!(f.ctrl_c_typed, "not taken");
    }

    #[test]
    fn wait_refuses_what_it_cannot_mean() {
````

- [ ] **Step 2: Extend the test support in `kernel/src/syscall/testing.rs`**

In `kernel/src/syscall/testing.rs`, replace:

````rust
    }
    fn kill(&mut self, target: i64) -> Result<(), Errno> {
````

with:

````rust
    }
    fn take_ctrl_c(&mut self) -> bool {
        core::mem::take(&mut self.ctrl_c_typed)
    }
    fn kill(&mut self, target: i64) -> Result<(), Errno> {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` method `take_ctrl_c` is not a member of trait `Caller` ``.

- [ ] **Step 4: Change `crates/relay-abi/src/spawn.rs`**

In `crates/relay-abi/src/spawn.rs`, replace:

````rust
/// (a shell's `wait` built-in, spec §9.2, §16 item 9). Without it a raw
/// Ctrl-C is only input, which the shell would read after the wait.
pub const WAIT_CTRL_C: u32 = 2;
````

with:

````rust
/// (a shell's `wait` built-in, spec §9.2, §16 item 9). Without it a raw
/// Ctrl-C is only input, which the shell would read after the wait. With
/// [`WAIT_NOHANG`] and pid 0, `wait` only asks whether such a Ctrl-C was
/// typed: `EINTR` if one was, which it takes, 0 if not; no child is
/// collected (a shell between its own commands, programmable shell gate
/// §15 item 2).
pub const WAIT_CTRL_C: u32 = 2;
````

- [ ] **Step 5: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
````

with:

````rust

    fn take_ctrl_c(&mut self) -> bool {
        holds_ctrl_c()
    }

    fn kill(&mut self, target: i64) -> Result<(), Errno> {
````

- [ ] **Step 6: Change `kernel/src/syscall.rs`**

In `kernel/src/syscall.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    ) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Kills a process, or a group for a negative `target`.
````

with:

````rust
    ) -> Result<Option<(u32, WaitStatus)>, Errno>;
    /// Whether a Ctrl-C was typed while the program's group has the console
    /// in raw mode; it is taken.
    fn take_ctrl_c(&mut self) -> bool;
    /// Kills a process, or a group for a negative `target`.
````

Replace:

````rust
/// memory is checked before a child is collected, so a bad pointer never
/// loses a child's status.
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
````

with:

````rust
/// memory is checked before a child is collected, so a bad pointer never
/// loses a child's status. `wait(0, WAIT_NOHANG | WAIT_CTRL_C)` only asks
/// whether a Ctrl-C was typed (`EINTR`, taken; 0 if not), as a shell does
/// between its own commands (programmable shell gate §15 item 2).
fn wait(caller: &mut impl Caller, pid: i64, flags: u64, addr: u64) -> Result<u64, Errno> {
````

Replace:

````rust
        p if p > 0 => Child::Pid(u32::try_from(p).map_err(|_| Errno::ECHILD)?),
        _ => return Err(Errno::EINVAL),
````

with:

````rust
        p if p > 0 => Child::Pid(u32::try_from(p).map_err(|_| Errno::ECHILD)?),
        0 if flags == u64::from(WAIT_NOHANG | WAIT_CTRL_C) => {
            return if caller.take_ctrl_c() {
                Err(Errno::EINTR)
            } else {
                Ok(0)
            };
        }
        _ => return Err(Errno::EINVAL),
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 392 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates kernel
git commit -F - <<'EOF'
feat(kernel,relay-abi): let wait ask whether a Ctrl-C was typed

`wait(0, WAIT_NOHANG | WAIT_CTRL_C)` returns `EINTR` if a Ctrl-C was
typed while the caller's group has the console in raw mode, taking it,
and 0 if not, collecting no child. An interactive shell asks so between
its own commands, so that a loop of built-ins ends at a Ctrl-C
(programmable shell gate §5.2, §15 item 2). Pid 0 was `EINVAL`, which
it stays with any other flags: no caller changes meaning, and the ABI's
version stays 3.
EOF
````


### Task 11: Ctrl-C ends the shell's own commands between them

Spec §5.2 and decision 9: the walker asks the console for a Ctrl-C before each pipeline and each compound command (`run_pipeline`), prints `^C` and ends the command line with 130, so a list of built-ins ends too (and Task 12's loops); `relay-rt`'s `SysConsole::interrupted` asks the kernel (`relay_rt::sys::take_ctrl_c`, Task 10's call) for an interactive shell only, as only it keeps the console in raw mode while its commands run. After Ctrl-C ends a command line `$?` is bash's 130, but for a lone pipeline of simple commands, which keeps its own (probes c1–c19: `! sleep 5` gives 0, `! sleep 5; echo x` 130), which fixes plan 1's deferred minor; bash's lone `if` going on to its `else` (c6) is a decided difference (spec §10). The test console takes a Ctrl-C when it reports one, as the real one does now, and every test that pressed one before its command now presses it after the shell's check (`interrupt_after`), so it still reaches the command it was meant for. The red run is the shell's tests. Mutation checks: the check dropped, the line not ended after it, the `$?` rule dropped or made unconditional, and each half of the lone-pipeline test, each fail a test; `SysConsole`'s `interactive` test only spares programs a system call (an equivalent mutant).

**Files:**
- Modify: `crates/relay-rt/src/sys.rs`
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/change.rs`
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/commands/seq.rs`
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 10's `wait(0, …)`; plan 1's `Shell.cancelled`, `runner::Ran::cancelled`.
- Produces: `relay_rt::sys::take_ctrl_c() -> bool`; `SysConsole::interrupted`; `shell::lone_pipeline(&List<Word>) -> bool` (private); `TestConsole::interrupted` takes its Ctrl-C.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/change.rs`**

In `crates/shell/src/commands/change.rs`, replace:

````rust
        h.run("mkdir -p /tmp/tree/a/b");
        h.console.interrupt = true;
        assert_eq!(h.run("cp /tmp/big /tmp/copy"), (130, "^C\n".into()));
        assert_eq!(h.run("rm -r /tmp/tree"), (130, "^C\n".into()));
````

with:

````rust
        h.run("mkdir -p /tmp/tree/a/b");
        // Typed once the shell's check before the command has passed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cp /tmp/big /tmp/copy"), (130, "^C\n".into()));
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("rm -r /tmp/tree"), (130, "^C\n".into()));
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
        );
        h.console.interrupt = true;
        assert_eq!(h.run("grep x /tmp/big"), (130, "^C\n".into()));
````

with:

````rust
        );
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("grep x /tmp/big"), (130, "^C\n".into()));
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        h.put("/tmp/s.sh", b"echo a &&\necho b\n");
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a &&\n^C\n".into()));
````

with:

````rust
        h.put("/tmp/s.sh", b"echo a &&\necho b\n");
        // Asked before `sh` runs and before each line.
        h.console.interrupt_after = Some(2);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a &&\n^C\n".into()));
````

Replace:

````rust
        h.put("/tmp/s.sh", b"echo a\necho b\necho c\n");
        // Asked once before each line: the second time Ctrl-C was pressed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
````

with:

````rust
        h.put("/tmp/s.sh", b"echo a\necho b\necho c\n");
        // Asked before `sh` runs, before each line and before its command:
        // Ctrl-C is pressed before the second line.
        h.console.interrupt_after = Some(3);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ echo a\na\n^C\n".into()));
````

Replace:

````rust
        h.put("/tmp/s.sh", b"cat /tmp/big > /tmp/copy\necho after\n");
        h.console.interrupt = false;
        h.console.interrupt_after = Some(1);
        assert_eq!(
````

with:

````rust
        h.put("/tmp/s.sh", b"cat /tmp/big > /tmp/copy\necho after\n");
        h.console.interrupt_after = Some(3);
        assert_eq!(
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/commands/seq.rs`**

In `crates/shell/src/commands/seq.rs`, replace:

````rust
        let mut h = Harness::new();
        h.console.interrupt_after = Some(3);
        assert_eq!(
````

with:

````rust
        let mut h = Harness::new();
        // Asked once before the command, then before each number.
        h.console.interrupt_after = Some(4);
        assert_eq!(
````

- [ ] **Step 5: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
        h.put("/tmp/big", numbered(20_000).as_bytes());
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big /etc/motd"), (130, "^C\n".into()));
        h.console.interrupt = false;
        assert_eq!(h.run("cat /etc/hostname"), (0, "relay\n".into()));
````

with:

````rust
        h.put("/tmp/big", numbered(20_000).as_bytes());
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big /etc/motd"), (130, "^C\n".into()));
        assert_eq!(h.run("cat /etc/hostname"), (0, "relay\n".into()));
````

- [ ] **Step 6: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
        h.put("/tmp/s.sh", b"if true; then\necho a\nfi\necho b\n");
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ if true; then\n^C\n".into()));
````

with:

````rust
        h.put("/tmp/s.sh", b"if true; then\necho a\nfi\necho b\n");
        h.console.interrupt_after = Some(2);
        assert_eq!(h.run("sh /tmp/s.sh"), (130, "+ if true; then\n^C\n".into()));
````

Replace:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big | wc -c"), (130, "^C\n".into()));
        // The rest does not run, even what would not have asked.
        assert_eq!(h.run("cat /tmp/big | echo after"), (130, "^C\n".into()));
````

with:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        // Typed once the shell's check before the pipeline has passed.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big | wc -c"), (130, "^C\n".into()));
        // The rest does not run, even what would not have asked.
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big | echo after"), (130, "^C\n".into()));
````

Replace:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        assert_eq!(h.run("cat /tmp/big | wc -c; echo no"), (130, "^C\n".into()));
````

with:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cat /tmp/big | wc -c; echo no"), (130, "^C\n".into()));
````

Replace:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt = true;
        // 130 is no success, but `||` does not run after Ctrl-C.
````

with:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        h.console.interrupt_after = Some(1);
        // 130 is no success, but `||` does not run after Ctrl-C.
````

Replace:

````rust
        assert_eq!(h.programs.spawned.len(), 5);
        // A pipeline a stage of which Ctrl-C killed, too.
        assert_eq!(
            h.spawning("t-spin | t-args x; t-args no"),
            (0, "^C\n".into())
        );
````

with:

````rust
        assert_eq!(h.programs.spawned.len(), 5);
        // A pipeline a stage of which Ctrl-C killed, too; the line's `$?`
        // is then bash's 130.
        assert_eq!(
            h.spawning("t-spin | t-args x; t-args no"),
            (130, "^C\n".into())
        );
````

Replace:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        // `$?` is the negated 130, as in bash, and the rest does not run.
        h.console.interrupt = true;
        assert_eq!(h.run("! cat /tmp/big | wc -c; echo no"), (0, "^C\n".into()));
        // Only that line: the next one runs whole.
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
````

with:

````rust
        h.put("/tmp/big", &alloc::vec![b'x'; 300_000]);
        // `$?` is 130, as in bash, and the rest does not run.
        h.console.interrupt_after = Some(2);
        assert_eq!(
            h.run("! cat /tmp/big | wc -c; echo no"),
            (130, "^C\n".into())
        );
        h.console.interrupt_after = Some(2);
        assert_eq!(h.run("! cat /tmp/big | wc -c"), (0, "^C\n".into()), "alone");
        // Only that line: the next one runs whole.
        h.console.interrupt_after = Some(2);
        let mut shell = Shell::new(&mut h.vfs, &mut h.console, &mut h.system);
````

Replace:

````rust
        h.put("/tmp/s.sh", b"! cat /tmp/big | wc -c\necho no\n");
        h.console.interrupt_after = Some(1);
        assert_eq!(
````

with:

````rust
        h.put("/tmp/s.sh", b"! cat /tmp/big | wc -c\necho no\n");
        h.console.interrupt_after = Some(3);
        assert_eq!(
````

Replace:

````rust
            "the script ends there"
        );
    }

````

with:

````rust
            "the script ends there"
        );
    }

    #[test]
    fn ctrl_c_ends_the_shell_s_own_commands_between_them() {
        // §5.2: the walker asks before each command, so a list or a
        // loop of built-ins ends too, with `^C` and 130.
        let mut h = Harness::new();
        h.console.interrupt_after = Some(1);
        assert_eq!(h.run("cd; cd; cd; echo no"), (130, "^C\n".into()));
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("if cd; then cd; echo no; fi; echo no"),
            (130, "^C\n".into())
        );
        // Taken by the shell: the next line runs whole.
        h.console.interrupt_after = None;
        assert_eq!(h.run("cd; echo a"), (0, "a\n".into()));
    }

    #[test]
    fn after_ctrl_c_ends_a_command_line_its_status_is_130() {
        // Plan 1's deferred minor: bash's `$?` is 130 after Ctrl-C ends a
        // list, an and-or list (c1, c3, c8, c9, c17, c18) or a compound
        // command (c4, c7, c10–c16), and a lone pipeline's own (c2:
        // `! sleep 5` is 0). bash's lone `if` goes on to its `else` (c6);
        // §5.2 ends it.
        let mut h = Harness::new();
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        h.programs
            .known
            .push(("/bin/t-args", WaitStatus::exited(0)));
        for (line, status) in [
            ("t-spin; t-args no", 130),
            ("! t-spin; t-args no", 130),
            ("t-spin && t-args no", 130),
            ("t-spin || t-args no", 130),
            ("! t-spin && t-args no", 130),
            ("! t-spin || t-args no", 130),
            ("if t-spin; then t-args no; else t-args no; fi", 130),
            ("! if t-spin; then t-args no; fi", 130),
            ("t-spin", 130),
            ("! t-spin", 0),
        ] {
            assert_eq!(h.spawning(line), (status, "^C\n".into()), "{line}");
        }
        assert!(h.programs.spawned.iter().all(|s| s.path == "/bin/t-spin"));
    }

````

- [ ] **Step 7: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, replace:

````rust
    }
    fn interrupted(&mut self) -> bool {
        match &mut self.interrupt_after {
            Some(0) => self.interrupt = true,
            Some(n) => *n -= 1,
            None => {}
        }
        self.interrupt
    }
````

with:

````rust
    }
    /// As the real console's, a Ctrl-C is taken by the call that sees it.
    fn interrupted(&mut self) -> bool {
        match &mut self.interrupt_after {
            Some(0) => {
                self.interrupt_after = None;
                self.interrupt = true;
            }
            Some(n) => *n -= 1,
            None => {}
        }
        core::mem::take(&mut self.interrupt)
    }
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 8 tests fail, among them `commands::script::tests::ctrl_c_while_a_command_is_read_ends_the_script_there`, `commands::script::tests::ctrl_c_stops_the_script`.

- [ ] **Step 9: Change `crates/relay-rt/src/sys.rs`**

In `crates/relay-rt/src/sys.rs`, replace:

````rust

/// Kills the process `target`, or the process group `-target`.
````

with:

````rust

/// Whether a Ctrl-C was typed while this program's group has the console
/// in raw mode; it is taken (`wait(0, WAIT_NOHANG | WAIT_CTRL_C)`).
pub fn take_ctrl_c() -> bool {
    use relay_abi::spawn::{WAIT_CTRL_C, WAIT_NOHANG};
    wait_with(0, WAIT_NOHANG | WAIT_CTRL_C) == Err(relay_abi::errno::EINTR)
}

/// Kills the process `target`, or the process group `-target`.
````

- [ ] **Step 10: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        sys::console_size().0 as usize
    }
````

with:

````rust
        sys::console_size().0 as usize
    }

    /// Only an interactive shell keeps the console in raw mode while its
    /// own commands run, a Ctrl-C there being input to ask the kernel for;
    /// another program's group has it in line mode, where Ctrl-C kills it.
    fn interrupted(&mut self) -> bool {
        self.interactive && sys::take_ctrl_c()
    }
````

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        }
        self.run_items(list)
    }
````

with:

````rust
        }
        let status = self.run_items(list);
        // bash's `$?` after Ctrl-C ends a command line is 130, but for a
        // lone pipeline of simple commands, which keeps its own (`! sleep
        // 5` gives 0).
        if self.cancelled && !lone_pipeline(list) {
            self.status = CANCELLED;
            return CANCELLED;
        }
        status
    }
````

Replace:

````rust
    ) -> i32 {
        let status = match &typed.run {
````

with:

````rust
    ) -> i32 {
        // A Ctrl-C typed while the shell runs its own commands, which no
        // program's end reports: a loop of them ends too (§5.2).
        if self.console.interrupted() {
            self.cancelled = true;
            return self.finish(CANCELLED, String::from("^C\n"));
        }
        let status = match &typed.run {
````

Replace:

````rust
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
    }
}
````

with:

````rust
            self.say(format!("{NAME}: sync failed: {e}\n").as_bytes());
        }
    }
}

/// Whether `list` is one pipeline of simple commands, and nothing else.
fn lone_pipeline(list: &parser::List<parser::Word>) -> bool {
    match &list.items[..] {
        [item] => {
            item.and_or.rest.is_empty() && matches!(item.and_or.first.run, parser::Run::Commands(_))
        }
        _ => false,
    }
}
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 348 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(relay-rt,shell): end the shell's own commands at a Ctrl-C

The walker asks for a Ctrl-C before each pipeline and compound
command, printing `^C` and ending the command line with 130, so a list
or loop of built-ins ends too; an interactive /bin/sh asks the kernel
(`wait(0, WAIT_NOHANG | WAIT_CTRL_C)`), which takes the Ctrl-C, and the
test console takes it as well (programmable shell gate §5.2).

After Ctrl-C ends a command line `$?` is bash's 130, but for a lone
pipeline of simple commands, which keeps its own (`! sleep 5` gives
0): plan 1 left `! sleep 5; echo x` at 0 (§15 item 2).
EOF
````


### Task 12: `while` and `until` run

Spec §4.1, §5.1, §5.2 and decisions 2, 4, 5, 8: `while`, `until`, `do` and `done` become keywords, the compound commands `Compound::While(Loop)` and `Compound::Until(Loop)` (`Loop { condition, body }`), read by the construct stack (`Kind`, the stages `Condition` and `Body`). `run_loop` runs the condition, then the body while its status is 0 (`until`: not 0); the status is the body's last, 0 when it never runs (g3, g7, r8), and `exit`, Ctrl-C and a bad substitution end it in its condition or its body. A keyword out of place is bash's error naming it (`while a; then`, `if a; do`, `done done`), and a loop is refused in a pipeline, before `&` or redirected. A loop a person writes may run for ever; Ctrl-C ends it: the scenario `control` ends `while cd; do cd; done` and `while true; do sleep 1; done` with Ctrl-C under `/bin/sh`, `$?` 130. The test console fails a test that asks for a Ctrl-C more than 100,000 times (`ASKED_MAX`), so a loop that never ends fails instead of hanging. The tests that refused `while` refuse `coproc` inside a loop instead. The red run is the shell's tests, which cannot find `Compound::While`. Mutation checks: each stage's transition by kind, `until` built as `while`, a keyword that opens taken after `done`, a job in a loop's body or condition not seen, `done`'s word, the condition's sense, the stops after the condition and the body (one only ended by a timeout before `ASKED_MAX`), and the status before the first pass, each fail a test; `SysConsole::interrupted` answering false fails `control`.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: Task 7's construct stack; Task 11's check.
- Produces: `parser::Compound::{While, Until}(Loop<W>)`, `parser::Loop<W> { condition: List<W>, body: List<W> }`, `Keyword::{While, Until, Do, Done}`, `Kind`, `Stage::Body`, `Keyword::opens` (private); `Shell::run_loop(&Loop<Word>, until: bool) -> i32` (private); `testing::ASKED_MAX`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
                        Compound::If(_) => String::from("if"),
                    },
````

with:

````rust
                        Compound::If(_) => String::from("if"),
                        Compound::While(_) => String::from("while"),
                        Compound::Until(_) => String::from("until"),
                    },
````

Replace:

````rust
                Compound::If(i) => (p.negated, i),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
````

with:

````rust
                Compound::If(i) => (p.negated, i),
                _ => panic!("{line}: no if"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    /// The loop of `line`'s one item, `until` or not, its condition's and
    /// body's command names.
    fn loop_of(line: &str) -> (bool, Vec<String>, Vec<String>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        match p.run {
            Run::Compound(c) => match *c {
                Compound::While(l) => (false, names_of(&l.condition), names_of(&l.body)),
                Compound::Until(l) => (true, names_of(&l.condition), names_of(&l.body)),
                Compound::If(_) => panic!("{line}: an if"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    #[test]
    fn while_and_until_hold_a_condition_and_a_body() {
        let s = |v: &[&str]| -> Vec<String> { v.iter().map(|x| String::from(*x)).collect() };
        assert_eq!(
            loop_of("while a; b; do c; d; done"),
            (false, s(&["a", "b"]), s(&["c", "d"]))
        );
        assert_eq!(
            loop_of("until a && b; do c | d; done"),
            (true, s(&["a", "b"]), s(&["c"]))
        );
        // Across lines (l2, p5), nested with `if` (p8, p9), and after `done`
        // a keyword with no `;`.
        assert_eq!(
            loop_of("while\na\n\n# c\ndo b\ndone"),
            (false, s(&["a"]), s(&["b"]))
        );
        assert_eq!(
            loop_of("while a; do if b; then c; fi done"),
            (false, s(&["a"]), s(&["if"]))
        );
        assert_eq!(
            loop_of("until while a; do b; done do c; done"),
            (true, s(&["while"]), s(&["c"]))
        );
        let (_, i) = if_of("if a; then while b; do c; done fi");
        assert_eq!(branches(&i), [["a"].as_slice(), &["while"]]);
        // `!`, lists and and-or lists.
        assert!(
            parse_line("! while a; do b; done").unwrap().items[0]
                .and_or
                .first
                .negated
        );
        let list = parse_line("a && while b; do c; done || d; until e; do f; done").unwrap();
        assert_eq!(names_of(&list), ["a", "while", "d", "until"]);
        // As words where no command name stands.
        assert_eq!(
            parse("echo while until do done").unwrap()[0].words,
            ["echo", "while", "until", "do", "done"]
        );
        for text in [
            "while",
            "while a",
            "while a;",
            "while a; do",
            "until a; do b",
            "while a; do b; done &&",
        ] {
            assert_eq!(parse_line(text), Err(ParseError::Incomplete), "{text}");
        }
    }

    #[test]
    fn a_loop_s_keyword_out_of_place_is_bash_s_syntax_error() {
        for (line, token) in [
            ("done", "done"),
            ("do", "do"),
            ("while true; done", "done"),
            ("while do echo; done", "do"),
            ("while false; do echo; done done", "done"),
            ("while a; do; echo; done", ";"),
            ("while a; then b; done", "then"),
            ("until a; do b; fi", "fi"),
            ("if a; then b; done", "done"),
            ("if a; do b; fi", "do"),
            ("while a; do b; done while", "while"),
            ("while a; do b; done echo", "echo"),
            ("while a; do b; else c; done", "else"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        for (line, what) in [
            ("while a; do b; done | cat", "| after done"),
            ("echo x | while a; do b; done", "while after |"),
            ("echo x | until a; do b; done", "until after |"),
            ("until a; do b; done > f", "> after done"),
            ("while a; do b; done &", "& after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
````

Replace:

````rust
        for word in [
            "while", "until", "for", "in", "do", "done", "case", "esac", "select", "function",
            "time", "coproc", "{", "}", "[[", "]]", "break", "continue",
        ] {
````

with:

````rust
        for word in [
            "for", "in", "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[",
            "]]", "break", "continue",
        ] {
````

Replace:

````rust
        for (line, words) in [
            ("echo while do done", &["echo", "while", "do", "done"][..]),
            ("'for' x", &["for", "x"]),
````

with:

````rust
        for (line, words) in [
            ("echo for in", &["echo", "for", "in"][..]),
            ("'for' x", &["for", "x"]),
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let mut r = Reader::new();
        assert_eq!(
            r.add("while true"),
            Err(ParseError::Unsupported("while".into()))
        );
        assert!(r.reading());
        for line in ["do echo a", "if b", "then c", "fi", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
````

with:

````rust
        let mut r = Reader::new();
        assert_eq!(r.add("while true; do"), Ok(None));
        assert_eq!(
            r.add("coproc a"),
            Err(ParseError::Unsupported("coproc".into()))
        );
        assert!(r.reading());
        for line in ["echo a", "if b", "then c", "fi", "echo d"] {
            assert_eq!(r.add(line), Ok(None), "{line}");
````

Replace:

````rust
        assert_eq!(r.add("a &&"), Ok(None));
        assert!(r.add("for x in b; do").is_err());
        assert_eq!(r.add("echo $x"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
````

with:

````rust
        assert_eq!(r.add("a &&"), Ok(None));
        assert!(r.add("until $(b); do").is_err());
        assert_eq!(r.add("echo c"), Ok(None));
        assert_eq!(r.add("done"), Ok(None));
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        // end, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo t-args d\ndone\nt-args e\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
````

with:

````rust
        // end, counting the constructs inside.
        let text = b"if t-args a\nthen t-args b\nwhile t-args c\ndo coproc d\ndone\nt-args e\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
````

Replace:

````rust
            "/tmp/s.sh",
            b"if true\nthen echo a\nwhile true\ndo echo b\ndone\nfi\necho next\n",
        );
````

with:

````rust
            "/tmp/s.sh",
            b"if true\nthen echo a\nwhile true\ndo coproc b\ndone\nfi\necho next\n",
        );
````

Replace:

````rust
                0,
                "+ if true\n+ then echo a\n+ while true\nrelay-sh: unsupported syntax: while\n\
                 + do echo b\n+ done\n+ fi\n+ echo next\nnext\n"
                    .into()
````

with:

````rust
                0,
                "+ if true\n+ then echo a\n+ while true\n+ do coproc b\n\
                 relay-sh: unsupported syntax: coproc\n+ done\n+ fi\n+ echo next\nnext\n"
                    .into()
````

Replace:

````rust
            &mut h,
            &["while t-args a", "do t-args b", "done", "t-args next"],
        );
````

with:

````rust
            &mut h,
            &["while t-args a", "do coproc b", "done", "t-args next"],
        );
````

Replace:

````rust
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
    }

````

with:

````rust
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
    }

    #[test]
    fn while_and_until_run_their_body_while_the_condition_allows() {
        // Each loop here ends by itself: on a file it removes or makes.
        let mut h = Harness::new();
        h.put("/tmp/f", b"f\n");
        assert_eq!(
            h.run("while cat /tmp/f; do rm /tmp/f; done"),
            (0, "f\ncat: /tmp/f: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("until cat /tmp/g; do echo g > /tmp/g; done"),
            (0, "cat: /tmp/g: No such file or directory\ng\n".into())
        );
        // The status is the body's last (r8), 0 when it never runs (g3,
        // g7), whatever the condition's or `$?` before.
        h.put("/tmp/f", b"f\n");
        assert_eq!(
            h.run("while cat /tmp/f; do rm /tmp/f; false; done; echo $?"),
            (0, "f\ncat: /tmp/f: No such file or directory\n1\n".into())
        );
        for (line, ran) in [
            ("while false; do echo no; done", (0, "")),
            ("false; while false; do echo no; done; echo $?", (0, "0\n")),
            ("until true; do echo no; done", (0, "")),
            ("! while false; do echo no; done", (1, "")),
            (
                "true && until true; do echo no; done && echo yes",
                (0, "yes\n"),
            ),
            // A condition's status is `$?` in the body.
            ("until false; do echo $?; exit 7; done", (7, "1\n")),
            // `exit` deep inside.
            (
                "while true; do if true; then exit 4; fi; echo no; done",
                (4, ""),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
        // A bad substitution abandons the whole line, or `exit` ends it,
        // in the body or the condition: no body runs after it.
        assert_eq!(
            h.run("while true; do echo ${1A}; echo no; done; echo no"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(
            h.run("until echo ${1A}; do echo no; done"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert_eq!(h.run("until exit 5; do echo no; done"), (5, "".into()));
        // The in-process runner refuses a job in a loop too.
        for line in [
            "while false; do echo a & done",
            "until echo a & do echo b; done",
        ] {
            assert_eq!(
                h.run(line),
                (2, "relay-sh: unsupported syntax: &\n".into()),
                "{line}"
            );
        }
    }

    #[test]
    fn ctrl_c_ends_a_loop_of_built_ins_or_of_programs() {
        // A loop a person writes may run forever; Ctrl-C ends it (§5.2).
        let mut h = Harness::new();
        h.console.interrupt_after = Some(50);
        assert_eq!(
            h.run("while true; do cd; done; echo no"),
            (130, "^C\n".into())
        );
        h.console.interrupt_after = Some(50);
        assert_eq!(
            h.run("until false; do A=x; done; echo no"),
            (130, "^C\n".into())
        );
        let mut h = spawning();
        h.programs.known.push((
            "/bin/t-spin",
            WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C),
        ));
        // `t-args` exits with 3 here; a Ctrl-C in the condition runs no
        // body.
        assert_eq!(
            h.spawning("until t-args a; do t-spin; done; t-args no"),
            (130, "^C\n".into())
        );
        assert_eq!(
            h.spawning("until t-spin; do t-args no; done"),
            (130, "^C\n".into())
        );
        let ran: Vec<&str> = h.programs.spawned.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(ran, ["/bin/t-args", "/bin/t-spin", "/bin/t-spin"]);
    }

````

- [ ] **Step 4: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

pub struct TestConsole {
````

with:

````rust

/// The most a test's shell may ask for a Ctrl-C, which it does before each
/// command: past it a loop that never ends fails the test instead of
/// hanging it.
const ASKED_MAX: usize = 100_000;

pub struct TestConsole {
````

Replace:

````rust
    pub interrupt_after: Option<usize>,
}
````

with:

````rust
    pub interrupt_after: Option<usize>,
    /// How many times `interrupted` was asked.
    asked: usize,
}
````

Replace:

````rust
            interrupt_after: None,
        }
````

with:

````rust
            interrupt_after: None,
            asked: 0,
        }
````

Replace:

````rust
    fn interrupted(&mut self) -> bool {
        match &mut self.interrupt_after {
````

with:

````rust
    fn interrupted(&mut self) -> bool {
        self.asked += 1;
        assert!(
            self.asked <= ASKED_MAX,
            "asked for a Ctrl-C {ASKED_MAX} times: a loop that does not end"
        );
        match &mut self.interrupt_after {
````

- [ ] **Step 5: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# and a script whose command goes on across lines, with its transcript.
# Compound commands (plan 2): the nesting bound under /bin/sh.
timeout 30
````

with:

````text
# and a script whose command goes on across lines, with its transcript.
# Compound commands (plan 2): Ctrl-C in a loop of built-ins and of
# programs, and the nesting bound, under /bin/sh.
timeout 30
````

Replace:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
````

with:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
# A loop of built-ins ends at a Ctrl-C (§5.2): /bin/sh asks the kernel
# for one before each command. A loop of programs ends at one too.
send while cd; do cd; done
alive 1
type {ctrl-c}
expect \n\^C\nroot@relay:~# $
send echo $?
expect \n130\nroot@relay:~# $
send while true; do sleep 1; done
alive 2
type {ctrl-c}
expect \^C\nroot@relay:~# $
send echo $?
expect \n130\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
````

- [ ] **Step 6: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `While` found for enum `parser::Compound<W>` in the current scope ``; `` no variant, associated function, or constant named `Until` found for enum `parser::Compound<W>` in the current scope ``.

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 17 replacements, top to bottom:

Replace:

````rust
//! [`ParseError::Incomplete`], and a reader asks for more. `if … then …
//! [elif … then …] [else …] fi` is a compound command, its words keywords
//! only unquoted, whole and where a command name would stand (after `fi`,
//! only a keyword or an operator may follow); it cannot stand in a pipeline
//! of several, before `&` or with a redirection. bash's other reserved
//! words (`while`, `{`, …) are refused where a command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
````

with:

````rust
//! [`ParseError::Incomplete`], and a reader asks for more. `if … then …
//! [elif … then …] [else …] fi`, `while … do … done` and `until … do …
//! done` are compound commands, their words keywords only unquoted, whole
//! and where a command name would stand (after `fi` or `done`, only a
//! keyword or an operator may follow); one cannot stand in a pipeline of
//! several, before `&` or with a redirection. bash's other reserved words
//! (`for`, `{`, …) are refused where a command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
````

Replace:

````rust
    If(If<W>),
}
````

with:

````rust
    If(If<W>),
    While(Loop<W>),
    Until(Loop<W>),
}
````

Replace:

````rust
    pub otherwise: Option<List<W>>,
}
````

with:

````rust
    pub otherwise: Option<List<W>>,
}

/// `while` or `until`: the condition run before each pass, and the body
/// run while its status is 0 (`while`) or not (`until`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loop<W = String> {
    pub condition: List<W>,
    pub body: List<W>,
}
````

Replace:

````rust
                    || i.otherwise.as_ref().is_some_and(List::has_job)
            }
        }
    }
````

with:

````rust
                    || i.otherwise.as_ref().is_some_and(List::has_job)
            }
            Compound::While(l) | Compound::Until(l) => l.condition.has_job() || l.body.has_job(),
        }
    }
````

Replace:

````rust
            Compound::If(_) => "fi",
        }
````

with:

````rust
            Compound::If(_) => "fi",
            Compound::While(_) | Compound::Until(_) => "done",
        }
````

Replace:

````rust
const RESERVED: &[&str] = &[
    "while", "until", "for", "in", "do", "done", "case", "esac", "select", "function", "time",
    "coproc", "{", "}", "[[", "]]", "break", "continue",
];
````

with:

````rust
const RESERVED: &[&str] = &[
    "for", "in", "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]",
    "break", "continue",
];
````

Replace:

````rust
    Fi,
}
````

with:

````rust
    Fi,
    While,
    Until,
    Do,
    Done,
}
````

Replace:

````rust
    ("fi", Keyword::Fi),
];
````

with:

````rust
    ("fi", Keyword::Fi),
    ("while", Keyword::While),
    ("until", Keyword::Until),
    ("do", Keyword::Do),
    ("done", Keyword::Done),
];
````

Replace:

````rust
            .map_or("", |&(w, _)| w)
    }
}

````

with:

````rust
            .map_or("", |&(w, _)| w)
    }

    /// The compound command it opens, if it opens one.
    fn opens(self) -> Option<Kind> {
        match self {
            Keyword::If => Some(Kind::If),
            Keyword::While => Some(Kind::While),
            Keyword::Until => Some(Kind::Until),
            _ => None,
        }
    }
}

/// What compound command is being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    If,
    While,
    Until,
}

````

Replace:

````rust
    Else,
}
````

with:

````rust
    Else,
    /// A loop's body, after `do`.
    Body,
}
````

Replace:

````rust
    bangs: usize,
    stage: Stage,
````

with:

````rust
    bangs: usize,
    kind: Kind,
    stage: Stage,
````

Replace:

````rust
            return match w.keyword() {
                Some(k) if k != Keyword::If => Ok(Some(k)),
                _ => Err(ParseError::Unexpected(w.typed)),
````

with:

````rust
            return match w.keyword() {
                Some(k) if k.opens().is_none() => Ok(Some(k)),
                _ => Err(ParseError::Unexpected(w.typed)),
````

Replace:

````rust
    fn keyword(&mut self, k: Keyword, at: usize) -> Result<(), ParseError> {
        if k == Keyword::If {
            // bash runs it in a subshell.
````

with:

````rust
    fn keyword(&mut self, k: Keyword, at: usize) -> Result<(), ParseError> {
        if let Some(kind) = k.opens() {
            // bash runs it in a subshell.
````

Replace:

````rust
                bangs,
                stage: Stage::Condition,
````

with:

````rust
                bangs,
                kind,
                stage: Stage::Condition,
````

Replace:

````rust
        }
        let next = match (self.open.last().map(|o| o.stage), k) {
            (Some(Stage::Condition), Keyword::Then) => Stage::Then,
            (Some(Stage::Then), Keyword::Elif) => Stage::Condition,
            (Some(Stage::Then), Keyword::Else) => Stage::Else,
            (Some(Stage::Then | Stage::Else), Keyword::Fi) => Stage::Else,
            _ => return Err(ParseError::MissingTarget(k.token())),
````

with:

````rust
        }
        // The stage the keyword moves the innermost one on to, or none if
        // it closes it.
        let next = match self.open.last().map(|o| (o.kind, o.stage, k)) {
            Some((Kind::If, Stage::Condition, Keyword::Then)) => Some(Stage::Then),
            Some((Kind::If, Stage::Then, Keyword::Elif)) => Some(Stage::Condition),
            Some((Kind::If, Stage::Then, Keyword::Else)) => Some(Stage::Else),
            Some((Kind::If, Stage::Then | Stage::Else, Keyword::Fi)) => None,
            Some((Kind::While | Kind::Until, Stage::Condition, Keyword::Do)) => Some(Stage::Body),
            Some((Kind::While | Kind::Until, Stage::Body, Keyword::Done)) => None,
            _ => return Err(ParseError::MissingTarget(k.token())),
````

Replace:

````rust
        };
        if k != Keyword::Fi {
            open.lists.push(list);
````

with:

````rust
        };
        if let Some(next) = next {
            open.lists.push(list);
````

Replace:

````rust
        let mut lists = open.lists;
        let otherwise = match open.stage {
            Stage::Else => Some(list),
            _ => {
                lists.push(list);
                None
            }
        };
        let mut branches = Vec::new();
        let mut lists = lists.into_iter();
        while let (Some(condition), Some(body)) = (lists.next(), lists.next()) {
            branches.push((condition, body));
        }
        self.items = open.outer;
        self.parts.bangs = open.bangs;
        self.parts.compound = Some(Compound::If(If {
            branches,
            otherwise,
        }));
        Ok(())
````

with:

````rust
        let mut lists = open.lists;
        let compound = match open.kind {
            Kind::If => {
                let otherwise = match open.stage {
                    Stage::Else => Some(list),
                    _ => {
                        lists.push(list);
                        None
                    }
                };
                let mut branches = Vec::new();
                let mut lists = lists.into_iter();
                while let (Some(condition), Some(body)) = (lists.next(), lists.next()) {
                    branches.push((condition, body));
                }
                Compound::If(If {
                    branches,
                    otherwise,
                })
            }
            Kind::While | Kind::Until => {
                let Some(condition) = lists.pop() else {
                    return Err(ParseError::MissingTarget(k.token()));
                };
                let l = Loop {
                    condition,
                    body: list,
                };
                match open.kind {
                    Kind::Until => Compound::Until(l),
                    _ => Compound::While(l),
                }
            }
        };
        self.items = open.outer;
        self.parts.bangs = open.bangs;
        self.parts.compound = Some(compound);
        Ok(())
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            parser::Compound::If(i) => self.run_if(i),
        }
````

with:

````rust
            parser::Compound::If(i) => self.run_if(i),
            parser::Compound::While(l) => self.run_loop(l, false),
            parser::Compound::Until(l) => self.run_loop(l, true),
        }
    }

    /// Runs a `while` loop, or an `until` one: the condition, then the
    /// body while its status is 0 (or, `until`, not 0). The status is the
    /// body's last, 0 if it never ran. A loop may run for ever: Ctrl-C
    /// ends it, as the walker asks before each command (§5.2).
    fn run_loop(&mut self, l: &parser::Loop<parser::Word>, until: bool) -> i32 {
        let mut status = 0;
        loop {
            let condition = self.run_items(&l.condition);
            if self.ends_line() {
                return condition;
            }
            if (condition == 0) == until {
                return status;
            }
            status = self.run_items(&l.body);
            if self.ends_line() {
                return status;
            }
        }
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 352 tests.

- [ ] **Step 10: Run the `control` scenario**

Run: `cargo xtask test --e2e-only --scenario control`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
feat(shell,e2e): run while and until loops

`while`, `until`, `do` and `done` become keywords where a command name
would stand; a loop runs its condition, then its body while the
condition's status is 0 (`until`: not 0). Its status is the body's
last, 0 when it never runs; `exit`, Ctrl-C and a bad substitution end
it in its condition or its body. A keyword out of place is bash's
syntax error naming it, and a loop is refused in a pipeline, before
`&` or redirected (programmable shell gate §4.1, §5.1, §15 item 2).

A test shell's console now fails a test that asks for a Ctrl-C more
than 100,000 times, so a loop that never ends fails instead of hanging.
The scenario `control` ends a loop of built-ins and one of programs
with Ctrl-C under /bin/sh, `$?` 130.
EOF
````


### Task 13: A Ctrl-C for an ended group waits for the console's next holder

The prototype's review (critical C1) and decision 10: a Ctrl-C typed after a foreground program had ended, earlier on the same command line, was lost: the program's group kept the console, in line mode, until the shell's next prompt, and the kernel spent the Ctrl-C killing that group, which had no process left (`true; while cd; do cd; done` ended only with a reset; plan 1's `sleep 20 & true; wait` likewise). A line-mode Ctrl-C is now taken only for a foreground group that is alive (`InputQueue::take_line_interrupt_for`, `tty::ctrl_c` asking `Table::has_group`); otherwise it waits, and the switch to raw mode of the shell that takes the console back (Task 14) hands it over as raw input, as it already did for a Ctrl-C nobody took. The red run is the kernel's tests, which cannot find `take_line_interrupt_for`. Mutation checks: the queue's rule dropped fails the test; the call's `has_group` replaced by `true` is not caught by any test, as the window it closes, between a program's end and the shell taking the console back, lasts microseconds.

**Files:**
- Modify: `kernel/src/input.rs`
- Modify: `kernel/src/proc.rs`
- Modify: `kernel/src/tty.rs`

**Interfaces:**
- Consumes: the kernel's `input::InputQueue`, `tty::ctrl_c`, `proc::console_input`, `Table::has_group`.
- Produces: `InputQueue::take_line_interrupt_for(&mut self, alive: bool) -> bool`; `tty::ctrl_c(alive: impl FnOnce(u32) -> bool) -> Option<u32>`.

- [ ] **Step 1: Add the failing tests to `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
    #[test]
    fn a_raw_ctrl_c_can_be_taken_out_of_the_input() {
````

with:

````rust
    #[test]
    fn a_line_ctrl_c_for_a_group_that_ended_waits_for_the_next_holder() {
        // The prototype's review: a Ctrl-C typed after a program ended,
        // its group still holding the console in line mode, killed no one
        // and was lost. It waits, and the shell that takes the console back
        // in raw mode reads it.
        let mut q = InputQueue::new();
        q.set_line_mode(true);
        q.push(&[INTERRUPT]);
        assert!(!q.take_line_interrupt_for(false), "no process to kill");
        q.set_line_mode(false);
        assert!(q.take_raw_interrupt());
        // A group with a process gets it, as before.
        let mut q = InputQueue::new();
        q.set_line_mode(true);
        q.push(&[INTERRUPT]);
        assert!(q.take_line_interrupt_for(true));
        q.set_line_mode(false);
        assert!(!q.has_raw_interrupt(), "taken");
    }

    #[test]
    fn a_raw_ctrl_c_can_be_taken_out_of_the_input() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p relay-kernel --lib`

Expected: FAIL: compile errors such as `` no method named `take_line_interrupt_for` found for struct `input::InputQueue` in the current scope ``.

- [ ] **Step 3: Change `kernel/src/input.rs`**

In `kernel/src/input.rs`, replace:

````rust
        core::mem::take(&mut self.interrupted)
    }
````

with:

````rust
        core::mem::take(&mut self.interrupted)
    }

    /// As [`InputQueue::take_line_interrupt`], for a foreground group that
    /// is `alive`: one with no process left to kill leaves the Ctrl-C for
    /// whoever takes the console back, to whom the switch to raw mode
    /// hands it (programmable shell gate §15 item 2).
    pub fn take_line_interrupt_for(&mut self, alive: bool) -> bool {
        alive && self.take_line_interrupt()
    }
````

- [ ] **Step 4: Change `kernel/src/proc.rs`**

In `kernel/src/proc.rs`, replace:

````rust
/// kills the foreground group (spec §6.4), and anything typed wakes whoever
/// waits for input.
fn console_input(t: &mut Table<Res>) {
    if let Some(pgid) = tty::ctrl_c() {
        // Refused only for process 1's group, which never has the console
````

with:

````rust
/// kills the foreground group (spec §6.4), and anything typed wakes whoever
/// waits for input. A group that has ended gets no Ctrl-C: the shell that
/// takes the console back reads it as raw input.
fn console_input(t: &mut Table<Res>) {
    if let Some(pgid) = tty::ctrl_c(|g| t.has_group(g)) {
        // Refused only for process 1's group, which never has the console
````

- [ ] **Step 5: Change `kernel/src/tty.rs`**

In `kernel/src/tty.rs`, replace:

````rust
/// The foreground group a Ctrl-C typed in line mode is for, if one was
/// typed: what was typed before it is dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    INPUT
        .lock()
        .take_line_interrupt()
        .then(|| FOREGROUND.load(Ordering::Relaxed))
}
````

with:

````rust
/// The foreground group a Ctrl-C typed in line mode is for, if one was
/// typed: what was typed before it is dropped (spec §6.4). Only while the
/// group is `alive`: once it has ended, the Ctrl-C waits for the group
/// that takes the console back.
pub fn ctrl_c(alive: impl FnOnce(u32) -> bool) -> Option<u32> {
    let pgid = FOREGROUND.load(Ordering::Relaxed);
    let alive = alive(pgid);
    INPUT.lock().take_line_interrupt_for(alive).then_some(pgid)
}
````

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p relay-kernel --lib`

Expected: PASS: 393 tests.

- [ ] **Step 7: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 8: Commit**

````bash
git add kernel
git commit -F - <<'EOF'
fix(kernel): keep a Ctrl-C for the console's next holder

The prototype's review lost a Ctrl-C typed after a foreground program
ended: its group kept the console in line mode until the shell's next
prompt, and the Ctrl-C was spent killing that group, which had no
process left. A line-mode Ctrl-C is now taken only for a foreground
group that is alive; otherwise it waits, and the switch to raw mode of
the shell that takes the console back hands it over as raw input
(programmable shell gate §15 item 2).
EOF
````


### Task 14: The shell takes the console back after each program

The prototype's review (critical C1) and decision 10: the interactive shell took the console back only before reading its next line, so for the rest of a line after a program had run its own commands could not hear a Ctrl-C. `Console::take_back` (default nothing) does what `SysConsole::read_byte` did before each read, the console's foreground made its own group and its mode raw, and the walker calls it once each program or pipeline it waited for has ended; built-ins never give the console away. With Task 13 no Ctrl-C typed meanwhile is lost. The test console counts the calls. The red run is the shell's tests, which cannot find `take_back`. Mutation checks: either call dropped fails the test, and the program's dropped fails `control` (Task 15) too.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 13's kept Ctrl-C; Task 11's check.
- Produces: `shell::Console::take_back(&mut self)` (default nothing); `SysConsole::take_back`; `TestConsole::taken_back: usize`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn after_ctrl_c_ends_a_command_line_its_status_is_130() {
````

with:

````rust
    #[test]
    fn the_shell_takes_the_console_back_after_each_program() {
        // The prototype's review: once a program had ended, its group kept
        // the console until the next prompt, so a Ctrl-C was lost for the
        // rest of the line. Built-ins never give it away.
        let mut h = spawning();
        h.spawning("t-args a; cd; t-args b | t-args c; cd");
        assert_eq!(h.console.taken_back, 2);
        h.console.taken_back = 0;
        h.spawning("cd; cd");
        assert_eq!(h.console.taken_back, 0);
    }

    #[test]
    fn after_ctrl_c_ends_a_command_line_its_status_is_130() {
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    asked: usize,
}
````

with:

````rust
    asked: usize,
    /// How many times the shell took the console back.
    pub taken_back: usize,
}
````

Replace:

````rust
            asked: 0,
        }
````

with:

````rust
            asked: 0,
            taken_back: 0,
        }
````

Replace:

````rust
    }
    /// As the real console's, a Ctrl-C is taken by the call that sees it.
````

with:

````rust
    }
    fn take_back(&mut self) {
        self.taken_back += 1;
    }

    /// As the real console's, a Ctrl-C is taken by the call that sees it.
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `take_back` is not a member of trait `Console` ``.

- [ ] **Step 4: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    fn read_byte(&mut self) -> Option<u8> {
        if self.interactive {
            let _ = sys::console_mode(MODE_RAW);
        }
        if let Some(pgid) = self.group {
            let _ = sys::console_foreground(pgid);
        }
        let mut byte = [0];
````

with:

````rust
    fn read_byte(&mut self) -> Option<u8> {
        self.take_back();
        let mut byte = [0];
````

Replace:

````rust
        self.interactive && sys::take_ctrl_c()
    }
````

with:

````rust
        self.interactive && sys::take_ctrl_c()
    }

    fn take_back(&mut self) {
        if self.interactive {
            let _ = sys::console_mode(MODE_RAW);
        }
        if let Some(pgid) = self.group {
            let _ = sys::console_foreground(pgid);
        }
    }
````

- [ ] **Step 5: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
        false
    }
}

````

with:

````rust
        false
    }
    /// The interactive shell takes the console back (its own group, raw
    /// mode) as it does before it reads, once a command it gave the
    /// console to has ended, so that a Ctrl-C typed before its next prompt
    /// is its own (programmable shell gate §15 item 2). The default does
    /// nothing.
    fn take_back(&mut self) {}
}

````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
                }
                None => self
                    .runner
                    .get()
                    .run(parts, name, args, cmd.redirect.as_ref()),
            },
````

with:

````rust
                }
                None => {
                    let ran = self
                        .runner
                        .get()
                        .run(parts, name, args, cmd.redirect.as_ref());
                    self.console.take_back();
                    ran
                }
            },
````

Replace:

````rust
        let ran = self.runner.get().pipeline(parts, stages);
        self.cancelled |= ran.cancelled;
````

with:

````rust
        let ran = self.runner.get().pipeline(parts, stages);
        self.console.take_back();
        self.cancelled |= ran.cancelled;
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 353 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 28 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(relay-rt,shell): take the console back after each program

The prototype's review found a loop of built-ins that no Ctrl-C ended
once a program had run earlier on its line (`true; while cd; do cd;
done`), and plan 1's `wait` after one alike: the program's group kept
the console, in line mode, until the next prompt. The interactive
/bin/sh now takes it back (`Console::take_back`: its own group, raw
mode, as before it reads) once each program or pipeline it waited for
has ended, and with the kernel's kept Ctrl-C none typed meanwhile is
lost (programmable shell gate §15 item 2).
EOF
````


### Task 15: The scenario `control` presses Ctrl-C after a program

The prototype's review (critical C1): the scenario `control` presses Ctrl-C in `true; while cd; do cd; done` and in `sleep 30 & true; wait` under `/bin/sh`, where it was lost; each ends with `^C`, the loop's `$?` 130, and the job is killed and waited for. It tests what Tasks 13 and 14 do, so there is no failing run: with the shell's call to `take_back` after a program dropped, the scenario times out at the loop.

**Files:**
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: Tasks 13, 14.
- Produces: `tests/e2e/control.txt`'s lines for Ctrl-C after a program.

- [ ] **Step 1: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, replace:

````text
expect \n130\nroot@relay:~# $
# Compound commands nest 32 levels deep, the most (§4.5): a script runs
````

with:

````text
expect \n130\nroot@relay:~# $
# The same after a program ran on the line (the prototype's review): the
# shell takes the console back once a program has ended, and a Ctrl-C
# typed meanwhile is kept for it; plan 1's `wait` too.
send true; while cd; do cd; done
alive 1
type {ctrl-c}
expect \n\^C\nroot@relay:~# $
send echo $?
expect \n130\nroot@relay:~# $
send sleep 30 & true; wait
expect \n\[1\] \d+\n
alive 1
type {ctrl-c}
expect \^C\nroot@relay:~# $
send kill %1
send wait %1
expect \n\[1\]\+  Killed                  sleep 30\nroot@relay:~# $
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
test(e2e): end a loop and a wait after a program with Ctrl-C

The scenario `control` presses Ctrl-C in `true; while cd; do cd;
done` and in `sleep 30 & true; wait` under /bin/sh, where the
prototype's review found it lost: each ends with `^C`, the loop's `$?`
130 (programmable shell gate §5.2, §15 item 2).
EOF
````


### Task 16: `for` runs

Spec §4.1, §5.1 and decisions 3, 2, 8: `for` and `in` become keywords, `Compound::For(For)` (`For { name, words, body }`, words `None` for `"$@"`). Its header has a grammar of its own (the stages `ForName`, `ForAfterName`, `ForWords`, `ForBeforeDo`): the name, any word, on `for`'s line (`for` alone is the error near `newline`); then `in` or `do`, on a later line too; the words, keywords among them, to a `;` or a newline; then `do`. Any other operator in the header is bash's error naming it (probes in `progress.md`), so each operator first ends the word before it (`>` after digits apart, which the `>` arm reads as an fd). `in` where a command name stands is bash's error too. `run_for` checks the name when it runs (`` `1x': not a valid identifier ``, status 1, the line going on, as bash), expands the words once as arguments (`expand::words`) or takes `"$@"` (`Vars::positional`), sets the variable before each pass, which keeps its last value, and fails the loop, status 1, on a value the 64 KiB of variables cannot hold; a bad substitution in its words abandons the line. The red run is the shell's tests, which cannot find `Compound::For`. Mutation checks: the early end of a word (never, or always), `in` or `do` taken where they may not stand, a word not kept, the newline and `;` rules, `||`, `&&` and `;;` not named, the empty list not made, the first stage, the name check, the abandon flag, `"$@"` empty or with `$0`, the variable not set, and the stop after a pass, each fail a test; `do` among the words is an equivalent mutant (the words' arm comes first).

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 12's construct stack.
- Produces: `parser::Compound::For(For<W>)`, `parser::For<W> { name: W, words: Option<Vec<W>>, body: List<W> }`, `Keyword::{For, In}`, `Kind::For`, `Stage::{ForName, ForAfterName, ForWords, ForBeforeDo}`, `Open::{name, words}`, `Parser::{for_header, for_word, for_operator}` (private); `expand::words(&[Word], &Vars, i32) -> Result<Vec<String>, Error>`, `Vars::positional(&self) -> &[String]` (crate-private); `Shell::run_for` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
                        Compound::Until(_) => String::from("until"),
                    },
````

with:

````rust
                        Compound::Until(_) => String::from("until"),
                        Compound::For(_) => String::from("for"),
                    },
````

Replace:

````rust
                Compound::Until(l) => (true, names_of(&l.condition), names_of(&l.body)),
                Compound::If(_) => panic!("{line}: an if"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
````

with:

````rust
                Compound::Until(l) => (true, names_of(&l.condition), names_of(&l.body)),
                _ => panic!("{line}: no while or until"),
            },
            Run::Commands(_) => panic!("{line}: no compound command"),
        }
    }

    /// The `for` of `line`'s one item: its name and words as typed (none
    /// for `"$@"`), and its body's command names.
    fn for_of(line: &str) -> (String, Option<Vec<String>>, Vec<String>) {
        let p = parse_line(line).unwrap().items.remove(0).and_or.first;
        let Run::Compound(c) = p.run else {
            panic!("{line}: no compound command");
        };
        let Compound::For(f) = *c else {
            panic!("{line}: no for");
        };
        let words = f.words.map(|w| w.into_iter().map(|w| w.typed).collect());
        (f.name.typed, words, names_of(&f.body))
    }

    #[test]
    fn a_for_holds_its_name_its_words_and_its_body() {
        let s = |v: &[&str]| -> Vec<String> { v.iter().map(|x| String::from(*x)).collect() };
        let c = s(&["c"]);
        for (line, name, words) in [
            ("for x in a b; do c; done", "x", Some(s(&["a", "b"]))),
            // f11, l3: none; f10, r9, l10: over "$@".
            ("for x in; do c; done", "x", Some(s(&[]))),
            ("for x in\ndo c; done", "x", Some(s(&[]))),
            ("for x; do c; done", "x", None),
            ("for x do c; done", "x", None),
            ("for x\ndo c\ndone", "x", None),
            ("for x;\ndo c; done", "x", None),
            // p4, l4, l9: across lines.
            ("for x\nin a b\ndo c\ndone", "x", Some(s(&["a", "b"]))),
            ("for x in a\n\n# d\ndo c; done", "x", Some(s(&["a"]))),
            ("for x in a b;\ndo c; done", "x", Some(s(&["a", "b"]))),
            ("for x in a # d\ndo c; done", "x", Some(s(&["a"]))),
            // r6, f11: keywords among the words; f15: any word a name.
            (
                "for x in if then do; do c; done",
                "x",
                Some(s(&["if", "then", "do"])),
            ),
            ("for in in in; do c; done", "in", Some(s(&["in"]))),
            ("for done in a; do c; done", "done", Some(s(&["a"]))),
            (
                "for \"x\" in \"$@\" ${a}b; do c; done",
                "\"x\"",
                Some(s(&["\"$@\"", "${a}b"])),
            ),
        ] {
            assert_eq!(
                for_of(line),
                (String::from(name), words, c.clone()),
                "{line}"
            );
        }
        assert_eq!(
            for_of("for x in a; do for y in b; do c; done done").2,
            ["for"]
        );
        for text in [
            "for x",
            "for x in a b",
            "for x in a b;",
            "for x; do",
            "for x in a; do b",
        ] {
            assert_eq!(parse_line(text), Err(ParseError::Incomplete), "{text}");
        }
    }

    #[test]
    fn a_for_s_header_takes_no_operator() {
        for (line, token) in [
            ("for\n", "newline"),
            ("for\nx in a; do echo; done", "newline"),
            ("for x a; do echo; done", "a"),
            ("for in a; do echo; done", "a"),
            ("for x in a b do echo $x; done", "done"),
            ("for x in a\nb", "b"),
            ("for x in a # c\necho", "echo"),
            ("for x in a | b; do echo; done", "|"),
            ("for x in a || b; do echo; done", "||"),
            ("for x in a > f; do echo; done", ">"),
            ("for x > f; do echo; done", ">"),
            ("for x in a && b; do echo; done", "&&"),
            ("for x in a & do echo; done", "&"),
            ("for x |", "|"),
            ("for ; do echo; done", ";"),
            ("for x; ; do echo; done", ";"),
            ("for x; in a; do echo; done", "in"),
            ("for x;; do echo; done", ";;"),
            ("for x in a;; do echo; done", ";;"),
            ("for x in a; do; echo; done", ";"),
            ("for x in a; then echo; done", "then"),
            ("for x in a; do echo; fi", "fi"),
            // g16, g10, k10: `in` where a command name stands.
            ("in", "in"),
            ("if true; then echo in; in; fi", "in"),
            ("if a; then b; fi in", "in"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        for (line, what) in [
            ("for x in a; do b; done | cat", "| after done"),
            ("echo x | for x in a; do b; done", "for after |"),
            ("for x in a; do b; done &", "& after done"),
            ("for x in a; do b; done > f", "> after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
````

Replace:

````rust
        for word in [
            "for", "in", "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[",
            "]]", "break", "continue",
        ] {
````

with:

````rust
        for word in [
            "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break",
            "continue",
        ] {
````

Replace:

````rust
        for (line, words) in [
            ("echo for in", &["echo", "for", "in"][..]),
            ("'for' x", &["for", "x"]),
````

with:

````rust
        for (line, words) in [
            ("echo case esac", &["echo", "case", "esac"][..]),
            ("'for' x", &["for", "x"]),
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn an_if_32_levels_deep_runs() {
````

with:

````rust
    #[test]
    fn a_for_runs_its_body_once_for_each_word() {
        let mut h = Harness::new();
        for (line, ran) in [
            ("for x in a b c; do echo $x; done", (0, "a\nb\nc\n")),
            // f9, r4, r5: a shell variable, its last value kept, none set
            // when there is no pass.
            ("for x in a b; do echo $x; done; echo $x", (0, "a\nb\nb\n")),
            ("for x in a; do x=z; done; echo $x", (0, "z\n")),
            ("x=y; for x in; do echo no; done; echo $x", (0, "y\n")),
            // r3: words expand as arguments do.
            (
                "for x in $E a '' \"$E\"; do echo [$x]; done",
                (0, "[a]\n[]\n[]\n"),
            ),
            // r8, r11: the body's last status, 0 with no pass.
            ("for x in a b; do false; done", (1, "")),
            ("false; for x in; do true; done; echo $?", (0, "0\n")),
            (
                "for x in a b; do echo $x; exit 6; done; echo no",
                (6, "a\n"),
            ),
            // g1, r1, r2: a name checked when it runs, as typed.
            (
                "echo a; for 1x in a; do echo no; done; echo b",
                (0, "a\nrelay-sh: `1x': not a valid identifier\nb\n"),
            ),
            (
                "for \"x\" in a; do echo no; done",
                (1, "relay-sh: `\"x\"': not a valid identifier\n"),
            ),
            (
                "for $E in a; do echo no; done",
                (1, "relay-sh: `$E': not a valid identifier\n"),
            ),
            // A bad substitution in its words abandons the line.
            (
                "for x in ${1A}; do echo no; done; echo no",
                (1, "relay-sh: ${1A}: bad substitution\n"),
            ),
            (
                "for x in a; do echo a & done",
                (2, "relay-sh: unsupported syntax: &\n"),
            ),
        ] {
            assert_eq!(h.run(line), (ran.0, ran.1.into()), "{line}");
        }
        // A value the variables cannot hold fails the loop; the line goes on.
        let line = alloc::format!(
            "A={}; for x in $A; do echo no; done; echo next",
            "a".repeat(40_000)
        );
        assert_eq!(
            h.run(&line),
            (
                0,
                "relay-sh: x: the variables would hold more than 64 KiB\nnext\n".into()
            )
        );
        // Ctrl-C ends it, before the first `echo` here.
        h.console.interrupt_after = Some(1);
        assert_eq!(
            h.run("for x in a b c; do echo $x; done"),
            (130, "^C\n".into())
        );
    }

    #[test]
    fn a_for_without_words_loops_over_the_script_s_arguments() {
        // f10: `for NAME; do`, `for NAME do` and `in "$@"` alike.
        let mut h = Harness::new();
        h.put(
            "/tmp/s.sh",
            b"for x; do echo [$x]; done\nfor y do echo [$y]; done\nfor z in \"$@\"; do echo [$z]; done\n",
        );
        let (status, out) = h.run("sh /tmp/s.sh p 'q r' ''");
        let printed: Vec<&str> = out.lines().filter(|l| !l.starts_with('+')).collect();
        assert_eq!(status, 0);
        assert_eq!(printed, ["[p]", "[q r]", "[]"].repeat(3));
        // With none, no pass.
        assert_eq!(h.run("for x; do echo no; done"), (0, "".into()));
    }

    #[test]
    fn an_if_32_levels_deep_runs() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `For` found for enum `parser::Compound<W>` in the current scope ``.

- [ ] **Step 4: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        vars.args.extend_from_slice(args);
        vars
    }

````

with:

````rust
        vars.args.extend_from_slice(args);
        vars
    }

    /// The arguments after `$0`, which `"$@"` gives.
    pub fn positional(&self) -> &[String] {
        self.args.get(1..).unwrap_or(&[])
    }

````

Replace:

````rust
    commands.iter().map(|c| x.command(c)).collect()
}
````

with:

````rust
    commands.iter().map(|c| x.command(c)).collect()
}

/// `words` expanded as a command's arguments are (a `for`'s list).
pub(crate) fn words(words: &[Word], vars: &Vars, status: i32) -> Result<Vec<String>, Error> {
    let mut x = Expander::new(vars, status);
    let mut out = Vec::new();
    for w in words {
        out.extend(x.word(w)?);
    }
    Ok(out)
}
````

- [ ] **Step 5: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 17 replacements, top to bottom:

Replace:

````rust
//! [`ParseError::Incomplete`], and a reader asks for more. `if … then …
//! [elif … then …] [else …] fi`, `while … do … done` and `until … do …
//! done` are compound commands, their words keywords only unquoted, whole
//! and where a command name would stand (after `fi` or `done`, only a
//! keyword or an operator may follow); one cannot stand in a pipeline of
//! several, before `&` or with a redirection. bash's other reserved words
//! (`for`, `{`, …) are refused where a command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
````

with:

````rust
//! [`ParseError::Incomplete`], and a reader asks for more. `if … then …
//! [elif … then …] [else …] fi`, `while … do … done`, `until … do … done`
//! and `for NAME [in WORD…] do … done` are compound commands, their words
//! keywords only unquoted, whole and where a command name would stand (and
//! `in` and `do` where a `for` takes them; after `fi` or `done`, only a
//! keyword or an operator may follow); one cannot stand in a pipeline of
//! several, before `&` or with a redirection. A `for`'s header takes no
//! operator but the `;` or newline that ends its words. bash's other
//! reserved words (`case`, `{`, …) are refused where a command name would
//! stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
````

Replace:

````rust
    Until(Loop<W>),
}
````

with:

````rust
    Until(Loop<W>),
    For(For<W>),
}
````

Replace:

````rust
    pub condition: List<W>,
    pub body: List<W>,
````

with:

````rust
    pub condition: List<W>,
    pub body: List<W>,
}

/// `for`: its variable's name as typed, the words it takes in turn (none
/// for `"$@"`), and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct For<W = String> {
    pub name: W,
    pub words: Option<Vec<W>>,
    pub body: List<W>,
````

Replace:

````rust
            Compound::While(l) | Compound::Until(l) => l.condition.has_job() || l.body.has_job(),
        }
````

with:

````rust
            Compound::While(l) | Compound::Until(l) => l.condition.has_job() || l.body.has_job(),
            Compound::For(f) => f.body.has_job(),
        }
````

Replace:

````rust
            Compound::If(_) => "fi",
            Compound::While(_) | Compound::Until(_) => "done",
        }
````

with:

````rust
            Compound::If(_) => "fi",
            Compound::While(_) | Compound::Until(_) | Compound::For(_) => "done",
        }
````

Replace:

````rust
const RESERVED: &[&str] = &[
    "for", "in", "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]",
    "break", "continue",
];
````

with:

````rust
const RESERVED: &[&str] = &[
    "case", "esac", "select", "function", "time", "coproc", "{", "}", "[[", "]]", "break",
    "continue",
];
````

Replace:

````rust
    Done,
}
````

with:

````rust
    Done,
    For,
    In,
}
````

Replace:

````rust
    ("done", Keyword::Done),
];
````

with:

````rust
    ("done", Keyword::Done),
    ("for", Keyword::For),
    ("in", Keyword::In),
];
````

Replace:

````rust
            Keyword::Until => Some(Kind::Until),
            _ => None,
````

with:

````rust
            Keyword::Until => Some(Kind::Until),
            Keyword::For => Some(Kind::For),
            _ => None,
````

Replace:

````rust
    Until,
}
````

with:

````rust
    Until,
    For,
}
````

Replace:

````rust
    Body,
}
````

with:

````rust
    Body,
    /// A `for`'s header: the next word is its name; then `in` or `do` may
    /// come, on a later line too; then its words, to a `;` or a newline;
    /// then, past blank lines, its `do`.
    ForName,
    ForAfterName,
    ForWords,
    ForBeforeDo,
}
````

Replace:

````rust
    lists: Vec<List<Word>>,
}
````

with:

````rust
    lists: Vec<List<Word>>,
    /// A `for`'s name and words.
    name: Option<Word>,
    words: Option<Vec<Word>>,
}
````

Replace:

````rust
            };
            match c {
````

with:

````rust
            };
            // The word before an operator ends at it (the `>` arm first
            // looks for an fd, as in `2>`); in a `for`'s header, which that
            // word may open or end, the operator is the header's.
            if matches!(c, '>' | '|' | ';' | '&' | '\n') {
                if c != '>' || !(self.word.started && self.word.word.digits().is_some()) {
                    self.end_word(line, at)?;
                }
                if let Some(stage) = self.for_header() {
                    self.for_operator(stage, c, &mut cur)?;
                    self.item_start = cur.pos();
                    continue;
                }
            }
            match c {
````

Replace:

````rust

    /// Ends the word being read, which ends at `at`; a keyword where one
    /// may stand opens, goes on with or closes a compound command.
    fn end_word(&mut self, line: &str, at: usize) -> Result<(), ParseError> {
        match self.parts.end_word(&mut self.word, line)? {
````

with:

````rust

    /// The stage of the `for` whose header is being read.
    fn for_header(&self) -> Option<Stage> {
        self.open.last().map(|o| o.stage).filter(|s| {
            matches!(
                s,
                Stage::ForName | Stage::ForAfterName | Stage::ForWords | Stage::ForBeforeDo
            )
        })
    }

    /// A word of a `for`'s header (stage `stage`): its name, `in`, a word
    /// of its list or `do`; anything else is bash's error naming it.
    fn for_word(&mut self, stage: Stage, w: Word) -> Result<(), ParseError> {
        let Some(open) = self.open.last_mut() else {
            return Err(ParseError::Unexpected(w.typed));
        };
        match (stage, w.keyword()) {
            (Stage::ForName, _) => {
                open.name = Some(w);
                open.stage = Stage::ForAfterName;
            }
            (Stage::ForAfterName, Some(Keyword::In)) => {
                open.words = Some(Vec::new());
                open.stage = Stage::ForWords;
            }
            (Stage::ForWords, _) => open.words.get_or_insert_with(Vec::new).push(w),
            (Stage::ForAfterName | Stage::ForBeforeDo, Some(Keyword::Do)) => {
                open.stage = Stage::Body;
            }
            _ => return Err(ParseError::Unexpected(w.typed)),
        }
        Ok(())
    }

    /// An operator, `c` and what follows it in `cur`, in a `for`'s header
    /// (stage `stage`): a `;` or a newline ends its words, a newline right
    /// after `for` is bash's error, and every other one is.
    fn for_operator(
        &mut self,
        stage: Stage,
        c: char,
        cur: &mut Cursor<'_>,
    ) -> Result<(), ParseError> {
        let token = match c {
            '\n' => "newline",
            ';' if cur.next_if_eq(';') => {
                if cur.next_if_eq('&') {
                    ";;&"
                } else {
                    ";;"
                }
            }
            ';' if cur.next_if_eq('&') => ";&",
            ';' => ";",
            '|' if cur.next_if_eq('|') => "||",
            '&' if cur.next_if_eq('&') => "&&",
            '>' if cur.next_if_eq('>') => ">>",
            '|' => "|",
            '&' => "&",
            _ => ">",
        };
        let next = match (stage, token) {
            (Stage::ForAfterName | Stage::ForBeforeDo, "newline") => stage,
            (Stage::ForWords, "newline") => Stage::ForBeforeDo,
            (Stage::ForAfterName | Stage::ForWords, ";") => Stage::ForBeforeDo,
            _ => return Err(ParseError::MissingTarget(token)),
        };
        if let Some(open) = self.open.last_mut() {
            open.stage = next;
        }
        Ok(())
    }

    /// Ends the word being read, which ends at `at`; a keyword where one
    /// may stand opens, goes on with or closes a compound command.
    fn end_word(&mut self, line: &str, at: usize) -> Result<(), ParseError> {
        if let Some(stage) = self.for_header() {
            return match core::mem::take(&mut self.word).finish(line) {
                Some(w) => self.for_word(stage, w),
                None => Ok(()),
            };
        }
        match self.parts.end_word(&mut self.word, line)? {
````

Replace:

````rust
                kind,
                stage: Stage::Condition,
                lists: Vec::new(),
            });
````

with:

````rust
                kind,
                stage: match kind {
                    Kind::For => Stage::ForName,
                    _ => Stage::Condition,
                },
                lists: Vec::new(),
                name: None,
                words: None,
            });
````

Replace:

````rust
            Some((Kind::While | Kind::Until, Stage::Condition, Keyword::Do)) => Some(Stage::Body),
            Some((Kind::While | Kind::Until, Stage::Body, Keyword::Done)) => None,
            _ => return Err(ParseError::MissingTarget(k.token())),
````

with:

````rust
            Some((Kind::While | Kind::Until, Stage::Condition, Keyword::Do)) => Some(Stage::Body),
            Some((Kind::While | Kind::Until | Kind::For, Stage::Body, Keyword::Done)) => None,
            _ => return Err(ParseError::MissingTarget(k.token())),
````

Replace:

````rust
                    _ => Compound::While(l),
                }
            }
        };
````

with:

````rust
                    _ => Compound::While(l),
                }
            }
            Kind::For => {
                let Some(name) = open.name else {
                    return Err(ParseError::MissingTarget(k.token()));
                };
                Compound::For(For {
                    name,
                    words: open.words,
                    body: list,
                })
            }
        };
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            parser::Compound::Until(l) => self.run_loop(l, true),
        }
    }
````

with:

````rust
            parser::Compound::Until(l) => self.run_loop(l, true),
            parser::Compound::For(f) => self.run_for(f),
        }
    }

    /// Runs a `for`: its body once for each of its words, expanded as
    /// arguments are (or `"$@"`'s), its variable set to the word first
    /// (programmable shell gate §5.1). Its status is the body's last, 0
    /// if it never ran. A name that is not one is bash's error, as it is
    /// typed; a value the variables cannot hold fails the loop.
    fn run_for(&mut self, f: &parser::For<parser::Word>) -> i32 {
        let name = f.name.typed.as_str();
        if !parser::is_name(name) {
            return self.finish(1, format!("{NAME}: `{name}': not a valid identifier\n"));
        }
        let words = match &f.words {
            Some(words) => match expand::words(words, &self.vars, self.status) {
                Ok(words) => words,
                Err(e) => return self.not_expanded(e, true),
            },
            None => self.vars.positional().to_vec(),
        };
        let mut status = 0;
        for word in words {
            if let Err(e) = self.vars.set(name, word) {
                return self.not_expanded(e, false);
            }
            status = self.run_items(&f.body);
            if self.ends_line() {
                return status;
            }
        }
        status
    }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 357 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): run for loops

`for NAME [in WORD...]; do ... done` and `for NAME do` run the body
once for each word, expanded as arguments are, or for each of `"$@"`,
setting the shell variable first; it keeps its last value. The name is
checked when the loop runs, bash's `not a valid identifier` naming it
as typed; a value the variables cannot hold fails the loop. A `for`'s
header takes no operator but the `;` or newline that ends its words,
each other one bash's syntax error naming it, and `in` where a command
name would stand is one too (programmable shell gate §4.1, §5.1, §15
item 2).
EOF
````


### Task 17: A job in a `for`'s body has a text of its own

The prototype's review (important I2): a background job in a `for` loop's body had its header in its text (`for x in a; do sleep 5 & done` showed `do sleep 5` in `jobs`), and the `!` of `for é do ! a & done` was taken off a byte at a time, which panicked inside the `é`: input reached the parser that could panic `/bin/sh`. A job's text now starts after the header's `do`, as after the other keywords (`for_word` sets it), and a `!` comes off as a character (`strip_prefix`). The red run is the shell's tests: the texts are wrong. Mutation checks: the start not set fails the test; `strip_prefix` replaced by `get(1..)` is an equivalent mutant now that the text starts with its `!`.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 16's `for_word`.
- Produces: `Parser::for_word(&mut self, Stage, Word, at: usize)` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
    #[test]
    fn a_for_s_header_takes_no_operator() {
````

with:

````rust
    #[test]
    fn a_job_in_a_for_s_body_has_its_own_text() {
        // The prototype's review: the header went into the job's text, and
        // taking a `!` off it a byte at a time panicked past a non-ASCII
        // name.
        for (line, text) in [
            ("for x in a; do sleep 5 & done", "sleep 5"),
            ("for x do sleep 1 & done", "sleep 1"),
            ("for x in a; do ! sleep 1 & done", "sleep 1"),
            ("for \u{e9} do ! sleep 1 & done", "sleep 1"),
            ("for x in a b\ndo sleep 2 &\ndone", "sleep 2"),
        ] {
            let p = parse_line(line).unwrap().items.remove(0).and_or.first;
            let Run::Compound(c) = p.run else {
                panic!("{line}");
            };
            let Compound::For(f) = *c else {
                panic!("{line}");
            };
            assert_eq!(f.body.items[0].background.as_deref(), Some(text), "{line}");
        }
    }

    #[test]
    fn a_for_s_header_takes_no_operator() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `parser::tests::a_job_in_a_for_s_body_has_its_own_text`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
                    for _ in 0..self.parts.bangs {
                        text = text[1..].trim_start_matches([' ', '\t']);
                    }
````

with:

````rust
                    for _ in 0..self.parts.bangs {
                        text = text
                            .strip_prefix('!')
                            .unwrap_or(text)
                            .trim_start_matches([' ', '\t']);
                    }
````

Replace:

````rust

    /// A word of a `for`'s header (stage `stage`): its name, `in`, a word
    /// of its list or `do`; anything else is bash's error naming it.
    fn for_word(&mut self, stage: Stage, w: Word) -> Result<(), ParseError> {
        let Some(open) = self.open.last_mut() else {
````

with:

````rust

    /// A word of a `for`'s header (stage `stage`), which ends at `at`: its
    /// name, `in`, a word of its list or `do`; anything else is bash's
    /// error naming it.
    fn for_word(&mut self, stage: Stage, w: Word, at: usize) -> Result<(), ParseError> {
        let Some(open) = self.open.last_mut() else {
````

Replace:

````rust
                open.stage = Stage::Body;
            }
````

with:

````rust
                open.stage = Stage::Body;
                // A job's text starts after it, as after other keywords.
                self.item_start = at;
            }
````

Replace:

````rust
            return match core::mem::take(&mut self.word).finish(line) {
                Some(w) => self.for_word(stage, w),
                None => Ok(()),
````

with:

````rust
            return match core::mem::take(&mut self.word).finish(line) {
                Some(w) => self.for_word(stage, w, at),
                None => Ok(()),
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 358 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): give a job in a for's body a text of its own

The prototype's review found a background job in a `for` loop's body
with its header in its text (`for x in a; do sleep 5 & done` showed
`do sleep 5`), and the `!` of `for é do ! a & done` taken off a byte at
a time, which panicked inside the `é`. A job's text now starts after
the header's `do`, as after other keywords, and a `!` comes off as a
character (programmable shell gate §15 item 2).
EOF
````


### Task 18: The corpus runs `while`, `until` and `for` as bash runs them

Spec §11.2: the corpus scripts `loops.sh` and `for.sh` check, against bash 5.2, what each loop runs and its status: loops that never run, conditions read through `grep` to end them (no `test` until plan 3), `$?` in a body, `!` and lists, `for` over words, none, `"$@"` (empty in the corpus) and keywords, across lines, and nested with `if` and each other. It tests what exists, so there is no failing run: the mutation checks of Tasks 12 and 16 break what it reaches.

**Files:**
- Create: `crates/shell/tests/corpus/for.sh`
- Create: `crates/shell/tests/corpus/loops.sh`

**Interfaces:**
- Consumes: Tasks 12, 16.
- Produces: `crates/shell/tests/corpus/{loops.sh, for.sh}`.

- [ ] **Step 1: Create `crates/shell/tests/corpus/for.sh`**

Create `crates/shell/tests/corpus/for.sh`:

````bash
# for: the body once for each word, the variable set first and kept.
for x in a b c; do echo "$x"; done
for x in a b; do echo "$x"; done; echo "last $x"
for x in; do echo no; done; echo $?
for x in a 'b c' "" $E; do echo "[$x]"; done
for x in if then do done; do echo "$x"; done
for done in a; do echo "$done"; done
for x in a b; do false; done; echo $?
# Without words, or with "$@", over the arguments: none here.
for x; do echo no; done
for x do echo no; done
for x in "$@" a; do echo "[$x]"; done
# Across lines, and nested.
for x in a b
do
  echo "$x"
done
for x
in c d
do echo "$x"; done
for x in a b; do for y in 1 2; do echo "$x$y"; done; done
for x in a b c; do if echo "$x" | grep b; then echo found; fi; done
for x in a b; do H=; until echo "$H" | grep "$x"; do H=$x; done; done
````

- [ ] **Step 2: Create `crates/shell/tests/corpus/loops.sh`**

Create `crates/shell/tests/corpus/loops.sh`:

````bash
# while and until: the body runs while the condition allows; the status
# is the body's last, 0 when the body never runs.
while false; do echo no; done; echo $?
false; until true; do echo no; done; echo $?
A=
while ! echo "$A" | grep xxx; do A=${A}x; echo "pass $A"; done
B=
until echo "$B" | grep yy; do B=${B}y; done; echo $?
C=
while echo "$C" | grep -v zz; do C=${C}z; false; done; echo $?
# The condition's status is $? in the body; a loop in lists, negated.
D=
until echo "$D" | grep d; do echo "status $?"; D=d; done
! while false; do true; done; echo $?
true && while false; do true; done && echo and
# Across lines, nested with if.
E=
while
  ! echo "$E" | grep ee
do
  E=${E}e
  if echo "$E" | grep -v ee; then
    echo "one $E"
  else
    echo "two $E"
  fi
done
F=
until echo "$F" | grep ff; do G=; while ! echo "$G" | grep gg; do G=${G}g; done; F=${F}f; done
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 358 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): run while, until and for as bash runs them

The corpus scripts `loops.sh` and `for.sh` check, against bash 5.2,
what each loop runs and its status: loops that never run, conditions
read through `grep` to end them, `$?` in a body, `!` and lists, `for`
over words, none, `"$@"` and keywords, across lines, and nested with
`if` and each other (programmable shell gate §11.2).
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 48 scenario(s) passed`.

````bash
git push -u origin m4p2/loops
gh pr create --base main --head m4p2/loops --title "feat(shell,kernel): run loops and end them at Ctrl-C" --body-file - <<'EOF'
## What

Milestone 4, plan 2, Tasks 10–18: `wait(0, WAIT_NOHANG | WAIT_CTRL_C)` asks whether a raw Ctrl-C was typed (it was `EINVAL`; ABI 3 unchanged); the walker asks before each command, so a list or loop of built-ins ends at Ctrl-C, and `$?` after Ctrl-C is bash's 130 but for a lone pipeline; a line-mode Ctrl-C for a group that has ended waits for the console's next holder, and the interactive shell takes the console back after each program; `while`, `until` and `for` run with bash's statuses and errors; a job in a `for`'s body has its own text; the corpus's `loops.sh` and `for.sh`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `control`

## Hardware

- [x] Not needed now: the kernel's change (a Ctrl-C asked for, or kept for the console's next holder) is the console's, which QEMU drives the same; plan 4's NUC check 6 presses Ctrl-C in a loop by hand on the NUC
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p2/loops --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-loops
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: `help` and the scenario `control` (Tasks 19–20)

`help` names the compound commands; the scenario `control` types them across lines at the `> ` prompt and runs a script of nested loops under `/bin/sh`.

Branch `m4p2/control`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-control`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m4p2/control /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-control origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-control
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m4p2/loops` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m4p2/control /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-control m4p2/loops`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m4p2/loops>` and re-run `cargo xtask ci` before pushing.

### Task 19: `help` names `if`, `while`, `until` and `for`

Spec §2: `help`'s syntax lines say what `if`, `elif`, `else`, `while`, `until` and `for` run and that each may go on across lines at the `> ` prompt, within 72 columns, after the built-ins (so they stay on the NUC's 33 rows). No scenario or check script prints `help`. The red run is `help`'s test, which looks for the new words. Mutation check: the last line dropped fails it.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
            "`! ",
        ] {
````

with:

````rust
            "`! ",
            "`if ",
            "elif",
            "else",
            "`while ",
            "`until`",
            "`for ",
            "`> `",
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::help_lists_every_command_with_its_description`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
         `NAME=value` sets `$NAME`."
    );
````

with:

````rust
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
         `NAME=value` sets `$NAME`. `if a; then b; fi` runs b if a succeeds,\n\
         with `elif c; then d;` and `else e;` before `fi` for other cases;\n\
         `while a; do b; done` repeats b while a succeeds, `until` while it\n\
         fails; `for x in w...; do b; done` runs b with each w as `$x`, and\n\
         `for x; do` with each argument. Each may go on across lines, at `> `."
    );
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 358 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): name if, while, until and for in help

`help`'s syntax lines say what `if`, `elif`, `else`, `while`, `until`
and `for` run, and that each may go on across lines at the `> ` prompt,
within 72 columns, after the built-ins.
EOF
````


### Task 20: The scenario `control` runs compound commands at the prompt and in a script

Spec §11.3, §5.4, §5.5: the scenario `control` types an `if` and a `for` across lines at the `> ` prompt, drops one with Ctrl-C there (`$?` 130) and one with a syntax error inside, to its `fi`, with `> ` until then; and runs a script of nested `for` loops under `/bin/sh`, each of its lines traced once before they run, as its transcript shows too. It tests what exists, so there is no failing run.

**Files:**
- Modify: `tests/e2e/control.txt`

**Interfaces:**
- Consumes: Tasks 7–19.
- Produces: `tests/e2e/control.txt`'s lines for constructs at the prompt and in a script.

- [ ] **Step 1: Expect the new lines in `tests/e2e/control.txt`**

In `tests/e2e/control.txt`, make these 2 replacements, top to bottom:

Replace:

````text
# and a script whose command goes on across lines, with its transcript.
# Compound commands (plan 2): Ctrl-C in a loop of built-ins and of
# programs, and the nesting bound, under /bin/sh.
timeout 30
````

with:

````text
# and a script whose command goes on across lines, with its transcript.
# Compound commands (plan 2): if and for typed across lines, Ctrl-C and
# a syntax error at >, a script with nested loops and its transcript,
# Ctrl-C in a loop of built-ins and of programs, and the nesting bound,
# under /bin/sh.
timeout 30
````

Replace:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
# A loop of built-ins ends at a Ctrl-C (§5.2): /bin/sh asks the kernel
````

with:

````text
expect \n\+ echo s1 &&\n\+ echo s2\ns1\ns2\nroot@relay:~# $
# An if and a for typed across lines, with > for each line after the
# first; Ctrl-C at > drops one, status 130; a syntax error inside one
# drops it to its fi, with > until then.
send if true
expect \n> $
send then echo yes
expect \n> $
send fi
expect \nyes\nroot@relay:~# $
send for x in a b
expect \n> $
send do echo $x
expect \n> $
send done
expect \na\nb\nroot@relay:~# $
send if true; then
expect \n> $
type {ctrl-c}
expect \^C\nroot@relay:~# $
send echo $?
expect \n130\nroot@relay:~# $
send if true; then
expect \n> $
send echo no; then
expect \nrelay-sh: syntax error near unexpected token `then'\n> $
send echo no
expect \n> $
send fi
expect \nroot@relay:~# $
# A script with nested loops: each line traced once as it is read, all
# before the loops run, and the same in its transcript.
send echo 'for x in a b' > loops.sh
expect \nroot@relay:~# $
send echo do >> loops.sh
expect \nroot@relay:~# $
send echo '  for y in 1 2; do echo $x$y; done' >> loops.sh
expect \nroot@relay:~# $
send echo done >> loops.sh
expect \nroot@relay:~# $
send sh loops.sh
expect \n\+ for x in a b\n\+ do\n\+ for y in 1 2; do echo \$x\$y; done\n\+ done\na1\na2\nb1\nb2\nroot@relay:~# $
send cat loops.log
expect \n\+ for x in a b\n\+ do\n\+ for y in 1 2; do echo \$x\$y; done\n\+ done\na1\na2\nb1\nb2\nroot@relay:~# $
# A loop of built-ins ends at a Ctrl-C (§5.2): /bin/sh asks the kernel
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
test(e2e): run compound commands at the prompt and in a script

The scenario `control` types an `if` and a `for` across lines at the
`> ` prompt, drops one with Ctrl-C there (`$?` 130) and one with a
syntax error inside, to its `fi`; and runs a script of nested `for`
loops under /bin/sh, each of its lines traced once before they run,
as its transcript shows too (programmable shell gate §5.4, §5.5,
§11.3).
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 48 scenario(s) passed`.

````bash
git push -u origin m4p2/control
gh pr create --base main --head m4p2/control --title "feat(shell,e2e): name compound commands in help and run them in control" --body-file - <<'EOF'
## What

Milestone 4, plan 2, Tasks 19–20: `help` names `if`, `while`, `until` and `for`; the scenario `control` types an `if` and a `for` across lines at the `> ` prompt, drops one with Ctrl-C there and one with a syntax error inside, and runs a script of nested loops under /bin/sh with its trace and transcript.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests and the QEMU scenario `control`

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; no check script uses a compound command, and plan 4's NUC check 6 runs them there
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Wait for CI**

Wait until `lint`, `unit` and `e2e` are green: `gh pr checks m4p2/control --watch`, or the harness's pull-request monitor. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m4p2-control
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
