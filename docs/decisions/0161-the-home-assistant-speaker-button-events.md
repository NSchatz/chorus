# 0161: each button of a speaker that declares the controller role is a Home Assistant event entity, fired once per `controller_event` and never on a reconnect, with `press` and `long_press` as its event types and unavailable while the stream of presses is not attached

- Status: accepted, 2026-10-04. Extends 0138 (the Home Assistant integration) and 0154 (the
  speaker devices); uses 0151 (controller events on the HTTP control plane) as it is.
- Decided by: the owner for what there is (K83: button presses as event entities; K65: the
  controller role); this record for which speakers get entities, how a command becomes a
  button and an event type, what a lost stream does, and the level of the log line.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/` (`event.py`,
  `coordinator.py`, `__init__.py`, `_aiochorus/client.py`, `_aiochorus/models.py`,
  `strings.json`, `icons.json`, `quality_scale.yaml`),
  `integrations/homeassistant/tests/test_event.py`,
  `tests/aiochorus/test_controller_events.py`, `tests/fake_server.py`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

The server sends one `controller_event` on `GET /api/controller-events` for every controller
command it accepted, to whoever is subscribed, and keeps none (0151,
`docs/control-plane.md`). The message names the endpoint, the room, the command, its value,
a join's target and the outcome. It does not name a button: a button is the firmware's, and
the wire carries what the button asked for. No server change was needed and none is made.

## What was read

Read 2026-10-04: `docs/control-plane.md` ("How the messages travel", the
`controller_event` table), `docs/decisions/0151-controller-events-on-the-http-control-plane.md`,
the shared vectors `fixtures/control/v2/controller_event.json` and
`controller_event-transport.json` with their `.fields`, `crates/server/src/controller.rs`
(what each command becomes), `firmware/include/chorus/controls.h` and
`firmware/src/controls.c` (which class has which buttons and what each sends),
`docs/protocol.md` (the roles), and the state's `speakers[]` (`id`, `roles`).

## Decision

**Which speakers.** A speaker whose `speakers[].roles` contains `controller`. The state
carries no speaker class, and the role is what a speaker with buttons declares; a two-way
speaker and a subwoofer have a pairing button only and send no controller command, so they
get no entity. `roles` are the latest hello's and empty while the speaker is away, so the
entities are added the first time the role is seen and removed only when the speaker is
forgotten (with its device).

**Five entities, by button.** Play/pause, volume up, volume down, next, previous: the
`BUTTONS` of the compact speaker and the streaming amp. Device class `button`.

**The command names the button and the event type.**

| `command` | entity | event type | why |
|---|---|---|---|
| `toggle` | Play/pause button | `press` | play/pause released before the long-press mark |
| `join`, `leave` | Play/pause button | `long_press` | play/pause held: leave when grouped, else join the configured target |
| `volume_step`, value above zero | Volume up button | `press` | also each repeat while held |
| `volume_step`, value below zero | Volume down button | `press` | also each repeat while held |
| `next` | Next button | `press` | |
| `previous` | Previous button | `press` | |

The event's attributes are the message's other members (`command`, `value`, `target`,
`room`, `outcome`), so an automation that cares whether a long press was a join or a leave
can read it.

**One event per message, to one entity.** The coordinator holds one listener per (speaker,
button) and hands a message to that one. The speaker is the message's `endpoint`, which is
the adopted speaker's id.

**A reconnect fires nothing.** The server's stream opens with a comment and no message, and
the reader keeps nothing between connections. The entity fires only from a message.

**Unavailable while presses cannot arrive.** With the stream of presses detached (lost, or a
server older than the route, which answers 404) the button entities are unavailable: an
entity that looked available while every press was being missed would be a quiet lie. One
info log line says the presses are unavailable and one that they are back. The state stream,
and every other entity, is unaffected.

**What is ignored, and how loudly.** A command no speaker button sends (`play`, `pause`,
`volume_set`, `mute_set`, a `volume_step` of zero, a name this client does not know) and an
endpoint with no enabled button entity are dropped with one debug log line per event. Debug
and not warning: a wall remote that is not an adopted speaker is a legitimate source that
repeats, and a warning per press would flood the log.

**The reader is the state stream's.** `_aiochorus/client.py` has one subscriber class for
both routes: the line splitter and its bound, the liveness check with `GET /api/state` after
45 s of silence, and the backoff with jitter. The only difference is when the backoff starts
over: at the first state for the state stream, at the open for the stream of presses, which
is silent most of the time.

## Not chosen

- **Entities for every adopted speaker.** Five dead entities on every two-way speaker and
  subwoofer.
- **One entity per speaker with the command as the event type.** K83 and the task ask for
  one per button, and a dashboard reads "Volume up button" better than a list of commands.
- **Event types named for the command** (`toggle`, `join`, `leave`). They would name what
  chorus did, not what the person did; the command is in the attributes for those who want
  it.
- **Device triggers.** Out of the task's scope; the entity's state is the trigger.
- **Keeping the button entities available when the stream of presses is lost.** See above.
- **Replaying anything after a reconnect.** The server keeps no press by decision (0151); a
  client that invented one would be wrong about when it happened.

## Consequences

- A press made while Home Assistant is not connected to the server is lost.
- An automation on an event entity's state should use `not_from: unavailable`: the entity
  shows its last event's time again when it comes back, which is not a press. The README's
  example does.
- A wall remote that is not an adopted speaker has no entities. If one arrives as the same
  event from an adopted endpoint with the controller role, it gets these five.
- A new button on a later speaker class needs a new command mapping here; until then its
  presses are ignored with the debug line.
