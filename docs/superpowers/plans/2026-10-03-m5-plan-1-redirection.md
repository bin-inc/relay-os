# Milestone 5 · Plan 1: Redirection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/bin/sh` redirects every standard stream (spec §7): `<`, `0<`, `>`, `>>`, `2>`, `2>>`, `2>&1`, `1>&2` and `>&2`, several per command made left to right, on any command of a pipeline as §7.3 allows, and on compound commands for everything inside (§7.4); a command's own messages and a built-in's errors go to its fd 2; `sh FILE` refuses a redirected fd 2 and reads a file given as input; `head -n` leaves a file just after its lines; and the drop scan's three gaps left by milestone 4 are mended. Milestone 5's roadmap and the spec's §15 item 5 land with this plan. It ends with `cargo xtask ci` green, 50 scenarios (the new `redirect` among them) and no NUC check.

**Architecture:** The parse tree's one `> file` becomes a list of redirections (`parser::Redirect { fd, op }`) on a command and on a compound command's pipeline. A new module, `crates/shell/src/fds.rs`, is the fd context the walker carries: what fds 0, 1 and 2 are (the shell's own, a file a redirection opened, or in a pipeline a pipe) and the files open, each counted by the fds that hold it and closed when none does. Under `/bin/sh` a file is an fd of the shell's (`Programs::open_output`, `open_input`), handed to a program at `spawn` with its other two (`[u32; 3]`) and written by a built-in through `Programs::write`, so programs and built-ins share its offset and the shell's own fds 0 to 2 never change; in the in-process runner a file is its node and a shared offset. `Ctx` gains an error target and an input file; the walker sends a command's own messages to its fd 2 and its reports to the context's (`Shell::say_on`). relay-rt gains `is_screen`, `open_input`, `write` and `seek_back`; the ABI does not change.

**Tech Stack:** Rust 1.98.1 stable, `no_std + alloc`, no new third-party crates; bash 5.2.21 and GNU coreutils 9.4 for comparison; QEMU 8.2 under KVM with `-cpu max`.

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (§7, §10, §11.1–§11.3, §12, §15 items 2 and 5)
**Roadmap:** `docs/superpowers/plans/2026-10-03-milestone-5-roadmap.md` — this is plan 1 of 4 of milestone 5.

## In brief

- **Size.** 24 tasks in three code pull requests, plus this plan as PR 1: PR 2 a simple command's redirections (the tree's list, the fd context through both runners, several per command, `2>`, `>>` at a file's end, the copies, `<`, an input that is the output refused, `sh FILE`, `head`, `help`, the corpus's order script); PR 3 pipelines, the last command's own messages and the refusals' names, targets expanded as reached, compound commands, the drop scan pinned, the corpus's constructs and the scenario `redirect`; PR 4 the drop scan's three gaps.
- **The fd context** (decision 3; the spike ran every scenario with it under `/bin/sh` and `host-shell`, all green): no `dup`, the shell's fds never change; a file is closed once no fd holds it, so a replaced file closes at once; built-ins and programs share a file's offset under `/bin/sh` and in the in-process runner alike (`for x in a b; do echo $x; help; done > f` keeps its order).
- **Where messages go** (decision 4, bash 5.2 probed for every case, `tmp/m5p1/probes/`): a command's own messages to its fd 2, a failed redirection on fd 2 as it stands then, the shell's reports of a command (an expansion error, a program killed, a job's `[1] 42`, a construct's own error) on the context's fd 2, `^C` on the screen.
- **Two decisions of the user's:** `sh FILE` refuses a redirected fd 2, which holds its trace, and reads a file given as input (decision 5); `head -n` seeks a file back to just after its lines, as GNU's does, so a loop of `head -n 1` reads line after line (decision 6).
- **The prototype's review.** A fresh reviewer read the whole prototype (then nineteen tasks), ran its tests, throwaway tests in both runners against bash 5.2 and GNU, and throwaway QEMU scenarios (an fd census of 40 rounds, 31 nested redirected loops, a 32 MiB disk). It found 0 critical, 1 important and 5 minor real defects, each settled in a task of its own after the task it concerns, with a test that fails first (or, for the untested guard, a mutant it catches); two design calls came to the user (decision 9). It found correct: every form and refusal of §7.1 against bash, both orders of `2>&1`, failures told on fd 2 as it stands, `$E` and `${1A}` targets, the built-ins with redirections in both runners, every fd closed once on every path traced (failed redirection, failed spawn, `exit`, Ctrl-C, a script in a context, a job in a construct), no fd leaked over 40 rounds, 31 nested levels ending with `Too many open files` and a working shell, `sh FILE`, `head`'s seek, the pipelines' ends, the drop scan's new levels and keywords, and every commit's widths. It declined to judge `a > /nodir/x &` (no job, status 1, as at milestone 4), `grep -q`'s offset on inputs larger than its read, and each PR's safety in QEMU on its own (CI ran each PR's tip).

| Finding (review) | Decision |
|---|---|
| Important (I-1): with `<`, `cat < f >> f` and `grep x < f >> f` read their own output until the disk was full (GNU refuses them) | Fixed, Task 9; with the user's choice to take GNU's condition for file operands too, so `cat f > f` (emptied by `>`) is silent with status 0 |
| Minor (M-1): in the in-process runner two fds appending to one file wrote over each other | Fixed, Task 5 |
| Minor (M-2): in the in-process runner a pipeline's last command's own message went to the screen, not its fd 2 | Fixed, Task 15 |
| Minor (M-3): under `X \| sh > f` a built-in's output, and the shell's messages after `2>&1`, take the shell's fd 1 for the screen (as since milestone 3) | Deferred to plan 4 (the user's choice), a roadmap note |
| Minor (M-4): four messages named a token nobody typed (a redirection alone in a pipeline, `1>&x`, an fd in a `for`'s words) | Fixed, Task 16 |
| Minor (M-5): no test noticed a copy dropping its slot before holding its file (`> f 1>&1` would reach `unreachable!`) | Pinned, Task 7 |

## Where this plan fits

Plan 1 of milestone 5 is the gate's fifth step (spec §12). It builds on milestone 4's grammar tree, walker, both runners and drop scan, and on its plan 4's notes (the drop scan's gaps and M-2). Plan 2 (ABI 4 and `/dev`) gives `/dev/null`, which the `redirect` scenario's last lines need, and `statfs`'s read-only flag; plan 3 the shell's environment and `cd`; plan 4 `check7.sh`, version 0.6.0 and the NUC checks. The roadmap's notes carry what this plan leaves for them.

## Working conventions

- Plan 1 lands as **four pull requests** (table below). This plan, with milestone 5's roadmap and the spec's §15 item 5 (with the §10 line and status line it changes), is PR 1. Each code PR starts by creating its own branch and worktree under `/home/maw/src/bin-inc/relay-os-impl/worktrees/` from `origin/main`, and ends by running `cargo xtask ci`, opening the PR and waiting for green CI. The user reviews and merges every PR; never merge yourself. While a PR waits for review the next one may be prepared on a local branch stacked on it and pushed after the merge (see each "Start the branch" step). After a merge, rebase only the next PR (`git rebase --onto origin/main <its predecessor's old tip>`, the old tips recorded in the ledger first), and re-run `cargo xtask ci` unless the tree's hash is unchanged. Compare a merged branch's tree with `origin/main`'s before deleting it, and check that the merged `main` holds everything its PR had. Run every command from the current PR's worktree root. Temporary files go into `/home/maw/src/bin-inc/relay-os-impl/tmp/`, never `/tmp` and never inside the repository.
- The executing-plans ledger lives in `/home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/` and is linked into each worktree as `.superpowers/sdd` (the "Start the branch" steps do this), because worktrees are removed after their merge. Worktrees do not contain this plan until PR 1 merges, so the executor passes one copy of it, `tmp/sdd-ledger/plan-files/2026-10-03-m5-plan-1-redirection.md`, to every task, and all tasks share one workspace directory. Record every branch tip in the ledger before rebasing anything.
- File steps come in five forms:
  - "Create `p`:" (complete contents);
  - "Create `p` containing only this test module" (the tests first);
  - "Insert this at the top of `p`, above `#[cfg(test)]`:" (the implementation after the tests failed);
  - "Replace the whole of `p` with:";
  - "In `p`, replace: … with: …". Each snippet occurs exactly once in the file at that point; several replacements in one step apply top to bottom.
- Each task first adds its failing tests (unit tests in each file's test module or a new file's; corpus scripts under `crates/shell/tests/corpus/`; scenarios under `tests/e2e/`), runs them to watch them fail for the stated reason, then implements. Every "Run" step states the expected outcome. If you see anything else, stop and use superpowers:systematic-debugging; do not change the expectation to match. Task 1 is a refactor and Tasks 7, 13, 19, 20 and 21 add tests only: they have no failing run; each names the mutants of the code its tests catch. The mutation checks the prototype ran are named in each task's introduction (all in `tmp/m5p1/mutations.md`).
- **The expectations are bash's and GNU's.** Every message, refusal, status and file of a redirection was compared with bash 5.2 (interactive in a pty, `tmp/m4p2/pty_bash.py`, and scripts; `tmp/m5p1/probes/p1.txt`–`p13.txt`) running GNU's programs, and the corpus compares the in-process runner with bash on every `cargo xtask unit`. In this session's shell `grep` may be a wrapper of another tool: any `grep` typed by hand should be `/usr/bin/grep`.
- **Bound every loop in a test.** The fd context holds at most three fds a level; the drop scan's loops are bounded by its input, and it remembers 64 levels of `$(…)` in double quotes at most (deeper drops the rest). Mutation checks run under an address-space limit and a timeout (`tmp/m5p1/mutate.py`). A probe must not loop in bash: `while head -n 1; do …; done` never ends, as GNU's `head` exits 0 at the end of its input.
- Every task ends with `cargo xtask lint` and a commit. Commit subjects and PR titles follow `CONTRIBUTING.md` (Conventional Commits): a task's scopes name the code modules whose behaviour it changes, at most two; the tests, corpus scripts and scenarios that come with the change belong to its commit. Every commit body line is within 72 columns, counted in characters. Chain a commit after a check with `&&`, never `;`, and on the check's own status (a `| grep` after it hides a failure). Commit messages and pull-request descriptions carry no attribution lines.
- The QEMU scenarios are timing-sensitive. The scenario parser refuses an input that no `expect` of the prompt paces (`send-ahead` and its kin mark input sent while something runs on purpose, `type-ahead {ctrl-c}` a Ctrl-C). If one fails oddly after hours of builds, check `sensors` first: the host NUC overheats before blaming code. Run scenarios one at a time when in doubt.
- This plan was generated by replaying every task, in order, into a fresh clone of `origin/main` at `877b874` with PR 1's spec and roadmap changes applied, and running every command (the push, pull-request and review steps are the exceptions). After each task the replayed tree was compared with the prototype it came from. Test counts and outputs in "Expected" are what that replay produced.

## Pull requests

| PR | Branch | Tasks | Delivers | Checks on the PR |
|---|---|---|---|---|
| 1 | `m5p1/plan` | — | Milestone 5's roadmap, the spec's §15 item 5 with its §10 line and status line, this plan | `lint`, `unit`, `e2e` |
| 2 | `m5p1/simple` | 1–13 | A simple command's redirections: the list, the fd context, several per command, `2>`, `>>` at a file's end, the copies, `<`; messages on fd 2; an input that is the output refused; `sh FILE`; `head`; `help`; the corpus's order script | `lint`, `unit`, `e2e` |
| 3 | `m5p1/compound` | 14–21 | Pipelines over their pipes, the last command's own messages, refusals naming what was typed; targets expanded as reached; compound commands; the drop scan pinned; the corpus's constructs; the scenario `redirect` | `lint`, `unit`, `e2e` |
| 4 | `m5p1/scan` | 22–24 | The drop scan's gaps: quotes in a substitution in double quotes, `time -p` and `coproc`, two subshells read as arithmetic | `lint`, `unit`, `e2e` |

## Global Constraints

- Toolchain: stable Rust pinned to `1.98.1`; no nightly features; all assembly inline (`asm!`, `naked_asm!`); no `.S` files.
- Crate policy (M1 §3.2, UG §3.2): `kernel/` and `crates/*` may use only `x86_64`, `bitflags` and `spin` (plus workspace crates); `xtask/` and dev-dependencies may use anything. Plan 1 adds no crate. `crates/shell` uses only `vfs` and `relay-abi` and is `no_std` outside its tests.
- Architecture-specific code only under `kernel/src/arch/` and `relay-rt`'s `arch` (UG §3.1). Plan 1 changes no ABI value or struct: `relay_abi::VERSION` stays 3, and `spawn` already maps any of the caller's fds to the child's.
- Every scenario and check script of milestones 1–4 passes unchanged but `fileops`, which gains a line of an input that is the output (Task 9); the scenarios gain `redirect` (Task 21). No NUC transcript changes: check 4's `cat f >> f` is still refused.
- What a person types is untrusted (AGENTS.md): nothing that follows from it may panic, index out of bounds, overflow, allocate without bound or loop forever; the drop scan never fails. An fd the shell opens is closed once no fd of any context holds it.
- The shell follows bash 5.2 and commands GNU word for word; the only decided differences are spec §10's and §15's.
- Missing tools fail tests, never skip them: bash and GNU's coreutils are on CI's runner.
- Every PR must pass `cargo xtask ci`. The workflow only installs and sets up tools and calls xtask; checks are never added to the YAML directly. CodeQL runs on every PR: triage any new alert, and ask the user before dismissing any.
- An intermediate PR must be green, and safe, on its own: output changes land with the tests and scenarios that expect them, in the same task.
- No tag or GitHub release without the user's word.

## Decisions and spec revisions introduced by this plan

The spec gets these as §15 item 5 in PR 1:

1. **The roadmap.** Milestone 5 keeps §12's four plans; no cut line is needed (§1.2): milestone 4 took a few days of the gate's one to two months, and redirection of compound commands is the same fd context that several redirections on one command need. Plan 1 is four pull requests: its plan with milestone 5's roadmap; the forms of §7.1 on a simple command, with the fd context, both runners, the built-ins' error target and `head`'s seek; pipelines and compound commands (§7.3, §7.4) with the scenario `redirect`; the drop scan's gaps. It changes no ABI: `spawn` already maps any of the caller's fds to each of the child's, handing it the caller's open file and its offset. It has no NUC check; plan 4's `check7.sh` runs redirection there.
2. **The forms** (§7.1). An fd is 0, 1 or 2 written as one unquoted digit, a word of its own before the operator (`a2>f` is the word `a2`, `"2">f` the word `2`), as bash takes it; bash's other numbers (`3>`, `02>`, `10>&1`) are refused, so is an fd 0 with `>` or one of 1 or 2 with `<`. `N>&M` copies fd M (1 or 2) as it is then into fd N (1 or 2), `>&2` meaning `1>&2`; a blank may follow `>&`, as bash allows (`2>& 1`), and `1>&1` and `2>&2` change nothing. A `>&` whose target is not a bare `1` or `2` (`2>&"1"`, `2>&$N`, `>&-`, `>&f`) is refused, where bash expands it. `&>`, `<&`, `<>`, `>|`, `<<`, `<<-` and `<<<` stay refused; `2>>&1`, `> &2` and an operator with no word after it are bash's syntax errors naming the token. A line of only redirections makes or opens its files, status 0.
3. **The fd context** (§7.2, §7.4). The walker carries what fds 0, 1 and 2 are: the shell's own, or a file a redirection opened. A command's redirections are made left to right over its context once its words are expanded, each target expanded as it is reached (one that does not expand is told on fd 2 as it stands then, and a bad substitution abandons the line too, as in bash); a compound command's once, before its first command, which with every command inside starts from them, and after them only an operator may follow (`fi > f fi` is bash's syntax error). Files are counted by the fds that hold them and closed when none does, so `> f > g` closes `f` before `g` opens and `2>&1 > f` keeps the old fd 1 as fd 2; a command's are closed when it ends, a construct's after its last command. Under `/bin/sh` a file is an fd of the shell's, opened for the command and mapped to the child's fds 0 to 2 at `spawn`, the shell's own fds 0 to 2 never changing; a built-in writes to it through that fd, so it shares the file's offset with the programs around it (`for …; do echo a; help; done > f` keeps its order). In the in-process runner a file is its node and an offset every fd of it shares, one opened with `>>` written at its end each time, as the kernel's append is (the prototype's review). A process has 32 fds, so redirections nested past them fail with `Too many open files`, status 1.
4. **Where messages go** (§7.5), as bash's: a command's own messages (`command not found`, a program that cannot start, a built-in's errors and its `write error`) go to its fd 2 once its redirections are made (`nope 2> e` writes into `e`); a redirection that fails is told on fd 2 as it stands then (`cat 2> e < missing` into `e`, `cat < missing 2> e` on the screen), the command does not run, status 1, and its files are closed; what the shell says of a command (an expansion error, a program killed, a background job's `[1] 42`, a construct's own error such as ``for``'s `not a valid identifier`) goes to fd 2 of the context around it, so inside `for …; done 2> e` into `e`. Syntax errors, `^C`, the reports at the prompt and a failed transcript stay on the screen. Output to a file reaches neither the screen nor a transcript, and a command with `< f` still gets the console's foreground, so Ctrl-C reaches it.
5. **`sh FILE`** (§7.5). `sh` refuses fd 1 or fd 2 that is not the console (`sh: a script's output cannot be redirected`, status 1): its trace and errors would miss its transcript (the spike's `sh x.sh 2> se` wrote its trace into `se`). `< f` is allowed: the script's commands read `f` (the maintainer, 2026-10-03).
6. **`head` on a file** (§7.4). GNU's `head -n` leaves a standard input it can seek just after the lines it printed, so in `for …; do head -n 1; done < f` each pass reads the next line; `head` here did not, and now seeks back the bytes it read past its last line, a failure (a pipe, the console) changing nothing, as GNU's (the maintainer, 2026-10-03). `grep`, `wc`, `cat` and `tail` read to the end, as GNU's do.
7. **In a pipeline** (§7.3). `<` on a later command is `unsupported syntax: < after |`, `>` and `>>` on an earlier one `> before |` as before; `2>`, `2>>` and the copies may stand on any command, applied over the pipes, so `a 2>&1 | b` sends both of `a`'s outputs into the pipe. A stage whose redirection fails runs nothing and leaves its neighbours an end; the pipeline's status is 1 when it is the last.
8. **The drop scan** (§15 item 2) reads every new form as bash does: a redirection's target is a word, never a keyword or a here-document's delimiter, and `<<<` has no body; a redirection may follow `fi` and `done`. It also mends milestone 4's gaps: quotes inside `$(…)` inside double quotes start afresh, as bash reads them; `time` with its options and `coproc` (`coproc {`, `coproc NAME {`) are command positions; and a `((` whose first `)` is not followed by another, two subshells to bash, is read on as two subshells when nothing before that `)` could read otherwise (no `<<`, keyword, quote or `$(`), and otherwise drops the rest of the script, as before.
9. **An input that is the output** (the prototype's review). With `<`, `cat < f >> f` and `grep x < f >> f` read their own output until the disk was full. `cat` and `grep` now take GNU's condition, for standard input and file operands alike: the output is a regular file, the input's file, and the input has bytes left to read (`cat: -: input file is output file`, status 1; `grep: (standard input): input file is also the output`, status 2). So `cat f > f`, whose `>` has emptied `f`, is silent with status 0, as GNU's is, where milestone 1 refused it (the maintainer, 2026-10-03). A shell whose own fd 1 is a file (`X | sh > f`) still writes a built-in's output and its own messages after `2>&1` to the screen, as since milestone 3: plan 4's (the maintainer).

## Review Focus

1. **Redirections at the prompt and in scripts under `/bin/sh`**, every form and refusal of §7.1, several per command in both orders, a missing file, a directory, an empty `$E` or `${1A}` target, the same file twice, a line of only redirections, a built-in redirected (`cd`, `help`, `jobs`, `wait`, `kill`, `exit`), a background job. Expected: bash 5.2's output, files, messages on the right fd and statuses, but for the decided differences. Tests: `redirection_errors`, `several_redirections_are_made_left_to_right`, `standard_error_may_go_to_a_file`, `one_output_may_be_made_a_copy_of_the_other`, `standard_input_may_be_a_file`, `a_redirection_s_target_is_expanded_when_it_is_reached` (Tasks 3–8, 17), the corpus's `redirect_order.sh` (Task 13), the scenario `redirect` (Task 21).
2. **The fd context's bookkeeping:** every fd the shell opens closed exactly once (a program started or not, Ctrl-C, `exit` inside a redirected construct, a script, a job started in a redirected loop), the counts never underflowing, a copy onto itself, `>>` at a file's end, 32 fds a process. Expected: no fd leaked and no `unreachable!` reached; both runners agree. Tests: the `fds` module's, `a_built_in_writes_its_redirection_through_the_shell_s_fd` (Task 2), `a_file_appended_to_is_written_at_its_end_each_time` (Task 5), `a_copy_of_an_fd_onto_itself_keeps_its_file` (Task 7), `a_compound_command_s_redirections_hold_for_everything_inside` (Task 18).
3. **An input that is the output:** `cat < f >> f`, `grep x < f >> f`, `cat f > f`, `cat f >> f`, `grep -c`/`-q`, under `/bin/sh` and in-process. Expected: GNU's refusal, message and status, or GNU's silence; the disk never filled. Tests: `cat_refuses_to_append_a_file_to_itself`, `grep_never_reads_its_own_output`, `a_file_is_not_its_own_output`, the scenario `fileops` (Task 9).
4. **Messages and the transcript:** a command's own messages on its fd 2, the shell's reports on the context's, `^C` on the screen, a pipeline's last command's, a script's transcript getting nothing sent to a file, `sh FILE 2> e` refused, `sh FILE < f` read. Tests: `bin_sh_gives_a_program_a_file_as_fd_2`, `sh_errors`, `a_script_s_input_may_be_a_file` (Tasks 4, 10), `a_pipeline_s_commands_redirect_over_its_pipes` (Tasks 14, 15), `a_job_s_number_goes_to_the_fd_2_around_it` (Task 18).
5. **Pipelines and compound commands:** `a 2>&1 | b`, `a 1>&2 | b`, a stage whose redirection fails, `<` and `>` placement and the refusals' names, `fi > f`, `done < f 2>&1`, `head -n 1` in a loop on a file. Tests: Tasks 14–18's, the corpus's `redirect_compound.sh` (Task 20).
6. **The drop scan** (Tasks 19, 22–24): every new form, quotes in a substitution in double quotes, `time -p` and `coproc`, `((a); b)`. Expected: the drop ends where bash's construct ends, or later, never earlier; no panic. Tests: `a_dropped_command_s_redirections_end_its_drop_where_bash_s_end`, `quotes_and_escapes_go_on_across_lines_as_bash_s_do`, `refused_syntax_is_read_as_bash_reads_it`, `arithmetic_holds_no_here_document`, and the `piped` tests of each task.

---

## PR 1: Milestone 5's roadmap, the spec's §15 item 5 and this plan

Milestone 5's roadmap; the spec's §15 item 5, the decisions of this plan, with the §10 line and the status line it changes; and this plan. They change no code, but CI runs on every pull request.

- [ ] **Commit and open the pull request**

The roadmap's and the spec's changes are the prototype's first commit, `docs(spec,plan): record milestone 5 plan 1's decisions and roadmap`; this plan goes with them.

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p1/plan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-plan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-plan
git fetch /home/maw/src/bin-inc/relay-os-impl/tmp/m5p1/proto refs/tags/p0 && git cherry-pick FETCH_HEAD
cp /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger/plan-files/2026-10-03-m5-plan-1-redirection.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-10-03-m5-plan-1-redirection.md
git commit -m "docs(plan): add milestone 5 plan 1, redirection"
cargo xtask lint
git push -u origin m5p1/plan
gh pr create --base main --head m5p1/plan --title "docs(spec,plan): add milestone 5's roadmap and plan 1, redirection" --body-file - <<'EOF2'
## What

Milestone 5's roadmap (four plans as spec §12, no cut line, ABI 4 whole in plan 2, no NUC check before plan 4, the carried items placed) and the implementation plan of its plan 1 ("Redirection"), with the spec's §15 item 5: four pull requests; the forms of §7.1 and their refusals (only fds 0 to 2 as one digit, a `>&` taking a bare `1` or `2`, a line §10 gains); the fd context the walker carries, no `dup`, a file closed once no fd holds it, built-ins and programs sharing its offset; where each message goes; `sh FILE` refusing a redirected fd 2 and reading a file; `head -n` seeking a file back after its lines; the rules in a pipeline; the drop scan's reading of the new forms and its three mended gaps.

## How it was tested

- [x] Every task of the plan was replayed into a fresh clone and every command run; each PR's end state passed `cargo xtask ci`

## Hardware

- [x] Not needed: documents only
EOF2
````

- [ ] **Bind the pull request, wait for CI and hand over for review**

Bind the pull request to the session (the harness's `bind_pr`); never poll CI. When `lint`, `unit` and `e2e` are green, ask the user to review and merge. PR 2 may be prepared meanwhile, stacked on `m5p1/plan`. After the merge, remove the worktree with `git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-plan` (`.superpowers` is not linked there) and continue with PR 2.

---

## PR 2: Redirection of a simple command (Tasks 1–13)

A command's redirections become a list made left to right over an fd context the walker carries, which both runners use; `2>`, `2>>`, the copies and `<` join `>` and `>>`, a command's own messages go to its fd 2, `cat` and `grep` refuse an input that is the output, `sh FILE` refuses a redirected fd 2, `head` leaves a file after its lines, `help` names the forms, and the corpus compares their order with bash.

Branch `m5p1/simple`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-simple`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p1/simple /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-simple origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-simple
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p1/plan` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p1/simple /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-simple m5p1/plan`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p1/plan>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 1: A command holds its redirections in a list

Decisions 2 and 3: a command's one `> file` (`Command::redirect`) becomes a list, `Command::redirects`, each an fd and what it is made (`Redirect { fd, op }`, `RedirectOp::{Write, Append}` for now), so that later tasks add several per command, other fds and the copies; a pipeline gains the list a compound command will take (`Pipeline::redirects`, empty until Task 18). Only `>` and `>>` on fd 1, at most one per command, are read as yet, and the runners take that file as before (`Command::output`). A refactor: the tests change only in shape, and none fails before the code, so there is no red run; every existing test passes after it.

**Files:**
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `parser::{Command, Pipeline, Parts}`, `expand::Expander::redirect`, the runners.
- Produces: `parser::Redirect { fd, op }`, `parser::RedirectOp::{Write, Append}`, `Command::redirects`, `Command::output() -> Option<(&W, bool)>`, `Pipeline::redirects`.

- [ ] **Step 1: Change the tests in `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
        let v = script();
        let target = |line: &str| {
            expand(&typed(line), &v, 0).map(|mut c| c.remove(0).redirect.unwrap().path)
        };
        assert_eq!(target("echo > $1.txt").unwrap(), "one.txt");
````

with:

````rust
        let v = script();
        let target =
            |line: &str| expand(&typed(line), &v, 0).map(|c| c[0].output().unwrap().0.clone());
        assert_eq!(target("echo > $1.txt").unwrap(), "one.txt");
````

- [ ] **Step 2: Change the tests in `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
            i.branches[0].0.items[0].and_or.first.commands()[0]
                .redirect
                .as_ref()
                .unwrap()
                .path
                .typed,
````

with:

````rust
            i.branches[0].0.items[0].and_or.first.commands()[0]
                .output()
                .unwrap()
                .0
                .typed,
````

Replace:

````rust
        let c = one(line).unwrap();
        assert_eq!(c.redirect, None);
        c.words
````

with:

````rust
        let c = one(line).unwrap();
        assert_eq!(c.redirects, []);
        c.words
````

Replace:

````rust
        let c = one("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }

````

with:

````rust
        let c = one("echo x > f # to f").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.output().unwrap().0, "f");
    }

````

Replace:

````rust
        assert_eq!(
            c.redirect.as_ref().unwrap().path.pieces,
            [text("o", true), text("ut", false)]
````

with:

````rust
        assert_eq!(
            c.output().unwrap().0.pieces,
            [text("o", true), text("ut", false)]
````

Replace:

````rust
        assert_eq!(typed, ["echo", r#"a"b c"$D"#, "2"]);
        assert_eq!(c.redirect.as_ref().unwrap().path.typed, "'$f'x#");
    }
````

with:

````rust
        assert_eq!(typed, ["echo", r#"a"b c"$D"#, "2"]);
        assert_eq!(c.output().unwrap().0.typed, "'$f'x#");
    }
````

Replace:

````rust
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "out.txt".into(),
                append: false
            })
        );
        let c = one("echo hi>>'my log'").unwrap();
        assert_eq!(
            c.redirect,
            Some(Redirect {
                path: "my log".into(),
                append: true
            })
        );
        // The redirection can come first.
        let c = one(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.redirect.unwrap().path, "f");
    }
````

with:

````rust
        assert_eq!(
            c.redirects,
            [Redirect {
                fd: 1,
                op: RedirectOp::Write("out.txt".into())
            }]
        );
        let c = one("echo hi>>'my log'").unwrap();
        assert_eq!(
            c.redirects,
            [Redirect {
                fd: 1,
                op: RedirectOp::Append("my log".into())
            }]
        );
        // The redirection can come first.
        let c = one(">f echo x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(c.output(), Some((&"f".into(), false)));
    }
````

Replace:

````rust
        );
        assert_eq!(p[0].redirect, None);
        assert_eq!(p[2].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
````

with:

````rust
        );
        assert_eq!(p[0].redirects, []);
        assert_eq!(p[2].output().unwrap().0, "out");
        // Quoted, escaped or in a comment it is a character.
````

Replace:

````rust
                words: Vec::new(),
                redirect: None
            }]
````

with:

````rust
                words: Vec::new(),
                redirects: Vec::new()
            }]
````

Replace:

````rust
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].redirect.as_ref().unwrap().path, "out");
        // Quoted, escaped or in a comment it is a character.
````

with:

````rust
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].output().unwrap().0, "out");
        // Quoted, escaped or in a comment it is a character.
````

Replace:

````rust
        let c = one("echo x > ~/out").unwrap();
        assert_eq!(c.redirect.unwrap().path, "/root/out");
        // As bash's: a quoted `/` after it, or a parameter, keeps it.
````

with:

````rust
        let c = one("echo x > ~/out").unwrap();
        assert_eq!(c.output().unwrap().0, "/root/out");
        // As bash's: a quoted `/` after it, or a parameter, keeps it.
````

- [ ] **Step 3: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

use crate::parser::{Command, Param, Piece, Redirect, Word};
use alloc::borrow::Cow;
````

with:

````rust

use crate::parser::{Command, Param, Piece, Redirect, RedirectOp, Word};
use alloc::borrow::Cow;
````

Replace:

````rust
        }
        let redirect = match &c.redirect {
            Some(r) => Some(self.redirect(r)?),
            None => None,
        };
        Ok(Command { words, redirect })
    }

    fn redirect(&mut self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let mut fields = self.word(&r.path)?;
        if fields.len() != 1 {
            return Err(Error::AmbiguousRedirect(r.path.typed.clone()));
        }
        Ok(Redirect {
            path: fields.remove(0),
            append: r.append,
        })
    }
````

with:

````rust
        }
        let mut redirects = Vec::new();
        for r in &c.redirects {
            redirects.push(self.redirect(r)?);
        }
        Ok(Command { words, redirects })
    }

    fn redirect(&mut self, r: &Redirect<Word>) -> Result<Redirect, Error> {
        let op = match &r.op {
            RedirectOp::Write(path) => RedirectOp::Write(self.target(path)?),
            RedirectOp::Append(path) => RedirectOp::Append(self.target(path)?),
        };
        Ok(Redirect { fd: r.fd, op })
    }

    /// A redirection's file, which must expand to one word.
    fn target(&mut self, path: &Word) -> Result<String, Error> {
        let mut fields = self.word(path)?;
        if fields.len() != 1 {
            return Err(Error::AmbiguousRedirect(path.typed.clone()));
        }
        Ok(fields.remove(0))
    }
````

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 15 replacements, top to bottom:

Replace:

````rust
    pub run: Run<W>,
}
````

with:

````rust
    pub run: Run<W>,
    /// A compound command's redirections; a simple command holds its own.
    pub redirects: Vec<Redirect<W>>,
}
````

Replace:

````rust

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
````

with:

````rust

/// One command: its words and its redirections, in the order typed. The
/// parser gives them as typed ([`Word`]), and expansion as the strings a
/// command gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command<W = String> {
    /// The command name first, then its arguments. Empty for a blank line.
    pub words: Vec<W>,
    pub redirects: Vec<Redirect<W>>,
}

impl<W> Command<W> {
    /// The file its standard output goes to, and whether it is appended
    /// to.
    pub fn output(&self) -> Option<(&W, bool)> {
        self.redirects.iter().rev().find_map(|r| match &r.op {
            RedirectOp::Write(path) if r.fd == 1 => Some((path, false)),
            RedirectOp::Append(path) if r.fd == 1 => Some((path, true)),
            _ => None,
        })
    }
}

/// A redirection (programmable shell gate §7.1): what fd `fd` becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect<W = String> {
    pub fd: u32,
    pub op: RedirectOp<W>,
}

/// What a redirection makes of its fd.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedirectOp<W = String> {
    /// `> path`: the file, created or emptied, written from its start.
    Write(W),
    /// `>> path`: the file, created if missing, written at its end.
    Append(W),
}
````

Replace:

````rust

/// A command of `words` and `redirect`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
) -> Result<Command<Word>, ParseError> {
    let mut leading = words
````

with:

````rust

/// A command of `words` and `redirects`. An assignment before a command,
/// which gives bash's command an environment, is not supported: programs
/// get none (user-space gate §9.4); nor is bash's `NAME+=value`, which
/// appends.
fn command(words: Vec<Word>, redirects: Vec<Redirect<Word>>) -> Result<Command<Word>, ParseError> {
    let mut leading = words
````

Replace:

````rust
    }
    Ok(Command { words, redirect })
}
````

with:

````rust
    }
    Ok(Command { words, redirects })
}
````

Replace:

````rust
    words: Vec<Word>,
    redirect: Option<Redirect<Word>>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
````

with:

````rust
    words: Vec<Word>,
    redirects: Vec<Redirect<Word>>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
````

Replace:

````rust
    fn is_empty(&self) -> bool {
        self.words.is_empty() && self.redirect.is_none() && self.compound.is_none()
    }
````

with:

````rust
    fn is_empty(&self) -> bool {
        self.words.is_empty() && self.redirects.is_empty() && self.compound.is_none()
    }
````

Replace:

````rust
        match self.pending.take() {
            Some(_) if self.redirect.is_some() => return Err(ParseError::Unsupported(">".into())),
            Some(append) => self.redirect = Some(Redirect { path: w, append }),
            // A `!` before anything of the command negates the pipeline
            // (programmable shell gate §4.1), only the first command's.
            None if w.is_bang() && self.words.is_empty() && self.redirect.is_none() => {
                if self.later {
````

with:

````rust
        match self.pending.take() {
            Some(_) if !self.redirects.is_empty() => {
                return Err(ParseError::Unsupported(">".into()));
            }
            Some(append) => {
                let op = if append {
                    RedirectOp::Append(w)
                } else {
                    RedirectOp::Write(w)
                };
                self.redirects.push(Redirect { fd: 1, op });
            }
            // A `!` before anything of the command negates the pipeline
            // (programmable shell gate §4.1), only the first command's.
            None if w.is_bang() && self.words.is_empty() && self.redirects.is_empty() => {
                if self.later {
````

Replace:

````rust
            }
            None if self.words.is_empty() && self.redirect.is_none() => {
                if let Some(k) = w.keyword() {
````

with:

````rust
            }
            None if self.words.is_empty() && self.redirects.is_empty() => {
                if let Some(k) = w.keyword() {
````

Replace:

````rust
        }
        if self.pending.is_some() || (self.words.is_empty() && self.redirect.is_none()) {
            return Err(ParseError::MissingTarget("|"));
        }
        if self.redirect.is_some() {
            return Err(ParseError::Unsupported("> before |".into()));
````

with:

````rust
        }
        if self.pending.is_some() || (self.words.is_empty() && self.redirects.is_empty()) {
            return Err(ParseError::MissingTarget("|"));
        }
        if !self.redirects.is_empty() {
            return Err(ParseError::Unsupported("> before |".into()));
````

Replace:

````rust
        self.later = true;
        command(p.words, None)
    }
````

with:

````rust
        self.later = true;
        command(p.words, Vec::new())
    }
````

Replace:

````rust
            words: Vec::new(),
            redirect: None,
        }]);
````

with:

````rust
            words: Vec::new(),
            redirects: Vec::new(),
        }]);
