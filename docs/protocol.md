# The chorus wire protocol

Version: the FOUNDATION-1 catalog. Every multi-byte header and message field
is big-endian (network byte order). Every timestamp is nanoseconds from a
monotonic source on the device that took it, never wall clock.

This document and the vectors under `fixtures/protocol/` are the contract. A
second implementation is correct when it produces those bytes and recovers
those fields, not when it matches the Rust code.

Why it is shaped this way: `docs/decisions/0003-wire-protocol-framing.md`.
What a decoder does when it cannot accept a frame:
`docs/decisions/0005-decoder-frame-validation.md`.

## Frame

```
 0        1                 3                        3 + payload length
 +--------+-----------------+------------------------+
 | type   | payload length  | payload                |
 | u8     | u16 big-endian  | payload length bytes   |
 +--------+-----------------+------------------------+
```

- Header is 3 bytes.
- Maximum payload is 65535 bytes, so the largest frame is 65538 bytes.
- Frames are self-delimiting, so any mix of types shares one connection.

## Message catalog

| type | name | payload |
|---|---|---|
| 0x00 | unassigned, never used | an all-zero buffer is an unknown type, not a message |
| 0x01 | `time_sync` | 32 bytes, fixed |
| 0x02 | `audio_chunk` | 32-byte chunk header, then PCM |
| 0x03 to 0xFF | unassigned | skipped by a decoder that meets one |

### 0x01 time sync

The four timestamps of one RFC 5905 section 8 exchange. Minimum and canonical
payload length is 32 bytes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `t0_ns` | client transmit, client clock |
| 8 | 8 | `t1_ns` | server receive, server clock |
| 16 | 8 | `t2_ns` | server transmit, server clock |
| 24 | 8 | `t3_ns` | client receive, client clock |

From these, `rtt = (t3 - t0) - (t2 - t1)` and
`offset = ((t1 - t0) + (t2 - t3)) / 2`. The offset is exact only for a
symmetric path, which is why the client filters a window of them rather than
trusting one.

The two clocks have unrelated epochs. Comparing a `t0` with a `t1` as if they
were the same timeline is the mistake this whole exchange exists to avoid.

### 0x02 audio chunk

A 32-byte chunk header followed by PCM. Minimum payload length is 33 bytes: a
chunk with no samples is not representable.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `sequence` | per stream, wraps |
| 4 | 8 | `timestamp_ns` | presentation time of the first sample, server timeline |
| 12 | 4 | `sample_rate_hz` | 8000 to 384000 |
| 16 | 1 | `channels` | 1 to 8 |
| 17 | 1 | `sample_format` | 1 `pcm_s16le`, 2 `pcm_s24le`, 3 `pcm_f32le` |
| 18 | 14 | reserved | opaque, no semantics; see below |
| 32 | rest | PCM | in the announced format's byte order |

The PCM byte count must be a nonzero whole number of frames, where a frame is
`channels * bytes per sample` and bytes per sample is 2, 3 or 4 for the three
formats above.

The PCM bytes are the one part of the protocol that is not big-endian: they
are carried exactly as the announced format names them (all three defined
formats are little-endian) because they pass through to a DAC untouched.

#### The reserved block

Fourteen bytes at offset 18, reserved for the low-latency TV path. See
`docs/decisions/0004-audio-chunk-reserved-bytes.md` for why there are fourteen
and why they are opaque.

Rules, which every implementation has to follow for the reservation to be
worth anything:

- An encoder writes zeros.
- A decoder ignores their value. It does not require zeros, and it does not
  reject a frame because they are not zero.
- A decoder makes the raw bytes available to its caller rather than dropping
  them.

## Decoder behaviour

In order. The order is the contract: it decides which answer a frame gets.

1. Fewer than 3 bytes remain: reject, truncated header. The next frame
   boundary is unknown, so stop consuming this buffer.
2. The payload length field exceeds the bytes actually remaining: reject. The
   next frame boundary is unknown, so stop consuming this buffer. This check
   comes before any payload is sliced.
3. The type byte is not in the catalog: skip the frame using the length
   prefix, and carry on with the next one. This is not an error.
4. The payload length is below the declared type's minimum: reject that frame,
   then carry on with the next one, which the length prefix locates.
5. A field value is not one the format accepts: reject that frame, then carry
   on.

A payload longer than the fields a decoder knows about is accepted and the
excess ignored, so that a field added later is not fatal to a decoder built
today.

None of the above closes a session. A malformed frame costs one frame.

## Encoder behaviour

An encoder refuses, and emits nothing at all, when:

- a value does not fit its wire field (for example more than 255 channels in a
  `u8`). It is never truncated and never allowed to wrap;
- the payload would exceed 65535 bytes;
- a field carries a value the decoder above would reject.

So valid encoder output always decodes. That invariant is asserted in
`crates/protocol/tests/encoder_range.rs`.

## Not in this catalog yet

The session hello and capabilities exchange, the stream format announcement,
control messages (volume, grouping, configuration) and client telemetry.
BRIEF.md 5.2 lists all of them and this phase has no connection to carry any
of them, so they arrive with the phase that does, as new type bytes that
existing decoders already know how to skip.

Port numbers are deliberately unassigned. Nothing binds a socket yet.
