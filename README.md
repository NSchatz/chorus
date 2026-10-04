# chorus

From-scratch multiroom and surround audio: a containerized server that cuts PCM into timestamped chunks on one authoritative timeline, a custom sync protocol, and self-developed endpoints (ESP32-S3 firmware and a Linux client) that discipline their own playout to it.

The target is sub-millisecond inter-device error on wired endpoints, measured rather than asserted, with a lip-sync-grade TV path later. Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule, never adopted as dependencies.

- `BRIEF.md` - the guiding document: requirements, measured targets, hard guardrails, design areas with recommendations, the phased roadmap (section 8), and the decisions still open.
- `CLAUDE.md` - the working agreement; work arrives as tasks from the owner's agent harness.
- `docs/decisions/` - one file per significant decision. `docs/measurements/` - harness reports; a timing claim without one is not evidence.
- `docs/protocol.md` - the audio wire format, which is what a second implementation is held to. `docs/control-plane.md` - the control catalog, which is a separate contract on a separate connection and held the same way. `docs/mqtt.md` - the opt-in, read-only MQTT publisher (off by default): its topics, what it does not publish, and the broker notes for the owner. `docs/inputs.md` - the four alarm sources, line-in sharing to any group and a streamer on a line-in, with the rule about URLs. `docs/upnp.md` - the opt-in UPnP AV media renderers (off by default): a renderer per room, saved group and live group, its flags and port, what is refused, and what was and was not tested.

## Where each phase stands

Each of the ten phases of BRIEF.md section 8, graded against its "success looks like" there. The
per-criterion record is `docs/verification-record.md`; the cold audit of every phase is
`docs/audit/2026-09-audit.md`. Nothing has been heard, flashed or deployed: **every criterion that
needs hardware is NOT PASSED**, and no report in `docs/measurements/` is a hardware measurement.

| Phase | Status (2026-09-30) |
|---|---|
| FOUNDATION-1, foundation | met in simulation only: the Rust and C cores read the same fixtures and agree exchange by exchange; the jitter model is narrow (audit A-1) |
| SOUND-2, first sound | code written; nothing heard; the hardware criteria NOT PASSED (`docs/sound-2.md`) |
| RIG-3, measurement rig | analysis written and checked against synthetic fixtures; no capture from real devices; the hardware criteria NOT PASSED |
| SYNC-4, synchronization | the sync loop is written on the real path; the one-hour two-endpoint criterion NOT PASSED (no rig, no endpoints) |
| EMBEDDED-5, embedded bring-up | host-built C cores and an ESP32-S3 image that compiles in the gate; the playout path (jitter buffer, DMA-consumed frames stamped in the I2S interrupt, the servo) is written and graded on a host against a fake DMA (ADR 0058); never flashed, never driven a pin; the hardware criteria NOT PASSED |
| PRODUCT-6, product hardening | zones, groups, volume, the control plane, the page and discovery work in software for Linux endpoints; the multi-day soak NOT PASSED; not deployed |
| WIFI-7, Wi-Fi tier | expectations published and the jitter analysis written; no wireless endpoint characterized; the hardware criteria NOT PASSED |
| DSP-8, DSP | not started |
| TV-9, TV/surround path | not started (the audio chunk header reserves bytes for it, `docs/decisions/0004`) |
| FLEET-10, fleet | not started |

The program's later phases (protocol v2, rooms and groups, inputs, the Home Assistant integration,
the app, and the hardware designs) are planned in BRIEF.md section 8 (8.1) and the retired program
plan, [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md); since 2026-10-04 they arrive as
tasks from the owner's agent harness.

## What exists

**Phase 1, foundation.** The two things every later phase needs before any
hardware exists.

- `crates/protocol` - the wire format. Encoder, decoder, and a session that
  survives frames it cannot use. Held to the committed golden vectors in
  `fixtures/protocol/`, which is what will keep the later C implementation from
  drifting away from this one.
- `crates/sync` - a deterministic, seeded simulator: two virtual clocks with
  configurable ppm skew, injected network jitter, the RFC 5905 exchange, and
  the servo that disciplines a modelled playout pointer against it. Scenarios
  live in `fixtures/sync/`.

**Phase 2, first sound.** Built to take PCM in at one end and play it out of a
speaker at the other, with the delay the device reports written down so someone
who did not run it can check the claim. Status: code written; nothing has been
heard yet (the audit, `docs/audit/2026-09-audit.md`, grades every phase). `docs/sound-2.md` is the guide: how to run it, every
number it chose and the arithmetic behind them, and where each part of BRIEF.md's
"success looks like" for the phase stands.