````

Replace:

````rust
            run: Run::Compound(Box::new(c)),
        }));
````

with:

````rust
            run: Run::Compound(Box::new(c)),
            redirects: Vec::new(),
        }));
````

Replace:

````rust
    if parts.words.is_empty() {
        match (pipeline.is_empty(), &parts.redirect) {
            // `!` alone is a command that does nothing, negated.
            (true, None) if parts.bangs == 0 => return Ok(None),
            (true, None) => {}
            (true, Some(_)) => {}
            (false, None) => return Err(ParseError::MissingTarget(end)),
            (false, Some(_)) => return Err(ParseError::Unsupported("| >".into())),
        }
    }
    let p = core::mem::take(parts);
    pipeline.push(command(p.words, p.redirect)?);
    Ok(Some(Pipeline {
        negated: p.bangs % 2 == 1,
        run: Run::Commands(core::mem::take(pipeline)),
    }))
````

with:

````rust
    if parts.words.is_empty() {
        match (pipeline.is_empty(), parts.redirects.is_empty()) {
            // `!` alone is a command that does nothing, negated.
            (true, true) if parts.bangs == 0 => return Ok(None),
            (true, _) => {}
            (false, true) => return Err(ParseError::MissingTarget(end)),
            (false, false) => return Err(ParseError::Unsupported("| >".into())),
        }
    }
    let p = core::mem::take(parts);
    pipeline.push(command(p.words, p.redirects)?);
    Ok(Some(Pipeline {
        negated: p.bangs % 2 == 1,
        run: Run::Commands(core::mem::take(pipeline)),
        redirects: Vec::new(),
    }))
````

Replace:

````rust
                    if self.parts.pending.is_some()
                        || self.parts.words.is_empty() && self.parts.redirect.is_none()
                    {
````

with:

````rust
                    if self.parts.pending.is_some()
                        || self.parts.words.is_empty() && self.parts.redirects.is_empty()
                    {
````

Replace:

````rust
        }
        if !pipeline.is_empty() && parts.words.is_empty() && parts.redirect.is_none() {
            return Err(ParseError::Incomplete);
````

with:

````rust
        }
        if !pipeline.is_empty() && parts.words.is_empty() && parts.redirects.is_empty() {
            return Err(ParseError::Incomplete);
````

- [ ] **Step 5: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 12 replacements, top to bottom:

Replace:

````rust
use crate::killed;
use crate::parser::{Command, Redirect};
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
````

with:

````rust
use crate::killed;
use crate::parser::Command;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
````

Replace:

````rust
pub(crate) trait Runner {
    /// Runs `name` with `args`, its standard output going to `redirect`
    /// if there is one.
    fn run(
````

with:

````rust
pub(crate) trait Runner {
    /// Runs `name` with `args`, its standard output going to `output` (a
    /// file, and whether it is appended to) if there is one.
    fn run(
````

Replace:

````rust
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran;
````

with:

````rust
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran;
````

Replace:

````rust
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let file = match redirect_to(&mut *parts.vfs, redirect) {
            Ok(file) => file,
````

with:

````rust
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran {
        let file = match redirect_to(&mut *parts.vfs, output) {
            Ok(file) => file,
````

Replace:

````rust
        let Some((name, args)) = last.words.split_first() else {
            return match redirect_to(&mut *vfs, last.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
````

with:

````rust
        let Some((name, args)) = last.words.split_first() else {
            return match redirect_to(&mut *vfs, last.output()) {
                Ok(_) => Ran::said(0, String::new()),
````

Replace:

````rust
        };
        self.run(parts, name, args, last.redirect.as_ref())
    }
````

with:

````rust
        };
        self.run(parts, name, args, last.output())
    }
````

Replace:

````rust
        args: &[String],
        redirect: Option<&Redirect>,
    ) -> Ran {
        let stdout = match redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {}: {e}\n", r.path)),
            },
````

with:

````rust
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran {
        let stdout = match output {
            Some((path, append)) => match self.programs.open_output(path.as_bytes(), append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {path}: {e}\n")),
            },
````

Replace:

````rust
        };
        let last_out = match &last.redirect {
            Some(r) => match self.programs.open_output(r.path.as_bytes(), r.append) {
                Ok(fd) => Some(fd),
                Err(e) => return (started, Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
            },
````

with:

````rust
        };
        let last_out = match last.output() {
            Some((path, append)) => match self.programs.open_output(path.as_bytes(), append) {
                Ok(fd) => Some(fd),
                Err(e) => return (started, Ran::said(1, format!("{NAME}: {path}: {e}\n"))),
            },
````

Replace:

````rust
    vfs: &mut dyn Vfs,
    redirect: Option<&Redirect>,
) -> Result<Option<(Node, u64)>, Ran> {
    match redirect {
        Some(r) => match open_redirect(vfs, r) {
            Ok(file) => Ok(Some(file)),
            Err(e) => Err(Ran::said(1, format!("{NAME}: {}: {e}\n", r.path))),
        },
````

with:

````rust
    vfs: &mut dyn Vfs,
    output: Option<(&String, bool)>,
) -> Result<Option<(Node, u64)>, Ran> {
    match output {
        Some((path, append)) => match open_redirect(vfs, path, append) {
            Ok(file) => Ok(Some(file)),
            Err(e) => Err(Ran::said(1, format!("{NAME}: {path}: {e}\n"))),
        },
````

Replace:

````rust
/// Opens a redirection target: created if missing, emptied for `>`,
/// written at its end for `>>`.
fn open_redirect(vfs: &mut dyn Vfs, r: &Redirect) -> Result<(Node, u64), Errno> {
    let path = r.path.as_bytes();
    let node = match vfs.lookup(path) {
````

with:

````rust
/// Opens a redirection target: created if missing, emptied for `>`,
/// written at its end for `>>` (`append`).
fn open_redirect(vfs: &mut dyn Vfs, path: &str, append: bool) -> Result<(Node, u64), Errno> {
    let path = path.as_bytes();
    let node = match vfs.lookup(path) {
````

Replace:

````rust
            }
            if !r.append {
                vfs.truncate(node, 0)?;
````

with:

````rust
            }
            if !append {
                vfs.truncate(node, 0)?;
````

Replace:

````rust
    };
    let offset = if r.append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
````

with:

````rust
    };
    let offset = if append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
        let cmd = pipeline.remove(0);
        if cmd.words.is_empty() && cmd.redirect.is_none() {
            // Its words expanded to nothing: bash's status 0.
````

with:

````rust
        let cmd = pipeline.remove(0);
        if cmd.words.is_empty() && cmd.redirects.is_empty() {
            // Its words expanded to nothing: bash's status 0.
````

Replace:

````rust
                    };
                    match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file, Some(control)),
````

with:

````rust
                    };
                    match runner::redirect_to(&mut *parts.vfs, cmd.output()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file, Some(control)),
````

Replace:

````rust
                None => {
                    let ran = self
                        .runner
                        .get()
                        .run(parts, name, args, cmd.redirect.as_ref());
                    self.console.take_back();
````

with:

````rust
                None => {
                    let ran = self.runner.get().run(parts, name, args, cmd.output());
                    self.console.take_back();
````

Replace:

````rust
            // A bare `> file` just creates or empties the file.
            None => match runner::redirect_to(&mut *parts.vfs, cmd.redirect.as_ref()) {
                Ok(_) => Ran::said(0, String::new()),
````

with:

````rust
            // A bare `> file` just creates or empties the file.
            None => match runner::redirect_to(&mut *parts.vfs, cmd.output()) {
                Ok(_) => Ran::said(0, String::new()),
````

Replace:

````rust
        }
        let redirect = match cmd.redirect.as_ref() {
            Some(r) => match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => Some(r),
                Err(e) => return self.not_expanded(e, true),
            },
            None => None,
        };
        match runner::redirect_to(&mut *self.vfs, redirect.as_ref()) {
            Ok(_) => self.finish(0, String::new()),
````

with:

````rust
        }
        let mut redirects = Vec::new();
        for r in &cmd.redirects {
            match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => redirects.push(r),
                Err(e) => return self.not_expanded(e, true),
            }
        }
        let output = parser::Command {
            words: Vec::new(),
            redirects,
        };
        match runner::redirect_to(&mut *self.vfs, output.output()) {
            Ok(_) => self.finish(0, String::new()),
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 384 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell): hold a command's redirections in a list

A command's one `> file` becomes a list of redirections, each an fd and
what it is made (`Redirect { fd, op }`), so that milestone 5 can add
several per command, other fds and the copies; a pipeline gains the list
a compound command will take. Only `>` and `>>` on fd 1, at most one per
command, are read as yet, and the runners take that file as before.
EOF
````


### Task 2: The walker carries an fd context

Decision 3: a new module, `crates/shell/src/fds.rs`, holds what a command's fds 0, 1 and 2 are (`Fds`, three `Slot`s: the shell's own fd, or a file a redirection opened) and the files open (`Files`, each counted by the fds of every context that holds it and closed when none does; `Opener` opens through `/bin/sh`'s `Programs` or the in-process runner's `Vfs`). The walker keeps the context where it stands (`Shell::fds`, the shell's own until Task 18) and makes each command's over it (`Shell::redirect`, `release`); the runners take a command's `Fds` instead of its output file, a pipeline's stages theirs (`runner::Stage`). Under `/bin/sh` a file is an fd of the shell's, and `Programs::spawn` takes the three fds a program gets (`[u32; 3]`, `relay_rt::sysio::command_fds`), so the shell's own fds 0 to 2 never change and it needs no `dup`; a built-in writes a redirection through that fd (`Programs::write`, `Ctx`'s `To::Fd` and `Output::Fd`), not through the `Vfs` by path, so that it shares the file's offset with the programs around it (Task 18). In the in-process runner a file is its node and offset (`Handle::Node`). Under the test double, a bare redirection and a built-in's now open their files through `Programs` too, so three control tests read what the shell wrote (`FakePrograms::written_to`) and the runner's test of a bare redirection counts its open and close; `Spawned` records the three fds. The red run is the shell's tests: the new module and `Programs::write` are missing, and the built-in's output does not go through the fd. Mutation checks (9): the built-in's fd, the program's fd, the close at a count of 0, the count of a context that holds a file, the close of a replaced file, the write through the fd and its error, the release after a command, and relay-rt's map of the three fds, each broken, fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/control.rs`
- Modify: `crates/shell/src/ctx.rs`
- Create: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/lib.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 1's `Command::redirects`; `io::Programs`; `ctx::Ctx::new`.
- Produces: `fds::{Fds, Slot::{Shell, File}, Handle::{Fd, Node}, Files, Opener, Failed}` (crate-private), `Files::{redirect, release, handle}`; `Programs::write(fd, bytes)`, `Programs::spawn(path, args, fds: [u32; 3], group)`; `ctx::To::{Console, File, Fd}`; `runner::Stage { words, fds }`; `Runner::run(parts, name, args, fds)`; `testing::FakePrograms::{written, write_error, written_to}`, `Spawned::fds`.

- [ ] **Step 1: Add the failing tests to `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
    #[test]
    fn a_command_gets_the_shell_s_fds_but_its_pipes_and_redirection() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds(None, None)), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds(None, Some(5))), [(0, 0), (1, 5), (2, 2)]);
        assert_eq!(
            pairs(command_fds(Some(4), Some(7))),
            [(0, 4), (1, 7), (2, 2)]
        );
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
````

with:

````rust
    #[test]
    fn a_command_gets_the_shell_s_fds_it_is_given() {
        let pairs = |fds: [FdMap; 3]| fds.map(|f| (f.child, f.parent));
        assert_eq!(pairs(command_fds([0, 1, 2])), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(pairs(command_fds([4, 7, 7])), [(0, 4), (1, 7), (2, 7)]);
        assert_eq!(arg_bytes(&[b"ls", b"", b"a b"]), b"ls\0\0a b\0");
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/control.rs`**

In `crates/shell/src/commands/control.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        );
        assert_eq!(h.get("/tmp/j"), b"[1]+  Running                 t-spin &\n");
    }
````

with:

````rust
        );
        assert_eq!(
            h.programs.written_to("/tmp/j"),
            b"[1]+  Running                 t-spin &\n"
        );
    }
````

Replace:

````rust
        );
        assert_eq!(h.get("/tmp/w"), b"", "never into the redirection");
    }
````

with:

````rust
        );
        assert_eq!(
            h.programs.written_to("/tmp/w"),
            b"",
            "never into the redirection"
        );
        assert_eq!(h.programs.opened.len(), 1);
    }
````

Replace:

````rust
        );
        assert_eq!(h.get("/tmp/w"), b"");
    }
````

with:

````rust
        );
        assert_eq!(h.programs.written_to("/tmp/w"), b"");
        assert_eq!(h.programs.opened.len(), 1);
    }
````

- [ ] **Step 3: Write the failing tests for `crates/shell/src/fds.rs`**

Create `crates/shell/src/fds.rs` containing only this test module (the implementation goes above it in a later step):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Harness;

    fn write(fd: u32, path: &str) -> Redirect {
        Redirect {
            fd,
            op: RedirectOp::Write(path.into()),
        }
    }

    #[test]
    fn a_context_holds_its_files_until_it_is_released() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: Some(&mut h.programs),
        };
        let mut files = Files::default();
        let outer = files
            .redirect(Fds::SHELL, &[write(1, "/tmp/a")], &mut opener)
            .unwrap();
        assert_eq!(outer.0, [Slot::Shell(0), Slot::File(0), Slot::Shell(2)]);
        // A command inside holds the same file, and its own.
        let inner = files.redirect(outer, &[], &mut opener).unwrap();
        assert_eq!(inner, outer);
        files.release(inner, &mut opener);
        assert_eq!(files.count(), 1, "the outer context still holds it");
        files.release(outer, &mut opener);
        assert_eq!(files.count(), 0);
        assert_eq!(h.programs.opened, [("/tmp/a".into(), false, 4)]);
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_file_a_later_redirection_replaces_is_closed_at_once() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: Some(&mut h.programs),
        };
        let mut files = Files::default();
        let fds = files
            .redirect(
                Fds::SHELL,
                &[write(1, "/tmp/a"), write(1, "/tmp/b")],
                &mut opener,
            )
            .unwrap();
        // `/tmp/a` (fd 4) is closed as soon as `/tmp/b` takes its place,
        // before the command runs.
        assert_eq!(fds.0[1], Slot::File(1));
        assert_eq!(files.handle(1), Handle::Fd(5));
        assert_eq!(files.count(), 1);
        files.release(fds, &mut opener);
        assert_eq!(h.programs.closed, [4, 5]);
    }

    #[test]
    fn a_failed_redirection_gives_the_context_as_it_stood() {
        let mut h = Harness::new();
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: None,
        };
        let mut files = Files::default();
        let failed = files
            .redirect(
                Fds::SHELL,
                &[write(1, "/tmp/a"), write(1, "/nodir/b")],
                &mut opener,
            )
            .unwrap_err();
        assert_eq!(
            (failed.path.as_str(), failed.error),
            ("/nodir/b", Errno::ENOENT)
        );
        assert_eq!(failed.fds.0[1], Slot::File(0));
        files.release(failed.fds, &mut opener);
        assert_eq!(files.count(), 0);
        // In the `Vfs` a file is made, emptied or appended to.
        h.put("/tmp/a", b"12345");
        let mut opener = Opener {
            vfs: &mut h.vfs,
            programs: None,
        };
        let append = Redirect {
            fd: 1,
            op: RedirectOp::Append("/tmp/a".into()),
        };
        let fds = files.redirect(Fds::SHELL, &[append], &mut opener).unwrap();
        let Handle::Node { offset, .. } = files.handle(0) else {
            unreachable!()
        };
        assert_eq!(offset, 5);
        files.release(fds, &mut opener);
        let fds = files
            .redirect(Fds::SHELL, &[write(1, "/tmp/a")], &mut opener)
            .unwrap();
        files.release(fds, &mut opener);
        assert_eq!(h.get("/tmp/a"), b"");
    }
}
````

- [ ] **Step 4: Declare the new module in `crates/shell/src/lib.rs`**

In `crates/shell/src/lib.rs`, replace:

````rust
mod expand;
mod io;
````

with:

````rust
mod expand;
mod fds;
mod io;
````

- [ ] **Step 5: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                stdin: None,
                stdout: None,
                group: Group::New,
````

with:

````rust
                args: words(&["t-args", "a", "b c", ""]),
                fds: [0, 1, 2],
                group: Group::New,
````

Replace:

````rust
        );
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.stdout).collect();
        assert_eq!(fds, [Some(4), Some(5)]);
        assert_eq!(h.programs.closed, [4, 5], "the shell keeps no copy");
````

with:

````rust
        );
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.fds).collect();
        assert_eq!(fds, [[0, 4, 2], [0, 5, 2]]);
        assert_eq!(h.programs.closed, [4, 5], "the shell keeps no copy");
````

Replace:

````rust
        assert!(h.programs.spawned.is_empty());
        // A bare redirection is the shell's alone.
        h.programs.open_error = None;
        assert_eq!(h.spawning("> /tmp/new"), (0, String::new()));
        assert!(h.exists("/tmp/new") && h.programs.opened.is_empty());
    }
````

with:

````rust
        assert!(h.programs.spawned.is_empty());
        // A bare redirection opens its file as any other, and closes it.
        h.programs.open_error = None;
        assert_eq!(h.spawning("> /tmp/new"), (0, String::new()));
        assert_eq!(h.programs.opened, [("/tmp/new".into(), false, 4)]);
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_built_in_writes_its_redirection_through_the_shell_s_fd() {
        // So it shares the file's offset with the programs around it
        // (programmable shell gate §7.4): not through the `Vfs`, by path.
        let mut h = with_programs();
        assert_eq!(h.spawning("help > /tmp/h").0, 0);
        assert_eq!(h.programs.opened, [("/tmp/h".into(), false, 4)]);
        assert!(
            h.programs
                .written_to("/tmp/h")
                .starts_with(b"Programs in /bin:\n  [ ")
        );
        assert_eq!(h.programs.closed, [4]);
        assert!(!h.exists("/tmp/h"));
        // A write that fails is the built-in's write error.
        h.programs.write_error = Some(Errno::ENOSPC);
        assert_eq!(
            h.spawning("help > /tmp/h"),
            (1, "help: write error: No space left on device\n".into())
        );
    }
````

Replace:

````rust
            args: words(args),
            stdin: fds.0,
            stdout: fds.1,
            group,
````

with:

````rust
            args: words(args),
            fds: [fds.0.unwrap_or(0), fds.1.unwrap_or(1), 2],
            group,
````

Replace:

````rust
        assert_eq!(
            h.programs.spawned[1].stdin,
            Some(6),
            "the second pipe's read end"
````

with:

````rust
        assert_eq!(
            h.programs.spawned[1].fds[0], 6,
            "the second pipe's read end"
````

- [ ] **Step 6: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(groups, [crate::Group::Background, crate::Group::Join(101)]);
        assert_eq!(h.programs.spawned[1].stdout, Some(4), "the redirection");
        let (r, w) = h.programs.pipes[0];
````

with:

````rust
        assert_eq!(groups, [crate::Group::Background, crate::Group::Join(101)]);
        assert_eq!(h.programs.spawned[1].fds[1], 4, "the redirection");
        let (r, w) = h.programs.pipes[0];
````

Replace:

````rust
        let (r, w) = h.programs.pipes[0];
        assert_eq!(s.stdin, Some(r), "the pipe it reads");
        assert!(h.programs.closed.contains(&w), "with no writer");
````

with:

````rust
        let (r, w) = h.programs.pipes[0];
        assert_eq!(s.fds[0], r, "the pipe it reads");
        assert!(h.programs.closed.contains(&w), "with no writer");
````

- [ ] **Step 7: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
    pub args: Vec<String>,
    /// What it got as fd 0 (a pipe's read end) and fd 1 (a pipe's write
    /// end or a redirection), instead of the shell's.
    pub stdin: Option<u32>,
    pub stdout: Option<u32>,
    pub group: Group,
````

with:

````rust
    pub args: Vec<String>,
    /// The shell's fds it got as its fds 0, 1 and 2.
    pub fds: [u32; 3],
    pub group: Group,
````

Replace:

````rust
    pub closed: Vec<u32>,
    pub spawned: Vec<Spawned>,
````

with:

````rust
    pub closed: Vec<u32>,
    /// Every write to an fd the shell opened: the fd and the bytes.
    pub written: Vec<(u32, Vec<u8>)>,
    /// What `write` fails with, if anything.
    pub write_error: Option<Errno>,
    pub spawned: Vec<Spawned>,
````

Replace:

````rust
            closed: Vec::new(),
            spawned: Vec::new(),
````

with:

````rust
            closed: Vec::new(),
            written: Vec::new(),
            write_error: None,
            spawned: Vec::new(),
````

Replace:

````rust
impl FakePrograms {
    /// The children not yet collected, by pid, with their groups.
````

with:

````rust
impl FakePrograms {
    /// What the shell wrote, through the fds it opened for `path`.
    pub fn written_to(&self, path: &str) -> Vec<u8> {
        let fds: Vec<u32> = self
            .opened
            .iter()
            .filter(|o| o.0 == path)
            .map(|o| o.2)
            .collect();
        self.written
            .iter()
            .filter(|w| fds.contains(&w.0))
            .flat_map(|w| w.1.iter().copied())
            .collect()
    }

    /// The children not yet collected, by pid, with their groups.
````

Replace:

````rust
        Ok(self.next_fd)
    }
````

with:

````rust
        Ok(self.next_fd)
    }
    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
        if let Some(e) = self.write_error {
            return Err(e);
        }
        self.written.push((fd, bytes.to_vec()));
        Ok(())
    }
````

Replace:

````rust
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
````

with:

````rust
        args: &[&[u8]],
        fds: [u32; 3],
        group: Group,
````

Replace:

````rust
                .collect(),
            stdin,
            stdout,
            group,
````

with:

````rust
                .collect(),
            fds,
            group,
````

- [ ] **Step 8: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find type `Redirect` in this scope ``; `` cannot find struct, variant or union type `Redirect` in this scope ``.

- [ ] **Step 9: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 4 replacements, top to bottom:

Replace:

````rust

/// A command's fds: `stdin` or the shell's 0, `stdout` or the shell's 1,
/// and the shell's 2.
pub fn command_fds(stdin: Option<u32>, stdout: Option<u32>) -> [FdMap; 3] {
    [(0, stdin.unwrap_or(0)), (1, stdout.unwrap_or(1)), (2, 2)]
        .map(|(child, parent)| FdMap { child, parent })
}
````

with:

````rust

/// A command's fds 0, 1 and 2: the shell's `fds`.
pub fn command_fds(fds: [u32; 3]) -> [FdMap; 3] {
    [0, 1, 2].map(|child| FdMap {
        child,
        parent: fds[child as usize],
    })
}
````

Replace:

````rust
        sys::open(path, output_flags(append)).map_err(Errno::from_number)
    }
````

with:

````rust
        sys::open(path, output_flags(append)).map_err(Errno::from_number)
    }

    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
        let mut done = 0;
        while done < bytes.len() {
            match sys::write(fd, &bytes[done..]) {
                Ok(0) => return Err(Errno::ENOSPC),
                Ok(n) => done += n,
                Err(e) => return Err(Errno::from_number(e)),
            }
        }
        Ok(())
    }
````

Replace:

````rust
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
````

with:

````rust
        args: &[&[u8]],
        fds: [u32; 3],
        group: Group,
````

Replace:

````rust
        }
        let fds = command_fds(stdin, stdout);
        let pid = sys::spawn(path, &arg_bytes(args), b"", &fds, flags, pgid)
````

with:

````rust
        }
        let fds = command_fds(fds);
        let pid = sys::spawn(path, &arg_bytes(args), b"", &fds, flags, pgid)
````

- [ ] **Step 10: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 13 replacements, top to bottom:

Replace:

````rust
//! input (none, a program's fd 0, or bytes in memory), standard output
//! (the screen, a redirection file, or a program's fd 1) and the screen
//! for errors; plus the helpers every command shares for options and
//! GNU-style messages.

````

with:

````rust
//! input (none, a program's fd 0, or bytes in memory), standard output
//! (the screen, a redirection file, `/bin/sh`'s fd for one, or a program's
//! fd 1) and the screen for errors; plus the helpers every command shares
//! for options and GNU-style messages.

````

Replace:

````rust

enum Output<'a> {
    Console,
    File {
````

with:

````rust

/// Where a command the shell runs itself writes (programmable shell gate
/// §7.5): the screen, a file of the in-process runner's at an offset, or
/// an fd of `/bin/sh`'s, which its `Programs` write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum To {
    Console,
    File(Node, u64),
    Fd(u32),
}

enum Output<'a> {
    Console,
    /// An fd of `/bin/sh`'s, written through its `Programs` in pieces of
    /// 4 KiB, so that a built-in shares the file's offset with programs.
    Fd {
        fd: u32,
        buf: Vec<u8>,
        error: Option<Errno>,
    },
    File {
````

Replace:

````rust
impl<'a> Ctx<'a> {
    /// `file`: the redirection target and the offset to write at.
    pub(crate) fn new(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        file: Option<(Node, u64)>,
    ) -> Ctx<'a> {
        let out = match file {
            Some((node, offset)) => Output::File {
                node,
````

with:

````rust
impl<'a> Ctx<'a> {
    /// Standard output goes `to` (an fd through `control`'s programs).
    pub(crate) fn new(
        vfs: &'a mut dyn Vfs,
        system: &'a mut dyn System,
        console: &'a mut dyn Console,
        to: To,
    ) -> Ctx<'a> {
        let out = match to {
            To::Console => Output::Console,
            To::File(node, offset) => Output::File {
                node,
````

Replace:

````rust
            },
            None => Output::Console,
        };
````

with:

````rust
            },
            To::Fd(fd) => Output::Fd {
                fd,
                buf: Vec::new(),
                error: None,
            },
        };
````

Replace:

````rust
            transcript: &mut self.transcript,
        }
````

with:

````rust
            transcript: &mut self.transcript,
            programs: self
                .control
                .as_mut()
                .and_then(|c| c.programs.as_deref_mut()),
        }
````

Replace:

````rust
            Output::Console => true,
            Output::File { .. } => false,
            Output::Program { tty, .. } => tty,
````

with:

````rust
            Output::Console => true,
            Output::File { .. } | Output::Fd { .. } => false,
            Output::Program { tty, .. } => tty,
````

Replace:

````rust
            self.out,
            Output::File { error: Some(_), .. } | Output::Program { error: Some(_), .. }
        )
````

with:

````rust
            self.out,
            Output::File { error: Some(_), .. }
                | Output::Fd { error: Some(_), .. }
                | Output::Program { error: Some(_), .. }
        )
````

Replace:

````rust
            Output::Program { stdout, .. } => stdout.node(),
            Output::Console => None,
        }
````

with:

````rust
            Output::Program { stdout, .. } => stdout.node(),
            Output::Console | Output::Fd { .. } => None,
        }
````

Replace:

````rust
        match self.out {
            Output::File { error: Some(e), .. } | Output::Program { error: Some(e), .. } => Err(e),
            _ => Ok(()),
````

with:

````rust
        match self.out {
            Output::File { error: Some(e), .. }
            | Output::Fd { error: Some(e), .. }
            | Output::Program { error: Some(e), .. } => Err(e),
            _ => Ok(()),
````

Replace:

````rust
    transcript: &'s mut Option<Transcript>,
}
````

with:

````rust
    transcript: &'s mut Option<Transcript>,
    /// `/bin/sh`'s, for `Output::Fd`.
    programs: Option<&'s mut (dyn Programs + 'a)>,
}
````

Replace:

````rust
            }
            Output::File { buf, error, .. } | Output::Program { buf, error, .. } => {
                if let Some(e) = error {
````

with:

````rust
            }
            Output::File { buf, error, .. }
            | Output::Fd { buf, error, .. }
            | Output::Program { buf, error, .. } => {
                if let Some(e) = error {
````

Replace:

````rust
        match &*self.out {
            Output::File { error: Some(e), .. } | Output::Program { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
````

with:

````rust
        match &*self.out {
            Output::File { error: Some(e), .. }
            | Output::Fd { error: Some(e), .. }
            | Output::Program { error: Some(e), .. } => Err(*e),
            _ => Ok(()),
````

Replace:

````rust
                {
                    *error = Some(e);
                }
                buf.clear();
````

with:

````rust
                {
                    *error = Some(e);
                }
                buf.clear();
            }
            Output::Fd { fd, buf, error } => {
                if error.is_none() && !buf.is_empty() {
                    let written = match self.programs.as_deref_mut() {
                        Some(programs) => programs.write(*fd, buf),
                        None => Err(Errno::EBADF),
                    };
                    if let Err(e) = written {
                        *error = Some(e);
                    }
                }
                buf.clear();
````

- [ ] **Step 11: Implement `crates/shell/src/fds.rs`**

Insert this at the top of `crates/shell/src/fds.rs`, above `#[cfg(test)]`:

````rust
//! The fd context (programmable shell gate §7.2, §7.4): what a command's
//! fds 0, 1 and 2 are where the walker stands, and the files its
//! redirections opened. A context is made from the one around it and the
//! command's redirections, left to right; a file is counted by the fds of
//! every context that holds it and closed once none does, so a file a later
//! redirection replaces is closed at once. Under `/bin/sh` a file is an fd
//! of the shell's, which a program gets as one of its fds and a built-in
//! writes through, so both share its offset; the shell's own fds 0 to 2
//! never change. In the in-process runner it is the file's node and an
//! offset every fd of it shares.

use crate::io::Programs;
use crate::parser::{Redirect, RedirectOp};
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs};

/// What one of a command's fds is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The shell's own fd of that number.
    Shell(u32),
    /// A file a redirection opened, by its place in [`Files`].
    File(usize),
}

/// A command's fds 0, 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fds(pub [Slot; 3]);

impl Fds {
    /// The shell's own, where nothing is redirected.
    pub const SHELL: Fds = Fds([Slot::Shell(0), Slot::Shell(1), Slot::Shell(2)]);
}

/// A file a redirection opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    /// `/bin/sh`'s fd.
    Fd(u32),
    /// The in-process runner's: the file, and where the next write goes.
    Node { node: Node, offset: u64 },
}

/// The files open for redirections, each with how many fds hold it.
#[derive(Default)]
pub(crate) struct Files {
    open: Vec<Option<(Handle, usize)>>,
}

/// How files are opened and closed: through `/bin/sh`'s calls when it has
/// `programs`, or else in the `Vfs`.
pub(crate) struct Opener<'x> {
    pub vfs: &'x mut dyn Vfs,
    pub programs: Option<&'x mut dyn Programs>,
}

impl Opener<'_> {
    fn open(&mut self, op: &RedirectOp) -> Result<Handle, Errno> {
        let (path, append) = match op {
            RedirectOp::Write(path) => (path, false),
            RedirectOp::Append(path) => (path, true),
        };
        if let Some(programs) = self.programs.as_deref_mut() {
            return programs
                .open_output(path.as_bytes(), append)
                .map(Handle::Fd);
        }
        let (node, offset) = open_output(&mut *self.vfs, path, append)?;
        Ok(Handle::Node { node, offset })
    }

    fn close(&mut self, handle: Handle) {
        if let (Handle::Fd(fd), Some(programs)) = (handle, self.programs.as_deref_mut()) {
            programs.close(fd);
        }
    }
}

/// Opens a file for output in `vfs`: created if missing, emptied, or with
/// `append` written at its end; the node and the offset to write at.
fn open_output(vfs: &mut dyn Vfs, path: &str, append: bool) -> Result<(Node, u64), Errno> {
    let path = path.as_bytes();
    let node = match vfs.lookup(path) {
        Ok(node) => {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            if !append {
                vfs.truncate(node, 0)?;
            }
            node
        }
        Err(Errno::ENOENT) => vfs.create(path)?,
        Err(e) => return Err(e),
    };
    let offset = if append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
}

/// A redirection that could not be made: the context as it stood, on
/// whose fd 2 it is told and which the caller releases, and the file and
/// why.
#[derive(Debug)]
pub(crate) struct Failed {
    pub fds: Fds,
    pub path: String,
    pub error: Errno,
}

impl Files {
    /// The file at `i`.
    pub fn handle(&self, i: usize) -> Handle {
        match self.open.get(i) {
            Some(Some((handle, _))) => *handle,
            _ => unreachable!("a slot names an open file"),
        }
    }

    /// The context made from `base` and `redirects`, left to right
    /// (programmable shell gate §7.2), which holds its files until it is
    /// released; a file a later redirection replaces is closed at once.
    pub fn redirect(
        &mut self,
        base: Fds,
        redirects: &[Redirect],
        opener: &mut Opener<'_>,
    ) -> Result<Fds, Failed> {
        let mut fds = base;
        self.hold(&fds);
        for r in redirects {
            let slot = match opener.open(&r.op) {
                Ok(handle) => Slot::File(self.add(handle)),
                Err(error) => {
                    let path = match &r.op {
                        RedirectOp::Write(path) | RedirectOp::Append(path) => path.clone(),
                    };
                    return Err(Failed { fds, path, error });
                }
            };
            let fd = r.fd as usize;
            let replaced = core::mem::replace(&mut fds.0[fd], slot);
            self.drop_slot(replaced, opener);
        }
        Ok(fds)
    }

    /// A context made with [`Files::redirect`] ends: the files it held
    /// last are closed.
    pub fn release(&mut self, fds: Fds, opener: &mut Opener<'_>) {
        for slot in fds.0 {
            self.drop_slot(slot, opener);
        }
    }

    /// One more fd holds each of `fds`' files.
    fn hold(&mut self, fds: &Fds) {
        for slot in fds.0 {
            if let Slot::File(i) = slot
                && let Some(Some((_, count))) = self.open.get_mut(i)
            {
                *count += 1;
            }
        }
    }

    /// An fd that held `slot` holds it no more.
    fn drop_slot(&mut self, slot: Slot, opener: &mut Opener<'_>) {
        let Slot::File(i) = slot else {
            return;
        };
        let Some(entry) = self.open.get_mut(i) else {
            return;
        };
        if let Some((handle, count)) = entry {
            *count -= 1;
            if *count == 0 {
                let handle = *handle;
                *entry = None;
                opener.close(handle);
            }
        }
    }

    /// A file just opened, held by one fd: its place.
    fn add(&mut self, handle: Handle) -> usize {
        let entry = Some((handle, 1));
        match self.open.iter().position(Option::is_none) {
            Some(i) => {
                self.open[i] = entry;
                i
            }
            None => {
                self.open.push(entry);
                self.open.len() - 1
            }
        }
    }

    /// How many files are open.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.open.iter().flatten().count()
    }
}

````

- [ ] **Step 12: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    fn close(&mut self, fd: u32);
    /// Makes a pipe: its read end and its write end.
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) in
    /// `group`; its pid. It gets `stdin` (or the shell's fd 0) as its fd 0,
    /// `stdout` (or the shell's fd 1) as its fd 1, and the shell's fd 2.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
````

with:

````rust
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    /// Writes all of `bytes` to the shell's fd `fd`, a file it opened (a
    /// built-in's redirected output); the error that stopped it (`ENOSPC`
    /// for a write that took nothing).
    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno>;
    fn close(&mut self, fd: u32);
    /// Makes a pipe: its read end and its write end.
    fn pipe(&mut self) -> Result<(u32, u32), Errno>;
    /// Starts the program at `path` with `args` (argument 0 first) in
    /// `group`; its pid. Its fds 0, 1 and 2 are the shell's `fds`.
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        fds: [u32; 3],
        group: Group,
````

- [ ] **Step 13: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 17 replacements, top to bottom:

Replace:

````rust
//! input; the spawning runner starts `/bin/<name>` for every one
//! (`/bin/sh`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::{Ctx, JobControl};
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
use crate::killed;
use crate::parser::Command;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
````

with:

````rust
//! input; the spawning runner starts `/bin/<name>` for every one
//! (`/bin/sh`). A command's fds are the ones the walker's context and its
//! redirections give it (programmable shell gate §7.2, `crate::fds`).

use crate::commands::{self, Builtin, Script};
use crate::ctx::{Ctx, JobControl, To};
use crate::fds::{Fds, Files, Handle, Slot};
use crate::io::{Bytes, Console, Group, Programs, Stdin, Stdout, System};
use crate::killed;
use crate::shell::{CANCELLED, CANNOT_RUN, NAME, NOT_FOUND, SYNTAX};
````

Replace:

````rust
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, Vfs};

````

with:

````rust
use alloc::vec::Vec;
use vfs::{Errno, Node, Vfs};

````

Replace:

````rust
    pub input: Option<&'s mut dyn Stdin>,
}
````

with:

````rust
    pub input: Option<&'s mut dyn Stdin>,
    /// The files the fds name.
    pub files: &'s mut Files,
}
````

Replace:

````rust

/// Runs the commands that are not the shell's own (`commands::BUILTINS`).
pub(crate) trait Runner {
    /// Runs `name` with `args`, its standard output going to `output` (a
    /// file, and whether it is appended to) if there is one.
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran;

    /// Runs a pipeline of two or more commands, none of them a built-in,
    /// the last one's output going to its redirection if it has one.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran;

    /// Starts a pipeline of one or more commands, none of them a built-in,
    /// in the background (spec §9.2), and does not wait for it.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started;
}
````

with:

````rust

/// A command of a pipeline: its words, and the fds its context and
/// redirections give it, the pipes aside.
pub(crate) struct Stage<'c> {
    pub words: &'c [String],
    pub fds: Fds,
}

/// Runs the commands that are not the shell's own (`commands::BUILTINS`).
pub(crate) trait Runner {
    /// Runs `name` with `args` on `fds`.
    fn run(&mut self, parts: Parts<'_>, name: &str, args: &[String], fds: Fds) -> Ran;

    /// Runs a pipeline of two or more commands, none of them a built-in.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Ran;

    /// Starts a pipeline of one or more commands, none of them a built-in,
    /// in the background (spec §9.2), and does not wait for it.
    fn background(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Started;
}
````

Replace:

````rust
impl Runner for InProcess {
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran {
        let file = match redirect_to(&mut *parts.vfs, output) {
            Ok(file) => file,
            Err(ran) => return ran,
        };
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, file, None),
            None => not_found(name),
````

with:

````rust
impl Runner for InProcess {
    fn run(&mut self, parts: Parts<'_>, name: &str, args: &[String], fds: Fds) -> Ran {
        match commands::find(name) {
            Some(command) => run_function(parts, command, args, fds, None),
            None => not_found(name),
````

Replace:

````rust
    /// first says so). Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
````

with:

````rust
    /// first says so). Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Ran {
        // `sh` reads a script for this shell to run after the command.
````

Replace:

````rust
            mut input,
        } = parts;
````

with:

````rust
            mut input,
            files,
        } = parts;
````

Replace:

````rust
        let Some((name, args)) = last.words.split_first() else {
            return match redirect_to(&mut *vfs, last.output()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            };
        };
````

with:

````rust
        let Some((name, args)) = last.words.split_first() else {
            return Ran::said(0, String::new());
        };
````

Replace:

````rust
            input: Some(&mut piped),
        };
        self.run(parts, name, args, last.output())
    }

    fn background(&mut self, _: Parts<'_>, _: &[Command]) -> Started {
        InProcess::refuse_background()
````

with:

````rust
            input: Some(&mut piped),
            files,
        };
        self.run(parts, name, args, last.fds)
    }

    fn background(&mut self, _: Parts<'_>, _: &[Stage<'_>]) -> Started {
        InProcess::refuse_background()
````

Replace:

````rust

impl Runner for Spawning<'_> {
    /// Redirections are opened in the shell and passed to the program as
    /// its fd 1, which the shell closes once the program has it. At the
    /// prompt a program gets a process group of its own and the console;
    /// in a script it runs in the shell's group, so that Ctrl-C ends the
    /// script with it (spec §6.4).
    fn run(
        &mut self,
        parts: Parts<'_>,
        name: &str,
        args: &[String],
        output: Option<(&String, bool)>,
    ) -> Ran {
        let stdout = match output {
            Some((path, append)) => match self.programs.open_output(path.as_bytes(), append) {
                Ok(fd) => Some(fd),
                Err(e) => return Ran::said(1, format!("{NAME}: {path}: {e}\n")),
            },
            None => None,
        };
        let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
````

with:

````rust

/// The shell's fd that `slot` is: its own, or a file it opened.
fn shell_fd(files: &Files, slot: Slot) -> u32 {
    match slot {
        Slot::Shell(fd) => fd,
        Slot::File(i) => match files.handle(i) {
            Handle::Fd(fd) => fd,
            Handle::Node { .. } => unreachable!("/bin/sh opens its files as fds"),
        },
    }
}

impl Runner for Spawning<'_> {
    /// The program gets the fds the shell has for it. At the prompt it gets
    /// a process group of its own and the console; in a script it runs in
    /// the shell's group, so that Ctrl-C ends the script with it (spec
    /// §6.4).
    fn run(&mut self, parts: Parts<'_>, name: &str, args: &[String], fds: Fds) -> Ran {
        let mut argv: Vec<&[u8]> = alloc::vec![name.as_bytes()];
````

Replace:

````rust
        };
        let started = self
            .programs
            .spawn(path.as_bytes(), &argv, None, stdout, group);
        if let Some(fd) = stdout {
            self.programs.close(fd);
        }
        match started {
            Ok(pid) => match self.programs.wait(pid) {
````

with:

````rust
        };
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot));
        match self.programs.spawn(path.as_bytes(), &argv, fds, group) {
            Ok(pid) => match self.programs.wait(pid) {
````

Replace:

````rust
    /// status.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Command]) -> Ran {
        let in_script = parts.in_script;
````

with:

````rust
    /// status.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Ran {
        let in_script = parts.in_script;
````

Replace:

````rust
    /// others joining it; nothing waits for them.
    fn background(&mut self, parts: Parts<'_>, stages: &[Command]) -> Started {
        let (started, mut ran) = self.start(parts, stages, false, true);
````

with:

````rust
    /// others joining it; nothing waits for them.
    fn background(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Started {
        let (started, mut ran) = self.start(parts, stages, false, true);
````

Replace:

````rust
        parts: Parts<'_>,
        stages: &'c [Command],
        in_script: bool,
        background: bool,
    ) -> (Stages<'c>, Ran) {
        let (last, _) = stages.split_last().expect("a pipeline has stages");
        let mut started = Stages {
            pids: Vec::new(),
            last: None,
        };
        let last_out = match last.output() {
            Some((path, append)) => match self.programs.open_output(path.as_bytes(), append) {
                Ok(fd) => Some(fd),
                Err(e) => return (started, Ran::said(1, format!("{NAME}: {path}: {e}\n"))),
            },
            None => None,
        };
````

with:

````rust
        parts: Parts<'_>,
        stages: &'c [Stage<'c>],
        in_script: bool,
        background: bool,
    ) -> (Stages<'c>, Ran) {
        let mut started = Stages {
            pids: Vec::new(),
            last: None,
        };
````

Replace:

````rust
            let (next, stdout) = if is_last {
                (None, last_out)
            } else {
                match self.programs.pipe() {
                    Ok((read, write)) => (Some(read), Some(write)),
                    Err(e) => {
                        for fd in [stdin, last_out].into_iter().flatten() {
                            self.programs.close(fd);
````

with:

````rust
            let (next, stdout) = if is_last {
                (None, None)
            } else {
                match self.programs.pipe() {
                    Ok((read, write)) => (Some(read), Some(write)),
                    Err(e) => {
                        if let Some(fd) = stdin {
                            self.programs.close(fd);
````

Replace:

````rust
            let path = program_path(name);
            let pid = self
                .programs
                .spawn(path.as_bytes(), &argv, stdin, stdout, group);
            for fd in [stdin, stdout].into_iter().flatten() {
````

with:

````rust
            let path = program_path(name);
            let mut fds = stage.fds.0.map(|slot| shell_fd(parts.files, slot));
            if let Some(fd) = stdin {
                fds[0] = fd;
            }
            if let Some(fd) = stdout {
                fds[1] = fd;
            }
            let pid = self.programs.spawn(path.as_bytes(), &argv, fds, group);
            for fd in [stdin, stdout].into_iter().flatten() {
````

Replace:

````rust

/// The redirection target, opened; a target that cannot be opened stops
/// the command.
pub(crate) fn redirect_to(
    vfs: &mut dyn Vfs,
    output: Option<(&String, bool)>,
) -> Result<Option<(Node, u64)>, Ran> {
    match output {
        Some((path, append)) => match open_redirect(vfs, path, append) {
            Ok(file) => Ok(Some(file)),
            Err(e) => Err(Ran::said(1, format!("{NAME}: {path}: {e}\n"))),
        },
        None => Ok(None),
    }
}

/// Opens a redirection target: created if missing, emptied for `>`,
/// written at its end for `>>` (`append`).
fn open_redirect(vfs: &mut dyn Vfs, path: &str, append: bool) -> Result<(Node, u64), Errno> {
    let path = path.as_bytes();
    let node = match vfs.lookup(path) {
        Ok(node) => {
            if vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            if !append {
                vfs.truncate(node, 0)?;
            }
            node
        }
        Err(Errno::ENOENT) => vfs.create(path)?,
        Err(e) => return Err(e),
    };
    let offset = if append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
}

/// Runs a command function with its standard output going to `file`, if
/// any.
pub(crate) fn run_function<'s>(
    parts: Parts<'s>,
    command: &Builtin,
    args: &[String],
    file: Option<(Node, u64)>,
    control: Option<JobControl<'s>>,
) -> Ran {
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, file);
    ctx.control = control;
````

with:

````rust

/// Runs a command function on `fds`: its output to the screen, a file or
/// (under `/bin/sh`, through `control`'s programs) an fd.
pub(crate) fn run_function<'s>(
    parts: Parts<'s>,
    command: &Builtin,
    args: &[String],
    fds: Fds,
    control: Option<JobControl<'s>>,
) -> Ran {
    let to = match fds.0[1] {
        Slot::Shell(_) => To::Console,
        Slot::File(i) => match parts.files.handle(i) {
            Handle::Fd(fd) => To::Fd(fd),
            Handle::Node { node, offset } => To::File(node, offset),
        },
    };
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, to);
    ctx.control = control;
````

- [ ] **Step 14: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 14 replacements, top to bottom:

Replace:

````rust
use crate::expand::{self, Vars};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::expand::{self, Vars};
use crate::fds::{Fds, Files, Opener};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
    cancelled: bool,
}
````

with:

````rust
    cancelled: bool,
    /// What fds 0, 1 and 2 are where the walker stands (programmable shell
    /// gate §7.4).
    fds: Fds,
    /// The files redirections have open.
    files: Files,
}
````

Replace:

````rust
            cancelled: false,
        }
````

with:

````rust
            cancelled: false,
            fds: Fds::SHELL,
            files: Files::default(),
        }
````

Replace:

````rust
            return self.finish(0, String::new());
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
````

with:

````rust
            return self.finish(0, String::new());
        }
        let fds = match self.redirect(self.fds, &cmd.redirects) {
            Ok(fds) => fds,
            Err(message) => return self.finish(1, message),
        };
        let parts = Parts {
            vfs: &mut *self.vfs,
````

Replace:

````rust
                None => None,
            },
        };
        let ran = match cmd.words.split_first() {
````

with:

````rust
                None => None,
            },
            files: &mut self.files,
        };
        let ran = match cmd.words.split_first() {
````

Replace:

````rust
                    };
                    match runner::redirect_to(&mut *parts.vfs, cmd.output()) {
                        Ok(file) => runner::run_function(parts, builtin, args, file, Some(control)),
                        Err(ran) => ran,
                    }
                }
                None => {
                    let ran = self.runner.get().run(parts, name, args, cmd.output());
                    self.console.take_back();
````

with:

````rust
                    };
                    runner::run_function(parts, builtin, args, fds, Some(control))
                }
                None => {
                    let ran = self.runner.get().run(parts, name, args, fds);
                    self.console.take_back();
````

Replace:

````rust
            // A bare `> file` just creates or empties the file.
            None => match runner::redirect_to(&mut *parts.vfs, cmd.output()) {
                Ok(_) => Ran::said(0, String::new()),
                Err(ran) => ran,
            },
        };
        self.stopped = ran.stop;
````

with:

````rust
            // A bare `> file` just creates or empties the file.
            None => Ran::said(0, String::new()),
        };
        self.release(fds);
        self.stopped = ran.stop;
````

Replace:

````rust
        }
        let output = parser::Command {
            words: Vec::new(),
            redirects,
        };
        match runner::redirect_to(&mut *self.vfs, output.output()) {
            Ok(_) => self.finish(0, String::new()),
            Err(ran) => self.finish(ran.status, ran.message),
        }
````

with:

````rust
        }
        match self.redirect(self.fds, &redirects) {
            Ok(fds) => {
                self.release(fds);
                self.finish(0, String::new())
            }
            Err(message) => self.finish(1, message),
        }
````

Replace:

````rust
            return self.finish(ran.status, ran.message);
        }
        let parts = Parts {
            vfs: &mut *self.vfs,
````

with:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = match self.stage_fds(stages) {
            Ok(all) => all,
            Err(message) => return self.finish(1, message),
        };
        let staged = runner_stages(stages, &all);
        let parts = Parts {
            vfs: &mut *self.vfs,
````

Replace:

````rust
            },
        };
        let ran = self.runner.get().pipeline(parts, stages);
        self.console.take_back();
````

with:

````rust
            },
            files: &mut self.files,
        };
        let ran = self.runner.get().pipeline(parts, &staged);
        for fds in all {
            self.release(fds);
        }
        self.console.take_back();
````

Replace:

````rust
        }
        let parts = Parts {
````

with:

````rust
        }
        let all = match self.stage_fds(stages) {
            Ok(all) => all,
            Err(message) => return self.finish(1, message),
        };
        let staged = runner_stages(stages, &all);
        let parts = Parts {
````

Replace:

````rust
            input: None,
        };
        let started = self.runner.get().background(parts, stages);
        if let Some(pgid) = started.pgid {
````

with:

````rust
            input: None,
            files: &mut self.files,
        };
        let started = self.runner.get().background(parts, &staged);
        for fds in all {
            self.release(fds);
        }
        if let Some(pgid) = started.pgid {
````

Replace:

````rust
        self.finish(started.ran.status, started.ran.message)
    }
````

with:

````rust
        self.finish(started.ran.status, started.ran.message)
    }

    /// The context `redirects` make over `base` (programmable shell gate
    /// §7.2), or what keeps one from being made.
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Result<Fds, String> {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        match self.files.redirect(base, redirects, &mut opener) {
            Ok(fds) => Ok(fds),
            Err(failed) => {
                self.files.release(failed.fds, &mut opener);
                Err(format!("{NAME}: {}: {}\n", failed.path, failed.error))
            }
        }
    }

    /// A context made by `redirect` ends: the files it held last are
    /// closed.
    fn release(&mut self, fds: Fds) {
        let mut opener = Opener {
            vfs: &mut *self.vfs,
            programs: self.runner.programs(),
        };
        self.files.release(fds, &mut opener);
    }

    /// Each stage's fds, its redirections made over the context; none if
    /// one cannot be made.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Result<Vec<Fds>, String> {
        let mut all = Vec::new();
        for stage in stages {
            match self.redirect(self.fds, &stage.redirects) {
                Ok(fds) => all.push(fds),
                Err(message) => {
                    for fds in all {
                        self.release(fds);
                    }
                    return Err(message);
                }
            }
        }
        Ok(all)
    }
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

