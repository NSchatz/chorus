# chorus

From-scratch multiroom and surround audio: a containerized server that cuts PCM into timestamped chunks on one authoritative timeline, a custom sync protocol, and self-developed endpoints (ESP32-S3 firmware and a Linux client) that discipline their own playout to it.

The target is sub-millisecond inter-device error on wired endpoints, measured rather than asserted, with a lip-sync-grade TV path later. Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule, never adopted as dependencies.

- `BRIEF.md` - the guiding document: requirements, measured targets, hard guardrails, design areas with recommendations, the 10-phase roadmap, and the decisions still open.
- `CLAUDE.md` - the working agreement, and how work reaches this repo through the SDD umbrella.
- `docs/decisions/` - one file per significant decision. `docs/measurements/` - harness reports; a timing claim without one is not evidence.
- `docs/protocol.md` - the audio wire format, which is what a second implementation is held to. `docs/control-plane.md` - the control catalog, which is a separate contract on a separate connection and held the same way.

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

**Phase 2, first sound.** PCM goes in one end and comes out of a speaker at the
other, with the delay the device reports written down so someone who did not
run it can check the claim. `docs/sound-2.md` is the guide: how to run it, every
number it chose and the arithmetic behind them, and which half of the phase's
roadmap outcome it delivers.

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
- `crates/audio-path` and `audio-path.conf` - the audio path enumerated, and
  the two checks that keep the enumeration honest (`docs/decisions/0011`); plus
  `real-time-acquisitions.conf` and the third check on it, that every
  real-time acquisition applies the CPU-time bound before it takes the
  scheduling policy (`docs/decisions/0012`).

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
  `tools/discovery-fallback-run.sh`, `tools/ui-render-run.sh`,
  `tools/mdns-live-run.sh`, `tools/soak-run.sh` - one entry point per criterion.
  The last of those is NOT PASSED anywhere: it needs three days of wall clock
  and the RIG-3 rig, and it refuses by name.

## Building and testing

Stable Rust and a C compiler, no external dependencies. `libasound.so.2` is
needed to play audio and is not needed to build or to run the suite.

```
cargo build --workspace
cargo test --workspace     # or: make test
make verify                # refusal paths, and that unrun checks are visibly unrun
make firmware-check        # the ESP32-S3 endpoint, on a host, with no ESP-IDF
```

CI runs those on every change, with the golden-vector round trip, the simulator
regression, the settable-clock check and the endpoint's three regressions as
separately named steps.

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
./tools/ui-render-run.sh         # needs Chromium and the driver under tools/ui
./tools/mdns-live-run.sh         # needs a link that carries multicast DNS
./tools/soak-run.sh              # needs three days of wall clock AND the rig
```

`docs/verification-record.md` says, per criterion, which of those actually ran
where this was built and which did not, with the refusal quoted.
