# 0017: the control subscriber queue ceiling, and dropping the subscriber

- Status: decided
- Recorded by: PRODUCT-6 (spec S0043-chorus-product-6)
- Implemented in: `crates/control/src/fanout.rs`; asserted by
  `crates/control/tests/slow_subscriber.rs` and reported by
  `GET /api/report`.

## Decision

**`CONTROL_QUEUE_LIMIT = 32` state messages per control subscriber.**

**At the ceiling the SUBSCRIBER is dropped**, not the message. What it never
received is counted, and both the count of dropped subscribers and the count of
messages they lost are reported.

## Reasoning

### Why there is a ceiling at all

The same argument `crates/server/src/stream.rs` makes about the audio fanout:
one subscriber that has stopped reading must not delay another's fanout, so
nothing blocks the thing producing. That decoupling needs its other half or it
is not decoupling, it is an unbounded allocation with a stalled socket on the
end of it. A browser tab left open on a laptop that was then suspended is
exactly that, and it is not a contrived case.

### Why 32

A state message is a complete snapshot. For a house of a dozen zones it is a few
kilobytes, so 32 of them is on the order of a hundred kilobytes per stalled
subscriber - small enough that the ceiling is not itself a limit on how many
browsers may be open, and large enough that it is never reached by anything that
is reading.

What decides the number is what the queue is FOR. It is not a buffer against a
slow reader in any useful sense, because every message in it supersedes the one
before: a subscriber that reads message 32 has been shown thirty-one states that
are no longer true. So the queue only needs to be deep enough to absorb a burst
of commands while a reader is momentarily descheduled. Thirty-two is more than a
person can generate from a UI in the time a socket takes to drain.

Four would have been enough for that and would have made a busy operator's own
tab liable to be dropped during a burst. Four thousand would be an allocation
nobody bounded, which is the thing this exists to prevent.

### Why the SUBSCRIBER is dropped and not the message

This is the one place the control fanout deliberately differs from the audio
one, and the difference looks like an inconsistency until the reason is written
down.

`Fanout::broadcast` in `crates/server/src/stream.rs` drops the ITEM and keeps
the subscriber, because an endpoint that stopped draining audio for a moment can
catch up on the next chunk and dropping it would end its playback. The
consequence is a gap in the sequence it receives, which is recoverable.

A control subscriber 32 whole snapshots behind is not behind, it is gone. Every
one of those snapshots superseded the one before it, so what it would eventually
read is a history nobody wants; meanwhile it is holding one of a fixed number of
worker threads that a browser which IS reading could have had. Dropping it
closes the socket, which is what a browser needs in order to reconnect: an
`EventSource` whose connection closes opens another by itself and is then sent
the state as it stands, which is the state it wanted all along.

Keeping it and dropping messages would have produced a subscriber holding a
state that is neither current nor identifiably stale, with no event that tells
it so. That is the worst of the three outcomes.

### Why the count is reported and not only kept

"Count what it dropped, report the count" is the criterion's own wording, and
the reason it says both is that a count nobody can read is not a report. The
count is on the server's status line at end of stream, and it is available while
the process is running at `GET /api/report`, because an operator debugging a UI
that keeps going blank needs it then rather than afterwards.

## What this cannot reach

Nothing in this fanout can delay or reorder the audio path. It holds no lock the
audio path takes, allocates nothing the audio path allocates, and the threads
that drain it are not the threads that carry audio.
`crates/control/tests/slow_subscriber.rs` drives a real
`chorus_server::stream::Fanout` beside a control fanout whose subscriber has
stalled and asserts the audio subscriber's sequence for both delay and order,
because that is a claim about two things running together and not about either
of them alone.
