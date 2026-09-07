# 0016: the control catalog, its version, and the volume range and curve

- Status: decided
- Recorded by: PRODUCT-6 (spec S0043-chorus-product-6)
- Specified in: `docs/control-plane.md`, which is the contract; the bytes are
  pinned by `fixtures/control/` and asserted by
  `crates/control/tests/catalog_vectors.rs`.

## Decision

**One catalog, versioned as a whole, at version 1.** Every message carries `v`
and every peer announces a version. A peer that announces a version this build
does not implement is refused the SESSION, told which version it offered and
which this build has, and nothing it sent is applied - including the rest of the
message the version arrived on.

**Volume is a decimal with exactly three fractional digits, from `0.000` to
`1.000` inclusive, and it is the AMPLITUDE FACTOR.** There is no perceptual
curve on the wire. `0.500` means every sample is multiplied by one half.

**The control plane is opt-in, at `--control-listen`.** There is no default
address and a server started without that flag has no control plane, no control
threads and no state file.

**The JSON codec is written in this repository**, in `crates/control/src/json.rs`.

## Reasoning

### Why the version refuses the session rather than skipping

`docs/protocol.md` has the audio decoder skip an unassigned type byte and accept
a payload longer than the fields it knows about, and
`docs/decisions/0003-wire-protocol-framing.md` records why: BRIEF.md 5.2 asks
for forward compatibility on the hot path, and a decoder that can step over a
message it has never heard of is what makes adding an audio type a non-event.

The control catalog takes the opposite decision, on purpose. The costs are not
comparable. An audio frame a decoder skips costs 20 ms of one stream on one
endpoint. A control message a decoder half-understands changes what a house is
doing: a `volume` message from a version that added a `ramp` field, read by a
build that does not know about ramps, is a volume change applied instantly in
every room at three in the morning. So a field this build does not know is a
message from a peer this build cannot serve, and the version field is where that
is said, once, at the top.

The refusal names both sides because that is the only thing that makes it
actionable: an operator with two versions on one network needs to know which
build to move.

### Why the volume is a decimal and not a float

Three reasons, in order of how much they cost.

1. **A golden vector needs one spelling.** `fixtures/control/volume.json` pins
   the bytes of a volume message. A binary floating-point value has several
   shortest decimal renderings depending on the printer, and two languages'
   printers do not agree about all of them. A fixed-point decimal with a
   declared number of places has exactly one.
2. **A value that comes back different from the value that went in is the thing
   a vector exists to catch.** `0.5001` is refused rather than rounded to
   `0.500`, so a client that sent something this catalog cannot represent learns
   that rather than being silently corrected.
3. **Applying it needs no floating point at all** for the two integer sample
   formats: an integer multiply and an integer divide, with a truncation toward
   zero that bounds the error at one unit in the last place, in the direction of
   silence. `crates/client-linux/src/zone.rs` is where that happens and
   `crates/client-linux/tests/zone_apply.rs` asserts the bound.

Thousandths, rather than hundredths or ten-thousandths: 1000 steps over the
range is finer than any slider a person operates and finer than the smallest
step that is audible at any point on the range, and it fits a `u32` with room to
multiply an `i16` sample without overflowing an `i64`.

### Why the curve is not on the wire

Loudness is not linear in amplitude, and a slider that moved amplitude linearly
would feel wrong in the top half of its travel. That is a real fact and it is a
UI fact, so the mapping from slider position to amplitude belongs in the UI.

Putting it on the wire would make the criterion ungradeable. PRODUCT-6's AC-1
says "after a volume change the accepted PCM is scaled by the commanded factor
within a stated tolerance", and that sentence has a meaning only if the
commanded value IS the factor. With a curve on the wire, the criterion would
have to be "scaled by some function of the commanded factor", and the function
would be a second thing to agree about between the server, two endpoint
implementations and every fixture.

So: the wire carries what the samples are multiplied by, the UI carries the
mapping a person feels, and a second implementation is held to the first without
having to reproduce the second. The shipped page currently uses the identity
mapping and shows the amplitude as a percentage; changing that is a UI change
and needs no catalog version.

### Why the control plane is opt-in

A default port would be bound by every one of this repository's own verification
runs at once - `tools/refusals.sh` alone starts several servers, and
`cargo test` runs several test binaries in parallel - and the second one would
fail to bind. An ephemeral default would avoid that and would give an operator a
port that changes on every restart, which is worse than a flag.

There is a second reason and it is a scope one: `deploy/run-server.sh` and
`deploy/Dockerfile` are outside PRODUCT-6's scope, so a default-on control plane
would change what the deployed container does without the file that starts it
saying so. An opt-in flag leaves that decision to whoever next edits `deploy/`,
with this document to read first.

### Why the JSON codec is written here

`docs/decisions/0002-repository-layout-and-ci.md` records that this workspace
has no third-party dependency and every `[dependencies]` entry is a path to
another crate in the tree. That would be reason enough.

The reason that would apply anyway is the vectors. A golden vector needs an
encoder whose key order, number spelling and escaping are DECIDED, and a
serialiser that derives those from a struct's field order decides them somewhere
else - in a crate's documentation, or in whichever version of it is resolved.
`docs/control-plane.md` states each rule and `crates/control/src/json.rs`
implements exactly them, so the bytes and the prose cannot drift.

The codec is about four hundred lines including its tests, it reads one value
and writes one value, and it refuses three things RFC 8259 permits - a duplicate
key, nesting past 32 levels, and content after the value - each because the
alternative is a guess about a message that changes what a house is doing.

## Alternatives rejected

- **Per-message versioning.** A `v` on each message that could differ between
  messages in one session. Rejected: a session where half the messages are
  version 1 and half are version 2 has no answer to "what does this peer
  support", and the refusal has nowhere to go.
- **A binary control catalog, sharing the audio framing.** Rejected: the control
  plane is edited by people, read out of a browser's developer tools and
  debugged with `curl`, and none of that is true of the audio wire. It also
  would have put a control decoder on the ESP32-S3's audio path, which is the
  one place this project has been careful to keep small.
- **Volume as an integer 0 to 100.** Rejected as too coarse at the quiet end,
  where one step is a large fraction of what is left.
- **Volume in decibels.** Rejected: it needs a floor for silence, which is a
  second constant to agree about, and `-inf` has no spelling in JSON.
