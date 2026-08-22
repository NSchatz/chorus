# 0003: wire protocol framing, field layout and connection model

- Status: decided (port numbers stay open, see below)
- BRIEF.md section 12, decision 3
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)

## Decision

Length-prefixed binary frames on the hot path:

```
offset 0      message type   u8
offset 1..3   payload length u16, big-endian (network byte order)
offset 3..    payload        exactly `payload length` bytes
```

Header length is 3 bytes and the maximum payload is 65535 bytes. The full field
layout of every catalogued message is in `docs/protocol.md`; that document is
the contract the C mirror is written against, and the committed vectors under
`fixtures/protocol/` are the bytes it is held to.

Connection model: one multiplexed connection per device. The framing above is
self-delimiting, so every message type shares one connection.

## Reasoning

- BRIEF.md 5.2 keeps the hot path binary and asks for forward compatibility:
  "unknown message types are skipped, not fatal". A type byte followed by a
  length is the smallest framing that lets a decoder skip a message it has
  never heard of without parsing a byte of its payload, because the length
  tells it exactly where the next frame starts.
- The same length prefix is what lets a decoder reject a malformed frame
  without reading past the end of its buffer (see 0005).
- Big-endian for all header and message fields: it is the network byte order
  every reference document (RFC 5905 included) is written in, and it removes
  an entire class of Rust-versus-C disagreement from the golden vectors. The
  PCM sample bytes inside an audio chunk are the exception and are carried in
  the byte order the announced sample format names, because they pass through
  untouched to the DAC.
- A u16 length caps a frame at 65535 bytes. A 20 ms chunk of 48 kHz 16-bit
  stereo is 3840 bytes and 24-bit 5.1 at the same chunk length is 17280, so
  the cap has better than 3x headroom over the largest chunk BRIEF.md 5.2
  contemplates while keeping the header at 3 bytes on a microcontroller.
- Monotonic clocks only, per BRIEF.md guardrail 4: every timestamp field in
  this protocol is nanoseconds from a monotonic source, never wall clock. The
  epoch is per-device and meaningless across devices, which is exactly why the
  time-sync exchange exists.
- One multiplexed connection per device is BRIEF.md 5.2's own recommendation
  because it "keeps embedded firmware simple". Nothing in this phase opens a
  socket, but the framing has to be chosen consistently with the connection
  model it will run on, so the model is recorded here with the framing.

## What is deliberately left open

**Port numbers.** BRIEF.md 5.2 lists them as open and attaches no rationale to
any particular number. Nothing in this phase binds a socket, so choosing one
now would be a default with no merits behind it. It stays open for the phase
that first listens.

Also still open, unchanged from BRIEF.md 5.2: chunk and buffer sizes, whether
FLAC ever enters for Wi-Fi zones, and single versus dual connection per device
(the recommendation above is adopted, but nothing has measured the
alternative).

## Message catalog at this phase

| type | name | payload |
|---|---|---|
| 0x01 | time sync | four RFC 5905 section 8 timestamps, 32 bytes fixed |
| 0x02 | audio chunk | 32-byte chunk header, then PCM sample bytes |

0x00 is never assigned; it is the value a zeroed buffer produces, so leaving it
unassigned makes an all-zero frame an unknown type rather than a valid message.
0x03 through 0xFF are unassigned and are skipped by a decoder that meets them,
which is the forward-compatibility path BRIEF.md 5.2 asks for.

The session hello/capabilities exchange, the stream format announcement, the
control messages and the client telemetry that BRIEF.md 5.2 also lists are not
in this phase's catalog: nothing in FOUNDATION-1 opens a connection to say
hello on, and inventing their fields with no consumer would freeze guesses into
golden vectors. They arrive with SOUND-2 and the control plane, as new type
bytes that an existing decoder already knows to skip.

## Consequences

- A decoder can always find the next frame boundary from the header alone,
  which is what makes "skip, do not fail" implementable.
- Any message type added later is invisible to older peers rather than fatal.
- The maximum frame is bounded, so a receiver can size buffers statically,
  which matters on the ESP32-S3.

## Revisit when

A phase binds a socket (port numbers), or a message needs to exceed 65535
bytes, which would mean the chunk size decision (BRIEF.md section 12 decision
4) has moved a long way from the 20 ms starting point.
