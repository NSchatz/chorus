# MQTT: what chorus publishes, and what it does not

chorus-server can publish what it already knows to an MQTT broker (goal 15). It is **off by
default**, **read-only** and **publish-only**: it tells a broker the state of each room and saved
group and each button press it accepted, and it is told nothing back. The decision record is
`docs/decisions/0000-an-opt-in-read-only-mqtt-publisher.md`; what was settled is proposal P10
(`docs/proposals/P10-ha-dashboard-mqtt.md`, Option M2, approved at Checkpoint K): "MQTT off by
default with an opt-in read-only publisher and no HA discovery".

The code is `crates/mqtt` (the packets, the topics, the payloads: a pure library) and
`crates/server/src/mqtt.rs` (the one thread that talks to the broker).

## The switch

Nothing happens without `--mqtt-broker`. With no such flag the server creates no thread for
MQTT, opens no connection and prints no line about it, and `deploy/` does not pass it.

| flag | default | what |
|---|---|---|
| `--mqtt-broker <host:port>` | off | publish to this MQTT 3.1.1 broker over plain TCP; needs `--control-listen` |
| `--mqtt-user <name>` | none | the user name to connect with |
| `--mqtt-password-file <path>` | none | read the password from this file at start (one trailing line ending is removed); needs `--mqtt-user` |
| `--mqtt-prefix <prefix>` | `chorus/v1` | the topic prefix |
| `--mqtt-client-id <id>` | `chorus` | the MQTT client id: 1 to 23 of `0-9 a-z A-Z`, the set every broker must accept |
| `--mqtt-keepalive-s <n>` | 60 (ASSUMED) | MQTT keep alive in seconds, 1 to 65535 |

Each of the last five is refused without `--mqtt-broker`, by its own name. There is **no flag
that takes the password itself**: it is read from the file, so it is never in a process list or a
shell history, and the server never prints it (its start line says `password=set` or
`password=none`). A password file that cannot be read, or is empty, stops the server at start
with exit code 2 before anything is bound: that is a mistake in the configuration, not a broker
being away.

Two servers on one broker need two client ids and two prefixes. A broker disconnects the older
of two connections that share a client id, so two servers with one id would take turns knocking
each other off.

The prefix is one or more topic levels: no empty level, no `+` or `#`, no control character, not
beginning with `$`, at most 128 bytes, and **not under `homeassistant/`**, which is refused by
name whatever else is asked.

## The topics

This is the whole list. `<prefix>` is `chorus/v1` unless `--mqtt-prefix` says otherwise.

| Topic | Retained | QoS | Payload |
|---|---|---|---|
| `<prefix>/server/status` | yes | 1 | `online` while the server is connected; `offline` otherwise (the last will, and what a clean stop publishes itself) |
| `<prefix>/rooms/<room id>/state` | yes | 1 | the room's object from the control state message: its `zones[]` entry, byte for byte |
| `<prefix>/groups/<saved group id>/state` | yes | 1 | the saved group's object: its `saved_groups[]` entry, byte for byte |
| `<prefix>/speakers/<endpoint id>/event` | **no** | 1 | one JSON object for each controller command the server accepted |

- A room or saved group that no longer exists gets a retained message with an **empty payload**
  on its topic, which is how MQTT clears a retained message.
- Only what changed is published: a volume change on one room publishes that room's topic and no
  other.
- After every connect, and every reconnect, `online` and then every room and every saved group
  are published again, whether or not they changed: a broker that restarted may hold nothing.
- The `v1` in the default prefix is the version of this topic layout. The payloads carry the
  control catalog's own shape (catalog version 2 today).

### An id as a topic level

A room id and a saved group id are the control catalog's identifiers (lower-case letters, digits
and `-`), and are used as they are. An endpoint id is chosen by the endpoint, so the rule that
makes any id exactly one topic level is: each of `/`, `+`, `#`, `%` and every control character
(U+0000 included) becomes `%XX` for each byte of its UTF-8 form, in upper-case hexadecimal;
everything else is kept. `%` itself is escaped, so two different ids never share a topic. An
empty id becomes a lone `%`.

### Payload examples

One schema: the payloads are pieces of the control state message that `GET /api/state` serves
and `docs/control-plane.md` ("The state message") documents, cut out of the message the server
already encoded and not encoded a second time. The two below are the `kitchen` room and the
`downstairs` saved group of `fixtures/control/v2/state-rich.json`, and
`crates/mqtt/tests/payloads.rs` holds this page to that file.

`chorus/v1/rooms/kitchen/state`:

