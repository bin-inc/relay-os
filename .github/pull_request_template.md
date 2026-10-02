## What

<!-- One or two sentences. Name the plan and tasks, e.g. "Milestone 1, plan 1, tasks 4–5". -->

## Format

- [ ] The title and every commit subject follow `CONTRIBUTING.md` (`type(scope): description`)

## How it was tested

- [ ] `cargo xtask ci` passes locally (lint, unit tests, QEMU scenarios)
- [ ] New behaviour is covered by a unit test or an e2e scenario
- [ ] A plan: every task was replayed into a fresh clone

## Hardware

- [ ] Not needed: nothing it changes behaves differently on the NUC than in QEMU (boot, display, USB, storage, timing)
- [ ] Needed: `docs/hardware-test.md` check ___; this pull request stays a draft until it passes, and the transcripts and the results-log row land in it
- [ ] Done on the NUC: `docs/hardware-test.md` check ___, result recorded in its log
