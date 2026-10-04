# 0112: the gate runs the workspace tests under nextest with the wall-clock tests one at a time, builds the firmware objects once and in parallel with header dependencies, lints in parallel, builds lighter, and stops after a failed cheap step; CI is started by hand

- Status: accepted (2026-10-03)
- Decided by: the owner (2026-10-03, in the goals program's main session: "Implement all of your
  findings and improvement across the board"), on the gate-speed research of the same day; the
  owner's standing rule is that quality is not sacrificed for speed, so every check the gate ran
  still runs and no target changes meaning
- Implemented in: `tools/gate.sh` (the test step, `CARGO_INCREMENTAL=0`, the stop after the cheap
  steps), `.config/nextest.toml` (new), `mise.toml` and `mise.lock` (cargo-nextest 0.9.146),
  `Cargo.toml` (`[profile.dev]`, `[profile.test]`, the proc-macro overrides), `firmware/Makefile`
  (objects, `-MD -MP`), `Makefile` (`firmware-check` builds with `-j`),
  `tools/conventions/check-cppcheck.sh`, `check-shell.sh`, `check-pins.sh`,
  `crates/server/tests/adoption.rs` (the build lock), `firmware/tests/test_protocol_v2.c` (two
  bounded `%s`), `.github/workflows/ci.yml`

## Context

`make gate` grew from 439 s to 1,171 s (the owner's agent-harness repository, its ADR 0003, from this program's ledgers) and measured 1,311 to 1,706 s on
2026-10-02 and 03 (the nightly, `/cache/goals/maker/nightly/chorus-2026-10-02T08:34:49Z.log`:
1,483 s; goal 14's twelve gates, 1,130 to 1,706 s). `make tier-fast` measured 520 s, 408 s of it
the test step. Every one of those minutes is spent under the host-wide heavy locks, and on
2026-10-02 `chorus-heavy` was held 6.05 hours. Measured on 2026-10-03 (logs under
`/cache/tmp/ci-research/chorus-out/`):

- **The test step is mostly running tests, one binary after another.** With the build already
  done ("Finished `test` profile ... 0.13s"), `cargo test --workspace` still took 424.5 s: it runs
  its 140 test binaries in series, so the step is the sum of them. Ten binaries are 282 s of it,
  each one either a CPU-bound simulation or a real server paced on the wall clock.
- **The build disk is the shared bottleneck.** `/workspace` and `/cache` are one 7200 rpm disk
  (a degraded RAID 1) at 100% utilisation for most of the day, with I/O pressure of 40 to 69% and
  CPU pressure under 2%. One goal worktree's `target/` held 5.8 GB in 26,491 files: 2.7 GB in
  23,730 files of incremental-compilation cache, and 3.0 GB of test executables, about 85% of
  each one debug information (a chorus-server test binary is 50 to 57 MB).
- **syn sat on the build's critical path** for 81 s: `[profile.dev.package."*"] opt-level = 2`
  optimises the proc-macro toolchain too, and only curve25519-dalek's derive uses it.
- **The firmware host build compiled the endpoint library once per binary.** Every test binary
  named every `firmware/src` file on one `cc` line, so the library (34 units, 15.7k lines, at
  `-O2 -g`) compiled about twenty-five times, serially: 262 s of CPU, 95.6 s at `make -j8`.
  The binaries named only `.c` files, so a change to a header alone rebuilt nothing and the next
  run tested stale binaries.
- **cppcheck and shellcheck ran on one CPU**: 49.6 s and 53.6 s of every tier.
- **A failed lint still cost the whole gate**: every step ran after a red fmt or clippy.
- **CI burned the month's Actions minutes in a day and never went green.** On 2026-10-01 the
  workflow ran 246 times for 1,995 minutes: on every `push` and again on every `pull_request`
  (91 of 155 commits twice, 782 minutes; the concurrency group was `github.ref`, which differs
  between the two events), and every run went red on two failures that only the runner has.

## What was read

All read 2026-10-03: the repository's `Makefile`, `tools/gate.sh`, `tools/lib.sh`,
`tools/control-determinism.sh`, `tools/conventions/*.sh`, `firmware/Makefile`,
`firmware/tests/session-outage.sh`, `Cargo.toml`, every crate's `Cargo.toml`, `rust-toolchain.toml`,
`mise.toml`, `.github/workflows/ci.yml`, ADRs 0002 and 0020, and the integration tests
named below; the nightly and goal gate logs named above; `gh run list` and the jobs and logs of
runs 36846006728 and 34797144550; the owner's agent-harness repository's ADRs 0003 (speed) and 0018 (gate tiers) and its spec
`gates.md`.
cargo-nextest's documentation at https://nexte.st (test groups, overrides, priorities, `retries`,
`fail-fast`, JUnit output) and its release page
https://github.com/nextest-rs/nextest/releases/tag/cargo-nextest-0.9.146 (Apache-2.0 OR MIT,
2026-09-21); cppcheck's manual on `--cppcheck-build-dir` and whole-program analysis; the Cargo
book's profile chapter (`debug = "line-tables-only"`, per-package overrides, precedence); GCC's
manual on `-MD`, `-MP` and `-Wformat-truncation`. No GPL source was opened: cppcheck and
shellcheck run as unmodified binaries, as `mise.toml` already says.