```json
{"id":"kitchen","name":"kitchen","group":"downstairs","volume":0.343,"muted":false,"endpoints":[],"present":[],"audio":"127.0.0.1:4011","transport":"wired","limit":0.400,"effective_limit":0.400,"quiet":[],"bond":[],"ramp":null,"sound":{"bass":0,"treble":0,"loudness":true,"night":false,"speech":false,"tv_upmix":"off"},"av_trim_ms":0,"bass_management":{"crossover_hz":80,"sub_level_db":0.00,"sub_polarity":"normal","active":false},"room_eq":{"enabled":false,"filters":[]}}
```

`chorus/v1/groups/downstairs/state`:

```json
{"id":"downstairs","name":"Downstairs","zones":["living","kitchen"],"active":true}
```

`chorus/v1/speakers/endpoint-a/event`, after a volume-down press on `endpoint-a` in the kitchen:

```json
{"endpoint":"endpoint-a","zone":"kitchen","command":"volume_step","value":-5,"target":"","outcome":"applied"}
```

The event's fields: `endpoint` is the endpoint whose button it was (its authenticated id);
`zone` is the room it plays in, which the command acted on; `command` is the protocol's name for
it (`play`, `pause`, `toggle`, `next`, `previous`, `volume_set`, `volume_step`, `mute_set`,
`join`, `leave`: `docs/protocol.md`, "controller command"); `value` is its argument, 0 where it
takes none; `target` is the group for `join` and empty otherwise; `outcome` is `applied` when
the command changed the room, and `waits-for-an-input` for a transport command, which acts on an
input. The server never sees a raw press, only the command the endpoint's controls made of it,
and a command it refused is not published.

An event is not retained, so a subscriber that connects later is never replayed a press. An
event that arrives while the broker is away is dropped, not kept for later, for the same reason.
At most 64 events wait for the publisher; past that an event is dropped.

## What is not there, and why

P10 settled these, and the tests hold the server to each against the fake broker's log of every
packet it was sent.

- **No Home Assistant discovery.** Nothing is ever published under `homeassistant/`. Home
  Assistant gets its entities from chorus's own integration (goals 18 and 19); MQTT discovery
  beside it would make every one of them twice, and MQTT has no media player to describe at
  all. Goal 15's line C says "MQTT discovery publishes what P10 settled": P10 settled that MQTT
  carries the topics above and no discovery, and that is what is built (the decision record
  says how the wording was read).
- **No command topics.** There is nothing to publish to that changes anything. The server never
  sends SUBSCRIBE: the library that makes its packets has no SUBSCRIBE to make. MQTT is not a
  second control path; the control API (`docs/control-plane.md`) and `chorusctl` are the ways to
  change something.
- **No duplicate entities beside the integration.** These topics are for tools outside Home
  Assistant. Home Assistant should not be pointed at them to build entities by hand.
- **No artwork, no audio, no telemetry.** Per-speaker telemetry is goal 15's Prometheus
  endpoint on the control listener, not MQTT.
- **No TLS.** The connection is plain TCP (see the notes below).

## When the broker is away

A broker that is down, refusing, slow or silent never blocks or fails the server and never
blocks a session. Everything that touches the broker runs on one thread (`mqtt-publisher`, an
ordinary thread created at start with the rest and counted in the thread population:
`docs/control-plane.md`, "The thread population"); a state change reaches it through the same
bounded fanout every subscriber uses, and a button event through a queue that is never waited
on.

- Connecting is tried at once, then again after 1 s, 2 s, 4 s and so on up to 60 s between
  attempts (ASSUMED values); an accepted connection starts the wait at 1 s again. Each attempt
  that fails prints one line, `mqtt not-connected broker=... reason=... retry_ms=...`. A refusal
  names the broker's return code (4 is a bad user name or password, 5 is not authorized).
- The connection is kept alive with PINGREQ at half the keep alive. A broker that does not
  answer a CONNECT, a PUBLISH or a PINGREQ within half the keep alive (at least 1 s, at most
  10 s) is given up on and connected to again.
- While it is away, state changes are not queued: when the connection is back, the state as it
  then stands is published.

### What consumers see when the server stops

- **A clean stop** (the run ends by itself): the server publishes a retained `offline`, waits up
  to 1 s for the broker to acknowledge it, sends DISCONNECT and closes. The supervisor waits at
  most 2 s for that before the process exits.
- **Anything else** (the process is killed, as `docker stop` and a power cut do; the host or the
  network goes away; the broker did not acknowledge `offline` in time): the server says nothing,
  and the broker publishes the last will, a retained `offline`. For a killed process the broker
  sees the connection close and does so at once; for a silent loss it does so after one and a
  half times the keep alive, as MQTT 3.1.1 specifies (section 3.1.2.10). chorus-server installs
  no signal handler, so in a deployment `offline` normally comes from the will.
- The retained room and group states stay on the broker while the server is away. A consumer
  reads `<prefix>/server/status` to know whether they are current.
