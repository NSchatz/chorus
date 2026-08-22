# 0005: what a decoder does with a frame it cannot accept

- Status: decided
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)

## Decision

A decoder never fails a session because of one frame. It classifies every frame
into exactly one of three outcomes and keeps going:

| outcome | when | does it find the next frame? |
|---|---|---|
| decoded | known type, length consistent, every field valid | yes |
| skipped | type byte is not in the catalog | yes, the length prefix says where |
| rejected | anything else below | sometimes, see the table under "resync" |

The checks run in this order, and the order is part of the contract because it
decides which error a frame gets:

1. **Fewer than 3 bytes left.** There is not a whole frame header. Rejected as
   a truncated header. The decoder cannot know where the next frame is.
2. **Declared payload length exceeds the bytes actually available.** Rejected.
   This check exists before anything touches the payload, and it is the one
   that makes "shall not read past the end of the buffer" true. See below.
3. **Type byte not in the catalog.** Skipped, not rejected, and the length
   prefix is used to step over the payload. Forward compatibility per
   BRIEF.md 5.2: unknown types are skipped, not fatal.
4. **Declared length below the minimum the declared type requires.** Rejected.
   The frame is well formed as a frame, so the length prefix still says where
   the next one starts and decoding continues after it.
5. **A field value the format cannot accept.** Rejected, same resync as 4.

## Why the length field is checked against the buffer, not only the minimum

These are two different failures and only one of them is a per-type constant.

A frame can satisfy every fixed minimum and still lie: type 0x01, a length
field declaring 1000 bytes of payload, and 4 bytes actually in the buffer. A
decoder that only checks "total bytes >= this type's minimum" passes that gate
and then slices `payload[0..1000]` out of a 4-byte tail. In Rust that is an
out-of-bounds slice, which panics, which ends the process. "Reject only that
frame and keep the session open" is then false in the most complete way
possible, and on a protocol whose length field is controlled by whatever is on
the other end of a socket.

So the declared length is validated against the actual remaining buffer before
any payload is sliced, and that check comes first. It is not a special case of
the minimum-length check and it is tested separately.

## Resync after a rejection

| rejection | can the decoder continue in the same buffer? |
|---|---|
| truncated header (case 1) | no |
| declared length exceeds buffer (case 2) | no |
| payload shorter than the type minimum (case 4) | yes |
| invalid field value (case 5) | yes |

Cases 1 and 2 are exactly the cases where the frame boundary is unknowable from
what is present, so the decoder stops consuming that buffer instead of guessing
where the next frame might start. Guessing is how a decoder turns one corrupt
frame into a stream of nonsense.

**The session stays open in every one of these cases.** Stopping on a buffer is
not closing a session: the next buffer is decoded normally. Nothing in this
phase closes a session except the caller.

## What "session" means here

This phase has no socket (it is explicitly out of scope), so a session is not a
connection. It is a stateful decoder instance that outlives any single buffer
and counts what it has seen: `Session` in `crates/protocol/src/session.rs`. It
stays open across skipped and rejected frames by construction, and it is the
object the "keep the session open" criteria are asserted against. When a
transport arrives in a later phase, that transport owns one of these per peer,
and closing the socket is its decision, not the decoder's.

## Invalid field values, specifically

A frame can be exactly the right length for its type and still carry a value
the format cannot accept. This is the case that only ever arrives from a
non-conforming peer, from corruption, or from the second-language
implementation that the golden vectors exist to hold to bytes, which is why it
is pinned down now rather than after EMBEDDED-5 starts. The audio chunk header
has four such cases:

- `sample_format` is not 1, 2 or 3. The value is undefined, so the decoder
  cannot know how wide a sample is or how to hand the payload to a DAC.
- `channels` is 0, or above 8. Zero channels cannot describe any audio;
  above 8 is beyond what any endpoint in this system carries.
- `sample_rate_hz` is outside 8000 to 384000. Outside that band the value is
  not a plausible rate, and a playout servo that trusted it would compute a
  chunk duration that is wrong by orders of magnitude.
- The PCM byte count is not a whole number of sample frames
  (`channels * bytes per sample`). A partial frame would split a sample across
  a chunk boundary, and no sender that agrees with the header can produce one.

All four are rejected per frame, with the session open and decoding continuing
after the frame. None of them closes anything, and none of them is treated as
"skip like an unknown type": an unknown type is expected traffic from a newer
peer, while an invalid value is a peer that disagrees with the format, and the
two deserve different counters.

A **zero-length audio payload** is handled one step earlier, by the length
rule rather than the value rule: the audio chunk type's minimum payload is the
32-byte chunk header plus at least one byte, so a chunk carrying no samples is
rejected at check 4 as too short for its type. That is deliberate. Making "no
samples" unrepresentable in the length rule is stronger than validating it as a
value, because it holds even for a decoder that skips value validation.

The reserved block (see 0004) is explicitly **not** validated. Any value is
accepted and preserved.

## Encoder side

The encoder rejects rather than emits when it is handed something the wire
cannot carry, so a caller can never produce bytes its own decoder would refuse:

- A value that does not fit its wire field, for example more than 255 channels
  in a `u8`, is refused as not representable. It is never truncated and never
  allowed to wrap.
- A payload longer than 65535 bytes is refused, because the u16 length field
  cannot describe it.
- Every value rule the decoder enforces is enforced at encode time too.

`encode` returns an error and emits nothing at all in these cases. A partial
frame is never written.

## Consequences

- One malformed frame costs one frame.
- A decoder cannot be made to panic by a hostile or broken length field, which
  matters because this is the code that will face the network in SOUND-2.
- The C mirror in EMBEDDED-5 has an explicit, ordered checklist to implement
  rather than prose to interpret.

## Revisit when

A transport lands and needs a policy for how many rejected frames in a row
justify dropping the connection. That is a transport decision, and the counters
on `Session` exist so it can be made on evidence.
