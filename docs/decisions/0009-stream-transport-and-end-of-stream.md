# 0009: the transport, and telling a finished stream from a lost one

- Status: decided
- Recorded by: umbrella spec S0015-chorus-sound-2 (roadmap phase SOUND-2)
- Extends 0003 (framing) and 0005 (decoder validation); adds message type 0x03

## Decision

**TCP**, one connection per client. The end of a stream is announced by a new
message type, `0x03 stream_end`, sent **in band** after the final chunk and
before the close. A close without it means the other thing.

On a stream transport the client treats a frame it can only step over using an
uncorroborated length prefix as lost alignment, and closes the session with a
typed framing error rather than skipping it.

## Why TCP

0003 left the transport open and this phase had to pick one.

The case for UDP is real and it is about the future rather than this phase:
head-of-line blocking on a retransmitted chunk is worse for audio than losing
that chunk, and the low-latency TV path in a later phase will care. The case
for TCP is that this phase is one client on copper with no correction of any
kind, so there is no packet-loss recovery to design and nothing yet that a
retransmission delay would disturb; and that a stream transport makes the
end-of-stream question sharp, which turned out to be the useful part.

Deciding it now on ordinary grounds, and recording that the decision is
reopenable by the phase that has a reason, is better than leaving it implicit.
Nothing in the framing changes if the transport does: `docs/protocol.md`'s
frames are self-delimiting, so the same bytes work on a datagram.

## Why the end of a stream is a message

A transport close and a transport that broke look identical to the peer. Both
are a read returning zero. There is no timing that separates them, because the
same silence follows a tidy finish and a dead server.

That matters here because the two cases have opposite correct behaviours. A
stream that ended cleanly should play out what the client holds, count no
underruns for the drain, and exit zero. A server that vanished should play out
what the client holds, count no underruns for the drain, **say which happened**,
and exit non-zero. A client that guessed would either report every clean end as
a failure or report every failure as a clean end.

So the signal is data:

| offset | size | field |
|---|---|---|
| 0 | 4 | `final_sequence` |
| 4 | 8 | `end_timestamp_ns` |

`final_sequence` lets a receiver notice it is missing audio that was sent,
which is a different thing from the stream being over.

Adding it cost nothing that was already committed. Type 0x03 was unassigned;
`fixtures/protocol/time_sync.hex` and `fixtures/protocol/audio_chunk.hex` are
byte-identical to what FOUNDATION-1 committed; and the new type has its own
golden vector, as the suite requires of every catalogued type.

## Why the skip rule stops at the client

This is the one place where the client is deliberately stricter than
`docs/protocol.md`'s decoder, and it is worth being precise about why.

Rule 3 of the decoder behaviour skips an unassigned type using its length
prefix and carries on. That is right, and it is what makes adding type 0x03 a
non-event for every existing implementation including the future C mirror.

It assumes something, though, and the assumption is about the transport rather
than the protocol: that the length prefix is trustworthy. When a frame arrived
in a datagram whose boundaries the transport preserved, it is. On a byte
stream, if alignment has already been lost, the byte read as a "type" is a PCM
sample and the two bytes read as a "length" are two more, and stepping over
them is how a reader stays lost rather than how it recovers.

The Linux client is the component holding a DAC, so it refuses. The cost is
that a newer server's new message type ends this client's session instead of
being ignored. That is a real cost and it is the smaller one: the alternative
is putting mis-framed bytes on an audio device as sound, which is the failure
mode this whole framing exists to prevent.

A reader whose transport preserves message boundaries should still skip.
`docs/protocol.md` now records that both readings are conformant and that what
is **not** conformant is stepping over an uncorroborated length and then
playing what follows.

## The rest of the client's session grammar

Recorded here so the whole detection story is in one place. A chunk whose
declared length does not match the bytes delivered for it is, on a byte stream,
indistinguishable from a chunk whose payload happens to contain the next
frame's header. Nothing in that frame says otherwise, so detection has to come
from what the stream is supposed to look like:

- the shape is fixed by the first chunk, and a later chunk disagreeing about
  rate, channels or format is a framing error;
- only the last chunk may be short, so a chunk arriving after a short one means
  the short one was truncated;
- a frame the decoder rejected is one bad frame: discarded, counted, session
  open, alignment intact because the length came from a catalogued type;
- a sequence already accepted is a duplicate: discarded, counted separately,
  session open.

## Consequences

- `MessageType::ALL` has three entries and the golden-vector suite asserts it.
- A client and a server disagreeing about the stream's shape mid-stream is a
  hard stop rather than a glitch.
- Reconnection is not in this phase. A client that loses its server exits.

## Revisit when

The low-latency TV path needs a datagram transport, or a phase adds a message
type the client has to tolerate mid-stream, at which point the client learns it
rather than the rule loosening.
