# 0162: a room's visualizer stream is one Home Assistant sensor per room, disabled by default, whose state is the level and whose attributes are the colour and the beat as `light.turn_on` takes them, written at most five times a second, with no state class and no recorded attribute

- Status: accepted, 2026-10-04. Extends 0138 (the Home Assistant integration); uses 0153 (the
  visualizer stream over HTTP) as it is.
- Decided by: the owner for what there is (brief section 23 item 1: "the visualizer stream
  exposed so HA automations can map it to lights"; K65); this record for the platform, the
  shape of the state, the cap and its number, the idle value, what the recorder keeps, and
  when the stream is held.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/` (`visualizer.py`,
  `sensor.py`, `const.py`, `_aiochorus/client.py`, `_aiochorus/models.py`, `strings.json`,
  `icons.json`, `quality_scale.yaml`), `integrations/homeassistant/tests/test_visualizer.py`,
  `tests/aiochorus/test_visualizer.py`, `tests/fake_server.py`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

The server sends a room's visualizer frames to any subscriber of
`GET /api/visualizer?zone=<room>`: one `visualizer` message per frame, at most ten a second,
the latest only, each with the level (`peak`), the last beat not yet sent, the colour in
force and `lead_ms` (0153, `docs/visualizer.md`, "The HTTP stream"). A room that plays
silence is sent the first silent frame and then nothing; a room whose group has no source is
sent nothing. No server change was needed and none is made.

Home Assistant has no entity platform for "a colour that changes ten times a second". The
integration must not drive lights itself (out of scope, and the owner's mapping), so the
frames have to become something an automation can trigger on and read.

## What was read

Read 2026-10-04: `docs/visualizer.md` ("What a frame carries", "The colour", "When a frame is
heard", "The HTTP stream"), `docs/decisions/0153-the-visualizer-stream-over-http.md`,
`docs/control-plane.md` (the route), the shared vector `fixtures/visualizer/http-frame.json`
with its `.fields` and `.sse`, `crates/dsp/src/visualizer.rs` (`Frame::is_silent`) and
`crates/server/src/slots.rs` (a run of silent frames sends its first). Home Assistant core
2026.9.3 (Apache-2.0), from the installed package: `helpers/entity.py`
(`_unrecorded_attributes`, the combined set in the state's `state_info`),
`components/recorder/db_schema.py` (`MATCH_ALL` drops every attribute but the device class,
the state class, the unit and the friendly name), `components/sensor/recorder.py` (statistics
are compiled for sensors with a state class), `components/light/__init__.py` (`turn_on` with
a brightness of 0 turns the light off), `helpers/event.py` (`async_call_later` is the loop's
`call_at`), `components/homeassistant/triggers/state.py` (a state trigger with no `to`,
`from`, `not_to` or `not_from` fires on attribute changes too).

## Decision

**The platform is `sensor`: one per room, on the room's device.** A sensor is the platform
whose state an automation triggers on and whose attributes a template reads, with nothing
else implied: no command, no "on" and "off", no event semantics. Its state is the level, a
number a dashboard can show; its attributes carry the rest.

**The attributes are `light.turn_on`'s own.** `rgb_color` (three bytes), `brightness` (0 to
255) and `transition` (seconds) are named and scaled as the light action takes them, so the
documented automation passes them through with three templates and no arithmetic. `beat` (0,
or 1 to 255) and `lead_ms` are the frame's. `timestamp_ns` is not exposed: it is on the
server's timeline, which Home Assistant has no clock for.

**Disabled by default, and no stream while disabled.** Most rooms have no light that should
follow the music, and an enabled one costs the server an analysis and Home Assistant up to
five state writes a second. The stream is opened by the entity when Home Assistant adds it
and closed when it is removed; a disabled entity is never added. So the number of streams
open to the server is the number of enabled visualizer sensors, and with none enabled the
route is never requested.

**The cap: one state write every 200 ms at most** (`VISUALIZER_MIN_WRITE_INTERVAL`), taken
on the event loop's monotonic clock, over every write the entity makes. A state write is a
`state_changed` event to every listener in Home Assistant, a recorder row and a run of the
owner's automation, which is a command to a light. The rule under the cap is the server's
own (0153): the latest frame supersedes the ones before, nothing is queued, a held write is
made when its interval runs out, and a beat that no write has shown rides in the next. The
number is ASSUMED: half the server's ten a second, which still gives every beat of music up
to 300 beats a minute a write of its own. No measurement of a Home Assistant host or of a
lamp stands behind it; one would be a report in `docs/measurements/`.

**Idle is all zeros.** State `0`, `rgb_color` `(0, 0, 0)`, `brightness` 0, `beat` 0. The
entity is idle before its first frame, at a silent frame (`peak` 0 and `beat` 0; the HTTP
frame carries no bands, and the peak falls to 0 seconds after the bands have), and 2 s
(`VISUALIZER_IDLE_AFTER`, ASSUMED: twenty frames missed at the server's rate) after the last
frame when no silent one came. The silent frame still carries the colour in force, and a
lamp left on that colour in a silent room would be wrong; a brightness of 0 turns a light
off through `light.turn_on` itself, so the documented automation needs no second branch.

**The recorder: no long-term statistics, no attributes; the state is the recorder's.** The
sensor has no state class, so no statistics are compiled, and `_unrecorded_attributes` is
`MATCH_ALL`. The level itself is recorded in the short-term history at up to five rows a
second while a room plays, until the purge. Home Assistant gives an integration no way to
keep its own entity's states out of the recorder; the README gives the owner the
`recorder: exclude:` lines.

**Unavailable while the stream is not attached**, as the button entities are (0161), with
one info log line when it is lost and one when it is back. Coming back is idle.

**A frame is shown when it arrives.** `lead_ms` is passed on as an attribute and not waited
for: what Home Assistant, a light's integration and a lamp take is unknown and unmeasured,
and in a wired room the lead is a few tens of milliseconds, less than they are likely to
take. An automation that wants to wait can.

## Not chosen

- **A `light` entity.** A light is something Home Assistant commands; this is something it
  reads. A read-only light that refuses `turn_on` would be a lie on every dashboard.
- **An `event` entity** (one event per frame or per beat). An event entity's state is the
  time of its last event and its attributes are that event's: the colour would exist only
  while the last event was a frame, the level would not be a state a dashboard or a numeric
  trigger can use, and five timestamps a second is a worse history than five levels. A beat
  as an event entity beside the sensor is a second entity to enable for one attribute.
- **Events on Home Assistant's bus** (`hass.bus.async_fire`). No entity to enable, disable,
  see or make unavailable, so nothing would say whether a stream is held.
- **Several sensors per room** (level, colour, beat). Three state writes per frame for one
  light command, and an automation that has to read them at slightly different instants.
- **The colour as the state** (a hex string). Not comparable, and the level is what a
  dashboard charts.
- **A state class.** It would make long-term statistics of a lamp's level.
- **The server's cap alone** (ten a second, no cap in the integration). The task asks for a
  bound inside Home Assistant, and a later server's rate should not become Home Assistant's
  load.
- **Waiting `lead_ms` before each write.** See above; it would also make the cap's interval
  and the frame's delay two timers on one state.
- **One stream for all rooms, or a stream held while disabled.** The server analyses a slot
  only while something watches it.
- **Driving lights from the integration, a shipped automation or a blueprint.** Out of
  scope: the mapping is the owner's.

## Consequences

- A light that follows the music is the owner's automation; the README's example is run as
  written by `test_visualizer_documented_automation_calls_the_light_with_the_frames_colour`.
- An enabled visualizer writes up to five recorder rows a second while its room plays, unless
  the owner excludes it.
- A beat reaches Home Assistant up to 200 ms after its frame arrived when the cap holds it.
  Nothing here is a timing claim about a light.
- Every enabled visualizer probes `GET /api/state` once per 45 s of silence on its stream,
  as the other two streams do.
- A room removed from the server leaves its sensor unavailable, its stream answered 404 and
  retried at the backoff's ceiling of one a minute, until the entry is reloaded.
- The live-server test (`tests/test_live_server.py`) does not read the visualizer route yet:
  the entity is tested against the tests' fake and the shared vector.
