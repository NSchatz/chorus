# 0116: MQTT is an opt-in, read-only, publish-only MQTT 3.1.1 client written here, on one declared thread, carrying exactly what P10 settled and no Home Assistant discovery

- Status: accepted (goal 15, 2026-10-03)
- Decided by: the owner for what MQTT carries (P10, Option M2, approved at Checkpoint K:
  "MQTT off by default with an opt-in read-only publisher and no HA discovery"); the goal
  (program section 19, line C; K46, K61) inside the coordinator's goal-15 design envelope
  (section 2), track `chorus-g15/mqtt`, for everything else; every default below not cited is
  ASSUMED
- Implemented in: `crates/mqtt` (new, package `chorus-mqtt`: `codec.rs` the packets,
  `topic.rs` the topics and the prefix and level rules, `payload.rs` the cuts out of the state
  message and the event), `crates/server/src/mqtt.rs` (new: the publisher thread, the event
  tap, the clean stop), `config.rs` (`--mqtt-*`, `MqttFlags`), `control.rs`
  (`publish_events_through`, the tap in `controller`), `main.rs` (the settings, the thread, the
  count, `USAGE`); `crates/mqtt/tests/` (golden packets, topics, payloads over
  `fixtures/control`), `crates/server/tests/mqtt_publisher.rs` (the fake broker and the real
  binary), `crates/server/tests/control_thread_population.rs` (the population with and without
  the flag). The owner's page is `docs/mqtt.md`

## Context

K46 asked for "MQTT discovery" as an interface beside the PWA and the Home Assistant path. K61
then made chorus's own Home Assistant integration the way Home Assistant gets its entities,
"without duplicate entities", and P10 was written to say what is left for MQTT. The owner
approved P10's recommendation at Checkpoint K: MQTT off by default, an opt-in read-only state
and event publisher, no Home Assistant discovery, no command topics (Option M2).

Goal 15's line C still reads "MQTT discovery publishes what P10 settled against a fake broker",
and its item 2 "device discovery for what P10 says MQTT carries". chorus-server has no async
runtime, a fixed thread population that is graded against `/proc`, and client reader threads
that must never block.

## What was read

- By this track, 2026-10-03: the goal-15 design envelope (section 2 and its rules); the goal's
  MQTT research note, sections 1 to 3 (the MQTT 3.1.1 wire format with the standard's statement
  numbers, the rumqttc measurement, the Mosquitto notes); the goal's code survey (sections C
  and D); `docs/proposals/P10-ha-dashboard-mqtt.md`; [`.claude/goals/CHECKPOINT-K.approved`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/CHECKPOINT-K.approved) (the
  P10 line); `BRIEF.md` sections 3.1 and 3.2; `docs/conventions.md`; ADRs 0110 and 0112 (the
  format); the code each "Implemented in" names, and `crates/server/src/events.rs`,
  `session.rs`, `controller.rs`, `crates/control/src/fanout.rs`, `json.rs`, `catalog.rs`,
  `crates/server/tests/common/mod.rs`, `control_thread_population.rs`,
  `limits_hold_for_every_volume_path.rs`, `fixtures/control/v2/state-rich.json`.
- By the goal's research, 2026-10-03, as its note records, and relied on here through it: OASIS
  "MQTT Version 3.1.1", OASIS Standard, 29 October 2014,
  <http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html>; mosquitto.conf(5),
  <https://mosquitto.org/man/mosquitto-conf-5.html>; mosquitto_passwd(1),
  <https://mosquitto.org/man/mosquitto_passwd-1.html>;
  <https://mosquitto.org/documentation/authentication-methods/>; the issue report
  <https://github.com/eclipse-mosquitto/mosquitto/issues/3552>; for rumqttc 0.25.1
  (Apache-2.0), <https://crates.io/api/v1/crates/rumqttc>,
  <https://crates.io/api/v1/crates/rumqttc/0.25.1/dependencies> and
  <https://raw.githubusercontent.com/bytebeamio/rumqtt/main/rumqttc/Cargo.toml>.
