# 0135: CI is the gate, and nothing gates on the development host

- Status: decided by the owner, 2026-10-03; reverses R12's local merge rule (0002, "The gate is `make gate`")
- Recorded by: the owner's session, with `.claude/goals/AMENDMENT-2026-10-03-owner-2026-09-chorus.md`
- Evidence: `.github/workflows/ci.yml` in the pull request that adds this record

## Decision

The owner wrote, on 2026-10-03: "Holdfast and chorus NEED to use the public CI and nothing local".
The repository was made public the same day, so GitHub Actions runs cost no minutes.

- `make gate` (`tools/gate.sh`) stays the one definition of the gate. `.github/workflows/ci.yml`
  installs what it needs on a fresh runner and runs it on `pull_request`, `push` to main, nightly on
  main and by hand.
- A pull request merges when that check (`make gate`) is green on its branch with main merged in,
  with the run's link in the pull request. The goals supervisor's drain merges chorus PRs on that
  check alone: it runs nothing and takes no lock on the host (the goals program's ADR 0036).
- No gate, tier, whole-workspace cargo build or test, or ESP-IDF image build runs on the
  development host. One crate's focused test on a built tree is the inner loop, not a gate.
- The identity scan's private term list comes from the repository secret
  `CHORUS_IDENTITY_TERMS_LIST`, written to the runner's temp directory and masked term by term before
  any other step prints. `tools/conventions/check-identity.sh` prints where a term is, never what. A
  fork's pull request has no secrets, so only there the identity step prints SKIPPED; on this
  repository's own events an empty secret fails the job.

## Why

The development host's disk was the gates' bottleneck: one degraded HDD, 100% busy, fsync p50
287 ms. chorus held 22.5 of the 74.5 heavy-lock hours on that host from 2026-10-01 17:00Z to
2026-10-03. The private repository's Actions quota ran out on 2026-10-01, which is why R12 made the
local run the gate; public, the quota no longer applies.

## Before going public

Scanned 2026-10-03: gitleaks over the whole history found nothing; the house term list matched only
a substring of a vendor's documentation URL; the private identity terms appear in commit author
metadata only, which the owner's other public repository already carries.
