# Relay OS — Programmable Shell Gate Design (milestones 4 and 5)

- **Date:** 2026-10-02
- **Status:** Draft for the owner's review
- **Builds on:** the user-space gate (version 0.4.0,
  `docs/superpowers/specs/2026-09-29-user-space-gate-design.md`, cited below
  as "UG §n") and milestone 1 (`2026-09-26-milestone-1-boot-shell-fs-design.md`,
  cited as "M1 §n")
- **Branch:** `main`, one pull request at a time, as in the earlier gates

## 1. Intent

At 0.4.0 a script is a list of commands with arguments and variables: it
cannot decide or repeat anything, read a file as input, send errors
elsewhere or pass a value to a program. This gate makes `/bin/sh` a shell
one can program in: lists, `&&` and `||`, `if`, `while`, `until` and `for`,
`test`, redirection of every standard stream, and environment variables
that reach programs. bash 5.2 stays the reference, word for word.

The gate spans two milestones, each built like the earlier ones (a roadmap
of plans, a version, a NUC check).

### 1.1 What the owner specified

- The next gate is about 1–2 months of work and can be split into more than
  one milestone.
- Direction: a programmable shell, chosen over users and permissions, an
  editor and interactive comforts, kernel work (interrupts, floating
  point) and networking. No ARM or AMD work in this gate.
- Only the core language: none of `$(…)`, `$((…))`, globbing, functions,
  `case`, `read` or here-documents yet.
- `cd` changes only where the design calls for it, which is `cd -` and
  `HOME`, `PWD` and `OLDPWD` with the environment.
- The check scripts must pass when started from any directory (today
  `check3-a.sh` run from `/root/checks` gets 82 of 84 commands as
  expected: `t-files cwd` and `t-files gone` print `/root/checks`).

### 1.2 Decisions made during brainstorming

| Topic | Decision |
|---|---|
| Word splitting | Kept out: expansion never splits words (UG §9.4, §16 item 10). `L="a b"; for x in $L` loops once, where bash loops twice. |
| `test` and `[` | Programs in `/bin`, one command function under two names, with GNU coreutils' messages (§6). |
| Compound commands at the prompt | Allowed across lines, with bash's continuation prompt `> ` (§5.5). |
| Parser | A grammar tree with a reader that can say "not finished yet", replacing `Line` (§4). Chosen over a block reader in front of today's line parser. |
| Compound commands in a pipeline or with `&` | Refused: bash runs them in a subshell (§4.1). |
| Script trace | `+ <line>` per line as written, once, when the line is read (§5.4). |
| Ctrl-C | Ends the whole construct it is typed in (§5.2). |
| `/dev/null` | Added in milestone 5, as an in-kernel filesystem at `/dev` (§8.4). |
| Milestone split | Milestone 4 "Control flow" (0.5.0): the grammar, `test`. Milestone 5 "Redirection and environment" (0.6.0): redirection, ABI 4's environment, `/dev/null`, `cd`. |
| Cut line if time runs short | In order: redirection of compound commands (§7.4), `2>&1` and `1>&2` (§7.1), `unset` (§8.5). |
| Limits | A construct being read holds at most 64 KiB; nesting is at most 32 levels deep (§4.5). |

### 1.3 Assumptions (carried over unless stated)

- Only x86_64, one CPU core; drivers poll (UG §1.3).
- Single user, `root`; permission bits are not enforced.
- The system-call ABI holds no architecture detail and keeps the same
  values on every architecture (UG §3.1).
- bash 5.2 (the host has 5.2.21) and GNU coreutils are the references for
  every message and status; differences are decided and listed (§10).

### 1.4 Success criteria (definition of done)

**Milestone 4 (version 0.5.0):**

1. `cargo xtask ci` passes, including the new scenarios of §11.3.
2. Every script of the bash corpus (§11.2) prints the same output and ends
   with the same status under host bash 5.2 and under the shell's
   in-process runner.
3. On the NUC, with a stick written by `cargo xtask flash --full`, checks 3,
   4, 5 and the new check 6 pass and `cargo xtask verify-usb` reports their
   transcripts clean; check 6's transcript is QEMU's except its timing
   line.

**Milestone 5 (version 0.6.0):** all of the above, plus the milestone 5
scenarios of §11.3 and check 7, which is started from `/root/checks`; every
NUC transcript is recorded again.

