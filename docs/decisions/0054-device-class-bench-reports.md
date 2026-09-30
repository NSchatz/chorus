# 0054: the device-class scripts report on a real device only, a FAIL is published, and a published run hands the checkout back

- Status: accepted (goal 7, 2026-09-30)
- Decided by: the goal (K45 applied to the entry points ADR 0049 left out; goal 7 item 4)
- Implemented in: `tools/stream-end-and-loss.sh`, `tools/start-fill-and-log-shape.sh`,
  `tools/delay-log-shape.sh`, `tools/overflow-run.sh`, `tools/device-loss-run.sh`,
  `tools/bench/lib.sh` (`bench_publish`), `tools/bench/topics.conf`, `Makefile`
  (`verify-device`), `fixtures/bench/`; held by `tools/bench/e2e-test.sh` (in `make verify`)

## Context

ADR 0049 put seven hardware entry points through `tools/bench/lib.sh` and left out the
device-class scripts the owner runs on the DAC+ in bench-packet session S1 (`make verify-device`)
and the device-loss run, because they grade inline and exit early. K45 says every hardware entry
point writes a hashed report and opens a PR. Three of these scripts also run on the ALSA `null`
device in the gate and in `make verify-null-device`, where a report would be wrong: `null` is
host evidence, not hardware.

## What was read

All read 2026-09-30: ADR 0049 and 0050, `docs/bench.md`, `docs/bench-packet.md`, the five
scripts, `tools/lib.sh`, `tools/bench/*`, and the client's and server's report lines in
`crates/client-linux/src/{main,run,delaylog}.rs` and `crates/server/src/main.rs`. No GPL source.

## Decision

1. **A real device reports; `null` does not.** A script writes a bench report when its playback
   device is not ALSA `null` (for the two that need a device that paces, that is every run past
   the guard). On `null` it behaves as before and writes nothing, like the ten-minute run's `null`
   form. Topics: `sound2-stream-end-and-loss`, `sound2-start-fill`, `sound2-delay-log-shape`,
   `sound2-overflow`, `sound2-device-loss`.
2. **Grading moves into a function over the raw files.** The inline checks are unchanged in what
   they check; they now read the run's saved files (delay logs, client and server output, a
   `run-status.txt` with the exit statuses), so `--report-from` regrades exactly what was
   hashed. A client that exits early is graded as a failure instead of ending the script.
3. **A FAIL is a result.** As for the ten-minute run, a failing run writes `Result: FAIL`, opens
   its PR when asked, and then exits non-zero.
4. **A published run hands the checkout back.** `bench_publish` switches back to where the
   checkout was once the branch is pushed, so the next script of a session reports from the
   same clean commit on main; before this the second script of `make verify-device` would have
   refused, being on the first one's bench commit. `make verify-device` runs all four scripts even
   after one fails and exits non-zero if any did, so a session opens one PR per script.
5. **Fixtures.** `fixtures/bench/` holds the real binaries' output from one ALSA `null` run of the
   two scripts that run there; the pacing-device and device-loss inputs are generated in the test,
   as the other delay logs already are.

## Consequences

Without `CHORUS_BENCH_PR=1` a report is left untracked in the tree, so the next script of
`make verify-device` refuses the dirty tree by name; the owner runs with `CHORUS_BENCH_PR=1`
(bench packet S1) or runs the scripts one at a time. A developer with a snd-aloop loopback
running these scripts on a branch is refused by the bench step (the build is not on main), as the
ten-minute run already is.
