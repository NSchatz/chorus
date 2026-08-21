# chorus

From-scratch multiroom and surround audio: a containerized server that cuts PCM into timestamped chunks on one authoritative timeline, a custom sync protocol, and self-developed endpoints (ESP32-S3 firmware and a Linux client) that discipline their own playout to it.

The target is sub-millisecond inter-device error on wired endpoints, measured rather than asserted, with a lip-sync-grade TV path later. Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule, never adopted as dependencies.

- `BRIEF.md` - the guiding document: requirements, measured targets, hard guardrails, design areas with recommendations, the 10-phase roadmap, and the decisions still open.
- `CLAUDE.md` - the working agreement, and how work reaches this repo through the SDD umbrella.
- `docs/decisions/` - one file per significant decision. `docs/measurements/` - harness reports; a timing claim without one is not evidence.

Status: phase 0. Nothing is built yet.
