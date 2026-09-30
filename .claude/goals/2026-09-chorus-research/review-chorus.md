# chorus survey (NSchatz/chorus), 2026-09-29

Surveyed at `main` = `1ea9f2a` (clean tree), in /workspace. Nothing tracked was changed. Build artifacts went to
`/workspace/target` and a scratch dir. All timings are on this container (2 CPUs, `-j2`, rustc 1.98.0 via mise).

---

## 0. Headline facts (for the interview)

1. **Nothing has ever touched hardware.** Every hardware-gated success criterion of every phase from SOUND-2 onward is
   recorded "NOT PASSED" in `docs/verification-record.md` (1,243 lines). No sound card, capture interface, second
   endpoint, ESP32-S3, amp, Wi-Fi AP or Proxmox deploy has ever been involved. "The endpoint has **never driven a pin**.
   Nothing here has been heard." (`README.md:84`).
2. **All 6 files in `docs/measurements/` are fixture/model-derived or host-only**, none is a measurement of audio timing
   on hardware (`docs/measurements/*`, each labelled `source = fixture` or "NOT A MEASUREMENT").
3. **The make-or-break phase (SYNC-4: sub-ms on two wired Linux clients) is unretired**, yet phases 5, 6, 7 were
   built on top of it in software. BRIEF 3.2 "Riskiest unknown first" and CLAUDE.md rule 7 are satisfied only in the
   "or say why not" sense.
4. **Software quality is high and fully green locally**: build 53 s, 543 Rust tests pass (94 s), firmware host suite
   510 checks + 3 real-socket outage runs pass (213 s), `make verify` 159 checks pass (15 s). Zero external crates.
5. **The Docker image cannot build today**: `deploy/Dockerfile` does not COPY `docs/`, but
   `crates/server/src/control.rs:217` does `include_str!("../../../docs/control-page.md")` (added by S0054, `0c08ef8`).
   Reproduced: `cargo check --locked --workspace --bins` on the Dockerfile's exact build context fails with
   "couldn't read crates/server/src/../../../docs/control-page.md". Separately, the declared MSRV/image toolchain
   (rust 1.74) cannot compile `crates/client-linux/src/main.rs:179-180` (`ExitCode == ExitCode`, E0369), reproduced
   with `mise exec rust@1.74.0 -- cargo check --locked --workspace --all-targets`. CI never builds the image.
6. **The ESP-IDF binding (`firmware/main/`, 4 files) has never been compiled by anyone**; ESP-IDF v5.3 is pinned
   (`firmware/config/endpoint.conf:197`) but absent everywhere (here, CI, and every machine in the verification record).
7. **The ESP32 endpoint has no wired network path at all** (Wi-Fi STA only in `firmware/main/esp_hal.c:301-321`; zero
   mentions of W5500/Ethernet/RMII outside BRIEF), and its committed default is `link_transport = wireless`
   (`firmware/config/endpoint.conf:36`), the opposite of BRIEF's "wired first".
8. **TAS5825M register map is DECLARED UNKNOWN** (`firmware/config/endpoint.conf:166-186`, 8 keys = `unknown`) because
   "the research pass could not extract text from the PDF". A human reading the datasheet unblocks it.
9. **Repo visibility flipped during the survey**: `gh repo view` at ~19:24 said `PUBLIC`/`isPrivate:false`; at ~19:34
   both GraphQL and REST say `PRIVATE`, and anonymous `https://github.com/NSchatz/chorus` returns 404. No LICENSE file.
10. **Idle 16 days**: last main commit 2026-09-13 (`1ea9f2a`); one open PR #18 (S0124, UI "interface craft" record,
    +2,384 lines, CI green, unreviewed since 2026-09-14).
11. **Roughly a third of merged specs are umbrella-convention/gate work, not phase work** (S0054, S0058, S0063, S0113,
    and open S0124), and the gates are heavy: e.g. 6,613 lines of `tools/ui` JS + 48 demonstration trees grade a
    1,505-line control page.

---

## 1. Purpose and layout

**What it is** (`BRIEF.md:1-40`, 367 lines, owner the owner, dated 2026-08-21, unchanged since `0f28efc`): replace Sonos with a
self-built multiroom + (later) surround system: a containerized Rust server on a Proxmox host (Dell R730xd) that cuts PCM
into timestamped chunks on one monotonic timeline; custom sync protocol; ESP32-S3 + TAS5825M smart speakers (C firmware)
and Linux endpoints (Rust + ALSA). Targets (`BRIEF.md:58-64`): multiroom < 5 ms (aspire < 1 ms), stereo pair < 0.5 ms
(aspire 0.2), TV A/V +/-40 ms, unattended "days" (aspire "weeks").

**Tree** (437 tracked files; 44,272 lines of Rust in 129 files; 10,270 lines of C/H in 43 files; 6,350 lines of shell in
33 files; 8,457 lines of Markdown in 58 files):

