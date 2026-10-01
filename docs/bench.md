# Bench runs: how a hardware result reaches the repository

Every result that needs hardware comes back through a bench script (K45): the
script runs the measurement on the owner's bench machine, writes a report into
`docs/measurements/` with every raw file hashed, and, when asked, commits it on a
`bench/<date>-<topic>` branch and opens a pull request. The next goal validates
the report against its schema and merges it, or comments why not. Nobody pastes
numbers into a session and nothing goes through a drop folder.

No bench script writes to a device: none flashes, sets a register or pushes an
image, so none reads `CHORUS_OWNER_AT_BENCH`. Flashing an endpoint before a run is
its own step with its own guard: `tools/firmware-flash.sh` (`make firmware-flash`).

## The flash guard

`tools/firmware-flash.sh` writes the image `make firmware-image` built onto a
board, and refuses unless `CHORUS_OWNER_AT_BENCH` is exactly `1`. Only the owner
sets it, at the bench, on the command line of the one run:

    CHORUS_OWNER_AT_BENCH=1 tools/firmware-flash.sh --port /dev/ttyACM0

Nothing in the repository sets it (the gate's `check-flash-guard.sh` fails if
anything outside `docs/` does), and the gate runs the tool without it to show it
refuses (`check-flash-tools-refuse.sh`). `--print` shows the exact esptool
command without the variable and without running anything. The tool never runs
an eFuse command and refuses an image or argument that asks for one (BRIEF.md
section 3.1 rule 2).

## One-time setup of the bench machine

