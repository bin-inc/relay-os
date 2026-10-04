# Contributing

## Commit messages

Every commit follows [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/),
in the form [release-please](https://github.com/googleapis/release-please)
reads (the repository runs no release automation; the format keeps that
open):

```
type(scope): description

Body: why the change is made, wrapped at 72 columns.

Refs: review M3
```

The first line is the subject. A blank line separates it from the body, and
another from the footers. Only the subject is required.

### Subject

- `type` is one of the types below, and `scope` one of the scopes below.
- `description` is in the imperative mood ("add", "fix", "remove", not
  "adds", "added" or "X does Y"), starts in lowercase and ends without a full
  stop.
- The whole subject is at most 72 characters. What does not fit goes into
  the body.
- A breaking change puts `!` before the colon: `feat(relay-abi)!: …`.

### Types

| Type | For |
|---|---|
| `feat` | New behaviour |
| `fix` | A bug fix |
| `perf` | Faster or smaller, same behaviour |
| `refactor` | Code changes with the same behaviour, including removing dead code |
| `test` | Tests only: unit tests, e2e scenarios, NUC check scripts and their transcripts |
| `docs` | Documentation only, including `.github/pull_request_template.md`, `.github/ISSUE_TEMPLATE/`, `.github/FUNDING.yml` and the licence files |
| `build` | Build system, toolchain, image layout |
| `ci` | `.github/workflows/` and `.github/dependabot.yml` |
| `chore` | Anything else; a release is `chore(release): 0.4.0` |
| `revert` | Undoing an earlier commit |

The type names the commit's main change. Tests that come with a feature or
a fix belong to that commit and take its type (`feat` or `fix`), not `test`.
Dependabot's pull requests keep its own prefixes (`chore(deps)`,
`chore(gha)`), toolchain bumps included.

### Scopes

The scope is the module the commit changes: its directory or crate name,
in lowercase.

| Scope | Path |
|---|---|
| `boot` | `boot/` |
| `kernel` | `kernel/` |
| `boot-info`, `crc32`, `elf`, `ext2`, `heap`, `relay-abi`, `relay-rt`, `shell`, `sysimg`, `term`, `usb`, `vfs` | `crates/<scope>/` |
| `sh` | `userland/sh/` |
| `utils` | `userland/utils/` |
| `userland` | `userland/tests/`, or changes across userland |
| `xtask` | `xtask/`, but not the check transcripts |
| `e2e` | `tests/e2e/` |
| `checks` | NUC check scripts and transcripts (`rootfs/root/checks/`, `xtask/fixtures/checks/`), `docs/hardware-test.md` |
| `rootfs` | `rootfs/`, but not the check scripts |
| `spec`, `plan` | `docs/superpowers/specs/`, `docs/superpowers/plans/` (but not the roadmaps) |
| `roadmap` | `docs/superpowers/plans/*-roadmap.md` |
| `release` | Version bumps |
| `deps`, `gha` | Dependency and GitHub Actions updates (Dependabot's prefixes) |

- A commit that changes two modules names both, comma-separated and without
  a space: `feat(boot,kernel): …`. A commit that changes more than two is
  split, unless it really is repo-wide.
- A repo-wide change leaves the scope out: `ci: …`, `chore: …`,
  `docs: …`. So does a change to the top-level files (`README.md`,
  `AGENTS.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `LICENSE-*`) or to the `.github/` templates, which belong to no module:
  `docs: …`.

### Body

- Explains why, and what the diff does not show. It does not repeat the
  subject.
- Wrapped at 72 columns.

### Footers

Footers use git's trailer form, `Token: value`, one per line:

- `BREAKING CHANGE: description` for every change marked `!`. In this repo
  that is a change to the system-call ABI (`crates/relay-abi`: call numbers,
  error numbers, result encoding), to the on-disk or `system.img` format, or
  to the loader → kernel hand-off in `crates/boot-info`.
- `Refs: review M3` or `Refs: #79` for review notes and issues.

No attribution lines (`Co-authored-by:`, "Generated with …").

### Reverts

The subject is `revert: ` and the reverted subject. The body names the
commit: `This reverts commit <sha>.` Edit git's default `Revert "…"` into
this form.

### Examples

Commits from earlier history, rewritten in this format:

| Before | After |
|---|---|
| `Kernel: process 1 is init, which starts /bin/sh and starts it again, and shows the error screen when the machine cannot run its shell` | `feat(kernel): run /bin/sh from init as process 1` |
| `Error screen: every other process is killed before it draws, so nothing writes over it` | `fix(kernel): kill every other process before the error screen draws` |
| `usb: a connection that kept bouncing for the debounce's 2 s is not tried again (UsbError::Unstable)` | `fix(usb): stop retrying a connection that bounces through the debounce` |
| `kernel: what only the in-kernel shell or nothing used goes (arg_bytes, …)` | `refactor(kernel): remove code that only the in-kernel shell used` |
| `e2e: the before-boot step system-drop, and the system_nosh scenario: …` | `test(e2e): add the system-drop step and the system_nosh scenario` |
| `Check scripts: the transcripts of NUC checks 3 and 4 on 0.3.0, and their row in the results log` | `test(checks): record the NUC transcripts of checks 3 and 4 on 0.3.0` |
| `CI: the unit job installs mtools, which the test of the ESP's files runs` | `ci: install mtools in the unit job for the ESP files test` |
| `docs: plan 5 of milestone 2, hardening and 0.3.0` | `docs(plan): add milestone 2 plan 5, hardening and 0.3.0` |
| `Version 0.3.0: milestone 2 is done (…)` | `chore(release): 0.3.0` |

A commit with a body and footers:

```
fix(kernel): start the error screen's clear with CAN

A program can leave a lone ESC on the console. Without CAN first, it
swallows the ESC that resets the colours, and the screen is drawn in
the program's colours.

Refs: review M3
```

A breaking change (made up):

```
feat(relay-abi)!: renumber the file system calls

BREAKING CHANGE: programs built against ABI 3 must be rebuilt; the
kernel refuses their ABI note.
```

## Pull requests

- One branch per pull request, named after the plan and its part
  (`m3p4/shell`) or the topic (`docs/…`, `perf/…`).
- `cargo xtask ci` passes locally before the branch is pushed.
- The pull request's title follows the same rules as a commit subject,
  since a squash merge makes it the commit on `main`. GitHub appends
  ` (#N)` to that subject, so keep the title within 65 characters. For a
  pull request that touches many modules, the title takes the main
  change's type and scope.
- A pull request that needs a NUC check (`docs/hardware-test.md`) is
  opened as a draft and stays one until the check has passed; its
  transcripts (copied off the stick and checked by `verify-usb`) and the
  results-log row go into a commit of that same pull request.
- Merge commits keep GitHub's default subject
  (`Merge pull request #N from …`).
- Plans in `docs/superpowers/plans/` write their commit messages in this
  format too.