| path | files | lines | what |
|---|---|---|---|
| `crates/protocol` | 9 | 1,923 | binary wire format: 3 message types (`time_sync` 0x01, `audio_chunk` 0x02, `stream_end` 0x03; `docs/protocol.md:31-37`), encoder/decoder, session that skips unknown types |
| `crates/sync` | 13 | 2,550 | deterministic seeded simulator (virtual clocks, ppm skew, jitter), min-RTT offset filter, PI servo, 2-tier (fine / hard resync), crosscheck vector generator |
| `crates/audio` | 5 | 772 | PCM ingest, chunker, the monotonic server timeline |
| `crates/alsa` | 3 | 858 | ALSA PCM bound at run time via dlopen (ADR 0008), so the build needs no libasound |
| `crates/hostctl` | 3 | 1,687 | container scheduling contract: RLIMIT_RTPRIO ceiling, RLIMIT_RTTIME, memlock, `/proc/self/task` inventory |
| `crates/audio-path` | 8 | 2,159 | enforces `audio-path.conf` (64 listed units) and `real-time-acquisitions.conf`; bans settable (wall) clocks in the audio path |
| `crates/measure` | 19 | 7,339 | RIG-3 harness: 2-channel capture -> median/p95/max lag (sub-sample), free-run ppm with confidence bound, jitter reports |
| `crates/server` | 25 | 9,552 | `chorus-server`: tone or file/FIFO source (`src/source.rs:145-151`), TCP audio serving, time-sync master, groups/zones, HTTP control plane + SSE + embedded UI (`src/ui/`, 1,505 lines), mDNS advertise; plus `chorus-rt-spin` |
| `crates/client-linux` | 22 | 10,830 | `chorus-client`: receive, jitter buffer, ALSA playout, SyncLoop (servo on the real path, reads `snd_pcm_delay`), zone gain, control, discovery; `chorus-delaylog-check` |
| `crates/control` | 12 | 3,641 | versioned JSON control catalog, hand-written JSON codec (ADR 0016), zone state, persisted state, bounded fanout |
| `crates/discovery` | 8 | 2,176 | hand-written mDNS + DNS-SD (advertise/browse), packet vectors, `chorus-mdns-probe` |
| `crates/comment-density` | 11 | 2,244 | S0113 gate: Rust tokenizer counting comment-vs-code lines per file |
| `firmware/` | 51 | 11,172 | ESP32-S3 endpoint in C11: `src/` 13 units (protocol, sync_{servo,sim,rng,scenario}, amp sequencer, i2s rules, session supervisor, telemetry, wifi, config), `include/chorus/` 10 headers, `main/` 4 files (ESP-IDF binding, never compiled), `check/` scans, `tests/` 14 files incl. fake amp/radio |
| `tools/` | 104 | 14,154 | 27 top-level `.sh` entry points (20 environment-dependent, all refuse by name), `tools/measure/`, `tools/ui/` (Playwright rendered grader, 6,613 lines), 48 demonstration trees (pinning 13, styling 25, comment-density 10) |
| `fixtures/` | 86 | 7,062 | `protocol/` 3 vector pairs; `sync/` 5 scenarios + 5 crosscheck vectors; `control/` 26; `discovery/` 13; `measure/` 30 (9 synthetic WAVs + `.params`) |
| `docs/` | 41 | 7,255 | 24 ADR files, 6 measurement files, `protocol.md`, `control-plane.md`, `control-page.md`, `sound-2.md`, `verification-record.md`, `wireless-expectations.md`, 3 convention records |
| `deploy/` | 3 | 233 | `Dockerfile`, `run-server.sh`, `README.md` |
| `config/` | 4 | 266 | `measure.conf`, `sync.conf`, `transport.conf`, `verification.conf` |
| root | | | `audio-path.conf` (10 KB), `real-time-acquisitions.conf`, `Makefile` (283 lines, 38 targets) |

Deviation from BRIEF 7: firmware lives at `firmware/` not `firmware/esp32s3/`; there is no `crates/dsp`; no compose
file or systemd units in `deploy/`.

### BRIEF section 8 phases vs reality

Phase IDs as used in-tree: FOUNDATION-1, SOUND-2, RIG-3, SYNC-4, EMBEDDED-5, PRODUCT-6, WIFI-7, DSP-8, TV-9, FLEET-10.
Note "WIFI-7" is BRIEF phase 7 (the Wi-Fi tier), not IEEE Wi-Fi 7.

