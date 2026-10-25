# 0000: a workspace test past its limit is ended and fails, named: 60 s by default, 3x its measured time for a longer one

- Status: decided, 2026-10-25 (harness plan 201, task 206: per-test limits in every repository)
- Decided by: the owner (2026-10-25: "All three runners", a per-test limit of 60 s with a longer
  test given its own; "nextest at 5 minutes" and "nextest only" not chosen)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `.config/nextest.toml`, `tools/nextest-timeout-test.sh` (new), `Makefile`
  (`verify`), `docs/measurements/nextest-test-durations-ci.md` (new)
- Reverses: the "said, never enforced" slow timeout of
  [0112](0112-a-faster-gate-with-the-same-checks.md)

## Context

0112 moved the workspace tests to nextest with `slow-timeout = { period = "60s" }`: a test past a
minute was named in the log and left to run. A hung test then held the gate until the job's own
timeout and the log said only which tests were still running. The owner's plan 201 gives every
repository's tests a per-test limit, so a hang fails fast and names the test.

## Decision

`[profile.default] slow-timeout = { period = "60s", terminate-after = 1 }`: at 60 s nextest ends
the test, reports it `TIMEOUT` by name and fails the run. A test whose own time on the nightly
gate is over a third of that has its own override, 3x its time rounded up to 10 s, measured from
two nightly runs' test logs (`docs/measurements/nextest-test-durations-ci.md`). The overrides set
only `slow-timeout`, so the wall-clock and firmware-build test-group overrides still apply: nextest
takes each setting from the first override that sets it. A test's own time excludes its wait for
a test-group slot, so the one-at-a-time wall-clock tests are not ended for queueing.

`tools/nextest-timeout-test.sh`, in `make verify`, holds it: it runs a throwaway crate whose one
test sleeps 5 s under a copy of the configuration with the period at 1 s and passes only on a
failed run naming the test `TIMEOUT`, and it fails unless the default profile's slow timeout is
exactly the line above.

`cargo test --doc` runs outside nextest and stays without a limit.

## Not chosen

- **5 minutes for every test**: no override to keep, but a hang still holds the gate for five
  minutes per test.
- **nextest only**: the Home Assistant tests (`tools/ha-test.sh`) and the web tests get their
  limits in the next task of the plan.