## Decision

1. **The test step runs the workspace tests under cargo-nextest, then the documentation tests
   with `cargo test --doc`.** nextest runs every test in its own process, eight at a time
   (`.config/nextest.toml`), so the slow binaries overlap instead of queueing; nextest does not
   run doctests, so `cargo test --doc --workspace --locked` runs them after it, and both run even
   when the first fails. Its fitness: per-test processes, a per-test duration for every run
   (`target/nextest/default/junit.xml`), and test groups, which `cargo test` has none of.
   `retries = 0` (a red test is red, never re-run until green) and `fail-fast = false` (a red
   run names every failing test). Pinned in `mise.toml` with its sha256 in `mise.lock`, run
   through `mise exec` as `tools/endpoint-package.sh` runs its pinned tools; `check-pins.sh`
   holds the gate's `cargo nextest run` to `--locked` as it holds build, test and clippy.
2. **The wall-clock tests run one at a time, first.** Under `cargo test` each of their binaries
   ran with nothing else of the suite beside it. Here the tests that pace a real server, client
   or hub on the wall clock and grade underruns, inserted zeros, latency or loss
   (`tv_low_latency`, `tv_capture_rate_match`, `alarms_sleep_autoplay`, `regress_0031_f1`,
   `dsp_end_to_end`, `front_panel`, `cec_tv`, `fifo_source`) are one test group with
   `max-threads = 1`: no two of them ever share the host's CPUs, which is less contention than
   before for each of them, not more. They start first, because their chain is the longest.
   `control_thread_population` stays out on purpose: ADR 0020 grades it under contention, and
   the determinism step repeats it alone.
3. **The three tests that run `make -f firmware/Makefile` take one file lock and one group.**
   `firmware_install.rs` and `firmware_session.rs` already took
   `firmware/build/.chorus-endpoint-session.lock`; `adoption.rs` now does too, and the three are
   a `max-threads = 1` group, since nextest runs them in separate processes where `cargo test`
   ran them one binary after another.
4. **Lighter builds, the same code.** `[profile.dev]` and `[profile.test]` carry
   `debug = "line-tables-only"`: a panic, a backtrace and a breakpoint still name the file and
   line, and debug information is not an input to code generation. syn, proc-macro2 and quote
   build at `opt-level = 0`: they run only at compile time and expand the same code at any
   optimisation level. The gate exports `CARGO_INCREMENTAL=0`: its builds are one-shot, the
   incremental cache is gigabytes of small files on the shared disk for a rebuild that never
   comes, and the nested builds its steps and tests start inherit it, so they reuse the gate's
   artifacts. `make check`, `make test` and every developer build are untouched by the last one.