- Not read: the source of Mosquitto (EPL-2.0 / EDL-1.0; documentation and one issue report
  only, by the envelope's rule) or of any MQTT client or broker, rumqttc's included (its
  manifest and registry metadata only). No GPL source and no reciprocally licensed design file
  was opened. The codec was written from the standard's sections as the research note restates
  them.

## Decision

1. **P10 is settled, and "discovery" in the goal's wording means what P10 settled MQTT
   carries.** Line C was written before P10 was decided and kept K46's word. P10 itself says
   how to read it: under "If the owner defers", goal 15 "builds M2 (the smallest thing that is
   'off by default' and satisfies its line C without duplicate entities)", and its comparison
   table marks line C "testable as written" for M2 and "needs amending" only for M1. So line C
   is satisfied by a test, against a fake broker, that the server publishes M2's topics and
   nothing else. **No Home Assistant discovery topic is published**, and that is asserted, not
   omitted: no packet the fake broker ever receives has a topic under `homeassistant/`, and a
   prefix under `homeassistant/` is refused at start. Building discovery "because the line says
   discovery" would build Option M4, which P10 records as declined by K61.

2. **What is built is M2 and nothing more.** Off unless `--mqtt-broker <host:port>` is given;
   `--mqtt-user`, `--mqtt-password-file`, `--mqtt-prefix` (default `chorus/v1`),
   `--mqtt-client-id` (default `chorus`), and `--mqtt-keepalive-s` (default 60, added so the
   keep alive can be tested in seconds and tuned). The topics, all QoS 1:
   `<prefix>/server/status` retained `online` with a retained `offline` will;
   `<prefix>/rooms/<id>/state` and `<prefix>/groups/<id>/state` retained;
   `<prefix>/speakers/<endpoint id>/event` not retained. No SUBSCRIBE, no command topic.

3. **One schema, by not encoding twice.** A room's payload is the bytes of its `zones[]` entry
   in the control state message and a saved group's the bytes of its `saved_groups[]` entry,
   cut out of the message the server already encoded (`chorus_mqtt::payload::retained_of` finds
   the entries by their brackets and quotes; the id is read with the catalog's own JSON
   reader). Nothing can drift from `docs/control-plane.md` and `fixtures/control`, because
   there is no second encoder. `crates/mqtt/tests/payloads.rs` cuts every committed state
   vector and requires each piece to be the vector's own bytes. The event is the one new
   payload: one flat object written with the catalog's JSON writer.

4. **Hand-written, not rumqttc.** BRIEF section 3.2 names MQTT in the gray zone: "prefer
   building when it is small and instructive, vendoring when it is large, fiddly, and
   undifferentiated". On chorus's own requirements:
   - *Size.* The need is seven packet types of MQTT 3.1.1: four encoded, three decoded, one
     varint. The codec is 434 lines with its documentation and the publisher 675, tests
     aside; the standard specifies each packet on a few pages. JSON, HTTP, SSE and DNS-SD were written here on the same
     argument (ADRs 0025, 0026).
   - *Architecture.* chorus-server has no async runtime; its threads are plain, declared before
     the scheduling report and graded against `/proc`. rumqttc 0.25.1 depends on tokio
     unconditionally (its manifest lists `tokio` with `rt`, `net`, `time`, `io-util` and
     `macros` as a non-optional dependency), so even its blocking client runs a runtime. A
     runtime's worker threads would be threads the report has to account for, for a feature
     that is off by default.
   - *Dependencies.* With default features off (plain TCP), the research resolved rumqttc to 27
     crates, 22 of them new to `Cargo.lock` (48 third-party crates would become about 70). All
     are MIT or Apache-2.0, so the licence check would pass; the bans check would not as
     resolved, since the tree brings `syn` 3 beside the `syn` 2 already locked and
     `multiple-versions = "deny"` forbids two.
   - *Semantics.* What chorus wants is specific: latest state wins while the broker is away, a
     stale button event is dropped and never replayed, everything is republished on reconnect,
     memory is bounded, timers are monotonic, `offline` is published before DISCONNECT. Here
     those are the loop; with a general client they are arranged around its request queue.
   - *Testing.* The fake broker has to exist either way (line C), and has its own decoder
     written from the standard, so the encoder is checked against an independent reading and
     against golden bytes, not against itself.
   - What rumqttc would bring: a maintained, widely used client with TLS, WebSocket, MQTT 5,
     QoS 2, subscriptions and session resumption. None is a chorus requirement, and P10 rules
     subscriptions out. **If MQTT over TLS becomes a requirement, reopen this**: TLS brings a
     TLS stack either way and changes the balance. A codec-only crate would avoid tokio and
     save only the codec, which is the small and best-specified part.

