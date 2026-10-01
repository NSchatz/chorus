# 0091: the TV path's low-latency wire is UDP datagrams under a per-offer ChaCha20-Poly1305 key with a 1024-wide replay window, one XOR parity per 4 chunks of 2.5 ms with an optional column interleave, offered and accepted inside the session, and L_tv 20 ms over a cited 19.4 ms floor

- Status: accepted (goal 13, 2026-10-01)
- Decided by: BRIEF.md 5.7 ("UDP + simple XOR parity FEC, wired only, stereo first") and 5.2;
  P2 Option A approved at Checkpoint K (program section 17); the coordinator's goal-13 design
  envelope, section 1 and "Defaults" (track `chorus-g13/lowlat-wire`); ADR 0004 (the reserved
  block, kept for TCP); ADR 0039 (the vendored AEAD); the research note `fec-latency.md`; the
  placements below where the envelope left them open
- Implemented in: `crates/protocol/src/v2/lowlat.rs` (datagram, `seal`/`open`, `Sealer`,
  `Opener`, `ReplayWindow`, `ChunkInfo`, `FecParams`, `FecEncoder`, `FecDecoder`, `FecRelay`,
  `Plan`/`DEFAULTS` and the budget), `catalog.rs`, `messages.rs`, `codec.rs` (0x16, 0x17,
  `capabilities.features`); `firmware/include/chorus/lowlat.h`, `firmware/src/lowlat.c` (host
  `LIB_SRC`, ESP-IDF component sources, `firmware/endpoint-units.conf`),
  `firmware/include/chorus/protocol_v2.h`, `firmware/src/protocol_v2.c`;
  `crates/sync/src/lowlat_sim.rs`, `crates/sync/src/bin/chorus-sim-lowlat.rs`,
  `config/sim-lowlat/*.lowlat`; held by `crates/protocol/tests/lowlat.rs`,
  `crates/protocol/tests/v2_vectors.rs` and `v2_rules.rs`, `firmware/tests/test_lowlat.c` and
  `test_protocol_v2.c` (`make firmware-check`), `crates/sync/tests/lowlat_budget.rs`; vectors
  `fixtures/protocol/lowlat/` (shared), `fixtures/protocol/v2/{low_latency_*,capabilities_low_latency}`
  and `fixtures/protocol/v2/rejected/low_latency_*`; report
  `docs/measurements/low-latency-budget-sim.md`

## Context

A TV input played in a wired room cannot ride the slot path: its 180 ms playout latency is
lip sync off by an order of magnitude. BRIEF.md 5.7 names the direction and ADR 0004 kept 14
bytes of every chunk header for it. The integration track (server relay, hub capture, Linux
client receive) needs a finished wire under it: messages to set a stream up, a datagram both
implementations seal and open to the same bytes, an FEC that repairs a lost chunk without a
retransmit, and defaults whose latency is accounted for term by term.

## What was read