5. **The firmware host build compiles each unit once, to its own object, recording the headers
   it read.** Every binary links the objects of the sources it named, in the same order; gcc
   compiled each source of a one-line build as its own translation unit anyway (no `-flto`), so
   the code linked is the code it was. Each unit keeps the include paths it had: a
   `firmware/src` unit includes nothing from `firmware/tests` or `firmware/check`, and its
   preprocessed text is the same with and without the `-I` the test binaries added (checked for
   all 67 units). `-MD -MP` (not `-MMD`, so the vendored headers seen through `-isystem` count
   too) records every header, and a changed header now rebuilds exactly the objects that include
   it; before, it rebuilt nothing. Nothing of the endpoint compiles until the configuration gate
   has passed, as before (an order-only prerequisite on the stamp). `make firmware-check` builds
   every binary first with `-j$(CHORUS_FIRMWARE_JOBS)` (default 8), then runs the checks one
   after another exactly as before.
6. **cppcheck and shellcheck run eight at a time and find the same things.** cppcheck takes
   `-j8 --cppcheck-build-dir` with a new directory each run: `-j` alone silently drops the
   whole-program (`ctu*`) checks, which run only over summaries kept in a build directory
   (measured: an injected `ctunullpointer` was found serially and with the build directory, and
   missed by `-j8` alone; with the build directory the findings on injected defects were
   identical to the serial run's). shellcheck runs once per script through `xargs -P 8`; it
   analyses every script it is given on its own, and the findings on injected defects were
   identical to the one-process run's.
7. **The gate stops after a failed cheap step.** The conventions checks, fmt and clippy all run
   and every failure among them is listed; if any failed, the gate prints the steps it did not
   run and fails. A change that fails a lint is red whatever its tests say. Once the cheap steps
   pass, every later step runs even after one fails, as before.
8. **CI is started by hand.** `ci.yml` runs on `workflow_dispatch` only: Actions does not start
   jobs on this account, and the local gate is the merge gate (ADR 0002, R12). Its concurrency
   group is the branch's name for a push and for a pull request alike, so if the triggers come
   back the two events share one group and never run twice. Of the two runner-only failures, the
   trivial one is fixed: `test_protocol_v2.c` bounds two `%s` of a `char[128]` stem with `%.127s`,
   as its rejected-vector loop already did, which the runner's fortified `snprintf` needs
   (reproduced here with `-D_FORTIFY_SOURCE=3`: those two lines, and nothing else).

## Measured (2026-10-03, this host, shared with live goal gates)

| What | Before | After |
|---|---|---|
| firmware host build, cold, `-j8` | 95.6 s (262 s of CPU serially) | 7.3 s |
| firmware host build after a header-only change | rebuilt nothing (stale binaries) | the objects that include it, and their binaries |
| cppcheck | 49.6 s | 9.9 s |
| shellcheck | 53.6 s | 12.5 s |
| a test binary's size (`latency_growth`) | 14.9 MB | 9.4 MB |
| files a test build leaves in `target/debug` (chorus-sync's tests) | 1,298 | 359 |

The gate's own before and after are in the pull request that adds this record.

## Not done here, each its own decision

- **`opt-level = 1` for chorus's own crates** would cut the simulations' run time about five
  times (`latency_growth` 30.5 s to 6.3 s, measured). Not taken: faster code could make the
  scheduling race the determinism step exists to catch rarer, and the owner decides that trade.
- **Determinism repetitions in parallel lanes** (four lanes on disjoint two-CPU sets, each
  repetition still on two CPUs, 100 on one build): about 254 s to 65 s. It changes how the claim
  of ADR 0020 and `config/verification.conf` is made, so it needs its own record.
- **One worktree per lane, reused between pull requests**, so a pull request's first gate is not
  a cold build of the workspace and the firmware. A change to the goals program's tooling.
- **The second runner-only failure**: systemd 255's `systemd-analyze verify` on the runner does
  not flag the misspelt directive `tools/endpoint-package.sh` plants to prove the check works.
  Not trivial; it matters only if CI's triggers come back.
