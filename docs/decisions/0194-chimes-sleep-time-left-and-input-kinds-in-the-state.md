# 0194: the state message lists the built-in chimes, says how long each sleep timer has left and says each offered input's kind, each written only by a server that knows it

- Status: accepted, 2026-10-05.
- Decided by: the owner's agent harness, for the wire shape: three additive members, each
  cheap to reverse while no released consumer reads it. What is asked for (the state carries
  the chime names, a sleep timer's time left and each input's kind, so the app's screens do
  not hard-code or guess them) is the task's.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/control/src/zones.rs` (`set_chimes`, `set_sleep_counted`,
  `sleep_remaining`, `set_input_kind`, the state builder), `crates/control/src/rooms.rs`
  (`InputKind`, `SleepTimer::remaining_s`), `crates/control/src/catalog.rs`
  (`input_kind_value`), `crates/server/src/schedule_runtime.rs` (`chime_names`,
  `describe_in`, `input_kind`, the count in `step_sleeps`), `crates/server/src/conductor.rs`
  (the kind said before an input is offered), `crates/server/src/main.rs`;
  `crates/control/tests/state_facts_v2.rs`, `crates/server/tests/state_facts.rs`,
  `fixtures/control/v2/state-facts`.

## Context

Three things a screen needs were not in the state message, checked against the code before
this record:

- **The chimes.** The catalog validates only the spelling of `chime:<name>`; which chimes
  exist is `CHIMES` in `crates/schedule/src/chime.rs`, and nothing sent it. A screen that
  offers an alarm's chime would keep its own copy of the three names.
- **A sleep timer's time left.** `sleep[]` carried `target` and `minutes`, the request. The
  countdown is the schedule runtime's, on the monotonic clock, and the room model reads no
  clock, so the state could not say how much was left.
- **An input's kind.** `inputs[]` is a list of `<endpoint>/<input>` strings. Whether one is
  analogue, optical or HDMI ARC was known only to the server's line-in registry (it was
  already read there for the OpenHome source list). A label's `role` is a person's word and
  is absent until a person gives it.

Two rules of the state message bear on the shape. A reader takes the members it knows by
name and ignores the rest, so a new member breaks no consumer. And every member added since
the first v2 state is written only when there is something to say, so the committed vectors
are the bytes they were (`docs/control-plane.md`, "The state message").

## Decisions

1. **`chimes[]`, last in the state: the names, read from the schedule library's own list.**
   `chorus_server::schedule_runtime::chime_names()` maps `CHIMES` to names and the server
   tells the room model once at start (`Zones::set_chimes`). The control crate stays without
   a dependency on the schedule crate (it has none at all); it reads the list only in a
   test, which holds the state's list and the committed vector's to `CHIMES`. Written only
   by a server that said any: a model no server feeds says nothing.

2. **`remaining_s` on each `sleep[]` entry: whole seconds left, rounded up, as the runtime
   last counted them.** Seconds and not minutes, because a screen shows the last minute; a
   duration and not an end time, because the countdown is on the monotonic clock and the
   state has no instant a reader could compare with its own clock. The model sets it to all
   of `minutes` when the timer is asked for (so the state that answers the command already
   carries it) and the runtime writes it every tick (`Zones::sleep_remaining`), at least
   once a second.

3. **The count is kept every tick, and a state is sent for it only when the whole minutes
   left change.** A serial that moved every second for every timer would send every
   subscriber, the MQTT bridge included, a whole state a second for nothing a person did.
   So `sleep_remaining` always stores the count, which makes `GET /api/state` and every
   state sent for another reason exact to the second, and moves the serial only when the
   count crosses a whole minute: at most one state a minute for each timer. The cost is
   that two states with one serial can differ in `remaining_s`; a reader that keys on the
   serial is at most a minute behind on this one member, and one that shows seconds counts
   down itself between states. `minutes` keeps its meaning: what was asked for.

4. **`remaining_s` is written only where a runtime counts.** A server with a schedule
   runtime says so once at start (`Zones::set_sleep_counted`); a model nothing counts
   writes `target` and `minutes` alone, which is what `state-rich.json` pins.

5. **`input_kinds[]`, a member of its own beside `inputs[]`: `input`, `kind`, `tv`.**
   `inputs[]` stays a list of strings: turning its entries into objects would change an
   existing member's meaning and break every reader. `input_kinds[]` has one entry for each
   offered input whose kind the server said, in the order of `inputs`. `kind` is the sync
   protocol's name (`line_in`, `optical`, `hdmi_arc`); `tv` is `true` for the last two, so
   a reader need not know which kinds are a TV's. The kind is not put on `input_labels[]`:
   a label is configuration and outlives the offer, and the kind is known only while the
   input is offered.

6. **The conductor says the kind before the runtime offers the input**, in the same change
   of the room model, so no state lists an input of `chorus-server` without its kind. A
   withdrawn input's kind is dropped with it. The server maps the protocol's `SourceKind`
   to the catalog's `InputKind` with an exhaustive match: a fourth kind does not compile
   until it has a name here.

## Consequences

- The app's screens (alarms, sleep, inputs) read all three from the state. No web change is
  in this record's pull request.
- Existing consumers (the Home Assistant integration, MQTT, `chorusctl`) read what they read
  before; no existing vector moved, and `fixtures/control/v2/state-facts.json` pins the new
  members.
- With several sleep timers running, subscribers get up to one more state a minute for each.
- Not done here: sound range bounds in the state (the catalog's documented bounds serve),
  snooze, and any new command.