| # | phase / "success looks like" (`BRIEF.md:267-276`) | spec / commit | assessment |
|---|---|---|---|
| 1 | FOUNDATION: servo converging in simulation under realistic jitter/skew | S0001 `7e50d3f` (#1) | **Met (by definition in sim).** 5 scenarios in `fixtures/sync/`, `crates/sync/tests/simulator_regression.rs` 7 tests; C mirror reproduces crosscheck vectors exactly (137 checks). |
| 2 | FIRST SOUND: clean audio, plausible reported DAC delay, stable buffer | S0015 `257d739` (#4), S0019, S0020 | **Code done, success NOT met.** Only ever ran against ALSA `null` (delay 0 forever). `tools/ten-minute-run.sh` refuses; `docs/verification-record.md:1084-1108`. |
| 3 | MEASUREMENT RIG: trustworthy numbers; drift matches crystal expectations | S0026 `ba6ca4f` (#8) | **Software done, NOT met.** Validated on synthetic captures only (e.g. 254.0 us declared, 253.981 measured). "no capture was ever taken here" (`verification-record.md:907`). Free-run baseline is a fixture (+37.5 ppm, `docs/measurements/free-run-baseline.conf`). |
| 4 | SYNCHRONIZATION: sustained sub-ms, zero hard resyncs at steady state, two wired Linux clients, rig-measured | S0031 `6af38bf` (#9) | **Loop implemented on the real path, NOT met.** AC-1 (1 h, median < 0.5 ms) "NOT PASSED" (`verification-record.md:597-640`). Modelled 72 h soak: worst 87.8 us vs 250 us modelled bound (`verification-record.md:283-291`). |
| 5 | EMBEDDED: I2S out, TAS5825M alive, network playback, sync core ported and measured vs Linux, survives abuse | S0039 `4a82765` (#10) | **Host-side C only, NOT met.** AC-1/AC-3 NOT PASSED (`verification-record.md:440-515`). No image ever built; amp registers unknown; no Ethernet; endpoint does not speak the control catalog (`verification-record.md:393-394`). Reconnect abuse graded on host sockets (3 outages, 130 s real outage). |
| 6 | PRODUCT HARDENING: groups, volume, control plane, minimal UI, discovery, reconnect storms, multi-day soak | S0043 `d567f06` (#11), S0054, S0063 | **Mostly done in software.** Control plane, SSE UI, mDNS + static fallback, persisted zones, restart storm (4 real clients on ALSA null) all ran. AC-4 (3-day soak) NOT PASSED; mDNS only on loopback, never across VLANs or into the container. |
| 7 | WI-FI TIER: one wireless endpoint characterized with and without power save | S0051 `1ea9f2a` (#16) | **Policy/config done, NOT met.** AC-2/AC-3 NOT PASSED (`verification-record.md:41-110`); the two jitter reports are from modelled series (`docs/measurements/rig3-jitter-wireless-ps-*.md:39`). `docs/wireless-expectations.md:8-10`: "What has been MEASURED over a real radio in a real house is nothing at all". |
| 8 | DSP: filter library fixture-validated on both platforms; active two-way demo | none | **Not started.** Zero hits for biquad/crossover/Linkwitz outside BRIEF. |
| 9 | TV/SURROUND: wired, small buffer, FEC, stereo first | none | **Not started** beyond 14 reserved header bytes (ADR 0004). No UDP/FEC code. |
| 10 | FLEET: OTA with rollback, MQTT/HA, dashboards, provisioning | none | **Not started.** No OTA partition (`firmware/sdkconfig.defaults:27-29`), no MQTT, no Prometheus/metrics endpoint. |

---

## 2. Languages and toolchain

- **Rust**: 12 workspace crates (`Cargo.toml:3-16`), edition 2021, `rust-version = "1.74"` (MSRV, `Cargo.toml:21`),
  resolver 2. **No `rust-toolchain` file**; CI uses whatever `ubuntu-latest` ships (its "Toolchain" step only prints
  versions, `.github/workflows/ci.yml:25-28`). The Docker build stage uses `rust:1.74-slim-bookworm` (pinned by digest).
  The MSRV is not tested anywhere (CI and every verification-record machine used 1.98.x), and it is **false**:
  `cargo check` on 1.74.0 fails with 2x E0369 at `crates/client-linux/src/main.rs:179-180` (`ExitCode` has no `==`
  on 1.74) (60 s incl. toolchain download).
- **Zero external crates**: `Cargo.lock` has 12 packages, all `chorus-*` path deps. Tracked since S0058 (`9b9decc`).
- **C firmware**: C11, `-Wall -Wextra -Werror -Wshadow ...` (`firmware/Makefile:23-29`), host-built with `cc`
  (gcc 14.2.0 here). Target `CONFIG_IDF_TARGET="esp32s3"` with octal PSRAM (`firmware/sdkconfig.defaults:7-13`).
  ESP-IDF pinned `v5.3` (`firmware/config/endpoint.conf:197`); `tools/firmware-image.sh` refuses without it.
- **JS**: `tools/ui` (dev-only grader), `@playwright/test` 1.56.1 via pnpm lockfile; CI installs `pnpm@10` and Chromium.
  The control page itself is vanilla JS/CSS/HTML embedded via `include_str!` (`crates/server/src/control.rs:213-217`).
- **Shell**: 33 scripts, bash.
- **Makefile**: 38 targets. Core: `check` (build + test), `build`, `test`, `verify`, `verify-pinning`, `verify-styling`,
  `verify-control-determinism`, `verify-comment-density(-suite)`, `comment-density-record`, `firmware-check`,
  `firmware-image`, `firmware-{golden-vectors,sync-scenarios,safety-scans,wireless}`, `measure-fixtures`,
  `sync-vectors`, `discovery-vectors`, `measure-fixture-reports`. Environment-gated (all refuse by name): `verify-device`,
  `verify-host`, `verify-control`, `verify-restart-storm`, `verify-ui`, `verify-soak`, `verify-mdns`,
  `verify-discovery-fallback`, `verify-measure-device`, `verify-sync-hour`, `verify-endpoint-rig`, `verify-wireless`,
  `verify-null-device`, `ten-minute-run`, `probe`.

**Installed here vs missing**

| tool | state |
|---|---|
| rustup | present but **no default toolchain** (`cargo --version` errors). mise has `rust 1.98.0`; `mise exec rust@1.98.0 -- cargo ...` works (rustfmt, clippy included) |
| gcc | 14.2.0 (`/usr/bin/gcc`) |
| node / python / go | mise: node 22.x and 24.21.0, python 3.12/3.13/3.14, go 1.23/1.25 |
| just | aqua casey/just 1.58.0 (umbrella uses `just implement` / `just land`) |
| gitleaks, shellcheck, actionlint | present via mise (none used by chorus's gate) |
| ESP-IDF / idf.py / xtensa gcc | **absent**; not in the mise registry; `IDF_PATH` unset |
| Chromium | **absent** (the PRODUCT-6 recording machine had Chromium 151; this container does not) |
| libasound.so.2 | **absent** here (earlier recording machines had it), so even the ALSA `null` runs cannot run here |
| /dev/snd | absent; `ulimit -r` = 0; `ulimit -l` = 8192 KiB |
| docker | client only, no usable daemon verified |

---

## 3. Build, test, lint: run now

| command | result | wall clock |
|---|---|---|
| `cargo build --workspace --all-targets -j2` | pass, 0 warnings | 53 s (cold) |
| `cargo test --workspace -j2` (= `make test`; `make check` is build + this) | **543 passed, 0 failed, 0 ignored** in 73 test binaries (incl. 2 doc tests) | 94 s |
| `make firmware-check` | pass: protocol 46, sync core 137, safety scans 42, clocking/pin map 72, amp 86, telemetry 23, wireless 104 = **510 checks, 0 failed**; plus 3 outage runs over real sockets | 213 s (130 s is a deliberate real outage) |
| `make verify` | pass: 159 `pass` lines, 0 fail; 20 environment-dependent entry points all refuse with exit 3 | 15 s |
| `make verify-control-determinism` | pass: 100 consecutive repetitions green in 95.3 s, plus two starved runs that correctly go red on the busy-worker refusal | 116 s |
| `cargo fmt --all -- --check` | **fails: 325 hunks in 83 of 129 .rs files** (client-linux 93, comment-density 59, server 55, measure 34, control 28, ...). Not in any gate | 1 s |
| `cargo clippy --workspace --all-targets` | exit 0 with ~50 distinct warnings (29 "doc list item overindented", 7 constant assertions, 5 neg-cmp on partial ord, ...). Not in any gate | 10 s (warm) |
| `make verify-ui` | not run: no Chromium here. Green in CI (101 s step) | |
| `cargo check --locked --workspace --all-targets` on rust 1.74.0 (declared MSRV) | **fails**, 2x E0369 in `crates/client-linux/src/main.rs:179-180` | 60 s |
| `cargo check --locked --workspace --bins` on the Dockerfile's build context (Cargo.toml, Cargo.lock, crates, fixtures, audio-path.conf) | **fails**: `include_str!` of `docs/control-page.md` at `crates/server/src/control.rs:217` | 2 s |

Local equivalent of the gate (build + test + firmware-check + verify + determinism, no verify-ui): about 491 s (8.2 min)
on 2 CPUs.

Per-crate test counts: measure 109, client-linux 100, server 73, sync 56, control 49, comment-density 41, audio-path 31,
protocol 26, discovery 21, audio 16, hostctl 15, alsa 4, doc tests 2. Slowest binaries: `server/regress_0031_f1` 20.5 s,
`client-linux/soak_72h` 13.6 s, `server/control_stalled_peer` 11.0 s, `client-linux/playout` 9.5 s.

**The merge gate** is the single CI job "build and regressions" in `.github/workflows/ci.yml` (26 steps), not a Makefile
target. It runs build, per-crate named test steps, `make verify-control-determinism`, `cargo test --workspace`, the
four firmware steps + `make firmware-check`, `make verify-pinning`, `make verify-comment-density`, `make verify`, then
installs Chromium and runs `make verify-ui`. Latest main run 34792751255: **10m26s**; critical steps: firmware-check
195 s, control determinism 113 s, verify-ui 101 s, workspace tests 67 s, "Chunking, buffering and playout" 62 s, build
14 s. No `actions/cache`, no fmt, no clippy, no secret scan, no image build.

Whether a failing CI actually blocks merge could not be read: branch-protection and rulesets APIs return 403 ("Upgrade
to GitHub Pro or make this repository public"), consistent with the repo now being private on a free plan. The umbrella
`just land` presumably enforces green CI; main went red 4 times (below), so it is not an absolute block.

---

## 4. CI history

- 1 workflow (`ci.yml`, on push + pull_request, `concurrency: cancel-in-progress`, `permissions: contents: read`),
  `actions/checkout@11d5960a...` (v4, SHA-pinned; Node 20 deprecation warning in logs).
- Last 115 runs: 85 success, 26 failure, 4 cancelled. Typical 10-11 min.
- **main branch**: 17 runs, 13 success, **4 failures on main after merge**:
  - S0021 (2026-08-24, run 32690807882): flaky `crates/client-linux/tests/playout.rs:206`
    (`a_source_faster_than_the_sink_crosses_the_maximum...`), timing-dependent.
  - S0039 (2026-09-06, run 34053792812): logs expired/unavailable.
  - S0054 (2026-09-08, run 34247162529): flaky `crates/server/tests/control_thread_population.rs:262`, later fixed by
    S0050-chorus-flaky-test-1 (`5cdd237`, ADR 0020) and by the 100-repetition determinism step.
  - S0063 (2026-09-13, run 34767146320): `make verify-comment-density` exit 3 (record disagrees with tree) because S0113
    and S0063 merged in sequence and the per-file record went stale. Fixed by the next merge (S0051).
- Pattern: the comment-density record lists every `.rs` file with counts, so **any two PRs touching Rust semantically
  conflict** on `docs/comment-density-record.md`.

---

## 5. Docs and rules

**CLAUDE.md (46 lines)** working agreement 1-8 (`CLAUDE.md:7-26`): propose before expensive-to-reverse; guardrails
(below); build over vendor for small/instructive, log gray-zone calls; sync engine is the project, simulator first,
measurement always, reports in `docs/measurements/`; protocol/sync/DSP cores are pure libraries with shared fixtures;
"verify against the datasheet" items are starting points; finish a phase's success or say why not; **rule 8 (added by
S0011, `3c4473f`)**: toolchain availability is never an admissible reason, citing umbrella ADR-0035. Umbrella section
(`CLAUDE.md:28-46`): work arrives as approved specs via `just implement`; direct changes "will not land" because only
`just land` moves the pin; tier floor `sensitive`, `critical` + human gate for eFuse burns, Proxmox deploys, OTA to
wall-mounted devices; phases cited as `chorus#<phase-id>`; BRIEF is owner's and must not be silently edited.

**BRIEF 3.1 guardrails** (`BRIEF.md:81-85`): (1) clean-room vs GPL (Snapcast GPL-3.0); (2) never enable Secure Boot,
Flash Encryption, anti-rollback eFuses on dev hardware; (3) timing claims need harness measurement; (4) monotonic clocks
only in audio/timestamp path; (5) no em dashes.

How each is enforced:
- Clean-room: by discipline only. Zero mentions of Snapcast/squeezelite/shairport in `crates/`, `firmware/`, `tools/`.
- eFuse/OTA: `firmware/check/endpoint_scan.c` safety scans (no eFuse write, no OTA activation, 42 checks, 8 red
  demonstrations), CI step "Endpoint safety scans".
- Measurement-backed: `tools/lib.sh` + `tools/unrun-checks-are-visibly-unrun.sh` (every env-dependent check must exit
  non-zero naming prerequisite and criterion; 20 found).
- Monotonic clocks: `crates/audio-path` scan over `audio-path.conf` (64 units) and `real-time-acquisitions.conf`;
  `crates/measure/tests/no_settable_wall_clock.rs`; firmware scan (no settable wall clock).
- **Em dashes: NOT enforced by any script.** Current tree: 1 occurrence, inside a regex at `tools/ui/reads.js:109`
  (a character class holding U+2013 and U+2014, so also 1 en dash). 0 em dashes in commit messages.

**Umbrella-imposed gates** (conventions live in the umbrella, not here):
- Pinning (S0058): `tools/pinning-check.sh` (237 lines) + `tools/pinning-scan.sh` (646), clauses P1-P8 of umbrella
  `documentation/pinning-conventions.md`; 7 demonstrations; offline by design. `make verify-pinning`.
- Comment density (S0113): `crates/comment-density` via `make verify-comment-density`; ceiling 90%, warn 45%, min 30
  lines (`docs/comment-density-record.md:22-25`); record regenerated with `make comment-density-record`; 6
  demonstrations; distinct exit codes 0/2-8 (`Makefile:79-97`); ADR `0021-the-comment-density-baseline.md`.
- Styling tokens (S0063): `tools/styling-check.sh` + `tools/ui/styling-scan.js`, clauses S1-S10 of umbrella
  `.sdd/conventions/styling.md`; 21 demonstrations, 13 rules; record `docs/styling-conventions-record.md`; ADR
  `0021-a-measured-ratio-is-recorded-beside-the-value.md`.
- Frontend conventions (S0054): F1-F11 rendered in Chromium by `tools/ui-render-run.sh` (`make verify-ui`); record
  `docs/frontend-conventions-record.md`.
- Wireless expectations (S0051): `tools/wireless-expectations-check.sh` derives from `config/transport.conf`.
- Interface craft (S0124, open PR #18): would add `tools/ui/interface-craft-scan.js` (1,145 lines) + 25 demo trees.

**ADRs** (`docs/decisions/`, 24 files, all "Status: decided"; dates = add commit):

| # | title | added |
|---|---|---|
| 0001 | Rust for the server and the Linux client (owner ruling 2026-08-22) | `7e50d3f` 2026-08-22 |
| 0002 | repository layout and CI shape | `7e50d3f` 2026-08-22 |
| 0003 | wire protocol framing, field layout and connection model (ports stay open) | `7e50d3f` 2026-08-22 |
| 0004 | audio chunk header reserves 14 opaque bytes for the TV path | `7e50d3f` 2026-08-22 |
| 0005 | what a decoder does with a frame it cannot accept | `7e50d3f` 2026-08-22 |
| 0006 | deterministic sync simulator and the servo it exercises | `7e50d3f` 2026-08-22 |
| 0007 | server/Linux client language, argued on the merits (confirms 0001) | `3c4473f` 2026-08-22 |
| 0008 | how the client reaches ALSA | `257d739` 2026-08-23 |
| 0009 | the transport, and telling a finished stream from a lost one | `257d739` 2026-08-23 |
| 0010 | scheduling and memory contract; refusing is the safe answer | `257d739` 2026-08-23 |
| 0011 | the audio path is an enumeration, and the list is graded | `257d739` 2026-08-23 |
| 0012 | the CPU-time bound goes on first, and a check says so | `150e3dc` 2026-08-24 |
| 0012 | the thread inventory is complete, or it is an error (**duplicate number**) | `ff892f5` 2026-08-24 |
| 0013 | the measurement rig, and the error it publishes | `ba6ca4f` 2026-09-03 |
| 0014 | the sync loop on the real path, and every constant it fixed (500 lines) | `6af38bf` 2026-09-06 |
| 0015 | the ESP32-S3 endpoint, and every constant it fixed (413 lines) | `4a82765` 2026-09-06 |
| 0016 | control catalog, version, volume range and curve (hand-written JSON) | `d567f06` 2026-09-07 |
| 0017 | control subscriber queue ceiling, dropping the subscriber | `d567f06` 2026-09-07 |
| 0018 | persisted zone state and its format | `d567f06` 2026-09-07 |
| 0019 | a persisted endpoint that never reconnects | `d567f06` 2026-09-07 |
| 0020 | checks that take a worker from the pool they grade | `5cdd237` 2026-09-12 |
| 0021 | a measured contrast ratio is recorded beside the value | `f97bf93` 2026-09-13 |
| 0021 | the comment density baseline (**duplicate number**) | `f53b447` 2026-09-13 |
| 0021 | the wireless tier (**triplicate number**) | `1ea9f2a` 2026-09-13 |

`docs/decisions/README.md` is one line.

**Measurements** (`docs/measurements/`, 6 files + 1-line README):

| file | what | source |
|---|---|---|
| `rig3-lag-fixture-reference-capture.md` | inter-device lag from `fixtures/measure/01-chirp-pair-a.wav` | synthetic fixture |
| `rig3-free-run-noiseless-fixture.md` + `free-run-baseline.conf` | +37.5000 ppm +/-0.0000 | synthetic fixture (`source = fixture`) |
| `rig3-jitter-wireless-ps-none.md` | median 523.2 us, p95 1,546.0 us | modelled series, "NOT A MEASUREMENT" |
| `rig3-jitter-wireless-ps-min-modem.md` | median 28,125.1 us, p95 79,004.7 us | modelled series, "NOT A MEASUREMENT" |
| `hostctl-thread-inventory-repeat.md` | 200 runs each side of a /proc thread-listing race | real, but host-only (not audio timing; "not the Proxmox host") |

Provenance gap: the reports cite "Build measured" SHAs `60b639b...` and `9233f2f...` that **do not exist** in this repo
(branch commits erased by squash merge); the wireless reports were written from trees with 22-23 uncommitted paths.

---

## 6. Open work

- TODO/FIXME/XXX/HACK: **0** in tracked files. `todo!()`/`unimplemented!()`: 0.
- Issues: **0** (issues enabled). PRs: 18 total, 17 merged, **1 open: #18** `sdd/S0124-chorus-interface-craft-design-record`
  (3 commits, 43 files, +2,384, CI green, mergeable, no review, created 2026-09-14).
- Remote branches: `origin/main`, `origin/sdd/S0124-chorus-interface-craft-design-record` only (merged `sdd/*` branches
  were deleted).
- "Verify against the datasheet": only BRIEF 5.5 (`BRIEF.md:184`, TAS5825M register notes). The firmware refused to
  use the BRIEF's community values: 8 keys `unknown` in `firmware/config/endpoint.conf:166-186` (I2C address, device id,
  fault, analog gain, state control, device-id value, fault-clear value, gain code).
- **BRIEF section 12 decisions** (`BRIEF.md:322-339`):

| # | decision | state |
|---|---|---|
| 1 | server/client language | decided, ADR 0001 + 0007 |
| 2 | repo layout, CI | decided, ADR 0002 (partly stale, see 7) |
| 3 | wire framing, ports, connection model | decided ADR 0003; **ports "stay open"** though 4010 is used everywhere |
| 4 | chunk size / buffer | 20 ms chunk, 120 ms start fill, 60-300 ms bounds (`config/verification.conf`), playout latency 180 ms (`config/sync.conf`); BRIEF suggested ~500 ms |
| 5 | filter params / servo gains | set in sim (ADR 0006) and for real path (ADR 0014, `config/sync.conf`); never tuned against a rig |
| 6 | hand-write vs vendor JSON/mDNS/WebSocket/MQTT | JSON hand-written (ADR 0016); mDNS hand-written with **no ADR of its own**; WebSocket **replaced by HTTP + SSE with no ADR**; MQTT open |
| 7 | dev bench hardware (board, amp module, W5500 vs RMII, measurement interface) | **OPEN, nothing chosen** |
| 8 | pin map | fixed in ADR 0015 / `endpoint.conf:120-135` without a chosen board |
| 9 | zone/group/config model | ADR 0018 |
| 10 | volume taper, mute | ADR 0016 |
| 11 | DSP v1 scope | open |
| 12 | Wi-Fi tier policy | ADR 0021-the-wireless-tier, `config/transport.conf` |
| 13 | TV capture hardware | open |
| 14 | OTA distribution/signing | open |
| 15 | HA/MQTT entity model | open |
| 16 | overturn recommendations | none logged |

Other BRIEF "Open" lines per section: 5.1 control plane in-process vs sidecar (in-process in practice); 5.2 FLAC for
Wi-Fi, single vs dual connection (control is a separate connection); 5.4 board, wired vs Wi-Fi per zone, ESP32-P4; 5.5
modules, PVDD, PoE class; 5.6 DSP scope, SRC for 44.1 k; 5.7 everything; 5.8 auth (control plane has **no auth**,
`docs/control-page.md` "There is nothing to log in to"); 5.9 OTA details, dashboards.

---

## 7. Rough edges

1. **Docker image build broken** since `0c08ef8` (S0054): Dockerfile build stage lacks `COPY docs`; reproduced.
2. **Deploy does not match the design**: `deploy/run-server.sh:52-66` uses `--publish` (bridge), not host networking, so
   mDNS cannot work from the container (BRIEF 5.8/6 say host mode); it does not pass `--control-listen`, so the deployed
   server has **no control plane or UI**; default `--source tone`. No compose, no systemd unit for the Linux client.
3. **MSRV 1.74 is false** (E0369 at `crates/client-linux/src/main.rs:179-180` on 1.74.0), untested in CI, and it is
   the Docker build toolchain, so the image fails for a second reason even after `docs/` is copied.
4. **README stale**: "What exists" covers phases 1, 2, 3, 5, 6 but **omits SYNC-4 and WIFI-7** (README last touched by
   S0043). README's "Building and testing" understates the gate (no mention of pinning/comment-density/ui/determinism).
5. **ADR 0002 stale**: `docs/decisions/0002-repository-layout-and-ci.md:87` says `Cargo.lock` is not committed and CI
   does not pass `--locked`; `verification-record.md:928` repeats "Cargo.lock is still untracked". It has been tracked
   since `9b9decc` and `tools/pinning-check.sh` asserts `--locked`.
6. **Duplicate ADR numbers**: 0012 x2, 0021 x3 (parallel specs claiming the next number).
7. **Dangling provenance SHAs** in `docs/measurements/` (section 5).
8. **Formatting**: 83/129 .rs files not rustfmt-clean; ~50 clippy warnings; neither gated.
9. **Em-dash guardrail unenforced** (1 occurrence in a regex).
10. **Rust vs C duplication is by design and wider than the cores**: firmware re-implements not just protocol and servo
    but the simulator too (`firmware/src/sync_sim.c` 309, `sync_rng.c` 91, `sync_scenario.c` 271 lines vs
    `crates/sync/src/{sim,rng,scenario}.rs` 335/94/356) so exchanges match bit-for-bit.
11. **Fixture sharing**: `fixtures/protocol` (3 pairs) and `fixtures/sync` (5+5) are shared Rust/C (read by
    `firmware/tests/test_protocol.c:587-823`, `test_sync.c:5-610`). `fixtures/control` (26) and `fixtures/discovery` (13)
    are Rust-only because the C endpoint has neither control nor mDNS. There are no DSP fixtures (no DSP).
12. **Protocol is narrower than BRIEF 5.2**: no hello/capabilities, no stream-format announcement (format is in every
    chunk header), no client telemetry message on the audio connection.
13. **Endpoint config default is wireless** (`endpoint.conf:36`) and `server_address = 127.0.0.1:4010` (`:67`).
14. **Gate friction**: per-file comment-density record forces regeneration on every Rust change and caused a red main.
15. **Tooling-to-product ratio**: `tools/` is 14,154 lines vs 29,001 lines of Rust `src`; `tools/ui` alone (6,613) is
    4.4x the UI it grades (1,505).
16. Node 20 deprecation warning for `actions/checkout` pin.

---

## 8. Dependencies

- Cargo: **0 external crates**; 12 path crates; dev-dependency cycles by design (`control` -> `server`,
  `server` -> `client-linux`, `measure` -> `audio-path`).
- Runtime libs: `libasound.so.2` dlopened at run time by `crates/alsa` (not a link dependency).
- Vendored code: **none** in tree. No LICENSE file and no SPDX headers anywhere; repo licence `null`.
- ESP-IDF v5.3 (external, pinned by version string only). ESP-IDF docs are "carried references" in the umbrella's
  `sources/` (cited in ADR 0015/0021), not in this repo.
- JS dev dep: `@playwright/test` 1.56.1, `tools/ui/pnpm-lock.yaml` committed; 3 `.npmrc` files (lifecycle scripts off,
  checked by pinning gate).
- Images: `rust:1.74-slim-bookworm@sha256:53596c66...` and `debian:bookworm-slim@sha256:88200866...`, resolved
  2026-09-08 (`deploy/Dockerfile:20,35`, `deploy/README.md`); bookworm-slim digest freezes security updates.
- Actions: `actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4`; runner `ubuntu-latest`; CI installs
  `pnpm@10` (major-only) at run time.

---

## 9. Hardware and external systems

- BRIEF names: Dell R730xd Proxmox host with Docker, HA and MQTT already running; TP-Link Omada PoE+ switch; UniFi
  Wi-Fi; VLANs; Cat6 in conduit (`BRIEF.md:26-38`). ESP32-S3 (~$15 board), TAS5825M/TAS5805M, MA12070, PCM5102A escape
  hatch, W5500 or RMII, Pi Zero 2 W / CM5 / mini PC Linux endpoints, USB audio interface for the rig, logic analyzer,
  240 fps camera.
- **What the owner owns is not stated anywhere in the repo.** Decision 7 (bench hardware) is open. No board model, amp
  module, audio interface or Linux endpoint is named outside BRIEF.
- `firmware/config/endpoint.conf`: I2S 48 kHz, 24-bit slots, MCLK x384, DMA 240 frames x 6; pins MCLK 16, BCLK 17,
  WS 18, DOUT 15, SDA 8, SCL 9, amp PDN 21; `board_octal_psram = yes`; DMA descriptors internal; analog gain ceiling
  0.0 dB (policy floor); `outage_minutes_seconds = 130`.
- eFuses: `firmware/sdkconfig.defaults:30-32` sets nothing; safety scan forbids eFuse writes and OTA activation.
- OTA: none (FLEET-10).
- Proxmox: never deployed. `deploy/run-server.sh` defaults rtprio 20, memlock 64 MiB, RLIMIT_RTTIME 200 ms, 1 CPU.
- Wi-Fi: `link_wifi_power_save = none`, coexistence `no`, SSID/secret `unknown`; `config/transport.conf` wired bound
  500 us, wireless 5,000 us, wireless buffer 120-900 ms, start fill 400 ms, playout latency 500 ms.
- `audio-path.conf`: 64 enumerated on-path/off-path units; `real-time-acquisitions.conf`: 3 files
  (`crates/server/src/hostreport.rs`, `crates/server/src/bin/chorus-rt-spin.rs`, `crates/hostctl/src/lib.rs`).
- Measurement config: capture 96 kHz, chirp 1-8 kHz, amplitude ceiling 0.25 FS, `max_lag_us 1000`
  (`config/measure.conf`).
- Operator entry points waiting on hardware (from `docs/verification-record.md`): `CHORUS_SECOND_ENDPOINT=user@host`,
  `CHORUS_CAPTURE_DEVICE=hw:1,0`, `CHORUS_CLIENT_DEVICE=hw:0,0`, `CHORUS_ESP32S3_PORT=/dev/ttyACM0`,
  `CHORUS_SOAK_SECONDS=259200`.

---

## 10. Secrets and identity

- Secrets: none needed; Wi-Fi SSID/secret declared `unknown` and never committed (`endpoint.conf:58-64`), refusal tested
  (WIFI-7 AC-17). No `.env`. **No secret scanning** in CI (gitleaks is installed locally but unused).
- Identity in tree: owner first name "the owner" in `BRIEF.md` (7 lines), `CLAUDE.md`, ADR 0001:4. Git author on all 20
  commits: `the owner <<owner email>>`.
- LAN IPs: `<private-address>` (8) and `<private-address>` (3), only in discovery fixtures/tests (synthetic). Hostnames:
  `chorus.local`, `hifi.local`, `spare.local`, `chorus-probe.local` (test names). No MAC addresses, no real hostnames.
  Proxmox/R730xd/Omada/UniFi mentioned in BRIEF.
- Visibility: now **private** (see headline 9).

---

## 11. SDD history

20 commits total on main: 3 bootstrap (`dde8f1e` seed, `0f28efc` brief + CLAUDE.md, `1a6f71f` README) and **17
squash-merged specs, one commit each**, PRs #1-#17:

| spec | kind | commit | PR | +lines |
|---|---|---|---|---|
| S0001-chorus-foundation-1 | phase 1 | `7e50d3f` | #1 | 5,140 |
| S0011-chorus-technology-review-1 | ADR | `3c4473f` | #2 | 300 |
| S0014-chorus-decisions-1 | doc fix | `b41d76f` | #3 | 21 |
| S0015-chorus-sound-2 | phase 2 | `257d739` | #4 | 12,633 |
| S0021 (decisions accuracy) | doc fix | `e27d577` | #5 | 27 |
| S0020-chorus-sound-2-docs-fix | fix | `150e3dc` | #6 | 1,454 |
| S0019-chorus-hostctl-thread-loss | fix | `ff892f5` | #7 | 1,583 |
| S0026-chorus-rig-3 | phase 3 | `ba6ca4f` | #8 | 9,015 |
| S0031-chorus-sync-4 | phase 4 | `6af38bf` | #9 | 7,293 |
| S0039-chorus-embedded-5 | phase 5 | `4a82765` | #10 | 11,329 |
| S0043-chorus-product-6 | phase 6 | `d567f06` | #11 | 13,802 |
| S0054-chorus-frontend-conventions | umbrella convention | `0c08ef8` | #12 | 5,508 |
| S0058-chorus-pinning-1 | umbrella convention | `9b9decc` | #13 | 1,257 |
| S0050-chorus-flaky-test-1 | fix | `5cdd237` | #15 | 1,088 |
| S0113-chorus-comment-prose | umbrella convention | `f53b447` | #14 | 3,006 |
| S0063-chorus-styling-tokens | umbrella convention | `f97bf93` | #17 | 4,244 |
| S0051-chorus-wifi-7 | phase 7 | `1ea9f2a` | #16 | 8,618 |
| S0124-chorus-interface-craft-design-record | umbrella convention | open | #18 | 2,384 |

Totals: 7 phase specs, 6 fix/doc specs, 4 merged + 1 open umbrella-convention specs. ~86.8k lines added in 23 days
(2026-08-21 to 2026-09-13). Spec IDs are non-contiguous (umbrella-global numbering; S0035 in `tools/ui/ui.spec.js:13` is
`S0035-holdfast-dashboard-ui`, another repo). Tests are named after spec review findings (`regress_0015_f1`,
`regress_0031_f1/F9/f6`, `regress_0043_f1`, `regress_0051_F1`, `regress_0113_F1`).

Constraints from CLAUDE.md umbrella section: no direct commits land (only `just land` moves the pin); tier floor
`sensitive`; anything burning eFuses, deploying to Proxmox or shipping OTA must propose `critical` and take the human
gate; specs cite `chorus#<phase>` and inherit acceptance from `documentation/roadmaps/chorus.md` (umbrella), which is
derived from BRIEF 8; BRIEF is owner-only.

---

## 12. Works / simulated-only / not started

**Works (graded end to end in software, on real processes and sockets, no audio hardware):**
- Wire protocol encode/decode, golden vectors, Rust and C agree byte for byte.
- `chorus-server`: tone or file/FIFO source, 20 ms chunks on a monotonic timeline, TCP serving, time-sync master,
  zones/groups, persisted state, HTTP control plane (`/api/state`, `/api/events` SSE, `/api/command`), embedded UI,
  mDNS advertise, host-contract refusals.
- `chorus-client`: receive, buffer, ALSA playout (only ever to `null`), SyncLoop on the real path, zone gain,
  discovery with static fallback, reconnect.
- Restart storm (4 clients, server SIGKILLed) and control-plane fanout ran against ALSA `null` on an earlier machine.
- Control page rendered in Chromium in CI against F1-F11.
- Measurement analysis (lag, drift, jitter reports) against synthetic captures.
- C endpoint cores on a host: protocol, sync servo + simulator, amp bring-up ORDER against a fake part, I2S/pin rules,
  Wi-Fi power-save policy against a fake radio, session supervisor against a real `chorus-server` over loopback.

**Simulated / modelled only:** sync convergence (5 scenarios), the 1-hour and 72-hour runs, wireless jitter series,
free-run drift baseline, all "measurements".

**Never done:** any audible sound; any real reported DAC delay; any capture; any two-endpoint sync number; any ESP-IDF
compile, flash or I2S clock; TAS5825M bring-up (registers unknown); Ethernet on the ESP32; the Docker image (currently
unbuildable); Proxmox deploy with real-time limits; mDNS across VLANs/containers; multi-day soak.

**Not started:** DSP-8, TV-9, FLEET-10 (OTA, MQTT/HA, metrics/dashboards, provisioning), control-plane auth, C endpoint
control-catalog support.