5. **A small crate for the pure half.** `crates/mqtt` holds the codec, the topic rules and the
   payload cuts, with no socket, thread or clock, and depends only on `chorus-control` (itself
   dependency-free). A crate and not a module of `chorus-server` because the boundary is then
   the compiler's: the codec cannot reach the server's state or a socket, its tests build
   without the server, and it is the same shape as the other pure cores (working agreement 5).
   There is no C twin and no endpoint speaks MQTT, so its golden bytes live in its tests and
   not under `fixtures/`. The codec has **no SUBSCRIBE encoder**: read-only is a property of
   what can be built, and a test proves none arrives.

6. **QoS 1 for everything, one packet in flight, a clean session.**
   - QoS 1 on the status and state topics so a retained message is acknowledged before the
     publisher believes the broker holds it; that belief is what "only what changed is
     republished" is computed against. QoS 0 would make it a guess.
   - QoS 1 on events as well: one code path, and the acknowledgement is how a dead link is
     noticed on the next thing sent.
   - One packet in flight: the next PUBLISH is sent when the last was acknowledged. Volume is
     low (a state change per human action) and it makes the packet identifier a counter and the
     link's health one timeout.
   - Clean Session 1: a publisher that never subscribes and republishes everything on connect
     has no use for a stored session, and with a clean session nothing is ever re-sent, so DUP
     is never set and an event is never delivered twice by chorus's doing. An event
     unacknowledged when a connection ends is dropped.
   - The will is QoS 1 and retained, and `online` is published after the CONNACK, so that an
     `offline` from a takeover of an older connection lands before it.
   - A PUBACK does not prove the broker's ACL accepted a message ([MQTT-3.3.5-2]); `docs/mqtt.md`
     gives the owner the check that does.

7. **One thread, declared, only when enabled.** `mqtt-publisher` is created in `main.rs` with
   the rest before the scheduling report, registers itself as ordinary, and is added to the
   expected count; without `--mqtt-broker` it does not exist. The population is `6 + 2N + M`,
   plus one with `--mqtt-broker`. Name resolution, connect, write and every wait for an answer
   happen on it under timeouts. A second thread (a reader) is not needed: with one packet in
   flight the thread knows when the broker owes it something, and otherwise looks at the socket
   without waiting. `control_thread_population.rs` grades both shapes against `/proc`, with a
   broker that never answers.

8. **Nothing waits for the publisher.** State reaches it through the control fanout like any
   subscriber (bounded; a subscriber that falls behind is dropped, and the publisher then
   subscribes again and reads the state as it stands). Controller events reach it by
   `EventTap::offer`, a `try_send` into a queue of 64 from the client reader thread, placed
   before the state is fanned out so the publisher wakes with the event already there; a full
   queue drops the event and counts it. The publisher wakes on a state change, and otherwise
   every 100 ms to look for an event, a stop, a closed connection and a due keep alive.

9. **Reconnect.** At once, then after 1 s doubling to 60 s; an accepted connection resets the
   wait. No jitter: there is one publisher, so there is no herd to spread. A CONNACK refusal is
   reported with its code and retried on the same schedule, which at 60 s is not hammering.
   After every connect everything is published again.

10. **The clean stop, as far as the shutdown allows.** The server has no signal handler and
    joins no thread; a run that ends by itself returns from `main`. A guard dropped on that
    return stops the publisher and waits at most 2 s while it publishes retained `offline`
    (waiting at most 1 s for the PUBACK), sends DISCONNECT and closes. If `offline` is not
    acknowledged no DISCONNECT is sent, so the will still fires. A killed process (the
    deployed case: `docker stop`) says nothing and the broker's will says `offline`.
    `docs/mqtt.md` says both.

