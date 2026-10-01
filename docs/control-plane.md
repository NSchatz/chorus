# The chorus control plane

Version: catalog version **2** (goal 11: rooms, bonded sets, saved and live
groups, limits, quiet hours, alarms), a superset of catalog version **1** (the
PRODUCT-6 catalog). A build implements both. Every message is one JSON object,
UTF-8, with no byte order mark and no trailing newline on the wire.

This document and the vectors under `fixtures/control/` (catalog v1) and
`fixtures/control/v2/` (what catalog v2 adds) are the contract. A
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
`docs/decisions/0019-a-persisted-endpoint-that-never-comes-back.md`, and for
catalog v2 `docs/decisions/0075-control-catalog-v2.md` (the version policy, group
volume's definition and its citation, take-the-room, the clamp rule, the bond
layouts and the Wi-Fi refusal, state-file format 2).

## The versions

- **A command is written at the lowest catalog version that declares it.** Every
  v1 command (`hello`, `attach` without `link`, `name`, `group`, `ungroup`,
  `volume`, `mute`) is written `"v":1`, so every v1 vector is byte-identical
  under a v2 build and a v1 server still reads what a v2 client sends it.
  Everything below marked (v2) is written `"v":2`.
- **A v1 command is accepted at `"v":1` or `"v":2`.** A v2-only command, or a
  v2-only field (`attach`'s `link`), at `"v":1` is rejected exactly as an
  unknown command or an undeclared field is at that version: one message, the
  session stays open.
- **The state message is v2** (`"v":2`) on `GET /api/state`, on
  `GET /api/events` and in the answer to a `POST /api/command`.
  `GET /api/state?v=1` and `GET /api/events?v=1` serve the v1 shape of the same
  state, byte for byte what a v1 build served (`fixtures/control/state.json` is
  that renderer's vector). A v2 state's `zones` carry every field a v1 state's
  do, under the same names and in the same order, followed by v2's.
- **An `error` is written at the version of the message it answers**, and at
  `"v":1` where that version could not be read (the message is not JSON, or has
  no readable `v`): every peer reads v1. A `refused` is written at the build's
  own version and lists every version it implements, `[1,2]`.

## The state a server holds

One server owns all of it. There is no state anywhere else that a subscriber
could be reading instead, which is what makes "fan the resulting state out to
all subscribers" a complete description rather than half of a reconciliation
problem.

A **zone** is a room (catalog v2's documents say "room"; the wire keeps the key
`zone`). It has:

| field | what it is |
|---|---|
| `id` | the identifier, fixed at configuration and never changed by a message |
| `name` | the human-set name, which is what a person sees |
| `group` | the group whose stream this zone plays |
| `volume` | the amplitude factor its endpoints apply |
| `muted` | whether it is silenced |
| `endpoints` | every endpoint this zone has ever had, persisted |
| `present` | the subset attached right now, never persisted |
| `limit` (v2) | the room's maximum volume, `1.000` unless set |
| `quiet` (v2) | the room's quiet-hours windows, each with its own cap |
| `bond` (v2) | the room's bonded set: endpoints each playing a channel role, or none |

**The clamp rule (v2).** A room's **effective limit** is
`min(limit, the cap of every quiet-hours window active now)`. Every volume path
is CLAMPED to it, never refused: `volume`, `volume_step`, `group_volume`,
`group_volume_step`, a speaker's buttons (the controller role, which becomes a
`volume` command), and the server runtime's own ramps and steps. A volume above
the effective limit is set to it; lowering a limit, or a window becoming active,
pulls the volume (and a running ramp's target) down to it; a window ending
raises nothing. Mute is untouched by all of it.

**Which windows are active is an input.** The room model reads no clock: the
server's runtime tells it the civil time (a weekday and `HH:MM` in the house's
own time zone), and a window is active when that time is inside it. A window
whose `end` is not after its `start` runs past midnight, and its `days` are the
days it STARTS on: a Friday `22:00` to `07:00` window covers Saturday `03:00`.
`start` is inside, `end` is not; a window starting and ending at the same minute
is refused as ambiguous. With no time given, no window is active.

An **endpoint** (v2) has a **link**: `wired`, `wireless` or `unknown`. It is
what the endpoint says on `attach` (`link`); an endpoint that never said is
`unknown`. It is persisted. It is not the room's TRANSPORT (`--zone
<id>=wireless`, `docs/decisions/0024-the-wireless-tier.md`), which is configured,
never commanded, and is in the state as `transport`.

A **bonded set** (v2) is endpoints of ONE room each playing a channel role, a
channel position from `docs/protocol.md`'s channel map: `FL FR FC LFE BL BR SL
SR`. A set is a valid layout or it is refused: stereo `FL FR`; a sub (`LFE`) may
be added to any; a front three `FL FR FC`; a theater `FL FR FC`, optional
`LFE`, and one surround pair, `SL SR` or `BL BR` (never one of each, never
surrounds without the centre). One endpoint per role and one role per endpoint;
every member is an endpoint of that room and in no other room's set. **An
endpoint whose link is not `wired` (wireless OR unknown) is refused from any
bonded set (K91), and a room declared wireless cannot hold one**; the refusal
names the field and the endpoint. An endpoint in a set that later reports a link
other than `wired` has that `attach` refused (field `link`) until the set is
dissolved, so a set never holds one. A room with no set plays the stream's
channels as catalog v1 did.
A **group** is the unit a stream is served to. Every zone is in a group,
always: a zone in no group would be a zone with nothing to play. `ungroup`
therefore puts a zone into a group of its own, named for the zone, rather than
into an absent state the state message would have to spell `null`. One
consequence worth stating: `ungroup` on a zone that is already alone is not an
error and changes nothing.

A group is FORMED by the rooms whose `group` names it, and (v2) has a kind:

| kind | what it is |
|---|---|
| `room` | one room alone in the group named for it |
| `saved` | a group whose id is a saved definition |
| `live` | any other: rooms grouped and not saved |

A **saved group** (v2) is a persisted definition: an id no room has, a name, and
two or more rooms. It is listed always (K59), with `active` true when every one
of its rooms is in it. A **live group** is formed by `join` (with an id the
server assigns, `live-<n>`, the smallest `n` from 1 that names no room, no
formed group and no saved group) or by catalog v1's `group` (with the id the
client chose). A live group that `join` or `take` leaves with one room
**dissolves** into that room's own group, still playing what it played; v1's
`group` and `ungroup` keep v1's behaviour exactly and dissolve nothing.

Each formed group plays a **source** (v2): `stream` (the server's configured
stream, the default), `none`, `chime:<name>` or `line-in:<endpoint>/<input>`.
The spelling is the catalog's; which chimes exist and which inputs are offered
is the server runtime's, and the state lists the offered ones as `inputs`.

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

(v2) `attach` may carry `link`, what the endpoint says about how it reaches the
server; without it the endpoint's link is what was known before:

```json
{"v":2,"t":"attach","zone":"living","endpoint":"endpoint-a","link":"wired"}
```

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

`volume`, at either version, is clamped to the room's effective limit (above).

## The commands catalog version 2 adds

Each is written `"v":2`, and `fixtures/control/v2/` has a vector for each. A
**target** is an identifier naming a room or a group; which kinds of group a
command accepts is in its row. A **step** is signed thousandths of full scale,
`-1000` to `1000`, written as a whole number. A **time** is `"HH:MM"`, `00:00`
to `23:59`, two digits each. **Days** are an array of `mon tue wed thu fri sat
sun`, each once and in that order. Members are written in the order the tables
give them.

| command | fields | what it does |
|---|---|---|
| `join` | `zone`, `target` (a room, or a formed or saved group) | the room plays in the target's group; joining a room that is alone forms a live group with an assigned id. The group the room left dissolves if it is left with one room and is not saved |
| `bond` | `zone`, `members`: `[{"endpoint","role"}]` | the room's bonded set, replacing any it had; refused under the rules above |
| `unbond` | `zone` | dissolves the room's set; a room with none is not an error |
| `group_save` | `group`, `name`, `zones` (two or more rooms) | saves or replaces a definition. Its id may not be a room's |
| `group_delete` | `group` (saved) | forgets the definition; its rooms stay where they are |
| `take` | `target` (a room, a saved group or a formed group), optional `source` | take the room (K78), below |
| `group_volume` | `group` (formed), `volume` | Sonos-style group volume, below |
| `group_volume_step` | `group` (formed), `step` | the same, from the group volume plus `step`, held to `0.000` to `1.000` |
| `volume_step` | `zone`, `step` | the room's volume plus `step`, held to `0.000` to `1.000`, then clamped |
| `limit` | `zone`, `limit` (a volume) | the room's maximum; pulls its volume down to it |
| `quiet_hours` | `zone`, `windows`: `[{"days","start","end","limit"}]`, at most 8 | replaces the room's windows; `[]` removes them. Days are never empty |
| `alarm_set` | `alarm`, `target` (a room or a saved group), `time`, `days` (empty is once), `source`, `volume`, `ramp_s` (0 to 600), `duration_min` (0 to 720; 0 plays until stopped), `enabled` | creates or replaces an alarm |
| `alarm_delete` | `alarm` | forgets it |
| `alarm_stop` | `alarm` | stops it ringing; one not ringing is not an error |
| `sleep` | `target` (a room or a formed group), `minutes` (0 to 720) | asks for a sleep timer; `0` cancels |
| `autoplay` | `input` (`<endpoint>/<input>`), `target` (a room or a saved group), `enabled` | creates or replaces the rule for that input |

```json
{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"}]}
{"v":2,"t":"take","target":"downstairs","source":"line-in:endpoint-c/line-1"}
{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[{"days":["mon","tue","wed","thu","fri"],"start":"22:00","end":"07:00","limit":0.250}]}
```

An alarm, an autoplay rule and a saved group outlive a restart, so their
targets are rooms or saved groups; a live group's id names nothing once it
dissolves. A sleep timer does not outlive a restart (its countdown cannot be
resumed) and may name any group formed now; it goes when its group does. At
most 32 saved groups, 32 alarms and 32 autoplay rules are held (ASSUMED: a
bound on the state message's size, far past a house of a dozen rooms).

**This catalog configures alarms, sleep timers and autoplay; it does not run
them.** Firing an alarm, ramping it, counting a sleep timer down and fading it,
and starting a line-in are the server runtime's, which tells the room model
what it did through the model's hooks (`crates/control/src/zones.rs`, "What the
runtime drives"), and every volume it sets is clamped like any other. What the
server's runtime does is "The schedule runtime" below.

### Take the room (K78)

`take` moves every room of the target out of whatever group it is in and into
the target's group: for a room, the group named for it; for a saved group, its
id (which makes it active); for a formed group, itself. Rooms left behind keep
playing what they were playing. A group that is not saved and is left with one
room dissolves into that room's own group, still playing. A room that had joined
the target room's own group without being the target leaves it, keeping what it
played (alone into its own group, or with the others into a new live group).
With `source`, the target then plays that; without, it keeps what its group
played (`stream` for a group that was not formed).

### Group volume (K77, Sonos-style)

Sonos defines a group's volume as the **average** of its players' volumes, and
setting it "proportionally adjusts the volume of each player so that the average
corresponds to the desired group volume level"
(<https://docs.sonos.com/docs/volume>, read 2026-10-01). chorus does the same:

- the group volume `G` is the average of the member rooms' volumes, in
  thousandths, rounded half up;
- setting it to `G'` scales every room by `G'/G` (rounded half up), so the
  balance between rooms is kept, then clamps each room to its own effective
  limit. A clamped room's shortfall is NOT redistributed to the others, so the
  average can end below `G'`;
- from `G = 0` there is no balance left to keep, and every room is set to `G'`;
- `group_volume_step` is the same with `G' = G + step`, held to `0` to `1000`;
- each room stays individually adjustable with `volume` and `volume_step`.

Two differences from Sonos, both deliberate: Sonos's `setVolume` also unmutes
the group (<https://docs.sonos.com/reference/groupvolume-setvolume-groupid>,
read 2026-10-01) and chorus's mute is untouched by any volume command; and
Sonos scales from a snapshot of the volumes taken before a run of changes,
where chorus scales from the volumes as they stand.

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

That is the v1 shape, served on `?v=1`. **The v2 state** is the same message
with more fields, in this order (`fixtures/control/v2/state-rich.json` pins one
of each, `fixtures/control/v2/state-empty.json` the empty house):

| field | notes |
|---|---|
| `zones[]` | v1's fields, then `transport` (`wired` or `wireless`, as declared), `limit`, `effective_limit`, `quiet` (`[{"days","start","end","limit","active"}]`), `bond` (`[{"endpoint","role"}]`, `[]` for none) and `ramp` (a running ramp's target, or `null`) |
| `groups[]` | every formed group, in the order its first room was configured: `id`, `kind`, `zones`, `volume` (the group volume), `source`, `audio` |
| `saved_groups[]` | every saved definition, sorted by id, active or not: `id`, `name`, `zones`, `active` |
| `endpoints[]` | every endpoint any room has or that has reported a link, sorted: `id`, `link` |
| `alarms[]` | sorted by id: `alarm_set`'s fields from `alarm` on, then `ringing` |
| `sleep[]` | the sleep timers asked for, sorted by target: `target`, `minutes` |
| `autoplay[]` | sorted by input: `autoplay`'s fields |
| `inputs[]` | the line-ins offered now, as `<endpoint>/<input>`, sorted |

```json
{"v":2,"t":"state","serial":0,"zones":[],"groups":[],"saved_groups":[],"endpoints":[],"alarms":[],"sleep":[],"autoplay":[],"inputs":[]}
```

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
`implemented` is every version this build has. That vector
(`fixtures/control/refused-unknown-version.json`) is what a build implementing
only version 1 sent; this build sends
`fixtures/control/v2/refused-unknown-version.json`:

```json
{"v":2,"t":"refused","field":"v","detail":"catalog version 9 was offered and this build implements 1, 2; nothing from this peer has been applied","offered":9,"implemented":[1,2]}
```

Catalog v2's refusals are `error`s like v1's, each pinned under
`fixtures/control/v2/` (`refused-*` and `error-*`). The bond refusals name the
field and the endpoint:

```json
{"v":2,"t":"error","field":"members","detail":"endpoint 'endpoint-b' (FR) has a link that is wireless, not wired; a bonded set holds wired endpoints only (K91), because a stereo pair or a theater is held to the wired tier's bound and a radio is not"}
```

## How the messages travel

The catalog above is the contract. The transport is HTTP on the address
`--control-listen` names, and it is deliberately ordinary:

| route | what it does |
|---|---|
| `GET /` | the control page |
| `GET /chorus.css`, `GET /chorus.js` | what the page loads |
| `GET /api/state` | the state message, once (v2; `?v=1` for the v1 shape) |
| `GET /api/events` | a `text/event-stream`, one `data: <state message>` per change, starting with the state as it stands (v2; `?v=1` for the v1 shape, rendered from the state as it stands when each change reaches the stream) |
| `GET /api/report` | one line of plain text: commands applied, commands refused, connections and streams turned away, and the fanout's ceiling and drops |
| `POST /api/command` | the body is one control message, sent as `Content-Type: application/json`. `200` with the resulting state (v2, the bytes every subscriber is sent), `400` with an `error` (at the message's version), or `426` with a `refused`; `415` or `403` under the rules below |
| `POST /api/leaving` | the body is an endpoint identifier, which stops being `present`. The same two rules as a command |

A browser opens `/api/events` with `EventSource` and a shell script opens it
with a socket and a `GET` line, so **the UI and the verification scripts are the
same subscriber**. That is deliberate: a check that exercised a second, private
subscriber protocol would not be checking the thing the browser uses.

Every request is answered with `Connection: close`. A connection arriving when
every worker is busy is answered `503` naming the ceiling and closed, rather
than served by a thread nobody declared; see below.

### What a request may be

A request is at most 16 KiB, request line, headers and body together, and every
byte of it is read through that one bound, so a line that never ends is cut at
the bound rather than grown. A request line or header block past it is answered
`431`, a body that would take the request past it `413`, both unread. A request
has 5 seconds from the moment a worker picks it up to arrive in full, measured
on the monotonic clock; one still arriving then (a peer trickling a byte at a
time) is answered `408` and the worker goes back to the pool.

### What a command must carry

There is no authentication (below), so what keeps a web page served from
somewhere else from changing zones through a browser on this network is the
browser's own rules, and the two `POST` routes are held to what makes them
apply:

- A body not declared `Content-Type: application/json` (parameters such as
  `charset` allowed) is refused `415`. A page elsewhere can send `text/plain`
  with no preflight; it cannot send `application/json` without one, and this
  server approves no preflight.
- An `Origin` header, when present, has to name this server: its host and port
  have to be the request's `Host`. Any other `Origin`, `null` included, is
  refused `403`. A browser sends `Origin` on every `POST`, and the control page
  posts from this server's own origin; an endpoint or a script sends none and is
  not refused for that. The scheme is not compared, so a page served over `https`
  by a proxy in front of this listener is still its own origin.

Neither refusal changes any state, and both count as refused in
`GET /api/report`.

### Event streams and the event writer

An event stream costs no worker (audit finding B-5, closed in goal 11). The
worker that answers `GET /api/events` writes the headers and the opening state,
under its own write timeout, and hands the socket to the **event writer**, one
thread that writes every stream on non-blocking sockets; the worker goes back
to the pool at once. So a command is served however many endpoints and pages
are subscribed, even by a plane of one worker.

The writer holds at most `--event-streams` streams (default 64, ASSUMED: a
house of a few dozen endpoints and pages with room to spare, not a measured
bound); one asked for past that is answered `503`, `every one of this server's
K event streams is held`, and nothing is held for it. A peer that stops
draining its socket cannot hold the writer: its next state waits in its own
buffer (one message; the next is taken off its fanout queue only once that is
written), the fanout drops it at its queue's ceiling as below, and the writer
drops its stream once it has made no write progress for 5 s, counted as
`stalled_dropped` in `GET /api/report`. Every other stream is written on every
pass regardless (`crates/server/tests/control_stalled_peer.rs`).

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

With `--control-listen`, the server runs one control acceptor, one worker per
`--control-workers` slot, the event writer (above) and the **conductor**
(below), all created before the scheduling report is taken and none created
afterwards however many subscribers, sessions or groups come and go. The whole
process is `6 + 2N + M` threads, plus one for `--advertise`, and it does not
depend on `--slots`: every stream slot is cut by the one audio thread.

This is a safety property and not a style. `std::thread::spawn` inherits the
creating thread's scheduling policy, and `deploy/run-server.sh` runs the server
with `--ulimit rtprio=20`, so a control plane that spawned a thread per
subscriber would be putting real-time threads on a host that also runs other
things - and the report the host contract is graded on would never have seen
them. `crates/server/tests/control_thread_population.rs` grades it against
`/proc`, with `--slots 1` and `--slots 8` among its runs.

## Stream slots: every group served from one process (goal 11)

`--slots S` (at most 32, ASSUMED; needs `--control-listen`; not combined with
`--group-audio`) makes one server cut S streams at once, one per group that
plays, on its one audio thread and ONE grid: chunk `k` of every slot carries
the same sequence and presentation timestamp. Without it (`--slots 0`, the
default) the server is the one-stream shape it always was.

- **Routing is inside the session, on the one audio port.** The C endpoint has
  one server address and never reads this control plane, so a room joining a
  group is not "go and connect elsewhere": each session's queue is attached to
  the slot of the group its endpoint's room is in (the room whose `present`
  names it, else whose `endpoints` do) and moved between slots between two
  chunks. The shared grid is why a move needs no restart: the sequences stay
  contiguous, only the content changes. A session whose endpoint is in no room,
  or whose group has no slot, hears silence. Every group's `audio` in the state
  is the one listen address, so a Linux client never moves either.
- **A group needs a slot** when it is formed and its source is not `none`.
  Every change (a command, a button) is applied to a copy of the room model,
  the slots are planned over the copy (a group keeps its slot; a freed slot
  goes to a new group, lowest first), and only a plan that fits is installed.
  One that needs a slot more is refused `400`, field `target`: `every one of
  this server's S stream slots is in use (groups ...)`, naming the groups and
  `--slots S+1`, with nothing applied or persisted. At start, groups past the
  ceiling start with source `none` and the server says so
  (`slots group=<id> source=none reason=every-slot-in-use`). The deployment's
  answer is S = the number of rooms, which can never run out.
- **What a slot plays** follows its group's source: `stream` is the configured
  `--source` (read once per chunk however many slots play it); `chime:<name>`
  is that chime, rendered at start and repeated with a gap; `line-in:<endpoint>/
  <input>` is that endpoint's input (below, "The schedule runtime"); `none` is
  no slot at all. In this shape a source that ends (a file) is reopened, and no
  slot sends `stream_end`.
- **The conductor** is the thread that carries every change to the sessions it
  concerns: which slot each plays and each session hears, `room_volume` to the
  players of a room whose gain or effective limit changed (`docs/protocol.md`,
  "0x38 room volume"), and `controller_state` to its controllers when what they
  show changed, wherever the change was made. Each is deduped against what that
  session was last sent, and a session's greeting carries both before its first
  chunk.

## The schedule runtime: alarms, sleep timers, quiet hours, line-in autoplay

With `--control-listen` the conductor runs the schedule runtime
(`crates/server/src/schedule_runtime.rs`, ADR 0076) as part of every pass, and
this is what carries out the alarms, sleep timers, quiet hours and autoplay
rules the catalog configures (ADR 0079 wires it):

- **Civil time** is kept in one time zone, loaded at start before any thread:
  `--tz <path>` (a TZif file), else `$TZ` (a zoneinfo name, which must pass the
  schedule library's safe-name check and is read under `/usr/share/zoneinfo`,
  an absolute path, or a POSIX TZ string), else `/etc/localtime`, else UTC. The
  server says which (`civil tz=<what> source=<--tz|TZ|TZ-posix|localtime|
  default> clock=<system|fixed|from>`); a file it was told to read and cannot,
  or that is not a zone, stops it with exit 2. The civil clock is the host's
  wall clock, read by the conductor once per wake and nowhere else in the
  server; it decides only WHEN (an alarm's minute, which quiet window is
  active). Everything timed (ramps, fades, holds, sleep timers) is on the
  monotonic clock.
- **The tick.** The runtime is ticked at least once a second and whenever it
  has something due (a ramp step, an alarm's end, a sleep fade, an autoplay
  hold); every person's command applied (on the page, or an endpoint's
  `controller_command`) is handed to it after it commits, never one the
  runtime applied itself; and every line-in's offer and departure. What it
  changes goes through the room model like a command: planned onto the slots,
  persisted when it says so, fanned out. A change of its that would need a
  slot more than the server has is not refused (an alarm must not be lost):
  that group plays nothing (`schedule group=<id> source=none
  outcome=no-free-slot`).
