# 0004: the audio chunk header reserves 14 opaque bytes for the TV path

- Status: decided
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)
- Answers the spec clause "an audio chunk header laid out now (reserving the
  fields the low-latency mode TV-9 will need) so it does not require an OTA of
  every device to retrofit later"

## Decision

The audio chunk header is **32 bytes**, of which **14 bytes at offset 18 are
reserved**: opaque, no semantics assigned by this phase.

```
offset  0..4    sequence          u32 big-endian
offset  4..12   timestamp_ns      u64 big-endian, monotonic server timeline
offset 12..16   sample_rate_hz    u32 big-endian
offset 16       channels          u8
offset 17       sample_format     u8 enum
offset 18..32   reserved          14 bytes, opaque
offset 32..     PCM sample bytes
```

Rules for the reserved block, which are the part that has to hold:

- An encoder writes zeros there and rejects nothing on their account.
- A decoder **ignores their value entirely**. It does not require them to be
  zero and it does not reject a frame because they are not. A future sender
  that assigns meaning to some of them must not be fatal to a decoder built
  today; that is the whole reason the block exists.
- A decoder preserves the raw 14 bytes on the decoded message, so a value that
  arrives is observable and survives a round trip rather than being silently
  flattened to zero.

## Why opaque and not named placeholder fields

Two readings of "reserving the fields TV-9 will need" were available.

(a) Reserve a fixed block of opaque bytes with a recorded count and rationale.
(b) Lay out named, typed fields anticipating what the low-latency mode will
    actually signal.

**Reading (a) is what this phase implements.** Reading (b) needs facts nobody
has yet. What BRIEF.md actually says about that path is 5.7 ("design the chunk
header now so the same protocol family covers a 5 ms low-latency mode; plan on
UDP + simple XOR parity FEC, wired only, stereo first") and 5.2 ("UDP with
forward error correction, even simple XOR parity over small groups"). That is a
direction, not a field list: it does not say how many FEC groups, how a parity
frame is identified, how wide a group index is, or whether a low-latency chunk
carries its own presentation deadline. Naming fields from it would freeze
guesses into golden vectors that a C mirror is then held to byte for byte,
which is a worse outcome than reserving space, because a wrong named field
costs the OTA that the reservation exists to avoid.

The escape clause in the review that raised this was "unless a documented TV-9
requirement in the checkout justifies otherwise". The checkout was searched for
every mention of the TV path, low latency, lip sync, FEC and surround. They are
BRIEF.md sections 1, 2.1, 2.2, 4, 5.2, 5.7, 6, 8, 9 and 12, plus one line each
in README.md and CLAUDE.md. Every one of them is a target, a budget or a
direction; not one names a header field, a width or a count. So the escape
clause does not fire, and reading (a) stands.

## Why 14 bytes

The number is not a guess dressed as a count. Reading (a) still owes a
rationale for the size, and this is it:

- The five fields this phase actually needs occupy 18 bytes. Rounding the chunk
  header to 32 bytes leaves exactly 14.
- A 32-byte header keeps the PCM payload 32-byte aligned when the frame header
  is stripped, which matters on the ESP32-S3 where the DMA-visible buffers live
  in internal RAM and cache line handling is easier on aligned blocks. It also
  makes the header a clean fixed-size struct for the C mirror.
- 14 bytes is comfortably more than the direction in BRIEF.md 5.7 implies: an
  FEC group index and a position within the group, a low-latency mode marker,
  and a signed A/V trim would fit in about 6 bytes, so the reservation absorbs
  roughly double what is currently foreseeable without another decision.
- Growing the header later costs an OTA of every wall-mounted device. Shrinking
  it costs nothing but a decision-log entry. When the two errors are that
  asymmetric, over-reserving inside the next alignment boundary is the cheap
  side to be wrong on.

## Consequences

- The wire layout of an audio chunk is frozen for the C mirror in EMBEDDED-5,
  including the reserved block, which is part of the committed golden vector.
- TV-9 can assign meaning to bytes inside the block without a version bump and
  without any deployed decoder rejecting the frames, because ignoring the block
  is specified behaviour rather than an accident.
- If TV-9 needs more than 14 bytes, it introduces a new message type. Older
  decoders skip it (see 0003), which is still not an OTA.

## Revisit when

TV-9 assigns the first byte of the block. That is a decision-log entry of its
own: it turns opaque space into a named field and the golden vectors move with
it.