- **Not covered:** a room or saved group removed while the server was not running (or removed
  from its configuration between runs) keeps its retained message, because a publisher that
  never subscribes cannot learn what a broker holds from an earlier run. Clear such a topic by
  hand with a retained empty message.

## Mosquitto ACL notes for the owner

These are notes for the broker's side, which is the owner's (`docs/decisions/0000-...`,
"Follow-ups"). Nothing in this repository touches a broker, and every name below is a
placeholder. Sources, each read 2026-10-03 by the goal's research: mosquitto.conf(5),
<https://mosquitto.org/man/mosquitto-conf-5.html>; mosquitto_passwd(1),
<https://mosquitto.org/man/mosquitto_passwd-1.html>;
<https://mosquitto.org/documentation/authentication-methods/>; and the issue report
<https://github.com/eclipse-mosquitto/mosquitto/issues/3552>. Only Mosquitto's documentation
and that report were read, never its source.

**A user for chorus that can write under the prefix and read nothing**, in the ACL file:

```
# chorus-server: may publish under chorus/v1/, may read nothing
user chorus
topic write chorus/v1/#

# a consumer: may subscribe, may not publish
user reader
topic read chorus/v1/#
```

`write` is "may publish to", `read` is "may subscribe to and receive". The user name is the one
in the password file, not the client id. `chorus/v1/#` also covers the last will's topic,
`chorus/v1/server/status`, which matters (below).

**Introducing an `acl_file` cuts off every user that has no entry in it.** mosquitto.conf(5):
"If this parameter is defined then only the topics listed will have access." A broker that has
no ACL file today lets every user publish and subscribe everywhere; the moment one is added,
the users already there can do nothing until they have entries. So the same edit that adds
`acl_file` gives every existing user its entry (for one that needs everything as before:
`topic readwrite #` under its own `user` line). If the broker already has an ACL file, only the
two blocks above are new.

**The password is the owner's step, and it lives outside git.** Add the user to the existing
password file, which prompts for the password:

```
mosquitto_passwd /path/to/passwd chorus
```

Never pass `-c` on a file that exists: `-c` creates the file and overwrites one that is there,
deleting every other user. Avoid `-b`, which takes the password on the command line. Reload the
broker afterwards (SIGHUP, or restart it). The same password goes, alone, into the file
`--mqtt-password-file` names, readable by the server's user only, and never into the
repository, a compose file or an argument.

**Plain TCP.** chorus connects without TLS, so the user name and password cross the wire in
the clear; mosquitto.conf(5) says of password files, "Be sure to use network encryption if you
are using this option otherwise the username and password will be vulnerable to interception."
That is acceptable only where the path between chorus-server and the broker is trusted: the
same host, or a container network. Do not point `--mqtt-broker` at a broker across an untrusted
network.

**Two things to confirm once on the real broker**, because the documentation does not settle
them and the fake broker in the tests cannot:

1. **The will under the ACL.** The issue report above (against Mosquitto 2.1.2, MQTT 3.1.1)
   says a will's topic is checked against the ACL only when the will is about to be published,
   not when the client connects, and a denied will is dropped without a word. So an ACL that is
   too narrow shows no error anywhere; the symptom is a status that stays `online` after the
   server is killed. Confirm: with a subscriber on `chorus/v1/server/status`, kill the server
   (not a clean stop) and see `offline` arrive.
2. **A denied publish.** MQTT 3.1.1 has no "publish denied" answer: a broker either
   acknowledges normally or closes the connection ([MQTT-3.3.5-2]), so chorus cannot tell a
   denied publish from an accepted one, and its log is not evidence. Confirm: with the reader
   subscribed to `chorus/v1/#`, see the room states arrive; and, to see what a denial looks
   like on this broker, start the server once with `--mqtt-prefix` set to something the ACL
   does not allow and note whether the broker acknowledges and drops, or disconnects.

The acceptance test for the ACL is a subscriber seeing the messages, not the absence of errors
in chorus's output.

## How it is tested

`crates/mqtt/tests/` holds the packets to bytes worked out from the standard (the CONNECT, each
PUBLISH flag, the remaining length at every boundary, the three packets read back), the topic
and prefix rules, and the payload cuts over every state vector in `fixtures/control`.
`crates/server/tests/mqtt_publisher.rs` starts the real server against a fake broker that lives
in the test (loopback, a port the kernel chose, its own decoder), and asserts what this page
says: `mqtt_publishes_what_p10_settled_against_a_fake_broker` and its neighbours. To see the
broker's packet log:

```
cargo nextest run -p chorus-server --test mqtt_publisher --no-capture \
    -E 'test(mqtt_publishes_what_p10_settled_against_a_fake_broker)'
```

No test, script or document here names or reaches a real broker.