- **Ramps** (an alarm's rise, its 2 s end fade, a sleep timer's 30 s fade) are
  stepped once a second: each step sets the room's volume in the state to
  where the ramp will be at the step's end and sends its players `room_volume`
  with that gain and `ramp_ms` the step's length, so an endpoint draws one
  straight line. A volume set any other way is sent at once (`ramp_ms` 0), and
  so is a limit or a quiet window pulling a room down, gain and limit in one
  message.
- **Alarms** ring at their civil minute (a minute a daylight-saving change
  skips rings at the first valid instant, a repeated one once), never twice
  for one instant whatever the wall clock does, and are skipped when more than
  60 s late (a server that was down). A ringing alarm takes its target (K78)
  with its source, sets its rooms to 0 and ramps them to its volume over
  `ramp_s`, clamped to each room's effective limit all the way; a source that
  cannot play (a line-in not offered, or played by another group) plays the
  `bell` chime instead. It ends after `duration_min`, on `alarm_stop` or
  `alarm_delete`, or when a person changes one of its rooms; then it fades
  out and the rooms go back to what they were (their group, volume, mute and
  source).
- **Sleep timers** fade the target to 0 over the last 30 s, then set its
  source to `none` and put the volumes back, silently, for next time. A person
  turning a fading room's volume cancels the timer.
