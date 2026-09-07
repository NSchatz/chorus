# The chorus control plane

Version: catalog version **1**, the PRODUCT-6 catalog. Every message is one
JSON object, UTF-8, with no byte order mark and no trailing newline on the wire.

This document and the vectors under `fixtures/control/` are the contract. A
second implementation is correct when it produces those bytes and recovers
those fields, not when it matches the Rust code. Where this document and
`crates/control` disagree, this document is right and the code is the defect.

**This is a SECOND catalog, beside the audio wire and not inside it.**
`docs/protocol.md` is the contract for the audio frames on the audio
connection; nothing here changes a byte of it. A control message never travels
on an audio connection and an audio frame never travels on a control one. Why
they are separate: the audio wire is decoded by a C endpoint on an ESP32-S3
with a fixed frame budget, and a JSON reader does not belong on that path.

Why the pieces are shaped the way they are:
`docs/decisions/0016-the-control-catalog.md` (the version, the volume range and
its curve), `docs/decisions/0017-the-control-fanout.md` (the subscriber queue
ceiling), `docs/decisions/0018-the-persisted-zone-state.md` (the state file),
`docs/decisions/0019-a-persisted-endpoint-that-never-comes-back.md`.

## The state a server holds

One server owns all of it. There is no state anywhere else that a subscriber
could be reading instead, which is what makes "fan the resulting state out to
all subscribers" a complete description rather than half of a reconciliation
problem.

A **zone** is a room. It has:

| field | what it is |
|---|---|
| `id` | the identifier, fixed at configuration and never changed by a message |
| `name` | the human-set name, which is what a person sees |
| `group` | the group whose stream this zone plays |
| `volume` | the amplitude factor its endpoints apply |
| `muted` | whether it is silenced |
| `endpoints` | every endpoint this zone has ever had, persisted |
| `present` | the subset attached right now, never persisted |

A **group** is the unit a stream is served to. Every zone is in a group,
always: a zone in no group would be a zone with nothing to play. `ungroup`
therefore puts a zone into a group of its own, named for the zone, rather than
into an absent state the state message would have to spell `null`. One
consequence worth stating: `ungroup` on a zone that is already alone is not an
error and changes nothing.

The set of zones is **configured, not commanded**. `--zone <id>` on the server's
command line, one per room. A control message cannot create a zone, because the
set of rooms is a fact about a house, and a typo in a zone name has to be a
refusal rather than a new room nobody has.

## Identifiers, names and numbers

- A **zone, group or endpoint identifier** is 1 to 32 characters of lower-case
  ASCII letters, digits and hyphens. Narrow on purpose: an identifier appears in
  a DNS-SD instance name, in a persisted state file's section header and in a
  URL, and a character that has to be escaped differently in each of those is a
  character that will eventually be escaped wrongly in one of them.
- A **name** is 1 to 64 characters, no control character, and no leading or
  trailing space. Every other printable character is a name character, `#`,
  `\`, `[`, `]` and `=` included, because a name is what a person types into
  the rename box and none of those is a reason to refuse one. The persisted
  state format carries whatever this rule admits: it escapes `\` and `#` on the
  way out and resolves them on the way back, and refuses to install a file it
  cannot read back as the same state, so **every name this rule accepts
  survives a restart byte for byte**
  (`docs/decisions/0018-the-persisted-zone-state.md`; pinned by
  `crates/server/tests/regress_0043_f1.rs` and by the names
  `tools/restart-storm-run.sh` sets). The one exclusion is the control
  characters above: the state file holds a name on one line, and a name
  carrying a newline would be a name that does not come back.
- A **volume** is a decimal from `0.000` to `1.000` inclusive, in steps of
  `0.001`, written with **exactly three fractional digits**. `0.5` and `0.50`
  are not this value on the wire. It is the AMPLITUDE FACTOR and not a position
  on a perceptual curve: `0.500` means every sample is multiplied by one half,
  and `docs/decisions/0016` records why the curve is not on the wire.

## The canonical encoding

One message has exactly one spelling. That is what makes a golden vector a
contract rather than a coincidence.

