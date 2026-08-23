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
| 0x03 | `stream_end` | 12 bytes, fixed |
| 0x04 to 0xFF | unassigned | skipped by a decoder that meets one |

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

### 0x03 stream end

Sent once, after the final chunk of a stream and before the connection is
closed. Minimum and canonical payload length is 12 bytes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `final_sequence` | sequence of the final chunk |
| 4 | 8 | `end_timestamp_ns` | one chunk duration past the final chunk's presentation timestamp, server timeline |

This message exists because a transport close and a transport that broke look
identical to the peer: both are a read returning zero. A receiver that saw
`stream_end` knows the sender finished; one that did not knows it lost the
sender. Timing cannot tell those apart, so a close is never the signal.

It was added by SOUND-2, after `time_sync` and `audio_chunk` were already
committed. That addition changed neither of their golden vectors, and a
decoder built before it exists steps over type 0x03 with its length prefix and
keeps the session open, which is rule 3 below. That is what the reservation of
the unassigned type space is for.

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
BRIEF.md 5.2 lists all of them, and they arrive with the phase that needs
them, as new type bytes that existing decoders already know how to skip.

## Carrying this on a stream transport

SOUND-2 puts these frames on TCP, which does not preserve message boundaries.
The framing above is what restores them: a reader accumulates bytes, decodes
whole frames from the front of its buffer, and keeps the tail. Two rules make
that safe, and both are already in the decoder behaviour above:

- A declared length longer than the bytes in hand consumes nothing, so the
  reader waits for more rather than guessing where the next frame starts.
- A frame whose type is in the catalog but whose payload is short for that
  type, or whose fields are out of range, is rejected and stepped over by its
  own length prefix. The stream stays aligned.

Alignment is only lost if a length prefix itself is wrong, which on TCP means
the peer is not speaking this protocol. A reader that finds the next header
undecodable after a correctly consumed frame reports a framing error and stops;
it never scans forward for something that looks like a header, because
resynchronising by pattern search is how mis-framed bytes reach a DAC as noise.

### Where the skip rule stops applying

Rule 3 above, "skip an unassigned type using its length prefix", is a rule
about a decoder handed one frame. It is exactly right when the transport
preserved that frame's boundaries, and it is what makes adding a type byte a
non-event for every existing implementation, including the future C mirror.

On a stream transport it needs care, and this is a property of the transport
rather than of the protocol. If alignment has already been lost, the byte a
reader reads as a "type" is a PCM sample and the two bytes it reads as a
"length" are two more, so stepping over that length is how a reader stays
lost rather than how it recovers. A reader on a stream transport is therefore
entitled to treat an unassigned type as lost alignment and stop, and the Linux
client does exactly that, for the reason that it is the component holding a
DAC. A reader whose transport preserves message boundaries skips, as rule 3
says.

Both readings are conformant. What is not conformant is stepping over an
uncorroborated length and then playing what follows.

Port numbers are still not assigned by this document. The server takes its
listen address from configuration.