/// A pipeline's stages for the runner: each one's words and fds.
fn runner_stages<'c>(stages: &'c [parser::Command], fds: &[Fds]) -> Vec<runner::Stage<'c>> {
    stages
        .iter()
        .zip(fds)
        .map(|(c, fds)| runner::Stage {
            words: &c.words,
            fds: *fds,
        })
        .collect()
}

````

- [ ] **Step 15: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 388 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 16: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 17: Commit**

````bash
git add crates
git commit -F - <<'EOF'
refactor(shell,relay-rt): carry an fd context through the walker

The walker keeps what fds 0, 1 and 2 are where it stands (`fds::Fds`)
and the files redirections opened (`fds::Files`), each counted by the
fds that hold it and closed when none does. A command's redirections
are made over that context; `/bin/sh` opens its files as its own fds
and hands a program all three at `spawn`, and a built-in writes a
redirection through the same fd (`Programs::write`, `Ctx`'s `To::Fd`),
so that it will share the file's offset with programs. The shell's own
fds 0 to 2 never change.
EOF
````


### Task 3: Several redirections per command, left to right

Decisions 2 and 3 (spec §7.2): a command may have any number of redirections, made left to right as bash makes them: `echo a > f > g` empties `f` and writes `g`, and `/bin/sh` closes `f` as soon as `g` takes its place; one that cannot be made stops the command after those before it (`tmp/m5p1/probes/p1.txt`). The parser's refusal of a second `>` goes. The red run is the shell's tests. Mutation checks (2): the redirections made right to left, and a failure skipped, each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 2's `Files::redirect`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
        assert_eq!(one("echo > > f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(
            one("echo > a > b"),
            Err(ParseError::Unsupported(">".into()))
        );
        // Other streams are not supported; a quoted or spaced digit is a word.
````

with:

````rust
        assert_eq!(one("echo > > f"), Err(ParseError::MissingTarget(">")));
        // Several are made left to right (programmable shell gate §7.2).
        let c = one("echo > a >> b x").unwrap();
        assert_eq!(c.words, ["echo", "x"]);
        assert_eq!(
            c.redirects,
            [
                Redirect {
                    fd: 1,
                    op: RedirectOp::Write("a".into())
                },
                Redirect {
                    fd: 1,
                    op: RedirectOp::Append("b".into())
                }
            ]
        );
        assert_eq!(c.output(), Some((&"b".into(), true)));
        // Other streams are not supported; a quoted or spaced digit is a word.
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn errors_go_to_the_screen_not_into_the_file() {
````

with:

````rust
    #[test]
    fn several_redirections_are_made_left_to_right() {
        // bash 5.2: each file is made, and the command writes to the last
        // (tmp/m5p1/probes/p1.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"old\n");
        assert_eq!(h.run("echo a > /tmp/f > /tmp/g"), (0, String::new()));
        assert_eq!(
            (h.get("/tmp/f"), h.get("/tmp/g")),
            (b"".to_vec(), b"a\n".to_vec())
        );
        assert_eq!(h.run("echo b >> /tmp/g > /tmp/f"), (0, String::new()));
        assert_eq!(
            (h.get("/tmp/f"), h.get("/tmp/g")),
            (b"b\n".to_vec(), b"a\n".to_vec())
        );
        // One that cannot be made stops the command, after those before.
        assert_eq!(
            h.run("echo c > /tmp/h > /nodir/x > /tmp/i"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert!(h.exists("/tmp/h") && !h.exists("/tmp/i"));
        // Under /bin/sh each file a later one replaces is closed at once,
        // and the program gets the last.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/a > /tmp/b").0, 3);
        assert_eq!(
            h.programs.closed[0], 4,
            "the first, which the second replaced"
        );
        assert_eq!(h.programs.spawned[0].fds, [0, 5, 2]);
        assert_eq!(h.programs.closed, [4, 5]);
    }

    #[test]
    fn errors_go_to_the_screen_not_into_the_file() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `parser::tests::redirection_errors`, `shell::tests::several_redirections_are_made_left_to_right`.

- [ ] **Step 4: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
//!
//! `> file` and `>> file` redirect standard output (at most one per
//! command). An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

with:

````rust
//!
//! `> file` and `>> file` redirect standard output, any number of them per
//! command, made left to right. An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

Replace:

````rust
        match self.pending.take() {
            Some(_) if !self.redirects.is_empty() => {
                return Err(ParseError::Unsupported(">".into()));
            }
            Some(append) => {
````

with:

````rust
        match self.pending.take() {
            Some(append) => {
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 389 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): make several redirections of a command, left to right

A command may have any number of redirections, made left to right as
bash makes them (programmable shell gate §7.2): `echo a > f > g` empties
`f` and writes `g`, and `/bin/sh` closes `f` as soon as `g` takes its
place. One that cannot be made stops the command after those before it.
EOF
````


### Task 4: `2>` and `2>>`, and messages on a command's fd 2

Decisions 2 and 4 (spec §7.1, §7.5): a word of digits just before `>` or `>>` is the redirection's fd, as in bash; 1 and 2 are taken, any other is refused (`unsupported syntax: 3>`, `02>`, `0>`, `10>`), and after an operator that awaits its file name it is bash's syntax error naming it (`>2>f`, `2>2>f`, `tmp/m5p1/probes/p8.txt`). `Ctx` gains an error target (`Ctx::new`'s `err`, `Ctx::err_is_tty` comes later), so a built-in's errors go to its fd 2: `cd /nope 2> e` writes into `e`. A command's own messages (not found, cannot start, a write error: `Ran::own`) go to its fd 2 once its redirections are made, and a redirection that cannot be made is told on fd 2 as it stands then (`Shell::say_on`): `echo a 2> e > /nodir/x` writes the message into `e`, `echo a > /nodir/x 2> e` on the screen and makes no `e` (`p1.txt`). Errors sent to a file reach neither the screen nor a script's transcript. Three older tests that used `2>` as their example of a refused line use `3>`. The test double's `write_error` fails one fd's writes. The red run is the shell's tests. Mutation checks (10): fd 2 taken for 1, bash's error after an operator, the built-in's error target, the write error as the command's own, the fd's write, the file's offset after each error, the own message's fd, `say_on`'s write, and the failure told on the screen, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/reader.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 2's context; `Ctx::err`.
- Produces: `Parser::fd_word`; `ctx::To` as `Ctx::new`'s error target; `runner::Ran::{own, own()}`; `Shell::say_on(fds, bytes)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
        assert_eq!(String::from_utf8(h.get("/tmp/s.log")).unwrap(), screen);
        // Running a script again starts a new transcript.
````

with:

````rust
        assert_eq!(String::from_utf8(h.get("/tmp/s.log")).unwrap(), screen);
        // Errors sent to a file reach neither the screen nor the
        // transcript (programmable shell gate §7.5).
        h.put("/tmp/s.sh", b"cat /tmp/nope 2> /tmp/e\nnope 2>> /tmp/e\n");
        assert_eq!(
            h.run("sh /tmp/s.sh"),
            (127, "+ cat /tmp/nope 2> /tmp/e\n+ nope 2>> /tmp/e\n".into())
        );
        assert_eq!(
            h.get("/tmp/s.log"),
            b"+ cat /tmp/nope 2> /tmp/e\n+ nope 2>> /tmp/e\n"
        );
        assert_eq!(
            h.get("/tmp/e"),
            b"cat: /tmp/nope: No such file or directory\n\
              relay-sh: nope: command not found\n"
        );
        // Running a script again starts a new transcript.
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(c.output(), Some((&"b".into(), true)));
        // Other streams are not supported; a quoted or spaced digit is a word.
        assert_eq!(
            one("cat f 2>err"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(
            one("echo a 2>>g"),
            Err(ParseError::Unsupported("2>".into()))
        );
        assert_eq!(one("echo 2 > g").unwrap().words, ["echo", "2"]);
````

with:

````rust
        assert_eq!(c.output(), Some((&"b".into(), true)));
        // A word of digits just before the operator is its fd, 1 or 2
        // (programmable shell gate §7.1); a quoted or spaced digit, or one
        // in a longer word, is a word.
        for (line, fd, op) in [
            ("cat f 2>err", 2, RedirectOp::Write("err".into())),
            ("echo a 2>>g", 2, RedirectOp::Append("g".into())),
            ("echo a 1> g", 1, RedirectOp::Write("g".into())),
            ("echo a 1>>g", 1, RedirectOp::Append("g".into())),
        ] {
            assert_eq!(
                one(line).unwrap().redirects,
                [Redirect { fd, op }],
                "{line}"
            );
        }
        for (line, refused) in [
            ("echo a 3> g", "3>"),
            ("echo a 0> g", "0>"),
            ("echo a 02> g", "02>"),
            ("echo a 10> g", "10>"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // Even where a file name is awaited (bash 5.2, probes/p8.txt).
        for line in ["echo a >2>f", "echo a 2>2>f"] {
            assert_eq!(
                one(line).unwrap_err().to_string(),
                "syntax error near unexpected token `2'",
                "{line}"
            );
        }
        assert_eq!(one("echo a 2> >f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(one("echo a 2>>"), Err(ParseError::MissingTarget("newline")));
        assert_eq!(one("echo 2 > g").unwrap().words, ["echo", "2"]);
````

Replace:

````rust
        assert_eq!(
            one("echo a 2>f").unwrap_err().to_string(),
            "unsupported syntax: 2>"
        );
````

with:

````rust
        assert_eq!(
            one("echo a 3>f").unwrap_err().to_string(),
            "unsupported syntax: 3>"
        );
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/reader.rs`**

In `crates/shell/src/reader.rs`, replace:

````rust
        assert_eq!(
            r.add("b 2> f &&"),
            Err(ParseError::Unsupported("2>".into()))
        );
````

with:

````rust
        assert_eq!(
            r.add("b 3> f &&"),
            Err(ParseError::Unsupported("3>".into()))
        );
````

- [ ] **Step 4: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
        // A write that fails is the built-in's write error.
        h.programs.write_error = Some(Errno::ENOSPC);
        assert_eq!(
````

with:

````rust
        // A write that fails is the built-in's write error.
        h.programs.write_error = Some((5, Errno::ENOSPC));
        assert_eq!(
````

- [ ] **Step 5: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
            "/tmp/s.sh",
            b"t-args a 2> log &&\nt-args tail\nt-args next\n",
        );
````

with:

````rust
            "/tmp/s.sh",
            b"t-args a 3> log &&\nt-args tail\nt-args next\n",
        );
````

Replace:

````rust
    #[test]
    fn a_redirection_that_cannot_open_stops_the_command() {
````

with:

````rust
    #[test]
    fn standard_error_may_go_to_a_file() {
        // bash 5.2 (tmp/m5p1/probes/p1.txt): a command's own messages go to
        // its fd 2, a built-in's and the shell's of it alike.
        let mut h = Harness::new();
        assert_eq!(h.run("cd /missing 2> /tmp/e"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: cd: /missing: No such file or directory\n"
        );
        assert_eq!(h.run("nope 2>> /tmp/e"), (127, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: cd: /missing: No such file or directory\n\
              relay-sh: nope: command not found\n"
        );
        assert_eq!(h.run("ls /tmp/e /nope 2> /tmp/e2"), (2, "/tmp/e\n".into()));
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        // Each error after the one before.
        assert_eq!(h.run("ls /nope /nope2 2> /tmp/e2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n\
              ls: cannot access '/nope2': No such file or directory\n"
        );
        // Output stays where it was; an fd 2 with nothing to say is made.
        assert_eq!(h.run("echo a 2> /tmp/e3"), (0, "a\n".into()));
        assert_eq!(h.get("/tmp/e3"), b"");
        // A redirection that cannot be made is told on fd 2 as it stands
        // then.
        assert_eq!(h.run("echo a 2> /tmp/e4 > /nodir/x"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e4"),
            b"relay-sh: /nodir/x: No such file or directory\n"
        );
        assert_eq!(
            h.run("echo a > /nodir/x 2> /tmp/e5"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert!(!h.exists("/tmp/e5"));
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
        let mut h = spawning();
        assert_eq!(h.spawning("t-args 2> /tmp/e").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 1, 4]);
        assert_eq!(h.programs.closed, [4]);
        // Its own messages, and a built-in's errors, go through the fd.
        assert_eq!(h.spawning("nope 2> /tmp/e"), (127, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/e"),
            b"relay-sh: nope: command not found\n"
        );
        assert_eq!(h.spawning("cd /missing 2>> /tmp/f"), (1, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/f"),
            b"relay-sh: cd: /missing: No such file or directory\n"
        );
        // A built-in's write error is its own message too.
        h.programs.write_error = Some((7, Errno::ENOSPC));
        assert_eq!(h.spawning("help > /tmp/h 2> /tmp/g"), (1, String::new()));
        assert_eq!(h.programs.opened[3], ("/tmp/h".into(), false, 7));
        assert_eq!(
            h.programs.written_to("/tmp/g"),
            b"help: write error: No space left on device\n"
        );
    }

    #[test]
    fn a_redirection_that_cannot_open_stops_the_command() {
````

- [ ] **Step 6: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub written: Vec<(u32, Vec<u8>)>,
    /// What `write` fails with, if anything.
    pub write_error: Option<Errno>,
    pub spawned: Vec<Spawned>,
````

with:

````rust
    pub written: Vec<(u32, Vec<u8>)>,
    /// The fd whose writes fail, and with what.
    pub write_error: Option<(u32, Errno)>,
    pub spawned: Vec<Spawned>,
````

Replace:

````rust
    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
        if let Some(e) = self.write_error {
            return Err(e);
````

with:

````rust
    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
        if let Some((bad, e)) = self.write_error
            && bad == fd
        {
            return Err(e);
````

- [ ] **Step 7: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 4 tests fail, among them `commands::script::tests::the_transcript_holds_what_the_screen_showed`, `parser::tests::redirection_errors`.

- [ ] **Step 8: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! (the screen, a redirection file, `/bin/sh`'s fd for one, or a program's
//! fd 1) and the screen for errors; plus the helpers every command shares
//! for options and GNU-style messages.

````

with:

````rust
//! (the screen, a redirection file, `/bin/sh`'s fd for one, or a program's
//! fd 1) and errors (the screen, or a redirection file); plus the helpers
//! every command shares for options and GNU-style messages.

````

Replace:

````rust
    out: Output<'a>,
    /// Standard input; without one, the input ends at once.
````

with:

````rust
    out: Output<'a>,
    /// Where errors go (programmable shell gate §7.5).
    err: To,
    /// Standard input; without one, the input ends at once.
````

Replace:

````rust
impl<'a> Ctx<'a> {
    /// Standard output goes `to` (an fd through `control`'s programs).
    pub(crate) fn new(
````

with:

````rust
impl<'a> Ctx<'a> {
    /// Standard output goes `to` and errors to `err` (an fd through
    /// `control`'s programs).
    pub(crate) fn new(
````

Replace:

````rust
        to: To,
    ) -> Ctx<'a> {
````

with:

````rust
        to: To,
        err: To,
    ) -> Ctx<'a> {
````

Replace:

````rust
            },
        };
        Ctx::with_output(vfs, system, console, out)
    }

````

with:

````rust
            },
        };
        let mut ctx = Ctx::with_output(vfs, system, console, out);
        ctx.err = err;
        ctx
    }

````

Replace:

````rust
            out,
            input: None,
````

with:

````rust
            out,
            err: To::Console,
            input: None,
````

Replace:

````rust

    /// Errors always go to the screen, never into a redirection file.
    pub fn err(&mut self, bytes: &[u8]) {
        self.streams().screen(bytes);
    }
````

with:

````rust

    /// Errors: to the screen, or where they are redirected. A write that
    /// fails is lost, as there is nowhere left to say so.
    pub fn err(&mut self, bytes: &[u8]) {
        match &mut self.err {
            To::Console => self.streams().screen(bytes),
            To::File(node, offset) => {
                let mut done = 0;
                while done < bytes.len() {
                    match self.vfs.write_at(*node, *offset, &bytes[done..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                    }
                }
            }
            To::Fd(fd) => {
                if let Some(programs) = self
                    .control
                    .as_mut()
                    .and_then(|c| c.programs.as_deref_mut())
                {
                    let _ = programs.write(*fd, bytes);
                }
            }
        }
    }
````

- [ ] **Step 9: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
//!
//! `> file` and `>> file` redirect standard output, any number of them per
//! command, made left to right. An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

with:

````rust
//!
//! `> file` and `>> file` redirect standard output, and `2> file` and
//! `2>> file` standard error (programmable shell gate §7.1), any number of
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

Replace:

````rust
//! passed on as if it were plain text; so are `|&` (the errors into the
//! pipe too), `>&` and `2>` (another stream).

````

with:

````rust
//! passed on as if it were plain text; so are `|&` (the errors into the
//! pipe too) and `>&`.

````

Replace:

````rust
    redirects: Vec<Redirect<Word>>,
    /// A `>` (false) or `>>` (true) seen, waiting for its file name.
    pending: Option<bool>,
    /// How many `!` stood before the pipeline's first command.
````

with:

````rust
    redirects: Vec<Redirect<Word>>,
    /// A redirection's fd and `>` (false) or `>>` (true), waiting for its
    /// file name.
    pending: Option<(u32, bool)>,
    /// How many `!` stood before the pipeline's first command.
````

Replace:

````rust
        match self.pending.take() {
            Some(append) => {
                let op = if append {
````

with:

````rust
        match self.pending.take() {
            Some((fd, append)) => {
                let op = if append {
````

Replace:

````rust
                };
                self.redirects.push(Redirect { fd: 1, op });
            }
````

with:

````rust
                };
                self.redirects.push(Redirect { fd, op });
            }
````

Replace:

````rust
                '>' => {
                    // `2>` redirects another stream in a real shell.
                    if self.word.started
                        && let Some(digits) = self.word.word.digits()
                    {
                        return Err(ParseError::Unsupported(format!("{digits}>")));
                    }
                    self.end_word(line, at)?;
````

with:

````rust
                '>' => {
                    // A word of digits just before it is its fd, as in bash,
                    // even where a file name is awaited (`>2>f` is bash's
                    // error naming the `2`); only 1 and 2 are taken.
                    let (fd, typed) = match self.fd_word()? {
                        Some(digits) => match digits.as_str() {
                            "1" => (1, "1"),
                            "2" => (2, "2"),
                            _ => return Err(ParseError::Unsupported(format!("{digits}>"))),
                        },
                        None => (1, ""),
                    };
                    self.end_word(line, at)?;
````

Replace:

````rust
                        let op = if append { ">>" } else { ">" };
                        return Err(ParseError::Unsupported(format!("{op} after {}", c.end())));
                    }
````

with:

````rust
                        let op = if append { ">>" } else { ">" };
                        let end = c.end();
                        return Err(ParseError::Unsupported(format!("{typed}{op} after {end}")));
                    }
````

Replace:

````rust
                    }
                    self.parts.pending = Some(append);
                }
````

with:

````rust
                    }
                    self.parts.pending = Some((fd, append));
                }
````

Replace:

````rust
            }
        }
        Ok(())
    }

    /// The stage of the `for` whose header is being read.
````

with:

````rust
            }
        }
        Ok(())
    }

    /// The word being read, if it is all unquoted digits and so the fd of
    /// the redirection operator after it (programmable shell gate §7.1);
    /// it is taken. After an operator that awaits its file name it is
    /// bash's syntax error naming it.
    fn fd_word(&mut self) -> Result<Option<String>, ParseError> {
        if !self.word.started {
            return Ok(None);
        }
        let Some(digits) = self.word.word.digits() else {
            return Ok(None);
        };
        let digits = String::from(digits);
        if self.parts.pending.is_some() {
            return Err(ParseError::Unexpected(digits));
        }
        self.word = Building::default();
        Ok(Some(digits))
    }

    /// The stage of the `for` whose header is being read.
````

- [ ] **Step 10: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
    pub message: String,
    /// `exit`, or `reboot`/`poweroff` returning: the shell stops.
````

with:

````rust
    pub message: String,
    /// The message is the command's own (it was not found, could not
    /// start, could not write), told on its fd 2, where the shell's
    /// reports go to the fd 2 around it (programmable shell gate §7.5).
    pub own: bool,
    /// `exit`, or `reboot`/`poweroff` returning: the shell stops.
````

Replace:

````rust
            message,
            stop: false,
            exited: false,
            script: None,
            cancelled: false,
        }
````

with:

````rust
            message,
            own: false,
            stop: false,
            exited: false,
            script: None,
            cancelled: false,
        }
    }

    /// The command's own message.
    pub fn own(status: i32, message: String) -> Ran {
        Ran {
            own: true,
            ..Ran::said(status, message)
        }
````

Replace:

````rust

/// Runs a command function on `fds`: its output to the screen, a file or
/// (under `/bin/sh`, through `control`'s programs) an fd.
pub(crate) fn run_function<'s>(
````

with:

````rust

/// Where a command the shell runs itself writes for `slot`.
fn to(files: &Files, slot: Slot) -> To {
    match slot {
        Slot::Shell(_) => To::Console,
        Slot::File(i) => match files.handle(i) {
            Handle::Fd(fd) => To::Fd(fd),
            Handle::Node { node, offset } => To::File(node, offset),
        },
    }
}

/// Runs a command function on `fds`: its output and errors to the screen,
/// a file or (under `/bin/sh`, through `control`'s programs) an fd.
pub(crate) fn run_function<'s>(
````

Replace:

````rust
) -> Ran {
    let to = match fds.0[1] {
        Slot::Shell(_) => To::Console,
        Slot::File(i) => match parts.files.handle(i) {
            Handle::Fd(fd) => To::Fd(fd),
            Handle::Node { node, offset } => To::File(node, offset),
        },
    };
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, to);
    ctx.control = control;
````

with:

````rust
) -> Ran {
    let out = to(parts.files, fds.0[1]);
    let err = to(parts.files, fds.0[2]);
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, out, err);
    ctx.control = control;
````

Replace:

````rust
    ctx.transcript = parts.transcript.take();
    let mut status = (command.run)(&mut ctx, args);
    let mut message = String::new();
    if let Err(e) = ctx.finish() {
        message = format!("{}: write error: {e}\n", command.name);
        status = ctx.write_error_status;
    }
    if ctx.cancelled {
        message = String::from("^C\n");
        status = CANCELLED;
    }
    *parts.transcript = ctx.transcript.take();
    Ran {
        status,
        message,
        stop: ctx.exit,
````

with:

````rust
    ctx.transcript = parts.transcript.take();
    let status = (command.run)(&mut ctx, args);
    let finished = ctx.finish();
    let (status, message, own) = if ctx.cancelled {
        (CANCELLED, String::from("^C\n"), false)
    } else if let Err(e) = finished {
        let message = format!("{}: write error: {e}\n", command.name);
        (ctx.write_error_status, message, true)
    } else {
        (status, String::new(), false)
    };
    *parts.transcript = ctx.transcript.take();
    Ran {
        status,
        message,
        own,
        stop: ctx.exit,
````

Replace:

````rust
    };
    Ran::said(status, format!("{NAME}: {name}: {e}\n"))
}

pub(crate) fn not_found(name: &str) -> Ran {
    Ran::said(NOT_FOUND, format!("{NAME}: {name}: command not found\n"))
}
````

with:

````rust
    };
    Ran::own(status, format!("{NAME}: {name}: {e}\n"))
}

pub(crate) fn not_found(name: &str) -> Ran {
    Ran::own(NOT_FOUND, format!("{NAME}: {name}: command not found\n"))
}
````

- [ ] **Step 11: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 10 replacements, top to bottom:

Replace:

````rust
use crate::expand::{self, Vars};
use crate::fds::{Fds, Files, Opener};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::expand::{self, Vars};
use crate::fds::{Fds, Files, Handle, Opener, Slot};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
        }
        let fds = match self.redirect(self.fds, &cmd.redirects) {
            Ok(fds) => fds,
            Err(message) => return self.finish(1, message),
        };
````

with:

````rust
        }
        let Some(fds) = self.redirect(self.fds, &cmd.redirects) else {
            return self.finish(1, String::new());
        };
````

Replace:

````rust
        };
        self.release(fds);
````

with:

````rust
        };
        let mut message = ran.message;
        if ran.own {
            self.say_on(fds, message.as_bytes());
            message.clear();
        }
        self.release(fds);
````

Replace:

````rust
        }
        self.finish(status, ran.message)
    }
````

with:

````rust
        }
        self.finish(status, message)
    }
````

Replace:

````rust
        match self.redirect(self.fds, &redirects) {
            Ok(fds) => {
                self.release(fds);
                self.finish(0, String::new())
            }
            Err(message) => self.finish(1, message),
        }
````

with:

````rust
        match self.redirect(self.fds, &redirects) {
            Some(fds) => {
                self.release(fds);
                self.finish(0, String::new())
            }
            None => self.finish(1, String::new()),
        }
````

Replace:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = match self.stage_fds(stages) {
            Ok(all) => all,
            Err(message) => return self.finish(1, message),
        };
        let staged = runner_stages(stages, &all);
````

with:

````rust
            return self.finish(ran.status, ran.message);
        }
        let Some(all) = self.stage_fds(stages) else {
            return self.finish(1, String::new());
        };
        let staged = runner_stages(stages, &all);
````

Replace:

````rust
        }
        let all = match self.stage_fds(stages) {
            Ok(all) => all,
            Err(message) => return self.finish(1, message),
        };
````

with:

````rust
        }
        let Some(all) = self.stage_fds(stages) else {
            return self.finish(1, String::new());
        };
````

Replace:

````rust
    /// The context `redirects` make over `base` (programmable shell gate
    /// §7.2), or what keeps one from being made.
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Result<Fds, String> {
        let mut opener = Opener {
````

with:

````rust
    /// The context `redirects` make over `base` (programmable shell gate
    /// §7.2). One that cannot be made is told on fd 2 as it stands then,
    /// as bash tells it (`cat 2> e < missing` writes into `e`), and what
    /// was made of it is closed.
    fn redirect(&mut self, base: Fds, redirects: &[parser::Redirect]) -> Option<Fds> {
        let mut opener = Opener {
````

Replace:

````rust
        match self.files.redirect(base, redirects, &mut opener) {
            Ok(fds) => Ok(fds),
            Err(failed) => {
                self.files.release(failed.fds, &mut opener);
                Err(format!("{NAME}: {}: {}\n", failed.path, failed.error))
            }
````

with:

````rust
        match self.files.redirect(base, redirects, &mut opener) {
            Ok(fds) => Some(fds),
            Err(failed) => {
                let message = format!("{NAME}: {}: {}\n", failed.path, failed.error);
                self.say_on(failed.fds, message.as_bytes());
                self.release(failed.fds);
                None
            }
        }
    }

    /// Writes where fd 2 of `fds` goes: the screen (and a running
    /// script's transcript), or a file. A write that fails is lost, as
    /// there is nowhere left to say so.
    fn say_on(&mut self, fds: Fds, bytes: &[u8]) {
        let Slot::File(i) = fds.0[2] else {
            return self.say(bytes);
        };
        match self.files.handle(i) {
            Handle::Fd(fd) => {
                if let Some(programs) = self.runner.programs() {
                    let _ = programs.write(fd, bytes);
                }
            }
            Handle::Node { node, mut offset } => {
                let mut done = 0;
                while done < bytes.len() {
                    match self.vfs.write_at(node, offset, &bytes[done..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            done += n;
                            offset += n as u64;
                        }
                    }
                }
            }
````

Replace:

````rust
    /// one cannot be made.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Result<Vec<Fds>, String> {
        let mut all = Vec::new();
        for stage in stages {
            match self.redirect(self.fds, &stage.redirects) {
                Ok(fds) => all.push(fds),
                Err(message) => {
                    for fds in all {
                        self.release(fds);
                    }
                    return Err(message);
                }
            }
        }
        Ok(all)
    }
````

with:

````rust
    /// one cannot be made.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Option<Vec<Fds>> {
        let mut all = Vec::new();
        for stage in stages {
            match self.redirect(self.fds, &stage.redirects) {
                Some(fds) => all.push(fds),
                None => {
                    for fds in all {
                        self.release(fds);
                    }
                    return None;
                }
            }
        }
        Some(all)
    }
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 391 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): redirect standard error with 2> and 2>>

A word of digits just before `>` or `>>` is the redirection's fd, as in
bash; 1 and 2 are taken and any other is refused (programmable shell
gate §7.1), and after an operator that awaits its file name it is bash's
syntax error naming it. `Ctx` gains an error target, so a built-in's
errors go to its fd 2 (§7.5). A command's own messages (not found,
cannot start, a write error) go to its fd 2 once its redirections are
made, and one that cannot be made is told on fd 2 as it stands then, as
bash tells them. Errors sent to a file reach neither the screen nor a
script's transcript.
EOF
````


### Task 5: A file opened with `>>` is written at its end each time

Decision 3 (the prototype's review, M-1): in the in-process runner a file opened with `>>` was written from the size it had when it was opened, so two fds appending to it (`ls f nope >> g 2>> g`) wrote over each other, where the kernel's append and bash write each at the end (`tmp/m5p1/probes/p14.txt`). Such a file now has no offset of its own (`fds::AT_END`), and every write goes to its end (`fds::write_file`, which the error target, the output and the shell's messages share). `/bin/sh` was right: the kernel's `OPEN_APPEND`. The red run is the shell's tests: the error and the output overwrite each other. Mutation checks (5): the old offset, the end not looked up, the marker lost after the first write (two errors on one fd), and a write that takes nothing, each fail a test.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 4's error target; `fds::open_output`.
- Produces: `fds::AT_END`, `fds::write_file(vfs, node, offset, bytes) -> (u64, Option<Errno>)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, replace:

````rust
        };
        assert_eq!(offset, 5);
        files.release(fds, &mut opener);
````

with:

````rust
        };
        assert_eq!(offset, AT_END, "written at its end each time");
        files.release(fds, &mut opener);
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn a_file_appended_to_is_written_at_its_end_each_time() {
        // The prototype's review (M-1): two fds that append to one file, as
        // the kernel's append does (bash 5.2, tmp/m5p1/probes/p14.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"x\n");
        h.put("/tmp/g", b"old\n");
        assert_eq!(
            h.run("ls /tmp/f /nope >> /tmp/g 2>> /tmp/g"),
            (2, String::new())
        );
        assert_eq!(
            h.get("/tmp/g"),
            b"old\nls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        // Each write of one fd at the end too.
        h.put("/tmp/g2", b"old\n");
        assert_eq!(h.run("ls /nope /nope2 2>> /tmp/g2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/g2"),
            b"old\nls: cannot access '/nope': No such file or directory\n\
              ls: cannot access '/nope2': No such file or directory\n"
        );
        // Wherever another fd wrote meanwhile.
        assert_eq!(h.run("cd /nope >> /tmp/g 2>> /tmp/g"), (1, String::new()));
        assert!(
            h.get("/tmp/g")
                .ends_with(b"/tmp/f\nrelay-sh: cd: /nope: No such file or directory\n")
        );
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` cannot find value `AT_END` in this scope ``.

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust

use crate::fds;
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
            To::File(node, offset) => {
                let mut done = 0;
                while done < bytes.len() {
                    match self.vfs.write_at(*node, *offset, &bytes[done..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                    }
                }
            }
````

with:

````rust
            To::File(node, offset) => {
                *offset = fds::write_file(&mut *self.vfs, *node, *offset, bytes).0;
            }
````

Replace:

````rust
            } => {
                let mut done = 0;
                while error.is_none() && done < buf.len() {
                    match self.vfs.write_at(*node, *offset, &buf[done..]) {
                        // Nothing written would loop forever; the contract
                        // says that is ENOSPC.
                        Ok(0) => *error = Some(Errno::ENOSPC),
                        Ok(n) => {
                            done += n;
                            *offset += n as u64;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
````

with:

````rust
            } => {
                if error.is_none() {
                    let (at, failed) = fds::write_file(&mut *self.vfs, *node, *offset, buf);
                    *offset = at;
                    *error = failed;
                }
````

- [ ] **Step 5: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, make these 3 replacements, top to bottom:

Replace:

````rust

/// A file a redirection opened.
````

with:

````rust

/// The in-process runner's offset of a file opened with `>>`: every write
/// goes to its end, as the kernel's append does, wherever another fd of it
/// has written.
pub(crate) const AT_END: u64 = u64::MAX;

/// Writes all of `bytes` to the in-process runner's file `node` at
/// `offset`, or at its end for [`AT_END`]: where the next write goes, and
/// the error that stopped it (`ENOSPC` for a write that took nothing).
pub(crate) fn write_file(
    vfs: &mut dyn Vfs,
    node: Node,
    offset: u64,
    bytes: &[u8],
) -> (u64, Option<Errno>) {
    let mut at = offset;
    let mut done = 0;
    while done < bytes.len() {
        let pos = if offset == AT_END {
            match vfs.stat(node) {
                Ok(st) => st.size,
                Err(e) => return (at, Some(e)),
            }
        } else {
            at
        };
        match vfs.write_at(node, pos, &bytes[done..]) {
            Ok(0) => return (at, Some(Errno::ENOSPC)),
            Ok(n) => {
                done += n;
                if offset != AT_END {
                    at += n as u64;
                }
            }
            Err(e) => return (at, Some(e)),
        }
    }
    (at, None)
}

/// A file a redirection opened.
````

Replace:

````rust
    Fd(u32),
    /// The in-process runner's: the file, and where the next write goes.
    Node { node: Node, offset: u64 },
````

with:

````rust
    Fd(u32),
    /// The in-process runner's: the file, and where the next write goes
    /// ([`AT_END`] for one opened with `>>`).
    Node { node: Node, offset: u64 },
````

Replace:

````rust
    };
    let offset = if append { vfs.stat(node)?.size } else { 0 };
    Ok((node, offset))
````

with:

````rust
    };
    let offset = if append { AT_END } else { 0 };
    Ok((node, offset))
````

- [ ] **Step 6: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use crate::expand::{self, Vars};
use crate::fds::{Fds, Files, Handle, Opener, Slot};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

with:

````rust
use crate::expand::{self, Vars};
use crate::fds::{self, Fds, Files, Handle, Opener, Slot};
use crate::io::{Console, Programs, Stdin, Stdout, System};
````

Replace:

````rust
            }
            Handle::Node { node, mut offset } => {
                let mut done = 0;
                while done < bytes.len() {
                    match self.vfs.write_at(node, offset, &bytes[done..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            done += n;
                            offset += n as u64;
                        }
                    }
                }
            }
````

with:

````rust
            }
            Handle::Node { node, offset } => {
                fds::write_file(&mut *self.vfs, node, offset, bytes);
            }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 392 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): write a file opened with >> at its end each time

In the in-process runner a file opened with `>>` was written from the
size it had when it was opened, so two fds appending to it, `ls f nope
>> g 2>> g`, wrote over each other, where the kernel's append and bash
write each at the end (the prototype's review, M-1). Such a file now
has no offset of its own (`AT_END`), and every write goes to its end
(`fds::write_file`, which the error target, the output and the shell's
messages share).
EOF
````


### Task 6: `2>&1`, `1>&2` and `>&2`; `&>` and `>|` refused

Decision 2 (spec §7.1): `2>&1`, `1>&2` and `>&2` make one of fds 1 and 2 a copy of the other as it is at that point (`RedirectOp::Copy`), so `> f 2>&1` sends both to `f` and `2>&1 > f` only the output (`p1.txt`); a blank may follow `>&` (`2>& 1`, `p6.txt`), and `1>&1` and `2>&2` change nothing. Any other word after `>&`, which bash expands and takes as another fd, a file or `-`, is refused naming it (`>&-`, `>&f`, `2>&3`, `2>&01`, `2>&"1"`, `2>&$N`), and so are bash's `&>`, which ran its command in the background and then made the file, and `>|`; `2>>&1` and `> &2` stay bash's syntax error. A copy holds its file once more (`Files::hold_slot`). When fd 2 is a copy of fd 1's file, a built-in writes its errors through the output (`To::Output`), in order. The red run is the shell's tests. Mutation checks (8): the copy's hold, its source fd, the close of the slot it replaces, the fd copied, a digit after `>&` taken for an fd, the shared output, and the refusals of `&>` and `>|`, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 4's fds and error target.
- Produces: `parser::RedirectOp::Copy(u32)`, `parser::Pending::{File, Copy}` (private); `Files::hold_slot`; `ctx::To::Output`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(one("echo a 2> >f"), Err(ParseError::MissingTarget(">")));
        assert_eq!(one("echo a 2>>"), Err(ParseError::MissingTarget("newline")));
````

with:

````rust
        assert_eq!(one("echo a 2> >f"), Err(ParseError::MissingTarget(">")));
        // `>&` copies fd 1 or 2, a blank before the fd or not (bash 5.2,
        // probes/p1.txt, p6.txt).
        let copy = |fd, from| Redirect {
            fd,
            op: RedirectOp::Copy(from),
        };
        for (line, redirects) in [
            ("echo a 2>&1", alloc::vec![copy(2, 1)]),
            ("echo a 1>&2", alloc::vec![copy(1, 2)]),
            ("echo a >&2", alloc::vec![copy(1, 2)]),
            ("echo a >& 2", alloc::vec![copy(1, 2)]),
            ("echo a 2>& 1", alloc::vec![copy(2, 1)]),
            ("echo a 2>&2 1>&1", alloc::vec![copy(2, 2), copy(1, 1)]),
        ] {
            let c = one(line).unwrap();
            assert_eq!(
                (c.words, c.redirects),
                (words("echo a"), redirects),
                "{line}"
            );
        }
        let c = one("echo a 2>&1>f").unwrap();
        assert_eq!(
            c.redirects,
            [
                copy(2, 1),
                Redirect {
                    fd: 1,
                    op: RedirectOp::Write("f".into())
                }
            ]
        );
        // Anything else bash takes there is refused: another fd, a file,
        // `-`, a word quoted or expanded.
        for (line, refused) in [
            ("echo a 2>&3", "2>&3"),
            ("echo a >&0", ">&0"),
            ("echo a 0>&1", "0>"),
            ("echo a >&-", ">&-"),
            ("echo a >&f", ">&f"),
            ("echo a 2>&1x", "2>&1x"),
            ("echo a 2>&01", "2>&01"),
            ("echo a 2>&\"1\"", "2>&\"1\""),
            ("echo a 2>&$N", "2>&$N"),
            ("echo a &> f", "&>"),
            ("echo a >| f", ">|"),
        ] {
            assert_eq!(
                one(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // bash's syntax errors.
        for (line, token) in [
            ("echo a 2>>&1", "&"),
            ("echo a > &2", "&"),
            ("echo a 2>&", "newline"),
        ] {
            assert_eq!(one(line), Err(ParseError::MissingTarget(token)), "{line}");
        }
        assert_eq!(one("echo a 2>>"), Err(ParseError::MissingTarget("newline")));
````

Replace:

````rust
            ("> f &", "> &"),
            // bash runs these (the review found them called its syntax
            // error).
            ("echo hi >&2", ">&"),
            ("echo hi >& f", ">&"),
        ] {
````

with:

````rust
            ("> f &", "> &"),
            // bash runs this (the review found it called its syntax
            // error); `>&2` copies fd 2 (programmable shell gate §7.1).
            ("echo hi >& f", ">&f"),
        ] {
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn one_output_may_be_made_a_copy_of_the_other() {
        // bash 5.2 (tmp/m5p1/probes/p1.txt, p2.txt): the copy is of the
        // fd as it is at that point.
        let mut h = Harness::new();
        h.put("/tmp/f", b"");
        assert_eq!(h.run("ls /tmp/f /nope > /tmp/o 2>&1"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/o"),
            b"ls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        assert_eq!(
            h.run("ls /tmp/f /nope 2>&1 > /tmp/o2"),
            (
                2,
                "ls: cannot access '/nope': No such file or directory\n".into()
            )
        );
        assert_eq!(h.get("/tmp/o2"), b"/tmp/f\n");
        // Output to fd 2.
        assert_eq!(h.run("echo a 2> /tmp/e >&2"), (0, String::new()));
        assert_eq!(h.get("/tmp/e"), b"a\n");
        assert_eq!(h.run("echo a >&2 2> /tmp/e2"), (0, "a\n".into()));
        assert_eq!(h.get("/tmp/e2"), b"");
        // A built-in's errors and output, in order.
        assert_eq!(h.run("cd /nope > /tmp/c 2>&1"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/c"),
            b"relay-sh: cd: /nope: No such file or directory\n"
        );
        // Under /bin/sh both fds are the one file, opened once.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/o 2>&1").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 4, 4]);
        assert_eq!(h.spawning("t-args 2>&1 > /tmp/o").0, 3);
        assert_eq!(h.programs.spawned[1].fds, [0, 5, 1]);
        assert_eq!(h.spawning("t-args 2> /tmp/e 1>&2 2>&1").0, 3);
        assert_eq!(h.programs.spawned[2].fds, [0, 6, 6]);
        assert_eq!(h.programs.closed, [4, 5, 6], "each once, after the program");
        assert_eq!(h.spawning("nope > /tmp/n 2>&1"), (127, String::new()));
        assert_eq!(
            h.programs.written_to("/tmp/n"),
            b"relay-sh: nope: command not found\n"
        );
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` no variant, associated function, or constant named `Copy` found for enum `parser::RedirectOp<W>` in the current scope ``.

- [ ] **Step 4: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
/// Where a command the shell runs itself writes (programmable shell gate
/// §7.5): the screen, a file of the in-process runner's at an offset, or
/// an fd of `/bin/sh`'s, which its `Programs` write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

with:

````rust
/// Where a command the shell runs itself writes (programmable shell gate
/// §7.5): the screen, a file of the in-process runner's at an offset, an
/// fd of `/bin/sh`'s, which its `Programs` write, or (errors) where the
/// output goes, when fd 2 is a copy of fd 1's file or the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

Replace:

````rust
    Fd(u32),
}
````

with:

````rust
    Fd(u32),
    Output,
}
````

Replace:

````rust
        let out = match to {
            To::Console => Output::Console,
            To::File(node, offset) => Output::File {
````

with:

````rust
        let out = match to {
            To::Console | To::Output => Output::Console,
            To::File(node, offset) => Output::File {
````

Replace:

````rust
            To::Console => self.streams().screen(bytes),
            To::File(node, offset) => {
````

with:

````rust
            To::Console => self.streams().screen(bytes),
            // In order with the output, through its buffer and offset.
            To::Output => {
                let _ = self.streams().out(bytes);
            }
            To::File(node, offset) => {
````

- [ ] **Step 5: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
            RedirectOp::Append(path) => RedirectOp::Append(self.target(path)?),
        };
````

with:

````rust
            RedirectOp::Append(path) => RedirectOp::Append(self.target(path)?),
            RedirectOp::Copy(fd) => RedirectOp::Copy(*fd),
        };
````

- [ ] **Step 6: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
impl Opener<'_> {
    fn open(&mut self, op: &RedirectOp) -> Result<Handle, Errno> {
        let (path, append) = match op {
            RedirectOp::Write(path) => (path, false),
            RedirectOp::Append(path) => (path, true),
        };
        if let Some(programs) = self.programs.as_deref_mut() {
````

with:

````rust
impl Opener<'_> {
    fn open(&mut self, path: &str, append: bool) -> Result<Handle, Errno> {
        if let Some(programs) = self.programs.as_deref_mut() {
````

Replace:

````rust
        for r in redirects {
            let slot = match opener.open(&r.op) {
                Ok(handle) => Slot::File(self.add(handle)),
                Err(error) => {
                    let path = match &r.op {
                        RedirectOp::Write(path) | RedirectOp::Append(path) => path.clone(),
                    };
                    return Err(Failed { fds, path, error });
````

with:

````rust
        for r in redirects {
            let (path, append) = match &r.op {
                RedirectOp::Write(path) => (path, false),
                RedirectOp::Append(path) => (path, true),
                // The other fd as it is now, held once more.
                RedirectOp::Copy(from) => {
                    let slot = fds.0[*from as usize];
                    self.hold_slot(slot);
                    let replaced = core::mem::replace(&mut fds.0[r.fd as usize], slot);
                    self.drop_slot(replaced, opener);
                    continue;
                }
            };
            let slot = match opener.open(path, append) {
                Ok(handle) => Slot::File(self.add(handle)),
                Err(error) => {
                    let path = path.clone();
                    return Err(Failed { fds, path, error });
````

Replace:

````rust
        for slot in fds.0 {
            if let Slot::File(i) = slot
                && let Some(Some((_, count))) = self.open.get_mut(i)
            {
                *count += 1;
            }
        }
````

with:

````rust
        for slot in fds.0 {
            self.hold_slot(slot);
        }
    }

    /// One more fd holds `slot`.
    fn hold_slot(&mut self, slot: Slot) {
        if let Slot::File(i) = slot
            && let Some(Some((_, count))) = self.open.get_mut(i)
        {
            *count += 1;
        }
````

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 9 replacements, top to bottom:

Replace:

````rust
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. An unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

with:

````rust
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. `2>&1`, `1>&2` and
//! `>&2` make one of them a copy of the other as it is at that point; any
//! other word after `>&` is refused, as are bash's `&>` and `>|`. An
//! unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
````

Replace:

````rust
    Append(W),
}
````

with:

````rust
    Append(W),
    /// `>&N`: a copy of fd N (1 or 2) as it is at that point.
    Copy(u32),
}
````

Replace:

````rust
    redirects: Vec<Redirect<Word>>,
    /// A redirection's fd and `>` (false) or `>>` (true), waiting for its
    /// file name.
    pending: Option<(u32, bool)>,
    /// How many `!` stood before the pipeline's first command.
````

with:

````rust
    redirects: Vec<Redirect<Word>>,
    /// A redirection waiting for its word.
    pending: Option<Pending>,
    /// How many `!` stood before the pipeline's first command.
````

Replace:

````rust
    compound: Option<Compound<Word>>,
}
````

with:

````rust
    compound: Option<Compound<Word>>,
}

/// A redirection operator waiting for its word.
#[derive(Clone, Copy)]
enum Pending {
    /// `>` or `>>` (`append`) on `fd`: a file name.
    File { fd: u32, append: bool },
    /// `>&` on `fd`, as typed (`2>&` or `>&`): the fd it copies.
    Copy { fd: u32, typed: &'static str },
}
````

Replace:

````rust
        match self.pending.take() {
            Some((fd, append)) => {
                let op = if append {
````

with:

````rust
        match self.pending.take() {
            Some(Pending::File { fd, append }) => {
                let op = if append {
````

Replace:

````rust
                self.redirects.push(Redirect { fd, op });
            }
````

with:

````rust
                self.redirects.push(Redirect { fd, op });
            }
            // Only a bare `1` or `2`: bash expands the word, and takes a
            // file, `-` or another fd too (§15 item 5).
            Some(Pending::Copy { fd, typed }) => {
                let copied = match w.digits() {
                    Some("1") => 1,
                    Some("2") => 2,
                    _ => return Err(ParseError::Unsupported(format!("{typed}{}", w.typed))),
                };
                self.redirects.push(Redirect {
                    fd,
                    op: RedirectOp::Copy(copied),
                });
            }
````

Replace:

````rust
                    let append = cur.next_if_eq('>');
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        let op = if append { ">>" } else { ">" };
                        let end = c.end();
                        return Err(ParseError::Unsupported(format!("{typed}{op} after {end}")));
                    }
                    // `>&2` and `>& f` send output elsewhere in bash; `>>&` and
                    // `> &` are its syntax errors.
                    if !append && cur.peek() == Some('&') {
                        return Err(ParseError::Unsupported(">&".into()));
                    }
                    self.parts.pending = Some((fd, append));
                }
````

with:

````rust
                    let append = cur.next_if_eq('>');
                    // `>&N` copies fd N; `>>&` and `> &` are bash's syntax
                    // errors (the `&` then meets a redirection without its
                    // word). bash's `>|` ignores `noclobber`.
                    let copy = !append && cur.next_if_eq('&');
                    if !append && !copy && cur.next_if_eq('|') {
                        return Err(ParseError::Unsupported(">|".into()));
                    }
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        let op = match (append, copy) {
                            (true, _) => ">>",
                            (_, true) => ">&",
                            _ => ">",
                        };
                        let end = c.end();
                        return Err(ParseError::Unsupported(format!("{typed}{op} after {end}")));
                    }
                    self.parts.pending = Some(if copy {
                        let typed = if typed == "2" { "2>&" } else { ">&" };
                        Pending::Copy { fd, typed }
                    } else {
                        Pending::File { fd, append }
                    });
                }
````

Replace:

````rust
                        Connector::And,
                    )?;
                }
                '&' => {
````

with:

````rust
                        Connector::And,
                    )?;
                }
                // bash's `&>` sends both outputs to a file.
                '&' if cur.peek() == Some('>') => {
                    return Err(ParseError::Unsupported("&>".into()));
                }
                '&' => {
````

Replace:

````rust
        let digits = String::from(digits);
        if self.parts.pending.is_some() {
            return Err(ParseError::Unexpected(digits));
        }
````

with:

````rust
        let digits = String::from(digits);
        match self.parts.pending {
            // The fd a `>&` copies (`2>&1>f`).
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. }) => return Err(ParseError::Unexpected(digits)),
            None => {}
        }
````

- [ ] **Step 8: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, replace:

````rust
    let out = to(parts.files, fds.0[1]);
    let err = to(parts.files, fds.0[2]);
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, out, err);
````

with:

````rust
    let out = to(parts.files, fds.0[1]);
    let err = match fds.0[2] {
        Slot::File(_) if fds.0[2] == fds.0[1] => To::Output,
        slot => to(parts.files, slot),
    };
    let mut ctx = Ctx::new(parts.vfs, parts.system, parts.console, out, err);
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 393 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): copy one output into the other with 2>&1 and 1>&2

`2>&1`, `1>&2` and `>&2` make one of fds 1 and 2 a copy of the other as
it is at that point (programmable shell gate §7.1), so `> f 2>&1` sends
both to `f` and `2>&1 > f` only the output. A blank may follow `>&`, as
bash allows. Any other word after `>&` (another fd, a file, `-`, a word
quoted or expanded) is refused, and so are bash's `&>`, which ran its
command in the background before, and `>|`. When fd 2 is a copy of fd
1's file, a built-in writes its errors through the output, in order.
EOF
````


### Task 7: A copy of an fd onto itself keeps its file

Decision 2 (the prototype's review, M-5): a copy holds its file before the slot it replaces lets it go, or `echo a > f 1>&1` would close `f` before the command writes it, and `/bin/sh` would reach the closed file's `unreachable!`; no test noticed the order. The test pins `> f 1>&1` and `2> e 2>&2` in both runners, which change nothing, as bash's do. A test only: it passes at once. Mutation check (1): the hold and the drop swapped fail it.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 6' `Files::redirect`.
- Produces: nothing new.

- [ ] **Step 1: Change the tests in `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn a_copy_of_an_fd_onto_itself_keeps_its_file() {
        // The prototype's review (M-5): a copy holds its file before the
        // slot it replaces lets it go, or `> f 1>&1` would close `f` before
        // the command writes it (bash 5.2: a copy onto itself changes
        // nothing).
        let mut h = Harness::new();
        assert_eq!(h.run("echo a > /tmp/f 1>&1"), (0, String::new()));
        assert_eq!(h.get("/tmp/f"), b"a\n");
        assert_eq!(h.run("ls /nope 2> /tmp/e 2>&2"), (2, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        let mut h = spawning();
        assert_eq!(h.spawning("t-args > /tmp/o 1>&1").0, 3);
        assert_eq!(h.programs.spawned[0].fds, [0, 4, 2]);
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 394 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): pin a copy of an fd onto itself

A copy holds its file before the slot it replaces lets it go, or `echo
a > f 1>&1` would close `f` before the command writes it, and `/bin/sh`
would reach the closed file's `unreachable!`; no test noticed the order
(the prototype's review, M-5). The test pins `> f 1>&1` and `2> e
2>&2` in both runners, as bash's change nothing.
EOF
````


### Task 8: `<` and `0<` read a file as standard input

Decision 2 and 7 (spec §7.1, §7.3): `< file` and `0< file` make the file a command's fd 0 (`RedirectOp::Read`): `/bin/sh` opens it (`Programs::open_input`) and hands it to the program, and the in-process runner reads it in pieces (`Ctx::set_input_file`). A missing file is bash's `relay-sh: f: No such file or directory`, status 1, and nothing runs; a directory is the command's own error (`cat: -: Is a directory`, `p9.txt`). Only a pipeline's first command may read a file (`unsupported syntax: < after |`); bash's other forms are refused (`1<`, `2<`, `<<`, `<<-`, `<<<`, `<&`, `<>`), and `<` in a `for`'s words is bash's syntax error, as `>` is. The parser dropped the redirections of a pipeline's earlier command (Task 1's slip, no form could show it before), now kept. The red run is the shell's tests. Mutation checks (10): `0<` taken for others, the here-document, here-string, `<&` and `<>` refusals, `<` after `|` (two places), the earlier command's redirections, bash's error after `<`, the read's offset, and the input file in a command and in a pipeline's first, each broken, fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/expand.rs`
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 7's parser and fds.
- Produces: `parser::RedirectOp::Read`; `Programs::open_input(path)`; `FakePrograms::inputs`; `Ctx::set_input_file`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        // Even where a file name is awaited (bash 5.2, probes/p8.txt).
        for line in ["echo a >2>f", "echo a 2>2>f"] {
            assert_eq!(
````

with:

````rust
        // Even where a file name is awaited (bash 5.2, probes/p8.txt).
        for line in ["echo a >2>f", "echo a 2>2>f", "cat <2>f"] {
            assert_eq!(
````

Replace:

````rust
        }
        // bash's syntax errors.
        for (line, token) in [
            ("echo a 2>>&1", "&"),
````

with:

````rust
        }
        // `<` and `0<` read a file as fd 0 (bash 5.2, probes/p9.txt).
        let read = |path: &str| Redirect {
            fd: 0,
            op: RedirectOp::Read(path.into()),
        };
        for line in ["cat < f", "cat 0< f", "cat <f", "<f cat", "cat 0<f"] {
            let c = one(line).unwrap();
            assert_eq!(
                (c.words, c.redirects),
                (words("cat"), alloc::vec![read("f")]),
                "{line}"
            );
        }
        assert_eq!(parse("cat < f | wc -l").unwrap()[0].redirects, [read("f")]);
        for (line, refused) in [
            ("cat 1< f", "1<"),
            ("cat 2< f", "2<"),
            ("cat 00< f", "00<"),
            ("cat << EOF", "<<"),
            ("cat <<-EOF", "<<"),
            ("cat <<< x", "<<<"),
            ("cat <&0", "<&"),
            ("cat <> f", "<>"),
            // Only a pipeline's first command reads a file (§7.3).
            ("cat | cat < f", "< after |"),
            ("cat | cat < f | wc", "< after |"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(refused.into())),
                "{line}"
            );
        }
        // bash's syntax errors.
        for (line, token) in [
            ("cat < < f", "<"),
            ("cat <", "newline"),
            ("for x in a < b; do echo; done", "<"),
            ("echo a 2>>&1", "&"),
````

Replace:

````rust
            ("ls file?", '?'),
            ("cat < f", '<'),
            ("echo `x`", '`'),
````

with:

````rust
            ("ls file?", '?'),
            ("echo `x`", '`'),
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn a_redirection_that_cannot_open_stops_the_command() {
````

with:

````rust
    #[test]
    fn standard_input_may_be_a_file() {
        // bash 5.2 (tmp/m5p1/probes/p9.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"l1\nl2\n");
        assert_eq!(h.run("cat < /tmp/f"), (0, "l1\nl2\n".into()));
        assert_eq!(h.run("wc -l 0</tmp/f"), (0, "2\n".into()));
        assert_eq!(h.run("cat < /tmp/f | wc -l"), (0, "2\n".into()));
        // Read whole, a piece after the other.
        h.put("/tmp/big", &alloc::vec![b'x'; 10_000]);
        assert_eq!(h.run("wc -c < /tmp/big"), (0, "10000\n".into()));
        assert_eq!(
            h.run("cat < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("cat 2> /tmp/e < /nope"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/e"),
            b"relay-sh: /nope: No such file or directory\n"
        );
        assert_eq!(h.run("cat < /tmp"), (1, "cat: -: Is a directory\n".into()));
        // Alone it opens the file and runs nothing.
        assert_eq!(h.run("< /tmp/f"), (0, String::new()));
        assert_eq!(
            h.run("< /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        // Under /bin/sh the program gets the file as fd 0, a pipeline's
        // first command too.
        let mut h = spawning();
        h.programs.known.push(("/bin/cat", WaitStatus::exited(0)));
        assert_eq!(h.spawning("cat < /tmp/f").0, 0);
        assert_eq!(h.programs.inputs, [("/tmp/f".into(), 4)]);
        assert_eq!(h.programs.spawned[0].fds, [4, 1, 2]);
        assert_eq!(h.spawning("cat < /tmp/g | t-args").0, 3);
        assert_eq!(h.programs.pipes, [(6, 7)]);
        assert_eq!(h.programs.spawned[1].fds, [5, 7, 2]);
        assert_eq!(h.programs.spawned[2].fds, [6, 1, 2]);
        // The pipe's ends as each stage has them, the files after the
        // pipeline.
        assert_eq!(h.programs.closed, [4, 7, 6, 5]);
        h.programs.open_error = Some(vfs::Errno::ENOENT);
        assert_eq!(
            h.spawning("cat < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 3);
    }

    #[test]
    fn a_redirection_that_cannot_open_stops_the_command() {
````

- [ ] **Step 3: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub refusals: Vec<(&'static str, Errno)>,
    /// Every redirection opened: its path, whether it appends, its fd.
    pub opened: Vec<(String, bool, u32)>,
    /// What `open_output` fails with, if anything.
````

with:

````rust
    pub refusals: Vec<(&'static str, Errno)>,
    /// Every redirection opened for output: its path, whether it appends,
    /// its fd.
    pub opened: Vec<(String, bool, u32)>,
    /// Every redirection opened for input: its path and fd.
    pub inputs: Vec<(String, u32)>,
    /// What `open_output` fails with, if anything.
````

Replace:

````rust
            opened: Vec::new(),
            open_error: None,
````

with:

````rust
            opened: Vec::new(),
            inputs: Vec::new(),
            open_error: None,
````

Replace:

````rust
        self.opened.push((path, append, self.next_fd));
        Ok(self.next_fd)
````

with:

````rust
        self.opened.push((path, append, self.next_fd));
        Ok(self.next_fd)
    }
    fn open_input(&mut self, path: &[u8]) -> Result<u32, Errno> {
        if let Some(e) = self.open_error {
            return Err(e);
        }
        self.next_fd += 1;
        let path = String::from_utf8_lossy(path).into_owned();
        self.inputs.push((path, self.next_fd));
        Ok(self.next_fd)
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `open_input` is not a member of trait `Programs` ``; `` no variant, associated function, or constant named `Read` found for enum `parser::RedirectOp<W>` in the current scope ``.

- [ ] **Step 5: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
use relay_abi::console::{MODE_LINE, MODE_RAW};
use relay_abi::file::{KIND_CHAR_DEVICE, OPEN_APPEND, OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
use relay_abi::info::LOG_MAX;
````

with:

````rust
use relay_abi::console::{MODE_LINE, MODE_RAW};
use relay_abi::file::{
    KIND_CHAR_DEVICE, OPEN_APPEND, OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
};
use relay_abi::info::LOG_MAX;
````

Replace:

````rust

    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
````

with:

````rust

    fn open_input(&mut self, path: &[u8]) -> Result<u32, Errno> {
        sys::open(path, OPEN_READ).map_err(Errno::from_number)
    }

    fn write(&mut self, fd: u32, bytes: &[u8]) -> Result<(), Errno> {
````

- [ ] **Step 6: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    input: Option<&'a mut dyn Stdin>,
    /// The exit status when standard output could not be written (1, as
````

with:

````rust
    input: Option<&'a mut dyn Stdin>,
    /// Standard input that is a file the in-process runner opened (`<`),
    /// and where the next read starts; it comes before `input`.
    input_file: Option<(Node, u64)>,
    /// The exit status when standard output could not be written (1, as
````

Replace:

````rust
            input: None,
            write_error_status: 1,
````

with:

````rust
            input: None,
            input_file: None,
            write_error_status: 1,
````

Replace:

````rust

    /// Reads standard input into `buf`: how many bytes, 0 at its end. What
````

with:

````rust

    /// Gives the command the file `node` as standard input, from its start.
    pub(crate) fn set_input_file(&mut self, node: Node) {
        self.input_file = Some((node, 0));
    }

    /// Reads standard input into `buf`: how many bytes, 0 at its end. What
````

Replace:

````rust
        self.streams().flush();
        match &mut self.input {
````

with:

````rust
        self.streams().flush();
        if let Some((node, offset)) = &mut self.input_file {
            let n = self.vfs.read_at(*node, *offset, buf)?;
            *offset += n as u64;
            return Ok(n);
        }
        match &mut self.input {
````

- [ ] **Step 7: Change `crates/shell/src/expand.rs`**

In `crates/shell/src/expand.rs`, replace:

````rust
        let op = match &r.op {
            RedirectOp::Write(path) => RedirectOp::Write(self.target(path)?),
````

with:

````rust
        let op = match &r.op {
            RedirectOp::Read(path) => RedirectOp::Read(self.target(path)?),
            RedirectOp::Write(path) => RedirectOp::Write(self.target(path)?),
````

- [ ] **Step 8: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
    Fd(u32),
    /// The in-process runner's: the file, and where the next write goes
    /// ([`AT_END`] for one opened with `>>`).
    Node { node: Node, offset: u64 },
````

with:

````rust
    Fd(u32),
    /// The in-process runner's: the file, and where the next read or
    /// write goes ([`AT_END`] for one opened with `>>`).
    Node { node: Node, offset: u64 },
````

Replace:

````rust
impl Opener<'_> {
    fn open(&mut self, path: &str, append: bool) -> Result<Handle, Errno> {
````

with:

````rust
impl Opener<'_> {
    /// Opens `path` for reading.
    fn open_input(&mut self, path: &str) -> Result<Handle, Errno> {
        if let Some(programs) = self.programs.as_deref_mut() {
            return programs.open_input(path.as_bytes()).map(Handle::Fd);
        }
        let node = self.vfs.lookup(path.as_bytes())?;
        Ok(Handle::Node { node, offset: 0 })
    }

    /// Opens `path` for output, emptied or, with `append`, at its end.
    fn open(&mut self, path: &str, append: bool) -> Result<Handle, Errno> {
````

Replace:

````rust
        for r in redirects {
            let (path, append) = match &r.op {
                RedirectOp::Write(path) => (path, false),
                RedirectOp::Append(path) => (path, true),
                // The other fd as it is now, held once more.
````

with:

````rust
        for r in redirects {
            let (path, opened) = match &r.op {
                RedirectOp::Read(path) => (path, opener.open_input(path)),
                RedirectOp::Write(path) => (path, opener.open(path, false)),
                RedirectOp::Append(path) => (path, opener.open(path, true)),
                // The other fd as it is now, held once more.
````

Replace:

````rust
            };
            let slot = match opener.open(path, append) {
                Ok(handle) => Slot::File(self.add(handle)),
````

with:

````rust
            };
            let slot = match opened {
                Ok(handle) => Slot::File(self.add(handle)),
````

- [ ] **Step 9: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    /// Writes all of `bytes` to the shell's fd `fd`, a file it opened (a
````

with:

````rust
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno>;
    /// Opens a redirection source for reading; its fd.
    fn open_input(&mut self, path: &[u8]) -> Result<u32, Errno>;
    /// Writes all of `bytes` to the shell's fd `fd`, a file it opened (a
````

- [ ] **Step 10: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 13 replacements, top to bottom:

Replace:

````rust
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. `2>&1`, `1>&2` and
//! `>&2` make one of them a copy of the other as it is at that point; any
//! other word after `>&` is refused, as are bash's `&>` and `>|`. An
//! unquoted `~` alone, or before `/` in the same unquoted
//! piece, at the start of a word means `/root`, as in Linux. An unquoted
//! `#` at the start of a word begins a comment, which runs to the end of
//! the line. An unquoted `|` joins commands into a pipeline (user-space
//! gate §9.1); each has a name, only the last may redirect its output, and
//! bash's syntax errors name a `|` with no command before it or none after.
````

with:

````rust
//! them per command, made left to right; a word of digits just before the
//! operator is its fd, and only 1 and 2 are taken. `2>&1`, `1>&2` and `>&2`
//! make one of them a copy of the other as it is at that point; any other
//! word after `>&` is refused, as are bash's `&>` and `>|`. `< file` and
//! `0< file` read the file as standard input; bash's other fds,
//! here-documents, `<&` and `<>` are refused. An unquoted `~` alone, or
//! before `/` in the same unquoted piece, at the start of a word means
//! `/root`, as in Linux. An unquoted `#` at the start of a word begins a
//! comment, which runs to the end of the line. An unquoted `|` joins
//! commands into a pipeline (user-space gate §9.1); each has a name, only
//! the last may redirect its output and only the first its input, and
//! bash's syntax errors name a `|` with no command before it or none after.
````

Replace:

````rust
//! stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `<`,
//! `` ` ``, `(` or `)` is an error naming the character, instead of being
//! passed on as if it were plain text; so are `|&` (the errors into the
//! pipe too) and `>&`.

````

with:

````rust
//! stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `` ` ``, `(`
//! or `)` is an error naming the character, instead of being passed on as
//! if it were plain text; so is `|&` (the errors into the pipe too).

````

Replace:

````rust
pub enum RedirectOp<W = String> {
    /// `> path`: the file, created or emptied, written from its start.
````

with:

````rust
pub enum RedirectOp<W = String> {
    /// `< path`: the file, read from its start.
    Read(W),
    /// `> path`: the file, created or emptied, written from its start.
````

Replace:

````rust

const UNSUPPORTED: &[char] = &['*', '?', '<', '`', '(', ')'];

````

with:

````rust

const UNSUPPORTED: &[char] = &['*', '?', '`', '(', ')'];

````

Replace:

````rust
enum Pending {
    /// `>` or `>>` (`append`) on `fd`: a file name.
````

with:

````rust
enum Pending {
    /// `<` on fd 0: a file name.
    Read,
    /// `>` or `>>` (`append`) on `fd`: a file name.
````

Replace:

````rust
        match self.pending.take() {
            Some(Pending::File { fd, append }) => {
````

with:

````rust
        match self.pending.take() {
            Some(Pending::Read) => self.redirects.push(Redirect {
                fd: 0,
                op: RedirectOp::Read(w),
            }),
            Some(Pending::File { fd, append }) => {
````

Replace:

````rust
        }
        if !self.redirects.is_empty() {
            return Err(ParseError::Unsupported("> before |".into()));
````

with:

````rust
        }
        // `<` may stand on the first command only (programmable shell gate
        // §7.3).
        if self.later && self.redirects.iter().any(|r| r.fd == 0) {
            return Err(ParseError::Unsupported("< after |".into()));
        }
        if self.words.is_empty() || self.redirects.iter().any(|r| r.fd != 0) {
            return Err(ParseError::Unsupported("> before |".into()));
````

Replace:

````rust
        self.later = true;
        command(p.words, Vec::new())
    }
````

with:

````rust
        self.later = true;
        command(p.words, p.redirects)
    }
````

Replace:

````rust
    let p = core::mem::take(parts);
    pipeline.push(command(p.words, p.redirects)?);
````

with:

````rust
    let p = core::mem::take(parts);
    if !pipeline.is_empty() && p.redirects.iter().any(|r| r.fd == 0) {
        return Err(ParseError::Unsupported("< after |".into()));
    }
    pipeline.push(command(p.words, p.redirects)?);
````

Replace:

````rust
            // word may open or end, the operator is the header's.
            if matches!(c, '>' | '|' | ';' | '&' | '\n') {
                if c != '>' || !(self.word.started && self.word.word.digits().is_some()) {
                    self.end_word(line, at)?;
````

with:

````rust
            // word may open or end, the operator is the header's.
            if matches!(c, '>' | '<' | '|' | ';' | '&' | '\n') {
                let redirection = matches!(c, '>' | '<');
                if !redirection || !(self.word.started && self.word.word.digits().is_some()) {
                    self.end_word(line, at)?;
````

Replace:

````rust
                        Connector::And,
                    )?;
                }
                // bash's `&>` sends both outputs to a file.
````

with:

````rust
                        Connector::And,
                    )?;
                }
                '<' => {
                    // As at `>`: `0<` is fd 0; bash's `1<`, `2<` and others
                    // are refused, as are its here-documents, `<&` and `<>`.
                    if let Some(digits) = self.fd_word()?
                        && digits != "0"
                    {
                        return Err(ParseError::Unsupported(format!("{digits}<")));
                    }
                    self.end_word(line, at)?;
                    if self.parts.pending.is_some() {
                        return Err(ParseError::MissingTarget("<"));
                    }
                    if cur.next_if_eq('<') {
                        let op = if cur.next_if_eq('<') { "<<<" } else { "<<" };
                        return Err(ParseError::Unsupported(op.into()));
                    }
                    for (c, op) in [('&', "<&"), ('>', "<>")] {
                        if cur.next_if_eq(c) {
                            return Err(ParseError::Unsupported(op.into()));
                        }
                    }
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        return Err(ParseError::Unsupported(format!("< after {}", c.end())));
                    }
                    self.parts.pending = Some(Pending::Read);
                }
                // bash's `&>` sends both outputs to a file.
````

Replace:

````rust
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. }) => return Err(ParseError::Unexpected(digits)),
            None => {}
````

with:

````rust
            Some(Pending::Copy { .. }) => return Ok(None),
            Some(Pending::File { .. } | Pending::Read) => {
                return Err(ParseError::Unexpected(digits));
            }
            None => {}
````

Replace:

````rust
            '>' if cur.next_if_eq('>') => ">>",
            '|' => "|",
            '&' => "&",
            _ => ">",
````

with:

````rust
            '>' if cur.next_if_eq('>') => ">>",
            '<' if cur.next_if_eq('<') => "<<",
            '|' => "|",
            '&' => "&",
            '<' => "<",
            _ => ">",
````

- [ ] **Step 11: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
                    (None, None) => {}
                }
````

with:

````rust
                    (None, None) => {}
                }
                // The first stage's `<`.
                if let Some(node) = input_file(files, stage.fds.0[0]) {
                    ctx.set_input_file(node);
                }
````

Replace:

````rust

/// Where a command the shell runs itself writes for `slot`.
````

with:

````rust

/// The in-process runner's file `slot` reads, if it is one.
fn input_file(files: &Files, slot: Slot) -> Option<Node> {
    match slot {
        Slot::File(i) => match files.handle(i) {
            Handle::Node { node, .. } => Some(node),
            Handle::Fd(_) => None,
        },
        Slot::Shell(_) => None,
    }
}

/// Where a command the shell runs itself writes for `slot`.
````

Replace:

````rust
        ctx.set_input(input);
    }
````

with:

````rust
        ctx.set_input(input);
    }
    if let Some(node) = input_file(parts.files, fds.0[0]) {
        ctx.set_input_file(node);
    }
````

- [ ] **Step 12: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 395 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): read standard input from a file with <

`< file` and `0< file` make the file a command's fd 0 (programmable
shell gate §7.1): `/bin/sh` opens it (`Programs::open_input`) and hands
it to the program, and the in-process runner reads it as the command's
input. A missing file is bash's `relay-sh: f: No such file or
directory`, status 1, and nothing runs. Only a pipeline's first command
may read a file (§7.3), and bash's other forms (`1<`, here-documents,
here-strings, `<&`, `<>`) are refused. A pipeline's earlier command now
keeps its redirections, which the parser dropped.
EOF
````


### Task 9: `cat` and `grep` refuse an input that is the output, as GNU's do

Decision 9 (the prototype's review, I-1, and the user's choice): with Task 8's `<`, `cat < f >> f` and `grep x < f >> f` read their own output until the disk was full, where GNU refuses them. `cat` now takes GNU's condition for standard input and file operands alike: the output is the input's regular file and the input has bytes left to read (`cat: -: input file is output file`, status 1), so `cat f > f`, whose `>` has emptied `f`, is silent with status 0, as GNU's is, where milestone 1 refused it; `cat f >> f` is still refused (check 4's line). `grep` refuses its standard input as it does a file that is the output, whatever is left in it, and, as GNU's, not under `-c`, which writes no lines either (`grep: (standard input): input file is also the output`, status 2; GNU 9.4 and grep 3.11, `tmp/m5p1/probes/p15.txt`). A program learns its input's file from `Stdin::file` (relay-rt: `fstat` and `seek` of fd 0) and names only a regular file as its output (`SysStdout::node`), as GNU checks only a regular output. `fileops` runs both under `/bin/sh`. The red run is the shell's tests. Mutation checks (8): `cat`'s check of its input, its `<` for `<=`, the operand's size, `grep`'s check of its input, `-c` not skipping it, `Ctx`'s input file, and relay-rt's `file` and regular output (the last two through `fileops`), each fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/grep.rs`
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/program.rs`
- Modify: `tests/e2e/fileops.txt`

**Interfaces:**
- Consumes: Task 8's input file; `Ctx::output_node`.
- Produces: `Stdin::file() -> Option<(Node, u64, u64)>`; `Ctx::input_file()`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, replace:

````rust
        );
        // Another file into it is fine.
````

with:

````rust
        );
        // Standard input too, whatever is left in it, unless no line is
        // written (`-q`, `-c`), as GNU's (the prototype's review, I-1; GNU
        // grep 3.11, tmp/m5p1/probes/p15.txt).
        h.put("/tmp/s", b"1\n");
        let said = "grep: (standard input): input file is also the output\n";
        assert_eq!(h.run("grep 1 < /tmp/s >> /tmp/s"), (2, said.into()));
        assert_eq!(h.get("/tmp/s"), b"1\n");
        assert_eq!(h.run("grep 1 < /tmp/s > /tmp/s"), (2, said.into()));
        h.put("/tmp/s", b"1\n");
        assert_eq!(h.run("grep -c 1 < /tmp/s >> /tmp/s"), (0, String::new()));
        assert_eq!(h.run("grep -c 1 /tmp/s >> /tmp/s"), (0, String::new()));
        assert_eq!(h.get("/tmp/s"), b"1\n1\n2\n");
        assert_eq!(h.run("grep -q 1 < /tmp/s >> /tmp/s"), (0, String::new()));
        // Another file into it is fine.
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
        assert_eq!(h.get("/tmp/a"), b"one\n");
    }
````

with:

````rust
        assert_eq!(h.get("/tmp/a"), b"one\n");
        // GNU's condition, standard input too (the prototype's review,
        // I-1; GNU 9.4, tmp/m5p1/probes/p15.txt): the same regular file,
        // with something left to read; `>` has emptied it.
        assert_eq!(
            h.run("cat < /tmp/a >> /tmp/a"),
            (1, "cat: -: input file is output file\n".into())
        );
        assert_eq!(h.get("/tmp/a"), b"one\n");
        assert_eq!(
            h.run("cat - /tmp/a < /tmp/a >> /tmp/a"),
            (
                1,
                "cat: -: input file is output file\ncat: /tmp/a: input file is output file\n"
                    .into()
            )
        );
        assert_eq!(h.run("cat < /tmp/a > /tmp/a"), (0, String::new()));
        assert_eq!(h.get("/tmp/a"), b"");
        h.put("/tmp/a", b"one\n");
        assert_eq!(h.run("cat /tmp/a > /tmp/a"), (0, String::new()));
        assert_eq!(h.get("/tmp/a"), b"");
    }
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/program.rs`**

In `crates/shell/src/program.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    use alloc::string::String;
    use vfs::{Errno, Vfs};

````

with:

````rust
    use alloc::string::String;
    use vfs::{Errno, Node, Vfs};

````

Replace:

````rust
        );
    }
````

with:

````rust
        );
        // Its fd 0 that file too, read up to some point (`cat < f >> f`
        // under `/bin/sh`).
        struct File(Node, u64);
        impl crate::Stdin for File {
            fn read(&mut self, _: &mut [u8]) -> Result<usize, Errno> {
                unreachable!("refused before it reads")
            }
            fn file(&mut self) -> Option<(Node, u64, u64)> {
                Some((self.0, self.1, 2))
            }
        }
        let cat = crate::commands::find("cat").unwrap().run;
        let mut out = FakeStdout::file(Some(node));
        let io = crate::CommandIo {
            vfs: &mut h.vfs,
            console: &mut h.console,
            system: &mut h.system,
            stdin: &mut File(node, 1),
            stdout: &mut out,
        };
        assert_eq!(crate::run_command("cat", cat, &[], io), 1);
        assert_eq!(h.console.take(), "cat: -: input file is output file\n");
    }
````

- [ ] **Step 4: Expect the new lines in `tests/e2e/fileops.txt`**

In `tests/e2e/fileops.txt`, replace:

````text
expect root@relay:~# $
poweroff
````

with:

````text
expect root@relay:~# $
# A program whose input is its output refuses it, as GNU's does, rather
# than read what it writes until the disk is full.
send echo abc > io; cat < io >> io; echo "status $?"; grep a < io >> io; echo "status $?"; wc -c io
expect \ncat: -: input file is output file\nstatus 1\ngrep: \(standard input\): input file is also the output\nstatus 2\n4 io\n
expect root@relay:~# $
poweroff
````

- [ ] **Step 5: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `file` is not a member of trait `crate::Stdin` ``.

- [ ] **Step 6: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
use relay_abi::file::{
    KIND_CHAR_DEVICE, OPEN_APPEND, OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
};
````

with:

````rust
use relay_abi::file::{
    KIND_CHAR_DEVICE, KIND_REGULAR, OPEN_APPEND, OPEN_CREATE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE,
    SEEK_CURRENT,
};
````

Replace:

````rust
        sys::read(0, buf).map_err(Errno::from_number)
    }
}

````

with:

````rust
        sys::read(0, buf).map_err(Errno::from_number)
    }

    fn file(&mut self) -> Option<(Node, u64, u64)> {
        let st = sys::fstat(0)
            .ok()
            .filter(|st| st.kind == u32::from(KIND_REGULAR))?;
        let at = sys::seek(0, 0, SEEK_CURRENT).ok()?;
        Some((node_of(&st), at, st.size))
    }
}

````

Replace:

````rust
            .ok()
            .filter(|st| st.dev != 0)
            .map(|st| node_of(&st))
````

with:

````rust
            .ok()
            .filter(|st| st.kind == u32::from(KIND_REGULAR))
            .map(|st| node_of(&st))
````

- [ ] **Step 7: Change `crates/shell/src/commands/grep.rs`**

In `crates/shell/src/commands/grep.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    let name = file.map_or("(standard input)", String::as_str);
    let mut source = match file {
        Some(path) => match open(ctx, path) {
            // It would read what it wrote, for ever (`grep x f >> f`);
            // `-q` writes nothing.
            Ok(node) if ctx.output_node() == Some(node) && !options.quiet => {
                ctx.fail(
````

with:

````rust
    let name = file.map_or("(standard input)", String::as_str);
    // It would read what it wrote, for ever (`grep x f >> f`): GNU refuses
    // a file that is the output, whatever is left in it, unless it writes
    // no lines (`-q`, `-c`).
    let checks = !options.quiet && !options.count;
    let mut source = match file {
        Some(path) => match open(ctx, path) {
            Ok(node) if checks && ctx.output_node() == Some(node) => {
                ctx.fail(
````

Replace:

````rust
        },
        None => Source::Input,
    };
````

with:

````rust
        },
        None => {
            let input = ctx.input_file().map(|(node, ..)| node);
            if checks && input.is_some() && input == ctx.output_node() {
                ctx.fail(
                    "grep",
                    format_args!("(standard input): input file is also the output"),
                );
                return Outcome::Failed;
            }
            Source::Input
        }
    };
````

- [ ] **Step 8: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        };
        // `cat f >> f` would read its own output forever.
        if ctx.output_node() == Some(node) {
            status = ctx.fail("cat", format_args!("{name}: input file is output file"));
````

with:

````rust
        };
        // `cat f >> f` would read its own output forever: GNU's check, the
        // output the same regular file and something left to read in it
        // (`cat f > f` has emptied `f` and reads nothing).
        if ctx.output_node() == Some(node) && ctx.vfs.stat(node).is_ok_and(|st| st.size > 0) {
            status = ctx.fail("cat", format_args!("{name}: input file is output file"));
````

Replace:

````rust
/// `cat` of standard input, to its end, Ctrl-C or a write error (which the
/// shell reports).
fn cat_input(ctx: &mut Ctx<'_>) -> i32 {
    let mut buf = vec![0; CHUNK];
````

with:

````rust
/// `cat` of standard input, to its end, Ctrl-C or a write error (which the
/// shell reports). A file that is the output, with bytes left to read, is
/// refused, as for an operand (`cat < f >> f`).
fn cat_input(ctx: &mut Ctx<'_>) -> i32 {
    if let Some((node, at, size)) = ctx.input_file()
        && ctx.output_node() == Some(node)
        && at < size
    {
        return ctx.fail("cat", format_args!("-: input file is output file"));
    }
    let mut buf = vec![0; CHUNK];
````

- [ ] **Step 9: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust
        self.input_file = Some((node, 0));
    }
````

with:

````rust
        self.input_file = Some((node, 0));
    }

    /// The regular file standard input is, where it has been read up to and
    /// its size, if it is one.
    pub fn input_file(&mut self) -> Option<(Node, u64, u64)> {
        if let Some((node, offset)) = self.input_file {
            let size = self.vfs.stat(node).ok()?.size;
            return Some((node, offset, size));
        }
        self.input.as_mut()?.file()
    }
````

- [ ] **Step 10: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
}
````

with:

````rust
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
    /// The regular file it is, as the `Vfs` names files, where it has been
    /// read up to and its size (`cat < f >> f` must not read its own
    /// output); none for a pipe, the console or bytes in memory (the
    /// default).
    fn file(&mut self) -> Option<(Node, u64, u64)> {
        None
    }
}
````

- [ ] **Step 11: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 395 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 12: Run the `fileops` scenario**

Run: `cargo xtask test --e2e-only --scenario fileops`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 13: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 14: Commit**

````bash
git add crates tests
git commit -F - <<'EOF'
fix(shell,relay-rt): refuse an input that is the output, as GNU does

With `<`, `cat < f >> f` and `grep x < f >> f` read their own output
until the disk was full, where GNU refuses them (the prototype's
review, I-1). `cat` now takes GNU's condition for standard input and
file operands alike: the output is the input's regular file and the
input has bytes left to read (`cat: -: input file is output file`), so
`cat f > f`, whose `>` has emptied `f`, is silent with status 0, as
GNU's is, where milestone 1 refused it. `grep` refuses its standard
input as it does a file that is the output, and, as GNU's, neither
under `-c`, which writes no lines either. A program learns its input's
file from `Stdin::file`; relay-rt names only a regular file.
EOF
````


### Task 10: `sh FILE` refuses a redirected fd 2, and reads a file given as input

Decision 5 (the user's choice): `sh FILE` refuses a redirected output, since what goes to a file misses the script's transcript, a copy of the screen; its errors hold its trace (the spike's `sh x.sh 2> se` wrote its trace into `se`), so it now refuses a redirected fd 2 too, with the same message, written where its errors go. A program learns it from its console (`Console::is_screen`: relay-rt's asks whether fd 2 is the console; `TestConsole::redirected`), and `Ctx::err_is_tty` combines it with the error target. Its input may be a file: in the in-process runner the script runs inside the `sh` command's context, so its commands read the file; under `/bin/sh` the child shell's fd 0 is the file already. The red run is the shell's tests. Mutation checks (4): the fd 2 refusal, the console's answer, the error target's, and the script's context, each broken, fail a test.

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`
- Modify: `crates/shell/src/shell.rs`
- Modify: `crates/shell/src/testing.rs`

**Interfaces:**
- Consumes: Task 9's input file; `commands::script::sh`.
- Produces: `Console::is_screen() -> bool` (default true); `Ctx::err_is_tty`; `TestConsole::redirected`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
mod tests {
    use crate::testing::Harness;
    use alloc::string::String;
````

with:

````rust
mod tests {
    use crate::testing::{FakeStdout, Harness};
    use alloc::string::String;
````

Replace:

````rust
    #[test]
    fn sh_errors() {
````

with:

````rust
    #[test]
    fn a_script_s_input_may_be_a_file() {
        // Its commands read it (programmable shell gate §15 item 5).
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cat\n");
        h.put("/tmp/in", b"one\ntwo\n");
        assert_eq!(
            h.run("sh /tmp/s.sh < /tmp/in"),
            (0, "+ cat\none\ntwo\n".into())
        );
    }

    #[test]
    fn sh_errors() {
````

Replace:

````rust
        );
        h.put("/tmp/bin.sh", b"echo \xff\n");
````

with:

````rust
        );
        // Nor its errors, which hold its trace (programmable shell gate
        // §15 item 5): the message goes where they would.
        assert_eq!(run(&mut h, "sh /tmp/s.sh 2> /tmp/err"), (1, String::new()));
        assert_eq!(
            h.get("/tmp/err"),
            b"sh: a script's output cannot be redirected\n"
        );
        assert_eq!(
            run(&mut h, "sh /tmp/s.sh > /tmp/out 2>&1"),
            (1, String::new())
        );
        assert_eq!(
            h.get("/tmp/out"),
            b"sh: a script's output cannot be redirected\n"
        );
        // As a program, `/bin/sh FILE` whose fd 2 is not the console.
        h.put("/tmp/b.sh", b"cd /tmp\n");
        let mut out = FakeStdout::console();
        h.console.redirected = true;
        assert_eq!(
            h.sh(&["/tmp/b.sh"], &mut out),
            (1, "sh: a script's output cannot be redirected\n".into())
        );
        h.console.redirected = false;
        assert_eq!(h.sh(&["/tmp/b.sh"], &mut out), (0, "+ cd /tmp\n".into()));
        h.put("/tmp/bin.sh", b"echo \xff\n");
````

- [ ] **Step 2: Extend the test support in `crates/shell/src/testing.rs`**

In `crates/shell/src/testing.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
    pub taken_back: usize,
}
````

with:

````rust
    pub taken_back: usize,
    /// What is written goes elsewhere than the screen (a program's fd 2
    /// sent to a file).
    pub redirected: bool,
}
````

Replace:

````rust
            taken_back: 0,
        }
````

with:

````rust
            taken_back: 0,
            redirected: false,
        }
````

Replace:

````rust
        self.columns
    }
````

with:

````rust
        self.columns
    }
    fn is_screen(&self) -> bool {
        !self.redirected
    }
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `is_screen` is not a member of trait `Console` ``.

- [ ] **Step 4: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        sys::console_size().0 as usize
    }
````

with:

````rust
        sys::console_size().0 as usize
    }

    fn is_screen(&self) -> bool {
        sys::fstat(2).is_ok_and(|st| is_console(&st))
    }
````

- [ ] **Step 5: Change `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
    }
    if !ctx.is_tty() {
        return ctx.fail("sh", format_args!("a script's output cannot be redirected"));
````

with:

````rust
    }
    // Its trace and errors would miss its transcript, a copy of the
    // screen (programmable shell gate §15 item 5); its input may be a file.
    if !ctx.is_tty() || !ctx.err_is_tty() {
        return ctx.fail("sh", format_args!("a script's output cannot be redirected"));
````

- [ ] **Step 6: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

````rust
        self.console.columns()
    }
````

with:

````rust
        self.console.columns()
    }

    /// Whether errors go to the screen.
    pub fn err_is_tty(&self) -> bool {
        self.err == To::Console && self.console.is_screen()
    }
````

- [ ] **Step 7: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
    fn columns(&self) -> usize;
    /// Whether Ctrl-C was pressed while a command runs. Long commands ask
````

with:

````rust
    fn columns(&self) -> usize;
    /// Whether what `write` writes reaches the screen: a program's errors
    /// are its fd 2, which its shell may have sent to a file (`sh FILE 2>
    /// e`). The default is the screen.
    fn is_screen(&self) -> bool {
        true
    }
    /// Whether Ctrl-C was pressed while a command runs. Long commands ask
````

- [ ] **Step 8: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        }
        self.release(fds);
        self.stopped = ran.stop;
        self.exited = ran.exited;
        self.cancelled |= ran.cancelled;
        let mut status = ran.status;
        if let Some(script) = ran.script {
            status = self.run_script(*script);
        }
        self.finish(status, message)
````

with:

````rust
        }
        self.stopped = ran.stop;
        self.exited = ran.exited;
        self.cancelled |= ran.cancelled;
        let mut status = ran.status;
        // A script `sh` read runs with its redirections: its commands read
        // a file it was given as input.
        if let Some(script) = ran.script {
            let outer = core::mem::replace(&mut self.fds, fds);
            status = self.run_script(*script);
            self.fds = outer;
        }
        self.release(fds);
        self.finish(status, message)
````

- [ ] **Step 9: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 396 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 10: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 11: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): refuse to send a script's errors to a file

`sh FILE` refuses a redirected output, since what goes to a file misses
the script's transcript, a copy of the screen. Its errors hold its
trace, so it now refuses a redirected fd 2 too (programmable shell gate
§15 item 5), which a program learns from its console
(`Console::is_screen`: its fd 2 is the console). Its input may be a
file: the script's commands read it.
EOF
````


### Task 11: `head` leaves a file just after the lines it printed

Decision 6 (the user's choice): GNU's `head -n` seeks a standard input it can seek back to just after the lines it printed, so in `for …; do head -n 1; done < f` each pass reads the next line; `grep`, `wc`, `cat` and `tail` read to the end (`p5.txt`). `head` here read in pieces of 4 KiB and kept what it read past; it now seeks back over it (`Stdin::seek_back`, whose default answers `EINVAL`, the kernel's answer for a pipe or the console; relay-rt's seeks its fd 0 from where it is), and where it cannot the bytes are gone with no message, as with GNU's. The in-process file input seeks with the shared offsets of Task 18. The red run is the shell's tests. Mutation checks (3): the seek skipped, a byte too far, and `Ctx`'s seek not passed on, each fail a test; relay-rt's is checked through the `redirect` scenario (Task 21).

**Files:**
- Modify: `crates/relay-rt/src/sysio.rs`
- Modify: `crates/shell/src/commands/text.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/io.rs`

**Interfaces:**
- Consumes: `commands::text::head_input`.
- Produces: `Stdin::seek_back(n) -> Result<(), Errno>`; `Ctx::seek_input_back(n)`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, replace:

````rust
    #[test]
    fn tail_of_standard_input_joins_lines_a_pipe_cuts() {
````

with:

````rust
    #[test]
    fn head_leaves_a_file_just_after_its_lines() {
        // GNU's `head -n` seeks standard input back to just after the
        // lines it printed when it can (tmp/m5p1/probes/p5.txt), so the
        // next command reads on from there.
        struct File {
            data: &'static [u8],
            at: usize,
            seeks: bool,
        }
        impl crate::Stdin for File {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, vfs::Errno> {
                let n = (self.data.len() - self.at).min(buf.len());
                buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
                self.at += n;
                Ok(n)
            }
            fn seek_back(&mut self, n: u64) -> Result<(), vfs::Errno> {
                if !self.seeks {
                    return Err(vfs::Errno::EINVAL);
                }
                self.at -= n as usize;
                Ok(())
            }
        }
        let mut h = Harness::new();
        for (lines, seeks, out_text, at) in [
            ("1", true, "one\n", 4),
            ("2", true, "one\ntwo\n", 8),
            ("9", true, "one\ntwo\nthree", 13),
            // A pipe's bytes are gone, with no message.
            ("1", false, "one\n", 13),
        ] {
            let mut input = File {
                data: b"one\ntwo\nthree",
                at: 0,
                seeks,
            };
            let mut out = crate::testing::FakeStdout::file(None);
            let io = crate::CommandIo {
                vfs: &mut h.vfs,
                console: &mut h.console,
                system: &mut h.system,
                stdin: &mut input,
                stdout: &mut out,
            };
            let args = [String::from("-n"), String::from(lines)];
            assert_eq!(crate::run_command("head", super::head, &args, io), 0);
            assert_eq!((out.text().as_str(), input.at), (out_text, at), "{lines}");
        }
        assert_eq!(h.console.take(), "");
    }

    #[test]
    fn tail_of_standard_input_joins_lines_a_pipe_cuts() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: compile errors such as `` method `seek_back` is not a member of trait `crate::Stdin` ``.

- [ ] **Step 3: Change `crates/relay-rt/src/sysio.rs`**

In `crates/relay-rt/src/sysio.rs`, replace:

````rust
        Some((node_of(&st), at, st.size))
    }
````

with:

````rust
        Some((node_of(&st), at, st.size))
    }

    fn seek_back(&mut self, n: u64) -> Result<(), Errno> {
        let back = i64::try_from(n).map_err(|_| Errno::EINVAL)?;
        sys::seek(0, -back, SEEK_CURRENT)
            .map(|_| ())
            .map_err(Errno::from_number)
    }
````

- [ ] **Step 4: Change `crates/shell/src/commands/text.rs`**

In `crates/shell/src/commands/text.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// `head` of standard input: no more of it is read once the lines are out,
/// so a pipe's writer gets `EPIPE` once `head` has ended.
fn head_input(ctx: &mut Ctx<'_>, mut left: u64) -> i32 {
````

with:

````rust
/// `head` of standard input: no more of it is read once the lines are out,
/// so a pipe's writer gets `EPIPE` once `head` has ended. Input it can
/// seek (a file) it leaves just after its last line, as GNU's `head` does,
/// so that the next command reads on from there; elsewhere what it read
/// past its lines is lost, as with GNU's.
fn head_input(ctx: &mut Ctx<'_>, mut left: u64) -> i32 {
````

Replace:

````rust
                ctx.out(&buf[..end]);
            }
````

with:

````rust
                ctx.out(&buf[..end]);
                if end < n {
                    let _ = ctx.seek_input_back((n - end) as u64);
                }
            }
````

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, replace:

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
    }

    /// Moves standard input back `n` bytes, where it can be (a file), so
    /// that the next command reads them (GNU's `head`, programmable shell
    /// gate §15 item 5).
    pub fn seek_input_back(&mut self, n: u64) -> Result<(), Errno> {
        match &mut self.input {
            Some(input) => input.seek_back(n),
            None => Err(Errno::EINVAL),
        }
    }
````

- [ ] **Step 6: Change `crates/shell/src/io.rs`**

In `crates/shell/src/io.rs`, replace:

````rust
        None
    }
````

with:

````rust
        None
    }
    /// Moves back `n` bytes, so that what was read past is read again by
    /// whoever reads next: a file can, a pipe or the console cannot
    /// (`EINVAL`, the kernel's answer there, the default).
    fn seek_back(&mut self, _n: u64) -> Result<(), Errno> {
        Err(Errno::EINVAL)
    }
````

- [ ] **Step 7: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 397 tests.

Run: `cargo test -p relay-rt`

Expected: PASS: 29 tests.

- [ ] **Step 8: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 9: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell,relay-rt): leave a file just after the lines head printed

GNU's `head -n` seeks a standard input it can seek back to just after
the lines it printed, so in `for …; do head -n 1; done < f` each pass
reads the next line. `head` here read in pieces of 4 KiB and kept what
it read past; it now seeks back over it (`Stdin::seek_back`: `seek` on
`/bin`'s fd 0), and where it cannot (a pipe, the console) the bytes are
gone with no message, as with GNU's.
EOF
````


### Task 12: `help` names the new redirections

Decision 2: `help`'s syntax lines name `2> file`, `> file 2>&1` and `< file`; they take one line more (13), and the built-ins and the syntax still fit the NUC's 33 rows. No scenario or check script shows them. The red run is `help`'s test. Mutation check (1): the line about `<`, changed, fails the test.

**Files:**
- Modify: `crates/shell/src/commands/basic.rs`

**Interfaces:**
- Consumes: `commands::basic::help`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
            ">> file",
            "| ",
````

with:

````rust
            ">> file",
            "2> file",
            "2>&1",
            "< file",
            "| ",
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `commands::basic::tests::help_lists_every_command_with_its_description`.

- [ ] **Step 3: Change `crates/shell/src/commands/basic.rs`**

In `crates/shell/src/commands/basic.rs`, replace:

````rust
        "Send output to a file with `> file` (replace) or `>> file` (append),\n\
         or into another program with `| cmd` (built-ins cannot be in a\n\
         pipeline). End a command with `&` to run it in the background, or\n\
         with `;` to run the next one after it. `a && b` runs b if a\n\
         succeeds, `a || b` if it fails, and `! a` turns a's status round.\n\
         `sh FILE ARG...` runs a script, which reads its arguments as\n\
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
````

with:

````rust
        "Send output to a file with `> file` (replace) or `>> file` (append),\n\
         errors with `2> file` (both with `> file 2>&1`), or output into\n\
         another program with `| cmd` (built-ins cannot be in a pipeline);\n\
         `< file` reads a file as input. End a command with `&` to run it in\n\
         the background, or with `;` to run the next one after it. `a && b`\n\
         runs b if a succeeds, `a || b` if it fails, and `! a` turns a's status\n\
         round. `sh FILE ARG...` runs a script, which reads its arguments as\n\
         `$1`...`$9`, `$#` and \"$@\". `$?` is the last command's status, and\n\
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 397 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): show redirection of errors and input in help

`help`'s syntax lines name `2> file`, `> file 2>&1` and `< file`; they
take one line more, and the built-ins and the syntax still fit the
NUC's 33 rows.
EOF
````


### Task 13: The corpus runs in an empty directory and compares the order of redirections

Spec §11.2: each corpus script now starts in an empty directory of its own, bash's under the workspace's `target/corpus/` and the in-process runner's `/tmp/corpus`, so that it may make files by relative names; none did before. `redirect_order.sh` compares the order of several redirections with bash's: both orders of `2>&1`, `>&2` into a file, a copy made before its fd changes, `<`, `0<` and a line of only redirections. Its messages are GNU's programs', never bash's own, which name a script's line. A test only: it passes at once. Its worth: three mutants of the code (a copy's source fd, the shared output, the order of the redirections) each fail it.

**Files:**
- Modify: `crates/shell/src/corpus.rs`
- Create: `crates/shell/tests/corpus/redirect_order.sh`

**Interfaces:**
- Consumes: `corpus::{bash, relay}`.
- Produces: `crates/shell/tests/corpus/redirect_order.sh`.

- [ ] **Step 1: Change `crates/shell/src/corpus.rs`**

In `crates/shell/src/corpus.rs`, make these 4 replacements, top to bottom:

Replace:

````rust
//! runner, read as `X | sh` reads its input (no trace), and must print the
//! same and end with the same status. A script writes nothing to standard
//! error under bash (messages are tested against bash apart, as bash names
//! a script's line in them) and holds no unquoted value with a blank in it
//! (expansion never splits words here). A missing bash fails the test.
#![cfg(test)]
````

with:

````rust
//! runner, read as `X | sh` reads its input (no trace), and must print the
//! same and end with the same status. Each starts in an empty directory of
//! its own, where it may make files by relative names: bash's under the
//! workspace's `target/corpus/`, the runner's `/tmp/corpus`. A script
//! writes nothing to standard error under bash (messages are tested
//! against bash apart, as bash names a script's line in them) and holds no
//! unquoted value with a blank in it (expansion never splits words here).
//! A missing bash fails the test.
#![cfg(test)]
````

Replace:

````rust
use std::process::Command;

````

with:

````rust
use std::process::Command;
use vfs::Vfs;

````

Replace:

````rust
/// What bash 5.2 prints for `script` and its status, in the C locale with
/// no startup files.
fn bash(script: &Path) -> (i32, String) {
    let out = Command::new("bash")
        .args(["--norc", "--noprofile"])
        .arg(script)
        .env_clear()
````

with:

````rust
/// What bash 5.2 prints for `script` and its status, in the C locale with
/// no startup files, in an empty directory.
fn bash(script: &Path) -> (i32, String) {
    let name = script.file_stem().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/corpus")
        .join(std::format!("{}-{}", std::process::id(), name.display()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new("bash")
        .args(["--norc", "--noprofile"])
        .arg(script)
        .current_dir(&dir)
        .env_clear()
````

Replace:

````rust
    let stdout = String::from_utf8(out.stdout).expect("bash's output is text");
    (out.status.code().expect("bash exited"), stdout)
}

/// What the in-process runner prints for `text` and its status.
fn relay(text: &[u8]) -> (i32, String) {
    let mut h = Harness::new();
    let mut input = Bytes::new(text.to_vec());
````

with:

````rust
    let stdout = String::from_utf8(out.stdout).expect("bash's output is text");
    std::fs::remove_dir_all(&dir).unwrap();
    (out.status.code().expect("bash exited"), stdout)
}

/// What the in-process runner prints for `text` and its status, in an
/// empty directory.
fn relay(text: &[u8]) -> (i32, String) {
    let mut h = Harness::new();
    h.dir("/tmp/corpus");
    h.vfs.chdir(b"/tmp/corpus").unwrap();
    let mut input = Bytes::new(text.to_vec());
````

- [ ] **Step 2: Create `crates/shell/tests/corpus/redirect_order.sh`**

Create `crates/shell/tests/corpus/redirect_order.sh`:

````bash
# Redirection (milestone 5's plan 1, programmable shell gate §7.1, §7.2):
# several per command, made left to right; a copy is of the other fd as
# it is at that point; errors only from GNU's programs, never bash's.
echo one > f
echo two >> f
cat f
echo three > f > g
wc -c f g
cat 0< g
wc -l < g
# Both outputs into one file, or the errors where the output was.
cat g missing > both 2>&1; echo $?
cat both
cat g missing 2>&1 > out; echo $?
cat out
cat g missing 2> err >> out; echo $?
cat out err
# Output sent where the errors go, and errors appended.
echo to-err 2> e >&2
echo more 2>> e 1>&2
cat e
# A copy of an fd that changes later is not changed with it.
echo first 2> e3 >&2 2> e2
wc -c e3 e2
# Only redirections: the file is made, or emptied.
> made
wc -c made e
>> made
>e
wc -c made e
````

- [ ] **Step 3: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 397 tests.

- [ ] **Step 4: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 5: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): run the corpus in an empty directory, with redirections

Each corpus script now starts in an empty directory of its own, bash's
under the workspace's `target/corpus/` and the in-process runner's
`/tmp/corpus`, so that it may make files by relative names; none did
before. `redirect_order.sh` compares the order of several redirections
with bash's (programmable shell gate §11.2): both orders of `2>&1`,
`>&2` into a file, a copy made before its fd changes, `<`, `0<` and a
line of only redirections.
EOF
````


### Finish PR 2

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 49 scenario(s) passed`.

````bash
git push -u origin m5p1/simple
gh pr create --base main --head m5p1/simple --title "feat(shell,relay-rt): redirect a command's three standard streams" --body-file - <<'EOF'
## What

Milestone 5, plan 1, tasks 1–13: a command's redirections become a list made left to right over an fd context the walker carries: `/bin/sh` opens each file as an fd of its own and hands a program all three at `spawn`, a built-in writing through the same fd; `2>`, `2>>`, `2>&1`, `1>&2`, `>&2`, `<` and `0<` join `>` and `>>`, every other form refused; a command's own messages and a built-in's errors go to its fd 2; a file opened with `>>` is written at its end each time; `cat` and `grep` refuse an input that is the output, as GNU's do (so `cat f > f`, whose `>` emptied `f`, is now silent with status 0); `sh FILE` refuses a redirected fd 2 and reads a file given as input; `head -n` leaves a file just after its lines, as GNU's does; `help` names the forms; the corpus runs in an empty directory and compares the order of redirections with bash.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; milestone 5's plan 4 runs NUC checks 3 to 7
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-simple
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 3: Pipelines and compound commands (Tasks 14–21)

A pipeline's commands redirect over its pipes, the last one's own messages on its fd 2, and refusals name what was typed; a target is expanded when it is reached; compound commands are redirected, for everything inside; the drop scan's reading of the new forms is pinned; the corpus compares redirected constructs with bash; the `redirect` scenario runs it all under `/bin/sh`.

Branch `m5p1/compound`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-compound`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p1/compound /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-compound origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-compound
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p1/simple` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p1/compound /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-compound m5p1/simple`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p1/simple>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 14: A pipeline's commands redirect over its pipes

Decision 7 (spec §7.3): each command of a pipeline starts from the pipe before it, the pipe after it and the context's fd 2 (`Slot::PipeIn`, `Slot::PipeOut`), and its redirections are made over them: `2>`, `2>>` and the copies may stand on any command, so `a 2>&1 | b` sends both of `a`'s outputs into the pipe, and a command's own messages go there too (`nope 2>&1 | b`); `>` and `>>` stay on the last command and `<` on the first. A command whose redirection fails runs nothing and leaves its neighbours an end, as bash's does; the pipeline fails only when it is the last (`p10.txt`), where before nothing of it started (the spike's S1). In the in-process runner a stage's errors go through its output when fd 2 is fd 1's pipe or file, and the output sent elsewhere (`a 1>&2 | b`) leaves the pipe empty; errors into the pipe while the output goes elsewhere cannot happen, since `>` before `|` is refused. The red run is the shell's tests. Mutation checks (11): the earlier command's output file allowed, its fd 2 refused, each stage's input and output pipes, the spawned stage's pipes, the failed last stage's status in each runner, the shared output of a stage, a stage's message into the pipe in each runner, and the output of a stage sent elsewhere, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 9's parser rules; Task 2's `Stage`.
- Produces: `fds::Slot::{PipeIn, PipeOut}`; `runner::Stage::fds: Option<Fds>`; `Shell::stage_fds` per stage.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, replace:

````rust
        assert_eq!(parse("a|&b"), Err(ParseError::Unsupported("|&".into())));
        // Only the last command redirects (spec §9.1): bash would send the
        // first one's output into the file and the second nothing.
        assert_eq!(
            parse("a > f | b").unwrap_err().to_string(),
            "unsupported syntax: > before |"
        );
````

with:

````rust
        assert_eq!(parse("a|&b"), Err(ParseError::Unsupported("|&".into())));
        // Only the last command redirects its output (spec §9.1): bash
        // would send the first one's output into the file and the second
        // nothing.
        for line in ["a > f | b", "a >> f | b", "a 1> f | b", "a 2>&1 > f | b"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "unsupported syntax: > before |",
                "{line}"
            );
        }
        // Errors may go anywhere (programmable shell gate §7.3), and a copy
        // is made over the pipes.
        for line in [
            "a 2> e | b",
            "a 2>> e | b 2> e2",
            "a 2>&1 | b",
            "a 1>&2 | b",
            "a < f 2>&1 | b | c 2>&1 > g",
        ] {
            assert!(parse(line).is_ok(), "{line}");
        }
        let p = parse("a 2>&1 | b").unwrap();
        assert_eq!(
            p[0].redirects,
            [Redirect {
                fd: 2,
                op: RedirectOp::Copy(1)
            }]
        );
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        assert_eq!(closed, [4, 5, 6], "the redirection and both ends");
        // A redirection that cannot be opened starts nothing.
        let mut h = with_stages();
````

with:

````rust
        assert_eq!(closed, [4, 5, 6], "the redirection and both ends");
        // A redirection that cannot be opened starts nothing of its
        // command, whose neighbours see an end, as in bash (programmable
        // shell gate §15 item 5).
        let mut h = with_stages();
````

Replace:

````rust
        );
        assert!(h.programs.spawned.is_empty() && h.programs.pipes.is_empty());
    }
````

with:

````rust
        );
        assert_eq!(h.programs.spawned.len(), 1);
        assert_eq!(h.programs.spawned[0].path, "/bin/cat");
    }
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn a_pipeline_s_commands_redirect_over_its_pipes() {
        // bash 5.2 (tmp/m5p1/probes/p10.txt): `2>&1` sends the errors into
        // the pipe, and a command's own messages go there too.
        let mut h = Harness::new();
        h.put("/tmp/f", b"x\n");
        assert_eq!(h.run("ls /tmp/f /nope 2>&1 | wc -l"), (0, "2\n".into()));
        assert_eq!(h.run("ls /nope 2> /tmp/e | wc -l"), (0, "0\n".into()));
        assert_eq!(
            h.get("/tmp/e"),
            b"ls: cannot access '/nope': No such file or directory\n"
        );
        assert_eq!(h.run("nope 2>&1 | wc -l"), (0, "1\n".into()));
        // Output sent where the errors go leaves the pipe empty, a file's
        // too.
        assert_eq!(
            h.run("ls /tmp/f /nope 2> /tmp/e2 1>&2 | wc -l"),
            (0, "0\n".into())
        );
        assert_eq!(
            h.get("/tmp/e2"),
            b"ls: cannot access '/nope': No such file or directory\n/tmp/f\n"
        );
        assert_eq!(
            h.run("ls /tmp/f /nope 1>&2 | wc -l"),
            (
                0,
                "ls: cannot access '/nope': No such file or directory\n/tmp/f\n0\n".into()
            )
        );
        assert_eq!(
            h.run("cat /tmp/f | ls /nope 2>&1"),
            (
                2,
                "ls: cannot access '/nope': No such file or directory\n".into()
            )
        );
        // A command whose redirection fails runs nothing and leaves its
        // neighbours an end; the pipeline fails only when it is the last.
        assert_eq!(
            h.run("cat < /nope | wc -l"),
            (0, "relay-sh: /nope: No such file or directory\n0\n".into())
        );
        assert_eq!(
            h.run("echo a 2> /nodir/y | cat"),
            (0, "relay-sh: /nodir/y: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("echo a | cat > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        // Under /bin/sh: each command gets the pipes and its files.
        let mut h = spawning();
        assert_eq!(h.spawning("t-args 2>&1 | t-args 2> /tmp/e").0, 3);
        assert_eq!(h.programs.pipes, [(5, 6)]);
        assert_eq!(h.programs.spawned[0].fds, [0, 6, 6]);
        assert_eq!(h.programs.spawned[1].fds, [5, 1, 4]);
        assert_eq!(
            h.spawning("nope 2>&1 | t-args"),
            (3, String::new()),
            "the message goes into the pipe"
        );
        assert_eq!(
            h.programs.written,
            [(8, b"relay-sh: nope: command not found\n".to_vec())]
        );
        h.programs.open_error = Some(vfs::Errno::ENOENT);
        assert_eq!(
            h.spawning("t-args | t-args > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert_eq!(h.programs.spawned.len(), 4, "the first ran");
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `parser::tests::a_bar_needs_a_command_on_each_side`, `runner::tests::a_pipeline_that_cannot_be_made_starts_no_more`.

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
/// fd of `/bin/sh`'s, which its `Programs` write, or (errors) where the
/// output goes, when fd 2 is a copy of fd 1's file or the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

with:

````rust
/// fd of `/bin/sh`'s, which its `Programs` write, or (errors) where the
/// output goes, when fd 2 is a copy of fd 1's file or pipe or the other
/// way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
````

Replace:

````rust
        self.input = Some(input);
    }
````

with:

````rust
        self.input = Some(input);
    }

    /// Errors go to `err`.
    pub(crate) fn set_err(&mut self, err: To) {
        self.err = err;
    }
````

- [ ] **Step 6: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, replace:

````rust
    File(usize),
}
````

with:

````rust
    File(usize),
    /// In a pipeline, the pipe from the command before.
    PipeIn,
    /// In a pipeline, the pipe to the command after.
    PipeOut,
}
````

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
    pub words: Vec<W>,
    pub redirects: Vec<Redirect<W>>,
}

````

with:

````rust
    pub words: Vec<W>,
    pub redirects: Vec<Redirect<W>>,
}

impl<W> Redirect<W> {
    /// `> file` or `>> file` on fd 1.
    fn is_output_file(&self) -> bool {
        self.fd == 1 && matches!(self.op, RedirectOp::Write(_) | RedirectOp::Append(_))
    }
}

````

Replace:

````rust
        }
        // `<` may stand on the first command only (programmable shell gate
        // §7.3).
        if self.later && self.redirects.iter().any(|r| r.fd == 0) {
            return Err(ParseError::Unsupported("< after |".into()));
        }
        if self.words.is_empty() || self.redirects.iter().any(|r| r.fd != 0) {
            return Err(ParseError::Unsupported("> before |".into()));
````

with:

````rust
        }
        // `<` may stand on the first command only, `>` and `>>` on the last
        // (programmable shell gate §7.3); errors may go anywhere.
        if self.later && self.redirects.iter().any(|r| r.fd == 0) {
            return Err(ParseError::Unsupported("< after |".into()));
        }
        if self.words.is_empty() || self.redirects.iter().any(Redirect::is_output_file) {
            return Err(ParseError::Unsupported("> before |".into()));
````

- [ ] **Step 8: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 14 replacements, top to bottom:

Replace:

````rust

/// A command of a pipeline: its words, and the fds its context and
/// redirections give it, the pipes aside.
pub(crate) struct Stage<'c> {
    pub words: &'c [String],
    pub fds: Fds,
}
````

with:

````rust

/// A command of a pipeline: its words, and the fds the pipes around it,
/// its context and its redirections give it; none when a redirection of
/// it could not be made (programmable shell gate §7.3), and it runs
/// nothing.
pub(crate) struct Stage<'c> {
    pub words: &'c [String],
    pub fds: Option<Fds>,
}
````

Replace:

````rust

    /// Each stage runs to its end before the next starts, its output kept
    /// as the next one's input; a stage that is not found, or whose words
    /// expanded to nothing, gives the next one nothing, as bash's does (the
    /// first says so). Ctrl-C stops the rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Ran {
````

with:

````rust

    /// Each stage runs to its end before the next starts, what it sends
    /// into the pipe (its output, its errors after `2>&1`) kept as the
    /// next one's input; a stage that is not found, whose words expanded
    /// to nothing or whose redirection failed gives the next one nothing,
    /// as bash's does (the first says so on its fd 2). Ctrl-C stops the
    /// rest.
    fn pipeline(&mut self, parts: Parts<'_>, stages: &[Stage<'_>]) -> Ran {
````

Replace:

````rust
            let mut out = Collected(Vec::new());
            let Some((name, args)) = stage.words.split_first() else {
                piped = Some(Bytes::new(out.0));
                continue;
            };
            if let Some(command) = commands::find(name) {
                let mut ctx = Ctx::program(&mut *vfs, &mut *system, &mut *console, &mut out);
                match (&mut piped, &mut input) {
````

with:

````rust
            let mut out = Collected(Vec::new());
            let (Some(fds), Some((name, args))) = (stage.fds, stage.words.split_first()) else {
                piped = Some(Bytes::new(out.0));
                continue;
            };
            let mut err_pipe = Vec::new();
            if let Some(command) = commands::find(name) {
                let mut ctx = match fds.0[1] {
                    Slot::PipeOut => Ctx::program(&mut *vfs, &mut *system, &mut *console, &mut out),
                    slot => {
                        let to = to(files, slot);
                        Ctx::new(&mut *vfs, &mut *system, &mut *console, to, To::Console)
                    }
                };
                // Into the pipe only with the output: `>` before `|` is
                // refused (§7.3).
                ctx.set_err(match fds.0[2] {
                    slot if slot == fds.0[1] => To::Output,
                    slot => to(files, slot),
                });
                match (&mut piped, &mut input) {
````

Replace:

````rust
                // The first stage's `<`.
                if let Some(node) = input_file(files, stage.fds.0[0]) {
                    ctx.set_input_file(node);
````

with:

````rust
                // The first stage's `<`.
                if let Some(node) = input_file(files, fds.0[0]) {
                    ctx.set_input_file(node);
````

Replace:

````rust
            } else {
                let message = not_found(name).message;
                console.write(message.as_bytes());
                if let Some(t) = transcript.as_mut() {
                    let _ = t.add(&mut *vfs, message.as_bytes());
                }
            }
            piped = Some(Bytes::new(out.0));
        }
        let mut piped = piped.expect("a stage before the last");
        let Some((name, args)) = last.words.split_first() else {
````

with:

````rust
            } else {
                // On its fd 2: the screen, a file, or the pipe.
                let message = not_found(name).message;
                match fds.0[2] {
                    Slot::PipeOut => err_pipe = message.into_bytes(),
                    Slot::File(i) => {
                        if let Handle::Node { node, offset } = files.handle(i) {
                            crate::fds::write_file(&mut *vfs, node, offset, message.as_bytes());
                        }
                    }
                    _ => {
                        console.write(message.as_bytes());
                        if let Some(t) = transcript.as_mut() {
                            let _ = t.add(&mut *vfs, message.as_bytes());
                        }
                    }
                }
            }
            let mut bytes = out.0;
            bytes.extend_from_slice(&err_pipe);
            piped = Some(Bytes::new(bytes));
        }
        let mut piped = piped.expect("a stage before the last");
        let Some(fds) = last.fds else {
            // Its redirection failed: bash's status 1.
            return Ran::said(1, String::new());
        };
        let Some((name, args)) = last.words.split_first() else {
````

Replace:

````rust
        };
        self.run(parts, name, args, last.fds)
    }
````

with:

````rust
        };
        self.run(parts, name, args, fds)
    }
````

Replace:

````rust

/// The shell's fd that `slot` is: its own, or a file it opened.
fn shell_fd(files: &Files, slot: Slot) -> u32 {
    match slot {
````

with:

````rust

/// The shell's fd that `slot` is: its own, a file it opened, or in a
/// pipeline the pipe before (`pipe_in`) or after (`pipe_out`).
fn shell_fd(files: &Files, slot: Slot, pipe_in: Option<u32>, pipe_out: Option<u32>) -> u32 {
    match slot {
````

Replace:

````rust
            Handle::Node { .. } => unreachable!("/bin/sh opens its files as fds"),
        },
    }
}
````

with:

````rust
            Handle::Node { .. } => unreachable!("/bin/sh opens its files as fds"),
        },
        Slot::PipeIn => pipe_in.unwrap_or(0),
        Slot::PipeOut => pipe_out.unwrap_or(1),
    }
}
````

Replace:

````rust
        };
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot));
        match self.programs.spawn(path.as_bytes(), &argv, fds, group) {
````

with:

````rust
        };
        let fds = fds.0.map(|slot| shell_fd(parts.files, slot, None, None));
        match self.programs.spawn(path.as_bytes(), &argv, fds, group) {
````

Replace:

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
````

with:

````rust
            };
            let (Some(fds), Some((name, args))) = (stage.fds, stage.words.split_first()) else {
                // Its words expanded to nothing or a redirection of it
                // failed: it runs nothing, and its neighbours see an end.
                for fd in [stdin, stdout].into_iter().flatten() {
                    self.programs.close(fd);
                }
                stdin = next;
                if is_last && stage.fds.is_none() {
                    ran.status = 1;
                }
                continue;
````

Replace:

````rust
            let path = program_path(name);
            let mut fds = stage.fds.0.map(|slot| shell_fd(parts.files, slot));
            if let Some(fd) = stdin {
                fds[0] = fd;
            }
            if let Some(fd) = stdout {
                fds[1] = fd;
            }
            let pid = self.programs.spawn(path.as_bytes(), &argv, fds, group);
            for fd in [stdin, stdout].into_iter().flatten() {
                self.programs.close(fd);
            }
            stdin = next;
            match pid {
````

with:

````rust
            let path = program_path(name);
            let shell_fds = fds.0.map(|slot| shell_fd(parts.files, slot, stdin, stdout));
            let pid = self
                .programs
                .spawn(path.as_bytes(), &argv, shell_fds, group);
            match pid {
````

Replace:

````rust
                Err(e) => {
                    let refused = cannot_start(name, e);
                    parts.console.write(refused.message.as_bytes());
                    if is_last {
````

with:

````rust
                Err(e) => {
                    // On its fd 2, the pipe after it too (`nope 2>&1 | b`).
                    let refused = cannot_start(name, e);
                    match fds.0[2] {
                        Slot::Shell(_) => parts.console.write(refused.message.as_bytes()),
                        _ => {
                            let _ = self
                                .programs
                                .write(shell_fds[2], refused.message.as_bytes());
                        }
                    }
                    if is_last {
````

Replace:

````rust
                }
            }
        }
        (started, ran)
````

with:

````rust
                }
            }
            for fd in [stdin, stdout].into_iter().flatten() {
                self.programs.close(fd);
            }
            stdin = next;
        }
        (started, ran)
````

Replace:

````rust
        },
        Slot::Shell(_) => None,
    }
}

/// Where a command the shell runs itself writes for `slot`.
fn to(files: &Files, slot: Slot) -> To {
    match slot {
        Slot::Shell(_) => To::Console,
        Slot::File(i) => match files.handle(i) {
````

with:

````rust
        },
        Slot::Shell(_) | Slot::PipeIn | Slot::PipeOut => None,
    }
}

/// Where a command the shell runs itself writes for `slot` (a pipe is the
/// pipeline's to make).
fn to(files: &Files, slot: Slot) -> To {
    match slot {
        Slot::Shell(_) | Slot::PipeIn | Slot::PipeOut => To::Console,
        Slot::File(i) => match files.handle(i) {
````

- [ ] **Step 9: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 6 replacements, top to bottom:

Replace:

````rust
            return self.finish(ran.status, ran.message);
        }
        let Some(all) = self.stage_fds(stages) else {
            return self.finish(1, String::new());
        };
        let staged = runner_stages(stages, &all);
        let parts = Parts {
````

with:

````rust
            return self.finish(ran.status, ran.message);
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
        let parts = Parts {
````

Replace:

````rust
        let ran = self.runner.get().pipeline(parts, &staged);
        for fds in all {
            self.release(fds);
````

with:

````rust
        let ran = self.runner.get().pipeline(parts, &staged);
        for fds in all.into_iter().flatten() {
            self.release(fds);
````

Replace:

````rust
        }
        let Some(all) = self.stage_fds(stages) else {
            return self.finish(1, String::new());
        };
        let staged = runner_stages(stages, &all);
````

with:

````rust
        }
        let all = self.stage_fds(stages);
        let staged = runner_stages(stages, &all);
````

Replace:

````rust
        let started = self.runner.get().background(parts, &staged);
        for fds in all {
            self.release(fds);
````

with:

````rust
        let started = self.runner.get().background(parts, &staged);
        for fds in all.into_iter().flatten() {
            self.release(fds);
````

Replace:

````rust

    /// Each stage's fds, its redirections made over the context; none if
    /// one cannot be made.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Option<Vec<Fds>> {
        let mut all = Vec::new();
        for stage in stages {
            match self.redirect(self.fds, &stage.redirects) {
                Some(fds) => all.push(fds),
                None => {
                    for fds in all {
                        self.release(fds);
                    }
                    return None;
                }
            }
        }
        Some(all)
    }
````

with:

````rust

    /// Each stage's fds (programmable shell gate §7.3): the pipe from the
    /// one before, the pipe to the one after, the context's for the rest,
    /// its redirections made over them; none for a stage whose redirection
    /// could not be made, which runs nothing.
    fn stage_fds(&mut self, stages: &[parser::Command]) -> Vec<Option<Fds>> {
        let last = stages.len() - 1;
        let mut all = Vec::new();
        for (i, stage) in stages.iter().enumerate() {
            let input = if i == 0 { self.fds.0[0] } else { Slot::PipeIn };
            let output = if i == last {
                self.fds.0[1]
            } else {
                Slot::PipeOut
            };
            let base = Fds([input, output, self.fds.0[2]]);
            all.push(self.redirect(base, &stage.redirects));
        }
        all
    }
````

Replace:

````rust
/// A pipeline's stages for the runner: each one's words and fds.
fn runner_stages<'c>(stages: &'c [parser::Command], fds: &[Fds]) -> Vec<runner::Stage<'c>> {
    stages
````

with:

````rust
/// A pipeline's stages for the runner: each one's words and fds.
fn runner_stages<'c>(stages: &'c [parser::Command], fds: &[Option<Fds>]) -> Vec<runner::Stage<'c>> {
    stages
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 398 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): redirect the commands of a pipeline over its pipes

Each command of a pipeline starts from the pipe before it, the pipe
after it and the context's fd 2, and its redirections are made over
them (programmable shell gate §7.3): `2>`, `2>>` and the copies may
stand on any command, so `a 2>&1 | b` sends both of `a`'s outputs into
the pipe, and a command's own messages go there too (`nope 2>&1 | b`).
`>` and `>>` stay on the last command and `<` on the first. A command
whose redirection fails runs nothing and leaves its neighbours an end,
as bash's does; the pipeline fails only when it is the last, where
before nothing of it started.
EOF
````


### Task 15: A pipeline's last command tells its own message on its fd 2

Decision 4 (the prototype's review, M-2): in the in-process runner, the message of a pipeline's last command that was not found or could not write went to the screen, so `echo a | nope 2> e` left `e` empty, where `/bin/sh` and bash write it there. The walker now writes the last command's own message on that command's fd 2, as it does a lone command's. The red run is the shell's tests. Mutation checks (2): the message not routed, and the first command's fd 2 taken for the last's, each fail a test.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 14's stages; `Ran::own`.
- Produces: nothing new.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
        assert_eq!(h.run("nope 2>&1 | wc -l"), (0, "1\n".into()));
        // Output sent where the errors go leaves the pipe empty, a file's
````

with:

````rust
        assert_eq!(h.run("nope 2>&1 | wc -l"), (0, "1\n".into()));
        // The last command's own message on its fd 2 (the prototype's
        // review, M-2).
        assert_eq!(h.run("echo a | nope 2> /tmp/n"), (127, String::new()));
        assert_eq!(h.get("/tmp/n"), b"relay-sh: nope: command not found\n");
        // Output sent where the errors go leaves the pipe empty, a file's
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_pipeline_s_commands_redirect_over_its_pipes`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 2 replacements, top to bottom:

Replace:

````rust
        let ran = self.runner.get().pipeline(parts, &staged);
        for fds in all.into_iter().flatten() {
````

with:

````rust
        let ran = self.runner.get().pipeline(parts, &staged);
        // The last command's own message (the in-process runner's) on its
        // fd 2, as a lone command's.
        let mut message = ran.message;
        if ran.own
            && let Some(Some(fds)) = all.last()
        {
            self.say_on(*fds, message.as_bytes());
            message.clear();
        }
        for fds in all.into_iter().flatten() {
````

Replace:

````rust
        self.cancelled |= ran.cancelled;
        self.finish(ran.status, ran.message)
    }
````

with:

````rust
        self.cancelled |= ran.cancelled;
        self.finish(ran.status, message)
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 398 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): tell a pipeline's last command's own message on its fd 2

In the in-process runner, the message of a pipeline's last command that
was not found or could not write went to the screen, so `echo a | nope
2> e` left `e` empty, where `/bin/sh` and bash write it there (the
prototype's review, M-2). The walker now writes the last command's own
message on that command's fd 2, as it does a lone command's.
EOF
````


### Task 16: Refusals and errors name the redirection typed

Decision 2 (the prototype's review, M-4): four messages named a token nobody typed. A redirection alone in a pipeline was `> before |` or `| >` whatever it was, and is now named as typed (`< before |`, `2> before |`, `| <`, `| 2>&1`; `Redirect::operator`), an output file before `|` too (`>> before |`); `1>&x` lost its `1`; and in a `for`'s words a redirection with an fd is bash's syntax error naming the fd (`` `2' ``), not the operator. The red run is the shell's tests. Mutation checks (7): the redirection alone, `<`, `>>` and the fd in its name, the `1>&`, the `for`'s fd, and the last command's name, each fail a test.

**Files:**
- Modify: `crates/shell/src/parser.rs`

**Interfaces:**
- Consumes: Task 15's parser rules.
- Produces: `parser::Redirect::operator()` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
        }
        // bash's syntax errors.
````

with:

````rust
        }
        // A `>&` not followed by a bare fd names the fd as typed.
        assert_eq!(
            one("echo a 1>&x"),
            Err(ParseError::Unsupported("1>&x".into()))
        );
        // In a `for`'s words a redirection is bash's error, naming its fd
        // if it has one (the prototype's review, M-4).
        for (line, token) in [
            ("for x in a 2>f; do b; done", "2"),
            ("for x in a 0<f; do b; done", "0"),
            ("for x in a >f; do b; done", ">"),
        ] {
            assert_eq!(
                parse_line(line).unwrap_err().to_string(),
                alloc::format!("syntax error near unexpected token `{token}'"),
                "{line}"
            );
        }
        // bash's syntax errors.
````

Replace:

````rust
        // nothing.
        for line in ["a > f | b", "a >> f | b", "a 1> f | b", "a 2>&1 > f | b"] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                "unsupported syntax: > before |",
                "{line}"
````

with:

````rust
        // nothing.
        for (line, op) in [
            ("a > f | b", ">"),
            ("a >> f | b", ">>"),
            ("a 1> f | b", ">"),
            ("a 2>&1 > f | b", ">"),
        ] {
            assert_eq!(
                parse(line).unwrap_err().to_string(),
                alloc::format!("unsupported syntax: {op} before |"),
                "{line}"
````

Replace:

````rust
            ("> f | b", "> before |"),
            (">> f | b | c", "> before |"),
            ("a | > f", "| >"),
            ("a | b | >> f", "| >"),
        ] {
````

with:

````rust
            ("> f | b", "> before |"),
            (">> f | b | c", ">> before |"),
            ("a | > f", "| >"),
            ("a | b | >> f", "| >>"),
            // Each names what was typed (the prototype's review, M-4).
            ("< f | cat", "< before |"),
            ("2> e | cat", "2> before |"),
            ("a 2> e >> f | b", ">> before |"),
            ("a | < f", "| <"),
            ("a | 2> e", "| 2>"),
            ("a | 2>&1", "| 2>&1"),
        ] {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 3 tests fail, among them `parser::tests::a_bar_needs_a_command_on_each_side`, `parser::tests::every_command_of_a_pipeline_has_a_name`.

- [ ] **Step 3: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
impl<W> Redirect<W> {
    /// `> file` or `>> file` on fd 1.
````

with:

````rust
impl<W> Redirect<W> {
    /// Its operator, as a refusal names it (`<`, `2>>`, `>&2`).
    fn operator(&self) -> String {
        let fd = |default| {
            if self.fd == default {
                String::new()
            } else {
                format!("{}", self.fd)
            }
        };
        match &self.op {
            RedirectOp::Read(_) => format!("{}<", fd(0)),
            RedirectOp::Write(_) => format!("{}>", fd(1)),
            RedirectOp::Append(_) => format!("{}>>", fd(1)),
            RedirectOp::Copy(from) => format!("{}>&{from}", fd(1)),
        }
    }

    /// `> file` or `>> file` on fd 1.
````

Replace:

````rust
        }
        if self.words.is_empty() || self.redirects.iter().any(Redirect::is_output_file) {
            return Err(ParseError::Unsupported("> before |".into()));
        }
````

with:

````rust
        }
        // A redirection alone, which bash runs, is refused like an output
        // file: each names what was typed.
        let refused = match self.redirects.first() {
            Some(first) if self.words.is_empty() => Some(first),
            _ => self.redirects.iter().find(|r| r.is_output_file()),
        };
        if let Some(r) = refused {
            return Err(ParseError::Unsupported(format!(
                "{} before |",
                r.operator()
            )));
        }
````

Replace:

````rust
            (false, true) => return Err(ParseError::MissingTarget(end)),
            (false, false) => return Err(ParseError::Unsupported("| >".into())),
        }
````

with:

````rust
            (false, true) => return Err(ParseError::MissingTarget(end)),
            (false, false) => {
                let first = parts.redirects.first().map(Redirect::operator);
                return Err(ParseError::Unsupported(format!(
                    "| {}",
                    first.unwrap_or_default()
                )));
            }
        }
````

Replace:

````rust
                if let Some(stage) = self.for_header() {
                    self.for_operator(stage, c, &mut cur)?;
````

with:

````rust
                if let Some(stage) = self.for_header() {
                    // A redirection there is bash's error, naming its fd if
                    // it has one (`for x in a 2>f`).
                    if redirection
                        && self.word.started
                        && let Some(digits) = self.word.word.digits()
                    {
                        return Err(ParseError::Unexpected(digits.into()));
                    }
                    self.for_operator(stage, c, &mut cur)?;
````

Replace:

````rust
                    self.parts.pending = Some(if copy {
                        let typed = if typed == "2" { "2>&" } else { ">&" };
                        Pending::Copy { fd, typed }
````

with:

````rust
                    self.parts.pending = Some(if copy {
                        let typed = match typed {
                            "1" => "1>&",
                            "2" => "2>&",
                            _ => ">&",
                        };
                        Pending::Copy { fd, typed }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 398 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): name the redirection typed in its refusals and errors

Four messages named a token nobody typed (the prototype's review,
M-4): a redirection alone in a pipeline was `> before |` or `| >`
whatever it was, and is now named as typed (`< before |`, `2> before
|`, `| <`); an output file before `|` is named so too (`>> before |`);
`1>&x` lost its `1`; and in a `for`'s words a redirection with an fd is
bash's syntax error naming the fd (`2`), not the operator.
EOF
````


### Task 17: A redirection's target is expanded when it is reached

Decision 3: a lone command's words and its redirections' targets were expanded together before any redirection was made, so `echo a 2> e > $E` told `$E: ambiguous redirect` on the screen. bash expands the words first, then each target as its redirection is made, and tells one that does not expand on fd 2 as it stands then (into `e`); a bad substitution there abandons the line too (`p11.txt`, `p12.txt`). `Shell::make` does that; a pipeline and a job are still expanded whole before they start (spec §10). The red run is the shell's tests. Mutation checks (4): the targets after a failed one expanded, the line not abandoned, the message on the screen, and a job taken for a lone command, each fail a test.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `expand::{words, redirect}`; Task 4's `say_on`.
- Produces: `Shell::make(base, typed redirects) -> Option<Fds>`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn a_redirection_s_target_is_expanded_when_it_is_reached() {
        // bash 5.2 (tmp/m5p1/probes/p11.txt, p12.txt): a target that does
        // not expand is told on fd 2 as it stands, and the redirections
        // after it are not made.
        let mut h = Harness::new();
        assert_eq!(h.run("E="), (0, String::new()));
        assert_eq!(h.run("echo a 2> /tmp/e1 > $E"), (1, String::new()));
        assert_eq!(h.get("/tmp/e1"), b"relay-sh: $E: ambiguous redirect\n");
        assert_eq!(
            h.run("echo a > $E 2> /tmp/e2"),
            (1, "relay-sh: $E: ambiguous redirect\n".into())
        );
        assert!(!h.exists("/tmp/e2"));
        // A bad substitution abandons the line too.
        assert_eq!(
            h.run("echo a 2> /tmp/e3 > ${1A}; echo after"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/e3"), b"relay-sh: ${1A}: bad substitution\n");
        // The words come first: one that does not expand makes no file.
        assert_eq!(
            h.run("echo ${1A} 2> /tmp/e4"),
            (1, "relay-sh: ${1A}: bad substitution\n".into())
        );
        assert!(!h.exists("/tmp/e4"));
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 1 test fails: `shell::tests::a_redirection_s_target_is_expanded_when_it_is_reached`.

- [ ] **Step 3: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 4 replacements, top to bottom:

Replace:

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
            }
        };
        if pipeline.len() > 1 {
            return self.pipeline(&pipeline);
        }
        let cmd = pipeline.remove(0);
        if cmd.words.is_empty() && cmd.redirects.is_empty() {
            // Its words expanded to nothing: bash's status 0.
            return self.finish(0, String::new());
        }
        let Some(fds) = self.redirect(self.fds, &cmd.redirects) else {
            return self.finish(1, String::new());
````

with:

````rust
        }
        // A pipeline or a job is expanded whole before it starts (§10); a
        // lone command's words first, then each redirection's target as
        // it is reached, as bash expands them.
        if commands.len() > 1 || background.is_some() {
            return match expand::expand(commands, &self.vars, self.status) {
                Ok(p) => match background {
                    Some(text) => self.background(&p, text),
                    None => self.pipeline(&p),
                },
                Err(e) => self.not_expanded(e, false),
            };
        }
        let typed = &commands[0];
        let words = match expand::words(&typed.words, &self.vars, self.status) {
            Ok(words) => words,
            Err(e) => return self.not_expanded(e, true),
        };
        if words.is_empty() && typed.redirects.is_empty() {
            // Its words expanded to nothing: bash's status 0.
            return self.finish(0, String::new());
        }
        let Some(fds) = self.make(self.fds, &typed.redirects) else {
            return self.finish(1, String::new());
````

Replace:

````rust
        };
        let ran = match cmd.words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
````

with:

````rust
        };
        let ran = match words.split_first() {
            Some((name, args)) => match commands::builtin(name) {
````

Replace:

````rust
        }
        let mut redirects = Vec::new();
        for r in &cmd.redirects {
            match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => redirects.push(r),
                Err(e) => return self.not_expanded(e, true),
            }
        }
        match self.redirect(self.fds, &redirects) {
            Some(fds) => {
````

with:

````rust
        }
        match self.make(self.fds, &cmd.redirects) {
            Some(fds) => {
````

Replace:

````rust
        self.finish(started.ran.status, started.ran.message)
    }
````

with:

````rust
        self.finish(started.ran.status, started.ran.message)
    }

    /// The context the typed `redirects` make over `base`, each target
    /// expanded as it is reached, as bash does (programmable shell gate
    /// §15 item 5): a target that does not expand is told on fd 2 as it
    /// stands then (`2> e > $E` writes `$E: ambiguous redirect` into `e`),
    /// and a bad substitution abandons the line too, as in bash.
    fn make(&mut self, base: Fds, redirects: &[parser::Redirect<parser::Word>]) -> Option<Fds> {
        let mut expanded = Vec::new();
        let mut failed = None;
        for r in redirects {
            match expand::redirect(r, &self.vars, self.status) {
                Ok(r) => expanded.push(r),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        let fds = self.redirect(base, &expanded)?;
        let Some(e) = failed else {
            return Some(fds);
        };
        self.abandoned = matches!(
            e,
            expand::Error::BadSubstitution(_) | expand::Error::TooLong
        );
        self.say_on(fds, format!("{NAME}: {e}\n").as_bytes());
        self.release(fds);
        None
    }
````

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 399 tests.

- [ ] **Step 5: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 6: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): expand a redirection's target when it is reached

A command's words and its redirections' targets were expanded together
before any redirection was made, so `echo a 2> e > $E` told `$E:
ambiguous redirect` on the screen. bash expands a lone command's words
first, then each target as its redirection is made, and tells a target
that does not expand on fd 2 as it stands then (into `e`), a bad
substitution abandoning the line too (programmable shell gate §15 item
5). A pipeline and a job are still expanded whole before they start
(§10).
EOF
````


### Task 18: Compound commands are redirected

Decisions 3 and 4 (spec §7.4): `if …; fi > f`, `for …; done 2> e` and `while …; done < f` are made once before the construct starts and hold for every command inside, which starts from the context they make (`Shell::run_redirected`); they close after its last command; one that cannot be made runs nothing of it, status 1. Only an operator may follow them, as in bash (`fi > f fi` is its syntax error, `p11.txt`). So a construct's own errors (`for`'s `not a valid identifier`), the shell's reports of the commands inside (an expansion error, a program killed: `Shell::finish`) and a job's `[1] 42` go to its fd 2, as bash's do (`p3.txt`, `p4.txt`), while `^C`, the key's echo, stays on the screen. In the in-process runner every command shares a file's offset, as programs share an open file's (`Files::set_offset`, `Ctx::offsets`), so each `head -n 1` in `for …; done < f` reads the next line there too, a pipeline's first command included. The red run is the shell's tests. Two fds appending to one file in a construct write each at its end (Task 5). Mutation checks (13): a keyword after the redirections, the pipeline's redirections, the route to `run_redirected`, the context restored and released, the offsets after a command and after a stage, `say_on`'s and a stage message's offsets, the input's seek, the job's number, the report's fd, and `^C`'s, each broken, fail a test.

**Files:**
- Modify: `crates/shell/src/commands/script.rs`
- Modify: `crates/shell/src/ctx.rs`
- Modify: `crates/shell/src/fds.rs`
- Modify: `crates/shell/src/parser.rs`
- Modify: `crates/shell/src/runner.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 17' `make`; Task 15's stages; Task 5's `write_file`.
- Produces: `Shell::run_redirected`; `Files::set_offset`; `Ctx::offsets() -> [Option<u64>; 3]`; `runner::put_offsets`.

- [ ] **Step 1: Add the failing tests to `crates/shell/src/commands/script.rs`**

In `crates/shell/src/commands/script.rs`, replace:

````rust
    fn a_script_s_input_may_be_a_file() {
        // Its commands read it (programmable shell gate §15 item 5).
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"cat\n");
        h.put("/tmp/in", b"one\ntwo\n");
        assert_eq!(
            h.run("sh /tmp/s.sh < /tmp/in"),
            (0, "+ cat\none\ntwo\n".into())
        );
````

with:

````rust
    fn a_script_s_input_may_be_a_file() {
        // Its commands read it, one after the other (programmable shell
        // gate §15 item 5).
        let mut h = Harness::new();
        h.put("/tmp/s.sh", b"head -n 1\ncat\n");
        h.put("/tmp/in", b"one\ntwo\n");
        assert_eq!(
            h.run("sh /tmp/s.sh < /tmp/in"),
            (0, "+ head -n 1\none\n+ cat\ntwo\n".into())
        );
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 3 replacements, top to bottom:

Replace:

````rust
            ("for x in a; do b; done &", "& after done"),
            ("for x in a; do b; done > f", "> after done"),
        ] {
````

with:

````rust
            ("for x in a; do b; done &", "& after done"),
        ] {
````

Replace:

````rust
            ("echo x | until a; do b; done", "until after |"),
            ("until a; do b; done > f", "> after done"),
            ("while a; do b; done &", "& after done"),
````

with:

````rust
            ("echo x | until a; do b; done", "until after |"),
            ("while a; do b; done &", "& after done"),
````

Replace:

````rust
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
````

with:

````rust
    #[test]
    fn an_if_in_a_pipeline_or_with_ampersand_is_refused() {
        // bash runs them in a subshell (f4–f7).
        for (line, what) in [
            ("if true; then echo a; fi | cat", "| after fi"),
            ("echo x | if true; then cat; fi", "if after |"),
            ("if true; then echo a; fi &", "& after fi"),
            ("if true; then echo a; fi & b", "& after fi"),
            ("if a; then if b; then c; fi | d; fi", "| after fi"),
            ("if true; then echo a; fi > f | cat", "| after fi"),
            ("for x in a; do b; done 2>&1 &", "& after done"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unsupported(what.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn a_compound_command_takes_redirections_after_its_end() {
        // Programmable shell gate §7.4, bash 5.2 (tmp/m5p1/probes/p11.txt).
        let redirects = |line: &str| -> Vec<String> {
            let first = parse_line(line).unwrap().items.remove(0).and_or.first;
            assert!(matches!(first.run, Run::Compound(_)), "{line}");
            first
                .redirects
                .iter()
                .map(|r| match &r.op {
                    RedirectOp::Read(w) => format!("{}<{}", r.fd, w.typed),
                    RedirectOp::Write(w) => format!("{}>{}", r.fd, w.typed),
                    RedirectOp::Append(w) => format!("{}>>{}", r.fd, w.typed),
                    RedirectOp::Copy(from) => format!("{}>&{from}", r.fd),
                })
                .collect()
        };
        assert_eq!(redirects("if true; then echo a; fi > f"), ["1>f"]);
        assert_eq!(redirects("while a; do b; done < f 2>&1"), ["0<f", "2>&1"]);
        assert_eq!(
            redirects("for x in a; do b; done >> f 2> e"),
            ["1>>f", "2>e"]
        );
        assert_eq!(redirects("until a; do b; done 2>e"), ["2>e"]);
        assert!(parse_line("if a; then for x in y; do b; done > f; fi").is_ok());
        assert!(parse_line("if a; then b; fi > f && c").is_ok());
        // Only an operator may follow them, and they need their words.
        for (line, token) in [
            ("if a; then if b; then c; fi > f fi", "fi"),
            ("if a; then b; fi > f x", "x"),
            ("if a; then b; fi 2 > f", "2"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::Unexpected(token.into())),
                "{line}"
            );
        }
        for (line, token) in [
            ("if a; then b; fi >", "newline"),
            ("if a; then b; fi > ; c", ";"),
        ] {
            assert_eq!(
                parse_line(line),
                Err(ParseError::MissingTarget(token)),
                "{line}"
````

- [ ] **Step 3: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

with:

````rust
    #[test]
    fn a_compound_command_s_redirections_hold_for_everything_inside() {
        // bash 5.2 (tmp/m5p1/probes/p3.txt, p4.txt, p11.txt).
        let mut h = Harness::new();
        h.put("/tmp/f", b"l1\nl2\nl3\n");
        assert_eq!(
            h.run("for x in a b; do echo $x; cd /nope; done > /tmp/fo 2> /tmp/fe"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/fo"), b"a\nb\n");
        assert_eq!(
            h.get("/tmp/fe"),
            b"relay-sh: cd: /nope: No such file or directory\n\
              relay-sh: cd: /nope: No such file or directory\n"
        );
        // One file, whose offset every command inside shares.
        assert_eq!(
            h.run("for x in 1 2; do echo s$x; cd /nope 2>&1; echo e$x; done > /tmp/mix"),
            (0, String::new())
        );
        assert_eq!(
            h.get("/tmp/mix"),
            b"s1\nrelay-sh: cd: /nope: No such file or directory\ne1\n\
              s2\nrelay-sh: cd: /nope: No such file or directory\ne2\n"
        );
        assert_eq!(
            h.run("if cat; then echo t; fi < /tmp/f"),
            (0, "l1\nl2\nl3\nt\n".into())
        );
        // Each `head` reads on where the one before stopped.
        assert_eq!(
            h.run("for x in 1 2; do head -n 1; done < /tmp/f"),
            (0, "l1\nl2\n".into())
        );
        // A pipeline inside reads on from where the one before stopped.
        assert_eq!(
            h.run("for x in 1 2; do head -n 1 | cat; done < /tmp/f"),
            (0, "l1\nl2\n".into())
        );
        // Messages follow one another in the file.
        assert_eq!(
            h.run("for x in 1 2; do nope; nope | cat; done 2> /tmp/n"),
            (0, String::new())
        );
        assert_eq!(
            h.get("/tmp/n"),
            b"relay-sh: nope: command not found\n".repeat(4)
        );
        // Ctrl-C's `^C` is the key's echo, on the screen.
        h.console.interrupt_after = Some(3);
        assert_eq!(
            h.run("while true; do true; done 2> /tmp/c"),
            (130, "^C\n".into())
        );
        assert_eq!(h.get("/tmp/c"), b"");
        // Two fds that append to one file, each at its end (the prototype's
        // review, M-1).
        assert_eq!(
            h.run("for x in 1 2; do echo out$x; cd /nope; done >> /tmp/ap 2>> /tmp/ap"),
            (1, String::new())
        );
        assert_eq!(
            h.get("/tmp/ap"),
            b"out1\nrelay-sh: cd: /nope: No such file or directory\n\
              out2\nrelay-sh: cd: /nope: No such file or directory\n"
        );
        // They end with the construct.
        assert_eq!(
            h.run("for x in 1; do echo in; done > /tmp/o; echo after"),
            (0, "after\n".into())
        );
        // A redirection inside goes over them.
        assert_eq!(
            h.run("for x in 1; do echo in > /tmp/in; echo out; done > /tmp/o"),
            (0, String::new())
        );
        assert_eq!(
            (h.get("/tmp/in"), h.get("/tmp/o")),
            (b"in\n".to_vec(), b"out\n".to_vec())
        );
        // The construct's own errors and the shell's go to its fd 2.
        assert_eq!(
            h.run("for 1x in a; do echo; done 2> /tmp/ie"),
            (1, String::new())
        );
        assert_eq!(
            h.get("/tmp/ie"),
            b"relay-sh: `1x': not a valid identifier\n"
        );
        assert_eq!(
            h.run("for x in 1; do echo ${1A}; done 2> /tmp/be; echo after"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/be"), b"relay-sh: ${1A}: bad substitution\n");
        // One that cannot be made runs nothing of it.
        assert_eq!(
            h.run("for x in 1; do echo a; done > /nodir/x"),
            (1, "relay-sh: /nodir/x: No such file or directory\n".into())
        );
        assert_eq!(
            h.run("while true; do echo a; done < /nope"),
            (1, "relay-sh: /nope: No such file or directory\n".into())
        );
        assert_eq!(h.run("E="), (0, String::new()));
        assert_eq!(
            h.run("for x in 1; do echo a; done 2> /tmp/ae > $E"),
            (1, String::new())
        );
        assert_eq!(h.get("/tmp/ae"), b"relay-sh: $E: ambiguous redirect\n");
        // Under /bin/sh the file is opened once, for every program and
        // built-in inside, and closed after the last.
        let mut h = spawning();
        assert_eq!(
            h.spawning("for x in 1 2; do t-args; help; done > /tmp/o"),
            (0, String::new())
        );
        assert_eq!(h.programs.opened, [("/tmp/o".into(), false, 4)]);
        let fds: Vec<_> = h.programs.spawned.iter().map(|s| s.fds).collect();
        assert_eq!(fds, [[0, 4, 2], [0, 4, 2]]);
        assert!(
            h.programs
                .written_to("/tmp/o")
                .starts_with(b"Programs in /bin:")
        );
        assert_eq!(h.programs.closed, [4]);
    }

    #[test]
    fn a_job_s_number_goes_to_the_fd_2_around_it() {
        // bash 5.2 (tmp/m5p1/probes/p4.txt): `[1] 42` into the file.
        let mut h = with_jobs();
        let out = typed(&mut h, &["for x in 1; do t-spin & done 2> /tmp/je"]);
        assert!(!out.contains("[1]"), "{out}");
        assert_eq!(h.programs.written_to("/tmp/je"), b"[1] 101\n");
    }

    #[test]
    fn bin_sh_gives_a_program_a_file_as_fd_2() {
````

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 5 tests fail, among them `commands::script::tests::a_script_s_input_may_be_a_file`, `parser::tests::a_compound_command_takes_redirections_after_its_end`.

- [ ] **Step 5: Change `crates/shell/src/ctx.rs`**

In `crates/shell/src/ctx.rs`, make these 2 replacements, top to bottom:

Replace:

````rust

    /// Gives the command the file `node` as standard input, from its start.
    pub(crate) fn set_input_file(&mut self, node: Node) {
        self.input_file = Some((node, 0));
    }
````

with:

````rust

    /// Gives the command the file `node` as standard input, from
    /// `offset`.
    pub(crate) fn set_input_file(&mut self, node: Node, offset: u64) {
        self.input_file = Some((node, offset));
    }

    /// Where the in-process runner's files have got to: the output's, the
    /// errors' and the input's, for their shared offsets.
    pub(crate) fn offsets(&self) -> [Option<u64>; 3] {
        let out = match &self.out {
            Output::File { offset, .. } => Some(*offset),
            _ => None,
        };
        let err = match self.err {
            To::File(_, offset) => Some(offset),
            _ => None,
        };
        [self.input_file.map(|(_, offset)| offset), out, err]
    }
````

Replace:

````rust
    pub fn seek_input_back(&mut self, n: u64) -> Result<(), Errno> {
        match &mut self.input {
````

with:

````rust
    pub fn seek_input_back(&mut self, n: u64) -> Result<(), Errno> {
        if let Some((_, offset)) = &mut self.input_file {
            *offset = offset.checked_sub(n).ok_or(Errno::EINVAL)?;
            return Ok(());
        }
        match &mut self.input {
````

- [ ] **Step 6: Change `crates/shell/src/fds.rs`**

In `crates/shell/src/fds.rs`, replace:

````rust
            _ => unreachable!("a slot names an open file"),
        }
````

with:

````rust
            _ => unreachable!("a slot names an open file"),
        }
    }

    /// The in-process runner's file `i` has been read or written up to
    /// `to`, where every fd of it goes on from (an open file's offset).
    pub fn set_offset(&mut self, i: usize, to: u64) {
        if let Some(Some((Handle::Node { offset, .. }, _))) = self.open.get_mut(i) {
            *offset = to;
        }
````

- [ ] **Step 7: Change `crates/shell/src/parser.rs`**

In `crates/shell/src/parser.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! ends one of its items, and an unquoted `&` ends one that runs in the
//! background (user-space gate §9.2), the line going on after either;
//! `&&` and `||` join pipelines into an and-or list. bash's syntax errors
//! name any of them with no command before it, and an and-or list ending
//! with `&` is refused. An unquoted `!` word at a pipeline's start negates
//! its status. A newline ends an item as `;` does, but after `|`, `&&` or
//! `||` the command goes on to the next line; text that ends there is
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
//! Every other shell feature is refused: an unquoted `*`, `?`, `` ` ``, `(`
````

with:

````rust
//! ends one of its items, and an unquoted `&` ends one that runs in the
//! background (user-space gate §9.2), the line going on after either; `&&`
//! and `||` join pipelines into an and-or list. bash's syntax errors name
//! any of them with no command before it, and an and-or list ending with
//! `&` is refused. An unquoted `!` word at a pipeline's start negates its
//! status. A newline ends an item as `;` does, but after `|`, `&&` or `||`
//! the command goes on to the next line; text that ends there is
//! [`ParseError::Incomplete`], and a reader asks for more.
//! `if … then … [elif … then …] [else …] fi`, `while … do … done`,
//! `until … do … done` and `for NAME [in WORD…] do … done` are compound
//! commands, their words keywords only unquoted, whole and where a command
//! name would stand (and `in` and `do` where a `for` takes them; after `fi`
//! or `done`, only a keyword or an operator may follow); one cannot stand
//! in a pipeline of several or before `&`, and redirections after its end
//! hold for all of it (§7.4), after which only an operator may come. A
//! `for`'s header takes no operator but the `;` or newline that ends its
//! words. bash's other reserved words (`case`, `{`, …) are refused where a
//! command name would stand.
//! Every other shell feature is refused: an unquoted `*`, `?`, `` ` ``, `(`
````

Replace:

````rust
        };
        // After a compound command, as in bash, only a keyword that closes
        // or goes on with the one around it may come (`fi fi`, `fi then`).
        if self.compound.is_some() {
            return match w.keyword() {
                Some(k) if k.opens().is_none() => Ok(Some(k)),
                _ => Err(ParseError::Unexpected(w.typed)),
            };
        }
        match self.pending.take() {
````

with:

````rust
        };
        match self.pending.take() {
````

Replace:

````rust
                    op: RedirectOp::Copy(copied),
                });
            }
            // A `!` before anything of the command negates the pipeline
````

with:

````rust
                    op: RedirectOp::Copy(copied),
                });
            }
            // After a compound command, as in bash, only a keyword that
            // closes or goes on with the one around it may come (`fi fi`,
            // `fi then`), and after its redirections no word at all (`fi >
            // f fi` is bash's error, probes/p11.txt).
            None if self.compound.is_some() => {
                return match w.keyword() {
                    Some(k) if k.opens().is_none() && self.redirects.is_empty() => Ok(Some(k)),
                    _ => Err(ParseError::Unexpected(w.typed)),
                };
            }
            // A `!` before anything of the command negates the pipeline
````

Replace:

````rust
) -> Result<Option<Pipeline<Word>>, ParseError> {
    if let Some(c) = parts.compound.take() {
````

with:

````rust
) -> Result<Option<Pipeline<Word>>, ParseError> {
    if parts.pending.is_some() {
        return Err(ParseError::MissingTarget(end));
    }
    if let Some(c) = parts.compound.take() {
````

Replace:

````rust
            run: Run::Compound(Box::new(c)),
            redirects: Vec::new(),
        }));
    }
    if parts.pending.is_some() {
        return Err(ParseError::MissingTarget(end));
    }
````

with:

````rust
            run: Run::Compound(Box::new(c)),
            redirects: p.redirects,
        }));
    }
````

Replace:

````rust
                    }
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        let op = match (append, copy) {
                            (true, _) => ">>",
                            (_, true) => ">&",
                            _ => ">",
                        };
                        let end = c.end();
                        return Err(ParseError::Unsupported(format!("{typed}{op} after {end}")));
                    }
                    self.parts.pending = Some(if copy {
````

with:

````rust
                    }
                    self.parts.pending = Some(if copy {
````

Replace:

````rust
                        }
                    }
                    // Until compound commands can be redirected.
                    if let Some(c) = &self.parts.compound {
                        return Err(ParseError::Unsupported(format!("< after {}", c.end())));
                    }
````

with:

````rust
                        }
                    }
````

- [ ] **Step 8: Change `crates/shell/src/runner.rs`**

In `crates/shell/src/runner.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
                // The first stage's `<`.
                if let Some(node) = input_file(files, fds.0[0]) {
                    ctx.set_input_file(node);
                }
                ctx.transcript = transcript.take();
                (command.run)(&mut ctx, args);
                let _ = ctx.finish();
                let cancelled = ctx.cancelled;
````

with:

````rust
                // The first stage's `<`.
                if let Some((node, offset)) = input_file(files, fds.0[0]) {
                    ctx.set_input_file(node, offset);
                }
                ctx.transcript = transcript.take();
                (command.run)(&mut ctx, args);
                let _ = ctx.finish();
                put_offsets(files, &fds, &ctx);
                let cancelled = ctx.cancelled;
````

Replace:

````rust
                        if let Handle::Node { node, offset } = files.handle(i) {
                            crate::fds::write_file(&mut *vfs, node, offset, message.as_bytes());
                        }
````

with:

````rust
                        if let Handle::Node { node, offset } = files.handle(i) {
                            let message = message.as_bytes();
                            let (at, _) = crate::fds::write_file(&mut *vfs, node, offset, message);
                            files.set_offset(i, at);
                        }
````

Replace:

````rust

/// The in-process runner's file `slot` reads, if it is one.
fn input_file(files: &Files, slot: Slot) -> Option<Node> {
    match slot {
        Slot::File(i) => match files.handle(i) {
            Handle::Node { node, .. } => Some(node),
            Handle::Fd(_) => None,
        },
        Slot::Shell(_) | Slot::PipeIn | Slot::PipeOut => None,
    }
````

with:

````rust

/// The in-process runner's file `slot` reads, if it is one, and where.
fn input_file(files: &Files, slot: Slot) -> Option<(Node, u64)> {
    match slot {
        Slot::File(i) => match files.handle(i) {
            Handle::Node { node, offset } => Some((node, offset)),
            Handle::Fd(_) => None,
        },
        Slot::Shell(_) | Slot::PipeIn | Slot::PipeOut => None,
    }
}

/// The in-process runner's files where `ctx` left them: each fd of `fds`
/// that is one goes on from there.
fn put_offsets(files: &mut Files, fds: &Fds, ctx: &Ctx<'_>) {
    for (slot, offset) in fds.0.iter().zip(ctx.offsets()) {
        if let (Slot::File(i), Some(offset)) = (slot, offset) {
            files.set_offset(*i, offset);
        }
    }
````

Replace:

````rust
    }
    if let Some(node) = input_file(parts.files, fds.0[0]) {
        ctx.set_input_file(node);
    }
````

with:

````rust
    }
    if let Some((node, offset)) = input_file(parts.files, fds.0[0]) {
        ctx.set_input_file(node, offset);
    }
````

Replace:

````rust
    let finished = ctx.finish();
    let (status, message, own) = if ctx.cancelled {
````

with:

````rust
    let finished = ctx.finish();
    put_offsets(parts.files, &fds, &ctx);
    let (status, message, own) = if ctx.cancelled {
````

- [ ] **Step 9: Change `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, make these 5 replacements, top to bottom:

Replace:

````rust
            parser::Run::Commands(commands) => self.run_commands(commands, background),
            parser::Run::Compound(c) => {
                self.status = self.run_compound(c);
                self.status
````

with:

````rust
            parser::Run::Commands(commands) => self.run_commands(commands, background),
            parser::Run::Compound(c) if typed.redirects.is_empty() => {
                self.status = self.run_compound(c);
                self.status
            }
            parser::Run::Compound(c) => {
                self.status = self.run_redirected(c, &typed.redirects);
                self.status
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

    /// Runs a compound command with its redirections (programmable shell
    /// gate §7.4), made once before it starts: every command inside starts
    /// from the context they make, and the files are closed after its
    /// last. One that cannot be made runs nothing of it, status 1.
    fn run_redirected(
        &mut self,
        c: &parser::Compound<parser::Word>,
        redirects: &[parser::Redirect<parser::Word>],
    ) -> i32 {
        let Some(fds) = self.make(self.fds, redirects) else {
            return self.finish(1, String::new());
        };
        let outer = core::mem::replace(&mut self.fds, fds);
        let status = self.run_compound(c);
        self.fds = outer;
        self.release(fds);
        status
    }

````

Replace:

````rust
            let number = self.jobs.add(pgid, &started.pids, text);
            if self.prompting && !self.in_script {
                let last = started.pids.last().copied().unwrap_or(pgid);
                self.say(format!("[{number}] {last}\n").as_bytes());
            }
````

with:

````rust
            let number = self.jobs.add(pgid, &started.pids, text);
            // On fd 2 of the context, as bash says it (`for …; do a &
            // done 2> e` writes it into `e`).
            if self.prompting && !self.in_script {
                let last = started.pids.last().copied().unwrap_or(pgid);
                self.say_on(self.fds, format!("[{number}] {last}\n").as_bytes());
            }
````

Replace:

````rust
            Handle::Node { node, offset } => {
                fds::write_file(&mut *self.vfs, node, offset, bytes);
            }
````

with:

````rust
            Handle::Node { node, offset } => {
                let (at, _) = fds::write_file(&mut *self.vfs, node, offset, bytes);
                self.files.set_offset(i, at);
            }
````

Replace:

````rust
    /// Prints `message`, adds the line's output to a running script's
    /// transcript, syncs, and records `status`.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        self.say(message.as_bytes());
        self.write_transcript();
````

with:

````rust
    /// Prints `message`, adds the line's output to a running script's
    /// transcript, syncs, and records `status`. The message is the shell's
    /// report, on fd 2 of the context (programmable shell gate §7.5), but
    /// for a `^C` at its end, the key's echo, which stays on the screen.
    fn finish(&mut self, status: i32, message: String) -> i32 {
        let (report, ctrl_c) = match message.strip_suffix("^C\n") {
            Some(report) => (report, true),
            None => (message.as_str(), false),
        };
        self.say_on(self.fds, report.as_bytes());
        if ctrl_c {
            self.say(b"^C\n");
        }
        self.write_transcript();
````

- [ ] **Step 10: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 402 tests.

- [ ] **Step 11: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 12: Commit**

````bash
git add crates
git commit -F - <<'EOF'
feat(shell): redirect compound commands

`if …; fi > f`, `for …; done 2> e` and `while …; done < f` are made once
before the construct starts and hold for every command inside, which
starts from the context they make (programmable shell gate §7.4); they
close after its last command. So a construct's own errors (`for`'s `not
a valid identifier`), the shell's reports of the commands inside (an
expansion error, a program killed) and a job's `[1] 42` go to its fd 2,
as bash's do, while `^C`, the key's echo, stays on the screen. Only an
operator may follow them, as in bash. In the in-process runner every
command shares a file's offset, as programs share an open file's, so
each `head -n 1` in `for …; done < f` reads the next line there too.
EOF
````


### Task 19: The drop scan's reading of redirections is pinned

Decision 8: a command dropped before its end is dropped to its end by the drop scan (`crates/shell/src/scan.rs`, spec §15 item 2), which must read every redirection of §7.1, refused or not, as bash does, or the end of a construct could run without its start. It does: a redirection after `fi` or `done`, with an fd, `>&`, `<&`, `&>` or `>|`, ends the drop where bash's command ends; `<<<` has no body; a target is a file's name, never a keyword; a line ending after `&&` or `||` goes on. A test only: it passes at once. Mutation checks (3): `<<<` read as a here-document, and a `>` or `<` target taken for a word that may be a keyword, each fail it.

**Files:**
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `scan::Scan`, through `/bin/sh`'s input.
- Produces: nothing new.

- [ ] **Step 1: Change the tests in `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            b"if t-args a; then t-args b; fi if t-args c\nt-args next\n",
        ] {
````

with:

````rust
            b"if t-args a; then t-args b; fi if t-args c\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_dropped_command_s_redirections_end_its_drop_where_bash_s_end() {
        // Programmable shell gate §15 item 5: the drop scan reads every
        // form of §7.1, refused or not, as bash does: a redirection after
        // `fi` or `done` (its fd, `>&`, `<&`, `&>`, `>|`), `<<<` with no
        // body; a line ending after `&&` or `||` goes on.
        for text in [
            &b"if true; then\nt-args 3> x\nfi 2> /tmp/e\nt-args next\n"[..],
            b"while true; do\nt-args 3> x\ndone < /tmp/f\nt-args next\n",
            b"for x in a; do\nt-args 3> x\ndone 2>&1\nt-args next\n",
            b"for x in a; do\nt-args 3> x\ndone >&2\nt-args next\n",
            b"t-args a <<< x\nt-args next\n",
            b"if true; then\nt-args <<< x\nt-args body\nfi\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 2>&1 > /tmp/o\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi > /tmp/o 2>&1 &&\nt-args tail\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 2>&1 ||\nt-args tail\nt-args next\n",
            b"t-args 3>&1 &&\nt-args tail\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi 0<&3\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi &> /tmp/o\nt-args next\n",
            b"if true; then\nt-args 3> x\nfi >| /tmp/o\nt-args next\n",
            // A target is a file's name, never a keyword.
            b"if true; then\nt-args 3> x\n< fi t-args\nt-args body\nfi\nt-args next\n",
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 403 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): pin the drop scan's reading of redirections

A command dropped before its end is dropped to its end by the drop scan
(programmable shell gate §15 item 2), which must read every redirection
of §7.1, refused or not, as bash does, or the end of a construct could
run without its start. It does: a redirection after `fi` or `done`, with
an fd, `>&`, `<&`, `&>` or `>|`, ends the drop where bash's command
ends; `<<<` has no body; a target is a file's name, never a keyword; a
line ending after `&&` or `||` goes on. The test pins them.
EOF
````


### Task 20: The corpus compares redirected constructs and pipelines with bash

Spec §11.2: `redirect_compound.sh` runs redirections of compound commands and of a pipeline's commands under bash and the in-process runner: `head -n 1` in a loop reading one file, `if grep -q` on a file, `done > f` and `done > f 2>&1` mixing commands, `fi 2>&1 > f`, `2>&1 |`, a redirection inside a construct, and a pipeline in a loop that reads on through the loop's file. A test only: it passes at once. Mutation checks (4): a stage's offset, the input's seek, a construct's redirections ignored, and the last stage given a pipe, each fail it.

**Files:**
- Create: `crates/shell/tests/corpus/redirect_compound.sh`

**Interfaces:**
- Consumes: Task 13's corpus.
- Produces: `crates/shell/tests/corpus/redirect_compound.sh`.

- [ ] **Step 1: Create `crates/shell/tests/corpus/redirect_compound.sh`**

Create `crates/shell/tests/corpus/redirect_compound.sh`:

````bash
# Redirection of compound commands and pipelines (milestone 5's plan 1,
# programmable shell gate §7.3, §7.4): made once, for everything inside;
# every command inside shares the file's offset.
seq 5 > lines
for n in 1 2 3; do head -n 1; done < lines
if grep -q 3; then echo found; fi < lines
for n in a b; do echo $n; cat lines | wc -l; done > out
cat out
while false; do echo never; done > none
wc -c none
for n in x y; do echo $n; cat missing; done > both 2>&1
cat both
for n in 1; do cat missing; done 2> err; echo $?
cat err
if true; then echo kept; cat missing; fi 2>&1 > kept
cat kept
# Each command of a pipeline over its pipes.
cat lines missing 2>&1 | wc -l
cat missing 2> err | wc -l
cat err
cat lines | head -n 2 2> err | wc -l
# A redirection inside a construct goes over the construct's.
for n in 1; do echo in > inner; echo outer; done > outside
cat inner outside
# Loops that read on through one file.
seq 4 > four
for n in a b; do head -n 2 | cat; done < four
````

- [ ] **Step 2: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 403 tests.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add crates
git commit -F - <<'EOF'
test(shell): compare redirected constructs and pipelines with bash

`redirect_compound.sh` runs redirections of compound commands and of a
pipeline's commands under bash and the in-process runner (programmable
shell gate §11.2): `head -n 1` in a loop reading one file, `if grep -q`
on a file, `done > f` and `done > f 2>&1` mixing commands, `fi 2>&1 >
f`, `2>&1 |`, a redirection inside a construct, and a pipeline in a loop
that reads on through the loop's file.
EOF
````


### Task 21: The `redirect` scenario

Spec §11.3, without the `/dev/null` lines, which plan 2 adds: under `/bin/sh`, `<`, `2>`, `2>>`, both orders of `2>&1` and `a 2>&1 | b`; a command's own messages and a built-in's on its fd 2; a construct's redirections, `help` (a built-in) and `echo` (a program) writing one after the other into one file, `head -n 1` leaving it after its line; `sh FILE` refusing a redirected fd 2 and reading a file given as input; errors sent to a file kept out of a script's transcript; and Ctrl-C reaching a program whose input is a file. A test only: it passes at once. Its worth: relay-rt's `Console::is_screen`, `Stdin::seek_back` and `Programs::open_input`, which only the real kernel answers, each broken, fail it (mutation checks 3; the fourth, `Programs::write`'s loop, is equivalent: only a partial write reaches it, which the kernel never gives for a file).

**Files:**
- Create: `tests/e2e/redirect.txt`

**Interfaces:**
- Consumes: Tasks 2–20.
- Produces: `tests/e2e/redirect.txt`.

- [ ] **Step 1: Add the scenario `tests/e2e/redirect.txt`**

Create `tests/e2e/redirect.txt`:

````text
# Redirection under /bin/sh (programmable shell gate §7; milestone 5,
# plan 1): `<`, `2>`, `2>>`, both orders of `2>&1`, `a 2>&1 | b`; a
# command's own messages on its fd 2, a built-in's too; a construct's
# redirections made once for everything inside, a built-in and a program
# sharing the file's offset, and `head` leaving the file just after its
# line; `sh FILE` refusing to send its errors to a file but reading one;
# errors sent to a file kept out of a script's transcript; and Ctrl-C
# reaching a program whose input is a file.
timeout 60
expect root@relay:~# $
send echo l1 > r-f; echo l2 >> r-f; echo l3 >> r-f
expect root@relay:~# $
send cat < r-f
expect \nl1\nl2\nl3\nroot@relay:~# $
send cat < r-nope; echo "status $?"
expect \nrelay-sh: r-nope: No such file or directory\nstatus 1\nroot@relay:~# $
send ls r-f r-nope 2> r-e
expect \nr-f\nroot@relay:~# $
send ls r-f r-nope 2>> r-e; cat r-e
expect \nr-f\nls: cannot access 'r-nope': No such file or directory\nls: cannot access 'r-nope': No such file or directory\nroot@relay:~# $
# Both outputs into the file, or the errors where the output was.
send ls r-f r-nope > r-o 2>&1; cat r-o
expect \nls: cannot access 'r-nope': No such file or directory\nr-f\nroot@relay:~# $
send ls r-f r-nope 2>&1 > r-o2
expect \nls: cannot access 'r-nope': No such file or directory\nroot@relay:~# $
send cat r-o2
expect \nr-f\nroot@relay:~# $
send ls r-f r-nope 2>&1 | wc -l
expect \n2\nroot@relay:~# $
send nope 2> r-e2; cd /nope 2>> r-e2; cat r-e2
expect \nrelay-sh: nope: command not found\nrelay-sh: cd: /nope: No such file or directory\nroot@relay:~# $
# A construct's redirections, made once: help (a built-in) and echo (a
# program) write one after the other into the one file.
send for x in a b; do echo $x; help; done > r-mix
expect root@relay:~# $
send grep -n '^[ab]$' r-mix
expect \n1:a\n\d+:b\nroot@relay:~# $
send for x in 1 2; do head -n 1; done < r-f
expect \nl1\nl2\nroot@relay:~# $
send if grep -q l2; then echo found; fi < r-f
expect \nfound\nroot@relay:~# $
send for x in 1; do nope; done 2> r-e3; cat r-e3
expect \nrelay-sh: nope: command not found\nroot@relay:~# $
# A script's errors hold its trace: refused; its input may be a file.
send echo echo hi > r-s.sh
expect root@relay:~# $
send sh r-s.sh 2> r-se; echo "status $?"; cat r-se
expect \nstatus 1\nsh: a script's output cannot be redirected\nroot@relay:~# $
send echo cat > r-in.sh
expect root@relay:~# $
send sh r-in.sh < r-f
expect \n\+ cat\nl1\nl2\nl3\nroot@relay:~# $
send echo 'ls r-nope 2> r-te' > r-t.sh
expect root@relay:~# $
send sh r-t.sh
expect \n\+ ls r-nope 2> r-te\nroot@relay:~# $
send cat r-t.log r-te
expect \n\+ ls r-nope 2> r-te\nls: cannot access 'r-nope': No such file or directory\nroot@relay:~# $
# A program whose input is a file still has the console's foreground.
send t-spin < r-f
alive 1
type-ahead {ctrl-c}
expect \n\^C\nroot@relay:~# $
````

- [ ] **Step 2: Run the `redirect` scenario**

Run: `cargo xtask test --e2e-only --scenario redirect`

Expected: PASS: ends with `all 1 scenario(s) passed`.

- [ ] **Step 3: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 4: Commit**

````bash
git add tests
git commit -F - <<'EOF'
test(e2e): add the redirect scenario

`redirect` runs redirection under `/bin/sh` (programmable shell gate
§11.3, without its `/dev/null` lines until plan 2): `<`, `2>`, `2>>`,
both orders of `2>&1` and `a 2>&1 | b`; a command's own messages and a
built-in's on its fd 2; a construct's redirections, a built-in and a
program sharing the file, `head` leaving it after its line; `sh FILE`
refusing a redirected fd 2 and reading a file; errors sent to a file
kept out of a script's transcript; and Ctrl-C reaching a program whose
input is a file.
EOF
````


### Finish PR 3

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 50 scenario(s) passed`.

````bash
git push -u origin m5p1/compound
gh pr create --base main --head m5p1/compound --title "feat(shell,e2e): redirect pipelines and compound commands" --body-file - <<'EOF'
## What

Milestone 5, plan 1, tasks 14–21: a pipeline's commands redirect over its pipes (`a 2>&1 | b`), a stage whose redirection fails running nothing and the last one's own messages going to its fd 2; refusals and errors name the redirection typed; each target is expanded when it is reached, as bash's is; `if …; fi > f`, `for …; done 2> e` and `while …; done < f` hold for everything inside, a construct's errors and the shell's reports of its commands going to its fd 2; the in-process runner's commands share a file's offset; the drop scan's reading of redirections is pinned; the corpus compares redirected constructs and pipelines with bash; the `redirect` scenario runs them under `/bin/sh`.

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests
- [x] New scenario `redirect`

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; milestone 5's plan 4 runs NUC checks 3 to 7
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-compound
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````

---

## PR 4: The drop scan's gaps (Tasks 22–24)

Milestone 4's deferred gaps of the drop scan: quotes inside `$(…)` in double quotes, `time`'s options and `coproc` before a group, and two subshells read as arithmetic (M-2).

Branch `m5p1/scan`, worktree `/home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-scan`. Checks that run on this PR: `lint`, `unit` and `e2e`.

- [ ] **Start the branch**

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree add -b m5p1/scan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-scan origin/main
cd /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-scan
mkdir -p /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers
ln -s /home/maw/src/bin-inc/relay-os-impl/tmp/sdd-ledger .superpowers/sdd
printf '*\n' > .superpowers/.gitignore
````

Run every command in this PR's tasks from that worktree. The executing-plans ledger lives outside the worktree (it survives the worktree's removal after the merge); `.superpowers/` is ignored by its own `.gitignore`, so it never shows up in `git status`. When the worktree goes, remove `.superpowers/sdd` and `.superpowers/.gitignore` by name, never recursively. If `m5p1/compound` is still waiting for review, branch from it instead of `origin/main` (`… worktree add -b m5p1/scan /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-scan m5p1/compound`), and after its merge record its old tip in the ledger, rebase with `git rebase --onto origin/main <old tip of m5p1/compound>` and re-run `cargo xtask ci` unless the tree's hash is unchanged.

### Task 22: The drop scan reads quotes inside a substitution in double quotes

Decision 8 (milestone 4's deferred gap): the drop scan read `"$(echo ")")"` as bash does not: the `"` inside `$(…)` ended the outer quotes, so a `fi` in a string that went on to later lines could end a dropped construct early and run the lines after it. In double quotes `$(` and `${` now open a level in which quotes start afresh, and the double quotes go on after it (`quoted_levels`, `close_level`), as bash reads them (`p13.txt`); a level too deep to remember drops the rest, the safe side. The red run is the shell's tests: the line after the string runs. Mutation checks (6): the level not opened, the quote not restored, the `$` not seen, the `$` kept past a byte, the level not marked, and the depth past 64, each fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `scan::Scan::{open_level, nested_byte}`.
- Produces: `Scan::{quoted_levels, dollar}`, `Scan::close_level` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
        assert_eq!(after(&["echo ${a:-\\}; if x}; if c"]), (1, false));
    }
````

with:

````rust
        assert_eq!(after(&["echo ${a:-\\}; if x}; if c"]), (1, false));
        // In double quotes `$(` and `${` start quotes afresh, as bash reads
        // `"$(echo ")")"` (tmp/m5p1/probes/p13.txt); the double quotes go
        // on after them (milestone 4's deferred gap).
        assert_eq!(after(&["echo \"$(echo \")\")\"; if c"]), (1, false));
        assert_eq!(after(&["echo \"${x:-\"}\"}\"; if c"]), (1, false));
        assert_eq!(
            after(&["echo \"$(echo \"$(echo \")\")\")\" if c"]),
            (0, false)
        );
        assert_eq!(after(&["echo \"$(( 1 + (2) ))\"; if c"]), (1, false));
        assert_eq!(after(&["echo \"$(a)if\" ; if c"]), (1, false));
        assert_eq!(after(&["echo \"$x(\" ; if c"]), (1, false));
        // Too deep to remember the quotes: the rest is dropped.
        let mut s = Scan::new();
        s.line(alloc::format!("echo \"{}", "$(\"".repeat(LEVELS_KEPT + 1)).as_bytes());
        assert!(s.lost);
        assert_eq!(after(&["echo \"\\$(\" ; if c"]), (1, false));
        assert_eq!(
            after(&["if a; then", "echo \"$(echo \"", "fi", "\")\""]),
            (1, false)
        );
    }
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_here_document_s_body_does_not_run() {
````

with:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_quote_in_a_substitution_in_double_quotes_ends_no_drop() {
        // Milestone 4's deferred gap: the scan took the `"` inside `$(…)`
        // for the end of the outer quotes, so the `fi` in the string ended
        // the drop and the line after it ran.
        let text = b"if true; then\nt-args 3> x\necho \"$(echo \"\nfi\nt-args body\n\")\"\nfi\nt-args next\n";
        assert_eq!(piped(text), ["next"]);
    }

    #[test]
    fn a_here_document_s_body_does_not_run() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::quotes_and_escapes_go_on_across_lines_as_bash_s_do`, `shell::tests::a_quote_in_a_substitution_in_double_quotes_ends_no_drop`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! refusal, so it reads what the parser refuses as bash reads it: `$(…)`
//! and backquotes whole, a command name's place after `time`, `{` and `}`,
//! `select` and `case` opening constructs, a `case` pattern before `)`,
//! groups (`{ … }`, a function's body too, `function f {` among them)
//! and subshells (`( … )`), each `(` paired with its `)`,
//! arithmetic (`((…))`) whole, and a here-document's body, data up to its
//! delimiter's line.

````

with:

````rust
//! refusal, so it reads what the parser refuses as bash reads it: `$(…)`
//! and backquotes whole, in double quotes too, where quotes inside `$(…)`
//! and `${…}` start afresh, a command name's place after `time`, `{` and
//! `}`, `select` and `case` opening constructs, a `case` pattern before
//! `)`, groups (`{ … }`, a function's body too, `function f {` among them)
//! and subshells (`( … )`), each `(` paired with its `)`, arithmetic
//! (`((…))`) whole, and a here-document's body, data up to its delimiter's
//! line.

````

Replace:

````rust
    braces: u64,
    /// A word inside a `$(…)`: its first bytes, how many it has, whether
````

with:

````rust
    braces: u64,
    /// Which of them were opened inside double quotes, which go on after
    /// them; and whether the byte before, in double quotes, was a `$`.
    quoted_levels: u64,
    dollar: bool,
    /// A word inside a `$(…)`: its first bytes, how many it has, whether
````

Replace:

````rust
            braces: 0,
            inner: [0; 4],
````

with:

````rust
            braces: 0,
            quoted_levels: 0,
            dollar: false,
            inner: [0; 4],
````

Replace:

````rust
        if let Some(q) = self.quote {
            match b {
                _ if b == q => self.quote = None,
                b'\\' if q != b'\'' => self.escaped = true,
                _ => {}
````

with:

````rust
        if let Some(q) = self.quote {
            let dollar = core::mem::take(&mut self.dollar);
            match b {
                _ if b == q => self.quote = None,
                b'\\' if q != b'\'' => self.escaped = true,
                b'$' if q == b'"' => self.dollar = true,
                // In double quotes `$(` and `${` open a level in which
                // quotes start afresh, as bash reads `"$(echo ")")"`; the
                // double quotes go on after it.
                b'(' | b'{' if dollar => {
                    self.quote = None;
                    if self.nested < LEVELS_KEPT {
                        self.quoted_levels |= 1u64 << self.nested;
                    } else {
                        self.lost = true;
                    }
                    self.open_level(b == b'{');
                }
                _ => {}
````

Replace:

````rust

    /// Whether the innermost open level is a `${`.
````

with:

````rust

    /// The innermost open level ends: the double quotes it was opened in,
    /// if it was, go on.
    fn close_level(&mut self) {
        self.nested -= 1;
        if self.nested < LEVELS_KEPT {
            let bit = 1u64 << self.nested;
            if self.quoted_levels & bit != 0 {
                self.quoted_levels &= !bit;
                self.quote = Some(b'"');
            }
        }
    }

    /// Whether the innermost open level is a `${`.
````

Replace:

````rust
                self.inner_end();
                self.nested -= 1;
            }
````

with:

````rust
                self.inner_end();
                self.close_level();
            }
````

Replace:

````rust
                } else {
                    self.nested -= 1;
                }
````

with:

````rust
                } else {
                    self.close_level();
                }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 404 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read quotes inside a substitution in double quotes

The drop scan read `"$(echo ")")"` as bash does not: the `"` inside
`$(…)` ended the outer quotes, so a `fi` in a string that went on to
later lines could end a dropped construct early and run the lines after
it (milestone 4's deferred gap). In double quotes `$(` and `${` now
open a level in which quotes start afresh, and the double quotes go on
after it, as bash reads them; a level too deep to remember drops the
rest, the safe side.
EOF
````


### Task 23: The drop scan drops a group after `time`'s options or `coproc` whole

Decision 8 (milestone 4's deferred gap): the scan took a command name's place to end after `time`'s `-p` or `--` and after `coproc`, so `time -p {` and `coproc {` opened no group and the group's lines ran without their guard. As bash reads them (`time -p -- {`, `coproc NAME {`, `coproc NAME if …`; `p13.txt`), those words keep the place for the word after them, until an operator or a newline. The red run is the shell's tests: the group's body runs. Mutation checks (8): the options not kept, kept anywhere, the name's place, `coproc` not seen, and the four resets after an operator and a newline, each fail a test.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: `scan::Scan::end_word`.
- Produces: `Scan::{time, coproc}` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
            (&["time if a; then"][..], 1),
            (&["{ while a; do"], 2),
````

with:

````rust
            (&["time if a; then"][..], 1),
            // And after `time`'s options, and after `coproc` and its name
            // (bash 5.2, tmp/m5p1/probes/p13.txt; milestone 4's deferred
            // gap).
            (&["time -p { a"], 1),
            (&["time -- while a"], 1),
            (&["time -p -- if a"], 1),
            (&["time -x {"], 0),
            (&["time a {"], 0),
            (&["coproc { a"], 1),
            (&["coproc N { a"], 1),
            (&["coproc N if a; then"], 1),
            (&["coproc N ( a"], 1),
            (&["coproc echo if"], 1),
            (&["coproc cat file {"], 0),
            (&["coproc N; {"], 1),
            (&["time; -p {"], 0),
            (&["coproc; a {"], 0),
            (&["time", "-p {"], 0),
            (&["coproc", "a {"], 0),
            (&["{ while a; do"], 2),
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
````

with:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_group_after_time_s_options_or_coproc_is_dropped_whole() {
        // Milestone 4's deferred gap: the scan took `{` after `time -p` or
        // `coproc` for a word, so the group's lines ran without their
        // guard.
        for text in [
            &b"t-args 3> x &&\ntime -p {\nt-args body\n}\nt-args next\n"[..],
            b"t-args 3> x &&\ncoproc {\nt-args body\n}\nt-args next\n",
            b"t-args 3> x &&\ncoproc N {\nt-args body\n}\nt-args next\n",
        ] {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::refused_syntax_is_read_as_bash_reads_it`, `shell::tests::a_group_after_time_s_options_or_coproc_is_dropped_whole`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 7 replacements, top to bottom:

Replace:

````rust
//! and backquotes whole, in double quotes too, where quotes inside `$(…)`
//! and `${…}` start afresh, a command name's place after `time`, `{` and
//! `}`, `select` and `case` opening constructs, a `case` pattern before
//! `)`, groups (`{ … }`, a function's body too, `function f {` among them)
````

with:

````rust
//! and backquotes whole, in double quotes too, where quotes inside `$(…)`
//! and `${…}` start afresh, a command name's place after `time` and its
//! options `-p` and `--`, after `coproc` and after its name, and after `{`
//! and `}`, `select` and `case` opening constructs, a `case` pattern before
//! `)`, groups (`{ … }`, a function's body too, `function f {` among them)
````

Replace:

````rust
    function: bool,
    /// The open levels are arithmetic's (`((`), until its first `)` shows
````

with:

````rust
    function: bool,
    /// After `time`: its options `-p` and `--` keep the command name's
    /// place for the word after them.
    time: bool,
    /// After `coproc`: a word that is no keyword is the coprocess's name
    /// or command, and the word after it stands where a command name
    /// would, as bash reads `coproc NAME {`.
    coproc: bool,
    /// The open levels are arithmetic's (`((`), until its first `)` shows
````

Replace:

````rust
            function: false,
            arithmetic: false,
````

with:

````rust
            function: false,
            time: false,
            coproc: false,
            arithmetic: false,
````

Replace:

````rust
        self.closed = false;
        self.for_ = For::No;
````

with:

````rust
        self.closed = false;
        self.time = false;
        self.coproc = false;
        self.for_ = For::No;
````

Replace:

````rust
        self.command = true;
        // The here-documents the line started take the next lines.
````

with:

````rust
        self.command = true;
        self.time = false;
        self.coproc = false;
        // The here-documents the line started take the next lines.
````

Replace:

````rust
        let closed = core::mem::take(&mut self.closed);
        match word {
````

with:

````rust
        let closed = core::mem::take(&mut self.closed);
        let after_time = core::mem::take(&mut self.time);
        let after_coproc = core::mem::take(&mut self.coproc);
        match word {
````

Replace:

````rust
            b"{" => self.open(Kind::Group),
            b"then" | b"elif" | b"else" | b"do" | b"!" | b"time" => {}
            _ => self.command = false,
        }
````

with:

````rust
            b"{" => self.open(Kind::Group),
            b"time" => self.time = true,
            b"-p" | b"--" if after_time => self.time = true,
            b"coproc" => self.coproc = true,
            b"then" | b"elif" | b"else" | b"do" | b"!" => {}
            _ => self.command = after_coproc,
        }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 405 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): drop a group after time's options or coproc whole

The drop scan took a command name's place to end after `time`'s `-p` or
`--` and after `coproc`, so `time -p {` and `coproc {` opened no group
and the group's lines ran without their guard (milestone 4's deferred
gap). As bash reads them (`time -p -- {`, `coproc NAME {`, `coproc NAME
if …`), those words keep the place for the word after them.
EOF
````


### Task 24: The drop scan reads on after two subshells read as arithmetic

Decision 8 (milestone 4's deferred minor M-2): when a dropped `((`'s first `)` is not followed by another, bash reads two subshells (`((a); b)`), which the scan had read as arithmetic, so it dropped the rest of the input, the safe side, though only a here-document needs it. When what the `((` held reads the same as a subshell's command (letters, digits, `_`, blanks and `+ - * / % = , . !`, and no word that could be a keyword), the scan now reads on with the outer subshell open, as bash does (`p13.txt`); otherwise it drops the rest, as before. A word right before a `)` stays no keyword (the scan's rule for a `case` pattern), so `fi)` keeps a drop going, the safe side. The red run is the shell's tests: the line after the subshells is dropped. Mutation checks (9): the reading on, the `((`'s mark, the keyword check, the long word, the `)` allowed, the level the bytes are checked at, the outer subshell, and `closed` cleared at the `((`, each fail a test; a throwaway fuzz of 200,000 random inputs found no panic.

**Files:**
- Modify: `crates/shell/src/scan.rs`
- Modify: `crates/shell/src/shell.rs`

**Interfaces:**
- Consumes: Task 22's scan; the arithmetic check of milestone 4's plan 4.
- Produces: `Scan::{plain, plain_word, plain_len}`, `Scan::plain_byte`, `scan::KEYWORDS` (private).

- [ ] **Step 1: Add the failing tests to `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, replace:

````rust
        );
        assert_eq!(done_after(&["((a) << 1)", "b", "1", "c"]), [false; 4]);
        assert_eq!(done_after(&["((a) )", "b"]), [false, false]);
        // After its `))`, a `$(…)` nested in a word is a word's again.
````

with:

````rust
        );
        // When what it held reads the same as a subshell's command, the
        // scan reads on as bash does, the outer subshell open (milestone
        // 4's deferred minor M-2; tmp/m5p1/probes/p13.txt).
        assert_eq!(done_after(&["((a); b)", "c"]), [true, true]);
        assert_eq!(done_after(&["((a) )", "b"]), [true, true]);
        assert_eq!(
            done_after(&["((a b) ; if c", "fi )", "d"]),
            [false, true, true]
        );
        assert_eq!(done_after(&["((x + 1) && (y))", "c"]), [true, true]);
        assert_eq!(done_after(&["((a) ; b", "c)", "d"]), [false, true, true]);
        // After the inner `)` an opener is no construct, as after `fi`.
        assert_eq!(done_after(&["((a) if b)", "c"]), [true, true]);
        assert_eq!(
            done_after(&["((a) << 1)", "b", "1", "c"]),
            [false, false, true, true]
        );
        // Otherwise the rest is dropped: a keyword, a quote, a `$`, a
        // redirection, a separator or a parenthesis before the `)`.
        for line in [
            "((if) ; b)",
            "((a fi); b)",
            "((a 'x'); b)",
            "((a $x); b)",
            "((a > f); b)",
            "((a; b); c)",
            "(((a)); b)",
            "((function); b)",
            "((abcdefghij); b)",
        ] {
            assert_eq!(done_after(&[line, "c"]), [false, false], "{line}");
        }
        // `$((a) b)` is a substitution of a subshell and a word: a word.
        assert_eq!(done_after(&["echo $((a) b)", "c"]), [true, true]);
        // After its `))`, a `$(…)` nested in a word is a word's again.
````

- [ ] **Step 2: Add the failing tests to `crates/shell/src/shell.rs`**

In `crates/shell/src/shell.rs`, replace:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn a_group_after_time_s_options_or_coproc_is_dropped_whole() {
````

with:

````rust
            b"if true; then\nt-args 3> x\n2> fi t-args\nt-args body\nfi\nt-args next\n",
        ] {
            assert_eq!(piped(text), ["next"], "{}", String::from_utf8_lossy(text));
        }
    }

    #[test]
    fn two_subshells_dropped_as_arithmetic_end_their_drop() {
        // Milestone 4's deferred minor M-2: `((a); b)`, two subshells to
        // bash, dropped the rest of the input.
        assert_eq!(piped(b"((t-args a); t-args b)\nt-args next\n"), ["next"]);
        assert_eq!(
            piped(b"t-args 3> x &&\n((t-args a) ; if t-args b\nfi )\nt-args next\n"),
            ["next"]
        );
    }

    #[test]
    fn a_group_after_time_s_options_or_coproc_is_dropped_whole() {
````

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p shell`

Expected: FAIL: 2 tests fail: `scan::tests::arithmetic_holds_no_here_document`, `shell::tests::two_subshells_dropped_as_arithmetic_end_their_drop`.

- [ ] **Step 4: Change `crates/shell/src/scan.rs`**

In `crates/shell/src/scan.rs`, make these 8 replacements, top to bottom:

Replace:

````rust
//! and subshells (`( … )`), each `(` paired with its `)`, arithmetic
//! (`((…))`) whole, and a here-document's body, data up to its delimiter's
//! line.

````

with:

````rust
//! and subshells (`( … )`), each `(` paired with its `)`, arithmetic
//! (`((…))`) whole, or as the two subshells bash reads when its first `)`
//! is not followed by another and what it held reads the same as a
//! subshell's command (the rest dropped when it does not), and a
//! here-document's body, data up to its delimiter's line.

````

Replace:

````rust
const OPENERS: &[&[u8]] = &[b"if", b"while", b"until", b"for", b"select", b"case"];

````

with:

````rust
const OPENERS: &[&[u8]] = &[b"if", b"while", b"until", b"for", b"select", b"case"];

/// bash's reserved words and the scan's other keywords, any of which
/// could change how a subshell's command reads.
const KEYWORDS: &[&[u8]] = &[
    b"if",
    b"then",
    b"elif",
    b"else",
    b"fi",
    b"while",
    b"until",
    b"for",
    b"select",
    b"in",
    b"do",
    b"done",
    b"case",
    b"esac",
    b"function",
    b"time",
    b"coproc",
];

````

Replace:

````rust
    arithmetic_check: bool,
    /// What was read cannot be told apart any more: the drop goes on to the
````

with:

````rust
    arithmetic_check: bool,
    /// What an arithmetic command holds up to its first `)` reads the same
    /// as a subshell's command: no quote, `$`, redirection, separator or
    /// parenthesis, and no word that could be a keyword (`plain`, the word
    /// being read so far, and its length).
    plain: bool,
    plain_word: [u8; KEYWORD_MAX],
    plain_len: usize,
    /// What was read cannot be told apart any more: the drop goes on to the
````

Replace:

````rust
            arithmetic_check: false,
            lost: false,
````

with:

````rust
            arithmetic_check: false,
            plain: false,
            plain_word: [0; KEYWORD_MAX],
            plain_len: 0,
            lost: false,
````

Replace:

````rust
        // An arithmetic `((` whose first `)` is not followed by another is
        // two subshells, as bash reads it, which the scan read as words:
        // a here-document or a keyword in them went unseen.
        if core::mem::take(&mut self.arithmetic_check) && b != b')' {
            self.arithmetic = false;
            self.lost = true;
        }
````

with:

````rust
        // An arithmetic `((` whose first `)` is not followed by another is
        // two subshells, as bash reads it, which the scan read as words.
        // When what it held reads the same as a subshell's command
        // (`((a); b)`), the outer subshell is open and the inner closed;
        // otherwise a here-document or a keyword in them went unseen.
        if core::mem::take(&mut self.arithmetic_check) && b != b')' {
            self.arithmetic = false;
            if self.plain {
                // The `((` left `closed` set: as after any subshell's `)`,
                // a keyword may follow and an opener is bash's error.
                self.nested = 0;
                self.open(Kind::Subshell);
            } else {
                self.lost = true;
            }
        }
````

Replace:

````rust
                self.arithmetic = true;
            }
````

with:

````rust
                self.arithmetic = true;
                self.plain = true;
                self.plain_len = 0;
            }
````

Replace:

````rust
    fn nested_byte(&mut self, b: u8, last: u8) {
        match b {
````

with:

````rust
    fn nested_byte(&mut self, b: u8, last: u8) {
        if self.plain && self.nested == 2 {
            self.plain_byte(b);
        }
        match b {
````

Replace:

````rust
                self.inner_len = self.inner_len.saturating_add(1);
            }
        }
    }
````

with:

````rust
                self.inner_len = self.inner_len.saturating_add(1);
            }
        }
    }

    /// A byte of an arithmetic command before its first `)`: whether what
    /// it holds still reads the same as a subshell's command.
    fn plain_byte(&mut self, b: u8) {
        if b.is_ascii_alphanumeric() || b == b'_' {
            if let Some(slot) = self.plain_word.get_mut(self.plain_len) {
                *slot = b;
            }
            self.plain_len = self.plain_len.saturating_add(1);
            return;
        }
        let len = core::mem::take(&mut self.plain_len);
        let word = self.plain_word.get(..len).unwrap_or(b"");
        if len > KEYWORD_MAX || KEYWORDS.contains(&word) {
            self.plain = false;
        }
        if !matches!(
            b,
            b' ' | b'\t' | b')' | b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b',' | b'.' | b'!'
        ) {
            self.plain = false;
        }
    }
````

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p shell`

Expected: PASS: 406 tests.

- [ ] **Step 6: Lint**

Run: `cargo xtask lint`

Expected: Every `== cargo …` step succeeds: formatting is clean and clippy reports no warnings.

- [ ] **Step 7: Commit**

````bash
git add crates
git commit -F - <<'EOF'
fix(shell): read on after two subshells read as arithmetic

When a dropped `((`'s first `)` is not followed by another, bash reads
two subshells (`((a); b)`), which the drop scan had read as arithmetic,
so it dropped the rest of the input (milestone 4's deferred minor M-2;
the safe side, but only a here-document needs it). When what the `((`
held reads the same as a subshell's command (no quote, `$`, redirection,
separator, parenthesis or word that could be a keyword), the scan now
reads on with the outer subshell open, as bash does; otherwise it drops
the rest, as before.
EOF
````


### Finish PR 4

- [ ] **Run the gate, push and open the pull request**

````bash
cargo xtask ci
````

Expected: Lint, unit tests and e2e all succeed; the last line is `all 50 scenario(s) passed`.

````bash
git push -u origin m5p1/scan
gh pr create --base main --head m5p1/scan --title "fix(shell): mend the drop scan's gaps left by milestone 4" --body-file - <<'EOF'
## What

Milestone 5, plan 1, tasks 22–24: the drop scan reads quotes inside `$(…)` and `${…}` in double quotes as bash does; it keeps a command name's place after `time`'s options and after `coproc` and its name, so a group there is dropped whole; and it reads on after `((a); b)`, two subshells to bash, when what the `((` held reads the same as a subshell's command (milestone 4's deferred minor M-2).

## How it was tested

- [x] `cargo xtask ci` passes locally
- [x] New behaviour is covered by tests

## Hardware

- [x] Not needed: nothing it changes is particular to the NUC; milestone 5's plan 4 runs NUC checks 3 to 7
EOF
````

Expected: `gh` prints the pull-request URL.

- [ ] **Bind the pull request and wait for CI**

Bind the pull request to the session (the harness's `bind_pr`), whose monitor reports CI; never poll it. When `lint`, `unit` and `e2e` are green, go on. If a check fails, read its log, fix it on the branch and push again. CodeQL also runs; for each new alert, decide whether it is real, and ask the user before dismissing any.

- [ ] **Hand over for review**

Ask the user to review and merge. Do not merge it yourself. After the merge:

````bash
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os worktree remove /home/maw/src/bin-inc/relay-os-impl/worktrees/m5p1-scan
git -C /home/maw/src/bin-inc/relay-os-impl/relay-os fetch origin
````