- **Quiet hours** follow the civil clock: a window starting pulls a room's
  volume down to its cap (and holds an alarm's rise under it); a window ending
  raises nothing.
- **Line-in autoplay**: an input whose signal arrives with an enabled rule
  takes the rule's target and plays the input; when the signal goes it is held
  30 s (on top of the endpoint's own 2 s), then stopped and the target
  restored; the signal coming back within the hold cancels the stop.

Every default above is ASSUMED and listed in ADR 0076 and ADR 0079.

**Line-ins in the slots.** A line-in is the source role's input (ADR 0066) on
an endpoint's own session. The server starts it (`source_control` start, PCM,
in that session) while some group plays it and stops it when none does; the
input's `stream_format` must be the server's own format or it is stopped and
refused by name (`line-in refused reason=format-mismatch`). Its chunks go into
one of S fixed ports (at most S line-ins stream at once), each holding one
second; the audio thread plays a port through the latency-growth plan and the
cubic resampler (`crates/sync/src/latency_grow.rs`, ADR 0071) once it holds
two chunks. The chunks go out on the slots' ONE grid (the same sequence and
timestamp as every other slot), so moving into or out of a line-in's group is
no timestamp jump; what the plan moves is which source frames each chunk
carries. While the group is only the source endpoint's own room the line-in
plays at L_local (30 ms, ASSUMED); once another room joins, the plan grows the
offset to the group's latency (180 ms wired, the wireless tier's 500 ms) by a
bounded, smooth time stretch (500 ppm at most), with no frame dropped,
repeated or zero-inserted for the room already playing, and shrinks it back
the same way when the room is alone again (K94). A tick that finds too little
of the input plays silence and the plan waits (counted, `underruns=` in the
`line-in` lines). The latency is the stamp offset on the wire; today's
endpoints add their own playout latency on top of every stamp, so a line-in
is heard at L_local plus that until an endpoint mode for local line-ins
exists (ADR 0079's follow-ups).

**Test clocks.** `--civil-time <day>-<HH:MM>` holds the civil clock at that
weekday and time (of the week of Monday 2024-01-01, in the zone loaded);
`--civil-time-from <YYYY-MM-DDTHH:MM:SSZ>` runs it from that UTC instant; and
`--schedule-time-scale <n>` (1 to 60) runs the schedule's durations, and the
civil clock `--civil-time-from` runs, `n` times faster, dividing a ramp step's
`ramp_ms` by the same factor. The audio thread's pace is never scaled. The two
civil flags are not given together.

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

Also absent, and belonging to later phases: telemetry from an endpoint, presets,
and anything about what is playing rather than where. Catalog v2 names a
group's source and the inputs offered; choosing what a stream URL or a service
plays is not here.

## What survives a restart (state-file format 2)

The server persists, in `--state-file`, everything a person configured: each
room's name, group, volume, mute, endpoints, `limit`, quiet-hours windows and
bonded set; each endpoint's link; saved groups; alarms; autoplay rules. It does
not persist what is a fact about now: which endpoints are present, which quiet
window is active, which alarm is ringing, a running ramp, what a group is
playing, a sleep timer, or which inputs are offered. A format 1 file (every build
before catalog v2) loads unchanged with the v2 defaults; the next write is format
2. A write goes to a temporary that is `fsync`ed, renamed over the file, and the
directory is `fsync`ed, and a render that would not read back as the same state
is never installed (`crates/control/src/persist.rs`).