11. **Ids as topic levels, and what is refused at start.** `/`, `+`, `#`, `%` and control
    characters in an id become `%XX` per UTF-8 byte, so an id is always exactly one level and
    the mapping is one to one. The client id is held to the 23 alphanumerics every broker must
    accept ([MQTT-3.1.3-5]); the prefix to well-formed levels, not `$...`, not
    `homeassistant/...`. A password with no user is refused ([MQTT-3.1.2-22]). The password
    comes only from a file, read once at start; no flag takes it, no type that holds it derives
    `Debug`, and a test searches the server's whole output and its `/proc/<pid>/cmdline` for
    it.

## The end-to-end tests

`crates/server/tests/mqtt_publisher.rs`, the real binary against the fake broker (loopback, a
kernel-chosen port; it records every packet, answers CONNACK, PUBACK and PINGRESP, models
retained messages and the will, and can refuse, drop and go silent):

- `mqtt_publishes_what_p10_settled_against_a_fake_broker` (line C): the CONNECT's will and the
  credentials from the file; retained `online` first; one retained state per room and per saved
  group, each the catalog writer's bytes for that entry and found in `GET /api/state`; a volume
  change publishes that room alone; a deleted saved group's topic is cleared; a controller
  command is one event that is not retained; a transport command likewise; no SUBSCRIBE, no
  packet type beyond the four, every topic under the prefix, none under `homeassistant/`; the
  password in nothing the server printed. It prints the broker's packet log.
- `mqtt_is_off_by_default_no_thread_no_connection_no_line`: without the flag no MQTT thread, no
  line, and exactly one socket fewer than the same server with it.
- `a_broker_that_refuses_then_dies_...`: CONNACK 5, then 4, then accepted, with commands served
  meanwhile; the broker drops the connection, the will fires, the publisher returns and
  republishes everything.
- `a_broker_that_is_not_there_does_not_stop_the_server`, `the_keep_alive_is_a_pingreq_...`,
  `a_clean_stop_says_offline_and_then_disconnects`, `a_password_file_that_cannot_be_read_...`.

No test is graded on how long something took.

## Deviations from the envelope

- `--mqtt-keepalive-s` is a flag the envelope does not list. It exists so the keep alive is
  tested in seconds, and it is a real tunable.
- `tests/thread_shape.rs` is unchanged: it grades the shape with no control plane, where
  `--mqtt-broker` is refused (the config test asserts that), so the publisher cannot exist
  there. The enabled and disabled shapes are graded in `control_thread_population.rs`.

## ASSUMED

The keep alive default (60 s), the backoff (1 s to 60 s), the response timeout (half the keep
alive, 1 to 10 s), the event queue (64), the poll (100 ms), the stop's waits (1 s and 2 s), the
prefix bound (128 bytes). None is a measured choice and none is a timing claim about audio.
That Mosquitto checks a will against its ACL only when the will fires rests on one issue
report, and what a 3.1.1 client sees on a denied publish was not found in its documentation:
both are the owner's confirmations in `docs/mqtt.md`.

## Follow-ups

- The owner's, on the homelab: a Mosquitto user for chorus with `topic write chorus/v1/#`,
  entries for every existing user in the same edit if an `acl_file` is introduced, the password
  set with `mosquitto_passwd` (never `-c`) and kept outside git, and the two confirmations on
  the real broker (the will under the ACL; a denied publish). `docs/mqtt.md` has the notes.
  `deploy/` does not pass `--mqtt-broker`.
- A retained topic of a room or saved group removed while the server was not running is not
  cleared (a publisher that never subscribes cannot learn what the broker holds).
- MQTT over TLS, if it is ever wanted, reopens decision 4.
- P10's option text also lists "playing, input ... now-playing title and artist" in a room's
  retained state. The room object published (the control state's own, one schema) carries
  volume, mute, limit and group membership only: the source lives on the live group
  (`groups[].source`), which is not published, and no title or artist exists before goals 16 and
  17. The goals that add inputs and now-playing put them in the control state's room object, or
  publish the live group, and the room topic then carries them (found by goal 15's adversarial
  check, 2026-10-03).
