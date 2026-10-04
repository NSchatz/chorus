# 0140: CI is the gate, on every pull request, on main and nightly; a pull request merges when an independent reviewer passes it, without waiting for CI, and a red main gets a fix-forward task

- Status: decided by the owner, 2026-10-04; supersedes R12's merge rule (0002, "The gate is
  `make gate`") and item 8 of 0112 (CI started by hand)
- Recorded by: the owner's agent harness, in the pull request that adds this record. It carries
  over the CI part of PR #135 (closed unmerged; its draft of this record was 0135), rewritten to
  the policy of 2026-10-04.
- Implemented in: `.github/workflows/ci.yml`; `CLAUDE.md`, `README.md` and `docs/conventions.md`
  (rules 19 and 21) say the same

"The gate" keeps its meaning: `make gate`, every check chorus holds a change to. What changes is
where it runs (CI, not the development host) and what a merge waits for (a review, not a run).

## Decision

1. **`make gate` stays the one definition of the gate, and CI runs it.**
   `.github/workflows/ci.yml` installs what the gate needs on a fresh runner and calls it, nothing
   more, on every pull request, on every push to main, nightly on main (07:41 UTC) and by hand.
   Every run is the full tier: `make tier-full` is `make gate`.
2. **A pull request merges when an independent reviewer passes it.** The merge does not wait for
   CI: the run on the pull request is evidence the reviewer can read, not a merge condition, and
   CI runs again on main after the merge.
3. **A red main gets a fix-forward task.** A red run on main, after a merge or nightly, is fixed
   by a new change that makes main green again. History is never rewritten.
4. **Nothing gates on the development host.** Local runs are narrow tests only: one crate's
   focused test on a built tree, or one conventions check. No gate, tier, whole-workspace cargo
   build or test, or image build runs there.
5. **The identity scan runs on CI from a secret.** Its private term list comes from the
   repository secret `CHORUS_IDENTITY_TERMS_LIST`, written to the runner's temp directory (never
   into the workspace) and masked term by term before any later step prints.
   `tools/conventions/check-identity.sh` prints where a term is, never what. A fork's pull request
   gets no secrets, so there and only there the identity step prints SKIPPED; on this
   repository's own events an empty secret fails the job.
6. **A red run explains itself.** On a failure the job uploads every gate step's whole log
   (`target/gate/*.log`, kept 7 days): the job log shows only a failing step's last 15 lines, and
   no local run reproduces it.
7. **Each change runs once.** `push` is main only, so a push to a pull request's branch starts
   only the `pull_request` run; the concurrency group is the branch's name (`main` for a push to
   main), and the nightly run has a group of its own, so a merge never cancels it.

## Why

- The policy is the owner's, adopted for chorus on 2026-10-04.
- A full run takes about 40 minutes on the runner: run 37162888240, the last on PR #135's head,
  ran from 2026-10-03 23:47:33Z to 2026-10-04 00:29:37Z and passed. A merge that waited for CI
  would wait that long; under this policy CI still runs on the pull request and again on main, and
  a red result is acted on by a fix-forward task instead of holding the merge.
- R12 made the local run the gate because GitHub Actions did not start jobs on the private
  repository (0002), and the account's minutes were spent on 2026-10-01 (this workflow's header at
  `535ed28`: 246 runs, 1,995 minutes that day). The repository is public since 2026-10-03, and
  standard GitHub-hosted runners are free in public repositories.
- The development host's disk was the bottleneck of every local gate run. PR #135's draft of this
  record measured one degraded HDD, 100 % busy, fsync p50 287 ms, and chorus holding 22.5 of the
  74.5 heavy-lock hours on that host from 2026-10-01 17:00Z to 2026-10-03.
- With nothing gating locally, the identity scan, the check a public repository needs most, would
  run nowhere unless CI has the term list.

## What was not chosen

- **PR #135's rule: a pull request merges when CI is green on its branch with main merged in.**
  Superseded by the owner's policy of 2026-10-04; every merge would wait about 40 minutes for a run.
- **R12: the local run is the gate.** It runs on the development host's disk, the bottleneck
  above.
- **`make tier-fast` on pull requests and the full tier only nightly.** Not taken: the workflow
  keeps main's one job, `make gate`, and since a merge does not wait for the run, a shorter
  pull-request run would buy no merge time.
- **`timeout-minutes: 120` on the job (in PR #135).** Left out: PR #135 gave no reason for it, and
  the job otherwise keeps exactly main's shape.

## What was read

All on 2026-10-04 unless dated otherwise.

- PR #135 at `4a7c919c`: its body, its `.github/workflows/ci.yml`, its change to
  `tools/endpoint-package.sh` (taken here unchanged: systemd 255 on the runner's Ubuntu 24.04
  writes "Unknown key name" where 256 and later write "Unknown key"), and its draft of this record
  (`docs/decisions/0135-ci-is-the-gate.md`), with the measurements quoted above and GitHub's
  Actions billing page as it read it on 2026-10-03,
  https://docs.github.com/en/billing/concepts/product-billing/github-actions (standard runners are
  free in public repositories).
- This repository's Actions run 37162888240 (`gh run view`: a `pull_request` run on PR #135's
  head, `success`, its start and end times) and its secrets (`gh secret list`:
  `CHORUS_IDENTITY_TERMS_LIST`, set 2026-10-03).
- In this repository at `535ed28`: `CLAUDE.md`, `README.md`, `docs/conventions.md` (the table,
  rules 8, 15, 19 and 21), `docs/decisions/0002-repository-layout-and-ci.md` (R12),
  `docs/decisions/0112-a-faster-gate-with-the-same-checks.md` (item 8), `tools/gate.sh` (its
  header and log directory), `.yamllint`, and `tools/conventions/check-adrs.sh`,
  `check-identity.sh`, `check-pins.sh`, `check-provenance.sh` and `check-workflows.sh`.
- actions/upload-artifact's tags (`gh api`): `v4.6.2` and `v4` are commit
  `ea165f8d65b6e75b540449e92b4886f43607fa02`, the pin PR #135 chose.
