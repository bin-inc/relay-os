# Milestone 4 Roadmap

**Spec:** `docs/superpowers/specs/2026-10-02-programmable-shell-gate-design.md` (the programmable shell gate; milestone 4 is its first half, "Control flow", version 0.5.0)

**Status:** Plan 1 is done (#99–#101); plan 2 is planned and lands with these notes; plans 3 and 4 are written just before each runs.

Milestone 4 makes `/bin/sh` a shell one can program in: lists (`;`, `&&`, `||`, `!`, `&` mid-line), commands read across lines with bash's `> ` prompt, the compound commands `if`, `while`, `until` and `for`, and `test` and `[` as programs (spec §4–§6). It is split into the four plans of spec §12, named `m4-plan-1` to `m4-plan-4` (their files `docs/superpowers/plans/<date>-m4-plan-<n>-<name>.md`, their branches `m4p<n>/…`). Each one ends with software that can be tested by itself. Each plan is written just before it is executed, so it builds on the code that actually exists and on what the previous plan's checks showed.

```
Plan 1 ──► Plan 2 ──► Plan 3 ──► Plan 4
```

| Plan | Delivers | Ends with |
|---|---|---|
| 1 · Lists | The parse tree (`List`, `Item`, `AndOr`, `Pipeline`); `;`, `&&`, `\|\|`, `!` and `&` mid-line; a command read across lines after `\|`, `&&` or `\|\|`, in scripts, `X \| sh` and at the prompt with `> `; the bash corpus (`crates/shell/tests/corpus/`); `help`'s syntax lines; bash's reserved words refused where a command name stands, until plan 2 | Scenario `control`; no NUC check |
| 2 · Compound commands | `if`/`elif`/`else`, `while`, `until`, `for`, their reserved words turned into grammar and the rest still refused (§4.2); the 32-level nesting bound; Ctrl-C checked between commands; the trace of a construct; compound commands refused in a pipeline or with `&` | Scenario `control` extended |
| 3 · `test` and `[` | `/bin/test` and `/bin/[`, one command function under two names, GNU's grammar in full, compared with the host's `/usr/bin/test` and `/usr/bin/[` (§6) | Scenario `test_cmd`; `system: 44 programs, ABI 3` |
| 4 · Hardening and 0.5.0 | `check6.sh` (§11.4); milestone 3's deferred minors; the spec's §15 up to date; version 0.5.0 | `cargo xtask ci` green; NUC checks 3–6 on a stick written by `flash --full`; milestone 4 done |

## Notes carried forward

- **CI and pull requests (all plans).** As in milestones 2 and 3: every pull request to `main` must pass `lint`, `unit` and `e2e`; every plan is split into PRs that are each green on their own, one branch and worktree per PR; new crates, user packages and scenarios are picked up by `cargo xtask ci`. Commits and PR titles follow `CONTRIBUTING.md`.
- **How plans are made (all plans).** As in `AGENTS.md`: every task prototyped in a clone under `tmp/m4p<n>/proto`, one commit per task, tagged `t1..tN` on a tag `p0` (`origin/main` plus the plan's first PR's docs); the plan generated and replayed from it by `tmp/m4p<n>/gen` (a copy of milestone 3's, `tmp/m3p3/gen`); an independent review of the prototype whose real findings become tasks of their own, each with a test that fails first; a spike first when the risk is "does everything still pass".
- **bash is the reference, in a pty for the prompt.** `tmp/m4p1/pty_bash.py` runs an interactive bash 5.2.21; the bash corpus (plan 1) runs host bash on every `cargo xtask unit`, and CI's ubuntu-24.04 has bash 5.2.21.
- **ABI 3 through milestone 4.** Nothing in milestone 4 changes the ABI; ABI 4 (the environment in `SpawnArgs`) is milestone 5's.
- **No NUC check before plan 4.** Plans 1–3 change only the shell and add programs; the stick keeps 0.4.0 until plan 4's `flash --full`, and plan 4's `check6.sh` runs lists, compound commands and `test` there.
- **Milestone 3's deferred minors** (`tmp/m3p4/plan-ledger-final.md`), for plan 4: `verify-usb` drops mcopy's reason for an unreadable `system.img`; the fixtures test lists every file of `rootfs/root/checks`, not only `*.sh`; the TLB scan reads comments, so one naming `INVPCID` outside `arch/` would fail it. The fourth, two commits of `docs/hardware-test.md` without the `checks` scope, is history and is ruled out.
- **Plan 1's deferred minors** (`tmp/m4p1/plan-ledger-final.md`), settled by plan 2 (spec §15 item 2): an error the whole text has but a line alone does not (`a |` then `! b |`), told a line late, goes with the parser that reads each line in its context; `$?` after Ctrl-C ends a line of several pipelines becomes bash's 130; `echo ${1A} | cat` giving 1, where bash gives 0, is a decided difference (§10). The fourth, six commit body lines of 73–74 columns, is history and is ruled out; plan 2's generator checks the width of every commit body line.
- **Plan 1's rulings that reach later plans:** a script's `if` written across lines ran its body, each refused line dropped and the next read afresh, until plan 2 makes those lines one construct; quotes do not continue across lines (`echo 'a` is an unterminated quote where bash shows `> `), which plan 4 adds to §10.
- **Plan 2 has no NUC check** (spec §15 item 2): the stick keeps 0.4.0, and plan 4's `check6.sh` runs compound commands on the NUC.
- **Plan 2 changes the kernel and `relay-rt`** (spec §15 item 2): `wait(0, WAIT_NOHANG | WAIT_CTRL_C)` asks for a raw Ctrl-C between the shell's own commands, and a line-mode Ctrl-C that reaches an ended group is kept for the next holder; the ABI stays at 3.
- **Plan 3 adds `grep -q`** (the maintainer, 2026-10-02): a condition's usual command, which `grep` (`-i`, `-v`, `-n`, `-c`) lacks.
- **Fixed outside the plans:** the xtask test `qmp::tests::an_event_cut_at_the_deadline_is_read_whole_later`, which failed in a checkout whose path is long, binds its fake QEMU's socket through a short path since #102.
- **Still out of the gate** (spec §14): command substitution, arithmetic, globbing, word splitting, functions, `case`, `break`, `continue`, `read`, here-documents, subshells; `PATH`, `CDPATH`; users, an editor, interrupts, floating point, networking, other architectures.
