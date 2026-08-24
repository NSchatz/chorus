# chorus

From-scratch multiroom and surround audio: a containerized server that cuts PCM into timestamped chunks on one authoritative timeline, a custom sync protocol, and self-developed endpoints (ESP32-S3 firmware and a Linux client) that discipline their own playout to it.

The target is sub-millisecond inter-device error on wired endpoints, measured rather than asserted, with a lip-sync-grade TV path later. Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule, never adopted as dependencies.

- `BRIEF.md` - the guiding document: requirements, measured targets, hard guardrails, design areas with recommendations, the 10-phase roadmap, and the decisions still open.
- `CLAUDE.md` - the working agreement, and how work reaches this repo through the SDD umbrella.
- `docs/decisions/` - one file per significant decision. `docs/measurements/` - harness reports; a timing claim without one is not evidence.
- `docs/protocol.md` - the wire format, which is what a second implementation is held to.

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

There is **no correction of any kind** yet. Two endpoints agreeing is a later
phase, and this one asserts nothing about it.

## Building and testing

Stable Rust, no external dependencies. `libasound.so.2` is needed to play
audio and is not needed to build or to run the suite.

```
cargo build --workspace
cargo test --workspace     # or: make test
make verify                # refusal paths, and that unrun checks are visibly unrun
```

CI runs those on every change, with the golden-vector round trip, the simulator
regression and the settable-clock check as separately named steps.

The verifications that need an environment are in `tools/`, one per check.
Each exits non-zero naming the prerequisite it is missing and the criterion it
was verifying, rather than reporting itself green:

```
./tools/ten-minute-run.sh        # needs a real audio device
./tools/spin-test.sh             # needs a granted rtprio ceiling above zero
./tools/host-contract.sh         # needs a granted rtprio ceiling above zero
./tools/device-loss-run.sh       # needs a device you can remove mid-run
```