- RFC 8259 JSON, UTF-8, no byte order mark.
- **No insignificant whitespace anywhere**: `{"k":1,"v":"x"}`.
- **Members in the order the tables below give them**, not sorted.
- Strings escape `"`, `\`, and the C0 controls, using `\b \f \n \r \t` where
  those exist and `\u00xx` otherwise. `/` is NOT escaped. A character above
  U+007F is written as itself in UTF-8.
- Whole numbers are written with no sign, no leading zeros and no exponent.
- A volume is written with exactly three fractional digits and no exponent.

## Decoder behaviour

In order. The order is the contract: it decides which answer a message gets.

1. The text is JSON, or the message is **malformed**. A duplicate key is
   malformed - two spellings of one field have two meanings and no way to choose
   between them. Nesting deeper than 32 levels is malformed. Content after the
   value ends is malformed, so two messages concatenated cannot read as one.
2. `v` is present, is a whole number, and is a version this build implements,
   or **the session is refused** and nothing from the peer is applied,
   including the rest of the message the version arrived on.
3. `t` names a command in this catalog, or the message is rejected.
4. The fields that command declares are present, of the declared type, and
   inside the declared range, or the message is rejected naming the field.
5. No field is present that the command does not declare, or the message is
   rejected naming it.

**Rules 2 and 5 are the opposite of what the audio wire does, and deliberately
so.** `docs/protocol.md` has a decoder skip an unassigned type byte and accept a
payload longer than the fields it knows about, which is what makes adding an
audio message type a non-event for every existing implementation. The control
catalog cannot afford that: an audio frame a decoder skips costs 20 ms of one
stream, and a control message a decoder half-understands changes what a house is
doing. A message with a field this build does not know is a message from a peer
speaking a version this build does not have, and the version field is where that
is said.

**A rejection costs one message; the session stays open.** Only an
unimplemented catalog version ends it.

**A refused message applies no part of itself and leaves every subscriber's
state byte-identical.** Validation happens before anything is changed.

## The commands

Client to server. Every one carries `v` and `t` first, in that order.

### `hello`

```json
{"v":1,"t":"hello"}
```

Opens a session and announces a catalog version. It changes nothing; its answer
is the state.

### `attach`

```json
{"v":1,"t":"attach","zone":"kitchen","endpoint":"endpoint-a"}
```

Says that an endpoint is playing a zone. Sent by the ENDPOINT, on every control
connection it opens and not only the first: a server that has restarted has the
zone's name, group, volume and mute back out of its state file and nothing about
which endpoints are switched on, because that is a fact about now.

The endpoint joins the zone's `endpoints` if it is not in it already, and its
`present`.

### `name`

```json
{"v":1,"t":"name","zone":"kitchen","name":"The Kitchen"}
```

### `group`

```json
{"v":1,"t":"group","zone":"kitchen","group":"downstairs"}
```

Puts a zone into a group. The zone's endpoints then play that group's stream:
the state message's `audio` field carries the address, and an endpoint that sees
it change ends its session and opens the next one there.

### `ungroup`

```json
{"v":1,"t":"ungroup","zone":"kitchen"}
```

Puts a zone into a group of its own, named for the zone.

### `volume`

```json
{"v":1,"t":"volume","zone":"kitchen","volume":0.500}
```

### `mute`

```json
{"v":1,"t":"mute","zone":"kitchen","muted":true}
```

Muting does not change `volume`, so unmuting gives back what was set. An
endpoint applies `0.000` while muted and keeps writing exactly as many frames:
a mute that stopped writing would change that endpoint's alignment, and coming
back from it would be a resync.

## The state message

Server to every subscriber, after every change, and once when a subscriber
attaches. It is a complete snapshot: there is no incremental form, and a
subscriber that has one message needs nothing else.

```json
{"v":1,"t":"state","serial":7,"zones":[{"id":"kitchen","name":"Kitchen","group":"downstairs","volume":0.375,"muted":false,"endpoints":["endpoint-a","endpoint-b"],"present":["endpoint-a"],"audio":"127.0.0.1:4011"}]}
```

| field | notes |
|---|---|
| `serial` | changes applied since the state was created or loaded. A subscriber can tell a message it has seen from one it has not without comparing the whole thing |
| `zones` | every zone, in the order they were configured or loaded |
| `zones[].audio` | where this zone's group's stream is served, from `--group-audio <group>=<address>` or the server's own listen address |

A server with no zone configured serves `{"v":1,"t":"state","serial":0,
"zones":[]}`. That is a state and not an error, and
`fixtures/control/state-empty.json` pins it, so "no zones yet" is a shape the
catalog declares rather than something a client infers from a missing field.

## The refusals

```json
{"v":1,"t":"error","field":"volume","detail":"the volume 2.000 is outside the range the catalog declares, which is 0.000 to 1.000 in steps of 0.001"}
```

`field` is `""` where the fault is the whole message. `detail` is for a person
and its wording is part of the committed vectors, because an error naming the
offending field is what the criterion asks for and a message that stopped naming
it would still be an error.

```json
{"v":1,"t":"refused","field":"v","detail":"catalog version 9 was offered and this build implements 1; nothing from this peer has been applied","offered":9,"implemented":[1]}
```

`offered` is the version the peer sent, or `null` where it sent no number.
`implemented` is every version this build has.

## How the messages travel

The catalog above is the contract. The transport is HTTP on the address
`--control-listen` names, and it is deliberately ordinary:

| route | what it does |
|---|---|
| `GET /` | the control page |
| `GET /chorus.css`, `GET /chorus.js` | what the page loads |
| `GET /api/state` | the state message, once |
| `GET /api/events` | a `text/event-stream`, one `data: <state message>` per change, starting with the state as it stands |
| `GET /api/report` | one line of plain text: commands applied, commands refused, connections turned away, and the fanout's ceiling and drops |
| `POST /api/command` | the body is one control message. `200` with the resulting state, `400` with an `error`, or `426` with a `refused` |
| `POST /api/leaving` | the body is an endpoint identifier, which stops being `present` |

A browser opens `/api/events` with `EventSource` and a shell script opens it
with a socket and a `GET` line, so **the UI and the verification scripts are the
same subscriber**. That is deliberate: a check that exercised a second, private
subscriber protocol would not be checking the thing the browser uses.

Every request is answered with `Connection: close`. A connection arriving when
every worker is busy is answered `503` naming the ceiling and closed, rather
than served by a thread nobody declared; see below.

## The bound on a subscriber

There is no back pressure: one subscriber that has stopped reading must not
delay another's fanout and must never reach the audio path. That decoupling
needs its other half or it is an unbounded allocation, so every subscriber's
queue is bounded at 32 state messages, and a subscriber that reaches the ceiling
is **dropped**, with what it never received counted and reported by
`GET /api/report`.

That is the opposite of what the audio fanout does with a slow client, which
drops the ITEM and keeps the subscriber. `docs/decisions/0017` records why the
two differ.

## The thread population

With `--control-listen`, the server runs one control acceptor and one worker per
`--control-workers` slot, all created before the scheduling report is taken and
none created afterwards however many subscribers come and go. The whole process
is `4 + 2N + M` threads, plus one for `--advertise`.

This is a safety property and not a style. `std::thread::spawn` inherits the
creating thread's scheduling policy, and `deploy/run-server.sh` runs the server
with `--ulimit rtprio=20`, so a control plane that spawned a thread per
subscriber would be putting real-time threads on a host that also runs other
things - and the report the host contract is graded on would never have seen
them. `crates/server/tests/control_thread_population.rs` grades it against
`/proc`.

## Discovery

The server advertises two DNS-SD services when started with `--advertise`:

| service | what is at it |
|---|---|
| `_chorus-audio._tcp.local.` | the audio stream, at the `--listen` port |
| `_chorus-ctl._tcp.local.` | the control channel, at the `--control-listen` port |

Each is a PTR answer naming the instance, with the SRV, TXT and address records
in the additional section, per RFC 6763 sections 4.1, 5 and 6. The SRV priority
and weight are both zero, which section 5 says they SHOULD be for one instance
described by one record. The TXT record carries `v=<catalog version>` and, on
the audio service, `ctl=<control port>`.

An endpoint started with `--discover` sends a PTR query for
`_chorus-audio._tcp.local.` to 224.0.0.251:5353 (RFC 6762 sections 2 and 3),
with the unicast-response bit set so a responder answers it directly and it does
not need to bind port 5353 itself. `fixtures/discovery/` pins both the query and
the response, byte for byte.

**What is deliberately NOT implemented**: probing and conflict resolution
(RFC 6762 section 8), known-answer suppression, a cache, and any of the
duplicate-question suppression. Those matter to a general-purpose responder
sharing a link with others; what is needed here is that one server can say where
it is and one endpoint can hear it. The static fallback is what covers every
case the omissions or the network make fail, and it is an assertion rather than
a nicety: whether multicast reaches a container and crosses this network's VLANs
is an open question, and an endpoint with no server address at all exits `7`
saying which of the two it lacked.

## What is NOT in this catalog

Authentication, authorisation, TLS, and anything that makes the control channel
reachable from outside a local network. The channel binds a configured address
and nothing in this phase changes that. The catalog has no message that grants
or checks a permission, and a version that adds one will be a new version.

Also absent, and belonging to later phases: telemetry from an endpoint, source
selection, presets, and anything about what is playing rather than where.