1. A chorus checkout: `git clone git@github.com:NSchatz/chorus.git && cd chorus`.
2. The toolchains: install mise (<https://mise.jdx.dev>), then in the checkout
   `mise install` (the pinned tools in `mise.toml`) and a Rust toolchain that
   honours `rust-toolchain.toml` (rustup does; or `mise use rust@1.98.1`).
   `make build` must pass.
3. The GitHub CLI, logged in as the owner: `gh auth login` (HTTPS or SSH, with
   the `repo` scope), then `gh auth status` to check. The script pushes over the
   checkout's `origin` and opens the PR with `gh pr create`.
4. For runs with a second endpoint: that machine has `chorus-client` on its
   `PATH` (built from the same commit) and accepts `ssh` from the bench machine
   without a password prompt.

## Running one

The checkout has to be clean and at a commit on `origin/main` (`git switch main
&& git pull`); the script refuses otherwise, because the report names that commit
as the build measured and a reader has to be able to check it out. Then, for
example, the SYNC-4 hour:

```sh
CHORUS_BENCH_PR=1 \
CHORUS_SECOND_ENDPOINT=user@endpoint-b \
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    ./tools/sync-hour-run.sh
```

Without `CHORUS_BENCH_PR=1` the report is written into the working tree and
nothing is committed, pushed or opened, which is how to look at a report first.
`CHORUS_BENCH_DEVICE_NOTE='ESP32-S3-DevKitC-1 N16R8'` adds a line naming the
hardware (a model, never a hostname, an address or a person).

Each script keeps its run in a run directory (printed at the start; under
`$TMPDIR` unless `CHORUS_BENCH_RUN_DIR` says where). If the report or the PR
step fails after the measurement, nothing is lost: fix the cause and run
`<script> --report-from <run directory>` from the same commit, which redoes only
the report and PR half.

| Topic | Script | Criterion | Needs |
|---|---|---|---|
| `rig3-capture` | `tools/measure/capture-run.sh [seconds]` | RIG-3 inter-device lag through two endpoints | second Linux endpoint, playback device, 2-channel capture interface |
| `rig3-free-run` | `tools/measure/free-run-run.sh [seconds]` | RIG-3 free-run drift baseline, correction disabled (audit A-4) | second Linux endpoint, playback device |
| `sync4-hour` | `tools/sync-hour-run.sh` | SYNC-4 AC-1, one hour under the wired bound | as `rig3-capture` |
| `embedded5-endpoint-rig` | `tools/endpoint-rig-run.sh` | EMBEDDED-5 AC-1 and AC-3 | ESP32-S3 endpoint with amplifier, Linux endpoint, capture interface |
| `embedded5-decode-cost` | `tools/decode-cost-run.sh` | the S3's FLAC and Opus decode cost and its DSP chain's cost in four configurations, measured on the chip (MEASURED, no bound) | ESP32-S3 endpoint with its serial console (ADR 0060; `dsp-cost`, ADR 0086) |
| `wifi7-wireless` | `tools/wireless-characterization-run.sh` | WIFI-7 AC-2 and AC-3 | as above on Wi-Fi; the endpoint console sets each power-save mode and its readback is kept (ADR 0060) |
| `product6-soak` | `CHORUS_SOAK_SECONDS=259200 tools/soak-run.sh` | PRODUCT-6 AC-4, three days | two endpoints, three days |
| `sound2-ten-minute` | `tools/ten-minute-run.sh` | SOUND-2, ten minutes on a real device | a playback device that reports a delay (not ALSA `null`) |
| `sound2-stream-end-and-loss` | `tools/stream-end-and-loss.sh` | SOUND-2, a clean end signalled in band and a lost server told apart, neither drain an underrun | a playback device that opens (a real one; on ALSA `null` no report) |
| `sound2-start-fill` | `tools/start-fill-and-log-shape.sh` | SOUND-2, output withheld until the start fill, the delay log's shape, the bound relations | as `sound2-stream-end-and-loss` |
| `sound2-delay-log-shape` | `tools/delay-log-shape.sh` | SOUND-2, a one-minute delay log with the reported delay inside its bounds | a playback device that reports a delay (not ALSA `null`) |
| `sound2-overflow` | `tools/overflow-run.sh` | SOUND-2, the over-rate run: crossing and discard at the maximum, no rate change | as `sound2-delay-log-shape` |
| `sound2-device-loss` | `CHORUS_REMOVABLE_DEVICE=<dev> CHORUS_REMOVE_COMMAND=<cmd> tools/device-loss-run.sh` | SOUND-2, a device removed mid-run reported with its reason, non-zero exit | a device the owner can remove mid-run (not ALSA `null`) |

`make verify-device` runs the first four of these (stream end and loss, start
fill, delay-log shape, over-rate), all four even when one fails, so on a real
device with `CHORUS_BENCH_PR=1` it opens one PR per script. Each script grades
inline as before and still exits non-zero on a failure, but a failure is a
result: the report is written with `Result: FAIL` and its PR is opened before
the non-zero exit, so a failing hardware run reaches the repository too. On the
ALSA `null` device (`make verify-null-device`, `make verify-alsa-null`) the
scripts behave as before and write no bench report: that is host evidence, not
hardware.

The measure half of every script still refuses by name when its hardware is
absent (`tools/unrun-checks-are-visibly-unrun.sh` holds that); the report half
needs no hardware.

## What the PR looks like

- Branch `bench/<date>-<topic>` (a second run of the topic that day adds `-2`),
  one commit `bench: <topic> <date>, <RESULT>` whose parent is the build
  measured.
- `docs/measurements/<topic>-<date>.md`: the report (schema below).
- `docs/measurements/raw/<topic>-<date>/`: the raw files of at most 16 MiB each,
  up to 64 MiB per report. A larger file (a three-day delay log, the sixth
  capture of an hour) is listed with its size and sha256 and stays in the run
  directory on the bench machine. GitHub warns above 50 MiB, blocks above
  100 MiB and asks for repositories ideally under 1 GB
  (<https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github>,
  read 2026-09-30), which is where the two limits come from.
- For `rig3-free-run` only: `docs/measurements/free-run-baseline.conf`, now
  `source = hardware`, citing the report and the paired series.
- The PR body: the result line, the report path, the build, and the report's
  field table.
- Once the branch is pushed and the PR opened, the checkout switches back to
  where it was (main, clean), so the next bench script of the session can
  report from the same build.

## The report schema (`chorus-bench-report/1`)

`tools/bench/validate-report.sh <report>` checks it; `tools/bench/topics.conf`
is the machine-readable part (each topic's script and required fields).

```text
# Bench report: <topic>, <date>

Schema: chorus-bench-report/1
Source: hardware
Build measured: `<40-hex commit on origin/main>`
Topic: <topic from topics.conf, matching the file name>
Entry point: `<the topic's script>`
Date: <YYYY-MM-DD, matching the file name>
Device: <one line per device, at least one>
Result: <PASS|FAIL|MEASURED|INCOMPLETE>: <one-line summary>
Criterion: <what the run grades>
Reproduce with: `<command>`

## Fields        a `| field | value |` table carrying the topic's required fields
## Raw data      a `| file | bytes | sha256 | kept |` table, kept = committed|owner
## Analysis      the analysis commands' output, one block each, with their exit codes
```

`MEASURED` is a characterization with no bound to pass (a lag figure, a
baseline); `INCOMPLETE` means part of the criterion has no analysis yet (the
EMBEDDED-5 produced sample rate) or no capture was taken (the soak's bound).

The validator also requires: the build commit is an ancestor of `origin/main`;
every `committed` raw file is present with its size and hash, every `owner` file
is above the committed limit (and matches when present), and nothing unlisted is
in the raw directory; no raw file is byte-identical to a committed fixture; and
no em dash, email or `user@host`, MAC, private IPv4 address or home-directory
path anywhere in the report.

## For goals: validating and merging open bench PRs

```sh
bash tools/bench/validate-open-prs.sh
```

lists the open `bench/*` PRs with `gh`, checks each touches only its own report,
its raw directory and (for `rig3-free-run`) the baseline, and runs this
checkout's validator on its files. A valid PR is merged with the usual gate; an
invalid one gets a comment quoting the failures.

`make verify` runs `tools/bench/e2e-test.sh`, which drives every script's report
and PR half from committed fixture captures (`fixtures/measure/`, `fixtures/bench/`) into a throwaway clone with a local
bare remote and a fake `gh`, and checks the report, the hashes, the branch, the
push and the `pr create` call.