## 2. Gate structure

| Milestone | Version | Delivers | Ends with |
|---|---|---|---|
| 4 · Control flow | 0.5.0 | The grammar tree; `;`, `&&`, `\|\|`, `!`, `&` mid-line; `if`, `while`, `until`, `for`; the `> ` prompt; `/bin/test` and `/bin/[`; the bash corpus | §1.4 items 1–3; NUC check 6 |
| 5 · Redirection and environment | 0.6.0 | `<`, `2>`, `2>>`, `2>&1`, `1>&2`, redirection of compound commands; ABI 4 (`SpawnArgs::env`); `/dev/null`; `export`, `unset`, `A=1 cmd`, `/bin/env`; `HOME`, `PWD`, `OLDPWD`, `cd -`; checks that pass from any directory | NUC check 7 |

Each milestone gets its own roadmap in `docs/superpowers/plans/`, written
when the milestone starts; §12 sketches its plans.

## 3. Components

```
relay-os/
├─ crates/
│  ├─ relay-abi/   CHANGED (M5)  SpawnArgs::env, VERSION 4
│  ├─ relay-rt/    CHANGED (M5)  the environment at entry, relay_rt::env
│  ├─ shell/       CHANGED       grammar tree and reader (M4), tree walker (M4),
│  │                             fd context (M5), export/unset/cd (M5);
│  │                             new commands test (M4) and env (M5)
│  └─ vfs/         CHANGED (M5)  a device filesystem for /dev
├─ kernel/         CHANGED (M5)  the environment on a new process's stack, /dev mounted
├─ userland/
│  ├─ utils/       CHANGED       test and [ (M4), env (M5)
│  └─ tests/       CHANGED (M5)  t-env
├─ rootfs/root/checks/  CHANGED  check6.sh (M4), check7.sh (M5), cd /root (M5)
└─ xtask/          CHANGED       test packed under two names (M4)
```

The principles of UG §3.1 and the crate policy of UG §3.2 are unchanged: no
new external crates, architecture detail only in the `arch` modules, and
every command function written once in `crates/shell/src/commands/`.

## 4. Milestone 4: the grammar

### 4.1 The tree

The parser (`crates/shell/src/parser.rs`) produces a tree that replaces
`Line` and `Pipeline`:

```
List      = AndOr { (';' | '&' | newline) AndOr } [';' | '&']
AndOr     = Pipeline { ('&&' | '||') newline* Pipeline }
Pipeline  = ['!'] Command { '|' newline* Command }
Command   = Simple | If | While | Until | For
If        = 'if' List 'then' List { 'elif' List 'then' List } ['else' List] 'fi'
While     = 'while' List 'do' List 'done'
Until     = 'until' List 'do' List 'done'
For       = 'for' NAME [ newline* 'in' Word* ] (';' | newline) newline* 'do' List 'done'
```

- `&&` and `||` have equal precedence and group left to right, as in bash.
  A newline after `&&`, `||` or `|` continues the command.
- `;` stops being refused. `&` ends a pipeline anywhere on a line (`a & b`
  runs `a` in the background, then `b`); the background job's text is what
  was typed of that pipeline. `! a &` is allowed, as bash allows it; its
  status is the background start's (UG §16 item 10).
- `&` applies only to a pipeline of simple commands. `a && b &`, a compound
  command with `&`, and a compound command in a pipeline (`if …; fi | cat`)
  are refused, `relay-sh: unsupported syntax: <what>` with status 2: bash
  runs them in a subshell, which would need a second shell carrying the
  variables.
- `for NAME; do` and `for NAME do` loop over `"$@"`. A `for` word list
  expands as arguments do (UG §16 item 10): without splitting, an unquoted
  empty expansion removing its word, `"$@"` giving each argument.
- A simple command is today's: words, assignments and redirections (`>`
  and `>>` in milestone 4). One of only assignments sets them (`A=1; echo
  $A`); in milestone 4 an assignment before a command's name stays refused
  (§8.5).

### 4.2 Reserved words and refused syntax

- `if then elif else fi while until for in do done !` are keywords only
  unquoted, as a whole word, where the grammar allows them: `echo if`
  prints `if`, and `"if"` is a command name, as in bash.
