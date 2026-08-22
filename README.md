# chorus

From-scratch multiroom and surround audio: a containerized server that cuts PCM into timestamped chunks on one authoritative timeline, a custom sync protocol, and self-developed endpoints (ESP32-S3 firmware and a Linux client) that discipline their own playout to it.

The target is sub-millisecond inter-device error on wired endpoints, measured rather than asserted, with a lip-sync-grade TV path later. Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule, never adopted as dependencies.

- `BRIEF.md` - the guiding document: requirements, measured targets, hard guardrails, design areas with recommendations, the 10-phase roadmap, and the decisions still open.
- `CLAUDE.md` - the working agreement, and how work reaches this repo through the SDD umbrella.
- `docs/decisions/` - one file per significant decision. `docs/measurements/` - harness reports; a timing claim without one is not evidence.
- `docs/protocol.md` - the wire format, which is what a second implementation is held to.

## What exists

Phase 1 of the roadmap, foundation: the two things every later phase needs
before any hardware exists.

- `crates/protocol` - the wire format. Encoder, decoder, and a session that
  survives frames it cannot use. Held to the committed golden vectors in
  `fixtures/protocol/`, which is what will keep the later C implementation from
  drifting away from this one.
- `crates/sync` - a deterministic, seeded simulator: two virtual clocks with
  configurable ppm skew, injected network jitter, the RFC 5905 exchange, and
  the servo that disciplines a modelled playout pointer against it. Scenarios
  live in `fixtures/sync/`.

Both are pure libraries. Nothing opens a socket, reads a clock, or plays a
sound yet.

## Building and testing

Stable Rust, no external dependencies.

```
cargo build --workspace
cargo test --workspace
```

CI runs exactly that on every change, with the golden-vector round trip and
the simulator regression as separately named steps.
