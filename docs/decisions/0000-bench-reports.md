# 0000: bench results come back as schema-checked reports on bench/* branches, raw data hashed and committed up to a size limit

- Status: accepted (goal 7, 2026-09-30)
- Decided by: the owner (K45: "script writes a PR"); the goal took the choices left open below
- Implemented in: `tools/bench/lib.sh`, `tools/bench/topics.conf`,
  `tools/bench/validate-report.sh`, `tools/bench/validate-open-prs.sh`, the seven entry points
  `docs/bench.md` lists, `crates/measure/src/pair.rs`, `crates/client-linux/src/offsets.rs`;
  held by `tools/bench/e2e-test.sh` (in `make verify`)

## Context

K45 decided that every hardware result reaches the repository through the bench script that
took it: a report in `docs/measurements/` with the raw data hashed, `Source: hardware`, the
device and the date, committed on a `bench/<date>-<topic>` branch and opened as a PR from the
owner's machine, and that every later goal validates open bench PRs against "its schema" and
merges them. It left open the schema, where raw data lives, how big it may be, how an entry point
whose measurement needs hardware is tested here, and how the free-run baseline (audit A-4) gets
real series at all.

## Decision

1. **One library, two halves per entry point.** Each hardware entry point keeps its measure half
   (the `require_*` refusals stay first, so a machine without the hardware still refuses by the
   prerequisite's name) and gains a report-and-PR half that needs no hardware. The measure half
   leaves a run directory (raw files plus a manifest); `<script> --report-from <run directory>`
   runs the second half alone, which is how a failed publish is retried and how the fixture test
   drives it.
2. **The build measured is a commit on origin/main, on a clean tree.** The report half refuses a
   dirty tree or a HEAD that is not an ancestor of `origin/main` (checked before the measurement
   starts and again before the report is written). `chorus-measure` refuses a `--source hardware`
   report from a dirty tree (audit A-6), and now writes the `Source:` line itself.
3. **Raw data.** Files of at most 16 MiB are committed under `docs/measurements/raw/<stem>/`, up
   to 64 MiB per report; larger files are listed with size and sha256 and stay on the owner's
   machine. GitHub warns on files above 50 MiB, blocks them above 100 MiB and asks for
   repositories ideally under 1 GB
   (https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github,
   read 2026-09-30); 16 and 64 keep a single capture (30 s at 96 kHz is about 11.5 MB) committed
   while leaving room for many sessions. Not chosen: Git LFS (another service and quota for a
   private repository whose owner acts in weeks); hashing everything and committing nothing (a
   report nobody else can re-analyse).
4. **The schema** is `chorus-bench-report/1` (`docs/bench.md`): fixed header lines, a `## Fields`
   table with each topic's required keys (`tools/bench/topics.conf`), a `## Raw data` table and
   the analysis output. The validator also refuses a raw file byte-identical to a committed
   fixture, and identity material (K27). `validate-open-prs.sh` runs this checkout's validator
   on each open PR's files, so a PR cannot bring its own rules, and refuses a PR that touches
   anything but its report, its raw directory and (for the free-run topic) the baseline.
5. **Result words**: PASS, FAIL, MEASURED (a characterization with no bound), INCOMPLETE (part of
   the criterion has no analysis yet). The EMBEDDED-5 script's old second analysis passed a WAV
   to `chorus-measure free-run`, which reads an offsets series; it is removed, and the report says
   AC-3's produced-sample-rate analysis does not exist yet.
6. **The free-run series (audit A-4, A-13).** `chorus-client --free-run --offsets-out <file>` forms
   the sync loop's error every tick and applies nothing, writing `t_ns offset_ns` on the server
   timeline in the `.offsets` format; `--offsets-out` alone (servo running) is the jitter series
   the wireless characterization fetches. `chorus-measure pair` interpolates one client's series
   at the other's instants, refuses a series not marked `correction = disabled` and
   `time_base = server`, and fits the difference as the free-run baseline. The five-second gap
   past which pairing skips rather than bridges is a choice (ten sync ticks), not a measurement.

## Consequences

- `make verify` runs `tools/bench/e2e-test.sh`: every topic's report half on committed fixtures
  into a throwaway clone with a bare remote and a fake `gh`.
- The ten-minute run publishes no bench report on the ALSA `null` device, which is not hardware
  evidence.
- A second run of a topic on one day gets `-2`, `-3` on its report, raw directory and branch.