- bash's other reserved words and its two loop built-ins, where a command
  name could stand, are refused, `relay-sh: unsupported syntax: <word>`
  with status 2: `case`, `esac`, `select`, `function`, `time`, `coproc`,
  `[[`, `]]`, `{`, `}`, `break`, `continue`. (`break` and `continue` would
  otherwise run as commands that are not found, where bash's leave the
  loop.)
- An unquoted `*`, `?`, `` ` ``, `(` or `)` stays refused, and so do `$(`,
  `$((` and every parameter UG §9.4 refuses. `<` stays refused until
  milestone 5.

### 4.3 Incomplete input

`parse(text)` returns `Complete(List)`, `Incomplete` or an error.
`Incomplete` means the text is a valid start that needs more lines: an open
`if`, `while`, `until` or `for`, or a line ending in `&&`, `||` or `|`. The
same reader serves the prompt (§5.5), `sh FILE` and `X | sh`: lines are
added until the text is complete or wrong.

### 4.4 Syntax errors

- Errors use bash's words, prefixed `relay-sh: `, status 2:
  ``syntax error near unexpected token `fi'`` (an empty `then` or `do`
  body, a keyword out of place, a missing `then`), and
  `syntax error: unexpected end of file` when the input ends inside a
  construct.
- A syntax error discards the whole construct read so far; in a script the
  next line starts afresh, as a failing line does not stop a script (UG
  §8.3).
- Messages carry no line numbers, as today.
- Exactly which token each error names is what interactive bash 5.2 names;
  the parser's tests record each case against it (§11.1).

### 4.5 Bounds

What is typed or read is untrusted (AGENTS.md):

- A construct being read holds at most 64 KiB, as a script does (M1 §15
  item 12). Beyond it the construct is dropped:
  `relay-sh: the command would be longer than 64 KiB`, status 2.
- Nesting is at most 32 levels deep, counting each compound command and
  each `&&`/`||` chain inside one: `relay-sh: unsupported syntax: more
  than 32 levels of nesting`, status 2. The parser and the walker recurse,
  and `/bin/sh` runs on a fixed stack.
- A loop's passes are not bounded (§5.2).

## 5. Milestone 4: execution

### 5.1 Statuses

The tree walker lives in `Shell`, so `/bin/sh`, the in-process runner
(`host-shell`) and the unit tests share it; today's code for a simple
command and a pipeline becomes its leaves.

- `if`: the status of the last command of the branch taken, 0 when none
  runs.
- `while` and `until`: the status of the body's last command, 0 when the
  body never runs. `for` likewise, and it runs no pass over no words.
- `&&` and `||` run the next pipeline only on success or failure; the
  status is the last pipeline's that ran. `!` makes 0 into 1 and anything
  else into 0.
- A condition's status sets `$?` like any other command's.
- `for`'s variable is a shell variable: set before each pass, it keeps its
  last value after the loop and counts in the 64 KiB of variables
  (UG §16 item 10).
- `exit` anywhere stops the shell at once.
- An expansion error (`bad substitution`, a line expanding past 64 KiB)
  ends the whole top-level command it is in, status 1, as interactive bash
  abandons the whole construct. A redirection that cannot be made fails
  only its own command, status 1, as in bash.

### 5.2 Ctrl-C

- Ctrl-C ends the running program (status 130) and every construct around
  it, up to the top-level command.
- The walker also checks for a Ctrl-C before each command, so a loop of
  built-ins only (`while cd; do cd; done`) ends too, printing `^C`, status
  130.
- In a script it ends the script, as today (UG §8.3).
- A loop a person writes may run forever: the rule that nothing loops
  forever (AGENTS.md) holds because Ctrl-C always ends it.

### 5.3 Sync and transcript

Every simple command is still followed by a sync and, in a script, a write
of the transcript (UG §8.3), so a command that hangs inside a loop leaves
the disk and the transcript up to date. A sync with nothing to write costs
little; check 6 measures a loop of 100 commands on the NUC (§11.4).

### 5.4 The script trace

Each line of a script is traced `+ <line>`, as written and trimmed, once,
when the reader takes it (M1 §15 item 12). A construct's lines are all
traced before it runs:

```
+ for x in a b
+ do echo $x
+ done
a
b
```

bash's `set -x` traces each expanded command instead (`+ echo a`); that is
not copied (UG §16 item 10 keeps the M1 form). Blank and comment lines are
not traced, inside a construct or not.

### 5.5 The `> ` prompt

- While the reader says `Incomplete`, the interactive shell shows bash's
  `> ` and reads the next line, each edited on its own (no going back to an
  earlier line).
- Ctrl-C at `> ` drops the construct: status 130, then a new
  `root@relay:…# ` prompt.
- The end of input at `> ` is `syntax error: unexpected end of file`,
  status 2.
- Finished background jobs are reported at the next full prompt only.

## 6. Milestone 4: `test` and `[`

### 6.1 One command, two names

`crates/shell/src/commands/test.rs` holds the command function; `COMMANDS`
lists it as `test` and as `[`, so both run in the shell's tests, in
`host-shell` and as programs. If Cargo refuses `[` as a binary's name,
xtask packs the `test` ELF into `system.img` under both names (the archive
allows the name, `crates/sysimg` `valid_name`). Called as `[`, its last
argument must be `]`: otherwise GNU's `[: missing ']'`, status 2. GNU's
messages are taken in the C locale, with plain quotes, as every command's
are.

### 6.2 Expressions

GNU coreutils' grammar in full, since a partial one would differ silently:

- POSIX's rules by argument count for 0 to 4 arguments, then `!`, `-a`,
  `-o`, `(` and `)` with GNU's precedence. The shell refuses an unquoted
  `(`, so one writes `\(` or `'('`, as in bash.
- **Strings:** one argument (true if not empty), `-n`, `-z`, `=`, `==`,
  `!=`, and `\<` and `\>` comparing bytes, as in the C locale.
- **Integers:** `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge`, parsed as GNU
  parses them: blanks around the number, an optional sign, any length,
  compared as digit strings so nothing overflows. A bad one is GNU's
  `test: invalid integer 'x'`, status 2 (bash's built-in says
  `integer expression expected`; ours is the program's).
- **Files**, answered from `stat` (`relay_abi::Stat`): `-e`, `-f`, `-d`,
  `-s`, `-b`, `-c`, `-p`, `-S`, `-h`, `-L`, `-g`, `-u`, `-k`, `-O`, `-G`,
  `-N`, `-t FD`, and `-nt`, `-ot`, `-ef`.
  - `-r`, `-w` and `-x` answer as GNU does for root: `-r` is true if the
    file exists, `-w` too except on a read-only filesystem (so
    `test -w /bin/ls` is false), `-x` if any execute bit is set or it is a
    directory.
  - `-t FD` is true for the console.
  - Symbolic links are never followed (UG §16 item 4), so `-e`, `-f` and
    `-d` of a link look at the link itself, where GNU's follow it (§10).
- 0 is true, 1 false, 2 an error. `[ --help` and `[ --version` are refused
  as other commands refuse an unknown option (GNU's first message line,
  status 2); `test --help` is one non-empty string, true, as in GNU.

## 7. Milestone 5: redirection

### 7.1 Syntax

An optional fd, 0, 1 or 2, before each operator:

| Form | Does |
|---|---|
| `< f`, `0< f` | `f` opened for reading as fd 0 |
| `> f`, `1> f`, `>> f`, `1>> f` | as today, on fd 1 |
| `2> f`, `2>> f` | the same on fd 2 |
| `2>&1`, `1>&2`, `>&2` | one fd made a copy of the other as it is at that point |

Refused, `relay-sh: unsupported syntax: <what>` with status 2: any other fd
number (`3>`, as today), `>&-`, `&>`, `>&file`, `<&`, `<>`, `>|`, `<<` and
`<<<`.

### 7.2 Several per command

- A command may have any number of redirections, made left to right as
  bash makes them: `> f 2>&1` sends both outputs to `f`; `2>&1 > f` sends
  fd 2 where fd 1 was and fd 1 to `f`.
- A file that a later redirection replaces is closed at once, so a command
  holds at most three redirected files open.
- A target is expanded as today; an unquoted one that expands to nothing is
  ``ambiguous redirect``, status 1. A missing file for `<` is
  `relay-sh: f: No such file or directory`, status 1, and the command does
  not run.
- A command of only redirections makes or opens its files and runs nothing
  (status 0), as bash's does.

### 7.3 In a pipeline

- `<` may stand only on the first command, `>` and `>>` only on the last
  (today's rule for `>`); elsewhere they are refused as today's `>` is.
- `2>`, `2>>` and `2>&1` may stand on any command. `a 2>&1 | b` sends both
  of `a`'s outputs into the pipe, which is bash's `|&`; `|&` stays refused.

### 7.4 On compound commands

`for …; done > f`, `if …; fi 2> err` and `while …; done < f` apply to
everything inside. The walker carries an fd context (what fds 0, 1 and 2
are at this point) down the tree; every spawn and built-in inside uses it.
The shell's own fds never change, so it needs no `dup` call. This is the
first thing cut if time runs short.

### 7.5 Built-ins and scripts

- `Ctx` gains an error target beside its output, so built-ins honour `2>`
  (`cd /nope 2> err` writes the message to `err`) as well as `>`.
- Output sent to a file reaches neither the screen nor a script's
  transcript.
- A command whose standard input is a file still gets the console's
  foreground (UG §6.4), so Ctrl-C reaches it.

## 8. Milestone 5: the environment

### 8.1 ABI 4

- `SpawnArgs` gains `env` and `env_len`: `NAME=value` entries, each
  followed by a NUL, in the order the caller gives them. The layout
  changes, so `relay_abi::VERSION` becomes 4 and the change is marked `!`
  with a `BREAKING CHANGE:` footer (CONTRIBUTING.md). `t-abi` is built for
  ABI 3, and the startup line says `ABI 4`.
- The kernel checks the block: at most 64 KiB (`E2BIG`), counted apart
  from the arguments' 64 KiB (UG §5.3); not empty and not ending in a NUL
  is `EINVAL`. It copies the block to the top of the new stack beside the
  arguments. The plan checks the user stack keeps room for both.

### 8.2 Entry state

UG §5.3 gains three registers: `rcx` holds the address of the environment
block, `r8` its length and `r9` the number of entries (zero for none).
They are the fourth to sixth argument registers of the entry function, as
`x3`–`x5` will be on aarch64, so the ABI's shape stays the same on every
architecture (UG §7.1).

### 8.3 `relay-rt`

`_start` passes the block to the runtime, which keeps it in a static;
`relay_rt::env::var(name)` and `relay_rt::env::vars()` read it. No
program's `main` changes. The test double in `relay-rt`'s `testing` module
gains an environment.

### 8.4 `/dev/null`

- A filesystem in the kernel with one node, `null`: a read returns the end
  of input at once, a write succeeds and keeps nothing, `seek` succeeds and
  stays at 0, `stat` says a character device. It is mounted at `/dev` (a
  mount point need not exist on the disk, UG §4.4).
- Its startup line is `[ ok ] dev: /dev/null`, after `mount /`. `df` lists
  it as `/dev`. Both ripple into the scenarios and the check scripts'
  expectations (§11.5).

### 8.5 The shell's variables and the environment

- **Import.** A shell takes its environment as exported variables, in the
  limits of UG §16 item 10. An entry whose name is not a valid name is
  dropped (§10).
- **`export`.** `export NAME[=value]...` marks variables for export, with
  an assignment's expansion (UG §16 item 10). `export` alone and
  `export -p` list the exported variables, sorted, as bash's
  `declare -x NAME="value"` (and `declare -x NAME` for one without a
  value). A bad name is bash's ``export: `1A=x': not a valid identifier``,
  status 1. `export -n` and `export -f` say `not supported`, status 1.
- **`unset [-v] NAME...`** removes variables, exported or not; `unset -f`
  says `not supported`.
- **Children** get exactly the exported variables, in a fixed order (that
  of export). So `sh FILE`, `X | sh` and a nested `sh` start with the
  exported variables: UG §16 item 10's "a script starts with none" becomes
  "a script starts with the exported ones", as in bash.
- **`A=1 cmd`** stops being refused (UG §16 item 10). The assignments,
  expanded left to right before the command's words, go into that
  command's environment only. Before a built-in they hold while it runs
  and are then dropped, as in bash's default mode. They may stand on any
  command of a pipeline and before a background job. Assignments alone in
  a pipeline or with `&` stay refused. `NAME+=value` stays refused.
- **Variables the system sets.**
  - init starts `/bin/sh` with the environment `HOME=/root`.
  - The shell sets and exports `PWD` (from `getcwd`) when it starts, and
    exports `OLDPWD` without a value until the first `cd`, as bash does.
  - `SHLVL`, `_` and `PATH` are not set. Commands are still looked up in
    `/bin` only; a `PATH` one sets changes nothing (§10).
  - `~`, alone or before `/`, and the prompt's `~` stand for `$HOME`, and
    for `/root` when `HOME` is unset, as bash falls back to the password
    file. An empty `HOME` makes `~` empty, as in bash.
- **Limits.** The exported variables' block is at most 64 KiB; past it a
  command fails before it starts: `relay-sh: <command>: Argument list too
  long`, status 126, bash's words for `E2BIG`.

### 8.6 `/bin/env`

GNU's `env [-i] [-u NAME]... [NAME=value]... [COMMAND [ARG]...]`, without
`PATH`:

- With no command it prints its environment, one entry per line, in the
  block's order.
- `-i` starts from an empty environment, `-u NAME` removes one entry,
  `NAME=value` sets one.
- A command is `/bin/COMMAND`, or the path as given if it holds a `/`; one
  that is not found ends `env` with 127, one that cannot run with 126, with
  GNU's messages.
- Other options are refused with the first line of GNU's message,
  status 125.

## 9. Milestone 5: `cd`, and checks from any directory

### 9.1 `cd [-L|-P] [--] [dir]`

| Typed | Does |
|---|---|
| `cd` | goes to `$HOME`. `HOME` unset: `relay-sh: cd: HOME not set`, status 1. `HOME` empty: stays, status 0. With init's `HOME=/root` this is today's behaviour. |
| `cd -` | goes to `$OLDPWD` and prints the new directory on standard output. `OLDPWD` unset: `relay-sh: cd: OLDPWD not set`, status 1. Empty: stays, prints nothing, status 0. |
| `cd ""` | stays, status 0 (as today) |
| `cd -L dir`, `cd -P dir` | accepted; the same, as symbolic links are never followed |
| `cd -e`, `cd -@` | `relay-sh: cd: -e: not supported`, status 1 (bash accepts them) |
| `cd -x` | bash's `relay-sh: cd: -x: invalid option`, status 2, without the usage line |
| `cd a b` | `relay-sh: cd: too many arguments`, status 1 (as today) |

- On success `OLDPWD` takes the old `PWD` and `PWD` the new directory from
  `getcwd`, which is canonical: `cd /root/../tmp` sets `PWD=/tmp`, as
  bash's does without symbolic links. A failed `cd` changes neither.
- `CDPATH` is ignored (§10). The prompt keeps showing `getcwd`, not `$PWD`
  as bash's `\w` does, so `PWD=x` typed by hand does not change it (§10).

### 9.2 Checks that pass from any directory

- Every check script (`check3-a.sh`, `check3-b.sh`, `check4.sh`,
  `check5.sh`, `check6.sh`, `check7.sh`) starts with `cd /root`. A
  script's `cd` stays in it, so the caller's directory does not change.
- The scripts' expectations stay as they are; their transcripts gain a
  `+ cd /root` line. The QEMU transcripts change with the scripts; the NUC
  transcripts are edited by hand until milestone 5's NUC check records
  real ones, as milestone 3's plan 1 did for `ABI 3`.
- The `checks` scenario runs `check3-a.sh` from `/root/checks` and the
  others from `/root`. `docs/hardware-test.md` keeps starting checks 3 to
  6 at `~` and starts check 7 in `/root/checks` (§11.4).

## 10. Decided differences from bash

Each is documented where it applies and in the shell crate's module
comments:

- Expansion never splits words (§1.2).
- The script trace shows lines as written, once (§5.4).
- Compound commands in a pipeline or with `&`, and `a && b &`, are refused
  (§4.1).
- `SHLVL`, `_`, `PATH` and `CDPATH` are not set or used (§8.5, §9.1); the
  prompt shows `getcwd`, not `$PWD` (§9.1).
- An environment entry without a valid name is dropped, where bash passes
  it on (§8.5).
- `test`'s file operators do not follow symbolic links (§6.2).
- `:` is not a built-in; `true` is the program for it (§14).

## 11. Testing

### 11.1 Host unit tests

- **Parser:** every rule of §4.1, keywords in and out of place (`echo if`,
  `"if"`), each refused word of §4.2, the `Incomplete` cases, both bounds
  of §4.5, and each syntax error, its message compared with interactive
  bash 5.2 in a pty.
- **Walker:** over the in-memory `Vfs`: the statuses of §5.1, nesting,
  `for`'s variable after the loop, `exit` deep inside, Ctrl-C at each
  depth, `&` mid-line, an expansion error ending the top-level command.
- **`test`:** every case of §6.2, word for word and status for status with
  the host's `/usr/bin/test` and `/usr/bin/[` (GNU's, not bash's
  built-in), including `[ -n ]`, `[ ! = ]`, `[ -a -a -a ]` and the errors.
- **Milestone 5:** `relay-abi`'s new offsets and `VERSION == 4`; the
  kernel library's checks of the block and its place on the stack;
  `/dev/null`'s calls; the fd context and a replaced file's close; built-ins
  honouring `2>`; `export`, `unset`, `A=1 cmd`, `cd` and the variables of
  §8.5, compared with `env -i HOME=/root bash` (ignoring `SHLVL` and `_`,
  and comparing `env`'s lines as a set, since bash prints them in hash
  order).
- **Mutation checks** for each guard: the nesting and 64 KiB bounds, the
  Ctrl-C check between commands, `[`'s `]`, the environment's size and NUL
  checks, the close of a replaced file, a failed `cd` leaving `PWD`.

### 11.2 The bash corpus

`crates/shell/tests/corpus/` holds small scripts; a test runs each under
host bash 5.2 and under the in-process runner and compares their output
and status (the trace is not compared: bash prints none without `set -x`).
The scripts use only commands that follow GNU word for word (`echo`,
`test`, `[`, `true`, `false`, `seq`, `cat`, `grep`, `wc`) and no unquoted
value holding a blank. Milestone 4 adds lists, the compound commands and
`test`; milestone 5 adds redirection order, redirection of compound
commands, export into a nested `sh` and `cd -`. A missing bash fails the
test (AGENTS.md).

### 11.3 QEMU scenarios

| Milestone | Scenario | Covers |
|---|---|---|
| 4 | `control` | `if` and `for` typed across lines with the `> ` prompt; Ctrl-C at `> ` and `$?` 130; Ctrl-C in `while true; do sleep 1; done` and in a loop of built-ins; a script with nested loops and its transcript |
| 4 | `test_cmd` | `/bin/test` and `/bin/[` on the real disk: `[ -w /bin/ls ]` false on the read-only `/bin`, `[ -t 0 ]` true on the console, `-nt` of two files made seconds apart |
| 5 | `redirect` | `<`, `2>`, `2>>`, both orders of `2>&1`, `a 2>&1 \| b`, `for …; done > f`, `> /dev/null`, `cat /dev/null` |
| 5 | `env_calls` | `t-env`: a child's environment, an empty one, `E2BIG`, `EINVAL`, a grandchild's through `sh` |
| 5 | `environment` | `env`, `env -i`, `env -u`, `export`, `A=1 env`, a script seeing exported variables, `cd -`, `cd` with `HOME` unset |

The `checks` scenario runs check 6 (milestone 4) and check 7 (milestone 5)
after check 5, and `check3-a.sh` from `/root/checks` (§9.2).

### 11.4 NUC checks

- **Check 6** (`check6.sh`, milestone 4, run after `check5.sh`): `if`,
  `elif`, `else`; `while` and `until` controlled by files; `for` over words
  and over `"$@"` (the script runs itself with arguments); the statuses of
  `&&`, `||` and `!`; `test` on the stick's files; and 100 commands as two
  nested `for` loops of 10 words, timed by `date` before and after (a
  regex line, so any time passes; the results log records it). Then, by
  hand: a `for` typed across three lines, Ctrl-C at `> `, and Ctrl-C in a
  running `while` loop.
- **Check 7** (`check7.sh`, milestone 5), started with `cd checks` and
  `sh check7.sh`: redirections with `/dev/null` and `[ -c /dev/null ]`;
  `export`, `env` with `-i` and `-u`, `A=1` before a command, a nested `sh`
  seeing exported values, `unset`; `cd -`, `PWD` and `OLDPWD`.
- `cargo xtask verify-usb` checks every transcript, as today.

### 11.5 What ripples

- **Milestone 4:** `/bin` gains `test` and `[` (42 → 44 programs:
  `system: 44 programs, ABI 3`), so the `system` scenario's `ls /bin`, the
  comment in `kernel/src/system.rs` and `docs/hardware-test.md` change. `[`
  sorts before `cat` in the C locale, so check 4's `ls /bin` lines change,
  and `help` gains two lines. Tests that assert `;` or `a & b` are refused
  become tests of what they do now.
- **Milestone 5:** `env` and `t-env` (46 programs, `ABI 4`); the startup
  line of `/dev`; `df`'s `/dev` line; the `+ cd /root` lines; version
  0.6.0 in `uname -a` (AGENTS.md lists where).

## 12. Build order

Each milestone's roadmap fixes its plans; this is the expected split.

| Milestone | Plan | Delivers |
|---|---|---|
| 4 | 1 · Lists | The tree, `;`, `&&`, `\|\|`, `!`, `&` mid-line; the `Incomplete` reader for the prompt (`> `), `sh FILE` and `X \| sh`; the corpus harness |
| 4 | 2 · Compound commands | `if`, `while`, `until`, `for`; reserved and refused words; the nesting bound; Ctrl-C between commands; the trace of a construct; the `control` scenario |
| 4 | 3 · `test` and `[` | The command under both names; the `test_cmd` scenario |
| 4 | 4 · Hardening, 0.5.0 | `check6.sh`; milestone 3's deferred minors (`tmp/m3p4/plan-ledger-final.md`: `verify-usb`'s reason for an unreadable `system.img`, the fixtures test listing files that are not `.sh`, the TLB scan reading comments; the two commit scopes are history and are ruled out); this spec's decision log; NUC checks 3–6 |
| 5 | 1 · Redirection | The forms of §7, the fd context, built-ins' error target, redirection of compound commands; the `redirect` scenario (without `/dev/null`) |
| 5 | 2 · ABI 4 and `/dev` | `SpawnArgs::env`, the entry registers, `relay_rt::env`, init's `HOME`, `t-env` and `env_calls`; `/dev/null` and the `redirect` scenario's `/dev/null` lines |
| 5 | 3 · The shell's environment | `export`, `unset`, `A=1 cmd`, `PWD`, `OLDPWD`, `HOME`, `~` and the prompt following `HOME`, `cd`, `/bin/env`, `cd /root` in the check scripts; the `environment` scenario |
| 5 | 4 · Hardening, 0.6.0 | `check7.sh`; the deferred findings; NUC checks 3–7 with every transcript recorded again |

## 13. Risks and mitigations

| Risk | Mitigation |
|---|---|
| The parser rewrite breaks what works at 0.4.0 | Plan 1 moves every existing parser and shell test onto the tree before adding syntax; `cargo xtask ci` passes at each task |
| bash's error tokens are hard to predict | Every syntax error is compared with interactive bash 5.2 in a pty (§11.1); none is written from memory |
| A sync after every command makes loops slow on the USB stick | Check 6 times 100 commands on the NUC; if it is too slow, a later plan syncs once per top-level command and after each program instead, recorded in §15 |
| Recursion overflows `/bin/sh`'s stack | The nesting bound (§4.5), with a test at the bound under `/bin/sh` in QEMU |
| A cut line is needed | §1.2's order; nothing else depends on the cut items |
| The environment block and the arguments do not both fit the new stack | Milestone 5's plan 2 measures it and grows the stack if needed, with a test at both 64 KiB limits at once |

## 14. Out of scope for this gate

- Command substitution (`$(…)`, `` `…` ``), arithmetic (`$((…))`; the
  first candidate for the next gate, since a loop cannot count without
  it), globbing, word splitting.
- Functions, `case`, `break`, `continue`, `read`, `:`, here-documents,
  subshells (`( … )`, compound commands in pipelines or with `&`).
- `PATH`, `CDPATH`, `SHLVL`, `set` and its options, `trap`, `eval`,
  `source`/`.`.
- Users and permissions, an editor, interrupt-driven I/O, floating point
  in programs, networking, other architectures.
- Everything UG §15 and M1 §14 list that this document does not add.

## 15. Revisions made during planning

Changes made while planning either milestone are recorded here, as UG §16
does.
