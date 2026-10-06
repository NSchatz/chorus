# chorus

From-scratch multiroom and surround audio: Rust server, custom sync protocol, ESP32-S3 firmware.
BRIEF.md guides; it recommends, it does not dictate. This repository is public.

## Never

- Break a BRIEF.md 3.1 guardrail, or relax one (they may only be tightened): clean-room vs GPL
  (never open the source of GPL projects or the design files of CERN-OHL-S or GPL hardware; their
  docs, issues and specs only; permissive source may be read and cited, `docs/clean-room.md`); no
  eFuse burns on dev hardware; every timing claim backed by a measurement in
  `docs/measurements/`; monotonic clocks in the audio path; no em dashes anywhere, commit
  messages and PR bodies included.
- Flash, burn an eFuse, deploy to the homelab or install OTA on an installed speaker: those are
  the owner's actions. Nothing here sets `CHORUS_OWNER_AT_BENCH`.
- Write a personal name, address, LAN address, hostname or secret into a tracked file, commit or
  PR (conventions rule 19). Prose says "the owner".
- Justify a technology or toolchain by what is installed in a container; an absent toolchain is
  installed, not designed around.
- Run a gate, tier, whole-workspace cargo build or test, or image build on this host. Local runs
  are one crate's focused test or one `tools/conventions/check-*.sh`; a throwaway build's target
  directory goes where `tools/build-dir.sh` says.
- Treat "verify against the datasheet" items as truth, or let the Rust and C cores drift (they
  share `fixtures/`).

## The gate

CI is the gate (`docs/decisions/0140-ci-is-the-gate.md`). A pull request runs only what its
change touches (`make gate-changed`), in 2 minutes or less (a warning when over); the full gate
(`make gate`) runs nightly on main, and a red main or night gets a fix-forward task.
`make gate-fast` is the conventions checks alone. Rules and their checks: `docs/conventions.md`.

## Git

- One branch `task/<n>` and PR per task, in a worktree from `origin/main`, never this checkout.
- Subject `<area>: <summary>`, lower-case area, at most 100 characters (conventions rule 21).
- One fresh-context reviewer passes it; squash-merge without waiting for CI. Never rewrite history.

## Questions

Ask the owner with AskUserQuestion in this session and wait; never in an issue or plain text.
Propose before anything expensive to reverse; cheap decisions are made and noted in
`docs/decisions/`. When measurement contradicts BRIEF.md, record it in `docs/decisions/` and
correct the brief in the same change.

## Before writing code, and across repos

- Before writing new code, search shopkit's index:
  `git -C /workspace/shopkit show origin/main:docs/INDEX.md | grep -i <word>`; reuse or extend
  shopkit instead of writing it again.
- Work a sibling repo must do goes to that repo as an exact spec; read its card first:
  `git -C /workspace/<repo> show origin/main:CARD.md`.

## Details

- `docs/working-agreement.md`: the full working agreement and plan of record ("CLAUDE.md rule N").
- `docs/conventions.md`: every rule and the check that holds it.
- `docs/clean-room.md`: what may be read, and how sources are cited.
- `docs/release.md`: what a release carries.
- `deploy/README.md`: the server container.