All read 2026-10-01. RFC 8439 section 2.8 (https://www.rfc-editor.org/rfc/rfc8439: "a 96-bit
nonce, different for each invocation with the same key"; a 128-bit tag). RFC 4303 section 3.4.3
(https://www.rfc-editor.org/rfc/rfc4303#section-3.4.3: a window of at least 32, 64 by default,
and "The receive window is updated only if the integrity verification succeeds"). Through the
research note `fec-latency.md` (its citations, each read 2026-10-01): RFC 5109 sections 7.3, 9,
15 (XOR parity, length recovery, zero padding, received packets delivered at once); RFC 2733;
RFC 8681 section 1.2 (a block code's repair waits for the parity after the block); the vbrick
SMPTE ST 2022-1 page (column FEC, L x D limits); the TI DIR9001 and PCM5102A datasheets
(3/fS and 20 tS); the RAVENNA AES67 guide (3 ms suggested playout delay); avlatency.com's
measurement examples (one TV, LEAD); ITU-R BT.1359-1 (the sign convention); Hasslinger and
Hohlfeld 2008 citing Gilbert 1960 and Elliott 1963. No GPL source was opened; Roc Toolkit was
read through its documentation only.

## Decision

1. **Setup inside the session.** `0x16 low_latency_offer` (server to endpoint, 53 bytes fixed)
   and `0x17 low_latency_accept` (7 bytes), exactly the envelope's fields, with value rules
   rejected as their field (vectors in `v2/` and `v2/rejected/`). A new trailing optional
   `capabilities.features` byte, bit 0 `low_latency`: written only when nonzero so every
   existing vector is byte for byte unchanged; an undefined bit is kept, not rejected (a feature
   a server does not know is one it does not use).
2. **The datagram**, at most 1472 bytes: the 16-byte header `CL`, 1, kind, `stream_tag`,
   `counter` as AEAD associated data; ChaCha20-Poly1305 under the offer's fresh key with nonce
   `stream_tag || counter`. A key is per offer, so the session's Noise keys are never exported
   (`noise.rs` exposes none, and needs none). Receive checks in a fixed order (form, tag, window,
   AEAD), each counted.
3. **The replay window** is 1024 counters (the envelope's "at least 1024"; RFC 4303's default is
   64, but at 400 datagrams a second 64 is 160 ms, shorter than a plausible reorder after a
   stall), moved only after the tag verified.
4. **The reserved block** on this path: 18 `ll_marker` 1, 19 `fec_k`, 20 `fec_depth`, 21
   `group_index`, 22..26 `group`, 26..32 zero. ADR 0004's TCP rule stands.
5. **The FEC**: one XOR parity per `k` data chunks (the parity plaintext is `group`, `k`,
   depth, `length_xor`, then the XOR, zero-padded to the longest), sent right after the group's
   last chunk; groups of depth `D` are the columns of a block of `k x D`. Received chunks are
   handed on at once, a rebuild the moment its group allows; a rebuild that does not carry its
   own group's reserved block is rejected, never played. A decoder holds three blocks
   (`OPEN_BLOCKS`), closing a group when a datagram two blocks newer arrives or the playout point
   passes it, and counts delivered, recovered, unrecoverable, late, duplicate and rejected.
6. **The relay keeps the groups** (`FecRelay`): the server forwards each chunk with its hub
   numbering after restamping, and a group's downstream parity leaves with the group's last
   chunk. The FEC wait is then paid once from capture to speaker, which is what lets the
   budget below fit 20 ms; re-encoding with a fresh numbering would pay it twice.
7. **Defaults** (`DEFAULTS`): 120 frames (2.5 ms) a chunk, `k` 4, depth 1, `L_tv` 20 ms
   (configurable 10 to 40 ms). **The floor rule**: an `L_tv` below capture buffering (2.0) +
   one chunk (2.5) + the FEC wait `(k-1) x D` chunks (7.5) + two UDP legs (2 x 0.25) + the relay
   (0.5) + a jitter margin (2.0) + the endpoint's output path (4.0) + the DAC filter (0.417, cited)
   = 19.417 ms is refused, never raised. Lip sync adds the S/PDIF receiver (0.0625, cited) and the
   TV's own audio lag (ASSUMED per scenario): -21.06 ms against [-40, +15] at the defaults with a
   standard-mode TV.
8. **The simulator** runs the real library over both legs with Bernoulli and Gilbert-Elliott
   loss, each scenario against a no-FEC control on identical draws, FIFO legs. At 1e-3
   independent loss per leg the residual is 0 with FEC against 521 of 240000 chunks without
   (computed expectations 1.9 and 479.8); with bursts of mean 2 chunks it is 340 with depth 1
   and 243 with depth 2 against 469; below the floor (L_tv 15.4 ms) 121 repaired chunks are late.
   Simulation, not timing evidence.

## ASSUMED values (each replaced by a measurement or a fact)

Capture buffering 2 ms (two 1 ms ALSA periods; bench session S8); each UDP leg 0.25 ms and the
relay 0.5 ms (no chorus LAN measurement); jitter margin 2 ms; endpoint output path 4 ms (BRIEF.md
5.7's 2 to 10 ms); `L_tv` 20 ms; the interleave bound 8 and `OPEN_BLOCKS` 3; the simulated legs'
80 us + exponential 20 us delay and their loss models; the TV's audio lag (1 ms standard mode, 66
ms game mode, one set, LEAD), which names the Needs item "The three TVs: model, eARC port,
optical out and audio menu".

## Deviations from the envelope

- The FEC wait is `(k - 1) x depth` chunks, not `k` chunks: the first chunk of a group waits for
  the parity sent with its last, and the chunk's own fill is counted separately. The floor is
  still the envelope's sum, one chunk lower.
- `FecRelay` (server-side, Rust only) is added to the library so the integration track gets the
  group-aligned relay rather than a second encoder.
- Field bounds the envelope left open: `chunk_frames` 1 to 700 (a data plaintext must leave the
  parity's 8 bytes room in 1472), `fec_depth` at most 8, `latency_ns` at most 5 s, a refusal's
  port 0, an end's other fields zero.
- The C mirror imports the key per call (as the free functions of `noise.c` would); a long-lived
  PSA key per stream is an integration-time optimisation for the ESP32-S3.

## Consequences and follow-ups (integration track `tv-path`)

- Server: offer with a fresh random key and a process-unique tag; refuse an offer whose chunk
  does not fit (`chunk_fits`); `Opener` + `FecDecoder` on the hub's stream, restamp
  `play_at = capture_stamp + max(floor, L_tv + trim)`, `FecRelay` + `Sealer` per player; call
  `expire_before` as playout passes groups; surface the counts in telemetry.
- Linux client: set `features::LOW_LATENCY` when wired; `Opener` + `FecDecoder`; a jitter buffer
  that drops chunks past their playout point and crossfades a missing one; refuse with the
  status that names why.
- Endpoint firmware: the layer and its vectors are in; the UDP socket, the session wiring and a
  per-stream PSA key are not.
- 5.1 at s24 needs 60-frame chunks (or s16) to fit one datagram.
- Bench: a 24 h loss and burst histogram on the owner's LAN, and S8, replace the ASSUMED legs;
  if bursts are real, depth 2 (with `L_tv` 30 ms) is the measured choice the simulator already
  prices.

## Revisit when

A LAN measurement shows bursts or a jitter beyond the margin, the TVs are known, or the floor
moves past 20 ms.
