# 0142: the Home Assistant gate steps run or fail, never a green SKIPPED under CI, and a narrowed `make ha-test` run is judged on the selected tests alone

- Status: accepted, 2026-10-04; changes the CI skip behaviour 0138 gave `tools/ha-test.sh` and
  `tools/ha-hassfest.sh`. How every other gate step treats a missing tool is unchanged.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `tools/ha-test.sh`, `tools/ha-hassfest.sh`, `tools/gate.sh` (`ha_step`,
  steps `ha-test`, `ha-hassfest`, `ha-live`), `Makefile`; `docs/home-assistant.md`,
  `docs/conventions.md` (rule 25) and `integrations/homeassistant/README.md` say the same

## Context

CI is the gate (0140), so a step's green line in a CI run is the only evidence that the step
ran. Two things made the Home Assistant steps weak evidence and a poor inner loop:

- Under `CI=true`, `tools/ha-test.sh` and `tools/ha-hassfest.sh` printed `SKIPPED` and exited
  0 when `uv` was absent, and `ha-hassfest` did the same when the core checkout could not be
  cloned. A runner that lost its pinned tools would have passed the gate without linting, typing
  or testing the integration. `make ha-live` exited 0 whenever its one test was skipped.
- A narrowed run (`make ha-test HA_TEST_ARGS="-k announce"`) ran the selected tests and then
  failed the 95 % coverage threshold, which a part of the tests cannot meet; with `--no-cov` it
  failed the config-flow coverage step for want of data. Local runs are narrow tests only
  (0140), so the documented inner loop always ended red.

## Decision

1. **A missing input fails the step, under CI as anywhere else.** No `uv`: `FAIL`, naming it.
   No core checkout and no clone: `FAIL`. The `CI=true` branches that printed `SKIPPED` are
   removed. The identity scan's skip on a fork's pull request (0140, item 5) is a different
   case, a private input a fork cannot have, and stays.
2. **`make ha-live` says whether the test ran.** With a built server it ends `ha-live: PASS`.
   Without `CHORUS_SERVER_BIN` it ends `ha-live: SKIPPED` and exits 0 on a developer's machine,
   and fails under `CI=true`. A `CHORUS_SERVER_BIN` that is not an executable file fails.
3. **The gate holds each of the three steps to its own PASS line.** `ha_step` in
   `tools/gate.sh` turns a step red when its target exits 0 without `<step>: PASS`, or with a
   `SKIPPED:` line. The gate passes `HA_TEST_ARGS` empty, so nothing in the environment can
   narrow its run.
4. **A narrowed run is pytest alone.** Any `HA_TEST_ARGS` makes `make ha-test` run pytest with
   those arguments and nothing else: no ruff, no mypy, no coverage threshold. It ends
   `ha-test: NARROWED PASS`, a line the gate does not accept. The whole run is unchanged: both
   thresholds, then `ha-test: PASS`.

## Consequences

- A CI runner without the pinned `uv`, without network for the first clone of the core tag, or
  without a built `chorus-server` is a red gate that names what is missing.
- `.github/workflows/ci.yml` is unchanged: its `Pinned tools` step installs `uv` from
  `mise.toml`, and the gate's build step makes the server before `ha-live`.
- A narrowed run proves only the tests it selected. Lint, types and coverage are proved by the
  whole run, which CI makes on every pull request.