- `crates/audio` - ingest, chunking, and the one monotonic timeline the server
  stamps on.
- `crates/server` - takes PCM, cuts it into timestamped chunks, serves them.
- `crates/client-linux` - receives, buffers, plays out through ALSA, logs the
  delay the device reports and the occupancy beside it.
- `crates/alsa` - the audio device, bound at run time so the build gains
  nothing (`docs/decisions/0008`).
- `crates/hostctl` and `deploy/` - the container's scheduling contract: the
  granted `rtprio` ceiling, a CPU-time bound on every real-time thread, and the
  memory-locking decision (`docs/decisions/0010`).
- `crates/hostprobe` - host evidence: a wakeup-jitter probe and a kernel
  receive-timestamp check, run here and in the deployed container
  (`docs/measurements/host-*.md`).
- `crates/audio-path` and `audio-path.conf` - the audio path enumerated, and
  the two checks that keep the enumeration honest (`docs/decisions/0011`); plus
  `real-time-acquisitions.conf` and the third check on it, that every
  real-time acquisition applies the CPU-time bound before it takes the
  scheduling policy (`docs/decisions/0022-the-cpu-time-bound-goes-on-first.md`).

**Phase 3, the measurement rig.** Built before the servo on purpose: guardrail 3
says a sync claim without a measurement is not a claim, and a harness written
after the servo tends to be written to agree with it. It measures; it corrects
nothing and holds no view about whether a number is good.

- `crates/measure` - a two-channel capture in, median, p95 and maximum
  inter-device lag out, resolved finer than one capture sample; the free-run
  drift of two uncorrected clients in ppm, published with its own confidence
  bound or not published at all; and the saved report. Every threshold it
  compares against is declared in `config/measure.conf`
  (`docs/decisions/0013`).
- `fixtures/measure` - the committed captures and offset series, each beside
  the parameters it was generated from, including the degenerate ones. A reader
  who did not take a capture can reproduce every published figure.
- `tools/measure/` - the device-backed run, which needs two endpoints and an
  audio interface, and the refusal it makes where there is none.

**Phase 4, synchronization.** The servo from phase 1, closed on the real path.

- `crates/client-linux/src/sync.rs` - the loop on the delay the device reports:
  min-RTT filtering, a PI rate law applied as sample insert and drop, and a step
  only under a mute. Its constants live in `config/sync.conf`
  (`docs/decisions/0014`), whose modelled figures are labelled as models.
- `tools/sync-hour-run.sh` - the one-hour two-endpoint run the phase is graded
  on. It needs two wired endpoints and the RIG-3 rig and refuses by name; the
  criterion is NOT PASSED.

**Phase 5, the ESP32-S3 endpoint.** The second kind of endpoint the project
exists to have: a microcontroller speaker that joins a group, disciplines its
playout to the same server timeline, drives a TAS5825M-class I2S amplifier and
comes back by itself after an outage.

- `firmware/` - a C implementation of the protocol core and the sync core, held
  to the same committed fixtures the Rust ones are and, exchange by exchange,
  to what the Rust sync core actually did; the amplifier bring-up sequencer,
  graded on the ORDER against a simulated part; the I2S clock and pin rules,
  enforced at build time; the session supervisor, graded over a real socket
  with a real server killed under it; and the safety scans
  (`docs/decisions/0015`).
- `firmware/config/endpoint.conf` - every value the endpoint needs, and every
  value this phase deliberately does NOT fix. **The amplifier's register map is
  DECLARED UNKNOWN**: the datasheet is normative and unread, so the phase
  asserts the bring-up behaviour and names no address.
- `tools/endpoint-rig-run.sh` - the hardware-attended run, and the refusal it
  makes where there is no ESP32-S3.

The endpoint has **never driven a pin**. Nothing here has been heard.

**Phase 6, zones, groups, control and a UI.** The phase that makes the system
usable by someone who did not build it: rooms with names, groups, volume, mute,
a page to see them on, an endpoint that finds its server by itself, and every
endpoint back to playing after a server restart with nobody doing anything.

- `crates/control` - the versioned JSON control catalog, the
  server-authoritative zone state it changes, that state's persisted form, and
  the bounded fanout that gets a change to every subscriber. No socket, no
  clock, no thread, and no third-party dependency: the JSON codec is written
  here for the same reason `crates/protocol` is (`docs/decisions/0016`).
  `docs/control-plane.md` is the contract and `fixtures/control/` pins its
  bytes.
