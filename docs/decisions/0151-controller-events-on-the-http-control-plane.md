# 0151: a speaker's button press reaches HTTP subscribers as a `controller_event` on its own server-sent event route, fed by a second bounded fanout and held by the one event writer, with nothing replayed

- Status: accepted, 2026-10-04. Extends 0026 (HTTP and server-sent events), 0017 (the control
  fanout's bound) and 0077 (the one event writer); 0116 (the MQTT publisher) is unchanged.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/control/src/catalog.rs` (`ControllerEvent`),
  `crates/server/src/control.rs` (`ControlState::presses`, `ControlState::controller`,
  `serve_controller_events`, the report line), `crates/server/src/events.rs` (documentation
  only: the writer is unchanged), `fixtures/control/v2/controller_event.{fields,json}`,
  `fixtures/control/v2/controller_event-transport.{fields,json}`,
  `crates/control/tests/catalog_v2.rs`, `crates/server/tests/controller_events.rs`,
  `crates/server/tests/control_thread_population.rs`, `docs/control-plane.md`

## Context

A controller command the server accepts (a button press on a speaker, `docs/protocol.md`,
"0x32 controller command") left the server in one way only: the opt-in MQTT publisher's
`<prefix>/speakers/<endpoint id>/event` topic (0116). A client of the HTTP control plane could
see what a press changed, in the next state message, and could not see the press: not which
speaker's button it was, and nothing at all for a transport command, which changes no room.
An automation that reacts to a button needs the press, and needs it without a broker.

Two properties were fixed before the transport was chosen. A press is not a state change, so
it must not ride in the state message: a state is a complete snapshot of what is true now, a
press is something that happened once, and a subscriber that reads "the latest state" must
never act on a press again because it read the state again. And a subscriber that connects
later is never replayed a press, which is the MQTT event's rule too (not retained).

## What was read

All on 2026-10-04. No GPL source was opened; nothing outside this repository was read.

- chorus's own: 0017, 0026, 0063 (the controller role), 0077, 0116; `docs/control-plane.md`
  ("How the messages travel", "Event streams and the event writer", "The bound on a
  subscriber", "The thread population"); `docs/mqtt.md` ("The topics");
  `crates/server/src/{control,controller,events,mqtt}.rs`, `crates/control/src/fanout.rs`,
  `crates/mqtt/src/payload.rs`, and the tests `controller_role.rs`,
  `control_thread_population.rs`, `control_stalled_peer.rs`, `mqtt_publisher.rs`.
- The server-sent events format (the `data:` field, comment lines, named events through the
  `event:` field) is the WHATWG HTML standard's "Server-sent events" section, as 0026 chose
  it; it was not read again for this record, and nothing here rests on more of it than the
  running server already uses.

## Decision

**The transport is a second server-sent event route, `GET /api/controller-events`.** The same
protocol as `GET /api/events`, so a browser opens it with `EventSource` and a script with a
socket and a `GET` line, and the test's subscriber is the browser's (0026's rule). Each
accepted controller command is one `data:` line holding one message.

**The message is a catalog message, `controller_event`, at catalog version 2.**
`{"v":2,"t":"controller_event","endpoint":...,"zone":...,"command":...,"value":...,"target":...,"outcome":...}`:
the catalog's `v` and `t`, then the MQTT event's six members in its order with its values.
The server sends it and nothing decodes it; `POST /api/command` refuses it as it refuses any
type it does not know. Two vectors pin its bytes. The MQTT payload is not touched: it keeps
its bytes, without `v` and `t`, because a topic already says what its payload is and 0116
settled those bytes.

**Nothing opens the stream and nothing is replayed.** The response is the headers and one
comment line. There is no "press as it stands" to open with, and a press accepted while
nobody is attached is built, offered to an empty fanout and gone. Nothing is stored, so
nothing can be replayed by mistake.

**The fanout is a second `ControlFanout`, with 0017's bound and 0017's drop rule.** At most
32 events wait for one subscriber; one that reaches that ceiling is dropped (its stream
closes) and counted. The subscriber is dropped and not the event, for the reason 0017 gives
for states made stronger: a state stream that skipped a message is repaired by the next
snapshot, an event stream that skipped a press is silently wrong for ever, so the only honest
outcomes are "every press since you attached" or "your stream ended". The fanout is one
non-blocking `try_send` per subscriber on the session's reader thread, so a slow subscriber
never delays a press or the endpoint's answer.

**The streams are held by the one event writer, under the one ceiling.** A worker writes the
headers and hands the socket over, exactly as for a state stream; the writer's peer is the
same structure fed by the other fanout. So the new subscriber kind costs no worker and no
thread, the declared population (`6 + 2N + M`) is unchanged, and `--event-streams` bounds the
two kinds together: it is a bound on sockets the writer holds, and a second ceiling would
make the total a sum nobody declared. The keepalive and the 5 s stall bound apply unchanged.

**The report counts them apart.** `GET /api/report` gains `press_subscribers`,
`press_dropped_subscribers` and `press_dropped_events`, under names of their own so that a
reader of the state fanout's `dropped_subscribers` and `dropped_messages`
(`tools/house-soak/report.py`) keeps reading the state fanout's.

## Not chosen

- **A field in the state message** (a `last_press`, or a list of recent presses). It makes a
  press a state: every later state would carry it again, a new subscriber's opening state
  would replay it, and a client would need a counter to tell a new press from an old one.
  The design note this task came with rules it out for that reason.
- **A named event on the existing `/api/events` stream** (`event: controller_event`, then
  `data:`). One socket instead of two, and a browser's `onmessage` would not see it. But every
  subscriber that reads that stream as lines (the Linux endpoint's control client, the Home
  Assistant client, the tests, shell scripts) takes each `data:` line as a state, and would
  have to learn the `event:` field at once or misread a press as a state. It would also put
  presses in the state subscriber's queue, so a burst of presses could drop a subscriber that
  only wanted states, and it would open the stream with a state the press subscriber did not
  ask for.
- **An opt-in on the same route** (`/api/events?controller=1`). The same mixing for the
  subscriber that opts in, and two payload kinds behind one queue and one drop count.
- **WebSocket.** 0026's reasons stand: nothing here needs the client to speak on the stream.
- **Long polling, or `GET /api/controller-events?since=N` over a kept log.** A kept log is a
  replay by construction and needs a retention rule; "no past press" is the requirement.
- **Requiring the MQTT publisher**, or routing the HTTP events through its queue. The HTTP
  control plane must not need a broker, and the publisher's queue has its own bound (64) and
  its own consumer.
- **Dropping the event and keeping the subscriber** (the audio fanout's rule). Above: a
  silent gap in an event stream cannot be detected by its reader.
- **A separate ceiling for these streams.** Above.
- **Reusing the MQTT payload's bytes unchanged.** Every message on this control plane carries
  `v` and `t`, and a subscriber that reads one stream's lines should be able to tell what it
  was sent without knowing which route it opened.

## Consequences

- An HTTP client that wants both states and presses opens two streams, and each counts
  against `--event-streams` (default 64).
- The order between a press's event and the state it changed is the order they are fanned
  out in (the event first), on two sockets: a client must not rely on reading them in that
  order.
- `?v=1` on the new route changes nothing: the message exists at catalog version 2 only.
- The Home Assistant client and the control page do not open the route yet; those are later
  changes.
- There is still no authentication on the control plane (K40), so anything that can reach
  the listener can watch presses, as it can already watch states.
