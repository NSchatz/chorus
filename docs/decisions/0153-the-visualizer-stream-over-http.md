# 0153: a room's visualizer stream is read by an HTTP subscriber on its own server-sent event route, one latest frame per room superseded and never queued, at most one frame every 100 ms, each carrying the time until the room hears it

- Status: accepted, 2026-10-04. Extends 0084 (the visualizer stream), 0026 (HTTP and
  server-sent events), 0077 (the one event writer) and 0151 (a second subscriber kind on the
  writer); 0017 (the control fanout's bound) is unchanged and deliberately not applied here.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/lights.rs` (`LightTap`, `LightSubscription`,
  `MIN_FRAME_INTERVAL`, the encoding), `crates/server/src/router.rs` (`Router::watched`,
  `Router::lights`), `crates/server/src/slots.rs` (the push, on the audio thread),
  `crates/server/src/conductor.rs` (the placing of subscribed rooms),
  `crates/server/src/events.rs` (the third feed), `crates/server/src/control.rs`
  (`serve_visualizer`, the report line), `crates/server/src/main.rs`,
  `fixtures/visualizer/http-frame.{fields,json,sse}`,
  `crates/server/tests/visualizer_stream.rs`, `docs/visualizer.md`, `docs/control-plane.md`

## Context

The visualizer stream (0084) reached one kind of receiver: an audio-wire session that
declared the `visualizer` role. A light that follows the music through Home Assistant (K65)
is not one. It has no endpoint identity, no session and no clock on the server timeline, and
it cannot take 25 frames a second: the smart lights' command rates the research found are
about 10 a second. `docs/visualizer.md` listed it as a follow-up. What it needs is a room's
colour, level and beat from the HTTP control plane, at a bounded rate, with a rule for the
frames it cannot be sent.

## What was read

All on 2026-10-04. No GPL source was opened; nothing outside this repository was read.

- chorus's own: 0017, 0026, 0077, 0084, 0151; `docs/visualizer.md`; `docs/control-plane.md`
  ("How the messages travel", "Event streams and the event writer", "The bound on a
  subscriber", "The thread population"); `docs/research/research-dsp-phase-b.md` section 2
  (the smart lights' rates: Nanoleaf external control "no faster than 10Hz", Hue's effect
  rate under 12.5 Hz, both LEADs, read 2026-10-01 for 0084 and not fetched again);
  `crates/server/src/{router,slots,conductor,events,control}.rs`,
  `crates/dsp/src/visualizer.rs`, and the tests `visualizer_stream.rs`,
  `controller_events.rs`, `control_thread_population.rs`.

## Decision

**The transport is a third server-sent event route, `GET /api/visualizer?zone=<room id>`.**
The subscriber protocol 0026 chose and 0151 reused, so the test's subscriber is a script's
and a browser's. One `data:` line per frame; the stream opens with a comment line and no
frame.

**The stream is a room's.** A light is in a room, the state a subscriber already reads is
keyed by room, and a frame's stamp depends on the room's tier. A group's stream is read
through any of its rooms (every room of a group is on the group's slot). The conductor says,
on every pass, which slot each subscribed room is on and after what latency it hears it, so
a subscription follows its room through grouping. A room on no slot (source `none`, or the
one-stream shape, which computes no visualizer stream) is sent nothing.

**The message is `visualizer`, at catalog version 2, built in the server crate.**
`{"v":2,"t":"visualizer","zone":...,"timestamp_ns":...,"lead_ms":...,"peak":...,"beat":...,"red":...,"green":...,"blue":...,"brightness":...,"transition_ms":...}`:
the wire's `visualizer_frame` without bands and the wire's `color`, in one message, because
a lamp has one colour and one level and the two wire messages only make sense together to a
receiver that kept the last colour. Three fixture files pin one frame's bytes.

**One latest frame per room, superseded and never queued.** The audio thread overwrites the
room's frame; a subscriber is sent whichever is there when it may next be sent one. This is
the audio fanout's rule (drop the item, keep the subscriber) and not 0017's (drop the
subscriber): a state or a press that is skipped leaves the subscriber silently wrong, a
frame that is skipped is repaired by the next one 40 ms later. It also means there is no
per-subscriber queue at all, so the bound is one frame per room however many subscribers
stall. The beat and the colour are carried into the next frame sent: a beat is an event
inside a stream of levels, and two frames in three are superseded under the cap, so a beat
that lived only in its own frame would be lost two times in three.

**At most one frame every 100 ms to one subscriber, enforced by the event writer.** Cited,
not measured: 10 a second is inside both of the research's light limits. The interval is
per subscriber, on the writer's monotonic clock, and the writer wakes itself when it runs
out, so a held frame waits for the cap and never for the next frame to arrive.

**Each frame says how long until it is heard (`lead_ms`).** The wire stamps a frame on the
server timeline and an endpoint has that timeline. An HTTP subscriber does not, so the
writer subtracts the timeline's now at the moment it renders the frame. The stamp is kept
beside it for a reader that does have the timeline. The end-to-end test measures the lead
against arrival times on its own clock (one machine, loopback); nothing is claimed about a
lamp.

**The streams are held by the one event writer, under the one ceiling.** No worker and no
thread: the population is unchanged. The audio thread's part is one atomic read per slot
per tick when nobody subscribes, and with a subscriber a short lock, a few stores and one
non-blocking wake per frame, with no allocation.

## Not chosen

- **A fourth `ControlFanout` of frames, with 0017's bound.** 25 messages a second into a
  queue of 32 drops a subscriber that pauses for 1.3 s, and a queue delivers old frames
  first: a slow subscriber would show the past. The goal says superseded, never queued.
- **Fields in the state message.** A frame is not a state change; 25 state messages a
  second to every state subscriber would be the cost of one light.
- **Decimating on the audio thread (every third frame to everyone).** One rate for all
  subscribers and no "latest when you are ready" for a slow one; and the cap would be a
  multiple of the frame interval (120 ms) and not the cited number.
- **Letting the subscriber name its rate (`?hz=`).** No reader needs it yet; the cap is
  the lights' own limit, and a slower reader simply reads less often and gets the latest.
- **A group id in the query.** A group's frames are its rooms' frames; the stamp is a
  room's. Nothing a group id would add.
- **MQTT.** The publisher is opt-in and the integration reads HTTP; 10 messages a second
  per room through a broker is also not what its retained topics are for.

## Consequences

- `GET /api/report` gains `light_subscribers`, `light_frames` and `light_superseded`, after
  the controller-event counters.
- A visualizer subscriber counts against `--event-streams` with the other two kinds.
- A slot is analysed while a subscribed room is on it, visualizer session or not.
- The Home Assistant integration's light entity is its own task.