- `crates/ctl` - `chorusctl`, the command line over that control API: rooms,
  groups, volume, inputs, endpoints and updates, with `--json` and documented
  exit codes. It sends the catalog's own bytes (held to `fixtures/control/`)
  and depends on `crates/control` alone. `docs/chorusctl.md` is its page.
- `crates/discovery` - multicast DNS and DNS-SD, enough of both for a server to
  say where it is and an endpoint to hear it, with `fixtures/discovery/` pinning
  the query and response packets so a second implementation is graded against
  them. The static fallback is an assertion and not a nicety: an endpoint with
  neither discovery nor an address exits saying which of the two it lacked.
- `crates/server/src/control.rs` and `crates/server/src/ui/` - the control
  channel and the page it serves. Every thread it needs is created before the
  scheduling report, for the reason `crates/server/src/clients.rs` gives about
  its own.
- `crates/client-linux/src/zone.rs` and `src/control.rs` - the endpoint's zone:
  one atomic the playout loop reads, applied to the PCM at the last point before
  the sink. Graded on the samples a modelled device accepted, never on a status
  line.
- `tools/control-plane-run.sh`, `tools/restart-storm-run.sh`,
  `tools/discovery-fallback-run.sh`, `tools/mdns-live-run.sh`,
  `tools/soak-run.sh` - one entry point per criterion.
  The last of those is NOT PASSED anywhere: it needs three days of wall clock
  and the RIG-3 rig, and it refuses by name.

**Phase 7, the Wi-Fi tier.** One wireless endpoint, held to the looser targets
and characterized with and without power save.

- `docs/wireless-expectations.md` and `config/transport.conf` - the published
  expectations and the per-zone tier (`docs/decisions/0024-the-wireless-tier.md`).
- `firmware/src/wifi.c` and `crates/measure/src/jitter.rs` - the endpoint's link
  and the jitter analysis. The two reports in `docs/measurements/` run it over
  synthetic series, not a radio.
- `tools/wireless-characterization-run.sh` - the characterization, which needs a
  wireless ESP32-S3 in another room and the rig, and refuses by name; its
  criteria are NOT PASSED.

## Building and testing

Rust 1.98.1 (pinned in `rust-toolchain.toml`; rustup installs it on first use) and a
C compiler, no external dependencies. `libasound.so.2` is
needed to play audio and is not needed to build or to run the suite.

```
cargo build --workspace
cargo test --workspace     # or: make test
make verify                # refusal paths, and that unrun checks are visibly unrun
make firmware-check        # the ESP32-S3 endpoint, on a host, with no ESP-IDF
```

`make gate` is the check every change passes before it merges: formatting,
clippy with warnings denied, the build and every test, the control-plane
determinism run, the endpoint's host checks, the refusal paths, the ESP32-S3
image compile (ESP-IDF v6.1 through ccache) and the daemonless server image,
each step timed. `make gate-fast` runs the docs checks alone. The toolchain is
pinned in `rust-toolchain.toml`. CI (`.github/workflows/ci.yml`) calls
`make gate`, but GitHub Actions does not run on this private repository, so the
local run is the gate.

The server and the Linux client speak protocol v2 on the audio connection:
every session is encrypted, and each side keeps a long-term key. The server
takes `--identity-dir <dir>` (or uses the directory of `--state-file`) and the
client takes `--identity-dir <dir>` and `--endpoint-id <id>`; either takes
`--ephemeral-identity` for a throwaway run, which is what the tests and the
`tools/` scripts pass. `deploy/README.md` has the files and the log lines.

The verifications that need an environment are in `tools/`, one per check.
Each exits non-zero naming the prerequisite it is missing and the criterion it
was verifying, rather than reporting itself green:

```
./tools/ten-minute-run.sh        # needs a real audio device
./tools/spin-test.sh             # needs a granted rtprio ceiling above zero
./tools/host-contract.sh         # needs a granted rtprio ceiling above zero
./tools/device-loss-run.sh       # needs a device you can remove mid-run
./tools/measure/capture-run.sh   # needs two endpoints and an audio interface
./tools/firmware-image.sh        # needs ESP-IDF at the declared version
./tools/endpoint-rig-run.sh      # needs an ESP32-S3, an amplifier and the rig
./tools/control-plane-run.sh     # needs a playback device that opens
./tools/restart-storm-run.sh     # needs a playback device that opens
./tools/discovery-fallback-run.sh # needs a playback device that opens
./tools/mdns-live-run.sh         # needs a link that carries multicast DNS
./tools/soak-run.sh              # needs three days of wall clock AND the rig
```

`docs/verification-record.md` says, per criterion, which of those actually ran
where this was built and which did not, with the refusal quoted.
